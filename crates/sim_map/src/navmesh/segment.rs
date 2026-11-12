//! The mesh segment predicate (plan D-7): may a disc sweep along a straight segment? And its
//! contact query: which wall point is nearest a disc?
//!
//! # Responsibility
//! Decides whether a disc of radius `r` centred on the segment `[a, b]` stays inside triangles
//! the request may enter and at least `r` from every edge that blocks the request; and finds,
//! for a disc at a point, the nearest point of the blocking edges within `r` (plan
//! `2026-11-11-contact-and-missiles` D-5).
//!
//! # How it fits
//! The route search validates every funnel leg with it, and WP9 makes it the predicate of
//! `Pathfinder::segment_is_passable`, so routes and movement keep agreeing. The contact query
//! is `Pathfinder::wall_contact`, with which movement keeps a ground body's disc from going
//! deeper into a wall (bug B-113).
//!
//! # Invariants
//! - Exact: geometry is compared on the Q16 grid in `i128` (`predicates.rs`); the radius rounds
//!   up to the grid, so rounding can only refuse, never admit.
//! - Complete: a blocking edge closer than `r` is found. The segment and the short path from it to
//!   that edge cross only edges within `r` of the segment, so a flood from the start triangle
//!   across non-blocking edges within `r` reaches it (or a nearer blocking edge first).
//! - Allocation-free in steady state: the flood reuses [`SegmentScratch`].

use sim_types::{SimScalar, SimVec2};

use super::predicates::{segments_within, LatticePoint, QPoint, Q_BITS};
use super::{next, prev, Access, NavMesh};

/// Reusable flood state for [`segment_clear`].
#[derive(Debug, Default)]
pub struct SegmentScratch {
    stamp: Vec<u32>,
    generation: u32,
    stack: Vec<u32>,
}

impl SegmentScratch {
    fn begin(&mut self, triangles: usize) {
        if self.stamp.len() != triangles {
            self.stamp.clear();
            self.stamp.resize(triangles, 0);
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.stamp.fill(0);
            self.generation = 1;
        }
        self.stack.clear();
    }

    fn seen(&self, t: u32) -> bool {
        self.stamp[t as usize] == self.generation
    }

    fn visit(&mut self, t: u32) -> bool {
        let slot = &mut self.stamp[t as usize];
        if *slot == self.generation {
            return false;
        }
        *slot = self.generation;
        self.stack.push(t);
        true
    }
}

/// A radius in Q16 units, rounded up.
pub(crate) fn radius_q(radius: SimScalar) -> i64 {
    let shift = 32 - Q_BITS;
    let raw = radius.raw().max(0);
    (raw >> shift) + i64::from(raw & ((1 << shift) - 1) != 0)
}

/// Whether a disc of `radius` world units may sweep from `a` to `b` under `access`.
pub(crate) fn segment_clear(
    mesh: &NavMesh,
    access: &Access<'_>,
    scratch: &mut SegmentScratch,
    a: SimVec2,
    b: SimVec2,
    radius: SimScalar,
) -> bool {
    let Some(start) = mesh.locate(a) else {
        return false;
    };
    if mesh.locate(b).is_none() || !access.open(start) {
        return false;
    }
    let (qa, qb) = (QPoint::from_sim(a), QPoint::from_sim(b));
    let r = radius_q(radius);
    scratch.begin(mesh.triangle_count());
    scratch.visit(start);
    while let Some(t) = scratch.stack.pop() {
        for k in 0..3 {
            let n = mesh.triangle_neighbors(t)[k];
            let blocks = mesh.edge_blocks(t, k, |x| access.open(x));
            // PERFORMANCE RATIONALE:
            // Why: The exact capsule test is the predicate's main cost, and every triangle's edge
            // back to the one it was reached from would repeat it with nothing to learn.
            // What: An open edge to a triangle already reached is skipped before the test; a
            // blocking edge is always tested, so the answer and the triangles reached are
            // unchanged.
            // Trade-off: One cheap edge lookup before the test instead of after it.
            if !blocks && scratch.seen(n) {
                continue;
            }
            let (u, v) = (
                mesh.corner(t, next(k)).to_q(),
                mesh.corner(t, prev(k)).to_q(),
            );
            if !segments_within(qa, qb, u, v, r) {
                continue;
            }
            if blocks {
                return false;
            }
            scratch.visit(n);
        }
    }
    true
}

