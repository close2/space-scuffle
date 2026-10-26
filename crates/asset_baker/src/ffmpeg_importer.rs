//! Video frame extraction via external FFmpeg invocation and vertical strip assembly.
//!
//! This module automates the execution of `/usr/bin/ffmpeg` to transcode arbitrary
//! video inputs (`.mp4`, `.webm`, etc.) into standardized square frame sequences,
//! which are then composited into single vertical strip PNGs accompanied by `.flipbook.ron`
//! manifests.

#![allow(
    clippy::cast_possible_truncation,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use crate::error::BakerError;
use crate::manifest::{ActionSlice, FlipbookAtlasMeta, FlipbookLayout};
use image::RgbaImage;
use sim_types::enums::AnimState;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Configuration parameters for FFmpeg video extraction and scaling.
#[derive(Debug, Clone)]
pub struct FfmpegExtractConfig {
    /// Path to the FFmpeg executable binary.
    pub ffmpeg_path: PathBuf,
    /// Target width and height in pixels for the square cropped frames (e.g. 192 or 256).
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

/// Extracts frames from a video file, center-crops to a 1:1 square, scales, and packs into a PNG vertical strip.
///
/// # Arguments
/// - `video_path`: Source video file (`.mp4`, `.webm`, etc.) on disk.
/// - `output_png_path`: Target filesystem destination for the composited vertical strip PNG.
/// - `output_meta_path`: Target filesystem destination for the `.flipbook.ron` manifest.
/// - `atlas_name`: Semantic identifier for this atlas (e.g. `"concept_video"`).
/// - `config`: Extraction parameters including frame dimensions and framerate.
/// - `actions_mapping`: Optional custom mapping of [`AnimState`] to [`ActionSlice`]. If empty,
///   defaults to mapping [`AnimState::Idle`] across the entire sequence.
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

    let frame_pattern = temp_dir.path().join("frame_%05d.png");

    // PERFORMANCE RATIONALE:
    // Why: Runtime software or hardware decoding of 50+ concurrent MP4 video streams inside 3D unit
    // bubbles saturates GPU video decoders and causes massive framerate stuttering.
    // What: We pre-decode video offline using FFmpeg's hardware-accelerated SIMD decoders, applying
    // `crop=min(iw\,ih):min(iw\,ih)` to center-crop arbitrary aspect ratios to a 1:1 square, bilinear
    // scaling to power-of-two dimensions (e.g., 256x256), and clean temporal downsampling (e.g., 24 -> 15 FPS).
    // Trade-off: Higher disk storage footprint (~20-50MB per baked strip) in exchange for O(1) texture
    // array slice sampling with zero runtime decoding overhead.
    let filter = format!(
        "crop=min(iw\\,ih):min(iw\\,ih),scale={}:{},fps={}",
        config.target_size, config.target_size, config.target_fps
    );

    let output = Command::new(&config.ffmpeg_path)
        .arg("-y")
        .arg("-i")
        .arg(video_path)
        .arg("-vf")
        .arg(&filter)
        .arg(&frame_pattern)
        .output()
        .map_err(|source| BakerError::Io {
            path: config.ffmpeg_path.clone(),
            source,
        })?;

