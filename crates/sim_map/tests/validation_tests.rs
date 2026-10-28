//! Integration tests for map validation engine and defect reporting.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown
)]

use sim_map::{
    validate_map, CellFlags, CellRect, CliffLevel, MapDocument, MapEntityId, MapFile, MapResource,
    MapStartLocation, MapValidationError, MapValidationWarning, MapWatchtower,
    PlacementFailureReason, RampDir, ResourceKind, TerrainKind,
};

fn base_doc() -> MapDocument {
    let metadata = sim_map::MapMetadata {
        title: "Base Validation Map".to_string(),
        author: "Space Scuffle Team".to_string(),
        description: "Valid reference document for focused validation tests".to_string(),
        theme: "orbital_foundry".to_string(),
    };
    let mut doc = MapDocument::new_blank(32, 32, metadata).expect("new_blank must succeed");

    // Two well-separated, valid start locations
    doc.start_locations = vec![
        MapStartLocation {
            id: MapEntityId(30),
            player_slot: 0,
            cell: (2, 2),
            footprint: (5, 5),
        },
        MapStartLocation {
            id: MapEntityId(31),
            player_slot: 1,
            cell: (25, 25),
            footprint: (5, 5),
        },
    ];

    doc
}

fn map_path(filename: &str) -> std::path::PathBuf {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir.join("../../assets/maps").join(filename);
    if candidate.exists() {
        candidate
    } else {
        std::path::PathBuf::from("assets/maps").join(filename)
    }
}

#[test]
fn showcase_canyon_is_valid() {
    let file =
        MapFile::load(&map_path("showcase_canyon.scmap.ron")).expect("load showcase_canyon failed");
    let doc = file.decode().expect("decode showcase_canyon failed");
    let report = validate_map(&doc);

    assert_eq!(
        report.errors,
        vec![],
        "showcase_canyon must have zero errors"
    );
    assert!(report.is_playable());
}

#[test]
fn default_arena_file_is_valid() {
    let file =
        MapFile::load(&map_path("default_arena.scmap.ron")).expect("load default_arena failed");
    let doc = file.decode().expect("decode default_arena failed");
    let report = validate_map(&doc);

    assert_eq!(report.errors, vec![], "default_arena must have zero errors");
    assert!(report.is_playable());
}

// -------------------------------------------------------------------------
// 22 Focused Error Tests
// -------------------------------------------------------------------------

#[test]
fn error_01_abyss_cell_traversable() {
    let mut doc = base_doc();
    let cell = doc.cell_mut(10, 10).unwrap();
    cell.terrain = TerrainKind::Abyss;
    cell.flags = CellFlags::GROUND_PATHABLE;

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::AbyssCellTraversable { x: 10, y: 10 }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (10, 10),
            max_inclusive: (10, 10)
        }
    );
}

#[test]
fn error_02_ramp_on_abyss() {
    let mut doc = base_doc();
    let cell = doc.cell_mut(10, 10).unwrap();
    cell.terrain = TerrainKind::Abyss;
    cell.flags = CellFlags::empty();
    cell.ramp = Some(RampDir::North);
    doc.cell_mut(10, 11).unwrap().level = CliffLevel(2);

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![
            MapValidationError::RampOnAbyss { x: 10, y: 10 },
            MapValidationError::RampNotPathable { x: 10, y: 10 },
        ]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (10, 10),
            max_inclusive: (10, 10)
        }
    );
}

#[test]
fn error_03_ramp_at_top_level() {
    let mut doc = base_doc();
    let cell = doc.cell_mut(10, 10).unwrap();
    cell.level = CliffLevel::MAX;
    cell.ramp = Some(RampDir::North);
    cell.flags = CellFlags::GROUND_PATHABLE;

    let report = validate_map(&doc);
    assert_eq!(
        report.errors[0],
        MapValidationError::RampAtTopLevel { x: 10, y: 10 }
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (10, 10),
            max_inclusive: (10, 10)
        }
    );
}

#[test]
fn error_04_ramp_not_pathable() {
    let mut doc = base_doc();
    let cell = doc.cell_mut(10, 10).unwrap();
    cell.ramp = Some(RampDir::North);
    cell.flags = CellFlags::empty();

    let report = validate_map(&doc);
    assert_eq!(
        report.errors[0],
        MapValidationError::RampNotPathable { x: 10, y: 10 }
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (10, 10),
            max_inclusive: (10, 10)
        }
    );
}

