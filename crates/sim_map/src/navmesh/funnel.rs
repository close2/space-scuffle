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
//! - An arc turning by at most 45 degrees becomes one waypoint, where the two tangent lines meet;
//!   a larger arc becomes two, where the tangent lines meet the tangent at the arc's middle. Each
//!   leg is then tangent to the circle or outside it, so it keeps at least `r + margin` from the
//!   vertex it turns around.

use sim_types::{SimScalar, SimVec2};

use super::NavMesh;

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

/// Offset sign of the route from an anchor's centre, along the left normal of travel.
fn sigma(anchor: Anchor) -> i32 {
    match anchor {
        Anchor::Point(_) => 0,
        Anchor::Vertex(_, Side::Left) => -1,
        Anchor::Vertex(_, Side::Right) => 1,
    }
}

/// The tangent from `from`'s circle to `to`'s circle, both of radius `rho` (points have none).
///
/// With `d` the centre difference and `k = (sigma_to - sigma_from) rho`, the unit direction is
/// `(L d + k perp(d)) / |d|^2` with `L = sqrt(|d|^2 - k^2)`; a degenerate pair (one circle within
/// reach of the other) gets `L = 0`, the direction perpendicular to `d`.
fn tangent(mesh: &NavMesh, from: Anchor, to: Anchor, rho: SimScalar) -> Tangent {
    let (a, b) = (centre(mesh, from), centre(mesh, to));
    let d = b - a;
    let k = rho * (sigma(to) - sigma(from));
    let length_sq = d.length_sq();
    let l = (length_sq - k * k)
        .max(SimScalar::ZERO)
        .sqrt()
        .unwrap_or(SimScalar::ZERO);
    let direction = (d * l + perp(d) * k).normalize_or_zero();
    let normal = perp(direction);
    Tangent {
        direction,
        from: a + normal * (rho * sigma(from)),
        to: b + normal * (rho * sigma(to)),
    }
}

