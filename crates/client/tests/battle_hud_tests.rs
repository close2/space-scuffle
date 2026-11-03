//! Drives the player's HUD through the real client plugin with real input events.
//!
//! # Responsibility
//! Boots the production client in [`ClientMode::Skirmish`] on the shipped skirmish, and proves what
//! the HUD promises: the inspector is gone, the panels reserve space that the world cannot be
//! clicked through, the feed reports refusals to their player only, and refreshing costs nothing
//! on a frame without change.
//!
//! # Limits
//! There is no window and no renderer here, so what egui painted is known only from the rectangles
//! recorded in [`HudGeometry`]. Clicks are real Bevy events, and egui resolves them.
//!
//! # Invariants
//! The pointer tests each contain a control: the same gesture over open ground does reach the
//! world, so a test cannot pass merely because input was not being delivered.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use client::battle_hud::catalog::CommandCatalog;
use client::battle_hud::feed::{BattleFeed, FEED_LINES};
use client::battle_hud::model::HudModel;
use client::battle_hud::panels::{
    COMMAND_PANEL_HEIGHT, MAX_SIMULATION_SPEED, MIN_SIMULATION_SPEED, RESOURCE_BAR_HEIGHT,
};
use client::battle_hud::HudGeometry;
use client::battle_pipeline::commands::{SessionGeneration, SubmissionError};
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::input_policy::{InputOwnership, UiGeometry};
use client::media::{ClientMediaError, ClientMediaStatus};
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::{window_title, ClientMode, FlipbookRegistry, LabState, RtsCamera};
use sim_core::UnitArchetype;
use sim_map::ResourceKind;
use sim_types::{
    CommandSubject, EntityId, MatchResult, PlayerId, ResourceNodeId, ShapeKind, SimScalar, Tick,
    UnitOrder, UnitTypeId,
};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{Harness, LOCAL_PLAYER};

const ENEMY: PlayerId = PlayerId(1);

/// Boots the skirmish with the local workers in view.
fn skirmish() -> Harness {
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let worker = workers(&harness, LOCAL_PLAYER)[0];
    let position = harness.simulated_position(worker);
    harness.look_at(position);
    harness
}

/// `owner`'s mobile units, ordered by ID.
fn workers(harness: &Harness, owner: PlayerId) -> Vec<EntityId> {
    harness
        .world()
        .entities
        .values()
        .filter(|unit| unit.owner == owner && !unit.is_immobile)
        .map(|unit| unit.id)
        .collect()
}

fn structure_of(harness: &Harness, owner: PlayerId) -> EntityId {
    harness
        .world()
        .entities
        .values()
        .find(|unit| unit.owner == owner && unit.is_immobile)
        .expect("the scenario gives each player a base")
        .id
}

fn hud(harness: &Harness) -> &HudGeometry {
    harness.app.world().resource::<HudGeometry>()
}

fn model(harness: &Harness) -> &HudModel {
    harness.app.world().resource::<HudModel>()
}

