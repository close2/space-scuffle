//! Integration tests for Fog of War visibility, high-ground occlusion,
//! line-of-sight blockers, and watchtowers.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown
)]

use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, PlayerVisionMap, VisionSource};
use sim_types::{MoverType, PlayerId, SimScalar, SimVec2, VisibilityState};

#[test]
fn test_low_ground_vs_high_ground_occlusion() {
    let arena = GameMap::default_arena();
    let mut vision = PlayerVisionMap::new(arena.width(), arena.height());

    let player1 = PlayerId(1);
    // Low-ground observer on Cliff 1 (y = 25), 5 cells south of cliff transition zone (y = 30)
    let ground_observer = VisionSource {
        pos: SimVec2::new(SimScalar::from_num(16.5), SimScalar::from_num(24.5)),
        mover_type: MoverType::Ground,
        sight_radius: SimScalar::from_num(12),
        is_detector: false,
    };

    vision.begin_tick_update();
    vision.apply_vision_source(&arena, &ground_observer);
    vision.apply_watchtowers(&arena, player1);
    vision.end_tick_update();

    // Observer's own cell is visible
    assert_eq!(vision.visibility_at(16, 24), VisibilityState::Visible);
    // Cells on low ground north of observer (y = 28) are visible
    assert_eq!(vision.visibility_at(16, 28), VisibilityState::Visible);

    // High ground plateau cells atop Cliff 2 (y = 35) must remain Unexplored (occluded by cliff)
    assert_eq!(vision.visibility_at(16, 35), VisibilityState::Unexplored);
    assert_eq!(vision.visibility_at(16, 40), VisibilityState::Unexplored);

    // Enemy placed on high ground (16.5, 35.5) cannot be seen by Player 1
    let enemy_cell = arena
        .world_to_cell(SimVec2::new(
            SimScalar::from_num(16.5),
            SimScalar::from_num(35.5),
        ))
        .unwrap();
    assert_ne!(
        vision.visibility_at(enemy_cell.0, enemy_cell.1),
        VisibilityState::Visible
    );
}

#[test]
fn test_air_spotter_high_ground_reveal() {
    let arena = GameMap::default_arena();
    let mut vision = PlayerVisionMap::new(arena.width(), arena.height());

    // Initially low ground observer only
    let ground_observer = VisionSource {
        pos: SimVec2::new(SimScalar::from_num(16.5), SimScalar::from_num(24.5)),
        mover_type: MoverType::Ground,
        sight_radius: SimScalar::from_num(12),
        is_detector: false,
    };

    vision.begin_tick_update();
    vision.apply_vision_source(&arena, &ground_observer);
    vision.end_tick_update();
    assert_eq!(vision.visibility_at(16, 35), VisibilityState::Unexplored);

    // Allied air scout moves near the cliff ledge overlooking high ground
    let air_scout = VisionSource {
        pos: SimVec2::new(SimScalar::from_num(16.5), SimScalar::from_num(32.5)),
        mover_type: MoverType::Air,
        sight_radius: SimScalar::from_num(8),
        is_detector: false,
    };

    vision.begin_tick_update();
    vision.apply_vision_source(&arena, &ground_observer);
    vision.apply_vision_source(&arena, &air_scout);
    vision.end_tick_update();

    // Cells on high ground plateau must now be Visible
    assert_eq!(vision.visibility_at(16, 35), VisibilityState::Visible);
    assert_eq!(vision.visibility_at(16, 36), VisibilityState::Visible);
}

