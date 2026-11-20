//! Property tests for the clearance field and the funnel path invariants.
//!
//! # Responsibility
//! Checks laws that must hold on every map rather than on chosen examples: the distance
//! transform agrees with a brute-force oracle, clearance has no directional bias, and every
//! path the search returns is legal, complete and no shorter than a straight line.
//!
//! # How it fits
//! All ground movement starts from these results. The oracle in this file is deliberately
//! simple and shares no code with the linear-time transform it verifies.
//!
//! # Invariants
//! Maps are small, so the O(cells * walls) oracle stays cheap. The navigation mesh's routes
//! (sc2 pathfinding WP6) are compared with the exact disc oracle of `support/oracle.rs`. Generated maps may be entirely
//! disconnected; properties that need a route are conditional on the search finding one.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "indices into a 16x16 test map always fit in i64 and u32"
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/oracle.rs"]
mod oracle;

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/arena.rs"]
mod arena;

#[path = "support/draw.rs"]
mod draw;
use draw::draw_index;

#[path = "support/radii.rs"]
mod radii;

#[allow(
    dead_code,
    reason = "the corpus's four kinds are named; this binary routes a subset"
)]
#[path = "support/layouts.rs"]
mod layouts;
use layouts::LayoutKind;
use radii::{required_clearance, LARGE_RADIUS, MEDIUM_RADIUS, SMALL_RADIUS};

use proptest::collection::vec;
use proptest::prelude::*;
use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapError, PathRequest, PathRequestWithVision,
    Pathfinder, Side, CORNER_SAFETY_MARGIN,
};
use sim_types::{SimScalar, SimVec2};

const SIDE: u32 = 16;
const CELLS: usize = (SIDE as usize) * (SIDE as usize);

/// A blocked-cell mask with roughly one cell in `density` blocked.
fn obstacle_mask(density: u32) -> impl Strategy<Value = Vec<bool>> {
    vec((0..density).prop_map(|roll| roll == 0), CELLS)
}

fn build_map(blocked: &[bool]) -> Result<GameMap, TestCaseError> {
    let mut map = GameMap::new(SIDE, SIDE, MapCell::default_ground(CliffLevel(1)))
        .map_err(|error| TestCaseError::fail(error.to_string()))?;
    for (cell, &is_blocked) in map.cells_mut().iter_mut().zip(blocked) {
        if is_blocked {
            cell.flags = CellFlags::empty();
        }
    }
    Ok(map)
}

