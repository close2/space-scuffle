//! Render verification pipeline and automated camera pose screenshot capture.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.7 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this module provides the automated headless/virtual GPU verification mode `--verify-render <out_dir>`.
//!
//! # Invariants Maintained
//! 1. **Pipeline Cache Health:** Verifies that no shader compilation errors (`CachedPipelineState::Err`)
//!    occurred across any terrain, bubble, or VFX material pipelines.
//! 2. **Canonical Camera Framing:** Captures 4 exact reference poses: `overview`, `cliff_corner`, `ramp`, and `overlays`.
//! 3. **Warmup & Settling Invariant:** Awaits complete chunk generation and 10 settling frames per pose
//!    before capturing screenshots.
//! 4. **Strict Timeout & Exit Codes:** Exits with code 1 if pipeline compilation fails, on timeout
//!    exceeding 900 frames, or if screenshot output is missing. Exits with code 0 on complete success.

use std::f32::consts::PI;
use std::fs;
use std::num::NonZeroU8;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::render_resource::{CachedPipelineState, PipelineCache};
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

use crate::camera::RtsCamera;
use crate::terrain::{
    TerrainChunks, TerrainParamsState, OVERLAY_FLAG_BRUSH, OVERLAY_FLAG_GRID,
    OVERLAY_FLAG_PATHABILITY, OVERLAY_FLAG_VALIDATION,
};

/// Maximum warmup frames allowed before timing out.
pub const VERIFY_WARMUP_TIMEOUT_FRAMES: u32 = 900;

/// Number of frames to wait for scene settling before triggering screenshot capture.
pub const POSE_SETTLING_FRAMES: u32 = 10;

/// Exit code for verification failure (pipeline error or timeout).
pub const EXIT_CODE_VERIFICATION_FAILURE: NonZeroU8 = match NonZeroU8::new(1) {
    Some(v) => v,
    None => unreachable!(),
};

/// Camera pose specification for render verification.
#[derive(Debug, Clone)]
pub struct VerifyPose {
    /// Identifier name matching the output PNG file name.
    pub name: &'static str,
    /// Horizontal ground focus target (x, z).
    pub target: Vec2,
    /// Distance from ground along view direction.
    pub distance: f32,
    /// Camera pitch angle in radians.
    pub pitch: f32,
    /// Active overlay flags for this pose.
    pub overlay_flags: u32,
    /// Optional brush center for brush footprint pose.
    pub brush_center: Option<Vec2>,
    /// Optional brush radius.
    pub brush_radius: Option<f32>,
}

/// The 4 canonical poses specified in Plan 21 §7.7.
#[must_use]
pub fn get_verify_poses() -> [VerifyPose; 4] {
    [
        VerifyPose {
            name: "overview",
            target: Vec2::new(32.0, 32.0),
            distance: 46.0,
            pitch: 58.0 * (PI / 180.0),
            overlay_flags: 0,
            brush_center: None,
            brush_radius: None,
        },
        VerifyPose {
            name: "cliff_corner",
            target: Vec2::new(25.0, 25.0),
            distance: 11.0,
            pitch: 40.0 * (PI / 180.0),
            overlay_flags: 0,
            brush_center: None,
            brush_radius: None,
        },
        VerifyPose {
            name: "ramp",
            target: Vec2::new(27.5, 9.5),
            distance: 9.0,
            pitch: 35.0 * (PI / 180.0),
            overlay_flags: 0,
            brush_center: None,
            brush_radius: None,
        },
        VerifyPose {
            name: "overlays",
            target: Vec2::new(32.0, 30.0),
            distance: 20.0,
            pitch: 58.0 * (PI / 180.0),
            overlay_flags: OVERLAY_FLAG_GRID
                | OVERLAY_FLAG_PATHABILITY
                | OVERLAY_FLAG_VALIDATION
                | OVERLAY_FLAG_BRUSH,
            brush_center: Some(Vec2::new(31.5, 24.5)),
            brush_radius: Some(3.0),
        },
    ]
}

/// Pipeline creation synchronization between RenderApp and MainApp.
#[derive(Resource, Clone, Default)]
pub struct PipelineSync {
    /// Atomic flag indicating whether any render pipelines are pending compilation.
    pub has_pending: Arc<AtomicBool>,
    /// Atomic flag indicating whether any pipeline failed with a compilation error.
    pub has_error: Arc<AtomicBool>,
}

/// Capturing lifecycle step for each camera pose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureStep {
    /// Waiting for initial chunks and pipelines to become ready.
    WaitingForWarmup,
    /// Settling frames after applying camera pose.
    Settling(u32),
    /// Triggering screenshot capture.
    Capturing,
    /// Awaiting file to be written to disk.
    WaitingForFile,
}

