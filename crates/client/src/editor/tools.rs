//! Editor interactive tools: brush footprints, paint operations, ramp inference, and entity editing.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §6.4 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this module encapsulates all pure editing tool logic, geometric footprints, ramp landing validation,
//! and entity selection/erasure routines.
//!
//! # Invariants Maintained
//! 1. **Footprint Bounds Invariant:** All generated brush and tool footprints are strictly clipped within `[0, width) × [0, height)`.
//! 2. **Ramp Geometry Invariant:** Ramps are placed exclusively on uniform-level cells with exactly one higher landing
//!    at `level + 1` and one lower landing at `level`, oriented along a cardinal direction.
//! 3. **Paint Op Side Effects:** Painting cliff levels clears ramps; painting Abyss clears pathability and ramps;
//!    painting non-Abyss on top of Abyss restores default ground pathability and buildability.
//! 4. **Deterministic Entity Order:** Entity picking and erasure operations produce stable, sorted entity vectors by ID.
//! 5. **Symmetry Integration:** All authoring tools apply operations symmetrically to the primary and mirrored footprints,
//!    ensuring cells on the symmetry axis are touched exactly once.

use std::collections::{BTreeMap, BTreeSet};

use sim_map::grid::{
    CellFlags, CliffLevel, RampDir, ResourceKind, TerrainKind, MAX_RAMP_RUN_CELLS,
};
use sim_map::map_format::{
    MapCellSpec, MapDocument, MapEntityId, MapResource, MapStartLocation, MapWatchtower,
};
use sim_map::validation::CellRect;

use crate::editor::commands::{CellDelta, EditCommand, MapEntity};
use crate::editor::stroke::StrokeRecorder;
use crate::editor::symmetry::SymmetryMode;

/// Maximum brush radius in cells.
pub const MAX_BRUSH_RADIUS: u8 = 7;

/// Default mineral deposit amount for freshly placed mineral fields.
pub const DEFAULT_MINERAL_AMOUNT: u32 = 1500;

/// Default vespene gas amount for freshly placed geysers.
pub const DEFAULT_GAS_AMOUNT: u32 = 2500;

/// Default watchtower capture radius in tenths of a world unit (3.0 units = 30 tenths).
pub const DEFAULT_WATCHTOWER_CAPTURE_RADIUS_TENTHS: u16 = 30;

/// Default watchtower reveal radius in tenths of a world unit (12.0 units = 120 tenths).
pub const DEFAULT_WATCHTOWER_REVEAL_RADIUS_TENTHS: u16 = 120;

/// Shape profile for circular or rectangular brush footprints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrushShape {
    /// Rounded footprint satisfying `dx² + dy² ≤ r² + r`.
    #[default]
    Circle,
    /// Axis-aligned square satisfying Chebyshev distance `max(|dx|, |dy|) ≤ r`.
    Square,
}

// PERFORMANCE RATIONALE:
// Why: The editor calls brush_footprint on every frame for hover cursor rendering and live stroke stamping.
// What: Footprint coordinates are written into a caller-provided `&mut Vec<(u32, u32)>` buffer,
// clearing it first and reusing its pre-allocated heap capacity across frames.
// Trade-off: The caller manages buffer lifecycle, achieving zero heap allocations per frame.
/// Computes the list of cell coordinates covered by a brush centered at `center`, writing into `out`.
///
/// # Brush Footprint Table
/// | Radius | Square cells | Circle condition: `dx² + dy² ≤ r² + r` | Circle cells | Shape description |
/// | :--- | :--- | :--- | :--- | :--- |
/// | 0 | 1 | `≤ 0` | 1 | Center cell only |
/// | 1 | 9 (3×3) | `≤ 2` | 9 | Full 3×3 square |
/// | 2 | 25 (5×5) | `≤ 6` | 21 | 5×5 minus 4 diagonal corners (±2, ±2) |
/// | 3 | 49 (7×7) | `≤ 12` | 37 | Rounded circle |
///
/// Footprints are strictly clipped within the map boundary `[0, width) × [0, height)`.
pub fn brush_footprint(
    shape: BrushShape,
    radius: u8,
    center: (u32, u32),
    width: u32,
    height: u32,
    out: &mut Vec<(u32, u32)>,
) {
    out.clear();
    let radius_int = i32::from(radius.min(MAX_BRUSH_RADIUS));
    let center_x = i32::try_from(center.0).unwrap_or(i32::MAX);
    let center_y = i32::try_from(center.1).unwrap_or(i32::MAX);
    let map_width = i32::try_from(width).unwrap_or(i32::MAX);
    let map_height = i32::try_from(height).unwrap_or(i32::MAX);

    let circle_threshold = radius_int * radius_int + radius_int;

    for dy in -radius_int..=radius_int {
        let y_pos = center_y + dy;
        if y_pos < 0 || y_pos >= map_height {
            continue;
        }
        for dx in -radius_int..=radius_int {
            let x_pos = center_x + dx;
            if x_pos < 0 || x_pos >= map_width {
                continue;
            }

            let include = match shape {
                BrushShape::Square => true,
                BrushShape::Circle => dx * dx + dy * dy <= circle_threshold,
            };

            if include {
                if let (Ok(x_u32), Ok(y_u32)) = (u32::try_from(x_pos), u32::try_from(y_pos)) {
                    out.push((x_u32, y_u32));
                }
            }
        }
    }
}

