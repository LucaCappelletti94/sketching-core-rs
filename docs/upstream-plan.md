# Sketching-core upstream plan

Working plan for identifying shared abstractions across the three consumer crates (`setsketch`, `hyperloglog-rs`, `hyperlogloglog-rs`) and lifting them into `sketching-core`. Each phase is atomic across all four crates. Every phase leaves every crate green. Phase ordering respects dependencies. Risk is separated by whether the move breaks any consumer's public API.

This document lives in `sketching-core-rs` because that is where the moves land. Every phase's completion updates this document with the resulting `sketching-core` version tag and the git rev pinned in each consumer.

## Repositories

The four repos are worked concurrently. All are single-author, single-branch (`main`), and today all consumers path-depend or git-depend on `sketching-core-rs`.

- `sketching-core-rs` at `~/github/sketching-core-rs`. Upstream target for every move.
- `setsketch` at `~/github/setsketch`. Depends on `sketching-core` via `git + rev` pin (see `Cargo.toml` line 16).
- `hyperloglog-rs` at `~/github/hyperloglog-rs`. Depends on `sketching-core` via path today.
- `hyperlogloglog-rs` at `~/github/hyperlogloglog-rs`. Depends on `sketching-core` via path today.

## Phase 0: Housekeeping in consumers

Purpose. Delete dead code and pre-1.53-stdlib workarounds so later phases anchor on a clean base. Nothing moves upstream. `sketching-core` is not touched. No version bump.

### 0.1 Delete dead files in `hyperloglog-rs`

`hyperloglog-rs`'s `src/utils.rs` re-exports `Number`, `One`, `PositiveInteger`, `Zero`, `FloatOps`, `iter_random_values`, `iter_var_len_random_values`, `splitmix64` from `sketching_core` (lines 13-16). The six files below still exist under `hyperloglog-rs/src/` but are declared as modules by nothing (verified by `grep -rE '^mod (bits|precisions|constants|matrix|number|variable_word)' src/*.rs` returning only `utils.rs: mod hasher_type`). Cargo never compiles them.

Files to delete outright (roughly 700 lines):
- `hyperloglog-rs/src/bits.rs`
- `hyperloglog-rs/src/precisions.rs`
- `hyperloglog-rs/src/utils/constants.rs`
- `hyperloglog-rs/src/utils/matrix.rs`
- `hyperloglog-rs/src/utils/number.rs`
- `hyperloglog-rs/src/utils/variable_word.rs`

Verification. `cargo check --workspace --all-targets --all-features` passes on `hyperloglog-rs`. `cargo test --workspace --all-features` passes. Nothing else changes.

Commit message: `Delete stale pre-migration source files replaced by sketching-core re-exports`.

### 0.2 Confirm `hyperlogloglog-rs` has no analogous stale files

Grep confirms only three live modules (`estimator`, `hyperlogloglog`, `sparse`) and no stale extras. No change unless the survey turns up something. Verification: `cargo check` on `hyperlogloglog-rs` remains green.

### 0.3 Delete `Words<N>` and `Slots<N>` newtypes in `setsketch`

`setsketch/src/storage.rs` defines `Words<const N: usize>(pub [u64; N])` and `Slots<const N: usize>(pub [(u32, u32); N])` with manual `Default` impls. The comment says these exist because "the stdlib impl only covers `N <= 32`". Rust stdlib since 1.53 (May 2021) provides `impl<T: Default, const N: usize> Default for [T; N]` for arbitrary `N` when `T: Default`. Verified in practice by `sketching-core::packed::Packed<[u64; N], V>::default()` at `sketching-core-rs/src/packed.rs:306`, which already uses `[0; N]` directly for arbitrary N.

Refactor `setsketch/src/storage.rs::PackedRegister` associated types to `type Words = [u64; ...]` and `type PermutationSlots = [(u32, u32); ...]` directly. Delete the newtype declarations and the `AsRef`/`AsMut`/`Default` impls (roughly 60 lines).

Verification. Full CI-mirror gate on `setsketch` (fmt, clippy, tests, doc, deny, audit, actionlint). The refactor is internal, no public API changes.

Commit message on `setsketch`: `Drop Words and Slots newtypes now that stdlib covers Default for arbitrary N`.

### Phase 0 exit criteria

- `hyperloglog-rs`, `hyperlogloglog-rs`, `setsketch` all pass their full test suites on the changed branches.
- `sketching-core-rs` is untouched.
- No version bumps.

