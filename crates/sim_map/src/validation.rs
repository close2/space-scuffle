//! Map validation engine for `Space Scuffle`.
//!
//! # Responsibility
//! This module implements strict, deterministic validation of editable map documents
//! ([`MapDocument`]). Every defect is categorized into strongly-typed errors with
//! precise cell bounding boxes ([`CellRect`]) for visual in-editor highlighting.
//!
//! # Invariants Maintained
//! - Deterministic check order: errors are checked in fixed numerical order (1..=22)
//!   and sorted deterministically by (check order, y, x).
//! - All defects are collected (not first-only), allowing map authors to see all issues at once.
//! - Pathfinding connectivity between bases is only evaluated if fundamental terrain
//!   and start location invariants are satisfied.
//! - Zero heap allocations during hot-path coordinate lookups; no non-deterministic collections.

use std::collections::{BTreeMap, BTreeSet};

use sim_types::{SimScalar, SimVec2};

use crate::error::PlacementFailureReason;
use crate::grid::{CellFlags, CliffLevel, RampDir, TerrainKind, MAX_RAMP_RUN_CELLS};
use crate::map_format::{
    MapDocument, MapEntityId, MapStartLocation, MAX_MAP_DIMENSION, MIN_MAP_DIMENSION,
    WATCHTOWER_ENTITY_ID_BASE,
};
use crate::pathfinding::{PathRequest, Pathfinder, UnitClearance};
use crate::vision::MAX_SIGHT_RADIUS_CELLS;

/// Minimum number of player start locations required for a valid playable map.
pub const MIN_START_LOCATIONS: usize = 2;

/// Maximum number of player slots supported by the simulation architecture.
pub const MAX_PLAYER_SLOTS: u8 = 8;

/// Minimum Chebyshev distance between start location footprint centers.
pub const MIN_START_SEPARATION_CELLS: u32 = 16;

/// Maximum cell radius within which natural resources are associated with a start location.
pub const RESOURCE_ASSOCIATION_RADIUS_CELLS: u32 = 12;

/// Recommended minimum width (perpendicular to direction) for navigable ramps.
pub const MIN_RECOMMENDED_RAMP_WIDTH: u8 = 2;

/// Unit clearance size required for base-to-base pathfinding validation.
pub const VALIDATION_PATH_CLEARANCE: UnitClearance = UnitClearance::Medium;

/// Axis-aligned bounding box of grid cells on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRect {
    /// Minimum corner `(min_x, min_y)`.
    pub min: (u32, u32),
    /// Maximum corner inclusive `(max_x, max_y)`.
    pub max_inclusive: (u32, u32),
}

/// Comprehensive report of all validation errors and warnings for a map document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapValidationReport {
    /// Critical defects preventing playability.
    pub errors: Vec<MapValidationError>,
    /// Design warnings that do not block simulation execution.
    pub warnings: Vec<MapValidationWarning>,
}

impl MapValidationReport {
    /// Returns `true` if the map contains zero critical validation errors.
    #[must_use]
    pub fn is_playable(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Strongly-typed map validation errors that prevent a map from being simulated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MapValidationError {
    /// Document dimensions are unsupported or disagree with the stored cell grid.
    ///
    /// Reported alone: no other check can index a grid whose shape is unknown.
    #[error("map grid has {cells} cells, which does not fit dimensions {width}x{height}")]
    CellGridMismatch {
        /// Declared width in cells.
        width: u32,
        /// Declared height in cells.
        height: u32,
        /// Number of cells actually stored.
        cells: usize,
    },

    /// Abyss terrain marked as buildable or ground pathable.
    #[error("abyss cell at ({x}, {y}) is marked traversable or buildable")]
    AbyssCellTraversable {
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell.
        y: u32,
    },

    /// Ramp placed on an abyss cell.
    #[error("ramp placed on abyss cell at ({x}, {y})")]
    RampOnAbyss {
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell.
        y: u32,
    },

    /// Ramp placed at maximum cliff level.
    #[error("ramp placed at maximum cliff level (3) at ({x}, {y})")]
    RampAtTopLevel {
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell.
        y: u32,
    },

    /// Watchtower identifier cannot be mapped into the simulation entity ID range.
    #[error("watchtower {id:?} identifier is too large for the simulation entity range")]
    EntityIdOutOfRange {
        /// Entity ID of watchtower.
        id: MapEntityId,
    },

    /// Ramp cell lacking the ground pathable flag.
    #[error("ramp cell at ({x}, {y}) is not marked ground pathable")]
    RampNotPathable {
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell.
        y: u32,
    },

    /// Ramp cell erroneously marked as buildable.
    #[error("ramp cell at ({x}, {y}) is marked buildable")]
    RampBuildable {
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell.
        y: u32,
    },

    /// Contiguous ramp run exceeds the maximum supported length.
    #[error(
        "ramp run at ({x}, {y}) exceeds maximum run length of {MAX_RAMP_RUN_CELLS} (found {len})"
    )]
    RampRunTooLong {
        /// X coordinate of run's first cell.
        x: u32,
        /// Y coordinate of run's first cell.
        y: u32,
        /// Contiguous run length encountered.
        len: u8,
    },

