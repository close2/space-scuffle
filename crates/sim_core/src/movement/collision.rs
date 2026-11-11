//! Tick phase 6: resolves circle contact between bodies after they moved.
//!
//! Locomotion avoids contact where it can; this phase repairs what remains. Pair corrections are
//! computed from one immutable snapshot and summed, so the result does not depend on pair order.
//! Each pair contribution commits to both accumulators together, and a unit's final position is
//! only written once its arithmetic and terrain checks have passed. The phase ends by recording
//! what every unit actually achieved, which is what the next tick steers against.
//!
//! # Invariants
//! - Bodies of different classes (`MotionPolicy::body_class`: ground, air, burrowed), or with a
//!   cliff between them, are never in contact.
//! - Friendly correction is capped to the unit's effective speed; hostile correction is not,
//!   because bodies of two sides must never overlap by choice.
//! - A body that may not move receives exactly zero correction.

use super::locomotion::{slide_target, MotionEntry};
use super::math::{
    checked_normalize, checked_vec_add, checked_vec_scale, checked_vec_sub, within_distance,
};
use super::tuning::{SOFT_PUSH_FLOOR, SOFT_PUSH_SPEED_SHARE};
use super::{MovementInvariant, MovementMathError};
use crate::entity::Entity;
use crate::steering::{
    compute_push_displacement_with_mobility, CollisionMobility, PushEntityParams,
};
use crate::SimWorld;
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2};

/// One body's fixed state for the phase, so pair tests touch flat memory only.
#[derive(Clone, Copy)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts about one body, read together by the contact rules"
)]
pub(super) struct CollisionBody {
    entry: MotionEntry,
    position: SimVec2,
    velocity: SimVec2,
    owner: PlayerId,
    /// Nothing at all can move this body.
    fixed: bool,
    /// Hold Position, immobile or unpushable.
    holding: bool,
    /// Stands fighting, which keeps friendly traffic off it.
    engaged: bool,
    /// Passes through other units.
    ghost: bool,
    /// Has nothing to do (no order, `Idle` or `Stop`); only then may a friend push it aside.
    idle: bool,
}

/// Corrections accumulated for one body, kept apart because only friendly ones are speed capped.
#[derive(Clone, Copy, Default)]
pub(super) struct BodyPush {
    friendly: SimVec2,
    hostile: SimVec2,
}

/// Adds one pair's corrections to both bodies, or to neither if the sum cannot be represented.
fn accumulate_pair(
    a: &mut SimVec2,
    b: &mut SimVec2,
    delta_a: SimVec2,
    delta_b: SimVec2,
) -> Result<(), MovementMathError> {
    let next_a = checked_vec_add(*a, delta_a)?;
    let next_b = checked_vec_add(*b, delta_b)?;
    *a = next_a;
    *b = next_b;
    Ok(())
}

/// Applies friendly correction to `position`, keeping the tick's total travel within `speed`.
fn soft_position(
    position: SimVec2,
    start: SimVec2,
    push: SimVec2,
    speed: SimScalar,
) -> Result<SimVec2, MovementMathError> {
    if speed < SimScalar::ZERO {
        return Err(MovementMathError::NegativeSpeed);
    }
    if speed == SimScalar::ZERO {
        return Ok(position);
    }
    let max_push = speed
        .checked_mul(SOFT_PUSH_SPEED_SHARE)
        .ok_or(MovementMathError::Unrepresentable)?
        .max(SOFT_PUSH_FLOOR);
    let push = if within_distance(push, SimVec2::ZERO, i128::from(max_push.raw())) {
        push
    } else {
        checked_vec_scale(checked_normalize(push)?, max_push)?
    };
    let proposed = checked_vec_add(position, push)?;
    let delta = checked_vec_sub(proposed, start)?;
    if within_distance(delta, SimVec2::ZERO, i128::from(speed.raw())) {
        Ok(proposed)
    } else {
        checked_vec_add(start, checked_vec_scale(checked_normalize(delta)?, speed)?)
    }
}

impl SimWorld {
    fn collision_body(&self, entry: MotionEntry, actor: &Entity) -> CollisionBody {
        CollisionBody {
            entry,
            position: actor.position,
            velocity: actor.velocity,
            owner: actor.owner,
            fixed: actor.is_immobile
                || actor.unpushable
                || self.map.structures().contains_key(&entry.id),
            holding: actor.cannot_be_pushed(),
            engaged: self.is_anchored_in_combat(actor),
            ghost: actor.is_mineral_walking || actor.status_modifiers().suppress_collision,
            idle: actor.is_idle(),
        }
    }

