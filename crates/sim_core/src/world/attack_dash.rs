//! The attack dash: a speed burst an attack order triggers by itself (`StarCraft` II's Charge).
//!
//! # Responsibility
//! Starts the dash of every unit whose archetype carries
//! [`sim_types::ArchetypeCapabilities::attack_dash`] once its trigger window holds (plan
//! `2026-11-05-sc2-deviation-fixes` work package 13c, bug B-43). The dash itself is an
//! [`crate::entity::ActiveBuff`] ([`crate::entity::ATTACK_DASH_BUFF_ID`]), so movement reads it
//! like any other speed buff; upkeep runs its duration and cooldown out, and the weapon phase
//! ends it when the unit's weapon starts a swing ([`crate::entity::Entity::end_attack_dash`]).
//!
//! # How it fits
//! The tick runs this between target acquisition (which sets the attack order an acquired
//! target produces) and movement, so a dash speeds up the very tick it starts.
//!
//! # The rule, from the game data
//! `SC2GameData` `5444e12f13`, chain `core → liberty → swarm → void → voidmulti`:
//! - `Charge` is an autocast augment of the `attack` command, on by default, with a 10 Normal
//!   s cooldown and the autocast validator `CasterNotHoldingPosition` (`liberty`
//!   AbilData:3825-3834); its autocast filters admit only visible enemies (`core`
//!   AbilData:146, `Visible;Player,Ally`).
//! - Its effect applies `Charging` to the caster when the attack order's target is farther than
//!   0.6 (`ChargeMinTriggerDistance`, `liberty` ValidatorData:183-190) and nearer than 4
//!   (`ChargeMaxDistance`, :1697-1704) (`liberty` EffectData:2062-2068).
//! - `Charging` lasts 3.5 Normal s, multiplies speed by 2.2, and is removed once the weapon
//!   fires (`liberty` BehaviorData:967-975, ValidatorData:3007-3011).
//!
//! What the data name but do not define, the owner's engine notes on Q0071 settle:
//! - the 0.6 is measured edge to edge ([`AttackDashSpec::min_gap`]), as weapon ranges are;
//! - the autocast looks on every tick of the attack order, until the window holds;
//! - the 4 (`Pathing 1`) is the ground path distance: the length of the route the dasher's
//!   disc walks on the navigation mesh to the target's centre. Where no route query is
//!   possible (a map without a mesh, a dasher that does not walk on the ground, a dasher or a
//!   target centre where the dasher's disc does not fit, such as a structure), the straight
//!   line between the centres stands in; a target the dasher cannot walk to at all is never
//!   in the window;
//! - an explicit `Move`, `Stop` or `HoldPosition` order ends a running dash
//!   ([`crate::entity::Entity::end_attack_dash`]), and a retarget to another attack keeps it.
//!
//! # Invariants
//! - A dash starts only for a living, completed unit in the world whose dash is unlocked for
//!   its owner and off cooldown, whose front order attacks a visible, attackable enemy, and
//!   which is not holding position or channeling.
//! - The decision reads every unit before any dash starts, so the result does not depend on the
//!   order units are visited in.

use crate::entity::Entity;
use crate::movement::{MovementInvariant, MovementLayer};
use crate::upgrades::ATTACK_DASH_ABILITY_SLUG;
use crate::weapon_timing::SimulationDiagnostic;
use crate::SimWorld;
use sim_map::{MapError, PathRequestWithVision, VisibilityState};
use sim_types::{AttackDashSpec, EntityId, SimScalar, Tick, UnitOrder};

impl SimWorld {
    /// Starts the attack dash of every unit whose trigger window holds this tick.
    ///
    /// A dash that cannot be represented (a malformed speed buff already on the unit) is not
    /// started and is reported as a [`SimulationDiagnostic::MovementInvariant`].
    // PERFORMANCE RATIONALE:
    // Why: The decision must see every unit unchanged, so it cannot mutate while it reads, and
    // collecting the dashing units into a fresh vector each tick would allocate.
    // What: The ids go into a retained buffer, which is taken out of the world while it is used.
    // Trade-off: The buffer keeps its high-water capacity; in exchange the phase allocates
    // nothing in steady state.
    pub fn trigger_attack_dashes(&mut self) {
        let mut starting = std::mem::take(&mut self.scratch_dashes);
        starting.clear();
        starting.extend(
            self.entities
                .iter()
                .filter_map(|(&id, entity)| self.attack_dash_ready(entity).map(|spec| (id, spec))),
        );
        // The straight line is never longer than the walk, so only the units it admits are
        // measured along the ground.
        starting.retain(|&(id, spec)| self.is_walk_shorter_than(id, spec.max_distance));
        for &(id, spec) in &starting {
            let Some(entity) = self.entities.get_mut(&id) else {
                continue;
            };
            if let Err(source) = entity.begin_attack_dash(&spec) {
                self.tick_diagnostics
                    .push(SimulationDiagnostic::MovementInvariant {
                        tick: Tick(self.current_tick),
                        source: MovementInvariant::Admission { entity: id, source },
                    });
            }
        }
        self.scratch_dashes = starting;
    }

