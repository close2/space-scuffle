//! Media of a unit's catalog modes: the mode atlas, its clips, prompts and pictures (plan
//! `2026-11-09-effect-visualization.md` WP13, Amendment 10; owner answer Q0075).
//!
//! # Responsibility
//! Owns everything that names a **mode atlas**, the flipbook a unit shows while it is in one of
//! its catalog modes (a sieged tank, a burrowed scorpion) and while it morphs into or out of
//! it:
//! - the file names: the atlas `<slug>-<mode>`, its clips `<slug>-<mode>_<slice>.mp4`, and the
//!   mode's picture `<slug>-<mode>.png` among the keyframes ([`ModeAtlasId`]);
//! - which slices a mode atlas must hold ([`required_mode_slices`]): every state the client can
//!   display in the mode, and both morphs;
//! - the `modes` section of `media_style.ron` ([`ModeStyle`]): the text of each slice's clip and
//!   the loop treatment of its looping states, checked against the catalog;
//! - each clip's prompt ([`build_mode_prompt`]) and pictures ([`plan_mode_pictures`]).
//!
//! # How it fits
//! `asset_baker mode-veo` generates one clip at a time through [`crate::paid_clip`];
//! `install-mode-clip` copies a reviewed download into the bundle's `videos` directory; and
//! `bake-mode-videos` packs a mode's clips into its atlas with a provenance record. The client
//! admits a mode atlas next to the unit's default atlas (`client::media`) and shows it from the
//! published stance (`client::flipbook_clock`).
//!
//! The pictures chain the clips into one continuous look: the enter morph starts on the unit's
//! default Idle picture and its last frame becomes the mode's picture; every state in the mode
//! starts (and, if it is pinned, ends) on the mode's picture; the leave morph starts on the
//! mode's picture and ends on the default Idle picture.
//!
//! A mode whose pose leaves the default frame (raised barrels) has a [`Reframe`]: its morphs
//! start and end on the Idle picture shrunk with a margin of backdrop
//! ([`ModeAtlasId::reframed_idle_picture`], `reframe-mode-picture`), and its bake crops the
//! clips back to the default atlas's scale ([`crate::BakeFraming::ReframedLetterbox`]).
//!
//! # Invariants
//! - Nothing here names a unit or a mode: slugs and mode names come from the catalog and the
//!   style file at run time (charter §3.3).
//! - Catalog slugs and mode names are snake case, so the `-` in an atlas name separates them
//!   unambiguously, and an atlas name is a valid media identity (`media::media_paths`).

use crate::error::BakerError;
use crate::ffmpeg_importer::frames_per_state;
use crate::framing::DEFAULT_UNIT_FRAME_SIZE;
use crate::manifest::{Morph, SliceKey};
use crate::prompts::{
    atlas_loops, check_keyframe, check_pin, check_prompt, ComposedPrompt, KeyframeSource, LoopSpec,
    MediaStyle, PromptIssue, MAX_CROSSFADE_FRAMES,
};
use serde::Deserialize;
use sim_core::{UnitArchetype, UnitCatalog};
use sim_types::commands::UnitModeSpec;
use sim_types::enums::AnimState;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Separator between the slug and the mode name in a mode atlas's name.
pub const MODE_ATLAS_SEPARATOR: char = '-';

/// One mode of one unit, by its catalog names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeAtlasId<'a> {
    /// Exact catalog slug of the unit.
    pub slug: &'a str,
    /// The mode's `mode_name` in the unit's catalog entry.
    pub stance: &'a str,
}

