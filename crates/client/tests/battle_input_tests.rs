//! Drives selection and orders through the real client plugin with real input events.
//!
//! # Responsibility
//! Boots the production [`client::ClientAppPlugin`] on the real default scenario, sends Bevy the
//! same keyboard, mouse and cursor events the window backend would, and asserts on the resulting
//! ECS state and on the simulation itself: units moved, stopped or received the attack order.
//!
//! # Limits
//! There is no window and no renderer here. Bevy's input, egui, camera and scheduling systems all
//! run, but pixels are not drawn and the operating system delivers nothing. Rings are checked by
//! their ECS visibility and material uniforms.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::ButtonState;
use bevy::prelude::*;
use client::battle_input::gesture::SelectionGesture;
use client::battle_input::order_marker::OrderMarkerKind;
use client::battle_pipeline::session::BattleSession;
use client::battle_presentation::BattlePresentation;
use client::bubble_shader::{BubbleDomeMaterial, BubbleInstance, FLAG_SELECTED};
use client::input_bindings::{HOLD_POSITION_KEY, STOP_KEY};
use client::scene_controller::SwarmBubble;
use client::{LabState, LabTab, RtsCamera, SelectionState};
use sim_types::{PlayerId, SimScalar, UnitOrder};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{sim_position, Harness, LOCAL_PLAYER};

/// A window position over the inspector panel.
const OVER_THE_PANEL: Vec2 = Vec2::new(100.0, 300.0);

// --- 5.1 Selection ---------------------------------------------------------------------------

#[test]
fn a_clicked_unit_keeps_its_ring_and_bar_while_the_battle_runs() {
    let mut harness = Harness::battle();
    let [unit, other, ..] = harness.local_army()[..] else {
        panic!("the scenario gives the player an army");
    };

    harness.select(unit);
    assert_eq!(harness.selection().ids(), [unit]);
    assert!(harness.draws_selected(unit), "shown on the same frame");

    harness.resume();
    harness.step(30);
    assert!(harness.world().current_tick > 0, "the battle must advance");
    assert!(harness.draws_selected(unit), "snapshots must not clear it");
    assert!(!harness.draws_selected(other));
}

#[test]
fn a_click_on_nothing_clears_the_selection() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);

    let empty_ground = harness.ground_on_screen(harness.near(unit, Vec2::new(-5.0, -5.0)));
    harness.click(MouseButton::Left, empty_ground);

    assert_eq!(harness.selection().ids().len(), 0);
    assert!(!harness.draws_selected(unit));
}

#[test]
fn a_shift_click_on_nothing_keeps_the_selection() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);

    let empty_ground = harness.ground_on_screen(harness.near(unit, Vec2::new(-5.0, -5.0)));
    harness.with_key_held(KeyCode::ShiftLeft, |harness| {
        harness.click(MouseButton::Left, empty_ground);
    });

    assert_eq!(harness.selection().ids(), [unit]);
}

#[test]
fn a_click_on_the_inspector_leaves_the_selection_alone() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);

    harness.click(MouseButton::Left, OVER_THE_PANEL);

    assert_eq!(harness.selection().ids(), [unit]);
}

#[test]
fn a_hidden_bubble_is_clicked_through() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    let position = harness.unit_on_screen(unit);
    let world = harness.app.world_mut();
    let entity = world
        .query::<(Entity, &BubbleInstance)>()
        .iter(world)
        .find(|(_, bubble)| bubble.0.entity_id == unit)
        .map(|(entity, _)| entity)
        .expect("unit bubble");
    world.entity_mut(entity).insert(Visibility::Hidden);
    harness.step(2);

    harness.click(MouseButton::Left, position);

    assert_eq!(harness.selection().ids().len(), 0);
}

