//! # Target Availability, Acquisition and Combat Retaliation
//!
//! Decides who shoots at whom. This module owns tick phase 4 (automatic acquisition), the
//! one availability gate every attack stage consults, and the reaction of units to damage.
//!
//! ## How it fits
//! Acquisition runs after vision and before movement, so pursuit acts on this tick's targets.
//! Orders it creates carry [`AttackOrigin::Acquired`]; [`crate::combat_resolution`] then fires
//! at whatever the orders and `Entity::current_target` name.
//!
//! ## Invariants Maintained
//! 1. Player orders are never rewritten: only [`AttackOrigin::Acquired`] attacks are replaced,
//!    and an explicit attack ends only when it can no longer be carried out.
//! 2. A holding or immobile unit only considers targets its weapon reaches from where it
//!    stands, and acquisition never moves a unit off a hold order.
//! 3. Candidates are ranked by [`TargetPriorityKey`], so the choice is deterministic. A unit that
//!    cannot hit its automatic target takes an equal enemy it can hit from where it stands, because
//!    bodies now block: walking on around a crowd would only lose the blows.
//! 4. Damage from a unit's own side never causes retaliation, flight or alerts.

use crate::catalog::UnitArchetype;
use crate::combat::{compute_threat_priority, TargetPriorityKey, MAX_THREAT_PRIORITY};
use crate::entity::Entity;
use crate::weapon_timing::SimulationDiagnostic;
use crate::world::SimWorld;
use sim_map::{PlayerVisionMap, SimMap};
use sim_types::{
    AttackOrigin, EntityId, PerPlayer, PlayerId, SimScalar, SimVec2, Tick, UnitOrder,
    VisibilityState,
};
use std::collections::BTreeMap;

/// World units a chased target may be farther than an equally rated rival before the unit
/// switches, so two enemies at similar distances do not make it dither between them.
const RETARGET_DISTANCE_MARGIN: SimScalar = SimScalar::ONE;

/// Radius within which a unit under attack alerts idle allies, squared, in world units.
const CALL_FOR_HELP_RADIUS_SQ: i32 = 25;

/// Distance a helpless unit flees from its attacker, in world units.
const HELPLESS_FLEE_DISTANCE: i32 = 5;

/// What acquisition may do for a unit, given its front order.
#[derive(Clone, Copy)]
enum Engagement {
    /// Idle or attack-moving: any visible enemy may become an acquired attack order.
    Seek,
    /// Holding: `current_target` tracks an enemy in reach; the order never changes.
    Hold,
    /// Under an automatic attack, which a better target may replace.
    Acquired(EntityId),
    /// Under a player's attack order, which only impossibility ends.
    Explicit(EntityId),
    /// The front order leaves no room for automatic targeting.
    Busy,
}

/// The acquisition outcome for one unit, applied after the read-only scan.
#[derive(Clone, Copy)]
pub(crate) enum Acquisition {
    /// Attack this enemy.
    Engage(EntityId),
    /// Forget a holding unit's target, which left its reach.
    ReleaseHoldTarget,
    /// End an attack order an immobile unit can never carry out.
    AbandonAttack,
}

/// Best candidates from one neighbourhood scan.
#[derive(Clone, Copy, Default)]
struct ScanResult {
    /// Top-ranked candidate.
    best: Option<(TargetPriorityKey, SimScalar)>,
    /// The unit's current target, if it is still a candidate.
    current: Option<(TargetPriorityKey, SimScalar)>,
}

impl SimWorld {
    /// Whether an entity can take damage at all: alive, in the world and not invulnerable.
    ///
    /// Area damage uses this gate alone, because a blast needs no aim: it reaches cloaked
    /// units whether or not anyone detects them.
    #[must_use]
    #[inline]
    pub(crate) fn is_damageable(target: &Entity) -> bool {
        target.is_alive() && !target.is_loaded_in_transport() && !target.is_invulnerable
    }

    /// The attack gate for an already resolved entity; see
    /// [`Self::is_entity_available_for_combat`] for its meaning.
    #[must_use]
    pub(crate) fn is_entity_targetable(
        target: &Entity,
        map: &SimMap,
        vision_maps: &PerPlayer<PlayerVisionMap>,
        attacker_owner: PlayerId,
    ) -> bool {
        if !Self::is_damageable(target) {
            return false;
        }
        !target.is_cloaked
            || Self::is_cloak_revealed(map, vision_maps, attacker_owner, target.position)
    }

