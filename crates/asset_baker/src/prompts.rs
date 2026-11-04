//! Veo prompts: composed from data and validated before any spend (plan
//! `2026-11-04-phase-8-5-veo-media.md`, decision D-2).
//!
//! # Responsibility
//! Loads the house style (`assets/catalog/media_style.ron`), including its action policy
//! ([`ActionPolicy`], plan `2026-11-04-veo-structure-action-text.md`, D-1), its framing policy
//! ([`crate::framing::FramingPolicy`]) and its loop policy ([`LoopPolicy`], plan
//! `2026-11-04-veo-seamless-loops.md`, D-1), composes the prompt of one clip
//! from the identity's authored `veo_prompts` entry, the action text of its drawing kind (or
//! its override) and the style sentence, composes its negative prompt
//! ([`compose_negative_prompt`]), and checks a composed prompt, its negative prompt and its
//! keyframe against the rules in that file.
//! A picture that also pins a clip's last frame (`lastFrame`) must pass the pin rules
//! ([`check_pin`]: PNG or JPEG, 16:9 within 1 %, at least 1280×720, at most 20 MB).
//!
//! # How it fits
//! Charter §3.3 puts media generation prompts in data: no prompt text lives in Rust. The plan
//! (`veo_plan`) composes every clip's prompt; `validate-prompts`, and `generate-veo` before it
//! reads a credential, refuse a run while any clip it would pay for has a problem.
//!
//! # Invariants
//! - A clip without an authored prompt still gets a composed prompt (from the display name and
//!   lore) so that a dry run can show it, but it is marked unauthored and refused for a paid run.
//! - Every problem of every clip is reported at once, so one edit pass can fix them all.
//! - The action and loop policies name no identity in Rust (charter §3.3): the wording per
//!   drawing kind, which states pin their last frame and which get a seam repair are data,
//!   checked against the catalog at load, and every state an identity publishes has a text.
//! - A keyframe's format is read from its first bytes, never from its file name.

use crate::error::BakerError;
use crate::framing::{FramingPolicy, DEFAULT_UNIT_FRAME_SIZE};
use serde::{Deserialize, Serialize};
use sim_types::enums::{AnimState, ShapeKind};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// Path of the media style file inside an asset bundle.
pub const MEDIA_STYLE_FILE: &str = "catalog/media_style.ron";

/// Most frames a seam cross-fade may blend (D-1): more would read as a long dissolve.
pub const MAX_CROSSFADE_FRAMES: u8 = 4;

/// Tallest atlas strip the bake writes, in pixels: the WebGPU 2D texture limit. It sets the
/// per-state frame budget, `(MAX_ATLAS_STRIP_HEIGHT / frame size) / published states`, that
/// the loop policy's cross-fade length is checked against. The bake (`ffmpeg_importer`) and
/// the provenance record use the same limit.
pub const MAX_ATLAS_STRIP_HEIGHT: u32 = 16_384;

/// Largest image the service accepts as a start or end frame, in bytes (Veo 3.1 model page:
/// an image-to-video input image is at most 20 MB).
pub const MAX_PIN_BYTES: u64 = 20 * 1000 * 1000;

/// Smallest pin, in pixels (width, height): the service's 720p input resolution.
pub const MIN_PIN_SIZE: (u32, u32) = (1280, 720);

/// Largest accepted deviation of a pin's aspect ratio from 16:9, in per mille (1 %).
pub const PIN_ASPECT_TOLERANCE_PERMILLE: u64 = 10;

/// Whether the atlas plays `state` as a loop. Idle, Walk and Cast loop; Attack plays once;
/// Constructing and Die hold their last frame. The bake (`ffmpeg_importer`) calls this to write
/// each manifest's `ActionSlice::looping` and to refuse a seam repair on a state that does not
/// loop, so it is the one source of that classification; an integration test bakes every state
/// and compares the two.
#[must_use]
pub const fn atlas_loops(state: AnimState) -> bool {
    match state {
        AnimState::Idle | AnimState::Walk | AnimState::Cast => true,
        AnimState::Constructing | AnimState::Attack | AnimState::Die => false,
    }
}

/// Repair of a loop seam done by the baker on a state's kept frames (D-5 of the loops plan): a
/// scale lock, then a cross-fade of the last frames back into frame 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeamRepair {
    /// Undo a steady camera zoom by enlarging every kept frame to the framing of the last one.
    pub scale_lock: bool,
    /// Number of last kept frames blended towards frame 0: at most [`MAX_CROSSFADE_FRAMES`]
    /// and at most a quarter of the state's frame budget. 0 blends nothing.
    pub crossfade_frames: u8,
}

/// How one state's clip is made seamless (D-1 of the loops plan).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopSpec {
    /// The start request sends the state's start picture also as `lastFrame`, so the clip
    /// must end on the picture it starts from.
    pub pin_end: bool,
    /// The seam repair done in the bake; only a state the atlas loops may have one.
    #[serde(default)]
    pub seam: Option<SeamRepair>,
}

