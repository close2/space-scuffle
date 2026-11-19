//! Demyen and Buro's radius funnel over a triangle channel (plan D-6).
//!
//! # Responsibility
//! Turns a channel of portals (mesh edges in travel order) into the shortest route a disc of
//! radius `r` takes through it, as tangents between circles of radius `r + CORNER_SAFETY_MARGIN`
//! centred on the channel vertices it turns around; then replaces each arc by waypoints that keep
//! the route outside its circle.
//!
//! # How it fits
//! `search.rs` calls [`radius_funnel`] on every channel that reaches the goal (TA*'s incumbent)
//! and on the final channel. The waypoints are then checked with the segment predicate.
//!
//! # Invariants
//! - The funnel is the Simple Stupid Funnel (Mononen) with every point replaced by an anchor: the
//!   start, the goal, or a channel vertex with the side on which the route passes it. Directions
//!   from the apex are tangent directions, so the comparisons are those of S11's modified funnel.
//! - All arithmetic is Q32.32 with `SimScalar::sqrt`; no float (charter §3.1).
//! - Each arc becomes waypoints on its circle: its two tangent points and the points of the arc
//!   halved until every chord keeps at least `r + margin - dip` from the vertex it turns around
//!   (plan Amendment 2), where the dip is half the margin ([`ARC_CHORD_DIP`] at the full margin).
//!   The polyline is therefore never longer than the tangent-and-arc route, and every leg keeps
//!   at least `r + margin / 2` from the vertices it rounds.
//! - The refinement's candidates are the channel's portal vertices, the start and goal triangles'
//!   vertices, and every obstacle vertex within `r + margin` of the leg under test, found by a
//!   flood around the leg (plan `2026-11-19-funnel-fallbacks` D-3): a structure corner beyond an
//!   edge of the start or goal triangle bounds the route as much as the channel's own vertices.
//! - The margin is [`CORNER_SAFETY_MARGIN`], except in the search's fitted retry (plan
//!   `2026-11-18-contour-footprints` Amendment 1): a channel narrower than the disc plus two
//!   margins is pulled once more with the margin it has room for, `(w_min - 2r) / 2`.
//! - The search's last retry, the zero-room retry (plan `2026-11-19-funnel-fallbacks` D-2), pulls
//!   at `r + ZERO_ROOM_SLACK` and emits each arc circumscribed ([`Emission::ZeroRoom`]): legs
//!   tangent to the circle, corners outside it, and a **gate line** halfway across every pinch
//!   closer than `r + rho`, followed for at least [`gate_run`] on each side of its touch point.
//!   An axis-aligned gate is exact: its waypoints get the gate's coordinate by assignment.
//! - **Free vertices are points** (plan `2026-11-19-partial-knowledge-funnel` D-1, D-4): a portal
//!   or end-triangle vertex at which no edge blocks the request (`NavMesh::vertex_blocks` false,
//!   such as a corner of a structure the request does not know) bounds nothing. It has no circle:
//!   its offset sign is zero, a tangent touches the vertex itself, the shrunk pull does not move a
//!   portal's free end, the emission gives the vertex once with no arc, and the refinement never
//!   inserts it. TA*'s width test already treats such a corner as unbounded (`width_at`).

use sim_types::{SimScalar, SimVec2};

use super::predicates::{point_segment_distance_sq, QPoint, Q_BITS};
use super::segment::{
    blocking_features_near, obstacle_vertices_near, radius_q, segment_clear, NearFeature,
    SegmentScratch,
};
use super::{Access, NavMesh};
use crate::pathfinding::CORNER_SAFETY_MARGIN;

/// The side of the route on which a channel vertex lies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// The vertex is on the walker's left: the route rounds it counter-clockwise.
    Left,
    /// The vertex is on the walker's right: the route rounds it clockwise.
    Right,
}

/// A point the funnel route passes: an endpoint or a channel vertex rounded on one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The start or the goal, in world units.
    Point(SimVec2),
    /// Mesh vertex index and the side on which the route passes it.
    Vertex(u32, Side),
}

/// A route through a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunnelRoute {
    /// Start, then the vertices the route turns around, then the goal.
    pub anchors: Vec<Anchor>,
    /// Polyline waypoints in world units, start first and goal last.
    pub waypoints: Vec<SimVec2>,
    /// Sum of the polyline's leg lengths, in world units.
    pub length: SimScalar,
}

/// One tangent between two anchors.
struct Tangent {
    /// Unit direction of travel.
    direction: SimVec2,
    /// Where the tangent leaves the first anchor's circle.
    from: SimVec2,
    /// Where it touches the second anchor's circle.
    to: SimVec2,
}

fn perp(v: SimVec2) -> SimVec2 {
    SimVec2::new(-v.y, v.x)
}

fn centre(mesh: &NavMesh, anchor: Anchor) -> SimVec2 {
    match anchor {
        Anchor::Point(p) => p,
        Anchor::Vertex(v, _) => mesh.vertex(v).to_sim(),
    }
}

/// Whether `anchor` is a free vertex of the pull: one in `free`, the pull's sorted list of the
/// channel vertices that block nothing for the request (D-1).
fn is_free(anchor: Anchor, free: &[u32]) -> bool {
    matches!(anchor, Anchor::Vertex(v, _) if free.binary_search(&v).is_ok())
}

/// Offset sign of the route from an anchor's centre, along the left normal of travel: zero for
/// a point and for a free vertex (`free`, D-1), which the route passes as a point.
fn sigma(anchor: Anchor, free: &[u32]) -> i32 {
    match anchor {
        Anchor::Point(_) => 0,
        Anchor::Vertex(..) if is_free(anchor, free) => 0,
        Anchor::Vertex(_, Side::Left) => -1,
        Anchor::Vertex(_, Side::Right) => 1,
    }
}

/// The tangent from `from`'s circle to `to`'s circle, both of radius `rho` (points and the free
/// vertices in `free` have none).
///
/// With `d` the centre difference and `k = (sigma_to - sigma_from) rho`, the unit direction is
/// `(L d + k perp(d)) / |d|^2` with `L = sqrt(|d|^2 - k^2)`; a degenerate pair (one circle within
/// reach of the other) gets `L = 0`, the direction perpendicular to `d`.
fn tangent(mesh: &NavMesh, (from, to): (Anchor, Anchor), rho: SimScalar, free: &[u32]) -> Tangent {
    tangent_between(
        (centre(mesh, from), sigma(from, free)),
        (centre(mesh, to), sigma(to, free)),
        rho,
    )
}

/// [`tangent`] between the centres `a` and `b` with offset signs `sa` and `sb` (see [`sigma`]).
fn tangent_between((a, sa): (SimVec2, i32), (b, sb): (SimVec2, i32), rho: SimScalar) -> Tangent {
    let d = b - a;
    let k = rho * (sb - sa);
    let length_sq = d.length_sq();
    let l = (length_sq - k * k)
        .max(SimScalar::ZERO)
        .sqrt()
        .unwrap_or(SimScalar::ZERO);
    let direction = (d * l + perp(d) * k).normalize_or_zero();
    let normal = perp(direction);
    Tangent {
        direction,
        from: a + normal * (rho * sa),
        to: b + normal * (rho * sb),
    }
}

/// Reusable buffers of the funnel's refinement (plan `2026-11-19-funnel-fallbacks` D-3) and of
/// the zero-room emission (D-2).
#[derive(Debug, Default)]
pub struct FunnelScratch {
    /// The obstacle vertices near the leg under test.
    near: Vec<u32>,
    /// The distinct vertices outside the channel and its end triangles that the refinement has
    /// inserted, in index order; each widens its round bound.
    outside: Vec<u32>,
    /// Whether every candidate, not only an outside one, is skipped when it comes nearest a leg
    /// at the start or the goal (D-2.2: set for the zero-room pulls).
    exempt_all: bool,
    /// The features near the vertex whose arc is being emitted (D-2.3).
    features: Vec<NearFeature>,
    /// The lines of the arc being emitted (D-2.4).
    lines: Vec<ArcLine>,
    /// Indices, in the route's waypoints, of the gate runs' ends (Amendment 1).
    run_ends: Vec<usize>,
    /// Which waypoints the zero-room route keeps after its unneeded run ends are dropped.
    keep: Vec<bool>,
    /// The free vertices of the current pull (plan `2026-11-19-partial-knowledge-funnel` D-1):
    /// the portal and end-triangle vertices at which no edge blocks the request, sorted by index.
    free: Vec<u32>,
}

