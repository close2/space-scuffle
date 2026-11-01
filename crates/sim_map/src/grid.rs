//! 1x1 Cell Grid, Cliff Levels, Ramps, and Building Placement.
//!
//! This module implements the discrete 1x1 tile grid foundation of Space Scuffle.
//! It models discrete cliff elevation plateaus (`0..=3`), ramp orientation,
//! cell passability and buildability flags, building footprint placement and removal,
//! and continuous altitude queries.

use crate::error::{MapError, PlacementFailureReason, Result};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2};
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
        }
    }
}

/// Type of natural resource node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    /// Standard mineral crystal cluster.
    Mineral,
    /// High-energy vespene gas geyser.
    Gas,
}

/// Natural resource node located on the map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceNode {
    /// Continuous 2D world position.
    pub pos: SimVec2,
    /// Type of resource.
    pub kind: ResourceKind,
    /// Remaining harvestable units.
    pub remaining: u32,
    /// Discrete grid cell coordinate.
    pub cell: (u32, u32),
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
        &mut self.cells
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
    /// - Any cell is marked unbuildable or overlaps a ramp.
    /// - Any cell is already occupied by another structure.
    /// - Cells in the footprint straddle multiple cliff elevation plateaus.
    pub fn can_place_building(&self, cell_x: u32, cell_y: u32, w: u8, h: u8) -> Result<()> {
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

                if cell.flags.contains(CellFlags::RAMP) {
                    return Err(MapError::PlacementBlocked {
                        cell_x,
                        cell_y,
                        reason: PlacementFailureReason::OverlapsRamp,
                    });
                }

                if cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE)
                    || cell.occupying_structure.is_some()
                {
                    let occ_id = cell.occupying_structure.unwrap_or(EntityId(0));
                    return Err(MapError::PlacementBlocked {
                        cell_x,
                        cell_y,
                        reason: PlacementFailureReason::OccupiedByStructure(occ_id),
                    });
                }

                if !cell.flags.contains(CellFlags::BUILDABLE) {
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

    /// Places a building foundation dome on the map, updating passability and occupancy flags.
    ///
    /// # Errors
    /// Returns `Err(MapError::PlacementBlocked)` if the footprint cannot be placed.
    pub fn place_structure(
        &mut self,
        id: EntityId,
        cell_x: u32,
        cell_y: u32,
        w: u8,
        h: u8,
    ) -> Result<()> {
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

    /// Constructs the canonical 64x64 two-level `StarCraft` II-style test arena.
    ///
    /// Features:
    /// - Low ground (Cliff Level 1) in the southern half.
    /// - High ground plateau (Cliff Level 2) in the northern half.
    /// - Navigable 4-cell wide ramp connecting low ground to high ground at center.
    /// - 3x3 cell smoke-vent line-of-sight blocker patch on low ground.
    /// - Neutral Watchtower at mid-ground vantage point.
    /// - Balanced resource clusters (8 mineral patches + 2 gas geysers) per base.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "one declarative map fixture; splitting it would only scatter the layout"
    )]
    pub fn default_arena() -> Self {
        let width = 64u32;
        let height = 64u32;
        let mut cells = Vec::with_capacity((width * height) as usize);

        for y in 0..height {
            for x in 0..width {
                let cell = if y < 30 {
                    // Low ground: Cliff Level 1
                    let terrain = if x < 32 {
                        TerrainKind::MetalPlate
                    } else {
                        TerrainKind::Dirt
                    };
                    MapCell {
                        cliff_level: CliffLevel(1),
                        ramp: None,
                        terrain_kind: terrain,
                        flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
                        occupying_structure: None,
                    }
                } else if y >= 34 {
                    // High ground plateau: Cliff Level 2
                    MapCell {
                        cliff_level: CliffLevel(2),
                        ramp: None,
                        terrain_kind: TerrainKind::Grass,
                        flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
                        occupying_structure: None,
                    }
                } else {
                    // Transition zone (y in 30..34)
                    if (30..34).contains(&x) {
                        // 4-cell wide ramp ascending North
                        MapCell {
                            cliff_level: CliffLevel(1),
                            ramp: Some(RampDir::North),
                            terrain_kind: TerrainKind::MetalPlate,
                            flags: CellFlags::GROUND_PATHABLE | CellFlags::RAMP,
                            occupying_structure: None,
                        }
                    } else {
                        // Impassable cliff ledge drop-off
                        MapCell {
                            cliff_level: CliffLevel(1),
                            ramp: None,
                            terrain_kind: TerrainKind::Abyss,
                            flags: CellFlags::UNPASSABLE_CLIFF,
                            occupying_structure: None,
                        }
                    }
                };
                cells.push(cell);
            }
        }

        // Apply 3x3 LoS blocker patch on low ground: x in [15..18), y in [15..18)
        for ly in 15..18 {
            for lx in 15..18 {
                let idx = (ly * width + lx) as usize;
                cells[idx].flags.insert(CellFlags::LOS_BLOCKER);
            }
        }

        // Neutral Watchtower near ramp base
        let watchtowers = vec![Watchtower {
            id: EntityId(100),
            pos: SimVec2::new(SimScalar::from_num(28.5), SimScalar::from_num(28.5)),
            capture_radius: SimScalar::from_num(3),
            reveal_radius: SimScalar::from_num(12),
            controlling_player: None,
        }];

        // Standard resource clusters: 8 minerals + 2 gas geysers per base
        let mut resources = Vec::with_capacity(20);

        // Low-ground base resource cluster around (8, 8)
        for i in 0..8 {
            let rx = 5 + (i * 2) % 6;
            let ry = 4 + (i * 2) / 6;
            resources.push(ResourceNode {
                pos: SimVec2::new(
                    SimScalar::from_num(rx) + SimScalar::HALF,
                    SimScalar::from_num(ry) + SimScalar::HALF,
                ),
                kind: ResourceKind::Mineral,
                remaining: 1500,
                cell: (rx, ry),
            });
        }
        resources.push(ResourceNode {
            pos: SimVec2::new(SimScalar::from_num(4.5), SimScalar::from_num(11.5)),
            kind: ResourceKind::Gas,
            remaining: 2500,
            cell: (4, 11),
        });
        resources.push(ResourceNode {
            pos: SimVec2::new(SimScalar::from_num(11.5), SimScalar::from_num(11.5)),
            kind: ResourceKind::Gas,
            remaining: 2500,
            cell: (11, 11),
        });

        // High-ground base resource cluster around (52, 52)
        for i in 0..8 {
            let rx = 49 + (i * 2) % 6;
            let ry = 52 + (i * 2) / 6;
            resources.push(ResourceNode {
                pos: SimVec2::new(
                    SimScalar::from_num(rx) + SimScalar::HALF,
                    SimScalar::from_num(ry) + SimScalar::HALF,
                ),
                kind: ResourceKind::Mineral,
                remaining: 1500,
                cell: (rx, ry),
            });
        }
        resources.push(ResourceNode {
            pos: SimVec2::new(SimScalar::from_num(48.5), SimScalar::from_num(56.5)),
            kind: ResourceKind::Gas,
            remaining: 2500,
            cell: (48, 56),
        });
        resources.push(ResourceNode {
            pos: SimVec2::new(SimScalar::from_num(55.5), SimScalar::from_num(56.5)),
            kind: ResourceKind::Gas,
            remaining: 2500,
            cell: (55, 56),
        });

        Self {
            width,
            height,
            cells,
            structures: BTreeMap::new(),
            resources,
            watchtowers,
        }
    }
}
