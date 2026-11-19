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
//! - The same flood, run to completion, lists the obstacle vertices near a funnel leg
//!   ([`obstacle_vertices_near`], plan `2026-11-19-funnel-fallbacks` D-3): an obstacle vertex is an
//!   end of a blocking edge, and the short path from the segment to a vertex within reach crosses
//!   only edges within reach, so the flood meets the vertex's blocking edge, or a blocking edge
//!   between the segment and the vertex first.

use sim_types::{SimScalar, SimVec2};

use super::cdt::NONE;
use super::predicates::{
    point_segment_distance_sq, segments_within, LatticePoint, QPoint, LATTICE_BITS, Q_BITS,
};
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

/// The obstacle vertices within `rho` of the segment `[a, b]` under `access` (plan
/// `2026-11-19-funnel-fallbacks` D-3), in index order and without repeats, into `out` (cleared
/// first).
///
/// The flood is [`segment_clear`]'s, seeded in the triangle of `a`, or of `b` when `a`'s is
/// closed or off the mesh, and run to completion: it crosses every open edge within reach of the
/// segment and keeps each end of a blocking edge it meets that lies within reach. The reach is
/// `rho` rounded up to the Q16 grid plus two Q16 units, which covers the rounding of `a` and `b`
/// down to the grid, so the list is a superset of the obstacle vertices strictly within `rho` that
/// the segment sees; the funnel measures each exactly. A vertex hidden behind a blocking edge
/// nearer the segment is not listed: that edge is the obstacle there. Both ends closed or off the
/// mesh: the list is empty, and such a leg fails the segment predicate anyway.
///
/// Cost: O(triangles within `rho` of the segment), with no allocation once `out` and `scratch`
/// have grown.
// PERFORMANCE RATIONALE:
// Why: The funnel's refinement asks this once per leg per round on every channel TA* pulls,
// and a scan of every obstacle vertex of the mesh made the prototype's routes five times slower.
// What: A flood bounded by the leg's neighbourhood, the segment predicate's own, instead of a
// scan of the mesh's vertices.
// Trade-off: Vertices behind a nearer blocking edge are not listed (documented above), and the
// flood shares the predicate's completeness argument instead of a simpler exhaustive one.
pub(crate) fn obstacle_vertices_near(
    mesh: &NavMesh,
    access: &Access<'_>,
    scratch: &mut SegmentScratch,
    (a, b): (SimVec2, SimVec2),
    rho: SimScalar,
    out: &mut Vec<u32>,
) {
    out.clear();
    let Some(start) = [a, b]
        .into_iter()
        .find_map(|p| mesh.locate(p).filter(|&t| access.open(t)))
    else {
        return;
    };
    let (qa, qb) = (QPoint::from_sim(a), QPoint::from_sim(b));
    let reach = radius_q(rho).saturating_add(2);
    scratch.begin(mesh.triangle_count());
    scratch.visit(start);
    while let Some(t) = scratch.stack.pop() {
        let corners = mesh.triangle_vertices(t);
        for k in 0..3 {
            let n = mesh.triangle_neighbors(t)[k];
            let blocks = mesh.edge_blocks(t, k, |x| access.open(x));
            if !blocks && scratch.seen(n) {
                continue;
            }
            let (u, v) = (corners[next(k)], corners[prev(k)]);
            let (uq, vq) = (mesh.vertex(u).to_q(), mesh.vertex(v).to_q());
            if !segments_within(qa, qb, uq, vq, reach) {
                continue;
            }
            if !blocks {
                scratch.visit(n);
                continue;
            }
            for (w, wq) in [(u, uq), (v, vq)] {
                if point_segment_distance_sq(wq, qa, qb).is_below(reach) {
                    out.push(w);
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
}

/// A point of a blocking edge near a vertex: where a gate line of the zero-room retry comes from
/// (plan `2026-11-19-funnel-fallbacks` D-2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NearFeature {
    /// The edge's point nearest the vertex, in world units: exact at an edge's end and on an
    /// axis-aligned edge, rounded towards negative infinity to the raw grid otherwise.
    pub point: SimVec2,
    /// Its squared distance from the vertex in raw Q32.32 units squared, rounded down.
    pub distance_sq: i128,
}

/// The open triangles that have mesh vertex `v` as a corner, marked in `scratch` and pushed on
/// its flood stack: the fan of `v`, walked both ways from `v`'s recorded triangle until the
/// mesh's border or the walk closes.
fn seed_open_fan(mesh: &NavMesh, access: &Access<'_>, scratch: &mut SegmentScratch, v: u32) {
    let start = mesh.vertex_triangle[v as usize];
    for turn in [next, prev] {
        let mut t = start;
        for _ in 0..mesh.triangles.len() {
            let Some(i) = mesh.triangles[t as usize].iter().position(|&w| w == v) else {
                break;
            };
            if access.open(t) {
                scratch.visit(t);
            }
            t = mesh.adjacent[t as usize][turn(i)];
            if t == NONE || t == start {
                break;
            }
        }
    }
}

/// The nearest points of the blocking edges that do not end at mesh vertex `v` and come
/// closer than `reach` to it, in order of position and without repeats, into `out` (cleared
/// first): the features across a pinch at `v` (plan `2026-11-19-funnel-fallbacks` D-2.3).
///
/// The flood is [`nearest_blocking_point`]'s for the point `v`, seeded in every open triangle of
/// `v`'s fan and run to completion; it crosses open edges within `reach` of `v` (plus two Q16
/// units) and measures each blocking edge it meets exactly. The straight line from `v` to a
/// feature crosses only open edges nearer than it, so every feature `v` sees is listed.
// PERFORMANCE RATIONALE:
// Why: The zero-room retry needs the features near each vertex it rounds, and a scan of every
// blocking edge of the mesh per vertex is linear in the mesh.
// What: The segment predicate's flood around the vertex, bounded by `reach`.
// Trade-off: Features hidden behind a nearer blocking edge are not listed; that edge is listed
// instead and bounds the route there.
pub(crate) fn blocking_features_near(
    mesh: &NavMesh,
    access: &Access<'_>,
    scratch: &mut SegmentScratch,
    (v, reach): (u32, SimScalar),
    out: &mut Vec<NearFeature>,
) {
    out.clear();
    let centre = mesh.vertex(v);
    let (p, qv) = (centre.to_sim(), centre.to_q());
    let reach_q = radius_q(reach).saturating_add(2);
    let reach_sq = radius_sq_raw(reach);
    scratch.begin(mesh.triangle_count());
    seed_open_fan(mesh, access, scratch, v);
    while let Some(t) = scratch.stack.pop() {
        let corners = mesh.triangle_vertices(t);
        for k in 0..3 {
            let n = mesh.triangle_neighbors(t)[k];
            let blocks = mesh.edge_blocks(t, k, |x| access.open(x));
            if !blocks && scratch.seen(n) {
                continue;
            }
            let (a, b) = (corners[next(k)], corners[prev(k)]);
            let (u, w) = (mesh.vertex(a), mesh.vertex(b));
            if !segments_within(qv, qv, u.to_q(), w.to_q(), reach_q) {
                continue;
            }
            if !blocks {
                scratch.visit(n);
            } else if a != v && b != v {
                let (point, distance_sq) = nearest_on_lattice_segment(p, u, w);
                if distance_sq < reach_sq {
                    out.push(NearFeature { point, distance_sq });
                }
            }
        }
    }
    out.sort_unstable_by_key(|f| (f.point.x, f.point.y));
    out.dedup_by_key(|f| f.point);
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
    // A lattice unit is a quarter cell, 2^30 raw units.
    const SHIFT: u32 = 32 - LATTICE_BITS;
    let (px, py) = (i128::from(p.x.raw()), i128::from(p.y.raw()));
    let (ux, uy) = (i128::from(u.x) << SHIFT, i128::from(u.y) << SHIFT);
    let (vx, vy) = (i128::from(v.x) << SHIFT, i128::from(v.y) << SHIFT);
    let (dx, dy) = (i128::from(v.x - u.x), i128::from(v.y - u.y));
    let length_sq = dx * dx + dy * dy;
    // A lattice coordinate is below 2^14 quarter cells and a map coordinate below 2^44 raw
    // units, so `along` and `cross` stay below 2^60 and `cross²` below 2^120. `along` is the
    // projection times `length_sq` in raw units over 2^SHIFT.
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
    if along >= length_sq << SHIFT {
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
    // The direction is reduced to lowest terms before it is normalised, so the normal depends
    // on the edge's direction only, never on the lattice's unit or the edge's length.
    let divisor = gcd(nx.unsigned_abs(), ny.unsigned_abs()).max(1);
    let reduce = |v: i32| v / i32::try_from(divisor).unwrap_or(1);
    SimVec2::from_i32(reduce(nx), reduce(ny)).normalize_or_zero()
}

/// Greatest common divisor by Euclid's algorithm; `gcd(0, 0)` is 0.
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
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

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests: a seeded map that does not build is a failed test"
)]
mod tests {
    use super::super::predicates::segments_touch as segments_touch_q;
    use super::*;
    use crate::footprint::StructureFootprint;
    use crate::grid::{CellFlags, CliffLevel, GameMap, MapCell};
    use crate::pathfinding::PathRequestWithVision;
    use proptest::prelude::*;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use sim_types::EntityId;

    /// A 24 × 24 ground map with seeded unpassable cells and seeded structures, and how many
    /// structures it holds.
    fn seeded_map(seed: u64) -> (GameMap, u32) {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut map = GameMap::new(24, 24, MapCell::default_ground(CliffLevel(1)))
            .expect("invariant: 24 × 24 is a valid map size");
        for _ in 0..rng.gen_range::<u32, _>(10..60) {
            let (x, y) = (
                rng.gen_range::<u32, _>(0..24),
                rng.gen_range::<u32, _>(0..24),
            );
            if let Ok(cell) = map.cell_mut(x, y) {
                cell.flags = CellFlags::empty();
            }
        }
        let mut placed = 0;
        for id in 0..rng.gen_range::<u32, _>(0..6) {
            let size = u8::try_from(rng.gen_range::<u32, _>(1..4)).unwrap_or(1);
            let cell = (
                rng.gen_range::<u32, _>(0..21),
                rng.gen_range::<u32, _>(0..21),
            );
            // A refused placement leaves the map as it was.
            if map
                .place_structure(EntityId(id), cell, StructureFootprint::square(size, size))
                .is_ok()
            {
                placed += 1;
            }
        }
        (map, placed)
    }

    /// The point of `[a, b]` nearest `p` on the Q16 grid, rounded down.
    fn nearest_q(p: QPoint, a: QPoint, b: QPoint) -> QPoint {
        let (dx, dy) = (i128::from(b.x - a.x), i128::from(b.y - a.y));
        let length_sq = dx * dx + dy * dy;
        let along = i128::from(p.x - a.x) * dx + i128::from(p.y - a.y) * dy;
        if length_sq == 0 || along <= 0 {
            return a;
        }
        if along >= length_sq {
            return b;
        }
        let step = |d: i128| i64::try_from((along * d).div_euclid(length_sq)).unwrap_or(0);
        QPoint {
            x: a.x + step(dx),
            y: a.y + step(dy),
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// D-3: the flood lists exactly the obstacle vertices within reach of the segment that it
        /// sees. Against an exhaustive scan of every vertex: each listed vertex is an obstacle
        /// vertex within reach, and each obstacle vertex well within reach that is not listed is
        /// hidden, its straight line to the segment's nearest point touching a blocking edge
        /// that does not end at it.
        #[test]
        fn outside_vertices_match_the_exhaustive_scan(
            seed in 0_u64..1_000_000,
            ends in proptest::array::uniform4(0_u32..24 * 64),
            rho_sixteenths in 5_i64..20,
            partial in proptest::bool::ANY,
        ) {
            let (map, _structures) = seeded_map(seed);
            let mesh = NavMesh::build(&map).expect("invariant: a valid map builds a mesh");
            let point = |x: u32, y: u32| {
                SimVec2::new(
                    SimScalar::from_raw(i64::from(x) << 26),
                    SimScalar::from_raw(i64::from(y) << 26),
                )
            };
            let (a, b) = (point(ends[0], ends[1]), point(ends[2], ends[3]));
            let known: Vec<bool> = (0..24 * 24).map(|i| !partial || i % 3 != 0).collect();
            let request = PathRequestWithVision {
                start: a,
                goal: b,
                radius: SimScalar::from_raw(rho_sixteenths << 28),
                vision: None,
                block_unexplored_ramps: false,
                known_structures: partial.then_some(known.as_slice()),
            };
            let access = Access::new(&mesh, &map, &request);
            let rho = request.radius;
            let mut scratch = SegmentScratch::default();
            prop_assume!(segment_clear(&mesh, &access, &mut scratch, a, b, SimScalar::ZERO));
            let mut listed = Vec::new();
            obstacle_vertices_near(&mesh, &access, &mut scratch, (a, b), rho, &mut listed);
            let (qa, qb) = (QPoint::from_sim(a), QPoint::from_sim(b));
            let reach = radius_q(rho) + 2;
            let open = |t: u32| access.open(t);
            let count = u32::try_from(mesh.vertex_count()).unwrap_or(u32::MAX);
            for &w in &listed {
                prop_assert!(mesh.vertex_blocks(w, open), "{w} is no obstacle vertex");
                let wq = mesh.vertex(w).to_q();
                prop_assert!(point_segment_distance_sq(wq, qa, qb).is_below(reach));
            }
            let triangles = u32::try_from(mesh.triangle_count()).unwrap_or(u32::MAX);
            for w in (0..count).filter(|&w| mesh.vertex_blocks(w, open)) {
                let wq = mesh.vertex(w).to_q();
                if listed.binary_search(&w).is_ok()
                    || !point_segment_distance_sq(wq, qa, qb).is_below(reach - 4)
                {
                    continue;
                }
                let q = nearest_q(wq, qa, qb);
                let hidden = (0..triangles).filter(|&t| access.open(t)).any(|t| {
                    let corners = mesh.triangle_vertices(t);
                    (0..3).any(|k| {
                        let (u, v) = (corners[next(k)], corners[prev(k)]);
                        u != w
                            && v != w
                            && mesh.edge_blocks(t, k, open)
                            && segments_touch_q(q, wq, mesh.vertex(u).to_q(), mesh.vertex(v).to_q())
                    })
                });
                prop_assert!(hidden, "vertex {w} at {:?} within reach is neither listed nor hidden", mesh.vertex(w));
            }
        }
    }
}
