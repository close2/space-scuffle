//! Dynamic Clearance Grid, A* Corridor Search, and Simple Stupid Funnel (String-Pulling).
//!
//! This module implements dynamic, clearance-aware pathfinding for Space Scuffle.
//! It computes multi-radius unit clearance, executes A* corridor searches on the grid,
//! and applies the Simple Stupid Funnel algorithm with corner clearance offsets to
//! produce smooth, arbitrary-angle Euclidean waypoints.
//!
//! # Clearance model
//! A unit is a disc. Clearance answers one question for each cell: how large a disc can stand
//! somewhere inside this cell without overlapping a wall? Walls are blocked cells, cliff edges
//! and the map border. The answer is a Euclidean distance field, so it is symmetric: it cannot
//! depend on which side of a unit a wall lies, and mirroring a map mirrors its clearance.
//!
//! # Invariants
//! - `clearance_at` is zero exactly on cells no unit may enter.
//! - A diagonal step requires both cells it passes between to be open, for every unit size:
//!   two obstacles touching at a corner leave no gap.

use crate::error::{MapError, Result};
use crate::grid::{CellFlags, GameMap};
use crate::vision::PlayerVisionMap;
use serde::{Deserialize, Serialize};
use sim_types::{SimScalar, SimVec2, VisibilityState};

// PERFORMANCE RATIONALE:
// Why: Pathfinding is called frequently (dozens of path requests per tick during army movement).
// Allocating Open/Closed sets, priority queues, and node graphs on the heap per query creates
// severe memory fragmentation and GC/allocator pauses.
// What:
// 1. PathfinderBuffers pre-allocates flat index arrays for A* (g_score, f_score, visited, came_from).
// 2. Generation counters are used to clear visited states in O(1) without zeroing the entire buffer.
// 3. Portals and Funnel waypoints are written into a reusable pre-allocated Vec<SimVec2>.
// Trade-off: Slightly larger persistent struct memory footprint in exchange for ZERO heap allocations
// during A* search and Simple Stupid Funnel string-pulling.

/// Safety margin added to unit radius when rounding obstacle corners.
pub const CORNER_SAFETY_MARGIN: SimScalar = SimScalar::from_raw(214_748_365); // ~0.05 in Q32.32 (0.05 * 2^32)

/// Unit clearance tier determining navigation aperture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum UnitClearance {
    /// Radius 0.375. Light infantry (e.g. Marine); fits a gap one cell wide.
    Small,
    /// Radius 0.625. Standard combat units; needs a gap two cells wide.
    Medium,
    /// Radius 1.0. The largest ground units; fits exactly in a gap two cells wide.
    Large,
}

impl UnitClearance {
    /// Returns the continuous collision radius in world units.
    #[must_use]
    #[inline]
    pub fn radius_scalar(self) -> SimScalar {
        match self {
            Self::Small => SimScalar::from_num(0.375),
            Self::Medium => SimScalar::from_num(0.625),
            Self::Large => SimScalar::ONE,
        }
    }

    /// Selects the tier that routes a disc of `radius` world units.
    ///
    /// A gap is a whole number of cells wide, so the only width that separates the tiers below
    /// [`Self::Large`] is one cell: any disc of radius 0.5 or less fits through it. Routing such a
    /// unit as [`Self::Medium`] would forbid gaps that its collision body passes.
    #[must_use]
    pub fn for_radius(radius: SimScalar) -> Self {
        if radius <= SimScalar::HALF {
            Self::Small
        } else if radius <= Self::MEDIUM_TIER_LIMIT {
            Self::Medium
        } else {
            Self::Large
        }
    }

    /// Largest radius [`Self::Medium`] routes, 0.875. It and [`Self::Large`] both need a gap two
    /// cells wide, so this boundary only decides how far routes keep from corners.
    const MEDIUM_TIER_LIMIT: SimScalar = SimScalar::from_raw(7 << 29);

    /// Returns the clearance this tier needs, in eighths of a world unit, rounded up.
    ///
    /// Compare against [`Pathfinder::clearance_at`].
    #[must_use]
    #[inline]
    pub fn required_clearance(self) -> u8 {
        match self {
            Self::Small => 3,
            Self::Medium => 5,
            Self::Large => 8,
        }
    }
}

/// Request parameters for a pathfinding query.
#[derive(Debug, Clone, Copy)]
pub struct PathRequest {
    /// Starting position in continuous world space.
    pub start: SimVec2,
    /// Goal position in continuous world space.
    pub goal: SimVec2,
    /// Unit clearance tier.
    pub clearance: UnitClearance,
}

/// Extended pathfinding request supporting optional player fog-of-war perception.
#[derive(Debug, Clone, Copy)]
pub struct PathRequestWithVision<'a> {
    /// Starting continuous world position.
    pub start: SimVec2,
    /// Destination continuous world position.
    pub goal: SimVec2,
    /// Unit clearance tier (Small, Medium, Large).
    pub clearance: UnitClearance,
    /// Optional player vision map. If `Some`, unexplored ramps and fogged obstacles
    /// are evaluated through the player's knowledge.
    pub vision: Option<&'a PlayerVisionMap>,
    /// If true, ramps or higher-ground cells that are `VisibilityState::Unexplored` cannot be traversed.
    pub block_unexplored_ramps: bool,
    /// Optional mask of cells known by the player to contain structures.
    pub known_structures: Option<&'a [bool]>,
}

impl From<PathRequest> for PathRequestWithVision<'_> {
    fn from(r: PathRequest) -> Self {
        Self {
            start: r.start,
            goal: r.goal,
            clearance: r.clearance,
            vision: None,
            block_unexplored_ramps: false,
            known_structures: None,
        }
    }
}

/// Resulting path consisting of smooth, corner-rounded waypoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathResult {
    /// Ordered Euclidean waypoints from start to goal.
    pub waypoints: Vec<SimVec2>,
    /// Total Euclidean length of the string-pulled path.
    pub total_length: SimScalar,
}

/// A 2D portal edge connecting two adjacent passable cells in the A* corridor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortalEdge {
    /// Left vertex when looking through the portal in travel direction.
    pub left: SimVec2,
    /// Right vertex when looking through the portal in travel direction.
    pub right: SimVec2,
}

/// Sentinel value indicating a cell is not currently in the open min-heap.
pub const NO_HEAP_POS: u32 = u32::MAX;