/// Applies a brush stamp to `doc` via `recorder`, touching both the primary footprint
/// and the mirrored footprint under `symmetry`.
///
/// Cells located directly on a symmetry axis are visited twice in geometry, but are
/// stamped and touched exactly once due to the single-touch generation invariant of [`StrokeRecorder`].
#[expect(
    clippy::too_many_arguments,
    reason = "explicit inputs keep this helper free of hidden state"
)]
pub fn paint_brush_stamp(
    recorder: &mut StrokeRecorder,
    doc: &mut MapDocument,
    shape: BrushShape,
    radius: u8,
    center: (u32, u32),
    op: &PaintOp,
    symmetry: SymmetryMode,
    footprint_buf: &mut Vec<(u32, u32)>,
) {
    brush_footprint(shape, radius, center, doc.width, doc.height, footprint_buf);
    for &(x, y) in footprint_buf.iter() {
        recorder.touch(doc, x, y, |cell| op.apply_to_cell(cell));
        if symmetry != SymmetryMode::None {
            let (mx, my) = symmetry.mirror_cell((x, y), doc.width, doc.height);
            recorder.touch(doc, mx, my, |cell| op.apply_to_cell(cell));
        }
    }
}

/// Directly authorable cell flag bits exposed in editor paint tool palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorableFlag {
    /// Bit 0: Ground structure placement allowed.
    Buildable,
    /// Bit 1: Ground unit traversal allowed.
    GroundPathable,
    /// Bit 2: Blocks ground and low-ground line of sight.
    LosBlocker,
    /// Bit 3: Speed boost zone.
    SpeedZone,
}

impl AuthorableFlag {
    /// Converts the authorable flag variant into its corresponding [`CellFlags`] bitmask.
    #[must_use]
    pub fn to_cell_flags(self) -> CellFlags {
        match self {
            Self::Buildable => CellFlags::BUILDABLE,
            Self::GroundPathable => CellFlags::GROUND_PATHABLE,
            Self::LosBlocker => CellFlags::LOS_BLOCKER,
            Self::SpeedZone => CellFlags::SPEED_ZONE,
        }
    }
}

/// Paint operation applied to individual cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaintOp {
    /// Sets cliff plateau elevation `0..=3`.
    Level(CliffLevel),
    /// Sets surface texture classification.
    Terrain(TerrainKind),
    /// Toggles or sets an authorable flag bit.
    Flag {
        /// Target authorable flag.
        flag: AuthorableFlag,
        /// New boolean state.
        value: bool,
    },
}

impl PaintOp {
    /// Computes the resulting [`MapCellSpec`] when applying this operation to `cell`.
    ///
    /// # Side Effects
    /// - Painting a cliff level clears any existing ramp.
    /// - Painting `TerrainKind::Abyss` clears `BUILDABLE | GROUND_PATHABLE` and ramps.
    /// - Painting non-Abyss terrain onto an Abyss cell restores default `BUILDABLE | GROUND_PATHABLE`.
    #[must_use]
    pub fn apply_to_cell(&self, cell: &MapCellSpec) -> MapCellSpec {
        let mut out = cell.clone();
        match self {
            Self::Level(level) => {
                out.level = *level;
                out.ramp = None;
            }
            Self::Terrain(terrain) => {
                if *terrain == TerrainKind::Abyss {
                    out.terrain = TerrainKind::Abyss;
                    out.ramp = None;
                    out.flags
                        .remove(CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE);
                } else {
                    if out.terrain == TerrainKind::Abyss {
                        out.flags
                            .insert(CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE);
                    }
                    out.terrain = *terrain;
                }
            }
            Self::Flag { flag, value } => {
                let bit = flag.to_cell_flags();
                if *value {
                    out.flags.insert(bit);
                } else {
                    out.flags.remove(bit);
                }
            }
        }
        out
    }
}

/// Inferred or validated ramp configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RampPlacement {
    /// Ascending compass direction of the ramp.
    pub dir: RampDir,
    /// Base elevation level of the lower landing.
    pub low_level: CliffLevel,
}

