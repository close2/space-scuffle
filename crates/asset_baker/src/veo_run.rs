//! Executes a planned Veo run under the spending rules (plan
//! `2026-11-04-phase-8-5-veo-media.md`, decisions D-3, D-4, D-5 and D-7, amended by
//! `2026-11-04-veo-seamless-loops.md`, D-2 to D-4).
//!
//! # Responsibility
//! Walks a [`RunPlan`] in order: validates reused clips, resumes recorded operations, and starts
//! new generations, writing every event to the [`Ledger`]. Each finished operation is recorded
//! (`<clips>/<slug>/<state>/<operation>.json`) and, if it carries a video, downloaded beside it
//! and validated. The first frame of an Idle clip is the start picture of the identity's other
//! states when the identity has no Idle pin. A pinned clip sends its start picture also as the
//! end frame (`lastFrame`), and the ledger records the end frame's hash. Each request sends its
//! clip's own parameters ([`RunPlan::parameters_for`]: the run's, including a `--seed`, with
//! the clip's negative prompt), and the ledger records them exactly as sent (plan
//! `2026-11-04-veo-structure-action-text.md`, D-2 and D-3). The service is a
//! [`VideoService`]: Veo, or Gemini Omni for the trial of the Veo plan's Amendment 2, whose
//! requests carry the text and parameters [`VideoService::request_as_sent`] gives, which are
//! also what the ledger records.
//!
//! # How it fits
//! `generate-veo` calls [`execute_plan`] after the plan, prompt validation and budget checks
//! passed and the credentials were resolved. Downloads go to the clips directory only; a clip
//! reaches `assets/videos` through `install-clip` after review (`clip_review`).
//!
//! # Invariants
//! - `request_started` is written before a start request is sent, so a crash cannot hide a
//!   spend, and the run never starts more than `max_generations` requests.
//! - A run that would send an end frame with a Gemini API key is refused before its first
//!   request ([`BakerError::LastFrameNeedsVertex`]), and so is a run with a seed
//!   ([`BakerError::SeedNeedsVertex`]); a chained picture that would pin a clip is checked
//!   against the pin rules before its request is recorded.
//! - A quota refusal ends the run at once ([`BakerError::VeoQuotaExhausted`], exit code 3).
//! - The first Cloud Storage result, the first filtered result, the first failure of a kind
//!   the ledger has not seen, and a start request refused as invalid (HTTP 400) end the run
//!   after recording it ([`BakerError::VeoRunStopped`], exit code 4), so a systematic problem
//!   costs at most one generation, not a batch.
//! - A failed or filtered clip is never regenerated automatically.

