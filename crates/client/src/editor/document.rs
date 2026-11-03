//! In-memory editor document state, dirty tracking, and simulation sync.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §6.1 and `AGENTS.md`,
//! `EditorDocument` is the authoritative ECS resource representing the editable map document in the client.
//!
//! # Invariants Maintained
//! 1. **Immediate Live Feedback:** Every edit updates `doc` synchronously and marks dirty.
//! 2. **Efficient Terrain Synchronization:** Cell changes are propagated directly to [`TerrainState`]
//!    via incremental [`ChangeSet`] bounding rectangles rather than rebuilding the full [`GameMap`](sim_map::GameMap).
//! 3. **Validation Traceability:** `validation` reflects the map state at `validated_revision`, allowing
//!    the UI to report diagnostic state and highlight invalid cells without blocking the render loop.

use std::path::PathBuf;

use bevy::prelude::Resource;
use sim_map::map_format::{MapDocument, MapFormatError, MapMetadata};
use sim_map::validation::{validate_map, MapValidationReport};

use crate::editor::commands::ChangeSet;
use crate::terrain::TerrainState;

/// Authoritative editor map document ECS resource.
#[derive(Resource, Debug, Clone)]
pub struct EditorDocument {
    /// Editable map document containing cells, entity lists, and metadata.
    pub doc: MapDocument,
    /// Absolute filesystem path to the currently opened file, or `None` if untitled.
    pub path: Option<PathBuf>,
    /// Whether the document contains unsaved modifications since the last save or open.
    pub dirty: bool,
    /// Monotonically increasing revision counter incremented on every applied/undone/redone change.
    pub revision: u64,
    /// Most recent validation report.
    pub validation: MapValidationReport,
    /// Revision counter value at the time `validation` was computed.
    pub validated_revision: u64,
    /// High-precision timestamp (seconds since editor startup) of the most recent modification.
    pub last_edit_at_secs: f64,
}

impl EditorDocument {
    /// Constructs a new editor document wrapping `doc`, computing initial validation.
    #[must_use]
    pub fn new(doc: MapDocument, path: Option<PathBuf>) -> Self {
        let validation = validate_map(&doc);
        Self {
            doc,
            path,
            dirty: false,
            revision: 0,
            validation,
            validated_revision: 0,
            last_edit_at_secs: 0.0,
        }
    }

    /// Creates a new untitled blank document of dimensions `width × height`.
    ///
    /// # Errors
    /// Returns `Err(MapFormatError::DimensionsOutOfRange)` if dimensions fall outside `[16, 256]`.
    pub fn new_blank(
        width: u32,
        height: u32,
        metadata: MapMetadata,
    ) -> Result<Self, MapFormatError> {
        let doc = MapDocument::new_blank(width, height, metadata)?;
        Ok(Self::new(doc, None))
    }

    /// Advances revision and marks document dirty when mutations occur in `cs`.
    pub fn apply_changeset(&mut self, cs: &ChangeSet, now_secs: f64) {
        if !cs.is_empty() {
            self.revision = self.revision.wrapping_add(1);
            self.dirty = true;
            self.last_edit_at_secs = now_secs;
        }
    }

    /// Synchronizes terrain representation with the document changes in `cs`.
    ///
    /// # Performance Rationale
    /// Why: Converting the full document into a `GameMap` via `to_game_map` takes $O(W \times H)$ memory copies
    /// and triggers a full quad-tree/chunk GPU mesh recreation.
    /// What: We only apply mutations within `cs.cells` bounding rectangle using [`TerrainState::apply_cells`].
    /// Trade-off: Incremental per-stroke update takes $O(\Delta)$ instead of $O(W \times H)$.
    pub fn sync_terrain(&self, cs: &ChangeSet, terrain: &mut TerrainState) {
        if let Some(rect) = cs.cells {
            terrain.apply_cells(rect, |x, y, target_cell| {
                if let Some(spec) = self.doc.cell(x, y) {
                    *target_cell = MapDocument::to_map_cell(spec);
                }
            });
        }
    }

    /// Re-runs map validation on `doc`, updating `validation` and `validated_revision`.
    pub fn revalidate(&mut self) {
        self.validation = validate_map(&self.doc);
        self.validated_revision = self.revision;
    }

    /// Returns `true` if `validation` corresponds to the latest `revision`.
    #[must_use]
    pub fn is_validation_current(&self) -> bool {
        self.validated_revision == self.revision
    }

    /// Marks the document saved at the specified filesystem path, clearing dirty flag.
    pub fn mark_saved(&mut self, path: PathBuf) {
        self.path = Some(path);
        self.dirty = false;
    }
}
