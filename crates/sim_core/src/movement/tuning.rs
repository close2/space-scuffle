//! Named movement constants: distances in world units, durations in 25 Hz ticks.
//!
//! This module owns every tuning value of arrival, steering, contact and stall detection, so a
//! rule reads by name at its use and is changed in one place. It holds no logic beyond the exact
//! rational constructor below.
//!
//! # Invariants
//! - Values are exact Q32.32 rationals fixed at compile time. Nothing here goes through a float,
//!   so the tick loop cannot pick up platform-dependent rounding from these constants.

use sim_types::{SimScalar, TICKS_PER_SECOND};

/// Exact `numerator / denominator` in Q32.32, rounded toward zero.
const fn ratio(numerator: i64, denominator: i64) -> SimScalar {
    SimScalar::from_raw((numerator << 32) / denominator)
}

/// Extra reach beyond touching at which a blocker starts to steer a unit.
pub(crate) const STEERING_LOOKAHEAD: SimScalar = ratio(35, 100);
/// Half-width added to the unit's corridor when deciding whether a blocker lies in its path.
pub(crate) const STEERING_CORRIDOR_MARGIN: SimScalar = ratio(5, 100);
/// Extra reach beyond touching at which a co-moving neighbour separates a unit.
pub(crate) const FLOCK_SHELL: SimScalar = ratio(8, 100);
/// Minimum cosine between a unit's heading and a neighbour's velocity to call them co-moving.
/// About 60 degrees: a unit crossing or opposing this one is an obstacle, not a flockmate.
pub(crate) const COMOVING_MIN_COSINE: SimScalar = ratio(1, 2);
/// Sideways steering weight applied by a touching flockmate.
pub(crate) const FLOCK_WEIGHT_TOUCHING: SimScalar = ratio(25, 100);
/// Sideways steering weight applied by a deeply overlapping flockmate.
pub(crate) const FLOCK_WEIGHT_OVERLAPPING: SimScalar = ratio(35, 100);
/// Sideways steering weight applied by an obstacle, about 40 degrees of deflection.
pub(crate) const OBSTACLE_WEIGHT: SimScalar = ratio(85, 100);
/// Fraction of touching distance below which a neighbour exerts its full weight.
pub(crate) const EMERGENCY_SEPARATION: SimScalar = ratio(85, 100);

/// How far from its witness's centre, beyond touching, a unit still counts as stopped by it.
pub(crate) const ARRIVAL_TOUCH_MARGIN: SimScalar = ratio(15, 100);
/// How far from the destination, beyond touching, a settled unit may stand and still stop others.
pub(crate) const ARRIVAL_WITNESS_REACH: SimScalar = ratio(3, 2);

/// Share of its step below which a unit blocked by bodies that will not yield waits instead of
/// moving: one eighth (plan `2026-11-09-friendly-blocking` D-1.3). A named tuning value of our own,
/// not a sourced one (U-23).
pub(crate) const WAIT_THRESHOLD: SimScalar = ratio(1, 8);

/// Ticks a surround walk's side must stay closed before the unit tries the other side, the
/// repath cooldown (plan `2026-11-09-friendly-blocking` D-2.4). A named tuning value of our own,
/// not a sourced one (U-23).
pub(crate) const FLANK_REVERSE_TICKS: u16 = 12;

/// Route length a unit must gain to count as progress.
pub(crate) const STALL_PROGRESS: SimScalar = ratio(1, 10);
/// Ticks without progress before a unit rebuilds its route, one second.
pub(crate) const STALL_REPATH_TICKS: u16 = TICKS_PER_SECOND;
/// Ticks without progress before a unit gives its order up, ten seconds.
pub(crate) const STALL_ABANDON_TICKS: u16 = 10 * TICKS_PER_SECOND;
/// Distance from the destination inside which a stalled unit on its last leg counts as arrived.
pub(crate) const STALL_ARRIVAL_REACH: SimScalar = SimScalar::from_raw(3 << 32);

/// Ticks a unit waits between route searches, half a second.
pub(crate) const REPATH_COOLDOWN_TICKS: u16 = 12;
/// A long route is re-checked this many ticks apart, spread across units by entity ID.
pub(crate) const ROUTE_CHECK_PERIOD: u32 = 8;
/// Distance a chased target may move from a route's end before the route is rebuilt.
pub(crate) const PURSUIT_ROUTE_DRIFT: SimScalar = SimScalar::from_raw(2 << 32);
/// Routes to an automatically chosen target may be this many times the unit's scan range.
pub(crate) const PURSUIT_LEASH_SCAN_RANGES: SimScalar = SimScalar::from_raw(2 << 32);
/// Ticks a unit ignores a target it gave up on, two seconds.
pub(crate) const ABANDONED_TARGET_TICKS: u16 = 2 * TICKS_PER_SECOND;

/// Waypoints are dropped once a unit is this far from them, beyond two of its radii, provided the
/// next waypoint is already in clear view: another unit may be parked on the waypoint.
pub(crate) const WAYPOINT_HANDOFF_MARGIN: SimScalar = ratio(1, 10);
/// A route's first waypoint within this distance of the unit is its own start point.
pub(crate) const ROUTE_START_TOLERANCE: SimScalar = ratio(1, 5);
/// How far ahead the terrain guard looks along a leg after local steering deflects a unit.
pub(crate) const TERRAIN_GUARD_HORIZON: SimScalar = SimScalar::from_raw(2 << 32);

/// Share of a dynamic pair's overlap resolved per tick, closing overlaps over about three ticks.
pub const PUSH_RELAXATION: SimScalar = ratio(35, 100);
/// Sideways share of the direction a unit slides along an immovable friendly body.
pub(crate) const FIXED_BODY_TANGENT_WEIGHT: SimScalar = ratio(85, 100);
/// Share of effective speed that friendly pushes may add per tick.
pub(crate) const SOFT_PUSH_SPEED_SHARE: SimScalar = ratio(65, 100);
/// Floor on the friendly push per tick, so very slow units still separate.
pub(crate) const SOFT_PUSH_FLOOR: SimScalar = ratio(15, 1000);