    /// Resolves the pair `(a, b)`, known to overlap, into the accumulators.
    fn resolve_pair(
        &mut self,
        bodies: &[CollisionBody],
        pushes: &mut [BodyPush],
        (index_a, index_b): (usize, usize),
    ) {
        let (a, b) = (&bodies[index_a], &bodies[index_b]);
        let id_a = a.entry.id;
        let (radius_a, radius_b) = (a.entry.policy.radius, b.entry.policy.radius);
        let arithmetic = radius_a.checked_add(radius_b).and_then(|combined| {
            super::checked_distance(a.position, b.position).map(|dist| (combined, dist))
        });
        let Some((combined, dist)) = arithmetic else {
            self.report_movement_invariant(MovementInvariant::InvalidState {
                entity: id_a,
                reason: "collision geometry exceeds fixed-point local push arithmetic",
            });
            return;
        };
        let friendly = a.owner == b.owner;
        let (priority_a, priority_b) = (a.entry.push_priority, b.entry.push_priority);
        let (still_a, still_b) = (a.velocity == SimVec2::ZERO, b.velocity == SimVec2::ZERO);
        let anchored_a = a.holding
            || (friendly && a.engaged && (priority_a >= priority_b || (still_a && !still_b)));
        let anchored_b = b.holding
            || (friendly && b.engaged && (priority_b >= priority_a || (still_b && !still_a)));
        let params = |body: &CollisionBody, priority, anchored| PushEntityParams {
            id: body.entry.id,
            pos: body.position,
            vel: body.velocity,
            owner: body.owner,
            push_priority: priority,
            cannot_be_pushed: anchored,
            is_holding: body.holding,
            idle: body.idle,
        };
        let mobility = |body: &CollisionBody| {
            if body.fixed {
                CollisionMobility::Fixed
            } else {
                CollisionMobility::Dynamic
            }
        };
        let displacement = compute_push_displacement_with_mobility(
            params(a, priority_a, anchored_a),
            params(b, priority_b, anchored_b),
            dist,
            combined,
            mobility(a),
            mobility(b),
        );
        let (delta_a, delta_b) = match displacement {
            Ok(displacement) => displacement,
            Err(source) => {
                self.report_movement_invariant(MovementInvariant::PushGeometry {
                    entity: id_a,
                    source,
                });
                return;
            }
        };
        let (low, high) = pushes.split_at_mut(index_b);
        let (push_a, push_b) = (&mut low[index_a], &mut high[0]);
        let result = if friendly {
            accumulate_pair(&mut push_a.friendly, &mut push_b.friendly, delta_a, delta_b)
        } else {
            accumulate_pair(&mut push_a.hostile, &mut push_b.hostile, delta_a, delta_b)
        };
        if let Err(source) = result {
            self.report_movement_invariant(MovementInvariant::Arithmetic {
                entity: id_a,
                operation: "collision pair accumulation",
                source,
            });
        }
    }

    /// Applies one kind of correction to a body, sliding along terrain where it must.
    fn apply_correction(
        &mut self,
        entry: MotionEntry,
        id: EntityId,
        proposed: Result<SimVec2, MovementMathError>,
    ) {
        let proposed = match proposed {
            Ok(position) => position,
            Err(source) => {
                self.report_movement_invariant(MovementInvariant::Arithmetic {
                    entity: id,
                    operation: "collision application",
                    source,
                });
                return;
            }
        };
        if let Some(actor) = self.entities.get_mut(&id) {
            // The whole tick's travel must be walkable, not just this correction: a unit may not
            // reach by two short hops a spot it could not have walked to in one step.
            let accepted =
                slide_target(&self.map, entry.policy, actor.position, proposed).filter(|&end| {
                    entry
                        .policy
                        .permits_segment(&self.map, actor.pos_at_tick_start, end)
                });
            if let Some(position) = accepted {
                actor.position = position;
            }
        }
    }

    /// Applies accumulated corrections, then records what each body really achieved this tick.
    fn apply_pushes(&mut self, bodies: &[CollisionBody], pushes: &[BodyPush]) {
        for (body, push) in bodies.iter().zip(pushes) {
            if body.fixed || push.friendly == SimVec2::ZERO {
                continue;
            }
            let id = body.entry.id;
            if let Some(actor) = self.entities.get(&id) {
                let proposed = soft_position(
                    actor.position,
                    actor.pos_at_tick_start,
                    push.friendly,
                    body.entry.speed,
                );
                self.apply_correction(body.entry, id, proposed);
            }
        }
        for (body, push) in bodies.iter().zip(pushes) {
            if body.fixed || push.hostile == SimVec2::ZERO {
                continue;
            }
            let id = body.entry.id;
            if let Some(actor) = self.entities.get(&id) {
                let proposed = checked_vec_add(actor.position, push.hostile);
                self.apply_correction(body.entry, id, proposed);
            }
        }
        for body in bodies {
            if let Some(actor) = self.entities.get_mut(&body.entry.id) {
                if let Ok(achieved) = checked_vec_sub(actor.position, actor.pos_at_tick_start) {
                    actor.velocity = achieved;
                }
            }
        }
    }

