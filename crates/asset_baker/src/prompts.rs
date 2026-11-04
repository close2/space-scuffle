//! Veo prompts: composed from data and validated before any spend (plan
//! `2026-11-04-phase-8-5-veo-media.md`, decision D-2).
//!
//! # Responsibility
//! Loads the house style (`assets/catalog/media_style.ron`), including its framing policy
//! ([`crate::framing::FramingPolicy`]) and its loop policy ([`LoopPolicy`], plan
//! `2026-11-04-veo-seamless-loops.md`, D-1), composes the prompt of one clip
//! from the identity's authored `veo_prompts` entry, the action description and the style
//! sentence, and checks a composed prompt and its keyframe against the rules in that file.
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
//! - The loop policy names no identity in Rust (charter §3.3): which states pin their last frame
//!   and which get a seam repair is data, checked against the catalog at load.
//! - A keyframe's format is read from its first bytes, never from its file name.

use crate::error::BakerError;
use crate::framing::{FramingPolicy, DEFAULT_UNIT_FRAME_SIZE};
use serde::{Deserialize, Serialize};
use sim_types::enums::AnimState;
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
/// Constructing and Die hold their last frame. This is the classification the bake writes into
/// each manifest's `ActionSlice::looping` (`ffmpeg_importer`); an integration test bakes every
/// state and compares the two, so they cannot drift apart.
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

/// One description per animation state, placed between an identity's prompt and the style.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDescriptions {
    /// `Constructing`: a structure being raised.
    pub constructing: String,
    /// `Idle`: the ambient loop.
    pub idle: String,
    /// `Walk`: locomotion.
    pub walk: String,
    /// `Attack`: a weapon cycle.
    pub attack: String,
    /// `Cast`: harvesting or building in place.
    pub cast: String,
    /// `Die`: destruction.
    pub die: String,
}

impl ActionDescriptions {
    /// The description of `state`.
    #[must_use]
    pub fn get(&self, state: AnimState) -> &str {
        match state {
            AnimState::Constructing => &self.constructing,
            AnimState::Idle => &self.idle,
            AnimState::Walk => &self.walk,
            AnimState::Attack => &self.attack,
            AnimState::Cast => &self.cast,
            AnimState::Die => &self.die,
        }
    }
}

/// The house style and prompt rules of `media_style.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaStyle {
    /// Sentence that ends every prompt.
    pub style: String,
    /// Per-state action descriptions.
    pub actions: ActionDescriptions,
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
    /// As [`Self::load`], [`BakerError::UnknownFramingOverride`] or
    /// [`BakerError::UnknownLoopOverride`] for an override that names no catalog identity, or
    /// [`BakerError::DataFile`] for an out-of-range feather or an invalid loop treatment (see
    /// [`LoopPolicy::check`]).
    pub fn load_for_catalog(
        assets_root: &Path,
        catalog: &sim_core::UnitCatalog,
    ) -> Result<Self, BakerError> {
        let style = Self::load(assets_root)?;
        let path = assets_root.join(MEDIA_STYLE_FILE);
        style.framing.check(catalog, &path)?;
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
/// # Errors
/// [`BakerError::UnknownArchetype`] if `slug` is not an exact catalog slug.
pub fn build_action_prompt(
    catalog: &sim_core::UnitCatalog,
    style: &MediaStyle,
    slug: &str,
    state: AnimState,
) -> Result<ComposedPrompt, BakerError> {
    let archetype = crate::veo_client::exact_catalog_archetype(slug, catalog)?;
    let action = style.actions.get(state);
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
        for state in AnimState::ALL {
            assert_eq!(
                check_prompt(&rules, &authored(rules.actions.get(state))),
                Vec::new(),
                "{state:?}"
            );
        }
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
