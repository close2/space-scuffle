//! Right-click context orders, proved with real input events on the shipped skirmish.
//!
//! # Responsibility
//! Boots the production client on `scenarios/skirmish.ron`, as `client --skirmish` does, sends the
//! mouse and keyboard events a window backend would, and asserts on the commands the battle queue
//! captured and on what the simulation then did with them.
//!
//! # How it fits
//! The per-row rules of the right click live in `battle_input::context_order`, tested there
//! without Bevy. This file proves that the pointer reaches those rules: that a billboard on the
//! screen resolves to its node, and that the resulting order earns income.
//!
//! # Limits
//! Shares the limits of `support/battle_harness.rs`: no window and no renderer. An order whose
//! product has no shipped media, such as gas, cannot be played here to its end, so those rows are
//! covered by the unit tests.
//!
//! # Invariants
//! - Setup may write the world through `world_mut`, as the harness allows. Every order under test
//!   arrives through an input event.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use bevy::input::ButtonState;
use bevy::prelude::*;
use client::battle_input::order_marker::OrderMarkerKind;
use client::battle_pipeline::session::BattleSession;
use client::input_bindings::SHIFT_MODIFIERS;
use client::runtime_battle::{MapPresentation, SKIRMISH_SCENARIO};
use sim_core::entity::Cargo;
use sim_types::{EntityId, PlayerCommand, ResourceKind, ResourceNodeId, UnitOrder};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{Harness, LOCAL_PLAYER};

/// Frames a worker gets to walk to a node, mine it and carry the load home, at 25 ticks a second.
const MINING_ROUND_FRAMES: usize = 900;

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
    position.x += sim_types::SimScalar::from_num(2);
    let soldier = world
        .spawn_unit(soldier_type, LOCAL_PLAYER, position)
        .expect("open ground beside a worker");
    harness.resume();
    harness.step(3);
    harness.pause();
    soldier
}

// --- Right click on a node ---------------------------------------------------------------------

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
