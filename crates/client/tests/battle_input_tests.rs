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

use std::time::Duration;

use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::window::PrimaryWindow;
use bevy::winit::WinitPlugin;
use client::battle_input::gesture::SelectionGesture;
use client::battle_input::order_marker::{OrderMarker, OrderMarkerKind};
use client::battle_pipeline::session::BattleSession;
use client::battle_presentation::BattlePresentation;
use client::battle_selection::BattleSelection;
use client::bubble_shader::{BubbleDomeMaterial, BubbleInstance, FLAG_SELECTED};
use client::indicators::{GroundRingMarker, OverheadBarRootMarker, SelectionRingMaterial};
use client::input_bindings::{HOLD_POSITION_KEY, STOP_KEY};
use client::input_policy::world_to_window;
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use client::scene_controller::{LiveBattleBubble, SwarmBubble};
use client::{LabState, LabTab, ResolvedAssetsRoot, RtsCamera, SelectionState};
use sim_core::SimWorld;
use sim_map::CellFlags;
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2, UnitOrder};

/// Owner of the armies driven by these tests: the scenario's first participant.
const LOCAL_PLAYER: PlayerId = PlayerId(0);

/// A window position over the inspector panel.
const OVER_THE_PANEL: Vec2 = Vec2::new(100.0, 300.0);

/// Camera distance that fits a starting army on screen, in world units.
const OVERVIEW_DISTANCE: f32 = 26.0;

struct Harness {
    app: App,
}

fn assets() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

