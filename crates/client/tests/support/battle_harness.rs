//! The production client without a window, driven by the events a window backend would send.
//!
//! # Responsibility
//! Boots the real [`client::ClientAppPlugin`] on a real scenario, then offers the keyboard, mouse
//! and cursor gestures that integration tests are written in, plus read-only views of the
//! simulation they assert on.
//!
//! # How tests use it
//! Include it with `#[path = "support/battle_harness.rs"]`. Each test binary uses a subset, so the
//! including module allows `dead_code`, as it does for `gpu_run.rs`.
//!
//! # Limits
//! There is no window and no renderer. Bevy's input, egui, camera and scheduling systems all run,
//! but pixels are not drawn and the operating system delivers nothing. Rendered results must be
//! checked by their ECS visibility and material uniforms.
//!
//! # Invariants
//! - `Harness::step` fails the test on the frame a battle failure appears, so no test can pass
//!   across a fatal media or simulation error.
//! - Gestures reach the client only as Bevy events. Only the setup helpers (`look_at`, `resume`,
//!   `pause`, `world_mut`) write state directly, and none of them issues an order.

#![allow(clippy::expect_used, clippy::panic, clippy::cast_precision_loss)]

use std::path::{Path, PathBuf};
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
use client::battle_input::order_marker::{OrderMarker, OrderMarkerKind};
use client::battle_pipeline::session::BattleSession;
use client::battle_selection::BattleSelection;
use client::bubble_shader::{BubbleDomeMaterial, BubbleInstance, FLAG_SELECTED};
use client::indicators::{GroundRingMarker, OverheadBarRootMarker, SelectionRingMaterial};
use client::input_policy::world_to_window;
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use client::scene_controller::LiveBattleBubble;
use client::{LabState, ResolvedAssetsRoot, RtsCamera};
use sim_core::SimWorld;
use sim_map::CellFlags;
use sim_types::{Concealment, EntityId, MoverType, PlayerId, SimScalar, SimVec2, UnitTypeId};

/// Owner of the armies driven by these tests: the scenario's first participant.
pub const LOCAL_PLAYER: PlayerId = PlayerId(0);

/// Camera distance that fits a starting army on screen, in world units.
const OVERVIEW_DISTANCE: f32 = 26.0;

/// A production client with no window or renderer, driven by real input events.
pub struct Harness {
    /// The running application, for assertions on any of its resources.
    pub app: App,
}

pub fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

impl Harness {
    /// Boots the production client on the default battle, paused.
    pub fn boot(mode: client::ClientMode) -> Self {
        Self::boot_scenario(mode, DEFAULT_BATTLE_SCENARIO)
    }

    /// Boots the production client on `scenario`, a path beneath the assets root, paused.
    pub fn boot_scenario(mode: client::ClientMode, scenario: &str) -> Self {
        Self::boot_root(&assets(), mode, scenario)
    }

