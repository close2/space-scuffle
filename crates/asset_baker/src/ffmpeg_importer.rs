//! Video frame extraction via external FFmpeg invocation and vertical strip assembly.
//!
//! This module automates the execution of `/usr/bin/ffmpeg` to transcode arbitrary
//! video inputs (`.mp4`, `.webm`, etc.) into standardized square frame sequences,
//! which are then composited into single vertical strip PNGs accompanied by `.flipbook.ron`
//! manifests.
//!
//! # Framing
//! A clip frame becomes a square in one of two ways ([`crate::framing::BakeFraming`]): a
//! centre crop, done by FFmpeg alone, or a letterbox, where FFmpeg scales the whole frame to fit
//! and [`crate::framing::frame_to_square`] adds bars of the clip's measured border colour.
//! Identity bakes ([`bake_framed_action_videos`]) take the framing chosen from
//! `media_style.ron`; the single-clip bakes and [`bake_multi_action_videos_with_states`] keep
//! the centre crop.
//!
//! # Seam repair
//! A looping state can be given a seam repair ([`decode_repaired_clip`], plan
//! `docs/plans/2026-11-04-veo-seamless-loops.md`, D-5): its frames are sampled over one loop
//! period, a steady camera zoom is locked out, and its last frames are blended into its first.
//! [`bake_repaired_action_videos`] takes the repair of each state; every other entry point bakes
//! without one, and a state without one bakes exactly as before the repair existed.
//!
//! # Invariants
//! - **Whole clips:** a clip with more frames than the GPU texture budget allows is sampled
//!   evenly from its first to its last frame, and is never cut short. Sampled frames keep the
//!   manifest's playback rate, so such a clip plays faster than real time. The bake reports
//!   this on stderr, because the only way to avoid it is a smaller frame size.
//! - **No invented actions:** a manifest lists only actions that a source clip was supplied
//!   for, so [`crate::media::require_action`] can report the rest as missing.

#![allow(clippy::cast_possible_truncation, clippy::doc_markdown)]

use crate::error::BakerError;
use crate::framing::{frame_to_square, BakeFraming, FramedFrames};
use crate::loop_metrics::{
    apply_scale_lock, crossfade_into_first, downsample_half, plan_scale_lock, SeamRepair,
};
use crate::manifest::{ActionSlice, FlipbookAtlasMeta, FlipbookLayout};
use crate::prompts::{atlas_loops, MAX_ATLAS_STRIP_HEIGHT};
use crate::provenance::{FrameSampling, SeamRecord};
use image::RgbaImage;
use sim_types::enums::AnimState;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Maximum texture dimension in pixels that WebGPU guarantees: the one limit the loop policy's
/// frame budget and the provenance record also use.
const MAX_STRIP_HEIGHT: u32 = MAX_ATLAS_STRIP_HEIGHT;

/// Default location of the `ffprobe` executable.
pub const DEFAULT_FFPROBE_PATH: &str = "/usr/bin/ffprobe";

/// Measured properties of a video file that passed [`probe_video`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoProbe {
    /// Width of the first video stream in pixels.
    pub width: u32,
    /// Height of the first video stream in pixels.
    pub height: u32,
    /// Container duration in seconds.
    pub duration_seconds: f64,
    /// Number of video packets actually read from the file.
    pub packets: u64,
}

