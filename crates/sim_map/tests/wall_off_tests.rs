//! Integration tests for building placement invariants, ramp wall-offs,
//! and multi-clearance pathfinding apertures.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::uninlined_format_args,
    clippy::doc_markdown
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/arena.rs"]
mod arena;

#[path = "support/radii.rs"]
mod radii;
use radii::{LARGE_RADIUS, SMALL_RADIUS};

use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapError, PathRequest, Pathfinder,
    PlacementFailureReason, RampDir,
};
use sim_types::{EntityId, SimScalar, SimVec2};

#[test]
fn test_building_placement_invariants() {
    let arena = arena::default_arena();
    let mut map = arena;

    let building_1 = EntityId(201);
    let building_2 = EntityId(202);

    // 1. Valid placement: 3x3 dome on flat low ground (Cliff 1)
    let res = map.place_structure(
        building_1,
        (10, 10),
        sim_map::StructureFootprint::square(3, 3),
    );
    assert!(
        res.is_ok(),
        "Placing 3x3 on flat Cliff 1 should succeed: {:?}",
        res
    );

    // 2. Overlapping existing structure: Placing 2x2 at (11, 11) overlaps building_1
    let overlap_res = map.place_structure(
        building_2,
        (11, 11),
        sim_map::StructureFootprint::square(2, 2),
    );
    assert!(matches!(
        overlap_res,
        Err(MapError::PlacementBlocked {
            reason: PlacementFailureReason::OccupiedByStructure(id),
            ..
        }) if id == building_1
    ));

    // 3. Overlapping a ramp: Ramp is at x in [30..34), y in [30..34)
    let ramp_res = map.place_structure(
        building_2,
        (30, 31),
        sim_map::StructureFootprint::square(2, 2),
    );
    assert!(matches!(
        ramp_res,
        Err(MapError::PlacementBlocked {
            reason: PlacementFailureReason::OverlapsRamp,
            ..
        })
    ));

    // 4. Cliff level mismatch: Straddling Cliff 1 (y < 30) and Cliff 2 (y >= 34)
    // Map cell (20, 29) is Cliff 1, (20, 34) is Cliff 2; on default_arena y=29 is Cliff 1, y=34 is Cliff 2.
    // Straddling y=29 to y=31 spans Cliff 1 and unpassable cliff boundary, or across cliff boundary
    let cliff_res = map.place_structure(
        building_2,
        (10, 29),
        sim_map::StructureFootprint::square(3, 3),
    );
    // The cliff cells between the levels are not walkable ground, which placement checks first
    // (plan `2026-11-18-contour-footprints` D-5).
    assert!(matches!(
        cliff_res,
        Err(MapError::PlacementBlocked {
            reason: PlacementFailureReason::CliffLevelMismatch
                | PlacementFailureReason::UnbuildableTerrain
                | PlacementFailureReason::UnpathableTerrain,
            ..
        })
    ));

    // 5. Structure removal restores buildability and pathability
    let removed = map.remove_structure(building_1);
    assert!(removed.is_ok());
    assert_eq!(removed.unwrap().id, building_1);

    // Can now place building_2 at the exact location previously occupied
    let replace_res = map.place_structure(
        building_2,
        (10, 10),
        sim_map::StructureFootprint::square(3, 3),
    );
    assert!(replace_res.is_ok());
}

