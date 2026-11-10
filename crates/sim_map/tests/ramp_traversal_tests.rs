//! Verifies shared ground connectivity, full-segment movement and legal smoothed
//! navigation through real versioned maps. Expected ramp entrances and forbidden
//! side faces are specified independently of the implementation under test.

#[path = "support/radii.rs"]
mod radii;
use radii::{required_clearance, LARGE_RADIUS, MEDIUM_RADIUS, SMALL_RADIUS};

use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapFile, PathRequest, PathRequestWithVision,
    Pathfinder, PlayerVisionMap, RampDir,
};
use sim_types::{EntityId, SimScalar, SimVec2};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn center(x: u32, y: u32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

fn ramp_map() -> sim_map::Result<GameMap> {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1)))?;
    for y in 8..16 {
        for x in 0..16 {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(2);
        }
    }
    for y in 5..8 {
        for x in 6..9 {
            let cell = map.cell_mut(x, y)?;
            cell.ramp = Some(RampDir::North);
            cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
        }
    }
    Ok(map)
}

fn segment(map: &GameMap, a: SimVec2, b: SimVec2) -> bool {
    map.ground_segment_is_connected(a, b, |_, next| {
        map.cell(next.0, next.1)
            .is_ok_and(|c| !c.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE))
    })
}

fn assert_symmetric(map: &GameMap, from: (u32, u32), to: (u32, u32), expected: bool) {
    assert_eq!(
        map.terrain_step_is_connected(from, to),
        expected,
        "{from:?} → {to:?}"
    );
    assert_eq!(
        map.terrain_step_is_connected(to, from),
        expected,
        "{to:?} → {from:?}"
    );
}

#[test]
fn ordinary_terrain_preserves_height_occupancy_and_cliff_boundaries() -> TestResult {
    let mut map = GameMap::new(4, 4, MapCell::default_ground(CliffLevel(1)))?;
    assert_symmetric(&map, (1, 1), (2, 1), true);
    assert!(map.terrain_step_is_connected((1, 1), (1, 1)));
    map.cell_mut(2, 1)?.cliff_level = CliffLevel(2);
    assert_symmetric(&map, (1, 1), (2, 1), false);
    map.cell_mut(2, 1)?.cliff_level = CliffLevel(1);

    map.place_structure(EntityId(900), 2, 1, 1, 1)?;
    assert_symmetric(&map, (1, 1), (2, 1), true);
    assert!(!segment(&map, center(1, 1), center(2, 1)));
    map.cell_mut(2, 1)?
        .flags
        .insert(CellFlags::UNPASSABLE_CLIFF);
    assert_symmetric(&map, (1, 1), (2, 1), false);
    assert!(!map.terrain_step_is_connected((2, 1), (2, 1)));

    map.cell_mut(1, 1)?.flags = CellFlags::empty();
    assert!(!map.terrain_step_is_connected((1, 1), (1, 1)));
    assert!(!map.terrain_step_is_connected((4, 1), (4, 1)));
    Ok(())
}

#[test]
fn cardinal_profiles_connect_only_matching_surfaces() -> TestResult {
    let north = ramp_map()?;
    for (direction, rotation) in [
        (RampDir::North, 0),
        (RampDir::East, 1),
        (RampDir::South, 2),
        (RampDir::West, 3),
    ] {
        let rotate = |(mut x, mut y): (u32, u32)| {
            for _ in 0..rotation {
                (x, y) = (y, 15 - x);
            }
            (x, y)
        };
        let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1)))?;
        for y in 0..16 {
            for x in 0..16 {
                let (rx, ry) = rotate((x, y));
                let mut cell = north.cell(x, y)?.clone();
                if cell.ramp.is_some() {
                    cell.ramp = Some(direction);
                }
                *map.cell_mut(rx, ry)? = cell;
            }
        }
        for x in 6..9 {
            for y in 4..8 {
                assert_symmetric(&map, rotate((x, y)), rotate((x, y + 1)), true);
            }
        }
        assert_symmetric(&map, rotate((6, 6)), rotate((7, 6)), true);
        assert_symmetric(&map, rotate((6, 6)), rotate((7, 7)), true);
        assert_symmetric(&map, rotate((6, 7)), rotate((7, 8)), true);
        assert_symmetric(&map, rotate((6, 6)), rotate((5, 6)), false);
        assert_symmetric(&map, rotate((6, 5)), rotate((5, 4)), false);
        assert!(segment(
            &map,
            center(rotate((7, 4)).0, rotate((7, 4)).1),
            center(rotate((7, 8)).0, rotate((7, 8)).1)
        ));
    }
    Ok(())
}

