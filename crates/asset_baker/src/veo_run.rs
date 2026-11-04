//! Executes a planned Veo run under the spending rules (plan
//! `2026-11-04-phase-8-5-veo-media.md`, decisions D-3, D-4, D-5 and D-7).
//!
//! # Responsibility
//! Walks a [`RunPlan`] in order: validates reused clips, resumes recorded operations, and starts
//! new generations, writing every event to the [`Ledger`]. Each finished operation is recorded
//! (`<clips>/<slug>/<state>/<operation>.json`) and, if it carries a video, downloaded beside it
//! and validated. Idle clips seed the keyframe of the identity's other states.
//!
//! # How it fits
//! `generate-veo` calls [`execute_plan`] after the plan, prompt validation and budget checks
//! passed and the credentials were resolved. Downloads go to the clips directory only; a clip
//! reaches `assets/videos` through `install-clip` after review (`clip_review`).
//!
//! # Invariants
//! - `request_started` is written before a start request is sent, so a crash cannot hide a
//!   spend, and the run never starts more than `max_generations` requests.
//! - A quota refusal ends the run at once ([`BakerError::VeoQuotaExhausted`], exit code 3).
//! - The first Cloud Storage result, the first filtered result and the first failure of a kind
//!   the ledger has not seen end the run after recording it ([`BakerError::VeoRunStopped`],
//!   exit code 4), so a systematic problem costs one generation, not a batch.
//! - A failed or filtered clip is never regenerated automatically.

use crate::clip_review::{extract_keyframe, CHAIN_KEYFRAME_TIME, DEFAULT_FFMPEG_PATH};
use crate::error::BakerError;
use crate::ffmpeg_importer::probe_video;
use crate::ledger::{
    sha256_file, Ledger, LedgerError, LedgerEvent, LedgerHistory, OperationOutcome,
};
use crate::prompts::KeyframeSource;
use crate::veo_client::{VeoClient, VeoOperation};
use crate::veo_plan::{state_slug, ClipAction, PlannedClip, RunPlan, VeoDirs};
use sim_types::enums::AnimState;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// What a run did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunSummary {
    /// Start requests sent.
    pub started: u32,
    /// Clips downloaded and validated.
    pub downloaded: u32,
    /// Clips whose operation failed with a failure kind the ledger already knew.
    pub failed: u32,
    /// Clips skipped because no Idle clip existed to chain their keyframe from.
    pub skipped: u32,
}

/// Everything a run needs besides its plan.
pub struct RunContext<'a> {
    /// The authenticated client.
    pub client: &'a VeoClient,
    /// The ledger, open for appending.
    pub ledger: &'a mut Ledger,
    /// The ledger as it was before the run.
    pub history: &'a LedgerHistory,
    /// Keyframe, video and clip directories.
    pub dirs: &'a VeoDirs,
    /// Most start requests the run may send.
    pub max_generations: u32,
    /// Progress output.
    pub out: &'a mut dyn Write,
}

/// Runs `plan`. See the module docs for the rules.
///
/// # Errors
/// [`BakerError::VeoQuotaExhausted`] and [`BakerError::VeoRunStopped`] as described above,
/// [`BakerError::VeoRunIncomplete`] if any clip failed or was skipped, and any transport,
/// ledger or validation error, which also ends the run (a recorded operation is then resumed
/// by the next run).
pub fn execute_plan(plan: &RunPlan, context: RunContext<'_>) -> Result<RunSummary, BakerError> {
    let mut run = Run {
        context,
        idle_clips: plan.existing_idle.clone(),
        summary: RunSummary::default(),
    };
    for clip in &plan.clips {
        run.clip(clip)?;
    }
    let summary = run.summary;
    if summary.failed > 0 || summary.skipped > 0 {
        return Err(BakerError::VeoRunIncomplete {
            failed: summary.failed,
            skipped: summary.skipped,
        });
    }
    Ok(summary)
}

/// The image input of one start request.
enum Keyframe {
    /// Send this image, or none (text to video).
    Send(Option<PathBuf>),
    /// The keyframe is chained from an Idle clip that does not exist: skip the clip.
    NoIdleClip,
}

/// The state of one run.
struct Run<'a> {
    context: RunContext<'a>,
    idle_clips: BTreeMap<String, PathBuf>,
    summary: RunSummary,
}

