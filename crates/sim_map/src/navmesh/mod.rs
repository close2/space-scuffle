//! Constrained Delaunay navigation mesh (the `StarCraft` II design of plan
//! `docs/plans/2026-11-09-sc2-pathfinding.md`, Part B).
//!
//! # Responsibility
//! Owns the triangle mesh of a map: its exact construction from terrain and structure footprints
//! (D-2, `cdt.rs`), its triangle labels and per-request passability (knowledge rules of
//! `find_path_with_vision`), point location through a per-cell triangle index (D-5), and the
//! radius-independent triangle widths of Demyen and Buro (D-4). The search (`search.rs`), the
//! radius funnel (`funnel.rs`) and the segment predicate (`segment.rs`) read it; the terrain lane
//! cuts and words (`lanes.rs`, plan `2026-11-10-group-routing` D-2) read its wall classification.
//!
//! # How it fits
//! [`crate::Pathfinder`] builds a [`NavMesh`] with its clearance fields and rebuilds it on every
//! structure change (D-8). Until the switch (WP9) nothing in the simulation reads it: it is
//! derived, unhashed state, so results stay bit-identical.
//!
//! # Invariants
//! - Every vertex is a point of the quarter-cell lattice (`predicates.rs`, plan
//!   `2026-11-18-contour-footprints` D-3, D-4). Constraints are chains of unit lattice edges in
//!   eight directions (the cell grid's and the two diagonals), so construction uses exact integer
//!   predicates only and no Steiner points.
//! - Constraints are the map border, every cell edge where terrain does not connect, every cell
//!   edge where the `(cliff_level, ramp)` tag changes, the boundary of every structure's
//!   footprint shape, and every interior cell line of a structure's footprint clipped to its
//!   shape. Each triangle therefore lies in one region of uniform label, and a structure triangle
//!   in one cell, so knowledge stays per cell.
//! - No two constraints cross: a diagonal unit edge crosses no lattice line inside, axis-aligned
//!   unit edges meet only at lattice points, and a lattice point where edges branch, turn or
//!   change kind is a vertex. `NavMeshError::ConstraintRecovery` guards the rule.
//! - The mesh is a pure function of the map's cells and structures: insertion order of
//!   structures cannot matter, and removing a structure restores the terrain mesh exactly (its
//!   cells' flags are restored). A structure whose shape is the full box of its cells cuts
//!   exactly the mesh it cut before footprint shapes.
//! - Storage is `Vec`s indexed by `u32`; loops run in index order; no float, no hashed
//!   collection (charter §3.1).

#![allow(
    clippy::many_single_char_names,
    reason = "computational geometry names points, vertices and triangles by single letters, \
              as the papers it implements do (Demyen and Buro 2006, Sloan 1993)"
)]

pub mod cdt;
pub mod funnel;
pub mod lanes;
pub mod predicates;
pub mod search;
pub mod segment;

use sim_types::{EntityId, ResourceNodeId, SimScalar, SimVec2, VisibilityState};
use thiserror::Error;

use crate::footprint::PlacedFootprint;
use crate::grid::{CellFlags, GameMap, RampDir};
use crate::pathfinding::PathRequestWithVision;
use cdt::{Triangulation, NONE};
use predicates::{
    lattice_distance_sq, lattice_dot, lattice_point_segment_width, orient_q, LatticePoint, QPoint,
    SquaredWidth, LATTICE_BITS, LATTICE_PER_CELL, NAV_MESH_MAX_DIMENSION,
};

/// Why a navigation mesh could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NavMeshError {
    /// A map side exceeds [`NAV_MESH_MAX_DIMENSION`], beyond the predicates' overflow margins.
    #[error("map of {width}x{height} cells exceeds the navigation mesh limit")]
    TooLarge {
        /// Map width in cells.
        width: u32,
        /// Map height in cells.
        height: u32,
    },
    /// A constraint run could not be recovered; the constraint set violated its invariants.
    #[error("constraint from {from:?} to {to:?} could not be recovered")]
    ConstraintRecovery {
        /// First lattice endpoint of the run.
        from: LatticePoint,
        /// Second lattice endpoint of the run.
        to: LatticePoint,
    },
}

/// One of the four canonical directions of a unit lattice edge, in insertion order (D-4): an
/// edge is stored at its start point and runs one lattice unit east, north, north-east or
/// north-west.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Direction {
    East,
    North,
    NorthEast,
    NorthWest,
}

impl Direction {
    const ALL: [Self; 4] = [Self::East, Self::North, Self::NorthEast, Self::NorthWest];

    /// The direction's position in [`Self::ALL`], its bit within a lattice point's four in a
    /// [`StructureEdges`] mask.
    const fn index(self) -> u32 {
        match self {
            Self::East => 0,
            Self::North => 1,
            Self::NorthEast => 2,
            Self::NorthWest => 3,
        }
    }

    /// The lattice step of the edge.
    const fn step(self) -> (i32, i32) {
        match self {
            Self::East => (1, 0),
            Self::North => (0, 1),
            Self::NorthEast => (1, 1),
            Self::NorthWest => (-1, 1),
        }
    }
}

/// The stronger of two constraint kinds: `Wall` over `Seam` over none.
fn stronger(a: Option<EdgeKind>, b: Option<EdgeKind>) -> Option<EdgeKind> {
    let rank = |k: Option<EdgeKind>| match k {
        None | Some(EdgeKind::Free) => 0,
        Some(EdgeKind::Seam) => 1,
        Some(EdgeKind::Wall) => 2,
    };
    if rank(b) > rank(a) {
        b
    } else {
        a
    }
}

/// What a mesh edge is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// An unconstrained Delaunay edge inside a region of one label.
    Free,
    /// A constraint between connected terrain: a tag change or a structure cell's edge. It blocks
    /// a request exactly when a triangle on either side is impassable for that request.
    Seam,
    /// A constraint no ground unit crosses: the map border, a cliff, a ramp side wall, or the
    /// edge of unwalkable terrain.
    Wall,
}

/// What occupies a triangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriangleKind {
    /// Terrain no ground unit can stand on.
    Blocked,
    /// Walkable terrain.
    Open,
    /// One cell of a structure's footprint; impassable to a request that knows the structure.
    Structure(EntityId),
    /// Inside a standing resource node's outline; impassable to every request, since nodes are
    /// public map data (plan `2026-11-20-resource-footprints` D-5 M1, M2, D-9).
    Resource(ResourceNodeId),
}

/// The label of one triangle (D-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TriangleLabel {
    /// What occupies the triangle.
    pub kind: TriangleKind,
    /// Cliff level of the triangle's terrain.
    pub cliff_level: u8,
    /// Ramp direction of the triangle's terrain, if it is a ramp.
    pub ramp: Option<RampDir>,
    /// Row-major index of the cell containing the triangle's centroid.
    pub cell: u32,
}

/// One maximal constraint run: a straight chain of unit lattice edges (east, north, north-east
/// or north-west) between two mesh vertices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstraintRun {
    /// Start endpoint: the lower one, or the western one of a horizontal run.
    pub from: LatticePoint,
    /// End endpoint, one or more unit steps from `from` in the run's direction.
    pub to: LatticePoint,
    /// Constraint kind shared by every unit edge of the run.
    pub kind: EdgeKind,
}

