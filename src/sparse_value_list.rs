//! Sorted descending codec-generic value list, allocation-free and `no_std`.
//!
//! Stores a set of distinct `u64` values as a sorted-descending list. Values live in a caller-owned
//! byte buffer; the codec is a type parameter drawn from `dsi_bitstream::dispatch`, so the codec
//! choice is picked at the call site. The first (largest) value is written absolutely, and every
//! subsequent value as the positive gap to its predecessor, minus one, so a gap of one costs a
//! single codeword for zero (in `Gamma` that is a single bit, in `Delta` it is two bits, etc).
//! Reading is a lazy iterator ([`ValueIter`]) that yields values in descending order without
//! allocating; insertion splices the new value into the bitstream in place, shifting the tail
//! toward higher bit indices.
//!
//! # Buffer alignment
//!
//! The caller-facing type is `&[u8]` but the codec operates internally on the buffer viewed as a
//! `&[u64]` word slice, so the byte buffer MUST be aligned to `align_of::<u64>()` and its length
//! MUST be a multiple of `size_of::<u64>()`. HyperLogLog's register arrays (backed by `[u64; N]`
//! internally) already satisfy both. `Vec<u8>` does not: use `Vec::<u64>::new()` and take a
//! `bytemuck::cast_slice` view, or construct the buffer as `Box<[u64]>` and cast.
//!
//! # Preamble
//!
//! Every entry point takes a `start_bit` parameter, so the caller can reserve a fixed preamble at
//! the front of the buffer (a stored element count, a mode flag, a signature) and hand the codec
//! the rest. The codec does not read or write anything below `start_bit`. Two thin helpers
//! ([`read_fixed_bits`] and [`write_fixed_bits`]) are provided to park a preamble in `[0,
//! start_bit)` without invoking a codec.
//!
//! # Codec
//!
//! The codec parameter is any type that implements `DynamicCodeRead + DynamicCodeWrite + CodeLen`
//! from `dsi_bitstream::dispatch`. In practice the zero-sized `ConstCode<{CODE}>` from the same
//! crate is the ergonomic choice: `ConstCode::<{ code_consts::GAMMA }>` for Elias gamma,
//! `ConstCode::<{ code_consts::DELTA }>` for Elias delta, and so on. Any user-defined `ConstCode`
//! that satisfies the trait bounds works too.

use core::marker::PhantomData;
pub use dsi_bitstream::dispatch::{
    code_consts, CodeLen, ConstCode, DynamicCodeRead, DynamicCodeWrite,
};
use dsi_bitstream::prelude::{
    BitRead, BitSeek, BitWrite, BufBitReader, BufBitWriter, CodesRead, CodesWrite, MemWordReader,
    MemWordWriterSlice,
};
pub use dsi_bitstream::traits::{Endianness, BE, LE};

/// Reinterprets a byte slice as a `u64` word slice, panicking if the slice's pointer is not
/// `u64`-aligned or its length is not a multiple of eight.
#[allow(unsafe_code)]
#[inline]
fn as_u64_slice(bytes: &[u8]) -> &[u64] {
    assert_eq!(
        (bytes.as_ptr() as usize) % core::mem::align_of::<u64>(),
        0,
        "byte buffer must be aligned to u64"
    );
    assert_eq!(
        bytes.len() % core::mem::size_of::<u64>(),
        0,
        "byte buffer length must be a multiple of 8"
    );
    // Safety: alignment and length are asserted above; `u8` -> `u64` reinterpret is well-defined
    // for the same lifetime because both slices are borrowed immutably from the caller.
    unsafe { core::slice::from_raw_parts(bytes.as_ptr().cast::<u64>(), bytes.len() / 8) }
}

/// Reads the bit at index `bit` from the buffer, interpreted as big-endian `u64` words with the
/// most significant bit first.
#[inline]
fn get_bit(buffer: &[u8], bit: u32) -> u64 {
    let byte = (bit / 8) as usize;
    let offset = 7 - (bit % 8) as u8;
    ((buffer[byte] >> offset) as u64) & 1
}

/// Sets the bit at index `bit` in the buffer to the low bit of `bit_value`.
#[inline]
fn set_bit(buffer: &mut [u8], bit: u32, bit_value: u64) {
    let byte = (bit / 8) as usize;
    let offset = 7 - (bit % 8) as u8;
    let mask = 1u8 << offset;
    if bit_value & 1 == 1 {
        buffer[byte] |= mask;
    } else {
        buffer[byte] &= !mask;
    }
}

