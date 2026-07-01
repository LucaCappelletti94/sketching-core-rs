//! Shape trait mapping `(Precision, Bits)` to the concrete `u64`-backed register storage.
//!
//! [`Words<N>`] is a newtype around `[u64; N]` with a manual `Default` impl. The stdlib provides
//! `Default for [T; N]` only for `N <= 32`, while [`PackedShape::Words`] for realistic
//! `(Precision, Bits)` pairs needs the full range up to `[u64; 16384]` at `Precision16, Bits16`.
//! Downstream traits (`setsketch::storage::PackedRegister`, `hyperloglog-rs::PackedRegister`,
//! `hyperlogloglog-rs::PackedRegister`) extend [`PackedShape`] with their own additional associated
//! types (permutation slots, `Vec` backing, and so on).

use crate::{Bits, Precision};
use core::fmt::Debug;

/// Fixed-size `[u64; N]` newtype with a manual `Default` impl for arbitrary `N`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Words<const N: usize>(pub [u64; N]);

impl<const N: usize> Default for Words<N> {
    #[inline]
    fn default() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> AsRef<[u64]> for Words<N> {
    #[inline]
    fn as_ref(&self) -> &[u64] {
        &self.0
    }
}

impl<const N: usize> AsMut<[u64]> for Words<N> {
    #[inline]
    fn as_mut(&mut self) -> &mut [u64] {
        &mut self.0
    }
}

/// Shape trait mapping `(Precision, Bits)` to the concrete `u64`-backed register storage.
pub trait PackedShape<B: Bits>: Precision {
    /// Register-storage word array. `N = ceil((2^EXPONENT * B::NUMBER_OF_BITS) / 64)`.
    type Words: AsRef<[u64]> + AsMut<[u64]> + Default + Clone + Eq + Debug + Send + Sync;
}

/// Implements [`PackedShape`] for a single `(Precision, Bits)` pair. Called by
/// [`crate::impl_packed_shape_for_precision`] and by downstream extension-trait macros.
#[macro_export]
macro_rules! impl_packed_shape_pair {
    ($precision:ident, $exp:expr, $bits_ty:ident, $bits:expr) => {
        impl $crate::PackedShape<$crate::$bits_ty> for $crate::$precision {
            type Words = $crate::Words<{ ((1usize << $exp) * $bits).div_ceil(64) }>;
        }
    };
}

/// Implements [`PackedShape`] for a single `Precision` across every supported `Bits`.
#[macro_export]
macro_rules! impl_packed_shape_for_precision {
    ($precision:ident, $exp:expr) => {
        $crate::impl_packed_shape_pair!($precision, $exp, Bits3, 3);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits4, 4);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits5, 5);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits6, 6);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits7, 7);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits8, 8);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits9, 9);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits10, 10);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits11, 11);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits12, 12);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits13, 13);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits14, 14);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits15, 15);
        $crate::impl_packed_shape_pair!($precision, $exp, Bits16, 16);
    };
}

impl_packed_shape_for_precision!(Precision4, 4);
impl_packed_shape_for_precision!(Precision5, 5);
impl_packed_shape_for_precision!(Precision6, 6);
impl_packed_shape_for_precision!(Precision7, 7);
impl_packed_shape_for_precision!(Precision8, 8);
impl_packed_shape_for_precision!(Precision9, 9);
impl_packed_shape_for_precision!(Precision10, 10);
impl_packed_shape_for_precision!(Precision11, 11);
impl_packed_shape_for_precision!(Precision12, 12);
impl_packed_shape_for_precision!(Precision13, 13);
impl_packed_shape_for_precision!(Precision14, 14);
impl_packed_shape_for_precision!(Precision15, 15);
impl_packed_shape_for_precision!(Precision16, 16);
impl_packed_shape_for_precision!(Precision17, 17);
impl_packed_shape_for_precision!(Precision18, 18);
