//! Exact shortest paths for a disc among lattice walls: the reference the mesh search is
//! measured against (sc2 pathfinding WP6).
//!
//! # Responsibility
//! Computes the length of the shortest route of a disc of radius `rho` between two points among
//! wall segments, by Dijkstra over a visibility graph of tangents between circles of radius
//! `rho` around every wall vertex, joined by arcs around those circles. Among polygonal
//! obstacles the shortest route of a disc consists of exactly such tangents and arcs, so the
//! result is exact up to floating-point tolerance and the one-degree arc sampling below.
//!
//! # How it fits
//! `pathfinding_properties.rs` and `funnel_tests.rs` compare `Pathfinder::find_route_mesh`
//! against it. It shares no code with the mesh: walls are derived from `GameMap` directly.
//!
//! # Invariants
//! - Test-only, in `f64`; it is never part of the simulation.
//! - A tangent is usable only when it keeps at least `rho - EPS` from every wall; an arc only
//!   when every one-degree sample on it does.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::many_single_char_names,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    reason = "test oracle in floating point over small lattice maps"
)]

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use sim_map::{CellFlags, GameMap};

/// Tolerance of every clearance comparison, in world units.
pub const EPS: f64 = 1e-7;

type P = (f64, f64);

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}
fn add(a: P, b: P) -> P {
    (a.0 + b.0, a.1 + b.1)
}
fn mul(a: P, s: f64) -> P {
    (a.0 * s, a.1 * s)
}
fn dot(a: P, b: P) -> f64 {
    a.0 * b.0 + a.1 * b.1
}
fn cross(a: P, b: P) -> f64 {
    a.0 * b.1 - a.1 * b.0
}
fn len(a: P) -> f64 {
    dot(a, a).sqrt()
}
fn perp(a: P) -> P {
    (-a.1, a.0)
}

fn point_segment(p: P, a: P, b: P) -> f64 {
    let d = sub(b, a);
    let l = dot(d, d);
    let t = if l == 0.0 {
        0.0
    } else {
        (dot(sub(p, a), d) / l).clamp(0.0, 1.0)
    };
    len(sub(p, add(a, mul(d, t))))
}

fn segment_segment(a: P, b: P, c: P, d: P) -> f64 {
    let o1 = cross(sub(b, a), sub(c, a));
    let o2 = cross(sub(b, a), sub(d, a));
    let o3 = cross(sub(d, c), sub(a, c));
    let o4 = cross(sub(d, c), sub(b, c));
    if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
        return 0.0;
    }
    point_segment(a, c, d)
        .min(point_segment(b, c, d))
        .min(point_segment(c, a, b))
        .min(point_segment(d, a, b))
}

/// The walls of `map` as unit cell edges merged into runs, with known structures as blocked
/// cells: an edge is a wall where exactly one side is usable, or both are and terrain does not
/// connect them. The map border is a wall.
#[must_use]
pub fn walls_of(map: &GameMap) -> Vec<(P, P)> {
    let (w, h) = (map.width() as i64, map.height() as i64);
    let usable = |x: i64, y: i64| {
        x >= 0
            && y >= 0
            && x < w
            && y < h
            && map.terrain_step_is_connected((x as u32, y as u32), (x as u32, y as u32))
            && map
                .cell(x as u32, y as u32)
                .is_ok_and(|c| !c.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE))
    };
    let wall = |a: (i64, i64), b: (i64, i64)| {
        let (ua, ub) = (usable(a.0, a.1), usable(b.0, b.1));
        (ua != ub)
            || (ua
                && ub
                && !map
                    .terrain_step_is_connected((a.0 as u32, a.1 as u32), (b.0 as u32, b.1 as u32)))
    };
    let mut runs = Vec::new();
    for y in 0..=h {
        let mut start = None;
        for x in 0..=w {
            let is_wall = x < w && wall((x, y - 1), (x, y));
            match (is_wall, start) {
                (true, None) => start = Some(x),
                (false, Some(s)) => {
                    runs.push(((s as f64, y as f64), (x as f64, y as f64)));
                    start = None;
                }
                _ => {}
            }
        }
    }
    for x in 0..=w {
        let mut start = None;
        for y in 0..=h {
            let is_wall = y < h && wall((x - 1, y), (x, y));
            match (is_wall, start) {
                (true, None) => start = Some(y),
                (false, Some(s)) => {
                    runs.push(((x as f64, s as f64), (x as f64, y as f64)));
                    start = None;
                }
                _ => {}
            }
        }
    }
    runs
}

/// One graph node: a tangent point on a circle (or the start or goal).
#[derive(Clone, Copy)]
struct Node {
    circle: usize,
    /// -1: the obstacle is on the left (counter-clockwise travel); +1: on the right (clockwise);
    /// 0: an endpoint.
    sigma: i8,
    point: P,
    angle: f64,
}

/// The precomputed circle-to-circle tangents of one wall set and radius.
pub struct Oracle {
    walls: Vec<(P, P)>,
    corners: Vec<P>,
    rho: f64,
    /// Per circle, 360 one-degree bins: whether the arc sample there is blocked; prefix sums.
    blocked_prefix: Vec<Vec<u32>>,
    nodes: Vec<Node>,
    edges: Vec<(usize, usize, f64)>,
}