impl Harness {
    /// Boots the production client on the default battle, paused, with the army in view.
    fn boot(mode: client::ClientMode) -> Self {
        let root = ResolvedAssetsRoot::from_directory(&assets()).expect("assets root");
        let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())
            .expect("default battle");
        let mut app = App::new();
        app.insert_resource(client::CatalogResource(definition.catalogs.units.clone()))
            .insert_resource(definition.palette.clone())
            .insert_resource(definition)
            .insert_resource(root.clone())
            .add_plugins(
                client::presentation::GamePresentationPlugins
                    .set(RenderPlugin {
                        render_creation: RenderCreation::Automatic(WgpuSettings {
                            backends: None,
                            ..default()
                        }),
                        ..default()
                    })
                    .set(AssetPlugin {
                        file_path: root.bevy_file_path().expect("asset path").into(),
                        ..default()
                    })
                    .set(WindowPlugin {
                        primary_window: Some(Window {
                            resolution: (1280.0_f32, 720.0_f32).into(),
                            ..default()
                        }),
                        ..default()
                    })
                    .disable::<WinitPlugin>()
                    .disable::<bevy::log::LogPlugin>(),
            )
            .add_plugins(bevy_egui::EguiPlugin)
            .add_plugins(client::ClientAppPlugin {
                mode,
                max_frames: None,
                screenshot_out: None,
            })
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                40,
            )));
        app.finish();
        app.cleanup();
        app.world_mut().resource_mut::<LabState>().battle_paused = true;
        let mut harness = Self { app };
        harness.step(4);
        harness
    }

    fn battle() -> Self {
        let mut harness = Self::boot(client::ClientMode::LiveBattle);
        harness.frame_army();
        harness
    }

    fn step(&mut self, frames: usize) {
        for _ in 0..frames {
            self.app.update();
            let lab = self.app.world().resource::<LabState>();
            assert_eq!(lab.battle_failure, None, "the battle must keep running");
        }
    }

    fn resume(&mut self) {
        self.app
            .world_mut()
            .resource_mut::<LabState>()
            .battle_paused = false;
    }

    fn world(&self) -> &SimWorld {
        self.app
            .world()
            .resource::<BattleSession>()
            .world()
            .expect("committed battle")
    }

    fn world_mut(&mut self) -> &mut SimWorld {
        self.app
            .world_mut()
            .resource_mut::<BattleSession>()
            .into_inner()
            .world_mut()
            .expect("committed battle")
    }

    fn selection(&self) -> &BattleSelection {
        self.app.world().resource::<BattleSelection>()
    }

    fn window(&mut self) -> Entity {
        self.app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(self.app.world())
    }

    /// Mobile armed units of the local player, ordered by ID.
    fn local_army(&self) -> Vec<EntityId> {
        let world = self.world();
        world
            .entities
            .values()
            .filter(|unit| {
                let armed = world.catalog.get(unit.unit_type_id).is_ok_and(|archetype| {
                    archetype.weapon.is_some() || archetype.ground_weapon.is_some()
                });
                unit.owner == LOCAL_PLAYER && !unit.is_immobile && armed
            })
            .map(|unit| unit.id)
            .collect()
    }

    fn enemy_units(&self) -> Vec<EntityId> {
        self.world()
            .entities
            .values()
            .filter(|unit| unit.owner != LOCAL_PLAYER && !unit.is_immobile)
            .map(|unit| unit.id)
            .collect()
    }

    fn local_structure(&self) -> EntityId {
        self.world()
            .entities
            .values()
            .find(|unit| unit.owner == LOCAL_PLAYER && unit.is_immobile)
            .expect("the scenario gives each player a base")
            .id
    }

    /// Points the camera at the local army, from high enough to see all of it.
    fn frame_army(&mut self) {
        let army = self.local_army();
        let center = army
            .iter()
            .map(|id| self.simulated_position(*id))
            .sum::<Vec2>()
            / army.len() as f32;
        self.look_at(center);
    }

    fn look_at(&mut self, target: Vec2) {
        let world = self.app.world_mut();
        let mut cameras = world.query::<&mut RtsCamera>();
        let mut camera = cameras.single_mut(world);
        camera.target = target;
        camera.distance = OVERVIEW_DISTANCE;
        // The look height eases over several frames, and a test should not start mid-ease.
        self.step(40);
    }

    /// Current simulated position, in world units on the map plane.
    fn simulated_position(&self, id: EntityId) -> Vec2 {
        let position = self.world().entities[&id].position;
        Vec2::new(position.x.to_f32(), position.y.to_f32())
    }

    /// Window position showing world point `point`.
    fn screen_position(&mut self, point: Vec3) -> Vec2 {
        let world = self.app.world_mut();
        let (camera, transform) = world
            .query_filtered::<(&Camera, &GlobalTransform), With<RtsCamera>>()
            .single(world);
        world_to_window(camera, transform, point).expect("point must be in front of the camera")
    }

    /// Window position showing the center of unit `id`'s bubble.
    fn unit_on_screen(&mut self, id: EntityId) -> Vec2 {
        let world = self.app.world_mut();
        let point = world
            .query::<(&GlobalTransform, &BubbleInstance)>()
            .iter(world)
            .find(|(_, bubble)| bubble.0.entity_id == id)
            .map(|(transform, _)| transform.translation())
            .expect("unit must have a bubble");
        self.screen_position(point)
    }

    /// Window position showing the ground at map position `position`.
    fn ground_on_screen(&mut self, position: Vec2) -> Vec2 {
        let altitude = self
            .world()
            .map
            .terrain_altitude_scalar(sim_position(position))
            .to_f32();
        self.screen_position(Vec3::new(position.x, altitude, position.y))
    }

    fn move_cursor(&mut self, position: Vec2) {
        let window = self.window();
        self.app
            .world_mut()
            .query::<&mut Window>()
            .single_mut(self.app.world_mut())
            .set_cursor_position(Some(position));
        self.app.world_mut().send_event(CursorMoved {
            window,
            position,
            delta: None,
        });
        self.step(1);
    }

    fn send_mouse(&mut self, button: MouseButton, state: ButtonState) {
        let window = self.window();
        self.app.world_mut().send_event(MouseButtonInput {
            button,
            state,
            window,
        });
        self.step(1);
    }

    fn click(&mut self, button: MouseButton, position: Vec2) {
        self.move_cursor(position);
        self.send_mouse(button, ButtonState::Pressed);
        self.send_mouse(button, ButtonState::Released);
    }

    fn drag(&mut self, from: Vec2, to: Vec2) {
        self.move_cursor(from);
        self.send_mouse(MouseButton::Left, ButtonState::Pressed);
        self.move_cursor((from + to) * 0.5);
        self.move_cursor(to);
        self.send_mouse(MouseButton::Left, ButtonState::Released);
    }

    fn send_key(&mut self, key_code: KeyCode, state: ButtonState) {
        let window = self.window();
        self.app.world_mut().send_event(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state,
            repeat: false,
            window,
        });
        self.step(1);
    }

    fn tap_key(&mut self, key: KeyCode) {
        self.send_key(key, ButtonState::Pressed);
        self.send_key(key, ButtonState::Released);
    }

    fn with_key_held(&mut self, modifier: KeyCode, action: impl FnOnce(&mut Self)) {
        self.send_key(modifier, ButtonState::Pressed);
        action(self);
        self.send_key(modifier, ButtonState::Released);
    }

    /// Left-clicks unit `id` and returns the window position clicked.
    fn select(&mut self, id: EntityId) -> Vec2 {
        let position = self.unit_on_screen(id);
        self.click(MouseButton::Left, position);
        position
    }

    /// A map position `offset` from unit `id`.
    fn near(&self, id: EntityId, offset: Vec2) -> Vec2 {
        self.simulated_position(id) + offset
    }

    /// Walkable open ground `distance` world units from `origin`, clear of structures.
    ///
    /// The simulation redirects a destination inside a structure to the nearest free point, so a
    /// test of arrival needs one that is not.
    fn open_ground(&self, origin: Vec2, distance: f32) -> Vec2 {
        let map = &self.world().map;
        let open = |point: Vec2| {
            (-1..=1).all(|x: i32| {
                (-1..=1).all(|y: i32| {
                    let probe = point + Vec2::new(x as f32, y as f32);
                    map.world_to_cell(sim_position(probe))
                        .ok()
                        .and_then(|(cx, cy)| map.cell(cx, cy).ok())
                        .is_some_and(|cell| {
                            cell.flags.contains(CellFlags::GROUND_PATHABLE)
                                && !cell.flags.intersects(
                                    CellFlags::OCCUPIED_BY_STRUCTURE | CellFlags::UNPASSABLE_CLIFF,
                                )
                        })
                })
            })
        };
        (0..16)
            .map(|step| {
                origin + Vec2::from_angle(step as f32 * std::f32::consts::TAU / 16.0) * distance
            })
            .find(|point| open(*point))
            .expect("the start area must have open ground")
    }

    fn pending_orders(&self) -> usize {
        self.app
            .world()
            .resource::<BattleSession>()
            .commands
            .pending_len()
    }

    fn status(&self) -> String {
        self.app
            .world()
            .resource::<BattleSession>()
            .commands
            .status()
            .to_owned()
    }

    fn marker(&mut self) -> Option<(OrderMarkerKind, Vec3)> {
        let world = self.app.world_mut();
        world
            .query::<(&OrderMarker, &Transform, &Visibility)>()
            .iter(world)
            .find(|(marker, _, visibility)| {
                marker.remaining > 0.0 && **visibility != Visibility::Hidden
            })
            .map(|(marker, transform, _)| (marker.kind, transform.translation))
    }

    /// Whether unit `id` draws as selected: instance, bubble material, ring and overhead bar.
    fn draws_selected(&mut self, id: EntityId) -> bool {
        let world = self.app.world_mut();
        let (bubble, material, children) = world
            .query_filtered::<(
                &BubbleInstance,
                &MeshMaterial3d<BubbleDomeMaterial>,
                &Children,
            ), With<LiveBattleBubble>>()
            .iter(world)
            .find(|(bubble, _, _)| bubble.0.entity_id == id)
            .map(|(bubble, material, children)| {
                (bubble.0.selected, material.0.clone(), children.to_vec())
            })
            .expect("unit must have a bubble");
        let uniform = world
            .resource::<Assets<BubbleDomeMaterial>>()
            .get(&material)
            .expect("bubble material")
            .uniform;
        let shown = |visibility: Option<&Visibility>| {
            visibility.is_some_and(|visibility| *visibility != Visibility::Hidden)
        };
        let ring = children
            .iter()
            .find(|child| world.get::<GroundRingMarker>(**child).is_some())
            .expect("ring child");
        let bar = children
            .iter()
            .find(|child| world.get::<OverheadBarRootMarker>(**child).is_some())
            .expect("overhead bar child");
        let ring_material = world
            .get::<MeshMaterial3d<SelectionRingMaterial>>(*ring)
            .expect("ring material")
            .0
            .clone();
        let ring_uniform = world
            .resource::<Assets<SelectionRingMaterial>>()
            .get(&ring_material)
            .expect("ring material asset")
            .uniform;
        let parts = [
            bubble,
            uniform.selected == 1,
            uniform.feature_flags & FLAG_SELECTED != 0,
            shown(world.get::<Visibility>(*ring)),
            ring_uniform.is_selected == 1,
            shown(world.get::<Visibility>(*bar)),
        ];
        assert!(
            parts.iter().all(|part| *part) || parts.iter().all(|part| !part),
            "instance, material, ring and bar must agree: {parts:?}"
        );
        bubble
    }
}

fn sim_position(position: Vec2) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(position.x),
        SimScalar::from_num(position.y),
    )
}

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
