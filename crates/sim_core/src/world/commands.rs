//! Tick phase 1: player commands.
//!
//! # Responsibility
//! Dispatches each command of a [`TickBundle`] to the subsystem that admits it (economy,
//! movement, production and research, stim, ability casts, plain orders) and records one
//! outcome per command where that subsystem reports outcomes.
//!
//! # How it fits
//! The first phase of `step_into`; the public `process_commands` validates transport links once
//! and runs the same dispatch for direct phase callers.
//!
//! # Invariants
//! - Commands are handled in bundle order, and each command's outcome carries its index in the
//!   bundle.
//! - No command writes carrier links, so one transport validation covers the whole phase.

use crate::economy::{GameCommandError, GameCommandOutcome};
use crate::movement::{MovementInvariant, MovementOutcome};
use crate::upgrades::OVERCLOCK_ABILITY_SLUG;
use crate::weapon_timing::SimulationDiagnostic;
use crate::SimWorld;
use sim_types::{Tick, TickBundle, UnitOrder};

impl SimWorld {
    /// Phase 1: Process scheduled player input commands.
    pub fn process_commands(&mut self, input: &TickBundle) {
        if let Err(source) = self.validate_transport_links() {
            self.report_transport_invariant(source);
            return;
        }
        self.process_commands_prevalidated(input);
    }

    // Normal step or public dispatcher validated once. No command writes carrier links.
    #[expect(
        clippy::too_many_lines,
        reason = "split in plan steps 3.1 and 9.1, which own this file's structure"
    )]
    pub(super) fn process_commands_prevalidated(&mut self, input: &TickBundle) {
        for (command_index, cmd) in input.commands.iter().enumerate() {
            let closed = if self.match_state.result.is_over() {
                Some(GameCommandError::MatchOver)
            } else if self.match_state.is_defeated(cmd.player_id) {
                Some(GameCommandError::PlayerDefeated(cmd.player_id))
            } else {
                None
            };
            if let Some(error) = closed {
                self.economy.outcomes.push(GameCommandOutcome {
                    tick: Tick(self.current_tick),
                    command_index,
                    player: cmd.player_id,
                    result: Err(error),
                });
                continue;
            }
            if matches!(
                cmd.order,
                UnitOrder::Build { .. }
                    | UnitOrder::Construct { .. }
                    | UnitOrder::CancelConstruction
                    | UnitOrder::Harvest { .. }
                    | UnitOrder::ReturnCargo
                    | UnitOrder::CancelProduction { .. }
                    | UnitOrder::Load { .. }
                    | UnitOrder::UnloadAll
            ) {
                let result = self.admit_game_command(cmd);
                if let Err(GameCommandError::Invariant(source)) = &result {
                    self.report_production_failure(source.clone());
                }
                self.economy.outcomes.push(GameCommandOutcome {
                    tick: Tick(self.current_tick),
                    command_index,
                    player: cmd.player_id,
                    result,
                });
                continue;
            }
            if matches!(
                cmd.order,
                UnitOrder::Move { .. } | UnitOrder::AttackMove { .. }
            ) {
                let result = self.issue_movement_prevalidated(
                    cmd.player_id,
                    &cmd.entities,
                    &cmd.order,
                    cmd.queue,
                );
                self.movement.outcomes.push(MovementOutcome {
                    tick: Tick(self.current_tick),
                    command_index,
                    player: cmd.player_id,
                    result,
                });
                continue;
            }
            if let UnitOrder::Produce { .. } | UnitOrder::Research { .. } = cmd.order {
                let result = match cmd.order {
                    UnitOrder::Research { upgrade } => {
                        self.queue_research_prevalidated(cmd.player_id, &cmd.entities, upgrade)
                    }
                    UnitOrder::Produce { unit_type } => {
                        self.queue_production_prevalidated(cmd.player_id, &cmd.entities, unit_type)
                    }
                    _ => continue,
                };
                self.production
                    .outcomes
                    .push(crate::production::ProductionOutcome {
                        tick: Tick(self.current_tick),
                        command_index,
                        player: cmd.player_id,
                        result,
                    });
                continue;
            }
            let player_idx = cmd.player_id.0 as usize;
            if player_idx >= self.player_resources.len() {
                continue;
            }

            match &cmd.order {
                UnitOrder::UseStim => {
                    for &ent_id in &cmd.entities {
                        let Some(actor) = self
                            .entities
                            .get(&ent_id)
                            .filter(|e| e.owner == cmd.player_id)
                        else {
                            continue;
                        };
                        let spec = if let Ok(arch) = self.catalog.get(actor.unit_type_id) {
                            arch.overclock.filter(|_| {
                                self.is_ability_unlocked(arch, actor.owner, OVERCLOCK_ABILITY_SLUG)
                            })
                        } else {
                            self.report_movement_invariant(MovementInvariant::MissingArchetype(
                                ent_id,
                            ));
                            continue;
                        };
                        if let Some(spec) = spec {
                            if let Err(source) = self.apply_world_overclock(ent_id, spec) {
                                self.report_movement_invariant(source);
                            }
                        }
                    }
                }
                UnitOrder::CastAbility {
                    ability_index,
                    target_pos,
                    target_entity,
                } => {
                    for &ent_id in &cmd.entities {
                        if let Some(ent) = self.entities.get_mut(&ent_id) {
                            if ent.owner == cmd.player_id {
                                if cmd.queue && !ent.order_queue.is_empty() {
                                    if let Err(source) = ent.issue_order(cmd.order.clone(), true) {
                                        self.tick_diagnostics.push(
                                            SimulationDiagnostic::MovementInvariant {
                                                tick: Tick(self.current_tick),
                                                source: MovementInvariant::Admission {
                                                    entity: ent_id,
                                                    source,
                                                },
                                            },
                                        );
                                    }
                                } else {
                                    self.execute_ability_prevalidated(
                                        ent_id,
                                        *ability_index,
                                        *target_pos,
                                        *target_entity,
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {
                    for &ent_id in &cmd.entities {
                        if let Some(ent) = self.entities.get_mut(&ent_id) {
                            if ent.owner == cmd.player_id {
                                if let Err(source) = ent.issue_order(cmd.order.clone(), cmd.queue) {
                                    self.tick_diagnostics.push(
                                        SimulationDiagnostic::MovementInvariant {
                                            tick: Tick(self.current_tick),
                                            source: MovementInvariant::Admission {
                                                entity: ent_id,
                                                source,
                                            },
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
