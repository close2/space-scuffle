//! Space Scuffle asset baking and flipbook atlas generation library.
//!
//! # Architectural Charter
//! In accordance with `AGENTS.md` and
//! `docs/06-factions-and-video-style-guide.md`,
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
pub mod media;
pub mod media_audit;
pub mod procedural_generator;
pub mod terrain_texture;
pub mod terrain_theme;
pub mod veo_client;

pub use error::BakerError;
pub use ffmpeg_importer::{
    bake_multi_action_videos, bake_multi_action_videos_with_states, extract_and_pack_video,
    probe_video, FfmpegExtractConfig, VideoProbe,
};
pub use manifest::{ActionSlice, FlipbookAtlasMeta, FlipbookLayout};
pub use procedural_generator::{
    generate_procedural_atlas, generate_procedural_dome_atlas, generate_procedural_unit_atlas,
    ProceduralAtlasOutput,
};
pub use terrain_texture::{
    bake_theme, derive_height, derive_normal, derive_orm, find_horizontal_min_cut,
    find_vertical_min_cut, linear_to_srgb, linear_to_srgb_image, load_and_resize, make_tileable,
    seam_score, smoothstep, srgb_to_linear, tiled_preview, BakeReport, GrayF32Image,
    LayerBakeReport, MinCutBuffers, Rgba32FImage, Rgba8Image, AO_MAX_CLAMP, AO_MIN_CLAMP, AO_SCALE,
    LUMA_REC709_B, LUMA_REC709_G, LUMA_REC709_R, MAX_ASPECT_RATIO_DEVIATION, PYRAMID_LEVELS,
    SEAM_CORE_WIDTH_DIVISOR, SEAM_HALF_WIDTH_DIVISOR, SEAM_SCORE_DISCONTINUITY_SENTINEL,
    STDDEV_5X5_RADIUS,
};
pub use terrain_theme::{
    BakedLayerPaths, TerrainTheme, TerrainThemeCatalog, TerrainThemeError, ThemeBackdrop,
    ThemeLayer, ThemeLayers, ThemeLighting, TilingMode, LAYER_INDEX_CLIFF, LAYER_INDEX_DIRT,
    LAYER_INDEX_GRASS, LAYER_INDEX_METAL_PLATE, MAX_THEME_TEXTURE_SIZE, MIN_THEME_TEXTURE_SIZE,
};
pub use veo_client::{
    build_action_prompt, resolve_credentials, resolve_credentials_from, validate_catalog_slug,
    VeoClient, VeoCredentials, VeoGenerateRequest, VeoGenerateVideoResponse, VeoImageInput,
    VeoInstance, VeoOperation, VeoParameters, VeoVideoOutput, DEFAULT_VEO_MODEL,
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

/// Rejects the retired operation that copied one concept video across an entire roster.
///
/// The signature remains available for embedding compatibility, but no input is read and
/// no output is written. Each unit requires an explicitly selected, unit-specific source.
///
/// # Errors
/// Always returns [`BakerError::ExplicitSourcesRequired`].
#[deprecated(note = "one concept movie cannot establish media for an entire roster")]
pub fn bake_starter_units_from_real_video(
    _video_path: &Path,
    _output_dir: &Path,
    _size: u32,
    _fps: u32,
) -> Result<Vec<FlipbookAtlasMeta>, BakerError> {
    Err(BakerError::ExplicitSourcesRequired)
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
    let (png_path, ron_path) = ffmpeg_importer::atlas_output_paths(output_dir, name)?;
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
