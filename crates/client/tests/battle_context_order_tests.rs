//! Context orders and command modes, proved with real input events on the shipped skirmish.
//!
//! # Responsibility
//! Boots the production client on `scenarios/skirmish.ron`, as `client --skirmish` does, sends the
//! mouse and keyboard events a window backend would, and asserts on the commands the battle queue
//! captured and on what the simulation then did with them.
//!
//! # How it fits
//! The per-row rules of the right click live in `battle_input::context_order` and the site
//! arithmetic in `battle_input::command_mode`, both tested there without Bevy. This file proves
//! that the pointer reaches those rules: that a billboard on the screen resolves to its node, that
//! the resulting order earns income, and that a mode owns the left button until it ends. Entering
//! a mode from the card belongs to `battle_card_tests.rs`, so tests here set `CommandMode` directly.
//!
//! # Limits
//! Shares the limits of `support/battle_harness.rs`: no window and no renderer. An order whose
//! product has no shipped media, such as gas, cannot be played here to its end, so those rows are
//! covered by the unit tests.
//!
//! # Invariants
//! - Setup may write the world through `world_mut`, as the harness allows, and may arm a mode.
//!   Every order under test arrives through an input event.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use client::battle_input::command_mode::CommandMode;
use client::battle_input::gesture::SelectionGesture;
use client::battle_input::order_marker::OrderMarkerKind;
use client::battle_pipeline::session::BattleSession;
use client::input_bindings::{CANCEL_KEY, HOLD_POSITION_KEY, SHIFT_MODIFIERS, STOP_KEY};
use client::runtime_battle::{MapPresentation, SKIRMISH_SCENARIO};
use client::LabState;
use sim_core::entity::Cargo;
use sim_types::{
    EntityId, PlayerCommand, ResourceKind, ResourceNodeId, SimScalar, UnitOrder, UnitTypeId,
};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{Harness, LOCAL_PLAYER};

/// Frames a worker gets to walk to a node, mine it and carry the load home, at 25 ticks a second.
const MINING_ROUND_FRAMES: usize = 900;

/// A structure with shipped media and no prerequisite but the starting base.
const STRUCTURE: &str = "assembly_bay_dome";

/// Minimum corner of open plateau in the skirmish, clear of the home player's nodes.
const SITE_CORNER: u32 = 2;

/// Height above the ground, in world units, of the middle of a blocker's bubble.
const BLOCKER_HEIGHT: f32 = 0.7;

/// The opposing player of the skirmish.
const ENEMY: sim_types::PlayerId = sim_types::PlayerId(1);

/// More minerals than any test can spend.
const RICH: u32 = 100_000;

/// A window position over the inspector panel of the default battle.
const OVER_THE_PANEL: Vec2 = Vec2::new(100.0, 300.0);

fn skirmish() -> Harness {
    Harness::boot_scenario(client::ClientMode::Skirmish, SKIRMISH_SCENARIO)
}

/// The local player's harvesters, ordered by ID.
fn workers(harness: &Harness) -> Vec<EntityId> {
    let world = harness.world();
    world
        .entities
        .values()
        .filter(|unit| {
            unit.owner == LOCAL_PLAYER
                && world
                    .catalog
                    .get(unit.unit_type_id)
                    .is_ok_and(|archetype| archetype.capabilities.harvester.is_some())
        })
        .map(|unit| unit.id)
        .collect()
}

/// Map position of `node`.
fn node_position(harness: &Harness, node: ResourceNodeId) -> Vec2 {
    let position = harness.world().map.resources()[node.0 as usize].pos;
    Vec2::new(position.x.to_f32(), position.y.to_f32())
}

/// The mineral node nearest `origin` that no unit stands in front of on screen.
///
/// A click resolves units first, so a test of node picking needs a node with nothing over it.
fn clear_mineral(harness: &Harness, origin: Vec2) -> ResourceNodeId {
    let world = harness.world();
    let distance = |node: &sim_map::ResourceNode| {
        Vec2::new(node.pos.x.to_f32(), node.pos.y.to_f32()).distance(origin)
    };
    let unit_clear = |node: &sim_map::ResourceNode| {
        let at = Vec2::new(node.pos.x.to_f32(), node.pos.y.to_f32());
        world.entities.values().all(|unit| {
            Vec2::new(unit.position.x.to_f32(), unit.position.y.to_f32()).distance(at) > 3.0
        })
    };
    let (index, _) = world
        .map
        .resources()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == ResourceKind::Mineral && unit_clear(node))
        .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))
        .expect("the map must have a mineral away from the start");
    ResourceNodeId(u32::try_from(index).expect("node index fits"))
}

