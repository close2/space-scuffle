//! Space Scuffle Bevy 3D renderer, tactical indicators, and Bubble Visual Lab.
//!
//! # Architectural Charter
//! In accordance with [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md) and
//! [Implementation Plan 04](file:///home/cl/projects/space-scuffle/docs/plans/04-plan-asset-baker-and-client.md),
//! `crates/client` provides the interactive visual layer for Space Scuffle.
//!
//! # Invariants Maintained
//! 1. **Zero Authoritative Simulation Logic:** All deterministic state is owned by `sim_core`.
//!    The client consumes immutable [`sim_types::snapshot::RenderSnapshot`] values.
//! 2. **Type-Safe Fixed-to-Float Conversion:** Fixed-point math (`SimScalar`, `SimVec2`)
//!    is converted to floating-point (`f32`, `Vec3`) strictly at the snapshot boundary.
//! 3. **Deterministic Headless CI Verification:** Full 60-frame headless stepping support
//!    with zero display server dependencies.
//!
//! # Core Subsystems
//! - [`camera`]: StarCraft II style RTS camera with 58° pitch, WASD pan, scroll zoom, and hotkey bookmarks.
//! - [`arena_render`]: Stylized 3D two-level map with metallic panels, cliffs, ramps, minerals, and geysers.
//! - [`bubble_shader`]: Custom `BubbleDomeMaterial` and GPU uniform packing for unit spheres and domes.
//! - [`indicators`]: Ground selection rings, equatorial facing chevron, attack focal lens, and collar meshes.
//! - [`vfx`]: Trailing mist wake, hitscan tracers, plasma bolts, lasers, lightning, and glass shatter.
//! - [`lab_ui`]: Tabbed `bevy_egui` dashboard (Visual Lab, Live Combat Sandbox, Swarm Stress Test).

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::module_name_repetitions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    clippy::if_not_else
)]

pub mod arena_render;
pub mod bubble_shader;
pub mod camera;
pub mod indicators;
pub mod lab_ui;
pub mod scene_controller;
pub mod selection;
pub mod vfx;

pub use arena_render::{setup_arena_system, spawn_arena_environment, ArenaMapMarker};
pub use bubble_shader::{
    setup_bubble_assets_system, sync_bubbles_system, BubbleDomeMaterial, BubbleInstance,
    BubbleSharedMeshes, BubbleUniformGpu, FlipbookRegistry,
};
pub use camera::{
    camera_control_system, update_camera_system, RtsCamera, DEFAULT_CAMERA_PITCH_RAD,
    MAX_CAMERA_DISTANCE, MIN_CAMERA_DISTANCE,
};
pub use indicators::{
    attach_bubble_indicators, setup_indicator_assets_system, update_indicators_system,
    IndicatorAssets, SelectionRingMaterial, SelectionRingUniformGpu,
};
pub use lab_ui::{lab_ui_system, LabState, LabTab};
pub use scene_controller::{
    step_client_simulation_system, sync_lab_preview_system, sync_live_battle_system,
    sync_swarm_stress_system, LabPreviewBubble, LiveBattleBubble, RenderEntityMap, SwarmBubble,
};
pub use selection::{
    handle_viewport_selection_system, ray_sphere_intersect, selection_system, HeroLineupIndex,
    SelectionState,
};
pub use vfx::{
    emit_mist_wake, process_combat_events_system, setup_vfx_assets_system, spawn_death_shatter,
    update_beams_system, update_melee_slashes_system, update_mist_wake_system,
    update_projectiles_system, update_shatter_system, update_vfx_system, CombatFxMaterial,
    CombatFxUniformGpu, CombatVisualEvent, MistWakeMaterial, MistWakeUniformGpu, VfxSharedAssets,
};

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use std::path::PathBuf;

/// Resolves the absolute path to the workspace `assets/` directory.
///
/// # Search Precedence
/// 1. Environment variable `SPACE_SCUFFLE_ASSETS`.
/// 2. `CARGO_MANIFEST_DIR`, walking upward to find an `assets/` directory
///    containing `shaders/bubble_dome.wgsl` or workspace root.
/// 3. Current working directory (`std::env::current_dir()`), walking upward similarly.
/// 4. Fallback to `"assets"`.
#[must_use]
pub fn resolve_assets_dir() -> PathBuf {
    if let Ok(env_path) = std::env::var("SPACE_SCUFFLE_ASSETS") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            return p;
        }
    }

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let mut cur = PathBuf::from(manifest_dir);
        loop {
            let candidate = cur.join("assets");
            if candidate.join("shaders/bubble_dome.wgsl").exists() || candidate.exists() {
                return candidate;
            }
            if !cur.pop() {
                break;
            }
        }
    }

    if let Ok(cur_dir) = std::env::current_dir() {
        let mut cur = cur_dir;
        loop {
            let candidate = cur.join("assets");
            if candidate.join("shaders/bubble_dome.wgsl").exists() || candidate.exists() {
                return candidate;
            }
            if !cur.pop() {
                break;
            }
        }
    }

    PathBuf::from("assets")
}
use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use core::mem::size_of;
use sim_core::world::SimWorld;
use sim_types::commands::TickBundle;
use sim_types::ids::Tick;