/// How the funnel turns each rounded vertex's arc into waypoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Emission {
    /// Waypoints on the circle of `rho`, its chords dipping at most `dip` inside it (D-6,
    /// Amendment 2 of the pathfinding plan): every pull but the zero-room retry.
    Inscribed {
        /// How far a chord may dip inside the circle, in world units.
        dip: SimScalar,
    },
    /// The zero-room retry's circumscribed arcs with gate lines (plan
    /// `2026-11-19-funnel-fallbacks` D-2), for a disc of `radius` world units.
    ZeroRoom {
        /// The disc's radius, in world units: pinches are measured against `radius + rho`.
        radius: SimScalar,
    },
}

/// The zero-room retry's rounding slack `ε`, in world units: the retry pulls at `r + ε`
/// (plan `2026-11-19-funnel-fallbacks` D-2.1).
///
/// Derived, not tuned: 16 steps of the segment predicate's Q16 grid, `2⁻¹²`. A leg tangent to
/// the circle of `r + ε` loses at most two grid steps to the predicate (its ends floored to the
/// grid, the radius rounded up) and a few raw units to `SimScalar::sqrt`, so it still keeps `r`
/// from the vertex; `zero_room_legs_keep_the_disc_radius` checks this for every shipped radius.
pub const ZERO_ROOM_SLACK: SimScalar = SimScalar::from_raw(16 << (32 - Q_BITS));

/// A request's obstacles, from which the refinement draws the obstacle vertices near a leg
/// (plan `2026-11-19-funnel-fallbacks` D-3).
pub(crate) struct Obstacles<'a, 'b> {
    /// The triangles the request may enter.
    pub access: &'a Access<'b>,
    /// The flood state of the segment predicate, reused.
    pub segment: &'a mut SegmentScratch,
    /// The refinement's own buffers.
    pub scratch: &'a mut FunnelScratch,
}

/// How the funnel orders the turns before making them exact for circles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullStrategy {
    /// Pull a point through the portals shrunk by `rho` at both ends: robust where circles
    /// overlap the apex. The search uses it for every channel.
    Shrunk,
    /// Pull a point through the portals' own vertices, and keep a turn of more than a half turn
    /// that the vertices bend with the side (a route wrapping the end of a thin wall). Used only
    /// when the shrunk route fails the segment predicate (plan Amendment 2): shrinking a fan's
    /// shared vertex towards each portal's other end makes it orbit its circle, and an apex
    /// beside that circle then sees the funnel's side sweep past it.
    Pinned,
}

/// Runs the radius funnel through `portals` (`(left, right)` vertex indices in travel order).
///
/// `rho` is the unit's radius plus its corner margin ([`crate::CORNER_SAFETY_MARGIN`], the
/// fitted margin of the fitted retry, or [`ZERO_ROOM_SLACK`] in the zero-room retry), and
/// `emission` how arcs become waypoints: inscribed with half the margin as their chords' dip, or
/// circumscribed with gates. `extra` lists the vertices of the start and goal triangles, which
/// bound the route but belong to no portal; `obstacles` gives the refinement every other obstacle
/// vertex near a leg (D-3). The free vertices among the portal and end-triangle vertices are
/// found once per call and pulled as points (plan `2026-11-19-partial-knowledge-funnel` D-1).
#[must_use]
pub(crate) fn radius_funnel(
    mesh: &NavMesh,
    (start, goal): (SimVec2, SimVec2),
    portals: &[(u32, u32)],
    extra: &[u32],
    (rho, emission, strategy): (SimScalar, Emission, PullStrategy),
    obstacles: &mut Obstacles<'_, '_>,
) -> FunnelRoute {
    let mut items: Vec<(Anchor, Anchor)> = Vec::with_capacity(portals.len() + 2);
    items.push((Anchor::Point(start), Anchor::Point(start)));
    items.extend(portals.iter().map(|&(l, r)| {
        (
            Anchor::Vertex(l, Side::Left),
            Anchor::Vertex(r, Side::Right),
        )
    }));
    items.push((Anchor::Point(goal), Anchor::Point(goal)));
    obstacles.scratch.exempt_all = matches!(emission, Emission::ZeroRoom { .. });
    // The list is moved out of the scratch for the pull, so the helpers can read it while the
    // refinement borrows the rest of the scratch, and moved back with its capacity.
    let mut free = std::mem::take(&mut obstacles.scratch.free);
    free_vertices(mesh, (portals, extra), obstacles.access, &mut free);
    let anchors = string_pull(mesh, (&items, extra), (rho, strategy), &free, obstacles);
    let waypoints = match emission {
        Emission::Inscribed { dip } => emit_waypoints(mesh, &anchors, (rho, dip), &free),
        Emission::ZeroRoom { radius } => {
            emit_zero_room(mesh, &anchors, (rho, radius), &free, obstacles)
        }
    };
    obstacles.scratch.free = free;
    let mut length = SimScalar::ZERO;
    for pair in waypoints.windows(2) {
        length += pair[0].distance(pair[1]);
    }
    FunnelRoute {
        anchors,
        waypoints,
        length,
    }
}

/// Fills `free` with the free vertices among the `portals`' ends and the end-triangle vertices
/// `extra` (D-1): those at which no edge blocks the request, sorted by index and distinct.
// PERFORMANCE RATIONALE:
// Why: Every pull asks, for each of its anchors and candidates, whether that vertex is free, and
// the answer is a fan walk round the vertex (`NavMesh::vertex_blocks`).
// What: The distinct channel vertices are walked once per pull into a sorted list that the
// helpers search by bisection, instead of a flag per mesh vertex or a walk per question.
// Trade-off: A lookup costs a binary search over a few dozen entries instead of an index, for no
// mesh-sized array to clear on every pull.
fn free_vertices(
    mesh: &NavMesh,
    (portals, extra): (&[(u32, u32)], &[u32]),
    access: &Access<'_>,
    free: &mut Vec<u32>,
) {
    free.clear();
    free.extend(
        portals
            .iter()
            .flat_map(|&(l, r)| [l, r])
            .chain(extra.iter().copied()),
    );
    free.sort_unstable();
    free.dedup();
    let open = |t: u32| access.open(t);
    free.retain(|&v| !mesh.vertex_blocks(v, open));
}

/// Moves the obstacle ends of a portal inwards by `rho` (`rounded`: whether each end is rounded
/// by a circle; a free end, D-1, imposes nothing).
///
/// Two rounded ends move inwards by `rho` each, and a portal no longer than `2 rho` shrinks to
/// its midpoint. With one rounded end, the point `rho` from it along the portal replaces it, and
/// replaces the free end too when the portal is no longer than `rho`: the disc cannot pass
/// between the two. With none, the portal is unchanged.
fn shrink(a: SimVec2, b: SimVec2, rho: SimScalar, rounded: (bool, bool)) -> (SimVec2, SimVec2) {
    let d = b - a;
    let length = d.length();
    match rounded {
        (true, true) => {
            if length <= rho * 2 {
                let mid = (a + b) / 2;
                return (mid, mid);
            }
            let step = d * (rho / length);
            (a + step, b - step)
        }
        (false, false) => (a, b),
        _ if length == SimScalar::ZERO => (a, b),
        (true, false) => {
            let p = a + d * (rho / length);
            (p, if length <= rho { p } else { b })
        }
        (false, true) => {
            let p = b - d * (rho / length);
            (if length <= rho { p } else { a }, p)
        }
    }
}

fn triarea2(a: SimVec2, b: SimVec2, c: SimVec2) -> SimScalar {
    (b - a).cross_2d(c - a)
}