impl ModeAtlasId<'_> {
    /// The atlas name, `<slug>-<mode>`, which names `<name>.png`, `<name>.flipbook.ron` and
    /// `<name>.provenance.ron` in the textures directory.
    #[must_use]
    pub fn atlas_name(&self) -> String {
        format!("{}{MODE_ATLAS_SEPARATOR}{}", self.slug, self.stance)
    }

    /// The installed clip file name of `slot`, `<slug>-<mode>_<slice>.mp4`.
    #[must_use]
    pub fn clip_file(&self, slot: SliceKey) -> String {
        format!("{}_{}.mp4", self.atlas_name(), slot.stem())
    }

    /// The mode's picture in `keyframes_dir`, `<slug>-<mode>.png`: the last frame of the enter
    /// morph, on which every clip in the mode starts.
    #[must_use]
    pub fn picture(&self, keyframes_dir: &Path) -> PathBuf {
        keyframes_dir.join(format!("{}.png", self.atlas_name()))
    }

    /// The unit's Idle picture reframed for the mode's clips in `keyframes_dir`,
    /// `<slug>-<mode>_reframed_idle.png` ([`Reframe`]): where a reframed mode's enter morph
    /// starts and its leave morph ends.
    #[must_use]
    pub fn reframed_idle_picture(&self, keyframes_dir: &Path) -> PathBuf {
        keyframes_dir.join(format!("{}_reframed_idle.png", self.atlas_name()))
    }
}

/// The catalog mode `stance` of `archetype` and its index in the archetype's `modes`.
///
/// # Errors
/// [`BakerError::DataFile`] naming `path` when the archetype has no such mode.
pub fn catalog_mode<'a>(
    archetype: &'a UnitArchetype,
    stance: &str,
    path: &Path,
) -> Result<(usize, &'a UnitModeSpec), BakerError> {
    archetype
        .modes
        .iter()
        .enumerate()
        .find(|(_, mode)| mode.mode_name == stance)
        .ok_or_else(|| BakerError::DataFile {
            path: path.to_path_buf(),
            reason: format!(
                "'{}' has no catalog mode '{stance}' (its modes: {:?})",
                archetype.slug,
                archetype
                    .modes
                    .iter()
                    .map(|mode| mode.mode_name.as_str())
                    .collect::<Vec<_>>()
            ),
        })
}

/// Whether an entity of `archetype` holds a weapon while it is in `mode`: the mode's override
/// for the ground and the air weapon where it has one, else the archetype's own.
#[must_use]
pub fn armed_in_mode(archetype: &UnitArchetype, mode: &UnitModeSpec) -> bool {
    let own_ground = archetype.weapon.is_some() || archetype.ground_weapon.is_some();
    let own_air = archetype.weapon.is_some() || archetype.air_weapon.is_some();
    let ground = mode
        .ground_weapon_override
        .as_ref()
        .map_or(own_ground, Option::is_some);
    let air = mode
        .air_weapon_override
        .as_ref()
        .map_or(own_air, Option::is_some);
    ground || air
}

/// The slices the atlas of `mode` must hold, in [`SliceKey`] order: every state the client can
/// display while the unit is in the mode, then both morphs.
///
/// - `Idle` always: every entity idles.
/// - `Walk` when the mode is not immobile (locomotion runs only for a mobile actor).
/// - `Attack` when the unit is armed in the mode: the simulation publishes it for a mobile
///   actor, and the client displays it during an immobile actor's attack cycle
///   (`client::flipbook_clock::displayed_action`), as for a sieged cannon.
/// - `Cast` when the archetype harvests or constructs.
/// - `Enter` and `Leave`, the morphs.
#[must_use]
pub fn required_mode_slices(archetype: &UnitArchetype, mode: &UnitModeSpec) -> Vec<SliceKey> {
    let casts = archetype.capabilities.harvester.is_some() || archetype.capabilities.can_construct;
    [
        (AnimState::Idle, true),
        (AnimState::Walk, !mode.is_immobile),
        (AnimState::Attack, armed_in_mode(archetype, mode)),
        (AnimState::Cast, casts),
    ]
    .into_iter()
    .filter(|(_, present)| *present)
    .map(|(state, _)| SliceKey::Action(state))
    .chain(Morph::ALL.into_iter().map(SliceKey::Morph))
    .collect()
}