#[test]
fn a_unit_behind_a_cliff_is_not_picked() {
    let mut harness = Harness::battle();
    let enemy = harness.enemy_units()[0];

    // The camera looks toward -Z, so ground just north of a plateau's edge is behind that edge.
    let map = &harness.world().map;
    let level = |x: u32, y: u32| map.cell(x, y).ok().map(|cell| cell.cliff_level.0);
    let (x, y) = (2..map.width() - 2)
        .flat_map(|x| (3..map.height() - 2).map(move |y| (x, y)))
        .find(|&(x, y)| {
            let edge_is_straight = (x - 1..=x + 1).all(|side| {
                level(side, y) > level(side, y - 1)
                    && level(side, y - 1) == level(side, y - 2)
                    && level(side, y + 1) == level(side, y)
                    && map.ramp_profile(side, y).is_none()
                    && map.ramp_profile(side, y - 1).is_none()
            });
            edge_is_straight && level(x, y - 1) != Some(0)
        })
        .expect("the map must have a plateau");
    let behind = Vec2::new(x as f32 + 0.5, y as f32 - 0.4);
    harness
        .world_mut()
        .entities
        .get_mut(&enemy)
        .expect("enemy")
        .position = sim_position(behind);
    harness.resume();
    harness.step(1);
    harness.pause();
    harness.look_at(behind);
    let on_plateau = harness.unit_on_screen(enemy);

    harness.click(MouseButton::Left, on_plateau);

    assert_eq!(harness.selection().inspected(), None, "the cliff hides it");
}

#[test]
fn a_cloaked_enemy_cannot_be_selected_or_targeted() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    let enemy = harness.enemy_units()[0];
    let beside = harness.near(unit, Vec2::new(0.0, 4.0));
    let entity = harness.world_mut().entities.get_mut(&enemy).expect("enemy");
    entity.position = sim_position(beside);
    entity.is_cloaked = true;
    harness.step(1);
    harness.resume();
    harness.step(2);
    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .battle_paused = true;
    harness.step(2);

    let enemy_position = harness.unit_on_screen(enemy);
    harness.click(MouseButton::Left, enemy_position);
    assert_eq!(harness.selection().inspected(), None);

    harness.select(unit);
    harness.click(MouseButton::Right, enemy_position);
    let session = harness.app.world().resource::<BattleSession>();
    let (_, command) = session
        .commands
        .pending_commands()
        .next()
        .expect("an order");
    assert!(
        matches!(command.order, UnitOrder::Move { .. }),
        "the order must pass through to the ground, not reveal the target"
    );
}

#[test]
fn selecting_one_unit_leaves_every_other_material_unchanged() {
    let mut harness = Harness::battle();
    let army = harness.local_army();
    harness.select(army[0]);
    for other in &army[1..] {
        assert!(!harness.draws_selected(*other));
    }
}

#[test]
fn a_dead_unit_leaves_the_selection_and_its_control_group() {
    let mut harness = Harness::battle();
    let [unit, survivor, ..] = harness.local_army()[..] else {
        panic!("the scenario gives the player an army");
    };
    harness.select(unit);
    harness.with_key_held(KeyCode::ShiftLeft, |harness| {
        let position = harness.unit_on_screen(survivor);
        harness.click(MouseButton::Left, position);
    });
    harness.with_key_held(KeyCode::ControlLeft, |harness| {
        harness.tap_key(KeyCode::Digit1);
    });
    assert_eq!(harness.selection().group(0).len(), 2);

    harness
        .world_mut()
        .entities
        .get_mut(&unit)
        .expect("unit")
        .hp = SimScalar::ZERO;
    harness.resume();
    harness.step(4);

    assert_eq!(harness.selection().ids(), [survivor]);
    assert_eq!(harness.selection().group(0), [survivor]);
}

#[test]
fn a_battle_reset_clears_selection_and_control_groups() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    harness.with_key_held(KeyCode::ControlLeft, |harness| {
        harness.tap_key(KeyCode::Digit1);
    });

    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .battle_reset_requested = true;
    harness.step(3);

    assert_eq!(harness.selection().ids().len(), 0);
    assert_eq!(harness.selection().group(0).len(), 0);
    assert!(
        !harness.draws_selected(unit),
        "IDs restart with the session"
    );
}

