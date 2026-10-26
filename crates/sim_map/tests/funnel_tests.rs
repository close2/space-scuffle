//! Integration tests for Simple Stupid Funnel (String-Pulling) algorithm,
//! Euclidean shortcuts around L-shaped obstacles, and corner radius offsets.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown
)]

use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, PathRequest, Pathfinder, UnitClearance,
    CORNER_SAFETY_MARGIN,
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
    };
    let req_medium = PathRequest {
        start,
        goal,
        clearance: UnitClearance::Medium,
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