/// Diagnostics returned when ramp area validation or direction inference fails.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RampPlacementError {
    /// Ramp rectangle coordinates fall outside map boundaries.
    #[error("ramp area coordinates fall outside map boundaries")]
    OutOfBounds,
    /// Ramp rectangle contains abyss cells.
    #[error("ramp area contains abyss terrain")]
    ContainsAbyss,
    /// Cells within the ramp rectangle have differing cliff levels.
    #[error("ramp area cells have mixed cliff levels")]
    MixedLevels,
    /// Ramp orientation cannot be disambiguated from surrounding terrain.
    #[error("ramp placement is ambiguous (no single ascending direction)")]
    Ambiguous,
    /// No valid adjacent landing exists at elevation `low_level + 1`.
    #[error("no higher landing of level low + 1 exists adjacent to ramp area")]
    NoUpperLanding,
    /// Ramp span along the ascending direction exceeds [`MAX_RAMP_RUN_CELLS`].
    #[error("ramp run length of {len} cells exceeds maximum allowed run of {max}")]
    RunTooLong {
        /// Measured run length in cells.
        len: u32,
        /// Maximum allowed run length.
        max: u8,
    },
}

fn check_strip<I>(doc: &MapDocument, cells: I) -> Option<CliffLevel>
where
    I: Iterator<Item = (u32, u32)>,
{
    let mut strip_level = None;
    for (x, y) in cells {
        let cell = doc.cell(x, y)?;
        if cell.terrain == TerrainKind::Abyss || cell.ramp.is_some() {
            return None;
        }
        match strip_level {
            None => strip_level = Some(cell.level),
            Some(lvl) if lvl != cell.level => return None,
            _ => {}
        }
    }
    strip_level
}

/// Infers the ascending direction and base level for a rectangular ramp drag.
///
/// # Invariants
/// - All cells within `rect` must be non-Abyss and share the same elevation `L`.
/// - Exactly one adjacent cardinal side must have a uniform landing strip at `L + 1`,
///   and the opposite side must have a uniform landing strip at `L`.
/// - Ramp length along the inferred direction must not exceed [`MAX_RAMP_RUN_CELLS`].
///
/// # Errors
/// Returns [`RampPlacementError`] if landings do not qualify or dimensions exceed limits.
pub fn infer_ramp(doc: &MapDocument, rect: CellRect) -> Result<RampPlacement, RampPlacementError> {
    let (min_x, min_y) = rect.min;
    let (max_x, max_y) = rect.max_inclusive;
    if min_x > max_x || min_y > max_y || max_x >= doc.width || max_y >= doc.height {
        return Err(RampPlacementError::OutOfBounds);
    }

    let mut base_level = None;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let cell = doc.cell(x, y).ok_or(RampPlacementError::OutOfBounds)?;
            if cell.terrain == TerrainKind::Abyss {
                return Err(RampPlacementError::ContainsAbyss);
            }
            match base_level {
                None => base_level = Some(cell.level),
                Some(lvl) if lvl != cell.level => return Err(RampPlacementError::MixedLevels),
                _ => {}
            }
        }
    }
    let low_level = base_level.ok_or(RampPlacementError::OutOfBounds)?;
    if low_level.0 >= CliffLevel::MAX.0 {
        return Err(RampPlacementError::NoUpperLanding);
    }
    let target_high = CliffLevel(low_level.0 + 1);

    let north_level = if max_y + 1 < doc.height {
        check_strip(doc, (min_x..=max_x).map(|x| (x, max_y + 1)))
    } else {
        None
    };
    let south_level = if min_y > 0 {
        check_strip(doc, (min_x..=max_x).map(|x| (x, min_y - 1)))
    } else {
        None
    };
    let east_level = if max_x + 1 < doc.width {
        check_strip(doc, (min_y..=max_y).map(|y| (max_x + 1, y)))
    } else {
        None
    };
    let west_level = if min_x > 0 {
        check_strip(doc, (min_y..=max_y).map(|y| (min_x - 1, y)))
    } else {
        None
    };

    let north_ok = north_level == Some(target_high) && south_level == Some(low_level);
    let south_ok = south_level == Some(target_high) && north_level == Some(low_level);
    let east_ok = east_level == Some(target_high) && west_level == Some(low_level);
    let west_ok = west_level == Some(target_high) && east_level == Some(low_level);

    let mut candidates = Vec::new();
    if north_ok {
        candidates.push(RampDir::North);
    }
    if south_ok {
        candidates.push(RampDir::South);
    }
    if east_ok {
        candidates.push(RampDir::East);
    }
    if west_ok {
        candidates.push(RampDir::West);
    }

    if candidates.is_empty() {
        let any_high = north_level == Some(target_high)
            || south_level == Some(target_high)
            || east_level == Some(target_high)
            || west_level == Some(target_high);
        if any_high {
            Err(RampPlacementError::Ambiguous)
        } else {
            Err(RampPlacementError::NoUpperLanding)
        }
    } else if candidates.len() > 1 {
        Err(RampPlacementError::Ambiguous)
    } else {
        let dir = candidates[0];
        #[expect(
            clippy::unreachable,
            reason = "candidates holds only the four cardinal directions"
        )]
        let run_len = match dir {
            RampDir::North | RampDir::South => max_y - min_y + 1,
            RampDir::East | RampDir::West => max_x - min_x + 1,
            _ => unreachable!(),
        };

        if run_len > u32::from(MAX_RAMP_RUN_CELLS) {
            Err(RampPlacementError::RunTooLong {
                len: run_len,
                max: MAX_RAMP_RUN_CELLS,
            })
        } else {
            Ok(RampPlacement { dir, low_level })
        }
    }
}