/// Shifts the bit range `[from, end)` to higher indices by `delta` bits, in place, copying
/// backward so the moved bits never overwrite bits not yet read. Codec-agnostic.
#[inline]
fn shift_bits_right(buffer: &mut [u8], from: u32, end: u32, delta: u32) {
    for bit in (from..end).rev() {
        let value = get_bit(buffer, bit);
        set_bit(buffer, bit + delta, value);
    }
}

/// Panics on a `Result::Err` returned by a `dsi-bitstream` operation. Wraps the deny-listed
/// `.unwrap()` so callers stay clippy-clean; every dsi-bitstream error under our usage indicates a
/// bounds violation the caller SHOULD have prevented via `start_bit + code.len(v) <= 8 *
/// buffer.len()`.
#[inline]
fn expect_ok<T, E: core::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("dsi-bitstream operation failed: {e:?}"),
    }
}

/// Writes a single codeword for `value` starting at bit position `pos` in the byte buffer, using
/// `code` as the codec and `E` as the endianness. Returns the position past the last written bit.
///
/// Implementation: writes the codeword into a local `[u64; 2]` scratch buffer via a `BufBitWriter`
/// starting at bit 0, then copies the bits out to the destination via [`set_bit`]. The scratch is
/// 128 bits, enough for any codeword up to a 64-bit value under the shipped codes.
#[inline]
fn write_code_at<E, C>(buffer: &mut [u8], pos: u32, value: u64, code: &C) -> u32
where
    E: Endianness,
    C: DynamicCodeWrite + CodeLen,
    // The trait bound on the actual writer we construct below. Same trick as the reader below.
    for<'w> BufBitWriter<E, MemWordWriterSlice<u64, &'w mut [u64]>>: CodesWrite<E> + BitWrite<E>,
{
    // 4 words = 256 bits: enough for any u64-domain codeword. The largest we handle here is
    // `SigBitsCode` on `u64::MAX` (129 bits); gamma, delta, omega all fit well below that.
    let mut scratch = [0u64; 4];
    let len = {
        let mut writer: BufBitWriter<E, MemWordWriterSlice<u64, &mut [u64]>> =
            BufBitWriter::new(MemWordWriterSlice::new(scratch.as_mut_slice()));
        expect_ok(code.write(&mut writer, value))
    };
    let len = len as u32;
    // Copy `len` bits from `scratch` to `buffer` starting at `pos`. `scratch` was written in
    // endianness `E`, so we read it back the same way via a helper reader positioned at bit 0.
    // Direct bit-copying works because our destination uses the same `E`.
    for i in 0..len {
        let bit = get_bit_from_words::<E>(&scratch, i);
        set_bit(buffer, pos + i, bit);
    }
    pos + len
}

/// Reads bit `bit` from a `[u64; 2]` scratch buffer that the `BufBitWriter<E, ...>` wrote to. On
/// LE hosts, `MemWordWriterSlice::write_word(w.to_be())` stores the word in byte-reversed order,
/// so a native `u64` read of `scratch[0]` has the writer's MSB in the low byte. We route the read
/// through the byte view so the bit ordering matches [`get_bit`] on the destination buffer, which
/// is what we ultimately want to reproduce.
#[inline]
#[allow(unsafe_code)]
fn get_bit_from_words<E: Endianness>(words: &[u64; 4], bit: u32) -> u64 {
    let _ = core::marker::PhantomData::<E>;
    // Safety: `[u64; 4]` is 32 bytes, `u64`-aligned by construction, so a byte view is valid.
    let bytes: &[u8; 32] = unsafe { &*(words.as_ptr().cast::<[u8; 32]>()) };
    get_bit(&bytes[..], bit)
}

/// Reads a single codeword starting at bit position `pos` in the byte buffer, using `code` as the
/// codec and `E` as the endianness. Returns the decoded value and the position past the last read
/// bit.
#[inline]
fn read_code_at<E, C>(buffer: &[u8], pos: u32, code: &C) -> (u64, u32)
where
    E: Endianness,
    C: DynamicCodeRead,
    // Attaches the CodesRead bound to the concrete reader we build. We express this via a HRTB
    // over the borrow so the buffer lifetime does not leak into the bound.
    for<'r> BufBitReader<E, MemWordReader<u64, &'r [u64], true>>:
        CodesRead<E> + BitSeek + BitRead<E>,
{
    let words = as_u64_slice(buffer);
    let mut reader: BufBitReader<E, MemWordReader<u64, &[u64], true>> =
        BufBitReader::new(MemWordReader::new_inf(words));
    expect_ok(reader.set_bit_pos(u64::from(pos)));
    let value = expect_ok(code.read(&mut reader));
    let next_pos = expect_ok(reader.bit_pos()) as u32;
    (value, next_pos)
}