/// The route's turning anchors through the channel `items` (start and goal included).
///
/// Step 1 pulls a point through the portals shrunk by `rho` (Mononen's Simple Stupid Funnel),
/// which orders the turns robustly even where circles overlap the apex. Step 2 makes the turns
/// exact for circles: a turn that bends against its side is dropped, and a candidate vertex
/// closer than `rho` to a tangent leg is inserted, until neither happens.
fn string_pull(
    mesh: &NavMesh,
    (items, extra): (&[(Anchor, Anchor)], &[u32]),
    (rho, strategy): (SimScalar, PullStrategy),
    free: &[u32],
    obstacles: &mut Obstacles<'_, '_>,
) -> Vec<Anchor> {
    let points: Vec<(SimVec2, SimVec2)> = items
        .iter()
        .map(|&(l, r)| match (l, strategy) {
            (Anchor::Point(p), _) => (p, p),
            (_, PullStrategy::Shrunk) => shrink(
                centre(mesh, l),
                centre(mesh, r),
                rho,
                (!is_free(l, free), !is_free(r, free)),
            ),
            (_, PullStrategy::Pinned) => (centre(mesh, l), centre(mesh, r)),
        })
        .collect();
    let mut path = vec![items[0].0];
    let (mut apex, mut left, mut right) = (points[0].0, points[0].0, points[0].1);
    let (mut left_i, mut right_i) = (0, 0);
    let mut i = 1;
    let mut guard = 0usize;
    let limit = 4 * items.len() * items.len() + 16;
    while i < items.len() && guard < limit {
        guard += 1;
        let (pl, pr) = points[i];
        if triarea2(apex, right, pr) >= SimScalar::ZERO {
            if apex == right || triarea2(apex, left, pr) < SimScalar::ZERO {
                right = pr;
                right_i = i;
            } else {
                apex = left;
                path.push(items[left_i].0);
                (right, right_i) = (apex, left_i);
                i = left_i + 1;
                continue;
            }
        }
        if triarea2(apex, left, pl) <= SimScalar::ZERO {
            if apex == left || triarea2(apex, right, pl) > SimScalar::ZERO {
                left = pl;
                left_i = i;
            } else {
                apex = right;
                path.push(items[right_i].1);
                (left, left_i) = (apex, right_i);
                i = right_i + 1;
                continue;
            }
        }
        i += 1;
    }
    path.push(items[items.len() - 1].0);
    path.dedup();
    refine(mesh, (items, extra), path, (rho, strategy), free, obstacles)
}

/// Whether `turn` (a cross product of travel directions) bends in the sense `side` requires.
fn bends_with(side: Side, turn: SimScalar) -> bool {
    match side {
        Side::Left => turn >= SimScalar::ZERO,
        Side::Right => turn <= SimScalar::ZERO,
    }
}

/// Whether the route turns at anchor `j` in the sense its side requires.
///
/// A route that wraps the end of a thin wall turns there by more than a half turn, and the
/// cross product of its tangent directions then has the other sign. Under
/// [`PullStrategy::Pinned`] such a turn is kept when it reverses the direction of travel and
/// still bends with the side when the anchors are taken as points (Mononen's test).
fn turns_with_side(
    mesh: &NavMesh,
    anchors: &[Anchor],
    j: usize,
    (rho, strategy): (SimScalar, PullStrategy),
    free: &[u32],
) -> bool {
    let Anchor::Vertex(_, side) = anchors[j] else {
        return true;
    };
    let incoming = tangent(mesh, (anchors[j - 1], anchors[j]), rho, free).direction;
    let outgoing = tangent(mesh, (anchors[j], anchors[j + 1]), rho, free).direction;
    if bends_with(side, incoming.cross_2d(outgoing)) {
        return true;
    }
    if strategy == PullStrategy::Shrunk {
        return false;
    }
    let (a, c, b) = (
        centre(mesh, anchors[j - 1]),
        centre(mesh, anchors[j]),
        centre(mesh, anchors[j + 1]),
    );
    incoming.dot(outgoing) < SimScalar::ZERO && bends_with(side, (c - a).cross_2d(b - c))
}

/// Distance from `p` to the segment `[a, b]`.
fn distance_to_segment(p: SimVec2, a: SimVec2, b: SimVec2) -> SimScalar {
    let d = b - a;
    let length_sq = d.length_sq();
    if length_sq == SimScalar::ZERO {
        return p.distance(a);
    }
    let t = ((p - a).dot(d) / length_sq).clamp(SimScalar::ZERO, SimScalar::ONE);
    p.distance(a + d * t)
}

/// The candidate vertex deepest inside `rho` of the tangent leg from `anchors[j]` to
/// `anchors[j + 1]`, if any, and whether it is an obstacle vertex outside the channel and its end
/// triangles (D-3). The candidates are, in this order, the portal vertices, the start and goal
/// triangles' vertices, and the obstacle vertices near the leg; of equally deep ones the first
/// wins, so a vertex of the channel keeps its portal side. A free vertex (`free`) is no
/// candidate: it bounds nothing, and the segment predicate judges the leg (plan
/// `2026-11-19-partial-knowledge-funnel` D-4).
fn deepest_violation(
    mesh: &NavMesh,
    (items, extra): (&[(Anchor, Anchor)], &[u32]),
    anchors: &[Anchor],
    (j, rho): (usize, SimScalar),
    free: &[u32],
    obstacles: &mut Obstacles<'_, '_>,
) -> Option<(Anchor, bool)> {
    let leg = tangent(mesh, (anchors[j], anchors[j + 1]), rho, free);
    obstacle_vertices_near(
        mesh,
        obstacles.access,
        obstacles.segment,
        (leg.from, leg.to),
        rho,
        &mut obstacles.scratch.near,
    );
    let rounded = |v: u32| {
        anchors
            .iter()
            .any(|a| matches!(*a, Anchor::Vertex(w, _) if w == v))
    };
    // The start and goal triangles' own vertices and the obstacle vertices near the leg are in
    // no portal; their side is the side of the leg they lie on.
    let side_of = |v: u32| {
        if leg.direction.cross_2d(mesh.vertex(v).to_sim() - leg.from) > SimScalar::ZERO {
            Side::Left
        } else {
            Side::Right
        }
    };
    let channel = 2 * (items.len() - 2) + extra.len();
    let candidates = items[1..items.len() - 1]
        .iter()
        .flat_map(|&(l, r)| [l, r])
        .chain(extra.iter().map(|&v| Anchor::Vertex(v, side_of(v))))
        .chain(
            obstacles
                .scratch
                .near
                .iter()
                .map(|&v| Anchor::Vertex(v, side_of(v))),
        );
    let mut worst: Option<(SimScalar, Anchor, bool)> = None;
    for (index, candidate) in candidates.enumerate() {
        let Anchor::Vertex(v, _) = candidate else {
            continue;
        };
        if rounded(v) || free.binary_search(&v).is_ok() {
            continue;
        }
        let p = centre(mesh, candidate);
        let distance = distance_to_segment(p, leg.from, leg.to);
        let outside = index >= channel;
        if (outside || obstacles.scratch.exempt_all)
            && approached_at_an_end(p, distance, (anchors[j], anchors[j + 1]))
        {
            continue;
        }
        // A hair inside the circle is rounding, not a collision.
        if distance + SimScalar::from_raw(1 << 12) < rho
            && worst.is_none_or(|(d, _, _)| distance < d)
        {
            worst = Some((distance, candidate, outside));
        }
    }
    worst.map(|(_, anchor, outside)| (anchor, outside))
}

/// Whether a vertex at `p`, `distance` from a leg, comes nearest the leg at one of its ends that
/// is the start or the goal (`ends`, the leg's two anchors). The disc stands at that point
/// already, fitting or wedged, so the approach is not the leg's to fix, and rounding the vertex
/// would only circle it (plan `2026-11-19-funnel-fallbacks` D-2.2, for every candidate of the
/// zero-room pulls; applied to the obstacle vertices of D-3 in every pull, evidence WP2).
fn approached_at_an_end(p: SimVec2, distance: SimScalar, ends: (Anchor, Anchor)) -> bool {
    [ends.0, ends.1]
        .into_iter()
        .any(|end| matches!(end, Anchor::Point(q) if p.distance(q) <= distance))
}