/// Validates landing conditions for an explicitly specified ramp direction override.
///
/// # Errors
/// Returns [`RampPlacementError`] if the rect or adjacent landings fail validation.
pub fn check_ramp_placement(
    doc: &MapDocument,
    rect: CellRect,
    dir: RampDir,
) -> Result<RampPlacement, RampPlacementError> {
    let (min_x, min_y) = rect.min;
    let (max_x, max_y) = rect.max_inclusive;
    if min_x > max_x || min_y > max_y || max_x >= doc.width || max_y >= doc.height {
        return Err(RampPlacementError::OutOfBounds);
    }

    let mut base_level = None;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let cell = doc.cell(x, y).ok_or(RampPlacementError::OutOfBounds)?;
            if cell.terrain == TerrainKind::Abyss {
                return Err(RampPlacementError::ContainsAbyss);
            }
            match base_level {
                None => base_level = Some(cell.level),
                Some(lvl) if lvl != cell.level => return Err(RampPlacementError::MixedLevels),
                _ => {}
            }
        }
    }
    let low_level = base_level.ok_or(RampPlacementError::OutOfBounds)?;
    if low_level.0 >= CliffLevel::MAX.0 {
        return Err(RampPlacementError::NoUpperLanding);
    }
    let target_high = CliffLevel(low_level.0 + 1);

    let north_level = if max_y + 1 < doc.height {
        check_strip(doc, (min_x..=max_x).map(|x| (x, max_y + 1)))
    } else {
        None
    };
    let south_level = if min_y > 0 {
        check_strip(doc, (min_x..=max_x).map(|x| (x, min_y - 1)))
    } else {
        None
    };
    let east_level = if max_x + 1 < doc.width {
        check_strip(doc, (min_y..=max_y).map(|y| (max_x + 1, y)))
    } else {
        None
    };
    let west_level = if min_x > 0 {
        check_strip(doc, (min_y..=max_y).map(|y| (min_x - 1, y)))
    } else {
        None
    };

    let (upper, lower, run_len) = match dir {
        RampDir::North => (north_level, south_level, max_y - min_y + 1),
        RampDir::South => (south_level, north_level, max_y - min_y + 1),
        RampDir::East => (east_level, west_level, max_x - min_x + 1),
        RampDir::West => (west_level, east_level, max_x - min_x + 1),
        _ => return Err(RampPlacementError::Ambiguous),
    };

    if upper != Some(target_high) {
        return Err(RampPlacementError::NoUpperLanding);
    }
    if lower != Some(low_level) {
        return Err(RampPlacementError::Ambiguous);
    }
    if run_len > u32::from(MAX_RAMP_RUN_CELLS) {
        return Err(RampPlacementError::RunTooLong {
            len: run_len,
            max: MAX_RAMP_RUN_CELLS,
        });
    }

    Ok(RampPlacement { dir, low_level })
}

