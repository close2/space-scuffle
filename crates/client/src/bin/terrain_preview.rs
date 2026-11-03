//! Standalone terrain preview & real render verification binary.
//!
//! # Architectural Responsibilities
//! In accordance with Plan 21 §5.7b (`docs/plans/21b-plan-map-editor.md`),
//! this binary boots a real windowed Bevy application (1600x900), instantiates the production
//! shared terrain renderer ([`client::terrain`]), loads `showcase_canyon.scmap.ron` under the
//! `orbital_foundry` theme, and captures screenshots from four canonical camera poses:
//! 1. `overview`: Full 64x64 battlefield overview (target (32, 32), distance 70).
//! 2. `p0_base`: Player 0 high plateau base with 45° staircase edges and ramp D (target (12, 12), distance 28).
//! 3. `mesa`: Two-level wall rows 35..=37, ramp G, and guard rails (target (32, 30), distance 24).
//! 4. `chasm`: Bottomless abyss rim into the void nebula backdrop (target (45, 8), distance 22).
//!
//! # Failure Modes & Exit Codes
//! - Exit `0`: All four screenshots successfully written to disk.
//! - Exit `1`: Any pipeline shader compilation failure (`CachedPipelineState::Err`).
//! - Exit `2`: Timeout exceeding 600 rendered frames before completing all captures.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a preview tool fails loudly on its fixed fixture instead of recovering"
)]

use asset_baker::TerrainThemeCatalog;
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::render_resource::{CachedPipelineState, PipelineCache};
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::render::{Render, RenderApp};
use bevy::window::PresentMode;
use clap::Parser;
use client::resolve_assets_dir;
use client::terrain::{spawn_terrain, TerrainChunks, TerrainPlugin};
use sim_map::map_format::MapFile;
use std::fs;
use std::num::NonZeroU8;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const EXIT_CODE_PIPELINE_ERROR: NonZeroU8 = NonZeroU8::MIN;
const EXIT_CODE_TIMEOUT: NonZeroU8 = NonZeroU8::MIN.saturating_add(1);

/// CLI command line options for `terrain_preview`.
#[derive(Parser, Debug)]
#[command(author, version, about = "Terrain preview and pipeline verification")]
struct CliArgs {
    /// Directory where the four output screenshot PNGs will be saved.
    #[arg(long)]
    out: PathBuf,
}

/// Camera pose specification.
#[derive(Debug, Clone, Copy)]
struct CameraPose {
    name: &'static str,
    target: Vec2,
    distance: f32,
    look_at_y: f32,
}

const POSES: [CameraPose; 4] = [
    CameraPose {
        name: "overview",
        target: Vec2::new(32.0, 32.0),
        distance: 70.0,
        look_at_y: 3.0,
    },
    CameraPose {
        name: "p0_base",
        target: Vec2::new(12.0, 12.0),
        distance: 28.0,
        look_at_y: 6.0,
    },
    CameraPose {
        name: "mesa",
        target: Vec2::new(32.0, 30.0),
        distance: 24.0,
        look_at_y: 6.0,
    },
    CameraPose {
        name: "chasm",
        target: Vec2::new(45.0, 8.0),
        distance: 22.0,
        look_at_y: 0.0,
    },
];

fn compute_pose_transform(pose: &CameraPose) -> Transform {
    let pitch_rad = 55.0 * (std::f32::consts::PI / 180.0);
    let rotation = Quat::from_rotation_x(-pitch_rad);
    let offset = rotation * Vec3::new(0.0, 0.0, pose.distance);
    let ground_center = Vec3::new(pose.target.x, 0.0, pose.target.y);
    let eye_pos = ground_center + offset;
    let look_at_pos = Vec3::new(pose.target.x, pose.look_at_y, pose.target.y);
    Transform::from_translation(eye_pos).looking_at(look_at_pos, Vec3::Y)
}

/// Pipeline creation synchronization between `RenderApp` and `MainApp`.
#[derive(Resource, Clone)]
struct PipelineSync {
    has_pending: Arc<AtomicBool>,
    has_error: Arc<AtomicBool>,
}

/// State of capture progression across the four camera poses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepState {
    WaitingForReadiness,
    Settling(u32),
    Capturing,
    WaitingForFile,
}

#[derive(Resource)]
struct PreviewState {
    out_dir: PathBuf,
    current_pose_idx: usize,
    step_state: StepState,
    total_frames: u32,
}

#[derive(Component)]
struct PreviewCamera;

fn main() -> AppExit {
    let args = CliArgs::parse();
    fs::create_dir_all(&args.out).expect("failed to create output screenshot directory");

    let pipeline_sync = PipelineSync {
        has_pending: Arc::new(AtomicBool::new(true)),
        has_error: Arc::new(AtomicBool::new(false)),
    };

    let mut app = App::new();

    app.add_plugins(
        client::presentation::GamePresentationPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Space Scuffle — Terrain Preview & Render Verification".into(),
                resolution: (1600.0_f32, 900.0_f32).into(),
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }),
    )
    .add_plugins((
        TerrainPlugin,
        client::render_compatibility::RenderCompatibilityPlugin,
    ))
    .insert_resource(pipeline_sync.clone())
    .insert_resource(PreviewState {
        out_dir: args.out,
        current_pose_idx: 0,
        step_state: StepState::WaitingForReadiness,
        total_frames: 0,
    })
    .add_systems(Startup, setup_preview_scene)
    .add_systems(Update, update_preview_poses);

    // Register RenderApp system monitoring shader pipeline cache errors
    let render_app = app.sub_app_mut(RenderApp);
    render_app.insert_resource(pipeline_sync);
    render_app.add_systems(Render, check_render_pipelines);

    client::render_compatibility::run_render_app(&mut app)
}

