//! Random number generators and test helpers.

use crate::{One, VariableWord, Zero};

/// `SplitMix64` step: a pseudorandom permutation of `u64`. Very fast, good quality.
#[must_use]
#[inline]
pub fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Returns an iterator over `size` random `V::Word` values in `[0..=maximal_value]`.
///
/// # Arguments
/// * `size` - The number of values to generate.
/// * `maximal_value` - The upper bound (inclusive). If `None`, uses `V::MASK`.
/// * `random_state` - Seed. If `None`, a fixed seed is used.
///
/// # Panics
/// Panics if `maximal_value` is `Some(0)`.
#[inline]
#[allow(unsafe_code)]
pub fn iter_random_values<V: VariableWord>(
    size: u64,
    maximal_value: Option<V::Word>,
    random_state: Option<u64>,
) -> impl Iterator<Item = V::Word> {
    iter_var_len_random_values::<V>(size, size, maximal_value, random_state)
}

/// Returns an iterator over a random number of random `V::Word` values.
///
/// The actual count is uniformly chosen in `[minimal_size..=maximal_size]`.
/// Each value is uniformly chosen in `[0..=maximal_value]` (or `[0..=V::MASK]` if `None`).
///
/// # Arguments
/// * `minimal_size` - Minimum number of values.
/// * `maximal_size` - Maximum number of values.
/// * `maximal_value` - Upper bound per value. If `None`, uses `V::MASK`.
/// * `random_state` - Seed. If `None`, a fixed seed is used.
///
/// # Panics
/// Panics if `minimal_size > maximal_size` or `maximal_value == Some(0)`.
#[inline]
#[allow(unsafe_code)]
pub fn iter_var_len_random_values<V: VariableWord>(
    minimal_size: u64,
    maximal_size: u64,
    maximal_value: Option<V::Word>,
    random_state: Option<u64>,
) -> impl Iterator<Item = V::Word> {
    assert!(
        minimal_size <= maximal_size,
        "The minimal size ({minimal_size}) must be less than or equal to the maximal size ({maximal_size})."
    );
    if let Some(mv) = maximal_value.as_ref() {
        assert!(
            !mv.is_zero(),
            "The maximal value must be provided if the variable mask is zero."
        );
    }

    let delta = maximal_size - minimal_size;
    let mut state = random_state.unwrap_or(12_834_791_235_231_473_875_u64);

    state = splitmix64(state);
    let size = minimal_size + if delta > 0 { state % delta } else { 0 };

    state = splitmix64(state);
    let actual_maximal_value: V::Word = maximal_value
        .map_or(unsafe { V::unchecked_from_u64(V::MASK) }, |mv| unsafe {
            V::Word::ONE + V::unchecked_from_u64(splitmix64(state) & V::MASK) % mv
        });
    state = splitmix64(state);

    (0..size).map(move |_| {
        state = splitmix64(state);
        unsafe { V::unchecked_from_u64(state & V::MASK) % actual_maximal_value }
    })
}
/// Stateful `SplitMix64` PRNG. Single `u64` of state; steps the underlying permutation from
/// [`splitmix64`] and layers Lemire-style bounded draws and inverse-CDF exponentials on top.
#[derive(Debug, Clone, Copy)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Whitening constant `XORed` into the seed so the per-instance PRNG output cannot tail-share
    /// with another instance whose input hash is a [`splitmix64`] step away from this one. Without
    /// it, callers feeding a [`splitmix64`]-derived hash stream pull a ~17 percent under-estimate
    /// bias out of the register state because consecutive elements' walks overlap. The XOR is
    /// non-commutative with the `SplitMix64` round mixing, so two chain-related inputs no longer
    /// produce chain-related seeds. Any non-zero constant outside the `SplitMix64` family works.
    const SEED_MIXER: u64 = 0xCBF2_9CE4_8422_2325;

    /// Seed from a 64-bit element hash. The hash is `XORed` with the whitening constant so a
    /// [`splitmix64`]-derived caller stream cannot share a PRNG output tail with this instance.
    #[inline]
    #[must_use]
    pub const fn new(hash: u64) -> Self {
        Self {
            state: hash ^ Self::SEED_MIXER,
        }
    }

    /// Next 64 bits of output. Advances state by one step of the underlying permutation.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = splitmix64(self.state);
        self.state
    }

    /// Uniform `f64` in `[0, 1)` from the top 53 bits.
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        <f64 as crate::FloatOps>::integer_exp2_minus(53) * (self.next_u64() >> 11) as f64
    }

    /// Uniform `u32` in `[0, n)` via Lemire's rejection method.
    ///
    /// # Panics
    /// Panics in debug builds when `n == 0`.
    #[inline]
    #[allow(clippy::many_single_char_names)]
    pub fn bounded_u32(&mut self, n: u32) -> u32 {
        debug_assert!(n > 0);
        let mut x = self.next_u64() as u32;
        let mut m = u64::from(x) * u64::from(n);
        let mut l = m as u32;
        if l < n {
            let t = n.wrapping_neg() % n;
            while l < t {
                x = self.next_u64() as u32;
                m = u64::from(x) * u64::from(n);
                l = m as u32;
            }
        }
        (m >> 32) as u32
    }

    /// `Exp(1)` sample via inverse-CDF: `-ln(1 - U)`, stable near `U = 0` thanks to `ln_1p`.
    #[inline]
    pub fn next_exp1(&mut self) -> f64 {
        let u = self.next_f64();
        -<f64 as crate::FloatOps>::ln_1p(-u)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Bits4;
    use std::vec::Vec;

    #[test]
    fn test_splitmix64_deterministic() {
        let v0 = splitmix64(0);
        let v1 = splitmix64(1);
        assert_ne!(v0, 0);
        assert_ne!(v1, 0);
        assert_ne!(v0, v1);
        assert_eq!(splitmix64(0), splitmix64(0));
    }

    #[test]
    fn test_splitmix64_different_inputs() {
        for i in 0..100 {
            assert_ne!(splitmix64(i), splitmix64(i + 1));
        }
    }

    #[test]
    fn test_splitmix64_nonzero_output() {
        for i in 0..1000_u64 {
            assert_ne!(splitmix64(i), 0, "splitmix64({i}) should not be zero");
        }
    }

    #[test]
    fn test_iter_random_values_count() {
        let values: Vec<_> = iter_random_values::<u8>(10, None, Some(42)).collect();
        assert_eq!(values.len(), 10);
    }

    #[test]
    fn test_iter_random_values_bounded() {
        let values: Vec<_> = iter_random_values::<u8>(50, Some(10), Some(123)).collect();
        for v in &values {
            assert!(*v <= 10, "Value {v} exceeds bound 10");
        }
    }

    #[test]
    fn test_iter_random_values_deterministic() {
        let a: Vec<_> = iter_random_values::<u8>(20, None, Some(999)).collect();
        let b: Vec<_> = iter_random_values::<u8>(20, None, Some(999)).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn test_iter_random_values_different_seeds() {
        let a: Vec<_> = iter_random_values::<u8>(20, None, Some(1)).collect();
        let b: Vec<_> = iter_random_values::<u8>(20, None, Some(2)).collect();
        assert_ne!(a, b);
    }

    #[test]
    fn test_iter_var_len_random_values_min_equals_max() {
        let values: Vec<_> = iter_var_len_random_values::<u8>(10, 10, None, Some(42)).collect();
        assert_eq!(values.len(), 10);
    }

    #[test]
    fn test_iter_var_len_random_values_range() {
        let mut min_len = u64::MAX;
        let mut max_len = 0;
        for seed in 0..50_u64 {
            let values: Vec<_> =
                iter_var_len_random_values::<u8>(5, 15, None, Some(seed)).collect();
            let len = values.len() as u64;
            assert!(len >= 5 && len <= 15, "Length {len} out of range [5, 15]");
            if len < min_len {
                min_len = len;
            }
            if len > max_len {
                max_len = len;
            }
        }
        assert!(max_len > min_len, "Expected variation in lengths");
    }

    #[test]
    fn test_iter_random_values_bits4() {
        let values: Vec<_> = iter_random_values::<Bits4>(30, None, Some(77)).collect();
        for v in &values {
            assert!(*v <= Bits4::MASK as u8, "Value {v} exceeds Bits4 mask");
        }
    }

    #[test]
    fn test_splitmix64_bitspread() {
        let mut all_bits = 0_u64;
        for i in 0..1000_u64 {
            all_bits |= splitmix64(i);
        }
        let bits_set = 64 - all_bits.leading_zeros();
        assert!(
            bits_set >= 60,
            "Only {bits_set} bits set after 1000 splitmix64 iterations"
        );
    }
    #[test]
    fn test_splitmix64_known_outputs() {
        // Verify known splitmix64 outputs to catch bit manipulation mutants
        assert_eq!(splitmix64(0), 0xE220_A839_7B1D_CDAF);
        assert_eq!(splitmix64(1), 0x910A_2DEC_8902_5CC1);
        assert_eq!(splitmix64(2), 0x9758_35DE_1C97_56CE);

        // Verify that changing input bits changes output significantly
        let a = splitmix64(0x1234_5678_9ABC_DEF0);
        let b = splitmix64(0x1234_5678_9ABC_DEF1);
        // XOR of outputs should have many bits set (avalanche effect)
        let diff = a ^ b;
        let bits_diff = diff.count_ones();
        assert!(
            bits_diff >= 20,
            "Expected significant bit difference, got {bits_diff}"
        );
    }

    #[test]
    fn splitmix64_wrapper_deterministic_from_seed() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..1024 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn splitmix64_wrapper_f64_in_unit_interval() {
        let mut r = SplitMix64::new(12345);
        for _ in 0..10_000 {
            let x = r.next_f64();
            assert!((0.0..1.0).contains(&x), "out of [0, 1): {x}");
        }
    }

    #[test]
    fn splitmix64_wrapper_bounded_u32_in_range() {
        let mut r = SplitMix64::new(99);
        for n in [1_u32, 2, 3, 5, 17, 1_000, 65_536, u32::MAX] {
            for _ in 0..512 {
                assert!(r.bounded_u32(n) < n);
            }
        }
    }

    #[test]
    fn splitmix64_wrapper_exp1_sample_mean_near_one() {
        let mut r = SplitMix64::new(7);
        let mut sum = 0.0;
        let n = 50_000;
        for _ in 0..n {
            let e = r.next_exp1();
            assert!(e >= 0.0);
            sum += e;
        }
        let mean = sum / f64::from(n);
        assert!((mean - 1.0).abs() < 0.05, "mean {mean} off from 1");
    }
}