/// Reusable internal buffers to eliminate heap allocations per query.
pub struct PathfinderBuffers {
    open_heap: Vec<u32>,
    heap_pos: Vec<u32>,
    came_from: Vec<u32>,
    g_score: Vec<u32>,
    f_score: Vec<u32>,
    visited_generation: Vec<u32>,
    current_generation: u32,
    corridor_cells: Vec<u32>,
    /// Start cell, corridor cells, then goal cell, as `(x, y)`.
    corridor_nodes: Vec<(u32, u32)>,
    /// Entry `i` counts ramp transitions among `corridor_nodes[..i]`.
    ramp_transitions_before: Vec<u32>,
    portals: Vec<PortalEdge>,
    waypoints_scratch: Vec<SimVec2>,
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "heap positions index the cell grid, whose indices are stored as u32 by design"
)]
impl PathfinderBuffers {
    fn new(size: usize) -> Self {
        Self {
            open_heap: Vec::with_capacity(size.min(1024)),
            heap_pos: vec![NO_HEAP_POS; size],
            came_from: vec![0; size],
            g_score: vec![u32::MAX; size],
            f_score: vec![u32::MAX; size],
            visited_generation: vec![0; size],
            current_generation: 0,
            corridor_cells: Vec::with_capacity(256),
            corridor_nodes: Vec::with_capacity(256),
            ramp_transitions_before: Vec::with_capacity(256),
            portals: Vec::with_capacity(256),
            waypoints_scratch: Vec::with_capacity(64),
        }
    }

    #[inline]
    fn node_less_than(&self, a: usize, b: usize) -> bool {
        let fa = self.f_score[a];
        let fb = self.f_score[b];
        if fa != fb {
            return fa < fb;
        }
        let ga = self.g_score[a];
        let gb = self.g_score[b];
        if ga != gb {
            return ga > gb;
        }
        a < b
    }

    fn sift_up(&mut self, mut i: usize) {
        while i > 0 {
            let parent = (i - 1) / 2;
            let node_idx = self.open_heap[i] as usize;
            let parent_idx = self.open_heap[parent] as usize;
            if self.node_less_than(node_idx, parent_idx) {
                self.open_heap.swap(i, parent);
                self.heap_pos[node_idx] = parent as u32;
                self.heap_pos[parent_idx] = i as u32;
                i = parent;
            } else {
                break;
            }
        }
    }

    fn sift_down(&mut self, mut i: usize) {
        let len = self.open_heap.len();
        loop {
            let left = 2 * i + 1;
            let right = 2 * i + 2;
            let mut smallest = i;

            if left < len {
                let left_node = self.open_heap[left] as usize;
                let smallest_node = self.open_heap[smallest] as usize;
                if self.node_less_than(left_node, smallest_node) {
                    smallest = left;
                }
            }
            if right < len {
                let right_node = self.open_heap[right] as usize;
                let smallest_node = self.open_heap[smallest] as usize;
                if self.node_less_than(right_node, smallest_node) {
                    smallest = right;
                }
            }

            if smallest == i {
                break;
            }
            let node = self.open_heap[i] as usize;
            let smallest_node = self.open_heap[smallest] as usize;
            self.open_heap.swap(i, smallest);
            self.heap_pos[node] = smallest as u32;
            self.heap_pos[smallest_node] = i as u32;
            i = smallest;
        }
    }

    #[inline]
    fn push_or_decrease_key(&mut self, idx: u32) {
        let pos = self.heap_pos[idx as usize];
        if pos == NO_HEAP_POS {
            let i = self.open_heap.len();
            self.open_heap.push(idx);
            self.heap_pos[idx as usize] = i as u32;
            self.sift_up(i);
        } else {
            self.sift_up(pos as usize);
        }
    }

    #[inline]
    fn push_open(&mut self, idx: u32) {
        self.push_or_decrease_key(idx);
    }

    fn pop_open(&mut self) -> Option<u32> {
        if self.open_heap.is_empty() {
            return None;
        }
        let root = self.open_heap[0];
        self.heap_pos[root as usize] = NO_HEAP_POS;
        let last = self.open_heap.pop()?;
        if !self.open_heap.is_empty() {
            self.open_heap[0] = last;
            self.heap_pos[last as usize] = 0;
            self.sift_down(0);
        }
        Some(root)
    }

    fn clear_open(&mut self) {
        for &idx in &self.open_heap {
            self.heap_pos[idx as usize] = NO_HEAP_POS;
        }
        self.open_heap.clear();
    }
}

/// Converts a cell coordinate for signed grid arithmetic.
///
/// Saturates, which stays correct: `i32::MAX` is off every map, like the coordinate it replaces.
#[inline]
fn signed(coordinate: u32) -> i32 {
    i32::try_from(coordinate).unwrap_or(i32::MAX)
}

/// Converts a signed grid coordinate back to a cell coordinate; negatives map off every map.
#[inline]
fn unsigned(coordinate: i32) -> u32 {
    u32::try_from(coordinate).unwrap_or(u32::MAX)
}

/// Reusable storage for the clearance distance transform.
///
/// Works on a lattice with a point every half cell, `2w + 1` by `2h + 1` points. Lattice
/// coordinates are therefore half-cell units, and `(2x, 2y)` is the corner of cell `(x, y)`.
struct ClearanceWorkspace {
    lattice_width: usize,
    lattice_height: usize,
    /// Lattice points lying on a wall.
    walls: Vec<bool>,
    /// Squared distance to the nearest wall, in squared half-cell units.
    squared_distance: Vec<u32>,
    /// One-dimensional distance to the nearest wall along each column.
    column_distance: Vec<u32>,
    /// Column index of each parabola in the row's lower envelope.
    envelope_column: Vec<usize>,
    /// First row position at which each parabola is the minimum.
    envelope_start: Vec<usize>,
}

impl ClearanceWorkspace {
    fn new(width: usize, height: usize) -> Self {
        let lattice_width = 2 * width + 1;
        let lattice_height = 2 * height + 1;
        let points = lattice_width * lattice_height;
        Self {
            lattice_width,
            lattice_height,
            walls: vec![false; points],
            squared_distance: vec![0; points],
            column_distance: vec![0; points],
            envelope_column: vec![0; lattice_width],
            envelope_start: vec![0; lattice_width],
        }
    }

    fn mark_wall(&mut self, x: usize, y: usize) {
        self.walls[y * self.lattice_width + x] = true;
    }

    /// Marks the map border and every wall that belongs to terrain.
    fn mark_terrain_walls(&mut self, map: &GameMap) {
        self.walls.fill(false);
        for x in 0..self.lattice_width {
            self.mark_wall(x, 0);
            self.mark_wall(x, self.lattice_height - 1);
        }
        for y in 0..self.lattice_height {
            self.mark_wall(0, y);
            self.mark_wall(self.lattice_width - 1, y);
        }
        let (width, height) = (map.width(), map.height());
        for y in 0..height {
            for x in 0..width {
                let cell = (x, y);
                if !map.terrain_step_is_connected(cell, cell) {
                    self.mark_cell_wall(x as usize, y as usize);
                    continue;
                }
                let east = (x + 1, y);
                if x + 1 < width
                    && map.terrain_step_is_connected(east, east)
                    && !map.terrain_step_is_connected(cell, east)
                {
                    let edge = 2 * x as usize + 2;
                    for offset in 0..=2 {
                        self.mark_wall(edge, 2 * y as usize + offset);
                    }
                }
                let north = (x, y + 1);
                if y + 1 < height
                    && map.terrain_step_is_connected(north, north)
                    && !map.terrain_step_is_connected(cell, north)
                {
                    let edge = 2 * y as usize + 2;
                    for offset in 0..=2 {
                        self.mark_wall(2 * x as usize + offset, edge);
                    }
                }
            }
        }
    }