    /// Upper landing of ramp does not connect to non-ramp terrain at level + 1.
    #[error(
        "ramp upper landing at ({x}, {y}) does not connect to valid non-ramp terrain at level + 1"
    )]
    RampUpperLandingMismatch {
        /// X coordinate of run's last cell.
        x: u32,
        /// Y coordinate of run's last cell.
        y: u32,
    },

    /// Lower landing of ramp does not connect to non-ramp terrain at low level.
    #[error(
        "ramp lower landing at ({x}, {y}) does not connect to valid non-ramp terrain at base level"
    )]
    RampLowerLandingMismatch {
        /// X coordinate of run's first cell.
        x: u32,
        /// Y coordinate of run's first cell.
        y: u32,
    },

    /// Ramp cells in the same contiguous run have differing elevation levels.
    #[error("ramp run cells at ({x}, {y}) have differing cliff elevation levels")]
    RampRunLevelMismatch {
        /// X coordinate of mismatching cell.
        x: u32,
        /// Y coordinate of mismatching cell.
        y: u32,
    },

    /// Map has fewer start locations than the required minimum.
    #[error("map has {count} start locations, minimum required is {MIN_START_LOCATIONS}")]
    TooFewStartLocations {
        /// Number of start locations found.
        count: usize,
    },

    /// Player slot index is out of supported range `0..MAX_PLAYER_SLOTS`.
    #[error(
        "player slot {slot} for start location {id:?} exceeds maximum allowed ({MAX_PLAYER_SLOTS})"
    )]
    PlayerSlotOutOfRange {
        /// Entity ID of start location.
        id: MapEntityId,
        /// Slot index found.
        slot: u8,
    },

    /// Duplicate player slot assigned to multiple start locations.
    #[error("duplicate player slot {slot} assigned to start location {id:?}")]
    DuplicatePlayerSlot {
        /// Entity ID of start location.
        id: MapEntityId,
        /// Duplicate slot index.
        slot: u8,
    },

    /// Start location footprint is blocked by unbuildable terrain, ramps, or elevation splits.
    #[error("start location footprint {id:?} is obstructed: {reason}")]
    StartFootprintBlocked {
        /// Entity ID of start location.
        id: MapEntityId,
        /// Underlying placement failure reason.
        reason: PlacementFailureReason,
    },

    /// Two start locations are placed closer than the required minimum separation distance.
    #[error("start locations {a:?} and {b:?} are too close ({distance} cells apart, minimum is {MIN_START_SEPARATION_CELLS})")]
    StartLocationsTooClose {
        /// Entity ID of first start location.
        a: MapEntityId,
        /// Entity ID of second start location.
        b: MapEntityId,
        /// Measured Chebyshev distance.
        distance: u32,
    },

    /// Duplicate entity ID shared by multiple resources, watchtowers, or start locations.
    #[error("duplicate entity id {id:?} discovered")]
    DuplicateEntityId {
        /// Duplicate entity ID.
        id: MapEntityId,
    },

    /// Entity position or start footprint extends outside map dimensions.
    #[error("entity {id:?} position ({x}, {y}) extends outside map bounds")]
    EntityOutOfBounds {
        /// Entity ID.
        id: MapEntityId,
        /// Target X coordinate.
        x: u32,
        /// Target Y coordinate.
        y: u32,
    },

    /// Resource node placed on abyss, ramp, or non-pathable terrain.
    #[error("resource {id:?} is placed on a blocked, abyss, or ramp cell")]
    ResourceOnBlockedCell {
        /// Entity ID of resource node.
        id: MapEntityId,
    },

    /// Resource node placed inside a player start footprint.
    #[error("resource {id:?} overlaps start location {start:?} footprint")]
    ResourceInsideStartFootprint {
        /// Entity ID of resource node.
        id: MapEntityId,
        /// Entity ID of start location.
        start: MapEntityId,
    },

    /// Multiple resources or watchtowers occupy the exact same grid cell.
    #[error("entities {a:?} and {b:?} occupy the exact same cell")]
    EntitiesOverlap {
        /// First entity ID.
        a: MapEntityId,
        /// Second entity ID.
        b: MapEntityId,
    },

    /// Resource node has zero remaining harvestable units.
    #[error("resource {id:?} has zero remaining harvestable amount")]
    ZeroResourceAmount {
        /// Entity ID of resource.
        id: MapEntityId,
    },

    /// Watchtower has zero capture radius or zero reveal radius.
    #[error("watchtower {id:?} has zero capture or reveal radius")]
    ZeroWatchtowerRadius {
        /// Entity ID of watchtower.
        id: MapEntityId,
    },

    /// Watchtower reveal radius is larger than vision can honour.
    #[error("watchtower {id:?} reveal radius exceeds {MAX_SIGHT_RADIUS_CELLS} cells")]
    WatchtowerRevealTooLarge {
        /// Entity ID of watchtower.
        id: MapEntityId,
    },

    /// Pathfinding between start locations failed (bases are disconnected).
    #[error("pathfinding between start locations {a:?} and {b:?} failed; bases are unreachable")]
    StartLocationsUnreachable {
        /// First start location entity ID.
        a: MapEntityId,
        /// Second start location entity ID.
        b: MapEntityId,
    },
}

