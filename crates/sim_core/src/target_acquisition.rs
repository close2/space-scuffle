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

    /// Whether `viewer`'s interface shows the enemy `target` itself: `viewer` sees it
    /// ([`Self::sees_enemy`]: its cell `Visible`, or attack-revealed) and it is either
    /// unconcealed or cloak-revealed to `viewer` ([`Self::is_cloak_revealed`]). The rule the
    /// client's fog copies; the attack snapshot's record and drop rules read it, and nothing
    /// else (plan `2026-11-24-attacker-snapshot` D-2). Whether `target` lives or is hidden is
    /// the caller's question.
    #[must_use]
    pub(crate) fn shown_to(
        map: &SimMap,
        vision_maps: &PerPlayer<PlayerVisionMap>,
        viewer: PlayerId,
        target: &Entity,
    ) -> bool {
        Self::sees_enemy(map, vision_maps, viewer, target)
            && (!target.is_concealed()
                || Self::is_cloak_revealed(
                    map,
                    vision_maps,
                    viewer,
                    target.position,
                    target.is_attack_revealed_to(viewer),
                ))
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
        Self::is_targetable_at(target, target.position, map, vision_maps, attacker_owner)
    }

    /// [`Self::is_entity_targetable`] with the target standing at `position`, world units: the
    /// movement phase asks it of a candidate's tick-start position (G-6′ of plan
    /// `2026-11-24-solver-retarget-and-shoving`).
    #[inline]
    pub(crate) fn is_targetable_at(
        target: &Entity,
        position: SimVec2,
        map: &SimMap,
        vision_maps: &PerPlayer<PlayerVisionMap>,
        attacker_owner: PlayerId,
    ) -> bool {
        if !Self::is_damageable(target) {
            return false;
        }
        !target.is_concealed()
            || Self::is_cloak_revealed(
                map,
                vision_maps,
                attacker_owner,
                position,
                target.is_attack_revealed_to(attacker_owner),
            )
    }

    /// Whether `viewer` sees the concealed unit at `position` (world units) and detects it: its
    /// cell is `Visible` to `viewer` or it is `attack_revealed` to `viewer` (plan
    /// `2026-11-23-vision-and-detection` D-5), and its cell is detected. Detection alone is not
    /// sight, and the reveal never gives detection (owner answer Q0101 2(b); plan
    /// `2026-11-24-owner-answers-cliff-and-vision` D-4). A viewer without a vision map detects
    /// nothing.
    #[must_use]
    pub(crate) fn is_cloak_revealed(
        map: &SimMap,
        vision_maps: &PerPlayer<PlayerVisionMap>,
        viewer: PlayerId,
        position: SimVec2,
        attack_revealed: bool,
    ) -> bool {
        let Some(vision) = vision_maps.get(viewer) else {
            return false;
        };
        Self::sees_point(map, vision_maps, viewer, position, attack_revealed)
            && map
                .world_to_cell(position)
                .is_ok_and(|(cx, cy)| vision.is_detected(cx, cy))
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
                || Self::is_cloak_revealed(
                    &self.map,
                    &self.vision_maps,
                    viewer,
                    attacker.position,
                    attacker.is_attack_revealed_to(viewer),
                ))
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

    /// The tie-break measure of a candidate standing at `at` for `entity` standing at `from`
    /// (world units; see [`TargetPriorityKey::tie_break`]), given the weapon it would use against
    /// the candidate and where the previous target stands for
    /// [`AcquireTieBreak::ClosestToPrevious`] (`previous`, world units).
    #[inline]
    fn tie_break(
        entity: &Entity,
        arch: &UnitArchetype,
        weapon: &WeaponSpec,
        (from, at): (SimVec2, SimVec2),
        previous: Option<SimVec2>,
    ) -> SimScalar {
        let distance_sq = from.distance_sq(at);
        match arch.acquire_tie_break {
            AcquireTieBreak::Closest => distance_sq,
            AcquireTieBreak::LeastTurning => {
                let delta = at - from;
                let bearing = SimAngle::atan2(delta.y, delta.x);
                let turn = weapon_aim(entity, weapon).diff(bearing).unsigned_abs();
                SimScalar::from_i32(i32::from(turn))
            }
            AcquireTieBreak::ClosestToPrevious => {
                previous.map_or(distance_sq, |previous| previous.distance_sq(at))
            }
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
        self.reaches_between(entity, arch, entity.position, target, target.position)
    }

    /// Whether `entity`'s weapon, standing at `from`, can engage `target` standing at `to`
    /// (world units).
    #[inline]
    fn reaches_between(
        &self,
        entity: &Entity,
        arch: &UnitArchetype,
        from: SimVec2,
        target: &Entity,
        to: SimVec2,
    ) -> bool {
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
        let distance = from.distance(to);
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
        // A valid target in reach is kept unless a candidate outranks it; so is the target G-6′
        // handed the unit to when its own target's ring was full (plan
        // `2026-11-24-solver-retarget-and-shoving` §4.2), so the nearer, full target is not taken
        // back the next tick.
        let outranked = scan.current.is_none_or(|held| best.rank() > held.rank());
        let handed = matches!(engagement, Engagement::Acquired(target)
            if entity.movement.ring_retarget == Some(target));
        if (reachable_current.is_some() || handed) && !outranked {
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
            let at = (entity.position, neighbor.position);
            let Some(admitted) = self.admit((entity, arch), scope, (neighbor_id, neighbor), at)
            else {
                continue;
            };
            let key =
                Self::candidate_key((entity, arch), scope, (neighbor_id, neighbor), admitted, at);
            if Some(neighbor_id) == current {
                result.current = Some(key);
            }
            if result.best.is_none_or(|best| key < best) {
                result.best = Some(key);
            }
        }
        result
    }

    /// Acquisition's test of one neighbour `(neighbor_id, neighbor)` for `(entity, arch)` under
    /// `scope`, with the unit standing at `from` and the neighbour at `at` (world units): the
    /// candidate's archetype and the weapon the unit would use against it, or `None` when it is
    /// no candidate: an ally, the unit's abandoned target, untargetable or unseen, beyond the scan
    /// range edge to edge, outside the weapon's target flags, not licensed by a tracking or
    /// own-target scan, inside the minimum range, or out of reach of a stationary unit. The
    /// cheapest tests come first. Acquisition ([`Self::scan_candidates`], from where both stand)
    /// and G-6′ ([`Self::ranked_candidates`], from the tick-start positions) share it, so they
    /// choose by one rule; its key is built only for what it admits ([`Self::candidate_key`]).
    // PERFORMANCE RATIONALE:
    // Why: This runs for every neighbour of every scanning unit each tick, acquisition's
    // costliest loop; the stopped G-6′ branch's shared helper raised acquisition from 25 % to
    // 33 % of a battle's instructions (plan `2026-11-24-solver-retarget-and-shoving` §4.2).
    // What: The test is split from the key, inlined, takes the positions by value and keeps the
    // early-out order of the scan it replaced, so acquisition does the same work as before.
    // Trade-off: Two small functions instead of one loop body; no behaviour changes (G-T9).
    #[inline]
    fn admit<'w>(
        &'w self,
        (entity, arch): (&'w Entity, &'w UnitArchetype),
        scope: ScanScope,
        (neighbor_id, neighbor): (EntityId, &Entity),
        (from, at): (SimVec2, SimVec2),
    ) -> Option<(&'w UnitArchetype, &'w WeaponSpec)> {
        if neighbor.owner == entity.owner
            || entity.movement.abandoned_target == Some(neighbor_id)
            || !Self::is_targetable_at(neighbor, at, &self.map, &self.vision_maps, entity.owner)
        {
            return None;
        }
        let dist = from.distance(at);
        let revealed = neighbor.is_attack_revealed_to(entity.owner);
        if self.map.world_to_cell(at).is_err()
            || !Self::sees_point(&self.map, &self.vision_maps, entity.owner, at, revealed)
        {
            return None;
        }
        let target_arch = self.catalog.get(neighbor.unit_type_id).ok()?;
        let weapon = arch.weapon_against(target_arch.mover_type, entity.active_siege_weapon())?;
        let radii = arch.collision_radius + target_arch.collision_radius;
        let scan_range = arch
            .scan_range
            .max(self.entity_weapon_range(entity, weapon, &arch.slug));
        if dist > scan_range + radii || !weapon.target_flags.intersects(target_arch.attributes) {
            return None;
        }
        if scope
            .tracking
            .is_some_and(|walking| !fires_without_attack_order(weapon, walking))
            || scope.own_target.is_some_and(|own| !own.licenses(weapon))
        {
            return None;
        }
        if weapon.min_range > SimScalar::ZERO && dist < weapon.min_range + radii {
            return None;
        }
        if scope.stationary && !self.reaches_between(entity, arch, from, neighbor, at) {
            return None;
        }
        Some((target_arch, weapon))
    }

    /// The ranking key of a neighbour [`Self::admit`] admitted, with the unit at `from` and the
    /// neighbour at `at` (world units).
    #[inline]
    fn candidate_key(
        (entity, arch): (&Entity, &UnitArchetype),
        scope: ScanScope,
        (neighbor_id, neighbor): (EntityId, &Entity),
        (target_arch, weapon): (&UnitArchetype, &WeaponSpec),
        positions: (SimVec2, SimVec2),
    ) -> TargetPriorityKey {
        let previous = scope
            .own_target
            .map_or(entity.previous_target_position, |own| Some(own.anchor));
        TargetPriorityKey {
            threat: is_threat_to(target_arch, neighbor, arch),
            attack_target_priority: target_arch.attack_target_priority,
            primary: arch.primary_weapon_reaches(target_arch.mover_type),
            tie_break: Self::tie_break(entity, arch, weapon, positions, previous),
            entity_id: neighbor_id,
        }
    }

    /// G-6′'s candidates (plan `2026-11-24-solver-retarget-and-shoving` §4.2): every enemy that
    /// acquisition would admit for `entity` under its automatic attack ([`Self::admit`]), judged
    /// from the movement phase's tick-start positions (`entity` at `position`, world units),
    /// ranked best first by [`TargetPriorityKey`] into `ranked`, `current` left out. The
    /// neighbour set comes from the spatial hash phase 4 built, which nothing rebuilds before
    /// movement.
    // PERFORMANCE RATIONALE:
    // Why: G-6′ ranks the candidates of every standing automatic chaser each tick; fresh vectors
    // would allocate in the movement phase's hot loop.
    // What: The neighbour ids and the ranked list go into retained buffers the caller owns,
    // cleared per call.
    // Trade-off: Memory stays at the most crowded scan's high-water mark.
    pub(crate) fn ranked_candidates(
        &self,
        entity: &Entity,
        position: SimVec2,
        current: EntityId,
        neighbors: &mut Vec<EntityId>,
        ranked: &mut Vec<(TargetPriorityKey, EntityId)>,
    ) {
        ranked.clear();
        let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
            return;
        };
        let largest_radius = self
            .catalog
            .archetypes()
            .fold(SimScalar::ZERO, |largest, a| {
                largest.max(a.collision_radius)
            });
        let scope = ScanScope {
            stationary: entity.is_immobile,
            tracking: None,
            own_target: None,
            largest_radius,
        };
        self.spatial_hash.query_radius_unordered(
            position,
            self.scan_extent(entity, arch) + arch.collision_radius + largest_radius,
            neighbors,
        );
        for &neighbor_id in neighbors.iter() {
            if neighbor_id == current {
                continue;
            }
            let Some(neighbor) = self.entities.get(&neighbor_id) else {
                continue;
            };
            let at = (position, self.tick_start_position(neighbor_id));
            if let Some(admitted) = self.admit((entity, arch), scope, (neighbor_id, neighbor), at) {
                let key = Self::candidate_key(
                    (entity, arch),
                    scope,
                    (neighbor_id, neighbor),
                    admitted,
                    at,
                );
                ranked.push((key, neighbor_id));
            }
        }
        ranked.sort_unstable_by_key(|&(key, _)| key);
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
    //! that starts the plan's WP6, which reads the chase from pursuit itself. Under the solver
    //! pipeline, the default since plan `2026-11-20-contact-solver` WP6, pursuit reads its
    //! target's tick-start position (that plan's WP5), the positions acquisition saw, so the two
    //! readings agree on crossing targets too; the split is pinned on the legacy pipeline.
    use crate::catalog::{ContactRules, MovementPipeline};
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
    /// Returns (reaches before the tick, stood on the tick), under `pipeline`.
    fn case(
        offset: f64,
        walk: i32,
        pipeline: MovementPipeline,
    ) -> Result<(bool, bool), Box<dyn std::error::Error>> {
        let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = world(map, 11);
        let rules = world.catalog.movement_rules();
        world
            .catalog
            .set_movement_rules(rules.with_contact(ContactRules {
                pipeline,
                ..rules.contact()
            })?);
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
            let (reaches, stood) = case(offset, 0, MovementPipeline::Solver)?;
            assert_eq!(
                (reaches, stood),
                case(offset, 0, MovementPipeline::Legacy)?,
                "offset {offset}: both pipelines"
            );
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
        let legacy = MovementPipeline::Legacy;
        assert_eq!(
            case(-0.05, 1, legacy)?,
            (true, false),
            "walking out: in reach for acquisition, chased by pursuit"
        );
        assert_eq!(
            case(0.05, -1, legacy)?,
            (false, true),
            "walking in: out of reach for acquisition, stood by pursuit"
        );
        assert_eq!(
            case(-0.5, 1, legacy)?,
            (true, true),
            "well inside, both stand"
        );
        assert_eq!(
            case(0.5, -1, legacy)?,
            (false, false),
            "well outside, both chase"
        );
        Ok(())
    }

    /// Under the solver pipeline pursuit reads the target's tick-start position, so a target that
    /// crosses the band's edge in the movement phase does not split the readings.
    #[test]
    fn under_the_solver_a_crossing_target_does_not_split_the_readings() -> TestResult {
        let solver = MovementPipeline::Solver;
        assert_eq!(
            case(-0.05, 1, solver)?,
            (true, true),
            "walking out: both stand"
        );
        assert_eq!(
            case(0.05, -1, solver)?,
            (false, false),
            "walking in: both chase"
        );
        Ok(())
    }
}