    fn mark_cell_wall(&mut self, x: usize, y: usize) {
        for offset_y in 0..=2 {
            for offset_x in 0..=2 {
                self.mark_wall(2 * x + offset_x, 2 * y + offset_y);
            }
        }
    }

    /// Adds every structure footprint cell to the walls. Returns whether there were any.
    fn mark_structure_walls(&mut self, map: &GameMap) -> bool {
        let mut found = false;
        let width = map.width() as usize;
        for (index, cell) in map.cells().iter().enumerate() {
            if cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE) {
                self.mark_cell_wall(index % width, index / width);
                found = true;
            }
        }
        found
    }

    /// Computes the exact squared Euclidean distance from every lattice point to a wall.
    // PERFORMANCE RATIONALE:
    // Why: Clearance is recomputed for the whole map whenever a structure changes, and a naive
    // nearest-wall search visits O(points * walls) pairs.
    // What: The two-pass transform of Meijster et al. runs in O(points). The first pass finds
    // the nearest wall along each column; the second takes the lower envelope of the resulting
    // parabolas along each row. All arithmetic is integer, so the result is deterministic.
    // Trade-off: Retained scratch buffers and an algorithm that needs explaining, in exchange
    // for an exact field with no heap allocation per recomputation.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "lattice coordinates are at most twice a map side, far below i64::MAX, and \
                  `first` is past a valid start and below `columns` where it is converted"
    )]
    fn run_distance_transform(&mut self) {
        let (columns, rows) = (self.lattice_width, self.lattice_height);
        // The border is always a wall, so every distance below is finite.
        for x in 0..columns {
            let mut distance = u32::MAX;
            for y in 0..rows {
                distance = if self.walls[y * columns + x] {
                    0
                } else {
                    distance.saturating_add(1)
                };
                self.column_distance[y * columns + x] = distance;
            }
            for y in (0..rows - 1).rev() {
                let above = self.column_distance[(y + 1) * columns + x].saturating_add(1);
                let here = &mut self.column_distance[y * columns + x];
                *here = (*here).min(above);
            }
        }

        for y in 0..rows {
            let row = y * columns;
            let height = |x: usize| i64::from(self.column_distance[row + x]);
            let parabola = |at: usize, x: usize| {
                let across = at as i64 - x as i64;
                across * across + height(x) * height(x)
            };
            let crossing = |left: usize, right: usize| {
                let (l, r) = (left as i64, right as i64);
                (r * r - l * l + height(right) * height(right) - height(left) * height(left))
                    .div_euclid(2 * (r - l))
            };

            let mut top = 0_usize;
            self.envelope_column[0] = 0;
            self.envelope_start[0] = 0;
            for x in 1..columns {
                loop {
                    let start = self.envelope_start[top];
                    if parabola(start, self.envelope_column[top]) <= parabola(start, x) {
                        let first = crossing(self.envelope_column[top], x) + 1;
                        if first < columns as i64 {
                            top += 1;
                            self.envelope_column[top] = x;
                            self.envelope_start[top] = first as usize;
                        }
                        break;
                    }
                    if top == 0 {
                        self.envelope_column[0] = x;
                        break;
                    }
                    top -= 1;
                }
            }
            for x in (0..columns).rev() {
                let squared = parabola(x, self.envelope_column[top]);
                self.squared_distance[row + x] = u32::try_from(squared).unwrap_or(u32::MAX);
                if top > 0 && x == self.envelope_start[top] {
                    top -= 1;
                }
            }
        }
    }

    /// Writes the clearance of each cell: the best clearance at any lattice point in the cell.
    fn write_cell_clearance(&self, grid: &mut [u8], width: usize) {
        for (index, clearance) in grid.iter_mut().enumerate() {
            let (x, y) = (index % width, index / width);
            let mut best = 0;
            for offset_y in 0..=2 {
                for offset_x in 0..=2 {
                    let point = (2 * y + offset_y) * self.lattice_width + 2 * x + offset_x;
                    best = best.max(self.squared_distance[point]);
                }
            }
            // Distance in eighths of a cell is 4 * sqrt(best), since `best` is in half cells.
            *clearance = u8::try_from((16 * u64::from(best)).isqrt()).unwrap_or(u8::MAX);
        }
    }
}

/// Dynamic pathfinder operating over `GameMap`.
pub struct Pathfinder {
    width: u32,
    height: u32,
    /// Clearance per cell in eighths of a world unit, counting every structure as a wall.
    clearance_grid: Vec<u8>,
    /// Clearance per cell in eighths of a world unit, ignoring structures.
    static_clearance_grid: Vec<u8>,
    clearance_workspace: ClearanceWorkspace,
    /// Incremented whenever obstacles change, invalidating cached paths.
    nav_version: u32,
    buffers: PathfinderBuffers,
}

impl Pathfinder {
    /// Constructs a pathfinder for `map` and computes initial clearance.
    #[must_use]
    pub fn new(map: &GameMap) -> Self {
        let width = map.width();
        let height = map.height();
        let size = (width as usize) * (height as usize);
        let mut pathfinder = Self {
            width,
            height,
            clearance_grid: vec![0; size],
            static_clearance_grid: vec![0; size],
            clearance_workspace: ClearanceWorkspace::new(width as usize, height as usize),
            nav_version: 1,
            buffers: PathfinderBuffers::new(size),
        };
        pathfinder.recompute_clearance(map);
        pathfinder
    }

    /// Navigation version incremented on clearance changes.
    #[must_use]
    #[inline]
    pub fn nav_version(&self) -> u32 {
        self.nav_version
    }

