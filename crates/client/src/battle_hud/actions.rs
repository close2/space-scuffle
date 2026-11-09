//! Carries out what the player pressed on the command card or the queue panel.
//!
//! # Responsibility
//! Owns [`apply_card_action`], the one function a click and a key both end in, the hotkey reader
//! [`collect_card_keys`], and [`HudIntent`], which carries a click from drawing to here. Everything
//! reaches the simulation through the battle queue, and a refusal reaches the feed through
//! [`BattleCommandQueue::reject`](crate::battle_pipeline::commands::BattleCommandQueue::reject).
//!
//! # How it fits
//! ```text
//! CollectIntent  battle_hud_system records a click in HudIntent
//!                apply_hud_intent carries it out, before collect_pointer_intent
//!                collect_key_intent, then collect_card_keys, read keys in order
//! ```
//! `battle_input` knows nothing of the card, so the card's keys have their own reader, which runs
//! after [`collect_key_intent`](crate::battle_input::collect_key_intent). Stop and Hold stay with
//! that system, so they are not read twice.
//!
//! # Invariants
//! - Keys are read as ordered events, never from `ButtonInput`, so a chord survives a slow frame.
//!   The card is re-derived after every page change, so the next key of that frame meets the page
//!   this one opened.
//! - A disabled slot issues nothing, by mouse or by key. A key says why through the feed, and the
//!   mouse through the tooltip, and both use the same sentence.
//! - Which structure trains is decided here, as UI policy: the simulation would take the first.
//! - Nothing here allocates on a frame without a press.

use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use sim_types::{EntityId, PlayerCommand, PlayerId, UnitOrder};

use super::card::{derive_card, CardAction, CardPage, CommandCard};
use super::catalog::ArchetypeCommands;
use super::model::HudModel;
use crate::battle_input::command_mode::CommandMode;
use crate::battle_input::controls_open;
use crate::battle_pipeline::commands::{BattleCommandQueue, SubmissionError};
use crate::battle_pipeline::session::BattleSession;
use crate::battle_selection::BattleSelection;
use crate::input_bindings::{any_pressed, CANCEL_KEY, CONTROL_MODIFIERS, SHIFT_MODIFIERS};
use crate::input_policy::InputOwnership;
use crate::lab_ui::LabState;

/// What the player clicked on this frame's panels, for [`apply_hud_intent`].
#[derive(Resource, Debug, Default)]
pub struct HudIntent {
    /// A card entry, or a control that does the same.
    pub action: Option<CardAction>,
    /// A queue row: the structure and the job's position in its queue.
    pub cancel_job: Option<(EntityId, u32)>,
}

/// Everything a pressed entry acts on.
pub struct CardContext<'a> {
    /// The battle, for its world and its queue.
    pub session: &'a mut BattleSession,
    /// The selected units.
    pub selection: &'a [EntityId],
    /// The armed command mode.
    pub mode: &'a mut CommandMode,
    /// The card, for its page.
    pub card: &'a mut CommandCard,
    /// The local player.
    pub viewer: PlayerId,
    /// What the HUD knows.
    pub model: &'a HudModel,
}

