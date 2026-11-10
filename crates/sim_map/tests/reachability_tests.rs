//! What the pathfinder answers when a goal cannot be reached, and which segments a body sweeps.
//!
//! # Responsibility
//! Pins the two contracts movement relies on: [`Pathfinder::nearest_reachable_point`] returns the
//! same point for every unit in a connected region, and [`Pathfinder::segment_is_passable`] never
//! admits a segment across terrain a disc cannot physically cross.
//!
//! # Invariants
//! All maps are built in place; nothing here touches the filesystem.

#[path = "support/radii.rs"]
mod radii;
use radii::{ALL_RADII, SMALL_RADIUS};

use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapError, PathRequest, Pathfinder, PlayerVisionMap,
};
use sim_types::{SimScalar, SimVec2};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

/// A map divided by a solid wall at `wall_x`.
fn divided(side: u32, wall_x: u32) -> Result<GameMap, Box<dyn std::error::Error>> {
    let mut map = GameMap::new(side, side, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..side {
        map.cell_mut(wall_x, y)?.flags = CellFlags::empty();
    }
    Ok(map)
}

fn request(start: SimVec2, goal: SimVec2) -> sim_map::PathRequestWithVision<'static> {
    PathRequest {
        start,
        goal,
        radius: SMALL_RADIUS,
    }
    .into()
}

#[test]
fn nearest_point_is_shared_across_a_connected_region() -> TestResult {
    let map = divided(32, 16)?;
    let mut pathfinder = Pathfinder::new(&map);
    let goal = at(24.5, 20.5);
    assert!(matches!(
        pathfinder.find_path_with_vision(&map, &request(at(4.5, 4.5), goal)),
        Err(MapError::NoPathFound { .. })
    ));
    let first = pathfinder.nearest_reachable_point(&map, &request(at(4.5, 4.5), goal))?;
    let second = pathfinder.nearest_reachable_point(&map, &request(at(9.5, 28.5), goal))?;
    assert_eq!(first, Some(at(15.5, 20.5)));
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn nearest_point_handles_a_blocked_goal_cell() -> TestResult {
    let map = divided(32, 16)?;
    let mut pathfinder = Pathfinder::new(&map);
    let found = pathfinder.nearest_reachable_point(&map, &request(at(4.5, 9.5), at(16.5, 9.5)))?;
    assert_eq!(found, Some(at(15.5, 9.5)));
    Ok(())
}

#[test]
fn nearest_point_is_none_when_already_standing_on_it() -> TestResult {
    let map = divided(32, 16)?;
    let mut pathfinder = Pathfinder::new(&map);
    let found = pathfinder.nearest_reachable_point(&map, &request(at(15.5, 9.5), at(24.5, 9.5)))?;
    assert_eq!(found, None);
    Ok(())
}

#[test]
fn nearest_point_rejects_a_start_without_clearance() -> TestResult {
    let map = divided(32, 16)?;
    let mut pathfinder = Pathfinder::new(&map);
    assert!(matches!(
        pathfinder.nearest_reachable_point(&map, &request(at(16.5, 9.5), at(24.5, 9.5))),
        Err(MapError::InvalidStart(_))
    ));
    Ok(())
}

#[test]
fn clear_line_needs_no_search_and_gives_the_straight_route() -> TestResult {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let mut pathfinder = Pathfinder::new(&map);
    let (start, goal) = (at(3.25, 4.75), at(28.5, 22.125));
    let route = pathfinder.find_path_with_vision(&map, &request(start, goal))?;
    assert_eq!(route.waypoints, [start, goal]);
    Ok(())
}

// --- The same contracts on the navigation mesh (sc2 pathfinding WP7).

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/arena.rs"]
mod arena;

fn mesh_request(
    start: SimVec2,
    goal: SimVec2,
    radius: SimScalar,
) -> sim_map::PathRequestWithVision<'static> {
    sim_map::PathRequestWithVision {
        start,
        goal,
        radius,
        vision: None,
        block_unexplored_ramps: false,
        known_structures: None,
    }
}

#[test]
fn mesh_nearest_reachable_point_is_shared_by_a_whole_region() -> TestResult {
    let map = divided(20, 10)?;
    let mut pathfinder = Pathfinder::new(&map);
    let goal = at(15.5, 7.5);
    let radius = SimScalar::from_num(0.375);
    let mut answers = Vec::new();
    for start in [at(2.5, 2.5), at(5.5, 17.5), at(8.5, 9.5), at(1.5, 1.5)] {
        answers.push(pathfinder.nearest_reachable_point(&map, &mesh_request(start, goal, radius))?);
    }
    assert!(
        answers.iter().all(|answer| *answer == answers[0]),
        "{answers:?}"
    );
    assert_eq!(
        answers[0],
        Some(at(9.5, 7.5)),
        "the reachable cell nearest the goal, beside the wall"
    );
    Ok(())
}