impl Run<'_> {
    fn say(&mut self, text: &str) -> Result<(), BakerError> {
        writeln!(self.context.out, "{text}").map_err(|source| BakerError::Io {
            path: PathBuf::from("<standard output>"),
            source,
        })
    }

    fn clip(&mut self, clip: &PlannedClip) -> Result<(), BakerError> {
        let label = format!("{} {}", clip.slug, state_slug(clip.state));
        match &clip.action {
            ClipAction::Reuse(path) => {
                probe_video(self.context.client.ffprobe_path(), path).map_err(
                    |error| match error {
                        BakerError::VideoValidationFailed { reason, .. } => {
                            BakerError::CachedVideoInvalid {
                                path: path.clone(),
                                reason,
                            }
                        }
                        other => other,
                    },
                )?;
                self.say(&format!(
                    "==> {label}: reusing installed {}",
                    path.display()
                ))
            }
            ClipAction::AwaitingReview(path) => self.say(&format!(
                "==> {label}: awaiting review at {}",
                path.display()
            )),
            ClipAction::Resume { operation, model } => {
                self.say(&format!("==> {label}: resuming {operation}"))?;
                let completed = self
                    .context
                    .client
                    .clone()
                    .with_model(model)
                    .poll_operation(operation)?;
                self.finish(clip, operation, &completed)
            }
            ClipAction::Generate { .. } => self.generate(clip),
        }
    }

    /// The keyframe image to send with `clip`.
    fn keyframe(&mut self, clip: &PlannedClip) -> Result<Keyframe, BakerError> {
        match &clip.keyframe {
            KeyframeSource::None => Ok(Keyframe::Send(None)),
            KeyframeSource::File(path) => Ok(Keyframe::Send(Some(path.clone()))),
            KeyframeSource::ChainedFromIdle => {
                let Some(idle) = self.idle_clips.get(&clip.slug).cloned() else {
                    return Ok(Keyframe::NoIdleClip);
                };
                let frame = self
                    .context
                    .dirs
                    .clips
                    .join(&clip.slug)
                    .join("idle-keyframe-1s.png");
                if let Some(parent) = frame.parent() {
                    std::fs::create_dir_all(parent).map_err(|source| BakerError::Io {
                        path: parent.to_path_buf(),
                        source,
                    })?;
                }
                extract_keyframe(
                    Path::new(DEFAULT_FFMPEG_PATH),
                    &idle,
                    CHAIN_KEYFRAME_TIME,
                    &frame,
                )?;
                Ok(Keyframe::Send(Some(frame)))
            }
        }
    }

    fn generate(&mut self, clip: &PlannedClip) -> Result<(), BakerError> {
        let label = format!("{} {}", clip.slug, state_slug(clip.state));
        if self.summary.started >= self.context.max_generations {
            return Err(BakerError::VeoRunStopped {
                reason: format!(
                    "--max-generations {} reached before {label}",
                    self.context.max_generations
                ),
            });
        }
        let Keyframe::Send(keyframe) = self.keyframe(clip)? else {
            self.summary.skipped += 1;
            return self.say(&format!(
                "==> {label}: skipped, no Idle clip to chain its keyframe from"
            ));
        };
        let keyframe_sha256 = keyframe.as_deref().map(sha256_file).transpose()?;
        let client = self.context.client;
        let request_id = self.context.ledger.next_request_id();
        self.context.ledger.append(LedgerEvent::RequestStarted {
            request_id: request_id.clone(),
            slug: clip.slug.clone(),
            state: clip.state,
            model: client.model().to_string(),
            mode: client.mode(),
            prompt_sha256: crate::ledger::sha256_hex(clip.prompt.text.as_bytes()),
            prompt: clip.prompt.text.clone(),
            keyframe_sha256,
            parameters: client.parameters(),
        })?;
        self.summary.started += 1;
        self.say(&format!(
            "==> {label}: starting a generation ({request_id})"
        ))?;
        let operation = self.start(clip, &request_id, keyframe.as_deref())?;
        let completed = if operation.done {
            operation.clone()
        } else {
            self.say(&format!("    polling {}", operation.name))?;
            client.poll_operation(&operation.name)?
        };
        self.finish(clip, &operation.name, &completed)
    }

    /// Sends the start request and records its outcome.
    fn start(
        &mut self,
        clip: &PlannedClip,
        request_id: &str,
        keyframe: Option<&Path>,
    ) -> Result<VeoOperation, BakerError> {
        let client = self.context.client;
        let operation = match client.initiate_generation(&clip.prompt.text, keyframe) {
            Ok(operation) => operation,
            Err(BakerError::VeoQuotaExhausted {
                http_status,
                status,
                reason,
            }) => {
                self.context.ledger.append(LedgerEvent::QuotaRefused {
                    request_id: request_id.to_string(),
                    slug: clip.slug.clone(),
                    state: clip.state,
                    http_status,
                    status: status.clone(),
                    reason: reason.clone(),
                })?;
                return Err(BakerError::VeoQuotaExhausted {
                    http_status,
                    status,
                    reason,
                });
            }
            Err(error) => {
                self.context.ledger.append(LedgerEvent::StartFailed {
                    request_id: request_id.to_string(),
                    slug: clip.slug.clone(),
                    state: clip.state,
                    error: client.redact(&error.to_string()),
                })?;
                return Err(error);
            }
        };
        let event = LedgerEvent::OperationNamed {
            request_id: request_id.to_string(),
            slug: clip.slug.clone(),
            state: clip.state,
            model: client.model().to_string(),
            operation: operation.name.clone(),
        };
        self.context
            .ledger
            .append(event)
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

    /// Records a finished operation and downloads its video, or stops the run.
    fn finish(
        &mut self,
        clip: &PlannedClip,
        operation_name: &str,
        completed: &VeoOperation,
    ) -> Result<(), BakerError> {
        let client = self.context.client;
        let label = format!("{} {}", clip.slug, state_slug(clip.state));
        let outcome = client.classify_operation(completed);
        let dir = self.context.dirs.clip_dir(&clip.slug, clip.state);
        let stem = operation_stem(operation_name);
        let record = dir.join(format!("{stem}.json"));
        write_record(&record, &client.operation_record(completed))?;
        self.context.ledger.append(LedgerEvent::OperationDone {
            slug: clip.slug.clone(),
            state: clip.state,
            operation: operation_name.to_string(),
            outcome: outcome.clone(),
        })?;
        match outcome {
            OperationOutcome::VideoInline | OperationOutcome::VideoUri { .. } => {
                let path = dir.join(format!("{stem}.mp4"));
                client.save_operation_video(completed, &path)?;
                self.downloaded(clip, operation_name, &path)
            }
            OperationOutcome::GcsUri { uri } => Err(BakerError::VeoRunStopped {
                reason: format!(
                    "{label}: the result is a Cloud Storage URI ({uri}), which this pipeline cannot download. It is in the ledger and in {}; fetch it from the console and install it with `generate-veo --import`",
                    record.display()
                ),
            }),
            OperationOutcome::Filtered { reason } => Err(BakerError::VeoRunStopped {
                reason: format!(
                    "{label}: the result was filtered ({reason}); reword the prompt before a regeneration"
                ),
            }),
            OperationOutcome::Failed { code, message } => {
                let kind = format!("failed:{}", code.map_or("none".to_string(), |c| c.to_string()));
                if self.context.history.knows_failure(&kind) {
                    self.summary.failed += 1;
                    self.say(&format!("==> {label}: failed ({kind}: {message}); a known failure kind, moving on"))
                } else {
                    Err(BakerError::VeoRunStopped {
                        reason: format!(
                            "{label}: the operation failed with a kind the ledger has not seen ({kind}: {message})"
                        ),
                    })
                }
            }
        }
    }

    /// Validates and records a downloaded clip.
    fn downloaded(
        &mut self,
        clip: &PlannedClip,
        operation: &str,
        path: &Path,
    ) -> Result<(), BakerError> {
        let probe = probe_video(self.context.client.ffprobe_path(), path)?;
        let bytes = std::fs::metadata(path)
            .map_err(|source| BakerError::Io {
                path: path.to_path_buf(),
                source,
            })?
            .len();
        self.context.ledger.append(LedgerEvent::Downloaded {
            slug: clip.slug.clone(),
            state: clip.state,
            operation: operation.to_string(),
            bytes,
            sha256: sha256_file(path)?,
            duration_seconds: probe.duration_seconds,
            frames: probe.packets,
            width: probe.width,
            height: probe.height,
            path: path.display().to_string(),
        })?;
        self.summary.downloaded += 1;
        if clip.state == AnimState::Idle {
            self.idle_clips
                .insert(clip.slug.clone(), path.to_path_buf());
        }
        self.say(&format!(
            "==> {} {}: downloaded {} ({} frames, {}x{}); review it before installing",
            clip.slug,
            state_slug(clip.state),
            path.display(),
            probe.packets,
            probe.width,
            probe.height
        ))
    }
}

/// A file-name stem for an operation: the last path segment, restricted to `[A-Za-z0-9_-]`.
#[must_use]
pub fn operation_stem(operation: &str) -> String {
    let last = operation.rsplit('/').next().unwrap_or(operation);
    let stem: String = last
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if stem.trim_matches('_').is_empty() {
        "operation".to_string()
    } else {
        stem
    }
}

/// Writes a pretty-printed JSON record, creating its directory.
fn write_record(path: &Path, value: &serde_json::Value) -> Result<(), BakerError> {
    let io = |source| BakerError::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let text = serde_json::to_string_pretty(value)?;
    std::fs::write(path, text).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_stems_are_safe_file_names() {
        assert_eq!(operation_stem("models/m/operations/abc-123"), "abc-123");
        assert_eq!(operation_stem("operations/../../x"), "x");
        assert_eq!(operation_stem("operations/.."), "operation");
        assert_eq!(operation_stem(""), "operation");
        assert_eq!(operation_stem("a b"), "a_b");
    }
}