/// Constructs an [`EditCommand::PaintCells`] applying a validated ramp to `rect`,
/// including its mirrored rectangle under `symmetry`.
///
/// Both the primary and mirrored ramps must pass landing validation, otherwise returns an error.
///
/// # Errors
/// Returns [`RampPlacementError`] if either the primary or mirrored ramp fails landing checks.
pub fn place_ramp_command(
    doc: &MapDocument,
    rect: CellRect,
    placement: RampPlacement,
    symmetry: SymmetryMode,
    label: &'static str,
) -> Result<EditCommand, RampPlacementError> {
    let primary = check_ramp_placement(doc, rect, placement.dir)?;
    let mut placements = vec![(rect, primary)];

    if symmetry != SymmetryMode::None {
        let mirrored_rect = symmetry.mirror_rect(rect, doc.width, doc.height);
        let mirrored_dir = symmetry.mirror_ramp(placement.dir);
        let mirrored = check_ramp_placement(doc, mirrored_rect, mirrored_dir)?;
        if mirrored_rect != rect {
            placements.push((mirrored_rect, mirrored));
        }
    }

    let mut deltas_map = BTreeMap::new();
    for (r, p) in placements {
        for y in r.min.1..=r.max_inclusive.1 {
            for x in r.min.0..=r.max_inclusive.0 {
                if let Some(cell) = doc.cell(x, y) {
                    let before = cell.clone();
                    let mut after = before.clone();
                    after.ramp = Some(p.dir);
                    after.level = p.low_level;
                    after.flags = CellFlags::GROUND_PATHABLE;
                    if before != after {
                        deltas_map.insert(
                            (x, y),
                            CellDelta {
                                x,
                                y,
                                before,
                                after,
                            },
                        );
                    }
                }
            }
        }
    }

    let deltas: Vec<CellDelta> = deltas_map.into_values().collect();
    Ok(EditCommand::PaintCells { label, deltas })
}

/// Constructs an [`EditCommand::PaintCells`] applying `op` across a rectangular area,
/// including its mirrored rectangle under `symmetry`.
#[must_use]
pub fn box_fill_command(
    doc: &MapDocument,
    rect: CellRect,
    op: &PaintOp,
    symmetry: SymmetryMode,
    label: &'static str,
) -> EditCommand {
    let mut touched_cells = BTreeSet::new();

    for y in rect.min.1..=rect.max_inclusive.1 {
        for x in rect.min.0..=rect.max_inclusive.0 {
            touched_cells.insert((x, y));
            if symmetry != SymmetryMode::None {
                let (mx, my) = symmetry.mirror_cell((x, y), doc.width, doc.height);
                touched_cells.insert((mx, my));
            }
        }
    }

    let mut deltas = Vec::with_capacity(touched_cells.len());
    for (x, y) in touched_cells {
        if let Some(cell) = doc.cell(x, y) {
            let before = cell.clone();
            let after = op.apply_to_cell(&before);
            if before != after {
                deltas.push(CellDelta {
                    x,
                    y,
                    before,
                    after,
                });
            }
        }
    }
    EditCommand::PaintCells { label, deltas }
}

/// Errors arising during town hall start footprint catalog resolution.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StartFootprintError {
    /// Specified start loadout slug not found in catalog.
    #[error("unknown start loadout slug: {0}")]
    UnknownLoadout(String),
    /// Unit slug specified by loadout's town hall not found in catalog.
    #[error("unknown town hall unit slug: {0}")]
    UnknownUnitSlug(String),
    /// Town hall archetype is not a dome structure with discrete footprint dimensions.
    #[error("town hall archetype {0} is not a building dome")]
    TownHallNotADome(String),
}

/// Resolves the town hall start location footprint `(width, height)` in cells
/// for the specified start loadout slug by querying the loadout and unit catalogs.
///
/// # Errors
/// Returns [`StartFootprintError`] if the loadout is missing, unit slug is unknown, or shape is not a dome.
pub fn resolve_start_footprint(
    loadout_slug: &str,
    start_loadouts: &sim_core::scenario::StartLoadoutCatalog,
    unit_catalog: &sim_core::catalog::UnitCatalog,
) -> Result<(u8, u8), StartFootprintError> {
    let loadout = start_loadouts
        .get(loadout_slug)
        .ok_or_else(|| StartFootprintError::UnknownLoadout(loadout_slug.to_string()))?;
    let town_hall = unit_catalog
        .get_by_slug(&loadout.town_hall)
        .ok_or_else(|| StartFootprintError::UnknownUnitSlug(loadout.town_hall.clone()))?;
    match town_hall.shape {
        sim_types::enums::ShapeKind::BuildingDome {
            footprint_w,
            footprint_h,
        } => Ok((footprint_w, footprint_h)),
        sim_types::enums::ShapeKind::UnitSphere => Err(StartFootprintError::TownHallNotADome(
            loadout.town_hall.clone(),
        )),
    }
}

/// Category of entity placed by the entity tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityToolKind {
    /// Mineral resource field deposit.
    Mineral,
    /// Vespene gas geyser deposit.
    Gas,
    /// Vision and control watchtower.
    Watchtower,
    /// Player starting base location with specific slot index and resolved footprint.
    StartLocation {
        /// Player slot `0..MAX_PLAYER_SLOTS`.
        slot: u8,
        /// Town hall footprint dimensions in cells `(width, height)`.
        footprint: (u8, u8),
    },
}