/// The `loops` section of `media_style.ron`: the treatment of each animation state, and
/// per-identity overrides.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopPolicy {
    /// Treatment per state. Every state the atlas loops ([`atlas_loops`]) needs an entry; a
    /// state without one is neither pinned nor repaired.
    pub states: BTreeMap<AnimState, LoopSpec>,
    /// Exact catalog slug to state to treatment, for one identity whose clip needs another
    /// one. A state the override does not name keeps the treatment of `states`.
    #[serde(default)]
    pub overrides: BTreeMap<String, BTreeMap<AnimState, LoopSpec>>,
}

impl LoopPolicy {
    /// The treatment of `slug`'s `state`: its override if the policy names one, else the
    /// state's entry, else `None` (neither pinned nor repaired).
    #[must_use]
    pub fn spec_for(&self, slug: &str, state: AnimState) -> Option<LoopSpec> {
        self.overrides
            .get(slug)
            .and_then(|states| states.get(&state))
            .or_else(|| self.states.get(&state))
            .copied()
    }

    /// Whether the start request of `slug`'s `state` pins its last frame to its start picture.
    #[must_use]
    pub fn pins_end(&self, slug: &str, state: AnimState) -> bool {
        self.spec_for(slug, state).is_some_and(|spec| spec.pin_end)
    }

    /// Checks the policy loaded from `path` against `catalog`, with the frame budget of an
    /// atlas of [`DEFAULT_UNIT_FRAME_SIZE`]-pixel frames.
    ///
    /// # Errors
    /// [`BakerError::UnknownLoopOverride`] for an override whose slug the catalog does not hold
    /// exactly, or [`BakerError::DataFile`] when a looping state has no entry, a state the atlas
    /// does not loop has a `seam`, or a cross-fade is longer than [`MAX_CROSSFADE_FRAMES`] or
    /// than a quarter of the frame budget of an identity that publishes the state.
    pub fn check(&self, catalog: &sim_core::UnitCatalog, path: &Path) -> Result<(), BakerError> {
        let invalid = |reason: String| BakerError::DataFile {
            path: path.to_path_buf(),
            reason,
        };
        for slug in self.overrides.keys() {
            if catalog.get_by_slug(slug).is_none_or(|a| a.slug != *slug) {
                return Err(BakerError::UnknownLoopOverride {
                    path: path.to_path_buf(),
                    slug: slug.clone(),
                });
            }
        }
        if let Some(state) = AnimState::ALL
            .into_iter()
            .find(|state| atlas_loops(*state) && !self.states.contains_key(state))
        {
            return Err(invalid(format!(
                "loops.states has no entry for {state:?}, which the atlas loops"
            )));
        }
        let defaults = self
            .states
            .iter()
            .map(|(state, spec)| (None, *state, *spec));
        let overrides = self.overrides.iter().flat_map(|(slug, states)| {
            states
                .iter()
                .map(move |(state, spec)| (Some(slug.as_str()), *state, *spec))
        });
        for (slug, state, spec) in defaults.chain(overrides) {
            let Some(seam) = spec.seam else { continue };
            let place = slug.map_or_else(
                || format!("loops.states {state:?}"),
                |slug| format!("loops.overrides {slug} {state:?}"),
            );
            if !atlas_loops(state) {
                return Err(invalid(format!(
                    "{place} has a seam repair, but the atlas does not loop {state:?}"
                )));
            }
            if seam.crossfade_frames > MAX_CROSSFADE_FRAMES {
                return Err(invalid(format!(
                    "{place}: crossfade_frames {} is more than {MAX_CROSSFADE_FRAMES}",
                    seam.crossfade_frames
                )));
            }
            self.check_budget(catalog, slug, state, seam)
                .map_err(|reason| invalid(format!("{place}: {reason}")))?;
        }
        Ok(())
    }

    /// Checks `seam`'s cross-fade against a quarter of the frame budget of every identity that
    /// publishes `state` and takes this treatment: `slug` alone for an override, else every
    /// identity without an override for the state.
    fn check_budget(
        &self,
        catalog: &sim_core::UnitCatalog,
        slug: Option<&str>,
        state: AnimState,
        seam: SeamRepair,
    ) -> Result<(), String> {
        let frames_in_atlas = MAX_ATLAS_STRIP_HEIGHT / DEFAULT_UNIT_FRAME_SIZE;
        for archetype in catalog.archetypes() {
            let applies = match slug {
                Some(slug) => archetype.slug == slug,
                None => self
                    .overrides
                    .get(&archetype.slug)
                    .is_none_or(|states| !states.contains_key(&state)),
            };
            let published = catalog.publishable_actions(archetype);
            if !applies || !published.contains(state) {
                continue;
            }
            let state_count = u32::try_from(published.iter().count())
                .unwrap_or(u32::MAX)
                .max(1);
            let budget = (frames_in_atlas / state_count).max(1);
            if u32::from(seam.crossfade_frames) * 4 > budget {
                return Err(format!(
                    "crossfade_frames {} is more than a quarter of the {budget} frames '{}' keeps per state",
                    seam.crossfade_frames, archetype.slug
                ));
            }
        }
        Ok(())
    }
}