/// Verifies with `ffprobe` that `video_path` is a complete video and measures it.
///
/// Every packet of the first video stream is read. A container header can survive truncation,
/// so the packets read must also match the frame count that the header declares, if any.
/// Only local file access is allowed, because the file may be an untrusted download and some
/// container formats can reference URLs.
///
/// # Errors
/// Returns [`BakerError::FfmpegNotFound`] if `ffprobe_path` does not exist, or
/// [`BakerError::VideoValidationFailed`] if the file is not a complete video.
pub fn probe_video(ffprobe_path: &Path, video_path: &Path) -> Result<VideoProbe, BakerError> {
    if !ffprobe_path.exists() {
        return Err(BakerError::FfmpegNotFound {
            path: ffprobe_path.to_path_buf(),
        });
    }
    let invalid = |reason: String| BakerError::VideoValidationFailed {
        path: video_path.to_path_buf(),
        reason,
    };

    let output = Command::new(ffprobe_path)
        .args(["-v", "error", "-protocol_whitelist", "file"])
        .args(["-count_packets", "-select_streams", "v:0"])
        .args(["-show_entries"])
        .arg("stream=width,height,nb_frames,nb_read_packets:format=duration")
        .args(["-of", "json", "-i"])
        .arg(video_path)
        .output()
        .map_err(|source| BakerError::Io {
            path: ffprobe_path.to_path_buf(),
            source,
        })?;
    if !output.status.success() {
        return Err(invalid(format!(
            "ffprobe rejected it: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let stream = report
        .get("streams")
        .and_then(|streams| streams.get(0))
        .ok_or_else(|| invalid("it has no video stream".into()))?;
    let number = |value: Option<&serde_json::Value>| -> Option<u64> {
        value.and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
    };
    let dimension = |field: &str| {
        number(stream.get(field))
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0)
            .ok_or_else(|| invalid(format!("its video stream has no valid {field}")))
    };
    let width = dimension("width")?;
    let height = dimension("height")?;

    let duration_seconds = report
        .pointer("/format/duration")
        .and_then(serde_json::Value::as_str)
        .and_then(|text| text.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .ok_or_else(|| invalid("it has no positive duration".into()))?;

    let packets = number(stream.get("nb_read_packets")).unwrap_or(0);
    if packets == 0 {
        return Err(invalid("no video packets could be read".into()));
    }
    if let Some(declared) = number(stream.get("nb_frames")) {
        if declared != packets {
            return Err(invalid(format!(
                "it is truncated: the header declares {declared} frames but {packets} were read"
            )));
        }
    }

    Ok(VideoProbe {
        width,
        height,
        duration_seconds,
        packets,
    })
}

/// Configuration parameters for FFmpeg extraction and scaling of a single clip
/// ([`extract_and_pack_video`]), which always takes the centred square of the clip
/// ([`BakeFraming::CenterCrop`]).
#[derive(Debug, Clone)]
pub struct FfmpegExtractConfig {
    /// Path to the FFmpeg executable binary.
    pub ffmpeg_path: PathBuf,
    /// Target width and height in pixels for the square centre-cropped frames (e.g. 192 or 256).
    pub target_size: u32,
    /// Target playback framerate (e.g. 15 or 24 FPS) for temporal downsampling.
    pub target_fps: u32,
}

impl Default for FfmpegExtractConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: PathBuf::from("/usr/bin/ffmpeg"),
            target_size: 256,
            target_fps: 24,
        }
    }
}

/// Chooses `wanted` frame indices spread evenly over `0..available`.
///
/// When frames must be dropped the result always includes the first and the last frame, or
/// just the first if `wanted` is 1. When `wanted >= available`, every index is returned.
#[must_use]
pub fn evenly_spaced_indices(available: usize, wanted: usize) -> Vec<usize> {
    if wanted >= available {
        return (0..available).collect();
    }
    if wanted <= 1 {
        return vec![0; wanted];
    }
    (0..wanted)
        .map(|step| step * (available - 1) / (wanted - 1))
        .collect()
}

/// Chooses at most `wanted` frame indices spread evenly over one loop of a clip of `available`
/// frames (plan `docs/plans/2026-11-04-veo-seamless-loops.md`, D-5).
///
/// A looping state plays its last kept frame and then its first again, so the clip's final
/// frame stands for the restart (for a clip pinned to end on its start picture it shows the
/// same picture as frame 0). It is therefore left out, and index `i` is
/// `i · (available - 1) / wanted` for `i` in `0..wanted`: every step, the wrap from the last
/// kept index back to `available - 1` included, is `(available - 1) / wanted` rounded down or
/// up. When `wanted` is at least `available - 1`, every frame but the final one is returned; a
/// one-frame clip returns that frame.
#[must_use]
pub fn evenly_spaced_loop_indices(available: usize, wanted: usize) -> Vec<usize> {
    if available == 0 || wanted == 0 {
        return Vec::new();
    }
    let period = available - 1;
    if period == 0 {
        return vec![0];
    }
    if wanted >= period {
        return (0..period).collect();
    }
    (0..wanted).map(|step| step * period / wanted).collect()
}

/// Reduces a clip's frames to `budget`, keeping its whole duration. See the module docs.
fn fit_clip_to_budget(frames: Vec<PathBuf>, budget: usize, label: &str, fps: u32) -> Vec<PathBuf> {
    if frames.len() <= budget {
        return frames;
    }
    eprintln!(
        "    Note: '{label}' has {} frames at {fps} fps but the texture budget allows {budget}. \
         Sampling evenly across the whole clip, which then plays up to {}x faster. \
         Use a smaller frame size to keep more frames.",
        frames.len(),
        frames.len().div_ceil(budget.max(1))
    );
    evenly_spaced_indices(frames.len(), budget)
        .into_iter()
        .filter_map(|index| frames.get(index).cloned())
        .collect()
}

/// Decodes a whole clip into PNG frames in `frames_dir` through `filter` (one of
/// [`BakeFraming::ffmpeg_filter`]), sorted in temporal order.
fn extract_clip_frames(
    ffmpeg_path: &Path,
    video_path: &Path,
    filter: &str,
    frames_dir: &Path,
) -> Result<Vec<PathBuf>, BakerError> {
    let frame_pattern = frames_dir.join("frame_%05d.png");

    // PERFORMANCE RATIONALE:
    // Why: Runtime software or hardware decoding of 50+ concurrent MP4 video streams inside 3D unit
    // bubbles saturates GPU video decoders and causes massive framerate stuttering.
    // What: We pre-decode video offline using FFmpeg's SIMD decoders, applying the framing's filter:
    // either `crop=min(iw\,ih):min(iw\,ih)` to centre-crop arbitrary aspect ratios to a 1:1 square,
    // or `force_original_aspect_ratio=decrease` to fit the whole frame inside the square for a
    // letterbox; then scaling to the frame size (e.g., 512x512 or 512x288) and clean temporal
    // downsampling (e.g., 24 -> 15 FPS). The letterbox square is completed in Rust afterwards.
    // Trade-off: Higher disk storage footprint (~20-50MB per baked strip) in exchange for O(1) texture
    // array slice sampling with zero runtime decoding overhead.
    let output = Command::new(ffmpeg_path)
        .arg("-y")
        .arg("-i")
        .arg(video_path)
        .arg("-vf")
        .arg(filter)
        .arg(&frame_pattern)
        .output()
        .map_err(|source| BakerError::Io {
            path: ffmpeg_path.to_path_buf(),
            source,
        })?;

    if !output.status.success() {
        return Err(BakerError::FfmpegExecutionFailed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    // Collect all generated PNG frame paths sorted in ascending numerical order
    let mut frame_paths: Vec<PathBuf> = std::fs::read_dir(frames_dir)
        .map_err(|source| BakerError::Io {
            path: frames_dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
        .collect();
    frame_paths.sort();

    if frame_paths.is_empty() {
        return Err(BakerError::ValidationFailed {
            name: video_path.display().to_string(),
            reason: "FFmpeg execution succeeded but produced 0 output frame files".into(),
        });
    }
    Ok(frame_paths)
}

/// Returns the PNG and manifest paths for an atlas, refusing a name that is not one safe file
/// name component, so that output stays inside `output_dir`.
///
/// # Errors
/// Returns [`BakerError::ValidationFailed`] for an empty name, separators or traversal.
pub fn atlas_output_paths(
    output_dir: &Path,
    atlas_name: &str,
) -> Result<(PathBuf, PathBuf), BakerError> {
    crate::media::media_paths(output_dir, atlas_name).map_err(|error| {
        BakerError::ValidationFailed {
            name: atlas_name.to_string(),
            reason: error.to_string(),
        }
    })
}

/// Extracts frames from a video file, centre-crops them to a 1:1 square
/// ([`BakeFraming::CenterCrop`]), scales, and packs them into a PNG vertical strip.
///
/// # Arguments
/// - `video_path`: Source video file (`.mp4`, `.webm`, etc.) on disk.
/// - `output_png_path`: Target filesystem destination for the composited vertical strip PNG.
/// - `output_meta_path`: Target filesystem destination for the `.flipbook.ron` manifest.
/// - `atlas_name`: Semantic identifier for this atlas (e.g. `"concept_video"`).
/// - `config`: Extraction parameters including frame dimensions and framerate.
/// - `actions_mapping`: Optional custom mapping of [`AnimState`] to [`ActionSlice`]. If empty,
///   the whole clip becomes a single [`AnimState::Idle`] action and no other action is listed.
///
/// # Errors
/// Returns [`BakerError::FfmpegNotFound`] if the binary is absent, [`BakerError::FfmpegExecutionFailed`]
/// if FFmpeg exits with an error, [`BakerError::ImageProcessing`] on compositing failure,
/// or [`BakerError::Io`] on filesystem read/write errors.
pub fn extract_and_pack_video(
    video_path: &Path,
    output_png_path: &Path,
    output_meta_path: &Path,
    atlas_name: &str,
    config: &FfmpegExtractConfig,
    actions_mapping: BTreeMap<AnimState, ActionSlice>,
) -> Result<FlipbookAtlasMeta, BakerError> {
    if !config.ffmpeg_path.exists() {
        return Err(BakerError::FfmpegNotFound {
            path: config.ffmpeg_path.clone(),
        });
    }

    if !video_path.exists() {
        return Err(BakerError::Io {
            path: video_path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "Video source file does not exist at {}",
                    video_path.display()
                ),
            ),
        });
    }

    let temp_dir = tempfile::tempdir().map_err(|source| BakerError::Io {
        path: PathBuf::from("tempdir"),
        source,
    })?;

    let frame_paths = extract_clip_frames(
        &config.ffmpeg_path,
        video_path,
        &BakeFraming::CenterCrop.ffmpeg_filter(config.target_size, config.target_fps),
        temp_dir.path(),
    )?;

    // PERFORMANCE RATIONALE:
    // Why: WebGPU and standard desktop GPU drivers enforce `max_texture_dimension_2d <= 16384` pixels.
    // What: We keep at most `16384 / config.target_size` vertically stacked frames, sampled across
    // the whole clip (e.g., 32 frames at 512x512 = 16384 px, or 64 frames at 256x256 = 16384 px).
    // Trade-off: Bounds temporal resolution to the hardware texture limit while guaranteeing zero
    // GPU texture allocation panics across Vulkan, Metal, DX12, and WebGPU.
    let max_frames = (MAX_STRIP_HEIGHT / config.target_size.max(1)) as usize;
    let frame_paths = fit_clip_to_budget(frame_paths, max_frames, atlas_name, config.target_fps);

    let total_frames = frame_paths.len() as u32;
    let strip_height = config.target_size.saturating_mul(total_frames);
    let mut packed_strip = RgbaImage::new(config.target_size, strip_height);

    for (i, path) in frame_paths.iter().enumerate() {
        let frame_img = image::open(path)
            .map_err(|source| BakerError::ImageProcessing {
                path: path.clone(),
                source,
            })?
            .to_rgba8();

        let y_offset = (i as u32).saturating_mul(config.target_size);
        image::imageops::overlay(&mut packed_strip, &frame_img, 0, i64::from(y_offset));
    }

    // Ensure output parent directory exists
    if let Some(parent) = output_png_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| BakerError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    if let Some(parent) = output_meta_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| BakerError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    packed_strip
        .save(output_png_path)
        .map_err(|source| BakerError::ImageProcessing {
            path: output_png_path.to_path_buf(),
            source,
        })?;

    // A lone clip says nothing about any other action, so no other action is claimed for it.
    let mut actions = actions_mapping;
    if actions.is_empty() {
        actions.insert(
            AnimState::Idle,
            ActionSlice {
                start_frame: 0,
                frame_count: total_frames,
                looping: true,
                hold_last_frame: false,
            },
        );
    }

    let meta = FlipbookAtlasMeta {
        name: atlas_name.to_string(),
        frame_width: config.target_size,
        frame_height: config.target_size,
        fps: config.target_fps,
        total_frames,
        layout: FlipbookLayout::VerticalStrip,
        actions,
    };

    meta.save_to_ron(output_meta_path)?;
    Ok(meta)
}

/// Bakes multiple video clips corresponding to sequential animation states into a single
/// vertical strip PNG and `.flipbook.ron` manifest.
///
/// Maps input video paths sequentially to [`AnimState::Idle`], [`AnimState::Walk`],
/// [`AnimState::Attack`], and [`AnimState::Die`]. Only those actions are listed in the manifest.
///
/// # Arguments
/// - `unit_name`: Semantic archetype name (e.g., `"marine"`, `"zealot"`).
/// - `video_paths`: Ordered slice of up to four video file paths, one per action above.
/// - `output_dir`: Directory where `<unit_name>.png` and `<unit_name>.flipbook.ron` will be created.
/// - `frame_size`: Square frame dimensions in pixels (e.g., 256 or 512).
/// - `fps`: Framerate for temporal downsampling.
///
/// # Errors
/// Returns [`BakerError::ValidationFailed`] for no paths or more than four, or [`BakerError`]
/// if FFmpeg execution fails or image files cannot be read or written.
pub fn bake_multi_action_videos<P: AsRef<Path>>(
    unit_name: &str,
    video_paths: &[P],
    output_dir: &Path,
    frame_size: u32,
    fps: u32,
) -> Result<FlipbookAtlasMeta, BakerError> {
    const DEFAULT_STATES: [AnimState; 4] = [
        AnimState::Idle,
        AnimState::Walk,
        AnimState::Attack,
        AnimState::Die,
    ];

    if video_paths.is_empty() {
        return Err(BakerError::ValidationFailed {
            name: unit_name.to_string(),
            reason: "At least one video path must be provided for multi-action baking".into(),
        });
    }

    if video_paths.len() > DEFAULT_STATES.len() {
        return Err(BakerError::ValidationFailed {
            name: unit_name.to_string(),
            reason: format!(
                "{} videos supplied but only {DEFAULT_STATES:?} can be mapped by position",
                video_paths.len()
            ),
        });
    }

    let clips: Vec<(AnimState, &P)> = DEFAULT_STATES.into_iter().zip(video_paths).collect();

    bake_multi_action_videos_with_states(unit_name, &clips, output_dir, frame_size, fps)
}

/// Bakes multiple video clips corresponding to explicit animation states into a single
/// vertical strip PNG and `.flipbook.ron` manifest, centre-cropping every frame
/// ([`BakeFraming::CenterCrop`]).
///
/// Every clip is represented from start to end, and the manifest lists exactly the supplied
/// states. See the module docs and [`bake_framed_action_videos`], which this calls.
///
/// # Arguments
/// - `unit_name`: Semantic archetype name (e.g., `"marine"`, `"zealot"`).
/// - `clips`: Pairs of [`AnimState`] and source video path.
/// - `output_dir`: Directory where `<unit_name>.png` and `<unit_name>.flipbook.ron` will be created.
/// - `frame_size`: Square frame dimensions in pixels (e.g., 256 or 512).
/// - `fps`: Framerate for temporal downsampling.
///
/// # Errors
/// Returns [`BakerError`] if FFmpeg execution fails, image files cannot be read or written,
/// or validation invariants are violated.
pub fn bake_multi_action_videos_with_states<P: AsRef<Path>>(
    unit_name: &str,
    clips: &[(AnimState, P)],
    output_dir: &Path,
    frame_size: u32,
    fps: u32,
) -> Result<FlipbookAtlasMeta, BakerError> {
    let spec = FrameSpec {
        size: frame_size,
        fps,
        framing: BakeFraming::CenterCrop,
    };
    bake_framed_action_videos(unit_name, clips, output_dir, spec).map(|bake| bake.meta)
}

/// The frame geometry of an identity bake: square side, rate and framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSpec {
    /// Side of each square frame, in pixels (e.g., 256 or 512).
    pub size: u32,
    /// Playback rate after temporal downsampling, in frames per second.
    pub fps: u32,
    /// How each clip frame is fitted into the square.
    pub framing: BakeFraming,
}

/// The frame budget of each state in an atlas of `frame_size`-pixel frames holding `states`
/// states: `(MAX_ATLAS_STRIP_HEIGHT / frame_size) / states`, at least 1.
///
/// The bake keeps at most this many frames of each clip, and `loop-check` measures the same
/// number, so the review judges the frames the atlas will hold.
#[must_use]
pub fn frames_per_state(frame_size: u32, states: usize) -> usize {
    let total = (MAX_STRIP_HEIGHT / frame_size.max(1)) as usize;
    (total / states.max(1)).max(1)
}

/// The result of [`bake_framed_action_videos`] and [`bake_repaired_action_videos`].
#[derive(Debug, Clone, PartialEq)]
pub struct FramedBake {
    /// The manifest written beside the atlas.
    pub meta: FlipbookAtlasMeta,
    /// The bar colour chosen for each state's clip (sRGB bytes), `None` where no bar was drawn
    /// (a centre crop, or a clip that already fills the square).
    pub bar_rgb: BTreeMap<AnimState, Option<[u8; 3]>>,
    /// What the seam repair did to each state's frames, `None` for a state baked without one.
    pub seam: BTreeMap<AnimState, Option<SeamRecord>>,
}

/// Bakes the clips of explicit animation states into one vertical strip PNG and
/// `.flipbook.ron` manifest, fitting each frame into the square as `spec.framing` says.
///
/// Every clip is represented from start to end, and the manifest lists exactly the supplied
/// states. Under a letterbox each clip gets one bar colour, measured over the frames it keeps.
/// No state gets a seam repair; see [`bake_repaired_action_videos`] and the module docs.
///
/// # Arguments
/// - `unit_name`: Atlas name, the identity's catalog slug.
/// - `clips`: Pairs of [`AnimState`] and source video path.
/// - `output_dir`: Directory where `<unit_name>.png` and `<unit_name>.flipbook.ron` will be created.
/// - `spec`: Frame side in pixels, rate in frames per second, and framing.
///
/// # Errors
/// Returns [`BakerError`] if FFmpeg execution fails, image files cannot be read or written,
/// or validation invariants are violated.
pub fn bake_framed_action_videos<P: AsRef<Path>>(
    unit_name: &str,
    clips: &[(AnimState, P)],
    output_dir: &Path,
    spec: FrameSpec,
) -> Result<FramedBake, BakerError> {
    bake_repaired_action_videos(unit_name, clips, output_dir, spec, &BTreeMap::new())
}

/// [`bake_framed_action_videos`] with a seam repair for each looping state `seams` names
/// ([`decode_repaired_clip`]); the other states bake exactly as without it, and an entry for a
/// state no clip shows is ignored.
///
/// # Errors
/// As [`bake_framed_action_videos`]; [`BakerError::ValidationFailed`] for a seam on a state
/// that does not loop (Attack, Constructing, Die), and [`BakerError::SeamRepairRefused`] for a
/// clip the scale lock cannot repair. Nothing is written on error.
pub fn bake_repaired_action_videos<P: AsRef<Path>>(
    unit_name: &str,
    clips: &[(AnimState, P)],
    output_dir: &Path,
    spec: FrameSpec,
    seams: &BTreeMap<AnimState, SeamRepair>,
) -> Result<FramedBake, BakerError> {
    let (output_png_path, output_meta_path) = atlas_output_paths(output_dir, unit_name)?;
    let ffmpeg_path = PathBuf::from("/usr/bin/ffmpeg");
    if !ffmpeg_path.exists() {
        return Err(BakerError::FfmpegNotFound { path: ffmpeg_path });
    }
    if clips.is_empty() {
        return Err(BakerError::ValidationFailed {
            name: unit_name.to_string(),
            reason: "No action clips provided for multi-action baking".into(),
        });
    }
    if let Some(state) = seams.keys().find(|state| !atlas_loops(**state)) {
        return Err(BakerError::ValidationFailed {
            name: unit_name.to_string(),
            reason: format!("{state:?} does not loop, so it cannot have a seam repair"),
        });
    }

    let max_frames_per_clip = frames_per_state(spec.size, clips.len());

    let mut actions = BTreeMap::new();
    let mut bar_rgb = BTreeMap::new();
    let mut seam = BTreeMap::new();
    let mut all_frames: Vec<RgbaImage> = Vec::new();
    for (state, video_path_ref) in clips {
        // PERFORMANCE RATIONALE:
        // Why: WebGPU limits 2D texture dimensions to 16384 pixels.
        // What: We keep max_frames_per_clip = (16384 / frame_size) / K frames, sampled across each clip.
        // Trade-off: Guarantees total strip height <= 16384 px across any number of action clips,
        // at the cost of temporal resolution in clips that exceed their share.
        let repaired = decode_repaired_clip(
            &ffmpeg_path,
            video_path_ref.as_ref(),
            spec,
            max_frames_per_clip,
            &format!("{unit_name} {state:?}"),
            seams.get(state).copied(),
        )?;
        bar_rgb.insert(*state, repaired.framed.bar_rgb);
        seam.insert(*state, repaired.seam);
        let start = all_frames.len() as u32;
        let frames = repaired.framed.frames;
        actions.insert(*state, pack_action(*state, start, frames.len() as u32));
        all_frames.extend(frames);
    }

    let meta = FlipbookAtlasMeta {
        name: unit_name.to_string(),
        frame_width: spec.size,
        frame_height: spec.size,
        fps: spec.fps,
        total_frames: all_frames.len() as u32,
        layout: FlipbookLayout::VerticalStrip,
        actions,
    };
    write_atlas(
        &meta,
        &all_frames,
        output_dir,
        &output_png_path,
        &output_meta_path,
    )?;
    Ok(FramedBake {
        meta,
        bar_rgb,
        seam,
    })
}

/// Packs `frames` into the strip PNG and writes it and `meta` to their paths in `output_dir`.
///
/// # Errors
/// Returns [`BakerError`] if the strip is too tall or a file cannot be written.
fn write_atlas(
    meta: &FlipbookAtlasMeta,
    frames: &[RgbaImage],
    output_dir: &Path,
    png_path: &Path,
    meta_path: &Path,
) -> Result<(), BakerError> {
    let packed_strip = pack_strip(&meta.name, frames, meta.frame_width)?;
    std::fs::create_dir_all(output_dir).map_err(|source| BakerError::Io {
        path: output_dir.to_path_buf(),
        source,
    })?;
    packed_strip
        .save(png_path)
        .map_err(|source| BakerError::ImageProcessing {
            path: png_path.to_path_buf(),
            source,
        })?;
    meta.save_to_ron(meta_path)
}

/// Decodes one clip at `spec.size` and `spec.fps`, keeps at most `budget` frames spread over
/// the whole clip, and completes them to squares as `spec.framing` says.
///
/// This is the one path from a clip to shipped square frames: the bake and the reviewer's
/// framed contact sheet both use it, the bake through [`decode_repaired_clip`]. `label` names
/// the clip in the budget note.
///
/// # Errors
/// Returns [`BakerError::Io`] for a missing clip, FFmpeg errors, image errors, and
/// [`BakerError::ValidationFailed`] for frames that cannot be framed.
pub fn decode_framed_clip(
    ffmpeg_path: &Path,
    video_path: &Path,
    spec: FrameSpec,
    budget: usize,
    label: &str,
) -> Result<FramedFrames, BakerError> {
    decode_repaired_clip(ffmpeg_path, video_path, spec, budget, label, None).map(|clip| clip.framed)
}

/// One clip's square frames and the seam repair done on them.
#[derive(Debug, Clone, PartialEq)]
pub struct RepairedClip {
    /// The square frames and bar colour, as [`decode_framed_clip`] returns them.
    pub framed: FramedFrames,
    /// What the repair did, `None` when no seam was asked for.
    pub seam: Option<SeamRecord>,
    /// Width and height in pixels of the clip's own pixels inside each square frame, which
    /// [`frame_to_square`] centres; the rest are bars. Equal to the square under a centre
    /// crop.
    pub content_size: (u32, u32),
}

/// [`decode_framed_clip`], with the seam repair `seam` asks for (plan
/// `docs/plans/2026-11-04-veo-seamless-loops.md`, D-5).
///
/// Without a seam this is exactly [`decode_framed_clip`]. With one, on the kept frames and
/// before they are completed to squares:
/// 1. **Loop sampling:** the frames are chosen by [`evenly_spaced_loop_indices`].
/// 2. **Scale lock** (if `seam.scale_lock`): the clip is decoded at twice the size
///    ([`BakeFraming::ffmpeg_filter_supersampled`]), [`plan_scale_lock`] fits the camera zoom,
///    [`apply_scale_lock`] enlarges every frame to the most zoomed-in framing, and
///    [`downsample_half`] brings the frames to the frame size.
/// 3. **Cross-fade:** the last `seam.crossfade_frames` frames are blended toward frame 0
///    ([`crossfade_into_first`]).
///
/// At most `budget` frames are kept, as without a repair; when the whole clip fits the budget,
/// its final frame is left out.
///
/// # Errors
/// As [`decode_framed_clip`]; [`BakerError::SeamRepairRefused`] when the scale lock refuses
/// the clip, and [`BakerError::ValidationFailed`] for a cross-fade longer than allowed.
pub fn decode_repaired_clip(
    ffmpeg_path: &Path,
    video_path: &Path,
    spec: FrameSpec,
    budget: usize,
    label: &str,
    seam: Option<SeamRepair>,
) -> Result<RepairedClip, BakerError> {
    let Some(seam) = seam else {
        let filter = spec.framing.ffmpeg_filter(spec.size, spec.fps);
        let (temp_dir, frame_paths) = probe_clip(ffmpeg_path, video_path, &filter)?;
        let frames = load_frames(&fit_clip_to_budget(frame_paths, budget, label, spec.fps))?;
        drop(temp_dir);
        let content_size = content_size_of(&frames, spec.size);
        return Ok(RepairedClip {
            framed: frame_to_square(frames, spec.size, spec.framing)?,
            seam: None,
            content_size,
        });
    };
    let filter = if seam.scale_lock {
        spec.framing.ffmpeg_filter_supersampled(spec.size, spec.fps)
    } else {
        spec.framing.ffmpeg_filter(spec.size, spec.fps)
    };
    let (temp_dir, frame_paths) = probe_clip(ffmpeg_path, video_path, &filter)?;
    let kept = load_frames(&fit_loop_to_budget(&frame_paths, budget, label, spec.fps))?;
    drop(temp_dir);
    let (mut frames, scale_permille) = if seam.scale_lock {
        let plan = plan_scale_lock(&kept, label)?;
        let locked = apply_scale_lock(&kept, &plan);
        let frames = locked.iter().map(downsample_half).collect();
        (frames, plan.enlargement_permille())
    } else {
        (kept, 0)
    };
    crossfade_into_first(&mut frames, usize::from(seam.crossfade_frames)).map_err(|error| {
        match error {
            BakerError::ValidationFailed { reason, .. } => BakerError::ValidationFailed {
                name: label.to_string(),
                reason,
            },
            other => other,
        }
    })?;
    let content_size = content_size_of(&frames, spec.size);
    Ok(RepairedClip {
        content_size,
        framed: frame_to_square(frames, spec.size, spec.framing)?,
        seam: Some(SeamRecord {
            sampling: FrameSampling::Loop,
            scale_permille,
            crossfade_frames: seam.crossfade_frames,
        }),
    })
}

/// The size of the decoded frames, or the square when there are none.
fn content_size_of(frames: &[RgbaImage], frame_size: u32) -> (u32, u32) {
    frames
        .first()
        .map_or((frame_size, frame_size), RgbaImage::dimensions)
}

/// Reduces a looping clip's frames to at most `budget` with [`evenly_spaced_loop_indices`],
/// noting on stderr when frames are dropped beyond the final one.
fn fit_loop_to_budget(frames: &[PathBuf], budget: usize, label: &str, fps: u32) -> Vec<PathBuf> {
    let period = frames.len().saturating_sub(1);
    if period > budget {
        eprintln!(
            "    Note: '{label}' loops over {period} frames at {fps} fps but the texture budget allows {budget}. \
             Sampling evenly over the whole loop, which then plays up to {}x faster. \
             Use a smaller frame size to keep more frames.",
            period.div_ceil(budget.max(1))
        );
    }
    evenly_spaced_loop_indices(frames.len(), budget)
        .into_iter()
        .filter_map(|index| frames.get(index).cloned())
        .collect()
}

/// Checks that one action clip exists and decodes it into PNG frames through `filter`.
///
/// Returns the temporary directory that owns the decoded frames together with their paths in
/// temporal order; the frames are deleted when the directory is dropped.
///
/// # Errors
/// Returns [`BakerError::Io`] for a missing clip or temporary directory, and any error of
/// `extract_clip_frames`.
fn probe_clip(
    ffmpeg_path: &Path,
    video_path: &Path,
    filter: &str,
) -> Result<(tempfile::TempDir, Vec<PathBuf>), BakerError> {
    if !video_path.exists() {
        return Err(BakerError::Io {
            path: video_path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "Source video clip does not exist at {}",
                    video_path.display()
                ),
            ),
        });
    }

    let temp_dir = tempfile::tempdir().map_err(|source| BakerError::Io {
        path: PathBuf::from("tempdir"),
        source,
    })?;

    let frame_paths = extract_clip_frames(ffmpeg_path, video_path, filter, temp_dir.path())?;
    Ok((temp_dir, frame_paths))
}

