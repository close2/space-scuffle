//! The concave as a goal (plan `2026-11-20-contact-solver` G-2 to G-5, deviation D-γ).
//!
//! # Responsibility
//! An attacker blocked by its own fighting line walks to a free **slot** on its weapon's arc round
//! the target instead of walking round the crowd. This module holds the geometry of that choice:
//! - [`blocking_interval`]: the arc a body (or another unit's claim) blocks on the circle of
//!   radius `ρ` round the target (G-2);
//! - [`choose_slot`]: the free angle nearest the unit's bearing, ties clockwise (G-3);
//! - [`resolve_claims`]: the admission of one tick's new claims on one target by a sort that does
//!   not depend on the order they were made in (G-4);
//! - [`claim_fate`]: whether a claim is kept, freed or chosen again (G-5).
//!
//! # How it fits
//! The solver pipeline's slot pass runs these over the tick-start snapshot before any unit moves,
//! and the chase walks to the claimed point (G-7). Angles are `SimAngle` binary units (65,536 per
//! turn) from `atan2`, so no float or trigonometric library is involved.
//!
//! # Invariants
//! - An angle is free when it lies strictly outside every blocked interval.
//! - The chosen slot depends on the bearing and the set of intervals only; equal distances go
//!   clockwise (decreasing angle), so no id decides.
//! - The admitted set of a tick depends on the set of requests, not their order: the sort key
//!   ends with the claimant's id, which decides only exact ties.

use super::math::{checked_vec_add, checked_vec_scale};
use super::MovementMathError;
use sim_types::{EntityId, SimAngle, SimScalar, SimVec2};

/// Half a turn in binary angle units: an interval this wide blocks the whole circle.
pub(crate) const HALF_TURN: u16 = 32_768;

/// An arc of the slot circle that a body blocks: `centre ± half`, binary units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ArcInterval {
    /// The direction from the target to the blocking body.
    pub centre: SimAngle,
    /// Half the blocked width; [`HALF_TURN`] blocks every angle.
    pub half: u16,
}

impl ArcInterval {
    /// Whether `angle` lies inside the interval (its ends included).
    #[must_use]
    pub(crate) fn blocks(self, angle: SimAngle) -> bool {
        self.half >= HALF_TURN || self.centre.diff(angle).unsigned_abs() <= self.half
    }
}

/// The interval that a body of radius `other_radius` at `other` blocks for a unit of radius
/// `radius` on the circle of radius `rho` round `target`, or `None` when the body's centre lies
/// farther than `radius + other_radius` from the circle (G-2). The half-width is
/// `asin((r + r_o) / ρ)`, computed as `atan2(s, √(1 − s²))`.
///
/// # Errors
/// An unrepresentable distance or ratio.
pub(crate) fn blocking_interval(
    target: SimVec2,
    rho: SimScalar,
    radius: SimScalar,
    other: SimVec2,
    other_radius: SimScalar,
) -> Result<Option<ArcInterval>, MovementMathError> {
    let reach = radius
        .checked_add(other_radius)
        .ok_or(MovementMathError::Unrepresentable)?;
    let distance =
        super::checked_distance(other, target).ok_or(MovementMathError::Unrepresentable)?;
    let off_circle = distance
        .checked_sub(rho)
        .ok_or(MovementMathError::Unrepresentable)?;
    if off_circle.abs() > reach {
        return Ok(None);
    }
    let centre = SimAngle::atan2(
        other.y.saturating_sub(target.y),
        other.x.saturating_sub(target.x),
    );
    if rho <= reach {
        return Ok(Some(ArcInterval {
            centre,
            half: HALF_TURN,
        }));
    }
    let sine = reach
        .checked_div(rho)
        .ok_or(MovementMathError::Unrepresentable)?;
    let cosine = SimScalar::ONE
        .checked_sub(
            sine.checked_mul(sine)
                .ok_or(MovementMathError::Unrepresentable)?,
        )
        .ok_or(MovementMathError::Unrepresentable)?
        .sqrt()
        .map_err(|_| MovementMathError::Unrepresentable)?;
    Ok(Some(ArcInterval {
        centre,
        half: SimAngle::atan2(sine, cosine).raw(),
    }))
}

/// Whether `angle` lies outside every interval of `intervals`.
#[must_use]
pub(crate) fn is_free(angle: SimAngle, intervals: &[ArcInterval]) -> bool {
    intervals.iter().all(|interval| !interval.blocks(angle))
}

