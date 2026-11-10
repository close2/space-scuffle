//! Unit radii shared by the `sim_map` integration tests.
//!
//! # Responsibility
//! Names the three body sizes the route tests exercise, in world units. Routes take each unit's
//! own radius (plan `2026-11-09-sc2-pathfinding`, D-3), so these are sample sizes, not tiers:
//! the smallest infantry body, a standard combat body and the largest ground body.
//!
//! # How it fits
//! Each test binary includes this file with `#[path = "support/radii.rs"] mod radii;` and uses
//! the subset it needs.
//!
//! # Invariants
//! - The values are exact in Q32.32 (multiples of 1/8), so a test's arithmetic on them is exact.

#![allow(
    dead_code,
    reason = "each test binary uses the subset of radii it needs"
)]

use sim_types::SimScalar;

/// Radius 0.375: light infantry; fits a gap one cell wide.
pub const SMALL_RADIUS: SimScalar = SimScalar::from_raw(3 << 29);
/// Radius 0.625: a standard combat unit; needs a gap two cells wide.
pub const MEDIUM_RADIUS: SimScalar = SimScalar::from_raw(5 << 29);
/// Radius 1.0: the largest ground units; fits exactly in a gap two cells wide.
pub const LARGE_RADIUS: SimScalar = SimScalar::ONE;
/// The three sample radii, smallest first.
pub const ALL_RADII: [SimScalar; 3] = [SMALL_RADIUS, MEDIUM_RADIUS, LARGE_RADIUS];

/// Clearance in eighths of a world unit that a disc of `radius` needs, rounded up: what
/// [`sim_map::Pathfinder::clearance_at`] must report for the disc to stand somewhere in a cell.
#[must_use]
pub fn required_clearance(radius: SimScalar) -> u8 {
    // One eighth of a world unit is 2^29 in Q32.32 raw units; round the quotient up.
    let eighths = (radius.raw() + (1 << 29) - 1) >> 29;
    u8::try_from(eighths).unwrap_or(u8::MAX)
}