    /// Whether `viewer` has detector vision over a cloaked entity at `position`.
    #[must_use]
    pub(crate) fn is_cloak_revealed(
        map: &SimMap,
        vision_maps: &PerPlayer<PlayerVisionMap>,
        viewer: PlayerId,
        position: SimVec2,
    ) -> bool {
        let Some(vision) = vision_maps.get(viewer) else {
            return false;
        };
        map.world_to_cell(position).is_ok_and(|(cx, cy)| {
            vision.visibility_at(cx, cy) == VisibilityState::Visible && vision.is_detected(cx, cy)
        })
    }

    /// Checks whether an entity is available to be tracked, acquired, or hit in combat
    /// by an attacker belonging to `attacker_owner` without whole-struct borrowing.
    #[must_use]
    pub fn is_target_available_for_combat(
        entities: &BTreeMap<EntityId, Entity>,
        map: &SimMap,
        vision_maps: &PerPlayer<PlayerVisionMap>,
        attacker_owner: PlayerId,
        target_id: EntityId,
    ) -> bool {
        entities.get(&target_id).is_some_and(|target| {
            Self::is_entity_targetable(target, map, vision_maps, attacker_owner)
        })
    }

    /// Checks whether an entity is available to be tracked, acquired, or hit in combat
    /// by an attacker belonging to `attacker_owner`.
    ///
    /// An entity is available if and only if:
    /// - It exists and is alive (`hp > 0`),
    /// - It is not loaded in a transport craft,
    /// - It is not invulnerable (e.g. stasis, phase shift),
    /// - If cloaked, the attacker's player possesses active detector vision at the entity's map cell.
    #[must_use]
    #[inline]
    pub fn is_entity_available_for_combat(
        &self,
        attacker_owner: PlayerId,
        target_id: EntityId,
    ) -> bool {
        Self::is_target_available_for_combat(
            &self.entities,
            &self.map,
            &self.vision_maps,
            attacker_owner,
            target_id,
        )
    }

    /// Triggers combat retaliation aggro and allied call-for-help when a unit sustains damage.
    ///
    /// If the victim is idle, mobile, and cannot retaliate against the attacker (e.g. ground vs air,
    /// unarmed, or attacker is untargetable/undetected), the victim automatically flees directly
    /// away from the threat origin. Damage from the victim's own side changes nothing.
    pub fn apply_combat_retaliation(&mut self, victim_id: EntityId, attacker_id: EntityId) {
        if let Err(source) = self.validate_transport_links() {
            self.report_transport_invariant(source);
            return;
        }
        self.apply_combat_retaliation_prevalidated(victim_id, attacker_id);
    }

    // Called from valid combat/hazard phases; no relation fields are mutated.
    pub(crate) fn apply_combat_retaliation_prevalidated(
        &mut self,
        victim_id: EntityId,
        attacker_id: EntityId,
    ) {
        let Some(victim) = self
            .entities
            .get(&victim_id)
            .filter(|v| !v.is_loaded_in_transport())
        else {
            return;
        };
        let Some(attacker) = self.entities.get(&attacker_id).filter(|a| a.is_alive()) else {
            return;
        };
        if attacker.owner == victim.owner {
            return;
        }
        let Ok(attacker_arch) = self.catalog.get(attacker.unit_type_id) else {
            return;
        };
        let Ok(victim_arch) = self.catalog.get(victim.unit_type_id) else {
            return;
        };

        let victim_owner = victim.owner;
        let victim_pos = victim.position;
        let attacker_pos = attacker.position;
        let attacker_targetable = self.is_attacker_visible_to(victim_owner, attacker);
        let victim_can_hit = attacker_targetable
            && victim_arch
                .weapon_against(attacker_arch.mover_type, None)
                .is_some_and(|w| w.target_flags.intersects(attacker_arch.attributes));
        let helpless = !victim_can_hit
            && matches!(victim.order_queue.front(), None | Some(UnitOrder::Idle))
            && !victim.is_holding_position
            && !victim.is_immobile
            && victim_arch.max_speed > SimScalar::ZERO;

        if helpless {
            self.flee_from_attacker(victim_id, attacker_pos);
        }
        // If attacker is not targetable, nearby allies cannot target it either
        if attacker_targetable {
            self.alert_idle_allies(victim_id, victim_owner, victim_pos, attacker_id);
        }
    }

