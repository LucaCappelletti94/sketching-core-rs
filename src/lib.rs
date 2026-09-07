#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

mod bits;
mod constants;
mod correction;
pub mod estimator;
mod hasher;
mod matrix;
mod number;
mod optimize;
mod packed;
mod precisions;
mod random;
mod shape;
mod sketches;
pub mod sparse_value_list;
mod variable_word;

pub use bits::*;
pub use constants::{One, Zero};
pub use correction::{sigma, tau, x_div_expm1};
pub use estimator::CardinalityEstimator;
pub use hasher::{DefaultBuildHasher, DefaultHasher, HasherType};
pub use matrix::Matrix;
pub use number::{FloatOps, Number, PositiveInteger};
pub use optimize::bisect_root;
pub use packed::{
    extract_bridge_value_from_word, extract_bridge_value_from_words, extract_value_from_word,
    extract_value_from_words, insert_bridge_value_into_word, insert_value_into_word,
    split_packed_index, Packed, PackedIter,
};
pub use precisions::*;
pub use random::{iter_random_values, iter_var_len_random_values, splitmix64, SplitMix64};
pub use shape::{PackedShape, Words};
pub use sketches::{
    inclusion_exclusion_joint_sketch, HyperSpheresSketch, JointSketch, JointSketchError,
};
pub use sparse_value_list::{
    contains_value, for_each_union_value, insert_value, merge_metrics, merge_write,
    read_fixed_bits, union_count, write_fixed_bits, ListSlice, SigBitsCode, SparseValueCodec,
    ValueInsertion, ValueIter,
};
pub use variable_word::VariableWord;

/// Re-exports of the most important traits and structs.
pub mod prelude {
    pub use crate::bits::*;
    pub use crate::constants::{One, Zero};
    pub use crate::correction::{sigma, tau, x_div_expm1};
    pub use crate::estimator::CardinalityEstimator;
    pub use crate::hasher::{DefaultBuildHasher, DefaultHasher, HasherType};
    pub use crate::matrix::Matrix;
    pub use crate::number::{FloatOps, Number, PositiveInteger};
    pub use crate::optimize::bisect_root;
    pub use crate::packed::{
        extract_bridge_value_from_word, extract_bridge_value_from_words, extract_value_from_word,
        extract_value_from_words, insert_bridge_value_into_word, insert_value_into_word,
        split_packed_index, Packed, PackedIter,
    };
    pub use crate::precisions::*;
    pub use crate::random::{
        iter_random_values, iter_var_len_random_values, splitmix64, SplitMix64,
    };
    pub use crate::shape::{PackedShape, Words};
    pub use crate::sketches::{
        inclusion_exclusion_joint_sketch, HyperSpheresSketch, JointSketch, JointSketchError,
    };
    pub use crate::sparse_value_list::{
        contains_value, for_each_union_value, insert_value, merge_metrics, merge_write,
        read_fixed_bits, union_count, write_fixed_bits, ListSlice, SigBitsCode, ValueInsertion,
        ValueIter,
    };
    pub use crate::variable_word::VariableWord;
}
