//! Flipbook atlas metadata definitions and RON serialization.
//!
//! This module defines [`FlipbookAtlasMeta`], [`ActionSlice`], and [`FlipbookLayout`],
//! which describe the layout of packed image atlases, frame rates, dimensions, and
//! the sub-slice ranges corresponding to gameplay animation states ([`AnimState`]).
//!
//! A mode atlas (plan `2026-11-09-effect-visualization.md` WP13, Amendment 10) holds the clips
//! of one catalog mode of a unit: its [`AnimState`] slices in `actions`, and the two morphs into
//! and out of the mode ([`Morph`]) in `morphs`. A default atlas has no morphs, and the field is
//! then neither read nor written, so every manifest baked before it keeps its bytes.
//!
//! In accordance with the Space Scuffle architectural charter, all action mappings use
//! deterministic ordered collections ([`BTreeMap`]) to guarantee reproducible serialization
//! across runs and platform targets. Written files end every line in `\n` on every host.

use crate::error::BakerError;
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use sim_types::enums::AnimState;
use std::collections::BTreeMap;
use std::path::Path;

/// Line terminator of every written `*.flipbook.ron` file.
///
/// Invariant: flipbook metadata is committed and digested, so its bytes must not depend on the
/// host that baked it. `ron` defaults `PrettyConfig::new_line` to `\r\n` when built for Windows,
/// which is why this value is always set explicitly.
const MANIFEST_RON_NEWLINE: &str = "\n";

/// Applies the canonical `*.flipbook.ron` layout to `base`, replacing its newline.
///
/// `base` is a parameter so that tests can start from the Windows default on any host.
fn canonical_pretty_config(base: PrettyConfig) -> PrettyConfig {
    base.new_line(MANIFEST_RON_NEWLINE.to_string())
}

/// Layout format of the packed flipbook atlas image file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlipbookLayout {
    /// Single vertical strip of frames stacked along the Y axis.
    ///
    /// The resulting image has dimensions:
    /// - `width = frame_width`
    /// - `height = frame_height * total_frames`
    VerticalStrip,
    /// Two-dimensional grid sheet with explicit columns and rows.
    Grid {
        /// Number of frame columns along the horizontal axis.
        columns: u32,
        /// Number of frame rows along the vertical axis.
        rows: u32,
    },
}

/// Sliced range of frames corresponding to a specific unit or structure animation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSlice {
    /// Zero-indexed starting frame index in the packed atlas.
    pub start_frame: u32,
    /// Total number of consecutive frames allocated to this action.
    pub frame_count: u32,
    /// Whether the animation cycle loops continuously while the action persists.
    pub looping: bool,
    /// If non-looping, whether the renderer holds on the final frame (e.g., Die or Constructing finish).
    pub hold_last_frame: bool,
    /// Real play time of one pass through the slice, in milliseconds (plan
    /// `docs/plans/2026-11-04-battle-flipbook-animation.md`, D-6), so a clip plays at the speed
    /// it was filmed at however few of its frames the atlas keeps.
    ///
    /// Written by the video bake from the clip's decoded frame count at the decode rate
    /// ([`crate::ffmpeg_importer::clip_duration_ms`]). `None` for slices with no source clip
    /// length (procedural atlases, a still image, every manifest baked before the field
    /// existed); the renderer then plays `frame_count / fps`. An absent value is neither read
    /// nor written, so older manifests keep their bytes and provenance hashes. Never `Some(0)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
}

/// A morph between a unit's default stance and one of its catalog modes (plan
/// `2026-11-09-effect-visualization.md` WP13): the clip played while the simulation channels the
/// mode switch (`sim_types::ModeTransitionView`), stretched over the channel's nominal ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Morph {
    /// From the default stance into the mode (siege, burrow).
    Enter,
    /// From the mode back to the default stance (unsiege, unburrow).
    Leave,
}

impl Morph {
    /// Both morphs, in declaration order.
    pub const ALL: [Self; 2] = [Self::Enter, Self::Leave];

    /// File-name stem of the morph (`enter`, `leave`).
    #[must_use]
    pub const fn stem(self) -> &'static str {
        match self {
            Self::Enter => "enter",
            Self::Leave => "leave",
        }
    }
}

/// What one slice of an atlas shows: an animation state, or a morph of a mode atlas.
///
/// Ordered with every action before every morph, so a bake that packs its clips in key order
/// lays a mode atlas out as its actions, then its morphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SliceKey {
    /// An animation state the simulation publishes (or the client displays).
    Action(AnimState),
    /// A morph into or out of the atlas's mode.
    Morph(Morph),
}