#[test]
fn mesh_nearest_reachable_point_is_none_from_the_best_cell() -> TestResult {
    let map = divided(20, 10)?;
    let mut pathfinder = Pathfinder::new(&map);
    let point = pathfinder.nearest_reachable_point(
        &map,
        &mesh_request(at(9.5, 7.5), at(15.5, 7.5), SimScalar::from_num(0.375)),
    )?;
    assert_eq!(point, None);
    Ok(())
}

/// A map split by a wall of structure cells at `x = 10`.
fn structure_wall() -> Result<GameMap, Box<dyn std::error::Error>> {
    let mut map = GameMap::new(20, 20, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..20 {
        map.place_structure(sim_types::EntityId(100 + y), 10, y, 1, 1)?;
    }
    Ok(map)
}

#[test]
fn mesh_walks_through_unknown_structures_and_not_known_ones() -> TestResult {
    let map = structure_wall()?;
    let mut pathfinder = Pathfinder::new(&map);
    let (start, goal) = (at(4.5, 9.5), at(15.5, 9.5));
    let radius = SimScalar::from_num(0.375);
    let known_all = mesh_request(start, goal, radius);
    assert!(matches!(
        pathfinder.find_route_mesh(&map, &known_all),
        Err(MapError::NoPathFound { .. })
    ));
    let unknown = vec![false; 400];
    let request = sim_map::PathRequestWithVision {
        known_structures: Some(&unknown),
        ..known_all
    };
    let route = pathfinder.find_route_mesh(&map, &request)?;
    assert_eq!(
        route.waypoints,
        vec![start, goal],
        "an unknown structure does not bend the route"
    );
    let mut known_cells = vec![false; 400];
    known_cells[9 * 20 + 10] = true;
    let request = sim_map::PathRequestWithVision {
        known_structures: Some(&known_cells),
        ..known_all
    };
    let route = pathfinder.find_route_mesh(&map, &request)?;
    assert!(
        route.waypoints.len() > 2,
        "a known structure cell is walked around: {route:?}"
    );
    for pair in route.waypoints.windows(2) {
        assert!(pathfinder.segment_is_passable(&map, &request, pair[0], pair[1]));
    }
    // Vision: an unexplored map knows no structure; a fully visible one knows them all.
    let unseen = PlayerVisionMap::new(20, 20);
    let request = sim_map::PathRequestWithVision {
        vision: Some(&unseen),
        ..known_all
    };
    assert!(pathfinder.find_route_mesh(&map, &request).is_ok());
    Ok(())
}

#[test]
fn mesh_refuses_unexplored_ramps_when_asked_like_the_grid() {
    let map = arena::default_arena();
    let mut pathfinder = Pathfinder::new(&map);
    let unexplored = PlayerVisionMap::new(map.width(), map.height());
    let request = sim_map::PathRequestWithVision {
        vision: Some(&unexplored),
        block_unexplored_ramps: true,
        ..mesh_request(at(31.5, 20.5), at(31.5, 31.5), SimScalar::from_num(0.375))
    };
    assert!(
        pathfinder.find_route_mesh(&map, &request).is_err(),
        "goal on an unexplored ramp"
    );
    assert!(
        pathfinder.find_path_with_vision(&map, &request).is_err(),
        "the grid agrees"
    );
    let mut explored = PlayerVisionMap::new(map.width(), map.height());
    explored.set_all_fogged();
    let request = sim_map::PathRequestWithVision {
        vision: Some(&explored),
        ..request
    };
    assert!(pathfinder.find_route_mesh(&map, &request).is_ok());
    assert!(pathfinder.find_path_with_vision(&map, &request).is_ok());
}

/// The mesh segment predicate never admits a segment that crosses terrain the physical-motion
/// truth refuses, on random segments of both shipped maps, for each sample radius, and it is not
/// vacuous: it admits some of them.
#[test]
fn mesh_segment_predicate_never_crosses_terrain() {
    use rand::{Rng, SeedableRng};
    for file in [arena::DEFAULT_ARENA_FILE, arena::SHOWCASE_CANYON_FILE] {
        let map = arena::load_game_map(file);
        let mut pathfinder = Pathfinder::new(&map);
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0x5e9_3e47);
        for radius in ALL_RADII {
            let mut admitted = 0u32;
            for _ in 0..10_000 {
                let mut point = || {
                    at(
                        rng.gen_range(0.0..f64::from(map.width())),
                        rng.gen_range(0.0..f64::from(map.height())),
                    )
                };
                let (a, b) = (point(), point());
                let request = PathRequest {
                    start: a,
                    goal: b,
                    radius,
                }
                .into();
                if pathfinder.segment_is_passable(&map, &request, a, b) {
                    admitted += 1;
                    assert!(
                        map.ground_segment_is_connected(a, b, |_, _| true),
                        "{file}: mesh admits {a:?} -> {b:?} across terrain"
                    );
                }
            }
            assert!(admitted > 0, "{file} {radius:?}: no segment admitted");
        }
    }
}