/// Window position of the billboard the player sees for `node`.
fn node_on_screen(harness: &mut Harness, node: ResourceNodeId) -> Vec2 {
    let position =
        harness.app.world().resource::<MapPresentation>().objects[node.0 as usize].position;
    harness.screen_position(position)
}

/// Captured commands in the order they were issued, while the battle is paused.
fn pending(harness: &Harness) -> Vec<PlayerCommand> {
    harness
        .app
        .world()
        .resource::<BattleSession>()
        .commands
        .pending_commands()
        .map(|(_, command)| command.clone())
        .collect()
}

/// Arms `mode` as the command card will, and lets a frame pass.
fn set_mode(harness: &mut Harness, mode: CommandMode) {
    *harness.app.world_mut().resource_mut::<CommandMode>() = mode;
    harness.step(1);
}

fn mode(harness: &Harness) -> CommandMode {
    *harness.app.world().resource::<CommandMode>()
}

/// Tells the client whether the window has keyboard and mouse focus.
fn set_focus(harness: &mut Harness, focused: bool) {
    harness
        .app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>()
        .single_mut(harness.app.world_mut())
        .focused = focused;
    harness.step(1);
}

fn minerals(harness: &Harness) -> u32 {
    harness.world().player_resources[usize::from(LOCAL_PLAYER.0)].minerals
}

/// Selects a worker with a clear mineral in view, and returns both.
fn worker_and_mineral(harness: &mut Harness) -> (EntityId, ResourceNodeId) {
    let worker = workers(harness)[0];
    let node = clear_mineral(harness, harness.simulated_position(worker));
    let worker_at = harness.simulated_position(worker);
    let node_at = node_position(harness, node);
    harness.look_at((worker_at + node_at) * 0.5);
    harness.select(worker);
    assert_eq!(harness.selection().ids(), [worker]);
    (worker, node)
}

/// Spawns a soldier beside `worker` and shows its bubble. It has no harvester.
fn soldier_beside(harness: &mut Harness, worker: EntityId) -> EntityId {
    let world = harness.world_mut();
    let soldier_type = world
        .catalog
        .id_by_slug("pulse_bot")
        .expect("the catalog ships this unit");
    let mut position = world.entities[&worker].position;
    position.x += SimScalar::from_num(2);
    let soldier = world
        .spawn_unit(soldier_type, LOCAL_PLAYER, position)
        .expect("open ground beside a worker");
    harness.resume();
    harness.step(3);
    harness.pause();
    soldier
}

/// Spawns a unit of `owner` standing in front of `node`'s billboard, and returns it with the
/// window position of that billboard.
fn blocker_in_front_of(
    harness: &mut Harness,
    node: ResourceNodeId,
    owner: sim_types::PlayerId,
) -> (EntityId, Vec2) {
    // The sight line from the camera to the billboard, where it passes at a unit's height.
    let billboard =
        harness.app.world().resource::<MapPresentation>().objects[node.0 as usize].position;
    let world = harness.app.world_mut();
    let eye = world
        .query_filtered::<&GlobalTransform, With<client::RtsCamera>>()
        .single(world)
        .translation();
    let node_at = node_position(harness, node);
    let ground = harness
        .world()
        .map
        .terrain_altitude_scalar(battle_harness::sim_position(node_at))
        .to_f32();
    let height = ground + BLOCKER_HEIGHT;
    let on_sight_line = eye.lerp(billboard, (eye.y - height) / (eye.y - billboard.y));
    let world = harness.world_mut();
    let unit_type = world
        .catalog
        .id_by_slug("pulse_bot")
        .expect("the catalog ships this unit");
    let position = sim_types::SimVec2::new(
        SimScalar::from_num(on_sight_line.x),
        SimScalar::from_num(on_sight_line.z),
    );
    let blocker = world
        .spawn_unit(unit_type, owner, position)
        .expect("open ground beside a node");
    harness.resume();
    harness.step(3);
    harness.pause();
    let billboard = node_on_screen(harness, node);
    (blocker, billboard)
}

