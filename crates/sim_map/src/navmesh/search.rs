//! Triangle A* (TA*, Demyen and Buro 2006) over the navigation mesh (plan D-5), and the mesh
//! queries built on it: routes, the nearest reachable point, and the segment predicate.
//!
//! # Responsibility
//! Finds a channel of triangles from the start to the goal that a disc of the request's radius
//! can traverse (the width test of D-4 at every triangle passed through), pulls the route through
//! it with the radius funnel (D-6), and validates every leg with the segment predicate (D-7).
//!
//! # How it fits
//! `Pathfinder::find_path_with_vision`, `Pathfinder::segment_is_passable` and
//! `Pathfinder::nearest_reachable_point` are thin wrappers over this module; since WP9 every
//! route, every route check and every unreachable-goal answer of the simulation comes from it.
//!
//! # Invariants
//! - Search nodes are paths, not triangles (TA*): a triangle may be reached along several
//!   channels, never twice on one channel. `g` is the largest of three lower bounds on a route
//!   of the channel to the entry edge: the straight distance from the start, each of up to eight
//!   ancestors' `g` plus the distance between the two edges, and the channel's point geodesic
//!   (plan `2026-11-20-tastar-budget` D-1: the shortest point path from the start through the
//!   channel's portals to its last bend, plus the distance from that bend to the edge). `h` is the
//!   distance from the goal to the entry edge. All are admissible: every pull of the channel
//!   crosses its portals in order, inside every portal end except a clampable one (a free vertex
//!   within `r + CORNER_SAFETY_MARGIN` of an obstacle vertex, which the radius funnel may clamp
//!   outwards), and the point funnel gives a clampable end no side.
//! - Each node carries the point funnel of its channel (apex, path length, two sides), so a
//!   child's bound costs one funnel step; the funnel is equal to the one recomputed from the
//!   start through the node's portals.
//! - The first channel to reach the goal sets the incumbent (its funnel length). The search goes
//!   on while an open node's `f` is below the incumbent, for at most
//!   [`NAV_TA_STAR_EXTRA_EXPANSIONS`] further expansions (S11's anytime loop).
//! - Ties break on `(f, triangle, node)`; every container is a `Vec` or `BinaryHeap` filled in
//!   index order, so the result is a pure function of the mesh and the request.
//! - The start triangle is entered whatever its width (D-3); every later triangle passed through
//!   must admit the disc between its entry and exit edges.
//! - A vertex at which no edge blocks the request (a free vertex, such as a corner of a structure
//!   the request does not know) bounds nothing: the width test treats it as unbounded, the funnel
//!   pulls it as a point, and the fitted retry's channel width leaves it out (plan
//!   `2026-11-19-partial-knowledge-funnel` D-1, D-5).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use sim_types::{SimScalar, SimVec2};

use super::cdt::NONE;
use super::funnel::{
    portal_midpoints, radius_funnel, Anchor, Emission, FunnelRoute, FunnelScratch, Obstacles,
    PullStrategy, Side, ARC_CHORD_DIP, ZERO_ROOM_SLACK,
};
use super::lanes::{LaneCuts, LaneScratch, LaneWord, WORD_ROOT};
use super::predicates::LatticePoint;
use super::predicates::{lattice_distance_sq, lattice_point_segment_width, SquaredWidth};
use super::segment::{
    nearest_blocking_point, nearest_blocking_point_exhaustive, obstacle_vertices_near,
    segment_clear, SegmentScratch, WallContact,
};
use super::{next, prev, Access, NavMesh};
use crate::error::{MapError, Result};
use crate::grid::GameMap;
use crate::pathfinding::{PathRequestWithVision, CORNER_SAFETY_MARGIN};

/// Expansions TA* may spend after its first route to the goal, looking for a shorter channel.
///
/// Measured in WP8 (`docs/evidence/2026-11-09-sc2-pathfinding.md`): on both shipped maps the
/// best channel is found within this budget for every query of the differential run.
pub const NAV_TA_STAR_EXTRA_EXPANSIONS: u32 = 256;

/// How many search nodes may expand the same (triangle, entry edge) state (bounds TA*'s
/// path-based node count; the first is always expanded, so no route is ever missed).
pub const NAV_TA_STAR_TRIANGLE_VISITS: u8 = 4;

const NO_EDGE: u8 = 3;

#[derive(Debug, Clone, Copy)]
struct Node {
    triangle: u32,
    /// Local index of the entry edge in `triangle`, or [`NO_EDGE`] for the start.
    entry: u8,
    /// Local index, in the parent's triangle, of the edge this node was entered through.
    via: u8,
    parent: u32,
    g: SimScalar,
    h: SimScalar,
    /// Interned lane word of the node's portal-midpoint polyline ([`WORD_ROOT`] when the search
    /// is not restricted to a lane).
    word: u32,
    /// Portals crossed from the start: 0 for the start node, its parent's plus one otherwise.
    depth: u32,
    /// The point funnel through the node's channel, up to and including its entry portal (D-1
    /// of plan `2026-11-20-tastar-budget`).
    // PERFORMANCE RATIONALE:
    // Why: The channel bound is read for every child, and recomputing the funnel from the start
    //      is O(depth) per child: +11.8 % instructions on the showcase throughput scenario.
    // What: Each node carries its funnel state, and a child advances it by its one entry portal;
    //      only when the apex moves are the portals since the new apex re-read from the ancestors.
    // Trade-off: Three points, a length and three depths more per node, and a re-read of the
    //      portals since the apex when it moves.
    funnel: PointFunnel,
}

/// A portal of the point funnel (D-1 of plan `2026-11-20-tastar-budget`): its ends, left and
/// right as the route crosses it, and whether each end constrains the funnel.
#[derive(Debug, Clone, Copy)]
struct BoundPortal {
    left: SimVec2,
    right: SimVec2,
    /// Whether the left end bounds the funnel (false for a clampable end, see
    /// `MeshSearch::clampable`).
    left_bounds: bool,
    /// Whether the right end bounds the funnel.
    right_bounds: bool,
}

/// The point funnel (Simple Stupid Funnel, Mononen) through a node's channel, start first: the
/// shortest point path from the start through the portals, cut at its last bend, and the two
/// sides of the funnel beyond it. Depths count portals from the start (the start is depth 0).
///
/// It is a separate, point-only funnel, not the radius funnel of `funnel.rs`: that one pulls a
/// disc round shrunk portals and circles, and only once per channel that reaches the goal; this
/// one is advanced at every node, so it holds a lower bound on every pull of the channel and
/// costs one step per child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PointFunnel {
    /// The last bend of the shortest point path (the start until the path bends).
    apex: SimVec2,
    /// Length of the shortest point path from the start to `apex`, in world units.
    length: SimScalar,
    /// Depth of the portal whose end is `apex`.
    apex_depth: u32,
    left: SimVec2,
    left_depth: u32,
    right: SimVec2,
    right_depth: u32,
}

impl PointFunnel {
    /// The funnel of the start node: apex and both sides at `start`.
    fn at(start: SimVec2) -> Self {
        Self {
            apex: start,
            length: SimScalar::ZERO,
            apex_depth: 0,
            left: start,
            left_depth: 0,
            right: start,
            right_depth: 0,
        }
    }

    /// Moves the apex to a side point at `depth`, `distance` further along the path.
    fn bend(&mut self, point: SimVec2, depth: u32) {
        self.length += self.apex.distance(point);
        self.apex = point;
        self.apex_depth = depth;
        (self.left, self.left_depth) = (point, depth);
        (self.right, self.right_depth) = (point, depth);
    }

    /// Advances the funnel through `portals`, the first of which is at depth `first`.
    ///
    /// A portal end that bounds narrows its side when it lies inside the funnel; when it crosses
    /// the other side, the apex moves to that side and the portals after it are taken again.
    /// The tests are inclusive (`>= 0` to narrow, `<= 0` not to cross): a portal end on the line
    /// of the other side does not move the apex (T4 of the plan, a start on the first portal's
    /// line). Returns false when the apex moved to a side set before `portals`: the state then
    /// holds the new apex, and the caller advances it again through every portal after the apex.
    fn advance(&mut self, portals: &[BoundPortal], first: u32) -> bool {
        let zero = SimScalar::ZERO;
        let mut i = 0;
        while let Some(portal) = portals.get(i) {
            let depth = first + u32::try_from(i).unwrap_or(u32::MAX - first);
            let apex = self.apex;
            if portal.right_bounds && (self.right - apex).cross_2d(portal.right - apex) >= zero {
                if apex == self.right || (self.left - apex).cross_2d(portal.right - apex) <= zero {
                    (self.right, self.right_depth) = (portal.right, depth);
                } else {
                    self.bend(self.left, self.left_depth);
                    let Some(next) = self.apex_depth.checked_sub(first) else {
                        return false;
                    };
                    i = next as usize + 1;
                    continue;
                }
            }
            let apex = self.apex;
            if portal.left_bounds && (self.left - apex).cross_2d(portal.left - apex) <= zero {
                if apex == self.left || (self.right - apex).cross_2d(portal.left - apex) >= zero {
                    (self.left, self.left_depth) = (portal.left, depth);
                } else {
                    self.bend(self.right, self.right_depth);
                    let Some(next) = self.apex_depth.checked_sub(first) else {
                        return false;
                    };
                    i = next as usize + 1;
                    continue;
                }
            }
            i += 1;
        }
        true
    }