impl SliceKey {
    /// File-name stem of the slice: the state's (`idle`, `attack`, ...) or the morph's
    /// (`enter`, `leave`).
    #[must_use]
    pub fn stem(self) -> &'static str {
        match self {
            Self::Action(state) => crate::veo_plan::state_slug(state),
            Self::Morph(morph) => morph.stem(),
        }
    }

    /// Parses a stem written by [`Self::stem`].
    ///
    /// # Errors
    /// [`UnknownSlice`] for any other text.
    pub fn parse(text: &str) -> Result<Self, UnknownSlice> {
        Morph::ALL
            .into_iter()
            .find(|morph| morph.stem() == text)
            .map(Self::Morph)
            .or_else(|| crate::veo_plan::parse_state(text).ok().map(Self::Action))
            .ok_or_else(|| UnknownSlice {
                name: text.to_string(),
            })
    }
}

impl std::fmt::Display for SliceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.stem())
    }
}

/// A text that names no slice.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown slice '{name}'; valid: idle, walk, attack, constructing, cast, die, enter, leave")]
pub struct UnknownSlice {
    /// The text given.
    pub name: String,
}

/// Serializes a [`SliceKey`] as its stem (`idle`, `enter`, ...), as the ledger writes it.
pub mod slice_name {
    use super::SliceKey;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Writes the slice's stem.
    ///
    /// # Errors
    /// Only the serializer's own errors.
    pub fn serialize<S: Serializer>(slice: &SliceKey, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(slice.stem())
    }

    /// Reads a slice's stem.
    ///
    /// # Errors
    /// A string that names no slice.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<SliceKey, D::Error> {
        let text = String::deserialize(deserializer)?;
        SliceKey::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Authoritative flipbook atlas metadata matching `<atlas_name>.flipbook.ron`.
///
/// Contains all geometric and timing parameters necessary for the client rendering
/// layer to sample individual animation frames from GPU textures or Texture2DArrays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlipbookAtlasMeta {
    /// Identifier name of the atlas (e.g. `"marine"`, `"concept_video"`).
    pub name: String,
    /// Width of a single animation frame in pixels.
    pub frame_width: u32,
    /// Height of a single animation frame in pixels.
    pub frame_height: u32,
    /// Playback framerate in frames per second (FPS).
    pub fps: u32,
    /// Total number of frames packed in the atlas image.
    pub total_frames: u32,
    /// Geometric packing layout of frames.
    pub layout: FlipbookLayout,
    /// Deterministic mapping of animation states to frame slices.
    #[serde(deserialize_with = "deserialize_unique_actions")]
    pub actions: BTreeMap<AnimState, ActionSlice>,
    /// The morphs of a mode atlas, played once over the mode switch's channel and held on their
    /// last frame (see the module docs). Empty, and then neither read nor written, for a
    /// default atlas.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub morphs: BTreeMap<Morph, ActionSlice>,
}

fn deserialize_unique_actions<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<AnimState, ActionSlice>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct UniqueActions;
    impl<'de> serde::de::Visitor<'de> for UniqueActions {
        type Value = BTreeMap<AnimState, ActionSlice>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an animation map with each action declared exactly once")
        }

        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut actions = BTreeMap::new();
            while let Some((action, slice)) = map.next_entry::<AnimState, ActionSlice>()? {
                match actions.entry(action) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(slice);
                    }
                    std::collections::btree_map::Entry::Occupied(_) => {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate animation action {action:?}"
                        )));
                    }
                }
            }
            Ok(actions)
        }
    }
    deserializer.deserialize_map(UniqueActions)
}

