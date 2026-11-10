//! Integration tests for Simple Stupid Funnel (String-Pulling) algorithm,
//! Euclidean shortcuts around L-shaped obstacles, and corner radius offsets.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/arena.rs"]
mod arena;
#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/oracle.rs"]
mod oracle;

use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapError, MapFile, PathRequest, PathRequestWithVision,
    Pathfinder, Side, UnitClearance, CORNER_SAFETY_MARGIN,
};
use sim_types::{EntityId, SimScalar, SimVec2};

#[test]
fn test_straight_line_optimization() {
    let width = 30u32;
    let height = 30u32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(1))).unwrap();
    let mut pathfinder = Pathfinder::new(&map);

    let start = SimVec2::new(SimScalar::from_num(2), SimScalar::from_num(2));
    let goal = SimVec2::new(SimScalar::from_num(25), SimScalar::from_num(20));

    let request = PathRequest {
        start,
        goal,
        clearance: UnitClearance::Medium,
        radius: UnitClearance::Medium.radius_scalar(),
    };

    let result = pathfinder.find_path(&map, &request).unwrap();

    // Straight open path must collapse down to exactly 2 waypoints: [Start, Goal]
    assert_eq!(
        result.waypoints.len(),
        2,
        "Path should contain exactly Start and Goal: {:?}",
        result.waypoints
    );
    assert_eq!(result.waypoints[0], start);
    assert_eq!(result.waypoints[1], goal);

    let expected_dist = start.distance(goal);
    assert_eq!(result.total_length, expected_dist);
}

#[test]
fn test_l_shaped_obstacle_euclidean_shortcut() {
    let width = 20u32;
    let height = 20u32;
    let mut map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(1))).unwrap();

    // Construct L-shaped impassable obstacle blocking direct line between (2, 2) and (10, 10):
    // Vertical stem: x in 5..=6, y in 0..=6
    for y in 0..=6 {
        for x in 5..=6 {
            map.cell_mut(x, y).unwrap().flags = CellFlags::UNPASSABLE_CLIFF;
        }
    }
    // Horizontal base extending East: x in 7..=9, y in 2..=3
    for y in 2..=3 {
        for x in 7..=9 {
            map.cell_mut(x, y).unwrap().flags = CellFlags::UNPASSABLE_CLIFF;
        }
    }

    let mut pathfinder = Pathfinder::new(&map);

    let start = SimVec2::new(SimScalar::from_num(2), SimScalar::from_num(2));
    let goal = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10));

    let request = PathRequest {
        start,
        goal,
        clearance: UnitClearance::Medium,
        radius: UnitClearance::Medium.radius_scalar(),
    };

    let result = pathfinder.find_path(&map, &request).unwrap();

    // The string-pulled path must not contain grid staircase zig-zags
    // It must contain exactly 3 waypoints: [Start, CornerWaypoint, Goal]
    assert_eq!(
        result.waypoints.len(),
        3,
        "String-pulled path around L-corner must have exactly 3 waypoints: {:?}",
        result.waypoints
    );
    assert_eq!(result.waypoints[0], start);
    assert_eq!(result.waypoints[2], goal);

    // Corner waypoint must be near obstacle corner (5, 7)
    let corner_wp = result.waypoints[1];
    let corner_vertex = SimVec2::new(SimScalar::from_num(5), SimScalar::from_num(7));
    let offset_dist = (corner_wp - corner_vertex).length();

    let expected_offset = UnitClearance::Medium.radius_scalar() + CORNER_SAFETY_MARGIN;
    let diff = (offset_dist - expected_offset).abs();
    assert!(
        diff < SimScalar::from_num(0.01),
        "Corner waypoint offset ({:?}) must match radius + margin ({:?})",
        offset_dist,
        expected_offset
    );

    // Confirm that a circle of unit radius centered at waypoint does not intersect the obstacle corner
    let dist_to_corner = corner_wp.distance(corner_vertex);
    assert!(
        dist_to_corner >= UnitClearance::Medium.radius_scalar(),
        "Waypoint must maintain safe clearance from obstacle corner"
    );
}