    /// A lower bound on the length of any route through the funnel's channel from the start to
    /// the edge `[a, b]` beyond its last portal: the path to the apex, then straight to the edge.
    fn bound_to(&self, a: SimVec2, b: SimVec2) -> SimScalar {
        self.length + point_segment_distance(self.apex, a, b)
    }
}

/// A route found on the mesh, with what the search learned on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshRoute {
    /// Waypoints in world units, start first and goal last.
    pub waypoints: Vec<SimVec2>,
    /// Sum of the leg lengths, in world units.
    pub total_length: SimScalar,
    /// The channel vertices the route turns around, with the side it passes them on.
    pub turns: Vec<(LatticePoint, Side)>,
    /// Whether a funnel leg failed the segment predicate and the portal midpoints were used.
    pub fallback: bool,
    /// Whether the zero-room retry (plan `2026-11-19-funnel-fallbacks` D-2) produced the route:
    /// every pull with a corner margin failed, and the route passes a pinch with no room to
    /// spare. A diagnostic, read by the tests that hold these routes to the oracle (Amendment 2).
    pub zero_room: bool,
    /// Search nodes expanded.
    pub expansions: u32,
}

/// Reusable buffers of the mesh queries.
#[derive(Debug, Default)]
pub struct MeshSearch {
    nodes: Vec<Node>,
    open: BinaryHeap<Reverse<(i64, u32, u32)>>,
    visits: Vec<u8>,
    portals: Vec<(u32, u32)>,
    /// Flood stack of [`nearest_reachable_point`]: `(triangle, entry edge)` states.
    flood_stack: Vec<(u32, u8)>,
    /// Reachable cells of [`nearest_reachable_point`]: `(distance squared, cell index, x)`.
    candidates: Vec<(u64, u32, u32)>,
    pub(crate) segment: SegmentScratch,
    /// Buffers of the funnel's refinement (plan `2026-11-19-funnel-fallbacks` D-3).
    funnel: FunnelScratch,
    /// Word trie and word-keyed visit lists of the lane-restricted search (D-5 of plan
    /// `2026-11-10-group-routing`).
    lanes: LaneScratch,
    /// Funnel legs that failed the segment predicate since construction (a diagnostic, D-6).
    pub fallbacks: u64,
    /// Per mesh vertex, whether it is clampable for the current search, stamped:
    /// `2 * generation + answer`, any other value meaning not yet asked.
    // PERFORMANCE RATIONALE:
    // Why: The clampable test is a fan walk and a flood, asked for the same vertex by many
    //      channels of one search.
    // What: A word per mesh vertex holding the answer under the search's generation, so a new
    //      search starts by bumping the generation instead of clearing.
    // Trade-off: One word per vertex kept between searches.
    clampable: Vec<u32>,
    /// The generation of `clampable`'s stamps.
    generation: u32,
    /// Obstacle vertices near a vertex, the clampable test's buffer.
    near: Vec<u32>,
    /// Portals re-read from a node's ancestors when its funnel's apex moves.
    bound_portals: Vec<BoundPortal>,
}

/// The vector from the point of `[a, b]` nearest `p` to `p`, in world units.
fn offset_from_segment(p: SimVec2, a: SimVec2, b: SimVec2) -> SimVec2 {
    let d = b - a;
    let length_sq = d.length_sq();
    if length_sq == SimScalar::ZERO {
        return p - a;
    }
    let t = ((p - a).dot(d) / length_sq).clamp(SimScalar::ZERO, SimScalar::ONE);
    p - (a + d * t)
}

/// Distance from `p` to the segment `[a, b]`, in world units.
fn point_segment_distance(p: SimVec2, a: SimVec2, b: SimVec2) -> SimScalar {
    offset_from_segment(p, a, b).length()
}

/// Exact squared length of `v` in raw units, the order [`SimVec2::length`] preserves.
fn raw_length_sq(v: SimVec2) -> u128 {
    let (x, y) = (
        u128::from(v.x.raw().unsigned_abs()),
        u128::from(v.y.raw().unsigned_abs()),
    );
    x * x + y * y
}

/// Distance between two mesh edges, which meet at most at a shared vertex.
// PERFORMANCE RATIONALE:
// Why: TA* bounds every child against up to eight ancestor edges, four point-to-segment distances
// each; a 128-bit square root per distance was the search's largest single cost (WP9 profile).
// What: Compare the four offsets by their exact squared lengths and take one root of the least.
// The integer root is monotonic, so the result is bit-identical to the least of four roots.
// Trade-off: A local squared-length helper next to `SimVec2::length`.
fn segment_distance(a: SimVec2, b: SimVec2, c: SimVec2, d: SimVec2) -> SimScalar {
    [
        offset_from_segment(a, c, d),
        offset_from_segment(b, c, d),
        offset_from_segment(c, a, b),
        offset_from_segment(d, a, b),
    ]
    .into_iter()
    .min_by_key(|&offset| raw_length_sq(offset))
    .map_or(SimScalar::ZERO, SimVec2::length)
}

/// How many ancestors' entry edges bound a node's `g` (see `MeshSearch::lower_bound`).
const NAV_TA_STAR_BOUND_ANCESTORS: usize = 8;

/// The lane restriction of a search (D-5 of plan `2026-11-10-group-routing`).
#[derive(Clone, Copy)]
struct LaneQuery<'a> {
    cuts: &'a LaneCuts,
    /// Interned id of the target word.
    target: u32,
    /// `K`: how many letters a node's word may carry beyond its longest common prefix with the
    /// target before the node is pruned.
    bound: u32,
}

#[derive(Clone, Copy)]
struct Query<'a> {
    mesh: &'a NavMesh,
    access: &'a Access<'a>,
    start: SimVec2,
    goal: SimVec2,
    goal_triangle: u32,
    radius: SimScalar,
    /// Whether the disc fits where it starts; a wedged start skips the start-triangle check.
    start_fits: bool,
    /// Whether the start and goal triangles get the straight-sweep reach check. It runs only in
    /// a second pass, when the first route fails the segment predicate: the check samples
    /// straight sweeps only, so it can refuse a triangle the disc crosses on a bend.
    end_checks: bool,
    /// The lane the route must keep, or `None` for an unrestricted search.
    lane: Option<LaneQuery<'a>>,
}

impl MeshSearch {
    fn reset(&mut self, triangles: usize, lane: bool) {
        self.nodes.clear();
        self.open.clear();
        if lane {
            self.lanes.reset_visits(triangles);
        } else {
            self.visits.clear();
            self.visits.resize(3 * triangles, 0);
        }
    }

    /// Starts a new generation of the clampable stamps, sized to `vertices`.
    fn begin_clampable(&mut self, vertices: usize) {
        if self.generation >= u32::MAX >> 1 {
            self.generation = 0;
            self.clampable.clear();
        }
        self.generation += 1;
        if self.clampable.len() != vertices {
            self.clampable.clear();
            self.clampable.resize(vertices, 0);
        }
    }

    /// The channel of the last route returned by [`find_route`] or [`find_route_in_lane`]: the
    /// portals it crosses, as `(left, right)` vertex pairs, start first; empty for a straight
    /// route.
    pub(crate) fn channel(&self) -> &[(u32, u32)] {
        &self.portals
    }

    /// The point a node's lane word ends at: the start, or the midpoint of its entry edge.
    fn node_point(&self, query: &Query<'_>, id: u32) -> SimVec2 {
        let node = self.nodes[id as usize];
        if node.parent == NONE {
            return query.start;
        }
        let entry = usize::from(node.entry);
        (query.mesh.corner(node.triangle, next(entry)).to_sim()
            + query.mesh.corner(node.triangle, prev(entry)).to_sim())
            / 2
    }

    /// `word` followed by the crossings of the leg `from -> to`, interned.
    fn extend_word(&mut self, cuts: &LaneCuts, word: u32, from: SimVec2, to: SimVec2) -> u32 {
        let lanes = &mut self.lanes;
        let mut word = word;
        cuts.for_each_crossing(from, to, |letter| word = lanes.push(word, letter));
        word
    }

