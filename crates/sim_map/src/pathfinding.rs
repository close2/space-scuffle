//! The pathfinder: routes on the navigation mesh, and the hashed clearance field.
//!
//! [`Pathfinder`] owns the map's triangle navigation mesh ([`crate::navmesh`]) and answers every
//! route question of the simulation from it (plan `2026-11-09-sc2-pathfinding`, D-4 to D-7,
//! `StarCraft` II's reported design): triangle A* with each triangle's width for the unit's own
//! radius, the radius funnel, and one segment predicate shared by routes and movement. It
//! rebuilds the mesh whenever structures change, together with the clearance field.
//!
//! # Clearance model
//! The clearance field answers one question for each cell: how large a disc can stand somewhere
//! inside this cell without overlapping a wall? Walls are blocked cells, cliff edges and the map
//! border. The answer is a Euclidean distance field, so it is symmetric: it cannot depend on which
//! side of a unit a wall lies, and mirroring a map mirrors its clearance. Routes no longer read it;
//! it is part of the simulation's checksum and of the map tools' diagnostics.
//!
//! # Invariants
//! - `clearance_at` is zero exactly on cells no unit may enter.
//! - The mesh and the clearance field always describe the same map state, and `nav_version`
//!   changes whenever either is rebuilt.
//! - The lane cuts (plan `2026-11-10-group-routing`, D-2) are built once from the terrain and
//!   kept across structure rebuilds; `K`, the most cuts any mesh edge crosses, is recomputed
//!   with every mesh.

use crate::error::{MapError, Result};
use crate::grid::{CellFlags, GameMap};
use crate::navmesh::lanes::{LaneCuts, LaneWord};
use crate::navmesh::search::{LaneRestriction, MeshRoute, MeshSearch};
use crate::navmesh::{self, NavMesh};
use crate::vision::PlayerVisionMap;
use sim_types::{SimScalar, SimVec2};

/// Safety margin added to unit radius when rounding obstacle corners.
pub const CORNER_SAFETY_MARGIN: SimScalar = SimScalar::from_raw(214_748_365); // ~0.05 in Q32.32 (0.05 * 2^32)

/// Request parameters for a pathfinding query.
#[derive(Debug, Clone, Copy)]
pub struct PathRequest {
    /// Starting position in continuous world space.
    pub start: SimVec2,
    /// Goal position in continuous world space.
    pub goal: SimVec2,
    /// The unit's own collision radius in world units: the route keeps the whole disc clear of
    /// walls, and corner waypoints lie at this radius plus [`CORNER_SAFETY_MARGIN`] from the
    /// vertex they round.
    pub radius: SimScalar,
}