/// Runs the radius funnel through `portals` (`(left, right)` vertex indices in travel order).
///
/// `rho` is the unit's radius plus [`crate::CORNER_SAFETY_MARGIN`]. `extra` lists the vertices
/// of the start and goal triangles, which bound the route but belong to no portal.
#[must_use]
pub fn radius_funnel(
    mesh: &NavMesh,
    (start, goal): (SimVec2, SimVec2),
    portals: &[(u32, u32)],
    extra: &[u32],
    rho: SimScalar,
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
    let anchors = string_pull(mesh, &items, extra, rho);
    let waypoints = emit_waypoints(mesh, &anchors, rho);
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

/// Moves both ends of a portal inwards by `rho`; a portal no longer than `2 rho` shrinks to its
/// midpoint.
fn shrink(a: SimVec2, b: SimVec2, rho: SimScalar) -> (SimVec2, SimVec2) {
    let d = b - a;
    let length = d.length();
    if length <= rho * 2 {
        let mid = (a + b) / 2;
        return (mid, mid);
    }
    let step = d * (rho / length);
    (a + step, b - step)
}

fn triarea2(a: SimVec2, b: SimVec2, c: SimVec2) -> SimScalar {
    (b - a).cross_2d(c - a)
}

/// The route's turning anchors through the channel `items` (start and goal included).
///
/// Step 1 pulls a point through the portals shrunk by `rho` (Mononen's Simple Stupid Funnel),
/// which orders the turns robustly even where circles overlap the apex. Step 2 makes the turns
/// exact for circles: a turn that bends against its side is dropped, and a channel vertex
/// closer than `rho` to a tangent leg is inserted, until neither happens.
fn string_pull(
    mesh: &NavMesh,
    items: &[(Anchor, Anchor)],
    extra: &[u32],
    rho: SimScalar,
) -> Vec<Anchor> {
    let points: Vec<(SimVec2, SimVec2)> = items
        .iter()
        .map(|&(l, r)| {
            if let Anchor::Point(p) = l {
                (p, p)
            } else {
                shrink(centre(mesh, l), centre(mesh, r), rho)
            }
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
    refine(mesh, items, extra, path, rho)
}

/// Whether the route turns at anchor `j` in the sense its side requires.
fn turns_with_side(mesh: &NavMesh, anchors: &[Anchor], j: usize, rho: SimScalar) -> bool {
    let Anchor::Vertex(_, side) = anchors[j] else {
        return true;
    };
    let incoming = tangent(mesh, anchors[j - 1], anchors[j], rho).direction;
    let outgoing = tangent(mesh, anchors[j], anchors[j + 1], rho).direction;
    let turn = incoming.cross_2d(outgoing);
    match side {
        Side::Left => turn >= SimScalar::ZERO,
        Side::Right => turn <= SimScalar::ZERO,
    }
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

/// The channel vertex deepest inside `rho` of the tangent leg from `anchors[j]` to
/// `anchors[j + 1]`, if any.
fn deepest_violation(
    mesh: &NavMesh,
    items: &[(Anchor, Anchor)],
    extra: &[u32],
    anchors: &[Anchor],
    j: usize,
    rho: SimScalar,
) -> Option<Anchor> {
    let leg = tangent(mesh, anchors[j], anchors[j + 1], rho);
    let rounded = |v: u32| {
        anchors
            .iter()
            .any(|a| matches!(*a, Anchor::Vertex(w, _) if w == v))
    };
    // The start and goal triangles' own vertices are in no portal; their side is the side of
    // the leg they lie on.
    let side_of = |v: u32| {
        if leg.direction.cross_2d(mesh.vertex(v).to_sim() - leg.from) > SimScalar::ZERO {
            Side::Left
        } else {
            Side::Right
        }
    };
    let candidates = items[1..items.len() - 1]
        .iter()
        .flat_map(|&(l, r)| [l, r])
        .chain(extra.iter().map(|&v| Anchor::Vertex(v, side_of(v))));
    let mut worst: Option<(SimScalar, Anchor)> = None;
    for candidate in candidates {
        let Anchor::Vertex(v, _) = candidate else {
            continue;
        };
        if rounded(v) {
            continue;
        }
        let distance = distance_to_segment(centre(mesh, candidate), leg.from, leg.to);
        // A hair inside the circle is rounding, not a collision.
        if distance + SimScalar::from_raw(1 << 12) < rho && worst.is_none_or(|(d, _)| distance < d)
        {
            worst = Some((distance, candidate));
        }
    }
    worst.map(|(_, anchor)| anchor)
}

/// Step 2 of [`string_pull`]: drops turns against their side and inserts violated vertices.
fn refine(
    mesh: &NavMesh,
    items: &[(Anchor, Anchor)],
    extra: &[u32],
    mut anchors: Vec<Anchor>,
    rho: SimScalar,
) -> Vec<Anchor> {
    for _ in 0..4 * items.len() + 8 {
        if let Some(j) =
            (1..anchors.len().saturating_sub(1)).find(|&j| !turns_with_side(mesh, &anchors, j, rho))
        {
            anchors.remove(j);
            continue;
        }
        let violation = (0..anchors.len() - 1)
            .find_map(|j| deepest_violation(mesh, items, extra, &anchors, j, rho).map(|v| (j, v)));
        match violation {
            Some((j, vertex)) => anchors.insert(j + 1, vertex),
            None => break,
        }
    }
    anchors
}

/// cos(45 degrees) in Q32.32.
const COS_45: SimScalar = SimScalar::from_raw(3_037_000_500);

/// Smallest `1 + cos` used when placing a corner point, so a near-reversal stays bounded.
const MIN_HALF_COS: SimScalar = SimScalar::from_raw(1 << 30);

/// Where the tangent lines at unit normals `n1` and `n2` of a circle of radius `rho` meet.
fn corner_point(c: SimVec2, n1: SimVec2, n2: SimVec2, rho: SimScalar) -> SimVec2 {
    let denominator = (SimScalar::ONE + n1.dot(n2)).max(MIN_HALF_COS);
    c + (n1 + n2) * (rho / denominator)
}

/// Replaces each rounded vertex's arc by one or two waypoints (D-6).
fn emit_waypoints(mesh: &NavMesh, anchors: &[Anchor], rho: SimScalar) -> Vec<SimVec2> {
    let tangents: Vec<Tangent> = anchors
        .windows(2)
        .map(|pair| tangent(mesh, pair[0], pair[1], rho))
        .collect();
    let mut points = vec![centre(mesh, anchors[0])];
    for j in 1..anchors.len().saturating_sub(1) {
        let Anchor::Vertex(_, side) = anchors[j] else {
            points.push(centre(mesh, anchors[j]));
            continue;
        };
        let c = centre(mesh, anchors[j]);
        let n_in = (tangents[j - 1].to - c).normalize_or_zero();
        let n_out = (tangents[j].from - c).normalize_or_zero();
        if n_in == SimVec2::ZERO || n_out == SimVec2::ZERO {
            points.push(tangents[j - 1].to);
            points.push(tangents[j].from);
            continue;
        }
        let arc_sign = if side == Side::Left { 1 } else { -1 };
        let turns_forward = n_in.cross_2d(n_out) * arc_sign >= SimScalar::ZERO;
        if turns_forward && n_in.dot(n_out) >= COS_45 {
            points.push(corner_point(c, n_in, n_out, rho));
            continue;
        }
        let sum = n_in + n_out;
        let middle = if sum.length_sq() > SimScalar::from_raw(1 << 20) {
            let bisector = sum.normalize_or_zero();
            if turns_forward {
                bisector
            } else {
                -bisector
            }
        } else {
            perp(n_in) * arc_sign
        };
        points.push(corner_point(c, n_in, middle, rho));
        points.push(corner_point(c, middle, n_out, rho));
    }
    points.push(centre(mesh, anchors[anchors.len() - 1]));
    points.dedup();
    points
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

/// The unit travel direction of the tangent between two anchors, exposed for tests.
#[must_use]
pub fn tangent_direction(mesh: &NavMesh, from: Anchor, to: Anchor, rho: SimScalar) -> SimVec2 {
    tangent(mesh, from, to, rho).direction
}