    /// The word of a child entered through the edge `[a, b]` of node `id`, or `None` when it
    /// strays more than `K` letters from every prefix of the target (D-5's pruning).
    fn child_word(&mut self, query: &Query<'_>, id: u32, a: SimVec2, b: SimVec2) -> Option<u32> {
        let Some(lane) = query.lane else {
            return Some(WORD_ROOT);
        };
        let from = self.node_point(query, id);
        let word = self.extend_word(lane.cuts, self.nodes[id as usize].word, from, (a + b) / 2);
        (self.lanes.excess(word) <= lane.bound).then_some(word)
    }

    fn push(&mut self, node: Node) {
        let id = u32::try_from(self.nodes.len()).unwrap_or(NONE);
        let f = node.g.saturating_add(node.h).raw();
        self.nodes.push(node);
        self.open.push(Reverse((f, node.triangle, id)));
    }

    fn on_channel(&self, mut id: u32, triangle: u32) -> bool {
        loop {
            let node = self.nodes[id as usize];
            if node.triangle == triangle {
                return true;
            }
            if node.parent == NONE {
                return false;
            }
            id = node.parent;
        }
    }

    /// Fills `self.portals` with the channel of node `id`, start first.
    fn collect_portals(&mut self, mesh: &NavMesh, mut id: u32) {
        self.portals.clear();
        while self.nodes[id as usize].parent != NONE {
            let node = self.nodes[id as usize];
            let parent = self.nodes[node.parent as usize].triangle;
            let k = usize::from(node.via);
            let tv = mesh.triangle_vertices(parent);
            self.portals.push((tv[prev(k)], tv[next(k)]));
            id = node.parent;
        }
        self.portals.reverse();
    }

    /// Expands node `id` into its children.
    fn expand(&mut self, query: &Query<'_>, id: u32) {
        let node = self.nodes[id as usize];
        let mesh = query.mesh;
        for k in 0..3 {
            if usize::from(node.entry) == k
                || mesh.edge_blocks(node.triangle, k, |t| query.access.open(t))
            {
                continue;
            }
            let child = mesh.triangle_neighbors(node.triangle)[k];
            if self.on_channel(id, child) {
                continue;
            }
            let tv = mesh.triangle_vertices(node.triangle);
            let (a, b) = (
                mesh.vertex(tv[next(k)]).to_sim(),
                mesh.vertex(tv[prev(k)]).to_sim(),
            );
            if node.entry != NO_EDGE {
                let corner = 3 - usize::from(node.entry) - k;
                let entry = usize::from(node.entry);
                // D-3: a disc wedged where it starts enters every triangle it already overlaps
                // whatever its width, so it can leave the gap it was spawned or pushed into.
                let wedged_in = !query.start_fits
                    && point_segment_distance(
                        query.start,
                        mesh.vertex(tv[next(entry)]).to_sim(),
                        mesh.vertex(tv[prev(entry)]).to_sim(),
                    ) < query.radius;
                if !wedged_in
                    && !query
                        .access
                        .width(node.triangle, corner)
                        .admits_diameter(query.radius)
                {
                    continue;
                }
            } else if query.end_checks
                && query.start_fits
                && !self.reaches_edge(query, query.start, a, b)
            {
                continue;
            }
            if query.end_checks
                && child == query.goal_triangle
                && !self.reaches_edge(query, query.goal, a, b)
            {
                continue;
            }
            let Some(word) = self.child_word(query, id, a, b) else {
                continue;
            };
            let h = point_segment_distance(query.goal, a, b);
            let g = self.lower_bound(query, id, a, b);
            let funnel = self.child_funnel(query, id, (tv[prev(k)], tv[next(k)]));
            let entry = mesh
                .triangle_vertices(child)
                .iter()
                .position(|&v| v == tv[next(k)])
                .map_or(0, next);
            self.push(Node {
                triangle: child,
                entry: u8::try_from(entry).unwrap_or(NO_EDGE),
                via: u8::try_from(k).unwrap_or(NO_EDGE),
                parent: id,
                g,
                h,
                word,
                depth: node.depth + 1,
                funnel,
            });
        }
    }

    /// Whether mesh vertex `v` is clampable for `query`: a free vertex (no edge at it blocks the
    /// request) with an obstacle vertex strictly within `rho_max = r + CORNER_SAFETY_MARGIN`.
    /// The radius funnel may clamp such a portal end outwards along its portal (plan
    /// `2026-11-19-partial-knowledge-funnel` D-2), so a pull may pass beyond it, and the point
    /// funnel gives it no side. `rho_max` is the largest radius any pull of the route uses (the
    /// fitted retry uses less, the zero-room retry `r + ZERO_ROOM_SLACK`).
    fn clampable(&mut self, query: &Query<'_>, v: u32) -> bool {
        let slot = v as usize;
        let asked = self.clampable[slot];
        if asked >> 1 == self.generation {
            return asked & 1 == 1;
        }
        let mesh = query.mesh;
        let answer = !mesh.vertex_blocks(v, |t| query.access.open(t)) && {
            let f = mesh.vertex(v).to_sim();
            let rho = query.radius + CORNER_SAFETY_MARGIN;
            obstacle_vertices_near(
                mesh,
                query.access,
                &mut self.segment,
                (f, f),
                rho,
                &mut self.near,
            );
            let rho_sq = rho * rho;
            self.near
                .iter()
                .any(|&u| mesh.vertex(u).to_sim().distance_sq(f) < rho_sq)
        };
        self.clampable[slot] = (self.generation << 1) | u32::from(answer);
        answer
    }

    /// The portal `(left, right)` of mesh vertices, with whether each end bounds the funnel.
    fn bound_portal(&mut self, query: &Query<'_>, (left, right): (u32, u32)) -> BoundPortal {
        BoundPortal {
            left: query.mesh.vertex(left).to_sim(),
            right: query.mesh.vertex(right).to_sim(),
            left_bounds: !self.clampable(query, left),
            right_bounds: !self.clampable(query, right),
        }
    }

    /// The point funnel of a child of node `id` entered through the portal `(left, right)`: the
    /// node's funnel advanced by that portal. When the apex moves to a side set before it, the
    /// portals after the new apex are re-read from the node's ancestors and the funnel advanced
    /// through them.
    fn child_funnel(&mut self, query: &Query<'_>, id: u32, portal: (u32, u32)) -> PointFunnel {
        let node = self.nodes[id as usize];
        let depth = node.depth + 1;
        let mut funnel = node.funnel;
        let entry = self.bound_portal(query, portal);
        if funnel.advance(&[entry], depth) {
            return funnel;
        }
        let mut portals = std::mem::take(&mut self.bound_portals);
        loop {
            portals.clear();
            portals.push(entry);
            let mut current = id;
            loop {
                let ancestor = self.nodes[current as usize];
                if ancestor.parent == NONE || ancestor.depth <= funnel.apex_depth {
                    break;
                }
                let parent = self.nodes[ancestor.parent as usize];
                let tv = query.mesh.triangle_vertices(parent.triangle);
                let k = usize::from(ancestor.via);
                portals.push(self.bound_portal(query, (tv[prev(k)], tv[next(k)])));
                current = ancestor.parent;
            }
            portals.reverse();
            if funnel.advance(&portals, funnel.apex_depth + 1) {
                break;
            }
        }
        self.bound_portals = portals;
        funnel
    }

    /// An admissible lower bound on the length of any route of this channel from the start to
    /// the edge `[a, b]` leaving node `id`, the largest of three: the straight distance from the
    /// start; for each of up to [`NAV_TA_STAR_BOUND_ANCESTORS`] ancestors, its bound at its entry
    /// edge `e` plus the distance from `e` to `[a, b]`; and the point funnel's path through the
    /// node's channel to its apex plus the distance from the apex to `[a, b]` (D-1 of plan
    /// `2026-11-20-tastar-budget`: every pull crosses the same portals, inside every end that
    /// bounds, so it is no shorter).
    fn lower_bound(&self, query: &Query<'_>, id: u32, a: SimVec2, b: SimVec2) -> SimScalar {
        let mut bound = point_segment_distance(query.start, a, b)
            .max(self.nodes[id as usize].funnel.bound_to(a, b));
        let mut current = id;
        for _ in 0..NAV_TA_STAR_BOUND_ANCESTORS {
            let node = self.nodes[current as usize];
            if node.parent == NONE {
                break;
            }
            let parent = self.nodes[node.parent as usize];
            let tv = query.mesh.triangle_vertices(parent.triangle);
            let k = usize::from(node.via);
            let (c, d) = (
                query.mesh.vertex(tv[next(k)]).to_sim(),
                query.mesh.vertex(tv[prev(k)]).to_sim(),
            );
            bound = bound.max(node.g + segment_distance(c, d, a, b));
            current = node.parent;
        }
        bound
    }