#[test]
fn error_05_ramp_buildable() {
    let mut doc = base_doc();
    let cell = doc.cell_mut(10, 10).unwrap();
    cell.ramp = Some(RampDir::North);
    cell.flags = CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE;

    let report = validate_map(&doc);
    assert_eq!(
        report.errors[0],
        MapValidationError::RampBuildable { x: 10, y: 10 }
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (10, 10),
            max_inclusive: (10, 10)
        }
    );
}

#[test]
fn error_06_ramp_run_too_long() {
    let mut doc = base_doc();
    // 9 contiguous north ramp cells from y = 5 to y = 13 at x = 10 (exceeds MAX_RAMP_RUN_CELLS 8)
    for y in 5..=13 {
        let cell = doc.cell_mut(10, y).unwrap();
        cell.ramp = Some(RampDir::North);
        cell.flags = CellFlags::GROUND_PATHABLE;
        cell.level = CliffLevel(1);
    }
    // High landing at (10, 14) level 2
    doc.cell_mut(10, 14).unwrap().level = CliffLevel(2);

    let report = validate_map(&doc);
    assert_eq!(
        report.errors[0],
        MapValidationError::RampRunTooLong {
            x: 10,
            y: 5,
            len: 9
        }
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (10, 5),
            max_inclusive: (10, 5)
        }
    );
}

#[test]
fn error_07_ramp_upper_landing_mismatch() {
    let mut doc = base_doc();
    // 2-long north ramp at (10, 5) and (10, 6) level 1
    for y in 5..=6 {
        let cell = doc.cell_mut(10, y).unwrap();
        cell.ramp = Some(RampDir::North);
        cell.flags = CellFlags::GROUND_PATHABLE;
        cell.level = CliffLevel(1);
    }
    // Upper landing at (10, 7) has level 1 (needs level 2)
    doc.cell_mut(10, 7).unwrap().level = CliffLevel(1);

    let report = validate_map(&doc);
    assert_eq!(
        report.errors[0],
        MapValidationError::RampUpperLandingMismatch { x: 10, y: 6 }
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (10, 6),
            max_inclusive: (10, 6)
        }
    );
}

#[test]
fn error_08_ramp_lower_landing_mismatch() {
    let mut doc = base_doc();
    // 2-long north ramp at (10, 5) and (10, 6) level 1
    for y in 5..=6 {
        let cell = doc.cell_mut(10, y).unwrap();
        cell.ramp = Some(RampDir::North);
        cell.flags = CellFlags::GROUND_PATHABLE;
        cell.level = CliffLevel(1);
    }
    // Upper landing at (10, 7) level 2
    doc.cell_mut(10, 7).unwrap().level = CliffLevel(2);
    // Lower landing at (10, 4) level 2 (needs level 1)
    doc.cell_mut(10, 4).unwrap().level = CliffLevel(2);

    let report = validate_map(&doc);
    assert_eq!(
        report.errors[0],
        MapValidationError::RampLowerLandingMismatch { x: 10, y: 5 }
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (10, 5),
            max_inclusive: (10, 5)
        }
    );
}

#[test]
fn error_09_ramp_run_level_mismatch() {
    let mut doc = base_doc();
    // 2-long north ramp at (10, 5) level 1 and (10, 6) level 2
    let c1 = doc.cell_mut(10, 5).unwrap();
    c1.ramp = Some(RampDir::North);
    c1.flags = CellFlags::GROUND_PATHABLE;
    c1.level = CliffLevel(1);

    let c2 = doc.cell_mut(10, 6).unwrap();
    c2.ramp = Some(RampDir::North);
    c2.flags = CellFlags::GROUND_PATHABLE;
    c2.level = CliffLevel(2);

    // Upper landing at (10, 7) has level 2 (low_level 1 + 1 = 2) so upper landing matches
    doc.cell_mut(10, 7).unwrap().level = CliffLevel(2);

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::RampRunLevelMismatch { x: 10, y: 6 }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (10, 6),
            max_inclusive: (10, 6)
        }
    );
}

#[test]
fn error_10_too_few_start_locations() {
    let mut doc = base_doc();
    doc.start_locations.clear();

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::TooFewStartLocations { count: 0 }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (u32::MAX, u32::MAX)
        }
    );
}

#[test]
fn error_11_player_slot_out_of_range() {
    let mut doc = base_doc();
    doc.start_locations[0].player_slot = 8; // slots must be 0..=7

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::PlayerSlotOutOfRange {
            id: MapEntityId(30),
            slot: 8
        }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (0, 0)
        }
    );
}

