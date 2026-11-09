//! Flee goals and ground-segment pathability.
//!
//! # Responsibility
//! Finds a reachable point away from a threat for units that evacuate a hazard or flee an
//! attacker, and answers whether a straight ground move between two points is passable.
//!
//! # How it fits
//! Called by hazard evacuation in phase 2, by target acquisition in phase 4, and by collision
//! and movement admission, which use [`SimWorld::is_position_pathable`] for ground segments.
//!
//! # Invariants
//! - A flee goal is returned only when the actor's movement policy permits the whole segment
//!   to it, and it is clamped inside the map by the actor's radius.
//! - Candidates are tried in a fixed order of distances and angles, so the result is
//!   deterministic.

use super::{MotionPolicy, MovementInvariant, MovementLayer, MovementMathError};
use crate::entity::Entity;
use crate::SimWorld;
use sim_map::{CellFlags, SimMap};
use sim_types::{EntityId, SimAngle, SimScalar, SimVec2};

/// Squared distance, in raw Q64.64, within which a threat counts as standing on its victim.
/// Fixed at 0.0001 world units squared, in the same widened scale as `squared_distance`.
const FLEE_COINCIDENCE_SQUARED: u128 = 429_497 << 32;

impl SimWorld {
    /// Finds a legacy ground/radius-one autonomous flee goal in world coordinates.
    ///
    /// # Errors
    /// Returns invalid distance/domain or unrepresentable arithmetic without mutation.
    pub fn compute_flee_destination(
        &self,
        victim_pos: SimVec2,
        threat_pos: SimVec2,
        desired_dist: SimScalar,
    ) -> Result<Option<SimVec2>, MovementMathError> {
        Self::flee_destination(
            &self.map,
            MotionPolicy {
                layer: MovementLayer::Ground,
                radius: SimScalar::ONE,
                burrowed: false,
            },
            victim_pos,
            threat_pos,
            desired_dist,
        )
    }

    /// Finds an autonomous flee goal using the actor's current authored policy.
    ///
    /// # Errors
    /// Rejects malformed actor/speed/foundation state or unrepresentable geometry.
    pub fn compute_entity_flee_destination(
        &self,
        id: EntityId,
        threat: SimVec2,
        distance: SimScalar,
    ) -> Result<Option<SimVec2>, MovementInvariant> {
        self.validate_transport_links()
            .map_err(MovementInvariant::Transport)?;
        self.compute_entity_flee_destination_prevalidated(id, threat, distance)
    }

    // Caller validated graph. Loaded actors have no world-space flee goal.
    pub(crate) fn compute_entity_flee_destination_prevalidated(
        &self,
        id: EntityId,
        threat: SimVec2,
        distance: SimScalar,
    ) -> Result<Option<SimVec2>, MovementInvariant> {
        if self
            .entities
            .get(&id)
            .is_some_and(Entity::is_loaded_in_transport)
        {
            return Ok(None);
        }
        let entry = self.motion_entry(id)?;
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "flee actor disappeared",
            })?;
        Self::flee_destination(&self.map, entry.policy, actor.position, threat, distance).map_err(
            |source| MovementInvariant::Arithmetic {
                entity: id,
                operation: "flee destination",
                source,
            },
        )
    }

    fn flee_destination(
        map: &SimMap,
        policy: MotionPolicy,
        victim_pos: SimVec2,
        threat_pos: SimVec2,
        desired_dist: SimScalar,
    ) -> Result<Option<SimVec2>, MovementMathError> {
        use crate::movement::math::{
            checked_vec_add, checked_vec_scale, direction, squared_distance,
        };
        if desired_dist < SimScalar::ZERO || !policy.contains(map, victim_pos) {
            return Err(MovementMathError::InvalidDistance);
        }
        let away = if squared_distance(victim_pos, threat_pos) > (0, FLEE_COINCIDENCE_SQUARED) {
            direction(victim_pos, threat_pos)?
        } else {
            -SimVec2::X_AXIS
        };
        let min = i128::from(policy.radius.raw());
        let max_x = (i128::from(map.width()) << 32) - min;
        let max_y = (i128::from(map.height()) << 32) - min;
        if min > max_x || min > max_y {
            return Err(MovementMathError::InvalidDistance);
        }
        let angles = [0, 45, -45, 90, -90, 135, -135];
        for dist in [
            desired_dist,
            SimScalar::from_num(4),
            SimScalar::from_num(3),
            SimScalar::from_num(2),
        ] {
            if dist > desired_dist {
                continue;
            }
            for degrees in angles {
                let dir = if degrees == 0 {
                    away
                } else {
                    let angle = SimAngle::from_degrees(degrees);
                    checked_vec_add(
                        checked_vec_scale(away, angle.cos())?,
                        checked_vec_scale(SimVec2::new(-away.y, away.x), angle.sin())?,
                    )?
                };
                let delta = checked_vec_scale(dir, dist)?;
                // Existing autonomous boundary adjustment is evaluated wide before
                // narrowing, so a legal clamped goal does not overflow on overshoot.
                let x =
                    (i128::from(victim_pos.x.raw()) + i128::from(delta.x.raw())).clamp(min, max_x);
                let y =
                    (i128::from(victim_pos.y.raw()) + i128::from(delta.y.raw())).clamp(min, max_y);
                let narrow = |raw| {
                    i64::try_from(raw)
                        .map(SimScalar::from_raw)
                        .map_err(|_| MovementMathError::Unrepresentable)
                };
                let candidate = SimVec2::new(narrow(x)?, narrow(y)?);
                if policy.permits_segment(map, victim_pos, candidate) {
                    return Ok(Some(candidate));
                }
            }
        }
        Ok(None)
    }

    /// Checks whether `target_pos` is valid and passable for a ground unit currently at `current_pos`.
    #[must_use]
    #[inline]
    pub fn is_position_pathable_from(&self, current_pos: SimVec2, target_pos: SimVec2) -> bool {
        Self::is_position_pathable(&self.map, current_pos, target_pos)
    }

    /// Checks the complete ground-motion segment in fixed-point world units.
    ///
    /// Every crossed terrain edge must be connected and every entered cell must be
    /// free of structures. This also applies to long flee candidates and collision pushes.
    #[must_use]
    pub fn is_position_pathable(map: &SimMap, current_pos: SimVec2, target_pos: SimVec2) -> bool {
        map.ground_segment_is_connected(current_pos, target_pos, |_, next| {
            map.cell(next.0, next.1).is_ok_and(|cell| {
                cell.flags.contains(CellFlags::GROUND_PATHABLE)
                    && !cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE)
            })
        })
    }
}
