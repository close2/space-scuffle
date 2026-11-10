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
//!   halved until every chord keeps at least `r + margin - ARC_CHORD_DIP` from the vertex it turns
//!   around (plan Amendment 2). The polyline is therefore never longer than the tangent-and-arc
//!   route, and every leg keeps at least `r + margin / 2` from the vertices it rounds.

use sim_types::{SimScalar, SimVec2};

use super::NavMesh;
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
/// `rho` is the unit's radius plus [`crate::CORNER_SAFETY_MARGIN`]. `extra` lists the vertices
/// of the start and goal triangles, which bound the route but belong to no portal.
#[must_use]
pub fn radius_funnel(
    mesh: &NavMesh,
    (start, goal): (SimVec2, SimVec2),
    portals: &[(u32, u32)],
    extra: &[u32],
    (rho, strategy): (SimScalar, PullStrategy),
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
    let anchors = string_pull(mesh, &items, extra, rho, strategy);
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
    strategy: PullStrategy,
) -> Vec<Anchor> {
    let points: Vec<(SimVec2, SimVec2)> = items
        .iter()
        .map(|&(l, r)| match (l, strategy) {
            (Anchor::Point(p), _) => (p, p),
            (_, PullStrategy::Shrunk) => shrink(centre(mesh, l), centre(mesh, r), rho),
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
    refine(mesh, items, extra, path, (rho, strategy))
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
) -> bool {
    let Anchor::Vertex(_, side) = anchors[j] else {
        return true;
    };
    let incoming = tangent(mesh, anchors[j - 1], anchors[j], rho).direction;
    let outgoing = tangent(mesh, anchors[j], anchors[j + 1], rho).direction;
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
    (rho, strategy): (SimScalar, PullStrategy),
) -> Vec<Anchor> {
    for _ in 0..4 * items.len() + 8 {
        if let Some(j) = (1..anchors.len().saturating_sub(1))
            .find(|&j| !turns_with_side(mesh, &anchors, j, (rho, strategy)))
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
/// least `rho - ARC_CHORD_DIP` from the vertex. The polyline is then never longer than the
/// tangent-and-arc route and dips at most [`ARC_CHORD_DIP`] inside its circle.
fn emit_waypoints(mesh: &NavMesh, anchors: &[Anchor], rho: SimScalar) -> Vec<SimVec2> {
    let tangents: Vec<Tangent> = anchors
        .windows(2)
        .map(|pair| tangent(mesh, pair[0], pair[1], rho))
        .collect();
    let floor = rho - ARC_CHORD_DIP;
    let mut points = vec![centre(mesh, anchors[0])];
    for j in 1..anchors.len().saturating_sub(1) {
        let Anchor::Vertex(_, side) = anchors[j] else {
            points.push(centre(mesh, anchors[j]));
            continue;
        };
        let c = centre(mesh, anchors[j]);
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
