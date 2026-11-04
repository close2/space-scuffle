//! Keyframes, contact sheets, review verdicts, installation and import of Veo clips (plan
//! `2026-11-04-phase-8-5-veo-media.md`, decisions D-5 and D-7).
//!
//! # Responsibility
//! - [`extract_keyframe`]: one frame of a clip at a given time, as a PNG. The run uses the
//!   Idle clip's frame at [`CHAIN_KEYFRAME_TIME`] (its first frame) as the image input of the
//!   identity's other states, so that they show the same design.
//! - [`contact_sheet`]: eight frames of a clip side by side, full size for the reviewer and a
//!   copy at most [`EVIDENCE_SHEET_MAX_WIDTH`] pixels wide for the committed evidence.
//! - [`framed_sheet`]: the same eight moments as the square game frames that ship, built by the
//!   bake's own decode and framing path (`ffmpeg_importer::decode_framed_clip`) with the
//!   identity's framing from `media_style.ron`, so the review judges exactly what ships.
//! - [`loop_check`]: the measured loop-continuity criterion (plan
//!   `2026-11-04-veo-seamless-loops.md`, D-6). It decodes a looping state's clip the way its
//!   bake will (frame budget, framing, loop sampling and seam repair), and measures the wrap,
//!   the median and worst steps (SSIM) and the camera zoom against frame 0, raw and repaired,
//!   plus a pinned clip's start fidelity; [`LoopCheck::failures`] applies the thresholds
//!   ([`LOOP_STEP_MARGIN`], [`LOOP_MAX_GROWTH`], [`START_FIDELITY_MIN`]) to the repaired frames,
//!   and [`loop_sheets`] writes the repaired and seam sheets for the visual check.
//! - [`reject_clip`]: records a reviewer's rejection in the ledger.
//! - [`install_clip`]: copies an accepted clip into `assets/videos`, refusing any clip the
//!   ledger did not record as downloaded or that was rejected.
//! - [`import_clip`]: records a clip fetched by hand (a Cloud Storage result) against the
//!   operation that produced it, so that it can be reviewed and installed like a download.
//!
//! # How it fits
//! These are the steps between a paid download (`veo_run`) and baking (`bake-unit-videos`):
//! review, then install. None of them makes a network request.
//!
//! # Invariants
//! - Every clip is validated with ffprobe before it is recorded or installed.
//! - A clip reaches `assets/videos` only by an atomic rename, and never replaces an installed
//!   clip unless asked to.

use crate::error::BakerError;
use crate::ffmpeg_importer::{
    decode_framed_clip, decode_repaired_clip, evenly_spaced_indices, probe_video, FrameSpec,
    RepairedClip, VideoProbe,
};
use crate::ledger::{sha256_file, Ledger, LedgerEvent, LedgerHistory};
use crate::loop_metrics::{ssim, SeamRepair, ZoomReference};
use crate::provenance::SeamRecord;
use crate::veo_plan::{state_slug, VeoDirs};
use image::RgbaImage;
use sim_types::enums::AnimState;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// The FFmpeg executable the pipeline uses.
pub const DEFAULT_FFMPEG_PATH: &str = "/usr/bin/ffmpeg";

/// Time of the Idle frame that seeds the other states' keyframes: the first frame (plan
/// `2026-11-04-veo-seamless-loops.md`, D-3). It is the picture the Idle loop restarts on, so a
/// state pinned to it (Constructing) ends exactly where Idle begins; and it is a 16:9 frame of
/// a real clip, so it passes the pin rules.
pub const CHAIN_KEYFRAME_TIME: Duration = Duration::ZERO;

/// Frames per contact sheet.
pub const CONTACT_SHEET_FRAMES: u32 = 8;

/// Largest width of the committed copy of a contact sheet, in pixels.
pub const EVIDENCE_SHEET_MAX_WIDTH: u32 = 1024;