/// Reads a fixed-width big-endian bit field, MSB-first, from the buffer.
#[inline]
pub fn read_fixed_bits(buffer: &[u8], start_bit: u32, width: u32) -> u64 {
    debug_assert!((1..=64).contains(&width));
    let mut value = 0u64;
    for i in 0..width {
        value = (value << 1) | get_bit(buffer, start_bit + i);
    }
    value
}

/// Writes a fixed-width big-endian bit field, MSB-first, into the buffer.
#[inline]
pub fn write_fixed_bits(buffer: &mut [u8], start_bit: u32, width: u32, value: u64) {
    debug_assert!((1..=64).contains(&width));
    for i in 0..width {
        let bit = (value >> (width - 1 - i)) & 1;
        set_bit(buffer, start_bit + i, bit);
    }
}

/// Lazy iterator over the stored values, yielded in descending order, without allocating.
///
/// The iterator does not hold a live `BufBitReader`. It stores its cursor as a `u32` bit position
/// and constructs a fresh reader on every `next` call. `BufBitReader::new` and `set_bit_pos` are
/// both zero-allocation, so `next` remains allocation-free.
pub struct ValueIter<'a, E, C> {
    buffer: &'a [u8],
    pos: u32,
    remaining: u32,
    previous: u64,
    first: bool,
    code: C,
    _endianness: PhantomData<E>,
}

impl<'a, E: Endianness, C> ValueIter<'a, E, C>
where
    C: DynamicCodeRead + Copy,
    for<'r> BufBitReader<E, MemWordReader<u64, &'r [u64], true>>:
        CodesRead<E> + BitSeek + BitRead<E>,
{
    /// Constructs a new iterator over a codec starting at `start_bit` in `buffer` and containing
    /// exactly `count` sorted distinct values.
    #[inline]
    pub fn new(buffer: &'a [u8], start_bit: u32, count: u32, code: C) -> Self {
        Self {
            buffer,
            pos: start_bit,
            remaining: count,
            previous: 0,
            first: true,
            code,
            _endianness: PhantomData,
        }
    }
}

impl<E: Endianness, C> Iterator for ValueIter<'_, E, C>
where
    C: DynamicCodeRead + Copy,
    for<'r> BufBitReader<E, MemWordReader<u64, &'r [u64], true>>:
        CodesRead<E> + BitSeek + BitRead<E>,
{
    type Item = u64;

    #[inline]
    fn next(&mut self) -> Option<u64> {
        if self.remaining == 0 {
            return None;
        }
        let (code_value, next_pos) = read_code_at::<E, C>(self.buffer, self.pos, &self.code);
        self.pos = next_pos;
        self.previous = if self.first {
            self.first = false;
            code_value
        } else {
            // Gaps are stored raw; recover the value as previous minus gap.
            self.previous - code_value
        };
        self.remaining -= 1;
        Some(self.previous)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining as usize, Some(self.remaining as usize))
    }
}

impl<E: Endianness, C> ExactSizeIterator for ValueIter<'_, E, C>
where
    C: DynamicCodeRead + Copy,
    for<'r> BufBitReader<E, MemWordReader<u64, &'r [u64], true>>:
        CodesRead<E> + BitSeek + BitRead<E>,
{
}

/// Returns whether `value` is stored, without allocating, stopping early once the descending scan
/// passes `value`.
pub fn contains_value<E, C>(buffer: &[u8], start_bit: u32, count: u32, value: u64, code: C) -> bool
where
    E: Endianness,
    C: DynamicCodeRead + Copy,
    for<'r> BufBitReader<E, MemWordReader<u64, &'r [u64], true>>:
        CodesRead<E> + BitSeek + BitRead<E>,
{
    let mut pos = start_bit;
    let mut previous = 0u64;
    for index in 0..count {
        let (code_value, next_pos) = read_code_at::<E, C>(buffer, pos, &code);
        pos = next_pos;
        previous = if index == 0 {
            code_value
        } else {
            previous - code_value
        };
        if previous == value {
            return true;
        }
        if previous < value {
            return false;
        }
    }
    false
}