fn point(rect: egui::Rect) -> Vec2 {
    Vec2::new(rect.center().x, rect.center().y)
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

/// Queues `order` for `recipients` the way a HUD control does.
fn submit(harness: &mut Harness, player: PlayerId, recipients: &[EntityId], order: UnitOrder) {
    let mut session = harness.app.world_mut().resource_mut::<BattleSession>();
    let (world, queue) = session.world_and_commands();
    queue
        .submit_selection(
            world.expect("committed battle"),
            player,
            recipients,
            order,
            false,
        )
        .expect("the client must admit this order");
}

/// The first unit `player`'s view says can be trained.
fn trainable(harness: &Harness) -> UnitTypeId {
    model(harness)
        .view
        .options
        .iter()
        .find_map(|option| match option.subject {
            CommandSubject::Train(unit_type) if option.lock.is_none() => Some(unit_type),
            _ => None,
        })
        .expect("a base must be able to train something")
}

fn run_ticks(harness: &mut Harness, ticks: u32) {
    let goal = harness.world().current_tick + ticks;
    harness.resume();
    let mut frames = 0;
    while harness.world().current_tick < goal {
        harness.step(1);
        frames += 1;
        assert!(frames < 10_000, "the battle stopped ticking");
    }
    harness.pause();
}

// --- The inspector is gone (G-17, D-1) -----------------------------------------------------------

#[test]
fn the_player_profile_shows_no_inspector_and_no_other_players_data() {
    let mut harness = skirmish();
    harness.world_mut().player_resources[0].minerals = 1234;
    harness.world_mut().player_resources[1].minerals = 5678;
    run_ticks(&mut harness, 1);

    let geometry = harness.app.world().resource::<UiGeometry>();
    assert_eq!(geometry.inspector, None);
    assert!(geometry.targets.iter().all(Option::is_none));

    let model = model(&harness);
    assert_eq!(model.viewer, Some(LOCAL_PLAYER));
    assert_eq!(model.view.viewer, LOCAL_PLAYER);
    assert_eq!(model.view.status.minerals, 1234);
    assert!(model.resource_line.contains("1234"));
    assert!(!model.resource_line.contains("5678"));
}

#[test]
fn the_sandbox_keeps_its_inspector_and_has_no_hud() {
    let harness = Harness::battle();
    assert!(harness
        .app
        .world()
        .resource::<UiGeometry>()
        .inspector
        .is_some());
    let geometry = hud(&harness);
    assert_eq!(geometry.top, None);
    assert_eq!(geometry.bottom, None);
    assert_eq!(model(&harness).viewer, None);
}

#[test]
fn only_the_skirmish_window_drops_the_lab_title() {
    assert_eq!(window_title(ClientMode::Skirmish), "Space Scuffle");
    for mode in [
        ClientMode::VisualLab,
        ClientMode::LiveBattle,
        ClientMode::SwarmStressTest,
    ] {
        assert!(window_title(mode).contains("Visual Lab"));
    }
}

// --- The world rectangle is the hit test (D-4) ---------------------------------------------------

#[test]
fn the_world_viewport_excludes_both_panels() {
    let mut harness = skirmish();
    let (top, bottom) = {
        let geometry = hud(&harness);
        (geometry.top.unwrap(), geometry.bottom.unwrap())
    };
    assert_eq!(top.top(), 0.0);
    assert_eq!(top.height(), RESOURCE_BAR_HEIGHT);
    assert_eq!(bottom.bottom(), 720.0);
    assert_eq!(bottom.height(), COMMAND_PANEL_HEIGHT);

    let world = harness.app.world_mut();
    let camera = world
        .query_filtered::<&Camera, With<RtsCamera>>()
        .single(world);
    let viewport = camera.viewport.as_ref().expect("a world viewport");
    assert_eq!(viewport.physical_position.x, 0);
    assert_eq!(viewport.physical_size.x, 1280);
    assert_eq!(viewport.physical_position.y as f32, top.bottom().ceil());
    assert_eq!(
        (viewport.physical_position.y + viewport.physical_size.y) as f32,
        bottom.top().floor()
    );
}

#[test]
fn the_command_panel_lays_out_three_columns() {
    let harness = skirmish();
    let geometry = hud(&harness);
    let bottom = geometry.bottom.unwrap();
    let selection = geometry.selection_column.unwrap();
    let queue = geometry.queue_column.unwrap();
    let card = geometry.card_column.unwrap();
    assert!(selection.right() <= queue.left());
    assert!(queue.right() <= card.left());
    for column in [selection, queue, card] {
        assert!(
            bottom.contains_rect(column),
            "{column:?} is outside {bottom:?}"
        );
        assert!(column.width() > 0.0);
    }
}

#[test]
fn a_click_on_the_bottom_panel_moves_nothing() {
    let mut harness = skirmish();
    let worker = workers(&harness, LOCAL_PLAYER)[0];
    harness.select(worker);
    let panel = point(hud(&harness).bottom.unwrap());

    harness.click(MouseButton::Left, panel);
    harness.click(MouseButton::Right, panel);
    assert_eq!(
        harness.selection().ids(),
        [worker],
        "left click selects nothing"
    );
    assert_eq!(harness.pending_orders(), 0, "right click orders nothing");

    // Control: the same click on open ground does order the worker.
    let ground = harness.open_ground(harness.near(worker, Vec2::ZERO), 3.0);
    let ground = harness.ground_on_screen(ground);
    harness.click(MouseButton::Right, ground);
    assert_eq!(
        harness.pending_orders(),
        1,
        "the click must reach the world"
    );
}

#[test]
fn a_right_click_on_the_top_panel_moves_nothing() {
    let mut harness = skirmish();
    let worker = workers(&harness, LOCAL_PLAYER)[0];
    harness.select(worker);
    let bar = hud(&harness).top.unwrap();

    harness.click(MouseButton::Right, point(bar));
    harness.click(MouseButton::Left, point(bar));
    assert_eq!(harness.pending_orders(), 0);
    assert_eq!(harness.selection().ids(), [worker]);
}

#[test]
fn a_drag_from_the_world_ends_over_the_panel_and_still_selects() {
    let mut harness = skirmish();
    let worker = workers(&harness, LOCAL_PLAYER)[0];
    let position = harness.unit_on_screen(worker);
    let panel = hud(&harness).bottom.unwrap();

    harness.drag(
        position - Vec2::new(30.0, 30.0),
        Vec2::new(position.x + 30.0, panel.center().y),
    );

    assert!(
        harness.selection().ids().contains(&worker),
        "a box released over a panel must still select"
    );
}

/// Where a test's floating surface was drawn on the latest frame.
#[derive(Resource, Default)]
struct FloatingSurface {
    rect: Option<egui::Rect>,
}

/// Top-left corner of a floating surface: clear of the units, inside the world rectangle.
const FLOATING_CORNER: egui::Pos2 = egui::pos2(950.0, 60.0);

/// Size of a floating surface's content, in egui points.
const FLOATING_SIZE: egui::Vec2 = egui::vec2(280.0, 200.0);

/// A stand-in for the result window of a later package.
fn draw_window(mut contexts: EguiContexts<'_, '_>, mut surface: ResMut<'_, FloatingSurface>) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let response = egui::Window::new("floating")
        .fixed_pos(FLOATING_CORNER)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.allocate_space(FLOATING_SIZE);
        });
    surface.rect = response.map(|shown| shown.response.rect);
}