## Phase 1: Trivial upstream moves

Purpose. Lift the strictly additive primitives that appear verbatim (or nearly verbatim) in multiple consumers. These are the lowest-risk moves and set up Phase 2. `sketching-core` gets a `0.2.0` tag at the end of this phase.

### 1.1 `HasherType` marker trait

`hyperloglog-rs/src/utils/hasher_type.rs` and `hyperlogloglog-rs/src/hyperlogloglog.rs:27-29` both define `pub trait HasherType: Default + Hasher + Send + Sync + Clone {}` with a blanket impl (`hyperlogloglog-rs` uses the alias `StdHasher` but the trait is otherwise identical).

Move to `sketching-core-rs/src/hasher.rs` (new file):

```rust
//! Sketch-family hasher marker trait.

use core::hash::Hasher;

/// Marker for hashers usable inside sketch counters: default-constructible, `Send + Sync`, `Clone`.
pub trait HasherType: Default + Hasher + Send + Sync + Clone {}

impl<T> HasherType for T where T: Default + Hasher + Send + Sync + Clone {}
```

Wire into `sketching-core-rs/src/lib.rs`:
- `mod hasher;`
- `pub use hasher::HasherType;`
- Add `HasherType` to the `prelude` re-export list.

Consumer refactor:
- `hyperloglog-rs`: delete `src/utils/hasher_type.rs`, drop `mod hasher_type` and the `pub use hasher_type::HasherType` line from `src/utils.rs`, add `HasherType` to the existing `pub use sketching_core::` line in `src/utils.rs`.
- `hyperlogloglog-rs`: delete the trait + blanket impl from `src/hyperlogloglog.rs`, add `pub use sketching_core::HasherType` at the top of the module. Update the `HasherType` re-export in `src/lib.rs`.
- `setsketch`: no change (setsketch uses `H: BuildHasher + Default + Clone + Debug` directly today and does not depend on `HasherType`).

### 1.2 Per-single-word packed helpers

`sketching-core-rs/src/packed.rs:37-100` defines four private helpers: `extract_value_from_word`, `insert_value_into_word`, `extract_bridge_value_from_word`, `insert_bridge_value_into_word`. `hyperloglog-rs/src/registers/packed_array.rs:25-100` re-implements the same four functions verbatim to satisfy its `Registers` impl.

Change in `sketching-core`:
- Make the four functions `pub`.
- Add them to `pub use packed::{...}` in `src/lib.rs` and to the `prelude`.

Consumer refactor:
- `hyperloglog-rs`: delete the four re-implementations in `src/registers/packed_array.rs:25-100`, replace with `use sketching_core::{extract_value_from_word, insert_value_into_word, extract_bridge_value_from_word, insert_bridge_value_into_word};`.
- `setsketch`: no change (does not use these primitives today).
- `hyperlogloglog-rs`: no change.

### 1.3 Ertl numerical primitives: `sigma`, `tau`, `x_div_expm1`

`setsketch/src/estimator.rs` defines all three (lines 24, 52, 77) as generic-in-`b` series evaluators. `hyperloglog-rs` today uses precomputed correction polynomials instead of `sigma`/`tau`; `hyperlogloglog-rs` uses the classical HLL correction without sigma/tau. Only `setsketch` uses them today, but the primitives belong to `sketching-core::estimator` because they are Ertl's HLL-family definitions, and Phase 3 can hook `hyperloglog-rs` onto them if the accuracy tradeoff is preferable.

Move to `sketching-core-rs/src/estimator.rs` (or a new `sketching-core-rs/src/correction.rs` if we want a dedicated home):

```rust
pub fn sigma(x: f64, b: f64) -> f64 { ... }
pub fn tau(x: f64, b: f64) -> f64 { ... }
pub fn x_div_expm1(x: f64) -> f64 { ... }
```

Consumer refactor:
- `setsketch`: replace bodies in `src/estimator.rs` with `pub use sketching_core::{sigma, tau, x_div_expm1};` so downstream public API (`setsketch::sigma`, `setsketch::tau`) is unchanged.
- Other consumers: no change.

### 1.4 Generic bisection root finder

`setsketch/src/estimator.rs:89-127` defines `pub fn bisect_root<F: Fn(f64) -> f64>(f: F, a: f64, b: f64, rel_tol: f64) -> f64`. No other consumer uses it today; moving it upstream anticipates the shared-optimizer story once `hyperloglog-rs` or others need one.