/// Returns the exact number of distinct values in the union of two exact lists, by a two-pointer
/// merge over the descending streams. Allocation-free.
pub fn union_count<E, C>(
    buffer_a: &[u8],
    start_a: u32,
    count_a: u32,
    buffer_b: &[u8],
    start_b: u32,
    count_b: u32,
    code: C,
) -> u32
where
    E: Endianness,
    C: DynamicCodeRead + Copy,
    for<'r> BufBitReader<E, MemWordReader<u64, &'r [u64], true>>:
        CodesRead<E> + BitSeek + BitRead<E>,
{
    let mut a = ValueIter::<E, C>::new(buffer_a, start_a, count_a, code).peekable();
    let mut b = ValueIter::<E, C>::new(buffer_b, start_b, count_b, code).peekable();
    let mut count = 0u32;
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (Some(x), Some(y)) => {
                count += 1;
                if x == y {
                    a.next();
                    b.next();
                } else if x > y {
                    a.next();
                } else {
                    b.next();
                }
            }
            (Some(_), None) => {
                a.next();
                count += 1;
            }
            (None, Some(_)) => {
                b.next();
                count += 1;
            }
            (None, None) => return count,
        }
    }
}

/// Calls `f(is_first, value)` for each distinct value of the union of two descending exact
/// streams, in descending order, by a two-pointer merge. Allocation-free.
pub fn for_each_union_value<E, C, F>(
    buffer_a: &[u8],
    start_a: u32,
    count_a: u32,
    buffer_b: &[u8],
    start_b: u32,
    count_b: u32,
    code: C,
    mut f: F,
) where
    E: Endianness,
    C: DynamicCodeRead + Copy,
    F: FnMut(bool, u64),
    for<'r> BufBitReader<E, MemWordReader<u64, &'r [u64], true>>:
        CodesRead<E> + BitSeek + BitRead<E>,
{
    let mut a = ValueIter::<E, C>::new(buffer_a, start_a, count_a, code).peekable();
    let mut b = ValueIter::<E, C>::new(buffer_b, start_b, count_b, code).peekable();
    let mut first = true;
    loop {
        let value = match (a.peek().copied(), b.peek().copied()) {
            (Some(x), Some(y)) => {
                if x == y {
                    a.next();
                    b.next();
                    x
                } else if x > y {
                    a.next();
                    x
                } else {
                    b.next();
                    y
                }
            }
            (Some(x), None) => {
                a.next();
                x
            }
            (None, Some(y)) => {
                b.next();
                y
            }
            (None, None) => return,
        };
        f(first, value);
        first = false;
    }
}

/// Returns `(distinct_count, encoded_bits)` of the union of two exact lists, without allocating or
/// writing. Use this to size the destination buffer before [`merge_write`]. `encoded_bits` is the
/// number of bits the codec would consume, exclusive of any destination preamble.
pub fn merge_metrics<E, C>(
    buffer_a: &[u8],
    start_a: u32,
    count_a: u32,
    buffer_b: &[u8],
    start_b: u32,
    count_b: u32,
    code: C,
) -> (u32, u32)
where
    E: Endianness,
    C: DynamicCodeRead + DynamicCodeWrite + CodeLen + Copy,
    for<'r> BufBitReader<E, MemWordReader<u64, &'r [u64], true>>:
        CodesRead<E> + BitSeek + BitRead<E>,
    for<'w> BufBitWriter<E, MemWordWriterSlice<u64, &'w mut [u64]>>: CodesWrite<E> + BitWrite<E>,
{
    let mut count = 0u32;
    let mut bits = 0u32;
    let mut previous = 0u64;
    for_each_union_value::<E, C, _>(
        buffer_a,
        start_a,
        count_a,
        buffer_b,
        start_b,
        count_b,
        code,
        |first, value| {
            bits += if first {
                code.len(value) as u32
            } else {
                code.len(previous - value) as u32
            };
            previous = value;
            count += 1;
        },
    );
    (count, bits)
}