    /// Whether `viewer` sees `attacker` well enough to shoot back at it.
    fn is_attacker_visible_to(&self, viewer: PlayerId, attacker: &Entity) -> bool {
        if attacker.is_loaded_in_transport() {
            return false;
        }
        let Some(vision) = self.vision_maps.get(viewer) else {
            return true;
        };
        self.map
            .world_to_cell(attacker.position)
            .is_ok_and(|(cx, cy)| {
                vision.visibility_at(cx, cy) == VisibilityState::Visible
                    && (!attacker.is_cloaked || vision.is_detected(cx, cy))
            })
    }

    /// Sends a helpless victim directly away from its attacker.
    fn flee_from_attacker(&mut self, victim_id: EntityId, attacker_pos: SimVec2) {
        let flee = self.compute_entity_flee_destination_prevalidated(
            victim_id,
            attacker_pos,
            SimScalar::from_i32(HELPLESS_FLEE_DISTANCE),
        );
        match flee {
            Err(source) => self
                .tick_diagnostics
                .push(SimulationDiagnostic::MovementInvariant {
                    tick: Tick(self.current_tick),
                    source,
                }),
            Ok(Some(flee_target)) => {
                if let Some(victim) = self.entities.get_mut(&victim_id) {
                    victim.order_queue.clear();
                    victim.order_queue.push_back(UnitOrder::Move {
                        target: flee_target,
                    });
                    victim.current_funnel_waypoints.clear();
                    victim.current_target = None;
                }
            }
            Ok(None) => {}
        }
    }