impl CardContext<'_> {
    /// Moves the card to `page` and re-derives it at once, so a key later this frame meets it.
    fn turn_page(&mut self, page: CardPage) {
        self.card.page = page;
        derive_card(self.model, self.model.selected_facts(), self.card);
    }

    /// The selected finished structure `capable` accepts with the shortest queue, lowest ID first.
    ///
    /// Jobs still waiting for their tick count, so two presses in one frame spread out.
    fn shortest_queue(&self, capable: impl Fn(&ArchetypeCommands) -> bool) -> Option<EntityId> {
        self.model
            .selected_facts()
            .iter()
            .filter(|fact| !fact.foundation)
            .filter(|fact| {
                self.model
                    .catalog
                    .archetype(fact.unit_type)
                    .is_some_and(&capable)
            })
            .min_by_key(|fact| (self.jobs_of(fact.id), fact.id))
            .map(|fact| fact.id)
    }

    /// Jobs of `structure`, counting those the queue has not delivered yet.
    fn jobs_of(&self, structure: EntityId) -> usize {
        let view = &self.model.view;
        let running = view
            .queue_of(structure)
            .map_or(0, |queue| view.jobs_of(queue).len());
        let waiting = self
            .session
            .commands
            .pending_commands()
            .filter(|(_, command)| {
                matches!(
                    command.order,
                    UnitOrder::Produce { .. } | UnitOrder::Research { .. }
                ) && command.entities.contains(&structure)
            })
            .count();
        running + waiting
    }

    /// Submits `order` to `structure`, or reports why not.
    fn submit_to(&mut self, structure: Option<EntityId>, order: UnitOrder) {
        let viewer = self.viewer;
        let (world, queue) = self.session.world_and_commands();
        let result = world.ok_or(SubmissionError::NotReady).and_then(|world| {
            let structure = structure.ok_or(SubmissionError::EmptySelection)?;
            let command = PlayerCommand {
                player_id: viewer,
                entities: vec![structure],
                order,
                queue: false,
            };
            queue.submit(world, command)
        });
        report(result.map(|_| ()), queue);
    }
}

fn report(result: Result<(), SubmissionError>, queue: &mut BattleCommandQueue) {
    if let Err(error) = result {
        queue.reject(&error);
    }
}

/// Does what `action` says. `queued` is whether Shift is held, which only an order to units uses.
pub fn apply_card_action(action: &CardAction, queued: bool, ctx: &mut CardContext<'_>) {
    match action {
        CardAction::Order(order) => {
            let (viewer, selection) = (ctx.viewer, ctx.selection);
            let (world, queue) = ctx.session.world_and_commands();
            let result = world.ok_or(SubmissionError::NotReady).and_then(|world| {
                queue
                    .submit_selection(world, viewer, selection, order.clone(), queued)
                    .map(|_| ())
            });
            report(result, queue);
        }
        CardAction::Mode(mode) => *ctx.mode = *mode,
        CardAction::SwitchMode { mode, direction } => {
            let (viewer, selection, model) = (ctx.viewer, ctx.selection, ctx.model);
            let (world, queue) = ctx.session.world_and_commands();
            let result = world.ok_or(SubmissionError::NotReady).and_then(|world| {
                let labels = model
                    .catalog
                    .mode(*mode)
                    .ok_or(SubmissionError::EmptySelection)?;
                queue
                    .submit_mode_switch(
                        world,
                        viewer,
                        selection,
                        &labels.mode_name,
                        *direction,
                        queued,
                    )
                    .map(|_| ())
            });
            report(result, queue);
        }
        CardAction::Train(unit_type) => {
            let unit_type = *unit_type;
            let structure = ctx.shortest_queue(|structure| structure.trains.contains(&unit_type));
            ctx.submit_to(structure, UnitOrder::Produce { unit_type });
        }
        CardAction::Research(upgrade) => {
            let upgrade = *upgrade;
            let structure = ctx.shortest_queue(|structure| structure.researches.contains(&upgrade));
            ctx.submit_to(structure, UnitOrder::Research { upgrade });
        }
        CardAction::OpenBuildMenu => ctx.turn_page(CardPage::Build { page: 0 }),
        CardAction::NextPage => ctx.turn_page(ctx.card.page.next()),
        CardAction::Back => ctx.turn_page(ctx.card.page.previous()),
    }
}