impl MapValidationError {
    /// Returns the cell bounding box associated with this defect for editor highlighting.
    #[must_use]
    pub fn location(&self) -> CellRect {
        match self {
            Self::AbyssCellTraversable { x, y }
            | Self::RampOnAbyss { x, y }
            | Self::RampAtTopLevel { x, y }
            | Self::RampNotPathable { x, y }
            | Self::RampBuildable { x, y }
            | Self::RampRunTooLong { x, y, .. }
            | Self::RampUpperLandingMismatch { x, y }
            | Self::RampLowerLandingMismatch { x, y }
            | Self::RampRunLevelMismatch { x, y }
            | Self::EntityOutOfBounds { x, y, .. } => CellRect {
                min: (*x, *y),
                max_inclusive: (*x, *y),
            },
            Self::TooFewStartLocations { .. } => CellRect {
                min: (0, 0),
                max_inclusive: (u32::MAX, u32::MAX),
            },
            _ => CellRect {
                min: (0, 0),
                max_inclusive: (0, 0),
            },
        }
    }
}

/// Advisory design warnings for map authors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MapValidationWarning {
    /// Ramp run width is less than the recommended minimum.
    #[error("narrow ramp at ({x}, {y}) with width {width} (recommended minimum is {MIN_RECOMMENDED_RAMP_WIDTH})")]
    NarrowRamp {
        /// X coordinate of ramp run's first cell.
        x: u32,
        /// Y coordinate of ramp run's first cell.
        y: u32,
        /// Measured ramp run width.
        width: u8,
    },

    /// Diagonal pinch in terrain height (2x2 diagonal check).
    #[error("diagonal pinch detected at 2x2 block starting at ({x}, {y})")]
    DiagonalPinch {
        /// Top-left X coordinate of 2x2 block.
        x: u32,
        /// Top-left Y coordinate of 2x2 block.
        y: u32,
    },

    /// Asymmetric resource distribution between player start bases.
    #[error("resource imbalance: player slot {slot} has {count} associated resources, while maximum is {max_count}")]
    ResourceImbalance {
        /// Player slot index.
        slot: u8,
        /// Resources associated with this slot.
        count: usize,
        /// Maximum resources associated with any slot.
        max_count: usize,
    },

    /// Resource node is further than `RESOURCE_ASSOCIATION_RADIUS_CELLS` from any player start base.
    #[error("resource {id:?} is not associated with any player base within {RESOURCE_ASSOCIATION_RADIUS_CELLS} cells")]
    UnassignedResource {
        /// Entity ID of unassigned resource.
        id: MapEntityId,
    },
}