/// The free angle nearest `bearing` among the bearing itself and the outer ends of every blocked
/// interval, for which `admissible` holds (G-3: inside the leash and the unit's terrain domain);
/// equal distances go clockwise. `None` when no candidate is free.
pub(crate) fn choose_slot(
    bearing: SimAngle,
    intervals: &[ArcInterval],
    mut admissible: impl FnMut(SimAngle) -> bool,
) -> Option<SimAngle> {
    let ends = intervals
        .iter()
        .filter(|i| i.half < HALF_TURN)
        .flat_map(|interval| {
            let past = interval.half.saturating_add(1);
            [
                SimAngle::from_raw(interval.centre.raw().wrapping_add(past)),
                SimAngle::from_raw(interval.centre.raw().wrapping_sub(past)),
            ]
        });
    let mut best: Option<(u16, bool, SimAngle)> = None;
    for candidate in std::iter::once(bearing).chain(ends) {
        if !is_free(candidate, intervals) {
            continue;
        }
        let turn = bearing.diff(candidate);
        // Ties: clockwise (a negative turn) first.
        let key = (turn.unsigned_abs(), turn >= 0, candidate);
        if best.is_none_or(|b| (key.0, key.1) < (b.0, b.1)) && admissible(candidate) {
            best = Some(key);
        }
    }
    best.map(|(_, _, angle)| angle)
}

/// The point at `angle` on the circle of radius `rho` round `target`.
///
/// # Errors
/// An unrepresentable product or sum.
pub(crate) fn slot_point(
    target: SimVec2,
    rho: SimScalar,
    angle: SimAngle,
) -> Result<SimVec2, MovementMathError> {
    checked_vec_add(
        target,
        checked_vec_scale(SimVec2::new(angle.cos(), angle.sin()), rho)?,
    )
}

/// One unit's new claim on one tick, before admission (G-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotRequest {
    /// The claimant.
    pub unit: EntityId,
    /// Its target.
    pub target: EntityId,
    /// The target's tick-start position.
    pub centre: SimVec2,
    /// The claimant's bearing from the target.
    pub bearing: SimAngle,
    /// The angle it chose.
    pub angle: SimAngle,
    /// The point it chose, world units.
    pub point: SimVec2,
    /// Its slot circle's radius, world units.
    pub rho: SimScalar,
    /// Its collision radius, world units.
    pub radius: SimScalar,
    /// Its distance to the target, world units.
    pub distance: SimScalar,
    /// Set by [`resolve_claims`]: whether the claim stands.
    pub admitted: bool,
}

impl SlotRequest {
    /// The admission order of G-4: angular distance from the bearing, distance to the target, id.
    fn order(&self) -> (EntityId, u16, SimScalar, EntityId) {
        (
            self.target,
            self.bearing.diff(self.angle).unsigned_abs(),
            self.distance,
            self.unit,
        )
    }
}

/// Admits one tick's new claims (G-4): per target, in the order of [`SlotRequest::order`], each
/// claim is admitted unless an earlier admitted claim on the same target blocks its angle; a
/// refused claimant asks again next tick. Sorts `requests` in place.
///
/// # Errors
/// An unrepresentable interval.
pub(crate) fn resolve_claims(requests: &mut [SlotRequest]) -> Result<(), MovementMathError> {
    requests.sort_unstable_by_key(SlotRequest::order);
    for index in 0..requests.len() {
        let request = requests[index];
        let mut free = true;
        for earlier in requests[..index]
            .iter()
            .filter(|e| e.admitted && e.target == request.target)
        {
            let interval = blocking_interval(
                request.centre,
                request.rho,
                request.radius,
                earlier.point,
                earlier.radius,
            )?;
            if interval.is_some_and(|i| i.blocks(request.angle)) {
                free = false;
                break;
            }
        }
        requests[index].admitted = free;
    }
    Ok(())
}

/// What becomes of a kept claim this tick (G-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaimFate {
    /// The claim stands.
    Keep,
    /// The claim is dropped: the unit is in range, or its target changed.
    Free,
    /// The claim is chosen again at once: its slot was taken, or the target moved.
    Rechoose,
}

