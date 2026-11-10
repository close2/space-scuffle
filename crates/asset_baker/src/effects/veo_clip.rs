//! Veo clips of effect elements (`asset_baker effect-veo`, plan
//! `2026-11-09-effect-visualization.md` §6.2 item 2, WP6a).
//!
//! # Responsibility
//! Runs one paid Veo generation of an effect element (a fireball, an acid crown, a spine
//! eruption) on pure black, or resumes the element's outstanding operation, and records every
//! step in the media ledger under the element's set and name (`effect_clip_*` events). The
//! downloaded clip is the source `effect-bake` turns into a flipbook; the bake finds its
//! origin by the clip's bytes ([`crate::provenance::veo_origin_by_clip_sha256`]).
//!
//! # How it fits
//! The request, polling, download and redaction are the Veo client's ([`VeoClient`]), and the
//! rules are those of a unit clip's run (`veo_run`): `effect_clip_request_started` is written
//! before the request is sent, a quota refusal is recorded as free and ends the command with
//! [`BakerError::VeoQuotaExhausted`] (exit status 3), any other start failure is recorded as
//! possibly paid, and a filtered, failed or Cloud Storage result ends the command with
//! [`BakerError::VeoRunStopped`] (exit status 4) after it is recorded. The CLI checks the
//! model's price and the total budget before it reads a credential.
//!
//! # Invariants
//! - An element whose last operation was named but never finished is resumed, never started
//!   again: a crash or a timeout cannot pay twice for one clip.
//! - No end frame is sent: effect clips start and end black, they do not loop on a picture.
//! - The parameters sent are the ones the ledger records.

use super::{check_name, EffectError};
use crate::error::BakerError;
use crate::ffmpeg_importer::probe_video;
use crate::ledger::{
    sha256_file, sha256_hex, Ledger, LedgerError, LedgerEvent, LedgerHistory, OperationOutcome,
};
use crate::prompts::ComposedPrompt;
use crate::veo_client::{GenerationImages, VeoClient, VeoOperation, VeoParameters};
use crate::veo_run::operation_stem;
use std::path::{Path, PathBuf};

/// One effect element's clip request.
#[derive(Debug, Clone, Copy)]
pub struct EffectClipJob<'a> {
    /// Effect texture set.
    pub set: &'a str,
    /// Element of the set.
    pub element: &'a str,
    /// The composed and checked prompt.
    pub prompt: &'a ComposedPrompt,
    /// A start picture (image to video), if any.
    pub keyframe: Option<&'a Path>,
    /// The parameters sent and recorded.
    pub parameters: &'a VeoParameters,
    /// Directory clips are written to; each element's go to `<out>/<set>/<element>/`.
    pub out_dir: &'a Path,
}

/// What [`run_effect_clip`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectClipResult {
    /// Whether a paid start request was sent (`false` when an operation was resumed).
    pub started: bool,
    /// The operation that produced the clip.
    pub operation: String,
    /// The validated clip.
    pub path: PathBuf,
    /// Its SHA-256.
    pub sha256: String,
}