/// Runs FFmpeg on local files only and reports a failure with its stderr.
fn run_ffmpeg(ffmpeg: &Path, arguments: &[std::ffi::OsString]) -> Result<(), BakerError> {
    if !ffmpeg.exists() {
        return Err(BakerError::FfmpegNotFound {
            path: ffmpeg.to_path_buf(),
        });
    }
    let output = Command::new(ffmpeg)
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-protocol_whitelist",
            "file",
        ])
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .map_err(|source| BakerError::Io {
            path: ffmpeg.to_path_buf(),
            source,
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(BakerError::FfmpegExecutionFailed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// Removes a stale output so that a file FFmpeg did not write cannot pass for its result.
fn remove_stale(path: &Path) -> Result<(), BakerError> {
    match std::fs::remove_file(path) {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => Err(BakerError::Io {
            path: path.to_path_buf(),
            source,
        }),
        Ok(()) | Err(_) => Ok(()),
    }
}

/// Checks that `path` decodes as an image, so that FFmpeg's silence is not mistaken for success.
fn require_image(path: &Path, what: &str) -> Result<image::DynamicImage, BakerError> {
    image::open(path).map_err(|source| match source {
        image::ImageError::IoError(io) => BakerError::ValidationFailed {
            name: path.display().to_string(),
            reason: format!("{what} was not written: {io}"),
        },
        source => BakerError::ImageProcessing {
            path: path.to_path_buf(),
            source,
        },
    })
}

/// Writes the frame of `clip` at time `at` to `output` as a PNG.
///
/// # Errors
/// [`BakerError::FfmpegNotFound`], [`BakerError::FfmpegExecutionFailed`], or
/// [`BakerError::ValidationFailed`] if no frame exists at `at` (past the end of the clip).
pub fn extract_keyframe(
    ffmpeg: &Path,
    clip: &Path,
    at: Duration,
    output: &Path,
) -> Result<(), BakerError> {
    remove_stale(output)?;
    run_ffmpeg(
        ffmpeg,
        &[
            "-ss".into(),
            format!("{:.3}", at.as_secs_f64()).into(),
            "-i".into(),
            clip.into(),
            "-frames:v".into(),
            "1".into(),
            "-update".into(),
            "1".into(),
            output.into(),
        ],
    )?;
    require_image(output, "the keyframe").map(|_| ())
}

/// Writes a contact sheet of [`CONTACT_SHEET_FRAMES`] frames of `clip`, evenly spaced over its
/// duration and tiled in one row, to `full`, and a copy at most [`EVIDENCE_SHEET_MAX_WIDTH`]
/// pixels wide to `small`. Returns the clip's probe.
///
/// # Errors
/// Validation, FFmpeg and image errors.
pub fn contact_sheet(
    ffmpeg: &Path,
    ffprobe: &Path,
    clip: &Path,
    full: &Path,
    small: &Path,
) -> Result<VideoProbe, BakerError> {
    let probe = probe_video(ffprobe, clip)?;
    if probe.duration_seconds <= 0.0 {
        return Err(BakerError::VideoValidationFailed {
            path: clip.to_path_buf(),
            reason: "the clip has no duration".to_string(),
        });
    }
    for path in [full, small] {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| BakerError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    let rate = f64::from(CONTACT_SHEET_FRAMES) / probe.duration_seconds;
    remove_stale(full)?;
    run_ffmpeg(
        ffmpeg,
        &[
            "-i".into(),
            clip.into(),
            "-vf".into(),
            format!("fps={rate:.6},tile={CONTACT_SHEET_FRAMES}x1").into(),
            "-frames:v".into(),
            "1".into(),
            "-update".into(),
            "1".into(),
            full.into(),
        ],
    )?;
    let sheet = require_image(full, "the contact sheet")?;
    save_evidence_copy(sheet, small)?;
    Ok(probe)
}

/// Saves `sheet` to `small`, downscaled to at most [`EVIDENCE_SHEET_MAX_WIDTH`] pixels wide.
fn save_evidence_copy(sheet: image::DynamicImage, small: &Path) -> Result<(), BakerError> {
    let copy = if sheet.width() > EVIDENCE_SHEET_MAX_WIDTH {
        let height = u64::from(sheet.height()) * u64::from(EVIDENCE_SHEET_MAX_WIDTH)
            / u64::from(sheet.width());
        let height = u32::try_from(height.max(1)).unwrap_or(u32::MAX);
        sheet.resize_exact(
            EVIDENCE_SHEET_MAX_WIDTH,
            height,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        sheet
    };
    copy.save(small)
        .map_err(|source| BakerError::ImageProcessing {
            path: small.to_path_buf(),
            source,
        })
}

/// Writes the framed contact sheet of `clip`: [`CONTACT_SHEET_FRAMES`] square frames, evenly
/// spaced over the clip, decoded and framed exactly as `bake-unit-videos` does with `spec`, and
/// tiled in one row. The full sheet goes to `full` and a copy at most
/// [`EVIDENCE_SHEET_MAX_WIDTH`] pixels wide to `small`. Returns the bar colour chosen, if any.
///
/// The bar colour is measured over every frame decoded at `spec.fps`, as a bake of the clip
/// alone would; a bake that shares the strip with other states keeps fewer frames, so its
/// colour can differ by a unit or two.
///
/// # Errors
/// Validation, FFmpeg and image errors.
pub fn framed_sheet(
    ffmpeg: &Path,
    clip: &Path,
    spec: FrameSpec,
    full: &Path,
    small: &Path,
) -> Result<Option<[u8; 3]>, BakerError> {
    let framed = decode_framed_clip(ffmpeg, clip, spec, usize::MAX, "framed review")?;
    let picks = evenly_spaced_indices(framed.frames.len(), CONTACT_SHEET_FRAMES as usize);
    let sheet = tile_row(&framed.frames, &picks);
    for path in [full, small] {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| BakerError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    sheet
        .save(full)
        .map_err(|source| BakerError::ImageProcessing {
            path: full.to_path_buf(),
            source,
        })?;
    save_evidence_copy(image::DynamicImage::ImageRgba8(sheet), small)?;
    Ok(framed.bar_rgb)
}

/// The clip a reviewer looks at for `slug`'s `state`: the download awaiting review, else the
/// installed clip.
#[must_use]
pub fn review_target(
    history: &LedgerHistory,
    dirs: &VeoDirs,
    slug: &str,
    state: AnimState,
) -> Option<PathBuf> {
    history
        .clip(slug, state)
        .and_then(|clip| clip.downloaded.as_ref())
        .map(|download| download.path.clone())
        .filter(|path| path.exists())
        .or_else(|| Some(dirs.installed_clip(slug, state)).filter(|path| path.exists()))
}

/// Records the rejection of the download awaiting review for `slug`'s `state`.
///
/// # Errors
/// [`BakerError::NothingToReview`] if no download awaits review, or ledger errors.
pub fn reject_clip(
    history: &LedgerHistory,
    ledger: &mut Ledger,
    slug: &str,
    state: AnimState,
    reason: &str,
) -> Result<String, BakerError> {
    let download = history
        .clip(slug, state)
        .and_then(|clip| clip.downloaded.as_ref())
        .ok_or_else(|| BakerError::NothingToReview {
            slug: slug.to_string(),
            state: state_slug(state).to_string(),
        })?;
    ledger.append(LedgerEvent::RejectedInReview {
        slug: slug.to_string(),
        state,
        sha256: download.sha256.clone(),
        reason: reason.to_string(),
    })?;
    Ok(download.sha256.clone())
}

/// Copies `src` to `dst` through a temporary file and an atomic rename.
fn atomic_copy(src: &Path, dst: &Path, replace: bool) -> Result<(), BakerError> {
    let parent = dst.parent().unwrap_or(Path::new("."));
    let io = |path: &Path, source| BakerError::Io {
        path: path.to_path_buf(),
        source,
    };
    std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
    let temp = tempfile::Builder::new()
        .prefix(".install-")
        .suffix(".part")
        .tempfile_in(parent)
        .map_err(|e| io(parent, e))?;
    std::fs::copy(src, temp.path()).map_err(|e| io(src, e))?;
    let persisted = if replace {
        temp.persist(dst)
    } else {
        temp.persist_noclobber(dst)
    };
    persisted.map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            BakerError::ClipAlreadyInstalled {
                path: dst.to_path_buf(),
            }
        } else {
            io(dst, error.error)
        }
    })?;
    Ok(())
}

/// Installs `clip` as `slug`'s `state` clip in the videos directory after review.
///
/// # Errors
/// [`BakerError::VideoValidationFailed`] for an invalid video, [`BakerError::ClipRejected`]
/// or [`BakerError::ClipNotInLedger`] if the ledger does not vouch for it, and
/// [`BakerError::ClipAlreadyInstalled`] unless `replace`.
pub fn install_clip(
    ffprobe: &Path,
    history: &LedgerHistory,
    dirs: &VeoDirs,
    slug: &str,
    state: AnimState,
    clip: &Path,
    replace: bool,
) -> Result<PathBuf, BakerError> {
    probe_video(ffprobe, clip)?;
    let sha256 = sha256_file(clip)?;
    let recorded = history.clip(slug, state);
    if recorded.is_some_and(|c| c.rejected_sha256.contains(&sha256)) {
        return Err(BakerError::ClipRejected {
            path: clip.to_path_buf(),
        });
    }
    if !recorded.is_some_and(|c| c.downloaded_sha256.contains(&sha256)) {
        return Err(BakerError::ClipNotInLedger {
            path: clip.to_path_buf(),
            slug: slug.to_string(),
            state: state_slug(state).to_string(),
        });
    }
    let target = dirs.installed_clip(slug, state);
    if !replace && target.exists() {
        return Err(BakerError::ClipAlreadyInstalled { path: target });
    }
    atomic_copy(clip, &target, replace)?;
    Ok(target)
}

/// Records `file`, fetched by hand from `operation`'s result, as `slug`'s `state` download
/// and copies it into the clips directory for review.
///
/// # Errors
/// [`BakerError::OperationNotInLedger`] if the ledger never named `operation` for this clip,
/// validation errors, or ledger errors.
pub fn import_clip(
    ffprobe: &Path,
    history: &LedgerHistory,
    ledger: &mut Ledger,
    dirs: &VeoDirs,
    clip: (&str, AnimState),
    file: &Path,
    operation: &str,
) -> Result<PathBuf, BakerError> {
    let (slug, state) = clip;
    let named = history
        .clip(slug, state)
        .is_some_and(|c| c.named_operations.contains(operation));
    if !named {
        return Err(BakerError::OperationNotInLedger {
            operation: operation.to_string(),
            slug: slug.to_string(),
            state: state_slug(state).to_string(),
        });
    }
    let probe = probe_video(ffprobe, file)?;
    let target = dirs.clip_dir(slug, state).join(format!(
        "{}-imported.mp4",
        crate::veo_run::operation_stem(operation)
    ));
    atomic_copy(file, &target, true)?;
    let bytes = std::fs::metadata(&target)
        .map_err(|source| BakerError::Io {
            path: target.clone(),
            source,
        })?
        .len();
    ledger.append(LedgerEvent::Downloaded {
        slug: slug.to_string(),
        state,
        operation: operation.to_string(),
        bytes,
        sha256: sha256_file(&target)?,
        duration_seconds: probe.duration_seconds,
        frames: probe.packets,
        width: probe.width,
        height: probe.height,
        path: target.display().to_string(),
    })?;
    Ok(target)
}

// --- Loop check (loops plan D-6) ------------------------------------------------------------

/// Largest drop of the worst step of a loop below its median step that still passes:
/// `worst ≥ step − LOOP_STEP_MARGIN` (SSIM units).
pub const LOOP_STEP_MARGIN: f64 = 0.03;

/// Largest camera zoom of any kept frame against frame 0 that still passes, as a fraction
/// (0.015 is 1.5 %).
pub const LOOP_MAX_GROWTH: f64 = 0.015;

/// Smallest SSIM between a pinned clip's frame 0 and its pin picture that still passes.
pub const START_FIDELITY_MIN: f64 = 0.90;

/// Frames on the repaired contact sheet that `loop-check` writes.
pub const LOOP_SHEET_FRAMES: usize = 8;

/// Frames before the wrap shown on the seam sheet, which then shows frame 0.
pub const SEAM_SHEET_TAIL: usize = 4;

/// What `loop-check` measures and how: one state's clip, decoded the way its bake will.
#[derive(Debug, Clone, Copy)]
pub struct LoopCheckRequest<'a> {
    /// The clip to measure.
    pub clip: &'a Path,
    /// The bake geometry: square side in pixels, rate in frames per second, framing.
    pub spec: FrameSpec,
    /// The state's frame budget in the atlas (`ffmpeg_importer::frames_per_state`).
    pub budget: usize,
    /// The state's seam repair from the loop policy, `None` for a state baked without one.
    pub seam: Option<SeamRepair>,
    /// The picture the clip was pinned to start (and end) on, if it was pinned.
    pub pin: Option<&'a Path>,
    /// Names the clip in notes and errors.
    pub label: &'a str,
}

/// The loop measures of one set of kept frames (plan D-6). Every SSIM value is the mean
/// luminance SSIM of [`ssim`], in −1 to 1.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopMeasures {
    /// Number of kept frames, which is the number of steps of one cycle (the wrap included).
    pub frames: usize,
    /// SSIM of the last kept frame and the first: the step shown when the loop restarts.
    pub wrap: f64,
    /// Median SSIM of the steps between consecutive kept frames, the wrap excluded (the mean
    /// of the two middle values for an even count).
    pub step: f64,
    /// Lowest SSIM of all steps of the cycle, the wrap included.
    pub worst: f64,
    /// The step `worst` was measured on: step `k` goes from kept frame `k` to `k + 1`, and
    /// step `frames − 1` is the wrap.
    pub worst_at: usize,
    /// The camera zoom of the kept frame that departs most from frame 0, as growth
    /// `magnification − 1` (signed: above 0 is a push-in).
    pub growth: f64,
    /// The kept frame `growth` was measured on.
    pub growth_at: usize,
}

/// Why a loop fails the D-6 criterion.
#[derive(Debug, Clone, PartialEq)]
pub enum LoopFailure {
    /// A step of the cycle (often the wrap) is much less alike than a typical step.
    Worst {
        /// The worst step's SSIM.
        worst: f64,
        /// The median step's SSIM.
        step: f64,
    },
    /// A kept frame is zoomed against frame 0 by more than [`LOOP_MAX_GROWTH`].
    Growth {
        /// The largest growth, as a fraction.
        growth: f64,
    },
    /// A pinned clip's frame 0 is not close enough to its pin picture.
    StartFidelity {
        /// SSIM of frame 0 and the pin.
        fidelity: f64,
    },
    /// The bake's scale lock refuses the clip, so it cannot be baked at all.
    RepairRefused {
        /// The refusal, as the bake reports it.
        reason: String,
    },
}

impl std::fmt::Display for LoopFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Worst { worst, step } => write!(
                f,
                "worst {worst:.4} < step {step:.4} - {LOOP_STEP_MARGIN} = {:.4}",
                step - LOOP_STEP_MARGIN
            ),
            Self::Growth { growth } => write!(
                f,
                "growth {:+.2} % beyond ±{:.2} %",
                growth * 100.0,
                LOOP_MAX_GROWTH * 100.0
            ),
            Self::StartFidelity { fidelity } => {
                write!(f, "start fidelity {fidelity:.4} < {START_FIDELITY_MIN:.2}")
            }
            Self::RepairRefused { reason } => write!(f, "repair refused: {reason}"),
        }
    }
}