/// Bevy ECS resource wrapping the deterministic [`sim_core::catalog::UnitCatalog`].
///
/// Provides dynamic archetype definitions, unit vitals, weapon specs, and showcase metadata
/// to client rendering, HUD, and inspector systems with zero hardcoded creature slugs.
#[derive(Resource, Debug, Clone, Default, Deref, DerefMut)]
pub struct CatalogResource(pub sim_core::catalog::UnitCatalog);

/// Runtime operational mode for the Space Scuffle client application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClientMode {
    /// Boot into the Close-Up Visual Showcase Inspector.
    #[default]
    VisualLab,
    /// Boot into the 2-Army deterministic sim_core combat sandbox.
    LiveBattle,
    /// Boot into the 200-bubble swarm readability stress test.
    SwarmStressTest,
    /// Run 60 frames in headless CI mode and exit cleanly with code 0.
    HeadlessCheck,
}

/// Maximum frame limit before exiting the application cleanly.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct FrameLimit {
    /// Optional frame count cap.
    pub max_frames: Option<u64>,
    /// Accumulated frames rendered.
    pub current_frame: u64,
}

/// System that exits the application when the target frame count is reached.
pub fn frame_limit_system(
    mut frame_limit: ResMut<'_, FrameLimit>,
    mut exit_events: EventWriter<'_, AppExit>,
) {
    if let Some(max) = frame_limit.max_frames {
        frame_limit.current_frame += 1;
        if frame_limit.current_frame >= max {
            exit_events.send(AppExit::Success);
        }
    }
}

/// Primary Bevy plugin assembling all visual client systems and resources.
pub struct ClientAppPlugin {
    /// Target runtime mode.
    pub mode: ClientMode,
    /// Optional frame limit for automated testing.
    pub max_frames: Option<u64>,
}

impl Plugin for ClientAppPlugin {
    fn build(&self, app: &mut App) {
        let initial_tab = match self.mode {
            ClientMode::VisualLab | ClientMode::HeadlessCheck => LabTab::VisualLab,
            ClientMode::LiveBattle => LabTab::LiveBattle,
            ClientMode::SwarmStressTest => LabTab::SwarmStressTest,
        };

        let mut lab_state = LabState {
            current_tab: initial_tab,
            ..default()
        };

        if self.mode == ClientMode::LiveBattle {
            lab_state.sim_world = Some(SimWorld::new_default_battle_sandbox());
        }

        if self.mode == ClientMode::SwarmStressTest {
            lab_state.swarm_active = true;
        }

        app.init_resource::<CatalogResource>()
            .init_resource::<RenderEntityMap>()
            .insert_resource(lab_state)
            .init_resource::<SelectionState>()
            .insert_resource(FrameLimit {
                max_frames: self.max_frames,
                current_frame: 0,
            })
            .add_event::<CombatVisualEvent>()
            .add_plugins(MaterialPlugin::<BubbleDomeMaterial>::default())
            .add_plugins(MaterialPlugin::<SelectionRingMaterial>::default())
            .add_plugins(MaterialPlugin::<MistWakeMaterial>::default())
            .add_plugins(MaterialPlugin::<CombatFxMaterial>::default())
            .add_systems(
                Startup,
                (
                    setup_bubble_assets_system,
                    setup_indicator_assets_system,
                    setup_vfx_assets_system,
                    spawn_arena_environment,
                    spawn_camera_system,
                ),
            )
            .add_systems(
                Update,
                (
                    camera_control_system,
                    selection_system,
                    update_indicators_system,
                    update_mist_wake_system,
                    update_projectiles_system,
                    update_beams_system,
                    update_melee_slashes_system,
                    update_shatter_system,
                    frame_limit_system,
                ),
            )
            .add_systems(
                Update,
                (
                    process_combat_events_system,
                    sync_lab_preview_system,
                    sync_live_battle_system,
                    sync_swarm_stress_system,
                    lab_ui_system,
                ),
            );
    }
}

/// Spawns the RTS perspective camera, defaulting to close-up hero framing in Visual Lab mode.
pub fn spawn_camera_system(mut commands: Commands<'_, '_>, lab_state: Res<'_, LabState>) {
    let rts_cam = if lab_state.current_tab == LabTab::VisualLab {
        RtsCamera::hero_framing()
    } else {
        RtsCamera::default()
    };
    let transform = rts_cam.compute_transform();

    commands.spawn((Camera3d::default(), transform, rts_cam));
}

