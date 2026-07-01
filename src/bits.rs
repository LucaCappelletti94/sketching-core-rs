//! Submodule providing the trait marker Bits.
use crate::VariableWord;
use core::{fmt::Debug, hash::Hash};

/// Trait marker for the number of bits.
pub trait Bits: VariableWord + Hash {}

/// Implementation for Bits4..=Bits8 with `type Word = u8`.
macro_rules! impl_bits_u8 {
    ($($n: expr),*) => {
        $(
            paste::paste! {
                #[non_exhaustive]
                #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
                #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
                /// A struct representing the number of bits.
                pub struct [<Bits $n>];

                impl VariableWord for [<Bits $n>] {
                    const NUMBER_OF_BITS: u8 = $n;
                    type Word = u8;
                    const MASK: u64 = (1 << $n) - 1;

                    #[inline]
                    #[allow(unsafe_code)]
                    #[allow(clippy::cast_possible_truncation)]
                    unsafe fn unchecked_from_u64(value: u64) -> Self::Word {
                        debug_assert!(value <= <Self as VariableWord>::MASK, "The value is too large for the number.");
                        value as u8
                    }
                }

                impl Bits for [<Bits $n>] {}
            }
        )*
    };
}

/// Implementation for Bits9..=Bits16 with `type Word = u16`.
macro_rules! impl_bits_u16 {
    ($($n: expr),*) => {
        $(
            paste::paste! {
                #[non_exhaustive]
                #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
                #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
                /// A struct representing the number of bits.
                pub struct [<Bits $n>];

                impl VariableWord for [<Bits $n>] {
                    const NUMBER_OF_BITS: u8 = $n;
                    type Word = u16;
                    const MASK: u64 = (1 << $n) - 1;

                    #[inline]
                    #[allow(unsafe_code)]
                    #[allow(clippy::cast_possible_truncation)]
                    unsafe fn unchecked_from_u64(value: u64) -> Self::Word {
                        debug_assert!(value <= <Self as VariableWord>::MASK, "The value is too large for the number.");
                        value as u16
                    }
                }

                impl Bits for [<Bits $n>] {}
            }
        )*
    };
}

impl_bits_u8!(4, 5, 6, 7, 8);
impl_bits_u16!(9, 10, 11, 12, 13, 14, 15, 16);
