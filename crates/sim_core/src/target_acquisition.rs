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
//! 2. A holding or immobile unit, a unit tracking a target outside an attack order (plan
//!    `2026-11-23-turret-wind-up` D-4), and a unit choosing its weapon's own target inside an
//!    attack order (plan `2026-11-24-fire-rules` D-5), only considers targets its weapon reaches
//!    from where it stands. Acquisition never moves a unit off a hold order and never changes the
//!    order of a unit it tracks for: only its `current_target`, and only for a weapon whose data
//!    lets it fire under that order. An own weapon target (`Entity::weapon_target`) changes
//!    neither the order nor `current_target`, so pursuit keeps chasing the order's target; it
//!    exists only while that target cannot be worked, and for a chasing unit only with a weapon
//!    that fires while moving.
//! 3. Candidates are ranked by [`TargetPriorityKey`], so the choice is deterministic. As in
//!    `StarCraft` II (Liquipedia "Automatic Targeting"), a unit keeps a target while it is valid
//!    and inside weapon reach, unless a candidate of higher rank (threat, then attack target
//!    priority) is in scan range; once the target is lost or out of reach, the unit takes the
//!    best candidate by the whole key; among equals, the archetype's tie-break decides (closest,
//!    least turning, or closest to the previous target).
//! 4. Damage from a unit's own side never causes retaliation, flight or alerts.
//! 5. Scan range, like weapon reach, is measured between the two units' edges: the centre
//!    distance minus both collision radii (`StarCraft` II `MinScanRange`, Q0057). A weapon
//!    scans at least to its own effective range.
//! 6. Weapons strike only what their player sees ([`SimWorld::passes_weapon_filters`], 5.0.14
//!    `TargetFilters Visible`); pursuit's gate, [`SimWorld::is_entity_available_for_combat`], does
//!    not ask for vision (plan `2026-11-24-weapon-validation` D-3, B-195). An automatic attack
//!    whose target is lost to fog gives way to the best seen candidate or, with none, is dropped
//!    so that the order below it resumes (D-5, Q0106 part 2's default).

use crate::catalog::{AcquireLevel, AcquireTieBreak, UnitArchetype, WeaponSpec};
use crate::combat::{fires_without_attack_order, is_threat_to, weapon_aim, TargetPriorityKey};
use crate::entity::Entity;
use crate::weapon_timing::SimulationDiagnostic;
use crate::world::SimWorld;
use sim_map::{PlayerVisionMap, SimMap};
use sim_types::{
    AllowedMovement, AttackOrigin, EntityId, PerPlayer, PlayerId, SimAngle, SimScalar, SimVec2,
    Tick, UnitOrder, VisibilityState,
};
use std::collections::BTreeMap;

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
    /// Under an order that is no attack, with a weapon that may fire under it
    /// ([`fires_without_attack_order`]; `walking` is whether the order walks the unit):
    /// `current_target` tracks an enemy that weapon reaches from where the unit is, as for a
    /// holder; the order never changes (plan `2026-11-23-turret-wind-up` D-4).
    Track {
        /// Whether the unit's front order walks it ([`Entity::order_walks`]).
        walking: bool,
    },
    /// The front order leaves no room for automatic targeting.
    Busy,
}

/// The acquisition outcome for one unit, applied after the read-only scan.
#[derive(Clone, Copy)]
pub(crate) enum Acquisition {
    /// Attack this enemy.
    Engage(EntityId),
    /// Forget the target of a holding or tracking unit, which left its reach.
    ReleaseHoldTarget,
    /// End an attack order an immobile unit can never carry out.
    AbandonAttack,
    /// Record where the unit's current target stands, for the tie-break
    /// [`AcquireTieBreak::ClosestToPrevious`].
    RememberTarget(SimVec2),
    /// Let the unit's weapon work against this enemy while its attack order's target cannot be
    /// worked ([`Entity::weapon_target`], plan `2026-11-24-fire-rules` D-5).
    SetWeaponTarget(EntityId),
    /// Forget the unit's own weapon target.
    ReleaseWeaponTarget,
}

/// Facts about the whole catalog that every unit's acquisition reads, gathered once per tick.
#[derive(Clone, Copy)]
struct CatalogBounds {
    /// The highest attack target priority of any archetype.
    top_priority: u8,
    /// The largest collision radius of any archetype, in world units: how far beyond a scan
    /// range a candidate's centre can lie while its edge is inside it.
    largest_radius: SimScalar,
}

