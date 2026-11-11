//! Active abilities: admission, payment and effect of a cast.
//!
//! # Responsibility
//! Owns ability casting (admission, payment and effect) and the world-level overclock used by
//! the stim order. An `Effect` ability starts a walk of its effect tree, which the effect
//! engine (`crate::effects`) evaluates and steps in phase 2.
//!
//! # How it fits
//! Casts run in phase 1 (`process_commands`) and when a queued cast reaches the front of an
//! order queue during movement.
//!
//! # Invariants
//! - Every fallible check of a cast runs before any write: a refused or failed cast leaves
//!   energy, cooldowns and the entity untouched.

use crate::catalog::UnitArchetype;
use crate::entity::{Entity, PreparedOverclock};
use crate::movement::{DisplacementAdmissionError, MovementInvariant, MovementMathError};
use crate::SimWorld;
use rand::Rng;
use sim_types::{AbilityEffectSpec, AbilitySource, EntityId, SimScalar, SimVec2, UnitModeSpec};

/// Everything an admitted cast will write, computed without mutation.
struct PreparedCast {
    /// Index of the cast ability in the caster's archetype.
    ability_index: u8,
    /// The caster's energy after paying the cost.
    energy: SimScalar,
    /// Cooldown, in 25 Hz ticks, the ability enters.
    cooldown_ticks: u16,
    /// The prepared buff of an overclock effect; `None` for every other effect.
    overclock: Option<PreparedOverclock>,
    /// The effect to apply.
    effect: AbilityEffectSpec,
    /// Ticks the caster stays locked after the cast (`cast_backswing_ticks`, plan D-7.4).
    backswing_ticks: u16,
    /// The effect tree's origin, placed at admission (plan D-6); the caster's position for
    /// every other ability.
    origin: SimVec2,
}

/// The verdict of an ability effect's own admission checks.
enum EffectAdmission {
    /// The cast is refused without payment.
    Refused,
    /// The cast may proceed, with the prepared buff of an overclock effect (`None` otherwise).
    Admitted(Option<PreparedOverclock>),
}

/// The channel, in ticks, of entering `spec` (or of leaving it when `leaving`): the direction's
/// fixed time plus a random extra drawn uniformly from `0..=max` ticks with `rng`, which must be
/// the world's seeded generator, and rounded half up to whole ticks.
///
/// `StarCraft` II's morphs add a random delay "just before the morph starts" (`SC2Mapster`
/// `CAbilMorph` reference, `InfoArray` `RandomDelayMax`; Siege Mode and Unsiege 0.5 Normal s,
/// `liberty` AbilData:1308, :1334), and each direction has its own sections (plan
/// `2026-11-05-sc2-deviation-fixes` WP11, B-4). A maximum of 0 draws nothing, so a mode
/// without an extra leaves the generator untouched. Saturates at `u16::MAX` ticks.
fn mode_channel_ticks(spec: &UnitModeSpec, leaving: bool, rng: &mut impl Rng) -> u16 {
    let (base, max) = if leaving {
        (spec.leave_cast_ticks, spec.leave_random_delay_max_ticks)
    } else {
        (spec.cast_ticks, spec.cast_random_delay_max_ticks)
    };
    if max <= SimScalar::ZERO {
        return base;
    }
    let extra = SimScalar::from_raw(rng.gen_range(0..=max.raw()));
    // Q32.32: adding one half and shifting down by 32 rounds half up to whole ticks.
    let whole = extra.saturating_add(SimScalar::HALF).raw() >> 32;
    u16::try_from(i64::from(base).saturating_add(whole)).unwrap_or(u16::MAX)
}

