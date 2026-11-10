//! Active abilities and the hazard zones they leave behind.
//!
//! # Responsibility
//! Owns [`ActiveHazardZone`], ability casting (admission, payment and effect), the
//! world-level overclock used by the stim order, and the phase 2 step of hazard zones.
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

use crate::catalog::UnitArchetype;
use crate::entity::{Entity, PreparedOverclock};
use crate::movement::{DisplacementAdmissionError, MovementInvariant, MovementMathError};
use crate::SimWorld;
use rand::Rng;
use sim_types::{
    AbilityEffectSpec, EntityId, PlayerId, SimAngle, SimScalar, SimVec2, Tick, UnitModeSpec,
    UnitOrder,
};

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
    /// The caster's owner, who owns any hazard zone the cast opens.
    owner: PlayerId,
}

/// The verdict of an ability effect's own admission checks.
enum EffectAdmission {
    /// The cast is refused without payment.
    Refused,
    /// The cast may proceed, with the prepared buff of an overclock effect (`None` otherwise).
    Admitted(Option<PreparedOverclock>),
}

/// The authored parameters of one hazard zone, as a storm ability states them.
struct HazardSpec {
    /// Radius of effect in world units.
    radius: SimScalar,
    /// Ticks before the first damage pulse.
    delay_ticks: u16,
    /// Lifetime in ticks.
    duration_ticks: u16,
    /// Ticks between damage pulses.
    interval_ticks: u16,
    /// Damage of each pulse.
    damage_per_interval: SimScalar,
    /// Whether the owner's own entities are also damaged.
    friendly_fire: bool,
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
        let Some(cast) = self.admit_cast(entity_id, ability_index, target_pos)? else {
            return Ok(false);
        };
        Ok(self.commit_cast(entity_id, cast, target_pos, target_entity))
    }

    /// Whether a cast of the ability at `ability_index` by `entity_id`, aimed at `target_pos` in
    /// world units, would be admitted now: the simulation's own admission, run without mutation
    /// and without drawing from the generator.
    ///
    /// `false` for every ordinary refusal (dead, carried, channelling, unknown or locked
    /// ability, cooldown, energy, range or placement) and also for malformed authored data,
    /// which a real cast reports as a tick diagnostic. The client asks this before it submits
    /// a mode switch, so its admission cannot drift from the tick's (plan
    /// `2026-11-09-client-mode-commands` D-4).
    #[must_use]
    pub fn would_admit_cast(
        &self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
    ) -> bool {
        matches!(
            self.admit_cast(entity_id, ability_index, target_pos),
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
    ) -> Result<Option<PreparedCast>, MovementInvariant> {
        let entity = match self.entities.get(&entity_id) {
            Some(e) if e.is_alive() => e,
            _ => return Ok(None),
        };

        if entity.is_loaded_in_transport() || entity.channeling_remaining_ticks > 0 {
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

        if ability.cast_range > SimScalar::ZERO {
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
            owner: entity.owner,
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
                let Some(dest) = target_pos else {
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
            AbilityEffectSpec::SpawnHazardZone { .. } => {
                if target_pos.is_none() {
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
            owner,
        } = cast;
        if let Some(prepared) = overclock {
            entity.commit_overclock(prepared);
        }
        entity.energy = energy;
        if (ability_index as usize) < entity.ability_cooldowns.len() {
            entity.ability_cooldowns[ability_index as usize] = cooldown_ticks;
        }

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
            AbilityEffectSpec::SpawnHazardZone {
                radius,
                delay_ticks,
                duration_ticks,
                interval_ticks,
                damage_per_interval,
                friendly_fire,
            } => {
                if let Some(pos) = target_pos {
                    let spec = HazardSpec {
                        radius,
                        delay_ticks,
                        duration_ticks,
                        interval_ticks,
                        damage_per_interval,
                        friendly_fire,
                    };
                    self.spawn_hazard_zone(owner, pos, &spec);
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

        true
    }

    /// Opens a hazard zone of `spec` owned by `owner` at `center`, taking the next hazard ID.
    fn spawn_hazard_zone(&mut self, owner: PlayerId, center: SimVec2, spec: &HazardSpec) {
        let h_id = self.next_hazard_id;
        self.next_hazard_id = self.next_hazard_id.wrapping_add(1);
        self.active_hazards.push(ActiveHazardZone {
            id: h_id,
            owner,
            center,
            radius: spec.radius,
            delay_ticks: spec.delay_ticks,
            remaining_ticks: spec.duration_ticks,
            interval_ticks: spec.interval_ticks,
            ticks_until_next_interval: spec.delay_ticks,
            damage_per_interval: spec.damage_per_interval,
            targets_friendly: spec.friendly_fire,
        });
    }

    /// Phase 2: ages each hazard zone, applies its damage pulses, and sends idle mobile
    /// victims out of its radius. Expired zones are removed after the pass.
    pub(super) fn step_hazards(&mut self) {
        let cur_tick = self.current_tick;
        let mut expired_hazards = Vec::new();
        let mut hazard_flee_candidates = Vec::new();
        for (h_idx, hazard) in self.active_hazards.iter_mut().enumerate() {
            if hazard.remaining_ticks == 0 {
                expired_hazards.push(h_idx);
                continue;
            }
            hazard.remaining_ticks = hazard.remaining_ticks.saturating_sub(1);

            if hazard.ticks_until_next_interval > 0 {
                hazard.ticks_until_next_interval =
                    hazard.ticks_until_next_interval.saturating_sub(1);
            }

            if hazard.ticks_until_next_interval == 0 {
                hazard.ticks_until_next_interval = hazard.interval_ticks;

                for (&entity_id, entity) in &mut self.entities {
                    if !Self::is_damageable(entity) {
                        continue;
                    }
                    if !hazard.targets_friendly && entity.owner == hazard.owner {
                        continue;
                    }
                    if entity.last_storm_damage_tick == Some(Tick(cur_tick)) {
                        continue;
                    }
                    let dist = hazard.center.distance(entity.position);
                    if dist <= hazard.radius {
                        // The barrier absorbs spell damage too (B-41).
                        let barrier = self
                            .catalog
                            .get(entity.unit_type_id)
                            .ok()
                            .and_then(|arch| arch.capabilities.barrier.as_ref());
                        let dmg = entity.barrier.absorb(barrier, hazard.damage_per_interval);
                        if dmg > SimScalar::ZERO {
                            let (shield_dmg, life_dmg) = if entity.shields >= dmg {
                                (dmg, SimScalar::ZERO)
                            } else {
                                let s = entity.shields;
                                (s, dmg - s)
                            };
                            entity.apply_damage(shield_dmg, life_dmg, SimAngle::ZERO);
                        }
                        entity.last_storm_damage_tick = Some(Tick(cur_tick));

                        // Idle mobile units evacuate the persistent hazard zone
                        if matches!(entity.order_queue.front(), None | Some(UnitOrder::Idle))
                            && !entity.is_holding_position
                            && !entity.is_immobile
                            && !entity.is_loaded_in_transport()
                        {
                            hazard_flee_candidates.push((
                                entity_id,
                                entity.position,
                                hazard.center,
                                hazard.radius,
                            ));
                        }
                    }
                }
            }
        }
        for h_idx in expired_hazards.into_iter().rev() {
            self.active_hazards.remove(h_idx);
        }

        // Direct hazard evacuation moves outside the affected radius
        for (id, _pos, center, radius) in hazard_flee_candidates {
            let flee_dist = radius + SimScalar::from_num(2);
            let flee = self.compute_entity_flee_destination_prevalidated(id, center, flee_dist);
            if let Err(source) = flee {
                self.report_movement_invariant(source);
                continue;
            }
            if let Ok(Some(flee_target)) = flee {
                if let Some(unit) = self.entities.get_mut(&id) {
                    unit.order_queue.clear();
                    unit.enqueue_order(UnitOrder::Move {
                        target: flee_target,
                    });
                    unit.current_funnel_waypoints.clear();
                    unit.current_target = None;
                }
            }
        }
    }
}