/// Exact constrained Delaunay navigation mesh of one map state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavMesh {
    width: u32,
    height: u32,
    vertices: Vec<LatticePoint>,
    triangles: Vec<[u32; 3]>,
    adjacent: Vec<[u32; 3]>,
    kinds: Vec<[EdgeKind; 3]>,
    labels: Vec<TriangleLabel>,
    vertex_triangle: Vec<u32>,
    /// Triangle containing each cell's centre, row-major (D-5's jump-and-walk index).
    cell_triangle: Vec<u32>,
    /// Width between the two edges at each corner, with every structure blocking (D-4 cache).
    widths: Vec<[SquaredWidth; 3]>,
    /// Whether each vertex touches an edge that blocks when every structure is known.
    vertex_obstacle: Vec<bool>,
    /// Per cell, row-major: whether the cell is open and none of its four sides blocks when
    /// every structure is known (the contact query's early-out, plan
    /// `2026-11-11-contact-and-missiles` D-5).
    contact_free: Vec<bool>,
    runs: Vec<ConstraintRun>,
}

/// Next local index, counter-clockwise.
#[inline]
pub(crate) fn next(k: usize) -> usize {
    (k + 1) % 3
}

/// Previous local index, counter-clockwise.
#[inline]
pub(crate) fn prev(k: usize) -> usize {
    (k + 2) % 3
}

#[inline]
fn signed(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// Per-cell facts the constraint rules need.
#[derive(Clone, Copy, PartialEq, Eq)]
struct CellClass {
    usable: bool,
    level: u8,
    ramp: Option<RampDir>,
    structure: bool,
}

/// Constraint kinds of every unit cell edge of a map.
struct EdgeGrid {
    width: usize,
    height: usize,
    /// Edge from lattice `(x, y)` to `(x + 1, y)`, at `y * width + x`.
    horizontal: Vec<Option<EdgeKind>>,
    /// Edge from lattice `(x, y)` to `(x, y + 1)`, at `y * (width + 1) + x`.
    vertical: Vec<Option<EdgeKind>>,
}

impl EdgeGrid {
    fn new(map: &GameMap) -> Self {
        let (width, height) = (map.width() as usize, map.height() as usize);
        let classes: Vec<CellClass> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| classify_cell(map, x, y))
            .collect();
        let class_at = |x: usize, y: usize| classes[y * width + x];
        let kind = |a: (usize, usize), b: (usize, usize)| {
            edge_kind(map, class_at(a.0, a.1), class_at(b.0, b.1), a, b)
        };
        let mut horizontal = vec![None; width * (height + 1)];
        for y in 0..=height {
            for x in 0..width {
                horizontal[y * width + x] = if y == 0 || y == height {
                    Some(EdgeKind::Wall)
                } else {
                    kind((x, y - 1), (x, y))
                };
            }
        }
        let mut vertical = vec![None; (width + 1) * height];
        for y in 0..height {
            for x in 0..=width {
                vertical[y * (width + 1) + x] = if x == 0 || x == width {
                    Some(EdgeKind::Wall)
                } else {
                    kind((x - 1, y), (x, y))
                };
            }
        }
        Self {
            width,
            height,
            horizontal,
            vertical,
        }
    }

    fn east(&self, x: usize, y: usize) -> Option<EdgeKind> {
        (x < self.width)
            .then(|| self.horizontal[y * self.width + x])
            .flatten()
    }

    fn north(&self, x: usize, y: usize) -> Option<EdgeKind> {
        (y < self.height)
            .then(|| self.vertical[y * (self.width + 1) + x])
            .flatten()
    }

    /// Per cell, row-major: whether the cell is usable, holds no structure, and none of its four
    /// sides blocks a request that knows every structure. A side blocks such a request when it
    /// is a wall (the map border included) or borders a structure cell; every other seam joins
    /// two open triangles. The mesh's blocking edges lie on such sides or inside structure cells
    /// only (a structure's shape lies in its cells): an edge between two open cells' triangles
    /// is free or a passable seam.
    fn contact_free_cells(&self, map: &GameMap) -> Vec<bool> {
        let class = |x: usize, y: usize| classify_cell(map, x, y);
        let blocks = |kind: Option<EdgeKind>, neighbour: Option<(usize, usize)>| {
            kind == Some(EdgeKind::Wall) || neighbour.is_some_and(|(x, y)| class(x, y).structure)
        };
        let (w, h) = (self.width, self.height);
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let own = class(x, y);
                own.usable
                    && !own.structure
                    && !blocks(self.horizontal[y * w + x], y.checked_sub(1).map(|y| (x, y)))
                    && !blocks(
                        self.horizontal[(y + 1) * w + x],
                        (y + 1 < h).then_some((x, y + 1)),
                    )
                    && !blocks(
                        self.vertical[y * (w + 1) + x],
                        x.checked_sub(1).map(|x| (x, y)),
                    )
                    && !blocks(
                        self.vertical[y * (w + 1) + x + 1],
                        (x + 1 < w).then_some((x + 1, y)),
                    )
            })
            .collect()
    }
}

/// The unit lattice edges of every structure's geometry (D-4): the boundary of its shape and
/// the interior cell lines of its footprint clipped to the shape's open interior, all `Seam`.
/// A standing resource node's footprint counts as a structure's.
// PERFORMANCE RATIONALE:
// Why: The build asks for the kind of the eight unit edges at every candidate vertex and at
// every lattice point along a run. A sorted edge list searched by bisection made that lookup
// 68.5 % of a rebuild with the shipped resource nodes, 3.04 times the node-free rebuild (B-155).
// What: One `u64` per whole cell of the closed map rectangle, row-major; its bits are the
// canonical unit edges starting at the cell's sixteen quarter-lattice points, four directions
// each (`Self::slot`). A query is two shifts, one bounds check and one bit test; building sets
// bits in place, with no sort and no deduplication.
// Trade-off: Eight bytes per cell of the map, zeroed on every rebuild, where the sorted list
// held memory in proportion to the structures; the build's candidate list already holds eight
// bytes per cell corner, so the rebuild's peak stays of the same order. The table is one
// allocation of known size, where the list grew by doubling.
struct StructureEdges {
    /// Cells per row of `masks`: the map's width plus one, so that start points on the map's
    /// east line have a cell.
    columns: usize,
    /// Rows of `masks`: the map's height plus one, for start points on the north line.
    rows: usize,
    /// Per cell, row-major, the mask of the structure edges that start at its lattice points.
    masks: Vec<u64>,
}

// One cell's lattice points, four canonical directions each, fill at most one `u64`.
const _: () = assert!(LATTICE_PER_CELL * LATTICE_PER_CELL * 4 <= 64);

