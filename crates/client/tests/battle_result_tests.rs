//! The end of a match, driven through the real client plugin with real input events.
//!
//! # Responsibility
//! Boots the production client on the shipped skirmish, ends the match, and proves what D-12
//! promises: orders stop, sight does not, the player is told once, and the result window offers a
//! clean new match or a way out.
//!
//! # How it fits
//! Ends are staged by zeroing structures with `world_mut`, because these tests are about the
//! screen and not the path to it. From there the simulation's own evaluation decides the result,
//! and every gesture is a real Bevy event.
//!
//! # Limits
//! Shares the limits of `support/battle_harness.rs`: no window and no renderer. What egui drew is
//! known from the rectangles in [`HudGeometry`]. `battle_result_window_tests.rs` repeats the path
//! that matters on a real window.
//!
//! # Invariants
//! The pointer tests contain a control, so a test cannot pass merely because input was not being
//! delivered.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]

use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy_egui::egui;
use client::battle_hud::actions::HudIntent;
use client::battle_hud::card::{CardAction, CardPage, CommandCard};
use client::battle_hud::feed::BattleFeed;
use client::battle_hud::model::HudModel;
use client::battle_hud::placement::{PlacementGhost, PlacementPreview};
use client::battle_hud::queue::QueuePanel;
use client::battle_hud::result::{ResultScreen, ViewerResult};
use client::battle_hud::HudGeometry;
use client::battle_input::command_mode::CommandMode;
use client::battle_pipeline::commands::SubmissionError;
use client::battle_pipeline::session::BattleSession;
use client::input_bindings::{CONTROL_GROUP_KEYS, HOLD_POSITION_KEY, STOP_KEY};
use client::input_policy::{InputOwnership, UiGeometry};
use client::runtime_battle::{BattleDefinition, BattleInitialOrder, SKIRMISH_SCENARIO};
use client::{ClientMode, LabState};
use sim_types::{
    EntityId, MatchResult, PlayerId, SimScalar, Tick, UnitOrder, UnitTypeId, VisibilityState,
};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{sim_position, Harness, LOCAL_PLAYER};

const ENEMY: PlayerId = PlayerId(1);

/// Frames allowed for a staged end to reach the screen.
const END_FRAMES: usize = 200;

/// Boots the skirmish with the local workers in view.
fn skirmish() -> Harness {
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let worker = workers(&harness)[0];
    let position = harness.simulated_position(worker);
    harness.look_at(position);
    harness
}

/// The local player's mobile units, ordered by ID.
fn workers(harness: &Harness) -> Vec<EntityId> {
    harness
        .world()
        .entities
        .values()
        .filter(|unit| unit.owner == LOCAL_PLAYER && !unit.is_immobile)
        .map(|unit| unit.id)
        .collect()
}

/// Zeroes every structure of `owners`, so the next tick's evaluation finds them gone.
fn raze(harness: &mut Harness, owners: &[PlayerId]) {
    for unit in harness.world_mut().entities.values_mut() {
        if owners.contains(&unit.owner) && unit.is_immobile {
            unit.hp = SimScalar::ZERO;
        }
    }
}

fn screen(harness: &Harness) -> &ResultScreen {
    harness.app.world().resource::<ResultScreen>()
}

fn hud(harness: &Harness) -> &HudGeometry {
    harness.app.world().resource::<HudGeometry>()
}

fn model(harness: &Harness) -> &HudModel {
    harness.app.world().resource::<HudModel>()
}

fn session(harness: &Harness) -> &BattleSession {
    harness.app.world().resource::<BattleSession>()
}

fn point(rect: egui::Rect) -> Vec2 {
    Vec2::new(rect.center().x, rect.center().y)
}

/// Runs the battle until the verdict shows, plus the frames its window needs to settle, then
/// pauses. Leaves the result on screen with the input of a paused battle.
fn end_match(harness: &mut Harness, owners: &[PlayerId]) {
    raze(harness, owners);
    harness.resume();
    for _ in 0..END_FRAMES {
        harness.step(1);
        if screen(harness).verdict().is_some() {
            harness.pause();
            harness.step(2);
            return;
        }
    }
    panic!("the staged end must reach the screen");
}