#[test]
fn test_corner_clearance_radius_offset_scaling() {
    let width = 20u32;
    let height = 20u32;
    let mut map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(1))).unwrap();

    // Single corner obstacle block: x in 5..=6, y in 0..=6
    for y in 0..=6 {
        for x in 5..=6 {
            map.cell_mut(x, y).unwrap().flags = CellFlags::UNPASSABLE_CLIFF;
        }
    }

    let mut pathfinder = Pathfinder::new(&map);
    let start = SimVec2::new(SimScalar::from_num(2), SimScalar::from_num(2));
    let goal = SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(10));

    // Test Small vs Medium clearance radius offsets
    let req_small = PathRequest {
        start,
        goal,
        clearance: UnitClearance::Small,
        radius: UnitClearance::Small.radius_scalar(),
    };
    let req_medium = PathRequest {
        start,
        goal,
        clearance: UnitClearance::Medium,
        radius: UnitClearance::Medium.radius_scalar(),
    };

    let path_small = pathfinder.find_path(&map, &req_small).unwrap();
    let path_medium = pathfinder.find_path(&map, &req_medium).unwrap();

    let corner_vertex = SimVec2::new(SimScalar::from_num(5), SimScalar::from_num(7));

    let dist_small = (path_small.waypoints[1] - corner_vertex).length();
    let dist_medium = (path_medium.waypoints[1] - corner_vertex).length();

    let expected_small = UnitClearance::Small.radius_scalar() + CORNER_SAFETY_MARGIN;
    let expected_medium = UnitClearance::Medium.radius_scalar() + CORNER_SAFETY_MARGIN;

    assert!((dist_small - expected_small).abs() < SimScalar::from_num(0.01));
    assert!((dist_medium - expected_medium).abs() < SimScalar::from_num(0.01));

    // Medium waypoint must be placed farther outward from the corner than Small waypoint
    assert!(dist_medium > dist_small);
}

#[test]
fn test_sandbox_all_blue_units_direct_path_no_errant_detour() {
    let mut map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0))).unwrap();

    // Blue structures
    map.place_structure(EntityId(23), 5, 5, 5, 5).unwrap();
    map.place_structure(EntityId(24), 12, 5, 2, 2).unwrap();
    map.place_structure(EntityId(25), 16, 5, 3, 3).unwrap();

    // Red structures
    map.place_structure(EntityId(26), 54, 54, 5, 5).unwrap();
    map.place_structure(EntityId(27), 48, 54, 2, 2).unwrap();
    map.place_structure(EntityId(28), 42, 54, 3, 3).unwrap();

    let mut pathfinder = Pathfinder::new(&map);
    let goal = SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(32));

    // 10 Light Ranged units at x = 15, y in {23, 25, 27, 29, 31, 33, 35, 37, 39, 41}
    for i in 0..10i32 {
        let y_offset = i * 2 - 9;
        let start = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32 + y_offset));
        let request = PathRequest {
            start,
            goal,
            clearance: UnitClearance::Small,
            radius: UnitClearance::Small.radius_scalar(),
        };
        let result = pathfinder
            .find_path(&map, &request)
            .expect("path should exist");
        assert_eq!(
            result.waypoints,
            vec![start, goal],
            "Light unit at {:?} should path directly to goal",
            start
        );
    }

    // 2 Heavy Armored units at (12, 30) and (12, 34) (including EntityId(11) at (12, 30))
    for &y in &[30i32, 34i32] {
        let start = SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(y));
        let request = PathRequest {
            start,
            goal,
            clearance: UnitClearance::Medium,
            radius: UnitClearance::Medium.radius_scalar(),
        };
        let result = pathfinder
            .find_path(&map, &request)
            .expect("path should exist");
        assert_eq!(
            result.waypoints,
            vec![start, goal],
            "Heavy unit at {:?} must path directly to {:?} without errant detour",
            start,
            goal
        );
        for wp in &result.waypoints {
            assert!(
                wp.y <= SimScalar::from_num(44),
                "Waypoint {:?} unexpectedly detoured North towards Red structures",
                wp
            );
        }
    }
}