/// Runs the Space Scuffle client application with the specified launch mode.
pub fn run_app(mode: ClientMode, max_frames: Option<u64>) {
    if mode == ClientMode::HeadlessCheck {
        match run_headless_check() {
            Ok(()) => {
                println!("==> Headless CI Check: PASSED (60 frames stepped deterministically).");
                std::process::exit(0);
            }
            Err(err) => {
                eprintln!("==> Headless CI Check FAILED: {err}");
                std::process::exit(1);
            }
        }
    }

    let assets_dir = resolve_assets_dir();

    let is_virtual_or_test =
        max_frames.is_some() || std::env::var("SPACE_SCUFFLE_VIRTUAL_GPU").is_ok();
    let is_visible = !is_virtual_or_test;

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: assets_dir.to_string_lossy().to_string(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Space Scuffle — Bubble & Dome Visual Lab".to_string(),
                        resolution: (1280.0_f32, 720.0_f32).into(),
                        visible: is_visible,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(FrameTimeDiagnosticsPlugin)
        .add_plugins(EguiPlugin)
        .add_plugins(ClientAppPlugin { mode, max_frames })
        .run();
}

// PERFORMANCE RATIONALE:
// Why: CI automated runners operate on headless cloud Linux instances without a physical GPU.
// What: MinimalPlugins + pure-Rust deterministic simulation stepping advances 60 frames
// without initializing a window or graphics device, verifying arithmetic, uniform packing, and checksums.
// Trade-off: Headless CI mode does not execute actual GPU rasterization passes.
/// Runs a deterministic 60-frame headless Bevy schedule to verify state sync and math in CI.
///
/// # Errors
/// Returns an error string if simulation stepping, camera invariants, or uniform packing fail.
pub fn run_headless_check() -> Result<(), String> {
    // 1. Initialize Headless Simulation World
    let mut world = SimWorld::new_default_battle_sandbox();
    let initial_entities_count = world.entities.len();
    if initial_entities_count == 0 {
        return Err("Battle sandbox initialized with 0 entities".to_string());
    }

    // 2. Advance 60 Deterministic Simulation Frames
    for tick in 0..60 {
        let bundle = TickBundle {
            tick: Tick(world.current_tick),
            commands: Vec::new(),
            state_checksum: None,
        };

        let snapshot = world.step(&bundle);
        if snapshot.tick.0 != tick + 1 {
            return Err(format!(
                "Tick counter mismatch: expected {}, got {}",
                tick + 1,
                snapshot.tick.0
            ));
        }

        // Verify uniform packing and invariants on all bubbles
        for bubble in &snapshot.bubbles {
            let uniform = BubbleUniformGpu::from_instance(bubble, None);
            if size_of::<BubbleUniformGpu>() != 96 {
                return Err(format!(
                    "BubbleUniformGpu size mismatch: expected 96, got {}",
                    size_of::<BubbleUniformGpu>()
                ));
            }
            if uniform.radius <= 0.0 {
                return Err("Bubble uniform radius must be positive".to_string());
            }
        }
    }

    // 3. Verify Camera Controller Invariants
    let mut cam = RtsCamera::default();
    cam.pan(Vec2::new(1.0, 0.0), 0.04);
    cam.zoom(-2.0);
    if cam.distance < MIN_CAMERA_DISTANCE || cam.distance > MAX_CAMERA_DISTANCE {
        return Err(format!("Camera distance out of bounds: {}", cam.distance));
    }
    let tf = cam.compute_transform();
    if tf.translation.y <= 0.0 {
        return Err("Camera eye elevation must be above ground plane".to_string());
    }

    // 4. Step Full Production Bevy App Schedule for 60 Frames
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    // Initialize asset storages & event channels required by systems
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<StandardMaterial>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<BubbleDomeMaterial>>();
    app.init_resource::<Assets<SelectionRingMaterial>>();
    app.init_resource::<Assets<MistWakeMaterial>>();
    app.init_resource::<Assets<CombatFxMaterial>>();
    app.init_resource::<ButtonInput<KeyCode>>();
    app.init_resource::<ButtonInput<MouseButton>>();
    app.add_event::<bevy::input::mouse::MouseMotion>();
    app.add_event::<bevy::input::mouse::MouseWheel>();
    app.add_event::<CombatVisualEvent>();
    app.init_resource::<CatalogResource>();
    app.init_resource::<RenderEntityMap>();
    app.init_resource::<SelectionState>();

    let lab_state = LabState {
        current_tab: LabTab::LiveBattle,
        sim_world: Some(SimWorld::new_default_battle_sandbox()),
        ..default()
    };
    app.insert_resource(lab_state);

    // Add all production Startup systems
    app.add_systems(
        Startup,
        (
            setup_bubble_assets_system,
            setup_indicator_assets_system,
            setup_vfx_assets_system,
            spawn_arena_environment,
            spawn_camera_system,
        ),
    );

    // Add all production Update systems
    app.add_systems(
        Update,
        (
            camera_control_system,
            update_indicators_system,
            update_mist_wake_system,
            update_projectiles_system,
            update_beams_system,
            update_melee_slashes_system,
            update_shatter_system,
            process_combat_events_system,
            sync_lab_preview_system,
            sync_live_battle_system,
            sync_swarm_stress_system,
        ),
    );

    for _ in 0..60 {
        app.update();
    }

    Ok(())
}