impl LoopMeasures {
    /// The D-6 failures of these frames: `worst ≥ step − LOOP_STEP_MARGIN`, `|growth| ≤
    /// LOOP_MAX_GROWTH`, and for a pinned clip `start_fidelity ≥ START_FIDELITY_MIN`. Empty
    /// when they pass.
    #[must_use]
    pub fn failures(&self, start_fidelity: Option<f64>) -> Vec<LoopFailure> {
        let mut failures = Vec::new();
        if self.worst < self.step - LOOP_STEP_MARGIN {
            failures.push(LoopFailure::Worst {
                worst: self.worst,
                step: self.step,
            });
        }
        if self.growth.abs() > LOOP_MAX_GROWTH {
            failures.push(LoopFailure::Growth {
                growth: self.growth,
            });
        }
        failures.extend(fidelity_failure(start_fidelity));
        failures
    }
}

/// The start-fidelity failure, if a pinned clip's fidelity is under the minimum.
fn fidelity_failure(start_fidelity: Option<f64>) -> Option<LoopFailure> {
    start_fidelity
        .filter(|fidelity| *fidelity < START_FIDELITY_MIN)
        .map(|fidelity| LoopFailure::StartFidelity { fidelity })
}

/// The frames the atlas holds after the state's seam repair, measured, or the repair's
/// refusal.
#[derive(Debug, Clone, PartialEq)]
pub enum RepairOutcome {
    /// The repaired frames (or, for a state without a seam, the frames its bake keeps).
    Measured {
        /// Their measures.
        measures: LoopMeasures,
        /// What the repair did, `None` for a state without a seam.
        seam: Option<SeamRecord>,
    },
    /// The bake's scale lock refuses the clip (`BakerError::SeamRepairRefused`).
    Refused {
        /// The refusal message.
        reason: String,
    },
}