    /// Whether a disc at `point` reaches the edge `[a, b]` of its triangle: a straight sweep
    /// to one of five points of the edge that keep the disc's radius from both ends is clear
    /// (the nearest such point, the two ends of that range, its middle and its quarter points).
    ///
    /// Demyen's width test covers a triangle crossed from edge to edge; the start and goal
    /// triangles are crossed from or to a point instead, so they get this direct check. A start
    /// where the disc does not fit skips it (D-3: a wedged unit may always leave).
    fn reaches_edge(&mut self, query: &Query<'_>, point: SimVec2, a: SimVec2, b: SimVec2) -> bool {
        let d = b - a;
        let length = d.length();
        let reach = query.radius + SimScalar::from_raw(1 << 16);
        if length <= reach * 2 {
            let mid = (a + b) / 2;
            return segment_clear(
                query.mesh,
                query.access,
                &mut self.segment,
                point,
                mid,
                query.radius,
            );
        }
        let (low, high) = (reach, length - reach);
        let nearest = ((point - a).dot(d) / length).clamp(low, high);
        let span = high - low;
        [
            nearest,
            low,
            high,
            low + span / 2,
            low + span / 4,
            high - span / 4,
        ]
        .into_iter()
        .any(|along| {
            let target = a + d * (along / length);
            segment_clear(
                query.mesh,
                query.access,
                &mut self.segment,
                point,
                target,
                query.radius,
            )
        })
    }

    /// Whether every leg of `route` passes the segment predicate. Legs from a wedged start up to
    /// the first waypoint where the disc fits cannot; they are held to the predicate for the
    /// unit's centre only (D-3: a wedged unit may always work its way out; the movement layer
    /// holds them to the terrain).
    fn is_legal(&mut self, query: &Query<'_>, route: &FunnelRoute) -> bool {
        let mut wedged = !query.start_fits;
        for pair in route.waypoints.windows(2) {
            let clear = |scratch: &mut SegmentScratch, a, b, radius| {
                segment_clear(query.mesh, query.access, scratch, a, b, radius)
            };
            if wedged {
                wedged = !clear(&mut self.segment, pair[0], pair[0], query.radius);
            }
            // A wedged leg must still keep the unit's centre in open triangles.
            let radius = if wedged {
                SimScalar::ZERO
            } else {
                query.radius
            };
            if !clear(&mut self.segment, pair[0], pair[1], radius) {
                return false;
            }
        }
        true
    }

    /// Pulls the channel in `self.portals` again with [`PullStrategy::Pinned`] after the shrunk
    /// route `shrunk` failed the segment predicate; returns the pinned route when it is legal,
    /// else `shrunk`, and whether the returned route is legal.
    fn pinned_retry(
        &mut self,
        query: &Query<'_>,
        start_triangle: u32,
        shrunk: FunnelRoute,
    ) -> (FunnelRoute, bool) {
        let (s, g) = (
            query.mesh.triangle_vertices(start_triangle),
            query.mesh.triangle_vertices(query.goal_triangle),
        );
        let pinned = radius_funnel(
            query.mesh,
            (query.start, query.goal),
            &self.portals,
            &[s[0], s[1], s[2], g[0], g[1], g[2]],
            (
                query.radius + CORNER_SAFETY_MARGIN,
                Emission::Inscribed { dip: ARC_CHORD_DIP },
                PullStrategy::Pinned,
            ),
            &mut Obstacles {
                access: query.access,
                segment: &mut self.segment,
                scratch: &mut self.funnel,
            },
        );
        if self.is_legal(query, &pinned) {
            (pinned, true)
        } else {
            (shrunk, false)
        }
    }

    /// Pulls the channel in `self.portals` once more, shrunk then pinned, with the corner margin
    /// fitted to the channel (plan `2026-11-18-contour-footprints` Amendment 1) after the shrunk
    /// and pinned pulls at the full margin both failed the segment predicate. Returns the first
    /// legal route, else `failed`, and whether the returned route is legal. A channel with room
    /// for the full margin, or none to spare, is not pulled again.
    fn fitted_retry(
        &mut self,
        query: &Query<'_>,
        start_triangle: u32,
        failed: FunnelRoute,
    ) -> (FunnelRoute, bool) {
        let width = channel_width(query, &self.portals);
        let Some(margin) = fitted_margin(width, query.radius) else {
            return (failed, false);
        };
        let (s, g) = (
            query.mesh.triangle_vertices(start_triangle),
            query.mesh.triangle_vertices(query.goal_triangle),
        );
        for strategy in [PullStrategy::Shrunk, PullStrategy::Pinned] {
            let route = radius_funnel(
                query.mesh,
                (query.start, query.goal),
                &self.portals,
                &[s[0], s[1], s[2], g[0], g[1], g[2]],
                (
                    query.radius + margin,
                    Emission::Inscribed { dip: margin / 2 },
                    strategy,
                ),
                &mut Obstacles {
                    access: query.access,
                    segment: &mut self.segment,
                    scratch: &mut self.funnel,
                },
            );
            if self.is_legal(query, &route) {
                return (route, true);
            }
        }
        (failed, false)
    }

    /// Pulls the channel in `self.portals` a last time, shrunk then pinned, with no room to spare
    /// (plan `2026-11-19-funnel-fallbacks` D-2), after every pull with a margin failed the
    /// segment predicate: at `r + ZERO_ROOM_SLACK`, with arcs circumscribed and gate lines across
    /// pinches, and no candidate rounded that comes nearest a leg at the start or the goal.
    /// Returns the first legal route, else `failed`, and whether the returned route is legal.
    fn zero_room_retry(
        &mut self,
        query: &Query<'_>,
        start_triangle: u32,
        failed: FunnelRoute,
    ) -> (FunnelRoute, bool) {
        let (s, g) = (
            query.mesh.triangle_vertices(start_triangle),
            query.mesh.triangle_vertices(query.goal_triangle),
        );
        for strategy in [PullStrategy::Shrunk, PullStrategy::Pinned] {
            let route = radius_funnel(
                query.mesh,
                (query.start, query.goal),
                &self.portals,
                &[s[0], s[1], s[2], g[0], g[1], g[2]],
                (
                    query.radius + ZERO_ROOM_SLACK,
                    Emission::ZeroRoom {
                        radius: query.radius,
                    },
                    strategy,
                ),
                &mut Obstacles {
                    access: query.access,
                    segment: &mut self.segment,
                    scratch: &mut self.funnel,
                },
            );
            if self.is_legal(query, &route) {
                return (route, true);
            }
        }
        (failed, false)
    }

    /// TA* without, then if its route is illegal with, the start and goal reach checks.
    /// Returns the route, whether it is legal, and the expansions spent; `self.portals` holds
    /// the returned route's channel.
    fn two_pass(&mut self, query: &Query<'_>, start: u32) -> (Option<FunnelRoute>, bool, u32) {
        let (first, spent) = self.ta_star(query, start);
        let Some(first) = first else {
            return (None, false, spent);
        };
        if self.is_legal(query, &first) {
            return (Some(first), true, spent);
        }
        let first_portals = self.portals.clone();
        let checked = Query {
            end_checks: true,
            ..*query
        };
        let (second, more) = self.ta_star(&checked, start);
        if let Some(second) = second {
            if self.is_legal(query, &second) {
                return (Some(second), true, spent + more);
            }
        }
        self.portals = first_portals;
        (Some(first), false, spent + more)
    }

    /// TA*: returns the best funnel route found and its channel in `self.portals`.
    fn ta_star(&mut self, query: &Query<'_>, start_triangle: u32) -> (Option<FunnelRoute>, u32) {
        let rho = query.radius + CORNER_SAFETY_MARGIN;
        let (s, g) = (
            query.mesh.triangle_vertices(start_triangle),
            query.mesh.triangle_vertices(query.goal_triangle),
        );
        let end_vertices = [s[0], s[1], s[2], g[0], g[1], g[2]];
        self.reset(query.mesh.triangle_count(), query.lane.is_some());
        self.begin_clampable(query.mesh.vertex_count());
        self.push(Node {
            triangle: start_triangle,
            entry: NO_EDGE,
            via: NO_EDGE,
            parent: NONE,
            g: SimScalar::ZERO,
            h: query.start.distance(query.goal),
            word: WORD_ROOT,
            depth: 0,
            funnel: PointFunnel::at(query.start),
        });
        let mut best: Option<(FunnelRoute, Vec<(u32, u32)>)> = None;
        let (mut expansions, mut extra) = (0u32, 0u32);
        while let Some(Reverse((f, _, id))) = self.open.pop() {
            if let Some((route, _)) = &best {
                if f >= route.length.raw() || extra >= NAV_TA_STAR_EXTRA_EXPANSIONS {
                    break;
                }
                extra += 1;
            }
            let node = self.nodes[id as usize];
            if node.triangle == query.goal_triangle && node.parent != NONE {
                // D-5: a restricted search accepts a channel only when its word is the target.
                if let Some(lane) = query.lane {
                    let from = self.node_point(query, id);
                    if self.extend_word(lane.cuts, node.word, from, query.goal) != lane.target {
                        continue;
                    }
                }
                self.collect_portals(query.mesh, id);
                let route = radius_funnel(
                    query.mesh,
                    (query.start, query.goal),
                    &self.portals,
                    &end_vertices,
                    (
                        rho,
                        Emission::Inscribed { dip: ARC_CHORD_DIP },
                        PullStrategy::Shrunk,
                    ),
                    &mut Obstacles {
                        access: query.access,
                        segment: &mut self.segment,
                        scratch: &mut self.funnel,
                    },
                );
                if best.as_ref().is_none_or(|(b, _)| route.length < b.length) {
                    best = Some((route, self.portals.clone()));
                }
                continue;
            }
            // Counted per (triangle, entry edge) state: the first arrival in a state is always
            // expanded, so the search is complete; later arrivals explore other channels.
            // A restricted search counts per (state, word) instead (D-5), so each word's first
            // arrival in a state is expanded.
            let state = 3 * node.triangle as usize + usize::from(node.entry.min(2));
            if query.lane.is_some() {
                if !self
                    .lanes
                    .visit(state, node.word, NAV_TA_STAR_TRIANGLE_VISITS)
                {
                    continue;
                }
            } else {
                let visits = &mut self.visits[state];
                if *visits >= NAV_TA_STAR_TRIANGLE_VISITS {
                    continue;
                }
                *visits += 1;
            }
            expansions += 1;
            self.expand(query, id);
        }
        self.open.clear();
        match best {
            Some((route, portals)) => {
                self.portals = portals;
                (Some(route), expansions)
            }
            None => (None, expansions),
        }
    }
}