impl MapValidationWarning {
    /// Returns the cell bounding box associated with this warning for editor highlighting.
    #[must_use]
    pub fn location(&self) -> CellRect {
        match self {
            Self::NarrowRamp { x, y, .. } => CellRect {
                min: (*x, *y),
                max_inclusive: (*x, *y),
            },
            Self::DiagonalPinch { x, y } => CellRect {
                min: (*x, *y),
                max_inclusive: (*x + 1, *y + 1),
            },
            Self::ResourceImbalance { .. } | Self::UnassignedResource { .. } => CellRect {
                min: (0, 0),
                max_inclusive: (0, 0),
            },
        }
    }
}

#[derive(Debug, Clone)]
struct RampRunInfo {
    first: (u32, u32),
    last: (u32, u32),
    dir: RampDir,
    cells: Vec<(u32, u32)>,
}

/// Returns the grid step of a cardinal ramp, or `None` for a legacy diagonal direction.
fn ramp_axis(dir: RampDir) -> Option<(i32, i32)> {
    match dir {
        RampDir::North => Some((0, 1)),
        RampDir::South => Some((0, -1)),
        RampDir::East => Some((1, 0)),
        RampDir::West => Some((-1, 0)),
        _ => None,
    }
}

/// Tests whether a start footprint fits on the grid, widening so huge coordinates cannot wrap.
fn footprint_in_bounds(start: &MapStartLocation, width: u32, height: u32) -> bool {
    u64::from(start.cell.0) + u64::from(start.footprint.0) <= u64::from(width)
        && u64::from(start.cell.1) + u64::from(start.footprint.1) <= u64::from(height)
}