/// Step 2 of [`string_pull`]: drops turns against their side and inserts violated vertices.
///
/// At most `4 (items + outside) + 8` rounds, where `outside` counts the distinct vertices outside
/// the channel and its end triangles inserted so far (D-3): each candidate may be inserted and
/// dropped a bounded number of times, and the outside candidates are finite, so the loop ends.
fn refine(
    mesh: &NavMesh,
    (items, extra): (&[(Anchor, Anchor)], &[u32]),
    mut anchors: Vec<Anchor>,
    (rho, strategy): (SimScalar, PullStrategy),
    free: &[u32],
    obstacles: &mut Obstacles<'_, '_>,
) -> Vec<Anchor> {
    obstacles.scratch.outside.clear();
    let mut round = 0;
    while round < 4 * (items.len() + obstacles.scratch.outside.len()) + 8 {
        round += 1;
        if let Some(j) = (1..anchors.len().saturating_sub(1))
            .find(|&j| !turns_with_side(mesh, &anchors, j, (rho, strategy), free))
        {
            anchors.remove(j);
            continue;
        }
        let mut violation = None;
        for j in 0..anchors.len() - 1 {
            if let Some(found) =
                deepest_violation(mesh, (items, extra), &anchors, (j, rho), free, obstacles)
            {
                violation = Some((j, found));
                break;
            }
        }
        let Some((j, (vertex, outside))) = violation else {
            break;
        };
        if let (true, Anchor::Vertex(v, _)) = (outside, vertex) {
            let known = &mut obstacles.scratch.outside;
            if let Err(at) = known.binary_search(&v) {
                known.insert(at, v);
            }
        }
        anchors.insert(j + 1, vertex);
    }
    anchors
}

/// How far a leg between two arc waypoints may dip inside the circle of `r + margin`, in world
/// units: half of [`CORNER_SAFETY_MARGIN`], so every leg keeps at least `r + margin / 2` from the
/// vertex it turns around.
pub const ARC_CHORD_DIP: SimScalar = SimScalar::from_raw(CORNER_SAFETY_MARGIN.raw() / 2);

/// Bisection depth bound of one arc: at most `2^ARC_MAX_DEPTH` legs per rounded vertex.
const ARC_MAX_DEPTH: u32 = 6;

/// Smallest `|a + b|^2` treated as two opposite directions when bisecting an arc.
const OPPOSITE_SUM_SQ: SimScalar = SimScalar::from_raw(1 << 20);

/// The unit direction halfway along the arc from `a` to `b` in the sense `arc_sign` (+1
/// counter-clockwise, -1 clockwise); both are unit vectors from the circle's centre.
fn arc_middle(a: SimVec2, b: SimVec2, arc_sign: i32) -> SimVec2 {
    let sum = a + b;
    if sum.length_sq() <= OPPOSITE_SUM_SQ {
        return perp(a) * arc_sign;
    }
    let bisector = sum.normalize_or_zero();
    if a.cross_2d(b) * arc_sign >= SimScalar::ZERO {
        bisector
    } else {
        -bisector
    }
}

/// Pushes the waypoints strictly inside the arc from `a` to `b` (unit directions from `c`, in
/// the sense `arc_sign`), on the circle of radius `rho`: the arc is halved until every chord
/// keeps at least `floor` from `c`, at most [`ARC_MAX_DEPTH`] times.
fn arc_points(
    (c, rho, floor): (SimVec2, SimScalar, SimScalar),
    (a, b): (SimVec2, SimVec2),
    arc_sign: i32,
    depth: u32,
    out: &mut Vec<SimVec2>,
) {
    // The chord from `a` to `b` keeps `rho |a + b| / 2` from the centre when the arc is at most
    // a half turn; compared on squares.
    let short = a.cross_2d(b) * arc_sign >= SimScalar::ZERO;
    let sum = a + b;
    if depth >= ARC_MAX_DEPTH || (short && sum.length_sq() * rho * rho >= floor * floor * 4) {
        return;
    }
    let m = arc_middle(a, b, arc_sign);
    arc_points((c, rho, floor), (a, m), arc_sign, depth + 1, out);
    out.push(c + m * rho);
    arc_points((c, rho, floor), (m, b), arc_sign, depth + 1, out);
}

/// Replaces each rounded vertex's arc by waypoints on its circle (D-6, Amendment 2): the two
/// tangent points and, between them, the points of the arc halved until every chord keeps at
/// least `rho - dip` from the vertex. The polyline is then never longer than the
/// tangent-and-arc route and dips at most `dip` inside its circle. A free vertex (`free`, D-1) has
/// no circle: it is emitted once, as itself.
fn emit_waypoints(
    mesh: &NavMesh,
    anchors: &[Anchor],
    (rho, dip): (SimScalar, SimScalar),
    free: &[u32],
) -> Vec<SimVec2> {
    let tangents: Vec<Tangent> = anchors
        .windows(2)
        .map(|pair| tangent(mesh, (pair[0], pair[1]), rho, free))
        .collect();
    let floor = rho - dip;
    let mut points = vec![centre(mesh, anchors[0])];
    for j in 1..anchors.len().saturating_sub(1) {
        let c = centre(mesh, anchors[j]);
        let Anchor::Vertex(_, side) = anchors[j] else {
            points.push(c);
            continue;
        };
        if is_free(anchors[j], free) {
            points.push(c);
            continue;
        }
        let (enter, leave) = (tangents[j - 1].to, tangents[j].from);
        points.push(enter);
        let n_in = (enter - c).normalize_or_zero();
        let n_out = (leave - c).normalize_or_zero();
        if n_in != SimVec2::ZERO && n_out != SimVec2::ZERO {
            let arc_sign = if side == Side::Left { 1 } else { -1 };
            arc_points((c, rho, floor), (n_in, n_out), arc_sign, 0, &mut points);
        }
        points.push(leave);
    }
    points.push(centre(mesh, anchors[anchors.len() - 1]));
    points.dedup();
    points
}

/// The finest step of a zero-room arc, as a fraction of a turn: `1 / 2^ZERO_ROOM_ARC_SPLITS`
/// (plan `2026-11-19-funnel-fallbacks` Amendment 2).
const ZERO_ROOM_ARC_SPLITS: u32 = 5;

/// `cos(2π / 32)` floored to the raw grid: the zero-room emission bisects any step between two
/// consecutive arc lines wider than a thirty-second of a turn (D-2.4 as Amendment 2 sets it).
// PERFORMANCE RATIONALE:
// Why: A circumscribed polygon whose sides each span an angle `θ` is `tan(θ/2) / (θ/2)` times
// the arc it replaces, so a route that rounds many corners in a row is longer than the exact
// route by that factor: 1.0548 for an eighth of a turn, 1.0130 for a sixteenth, 1.0032 for a
// thirty-second. The oracle bound the routes are held to is 1.01.
// What: The step is the largest power-of-two fraction of a turn whose factor is at most 1.01,
// a thirty-second (`zero_room_arc_step_is_the_largest_within_the_oracle_bound` computes it), and
// a lookup constant instead of a cosine computed at run time.
// Trade-off: Up to 32 lines per full turn of arc instead of 8, in the zero-room retry only (the
// other pulls emit inscribed arcs and never reach this code), which runs in no measured battle.
const ZERO_ROOM_ARC_STEP_COS: SimScalar = SimScalar::from_raw(4_212_440_703);

/// Bound on the bisections and on the redundancy passes of one circumscribed arc: a full turn
/// at the finest step needs 32 lines and 31 bisections, with gates besides.
const ARC_MAX_LINES: usize = 4 << ZERO_ROOM_ARC_SPLITS;

/// The tolerance within which a gate's normal counts as the arc's end normal: a gate
/// exactly at an end of the arc is kept, not lost to the rounding of `diamond`.
const ARC_ANGLE_TOLERANCE: SimScalar = SimScalar::from_raw(1 << 12);

/// The run of a gate line (plan `2026-11-19-funnel-fallbacks` D-2.5): how far, in world units,
/// the zero-room route follows a gate on each side of its touch point before a neighbouring
/// leg may leave it, for a disc of `radius`.
///
/// A neighbouring leg reaches the gate line at most `ε` ([`ZERO_ROOM_SLACK`]) off it. Leaving the
/// gate `s` from the touch point, it stays `sqrt(r² + s²) - ε ≈ r + s² / 2r - ε` from the feature
/// across, which is at least `r` when `s ≥ sqrt(2 r ε)`. The run is that bound rounded up to a
/// power of two, `2^⌈log₂ √(2rε)⌉`, so the run's points are exact: 2⁻⁵ for `r = 1`, 2⁻⁶ for
/// `r = 0.5`.
#[must_use]
pub fn gate_run(radius: SimScalar) -> SimScalar {
    let need = radius * 2 * ZERO_ROOM_SLACK;
    let mut run = SimScalar::from_raw(1);
    // At most 63 doublings reach the largest raw value; the square of a run of 2^-16 world
    // units or more is exact.
    for _ in 0..63 {
        if run * run >= need {
            break;
        }
        run = run * 2;
    }
    run
}

