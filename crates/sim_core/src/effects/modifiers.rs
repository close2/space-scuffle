//! The combined modifiers of the statuses a unit carries (plan
//! `2026-11-10-spellcaster-mechanics` D-3, §6): stun, movement speed, weapon range, reveal and
//! the suppressions of detection, collision, boarding and Blink.
//!
//! # Responsibility
//! Folds a unit's statuses into one [`StatusModifiers`] and states how each modifier changes
//! the quantity it governs ([`StatusModifiers::weapon_range`]). Every system that a status may
//! change reads the fold through `Entity::status_modifiers`, never a status by name (C5).
//!
//! # How it fits
//! The fold is a cache on each entity, derived from the hashed status table
//! (`SimWorld::effects`) and the spell catalog, and so left out of the checksum. The effect
//! engine refreshes it whenever it inserts or removes a status of the entity
//! ([`crate::SimWorld::refresh_status_modifiers`]); a refresh of an existing status changes
//! only its duration, which no modifier reads.
//!
//! # Invariants
//! - An entity without statuses carries [`StatusModifiers::NEUTRAL`], under which every
//!   reader computes exactly what it computed before statuses existed.
//! - The fold is independent of status order: factors multiply, deltas add, flags or, and
//!   the range floor is the greatest floor of the statuses that change a range.

use super::state::EffectWorld;
use crate::catalog::SpellCatalog;
use sim_types::{EntityId, SimScalar, StatusSpec};

/// What a unit's statuses do to it, combined.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent modifiers of the status data (stun, reveal, four suppressions), each read \
              by a different system"
)]
pub struct StatusModifiers {
    /// The unit cannot move, attack or cast (its orders wait).
    pub stun: bool,
    /// Product of the statuses' movement speed factors, `0..=1`.
    pub move_speed_factor: SimScalar,
    /// Sum of the range changes of ranged (non-melee) weapons, world units.
    pub ranged_weapon_range_delta: SimScalar,
    /// Sum of the range changes of every weapon, world units.
    pub weapon_range_delta: SimScalar,
    /// The least range a reduction leaves, world units: the greatest floor among the statuses
    /// that change a range.
    pub range_floor: SimScalar,
    /// The unit is seen through cloak and burrow by every player with vision of it.
    pub reveal: bool,
    /// The unit does not detect.
    pub suppress_detection: bool,
    /// The unit neither pushes nor is pushed, and nothing steers around it.
    pub suppress_collision: bool,
    /// The unit cannot board a transport.
    pub suppress_boarding: bool,
    /// The unit cannot Blink.
    pub suppress_blink: bool,
}

impl Default for StatusModifiers {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

impl StatusModifiers {
    /// The modifiers of a unit without statuses: nothing changes.
    pub const NEUTRAL: Self = Self {
        stun: false,
        move_speed_factor: SimScalar::ONE,
        ranged_weapon_range_delta: SimScalar::ZERO,
        weapon_range_delta: SimScalar::ZERO,
        range_floor: SimScalar::ZERO,
        reveal: false,
        suppress_detection: false,
        suppress_collision: false,
        suppress_boarding: false,
        suppress_blink: false,
    };

    /// Folds the statuses `entity` carries in `effects`, reading their specs from `spells`.
    #[must_use]
    pub fn of(effects: &EffectWorld, spells: &SpellCatalog, entity: EntityId) -> Self {
        effects
            .statuses_of(entity)
            .iter()
            .filter_map(|active| spells.status(active.status))
            .fold(Self::NEUTRAL, |folded, status| folded.with(&status.spec))
    }