/// Writes the union of two exact lists into `dest` as a fresh descending codec-encoded stream,
/// starting at `dest_start_bit`, returning the number of distinct values written. `dest` must hold
/// at least `dest_start_bit + merge_metrics(..).1` bits and be disjoint from both inputs.
pub fn merge_write<E, C>(
    buffer_a: &[u8],
    start_a: u32,
    count_a: u32,
    buffer_b: &[u8],
    start_b: u32,
    count_b: u32,
    dest: &mut [u8],
    dest_start_bit: u32,
    code: C,
) -> u32
where
    E: Endianness,
    C: DynamicCodeRead + DynamicCodeWrite + CodeLen + Copy,
    for<'r> BufBitReader<E, MemWordReader<u64, &'r [u64], true>>:
        CodesRead<E> + BitSeek + BitRead<E>,
    for<'w> BufBitWriter<E, MemWordWriterSlice<u64, &'w mut [u64]>>: CodesWrite<E> + BitWrite<E>,
{
    let mut count = 0u32;
    let mut pos = dest_start_bit;
    let mut previous = 0u64;
    for_each_union_value::<E, C, _>(
        buffer_a,
        start_a,
        count_a,
        buffer_b,
        start_b,
        count_b,
        code,
        |first, value| {
            let payload = if first { value } else { previous - value };
            pos = write_code_at::<E, C>(dest, pos, payload, &code);
            previous = value;
            count += 1;
        },
    );
    count
}

/// Result of inserting a value into the exact list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueInsertion {
    /// The value was inserted; the new count is `count + 1`.
    Inserted,
    /// The value was already present; the list is unchanged.
    Duplicate,
    /// The value does not fit in the current buffer; the caller must grow or transition.
    DoesNotFit,
}

/// Inserts `value` into the sorted (descending) exact list of `count` values stored in `buffer`
/// starting at `start_bit`, splicing it in place. Allocation-free.
pub fn insert_value<E, C>(
    buffer: &mut [u8],
    start_bit: u32,
    count: u32,
    value: u64,
    code: C,
) -> ValueInsertion
where
    E: Endianness,
    C: DynamicCodeRead + DynamicCodeWrite + CodeLen + Copy,
    for<'r> BufBitReader<E, MemWordReader<u64, &'r [u64], true>>:
        CodesRead<E> + BitSeek + BitRead<E>,
    for<'w> BufBitWriter<E, MemWordWriterSlice<u64, &'w mut [u64]>>: CodesWrite<E> + BitWrite<E>,
{
    let capacity_bits = (buffer.len() * 8) as u32;

    if count == 0 {
        if start_bit + code.len(value) as u32 > capacity_bits {
            return ValueInsertion::DoesNotFit;
        }
        write_code_at::<E, C>(buffer, start_bit, value, &code);
        return ValueInsertion::Inserted;
    }

    // Single descending pass: locate the insertion point, detect duplicates, and find the total
    // length. `before` is the predecessor (the smallest stored value still greater than `value`).
    let mut pos = start_bit;
    let mut previous = 0u64;
    let mut before: Option<u64> = None;
    let mut next: Option<(u64, u32, u32)> = None; // (value, code_start, code_end)
    for index in 0..count {
        let start = pos;
        let (code_value, end) = read_code_at::<E, C>(buffer, pos, &code);
        let current = if index == 0 {
            code_value
        } else {
            previous - code_value
        };
        pos = end;
        previous = current;
        if next.is_none() {
            if current == value {
                return ValueInsertion::Duplicate;
            }
            if current < value {
                next = Some((current, start, end));
            } else {
                before = Some(current);
            }
        }
    }
    let total_bits = pos;

    match next {
        None => {
            let predecessor = match before {
                Some(v) => v,
                None => panic!("a non-empty list always has a predecessor here"),
            };
            let gap = predecessor - value;
            let extra = code.len(gap) as u32;
            if total_bits + extra > capacity_bits {
                return ValueInsertion::DoesNotFit;
            }
            write_code_at::<E, C>(buffer, total_bits, gap, &code);
            ValueInsertion::Inserted
        }
        Some((next_value, split_bit, next_end)) => {
            let payload_a = match before {
                None => value,
                Some(prev) => prev - value,
            };
            let new_next_gap = value - next_value;
            let old_next_len = next_end - split_bit;
            let new_len = code.len(payload_a) as u32 + code.len(new_next_gap) as u32;
            debug_assert!(new_len > old_next_len);
            let delta = new_len - old_next_len;
            if total_bits + delta > capacity_bits {
                return ValueInsertion::DoesNotFit;
            }
            shift_bits_right(buffer, next_end, total_bits, delta);
            let after_a = write_code_at::<E, C>(buffer, split_bit, payload_a, &code);
            write_code_at::<E, C>(buffer, after_a, new_next_gap, &code);
            ValueInsertion::Inserted
        }
    }
}