    /// Radius of the largest disc that fits somewhere in cell `(x, y)`, counting structures.
    ///
    /// Measured in eighths of a world unit, rounded down and saturating at 255 (31.875 units).
    /// Zero means no unit can enter the cell. Out-of-bounds cells report zero.
    #[must_use]
    pub fn clearance_at(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            0
        } else {
            self.clearance_grid[(y as usize) * (self.width as usize) + (x as usize)]
        }
    }

    /// Same as [`Self::clearance_at`] but ignoring structures, in eighths of a world unit.
    #[must_use]
    pub fn static_clearance_at(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            0
        } else {
            self.static_clearance_grid[(y as usize) * (self.width as usize) + (x as usize)]
        }
    }

    /// Recomputes both clearance grids across the entire map.
    pub fn recompute_clearance(&mut self, map: &GameMap) {
        let width = self.width as usize;
        let workspace = &mut self.clearance_workspace;

        workspace.mark_terrain_walls(map);
        workspace.run_distance_transform();
        workspace.write_cell_clearance(&mut self.static_clearance_grid, width);

        if workspace.mark_structure_walls(map) {
            workspace.run_distance_transform();
            workspace.write_cell_clearance(&mut self.clearance_grid, width);
        } else {
            self.clearance_grid
                .copy_from_slice(&self.static_clearance_grid);
        }

        self.nav_version = self.nav_version.wrapping_add(1);
    }

    /// Finds a smooth, corner-rounded shortest path using A* and Simple Stupid Funnel.
    ///
    /// # Errors
    /// Returns `Err(MapError::InvalidStart)` or `Err(MapError::InvalidGoal)` if endpoints are impassable.
    /// Returns `Err(MapError::NoPathFound)` if start and goal are disconnected.
    pub fn find_path(&mut self, map: &GameMap, request: &PathRequest) -> Result<PathResult> {
        self.find_path_with_vision(map, &(*request).into())
    }

    /// Computes a smooth shortest path taking player vision and fog-of-war into account.
    ///
    /// # Invariants
    /// 1. If `request.vision` is `Some` and `block_unexplored_ramps == true`, cells with `CellFlags::RAMP`
    ///    or high-ground (`cliff_level > start_cliff`) are passable only if
    ///    `vision.visibility_at(x, y) != VisibilityState::Unexplored`.
    /// 2. If a cell contains `CellFlags::OCCUPIED_BY_STRUCTURE`, but the cell in `vision` is
    ///    `VisibilityState::Unexplored` or `VisibilityState::Fogged` (and not in `known_structures`),
    ///    the pathfinder treats the cell as open until revealed!
    ///
    /// # Errors
    /// Returns [`MapError::NoPathFound`] if no valid route connects the start and goal cells.
    #[expect(
        clippy::too_many_lines,
        reason = "one search loop whose steps share local state; splitting is plan step 9.2"
    )]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "cell indices are stored as u32 by design, and start was validated on the map"
    )]
    pub fn find_path_with_vision(
        &mut self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
    ) -> Result<PathResult> {
        let (start_cx, start_cy) = map
            .world_to_cell(request.start)
            .map_err(|_| MapError::InvalidStart(request.start))?;
        let (goal_cx, goal_cy) = map
            .world_to_cell(request.goal)
            .map_err(|_| MapError::InvalidGoal(request.goal))?;

        let req_clearance = request.clearance.required_clearance();

        let start_cell = map
            .cell(start_cx, start_cy)
            .map_err(|_| MapError::InvalidStart(request.start))?;
        let goal_cell = map
            .cell(goal_cx, goal_cy)
            .map_err(|_| MapError::InvalidGoal(request.goal))?;
        let start_cliff = start_cell.cliff_level;

        let is_structure_known = |c_idx: usize, cx: u32, cy: u32| -> bool {
            let Ok(c) = map.cell(cx, cy) else {
                return true;
            };
            if !c.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE) {
                return false;
            }
            if let Some(mask) = request.known_structures {
                mask.get(c_idx).copied().unwrap_or(false)
            } else if let Some(vis) = request.vision {
                vis.visibility_at(cx, cy) == VisibilityState::Visible
            } else {
                true
            }
        };

        let start_idx = (start_cy as usize) * (self.width as usize) + (start_cx as usize);
        let goal_idx = (goal_cy as usize) * (self.width as usize) + (goal_cx as usize);

        let start_terrain_ok =
            map.terrain_step_is_connected((start_cx, start_cy), (start_cx, start_cy));
        if !start_terrain_ok || is_structure_known(start_idx, start_cx, start_cy) {
            return Err(MapError::InvalidStart(request.start));
        }

        let goal_terrain_ok = map.terrain_step_is_connected((goal_cx, goal_cy), (goal_cx, goal_cy));
        if !goal_terrain_ok || is_structure_known(goal_idx, goal_cx, goal_cy) {
            return Err(MapError::InvalidGoal(request.goal));
        }

        if request.block_unexplored_ramps {
            if let Some(vis) = request.vision {
                if vis.visibility_at(goal_cx, goal_cy) == VisibilityState::Unexplored
                    && (goal_cell.flags.contains(CellFlags::RAMP)
                        || goal_cell.cliff_level > start_cliff)
                {
                    return Err(MapError::NoPathFound {
                        start: request.start,
                        goal: request.goal,
                    });
                }
            }
        }

        let clearance_table = if request.vision.is_some() || request.known_structures.is_some() {
            &self.static_clearance_grid
        } else {
            &self.clearance_grid
        };

        if clearance_table[start_idx] < req_clearance {
            return Err(MapError::InvalidStart(request.start));
        }
        if clearance_table[goal_idx] < req_clearance {
            return Err(MapError::InvalidGoal(request.goal));
        }

        if !self.segment_is_passable(map, request, request.start, request.start) {
            return Err(MapError::InvalidStart(request.start));
        }
        if !self.segment_is_passable(map, request, request.goal, request.goal) {
            return Err(MapError::InvalidGoal(request.goal));
        }

        if start_cx == goal_cx && start_cy == goal_cy {
            return Ok(PathResult {
                waypoints: vec![request.start, request.goal],
                total_length: request.start.distance(request.goal),
            });
        }

        if self.segment_is_passable(map, request, request.start, request.goal) {
            // PERFORMANCE RATIONALE:
            // Why: An army crossing open ground issues hundreds of searches whose funnel
            // collapses to the straight line, each visiting and portalling every corridor cell.
            // What: Test the straight line first with the predicate that validates routes.
            // Trade-off: A failed test adds one line walk before the search, which is small
            // next to the search it precedes.
            return Ok(PathResult {
                waypoints: vec![request.start, request.goal],
                total_length: request.start.distance(request.goal),
            });
        }

        let found_path = self.search_corridor(map, request, start_idx, (goal_cx, goal_cy));

        if !found_path {
            return Err(MapError::NoPathFound {
                start: request.start,
                goal: request.goal,
            });
        }

        // Reconstruct corridor of cells
        self.buffers.corridor_cells.clear();
        let mut curr = goal_idx as u32;
        self.buffers.corridor_cells.push(curr);
        while curr != start_idx as u32 {
            curr = self.buffers.came_from[curr as usize];
            self.buffers.corridor_cells.push(curr);
        }
        self.buffers.corridor_cells.reverse();

        // Build portals from corridor
        self.build_portals(map, request);

        // Run Simple Stupid Funnel string-pulling
        let mut waypoints = self.run_funnel(map, request);
        if !waypoints
            .windows(2)
            .all(|pair| self.segment_is_passable(map, request, pair[0], pair[1]))
        {
            waypoints = self.smooth_connected_corridor(map, request)?;
        }

        let mut total_length = SimScalar::ZERO;
        for w in waypoints.windows(2) {
            total_length += w[0].distance(w[1]);
        }

        Ok(PathResult {
            waypoints,
            total_length,
        })
    }

    /// Runs A* from `start_idx` toward `goal`, leaving every cell it reached marked visited.
    ///
    /// Returns whether the goal cell was reached. On `false` the search has exhausted the region
    /// connected to the start, which is what [`Self::nearest_reachable_point`] relies on.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "cell indices are stored as u32 by design, and start was validated on the map"
    )]
    fn search_corridor(
        &mut self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
        start_idx: usize,
        (goal_cx, goal_cy): (u32, u32),
    ) -> bool {
        let goal_idx = (goal_cy as usize) * (self.width as usize) + (goal_cx as usize);
        let start_cx = start_idx as u32 % self.width;
        let start_cy = start_idx as u32 / self.width;
        let req_clearance = request.clearance.required_clearance();
        let start_cliff = map
            .cell(start_cx, start_cy)
            .map_or(crate::grid::CliffLevel(0), |cell| cell.cliff_level);
        let clearance_table = if request.vision.is_some() || request.known_structures.is_some() {
            &self.static_clearance_grid
        } else {
            &self.clearance_grid
        };

        // Initialize A* search state
        self.buffers.current_generation = self.buffers.current_generation.wrapping_add(1);
        if self.buffers.current_generation == 0 {
            self.buffers.visited_generation.fill(0);
            self.buffers.current_generation = 1;
        }

        self.buffers.clear_open();
        self.buffers.g_score[start_idx] = 0;
        let h_start = octile_heuristic(start_cx, start_cy, goal_cx, goal_cy);
        self.buffers.f_score[start_idx] = h_start;
        self.buffers.visited_generation[start_idx] = self.buffers.current_generation;
        self.buffers.push_open(start_idx as u32);

        let mut found_path = false;

        while let Some(current_u32) = self.buffers.pop_open() {
            let current_idx = current_u32 as usize;
            if current_idx == goal_idx {
                found_path = true;
                break;
            }

            let cx = current_u32 % self.width;
            let cy = current_u32 / self.width;
            let current_g = self.buffers.g_score[current_idx];

            // Explore 8 neighbors
            for dy in -1i32..=1i32 {
                for dx in -1i32..=1i32 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }

                    let Some((nx_u32, ny_u32)) = map.offset_cell((cx, cy), (dx, dy)) else {
                        continue;
                    };
                    let n_idx = (ny_u32 as usize) * (self.width as usize) + (nx_u32 as usize);

                    if clearance_table[n_idx] < req_clearance {
                        continue;
                    }

                    if !body_fits(&self.clearance_grid, map, request, nx_u32, ny_u32) {
                        continue;
                    }

                    let Ok(n_cell) = map.cell(nx_u32, ny_u32) else {
                        continue;
                    };

                    if request.block_unexplored_ramps {
                        if let Some(vis) = request.vision {
                            if vis.visibility_at(nx_u32, ny_u32) == VisibilityState::Unexplored
                                && (n_cell.flags.contains(CellFlags::RAMP)
                                    || n_cell.cliff_level > start_cliff)
                            {
                                continue;
                            }
                        }
                    }

                    if !map.terrain_step_is_connected((cx, cy), (nx_u32, ny_u32)) {
                        continue;
                    }

                    let is_diag = dx != 0 && dy != 0;

                    // Terrain topology ignores occupancy, so structures are checked here: two
                    // buildings touching at a corner leave no gap for a unit of any size.
                    if is_diag
                        && (!is_cell_passable_for_request(map, request, signed(nx_u32), signed(cy))
                            || !is_cell_passable_for_request(
                                map,
                                request,
                                signed(cx),
                                signed(ny_u32),
                            ))
                    {
                        continue;
                    }

                    let step_cost = if is_diag { 14 } else { 10 };
                    let tentative_g = current_g.saturating_add(step_cost);

                    let is_visited =
                        self.buffers.visited_generation[n_idx] == self.buffers.current_generation;
                    if !is_visited || tentative_g < self.buffers.g_score[n_idx] {
                        self.buffers.came_from[n_idx] = current_u32;
                        self.buffers.g_score[n_idx] = tentative_g;
                        let h = octile_heuristic(nx_u32, ny_u32, goal_cx, goal_cy);
                        self.buffers.f_score[n_idx] = tentative_g.saturating_add(h);
                        self.buffers.visited_generation[n_idx] = self.buffers.current_generation;
                        self.buffers.push_open(n_idx as u32);
                    }
                }
            }
        }

        self.buffers.clear_open();
        found_path
    }

    /// Finds the world point closest to `request.goal` that this unit can actually reach.
    ///
    /// A blocked goal, or one cut off from the start, is the normal case for a click on a cliff
    /// or behind a wall. The answer is the centre of the reachable cell nearest the goal, with ties
    /// going to the lowest cell index, so every unit in one connected region gets the same point.
    /// Returns `Ok(None)` when the unit already stands in that cell.
    ///
    /// # Errors
    /// Returns [`MapError::InvalidStart`] if the unit cannot path from where it stands, including
    /// when `request.start` lacks this tier's clearance.
    // PERFORMANCE RATIONALE:
    // Why: The answer needs the whole connected region, and a group asks the same question.
    // What: The search that fails has already marked that region; this scans the marks once.
    // Trade-off: O(map area) per call, so callers must cache per goal and throttle by unit.
    pub fn nearest_reachable_point(
        &mut self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
    ) -> Result<Option<SimVec2>> {
        let invalid_start = || MapError::InvalidStart(request.start);
        let (start_cx, start_cy) = map
            .world_to_cell(request.start)
            .map_err(|_| invalid_start())?;
        let start_idx = (start_cy as usize) * (self.width as usize) + (start_cx as usize);
        let clearance_table = if request.vision.is_some() || request.known_structures.is_some() {
            &self.static_clearance_grid
        } else {
            &self.clearance_grid
        };
        if clearance_table[start_idx] < request.clearance.required_clearance() {
            return Err(invalid_start());
        }
        let goal = map
            .world_to_cell(request.goal)
            .map_err(|_| MapError::InvalidGoal(request.goal))?;
        self.search_corridor(map, request, start_idx, goal);

        let generation = self.buffers.current_generation;
        let mut best: Option<(u64, u32)> = None;
        for (index, &mark) in self.buffers.visited_generation.iter().enumerate() {
            if mark != generation {
                continue;
            }
            let Ok(index) = u32::try_from(index) else {
                break;
            };
            let dx = u64::from((index % self.width).abs_diff(goal.0));
            let dy = u64::from((index / self.width).abs_diff(goal.1));
            let distance_sq = dx * dx + dy * dy;
            if best.is_none_or(|(closest, _)| distance_sq < closest) {
                best = Some((distance_sq, index));
            }
        }
        best.filter(|&(_, index)| index as usize != start_idx)
            .map(|(_, index)| map.cell_to_world_center(index % self.width, index / self.width))
            .transpose()
    }

    /// Validates a continuous world-space segment using this request's current
    /// ground clearance, vision and remembered structure policy without allocating.
    /// This is the same predicate used to verify and smooth generated routes.
    #[must_use]
    pub fn segment_is_passable(
        &self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
        start: SimVec2,
        goal: SimVec2,
    ) -> bool {
        let required = request.clearance.required_clearance();
        let clearance = if request.vision.is_some() || request.known_structures.is_some() {
            &self.static_clearance_grid
        } else {
            &self.clearance_grid
        };
        map.ground_segment_is_connected(start, goal, |previous, next| {
            let index = (next.1 as usize) * (self.width as usize) + next.0 as usize;
            if clearance[index] < required {
                return false;
            }
            if !body_fits(&self.clearance_grid, map, request, next.0, next.1) {
                return false;
            }
            if previous.0 != next.0 && previous.1 != next.1 {
                return is_cell_passable_for_request(
                    map,
                    request,
                    signed(previous.0),
                    signed(next.1),
                ) && is_cell_passable_for_request(
                    map,
                    request,
                    signed(next.0),
                    signed(previous.1),
                );
            }
            true
        })
    }

    /// Records each corridor node's cell and where ramp transitions fall along the corridor.
    ///
    /// Nodes are the start cell, every corridor cell, then the goal cell. A node is a ramp
    /// transition when it differs from either neighbour in being on a ramp.
    // PERFORMANCE RATIONALE:
    // Why: Smoothing asks, for every candidate shortcut, whether it skips a ramp transition.
    // Rescanning the skipped nodes made each anchor quadratic in corridor length.
    // What: A prefix count of transitions answers that question for any node range in O(1).
    // Trade-off: Two reusable buffers filled once per fallback, in exchange for a smoothing
    // pass whose cost no longer depends on how many nodes each shortcut skips.
    fn index_corridor(&mut self, map: &GameMap, request: &PathRequestWithVision<'_>) -> Result<()> {
        let start = map.world_to_cell(request.start)?;
        let goal = map.world_to_cell(request.goal)?;
        let width = self.width;
        let buffers = &mut self.buffers;

        buffers.corridor_nodes.clear();
        buffers.corridor_nodes.push(start);
        buffers.corridor_nodes.extend(
            buffers
                .corridor_cells
                .iter()
                .map(|cell| (cell % width, cell / width)),
        );
        buffers.corridor_nodes.push(goal);

        let nodes = &buffers.corridor_nodes;
        let is_ramp = |index: usize| {
            let (x, y) = nodes[index];
            map.cell(x, y)
                .is_ok_and(|cell| cell.flags.contains(CellFlags::RAMP))
        };
        buffers.ramp_transitions_before.clear();
        buffers.ramp_transitions_before.push(0);
        let mut transitions = 0;
        for index in 0..nodes.len() {
            let interior = index > 0 && index + 1 < nodes.len();
            if interior
                && (is_ramp(index) != is_ramp(index - 1) || is_ramp(index) != is_ramp(index + 1))
            {
                transitions += 1;
            }
            buffers.ramp_transitions_before.push(transitions);
        }
        Ok(())
    }

    /// Constrains string-pulling to visible points of the verified A* corridor.
    ///
    /// Funnel corner offsets and widened portals can leave the original corridor.
    /// When their segments are invalid, greedily retain the furthest visible corridor
    /// center. Every retained edge is tested against the complete request policy, and a
    /// shortcut may only cut across a ramp transition in a straight grid line, so it cannot
    /// slip off a ramp's side.
    fn smooth_connected_corridor(
        &mut self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
    ) -> Result<Vec<SimVec2>> {
        self.index_corridor(map, request)?;
        self.buffers.waypoints_scratch.clear();
        self.buffers.waypoints_scratch.push(request.start);
        let end_index = self.buffers.corridor_nodes.len() - 1;
        let point = |index: usize| -> Result<SimVec2> {
            if index == 0 {
                Ok(request.start)
            } else if index == end_index {
                Ok(request.goal)
            } else {
                let (x, y) = self.buffers.corridor_nodes[index];
                map.cell_to_world_center(x, y)
            }
        };
        let mut current_index = 0;
        while current_index < end_index {
            let current = point(current_index)?;
            let anchor = self.buffers.corridor_nodes[current_index];
            let mut visible = None;
            for candidate in ((current_index + 1)..=end_index).rev() {
                let target = self.buffers.corridor_nodes[candidate];
                let skipped_transitions = candidate > current_index + 1
                    && self.buffers.ramp_transitions_before[candidate]
                        > self.buffers.ramp_transitions_before[current_index + 1];
                let collinear = anchor.0 == target.0 || anchor.1 == target.1;
                if skipped_transitions && !collinear {
                    continue;
                }
                let candidate_point = point(candidate)?;
                if self.segment_is_passable(map, request, current, candidate_point) {
                    visible = Some((candidate, candidate_point));
                    break;
                }
            }
            let Some((index, next)) = visible else {
                return Err(MapError::NoPathFound {
                    start: request.start,
                    goal: request.goal,
                });
            };
            if self.buffers.waypoints_scratch.last() != Some(&next) {
                self.buffers.waypoints_scratch.push(next);
            }
            current_index = index;
        }
        Ok(self.buffers.waypoints_scratch.clone())
    }

    fn build_portals(&mut self, map: &GameMap, request: &PathRequestWithVision<'_>) {
        self.buffers.portals.clear();
        let width = self.width;
        let corridor_len = self.buffers.corridor_cells.len();
        if corridor_len <= 1 {
            return;
        }

        for i in 0..corridor_len - 1 {
            let c1 = self.buffers.corridor_cells[i];
            let c2 = self.buffers.corridor_cells[i + 1];

            let x1 = signed(c1 % width);
            let y1 = signed(c1 / width);
            let x2 = signed(c2 % width);
            let y2 = signed(c2 / width);

            let dx = x2 - x1;
            let dy = y2 - y1;

            if dx != 0 && dy != 0 {
                // Diagonal step: expand both orthogonal faces
                let intermediate_x = x2;
                let intermediate_y = y1;
                Self::add_orthogonal_portal(
                    &mut self.buffers.portals,
                    map,
                    request,
                    x1,
                    y1,
                    intermediate_x,
                    intermediate_y,
                );
                Self::add_orthogonal_portal(
                    &mut self.buffers.portals,
                    map,
                    request,
                    intermediate_x,
                    intermediate_y,
                    x2,
                    y2,
                );
            } else {
                Self::add_orthogonal_portal(
                    &mut self.buffers.portals,
                    map,
                    request,
                    x1,
                    y1,
                    x2,
                    y2,
                );
            }
        }

        // Goal portal
        self.buffers.portals.push(PortalEdge {
            left: request.goal,
            right: request.goal,
        });
    }

    /// Pushes the portal crossed by one orthogonal step from cell `(x1, y1)` to cell `(x2, y2)`.
    ///
    /// The portal lies on the grid line shared by the two cells. It is widened along that line,
    /// in both directions, while the strip beside it stays connected on both sides
    /// ([`is_portal_pair_compatible`]), and its ends are ordered left then right as seen walking
    /// the step, per [`ORTHOGONAL_PORTALS`]. A zero step pushes nothing.
    fn add_orthogonal_portal(
        portals: &mut Vec<PortalEdge>,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
    ) {
        let dx = x2 - x1;
        let dy = y2 - y1;
        // The x axis takes precedence, as for any step that reaches here from `build_portals`.
        let step = if dx > 0 {
            (1, 0)
        } else if dx < 0 {
            (-1, 0)
        } else if dy > 0 {
            (0, 1)
        } else if dy < 0 {
            (0, -1)
        } else {
            return;
        };
        let Some(&(_, _, handedness)) = ORTHOGONAL_PORTALS
            .iter()
            .find(|&&(step_x, step_y, _)| (step_x, step_y) == step)
        else {
            return;
        };

        // A step along x crosses a vertical grid line and the portal is widened along y; a step
        // along y crosses a horizontal one and the portal is widened along x.
        let along_x = step.0 != 0;
        let extent = signed(if along_x { map.height() } else { map.width() });
        let line = if along_x { x1.max(x2) } else { y1.max(y2) };
        let origin = if along_x { y1 } else { x1 };
        let from = |t: i32| if along_x { (x1, t) } else { (t, y1) };
        let to = |t: i32| if along_x { (x2, t) } else { (t, y2) };
        let compatible = |next: i32, previous: i32| -> bool {
            is_portal_pair_compatible(
                map,
                request,
                from(next),
                to(next),
                from(previous),
                to(previous),
            )
        };

        let mut high = origin;
        while high + 1 < extent && compatible(high + 1, high) {
            high += 1;
        }
        let mut low = origin;
        while low > 0 && compatible(low - 1, low) {
            low -= 1;
        }

        let point = |t: i32| {
            if along_x {
                SimVec2::new(SimScalar::from_num(line), SimScalar::from_num(t))
            } else {
                SimVec2::new(SimScalar::from_num(t), SimScalar::from_num(line))
            }
        };
        let low_end = point(low);
        let high_end = point(high + 1);
        let (left, right) = match handedness {
            PortalHandedness::HighIsLeft => (high_end, low_end),
            PortalHandedness::LowIsLeft => (low_end, high_end),
        };
        portals.push(PortalEdge { left, right });
    }

    fn run_funnel(&mut self, map: &GameMap, request: &PathRequestWithVision<'_>) -> Vec<SimVec2> {
        self.buffers.waypoints_scratch.clear();
        let portals = &self.buffers.portals;

        if portals.is_empty() {
            return vec![request.start, request.goal];
        }

        let mut apex = request.start;
        let mut portal_left = apex;
        let mut portal_right = apex;
        let mut left_idx = 0usize;
        let mut right_idx = 0usize;

        self.buffers.waypoints_scratch.push(apex);

        let mut i = 0usize;
        while i < portals.len() {
            let p_left = portals[i].left;
            let p_right = portals[i].right;

            // Skip backwards-facing portals relative to apex
            if tri_area(apex, p_left, p_right) > SimScalar::ZERO {
                i += 1;
                continue;
            }

            // Update Right leg: does p_right narrow the right leg?
            if tri_area(apex, portal_right, p_right) >= SimScalar::ZERO {
                if portal_left == apex || tri_area(apex, portal_left, p_right) <= SimScalar::ZERO {
                    // Narrows right without crossing left
                    portal_right = p_right;
                    right_idx = i;
                } else {
                    // Right crossed over Left leg -> portal_left is new apex
                    let offset_left = if portal_left == request.goal {
                        portal_left
                    } else {
                        Self::offset_corner_waypoint(portal_left, request.clearance, map, request)
                    };
                    self.buffers.waypoints_scratch.push(offset_left);
                    apex = offset_left;
                    portal_left = apex;
                    portal_right = apex;
                    i = left_idx + 1;
                    left_idx = i;
                    right_idx = i;
                    continue;
                }
            }

            // Update Left leg: does p_left narrow the left leg?
            if tri_area(apex, portal_left, p_left) <= SimScalar::ZERO {
                if portal_right == apex || tri_area(apex, portal_right, p_left) >= SimScalar::ZERO {
                    // Narrows left without crossing right
                    portal_left = p_left;
                    left_idx = i;
                } else {
                    // Left crossed over Right leg -> portal_right is new apex
                    let offset_right = if portal_right == request.goal {
                        portal_right
                    } else {
                        Self::offset_corner_waypoint(portal_right, request.clearance, map, request)
                    };
                    self.buffers.waypoints_scratch.push(offset_right);
                    apex = offset_right;
                    portal_left = apex;
                    portal_right = apex;
                    i = right_idx + 1;
                    left_idx = i;
                    right_idx = i;
                    continue;
                }
            }

            i += 1;
        }

        if self.buffers.waypoints_scratch.last() != Some(&request.goal) {
            self.buffers.waypoints_scratch.push(request.goal);
        }

        self.buffers.waypoints_scratch.clone()
    }

    fn offset_corner_waypoint(
        vertex: SimVec2,
        clearance: UnitClearance,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
    ) -> SimVec2 {
        let vx = vertex.x.0.saturating_round().saturating_to_num::<i32>();
        let vy = vertex.y.0.saturating_round().saturating_to_num::<i32>();

        let offsets = [
            (-1i32, -1i32, 1i32, 1i32),
            (0, -1, -1, 1),
            (-1, 0, 1, -1),
            (0, 0, -1, -1),
        ];

        let mut dir_x = SimScalar::ZERO;
        let mut dir_y = SimScalar::ZERO;
        let mut blocked_count = 0;

        for (cx_off, cy_off, away_x, away_y) in offsets {
            let cx = vx + cx_off;
            let cy = vy + cy_off;
            let blocked = !is_cell_passable_for_request(map, request, cx, cy);

            if blocked {
                dir_x += SimScalar::from_num(away_x);
                dir_y += SimScalar::from_num(away_y);
                blocked_count += 1;
            }
        }

        // If all 4 cells around `(vx, vy)` are individually passable, check whether a
        // cliff or ramp side-wall disconnects any orthogonal edge in the 2x2 quad.
        let mut min_margin = SimScalar::ZERO;
        if blocked_count == 0 && vx > 0 && vy > 0 {
            let (ux, uy) = (unsigned(vx), unsigned(vy));
            let q00 = (ux - 1, uy - 1);
            let q10 = (ux, uy - 1);
            let q01 = (ux - 1, uy);
            let q11 = (ux, uy);
            if !map.terrain_step_is_connected(q00, q11) {
                let quad = [
                    (q00, [q10, q01], -1_i32, -1_i32),
                    (q10, [q00, q11], 1_i32, -1_i32),
                    (q01, [q00, q11], -1_i32, 1_i32),
                    (q11, [q10, q01], 1_i32, 1_i32),
                ];
                let mut ramp_pivot_x = 0_i32;
                let mut ramp_pivot_y = 0_i32;
                let mut has_ramp_pivot = false;
                for &(cell, orth, into_x, into_y) in &quad {
                    let is_ramp = map
                        .cell(cell.0, cell.1)
                        .is_ok_and(|c| c.flags.contains(CellFlags::RAMP));
                    let connected_to_both = orth
                        .iter()
                        .all(|&nbr| map.terrain_step_is_connected(cell, nbr));
                    let touches_ramp = orth.iter().any(|&nbr| {
                        map.cell(nbr.0, nbr.1)
                            .is_ok_and(|c| c.flags.contains(CellFlags::RAMP))
                    });
                    if !is_ramp && connected_to_both && touches_ramp {
                        ramp_pivot_x += into_x;
                        ramp_pivot_y += into_y;
                        has_ramp_pivot = true;
                    }
                }
                if has_ramp_pivot {
                    dir_x = SimScalar::from_num(ramp_pivot_x);
                    dir_y = SimScalar::from_num(ramp_pivot_y);
                    blocked_count += 1;
                } else {
                    for &(cell, orth, away_x, away_y) in &[
                        (q00, [q10, q01], 1_i32, 1_i32),
                        (q10, [q00, q11], -1_i32, 1_i32),
                        (q01, [q00, q11], 1_i32, -1_i32),
                        (q11, [q10, q01], -1_i32, -1_i32),
                    ] {
                        if orth
                            .iter()
                            .any(|&nbr| !map.terrain_step_is_connected(cell, nbr))
                        {
                            dir_x += SimScalar::from_num(away_x);
                            dir_y += SimScalar::from_num(away_y);
                            blocked_count += 1;
                        }
                    }
                }
                min_margin = SimScalar::from_f32_lossy(0.25);
            }
        }

        if blocked_count == 0 || (dir_x == SimScalar::ZERO && dir_y == SimScalar::ZERO) {
            return vertex;
        }

        let dir = SimVec2::new(dir_x, dir_y).normalize_or_zero();
        let offset_dist = (clearance.radius_scalar() + CORNER_SAFETY_MARGIN).max(min_margin);
        vertex + SimVec2::new(dir.x * offset_dist, dir.y * offset_dist)
    }
}

