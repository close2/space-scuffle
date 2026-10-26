//! Space Scuffle asset baking and flipbook atlas generation library.
//!
//! # Architectural Charter
//! In accordance with [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md) and
//! [06-factions-and-video-style-guide.md](file:///home/cl/projects/space-scuffle/docs/06-factions-and-video-style-guide.md),
//! `asset_baker` provides an offline build-time pipeline that transforms both raw video
//! files (`.mp4`, `.webm`) and AI-generated video sequences from Google Veo into deterministic,
//! GPU-ready vertical strip image atlases paired with strongly-typed `.flipbook.ron`
//! manifests.
//!
//! # Core Subsystems
//! - [`error`]: Strongly-typed error domain via [`BakerError`].
//! - [`manifest`]: Structural layout and slice metadata via [`FlipbookAtlasMeta`].
//! - [`ffmpeg_importer`]: Transcoding, center-cropping, scaling, and strip packing via external FFmpeg.
//! - [`veo_client`]: Strongly-typed Google Gemini and Vertex AI Veo Video Generation client.
//! - [`procedural_generator`]: Pure-Rust procedural sprite synthesizers for mobile units and building domes.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(clippy::doc_markdown)]

pub mod error;
pub mod ffmpeg_importer;
pub mod manifest;
pub mod procedural_generator;
pub mod veo_client;

pub use error::BakerError;
pub use ffmpeg_importer::{
    bake_multi_action_videos, bake_multi_action_videos_with_states, extract_and_pack_video,
    FfmpegExtractConfig,
};
pub use manifest::{ActionSlice, FlipbookAtlasMeta, FlipbookLayout};
pub use procedural_generator::{
    generate_procedural_atlas, generate_procedural_dome_atlas, generate_procedural_unit_atlas,
    ProceduralAtlasOutput,
};
pub use veo_client::{
    build_action_prompt, resolve_credentials, VeoClient, VeoCredentials, VeoGenerateRequest,
    VeoGenerateVideoResponse, VeoImageInput, VeoInstance, VeoOperation, VeoParameters,
    VeoVideoOutput, DEFAULT_VEO_MODEL,
};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Returns all mobile unit archetype slugs discovered dynamically from the catalog.
#[must_use]
pub fn default_unit_archetypes() -> Vec<String> {
    sim_core::UnitCatalog::default()
        .archetypes()
        .filter(|a| !matches!(a.shape, sim_types::enums::ShapeKind::BuildingDome { .. }))
        .map(|a| a.slug.clone())
        .collect()
}

/// Returns all building dome archetype slugs discovered dynamically from the catalog.
#[must_use]
pub fn default_dome_archetypes() -> Vec<String> {
    sim_core::UnitCatalog::default()
        .archetypes()
        .filter(|a| matches!(a.shape, sim_types::enums::ShapeKind::BuildingDome { .. }))
        .map(|a| a.slug.clone())
        .collect()
}

/// Locates the concept video path across workspace execution contexts.
#[must_use]
pub fn find_concept_video() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("tmp/gemini_generated_video_de659ff9.mp4"),
        PathBuf::from("../tmp/gemini_generated_video_de659ff9.mp4"),
        PathBuf::from("../../tmp/gemini_generated_video_de659ff9.mp4"),
        PathBuf::from("/home/cl/projects/space-scuffle/tmp/gemini_generated_video_de659ff9.mp4"),
    ];
    candidates.into_iter().find(|candidate| candidate.exists())
}

/// Locates the AI keyframes directory across workspace execution contexts.
#[must_use]
pub fn find_keyframes_dir() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("SPACE_SCUFFLE_KEYFRAMES") {
        let p = PathBuf::from(env_path);
        if p.is_dir() {
            return Some(p);
        }
    }
    let candidates = [
        PathBuf::from("assets/ai_keyframes"),
        PathBuf::from("../assets/ai_keyframes"),
        PathBuf::from("../../assets/ai_keyframes"),
        PathBuf::from("/home/cl/projects/space-scuffle/assets/ai_keyframes"),
    ];
    candidates.into_iter().find(|candidate| candidate.is_dir())
}