impl StructureEdges {
    fn new(map: &GameMap) -> Self {
        let columns = map.width() as usize + 1;
        let rows = map.height() as usize + 1;
        let mut edges = Self {
            columns,
            rows,
            masks: vec![0; columns * rows],
        };
        let mut insert = |start: (i32, i32), direction: Direction| {
            // A footprint lies on the map (placement refuses a cell beyond it), so every start
            // point lies in the closed map rectangle and has a slot.
            if let Some((index, bit)) = edges.slot(start, direction) {
                edges.masks[index] |= bit;
            }
        };
        for placed in map.structures().values() {
            footprint_edges(&placed.footprint(), &mut insert);
        }
        for resource in map.resources() {
            if let Some(placed) = resource.placed {
                footprint_edges(&placed.footprint, &mut insert);
            }
        }
        edges
    }

    /// The index in `masks` and the bit of the unit edge from lattice point `start` in
    /// `direction`; `None` when `start` lies outside the closed map rectangle, where no
    /// structure edge starts. Bit `4 * (LATTICE_PER_CELL * (y mod L) + x mod L) + d`, with
    /// `L = LATTICE_PER_CELL` and `d` the direction's index.
    #[inline]
    fn slot(&self, (x, y): (i32, i32), direction: Direction) -> Option<(usize, u64)> {
        let column = usize::try_from(x >> LATTICE_BITS).ok()?;
        let row = usize::try_from(y >> LATTICE_BITS).ok()?;
        if column >= self.columns || row >= self.rows {
            return None;
        }
        let local = |v: i32| (v & (LATTICE_PER_CELL - 1)).unsigned_abs();
        let point = local(y) * LATTICE_PER_CELL.unsigned_abs() + local(x);
        Some((
            row * self.columns + column,
            1 << (4 * point + direction.index()),
        ))
    }

    /// Whether the unit edge from lattice point `start` in `direction` is a structure edge.
    #[inline]
    fn contains(&self, start: (i32, i32), direction: Direction) -> bool {
        self.slot(start, direction)
            .is_some_and(|(index, bit)| self.masks[index] & bit != 0)
    }
}

/// Calls `sink` with the canonical form (start point `(x, y)` and direction) of each unit edge
/// of a placed footprint's outline and of its interior cell lines clipped to the outline's open
/// interior. Outline vertices are lattice points and its edges run along axes and diagonals, so
/// the outline crosses a cell line only at lattice points: a unit edge of the line lies in the
/// open interior exactly when its midpoint does. An edge may be reported more than once.
fn footprint_edges(footprint: &PlacedFootprint, sink: &mut impl FnMut((i32, i32), Direction)) {
    let vertices = footprint.contour();
    let count = vertices.len();
    for i in 0..count {
        let (u, v) = (vertices[i], vertices[(i + 1) % count]);
        let (sx, sy) = ((v.x - u.x).signum(), (v.y - u.y).signum());
        let length = (v.x - u.x).abs().max((v.y - u.y).abs());
        for k in 0..length {
            let (start, direction) = canonical((u.x + k * sx, u.y + k * sy), (sx, sy));
            sink(start, direction);
        }
    }
    let corner = LatticePoint::cell_corner(
        i32::try_from(footprint.top_left.0).unwrap_or(i32::MAX),
        i32::try_from(footprint.top_left.1).unwrap_or(i32::MAX),
    );
    let (span_x, span_y) = (
        i32::from(footprint.width) * LATTICE_PER_CELL,
        i32::from(footprint.height) * LATTICE_PER_CELL,
    );
    let inside = |x2: i32, y2: i32| footprint.contains_open_scaled(i64::from(x2), i64::from(y2), 2);
    for i in 1..i32::from(footprint.width) {
        let x = corner.x + i * LATTICE_PER_CELL;
        for y in corner.y..corner.y + span_y {
            if inside(2 * x, 2 * y + 1) {
                sink((x, y), Direction::North);
            }
        }
    }
    for j in 1..i32::from(footprint.height) {
        let y = corner.y + j * LATTICE_PER_CELL;
        for x in corner.x..corner.x + span_x {
            if inside(2 * x + 1, 2 * y) {
                sink((x, y), Direction::East);
            }
        }
    }
}

/// The stored form of the unit edge from lattice point `a` one step along `step` (one of the
/// eight unit steps): its start point `(x, y)` and canonical direction.
#[inline]
fn canonical(a: (i32, i32), step: (i32, i32)) -> ((i32, i32), Direction) {
    let (x, y) = a;
    match step {
        (1, 0) => ((x, y), Direction::East),
        (-1, 0) => ((x - 1, y), Direction::East),
        (0, 1) => ((x, y), Direction::North),
        (0, -1) => ((x, y - 1), Direction::North),
        (1, 1) => ((x, y), Direction::NorthEast),
        (-1, -1) => ((x - 1, y - 1), Direction::NorthEast),
        (-1, 1) => ((x, y), Direction::NorthWest),
        _ => ((x + 1, y - 1), Direction::NorthWest),
    }
}

/// The constraint graph of a map on the quarter lattice (D-4): terrain from the whole-cell
/// [`EdgeGrid`], structures from [`StructureEdges`], each unit edge of the stronger kind.
struct ConstraintGraph<'a> {
    terrain: &'a EdgeGrid,
    structures: StructureEdges,
}