/// The result of [`loop_check`]: the measures raw and repaired, and the start fidelity.
#[derive(Debug, Clone)]
pub struct LoopCheck {
    /// Loop sampling only: the kept frames as decoded, no scale lock, no cross-fade.
    pub raw: LoopMeasures,
    /// The frames the atlas will hold, after the state's seam repair.
    pub repaired: RepairOutcome,
    /// SSIM of the clip's decoded frame 0 and the pin picture, framed the same way; `None`
    /// for a clip that was not pinned. It is a property of the generation, measured before any
    /// repair, and judged with both sets of frames.
    pub start_fidelity: Option<f64>,
    /// The square frames the verdict is about (repaired, or raw when the repair refused the
    /// clip), for the sheets.
    pub judged_frames: Vec<RgbaImage>,
}

impl LoopCheck {
    /// The D-6 failures of the repaired frames. Empty when the clip passes.
    #[must_use]
    pub fn failures(&self) -> Vec<LoopFailure> {
        match &self.repaired {
            RepairOutcome::Measured { measures, .. } => measures.failures(self.start_fidelity),
            RepairOutcome::Refused { reason } => {
                let mut failures = vec![LoopFailure::RepairRefused {
                    reason: reason.clone(),
                }];
                failures.extend(fidelity_failure(self.start_fidelity));
                failures
            }
        }
    }