/// Tests whether a unit's disc fits at some point of cell `(x, y)` under this request's knowledge.
///
/// The caller has already checked the cell against the clearance table that applies to the
/// request. A request with player knowledge uses the static table, because which structures it
/// knows differs per player and cannot be precomputed.
// PERFORMANCE RATIONALE:
// Why: This runs for every neighbour the search expands, and most cells have no structure nearby.
// What: `all_structures` treats every structure as a wall, so a cell with enough clearance there
// fits whatever the player knows, and only cells near a structure need the per-cell disc test.
// Trade-off: Two code paths, in exchange for a table lookup in the common case.
fn body_fits(
    all_structures: &[u8],
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
    x: u32,
    y: u32,
) -> bool {
    let (cell_x, cell_y) = (signed(x), signed(y));
    if !is_cell_passable_for_request(map, request, cell_x, cell_y) {
        return false;
    }
    let required = request.clearance.required_clearance();
    let index = (y as usize) * (map.width() as usize) + (x as usize);
    if !request.block_unexplored_ramps && all_structures[index] >= required {
        return true;
    }
    // Candidate disc centres are the nine half-cell lattice points of this cell. A disc of
    // `required` eighths overlaps another cell when 4 * sqrt(D) < required, for D the squared
    // half-cell distance from the centre to that cell, hence the squared comparison below.
    let limit = u32::from(required) * u32::from(required);
    let reach = 2;
    (0..=2).any(|offset_y| {
        (0..=2).any(|offset_x| {
            let (centre_x, centre_y) = (2 * cell_x + offset_x, 2 * cell_y + offset_y);
            ((cell_y - reach)..=(cell_y + reach)).all(|other_y| {
                ((cell_x - reach)..=(cell_x + reach)).all(|other_x| {
                    let gap_x = half_cell_gap(centre_x, other_x);
                    let gap_y = half_cell_gap(centre_y, other_y);
                    16 * (gap_x * gap_x + gap_y * gap_y) >= limit
                        || is_cell_passable_for_request(map, request, other_x, other_y)
                })
            })
        })
    })
}