fn centre(x: u32, y: u32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

/// One of the three sample body radii, by index.
fn tier(index: u8) -> SimScalar {
    match index {
        0 => SMALL_RADIUS,
        1 => MEDIUM_RADIUS,
        _ => LARGE_RADIUS,
    }
}

/// Every half-cell lattice point lying on a wall of a flat map.
fn wall_points(blocked: &[bool]) -> Vec<(i64, i64)> {
    let lattice = i64::from(2 * SIDE);
    let mut points = Vec::new();
    for offset in 0..=lattice {
        points.extend([
            (offset, 0),
            (offset, lattice),
            (0, offset),
            (lattice, offset),
        ]);
    }
    for (index, _) in blocked.iter().enumerate().filter(|(_, &cell)| cell) {
        let (x, y) = (
            index as i64 % i64::from(SIDE),
            index as i64 / i64::from(SIDE),
        );
        for offset_y in 0..=2 {
            for offset_x in 0..=2 {
                points.push((2 * x + offset_x, 2 * y + offset_y));
            }
        }
    }
    points
}

/// Brute-force clearance of a cell, in eighths of a world unit.
fn oracle_clearance(walls: &[(i64, i64)], x: i64, y: i64) -> u8 {
    let mut best = 0;
    for offset_y in 0..=2 {
        for offset_x in 0..=2 {
            let (point_x, point_y) = (2 * x + offset_x, 2 * y + offset_y);
            let nearest = walls
                .iter()
                .map(|&(wall_x, wall_y)| (wall_x - point_x).pow(2) + (wall_y - point_y).pow(2))
                .min()
                .unwrap_or(0);
            best = best.max(nearest);
        }
    }
    u8::try_from((16 * best.unsigned_abs()).isqrt()).unwrap_or(u8::MAX)
}

/// Fallbacks from a wedged start that `funnel_paths_are_legal_and_no_shorter_than_a_straight_line`
/// exempted, counted across its cases.
static WEDGED_FALLBACKS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The linear-time transform gives the same answer as checking every wall point.
    #[test]
    fn clearance_matches_brute_force_oracle(blocked in obstacle_mask(6)) {
        let map = build_map(&blocked)?;
        let pathfinder = Pathfinder::new(&map);
        let walls = wall_points(&blocked);
        for y in 0..SIDE {
            for x in 0..SIDE {
                prop_assert_eq!(
                    pathfinder.clearance_at(x, y),
                    oracle_clearance(&walls, i64::from(x), i64::from(y)),
                    "cell ({}, {})", x, y
                );
            }
        }
    }

    /// Blocked cells have zero clearance, and every open cell admits the smallest unit.
    #[test]
    fn zero_clearance_means_exactly_blocked(blocked in obstacle_mask(4)) {
        let map = build_map(&blocked)?;
        let pathfinder = Pathfinder::new(&map);
        for (index, &is_blocked) in blocked.iter().enumerate() {
            let index = index as u32;
            let clearance = pathfinder.clearance_at(index % SIDE, index / SIDE);
            prop_assert_eq!(clearance == 0, is_blocked);
            if !is_blocked {
                prop_assert!(clearance >= required_clearance(SMALL_RADIUS));
            }
        }
    }

    /// Rotating a map half a turn rotates its clearance, so walls have no preferred side.
    #[test]
    fn clearance_has_no_directional_bias(blocked in obstacle_mask(5)) {
        let rotated: Vec<bool> = blocked.iter().rev().copied().collect();
        let forward = Pathfinder::new(&build_map(&blocked)?);
        let backward = Pathfinder::new(&build_map(&rotated)?);
        for y in 0..SIDE {
            for x in 0..SIDE {
                prop_assert_eq!(
                    forward.clearance_at(x, y),
                    backward.clearance_at(SIDE - 1 - x, SIDE - 1 - y)
                );
            }
        }
    }

    /// Every returned path joins its endpoints through legal segments, and reporting is honest.
    /// A start where the disc does not fit may always be left (plan D-3), so its first leg is
    /// held to the predicate only when the disc fits at the start. A counted fallback to portal
    /// midpoints is exempt only from a wedged start (plan `2026-11-19-funnel-fallbacks` §9), and
    /// each exempted case is counted in the test's output. Half the requests know only some of the
    /// map's seeded structures, each known or not as a whole (plan
    /// `2026-11-19-partial-knowledge-funnel` WP4: free portal vertices are pulled as points, so
    /// partial knowledge needs no exemption).
    #[test]
    fn funnel_paths_are_legal_and_no_shorter_than_a_straight_line(
        blocked in obstacle_mask(8),
        structures in vec((0..SIDE - 3, 0..SIDE - 3, 1_u8..4, proptest::bool::ANY), 0..5),
        partial in proptest::bool::ANY,
        start in (0..SIDE, 0..SIDE),
        goal in (0..SIDE, 0..SIDE),
        tier_index in 0_u8..3,
    ) {
        let mut map = build_map(&blocked)?;
        let mut known = vec![false; CELLS];
        for (id, &(x, y, size, is_known)) in (60_000_u32..).zip(&structures) {
            let footprint = sim_map::StructureFootprint::square(size, size);
            // A refused placement leaves the map as it was.
            if map.place_structure(sim_types::EntityId(id), (x, y), footprint).is_ok() && is_known {
                for cy in y..y + u32::from(size) {
                    for cx in x..x + u32::from(size) {
                        known[(cy * SIDE + cx) as usize] = true;
                    }
                }
            }
        }
        let mut pathfinder = Pathfinder::new(&map);
        let request = PathRequestWithVision {
            known_structures: partial.then_some(known.as_slice()),
            ..mesh_request(centre(start.0, start.1), centre(goal.0, goal.1), tier(tier_index))
        };
        let fallbacks = pathfinder.mesh_funnel_fallbacks();
        let found = pathfinder.find_path_with_vision(&map, &request);
        // A counted fallback through portal midpoints (plan D-6, R-2) is exempt from legality.
        let fell_back = pathfinder.mesh_funnel_fallbacks() != fallbacks;
        let start_fits =
            pathfinder.segment_is_passable(&map, &request, request.start, request.start);
        prop_assert!(!fell_back || !start_fits, "fallback from a fitting start: {:?}", request);
        if fell_back {
            let exempted = WEDGED_FALLBACKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            eprintln!("wedged-start fallback {exempted}: {request:?}");
        }
        if let Ok(path) = found {
            prop_assert_eq!(path.waypoints.first(), Some(&request.start));
            prop_assert_eq!(path.waypoints.last(), Some(&request.goal));
            // D-3: legs from a wedged start up to the first waypoint where the disc fits are held
            // to the predicate for the unit's centre only, as the search's own legality check
            // holds them. A turn rounded right beside the wedged start puts its first arc point
            // inside the disc's band along the wall that wedges it (the seed of record, start
            // (9, 12), radius 0.625).
            let centre_only = PathRequestWithVision {
                radius: SimScalar::ZERO,
                ..request
            };
            let mut wedged = true;
            let mut measured = SimScalar::ZERO;
            for pair in path.waypoints.windows(2) {
                measured += pair[0].distance(pair[1]);
                wedged = wedged
                    && !pathfinder.segment_is_passable(&map, &request, pair[0], pair[0]);
                let legal = if wedged {
                    pathfinder.segment_is_passable(&map, &centre_only, pair[0], pair[1])
                } else {
                    pathfinder.segment_is_passable(&map, &request, pair[0], pair[1])
                };
                prop_assert!(fell_back || legal, "illegal segment {:?} (wedged {})", pair, wedged);
            }
            prop_assert_eq!(measured, path.total_length);
            let straight = request.start.distance(request.goal);
            prop_assert!(path.total_length >= straight - SimScalar::from_num(0.001));
        }
    }

    /// Reachability is symmetric: terrain has no one-way doors. Compared where both endpoints
    /// take the disc: a wedged start may be left (D-3) while the same point is refused as a goal.
    #[test]
    fn reachability_is_symmetric(
        blocked in obstacle_mask(5),
        start in (0..SIDE, 0..SIDE),
        goal in (0..SIDE, 0..SIDE),
        tier_index in 0_u8..3,
    ) {
        let map = build_map(&blocked)?;
        let mut pathfinder = Pathfinder::new(&map);
        let radius = tier(tier_index);
        let forward = PathRequest {
            start: centre(start.0, start.1),
            goal: centre(goal.0, goal.1),
            radius,
        };
        let backward = PathRequest {
            start: forward.goal,
            goal: forward.start,
            radius,
        };
        let fits = |pathfinder: &mut Pathfinder, at: SimVec2| {
            pathfinder.segment_is_passable(&map, &forward.into(), at, at)
        };
        if fits(&mut pathfinder, forward.start) && fits(&mut pathfinder, forward.goal) {
            prop_assert_eq!(
                pathfinder.find_path(&map, &forward).is_ok(),
                pathfinder.find_path(&map, &backward).is_ok()
            );
        }
    }

    /// Searching twice with reused buffers gives the same result as a fresh pathfinder.
    #[test]
    fn repeated_queries_do_not_depend_on_buffer_history(
        blocked in obstacle_mask(8),
        first in (0..SIDE, 0..SIDE),
        second in (0..SIDE, 0..SIDE),
    ) {
        let map = build_map(&blocked)?;
        let mut reused = Pathfinder::new(&map);
        let distraction = PathRequest {
            start: centre(first.0, first.1),
            goal: centre(second.0, second.1),
            radius: SMALL_RADIUS,
        };
        let target = PathRequest {
            start: distraction.goal,
            goal: distraction.start,
            radius: MEDIUM_RADIUS,
        };
        let _discarded = reused.find_path(&map, &distraction);
        let after_reuse = reused.find_path(&map, &target);
        let fresh = Pathfinder::new(&map).find_path(&map, &target);
        prop_assert_eq!(after_reuse.ok(), fresh.ok());
    }
}

/// Radii whose disc and safety-margin disc fit the same lattice gaps, with room to spare: no gap
/// width `sqrt(k)` lies in `[2r - 0.02, 2(r + margin) + 0.02]`, so a route exists at `r` exactly
/// when it does at `r + margin`, and no route squeezes through a slot a hair wider than the disc
/// (where floating-point tolerance, not geometry, would decide the oracle's answer).
fn radius_is_unambiguous(radius: f64) -> bool {
    let rho = radius + f64::from(CORNER_SAFETY_MARGIN.to_f32());
    (1..=64).all(|k: u32| {
        let gap = f64::from(k).sqrt();
        gap < 2.0 * radius - 0.02 || gap > 2.0 * rho + 0.02
    })
}