#[test]
fn error_12_duplicate_player_slot() {
    let mut doc = base_doc();
    doc.start_locations[1].player_slot = 0; // duplicate of slot 0

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::DuplicatePlayerSlot {
            id: MapEntityId(31),
            slot: 0
        }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (0, 0)
        }
    );
}

#[test]
fn error_13_start_footprint_blocked() {
    let mut doc = base_doc();
    // Block cell (3, 3) inside start location 30's 5x5 footprint [2..7, 2..7]
    doc.cell_mut(3, 3)
        .unwrap()
        .flags
        .remove(CellFlags::BUILDABLE);

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::StartFootprintBlocked {
            id: MapEntityId(30),
            reason: PlacementFailureReason::UnbuildableTerrain,
        }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (0, 0)
        }
    );
}

#[test]
fn error_14_start_locations_too_close() {
    let mut doc = base_doc();
    // Move start 31 to (10, 10). Center distance: (12.5 - 4.5) = 8 cells < 16
    doc.start_locations[1].cell = (10, 10);

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::StartLocationsTooClose {
            a: MapEntityId(30),
            b: MapEntityId(31),
            distance: 8
        }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (0, 0)
        }
    );
}

#[test]
fn error_15_duplicate_entity_id() {
    let mut doc = base_doc();
    // Add resource sharing ID 30 with start location 30
    doc.resources.push(MapResource {
        id: MapEntityId(30),
        kind: ResourceKind::Mineral,
        cell: (10, 10),
        amount: 1500,
    });

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::DuplicateEntityId {
            id: MapEntityId(30)
        }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (0, 0)
        }
    );
}

#[test]
fn error_16_entity_out_of_bounds() {
    let mut doc = base_doc();
    doc.resources.push(MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (35, 10), // x = 35 >= width 32
        amount: 1500,
    });

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::EntityOutOfBounds {
            id: MapEntityId(1),
            x: 35,
            y: 10
        }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (35, 10),
            max_inclusive: (35, 10)
        }
    );
}

#[test]
fn error_17_resource_on_blocked_cell() {
    let mut doc = base_doc();
    doc.resources.push(MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (10, 10),
        amount: 1500,
    });
    // Remove ground pathable flag from (10, 10)
    doc.cell_mut(10, 10).unwrap().flags = CellFlags::empty();

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::ResourceOnBlockedCell { id: MapEntityId(1) }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (0, 0)
        }
    );
}

#[test]
fn error_18_resource_inside_start_footprint() {
    let mut doc = base_doc();
    // Start 30 is at (2, 2) footprint 5x5 -> spans [2..7, 2..7]
    doc.resources.push(MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (3, 3),
        amount: 1500,
    });

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::ResourceInsideStartFootprint {
            id: MapEntityId(1),
            start: MapEntityId(30),
        }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (0, 0)
        }
    );
}

#[test]
fn error_19_entities_overlap() {
    let mut doc = base_doc();
    doc.resources.push(MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (10, 10),
        amount: 1500,
    });
    doc.watchtowers.push(MapWatchtower {
        id: MapEntityId(2),
        cell: (10, 10),
        capture_radius_tenths: 30,
        reveal_radius_tenths: 120,
    });

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::EntitiesOverlap {
            a: MapEntityId(1),
            b: MapEntityId(2),
        }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (0, 0)
        }
    );
}

#[test]
fn error_20_zero_resource_amount() {
    let mut doc = base_doc();
    doc.resources.push(MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (10, 10),
        amount: 0,
    });

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::ZeroResourceAmount { id: MapEntityId(1) }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (0, 0)
        }
    );
}

#[test]
fn error_21_zero_watchtower_radius() {
    let mut doc = base_doc();
    doc.watchtowers.push(MapWatchtower {
        id: MapEntityId(1),
        cell: (10, 10),
        capture_radius_tenths: 0,
        reveal_radius_tenths: 120,
    });

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::ZeroWatchtowerRadius { id: MapEntityId(1) }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (0, 0)
        }
    );
}