fn feed_lines(harness: &Harness) -> Vec<String> {
    harness
        .app
        .world()
        .resource::<BattleFeed>()
        .lines()
        .map(|line| line.text.clone())
        .collect()
}

/// Queues a `Stop` for one unit the way a HUD control does.
fn queue_stop(harness: &mut Harness, unit: EntityId) {
    let mut session = harness.app.world_mut().resource_mut::<BattleSession>();
    let (world, queue) = session.world_and_commands();
    queue
        .submit_selection(
            world.expect("committed battle"),
            LOCAL_PLAYER,
            &[unit],
            UnitOrder::Stop,
            false,
        )
        .expect("the client must admit this order before the end");
}

fn mode(harness: &Harness) -> CommandMode {
    *harness.app.world().resource::<CommandMode>()
}

fn set_mode(harness: &mut Harness, mode: CommandMode) {
    *harness.app.world_mut().resource_mut::<CommandMode>() = mode;
}

/// A structure `unit` can raise, from catalog data.
fn raised_by(harness: &Harness, unit: EntityId) -> UnitTypeId {
    let unit_type = harness.world().entities[&unit].unit_type_id;
    *model(harness)
        .catalog
        .archetype(unit_type)
        .expect("every identity is listed")
        .raises
        .first()
        .expect("the skirmish starts with a builder")
}

/// Open ground beside `worker`, as a window position outside the result window, if one is shown.
fn ground_beside_window(harness: &mut Harness, worker: EntityId) -> Vec2 {
    let window = hud(harness).result;
    for distance in [3.0, 5.0, 7.0, 9.0] {
        let ground = harness.open_ground(harness.near(worker, Vec2::ZERO), distance);
        let on_screen = harness.ground_on_screen(ground);
        if window.is_none_or(|rect| !rect.contains(egui::pos2(on_screen.x, on_screen.y))) {
            return on_screen;
        }
    }
    panic!("the view must show ground beside the window");
}

/// Whether window position `at` lies on the window, on a button of it.
fn on_a_button(harness: &Harness, at: Vec2) -> bool {
    let geometry = hud(harness);
    [geometry.play_again, geometry.exit]
        .into_iter()
        .flatten()
        .any(|button| button.expand(4.0).contains(egui::pos2(at.x, at.y)))
}

/// Moves the camera until `units` all show in the world rectangle, clear of the result window.
fn view_beside_window(harness: &mut Harness, units: &[EntityId]) {
    let origin = harness.simulated_position(units[0]);
    for offset in [-6.0, 6.0, -9.0, 9.0, -12.0, 12.0] {
        harness.look_at(origin + Vec2::new(0.0, offset));
        let window = hud(harness).result.expect("the window must be up");
        let world = harness.app.world().resource::<UiGeometry>().world.unwrap();
        let clear = units.iter().all(|unit| {
            let at = harness.unit_on_screen(*unit);
            let at = egui::pos2(at.x, at.y);
            world.shrink(16.0).contains(at) && !window.expand(16.0).contains(at)
        });
        if clear {
            return;
        }
    }
    panic!("some view must show these units beside the window");
}

// --- Each ending ---------------------------------------------------------------------------------

/// Asserts that the window is up with both buttons, says `verdict`, and shows the final tick.
fn assert_shows(harness: &Harness, verdict: ViewerResult) {
    assert_eq!(screen(harness).verdict(), Some(verdict));
    let geometry = hud(harness);
    let window = geometry.result.expect("the window must be drawn");
    for button in [geometry.play_again, geometry.exit] {
        let button = button.expect("both buttons must be drawn");
        assert!(window.contains_rect(button), "{button:?} in {window:?}");
    }
    let (MatchResult::Victory { tick, .. } | MatchResult::Draw { tick }) =
        harness.world().match_state.result
    else {
        panic!("the match must be over");
    };
    let seconds = tick.0 / 25;
    assert_eq!(
        screen(harness).length_line(),
        format!("Match length {}:{:02}", seconds / 60, seconds % 60)
    );
}

#[test]
fn destroying_the_enemy_shows_victory() {
    let mut harness = skirmish();
    assert_eq!(screen(&harness).verdict(), None);
    assert_eq!(hud(&harness).result, None, "no window during a match");

    end_match(&mut harness, &[ENEMY]);

    assert_shows(&harness, ViewerResult::Victory);
}

