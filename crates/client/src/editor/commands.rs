//! Command pattern, edit history, and change tracking for the map editor.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §6.2 and `AGENTS.md`,
//! this module provides the pure-logic undo/redo command system, delta tracking, and change set calculation.
//!
//! # Invariants Maintained
//! 1. **Lossless Undo/Redo Round-Trip:** Applying and then reverting any command returns the [`MapDocument`]
//!    to its exact bit-for-bit previous state.
//! 2. **Deterministic Ordering:** Entity insertions and removals maintain strictly sorted vectors by [`MapEntityId`].
//! 3. **Bounded Memory:** Undo history enforces hard caps on command count (`MAX_UNDO_COMMANDS`) and accumulated
//!    cell delta count (`MAX_UNDO_CELL_DELTAS`), evicting oldest commands from the history queue.
//! 4. **No Allocations During Steady-State Queries:** Command execution updates pre-allocated change sets.

use std::collections::VecDeque;

use sim_map::map_format::{
    MapCellSpec, MapDocument, MapEntityId, MapMetadata, MapResource, MapStartLocation,
    MapWatchtower,
};
use sim_map::validation::CellRect;

/// Maximum number of undo commands preserved in history before evicting oldest entries.
pub const MAX_UNDO_COMMANDS: usize = 256;

/// Maximum number of cumulative cell deltas preserved in undo history before evicting oldest entries.
///
/// Bounded to approximately 40 MB worst-case memory footprint.
pub const MAX_UNDO_CELL_DELTAS: usize = 1_048_576;

/// Single cell mutation recording pre-edit and post-edit states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellDelta {
    /// Column coordinate `0..width`.
    pub x: u32,
    /// Row coordinate `0..height`.
    pub y: u32,
    /// Cell specification before modification.
    pub before: MapCellSpec,
    /// Cell specification after modification.
    pub after: MapCellSpec,
}

/// Unified map entity representation across resources, watchtowers, and start locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapEntity {
    /// Mineral field or gas geyser resource deposit.
    Resource(MapResource),
    /// Vision and control watchtower.
    Watchtower(MapWatchtower),
    /// Player starting base location with town hall footprint.
    StartLocation(MapStartLocation),
}

impl MapEntity {
    /// Returns the unique identifier of the entity.
    #[must_use]
    pub fn id(&self) -> MapEntityId {
        match self {
            Self::Resource(r) => r.id,
            Self::Watchtower(w) => w.id,
            Self::StartLocation(s) => s.id,
        }
    }

    /// Returns the grid cell coordinates `(x, y)` of the entity's anchor.
    #[must_use]
    pub fn cell(&self) -> (u32, u32) {
        match self {
            Self::Resource(r) => r.cell,
            Self::Watchtower(w) => w.cell,
            Self::StartLocation(s) => s.cell,
        }
    }
}

/// Closed enum of all possible editor mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditCommand {
    /// Modifies terrain, cliff level, or authorable flags on a set of cells.
    PaintCells {
        /// Human-readable label for UI undo/redo menus (e.g. "Paint Dirt", "Ramp").
        label: &'static str,
        /// List of cell transitions.
        deltas: Vec<CellDelta>,
    },
    /// Spawns new entities into the document.
    AddEntities {
        /// Entities to insert.
        entities: Vec<MapEntity>,
    },
    /// Removes existing entities from the document.
    RemoveEntities {
        /// Full entities being removed (stored to allow exact restoration on undo).
        entities: Vec<MapEntity>,
    },
    /// Updates properties or position of a single entity.
    ModifyEntity {
        /// Entity state prior to edit.
        before: MapEntity,
        /// Entity state after edit.
        after: MapEntity,
    },
    /// Updates properties or positions of multiple entities in one atomic command.
    ModifyEntities {
        /// List of `(before, after)` entity state pairs.
        modifications: Vec<(MapEntity, MapEntity)>,
    },
    /// Updates document title, author, description, or theme.
    SetMetadata {
        /// Metadata state prior to edit.
        before: MapMetadata,
        /// Metadata state after edit.
        after: MapMetadata,
    },
    /// Applies several commands as one undoable edit, reverting them in reverse order.
    Batch {
        /// Commands in application order.
        commands: Vec<EditCommand>,
    },
}

