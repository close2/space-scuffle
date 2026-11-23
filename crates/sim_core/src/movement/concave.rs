//! The concave as a goal (plan `2026-11-20-contact-solver` G-2, G-3, Amendment 7 item 2).
//!
//! # Responsibility
//! The geometry of the ring round a chase target, the circle on which the unit's weapon just
//! reaches it (the band's outer edge):
//! - [`blocking_interval`]: the arc a body blocks on the circle of radius `ρ` round the target
//!   (G-2);
//! - [`is_free`]: whether an angle lies outside every blocked arc;
//! - [`choose_slot`]: the free angle nearest the unit's bearing, ties clockwise (G-3);
//! - [`slot_point`]: the point at an angle on the circle.
//!
//! # How it fits
//! The solver pipeline's ring rule (`movement::ring`) runs these over the tick-start snapshot
//! for every chase. There are no claims: occupancy is geometry only (Amendment 7, item 2), so no
//! unit's choice depends on another's choice this tick. Angles are `SimAngle` binary units
//! (65,536 per turn) from `atan2`, so no float or trigonometric library is involved.
//!
//! # Invariants
//! - An angle is free when it lies strictly outside every blocked interval.
//! - The chosen angle depends on the bearing and the set of intervals only; equal distances go
//!   clockwise (decreasing angle), so no id decides.

use super::math::{checked_vec_add, checked_vec_scale};
use super::MovementMathError;
use sim_types::{SimAngle, SimScalar, SimVec2};

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
}