#[test]
fn test_showcase_canyon_ramp_funnel_paths_are_concise() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/maps/showcase_canyon.scmap.ron");
    let map = MapFile::load(&path)?.decode()?.to_game_map()?;
    let mut pathfinder = Pathfinder::new(&map);

    // Ramp 1: Low-ground (32.0, 52.5) to High-ground (41.0, 52.5)
    let r1_start = SimVec2::new(
        SimScalar::from_f32_lossy(32.0),
        SimScalar::from_f32_lossy(52.5),
    );
    let r1_goal = SimVec2::new(
        SimScalar::from_f32_lossy(41.0),
        SimScalar::from_f32_lossy(52.5),
    );
    let r1_req = PathRequest {
        start: r1_start,
        goal: r1_goal,
        clearance: UnitClearance::Small,
        radius: UnitClearance::Small.radius_scalar(),
    };
    let r1_path = pathfinder.find_path(&map, &r1_req)?;
    assert!(
        r1_path.waypoints.len() <= 4,
        "Ramp 1 path must be concise (<= 4 waypoints, got {:?})",
        r1_path.waypoints
    );

    // Ramp 2: Low-ground (32.0, 11.5) to High-ground (23.0, 11.5)
    let r2_start = SimVec2::new(
        SimScalar::from_f32_lossy(32.0),
        SimScalar::from_f32_lossy(11.5),
    );
    let r2_goal = SimVec2::new(
        SimScalar::from_f32_lossy(23.0),
        SimScalar::from_f32_lossy(11.5),
    );
    let r2_req = PathRequest {
        start: r2_start,
        goal: r2_goal,
        clearance: UnitClearance::Small,
        radius: UnitClearance::Small.radius_scalar(),
    };
    let r2_path = pathfinder.find_path(&map, &r2_req)?;
    assert!(
        r2_path.waypoints.len() <= 4,
        "Ramp 2 path must be concise (<= 4 waypoints, got {:?})",
        r2_path.waypoints
    );

    Ok(())
}

/// Plan 2026-11-09-sc2-pathfinding WP4 (B-101): a corner waypoint keeps the requesting unit's
/// own radius plus `CORNER_SAFETY_MARGIN` from the corner it turns around: 0.55 for a body of
/// radius 0.5 and 0.425 for one of 0.375, though both route on the Small tier.
#[test]
fn corner_offsets_use_the_units_own_radius() {
    let mut map = GameMap::new(20, 20, MapCell::default_ground(CliffLevel(1))).unwrap();
    for y in 0..10 {
        for x in 0..10 {
            map.cell_mut(x, y).unwrap().flags = CellFlags::UNPASSABLE_CLIFF;
        }
    }
    let vertex = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10));
    for (radius, expected) in [(0.5_f64, 0.55_f64), (0.375, 0.425)] {
        let mut pathfinder = Pathfinder::new(&map);
        let request = PathRequest {
            start: SimVec2::new(SimScalar::from_num(2.5), SimScalar::from_num(12.5)),
            goal: SimVec2::new(SimScalar::from_num(12.5), SimScalar::from_num(2.5)),
            clearance: UnitClearance::Small,
            radius: SimScalar::from_num(radius),
        };
        let path = pathfinder.find_path(&map, &request).unwrap();
        let corners: Vec<SimVec2> = path
            .waypoints
            .iter()
            .copied()
            .filter(|&w| {
                w != request.start
                    && w != request.goal
                    && w.distance(vertex) < SimScalar::from_num(2)
            })
            .collect();
        assert_eq!(
            corners.len(),
            1,
            "one corner near the vertex: {:?}",
            path.waypoints
        );
        let gap = f64::from(corners[0].distance(vertex).to_f32());
        assert!(
            (gap - expected).abs() < 1e-4,
            "radius {radius}: corner {:?} is {gap} from the vertex, expected {expected}",
            corners[0]
        );
    }
}

// --- Navigation mesh routes (sc2 pathfinding WP6): TA*, Demyen widths and the radius funnel.

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

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn f64_of(p: SimVec2) -> (f64, f64) {
    (f64::from(p.x.to_f32()), f64::from(p.y.to_f32()))
}