/// How an identity is drawn, which chooses its action texts (plan
/// `2026-11-04-veo-structure-action-text.md`, D-1). It mirrors the two render shapes of
/// [`ShapeKind`] without their footprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawingKind {
    /// A mobile unit, drawn as a `UnitSphere`.
    UnitSphere,
    /// A structure, drawn as a `BuildingDome`.
    BuildingDome,
}

impl DrawingKind {
    /// The kind of an identity rendered as `shape`.
    #[must_use]
    pub fn of(shape: &ShapeKind) -> Self {
        match shape {
            ShapeKind::UnitSphere => Self::UnitSphere,
            ShapeKind::BuildingDome { .. } => Self::BuildingDome,
        }
    }

    /// The field name of this kind in `media_style.ron` (`unit_sphere`, `building_dome`).
    #[must_use]
    pub fn field_name(self) -> &'static str {
        match self {
            Self::UnitSphere => "unit_sphere",
            Self::BuildingDome => "building_dome",
        }
    }
}

impl fmt::Display for DrawingKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.field_name())
    }
}

/// The action texts of one drawing kind: one text per animation state, placed between an
/// identity's prompt and the style, and what every clip of the kind must also avoid.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KindActions {
    /// Text per state. Every state an identity of this kind publishes needs one, unless an
    /// override gives that identity its own.
    pub texts: BTreeMap<AnimState, String>,
    /// Plain nouns appended (after `", "`) to the global negative prompt of
    /// `media/veo_request.ron` for every clip of this kind; `None` adds nothing.
    #[serde(default)]
    pub negative_terms: Option<String>,
}

/// Where an action text came from, which the dry run prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSource {
    /// The text of the identity's drawing kind.
    Kind(DrawingKind),
    /// The identity's own override.
    Override,
}

impl fmt::Display for ActionSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kind(kind) => kind.fmt(f),
            Self::Override => f.write_str("override"),
        }
    }
}

/// The action text of one identity's state, with its source and its kind's negative terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionText<'a> {
    /// The text placed between the identity's prompt and the style.
    pub text: &'a str,
    /// The kind's text or the identity's override.
    pub source: ActionSource,
    /// The negative terms of the identity's drawing kind; an override does not change them.
    pub negative_terms: Option<&'a str>,
}

/// The `actions` section of `media_style.ron`: action text per drawing kind, and per-identity
/// overrides (plan `2026-11-04-veo-structure-action-text.md`, D-1). It mirrors
/// [`FramingPolicy`] and names no identity in Rust (charter §3.3).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionPolicy {
    /// Texts of identities drawn as a `UnitSphere` (mobile units).
    pub unit_sphere: KindActions,
    /// Texts of identities drawn as a `BuildingDome` (structures).
    pub building_dome: KindActions,
    /// Exact catalog slug to state to text, for one identity whose clip needs other wording.
    /// A state the override does not name keeps its kind's text.
    #[serde(default)]
    pub overrides: BTreeMap<String, BTreeMap<AnimState, String>>,
}

impl ActionPolicy {
    /// The texts of `kind`.
    #[must_use]
    pub fn kind(&self, kind: DrawingKind) -> &KindActions {
        match kind {
            DrawingKind::UnitSphere => &self.unit_sphere,
            DrawingKind::BuildingDome => &self.building_dome,
        }
    }