#[test]
fn the_selection_survives_a_visit_to_another_tab() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);

    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .current_tab = LabTab::VisualLab;
    harness.step(3);
    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .current_tab = LabTab::LiveBattle;
    harness.step(3);

    assert!(
        harness.draws_selected(unit),
        "respawned bubbles must draw it"
    );
}

#[test]
fn a_swarm_click_touches_no_shared_material_and_selects_nothing() {
    let mut harness = Harness::boot(client::ClientMode::SwarmStressTest);
    harness.step(3);
    let world = harness.app.world_mut();
    let (center, material) = world
        .query_filtered::<(&GlobalTransform, &MeshMaterial3d<BubbleDomeMaterial>), With<SwarmBubble>>()
        .iter(world)
        .next()
        .map(|(transform, material)| (transform.translation(), material.0.clone()))
        .expect("swarm bubble");
    let position = harness.screen_position(center);

    harness.click(MouseButton::Left, position);
    harness.step(2);

    let uniform = harness
        .app
        .world()
        .resource::<Assets<BubbleDomeMaterial>>()
        .get(&material)
        .expect("shared material")
        .uniform;
    assert_eq!(uniform.selected, 0, "all bubbles sharing it would light up");
    assert_eq!(uniform.feature_flags & FLAG_SELECTED, 0);
    let selection = harness.app.world().resource::<SelectionState>();
    assert_eq!(selection.selected_entity, None);
}

#[test]
fn a_hidden_lab_actor_is_clicked_through() {
    let mut harness = Harness::boot(client::ClientMode::VisualLab);
    harness.step(6);
    let mut actor_at = |index: usize| {
        let world = harness.app.world_mut();
        let center = world
            .query::<(&GlobalTransform, &client::HeroLineupIndex)>()
            .iter(world)
            .find(|(_, hero)| hero.0 == index)
            .map(|(transform, _)| transform.translation())
            .expect("lineup actor");
        harness.screen_position(center)
    };
    let second = actor_at(1);
    let third = actor_at(2);

    harness.click(MouseButton::Left, second);
    harness.step(2);
    let hero = |harness: &Harness| {
        harness
            .app
            .world()
            .resource::<SelectionState>()
            .selected_hero_index
    };
    assert_eq!(hero(&harness), Some(1), "a visible actor is picked");

    // The bouncing showcase hides the whole lineup.
    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .smog_bouncing_active = true;
    harness.step(3);
    harness.click(MouseButton::Left, third);
    harness.step(2);

    assert_ne!(hero(&harness), Some(2), "a hidden actor must not be picked");
}

// --- 5.2 Right-click orders ------------------------------------------------------------------

#[test]
fn right_click_on_terrain_moves_the_unit_there_and_marks_the_spot() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    let destination = harness.open_ground(harness.simulated_position(unit), 4.0);
    let position = harness.ground_on_screen(destination);
    let start_distance = harness.simulated_position(unit).distance(destination);

    harness.click(MouseButton::Right, position);

    let (kind, marker) = harness.marker().expect("the order must show a marker");
    assert_eq!(kind, OrderMarkerKind::Move);
    assert!(
        Vec2::new(marker.x, marker.z).distance(destination) < 0.5,
        "marker at {marker} must sit on the destination {destination}"
    );
    assert_eq!(harness.status(), "1 order queued");

    harness.resume();
    harness.step(120);
    let session = harness.app.world().resource::<BattleSession>();
    assert!(session.outcome_count > 0, "the simulation received it");
    let end_distance = harness.simulated_position(unit).distance(destination);
    assert!(
        end_distance < 1.0 && end_distance < start_distance,
        "the unit must arrive: {start_distance} -> {end_distance}"
    );
}

