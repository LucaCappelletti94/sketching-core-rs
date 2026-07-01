//! Sketch-family hasher marker trait.

use core::hash::Hasher;

/// Marker for hashers usable inside sketch counters: default-constructible, `Send + Sync`, `Clone`.
pub trait HasherType: Default + Hasher + Send + Sync + Clone {}

impl<T> HasherType for T where T: Default + Hasher + Send + Sync + Clone {}