    /// The action text of `slug`'s `state`, where `slug` is drawn as `shape`: its override if
    /// the policy names one, else its kind's text, else `None`.
    #[must_use]
    pub fn text_for(
        &self,
        slug: &str,
        shape: &ShapeKind,
        state: AnimState,
    ) -> Option<ActionText<'_>> {
        let kind = DrawingKind::of(shape);
        let actions = self.kind(kind);
        let negative_terms = actions.negative_terms.as_deref();
        if let Some(text) = self
            .overrides
            .get(slug)
            .and_then(|states| states.get(&state))
        {
            return Some(ActionText {
                text,
                source: ActionSource::Override,
                negative_terms,
            });
        }
        actions.texts.get(&state).map(|text| ActionText {
            text,
            source: ActionSource::Kind(kind),
            negative_terms,
        })
    }

    /// As [`Self::text_for`], for a state that must have a text.
    ///
    /// # Errors
    /// [`BakerError::MissingActionText`] (naming [`MEDIA_STYLE_FILE`]) when neither the kind
    /// nor an override has a text for `state`. A style loaded with
    /// [`MediaStyle::load_for_catalog`] has one for every published state.
    pub fn require(
        &self,
        slug: &str,
        shape: &ShapeKind,
        state: AnimState,
    ) -> Result<ActionText<'_>, BakerError> {
        self.text_for(slug, shape, state)
            .ok_or_else(|| BakerError::MissingActionText {
                path: PathBuf::from(MEDIA_STYLE_FILE),
                kind: DrawingKind::of(shape).to_string(),
                state: format!("{state:?}"),
                slug: slug.to_string(),
            })
    }

    /// Every text of the policy with the place it is written, such as `building_dome Idle` or
    /// `overrides <slug> Idle`, in file order: unit texts, structure texts, then overrides.
    pub fn texts(&self) -> impl Iterator<Item = (String, &str)> {
        let kinds = [DrawingKind::UnitSphere, DrawingKind::BuildingDome]
            .into_iter()
            .flat_map(move |kind| {
                self.kind(kind)
                    .texts
                    .iter()
                    .map(move |(state, text)| (format!("{kind} {state:?}"), text.as_str()))
            });
        let overrides = self.overrides.iter().flat_map(|(slug, states)| {
            states
                .iter()
                .map(move |(state, text)| (format!("overrides {slug} {state:?}"), text.as_str()))
        });
        kinds.chain(overrides)
    }

    /// Checks the policy loaded from `path` against `catalog`.
    ///
    /// # Errors
    /// [`BakerError::UnknownActionOverride`] for an override whose slug the catalog does not
    /// hold exactly; [`BakerError::DataFile`] for an override of a state its identity does not
    /// publish, or for a blank text or blank negative terms; and
    /// [`BakerError::MissingActionText`] for the first identity, in catalog order, that
    /// publishes a state for which neither its kind nor an override has a text.
    pub fn check(&self, catalog: &sim_core::UnitCatalog, path: &Path) -> Result<(), BakerError> {
        let invalid = |reason: String| BakerError::DataFile {
            path: path.to_path_buf(),
            reason,
        };
        for (slug, states) in &self.overrides {
            let Some(archetype) = catalog.get_by_slug(slug).filter(|a| a.slug == *slug) else {
                return Err(BakerError::UnknownActionOverride {
                    path: path.to_path_buf(),
                    slug: slug.clone(),
                });
            };
            let published = catalog.publishable_actions(archetype);
            if let Some(state) = states.keys().find(|state| !published.contains(**state)) {
                return Err(invalid(format!(
                    "actions.overrides {slug} {state:?}: '{slug}' does not publish {state:?}"
                )));
            }
        }
        if let Some((place, _)) = self.texts().find(|(_, text)| text.trim().is_empty()) {
            return Err(invalid(format!("actions {place} is blank")));
        }
        for kind in [DrawingKind::UnitSphere, DrawingKind::BuildingDome] {
            if self
                .kind(kind)
                .negative_terms
                .as_deref()
                .is_some_and(|terms| terms.trim().is_empty())
            {
                return Err(invalid(format!("actions {kind} negative_terms is blank")));
            }
        }
        for archetype in catalog.archetypes() {
            for state in catalog.publishable_actions(archetype).iter() {
                if self
                    .text_for(&archetype.slug, &archetype.shape, state)
                    .is_none()
                {
                    return Err(BakerError::MissingActionText {
                        path: path.to_path_buf(),
                        kind: DrawingKind::of(&archetype.shape).to_string(),
                        state: format!("{state:?}"),
                        slug: archetype.slug.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

/// The negative prompt of one clip: the global text, then `", "`, then its kind's terms
/// (plan `2026-11-04-veo-structure-action-text.md`, D-2). Either part alone is used as it is;
/// with neither, no negative prompt is sent.
#[must_use]
pub fn compose_negative_prompt(global: Option<&str>, kind_terms: Option<&str>) -> Option<String> {
    match (global, kind_terms) {
        (Some(global), Some(terms)) => Some(format!("{global}, {terms}")),
        (Some(text), None) | (None, Some(text)) => Some(text.to_string()),
        (None, None) => None,
    }
}

/// The house style and prompt rules of `media_style.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaStyle {
    /// Sentence that ends every prompt.
    pub style: String,
    /// Action text per drawing kind and per-identity overrides.
    pub actions: ActionPolicy,
    /// Longest composed prompt accepted, in characters (Unicode scalar values).
    pub max_prompt_chars: usize,
    /// Real names a prompt must not contain, matched as whole words ignoring case.
    pub deny_list: Vec<String>,
    /// Text that marks an unfinished prompt (see [`contains_marker`]).
    pub placeholder_markers: Vec<String>,
    /// How clips are fitted into the square game frame, per render shape and per identity.
    pub framing: FramingPolicy,
    /// Which states pin their last frame and which get a seam repair in the bake.
    pub loops: LoopPolicy,
}

impl MediaStyle {
    /// Loads `media_style.ron` from the asset bundle at `assets_root` and checks its framing
    /// and loop policies against `catalog`, the catalog loaded at run time. Every command uses
    /// this.
    ///
    /// # Errors
    /// As [`Self::load`]; [`BakerError::UnknownFramingOverride`],
    /// [`BakerError::UnknownActionOverride`] or [`BakerError::UnknownLoopOverride`] for an
    /// override that names no catalog identity; [`BakerError::MissingActionText`] for a
    /// published state without an action text; or [`BakerError::DataFile`] for an out-of-range
    /// feather, an invalid action override (see [`ActionPolicy::check`]) or an invalid loop
    /// treatment (see [`LoopPolicy::check`]).
    pub fn load_for_catalog(
        assets_root: &Path,
        catalog: &sim_core::UnitCatalog,
    ) -> Result<Self, BakerError> {
        let style = Self::load(assets_root)?;
        let path = assets_root.join(MEDIA_STYLE_FILE);
        style.framing.check(catalog, &path)?;
        style.actions.check(catalog, &path)?;
        style.loops.check(catalog, &path)?;
        Ok(style)
    }

    /// Loads `media_style.ron` from the asset bundle at `assets_root`, without checking the
    /// framing overrides against a catalog (see [`Self::load_for_catalog`]).
    ///
    /// # Errors
    /// [`BakerError::DataFile`] if it cannot be read or parsed.
    pub fn load(assets_root: &Path) -> Result<Self, BakerError> {
        let path = assets_root.join(MEDIA_STYLE_FILE);
        let text = std::fs::read_to_string(&path).map_err(|source| BakerError::Io {
            path: path.clone(),
            source,
        })?;
        ron::from_str(&text).map_err(|error| BakerError::DataFile {
            path,
            reason: error.to_string(),
        })
    }
}

/// A composed prompt and whether its identity part was authored for this state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedPrompt {
    /// The full prompt text sent to the model.
    pub text: String,
    /// False when the identity had no `veo_prompts` entry for the state and the display name
    /// and lore were used instead.
    pub authored: bool,
}

/// Composes the prompt of `slug`'s `state` clip.
///
/// `slug` must be an exact slug of `catalog`, the catalog loaded at run time: no generic prompt
/// exists for an unknown identity, because money must not be spent on a clip nothing can use.
///
/// The action text is chosen by the identity's drawing kind, or its override
/// ([`ActionPolicy::text_for`]); composition order and spacing are the same for every kind.
///
/// # Errors
/// [`BakerError::UnknownArchetype`] if `slug` is not an exact catalog slug, or
/// [`BakerError::MissingActionText`] if the style has no text for the state (a style loaded
/// with [`MediaStyle::load_for_catalog`] has one for every published state).
pub fn build_action_prompt(
    catalog: &sim_core::UnitCatalog,
    style: &MediaStyle,
    slug: &str,
    state: AnimState,
) -> Result<ComposedPrompt, BakerError> {
    let archetype = crate::veo_client::exact_catalog_archetype(slug, catalog)?;
    let action = style.actions.require(slug, &archetype.shape, state)?.text;
    Ok(match archetype.veo_prompt(state) {
        Some(prompt) => ComposedPrompt {
            text: format!("{prompt} {action} {}", style.style),
            authored: true,
        },
        None => ComposedPrompt {
            text: format!(
                "{}: {} {action} {}",
                archetype.display_name, archetype.lore_description, style.style
            ),
            authored: false,
        },
    })
}

/// Where a clip's keyframe (its start picture) comes from.
///
/// A state other than Idle without an authored file of its own shares one picture with the
/// identity's other states (D-3 of the loops plan): the authored Idle picture when it passes
/// the pin rules ([`Self::IdlePin`]), else the first frame of the Idle clip
/// ([`Self::ChainedFromIdle`]), which is also the frame the Idle loop restarts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyframeSource {
    /// Text-to-video: no image.
    None,
    /// An authored keyframe file of this state.
    File(PathBuf),
    /// The identity's authored Idle picture, which passes the pin rules.
    IdlePin(PathBuf),
    /// The first frame (0 s) of the identity's Idle clip, extracted when the clip is generated.
    ChainedFromIdle,
}

impl KeyframeSource {
    /// The image file, when the source is a file that exists before the run.
    #[must_use]
    pub fn file(&self) -> Option<&Path> {
        match self {
            Self::File(path) | Self::IdlePin(path) => Some(path),
            Self::None | Self::ChainedFromIdle => None,
        }
    }
}

impl fmt::Display for KeyframeSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str("none (text to video)"),
            Self::File(path) => write!(f, "{}", path.display()),
            Self::IdlePin(path) => write!(f, "{} (the identity's Idle pin)", path.display()),
            Self::ChainedFromIdle => f.write_str("first frame (0 s) of the Idle clip"),
        }
    }
}