    /// Orders the victim and idle armed allies nearby to attack `attacker_id`.
    fn alert_idle_allies(
        &mut self,
        victim_id: EntityId,
        owner: PlayerId,
        victim_pos: SimVec2,
        attacker_id: EntityId,
    ) {
        let Some(attacker_arch) = self
            .entities
            .get(&attacker_id)
            .and_then(|a| self.catalog.get(a.unit_type_id).ok())
        else {
            return;
        };
        let call_radius_sq = SimScalar::from_i32(CALL_FOR_HELP_RADIUS_SQ);
        let mut alerted = std::mem::take(&mut self.scratch_neighbors);
        alerted.clear();
        for (&id, entity) in &self.entities {
            if entity.owner != owner
                || !entity.is_alive()
                || entity.is_holding_position
                || entity.is_immobile
                || entity.is_loaded_in_transport()
                || !matches!(entity.order_queue.front(), None | Some(UnitOrder::Idle))
                || (id != victim_id && entity.position.distance_sq(victim_pos) > call_radius_sq)
            {
                continue;
            }
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };
            let can_answer = arch
                .weapon_against(attacker_arch.mover_type, entity.active_siege_weapon())
                .is_some_and(|w| w.target_flags.intersects(attacker_arch.attributes));
            if can_answer {
                alerted.push(id);
            }
        }
        for &unit_id in &alerted {
            if let Some(unit) = self.entities.get_mut(&unit_id) {
                unit.order_queue.clear();
                unit.order_queue.push_back(UnitOrder::AttackTarget {
                    target: attacker_id,
                    origin: AttackOrigin::Acquired,
                });
                unit.current_target = Some(attacker_id);
                unit.current_funnel_waypoints.clear();
            }
        }
        self.scratch_neighbors = alerted;
    }

    /// Phase 4: Deterministic target acquisition.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    pub fn acquire_targets(&mut self) {
        // Populate spatial hash grid with current entity positions
        self.spatial_hash.clear();
        for (id, entity) in &self.entities {
            if !entity.is_loaded_in_transport() {
                self.spatial_hash.insert(*id, entity.position);
            }
        }

        let mut acquisitions = std::mem::take(&mut self.scratch_acquisitions);
        acquisitions.clear();
        let mut neighbors = std::mem::take(&mut self.scratch_neighbors);
        let mut candidates = std::mem::take(&mut self.scratch_targets);
        for (&id, entity) in &self.entities {
            if entity.channeling_remaining_ticks > 0
                || entity.is_loaded_in_transport()
                || !entity.is_completed
            {
                continue;
            }
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };
            let decision = self.decide_acquisition(entity, arch, &mut neighbors, &mut candidates);
            if let Some(decision) = decision {
                acquisitions.push((id, decision));
            }
        }
        self.scratch_neighbors = neighbors;
        self.scratch_targets = candidates;

        for &(unit_id, decision) in &acquisitions {
            if let Some(unit) = self.entities.get_mut(&unit_id) {
                match decision {
                    Acquisition::Engage(target) => Self::engage(unit, target),
                    Acquisition::ReleaseHoldTarget => unit.current_target = None,
                    Acquisition::AbandonAttack => {
                        unit.order_queue.pop_front();
                        unit.current_target = None;
                    }
                }
            }
        }
        self.scratch_acquisitions = acquisitions;
    }

    /// Classifies what acquisition may do for `entity`.
    fn engagement_of(entity: &Entity) -> Engagement {
        match entity.order_queue.front() {
            None | Some(UnitOrder::Idle | UnitOrder::AttackMove { .. }) => Engagement::Seek,
            Some(UnitOrder::HoldPosition) => Engagement::Hold,
            Some(UnitOrder::AttackTarget { target, origin }) => match origin {
                AttackOrigin::Acquired => Engagement::Acquired(*target),
                AttackOrigin::Explicit => Engagement::Explicit(*target),
            },
            Some(_) => Engagement::Busy,
        }
    }

    /// Whether `entity`'s weapon can engage `target` from where it stands right now.
    fn reaches_from_here(&self, entity: &Entity, arch: &UnitArchetype, target: &Entity) -> bool {
        let Ok(target_arch) = self.catalog.get(target.unit_type_id) else {
            return false;
        };
        let Some(weapon) =
            arch.weapon_against(target_arch.mover_type, entity.active_siege_weapon())
        else {
            return false;
        };
        let radii = arch.collision_radius + target_arch.collision_radius;
        let reach = self.effective_weapon_range(weapon, &arch.slug, entity.owner) + radii;
        let distance = entity.position.distance(target.position);
        distance <= reach
            && (weapon.min_range <= SimScalar::ZERO || distance >= weapon.min_range + radii)
    }

    /// Scans for a better target and returns what should change, without mutating.
    ///
    // PERFORMANCE RATIONALE:
    // Why: The neighbourhood scan is the costliest step of this phase, and in a battle nearly
    // every unit is under an automatic attack.
    // What: A unit already engaging a target of the highest possible threat skips the scan,
    // because nothing can outrank it and an in-reach target is never traded for an equal.
    // Trade-off: One extra catalog lookup per engaged unit, for far fewer scans.
    fn decide_acquisition(
        &self,
        entity: &Entity,
        arch: &UnitArchetype,
        neighbors: &mut Vec<EntityId>,
        candidates: &mut Vec<(TargetPriorityKey, EntityId)>,
    ) -> Option<Acquisition> {
        let engagement = Self::engagement_of(entity);
        let current = match engagement {
            Engagement::Busy => return None,
            Engagement::Seek => None,
            Engagement::Hold => entity.current_target,
            Engagement::Acquired(target) | Engagement::Explicit(target) => Some(target),
        };
        let reachable_current = current
            .and_then(|id| self.entities.get(&id))
            .filter(|target| {
                Self::is_entity_targetable(target, &self.map, &self.vision_maps, entity.owner)
                    && self.reaches_from_here(entity, arch, target)
            });

        if matches!(engagement, Engagement::Explicit(_)) {
            let abandoned = entity.is_immobile && reachable_current.is_none();
            return abandoned.then_some(Acquisition::AbandonAttack);
        }
        let top_threat = reachable_current.is_some_and(|target| {
            self.catalog
                .get(target.unit_type_id)
                .is_ok_and(|target_arch| {
                    compute_threat_priority(target_arch, target.order_queue.front())
                        >= MAX_THREAT_PRIORITY
                })
        });
        if top_threat {
            return None;
        }

        let stationary = entity.is_immobile || matches!(engagement, Engagement::Hold);
        let scan = self.scan_candidates(entity, arch, current, stationary, neighbors, candidates);
        let Some((best, best_distance)) = scan.best else {
            return match engagement {
                Engagement::Hold if current.is_some() && reachable_current.is_none() => {
                    Some(Acquisition::ReleaseHoldTarget)
                }
                Engagement::Acquired(_) if entity.is_immobile && reachable_current.is_none() => {
                    Some(Acquisition::AbandonAttack)
                }
                _ => None,
            };
        };
        if let Some((held, held_distance)) = scan.current {
            let in_reach_now = reachable_current.is_none()
                && self
                    .entities
                    .get(&best.entity_id)
                    .is_some_and(|enemy| self.reaches_from_here(entity, arch, enemy));
            let keeps = best.threat_priority <= held.threat_priority
                && !in_reach_now
                && (reachable_current.is_some()
                    || held_distance <= best_distance + RETARGET_DISTANCE_MARGIN);
            if keeps {
                return None;
            }
        }
        Some(Acquisition::Engage(best.entity_id))
    }

    /// Ranks every enemy `entity` could attack, returning the best and the current target.
    ///
    /// With `stationary` set, only enemies inside weapon reach qualify.
    fn scan_candidates(
        &self,
        entity: &Entity,
        arch: &UnitArchetype,
        current: Option<EntityId>,
        stationary: bool,
        neighbors: &mut Vec<EntityId>,
        candidates: &mut Vec<(TargetPriorityKey, EntityId)>,
    ) -> ScanResult {
        // The ranking key ends in the entity ID, so visiting order cannot change the winner.
        self.spatial_hash
            .query_radius_unordered(entity.position, arch.scan_range, neighbors);
        candidates.clear();
        let mut result = ScanResult::default();
        let vision = self.vision_maps.get(entity.owner);

        for &neighbor_id in neighbors.iter() {
            let Some(neighbor) = self.entities.get(&neighbor_id) else {
                continue;
            };
            if neighbor.owner == entity.owner
                || entity.movement.abandoned_target == Some(neighbor_id)
                || !Self::is_entity_targetable(neighbor, &self.map, &self.vision_maps, entity.owner)
            {
                continue;
            }
            let dist = entity.position.distance(neighbor.position);
            if dist > arch.scan_range {
                continue;
            }
            let Ok((cx, cy)) = self.map.world_to_cell(neighbor.position) else {
                continue;
            };
            if vision.is_some_and(|v| v.visibility_at(cx, cy) != VisibilityState::Visible) {
                continue;
            }
            let Ok(target_arch) = self.catalog.get(neighbor.unit_type_id) else {
                continue;
            };
            let Some(weapon) =
                arch.weapon_against(target_arch.mover_type, entity.active_siege_weapon())
            else {
                continue;
            };
            if !weapon.target_flags.intersects(target_arch.attributes) {
                continue;
            }
            let radii = arch.collision_radius + target_arch.collision_radius;
            if weapon.min_range > SimScalar::ZERO && dist < weapon.min_range + radii {
                continue;
            }
            if stationary && !self.reaches_from_here(entity, arch, neighbor) {
                continue;
            }

            let key = TargetPriorityKey {
                threat_priority: compute_threat_priority(target_arch, neighbor.order_queue.front()),
                distance_sq: entity.position.distance_sq(neighbor.position),
                entity_id: neighbor_id,
            };
            if Some(neighbor_id) == current {
                result.current = Some((key, dist));
            }
            if result.best.is_none_or(|(best, _)| key < best) {
                result.best = Some((key, dist));
            }
        }
        result
    }

    /// Commits an acquired target to `unit` according to its front order.
    fn engage(unit: &mut Entity, target: EntityId) {
        unit.current_target = Some(target);
        let acquired = UnitOrder::AttackTarget {
            target,
            origin: AttackOrigin::Acquired,
        };
        match unit.order_queue.front_mut() {
            Some(UnitOrder::AttackMove { .. }) => unit.order_queue.push_front(acquired),
            Some(order @ UnitOrder::AttackTarget { .. }) => *order = acquired,
            None | Some(UnitOrder::Idle) => {
                unit.order_queue.clear();
                unit.order_queue.push_back(acquired);
            }
            // A holder only tracks `current_target`: its order must survive.
            Some(_) => return,
        }
        unit.current_funnel_waypoints.clear();
        unit.movement.restart();
    }
}
