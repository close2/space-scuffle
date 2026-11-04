//! What a Veo run would do, decided before anything is spent (plan
//! `2026-11-04-phase-8-5-veo-media.md`, decisions D-1, D-2, D-4 and D-7).
//!
//! # Responsibility
//! For each requested identity, lists exactly the animation states the simulation can publish
//! for it ([`sim_core::UnitCatalog::publishable_actions`]), Idle first, and decides per clip
//! whether to reuse an installed clip, resume a recorded operation, leave a download awaiting
//! review, or generate (a first attempt or a regeneration). It composes each clip's prompt,
//! resolves its start picture, decides from the loop policy whether the request also pins the
//! clip's last frame to that picture (`lastFrame`, plan `2026-11-04-veo-seamless-loops.md`,
//! D-2), composes its negative prompt from the request's global text and its drawing kind's
//! terms (plan `2026-11-04-veo-structure-action-text.md`, D-2), and reports every prompt,
//! negative prompt and picture problem of the clips it would pay for.
//!
//! # How it fits
//! `generate-veo` builds a [`RunPlan`] from the catalog, the media style and the ledger's
//! [`LedgerHistory`], prints it (the whole output of `--dry-run`), checks it with
//! [`RunPlan::problems`] and the budget, and only then hands it to `veo_run` to execute.
//! `validate-prompts` and `review` use the same plan.
//!
//! # Invariants
//! - A state the identity cannot publish is never planned; a `--states` filter naming one is an
//!   error rather than a silent skip.
//! - Within an identity, Idle comes first, so that its clip exists before the other states
//!   chain their keyframe from it.
//! - All states of an identity start from one picture (D-3 of the loops plan): a state without
//!   its own authored keyframe uses the identity's Idle pin, or else the Idle clip's first frame.
//! - A pinned state without any picture is planned as text-to-video without `lastFrame`; the
//!   printed plan says that the clip seeds the identity's pin.
//! - Planning reads files and the ledger but writes nothing and makes no request.
//! - The plan holds the parameters every request sends ([`RunPlan::parameters_for`]): the
//!   run-wide ones (including a `--seed`) with the clip's own negative prompt, so the dry run
//!   prints and the ledger records exactly what is sent.

use crate::error::BakerError;
use crate::ledger::LedgerHistory;
use crate::prompts::{
    build_action_prompt, check_keyframe, check_negative_prompt, check_pin, check_prompt,
    compose_negative_prompt, ActionSource, ComposedPrompt, KeyframeSource, MediaStyle, PromptIssue,
    PromptProblem,
};
use crate::veo_client::VeoParameters;
use sim_types::enums::AnimState;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// File-name stem of an animation state (`idle`, `walk`, ...).
#[must_use]
pub fn state_slug(state: AnimState) -> &'static str {
    match state {
        AnimState::Idle => "idle",
        AnimState::Walk => "walk",
        AnimState::Attack => "attack",
        AnimState::Constructing => "constructing",
        AnimState::Cast => "cast",
        AnimState::Die => "die",
    }
}

/// A text that names no animation state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown animation state '{name}'; valid: idle, walk, attack, constructing, cast, die")]
pub struct UnknownState {
    /// The text given.
    pub name: String,
}

/// Parses a state stem as written by [`state_slug`].
///
/// # Errors
/// [`UnknownState`] for any other text.
pub fn parse_state(text: &str) -> Result<AnimState, UnknownState> {
    AnimState::ALL
        .into_iter()
        .find(|state| state_slug(*state) == text)
        .ok_or_else(|| UnknownState {
            name: text.to_string(),
        })
}

/// The directories a Veo run reads and writes.
#[derive(Debug, Clone)]
pub struct VeoDirs {
    /// Authored keyframes, named `<slug>_<state>.jpg|png`, or `<slug>_active.jpg` for Attack.
    pub keyframes: PathBuf,
    /// Installed clips, `<slug>_<state>.mp4`.
    pub videos: PathBuf,
    /// Downloads awaiting review, `<slug>/<state>/<operation>.mp4` with a `.json` record.
    pub clips: PathBuf,
}