/// Constructs an [`EditCommand`] to place a new entity at discrete cell coordinates,
/// including its mirrored partner under `symmetry`.
///
/// If placing a start location for an already occupied player slot, moves it instead. Under symmetry
/// the result always holds both slots, adding whichever is missing as one [`EditCommand::Batch`].
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "a single ordered sequence that shares its local state"
)]
pub fn place_entity_command(
    doc: &MapDocument,
    kind: EntityToolKind,
    cell: (u32, u32),
    symmetry: SymmetryMode,
) -> EditCommand {
    match kind {
        EntityToolKind::Mineral => {
            let primary_id = doc.next_entity_id();
            let mut entities = vec![MapEntity::Resource(MapResource {
                id: primary_id,
                kind: ResourceKind::Mineral,
                cell,
                amount: DEFAULT_MINERAL_AMOUNT,
            })];

            if symmetry != SymmetryMode::None {
                let mirrored_cell = symmetry.mirror_cell(cell, doc.width, doc.height);
                if mirrored_cell != cell {
                    entities.push(MapEntity::Resource(MapResource {
                        id: MapEntityId(primary_id.0 + 1),
                        kind: ResourceKind::Mineral,
                        cell: mirrored_cell,
                        amount: DEFAULT_MINERAL_AMOUNT,
                    }));
                }
            }
            EditCommand::AddEntities { entities }
        }
        EntityToolKind::Gas => {
            let primary_id = doc.next_entity_id();
            let mut entities = vec![MapEntity::Resource(MapResource {
                id: primary_id,
                kind: ResourceKind::Gas,
                cell,
                amount: DEFAULT_GAS_AMOUNT,
            })];

            if symmetry != SymmetryMode::None {
                let mirrored_cell = symmetry.mirror_cell(cell, doc.width, doc.height);
                if mirrored_cell != cell {
                    entities.push(MapEntity::Resource(MapResource {
                        id: MapEntityId(primary_id.0 + 1),
                        kind: ResourceKind::Gas,
                        cell: mirrored_cell,
                        amount: DEFAULT_GAS_AMOUNT,
                    }));
                }
            }
            EditCommand::AddEntities { entities }
        }
        EntityToolKind::Watchtower => {
            let primary_id = doc.next_entity_id();
            let mut entities = vec![MapEntity::Watchtower(MapWatchtower {
                id: primary_id,
                cell,
                capture_radius_tenths: DEFAULT_WATCHTOWER_CAPTURE_RADIUS_TENTHS,
                reveal_radius_tenths: DEFAULT_WATCHTOWER_REVEAL_RADIUS_TENTHS,
            })];

            if symmetry != SymmetryMode::None {
                let mirrored_cell = symmetry.mirror_cell(cell, doc.width, doc.height);
                if mirrored_cell != cell {
                    entities.push(MapEntity::Watchtower(MapWatchtower {
                        id: MapEntityId(primary_id.0 + 1),
                        cell: mirrored_cell,
                        capture_radius_tenths: DEFAULT_WATCHTOWER_CAPTURE_RADIUS_TENTHS,
                        reveal_radius_tenths: DEFAULT_WATCHTOWER_REVEAL_RADIUS_TENTHS,
                    }));
                }
            }
            EditCommand::AddEntities { entities }
        }
        EntityToolKind::StartLocation { slot, footprint } => {
            let primary_existing = doc.start_locations.iter().find(|s| s.player_slot == slot);
            let partner_slot = slot ^ 1;
            let partner_existing = if symmetry == SymmetryMode::None {
                None
            } else {
                doc.start_locations
                    .iter()
                    .find(|s| s.player_slot == partner_slot)
            };

            let primary_rect = CellRect {
                min: cell,
                max_inclusive: (
                    cell.0 + u32::from(footprint.0).saturating_sub(1),
                    cell.1 + u32::from(footprint.1).saturating_sub(1),
                ),
            };
            let mirrored_anchor = if symmetry == SymmetryMode::None {
                cell
            } else {
                symmetry
                    .mirror_rect(primary_rect, doc.width, doc.height)
                    .min
            };

            let primary_after = |id| {
                MapEntity::StartLocation(MapStartLocation {
                    id,
                    player_slot: slot,
                    cell,
                    footprint,
                })
            };
            let partner_after = |id| {
                MapEntity::StartLocation(MapStartLocation {
                    id,
                    player_slot: partner_slot,
                    cell: mirrored_anchor,
                    footprint,
                })
            };

            // Without symmetry only the primary slot is involved.
            if symmetry == SymmetryMode::None {
                return primary_existing.map_or_else(
                    || EditCommand::AddEntities {
                        entities: vec![primary_after(doc.next_entity_id())],
                    },
                    |existing| EditCommand::ModifyEntity {
                        before: MapEntity::StartLocation(existing.clone()),
                        after: primary_after(existing.id),
                    },
                );
            }

            // A mirrored placement always leaves both slots present and paired. A slot that
            // already exists is moved, keeping its identity and its one-start-per-slot rule.
            let new_id = doc.next_entity_id();
            match (primary_existing, partner_existing) {
                (Some(primary), Some(partner)) => EditCommand::ModifyEntities {
                    modifications: vec![
                        (
                            MapEntity::StartLocation(primary.clone()),
                            primary_after(primary.id),
                        ),
                        (
                            MapEntity::StartLocation(partner.clone()),
                            partner_after(partner.id),
                        ),
                    ],
                },
                (Some(primary), None) => EditCommand::Batch {
                    commands: vec![
                        EditCommand::ModifyEntity {
                            before: MapEntity::StartLocation(primary.clone()),
                            after: primary_after(primary.id),
                        },
                        EditCommand::AddEntities {
                            entities: vec![partner_after(new_id)],
                        },
                    ],
                },
                (None, Some(partner)) => EditCommand::Batch {
                    commands: vec![
                        EditCommand::ModifyEntity {
                            before: MapEntity::StartLocation(partner.clone()),
                            after: partner_after(partner.id),
                        },
                        EditCommand::AddEntities {
                            entities: vec![primary_after(new_id)],
                        },
                    ],
                },
                (None, None) => EditCommand::AddEntities {
                    entities: vec![
                        primary_after(new_id),
                        partner_after(MapEntityId(new_id.0 + 1)),
                    ],
                },
            }
        }
    }
}