/// Where a disc touches the walls: the nearest point of the blocking edges within its radius
/// (plan `2026-11-11-contact-and-missiles` D-5, bug B-113).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallContact {
    /// The wall point nearest the disc's centre, in world units. It is exact at an edge's end;
    /// inside an edge its coordinates are rounded towards negative infinity to the raw Q32.32
    /// grid.
    pub point: SimVec2,
    /// The squared distance from the centre to the nearest wall in raw Q32.32 units squared
    /// (2⁻⁶⁴ square world units), exact and rounded down: the squared perpendicular distance for
    /// a point inside an edge, the squared distance to the end otherwise.
    pub distance_sq: i128,
    /// The unit normal of the nearest edge, pointing to its open side (into the open triangle
    /// the edge bounds), for a centre that lies on the wall. Of two equally near edges, the
    /// first found.
    pub normal: SimVec2,
}

/// The nearest point of the lattice segment `[u, v]` to the world point `p`, and its exact
/// squared distance from `p` in raw Q32.32 units squared (see [`WallContact`]).
fn nearest_on_lattice_segment(p: SimVec2, u: LatticePoint, v: LatticePoint) -> (SimVec2, i128) {
    let (px, py) = (i128::from(p.x.raw()), i128::from(p.y.raw()));
    let (ux, uy) = (i128::from(u.x) << 32, i128::from(u.y) << 32);
    let (vx, vy) = (i128::from(v.x) << 32, i128::from(v.y) << 32);
    let (dx, dy) = (i128::from(v.x - u.x), i128::from(v.y - u.y));
    let length_sq = dx * dx + dy * dy;
    // A lattice coordinate is below 2^12 cells and a map coordinate below 2^44 raw units, so
    // `along` and `cross` stay below 2^58 and `cross²` below 2^116.
    let along = (px - ux) * dx + (py - uy) * dy;
    let squared = |x: i128, y: i128| x * x + y * y;
    let to_sim = |x: i128, y: i128| {
        SimVec2::new(
            SimScalar::from_raw(i64::try_from(x).unwrap_or(i64::MAX)),
            SimScalar::from_raw(i64::try_from(y).unwrap_or(i64::MAX)),
        )
    };
    if length_sq == 0 || along <= 0 {
        return (to_sim(ux, uy), squared(px - ux, py - uy));
    }
    if along >= length_sq << 32 {
        return (to_sim(vx, vy), squared(px - vx, py - vy));
    }
    let cross = (px - ux) * dy - (py - uy) * dx;
    let point = to_sim(
        ux + (along * dx).div_euclid(length_sq),
        uy + (along * dy).div_euclid(length_sq),
    );
    (point, (cross * cross).div_euclid(length_sq))
}

/// The unit normal of edge `k` of triangle `t`, pointing into `t`.
fn inward_normal(mesh: &NavMesh, t: u32, k: usize) -> SimVec2 {
    let (u, v, w) = (
        mesh.corner(t, next(k)),
        mesh.corner(t, prev(k)),
        mesh.corner(t, k),
    );
    let (dx, dy) = (v.x - u.x, v.y - u.y);
    let side = i64::from(-dy) * i64::from(w.x - u.x) + i64::from(dx) * i64::from(w.y - u.y);
    let (nx, ny) = if side >= 0 { (-dy, dx) } else { (dy, -dx) };
    SimVec2::from_i32(nx, ny).normalize_or_zero()
}

