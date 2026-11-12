//! One paid Veo clip of a named subject: an effect element (`asset_baker effect-veo`) or a
//! slice of a unit's mode atlas (`asset_baker mode-veo`).
//!
//! # Responsibility
//! Runs one generation, or resumes the subject's outstanding operation, and records every step
//! in the media ledger under the subject's own events: `effect_clip_*` for an effect element
//! (set, element), `mode_clip_*` for a mode clip (unit, catalog mode, slice). It downloads and
//! validates the video and returns where it is and its SHA-256, by which the bake later finds
//! its origin ([`crate::provenance::veo_origin_by_clip_sha256`]).
//!
//! # How it fits
//! The request, polling, download and redaction are the Veo client's ([`VeoClient`]), and the
//! rules are those of a unit clip's run (`veo_run`): the `*_request_started` line is written
//! before the request is sent, a quota refusal is recorded as free and ends the command with
//! [`BakerError::VeoQuotaExhausted`] (exit status 3), any other start failure is recorded as
//! possibly paid, and a filtered, failed or Cloud Storage result ends the command with
//! [`BakerError::VeoRunStopped`] (exit status 4) after it is recorded. The callers check the
//! model's price and the total budget before they read a credential.
//!
//! # Invariants
//! - A subject whose last operation was named but never finished is resumed, never started
//!   again: a crash or a timeout cannot pay twice for one clip.
//! - The parameters and pictures sent are the ones the ledger records.
//! - Every event of one subject carries the same key, so the ledger history folds them into one
//!   clip ([`LedgerHistory::effect_clip`], [`LedgerHistory::mode_clip`]).

use crate::error::BakerError;
use crate::ffmpeg_importer::probe_video;
use crate::ledger::{
    sha256_file, sha256_hex, Ledger, LedgerError, LedgerEvent, LedgerHistory, OperationOutcome,
};
use crate::manifest::SliceKey;
use crate::prompts::ComposedPrompt;
use crate::veo_client::{GenerationImages, VeoClient, VeoOperation, VeoParameters};
use crate::veo_run::operation_stem;
use std::path::{Path, PathBuf};

/// What a paid clip shows, which names its ledger events and its download directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipSubject<'a> {
    /// An element of an effect texture set.
    Effect {
        /// Effect texture set.
        set: &'a str,
        /// Element of the set.
        element: &'a str,
    },
    /// A slice of a unit's mode atlas.
    Mode {
        /// Catalog slug of the unit.
        slug: &'a str,
        /// Catalog mode (`mode_name`).
        stance: &'a str,
        /// The slice: a state in the mode, or a morph.
        slot: SliceKey,
    },
}

