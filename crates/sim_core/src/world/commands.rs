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
//! - No command reaches a unit inside a structure (plan `2026-11-24-hidden-harvesters` D-8,
//!   Q0108's default (a)): such a recipient is dropped from the command, and a command left with
//!   no recipient is refused with `GameCommandError::HousedRecipient`.

use crate::economy::{GameCommandError, GameCommandOutcome};
use crate::movement::{MovementInvariant, MovementOutcome};
use crate::upgrades::OVERCLOCK_ABILITY_SLUG;
use crate::weapon_timing::SimulationDiagnostic;
use crate::SimWorld;
use sim_types::{
    AbilityTargetKind, EntityId, PlayerCommand, PlayerId, SimVec2, Tick, TickBundle, UnitOrder,
};

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
    pub(super) fn process_commands_prevalidated(&mut self, input: &TickBundle) {
        for (command_index, cmd) in input.commands.iter().enumerate() {
            self.dispatch_command(command_index, cmd);
        }
    }

    /// Routes one command to the subsystem that admits it, recording its outcome there.
    fn dispatch_command(&mut self, command_index: usize, cmd: &PlayerCommand) {
        if let Some(error) = self.closed_command_error(cmd) {
            self.economy.outcomes.push(GameCommandOutcome {
                tick: Tick(self.current_tick),
                command_index,
                player: cmd.player_id,
                result: Err(error),
            });
            return;
        }
        if cmd.entities.iter().any(|id| self.is_housed_unit(*id)) {
            self.dispatch_without_housed(command_index, cmd);
            return;
        }
        self.route_command(command_index, cmd);
    }

    /// Whether `id` is a unit inside a structure, gathering (plan `2026-11-24-hidden-harvesters`
    /// D-3).
    fn is_housed_unit(&self, id: EntityId) -> bool {
        self.entities
            .get(&id)
            .is_some_and(crate::entity::Entity::is_housed)
    }

    /// Dispatches `cmd` without its housed recipients: no order reaches a unit while it is inside
    /// a structure (plan `2026-11-24-hidden-harvesters` D-8, Q0108's default (a)), whether the
    /// command is immediate or queued, so the unit's gather runs its time. The command reaches
    /// the others; with no other recipient it is refused with
    /// [`GameCommandError::HousedRecipient`] naming the first housed one.
    ///
    /// The client sends no order to a housed unit, but a unit may go inside between the frame
    /// a command was given and the tick that runs it; dropping the unit rather than refusing the
    /// whole command keeps a group order from failing on that race.
    // PERFORMANCE RATIONALE:
    // Why: A command's recipients are borrowed from the tick bundle and cannot be edited there.
    // What: A command that names a housed unit is rebuilt once without it.
    // Trade-off: One allocation for such a command, rare and outside the steady state; a
    //            command naming no housed unit is routed as it is.
    fn dispatch_without_housed(&mut self, command_index: usize, cmd: &PlayerCommand) {
        let entities: Vec<EntityId> = cmd
            .entities
            .iter()
            .copied()
            .filter(|id| !self.is_housed_unit(*id))
            .collect();
        if entities.is_empty() {
            if let Some(&first) = cmd.entities.first() {
                self.economy.outcomes.push(GameCommandOutcome {
                    tick: Tick(self.current_tick),
                    command_index,
                    player: cmd.player_id,
                    result: Err(GameCommandError::HousedRecipient(first)),
                });
            }
            return;
        }
        let admitted = PlayerCommand {
            player_id: cmd.player_id,
            entities,
            order: cmd.order.clone(),
            queue: cmd.queue,
        };
        self.route_command(command_index, &admitted);
    }

    /// Routes a command whose recipients are all outside structures to the subsystem that
    /// admits it.
    fn route_command(&mut self, command_index: usize, cmd: &PlayerCommand) {
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
                | UnitOrder::SetRally { .. }
        ) {
            self.dispatch_game_command(command_index, cmd);
            return;
        }
        if matches!(
            cmd.order,
            UnitOrder::Move { .. } | UnitOrder::AttackMove { .. }
        ) {
            self.dispatch_movement(command_index, cmd);
            return;
        }
        if let UnitOrder::Produce { .. } | UnitOrder::Research { .. } = cmd.order {
            self.dispatch_production(command_index, cmd);
            return;
        }
        if cmd.player_id.slot().is_none() {
            return;
        }

        match &cmd.order {
            UnitOrder::UseStim => self.apply_stim(cmd),
            UnitOrder::CastAbility {
                ability_index,
                target_pos,
                target_entity,
            } => self.dispatch_cast(cmd, *ability_index, *target_pos, *target_entity),
            _ => self.issue_plain_order(cmd),
        }
    }

    /// Why no command of this player is admitted any more, if that is the case.
    fn closed_command_error(&self, cmd: &PlayerCommand) -> Option<GameCommandError> {
        if self.match_state.result.is_over() {
            Some(GameCommandError::MatchOver)
        } else if self.match_state.is_defeated(cmd.player_id) {
            Some(GameCommandError::PlayerDefeated(cmd.player_id))
        } else {
            None
        }
    }

    /// Admits a construction, harvest, cancellation, transport or rally command through the
    /// economy.
    fn dispatch_game_command(&mut self, command_index: usize, cmd: &PlayerCommand) {
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
    }

    /// Admits a move or attack-move through movement admission.
    fn dispatch_movement(&mut self, command_index: usize, cmd: &PlayerCommand) {
        let result =
            self.issue_movement_prevalidated(cmd.player_id, &cmd.entities, &cmd.order, cmd.queue);
        self.movement.outcomes.push(MovementOutcome {
            tick: Tick(self.current_tick),
            command_index,
            player: cmd.player_id,
            result,
        });
    }

    /// Queues a unit or a research job; any other order is not a production command.
    fn dispatch_production(&mut self, command_index: usize, cmd: &PlayerCommand) {
        let result = match cmd.order {
            UnitOrder::Research { upgrade } => {
                self.queue_research_prevalidated(cmd.player_id, &cmd.entities, upgrade)
            }
            UnitOrder::Produce { unit_type } => {
                self.queue_production_prevalidated(cmd.player_id, &cmd.entities, unit_type)
            }
            _ => return,
        };
        self.production
            .outcomes
            .push(crate::production::ProductionOutcome {
                tick: Tick(self.current_tick),
                command_index,
                player: cmd.player_id,
                result,
            });
    }

    /// Overclocks every commanded unit of the player whose archetype has an unlocked overclock.
    fn apply_stim(&mut self, cmd: &PlayerCommand) {
        for &ent_id in &cmd.entities {
            let Some(actor) = self
                .entities
                .get(&ent_id)
                .filter(|e| e.owner == cmd.player_id)
            else {
                continue;
            };
            let spec = if let Ok(arch) = self.catalog.get(actor.unit_type_id) {
                arch.overclock
                    .filter(|_| self.is_ability_unlocked(arch, actor.owner, OVERCLOCK_ABILITY_SLUG))
            } else {
                self.report_movement_invariant(MovementInvariant::MissingArchetype(ent_id));
                continue;
            };
            if let Some(spec) = spec {
                if let Err(source) = self.apply_world_overclock(ent_id, spec) {
                    self.report_movement_invariant(source);
                }
            }
        }
    }

    /// Casts now, or queues the cast behind a busy unit's orders when the command is queued.
    ///
    /// An effect-tree ability goes through the cast pipeline instead
    /// ([`Self::order_effect_cast`]); a non-queued single-caster one reaches only the unit
    /// [`Self::pick_single_caster`] picks, and the others keep their orders (plan
    /// `2026-11-10-spellcaster-mechanics` D-7.6).
    fn dispatch_cast(
        &mut self,
        cmd: &PlayerCommand,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) {
        let picked = if cmd.queue {
            None
        } else {
            self.pick_single_caster(cmd, ability_index, target_pos, target_entity)
        };
        for &ent_id in &cmd.entities {
            if self.is_effect_ability(ent_id, ability_index) {
                let single = self.is_single_caster_ability(ent_id, ability_index);
                if cmd.queue || !single || picked == Some(ent_id) {
                    self.order_effect_cast(cmd, ent_id, ability_index, target_pos, target_entity);
                }
                continue;
            }
            if let Some(ent) = self.entities.get_mut(&ent_id) {
                if ent.owner == cmd.player_id {
                    if cmd.queue && !ent.order_queue.is_empty() {
                        if let Err(source) = ent.issue_order(cmd.order.clone(), true) {
                            self.tick_diagnostics
                                .push(SimulationDiagnostic::MovementInvariant {
                                    tick: Tick(self.current_tick),
                                    source: MovementInvariant::Admission {
                                        entity: ent_id,
                                        source,
                                    },
                                });
                        }
                    } else if self.execute_ability_prevalidated(
                        ent_id,
                        ability_index,
                        target_pos,
                        target_entity,
                    ) && self.is_mode_toggle(ent_id, ability_index)
                    {
                        if let Some(ent) = self.entities.get_mut(&ent_id) {
                            ent.replace_orders_after_mode_switch();
                        }
                    }
                }
            }
        }
    }

    /// Gives `ent_id` the order to cast an effect-tree ability (plan
    /// `2026-11-10-spellcaster-mechanics` D-7): nothing happens unless the unit would accept
    /// the order; a queued command joins a busy unit's queue; otherwise the order replaces the
    /// unit's orders and the cast runs at once if the unit stands in range and is free to act.
    /// Else movement walks it into range and casts there (`cast_approach_goal`).
    fn order_effect_cast(
        &mut self,
        cmd: &PlayerCommand,
        ent_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) {
        let owned = self
            .entities
            .get(&ent_id)
            .is_some_and(|ent| ent.owner == cmd.player_id);
        if !owned || !self.would_accept_cast_order(ent_id, ability_index, target_pos, target_entity)
        {
            return;
        }
        let Some(ent) = self.entities.get_mut(&ent_id) else {
            return;
        };
        let queued = cmd.queue && !ent.order_queue.is_empty();
        if let Err(source) = ent.issue_order(cmd.order.clone(), queued) {
            self.tick_diagnostics
                .push(SimulationDiagnostic::MovementInvariant {
                    tick: Tick(self.current_tick),
                    source: MovementInvariant::Admission {
                        entity: ent_id,
                        source,
                    },
                });
            return;
        }
        if !queued
            && self.execute_ability_prevalidated(ent_id, ability_index, target_pos, target_entity)
        {
            if let Some(ent) = self.entities.get_mut(&ent_id) {
                ent.order_queue.pop_front();
            }
        }
    }

    /// Whether the ability at `ability_index` of `entity`'s archetype goes to one unit of a
    /// group command (`AbilitySpec::single_caster`).
    fn is_single_caster_ability(&self, entity: EntityId, ability_index: u8) -> bool {
        self.entities
            .get(&entity)
            .and_then(|ent| self.catalog.get(ent.unit_type_id).ok())
            .and_then(|arch| arch.abilities.get(usize::from(ability_index)))
            .is_some_and(|ability| ability.single_caster)
    }

    /// Whether the ability at `ability_index` of `entity`'s archetype toggles a mode
    /// (`AbilityTargetKind::ToggleMode`). An admitted non-queued cast of one replaces the
    /// unit's orders, because `StarCraft` II's morphs are not transient (plan
    /// `2026-11-09-client-mode-commands` D-6a); every other ability leaves them.
    fn is_mode_toggle(&self, entity: EntityId, ability_index: u8) -> bool {
        self.entities
            .get(&entity)
            .and_then(|ent| self.catalog.get(ent.unit_type_id).ok())
            .and_then(|arch| arch.abilities.get(usize::from(ability_index)))
            .is_some_and(|ability| matches!(ability.target_kind, AbilityTargetKind::ToggleMode))
    }

    /// Issues any other order to every commanded unit the player owns.
    ///
    /// A unit takes Stop or Hold Position only when its settled stance has the command
    /// ([`Entity::settled_stance_orders`]), as `StarCraft` II sends a command to every selected
    /// unit that has it and to no other (`ToSelection`, `core` AbilData:1015-1016, :553; plan
    /// `2026-11-23-stance-checked-orders` D-1). It takes an `AttackTarget` only when it can
    /// attack the target ([`Entity::can_attack`]: its settled stance has Attack, the target is
    /// another live unit, and the stance's weapon reaches the target's plane and attributes),
    /// whoever owns the target (`core` AbilData:117-129; plan `2026-11-24-attack-orders` D-3,
    /// B-174, B-175). A unit that does not take it is left exactly as it was; that is the
    /// game's admission rule, not a failure, so it raises no diagnostic, and a command no named
    /// unit takes is valid and does nothing. A unit whose archetype is missing from the catalog
    /// takes none of the three.
    ///
    /// A stop or hold-position order ends a move without arriving. A unit whose archetype brakes
    /// over several ticks keeps the velocity it had, and movement glides it to rest (B-39, the
    /// owner's answer to Q0067); any other unit stops at once.
    ///
    /// [`Entity::settled_stance_orders`]: crate::entity::Entity::settled_stance_orders
    /// [`Entity::can_attack`]: crate::entity::Entity::can_attack
    fn issue_plain_order(&mut self, cmd: &PlayerCommand) {
        let ends_motion =
            !cmd.queue && matches!(cmd.order, UnitOrder::Stop | UnitOrder::HoldPosition);
        for &ent_id in &cmd.entities {
            if !self.takes_plain_order(cmd, ent_id) {
                continue;
            }
            let Some(ent) = self.entities.get_mut(&ent_id) else {
                continue;
            };
            let arch = self.catalog.get(ent.unit_type_id).ok();
            let momentum = ent.velocity;
            let glides = ends_motion && arch.is_some_and(|arch| arch.deceleration < arch.max_speed);
            let issued = ent.issue_order(cmd.order.clone(), cmd.queue);
            if glides && issued.is_ok() {
                ent.velocity = momentum;
            }
            if let Err(source) = issued {
                self.tick_diagnostics
                    .push(SimulationDiagnostic::MovementInvariant {
                        tick: Tick(self.current_tick),
                        source: MovementInvariant::Admission {
                            entity: ent_id,
                            source,
                        },
                    });
            }
        }
    }

    /// Whether the unit `ent_id`, named in `cmd`, takes its plain order: it exists, the player
    /// owns it, and, for Stop, Hold Position and an attack on a unit, its archetype is in the
    /// catalog and its settled stance takes the order. An attack's target must exist and be
    /// alive; a target that is gone refuses the command for every recipient, with no
    /// diagnostic (plan `2026-11-24-attack-orders` D-3). Read-only: the recipient is borrowed
    /// mutably only after this answer.
    fn takes_plain_order(&self, cmd: &PlayerCommand, ent_id: EntityId) -> bool {
        self.takes_order(cmd.player_id, ent_id, &cmd.order)
    }

    /// The rule of [`Self::takes_plain_order`] for `player`'s unit `ent_id` and `order`, shared
    /// with the rally's attack order on an enemy (plan `2026-11-25-rallies` D-6), so the rally
    /// and the command admit an attack by one rule without building a [`PlayerCommand`].
    pub(crate) fn takes_order(
        &self,
        player: PlayerId,
        ent_id: EntityId,
        order: &UnitOrder,
    ) -> bool {
        let Some(ent) = self.entities.get(&ent_id).filter(|ent| ent.owner == player) else {
            return false;
        };
        let arch = self.catalog.get(ent.unit_type_id).ok();
        match *order {
            UnitOrder::Stop => arch.is_some_and(|arch| ent.settled_stance_orders(arch).stop),
            UnitOrder::HoldPosition => {
                arch.is_some_and(|arch| ent.settled_stance_orders(arch).hold_position)
            }
            UnitOrder::AttackTarget { target, .. } => {
                let Some(target) = self
                    .entities
                    .get(&target)
                    .filter(|target| target.is_alive())
                else {
                    return false;
                };
                let target_arch = self.catalog.get(target.unit_type_id).ok();
                arch.zip(target_arch)
                    .is_some_and(|(arch, target_arch)| ent.can_attack(arch, target, target_arch))
            }
            _ => true,
        }
    }
}