#[test]
fn losing_the_last_structure_shows_defeat() {
    let mut harness = skirmish();
    end_match(&mut harness, &[LOCAL_PLAYER]);
    assert_shows(&harness, ViewerResult::Defeat);
}

#[test]
fn both_falling_on_one_tick_shows_a_draw() {
    let mut harness = skirmish();
    end_match(&mut harness, &[LOCAL_PLAYER, ENEMY]);
    assert_shows(&harness, ViewerResult::Draw);
}

#[test]
fn the_length_is_the_tick_the_match_ended_on() {
    let mut harness = skirmish();
    harness.resume();
    let goal = 60;
    let mut frames = 0;
    while harness.world().current_tick < goal {
        harness.step(1);
        frames += 1;
        assert!(frames < 1000, "the battle stopped ticking");
    }
    end_match(&mut harness, &[ENEMY]);
    assert_shows(&harness, ViewerResult::Victory);
    assert_eq!(screen(&harness).length_line(), "Match length 0:02");
}

#[test]
fn the_verdict_waits_for_the_displayed_tick() {
    let mut harness = skirmish();
    raze(&mut harness, &[ENEMY]);
    harness.resume();
    let mut ahead = 0;
    for _ in 0..END_FRAMES {
        harness.step(1);
        let shown = session(&harness).frames.presented().match_result;
        assert_eq!(
            screen(&harness).verdict().is_some(),
            shown.is_over(),
            "the screen follows the presented snapshot"
        );
        if shown.is_over() {
            break;
        }
        ahead += usize::from(harness.world().match_state.result.is_over());
    }
    assert!(
        screen(&harness).verdict().is_some(),
        "the staged end must reach the screen"
    );
    assert!(
        ahead > 0,
        "the world must end before the screen for this test to mean anything"
    );
}

#[test]
fn the_world_keeps_ticking_behind_the_window() {
    let mut harness = skirmish();
    end_match(&mut harness, &[ENEMY]);
    let tick = harness.world().current_tick;
    let line = screen(&harness).length_line().to_owned();

    harness.resume();
    harness.step(20);
    harness.pause();

    assert!(harness.world().current_tick > tick, "D-12: the world ticks");
    assert_eq!(screen(&harness).length_line(), line, "the length is final");
    assert_shows(&harness, ViewerResult::Victory);
}

#[test]
fn the_window_is_centred_in_the_world_rectangle() {
    let mut harness = skirmish();
    end_match(&mut harness, &[ENEMY]);
    let window = hud(&harness).result.unwrap();
    let world = harness.app.world().resource::<UiGeometry>().world.unwrap();
    assert!(world.contains_rect(window), "{window:?} in {world:?}");
    assert!(
        (window.center() - world.center()).length() < 1.0,
        "{window:?} in {world:?}"
    );
}

#[test]
fn the_verdict_text_does_not_allocate_on_a_frame_without_change() {
    let mut harness = skirmish();
    end_match(&mut harness, &[ENEMY]);
    let storage = screen(&harness).length_line().as_ptr();
    harness.resume();
    harness.step(100);
    assert_eq!(screen(&harness).length_line().as_ptr(), storage);
}

// --- Orders stop, sight does not (D-12) ----------------------------------------------------------

/// The plan's edge case: refused and reported, and the feed shows one line, not one per order.
#[test]
fn orders_pending_at_the_end_are_refused_and_reported_as_one_line() {
    let mut harness = skirmish();
    let crew = workers(&harness);
    for unit in &crew {
        queue_stop(&mut harness, *unit);
    }
    assert_eq!(harness.pending_orders(), crew.len());
    assert!(crew.len() > 1, "the case needs several pending orders");

    harness.world_mut().match_state.result = MatchResult::Draw { tick: Tick(0) };
    harness.resume();
    harness.step(4);
    harness.pause();

    assert_eq!(harness.pending_orders(), 0, "the refusal consumes them");
    assert_eq!(feed_lines(&harness), ["Order rejected: the match is over"]);
}