impl FlipbookAtlasMeta {
    /// Validates all structural invariants of the atlas metadata.
    ///
    /// # Invariants Checked:
    /// 1. Frame dimensions must be strictly positive (`> 0`).
    /// 2. Playback framerate must be strictly positive (`> 0`).
    /// 3. Total frame count must be strictly positive (`> 0`).
    /// 4. Every action slice must contain at least 1 frame (`frame_count > 0`).
    /// 5. Every action slice's range (`start_frame + frame_count`) must not exceed `total_frames`.
    /// 6. An action slice's `duration_ms`, when present, must be strictly positive.
    /// 7. If layout is `Grid`, `columns * rows` must be `>= total_frames`.
    ///
    /// # Errors
    /// Returns [`BakerError::ValidationFailed`] if any invariant is violated.
    pub fn validate(&self) -> Result<(), BakerError> {
        if self.frame_width == 0 || self.frame_height == 0 {
            return Err(BakerError::ValidationFailed {
                name: self.name.clone(),
                reason: "Frame dimensions must be strictly greater than zero".into(),
            });
        }
        if self.fps == 0 {
            return Err(BakerError::ValidationFailed {
                name: self.name.clone(),
                reason: "Framerate (FPS) must be strictly greater than zero".into(),
            });
        }
        if self.total_frames == 0 {
            return Err(BakerError::ValidationFailed {
                name: self.name.clone(),
                reason: "Total frames must be strictly greater than zero".into(),
            });
        }
        if let FlipbookLayout::Grid { columns, rows } = self.layout {
            if columns == 0 || rows == 0 {
                return Err(BakerError::ValidationFailed {
                    name: self.name.clone(),
                    reason: "Grid layout columns and rows must both be greater than zero".into(),
                });
            }
            if columns.saturating_mul(rows) < self.total_frames {
                return Err(BakerError::ValidationFailed {
                    name: self.name.clone(),
                    reason: format!(
                        "Grid capacity ({}x{} = {}) is smaller than total frames ({})",
                        columns,
                        rows,
                        columns.saturating_mul(rows),
                        self.total_frames
                    ),
                });
            }
        }
        let actions = self
            .actions
            .iter()
            .map(|(state, slice)| (SliceKey::Action(*state), slice));
        let morphs = self
            .morphs
            .iter()
            .map(|(morph, slice)| (SliceKey::Morph(*morph), slice));
        for (state, slice) in actions.chain(morphs) {
            if slice.frame_count == 0 {
                return Err(BakerError::ValidationFailed {
                    name: self.name.clone(),
                    reason: format!("Slice '{state}' has a frame count of 0"),
                });
            }
            if slice.duration_ms == Some(0) {
                return Err(BakerError::ValidationFailed {
                    name: self.name.clone(),
                    reason: format!(
                        "Slice '{state}' has a duration of 0 ms; an unknown duration is omitted"
                    ),
                });
            }
            let end_frame = slice.start_frame.saturating_add(slice.frame_count);
            if end_frame > self.total_frames {
                return Err(BakerError::ValidationFailed {
                    name: self.name.clone(),
                    reason: format!(
                        "Slice '{state}' range ({}..{}) exceeds total frames {}",
                        slice.start_frame, end_frame, self.total_frames
                    ),
                });
            }
        }
        Ok(())
    }

    /// Serializes this metadata struct into formatted RON format and writes it to disk.
    ///
    /// # Errors
    /// Returns [`BakerError::ValidationFailed`] if metadata fails validation,
    /// [`BakerError::RonError`] on serialization failure, or [`BakerError::Io`] on write error.
    pub fn save_to_ron(&self, path: &Path) -> Result<(), BakerError> {
        self.validate()?;
        let ron_string =
            ron::ser::to_string_pretty(self, canonical_pretty_config(PrettyConfig::default()))?;
        std::fs::write(path, ron_string).map_err(|source| BakerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }

    /// Loads and validates metadata from a `.ron` file on disk.
    ///
    /// # Errors
    /// Returns [`BakerError::Io`] on read failure, [`BakerError::RonError`] on parsing error,
    /// or [`BakerError::ValidationFailed`] if deserialized metadata violates invariants.
    pub fn load_from_ron(path: &Path) -> Result<Self, BakerError> {
        let content = std::fs::read_to_string(path).map_err(|source| BakerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let meta: Self = ron::from_str(&content)?;
        meta.validate()?;
        Ok(meta)
    }
}

#[cfg(test)]
mod tests {
    use super::{canonical_pretty_config, FlipbookAtlasMeta, FlipbookLayout, MANIFEST_RON_NEWLINE};
    use ron::ser::PrettyConfig;
    use std::collections::BTreeMap;

    /// The config `PrettyConfig::default()` returns on Windows, reproduced on any host.
    ///
    /// On Linux the `ron` default is already `\n`, so only this base can show the override working.
    fn windows_default() -> PrettyConfig {
        PrettyConfig::default().new_line("\r\n".to_string())
    }

    fn sample() -> FlipbookAtlasMeta {
        FlipbookAtlasMeta {
            name: "newline_probe".into(),
            frame_width: 64,
            frame_height: 64,
            fps: 15,
            total_frames: 4,
            layout: FlipbookLayout::VerticalStrip,
            actions: BTreeMap::new(),
            morphs: BTreeMap::new(),
        }
    }

    #[test]
    fn canonical_config_replaces_a_windows_default_newline() {
        assert_eq!(MANIFEST_RON_NEWLINE, "\n");
        assert_eq!(
            canonical_pretty_config(windows_default()).new_line,
            MANIFEST_RON_NEWLINE
        );
    }

    #[test]
    fn encoding_from_a_windows_default_writes_no_carriage_return() -> Result<(), ron::Error> {
        let encoded =
            ron::ser::to_string_pretty(&sample(), canonical_pretty_config(windows_default()))?;
        assert!(encoded.contains('\n'));
        assert!(!encoded.contains('\r'));
        Ok(())
    }

    #[test]
    fn save_to_ron_writes_no_carriage_return() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("probe.flipbook.ron");
        sample().save_to_ron(&path)?;
        let bytes = std::fs::read(&path)?;
        assert!(bytes.contains(&b'\n'));
        assert!(!bytes.contains(&b'\r'));
        Ok(())
    }
}
