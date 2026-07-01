//! Packed array for registers.
//!
//! The principal difference between this implementation and the one in either a plain array or
//! a vector is that this implementation uses a packed array to store the registers. This means
//! that while in the other implementations we store as many registers as they fit in a word and we
//! discard the padding bits (e.g. when using a 64-bit word and a 6-bit register, we store 10 registers and
//! discard 4 bits), in this implementation we store the registers in a packed array, so we don't discard
//! any bits. This will tendentially make the packed array implementation more memory efficient, but
//! it will also make it slower, as we need to perform more operations to extract the registers from the
//! packed array, especially in the case of bridge registers, i.e. registers that span two words.

use crate::{Matrix, VariableWord, Zero};
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::size_of;

#[allow(unsafe_code)]
#[inline]
/// Extracts the register from one or more words at the given offset.
///
/// # Arguments
/// * `word` - The word array from which the register is to be extracted.
/// * `offset` - The offset (from the right) at which the register starts.
///
/// # Implementation details
/// We store the values starting from the left-side of the word, so the offset is the number of bits
/// from the right side of the word at which the register starts. We then shift the word to the right
/// by the offset and apply a mask to extract the register.
///
/// # Safety
/// This method uses an unsafe conversion from `u64` to `V::Word`, as we do not check
/// whether the value extracted from the word is a valid value for the register type.
/// This is okay because we apply a mask to the value, and it is not possible for the
/// value we cast to be greater than the mask.
pub fn extract_value_from_word<V: VariableWord>(word: u64, offset: u8) -> V::Word {
    debug_assert!(
        offset + V::NUMBER_OF_BITS <= 64,
        "The offset ({offset} + {}) should be less than or equal to 64",
        V::NUMBER_OF_BITS,
    );
    unsafe { V::unchecked_from_u64((word >> (64 - V::NUMBER_OF_BITS - offset)) & V::MASK) }
}

#[inline]
/// We insert the value into the word at the given offset.
///
/// # Arguments
/// * `word` - The word in which the value is to be inserted.
/// * `offset` - The offset (from the right) at which the value is to be inserted.
/// * `value` - The value to be inserted.
pub fn insert_value_into_word<V: VariableWord>(word: &mut u64, offset: u8, value: u64) {
    debug_assert!(
        offset + V::NUMBER_OF_BITS <= 64,
        "The offset ({offset} + {}) should be less than or equal to 64",
        V::NUMBER_OF_BITS,
    );

    let flipped_offset = 64 - V::NUMBER_OF_BITS - offset;
    *word &= !(V::MASK << flipped_offset);
    *word |= value << flipped_offset;
}

#[inline]
#[allow(unsafe_code)]
/// Returns the number of bits in the upper and lower value of a bridge value.
///
/// # Arguments
/// * `lower_word` - The lower word of the bridge value.
/// * `upper_word` - The upper word of the bridge value.
/// * `offset` - The offset (from the right) of the bridge value.
///
/// # Safety
/// * The method converts in an unchecked manner the value from a `u64` to a `V::Word`.
pub fn extract_bridge_value_from_word<V: VariableWord>(
    lower_word: u64,
    upper_word: u64,
    offset: u8,
) -> V::Word {
    debug_assert!(offset != 0, "Offset should be greater than 0");
    debug_assert!(offset != 64, "Offset should be less than 64");
    debug_assert!(
        offset > 64 - V::NUMBER_OF_BITS,
        "Offset should be greater than 64 - V::NUMBER_OF_BITS"
    );

    let number_of_high_bits_in_lower_value: u8 = 64 - offset;
    let number_of_low_bits_in_upper_value = V::NUMBER_OF_BITS - number_of_high_bits_in_lower_value;
    let higher_bits_mask = V::MASK >> number_of_low_bits_in_upper_value;

    let higher_bits = (lower_word & higher_bits_mask) << number_of_low_bits_in_upper_value;
    let lower_bits = upper_word >> (64 - number_of_low_bits_in_upper_value);

    let word = higher_bits | lower_bits;

    unsafe { V::unchecked_from_u64(word) }
}

/// Inserts a bridge value into a pair of adjacent words at the given bit offset.
///
/// # Arguments
/// * `lower_word` - The lower word (holds the high bits of the packed value).
/// * `upper_word` - The upper word (holds the low bits of the packed value).
/// * `offset` - The offset (from the right) at which the value starts in `lower_word`. The value
///   crosses the word boundary, so `offset + V::NUMBER_OF_BITS > 64`.
/// * `value` - The value to insert, masked into the two words.
#[inline]
pub fn insert_bridge_value_into_word<V: VariableWord>(
    lower_word: &mut u64,
    upper_word: &mut u64,
    offset: u8,
    value: u64,
) {
    debug_assert!(
        offset + V::NUMBER_OF_BITS > 64,
        "Offset + bits ({} + {}) should be greater than {}",
        offset,
        V::NUMBER_OF_BITS,
        64
    );

    debug_assert!(offset < 64, "Offset {} should be less than {}", offset, 64);

    let number_of_lower_bits = V::NUMBER_OF_BITS + offset - 64;
    let lower_bits_mask = (1 << number_of_lower_bits) - 1;
    let higher_bits_mask = V::MASK >> number_of_lower_bits;
    let lower_bits = value & lower_bits_mask;
    let higher_bits = value >> number_of_lower_bits;

    // First, we clear the bits that will be replaced by the new value.
    *lower_word &= !higher_bits_mask;
    // Then, we insert the lower part of the new value.
    *lower_word |= higher_bits;
    // We do the same for the upper part of the new value.
    *upper_word &= !(lower_bits_mask << (64 - number_of_lower_bits));
    *upper_word |= lower_bits << (64 - number_of_lower_bits);
}

const LOG2_USIZE: usize = (usize::BITS as usize).trailing_zeros() as usize;

#[must_use]
/// Extracts registers from multiple words at the same offset.
#[inline]
pub fn extract_value_from_words<V: VariableWord, const N: usize>(
    words: [u64; N],
    offset: u8,
) -> [V::Word; N] {
    let mut values = [V::Word::ZERO; N];
    for i in 0..N {
        values[i] = extract_value_from_word::<V>(words[i], offset);
    }
    values
}