    /// Whether the repaired frames pass D-6.
    #[must_use]
    pub fn passes(&self) -> bool {
        self.failures().is_empty()
    }

    /// The report `loop-check` and `review` print: one line raw, one repaired, the start
    /// fidelity and the verdict (which judges the repaired frames), each indented by two
    /// spaces.
    #[must_use]
    pub fn render(&self) -> String {
        let raw = format!(
            "  raw:      {}  {}",
            render_measures(&self.raw),
            render_verdict(&self.raw.failures(self.start_fidelity))
        );
        let repaired = match &self.repaired {
            RepairOutcome::Measured { measures, seam } => format!(
                "  repaired: {}  {}  {}",
                render_measures(measures),
                seam.map_or_else(
                    || "(no seam repair: the bake's own sampling)".to_string(),
                    |seam| format!(
                        "(scale lock {} per mille, cross-fade {})",
                        seam.scale_permille, seam.crossfade_frames
                    )
                ),
                render_verdict(&measures.failures(self.start_fidelity))
            ),
            RepairOutcome::Refused { reason } => {
                format!("  repaired: REFUSED by the bake: {reason}")
            }
        };
        let fidelity = self.start_fidelity.map_or_else(
            || "  start fidelity: not pinned".to_string(),
            |fidelity| format!("  start fidelity: {fidelity:.4} (min {START_FIDELITY_MIN:.2})"),
        );
        let verdict = format!("  verdict: {}", render_verdict(&self.failures()));
        [raw, repaired, fidelity, verdict].join("\n")
    }
}