use crate::clip_review::{extract_keyframe, CHAIN_KEYFRAME_TIME, DEFAULT_FFMPEG_PATH};
use crate::error::BakerError;
use crate::ffmpeg_importer::probe_video;
use crate::ledger::{
    sha256_file, Ledger, LedgerError, LedgerEvent, LedgerHistory, OperationOutcome,
};
use crate::prompts::{check_pin, KeyframeSource, PromptProblem};
use crate::veo_client::{GenerationImages, VeoOperation, VeoParameters};
use crate::veo_plan::{state_slug, ClipAction, PlannedClip, RunPlan, VeoDirs};
use crate::video_service::VideoService;
use sim_types::enums::AnimState;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// File name, inside `<clips>/<slug>/`, of the Idle clip's first frame extracted as the start
/// picture of the identity's other states.
pub const CHAINED_FRAME_FILE: &str = "idle-first-frame.png";

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
    pub client: &'a dyn VideoService,
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
/// [`BakerError::LastFrameNeedsVertex`] before any request when the client selects the Gemini
/// API and a paid clip would send an end frame; [`BakerError::SeedNeedsVertex`] before any
/// request when the client selects the Gemini API and the run has a seed and a paid clip;
/// [`BakerError::VeoQuotaExhausted`] and
/// [`BakerError::VeoRunStopped`] as described above; [`BakerError::PromptsInvalid`] when a
/// chained picture breaks the pin rules; [`BakerError::VeoRunIncomplete`] if any clip failed or
/// was skipped; and any transport, ledger or validation error, which also ends the run (a
/// recorded operation is then resumed by the next run).
pub fn execute_plan(plan: &RunPlan, context: RunContext<'_>) -> Result<RunSummary, BakerError> {
    if !context.client.accepts_end_frame_and_seed() {
        if plan.parameters.seed.is_some() && plan.generations() > 0 {
            return Err(BakerError::SeedNeedsVertex);
        }
        let clips = plan.pinned_clips();
        if !clips.is_empty() {
            return Err(BakerError::LastFrameNeedsVertex { clips });
        }
    }
    let mut run = Run {
        context,
        idle_clips: plan.existing_idle.clone(),
        summary: RunSummary::default(),
    };
    for clip in &plan.clips {
        run.clip(clip, &plan.parameters_for(clip))?;
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

    fn clip(&mut self, clip: &PlannedClip, parameters: &VeoParameters) -> Result<(), BakerError> {
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
                let completed = self.context.client.resume(model, operation)?;
                self.finish(clip, operation, &completed)
            }
            ClipAction::Generate { .. } => self.generate(clip, parameters),
        }
    }

    /// The start picture to send with `clip`. A chained picture is the Idle clip's first frame
    /// (`CHAIN_KEYFRAME_TIME`), extracted now and, for a pinned clip, checked against the pin
    /// rules before anything is recorded or sent.
    fn keyframe(&mut self, clip: &PlannedClip) -> Result<Keyframe, BakerError> {
        match &clip.keyframe {
            KeyframeSource::None => Ok(Keyframe::Send(None)),
            KeyframeSource::File(path) | KeyframeSource::IdlePin(path) => {
                Ok(Keyframe::Send(Some(path.clone())))
            }
            KeyframeSource::ChainedFromIdle => {
                let Some(idle) = self.idle_clips.get(&clip.slug).cloned() else {
                    return Ok(Keyframe::NoIdleClip);
                };
                let frame = self
                    .context
                    .dirs
                    .clips
                    .join(&clip.slug)
                    .join(CHAINED_FRAME_FILE);
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
                if clip.pin_end {
                    if let Some(issue) = check_pin(&frame) {
                        return Err(BakerError::PromptsInvalid {
                            problems: vec![PromptProblem {
                                slug: clip.slug.clone(),
                                state: clip.state,
                                issue,
                            }],
                        });
                    }
                }
                Ok(Keyframe::Send(Some(frame)))
            }
        }
    }

    /// Starts one generation of `clip` with `parameters`, the clip's own request parameters.
    fn generate(
        &mut self,
        clip: &PlannedClip,
        parameters: &VeoParameters,
    ) -> Result<(), BakerError> {
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
        // A pinned clip ends on the picture it starts from (loops plan D-2).
        let end = keyframe.as_deref().filter(|_| clip.pin_end);
        let keyframe_sha256 = keyframe.as_deref().map(sha256_file).transpose()?;
        let last_frame_sha256 = end.and(keyframe_sha256.clone());
        let client = self.context.client;
        let images = GenerationImages {
            start: keyframe.as_deref(),
            end,
        };
        // The text and parameters both sent and recorded (Omni folds the negative prompt into
        // the text, Veo sends both unchanged).
        let (text, sent) = client.request_as_sent(&clip.prompt.text, images, parameters)?;
        let request_id = self.context.ledger.next_request_id();
        self.context.ledger.append(LedgerEvent::RequestStarted {
            request_id: request_id.clone(),
            slug: clip.slug.clone(),
            state: clip.state,
            model: client.model().to_string(),
            mode: client.mode(),
            prompt_sha256: crate::ledger::sha256_hex(text.as_bytes()),
            prompt: text.clone(),
            keyframe_sha256,
            last_frame_sha256,
            parameters: sent.clone(),
        })?;
        self.summary.started += 1;
        self.say(&format!(
            "==> {label}: starting a generation ({request_id})"
        ))?;
        let operation = self.start(clip, &request_id, images, (&text, &sent))?;
        let completed = if operation.done {
            operation.clone()
        } else {
            self.say(&format!("    polling {}", operation.name))?;
            client.poll(&operation.name)?
        };
        self.finish(clip, &operation.name, &completed)
    }

    /// Sends the start request and records its outcome.
    ///
    /// A request the service refuses as invalid (HTTP 400, such as `INVALID_ARGUMENT` for an
    /// end frame it does not accept) started no operation and is not billed. It is recorded as
    /// `start_failed`, as every start failure is, and ends the run as a result that needs a
    /// person ([`BakerError::VeoRunStopped`]); it is never retried, because the same request
    /// would be refused again.
    fn start(
        &mut self,
        clip: &PlannedClip,
        request_id: &str,
        images: GenerationImages<'_>,
        (text, parameters): (&str, &VeoParameters),
    ) -> Result<VeoOperation, BakerError> {
        let client = self.context.client;
        let operation = match client.start(text, images, parameters) {
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
                let message = client.redact(&error.to_string());
                self.context.ledger.append(LedgerEvent::StartFailed {
                    request_id: request_id.to_string(),
                    slug: clip.slug.clone(),
                    state: clip.state,
                    error: message.clone(),
                })?;
                if matches!(error, BakerError::VeoHttpStatus { status: 400, .. }) {
                    let end = if images.end.is_some() {
                        "with an end frame (lastFrame)"
                    } else {
                        "without an end frame"
                    };
                    return Err(BakerError::VeoRunStopped {
                        reason: format!(
                            "{} {}: the service refused the start request {end} as invalid; it started no operation and is not billed. It is recorded as start_failed and not retried: {message}",
                            clip.slug,
                            state_slug(clip.state)
                        ),
                    });
                }
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
        let outcome = client.classify(completed);
        let dir = self.context.dirs.clip_dir(&clip.slug, clip.state);
        let stem = operation_stem(operation_name);
        let record = dir.join(format!("{stem}.json"));
        write_record(&record, &client.record(completed))?;
        self.context.ledger.append(LedgerEvent::OperationDone {
            slug: clip.slug.clone(),
            state: clip.state,
            operation: operation_name.to_string(),
            outcome: outcome.clone(),
        })?;
        match outcome {
            OperationOutcome::VideoInline | OperationOutcome::VideoUri { .. } => {
                let path = dir.join(format!("{stem}.mp4"));
                client.save_video(completed, &path)?;
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