#[must_use]
/// Extracts bridge registers from pairs of words at the same offset.
#[inline]
pub fn extract_bridge_value_from_words<V: VariableWord, const N: usize>(
    lower_word: [u64; N],
    upper_word: [u64; N],
    offset: u8,
) -> [V::Word; N] {
    let mut values = [V::Word::ZERO; N];
    for i in 0..N {
        values[i] = extract_bridge_value_from_word::<V>(lower_word[i], upper_word[i], offset);
    }
    values
}

#[must_use]
#[allow(clippy::cast_possible_truncation)]
/// Extracts the word position and the relative register offset from the packed index.
#[inline]
pub const fn split_packed_index<V: VariableWord>(index: usize) -> (usize, u8) {
    let absolute_register_offset: usize = V::NUMBER_OF_BITS_USIZE * index;
    let word_index: usize = absolute_register_offset >> LOG2_USIZE;
    let relative_register_offset = (absolute_register_offset - word_index * 64) as u8;
    (word_index, relative_register_offset)
}

/// Packed register array storage.
///
/// Stores `V::NUMBER_OF_BITS` bits per register in a contiguous array of `u64` words,
/// with no wasted padding bits. Registers that span word boundaries (bridge registers)
/// are handled transparently by [`get`] and [`set`].
///
/// [`get`]: Packed::get
/// [`set`]: Packed::set
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Packed<W, V> {
    /// The packed array of registers.
    words: W,
    /// Phantom data to keep track of the variable word type.
    _phantom: PhantomData<V>,
}

impl<W, V> Packed<W, V> {
    /// Creates a new packed register array from the given word storage.
    #[inline]
    pub fn new(words: W) -> Self {
        Self {
            words,
            _phantom: PhantomData,
        }
    }

    /// Returns the underlying word storage.
    #[inline]
    pub fn into_words(self) -> W {
        self.words
    }
}

impl<W: AsRef<[u64]>, V> AsRef<[u64]> for Packed<W, V> {
    #[inline]
    fn as_ref(&self) -> &[u64] {
        self.words.as_ref()
    }
}

impl<W: AsRef<[u64]>, V> AsRef<[u8]> for Packed<W, V> {
    #[inline]
    #[allow(unsafe_code)]
    fn as_ref(&self) -> &[u8] {
        let words_u64: &[u64] = self.words.as_ref();
        unsafe { core::slice::from_raw_parts(words_u64.as_ptr().cast::<u8>(), words_u64.len() * 8) }
    }
}

impl<W: AsMut<[u64]>, V> AsMut<[u8]> for Packed<W, V> {
    #[inline]
    #[allow(unsafe_code)]
    fn as_mut(&mut self) -> &mut [u8] {
        let words_u64: &mut [u64] = self.words.as_mut();
        let slice = unsafe {
            core::slice::from_raw_parts_mut(
                words_u64.as_mut_ptr().cast::<u8>(),
                words_u64.len() * 8,
            )
        };
        debug_assert_eq!(slice.len() % size_of::<u64>(), 0);
        slice
    }
}
impl<W: AsMut<[u64]>, V> AsMut<[u64]> for Packed<W, V> {
    #[inline]
    fn as_mut(&mut self) -> &mut [u64] {
        self.words.as_mut()
    }
}

impl<W: AsRef<[u64]> + AsMut<[u64]>, V: VariableWord> Packed<W, V> {
    #[inline]
    #[must_use]
    /// Returns whether a given offset is a bridge offset.
    pub const fn is_bridge_offset(offset: u8) -> bool {
        (V::NUMBER_OF_BITS_USIZE * V::NUMBER_OF_ENTRIES < 64) && (offset + V::NUMBER_OF_BITS > 64)
    }

    #[inline]
    /// Returns the value stored at the given index.
    pub fn get(&self, index: usize) -> V::Word {
        // We determine the word which contains the value and the position of the value,
        // taking into account the bridge values.
        let (word_index, relative_value_offset) = split_packed_index::<V>(index);

        debug_assert!(
            word_index < self.words.as_ref().len(),
            "The word index {} (started out as {}) should be less than {} (the number of words) in an object of type {}",
            word_index,
            index,
            self.words.as_ref().len(),
            core::any::type_name::<Self>()
        );

        // Now we determine whether the value is a bridge value or not, i.e. if it spans
        // two words.
        if Self::is_bridge_offset(relative_value_offset) {
            extract_bridge_value_from_word::<V>(
                self.words.as_ref()[word_index],
                self.words.as_ref()[word_index + 1],
                relative_value_offset,
            )
        } else {
            extract_value_from_word::<V>(self.words.as_ref()[word_index], relative_value_offset)
        }
    }

    #[inline]
    /// Set the value at the given index.
    ///
    /// # Arguments
    /// * `index` - The index at which the value is to be set.
    /// * `value` - The value to be set.
    pub fn set(&mut self, index: usize, value: V::Word) {
        let (word_index, relative_value_offset) = split_packed_index::<V>(index);

        if Self::is_bridge_offset(relative_value_offset) {
            let (low, high) = self.words.as_mut().split_at_mut(word_index + 1);
            let low = &mut low[word_index];
            let high = &mut high[0];
            insert_bridge_value_into_word::<V>(low, high, relative_value_offset, value.into());
        } else {
            insert_value_into_word::<V>(
                &mut self.words.as_mut()[word_index],
                relative_value_offset,
                value.into(),
            );
        }
    }
}

impl<const N: usize, V: VariableWord> Default for Packed<[u64; N], V> {
    #[inline]
    fn default() -> Self {
        Self {
            words: [0; N],
            _phantom: PhantomData,
        }
    }
}

#[cfg(feature = "alloc")]
impl<V: VariableWord> Default for Packed<Vec<u64>, V> {
    #[inline]
    fn default() -> Self {
        Self {
            words: Vec::new(),
            _phantom: PhantomData,
        }
    }
}