/// The media of one mode in `media_style.ron`: the text of each slice's clip and the loop
/// treatment of its looping states.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeMediaSpec {
    /// The unit's look for the mode's prompts, replacing its Idle `veo_prompts` entry when that
    /// entry describes something the mode cannot keep (a unit floating in empty space that
    /// must dig into soil) or leaves too little room under `max_prompt_chars`.
    #[serde(default)]
    pub look: Option<String>,
    /// Action text of the enter morph (default stance into the mode).
    pub enter: String,
    /// Action text of the leave morph (the mode back to the default stance).
    pub leave: String,
    /// Action text of each state shown in the mode; exactly the states of
    /// [`required_mode_slices`].
    pub texts: BTreeMap<AnimState, String>,
    /// Loop treatment of the mode's states: whether the request pins the clip's last frame to
    /// the mode's picture, and the bake's seam repair. Only a state the mode shows may appear,
    /// and only a looping one may have a seam.
    #[serde(default)]
    pub loops: BTreeMap<AnimState, LoopSpec>,
    /// Room around the unit for the mode's clips, when the mode's pose leaves the default
    /// frame (a sieged cannon's raised barrels). `None` (the default): the clips start on the
    /// unit's Idle picture itself and bake with the unit's framing.
    #[serde(default)]
    pub reframe: Option<Reframe>,
}

/// A mode's reframe (`modes.<slug>.<mode>.reframe` in `media_style.ron`): its clips start on
/// the unit's Idle picture shrunk about its centre ([`crate::framing::reframe_picture`], saved
/// as [`ModeAtlasId::reframed_idle_picture`]) and are baked with
/// [`crate::BakeFraming::ReframedLetterbox`], which crops them back to the default atlas's
/// scale and place and keeps the margin above and below the unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reframe {
    /// The scale of the unit in the reframed picture, in thousandths, from
    /// [`crate::framing::MIN_REFRAME_PERMILLE`] to [`crate::framing::MAX_REFRAME_PERMILLE`].
    pub scale_permille: u32,
    /// Width of the blend from the shrunk picture to the margin, in thousandths of the picture's
    /// height, at most [`crate::framing::MAX_FEATHER_PERMILLE`].
    pub feather_permille: u32,
}

impl Reframe {
    /// Why this reframe is invalid, or `None` when it is in range.
    #[must_use]
    pub fn issue(&self) -> Option<String> {
        use crate::framing::{MAX_FEATHER_PERMILLE, MAX_REFRAME_PERMILLE, MIN_REFRAME_PERMILLE};
        if !(MIN_REFRAME_PERMILLE..=MAX_REFRAME_PERMILLE).contains(&self.scale_permille) {
            return Some(format!(
                "reframe scale_permille {} is outside {MIN_REFRAME_PERMILLE} to {MAX_REFRAME_PERMILLE}",
                self.scale_permille
            ));
        }
        (self.feather_permille > MAX_FEATHER_PERMILLE).then(|| {
            format!(
                "reframe feather_permille {} is wider than {MAX_FEATHER_PERMILLE}",
                self.feather_permille
            )
        })
    }
}

impl ModeMediaSpec {
    /// The framing the mode's clips are baked with: `unit_framing` (the unit's own, from the
    /// `framing` policy), reframed when the spec has a [`Reframe`].
    ///
    /// # Errors
    /// [`BakerError::DataFile`] naming `path` when the spec reframes a unit whose framing is a
    /// centre crop, which would cut the margin the reframe makes.
    pub fn bake_framing(
        &self,
        unit_framing: crate::BakeFraming,
        path: &Path,
    ) -> Result<crate::BakeFraming, BakerError> {
        let Some(reframe) = self.reframe else {
            return Ok(unit_framing);
        };
        unit_framing
            .reframed(reframe.scale_permille)
            .ok_or_else(|| BakerError::DataFile {
                path: path.to_path_buf(),
                reason: format!(
                    "modes: a reframe needs a letterboxed unit, and this unit's framing is {unit_framing}"
                ),
            })
    }

    /// The action text of `slot`, if the spec has one.
    #[must_use]
    pub fn text(&self, slot: SliceKey) -> Option<&str> {
        match slot {
            SliceKey::Morph(Morph::Enter) => Some(&self.enter),
            SliceKey::Morph(Morph::Leave) => Some(&self.leave),
            SliceKey::Action(state) => self.texts.get(&state).map(String::as_str),
        }
    }