/// One thing wrong with one clip's prompt or keyframe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptIssue {
    /// No `veo_prompts` entry for the state; the display-name fallback is refused.
    NotAuthored,
    /// The composed prompt is too long.
    TooLong {
        /// Its length in characters.
        chars: usize,
        /// The limit.
        max: usize,
    },
    /// The prompt contains a placeholder marker.
    Placeholder {
        /// The marker found.
        marker: String,
    },
    /// The prompt names a denied term.
    Denied {
        /// The term found.
        term: String,
    },
    /// The keyframe file is missing or not a readable image.
    KeyframeUnreadable {
        /// The keyframe file.
        path: PathBuf,
        /// Why it cannot be used.
        reason: String,
    },
    /// The keyframe is neither PNG nor JPEG by its first bytes, the only formats the service
    /// accepts.
    KeyframeFormat {
        /// The keyframe file.
        path: PathBuf,
    },
    /// The picture would pin the clip's last frame but breaks a pin rule (see [`check_pin`]).
    PinShape {
        /// The picture.
        path: PathBuf,
        /// Every rule it breaks, separated by semicolons.
        reason: String,
    },
    /// The keyframe is chained from an Idle clip that neither exists nor is in the run.
    NoIdleToChain,
    /// The clip's negative prompt contains a placeholder marker.
    NegativePlaceholder {
        /// The marker found.
        marker: String,
    },
    /// The clip's negative prompt names a denied term.
    NegativeDenied {
        /// The term found.
        term: String,
    },
}