impl VeoDirs {
    /// The installed clip path of `slug`'s `state`.
    #[must_use]
    pub fn installed_clip(&self, slug: &str, state: AnimState) -> PathBuf {
        self.videos
            .join(format!("{slug}_{}.mp4", state_slug(state)))
    }

    /// The directory of `slug`'s `state` downloads.
    #[must_use]
    pub fn clip_dir(&self, slug: &str, state: AnimState) -> PathBuf {
        self.clips.join(slug).join(state_slug(state))
    }
}

/// What the run does for one clip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipAction {
    /// An installed clip exists; it is validated and used.
    Reuse(PathBuf),
    /// A download exists and waits for review; nothing is spent.
    AwaitingReview(PathBuf),
    /// A paid operation was named and never finished here; it is polled, not restarted.
    Resume {
        /// Operation resource name.
        operation: String,
        /// Model it runs on.
        model: String,
    },
    /// A new paid generation.
    Generate {
        /// Whether an earlier attempt ended without a usable clip.
        regeneration: bool,
    },
}

/// One clip of a run.
#[derive(Debug, Clone)]
pub struct PlannedClip {
    /// Catalog slug.
    pub slug: String,
    /// Animation state.
    pub state: AnimState,
    /// What the run does for it.
    pub action: ClipAction,
    /// The composed prompt.
    pub prompt: ComposedPrompt,
    /// Where its keyframe (start picture) comes from.
    pub keyframe: KeyframeSource,
    /// Whether the loop policy pins the clip's last frame to its start picture.
    pub pin_end: bool,
    /// Where the clip's action text came from: its drawing kind or its override.
    pub action_source: ActionSource,
    /// The negative prompt the clip sends: the global text of `media/veo_request.ron`, then
    /// `", "`, then its drawing kind's terms; `None` sends no field.
    pub negative_prompt: Option<String>,
}

impl PlannedClip {
    /// Whether the clip costs a generation.
    #[must_use]
    pub fn is_paid(&self) -> bool {
        matches!(self.action, ClipAction::Generate { .. })
    }

    /// Whether the start request sends the start picture also as the end frame (`lastFrame`):
    /// the state is pinned and has a start picture.
    #[must_use]
    pub fn sends_last_frame(&self) -> bool {
        self.pin_end && self.keyframe != KeyframeSource::None
    }
}

/// The clips of a run, in execution order.
#[derive(Debug, Clone, Default)]
pub struct RunPlan {
    /// Every clip, grouped by identity, Idle first within each.
    pub clips: Vec<PlannedClip>,
    /// The Idle clip each identity can chain keyframes from before the run starts.
    pub existing_idle: BTreeMap<String, PathBuf>,
    /// The parameters every request of the run sends (`media/veo_request.ron` and the run's
    /// `--seed`), before each clip's own negative prompt replaces the global one.
    pub parameters: VeoParameters,
}

impl RunPlan {
    /// The parameters `clip`'s start request sends: the run's, with the clip's negative prompt.
    #[must_use]
    pub fn parameters_for(&self, clip: &PlannedClip) -> VeoParameters {
        VeoParameters {
            negative_prompt: clip.negative_prompt.clone(),
            ..self.parameters.clone()
        }
    }

    /// Generations the run would start.
    #[must_use]
    pub fn generations(&self) -> u32 {
        u32::try_from(self.clips.iter().filter(|clip| clip.is_paid()).count()).unwrap_or(u32::MAX)
    }

    /// `<slug> <state>` of every planned regeneration.
    #[must_use]
    pub fn regenerations(&self) -> Vec<String> {
        self.clips
            .iter()
            .filter(|clip| matches!(clip.action, ClipAction::Generate { regeneration: true }))
            .map(|clip| format!("{} {}", clip.slug, state_slug(clip.state)))
            .collect()
    }

    /// `<slug> <state>` of every paid clip whose request sends an end frame (`lastFrame`).
    #[must_use]
    pub fn pinned_clips(&self) -> Vec<String> {
        self.clips
            .iter()
            .filter(|clip| clip.is_paid() && clip.sends_last_frame())
            .map(|clip| format!("{} {}", clip.slug, state_slug(clip.state)))
            .collect()
    }