// --- Right click on a node ---------------------------------------------------------------------

#[test]
fn a_friendly_unit_in_front_of_a_mineral_does_not_turn_harvest_into_move() {
    let mut harness = skirmish();
    let (worker, node) = worker_and_mineral(&mut harness);
    let (blocker, billboard) = blocker_in_front_of(&mut harness, node, LOCAL_PLAYER);

    // Control: the pointer really is on the blocker, as a left click there selects it.
    harness.click(MouseButton::Left, billboard);
    assert_eq!(harness.selection().ids(), [blocker]);
    harness.select(worker);
    assert_eq!(harness.selection().ids(), [worker]);

    harness.click(MouseButton::Right, billboard);

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [worker]);
    assert_eq!(orders[0].order, UnitOrder::Harvest { target: node });
}

#[test]
fn a_soldier_right_clicking_a_mineral_behind_a_friend_still_moves() {
    let mut harness = skirmish();
    let (worker, node) = worker_and_mineral(&mut harness);
    let soldier = soldier_beside(&mut harness, worker);
    let (_, billboard) = blocker_in_front_of(&mut harness, node, LOCAL_PLAYER);
    harness.select(soldier);

    harness.click(MouseButton::Right, billboard);

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert!(
        matches!(orders[0].order, UnitOrder::Move { .. }),
        "{:?}",
        orders[0].order
    );
}

#[test]
fn an_enemy_in_front_of_a_mineral_is_attacked_rather_than_harvested_past() {
    let mut harness = skirmish();
    let (worker, node) = worker_and_mineral(&mut harness);
    let (enemy, billboard) = blocker_in_front_of(&mut harness, node, ENEMY);

    harness.click(MouseButton::Right, billboard);

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [worker]);
    assert_eq!(orders[0].order, UnitOrder::attack(enemy));
}

#[test]
fn right_click_on_a_mineral_sends_a_worker_to_harvest_and_income_rises() {
    let mut harness = skirmish();
    let (worker, node) = worker_and_mineral(&mut harness);
    let before = minerals(&harness);

    let position = node_on_screen(&mut harness, node);
    harness.click(MouseButton::Right, position);

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [worker]);
    assert_eq!(orders[0].order, UnitOrder::Harvest { target: node });
    assert!(!orders[0].queue);
    let (kind, _) = harness.marker().expect("the order must show a marker");
    assert_eq!(kind, OrderMarkerKind::Move);

    harness.resume();
    for _ in 0..MINING_ROUND_FRAMES {
        harness.step(1);
        if minerals(&harness) > before {
            return;
        }
    }
    panic!("income must rise from {before}: {}", harness.status());
}

#[test]
fn right_click_on_a_mineral_with_no_harvester_selected_moves() {
    let mut harness = skirmish();
    let (worker, node) = worker_and_mineral(&mut harness);
    let soldier = soldier_beside(&mut harness, worker);
    harness.select(soldier);
    assert_eq!(harness.selection().ids(), [soldier]);

    let position = node_on_screen(&mut harness, node);
    harness.click(MouseButton::Right, position);

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [soldier]);
    assert!(
        matches!(orders[0].order, UnitOrder::Move { .. }),
        "{:?}",
        orders[0].order
    );
}

#[test]
fn shift_right_click_on_a_mineral_queues_after_a_move() {
    let mut harness = skirmish();
    let (worker, node) = worker_and_mineral(&mut harness);
    let destination = harness.open_ground(harness.simulated_position(worker), 3.0);
    let ground = harness.ground_on_screen(destination);
    let billboard = node_on_screen(&mut harness, node);

    harness.click(MouseButton::Right, ground);
    harness.with_key_held(SHIFT_MODIFIERS[0], |harness| {
        harness.click(MouseButton::Right, billboard);
    });

    let orders = pending(&harness);
    assert_eq!(orders.len(), 2, "status: {}", harness.status());
    assert!(matches!(orders[0].order, UnitOrder::Move { .. }));
    assert!(!orders[0].queue);
    assert_eq!(orders[1].order, UnitOrder::Harvest { target: node });
    assert!(orders[1].queue, "Shift must queue the harvest");

    harness.resume();
    harness.step(3);
    let unit = &harness.world().entities[&worker];
    assert_eq!(
        unit.order_queue.back(),
        Some(&UnitOrder::Harvest { target: node }),
        "the harvest must wait behind the move"
    );
}

