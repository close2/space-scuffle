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
//! How the engine measures the two distances, when the autocast looks, and whether a new order
//! ends the buff are not stated; this module reads them as [`AttackDashSpec`] documents (owner
//! question Q0071, default this reading).
//!
//! # Invariants
//! - A dash starts only for a living, completed unit in the world whose dash is unlocked for
//!   its owner and off cooldown, whose front order attacks a visible, attackable enemy, and
//!   which is not holding position or channeling.
//! - The decision reads every unit before any dash starts, so the result does not depend on the
//!   order units are visited in.

use crate::entity::Entity;
use crate::movement::MovementInvariant;
use crate::upgrades::ATTACK_DASH_ABILITY_SLUG;
use crate::weapon_timing::SimulationDiagnostic;
use crate::SimWorld;
use sim_map::VisibilityState;
use sim_types::{AttackDashSpec, EntityId, Tick, UnitOrder};

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