impl ClipSubject<'_> {
    /// `<set>/<element>` or `<slug>/<stance>/<slot>`, for messages.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Effect { set, element } => format!("{set}/{element}"),
            Self::Mode { slug, stance, slot } => format!("{slug}/{stance}/{slot}"),
        }
    }

    /// Directory the subject's downloads go to inside `out_dir`: `<set>/<element>` or
    /// `<slug>/<stance>/<slot>`.
    #[must_use]
    pub fn directory(&self, out_dir: &Path) -> PathBuf {
        match self {
            Self::Effect { set, element } => out_dir.join(set).join(element),
            Self::Mode { slug, stance, slot } => out_dir.join(slug).join(stance).join(slot.stem()),
        }
    }

    /// The operation of the subject that was named and never finished, if any.
    #[must_use]
    pub fn pending(&self, history: &LedgerHistory) -> Option<crate::ledger::PendingOperation> {
        let clip = match self {
            Self::Effect { set, element } => history.effect_clip(set, element),
            Self::Mode { slug, stance, slot } => history.mode_clip(slug, stance, *slot),
        };
        clip.and_then(|clip| clip.pending.clone())
    }

    fn started(&self, request: StartRecord) -> LedgerEvent {
        match *self {
            Self::Effect { set, element } => LedgerEvent::EffectClipRequestStarted {
                request_id: request.request_id,
                set: set.to_string(),
                element: element.to_string(),
                model: request.model,
                mode: request.mode,
                prompt_sha256: request.prompt_sha256,
                prompt: request.prompt,
                keyframe_sha256: request.keyframe_sha256,
                parameters: request.parameters,
            },
            Self::Mode { slug, stance, slot } => LedgerEvent::ModeClipRequestStarted {
                request_id: request.request_id,
                slug: slug.to_string(),
                stance: stance.to_string(),
                slot,
                model: request.model,
                mode: request.mode,
                prompt_sha256: request.prompt_sha256,
                prompt: request.prompt,
                keyframe_sha256: request.keyframe_sha256,
                last_frame_sha256: request.last_frame_sha256,
                parameters: request.parameters,
            },
        }
    }

    fn named(&self, request_id: &str, model: &str, operation: &str) -> LedgerEvent {
        let (request_id, model, operation) = (
            request_id.to_string(),
            model.to_string(),
            operation.to_string(),
        );
        match *self {
            Self::Effect { set, element } => LedgerEvent::EffectClipOperationNamed {
                request_id,
                set: set.to_string(),
                element: element.to_string(),
                model,
                operation,
            },
            Self::Mode { slug, stance, slot } => LedgerEvent::ModeClipOperationNamed {
                request_id,
                slug: slug.to_string(),
                stance: stance.to_string(),
                slot,
                model,
                operation,
            },
        }
    }

    fn done(&self, operation: &str, outcome: OperationOutcome) -> LedgerEvent {
        let operation = operation.to_string();
        match *self {
            Self::Effect { set, element } => LedgerEvent::EffectClipOperationDone {
                set: set.to_string(),
                element: element.to_string(),
                operation,
                outcome,
            },
            Self::Mode { slug, stance, slot } => LedgerEvent::ModeClipOperationDone {
                slug: slug.to_string(),
                stance: stance.to_string(),
                slot,
                operation,
                outcome,
            },
        }
    }

    fn downloaded(&self, download: DownloadRecord) -> LedgerEvent {
        match *self {
            Self::Effect { set, element } => LedgerEvent::EffectClipDownloaded {
                set: set.to_string(),
                element: element.to_string(),
                operation: download.operation,
                bytes: download.bytes,
                sha256: download.sha256,
                duration_seconds: download.duration_seconds,
                frames: download.frames,
                width: download.width,
                height: download.height,
                path: download.path,
            },
            Self::Mode { slug, stance, slot } => LedgerEvent::ModeClipDownloaded {
                slug: slug.to_string(),
                stance: stance.to_string(),
                slot,
                operation: download.operation,
                bytes: download.bytes,
                sha256: download.sha256,
                duration_seconds: download.duration_seconds,
                frames: download.frames,
                width: download.width,
                height: download.height,
                path: download.path,
            },
        }
    }

    fn quota_refused(
        &self,
        request_id: &str,
        http_status: u16,
        status: Option<String>,
        reason: Option<String>,
    ) -> LedgerEvent {
        let request_id = request_id.to_string();
        match *self {
            Self::Effect { set, element } => LedgerEvent::EffectClipQuotaRefused {
                request_id,
                set: set.to_string(),
                element: element.to_string(),
                http_status,
                status,
                reason,
            },
            Self::Mode { slug, stance, slot } => LedgerEvent::ModeClipQuotaRefused {
                request_id,
                slug: slug.to_string(),
                stance: stance.to_string(),
                slot,
                http_status,
                status,
                reason,
            },
        }
    }

    fn start_failed(&self, request_id: &str, error: String) -> LedgerEvent {
        let request_id = request_id.to_string();
        match *self {
            Self::Effect { set, element } => LedgerEvent::EffectClipStartFailed {
                request_id,
                set: set.to_string(),
                element: element.to_string(),
                error,
            },
            Self::Mode { slug, stance, slot } => LedgerEvent::ModeClipStartFailed {
                request_id,
                slug: slug.to_string(),
                stance: stance.to_string(),
                slot,
                error,
            },
        }
    }

    /// The ledger name of the subject's start-failure event, for messages.
    fn start_failed_name(&self) -> &'static str {
        match self {
            Self::Effect { .. } => "effect_clip_start_failed",
            Self::Mode { .. } => "mode_clip_start_failed",
        }
    }
}