/// A solid surface that asks for no clicks, so no widget of it claims the pointer on a press.
fn draw_hover_area(mut contexts: EguiContexts<'_, '_>, mut surface: ResMut<'_, FloatingSurface>) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let response = egui::Area::new(egui::Id::new("floating"))
        .fixed_pos(FLOATING_CORNER)
        .sense(egui::Sense::hover())
        .show(ctx, |ui| {
            ui.allocate_space(FLOATING_SIZE);
        });
    surface.rect = Some(response.response.rect);
}

fn assert_surface_keeps_clicks<Marker>(draw: impl IntoSystemConfigs<Marker>) {
    let mut harness = skirmish();
    let worker = workers(&harness, LOCAL_PLAYER)[0];
    harness.select(worker);
    harness.app.init_resource::<FloatingSurface>().add_systems(
        Update,
        draw.before(client::lab_ui_system)
            .in_set(BattleSet::CollectIntent),
    );
    harness.step(4);
    let surface = harness
        .app
        .world()
        .resource::<FloatingSurface>()
        .rect
        .expect("the surface must be drawn");
    let world = harness.app.world().resource::<UiGeometry>().world.unwrap();
    assert!(
        world.contains_rect(surface),
        "a floating surface reserves nothing, so the world rectangle cannot protect it"
    );

    harness.click(MouseButton::Right, point(surface));
    assert_eq!(
        harness.pending_orders(),
        0,
        "the surface took the right click"
    );
    assert_eq!(
        harness.status(),
        "",
        "the world must not have tried to order anything"
    );
    harness.click(MouseButton::Left, point(surface));
    assert_eq!(
        harness.selection().ids(),
        [worker],
        "the surface took the left click"
    );

    // Control: the same click beside the surface does reach the world.
    let ground = harness.open_ground(harness.near(worker, Vec2::ZERO), 3.0);
    let ground = harness.ground_on_screen(ground);
    assert!(!surface.contains(egui::pos2(ground.x, ground.y)));
    harness.click(MouseButton::Right, ground);
    assert_eq!(
        harness.pending_orders(),
        1,
        "the click must reach the world"
    );
}

#[test]
fn a_floating_window_keeps_the_click_on_the_frame_of_the_press() {
    assert_surface_keeps_clicks(draw_window);
}

#[test]
fn a_floating_surface_that_senses_no_click_still_keeps_it_from_the_world() {
    assert_surface_keeps_clicks(draw_hover_area);
}