/// State tracking automated render verification.
#[derive(Resource)]
pub struct VerifyRenderState {
    /// Destination directory for output screenshots.
    pub out_dir: PathBuf,
    /// Current pose index in `poses`.
    pub current_pose_idx: usize,
    /// Lifecycle step within the current pose.
    pub step: CaptureStep,
    /// Total frames elapsed since application start.
    pub total_frames: u32,
    /// Poses to capture.
    pub poses: [VerifyPose; 4],
}

impl VerifyRenderState {
    /// Creates a new verification state writing to `out_dir`.
    #[must_use]
    pub fn new(out_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&out_dir);
        Self {
            out_dir,
            current_pose_idx: 0,
            step: CaptureStep::WaitingForWarmup,
            total_frames: 0,
            poses: get_verify_poses(),
        }
    }
}

/// System running in `RenderApp`'s `Render` schedule monitoring pipeline cache state.
pub fn check_render_pipelines(pipeline_cache: Res<'_, PipelineCache>, sync: Res<'_, PipelineSync>) {
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

/// System advancing the automated verification state machine and capturing screenshots.
pub fn update_render_verification_system(
    mut commands: Commands<'_, '_>,
    mut state: ResMut<'_, VerifyRenderState>,
    sync: Res<'_, PipelineSync>,
    terrain_chunks: Option<Res<'_, TerrainChunks>>,
    mut terrain_params: Option<ResMut<'_, TerrainParamsState>>,
    mut cam_query: Query<'_, '_, (&mut RtsCamera, &mut Transform)>,
    mut exit_events: EventWriter<'_, AppExit>,
) {
    // 1. Pipeline compilation error guard
    if sync.has_error.load(Ordering::SeqCst) {
        eprintln!("CRITICAL: Pipeline cache compilation error detected; exiting code 1");
        exit_events.send(AppExit::Error(EXIT_CODE_VERIFICATION_FAILURE));
        return;
    }

    // 2. Timeout guard
    if state.total_frames >= VERIFY_WARMUP_TIMEOUT_FRAMES {
        eprintln!(
            "CRITICAL TIMEOUT: Exceeded {VERIFY_WARMUP_TIMEOUT_FRAMES} frames waiting for render verification"
        );
        exit_events.send(AppExit::Error(EXIT_CODE_VERIFICATION_FAILURE));
        return;
    }
    state.total_frames += 1;

    // 3. Completion check
    if state.current_pose_idx >= state.poses.len() {
        println!(
            "All {} verification screenshots captured successfully. Exiting 0.",
            state.poses.len()
        );
        exit_events.send(AppExit::Success);
        return;
    }

    let pose = state.poses[state.current_pose_idx].clone();

    // 4. Update camera to match current pose
    if let Ok((mut cam, mut transform)) = cam_query.get_single_mut() {
        cam.target = pose.target;
        cam.distance = pose.distance;
        cam.pitch = pose.pitch;
        *transform = cam.compute_transform();
    }

    // 5. Update terrain overlays if params exist
    if let Some(ref mut params) = terrain_params {
        params.overlay_flags = pose.overlay_flags;
        if let Some(bc) = pose.brush_center {
            params.brush_center = bc;
        }
        if let Some(br) = pose.brush_radius {
            params.brush_radius = br;
        }
    }

    let chunks_ready = terrain_chunks
        .as_ref()
        .map_or(true, |tc| tc.chunks.iter().all(|c| !c.dirty));
    let pipeline_ready = !sync.has_pending.load(Ordering::SeqCst);

    match state.step {
        CaptureStep::WaitingForWarmup => {
            if chunks_ready && pipeline_ready {
                state.step = CaptureStep::Settling(0);
            }
        }
        CaptureStep::Settling(f) => {
            if f >= POSE_SETTLING_FRAMES {
                state.step = CaptureStep::Capturing;
            } else {
                state.step = CaptureStep::Settling(f + 1);
            }
        }
        CaptureStep::Capturing => {
            let file_path = state.out_dir.join(format!("{}.png", pose.name));
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(file_path));
            state.step = CaptureStep::WaitingForFile;
        }
        CaptureStep::WaitingForFile => {
            let file_path = state.out_dir.join(format!("{}.png", pose.name));
            if file_path.exists() {
                if let Ok(meta) = fs::metadata(&file_path) {
                    if meta.len() > 1024 {
                        println!("Captured pose '{}' ({} bytes)", pose.name, meta.len());
                        state.current_pose_idx += 1;
                        state.step = CaptureStep::WaitingForWarmup;

                        if state.current_pose_idx >= state.poses.len() {
                            println!("All verification screenshots captured. Exiting 0.");
                            exit_events.send(AppExit::Success);
                        }
                    }
                }
            }
        }
    }
}
