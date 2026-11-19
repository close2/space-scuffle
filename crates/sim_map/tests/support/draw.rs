//! Seeded draws that produce the same corpus on every target, shared by the `sim_map` tests.
//!
//! # Responsibility
//! `rand` samples a `usize` range by the pointer width: one 64-bit word on a 64-bit target, one
//! 32-bit word on `wasm32`. A corpus that draws an index as `usize` is therefore a different
//! corpus on each, and a digest pinned on one cannot match the other (bug B-140). This module
//! draws an index as a `u32` and converts it after the draw, so the corpus is a pure function of
//! the seed on every target.
//!
//! # How it fits
//! Each test binary that draws indices includes this file with
//! `#[path = "support/draw.rs"] mod draw;`. Charter check C25 requires every `gen_range` call in
//! the simulation crates to name a fixed-width type with a turbofish, so a `usize` draw cannot
//! return unnoticed.
//!
//! # Invariants
//! - [`draw_index`] consumes exactly the words a `u32` range draw consumes, on every target.

#![allow(
    clippy::expect_used,
    reason = "each `expect` states the invariant that holds for a test corpus"
)]

use rand::Rng;

/// A uniformly drawn index in `0..len`, sampled as a `u32` range so every target draws the same.
///
/// `len` is a collection length and must be positive and below 2³²; both hold for every corpus
/// collection of these tests.
pub fn draw_index<R: Rng>(rng: &mut R, len: usize) -> usize {
    let bound =
        u32::try_from(len).expect("invariant: corpus collections hold fewer than 2^32 items");
    usize::try_from(rng.gen_range::<u32, _>(0..bound))
        .expect("invariant: a u32 index fits a usize on every supported target")
}