/// Bounding rectangle and subsystem flags describing document mutations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChangeSet {
    /// Bounding box of modified cells, if any.
    pub cells: Option<CellRect>,
    /// Whether any entity was added, removed, or modified.
    pub entities_changed: bool,
    /// Whether document metadata was modified.
    pub metadata_changed: bool,
}

impl ChangeSet {
    /// Creates an empty change set.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns true if no changes are recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_none() && !self.entities_changed && !self.metadata_changed
    }

    /// Expands the bounding cell rectangle to include the given cell coordinates.
    pub fn include_cell(&mut self, x: u32, y: u32) {
        self.cells = Some(match self.cells {
            Some(rect) => CellRect {
                min: (rect.min.0.min(x), rect.min.1.min(y)),
                max_inclusive: (rect.max_inclusive.0.max(x), rect.max_inclusive.1.max(y)),
            },
            None => CellRect {
                min: (x, y),
                max_inclusive: (x, y),
            },
        });
    }

    /// Expands the bounding cell rectangle to encompass another rectangle.
    pub fn union_rect(&mut self, rect: CellRect) {
        self.cells = Some(match self.cells {
            Some(prev) => CellRect {
                min: (prev.min.0.min(rect.min.0), prev.min.1.min(rect.min.1)),
                max_inclusive: (
                    prev.max_inclusive.0.max(rect.max_inclusive.0),
                    prev.max_inclusive.1.max(rect.max_inclusive.1),
                ),
            },
            None => rect,
        });
    }
}

fn add_entity_to_doc(doc: &mut MapDocument, entity: &MapEntity) {
    match entity {
        MapEntity::Resource(r) => {
            doc.resources.retain(|e| e.id != r.id);
            doc.resources.push(r.clone());
            doc.resources.sort_by_key(|e| e.id);
        }
        MapEntity::Watchtower(w) => {
            doc.watchtowers.retain(|e| e.id != w.id);
            doc.watchtowers.push(w.clone());
            doc.watchtowers.sort_by_key(|e| e.id);
        }
        MapEntity::StartLocation(s) => {
            doc.start_locations.retain(|e| e.id != s.id);
            doc.start_locations.push(s.clone());
            doc.start_locations.sort_by_key(|e| e.id);
        }
    }
}

fn remove_entity_from_doc(doc: &mut MapDocument, entity: &MapEntity) {
    let id = entity.id();
    match entity {
        MapEntity::Resource(_) => {
            doc.resources.retain(|e| e.id != id);
        }
        MapEntity::Watchtower(_) => {
            doc.watchtowers.retain(|e| e.id != id);
        }
        MapEntity::StartLocation(_) => {
            doc.start_locations.retain(|e| e.id != id);
        }
    }
}

impl EditCommand {
    /// Applies the forward mutation to `doc` and records the modified bounds in `cs`.
    pub fn apply(&self, doc: &mut MapDocument, cs: &mut ChangeSet) {
        match self {
            Self::PaintCells { deltas, .. } => {
                for delta in deltas {
                    if let Some(cell) = doc.cell_mut(delta.x, delta.y) {
                        *cell = delta.after.clone();
                        cs.include_cell(delta.x, delta.y);
                    }
                }
            }
            Self::AddEntities { entities } => {
                for entity in entities {
                    add_entity_to_doc(doc, entity);
                }
                cs.entities_changed = true;
            }
            Self::RemoveEntities { entities } => {
                for entity in entities {
                    remove_entity_from_doc(doc, entity);
                }
                cs.entities_changed = true;
            }
            Self::ModifyEntity { before, after } => {
                remove_entity_from_doc(doc, before);
                add_entity_to_doc(doc, after);
                cs.entities_changed = true;
            }
            Self::ModifyEntities { modifications } => {
                for (before, after) in modifications {
                    remove_entity_from_doc(doc, before);
                    add_entity_to_doc(doc, after);
                }
                cs.entities_changed = true;
            }
            Self::SetMetadata { after, .. } => {
                doc.metadata = after.clone();
                cs.metadata_changed = true;
            }
            Self::Batch { commands } => {
                for command in commands {
                    command.apply(doc, cs);
                }
            }
        }
    }

