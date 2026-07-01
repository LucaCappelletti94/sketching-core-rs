//! Numerical optimizers shared across sketch estimators.

/// Bisection root finder on a monotone function `f` over `[a, b]` with `f(a)` and `f(b)` of
/// opposite sign. Caps at 200 iterations; converges in about `log2((b - a) / (rel_tol * scale))`
/// iterations in practice.
#[must_use]
pub fn bisect_root<F: Fn(f64) -> f64>(f: F, mut a: f64, mut b: f64, rel_tol: f64) -> f64 {
    let mut fa = f(a);
    if fa == 0.0 {
        return a;
    }
    let mut fb = f(b);
    if fb == 0.0 {
        return b;
    }
    debug_assert!(
        (fa < 0.0) != (fb < 0.0),
        "bisect_root needs a sign change in the bracket"
    );
    for _ in 0..200 {
        let width = b - a;
        let scale = a.abs().max(b.abs()).max(1.0);
        if width.abs() <= rel_tol * scale {
            return 0.5 * (a + b);
        }
        let mid = 0.5 * (a + b);
        let fmid = f(mid);
        if fmid == 0.0 {
            return mid;
        }
        if (fa < 0.0) == (fmid < 0.0) {
            a = mid;
            fa = fmid;
        } else {
            b = mid;
            fb = fmid;
        }
        // Bail-out if the bracket has collapsed below f64 ULP.
        let _ = fb;
        if a == b {
            return a;
        }
    }
    0.5 * (a + b)
}