    /// Every action text with its slice, in [`SliceKey`] order.
    pub fn slot_texts(&self) -> impl Iterator<Item = (SliceKey, &str)> {
        self.texts
            .iter()
            .map(|(state, text)| (SliceKey::Action(*state), text.as_str()))
            .chain([
                (SliceKey::Morph(Morph::Enter), self.enter.as_str()),
                (SliceKey::Morph(Morph::Leave), self.leave.as_str()),
            ])
    }

    /// Whether the clip of `slot` pins its last frame: a state whose loop treatment says so.
    /// The morphs' end pictures are decided by [`plan_mode_pictures`], not here.
    #[must_use]
    pub fn pins_end(&self, slot: SliceKey) -> bool {
        match slot {
            SliceKey::Action(state) => self.loops.get(&state).is_some_and(|spec| spec.pin_end),
            SliceKey::Morph(_) => false,
        }
    }
}

/// The `modes` section of `media_style.ron`: exact catalog slug, then catalog mode name, then
/// that mode's media. A unit or mode it does not name keeps showing its default atlas.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ModeStyle(pub BTreeMap<String, BTreeMap<String, ModeMediaSpec>>);

impl ModeStyle {
    /// The media of `id`'s mode, if the style names it.
    #[must_use]
    pub fn spec(&self, id: ModeAtlasId<'_>) -> Option<&ModeMediaSpec> {
        self.0.get(id.slug).and_then(|modes| modes.get(id.stance))
    }

    /// Every mode the style names, in slug then mode order.
    pub fn modes(&self) -> impl Iterator<Item = (ModeAtlasId<'_>, &ModeMediaSpec)> {
        self.0.iter().flat_map(|(slug, modes)| {
            modes.iter().map(move |(stance, spec)| {
                (
                    ModeAtlasId {
                        slug: slug.as_str(),
                        stance: stance.as_str(),
                    },
                    spec,
                )
            })
        })
    }

    /// Checks the section loaded from `path` against `catalog`.
    ///
    /// # Errors
    /// [`BakerError::DataFile`] naming the first problem: a slug the catalog does not hold
    /// exactly, a mode the unit does not have, a state the mode needs without a text or a text
    /// for a state the mode never shows, an empty text, a loop treatment for a state the mode
    /// does not show, a seam on a state that does not loop, a cross-fade longer than
    /// [`MAX_CROSSFADE_FRAMES`] or a quarter of the slice's frame budget, or a reframe out of range.
    pub fn check(&self, catalog: &UnitCatalog, path: &Path) -> Result<(), BakerError> {
        let invalid = |reason: String| BakerError::DataFile {
            path: path.to_path_buf(),
            reason,
        };
        for (id, spec) in self.modes() {
            let archetype = catalog
                .archetypes()
                .find(|archetype| archetype.slug == id.slug)
                .ok_or_else(|| {
                    invalid(format!("modes: '{}' is not an exact catalog slug", id.slug))
                })?;
            let (_, mode) = catalog_mode(archetype, id.stance, path)?;
            let required = required_mode_slices(archetype, mode);
            let label = id.atlas_name();
            for slot in &required {
                match spec.text(*slot) {
                    None => {
                        return Err(invalid(format!(
                            "modes: {label} shows {slot} but has no text for it"
                        )))
                    }
                    Some(text) if text.trim().is_empty() => {
                        return Err(invalid(format!("modes: {label} {slot} text is empty")))
                    }
                    Some(_) => {}
                }
            }
            if spec
                .look
                .as_deref()
                .is_some_and(|look| look.trim().is_empty())
            {
                return Err(invalid(format!("modes: {label} look is empty")));
            }
            if let Some(issue) = spec.reframe.as_ref().and_then(Reframe::issue) {
                return Err(invalid(format!("modes: {label} {issue}")));
            }
            if let Some(state) = spec
                .texts
                .keys()
                .find(|state| !required.contains(&SliceKey::Action(**state)))
            {
                return Err(invalid(format!(
                    "modes: {label} has a text for {state:?}, which the mode never shows"
                )));
            }
            let budget = frames_per_state(DEFAULT_UNIT_FRAME_SIZE, required.len());
            for (state, loop_spec) in &spec.loops {
                if !required.contains(&SliceKey::Action(*state)) {
                    return Err(invalid(format!(
                        "modes: {label} has a loop treatment for {state:?}, which the mode never shows"
                    )));
                }
                let Some(seam) = loop_spec.seam else {
                    continue;
                };
                if !atlas_loops(*state) {
                    return Err(invalid(format!(
                        "modes: {label} {state:?} does not loop, so it cannot have a seam"
                    )));
                }
                let quarter = budget / 4;
                if seam.crossfade_frames > MAX_CROSSFADE_FRAMES
                    || usize::from(seam.crossfade_frames) > quarter
                {
                    return Err(invalid(format!(
                        "modes: {label} {state:?} cross-fades {} frames, more than {MAX_CROSSFADE_FRAMES} or a quarter of its {budget}-frame budget",
                        seam.crossfade_frames
                    )));
                }
            }
        }
        Ok(())
    }