fn mesh_request(
    start: SimVec2,
    goal: SimVec2,
    radius: SimScalar,
) -> PathRequestWithVision<'static> {
    PathRequestWithVision {
        start,
        goal,
        radius,
        vision: None,
        block_unexplored_ramps: false,
        known_structures: None,
    }
}

fn to_f64(p: SimVec2) -> (f64, f64) {
    (f64::from(p.x.to_f32()), f64::from(p.y.to_f32()))
}

/// Compares the mesh route with the oracle for one pair. Returns the ratio of the route's
/// tangent-and-arc length to the oracle's at `r + margin` (`None` when neither finds a route),
/// or a violated law. The route may be shorter than that oracle where its straight shortcut
/// passes within the margin of a corner, but never shorter than the oracle at `r` itself.
fn compare_with_oracle(
    map: &GameMap,
    pathfinder: &mut Pathfinder,
    tight: &oracle::Oracle,
    padded: &oracle::Oracle,
    (start, goal): (SimVec2, SimVec2),
    radius: SimScalar,
) -> Result<Option<f64>, String> {
    let rho = f64::from((radius + CORNER_SAFETY_MARGIN).to_f32());
    let (s, g) = (to_f64(start), to_f64(goal));
    let request = mesh_request(start, goal, radius);
    let mesh = pathfinder.find_route_mesh(map, &request);
    let reference = padded.shortest(s, g);
    let (route, reference) = match (mesh, reference) {
        (Err(_), None) => return Ok(None),
        (Ok(route), Some(reference)) => (route, reference),
        (mesh, reference) => {
            return Err(format!(
                "existence differs: mesh {mesh:?}, oracle {reference:?}"
            ));
        }
    };
    if route.fallback {
        // R-2's second outcome: the D-6 post-check refused the funnel route and the portal
        // midpoints were used. Counted by the share test; existence still had to agree.
        return Ok(Some(FALLBACK));
    }
    for pair in route.waypoints.windows(2) {
        if !pathfinder.segment_is_passable(map, &request, pair[0], pair[1]) {
            return Err(format!("leg {pair:?} fails the segment predicate"));
        }
    }
    let turns: Vec<((f64, f64), i8)> = route
        .turns
        .iter()
        .map(|&(p, side)| {
            (
                (
                    f64::from(p.to_sim().x.to_f32()),
                    f64::from(p.to_sim().y.to_f32()),
                ),
                if side == Side::Left { -1 } else { 1 },
            )
        })
        .collect();
    let polyline = f64::from(route.total_length.to_f32());
    let smooth = if turns.is_empty() {
        polyline
    } else {
        oracle::smooth_length(s, g, &turns, rho)
    };
    let floor = tight.shortest(s, g).unwrap_or(reference);
    if polyline < floor - 1e-3 {
        return Err(format!(
            "route {polyline} is shorter than the legal minimum {floor}"
        ));
    }
    Ok(Some(if reference > 0.0 {
        smooth / reference
    } else {
        1.0
    }))
}

/// The ratio reported for a route that fell back to portal midpoints.
const FALLBACK: f64 = -1.0;

/// Every cell centre with room for the padded disc, in index order.
fn roomy_centres(map: &GameMap, padded: &oracle::Oracle, rho: f64) -> Vec<SimVec2> {
    let mut out = Vec::new();
    for y in 0..map.height() {
        for x in 0..map.width() {
            let p = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            if map.terrain_step_is_connected((x, y), (x, y)) && padded.clearance(p) >= rho + 1e-6 {
                out.push(centre(x, y));
            }
        }
    }
    out
}

fn is_no_path<T>(result: &Result<T, MapError>) -> bool {
    matches!(result, Err(MapError::NoPathFound { .. }))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// The mesh finds a route exactly when the exact oracle does, its legs pass the segment
    /// predicate, it is never shorter than the legal minimum, and it is never more than
    /// [`MESH_RATIO_BOUND`] times the oracle's length (sc2 pathfinding WP6, D-11; the share
    /// within 1 % is measured by `mesh_route_lengths_are_within_one_percent_of_the_oracle`).
    #[test]
    fn mesh_routes_agree_with_the_exact_oracle(
        blocked in obstacle_mask(5),
        radius_steps in 0u32..=75,
        picks in vec((0usize..1000, 0usize..1000), 4),
    ) {
        let radius_f = 0.25 + f64::from(radius_steps) * 0.01;
        prop_assume!(radius_is_unambiguous(radius_f));
        let radius = SimScalar::from_num(radius_f);
        let rho = f64::from((radius + CORNER_SAFETY_MARGIN).to_f32());
        let map = build_map(&blocked)?;
        let walls = oracle::walls_of(&map);
        let tight = oracle::Oracle::new(walls.clone(), f64::from(radius.to_f32()));
        let padded = oracle::Oracle::new(walls, rho);
        let centres = roomy_centres(&map, &padded, rho);
        prop_assume!(centres.len() >= 2);
        let mut pathfinder = Pathfinder::new(&map);
        for (a, b) in picks {
            let pair = (centres[a % centres.len()], centres[b % centres.len()]);
            let ratio = compare_with_oracle(&map, &mut pathfinder, &tight, &padded, pair, radius)
                .map_err(TestCaseError::fail)?;
            if let Some(ratio) = ratio.filter(|&ratio| ratio != FALLBACK) {
                prop_assert!(ratio <= MESH_RATIO_BOUND, "{pair:?} at r {radius_f}: ratio {ratio}");
            }
        }
    }

    /// Mesh routes exist symmetrically and repeat exactly, with reused or fresh buffers.
    #[test]
    fn mesh_routes_are_symmetric_and_repeatable(
        blocked in obstacle_mask(5),
        start in (0..SIDE, 0..SIDE),
        goal in (0..SIDE, 0..SIDE),
        radius_steps in 0u32..=75,
    ) {
        let map = build_map(&blocked)?;
        let radius = SimScalar::from_num(0.25 + f64::from(radius_steps) * 0.01);
        let (a, b) = (centre(start.0, start.1), centre(goal.0, goal.1));
        let mut pathfinder = Pathfinder::new(&map);
        let forward = pathfinder.find_route_mesh(&map, &mesh_request(a, b, radius));
        let backward = pathfinder.find_route_mesh(&map, &mesh_request(b, a, radius));
        let endpoints_fit = |r: &Result<sim_map::MeshRoute, MapError>| {
            !matches!(r, Err(MapError::InvalidStart(_) | MapError::InvalidGoal(_)))
        };
        if endpoints_fit(&forward) && endpoints_fit(&backward) {
            prop_assert_eq!(is_no_path(&forward), is_no_path(&backward));
        }
        let again = pathfinder.find_route_mesh(&map, &mesh_request(a, b, radius));
        let fresh = Pathfinder::new(&map).find_route_mesh(&map, &mesh_request(a, b, radius));
        prop_assert_eq!(&forward, &again);
        prop_assert_eq!(&forward, &fresh);
    }
}