/// Iterator over the registers of packed arrays.
#[derive(Debug)]
pub struct PackedIter<A, const M: usize> {
    /// Number of values to be iterated in total.
    total_values: usize,
    /// The current register being processed.
    value_index: usize,
    /// The current column of the matrix being processed.
    word_index: usize,
    /// The offset in bits of the current word.
    word_offset: u8,
    /// The arrays being processed.
    arrays: [A; M],
    /// The current n-uple of words being processed.
    column: [u64; M],
}

impl<A, const M: usize> PackedIter<A, M> {
    #[inline]
    /// Creates a new instance of the register tuple iterator.
    pub fn new(arrays: [A; M], total_values: usize) -> Self
    where
        [A; M]: Matrix<u64, M>,
    {
        Self {
            total_values,
            value_index: 0,
            word_offset: 0,
            word_index: 0,
            column: arrays.column(0),
            arrays,
        }
    }
}

impl<W: Debug + AsRef<[u64]> + AsMut<[u64]>, V: VariableWord> Iterator
    for PackedIter<&Packed<W, V>, 2>
{
    type Item = [V::Word; 2];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.total_values == self.value_index {
            return None;
        }

        self.value_index += 1;

        Some(if <Packed<W, V>>::is_bridge_offset(self.word_offset) {
            let current_column = self.column;
            self.word_index += 1;
            self.column = self.arrays.column(self.word_index);
            let values = extract_bridge_value_from_words::<V, 2>(
                current_column,
                self.column,
                self.word_offset,
            );

            self.word_offset = V::NUMBER_OF_BITS - (64 - self.word_offset);
            values
        } else {
            let values = extract_value_from_words::<V, 2>(self.column, self.word_offset);
            self.word_offset += V::NUMBER_OF_BITS;
            if self.value_index < self.total_values && self.word_offset == 64 {
                self.word_offset = 0;
                self.word_index += 1;
                self.column = self.arrays.column(self.word_index);
            }
            values
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining_values = self.total_values - self.value_index;
        (remaining_values, Some(remaining_values))
    }
}

impl<W: Debug + AsRef<[u64]> + AsMut<[u64]>, V: VariableWord> Iterator
    for PackedIter<&Packed<W, V>, 1>
{
    type Item = V::Word;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.total_values == self.value_index {
            return None;
        }

        self.value_index += 1;

        Some(if <Packed<W, V>>::is_bridge_offset(self.word_offset) {
            let current_column = self.column;
            self.word_index += 1;
            self.column = self.arrays.column(self.word_index);
            let [value] = extract_bridge_value_from_words::<V, 1>(
                current_column,
                self.column,
                self.word_offset,
            );

            self.word_offset = V::NUMBER_OF_BITS - (64 - self.word_offset);
            value
        } else {
            let [value] = extract_value_from_words::<V, 1>(self.column, self.word_offset);
            self.word_offset += V::NUMBER_OF_BITS;
            if self.value_index < self.total_values && self.word_offset == 64 {
                self.word_offset = 0;
                self.word_index += 1;
                self.column = self.arrays.column(self.word_index);
            }
            value
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining_values = self.total_values - self.value_index;
        (remaining_values, Some(remaining_values))
    }
}

impl<W: AsRef<[u64]>, const M: usize, V: VariableWord> ExactSizeIterator
    for PackedIter<&Packed<W, V>, M>
where
    Self: Iterator,
{
}

/// Iterator and bulk-operation helpers for [`Packed`].
impl<W: Debug + AsRef<[u64]>, V: VariableWord> Packed<W, V> {
    #[inline]
    /// Returns an iterator over `len` values in the packed array.
    pub fn iter_values(&self, len: usize) -> PackedIter<&Self, 1> {
        PackedIter::new([self], len)
    }

    #[inline]
    /// Returns an iterator over `len` value pairs from two packed arrays.
    pub fn iter_values_zipped<'words>(
        &'words self,
        other: &'words Self,
        len: usize,
    ) -> PackedIter<&'words Self, 2> {
        PackedIter::new([self, other], len)
    }
}

/// Bulk mutation helpers for [`Packed`].
impl<W: AsMut<[u64]>, V: VariableWord> Packed<W, V> {
    /// Clears all registers to zero.
    #[inline]
    pub fn clear(&mut self) {
        self.words.as_mut().fill(0_u64);
    }
}

impl<W: AsRef<[u64]> + AsMut<[u64]>, V: VariableWord> Packed<W, V> {
    /// Applies the given function to up to `len` values in the packed array.
    #[inline]
    pub fn apply<F>(&mut self, mut ops: F, len: usize)
    where
        F: FnMut(V::Word) -> V::Word,
    {
        let mut number_of_values: usize = 0;
        let mut value_offset = 0;
        for i in 0..self.words.as_ref().len() {
            let mut number_of_values_in_word =
                (64 - usize::from(value_offset)) / V::NUMBER_OF_BITS_USIZE;

            if number_of_values + number_of_values_in_word > len {
                number_of_values_in_word = len - number_of_values;
            }

            let word = &mut self.words.as_mut()[i];
            for _ in 0..number_of_values_in_word {
                let register = extract_value_from_word::<V>(*word, value_offset);
                let new_register = ops(register);
                insert_value_into_word::<V>(word, value_offset, new_register.into());
                value_offset += V::NUMBER_OF_BITS;
            }
            number_of_values += number_of_values_in_word;

            if Self::is_bridge_offset(value_offset) && i != self.words.as_ref().len() - 1 {
                let (low, high) = self.words.as_mut().split_at_mut(i + 1);
                let low = &mut low[i];
                let high = &mut high[0];
                let value = extract_bridge_value_from_word::<V>(*low, *high, value_offset);
                let new_value = ops(value);
                insert_bridge_value_into_word::<V>(low, high, value_offset, new_value.into());
                value_offset = V::NUMBER_OF_BITS - (64 - value_offset);
                number_of_values += 1;
            } else {
                value_offset = 0;
            }
        }
    }
}

#[cfg(test)]
mod test_extract_bridge_value_from_word {
    use super::*;
    use crate::{Bits4, Bits5, Bits6};

    #[test]
    fn test_extract_bridge_value_from_word_bits4() {
        test_bridge::<Bits4>();
    }

    #[test]
    fn test_extract_bridge_value_from_word_bits5() {
        test_bridge::<Bits5>();
    }