/// The fixed coordinate of an axis-aligned gate line, which its waypoints take exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateAxis {
    /// The line `x = c`.
    X(SimScalar),
    /// The line `y = c`.
    Y(SimScalar),
    /// A gate across a diagonal pinch: its points are not snapped (plan §9).
    Diagonal,
}

/// One line of a circumscribed arc: the boundary of the half-plane `(x - c) · normal <= offset`
/// round the arc's vertex `c` (D-2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ArcLine {
    /// Diamond angle of the normal from the arc's first normal, in the arc's sense, in [0, 4).
    angle: SimScalar,
    /// Unit normal, pointing away from `c`.
    normal: SimVec2,
    /// Distance of the line from `c`, in world units.
    offset: SimScalar,
    /// Order among lines of equal angle and offset (D-4): tangents and bisectors 0, gates their
    /// feature's index plus one.
    order: u32,
    /// For a gate line, its axis; `None` for a tangent or a bisector.
    gate: Option<GateAxis>,
    /// For a gate line, the feature across the pinch, in world units (the origin otherwise).
    feature: SimVec2,
}

/// A monotone stand-in for the angle from `a` to `b` in the sense `arc_sign`, in [0, 4): the
/// diamond angle of `b` in the frame of `a`, linear in each quarter turn. Both are unit vectors.
fn diamond(a: SimVec2, b: SimVec2, arc_sign: i32) -> SimScalar {
    let x = a.dot(b);
    let y = a.cross_2d(b) * arc_sign;
    let one = SimScalar::ONE;
    if y >= SimScalar::ZERO {
        if x >= SimScalar::ZERO {
            if x + y == SimScalar::ZERO {
                SimScalar::ZERO
            } else {
                y / (x + y)
            }
        } else {
            one - x / (y - x)
        }
    } else if x < SimScalar::ZERO {
        one * 2 - y / (-x - y)
    } else {
        one * 3 + x / (x - y)
    }
}

/// The point, relative to the arc's vertex, where the lines `x · a = ha` and `x · b = hb` meet;
/// `None` for parallel lines, or when the point lies beyond the fixed-point range.
fn line_meet(a: SimVec2, ha: SimScalar, b: SimVec2, hb: SimScalar) -> Option<SimVec2> {
    let cross = a.cross_2d(b);
    if cross == SimScalar::ZERO {
        return None;
    }
    let x = perp(a) * hb - perp(b) * ha;
    Some(SimVec2::new(
        x.x.checked_div(cross)?,
        x.y.checked_div(cross)?,
    ))
}

/// The gate line of the vertex `c` against the feature point `f` (D-2.3): normal towards `f`,
/// offset half the distance. An axis-aligned pair gets an exact normal, offset and axis, so its
/// waypoints are exact; `None` when `f` is `c`.
fn gate_line(c: SimVec2, f: SimVec2, order: u32) -> Option<ArcLine> {
    let w = f - c;
    let (normal, offset, axis) = if w.y == SimScalar::ZERO && w.x != SimScalar::ZERO {
        let sign = w.x.signum();
        (
            SimVec2::new(sign, SimScalar::ZERO),
            w.x.abs() / 2,
            GateAxis::X(c.x + w.x / 2),
        )
    } else if w.x == SimScalar::ZERO && w.y != SimScalar::ZERO {
        let sign = w.y.signum();
        (
            SimVec2::new(SimScalar::ZERO, sign),
            w.y.abs() / 2,
            GateAxis::Y(c.y + w.y / 2),
        )
    } else {
        let length = w.length();
        if length == SimScalar::ZERO {
            return None;
        }
        (w / length, length / 2, GateAxis::Diagonal)
    };
    Some(ArcLine {
        angle: SimScalar::ZERO,
        normal,
        offset,
        order,
        gate: Some(axis),
        feature: f,
    })
}

/// Fills `lines` with the circumscribed arc of the vertex `c` from the unit normal `n_in` to
/// `n_out` in the sense `arc_sign` (D-2.4): its two tangent lines at `rho`, the gate line of
/// every feature whose normal lies in the arc, and bisecting lines at `rho` wherever two
/// consecutive normals are more than a thirty-second of a turn apart (Amendment 2), in angular
/// order. Of lines with
/// one normal the nearest is kept.
fn arc_lines(
    (c, rho): (SimVec2, SimScalar),
    (n_in, n_out, arc_sign): (SimVec2, SimVec2, i32),
    features: &[NearFeature],
    lines: &mut Vec<ArcLine>,
) {
    lines.clear();
    let total = diamond(n_in, n_out, arc_sign);
    let tangent_line = |angle, normal| ArcLine {
        angle,
        normal,
        offset: rho,
        order: 0,
        gate: None,
        feature: SimVec2::ZERO,
    };
    lines.push(tangent_line(SimScalar::ZERO, n_in));
    if n_out != n_in {
        lines.push(tangent_line(total, n_out));
    }
    for (order, feature) in (1_u32..).zip(features) {
        let Some(mut gate) = gate_line(c, feature.point, order) else {
            continue;
        };
        let mut angle = diamond(n_in, gate.normal, arc_sign);
        if angle >= SimScalar::from_i32(4) - ARC_ANGLE_TOLERANCE {
            angle = SimScalar::ZERO;
        }
        if angle <= total + ARC_ANGLE_TOLERANCE {
            gate.angle = angle.min(total);
            lines.push(gate);
        }
    }
    lines.sort_unstable_by_key(|l| (l.angle, l.offset, l.order));
    lines.dedup_by(|later, kept| {
        if later.normal != kept.normal {
            return false;
        }
        if later.offset < kept.offset {
            *kept = ArcLine {
                angle: kept.angle,
                ..*later
            };
        }
        true
    });
    let mut k = 0;
    for _ in 0..ARC_MAX_LINES {
        if k + 1 >= lines.len() {
            break;
        }
        let (a, b) = (lines[k].normal, lines[k + 1].normal);
        let short = a.cross_2d(b) * arc_sign >= SimScalar::ZERO;
        if short && a.dot(b) >= ZERO_ROOM_ARC_STEP_COS {
            k += 1;
            continue;
        }
        let m = arc_middle(a, b, arc_sign);
        lines.insert(k + 1, tangent_line(diamond(n_in, m, arc_sign), m));
    }
}

/// Drops every inner line of a circumscribed arc that its neighbours' meeting point already
/// satisfies (D-2.4), until none is left to drop.
fn drop_redundant_lines(lines: &mut Vec<ArcLine>, arc_sign: i32) {
    for _ in 0..ARC_MAX_LINES {
        let mut changed = false;
        let mut i = 1;
        while i + 1 < lines.len() {
            let (a, b) = (lines[i - 1], lines[i + 1]);
            let short = a.normal.cross_2d(b.normal) * arc_sign > SimScalar::ZERO;
            let covered = line_meet(a.normal, a.offset, b.normal, b.offset)
                .is_some_and(|x| x.dot(lines[i].normal) <= lines[i].offset);
            if short && covered {
                lines.remove(i);
                changed = true;
            } else {
                i += 1;
            }
        }
        if !changed {
            break;
        }
    }
}

/// `p` moved onto `line` when it is an axis-aligned gate: its fixed coordinate is assigned.
fn snap(mut p: SimVec2, line: &ArcLine) -> SimVec2 {
    match line.gate {
        Some(GateAxis::X(x)) => p.x = x,
        Some(GateAxis::Y(y)) => p.y = y,
        Some(GateAxis::Diagonal) | None => {}
    }
    p
}

/// Whether a leg that leaves (or reaches) a gate at `x` in the unit direction `direction`
/// passes within `radius` of the gate's `feature`, under the segment predicate's own exact Q16
/// test (plan `2026-11-19-funnel-fallbacks` Amendment 1).
///
/// The leg is taken `4 radius` long: `x` lies at most a run from the gate's touch point, so it is
/// less than `radius + rho` plus a run from the feature, and no point of the leg farther than
/// that plus `radius` from `x` can come within `radius` of it. A longer leg than the real one can
/// only ask for a run where none was needed, never drop a needed one.
fn leg_needs_run(x: SimVec2, direction: SimVec2, feature: SimVec2, radius: SimScalar) -> bool {
    let far = x + direction * (radius * 4);
    let q = QPoint::from_sim;
    point_segment_distance_sq(q(feature), q(x), q(far)).is_below(radius_q(radius))
}

