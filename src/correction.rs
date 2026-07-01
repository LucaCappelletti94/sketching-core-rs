//! Ertl HyperLogLog-family range corrections.
//!
//! [`sigma`] and [`tau`] are the small-cardinality and saturation-cardinality corrections from
//! Ertl's "New cardinality estimation algorithms for `HyperLogLog` sketches" (2017). [`x_div_expm1`]
//! is the well-conditioned form of `x / (e^x - 1)` used by the `SetSketch` estimator.

use crate::FloatOps;

/// `sigma_b(x) = x + (b - 1) * sum_{k>=1} x^{b^k} * b^{k-1}`.
///
/// Convergence: each term is strictly smaller than the previous one for `x in (0, 1)` and `b > 1`;
/// the loop stops once `sum` stops changing (terms drop below f64 ULP). Returns `0` at `x = 0` and
/// `+inf` at `x = 1`.
#[must_use]
pub fn sigma(x: f64, b: f64) -> f64 {
    debug_assert!(b > 1.0);
    debug_assert!((0.0..=1.0).contains(&x));
    if x == 0.0 {
        return 0.0;
    }
    if x == 1.0 {
        return f64::INFINITY;
    }
    let mut sum = 0.0_f64;
    let mut x_bk = x;
    let mut b_km1 = 1.0_f64;
    loop {
        let prev = sum;
        x_bk = FloatOps::powf(x_bk, b);
        sum += x_bk * b_km1;
        if sum == prev {
            break;
        }
        b_km1 *= b;
    }
    x + (b - 1.0) * sum
}

/// `tau_b(x) = (1 - x) + (b - 1) * sum_{k>=1} (x^{b^-k} - 1) * b^-k`.
///
/// Returns `0` at `x = 0` and `x = 1` (the formula is zero at both endpoints by inspection).
#[must_use]
pub fn tau(x: f64, b: f64) -> f64 {
    debug_assert!(b > 1.0);
    debug_assert!((0.0..=1.0).contains(&x));
    if x == 0.0 || x == 1.0 {
        return 0.0;
    }
    let b_inv = 1.0 / b;
    let mut sum = 0.0_f64;
    let mut x_bmk = x;
    let mut b_mk = b_inv;
    loop {
        let prev = sum;
        sum += (x_bmk - 1.0) * b_mk;
        if sum == prev {
            break;
        }
        x_bmk = FloatOps::powf(x_bmk, b_inv);
        b_mk *= b_inv;
    }
    (1.0 - x) + (b - 1.0) * sum
}

/// `x / (e^x - 1)`, with the limit `1` at `x = 0` (the C++ `xDivExpm1`).
#[inline]
#[must_use]
pub fn x_div_expm1(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        x / FloatOps::exp_m1(x)
    }
}