/// Loads the decoded frames at `paths` as RGBA, in order.
///
/// # Errors
/// Returns [`BakerError::ImageProcessing`] if a decoded frame cannot be read.
fn load_frames(paths: &[PathBuf]) -> Result<Vec<RgbaImage>, BakerError> {
    paths
        .iter()
        .map(|path| {
            image::open(path)
                .map(|frame| frame.to_rgba8())
                .map_err(|source| BakerError::ImageProcessing {
                    path: path.clone(),
                    source,
                })
        })
        .collect()
}

/// Describes where one action's frames sit in the strip and how they play back.
///
/// `start_frame` is the action's first frame index in the strip and `frame_count` its length,
/// both in frames. Which states loop is [`atlas_loops`], the one classification the loop policy
/// (`media_style.ron`) is also checked against; deaths and construction hold their last frame,
/// and attacks play once.
fn pack_action(state: AnimState, start_frame: u32, frame_count: u32) -> ActionSlice {
    ActionSlice {
        start_frame,
        frame_count,
        looping: atlas_loops(state),
        hold_last_frame: matches!(state, AnimState::Die | AnimState::Constructing),
    }
}

/// Stacks square frames of `frame_size` pixels into one vertical strip, top to bottom.
///
/// # Errors
/// Returns [`BakerError::ValidationFailed`] naming `unit_name` if the strip would be taller than
/// the WebGPU 2D texture limit.
fn pack_strip(
    unit_name: &str,
    frames: &[RgbaImage],
    frame_size: u32,
) -> Result<RgbaImage, BakerError> {
    let total_frames = frames.len() as u32;
    let strip_height = frame_size.saturating_mul(total_frames);

    // PERFORMANCE RATIONALE:
    // Why: Standard WebGPU hardware drivers mandate max_texture_dimension_2d <= 16384 pixels.
    // What: We validate that the total height of the vertically stacked strip does not exceed 16384.
    // Trade-off: Guarantees 100% portable GPU texture allocation across all platforms.
    if strip_height > MAX_STRIP_HEIGHT {
        return Err(BakerError::ValidationFailed {
            name: unit_name.to_string(),
            reason: format!(
                "Total strip height ({strip_height} px for {total_frames} frames at {frame_size}x{frame_size}) exceeds WebGPU 2D texture limit of {MAX_STRIP_HEIGHT} px"
            ),
        });
    }

    let mut packed_strip = RgbaImage::new(frame_size, strip_height);
    for (i, frame_img) in frames.iter().enumerate() {
        let y_offset = (i as u32).saturating_mul(frame_size);
        image::imageops::overlay(&mut packed_strip, frame_img, 0, i64::from(y_offset));
    }
    Ok(packed_strip)
}