#[test]
fn the_order_marker_hides_itself() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    let position = harness.ground_on_screen(harness.near(unit, Vec2::new(4.0, 2.0)));
    harness.click(MouseButton::Right, position);
    assert!(harness.marker().is_some());

    harness.step(20);

    assert_eq!(harness.marker(), None);
}

#[test]
fn right_click_on_an_enemy_orders_the_attack_and_it_lands() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    let enemy = harness.enemy_units()[0];
    let beside = harness.near(unit, Vec2::new(0.0, 3.0));
    harness
        .world_mut()
        .entities
        .get_mut(&enemy)
        .expect("enemy")
        .position = sim_position(beside);
    harness.resume();
    harness.step(2);
    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .battle_paused = true;
    harness.step(2);
    let vitals = |harness: &Harness| {
        let target = &harness.world().entities[&enemy];
        target.hp + target.shields
    };
    let before = vitals(&harness);

    harness.select(unit);
    let target = harness.unit_on_screen(enemy);
    harness.click(MouseButton::Right, target);

    let (kind, _) = harness.marker().expect("the order must show a marker");
    assert_eq!(kind, OrderMarkerKind::Attack);
    harness.resume();
    harness.step(1);
    let orders = &harness.world().entities[&unit].order_queue;
    assert!(
        orders.iter().any(
            |order| matches!(order, UnitOrder::AttackTarget { target, .. } if *target == enemy)
        ),
        "the simulation must hold the attack order: {orders:?}"
    );
    harness.step(60);
    let damaged = harness
        .world()
        .entities
        .get(&enemy)
        .is_none_or(|target| target.hp + target.shields < before);
    assert!(damaged, "the target must take damage");
}

#[test]
fn right_click_with_a_foreign_unit_selected_issues_nothing() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    let enemy = harness.enemy_units()[0];
    let beside = harness.near(unit, Vec2::new(0.0, 3.0));
    harness
        .world_mut()
        .entities
        .get_mut(&enemy)
        .expect("enemy")
        .position = sim_position(beside);
    harness.resume();
    harness.step(2);
    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .battle_paused = true;
    harness.step(2);

    harness.select(enemy);
    assert_eq!(harness.selection().inspected(), Some(enemy));
    assert!(harness.draws_selected(enemy), "an enemy can be looked at");

    let ground = harness.ground_on_screen(harness.near(unit, Vec2::new(4.0, 2.0)));
    harness.click(MouseButton::Right, ground);

    assert_eq!(harness.pending_orders(), 0);
    assert_eq!(harness.marker(), None);
}

#[test]
fn a_shift_click_cannot_add_a_foreign_unit_to_an_army() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    let enemy = harness.enemy_units()[0];
    let beside = harness.near(unit, Vec2::new(0.0, 3.0));
    harness
        .world_mut()
        .entities
        .get_mut(&enemy)
        .expect("enemy")
        .position = sim_position(beside);
    harness.resume();
    harness.step(2);
    harness
        .app
        .world_mut()
        .resource_mut::<LabState>()
        .battle_paused = true;
    harness.step(2);

    harness.select(unit);
    harness.with_key_held(KeyCode::ShiftLeft, |harness| {
        let position = harness.unit_on_screen(enemy);
        harness.click(MouseButton::Left, position);
    });

    assert_eq!(harness.selection().ids(), [unit]);
    assert_eq!(harness.selection().inspected(), None);
}

#[test]
fn only_capable_units_of_a_mixed_selection_receive_an_order() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    let base = harness.local_structure();
    harness.select(unit);
    harness.with_key_held(KeyCode::ShiftLeft, |harness| {
        let position = harness.unit_on_screen(base);
        harness.click(MouseButton::Left, position);
    });
    let mut expected = vec![unit, base];
    expected.sort();
    assert_eq!(harness.selection().ids(), expected);

    let ground = harness.ground_on_screen(harness.near(unit, Vec2::new(-4.0, 2.0)));
    harness.click(MouseButton::Right, ground);

    let session = harness.app.world().resource::<BattleSession>();
    let (_, command) = session
        .commands
        .pending_commands()
        .next()
        .expect("an order");
    assert_eq!(command.entities, [unit], "a structure cannot move");
}

