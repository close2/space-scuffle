//! Constrained Delaunay navigation mesh (the `StarCraft` II design of plan
//! `docs/plans/2026-11-09-sc2-pathfinding.md`, Part B).
//!
//! # Responsibility
//! Owns the triangle mesh of a map: its exact construction from terrain and structure footprints
//! (D-2, `cdt.rs`), its triangle labels and per-request passability (knowledge rules of
//! `find_path_with_vision`), point location through a per-cell triangle index (D-5), and the
//! radius-independent triangle widths of Demyen and Buro (D-4). The search (`search.rs`), the
//! radius funnel (`funnel.rs`) and the segment predicate (`segment.rs`) read it.
//!
//! # How it fits
//! [`crate::Pathfinder`] builds a [`NavMesh`] with its clearance fields and rebuilds it on every
//! structure change (D-8). Until the switch (WP9) nothing in the simulation reads it: it is
//! derived, unhashed state, so results stay bit-identical.
//!
//! # Invariants
//! - Every vertex is a lattice point and every constraint lies on cell edges, so construction
//!   uses exact integer predicates only (`predicates.rs`) and no Steiner points.
//! - Constraints are the map border, every edge where terrain does not connect, every edge where
//!   the `(cliff_level, ramp)` tag changes, and every edge of every structure cell. Each triangle
//!   therefore lies in one region of uniform label, and a structure triangle in one cell.
//! - The mesh is a pure function of the map's cells: structure insertion order cannot matter,
//!   and removing a structure restores the terrain mesh exactly (its cells' flags are restored).
//! - Storage is `Vec`s indexed by `u32`; loops run in index order; no float, no hashed
//!   collection (charter §3.1).

#![allow(
    clippy::many_single_char_names,
    reason = "computational geometry names points, vertices and triangles by single letters, \
              as the papers it implements do (Demyen and Buro 2006, Sloan 1993)"
)]

pub mod cdt;
pub mod funnel;
pub mod predicates;
pub mod search;
pub mod segment;

use sim_types::{EntityId, SimVec2, VisibilityState};
use thiserror::Error;

