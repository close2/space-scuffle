//! Keyframes, contact sheets, review verdicts, installation and import of Veo clips (plan
//! `2026-11-04-phase-8-5-veo-media.md`, decisions D-5 and D-7).
//!
//! # Responsibility
//! - [`extract_keyframe`]: one frame of a clip at a given time, as a PNG. The run uses the
//!   Idle clip's frame at [`CHAIN_KEYFRAME_TIME`] as the image input of the identity's other
//!   states, so that they show the same design.
//! - [`contact_sheet`]: eight frames of a clip side by side, full size for the reviewer and a
//!   copy at most [`EVIDENCE_SHEET_MAX_WIDTH`] pixels wide for the committed evidence.
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
use crate::ffmpeg_importer::{probe_video, VideoProbe};
use crate::ledger::{sha256_file, Ledger, LedgerEvent, LedgerHistory};
use crate::veo_plan::{state_slug, VeoDirs};
use sim_types::enums::AnimState;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// The FFmpeg executable the pipeline uses.
pub const DEFAULT_FFMPEG_PATH: &str = "/usr/bin/ffmpeg";

/// Time of the Idle frame that seeds the other states' keyframes.
pub const CHAIN_KEYFRAME_TIME: Duration = Duration::from_secs(1);

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
        })?;
    Ok(probe)
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