#[test]
fn a_structure_alone_reports_why_it_cannot_move() {
    let mut harness = Harness::battle();
    let base = harness.local_structure();
    harness.select(base);
    assert_eq!(harness.selection().ids(), [base]);
    let position = harness.simulated_position(base);
    let ground = harness.ground_on_screen(position + Vec2::new(-5.0, 0.0));

    harness.click(MouseButton::Right, ground);

    assert_eq!(harness.pending_orders(), 0);
    assert_eq!(harness.marker(), None);
    assert!(
        harness.status().starts_with("Order rejected"),
        "the player must see why: {:?}",
        harness.status()
    );
}

#[test]
fn right_click_over_the_abyss_reports_no_destination() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    let world = harness.app.world_mut();
    let mut cameras = world.query::<&mut RtsCamera>();
    cameras.single_mut(world).target = Vec2::new(-30.0, -30.0);
    // Live clamping keeps the focus on the map, so aim off its edge from a corner instead.
    harness.step(40);

    harness.click(MouseButton::Right, Vec2::new(420.0, 20.0));

    assert_eq!(harness.pending_orders(), 0);
    assert_eq!(
        harness.status(),
        "Order rejected: there is no walkable ground there"
    );
}

#[test]
fn orders_issued_while_paused_wait_for_the_battle_to_resume() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    let start = harness.simulated_position(unit);
    let ground = harness.ground_on_screen(harness.near(unit, Vec2::new(4.0, 2.0)));

    harness.click(MouseButton::Right, ground);
    harness.step(10);

    assert_eq!(harness.pending_orders(), 1);
    assert_eq!(harness.simulated_position(unit), start);
    harness.resume();
    harness.step(30);
    assert_eq!(harness.pending_orders(), 0);
    assert_ne!(harness.simulated_position(unit), start);
}

// --- 5.3 Box selection, control groups, Hold, Stop, queued orders ----------------------------

#[test]
fn a_box_drag_selects_the_players_units_inside_it() {
    let mut harness = Harness::battle();
    let army = harness.local_army();
    let positions: Vec<Vec2> = army.iter().map(|id| harness.unit_on_screen(*id)).collect();
    let from = positions.iter().fold(Vec2::MAX, |low, p| low.min(*p)) - Vec2::splat(12.0);
    let to = positions.iter().fold(Vec2::MIN, |high, p| high.max(*p)) + Vec2::splat(12.0);

    harness.drag(from, to);

    let selected = harness.selection().ids();
    assert!(
        army.iter().all(|id| selected.contains(id)),
        "{selected:?} must contain the whole army {army:?}"
    );
    let base = harness.local_structure();
    assert!(
        !selected.contains(&base),
        "mobile units win over structures"
    );
    for id in selected {
        assert_eq!(harness.world().entities[id].owner, LOCAL_PLAYER);
    }
}

#[test]
fn a_box_with_only_a_structure_selects_the_structure() {
    let mut harness = Harness::battle();
    let base = harness.local_structure();
    let center = harness.unit_on_screen(base);
    harness.drag(center - Vec2::splat(10.0), center + Vec2::splat(10.0));

    assert_eq!(harness.selection().ids(), [base]);
}

#[test]
fn a_box_over_nothing_clears_the_selection_and_shift_keeps_it() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    let corner = harness.ground_on_screen(harness.near(unit, Vec2::new(-6.0, -6.0)));

    harness.with_key_held(KeyCode::ShiftLeft, |harness| {
        harness.drag(corner, corner + Vec2::splat(20.0));
    });
    assert_eq!(harness.selection().ids(), [unit]);

    harness.drag(corner, corner + Vec2::splat(20.0));
    assert_eq!(harness.selection().ids().len(), 0);
}

