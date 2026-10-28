//! Atomic per-actor locomotion proposals, using current authored movement policy.
//! Preparation borrows existing routes/orders and owns only newly generated paths.
//! Scalar pose edits and route/order edits commit together after all fallible work;
//! earlier successful actors remain committed when a later actor is diagnosed.
use super::math::{checked_vec_add, checked_vec_scale, distance};
use super::{MotionPolicy, MovementInvariant, MovementLayer, MovementMathError};
use crate::entity::Entity;
use crate::steering::{
    check_cluster_arrival_with, compute_unified_local_steering, SteeringObstacle,
};
use crate::{SimWorld, WeaponPhase};
use sim_map::{PathRequestWithVision, UnitClearance};
use sim_types::{AnimState, EntityId, SimAngle, SimScalar, SimVec2, UnitOrder};

#[derive(Clone, Copy)]
pub(crate) struct MotionEntry {
    pub(crate) id: EntityId,
    pub(crate) policy: MotionPolicy,
    speed: SimScalar,
}

struct MotionProposal {
    position: SimVec2,
    velocity: SimVec2,
    heading: SimAngle,
    turret_angle: SimAngle,
    anim: AnimState,
    clear_route: bool,
    replacement: Option<Vec<SimVec2>>,
    remove_prefix: usize,
    pop_order: bool,
}
impl MotionProposal {
    fn new(actor: &Entity, policy: MotionPolicy) -> Self {
        Self {
            position: actor.position,
            velocity: actor.velocity,
            heading: actor.heading,
            turret_angle: actor.turret_angle,
            anim: actor.current_anim,
            clear_route: policy.layer == MovementLayer::Air,
            replacement: None,
            remove_prefix: 0,
            pop_order: false,
        }
    }
    fn route<'a>(&'a self, actor: &'a Entity) -> &'a [SimVec2] {
        let route = match self.replacement.as_ref() {
            Some(route) => route.as_slice(),
            None if self.clear_route => &[],
            None => actor.current_funnel_waypoints.as_slice(),
        };
        &route[self.remove_prefix.min(route.len())..]
    }
    fn finish(&mut self) {
        self.discard_order();
        self.anim = AnimState::Idle;
    }
    fn discard_order(&mut self) {
        self.velocity = SimVec2::ZERO;
        self.pop_order = true;
        self.clear_route = true;
        self.replacement = None;
        self.remove_prefix = 0;
    }
    fn commit(self, actor: &mut Entity) {
        actor.pos_at_tick_start = actor.position;
        actor.position = self.position;
        actor.velocity = self.velocity;
        actor.heading = self.heading;
        actor.turret_angle = self.turret_angle;
        actor.current_anim = self.anim;
        if let Some(route) = self.replacement {
            actor.current_funnel_waypoints = route;
        } else if self.clear_route {
            actor.current_funnel_waypoints.clear();
        }
        if self.remove_prefix > 0 {
            let count = self.remove_prefix.min(actor.current_funnel_waypoints.len());
            actor.current_funnel_waypoints.drain(..count);
        }
        if self.pop_order {
            actor.order_queue.pop_front();
        }
    }
}

pub(crate) type CastRequest = (EntityId, u8, Option<SimVec2>, Option<EntityId>);
enum MotionAction {
    Commit(MotionProposal),
    Pursue(EntityId),
    Cast(CastRequest),
}
fn arithmetic(id: EntityId, source: MovementMathError) -> MovementInvariant {
    MovementInvariant::Arithmetic {
        entity: id,
        operation: "prepare locomotion",
        source,
    }
}

