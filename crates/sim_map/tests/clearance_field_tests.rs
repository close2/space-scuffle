//! Clearance must be a symmetric property of the terrain and of a unit's real size.
//!
//! # Responsibility
//! Pins that a unit's ability to stand or pass somewhere depends on the free space around it,
//! not on which side of it a wall happens to lie, and that zero-width diagonal gaps are closed.
//!
//! # How it fits
//! Every ground path is built on this field, so a directional bias here shows up as large units
//! that refuse to approach walls on two sides of the map only.
//!
//! # Invariants
//! Symmetry tests compare a map with its own mirror image, so they need no hand-computed
//! expected values and cannot share a bias with the implementation.

use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapError, PathRequest, Pathfinder, UnitClearance,
};
use sim_types::{EntityId, SimScalar, SimVec2};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SIDE: u32 = 24;

fn open_map() -> Result<GameMap, Box<dyn std::error::Error>> {
    Ok(GameMap::new(
        SIDE,
        SIDE,
        MapCell::default_ground(CliffLevel(1)),
    )?)
}

fn block(map: &mut GameMap, x: u32, y: u32) -> sim_map::Result<()> {
    map.cell_mut(x, y)?.flags = CellFlags::empty();
    Ok(())
}

fn centre(x: u32, y: u32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

fn route_exists(map: &GameMap, from: (u32, u32), to: (u32, u32), clearance: UnitClearance) -> bool {
    Pathfinder::new(map)
        .find_path(
            map,
            &PathRequest {
                start: centre(from.0, from.1),
                goal: centre(to.0, to.1),
                clearance,
                radius: clearance.radius_scalar(),
            },
        )
        .is_ok()
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

/// The route of a disc of `clearance`'s radius between two world points (the mesh refuses a
/// goal where the whole disc does not fit, plan Amendment 2).
fn route_between(
    map: &GameMap,
    start: SimVec2,
    goal: SimVec2,
    clearance: UnitClearance,
) -> sim_map::Result<sim_map::PathResult> {
    Pathfinder::new(map).find_path(
        map,
        &PathRequest {
            start,
            goal,
            clearance,
            radius: clearance.radius_scalar(),
        },
    )
}

/// Mirrors a blocked-cell list through the map centre on both axes.
fn mirrored(cells: &[(u32, u32)]) -> Vec<(u32, u32)> {
    cells
        .iter()
        .map(|&(x, y)| (SIDE - 1 - x, SIDE - 1 - y))
        .collect()
}

fn map_with(cells: &[(u32, u32)]) -> Result<GameMap, Box<dyn std::error::Error>> {
    let mut map = open_map()?;
    for &(x, y) in cells {
        block(&mut map, x, y)?;
    }
    Ok(map)
}

fn wall_samples() -> Vec<Vec<(u32, u32)>> {
    let vertical: Vec<_> = (0..SIDE).map(|y| (12, y)).collect();
    let horizontal: Vec<_> = (0..SIDE).map(|x| (x, 9)).collect();
    let pillar = vec![(7, 7), (8, 7), (7, 8), (8, 8)];
    let scattered = vec![(3, 4), (15, 6), (9, 18), (20, 20), (6, 13)];
    vec![vertical, horizontal, pillar, scattered]
}

#[test]
fn m8_clearance_is_invariant_under_map_rotation() -> TestResult {
    for cells in wall_samples() {
        let forward = Pathfinder::new(&map_with(&cells)?);
        let rotated = Pathfinder::new(&map_with(&mirrored(&cells))?);
        for y in 0..SIDE {
            for x in 0..SIDE {
                assert_eq!(
                    forward.clearance_at(x, y),
                    rotated.clearance_at(SIDE - 1 - x, SIDE - 1 - y),
                    "clearance differs between ({x}, {y}) and its rotation for {cells:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn m8_large_unit_can_stand_beside_a_wall_on_any_side() -> TestResult {
    // A vertical wall at x = 12: the cells either side of it are mirror images.
    let map = map_with(&(0..SIDE).map(|y| (12, y)).collect::<Vec<_>>())?;
    let large = UnitClearance::Large;
    assert!(
        route_exists(&map, (5, 5), (10, 5), large),
        "west of wall, one cell clear"
    );
    assert!(
        route_exists(&map, (18, 5), (14, 5), large),
        "east of wall, one cell clear"
    );
    // The wall occupies 12 <= x <= 13: a disc of radius 1 touches it at x = 11 and x = 14.
    assert!(
        route_between(&map, at(5.5, 5.5), at(11.0, 5.5), large).is_ok(),
        "touching the wall from the west"
    );
    assert!(
        route_between(&map, at(18.5, 5.5), at(14.0, 5.5), large).is_ok(),
        "touching the wall from the east"
    );
    // A goal the disc overlaps is refused on both sides alike.
    for (start, goal) in [((5.5, 5.5), (11.5, 5.5)), ((18.5, 5.5), (13.5, 5.5))] {
        assert!(
            matches!(
                route_between(&map, at(start.0, start.1), at(goal.0, goal.1), large),
                Err(MapError::InvalidGoal(_))
            ),
            "{goal:?} overlaps the wall"
        );
    }
    Ok(())
}

#[test]
fn m8_large_unit_can_reach_every_map_edge() -> TestResult {
    let map = open_map()?;
    let large = UnitClearance::Large;
    // The disc touches the map border with its centre one radius inside it.
    let far = f64::from(SIDE) - 1.0;
    for goal in [(1.0, 12.5), (far, 12.5), (12.5, 1.0), (12.5, far)] {
        assert!(
            route_between(&map, at(12.5, 12.5), at(goal.0, goal.1), large).is_ok(),
            "edge point {goal:?}"
        );
    }
    Ok(())
}

#[test]
fn m8_units_fit_through_gaps_as_wide_as_their_real_diameter() -> TestResult {
    let gap = |width: u32| -> Result<GameMap, Box<dyn std::error::Error>> {
        let cells: Vec<_> = (0..SIDE)
            .filter(|&y| !(10..10 + width).contains(&y))
            .map(|y| (12, y))
            .collect();
        map_with(&cells)
    };
    let crossing = |map: &GameMap, clearance| route_exists(map, (6, 11), (18, 11), clearance);

    let one = gap(1)?;
    assert!(
        crossing(&one, UnitClearance::Small),
        "0.75 fits a 1-cell gap"
    );
    assert!(
        !crossing(&one, UnitClearance::Medium),
        "1.25 does not fit a 1-cell gap"
    );
    assert!(
        !crossing(&one, UnitClearance::Large),
        "2.0 does not fit a 1-cell gap"
    );

    let two = gap(2)?;
    assert!(
        crossing(&two, UnitClearance::Medium),
        "1.25 fits a 2-cell gap"
    );
    assert!(
        crossing(&two, UnitClearance::Large),
        "2.0 fits a 2-cell gap"
    );
    Ok(())
}

#[test]
fn m8_known_structures_block_symmetrically() -> TestResult {
    let mut map = open_map()?;
    map.place_structure(EntityId(1), 11, 4, 2, 16)?;
    // The structure occupies 11 <= x <= 13: a disc of radius 1 touches it at x = 10 and 14.
    let reach = |x: f64| route_between(&map, at(x, 1.5), at(x, 8.5), UnitClearance::Large).is_ok();
    assert!(reach(10.0), "west face of the structure");
    assert!(reach(14.0), "east face of the structure");
    Ok(())
}

#[test]
fn m8_cliff_edges_count_as_walls() -> TestResult {
    let mut map = open_map()?;
    for y in 12..SIDE {
        for x in 0..SIDE {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(2);
        }
    }
    let pathfinder = Pathfinder::new(&map);
    assert!(pathfinder.clearance_at(5, 11) < pathfinder.clearance_at(5, 8));
    assert_eq!(
        pathfinder.clearance_at(5, 11),
        pathfinder.clearance_at(5, 12)
    );
    Ok(())
}

#[test]
fn f7_no_tier_passes_between_structures_touching_at_a_corner() -> TestResult {
    // Two 2x2 buildings touch only at the vertex (12, 12). Terrain staircases extend them along
    // the main diagonal into a complete wall separating north-west from south-east.
    let mut map = open_map()?;
    for i in 0..10 {
        block(&mut map, i, i)?;
        block(&mut map, i + 1, i)?;
    }
    for i in 14..SIDE {
        block(&mut map, i, i)?;
        if i + 1 < SIDE {
            block(&mut map, i + 1, i)?;
        }
    }
    map.place_structure(EntityId(1), 10, 10, 2, 2)?;
    map.place_structure(EntityId(2), 12, 12, 2, 2)?;
    for clearance in [
        UnitClearance::Small,
        UnitClearance::Medium,
        UnitClearance::Large,
    ] {
        let request = PathRequest {
            start: centre(3, 18),
            goal: centre(18, 3),
            clearance,
            radius: clearance.radius_scalar(),
        };
        assert!(
            matches!(
                Pathfinder::new(&map).find_path(&map, &request),
                Err(MapError::NoPathFound { .. })
            ),
            "a corner contact has no width for {clearance:?}"
        );
    }
    Ok(())
}

#[test]
fn f7_small_units_keep_passing_real_one_cell_gaps() -> TestResult {
    let cells: Vec<_> = (0..SIDE).filter(|&y| y != 12).map(|y| (12, y)).collect();
    let map = map_with(&cells)?;
    assert!(route_exists(&map, (6, 6), (18, 18), UnitClearance::Small));
    Ok(())
}

#[test]
fn f7_diagonal_segment_between_touching_structures_is_rejected() -> TestResult {
    let mut map = open_map()?;
    map.place_structure(EntityId(1), 5, 6, 1, 1)?;
    map.place_structure(EntityId(2), 6, 5, 1, 1)?;
    let mut pathfinder = Pathfinder::new(&map);
    let request = PathRequest {
        start: centre(5, 5),
        goal: centre(6, 6),
        clearance: UnitClearance::Small,
        radius: UnitClearance::Small.radius_scalar(),
    };
    assert!(!pathfinder.segment_is_passable(&map, &request.into(), request.start, request.goal));
    Ok(())
}