    #[test]
    fn test_extract_bridge_value_from_word_bits6() {
        test_bridge::<Bits6>();
    }

    #[allow(unsafe_code)]
    fn test_bridge<V: VariableWord>() {
        let mut lower_word = 0_u64;
        let mut upper_word = 0_u64;
        for value in 0..=V::MASK.min(200) as u64 {
            for offset in (65_u8 - V::NUMBER_OF_BITS)..64_u8 {
                insert_bridge_value_into_word::<V>(&mut lower_word, &mut upper_word, offset, value);
                let extracted = extract_bridge_value_from_word::<V>(lower_word, upper_word, offset);
                assert_eq!(
                    extracted,
                    unsafe { V::unchecked_from_u64(value) },
                    "The value extracted from the word {} at offset {} should be equal to the value {}",
                    lower_word,
                    offset,
                    value
                );
            }
        }
    }
}

#[cfg(test)]
mod test_extract_value_from_word {
    use super::*;
    use crate::{Bits4, Bits5, Bits6};

    #[test]
    fn test_extract_value_from_word_bits4() {
        test_extract::<Bits4>();
    }

    #[test]
    fn test_extract_value_from_word_bits5() {
        test_extract::<Bits5>();
    }

    #[test]
    fn test_extract_value_from_word_bits6() {
        test_extract::<Bits6>();
    }

    #[allow(unsafe_code)]
    fn test_extract<V: VariableWord>() {
        let mut word = 0_u64;
        for value in 0..=V::MASK.min(200) as u64 {
            for offset in 0_u8..=(64_u8 - V::NUMBER_OF_BITS) {
                insert_value_into_word::<V>(&mut word, offset, value);
                assert_eq!(
                    extract_value_from_word::<V>(word, offset),
                    unsafe { V::unchecked_from_u64(value) },
                    "The value extracted from the word {} at offset {} should be equal to the value {}",
                    word,
                    offset,
                    value
                );
            }
        }
    }
}

#[cfg(test)]
mod test_split_index {
    use super::*;
    use crate::{Bits4, Bits5, Bits6};

    #[test]
    fn test_split_packed_index_bits4() {
        test_split::<Bits4>();
    }

    #[test]
    fn test_split_packed_index_bits5() {
        test_split::<Bits5>();
    }

    #[test]
    fn test_split_packed_index_bits6() {
        test_split::<Bits6>();
    }

    fn test_split<V: VariableWord>() {
        let minimum_index = 0_usize;
        let maximum_index = 1_usize << 18;
        for index in minimum_index..maximum_index {
            let expected_word_index = (usize::from(V::NUMBER_OF_BITS) * index) / 64;
            let expected_relative_register_offset = (usize::from(V::NUMBER_OF_BITS) * index) % 64;
            let (word_index, relative_register_offset) = split_packed_index::<V>(index);
            assert_eq!(
                word_index, expected_word_index as usize,
                "The word index {} should be equal to the word index {}",
                word_index, expected_word_index
            );
            assert_eq!(
                relative_register_offset,
                expected_relative_register_offset as u8,
                "The relative register offset {} should be equal to the relative register offset {}",
                relative_register_offset,
                expected_relative_register_offset
            );
        }
    }
}
#[cfg(test)]
mod test_bits_u16 {
    use super::*;
    use crate::{Bits10, Bits12, Bits16, Bits9};

    #[test]
    fn test_extract_value_from_word_bits9() {
        test_extract::<Bits9>();
    }

    #[test]
    fn test_extract_value_from_word_bits10() {
        test_extract::<Bits10>();
    }

    #[test]
    fn test_extract_value_from_word_bits12() {
        test_extract::<Bits12>();
    }

    #[test]
    fn test_extract_value_from_word_bits16() {
        test_extract::<Bits16>();
    }

    #[test]
    fn test_extract_bridge_value_from_word_bits9() {
        test_bridge::<Bits9>();
    }

    #[test]
    fn test_extract_bridge_value_from_word_bits16() {
        test_bridge::<Bits16>();
    }

    #[test]
    fn test_split_packed_index_bits9() {
        test_split::<Bits9>();
    }

    #[test]
    fn test_split_packed_index_bits16() {
        test_split::<Bits16>();
    }

    #[allow(unsafe_code)]
    fn test_extract<V: VariableWord>() {
        let mut word = 0_u64;
        for value in 0..=V::MASK.min(500) as u64 {
            for offset in 0_u8..=(64_u8 - V::NUMBER_OF_BITS) {
                insert_value_into_word::<V>(&mut word, offset, value);
                assert_eq!(extract_value_from_word::<V>(word, offset), unsafe {
                    V::unchecked_from_u64(value)
                });
            }
        }
    }

    #[allow(unsafe_code)]
    fn test_bridge<V: VariableWord>() {
        let mut lower_word = 0_u64;
        let mut upper_word = 0_u64;
        for value in 0..=V::MASK.min(500) as u64 {
            for offset in (65_u8 - V::NUMBER_OF_BITS)..64_u8 {
                insert_bridge_value_into_word::<V>(&mut lower_word, &mut upper_word, offset, value);
                let extracted = extract_bridge_value_from_word::<V>(lower_word, upper_word, offset);
                assert_eq!(extracted, unsafe { V::unchecked_from_u64(value) });
            }
        }
    }

    fn test_split<V: VariableWord>() {
        for index in 0..(1_usize << 10) {
            let expected_word_index = (usize::from(V::NUMBER_OF_BITS) * index) / 64;
            let expected_relative_register_offset = (usize::from(V::NUMBER_OF_BITS) * index) % 64;
            let (word_index, relative_register_offset) = split_packed_index::<V>(index);
            assert_eq!(word_index, expected_word_index);
            assert_eq!(
                relative_register_offset,
                expected_relative_register_offset as u8
            );
        }
    }
}

#[cfg(test)]
mod test_packed_set_get {
    use super::*;
    use crate::{Bits12, Bits16, Bits4, Bits6, Bits9};

    #[allow(unsafe_code)]
    fn test_set_get<V: VariableWord>(num_registers: usize)
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();

        // Write known values.
        for i in 0..num_registers {
            let value = (i as u64 % (V::MASK + 1)) as u64;
            packed.set(i, unsafe { V::unchecked_from_u64(value) });
        }