/// Generates all default procedural unit bubbles and building domes and writes their
/// `.png` image strips and `.flipbook.ron` manifests into the target directory.
///
/// # Arguments
/// - `output_dir`: Directory where all generated flipbook images and manifests will be written.
///
/// # Errors
/// Returns [`BakerError::Io`] if directory creation or file writing fails,
/// or [`BakerError::ValidationFailed`] if any atlas fails invariant checks.
pub fn generate_all_procedural_atlases(
    output_dir: &Path,
) -> Result<Vec<FlipbookAtlasMeta>, BakerError> {
    std::fs::create_dir_all(output_dir).map_err(|source| BakerError::Io {
        path: output_dir.to_path_buf(),
        source,
    })?;

    let mut generated_metas = Vec::new();

    // 1. Generate Mobile Units (192x192, 15 FPS)
    for unit_name in default_unit_archetypes() {
        let output = generate_procedural_unit_atlas(&unit_name, 192, 15)?;
        let png_path = output_dir.join(format!("{unit_name}.png"));
        let ron_path = output_dir.join(format!("{unit_name}.flipbook.ron"));

        output
            .image
            .save(&png_path)
            .map_err(|source| BakerError::ImageProcessing {
                path: png_path.clone(),
                source,
            })?;

        output.meta.save_to_ron(&ron_path)?;
        generated_metas.push(output.meta);
    }

    // 2. Generate Building Domes (256x256, 15 FPS)
    for dome_name in default_dome_archetypes() {
        let output = generate_procedural_dome_atlas(&dome_name, 256, 15)?;
        let png_path = output_dir.join(format!("{dome_name}.png"));
        let ron_path = output_dir.join(format!("{dome_name}.flipbook.ron"));

        output
            .image
            .save(&png_path)
            .map_err(|source| BakerError::ImageProcessing {
                path: png_path.clone(),
                source,
            })?;

        output.meta.save_to_ron(&ron_path)?;
        generated_metas.push(output.meta);
    }

    Ok(generated_metas)
}

/// Bakes starter unit flipbook atlases using real video frames from the concept video.
///
/// In accordance with AGENTS.md Section 1.2 (Zero Fake Assets), this ensures that all starter
/// entities are backed by genuine video frames extracted via FFmpeg rather than fake procedural
/// pixel drawings or static wobbles.
///
/// # Arguments
/// - `video_path`: Path to the real MP4 video (e.g. `tmp/gemini_generated_video_de659ff9.mp4`).
/// - `output_dir`: Destination directory for baked PNGs and `.flipbook.ron` manifests.
/// - `size`: Frame width and height in pixels (e.g. 512).
/// - `fps`: Playback framerate.
///
/// # Errors
/// Returns [`BakerError`] on FFmpeg execution or image processing failure.
pub fn bake_starter_units_from_real_video(
    video_path: &Path,
    output_dir: &Path,
    size: u32,
    fps: u32,
) -> Result<Vec<FlipbookAtlasMeta>, BakerError> {
    let catalog = sim_core::UnitCatalog::default();
    let mut metas = Vec::new();
    for arch in catalog.archetypes() {
        let meta = bake_custom_video(video_path, output_dir, &arch.slug, size, fps)?;
        metas.push(meta);
    }
    if let Ok(meta) = bake_custom_video(video_path, output_dir, "concept_video", size, fps) {
        metas.push(meta);
    }
    Ok(metas)
}

/// Bakes the standard Space Scuffle concept video into a Texture2DArray vertical strip and RON manifest.
///
/// Uses default production settings: 256x256 resolution, 24 FPS temporal downsampling,
/// and maps the entire video sequence to [`sim_types::enums::AnimState::Idle`].
///
/// # Errors
/// Returns [`BakerError::FfmpegNotFound`] if FFmpeg is not installed,
/// [`BakerError::Io`] on file access failure, or [`BakerError::FfmpegExecutionFailed`] on transcode error.
pub fn bake_concept_video(
    video_path: &Path,
    output_dir: &Path,
) -> Result<FlipbookAtlasMeta, BakerError> {
    let png_path = output_dir.join("concept_video.png");
    let ron_path = output_dir.join("concept_video.flipbook.ron");
    let config = FfmpegExtractConfig {
        ffmpeg_path: PathBuf::from("/usr/bin/ffmpeg"),
        target_size: 256,
        target_fps: 24,
    };

    extract_and_pack_video(
        video_path,
        &png_path,
        &ron_path,
        "concept_video",
        &config,
        BTreeMap::new(),
    )
}

/// Bakes an arbitrary user video into a vertical strip PNG and `.flipbook.ron` manifest.
///
/// # Arguments
/// - `video_path`: Input video file on disk.
/// - `output_dir`: Target directory for output files.
/// - `name`: Atlas name prefix used for files: `<name>.png` and `<name>.flipbook.ron`.
/// - `size`: Frame width and height in pixels.
/// - `fps`: Framerate for temporal downsampling.
///
/// # Errors
/// Returns [`BakerError`] on any failure in transcoding or packing.
pub fn bake_custom_video(
    video_path: &Path,
    output_dir: &Path,
    name: &str,
    size: u32,
    fps: u32,
) -> Result<FlipbookAtlasMeta, BakerError> {
    let png_path = output_dir.join(format!("{name}.png"));
    let ron_path = output_dir.join(format!("{name}.flipbook.ron"));
    let config = FfmpegExtractConfig {
        ffmpeg_path: PathBuf::from("/usr/bin/ffmpeg"),
        target_size: size,
        target_fps: fps,
    };

    extract_and_pack_video(
        video_path,
        &png_path,
        &ron_path,
        name,
        &config,
        BTreeMap::new(),
    )
}