    /// Every prompt, negative prompt and picture problem of the clips the run would pay for. A
    /// negative prompt follows the deny list and placeholder markers of a prompt
    /// ([`check_negative_prompt`]). A picture that would pin a clip's last frame must also pass
    /// the pin rules ([`check_pin`]); a picture chained from an Idle clip is checked when the
    /// run extracts it.
    #[must_use]
    pub fn problems(&self, style: &MediaStyle) -> Vec<PromptProblem> {
        let mut problems = Vec::new();
        for clip in self.clips.iter().filter(|clip| clip.is_paid()) {
            let mut issues = check_prompt(style, &clip.prompt);
            if let Some(negative) = &clip.negative_prompt {
                issues.extend(check_negative_prompt(style, negative));
            }
            match check_keyframe(&clip.keyframe) {
                Some(issue) => issues.push(issue),
                None => {
                    if let Some(path) = clip.keyframe.file().filter(|_| clip.pin_end) {
                        issues.extend(check_pin(path));
                    }
                }
            }
            if clip.keyframe == KeyframeSource::ChainedFromIdle && !self.idle_will_exist(&clip.slug)
            {
                issues.push(PromptIssue::NoIdleToChain);
            }
            problems.extend(issues.into_iter().map(|issue| PromptProblem {
                slug: clip.slug.clone(),
                state: clip.state,
                issue,
            }));
        }
        problems
    }

    /// Whether `slug` will have an Idle clip when its other states run.
    fn idle_will_exist(&self, slug: &str) -> bool {
        self.existing_idle.contains_key(slug)
            || self
                .clips
                .iter()
                .any(|clip| clip.slug == slug && clip.state == AnimState::Idle)
    }

    /// Writes the plan: one `clip <slug> <state>: <action>` line per clip; for each paid clip
    /// its start picture, its end frame (`same as start` or `none`), a note when a pinned clip
    /// has no picture yet and so seeds one, the source of its action text
    /// (`action: building_dome Idle` or `action: override Idle`), its negative prompt, its seed
    /// (`seed: none (service picks)` without `--seed`) and its prompt; then the total.
    ///
    /// # Errors
    /// Errors of `out`.
    pub fn print(&self, out: &mut dyn Write) -> std::io::Result<()> {
        for clip in &self.clips {
            let state = state_slug(clip.state);
            let action = match &clip.action {
                ClipAction::Reuse(path) => format!("reuse {}", path.display()),
                ClipAction::AwaitingReview(path) => format!("awaiting review {}", path.display()),
                ClipAction::Resume { operation, .. } => format!("resume {operation}"),
                ClipAction::Generate { regeneration: true } => "regenerate".to_string(),
                ClipAction::Generate {
                    regeneration: false,
                } => "generate".to_string(),
            };
            writeln!(out, "clip {} {state}: {action}", clip.slug)?;
            if clip.is_paid() {
                writeln!(out, "    start: {}", clip.keyframe)?;
                let end = if clip.sends_last_frame() {
                    "same as start"
                } else {
                    "none"
                };
                writeln!(out, "    end: {end}")?;
                if clip.pin_end && clip.keyframe == KeyframeSource::None {
                    writeln!(out, "    loop: no start picture yet, this clip seeds one")?;
                }
                writeln!(out, "    action: {} {:?}", clip.action_source, clip.state)?;
                match &clip.negative_prompt {
                    Some(negative) => writeln!(out, "    negative: {negative}")?,
                    None => writeln!(out, "    negative: none")?,
                }
                match self.parameters.seed {
                    Some(seed) => writeln!(out, "    seed: {seed}")?,
                    None => writeln!(out, "    seed: none (service picks)")?,
                }
                writeln!(out, "    prompt: {}", clip.prompt.text)?;
            }
        }
        writeln!(out, "Clips to generate: {}", self.generations())
    }
}