const NO_CIRCLE: usize = usize::MAX;

impl Oracle {
    /// Builds the graph for a disc of radius `rho` among `walls`.
    #[must_use]
    pub fn new(walls: Vec<(P, P)>, rho: f64) -> Self {
        let mut corners: Vec<P> = walls.iter().flat_map(|&(a, b)| [a, b]).collect();
        corners.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        corners.dedup();
        let mut oracle = Self {
            walls,
            corners,
            rho,
            blocked_prefix: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        oracle.blocked_prefix = (0..oracle.corners.len())
            .map(|i| {
                let mut prefix = vec![0u32; 361];
                for degree in 0..360 {
                    let angle = (degree as f64 + 0.5).to_radians();
                    let p = add(oracle.corners[i], mul((angle.cos(), angle.sin()), rho));
                    prefix[degree + 1] =
                        prefix[degree] + u32::from(oracle.clearance(p) < rho - EPS);
                }
                prefix
            })
            .collect();
        for i in 0..oracle.corners.len() {
            for j in 0..oracle.corners.len() {
                if i != j {
                    oracle.add_tangents(i, j);
                }
            }
        }
        oracle
    }

    /// Distance from `p` to the nearest wall.
    #[must_use]
    pub fn clearance(&self, p: P) -> f64 {
        self.walls
            .iter()
            .map(|&(a, b)| point_segment(p, a, b))
            .fold(f64::INFINITY, f64::min)
    }

    fn segment_clear(&self, a: P, b: P) -> bool {
        let r = self.rho - EPS;
        let (lo_x, hi_x) = (a.0.min(b.0) - r, a.0.max(b.0) + r);
        let (lo_y, hi_y) = (a.1.min(b.1) - r, a.1.max(b.1) + r);
        self.walls.iter().all(|&(c, d)| {
            c.0.max(d.0) < lo_x
                || c.0.min(d.0) > hi_x
                || c.1.max(d.1) < lo_y
                || c.1.min(d.1) > hi_y
                || segment_segment(a, b, c, d) >= r
        })
    }

    /// The tangent from `(ca, sa)` to `(cb, sb)`; `None` when it does not exist.
    fn tangent(&self, ca: P, sa: i8, cb: P, sb: i8) -> Option<(P, P)> {
        let d = sub(cb, ca);
        let k = self.rho * f64::from(sb - sa);
        let l2 = dot(d, d) - k * k;
        if l2 <= 0.0 {
            return None;
        }
        let u = mul(add(mul(d, l2.sqrt()), mul(perp(d), k)), 1.0 / dot(d, d));
        let n = perp(u);
        Some((
            add(ca, mul(n, self.rho * f64::from(sa))),
            add(cb, mul(n, self.rho * f64::from(sb))),
        ))
    }

    fn add_tangents(&mut self, i: usize, j: usize) {
        for sa in [-1i8, 1] {
            for sb in [-1i8, 1] {
                let Some((ta, tb)) = self.tangent(self.corners[i], sa, self.corners[j], sb) else {
                    continue;
                };
                if self.segment_clear(ta, tb) {
                    let a = push_node(&mut self.nodes, &self.corners, i, sa, ta);
                    let b = push_node(&mut self.nodes, &self.corners, j, sb, tb);
                    self.edges.push((a, b, len(sub(tb, ta))));
                }
            }
        }
    }

    /// Whether the arc of circle `c` from angle `from` travelling in direction `sigma` to `to`
    /// is clear; returns its length.
    fn arc(&self, c: usize, sigma: i8, from: f64, to: f64) -> Option<f64> {
        // sigma +1 keeps the obstacle on the right of travel: the route turns clockwise.
        let sweep = if sigma < 0 {
            (to - from).rem_euclid(std::f64::consts::TAU)
        } else {
            (from - to).rem_euclid(std::f64::consts::TAU)
        };
        let (lo, hi) = if sigma < 0 {
            (from, from + sweep)
        } else {
            (to, to + sweep)
        };
        let first = (lo.to_degrees() - 0.5 + 1e-9).ceil() as i64;
        let last = (hi.to_degrees() - 0.5 - 1e-9).floor() as i64;
        let prefix = &self.blocked_prefix[c];
        let mut blocked = 0;
        let mut bin = first;
        while bin <= last {
            let b = bin.rem_euclid(360) as usize;
            let run_end = (last - bin).min(359 - b as i64) as usize + b;
            blocked += prefix[run_end + 1] - prefix[b];
            bin += (run_end - b) as i64 + 1;
        }
        (blocked == 0).then_some(sweep * self.rho)
    }

    /// Length of the shortest route of the disc from `s` to `g`, or `None` if there is none.
    #[must_use]
    pub fn shortest(&self, s: P, g: P) -> Option<f64> {
        if self.clearance(s) < self.rho - EPS || self.clearance(g) < self.rho - EPS {
            return None;
        }
        let mut nodes = self.nodes.clone();
        let mut edges = self.edges.clone();
        let source = push_node(&mut nodes, &self.corners, NO_CIRCLE, 0, s);
        let target = push_node(&mut nodes, &self.corners, NO_CIRCLE, 0, g);
        if self.segment_clear(s, g) {
            edges.push((source, target, len(sub(g, s))));
        }
        for c in 0..self.corners.len() {
            for sigma in [-1i8, 1] {
                if let Some((_, t)) = self.tangent(s, 0, self.corners[c], sigma) {
                    if self.segment_clear(s, t) {
                        let n = push_node(&mut nodes, &self.corners, c, sigma, t);
                        edges.push((source, n, len(sub(t, s))));
                    }
                }
                if let Some((t, _)) = self.tangent(self.corners[c], sigma, g, 0) {
                    if self.segment_clear(t, g) {
                        let n = push_node(&mut nodes, &self.corners, c, sigma, t);
                        edges.push((n, target, len(sub(g, t))));
                    }
                }
            }
        }
        self.add_arcs(&nodes, &mut edges);
        dijkstra(nodes.len(), &edges, source, target)
    }

    /// Joins consecutive tangent points of each circle and direction with arcs.
    fn add_arcs(&self, nodes: &[Node], edges: &mut Vec<(usize, usize, f64)>) {
        let mut groups: Vec<Vec<usize>> = vec![Vec::new(); 2 * self.corners.len()];
        for (index, node) in nodes.iter().enumerate() {
            if node.circle != NO_CIRCLE {
                groups[2 * node.circle + usize::from(node.sigma > 0)].push(index);
            }
        }
        for (key, group) in groups.iter_mut().enumerate() {
            if group.len() < 2 {
                continue;
            }
            let (c, sigma) = (key / 2, if key % 2 == 1 { 1i8 } else { -1 });
            group.sort_by(|&a, &b| {
                nodes[a]
                    .angle
                    .partial_cmp(&nodes[b].angle)
                    .unwrap_or(Ordering::Equal)
            });
            if sigma > 0 {
                group.reverse();
            }
            for w in 0..group.len() {
                let (a, b) = (group[w], group[(w + 1) % group.len()]);
                if let Some(length) = self.arc(c, sigma, nodes[a].angle, nodes[b].angle) {
                    edges.push((a, b, length));
                }
            }
        }
    }
}

fn push_node(nodes: &mut Vec<Node>, corners: &[P], circle: usize, sigma: i8, point: P) -> usize {
    let angle = if circle == NO_CIRCLE {
        0.0
    } else {
        let v = sub(point, corners[circle]);
        v.1.atan2(v.0).rem_euclid(std::f64::consts::TAU)
    };
    nodes.push(Node {
        circle,
        sigma,
        point,
        angle,
    });
    nodes.len() - 1
}

#[derive(PartialEq)]
struct State(f64, usize);
impl Eq for State {}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .partial_cmp(&self.0)
            .unwrap_or(Ordering::Equal)
            .then(self.1.cmp(&other.1))
    }
}