/// Generates (or resumes) one effect element's clip. See the module docs for the rules.
///
/// # Errors
/// [`EffectError::InvalidName`] for a set or element that cannot name a directory;
/// [`BakerError::VeoQuotaExhausted`] and [`BakerError::VeoRunStopped`] (wrapped in
/// [`EffectError::Baker`]) as described in the module docs; and transport, ledger, probe and
/// I/O failures.
pub fn run_effect_clip(
    client: &VeoClient,
    ledger: &mut Ledger,
    history: &LedgerHistory,
    job: &EffectClipJob<'_>,
) -> Result<EffectClipResult, EffectError> {
    check_name("set", job.set)?;
    check_name("element", job.element)?;
    let pending = history
        .effect_clip(job.set, job.element)
        .and_then(|clip| clip.pending.clone());
    if let Some(pending) = pending {
        let completed = client
            .clone()
            .with_model(&pending.model)
            .poll_operation(&pending.operation)?;
        let (path, sha256) = finish(client, ledger, job, &pending.operation, &completed)?;
        return Ok(EffectClipResult {
            started: false,
            operation: pending.operation,
            path,
            sha256,
        });
    }
    let keyframe_sha256 = job.keyframe.map(sha256_file).transpose()?;
    let request_id = ledger.next_request_id();
    ledger.append(LedgerEvent::EffectClipRequestStarted {
        request_id: request_id.clone(),
        set: job.set.to_string(),
        element: job.element.to_string(),
        model: client.model().to_string(),
        mode: client.mode(),
        prompt_sha256: sha256_hex(job.prompt.text.as_bytes()),
        prompt: job.prompt.text.clone(),
        keyframe_sha256,
        parameters: job.parameters.clone(),
    })?;
    let operation = start(client, ledger, job, &request_id)?;
    let completed = if operation.done {
        operation.clone()
    } else {
        client.poll_operation(&operation.name)?
    };
    let (path, sha256) = finish(client, ledger, job, &operation.name, &completed)?;
    Ok(EffectClipResult {
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
    job: &EffectClipJob<'_>,
    request_id: &str,
) -> Result<VeoOperation, EffectError> {
    let images = GenerationImages {
        start: job.keyframe,
        end: None,
    };
    let operation = match client.initiate_generation_with(&job.prompt.text, images, job.parameters)
    {
        Ok(operation) => operation,
        Err(BakerError::VeoQuotaExhausted {
            http_status,
            status,
            reason,
        }) => {
            ledger.append(LedgerEvent::EffectClipQuotaRefused {
                request_id: request_id.to_string(),
                set: job.set.to_string(),
                element: job.element.to_string(),
                http_status,
                status: status.clone(),
                reason: reason.clone(),
            })?;
            return Err(BakerError::VeoQuotaExhausted {
                http_status,
                status,
                reason,
            }
            .into());
        }
        Err(error) => {
            let message = client.redact(&error.to_string());
            ledger.append(LedgerEvent::EffectClipStartFailed {
                request_id: request_id.to_string(),
                set: job.set.to_string(),
                element: job.element.to_string(),
                error: message.clone(),
            })?;
            if matches!(error, BakerError::VeoHttpStatus { status: 400, .. }) {
                return Err(BakerError::VeoRunStopped {
                    reason: format!(
                        "{}/{}: the service refused the start request as invalid; it started no operation and is not billed. It is recorded as effect_clip_start_failed and not retried: {message}",
                        job.set, job.element
                    ),
                }
                .into());
            }
            return Err(error.into());
        }
    };
    ledger
        .append(LedgerEvent::EffectClipOperationNamed {
            request_id: request_id.to_string(),
            set: job.set.to_string(),
            element: job.element.to_string(),
            model: client.model().to_string(),
            operation: operation.name.clone(),
        })
        .map_err(|error| match error {
            LedgerError::Io { path, source } => {
                EffectError::from(BakerError::OperationNotRecorded {
                    operation: operation.name.clone(),
                    path,
                    source,
                })
            }
            other => other.into(),
        })?;
    Ok(operation)
}

/// Records a finished operation and downloads and validates its video, or stops.
fn finish(
    client: &VeoClient,
    ledger: &mut Ledger,
    job: &EffectClipJob<'_>,
    operation_name: &str,
    completed: &VeoOperation,
) -> Result<(PathBuf, String), EffectError> {
    let label = format!("{}/{}", job.set, job.element);
    let outcome = client.classify_operation(completed);
    let dir = job.out_dir.join(job.set).join(job.element);
    let stem = operation_stem(operation_name);
    let record = dir.join(format!("{stem}.json"));
    let text = serde_json::to_string_pretty(&client.operation_record(completed))
        .map_err(BakerError::from)?;
    std::fs::create_dir_all(&dir).map_err(|source| EffectError::Io {
        path: dir.clone(),
        source,
    })?;
    super::write_atomic(&record, text.as_bytes())?;
    ledger.append(LedgerEvent::EffectClipOperationDone {
        set: job.set.to_string(),
        element: job.element.to_string(),
        operation: operation_name.to_string(),
        outcome: outcome.clone(),
    })?;
    let stopped = |reason: String| EffectError::from(BakerError::VeoRunStopped { reason });
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
        .map_err(|source| EffectError::Io {
            path: path.clone(),
            source,
        })?
        .len();
    let sha256 = sha256_file(&path)?;
    ledger.append(LedgerEvent::EffectClipDownloaded {
        set: job.set.to_string(),
        element: job.element.to_string(),
        operation: operation_name.to_string(),
        bytes,
        sha256: sha256.clone(),
        duration_seconds: probe.duration_seconds,
        frames: probe.packets,
        width: probe.width,
        height: probe.height,
        path: path.display().to_string(),
    })?;
    Ok((path, sha256))
}
