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
//!   channels, never twice on one channel. `g` is the larger of two lower bounds, the straight
//!   distance from the start to the entry edge and the parent's `g` plus the drop in `h`; `h` is
//!   the distance from the goal to the entry edge. Both are admissible.
//! - The first channel to reach the goal sets the incumbent (its funnel length). The search goes
//!   on while an open node's `f` is below the incumbent, for at most
//!   [`NAV_TA_STAR_EXTRA_EXPANSIONS`] further expansions (S11's anytime loop).
//! - Ties break on `(f, triangle, node)`; every container is a `Vec` or `BinaryHeap` filled in
//!   index order, so the result is a pure function of the mesh and the request.
//! - The start triangle is entered whatever its width (D-3); every later triangle passed through
//!   must admit the disc between its entry and exit edges.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use sim_types::{SimScalar, SimVec2};

use super::cdt::NONE;
use super::funnel::{portal_midpoints, radius_funnel, Anchor, FunnelRoute, PullStrategy, Side};
use super::predicates::LatticePoint;
use super::segment::{segment_clear, SegmentScratch};
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
    /// Funnel legs that failed the segment predicate since construction (a diagnostic, D-6).
    pub fallbacks: u64,
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
}

impl MeshSearch {
    fn reset(&mut self, triangles: usize) {
        self.nodes.clear();
        self.open.clear();
        self.visits.clear();
        self.visits.resize(3 * triangles, 0);
    }

    /// The channel of the last route returned by [`find_route`]: the portals it crosses, as
    /// `(left, right)` vertex pairs, start first; empty for a straight route.
    pub(crate) fn channel(&self) -> &[(u32, u32)] {
        &self.portals
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
            let h = point_segment_distance(query.goal, a, b);
            let g = self.lower_bound(query, id, a, b);
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
            });
        }
    }

    /// An admissible lower bound on the length of any route of this channel from the start to
    /// the edge `[a, b]` leaving node `id`: the route crosses each ancestor's entry edge `e` after
    /// at least that ancestor's bound, then needs at least the distance from `e` to `[a, b]`.
    fn lower_bound(&self, query: &Query<'_>, id: u32, a: SimVec2, b: SimVec2) -> SimScalar {
        let mut bound = point_segment_distance(query.start, a, b);
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
            (query.radius + CORNER_SAFETY_MARGIN, PullStrategy::Pinned),
        );
        if self.is_legal(query, &pinned) {
            (pinned, true)
        } else {
            (shrunk, false)
        }
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
        self.reset(query.mesh.triangle_count());
        self.push(Node {
            triangle: start_triangle,
            entry: NO_EDGE,
            via: NO_EDGE,
            parent: NONE,
            g: SimScalar::ZERO,
            h: query.start.distance(query.goal),
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
                self.collect_portals(query.mesh, id);
                let route = radius_funnel(
                    query.mesh,
                    (query.start, query.goal),
                    &self.portals,
                    &end_vertices,
                    (rho, PullStrategy::Shrunk),
                );
                if best.as_ref().is_none_or(|(b, _)| route.length < b.length) {
                    best = Some((route, self.portals.clone()));
                }
                continue;
            }
            // Counted per (triangle, entry edge) state: the first arrival in a state is always
            // expanded, so the search is complete; later arrivals explore other channels.
            let state = 3 * node.triangle as usize + usize::from(node.entry.min(2));
            let visits = &mut self.visits[state];
            if *visits >= NAV_TA_STAR_TRIANGLE_VISITS {
                continue;
            }
            *visits += 1;
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
    let access = Access::new(mesh, map, request);
    let (start_triangle, goal_triangle) = endpoints(mesh, &access, &mut search.segment, request)?;
    let (start, goal) = (request.start, request.goal);
    let straight = |expansions| MeshRoute {
        waypoints: vec![start, goal],
        total_length: start.distance(goal),
        turns: Vec::new(),
        fallback: false,
        expansions,
    };
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
    };
    let (route, legal, expansions) = search.two_pass(&query, start_triangle);
    let route = route.ok_or(MapError::NoPathFound { start, goal })?;
    let (route, legal) = if legal {
        (route, true)
    } else {
        search.pinned_retry(&query, start_triangle, route)
    };
    let turns = route
        .anchors
        .iter()
        .filter_map(|anchor| match *anchor {
            Anchor::Vertex(v, side) => Some((mesh.vertex(v), side)),
            Anchor::Point(_) => None,
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
        expansions,
    })
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
