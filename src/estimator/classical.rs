//! Classical `HyperLogLog` cardinality estimator.
//!
//! The canonical harmonic-sum estimator from Flajolet et al. 2007, with the standard bias
//! corrections: linear counting at the low end and the log-tail saturation correction at the top
//! of the 32-bit range. Each function operates on the register counts and the harmonic sum, with
//! no dependence on the register-storage layout, so the same code drives every consumer that
//! keeps a classical HLL register array.

use crate::{FloatOps, Precision};

/// `alpha_m` bias-correction constant for a given number of registers `m`.
///
/// # Examples
///
/// ```
/// use sketching_core::estimator::classical::alpha_m;
/// assert!((alpha_m(16) - 0.673).abs() < 1e-6);
/// assert!((alpha_m(64) - 0.709).abs() < 1e-6);
/// ```
#[inline]
#[must_use]
pub fn alpha_m(m: u64) -> f64 {
    match m {
        16 => 0.673,
        32 => 0.697,
        64 => 0.709,
        _ => 0.7213 / (1.0 + 1.079 / m as f64),
    }
}

/// Raw `HyperLogLog` estimate from a harmonic sum.
///
/// The harmonic sum is `sum(2^(-M[j]))` over all registers.
///
/// # Examples
///
/// ```
/// use sketching_core::estimator::classical::raw_estimate;
/// // All registers at value 1: harmonic_sum = m * 0.5
/// let est = raw_estimate(256, 128.0);
/// assert!(est > 0.0);
/// ```
#[inline]
#[must_use]
pub fn raw_estimate(m: u64, harmonic_sum: f64) -> f64 {
    alpha_m(m) * m as f64 * m as f64 / harmonic_sum
}

/// Bias-corrected `HyperLogLog` estimate.
///
/// Uses linear counting for small cardinalities (raw estimate at most `5m/2`), the log-tail
/// saturation correction for large ranges (raw estimate above `u64::MAX / 30`), and the raw
/// estimate otherwise.
///
/// # Examples
///
/// ```
/// use sketching_core::estimator::classical::corrected_estimate;
/// // Empty sketch: all registers zero.
/// assert_eq!(corrected_estimate(256, 256.0, 256), 0.0);
/// ```
#[inline]
#[must_use]
pub fn corrected_estimate(m: u64, harmonic_sum: f64, zeros: u64) -> f64 {
    if zeros == m {
        return 0.0;
    }

    let e = raw_estimate(m, harmonic_sum);

    // Linear counting for small cardinalities: the classic HyperLogLog small-range correction
    // applies while the raw estimate is at most 5/2 of the register count.
    if e <= (5.0 / 2.0) * m as f64 && zeros > 0 {
        return m as f64 * FloatOps::natural_log(m as f64 / zeros as f64);
    }

    // Saturation correction for large cardinalities.
    if e > (u64::MAX as f64) / 30.0 {
        return -(u64::MAX as f64) * FloatOps::natural_log(1.0 - e / (u64::MAX as f64));
    }

    e
}

/// Harmonic sum and zero count from a register accessor.
///
/// `get_register(j)` yields the `j`-th register value; iterates `m` positions.
#[inline]
#[must_use]
pub fn harmonic_sum_and_zeros<F>(m: usize, mut get_register: F) -> (f64, u64)
where
    F: FnMut(usize) -> u8,
{
    let mut harmonic_sum = 0.0f64;
    let mut zeros = 0u64;

    for j in 0..m {
        let r = get_register(j);
        if r == 0 {
            zeros += 1;
        }
        harmonic_sum += <f64 as FloatOps>::integer_exp2_minus(r);
    }

    (harmonic_sum, zeros)
}

/// Theoretical relative standard error for a given precision.
#[inline]
#[must_use]
pub fn relative_standard_error<P: Precision>() -> f64 {
    1.04 / FloatOps::sqrt((1u64 << P::EXPONENT) as f64)
}

/// End-to-end classical `HyperLogLog` cardinality estimate.
///
/// Iterates the `m = 2^P::EXPONENT` registers via `get_register`, computes the harmonic sum,
/// then applies the bias correction.
#[inline]
#[must_use]
pub fn estimate_cardinality<P: Precision, F>(mut get_register: F) -> f64
where
    F: FnMut(usize) -> u8,
{
    let m = 1u64 << P::EXPONENT;
    let m_usize = m as usize;

    let (harmonic_sum, zeros) = harmonic_sum_and_zeros(m_usize, &mut get_register);
    corrected_estimate(m, harmonic_sum, zeros)
}