/// Extended pathfinding request supporting optional player fog-of-war perception.
#[derive(Debug, Clone, Copy)]
pub struct PathRequestWithVision<'a> {
    /// Starting continuous world position.
    pub start: SimVec2,
    /// Destination continuous world position.
    pub goal: SimVec2,
    /// The unit's own collision radius in world units: the route keeps the whole disc clear of
    /// walls, and corner waypoints lie at this radius plus [`CORNER_SAFETY_MARGIN`] from the
    /// vertex they round.
    pub radius: SimScalar,
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
            radius: r.radius,
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
    /// The triangle navigation mesh of the current map state (plan D-2, D-8), rebuilt with the
    /// clearance fields. `None` for a map beyond the mesh's size limit, which has no routes.
    navmesh: Option<NavMesh>,
    mesh_search: MeshSearch,
    /// The terrain's lane cuts (group-routing D-2): structure-independent, built once.
    lane_cuts: LaneCuts,
    /// `K` of group-routing D-5 for the current mesh: the most cuts any of its edges crosses.
    lane_edge_bound: u32,
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
            navmesh: None,
            mesh_search: MeshSearch::default(),
            lane_cuts: LaneCuts::build(map),
            lane_edge_bound: 0,
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

        self.navmesh = NavMesh::build(map).ok();
        self.lane_edge_bound = self
            .navmesh
            .as_ref()
            .map_or(0, |mesh| self.lane_cuts.max_edge_crossings(mesh));
        self.nav_version = self.nav_version.wrapping_add(1);
    }

    /// The navigation mesh of the current map state, if the map is within the mesh's limits.
    #[must_use]
    pub fn navmesh(&self) -> Option<&NavMesh> {
        self.navmesh.as_ref()
    }

    /// Funnel routes that failed the segment predicate and fell back to portal midpoints since
    /// this pathfinder was built (plan D-6 diagnostic).
    #[must_use]
    pub fn mesh_funnel_fallbacks(&self) -> u64 {
        self.mesh_search.fallbacks
    }

    /// Finds a route on the navigation mesh for a disc of `request.radius` (plan D-4 to D-7):
    /// triangle A* with Demyen widths, the radius funnel, and the mesh segment predicate; the
    /// details of the route are in [`MeshRoute`].
    ///
    /// # Errors
    /// [`MapError::InvalidStart`], [`MapError::InvalidGoal`] or [`MapError::NoPathFound`], as
    /// [`navmesh::search::find_route`] documents; [`MapError::NoPathFound`] also when the map has
    /// no mesh (a side above [`navmesh::NAV_MESH_MAX_DIMENSION`] cells).
    pub fn find_route_mesh(
        &mut self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
    ) -> Result<MeshRoute> {
        let mesh = self.navmesh.as_ref().ok_or(MapError::NoPathFound {
            start: request.start,
            goal: request.goal,
        })?;
        navmesh::search::find_route(mesh, &mut self.mesh_search, map, request)
    }

    /// The terrain's lane cuts (plan `2026-11-10-group-routing`, D-2), one per terrain obstacle.
    #[must_use]
    pub fn lane_cuts(&self) -> &LaneCuts {
        &self.lane_cuts
    }

    /// Number of lane cuts, which is the number of terrain obstacles (a diagnostic).
    #[must_use]
    pub fn lane_cut_count(&self) -> usize {
        self.lane_cuts.len()
    }

    /// `K` of the current mesh: the largest number of lane cuts any of its edges crosses (0 when
    /// the map has no mesh or no obstacle).
    #[must_use]
    pub fn lane_edge_bound(&self) -> u32 {
        self.lane_edge_bound
    }

    /// The lane word of the polyline through `polyline`, in world units: the reduced sequence of
    /// the cuts it crosses (D-2). Two routes with the same endpoints are in the same lane exactly
    /// when their words are equal.
    #[must_use]
    pub fn lane_word(&self, polyline: &[SimVec2]) -> LaneWord {
        self.lane_cuts.word(polyline)
    }

    /// The portal-midpoint polyline of the channel of the last route this pathfinder returned
    /// (`start`, the midpoint of every portal crossed, `goal`), for a route from `start` to
    /// `goal`: `[start, goal]` after a straight route. A diagnostic: the route and this polyline
    /// lie in one sleeve of triangles, so their lane words agree (group-routing D-2).
    #[must_use]
    pub fn last_channel_midpoints(&self, start: SimVec2, goal: SimVec2) -> Vec<SimVec2> {
        self.navmesh.as_ref().map_or_else(
            || vec![start, goal],
            |mesh| navmesh::funnel::portal_midpoints(mesh, start, goal, self.mesh_search.channel()),
        )
    }

    /// Finds a route for a disc of `request.radius` from `request.start` to `request.goal`,
    /// seeing every structure (no player knowledge). See [`Self::find_path_with_vision`].
    ///
    /// # Errors
    /// As [`Self::find_path_with_vision`].
    pub fn find_path(&mut self, map: &GameMap, request: &PathRequest) -> Result<PathResult> {
        self.find_path_with_vision(map, &(*request).into())
    }

    /// Finds the route a unit of radius `request.radius` walks, on the navigation mesh (plan
    /// D-4 to D-7, `StarCraft` II's reported design): the waypoints are the start, points on the
    /// circles of radius `radius + CORNER_SAFETY_MARGIN` round the vertices the route turns
    /// around, and the goal, in world units. Every leg passes [`Self::segment_is_passable`].
    ///
    /// A unit standing where its body does not fit may always leave (D-3): the start triangle
    /// is entered whatever its width. The goal must fit the whole disc, as in `StarCraft` II
    /// (plan Amendment 2); a goal that does not is reported as invalid, and the caller walks to
    /// [`Self::nearest_reachable_point`] instead.
    ///
    /// # Invariants
    /// 1. If `request.vision` is `Some` and `block_unexplored_ramps == true`, ramp or higher-ground
    ///    triangles with an unexplored cell of their terrain are closed (per triangle, coarser than
    ///    a per-cell rule; no production caller sets the flag).
    /// 2. A structure the request does not know (not in `known_structures`, or not `Visible` in
    ///    `vision`) is walked through until revealed.
    ///
    /// # Errors
    /// [`MapError::InvalidStart`] when the start is off the map or in a triangle the request may
    /// not enter; [`MapError::InvalidGoal`] likewise, or when the disc does not fit at the goal;
    /// [`MapError::NoPathFound`] when no channel admits the disc, or the map has no mesh.
    pub fn find_path_with_vision(
        &mut self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
    ) -> Result<PathResult> {
        self.find_route_mesh(map, request).map(|route| PathResult {
            waypoints: route.waypoints,
            total_length: route.total_length,
        })
    }

    /// Finds the route of [`Self::find_route_mesh`] restricted to the lane `lane` (plan
    /// `2026-11-10-group-routing`, D-5): the shortest route the search finds whose lane word is
    /// `lane`. With `lane` equal to the word of the unrestricted route, it returns that route.
    ///
    /// # Errors
    /// As [`Self::find_path_with_vision`]; [`MapError::NoPathFound`] also when no route of that
    /// lane admits the disc.
    pub fn find_route_mesh_in_lane(
        &mut self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
        lane: &LaneWord,
    ) -> Result<MeshRoute> {
        let mesh = self.navmesh.as_ref().ok_or(MapError::NoPathFound {
            start: request.start,
            goal: request.goal,
        })?;
        let restriction = LaneRestriction {
            cuts: &self.lane_cuts,
            bound: self.lane_edge_bound,
            word: lane,
        };
        navmesh::search::find_route_in_lane(mesh, &mut self.mesh_search, map, request, &restriction)
    }

    /// Finds the route a unit walks, as [`Self::find_path_with_vision`] does, restricted to the
    /// lane `lane` (plan `2026-11-10-group-routing`, D-5). See [`Self::find_route_mesh_in_lane`].
    ///
    /// # Errors
    /// As [`Self::find_route_mesh_in_lane`].
    pub fn find_path_in_lane(
        &mut self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
        lane: &LaneWord,
    ) -> Result<PathResult> {
        self.find_route_mesh_in_lane(map, request, lane)
            .map(|route| PathResult {
                waypoints: route.waypoints,
                total_length: route.total_length,
            })
    }

    /// The segment predicate of routes and movement (plan D-7): whether a disc of
    /// `request.radius` may sweep from `start` to `goal` through triangles the request may enter,
    /// keeping `radius` from every edge that blocks it. `false` when the map has no mesh.
    ///
    /// Takes `&mut self` for the mesh's reusable flood buffers, so it allocates nothing.
    pub fn segment_is_passable(
        &mut self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
        start: SimVec2,
        goal: SimVec2,
    ) -> bool {
        self.navmesh.as_ref().is_some_and(|mesh| {
            navmesh::search::segment_is_passable(
                mesh,
                &mut self.mesh_search,
                map,
                request,
                start,
                goal,
            )
        })
    }

    /// Finds the world point closest to `request.goal` that a disc of `request.radius` can reach
    /// from `request.start`, on the mesh.
    ///
    /// A blocked goal, or one cut off from the start, is the normal case for a click on a cliff
    /// or behind a wall. The answer is the centre of the reachable cell nearest the goal cell
    /// where the disc fits, ties to the lowest cell index, so every unit of one size in one
    /// region gets the same point. `Ok(None)` when the unit already stands in that cell.
    ///
    /// # Errors
    /// [`MapError::InvalidStart`] if the start is off the map or in a closed triangle, or the map
    /// has no mesh; [`MapError::InvalidGoal`] if the goal is off the map.
    // PERFORMANCE RATIONALE:
    // Why: The answer needs the whole region the disc reaches, and a group asks the same question.
    // What: One flood over the triangles, then one pass over the cells, into reused buffers.
    // Trade-off: O(map area) per call, so callers cache per goal and throttle by unit.
    pub fn nearest_reachable_point(
        &mut self,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
    ) -> Result<Option<SimVec2>> {
        let mesh = self
            .navmesh
            .as_ref()
            .ok_or(MapError::InvalidStart(request.start))?;
        navmesh::search::nearest_reachable_point(mesh, &mut self.mesh_search, map, request)
    }
}