#[test]
fn a_box_finishes_on_release_over_the_inspector() {
    let mut harness = Harness::battle();
    let army = harness.local_army();
    let unit = army[0];
    let position = harness.unit_on_screen(unit);
    let start = position + Vec2::new(20.0, 20.0);
    harness.move_cursor(start);
    harness.send_mouse(MouseButton::Left, ButtonState::Pressed);
    harness.move_cursor(position);
    let drawing = harness
        .app
        .world()
        .resource::<SelectionGesture>()
        .drag_rect();
    assert!(drawing.is_some(), "the box shows while dragging");

    // Crossing the panel clamps the box to the viewport edge instead of dropping it.
    harness.move_cursor(Vec2::new(OVER_THE_PANEL.x, position.y));
    harness.send_mouse(MouseButton::Left, ButtonState::Released);

    assert!(harness.selection().ids().contains(&unit));
    let gesture = harness.app.world().resource::<SelectionGesture>();
    assert_eq!(gesture.drag_rect(), None);
}

#[test]
fn losing_focus_abandons_a_box_drag() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    let start = harness.ground_on_screen(harness.near(unit, Vec2::new(-6.0, -6.0)));
    harness.move_cursor(start);
    harness.send_mouse(MouseButton::Left, ButtonState::Pressed);
    harness.move_cursor(start + Vec2::splat(30.0));

    harness
        .app
        .world_mut()
        .query::<&mut Window>()
        .single_mut(harness.app.world_mut())
        .focused = false;
    harness.step(1);
    harness.send_mouse(MouseButton::Left, ButtonState::Released);

    assert_eq!(harness.selection().ids(), [unit], "nothing was committed");
}

#[test]
fn control_groups_store_and_recall_selections_in_every_digit() {
    let mut harness = Harness::battle();
    let army = harness.local_army();
    harness.select(army[0]);
    harness.with_key_held(KeyCode::ControlLeft, |harness| {
        harness.tap_key(KeyCode::Digit1);
    });
    harness.select(army[1]);
    harness.with_key_held(KeyCode::ControlRight, |harness| {
        harness.tap_key(KeyCode::Digit0);
    });

    harness.tap_key(KeyCode::Digit1);
    assert_eq!(harness.selection().ids(), [army[0]]);
    assert!(harness.draws_selected(army[0]));
    assert!(!harness.draws_selected(army[1]));

    harness.tap_key(KeyCode::Digit0);
    assert_eq!(harness.selection().ids(), [army[1]]);

    harness.tap_key(KeyCode::Digit5);
    assert_eq!(harness.selection().ids(), [army[1]], "group 5 is empty");
}

#[test]
fn a_whole_chord_delivered_within_one_frame_still_assigns_the_group() {
    let mut harness = Harness::battle();
    let army = harness.local_army();
    harness.select(army[0]);

    // A slow frame delivers press and release of both keys together.
    let window = harness.window();
    for (key_code, state) in [
        (KeyCode::ControlLeft, ButtonState::Pressed),
        (KeyCode::Digit1, ButtonState::Pressed),
        (KeyCode::Digit1, ButtonState::Released),
        (KeyCode::ControlLeft, ButtonState::Released),
    ] {
        harness.app.world_mut().send_event(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state,
            repeat: false,
            window,
        });
    }
    harness.step(1);

    assert_eq!(harness.selection().group(0), [army[0]]);
}