impl ConstraintGraph<'_> {
    /// The kind of the unit edge from lattice point `p` one step along `(dx, dy)`, if any.
    fn kind(&self, p: (i32, i32), (dx, dy): (i32, i32)) -> Option<EdgeKind> {
        let (start, direction) = canonical(p, (dx, dy));
        let structure = self
            .structures
            .contains(start, direction)
            .then_some(EdgeKind::Seam);
        stronger(self.terrain_kind(p, (dx, dy)), structure)
    }

    /// The terrain kind of a unit edge: that of the whole-cell edge containing it, if it lies on
    /// a cell line; diagonal edges are never terrain.
    fn terrain_kind(&self, (x, y): (i32, i32), (dx, dy): (i32, i32)) -> Option<EdgeKind> {
        let cell = |v: i32| usize::try_from(v.div_euclid(LATTICE_PER_CELL)).ok();
        match (dx, dy) {
            (1 | -1, 0) if y.rem_euclid(LATTICE_PER_CELL) == 0 => {
                let column = cell(x.min(x + dx))?;
                self.terrain.east(column, cell(y)?)
            }
            (0, 1 | -1) if x.rem_euclid(LATTICE_PER_CELL) == 0 => {
                let row = cell(y.min(y + dy))?;
                self.terrain.north(cell(x)?, row)
            }
            _ => None,
        }
    }

    /// Whether lattice point `p` must be a mesh vertex: its incident unit edges are not exactly
    /// one straight pair of one kind (today's rule, in eight directions).
    fn is_vertex(&self, p: (i32, i32)) -> bool {
        let steps = [
            (1, 0),
            (0, 1),
            (1, 1),
            (-1, 1),
            (-1, 0),
            (0, -1),
            (-1, -1),
            (1, -1),
        ];
        let kinds = steps.map(|step| self.kind(p, step));
        let count = kinds.iter().filter(|k| k.is_some()).count();
        if count == 0 {
            return false;
        }
        let straight = count == 2 && (0..4).any(|d| kinds[d].is_some() && kinds[d] == kinds[d + 4]);
        !straight
    }

    /// Every vertex in row-major `(y, x)` order: the cell corners that are vertices, and the
    /// quarter points of every closed footprint rectangle that are.
    fn vertices(&self, map: &GameMap) -> Vec<LatticePoint> {
        let (w, h) = (signed(map.width()), signed(map.height()));
        let mut candidates: Vec<(i32, i32)> = (0..=h)
            .flat_map(|y| (0..=w).map(move |x| (y * LATTICE_PER_CELL, x * LATTICE_PER_CELL)))
            .collect();
        let structures = map
            .structures()
            .values()
            .map(|placed| (placed.top_left, placed.width, placed.height));
        let nodes = map.resources().iter().filter_map(|resource| {
            resource
                .placed
                .map(|p| (p.footprint.top_left, p.footprint.width, p.footprint.height))
        });
        for (top_left, width, height) in structures.chain(nodes) {
            let corner = LatticePoint::cell_corner(signed(top_left.0), signed(top_left.1));
            let (pw, ph) = (
                i32::from(width) * LATTICE_PER_CELL,
                i32::from(height) * LATTICE_PER_CELL,
            );
            for y in corner.y..=corner.y + ph {
                for x in corner.x..=corner.x + pw {
                    candidates.push((y, x));
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        candidates
            .into_iter()
            .filter(|&(y, x)| self.is_vertex((x, y)))
            .map(|(y, x)| LatticePoint::new(x, y))
            .collect()
    }

    /// The maximal runs from each vertex in order, directions east, north, north-east and
    /// north-west (D-4).
    fn runs(&self, vertices: &[LatticePoint]) -> Vec<ConstraintRun> {
        let mut runs = Vec::new();
        for &from in vertices {
            for direction in Direction::ALL {
                let step = direction.step();
                let Some(kind) = self.kind((from.x, from.y), step) else {
                    continue;
                };
                let mut to = (from.x + step.0, from.y + step.1);
                while !self.is_vertex(to) {
                    to = (to.0 + step.0, to.1 + step.1);
                }
                runs.push(ConstraintRun {
                    from,
                    to: LatticePoint::new(to.0, to.1),
                    kind,
                });
            }
        }
        runs
    }
}

fn classify_cell(map: &GameMap, x: usize, y: usize) -> CellClass {
    let (cx, cy) = (
        u32::try_from(x).unwrap_or(u32::MAX),
        u32::try_from(y).unwrap_or(u32::MAX),
    );
    match map.cell(cx, cy) {
        Ok(cell) => CellClass {
            usable: map.terrain_step_is_connected((cx, cy), (cx, cy)),
            level: cell.cliff_level.0,
            ramp: cell.ramp,
            structure: cell
                .flags
                .intersects(CellFlags::OCCUPIED_BY_STRUCTURE | CellFlags::OCCUPIED_BY_RESOURCE),
        },
        Err(_) => CellClass {
            usable: false,
            level: 0,
            ramp: None,
            structure: false,
        },
    }
}

fn edge_kind(
    map: &GameMap,
    a: CellClass,
    b: CellClass,
    cell_a: (usize, usize),
    cell_b: (usize, usize),
) -> Option<EdgeKind> {
    if !a.usable && !b.usable {
        return None;
    }
    let to_cell = |c: (usize, usize)| {
        (
            u32::try_from(c.0).unwrap_or(u32::MAX),
            u32::try_from(c.1).unwrap_or(u32::MAX),
        )
    };
    if !a.usable || !b.usable || !map.terrain_step_is_connected(to_cell(cell_a), to_cell(cell_b)) {
        return Some(EdgeKind::Wall);
    }
    // Structures add their own geometry (`StructureEdges`); the edge grid is terrain only.
    if a.level != b.level || a.ramp != b.ramp {
        return Some(EdgeKind::Seam);
    }
    None
}

impl NavMesh {
    /// Builds the mesh of `map` as it stands, structures included (D-2, D-8).
    ///
    /// # Errors
    /// [`NavMeshError::TooLarge`] for a map side above [`NAV_MESH_MAX_DIMENSION`];
    /// [`NavMeshError::ConstraintRecovery`] if a constraint could not be inserted, which the
    /// constraint rules exclude.
    pub fn build(map: &GameMap) -> Result<Self, NavMeshError> {
        let (width, height) = (map.width(), map.height());
        if width > NAV_MESH_MAX_DIMENSION || height > NAV_MESH_MAX_DIMENSION {
            return Err(NavMeshError::TooLarge { width, height });
        }
        let edges = EdgeGrid::new(map);
        let graph = ConstraintGraph {
            terrain: &edges,
            structures: StructureEdges::new(map),
        };
        let mut tri = Triangulation::rectangle(
            signed(width) * LATTICE_PER_CELL,
            signed(height) * LATTICE_PER_CELL,
        );
        // Each inserted vertex's index, keyed by `(y, x)`; the rectangle's corners come first.
        let mut index: Vec<((i32, i32), u32)> = tri
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| ((p.y, p.x), u32::try_from(i).unwrap_or(NONE)))
            .collect();
        let vertices = graph.vertices(map);
        let mut hint = 0;
        for &p in &vertices {
            let before = tri.points.len();
            hint = tri.insert_point(p, hint);
            if tri.points.len() > before {
                index.push(((p.y, p.x), u32::try_from(before).unwrap_or(NONE)));
            }
        }
        index.sort_unstable();
        let runs = graph.runs(&vertices);
        for run in &runs {
            let index_of = |p: LatticePoint| {
                index
                    .binary_search_by_key(&(p.y, p.x), |&(key, _)| key)
                    .map_or(NONE, |i| index[i].1)
            };
            if !tri.insert_constraint(index_of(run.from), index_of(run.to), run.kind) {
                return Err(NavMeshError::ConstraintRecovery {
                    from: run.from,
                    to: run.to,
                });
            }
        }
        tri.make_delaunay();
        let contact_free = edges.contact_free_cells(map);
        Ok(Self::finish(map, tri, runs, contact_free))
    }

    fn finish(
        map: &GameMap,
        tri: Triangulation,
        runs: Vec<ConstraintRun>,
        contact_free: Vec<bool>,
    ) -> Self {
        let mut mesh = Self {
            width: map.width(),
            height: map.height(),
            vertices: tri.points,
            triangles: tri.triangles,
            adjacent: tri.adjacent,
            kinds: tri.kinds,
            labels: Vec::new(),
            vertex_triangle: tri.vertex_triangle,
            cell_triangle: Vec::new(),
            widths: Vec::new(),
            vertex_obstacle: Vec::new(),
            contact_free,
            runs,
        };
        mesh.labels = (0..mesh.triangles.len())
            .map(|t| mesh.label_triangle(map, t))
            .collect();
        mesh.index_cells();
        let all_known = |t: u32| mesh.labels[t as usize].kind == TriangleKind::Open;
        mesh.vertex_obstacle = (0..mesh.vertices.len())
            .map(|v| mesh.vertex_blocks(u32::try_from(v).unwrap_or(NONE), all_known))
            .collect();
        mesh.widths = (0..mesh.triangles.len())
            .map(|t| {
                let t = u32::try_from(t).unwrap_or(NONE);
                if all_known(t) {
                    [0, 1, 2].map(|k| mesh.width_at(t, k, all_known))
                } else {
                    [SquaredWidth::whole(0); 3]
                }
            })
            .collect();
        mesh
    }

    fn label_triangle(&self, map: &GameMap, t: usize) -> TriangleLabel {
        let [a, b, c] = self.triangles[t].map(|v| self.vertices[v as usize]);
        let (sx, sy) = (
            i64::from(a.x) + i64::from(b.x) + i64::from(c.x),
            i64::from(a.y) + i64::from(b.y) + i64::from(c.y),
        );
        // The centroid is the sum over three, in lattice units: its cell is the sum over
        // `3 * LATTICE_PER_CELL`, floored.
        let per_cell = 3 * i64::from(LATTICE_PER_CELL);
        let cx = u32::try_from(sx.div_euclid(per_cell))
            .unwrap_or(0)
            .min(self.width - 1);
        let cy = u32::try_from(sy.div_euclid(per_cell))
            .unwrap_or(0)
            .min(self.height - 1);
        let class = classify_cell(map, cx as usize, cy as usize);
        // A triangle in a structure's cell is the structure's when its centroid lies inside the
        // shape (exact on three times the centroid, D-4); a centroid is never on a constraint.
        let cell = map.cell(cx, cy).ok();
        // A standing node's triangles are the node's, tested first: a geyser under an
        // extractor stays closed to a player who does not know the extractor (D-5 M1).
        let resource = cell
            .and_then(|cell| cell.occupying_resource)
            .filter(|node| {
                map.resources()
                    .get(node.0 as usize)
                    .and_then(|resource| resource.placed)
                    .is_some_and(|placed| placed.footprint.contains_open_scaled(sx, sy, 3))
            });
        let occupant = cell
            .and_then(|cell| cell.occupying_structure)
            .and_then(|id| map.structures().get(&id))
            .filter(|placed| placed.contains_open_scaled(sx, sy, 3));
        let kind = if let Some(node) = resource {
            TriangleKind::Resource(node)
        } else if let Some(placed) = occupant {
            TriangleKind::Structure(placed.id)
        } else if class.usable {
            TriangleKind::Open
        } else {
            TriangleKind::Blocked
        };
        TriangleLabel {
            kind,
            cliff_level: class.level,
            ramp: class.ramp,
            cell: cy * self.width + cx,
        }
    }

    fn index_cells(&mut self) {
        let mut hint = 0;
        let mut index = Vec::with_capacity((self.width as usize) * (self.height as usize));
        for y in 0..self.height {
            for x in 0..self.width {
                let centre = QPoint {
                    x: (i64::from(x) << predicates::Q_BITS) + (1 << (predicates::Q_BITS - 1)),
                    y: (i64::from(y) << predicates::Q_BITS) + (1 << (predicates::Q_BITS - 1)),
                };
                hint = self.walk(centre, hint);
                index.push(hint);
            }
        }
        self.cell_triangle = index;
    }

    /// Map width in cells.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Map height in cells.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Number of vertices.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Every vertex, in index order.
    #[must_use]
    pub fn vertices(&self) -> &[LatticePoint] {
        &self.vertices
    }

    /// Counter-clockwise vertex indices of triangle `t`.
    #[must_use]
    pub fn triangle_vertices(&self, t: u32) -> [u32; 3] {
        self.triangles[t as usize]
    }

    /// Neighbours of triangle `t` across the edges opposite its local vertices; `u32::MAX` on
    /// the map border.
    #[must_use]
    pub fn triangle_neighbors(&self, t: u32) -> [u32; 3] {
        self.adjacent[t as usize]
    }

    /// Kinds of the edges of `t` opposite its local vertices.
    #[must_use]
    pub fn edge_kinds(&self, t: u32) -> [EdgeKind; 3] {
        self.kinds[t as usize]
    }

    /// Label of triangle `t`.
    #[must_use]
    pub fn label(&self, t: u32) -> TriangleLabel {
        self.labels[t as usize]
    }

    /// The maximal constraint runs the mesh was built from.
    #[must_use]
    pub fn constraint_runs(&self) -> &[ConstraintRun] {
        &self.runs
    }

    /// Cached width at corner `k` of `t` with every structure known (D-4).
    #[must_use]
    pub fn cached_width(&self, t: u32, k: usize) -> SquaredWidth {
        self.widths[t as usize][k]
    }

    /// Lattice position of vertex `v`.
    #[must_use]
    pub fn vertex(&self, v: u32) -> LatticePoint {
        self.vertices[v as usize]
    }

    pub(crate) fn corner(&self, t: u32, k: usize) -> LatticePoint {
        self.vertices[self.triangles[t as usize][k] as usize]
    }

    /// A 64-bit FNV-1a digest of the whole mesh, for determinism checks across targets.
    ///
    /// The digest is independent of the lattice's unit (plan `2026-11-18-contour-footprints`
    /// D-3): it eats each vertex as its raw Q32.32 world position and each width as its value in
    /// cells squared, reduced to lowest terms. A mesh whose coordinates are rescaled without
    /// changing its geometry therefore keeps its digest.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut eat = |value: u64| {
            for byte in value.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        };
        for v in &self.vertices {
            let world = v.to_sim();
            eat(world.x.raw().unsigned_abs());
            eat(world.y.raw().unsigned_abs());
        }
        for t in 0..self.triangles.len() {
            for k in 0..3 {
                eat(u64::from(self.triangles[t][k]));
                eat(u64::from(self.adjacent[t][k]));
                eat(edge_kind_code(self.kinds[t][k]));
                let (num, den) = self.widths[t][k].reduced_parts();
                eat(num);
                eat(den);
            }
            let label = self.labels[t];
            eat(match label.kind {
                TriangleKind::Blocked => 0,
                TriangleKind::Open => 1,
                TriangleKind::Structure(id) => 2 + u64::from(id.0),
                TriangleKind::Resource(node) => (1 << 33) + u64::from(node.0),
            });
            eat(u64::from(label.cliff_level) << 8 | label.ramp.map_or(0, |r| 1 + r as u64));
            eat(u64::from(label.cell));
        }
        hash
    }

    /// Triangle containing the world point `p`, or `None` off the map. A point on an edge or
    /// vertex reports one of the triangles touching it, deterministically.
    #[must_use]
    pub fn locate(&self, p: SimVec2) -> Option<u32> {
        let q = QPoint::from_sim(p);
        let limit_x = i64::from(self.width) << predicates::Q_BITS;
        let limit_y = i64::from(self.height) << predicates::Q_BITS;
        if q.x < 0 || q.y < 0 || q.x > limit_x || q.y > limit_y {
            return None;
        }
        let cx = u32::try_from(q.x >> predicates::Q_BITS)
            .unwrap_or(0)
            .min(self.width - 1);
        let cy = u32::try_from(q.y >> predicates::Q_BITS)
            .unwrap_or(0)
            .min(self.height - 1);
        let hint = self.cell_triangle[(cy * self.width + cx) as usize];
        Some(self.walk(q, hint))
    }

    /// Visibility walk from `hint` to a triangle containing `p` (D-5's jump-and-walk).
    // PERFORMANCE RATIONALE:
    // Why: A constrained triangulation can make a deterministic visibility walk cycle, and a
    // scan of every triangle per lookup would make point location O(triangles).
    // What: Start from the triangle of the point's cell, rotate the first edge tested each step
    // so the walk cannot cycle in practice, and fall back to a scan after a step cap.
    // Trade-off: An O(triangles) worst case that the cap bounds, for O(1) typical lookups.
    pub(crate) fn walk(&self, p: QPoint, hint: u32) -> u32 {
        let mut t = hint;
        let cap = 2 * self.triangles.len() + 16;
        'walk: for step in 0..cap {
            for j in 0..3 {
                let k = (j + step) % 3;
                let (a, b) = (
                    self.corner(t, next(k)).to_q(),
                    self.corner(t, prev(k)).to_q(),
                );
                if orient_q(a, b, p) < 0 {
                    let n = self.adjacent[t as usize][k];
                    if n == NONE {
                        break 'walk;
                    }
                    t = n;
                    continue 'walk;
                }
            }
            return t;
        }
        (0..self.triangles.len())
            .map(|t| u32::try_from(t).unwrap_or(NONE))
            .find(|&t| {
                (0..3).all(|k| {
                    orient_q(
                        self.corner(t, next(k)).to_q(),
                        self.corner(t, prev(k)).to_q(),
                        p,
                    ) >= 0
                })
            })
            .unwrap_or(hint)
    }

    /// Whether the edge opposite `k` in `t` blocks a request whose open triangles are `open`.
    pub(crate) fn edge_blocks(&self, t: u32, k: usize, open: impl Fn(u32) -> bool) -> bool {
        let n = self.adjacent[t as usize][k];
        n == NONE || self.kinds[t as usize][k] == EdgeKind::Wall || !open(t) || !open(n)
    }

    /// Whether no edge that blocks a request knowing every structure lies within the square of
    /// half-side `radius` world units round `p`: every cell the square touches (its boundary
    /// included) is open with no blocking side. `false` when the square leaves the map or a
    /// coordinate overflows. Exact: `true` implies that no blocking edge lies within `radius`
    /// of `p`; `false` decides nothing. `Pathfinder::wall_contact` answers `None` without a flood
    /// when this holds for an all-known request; public so tests compare it with the exhaustive
    /// reference.
    #[must_use]
    pub fn contact_free_near(&self, map: &GameMap, p: SimVec2, radius: SimScalar) -> bool {
        let corner = |sign_add: bool| {
            let shift = |v: SimScalar| {
                if sign_add {
                    v.checked_add(radius)
                } else {
                    v.checked_sub(radius)
                }
            };
            Some(SimVec2::new(shift(p.x)?, shift(p.y)?))
        };
        let (Some(low), Some(high)) = (corner(false), corner(true)) else {
            return false;
        };
        let (Ok((x0, y0)), Ok((x1, y1))) = (map.world_to_cell(low), map.world_to_cell(high)) else {
            return false;
        };
        let width = self.width as usize;
        (y0..=y1).all(|y| {
            (x0..=x1).all(|x| {
                self.contact_free
                    .get(y as usize * width + x as usize)
                    .copied()
                    .unwrap_or(false)
            })
        })
    }

    /// Whether some edge at vertex `v` blocks: whether `v` is an obstacle point.
    pub(crate) fn vertex_blocks(&self, v: u32, open: impl Fn(u32) -> bool + Copy) -> bool {
        let start = self.vertex_triangle[v as usize];
        let mut t = start;
        for _ in 0..self.triangles.len() {
            let Some(i) = self.triangles[t as usize].iter().position(|&w| w == v) else {
                return true;
            };
            if self.edge_blocks(t, next(i), open) || self.edge_blocks(t, prev(i), open) {
                return true;
            }
            t = self.adjacent[t as usize][next(i)];
            if t == start {
                return false;
            }
        }
        true
    }

    /// Demyen and Buro's width between the two edges at corner `k` of `t` (D-4), in cells
    /// squared, for a request whose open triangles are `open`.
    ///
    /// A corner that is not an obstacle point bounds nothing and reports
    /// [`SquaredWidth::UNBOUNDED`]; a passage past it is checked where it narrows.
    pub(crate) fn width_at(
        &self,
        t: u32,
        k: usize,
        open: impl Fn(u32) -> bool + Copy,
    ) -> SquaredWidth {
        let tv = self.triangles[t as usize];
        if !self.vertex_blocks(tv[k], open) {
            return SquaredWidth::UNBOUNDED;
        }
        let (c, a, b) = (
            self.corner(t, k),
            self.corner(t, next(k)),
            self.corner(t, prev(k)),
        );
        let mut d = SquaredWidth::UNBOUNDED;
        if self.vertex_blocks(tv[next(k)], open) {
            d = d.min(lattice_distance_sq(c, a));
        }
        if self.vertex_blocks(tv[prev(k)], open) {
            d = d.min(lattice_distance_sq(c, b));
        }
        if lattice_dot(a, c, b) <= 0 || lattice_dot(b, c, a) <= 0 {
            return d;
        }
        if self.edge_blocks(t, k, open) {
            return d.min(lattice_point_segment_width(c, a, b));
        }
        self.search_width(c, t, k, d, open, 0)
    }

    /// Demyen and Buro's `SearchWidth`: the nearest obstacle to `c` beyond edge `k` of `t`.
    fn search_width(
        &self,
        c: LatticePoint,
        t: u32,
        k: usize,
        d: SquaredWidth,
        open: impl Fn(u32) -> bool + Copy,
        depth: u32,
    ) -> SquaredWidth {
        let (u, v) = (self.corner(t, next(k)), self.corner(t, prev(k)));
        if lattice_dot(u, c, v) <= 0 || lattice_dot(v, c, u) <= 0 {
            return d;
        }
        let here = lattice_point_segment_width(c, u, v);
        if !here.less_than(d) {
            return d;
        }
        if self.edge_blocks(t, k, open) || depth >= WIDTH_SEARCH_DEPTH {
            // At the depth cap the edge is treated as an obstacle: the width can only shrink.
            return here;
        }
        let n = self.adjacent[t as usize][k];
        let q = self.triangles[t as usize][next(k)];
        let j = self.triangles[n as usize]
            .iter()
            .position(|&w| w == q)
            .map_or(0, next);
        let d = self.search_width(c, n, next(j), d, open, depth + 1);
        self.search_width(c, n, prev(j), d, open, depth + 1)
    }
}