// --- The feed passes clicks through (D-4) --------------------------------------------------------

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
fn the_feed_does_not_block_a_click_beneath_it() {
    let mut harness = skirmish();
    let worker = workers(&harness, LOCAL_PLAYER)[0];
    harness.select(worker);
    // Halfway between the bases there is map beneath the feed, wherever it stands on screen.
    let bases = [LOCAL_PLAYER, ENEMY].map(|owner| structure_of(&harness, owner));
    let middle = bases.map(|base| harness.simulated_position(base));
    harness.look_at((middle[0] + middle[1]) * 0.5);
    refuse(&mut harness, &SubmissionError::EmptySelection);
    let feed = hud(&harness).feed.expect("a line must be on screen");
    assert!(feed.is_positive(), "{feed:?}");
    let world = harness.app.world().resource::<UiGeometry>().world.unwrap();
    assert!(world.contains_rect(feed), "the feed floats over the world");

    harness.move_cursor(point(feed));
    assert!(
        harness.app.world().resource::<InputOwnership>().pointer,
        "the world keeps the pointer under the feed"
    );
    harness.click(MouseButton::Right, point(feed));
    assert_eq!(
        harness.pending_orders(),
        1,
        "the order lands beneath the feed: {}",
        harness.status()
    );

    harness.click(MouseButton::Left, point(feed));
    assert_eq!(
        harness.selection().ids().len(),
        0,
        "a click on the ground beneath the feed clears the selection"
    );
}

// --- Resources and selection ---------------------------------------------------------------------

#[test]
fn resources_show_the_bank_and_change_with_income() {
    let mut harness = skirmish();
    let status = model(&harness).view.status;
    assert_eq!(
        status.minerals,
        harness.world().player_resources[0].minerals
    );
    assert_eq!(
        model(&harness).resource_line,
        format!(
            "Minerals {}    Gas {}    Supply {} / {}",
            status.minerals, status.gas, status.supply_used, status.supply_max
        )
    );

    let crew = workers(&harness, LOCAL_PLAYER);
    let origin = harness.world().entities[&crew[0]].position;
    let world = harness.world();
    let (worker_x, worker_y) = world.map.world_to_cell(origin).expect("worker on the map");
    let node = world
        .map
        .resources()
        .iter()
        .zip(0_u32..)
        .filter(|(node, _)| node.kind == ResourceKind::Mineral)
        .min_by_key(|(node, _)| node.cell.0.abs_diff(worker_x) + node.cell.1.abs_diff(worker_y))
        .expect("the map has minerals")
        .1;
    submit(
        &mut harness,
        LOCAL_PLAYER,
        &crew,
        UnitOrder::Harvest {
            target: ResourceNodeId(node),
        },
    );
    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .simulation_speed = MAX_SIMULATION_SPEED;
    harness.resume();
    let before = model(&harness).resource_line.clone();
    let mut frames = 0;
    while model(&harness).resource_line == before {
        harness.step(1);
        frames += 1;
        assert!(frames < 3000, "harvesting must earn income");
    }
    let earned = model(&harness).view.status.minerals;
    assert!(earned > status.minerals);
    assert!(model(&harness).resource_line.contains(&earned.to_string()));
}

#[test]
fn the_clock_follows_the_simulation() {
    let mut harness = skirmish();
    assert_eq!(model(&harness).clock_line, "0:00");
    run_ticks(&mut harness, 75);
    let seconds = harness.world().current_tick / 25;
    assert_eq!(model(&harness).clock_line, format!("0:{seconds:02}"));
    assert!(seconds >= 3);
}

#[test]
fn the_selection_panel_summarizes_by_type_even_while_paused() {
    let mut harness = skirmish();
    let crew = workers(&harness, LOCAL_PLAYER);
    let name = {
        let world = harness.world();
        let unit_type = world.entities[&crew[0]].unit_type_id;
        world.catalog.get(unit_type).unwrap().display_name.clone()
    };
    assert_eq!(model(&harness).focus, None);

    harness.select(crew[0]);
    let focus = model(&harness).focus.expect("one unit shows its bars");
    assert_eq!(focus.health, 1.0);
    assert_eq!(model(&harness).selection_groups().len(), 1);
    assert_eq!(model(&harness).selection_groups()[0].count, 1);

    let base = structure_of(&harness, LOCAL_PLAYER);
    let base_on_screen = harness.unit_on_screen(base);
    harness.with_key_held(KeyCode::ShiftLeft, |harness| {
        harness.click(MouseButton::Left, base_on_screen);
    });
    assert_eq!(model(&harness).focus, None, "two units show as a list");
    let groups = model(&harness).selection_groups();
    assert_eq!(groups.len(), 2);
    assert!(groups[0].unit_type < groups[1].unit_type);
    assert!(
        groups
            .iter()
            .any(|group| group.label == format!("1 × {name}")),
        "{groups:?}"
    );
}