/// Hard bound of a mesh route's length over the oracle's. D-11 asks for 1.01; TA* with a
/// bounded anytime search can settle on a slightly longer channel, so 1.01 is held as a share
/// below (recorded deviation of WP6) and this bound catches real defects.
const MESH_RATIO_BOUND: f64 = 1.10;

/// Over a fixed set of seeded maps, radii and pairs, at least 95 % of mesh routes are within
/// 1 % of the oracle's length, none is beyond [`MESH_RATIO_BOUND`], none disagrees on
/// existence or fails a leg, and none falls back to portal midpoints: every endpoint has room
/// for the disc (plan `2026-11-19-funnel-fallbacks` WP4; before its D-2 and D-3, up to 2 % did).
#[test]
fn mesh_route_lengths_are_within_one_percent_of_the_oracle() {
    use rand::{Rng, SeedableRng};
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0x0_7ac1e);
    let (mut within, mut total, mut worst, mut fallbacks) = (0u32, 0u32, 1.0f64, 0u32);
    let mut maps = 0;
    while maps < 120 {
        let radius_f = 0.25 + f64::from(rng.gen_range::<u32, _>(0..=75)) * 0.01;
        if !radius_is_unambiguous(radius_f) {
            continue;
        }
        maps += 1;
        let blocked: Vec<bool> = (0..CELLS)
            .map(|_| rng.gen_range::<i32, _>(0..5) == 0)
            .collect();
        let Ok(map) = build_map(&blocked) else {
            continue;
        };
        let radius = SimScalar::from_num(radius_f);
        let rho = f64::from((radius + CORNER_SAFETY_MARGIN).to_f32());
        let walls = oracle::walls_of(&map);
        let tight = oracle::Oracle::new(walls.clone(), f64::from(radius.to_f32()));
        let padded = oracle::Oracle::new(walls, rho);
        let centres = roomy_centres(&map, &padded, rho);
        if centres.len() < 2 {
            continue;
        }
        let mut pathfinder = Pathfinder::new(&map);
        for _ in 0..6 {
            let pair = (
                centres[draw_index(&mut rng, centres.len())],
                centres[draw_index(&mut rng, centres.len())],
            );
            let outcome = compare_with_oracle(&map, &mut pathfinder, &tight, &padded, pair, radius);
            assert!(outcome.is_ok(), "{pair:?} at r {radius_f}: {outcome:?}");
            let ratio = outcome.ok().flatten();
            if let Some(ratio) = ratio {
                total += 1;
                fallbacks += u32::from(ratio == FALLBACK);
                within += u32::from(ratio != FALLBACK && ratio <= 1.01);
                worst = worst.max(ratio);
            }
        }
    }
    let share = f64::from(within) / f64::from(total.max(1));
    eprintln!(
        "mesh against oracle: {within}/{total} within 1 %, worst ratio {worst:.4}, \
         {fallbacks} fallbacks"
    );
    assert_eq!(fallbacks, 0, "{fallbacks}/{total} fell back");
    assert!(worst <= MESH_RATIO_BOUND, "worst ratio {worst}");
    assert!(share >= 0.95, "only {within}/{total} within 1 %");
}

/// Plan `2026-11-19-funnel-fallbacks` WP4: routes for discs exactly as wide as whole-cell gaps
/// (r 0.5 and 1.0) on seeded maps, between cell centres where the disc fits. Every route is
/// legal with no fallback, none is shorter than the oracle's by more than 10⁻³, existence agrees,
/// and none is beyond [`MESH_RATIO_BOUND`]. Of the routes the zero-room retry produced
/// (`MeshRoute::zero_room`: no pull with a corner margin was legal, so the route is an exact fit),
/// at least 95 % are within 1 % of the oracle's length: stop rule 8's measure (plan Amendment 2).
/// Its arcs are bisected to a thirty-second of a turn, whose circumscribed factor 1.0032 keeps a
/// route of many corners within that bound. Routes that keep the margin are longer than this
/// margin-free oracle by design and are compared with it only from below. The six zero-room
/// routes beyond 1 % take another channel than the oracle's (TA*'s channel choice, evidence
/// WP4).
///
/// The oracle runs at `r - 10⁻⁵`, not at `r`: its float tolerance and one-degree arc samples
/// miss a passage exactly `2r` wide (the plan's §4.3 found it reporting 0.976 and 0.899 times
/// the mesh's length at `r`), and a hair less finds it. Routes with room for the margin keep it
/// round their corners, so they are compared with this margin-free oracle only from below.
#[test]
fn exact_fit_routes_agree_with_the_oracle() {
    use rand::{Rng, SeedableRng};
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0x0e_8ac7_f175);
    let (mut routes, mut exact, mut within, mut worst) = (0u32, 0u32, 0u32, 1.0f64);
    for map_index in 0..60_u32 {
        let radius_f = if map_index % 2 == 0 { 0.5 } else { 1.0 };
        let blocked: Vec<bool> = (0..CELLS)
            .map(|_| rng.gen_range::<i32, _>(0..6) == 0)
            .collect();
        let Ok(map) = build_map(&blocked) else {
            continue;
        };
        let radius = SimScalar::from_num(radius_f);
        let tight = oracle::Oracle::new(oracle::walls_of(&map), radius_f - 1e-5);
        let mut pathfinder = Pathfinder::new(&map);
        let mut fitting = Vec::new();
        for y in 0..SIDE {
            for x in 0..SIDE {
                let p = centre(x, y);
                let request = mesh_request(p, p, radius);
                if pathfinder.segment_is_passable(&map, &request, p, p) {
                    fitting.push(p);
                }
            }
        }
        if fitting.len() < 2 {
            continue;
        }
        for _ in 0..6 {
            let (start, goal) = (
                fitting[draw_index(&mut rng, fitting.len())],
                fitting[draw_index(&mut rng, fitting.len())],
            );
            let request = mesh_request(start, goal, radius);
            let mesh = pathfinder.find_route_mesh(&map, &request);
            let reference = tight.shortest(to_f64(start), to_f64(goal));
            let (Ok(route), Some(reference)) = (&mesh, reference) else {
                assert!(
                    mesh.is_err() && reference.is_none(),
                    "{start:?} -> {goal:?} at r {radius_f}: mesh {mesh:?}, oracle {reference:?}"
                );
                continue;
            };
            assert!(!route.fallback, "fallback at r {radius_f}: {route:?}");
            for pair in route.waypoints.windows(2) {
                assert!(
                    pathfinder.segment_is_passable(&map, &request, pair[0], pair[1]),
                    "leg {pair:?} at r {radius_f}"
                );
            }
            let length = f64::from(route.total_length.to_f32());
            assert!(
                length >= reference - 1e-3,
                "{start:?} -> {goal:?} at r {radius_f}: {length} shorter than {reference}"
            );
            routes += 1;
            if reference <= 0.0 {
                continue;
            }
            let ratio = length / reference;
            worst = worst.max(ratio);
            if route.zero_room {
                exact += 1;
                within += u32::from(ratio <= 1.01);
            }
        }
    }
    eprintln!(
        "exact fits against the oracle: {routes} routes (worst ratio {worst:.4}), {within} of \
         {exact} zero-room routes within 1 %"
    );
    assert!(
        routes >= 200 && exact >= 20,
        "{routes} routes, {exact} exact fits"
    );
    assert!(worst <= MESH_RATIO_BOUND, "worst ratio {worst}");
    assert!(
        within * 100 >= exact * 95,
        "only {within} of {exact} zero-room routes within 1 %"
    );
}

