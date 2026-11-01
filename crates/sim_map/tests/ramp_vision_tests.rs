//! Fog of War rules that depend on terrain height, long sight and watchtower occupancy.
//!
//! # Responsibility
//! Pins three behaviours of `PlayerVisionMap` and `GameMap`: a ground observer's view along a
//! ramp follows the height it stands at, sight is not silently shortened, and a watchtower
//! is released when its occupants leave.
//!
//! # How it fits
//! Fog state feeds targeting and pathfinding, so a wrong answer here changes what units do.
//!
//! # Invariants
//! The fixture is a two-level map joined by one straight four-cell ramp, so the expected
//! height of every cell can be worked out by hand: level 1 is 3.0 world units, level 2 is 6.0,
//! and the ramp rises 0.75 per cell.

use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, PlayerVisionMap, RampDir, VisibilityState,
    VisionSource, MAX_SIGHT_RADIUS_CELLS,
};
use sim_types::{MoverType, PlayerId, SimScalar, SimVec2};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const RAMP_X: std::ops::Range<u32> = 10..15;
const RAMP_Y: std::ops::Range<u32> = 10..14;

fn ramp_map() -> Result<GameMap, Box<dyn std::error::Error>> {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(1)))?;
    for y in RAMP_Y.end..32 {
        for x in 0..32 {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(2);
        }
    }
    for y in RAMP_Y {
        for x in RAMP_X {
            let cell = map.cell_mut(x, y)?;
            cell.ramp = Some(RampDir::North);
            cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
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

fn seen_from(map: &GameMap, pos: SimVec2, mover_type: MoverType, radius: i32) -> PlayerVisionMap {
    let mut vision = PlayerVisionMap::new(map.width(), map.height());
    vision.begin_tick_update();
    vision.apply_vision_source(
        map,
        &VisionSource {
            pos,
            mover_type,
            sight_radius: SimScalar::from_i32(radius),
            is_detector: false,
        },
    );
    vision.end_tick_update();
    vision
}

fn visible(vision: &PlayerVisionMap, x: u32, y: u32) -> bool {
    vision.visibility_at(x, y) == VisibilityState::Visible
}

#[test]
fn f4_low_ground_observer_sees_only_the_lower_part_of_a_ramp() -> TestResult {
    let map = ramp_map()?;
    let vision = seen_from(&map, centre(12, 8), MoverType::Ground, 12);
    assert!(
        visible(&vision, 12, 10),
        "bottom ramp cell is below eye level limit"
    );
    assert!(
        visible(&vision, 12, 11),
        "second ramp cell is within half a cliff"
    );
    assert!(
        !visible(&vision, 12, 12),
        "upper half of the ramp must be hidden"
    );
    assert!(!visible(&vision, 12, 13), "top ramp cell must be hidden");
    assert!(!visible(&vision, 12, 15), "plateau must stay hidden");
    Ok(())
}

#[test]
fn f4_observer_near_the_top_of_a_ramp_sees_the_plateau() -> TestResult {
    let map = ramp_map()?;
    let vision = seen_from(&map, centre(12, 13), MoverType::Ground, 10);
    assert!(
        visible(&vision, 12, 15),
        "a unit cresting the ramp sees the plateau"
    );
    assert!(visible(&vision, 12, 20));
    Ok(())
}

#[test]
fn f4_observer_at_the_foot_of_a_ramp_does_not_see_the_plateau() -> TestResult {
    let map = ramp_map()?;
    let vision = seen_from(&map, centre(12, 10), MoverType::Ground, 10);
    assert!(!visible(&vision, 12, 15));
    Ok(())
}

#[test]
fn f4_high_ground_observer_sees_down_the_whole_ramp() -> TestResult {
    let map = ramp_map()?;
    let vision = seen_from(&map, centre(12, 16), MoverType::Ground, 12);
    for y in RAMP_Y {
        assert!(
            visible(&vision, 12, y),
            "high ground must see ramp cell y={y}"
        );
    }
    assert!(visible(&vision, 12, 7), "and the low ground beyond it");
    Ok(())
}

#[test]
fn f4_air_observer_ignores_ramp_height() -> TestResult {
    let map = ramp_map()?;
    let vision = seen_from(&map, centre(12, 8), MoverType::Air, 12);
    assert!(visible(&vision, 12, 13));
    assert!(visible(&vision, 12, 18));
    Ok(())
}

#[test]
fn f12_sight_beyond_the_old_cap_reaches_its_authored_distance() -> TestResult {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(1)))?;
    let radius = 22;
    let vision = seen_from(&map, centre(2, 30), MoverType::Ground, radius);
    for (x, y) in [(23, 30), (21, 30), (16, 44), (18, 42)] {
        assert!(
            visible(&vision, x, y),
            "radius {radius} must reach ({x}, {y})"
        );
    }
    assert!(!visible(&vision, 26, 30), "beyond the radius stays hidden");
    Ok(())
}

#[test]
fn f12_largest_supported_radius_is_honoured() -> TestResult {
    let map = GameMap::new(128, 128, MapCell::default_ground(CliffLevel(1)))?;
    let radius = i32::try_from(MAX_SIGHT_RADIUS_CELLS)?;
    let vision = seen_from(&map, centre(40, 40), MoverType::Ground, radius);
    let edge = 40 + MAX_SIGHT_RADIUS_CELLS - 1;
    assert!(visible(&vision, edge, 40));
    Ok(())
}

#[test]
fn f2_vacated_tower_is_released() {
    let mut arena = GameMap::default_arena();
    let player = PlayerId(1);
    let tower = arena.watchtowers()[0].pos;

    arena.update_watchtower_capture(&[(player, tower)]);
    assert_eq!(arena.watchtowers()[0].controlling_player, Some(player));

    arena.update_watchtower_capture(&[(player, SimVec2::from_i32(2, 2))]);
    assert_eq!(arena.watchtowers()[0].controlling_player, None);
}