/// Pushes the waypoints of one circumscribed arc round `c` (D-2.4 and D-2.5): the meeting
/// points of consecutive lines, and on each gate its run's two ends unless a meeting point
/// lies farther along it. A meeting point within a gate's run of its touch point is replaced by
/// the run's end only when the neighbouring leg through it would pass within `radius` of the
/// gate's feature (Amendment 1): elsewhere the meeting point is kept and the route walks no run
/// it does not need. `ends` describes the anchor before and the one after: whether it is a point
/// within the circle, whose side of a gate keeps no run end (the route reaches the point
/// directly), and the point itself when it is the start or the goal. A start or goal lying on
/// the gate next to its tangent line is where that line meets the gate: the meeting point is the
/// point itself, not its rounded image, which could lie a hair beyond it (D-2.5).
fn emit_arc(
    c: SimVec2,
    lines: &[ArcLine],
    (run, radius, arc_sign): (SimScalar, SimScalar, i32),
    ends: [(bool, Option<SimVec2>); 2],
    (out, run_ends): (&mut Vec<SimVec2>, &mut Vec<usize>),
) {
    let n = lines.len();
    if n == 1 && lines[0].gate.is_none() {
        out.push(c + lines[0].normal * lines[0].offset);
        return;
    }
    // A start or goal on an axis gate beside its own tangent line (the first or the last).
    let on_gate = |point: Option<SimVec2>, gate: &ArcLine| {
        point.filter(|&p| {
            matches!(gate.gate, Some(GateAxis::X(_) | GateAxis::Y(_))) && snap(p, gate) == p
        })
    };
    let meet = |i: usize| {
        let (a, b) = (&lines[i], &lines[i + 1]);
        let end = if i == 0 {
            on_gate(ends[0].1, b)
        } else if i + 2 == n {
            on_gate(ends[1].1, a)
        } else {
            None
        };
        end.or_else(|| {
            line_meet(a.normal, a.offset, b.normal, b.offset).map(|x| snap(snap(c + x, a), b))
        })
    };
    // Signed position of `p` along line `i` from its touch point, in the arc's sense.
    let along = |i: usize, p: SimVec2| {
        let (u, h) = (lines[i].normal, lines[i].offset);
        (p - c - u * h).dot(perp(u) * arc_sign)
    };
    let run_end = |i: usize, sign: i32| {
        let (u, h) = (lines[i].normal, lines[i].offset);
        snap(c + u * h + perp(u) * (run * (arc_sign * sign)), &lines[i])
    };
    let is_gate = |i: usize| lines[i].gate.is_some();
    // The direction of travel along line `i`, in the arc's sense.
    let forward = |i: usize| perp(lines[i].normal) * arc_sign;
    // Whether gate `i` keeps its meeting point with the line before it: the point lies before
    // the run, or the leg arriving along that line keeps `radius` from the gate's feature.
    let entry_kept = |i: usize| {
        i > 0
            && meet(i - 1).is_some_and(|x| {
                along(i, x) < -run || !leg_needs_run(x, -forward(i - 1), lines[i].feature, radius)
            })
    };
    // Likewise its meeting point with the line after it, and the leg leaving along that line.
    let exit_kept = |i: usize| {
        i + 1 < n
            && meet(i).is_some_and(|x| {
                along(i, x) > run || !leg_needs_run(x, forward(i + 1), lines[i].feature, radius)
            })
    };
    for i in 0..n {
        if i > 0 {
            if let Some(x) = meet(i - 1) {
                let absorbed =
                    (is_gate(i - 1) && !exit_kept(i - 1)) || (is_gate(i) && !entry_kept(i));
                if !absorbed {
                    out.push(x);
                }
            }
        }
        if is_gate(i) {
            let entry_reached = i == 0 && ends[0].0;
            if !entry_kept(i) && !entry_reached {
                run_ends.push(out.len());
                out.push(run_end(i, -1));
            }
            let exit_kept = exit_kept(i);
            let exit_reached = i + 1 == n && ends[1].0;
            if !exit_kept && !exit_reached {
                run_ends.push(out.len());
                out.push(run_end(i, 1));
            }
        }
    }
}

/// The zero-room retry's waypoints (plan `2026-11-19-funnel-fallbacks` D-2): each rounded
/// vertex's arc is emitted circumscribed at `rho`, with a gate line across every pinch closer
/// than `radius + rho` whose normal lies in the arc.
///
/// A point anchor within `rho` of the vertex gives its end of the arc the direction to the
/// point, since the tangent from a point inside the circle is degenerate. A free vertex (`free`,
/// D-1) has no circle and no gate: it is emitted once, as itself.
fn emit_zero_room(
    mesh: &NavMesh,
    anchors: &[Anchor],
    (rho, radius): (SimScalar, SimScalar),
    free: &[u32],
    obstacles: &mut Obstacles<'_, '_>,
) -> Vec<SimVec2> {
    let tangents: Vec<Tangent> = anchors
        .windows(2)
        .map(|pair| tangent(mesh, (pair[0], pair[1]), rho, free))
        .collect();
    let run = gate_run(radius);
    let mut points = vec![centre(mesh, anchors[0])];
    for j in 1..anchors.len().saturating_sub(1) {
        let c = centre(mesh, anchors[j]);
        let Anchor::Vertex(v, side) = anchors[j] else {
            points.push(c);
            continue;
        };
        if is_free(anchors[j], free) {
            points.push(c);
            continue;
        }
        let inside = |a: Anchor| match a {
            Anchor::Point(p) if p.distance(c) < rho => Some(p),
            _ => None,
        };
        let (before, after) = (inside(anchors[j - 1]), inside(anchors[j + 1]));
        let point_of = |a: Anchor| match a {
            Anchor::Point(p) => Some(p),
            Anchor::Vertex(..) => None,
        };
        let n_in = (before.unwrap_or(tangents[j - 1].to) - c).normalize_or_zero();
        let n_out = (after.unwrap_or(tangents[j].from) - c).normalize_or_zero();
        if n_in == SimVec2::ZERO || n_out == SimVec2::ZERO {
            points.push(tangents[j - 1].to);
            continue;
        }
        let arc_sign = if side == Side::Left { 1 } else { -1 };
        let scratch = &mut *obstacles.scratch;
        blocking_features_near(
            mesh,
            obstacles.access,
            obstacles.segment,
            (v, radius + rho),
            &mut scratch.features,
        );
        arc_lines(
            (c, rho),
            (n_in, n_out, arc_sign),
            &scratch.features,
            &mut scratch.lines,
        );
        drop_redundant_lines(&mut scratch.lines, arc_sign);
        emit_arc(
            c,
            &scratch.lines,
            (run, radius, arc_sign),
            [
                (before.is_some(), point_of(anchors[j - 1])),
                (after.is_some(), point_of(anchors[j + 1])),
            ],
            (&mut points, &mut scratch.run_ends),
        );
    }
    points.push(centre(mesh, anchors[anchors.len() - 1]));
    drop_unneeded_runs(mesh, obstacles, radius, &mut points);
    drop_rounding_beside_the_ends(&mut points);
    points.dedup();
    points
}

/// Drops every inner waypoint closer than [`ZERO_ROOM_SLACK`] to the start or the goal. Such a
/// point is a meeting point of a line through that end, computed with rounding: it can lie a hair
/// beyond the end, towards the obstacle the disc touches there, where the end itself fits. The
/// route goes to the end directly instead.
fn drop_rounding_beside_the_ends(points: &mut Vec<SimVec2>) {
    let (Some(&start), Some(&goal)) = (points.first(), points.last()) else {
        return;
    };
    let last = points.len() - 1;
    let slack_sq = ZERO_ROOM_SLACK * ZERO_ROOM_SLACK;
    let mut index = 0;
    points.retain(|&p| {
        index += 1;
        let inner = index > 1 && index - 1 < last;
        !inner || (p.distance_sq(start) >= slack_sq && p.distance_sq(goal) >= slack_sq)
    });
}