impl SimWorld {
    pub(crate) fn motion_entry(&self, id: EntityId) -> Result<MotionEntry, MovementInvariant> {
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "movement actor is absent",
            })?;
        let policy = MotionPolicy::resolve(&self.catalog, &self.map, actor)?;
        if self.map.structures().contains_key(&id) && !actor.is_immobile {
            return Err(MovementInvariant::InvalidState {
                entity: id,
                reason: "registered foundation cannot author self-motion without atomic relocation",
            });
        }
        if !policy.contains(&self.map, actor.position) {
            return Err(MovementInvariant::InvalidState {
                entity: id,
                reason: "actor is outside its current full-radius movement domain",
            });
        }
        if !actor.is_immobile {
            policy.validate_origin(&self.map, actor)?;
        }
        let arch = self
            .catalog
            .get(actor.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(id))?;
        let speed = self
            .effective_max_speed(arch, actor.owner)
            .and_then(|base| {
                base.checked_mul(actor.speed_multiplier()?)
                    .ok_or(MovementMathError::Unrepresentable)
            })
            .map_err(|e| arithmetic(id, e))?;
        Ok(MotionEntry { id, policy, speed })
    }

    pub(crate) fn advance_checked_movement(&mut self) {
        // PERFORMANCE RATIONALE:
        // Why: Proposals must not clone an Entity or allocate an ID vector per actor.
        // What: Rebuild retained validated entries/obstacles once and borrow each
        // actor while preparing compact scalar edits and optional new path ownership.
        // Trade-off: Buff/radius validation is phase-entry work; scratch capacities
        // grow at population high-water marks, while pathfinding retains its costs.
        let mut entries = std::mem::take(&mut self.movement.entries);
        entries.clear();
        let mut ids = std::mem::take(&mut self.movement.phase_ids);
        ids.clear();
        let mut pursuits = std::mem::take(&mut self.movement.pursuits);
        pursuits.clear();
        let mut casts = std::mem::take(&mut self.movement.casts);
        casts.clear();
        for layer in &mut self.movement.obstacles {
            layer.clear();
        }
        ids.extend(
            self.entities
                .values()
                .filter(|e| e.is_alive() && !e.is_loaded_in_transport())
                .map(|e| e.id),
        );
        for &id in &ids {
            match self.motion_entry(id) {
                Ok(entry) => {
                    if let Some(actor) = self.entities.get(&id) {
                        self.movement.obstacles[entry.policy.layer.index()].push(
                            SteeringObstacle {
                                id,
                                pos: actor.position,
                                radius: entry.policy.radius,
                                velocity: actor.velocity,
                            },
                        );
                    }
                    entries.push(entry);
                }
                Err(error) => self.report_movement_invariant(error),
            }
        }
        for &entry in &entries {
            match self.prepare_actor_motion(entry) {
                Ok(MotionAction::Commit(proposal)) => {
                    if let Some(actor) = self.entities.get_mut(&entry.id) {
                        proposal.commit(actor);
                    }
                }
                Ok(MotionAction::Pursue(target)) => pursuits.push((entry, target)),
                Ok(MotionAction::Cast(cast)) => casts.push((entry, cast)),
                Err(error) => self.report_movement_invariant(error),
            }
        }
        for (entry, (_, ability, pos, target)) in casts.drain(..) {
            let Some(origin) = self.entities.get(&entry.id).map(|actor| actor.position) else {
                self.report_movement_invariant(MovementInvariant::InvalidState {
                    entity: entry.id,
                    reason: "prepared cast actor disappeared",
                });
                continue;
            };
            match self.try_execute_ability_prevalidated(entry.id, ability, pos, target) {
                Ok(_) => {
                    if let Some(actor) = self.entities.get_mut(&entry.id) {
                        actor.pos_at_tick_start = origin;
                        if entry.policy.layer == MovementLayer::Air {
                            actor.current_funnel_waypoints.clear();
                        }
                        actor.order_queue.pop_front();
                    }
                }
                Err(source) => self.report_movement_invariant(source),
            }
        }
        for (entry, target) in pursuits.drain(..) {
            match self.prepare_pursuit(entry, target) {
                Ok(proposal) => {
                    if let Some(actor) = self.entities.get_mut(&entry.id) {
                        proposal.commit(actor);
                    }
                }
                Err(error) => self.report_movement_invariant(error),
            }
        }
        self.movement.entries = entries;
        self.movement.phase_ids = ids;
        self.movement.pursuits = pursuits;
        self.movement.casts = casts;
    }

    fn prepare_actor_motion(
        &mut self,
        entry: MotionEntry,
    ) -> Result<MotionAction, MovementInvariant> {
        let id = entry.id;
        let policy = entry.policy;
        let speed = entry.speed;
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "validated actor disappeared",
            })?;
        let mut proposal = MotionProposal::new(actor, policy);
        if actor.is_holding_position || actor.is_immobile || actor.channeling_remaining_ticks > 0 {
            proposal.clear_route = false;
            proposal.velocity = SimVec2::ZERO;
            return Ok(MotionAction::Commit(proposal));
        }
        let arch = self
            .catalog
            .get(actor.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(id))?;
        let has_turret = actor
            .siege_weapon
            .as_ref()
            .or(arch.ground_weapon.as_ref())
            .or(arch.weapon.as_ref())
            .is_some_and(|w| w.has_turret);
        if matches!(actor.weapon_phase, WeaponPhase::Windup { .. }) && !has_turret {
            proposal.velocity = SimVec2::ZERO;
            return Ok(MotionAction::Commit(proposal));
        }
        let target = match actor.order_queue.front() {
            Some(UnitOrder::Move { target } | UnitOrder::AttackMove { target }) => *target,
            Some(UnitOrder::AttackTarget { target }) => return Ok(MotionAction::Pursue(*target)),
            Some(UnitOrder::CastAbility {
                ability_index,
                target_pos,
                target_entity,
            }) => {
                return Ok(MotionAction::Cast((
                    id,
                    *ability_index,
                    *target_pos,
                    *target_entity,
                )))
            }
            _ => {
                proposal.velocity = SimVec2::ZERO;
                if proposal.anim == AnimState::Walk {
                    proposal.anim = AnimState::Idle;
                }
                return Ok(MotionAction::Commit(proposal));
            }
        };
        if !policy.contains(&self.map, target) {
            return Err(MovementInvariant::InvalidState {
                entity: id,
                reason: "active movement order is outside the current full-radius domain",
            });
        }
        let owner = usize::from(actor.owner.0);
        let req = PathRequestWithVision {
            start: actor.position,
            goal: target,
            clearance: if policy.radius <= SimScalar::from_num(0.4) {
                UnitClearance::Small
            } else if policy.radius <= SimScalar::from_num(0.8) {
                UnitClearance::Medium
            } else {
                UnitClearance::Large
            },
            vision: self
                .vision_aware_pathfinding
                .then_some(&self.vision_maps[owner]),
            block_unexplored_ramps: false,
            known_structures: self
                .vision_aware_pathfinding
                .then_some(&self.known_blocked_cells[owner]),
        };
        if policy.layer == MovementLayer::Ground {
            let mut previous = actor.position;
            let valid = actor.current_funnel_waypoints.iter().all(|&point| {
                let valid = self
                    .pathfinder
                    .segment_is_passable(&self.map, &req, previous, point);
                previous = point;
                valid
            });
            if !valid {
                proposal.clear_route = true;
            }
        }
        let obstacles = &self.movement.obstacles[policy.layer.index()];
        if proposal.route(actor).is_empty() {
            if distance(actor.position, target).map_err(|e| arithmetic(id, e))? <= speed
                && policy.permits_segment(&self.map, actor.position, target)
            {
                proposal.position = target;
                proposal.finish();
                return Ok(MotionAction::Commit(proposal));
            }
            // Arrival needs a local terrain path through its settled witness.
            // This is not displacement/placement: occupied cells retain their
            // underlying terrain, preserving settlement near an occupied goal.
            // Both legs use the shared ramp/diagonal predicate without consulting
            // fog or hidden structure knowledge. Air retains geometric arrival.
            if check_cluster_arrival_with(
                Some(id),
                actor.position,
                policy.radius,
                target,
                obstacles,
                |witness| {
                    policy.layer == MovementLayer::Air
                        || (self.map.ground_segment_is_connected(
                            actor.position,
                            witness.pos,
                            |_, _| true,
                        ) && self.map.ground_segment_is_connected(
                            witness.pos,
                            target,
                            |_, _| true,
                        ))
                },
            )
            .map_err(|e| arithmetic(id, e))?
            {
                proposal.finish();
                return Ok(MotionAction::Commit(proposal));
            }
            if policy.layer == MovementLayer::Ground {
                if let Ok(path) = self.pathfinder.find_path_with_vision(&self.map, &req) {
                    if let Some(first) = path.waypoints.first() {
                        if distance(*first, actor.position).map_err(|e| arithmetic(id, e))?
                            <= SimScalar::from_num(0.2)
                        {
                            proposal.remove_prefix = 1;
                        }
                    }
                    proposal.replacement = Some(path.waypoints);
                }
            }
        }
        let next = proposal.route(actor).first().copied().unwrap_or(target);
        if distance(actor.position, next).map_err(|e| arithmetic(id, e))? <= speed
            && policy.permits_segment(&self.map, actor.position, next)
        {
            proposal.position = next;
            if !proposal.route(actor).is_empty() {
                proposal.remove_prefix += 1;
            }
            if proposal.route(actor).is_empty()
                && distance(next, target).map_err(|e| arithmetic(id, e))? <= speed
                && policy.permits_segment(&self.map, next, target)
            {
                proposal.position = target;
                proposal.finish();
            } else {
                proposal.anim = AnimState::Walk;
            }
        } else {
            let direction = compute_unified_local_steering(
                id,
                actor.position,
                policy.radius,
                next,
                None,
                obstacles,
            )
            .map_err(|e| arithmetic(id, e))?;
            prepare_slide(
                &self.map,
                policy,
                actor,
                &mut proposal,
                checked_vec_scale(direction, speed).map_err(|e| arithmetic(id, e))?,
            )
            .map_err(|e| arithmetic(id, e))?;
        }
        Ok(MotionAction::Commit(proposal))
    }

    fn prepare_pursuit(
        &self,
        entry: MotionEntry,
        target_id: EntityId,
    ) -> Result<MotionProposal, MovementInvariant> {
        let id = entry.id;
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "validated pursuer disappeared",
            })?;
        let mut proposal = MotionProposal::new(actor, entry.policy);
        if !self.is_entity_available_for_combat(actor.owner, target_id) {
            proposal.discard_order();
            return Ok(proposal);
        }
        let target = self
            .entities
            .get(&target_id)
            .ok_or(MovementInvariant::InvalidState {
                entity: target_id,
                reason: "available target disappeared",
            })?;
        let target_policy = MotionPolicy::resolve(&self.catalog, &self.map, target)?;
        let arch = self
            .catalog
            .get(actor.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(id))?;
        let target_arch = self
            .catalog
            .get(target.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(target_id))?;
        let Some(weapon) = arch.weapon_against(
            target_arch.mover_type,
            actor
                .is_sieged
                .then_some(actor.siege_weapon.as_ref())
                .flatten(),
        ) else {
            proposal.discard_order();
            return Ok(proposal);
        };
        let delta = self.player_upgrades[usize::from(actor.owner.0)]
            .unit_stat_modifiers
            .get(&arch.slug)
            .map_or(SimScalar::ZERO, |m| m.weapon_range_delta);
        let max = weapon
            .max_range
            .checked_add(delta)
            .ok_or_else(|| arithmetic(id, MovementMathError::Unrepresentable))?;
        if max < SimScalar::ZERO || weapon.min_range < SimScalar::ZERO {
            return Err(arithmetic(id, MovementMathError::InvalidDistance));
        }
        let radii = i128::from(entry.policy.radius.raw()) + i128::from(target_policy.radius.raw());
        let max = i128::from(max.raw()) + radii;
        let min = if weapon.min_range > SimScalar::ZERO {
            i128::from(weapon.min_range.raw()) + radii
        } else {
            0
        };
        let separation = i128::from(
            distance(actor.position, target.position)
                .map_err(|e| arithmetic(id, e))?
                .raw(),
        );
        if separation <= max && separation >= min {
            proposal.velocity = SimVec2::ZERO;
            let direction = super::math::direction(target.position, actor.position)
                .map_err(|e| arithmetic(id, e))?;
            let angle = SimAngle::atan2(direction.y, direction.x);
            if weapon.has_turret {
                proposal.turret_angle = angle;
            } else {
                proposal.heading = angle;
            }
            proposal.anim = if matches!(
                actor.weapon_phase,
                WeaponPhase::Windup { .. } | WeaponPhase::Backswing { .. }
            ) {
                AnimState::Attack
            } else {
                AnimState::Idle
            };
        } else {
            let direction = compute_unified_local_steering(
                id,
                actor.position,
                entry.policy.radius,
                target.position,
                Some(target_id),
                &self.movement.obstacles[entry.policy.layer.index()],
            )
            .map_err(|e| arithmetic(id, e))?;
            prepare_slide(
                &self.map,
                entry.policy,
                actor,
                &mut proposal,
                checked_vec_scale(direction, entry.speed).map_err(|e| arithmetic(id, e))?,
            )
            .map_err(|e| arithmetic(id, e))?;
        }
        Ok(proposal)
    }
}

fn prepare_slide(
    map: &sim_map::SimMap,
    policy: MotionPolicy,
    actor: &Entity,
    proposal: &mut MotionProposal,
    velocity: SimVec2,
) -> Result<(), MovementMathError> {
    let next = checked_vec_add(actor.position, velocity)?;
    let candidate = if policy.permits_segment(map, actor.position, next) {
        Some((next, velocity))
    } else {
        let x = SimVec2::new(next.x, actor.position.y);
        let y = SimVec2::new(actor.position.x, next.y);
        if policy.permits_segment(map, actor.position, x) {
            Some((x, SimVec2::new(velocity.x, SimScalar::ZERO)))
        } else if policy.permits_segment(map, actor.position, y) {
            Some((y, SimVec2::new(SimScalar::ZERO, velocity.y)))
        } else {
            None
        }
    };
    if let Some((position, velocity)) = candidate {
        proposal.position = position;
        proposal.velocity = velocity;
        proposal.heading = SimAngle::atan2(velocity.y, velocity.x);
        proposal.anim = AnimState::Walk;
    } else {
        proposal.velocity = SimVec2::ZERO;
    }
    Ok(())
}