Move to `sketching-core-rs/src/optimize.rs` (new file):

```rust
pub fn bisect_root<F: Fn(f64) -> f64>(f: F, a: f64, b: f64, rel_tol: f64) -> f64 { ... }
```

Consumer refactor. `setsketch::estimator::bisect_root` becomes a `pub use sketching_core::bisect_root;` re-export. Callers unchanged.

### Phase 1 exit criteria

- `sketching-core` tagged `v0.2.0`.
- All three consumers' `Cargo.toml` git-rev pin bumped to the new SHA in one commit per consumer.
- All three consumers pass their full CI-mirror gate at the new pin.
- Every `pub` symbol removed from a consumer file is preserved as a `pub use` re-export at the same path.

Version bump on `sketching-core`. `Cargo.toml` `version = "0.1.0"` becomes `0.2.0`. Tag `v0.2.0`. Push. Consumers update pins.

## Phase 2: Unified `PackedRegister<B>` shape

Purpose. Collapse the three independent declarations of `pub trait PackedRegister<B: Bits>: Precision` into a shared base in `sketching-core`. This is the one structurally interesting move and the reason to do it BEFORE `setsketch` v0.1.0 hits crates.io: the trait shape becomes stable for the public API.

### 2.1 Design

Introduce in `sketching-core-rs/src/packed.rs`:

```rust
/// Shape trait mapping `(Precision, Bits)` to the concrete `u64`-backed register storage.
/// Downstream traits (e.g. `setsketch::PackedRegister`, `hyperloglog-rs::PackedRegister`) extend
/// this with their own additional associated types (permutation slots, `Vec` backing, etc.).
pub trait PackedShape<B: Bits>: Precision {
    /// Register-storage word array. `N = ceil((2^EXPONENT * B::NUMBER_OF_BITS) / 64)`.
    type Words: AsRef<[u64]> + AsMut<[u64]> + Default + Clone + Eq + Debug + Send + Sync;
}
```

Provide the two macros that all three consumers duplicate:

```rust
#[macro_export]
macro_rules! impl_packed_shape_pair { ($precision:ident, $exp:expr, $bits_ty:ident, $bits:expr) => { ... } }

#[macro_export]
macro_rules! impl_packed_shape_for_precision { ($precision:ident, $exp:expr) => {
    $crate::impl_packed_shape_pair!($precision, $exp, Bits4, 4);
    $crate::impl_packed_shape_pair!($precision, $exp, Bits5, 5);
    ... Bits6 through Bits16 ...
}; }
```

Invoke the macro over `Precision4..=Precision18` inside `sketching-core`.

### 2.2 Consumer refactor

`setsketch/src/storage.rs`:

```rust
pub trait PackedRegister<B: Bits>: PackedShape<B> {
    type PermutationSlots: AsRef<[(u32, u32)]> + AsMut<[(u32, u32)]> + Default + Clone + Eq + Debug + Send + Sync;
}
```

Delete the local `impl_packed_register_pair!` and `impl_packed_register_for_precision!` macros. Keep only the tiny `impl PackedRegister<Bits...> for Precision...` blocks that supply `type PermutationSlots`.

`hyperloglog-rs/src/registers/packed_array.rs`:

```rust
pub trait PackedRegister<B: Bits>: PackedShape<B> {
    type Array: Registers<Self, B>;
    #[cfg(feature = "alloc")]
    type Vec: Registers<Self, B>;
}
```

Delete the local `impl_packed_array_register_for_precision_and_bits!` and `impl_registers_for_precisions!` macros' per-`(P, B)` walk, keep only the per-`(P, B)` `Array` / `Vec` associations. Note: `hyperloglog-rs` currently only supplies `Bits4`, `Bits5`, `Bits6` (line 277 of the macro), not the full `Bits4..=Bits16` range. `PackedShape` in `sketching-core` provides all thirteen, but `hyperloglog-rs`'s `PackedRegister` still only extends for the three it needs. This is fine because `PackedShape` is a supertrait; not every `PackedShape<B>` needs a downstream `PackedRegister<B>`.

`hyperlogloglog-rs`: today has no `PackedRegister` trait at all; per-precision structs inline `Packed<[u64; $words], Bits3>` directly through `impl_hlll!`. Refactor to define its own `PackedRegister<Bits3>` extension trait (or use `PackedShape<Bits3>` directly, no extension needed) so it stops hardcoding `[u64; $words]` array sizes and pulls them from the `PackedShape::Words` associated type. This unifies the pattern across all three consumers.