fn setup_preview_scene(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut terrain_materials: ResMut<'_, Assets<client::terrain::TerrainMaterial>>,
    mut std_materials: ResMut<'_, Assets<StandardMaterial>>,
    mut images: ResMut<'_, Assets<Image>>,
) {
    let assets_dir = resolve_assets_dir();

    // 1. Load showcase_canyon map
    let map_path = assets_dir.join("maps/showcase_canyon.scmap.ron");
    let map_file = MapFile::load(&map_path)
        .unwrap_or_else(|e| panic!("failed to load {}: {e}", map_path.display()));
    let doc = map_file
        .decode()
        .unwrap_or_else(|e| panic!("failed to decode {}: {e}", map_path.display()));
    let game_map = doc
        .to_game_map()
        .unwrap_or_else(|e| panic!("failed to build GameMap from {}: {e}", map_path.display()));

    // 2. Load orbital_foundry theme
    let themes_path = assets_dir.join("catalog/terrain_themes.ron");
    let catalog = TerrainThemeCatalog::load(&themes_path)
        .unwrap_or_else(|e| panic!("failed to load {}: {e}", themes_path.display()));
    let theme = catalog
        .get("orbital_foundry")
        .expect("orbital_foundry theme must exist in catalog");

    // 3. Spawn terrain hierarchy, lighting, and void backdrop
    spawn_terrain(
        &mut commands,
        &mut meshes,
        &mut terrain_materials,
        &mut std_materials,
        &mut images,
        theme,
        game_map,
        &assets_dir,
    )
    .expect("failed to spawn terrain hierarchy");

    // 4. Spawn 3D camera configured for first pose
    let initial_transform = compute_pose_transform(&POSES[0]);
    commands.spawn((Camera3d::default(), initial_transform, PreviewCamera));
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn check_render_pipelines(pipeline_cache: Res<'_, PipelineCache>, sync: Res<'_, PipelineSync>) {
    for p in pipeline_cache.pipelines() {
        if let CachedPipelineState::Err(ref err) = p.state {
            eprintln!("CRITICAL: Pipeline cache compilation error: {err:?}");
            sync.has_error.store(true, Ordering::SeqCst);
            return;
        }
    }

    let pending = pipeline_cache.pipelines().any(|p| {
        matches!(
            p.state,
            CachedPipelineState::Queued | CachedPipelineState::Creating(..)
        )
    }) || pipeline_cache.waiting_pipelines().next().is_some();

    sync.has_pending.store(pending, Ordering::SeqCst);
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn update_preview_poses(
    mut commands: Commands<'_, '_>,
    mut state: ResMut<'_, PreviewState>,
    sync: Res<'_, PipelineSync>,
    terrain_chunks: Res<'_, TerrainChunks>,
    mut cam_query: Query<'_, '_, &mut Transform, With<PreviewCamera>>,
    mut exit_events: EventWriter<'_, AppExit>,
) {
    if sync.has_error.load(Ordering::SeqCst) {
        eprintln!("CRITICAL: Pipeline cache error detected; exiting code 1");
        exit_events.send(AppExit::Error(EXIT_CODE_PIPELINE_ERROR));
        return;
    }

    // 600-frame timeout guard
    if state.total_frames >= 600 {
        eprintln!("CRITICAL TIMEOUT: Exceeded 600 frames waiting for terrain preview completion");
        exit_events.send(AppExit::Error(EXIT_CODE_TIMEOUT));
        return;
    }
    state.total_frames += 1;

    if state.current_pose_idx >= POSES.len() {
        println!(
            "All {} poses successfully written to disk. Exiting 0.",
            POSES.len()
        );
        exit_events.send(AppExit::Success);
        return;
    }

    let pose = POSES[state.current_pose_idx];

    // Ensure camera transform matches current pose
    if let Ok(mut transform) = cam_query.get_single_mut() {
        let target_transform = compute_pose_transform(&pose);
        *transform = target_transform;
    }

    let chunks_ready = terrain_chunks.chunks.iter().all(|c| !c.dirty);
    let pipeline_ready = !sync.has_pending.load(Ordering::SeqCst);

    match state.step_state {
        StepState::WaitingForReadiness => {
            if chunks_ready && pipeline_ready {
                state.step_state = StepState::Settling(0);
            }
        }
        StepState::Settling(f) => {
            if f >= 3 {
                state.step_state = StepState::Capturing;
            } else {
                state.step_state = StepState::Settling(f + 1);
            }
        }
        StepState::Capturing => {
            let file_path = state.out_dir.join(format!("{}.png", pose.name));
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(file_path));
            state.step_state = StepState::WaitingForFile;
        }
        StepState::WaitingForFile => {
            let file_path = state.out_dir.join(format!("{}.png", pose.name));
            if file_path.exists() {
                if let Ok(meta) = fs::metadata(&file_path) {
                    if meta.len() > 1024 {
                        println!(
                            "Successfully captured pose '{}' ({} bytes)",
                            pose.name,
                            meta.len()
                        );
                        state.current_pose_idx += 1;
                        state.step_state = StepState::WaitingForReadiness;

                        if state.current_pose_idx >= POSES.len() {
                            println!("All four poses captured successfully! Exiting 0.");
                            exit_events.send(AppExit::Success);
                        }
                    }
                }
            }
        }
    }
}