/// Validates the request's endpoints on the mesh; returns the start and goal triangles.
fn endpoints(
    mesh: &NavMesh,
    access: &Access<'_>,
    scratch: &mut SegmentScratch,
    request: &PathRequestWithVision<'_>,
) -> Result<(u32, u32)> {
    let start = mesh
        .locate(request.start)
        .filter(|&t| access.open(t))
        .ok_or(MapError::InvalidStart(request.start))?;
    let goal = mesh
        .locate(request.goal)
        .filter(|&t| access.open(t))
        .ok_or(MapError::InvalidGoal(request.goal))?;
    if !segment_clear(
        mesh,
        access,
        scratch,
        request.goal,
        request.goal,
        request.radius,
    ) {
        return Err(MapError::InvalidGoal(request.goal));
    }
    Ok((start, goal))
}

/// Finds a route on the mesh for a disc of `request.radius` (D-4 to D-7).
///
/// # Errors
/// [`MapError::InvalidStart`] when the start is off the map or in a triangle the request may not
/// enter; [`MapError::InvalidGoal`] likewise, or when a disc does not fit at the goal;
/// [`MapError::NoPathFound`] when no channel admits the disc.
pub fn find_route(
    mesh: &NavMesh,
    search: &mut MeshSearch,
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
) -> Result<MeshRoute> {
    route(mesh, search, map, request, None)
}

/// A lane restriction: the terrain's cuts, `K` of the current mesh, and the target word.
#[derive(Debug, Clone, Copy)]
pub struct LaneRestriction<'a> {
    /// The terrain's cuts (D-2).
    pub cuts: &'a LaneCuts,
    /// `K`: the largest number of cuts any edge of the mesh crosses
    /// ([`LaneCuts::max_edge_crossings`]).
    pub bound: u32,
    /// The lane word the route must have.
    pub word: &'a LaneWord,
}

/// Finds the route of [`find_route`] restricted to one lane (D-5 of plan
/// `2026-11-10-group-routing`): TA* whose nodes carry the lane word of their portal-midpoint
/// polyline, which accepts a channel only when its word is `lane.word`. Nodes whose word strays
/// more than `lane.bound` letters from every prefix of the target are pruned, and the visit cap
/// counts per (triangle, entry edge, word). Everything else is [`find_route`]'s: the anytime
/// budget, both passes, the pinned retry, the legality check and the fallback.
///
/// # Errors
/// As [`find_route`]; [`MapError::NoPathFound`] also when no channel of that lane admits the
/// disc (the lane is too narrow, or the word names no route between the endpoints).
pub fn find_route_in_lane(
    mesh: &NavMesh,
    search: &mut MeshSearch,
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
    lane: &LaneRestriction<'_>,
) -> Result<MeshRoute> {
    route(mesh, search, map, request, Some(lane))
}

/// [`find_route`] and [`find_route_in_lane`].
fn route(
    mesh: &NavMesh,
    search: &mut MeshSearch,
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
    restriction: Option<&LaneRestriction<'_>>,
) -> Result<MeshRoute> {
    let access = Access::new(mesh, map, request);
    let (start_triangle, goal_triangle) = endpoints(mesh, &access, &mut search.segment, request)?;
    let (start, goal) = (request.start, request.goal);
    let straight = |expansions| MeshRoute {
        waypoints: vec![start, goal],
        total_length: start.distance(goal),
        turns: Vec::new(),
        fallback: false,
        zero_room: false,
        expansions,
    };
    let lane = restriction.map(|r| LaneQuery {
        cuts: r.cuts,
        target: search.lanes.begin(r.word),
        bound: r.bound,
    });
    let straight_in_lane = restriction.is_none_or(|r| r.cuts.word(&[start, goal]) == *r.word);
    if straight_in_lane
        && (start_triangle == goal_triangle
            || segment_clear(
                mesh,
                &access,
                &mut search.segment,
                start,
                goal,
                request.radius,
            ))
    {
        search.portals.clear();
        return Ok(straight(0));
    }
    let query = Query {
        mesh,
        access: &access,
        start,
        goal,
        goal_triangle,
        radius: request.radius,
        start_fits: segment_clear(
            mesh,
            &access,
            &mut search.segment,
            start,
            start,
            request.radius,
        ),
        end_checks: false,
        lane,
    };
    let (route, legal, expansions) = search.two_pass(&query, start_triangle);
    let route = route.ok_or(MapError::NoPathFound { start, goal })?;
    let (route, legal) = if legal {
        (route, true)
    } else {
        search.pinned_retry(&query, start_triangle, route)
    };
    let (route, legal) = if legal {
        (route, true)
    } else {
        search.fitted_retry(&query, start_triangle, route)
    };
    let (route, legal, zero_room) = if legal {
        (route, true, false)
    } else {
        let (route, legal) = search.zero_room_retry(&query, start_triangle, route);
        (route, legal, legal)
    };
    let turns = route
        .anchors
        .iter()
        .filter_map(|anchor| match *anchor {
            Anchor::Vertex(v, side) => Some((mesh.vertex(v), side)),
            Anchor::Point(_) | Anchor::Pass(_) => None,
        })
        .collect();
    let (waypoints, fallback) = if legal {
        (route.waypoints, false)
    } else {
        search.fallbacks += 1;
        (portal_midpoints(mesh, start, goal, &search.portals), true)
    };
    let mut total_length = SimScalar::ZERO;
    for pair in waypoints.windows(2) {
        total_length += pair[0].distance(pair[1]);
    }
    Ok(MeshRoute {
        waypoints,
        total_length,
        turns,
        fallback,
        zero_room,
        expansions,
    })
}

/// The narrowest width of the channel `portals`, in cells squared: the least distance between
/// an obstacle on its left and one on its right. Left obstacles are the portals' left vertices
/// that block the request (a free vertex bounds nothing, plan `2026-11-19-partial-knowledge-funnel`
/// D-5) and the blocking edges of channel triangles on the left; right obstacles likewise. The
/// triangle corner widths TA* tests (Demyen and Buro, D-4) can miss a pair that lies in two
/// different triangles, such as a structure's face across from a long wall edge (plan
/// `2026-11-18-contour-footprints` Amendment 1), so the retry measures the pairs themselves. The
/// result can only be smaller than the room the channel has, which makes the fitted margin
/// smaller, never larger. [`SquaredWidth::UNBOUNDED`] for a channel of at most one portal.
// PERFORMANCE RATIONALE:
// Why: The width is needed only after two funnel pulls have failed, which is rare.
// What: Compare every left obstacle with every right one, and find each channel triangle by a
// scan of the mesh, instead of keeping the search's triangle sequence.
// Trade-off: Quadratic in the channel's length and linear in the mesh on that rare path, for no
// extra state in the search.
fn channel_width(query: &Query<'_>, portals: &[(u32, u32)]) -> SquaredWidth {
    let mesh = query.mesh;
    if portals.len() < 2 {
        return SquaredWidth::UNBOUNDED;
    }
    let mut left_edges = Vec::new();
    let mut right_edges = Vec::new();
    for pair in portals.windows(2) {
        let ((l1, r1), (l2, r2)) = (pair[0], pair[1]);
        let shared_left = l1 == l2;
        let shared = if shared_left { l1 } else { r1 };
        let a = if shared_left { r1 } else { l1 };
        let b = if shared_left { r2 } else { l2 };
        let triangle = (0..mesh.triangle_count())
            .map(|t| u32::try_from(t).unwrap_or(NONE))
            .find(|&t| {
                let v = mesh.triangle_vertices(t);
                [shared, a, b].iter().all(|x| v.contains(x))
            });
        let Some(t) = triangle else {
            continue;
        };
        let corner = mesh
            .triangle_vertices(t)
            .iter()
            .position(|&v| v == shared)
            .unwrap_or(0);
        if mesh.edge_blocks(t, corner, |u| query.access.open(u)) {
            // The edge opposite the shared vertex lies on the channel's other side.
            let edge = (mesh.vertex(a), mesh.vertex(b));
            if shared_left {
                right_edges.push(edge);
            } else {
                left_edges.push(edge);
            }
        }
    }
    let obstacle = |v: u32| mesh.vertex_blocks(v, |u| query.access.open(u));
    let left: Vec<LatticePoint> = portals
        .iter()
        .filter(|&&(l, _)| obstacle(l))
        .map(|&(l, _)| mesh.vertex(l))
        .collect();
    let right: Vec<LatticePoint> = portals
        .iter()
        .filter(|&&(_, r)| obstacle(r))
        .map(|&(_, r)| mesh.vertex(r))
        .collect();
    let mut width = SquaredWidth::UNBOUNDED;
    for &l in &left {
        for &r in &right {
            width = width.min(lattice_distance_sq(l, r));
        }
        for &(u, v) in &right_edges {
            width = width.min(lattice_point_segment_width(l, u, v));
        }
    }
    for &r in &right {
        for &(u, v) in &left_edges {
            width = width.min(lattice_point_segment_width(r, u, v));
        }
    }
    width
}

