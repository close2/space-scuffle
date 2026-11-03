//! Active abilities and the hazard zones they leave behind.
//!
//! # Responsibility
//! Owns [`ActiveHazardZone`], ability casting (admission, payment and effect) and the
//! world-level overclock used by the stim order.
//!
//! # How it fits
//! Casts run in phase 1 (`process_commands`) and when a queued cast reaches the front of an
//! order queue during movement. `world.rs` re-exports [`ActiveHazardZone`], so its public path
//! is unchanged.
//!
//! # Invariants
//! - Every fallible check of a cast runs before any write: a refused or failed cast leaves
//!   energy, cooldowns and the entity untouched.
//! - Hazard IDs come from `next_hazard_id` and are never reused within a world.

use crate::movement::{DisplacementAdmissionError, MovementInvariant, MovementMathError};
use crate::SimWorld;
use sim_types::{AbilityEffectSpec, EntityId, PlayerId, SimScalar, SimVec2};

/// Active continuous hazard/AoE zone (e.g. Psionic Storm) ticking in the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveHazardZone {
    /// Monotonically increasing hazard ID.
    pub id: u32,
    /// Player owner handle.
    pub owner: PlayerId,
    /// Center world coordinates.
    pub center: SimVec2,
    /// Circular radius of effect.
    pub radius: SimScalar,
    /// Initial detonation delay in ticks before first damage application.
    pub delay_ticks: u16,
    /// Total lifetime remaining ticks.
    pub remaining_ticks: u16,
    /// Ticks between consecutive periodic damage pulses.
    pub interval_ticks: u16,
    /// Ticks remaining until the next damage pulse.
    pub ticks_until_next_interval: u16,
    /// Damage applied per interval pulse.
    pub damage_per_interval: SimScalar,
    /// If true, friendly entities also suffer damage.
    pub targets_friendly: bool,
}

impl SimWorld {
    /// Executes an ability, surfacing malformed numeric state through tick diagnostics.
    /// Ordinary eligibility/placement rejection returns false without payment.
    pub fn execute_ability(
        &mut self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> bool {
        if let Err(source) = self.validate_transport_links() {
            self.report_transport_invariant(source);
            return false;
        }
        self.execute_ability_prevalidated(entity_id, ability_index, target_pos, target_entity)
    }

    // Tick/standalone wrapper proved relations; effect admission cannot alter links.
    pub(super) fn execute_ability_prevalidated(
        &mut self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> bool {
        match self.try_execute_ability_prevalidated(
            entity_id,
            ability_index,
            target_pos,
            target_entity,
        ) {
            Ok(applied) => applied,
            Err(source) => {
                self.report_movement_invariant(source);
                false
            }
        }
    }

    pub(super) fn apply_world_overclock(
        &mut self,
        id: EntityId,
        spec: sim_types::OverclockSpec,
    ) -> Result<bool, MovementInvariant> {
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "Overclock actor disappeared",
            })?;
        if actor.is_loaded_in_transport() {
            return Ok(false);
        }
        let prepared = actor
            .prepare_overclock(spec)
            .map_err(|source| MovementInvariant::Admission { entity: id, source })?;
        let Some(prepared) = prepared else {
            return Ok(false);
        };
        let arch = self
            .catalog
            .get(actor.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(id))?;
        self.effective_max_speed(arch, actor.owner)
            .and_then(|speed| {
                speed
                    .checked_mul(prepared.multiplier)
                    .ok_or(MovementMathError::Unrepresentable)
            })
            .map_err(|source| MovementInvariant::Arithmetic {
                entity: id,
                operation: "Overclock effective speed",
                source,
            })?;
        if let Some(actor) = self.entities.get_mut(&id) {
            actor.commit_overclock(prepared);
        }
        Ok(true)
    }