// --- Right click on a friendly structure ---------------------------------------------------------

#[test]
fn a_worker_with_cargo_returns_it_on_right_click_at_a_depot() {
    let mut harness = skirmish();
    let worker = workers(&harness)[0];
    let depot = harness.local_structure();
    let load = 5;
    harness
        .world_mut()
        .entities
        .get_mut(&worker)
        .expect("worker")
        .cargo = Some(Cargo {
        kind: ResourceKind::Mineral,
        amount: load,
    });
    let depot_at = harness.simulated_position(depot);
    harness.look_at(depot_at);
    harness.select(worker);
    let before = minerals(&harness);

    let position = harness.unit_on_screen(depot);
    harness.click(MouseButton::Right, position);

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::ReturnCargo);

    harness.resume();
    for _ in 0..MINING_ROUND_FRAMES {
        harness.step(1);
        if minerals(&harness) == before + load {
            assert_eq!(harness.world().entities[&worker].cargo, None);
            return;
        }
    }
    panic!("the load must reach the depot: {}", harness.status());
}

// --- Construction -------------------------------------------------------------------------------

/// Arms placement of the structure the skirmish opens with, for a funded player's worker.
///
/// Returns the worker, the structure and the map point at the middle of a free site.
fn ready_to_place(harness: &mut Harness) -> (EntityId, UnitTypeId, Vec2) {
    let worker = workers(harness)[0];
    harness.world_mut().player_resources[usize::from(LOCAL_PLAYER.0)].minerals = RICH;
    let structure = harness
        .world()
        .catalog
        .id_by_slug(STRUCTURE)
        .expect("the catalog ships this structure");
    let site = Vec2::new(SITE_CORNER as f32 + 1.5, SITE_CORNER as f32 + 1.5);
    let worker_at = harness.simulated_position(worker);
    harness.look_at((worker_at + site) * 0.5);
    harness.select(worker);
    assert_eq!(harness.selection().ids(), [worker]);
    set_mode(harness, CommandMode::Place(structure));
    (worker, structure, site)
}

/// Runs the battle until a foundation stands, checking `each_frame` until then.
fn run_until_laid(harness: &mut Harness, mut each_frame: impl FnMut(&Harness)) -> EntityId {
    harness.resume();
    for _ in 0..MINING_ROUND_FRAMES {
        harness.step(1);
        let laid = harness
            .world()
            .entities
            .values()
            .find(|unit| unit.construction.is_some())
            .map(|unit| unit.id);
        if let Some(foundation) = laid {
            return foundation;
        }
        each_frame(harness);
    }
    panic!(
        "the builder never laid the foundation: {}",
        harness.status()
    );
}

#[test]
fn a_placed_build_pays_on_arrival() {
    let mut harness = skirmish();
    let (worker, structure, site) = ready_to_place(&mut harness);
    let position = harness.ground_on_screen(site);

    harness.click(MouseButton::Left, position);

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [worker]);
    assert_eq!(
        orders[0].order,
        UnitOrder::Build {
            structure_type: structure,
            cell_x: SITE_CORNER,
            cell_y: SITE_CORNER,
        }
    );
    assert_eq!(mode(&harness), CommandMode::Normal);
    let (kind, marker) = harness.marker().expect("the order must show a marker");
    assert_eq!(kind, OrderMarkerKind::Move);
    assert!(
        Vec2::new(marker.x, marker.z).distance(site) < 0.01,
        "marker at {marker} must sit on the site's centre {site}"
    );

    let foundation = run_until_laid(&mut harness, |harness| {
        assert_eq!(minerals(harness), RICH, "nothing is paid on the way");
    });
    let cost = harness
        .world()
        .catalog
        .get(structure)
        .expect("structure")
        .mineral_cost;
    assert_eq!(minerals(&harness), RICH - cost, "paid on arrival");
    assert_eq!(
        harness.world().entities[&foundation].unit_type_id,
        structure
    );
}