        // Read back and verify.
        for i in 0..num_registers {
            let expected = (i as u64 % (V::MASK + 1)) as u64;
            let got = packed.get(i);
            assert_eq!(
                got,
                unsafe { V::unchecked_from_u64(expected) },
                "Mismatch at index {} for {:?}: expected {}, got {}",
                i,
                core::any::type_name::<V>(),
                expected,
                got
            );
        }
    }

    #[test]
    fn test_set_get_bits4() {
        test_set_get::<Bits4>(80);
    }

    #[test]
    fn test_set_get_bits6() {
        test_set_get::<Bits6>(60);
    }

    #[test]
    fn test_set_get_bits9() {
        test_set_get::<Bits9>(40);
    }

    #[test]
    fn test_set_get_bits12() {
        test_set_get::<Bits12>(30);
    }

    #[test]
    fn test_set_get_bits16() {
        test_set_get::<Bits16>(20);
    }
}

#[cfg(test)]
mod test_packed_iter {
    use super::*;
    use crate::{Bits4, Bits6, Bits9};

    #[allow(unsafe_code)]
    fn test_iter<V: VariableWord>(num_registers: usize)
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();

        // Write sequential values.
        for i in 0..num_registers {
            let value = (i as u64 % (V::MASK + 1)) as u64;
            packed.set(i, unsafe { V::unchecked_from_u64(value) });
        }

        // Iterate and verify.
        let collected: Vec<V::Word> = packed.iter_values(num_registers).collect();
        assert_eq!(collected.len(), num_registers);
        for (i, &got) in collected.iter().enumerate() {
            let expected = (i as u64 % (V::MASK + 1)) as u64;
            assert_eq!(
                got,
                unsafe { V::unchecked_from_u64(expected) },
                "Iterator mismatch at index {} for {:?}",
                i,
                core::any::type_name::<V>()
            );
        }
    }

    #[test]
    fn test_iter_bits4() {
        test_iter::<Bits4>(80);
    }

    #[test]
    fn test_iter_bits6() {
        test_iter::<Bits6>(60);
    }

    #[test]
    fn test_iter_bits9() {
        test_iter::<Bits9>(40);
    }
}

#[cfg(test)]
mod test_packed_clear {
    use super::*;
    use crate::{Bits16, Bits4, Bits9};

    #[allow(unsafe_code)]
    fn test_clear<V: VariableWord>(num_registers: usize)
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();

        // Fill with non-zero values.
        for i in 0..num_registers {
            packed.set(i, unsafe { V::unchecked_from_u64(V::MASK) });
        }

        // Clear.
        packed.clear();

        // Verify all zeros.
        for i in 0..num_registers {
            assert_eq!(
                packed.get(i),
                V::Word::ZERO,
                "Register {} not zero after clear for {:?}",
                i,
                core::any::type_name::<V>()
            );
        }
    }

    #[test]
    fn test_clear_bits4() {
        test_clear::<Bits4>(80);
    }

    #[test]
    fn test_clear_bits9() {
        test_clear::<Bits9>(40);
    }

    #[test]
    fn test_clear_bits16() {
        test_clear::<Bits16>(20);
    }
}
#[cfg(test)]
mod test_packed_apply {
    use super::*;
    use crate::{Bits4, Bits8};

    #[test]
    fn test_apply_identity() {
        let mut packed: Packed<[u64; 4], Bits4> = Packed::default();
        for i in 0..10 {
            packed.set(i, i as u8);
        }
        packed.apply(|v| v, 10);
        for i in 0..10 {
            assert_eq!(packed.get(i), i as u8);
        }
    }

    #[test]
    fn test_apply_double() {
        let mut packed: Packed<[u64; 4], Bits4> = Packed::default();
        for i in 0..10 {
            packed.set(i, 2_u8);
        }
        packed.apply(|v| v * 2, 10);
        for i in 0..10 {
            assert_eq!(packed.get(i), 4);
        }
    }

    #[test]
    fn test_apply_invert_bits() {
        let mut packed: Packed<[u64; 4], Bits4> = Packed::default();
        for i in 0..10 {
            packed.set(i, 0x5);
        }
        packed.apply(|v| v ^ 0xF, 10);
        for i in 0..10 {
            assert_eq!(packed.get(i), 0xA);
        }
    }

    #[test]
    fn test_apply_bits8() {
        let mut packed: Packed<[u64; 4], Bits8> = Packed::default();
        for i in 0..8 {
            packed.set(i, 10_u8);
        }
        packed.apply(|v| v.wrapping_add(5), 8);
        for i in 0..8 {
            assert_eq!(packed.get(i), 15);
        }
    }
}

#[cfg(test)]
mod test_is_bridge_offset {
    use super::*;
    use crate::{Bits4, Bits5, Bits9};

    #[test]
    fn test_bridge_offsets_bits4() {
        for i in 0..32u8 {
            let _ = Packed::<[u64; 2], Bits4>::is_bridge_offset(i);
        }
    }

    #[test]
    fn test_bridge_offsets_bits5() {
        for i in 0..24u8 {
            let _ = Packed::<[u64; 2], Bits5>::is_bridge_offset(i);
        }
    }

    #[test]
    fn test_bridge_offsets_bits9() {
        for i in 0..14u8 {
            let _ = Packed::<[u64; 2], Bits9>::is_bridge_offset(i);
        }
    }
}

#[cfg(test)]
mod test_packed_exact_size_iter {
    use super::*;
    use crate::Bits4;

    #[test]
    fn test_exact_size_iterator_len() {
        let packed: Packed<[u64; 4], Bits4> = Packed::default();
        let iter = packed.iter_values(64);
        assert_eq!(iter.len(), 64);
        assert_eq!(iter.size_hint(), (64, Some(64)));
    }
}

#[cfg(test)]
mod test_variable_word {
    use super::*;
    use crate::{Bits4, Bits8};

    #[test]
    fn test_variable_word_constants() {
        assert_eq!(Bits4::NUMBER_OF_BITS, 4);
        assert_eq!(Bits4::MASK, 0xF);
        assert_eq!(Bits4::NUMBER_OF_ENTRIES, 16);

        assert_eq!(Bits8::NUMBER_OF_BITS, 8);
        assert_eq!(Bits8::MASK, 0xFF);
        assert_eq!(Bits8::NUMBER_OF_ENTRIES, 8);
    }