/// A shipped identity with a shield, so these tests hold for whatever data ships.
fn shielded_identity(harness: &Harness) -> UnitTypeId {
    let world = harness.app.world();
    let status = world.resource::<ClientMediaStatus>();
    let registry = world.resource::<FlipbookRegistry>();
    harness
        .world()
        .catalog
        .archetypes()
        .find(|arch: &&UnitArchetype| {
            arch.max_shields > SimScalar::ZERO
                && !matches!(arch.shape, ShapeKind::BuildingDome { .. })
                && status.presentable(registry, arch.unit_type_id).is_ok()
        })
        .expect("the catalog must have a presentable shielded unit")
        .unit_type_id
}

#[test]
fn an_inspected_enemy_shows_name_and_health_only() {
    let mut harness = skirmish();
    let worker = workers(&harness, LOCAL_PLAYER)[0];
    let identity = shielded_identity(&harness);
    let place = harness.open_ground(harness.simulated_position(worker), 4.0);
    let at = battle_harness::sim_position(place);
    let own = harness
        .world_mut()
        .spawn_unit(identity, LOCAL_PLAYER, at)
        .unwrap();
    let foe = {
        let beside = battle_harness::sim_position(harness.open_ground(place, 3.0));
        harness
            .world_mut()
            .spawn_unit(identity, ENEMY, beside)
            .unwrap()
    };
    run_ticks(&mut harness, 2);

    harness.select(own);
    let focus = model(&harness).focus.expect("one unit shows its bars");
    assert!(focus.shield.is_some(), "the player sees their own shield");

    harness.select(foe);
    assert_eq!(harness.selection().inspected(), Some(foe));
    let focus = model(&harness)
        .focus
        .expect("an inspected unit shows its bar");
    assert_eq!(focus.unit_type, identity);
    assert_eq!(focus.shield, None, "another player's shield stays private");
}

// --- The bar's controls --------------------------------------------------------------------------

#[test]
fn the_bar_pauses_and_changes_speed_within_its_bounds() {
    let mut harness = skirmish();
    let speed = |harness: &Harness| harness.app.world().resource::<LabState>().simulation_speed;
    let paused = |harness: &Harness| harness.app.world().resource::<LabState>().battle_paused;
    assert!(paused(&harness));

    harness.click(MouseButton::Left, point(hud(&harness).pause.unwrap()));
    harness.step(2);
    assert!(!paused(&harness), "the click must reach the button");

    let up = point(hud(&harness).speed_up.unwrap());
    let steps = MAX_SIMULATION_SPEED - MIN_SIMULATION_SPEED + 1;
    for _ in 0..steps {
        harness.click(MouseButton::Left, up);
        harness.step(2);
    }
    assert_eq!(speed(&harness), MAX_SIMULATION_SPEED);

    let down = point(hud(&harness).speed_down.unwrap());
    for _ in 0..steps {
        harness.click(MouseButton::Left, down);
        harness.step(2);
    }
    assert_eq!(speed(&harness), MIN_SIMULATION_SPEED);
}

// --- The feed ------------------------------------------------------------------------------------

#[test]
fn a_refused_order_appears_in_the_feed() {
    let mut harness = skirmish();
    let base = structure_of(&harness, LOCAL_PLAYER);
    let unit_type = trainable(&harness);
    harness.world_mut().player_resources[0].minerals = 0;
    let needed = harness.world().catalog.get(unit_type).unwrap().mineral_cost;

    submit(
        &mut harness,
        LOCAL_PLAYER,
        &[base],
        UnitOrder::Produce { unit_type },
    );
    run_ticks(&mut harness, 2);

    assert_eq!(
        feed_lines(&harness),
        [format!(
            "Production order rejected: not enough minerals (need {needed}, have 0)"
        )]
    );
}