impl fmt::Display for PromptIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAuthored => f.write_str(
                "no authored prompt in veo_prompts (the display-name fallback is refused for a paid run)",
            ),
            Self::TooLong { chars, max } => {
                write!(f, "composed prompt is {chars} characters, longer than {max}")
            }
            Self::Placeholder { marker } => write!(f, "contains placeholder text '{marker}'"),
            Self::Denied { term } => write!(f, "names the denied term '{term}'"),
            Self::KeyframeUnreadable { path, reason } => {
                write!(f, "keyframe '{}' cannot be used: {reason}", path.display())
            }
            Self::KeyframeFormat { path } => write!(
                f,
                "keyframe '{}' is neither PNG nor JPEG by its contents",
                path.display()
            ),
            Self::PinShape { path, reason } => write!(
                f,
                "'{}' cannot pin the clip's last frame: {reason}",
                path.display()
            ),
            Self::NoIdleToChain => f.write_str(
                "its keyframe is chained from the Idle clip, which neither exists nor is in this run",
            ),
            Self::NegativePlaceholder { marker } => {
                write!(f, "negative prompt contains placeholder text '{marker}'")
            }
            Self::NegativeDenied { term } => {
                write!(f, "negative prompt names the denied term '{term}'")
            }
        }
    }
}

/// A [`PromptIssue`] of one clip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptProblem {
    /// Catalog slug.
    pub slug: String,
    /// Animation state.
    pub state: AnimState,
    /// What is wrong.
    pub issue: PromptIssue,
}

impl fmt::Display for PromptProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.slug,
            crate::veo_plan::state_slug(self.state),
            self.issue
        )
    }
}

/// Renders `problems` one per line, for an error message.
#[must_use]
pub fn render_problems(problems: &[PromptProblem]) -> String {
    problems.iter().fold(String::new(), |mut text, problem| {
        use std::fmt::Write as _;
        // Writing to a String cannot fail.
        let _ = write!(text, "\n  {problem}");
        text
    })
}

/// Every issue of a composed prompt under `style`'s rules.
#[must_use]
pub fn check_prompt(style: &MediaStyle, prompt: &ComposedPrompt) -> Vec<PromptIssue> {
    let mut issues = Vec::new();
    if !prompt.authored {
        issues.push(PromptIssue::NotAuthored);
    }
    let chars = prompt.text.chars().count();
    if chars > style.max_prompt_chars {
        issues.push(PromptIssue::TooLong {
            chars,
            max: style.max_prompt_chars,
        });
    }
    for marker in &style.placeholder_markers {
        if contains_marker(&prompt.text, marker) {
            issues.push(PromptIssue::Placeholder {
                marker: marker.clone(),
            });
        }
    }
    for term in &style.deny_list {
        if contains_term(&prompt.text, term) {
            issues.push(PromptIssue::Denied { term: term.clone() });
        }
    }
    issues
}

/// Every issue of a clip's composed negative prompt under `style`'s rules: the same deny list
/// and placeholder markers as a prompt (plan `2026-11-04-veo-structure-action-text.md`, D-2).
#[must_use]
pub fn check_negative_prompt(style: &MediaStyle, negative: &str) -> Vec<PromptIssue> {
    let markers = style
        .placeholder_markers
        .iter()
        .filter(|marker| contains_marker(negative, marker))
        .map(|marker| PromptIssue::NegativePlaceholder {
            marker: marker.clone(),
        });
    let denied = style
        .deny_list
        .iter()
        .filter(|term| contains_term(negative, term))
        .map(|term| PromptIssue::NegativeDenied { term: term.clone() });
    markers.chain(denied).collect()
}

/// The format of an image the service accepts, read from its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// PNG (signature `89 50 4E 47 0D 0A 1A 0A`).
    Png,
    /// JPEG (signature `FF D8 FF`).
    Jpeg,
}

impl ImageFormat {
    /// The format of `bytes` by its signature, or `None` for anything but PNG and JPEG.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        const JPEG: [u8; 3] = [0xFF, 0xD8, 0xFF];
        if bytes.starts_with(&PNG) {
            Some(Self::Png)
        } else if bytes.starts_with(&JPEG) {
            Some(Self::Jpeg)
        } else {
            None
        }
    }

    /// The MIME type sent with the image (`mimeType`).
    #[must_use]
    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    /// The decoder of this format in the `image` crate.
    fn codec(self) -> image::ImageFormat {
        match self {
            Self::Png => image::ImageFormat::Png,
            Self::Jpeg => image::ImageFormat::Jpeg,
        }
    }
}

