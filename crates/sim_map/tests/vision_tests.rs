//! Integration tests for Fog of War visibility, high-ground occlusion,
//! line-of-sight blockers, and watchtowers.

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

use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, PlayerVisionMap, VisionSource};
use sim_types::{MoverType, PlayerId, SimScalar, SimVec2, VisibilityState};

#[test]
fn test_low_ground_vs_high_ground_occlusion() {
    let arena = arena::default_arena();
    let mut vision = PlayerVisionMap::new(arena.width(), arena.height());

    let player1 = PlayerId(1);
    // Low-ground observer on Cliff 1 (y = 25), 5 cells south of cliff transition zone (y = 30)
    let ground_observer = VisionSource {
        pos: SimVec2::new(SimScalar::from_num(16.5), SimScalar::from_num(24.5)),
        mover_type: MoverType::Ground,
        sight_radius: SimScalar::from_num(12),
        detection_radius: SimScalar::ZERO,
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
    let arena = arena::default_arena();
    let mut vision = PlayerVisionMap::new(arena.width(), arena.height());

    // Initially low ground observer only
    let ground_observer = VisionSource {
        pos: SimVec2::new(SimScalar::from_num(16.5), SimScalar::from_num(24.5)),
        mover_type: MoverType::Ground,
        sight_radius: SimScalar::from_num(12),
        detection_radius: SimScalar::ZERO,
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
        detection_radius: SimScalar::ZERO,
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
        detection_radius: SimScalar::ZERO,
    };

    vision.begin_tick_update();
    vision.apply_vision_source(&map, &unit_west);
    vision.end_tick_update();

    // Can see up to the smoke vent (x = 7), and none of it: not its edge cell (x = 8), which
    // the ray stops before (plan `2026-11-23-vision-and-detection` D-3), nor its centre (x = 9),
    // nor beyond it to the East (x = 12)
    assert_eq!(vision.visibility_at(7, 9), VisibilityState::Visible);
    assert_ne!(vision.visibility_at(8, 9), VisibilityState::Visible);
    assert_ne!(vision.visibility_at(10, 9), VisibilityState::Visible);
    assert_ne!(vision.visibility_at(12, 9), VisibilityState::Visible);

    // Now move observer INSIDE the smoke vent at (9.5, 9.5)
    let unit_inside = VisionSource {
        pos: SimVec2::new(SimScalar::from_num(9.5), SimScalar::from_num(9.5)),
        mover_type: MoverType::Ground,
        sight_radius: SimScalar::from_num(10),
        detection_radius: SimScalar::ZERO,
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
    let mut arena = arena::default_arena();
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

/// A ground source at cell centre `(x, y)` with the given sight and detection radii.
fn ground_source(x: i32, y: i32, sight: f64, detection: f64) -> VisionSource {
    VisionSource {
        pos: SimVec2::new(
            SimScalar::from_num(f64::from(x) + 0.5),
            SimScalar::from_num(f64::from(y) + 0.5),
        ),
        mover_type: MoverType::Ground,
        sight_radius: SimScalar::from_num(sight),
        detection_radius: SimScalar::from_num(detection),
    }
}

/// A fogged map with `sources` counted by a full recount.
fn recount(map: &GameMap, sources: &[VisionSource]) -> PlayerVisionMap {
    let mut vision = PlayerVisionMap::new(map.width(), map.height());
    vision.set_all_fogged();
    vision.begin_tick_update();
    for source in sources {
        vision.apply_vision_source(map, source);
    }
    vision.end_tick_update();
    vision
}

/// A flat 32x32 map with a 3x3 sight blocker at cells x 20–22, y 14–16.
fn flat_with_blocker() -> GameMap {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0))).unwrap();
    for y in 14..17 {
        for x in 20..23 {
            map.cell_mut(x, y)
                .unwrap()
                .flags
                .insert(CellFlags::LOS_BLOCKER);
        }
    }
    map
}

