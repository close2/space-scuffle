//! 1x1 Cell Grid, Cliff Levels, Ramps, and Building Placement.
//!
//! This module implements the discrete 1x1 tile grid foundation of Space Scuffle.
//! It models discrete cliff elevation plateaus (`0..=3`), ramp orientation,
//! cell passability and buildability flags, building footprint placement and removal,
//! and continuous altitude queries.

use crate::error::{MapError, PlacementFailureReason, ResourceFootprintFailure, Result};
use crate::footprint::{PlacedFootprint, ResourceFootprints, StructureFootprint};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
pub use sim_types::ResourceKind;
use sim_types::{EntityId, FootprintShape, PlayerId, ResourceNodeId, SimScalar, SimVec2};
use std::collections::BTreeMap;

/// Discrete cliff elevation plateau (0 to 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CliffLevel(pub u8);

impl CliffLevel {
    /// Minimum allowed cliff plateau level (ground level 0).
    pub const MIN: Self = Self(0);

    /// Maximum allowed cliff plateau level (high ground 3).
    pub const MAX: Self = Self(3);

    /// Computes deterministic fixed-point altitude for this cliff plateau.
    ///
    /// Each cliff level increments altitude by 3.0 world units.
    #[must_use]
    #[inline]
    pub fn height_scalar(self) -> SimScalar {
        SimScalar::from_num(self.0) * SimScalar::from_num(CLIFF_LEVEL_HEIGHT)
    }

    /// Computes visual floating-point altitude for rendering interpolation.
    #[must_use]
    #[inline]
    pub fn height_f32(self) -> f32 {
        self.height_scalar().to_f32()
    }
}

/// World-unit height of one cliff level step.
pub const CLIFF_LEVEL_HEIGHT: i32 = 3;

/// Maximum allowed contiguous ramp run in cells along its ascending direction.
///
/// Maps containing contiguous ramp runs strictly exceeding this length are rejected by
/// [`crate::validation::validate_map`] with [`crate::validation::MapValidationError::RampRunTooLong`].
pub const MAX_RAMP_RUN_CELLS: u8 = 8;

/// Compass direction that a ramp ascends towards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RampDir {
    /// Ascends towards positive Y (+Y).
    North,
    /// Ascends towards negative Y (-Y).
    South,
    /// Ascends towards positive X (+X).
    East,
    /// Ascends towards negative X (-X).
    West,
    /// Ascends towards positive X and positive Y (+X, +Y).
    NorthEast,
    /// Ascends towards negative X and positive Y (-X, +Y).
    NorthWest,
    /// Ascends towards positive X and negative Y (+X, -Y).
    SouthEast,
    /// Ascends towards negative X and negative Y (-X, -Y).
    SouthWest,
}

/// Position of a ramp cell inside its contiguous ramp run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RampProfile {
    /// Compass direction that this ramp ascends towards.
    pub dir: RampDir,
    /// Base cliff level at the lower landing of this ramp run.
    pub low_level: CliffLevel,
    /// Zero-based cell index of this cell along the ascending direction within its run.
    pub index: u8,
    /// Total number of contiguous ramp cells in this run (invariant: strictly >= 1).
    pub run_len: u8,
}

impl RampProfile {
    /// Ramp progress t ∈ [0, 1] at cell-local coordinates `(fx, fy)` ∈ [0, 1]².
    #[must_use]
    pub fn progress(&self, fx: SimScalar, fy: SimScalar) -> SimScalar {
        let zero = SimScalar::ZERO;
        let one = SimScalar::ONE;
        let fx = fx.clamp(zero, one);
        let fy = fy.clamp(zero, one);

        let along = match self.dir {
            RampDir::North => fy,
            RampDir::South => one - fy,
            RampDir::East => fx,
            RampDir::West => one - fx,
            // Legacy diagonal progress formulas (not producible by map format v1)
            RampDir::NorthEast => (fx + fy) / SimScalar::from_num(2),
            RampDir::NorthWest => ((one - fx) + fy) / SimScalar::from_num(2),
            RampDir::SouthEast => (fx + (one - fy)) / SimScalar::from_num(2),
            RampDir::SouthWest => ((one - fx) + (one - fy)) / SimScalar::from_num(2),
        };

        if matches!(
            self.dir,
            RampDir::NorthEast | RampDir::NorthWest | RampDir::SouthEast | RampDir::SouthWest
        ) {
            along
        } else {
            (SimScalar::from_num(self.index) + along) / SimScalar::from_num(self.run_len)
        }
    }

    /// Surface altitude at cell-local coordinates `(fx, fy)` in world units.
    #[must_use]
    pub fn height_at(&self, fx: SimScalar, fy: SimScalar) -> SimScalar {
        self.low_level.height_scalar()
            + self.progress(fx, fy) * SimScalar::from_num(CLIFF_LEVEL_HEIGHT)
    }
}

/// Visual and acoustic terrain type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerrainKind {
    /// Natural vegetation.
    Grass,
    /// Manufactured steel floor.
    MetalPlate,
    /// Barren rock and dust.
    Dirt,
    /// Bottomless void / space abyss.
    Abyss,
}