/// B-143's shrunk case (plan `2026-11-18-contour-footprints` Amendment 5, evidence §6e): the
/// route from (11.5, 12.5) to (6.5, 7.5) at radius 0.26 turns left round three collinear
/// corners, (11, 12), (8, 9) and (7, 8). The middle turn sweeps nothing, and the smooth length
/// once counted a whole circle for it (ratio 1.270); the route equals the oracle's.
#[test]
fn a_straight_pass_round_collinear_corners_sweeps_no_circle() -> Result<(), TestCaseError> {
    const MASK: [&str; 16] = [
        "......#....#....",
        ".#......##....#.",
        ".......##..##...",
        ".#.....#...#....",
        "..#.....##......",
        "#..#...#.....#..",
        ".##..#....#....#",
        "#....#.#....##..",
        ".....#..#......#",
        ".##..##.......#.",
        ".......#...#....",
        "#.#...#....##..#",
        "..#.............",
        "##...........#..",
        "..............#.",
        "#........#.#..##",
    ];
    let blocked: Vec<bool> = MASK
        .iter()
        .flat_map(|row| row.bytes().map(|c| c == b'#'))
        .collect();
    let map = build_map(&blocked)?;
    let radius = SimScalar::from_num(0.26);
    let rho = f64::from((radius + CORNER_SAFETY_MARGIN).to_f32());
    let walls = oracle::walls_of(&map);
    let tight = oracle::Oracle::new(walls.clone(), f64::from(radius.to_f32()));
    let padded = oracle::Oracle::new(walls, rho);
    let pair = (centre(11, 12), centre(6, 7));
    let mut pathfinder = Pathfinder::new(&map);
    let ratio = compare_with_oracle(&map, &mut pathfinder, &tight, &padded, pair, radius)
        .map_err(TestCaseError::fail)?;
    let Some(ratio) = ratio else {
        return Err(TestCaseError::fail(
            "the oracle and the mesh both find a route",
        ));
    };
    assert!((ratio - 1.0).abs() < 1e-9, "ratio {ratio}");
    let route = pathfinder
        .find_route_mesh(&map, &mesh_request(pair.0, pair.1, radius))
        .map_err(|error| TestCaseError::fail(error.to_string()))?;
    // Lattice points are in quarter cells.
    let corners: Vec<(i32, i32, Side)> = route
        .turns
        .iter()
        .map(|&(p, side)| (p.x, p.y, side))
        .collect();
    assert_eq!(
        corners,
        [
            (44, 48, Side::Left),
            (32, 36, Side::Left),
            (28, 32, Side::Left)
        ],
        "the case still turns round three collinear corners"
    );
    Ok(())
}