/// The fate of a claim on `claim_target` at `point` for a unit whose target is now `target`,
/// standing at `target_at` (tick start), with slot circle `rho` and radius `radius`; `in_band`
/// when the unit can fire, `taken` when a tick-start body blocks the claimed slot (G-5).
///
/// # Errors
/// An unrepresentable distance.
pub(crate) fn claim_fate(
    (claim_target, point): (EntityId, SimVec2),
    (target, target_at): (EntityId, SimVec2),
    (rho, radius): (SimScalar, SimScalar),
    in_band: bool,
    taken: bool,
) -> Result<ClaimFate, MovementMathError> {
    if in_band || claim_target != target {
        return Ok(ClaimFate::Free);
    }
    let distance =
        super::checked_distance(point, target_at).ok_or(MovementMathError::Unrepresentable)?;
    let drift = distance
        .checked_sub(rho)
        .ok_or(MovementMathError::Unrepresentable)?;
    if drift.abs() > radius || taken {
        return Ok(ClaimFate::Rechoose);
    }
    Ok(ClaimFate::Keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    fn degrees(value: i32) -> SimAngle {
        SimAngle::from_degrees(value)
    }

    /// G-2 against a hand-built ring: on a circle of radius 2 round the origin, a unit of radius
    /// 0.5 is blocked by bodies of radius 0.5 within asin(1/2) = 30° of their bearings, and not by
    /// a body off the circle.
    #[test]
    fn free_intervals_match_a_hand_built_ring() -> Result<(), Box<dyn std::error::Error>> {
        let (rho, r) = (SimScalar::from_num(2), SimScalar::HALF);
        let east = blocking_interval(SimVec2::ZERO, rho, r, at(2.0, 0.0), r)?.ok_or("east")?;
        let north = blocking_interval(SimVec2::ZERO, rho, r, at(0.0, 2.0), r)?.ok_or("north")?;
        assert!(blocking_interval(SimVec2::ZERO, rho, r, at(4.5, 0.0), r)?.is_none());
        // asin(1/2) is 30°: 5461 or 5462 binary units.
        assert!((5460..=5462).contains(&east.half), "{}", east.half);
        let intervals = [east, north];
        for (angle, free) in [
            (0, false),
            (29, false),
            (31, true),
            (45, true),
            (59, true),
            (61, false),
            (180, true),
        ] {
            assert_eq!(is_free(degrees(angle), &intervals), free, "{angle}°");
        }
        // A ring too small for the pair blocks everything.
        let whole = blocking_interval(SimVec2::ZERO, SimScalar::HALF, r, at(0.5, 0.0), r)?;
        assert_eq!(whole.map(|i| i.half), Some(HALF_TURN));
        Ok(())
    }

    /// G-3: the bearing itself when free; else the nearer end of the interval blocking it; and an
    /// exact tie goes clockwise.
    #[test]
    fn the_nearest_free_angle_is_chosen_and_ties_go_clockwise() {
        let interval = ArcInterval {
            centre: degrees(90),
            half: 4096,
        };
        assert_eq!(
            choose_slot(degrees(180), &[interval], |_| true),
            Some(degrees(180))
        );
        // The bearing sits on the interval's centre: both ends are equally far; clockwise wins.
        let chosen = choose_slot(degrees(90), &[interval], |_| true);
        assert_eq!(chosen.map(SimAngle::raw), Some(degrees(90).raw() - 4097));
        // With the clockwise end inadmissible, the other end.
        let other = choose_slot(degrees(90), &[interval], |a| a.raw() > degrees(90).raw());
        assert_eq!(other.map(SimAngle::raw), Some(degrees(90).raw() + 4097));
        // Nothing free.
        let all = ArcInterval {
            centre: degrees(0),
            half: HALF_TURN,
        };
        assert_eq!(choose_slot(degrees(10), &[all], |_| true), None);
    }

    fn request(unit: u32, angle: i32, bearing: i32) -> Result<SlotRequest, MovementMathError> {
        let rho = SimScalar::from_num(2);
        Ok(SlotRequest {
            unit: EntityId(unit),
            target: EntityId(100),
            centre: SimVec2::ZERO,
            bearing: degrees(bearing),
            angle: degrees(angle),
            point: slot_point(SimVec2::ZERO, rho, degrees(angle))?,
            rho,
            radius: SimScalar::HALF,
            distance: SimScalar::from_num(5),
            admitted: false,
        })
    }

    /// G-4: two claimants on overlapping slots; the one whose slot lies nearer its bearing is
    /// admitted whichever order they came in, and the other is refused.
    #[test]
    fn claims_are_admitted_in_sort_order_whatever_the_input_order() -> Result<(), MovementMathError>
    {
        let near = request(7, 40, 38)?;
        let far = request(3, 50, 80)?;
        let apart = request(5, 200, 200)?;
        let mut forward = [near, far, apart];
        let mut backward = [apart, far, near];
        resolve_claims(&mut forward)?;
        resolve_claims(&mut backward)?;
        assert_eq!(forward, backward);
        let admitted: Vec<u32> = forward
            .iter()
            .filter(|r| r.admitted)
            .map(|r| r.unit.0)
            .collect();
        assert_eq!(admitted, vec![5, 7]);
        Ok(())
    }

    /// G-5: freed in band or on a new target; chosen again when taken or when the target moved
    /// off the claim's circle by more than the unit's radius; kept otherwise.
    #[test]
    fn a_claim_is_kept_freed_or_chosen_again() -> Result<(), MovementMathError> {
        let point = at(2.0, 0.0);
        let shape = (SimScalar::from_num(2), SimScalar::HALF);
        let fate = |target: u32, at: SimVec2, in_band, taken| {
            claim_fate(
                (EntityId(9), point),
                (EntityId(target), at),
                shape,
                in_band,
                taken,
            )
        };
        assert_eq!(fate(9, SimVec2::ZERO, false, false)?, ClaimFate::Keep);
        assert_eq!(fate(9, SimVec2::ZERO, true, false)?, ClaimFate::Free);
        assert_eq!(fate(8, SimVec2::ZERO, false, false)?, ClaimFate::Free);
        assert_eq!(fate(9, SimVec2::ZERO, false, true)?, ClaimFate::Rechoose);
        assert_eq!(fate(9, at(0.4, 0.0), false, false)?, ClaimFate::Keep);
        assert_eq!(fate(9, at(-0.6, 0.0), false, false)?, ClaimFate::Rechoose);
        Ok(())
    }
}