### 2.3 Blast radius

- `sketching-core`: +120 lines (trait + two exported macros + macro invocations over Precision4..=Precision18).
- `setsketch`: net -80 lines (drops the two macros' per-`(P, B)` walk, keeps a thin extension trait).
- `hyperloglog-rs`: net -60 lines (same pattern).
- `hyperlogloglog-rs`: net -40 lines (drops the per-precision `[u64; $words]` inline arrays in favour of `PackedShape<Bits3>::Words`).

### 2.4 Public-API semantics

`setsketch::storage::PackedRegister` gains a supertrait `PackedShape<B>`. Users writing bounds `where P: PackedRegister<B>` see no change. Users pattern-matching the trait's exact shape (rare) will need to also pull `PackedShape` into scope. Since we keep the same name and same module path, most call sites remain identical.

### 2.5 Phase 2 exit criteria

- `sketching-core` tagged `v0.3.0`.
- All three consumers' git-rev pins bumped, one commit each, all CI-mirror gates green.
- The three consumer-local `PackedRegister` traits each become a 4-to-6-line extension of `PackedShape`.
- No consumer duplicates the `Bits4..=Bits16` / `Precision4..=Precision18` walk anymore.

## Phase 3: Classical HLL cardinality estimator

Purpose. Lift the canonical harmonic-sum cardinality math into `sketching-core::estimator::classical`.

### 3.1 What moves

`hyperlogloglog-rs/src/estimator.rs` has:
- `alpha_m(m: u64) -> f64`: bias correction (0.673 at m=16, 0.697 at 32, 0.709 at 64, `0.7213 / (1 + 1.079/m)` above).
- `raw_estimate(m: u64, harmonic_sum: f64) -> f64`.
- `corrected_estimate(m: u64, harmonic_sum: f64, zeros: u64) -> f64`: linear counting below `5m/2`, saturation correction above `2^32/30`.
- `compute_harmonic_sum<F: FnMut(usize) -> u8>(m: usize, get: F) -> (f64, u64)`.
- `relative_standard_error<P: Precision>() -> f64`: `1.04 / sqrt(2^P::EXPONENT)`.
- `estimate_cardinality<P: Precision, F>(get: F) -> f64`: the full harmonic + correction pipeline.

`hyperloglog-rs`'s `src/estimator.rs` implements the same math folded into its per-mode dispatch with `correction_coefficients` polynomial fits. Not literally identical but occupies the same conceptual role.

Move to `sketching-core-rs/src/estimator/classical.rs`:

```rust
pub fn alpha_m(m: u64) -> f64 { ... }
pub fn raw_estimate(m: u64, harmonic_sum: f64) -> f64 { ... }
pub fn corrected_estimate(m: u64, harmonic_sum: f64, zeros: u64) -> f64 { ... }
pub fn relative_standard_error<P: Precision>() -> f64 { ... }
pub fn harmonic_sum_and_zeros<F: FnMut(usize) -> u8>(m: usize, get: F) -> (f64, u64) { ... }
pub fn estimate_cardinality<P: Precision, F: FnMut(usize) -> u8>(get: F) -> f64 { ... }
```

### 3.2 Consumer refactor

- `hyperlogloglog-rs`: delete `src/estimator.rs`, replace with `pub use sketching_core::estimator::classical::{...}` in `src/lib.rs`. The public API path `hyperlogloglog::estimate_cardinality` etc. remains identical via re-export.
- `hyperloglog-rs`: switch its adaptive estimator to consume `sketching_core::estimator::classical::{harmonic_sum_and_zeros, raw_estimate}` as building blocks. Keeps its own `correction_coefficients` polynomial fit; those stay a `hyperloglog-rs` concern because they are algorithm-specific to hll's precomputed table strategy.
- `setsketch`: no change. `setsketch::estimator::estimate_cardinality` is a separate function operating on SetSketch's non-HLL register semantics (base `b != 2`, offsets not `rho`).

### 3.3 Phase 3 exit criteria

- `sketching-core` tagged `v0.4.0`.
- `hyperlogloglog-rs::estimator` is empty save for re-exports.
- `hyperloglog-rs` compiles unchanged in behavior against the new pin; its precision correction polynomials still take precedence for the exact estimate value.
- All three consumers CI-mirror green.

## Phase 4: Optional shared primitives

Purpose. Move the primitives currently used by one consumer but generically useful to others. Non-blocking. Do only when a second consumer materializes or when the code is small enough that moving proactively is cheap.

### 4.1 `SplitMix64` stateful wrapper

`setsketch/src/utils/prng.rs` defines `SplitMix64` with the whitening XOR (`SEED_MIXER = 0xCBF2_9CE4_8422_2325`), `next_u64`, `next_f64` (uniform in `[0, 1)` from top 53 bits), `bounded_u32(n)` via Lemire, `next_exp1()` via `-ln_1p(-U)`.

Move to `sketching-core-rs/src/random.rs` (existing file):

```rust
pub struct SplitMix64 { state: u64 }
impl SplitMix64 { pub const fn new(hash: u64) -> Self { ... } pub fn next_u64(&mut self) -> u64 { ... } ... }
```

`setsketch::utils::SplitMix64` becomes a `pub use sketching_core::SplitMix64;` re-export.

### 4.2 `DefaultHasher` and `DefaultBuildHasher`

`setsketch/src/hash.rs` vendors a wyhash-style hasher (~120 lines, no dep). Move verbatim to `sketching-core-rs/src/hasher.rs` (extending the module introduced in Phase 1).

Once landed, `hyperloglog-rs` and `hyperlogloglog-rs` may switch their default from `twox_hash::XxHash64` to `sketching_core::DefaultHasher` and drop the `twox_hash` dependency. That switch is a separate decision per consumer and not part of this phase.

### 4.3 Phase 4 exit criteria

- `sketching-core` tagged `v0.5.0`.
- `setsketch` re-exports `SplitMix64` and `DefaultHasher` from upstream. No consumer public API changes.

## Phase 5: Sparse register storage (deferred)

Gated on `setsketch`'s sparse-mode hash-list backend design (deferred item from the pre-v0.1.0 handoff).

`hyperlogloglog-rs/src/sparse.rs::SparseRegisters<CAPACITY>` is a fixed-capacity sorted `[Option<SparseEntry>; CAPACITY]`. `hyperloglog-rs`'s sparse mode is different in shape (composite hash list with gap encoding), so this is not a natural three-way abstraction today.

When `setsketch` gains its sparse-mode backend, revisit whether:
- `sketching-core::sparse::SparseRegisters<CAPACITY>` is the right shape for both `hyperlogloglog-rs` and `setsketch`.
- Or the two want distinct backing arrays (`setsketch` may want raw `[u64; CAP]` for hashes, `hyperlogloglog-rs` wants `(index, value)` pairs).

No action until then.

## Not upstreamed

The following primitives were considered and rejected as upstream candidates today because they have exactly one consumer. Re-evaluate if a second consumer materializes.

- `setsketch::merge::RegisterFastMerge` (SIMD-friendly max on packed `u64` for widths that divide 64). Only `setsketch` needs it; `hyperloglog-rs` merges via its composite-hash mode logic and does not do per-`u64` word max.
- `setsketch::sampling::PermutationBuffer` (versioned Fisher-Yates). Only `setsketch` needs it; `hyperloglog-rs` indexes by hash bits.
- `setsketch::sampling::ExponentialSpacing`, `truncated_exp`. Only `setsketch` needs them.
- `setsketch::estimator::RangeCorrection` enum. Only `setsketch` uses it (its correction toggle).
- `hyperloglog-rs`'s composite-hash and mode-dispatch machinery. Algorithm-specific to `hyperloglog-rs`'s adaptive design.
- `hyperloglog-rs::correction_coefficients` polynomial tables. Algorithm-specific to hll's precomputed correction strategy.
- `hyperloglog-rs::mle` (damped Newton on disjoint-region MLE). Algorithm-specific.

## Ordering and timing

Phase 1 must precede Phase 2 because Phase 2 references `HasherType` when constraining consumer register types. Phase 3 is independent of Phase 2 but is cleaner to reason about after Phase 2. Phase 4 and Phase 5 are independent and can run in either order after Phase 3.

Pre-publish versus post-publish for `setsketch` v0.1.0:

- Phase 0 and Phase 1 are strictly non-breaking on `setsketch`'s public API. Land BEFORE v0.1.0.
- Phase 2 introduces a supertrait on `setsketch::storage::PackedRegister`. Land BEFORE v0.1.0 to avoid a future semver bump for the trait shape.
- Phase 3, 4, 5 are non-breaking on `setsketch`'s public API. Can happen either side of v0.1.0.

## Version discipline

Each phase is one atomic change per crate.

- `sketching-core` version bumps: `0.1.0` (current) -> `0.2.0` (Phase 1) -> `0.3.0` (Phase 2) -> `0.4.0` (Phase 3) -> `0.5.0` (Phase 4) -> `0.6.0` (Phase 5, if executed).
- Consumers pin `sketching-core` by git rev today. Each phase bumps the pin in one commit per consumer at the end of the phase.
- Once `sketching-core` publishes to crates.io, the pin becomes `version = "0.x.y"` instead of `rev = "..."`. The phase discipline stays.

## Blast radius summary

| Phase | sketching-core | setsketch | hyperloglog-rs | hyperlogloglog-rs |
|---|---|---|---|---|
| 0 | 0 | -60 | -700 | 0 |
| 1 | +250 | -30 (net; re-exports kept) | -100 | -10 |
| 2 | +120 | -80 | -60 | -40 |
| 3 | +180 | 0 | ~0 (structural, similar LOC) | -130 |
| 4 | +250 | ~0 (re-exports) | 0 (opt-in later) | 0 (opt-in later) |

Numbers are approximate and change with review.

## Verification protocol per phase

Between phases, on every touched crate, run the full CI-mirror gate:

```sh
export RUSTFLAGS='-D warnings' RUSTDOCFLAGS='-D warnings'
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo check --workspace --all-targets --features joint      # setsketch only
cargo check --workspace --no-default-features
cargo clippy --workspace --all-targets --features <feats> -- -D warnings
cargo clippy --workspace --no-default-features -- -D warnings
cargo test --workspace --features <feats>
cargo test --release --test statistical                     # setsketch only
cargo test --release --test prng_decorrelation              # setsketch only
cargo test --workspace --doc --features <feats>
cargo build --workspace --no-default-features
cargo doc --workspace --no-deps --features <feats>
cargo deny --all-features check
cargo audit --deny unmaintained --deny yanked --ignore RUSTSEC-2024-0436
```

For `sketching-core` itself: fmt, check, clippy, test, doc.

## Log

Fill in as phases execute.

| Phase | Date | `sketching-core` tag | `setsketch` pin | `hyperloglog-rs` pin | `hyperlogloglog-rs` pin | Notes |
|---|---|---|---|---|---|---|
| 0 | 2026-07-01 | (unchanged) | (unchanged) | `hyperloglog-rs` 62291f3 | (unchanged) | 0.1 deleted six stale files. 0.2 no-op (no stale files). 0.3 redirected: stdlib `Default for [T; N]` is still N ≤ 32 on rustc 1.98 nightly. `Words<N>` moved upstream to `sketching-core` in phase 2.1 (see below) rather than deleted, so `setsketch` still loses its local newtype. `Slots<N>` stays in `setsketch` as a local `PermutationSlots` concern. |
| 1 | 2026-07-01 | v0.2.0 (c38a2f75) | c38a2f75 | c38a2f75 | c38a2f75 | Landed as one sketching-core commit ffc875e (four items combined). Consumers switched to `git + rev` pins with a local `[patch]` for dev. |
| 2 | 2026-07-01 | v0.3.0 (5f554bf) | 5f554bf | 5f554bf | 5f554bf | Introduced `PackedShape` trait plus `Words<N>` newtype in `sketching-core`. Consolidated `Packed<W, V>` `Default` to one generic impl `W: Default`. Consumers extend `PackedShape<B>` for their added associated types (`Array` and `Vec` in `hyperloglog-rs`, `PermutationSlots` in `setsketch`, direct use in `hyperlogloglog-rs`). |
| 3 | 2026-07-01 | v0.4.0 (cead42b) | cead42b | cead42b | cead42b | `hyperlogloglog-rs` `src/estimator.rs` deleted, re-exports the six primitives from `sketching_core::estimator::classical`. Preserved the user's `5m/32` -> `5m/2` linear-counting correction upstream. `hyperloglog-rs` pin bump only (its harmonic-sum machinery is NaN-boxed and adaptive; swapping the classical primitives in would require unwinding the NaN-boxing layer and give no measurable win). Upstream `harmonic_sum_and_zeros` uses `FloatOps::integer_exp2_minus` for register-to-`2^{-r}`, matching `hyperloglog-rs`'s hot path. |
| 4 | pending | v0.5.0 | | | | |
| 5 | pending | v0.6.0 | | | | |