use crate::grid::{CellFlags, GameMap, RampDir};
use crate::pathfinding::PathRequestWithVision;
use cdt::{Triangulation, NONE};
use predicates::{
    lattice_distance_sq, lattice_dot, lattice_point_segment_width, orient_q, LatticePoint, QPoint,
    SquaredWidth, NAV_MESH_MAX_DIMENSION,
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

/// One maximal constraint run: an axis-aligned segment between two mesh vertices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstraintRun {
    /// Lower-left endpoint.
    pub from: LatticePoint,
    /// Upper-right endpoint.
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

    fn west(&self, x: usize, y: usize) -> Option<EdgeKind> {
        x.checked_sub(1).and_then(|x| self.east(x, y))
    }

    fn south(&self, x: usize, y: usize) -> Option<EdgeKind> {
        y.checked_sub(1).and_then(|y| self.north(x, y))
    }

    /// Whether lattice point `(x, y)` must be a mesh vertex: a constraint ends, turns, branches
    /// or changes kind there.
    fn is_vertex(&self, x: usize, y: usize) -> bool {
        let (w, e, s, n) = (
            self.west(x, y),
            self.east(x, y),
            self.south(x, y),
            self.north(x, y),
        );
        let count = [w, e, s, n].iter().filter(|k| k.is_some()).count();
        if count == 0 {
            return false;
        }
        let straight_x = count == 2 && w.is_some() && w == e;
        let straight_y = count == 2 && s.is_some() && s == n;
        !(straight_x || straight_y)
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
            structure: cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE),
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
    if a.level != b.level || a.ramp != b.ramp || a.structure || b.structure {
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
        let (w, h) = (width as usize, height as usize);
        let mut tri = Triangulation::rectangle(signed(width), signed(height));
        let mut lattice_vertex = vec![NONE; (w + 1) * (h + 1)];
        for (corner, (x, y)) in [(0, 0), (w, 0), (w, h), (0, h)].into_iter().enumerate() {
            lattice_vertex[y * (w + 1) + x] = u32::try_from(corner).unwrap_or(NONE);
        }
        let mut hint = 0;
        for y in 0..=h {
            for x in 0..=w {
                if lattice_vertex[y * (w + 1) + x] != NONE || !edges.is_vertex(x, y) {
                    continue;
                }
                let before = tri.points.len();
                hint = tri.insert_point(LatticePoint::new(signed_usize(x), signed_usize(y)), hint);
                if tri.points.len() > before {
                    lattice_vertex[y * (w + 1) + x] = u32::try_from(before).unwrap_or(NONE);
                }
            }
        }
        let runs = collect_runs(&edges);
        for run in &runs {
            let index_of = |p: LatticePoint| {
                lattice_vertex[usize::try_from(p.y).unwrap_or(0) * (w + 1)
                    + usize::try_from(p.x).unwrap_or(0)]
            };
            if !tri.insert_constraint(index_of(run.from), index_of(run.to), run.kind) {
                return Err(NavMeshError::ConstraintRecovery {
                    from: run.from,
                    to: run.to,
                });
            }
        }
        tri.make_delaunay();
        Ok(Self::finish(map, tri, runs))
    }

    fn finish(map: &GameMap, tri: Triangulation, runs: Vec<ConstraintRun>) -> Self {
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
        let cx = u32::try_from(sx.div_euclid(3))
            .unwrap_or(0)
            .min(self.width - 1);
        let cy = u32::try_from(sy.div_euclid(3))
            .unwrap_or(0)
            .min(self.height - 1);
        let class = classify_cell(map, cx as usize, cy as usize);
        let kind = if !class.usable {
            TriangleKind::Blocked
        } else if class.structure {
            map.cell(cx, cy)
                .ok()
                .and_then(|cell| cell.occupying_structure)
                .map_or(TriangleKind::Blocked, TriangleKind::Structure)
        } else {
            TriangleKind::Open
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
            eat(u64::from(v.x.unsigned_abs()) << 32 | u64::from(v.y.unsigned_abs()));
        }
        for t in 0..self.triangles.len() {
            for k in 0..3 {
                eat(u64::from(self.triangles[t][k]));
                eat(u64::from(self.adjacent[t][k]));
                eat(edge_kind_code(self.kinds[t][k]));
                let (num, den) = self.widths[t][k].parts();
                eat(num);
                eat(den);
            }
            let label = self.labels[t];
            eat(match label.kind {
                TriangleKind::Blocked => 0,
                TriangleKind::Open => 1,
                TriangleKind::Structure(id) => 2 + u64::from(id.0),
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

fn signed_usize(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn collect_runs(edges: &EdgeGrid) -> Vec<ConstraintRun> {
    let mut runs = Vec::new();
    for y in 0..=edges.height {
        for x in 0..=edges.width {
            if !edges.is_vertex(x, y) {
                continue;
            }
            if let Some(kind) = edges.east(x, y) {
                let mut end = x + 1;
                while !edges.is_vertex(end, y) {
                    end += 1;
                }
                runs.push(ConstraintRun {
                    from: LatticePoint::new(signed_usize(x), signed_usize(y)),
                    to: LatticePoint::new(signed_usize(end), signed_usize(y)),
                    kind,
                });
            }
            if let Some(kind) = edges.north(x, y) {
                let mut end = y + 1;
                while !edges.is_vertex(x, end) {
                    end += 1;
                }
                runs.push(ConstraintRun {
                    from: LatticePoint::new(signed_usize(x), signed_usize(y)),
                    to: LatticePoint::new(signed_usize(x), signed_usize(end)),
                    kind,
                });
            }
        }
    }
    runs
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
            TriangleKind::Blocked => false,
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
        let corners = self.mesh.triangles[t as usize].map(|v| self.mesh.vertices[v as usize]);
        let min_x = corners.iter().map(|p| p.x).min().unwrap_or(0).max(0);
        let max_x = corners.iter().map(|p| p.x).max().unwrap_or(0);
        let min_y = corners.iter().map(|p| p.y).min().unwrap_or(0).max(0);
        let max_y = corners.iter().map(|p| p.y).max().unwrap_or(0);
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