#[test]
fn hostile_neighbors_and_malformed_ramps_never_waive_cliffs() -> TestResult {
    let mut map = ramp_map()?;
    for level in 0..=3 {
        map.cell_mut(5, 6)?.cliff_level = CliffLevel(level);
        assert_symmetric(&map, (6, 6), (5, 6), false);
    }
    map.cell_mut(7, 8)?.cliff_level = CliffLevel(3);
    assert_symmetric(&map, (7, 7), (7, 8), false);
    map.cell_mut(7, 4)?.cliff_level = CliffLevel(0);
    assert_symmetric(&map, (7, 5), (7, 4), false);
    map.cell_mut(6, 6)?.ramp = Some(RampDir::South);
    assert_symmetric(&map, (6, 6), (7, 6), false);
    map.cell_mut(6, 6)?.ramp = Some(RampDir::NorthEast);
    assert!(!map.terrain_step_is_connected((6, 6), (6, 6)));
    map.cell_mut(6, 6)?.ramp = None;
    map.cell_mut(6, 6)?.cliff_level = CliffLevel(3);
    assert_symmetric(&map, (6, 6), (7, 6), false);
    let mut oversized = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1)))?;
    for y in 3..12 {
        oversized.cell_mut(7, y)?.ramp = Some(RampDir::North);
    }
    for y in 3..12 {
        assert!(!oversized.terrain_step_is_connected((7, y), (7, y)));
    }
    oversized.cell_mut(2, 2)?.ramp = Some(RampDir::North);
    oversized.cell_mut(2, 2)?.cliff_level = CliffLevel(u8::MAX);
    assert!(!oversized.terrain_step_is_connected((2, 2), (2, 2)));
    let mut lanes = ramp_map()?;
    lanes.cell_mut(6, 5)?.ramp = None;
    assert_symmetric(&lanes, (6, 6), (7, 6), false);
    Ok(())
}

#[test]
fn continuous_segments_visit_intervening_cells_and_exact_boundaries() -> TestResult {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1)))?;
    for (a, b) in [
        ((1, 1), (14, 1)),
        ((1, 1), (1, 14)),
        ((1, 1), (14, 14)),
        ((1, 1), (14, 2)),
        ((3, 3), (3, 3)),
    ] {
        assert!(segment(&map, center(a.0, a.1), center(b.0, b.1)));
        assert!(segment(&map, center(b.0, b.1), center(a.0, a.1)));
    }
    map.cell_mut(7, 1)?.cliff_level = CliffLevel(2);
    assert!(!segment(&map, center(1, 1), center(14, 1)));
    assert!(!segment(&map, center(14, 1), center(1, 1)));
    map.cell_mut(7, 1)?.cliff_level = CliffLevel(1);
    map.place_structure(EntityId(1), 7, 1, 1, 1)?;
    assert!(!segment(&map, center(1, 1), center(14, 1)));
    assert!(!segment(&map, center(7, 1), center(7, 1)));
    assert!(!segment(&map, center(1, 1), center(16, 1)));
    let boundary = SimVec2::new(SimScalar::from_num(7), SimScalar::from_num(1));
    assert!(!segment(&map, center(6, 1), boundary));
    assert!(!segment(&map, boundary, center(6, 1)));
    Ok(())
}