#[test]
fn test_los_blocker_smoke_vent() {
    // Construct custom 20x20 flat map
    let default_cell = MapCell::default_ground(CliffLevel(1));
    let mut map = GameMap::new(20, 20, default_cell).unwrap();

    // Place a 3x3 smoke vent LOS_BLOCKER patch at x in [8..11), y in [8..11)
    for y in 8..11 {
        for x in 8..11 {
            map.cell_mut(x, y)
                .unwrap()
                .flags
                .insert(CellFlags::LOS_BLOCKER);
        }
    }

    let mut vision = PlayerVisionMap::new(map.width(), map.height());

    // Observer West of smoke vent at (5.5, 9.5)
    let unit_west = VisionSource {
        pos: SimVec2::new(SimScalar::from_num(5.5), SimScalar::from_num(9.5)),
        mover_type: MoverType::Ground,
        sight_radius: SimScalar::from_num(10),
        is_detector: false,
    };

    vision.begin_tick_update();
    vision.apply_vision_source(&map, &unit_west);
    vision.end_tick_update();

    // Can see up to the edge of the smoke vent (x = 8)
    assert_eq!(vision.visibility_at(8, 9), VisibilityState::Visible);
    // Cannot see inside the center of smoke vent (x = 9) or beyond it to East (x = 12)
    assert_ne!(vision.visibility_at(10, 9), VisibilityState::Visible);
    assert_ne!(vision.visibility_at(12, 9), VisibilityState::Visible);

    // Now move observer INSIDE the smoke vent at (9.5, 9.5)
    let unit_inside = VisionSource {
        pos: SimVec2::new(SimScalar::from_num(9.5), SimScalar::from_num(9.5)),
        mover_type: MoverType::Ground,
        sight_radius: SimScalar::from_num(10),
        is_detector: false,
    };

    vision.begin_tick_update();
    vision.apply_vision_source(&map, &unit_inside);
    vision.end_tick_update();

    // Unit inside CAN see outside across smoke vent boundary
    assert_eq!(vision.visibility_at(5, 9), VisibilityState::Visible);
    assert_eq!(vision.visibility_at(14, 9), VisibilityState::Visible);
}

#[test]
fn test_watchtower_capture_and_vision() {
    let mut arena = GameMap::default_arena();
    let mut vision_p1 = PlayerVisionMap::new(arena.width(), arena.height());
    let mut vision_p2 = PlayerVisionMap::new(arena.width(), arena.height());

    let p1 = PlayerId(1);
    let p2 = PlayerId(2);

    // Tower is at (28.5, 28.5), capture_radius = 3.0, reveal_radius = 12.0
    let tower = arena.watchtowers()[0].clone();
    assert_eq!(tower.controlling_player, None);

    // Player 1 moves a unit within capture radius (2.0 distance from tower)
    let p1_unit_pos = SimVec2::new(SimScalar::from_num(27.5), SimScalar::from_num(28.5));
    arena.update_watchtower_capture(&[(p1, p1_unit_pos)]);

    assert_eq!(arena.watchtowers()[0].controlling_player, Some(p1));

    // Player 1 applies watchtowers
    vision_p1.begin_tick_update();
    vision_p1.apply_watchtowers(&arena, p1);
    vision_p1.end_tick_update();

    // Player 1 receives circular 12-radius vision around tower
    assert_eq!(vision_p1.visibility_at(28, 28), VisibilityState::Visible);
    assert_eq!(vision_p1.visibility_at(28, 38), VisibilityState::Visible); // 10 cells away
    assert_eq!(vision_p1.visibility_at(20, 28), VisibilityState::Visible); // 8 cells away

    // Player 2 unit arrives into capture radius, making tower contested
    let p2_unit_pos = SimVec2::new(SimScalar::from_num(29.5), SimScalar::from_num(28.5));
    arena.update_watchtower_capture(&[(p1, p1_unit_pos), (p2, p2_unit_pos)]);

    // Contested tower yields None controlling player
    assert_eq!(arena.watchtowers()[0].controlling_player, None);

    // Watchtower no longer provides vision to either player
    vision_p1.begin_tick_update();
    vision_p1.apply_watchtowers(&arena, p1);
    vision_p1.end_tick_update();

    vision_p2.begin_tick_update();
    vision_p2.apply_watchtowers(&arena, p2);
    vision_p2.end_tick_update();

    assert_ne!(vision_p1.visibility_at(28, 38), VisibilityState::Visible);
    assert_ne!(vision_p2.visibility_at(28, 38), VisibilityState::Visible);
}
