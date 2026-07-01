# sketching-core

[![Rust CI](https://github.com/LucaCappelletti94/sketching-core-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/LucaCappelletti94/sketching-core-rs/actions/workflows/rust.yml)
[![Crates.io](https://img.shields.io/crates/v/sketching-core.svg)](https://crates.io/crates/sketching-core)
[![docs.rs](https://img.shields.io/docsrs/sketching-core)](https://docs.rs/sketching-core)
[![License: MIT](https://img.shields.io/crates/l/sketching-core)](https://github.com/LucaCappelletti94/sketching-core-rs/blob/main/LICENSE)

Core primitives for probabilistic cardinality sketches like HyperLogLog and SetSketch. Provides `no_std`-compatible packed register arrays, cardinality estimation traits with inclusion-exclusion defaults, joint sketching for overlap estimation, precision types from 4 to 18 bits, and floating-point math primitives.