#[cfg(test)]
mod tests {
    use super::evenly_spaced_indices;

    #[test]
    fn indices_cover_first_and_last_frame() {
        assert_eq!(evenly_spaced_indices(60, 10)[0], 0);
        assert_eq!(evenly_spaced_indices(60, 10)[9], 59);
        assert_eq!(evenly_spaced_indices(5, 2), vec![0, 4]);
    }

    #[test]
    fn indices_are_strictly_increasing_and_evenly_spread() {
        let indices = evenly_spaced_indices(1000, 7);
        assert_eq!(indices.len(), 7);
        for pair in indices.windows(2) {
            let gap = pair[1] - pair[0];
            assert!((166..=167).contains(&gap), "uneven gap {gap}");
        }
    }

    #[test]
    fn nothing_is_dropped_within_budget() {
        assert_eq!(evenly_spaced_indices(3, 3), vec![0, 1, 2]);
        assert_eq!(evenly_spaced_indices(3, 9), vec![0, 1, 2]);
    }

    #[test]
    fn degenerate_requests_are_handled() {
        assert_eq!(evenly_spaced_indices(5, 1), vec![0]);
        assert_eq!(evenly_spaced_indices(5, 0), Vec::<usize>::new());
        assert_eq!(evenly_spaced_indices(0, 4), Vec::<usize>::new());
    }
}