#[test]
fn test_ramp_wall_off_geometry() {
    let width = 30u32;
    let height = 30u32;
    let mut map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(1))).unwrap();

    // High ground on north half (y >= 15)
    for y in 15..height {
        for x in 0..width {
            let cell = map.cell_mut(x, y).unwrap();
            cell.cliff_level = CliffLevel(2);
        }
    }

    // Impassable cliff wall at y = 14 except for 3-cell ramp at x in [12, 15)
    for x in 0..width {
        let cell = map.cell_mut(x, 14).unwrap();
        if (12..15).contains(&x) {
            cell.ramp = Some(RampDir::North);
            cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
        } else {
            cell.flags = CellFlags::UNPASSABLE_CLIFF;
        }
    }

    let mut pathfinder = Pathfinder::new(&map);

    let start_pos = SimVec2::new(SimScalar::from_num(13.5), SimScalar::from_num(8.5)); // Low ground
    let goal_pos = SimVec2::new(SimScalar::from_num(13.5), SimScalar::from_num(22.5)); // High ground

    // Open ramp: both Large and Small units can traverse
    let req_small = PathRequest {
        start: start_pos,
        goal: goal_pos,
        radius: SMALL_RADIUS,
    };
    let req_large = PathRequest {
        start: start_pos,
        goal: goal_pos,
        radius: LARGE_RADIUS,
    };

    assert!(pathfinder.find_path(&map, &req_small).is_ok());
    assert!(pathfinder.find_path(&map, &req_large).is_ok());

    // Place complete wall-off at top of ramp:
    // Ramp exit is x in 12..15 at y = 15.
    // Structure 1: 2x2 at (11, 15), covers x=11..13, y=15..17
    // Structure 2: 2x2 at (13, 15), covers x=13..15, y=15..17
    map.place_structure(
        EntityId(1),
        (11, 15),
        sim_map::StructureFootprint::square(2, 2),
    )
    .unwrap();
    map.place_structure(
        EntityId(2),
        (13, 15),
        sim_map::StructureFootprint::square(2, 2),
    )
    .unwrap();
    pathfinder.recompute_clearance(&map);

    // Fully walled off: neither can path through
    assert!(matches!(
        pathfinder.find_path(&map, &req_small),
        Err(MapError::NoPathFound { .. })
    ));
    assert!(matches!(
        pathfinder.find_path(&map, &req_large),
        Err(MapError::NoPathFound { .. })
    ));

    // Create a 1-cell diagonal choke gap by shifting Structure 2 to (14, 16)
    map.remove_structure(EntityId(2)).unwrap();
    map.place_structure(
        EntityId(2),
        (14, 16),
        sim_map::StructureFootprint::square(2, 2),
    )
    .unwrap();
    pathfinder.recompute_clearance(&map);

    // Diagonal gap at (13, 15) allows Small unit (Marine) through
    let small_path = pathfinder.find_path(&map, &req_small);
    assert!(
        small_path.is_ok(),
        "Small unit should slip through diagonal 1-cell wall-off gap: {:?}",
        small_path
    );

    // Large unit (Siege Tank) requires 2x2 clearance, which is blocked by the 1-cell gap
    let large_path = pathfinder.find_path(&map, &req_large);
    assert!(
        matches!(large_path, Err(MapError::NoPathFound { .. })),
        "Large unit must be blocked by 1-cell gap: {:?}",
        large_path
    );
}

/// A flat map with one placed 3x3 structure at (4, 4), one unbuildable cell and one plateau.
fn placement_map() -> (GameMap, EntityId) {
    let mut cells = vec![MapCell::default_ground(CliffLevel(0)); 400];
    cells[10 * 20 + 10].flags.remove(CellFlags::BUILDABLE);
    cells[15 * 20 + 15] = MapCell::default_ground(CliffLevel(1));
    let mut map = GameMap::from_cells(20, 20, cells, Vec::new(), Vec::new())
        .expect("a 20x20 grid has exactly 400 cells");
    let occupant = EntityId(7);
    map.place_structure(occupant, (4, 4), sim_map::StructureFootprint::square(3, 3))
        .expect("open ground accepts the structure");
    (map, occupant)
}

#[test]
fn a_structure_the_predicate_ignores_does_not_block() {
    let (map, _) = placement_map();
    assert!(map.can_place_building(4, 4, 2, 2).is_err());
    assert!(map
        .can_place_building_where(4, 4, 2, 2, |_, _, _| false)
        .is_ok());
    assert!(map
        .can_place_building_where(3, 3, 3, 3, |_, _, _| false)
        .is_ok());
}