/// One line of measures: frames, wrap, step, worst and where, growth and where.
fn render_measures(m: &LoopMeasures) -> String {
    let at = if m.worst_at + 1 == m.frames {
        "wrap".to_string()
    } else {
        format!("step {}->{}", m.worst_at, m.worst_at + 1)
    };
    format!(
        "{} frames  wrap {:.4}  step {:.4}  worst {:.4} ({at})  growth {:+.2} % (frame {})",
        m.frames,
        m.wrap,
        m.step,
        m.worst,
        m.growth * 100.0,
        m.growth_at
    )
}

/// `PASS`, or `FAIL (` the failures `)`.
fn render_verdict(failures: &[LoopFailure]) -> String {
    if failures.is_empty() {
        "PASS".to_string()
    } else {
        let reasons: Vec<String> = failures.iter().map(ToString::to_string).collect();
        format!("FAIL ({})", reasons.join("; "))
    }
}

/// Measures one clip's loop continuity on the frames the atlas will hold (loops plan D-6).
///
/// The clip is decoded through the bake's own path (`ffmpeg_importer::decode_repaired_clip`)
/// at `request.spec`, keeping at most `request.budget` frames, twice: raw (loop sampling only)
/// and repaired (the state's seam, as the bake applies it). Every measure is taken over the
/// clip's own pixels inside each square frame: the bars of a letterbox are the same in every
/// frame and would only pull every SSIM towards 1. For a pinned clip, the pin picture is
/// framed the same way and compared with the raw frame 0.
///
/// # Errors
/// FFmpeg, image and validation errors, for example a pin whose aspect ratio differs from the
/// clip's. A clip the scale lock refuses is not an error: it is [`RepairOutcome::Refused`].
pub fn loop_check(ffmpeg: &Path, request: &LoopCheckRequest<'_>) -> Result<LoopCheck, BakerError> {
    let raw_seam = SeamRepair {
        scale_lock: false,
        crossfade_frames: 0,
    };
    let decode = |clip: &Path, budget: usize, seam: Option<SeamRepair>| {
        decode_repaired_clip(ffmpeg, clip, request.spec, budget, request.label, seam)
    };
    let raw_clip = decode(request.clip, request.budget, Some(raw_seam))?;
    let raw_content = content_frames(&raw_clip, request.spec.size);
    let raw = measure_loop(&raw_content)?;
    let start_fidelity = match request.pin {
        Some(pin) => {
            let framed_pin = decode(pin, 1, Some(raw_seam))?;
            let pin_content = content_frames(&framed_pin, request.spec.size);
            match (raw_content.first(), pin_content.first()) {
                (Some(start), Some(pin)) => Some(ssim(start, pin)?),
                _ => None,
            }
        }
        None => None,
    };
    let (repaired, judged_frames) = match decode(request.clip, request.budget, request.seam) {
        Ok(clip) => {
            let measures = measure_loop(&content_frames(&clip, request.spec.size))?;
            (
                RepairOutcome::Measured {
                    measures,
                    seam: clip.seam,
                },
                clip.framed.frames,
            )
        }
        Err(refusal @ BakerError::SeamRepairRefused { .. }) => (
            RepairOutcome::Refused {
                reason: refusal.to_string(),
            },
            raw_clip.framed.frames,
        ),
        Err(other) => return Err(other),
    };
    Ok(LoopCheck {
        raw,
        repaired,
        start_fidelity,
        judged_frames,
    })
}