    /// Checks that every reframed mode's unit is letterboxed under `framing`, the `framing`
    /// section of the style loaded from `path` ([`ModeMediaSpec::bake_framing`]). The mode-media
    /// commands run it after loading, so no clip is paid for that its bake would refuse.
    ///
    /// # Errors
    /// [`BakerError::DataFile`] for a reframed mode of a centre-cropped unit.
    pub fn check_framing(
        &self,
        framing: &crate::FramingPolicy,
        catalog: &UnitCatalog,
        path: &Path,
    ) -> Result<(), BakerError> {
        for (id, spec) in self.modes() {
            if let Some(archetype) = catalog.get_by_slug(id.slug) {
                spec.bake_framing(framing.framing_for(id.slug, &archetype.shape), path)
                    .map_err(|error| BakerError::DataFile {
                        path: path.to_path_buf(),
                        reason: format!("{}: {error}", id.atlas_name()),
                    })?;
            }
        }
        Ok(())
    }
}

/// Composes the prompt of `id`'s `slot` clip: the mode's `look` if the style gives one, else the
/// unit's Idle `veo_prompts` entry, then the slot's action text from the style's `modes`
/// section, then the style sentence.
///
/// # Errors
/// [`BakerError::UnknownArchetype`] for a slug that is not an exact catalog slug, and
/// [`BakerError::DataFile`] naming the style file for a mode the unit does not have or a slot
/// without a text.
pub fn build_mode_prompt(
    catalog: &UnitCatalog,
    style: &MediaStyle,
    id: ModeAtlasId<'_>,
    slot: SliceKey,
    style_path: &Path,
) -> Result<ComposedPrompt, BakerError> {
    let archetype = crate::veo_client::exact_catalog_archetype(id.slug, catalog)?;
    catalog_mode(archetype, id.stance, style_path)?;
    let spec = style.modes.spec(id);
    let text = spec
        .and_then(|spec| spec.text(slot))
        .ok_or_else(|| BakerError::DataFile {
            path: style_path.to_path_buf(),
            reason: format!("modes: {} has no text for {slot}", id.atlas_name()),
        })?;
    let look = spec
        .and_then(|spec| spec.look.as_deref())
        .or_else(|| archetype.veo_prompt(AnimState::Idle));
    Ok(match look {
        Some(look) => ComposedPrompt {
            text: format!("{look} {text} {}", style.style),
            authored: true,
        },
        None => ComposedPrompt {
            text: format!(
                "{}: {} {text} {}",
                archetype.display_name, archetype.lore_description, style.style
            ),
            authored: false,
        },
    })
}

/// The pictures one mode clip sends: its start picture, and the end picture sent as
/// `lastFrame`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModePictures {
    /// The start picture.
    pub start: PathBuf,
    /// The end picture, if the clip ends on a fixed picture.
    pub end: Option<PathBuf>,
}