/// Recursion bound of the width search; deeper edges count as obstacles (conservative).
const WIDTH_SEARCH_DEPTH: u32 = 48;

fn edge_kind_code(kind: EdgeKind) -> u64 {
    match kind {
        EdgeKind::Free => 0,
        EdgeKind::Seam => 1,
        EdgeKind::Wall => 2,
    }
}

/// Which triangles a path request may enter, by the knowledge rules of
/// `Pathfinder::find_path_with_vision` (invariants 1 and 2).
pub(crate) struct Access<'a> {
    mesh: &'a NavMesh,
    map: &'a GameMap,
    request: &'a PathRequestWithVision<'a>,
    start_cell: Option<(u32, u32)>,
    start_level: u8,
}

impl<'a> Access<'a> {
    pub(crate) fn new(
        mesh: &'a NavMesh,
        map: &'a GameMap,
        request: &'a PathRequestWithVision<'a>,
    ) -> Self {
        let start_cell = map.world_to_cell(request.start).ok();
        let start_level = start_cell
            .and_then(|(x, y)| map.cell(x, y).ok())
            .map_or(0, |cell| cell.cliff_level.0);
        Self {
            mesh,
            map,
            request,
            start_cell,
            start_level,
        }
    }

    /// Whether every structure counts as known and no exploration rule applies, so the cached
    /// widths are this request's widths.
    pub(crate) fn is_all_known(&self) -> bool {
        self.request.vision.is_none()
            && self.request.known_structures.is_none()
            && !self.request.block_unexplored_ramps
    }

