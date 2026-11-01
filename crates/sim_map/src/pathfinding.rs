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
    portals: Vec<PortalEdge>,
    waypoints_scratch: Vec<SimVec2>,
}

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

            let cx = (current_idx % (self.width as usize)) as u32;
            let cy = (current_idx / (self.width as usize)) as u32;
            let current_g = self.buffers.g_score[current_idx];

            // Explore 8 neighbors
            for dy in -1i32..=1i32 {
                for dx in -1i32..=1i32 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }

                    let nx = cx as i32 + dx;
                    let ny = cy as i32 + dy;

                    if !map.in_bounds(nx, ny) {
                        continue;
                    }

                    let nx_u32 = nx as u32;
                    let ny_u32 = ny as u32;
                    let n_idx = (ny_u32 as usize) * (self.width as usize) + (nx_u32 as usize);

                    if clearance_table[n_idx] < req_clearance {
                        continue;
                    }

                    if !body_fits(&self.clearance_grid, map, request, nx_u32, ny_u32) {
                        continue;
                    }

                    let n_cell = match map.cell(nx_u32, ny_u32) {
                        Ok(c) => c,
                        Err(_) => continue,
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
                        && (!is_cell_passable_for_request(map, request, nx, cy as i32)
                            || !is_cell_passable_for_request(map, request, cx as i32, ny))
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
                    previous.0 as i32,
                    next.1 as i32,
                ) && is_cell_passable_for_request(
                    map,
                    request,
                    next.0 as i32,
                    previous.1 as i32,
                );
            }
            true
        })
    }

    /// Constrains string-pulling to visible points of the verified A* corridor.
    ///
    /// Funnel corner offsets and widened portals can leave the original corridor.
    /// When their segments are invalid, greedily retain the furthest visible corridor
    /// center. Every retained edge is tested against the complete request policy.
    fn smooth_connected_corridor(
        &mut self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
    ) -> Result<Vec<SimVec2>> {
        self.buffers.waypoints_scratch.clear();
        self.buffers.waypoints_scratch.push(request.start);
        let end_index = self.buffers.corridor_cells.len() + 1;
        let point = |index: usize| -> Result<SimVec2> {
            if index == 0 {
                Ok(request.start)
            } else if index == end_index {
                Ok(request.goal)
            } else {
                let cell = self.buffers.corridor_cells[index - 1];
                map.cell_to_world_center(cell % self.width, cell / self.width)
            }
        };
        let mut current_index = 0;
        while current_index < end_index {
            let current = point(current_index)?;
            let mut visible = None;
            for candidate in ((current_index + 1)..=end_index).rev() {
                let candidate_point = point(candidate)?;
                let skips_ramp_turn = if candidate > current_index + 1 {
                    ((current_index + 1)..candidate).any(|k| {
                        if k == 0 || k >= end_index {
                            return false;
                        }
                        let curr_cell = self.buffers.corridor_cells[k - 1];
                        let curr_cx = curr_cell % self.width;
                        let curr_cy = curr_cell / self.width;
                        let is_curr_ramp = map
                            .cell(curr_cx, curr_cy)
                            .is_ok_and(|c| c.flags.contains(CellFlags::RAMP));

                        let prev_is_ramp = if k == 1 {
                            let (scx, scy) = map
                                .world_to_cell(request.start)
                                .unwrap_or((curr_cx, curr_cy));
                            map.cell(scx, scy)
                                .is_ok_and(|c| c.flags.contains(CellFlags::RAMP))
                        } else {
                            let prev_cell = self.buffers.corridor_cells[k - 2];
                            map.cell(prev_cell % self.width, prev_cell / self.width)
                                .is_ok_and(|c| c.flags.contains(CellFlags::RAMP))
                        };

                        let next_is_ramp = if k + 1 == end_index {
                            let (gcx, gcy) = map
                                .world_to_cell(request.goal)
                                .unwrap_or((curr_cx, curr_cy));
                            map.cell(gcx, gcy)
                                .is_ok_and(|c| c.flags.contains(CellFlags::RAMP))
                        } else {
                            let next_cell = self.buffers.corridor_cells[k];
                            map.cell(next_cell % self.width, next_cell / self.width)
                                .is_ok_and(|c| c.flags.contains(CellFlags::RAMP))
                        };

                        let ramp_transition =
                            is_curr_ramp != prev_is_ramp || is_curr_ramp != next_is_ramp;
                        if !ramp_transition {
                            return false;
                        }
                        let anchor_c = if current_index == 0 {
                            map.world_to_cell(request.start)
                                .unwrap_or((curr_cx, curr_cy))
                        } else {
                            let cell = self.buffers.corridor_cells[current_index - 1];
                            (cell % self.width, cell / self.width)
                        };
                        let candidate_c = if candidate == end_index {
                            map.world_to_cell(request.goal)
                                .unwrap_or((curr_cx, curr_cy))
                        } else {
                            let cell = self.buffers.corridor_cells[candidate - 1];
                            (cell % self.width, cell / self.width)
                        };
                        let collinear = anchor_c.0 == candidate_c.0 || anchor_c.1 == candidate_c.1;
                        !collinear
                    })
                } else {
                    false
                };
                if !skips_ramp_turn
                    && self.segment_is_passable(map, request, current, candidate_point)
                {
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
            let c1 = self.buffers.corridor_cells[i] as usize;
            let c2 = self.buffers.corridor_cells[i + 1] as usize;

            let x1 = (c1 % (width as usize)) as i32;
            let y1 = (c1 / (width as usize)) as i32;
            let x2 = (c2 % (width as usize)) as i32;
            let y2 = (c2 / (width as usize)) as i32;

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

    fn add_orthogonal_portal(
        portals: &mut Vec<PortalEdge>,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
    ) {
        let width = map.width();
        let height = map.height();
        let dx = x2 - x1;
        let dy = y2 - y1;

        if dx > 0 {
            // East step (+X): portal along line x = x2
            let mut y_up = y1;
            while y_up + 1 < height as i32
                && is_portal_pair_compatible(
                    map,
                    request,
                    (x1, y_up + 1),
                    (x2, y_up + 1),
                    (x1, y_up),
                    (x2, y_up),
                )
            {
                y_up += 1;
            }
            let mut y_down = y1;
            while y_down > 0
                && is_portal_pair_compatible(
                    map,
                    request,
                    (x1, y_down - 1),
                    (x2, y_down - 1),
                    (x1, y_down),
                    (x2, y_down),
                )
            {
                y_down -= 1;
            }
            portals.push(PortalEdge {
                left: SimVec2::new(SimScalar::from_num(x2), SimScalar::from_num(y_up + 1)),
                right: SimVec2::new(SimScalar::from_num(x2), SimScalar::from_num(y_down)),
            });
        } else if dx < 0 {
            // West step (-X): portal along line x = x1
            let mut y_up = y1;
            while y_up + 1 < height as i32
                && is_portal_pair_compatible(
                    map,
                    request,
                    (x1, y_up + 1),
                    (x2, y_up + 1),
                    (x1, y_up),
                    (x2, y_up),
                )
            {
                y_up += 1;
            }
            let mut y_down = y1;
            while y_down > 0
                && is_portal_pair_compatible(
                    map,
                    request,
                    (x1, y_down - 1),
                    (x2, y_down - 1),
                    (x1, y_down),
                    (x2, y_down),
                )
            {
                y_down -= 1;
            }
            portals.push(PortalEdge {
                left: SimVec2::new(SimScalar::from_num(x1), SimScalar::from_num(y_down)),
                right: SimVec2::new(SimScalar::from_num(x1), SimScalar::from_num(y_up + 1)),
            });
        } else if dy > 0 {
            // North step (+Y): portal along line y = y2
            let mut x_left = x1;
            while x_left > 0
                && is_portal_pair_compatible(
                    map,
                    request,
                    (x_left - 1, y1),
                    (x_left - 1, y2),
                    (x_left, y1),
                    (x_left, y2),
                )
            {
                x_left -= 1;
            }
            let mut x_right = x1;
            while x_right + 1 < width as i32
                && is_portal_pair_compatible(
                    map,
                    request,
                    (x_right + 1, y1),
                    (x_right + 1, y2),
                    (x_right, y1),
                    (x_right, y2),
                )
            {
                x_right += 1;
            }
            portals.push(PortalEdge {
                left: SimVec2::new(SimScalar::from_num(x_left), SimScalar::from_num(y2)),
                right: SimVec2::new(SimScalar::from_num(x_right + 1), SimScalar::from_num(y2)),
            });
        } else if dy < 0 {
            // South step (-Y): portal along line y = y1
            let mut x_left = x1;
            while x_left > 0
                && is_portal_pair_compatible(
                    map,
                    request,
                    (x_left - 1, y1),
                    (x_left - 1, y2),
                    (x_left, y1),
                    (x_left, y2),
                )
            {
                x_left -= 1;
            }
            let mut x_right = x1;
            while x_right + 1 < width as i32
                && is_portal_pair_compatible(
                    map,
                    request,
                    (x_right + 1, y1),
                    (x_right + 1, y2),
                    (x_right, y1),
                    (x_right, y2),
                )
            {
                x_right += 1;
            }
            portals.push(PortalEdge {
                left: SimVec2::new(SimScalar::from_num(x_right + 1), SimScalar::from_num(y1)),
                right: SimVec2::new(SimScalar::from_num(x_left), SimScalar::from_num(y1)),
            });
        }
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
        let vx = vertex.x.0.round().to_num::<i32>();
        let vy = vertex.y.0.round().to_num::<i32>();

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
            let q00 = ((vx - 1) as u32, (vy - 1) as u32);
            let q10 = (vx as u32, (vy - 1) as u32);
            let q01 = ((vx - 1) as u32, vy as u32);
            let q11 = (vx as u32, vy as u32);
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
    if !is_cell_passable_for_request(map, request, x as i32, y as i32) {
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
    let (cell_x, cell_y) = (x as i32, y as i32);
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
    if !map.in_bounds(x, y) {
        return false;
    }
    let ux = x as u32;
    let uy = y as u32;
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
    let cell = |p: (i32, i32)| (p.0 as u32, p.1 as u32);
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
    let dx = (x1 as i32 - x2 as i32).unsigned_abs();
    let dy = (y1 as i32 - y2 as i32).unsigned_abs();
    10 * dx.max(dy) + 4 * dx.min(dy)
}