#[test]
fn no_order_is_accepted_after_the_end_but_selection_still_works() {
    let mut harness = skirmish();
    let crew = workers(&harness);
    harness.select(crew[0]);
    end_match(&mut harness, &[ENEMY]);
    view_beside_window(&mut harness, &crew[..2]);
    let status = harness.status();
    let lines = feed_lines(&harness);
    let ground = ground_beside_window(&mut harness, crew[0]);

    harness.click(MouseButton::Right, ground);
    harness.tap_key(HOLD_POSITION_KEY);
    harness.tap_key(STOP_KEY);
    assert_eq!(harness.pending_orders(), 0, "right click and keys are shut");
    assert_eq!(harness.status(), status, "and they report nothing");
    assert_eq!(feed_lines(&harness), lines);
    assert_eq!(harness.marker(), None, "no order, so no marker");

    let refusal = {
        let mut session = harness.app.world_mut().resource_mut::<BattleSession>();
        let (world, queue) = session.world_and_commands();
        queue.submit_selection(world.unwrap(), LOCAL_PLAYER, &crew, UnitOrder::Stop, false)
    };
    assert_eq!(refusal, Err(SubmissionError::MatchOver), "submit refuses");

    harness.select(crew[1]);
    assert_eq!(harness.selection().ids(), [crew[1]], "left click selects");
    let from = harness.unit_on_screen(crew[0]);
    harness.drag(from - Vec2::splat(12.0), from + Vec2::splat(12.0));
    assert!(
        harness.selection().ids().contains(&crew[0]),
        "a box selects too"
    );
}

/// The tick runs after `CollectIntent`, so the mode outlives the end by exactly one frame.
#[test]
fn an_armed_mode_ends_on_the_frame_after_the_match() {
    let mut harness = skirmish();
    let worker = workers(&harness)[0];
    harness.select(worker);
    let structure = raised_by(&harness, worker);
    set_mode(&mut harness, CommandMode::Place(structure));
    harness.step(1);
    assert_eq!(mode(&harness), CommandMode::Place(structure));

    raze(&mut harness, &[ENEMY]);
    harness.resume();
    let mut frames = 0;
    while !harness.world().match_state.result.is_over() {
        harness.step(1);
        frames += 1;
        assert!(frames < END_FRAMES, "the staged end must happen");
    }

    assert_eq!(
        mode(&harness),
        CommandMode::Place(structure),
        "input ran before the tick that ended the match"
    );
    harness.step(1);
    assert_eq!(mode(&harness), CommandMode::Normal);
}

#[test]
fn a_defeat_lifts_no_fog() {
    let mut harness = skirmish();
    let visible = |harness: &Harness| -> Vec<usize> {
        let cells = &model(harness).view.visibility;
        (0..cells.len())
            .filter(|cell| cells[*cell] == VisibilityState::Visible)
            .collect()
    };
    // Vision is computed by the first tick.
    harness.resume();
    while harness.world().current_tick == 0 {
        harness.step(1);
    }
    harness.pause();
    let before = visible(&harness);
    let nodes = model(&harness).view.visible_nodes.len();
    assert_ne!(before.len(), 0);

    end_match(&mut harness, &[LOCAL_PLAYER]);

    assert_eq!(model(&harness).view.viewer, LOCAL_PLAYER);
    let after = visible(&harness);
    assert!(
        after.iter().all(|cell| before.contains(cell)),
        "the end must not show a cell the viewer could not see"
    );
    assert!(after.len() < before.len(), "the lost base saw some of it");
    assert!(model(&harness).view.visible_nodes.len() <= nodes);
}

// --- The window keeps its clicks (D-4) -----------------------------------------------------------