#[test]
fn an_accepted_order_is_not_reported() {
    let mut harness = skirmish();
    let crew = workers(&harness, LOCAL_PLAYER);
    let target =
        battle_harness::sim_position(harness.open_ground(harness.near(crew[0], Vec2::ZERO), 3.0));
    submit(
        &mut harness,
        LOCAL_PLAYER,
        &crew,
        UnitOrder::Move { target },
    );
    assert_eq!(harness.pending_orders(), 1);
    run_ticks(&mut harness, 2);
    assert_eq!(feed_lines(&harness), Vec::<String>::new());
}

#[test]
fn a_refused_order_of_another_player_stays_out_of_the_feed() {
    let mut harness = skirmish();
    let base = structure_of(&harness, ENEMY);
    let unit_type = trainable(&harness);
    harness.world_mut().player_resources[1].minerals = 0;

    submit(
        &mut harness,
        ENEMY,
        &[base],
        UnitOrder::Produce { unit_type },
    );
    run_ticks(&mut harness, 2);

    assert!(
        !harness
            .app
            .world()
            .resource::<BattleSession>()
            .outcome_status
            .is_empty(),
        "the simulation must have answered"
    );
    assert_eq!(feed_lines(&harness), Vec::<String>::new());
}

/// Gives a unit a task naming a node that does not exist. The simulation reports how that task
/// ended, through the same event path as any other. Only direct order injection can do this,
/// because admission refuses such a node.
fn give_a_doomed_task(harness: &mut Harness, unit: EntityId) {
    harness
        .world_mut()
        .entities
        .get_mut(&unit)
        .unwrap()
        .order_queue
        .push_front(UnitOrder::Harvest {
            target: ResourceNodeId(u32::MAX),
        });
}

#[test]
fn a_task_event_of_another_player_stays_out_of_the_feed() {
    let mut harness = skirmish();
    let theirs = workers(&harness, ENEMY)[0];
    give_a_doomed_task(&mut harness, theirs);
    run_ticks(&mut harness, 2);
    assert_eq!(feed_lines(&harness), Vec::<String>::new());

    // Control: the same event of the viewer's own unit is reported.
    let ours = workers(&harness, LOCAL_PLAYER)[0];
    give_a_doomed_task(&mut harness, ours);
    run_ticks(&mut harness, 2);
    assert_eq!(feed_lines(&harness), ["That resource node is unavailable"]);
}

#[test]
fn an_order_pending_when_the_match_ends_appears_once() {
    let mut harness = skirmish();
    let crew = workers(&harness, LOCAL_PLAYER);
    let target =
        battle_harness::sim_position(harness.open_ground(harness.near(crew[0], Vec2::ZERO), 3.0));
    submit(
        &mut harness,
        LOCAL_PLAYER,
        &crew,
        UnitOrder::Move { target },
    );
    harness.world_mut().match_state.result = MatchResult::Draw { tick: Tick(0) };

    run_ticks(&mut harness, 3);
    run_ticks(&mut harness, 3);

    assert_eq!(feed_lines(&harness), ["Order rejected: the match is over"]);
}

#[test]
fn an_immediate_refusal_appears_once_even_while_paused() {
    let mut harness = skirmish();
    refuse(&mut harness, &SubmissionError::NoDestination);
    harness.step(10);
    assert_eq!(
        feed_lines(&harness),
        ["Order rejected: there is no walkable ground there"]
    );
    refuse(&mut harness, &SubmissionError::NoDestination);
    assert_eq!(feed_lines(&harness).len(), 2, "a second refusal is news");
}

#[test]
fn the_routine_status_of_a_queue_is_not_news() {
    let mut harness = skirmish();
    let worker = workers(&harness, LOCAL_PLAYER)[0];
    harness.select(worker);
    let ground = harness.open_ground(harness.near(worker, Vec2::ZERO), 3.0);
    let ground = harness.ground_on_screen(ground);
    harness.click(MouseButton::Right, ground);
    assert!(harness.status().contains("queued"));
    assert_eq!(feed_lines(&harness), Vec::<String>::new());
}

#[test]
fn the_feed_keeps_only_its_newest_lines_and_lets_them_expire() {
    let mut harness = skirmish();
    for _ in 0..=FEED_LINES {
        refuse(&mut harness, &SubmissionError::NoDestination);
    }
    assert_eq!(feed_lines(&harness).len(), FEED_LINES);
    // Eight seconds of 40 ms frames, and a margin.
    harness.step(220);
    assert_eq!(feed_lines(&harness).len(), 0);
    assert_eq!(hud(&harness).feed, None, "an empty feed draws nothing");
}