#[test]
fn right_click_on_a_foundation_resumes_it() {
    let mut harness = skirmish();
    let (first, _, site) = ready_to_place(&mut harness);
    let position = harness.ground_on_screen(site);
    harness.click(MouseButton::Left, position);
    let foundation = run_until_laid(&mut harness, |_| {});
    harness.tap_key(STOP_KEY);
    harness.pause();
    harness.step(3);
    let left_at = harness.world().entities[&foundation]
        .construction
        .as_ref()
        .expect("still a foundation")
        .remaining_ticks;

    let second = workers(&harness)[1];
    assert_ne!(first, second);
    harness.select(second);
    assert_eq!(harness.selection().ids(), [second]);
    let dome = harness.unit_on_screen(foundation);
    harness.click(MouseButton::Right, dome);

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].order, UnitOrder::Construct { foundation });
    assert_eq!(orders[0].entities, [second]);

    harness.resume();
    for _ in 0..MINING_ROUND_FRAMES {
        harness.step(1);
        let remaining = harness.world().entities[&foundation]
            .construction
            .as_ref()
            .map_or(0, |construction| construction.remaining_ticks);
        if remaining < left_at {
            return;
        }
    }
    panic!(
        "the second worker must resume the work: {}",
        harness.status()
    );
}

#[test]
fn a_refused_site_issues_nothing_and_keeps_the_mode() {
    let mut harness = skirmish();
    let (_, structure, _) = ready_to_place(&mut harness);
    let base = harness.local_structure();
    let base_at = harness.simulated_position(base);
    let position = harness.ground_on_screen(base_at);

    harness.click(MouseButton::Left, position);

    assert_eq!(pending(&harness).len(), 0);
    assert_eq!(mode(&harness), CommandMode::Place(structure));
    assert_eq!(
        harness.status(),
        "Order rejected: a structure is in the way"
    );
    assert_eq!(harness.marker(), None, "no marker for a refused site");
}

#[test]
fn shift_queues_a_second_site_and_keeps_the_mode() {
    let mut harness = skirmish();
    let (_, structure, site) = ready_to_place(&mut harness);
    let first = harness.ground_on_screen(site);
    let second = harness.ground_on_screen(site + Vec2::new(0.0, 5.0));

    harness.with_key_held(SHIFT_MODIFIERS[0], |harness| {
        harness.click(MouseButton::Left, first);
        harness.click(MouseButton::Left, second);
    });

    let orders = pending(&harness);
    assert_eq!(orders.len(), 2, "status: {}", harness.status());
    assert!(orders.iter().all(|command| command.queue));
    assert_ne!(orders[0].order, orders[1].order);
    assert_eq!(mode(&harness), CommandMode::Place(structure));
}

// --- Attack-move mode ----------------------------------------------------------------------------

/// A selected soldier, ready for a mode, and open ground in view.
fn soldier_and_ground(harness: &mut Harness) -> (EntityId, Vec2) {
    let (worker, _) = worker_and_mineral(harness);
    let soldier = soldier_beside(harness, worker);
    harness.select(soldier);
    assert_eq!(harness.selection().ids(), [soldier]);
    let ground = harness.open_ground(harness.simulated_position(soldier), 4.0);
    (soldier, ground)
}

#[test]
fn attack_move_mode_orders_the_ground_and_returns_to_normal() {
    let mut harness = skirmish();
    let (soldier, ground) = soldier_and_ground(&mut harness);
    set_mode(&mut harness, CommandMode::AttackMove);
    let position = harness.ground_on_screen(ground);

    harness.click(MouseButton::Left, position);

    let orders = pending(&harness);
    assert_eq!(orders.len(), 1, "status: {}", harness.status());
    assert_eq!(orders[0].entities, [soldier]);
    assert!(!orders[0].queue);
    let UnitOrder::AttackMove { target } = orders[0].order else {
        panic!("expected an attack-move: {:?}", orders[0].order);
    };
    let target = Vec2::new(target.x.to_f32(), target.y.to_f32());
    assert!(target.distance(ground) < 0.5, "{target} is not {ground}");
    assert_eq!(mode(&harness), CommandMode::Normal);
    assert_eq!(harness.selection().ids(), [soldier], "the selection stays");
    let (kind, _) = harness.marker().expect("the order must show a marker");
    assert_eq!(kind, OrderMarkerKind::Attack);

    let start = harness.simulated_position(soldier).distance(ground);
    harness.resume();
    harness.step(120);
    let end = harness.simulated_position(soldier).distance(ground);
    assert!(end < start, "the soldier must set off: {start} -> {end}");
}