#[test]
fn keys_work_right_after_clicking_an_inspector_button() {
    let mut harness = Harness::battle();
    let army = harness.local_army();
    harness.select(army[0]);
    harness.with_key_held(KeyCode::ControlLeft, |harness| {
        harness.tap_key(KeyCode::Digit1);
    });
    harness.select(army[1]);

    let pause = harness
        .app
        .world()
        .resource::<client::input_policy::UiGeometry>()
        .targets[client::input_policy::UiTarget::Pause as usize]
        .expect("the battle tab shows a pause button")
        .center();
    harness.click(MouseButton::Left, Vec2::new(pause.x, pause.y));
    harness.step(2);
    assert!(
        !harness.app.world().resource::<LabState>().battle_paused,
        "the click must reach the button"
    );

    harness.move_cursor(Vec2::new(700.0, 100.0));
    harness.tap_key(KeyCode::Digit1);

    assert_eq!(harness.selection().ids(), [army[0]]);
}

#[test]
fn hold_position_key_reaches_the_simulation_without_panning_the_camera() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    harness.resume();
    let camera_before = harness
        .app
        .world_mut()
        .query::<&RtsCamera>()
        .single(harness.app.world())
        .target;

    harness.tap_key(HOLD_POSITION_KEY);
    harness.step(2);

    let orders = &harness.world().entities[&unit].order_queue;
    assert!(
        matches!(orders.front(), Some(UnitOrder::HoldPosition)),
        "the simulation must hold the order: {orders:?}"
    );
    let camera_after = harness
        .app
        .world_mut()
        .query::<&RtsCamera>()
        .single(harness.app.world())
        .target;
    assert_eq!(camera_before, camera_after);
}

#[test]
fn stop_key_halts_a_moving_unit() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    let far = harness.ground_on_screen(harness.near(unit, Vec2::new(5.0, 3.0)));
    harness.click(MouseButton::Right, far);
    harness.resume();
    harness.step(10);
    let moving_from = harness.simulated_position(unit);
    harness.step(5);
    assert_ne!(
        harness.simulated_position(unit),
        moving_from,
        "it was moving"
    );

    harness.tap_key(STOP_KEY);
    harness.step(15);
    let stopped_at = harness.simulated_position(unit);
    harness.step(15);

    assert!(
        harness.simulated_position(unit).distance(stopped_at) < 0.05,
        "the unit must stay put"
    );
}

#[test]
fn order_keys_with_nothing_selected_issue_nothing() {
    let mut harness = Harness::battle();
    harness.tap_key(HOLD_POSITION_KEY);
    harness.tap_key(STOP_KEY);
    assert_eq!(harness.pending_orders(), 0);
    assert_eq!(harness.status(), "");
}

#[test]
fn shift_right_click_queues_a_second_destination() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    let first = harness.open_ground(harness.simulated_position(unit), 3.0);
    let second = harness.open_ground(first, 3.0);
    let first_on_screen = harness.ground_on_screen(first);
    let second_on_screen = harness.ground_on_screen(second);

    harness.click(MouseButton::Right, first_on_screen);
    harness.with_key_held(KeyCode::ShiftLeft, |harness| {
        harness.click(MouseButton::Right, second_on_screen);
    });
    {
        let session = harness.app.world().resource::<BattleSession>();
        let flags: Vec<bool> = session
            .commands
            .pending_commands()
            .map(|(_, command)| command.queue)
            .collect();
        assert_eq!(flags, [false, true]);
    }

    harness.resume();
    harness.step(200);

    assert!(
        harness.simulated_position(unit).distance(second) < 1.0,
        "the unit must finish at the queued destination"
    );
}

#[test]
fn changing_the_controlled_player_drops_the_old_players_selection() {
    let mut harness = Harness::battle();
    let unit = harness.local_army()[0];
    harness.select(unit);
    harness.with_key_held(KeyCode::ControlLeft, |harness| {
        harness.tap_key(KeyCode::Digit1);
    });

    harness
        .app
        .world_mut()
        .resource_mut::<BattlePresentation>()
        .controlled = Some(PlayerId(1));
    harness.step(1);

    assert_eq!(harness.selection().ids().len(), 0);
    assert_eq!(harness.selection().group(0).len(), 0);
}