/// A reader of the image at `path` that decodes it as `format`, the format its bytes show,
/// whatever its file name says.
fn reader_as(
    path: &Path,
    format: ImageFormat,
) -> Result<image::ImageReader<std::io::BufReader<std::fs::File>>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    Ok(image::ImageReader::with_format(
        std::io::BufReader::new(file),
        format.codec(),
    ))
}

/// The format of the image file at `path`, read from its first bytes.
///
/// # Errors
/// The I/O error of opening or reading the file.
pub fn sniff_file(path: &Path) -> std::io::Result<Option<ImageFormat>> {
    use std::io::Read as _;
    let mut head = Vec::with_capacity(8);
    std::fs::File::open(path)?.take(8).read_to_end(&mut head)?;
    Ok(ImageFormat::sniff(&head))
}

/// The issue of a keyframe file, if it is missing, not a decodable image, or neither PNG nor
/// JPEG by its contents.
#[must_use]
pub fn check_keyframe(source: &KeyframeSource) -> Option<PromptIssue> {
    let path = source.file()?;
    let unreadable = |reason: String| PromptIssue::KeyframeUnreadable {
        path: path.to_path_buf(),
        reason,
    };
    let format = match sniff_file(path) {
        Err(error) => return Some(unreadable(error.to_string())),
        Ok(None) => {
            return Some(PromptIssue::KeyframeFormat {
                path: path.to_path_buf(),
            })
        }
        Ok(Some(format)) => format,
    };
    reader_as(path, format)
        .and_then(|reader| reader.decode().map_err(|e| e.to_string()))
        .err()
        .map(unreadable)
}

/// The issue of a picture that would pin a clip's last frame (`lastFrame`), if it breaks a pin
/// rule: it must be PNG or JPEG by its contents, 16:9 within
/// [`PIN_ASPECT_TOLERANCE_PERMILLE`], at least [`MIN_PIN_SIZE`] and at most [`MAX_PIN_BYTES`]
/// (plan `2026-11-04-veo-seamless-loops.md`, D-2).
///
/// The start and the end frame of a pinned request are the same file, so a rule that both
/// have the same size holds by construction. A square legacy keyframe fails here but stays a
/// valid start picture of a state that is not pinned.
#[must_use]
pub fn check_pin(path: &Path) -> Option<PromptIssue> {
    let unreadable = |reason: String| PromptIssue::KeyframeUnreadable {
        path: path.to_path_buf(),
        reason,
    };
    let format = match sniff_file(path) {
        Err(error) => return Some(unreadable(error.to_string())),
        Ok(None) => {
            return Some(PromptIssue::PinShape {
                path: path.to_path_buf(),
                reason: "it is neither PNG nor JPEG by its contents".to_string(),
            })
        }
        Ok(Some(format)) => format,
    };
    let bytes = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) => return Some(unreadable(error.to_string())),
    };
    let (width, height) = match reader_as(path, format)
        .and_then(|reader| reader.into_dimensions().map_err(|e| e.to_string()))
    {
        Ok(size) => size,
        Err(reason) => return Some(unreadable(reason)),
    };
    let mut broken = Vec::new();
    // |w/h - 16/9| <= t * 16/9, with t in per mille, is |9w - 16h| * 1000 <= t * 16h.
    let deviation = (9 * u64::from(width)).abs_diff(16 * u64::from(height)) * 1000;
    if deviation > PIN_ASPECT_TOLERANCE_PERMILLE * 16 * u64::from(height) {
        let square = if width == height {
            " (a square legacy keyframe can start a clip but cannot pin its last frame)"
        } else {
            ""
        };
        broken.push(format!("{width}x{height} is not 16:9 within 1 %{square}"));
    }
    let (min_width, min_height) = MIN_PIN_SIZE;
    if width < min_width || height < min_height {
        broken.push(format!(
            "{width}x{height} is smaller than {min_width}x{min_height}"
        ));
    }
    if bytes > MAX_PIN_BYTES {
        broken.push(format!(
            "{bytes} bytes is more than the service's limit of {MAX_PIN_BYTES}"
        ));
    }
    (!broken.is_empty()).then(|| PromptIssue::PinShape {
        path: path.to_path_buf(),
        reason: broken.join("; "),
    })
}

/// Whether `text` contains `term` as a whole word or phrase, ignoring ASCII case.
///
/// A match must not be preceded or followed by a letter or digit, so `Terran` does not match
/// `Subterranean`.
#[must_use]
pub fn contains_term(text: &str, term: &str) -> bool {
    let text = text.to_ascii_lowercase();
    let term = term.to_ascii_lowercase();
    if term.is_empty() {
        return false;
    }
    let is_word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    text.match_indices(&term).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + term.len()..].chars().next();
        !is_word(before) && !is_word(after)
    })
}