// --- No allocation on a frame without change (charter 3.3) ---------------------------------------

/// Address and capacity of every retained buffer, which an allocation would move or grow.
fn buffers(model: &HudModel) -> Vec<(usize, usize)> {
    let view = &model.view;
    let groups = model.selection_groups();
    vec![
        (
            model.resource_line.as_ptr() as usize,
            model.resource_line.capacity(),
        ),
        (
            model.clock_line.as_ptr() as usize,
            model.clock_line.capacity(),
        ),
        (
            view.visibility.as_ptr() as usize,
            view.visibility.capacity(),
        ),
        (view.options.as_ptr() as usize, view.options.capacity()),
        (
            view.completed_types.as_ptr() as usize,
            view.completed_types.capacity(),
        ),
        (
            view.visible_nodes.as_ptr() as usize,
            view.visible_nodes.capacity(),
        ),
        (groups.as_ptr() as usize, groups.len()),
        (
            groups[0].label.as_ptr() as usize,
            groups[0].label.capacity(),
        ),
    ]
}

#[test]
fn the_model_allocates_nothing_on_a_frame_without_change() {
    let mut harness = skirmish();
    harness.select(workers(&harness, LOCAL_PLAYER)[0]);
    let warmed = buffers(model(&harness));
    let refills = model(&harness).refills();
    harness.step(200);
    assert_eq!(buffers(model(&harness)), warmed);
    assert_eq!(
        model(&harness).refills(),
        refills,
        "nothing changed, so nothing may be rewritten"
    );
}

#[test]
fn the_model_reuses_its_buffers_on_every_tick() {
    let mut harness = skirmish();
    harness.select(workers(&harness, LOCAL_PLAYER)[0]);
    run_ticks(&mut harness, 30);
    let warmed = buffers(model(&harness));
    let refills = model(&harness).refills();
    run_ticks(&mut harness, 200);
    assert_eq!(buffers(model(&harness)), warmed);
    assert!(
        model(&harness).refills() > refills,
        "each tick must have refreshed the model"
    );
}

#[test]
fn a_changed_selection_refreshes_the_model_without_a_tick() {
    let mut harness = skirmish();
    let refills = model(&harness).refills();
    let worker = workers(&harness, LOCAL_PLAYER)[0];
    harness.select(worker);
    assert_eq!(harness.world().current_tick, 0, "the battle stayed paused");
    assert!(model(&harness).refills() > refills);
    assert_eq!(model(&harness).selection_groups().len(), 1);
}

// --- Failures (G-18) -----------------------------------------------------------------------------

fn fail_simulation(harness: &mut Harness) {
    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .battle_failure = Some("simulation failed: a test stopped it".to_owned());
    harness.app.update();
    harness.app.update();
}