/// Routes `start -> goal` on the mesh and checks it against the exact oracle: every leg passes
/// the segment predicate, the route's tangent-and-arc length is within 1 % of the oracle's, and
/// its polyline is no longer than its tangent-and-arc length (arcs are emitted on their circle,
/// plan Amendment 2). Returns the polyline length and the oracle's length at
/// `r + margin / 2`, the closest any leg may come to a vertex it turns around.
fn assert_mesh_route_matches_oracle(
    map: &GameMap,
    start: SimVec2,
    goal: SimVec2,
    radius: f64,
) -> (f64, f64) {
    let radius = SimScalar::from_num(radius);
    let rho = f64::from((radius + CORNER_SAFETY_MARGIN).to_f32());
    let mut pathfinder = Pathfinder::new(map);
    let request = mesh_request(start, goal, radius);
    let route = pathfinder
        .find_route_mesh(map, &request)
        .expect("a route exists");
    assert!(!route.fallback, "funnel fell back: {route:?}");
    for pair in route.waypoints.windows(2) {
        assert!(
            pathfinder.mesh_segment_is_passable(map, &request, pair[0], pair[1]),
            "leg {pair:?}"
        );
    }
    let oracle = oracle::Oracle::new(oracle::walls_of(map), rho);
    let reference = oracle
        .shortest(f64_of(start), f64_of(goal))
        .expect("the oracle finds a route");
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
    let smooth = oracle::smooth_length(f64_of(start), f64_of(goal), &turns, rho);
    assert!(
        smooth <= reference * 1.01,
        "route {smooth} against oracle {reference}: {route:?}"
    );
    let polyline = f64::from(route.total_length.to_f32());
    assert!(
        polyline <= smooth + 1e-3,
        "polyline {polyline} against arcs {smooth}"
    );
    let dipped = f64::from((radius + CORNER_SAFETY_MARGIN / 2).to_f32());
    let floor = oracle::Oracle::new(oracle::walls_of(map), dipped)
        .shortest(f64_of(start), f64_of(goal))
        .expect("the oracle finds a route");
    (polyline, floor)
}

#[test]
fn mesh_route_rounds_an_l_shaped_wall_at_the_oracle_length() {
    let mut map = GameMap::new(20, 20, MapCell::default_ground(CliffLevel(1))).unwrap();
    for i in 5..15 {
        map.cell_mut(i, 10).unwrap().flags = CellFlags::empty();
        map.cell_mut(14, i).unwrap().flags = CellFlags::empty();
    }
    for radius in [0.25, 0.375, 0.5, 0.875] {
        let (length, reference) =
            assert_mesh_route_matches_oracle(&map, at(10.5, 5.5), at(10.5, 15.5), radius);
        assert!(
            length >= reference - 1e-3,
            "radius {radius}: {length} shorter than {reference}"
        );
    }
}

#[test]
fn mesh_route_climbs_the_default_arena_ramp_at_the_oracle_length() {
    let map = arena::default_arena();
    assert_mesh_route_matches_oracle(&map, at(10.5, 20.5), at(45.5, 50.5), 0.375);
    assert_mesh_route_matches_oracle(&map, at(31.5, 12.5), at(32.5, 40.5), 0.5);
}

/// The §3.4 pair of the plan, where the grid corridor was 2.7 % longer than the reference.
#[test]
fn mesh_route_for_the_showcase_pair_is_at_the_oracle_length() {
    let map = arena::load_game_map(arena::SHOWCASE_CANYON_FILE);
    for radius in [0.375, 0.5] {
        assert_mesh_route_matches_oracle(&map, at(61.5, 24.5), at(16.5, 35.5), radius);
    }
}

/// A unit standing just south of ramp E's side wall, its body touching it, goes round the wall's
/// western end, up the ramp and out of its eastern side: the route wraps the corner (35, 52) by
/// more than a half turn. The shrunk funnel loses that turn; the pinned retry keeps it, so the
/// route is legal and no fallback to portal midpoints is needed (plan Amendment 2; WP8's worst
/// pair, 3.08 times the grid's length through the midpoints).
#[test]
fn mesh_route_wraps_the_end_of_a_thin_wall_without_falling_back() {
    let map = arena::load_game_map(arena::SHOWCASE_CANYON_FILE);
    let (start, goal) = (at(36.5, 51.5), at(43.5, 49.5));
    let radius = SimScalar::HALF;
    let mut pathfinder = Pathfinder::new(&map);
    let request = mesh_request(start, goal, radius);
    let route = pathfinder
        .find_route_mesh(&map, &request)
        .expect("a route exists");
    assert!(!route.fallback, "funnel fell back: {route:?}");
    for pair in route.waypoints.windows(2) {
        assert!(
            pathfinder.mesh_segment_is_passable(&map, &request, pair[0], pair[1]),
            "leg {pair:?}"
        );
    }
    let corners: Vec<(i32, i32, Side)> = route.turns.iter().map(|&(p, s)| (p.x, p.y, s)).collect();
    assert_eq!(corners, [(35, 52, Side::Right), (38, 52, Side::Right)]);
    let legal = oracle::Oracle::new(oracle::walls_of(&map), 0.5)
        .shortest(f64_of(start), f64_of(goal))
        .expect("the oracle finds a route");
    let length = f64::from(route.total_length.to_f32());
    assert!(
        length >= legal - 1e-3 && length <= legal * 1.02,
        "route {length} against the legal minimum {legal}"
    );
}

