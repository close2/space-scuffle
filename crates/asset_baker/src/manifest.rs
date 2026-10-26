//! Flipbook atlas metadata definitions and RON serialization.
//!
//! This module defines [`FlipbookAtlasMeta`], [`ActionSlice`], and [`FlipbookLayout`],
//! which describe the layout of packed image atlases, frame rates, dimensions, and
//! the sub-slice ranges corresponding to gameplay animation states ([`AnimState`]).
//!
//! In accordance with the Space Scuffle architectural charter, all action mappings use
//! deterministic ordered collections ([`BTreeMap`]) to guarantee reproducible serialization
//! across runs and platform targets.

use crate::error::BakerError;
use serde::{Deserialize, Serialize};
use sim_types::enums::AnimState;
use std::collections::BTreeMap;
use std::path::Path;

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
pub struct ActionSlice {
    /// Zero-indexed starting frame index in the packed atlas.
    pub start_frame: u32,
    /// Total number of consecutive frames allocated to this action.
    pub frame_count: u32,
    /// Whether the animation cycle loops continuously while the action persists.
    pub looping: bool,
    /// If non-looping, whether the renderer holds on the final frame (e.g., Die or Constructing finish).
    pub hold_last_frame: bool,
}

/// Authoritative flipbook atlas metadata matching `<atlas_name>.flipbook.ron`.
///
/// Contains all geometric and timing parameters necessary for the client rendering
/// layer to sample individual animation frames from GPU textures or Texture2DArrays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub actions: BTreeMap<AnimState, ActionSlice>,
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
    /// 6. If layout is `Grid`, `columns * rows` must be `>= total_frames`.
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
        for (state, slice) in &self.actions {
            if slice.frame_count == 0 {
                return Err(BakerError::ValidationFailed {
                    name: self.name.clone(),
                    reason: format!("Action state '{state:?}' has a frame count of 0"),
                });
            }
            let end_frame = slice.start_frame.saturating_add(slice.frame_count);
            if end_frame > self.total_frames {
                return Err(BakerError::ValidationFailed {
                    name: self.name.clone(),
                    reason: format!(
                        "Action state '{state:?}' range ({}..{}) exceeds total frames {}",
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
        let pretty_config = ron::ser::PrettyConfig::default();
        let ron_string = ron::ser::to_string_pretty(self, pretty_config)?;
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
