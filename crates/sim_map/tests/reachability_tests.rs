//! What the pathfinder answers when a goal cannot be reached, and which tier routes a body.
//!
//! # Responsibility
//! Pins the two contracts movement relies on: [`Pathfinder::nearest_reachable_point`] returns the
//! same point for every unit in a connected region, and [`UnitClearance::for_radius`] agrees with
//! what a disc of that radius physically fits through.
//!
//! # Invariants
//! All maps are built in place; nothing here touches the filesystem.

use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapError, PathRequest, Pathfinder, UnitClearance,
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
        clearance: UnitClearance::Small,
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
fn tier_follows_what_the_disc_fits_through() {
    let tier = |radius: f64| UnitClearance::for_radius(SimScalar::from_num(radius));
    assert_eq!(tier(0.375), UnitClearance::Small);
    assert_eq!(
        tier(0.5),
        UnitClearance::Small,
        "fits a one-cell gap exactly"
    );
    assert_eq!(tier(0.5625), UnitClearance::Medium);
    assert_eq!(tier(0.875), UnitClearance::Medium);
    assert_eq!(tier(1.0), UnitClearance::Large);
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