/// Distance in half cells from a lattice coordinate to the extent of a cell along one axis.
#[inline]
fn half_cell_gap(point: i32, cell: i32) -> u32 {
    let (low, high) = (2 * cell, 2 * cell + 2);
    (low - point).max(point - high).max(0).unsigned_abs()
}

/// Returns `true` if cell `(x, y)` is walkable terrain and not blocked by a known structure.
#[inline]
fn is_cell_passable_for_request(
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
    x: i32,
    y: i32,
) -> bool {
    let (Ok(ux), Ok(uy)) = (u32::try_from(x), u32::try_from(y)) else {
        return false;
    };
    let Ok(c) = map.cell(ux, uy) else {
        return false;
    };
    if !map.terrain_step_is_connected((ux, uy), (ux, uy)) {
        return false;
    }
    if request.block_unexplored_ramps {
        if let Some(vision) = request.vision {
            let Ok((start_x, start_y)) = map.world_to_cell(request.start) else {
                return false;
            };
            let Ok(start_cell) = map.cell(start_x, start_y) else {
                return false;
            };
            if (ux, uy) != (start_x, start_y)
                && vision.visibility_at(ux, uy) == VisibilityState::Unexplored
                && (c.ramp.is_some() || c.cliff_level > start_cell.cliff_level)
            {
                return false;
            }
        }
    }
    if c.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE) {
        let is_known = if let Some(mask) = request.known_structures {
            let idx = (uy as usize) * (map.width() as usize) + (ux as usize);
            mask.get(idx).copied().unwrap_or(false)
        } else if let Some(vis) = request.vision {
            vis.visibility_at(ux, uy) == VisibilityState::Visible
        } else {
            true
        };
        if is_known {
            return false;
        }
    }
    true
}