/// The corner margin a channel of squared width `width` has room for, around a disc of
/// `radius`: `min(CORNER_SAFETY_MARGIN, (w - 2 radius) / 2)`, with `w` the width's square root
/// rounded down to the raw grid, so the margin can only be smaller than the room. `None` when the
/// full margin fits (the earlier pulls already used it) or no room is left.
fn fitted_margin(width: SquaredWidth, radius: SimScalar) -> Option<SimScalar> {
    if width.is_unbounded() {
        return None;
    }
    let (num, den) = width.parts();
    // `num < 2^62`, so `num * 2^64 < 2^126`: the width in raw units squared, floored.
    let squared_raw = (u128::from(num) << 64) / u128::from(den.max(1));
    let width_raw = i64::try_from(squared_raw.isqrt()).unwrap_or(i64::MAX);
    let spare = SimScalar::from_raw(width_raw) - radius * 2;
    let margin = spare / 2;
    (margin > SimScalar::ZERO && margin < CORNER_SAFETY_MARGIN).then_some(margin)
}

/// Whether a disc of `request.radius` may sweep from `a` to `b` (D-7).
pub fn segment_is_passable(
    mesh: &NavMesh,
    search: &mut MeshSearch,
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
    a: SimVec2,
    b: SimVec2,
) -> bool {
    let access = Access::new(mesh, map, request);
    segment_clear(mesh, &access, &mut search.segment, a, b, request.radius)
}

/// Where a disc of `request.radius` at `p` touches the edges that block `request`, or `None`
/// (plan `2026-11-11-contact-and-missiles` D-5; see [`nearest_blocking_point`]).
pub fn wall_contact(
    mesh: &NavMesh,
    search: &mut MeshSearch,
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
    p: SimVec2,
) -> Option<WallContact> {
    let access = Access::new(mesh, map, request);
    // PERFORMANCE RATIONALE:
    // Why: Every moving ground body asks for its wall contact once or twice a tick; in the
    //      400-unit canyon scenario that is up to 800 floods per tick, most in open ground.
    // What: An exact early-out: if every cell the disc's bounding square touches is open with no
    //      blocking side (one look-up per cell of a per-cell table built with the mesh), no wall
    //      is within the radius and the flood is skipped. Only for all-known requests.
    // Trade-off: A second code path that must agree with the flood; a property test compares it
    //      with the exhaustive reference on both shipped maps, with and without structures.
    if access.is_all_known() && mesh.contact_free_near(map, p, request.radius) {
        return None;
    }
    nearest_blocking_point(mesh, &access, &mut search.segment, p, request.radius)
}

/// [`wall_contact`] from every blocking edge of the mesh, with no flood: its reference.
#[must_use]
pub fn wall_contact_exhaustive(
    mesh: &NavMesh,
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
    p: SimVec2,
) -> Option<WallContact> {
    let access = Access::new(mesh, map, request);
    nearest_blocking_point_exhaustive(mesh, &access, p, request.radius)
}

/// Marks every triangle a disc of `radius` reaches from `start` (width test included) in
/// `search.visits` (1 = reached).
fn flood(
    search: &mut MeshSearch,
    mesh: &NavMesh,
    access: &Access<'_>,
    start: u32,
    radius: SimScalar,
) {
    // A triangle may be reached through each of its three edges; each (triangle, entry) state is
    // expanded once. Bit `entry` of `visits` records the state, bit 3 the start.
    search.visits.clear();
    search.visits.resize(mesh.triangle_count(), 0);
    let mut stack = std::mem::take(&mut search.flood_stack);
    stack.clear();
    stack.push((start, NO_EDGE));
    search.visits[start as usize] = 1 << NO_EDGE;
    while let Some((t, entry)) = stack.pop() {
        for k in 0..3 {
            if usize::from(entry) == k || mesh.edge_blocks(t, k, |x| access.open(x)) {
                continue;
            }
            if entry != NO_EDGE
                && !access
                    .width(t, 3 - usize::from(entry) - k)
                    .admits_diameter(radius)
            {
                continue;
            }
            let child = mesh.triangle_neighbors(t)[k];
            let q = mesh.triangle_vertices(t)[next(k)];
            let child_entry = mesh
                .triangle_vertices(child)
                .iter()
                .position(|&v| v == q)
                .map_or(0, next);
            let bit = 1u8 << child_entry;
            if search.visits[child as usize] & bit == 0 {
                search.visits[child as usize] |= bit;
                stack.push((child, u8::try_from(child_entry).unwrap_or(NO_EDGE)));
            }
        }
    }
    search.flood_stack = stack;
}

