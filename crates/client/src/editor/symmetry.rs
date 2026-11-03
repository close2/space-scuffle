//! Map symmetry transformations for cells, ramps, rectangles, and entities.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §6.4 and `AGENTS.md`,
//! this module provides mathematical mirror and 180° rotation operations for all authorable map elements.
//!
//! # Invariants Maintained
//! 1. **Involution (Self-Inverse):** For any symmetry mode `M` and coordinate `C`, `mirror(mirror(C)) == C`.
//! 2. **Ramp Orientation Preservation:** Ramps reflected across an axis invert their climbing direction
//!    along that axis (e.g. `MirrorX` maps East ↔ West; `MirrorY` maps North ↔ South).
//! 3. **Partner Slot Mapping:** Starting location player slots map `s ↔ s ^ 1` (slot 0 ↔ 1, 2 ↔ 3, etc.).
//! 4. **Footprint Anchor Invariance:** Mirrored multi-cell rectangles use the minimum corner `(min_x, min_y)`
//!    of the reflected bounding box as their anchor.

use sim_map::grid::RampDir;
use sim_map::map_format::{MapEntityId, MapResource, MapStartLocation, MapWatchtower};
use sim_map::validation::CellRect;

use crate::editor::commands::MapEntity;

/// Symmetry modes supported by the map editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SymmetryMode {
    /// No symmetry mirroring.
    #[default]
    None,
    /// Horizontal reflection across vertical center axis (`x ↔ W - 1 - x`).
    MirrorX,
    /// Vertical reflection across horizontal center axis (`y ↔ H - 1 - y`).
    MirrorY,
    /// Point reflection / 180° rotation about map center (`x ↔ W - 1 - x, y ↔ H - 1 - y`).
    Rotate180,
}

impl SymmetryMode {
    /// Mirrors a single cell coordinate `(x, y)` given map dimensions `(width, height)`.
    #[must_use]
    pub fn mirror_cell(&self, cell: (u32, u32), width: u32, height: u32) -> (u32, u32) {
        let (x, y) = cell;
        match self {
            Self::None => (x, y),
            Self::MirrorX => (width.saturating_sub(1).saturating_sub(x), y),
            Self::MirrorY => (x, height.saturating_sub(1).saturating_sub(y)),
            Self::Rotate180 => (
                width.saturating_sub(1).saturating_sub(x),
                height.saturating_sub(1).saturating_sub(y),
            ),
        }
    }

    /// Mirrors an ascending ramp direction.
    #[must_use]
    pub fn mirror_ramp(&self, dir: RampDir) -> RampDir {
        match self {
            Self::None => dir,
            Self::MirrorX => match dir {
                RampDir::East => RampDir::West,
                RampDir::West => RampDir::East,
                RampDir::North => RampDir::North,
                RampDir::South => RampDir::South,
                RampDir::NorthEast => RampDir::NorthWest,
                RampDir::NorthWest => RampDir::NorthEast,
                RampDir::SouthEast => RampDir::SouthWest,
                RampDir::SouthWest => RampDir::SouthEast,
            },
            Self::MirrorY => match dir {
                RampDir::North => RampDir::South,
                RampDir::South => RampDir::North,
                RampDir::East => RampDir::East,
                RampDir::West => RampDir::West,
                RampDir::NorthEast => RampDir::SouthEast,
                RampDir::NorthWest => RampDir::SouthWest,
                RampDir::SouthEast => RampDir::NorthEast,
                RampDir::SouthWest => RampDir::NorthWest,
            },
            Self::Rotate180 => match dir {
                RampDir::North => RampDir::South,
                RampDir::South => RampDir::North,
                RampDir::East => RampDir::West,
                RampDir::West => RampDir::East,
                RampDir::NorthEast => RampDir::SouthWest,
                RampDir::NorthWest => RampDir::SouthEast,
                RampDir::SouthEast => RampDir::NorthWest,
                RampDir::SouthWest => RampDir::NorthEast,
            },
        }
    }

    /// Mirrors an axis-aligned bounding box of cells.
    #[must_use]
    pub fn mirror_rect(&self, rect: CellRect, width: u32, height: u32) -> CellRect {
        match self {
            Self::None => rect,
            Self::MirrorX => {
                let max_x = width.saturating_sub(1).saturating_sub(rect.min.0);
                let min_x = width.saturating_sub(1).saturating_sub(rect.max_inclusive.0);
                CellRect {
                    min: (min_x, rect.min.1),
                    max_inclusive: (max_x, rect.max_inclusive.1),
                }
            }
            Self::MirrorY => {
                let max_y = height.saturating_sub(1).saturating_sub(rect.min.1);
                let min_y = height
                    .saturating_sub(1)
                    .saturating_sub(rect.max_inclusive.1);
                CellRect {
                    min: (rect.min.0, min_y),
                    max_inclusive: (rect.max_inclusive.0, max_y),
                }
            }
            Self::Rotate180 => {
                let max_x = width.saturating_sub(1).saturating_sub(rect.min.0);
                let min_x = width.saturating_sub(1).saturating_sub(rect.max_inclusive.0);
                let max_y = height.saturating_sub(1).saturating_sub(rect.min.1);
                let min_y = height
                    .saturating_sub(1)
                    .saturating_sub(rect.max_inclusive.1);
                CellRect {
                    min: (min_x, min_y),
                    max_inclusive: (max_x, max_y),
                }
            }
        }
    }

    /// Mirrors a map entity to its symmetric partner, assigning `new_id`.
    ///
    /// For start locations, `player_slot` is mapped to partner slot `s ^ 1`.
    /// For multi-cell footprints, the new anchor is the min corner of the mirrored bounding box.
    #[must_use]
    pub fn mirror_entity(
        &self,
        entity: &MapEntity,
        width: u32,
        height: u32,
        new_id: MapEntityId,
    ) -> MapEntity {
        match entity {
            MapEntity::Resource(r) => MapEntity::Resource(MapResource {
                id: new_id,
                kind: r.kind,
                cell: self.mirror_cell(r.cell, width, height),
                amount: r.amount,
            }),
            MapEntity::Watchtower(w) => MapEntity::Watchtower(MapWatchtower {
                id: new_id,
                cell: self.mirror_cell(w.cell, width, height),
                capture_radius_tenths: w.capture_radius_tenths,
                reveal_radius_tenths: w.reveal_radius_tenths,
            }),
            MapEntity::StartLocation(s) => {
                let rect = CellRect {
                    min: s.cell,
                    max_inclusive: (
                        s.cell.0 + u32::from(s.footprint.0).saturating_sub(1),
                        s.cell.1 + u32::from(s.footprint.1).saturating_sub(1),
                    ),
                };
                let mirrored_rect = self.mirror_rect(rect, width, height);
                MapEntity::StartLocation(MapStartLocation {
                    id: new_id,
                    player_slot: s.player_slot ^ 1,
                    cell: mirrored_rect.min,
                    footprint: s.footprint,
                })
            }
        }
    }
}