/// [`Harness::click`], without the check that fails a test on a battle failure.
fn click_during_failure(harness: &mut Harness, position: Vec2) {
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

#[test]
fn a_presentation_failure_is_visible_without_the_inspector() {
    let mut harness = skirmish();
    assert_eq!(hud(&harness).failure, None);
    let reported = ClientMediaError::UnknownIdentity {
        unit_type: UnitTypeId(u16::MAX),
    };
    harness
        .app
        .world_mut()
        .resource_mut::<ClientMediaStatus>()
        .record_failure(reported);
    for _ in 0..3 {
        harness.app.update();
    }

    let geometry = hud(&harness);
    let banner = geometry.failure.expect("the failure must be shown");
    assert_eq!(geometry.reset, None, "resetting cannot repair media");
    let world = harness.app.world().resource::<UiGeometry>().world.unwrap();
    assert!(
        !banner.intersects(world),
        "the banner reserves its own space"
    );
    assert_eq!(harness.app.world().resource::<UiGeometry>().inspector, None);
}

/// A stopped battle no longer clears the session's lists, so they must not be read twice.
#[test]
fn a_failure_does_not_repeat_the_last_evidence_of_the_battle() {
    let mut harness = skirmish();
    let base = structure_of(&harness, LOCAL_PLAYER);
    let unit_type = trainable(&harness);
    harness.world_mut().player_resources[0].minerals = 0;
    submit(
        &mut harness,
        LOCAL_PLAYER,
        &[base],
        UnitOrder::Produce { unit_type },
    );
    harness.resume();
    let mut frames = 0;
    while feed_lines(&harness).is_empty() {
        harness.step(1);
        frames += 1;
        assert!(frames < 100, "the refusal must reach the feed");
    }
    assert!(
        !harness
            .app
            .world()
            .resource::<BattleSession>()
            .notices
            .is_empty(),
        "the refusal is still in the session's list"
    );

    fail_simulation(&mut harness);
    for _ in 0..5 {
        harness.app.update();
    }

    assert_eq!(feed_lines(&harness).len(), 1);
}

#[test]
fn a_failed_battle_offers_a_reset_that_recovers_it() {
    let mut harness = skirmish();
    let before = harness
        .app
        .world()
        .resource::<BattleSession>()
        .commands
        .generation();
    fail_simulation(&mut harness);
    let reset = point(
        hud(&harness)
            .reset
            .expect("a simulation failure can be reset"),
    );
    assert!(hud(&harness).pause.is_some(), "the bar stays on screen");

    click_during_failure(&mut harness, reset);
    for _ in 0..8 {
        harness.app.update();
    }

    let lab = harness.app.world().resource::<LabState>();
    assert_eq!(lab.battle_failure, None, "a new world recovers the failure");
    assert_eq!(hud(&harness).failure, None);
    let after = harness
        .app
        .world()
        .resource::<BattleSession>()
        .commands
        .generation();
    assert!(after > before, "{after:?} must replace {before:?}");
    assert!(after > Some(SessionGeneration::FIRST));
}

#[test]
fn a_failure_with_no_battle_still_sets_the_world_rectangle_and_shows() {
    let mut harness = skirmish();
    harness
        .app
        .world_mut()
        .resource_mut::<BattleSession>()
        .release_world();
    fail_simulation(&mut harness);
    harness.app.update();

    let geometry = hud(&harness);
    assert!(geometry.failure.is_some());
    assert_eq!(
        geometry.top, None,
        "with no battle there is nothing to show"
    );
    assert_eq!(geometry.bottom, None);
    assert!(harness.app.world().resource::<UiGeometry>().world.is_some());
}

// --- The catalog ---------------------------------------------------------------------------------

#[test]
fn the_catalog_derives_labels_and_capabilities_from_data() {
    let harness = skirmish();
    let world = harness.world();
    let catalog: &CommandCatalog = &model(&harness).catalog;
    let status = harness.app.world().resource::<ClientMediaStatus>();
    let registry = harness.app.world().resource::<FlipbookRegistry>();
    let mut builders = 0;
    for archetype in world.catalog.archetypes() {
        let id = archetype.unit_type_id;
        let entry = catalog.archetype(id).expect("every identity is listed");
        assert_eq!(entry.label, archetype.display_name);
        assert_eq!(catalog.label(id), archetype.display_name);
        assert_eq!(entry.mobile, !archetype.is_immobile_by_default());
        assert_eq!(entry.overclock, archetype.overclock.is_some());
        assert_eq!(entry.harvester, archetype.capabilities.harvester.is_some());
        assert_eq!(entry.carrier, archetype.capabilities.transport_capacity > 0);
        assert_eq!(
            entry.structure,
            matches!(archetype.shape, ShapeKind::BuildingDome { .. })
        );
        assert_eq!(
            entry.unpresentable.is_none(),
            status.presentable(registry, id).is_ok(),
            "{}",
            archetype.slug
        );
        for product in &entry.trains {
            assert!(!catalog.archetype(*product).unwrap().structure);
        }
        for upgrade in &entry.researches {
            let definition = world.upgrade_catalog.get_by_id(*upgrade).unwrap();
            assert_eq!(
                catalog.upgrade(*upgrade).unwrap().label,
                definition.display_name
            );
        }
        builders += usize::from(!entry.raises.is_empty());
        for raised in &entry.raises {
            assert!(archetype.can_raise(world.catalog.get(*raised).unwrap()));
        }
    }
    assert!(builders > 0, "the catalog must have a builder");
    assert_eq!(catalog.label(UnitTypeId(u16::MAX)), "");
    assert_eq!(catalog.upgrade(u32::MAX), None);
}