#[test]
fn a_click_on_the_result_window_reaches_no_world_system() {
    let mut harness = skirmish();
    let crew = workers(&harness);
    harness.select(crew[0]);
    end_match(&mut harness, &[ENEMY]);
    let generation = session(&harness).commands.generation();

    // The middle of the window holds its buttons, so aim at a unit under the window's bare part.
    let window = hud(&harness).result.unwrap();
    let mut beneath = None;
    for unit in &crew[1..] {
        let at = harness.unit_on_screen(*unit);
        if window.contains(egui::pos2(at.x, at.y)) && !on_a_button(&harness, at) {
            beneath = Some((*unit, at));
        }
    }
    let (unit, at) = beneath.expect("the case needs a unit beneath the window");

    harness.move_cursor(at);
    assert!(
        !harness.app.world().resource::<InputOwnership>().pointer,
        "the world must not own the pointer over the window"
    );
    harness.click(MouseButton::Left, at);
    harness.step(2);
    assert_eq!(
        harness.selection().ids(),
        [crew[0]],
        "the unit beneath must not be selected"
    );
    assert_eq!(session(&harness).commands.generation(), generation);

    harness.click(MouseButton::Right, at);
    assert_eq!(harness.pending_orders(), 0);

    // Control: the same unit is selected by a click the window does not cover.
    view_beside_window(&mut harness, &[unit]);
    harness.select(unit);
    assert_eq!(harness.selection().ids(), [unit], "control");
}

fn card(harness: &Harness) -> &CommandCard {
    harness.app.world().resource::<CommandCard>()
}

fn queue_panel(harness: &Harness) -> &QueuePanel {
    harness.app.world().resource::<QueuePanel>()
}

fn preview(harness: &Harness) -> &PlacementPreview {
    harness.app.world().resource::<PlacementPreview>()
}

fn ghost_shown(harness: &mut Harness) -> bool {
    let world = harness.app.world_mut();
    *world
        .query_filtered::<&Visibility, With<PlacementGhost>>()
        .single(world)
        != Visibility::Hidden
}

/// Taps the key of the first card slot whose action `wanted` accepts.
fn tap_slot(harness: &mut Harness, wanted: impl Fn(&CardAction) -> bool) {
    let key = card(harness)
        .slots
        .iter()
        .flatten()
        .find(|slot| wanted(&slot.action))
        .unwrap_or_else(|| panic!("no such slot: {:?}", card(harness).slots))
        .key;
    harness.tap_key(key);
}

/// Queues one job at the local base through its card, and leaves the base selected.
fn train_at_base(harness: &mut Harness) -> EntityId {
    harness.world_mut().player_resources[0].minerals += 1_000;
    let base = harness.local_structure();
    let at = harness.simulated_position(base);
    harness.look_at(at);
    harness.select(base);
    assert_eq!(harness.selection().ids(), [base]);
    tap_slot(harness, |action| matches!(action, CardAction::Train(_)));
    harness.resume();
    harness.step(3);
    harness.pause();
    assert_eq!(
        queue_panel(harness).structure,
        Some(base),
        "a queue is listed"
    );
    base
}

/// Arms a placement through the card and shows its ghost at open ground beside `worker`.
fn show_ghost(harness: &mut Harness, worker: EntityId) {
    let at = harness.simulated_position(worker);
    harness.look_at(at);
    harness.select(worker);
    tap_slot(harness, |action| *action == CardAction::OpenBuildMenu);
    assert_eq!(card(harness).page, CardPage::Build { page: 0 });
    tap_slot(harness, |action| {
        matches!(action, CardAction::Mode(CommandMode::Place(_)))
    });
    assert!(matches!(mode(harness), CommandMode::Place(_)));
    let ground = harness.open_ground(harness.near(worker, Vec2::ZERO), 4.0);
    let ground = harness.ground_on_screen(ground);
    harness.move_cursor(ground);
    harness.step(1);
    assert!(preview(harness).cell.is_some());
    assert!(ghost_shown(harness), "the ghost is up before the end");
}

/// The card, the queue panel, the ghost and the intent of a closed match: all empty.
fn assert_no_orders_on_show(harness: &mut Harness) {
    let card = card(harness);
    assert!(card.slots.iter().all(Option::is_none), "{:?}", card.slots);
    assert!(card.fixed.iter().all(Option::is_none));
    assert_eq!(card.page, CardPage::HOME);
    let geometry = hud(harness);
    assert!(geometry.card.iter().all(Option::is_none));
    assert!(geometry.card_fixed.iter().all(Option::is_none));
    assert!(geometry.queue.iter().all(Option::is_none));
    assert_eq!(geometry.queue_cancel, None);
    assert_eq!(geometry.site_note, None);
    let panel = queue_panel(harness);
    assert_eq!(panel.structure, None);
    assert!(panel.rows.iter().all(Option::is_none));
    assert_eq!(panel.foundation, None);
    assert_eq!(panel.lines().len(), 0);
    assert_eq!(mode(harness), CommandMode::Normal);
    assert_eq!(preview(harness).cell, None);
    assert_eq!(preview(harness).verdict, None);
    assert!(!ghost_shown(harness));
    let intent = harness.app.world().resource::<HudIntent>();
    assert_eq!(intent.action, None);
    assert_eq!(intent.cancel_job, None);
}