/// A wall across the map with one opening of `gap` cells at `x = 8`.
fn wall_with_gap(gap: u32) -> GameMap {
    let mut map = GameMap::new(20, 12, MapCell::default_ground(CliffLevel(1))).unwrap();
    for x in (0..20).filter(|&x| !(8..8 + gap).contains(&x)) {
        map.cell_mut(x, 6).unwrap().flags = CellFlags::empty();
    }
    map
}

#[test]
fn mesh_gap_of_exactly_two_radii_passes_and_a_hair_less_does_not() {
    let (start, goal) = (at(8.5, 2.5), at(8.5, 10.5));
    for (gap, radius) in [(1, SimScalar::HALF), (2, SimScalar::ONE)] {
        let map = wall_with_gap(gap);
        let mut pathfinder = Pathfinder::new(&map);
        let fits = pathfinder.find_route_mesh(&map, &mesh_request(start, goal, radius));
        assert!(
            fits.is_ok(),
            "gap {gap} must admit radius {radius}: {fits:?}"
        );
        let wider = radius + SimScalar::EPSILON;
        let blocked = pathfinder.find_route_mesh(&map, &mesh_request(start, goal, wider));
        assert!(
            matches!(blocked, Err(MapError::NoPathFound { .. })),
            "gap {gap} must refuse radius {wider}: {blocked:?}"
        );
    }
}

#[test]
fn mesh_diagonal_gap_is_exact() {
    // Row 5 is blocked for x < 6 and row 7 for x >= 7: the only way up passes between the
    // corners (6, 6) and (7, 7), sqrt(2) = 1.41421 apart.
    let mut map = GameMap::new(14, 12, MapCell::default_ground(CliffLevel(1))).unwrap();
    for x in 0..6 {
        map.cell_mut(x, 5).unwrap().flags = CellFlags::empty();
    }
    for x in 7..14 {
        map.cell_mut(x, 7).unwrap().flags = CellFlags::empty();
    }
    for y in 0..12 {
        if y != 5 && y != 6 && y != 7 {
            continue;
        }
        map.cell_mut(13, y).unwrap().flags = CellFlags::empty();
    }
    let (start, goal) = (at(10.5, 2.5), at(2.5, 10.5));
    let mut pathfinder = Pathfinder::new(&map);
    let fits = pathfinder.find_route_mesh(
        &map,
        &mesh_request(start, goal, SimScalar::from_num(7071) / 10_000),
    );
    assert!(fits.is_ok(), "{fits:?}");
    let blocked = pathfinder.find_route_mesh(
        &map,
        &mesh_request(start, goal, SimScalar::from_num(7072) / 10_000),
    );
    assert!(
        matches!(blocked, Err(MapError::NoPathFound { .. })),
        "{blocked:?}"
    );
}

#[test]
fn mesh_structures_touching_at_a_corner_block_every_radius_until_unknown() {
    // A barrier of column 7 above y = 9 and column 8 below y = 7, closed by two one-cell
    // structures at (7, 8) and (8, 7) that touch only at the point (8, 8).
    let mut map = GameMap::new(18, 16, MapCell::default_ground(CliffLevel(1))).unwrap();
    for y in 9..16 {
        map.cell_mut(7, y).unwrap().flags = CellFlags::empty();
    }
    for y in 0..7 {
        map.cell_mut(8, y).unwrap().flags = CellFlags::empty();
    }
    map.place_structure(EntityId(1), 7, 8, 1, 1).unwrap();
    map.place_structure(EntityId(2), 8, 7, 1, 1).unwrap();
    let mut pathfinder = Pathfinder::new(&map);
    let (start, goal) = (at(3.5, 7.5), at(14.5, 7.5));
    for radius in [0.05, 0.25, 0.375, 0.5] {
        let result = pathfinder.find_route_mesh(
            &map,
            &mesh_request(start, goal, SimScalar::from_num(radius)),
        );
        assert!(
            matches!(result, Err(MapError::NoPathFound { .. })),
            "radius {radius}: {result:?}"
        );
    }
    let unknown = vec![false; 18 * 16];
    let request = PathRequestWithVision {
        known_structures: Some(&unknown),
        ..mesh_request(start, goal, SimScalar::from_num(0.375))
    };
    assert!(
        pathfinder.find_route_mesh(&map, &request).is_ok(),
        "unknown structures are walked through"
    );
}