/// The point nearest `request.goal` that the unit can reach, on the mesh: the centre of the
/// reachable cell nearest the goal cell (ties to the lowest cell index) where the disc fits, so
/// every unit of one region gets the same point. `None` when that is the unit's own cell.
///
/// # Errors
/// [`MapError::InvalidStart`] if the start is off the map or in a closed triangle;
/// [`MapError::InvalidGoal`] if the goal is off the map.
pub fn nearest_reachable_point(
    mesh: &NavMesh,
    search: &mut MeshSearch,
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
) -> Result<Option<SimVec2>> {
    let access = Access::new(mesh, map, request);
    let start = mesh
        .locate(request.start)
        .filter(|&t| access.open(t))
        .ok_or(MapError::InvalidStart(request.start))?;
    let start_cell = map
        .world_to_cell(request.start)
        .map_err(|_| MapError::InvalidStart(request.start))?;
    let goal = map
        .world_to_cell(request.goal)
        .map_err(|_| MapError::InvalidGoal(request.goal))?;
    flood(search, mesh, &access, start, request.radius);
    let mut candidates = std::mem::take(&mut search.candidates);
    candidates.clear();
    for y in 0..map.height() {
        for x in 0..map.width() {
            let centre = map.cell_to_world_center(x, y)?;
            if mesh
                .locate(centre)
                .is_some_and(|t| search.visits[t as usize] != 0)
            {
                let (dx, dy) = (u64::from(x.abs_diff(goal.0)), u64::from(y.abs_diff(goal.1)));
                candidates.push((dx * dx + dy * dy, y * map.width() + x, x));
            }
        }
    }
    candidates.sort_unstable();
    let mut found = None;
    for &(_, index, x) in &candidates {
        let y = index / map.width();
        let centre = map.cell_to_world_center(x, y)?;
        if segment_clear(
            mesh,
            &access,
            &mut search.segment,
            centre,
            centre,
            request.radius,
        ) {
            found = ((x, y) != start_cell).then_some(centre);
            break;
        }
    }
    search.candidates = candidates;
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_format::MapFile;
    use crate::StructureFootprint;
    use sim_types::{EntityId, FootprintShape};

    /// A point of the plan's hand-checked channels, in world units.
    fn at(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    /// Portals `(left x, left y, right x, right y)` whose ends all bound, except `skipped`.
    fn portals_of(ends: &[(f64, f64, f64, f64)], skipped: Option<SimVec2>) -> Vec<BoundPortal> {
        ends.iter()
            .map(|&(lx, ly, rx, ry)| {
                let (left, right) = (at(lx, ly), at(rx, ry));
                BoundPortal {
                    left,
                    right,
                    left_bounds: Some(left) != skipped,
                    right_bounds: Some(right) != skipped,
                }
            })
            .collect()
    }

    /// The funnel after each of `portals` as the search keeps it: advanced by one portal per
    /// node, and through every portal after the apex again when the apex moves before it.
    fn funnels_by_node(start: SimVec2, portals: &[BoundPortal]) -> Vec<PointFunnel> {
        let mut funnel = PointFunnel::at(start);
        let mut out = vec![funnel];
        for depth in 1..=portals.len() {
            let first = depth - 1;
            if !funnel.advance(
                &portals[first..depth],
                u32::try_from(depth).unwrap_or(u32::MAX),
            ) {
                let from = funnel.apex_depth as usize;
                assert!(funnel.advance(&portals[from..depth], funnel.apex_depth + 1));
            }
            out.push(funnel);
        }
        out
    }

    /// The largest `g + h` the channel bound gives along `portals` from `start` to `goal`: at
    /// each node, the funnel's bound to the next portal (or to the goal) plus that edge's
    /// distance to the goal.
    fn largest_f(start: SimVec2, goal: SimVec2, portals: &[BoundPortal]) -> SimScalar {
        let funnels = funnels_by_node(start, portals);
        funnels
            .iter()
            .enumerate()
            .map(|(depth, funnel)| {
                let (a, b) = portals
                    .get(depth)
                    .map_or((goal, goal), |portal| (portal.right, portal.left));
                funnel.bound_to(a, b) + point_segment_distance(goal, a, b)
            })
            .max()
            .unwrap_or(SimScalar::ZERO)
    }

    /// T4 of plan `2026-11-20-tastar-budget` (whole squares layout 23 q 0, r 0.75, a route of
    /// 42.975): the start (42.5, 55.5) lies on the line of the first portal's ends. With
    /// Mononen's strict crossing tests the funnel collapses onto the portal's far end and the
    /// bound reaches 48.9, so the search would stop on a longer channel (46.128 was found); the
    /// inclusive tests keep every node's `g + h` at most the route's length.
    #[test]
    fn a_start_on_the_first_portal_line_does_not_inflate_the_bound() {
        let ends = [
            (45.0, 58.0, 36.0, 49.0),
            (45.0, 57.0, 36.0, 49.0),
            (45.0, 57.0, 37.0, 49.0),
            (45.0, 56.0, 37.0, 49.0),
            (45.0, 56.0, 38.0, 49.0),
            (45.0, 55.0, 38.0, 49.0),
            (45.0, 54.0, 38.0, 49.0),
            (45.0, 53.0, 38.0, 49.0),
            (45.0, 53.0, 38.0, 48.0),
            (47.0, 44.0, 38.0, 48.0),
            (47.0, 44.0, 38.0, 47.0),
            (47.0, 44.0, 38.0, 46.0),
            (47.0, 44.0, 38.0, 45.0),
            (47.0, 44.0, 38.0, 44.0),
            (47.0, 43.0, 38.0, 44.0),
            (48.0, 41.0, 38.0, 44.0),
            (34.0, 34.0, 38.0, 44.0),
            (34.0, 34.0, 37.0, 44.0),
            (34.0, 34.0, 36.0, 44.0),
            (34.0, 34.0, 35.0, 44.0),
            (34.0, 34.0, 34.0, 44.0),
            (34.0, 34.0, 33.0, 44.0),
            (34.0, 34.0, 30.0, 34.0),
            (34.0, 30.0, 30.0, 34.0),
            (34.0, 30.0, 30.0, 30.0),
            (40.0, 17.0, 30.0, 30.0),
            (40.0, 16.0, 30.0, 30.0),
        ];
        let (start, goal) = (at(42.5, 55.5), at(16.5, 28.5));
        let portals = portals_of(&ends, None);
        assert_eq!(
            (at(45.0, 58.0) - start).cross_2d(at(36.0, 49.0) - start),
            SimScalar::ZERO,
            "the start is not on the first portal's line"
        );
        let f = largest_f(start, goal, &portals);
        assert!(
            f <= SimScalar::from_num(42.975),
            "a bound of {} beyond 42.975",
            f.to_f32()
        );
    }

    /// One draw of the partial-knowledge layout corpus's generator (`SplitMix64`), below
    /// `bound`: the generator of `tests/support/layouts.rs`, which this module cannot include.
    fn layout_draw(state: &mut u64, bound: u64) -> u64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        z % bound
    }

    /// The corpus's footprints: `Place` size in cells, then the contour in quarter cells.
    const SHAPES: [(u8, u8, u16, u16, u16); 7] = [
        (5, 5, 10, 10, 6),
        (3, 3, 6, 6, 2),
        (3, 3, 6, 6, 3),
        (2, 2, 4, 4, 2),
        (2, 2, 2, 2, 1),
        (1, 1, 2, 2, 0),
        (3, 2, 6, 4, 2),
    ];

    /// The shipped terrain radii, in sixteenths of a cell.
    const RADII: [i64; 9] = [5, 6, 8, 9, 10, 12, 14, 15, 16];

    /// Knowledge of a corpus layout's structures.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Knowledge {
        /// Each structure known or not as a whole.
        Whole,
        /// Each cell known or not.
        PerCell,
        /// Every structure known.
        All,
    }

    /// The default arena, loaded from its shipped file.
    fn default_arena() -> std::result::Result<GameMap, String> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/maps/default_arena.scmap.ron");
        let file = MapFile::load(&path).map_err(|e| e.to_string())?;
        let document = file.decode().map_err(|e| e.to_string())?;
        document.to_game_map().map_err(|e| e.to_string())
    }

    /// One request of a corpus layout: start, goal, radius.
    type LayoutRequest = (SimVec2, SimVec2, SimScalar);

    /// A corpus layout: the map, the knowledge mask (`None`: all known) and its requests.
    type Layout = (GameMap, Option<Vec<bool>>, Vec<LayoutRequest>);

    /// Layout `number` of the corpus: the map with every structure, the knowledge mask (`None`
    /// when every structure is known) and the 16 requests in draw order.
    fn layout(
        number: u64,
        contours: bool,
        knowledge: Knowledge,
    ) -> std::result::Result<Layout, String> {
        let mut state = number.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5eed;
        let mut map = default_arena()?;
        let count = 2 + u32::try_from(number % 12).unwrap_or(0);
        let (mut placed, mut whole) = (0, Vec::new());
        for attempt in 0..count * 6 {
            if placed == count {
                break;
            }
            let (width, height, half_width_q, half_height_q, chamfer_q) =
                SHAPES[usize::try_from(layout_draw(&mut state, 7)).unwrap_or(0)];
            let cell = (
                u32::try_from(layout_draw(&mut state, u64::from(map.width() - 5))).unwrap_or(0),
                u32::try_from(layout_draw(&mut state, u64::from(map.height() - 5))).unwrap_or(0),
            );
            let is_known = layout_draw(&mut state, 2) == 0;
            let footprint = if contours {
                StructureFootprint {
                    width,
                    height,
                    shape: FootprintShape {
                        half_width_q,
                        half_height_q,
                        chamfer_q,
                    },
                }
            } else {
                StructureFootprint::square(width, height)
            };
            if map
                .place_structure(EntityId(40_000 + attempt), cell, footprint)
                .is_ok()
            {
                placed += 1;
                whole.push((EntityId(40_000 + attempt), is_known));
            }
        }
        let width = map.width();
        let mut known = vec![false; (map.width() * map.height()) as usize];
        for (id, structure) in map.structures() {
            let is_known = whole.iter().any(|&(w, k)| w == *id && k);
            let (left, top) = structure.top_left;
            for y in top..top + u32::from(structure.height) {
                for x in left..left + u32::from(structure.width) {
                    known[(y * width + x) as usize] = if knowledge == Knowledge::PerCell {
                        layout_draw(&mut state, 2) == 0
                    } else {
                        is_known
                    };
                }
            }
        }
        let mut requests = Vec::new();
        for _ in 0..16 {
            let mut centre = || {
                let x = u32::try_from(layout_draw(&mut state, u64::from(map.width()))).unwrap_or(0);
                let y =
                    u32::try_from(layout_draw(&mut state, u64::from(map.height()))).unwrap_or(0);
                map.cell_to_world_center(x, y).map_err(|e| e.to_string())
            };
            let (start, goal) = (centre()?, centre()?);
            let sixteenths = RADII[usize::try_from(layout_draw(&mut state, 9)).unwrap_or(0)];
            requests.push((start, goal, SimScalar::from_raw(sixteenths << 28)));
        }
        let known = (knowledge != Knowledge::All).then_some(known);
        Ok((map, known, requests))
    }

    /// What [`audit_channels`] found over the channels of one search.
    #[derive(Default)]
    struct Audit {
        /// Channels that reached the goal with a legal pull.
        legal_channels: u32,
        /// Nodes on those channels whose `g + h` exceeds the pull's length: `(g + h, length)`.
        excess: Vec<(f32, f32)>,
        /// Nodes whose incremental funnel differs from the funnel recomputed from the start.
        diverged: u32,
    }

    /// Runs TA*'s first pass for `request` as `route` sets it up and checks every node it
    /// pushed: its incremental funnel equals the funnel recomputed from the start through its
    /// channel (R-5), and on every channel that reaches the goal with a legal inscribed pull,
    /// every node's `g + h` is at most the pull's length (D-1's admissibility).
    fn audit_channels(
        mesh: &NavMesh,
        search: &mut MeshSearch,
        map: &GameMap,
        request: &PathRequestWithVision<'_>,
    ) -> Audit {
        let mut audit = Audit::default();
        let access = Access::new(mesh, map, request);
        let Ok((start_triangle, goal_triangle)) =
            endpoints(mesh, &access, &mut search.segment, request)
        else {
            return audit;
        };
        let (start, goal) = (request.start, request.goal);
        if start_triangle == goal_triangle
            || segment_clear(
                mesh,
                &access,
                &mut search.segment,
                start,
                goal,
                request.radius,
            )
        {
            return audit;
        }
        let query = Query {
            mesh,
            access: &access,
            start,
            goal,
            goal_triangle,
            radius: request.radius,
            start_fits: segment_clear(
                mesh,
                &access,
                &mut search.segment,
                start,
                start,
                request.radius,
            ),
            end_checks: false,
            lane: None,
        };
        search.ta_star(&query, start_triangle);
        let (s, g) = (
            mesh.triangle_vertices(start_triangle),
            mesh.triangle_vertices(goal_triangle),
        );
        for id in 0..u32::try_from(search.nodes.len()).unwrap_or(0) {
            search.collect_portals(mesh, id);
            let channel = search.portals.clone();
            let portals: Vec<BoundPortal> = channel
                .iter()
                .map(|&portal| search.bound_portal(&query, portal))
                .collect();
            let mut recomputed = PointFunnel::at(start);
            recomputed.advance(&portals, 1);
            let kept = search.nodes[id as usize].funnel;
            if (kept.apex, kept.length) != (recomputed.apex, recomputed.length) {
                audit.diverged += 1;
            }
            let node = search.nodes[id as usize];
            if node.triangle != goal_triangle || node.parent == NONE {
                continue;
            }
            let route = radius_funnel(
                mesh,
                (start, goal),
                &channel,
                &[s[0], s[1], s[2], g[0], g[1], g[2]],
                (
                    request.radius + CORNER_SAFETY_MARGIN,
                    Emission::Inscribed { dip: ARC_CHORD_DIP },
                    PullStrategy::Shrunk,
                ),
                &mut Obstacles {
                    access: &access,
                    segment: &mut search.segment,
                    scratch: &mut search.funnel,
                },
            );
            if !search.is_legal(&query, &route) {
                continue;
            }
            audit.legal_channels += 1;
            let mut current = id;
            while current != NONE {
                let on = search.nodes[current as usize];
                if on.g + on.h > route.length {
                    audit
                        .excess
                        .push(((on.g + on.h).to_f32(), route.length.to_f32()));
                }
                current = on.parent;
            }
        }
        audit
    }

    /// T5 of plan `2026-11-20-tastar-budget` (per-cell squares layout 142 q 0, r 1.0, a route of
    /// 34.324): the free vertex (26, 26) is 1.0 from the known corner (25, 26), within `rho`
    /// 1.05, so the radius funnel clamps that portal end outwards and the route passes beyond it
    /// (x 26.42 at y 26). The channel bound gives that end no side, and no node of any channel
    /// with a legal pull bounds the route above its length.
    #[test]
    fn a_clamped_free_end_bounds_nothing() -> std::result::Result<(), String> {
        let (map, known, requests) = layout(142, false, Knowledge::PerCell)?;
        let mesh = NavMesh::build(&map).map_err(|e| e.to_string())?;
        let (start, goal, radius) = requests[0];
        assert_eq!(
            (start, goal, radius),
            (at(21.5, 20.5), at(40.5, 48.5), SimScalar::ONE),
            "layout 142 q 0 is not the plan's request"
        );
        let request = PathRequestWithVision {
            start,
            goal,
            radius,
            vision: None,
            block_unexplored_ramps: false,
            known_structures: known.as_deref(),
        };
        let mut search = MeshSearch::default();
        let route = find_route(&mesh, &mut search, &map, &request).map_err(|e| e.to_string())?;
        assert!(!route.fallback);
        // The clamped end is a portal end of the route's channel, and clampable.
        let access = Access::new(&mesh, &map, &request);
        let query = Query {
            mesh: &mesh,
            access: &access,
            start,
            goal,
            goal_triangle: 0,
            radius,
            start_fits: true,
            end_checks: false,
            lane: None,
        };
        search.begin_clampable(mesh.vertex_count());
        let channel = search.portals.clone();
        let clamped = at(26.0, 26.0);
        let portals: Vec<BoundPortal> = channel
            .iter()
            .map(|&portal| search.bound_portal(&query, portal))
            .collect();
        assert!(
            portals
                .iter()
                .any(|p| (p.right == clamped && !p.right_bounds)
                    || (p.left == clamped && !p.left_bounds)),
            "(26, 26) is not a clampable end of the channel"
        );
        let f = largest_f(start, goal, &portals);
        assert!(
            f <= route.total_length,
            "the channel bound {} exceeds the route's {}",
            f.to_f32(),
            route.total_length.to_f32()
        );
        let audit = audit_channels(&mesh, &mut search, &map, &request);
        assert!(audit.legal_channels > 0);
        assert!(audit.excess.is_empty(), "{:?}", audit.excess);
        Ok(())
    }

    /// D-1 of plan `2026-11-20-tastar-budget`: over the four partial-knowledge layout corpora at
    /// test size (40 layouts each: squares and contours, knowledge per structure and per cell)
    /// and the all-known corpus, no node on a channel with a legal pull bounds the route above
    /// the pull's length, and every node's incremental funnel is the funnel recomputed from the
    /// start.
    #[test]
    fn channel_bounds_never_exceed_a_legal_pull() -> std::result::Result<(), String> {
        let corpora = [
            (false, Knowledge::Whole),
            (true, Knowledge::Whole),
            (false, Knowledge::PerCell),
            (true, Knowledge::PerCell),
            (false, Knowledge::All),
        ];
        let mut search = MeshSearch::default();
        let (mut channels, mut excess, mut diverged) = (0u32, Vec::new(), 0u32);
        for (contours, knowledge) in corpora {
            for number in 0..40 {
                let (map, known, requests) = layout(number, contours, knowledge)?;
                let mesh = NavMesh::build(&map).map_err(|e| e.to_string())?;
                for (start, goal, radius) in requests {
                    let request = PathRequestWithVision {
                        start,
                        goal,
                        radius,
                        vision: None,
                        block_unexplored_ramps: false,
                        known_structures: known.as_deref(),
                    };
                    let audit = audit_channels(&mesh, &mut search, &map, &request);
                    channels += audit.legal_channels;
                    diverged += audit.diverged;
                    excess.extend(audit.excess.iter().map(|&e| (number, contours, e)));
                }
            }
        }
        eprintln!(
            "{channels} legal channels audited, {} excess, {diverged} diverged",
            excess.len()
        );
        assert!(channels > 1_000, "only {channels} channels audited");
        assert!(excess.is_empty(), "bounds beyond a legal pull: {excess:?}");
        assert_eq!(
            diverged, 0,
            "incremental funnels that differ from the recomputed"
        );
        Ok(())
    }

    /// Amendment 1: the fitted margin is half the spare width, floored to the raw grid, and only
    /// when it is positive and below the full margin.
    #[test]
    fn the_fitted_margin_is_half_the_spare_width() {
        let cell = SquaredWidth::whole(1);
        let spare = SimScalar::from_raw(CORNER_SAFETY_MARGIN.raw() / 4);
        let radius = (SimScalar::ONE - spare) / 2;
        let margin = fitted_margin(cell, radius);
        assert_eq!(margin, Some((SimScalar::ONE - radius * 2) / 2));
        // The radius was halved with rounding down, so the spare is at most one raw unit more.
        assert!(margin.is_some_and(|m| m * 2 <= spare + SimScalar::EPSILON && m > SimScalar::ZERO));
        // Room for the full margin: no retry.
        let roomy = (SimScalar::ONE - CORNER_SAFETY_MARGIN * 2) / 2;
        assert_eq!(fitted_margin(cell, roomy), None);
        // No room at all, or an unbounded channel: no retry.
        assert_eq!(fitted_margin(cell, SimScalar::HALF), None);
        assert_eq!(
            fitted_margin(SquaredWidth::UNBOUNDED, SimScalar::HALF),
            None
        );
    }
}
