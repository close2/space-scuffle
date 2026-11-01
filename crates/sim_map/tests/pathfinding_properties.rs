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
//! Maps are small, so the O(cells * walls) oracle stays cheap. Generated maps may be entirely
//! disconnected; properties that need a route are conditional on the search finding one.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "indices into a 16x16 test map always fit in i64 and u32"
)]

use proptest::collection::vec;
use proptest::prelude::*;
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, PathRequest, Pathfinder, UnitClearance};
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
        };
        if let Ok(path) = pathfinder.find_path(&map, &request) {
            prop_assert_eq!(path.waypoints.first(), Some(&request.start));
            prop_assert_eq!(path.waypoints.last(), Some(&request.goal));
            let mut measured = SimScalar::ZERO;
            for pair in path.waypoints.windows(2) {
                measured += pair[0].distance(pair[1]);
                prop_assert!(
                    pathfinder.segment_is_passable(&map, &request.into(), pair[0], pair[1]),
                    "illegal segment {:?}", pair
                );
            }
            prop_assert_eq!(measured, path.total_length);
            let straight = request.start.distance(request.goal);
            prop_assert!(path.total_length >= straight - SimScalar::from_num(0.001));
        }
    }

    /// Reachability is symmetric: terrain has no one-way doors.
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
        };
        let backward = PathRequest {
            start: forward.goal,
            goal: forward.start,
            clearance,
        };
        prop_assert_eq!(
            pathfinder.find_path(&map, &forward).is_ok(),
            pathfinder.find_path(&map, &backward).is_ok()
        );
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
        };
        let target = PathRequest {
            start: distraction.goal,
            goal: distraction.start,
            clearance: UnitClearance::Medium,
        };
        let _discarded = reused.find_path(&map, &distraction);
        let after_reuse = reused.find_path(&map, &target);
        let fresh = Pathfinder::new(&map).find_path(&map, &target);
        prop_assert_eq!(after_reuse.ok(), fresh.ok());
    }
}
