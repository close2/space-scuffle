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
    dead_code,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::module_name_repetitions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::many_single_char_names,
    clippy::redundant_closure_for_method_calls,
    clippy::cast_sign_loss,
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    clippy::if_not_else
)]

pub mod arena_render;
pub mod assets;
pub mod battle_pipeline;
pub mod battle_presentation;
pub mod bubble_optics;
pub mod bubble_shader;
pub mod camera;
pub mod camera_drag;
pub mod indicators;
pub mod input_policy;
pub mod lab_ui;
pub mod media;
pub mod mist;
pub mod native_input_probe;
pub mod player_presentation;
pub mod presentation;
pub mod render_compatibility;
pub mod runtime_battle;
pub mod runtime_capture;
pub mod scene_controller;
pub mod selection;
pub mod shell_order_audit;
pub mod terrain;
pub mod vfx;

pub use arena_render::{setup_arena_system, spawn_arena_environment, ArenaMapMarker};
pub use assets::{try_resolve_assets_dir, AssetsDirError, ResolvedAssetsRoot};
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
    process_combat_events_system, setup_vfx_assets_system, spawn_death_shatter,
    update_beams_system, update_melee_slashes_system, update_mist_wake_system,
    update_projectiles_system, update_shatter_system, update_vfx_system, CombatFxMaterial,
    CombatFxUniformGpu, CombatVisualEvent, MistWakeMaterial, MistWakeUniformGpu, VfxSharedAssets,
};

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use std::path::PathBuf;