#[test]
fn shift_keeps_the_mode() {
    let mut harness = skirmish();
    let (_, ground) = soldier_and_ground(&mut harness);
    set_mode(&mut harness, CommandMode::AttackMove);
    let position = harness.ground_on_screen(ground);

    harness.with_key_held(SHIFT_MODIFIERS[0], |harness| {
        harness.click(MouseButton::Left, position);
        harness.click(MouseButton::Left, position);
    });

    let orders = pending(&harness);
    assert_eq!(orders.len(), 2, "status: {}", harness.status());
    assert!(orders.iter().all(|command| command.queue), "Shift queues");
    assert_eq!(mode(&harness), CommandMode::AttackMove);
}

#[test]
fn right_click_cancels_the_mode_and_issues_nothing() {
    let mut harness = skirmish();
    let (_, ground) = soldier_and_ground(&mut harness);
    set_mode(&mut harness, CommandMode::AttackMove);
    let position = harness.ground_on_screen(ground);

    harness.click(MouseButton::Right, position);

    assert_eq!(mode(&harness), CommandMode::Normal);
    assert_eq!(pending(&harness).len(), 0);
    assert_eq!(harness.marker(), None);
}

#[test]
fn escape_cancels_the_mode() {
    let mut harness = skirmish();
    soldier_and_ground(&mut harness);
    set_mode(&mut harness, CommandMode::AttackMove);

    harness.tap_key(CANCEL_KEY);

    assert_eq!(mode(&harness), CommandMode::Normal);
    assert_eq!(pending(&harness).len(), 0);
}

#[test]
fn a_left_click_in_a_mode_changes_no_selection() {
    let mut harness = skirmish();
    let (soldier, _) = soldier_and_ground(&mut harness);
    let worker = workers(&harness)[0];
    set_mode(&mut harness, CommandMode::AttackMove);

    let position = harness.unit_on_screen(worker);
    harness.click(MouseButton::Left, position);

    assert_eq!(harness.selection().ids(), [soldier]);
    assert_eq!(
        harness
            .app
            .world()
            .resource::<SelectionGesture>()
            .drag_rect(),
        None
    );
    assert_eq!(pending(&harness).len(), 1, "the click was the order");
}

#[test]
fn a_mode_armed_during_a_drag_abandons_the_box() {
    let mut harness = skirmish();
    let (soldier, ground) = soldier_and_ground(&mut harness);
    let position = harness.ground_on_screen(ground);

    harness.move_cursor(position);
    harness.send_mouse(MouseButton::Left, ButtonState::Pressed);
    harness.move_cursor(position + Vec2::splat(40.0));
    set_mode(&mut harness, CommandMode::AttackMove);
    harness.step(1);
    harness.send_mouse(MouseButton::Left, ButtonState::Released);

    assert_eq!(harness.selection().ids(), [soldier], "no box was applied");
    assert_eq!(pending(&harness).len(), 0, "no order came from the release");
    assert_eq!(mode(&harness), CommandMode::AttackMove);
}

#[test]
fn a_selection_that_cannot_carry_out_a_mode_leaves_it() {
    let mut harness = skirmish();
    let base = harness.local_structure();
    let base_at = harness.simulated_position(base);
    harness.look_at(base_at);
    harness.select(base);
    assert_eq!(harness.selection().ids(), [base]);

    set_mode(&mut harness, CommandMode::AttackMove);

    assert_eq!(mode(&harness), CommandMode::Normal);
}