#[test]
fn the_predicate_sees_each_occupied_cell_and_its_occupant() {
    let (map, occupant) = placement_map();
    let result = map.can_place_building_where(5, 5, 3, 3, |x, y, id| {
        assert_eq!(id, occupant);
        (x, y) == (6, 6)
    });
    assert!(matches!(
        result,
        Err(MapError::PlacementBlocked {
            reason: PlacementFailureReason::OccupiedByStructure(id),
            ..
        }) if id == occupant
    ));
}

#[test]
fn the_predicate_never_excuses_terrain() {
    let (map, _) = placement_map();
    let terrain = |x, y, w, h| match map.can_place_building_where(x, y, w, h, |_, _, _| false) {
        Err(MapError::PlacementBlocked { reason, .. }) => Some(reason),
        _ => None,
    };
    assert_eq!(
        terrain(10, 10, 1, 1),
        Some(PlacementFailureReason::UnbuildableTerrain)
    );
    assert_eq!(
        terrain(14, 14, 2, 2),
        Some(PlacementFailureReason::CliffLevelMismatch)
    );
    assert_eq!(
        terrain(19, 19, 2, 2),
        Some(PlacementFailureReason::OutOfBounds)
    );

    let arena = arena::default_arena();
    assert!(matches!(
        arena.can_place_building_where(30, 31, 2, 2, |_, _, _| false),
        Err(MapError::PlacementBlocked {
            reason: PlacementFailureReason::OverlapsRamp,
            ..
        })
    ));
}

#[test]
fn the_terrain_beside_an_ignored_structure_still_counts() {
    let (mut map, _) = placement_map();
    map.place_structure(
        EntityId(9),
        (14, 15),
        sim_map::StructureFootprint::square(1, 1),
    )
    .expect("flat ground beside the plateau accepts the structure");
    assert!(matches!(
        map.can_place_building_where(14, 15, 2, 1, |_, _, _| false),
        Err(MapError::PlacementBlocked {
            reason: PlacementFailureReason::CliffLevelMismatch,
            ..
        })
    ));
}

/// Plan `2026-11-18-contour-footprints` D-5: the ground under a structure is walkable. A cell
/// that is buildable but not walkable refuses the site as `UnpathableTerrain`, before the
/// buildable check, and the predicate that ignores structures never excuses it.
#[test]
fn placement_refuses_unwalkable_ground() {
    let mut cells = vec![MapCell::default_ground(CliffLevel(0)); 400];
    cells[8 * 20 + 9].flags.remove(CellFlags::GROUND_PATHABLE);
    cells[12 * 20 + 12]
        .flags
        .remove(CellFlags::GROUND_PATHABLE | CellFlags::BUILDABLE);
    let mut map = GameMap::from_cells(20, 20, cells, Vec::new(), Vec::new())
        .expect("a 20x20 grid has exactly 400 cells");
    assert!(map.cell(9, 8).unwrap().flags.contains(CellFlags::BUILDABLE));
    let refused =
        |map: &GameMap, x, y| match map.can_place_building_where(x, y, 2, 2, |_, _, _| false) {
            Err(MapError::PlacementBlocked { reason, .. }) => Some(reason),
            _ => None,
        };
    assert_eq!(
        refused(&map, 8, 7),
        Some(PlacementFailureReason::UnpathableTerrain)
    );
    assert_eq!(
        refused(&map, 11, 11),
        Some(PlacementFailureReason::UnpathableTerrain)
    );
    assert_eq!(refused(&map, 2, 2), None);
    assert!(matches!(
        map.place_structure(
            EntityId(5),
            (9, 8),
            sim_map::StructureFootprint::square(1, 1)
        ),
        Err(MapError::PlacementBlocked {
            reason: PlacementFailureReason::UnpathableTerrain,
            ..
        })
    ));
    assert!(map.structures().is_empty());
}

// ---------------------------------------------------------------------------------------------
// Contour walls (plan `2026-11-18-contour-footprints` WP3, §2.2 rule 3, S9 and S10).
// ---------------------------------------------------------------------------------------------