    /// Reverts the mutation from `doc` and records the modified bounds in `cs`.
    pub fn revert(&self, doc: &mut MapDocument, cs: &mut ChangeSet) {
        match self {
            Self::PaintCells { deltas, .. } => {
                for delta in deltas {
                    if let Some(cell) = doc.cell_mut(delta.x, delta.y) {
                        *cell = delta.before.clone();
                        cs.include_cell(delta.x, delta.y);
                    }
                }
            }
            Self::AddEntities { entities } => {
                for entity in entities {
                    remove_entity_from_doc(doc, entity);
                }
                cs.entities_changed = true;
            }
            Self::RemoveEntities { entities } => {
                for entity in entities {
                    add_entity_to_doc(doc, entity);
                }
                cs.entities_changed = true;
            }
            Self::ModifyEntity { before, after } => {
                remove_entity_from_doc(doc, after);
                add_entity_to_doc(doc, before);
                cs.entities_changed = true;
            }
            Self::ModifyEntities { modifications } => {
                for (before, after) in modifications {
                    remove_entity_from_doc(doc, after);
                    add_entity_to_doc(doc, before);
                }
                cs.entities_changed = true;
            }
            Self::SetMetadata { before, .. } => {
                doc.metadata = before.clone();
                cs.metadata_changed = true;
            }
            Self::Batch { commands } => {
                for command in commands.iter().rev() {
                    command.revert(doc, cs);
                }
            }
        }
    }

    /// Returns the number of cell deltas contained in this command.
    #[must_use]
    pub fn cell_delta_count(&self) -> usize {
        match self {
            Self::PaintCells { deltas, .. } => deltas.len(),
            Self::AddEntities { .. }
            | Self::RemoveEntities { .. }
            | Self::ModifyEntity { .. }
            | Self::ModifyEntities { .. }
            | Self::SetMetadata { .. } => 0,
            Self::Batch { commands } => commands.iter().map(Self::cell_delta_count).sum(),
        }
    }

    /// Extends this command with `next` if `next` continues the same edit from where it stopped.
    ///
    /// Returns `true` if `next` was absorbed. Reverting the result restores the state from
    /// before this command, as if the intermediate steps never happened.
    fn absorb(&mut self, next: &Self) -> bool {
        match (self, next) {
            (
                Self::SetMetadata { after, .. },
                Self::SetMetadata {
                    before: next_before,
                    after: next_after,
                },
            ) if after == next_before => {
                after.clone_from(next_after);
                true
            }
            (
                Self::ModifyEntity { after, .. },
                Self::ModifyEntity {
                    before: next_before,
                    after: next_after,
                },
            ) if after == next_before => {
                after.clone_from(next_after);
                true
            }
            _ => false,
        }
    }
}