/// Toggles `entity`'s cloak; a burrowing cloak also anchors it while hidden and stops it.
fn toggle_cloak(entity: &mut Entity, is_burrow: bool) {
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
    pub(crate) fn try_execute_ability_prevalidated(
        &mut self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> Result<bool, MovementInvariant> {
        let Some(cast) = self.admit_cast(entity_id, ability_index, target_pos, target_entity)?
        else {
            return Ok(false);
        };
        Ok(self.commit_cast(entity_id, cast, target_pos, target_entity))
    }

    /// Whether a cast of the ability at `ability_index` by `entity_id`, aimed at `target_pos` in
    /// world units, would be admitted now: the simulation's own admission, run without mutation
    /// and without drawing from the generator.
    ///
    /// `false` for every ordinary refusal (dead, carried, channelling, stunned or locked after a
    /// cast, unknown or locked ability, cooldown, energy, range, target or placement) and also
    /// for malformed authored data,
    /// which a real cast reports as a tick diagnostic. The client asks this before it submits
    /// a mode switch, so its admission cannot drift from the tick's (plan
    /// `2026-11-09-client-mode-commands` D-4).
    #[must_use]
    pub fn would_admit_cast(
        &self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> bool {
        matches!(
            self.admit_cast(entity_id, ability_index, target_pos, target_entity),
            Ok(Some(_))
        )
    }

    /// Runs every check of a cast, in order, without mutation.
    ///
    /// Returns `Ok(None)` for an ordinary refusal (dead, carried, channeling, unknown or locked
    /// ability, cooldown, energy, range or placement) and an error for malformed authored data.
    fn admit_cast(
        &self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> Result<Option<PreparedCast>, MovementInvariant> {
        let entity = match self.entities.get(&entity_id) {
            Some(e) if e.is_alive() => e,
            _ => return Ok(None),
        };

        if entity.is_loaded_in_transport()
            || entity.channeling_remaining_ticks > 0
            || entity.is_action_locked()
        {
            return Ok(None);
        }

        let arch = self
            .catalog
            .get(entity.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(entity_id))?;

        let Some(ability) = arch.abilities.get(ability_index as usize) else {
            return Ok(None);
        };
        if !self.is_ability_unlocked(arch, entity.owner, &ability.name) {
            return Ok(None);
        }

        if let Some(&cd) = entity.ability_cooldowns.get(ability_index as usize) {
            if cd > 0 {
                return Ok(None);
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
            return Ok(None);
        }

        let mut origin = entity.position;
        if matches!(ability.effect, AbilityEffectSpec::Effect(_)) {
            // The cast pipeline of plan `2026-11-10-spellcaster-mechanics` D-7: the target
            // suits the ability and is seen, it stands in range of the caster's edge, and the
            // origin finds a place (D-6).
            let targets = (target_pos, target_entity);
            if !self.cast_target_admitted(entity, ability, ability_index, targets)
                || self.cast_in_range(entity, ability, targets) != Some(true)
            {
                return Ok(None);
            }
            match self.place_cast_origin(entity, ability, target_entity) {
                Some(placed) => origin = placed,
                None => return Ok(None),
            }
        } else if ability.cast_range > SimScalar::ZERO {
            if let Some(pos) = target_pos {
                if !crate::movement::math::within_distance(
                    entity.position,
                    pos,
                    i128::from(ability.cast_range.raw()),
                ) {
                    return Ok(None);
                }
            }
        }

        let prepared_energy = entity
            .energy
            .checked_sub(ability.energy_cost)
            .ok_or_else(|| admission(crate::entity::EntityAdmissionError::PaymentOverflow))?;
        // Validate every fallible payment/rate/placement before any commit.
        let EffectAdmission::Admitted(overclock) =
            self.admit_effect(entity_id, entity, arch, &ability.effect, target_pos)?
        else {
            return Ok(None);
        };
        Ok(Some(PreparedCast {
            ability_index,
            energy: prepared_energy,
            cooldown_ticks: ability.cooldown_ticks,
            overclock,
            effect: ability.effect.clone(),
            backswing_ticks: ability.cast_backswing_ticks,
            origin,
        }))
    }

    /// Checks the effect-specific conditions of a cast without mutation.
    ///
    /// Refusals are ordinary outcomes; errors report malformed authored data.
    fn admit_effect(
        &self,
        entity_id: EntityId,
        entity: &Entity,
        arch: &UnitArchetype,
        effect: &AbilityEffectSpec,
        target_pos: Option<SimVec2>,
    ) -> Result<EffectAdmission, MovementInvariant> {
        let admission = |source| MovementInvariant::Admission {
            entity: entity_id,
            source,
        };
        let mut prepared_overclock = None;
        match effect {
            AbilityEffectSpec::Overclock(spec) => {
                let Some(prepared) = entity.prepare_overclock(*spec).map_err(admission)? else {
                    return Ok(EffectAdmission::Refused);
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
                // A status may forbid Blink (plan `2026-11-10-spellcaster-mechanics` §6.3).
                let Some(dest) = target_pos.filter(|_| !entity.status_modifiers().suppress_blink)
                else {
                    return Ok(EffectAdmission::Refused);
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
                    return Ok(EffectAdmission::Refused);
                }
                if let Err(error) =
                    self.validate_displacement_destination_prevalidated(entity_id, dest)
                {
                    if let DisplacementAdmissionError::Invariant(source) = error {
                        return Err(source);
                    }
                    return Ok(EffectAdmission::Refused);
                }
            }
            // The target, range and origin were admitted by `admit_cast` (`casts.rs`); the root
            // must exist in the loaded spell catalog.
            AbilityEffectSpec::Effect(key) => {
                if self.catalog.spells().effect_id(key).is_none() {
                    return Ok(EffectAdmission::Refused);
                }
            }
        }
        Ok(EffectAdmission::Admitted(prepared_overclock))
    }

    /// Commits an admitted cast: payment and cooldown, then the effect.
    ///
    /// Returns `false`, with nothing written, only if the caster or its archetype vanished
    /// since admission, which cannot happen while admission and commit run back to back.
    fn commit_cast(
        &mut self,
        entity_id: EntityId,
        cast: PreparedCast,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> bool {
        // All placement/ability preparation precedes even cooldown bookkeeping.
        let Some(entity) = self.entities.get_mut(&entity_id) else {
            return false;
        };
        let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
            return false;
        };
        let PreparedCast {
            ability_index,
            energy,
            cooldown_ticks,
            overclock,
            effect,
            backswing_ticks,
            origin,
        } = cast;
        entity.cast_lock_ticks = backswing_ticks;
        if let Some(prepared) = overclock {
            entity.commit_overclock(prepared);
        }
        entity.energy = energy;
        if let Some(cooldown) = entity.ability_cooldowns.get_mut(usize::from(ability_index)) {
            *cooldown = cooldown_ticks;
        }

        match effect {
            AbilityEffectSpec::Overclock(_) | AbilityEffectSpec::Effect(_) => {
                // The prepared buff committed atomically with resources above; an effect's
                // walk starts below, once the caster is no longer borrowed.
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
                    let leaving = entity.active_mode.as_deref() == Some(mode_name.as_str());
                    let channel_ticks = mode_channel_ticks(spec, leaving, &mut self.rng);
                    if channel_ticks > 0 {
                        entity.begin_mode_channel(
                            channel_ticks,
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
            AbilityEffectSpec::CloakToggle { is_burrow, .. } => toggle_cloak(entity, is_burrow),
        }

        if let AbilityEffectSpec::Effect(key) = &effect {
            self.start_cast_walk(
                entity_id,
                ability_index,
                key,
                origin,
                (target_pos, target_entity),
            );
        }
        true
    }

    /// Starts the walk of an `Effect` ability's root for a committed cast: the target unit if
    /// one was given, else the target point; `origin` is the point admission placed (plan
    /// `2026-11-10-spellcaster-mechanics` D-6) and the source point the caster's position.
    fn start_cast_walk(
        &mut self,
        caster: EntityId,
        ability_index: u8,
        key: &str,
        origin: SimVec2,
        (target_pos, target_entity): (Option<SimVec2>, Option<EntityId>),
    ) {
        let target = match (target_entity, target_pos) {
            (Some(unit), _) => sim_types::EffectTarget::Unit(unit),
            (None, Some(point)) => sim_types::EffectTarget::Point(point),
            (None, None) => return,
        };
        let (Some(root), Some(entity)) = (
            self.catalog.spells().effect_id(key),
            self.entities.get(&caster),
        ) else {
            return;
        };
        let context = sim_types::EffectContext {
            owner: entity.owner,
            caster: Some(caster),
            ability: AbilitySource {
                unit_type: entity.unit_type_id,
                ability: ability_index,
            },
            origin,
            source_point: entity.position,
            target,
        };
        self.run_effect_walk(root, context);
    }
}