/// P-4 (plan `2026-11-23-vision-and-detection` D-2): sight and detection are two circles cast
/// along the same rays. A detector seeing 4 and detecting 7 detects cells at 6 it does not
/// see; one seeing 7 and detecting 3 sees cells at 5 it does not detect.
#[test]
fn detection_and_sight_are_separate_circles() {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0))).unwrap();
    let far_detector = recount(&map, &[ground_source(10, 10, 4.0, 7.0)]);
    assert_eq!(far_detector.visibility_at(13, 10), VisibilityState::Visible);
    assert!(far_detector.is_detected(13, 10), "inside both circles");
    assert_ne!(far_detector.visibility_at(16, 10), VisibilityState::Visible);
    assert!(
        far_detector.is_detected(16, 10),
        "detected beyond its sight, within its detection"
    );
    assert!(!far_detector.is_detected(18, 10), "beyond both circles");

    let near_detector = recount(&map, &[ground_source(10, 10, 7.0, 3.0)]);
    assert_eq!(
        near_detector.visibility_at(15, 10),
        VisibilityState::Visible
    );
    assert!(
        !near_detector.is_detected(15, 10),
        "seen beyond its detection, not detected"
    );
    assert!(near_detector.is_detected(12, 10), "inside both circles");
    assert!(near_detector.is_detected(10, 10), "its own cell");

    let plain = recount(&map, &[ground_source(10, 10, 7.0, 0.0)]);
    assert!(
        plain.detector_counter().iter().all(|&count| count == 0),
        "a source without a detection radius detects nothing, not even its own cell"
    );
}

/// P-4: detection follows the detector's line of sight: a ground detector below a plateau
/// detects none of the plateau within its detection circle, and one outside a blocker detects
/// no cell beyond the blocker's edge.
#[test]
fn detection_follows_line_of_sight() {
    let arena = arena::default_arena();
    // The low-ground observer of `test_low_ground_vs_high_ground_occlusion`, detecting 12.
    let below = recount(&arena, &[ground_source(16, 24, 12.0, 12.0)]);
    assert!(below.is_detected(16, 28), "low ground in reach is detected");
    assert!(
        !below.is_detected(16, 35),
        "the plateau the detector cannot see is not detected"
    );

    let map = flat_with_blocker();
    let outside = recount(&map, &[ground_source(15, 15, 11.0, 11.0)]);
    assert!(
        !outside.is_detected(22, 15) && !outside.is_detected(24, 15),
        "a blocker stops detection as it stops sight"
    );
}

/// P-4: with the detection radius equal to the sight radius, the cast detects exactly the cells
/// it sees, on a flat map, around plateaus and ramps (the shipped default arena), and around a
/// sight blocker, and its visibility equals that of the same source without detection: the
/// cast before plan `2026-11-23-vision-and-detection` WP2, cell for cell.
#[test]
fn equal_radii_detect_exactly_the_seen_cells() {
    let flat = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0))).unwrap();
    let cases = [
        (flat, vec![(10, 10), (0, 0), (31, 17)]),
        (
            arena::default_arena(),
            vec![(16, 24), (16, 33), (30, 40), (9, 9)],
        ),
        (
            flat_with_blocker(),
            vec![(15, 15), (21, 15), (21, 14), (27, 20)],
        ),
    ];
    for (map, cells) in &cases {
        for &(x, y) in cells {
            for radius in [1.0, 6.5, 11.0, 13.75] {
                let detector = recount(map, &[ground_source(x, y, radius, radius)]);
                let plain = recount(map, &[ground_source(x, y, radius, 0.0)]);
                assert_eq!(
                    detector.states(),
                    plain.states(),
                    "at ({x}, {y}) r {radius}"
                );
                assert_eq!(
                    detector.visible_counter(),
                    plain.visible_counter(),
                    "at ({x}, {y}) r {radius}"
                );
                assert_eq!(
                    detector.detector_counter(),
                    detector.visible_counter(),
                    "a detector detects exactly what it sees at ({x}, {y}) r {radius}"
                );
            }
        }
    }
}

/// P-4: a sight upgrade, which widens the sight circle alone (5.0.14 changes no `Detect`), leaves
/// the detection circle unchanged.
#[test]
fn an_upgraded_sight_leaves_detection() {
    let map = GameMap::new(40, 40, MapCell::default_ground(CliffLevel(0))).unwrap();
    let base = recount(&map, &[ground_source(20, 20, 11.0, 11.0)]);
    let upgraded = recount(&map, &[ground_source(20, 20, 13.0, 11.0)]);
    assert_eq!(base.detector_counter(), upgraded.detector_counter());
    assert_ne!(base.visible_counter(), upgraded.visible_counter());
}