/// Picks the topmost entity whose cell or multi-cell footprint intersects `cell`.
///
/// Resources and watchtowers take precedence over start location bounding footprints.
#[must_use]
pub fn pick_entity(doc: &MapDocument, cell: (u32, u32)) -> Option<MapEntity> {
    for r in doc.resources.iter().rev() {
        if r.cell == cell {
            return Some(MapEntity::Resource(r.clone()));
        }
    }

    for w in doc.watchtowers.iter().rev() {
        if w.cell == cell {
            return Some(MapEntity::Watchtower(w.clone()));
        }
    }

    for s in doc.start_locations.iter().rev() {
        let min_x = s.cell.0;
        let min_y = s.cell.1;
        let max_x = min_x + u32::from(s.footprint.0);
        let max_y = min_y + u32::from(s.footprint.1);

        if cell.0 >= min_x && cell.0 < max_x && cell.1 >= min_y && cell.1 < max_y {
            return Some(MapEntity::StartLocation(s.clone()));
        }
    }

    None
}

/// Constructs an [`EditCommand`] moving `entity` to `new_cell`.
///
/// If `symmetry` is active and a mirrored partner exists at the mirrored position,
/// moves both entities atomically via [`EditCommand::ModifyEntities`].
#[must_use]
pub fn move_entity_command(
    doc: &MapDocument,
    entity: &MapEntity,
    new_cell: (u32, u32),
    symmetry: SymmetryMode,
) -> EditCommand {
    let mut primary_after = entity.clone();
    match &mut primary_after {
        MapEntity::Resource(r) => r.cell = new_cell,
        MapEntity::Watchtower(w) => w.cell = new_cell,
        MapEntity::StartLocation(s) => s.cell = new_cell,
    }

    if symmetry != SymmetryMode::None {
        let old_mirrored_cell = symmetry.mirror_cell(entity.cell(), doc.width, doc.height);
        let new_mirrored_cell = symmetry.mirror_cell(new_cell, doc.width, doc.height);

        match entity {
            MapEntity::Resource(r) => {
                if let Some(partner) = doc.resources.iter().find(|res| {
                    res.cell == old_mirrored_cell && res.id != r.id && res.kind == r.kind
                }) {
                    let partner_before = MapEntity::Resource(partner.clone());
                    let mut partner_after = partner.clone();
                    partner_after.cell = new_mirrored_cell;
                    return EditCommand::ModifyEntities {
                        modifications: vec![
                            (entity.clone(), primary_after),
                            (partner_before, MapEntity::Resource(partner_after)),
                        ],
                    };
                }
            }
            MapEntity::Watchtower(w) => {
                if let Some(partner) = doc
                    .watchtowers
                    .iter()
                    .find(|wt| wt.cell == old_mirrored_cell && wt.id != w.id)
                {
                    let partner_before = MapEntity::Watchtower(partner.clone());
                    let mut partner_after = partner.clone();
                    partner_after.cell = new_mirrored_cell;
                    return EditCommand::ModifyEntities {
                        modifications: vec![
                            (entity.clone(), primary_after),
                            (partner_before, MapEntity::Watchtower(partner_after)),
                        ],
                    };
                }
            }
            MapEntity::StartLocation(s) => {
                let partner_slot = s.player_slot ^ 1;
                if let Some(partner) = doc
                    .start_locations
                    .iter()
                    .find(|loc| loc.player_slot == partner_slot && loc.id != s.id)
                {
                    let primary_rect = CellRect {
                        min: new_cell,
                        max_inclusive: (
                            new_cell.0 + u32::from(s.footprint.0).saturating_sub(1),
                            new_cell.1 + u32::from(s.footprint.1).saturating_sub(1),
                        ),
                    };
                    let mirrored_anchor = symmetry
                        .mirror_rect(primary_rect, doc.width, doc.height)
                        .min;

                    let partner_before = MapEntity::StartLocation(partner.clone());
                    let mut partner_after = partner.clone();
                    partner_after.cell = mirrored_anchor;
                    return EditCommand::ModifyEntities {
                        modifications: vec![
                            (entity.clone(), primary_after),
                            (partner_before, MapEntity::StartLocation(partner_after)),
                        ],
                    };
                }
            }
        }
    }

    EditCommand::ModifyEntity {
        before: entity.clone(),
        after: primary_after,
    }
}