#[test]
fn the_end_hides_the_card_queue_and_ghost_behind_the_result() {
    let mut harness = skirmish();
    let crew = workers(&harness);
    let base = train_at_base(&mut harness);
    show_ghost(&mut harness, crew[0]);

    end_match(&mut harness, &[ENEMY]);

    assert_eq!(screen(&harness).verdict(), Some(ViewerResult::Victory));
    assert_no_orders_on_show(&mut harness);

    // Looking around still works, and a structure with a queue offers nothing to do with it.
    view_beside_window(&mut harness, &[base]);
    harness.select(base);
    harness.step(1);
    assert_eq!(harness.selection().ids(), [base]);
    assert_no_orders_on_show(&mut harness);
}

// --- Play again ----------------------------------------------------------------------------------

fn refuse(harness: &mut Harness, error: &SubmissionError) {
    harness
        .app
        .world_mut()
        .resource_mut::<BattleSession>()
        .commands
        .reject(error);
    harness.step(3);
}

#[test]
fn play_again_starts_a_new_match_with_clean_state() {
    let mut harness = skirmish();
    let opening = model(&harness).resource_line.clone();
    let crew = workers(&harness);

    // Fill every per-session buffer before the end.
    harness.select(crew[0]);
    harness.with_key_held(KeyCode::ControlLeft, |harness| {
        harness.tap_key(CONTROL_GROUP_KEYS[0]);
    });
    assert_eq!(harness.selection().group(0), [crew[0]]);
    // The card's page, its queue panel, the selection facts and the ghost of WP6.
    let base = train_at_base(&mut harness);
    show_ghost(&mut harness, crew[0]);
    assert_eq!(model(&harness).selected_facts().len(), 1);
    harness.world_mut().player_resources[0].minerals += 77;
    refuse(&mut harness, &SubmissionError::NoDestination);
    end_match(&mut harness, &[ENEMY]);
    assert_eq!(feed_lines(&harness).len(), 1, "the refusal is shown");
    assert_eq!(model(&harness).selection_groups().len(), 1);
    assert_ne!(model(&harness).resource_line, opening);
    assert!(harness.world().current_tick > 0);
    let generation = session(&harness).commands.generation();

    harness.click(
        MouseButton::Left,
        point(hud(&harness).play_again.expect("the button is up")),
    );

    assert!(
        session(&harness).commands.generation() > generation,
        "the click must commit a new session"
    );
    assert_eq!(harness.world().match_state.result, MatchResult::InProgress);
    assert_eq!(harness.world().current_tick, 0);
    assert!(
        !harness
            .app
            .world()
            .resource::<LabState>()
            .battle_reset_requested
    );

    // The result is gone on the frame after the click, and stays gone.
    harness.step(1);
    let check = |harness: &Harness| {
        assert_eq!(screen(harness).verdict(), None);
        assert_eq!(screen(harness).length_line(), "");
        let geometry = hud(harness);
        assert_eq!(geometry.result, None);
        assert_eq!(geometry.play_again, None);
        assert_eq!(geometry.exit, None);

        assert_eq!(feed_lines(harness), Vec::<String>::new());

        assert_eq!(mode(harness), CommandMode::Normal);
        let selection = harness.selection();
        assert_eq!(selection.ids().len(), 0);
        assert_eq!(selection.inspected(), None);
        assert_eq!(selection.group(0).len(), 0);

        let model = model(harness);
        assert_eq!(model.selected_facts().len(), 0);
        assert!(model.orders_open, "the new match takes orders");
        assert_eq!(model.view.tick, Tick(0));
        assert_eq!(model.clock_line, "0:00");
        assert_eq!(model.resource_line, opening);
        assert_eq!(model.focus, None);
        assert_eq!(model.selection_groups().len(), 0);

        let session = session(harness);
        assert_eq!(session.commands.pending_len(), 0);
        assert_eq!(session.commands.status(), "");
        assert_eq!(session.commands.latest_refusal(), None);
        assert_eq!(session.rejected, []);
        assert_eq!(session.notices.len(), 0);
        assert_eq!(session.task_events.len(), 0);
    };
    check(&harness);
    assert_no_orders_on_show(&mut harness);
    harness.step(5);
    check(&harness);
    assert_no_orders_on_show(&mut harness);
    assert!(
        harness
            .world()
            .entities
            .get(&base)
            .is_none_or(|unit| unit.production_queue.is_empty()),
        "the new base has no inherited job"
    );

    // A refusal numbered like one read in the old session is still news.
    refuse(&mut harness, &SubmissionError::NoDestination);
    assert_eq!(
        feed_lines(&harness),
        ["Order rejected: there is no walkable ground there"]
    );

    // The new match takes orders.
    let worker = workers(&harness)[0];
    let position = harness.simulated_position(worker);
    harness.look_at(position);
    harness.select(worker);
    assert_eq!(harness.selection().ids(), [worker]);
    let ground = harness.open_ground(harness.near(worker, Vec2::ZERO), 3.0);
    assert!(harness
        .world()
        .map
        .world_to_cell(sim_position(ground))
        .is_ok());
    let ground = harness.ground_on_screen(ground);
    harness.click(MouseButton::Right, ground);
    assert_eq!(harness.pending_orders(), 1, "orders are open again");
    harness.step(1);
    assert!(
        card(&harness).slots.iter().any(Option::is_some),
        "the new match's card is back"
    );
    assert_eq!(card(&harness).page, CardPage::HOME);
}