/// The clip's own pixels of each square frame: the centred `content_size` rectangle.
fn content_frames(clip: &RepairedClip, frame_size: u32) -> Vec<RgbaImage> {
    let (width, height) = clip.content_size;
    let left = frame_size.saturating_sub(width) / 2;
    let top = frame_size.saturating_sub(height) / 2;
    clip.framed
        .frames
        .iter()
        .map(|frame| image::imageops::crop_imm(frame, left, top, width, height).to_image())
        .collect()
}

/// The D-6 measures of one cycle of `frames`, in order; the cycle wraps from the last frame
/// to the first.
///
/// # Errors
/// [`BakerError::ValidationFailed`] for fewer than two frames, or for frames the measures
/// cannot compare (different or too small sizes).
pub fn measure_loop(frames: &[RgbaImage]) -> Result<LoopMeasures, BakerError> {
    let count = frames.len();
    let Some(first) = frames.first().filter(|_| count >= 2) else {
        return Err(BakerError::ValidationFailed {
            name: "loop-check".into(),
            reason: format!("{count} kept frame(s): a loop needs at least two"),
        });
    };
    let mut steps = Vec::with_capacity(count);
    for (k, frame) in frames.iter().enumerate() {
        steps.push(ssim(frame, &frames[(k + 1) % count])?);
    }
    let wrap = steps[count - 1];
    let mut inner = steps[..count - 1].to_vec();
    inner.sort_by(f64::total_cmp);
    let middle = inner.len() / 2;
    let step = if inner.len() % 2 == 0 {
        f64::midpoint(inner[middle - 1], inner[middle])
    } else {
        inner[middle]
    };
    let (worst_at, worst) = steps
        .iter()
        .copied()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((count - 1, wrap));
    let reference = ZoomReference::new(first)?;
    let (mut growth_at, mut growth) = (0, 0.0_f64);
    for (k, frame) in frames.iter().enumerate().skip(1) {
        let measured = reference.estimate(frame)?.growth();
        if measured.abs() > growth.abs() {
            (growth_at, growth) = (k, measured);
        }
    }
    Ok(LoopMeasures {
        frames: count,
        wrap,
        step,
        worst,
        worst_at,
        growth,
        growth_at,
    })
}