    /// Whether the request may enter triangle `t`.
    pub(crate) fn open(&self, t: u32) -> bool {
        let label = self.mesh.labels[t as usize];
        let passable = match label.kind {
            // Resource nodes are public: closed to every request (D-9).
            TriangleKind::Blocked | TriangleKind::Resource(_) => false,
            TriangleKind::Open => true,
            TriangleKind::Structure(_) => !self.structure_known(label.cell),
        };
        passable && !self.unexplored_block(t, label)
    }

    fn structure_known(&self, cell: u32) -> bool {
        if let Some(mask) = self.request.known_structures {
            mask.get(cell as usize).copied().unwrap_or(false)
        } else if let Some(vision) = self.request.vision {
            let width = self.mesh.width;
            vision.visibility_at(cell % width, cell / width) == VisibilityState::Visible
        } else {
            true
        }
    }

    /// Invariant 1, per triangle: a ramp or higher-ground triangle is closed when any cell of
    /// its terrain under its bounding box is unexplored. This is coarser than the grid's
    /// per-cell rule (a recorded deviation); no production caller sets the flag.
    fn unexplored_block(&self, t: u32, label: TriangleLabel) -> bool {
        let (true, Some(vision)) = (self.request.block_unexplored_ramps, self.request.vision)
        else {
            return false;
        };
        if label.ramp.is_none() && label.cliff_level <= self.start_level {
            return false;
        }
        // The triangle's bounding box in lattice units, widened to whole cells: cells
        // `min..max` cover it.
        let corners = self.mesh.triangles[t as usize].map(|v| self.mesh.vertices[v as usize]);
        let floor_cell = |v: i32| v.div_euclid(LATTICE_PER_CELL);
        let ceil_cell = |v: i32| (v + LATTICE_PER_CELL - 1).div_euclid(LATTICE_PER_CELL);
        let min_x = corners
            .iter()
            .map(|p| floor_cell(p.x))
            .min()
            .unwrap_or(0)
            .max(0);
        let max_x = corners.iter().map(|p| ceil_cell(p.x)).max().unwrap_or(0);
        let min_y = corners
            .iter()
            .map(|p| floor_cell(p.y))
            .min()
            .unwrap_or(0)
            .max(0);
        let max_y = corners.iter().map(|p| ceil_cell(p.y)).max().unwrap_or(0);
        for y in min_y..max_y {
            for x in min_x..max_x {
                let cell = (x.unsigned_abs(), y.unsigned_abs());
                if Some(cell) == self.start_cell {
                    continue;
                }
                let same_terrain = self
                    .map
                    .cell(cell.0, cell.1)
                    .is_ok_and(|c| c.ramp == label.ramp && c.cliff_level.0 == label.cliff_level);
                if same_terrain
                    && vision.visibility_at(cell.0, cell.1) == VisibilityState::Unexplored
                {
                    return true;
                }
            }
        }
        false
    }