    #[test]
    fn test_u64_variable_word() {
        assert_eq!(<u64 as VariableWord>::NUMBER_OF_BITS, 64);
        assert_eq!(<u64 as VariableWord>::MASK, u64::MAX);
        assert_eq!(<u64 as VariableWord>::NUMBER_OF_ENTRIES, 1);
    }
}
#[cfg(test)]
mod test_packed_iter_values {
    use super::*;
    use crate::{Bits12, Bits16, Bits4, Bits5, Bits6, Bits9};

    #[allow(unsafe_code)]
    fn iter_exact_values<V: VariableWord>(num_registers: usize)
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();

        // Write distinct values at each position.
        for i in 0..num_registers {
            let value = i as u64 % (V::MASK + 1);
            packed.set(i, unsafe { V::unchecked_from_u64(value) });
        }

        // Iterate and verify each value matches what we set.
        let collected: Vec<V::Word> = packed.iter_values(num_registers).collect();
        assert_eq!(collected.len(), num_registers);
        for (i, &got) in collected.iter().enumerate() {
            let expected = (i as u64 % (V::MASK + 1)) as u64;
            assert_eq!(
                got,
                unsafe { V::unchecked_from_u64(expected) },
                "Mismatch at index {} for {:?}: expected {} got {}",
                i,
                core::any::type_name::<V>(),
                expected,
                got
            );
        }
    }

    #[test]
    fn test_iter_bits4_exact_values() {
        iter_exact_values::<Bits4>(64);
    }

    #[test]
    fn test_iter_bits5_exact_values() {
        iter_exact_values::<Bits5>(64);
    }

    #[test]
    fn test_iter_bits6_exact_values() {
        iter_exact_values::<Bits6>(64);
    }

    #[test]
    fn test_iter_bits9_exact_values() {
        iter_exact_values::<Bits9>(56);
    }

    #[test]
    fn test_iter_bits12_exact_values() {
        iter_exact_values::<Bits12>(48);
    }

    #[test]
    fn test_iter_bits16_exact_values() {
        iter_exact_values::<Bits16>(32);
    }
}

#[cfg(test)]
mod test_packed_iter_exhaustion {
    use super::*;
    use crate::{Bits6, Bits9};

    #[allow(unsafe_code)]
    fn iter_exhaustion<V: VariableWord>(num_registers: usize)
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();
        for i in 0..num_registers {
            packed.set(i, unsafe { V::unchecked_from_u64(i as u64) });
        }

        let mut iter = packed.iter_values(num_registers);

        // Consume exactly num_registers values.
        for i in 0..num_registers {
            let got = iter.next();
            assert!(
                got.is_some(),
                "Iterator returned None at index {} for {:?}",
                i,
                core::any::type_name::<V>()
            );
            let expected = unsafe { V::unchecked_from_u64(i as u64) };
            assert_eq!(got, Some(expected));
        }

        // Iterator should now be exhausted.
        assert!(
            iter.next().is_none(),
            "Iterator should be exhausted after {} values for {:?}",
            num_registers,
            core::any::type_name::<V>()
        );
    }

    #[test]
    fn test_iter_exhaustion_bits6() {
        iter_exhaustion::<Bits6>(64);
    }

    #[test]
    fn test_iter_exhaustion_bits9() {
        iter_exhaustion::<Bits9>(56);
    }
}

#[cfg(test)]
mod test_packed_iter_zipped {
    use super::*;
    use crate::{Bits4, Bits5, Bits9};

    #[allow(unsafe_code)]
    fn zipped_iter<V: VariableWord>(num_registers: usize)
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed_a: Packed<[u64; 16], V> = Packed::default();
        let mut packed_b: Packed<[u64; 16], V> = Packed::default();

        for i in 0..num_registers {
            let val_a = i as u64 % (V::MASK + 1);
            let val_b = (i * 3 + 7) as u64 % (V::MASK + 1);
            packed_a.set(i, unsafe { V::unchecked_from_u64(val_a) });
            packed_b.set(i, unsafe { V::unchecked_from_u64(val_b) });
        }

        let collected: Vec<[V::Word; 2]> = packed_a
            .iter_values_zipped(&packed_b, num_registers)
            .collect();
        assert_eq!(collected.len(), num_registers);

        for (i, pair) in collected.iter().enumerate() {
            let expected_a = (i as u64 % (V::MASK + 1)) as u64;
            let expected_b = ((i * 3 + 7) as u64 % (V::MASK + 1)) as u64;
            assert_eq!(
                pair[0],
                unsafe { V::unchecked_from_u64(expected_a) },
                "Zipped A mismatch at index {} for {:?}",
                i,
                core::any::type_name::<V>()
            );
            assert_eq!(
                pair[1],
                unsafe { V::unchecked_from_u64(expected_b) },
                "Zipped B mismatch at index {} for {:?}",
                i,
                core::any::type_name::<V>()
            );
        }
    }

    #[test]
    fn test_zipped_iter_bits4() {
        zipped_iter::<Bits4>(64);
    }

    #[test]
    fn test_zipped_iter_bits5() {
        zipped_iter::<Bits5>(64);
    }

    #[test]
    fn test_zipped_iter_bits9() {
        zipped_iter::<Bits9>(56);
    }
}

#[cfg(test)]
mod test_apply_lengths {
    use super::*;
    use crate::{Bits4, Bits5, Bits9};