/// The published states of `slug`, Idle first, then in [`AnimState::ALL`] order, restricted to
/// `filter` when one is given.
///
/// # Errors
/// [`BakerError::UnknownArchetype`] for an inexact slug, or [`BakerError::StateNotPublished`]
/// if `filter` names a state the identity cannot publish.
pub fn published_states(
    catalog: &sim_core::UnitCatalog,
    slug: &str,
    filter: Option<&[AnimState]>,
) -> Result<Vec<AnimState>, BakerError> {
    let archetype = crate::veo_client::exact_catalog_archetype(slug, catalog)?;
    let published = catalog.publishable_actions(archetype);
    let names = || {
        published
            .iter()
            .map(|s| state_slug(s).to_string())
            .collect()
    };
    if let Some(filter) = filter {
        if let Some(state) = filter.iter().find(|state| !published.contains(**state)) {
            return Err(BakerError::StateNotPublished {
                slug: slug.to_string(),
                state: state_slug(*state).to_string(),
                published: names(),
            });
        }
    }
    let wanted =
        |state: AnimState| published.contains(state) && filter.is_none_or(|f| f.contains(&state));
    let mut states: Vec<AnimState> = AnimState::ALL
        .into_iter()
        .filter(|state| *state != AnimState::Idle && wanted(*state))
        .collect();
    if wanted(AnimState::Idle) {
        states.insert(0, AnimState::Idle);
    }
    Ok(states)
}

/// The authored keyframe of `slug`'s `state`, or the default for the state.
///
/// A file `<slug>_<state>.jpg` (or `.png`) is used if it exists, and for Attack also
/// `<slug>_active.jpg`. Otherwise Idle is text-to-video, and every other state shares the
/// identity's picture (loops plan D-3): the authored Idle picture if it passes the pin rules
/// ([`KeyframeSource::IdlePin`]), else the Idle clip's first frame
/// ([`KeyframeSource::ChainedFromIdle`]). A square legacy Idle picture is therefore never the
/// start of another state.
#[must_use]
pub fn resolve_keyframe(keyframes_dir: &Path, slug: &str, state: AnimState) -> KeyframeSource {
    if let Some(path) = authored_keyframe(keyframes_dir, slug, state) {
        return KeyframeSource::File(path);
    }
    if state == AnimState::Idle {
        return KeyframeSource::None;
    }
    match authored_keyframe(keyframes_dir, slug, AnimState::Idle) {
        Some(pin) if check_pin(&pin).is_none() => KeyframeSource::IdlePin(pin),
        _ => KeyframeSource::ChainedFromIdle,
    }
}

/// The first existing authored keyframe file of `slug`'s `state`: `<slug>_<state>.jpg`, then
/// `.png`, and for Attack also `<slug>_active.jpg`.
fn authored_keyframe(keyframes_dir: &Path, slug: &str, state: AnimState) -> Option<PathBuf> {
    let stem = state_slug(state);
    let mut candidates = vec![
        keyframes_dir.join(format!("{slug}_{stem}.jpg")),
        keyframes_dir.join(format!("{slug}_{stem}.png")),
    ];
    if state == AnimState::Attack {
        candidates.push(keyframes_dir.join(format!("{slug}_active.jpg")));
    }
    candidates.into_iter().find(|path| path.exists())
}

/// Decides the action of one clip from the installed files and the ledger.
fn clip_action(
    dirs: &VeoDirs,
    history: &LedgerHistory,
    slug: &str,
    state: AnimState,
) -> ClipAction {
    let installed = dirs.installed_clip(slug, state);
    if installed.exists() {
        return ClipAction::Reuse(installed);
    }
    let Some(clip) = history.clip(slug, state) else {
        return ClipAction::Generate {
            regeneration: false,
        };
    };
    if let Some(pending) = &clip.pending {
        return ClipAction::Resume {
            operation: pending.operation.clone(),
            model: pending.model.clone(),
        };
    }
    match &clip.downloaded {
        Some(download) if download.path.exists() => {
            ClipAction::AwaitingReview(download.path.clone())
        }
        // A download whose file is gone (scratch was wiped) cannot be reviewed: it is redone.
        Some(_) => ClipAction::Generate { regeneration: true },
        None => ClipAction::Generate {
            regeneration: clip.unusable_results > 0,
        },
    }
}

