//! Classification of what one unit was doing during one tick.
//!
//! # Responsibility
//! Turns a unit's observable state after a tick (whether an enemy is inside its weapon band,
//! whether it has a target, and how far and in which direction it moved) into one
//! [`Activity`]. The classes are the ones the investigation plan asks for
//! (`docs/plans/2026-11-04-zealot-marine-investigation.md`, D-2): in contact, blocked, flanking
//! or walking.
//!
//! # How it fits
//! [`super::BattleTrace`] computes the inputs from the public world state and counts the classes
//! per side per tick. The tests of this module pin the thresholds on hand-made inputs, so the
//! counts in the evidence file can be read without the code.
//!
//! # Invariants
//! - [`Activity::Engaged`] wins over every movement class: a unit with an enemy in its band is in
//!   contact whether or not it is being pushed.
//! - The movement thresholds are fractions of the unit's own maximum speed, so the classes mean
//!   the same for slow and fast units.

/// What a unit was doing during one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Activity {
    /// Some living enemy lies inside its weapon's engagement band (`max_range` plus both
    /// collision radii, centre to centre, without the release slop).
    Engaged,
    /// It has a target outside its band and moved less than [`STALLED_SPEED_FRACTION`] of its
    /// maximum speed.
    Blocked,
    /// It has a target outside its band and moved at least [`STALLED_SPEED_FRACTION`] of its
    /// maximum speed, but more than [`FLANK_ANGLE_DEGREES`] away from the direction to it.
    Flanking,
    /// It has a target outside its band and moved towards it.
    Approaching,
    /// It has no target and moved.
    Walking,
    /// It has no target and stood still.
    Idle,
}

impl Activity {
    /// Every class, in report order.
    pub const ALL: [Self; 6] = [
        Self::Engaged,
        Self::Blocked,
        Self::Flanking,
        Self::Approaching,
        Self::Walking,
        Self::Idle,
    ];

    /// Position of this class in [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Engaged => 0,
            Self::Blocked => 1,
            Self::Flanking => 2,
            Self::Approaching => 3,
            Self::Walking => 4,
            Self::Idle => 5,
        }
    }

    /// Lower-case name used in CSV headers and reports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Engaged => "engaged",
            Self::Blocked => "blocked",
            Self::Flanking => "flanking",
            Self::Approaching => "approaching",
            Self::Walking => "walking",
            Self::Idle => "idle",
        }
    }
}

/// A unit that moved less than this fraction of its maximum speed in a tick stood still.
pub const STALLED_SPEED_FRACTION: f64 = 0.1;

/// A unit whose step points more than this many degrees away from its target is going around.
pub const FLANK_ANGLE_DEGREES: f64 = 60.0;

/// The observable inputs of one unit's classification for one tick, in world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActivityInput {
    /// Whether a living enemy lies inside the unit's engagement band.
    pub enemy_in_band: bool,
    /// Offset from the unit to its target's centre, if it has a living target.
    pub to_target: Option<(f64, f64)>,
    /// The unit's displacement this tick (`Entity::velocity`, which the collision phase sets to
    /// the movement actually achieved, pushes included).
    pub step: (f64, f64),
    /// The unit's maximum speed, in world units per tick.
    pub max_speed: f64,
}

/// The class of a unit with inputs `input`.
#[must_use]
pub fn classify(input: &ActivityInput) -> Activity {
    if input.enemy_in_band {
        return Activity::Engaged;
    }
    let (sx, sy) = input.step;
    let moved = sx.hypot(sy);
    let stalled = moved < STALLED_SPEED_FRACTION * input.max_speed;
    let Some((tx, ty)) = input.to_target else {
        return if stalled {
            Activity::Idle
        } else {
            Activity::Walking
        };
    };
    if stalled {
        return Activity::Blocked;
    }
    let reach = tx.hypot(ty);
    if reach <= f64::EPSILON {
        return Activity::Approaching;
    }
    let cosine = (sx * tx + sy * ty) / (moved * reach);
    if cosine < FLANK_ANGLE_DEGREES.to_radians().cos() {
        Activity::Flanking
    } else {
        Activity::Approaching
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        enemy_in_band: bool,
        to_target: Option<(f64, f64)>,
        step: (f64, f64),
    ) -> ActivityInput {
        ActivityInput {
            enemy_in_band,
            to_target,
            step,
            max_speed: 0.126,
        }
    }

    #[test]
    fn an_enemy_in_band_means_engaged_whatever_the_movement() {
        assert_eq!(classify(&input(true, None, (0.0, 0.0))), Activity::Engaged);
        assert_eq!(
            classify(&input(true, Some((5.0, 0.0)), (0.0, 0.126))),
            Activity::Engaged
        );
    }

    #[test]
    fn a_unit_with_a_target_that_barely_moves_is_blocked() {
        // 10 % of 0.126 is 0.0126: just below it is blocked, just above it is not.
        assert_eq!(
            classify(&input(false, Some((3.0, 0.0)), (0.0125, 0.0))),
            Activity::Blocked
        );
        assert_eq!(
            classify(&input(false, Some((3.0, 0.0)), (0.0127, 0.0))),
            Activity::Approaching
        );
    }

    #[test]
    fn a_step_more_than_sixty_degrees_off_the_target_is_flanking() {
        let target = Some((3.0, 0.0));
        let at = |degrees: f64| {
            let r = degrees.to_radians();
            (0.126 * r.cos(), 0.126 * r.sin())
        };
        assert_eq!(
            classify(&input(false, target, at(0.0))),
            Activity::Approaching
        );
        assert_eq!(
            classify(&input(false, target, at(59.0))),
            Activity::Approaching
        );
        assert_eq!(
            classify(&input(false, target, at(61.0))),
            Activity::Flanking
        );
        assert_eq!(
            classify(&input(false, target, at(90.0))),
            Activity::Flanking
        );
        assert_eq!(
            classify(&input(false, target, at(-120.0))),
            Activity::Flanking
        );
    }

    #[test]
    fn a_unit_without_a_target_walks_or_idles() {
        assert_eq!(
            classify(&input(false, None, (0.09, 0.0))),
            Activity::Walking
        );
        assert_eq!(classify(&input(false, None, (0.0, 0.0))), Activity::Idle);
    }

    #[test]
    fn indices_follow_the_report_order() {
        for (position, activity) in Activity::ALL.iter().enumerate() {
            assert_eq!(activity.index(), position);
        }
    }
}