    #[allow(unsafe_code)]
    fn apply_zero_length<V: VariableWord>(num_registers: usize)
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();
        for i in 0..num_registers {
            packed.set(i, unsafe { V::unchecked_from_u64(i as u64) });
        }
        packed.apply(|v| v + V::Word::from(1), 0);
        for i in 0..num_registers {
            let expected = unsafe { V::unchecked_from_u64(i as u64) };
            assert_eq!(
                packed.get(i),
                expected,
                "Value at index {} changed with zero-length apply",
                i
            );
        }
    }

    #[test]
    fn test_apply_zero_length_bits4() {
        apply_zero_length::<Bits4>(16);
    }

    #[test]
    fn test_apply_zero_length_bits9() {
        apply_zero_length::<Bits9>(14);
    }

    #[allow(unsafe_code)]
    fn apply_single_value<V: VariableWord>()
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();
        packed.set(0, unsafe { V::unchecked_from_u64(5) });
        packed.set(1, unsafe { V::unchecked_from_u64(10) });
        packed.apply(|v| v + V::Word::from(1), 1);
        assert_eq!(packed.get(0), unsafe { V::unchecked_from_u64(6) });
        assert_eq!(packed.get(1), unsafe { V::unchecked_from_u64(10) });
    }

    #[test]
    fn test_apply_single_value_bits4() {
        apply_single_value::<Bits4>();
    }

    #[test]
    fn test_apply_single_value_bits5() {
        apply_single_value::<Bits5>();
    }

    #[test]
    fn test_apply_single_value_bits9() {
        apply_single_value::<Bits9>();
    }

    #[allow(unsafe_code)]
    fn apply_partial_length<V: VariableWord>(total: usize, applied: usize)
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();
        for i in 0..total {
            packed.set(i, unsafe { V::unchecked_from_u64(i as u64) });
        }
        packed.apply(|v| v + V::Word::from(1), applied);
        for i in 0..applied {
            let expected = (i as u64 + 1) % (V::MASK + 1);
            assert_eq!(
                packed.get(i),
                unsafe { V::unchecked_from_u64(expected) },
                "Applied value at index {} wrong",
                i
            );
        }
        for i in applied..total {
            assert_eq!(
                packed.get(i),
                unsafe { V::unchecked_from_u64(i as u64) },
                "Unapplied value at index {} changed",
                i
            );
        }
    }

    #[test]
    fn test_apply_partial_bits5() {
        apply_partial_length::<Bits5>(32, 13);
    }

    #[test]
    fn test_apply_partial_bits9() {
        apply_partial_length::<Bits9>(28, 8);
    }

    #[allow(unsafe_code)]
    fn apply_across_word_boundary<V: VariableWord>()
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();
        let entries_per_word = V::NUMBER_OF_ENTRIES;
        let total = entries_per_word * 2 + 1;

        for i in 0..total {
            packed.set(i, unsafe { V::unchecked_from_u64(1) });
        }

        packed.apply(|v| v + V::Word::from(1), total);

        for i in 0..total {
            assert_eq!(
                packed.get(i),
                unsafe { V::unchecked_from_u64(2) },
                "Value at index {} not doubled across word boundary",
                i
            );
        }
    }

    #[test]
    fn test_apply_across_boundary_bits9() {
        apply_across_word_boundary::<Bits9>();
    }

    #[allow(unsafe_code)]
    fn apply_bridge_values<V: VariableWord>()
    where
        Packed<[u64; 16], V>: Default,
    {
        // Only test types that actually have bridge offsets.
        if V::NUMBER_OF_BITS_USIZE * V::NUMBER_OF_ENTRIES >= 64 {
            return;
        }

        let mut packed: Packed<[u64; 16], V> = Packed::default();
        let entries_per_word = V::NUMBER_OF_ENTRIES;
        let bridge_start = entries_per_word;

        for i in 0..bridge_start + 2 {
            packed.set(i, unsafe { V::unchecked_from_u64(3) });
        }

        packed.apply(|v| v * V::Word::from(2), bridge_start + 2);

        for i in 0..bridge_start + 2 {
            assert_eq!(
                packed.get(i),
                unsafe { V::unchecked_from_u64(6) },
                "Bridge value at index {} wrong after apply",
                i
            );
        }
    }

    #[test]
    fn test_apply_bridge_bits5() {
        apply_bridge_values::<Bits5>();
    }

    #[test]
    fn test_apply_bridge_bits9() {
        apply_bridge_values::<Bits9>();
    }
}

#[cfg(test)]
mod test_iter_values_count {
    use super::*;
    use crate::{Bits12, Bits16, Bits4, Bits5, Bits9};

    #[allow(unsafe_code)]
    fn iter_exact_count<V: VariableWord>(requested: usize)
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();
        for i in 0..requested {
            packed.set(i, unsafe { V::unchecked_from_u64(i as u64) });
        }

        let iter = packed.iter_values(requested);
        assert_eq!(
            iter.len(),
            requested,
            "Iterator len should match requested count for {:?}",
            core::any::type_name::<V>()
        );

        let count = iter.count();
        assert_eq!(
            count,
            requested,
            "Iterator yielded {} items, expected {} for {:?}",
            count,
            requested,
            core::any::type_name::<V>()
        );
    }

    #[test]
    fn test_iter_count_bits4_one_word() {
        iter_exact_count::<Bits4>(16);
    }

    #[test]
    fn test_iter_count_bits4_partial() {
        iter_exact_count::<Bits4>(13);
    }

    #[test]
    fn test_iter_count_bits5_one_word() {
        iter_exact_count::<Bits5>(12);
    }

    #[test]
    fn test_iter_count_bits5_cross_boundary() {
        iter_exact_count::<Bits5>(14);
    }

    #[test]
    fn test_iter_count_bits9_one_word() {
        iter_exact_count::<Bits9>(7);
    }

    #[test]
    fn test_iter_count_bits9_cross_boundary() {
        iter_exact_count::<Bits9>(9);
    }

    #[test]
    fn test_iter_count_bits12_one_word() {
        iter_exact_count::<Bits12>(5);
    }

    #[test]
    fn test_iter_count_bits16_one_word() {
        iter_exact_count::<Bits16>(4);
    }

    #[test]
    fn test_iter_count_bits16_cross_boundary() {
        iter_exact_count::<Bits16>(6);
    }
}

#[cfg(test)]
mod test_bridge_register_get_set {
    use super::*;
    use crate::{Bits12, Bits5, Bits6, Bits9};