    /// These modifiers with one more status's.
    #[must_use]
    fn with(self, spec: &StatusSpec) -> Self {
        let changes_range = spec.ranged_weapon_range_delta != SimScalar::ZERO
            || spec.weapon_range_delta != SimScalar::ZERO;
        Self {
            stun: self.stun || spec.stun,
            move_speed_factor: self
                .move_speed_factor
                .saturating_mul(spec.move_speed_factor),
            ranged_weapon_range_delta: self
                .ranged_weapon_range_delta
                .saturating_add(spec.ranged_weapon_range_delta),
            weapon_range_delta: self
                .weapon_range_delta
                .saturating_add(spec.weapon_range_delta),
            range_floor: if changes_range {
                self.range_floor.max(spec.range_floor)
            } else {
                self.range_floor
            },
            reveal: self.reveal || spec.reveal,
            suppress_detection: self.suppress_detection || spec.suppress_detection,
            suppress_collision: self.suppress_collision || spec.suppress_collision,
            suppress_boarding: self.suppress_boarding || spec.suppress_boarding,
            suppress_blink: self.suppress_blink || spec.suppress_blink,
        }
    }

    /// The range, in world units, of a weapon of `range` (upgrades included) that is `melee` or
    /// ranged, under these modifiers (plan §6.4): range plus the deltas that apply to it, then
    /// no less than the floor. A weapon already at or below the floor keeps its range, and a
    /// weapon no delta applies to is returned unchanged.
    #[must_use]
    pub fn weapon_range(self, range: SimScalar, melee: bool) -> SimScalar {
        let delta = if melee {
            self.weapon_range_delta
        } else {
            self.weapon_range_delta
                .saturating_add(self.ranged_weapon_range_delta)
        };
        if delta == SimScalar::ZERO || range <= self.range_floor {
            return range;
        }
        range.saturating_add(delta).max(self.range_floor)
    }

    /// The movement speed, in world units per tick, of a unit whose speed is `speed` before its
    /// statuses.
    #[must_use]
    pub fn move_speed(self, speed: SimScalar) -> SimScalar {
        if self.move_speed_factor == SimScalar::ONE {
            speed
        } else {
            speed.saturating_mul(self.move_speed_factor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StatusModifiers;
    use sim_types::SimScalar;

    fn reduced(ranged: i32, all: i32) -> StatusModifiers {
        StatusModifiers {
            ranged_weapon_range_delta: SimScalar::from_i32(ranged),
            weapon_range_delta: SimScalar::from_i32(all),
            range_floor: SimScalar::from_raw(SimScalar::ONE.raw() / 10),
            ..StatusModifiers::NEUTRAL
        }
    }

    #[test]
    fn neutral_modifiers_change_nothing() {
        let neutral = StatusModifiers::NEUTRAL;
        for range in [0, 1, 6, 13] {
            let range = SimScalar::from_i32(range);
            assert_eq!(neutral.weapon_range(range, false), range);
            assert_eq!(neutral.weapon_range(range, true), range);
        }
        assert_eq!(neutral.move_speed(SimScalar::HALF), SimScalar::HALF);
    }

    #[test]
    fn a_ranged_reduction_spares_melee_and_stops_at_the_floor() {
        let cloud = reduced(-20, 0);
        let floor = cloud.range_floor;
        assert_eq!(cloud.weapon_range(SimScalar::from_i32(5), false), floor);
        assert_eq!(
            cloud.weapon_range(SimScalar::from_i32(5), true),
            SimScalar::from_i32(5)
        );
        // A weapon already at or below the floor keeps its range.
        let short = SimScalar::from_raw(floor.raw() / 2);
        assert_eq!(cloud.weapon_range(short, false), short);
        // A smaller reduction leaves the difference.
        assert_eq!(
            reduced(-2, 0).weapon_range(SimScalar::from_i32(5), false),
            SimScalar::from_i32(3)
        );
    }

    #[test]
    fn an_all_weapons_reduction_reaches_melee_too() {
        let structure = reduced(0, -20);
        assert_eq!(
            structure.weapon_range(SimScalar::from_i32(7), true),
            structure.range_floor
        );
        assert_eq!(
            structure.weapon_range(SimScalar::from_i32(7), false),
            structure.range_floor
        );
    }

    #[test]
    fn a_speed_factor_scales_speed() {
        let slow = StatusModifiers {
            move_speed_factor: SimScalar::from_raw(SimScalar::ONE.raw() / 4),
            ..StatusModifiers::NEUTRAL
        };
        assert_eq!(slow.move_speed(SimScalar::ONE), slow.move_speed_factor);
    }
}