#[test]
fn clearance_and_smoothing_keep_ramp_side_walls_closed() -> TestResult {
    let map = ramp_map()?;
    let mut pathfinder = Pathfinder::new(&map);
    // The ramp is three cells wide, and its side wall limits the ground beside it.
    assert!(pathfinder.clearance_at(6, 6) >= required_clearance(LARGE_RADIUS));
    assert!(pathfinder.clearance_at(5, 6) < pathfinder.clearance_at(2, 2));
    for radius in [SMALL_RADIUS, MEDIUM_RADIUS, LARGE_RADIUS] {
        let request = PathRequest {
            start: center(6, 4),
            goal: center(7, 9),
            radius,
        };
        let path = pathfinder.find_path(&map, &request)?;
        for pair in path.waypoints.windows(2) {
            assert!(segment(&map, pair[0], pair[1]), "illegal segment {pair:?}");
        }
    }
    // The direct line enters the ramp's west side; a route via its lower landing exists.
    let request = PathRequest {
        start: center(4, 6),
        goal: center(7, 9),
        radius: SMALL_RADIUS,
    };
    assert!(!segment(&map, request.start, request.goal));
    let path = pathfinder.find_path(&map, &request)?;
    assert!(path.waypoints.len() > 2);
    for pair in path.waypoints.windows(2) {
        assert!(segment(&map, pair[0], pair[1]));
        // Independent west ramp-face intersection: x=6, y in [5,8) is forbidden.
        let wall = SimScalar::from_num(6);
        let dx = pair[1].x - pair[0].x;
        if dx != SimScalar::ZERO
            && ((pair[0].x < wall && pair[1].x >= wall) || (pair[1].x < wall && pair[0].x >= wall))
        {
            let crossing_y = pair[0].y + (pair[1].y - pair[0].y) * (wall - pair[0].x) / dx;
            assert!(crossing_y < SimScalar::from_num(5) || crossing_y >= SimScalar::from_num(8));
        }
    }
    let flat = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1)))?;
    let request = PathRequest {
        start: center(2, 2),
        goal: center(12, 9),
        radius: SMALL_RADIUS,
    };
    assert_eq!(
        Pathfinder::new(&flat).find_path(&flat, &request)?.waypoints,
        [request.start, request.goal]
    );
    Ok(())
}

#[test]
fn unexplored_ramps_are_not_reintroduced_by_smoothing() -> TestResult {
    let map = ramp_map()?;
    let vision = PlayerVisionMap::new(16, 16);
    let request = PathRequestWithVision {
        start: center(4, 6),
        goal: center(7, 9),
        radius: SMALL_RADIUS,
        vision: Some(&vision),
        block_unexplored_ramps: true,
        known_structures: None,
    };
    assert!(Pathfinder::new(&map)
        .find_path_with_vision(&map, &request)
        .is_err());
    let mut explored = PlayerVisionMap::new(16, 16);
    explored.set_all_fogged();
    let request = PathRequestWithVision {
        vision: Some(&explored),
        ..request
    };
    let path = Pathfinder::new(&map).find_path_with_vision(&map, &request)?;
    assert!(path
        .waypoints
        .windows(2)
        .all(|pair| segment(&map, pair[0], pair[1])));
    Ok(())
}

