//! Constants for common values.

/// The zero value for this type.
pub trait Zero {
    /// The zero value for this type.
    const ZERO: Self;
    /// Whether the value is zero.
    fn is_zero(&self) -> bool;
}

/// The one value for this type.
pub trait One {
    /// The one value for this type.
    const ONE: Self;
    /// Whether the value is one.
    fn is_one(&self) -> bool;
}

/// Macro implementing several constants for integers.
macro_rules! impl_constants {
    ($($t:ty)*) => ($(
        impl One for $t {
            const ONE: Self = 1;
            #[inline]
            fn is_one(&self) -> bool { *self == 1 }
        }
        impl Zero for $t {
            const ZERO: Self = 0;
            #[inline]
            fn is_zero(&self) -> bool { *self == 0 }
        }
    )*)
}

impl_constants! { u8 u16 u32 u64 usize }
impl_constants! { i32 }

impl One for f64 {
    const ONE: Self = 1.0;
    #[inline]
    fn is_one(&self) -> bool {
        let delta = *self - 1.0;
        if delta < 0.0 {
            delta > -f64::EPSILON
        } else {
            delta < f64::EPSILON
        }
    }
}
impl Zero for f64 {
    const ZERO: Self = 0.0;
    #[inline]
    fn is_zero(&self) -> bool {
        *self == 0.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_zero() {
        assert!(0_u8.is_zero());
        assert!(!1_u8.is_zero());
        assert!(!255_u8.is_zero());
        assert!(0_u16.is_zero());
        assert!(!1_u16.is_zero());
        assert!(0_u32.is_zero());
        assert!(!1_u32.is_zero());
        assert!(0_u64.is_zero());
        assert!(!1_u64.is_zero());
        assert!(0_usize.is_zero());
        assert!(!1_usize.is_zero());
        assert!(0_i32.is_zero());
        assert!(!1_i32.is_zero());
        assert!(!(-1_i32).is_zero());
    }

    #[test]
    fn test_integer_one() {
        assert!(1_u8.is_one());
        assert!(!0_u8.is_one());
        assert!(!2_u8.is_one());
        assert!(1_u16.is_one());
        assert!(!0_u16.is_one());
        assert!(1_u32.is_one());
        assert!(!0_u32.is_one());
        assert!(1_u64.is_one());
        assert!(!0_u64.is_one());
        assert!(1_usize.is_one());
        assert!(!0_usize.is_one());
        assert!(1_i32.is_one());
        assert!(!0_i32.is_one());
        assert!(!(-1_i32).is_one());
    }

    #[test]
    fn test_f64_zero() {
        assert!(0.0_f64.is_zero());
        assert!(!1.0_f64.is_zero());
        assert!(!(-1.0_f64).is_zero());
        assert!(!f64::EPSILON.is_zero());
        assert!(!f64::MIN_POSITIVE.is_zero());
    }

    #[test]
    fn test_f64_one() {
        assert!(1.0_f64.is_one());
        assert!(!0.0_f64.is_one());
        assert!(!2.0_f64.is_one());
        assert!(!(-1.0_f64).is_one());
        // Near-one values within epsilon
        assert!((1.0 + f64::EPSILON * 0.5).is_one());
        assert!((1.0 - f64::EPSILON * 0.5).is_one());
        // Just outside epsilon
        assert!(!(1.0 + f64::EPSILON * 2.0).is_one());
        assert!(!(1.0 - f64::EPSILON * 2.0).is_one());
    }

    #[test]
    fn test_zero_const() {
        assert_eq!(<u8 as Zero>::ZERO, 0);
        assert_eq!(<u16 as Zero>::ZERO, 0);
        assert_eq!(<u32 as Zero>::ZERO, 0);
        assert_eq!(<u64 as Zero>::ZERO, 0);
        assert_eq!(<usize as Zero>::ZERO, 0);
        assert_eq!(<f64 as Zero>::ZERO, 0.0);
    }

    #[test]
    fn test_one_const() {
        assert_eq!(<u8 as One>::ONE, 1);
        assert_eq!(<u16 as One>::ONE, 1);
        assert_eq!(<u32 as One>::ONE, 1);
        assert_eq!(<u64 as One>::ONE, 1);
        assert_eq!(<usize as One>::ONE, 1);
        assert_eq!(<f64 as One>::ONE, 1.0);
    }
    #[test]
    fn test_f64_one_epsilon_boundaries() {
        // Value exactly at positive epsilon boundary
        let above = 1.0 + f64::EPSILON;
        assert!(!above.is_one(), "1.0 + EPSILON should not be one");

        // Value exactly at negative epsilon boundary
        let below = 1.0 - f64::EPSILON;
        assert!(!below.is_one(), "1.0 - EPSILON should not be one");

        // Value just inside positive boundary
        let just_inside_pos = 1.0 + (f64::EPSILON / 2.0);
        assert!(just_inside_pos.is_one(), "1.0 + EPSILON/2 should be one");

        // Value just inside negative boundary
        let just_inside_neg = 1.0 - (f64::EPSILON / 2.0);
        assert!(just_inside_neg.is_one(), "1.0 - EPSILON/2 should be one");

        // Values far from one
        assert!(!2.0_f64.is_one());
        assert!(!0.0_f64.is_one());
        assert!(!(-1.0_f64).is_one());
    }
}
