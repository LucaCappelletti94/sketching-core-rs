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
        assert!(!mv.is_zero(), "The maximal value must be provided if the variable mask is zero.");
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
}