/// The legality law of `funnel_paths_are_legal_and_no_shorter_than_a_straight_line`, for one
/// route: legs from a wedged start are held to the predicate for the centre only (D-3).
fn route_is_legal(
    pathfinder: &mut Pathfinder,
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
    waypoints: &[SimVec2],
) -> bool {
    let centre_only = PathRequestWithVision {
        radius: SimScalar::ZERO,
        ..*request
    };
    let mut wedged = true;
    waypoints.windows(2).all(|pair| {
        wedged = wedged && !pathfinder.segment_is_passable(map, request, pair[0], pair[0]);
        let leg_request = if wedged { &centre_only } else { request };
        pathfinder.segment_is_passable(map, leg_request, pair[0], pair[1])
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The lane-restricted search (group-routing WP2): asked for the unrestricted route's own
    /// lane it returns exactly that route (H-4); asked for the lane of a detour through a third
    /// point, any route it returns carries that lane's word and is legal (or a counted fallback).
    #[test]
    fn restricted_routes_are_legal_and_carry_their_target_word(
        blocked in obstacle_mask(7),
        start in (0..SIDE, 0..SIDE),
        via in (0..SIDE, 0..SIDE),
        goal in (0..SIDE, 0..SIDE),
        tier_index in 0_u8..2,
    ) {
        let map = build_map(&blocked)?;
        let mut pathfinder = Pathfinder::new(&map);
        let radius = tier(tier_index);
        let (start, via, goal) = (centre(start.0, start.1), centre(via.0, via.1), centre(goal.0, goal.1));
        let request = mesh_request(start, goal, radius);
        let Ok(free) = pathfinder.find_path_with_vision(&map, &request) else {
            return Ok(());
        };
        let own = pathfinder.lane_word(&free.waypoints);
        prop_assert_eq!(pathfinder.find_path_in_lane(&map, &request, &own), Ok(free));
        let (Ok(first), Ok(second)) = (
            pathfinder.find_path_with_vision(&map, &mesh_request(start, via, radius)),
            pathfinder.find_path_with_vision(&map, &mesh_request(via, goal, radius)),
        ) else {
            return Ok(());
        };
        let mut detour = first.waypoints;
        detour.extend_from_slice(&second.waypoints[1..]);
        let lane = pathfinder.lane_word(&detour);
        let fallbacks = pathfinder.mesh_funnel_fallbacks();
        if let Ok(routed) = pathfinder.find_path_in_lane(&map, &request, &lane) {
            let fell_back = pathfinder.mesh_funnel_fallbacks() != fallbacks;
            prop_assert_eq!(pathfinder.lane_word(&routed.waypoints), lane);
            prop_assert_eq!(routed.waypoints.first(), Some(&start));
            prop_assert_eq!(routed.waypoints.last(), Some(&goal));
            prop_assert!(fell_back || route_is_legal(&mut pathfinder, &map, &request, &routed.waypoints));
        }
    }
}

/// Side of the maps of `free_vertices_carry_no_circle`, in cells.
const KNOWLEDGE_SIDE: u32 = 24;

/// A flat `KNOWLEDGE_SIDE`² map with the full-square structures `structures` (`(x, y, size,
/// known)`) placed where they fit, and the known-structure mask that knows the cells of the
/// placed structures marked known.
fn knowledge_map(
    structures: &[(u32, u32, u8, bool)],
) -> Result<(GameMap, Vec<bool>), TestCaseError> {
    let side = KNOWLEDGE_SIDE;
    let mut map = GameMap::new(side, side, MapCell::default_ground(CliffLevel(1)))
        .map_err(|error| TestCaseError::fail(error.to_string()))?;
    let mut known = vec![false; (side * side) as usize];
    for (id, &(x, y, size, is_known)) in (50_000_u32..).zip(structures) {
        let footprint = sim_map::StructureFootprint::square(size, size);
        // A refused placement leaves the map as it was.
        if map
            .place_structure(sim_types::EntityId(id), (x, y), footprint)
            .is_ok()
            && is_known
        {
            for cy in y..y + u32::from(size) {
                for cx in x..x + u32::from(size) {
                    known[(cy * side + cx) as usize] = true;
                }
            }
        }
    }
    Ok((map, known))
}

/// Whether the cell corner `(x, y)` lies at least `rho` from every obstacle of the request: the
/// map border and every cell of a known structure. Such a corner of an unknown structure is a
/// free vertex that no clamp or kept circle concerns (plan `2026-11-19-partial-knowledge-funnel`
/// D-2, D-3): it bounds nothing within the disc's clearance.
fn clear_of_obstacles(known: &[bool], (x, y): (i32, i32), rho: f64) -> bool {
    let side = f64::from(KNOWLEDGE_SIDE);
    let (px, py) = (f64::from(x), f64::from(y));
    let border = px.min(py).min(side - px).min(side - py);
    border >= rho
        && (0..KNOWLEDGE_SIDE * KNOWLEDGE_SIDE)
            .filter(|&i| known[i as usize])
            .all(|i| {
                let (cx, cy) = (f64::from(i % KNOWLEDGE_SIDE), f64::from(i / KNOWLEDGE_SIDE));
                let dx = (cx - px).max(0.0).max(px - (cx + 1.0));
                let dy = (cy - py).max(0.0).max(py - (cy + 1.0));
                dx.hypot(dy) >= rho
            })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Plan `2026-11-19-partial-knowledge-funnel` D-1: under partial knowledge (half the
    /// structures unknown) every route is legal or a counted fallback, and a turn at a free
    /// vertex that lies at least `rho` from every obstacle is a point bend: the vertex itself is
    /// a waypoint, once, where a rounded vertex's waypoints all lie on its circle.
    #[test]
    fn free_vertices_carry_no_circle(
        structures in vec((0_u32..KNOWLEDGE_SIDE - 4, 0_u32..KNOWLEDGE_SIDE - 4, 1_u8..5, proptest::bool::ANY), 2..10),
        start in (0..KNOWLEDGE_SIDE, 0..KNOWLEDGE_SIDE),
        goal in (0..KNOWLEDGE_SIDE, 0..KNOWLEDGE_SIDE),
        tier_index in 0_u8..3,
    ) {
        let (map, known) = knowledge_map(&structures)?;
        let mut pathfinder = Pathfinder::new(&map);
        let request = PathRequestWithVision {
            known_structures: Some(&known),
            ..mesh_request(centre(start.0, start.1), centre(goal.0, goal.1), tier(tier_index))
        };
        let Ok(route) = pathfinder.find_route_mesh(&map, &request) else {
            return Ok(());
        };
        prop_assert!(
            route.fallback || route_is_legal(&mut pathfinder, &map, &request, &route.waypoints),
            "illegal route {:?}", route
        );
        if route.fallback {
            return Ok(());
        }
        let rho = f64::from((request.radius + CORNER_SAFETY_MARGIN).to_f32());
        for &(corner, _) in &route.turns {
            let (x, y) = (corner.x, corner.y);
            if x % 4 != 0 || y % 4 != 0 || !clear_of_obstacles(&known, (x / 4, y / 4), rho) {
                continue;
            }
            let vertex = corner.to_sim();
            prop_assert_eq!(
                route.waypoints.iter().filter(|&&w| w == vertex).count(),
                1,
                "the free turn {:?} is not a point bend: {:?}", vertex, route
            );
        }
    }
}

/// Plan `2026-11-19-partial-knowledge-funnel` WP4 (§4.4's corpus at a test's scale): 40 seeded
/// layouts of the default arena with 2 to 13 full-square structures, each known or not as a
/// whole, and 16 requests each between cells where the disc fits. The oracle runs on a map that
/// holds only the known structures, which is the free space of the request. Every route from a
/// fitting start is legal with no fallback, none is shorter than the oracle's by more than 10⁻³,
/// at least 95 % are within 1 % of it, and none is beyond [`MESH_RATIO_BOUND`] (plan
/// `2026-11-20-tastar-budget` WP1: before its D-1, layout 25 reached 1.1343, B-149).
///
/// The oracle runs at `r - 10⁻⁵`, not at `r`: its float tolerance misses a passage exactly `2r`
/// wide (plan `2026-11-19-funnel-fallbacks` §4.3), and a hair less finds it. Routes keep the
/// corner margin, so they are compared with this margin-free oracle from below and by share.
#[test]
fn partial_knowledge_routes_agree_with_the_oracle() {
    let (mut within, mut total, mut worst) = (0u32, 0u32, 1.0f64);
    for number in 0..40_u64 {
        let layout = layouts::layout(number, LayoutKind::WHOLE_SQUARES);
        let map = &layout.map;
        let mut pathfinder = Pathfinder::new(map);
        for drawn in &layout.requests {
            let request = layout_request(&layout, drawn);
            let fits = |p: SimVec2, pathfinder: &mut Pathfinder| {
                pathfinder.segment_is_passable(map, &request, p, p)
            };
            if !fits(drawn.start, &mut pathfinder) || !fits(drawn.goal, &mut pathfinder) {
                continue;
            }
            let Ok(route) = pathfinder.find_route_mesh(map, &request) else {
                continue;
            };
            assert!(!route.fallback, "layout {number}: fallback {request:?}");
            assert!(route_is_legal(
                &mut pathfinder,
                map,
                &request,
                &route.waypoints
            ));
            let radius = f64::from(request.radius.to_f32());
            let oracle = oracle::Oracle::new(oracle::walls_of(&layout.known_map), radius - 1e-5);
            let Some(reference) = oracle.shortest(to_f64(drawn.start), to_f64(drawn.goal)) else {
                continue;
            };
            if reference <= 0.0 {
                continue;
            }
            let length = f64::from(route.total_length.to_f32());
            assert!(
                length >= reference - 1e-3,
                "layout {number}: {length} shorter than the oracle's {reference}: {request:?}"
            );
            let ratio = length / reference;
            total += 1;
            within += u32::from(ratio <= 1.01);
            worst = worst.max(ratio);
        }
    }
    eprintln!(
        "partial knowledge against the oracle: {within}/{total} within 1 %, worst {worst:.4}"
    );
    assert!(total >= 300, "only {total} routes compared");
    assert!(
        f64::from(within) >= 0.95 * f64::from(total),
        "only {within}/{total} within 1 %"
    );
    assert!(
        worst <= MESH_RATIO_BOUND,
        "worst ratio {worst:.4} beyond {MESH_RATIO_BOUND}"
    );
}

/// Plan `2026-11-20-tastar-budget` WP5: the per-cell half of
/// `partial_knowledge_routes_agree_with_the_oracle`. The same 40 layouts of full squares, each
/// cell known or not, and their requests between cells where the disc fits. The oracle runs on
/// the arena with every known cell as a one-cell structure. The plan does not hold this oracle
/// exact under per-cell knowledge (it was never validated there; worst ratio 1.0309 on this
/// corpus), so the test holds only what a lower bound can: no route is shorter than it by more
/// than 10⁻³, and no route from a fitting start falls back, except per-cell layout 8 q 2, the
/// partial-knowledge funnel's class N (a free anchor beside a known face with no room for the
/// disc; no channel has a legal pull, plan `2026-11-20-tastar-budget` §9).
#[test]
fn partial_knowledge_routes_agree_with_the_oracle_per_cell() {
    let (mut total, mut worst, mut fallbacks) = (0u32, 1.0f64, Vec::new());
    for number in 0..40_u64 {
        let layout = layouts::layout(number, LayoutKind::PER_CELL_SQUARES);
        let map = &layout.map;
        let mut known_cells = arena::default_arena();
        for (index, &known) in (0_u32..).zip(&layout.known) {
            let cell = (index % map.width(), index / map.width());
            let id = sim_types::EntityId(60_000 + index);
            assert!(
                !known
                    || known_cells
                        .place_structure(id, cell, sim_map::StructureFootprint::square(1, 1))
                        .is_ok(),
                "layout {number}: known cell {cell:?} is not open ground"
            );
        }
        let mut pathfinder = Pathfinder::new(map);
        for (q, drawn) in layout.requests.iter().enumerate() {
            let request = layout_request(&layout, drawn);
            let fits = |p: SimVec2, pathfinder: &mut Pathfinder| {
                pathfinder.segment_is_passable(map, &request, p, p)
            };
            if !fits(drawn.start, &mut pathfinder) || !fits(drawn.goal, &mut pathfinder) {
                continue;
            }
            let Ok(route) = pathfinder.find_route_mesh(map, &request) else {
                continue;
            };
            if route.fallback {
                fallbacks.push((number, q));
                continue;
            }
            assert!(route_is_legal(
                &mut pathfinder,
                map,
                &request,
                &route.waypoints
            ));
            let radius = f64::from(request.radius.to_f32());
            let oracle = oracle::Oracle::new(oracle::walls_of(&known_cells), radius - 1e-5);
            let Some(reference) = oracle.shortest(to_f64(drawn.start), to_f64(drawn.goal)) else {
                continue;
            };
            if reference <= 0.0 {
                continue;
            }
            let length = f64::from(route.total_length.to_f32());
            assert!(
                length >= reference - 1e-3,
                "layout {number} q {q}: {length} shorter than the oracle's {reference}: {request:?}"
            );
            total += 1;
            worst = worst.max(length / reference);
        }
    }
    eprintln!("per-cell knowledge against the oracle: {total} routes, worst ratio {worst:.4}, fallbacks {fallbacks:?}");
    assert!(total >= 300, "only {total} routes compared");
    assert!(
        fallbacks.iter().all(|&case| case == (8, 2)),
        "fallbacks from fitting starts: {fallbacks:?}"
    );
}

/// The mesh request of a drawn layout request, under the layout's knowledge.
fn layout_request<'a>(
    layout: &'a layouts::Layout,
    drawn: &layouts::LayoutRequest,
) -> PathRequestWithVision<'a> {
    PathRequestWithVision {
        known_structures: Some(&layout.known),
        ..mesh_request(drawn.start, drawn.goal, drawn.radius)
    }
}

/// One B-149 case of plan `2026-11-20-tastar-budget` §3.3: layout `number`, request `q`, of
/// kind `kind`, checked against the endpoints and radius the plan names. Returns the layout and
/// the route, whose legs pass the segment predicate unless it fell back.
///
/// # Errors
/// The route's error, when the mesh finds none.
fn b149_case(
    (number, q, kind): (u64, usize, LayoutKind),
    (start, goal, radius): ((f32, f32), (f32, f32), f32),
) -> Result<(layouts::Layout, sim_map::MeshRoute), MapError> {
    let layout = layouts::layout(number, kind);
    let drawn = layout.requests[q];
    assert_eq!(
        (
            (drawn.start.x.to_f32(), drawn.start.y.to_f32()),
            (drawn.goal.x.to_f32(), drawn.goal.y.to_f32()),
            drawn.radius.to_f32()
        ),
        (start, goal, radius),
        "layout {number} q {q} is not the plan's request"
    );
    let request = layout_request(&layout, &drawn);
    let mut pathfinder = Pathfinder::new(&layout.map);
    let route = pathfinder.find_route_mesh(&layout.map, &request)?;
    assert!(
        route.fallback || route_is_legal(&mut pathfinder, &layout.map, &request, &route.waypoints),
        "layout {number} q {q}: an illegal leg: {route:?}"
    );
    Ok((layout, route))
}

/// Whether `length` is within a factor of 1.001 of `expected`, either way.
fn within_a_thousandth(length: SimScalar, expected: f64) -> bool {
    let length = f64::from(length.to_f32());
    length <= expected * 1.001 && length >= expected / 1.001
}

/// T1 of plan `2026-11-20-tastar-budget` (B-149): across an unknown 5 × 5 the straight channel is
/// found. On `main` the route turns at the free corners (48, 38), (49, 41) and (49, 43) at
/// 57.104 (1.1343 of the oracle): the anytime budget ran out 1,136 pops before the straight
/// channel. Every turn of the route is on a wall of the known structures or the terrain.
#[test]
fn the_straight_channel_across_an_unknown_footprint_is_found() -> Result<(), MapError> {
    let (layout, route) = b149_case(
        (25, 11, LayoutKind::WHOLE_SQUARES),
        ((39.5, 11.5), (48.5, 56.5), 0.3125),
    )?;
    assert!(!route.fallback, "{route:?}");
    assert!(
        within_a_thousandth(route.total_length, 50.382),
        "length {} is not 50.382: {route:?}",
        route.total_length.to_f32()
    );
    let walls = oracle::walls_of(&layout.known_map);
    for &(corner, _) in &route.turns {
        let p = to_f64(corner.to_sim());
        assert!(
            walls
                .iter()
                .any(|&(a, b)| oracle::point_segment(p, a, b) < 1e-9),
            "the route turns at the free vertex {p:?}: {route:?}"
        );
    }
    Ok(())
}

/// T2 of plan `2026-11-20-tastar-budget` (B-149): the visit cap keeps the straight channel at
/// its state once the channel bound orders the open list. On `main` 50.904, a point bend at the
/// free (44, 19); the budget is not the cause (unbounded, the open list empties at 50.904).
#[test]
fn a_capped_state_keeps_the_straight_channel() -> Result<(), MapError> {
    let (_, route) = b149_case(
        (92, 13, LayoutKind::WHOLE_SQUARES),
        ((28.5, 49.5), (49.5, 4.5), 0.5),
    )?;
    assert!(!route.fallback, "{route:?}");
    assert!(
        within_a_thousandth(route.total_length, 50.263),
        "length {} is not 50.263: {route:?}",
        route.total_length.to_f32()
    );
    Ok(())
}

/// T3 of plan `2026-11-20-tastar-budget` (B-149): per-cell squares layout 19 q 8, a wedged
/// start of radius 1.0 beside unknown cells, takes the channel south of a structure. On `main`
/// 26.195 (19.951 before B-146's fix, the partial-knowledge funnel's 31 % observation).
#[test]
fn a_wedged_start_beside_unknown_cells_takes_the_short_channel() -> Result<(), MapError> {
    let layout = layouts::layout(19, LayoutKind::PER_CELL_SQUARES);
    let drawn = layout.requests[8];
    assert_eq!(drawn.radius.to_f32(), 1.0);
    let request = layout_request(&layout, &drawn);
    let mut pathfinder = Pathfinder::new(&layout.map);
    assert!(
        !pathfinder.segment_is_passable(&layout.map, &request, drawn.start, drawn.start),
        "the start is not wedged"
    );
    let route = pathfinder.find_route_mesh(&layout.map, &request)?;
    assert!(
        within_a_thousandth(route.total_length, 19.876),
        "length {} is not 19.876: {route:?}",
        route.total_length.to_f32()
    );
    Ok(())
}

/// T7 of plan `2026-11-20-tastar-budget` (B-149): the anytime loop runs until no open channel's
/// bound is below the incumbent (S11's rule). On `main` 76.571, a point bend at the free
/// (45, 12): the first route comes after 97 expansions, and the better channel needs 1,041 more
/// pops than the budget's 256, with the channel bound or without it.
#[test]
fn the_anytime_loop_runs_to_the_incumbent() -> Result<(), MapError> {
    let (_, route) = b149_case(
        (113, 1, LayoutKind::WHOLE_SQUARES),
        ((13.5, 61.5), (59.5, 2.5), 1.0),
    )?;
    assert!(!route.fallback, "{route:?}");
    assert!(
        within_a_thousandth(route.total_length, 75.372),
        "length {} is not 75.372: {route:?}",
        route.total_length.to_f32()
    );
    Ok(())
}

/// Plan `2026-11-20-tastar-budget` D-3: per-cell contours layout 58 q 13 (the partial-knowledge
/// funnel's accepted residual) has a channel with a legal pull, so it does not fall back to the
/// portal midpoints. On `main` no pull of the channel TA* returns is legal, and it falls back.
#[test]
fn a_legal_channel_is_sought_before_the_midpoints() -> Result<(), MapError> {
    let (_, route) = b149_case(
        (58, 13, LayoutKind::PER_CELL_CONTOURS),
        ((42.5, 44.5), (52.5, 7.5), 0.9375),
    )?;
    assert!(!route.fallback, "{route:?}");
    assert!(
        within_a_thousandth(route.total_length, 50.403),
        "length {} is not 50.403: {route:?}",
        route.total_length.to_f32()
    );
    Ok(())
}

/// Plan `2026-11-20-tastar-budget` D-3: per-cell contours layout 92 q 12. With the channel bound
/// the first channel's pull is illegal and shorter than every other channel's bound, so it ends
/// the search, the retries fail and the request would fall back; the legal-only search finds a
/// legal channel of 70.863. On `main` the route is 69.905, so the length is held from above
/// only: no fallback, and no longer than the legal-only search's route.
#[test]
fn an_illegal_first_pull_does_not_end_the_search() -> Result<(), MapError> {
    let (_, route) = b149_case(
        (92, 12, LayoutKind::PER_CELL_CONTOURS),
        ((60.5, 13.5), (60.5, 52.5), 1.0),
    )?;
    assert!(!route.fallback, "{route:?}");
    assert!(
        f64::from(route.total_length.to_f32()) <= 70.863 * 1.001,
        "length {} is beyond 70.863: {route:?}",
        route.total_length.to_f32()
    );
    Ok(())
}