/// Which end of an orthogonal portal the funnel treats as its left side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PortalHandedness {
    /// The end at the higher coordinate along the portal's line is on the left.
    HighIsLeft,
    /// The end at the lower coordinate along the portal's line is on the left.
    LowIsLeft,
}

/// Each orthogonal grid step `(dx, dy)` with the side of its portal that lies on the walker's
/// left: walking east (+x) the left is +y, walking west -y, walking north (+y) -x, walking south
/// +x. The funnel requires this orientation, so a wrong row bends every path through that face.
const ORTHOGONAL_PORTALS: [(i32, i32, PortalHandedness); 4] = [
    (1, 0, PortalHandedness::HighIsLeft),
    (-1, 0, PortalHandedness::LowIsLeft),
    (0, 1, PortalHandedness::LowIsLeft),
    (0, -1, PortalHandedness::HighIsLeft),
];

/// Extends a portal only across a connected strip of terrain on both sides.
#[inline]
fn is_portal_pair_compatible(
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
    p1: (i32, i32),
    p2: (i32, i32),
    previous1: (i32, i32),
    previous2: (i32, i32),
) -> bool {
    if !is_cell_passable_for_request(map, request, p1.0, p1.1)
        || !is_cell_passable_for_request(map, request, p2.0, p2.1)
    {
        return false;
    }
    let cell = |p: (i32, i32)| (unsigned(p.0), unsigned(p.1));
    map.terrain_step_is_connected(cell(p1), cell(p2))
        && map.terrain_step_is_connected(cell(previous1), cell(p1))
        && map.terrain_step_is_connected(cell(previous2), cell(p2))
}

#[inline]
fn tri_area(a: SimVec2, b: SimVec2, c: SimVec2) -> SimScalar {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

#[inline]
fn octile_heuristic(x1: u32, y1: u32, x2: u32, y2: u32) -> u32 {
    let dx = x1.abs_diff(x2);
    let dy = y1.abs_diff(y2);
    10 * dx.max(dy) + 4 * dx.min(dy)
}