bitflags! {
    /// 16-bit flags defining spatial, pathfinding, and vision properties of a single 1x1 cell.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct CellFlags: u16 {
        /// Ground units can construct buildings on this cell.
        const BUILDABLE              = 1 << 0;
        /// Ground units can traverse this cell.
        const GROUND_PATHABLE        = 1 << 1;
        /// Ground units cannot see through this cell (e.g., tall grass or smoke vents).
        const LOS_BLOCKER            = 1 << 2;
        /// Modifies ground unit movement speed (creep or speed zone).
        const SPEED_ZONE             = 1 << 3;
        /// A structure currently occupies this cell.
        const OCCUPIED_BY_STRUCTURE  = 1 << 4;
        /// Cell is part of a ramp connecting two cliff levels.
        const RAMP                   = 1 << 5;
        /// Cell is an impassable cliff drop-off.
        const UNPASSABLE_CLIFF       = 1 << 6;
        /// A standing resource node's footprint occupies this cell (runtime state, plan
        /// `2026-11-20-resource-footprints` D-5).
        const OCCUPIED_BY_RESOURCE   = 1 << 7;
    }
}

/// Properties of a single 1x1 tile on the map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapCell {
    /// Elevation plateau of this cell.
    pub cliff_level: CliffLevel,
    /// Optional ramp ascending direction if this cell is a ramp tile.
    pub ramp: Option<RampDir>,
    /// Visual and material terrain category.
    pub terrain_kind: TerrainKind,
    /// Bitflags controlling passability, vision, and buildability.
    pub flags: CellFlags,
    /// Identifier of structure occupying this cell, if any.
    pub occupying_structure: Option<EntityId>,
    /// The standing resource node whose footprint occupies this cell, if any (runtime state).
    pub occupying_resource: Option<ResourceNodeId>,
}

impl MapCell {
    /// Creates a default buildable and pathable flat ground cell at the given cliff level.
    #[must_use]
    pub fn default_ground(cliff: CliffLevel) -> Self {
        Self {
            cliff_level: cliff,
            ramp: None,
            terrain_kind: TerrainKind::Grass,
            flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
            occupying_structure: None,
            occupying_resource: None,
        }
    }
}

/// Natural resource node located on the map.
///
/// A node blocks exactly when it is placed (plan `2026-11-20-resource-footprints` D-4): its
/// footprint, from the catalog's resource footprint of its kind, takes cells relative to its
/// anchor `cell`, and its outline is cut into the navigation mesh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceNode {
    /// Continuous 2D world position: the anchor cell's centre until the node is first placed,
    /// the centre of its footprint from then on.
    pub pos: SimVec2,
    /// Type of resource.
    pub kind: ResourceKind,
    /// Remaining harvestable units.
    pub remaining: u32,
    /// The anchor cell: the node's footprint is placed relative to it.
    pub cell: (u32, u32),
    /// The node's placed footprint while it stands; `None` before placement and after removal.
    pub placed: Option<PlacedResource>,
}

/// A standing resource node's footprint and the flags its cells had before it was placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacedResource {
    /// The footprint's cells and outline.
    pub footprint: PlacedFootprint,
    /// Row-major flags of the footprint cells from before placement, restored on removal; slots
    /// past the footprint's cell count are empty.
    pub prior_flags: [CellFlags; 16],
}

/// Neutral observation tower that grants vision when occupied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watchtower {
    /// Unique entity ID of the tower.
    pub id: EntityId,
    /// Continuous 2D world position.
    pub pos: SimVec2,
    /// Proximity radius required to capture control of the tower.
    pub capture_radius: SimScalar,
    /// Vision radius granted to controlling player.
    pub reveal_radius: SimScalar,
    /// Player ID currently controlling this tower, or `None` if neutral/contested.
    pub controlling_player: Option<PlayerId>,
}

/// Metadata tracking a building dome placed on the grid.
///
/// A structure has two layers (plan `2026-11-18-contour-footprints` D-1): its footprint cells
/// (`top_left`, `width`, `height`), which it takes from the placement grid, and its pathing
/// `shape`, centred on those cells, which the navigation mesh, the clearance field and ground
/// movement read. Its geometry is in `footprint.rs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacedStructure {
    /// Structure entity ID.
    pub id: EntityId,
    /// Top-left (minimum X, minimum Y) cell coordinate.
    pub top_left: (u32, u32),
    /// Foundation width in 1x1 cells.
    pub width: u8,
    /// Foundation height in 1x1 cells.
    pub height: u8,
    /// Row-major flags of the footprint cells from before placement, restored on removal.
    pub prior_flags: Vec<CellFlags>,
    /// Pathing shape, centred on the footprint cells, in quarter cells; it lies in the cells.
    pub shape: FootprintShape,
}

/// The authoritative game map representing terrain, cliffs, ramps, and obstacles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMap {
    width: u32,
    height: u32,
    cells: Vec<MapCell>,
    structures: BTreeMap<EntityId, PlacedStructure>,
    resources: Vec<ResourceNode>,
    watchtowers: Vec<Watchtower>,
    /// Counts mutable accesses to cells. Derived state, neither saved nor hashed.
    #[serde(skip)]
    cell_revision: u64,
}

