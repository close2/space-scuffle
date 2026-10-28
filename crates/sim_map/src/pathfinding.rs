//! Dynamic Clearance Grid, A* Corridor Search, and Simple Stupid Funnel (String-Pulling).
//!
//! This module implements dynamic, clearance-aware pathfinding for Space Scuffle.
//! It computes multi-radius unit clearance, executes A* corridor searches on the grid,
//! and applies the Simple Stupid Funnel algorithm with corner clearance offsets to
//! produce smooth, arbitrary-angle Euclidean waypoints.

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
    /// Radius ~0.375. Light infantry (e.g. Marine); can squeeze through diagonal corners.
    Small,
    /// Radius ~0.625. Standard combat units; requires 1 full cell.
    Medium,
    /// Radius ~1.125. Massive units / siege tanks; requires 2x2 clear passage.
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
            Self::Large => SimScalar::from_num(1.125),
        }
    }

    /// Returns the minimum contiguous clearance box size in 1x1 cells.
    #[must_use]
    #[inline]
    pub fn required_clearance_cells(self) -> u8 {
        match self {
            Self::Small | Self::Medium => 1,
            Self::Large => 2,
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

/// Dynamic pathfinder operating over `GameMap`.
pub struct Pathfinder {
    width: u32,
    height: u32,
    /// Clearance value per cell: maximum square size in cells that fits at this cell.
    clearance_grid: Vec<u8>,
    /// Static clearance value per cell, ignoring dynamic structure obstacles.
    static_clearance_grid: Vec<u8>,
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

    /// Clearance value in cells at `(x, y)`.
    #[must_use]
    pub fn clearance_at(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            0
        } else {
            self.clearance_grid[(y as usize) * (self.width as usize) + (x as usize)]
        }
    }

    /// Static terrain clearance value in cells at `(x, y)` ignoring structures.
    #[must_use]
    pub fn static_clearance_at(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            0
        } else {
            self.static_clearance_grid[(y as usize) * (self.width as usize) + (x as usize)]
        }
    }

    /// Recomputes the clearance grid across the entire map.
    pub fn recompute_clearance(&mut self, map: &GameMap) {
        let w = self.width as usize;
        let h = self.height as usize;
        self.clearance_grid.fill(0);
        self.static_clearance_grid.fill(0);

        for y in (0..h).rev() {
            for x in (0..w).rev() {
                let idx = y * w + x;
                let cell = match map.cell(x as u32, y as u32) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let static_passable =
                    map.terrain_step_is_connected((x as u32, y as u32), (x as u32, y as u32));
                let dynamic_passable =
                    static_passable && !cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE);

                let origin = (x as u32, y as u32);
                let right = ((x + 1) as u32, y as u32);
                let down = (x as u32, (y + 1) as u32);
                let diagonal = ((x + 1) as u32, (y + 1) as u32);
                let cliff_match = map.terrain_step_is_connected(origin, right)
                    && map.terrain_step_is_connected(origin, down)
                    && map.terrain_step_is_connected(right, diagonal)
                    && map.terrain_step_is_connected(down, diagonal);

                if static_passable {
                    if x == w - 1 || y == h - 1 {
                        self.static_clearance_grid[idx] = 1;
                    } else {
                        let right = self.static_clearance_grid[idx + 1];
                        let down = self.static_clearance_grid[idx + w];
                        let diag = self.static_clearance_grid[idx + w + 1];
                        if cliff_match {
                            let min_c = right.min(down).min(diag);
                            self.static_clearance_grid[idx] = min_c.saturating_add(1);
                        } else {
                            self.static_clearance_grid[idx] = 1;
                        }
                    }
                }

                if dynamic_passable {
                    if x == w - 1 || y == h - 1 {
                        self.clearance_grid[idx] = 1;
                    } else {
                        let right = self.clearance_grid[idx + 1];
                        let down = self.clearance_grid[idx + w];
                        let diag = self.clearance_grid[idx + w + 1];
                        if cliff_match {
                            let min_c = right.min(down).min(diag);
                            self.clearance_grid[idx] = min_c.saturating_add(1);
                        } else {
                            self.clearance_grid[idx] = 1;
                        }
                    }
                }
            }
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

        let req_clearance = request.clearance.required_clearance_cells();

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

                    let mut footprint_blocked = false;
                    for oy in 0..req_clearance as u32 {
                        for ox in 0..req_clearance as u32 {
                            let fx = nx_u32 + ox;
                            let fy = ny_u32 + oy;
                            if !is_cell_passable_for_request(map, request, fx as i32, fy as i32) {
                                footprint_blocked = true;
                                break;
                            }
                        }
                        if footprint_blocked {
                            break;
                        }
                    }
                    if footprint_blocked {
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

                    // Structure corner squeezing is separate from terrain topology.
                    if is_diag
                        && request.clearance != UnitClearance::Small
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
        let required = request.clearance.required_clearance_cells();
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
            for oy in 0..u32::from(required) {
                for ox in 0..u32::from(required) {
                    if !is_cell_passable_for_request(
                        map,
                        request,
                        (next.0 + ox) as i32,
                        (next.1 + oy) as i32,
                    ) {
                        return false;
                    }
                }
            }
            if previous.0 != next.0
                && previous.1 != next.1
                && request.clearance != UnitClearance::Small
            {
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

        if blocked_count == 0 || (dir_x == SimScalar::ZERO && dir_y == SimScalar::ZERO) {
            return vertex;
        }

        let dir = SimVec2::new(dir_x, dir_y).normalize_or_zero();
        let offset_dist = clearance.radius_scalar() + CORNER_SAFETY_MARGIN;
        vertex + SimVec2::new(dir.x * offset_dist, dir.y * offset_dist)
    }
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