    /// Width at corner `k` of `t` for this request.
    pub(crate) fn width(&self, t: u32, k: usize) -> SquaredWidth {
        if self.is_all_known() {
            self.mesh.widths[t as usize][k]
        } else {
            self.mesh.width_at(t, k, |u| self.open(u))
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests: a shipped map or fixture that does not build is a failed test"
)]
mod edge_lookup_tests {
    //! The structure edge table against the sorted edge list it replaced (plan
    //! `2026-11-20-resource-footprints` Amendment 5, B-155), kept here as the oracle: every unit
    //! edge kind of random layouts with terrain, structures and resource nodes must agree.

    use super::*;
    use crate::footprint::{ResourceFootprint, ResourceFootprints};
    use crate::grid::{ResourceKind, ResourceNode};
    use crate::map_format::MapFile;
    use crate::StructureFootprint;
    use proptest::prelude::*;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use sim_types::{EntityId, FootprintShape, QuarterOutline, ResourceNodeId};

    /// The eight unit lattice steps.
    const STEPS: [(i32, i32); 8] = [
        (1, 0),
        (0, 1),
        (1, 1),
        (-1, 1),
        (-1, 0),
        (0, -1),
        (-1, -1),
        (1, -1),
    ];

    /// The lookup before B-155: the structure edges sorted by start point and direction and
    /// searched by bisection, over the same terrain.
    struct Oracle<'a> {
        graph: &'a ConstraintGraph<'a>,
        edges: Vec<((i32, i32), Direction)>,
    }

    impl<'a> Oracle<'a> {
        fn new(map: &GameMap, graph: &'a ConstraintGraph<'a>) -> Self {
            let mut edges = Vec::new();
            let mut push = |start, direction| edges.push((start, direction));
            for placed in map.structures().values() {
                footprint_edges(&placed.footprint(), &mut push);
            }
            for resource in map.resources() {
                if let Some(placed) = resource.placed {
                    footprint_edges(&placed.footprint, &mut push);
                }
            }
            edges.sort_unstable();
            edges.dedup();
            Self { graph, edges }
        }

        fn kind(&self, p: (i32, i32), step: (i32, i32)) -> Option<EdgeKind> {
            let structure = self
                .edges
                .binary_search(&canonical(p, step))
                .is_ok()
                .then_some(EdgeKind::Seam);
            stronger(self.graph.terrain_kind(p, step), structure)
        }
    }

    /// Asserts that the table and the oracle agree on every unit edge from every lattice point
    /// of `map` and of a two-point margin around it; returns the number of structure edges.
    fn assert_lookups_agree(map: &GameMap) -> usize {
        let terrain = EdgeGrid::new(map);
        let graph = ConstraintGraph {
            terrain: &terrain,
            structures: StructureEdges::new(map),
        };
        let oracle = Oracle::new(map, &graph);
        let (w, h) = (
            signed(map.width()) * LATTICE_PER_CELL,
            signed(map.height()) * LATTICE_PER_CELL,
        );
        for y in -2..=h + 2 {
            for x in -2..=w + 2 {
                for step in STEPS {
                    assert_eq!(
                        graph.kind((x, y), step),
                        oracle.kind((x, y), step),
                        "unit edge from ({x}, {y}) along {step:?}"
                    );
                }
            }
        }
        oracle.edges.len()
    }

    /// The 5.0.14 geyser outline in quarter cells, as the shipped catalog has it.
    fn geyser_outline() -> FootprintShape {
        FootprintShape::Outline(
            QuarterOutline::new(&[
                (-2, -4),
                (-3, -5),
                (-5, -3),
                (-4, -2),
                (-4, 2),
                (-5, 3),
                (-3, 5),
                (-2, 4),
                (2, 4),
                (3, 5),
                (5, 3),
                (4, 2),
                (4, -2),
                (5, -3),
                (3, -5),
                (2, -4),
            ])
            .expect("invariant: sixteen vertices fit an outline"),
        )
    }