#[test]
fn real_structures_obey_knowledge_and_same_cell_footprint_policy() -> TestResult {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1)))?;
    // A wall closes every physical route. An observer who has not discovered it
    // must still plan the straight route through the underlying ground surface.
    for y in 0..16 {
        map.place_structure(EntityId(y + 1), 8, y, 1, 1)?;
    }
    // Together with the wall this leaves cell (7, 7) one cell wide.
    map.place_structure(EntityId(100), 6, 7, 1, 1)?;
    let mut pathfinder = Pathfinder::new(&map);
    let unknown = vec![false; 256];
    let known = vec![true; 256];
    let request = PathRequestWithVision {
        start: center(3, 7),
        goal: center(12, 7),
        radius: SMALL_RADIUS,
        vision: None,
        block_unexplored_ramps: false,
        known_structures: Some(&unknown),
    };
    let path = pathfinder.find_path_with_vision(&map, &request)?;
    assert_eq!(path.waypoints, [request.start, request.goal]);
    assert!(!segment(&map, request.start, request.goal));
    assert!(pathfinder
        .find_path_with_vision(
            &map,
            &PathRequestWithVision {
                known_structures: Some(&known),
                ..request
            }
        )
        .is_err());
    // The cell itself is clear, but no point in it fits a Large disc between known walls.
    let same_cell = PathRequestWithVision {
        start: center(7, 7),
        goal: center(7, 7),
        radius: LARGE_RADIUS,
        known_structures: Some(&known),
        ..request
    };
    assert!(pathfinder.find_path_with_vision(&map, &same_cell).is_err());
    assert!(pathfinder
        .find_path_with_vision(
            &map,
            &PathRequestWithVision {
                known_structures: Some(&unknown),
                ..same_cell
            }
        )
        .is_ok());
    map.remove_structure(EntityId(8))?;
    pathfinder.recompute_clearance(&map);
    assert!(pathfinder
        .find_path_with_vision(
            &map,
            &PathRequestWithVision {
                known_structures: Some(&known),
                ..request
            }
        )
        .is_ok());
    Ok(())
}

#[test]
fn real_versioned_maps_have_bidirectional_ramp_routes() -> TestResult {
    for filename in ["default_arena.scmap.ron", "showcase_canyon.scmap.ron"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/maps")
            .join(filename);
        let map = MapFile::load(&path)?.decode()?.to_game_map()?;
        let mut pathfinder = Pathfinder::new(&map);
        let mut runs = 0;
        for y in 0..map.height() {
            for x in 0..map.width() {
                let Some(profile) = map.ramp_profile(x, y) else {
                    continue;
                };
                if profile.index != 0 {
                    continue;
                }
                let axis = match profile.dir {
                    RampDir::North => (0, 1),
                    RampDir::South => (0, -1),
                    RampDir::East => (1, 0),
                    RampDir::West => (-1, 0),
                    _ => return Err("versioned map unexpectedly contains diagonal ramp".into()),
                };
                let low_x = u32::try_from(i64::from(x) - axis.0)?;
                let low_y = u32::try_from(i64::from(y) - axis.1)?;
                let high_x = u32::try_from(i64::from(x) + axis.0 * i64::from(profile.run_len))?;
                let high_y = u32::try_from(i64::from(y) + axis.1 * i64::from(profile.run_len))?;
                let low = center(low_x, low_y);
                let high = center(high_x, high_y);
                assert!(segment(&map, low, high), "{filename}: run ({x}, {y})");
                for (start, goal) in [(low, high), (high, low)] {
                    let request = PathRequest {
                        start,
                        goal,
                        radius: SMALL_RADIUS,
                    };
                    let first = pathfinder.find_path(&map, &request)?;
                    assert_eq!(first, pathfinder.find_path(&map, &request)?);
                    assert!(first
                        .waypoints
                        .windows(2)
                        .all(|pair| segment(&map, pair[0], pair[1])));
                }
                runs += 1;
            }
        }
        assert!(runs > 0, "fixture {filename} must exercise ramps");
        if filename == "default_arena.scmap.ron" {
            assert!(segment(&map, center(31, 29), center(31, 34)));
            assert!(!segment(&map, center(29, 29), center(29, 34)));
            let mut previous = map.terrain_altitude_scalar(center(31, 29));
            for quarter in 0..=20 {
                let position = SimVec2::new(
                    SimScalar::from_num(31) + SimScalar::HALF,
                    SimScalar::from_num(29)
                        + SimScalar::HALF
                        + SimScalar::from_num(quarter) / SimScalar::from_num(4),
                );
                let altitude = map.terrain_altitude_scalar(position);
                assert!(altitude >= previous);
                previous = altitude;
            }
        }
    }
    Ok(())
}