/// The fields of a `*_request_started` line.
struct StartRecord {
    request_id: String,
    model: String,
    mode: crate::ledger::ApiMode,
    prompt_sha256: String,
    prompt: String,
    keyframe_sha256: Option<String>,
    last_frame_sha256: Option<String>,
    parameters: VeoParameters,
}

/// The fields of a `*_downloaded` line.
struct DownloadRecord {
    operation: String,
    bytes: u64,
    sha256: String,
    duration_seconds: f64,
    frames: u64,
    width: u32,
    height: u32,
    path: String,
}

/// One subject's clip request.
#[derive(Debug, Clone, Copy)]
pub struct PaidClipJob<'a> {
    /// What the clip shows.
    pub subject: ClipSubject<'a>,
    /// The composed and checked prompt.
    pub prompt: &'a ComposedPrompt,
    /// A start picture (image to video), if any.
    pub keyframe: Option<&'a Path>,
    /// An end picture sent as `lastFrame`, if any; refused without a start picture.
    pub end_frame: Option<&'a Path>,
    /// The parameters sent and recorded.
    pub parameters: &'a VeoParameters,
    /// Directory clips are written to ([`ClipSubject::directory`] inside it).
    pub out_dir: &'a Path,
}

/// What [`run_paid_clip`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaidClipResult {
    /// Whether a paid start request was sent (`false` when an operation was resumed).
    pub started: bool,
    /// The operation that produced the clip.
    pub operation: String,
    /// The validated clip.
    pub path: PathBuf,
    /// Its SHA-256.
    pub sha256: String,
}

/// Generates (or resumes) one subject's clip. See the module docs for the rules.
///
/// # Errors
/// [`BakerError::ValidationFailed`] for an end picture without a start picture;
/// [`BakerError::VeoQuotaExhausted`] and [`BakerError::VeoRunStopped`] as described in the
/// module docs; and transport, ledger, probe and I/O failures.
pub fn run_paid_clip(
    client: &VeoClient,
    ledger: &mut Ledger,
    history: &LedgerHistory,
    job: &PaidClipJob<'_>,
) -> Result<PaidClipResult, BakerError> {
    if job.end_frame.is_some() && job.keyframe.is_none() {
        return Err(BakerError::ValidationFailed {
            name: job.subject.label(),
            reason: "an end picture needs a start picture".into(),
        });
    }
    if let Some(pending) = job.subject.pending(history) {
        let completed = client
            .clone()
            .with_model(&pending.model)
            .poll_operation(&pending.operation)?;
        let (path, sha256) = finish(client, ledger, job, &pending.operation, &completed)?;
        return Ok(PaidClipResult {
            started: false,
            operation: pending.operation,
            path,
            sha256,
        });
    }
    let request_id = ledger.next_request_id();
    ledger.append(job.subject.started(StartRecord {
        request_id: request_id.clone(),
        model: client.model().to_string(),
        mode: client.mode(),
        prompt_sha256: sha256_hex(job.prompt.text.as_bytes()),
        prompt: job.prompt.text.clone(),
        keyframe_sha256: job.keyframe.map(sha256_file).transpose()?,
        last_frame_sha256: job.end_frame.map(sha256_file).transpose()?,
        parameters: job.parameters.clone(),
    }))?;
    let operation = start(client, ledger, job, &request_id)?;
    let completed = if operation.done {
        operation.clone()
    } else {
        client.poll_operation(&operation.name)?
    };
    let (path, sha256) = finish(client, ledger, job, &operation.name, &completed)?;
    Ok(PaidClipResult {
        started: true,
        operation: operation.name,
        path,
        sha256,
    })
}