fn dijkstra(n: usize, edges: &[(usize, usize, f64)], source: usize, target: usize) -> Option<f64> {
    let mut adjacency: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for &(a, b, w) in edges {
        adjacency[a].push((b, w));
    }
    let mut dist = vec![f64::INFINITY; n];
    dist[source] = 0.0;
    let mut heap = BinaryHeap::new();
    heap.push(State(0.0, source));
    while let Some(State(d, u)) = heap.pop() {
        if u == target {
            return Some(d);
        }
        if d > dist[u] {
            continue;
        }
        for &(v, w) in &adjacency[u] {
            if d + w < dist[v] {
                dist[v] = d + w;
                heap.push(State(d + w, v));
            }
        }
    }
    None
}

/// The length of a mesh route measured as tangents plus arcs, from its turns (smooth length),
/// so that it compares with [`Oracle::shortest`] without the polyline's corner cuts.
#[must_use]
pub fn smooth_length(start: P, goal: P, turns: &[(P, i8)], rho: f64) -> f64 {
    let mut anchors: Vec<(P, i8)> = vec![(start, 0)];
    anchors.extend_from_slice(turns);
    anchors.push((goal, 0));
    let probe = Oracle {
        walls: Vec::new(),
        corners: Vec::new(),
        rho,
        blocked_prefix: Vec::new(),
        nodes: Vec::new(),
        edges: Vec::new(),
    };
    let mut total = 0.0;
    let mut points: Vec<(P, P)> = Vec::new();
    for pair in anchors.windows(2) {
        let ((ca, sa), (cb, sb)) = (pair[0], pair[1]);
        let (ta, tb) = probe.tangent(ca, sa, cb, sb).unwrap_or((ca, cb));
        total += len(sub(tb, ta));
        points.push((ta, tb));
    }
    for j in 1..anchors.len() - 1 {
        let (c, sigma) = anchors[j];
        let a = sub(points[j - 1].1, c);
        let b = sub(points[j].0, c);
        let mut sweep = b.1.atan2(b.0) - a.1.atan2(a.0);
        if sigma > 0 {
            sweep = -sweep;
        }
        total += sweep.rem_euclid(std::f64::consts::TAU) * rho;
    }
    total
}