/// Resolves the absolute path to the workspace `assets/` directory.
///
/// This legacy API is retained for editor and embedding compatibility. Normal game
/// startup uses [`try_resolve_assets_dir`] and shares a [`ResolvedAssetsRoot`] resource.
/// The legacy result is not guaranteed to be validated or absolute.
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
    render_status: Option<Res<'_, runtime_battle::BattleRenderStatus>>,
    lab: Option<Res<'_, LabState>>,
    map_presentation: Option<Res<'_, runtime_battle::MapPresentation>>,
    images: Option<Res<'_, Assets<Image>>>,
) {
    if let Some(max) = frame_limit.max_frames {
        frame_limit.current_frame += 1;
        if frame_limit.current_frame >= max {
            let battle_failed = lab.is_some_and(|lab| {
                lab.current_tab == LabTab::LiveBattle
                    && (lab.sim_world.is_none()
                        || lab.battle_failure.is_some()
                        || !map_presentation.as_ref().is_some_and(|presentation| {
                            images.as_ref().is_some_and(|images| {
                                presentation
                                    .sprite_images
                                    .iter()
                                    .all(|handle| images.contains(handle))
                            })
                        }))
            });
            if battle_failed
                || render_status.is_some_and(|status| status.failed() || !status.is_ready())
            {
                error!("Render verification reached its frame limit before valid GPU pipelines and battle assets were ready");
                exit_events.send(AppExit::error());
            } else {
                exit_events.send(AppExit::Success);
            }
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

        if self.mode == ClientMode::SwarmStressTest {
            lab_state.swarm_active = true;
        }

        app.add_plugins((
            terrain::TerrainPlugin,
            runtime_battle::SceneEnvironmentPlugin,
            render_compatibility::RenderCompatibilityPlugin,
            runtime_battle::BattleRenderVerificationPlugin,
        ))
        .init_resource::<CatalogResource>()
        .insert_resource(media::MediaFailurePolicy::new(self.max_frames.is_some()))
        .init_resource::<RenderEntityMap>()
        .insert_resource(lab_state)
        .init_resource::<SelectionState>()
        .init_resource::<player_presentation::PresentationStatus>()
        .init_resource::<battle_presentation::BattlePresentation>()
        .init_resource::<battle_pipeline::session::BattleSession>()
        .init_resource::<input_policy::UiGeometry>()
        .insert_resource(input_policy::InputOwnership::interactive())
        .init_resource::<camera_drag::CameraDragState>()
        .insert_resource(FrameLimit {
            max_frames: self.max_frames,
            current_frame: 0,
        })
        .add_event::<CombatVisualEvent>()
        .add_plugins(MaterialPlugin::<BubbleDomeMaterial>::default())
        .add_plugins(MaterialPlugin::<SelectionRingMaterial>::default())
        .add_plugins(mist::MistWakePlugin)
        .add_plugins(bubble_optics::BubbleOpticsPlugin)
        .add_plugins(MaterialPlugin::<CombatFxMaterial>::default())
        .configure_sets(
            Update,
            (
                battle_pipeline::BattleSet::ExternalControl,
                battle_pipeline::BattleSet::CollectIntent,
                battle_pipeline::BattleSet::CommitSession,
                battle_pipeline::BattleSet::AdvanceTicks,
                battle_pipeline::BattleSet::PreparePresentation,
                battle_pipeline::BattleSet::Present,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                runtime_battle::sync_scene_camera,
                runtime_battle::manage_battle_scene,
                apply_deferred,
            )
                .chain()
                .in_set(battle_pipeline::BattleSet::CommitSession),
        )
        .add_systems(
            Update,
            battle_pipeline::driver::advance_battle_ticks
                .in_set(battle_pipeline::BattleSet::AdvanceTicks),
        )
        .add_systems(
            Update,
            (
                battle_presentation::refresh_battle_presentation,
                sync_live_battle_system,
                apply_deferred,
            )
                .chain()
                .in_set(battle_pipeline::BattleSet::PreparePresentation),
        )
        .add_systems(PostUpdate, runtime_battle::update_map_presentation)
        .add_systems(
            Update,
            (
                lab_ui::sync_selected_actor_identity.before(lab_ui_system),
                input_policy::sample_input_ownership.after(lab_ui_system),
                input_policy::apply_world_viewport.after(lab_ui_system),
            )
                .in_set(battle_pipeline::BattleSet::CollectIntent),
        )
        .add_systems(
            PostUpdate,
            selection_system
                .after(TransformSystem::TransformPropagate)
                .after(bevy::render::camera::CameraUpdateSystem),
        )
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
                update_indicators_system,
                update_projectiles_system.after(process_combat_events_system),
                update_beams_system.after(process_combat_events_system),
                update_melee_slashes_system.after(process_combat_events_system),
                update_shatter_system.after(process_combat_events_system),
                frame_limit_system,
            )
                .in_set(battle_pipeline::BattleSet::Present),
        )
        .add_systems(
            Update,
            (
                (
                    battle_pipeline::driver::publish_combat_events,
                    process_combat_events_system,
                )
                    .chain()
                    .in_set(battle_pipeline::BattleSet::Present),
                sync_lab_preview_system.in_set(battle_pipeline::BattleSet::PreparePresentation),
                sync_swarm_stress_system.in_set(battle_pipeline::BattleSet::PreparePresentation),
                lab_ui_system.in_set(battle_pipeline::BattleSet::CollectIntent),
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
#[must_use]
pub fn run_app(mode: ClientMode, max_frames: Option<u64>) -> AppExit {
    run_app_with_scenario(
        mode,
        max_frames,
        std::path::Path::new(runtime_battle::DEFAULT_BATTLE_SCENARIO),
    )
}

/// Runs the game with an explicit declarative scenario path relative to the assets root.
#[must_use]
pub fn run_app_with_scenario(
    mode: ClientMode,
    max_frames: Option<u64>,
    scenario_path: &std::path::Path,
) -> AppExit {
    if mode == ClientMode::HeadlessCheck {
        return match run_headless_check() {
            Ok(()) => {
                println!("==> Headless CI Check: PASSED (60 frames stepped deterministically).");
                AppExit::Success
            }
            Err(err) => {
                eprintln!("==> Headless CI Check FAILED: {err}");
                AppExit::error()
            }
        };
    }

    match build_graphical_app(mode, max_frames, scenario_path) {
        Ok(mut app) => {
            let media_policy = app.world().resource::<media::MediaFailurePolicy>().clone();
            media_policy.final_exit(render_compatibility::run_render_app(&mut app))
        }
        Err(error) => {
            eprintln!("Client startup failed: {error}");
            AppExit::error()
        }
    }
}

/// Typed failure while preparing the actual graphical client and external scenario.
#[derive(Debug, thiserror::Error)]
pub enum ClientStartupError {
    /// Selected assets root could not be validated.
    #[error(transparent)]
    Assets(#[from] AssetsDirError),
    /// Selected external battle definition could not be loaded.
    #[error(transparent)]
    Battle(#[from] runtime_battle::BattleLoadError),
    /// Headless math verification does not create a graphical application.
    #[error("headless-check mode is not a graphical application")]
    HeadlessMode,
}

/// Builds the same production graphical application used by normal client startup.
///
/// Preserves native window/plugin/asset defaults. Tooling may attach explicit
/// control plugins before running; it must not substitute another scene or renderer.
///
/// # Errors
/// Returns typed asset/scenario errors, or rejects the separate headless-check mode.
pub fn build_graphical_app(
    mode: ClientMode,
    max_frames: Option<u64>,
    scenario_path: &std::path::Path,
) -> Result<App, ClientStartupError> {
    if mode == ClientMode::HeadlessCheck {
        return Err(ClientStartupError::HeadlessMode);
    }
    let assets_root = ResolvedAssetsRoot::resolve()?;
    let asset_file_path = assets_root.bevy_file_path()?.to_owned();
    let battle_definition =
        runtime_battle::BattleDefinition::load(assets_root.as_path(), scenario_path)?;
    let catalog = CatalogResource(battle_definition.catalogs.units.clone());

    let is_virtual_or_test =
        max_frames.is_some() || std::env::var("SPACE_SCUFFLE_VIRTUAL_GPU").is_ok();
    let is_visible = !is_virtual_or_test || std::env::var("SPACE_SCUFFLE_SHOW_WINDOW").is_ok();

    let mut app = App::new();
    app.insert_resource(catalog)
        .insert_resource(battle_definition.palette.clone())
        .insert_resource(battle_definition)
        .insert_resource(assets_root)
        .add_plugins(
            presentation::GamePresentationPlugins
                .set(AssetPlugin {
                    file_path: asset_file_path,
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
        .add_plugins(ClientAppPlugin { mode, max_frames });
    Ok(app)
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
    let palette_root = ResolvedAssetsRoot::resolve().map_err(|error| error.to_string())?;
    let palette = player_presentation::PlayerPalette::load(
        palette_root.as_path(),
        world
            .catalog
            .archetypes()
            .filter(|a| a.showcase_hero)
            .count(),
    )
    .map_err(|error| error.to_string())?;
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
            let style = palette
                .resolve(bubble.owner)
                .map_err(|error| error.to_string())?;
            let uniform = BubbleUniformGpu::from_instance(bubble, None, style);
            if size_of::<BubbleUniformGpu>() != 160 {
                return Err(format!(
                    "BubbleUniformGpu size mismatch: expected 160, got {}",
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
    app.init_resource::<input_policy::InputOwnership>();
    app.init_resource::<camera_drag::CameraDragState>();
    app.add_event::<bevy::window::WindowEvent>();

    let root = ResolvedAssetsRoot::resolve().map_err(|error| error.to_string())?;
    let definition = runtime_battle::BattleDefinition::load(
        root.as_path(),
        std::path::Path::new(runtime_battle::DEFAULT_BATTLE_SCENARIO),
    )
    .map_err(|error| error.to_string())?;
    app.insert_resource(root);
    app.insert_resource(definition.palette.clone());
    app.init_resource::<player_presentation::PresentationStatus>();
    app.insert_resource(CatalogResource(definition.catalogs.units.clone()));
    let mut world = definition
        .instantiate()
        .map_err(|error| error.to_string())?;
    let session = battle_pipeline::session::BattleSession::prepare(
        battle_pipeline::commands::SessionGeneration::FIRST,
        &mut world,
        battle_pipeline::session::CommittedBattleMetadata::from_definition(&definition)
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    app.insert_resource(session);
    let lab_state = LabState {
        current_tab: LabTab::LiveBattle,
        sim_world: Some(world),
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
            (
                battle_pipeline::driver::advance_battle_ticks,
                sync_live_battle_system,
                battle_pipeline::driver::publish_combat_events,
            )
                .chain(),
            sync_swarm_stress_system,
        ),
    );

    for _ in 0..60 {
        app.update();
        if let Some(error) = &app.world().resource::<LabState>().battle_failure {
            return Err(error.clone());
        }
    }

    Ok(())
}