/// Carries out what was clicked on this frame's panels.
#[expect(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "Bevy passes each system parameter by value"
)]
pub fn apply_hud_intent(
    mut intent: ResMut<'_, HudIntent>,
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    lab: Res<'_, LabState>,
    model: Res<'_, HudModel>,
    selection: Res<'_, BattleSelection>,
    mut session: ResMut<'_, BattleSession>,
    mut mode: ResMut<'_, CommandMode>,
    mut card: ResMut<'_, CommandCard>,
) {
    if intent.action.is_none() && intent.cancel_job.is_none() {
        return;
    }
    let (action, job) = (intent.action.take(), intent.cancel_job.take());
    let Some(viewer) = model.viewer.filter(|_| controls_open(&lab, &session)) else {
        return;
    };
    let mut ctx = CardContext {
        session: &mut session,
        selection: selection.ids(),
        mode: &mut mode,
        card: &mut card,
        viewer,
        model: &model,
    };
    if let Some(action) = action {
        apply_card_action(&action, any_pressed(&keyboard, &SHIFT_MODIFIERS), &mut ctx);
    }
    if let Some((structure, job_index)) = job {
        ctx.submit_to(Some(structure), UnitOrder::CancelProduction { job_index });
    }
}

/// Presses the slot of `key`, if the card shows one. A disabled slot says why instead.
fn press_key(key: KeyCode, queued: bool, ctx: &mut CardContext<'_>) {
    let Some(position) = ctx
        .card
        .slots
        .iter()
        .position(|slot| slot.as_ref().is_some_and(|slot| slot.key == key))
    else {
        return;
    };
    let Some(slot) = ctx.card.slots[position].as_ref() else {
        return;
    };
    if slot.enabled {
        let action = slot.action.clone();
        apply_card_action(&action, queued, ctx);
    } else {
        let reason = ctx.card.text[position].tooltip.clone();
        ctx.session
            .commands
            .reject(&SubmissionError::CardLocked(reason));
    }
}

/// Modifiers tracked through one frame's ordered events.
#[derive(Debug, Default)]
pub struct HeldModifiers {
    control: bool,
    shift: bool,
}

/// Reads the card's keys, and [`CANCEL_KEY`] as Back when it has no mode left to leave.
///
/// Runs after [`collect_key_intent`](crate::battle_input::collect_key_intent), which has already
/// cleared the mode on that key, so `mode_seen` remembers whether one was armed. Ctrl is ignored
/// here, so Ctrl with a letter does nothing.
#[expect(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "Bevy passes each system parameter by value"
)]
pub fn collect_card_keys(
    mut events: EventReader<'_, '_, KeyboardInput>,
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    mut held: Local<'_, HeldModifiers>,
    mut mode_seen: Local<'_, CommandMode>,
    ownership: Res<'_, InputOwnership>,
    lab: Res<'_, LabState>,
    selection: Res<'_, BattleSelection>,
    mut model: ResMut<'_, HudModel>,
    mut session: ResMut<'_, BattleSession>,
    mut mode: ResMut<'_, CommandMode>,
    mut card: ResMut<'_, CommandCard>,
) {
    let mut armed = *mode_seen != CommandMode::Normal;
    let owned = ownership.keyboard && controls_open(&lab, &session);
    let viewer = model.viewer.filter(|_| owned);
    if !events.is_empty() && viewer.is_some() {
        model.sync_selection(&selection, session.frames.presented().bubbles.as_slice());
        card.follow_selection(&model);
    }
    for event in events.read() {
        let pressed = event.state.is_pressed();
        if CONTROL_MODIFIERS.contains(&event.key_code) {
            held.control = pressed;
        } else if SHIFT_MODIFIERS.contains(&event.key_code) {
            held.shift = pressed;
        } else if let Some(viewer) = viewer.filter(|_| pressed && !event.repeat && !held.control) {
            let mut ctx = CardContext {
                session: &mut session,
                selection: selection.ids(),
                mode: &mut mode,
                card: &mut card,
                viewer,
                model: &model,
            };
            if event.key_code != CANCEL_KEY {
                press_key(event.key_code, held.shift, &mut ctx);
            } else if armed {
                armed = false;
            } else {
                apply_card_action(&CardAction::Back, false, &mut ctx);
            }
        }
    }
    // The held state is authoritative again once this frame's events are consumed, which also
    // recovers from a release that went to another window.
    held.control = any_pressed(&keyboard, &CONTROL_MODIFIERS);
    held.shift = any_pressed(&keyboard, &SHIFT_MODIFIERS);
    *mode_seen = *mode;
}