/// P-5: adding and removing sources whose detection and sight radii differ, on a map with
/// plateaus, ramps and a sight blocker, leaves every state and counter exactly as a full
/// recount does.
#[test]
fn incremental_casts_with_unequal_radii_equal_a_recount() {
    let mut map = arena::default_arena();
    for y in 20..23 {
        for x in 40..43 {
            map.cell_mut(x, y)
                .unwrap()
                .flags
                .insert(CellFlags::LOS_BLOCKER);
        }
    }
    let fixed = ground_source(16, 24, 9.0, 11.0);
    let moving = [
        ground_source(41, 21, 11.0, 6.0),
        ground_source(30, 40, 6.0, 13.75),
        VisionSource {
            mover_type: MoverType::Air,
            ..ground_source(16, 33, 7.0, 10.0)
        },
        ground_source(36, 21, 11.0, 0.0),
    ];
    let mut incremental = recount(&map, &[fixed, moving[0]]);
    let mut sources = vec![fixed, moving[0]];
    for pair in moving.windows(2) {
        incremental.remove_vision_source(&map, &pair[0]);
        incremental.add_vision_source(&map, &pair[1]);
        // A recount that saw every earlier source carries the same fog memory.
        let mut full = recount(&map, &sources);
        full.begin_tick_update();
        full.apply_vision_source(&map, &fixed);
        full.apply_vision_source(&map, &pair[1]);
        full.end_tick_update();
        assert_eq!(incremental.states(), full.states());
        assert_eq!(incremental.visible_counter(), full.visible_counter());
        assert_eq!(incremental.detector_counter(), full.detector_counter());
        sources.push(pair[1]);
    }
}

/// Whether any cell of the 3x3 blocker of [`flat_with_blocker`] is visible or detected.
fn any_blocker_cell_reached(vision: &PlayerVisionMap) -> bool {
    (14..17).any(|y| {
        (20..23).any(|x| {
            vision.visibility_at(x, y) == VisibilityState::Visible || vision.is_detected(x, y)
        })
    })
}

/// P-8 (plan `2026-11-23-vision-and-detection` D-3; Blizzard, *Game Guide: Map Elements*: "Units
/// inside obstructing terrain can't be seen by units or structures on the outside"): a ground
/// source outside a sight blocker marks none of its cells, from any side and at any distance,
/// and nothing beyond it.
#[test]
fn a_blocker_hides_every_cell_from_outside() {
    let map = flat_with_blocker();
    for (x, y) in [
        (15, 15),
        (19, 15),
        (19, 13),
        (23, 17),
        (21, 12),
        (21, 18),
        (26, 10),
    ] {
        let vision = recount(&map, &[ground_source(x, y, 11.0, 11.0)]);
        assert!(
            !any_blocker_cell_reached(&vision),
            "a ground source at ({x}, {y}) sees into the blocker"
        );
    }
    let west = recount(&map, &[ground_source(15, 15, 11.0, 11.0)]);
    assert_eq!(west.visibility_at(19, 15), VisibilityState::Visible);
    assert_ne!(
        west.visibility_at(24, 15),
        VisibilityState::Visible,
        "nor beyond it"
    );
}

/// P-8: a ground source inside a blocker sees the blocker cells its rays cross and the open
/// ground beyond, and a ray that left the blocker stops at a second one.
#[test]
fn a_source_inside_a_blocker_sees_it_and_out() {
    let mut map = flat_with_blocker();
    for y in 14..17 {
        for x in 26..28 {
            map.cell_mut(x, y)
                .unwrap()
                .flags
                .insert(CellFlags::LOS_BLOCKER);
        }
    }
    let inside = recount(&map, &[ground_source(21, 15, 11.0, 11.0)]);
    for y in 14..17 {
        for x in 20..23 {
            assert_eq!(
                inside.visibility_at(x, y),
                VisibilityState::Visible,
                "({x}, {y})"
            );
        }
    }
    assert_eq!(inside.visibility_at(16, 15), VisibilityState::Visible);
    assert_eq!(inside.visibility_at(25, 15), VisibilityState::Visible);
    assert_ne!(
        inside.visibility_at(26, 15),
        VisibilityState::Visible,
        "a second blocker hides itself"
    );
    assert_ne!(inside.visibility_at(29, 15), VisibilityState::Visible);
}

/// P-8: air sources and watchtowers see into a blocker (Q0101 part 1, default (a)).
#[test]
fn air_and_towers_see_into_a_blocker() {
    let map = flat_with_blocker();
    let air = VisionSource {
        mover_type: MoverType::Air,
        ..ground_source(15, 15, 11.0, 0.0)
    };
    let tower = sim_map::watchtower_source(&sim_map::Watchtower {
        id: sim_types::EntityId(9000),
        pos: SimVec2::from_i32(15, 15),
        capture_radius: SimScalar::from_i32(2),
        reveal_radius: SimScalar::from_i32(11),
        controlling_player: Some(PlayerId(0)),
    });
    for source in [air, tower] {
        let vision = recount(&map, &[source]);
        for y in 14..17 {
            for x in 20..23 {
                assert_eq!(
                    vision.visibility_at(x, y),
                    VisibilityState::Visible,
                    "({x}, {y})"
                );
            }
        }
    }
}