/// Undo/redo stack managing bounded command history.
#[derive(bevy::prelude::Resource, Debug, Clone)]
pub struct History {
    undo: VecDeque<EditCommand>,
    redo: Vec<EditCommand>,
    total_deltas: usize,
    max_commands: usize,
    max_deltas: usize,
    /// Whether the newest undo entry may still absorb the next step of a continuous edit.
    merge_open: bool,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    /// Creates a new history stack with default bounds (`MAX_UNDO_COMMANDS`, `MAX_UNDO_CELL_DELTAS`).
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(MAX_UNDO_COMMANDS, MAX_UNDO_CELL_DELTAS)
    }

    /// Creates a new history stack with custom capacity limits.
    #[must_use]
    pub fn with_limits(max_commands: usize, max_deltas: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            total_deltas: 0,
            max_commands,
            max_deltas,
            merge_open: false,
        }
    }

    /// Clears redo stack, appends an already applied command to undo history, and evicts oldest commands exceeding limits.
    pub fn push_applied(&mut self, cmd: EditCommand) {
        self.merge_open = false;
        self.redo.clear();
        let deltas = cmd.cell_delta_count();
        self.total_deltas += deltas;
        self.undo.push_back(cmd);

        self.enforce_limits();
    }

    /// Executes `cmd` on `doc`, updates `cs`, and registers it into the undo history.
    pub fn execute(&mut self, cmd: EditCommand, doc: &mut MapDocument, cs: &mut ChangeSet) {
        cmd.apply(doc, cs);
        self.push_applied(cmd);
    }

    /// Executes one step of a continuous edit such as typing or dragging a slider.
    ///
    /// Consecutive steps that continue each other share one undo entry until [`Self::seal_merge`]
    /// or any other history operation. A step that does not continue the newest entry starts a
    /// new one.
    pub fn execute_merged(&mut self, cmd: EditCommand, doc: &mut MapDocument, cs: &mut ChangeSet) {
        cmd.apply(doc, cs);
        if self.merge_open {
            if let Some(newest) = self.undo.back_mut() {
                if newest.absorb(&cmd) {
                    self.redo.clear();
                    return;
                }
            }
        }
        self.push_applied(cmd);
        self.merge_open = true;
    }

    /// Ends the current continuous edit, so the next step starts a new undo entry.
    ///
    /// Called when a text field loses focus or a slider drag stops.
    pub fn seal_merge(&mut self) {
        self.merge_open = false;
    }

    /// Reverts the most recent command on the undo stack, pushing it onto redo.
    ///
    /// Returns `true` if a command was undone, `false` if undo was empty.
    pub fn undo(&mut self, doc: &mut MapDocument, cs: &mut ChangeSet) -> bool {
        self.merge_open = false;
        if let Some(cmd) = self.undo.pop_back() {
            self.total_deltas = self.total_deltas.saturating_sub(cmd.cell_delta_count());
            cmd.revert(doc, cs);
            self.redo.push(cmd);
            true
        } else {
            false
        }
    }

    /// Re-applies the most recent command on the redo stack, pushing it onto undo.
    ///
    /// Returns `true` if a command was redone, `false` if redo was empty.
    pub fn redo(&mut self, doc: &mut MapDocument, cs: &mut ChangeSet) -> bool {
        self.merge_open = false;
        if let Some(cmd) = self.redo.pop() {
            cmd.apply(doc, cs);
            self.total_deltas += cmd.cell_delta_count();
            self.undo.push_back(cmd);
            self.enforce_limits();
            true
        } else {
            false
        }
    }

    /// Clears both undo and redo stacks, resetting delta counter.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.total_deltas = 0;
        self.merge_open = false;
    }

    fn enforce_limits(&mut self) {
        // PERFORMANCE RATIONALE:
        // Why: Unbounded history could exhaust physical RAM during long painting sessions.
        // What: Oldest commands are evicted from the front of the VecDeque when count exceeds `max_commands`
        // or accumulated cell deltas exceed `max_deltas`.
        // Trade-off: Users cannot undo infinitely far back, in exchange for strict O(1) memory bounds.
        while self.undo.len() > self.max_commands
            || (self.total_deltas > self.max_deltas && !self.undo.is_empty())
        {
            if let Some(evicted) = self.undo.pop_front() {
                self.total_deltas = self.total_deltas.saturating_sub(evicted.cell_delta_count());
            } else {
                break;
            }
        }
    }

    /// Returns the number of commands currently on the undo stack.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Returns the number of commands currently on the redo stack.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Returns true if undo is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Returns true if redo is available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Returns the total accumulated cell delta count in the undo stack.
    #[must_use]
    pub fn total_deltas(&self) -> usize {
        self.total_deltas
    }
}