/// The pictures of `id`'s `slot` clip (see the module docs):
///
/// | Slot | Start | End |
/// |---|---|---|
/// | `Enter` | the default Idle picture | the mode's picture if it exists (a regeneration) |
/// | a state | the mode's picture | the mode's picture when the state is pinned |
/// | `Leave` | the mode's picture | the default Idle picture |
///
/// `default_idle` is the unit's Idle picture (`veo_plan::resolve_keyframe` of Idle). A
/// reframed mode ([`Reframe`]) uses its reframed Idle picture
/// ([`ModeAtlasId::reframed_idle_picture`]) in its place, so the morphs start and end on the
/// same framing as the mode's clips.
///
/// # Errors
/// [`BakerError::DataFile`] naming the mode's picture when a slot needs it and it does not
/// exist yet: the enter morph is generated first, and its last frame is extracted as the
/// picture (`keyframe-from-clip`). For a reframed mode, likewise naming the reframed Idle
/// picture when a morph needs it and it does not exist (`reframe-mode-picture`).
pub fn plan_mode_pictures(
    spec: &ModeMediaSpec,
    id: ModeAtlasId<'_>,
    slot: SliceKey,
    default_idle: &Path,
    keyframes_dir: &Path,
) -> Result<ModePictures, BakerError> {
    let picture = id.picture(keyframes_dir);
    let existing = picture.is_file().then(|| picture.clone());
    let need_picture = || {
        existing.clone().ok_or_else(|| BakerError::DataFile {
            path: picture.clone(),
            reason: format!(
                "{} {slot} starts on the mode's picture, which does not exist yet: generate the enter morph first and extract its last frame with `keyframe-from-clip`",
                id.atlas_name()
            ),
        })
    };
    let idle = || -> Result<PathBuf, BakerError> {
        if spec.reframe.is_none() {
            return Ok(default_idle.to_path_buf());
        }
        let reframed = id.reframed_idle_picture(keyframes_dir);
        if reframed.is_file() {
            Ok(reframed)
        } else {
            Err(BakerError::DataFile {
                path: reframed,
                reason: format!(
                    "{} is reframed, and its {slot} needs the reframed Idle picture: write it with `reframe-mode-picture` first",
                    id.atlas_name()
                ),
            })
        }
    };
    Ok(match slot {
        SliceKey::Morph(Morph::Enter) => ModePictures {
            start: idle()?,
            end: existing.clone(),
        },
        SliceKey::Morph(Morph::Leave) => ModePictures {
            start: need_picture()?,
            end: Some(idle()?),
        },
        SliceKey::Action(_) => {
            let start = need_picture()?;
            let end = spec.pins_end(slot).then(|| start.clone());
            ModePictures { start, end }
        }
    })
}

/// Every issue of a mode clip's prompt and pictures under `style`'s rules: those of a unit
/// clip's prompt ([`check_prompt`]), a readable start picture, and an end picture that passes
/// the pin rules ([`check_pin`]).
#[must_use]
pub fn check_mode_clip(
    style: &MediaStyle,
    prompt: &ComposedPrompt,
    pictures: &ModePictures,
) -> Vec<PromptIssue> {
    let mut issues = check_prompt(style, prompt);
    issues.extend(check_keyframe(&KeyframeSource::File(
        pictures.start.clone(),
    )));
    if let Some(end) = &pictures.end {
        issues.extend(check_pin(end));
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_keep_slug_and_mode_apart() {
        let id = ModeAtlasId {
            slug: "fixture_unit",
            stance: "fixture_mode",
        };
        assert_eq!(id.atlas_name(), "fixture_unit-fixture_mode");
        assert_eq!(
            id.clip_file(SliceKey::Morph(Morph::Enter)),
            "fixture_unit-fixture_mode_enter.mp4"
        );
        assert_eq!(
            id.clip_file(SliceKey::Action(AnimState::Idle)),
            "fixture_unit-fixture_mode_idle.mp4"
        );
        assert_eq!(
            id.picture(Path::new("frames")),
            Path::new("frames").join("fixture_unit-fixture_mode.png")
        );
        assert!(crate::media::media_paths(Path::new("."), &id.atlas_name()).is_ok());
    }

    #[test]
    fn slices_parse_back_from_their_stems() {
        for slot in AnimState::ALL
            .into_iter()
            .map(SliceKey::Action)
            .chain(Morph::ALL.into_iter().map(SliceKey::Morph))
        {
            assert_eq!(SliceKey::parse(slot.stem()), Ok(slot));
        }
        assert!(SliceKey::parse("sieged").is_err());
        assert!(
            SliceKey::Action(AnimState::Die) < SliceKey::Morph(Morph::Enter),
            "actions order before morphs"
        );
    }
}