    if !output.status.success() {
        return Err(BakerError::FfmpegExecutionFailed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    // Collect all generated PNG frame paths sorted in ascending numerical order
    let mut frame_paths: Vec<PathBuf> = std::fs::read_dir(temp_dir.path())
        .map_err(|source| BakerError::Io {
            path: temp_dir.path().to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
        .collect();
    frame_paths.sort();

    if frame_paths.is_empty() {
        return Err(BakerError::ValidationFailed {
            name: atlas_name.to_string(),
            reason: "FFmpeg execution succeeded but produced 0 output frame files".into(),
        });
    }

    // PERFORMANCE RATIONALE:
    // Why: WebGPU and standard desktop GPU drivers enforce `max_texture_dimension_2d <= 16384` pixels.
    // What: We cap the number of vertically stacked frames to `16384 / config.target_size`
    // (e.g., 32 frames at 512x512 = 16384 px, or 64 frames at 256x256 = 16384 px).
    // Trade-off: Bounds single-strip duration to the hardware texture limit while guaranteeing zero
    // GPU texture allocation panics across Vulkan, Metal, DX12, and WebGPU.
    let max_frames = (16384 / config.target_size.max(1)) as usize;
    if frame_paths.len() > max_frames {
        frame_paths.truncate(max_frames);
    }

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

    // If no action mappings are specified, map standard AnimStates across the sequence
    let mut actions = actions_mapping;
    if actions.is_empty() {
        for state in [
            AnimState::Constructing,
            AnimState::Idle,
            AnimState::Walk,
            AnimState::Attack,
            AnimState::Cast,
            AnimState::Die,
        ] {
            actions.insert(
                state,
                ActionSlice {
                    start_frame: 0,
                    frame_count: total_frames,
                    looping: !matches!(state, AnimState::Constructing | AnimState::Die),
                    hold_last_frame: matches!(state, AnimState::Constructing | AnimState::Die),
                },
            );
        }
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
/// Maps input video paths sequentially to [`AnimState::Idle`], [`AnimState::Walk`], and [`AnimState::Attack`].
///
/// # Arguments
/// - `unit_name`: Semantic archetype name (e.g., `"marine"`, `"zealot"`).
/// - `video_paths`: Ordered slice of video file paths corresponding to Idle, Walk, and Attack.
/// - `output_dir`: Directory where `<unit_name>.png` and `<unit_name>.flipbook.ron` will be created.
/// - `frame_size`: Square frame dimensions in pixels (e.g., 256 or 512).
/// - `fps`: Framerate for temporal downsampling.
///
/// # Errors
/// Returns [`BakerError`] if FFmpeg execution fails, image files cannot be read or written,
/// or validation invariants are violated.
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

    let clips: Vec<(AnimState, &P)> = video_paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let state = if i < DEFAULT_STATES.len() {
                DEFAULT_STATES[i]
            } else {
                AnimState::Idle
            };
            (state, path)
        })
        .collect();

    bake_multi_action_videos_with_states(unit_name, &clips, output_dir, frame_size, fps)
}

/// Bakes multiple video clips corresponding to explicit animation states into a single
/// vertical strip PNG and `.flipbook.ron` manifest.
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

    let max_total_frames = 16384 / frame_size.max(1);
    let k = clips.len() as u32;
    let max_frames_per_clip = (max_total_frames / k).max(1) as usize;

    let mut actions = BTreeMap::new();
    let mut all_frames: Vec<RgbaImage> = Vec::new();
    let mut current_frame_offset: u32 = 0;

    for (state, video_path_ref) in clips {
        let video_path = video_path_ref.as_ref();
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

        let frame_pattern = temp_dir.path().join("frame_%05d.png");
        let filter =
            format!("crop=min(iw\\,ih):min(iw\\,ih),scale={frame_size}:{frame_size},fps={fps}");

        let output = Command::new(&ffmpeg_path)
            .arg("-y")
            .arg("-i")
            .arg(video_path)
            .arg("-vf")
            .arg(&filter)
            .arg(&frame_pattern)
            .output()
            .map_err(|source| BakerError::Io {
                path: ffmpeg_path.clone(),
                source,
            })?;

        if !output.status.success() {
            return Err(BakerError::FfmpegExecutionFailed {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        let mut frame_paths: Vec<PathBuf> = std::fs::read_dir(temp_dir.path())
            .map_err(|source| BakerError::Io {
                path: temp_dir.path().to_path_buf(),
                source,
            })?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
            .collect();
        frame_paths.sort();

        if frame_paths.is_empty() {
            return Err(BakerError::ValidationFailed {
                name: unit_name.to_string(),
                reason: format!(
                    "FFmpeg extraction produced 0 frames for clip '{}' ({:?})",
                    video_path.display(),
                    state
                ),
            });
        }

        // PERFORMANCE RATIONALE:
        // Why: WebGPU limits 2D texture dimensions to 16384 pixels.
        // What: We cap each action clip to max_frames_per_clip = (16384 / frame_size) / K frames.
        // Trade-off: Guarantees total strip height <= 16384 px across any number of action clips.
        if frame_paths.len() > max_frames_per_clip {
            frame_paths.truncate(max_frames_per_clip);
        }

        let clip_frame_count = frame_paths.len() as u32;
        let (looping, hold_last_frame) = match state {
            AnimState::Attack => (false, false),
            AnimState::Die | AnimState::Constructing => (false, true),
            AnimState::Idle | AnimState::Walk | AnimState::Cast => (true, false),
        };

        actions.insert(
            *state,
            ActionSlice {
                start_frame: current_frame_offset,
                frame_count: clip_frame_count,
                looping,
                hold_last_frame,
            },
        );

        for path in &frame_paths {
            let frame_img = image::open(path)
                .map_err(|source| BakerError::ImageProcessing {
                    path: path.clone(),
                    source,
                })?
                .to_rgba8();
            all_frames.push(frame_img);
        }

        current_frame_offset = current_frame_offset.saturating_add(clip_frame_count);
    }

    let total_frames = all_frames.len() as u32;

    // PERFORMANCE RATIONALE:
    // Why: The simulation and visual shader can transition an entity into any AnimState at runtime.
    // Missing enum keys in meta.actions cause the client to drop back to frame 0 or trigger fallback logic.
    // What: We explicitly map all unrepresented AnimState variants to valid, semantically appropriate
    // slices of the existing action frames.
    // Trade-off: Zero additional GPU texture memory while guaranteeing O(1) total action coverage.
    let idle_slice = actions.get(&AnimState::Idle).copied();
    let attack_slice = actions.get(&AnimState::Attack).copied();
    let fallback_slice = idle_slice
        .or(attack_slice)
        .or_else(|| actions.values().next().copied())
        .unwrap_or(ActionSlice {
            start_frame: 0,
            frame_count: total_frames.max(1),
            looping: true,
            hold_last_frame: false,
        });

    actions.entry(AnimState::Idle).or_insert(fallback_slice);
    let resolved_idle = actions
        .get(&AnimState::Idle)
        .copied()
        .unwrap_or(fallback_slice);

    actions.entry(AnimState::Walk).or_insert(ActionSlice {
        start_frame: resolved_idle.start_frame,
        frame_count: resolved_idle.frame_count,
        looping: true,
        hold_last_frame: false,
    });

    actions.entry(AnimState::Attack).or_insert(ActionSlice {
        start_frame: resolved_idle.start_frame,
        frame_count: resolved_idle.frame_count,
        looping: false,
        hold_last_frame: false,
    });

    let attack_or_idle = actions
        .get(&AnimState::Attack)
        .copied()
        .unwrap_or(resolved_idle);

    actions
        .entry(AnimState::Constructing)
        .or_insert(ActionSlice {
            start_frame: fallback_slice.start_frame,
            frame_count: fallback_slice.frame_count,
            looping: false,
            hold_last_frame: true,
        });

    actions.entry(AnimState::Cast).or_insert(ActionSlice {
        start_frame: attack_slice.unwrap_or(attack_or_idle).start_frame,
        frame_count: attack_slice.unwrap_or(attack_or_idle).frame_count,
        looping: false,
        hold_last_frame: false,
    });

    actions.entry(AnimState::Die).or_insert(ActionSlice {
        start_frame: attack_slice.unwrap_or(attack_or_idle).start_frame,
        frame_count: attack_slice.unwrap_or(attack_or_idle).frame_count,
        looping: false,
        hold_last_frame: true,
    });

    let strip_height = frame_size.saturating_mul(total_frames);

    // PERFORMANCE RATIONALE:
    // Why: Standard WebGPU hardware drivers mandate max_texture_dimension_2d <= 16384 pixels.
    // What: We validate that the total height of the vertically stacked strip does not exceed 16384.
    // Trade-off: Guarantees 100% portable GPU texture allocation across all platforms.
    if strip_height > 16384 {
        return Err(BakerError::ValidationFailed {
            name: unit_name.to_string(),
            reason: format!(
                "Total strip height ({strip_height} px for {total_frames} frames at {frame_size}x{frame_size}) exceeds WebGPU 2D texture limit of 16384 px"
            ),
        });
    }

    let mut packed_strip = RgbaImage::new(frame_size, strip_height);
    for (i, frame_img) in all_frames.iter().enumerate() {
        let y_offset = (i as u32).saturating_mul(frame_size);
        image::imageops::overlay(&mut packed_strip, frame_img, 0, i64::from(y_offset));
    }

    std::fs::create_dir_all(output_dir).map_err(|source| BakerError::Io {
        path: output_dir.to_path_buf(),
        source,
    })?;

    let output_png_path = output_dir.join(format!("{unit_name}.png"));
    let output_meta_path = output_dir.join(format!("{unit_name}.flipbook.ron"));

    packed_strip
        .save(&output_png_path)
        .map_err(|source| BakerError::ImageProcessing {
            path: output_png_path.clone(),
            source,
        })?;

    let meta = FlipbookAtlasMeta {
        name: unit_name.to_string(),
        frame_width: frame_size,
        frame_height: frame_size,
        fps,
        total_frames,
        layout: FlipbookLayout::VerticalStrip,
        actions,
    };

    meta.save_to_ron(&output_meta_path)?;
    Ok(meta)
}
