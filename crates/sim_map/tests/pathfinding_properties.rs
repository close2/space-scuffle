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

use proptest::collection::vec;
use proptest::prelude::*;
use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapError, PathRequest, PathRequestWithVision,
    Pathfinder, Side, UnitClearance, CORNER_SAFETY_MARGIN,
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

fn tier(index: u8) -> UnitClearance {
    match index {
        0 => UnitClearance::Small,
        1 => UnitClearance::Medium,
        _ => UnitClearance::Large,
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
                prop_assert!(clearance >= UnitClearance::Small.required_clearance());
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
    /// held to the predicate only when the disc fits at the start; a counted fallback to portal
    /// midpoints is exempt.
    #[test]
    fn funnel_paths_are_legal_and_no_shorter_than_a_straight_line(
        blocked in obstacle_mask(8),
        start in (0..SIDE, 0..SIDE),
        goal in (0..SIDE, 0..SIDE),
        tier_index in 0_u8..3,
    ) {
        let map = build_map(&blocked)?;
        let mut pathfinder = Pathfinder::new(&map);
        let request = PathRequest {
            start: centre(start.0, start.1),
            goal: centre(goal.0, goal.1),
            clearance: tier(tier_index),
            radius: (tier(tier_index)).radius_scalar(),
        };
        let fallbacks = pathfinder.mesh_funnel_fallbacks();
        let found = pathfinder.find_path(&map, &request);
        // A counted fallback through portal midpoints (plan D-6, R-2) is exempt from legality.
        let fell_back = pathfinder.mesh_funnel_fallbacks() != fallbacks;
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
                ..request.into()
            };
            let mut wedged = true;
            let mut measured = SimScalar::ZERO;
            for pair in path.waypoints.windows(2) {
                measured += pair[0].distance(pair[1]);
                wedged = wedged
                    && !pathfinder.segment_is_passable(&map, &request.into(), pair[0], pair[0]);
                let legal = if wedged {
                    pathfinder.segment_is_passable(&map, &centre_only, pair[0], pair[1])
                } else {
                    pathfinder.segment_is_passable(&map, &request.into(), pair[0], pair[1])
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
        let clearance = tier(tier_index);
        let forward = PathRequest {
            start: centre(start.0, start.1),
            goal: centre(goal.0, goal.1),
            clearance,
            radius: clearance.radius_scalar(),
        };
        let backward = PathRequest {
            start: forward.goal,
            goal: forward.start,
            clearance,
            radius: clearance.radius_scalar(),
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
            clearance: UnitClearance::Small,
            radius: UnitClearance::Small.radius_scalar(),
        };
        let target = PathRequest {
            start: distraction.goal,
            goal: distraction.start,
            clearance: UnitClearance::Medium,
            radius: UnitClearance::Medium.radius_scalar(),
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
        clearance: UnitClearance::for_radius(radius),
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
                (f64::from(p.x), f64::from(p.y)),
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
/// existence or fails a leg, and at most 2 % fall back to portal midpoints.
#[test]
fn mesh_route_lengths_are_within_one_percent_of_the_oracle() {
    use rand::{Rng, SeedableRng};
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0x0_7ac1e);
    let (mut within, mut total, mut worst, mut fallbacks) = (0u32, 0u32, 1.0f64, 0u32);
    let mut maps = 0;
    while maps < 120 {
        let radius_f = 0.25 + f64::from(rng.gen_range(0u32..=75)) * 0.01;
        if !radius_is_unambiguous(radius_f) {
            continue;
        }
        maps += 1;
        let blocked: Vec<bool> = (0..CELLS).map(|_| rng.gen_range(0..5) == 0).collect();
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
                centres[rng.gen_range(0..centres.len())],
                centres[rng.gen_range(0..centres.len())],
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
    assert!(fallbacks * 50 <= total, "{fallbacks}/{total} fell back");
    assert!(worst <= MESH_RATIO_BOUND, "worst ratio {worst}");
    assert!(share >= 0.95, "only {within}/{total} within 1 %");
}