/// Drops, in route order, each gate run's end whose neighbouring waypoints are joined by a leg
/// the segment predicate passes for the disc of `radius` (plan `2026-11-19-funnel-fallbacks`
/// Amendment 1): a gate is followed for its run only where the leg leaving it would otherwise
/// pass within `r` of an obstacle. Two arcs that share a gate (a pinch between two rounded
/// vertices) each add a run end on their side of the touch point; the route then walks the
/// union of the two runs once, not forth and back.
fn drop_unneeded_runs(
    mesh: &NavMesh,
    obstacles: &mut Obstacles<'_, '_>,
    radius: SimScalar,
    points: &mut Vec<SimVec2>,
) {
    let scratch = &mut *obstacles.scratch;
    scratch.keep.clear();
    scratch.keep.resize(points.len(), true);
    for &k in &scratch.run_ends {
        let Some(before) = (0..k).rev().find(|&i| scratch.keep[i]) else {
            continue;
        };
        let Some(&after) = points.get(k + 1) else {
            continue;
        };
        if segment_clear(
            mesh,
            obstacles.access,
            obstacles.segment,
            points[before],
            after,
            radius,
        ) {
            scratch.keep[k] = false;
        }
    }
    let mut index = 0;
    points.retain(|_| {
        index += 1;
        scratch.keep[index - 1]
    });
    scratch.run_ends.clear();
}

/// Waypoints through the midpoints of the portals: the fallback when a funnel leg fails the
/// segment predicate (D-6).
#[must_use]
pub fn portal_midpoints(
    mesh: &NavMesh,
    start: SimVec2,
    goal: SimVec2,
    portals: &[(u32, u32)],
) -> Vec<SimVec2> {
    let mut points = Vec::with_capacity(portals.len() + 2);
    points.push(start);
    for &(l, r) in portals {
        points.push((mesh.vertex(l).to_sim() + mesh.vertex(r).to_sim()) / 2);
    }
    points.push(goal);
    points.dedup();
    points
}

/// The unit travel direction of the tangent between two anchors, exposed for tests. Every vertex
/// anchor is taken as rounded by its circle of `rho`, as for a request that knows every structure.
#[must_use]
pub fn tangent_direction(mesh: &NavMesh, from: Anchor, to: Anchor, rho: SimScalar) -> SimVec2 {
    tangent(mesh, (from, to), rho, &[]).direction
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests: a seeded map that does not build is a failed test"
)]
mod tests {
    use super::*;
    use crate::navmesh::segment::NearFeature;

    fn unit(degrees: f64) -> SimVec2 {
        let radians = degrees.to_radians();
        SimVec2::new(
            SimScalar::from_num(radians.cos()),
            SimScalar::from_num(radians.sin()),
        )
        .normalize_or_zero()
    }