    /// Minerals a 2×1 box anchored on the west cell, gas the 3×3 outline centred on its anchor.
    fn table() -> ResourceFootprints {
        ResourceFootprints::new(vec![
            (
                ResourceKind::Mineral,
                ResourceFootprint {
                    cells: (0, 0, 2, 1),
                    shape: FootprintShape::full(2, 1),
                    remove_when_empty: true,
                },
            ),
            (
                ResourceKind::Gas,
                ResourceFootprint {
                    cells: (-1, -1, 3, 3),
                    shape: geyser_outline(),
                    remove_when_empty: false,
                },
            ),
        ])
    }

    /// Structure footprints: full boxes and chamfered boxes of the shipped sizes.
    fn footprint(rng: &mut ChaCha8Rng) -> StructureFootprint {
        const SHAPES: [(u8, u8, u16, u16, u16); 7] = [
            (5, 5, 10, 10, 6),
            (3, 3, 6, 6, 2),
            (3, 3, 6, 6, 3),
            (2, 2, 4, 4, 2),
            (2, 2, 2, 2, 1),
            (1, 1, 2, 2, 0),
            (3, 2, 6, 4, 2),
        ];
        let (width, height, half_width_q, half_height_q, chamfer_q) =
            SHAPES[usize::try_from(rng.gen_range::<u32, _>(0..7)).unwrap_or(0)];
        if rng.gen_bool(0.3) {
            StructureFootprint::square(width, height)
        } else {
            StructureFootprint {
                width,
                height,
                shape: FootprintShape::Box {
                    half_width_q,
                    half_height_q,
                    chamfer_q,
                },
            }
        }
    }

    /// The default arena's cells and nodes, its nodes not placed.
    fn default_arena() -> GameMap {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/maps/default_arena.scmap.ron");
        MapFile::load(&path)
            .expect("invariant: the shipped arena loads")
            .decode()
            .expect("invariant: the shipped arena decodes")
            .to_game_map()
            .expect("invariant: the shipped arena is a valid map")
    }

    /// A random layout on the arena's terrain: its own nodes and up to 24 more, every node that
    /// fits placed, an extractor over the first standing geyser, then up to 16 structures, some
    /// flush with the map border. Returns the map, the standing nodes and the structures.
    fn layout(seed: u64) -> (GameMap, usize, usize) {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let arena = default_arena();
        let (width, height) = (arena.width(), arena.height());
        let mut nodes: Vec<ResourceNode> = arena.resources().to_vec();
        for _ in 0..rng.gen_range::<u32, _>(0..=24) {
            let kind = if rng.gen_bool(0.25) {
                ResourceKind::Gas
            } else {
                ResourceKind::Mineral
            };
            let cell = (
                rng.gen_range::<u32, _>(0..width),
                rng.gen_range::<u32, _>(0..height),
            );
            let half = |v: u32| SimScalar::from_raw((i64::from(v) << 32) + (1 << 31));
            nodes.push(ResourceNode {
                pos: SimVec2::new(half(cell.0), half(cell.1)),
                kind,
                remaining: 1_500,
                cell,
                placed: None,
            });
        }
        let mut map = GameMap::from_cells(
            width,
            height,
            arena.cells().to_vec(),
            nodes,
            arena.watchtowers().to_vec(),
        )
        .expect("invariant: the arena's own size and cells");
        let table = table();
        let count = map.resources().len();
        let ids = (0..count).map(|i| ResourceNodeId(u32::try_from(i).unwrap_or(u32::MAX)));
        let standing = ids
            .filter(|&id| map.place_one_resource_footprint(id, &table).is_ok())
            .count();
        let geyser = (0..count).find(|&i| {
            map.resources()[i].kind == ResourceKind::Gas && map.resources()[i].placed.is_some()
        });
        let mut structures = 0;
        if let Some(i) = geyser {
            let id = ResourceNodeId(u32::try_from(i).unwrap_or(u32::MAX));
            let extractor = StructureFootprint {
                width: 3,
                height: 3,
                shape: geyser_outline(),
            };
            structures += usize::from(
                map.place_structure_over_resource(EntityId(50_000), id, extractor)
                    .is_ok(),
            );
        }
        for attempt in 0..64u32 {
            if structures >= 16 {
                break;
            }
            let footprint = footprint(&mut rng);
            let cell = if rng.gen_bool(0.2) {
                (
                    width - u32::from(footprint.width),
                    rng.gen_range::<u32, _>(0..height),
                )
            } else {
                (
                    rng.gen_range::<u32, _>(0..width),
                    rng.gen_range::<u32, _>(0..height),
                )
            };
            structures += usize::from(
                map.place_structure(EntityId(40_000 + attempt), cell, footprint)
                    .is_ok(),
            );
        }
        (map, standing, structures)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]

        /// The table answers every unit edge of a random layout as the sorted list did.
        #[test]
        fn the_table_agrees_with_the_sorted_list_on_random_layouts(seed in any::<u64>()) {
            let (map, standing, structures) = layout(seed);
            prop_assert!(standing > 0 && structures > 0, "a layout with nodes and structures");
            let edges = assert_lookups_agree(&map);
            prop_assert!(edges > 0);
        }
    }

    /// The shipped arena with its nodes standing: the case B-155 measured.
    #[test]
    fn the_table_agrees_with_the_sorted_list_on_the_arena_with_its_nodes() {
        let mut map = default_arena();
        map.place_resource_footprints(&table())
            .expect("invariant: the arena's nodes fit the test table");
        assert!(assert_lookups_agree(&map) > 0);
    }

    /// A structure flush with the north-east corner has edges on the map's east and north
    /// lines, which only the table's extra column and row hold; points beyond have none.
    #[test]
    fn edges_on_the_map_border_are_found_and_points_beyond_have_none() {
        let side = 8;
        let cells = vec![crate::grid::MapCell::default_ground(crate::grid::CliffLevel(1)); 64];
        let mut map = GameMap::from_cells(side, side, cells, Vec::new(), Vec::new())
            .expect("invariant: a valid size");
        map.place_structure(EntityId(1), (6, 6), StructureFootprint::square(2, 2))
            .expect("invariant: open ground");
        let edges = StructureEdges::new(&map);
        let east_line = signed(side) * LATTICE_PER_CELL;
        assert!(edges.contains((east_line, east_line - 1), Direction::North));
        assert!(edges.contains((east_line - 1, east_line), Direction::East));
        assert!(!edges.contains((east_line + 1, east_line - 1), Direction::North));
        assert!(!edges.contains((-1, 0), Direction::East));
        assert!(!edges.contains((0, -1), Direction::North));
        assert!(!edges.contains((i32::MAX, i32::MAX), Direction::NorthEast));
        assert!(!edges.contains((i32::MIN, 0), Direction::NorthWest));
        // The outline: four sides of eight quarter steps; the interior: one line each way.
        assert_eq!(assert_lookups_agree(&map), 4 * 8 + 2 * 8);
    }
}