    /// Executes an active ability or spell for an entity.
    ///
    /// Validates energy cost, cooldowns, range, terrain pathability, and channeling.
    /// Returns `true` if cast successfully; `false` otherwise.
    #[expect(
        clippy::too_many_lines,
        reason = "split in plan steps 3.1 and 9.1, which own this file's structure"
    )]
    pub(crate) fn try_execute_ability_prevalidated(
        &mut self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> Result<bool, MovementInvariant> {
        let _ = target_entity;
        let entity = match self.entities.get(&entity_id) {
            Some(e) if e.is_alive() => e,
            _ => return Ok(false),
        };

        if entity.is_loaded_in_transport() || entity.channeling_remaining_ticks > 0 {
            return Ok(false);
        }

        let arch = self
            .catalog
            .get(entity.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(entity_id))?;

        let Some(ability) = arch.abilities.get(ability_index as usize) else {
            return Ok(false);
        };
        if !self.is_ability_unlocked(arch, entity.owner, &ability.name) {
            return Ok(false);
        }

        if let Some(&cd) = entity.ability_cooldowns.get(ability_index as usize) {
            if cd > 0 {
                return Ok(false);
            }
        }

        let admission = |source| MovementInvariant::Admission {
            entity: entity_id,
            source,
        };
        if ability.energy_cost < SimScalar::ZERO {
            return Err(admission(crate::entity::EntityAdmissionError::NegativeCost));
        }
        if ability.cast_range < SimScalar::ZERO {
            return Err(MovementInvariant::Arithmetic {
                entity: entity_id,
                operation: "ability range",
                source: MovementMathError::InvalidDistance,
            });
        }
        if entity.energy < ability.energy_cost {
            return Ok(false);
        }

        if ability.cast_range > SimScalar::ZERO {
            if let Some(pos) = target_pos {
                if !crate::movement::math::within_distance(
                    entity.position,
                    pos,
                    i128::from(ability.cast_range.raw()),
                ) {
                    return Ok(false);
                }
            }
        }

        let prepared_energy = entity
            .energy
            .checked_sub(ability.energy_cost)
            .ok_or_else(|| admission(crate::entity::EntityAdmissionError::PaymentOverflow))?;
        let mut prepared_overclock = None;
        // Validate every fallible payment/rate/placement before any commit.
        match &ability.effect {
            AbilityEffectSpec::Overclock(spec) => {
                let Some(prepared) = entity.prepare_overclock(*spec).map_err(admission)? else {
                    return Ok(false);
                };
                self.effective_max_speed(arch, entity.owner)
                    .and_then(|base| {
                        base.checked_mul(prepared.multiplier)
                            .ok_or(MovementMathError::Unrepresentable)
                    })
                    .map_err(|source| MovementInvariant::Arithmetic {
                        entity: entity_id,
                        operation: "Overclock effective speed",
                        source,
                    })?;
                prepared_overclock = Some(prepared);
            }
            AbilityEffectSpec::SiegeModeToggle { .. }
            | AbilityEffectSpec::CloakToggle { .. }
            | AbilityEffectSpec::SwitchMode { .. } => {}
            AbilityEffectSpec::BlinkTeleport { max_range, .. } => {
                let Some(dest) = target_pos else {
                    return Ok(false);
                };
                if *max_range < SimScalar::ZERO {
                    return Err(MovementInvariant::Arithmetic {
                        entity: entity_id,
                        operation: "Blink range",
                        source: MovementMathError::InvalidDistance,
                    });
                }
                if !crate::movement::math::within_distance(
                    entity.position,
                    dest,
                    i128::from(max_range.raw()),
                ) {
                    return Ok(false);
                }
                if let Err(error) =
                    self.validate_displacement_destination_prevalidated(entity_id, dest)
                {
                    if let DisplacementAdmissionError::Invariant(source) = error {
                        return Err(source);
                    }
                    return Ok(false);
                }
            }
            AbilityEffectSpec::PsionicStormAoE { .. } => {
                if target_pos.is_none() {
                    return Ok(false);
                }
            }
        }

        // All placement/ability preparation precedes even cooldown bookkeeping.
        let Some(entity) = self.entities.get_mut(&entity_id) else {
            return Ok(false);
        };
        if let Some(prepared) = prepared_overclock {
            entity.commit_overclock(prepared);
        }
        entity.energy = prepared_energy;
        if (ability_index as usize) < entity.ability_cooldowns.len() {
            entity.ability_cooldowns[ability_index as usize] = ability.cooldown_ticks;
        }

        let effect = ability.effect.clone();
        let entity_owner = entity.owner;
        match effect {
            AbilityEffectSpec::Overclock(_) => {
                // The prepared buff committed atomically with resources above.
            }
            AbilityEffectSpec::SiegeModeToggle { cast_ticks, .. } => {
                if cast_ticks > 0 {
                    entity.begin_mode_channel(cast_ticks, ability_index, target_pos, target_entity);
                } else {
                    entity.apply_mode_transition(arch, &effect);
                }
                entity.velocity = SimVec2::ZERO;
                entity.current_funnel_waypoints.clear();
            }
            AbilityEffectSpec::SwitchMode { ref mode_name } => {
                if let Some(spec) = arch.get_mode(mode_name) {
                    if spec.cast_ticks > 0 {
                        entity.begin_mode_channel(
                            spec.cast_ticks,
                            ability_index,
                            target_pos,
                            target_entity,
                        );
                    } else {
                        entity.apply_mode_transition(arch, &effect);
                    }
                    entity.velocity = SimVec2::ZERO;
                    entity.current_funnel_waypoints.clear();
                }
            }
            AbilityEffectSpec::BlinkTeleport { .. } => {
                if let Some(dest) = target_pos {
                    entity.position = dest;
                    entity.velocity = SimVec2::ZERO;
                    entity.current_funnel_waypoints.clear();
                }
            }
            AbilityEffectSpec::PsionicStormAoE {
                radius,
                delay_ticks,
                duration_ticks,
                interval_ticks,
                damage_per_interval,
                friendly_fire,
            } => {
                if let Some(pos) = target_pos {
                    let h_id = self.next_hazard_id;
                    self.next_hazard_id = self.next_hazard_id.wrapping_add(1);
                    self.active_hazards.push(ActiveHazardZone {
                        id: h_id,
                        owner: entity_owner,
                        center: pos,
                        radius,
                        delay_ticks,
                        remaining_ticks: duration_ticks,
                        interval_ticks,
                        ticks_until_next_interval: delay_ticks,
                        damage_per_interval,
                        targets_friendly: friendly_fire,
                    });
                }
            }
            AbilityEffectSpec::CloakToggle { is_burrow, .. } => {
                entity.is_cloaked = !entity.is_cloaked;
                if is_burrow {
                    entity.is_immobile = entity.is_cloaked;
                    entity.unpushable = entity.is_cloaked;
                    if entity.is_cloaked {
                        entity.velocity = SimVec2::ZERO;
                        entity.current_funnel_waypoints.clear();
                    }
                }
            }
        }

        Ok(true)
    }
}