/// Writes the sheets of a [`LoopCheck`]'s judged frames: `<stem>_repaired.png`, up to
/// [`LOOP_SHEET_FRAMES`] frames evenly spaced over the cycle, and `<stem>_seam.png`, the last
/// [`SEAM_SHEET_TAIL`] frames followed by frame 0 (what the restart shows); full size in
/// `full_dir` and at most [`EVIDENCE_SHEET_MAX_WIDTH`] pixels wide in `small_dir`. Returns the
/// two full-size paths, repaired sheet first.
///
/// # Errors
/// Directory and image errors.
pub fn loop_sheets(
    check: &LoopCheck,
    full_dir: &Path,
    small_dir: &Path,
    stem: &str,
) -> Result<(PathBuf, PathBuf), BakerError> {
    let frames = &check.judged_frames;
    let spread = evenly_spaced_indices(frames.len(), LOOP_SHEET_FRAMES);
    let mut seam: Vec<usize> =
        (frames.len().saturating_sub(SEAM_SHEET_TAIL)..frames.len()).collect();
    seam.push(0);
    for dir in [full_dir, small_dir] {
        std::fs::create_dir_all(dir).map_err(|source| BakerError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
    }
    let write = |suffix: &str, picks: &[usize]| -> Result<PathBuf, BakerError> {
        let name = format!("{stem}_{suffix}.png");
        let sheet = tile_row(frames, picks);
        let full = full_dir.join(&name);
        sheet
            .save(&full)
            .map_err(|source| BakerError::ImageProcessing {
                path: full.clone(),
                source,
            })?;
        save_evidence_copy(
            image::DynamicImage::ImageRgba8(sheet),
            &small_dir.join(&name),
        )?;
        Ok(full)
    };
    Ok((write("repaired", &spread)?, write("seam", &seam)?))
}

/// `frames[picks]` side by side in one row; every frame has the first one's size.
fn tile_row(frames: &[RgbaImage], picks: &[usize]) -> RgbaImage {
    let (width, height) = frames.first().map_or((1, 1), RgbaImage::dimensions);
    let tiles = u32::try_from(picks.len()).unwrap_or(u32::MAX).max(1);
    let mut sheet = RgbaImage::new(width.saturating_mul(tiles), height);
    for (column, index) in (0_u32..).zip(picks) {
        if let Some(frame) = frames.get(*index) {
            let x = i64::from(column.saturating_mul(width));
            image::imageops::replace(&mut sheet, frame, x, 0);
        }
    }
    sheet
}
