//! Sketch-family hasher marker trait plus the reproducible default hasher.

use core::hash::{BuildHasherDefault, Hasher};

/// Marker for hashers usable inside sketch counters: default-constructible, `Send + Sync`, `Clone`.
pub trait HasherType: Default + Hasher + Send + Sync + Clone {}

impl<T> HasherType for T where T: Default + Hasher + Send + Sync + Clone {}

const WY_PRIME_0: u64 = 0xa076_1d64_78bd_642f;
const WY_PRIME_1: u64 = 0xe703_7ed1_a0b4_28db;

#[inline]
fn wymix(a: u64, b: u64) -> u64 {
    let prod = u128::from(a).wrapping_mul(u128::from(b));
    (prod as u64) ^ ((prod >> 64) as u64)
}

/// Fixed-seed 64-bit hasher.
///
/// Streams arbitrary bytes by chunking into `u64` words and mixing each with the wyhash primitive.
/// Two equal byte sequences produce equal hashes; the seed is constant so the mapping is stable
/// across runs (and across processes), trading hash-flooding resistance for reproducibility.
#[derive(Debug, Clone, Copy)]
pub struct DefaultHasher {
    state: u64,
}

impl Default for DefaultHasher {
    #[inline]
    fn default() -> Self {
        Self { state: WY_PRIME_0 }
    }
}

impl Hasher for DefaultHasher {
    #[inline]
    fn finish(&self) -> u64 {
        wymix(self.state, WY_PRIME_1)
    }

    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        while bytes.len() >= 8 {
            let word = u64::from_le_bytes(bytes[..8].try_into().unwrap_or([0; 8]));
            self.state = wymix(self.state ^ word, WY_PRIME_1);
            bytes = &bytes[8..];
        }
        if !bytes.is_empty() {
            let mut buf = [0u8; 8];
            buf[..bytes.len()].copy_from_slice(bytes);
            let word = u64::from_le_bytes(buf);
            self.state = wymix(self.state ^ word, WY_PRIME_1);
        }
    }

    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.write_u64(u64::from(n));
    }

    #[inline]
    fn write_u16(&mut self, n: u16) {
        self.write_u64(u64::from(n));
    }

    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.write_u64(u64::from(n));
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.state = wymix(self.state ^ n, WY_PRIME_1);
    }
}

/// Default `BuildHasher` built on [`DefaultHasher`].
pub type DefaultBuildHasher = BuildHasherDefault<DefaultHasher>;

#[cfg(test)]
mod tests {
    use super::*;
    use core::hash::{BuildHasher, Hash};

    fn hash_via<H: BuildHasher, T: Hash>(builder: &H, value: T) -> u64 {
        builder.hash_one(&value)
    }

    #[test]
    fn deterministic_across_calls() {
        let bh = DefaultBuildHasher::default();
        let a = hash_via(&bh, 0xdead_beef_u64);
        let b = hash_via(&bh, 0xdead_beef_u64);
        assert_eq!(a, b);
    }

    #[test]
    fn distinct_inputs_get_distinct_hashes() {
        let bh = DefaultBuildHasher::default();
        let mut seen = [0_u64; 32];
        for (i, slot) in seen.iter_mut().enumerate() {
            *slot = hash_via(&bh, i as u64);
        }
        for i in 0..seen.len() {
            for j in (i + 1)..seen.len() {
                assert_ne!(seen[i], seen[j], "collision at ({i}, {j})");
            }
        }
    }

    #[test]
    fn hashes_strings() {
        let bh = DefaultBuildHasher::default();
        let a = hash_via(&bh, "hello");
        let b = hash_via(&bh, "world");
        assert_ne!(a, b);
        let c = hash_via(&bh, "hello");
        assert_eq!(a, c);
    }
}