    /// Phase 6: Resolve circle-circle collisions, engagement anchoring, and push priority.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    pub fn resolve_collisions_and_push(&mut self) {
        // PERFORMANCE RATIONALE:
        // Why: Map lookups by entity ID inside a pair loop are the slowest thing it could do,
        // and every pair of a 400-unit crowd passes through it each tick.
        // What: Snapshot each body once into a flat record, and index those records in a grid.
        // Pair tests then touch contiguous memory, and corrections go to a parallel slice
        // instead of a map. Actors validated by this tick's movement phase are reused.
        // Trade-off: Two retained buffers sized by population, in exchange for O(N) pair work
        // with no allocation and no ordered-map traversal per contact.
        let mut entries = std::mem::take(&mut self.movement.entries);
        if !std::mem::take(&mut self.movement.entries_are_fresh) {
            entries.clear();
            let ids: Vec<EntityId> = self
                .entities
                .values()
                .filter(|e| e.takes_part_in_movement())
                .map(|e| e.id)
                .collect();
            for id in ids {
                match self.motion_entry(id) {
                    Ok(entry) => entries.push(entry),
                    Err(source) => self.report_movement_invariant(source),
                }
            }
        }
        let mut bodies = std::mem::take(&mut self.movement.bodies);
        let mut pushes = std::mem::take(&mut self.movement.pushes);
        let mut grid = std::mem::take(&mut self.movement.collision_grid);
        bodies.clear();
        let mut largest = [SimScalar::ZERO; super::BODY_CLASSES];
        for entry in &entries {
            if let Some(actor) = self.entities.get(&entry.id) {
                bodies.push(self.collision_body(*entry, actor));
                let class = entry.policy.body_class();
                largest[class] = largest[class].max(entry.policy.radius);
            }
        }
        pushes.clear();
        pushes.resize(bodies.len(), BodyPush::default());
        grid.rebuild(
            self.map.width(),
            self.map.height(),
            bodies.iter().map(|body| body.position),
        );

        for index_a in 0..bodies.len() {
            let a = bodies[index_a];
            if a.ghost {
                continue;
            }
            let policy = a.entry.policy;
            let reach = policy.radius + largest[policy.body_class()];
            let mut neighbours = std::mem::take(&mut self.movement.pair_scratch);
            neighbours.clear();
            grid.for_each_near(a.position, reach, |index_b| {
                let b = &bodies[index_b];
                if index_b > index_a
                    && !b.ghost
                    && b.entry.policy.body_class() == policy.body_class()
                    && super::circles_overlap(
                        a.position,
                        policy.radius,
                        b.position,
                        b.entry.policy.radius,
                    )
                {
                    neighbours.push(index_b);
                }
            });
            for &index_b in &neighbours {
                let touching = policy.layer != super::MovementLayer::Ground
                    || self.map.world_to_cell(a.position).ok()
                        == self.map.world_to_cell(bodies[index_b].position).ok()
                    || self.map.ground_segment_is_connected(
                        a.position,
                        bodies[index_b].position,
                        |_, _| true,
                    );
                if touching {
                    self.resolve_pair(&bodies, &mut pushes, (index_a, index_b));
                }
            }
            self.movement.pair_scratch = neighbours;
        }

        self.apply_pushes(&bodies, &pushes);
        self.movement.entries = entries;
        self.movement.bodies = bodies;
        self.movement.pushes = pushes;
        self.movement.collision_grid = grid;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_pair_keeps_both_accumulators_and_prior_contributions() {
        let mut first = SimVec2::X_AXIS;
        let mut second = SimVec2::new(SimScalar::from_raw(i64::MAX), SimScalar::ZERO);
        let before = (first, second);
        assert_eq!(
            accumulate_pair(&mut first, &mut second, SimVec2::X_AXIS, SimVec2::X_AXIS),
            Err(MovementMathError::Unrepresentable)
        );
        assert_eq!((first, second), before);
    }

    #[test]
    fn soft_correction_never_exceeds_the_units_own_speed() -> Result<(), MovementMathError> {
        let start = SimVec2::ZERO;
        let speed = SimScalar::ONE;
        let moved = soft_position(SimVec2::X_AXIS, start, SimVec2::X_AXIS, speed)?;
        assert_eq!(moved, SimVec2::X_AXIS);
        let doubled = SimScalar::from_i32(2);
        let kept = soft_position(SimVec2::X_AXIS, start, SimVec2::Y_AXIS, doubled)?;
        assert!(kept.y > SimScalar::ZERO);
        Ok(())
    }
}