    /// Boots the production client on `scenario`, a path beneath the assets root `directory`.
    ///
    /// The real loader reads `directory`, so a temporary root is held to the same rules as the
    /// shipped one.
    pub fn boot_root(directory: &Path, mode: client::ClientMode, scenario: &str) -> Self {
        let root = ResolvedAssetsRoot::from_directory(directory).expect("assets root");
        let definition =
            BattleDefinition::load(root.as_path(), scenario.as_ref()).expect("battle scenario");
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

    /// Boots the default battle with the local army in view.
    pub fn battle() -> Self {
        let mut harness = Self::boot(client::ClientMode::LiveBattle);
        harness.frame_army();
        harness
    }

    pub fn step(&mut self, frames: usize) {
        for _ in 0..frames {
            self.app.update();
            let lab = self.app.world().resource::<LabState>();
            assert_eq!(lab.battle_failure, None, "the battle must keep running");
        }
    }

    pub fn resume(&mut self) {
        self.app
            .world_mut()
            .resource_mut::<LabState>()
            .battle_paused = false;
    }

    pub fn pause(&mut self) {
        self.app
            .world_mut()
            .resource_mut::<LabState>()
            .battle_paused = true;
        self.step(2);
    }

    pub fn world(&self) -> &SimWorld {
        self.app
            .world()
            .resource::<BattleSession>()
            .world()
            .expect("committed battle")
    }

    pub fn world_mut(&mut self) -> &mut SimWorld {
        self.app
            .world_mut()
            .resource_mut::<BattleSession>()
            .into_inner()
            .world_mut()
            .expect("committed battle")
    }

    /// A presentable burrowing unit type and its burrowed mode's name, see [`burrower`].
    pub fn burrower(&mut self) -> (UnitTypeId, String) {
        burrower(self.world_mut(), &assets())
    }

    /// A shipped mobile detector.
    pub fn detector(&self) -> UnitTypeId {
        self.world()
            .catalog
            .archetypes()
            .find(|arch| arch.is_detector && arch.max_speed > SimScalar::ZERO)
            .expect("the shipped catalog has a mobile detector")
            .unit_type_id
    }

    /// Spawns a [`Self::burrower`] of `owner` at `place`, hidden as `concealment`, and returns it.
    ///
    /// Burrowed, it stands in its burrowed mode as the end of the mode's channel leaves it, but
    /// without the mode's weapon, so it cannot fight and change what is on screen. Cloaked, it
    /// keeps its default stance, which has no weapon either.
    pub fn spawn_concealed(
        &mut self,
        owner: PlayerId,
        place: Vec2,
        concealment: Concealment,
    ) -> EntityId {
        let (unit_type, mode) = self.burrower();
        let world = self.world_mut();
        let id = world
            .spawn_unit(unit_type, owner, sim_position(place))
            .expect("open ground");
        let unit = world.entities.get_mut(&id).expect("just spawned");
        match concealment {
            Concealment::None => {}
            Concealment::Cloaked => unit.is_cloaked = true,
            Concealment::Burrowed => {
                unit.is_cloaked = true;
                unit.active_mode = Some(mode);
                unit.is_immobile = true;
                unit.unpushable = true;
            }
        }
        id
    }

    /// The concealment the bubble of unit `id` was last synchronized with.
    pub fn published_concealment(&mut self, id: EntityId) -> Concealment {
        let world = self.app.world_mut();
        world
            .query_filtered::<&BubbleInstance, With<LiveBattleBubble>>()
            .iter(world)
            .find(|bubble| bubble.0.entity_id == id)
            .map(|bubble| bubble.0.concealment)
            .expect("unit must have a bubble")
    }

    pub fn selection(&self) -> &BattleSelection {
        self.app.world().resource::<BattleSelection>()
    }

    pub fn window(&mut self) -> Entity {
        self.app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(self.app.world())
    }

    /// Mobile armed units of the local player, ordered by ID.
    pub fn local_army(&self) -> Vec<EntityId> {
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

    pub fn enemy_units(&self) -> Vec<EntityId> {
        self.world()
            .entities
            .values()
            .filter(|unit| unit.owner != LOCAL_PLAYER && !unit.is_immobile)
            .map(|unit| unit.id)
            .collect()
    }

    pub fn local_structure(&self) -> EntityId {
        self.world()
            .entities
            .values()
            .find(|unit| unit.owner == LOCAL_PLAYER && unit.is_immobile)
            .expect("the scenario gives each player a base")
            .id
    }

    /// Points the camera at the local army, from high enough to see all of it.
    pub fn frame_army(&mut self) {
        let army = self.local_army();
        let center = army
            .iter()
            .map(|id| self.simulated_position(*id))
            .sum::<Vec2>()
            / army.len() as f32;
        self.look_at(center);
    }

    pub fn look_at(&mut self, target: Vec2) {
        let world = self.app.world_mut();
        let mut cameras = world.query::<&mut RtsCamera>();
        let mut camera = cameras.single_mut(world);
        camera.target = target;
        camera.distance = OVERVIEW_DISTANCE;
        // The look height eases over several frames, and a test should not start mid-ease.
        self.step(40);
    }

    /// Points the camera at unit `id`.
    pub fn look_at_unit(&mut self, id: EntityId) {
        let position = self.simulated_position(id);
        self.look_at(position);
    }

    /// Current simulated position, in world units on the map plane.
    pub fn simulated_position(&self, id: EntityId) -> Vec2 {
        let position = self.world().entities[&id].position;
        Vec2::new(position.x.to_f32(), position.y.to_f32())
    }

    /// Window position showing world point `point`.
    pub fn screen_position(&mut self, point: Vec3) -> Vec2 {
        let world = self.app.world_mut();
        let (camera, transform) = world
            .query_filtered::<(&Camera, &GlobalTransform), With<RtsCamera>>()
            .single(world);
        world_to_window(camera, transform, point).expect("point must be in front of the camera")
    }

    /// Window position showing the center of unit `id`'s bubble.
    pub fn unit_on_screen(&mut self, id: EntityId) -> Vec2 {
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
    pub fn ground_on_screen(&mut self, position: Vec2) -> Vec2 {
        let altitude = self
            .world()
            .map
            .terrain_altitude_scalar(sim_position(position))
            .to_f32();
        self.screen_position(Vec3::new(position.x, altitude, position.y))
    }

    pub fn move_cursor(&mut self, position: Vec2) {
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

    pub fn send_mouse(&mut self, button: MouseButton, state: ButtonState) {
        let window = self.window();
        self.app.world_mut().send_event(MouseButtonInput {
            button,
            state,
            window,
        });
        self.step(1);
    }

    pub fn click(&mut self, button: MouseButton, position: Vec2) {
        self.move_cursor(position);
        self.send_mouse(button, ButtonState::Pressed);
        self.send_mouse(button, ButtonState::Released);
    }

    pub fn drag(&mut self, from: Vec2, to: Vec2) {
        self.move_cursor(from);
        self.send_mouse(MouseButton::Left, ButtonState::Pressed);
        self.move_cursor((from + to) * 0.5);
        self.move_cursor(to);
        self.send_mouse(MouseButton::Left, ButtonState::Released);
    }

    pub fn send_key(&mut self, key_code: KeyCode, state: ButtonState) {
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

    pub fn tap_key(&mut self, key: KeyCode) {
        self.send_key(key, ButtonState::Pressed);
        self.send_key(key, ButtonState::Released);
    }

    pub fn with_key_held(&mut self, modifier: KeyCode, action: impl FnOnce(&mut Self)) {
        self.send_key(modifier, ButtonState::Pressed);
        action(self);
        self.send_key(modifier, ButtonState::Released);
    }

    /// Left-clicks unit `id` and returns the window position clicked.
    pub fn select(&mut self, id: EntityId) -> Vec2 {
        let position = self.unit_on_screen(id);
        self.click(MouseButton::Left, position);
        position
    }

    /// A map position `offset` from unit `id`.
    pub fn near(&self, id: EntityId, offset: Vec2) -> Vec2 {
        self.simulated_position(id) + offset
    }

    /// Walkable open ground `distance` world units from `origin`, clear of structures.
    ///
    /// The simulation redirects a destination inside a structure to the nearest free point, so a
    /// test of arrival needs one that is not.
    pub fn open_ground(&self, origin: Vec2, distance: f32) -> Vec2 {
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

    pub fn pending_orders(&self) -> usize {
        self.app
            .world()
            .resource::<BattleSession>()
            .commands
            .pending_len()
    }

    pub fn status(&self) -> String {
        self.app
            .world()
            .resource::<BattleSession>()
            .commands
            .status()
            .to_owned()
    }

    pub fn marker(&mut self) -> Option<(OrderMarkerKind, Vec3)> {
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
    pub fn draws_selected(&mut self, id: EntityId) -> bool {
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

pub fn sim_position(position: Vec2) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(position.x),
        SimScalar::from_num(position.y),
    )
}

// A presentable, unarmed unit type with a burrowed mode, for tests of what a concealed unit shows.
//
// The shipped burrowing unit (the Lurker analogue) has no media yet, and the client refuses to
// present a unit without media. Until it has, `burrower` takes a presentable mobile ground
// archetype that nothing on the field uses, removes its weapons in the session's catalog and
// gives it a copy of the shipped burrowed mode without the mode's weapon. Only the test world's
// catalog changes, never a file; once the shipped unit is presentable and unarmed by default, it
// is used unchanged, as `battle_mode_tests.rs` does for the same unit. Every choice is made by
// catalog properties, so no catalog identity is named.

/// Whether the client can load the media of the unit `slug`: its flipbook manifest ships.
pub fn presentable(assets: &Path, slug: &str) -> bool {
    assets
        .join("textures")
        .join(format!("{slug}.flipbook.ron"))
        .is_file()
}

/// A presentable, unarmed unit type with a burrowed mode in `world`'s catalog, and that mode's
/// name. `assets` is the shipped asset root.
pub fn burrower(world: &mut SimWorld, assets: &Path) -> (UnitTypeId, String) {
    let (shipped, mode) = world
        .catalog
        .archetypes()
        .find_map(|arch| {
            let mode = arch.modes.iter().find(|mode| mode.is_burrowed)?;
            Some((arch.clone(), mode.clone()))
        })
        .expect("the shipped catalog has a burrowing unit");
    let unarmed =
        shipped.weapon.is_none() && shipped.ground_weapon.is_none() && shipped.air_weapon.is_none();
    if unarmed && presentable(assets, &shipped.slug) {
        return (shipped.unit_type_id, mode.mode_name);
    }
    let in_use = |id: UnitTypeId| world.entities.values().any(|unit| unit.unit_type_id == id);
    let stand_in = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.max_speed > SimScalar::ZERO
                && arch.mover_type == MoverType::Ground
                && arch.modes.is_empty()
                && !in_use(arch.unit_type_id)
                && presentable(assets, &arch.slug)
        })
        .expect("the catalog ships a presentable ground unit nothing on the field uses")
        .unit_type_id;
    let archetype = world
        .catalog
        .get_mut(stand_in)
        .expect("invariant: the type was just found in this catalog");
    archetype.weapon = None;
    archetype.ground_weapon = None;
    archetype.air_weapon = None;
    let mut burrowed = mode.clone();
    burrowed.ground_weapon_override = None;
    archetype.modes.push(burrowed);
    (stand_in, mode.mode_name)
}