/// Plans a run over `slugs`, whose requests send `parameters` (the run-wide parameters, with
/// the global negative prompt) with each clip's own negative prompt. See the module docs.
///
/// # Errors
/// [`BakerError::UnknownArchetype`] or [`BakerError::StateNotPublished`] before anything is
/// read from disk for the offending identity, [`BakerError::ValidationFailed`] for a slug
/// that cannot name a portable file, or [`BakerError::MissingActionText`] for a state the
/// style has no action text for.
pub fn plan_run(
    catalog: &sim_core::UnitCatalog,
    style: &MediaStyle,
    parameters: &VeoParameters,
    slugs: &[String],
    filter: Option<&[AnimState]>,
    dirs: &VeoDirs,
    history: &LedgerHistory,
) -> Result<RunPlan, BakerError> {
    let mut plan = RunPlan {
        parameters: parameters.clone(),
        ..RunPlan::default()
    };
    for slug in slugs {
        crate::veo_client::validate_catalog_slug(catalog, slug)?;
        let states = published_states(catalog, slug, filter)?;
        let shape = crate::veo_client::exact_catalog_archetype(slug, catalog)?.shape;
        if let ClipAction::Reuse(path) | ClipAction::AwaitingReview(path) =
            clip_action(dirs, history, slug, AnimState::Idle)
        {
            plan.existing_idle.insert(slug.clone(), path);
        }
        for state in states {
            let action = style.actions.require(slug, &shape, state)?;
            let prompt = build_action_prompt(catalog, style, slug, state)?;
            plan.clips.push(PlannedClip {
                slug: slug.clone(),
                state,
                action: clip_action(dirs, history, slug, state),
                prompt,
                keyframe: resolve_keyframe(&dirs.keyframes, slug, state),
                pin_end: style.loops.pins_end(slug, state),
                action_source: action.source,
                negative_prompt: compose_negative_prompt(
                    parameters.negative_prompt.as_deref(),
                    action.negative_terms,
                ),
            });
        }
    }
    Ok(plan)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn state_stems_round_trip() {
        for state in AnimState::ALL {
            assert_eq!(parse_state(state_slug(state)), Ok(state));
        }
        assert!(parse_state("Idle").is_err());
    }

    #[test]
    fn other_states_share_a_valid_idle_pin_but_not_a_square_idle_picture() {
        let dir = tempfile::tempdir().expect("tempdir");
        let idle = dir.path().join("subject_idle.png");
        image::RgbImage::new(1024, 1024)
            .save(&idle)
            .expect("square");
        assert_eq!(
            resolve_keyframe(dir.path(), "subject", AnimState::Walk),
            KeyframeSource::ChainedFromIdle
        );
        image::RgbImage::new(1280, 720).save(&idle).expect("pin");
        assert_eq!(
            resolve_keyframe(dir.path(), "subject", AnimState::Walk),
            KeyframeSource::IdlePin(idle.clone())
        );
        assert_eq!(
            resolve_keyframe(dir.path(), "subject", AnimState::Idle),
            KeyframeSource::File(idle)
        );
    }

    #[test]
    fn keyframes_prefer_authored_files_then_chain_from_idle() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            resolve_keyframe(dir.path(), "subject", AnimState::Idle),
            KeyframeSource::None
        );
        assert_eq!(
            resolve_keyframe(dir.path(), "subject", AnimState::Walk),
            KeyframeSource::ChainedFromIdle
        );
        let active = dir.path().join("subject_active.jpg");
        std::fs::write(&active, b"x").expect("write");
        assert_eq!(
            resolve_keyframe(dir.path(), "subject", AnimState::Attack),
            KeyframeSource::File(active)
        );
        let walk = dir.path().join("subject_walk.png");
        std::fs::write(&walk, b"x").expect("write");
        assert_eq!(
            resolve_keyframe(dir.path(), "subject", AnimState::Walk),
            KeyframeSource::File(walk)
        );
    }
}
