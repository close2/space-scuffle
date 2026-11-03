//! Tick phase 2: production, per-entity timers and regeneration, and hazard zones.
//!
//! # Responsibility
//! Advances production, then each living entity's channel, cooldown, buff, regeneration,
//! mode-drain, shield-delay and hit-flash timers, then the active hazard zones, then completes
//! production.
//!
//! # How it fits
//! The second phase of `step_into`, after commands and before unit tasks.
//!
//! # Invariants
//! - Dead entities are skipped: regeneration never resurrects them before reap.
//! - Entities are visited in `EntityId` order, so the phase is deterministic.

use crate::SimWorld;
use sim_types::{SimAngle, SimScalar, Tick, UnitOrder};

impl SimWorld {
    /// Phase 2: Update production timers, buffs, passive regeneration, and shield delays.
    ///
    /// One front production job advances per entity, including jobs enqueued in
    /// phase 1 of this tick. A zero-duration job completes on its first update;
    /// the next queued job starts advancing on the following update.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    #[expect(
        clippy::too_many_lines,
        reason = "split in plan steps 3.1 and 9.1, which own this file's structure"
    )]
    pub fn update_production_and_buffs(&mut self) {
        self.advance_production();
        let catalog = &self.catalog;

        for entity in self.entities.values_mut() {
            // Dead entities retain their ledger until reap; passive regeneration
            // must never resurrect them or resume their production.
            if !entity.is_alive() {
                continue;
            }
            // Channeling timers
            if entity.channeling_remaining_ticks > 0 {
                entity.channeling_remaining_ticks =
                    entity.channeling_remaining_ticks.saturating_sub(1);
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

            // Buff timers
            for buff in &mut entity.active_buffs {
                buff.remaining_ticks = buff.remaining_ticks.saturating_sub(1);
            }
            entity.active_buffs.retain(|b| b.remaining_ticks > 0);

            // Passive Vitals & Shield delay
            if let Ok(arch) = catalog.get(entity.unit_type_id) {
                entity.hp = (entity.hp + arch.hp_regen_per_tick).min(entity.max_hp);
                entity.energy = (entity.energy + arch.energy_regen_per_tick).min(arch.max_energy);

                // Continuous energy drain for active stance/mode
                if let Some(ref mode_name) = entity.active_mode {
                    if let Some(mode) = arch.get_mode(mode_name) {
                        if mode.energy_cost_per_second > SimScalar::ZERO {
                            let drain_per_tick =
                                mode.energy_cost_per_second / SimScalar::from_i32(25);
                            if entity.energy < drain_per_tick {
                                entity.energy = SimScalar::ZERO;
                                entity.leave_mode(arch);
                            } else {
                                entity.energy -= drain_per_tick;
                            }
                        }
                    }
                }

                entity.ticks_since_last_damage = entity.ticks_since_last_damage.saturating_add(1);
                if entity.ticks_since_last_damage >= 250 && entity.is_completed {
                    entity.shields =
                        (entity.shields + arch.shield_regen_per_tick).min(arch.max_shields);
                }
            }

            // Hit flash timers
            entity.shield_hit_flash_ticks = entity.shield_hit_flash_ticks.saturating_sub(1);
            entity.hull_hit_flash_ticks = entity.hull_hit_flash_ticks.saturating_sub(1);
        }

        // Step active hazard zones (e.g. Psionic Storm)
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
                        let dmg = hazard.damage_per_interval;
                        let (shield_dmg, life_dmg) = if entity.shields >= dmg {
                            (dmg, SimScalar::ZERO)
                        } else {
                            let s = entity.shields;
                            (s, dmg - s)
                        };
                        entity.apply_damage(shield_dmg, life_dmg, SimAngle::ZERO);
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
                    unit.order_queue.push_back(UnitOrder::Move {
                        target: flee_target,
                    });
                    unit.current_funnel_waypoints.clear();
                    unit.current_target = None;
                }
            }
        }

        // Hazards may have killed a ready producer; completion rechecks its life.
        self.complete_production();
    }
}