/// The squared radius in raw Q32.32 units squared, comparable with [`WallContact::distance_sq`].
fn radius_sq_raw(radius: SimScalar) -> i128 {
    let raw = i128::from(radius.raw().max(0));
    raw * raw
}

/// Keeps in `best` the nearer of it and the blocking edge `k` of `t`, if that edge lies within
/// `radius_sq` of `p`. A tie keeps the edge found first.
fn keep_nearest(
    mesh: &NavMesh,
    best: &mut Option<WallContact>,
    (t, k): (u32, usize),
    p: SimVec2,
    radius_sq: i128,
) {
    let (u, v) = (mesh.corner(t, next(k)), mesh.corner(t, prev(k)));
    let (point, distance_sq) = nearest_on_lattice_segment(p, u, v);
    if distance_sq > radius_sq || best.is_some_and(|known| known.distance_sq <= distance_sq) {
        return;
    }
    *best = Some(WallContact {
        point,
        distance_sq,
        normal: inward_normal(mesh, t, k),
    });
}

/// Where a disc of `radius` centred on `p` touches the edges that block `access` (D-5), or
/// `None` when no blocking edge lies within `radius`, or `p` is off the mesh or in a closed
/// triangle.
///
/// The flood is [`segment_clear`]'s for the degenerate segment `[p, p]`, run to completion: it
/// measures every blocking edge it meets within `radius` and keeps the nearest. The straight
/// line from `p` to the nearest wall point crosses only open edges nearer than it, so the flood
/// reaches that edge (the module's completeness argument). The flood prunes with the Q16
/// predicate at a radius two Q16 units larger, which covers the rounding of `p` down to the
/// Q16 grid, and measures each edge exactly, so its distance equals that of
/// [`nearest_blocking_point_exhaustive`].
pub(crate) fn nearest_blocking_point(
    mesh: &NavMesh,
    access: &Access<'_>,
    scratch: &mut SegmentScratch,
    p: SimVec2,
    radius: SimScalar,
) -> Option<WallContact> {
    let start = mesh.locate(p).filter(|&t| access.open(t))?;
    let qp = QPoint::from_sim(p);
    let reach = radius_q(radius).saturating_add(2);
    let radius_sq = radius_sq_raw(radius);
    let mut best = None;
    scratch.begin(mesh.triangle_count());
    scratch.visit(start);
    while let Some(t) = scratch.stack.pop() {
        for k in 0..3 {
            let n = mesh.triangle_neighbors(t)[k];
            let blocks = mesh.edge_blocks(t, k, |x| access.open(x));
            if !blocks && scratch.seen(n) {
                continue;
            }
            let (u, v) = (
                mesh.corner(t, next(k)).to_q(),
                mesh.corner(t, prev(k)).to_q(),
            );
            if !segments_within(qp, qp, u, v, reach) {
                continue;
            }
            if blocks {
                keep_nearest(mesh, &mut best, (t, k), p, radius_sq);
            } else {
                scratch.visit(n);
            }
        }
    }
    best
}

/// The reference of [`nearest_blocking_point`]: the nearest wall from every blocking edge of
/// every triangle open to `access`, in triangle order, with no flood. A wall of another open
/// region is never nearer than the first wall the straight line to it crosses, so the distance
/// is the flood's. O(triangles): for tests and diagnostic tools.
pub(crate) fn nearest_blocking_point_exhaustive(
    mesh: &NavMesh,
    access: &Access<'_>,
    p: SimVec2,
    radius: SimScalar,
) -> Option<WallContact> {
    mesh.locate(p).filter(|&t| access.open(t))?;
    let radius_sq = radius_sq_raw(radius);
    let mut best = None;
    let count = u32::try_from(mesh.triangle_count()).unwrap_or(u32::MAX);
    for t in (0..count).filter(|&t| access.open(t)) {
        for k in 0..3 {
            if mesh.edge_blocks(t, k, |x| access.open(x)) {
                keep_nearest(mesh, &mut best, (t, k), p, radius_sq);
            }
        }
    }
    best
}