/// The bundle of trait bounds a codec must satisfy to drive the sorted value list. Blanket
/// implemented for every type that meets the underlying `dsi-bitstream` bounds, so `SigBitsCode`
/// and any user codec that already implements `DynamicCodeRead + DynamicCodeWrite + CodeLen +
/// Copy + Default` picks it up automatically.
///
/// Serves purely as a name for the union: no member methods, no additional obligations. Downstream
/// crates can write `where C: SparseValueCodec` instead of enumerating the four traits by hand.
pub trait SparseValueCodec: DynamicCodeRead + DynamicCodeWrite + CodeLen + Copy + Default {}

impl<T> SparseValueCodec for T where T: DynamicCodeRead + DynamicCodeWrite + CodeLen + Copy + Default
{}

/// A code that encodes `v` as `unary(nbits) . binary(v, nbits)` where
/// `nbits = 64 - v.leading_zeros()` is the number of significant bits of `v` (0 for `v = 0`).
///
/// The codeword length is `2 * nbits + 1` bits, i.e. `1` bit for `v = 0` and `129` bits for
/// `v = u64::MAX`. This is approximately two bits wider than Elias gamma for typical values, and
/// the tradeoff is that this code natively represents the full `u64` range (Elias gamma internally
/// computes `n + 1` and thus cannot represent `u64::MAX`).
///
/// `SigBitsCode` is the historical encoding used by the `hyperloglog-rs` value list. Preserved
/// here so that the value-list layout stays bit-for-bit compatible when migrating callers to the
/// codec-generic [`sparse_value_list`](crate::sparse_value_list) surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SigBitsCode;

impl DynamicCodeRead for SigBitsCode {
    #[inline]
    fn read<E: Endianness, CR: CodesRead<E> + ?Sized>(
        &self,
        reader: &mut CR,
    ) -> Result<u64, CR::Error> {
        let nbits = reader.read_unary()?;
        if nbits == 0 {
            Ok(0)
        } else {
            reader.read_bits(nbits as usize)
        }
    }
}

impl DynamicCodeWrite for SigBitsCode {
    #[inline]
    fn write<E: Endianness, CW: CodesWrite<E> + ?Sized>(
        &self,
        writer: &mut CW,
        value: u64,
    ) -> Result<usize, CW::Error> {
        let nbits = u64::from(64 - value.leading_zeros());
        let unary_len = writer.write_unary(nbits)?;
        if nbits == 0 {
            Ok(unary_len)
        } else {
            let bits_len = writer.write_bits(value, nbits as usize)?;
            Ok(unary_len + bits_len)
        }
    }
}

impl CodeLen for SigBitsCode {
    #[inline]
    fn len(&self, value: u64) -> usize {
        let nbits = (64 - value.leading_zeros()) as usize;
        2 * nbits + 1
    }
}

// (Convenience re-exports are declared at the top of this module.)
#[cfg(all(test, feature = "alloc"))]
mod tests {
    use super::*;
    use crate::iter_random_values;
    use alloc::vec;
    use alloc::vec::Vec;
    use dsi_bitstream::dispatch::code_consts;

    /// A u64-aligned buffer of `bytes` zero bytes, obtained by allocating a `Vec<u64>` and taking a
    /// byte view. `alloc`'s allocator guarantees `u64` alignment for `Vec<u64>`.
    fn aligned_buffer(bytes: usize) -> Vec<u64> {
        assert_eq!(bytes % 8, 0, "aligned_buffer only accepts multiples of 8");
        vec![0u64; bytes / 8]
    }