#[cfg(test)]
mod candidate_test_tests {
    //! G-T9 of plan `2026-11-24-solver-retarget-and-shoving` (§4.2, item 1): acquisition's scan
    //! was split into one admission test ([`SimWorld::admit`]) and a key built only for what it
    //! admits ([`SimWorld::candidate_key`]), so that G-6′ chooses by the same rule. The scan of
    //! the base (`main` at `e5f5c565`) is kept here, verbatim but for its signature, as the test
    //! oracle: on generated scenes, for every unit and every scan scope, the shared test admits
    //! the same enemies with the same keys, in the same order.
    use super::{OwnTargetScan, ScanScope};
    use crate::catalog::AcquireTieBreak;
    use crate::combat::{is_threat_to, TargetPriorityKey};
    use crate::test_support::world;
    use crate::world::SimWorld;
    use crate::{Entity, UnitArchetype, WeaponSpec};
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{
        EntityId, MoverType, PlayerCommand, PlayerId, SimAngle, SimScalar, SimVec2, Tick,
        TickBundle, UnitOrder,
    };

    /// The base's tie-break, verbatim.
    fn oracle_tie_break(
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
                let turn = super::weapon_aim(entity, weapon)
                    .diff(bearing)
                    .unsigned_abs();
                SimScalar::from_i32(i32::from(turn))
            }
            AcquireTieBreak::ClosestToPrevious => previous.map_or(distance_sq, |previous| {
                previous.distance_sq(candidate.position)
            }),
        }
    }

    /// The base's scan loop, verbatim, collecting every admitted neighbour's key in visiting
    /// order.
    fn oracle(
        world: &SimWorld,
        (entity, arch): (&Entity, &UnitArchetype),
        scope: ScanScope,
        neighbors: &[EntityId],
    ) -> Vec<TargetPriorityKey> {
        let mut admitted = Vec::new();
        for &neighbor_id in neighbors {
            let Some(neighbor) = world.entities.get(&neighbor_id) else {
                continue;
            };
            if neighbor.owner == entity.owner
                || entity.movement.abandoned_target == Some(neighbor_id)
                || !SimWorld::is_entity_targetable(
                    neighbor,
                    &world.map,
                    &world.vision_maps,
                    entity.owner,
                )
            {
                continue;
            }
            let dist = entity.position.distance(neighbor.position);
            if world.map.world_to_cell(neighbor.position).is_err()
                || !SimWorld::sees_enemy(&world.map, &world.vision_maps, entity.owner, neighbor)
            {
                continue;
            }
            let Ok(target_arch) = world.catalog.get(neighbor.unit_type_id) else {
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
                .max(world.entity_weapon_range(entity, weapon, &arch.slug));
            if dist > scan_range + radii {
                continue;
            }
            if !weapon.target_flags.intersects(target_arch.attributes) {
                continue;
            }
            if scope
                .tracking
                .is_some_and(|walking| !super::fires_without_attack_order(weapon, walking))
                || scope.own_target.is_some_and(|own| !own.licenses(weapon))
            {
                continue;
            }
            if weapon.min_range > SimScalar::ZERO && dist < weapon.min_range + radii {
                continue;
            }
            if scope.stationary && !world.reaches_from_here(entity, arch, neighbor) {
                continue;
            }
            admitted.push(TargetPriorityKey {
                threat: is_threat_to(target_arch, neighbor, arch),
                attack_target_priority: target_arch.attack_target_priority,
                primary: arch.primary_weapon_reaches(target_arch.mover_type),
                tie_break: oracle_tie_break(
                    entity,
                    arch,
                    weapon,
                    neighbor,
                    scope
                        .own_target
                        .map_or(entity.previous_target_position, |own| Some(own.anchor)),
                ),
                entity_id: neighbor_id,
            });
        }
        admitted
    }

    /// The shared test and key over the same neighbours, from where both stand.
    fn shared(
        world: &SimWorld,
        (entity, arch): (&Entity, &UnitArchetype),
        scope: ScanScope,
        neighbors: &[EntityId],
    ) -> Vec<TargetPriorityKey> {
        neighbors
            .iter()
            .filter_map(|&id| {
                let neighbor = world.entities.get(&id)?;
                let at = (entity.position, neighbor.position);
                let admitted = world.admit((entity, arch), scope, (id, neighbor), at)?;
                Some(SimWorld::candidate_key(
                    (entity, arch),
                    scope,
                    (id, neighbor),
                    admitted,
                    at,
                ))
            })
            .collect()
    }

    /// A small deterministic stream.
    struct Stream(u64);

    impl Stream {
        fn below(&mut self, bound: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) % bound
        }
    }

    /// A generated scene: 30 shipped mobile archetypes of both planes, two owners, scattered over
    /// a 24 × 24 square, some holding, some attack-moving, stepped a few ticks so that vision,
    /// targets and remembered positions are live.
    fn scene(seed: u64) -> Result<SimWorld, Box<dyn std::error::Error>> {
        let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = world(map, seed);
        let kinds: Vec<_> = world
            .catalog
            .archetypes()
            .filter(|a| a.max_speed > SimScalar::ZERO && a.mover_type != MoverType::CliffJumper)
            .map(|a| a.unit_type_id)
            .collect();
        let mut rng = Stream(seed);
        let mut units = [Vec::new(), Vec::new()];
        for _ in 0..30 {
            let kind = kinds[usize::try_from(rng.below(kinds.len() as u64))?];
            let owner = u8::from(rng.below(2) == 1);
            let x = 12 + i32::try_from(rng.below(24))?;
            let y = 12 + i32::try_from(rng.below(24))?;
            if let Ok(id) = world.spawn_unit(kind, PlayerId(owner), SimVec2::from_i32(x, y)) {
                units[usize::from(owner)].push(id);
            }
        }
        let mut commands = Vec::new();
        for (owner, side) in units.iter().enumerate() {
            let (hold, walk): (Vec<_>, Vec<_>) = side.iter().partition(|id| id.0 % 3 == 0);
            let player = PlayerId(u8::try_from(owner)?);
            commands.push(PlayerCommand {
                player_id: player,
                entities: hold,
                order: UnitOrder::HoldPosition,
                queue: false,
            });
            commands.push(PlayerCommand {
                player_id: player,
                entities: walk,
                order: UnitOrder::AttackMove {
                    target: SimVec2::from_i32(24, 24),
                },
                queue: false,
            });
        }
        for tick in 0..3 {
            world.step(&TickBundle {
                tick: Tick(world.current_tick),
                commands: if tick == 0 {
                    commands.clone()
                } else {
                    Vec::new()
                },
                state_checksum: None,
            });
        }
        world.acquire_targets();
        Ok(world)
    }

    /// G-T9: on 40 generated scenes, for every unit and each of twelve scan scopes, the shared
    /// candidate test admits exactly the neighbours the base's scan admitted, with equal keys in
    /// the same order.
    #[test]
    fn the_shared_candidate_test_admits_what_scan_candidates_admitted(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut compared = 0_usize;
        for seed in 1..=40 {
            let world = scene(seed)?;
            let largest_radius = world
                .catalog
                .archetypes()
                .fold(SimScalar::ZERO, |largest, a| {
                    largest.max(a.collision_radius)
                });
            let mut neighbors = Vec::new();
            for entity in world.entities.values() {
                let arch = world.catalog.get(entity.unit_type_id)?;
                world.spatial_hash.query_radius_unordered(
                    entity.position,
                    world.scan_extent(entity, arch) + arch.collision_radius + largest_radius,
                    &mut neighbors,
                );
                let own = |chasing| OwnTargetScan {
                    chasing,
                    anchor: SimVec2::from_i32(24, 24),
                };
                for stationary in [false, true] {
                    for tracking in [None, Some(false), Some(true)] {
                        for own_target in [None, Some(own(false)), Some(own(true))] {
                            if tracking.is_some() && own_target.is_some() {
                                continue;
                            }
                            let scope = ScanScope {
                                stationary,
                                tracking,
                                own_target,
                                largest_radius,
                            };
                            let expected = oracle(&world, (entity, arch), scope, &neighbors);
                            let got = shared(&world, (entity, arch), scope, &neighbors);
                            assert_eq!(got, expected, "seed {seed}, unit {:?}", entity.id);
                            compared += expected.len();
                        }
                    }
                }
            }
        }
        assert!(compared > 1_000, "{compared} admitted candidates compared");
        Ok(())
    }
}