    /// The dash `entity` starts this tick, if its trigger window holds (see the module docs).
    fn attack_dash_ready(&self, entity: &Entity) -> Option<AttackDashSpec> {
        if !entity.is_alive()
            || !entity.is_completed
            || entity.is_loaded_in_transport()
            || entity.channeling_remaining_ticks > 0
            || entity.attack_dash_cooldown > 0
            || entity.is_on_hold()
        {
            return None;
        }
        let Some(UnitOrder::AttackTarget { target, .. }) = entity.order_queue.front() else {
            return None;
        };
        let arch = self.catalog.get(entity.unit_type_id).ok()?;
        let spec = arch.capabilities.attack_dash?;
        let target_entity = self.entities.get(target)?;
        let target_arch = self.catalog.get(target_entity.unit_type_id).ok()?;
        let distance = entity.position.distance(target_entity.position);
        let gap = distance - arch.collision_radius - target_arch.collision_radius;
        let in_window = gap > spec.min_gap && distance < spec.max_distance;
        (in_window
            && target_entity.owner != entity.owner
            && self.is_visible_enemy_target(entity, *target)
            && self.is_ability_unlocked(arch, entity.owner, ATTACK_DASH_ABILITY_SLUG))
        .then_some(spec)
    }

    /// Whether the ground path from unit `id` to the target of its attack order is shorter than
    /// `limit` world units (`ChargeMaxDistance`'s `Pathing 1`, the owner's engine note on
    /// Q0071), seen through the owner's knowledge of the map as its routes are.
    ///
    /// `true` where no route query is possible, so the straight line the caller checked stands
    /// (see the module docs); `false` when the dasher cannot walk to the target at all.
    fn is_walk_shorter_than(&mut self, id: EntityId, limit: SimScalar) -> bool {
        let Some(entity) = self.entities.get(&id) else {
            return false;
        };
        let Some(&UnitOrder::AttackTarget { target, .. }) = entity.order_queue.front() else {
            return false;
        };
        let Some(goal) = self.entities.get(&target).map(|t| t.position) else {
            return false;
        };
        let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
            return false;
        };
        if MovementLayer::from_mover(arch.mover_type) != MovementLayer::Ground
            || self.pathfinder.navmesh().is_none()
        {
            return true;
        }
        let knowledge = entity
            .owner
            .slot()
            .filter(|_| self.vision_aware_pathfinding)
            .map(|owner| (&self.vision_maps[owner], &self.known_blocked_cells[owner]));
        let request = PathRequestWithVision {
            start: entity.position,
            goal,
            radius: arch.collision_radius,
            vision: knowledge.map(|(vision, _)| vision),
            block_unexplored_ramps: false,
            known_structures: knowledge.map(|(_, known)| known.as_slice()),
        };
        // An open straight line is the walk; only a blocked one needs a route.
        if self
            .pathfinder
            .segment_is_passable(&self.map, &request, request.start, goal)
        {
            return true;
        }
        match self.pathfinder.find_path_with_vision(&self.map, &request) {
            Ok(path) => path.total_length < limit,
            Err(MapError::NoPathFound { .. }) => false,
            Err(_) => true,
        }
    }

    /// Whether `entity`'s owner sees `target` and may attack it: the autocast's `Visible`
    /// filter, read as target acquisition reads it (a visible cell, and detection for a cloaked
    /// unit).
    fn is_visible_enemy_target(&self, entity: &Entity, target: EntityId) -> bool {
        let Some(target_entity) = self.entities.get(&target) else {
            return false;
        };
        if !Self::is_entity_targetable(target_entity, &self.map, &self.vision_maps, entity.owner) {
            return false;
        }
        let Some(vision) = self.vision_maps.get(entity.owner) else {
            return true;
        };
        self.map
            .world_to_cell(target_entity.position)
            .is_ok_and(|(cx, cy)| vision.visibility_at(cx, cy) == VisibilityState::Visible)
    }
}