/// Sends the start request and records its outcome.
fn start(
    client: &VeoClient,
    ledger: &mut Ledger,
    job: &PaidClipJob<'_>,
    request_id: &str,
) -> Result<VeoOperation, BakerError> {
    let images = GenerationImages {
        start: job.keyframe,
        end: job.end_frame,
    };
    let operation = match client.initiate_generation_with(&job.prompt.text, images, job.parameters)
    {
        Ok(operation) => operation,
        Err(BakerError::VeoQuotaExhausted {
            http_status,
            status,
            reason,
        }) => {
            ledger.append(job.subject.quota_refused(
                request_id,
                http_status,
                status.clone(),
                reason.clone(),
            ))?;
            return Err(BakerError::VeoQuotaExhausted {
                http_status,
                status,
                reason,
            });
        }
        Err(error) => {
            let message = client.redact(&error.to_string());
            ledger.append(job.subject.start_failed(request_id, message.clone()))?;
            if matches!(error, BakerError::VeoHttpStatus { status: 400, .. }) {
                return Err(BakerError::VeoRunStopped {
                    reason: format!(
                        "{}: the service refused the start request as invalid; it started no operation and is not billed. It is recorded as {} and not retried: {message}",
                        job.subject.label(),
                        job.subject.start_failed_name()
                    ),
                });
            }
            return Err(error);
        }
    };
    ledger
        .append(
            job.subject
                .named(request_id, client.model(), &operation.name),
        )
        .map_err(|error| match error {
            LedgerError::Io { path, source } => BakerError::OperationNotRecorded {
                operation: operation.name.clone(),
                path,
                source,
            },
            other => other.into(),
        })?;
    Ok(operation)
}

/// Records a finished operation and downloads and validates its video, or stops.
fn finish(
    client: &VeoClient,
    ledger: &mut Ledger,
    job: &PaidClipJob<'_>,
    operation_name: &str,
    completed: &VeoOperation,
) -> Result<(PathBuf, String), BakerError> {
    let label = job.subject.label();
    let outcome = client.classify_operation(completed);
    let dir = job.subject.directory(job.out_dir);
    let stem = operation_stem(operation_name);
    let record = dir.join(format!("{stem}.json"));
    let text = serde_json::to_string_pretty(&client.operation_record(completed))?;
    write_atomic(&record, text.as_bytes())?;
    ledger.append(job.subject.done(operation_name, outcome.clone()))?;
    let stopped = |reason: String| BakerError::VeoRunStopped { reason };
    match outcome {
        OperationOutcome::VideoInline | OperationOutcome::VideoUri { .. } => {}
        OperationOutcome::GcsUri { uri } => {
            return Err(stopped(format!(
                "{label}: the result is a Cloud Storage URI ({uri}), which this pipeline cannot download; it is in the ledger and in {}",
                record.display()
            )))
        }
        OperationOutcome::Filtered { reason } => {
            return Err(stopped(format!(
                "{label}: the result was filtered ({reason}); reword the prompt before a regeneration"
            )))
        }
        OperationOutcome::Failed { code, message } => {
            return Err(stopped(format!(
                "{label}: the operation failed (code {code:?}: {message})"
            )))
        }
    }
    let path = dir.join(format!("{stem}.mp4"));
    client.save_operation_video(completed, &path)?;
    let probe = probe_video(client.ffprobe_path(), &path)?;
    let bytes = std::fs::metadata(&path)
        .map_err(|source| BakerError::Io {
            path: path.clone(),
            source,
        })?
        .len();
    let sha256 = sha256_file(&path)?;
    ledger.append(job.subject.downloaded(DownloadRecord {
        operation: operation_name.to_string(),
        bytes,
        sha256: sha256.clone(),
        duration_seconds: probe.duration_seconds,
        frames: probe.packets,
        width: probe.width,
        height: probe.height,
        path: path.display().to_string(),
    }))?;
    Ok((path, sha256))
}

/// Writes `bytes` to `path` through a temporary file in the same directory, creating the
/// directory first, so a crash leaves either the old file or the whole new one.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), BakerError> {
    let io = |source| BakerError::Io {
        path: path.to_path_buf(),
        source,
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(io)?;
    let temp = tempfile::NamedTempFile::new_in(parent).map_err(io)?;
    std::fs::write(temp.path(), bytes).map_err(io)?;
    temp.persist(path).map_err(|error| io(error.error))?;
    Ok(())
}
