//! Tick phase 2: production, per-entity timers and regeneration, and the effect engine.
//!
//! # Responsibility
//! Advances production, then each living entity's channel, cooldown, buff, damage-delay,
//! regeneration, mode-drain, strike-magazine, hit-flash and timed-life timers, then the effect
//! engine's missiles, persistents, statuses
//! and displacements (stepped by `crate::effects`, which owns them), then completes
//! production.
//!
//! # How it fits
//! The second phase of `step_into`, after commands and before unit tasks. Damage (phase 9,
//! effects here) resets `Entity::ticks_since_last_damage` through `Entity::apply_damage`; this
//! phase counts it up and gates life and shield regeneration on the archetype's catalog delays
//! (`hp_regen_delay_ticks`, `shield_regen_delay_ticks`). Construction (phase of unit tasks,
//! `production::construction`) completes a foundation after this phase, so regeneration
//! starts on the tick after completion.
//!
//! # Invariants
//! - Dead entities are skipped: regeneration never resurrects them before reap.
//! - Entities are visited in `EntityId` order, so the phase is deterministic.
//! - `ticks_since_last_damage` rises by exactly one per tick per living entity, before any
//!   regeneration reads it, and is reset to 0 only by `Entity::apply_damage`, for damage to
//!   shields or life.
//! - No vital regenerates before the entity is completed (plan `2026-11-23-regeneration`
//!   D-1): a foundation gains life only from the work done on it and holds its energy, and
//!   regeneration starts on the tick after completion. The counter counts during
//!   construction all the same.
//! - Life regenerates on a tick if and only if the entity is completed and that counter is at
//!   least `hp_regen_delay_ticks`: a delay of d regenerates first on the d-th tick after the
//!   damage, and 0 means every tick.
//! - Energy regenerates on every tick the entity is completed, with no delay: neither a cast
//!   nor damage pauses it, as no layer of the 5.0.14 data sets `EnergyRegenDelay`.
//! - Shields regenerate on a tick if and only if the counter is at least
//!   `shield_regen_delay_ticks` and the entity is completed.
//! - Regeneration never exceeds `max_hp` or `max_shields`.
//! - A new entity's counter is `u16::MAX`, which satisfies every delay.
//! - A strike magazine (plan `2026-11-28-broodlings` D-3) steps once per tick, before the weapon
//!   phase reads it ([`crate::entity::StrikeMagazineState::tick`]).
//! - A timed life (D-5) falls by one per tick and is counted last: on the tick it reaches 0 the
//!   unit's life and shields are set to 0 with no damage source, after regeneration, so the reap
//!   of the same tick removes it with its ordinary death. A unit created on tick T with `n` ticks
//!   dies on tick T + n.

use crate::catalog::{UnitArchetype, UnitCatalog};
use crate::entity::Entity;
use crate::SimWorld;
use sim_types::SimScalar;

impl SimWorld {
    /// Phase 2: Update production timers, buffs, passive regeneration, and shield delays.
    ///
    /// One front production job advances per entity, including jobs enqueued in
    /// phase 1 of this tick. A zero-duration job completes on its first update;
    /// the next queued job starts advancing on the following update.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    pub fn update_production_and_buffs(&mut self) {
        self.advance_production();
        let catalog = &self.catalog;

        for entity in self.entities.values_mut() {
            upkeep_entity(entity, catalog);
        }

        self.step_effects();

        // Effects may have killed a ready producer; completion rechecks its life.
        self.complete_production();
    }
}

/// One tick of a docked hangar member's regeneration (plan `2026-11-28-carrier-interceptors`
/// D-3): the clock rises, then life and shields regenerate by the member archetype `arch`'s own
/// rules, as a completed entity's do in [`upkeep_entity`]. `max_hp` is the member's life maximum
/// for its owner.
pub(crate) fn regenerate_docked(
    arch: &UnitArchetype,
    max_hp: SimScalar,
    member: &mut crate::hangar::DockedMember,
) {
    member.ticks_since_last_damage = member.ticks_since_last_damage.saturating_add(1);
    if member.ticks_since_last_damage >= arch.hp_regen_delay_ticks {
        member.hp = (member.hp + arch.hp_regen_per_tick).min(max_hp);
    }
    if shields_regenerate(arch, member.ticks_since_last_damage) {
        member.shields = (member.shields + arch.shield_regen_per_tick).min(arch.max_shields);
    }
}