    #[allow(unsafe_code)]
    fn bridge_get_set<V: VariableWord>(test_value: u64)
    where
        Packed<[u64; 4], V>: Default,
    {
        // Only meaningful for types with bridge offsets.
        if V::NUMBER_OF_BITS_USIZE * V::NUMBER_OF_ENTRIES >= 64 {
            return;
        }

        let entries_per_word = V::NUMBER_OF_ENTRIES;
        let bridge_index = entries_per_word; // First index that is a bridge.

        let mut packed: Packed<[u64; 4], V> = Packed::default();

        // Set non-bridge values around the bridge.
        packed.set(bridge_index - 1, unsafe { V::unchecked_from_u64(1) });
        packed.set(bridge_index, unsafe { V::unchecked_from_u64(test_value) });
        packed.set(bridge_index + 1, unsafe { V::unchecked_from_u64(2) });

        // Verify the bridge value.
        assert_eq!(
            packed.get(bridge_index),
            unsafe { V::unchecked_from_u64(test_value) },
            "Bridge get at index {} wrong for {:?}: expected {} got {}",
            bridge_index,
            core::any::type_name::<V>(),
            test_value,
            packed.get(bridge_index)
        );

        // Verify neighbors are unchanged.
        assert_eq!(
            packed.get(bridge_index - 1),
            unsafe { V::unchecked_from_u64(1) },
            "Neighbor before bridge changed for {:?}",
            core::any::type_name::<V>()
        );
        assert_eq!(
            packed.get(bridge_index + 1),
            unsafe { V::unchecked_from_u64(2) },
            "Neighbor after bridge changed for {:?}",
            core::any::type_name::<V>()
        );

        // Overwrite the bridge value and verify again.
        packed.set(bridge_index, unsafe { V::unchecked_from_u64(V::MASK) });
        assert_eq!(
            packed.get(bridge_index),
            unsafe { V::unchecked_from_u64(V::MASK) },
            "Bridge overwrite failed for {:?}",
            core::any::type_name::<V>()
        );
    }

    #[test]
    fn test_bridge_get_set_bits5() {
        bridge_get_set::<Bits5>(31);
    }

    #[test]
    fn test_bridge_get_set_bits5_mid() {
        bridge_get_set::<Bits5>(17);
    }

    #[test]
    fn test_bridge_get_set_bits6() {
        bridge_get_set::<Bits6>(63);
    }

    #[test]
    fn test_bridge_get_set_bits9() {
        bridge_get_set::<Bits9>(255);
    }

    #[test]
    fn test_bridge_get_set_bits9_mid() {
        bridge_get_set::<Bits9>(128);
    }

    #[test]
    fn test_bridge_get_set_bits12() {
        bridge_get_set::<Bits12>(4095);
    }
}

#[cfg(test)]
mod test_packed_clear_then_set {
    use super::*;
    use crate::{Bits12, Bits16, Bits4, Bits5, Bits9};

    #[allow(unsafe_code)]
    fn clear_then_set<V: VariableWord>(num_registers: usize, fill_value: u64)
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();

        // Fill with max values.
        for i in 0..num_registers {
            packed.set(i, unsafe { V::unchecked_from_u64(V::MASK) });
        }

        // Clear.
        packed.clear();

        // Verify all zeros.
        for i in 0..num_registers {
            assert_eq!(
                packed.get(i),
                V::Word::ZERO,
                "Register {} not zero after clear for {:?}",
                i,
                core::any::type_name::<V>()
            );
        }

        // Set all registers to a new value.
        for i in 0..num_registers {
            packed.set(i, unsafe { V::unchecked_from_u64(fill_value) });
        }

        // Verify all have the new value.
        for i in 0..num_registers {
            assert_eq!(
                packed.get(i),
                unsafe { V::unchecked_from_u64(fill_value) },
                "Register {} wrong after set post-clear for {:?}: expected {} got {}",
                i,
                core::any::type_name::<V>(),
                fill_value,
                packed.get(i)
            );
        }
    }

    #[test]
    fn test_clear_set_bits4() {
        clear_then_set::<Bits4>(64, 0xF);
    }

    #[test]
    fn test_clear_set_bits5() {
        clear_then_set::<Bits5>(64, 31);
    }

    #[test]
    fn test_clear_set_bits9() {
        clear_then_set::<Bits9>(56, 511);
    }

    #[test]
    fn test_clear_set_bits12() {
        clear_then_set::<Bits12>(48, 4095);
    }

    #[test]
    fn test_clear_set_bits16() {
        clear_then_set::<Bits16>(32, 65535);
    }

    #[allow(unsafe_code)]
    fn clear_set_alternating<V: VariableWord>(num_registers: usize)
    where
        Packed<[u64; 16], V>: Default,
    {
        let mut packed: Packed<[u64; 16], V> = Packed::default();

        // Fill with max, clear, then set alternating values.
        for i in 0..num_registers {
            packed.set(i, unsafe { V::unchecked_from_u64(V::MASK) });
        }
        packed.clear();

        for i in 0..num_registers {
            let value = if i % 2 == 0 { V::MASK } else { 0 };
            packed.set(i, unsafe { V::unchecked_from_u64(value) });
        }

        for i in 0..num_registers {
            let expected = if i % 2 == 0 { V::MASK } else { 0 };
            assert_eq!(
                packed.get(i),
                unsafe { V::unchecked_from_u64(expected) },
                "Alternating pattern wrong at index {} for {:?}",
                i,
                core::any::type_name::<V>()
            );
        }
    }

    #[test]
    fn test_clear_set_alternating_bits5() {
        clear_set_alternating::<Bits5>(32);
    }

    #[test]
    fn test_clear_set_alternating_bits9() {
        clear_set_alternating::<Bits9>(28);
    }
    #[allow(unsafe_code)]
    #[test]
    fn test_bridge_extraction_both_bits() {
        // Test bridge value extraction when both higher and lower bits are non-zero
        use crate::Bits5;

        let mut packed: Packed<[u64; 2], Bits5> = Packed::default();

        // Set value at the bridge position (offset 60 for Bits5)
        // This will split across two words
        let bridge_value: u64 = 0b11011; // All bits set
        packed.set(12, unsafe { Bits5::unchecked_from_u64(bridge_value) });

        // Get the value back
        assert_eq!(
            packed.get(12),
            unsafe { Bits5::unchecked_from_u64(bridge_value) },
            "Bridge value should be correctly extracted"
        );
    }
}