/// Where one unit's neighbourhood scan looks.
#[derive(Clone, Copy)]
struct ScanScope {
    /// Whether the unit stays where it stands (holding or immobile), or tracks outside an attack
    /// order, so only enemies inside its weapon reach qualify.
    stationary: bool,
    /// Under [`Engagement::Track`], whether the order walks the unit: only enemies whose plane
    /// the unit meets with a weapon that may fire under that order qualify. `None` otherwise.
    tracking: Option<bool>,
    /// For a scan for the unit's own weapon target inside an attack order: only enemies whose
    /// plane the unit meets with a weapon that may fire at other targets (and, while the unit
    /// chases, fires while moving) qualify, ranked from the order's target. `None` otherwise.
    own_target: Option<OwnTargetScan>,
    /// The catalog's largest collision radius in world units ([`CatalogBounds::largest_radius`]).
    largest_radius: SimScalar,
}

/// The licence and anchor of a scan for a unit's own weapon target (plan
/// `2026-11-24-fire-rules` D-5).
#[derive(Clone, Copy)]
struct OwnTargetScan {
    /// Whether the unit chases its order's target (mobile, the target out of reach): only a
    /// weapon that fires while moving may then fire at another enemy.
    chasing: bool,
    /// Where the order's target stands, world units: "the previous target" of the tie-break
    /// [`AcquireTieBreak::ClosestToPrevious`] (5.0.14's `ByDistanceFromTarget`).
    anchor: SimVec2,
}

impl OwnTargetScan {
    /// Whether `weapon` may fire at another enemy under this scan.
    fn licenses(self, weapon: &WeaponSpec) -> bool {
        weapon.fires_at_other_targets
            && (!self.chasing || weapon.allowed_movement == AllowedMovement::Moving)
    }
}

/// Best candidates from one neighbourhood scan.
#[derive(Clone, Copy, Default)]
struct ScanResult {
    /// Top-ranked candidate.
    best: Option<TargetPriorityKey>,
    /// The unit's current target, if it is still a candidate.
    current: Option<TargetPriorityKey>,
}

impl SimWorld {
    /// Whether an entity can take damage at all: alive, not hidden (carried, or inside a
    /// structure while it gathers; plan `2026-11-24-hidden-harvesters` D-4) and not invulnerable.
    ///
    /// Area damage uses this gate alone, because a blast needs no aim: it reaches cloaked
    /// units whether or not anyone detects them.
    #[must_use]
    #[inline]
    pub(crate) fn is_damageable(target: &Entity) -> bool {
        target.is_alive() && !target.is_hidden() && !target.is_invulnerable
    }

    /// Whether `viewer` sees the enemy `target`: a viewer without a vision map sees everything;
    /// otherwise the target's cell is `Visible` to the viewer, or the target's own attacks reveal
    /// it to the viewer (plan `2026-11-23-vision-and-detection` D-5). Concealment is not judged
    /// here: a concealed target still needs [`Self::is_cloak_revealed`].
    #[must_use]
    pub(crate) fn sees_enemy(
        map: &SimMap,
        vision_maps: &PerPlayer<PlayerVisionMap>,
        viewer: PlayerId,
        target: &Entity,
    ) -> bool {
        Self::sees_point(
            map,
            vision_maps,
            viewer,
            target.position,
            target.is_attack_revealed_to(viewer),
        )
    }

    /// The rule of [`Self::sees_enemy`] for an enemy at `position` (world units) that is
    /// `attack_revealed` to `viewer` or not, for callers holding a snapshot rather than the
    /// entity.
    #[must_use]
    pub(crate) fn sees_point(
        map: &SimMap,
        vision_maps: &PerPlayer<PlayerVisionMap>,
        viewer: PlayerId,
        position: SimVec2,
        attack_revealed: bool,
    ) -> bool {
        let Some(vision) = vision_maps.get(viewer) else {
            return true;
        };
        attack_revealed
            || map
                .world_to_cell(position)
                .is_ok_and(|(cx, cy)| vision.visibility_at(cx, cy) == VisibilityState::Visible)
    }

    /// The weapon filter: whether a weapon of `attacker_owner` may start, land or keep a strike
    /// on `target`. It passes [`Self::is_entity_targetable`] (alive, unloaded, vulnerable and, if
    /// concealed, detected) and `attacker_owner` sees it ([`Self::sees_enemy`]: its cell
    /// `Visible`, or attack-revealed). 5.0.14's `CWeapon` default `TargetFilters Visible;-`
    /// (`core` WeaponData:29), which every one of the 146 `TargetFilters` rows keeps (plan
    /// `2026-11-24-weapon-validation` D-3, S4).
    #[must_use]
    pub(crate) fn passes_weapon_filters(
        target: &Entity,
        map: &SimMap,
        vision_maps: &PerPlayer<PlayerVisionMap>,
        attacker_owner: PlayerId,
    ) -> bool {
        Self::is_entity_targetable(target, map, vision_maps, attacker_owner)
            && Self::sees_enemy(map, vision_maps, attacker_owner, target)
    }