/// Constructs an [`EditCommand::RemoveEntities`] removing `entity` from the document.
#[must_use]
pub fn delete_entity_command(entity: &MapEntity) -> EditCommand {
    EditCommand::RemoveEntities {
        entities: vec![entity.clone()],
    }
}

/// Clears painted ramps and flags from `cell`, restoring default buildable ground.
///
/// Abyss cells are returned unchanged: they must stay non-traversable to remain valid.
fn erased_cell(cell: &MapCellSpec) -> MapCellSpec {
    let mut out = cell.clone();
    if out.terrain != TerrainKind::Abyss {
        out.ramp = None;
        out.flags = CellFlags::GROUND_PATHABLE | CellFlags::BUILDABLE;
    }
    out
}

/// Constructs an [`EditCommand::PaintCells`] clearing `cell` and its reflection under `symmetry`.
///
/// Returns `None` if neither cell has anything to erase, so a no-op leaves no undo entry.
#[must_use]
pub fn erase_cell_command(
    doc: &MapDocument,
    cell: (u32, u32),
    symmetry: SymmetryMode,
) -> Option<EditCommand> {
    let mirrored = symmetry.mirror_cell(cell, doc.width, doc.height);
    let deltas: Vec<CellDelta> = BTreeSet::from([cell, mirrored])
        .into_iter()
        .filter_map(|(x, y)| {
            let before = doc.cell(x, y)?;
            let after = erased_cell(before);
            (*before != after).then(|| CellDelta {
                x,
                y,
                before: before.clone(),
                after,
            })
        })
        .collect();

    (!deltas.is_empty()).then_some(EditCommand::PaintCells {
        label: "Erase Cell",
        deltas,
    })
}

/// Identifies all entities intersecting `footprint_cells` (and their symmetric reflections under `symmetry`)
/// and returns an [`EditCommand::RemoveEntities`].
#[must_use]
pub fn erase_entities_command(
    doc: &MapDocument,
    footprint_cells: &[(u32, u32)],
    symmetry: SymmetryMode,
) -> Option<EditCommand> {
    let mut all_cells = BTreeSet::new();
    for &(x, y) in footprint_cells {
        all_cells.insert((x, y));
        if symmetry != SymmetryMode::None {
            all_cells.insert(symmetry.mirror_cell((x, y), doc.width, doc.height));
        }
    }

    let mut entities = Vec::new();

    for r in &doc.resources {
        if all_cells.contains(&r.cell) {
            entities.push(MapEntity::Resource(r.clone()));
        }
    }

    for w in &doc.watchtowers {
        if all_cells.contains(&w.cell) {
            entities.push(MapEntity::Watchtower(w.clone()));
        }
    }

    for s in &doc.start_locations {
        let min_x = s.cell.0;
        let min_y = s.cell.1;
        let max_x = min_x + u32::from(s.footprint.0);
        let max_y = min_y + u32::from(s.footprint.1);

        let intersects = all_cells
            .iter()
            .any(|&(x, y)| x >= min_x && x < max_x && y >= min_y && y < max_y);

        if intersects {
            entities.push(MapEntity::StartLocation(s.clone()));
        }
    }

    entities.sort_by_key(MapEntity::id);
    entities.dedup_by_key(|e| e.id());

    if entities.is_empty() {
        None
    } else {
        Some(EditCommand::RemoveEntities { entities })
    }
}