#[test]
fn a_dead_selection_leaves_the_mode() {
    let mut harness = skirmish();
    let (soldier, _) = soldier_and_ground(&mut harness);
    set_mode(&mut harness, CommandMode::AttackMove);
    assert_eq!(mode(&harness), CommandMode::AttackMove);

    harness
        .world_mut()
        .entities
        .get_mut(&soldier)
        .expect("soldier")
        .hp = SimScalar::ZERO;
    harness.step(1);

    assert_eq!(mode(&harness), CommandMode::Normal);
}

#[test]
fn a_battle_reset_leaves_the_mode() {
    let mut harness = skirmish();
    soldier_and_ground(&mut harness);
    set_mode(&mut harness, CommandMode::AttackMove);

    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .battle_reset_requested = true;
    harness.step(3);

    assert_eq!(mode(&harness), CommandMode::Normal);
}

#[test]
fn losing_focus_keeps_the_mode_and_ignores_the_mouse() {
    let mut harness = skirmish();
    let (_, ground) = soldier_and_ground(&mut harness);
    set_mode(&mut harness, CommandMode::AttackMove);
    let position = harness.ground_on_screen(ground);
    harness.move_cursor(position);
    set_focus(&mut harness, false);

    harness.send_mouse(MouseButton::Left, ButtonState::Pressed);
    harness.send_mouse(MouseButton::Left, ButtonState::Released);

    assert_eq!(mode(&harness), CommandMode::AttackMove);
    assert_eq!(pending(&harness).len(), 0);

    set_focus(&mut harness, true);
    harness.click(MouseButton::Left, position);
    assert_eq!(
        pending(&harness).len(),
        1,
        "the mode works once focus returns"
    );
}

#[test]
fn a_click_on_a_panel_issues_nothing_and_keeps_the_mode() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    set_mode(&mut harness, CommandMode::AttackMove);

    harness.click(MouseButton::Left, OVER_THE_PANEL);
    harness.click(MouseButton::Right, OVER_THE_PANEL);

    assert_eq!(mode(&harness), CommandMode::AttackMove);
    assert_eq!(pending(&harness).len(), 0);
}

// --- The end of the match ------------------------------------------------------------------------

#[test]
fn orders_after_the_match_do_nothing() {
    let mut harness = skirmish();
    let (soldier, ground) = soldier_and_ground(&mut harness);
    let worker = workers(&harness)[0];
    let position = harness.ground_on_screen(ground);
    set_mode(&mut harness, CommandMode::AttackMove);

    for unit in harness.world_mut().entities.values_mut() {
        if unit.owner != LOCAL_PLAYER && unit.is_immobile {
            unit.hp = SimScalar::ZERO;
        }
    }
    harness.resume();
    for _ in 0..MINING_ROUND_FRAMES {
        harness.step(1);
        if harness.world().match_state.result.is_over() {
            break;
        }
    }
    assert!(harness.world().match_state.result.is_over());
    harness.pause();
    assert_eq!(mode(&harness), CommandMode::Normal, "the end resets a mode");
    let status = harness.status();

    assert_eq!(harness.selection().ids(), [soldier]);
    harness.click(MouseButton::Right, position);
    harness.tap_key(HOLD_POSITION_KEY);
    harness.tap_key(STOP_KEY);
    assert_eq!(pending(&harness).len(), 0, "orders are closed");
    assert_eq!(harness.status(), status, "and nothing is reported");
    assert_eq!(harness.marker(), None);

    set_mode(&mut harness, CommandMode::AttackMove);
    assert_eq!(mode(&harness), CommandMode::Normal, "no mode can be armed");

    harness.select(worker);
    assert_eq!(harness.selection().ids(), [worker], "looking around works");
}

// --- Bevy event path ---------------------------------------------------------------------------

#[test]
fn a_right_press_alone_issues_the_order() {
    let mut harness = skirmish();
    let (_, node) = worker_and_mineral(&mut harness);
    let position = node_on_screen(&mut harness, node);
    harness.move_cursor(position);

    harness.send_mouse(MouseButton::Right, ButtonState::Pressed);

    assert_eq!(pending(&harness).len(), 1, "orders go on the press");
    harness.send_mouse(MouseButton::Right, ButtonState::Released);
    assert_eq!(pending(&harness).len(), 1, "the release adds nothing");
}