/// Evaluates all validation rules on the given map document, returning a deterministic report.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "numbered checks in documented order; splitting is plan step 9.2"
)]
pub fn validate_map(doc: &MapDocument) -> MapValidationReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let width = doc.width;
    let height = doc.height;

    // Check 0: CellGridMismatch. Every later check indexes `cells` by (y * width + x), so an
    // untrusted document must prove its shape before any of them may run.
    let supported = (MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&width)
        && (MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&height);
    if !supported || doc.cells.len() as u64 != u64::from(width) * u64::from(height) {
        errors.push(MapValidationError::CellGridMismatch {
            width,
            height,
            cells: doc.cells.len(),
        });
        return MapValidationReport { errors, warnings };
    }

    // Check 1: AbyssCellTraversable
    for y in 0..height {
        for x in 0..width {
            let spec = &doc.cells[(y * width + x) as usize];
            if spec.terrain == TerrainKind::Abyss
                && (spec.flags.contains(CellFlags::BUILDABLE)
                    || spec.flags.contains(CellFlags::GROUND_PATHABLE))
            {
                errors.push(MapValidationError::AbyssCellTraversable { x, y });
            }
        }
    }

    // Check 2: RampOnAbyss
    for y in 0..height {
        for x in 0..width {
            let spec = &doc.cells[(y * width + x) as usize];
            if spec.ramp.is_some() && spec.terrain == TerrainKind::Abyss {
                errors.push(MapValidationError::RampOnAbyss { x, y });
            }
        }
    }

    // Check 3: RampAtTopLevel
    for y in 0..height {
        for x in 0..width {
            let spec = &doc.cells[(y * width + x) as usize];
            if spec.ramp.is_some() && spec.level == CliffLevel::MAX {
                errors.push(MapValidationError::RampAtTopLevel { x, y });
            }
        }
    }

    // Check 4: RampNotPathable
    for y in 0..height {
        for x in 0..width {
            let spec = &doc.cells[(y * width + x) as usize];
            if spec.ramp.is_some() && !spec.flags.contains(CellFlags::GROUND_PATHABLE) {
                errors.push(MapValidationError::RampNotPathable { x, y });
            }
        }
    }

    // Check 5: RampBuildable
    for y in 0..height {
        for x in 0..width {
            let spec = &doc.cells[(y * width + x) as usize];
            if spec.ramp.is_some() && spec.flags.contains(CellFlags::BUILDABLE) {
                errors.push(MapValidationError::RampBuildable { x, y });
            }
        }
    }

    let mut ramp_runs: Vec<RampRunInfo> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let spec = &doc.cells[(y * width + x) as usize];
            let Some(dir) = spec.ramp else { continue };
            let Some((dx, dy)) = ramp_axis(dir) else {
                continue;
            };

            let previous = doc
                .offset_cell((x, y), (-dx, -dy))
                .and_then(|cell| doc.cell(cell.0, cell.1));
            let is_first = previous.and_then(|c| c.ramp) != Some(dir);

            if is_first {
                let mut run_cells = Vec::new();
                let mut next = Some((x, y));
                while let Some(cell) = next {
                    if doc.cell(cell.0, cell.1).and_then(|c| c.ramp) != Some(dir) {
                        break;
                    }
                    run_cells.push(cell);
                    next = doc.offset_cell(cell, (dx, dy));
                }
                let Some(&last) = run_cells.last() else {
                    continue;
                };
                ramp_runs.push(RampRunInfo {
                    first: (x, y),
                    last,
                    dir,
                    cells: run_cells,
                });
            }
        }
    }

    // Check 6: RampRunTooLong
    for run in &ramp_runs {
        if run.cells.len() > MAX_RAMP_RUN_CELLS as usize {
            errors.push(MapValidationError::RampRunTooLong {
                x: run.first.0,
                y: run.first.1,
                len: u8::try_from(run.cells.len()).unwrap_or(u8::MAX),
            });
        }
    }

    // Check 7: RampUpperLandingMismatch
    for run in &ramp_runs {
        let Some((dx, dy)) = ramp_axis(run.dir) else {
            continue;
        };
        let Some(first_cell) = doc.cell(run.first.0, run.first.1) else {
            continue;
        };
        let low_level = first_cell.level;

        let upper = doc
            .offset_cell(run.last, (dx, dy))
            .and_then(|cell| doc.cell(cell.0, cell.1));
        let mismatch = upper.is_none_or(|next_cell| {
            next_cell.ramp.is_some()
                || next_cell.terrain == TerrainKind::Abyss
                || !next_cell.flags.contains(CellFlags::GROUND_PATHABLE)
                || next_cell.level.0 != low_level.0 + 1
        });

        if mismatch {
            errors.push(MapValidationError::RampUpperLandingMismatch {
                x: run.last.0,
                y: run.last.1,
            });
        }
    }

    // Check 8: RampLowerLandingMismatch
    for run in &ramp_runs {
        let Some((dx, dy)) = ramp_axis(run.dir) else {
            continue;
        };
        let Some(first_cell) = doc.cell(run.first.0, run.first.1) else {
            continue;
        };
        let low_level = first_cell.level;

        let lower = doc
            .offset_cell(run.first, (-dx, -dy))
            .and_then(|cell| doc.cell(cell.0, cell.1));
        let mismatch = lower.is_none_or(|prev_cell| {
            prev_cell.ramp.is_some()
                || prev_cell.terrain == TerrainKind::Abyss
                || !prev_cell.flags.contains(CellFlags::GROUND_PATHABLE)
                || prev_cell.level != low_level
        });

        if mismatch {
            errors.push(MapValidationError::RampLowerLandingMismatch {
                x: run.first.0,
                y: run.first.1,
            });
        }
    }

    // Check 9: RampRunLevelMismatch
    for run in &ramp_runs {
        for window in run.cells.windows(2) {
            let (c1_x, c1_y) = window[0];
            let (c2_x, c2_y) = window[1];
            let Some(spec1) = doc.cell(c1_x, c1_y) else {
                continue;
            };
            let Some(spec2) = doc.cell(c2_x, c2_y) else {
                continue;
            };
            if spec1.level != spec2.level {
                errors.push(MapValidationError::RampRunLevelMismatch { x: c2_x, y: c2_y });
            }
        }
    }

    // Check 10: TooFewStartLocations
    if doc.start_locations.len() < MIN_START_LOCATIONS {
        errors.push(MapValidationError::TooFewStartLocations {
            count: doc.start_locations.len(),
        });
    }

    // Check 11: PlayerSlotOutOfRange
    for start in &doc.start_locations {
        if start.player_slot >= MAX_PLAYER_SLOTS {
            errors.push(MapValidationError::PlayerSlotOutOfRange {
                id: start.id,
                slot: start.player_slot,
            });
        }
    }

    // Check 12: DuplicatePlayerSlot
    let mut seen_slots = BTreeSet::new();
    for start in &doc.start_locations {
        if !seen_slots.insert(start.player_slot) {
            errors.push(MapValidationError::DuplicatePlayerSlot {
                id: start.id,
                slot: start.player_slot,
            });
        }
    }

    // Build temporary GameMap for footprint and pathability testing
    let game_map_res = doc.to_game_map();

    // Check 13: StartFootprintBlocked
    if let Ok(ref map) = game_map_res {
        for start in &doc.start_locations {
            match map.can_place_building(
                start.cell.0,
                start.cell.1,
                start.footprint.0,
                start.footprint.1,
            ) {
                Ok(()) => {}
                Err(crate::error::MapError::PlacementBlocked { reason, .. }) => {
                    errors.push(MapValidationError::StartFootprintBlocked {
                        id: start.id,
                        reason,
                    });
                }
                Err(
                    crate::error::MapError::OutOfBounds { .. }
                    | crate::error::MapError::InvalidFootprintDimensions { .. },
                ) => {
                    errors.push(MapValidationError::StartFootprintBlocked {
                        id: start.id,
                        reason: PlacementFailureReason::OutOfBounds,
                    });
                }
                Err(_) => {
                    errors.push(MapValidationError::StartFootprintBlocked {
                        id: start.id,
                        reason: PlacementFailureReason::UnbuildableTerrain,
                    });
                }
            }
        }
    }

    // Check 14: StartLocationsTooClose
    for i in 0..doc.start_locations.len() {
        for j in (i + 1)..doc.start_locations.len() {
            let start_a = &doc.start_locations[i];
            let start_b = &doc.start_locations[j];

            let ca_x = i64::from(start_a.cell.0) * 2 + i64::from(start_a.footprint.0);
            let ca_y = i64::from(start_a.cell.1) * 2 + i64::from(start_a.footprint.1);
            let cb_x = i64::from(start_b.cell.0) * 2 + i64::from(start_b.footprint.0);
            let cb_y = i64::from(start_b.cell.1) * 2 + i64::from(start_b.footprint.1);

            let diff_x = (ca_x - cb_x).abs();
            let diff_y = (ca_y - cb_y).abs();
            let chebyshev_doubled = diff_x.max(diff_y);
            let distance = u32::try_from(chebyshev_doubled / 2).unwrap_or(u32::MAX);

            if distance < MIN_START_SEPARATION_CELLS {
                errors.push(MapValidationError::StartLocationsTooClose {
                    a: start_a.id,
                    b: start_b.id,
                    distance,
                });
            }
        }
    }

    // Check 15: DuplicateEntityId
    let mut seen_ids = BTreeSet::new();
    for r in &doc.resources {
        if !seen_ids.insert(r.id) {
            errors.push(MapValidationError::DuplicateEntityId { id: r.id });
        }
    }
    for wt in &doc.watchtowers {
        if !seen_ids.insert(wt.id) {
            errors.push(MapValidationError::DuplicateEntityId { id: wt.id });
        }
    }
    for s in &doc.start_locations {
        if !seen_ids.insert(s.id) {
            errors.push(MapValidationError::DuplicateEntityId { id: s.id });
        }
    }

    // Check 16: EntityOutOfBounds
    for r in &doc.resources {
        if r.cell.0 >= width || r.cell.1 >= height {
            errors.push(MapValidationError::EntityOutOfBounds {
                id: r.id,
                x: r.cell.0,
                y: r.cell.1,
            });
        }
    }
    for wt in &doc.watchtowers {
        if wt.cell.0 >= width || wt.cell.1 >= height {
            errors.push(MapValidationError::EntityOutOfBounds {
                id: wt.id,
                x: wt.cell.0,
                y: wt.cell.1,
            });
        }
    }
    for s in &doc.start_locations {
        if !footprint_in_bounds(s, width, height) {
            errors.push(MapValidationError::EntityOutOfBounds {
                id: s.id,
                x: s.cell.0,
                y: s.cell.1,
            });
        }
    }
    for wt in &doc.watchtowers {
        if WATCHTOWER_ENTITY_ID_BASE.checked_add(wt.id.0).is_none() {
            errors.push(MapValidationError::EntityIdOutOfRange { id: wt.id });
        }
    }

    // Check 17: ResourceOnBlockedCell
    for r in &doc.resources {
        if r.cell.0 < width && r.cell.1 < height {
            let spec = &doc.cells[(r.cell.1 * width + r.cell.0) as usize];
            if spec.terrain == TerrainKind::Abyss
                || spec.ramp.is_some()
                || !spec.flags.contains(CellFlags::GROUND_PATHABLE)
            {
                errors.push(MapValidationError::ResourceOnBlockedCell { id: r.id });
            }
        }
    }

    // Check 18: ResourceInsideStartFootprint
    for r in &doc.resources {
        for s in &doc.start_locations {
            let in_axis = |value: u32, start: u32, size: u8| {
                value >= start && u64::from(value) < u64::from(start) + u64::from(size)
            };
            if in_axis(r.cell.0, s.cell.0, s.footprint.0)
                && in_axis(r.cell.1, s.cell.1, s.footprint.1)
            {
                errors.push(MapValidationError::ResourceInsideStartFootprint {
                    id: r.id,
                    start: s.id,
                });
            }
        }
    }

    // Check 19: EntitiesOverlap (resources and watchtowers sharing the same cell)
    let mut cell_entities: Vec<(MapEntityId, (u32, u32))> = Vec::new();
    for r in &doc.resources {
        cell_entities.push((r.id, r.cell));
    }
    for wt in &doc.watchtowers {
        cell_entities.push((wt.id, wt.cell));
    }
    for i in 0..cell_entities.len() {
        for j in (i + 1)..cell_entities.len() {
            if cell_entities[i].1 == cell_entities[j].1 {
                errors.push(MapValidationError::EntitiesOverlap {
                    a: cell_entities[i].0,
                    b: cell_entities[j].0,
                });
            }
        }
    }

    // Check 20: ZeroResourceAmount
    for r in &doc.resources {
        if r.amount == 0 {
            errors.push(MapValidationError::ZeroResourceAmount { id: r.id });
        }
    }

    // Check 21: ZeroWatchtowerRadius
    for wt in &doc.watchtowers {
        if wt.capture_radius_tenths == 0 || wt.reveal_radius_tenths == 0 {
            errors.push(MapValidationError::ZeroWatchtowerRadius { id: wt.id });
        }
        if u32::from(wt.reveal_radius_tenths) > MAX_SIGHT_RADIUS_CELLS * 10 {
            errors.push(MapValidationError::WatchtowerRevealTooLarge { id: wt.id });
        }
    }

    // Check 22: StartLocationsUnreachable
    // Pathfinding is only evaluated if fundamental terrain, ramp, and start errors are absent.
    let has_fundamental_errors = errors.iter().any(|e| {
        matches!(
            e,
            MapValidationError::AbyssCellTraversable { .. }
                | MapValidationError::RampOnAbyss { .. }
                | MapValidationError::RampAtTopLevel { .. }
                | MapValidationError::RampNotPathable { .. }
                | MapValidationError::RampBuildable { .. }
                | MapValidationError::RampRunTooLong { .. }
                | MapValidationError::RampUpperLandingMismatch { .. }
                | MapValidationError::RampLowerLandingMismatch { .. }
                | MapValidationError::RampRunLevelMismatch { .. }
                | MapValidationError::TooFewStartLocations { .. }
                | MapValidationError::PlayerSlotOutOfRange { .. }
                | MapValidationError::DuplicatePlayerSlot { .. }
                | MapValidationError::StartFootprintBlocked { .. }
                | MapValidationError::StartLocationsTooClose { .. }
                | MapValidationError::EntityOutOfBounds { .. }
        )
    });

    if !has_fundamental_errors {
        if let Ok(ref map) = game_map_res {
            let mut sorted_starts = doc.start_locations.clone();
            sorted_starts.sort_by_key(|s| s.player_slot);

            for i in 0..sorted_starts.len() {
                for j in (i + 1)..sorted_starts.len() {
                    let start_a = &sorted_starts[i];
                    let start_b = &sorted_starts[j];

                    let pos_a = SimVec2::new(
                        SimScalar::from_num(start_a.cell.0)
                            + SimScalar::from_num(start_a.footprint.0) * SimScalar::HALF,
                        SimScalar::from_num(start_a.cell.1)
                            + SimScalar::from_num(start_a.footprint.1) * SimScalar::HALF,
                    );
                    let pos_b = SimVec2::new(
                        SimScalar::from_num(start_b.cell.0)
                            + SimScalar::from_num(start_b.footprint.0) * SimScalar::HALF,
                        SimScalar::from_num(start_b.cell.1)
                            + SimScalar::from_num(start_b.footprint.1) * SimScalar::HALF,
                    );

                    let mut pathfinder = Pathfinder::new(map);
                    let req = PathRequest {
                        start: pos_a,
                        goal: pos_b,
                        clearance: VALIDATION_PATH_CLEARANCE,
                    };

                    if pathfinder.find_path(map, &req).is_err() {
                        errors.push(MapValidationError::StartLocationsUnreachable {
                            a: start_a.id,
                            b: start_b.id,
                        });
                    }
                }
            }
        }
    }

    // Warnings:
    // 1. NarrowRamp: run width perpendicular to dir < MIN_RECOMMENDED_RAMP_WIDTH
    for run in &ramp_runs {
        let perpendicular = match run.dir {
            RampDir::North | RampDir::South => (1i32, 0i32),
            RampDir::East | RampDir::West => (0i32, 1i32),
            _ => continue,
        };

        let (x, y) = run.first;
        let Some(first_cell) = doc.cell(x, y) else {
            continue;
        };
        let base_level = first_cell.level;

        let count_beside = |step: (i32, i32)| {
            let mut count = 0u8;
            let mut at = (x, y);
            while let Some(next) = doc.offset_cell(at, step) {
                let matches_run = doc.cell(next.0, next.1).is_some_and(|neighbor| {
                    neighbor.ramp == Some(run.dir) && neighbor.level == base_level
                });
                if !matches_run {
                    break;
                }
                count = count.saturating_add(1);
                at = next;
            }
            count
        };
        let count_neg = count_beside((-perpendicular.0, -perpendicular.1));
        let count_pos = count_beside(perpendicular);

        let width = count_neg.saturating_add(count_pos).saturating_add(1);
        if width < MIN_RECOMMENDED_RAMP_WIDTH {
            warnings.push(MapValidationWarning::NarrowRamp { x, y, width });
        }
    }

    // 2. DiagonalPinch: 2x2 block where exactly the two diagonal cells are higher
    for y in 0..(height.saturating_sub(1)) {
        for x in 0..(width.saturating_sub(1)) {
            let (Some(c00), Some(c10), Some(c01), Some(c11)) = (
                doc.cell(x, y),
                doc.cell(x + 1, y),
                doc.cell(x, y + 1),
                doc.cell(x + 1, y + 1),
            ) else {
                continue;
            };
            let l00 = c00.level.0;
            let l10 = c10.level.0;
            let l01 = c01.level.0;
            let l11 = c11.level.0;

            let is_pinch = (l10 > l00 || l00 > l10) && l10 == l01 && l00 == l11;

            if is_pinch {
                warnings.push(MapValidationWarning::DiagonalPinch { x, y });
            }
        }
    }

    // 3 & 4. Resource association & imbalance
    let radius_sq =
        i64::from(RESOURCE_ASSOCIATION_RADIUS_CELLS) * i64::from(RESOURCE_ASSOCIATION_RADIUS_CELLS);
    let mut slot_resource_counts: BTreeMap<u8, usize> = BTreeMap::new();
    for s in &doc.start_locations {
        slot_resource_counts.insert(s.player_slot, 0);
    }

    for r in &doc.resources {
        // Out-of-bounds entities are already errors; association is only defined on the grid.
        if r.cell.0 >= width || r.cell.1 >= height {
            continue;
        }
        let mut best: Option<(u8, i64)> = None;

        for s in &doc.start_locations {
            if !footprint_in_bounds(s, width, height) {
                continue;
            }
            let center_x = i64::from(s.cell.0) + i64::from(s.footprint.0) / 2;
            let center_y = i64::from(s.cell.1) + i64::from(s.footprint.1) / 2;
            let dx = i64::from(r.cell.0) - center_x;
            let dy = i64::from(r.cell.1) - center_y;
            let dist_sq = dx * dx + dy * dy;

            if dist_sq <= radius_sq {
                match best {
                    None => best = Some((s.player_slot, dist_sq)),
                    Some((best_slot, best_dist)) => {
                        if dist_sq < best_dist
                            || (dist_sq == best_dist && s.player_slot < best_slot)
                        {
                            best = Some((s.player_slot, dist_sq));
                        }
                    }
                }
            }
        }

        if let Some((slot, _)) = best {
            *slot_resource_counts.entry(slot).or_insert(0) += 1;
        } else {
            warnings.push(MapValidationWarning::UnassignedResource { id: r.id });
        }
    }

    let max_count = slot_resource_counts.values().copied().max().unwrap_or(0);
    for s in &doc.start_locations {
        let count = slot_resource_counts
            .get(&s.player_slot)
            .copied()
            .unwrap_or(0);
        if count < max_count {
            warnings.push(MapValidationWarning::ResourceImbalance {
                slot: s.player_slot,
                count,
                max_count,
            });
        }
    }

    MapValidationReport { errors, warnings }
}