/// Whether `text` contains the placeholder `marker`: as a whole word ignoring case when the
/// marker is alphanumeric, anywhere otherwise (`{`, `}`).
#[must_use]
pub fn contains_marker(text: &str, marker: &str) -> bool {
    if marker.chars().all(char::is_alphanumeric) {
        contains_term(text, marker)
    } else {
        !marker.is_empty() && text.contains(marker)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn style() -> MediaStyle {
        MediaStyle::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
            .expect("shipped media style loads")
    }

    fn authored(text: &str) -> ComposedPrompt {
        ComposedPrompt {
            text: text.to_string(),
            authored: true,
        }
    }

    #[test]
    fn whole_words_only() {
        assert!(contains_term("A Terran outpost", "terran"));
        assert!(!contains_term("Subterranean scorpion", "Terran"));
        assert!(contains_term("like star wars, with", "Star Wars"));
        assert!(!contains_term("anything", ""));
        assert!(contains_marker("value {name}", "{"));
        assert!(contains_marker("tbd later", "TBD"));
        assert!(!contains_marker("tbdx later", "TBD"));
    }

    #[test]
    fn every_issue_of_a_prompt_is_reported() {
        let mut rules = style();
        rules.max_prompt_chars = 10;
        let marker = concat!("TO", "DO");
        let issues = check_prompt(
            &rules,
            &ComposedPrompt {
                text: format!("{marker} a StarCraft unit {{x}}"),
                authored: false,
            },
        );
        assert!(issues.contains(&PromptIssue::NotAuthored));
        assert!(issues
            .iter()
            .any(|i| matches!(i, PromptIssue::TooLong { max: 10, .. })));
        assert!(issues.contains(&PromptIssue::Placeholder {
            marker: marker.to_string()
        }));
        assert!(issues.contains(&PromptIssue::Placeholder {
            marker: "{".to_string()
        }));
        assert!(issues.contains(&PromptIssue::Denied {
            term: "StarCraft".to_string()
        }));
    }

    #[test]
    fn the_shipped_style_passes_its_own_rules_and_names_no_franchise() {
        let rules = style();
        let sentence = authored(&rules.style);
        assert_eq!(check_prompt(&rules, &sentence), Vec::new());
        for (place, text) in rules.actions.texts() {
            assert_eq!(check_prompt(&rules, &authored(text)), Vec::new(), "{place}");
        }
    }

    #[test]
    fn negative_prompts_join_the_global_text_and_the_kind_terms() {
        assert_eq!(
            compose_negative_prompt(Some("text, watermark"), Some("swelling")),
            Some("text, watermark, swelling".to_string())
        );
        assert_eq!(
            compose_negative_prompt(Some("text"), None),
            Some("text".to_string())
        );
        assert_eq!(
            compose_negative_prompt(None, Some("swelling")),
            Some("swelling".to_string())
        );
        assert_eq!(compose_negative_prompt(None, None), None);
        let rules = style();
        assert_eq!(check_negative_prompt(&rules, "text, watermark"), Vec::new());
        assert_eq!(
            check_negative_prompt(&rules, "Zerg look, {x}"),
            vec![
                PromptIssue::NegativePlaceholder {
                    marker: "{".to_string()
                },
                PromptIssue::NegativePlaceholder {
                    marker: "}".to_string()
                },
                PromptIssue::NegativeDenied {
                    term: "Zerg".to_string()
                },
            ]
        );
    }

    #[test]
    fn an_override_wins_over_the_kind_and_keeps_the_kind_terms() {
        let mut policy = style().actions;
        let dome = ShapeKind::BuildingDome {
            footprint_w: 2,
            footprint_h: 2,
        };
        policy.overrides.insert(
            "one".to_string(),
            BTreeMap::from([(AnimState::Idle, "own text".to_string())]),
        );
        let own = policy
            .text_for("one", &dome, AnimState::Idle)
            .expect("override");
        assert_eq!(own.text, "own text");
        assert_eq!(own.source, ActionSource::Override);
        assert_eq!(
            own.negative_terms,
            policy.building_dome.negative_terms.as_deref()
        );
        let other = policy
            .text_for("two", &dome, AnimState::Idle)
            .expect("kind text");
        assert_eq!(other.source, ActionSource::Kind(DrawingKind::BuildingDome));
        assert_eq!(
            policy
                .text_for("one", &ShapeKind::UnitSphere, AnimState::Die)
                .map(|text| text.source),
            Some(ActionSource::Kind(DrawingKind::UnitSphere))
        );
        assert_eq!(
            policy.text_for("two", &dome, AnimState::Attack),
            None,
            "no dome text for a state no structure publishes"
        );
    }

    #[test]
    fn a_missing_or_corrupt_keyframe_is_an_issue() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("missing.png");
        assert!(matches!(
            check_keyframe(&KeyframeSource::File(missing)),
            Some(PromptIssue::KeyframeUnreadable { .. })
        ));
        let corrupt = dir.path().join("corrupt.jpg");
        std::fs::write(&corrupt, b"not an image").expect("write");
        assert!(check_keyframe(&KeyframeSource::File(corrupt)).is_some());
        let good = dir.path().join("good.png");
        image::RgbImage::new(2, 2).save(&good).expect("png");
        assert_eq!(check_keyframe(&KeyframeSource::File(good)), None);
        assert_eq!(check_keyframe(&KeyframeSource::ChainedFromIdle), None);
    }
}