#[test]
fn unreachable_when_walled() {
    let mut doc = base_doc();
    // Starts at (2, 2) and (25, 25). Set column x = 16 to Abyss with flags 0
    for y in 0..32 {
        let cell = doc.cell_mut(16, y).unwrap();
        cell.terrain = TerrainKind::Abyss;
        cell.level = CliffLevel(0);
        cell.flags = CellFlags::empty();
        cell.ramp = None;
    }

    let report = validate_map(&doc);
    assert_eq!(
        report.errors,
        vec![MapValidationError::StartLocationsUnreachable {
            a: MapEntityId(30),
            b: MapEntityId(31),
        }]
    );
    assert_eq!(
        report.errors[0].location(),
        CellRect {
            min: (0, 0),
            max_inclusive: (0, 0)
        }
    );
}

// -------------------------------------------------------------------------
// Warnings & Determinism Tests
// -------------------------------------------------------------------------

#[test]
fn narrow_ramp_warning() {
    let mut doc = base_doc();
    // 1-wide north ramp at (10, 5) and (10, 6) level 1
    for y in 5..=6 {
        let cell = doc.cell_mut(10, y).unwrap();
        cell.ramp = Some(RampDir::North);
        cell.flags = CellFlags::GROUND_PATHABLE;
        cell.level = CliffLevel(1);
    }
    doc.cell_mut(10, 7).unwrap().level = CliffLevel(2);

    let report = validate_map(&doc);
    assert!(report.warnings.contains(&MapValidationWarning::NarrowRamp {
        x: 10,
        y: 5,
        width: 1
    }));
    let warn = report
        .warnings
        .iter()
        .find(|w| matches!(w, MapValidationWarning::NarrowRamp { .. }))
        .unwrap();
    assert_eq!(
        warn.location(),
        CellRect {
            min: (10, 5),
            max_inclusive: (10, 5)
        }
    );
}

#[test]
fn diagonal_pinch_warning() {
    let mut doc = base_doc();
    // 2x2 block at (10, 10): (10, 10) and (11, 11) level 2; (11, 10) and (10, 11) level 1
    doc.cell_mut(10, 10).unwrap().level = CliffLevel(2);
    doc.cell_mut(11, 11).unwrap().level = CliffLevel(2);
    doc.cell_mut(11, 10).unwrap().level = CliffLevel(1);
    doc.cell_mut(10, 11).unwrap().level = CliffLevel(1);

    let report = validate_map(&doc);
    assert!(report
        .warnings
        .contains(&MapValidationWarning::DiagonalPinch { x: 10, y: 10 }));
    let warn = report
        .warnings
        .iter()
        .find(|w| matches!(w, MapValidationWarning::DiagonalPinch { .. }))
        .unwrap();
    assert_eq!(
        warn.location(),
        CellRect {
            min: (10, 10),
            max_inclusive: (11, 11)
        }
    );
}

#[test]
fn resource_imbalance_warning() {
    let mut doc = base_doc();
    // Start 0 at (2, 2) gets two resources at (3, 8) and (4, 8) (dist ~ 6 cells <= 12)
    doc.resources.push(MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (3, 8),
        amount: 1500,
    });
    doc.resources.push(MapResource {
        id: MapEntityId(2),
        kind: ResourceKind::Mineral,
        cell: (4, 8),
        amount: 1500,
    });
    // Start 1 at (25, 25) gets one resource at (25, 20) (dist ~ 7 cells <= 12)
    doc.resources.push(MapResource {
        id: MapEntityId(3),
        kind: ResourceKind::Mineral,
        cell: (25, 20),
        amount: 1500,
    });

    let report = validate_map(&doc);
    assert!(report
        .warnings
        .contains(&MapValidationWarning::ResourceImbalance {
            slot: 1,
            count: 1,
            max_count: 2,
        }));
}

#[test]
fn unassigned_resource_warning() {
    let mut doc = base_doc();
    // Place resource at (16, 16) - center of map, distance to (2,2) and (25,25) > 12 cells
    doc.resources.push(MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (16, 16),
        amount: 1500,
    });

    let report = validate_map(&doc);
    assert!(report
        .warnings
        .contains(&MapValidationWarning::UnassignedResource { id: MapEntityId(1) }));
}

#[test]
fn report_order_is_deterministic() {
    let mut doc = base_doc();
    doc.cell_mut(10, 10).unwrap().terrain = TerrainKind::Abyss;
    doc.cell_mut(10, 10).unwrap().flags = CellFlags::GROUND_PATHABLE;
    doc.start_locations[0].player_slot = 8;

    let report1 = validate_map(&doc);
    let report2 = validate_map(&doc);

    assert_eq!(report1.errors, report2.errors);
    assert_eq!(report1.warnings, report2.warnings);
}
