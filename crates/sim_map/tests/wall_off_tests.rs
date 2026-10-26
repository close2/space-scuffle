//! Integration tests for building placement invariants, ramp wall-offs,
//! and multi-clearance pathfinding apertures.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown
)]

use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapError, PathRequest, Pathfinder,
    PlacementFailureReason, RampDir, UnitClearance,
};
use sim_types::{EntityId, SimScalar, SimVec2};

#[test]
fn test_building_placement_invariants() {
    let arena = GameMap::default_arena();
    let mut map = arena;

    let building_1 = EntityId(201);
    let building_2 = EntityId(202);

    // 1. Valid placement: 3x3 dome on flat low ground (Cliff 1)
    let res = map.place_structure(building_1, 10, 10, 3, 3);
    assert!(
        res.is_ok(),
        "Placing 3x3 on flat Cliff 1 should succeed: {:?}",
        res
    );

    // 2. Overlapping existing structure: Placing 2x2 at (11, 11) overlaps building_1
    let overlap_res = map.place_structure(building_2, 11, 11, 2, 2);
    assert!(matches!(
        overlap_res,
        Err(MapError::PlacementBlocked {
            reason: PlacementFailureReason::OccupiedByStructure(id),
            ..
        }) if id == building_1
    ));

    // 3. Overlapping a ramp: Ramp is at x in [30..34), y in [30..34)
    let ramp_res = map.place_structure(building_2, 30, 31, 2, 2);
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
    let cliff_res = map.place_structure(building_2, 10, 29, 3, 3);
    assert!(matches!(
        cliff_res,
        Err(MapError::PlacementBlocked {
            reason: PlacementFailureReason::CliffLevelMismatch
                | PlacementFailureReason::UnbuildableTerrain,
            ..
        })
    ));

    // 5. Structure removal restores buildability and pathability
    let removed = map.remove_structure(building_1);
    assert!(removed.is_ok());
    assert_eq!(removed.unwrap().id, building_1);

    // Can now place building_2 at the exact location previously occupied
    let replace_res = map.place_structure(building_2, 10, 10, 3, 3);
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
        clearance: UnitClearance::Small,
    };
    let req_large = PathRequest {
        start: start_pos,
        goal: goal_pos,
        clearance: UnitClearance::Large,
    };

    assert!(pathfinder.find_path(&map, &req_small).is_ok());
    assert!(pathfinder.find_path(&map, &req_large).is_ok());

    // Place complete wall-off at top of ramp:
    // Ramp exit is x in 12..15 at y = 15.
    // Structure 1: 2x2 at (11, 15), covers x=11..13, y=15..17
    // Structure 2: 2x2 at (13, 15), covers x=13..15, y=15..17
    map.place_structure(EntityId(1), 11, 15, 2, 2).unwrap();
    map.place_structure(EntityId(2), 13, 15, 2, 2).unwrap();
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
    map.place_structure(EntityId(2), 14, 16, 2, 2).unwrap();
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
