//! Active drag-stroke recorder with single-touch generation stamping.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §6.3 and `AGENTS.md`,
//! this module provides the [`StrokeRecorder`] for interactive brush painting, ensuring each cell
//! is touched at most once per drag stroke with live feedback and zero per-frame allocations.
//!
//! # Invariants Maintained
//! 1. **Idempotent Single-Touch:** Cells visited multiple times during a single drag stroke record
//!    their original `before` state on first touch and are not overwritten on subsequent touches.
//! 2. **Live Feedback:** Document cells are mutated immediately during `touch` for real-time visualization.
//! 3. **Zero Per-Frame Allocations:** Scratch buffers (`stamps` and `deltas`) retain capacity across strokes;
//!    only finalized commands allocate exact-sized delta vectors.
//! 4. **Safe Generation Wrap:** Generation overflow wraps cleanly by re-zeroing the stamp array and resetting
//!    generation to 1.

use sim_map::map_format::{MapCellSpec, MapDocument};

use crate::editor::commands::{CellDelta, EditCommand};

/// Manages interactive painting strokes with generation stamping.
#[derive(Debug, Clone)]
pub struct StrokeRecorder {
    /// Per-cell generation stamp array of size `width * height`.
    stamps: Vec<u32>,
    /// Monotonically incremented stroke generation counter.
    generation: u32,
    /// Accumulated cell deltas for the active stroke.
    deltas: Vec<CellDelta>,
    /// Grid width in cells.
    width: u32,
    /// Grid height in cells.
    height: u32,
}

impl StrokeRecorder {
    /// Constructs a stroke recorder dimensioned for a `width × height` map.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width as usize) * (height as usize);
        Self {
            stamps: vec![0; size],
            generation: 0,
            deltas: Vec::new(),
            width,
            height,
        }
    }

    /// Resizes internal stamp buffers when map dimensions change.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        let size = (width as usize) * (height as usize);
        self.stamps.resize(size, 0);
        self.stamps.fill(0);
        self.generation = 0;
        self.deltas.clear();
    }

    /// Begins a new stroke, incrementing the generation counter.
    ///
    /// If generation wraps to 0, resets all stamps to 0 and starts at generation 1.
    pub fn begin(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            // PERFORMANCE RATIONALE:
            // Why: u32 generation can theoretically wrap after 4.2 billion strokes.
            // What: When wrap occurs, we re-zero stamps and start at generation 1 so 0 remains the uninitialized sentinel.
            // Trade-off: One-time linear memory fill per 4 billion strokes.
            self.stamps.fill(0);
            self.generation = 1;
        }
        self.deltas.clear();
    }

    /// Touches a cell at `(x, y)`: if not previously touched in the current stroke,
    /// records `before`, mutates `doc` immediately with `f`, and records `after`.
    ///
    /// Returns `true` if the cell was newly touched, `false` if it was already stamped or out of bounds.
    pub fn touch<F>(&mut self, doc: &mut MapDocument, x: u32, y: u32, f: F) -> bool
    where
        F: FnOnce(&MapCellSpec) -> MapCellSpec,
    {
        if x >= self.width || y >= self.height || x >= doc.width || y >= doc.height {
            return false;
        }

        let idx = (y * self.width + x) as usize;
        if idx >= self.stamps.len() {
            return false;
        }

        if self.stamps[idx] == self.generation {
            return false;
        }

        self.stamps[idx] = self.generation;

        if let Some(cell) = doc.cell_mut(x, y) {
            let before = cell.clone();
            let after = f(&before);
            *cell = after.clone();
            self.deltas.push(CellDelta {
                x,
                y,
                before,
                after,
            });
            true
        } else {
            false
        }
    }

    /// Finalizes the stroke, returning an [`EditCommand::PaintCells`] if any cells actually changed.
    ///
    /// Deltas where `before == after` are discarded. Scratch capacity is preserved.
    pub fn finish(&mut self, label: &'static str) -> Option<EditCommand> {
        // PERFORMANCE RATIONALE:
        // Why: Avoid allocating per frame while dragging.
        // What: Drain `self.deltas` filtering for actual modifications into a new Vec.
        // Trade-off: Exactly one allocation when a stroke completes.
        let deltas: Vec<CellDelta> = self
            .deltas
            .drain(..)
            .filter(|d| d.before != d.after)
            .collect();

        if deltas.is_empty() {
            None
        } else {
            Some(EditCommand::PaintCells { label, deltas })
        }
    }

    /// Returns the active generation counter (useful for unit testing wrap-around).
    #[must_use]
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Returns a slice of the pending deltas accumulated in the active stroke.
    #[must_use]
    pub fn deltas(&self) -> &[CellDelta] {
        &self.deltas
    }

    /// Internal test hook for testing generation counter overflow wrap-around behavior
    /// without performing 4.2 billion simulated strokes.
    #[doc(hidden)]
    pub fn set_generation_for_test(&mut self, gen: u32) {
        self.generation = gen;
    }
}