impl GameMap {
    /// Constructs a new empty game map initialized with `default_cell`.
    ///
    /// # Errors
    /// Returns `Err(MapError::InvalidDimensions)` if width or height is 0.
    pub fn new(width: u32, height: u32, default_cell: MapCell) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(MapError::InvalidDimensions { width, height });
        }
        let total = (width as usize)
            .checked_mul(height as usize)
            .ok_or(MapError::InvalidDimensions { width, height })?;
        let cells = vec![default_cell; total];
        Ok(Self {
            width,
            height,
            cells,
            structures: BTreeMap::new(),
            resources: Vec::new(),
            watchtowers: Vec::new(),
            cell_revision: 0,
        })
    }

    /// Constructs a game map from an existing collection of cells and entities.
    ///
    /// # Errors
    /// Returns `Err(MapError::InvalidDimensions)` if dimensions do not match cell vector length.
    pub fn from_cells(
        width: u32,
        height: u32,
        cells: Vec<MapCell>,
        resources: Vec<ResourceNode>,
        watchtowers: Vec<Watchtower>,
    ) -> Result<Self> {
        if width == 0 || height == 0 || cells.len() != (width as usize) * (height as usize) {
            return Err(MapError::InvalidDimensions { width, height });
        }
        Ok(Self {
            width,
            height,
            cells,
            structures: BTreeMap::new(),
            resources,
            watchtowers,
            cell_revision: 0,
        })
    }

    /// Width of the map in 1x1 grid cells.
    #[must_use]
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height of the map in 1x1 grid cells.
    #[must_use]
    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns a slice of all cells in row-major order (`index = y * width + x`).
    #[must_use]
    #[inline]
    pub fn cells(&self) -> &[MapCell] {
        &self.cells
    }

    /// Returns a mutable slice of all cells.
    #[must_use]
    #[inline]
    pub fn cells_mut(&mut self) -> &mut [MapCell] {
        self.cell_revision = self.cell_revision.wrapping_add(1);
        &mut self.cells
    }

    /// Changes whenever cells may have been edited, so derived data knows to rebuild.
    #[must_use]
    pub fn cell_revision(&self) -> u64 {
        self.cell_revision
    }

    /// Returns a reference to all placed structures on the map.
    #[must_use]
    #[inline]
    pub fn structures(&self) -> &BTreeMap<EntityId, PlacedStructure> {
        &self.structures
    }

    /// Returns a slice of natural resource nodes.
    #[must_use]
    #[inline]
    pub fn resources(&self) -> &[ResourceNode] {
        &self.resources
    }

    /// Returns a mutable slice of natural resource nodes.
    #[must_use]
    #[inline]
    pub fn resources_mut(&mut self) -> &mut [ResourceNode] {
        &mut self.resources
    }

    /// Returns a slice of watchtowers.
    #[must_use]
    #[inline]
    pub fn watchtowers(&self) -> &[Watchtower] {
        &self.watchtowers
    }

    /// Returns a mutable slice of watchtowers.
    #[must_use]
    #[inline]
    pub fn watchtowers_mut(&mut self) -> &mut [Watchtower] {
        &mut self.watchtowers
    }

    /// Tests whether discrete grid coordinates `(x, y)` reside within map boundaries.
    #[must_use]
    #[inline]
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        u32::try_from(x).is_ok_and(|x| x < self.width)
            && u32::try_from(y).is_ok_and(|y| y < self.height)
    }

    /// Returns the cell at a signed offset from `cell`, or `None` if that leaves the map.
    #[must_use]
    #[inline]
    pub fn offset_cell(&self, cell: (u32, u32), offset: (i32, i32)) -> Option<(u32, u32)> {
        let x = cell.0.checked_add_signed(offset.0)?;
        let y = cell.1.checked_add_signed(offset.1)?;
        (x < self.width && y < self.height).then_some((x, y))
    }

    fn out_of_bounds(&self, x: u32, y: u32) -> MapError {
        MapError::OutOfBounds {
            x: i32::try_from(x).unwrap_or(i32::MAX),
            y: i32::try_from(y).unwrap_or(i32::MAX),
            width: self.width,
            height: self.height,
        }
    }

    /// Retrieves an immutable reference to the cell at discrete grid coordinates `(x, y)`.
    ///
    /// # Errors
    /// Returns `Err(MapError::OutOfBounds)` if coordinates fall outside the map.
    pub fn cell(&self, x: u32, y: u32) -> Result<&MapCell> {
        if x >= self.width || y >= self.height {
            return Err(self.out_of_bounds(x, y));
        }
        let idx = (y as usize) * (self.width as usize) + (x as usize);
        Ok(&self.cells[idx])
    }

    /// Retrieves a mutable reference to the cell at discrete grid coordinates `(x, y)`.
    ///
    /// # Errors
    /// Returns `Err(MapError::OutOfBounds)` if coordinates fall outside the map.
    pub fn cell_mut(&mut self, x: u32, y: u32) -> Result<&mut MapCell> {
        if x >= self.width || y >= self.height {
            return Err(self.out_of_bounds(x, y));
        }
        self.cell_revision = self.cell_revision.wrapping_add(1);
        let idx = (y as usize) * (self.width as usize) + (x as usize);
        Ok(&mut self.cells[idx])
    }

    /// Converts continuous 2D world coordinates into discrete 1x1 grid cell coordinates.
    ///
    /// # Errors
    /// Returns `Err(MapError::OutOfBounds)` if `pos` lies outside map bounds.
    pub fn world_to_cell(&self, pos: SimVec2) -> Result<(u32, u32)> {
        if pos.x < SimScalar::ZERO || pos.y < SimScalar::ZERO {
            return Err(MapError::OutOfBounds {
                x: -1,
                y: -1,
                width: self.width,
                height: self.height,
            });
        }
        // Both coordinates are nonnegative here, so this conversion is a plain floor.
        let cx = pos.x.saturating_to_u32();
        let cy = pos.y.saturating_to_u32();
        if cx >= self.width || cy >= self.height {
            return Err(self.out_of_bounds(cx, cy));
        }
        Ok((cx, cy))
    }

    /// The cliff level a position stands on: the level stored in the cell under `pos`, a ramp
    /// cell counting as its lower landing (as the map stores it). A position off the map takes
    /// the level of the nearest cell on the map, so every position has a level. The level
    /// depends on the position alone, not on what stands there: an air unit has the level of
    /// the ground below it.
    ///
    /// `pos` is in world units; a coordinate is floored to its cell after a clamp to
    /// `[0, width)` × `[0, height)`, so a whole-number coordinate lies in the cell that starts
    /// there. Area damage reads it to keep a search to its centre's level (plan
    /// `2026-11-23-splash-cliff-levels` D-2, `StarCraft` II's `SameCliff`).
    #[must_use]
    pub fn cliff_level_at(&self, pos: SimVec2) -> CliffLevel {
        // `saturating_to_u32` floors and turns a negative coordinate into 0; `min` keeps a
        // coordinate at or past the far edge on the last cell. `new` and `from_cells` refuse a
        // zero size, so the subtraction cannot wrap. Both constructors also make `cells` cover
        // the size; only a map deserialized with fewer cells than its size could miss, and it
        // reads the lowest level there rather than panicking in the simulation.
        let x = pos.x.saturating_to_u32().min(self.width.saturating_sub(1));
        let y = pos.y.saturating_to_u32().min(self.height.saturating_sub(1));
        let index = (y as usize) * (self.width as usize) + (x as usize);
        self.cells
            .get(index)
            .map_or(CliffLevel::MIN, |cell| cell.cliff_level)
    }

    /// Converts discrete grid cell coordinates into the continuous world center position `(x + 0.5, y + 0.5)`.
    ///
    /// # Errors
    /// Returns `Err(MapError::OutOfBounds)` if cell coordinates are outside map bounds.
    pub fn cell_to_world_center(&self, x: u32, y: u32) -> Result<SimVec2> {
        if x >= self.width || y >= self.height {
            return Err(self.out_of_bounds(x, y));
        }
        Ok(SimVec2::new(
            SimScalar::from_num(x) + SimScalar::HALF,
            SimScalar::from_num(y) + SimScalar::HALF,
        ))
    }

    /// Returns the ramp profile of cell `(x, y)`, or `None` if it is not a ramp.
    #[must_use]
    pub fn ramp_profile(&self, x: u32, y: u32) -> Option<RampProfile> {
        let cell = self.cell(x, y).ok()?;
        let dir = cell.ramp?;

        let (dx, dy) = match dir {
            RampDir::North => (0i32, 1i32),
            RampDir::South => (0i32, -1i32),
            RampDir::East => (1i32, 0i32),
            RampDir::West => (-1i32, 0i32),
            RampDir::NorthEast | RampDir::NorthWest | RampDir::SouthEast | RampDir::SouthWest => {
                // Legacy behaviour for diagonal ramps (index = 0, run_len = 1)
                return Some(RampProfile {
                    dir,
                    low_level: cell.cliff_level,
                    index: 0,
                    run_len: 1,
                });
            }
        };

        // Walk in -s (descending direction)
        let continues_run = |candidate: Option<(u32, u32)>| {
            candidate.filter(|&(cx, cy)| {
                self.cell(cx, cy).is_ok_and(|neighbor| {
                    neighbor.ramp == Some(dir) && neighbor.cliff_level == cell.cliff_level
                })
            })
        };

        let mut index = 0u8;
        let mut at = (x, y);
        while index < MAX_RAMP_RUN_CELLS - 1 {
            let Some(next) = continues_run(self.offset_cell(at, (-dx, -dy))) else {
                break;
            };
            at = next;
            index += 1;
        }

        // Walk in +s (ascending direction)
        let mut after = 0u8;
        at = (x, y);
        while after < MAX_RAMP_RUN_CELLS - 1 {
            let Some(next) = continues_run(self.offset_cell(at, (dx, dy))) else {
                break;
            };
            at = next;
            after += 1;
        }

        let run_len = index + 1 + after;
        Some(RampProfile {
            dir,
            low_level: cell.cliff_level,
            index,
            run_len,
        })
    }

    /// Computes deterministic fixed-point terrain altitude at continuous world position `pos`.
    ///
    /// If `pos` falls on a flat plateau, returns `cliff_level.height_scalar()`.
    /// If on a ramp cell, performs deterministic linear interpolation along the ramp axis using `RampProfile`.
    #[must_use]
    pub fn terrain_altitude_scalar(&self, pos: SimVec2) -> SimScalar {
        let Ok((cx, cy)) = self.world_to_cell(pos) else {
            return SimScalar::ZERO;
        };
        let Ok(cell) = self.cell(cx, cy) else {
            return SimScalar::ZERO;
        };
        let Some(profile) = self.ramp_profile(cx, cy) else {
            return cell.cliff_level.height_scalar();
        };

        let fx = pos.x - SimScalar::from_num(cx);
        let fy = pos.y - SimScalar::from_num(cy);
        profile.height_at(fx, fy)
    }

    /// Computes smooth visual floating-point altitude for client rendering interpolation.
    #[must_use]
    pub fn terrain_altitude_at(&self, pos: SimVec2) -> f32 {
        self.terrain_altitude_scalar(pos).to_f32()
    }

    /// Validates whether a building footprint of dimensions `w x h` can be placed at `(cell_x, cell_y)`.
    ///
    /// # Errors
    /// Returns `Err(MapError::InvalidFootprintDimensions)` if `w == 0` or `h == 0`.
    /// Returns `Err(MapError::PlacementBlocked)` if:
    /// - Footprint extends outside map bounds.
    /// - Any cell is not walkable ground (plan `2026-11-18-contour-footprints` D-5), is marked
    ///   unbuildable, or overlaps a ramp.
    /// - Any cell is already occupied by another structure.
    /// - Cells in the footprint straddle multiple cliff elevation plateaus.
    pub fn can_place_building(&self, cell_x: u32, cell_y: u32, w: u8, h: u8) -> Result<()> {
        self.can_place_building_where(cell_x, cell_y, w, h, |_, _, _| true)
    }

    /// As [`Self::can_place_building`], with `occupied` deciding whether a structure on a cell
    /// counts against the site.
    ///
    /// This is how a caller asks about a site by what one player knows rather than by the
    /// truth. Terrain is public, so the predicate can never excuse it. `occupied` receives the
    /// cell coordinates and the occupant, and is called only for cells holding a structure.
    /// A cell whose structure it ignores is judged as the bare ground the structure was placed
    /// on, which [`Self::place_structure`] proved buildable.
    ///
    /// # Errors
    /// As [`Self::can_place_building`].
    pub fn can_place_building_where(
        &self,
        cell_x: u32,
        cell_y: u32,
        w: u8,
        h: u8,
        occupied: impl Fn(u32, u32, EntityId) -> bool,
    ) -> Result<()> {
        if w == 0 || h == 0 {
            return Err(MapError::InvalidFootprintDimensions {
                width: w,
                height: h,
            });
        }

        let mut expected_cliff: Option<CliffLevel> = None;

        for dy in 0..h {
            for dx in 0..w {
                let cx = cell_x + u32::from(dx);
                let cy = cell_y + u32::from(dy);

                if cx >= self.width || cy >= self.height {
                    return Err(MapError::PlacementBlocked {
                        cell_x,
                        cell_y,
                        reason: PlacementFailureReason::OutOfBounds,
                    });
                }

                let cell = self.cell(cx, cy)?;

                // Resource nodes are public map data (plan `2026-11-20-resource-footprints` D-9):
                // no knowledge predicate excuses one.
                if let Some(node) = cell.occupying_resource {
                    return Err(MapError::PlacementBlocked {
                        cell_x,
                        cell_y,
                        reason: PlacementFailureReason::OccupiedByResource(node),
                    });
                }

                if cell.flags.contains(CellFlags::RAMP) {
                    return Err(MapError::PlacementBlocked {
                        cell_x,
                        cell_y,
                        reason: PlacementFailureReason::OverlapsRamp,
                    });
                }

                let has_structure = cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE)
                    || cell.occupying_structure.is_some();
                if has_structure {
                    let occ_id = cell.occupying_structure.unwrap_or(EntityId(0));
                    if occupied(cx, cy, occ_id) {
                        return Err(MapError::PlacementBlocked {
                            cell_x,
                            cell_y,
                            reason: PlacementFailureReason::OccupiedByStructure(occ_id),
                        });
                    }
                }

                // `place_structure` clears BUILDABLE and GROUND_PATHABLE, so the flags say nothing
                // under a structure; the ground it was placed on passed both checks.
                if !has_structure && !cell.flags.contains(CellFlags::GROUND_PATHABLE) {
                    return Err(MapError::PlacementBlocked {
                        cell_x,
                        cell_y,
                        reason: PlacementFailureReason::UnpathableTerrain,
                    });
                }
                if !has_structure && !cell.flags.contains(CellFlags::BUILDABLE) {
                    return Err(MapError::PlacementBlocked {
                        cell_x,
                        cell_y,
                        reason: PlacementFailureReason::UnbuildableTerrain,
                    });
                }

                match expected_cliff {
                    None => expected_cliff = Some(cell.cliff_level),
                    Some(expected) if expected == cell.cliff_level => {}
                    Some(_) => {
                        return Err(MapError::PlacementBlocked {
                            cell_x,
                            cell_y,
                            reason: PlacementFailureReason::CliffLevelMismatch,
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Places a building foundation dome on the map with its footprint cells at `cell` (the
    /// minimum cell) and its pathing shape, updating passability and occupancy flags.
    ///
    /// # Errors
    /// Returns `Err(MapError::InvalidFootprintShape)` if the shape does not lie in its cells, and
    /// `Err(MapError::PlacementBlocked)` if the footprint cannot be placed.
    pub fn place_structure(
        &mut self,
        id: EntityId,
        cell: (u32, u32),
        footprint: StructureFootprint,
    ) -> Result<()> {
        let ((cell_x, cell_y), w, h) = (cell, footprint.width, footprint.height);
        footprint.validate()?;
        self.can_place_building(cell_x, cell_y, w, h)?;

        let mut prior_flags = Vec::with_capacity(usize::from(w) * usize::from(h));
        for dy in 0..h {
            for dx in 0..w {
                let cx = cell_x + u32::from(dx);
                let cy = cell_y + u32::from(dy);
                let idx = (cy as usize) * (self.width as usize) + (cx as usize);
                let cell = &mut self.cells[idx];
                prior_flags.push(cell.flags);
                cell.flags.insert(CellFlags::OCCUPIED_BY_STRUCTURE);
                cell.flags
                    .remove(CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE);
                cell.occupying_structure = Some(id);
            }
        }

        self.structures.insert(
            id,
            PlacedStructure {
                id,
                top_left: (cell_x, cell_y),
                width: w,
                height: h,
                prior_flags,
                shape: footprint.shape,
            },
        );

        Ok(())
    }

    /// Removes a building foundation dome, restoring exactly the cell flags found at placement.
    ///
    /// A cell that was buildable but not walkable stays unwalkable after the building is gone.
    ///
    /// # Errors
    /// Returns `Err(MapError::StructureNotFound)` if `id` is not registered on the map.
    pub fn remove_structure(&mut self, id: EntityId) -> Result<PlacedStructure> {
        let placed = self
            .structures
            .remove(&id)
            .ok_or(MapError::StructureNotFound(id))?;

        let mut prior_flags = placed.prior_flags.iter();
        for dy in 0..placed.height {
            for dx in 0..placed.width {
                let cx = placed.top_left.0 + u32::from(dx);
                let cy = placed.top_left.1 + u32::from(dy);
                let idx = (cy as usize) * (self.width as usize) + (cx as usize);
                let cell = &mut self.cells[idx];
                if let Some(flags) = prior_flags.next() {
                    cell.flags = *flags;
                }
                cell.occupying_structure = None;
            }
        }

        Ok(placed)
    }

    /// Places every resource node's footprint, in node order, by the footprint of its kind in
    /// `table` (plan `2026-11-20-resource-footprints` D-4, D-5): each footprint cell gains
    /// `OCCUPIED_BY_RESOURCE` and the node as its occupant and loses `BUILDABLE` and
    /// `GROUND_PATHABLE`, the prior flags are kept for removal, and the node's position becomes
    /// the centre of its footprint. Placement stops at the first node that cannot be placed;
    /// nodes before it stay placed.
    ///
    /// # Errors
    /// [`MapError::ResourceFootprint`] naming the node: its kind has no footprint or an invalid
    /// shape, a cell lies beyond the map, overlaps another node or a structure, is not walkable,
    /// buildable, non-ramp ground or is on another cliff level than the anchor, or the node is
    /// already placed.
    pub fn place_resource_footprints(&mut self, table: &ResourceFootprints) -> Result<()> {
        for index in 0..self.resources.len() {
            let node = ResourceNodeId(u32::try_from(index).unwrap_or(u32::MAX));
            self.place_one_resource_footprint(node, table)?;
        }
        Ok(())
    }

    /// Places one resource node's footprint by `table`, as
    /// [`Self::place_resource_footprints`] places each: a node removed from the map, or moved
    /// while not placed, can stand again.
    ///
    /// # Errors
    /// As [`Self::place_resource_footprints`] for this node; a node index beyond the map's
    /// nodes is [`ResourceFootprintFailure::NotPlaced`].
    pub fn place_one_resource_footprint(
        &mut self,
        node: ResourceNodeId,
        table: &ResourceFootprints,
    ) -> Result<()> {
        let index = node.0 as usize;
        if index >= self.resources.len() {
            return Err(MapError::ResourceFootprint {
                node,
                reason: ResourceFootprintFailure::NotPlaced,
            });
        }
        self.place_resource_footprint(node, index, table)
            .map_err(|reason| MapError::ResourceFootprint { node, reason })
    }

    /// Places node `index` (identified as `node`) by `table`; on failure nothing changes.
    pub(crate) fn place_resource_footprint(
        &mut self,
        node: ResourceNodeId,
        index: usize,
        table: &ResourceFootprints,
    ) -> std::result::Result<(), ResourceFootprintFailure> {
        let resource = &self.resources[index];
        if resource.placed.is_some() {
            return Err(ResourceFootprintFailure::AlreadyPlaced);
        }
        let row = table
            .get(resource.kind)
            .ok_or(ResourceFootprintFailure::NoFootprint)?;
        let footprint = row.footprint();
        footprint
            .validate()
            .map_err(|_| ResourceFootprintFailure::InvalidShape)?;
        let (left, bottom) = self.resource_cells_origin(resource.cell, row.cells)?;
        let anchor_level = self
            .cell(resource.cell.0, resource.cell.1)
            .map_err(|_| ResourceFootprintFailure::OutOfBounds)?
            .cliff_level;
        let cells = Self::rectangle(left, bottom, footprint.width, footprint.height);
        for (x, y) in cells.clone() {
            let cell = &self.cells[self.index_of(x, y)];
            if let Some(other) = cell.occupying_resource {
                return Err(ResourceFootprintFailure::OverlapsNode(other));
            }
            if let Some(structure) = cell.occupying_structure {
                return Err(ResourceFootprintFailure::OverlapsStructure(structure));
            }
            let open = cell
                .flags
                .contains(CellFlags::GROUND_PATHABLE | CellFlags::BUILDABLE)
                && !cell.flags.contains(CellFlags::RAMP)
                && cell.ramp.is_none();
            if !open {
                return Err(ResourceFootprintFailure::NotOpenGround { x, y });
            }
            if cell.cliff_level != anchor_level {
                return Err(ResourceFootprintFailure::CliffLevelMismatch { x, y });
            }
        }
        let mut prior_flags = [CellFlags::empty(); 16];
        for (slot, (x, y)) in prior_flags.iter_mut().zip(cells) {
            let at = self.index_of(x, y);
            let cell = &mut self.cells[at];
            *slot = cell.flags;
            cell.flags.insert(CellFlags::OCCUPIED_BY_RESOURCE);
            cell.flags
                .remove(CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE);
            cell.occupying_resource = Some(node);
        }
        let placed = PlacedFootprint::new((left, bottom), footprint);
        let resource = &mut self.resources[index];
        resource.pos = placed.centre().to_sim();
        resource.placed = Some(PlacedResource {
            footprint: placed,
            prior_flags,
        });
        Ok(())
    }

    /// The bottom-left cell of a footprint whose cells are `(dx, dy, width, height)` from
    /// `anchor`, if the whole rectangle lies on the map.
    fn resource_cells_origin(
        &self,
        anchor: (u32, u32),
        (dx, dy, width, height): (i8, i8, u8, u8),
    ) -> std::result::Result<(u32, u32), ResourceFootprintFailure> {
        let left = i64::from(anchor.0) + i64::from(dx);
        let bottom = i64::from(anchor.1) + i64::from(dy);
        let fits = left >= 0
            && bottom >= 0
            && left + i64::from(width) <= i64::from(self.width)
            && bottom + i64::from(height) <= i64::from(self.height);
        match (u32::try_from(left), u32::try_from(bottom)) {
            (Ok(left), Ok(bottom)) if fits => Ok((left, bottom)),
            _ => Err(ResourceFootprintFailure::OutOfBounds),
        }
    }

    /// The cells of a `width` by `height` rectangle from `(left, bottom)`, row by row.
    fn rectangle(
        left: u32,
        bottom: u32,
        width: u8,
        height: u8,
    ) -> impl Iterator<Item = (u32, u32)> + Clone {
        (0..u32::from(height))
            .flat_map(move |dy| (0..u32::from(width)).map(move |dx| (left + dx, bottom + dy)))
    }

    /// The row-major index of an in-bounds cell.
    fn index_of(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.width as usize) + (x as usize)
    }

    /// Removes a standing resource node from the map (plan `2026-11-20-resource-footprints`
    /// D-8): its cells get back their prior flags and lose it as their occupant, and it is no
    /// longer placed. The node itself stays in [`Self::resources`].
    ///
    /// # Errors
    /// [`MapError::ResourceFootprint`] with [`ResourceFootprintFailure::NotPlaced`] for a node
    /// that does not stand (or does not exist), and
    /// [`ResourceFootprintFailure::UnderStructure`] while a structure stands on it.
    pub fn remove_resource_footprint(&mut self, node: ResourceNodeId) -> Result<()> {
        let fail = |reason| MapError::ResourceFootprint { node, reason };
        let placed = self
            .resources
            .get(node.0 as usize)
            .and_then(|resource| resource.placed)
            .ok_or(fail(ResourceFootprintFailure::NotPlaced))?;
        let cells = Self::rectangle(
            placed.footprint.top_left.0,
            placed.footprint.top_left.1,
            placed.footprint.width,
            placed.footprint.height,
        );
        if cells.clone().any(|(x, y)| {
            self.cells[self.index_of(x, y)]
                .occupying_structure
                .is_some()
        }) {
            return Err(fail(ResourceFootprintFailure::UnderStructure));
        }
        for (flags, (x, y)) in placed.prior_flags.iter().zip(cells) {
            let at = self.index_of(x, y);
            let cell = &mut self.cells[at];
            cell.flags = *flags;
            cell.occupying_resource = None;
        }
        if let Some(resource) = self.resources.get_mut(node.0 as usize) {
            resource.placed = None;
        }
        Ok(())
    }

    /// Places structure `id` exactly over the standing resource node `node` (plan
    /// `2026-11-20-resource-footprints` D-6: an extractor over a geyser). The cells then hold
    /// both occupants: they gain `OCCUPIED_BY_STRUCTURE` and the structure, keep the node, and
    /// the structure's prior flags are the node-occupied ones, so removing the structure leaves
    /// the node standing.
    ///
    /// # Errors
    /// [`MapError::InvalidFootprintShape`] for a shape outside its cells;
    /// [`MapError::NotOverResource`] unless the node stands and `footprint` has exactly its
    /// cells; [`MapError::PlacementBlocked`] with
    /// [`PlacementFailureReason::OccupiedByStructure`] when a structure already stands there.
    pub fn place_structure_over_resource(
        &mut self,
        id: EntityId,
        node: ResourceNodeId,
        footprint: StructureFootprint,
    ) -> Result<()> {
        footprint.validate()?;
        let not_over = MapError::NotOverResource {
            structure: id,
            node,
        };
        let placed = self
            .resources
            .get(node.0 as usize)
            .and_then(|resource| resource.placed)
            .ok_or(not_over.clone())?
            .footprint;
        if (placed.width, placed.height) != (footprint.width, footprint.height) {
            return Err(not_over);
        }
        let (left, bottom) = placed.top_left;
        let cells = Self::rectangle(left, bottom, footprint.width, footprint.height);
        for (x, y) in cells.clone() {
            if let Some(other) = self.cells[self.index_of(x, y)].occupying_structure {
                return Err(MapError::PlacementBlocked {
                    cell_x: left,
                    cell_y: bottom,
                    reason: PlacementFailureReason::OccupiedByStructure(other),
                });
            }
        }
        let mut prior_flags =
            Vec::with_capacity(usize::from(footprint.width) * usize::from(footprint.height));
        for (x, y) in cells {
            let at = self.index_of(x, y);
            let cell = &mut self.cells[at];
            prior_flags.push(cell.flags);
            cell.flags.insert(CellFlags::OCCUPIED_BY_STRUCTURE);
            cell.occupying_structure = Some(id);
        }
        self.structures.insert(
            id,
            PlacedStructure {
                id,
                top_left: (left, bottom),
                width: footprint.width,
                height: footprint.height,
                prior_flags,
                shape: footprint.shape,
            },
        );
        Ok(())
    }

    /// Updates ownership of neutral watchtowers from the ground units currently near them.
    ///
    /// `units` lists each eligible unit's owner and world position. A tower belongs to the only
    /// player with a unit inside its capture radius. It becomes neutral when two players contest
    /// it or when it is vacated, so holding a tower's vision requires staying on it.
    pub fn update_watchtower_capture(&mut self, units: &[(PlayerId, SimVec2)]) {
        for tower in &mut self.watchtowers {
            let mut occupying_player = None;
            let mut contested = false;

            for (player, unit_pos) in units {
                if tower.pos.distance_sq(*unit_pos) <= tower.capture_radius * tower.capture_radius {
                    match occupying_player {
                        None => occupying_player = Some(*player),
                        Some(p) if p == *player => {}
                        Some(_) => {
                            contested = true;
                            break;
                        }
                    }
                }
            }

            tower.controlling_player = if contested { None } else { occupying_player };
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a map that cannot be built or a cell outside it is a broken test, stopped loudly"
)]
mod tests {
    use super::{CliffLevel, GameMap, MapCell};
    use sim_types::{SimScalar, SimVec2};

    const WIDTH: u32 = 6;
    const HEIGHT: u32 = 5;

    /// The level stored in cell `(x, y)` of [`checkerboard`]: every two neighbouring cells,
    /// side or diagonal, differ, so a position read in the wrong cell fails.
    fn level_of(x: u32, y: u32) -> CliffLevel {
        CliffLevel(u8::try_from((x + 2 * y) % 4).unwrap_or(0))
    }

    /// A `WIDTH` × `HEIGHT` map whose cell `(x, y)` stores [`level_of`]`(x, y)`.
    fn checkerboard() -> GameMap {
        let mut map = GameMap::new(WIDTH, HEIGHT, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a nonzero size");
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                map.cell_mut(x, y)
                    .expect("invariant: inside the map")
                    .cliff_level = level_of(x, y);
            }
        }
        map
    }

    fn at(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    /// Plan `2026-11-23-splash-cliff-levels` P-4: a position on the map has the level of the
    /// cell it lies in, at the cell's centre and anywhere inside it.
    #[test]
    fn a_position_has_the_level_of_its_cell() {
        let map = checkerboard();
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                for (fx, fy) in [(0.5, 0.5), (0.0, 0.0), (0.999, 0.001), (0.25, 0.75)] {
                    assert_eq!(
                        map.cliff_level_at(at(f64::from(x) + fx, f64::from(y) + fy)),
                        level_of(x, y),
                        "cell ({x}, {y}) at ({fx}, {fy})"
                    );
                }
            }
        }
    }

    /// P-4: on a cell boundary the floor decides: a coordinate equal to a whole number lies in
    /// the cell that starts there, and one 2⁻³² below it in the cell before.
    #[test]
    fn a_cell_boundary_belongs_to_the_cell_the_floor_gives() {
        let map = checkerboard();
        let below = SimScalar::EPSILON;
        for x in 1..WIDTH {
            let edge = SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(2.5));
            assert_eq!(map.cliff_level_at(edge), level_of(x, 2), "x = {x}");
            let before = SimVec2::new(edge.x - below, edge.y);
            assert_eq!(map.cliff_level_at(before), level_of(x - 1, 2), "x = {x}⁻");
        }
        for y in 1..HEIGHT {
            let edge = SimVec2::new(SimScalar::from_num(3.5), SimScalar::from_num(y));
            assert_eq!(map.cliff_level_at(edge), level_of(3, y), "y = {y}");
            let before = SimVec2::new(edge.x, edge.y - below);
            assert_eq!(map.cliff_level_at(before), level_of(3, y - 1), "y = {y}⁻");
        }
    }

    /// P-4: a position off the map takes the level of the nearest border cell, on each of the
    /// four sides and beyond each of the four corners, however far off and from 2⁻³² off.
    #[test]
    fn a_position_off_the_map_takes_the_nearest_border_cell() {
        let map = checkerboard();
        let (w, h) = (f64::from(WIDTH), f64::from(HEIGHT));
        let (last_x, last_y) = (WIDTH - 1, HEIGHT - 1);
        for far in [1e-9, 0.5, 3.0, 1.0e6] {
            // The four sides, beside cell column 2 or row 1.
            assert_eq!(
                map.cliff_level_at(at(-far, 1.5)),
                level_of(0, 1),
                "west {far}"
            );
            assert_eq!(
                map.cliff_level_at(at(w + far, 1.5)),
                level_of(last_x, 1),
                "east {far}"
            );
            assert_eq!(
                map.cliff_level_at(at(2.5, -far)),
                level_of(2, 0),
                "south {far}"
            );
            assert_eq!(
                map.cliff_level_at(at(2.5, h + far)),
                level_of(2, last_y),
                "north {far}"
            );
            // The four corners.
            assert_eq!(
                map.cliff_level_at(at(-far, -far)),
                level_of(0, 0),
                "south-west {far}"
            );
            assert_eq!(
                map.cliff_level_at(at(w + far, -far)),
                level_of(last_x, 0),
                "south-east {far}"
            );
            assert_eq!(
                map.cliff_level_at(at(-far, h + far)),
                level_of(0, last_y),
                "north-west {far}"
            );
            assert_eq!(
                map.cliff_level_at(at(w + far, h + far)),
                level_of(last_x, last_y),
                "north-east {far}"
            );
        }
        // Exactly on the far edges: the coordinate equal to the width or height lies past the
        // last cell, which it takes.
        assert_eq!(
            map.cliff_level_at(at(w, 1.5)),
            level_of(last_x, 1),
            "x = width"
        );
        assert_eq!(
            map.cliff_level_at(at(2.5, h)),
            level_of(2, last_y),
            "y = height"
        );
        let extreme = SimVec2::new(SimScalar::MIN, SimScalar::MAX);
        assert_eq!(
            map.cliff_level_at(extreme),
            level_of(0, last_y),
            "the scalar bounds"
        );
    }

    /// P-4: the level depends on the position alone; a structure placed over cells, as any
    /// unit standing there, leaves it unchanged (an air unit has the level of the ground below).
    #[test]
    fn the_level_reads_the_terrain_only() {
        let mut map = GameMap::new(8, 8, MapCell::default_ground(CliffLevel(2)))
            .expect("invariant: a nonzero size");
        let before = map.cliff_level_at(at(3.5, 3.5));
        map.place_structure(
            sim_types::EntityId(7),
            (3, 3),
            crate::footprint::StructureFootprint::square(2, 2),
        )
        .expect("invariant: open buildable ground");
        assert_eq!(map.cliff_level_at(at(3.5, 3.5)), before);
        assert_eq!(map.cliff_level_at(at(4.5, 4.5)), before);
        assert_eq!(before, CliffLevel(2));
    }
}
