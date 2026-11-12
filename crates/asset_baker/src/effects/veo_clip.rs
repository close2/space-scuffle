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
//! The run itself is [`crate::paid_clip::run_paid_clip`], shared with the mode clips of units;
//! its rules are those of a unit clip's run (`veo_run`): `effect_clip_request_started` is written
//! before the request is sent, a quota refusal is recorded as free and ends the command with
//! [`crate::BakerError::VeoQuotaExhausted`] (exit status 3), any other start failure is recorded as
//! possibly paid, and a filtered, failed or Cloud Storage result ends the command with
//! [`crate::BakerError::VeoRunStopped`] (exit status 4) after it is recorded. The CLI checks the
//! model's price and the total budget before it reads a credential.
//!
//! # Invariants
//! - An element whose last operation was named but never finished is resumed, never started
//!   again: a crash or a timeout cannot pay twice for one clip.
//! - No end frame is sent: effect clips start and end black, they do not loop on a picture.
//! - The parameters sent are the ones the ledger records.

use super::{check_name, EffectError};
use crate::ledger::{Ledger, LedgerHistory};
use crate::paid_clip::{run_paid_clip, ClipSubject, PaidClipJob};
use crate::prompts::ComposedPrompt;
use crate::veo_client::{VeoClient, VeoParameters};
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
/// [`crate::BakerError::VeoQuotaExhausted`] and [`crate::BakerError::VeoRunStopped`] (wrapped
/// in [`EffectError::Baker`]) as described in the module docs; and transport, ledger, probe and
/// I/O failures.
pub fn run_effect_clip(
    client: &VeoClient,
    ledger: &mut Ledger,
    history: &LedgerHistory,
    job: &EffectClipJob<'_>,
) -> Result<EffectClipResult, EffectError> {
    check_name("set", job.set)?;
    check_name("element", job.element)?;
    let result = run_paid_clip(
        client,
        ledger,
        history,
        &PaidClipJob {
            subject: ClipSubject::Effect {
                set: job.set,
                element: job.element,
            },
            prompt: job.prompt,
            keyframe: job.keyframe,
            end_frame: None,
            parameters: job.parameters,
            out_dir: job.out_dir,
        },
    )?;
    Ok(EffectClipResult {
        started: result.started,
        operation: result.operation,
        path: result.path,
        sha256: result.sha256,
    })
}