    /// Every emitted arc point lies on the circle, and every chord between consecutive points
    /// (tangent points included) keeps at least `rho - ARC_CHORD_DIP` from the centre, for
    /// small turns, a right angle, a half turn and a wrap beyond it, in both senses.
    #[test]
    fn arc_points_stay_on_the_circle_and_chords_keep_the_floor() {
        let c = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(7));
        let rho = SimScalar::from_num(0.55);
        let floor = rho - ARC_CHORD_DIP;
        let tolerance = SimScalar::from_raw(1 << 16);
        for (from, sweep) in [(0.0, 10.0), (30.0, 90.0), (-90.0, 180.0), (45.0, 250.0)] {
            for arc_sign in [1, -1] {
                let a = unit(from);
                let b = unit(from + sweep * f64::from(arc_sign));
                let mut points = vec![c + a * rho];
                arc_points((c, rho, floor), (a, b), arc_sign, 0, &mut points);
                points.push(c + b * rho);
                for &p in &points {
                    assert!(
                        (p.distance(c) - rho).abs() <= tolerance,
                        "{p:?} off the circle"
                    );
                }
                for pair in points.windows(2) {
                    let middle = (pair[0] + pair[1]) / 2;
                    assert!(
                        middle.distance(c) + tolerance >= floor,
                        "sweep {sweep} sense {arc_sign}: chord {pair:?} dips below the floor"
                    );
                    // Consecutive points advance in the arc's sense.
                    let turn = (pair[0] - c).cross_2d(pair[1] - c) * arc_sign;
                    assert!(turn >= SimScalar::ZERO, "sweep {sweep}: {pair:?} goes back");
                }
                let pieces = points.len() - 1;
                assert!(pieces <= 1 << ARC_MAX_DEPTH, "{pieces} pieces");
            }
        }
    }

    /// The predicate's verdict on the disc of `radius` swept along `[a, b]` against the point
    /// `p`: whether it keeps `radius` (rounded up to the Q16 grid, the ends floored to it).
    fn keeps(p: SimVec2, (a, b): (SimVec2, SimVec2), radius: SimScalar) -> bool {
        use super::super::predicates::{point_segment_distance_sq, QPoint};
        use super::super::segment::radius_q;
        let q = |v: SimVec2| QPoint::from_sim(v);
        !point_segment_distance_sq(q(p), q(a), q(b)).is_below(radius_q(radius))
    }

    /// The shipped terrain radii, in sixteenths of a world unit.
    const SHIPPED_RADII: [i64; 9] = [5, 6, 8, 9, 10, 12, 14, 15, 16];

    /// D-2.1: legs tangent to the circles of `r + ZERO_ROOM_SLACK` keep `r` from the vertices
    /// they are tangent to under the segment predicate, for every shipped radius and sixteen
    /// directions (the eight axis and diagonal classes, and each turned by 20 degrees): from a
    /// point to a vertex on either side, and between two vertices on the same and on opposite
    /// sides.
    #[test]
    fn zero_room_legs_keep_the_disc_radius() {
        let c = SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(20));
        for sixteenths in SHIPPED_RADII {
            let radius = SimScalar::from_raw(sixteenths << 28);
            let rho = radius + ZERO_ROOM_SLACK;
            for class in 0..8 {
                for turn in [0.0, 20.0] {
                    let direction = unit(f64::from(class) * 45.0 + turn);
                    let far = c + direction * SimScalar::from_num(7);
                    for side in [-1, 1] {
                        let leg = tangent_between((far, 0), (c, side), rho);
                        assert!(
                            keeps(c, (far, leg.to), radius),
                            "r {radius}, class {class} + {turn}, side {side}: {:?}",
                            (far, leg.to)
                        );
                        let other = c + direction * SimScalar::from_num(5);
                        for other_side in [-1, 1] {
                            let leg = tangent_between((c, side), (other, other_side), rho);
                            for vertex in [c, other] {
                                assert!(
                                    keeps(vertex, (leg.from, leg.to), radius),
                                    "r {radius}, class {class} + {turn}, sides {side} \
                                     {other_side}: {:?}",
                                    (leg.from, leg.to)
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// D-2.5: a neighbouring leg that leaves a gate line `ε` off it, on the feature's side,
    /// keeps `r` from the feature across when it leaves `GATE_RUN(r)` from the touch point, and
    /// does not when it leaves a quarter of that, for every shipped radius: the run is needed and
    /// enough. The gate is `y = 10`, the feature `(10, 10 - r)` below it.
    #[test]
    fn gate_runs_keep_the_feature_across() {
        let ten = SimScalar::from_num(10);
        for sixteenths in SHIPPED_RADII {
            let radius = SimScalar::from_raw(sixteenths << 28);
            let feature = SimVec2::new(ten, ten - radius);
            let run = gate_run(radius);
            let leg = |s: SimScalar| {
                let y = ten - ZERO_ROOM_SLACK;
                (
                    SimVec2::new(ten + s, y),
                    SimVec2::new(ten + SimScalar::from_num(4), y),
                )
            };
            assert!(keeps(feature, leg(run), radius), "r {radius}: run {run}");
            assert!(
                !keeps(feature, leg(run / 4), radius),
                "r {radius}: a quarter run {} is already enough",
                run / 4
            );
        }
        assert_eq!(gate_run(SimScalar::ONE), SimScalar::from_raw(1 << 27));
        assert_eq!(gate_run(SimScalar::HALF), SimScalar::from_raw(1 << 26));
    }

    /// D-2.4 and plan §9: the circumscribed arc of a vertex across a pinch of exactly `2r`,
    /// with the route arriving and leaving at 30 degrees from the pinch's normal and following
    /// the arc's tangent lines from 3 units away.
    ///
    /// - An axis-aligned pinch (feature `(1.25, 0)` from the vertex) is snapped onto its gate,
    ///   and every leg keeps `r` from the vertex and from the feature.
    /// - A diagonal pinch on a Pythagorean lattice pair (feature `(0.75, 1.0)` from it) is not
    ///   snapped: its gate's direction `(-0.8, 0.6)` is not dyadic, so the run's points round off
    ///   the line and the predicate refuses the run by rounding alone (it keeps `r` less two Q16
    ///   steps). Such a geometry keeps the counted fallback (plan §9; evidence WP3). None was
    ///   found in the plan's 47,607 corpus routes.
    #[test]
    fn a_circumscribed_arc_passes_an_exact_pinch() {
        let radius = SimScalar::from_num(0.625);
        let rho = radius + ZERO_ROOM_SLACK;
        let c = SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(20));
        let quarter =
            |x: i32, y: i32| SimVec2::new(SimScalar::from_num(x) / 4, SimScalar::from_num(y) / 4);
        for (offset, degrees) in [(quarter(5, 0), 0.0), (quarter(3, 4), 53.130_102_354_155_98)] {
            let f = c + offset;
            for arc_sign in [1, -1] {
                let n_in = unit(degrees - 30.0 * f64::from(arc_sign));
                let n_out = unit(degrees + 30.0 * f64::from(arc_sign));
                let feature = NearFeature {
                    point: f,
                    distance_sq: 0,
                };
                let mut lines = Vec::new();
                arc_lines((c, rho), (n_in, n_out, arc_sign), &[feature], &mut lines);
                drop_redundant_lines(&mut lines, arc_sign);
                assert!(lines.iter().any(|l| l.gate.is_some()), "{lines:?}");
                let back = |n: SimVec2, sign: i32| {
                    c + n * rho + perp(n) * (SimScalar::from_num(3) * (arc_sign * sign))
                };
                let mut points = vec![back(n_in, -1)];
                emit_arc(
                    c,
                    &lines,
                    (gate_run(radius), radius, arc_sign),
                    [(false, None), (false, None)],
                    (&mut points, &mut Vec::new()),
                );
                points.push(back(n_out, 1));
                let diagonal = offset.x != SimScalar::ZERO && offset.y != SimScalar::ZERO;
                let rounding = SimScalar::from_raw(2 << 16);
                let mut all_keep_r = true;
                for pair in points.windows(2) {
                    for p in [c, f] {
                        all_keep_r &= keeps(p, (pair[0], pair[1]), radius);
                        assert!(
                            keeps(p, (pair[0], pair[1]), radius - rounding),
                            "offset {offset:?}, sense {arc_sign}: leg {pair:?} within r of {p:?}"
                        );
                    }
                }
                assert_eq!(all_keep_r, !diagonal, "offset {offset:?}, sense {arc_sign}");
            }
        }
    }

    /// Amendment 2: the zero-room arc's step is the largest power-of-two fraction of a turn whose
    /// circumscribed length factor `tan(θ/2) / (θ/2)` is at most the oracle bound 1.01, and the
    /// constant is its cosine floored to the raw grid.
    #[test]
    #[allow(
        clippy::cast_precision_loss,
        reason = "the raw constant is below 2^33, exact in an f64"
    )]
    fn zero_room_arc_step_is_the_largest_within_the_oracle_bound() {
        let factor = |splits: u32| {
            let half = std::f64::consts::PI / f64::from(1_u32 << splits);
            half.tan() / half
        };
        let splits = (2..16).find(|&k| factor(k) <= 1.01).unwrap_or(0);
        assert_eq!(splits, ZERO_ROOM_ARC_SPLITS);
        assert!(factor(splits) < 1.0033 && factor(splits - 1) > 1.01);
        let step = 2.0 * std::f64::consts::PI / f64::from(1_u32 << splits);
        let raw = (step.cos() * 4_294_967_296.0).floor();
        assert!((raw - ZERO_ROOM_ARC_STEP_COS.raw() as f64).abs() < 0.5);
    }

    /// The N1 layout of plan `2026-11-19-partial-knowledge-funnel` §3.4 on a flat 24 × 24 map: a
    /// known 2×8 at cell (6, 2) and an unknown 4×4 at (9, 9); returns the map and its mask.
    fn n1_map() -> (crate::grid::GameMap, Vec<bool>) {
        use crate::footprint::StructureFootprint;
        use crate::grid::{CliffLevel, GameMap, MapCell};
        use sim_types::EntityId;
        let mut map = GameMap::new(24, 24, MapCell::default_ground(CliffLevel(1)))
            .expect("invariant: 24 × 24 is a valid map size");
        map.place_structure(EntityId(1), (6, 2), StructureFootprint::square(2, 8))
            .expect("invariant: the 2×8 fits the empty map");
        map.place_structure(EntityId(2), (9, 9), StructureFootprint::square(4, 4))
            .expect("invariant: the 4×4 fits beside the 2×8");
        let mut known = vec![false; 24 * 24];
        for y in 2..10 {
            for x in 6..8 {
                known[y * 24 + x] = true;
            }
        }
        (map, known)
    }

    /// D-1: a free vertex has no circle. A tangent touches it, a portal's free end is not
    /// shrunk, and a pull through a channel that bends at a free vertex shared by two portals
    /// emits that vertex itself, once, with no waypoint of an arc round it.
    #[test]
    fn free_vertices_are_points() {
        use crate::navmesh::search::{find_route, MeshSearch};
        use crate::pathfinding::PathRequestWithVision;
        let (map, known) = n1_map();
        let mesh = NavMesh::build(&map).expect("invariant: a valid map builds a mesh");
        let radius = SimScalar::from_num(0.75);
        let request = PathRequestWithVision {
            start: SimVec2::new(SimScalar::from_num(3.5), SimScalar::from_num(4.5)),
            goal: SimVec2::new(SimScalar::from_num(15.5), SimScalar::from_num(14.5)),
            radius,
            vision: None,
            block_unexplored_ramps: false,
            known_structures: Some(&known),
        };
        let mut search = MeshSearch::default();
        find_route(&mesh, &mut search, &map, &request).expect("a route exists");
        let portals = search.channel().to_vec();
        let access = Access::new(&mesh, &map, &request);
        let mut segment = SegmentScratch::default();
        let mut scratch = FunnelScratch::default();
        let rho = radius + CORNER_SAFETY_MARGIN;
        let route = radius_funnel(
            &mesh,
            (request.start, request.goal),
            &portals,
            &[],
            (
                rho,
                Emission::Inscribed { dip: ARC_CHORD_DIP },
                PullStrategy::Shrunk,
            ),
            &mut Obstacles {
                access: &access,
                segment: &mut segment,
                scratch: &mut scratch,
            },
        );
        let open = |t: u32| access.open(t);
        let bends: Vec<u32> = route
            .anchors
            .iter()
            .filter_map(|&a| match a {
                Anchor::Vertex(v, _) if !mesh.vertex_blocks(v, open) => Some(v),
                _ => None,
            })
            .collect();
        assert!(
            !bends.is_empty(),
            "the route bends at no free vertex: {route:?}"
        );
        for v in bends {
            let shared = portals.windows(2).any(|pair| {
                [pair[0].0, pair[0].1].contains(&v) && [pair[1].0, pair[1].1].contains(&v)
            });
            assert!(shared, "vertex {v} is in no two consecutive portals");
            let c = mesh.vertex(v).to_sim();
            assert_eq!(
                route.waypoints.iter().filter(|&&w| w == c).count(),
                1,
                "the free vertex {c:?} is not emitted once: {route:?}"
            );
            assert!(
                route
                    .waypoints
                    .iter()
                    .all(|&w| w == c || w.distance(c) >= rho),
                "a waypoint on a circle round the free vertex {c:?}: {route:?}"
            );
            let free = [v];
            let left = Anchor::Vertex(v, Side::Left);
            let tangent = tangent(&mesh, (Anchor::Point(request.start), left), rho, &free);
            assert_eq!(tangent.to, c, "a tangent touches the free vertex itself");
        }
        let (a, b) = (
            SimVec2::ZERO,
            SimVec2::new(SimScalar::from_num(2), SimScalar::ZERO),
        );
        assert_eq!(shrink(a, b, rho, (false, false)), (a, b));
        let (_, far) = shrink(a, b, rho, (true, false));
        assert_eq!(far, b, "a free end is not shrunk");
    }
}