    #[allow(unsafe_code)]
    fn as_bytes_mut(words: &mut [u64]) -> &mut [u8] {
        // Safety: u64 to u8 downcast is always well-aligned; we own the exclusive borrow.
        unsafe { core::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), words.len() * 8) }
    }

    #[allow(unsafe_code)]
    fn as_bytes(words: &[u64]) -> &[u8] {
        // Safety: same as above, immutable.
        unsafe { core::slice::from_raw_parts(words.as_ptr().cast::<u8>(), words.len() * 8) }
    }

    /// Inserts an ascending value set and checks lazy recovery (descending), membership, and the
    /// reported insert outcomes.
    fn check_inserts<C>(values: &[u64], words: usize, code: C)
    where
        C: DynamicCodeRead + DynamicCodeWrite + CodeLen + Copy,
        for<'r> BufBitReader<BE, MemWordReader<u64, &'r [u64], true>>:
            CodesRead<BE> + BitSeek + BitRead<BE>,
        for<'w> BufBitWriter<BE, MemWordWriterSlice<u64, &'w mut [u64]>>:
            CodesWrite<BE> + BitWrite<BE>,
    {
        let mut backing = aligned_buffer(words * 8);
        let mut count = 0u32;
        let mut expected: Vec<u64> = Vec::new();
        for &value in values {
            let outcome = insert_value::<BE, C>(as_bytes_mut(&mut backing), 0, count, value, code);
            if expected.contains(&value) {
                assert_eq!(outcome, ValueInsertion::Duplicate, "value {value}");
            } else {
                assert_eq!(outcome, ValueInsertion::Inserted, "value {value}");
                expected.push(value);
                count += 1;
            }
        }
        expected.sort_unstable();

        let buffer = as_bytes(&backing);
        let mut recovered: Vec<u64> = ValueIter::<BE, C>::new(buffer, 0, count, code).collect();
        assert_eq!(recovered.len(), count as usize);
        // ValueIter yields descending.
        let mut descending = expected.clone();
        descending.reverse();
        assert_eq!(recovered, descending, "recovery mismatch");

        recovered.sort_unstable();
        assert_eq!(recovered, expected);

        for &value in &expected {
            assert!(
                contains_value::<BE, C>(buffer, 0, count, value, code),
                "missing {value}"
            );
        }
    }

    #[test]
    fn test_insert_edge_cases_gamma() {
        let g = ConstCode::<{ code_consts::GAMMA }>;
        check_inserts(&[], 1, g);
        check_inserts(&[0], 1, g);
        // Standard Elias gamma internally computes n+1, so it cannot represent u64::MAX.
        // The largest gamma-encodable value is u64::MAX - 1.
        check_inserts(&[u64::MAX - 1], 4, g);
        check_inserts(&[0, u64::MAX - 1], 8, g);
        check_inserts(&[u64::MAX - 1, 0], 8, g);
        check_inserts(&[3, 2, 1, 0], 2, g);
        check_inserts(&[0, 1, 2, 3], 2, g);
        check_inserts(&[5, 1, 9, 1, 3], 4, g);
        check_inserts(&[1_000_000, 12, 11, 10], 4, g);
    }

    #[test]
    fn test_insert_random_order_gamma() {
        let g = ConstCode::<{ code_consts::GAMMA }>;
        for seed in 0..32u64 {
            let values: Vec<u64> =
                iter_random_values::<u64>(150, Some(1 << 40), Some(seed)).collect();
            check_inserts(&values, 512, g);
        }
    }

    /// Same insert/contains/union sequence under a different codec, to prove the layer is not
    /// gamma-hardcoded anywhere.
    #[test]
    fn test_insert_edge_cases_delta() {
        let d = ConstCode::<{ code_consts::DELTA }>;
        check_inserts(&[], 1, d);
        check_inserts(&[3, 2, 1, 0], 2, d);
        check_inserts(&[5, 1, 9, 1, 3], 4, d);
        check_inserts(&[1_000_000, 12, 11, 10], 4, d);
    }

    /// `SigBitsCode` handles the full u64 range including `u64::MAX`, which real Elias gamma
    /// cannot. This test asserts round-trip on the extreme values, plus the same generic
    /// edge-case sweep as gamma.
    #[test]
    fn test_insert_edge_cases_sig_bits() {
        let s = SigBitsCode;
        check_inserts(&[], 1, s);
        check_inserts(&[0], 1, s);
        check_inserts(&[u64::MAX], 4, s);
        check_inserts(&[0, u64::MAX], 8, s);
        check_inserts(&[u64::MAX, 0], 8, s);
        check_inserts(&[3, 2, 1, 0], 2, s);
        check_inserts(&[0, 1, 2, 3], 2, s);
        check_inserts(&[5, 1, 9, 1, 3], 4, s);
        check_inserts(&[1_000_000, 12, 11, 10], 4, s);
    }

    #[test]
    fn test_contains_absent_gamma() {
        let g = ConstCode::<{ code_consts::GAMMA }>;
        let mut backing = aligned_buffer(8 * 8);
        let mut count = 0u32;
        for value in [10u64, 20, 30, 40] {
            if insert_value::<BE, _>(as_bytes_mut(&mut backing), 0, count, value, g)
                == ValueInsertion::Inserted
            {
                count += 1;
            }
        }
        let buffer = as_bytes(&backing);
        assert!(!contains_value::<BE, _>(buffer, 0, count, 25, g));
        assert!(!contains_value::<BE, _>(buffer, 0, count, 5, g));
        assert!(!contains_value::<BE, _>(buffer, 0, count, 50, g));
    }

    #[test]
    fn test_saturation_reported_not_panic() {
        let g = ConstCode::<{ code_consts::GAMMA }>;
        let mut backing = aligned_buffer(8);
        let mut count = 0u32;
        let mut saturated = false;
        for value in iter_random_values::<u64>(64, None, Some(7)) {
            match insert_value::<BE, _>(as_bytes_mut(&mut backing), 0, count, value, g) {
                ValueInsertion::Inserted => count += 1,
                ValueInsertion::Duplicate => {}
                ValueInsertion::DoesNotFit => {
                    saturated = true;
                    break;
                }
            }
        }
        assert!(saturated, "the tiny buffer must saturate");
    }

    #[test]
    fn test_union_and_merge_gamma() {
        let g = ConstCode::<{ code_consts::GAMMA }>;
        let mut a = aligned_buffer(8 * 8);
        let mut b = aligned_buffer(8 * 8);
        let mut count_a = 0u32;
        let mut count_b = 0u32;
        for value in [10u64, 20, 30, 40] {
            if insert_value::<BE, _>(as_bytes_mut(&mut a), 0, count_a, value, g)
                == ValueInsertion::Inserted
            {
                count_a += 1;
            }
        }
        for value in [25u64, 35, 40, 50, 60] {
            if insert_value::<BE, _>(as_bytes_mut(&mut b), 0, count_b, value, g)
                == ValueInsertion::Inserted
            {
                count_b += 1;
            }
        }

        let ab = as_bytes(&a);
        let bb = as_bytes(&b);
        assert_eq!(union_count::<BE, _>(ab, 0, count_a, bb, 0, count_b, g), 8);
        assert_eq!(union_count::<BE, _>(bb, 0, count_b, ab, 0, count_a, g), 8);

        let (n, needed) = merge_metrics::<BE, _>(ab, 0, count_a, bb, 0, count_b, g);
        assert_eq!(n, 8);

        let mut dest = aligned_buffer(8 * 8);
        let written = merge_write::<BE, _>(
            ab,
            0,
            count_a,
            bb,
            0,
            count_b,
            as_bytes_mut(&mut dest),
            0,
            g,
        );
        assert_eq!(written, 8);

        let db = as_bytes(&dest);
        let recovered: Vec<u64> = ValueIter::<BE, _>::new(db, 0, written, g).collect();
        assert_eq!(recovered, vec![60, 50, 40, 35, 30, 25, 20, 10]);
        assert!(needed > 0);
    }

    #[test]
    fn test_start_bit_reserves_preamble() {
        // The codec must not touch bits below `start_bit`, and it must decode correctly when read
        // back with the same `start_bit`.
        let g = ConstCode::<{ code_consts::GAMMA }>;
        const PREAMBLE_BITS: u32 = 24;
        const MAGIC: u64 = 0xABCDEF;

        let mut backing = aligned_buffer(32 * 8);
        write_fixed_bits(as_bytes_mut(&mut backing), 0, PREAMBLE_BITS, MAGIC);

        let values = [10u64, 5, 20, 15, 30, 25];
        let mut count = 0u32;
        for &value in &values {
            let outcome =
                insert_value::<BE, _>(as_bytes_mut(&mut backing), PREAMBLE_BITS, count, value, g);
            assert_eq!(outcome, ValueInsertion::Inserted, "value {value}");
            count += 1;
        }

        let buffer = as_bytes(&backing);
        assert_eq!(read_fixed_bits(buffer, 0, PREAMBLE_BITS), MAGIC);
        let recovered: Vec<u64> =
            ValueIter::<BE, _>::new(buffer, PREAMBLE_BITS, count, g).collect();
        assert_eq!(recovered, vec![30, 25, 20, 15, 10, 5]);
        for &value in &values {
            assert!(contains_value::<BE, _>(
                buffer,
                PREAMBLE_BITS,
                count,
                value,
                g
            ));
        }
    }
}