/// Whether shields regenerate `ticks_since_last_damage` ticks after the last damage: the
/// archetype has a shield regeneration delay and it has passed.
fn shields_regenerate(arch: &UnitArchetype, ticks_since_last_damage: u16) -> bool {
    arch.shield_regen_delay_ticks
        .is_some_and(|delay| ticks_since_last_damage >= delay)
}

/// Advances one living entity's channel, cooldown, buff, damage-delay, life and energy
/// regeneration (completed entities only), mode-drain, shield regeneration, strike-magazine,
/// hit-flash and timed-life timers by one tick, in that order. Dead entities are left untouched
/// until reap.
fn upkeep_entity(entity: &mut Entity, catalog: &UnitCatalog) {
    // Dead entities retain their ledger until reap; passive regeneration
    // must never resurrect them or resume their production.
    if !entity.is_alive() {
        return;
    }
    // Channeling timers
    if entity.channeling_remaining_ticks > 0 {
        entity.channeling_remaining_ticks = entity.channeling_remaining_ticks.saturating_sub(1);
        if entity.channeling_remaining_ticks == 0 {
            if let Some((ability_index, _, _)) = entity.pending_ability.take() {
                if let Ok(arch) = catalog.get(entity.unit_type_id) {
                    match arch.abilities.get(usize::from(ability_index)) {
                        Some(ability) => {
                            entity.apply_mode_transition(arch, &ability.effect);
                        }
                        None => entity.leave_mode(arch),
                    }
                }
            }
        }
    }

    // Ability cooldowns
    for cd in &mut entity.ability_cooldowns {
        *cd = cd.saturating_sub(1);
    }

    // The barrier's window and cooldown (B-41).
    entity.barrier.tick();

    // The attack dash's cooldown (B-43).
    entity.attack_dash_cooldown = entity.attack_dash_cooldown.saturating_sub(1);

    // Buff timers
    for buff in &mut entity.active_buffs {
        buff.remaining_ticks = buff.remaining_ticks.saturating_sub(1);
    }
    entity.active_buffs.retain(|b| b.remaining_ticks > 0);

    // Passive Vitals & regeneration delays: the counter rises before either delay reads it,
    // and counts on a foundation too, so a structure finished under fire still waits its delays.
    if let Ok(arch) = catalog.get(entity.unit_type_id) {
        entity.ticks_since_last_damage = entity.ticks_since_last_damage.saturating_add(1);
        // A foundation gains life only from the work done on it (plan
        // `2026-11-23-regeneration` D-1); energy has no delay (no `EnergyRegenDelay` in 5.0.14).
        if entity.is_completed {
            if entity.ticks_since_last_damage >= arch.hp_regen_delay_ticks {
                entity.hp = (entity.hp + arch.hp_regen_per_tick).min(entity.max_hp);
            }
            entity.energy = (entity.energy + arch.energy_regen_per_tick).min(arch.max_energy);
        }

        // Continuous energy drain for active stance/mode
        if let Some(ref mode_name) = entity.active_mode {
            if let Some(mode) = arch.get_mode(mode_name) {
                if mode.energy_cost_per_second > SimScalar::ZERO {
                    let drain_per_tick = mode.energy_cost_per_second
                        / SimScalar::from_i32(i32::from(sim_types::TICKS_PER_SECOND));
                    if entity.energy < drain_per_tick {
                        entity.energy = SimScalar::ZERO;
                        entity.leave_mode(arch);
                    } else {
                        entity.energy -= drain_per_tick;
                    }
                }
            }
        }

        let regenerates = shields_regenerate(arch, entity.ticks_since_last_damage);
        if regenerates && entity.is_completed {
            entity.shields = (entity.shields + arch.shield_regen_per_tick).min(arch.max_shields);
        }

        // The strike magazine's arming and build (plan `2026-11-28-broodlings` D-3).
        if let (Some(spec), Some(magazine)) = (
            arch.capabilities.strike_magazine,
            entity.strike_magazine.as_mut(),
        ) {
            magazine.tick(spec);
        }
    }

    // Hit flash timers
    entity.shield_hit_flash_ticks = entity.shield_hit_flash_ticks.saturating_sub(1);
    entity.hull_hit_flash_ticks = entity.hull_hit_flash_ticks.saturating_sub(1);

    // The clock runs out last, after regeneration (plan `2026-11-28-broodlings` D-5).
    if let Some(left) = entity.timed_life.as_mut() {
        *left = left.saturating_sub(1);
        if *left == 0 {
            entity.hp = SimScalar::ZERO;
            entity.shields = SimScalar::ZERO;
        }
    }
}