    /// [`Self::passes_weapon_filters`] for the entity `target_id`; `false` when it does not
    /// exist.
    #[must_use]
    pub(crate) fn is_weapon_target_available(
        &self,
        attacker_owner: PlayerId,
        target_id: EntityId,
    ) -> bool {
        self.entities.get(&target_id).is_some_and(|target| {
            Self::passes_weapon_filters(target, &self.map, &self.vision_maps, attacker_owner)
        })
    }

    /// Whether `target_id` is lost to `viewer`'s fog: it exists and passes
    /// [`Self::is_entity_targetable`] but `viewer` does not see it (plan
    /// `2026-11-24-weapon-validation` D-5).
    fn is_lost_to_fog(&self, viewer: PlayerId, target_id: EntityId) -> bool {
        self.entities.get(&target_id).is_some_and(|target| {
            Self::is_entity_targetable(target, &self.map, &self.vision_maps, viewer)
                && !Self::sees_enemy(&self.map, &self.vision_maps, viewer, target)
        })
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
        !target.is_concealed()
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
    /// - It is not hidden: loaded in a transport craft, or inside a structure while it gathers,
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

    // Called from valid combat/effect phases; no relation fields are mutated.
    pub(crate) fn apply_combat_retaliation_prevalidated(
        &mut self,
        victim_id: EntityId,
        attacker_id: EntityId,
    ) {
        let Some(victim) = self.entities.get(&victim_id).filter(|v| !v.is_hidden()) else {
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

    /// The responses of spell damage (plan `2026-11-10-spellcaster-mechanics` D-9): with
    /// `flee`, an idle victim that cannot hit the caster (or has none to hit) and may move
    /// flees from `source_point`; with `acquire`, the victim and idle allies nearby attack the
    /// caster when they see it. Own and allied casters raise nothing.
    pub(crate) fn apply_spell_responses(
        &mut self,
        victim_id: EntityId,
        caster: Option<EntityId>,
        source_point: SimVec2,
        responses: sim_types::DamageResponses,
    ) {
        let Some(victim) = self
            .entities
            .get(&victim_id)
            .filter(|v| v.is_alive() && !v.is_hidden())
        else {
            return;
        };
        let Ok(victim_arch) = self.catalog.get(victim.unit_type_id) else {
            return;
        };
        let attacker = caster
            .and_then(|id| self.entities.get(&id).map(|a| (id, a)))
            .filter(|(_, a)| a.is_alive() && a.owner != victim.owner);
        let visible = attacker.filter(|(_, a)| self.is_attacker_visible_to(victim.owner, a));
        let can_hit = visible.is_some_and(|(_, a)| {
            self.catalog.get(a.unit_type_id).is_ok_and(|attacker_arch| {
                victim_arch
                    .weapon_against(attacker_arch.mover_type, None)
                    .is_some_and(|w| w.target_flags.intersects(attacker_arch.attributes))
            })
        });
        let helpless = !can_hit
            && matches!(victim.order_queue.front(), None | Some(UnitOrder::Idle))
            && !victim.is_holding_position
            && !victim.is_immobile
            && victim_arch.max_speed > SimScalar::ZERO;
        let owner = victim.owner;
        let position = victim.position;
        let visible = visible.map(|(id, _)| id);
        if responses.flee && helpless {
            self.flee_from_attacker(victim_id, source_point);
        }
        if let (true, Some(attacker_id)) = (responses.acquire, visible) {
            self.alert_idle_allies(victim_id, owner, position, attacker_id);
        }
    }

    /// Whether `viewer` sees `attacker` well enough to shoot back at it.
    fn is_attacker_visible_to(&self, viewer: PlayerId, attacker: &Entity) -> bool {
        if attacker.is_hidden() {
            return false;
        }
        if self.vision_maps.get(viewer).is_none() {
            return true;
        }
        Self::sees_enemy(&self.map, &self.vision_maps, viewer, attacker)
            && (!attacker.is_concealed()
                || Self::is_cloak_revealed(&self.map, &self.vision_maps, viewer, attacker.position))
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
                    victim.enqueue_order(UnitOrder::Move {
                        target: flee_target,
                    });
                    victim.current_funnel_waypoints.clear();
                    victim.current_target = None;
                    victim.weapon_target = None;
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
                || entity.is_hidden()
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
                unit.weapon_target = None;
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
            if !entity.is_hidden() {
                self.spatial_hash.insert(*id, entity.position);
            }
        }

        let mut acquisitions = std::mem::take(&mut self.scratch_acquisitions);
        acquisitions.clear();
        let mut neighbors = std::mem::take(&mut self.scratch_neighbors);
        let mut candidates = std::mem::take(&mut self.scratch_targets);
        let bounds = self.catalog.archetypes().fold(
            CatalogBounds {
                top_priority: 0,
                largest_radius: SimScalar::ZERO,
            },
            |bounds, archetype| CatalogBounds {
                top_priority: bounds.top_priority.max(archetype.attack_target_priority),
                largest_radius: bounds.largest_radius.max(archetype.collision_radius),
            },
        );
        for (&id, entity) in &self.entities {
            if entity.channeling_remaining_ticks > 0
                || entity.is_action_locked()
                || entity.is_hidden()
                || !entity.is_completed
            {
                continue;
            }
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };
            let decision =
                self.decide_acquisition(entity, arch, bounds, &mut neighbors, &mut candidates);
            let order_kept = decision.is_none();
            if let Some(decision) = decision.or_else(|| self.target_to_remember(entity, arch)) {
                acquisitions.push((id, decision));
            }
            let own = self.decide_weapon_target(
                entity,
                arch,
                order_kept,
                bounds.largest_radius,
                &mut neighbors,
                &mut candidates,
            );
            if let Some(own) = own {
                acquisitions.push((id, own));
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
                        unit.weapon_target = None;
                    }
                    Acquisition::RememberTarget(position) => {
                        unit.previous_target_position = Some(position);
                    }
                    Acquisition::SetWeaponTarget(target) => unit.weapon_target = Some(target),
                    Acquisition::ReleaseWeaponTarget => unit.weapon_target = None,
                }
            }
        }
        self.scratch_acquisitions = acquisitions;
    }

    /// Where `entity`'s current target stands, if its archetype breaks ties by the previous
    /// target's position and the target still exists.
    fn target_to_remember(&self, entity: &Entity, arch: &UnitArchetype) -> Option<Acquisition> {
        if arch.acquire_tie_break != AcquireTieBreak::ClosestToPrevious {
            return None;
        }
        let target = self.entities.get(&entity.current_target?)?;
        Some(Acquisition::RememberTarget(target.position))
    }

    /// The tie-break measure of `candidate` for `entity` (see [`TargetPriorityKey::tie_break`]),
    /// given the weapon it would use against the candidate and where the previous target stands
    /// for [`AcquireTieBreak::ClosestToPrevious`] (`previous`, world units).
    fn tie_break(
        entity: &Entity,
        arch: &UnitArchetype,
        weapon: &WeaponSpec,
        candidate: &Entity,
        previous: Option<SimVec2>,
    ) -> SimScalar {
        let distance_sq = entity.position.distance_sq(candidate.position);
        match arch.acquire_tie_break {
            AcquireTieBreak::Closest => distance_sq,
            AcquireTieBreak::LeastTurning => {
                let delta = candidate.position - entity.position;
                let bearing = SimAngle::atan2(delta.y, delta.x);
                let turn = weapon_aim(entity, weapon).diff(bearing).unsigned_abs();
                SimScalar::from_i32(i32::from(turn))
            }
            AcquireTieBreak::ClosestToPrevious => previous.map_or(distance_sq, |previous| {
                previous.distance_sq(candidate.position)
            }),
        }
    }

    /// Whether `entity` acquires no target on its own: an archetype with the defensive acquire
    /// level (`StarCraft` II's workers) acquires automatically only while it attack-moves
    /// (Liquipedia "Automatic Targeting", Notes). Idle or holding, it leaves enemies alone; an
    /// attack it already carries out goes on.
    fn holds_fire(entity: &Entity, arch: &UnitArchetype, engagement: Engagement) -> bool {
        arch.acquire_level == AcquireLevel::Defensive
            && matches!(engagement, Engagement::Seek | Engagement::Hold)
            && !matches!(
                entity.order_queue.front(),
                Some(UnitOrder::AttackMove { .. })
            )
    }

    /// Classifies what acquisition may do for `entity` (of archetype `arch`).
    fn engagement_of(entity: &Entity, arch: &UnitArchetype) -> Engagement {
        match entity.order_queue.front() {
            None | Some(UnitOrder::Idle | UnitOrder::AttackMove { .. }) => Engagement::Seek,
            Some(UnitOrder::HoldPosition) => Engagement::Hold,
            Some(UnitOrder::AttackTarget { target, origin }) => match origin {
                AttackOrigin::Acquired => Engagement::Acquired(*target),
                AttackOrigin::Explicit => Engagement::Explicit(*target),
            },
            Some(_) => {
                let walking = entity.order_walks();
                let tracks = [
                    arch.ground_weapon.as_ref(),
                    arch.air_weapon.as_ref(),
                    arch.weapon.as_ref(),
                    entity.active_siege_weapon(),
                ]
                .into_iter()
                .flatten()
                .any(|weapon| fires_without_attack_order(weapon, walking));
                if tracks {
                    Engagement::Track { walking }
                } else {
                    Engagement::Busy
                }
            }
        }
    }

    /// Whether `entity` may work against `target` under `engagement`: always, except under
    /// [`Engagement::Track`], where the weapon it would use against `target`'s plane must be one
    /// that may fire under the unit's order.
    fn licensed(
        &self,
        engagement: Engagement,
        entity: &Entity,
        arch: &UnitArchetype,
        target: &Entity,
    ) -> bool {
        let Engagement::Track { walking } = engagement else {
            return true;
        };
        self.catalog
            .get(target.unit_type_id)
            .is_ok_and(|target_arch| {
                arch.weapon_against(target_arch.mover_type, entity.active_siege_weapon())
                    .is_some_and(|weapon| fires_without_attack_order(weapon, walking))
            })
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
        let reach = self.entity_weapon_range(entity, weapon, &arch.slug) + radii;
        let distance = entity.position.distance(target.position);
        distance <= reach
            && (weapon.min_range <= SimScalar::ZERO || distance >= weapon.min_range + radii)
    }

    /// Scans for a better target and returns what should change, without mutating.
    ///
    // PERFORMANCE RATIONALE:
    // Why: The neighbourhood scan is the costliest step of this phase, and in a battle nearly
    // every unit is under an automatic attack.
    // What: A unit already engaging an in-reach threat of the catalog's highest attack target
    // priority (`top_priority`), for its primary weapon, skips the scan, because nothing can outrank it and an in-reach
    // target is never traded for an equal.
    // Trade-off: One pass over the catalog per tick and one threat test per engaged unit, for
    // far fewer scans.
    fn decide_acquisition(
        &self,
        entity: &Entity,
        arch: &UnitArchetype,
        bounds: CatalogBounds,
        neighbors: &mut Vec<EntityId>,
        candidates: &mut Vec<(TargetPriorityKey, EntityId)>,
    ) -> Option<Acquisition> {
        let engagement = Self::engagement_of(entity, arch);
        if Self::holds_fire(entity, arch, engagement) {
            return None;
        }
        let current = match engagement {
            Engagement::Busy => return None,
            Engagement::Seek => None,
            Engagement::Hold | Engagement::Track { .. } => entity.current_target,
            Engagement::Acquired(target) | Engagement::Explicit(target) => Some(target),
        };
        let reachable_current = current
            .and_then(|id| self.entities.get(&id))
            .filter(|target| {
                Self::passes_weapon_filters(target, &self.map, &self.vision_maps, entity.owner)
                    && self.reaches_from_here(entity, arch, target)
                    && self.licensed(engagement, entity, arch, target)
            });

        if matches!(engagement, Engagement::Explicit(_)) {
            let abandoned = entity.is_immobile && reachable_current.is_none();
            return abandoned.then_some(Acquisition::AbandonAttack);
        }
        let top_threat = reachable_current.is_some_and(|target| {
            self.catalog
                .get(target.unit_type_id)
                .is_ok_and(|target_arch| {
                    target_arch.attack_target_priority >= bounds.top_priority
                        && arch.primary_weapon_reaches(target_arch.mover_type)
                        && is_threat_to(target_arch, target, arch)
                })
        });
        if top_threat {
            return None;
        }

        let stationary =
            entity.is_immobile || matches!(engagement, Engagement::Hold | Engagement::Track { .. });
        let tracking = match engagement {
            Engagement::Track { walking } => Some(walking),
            _ => None,
        };
        let scan = self.scan_candidates(
            entity,
            arch,
            current,
            ScanScope {
                stationary,
                tracking,
                own_target: None,
                largest_radius: bounds.largest_radius,
            },
            neighbors,
            candidates,
        );
        let Some(best) = scan.best else {
            return self.without_candidate(engagement, entity, reachable_current.is_some());
        };
        if current == Some(best.entity_id) {
            return None;
        }
        // A valid target in reach is kept unless a candidate outranks it.
        let outranked = scan.current.is_none_or(|held| best.rank() > held.rank());
        if reachable_current.is_some() && !outranked {
            return None;
        }
        Some(Acquisition::Engage(best.entity_id))
    }

    /// What changes for `entity` under `engagement` when its scan found no candidate, given
    /// whether its current target is still `reachable` (passes the weapon filter and is in
    /// reach): a holding or tracking unit releases a target it cannot reach; an automatic attack
    /// ends when its unit is immobile and cannot reach the target, or when the target is lost
    /// to fog (plan `2026-11-24-weapon-validation` D-5): the order below it resumes, and an idle
    /// unit stays idle. A target merely out of reach and seen is still chased.
    fn without_candidate(
        &self,
        engagement: Engagement,
        entity: &Entity,
        reachable: bool,
    ) -> Option<Acquisition> {
        match engagement {
            Engagement::Hold | Engagement::Track { .. }
                if entity.current_target.is_some() && !reachable =>
            {
                Some(Acquisition::ReleaseHoldTarget)
            }
            Engagement::Acquired(_) if entity.is_immobile && !reachable => {
                Some(Acquisition::AbandonAttack)
            }
            Engagement::Acquired(target) if self.is_lost_to_fog(entity.owner, target) => {
                Some(Acquisition::AbandonAttack)
            }
            _ => None,
        }
    }

    /// What changes for `entity`'s own weapon target ([`Entity::weapon_target`], plan
    /// `2026-11-24-fire-rules` D-5), given whether this tick's decision for its order kept the
    /// order (`order_kept`).
    ///
    /// Only a mobile unit under an attack order that acquisition keeps, with a weapon that may
    /// fire at other targets, has one. It has none while the order's target passes the weapon
    /// filter and lies in reach (the order's target first, Q0109 part 3), and none while it
    /// chases the target (mobile, target out of reach) unless such a weapon fires while moving.
    /// Otherwise the best enemy it reaches from where it stands is chosen, ranked as
    /// acquisition ranks with "the previous target" read as the order's target. An immobile
    /// unit's order on a target it cannot work ends instead (rule 6), so it never has one.
    fn decide_weapon_target(
        &self,
        entity: &Entity,
        arch: &UnitArchetype,
        order_kept: bool,
        largest_radius: SimScalar,
        neighbors: &mut Vec<EntityId>,
        candidates: &mut Vec<(TargetPriorityKey, EntityId)>,
    ) -> Option<Acquisition> {
        let release = entity
            .weapon_target
            .is_some()
            .then_some(Acquisition::ReleaseWeaponTarget);
        let order_target = match Self::engagement_of(entity, arch) {
            Engagement::Acquired(target) | Engagement::Explicit(target) if order_kept => target,
            _ => return release,
        };
        let weapons = || {
            [
                arch.ground_weapon.as_ref(),
                arch.air_weapon.as_ref(),
                arch.weapon.as_ref(),
                entity.active_siege_weapon(),
            ]
            .into_iter()
            .flatten()
        };
        let immobile = entity.is_immobile || arch.max_speed <= SimScalar::ZERO;
        if immobile || !weapons().any(|weapon| weapon.fires_at_other_targets) {
            return release;
        }
        let Some(target) = self.entities.get(&order_target) else {
            return release;
        };
        let reaches = self.reaches_from_here(entity, arch, target);
        if reaches
            && Self::passes_weapon_filters(target, &self.map, &self.vision_maps, entity.owner)
        {
            return release;
        }
        let scan = OwnTargetScan {
            chasing: !reaches,
            anchor: target.position,
        };
        if !weapons().any(|weapon| scan.licenses(weapon)) {
            return release;
        }
        let best = self
            .scan_candidates(
                entity,
                arch,
                None,
                ScanScope {
                    stationary: true,
                    tracking: None,
                    own_target: Some(scan),
                    largest_radius,
                },
                neighbors,
                candidates,
            )
            .best;
        match best {
            Some(best) if entity.weapon_target == Some(best.entity_id) => None,
            Some(best) => Some(Acquisition::SetWeaponTarget(best.entity_id)),
            None => release,
        }
    }

    /// The farthest `entity` scans beyond both edges against any candidate, in world units: its
    /// archetype's scan range, or the effective range (upgrades included) of its longest weapon,
    /// the siege weapon while sieged, when that is greater.
    fn scan_extent(&self, entity: &Entity, arch: &UnitArchetype) -> SimScalar {
        [
            arch.ground_weapon.as_ref(),
            arch.air_weapon.as_ref(),
            arch.weapon.as_ref(),
            entity.active_siege_weapon(),
        ]
        .into_iter()
        .flatten()
        .map(|weapon| self.entity_weapon_range(entity, weapon, &arch.slug))
        .fold(arch.scan_range, SimScalar::max)
    }

    /// Ranks every enemy `entity` could attack, returning the best and the current target.
    ///
    /// An enemy is a candidate when its edge lies within the scan range of `entity`'s edge: the
    /// centre distance minus both collision radii, as weapon reach is measured (`StarCraft` II
    /// `MinScanRange`, Q0057). The scan range against a candidate is the archetype's, or the
    /// effective range of the weapon `entity` would use against it when that is greater: in
    /// `StarCraft` II a weapon scans to its `MinScanRange` or its `Range`, whichever is greater
    /// (audit §11.1), so a sieged unit scans as far as its siege weapon reaches. The catalog's
    /// largest collision radius in `scope` bounds how far beyond that range a candidate's centre
    /// can lie. With `scope.stationary` set, only enemies inside weapon reach qualify.
    fn scan_candidates(
        &self,
        entity: &Entity,
        arch: &UnitArchetype,
        current: Option<EntityId>,
        scope: ScanScope,
        neighbors: &mut Vec<EntityId>,
        candidates: &mut Vec<(TargetPriorityKey, EntityId)>,
    ) -> ScanResult {
        // The ranking key ends in the entity ID, so visiting order cannot change the winner.
        self.spatial_hash.query_radius_unordered(
            entity.position,
            self.scan_extent(entity, arch) + arch.collision_radius + scope.largest_radius,
            neighbors,
        );
        candidates.clear();
        let mut result = ScanResult::default();

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
            if self.map.world_to_cell(neighbor.position).is_err()
                || !Self::sees_enemy(&self.map, &self.vision_maps, entity.owner, neighbor)
            {
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
            let radii = arch.collision_radius + target_arch.collision_radius;
            let scan_range = arch
                .scan_range
                .max(self.entity_weapon_range(entity, weapon, &arch.slug));
            if dist > scan_range + radii {
                continue;
            }
            if !weapon.target_flags.intersects(target_arch.attributes) {
                continue;
            }
            if scope
                .tracking
                .is_some_and(|walking| !fires_without_attack_order(weapon, walking))
                || scope.own_target.is_some_and(|own| !own.licenses(weapon))
            {
                continue;
            }
            if weapon.min_range > SimScalar::ZERO && dist < weapon.min_range + radii {
                continue;
            }
            if scope.stationary && !self.reaches_from_here(entity, arch, neighbor) {
                continue;
            }

            let key = TargetPriorityKey {
                threat: is_threat_to(target_arch, neighbor, arch),
                attack_target_priority: target_arch.attack_target_priority,
                primary: arch.primary_weapon_reaches(target_arch.mover_type),
                tie_break: Self::tie_break(
                    entity,
                    arch,
                    weapon,
                    neighbor,
                    scope
                        .own_target
                        .map_or(entity.previous_target_position, |own| Some(own.anchor)),
                ),
                entity_id: neighbor_id,
            };
            if Some(neighbor_id) == current {
                result.current = Some(key);
            }
            if result.best.is_none_or(|best| key < best) {
                result.best = Some(key);
            }
        }
        result
    }

    /// Commits an acquired target to `unit` according to its front order.
    fn engage(unit: &mut Entity, target: EntityId) {
        unit.current_target = Some(target);
        unit.weapon_target = None;
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

#[cfg(test)]
mod weapon_filter_tests {
    //! Plan `2026-11-24-weapon-validation` D-3 (P-6): the weapon filter asks for vision only of
    //! a player that has a vision map; every player slot has one, so this is the only place the
    //! other case can be reached.
    use crate::test_support::world;
    use crate::world::SimWorld;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{MoverType, PlayerId, SimScalar, SimVec2, Tick, TickBundle};

    #[test]
    fn a_player_without_a_vision_map_sees_every_enemy() -> Result<(), Box<dyn std::error::Error>> {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = world(map, 9);
        let kind = world
            .catalog
            .archetypes()
            .find(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
            .map(|arch| arch.unit_type_id)
            .ok_or("the shipped catalog has a mobile ground archetype")?;
        let target = world.spawn_unit(kind, PlayerId(1), SimVec2::from_i32(28, 28))?;
        world.step(&TickBundle {
            tick: Tick(1),
            commands: Vec::new(),
            state_checksum: None,
        });
        let entity = world.entities.get(&target).ok_or("spawned")?;
        assert!(
            !SimWorld::passes_weapon_filters(entity, &world.map, &world.vision_maps, PlayerId(0)),
            "player 0, with no unit, does not see the target"
        );
        assert!(
            SimWorld::passes_weapon_filters(entity, &world.map, &world.vision_maps, PlayerId(8)),
            "an owner outside the slots has no vision map and sees everything"
        );
        assert!(world.is_weapon_target_available(PlayerId(8), target));
        assert!(!world.is_weapon_target_available(PlayerId(0), target));
        Ok(())
    }
}

#[cfg(test)]
mod chase_reading_tests {
    //! Plan `2026-11-24-fire-rules` D-4 (**verify**): acquisition reads "the unit chases its
    //! order's target" as "the target lies outside [`SimWorld::reaches_from_here`]", measured on
    //! the positions acquisition sees, while pursuit stands a unit when the target lies in its
    //! band (`Pursuit::aim_if_in_range`) on the positions the movement phase sees. For a standing
    //! target the two agree on every constructed case, the band's edge included. A target that
    //! crosses the edge in the movement phase before its attacker's pursuit runs (the legacy
    //! pipeline moves units one by one) splits them for that tick: that is the gate mismatch
    //! that starts the plan's WP6, which reads the chase from pursuit itself.
    use crate::test_support::world;
    use crate::world::SimWorld;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{
        EntityId, MoverType, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle,
        UnitOrder, UnitTypeId,
    };

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The shipped mobile ground archetype of lowest type id with a ranged ground weapon (at
    /// least 4 of reach, no minimum range), and that weapon's reach against its own kind:
    /// range plus both radii.
    fn ranged(world: &SimWorld) -> Result<(UnitTypeId, SimScalar), String> {
        world
            .catalog
            .archetypes()
            .filter(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
            .filter_map(|arch| {
                let weapon = arch.weapon_against(MoverType::Ground, None)?;
                (weapon.max_range >= SimScalar::from_i32(4)
                    && weapon.min_range == SimScalar::ZERO
                    && weapon.target_flags.intersects(arch.attributes))
                .then(|| {
                    (
                        arch.unit_type_id,
                        weapon.max_range + arch.collision_radius + arch.collision_radius,
                    )
                })
            })
            .min_by_key(|(id, _)| *id)
            .ok_or_else(|| "a shipped ranged ground archetype".to_owned())
    }

    /// One case: a unit at (10, 20) under a player's attack on a unit of its kind standing
    /// `offset` beyond its reach in +x, holding (`walk` 0) or walking towards +x (`walk` 1) or
    /// −x (`walk` −1), spawned first so that the movement phase moves it before the attacker.
    /// Returns (reaches before the tick, stood on the tick).
    fn case(offset: f64, walk: i32) -> Result<(bool, bool), Box<dyn std::error::Error>> {
        let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = world(map, 11);
        let (kind, reach) = ranged(&world)?;
        let at = SimVec2::new(
            SimScalar::from_i32(10) + reach + SimScalar::from_num(offset),
            SimScalar::from_i32(20),
        );
        let target = world.spawn_unit(kind, PlayerId(1), at)?;
        let unit = world.spawn_unit(kind, PlayerId(0), SimVec2::from_i32(10, 20))?;
        let reaches = {
            let entity = world.entities.get(&unit).ok_or("unit")?;
            let arch = world.catalog.get(kind)?;
            let other = world.entities.get(&target).ok_or("target")?;
            world.reaches_from_here(entity, arch, other)
        };
        let command = |player: u8, id: EntityId, order: UnitOrder| PlayerCommand {
            player_id: PlayerId(player),
            entities: vec![id],
            order,
            queue: false,
        };
        let target_order = match walk {
            0 => UnitOrder::HoldPosition,
            _ => UnitOrder::Move {
                target: SimVec2::from_i32(if walk > 0 { 60 } else { 2 }, 21),
            },
        };
        world.step(&TickBundle {
            tick: Tick(0),
            commands: vec![
                command(0, unit, UnitOrder::attack(target)),
                command(1, target, target_order),
            ],
            state_checksum: None,
        });
        let stood = world.entities.get(&unit).ok_or("unit")?.position == SimVec2::from_i32(10, 20);
        Ok((reaches, stood))
    }

    #[test]
    fn reach_agrees_with_pursuit_for_a_standing_target() -> TestResult {
        for offset in [-0.5, -0.01, 0.0, 0.01, 0.5] {
            let (reaches, stood) = case(offset, 0)?;
            assert_eq!(reaches, offset <= 0.0, "premise: offset {offset}");
            assert_eq!(
                reaches, stood,
                "offset {offset}: reach {reaches}, pursuit stood {stood}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_target_that_crosses_the_band_edge_first_splits_the_readings() -> TestResult {
        assert_eq!(
            case(-0.05, 1)?,
            (true, false),
            "walking out: in reach for acquisition, chased by pursuit"
        );
        assert_eq!(
            case(0.05, -1)?,
            (false, true),
            "walking in: out of reach for acquisition, stood by pursuit"
        );
        assert_eq!(case(-0.5, 1)?, (true, true), "well inside, both stand");
        assert_eq!(case(0.5, -1)?, (false, false), "well outside, both chase");
        Ok(())
    }
}