/// The side of the contour-wall maps, in cells.
const WALL_SIDE: u32 = 24;

/// A world unit fraction `k / 256` as a radius.
fn radius_256(k: i64) -> SimScalar {
    SimScalar::from_raw(k << 24)
}

fn cell_centre(x: u32, y: u32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

/// A 2×2, 3×3 or 5×5 footprint with its 5.0.14 contour (§2.3): corners cut by 0.5, 0.5, 1.5.
fn contour(size: u8) -> sim_map::StructureFootprint {
    let chamfer_q = if size == 5 { 6 } else { 2 };
    sim_map::StructureFootprint {
        width: size,
        height: size,
        shape: sim_types::FootprintShape::Box {
            half_width_q: 2 * u16::from(size),
            half_height_q: 2 * u16::from(size),
            chamfer_q,
        },
    }
}

/// The staircase map of `clearance_field_tests::f7_no_tier_passes_between_structures_touching_
/// at_a_corner` (terrain blocks `(i, i)` and `(i + 1, i)` below cell 10 and from cell 14), with
/// two 2×2 contours at cells (10, 10) and (12, 12). `mirror` reflects the layout across the
/// anti-diagonal, `(x, y) -> (23 - y, 23 - x)`. Returns the map, the start and the goal.
fn diagonal_depot_wall(mirror: bool) -> (GameMap, SimVec2, SimVec2) {
    let flip = |(x, y): (u32, u32)| {
        if mirror {
            (WALL_SIDE - 1 - y, WALL_SIDE - 1 - x)
        } else {
            (x, y)
        }
    };
    let mut map = GameMap::new(WALL_SIDE, WALL_SIDE, MapCell::default_ground(CliffLevel(1)))
        .expect("invariant: a positive side is a valid map size");
    let mut blocked = Vec::new();
    for i in (0..10).chain(14..WALL_SIDE) {
        blocked.push((i, i));
        if i + 1 < WALL_SIDE {
            blocked.push((i + 1, i));
        }
    }
    for cell in blocked {
        let (x, y) = flip(cell);
        map.cell_mut(x, y).expect("invariant: on the map").flags = CellFlags::empty();
    }
    // A 2×2 at cells (x, y) to (x + 1, y + 1) mirrors to the 2×2 whose minimum cell is the
    // image of (x + 1, y + 1).
    for (id, cell) in [(1, (10, 10)), (2, (12, 12))] {
        let origin = if mirror {
            flip((cell.0 + 1, cell.1 + 1))
        } else {
            cell
        };
        map.place_structure(EntityId(id), origin, contour(2))
            .expect("invariant: open ground");
    }
    let (start, goal) = (flip((3, 18)), flip((18, 3)));
    (
        map,
        cell_centre(start.0, start.1),
        cell_centre(goal.0, goal.1),
    )
}

/// A wall of terrain bands and the given contours separating the north-west of a 32-cell map
/// from its south-east, with a gap only where the structures leave one. Each band is the cells
/// within 2 of a diagonal, running from `low` (the south-west corner of the lowest structure)
/// to the map's south-west and from `high` (the north-east corner of the highest) to its
/// north-east, so a chamfered corner's pocket is closed by the band's faces.
fn banded_wall(
    structures: &[((u32, u32), sim_map::StructureFootprint)],
    low: (i64, i64),
    high: (i64, i64),
) -> GameMap {
    let side = 32_u32;
    let mut map = GameMap::new(side, side, MapCell::default_ground(CliffLevel(1)))
        .expect("invariant: a positive side is a valid map size");
    let inside = |x: i64, y: i64| {
        structures.iter().any(|&((left, bottom), footprint)| {
            (i64::from(left)..i64::from(left) + i64::from(footprint.width)).contains(&x)
                && (i64::from(bottom)..i64::from(bottom) + i64::from(footprint.height)).contains(&y)
        })
    };
    for y in 0..i64::from(side) {
        for x in 0..i64::from(side) {
            let below = (x - y - (low.0 - low.1)).abs() <= 2 && x + y <= low.0 + low.1;
            let above = (x - y - (high.0 - high.1)).abs() <= 2 && x + y >= high.0 + high.1 - 2;
            if (below || above) && !inside(x, y) {
                let (cx, cy) = (u32::try_from(x).unwrap_or(0), u32::try_from(y).unwrap_or(0));
                map.cell_mut(cx, cy).expect("invariant: on the map").flags = CellFlags::empty();
            }
        }
    }
    for (index, &(cell, footprint)) in structures.iter().enumerate() {
        map.place_structure(
            EntityId(u32::try_from(index).unwrap_or(0) + 1),
            cell,
            footprint,
        )
        .expect("invariant: open ground");
    }
    map
}

/// Whether a disc of `radius` routes from the north-west corner region to the south-east one,
/// and every leg of the route passes the segment predicate at that radius.
fn routes(map: &GameMap, start: SimVec2, goal: SimVec2, radius: SimScalar) -> bool {
    let mut pathfinder = Pathfinder::new(map);
    let request = PathRequest {
        start,
        goal,
        radius,
    };
    match pathfinder.find_path(map, &request) {
        Ok(path) => {
            for pair in path.waypoints.windows(2) {
                assert!(
                    pathfinder.segment_is_passable(map, &request.into(), pair[0], pair[1]),
                    "leg {pair:?} at {radius:?}"
                );
            }
            true
        }
        Err(MapError::NoPathFound { .. }) => false,
        Err(other) => panic!("unexpected routing error {other:?}"),
    }
}

/// S10, the owner's case (Q0087): two 2×2 contours corner to corner leave a diagonal gap of
/// `1/√2 ≈ 0.707`. A worker (terrain radius 0.3125) passes; the Zergling analogue (0.375)
/// does not. The same holds mirrored across the anti-diagonal.
#[test]
fn a_diagonal_contour_wall_passes_a_worker_and_stops_a_zergling() {
    for mirror in [false, true] {
        let (map, start, goal) = diagonal_depot_wall(mirror);
        assert!(
            routes(&map, start, goal, radius_256(80)),
            "worker, mirror {mirror}"
        );
        assert!(
            !routes(&map, start, goal, radius_256(96)),
            "zergling, mirror {mirror}"
        );
    }
}

/// The gap admits exactly the discs that fit: for `t = k / 256`, `k` in 64 to 115, a route
/// exists iff `2t ≤ √2 / 2`, that is `8t² ≤ 1`.
#[test]
fn a_diagonal_contour_gap_admits_exactly_the_discs_that_fit() {
    let (map, start, goal) = diagonal_depot_wall(false);
    for k in 64..=115_i64 {
        let fits = 8 * k * k <= 256 * 256;
        assert_eq!(
            routes(&map, start, goal, radius_256(k)),
            fits,
            "t = {k}/256"
        );
    }
}

/// S9: a 5×5 and a 3×3 corner to corner leave a gap of √2: a Stalker-sized disc (0.6875)
/// passes, a tank-sized one (0.75) does not.
#[test]
fn a_5x5_and_a_3x3_corner_to_corner_pass_medium_units() {
    let map = banded_wall(
        &[((8, 8), contour(5)), ((13, 13), contour(3))],
        (8, 8),
        (16, 16),
    );
    let (start, goal) = (cell_centre(3, 28), cell_centre(28, 3));
    assert!(routes(&map, start, goal, radius_256(176)));
    assert!(!routes(&map, start, goal, radius_256(192)));
}

/// S9: a 3×3 sharing one cell of border with a 5×5's face, corner beside chamfer, is a
/// semi-wall: the 3×3's chamfer vertex is `1/√2` from the 5×5's chamfer, so 0.3125 passes and
/// 0.375 does not. Sharing two cells, the faces meet: no route even at 0.0625.
#[test]
fn a_5x5_sharing_one_cell_of_border_is_a_semi_wall_and_two_cells_is_tight() {
    let (start, goal) = (cell_centre(3, 28), cell_centre(28, 3));
    let one = banded_wall(
        &[((8, 8), contour(5)), ((13, 12), contour(3))],
        (8, 8),
        (16, 15),
    );
    assert!(routes(&one, start, goal, radius_256(80)));
    assert!(!routes(&one, start, goal, radius_256(96)));
    let two = banded_wall(
        &[((8, 8), contour(5)), ((13, 11), contour(3))],
        (8, 8),
        (16, 14),
    );
    assert!(!routes(&two, start, goal, radius_256(16)));
}

/// S9: buildings side by side share a face and completely block units.
#[test]
fn face_adjacent_contours_are_tight() {
    let (start, goal) = (cell_centre(3, 28), cell_centre(28, 3));
    let map = banded_wall(
        &[((10, 10), contour(3)), ((13, 10), contour(3))],
        (10, 10),
        (16, 13),
    );
    assert!(!routes(&map, start, goal, radius_256(16)));
}

/// An FNV-1a digest of every waypoint's raw coordinates of the routes through the diagonal
/// depot wall for `t = k / 256`, `k` in `ks`, both orientations.
fn route_bytes_digest(ks: std::ops::RangeInclusive<i64>) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut eat = |value: i64| {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for mirror in [false, true] {
        let (map, start, goal) = diagonal_depot_wall(mirror);
        for k in ks.clone() {
            let request = PathRequest {
                start,
                goal,
                radius: radius_256(k),
            };
            let route = Pathfinder::new(&map)
                .find_route_mesh(&map, &request.into())
                .expect("invariant: these discs fit the gap");
            assert!(!route.fallback, "t = {k}/256, mirror {mirror}");
            for p in &route.waypoints {
                eat(p.x.raw());
                eat(p.y.raw());
            }
        }
    }
    hash
}

/// The route bytes of [`route_bytes_digest`] for `k` in 64 to 84, recorded on the tree before the
/// fitted retry of plan `2026-11-18-contour-footprints` Amendment 1.
const ROUTES_WITH_ROOM_FOR_THE_MARGIN: u64 = 0xfeff_042e_30b7_7389;

/// Amendment 1 guard: a channel whose spare width `w - 2t` is at least one corner margin (here
/// `k ≤ 84`, spare 0.0509 and more) never reaches the fitted retry, so its route is byte for byte
/// the route before the retry existed.
#[test]
fn a_channel_with_room_for_the_margin_keeps_its_route() {
    assert_eq!(
        route_bytes_digest(64..=84),
        ROUTES_WITH_ROOM_FOR_THE_MARGIN,
        "digest {:#x}",
        route_bytes_digest(64..=84)
    );
}

/// Amendment 1: a channel whose spare width is a quarter of the corner margin. The disc fits the
/// 2×2 diagonal gap (`1/√2`) with `w - 2t = CORNER_SAFETY_MARGIN / 4`; the shrunk and pinned
/// pulls at the full margin cannot pass it, so the fitted retry does: the route is no fallback
/// and every leg passes the segment predicate.
#[test]
fn the_fitted_retry_passes_a_channel_with_a_quarter_margin_to_spare() {
    // √2 / 2 in raw Q32.32 units, rounded down: 3,037,000,499.97...
    let gap = SimScalar::from_raw(3_037_000_499);
    let spare = SimScalar::from_raw(sim_map::CORNER_SAFETY_MARGIN.raw() / 4);
    let radius = (gap - spare) / 2;
    for mirror in [false, true] {
        let (map, start, goal) = diagonal_depot_wall(mirror);
        let request = PathRequest {
            start,
            goal,
            radius,
        };
        let mut pathfinder = Pathfinder::new(&map);
        let route = pathfinder
            .find_route_mesh(&map, &request.into())
            .expect("invariant: the disc fits the gap");
        assert!(!route.fallback, "mirror {mirror}: {route:?}");
        for pair in route.waypoints.windows(2) {
            assert!(
                pathfinder.segment_is_passable(&map, &request.into(), pair[0], pair[1]),
                "leg {pair:?}"
            );
        }
    }
}