/// The same reset can fail: the banner explains, and the window does not cover it.
#[test]
fn a_play_again_that_fails_shows_the_banner_instead_of_the_window() {
    let mut harness = skirmish();
    end_match(&mut harness, &[ENEMY]);
    let generation = session(&harness).commands.generation();
    let unusable_slot = 7;
    let mut definition = harness.app.world_mut().resource_mut::<BattleDefinition>();
    definition
        .scenario
        .initial_orders
        .push(BattleInitialOrder::AttackMoveToStart {
            player_slot: unusable_slot,
            target_player_slot: 0,
        });

    let button = point(hud(&harness).play_again.unwrap());
    click_while_failing(&mut harness, button);
    for _ in 0..3 {
        harness.app.update();
    }

    let failure = harness
        .app
        .world()
        .resource::<LabState>()
        .battle_failure
        .clone();
    assert!(
        failure.is_some(),
        "the broken definition must fail the reset"
    );
    assert_eq!(session(&harness).commands.generation(), generation);
    let geometry = hud(&harness);
    assert!(geometry.failure.is_some(), "the banner must be shown");
    assert!(geometry.reset.is_some(), "and it can still be retried");
    assert_eq!(geometry.result, None, "the window gives way");
    assert_eq!(geometry.play_again, None);
    assert_eq!(geometry.exit, None);
}

/// [`Harness::click`], without the check that fails a test on a battle failure.
fn click_while_failing(harness: &mut Harness, position: Vec2) {
    let window = harness.window();
    let world = harness.app.world_mut();
    world
        .query::<&mut Window>()
        .single_mut(world)
        .set_cursor_position(Some(position));
    world.send_event(CursorMoved {
        window,
        position,
        delta: None,
    });
    harness.app.update();
    for state in [ButtonState::Pressed, ButtonState::Released] {
        harness.app.world_mut().send_event(MouseButtonInput {
            button: MouseButton::Left,
            state,
            window,
        });
        harness.app.update();
    }
}

// --- Exit ----------------------------------------------------------------------------------------

#[test]
fn exit_ends_the_app_successfully() {
    let mut harness = skirmish();
    end_match(&mut harness, &[LOCAL_PLAYER]);
    assert_eq!(harness.app.should_exit(), None);

    harness.click(
        MouseButton::Left,
        point(hud(&harness).exit.expect("the button is up")),
    );

    assert_eq!(harness.app.should_exit(), Some(AppExit::Success));
}
