//! Tests for the map file gate: bounds, round trips, terrain flags and legacy removal.
//!
//! # Responsibility
//! `validate_map` and the document conversions are the boundary between untrusted map files
//! and authoritative state. These tests pin that hostile documents produce typed reports
//! rather than arithmetic overflow, and that data survives the trip through the format.
//!
//! # How it fits
//! The editor and scenario loader both rely on this gate, so a panic here is a crash on open.
//!
//! # Invariants
//! Hostile values are written into an otherwise valid blank document, so each test isolates
//! one defect. Tests run in debug and release because the historic failures differ by profile.

use sim_map::{
    validate_map, CellFlags, CliffLevel, GameMap, MapCell, MapDocument, MapEntityId, MapMetadata,
    MapResource, MapStartLocation, MapValidationError, MapWatchtower, RampDir, ResourceKind,
    Watchtower,
};
use sim_types::{EntityId, SimScalar, SimVec2};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn blank(side: u32) -> Result<MapDocument, Box<dyn std::error::Error>> {
    Ok(MapDocument::new_blank(side, side, MapMetadata::default())?)
}

fn valid_start(id: u32, slot: u8, cell: (u32, u32)) -> MapStartLocation {
    MapStartLocation {
        id: MapEntityId(id),
        player_slot: slot,
        cell,
        footprint: (5, 5),
    }
}

fn watchtower(id: u32, cell: (u32, u32), capture: u16, reveal: u16) -> MapWatchtower {
    MapWatchtower {
        id: MapEntityId(id),
        cell,
        capture_radius_tenths: capture,
        reveal_radius_tenths: reveal,
    }
}

fn out_of_bounds(report: &[MapValidationError], id: u32) -> bool {
    report.iter().any(|error| {
        matches!(error, MapValidationError::EntityOutOfBounds { id: found, .. } if found.0 == id)
    })
}

#[test]
fn f5_huge_resource_cell_reports_out_of_bounds() -> TestResult {
    let mut doc = blank(32)?;
    doc.resources.push(MapResource {
        id: MapEntityId(7),
        kind: ResourceKind::Mineral,
        cell: (u32::MAX, u32::MAX),
        amount: 100,
    });
    assert!(out_of_bounds(&validate_map(&doc).errors, 7));
    Ok(())
}

#[test]
fn f5_huge_watchtower_cell_reports_out_of_bounds() -> TestResult {
    let mut doc = blank(32)?;
    doc.watchtowers
        .push(watchtower(8, (u32::MAX, 2_147_483_648), 30, 120));
    assert!(out_of_bounds(&validate_map(&doc).errors, 8));
    Ok(())
}

#[test]
fn f5_huge_start_cell_reports_out_of_bounds() -> TestResult {
    let mut doc = blank(32)?;
    doc.start_locations
        .push(valid_start(9, 0, (u32::MAX, u32::MAX)));
    doc.start_locations.push(valid_start(10, 1, (20, 20)));
    assert!(out_of_bounds(&validate_map(&doc).errors, 9));
    Ok(())
}

#[test]
fn f5_huge_resource_with_a_start_location_does_not_overflow_association() -> TestResult {
    let mut doc = blank(32)?;
    doc.start_locations.push(valid_start(1, 0, (2, 2)));
    doc.start_locations.push(valid_start(2, 1, (22, 22)));
    doc.resources.push(MapResource {
        id: MapEntityId(3),
        kind: ResourceKind::Gas,
        cell: (u32::MAX, 0),
        amount: 10,
    });
    assert!(out_of_bounds(&validate_map(&doc).errors, 3));
    Ok(())
}

#[test]
fn f5_mismatched_cell_grid_is_reported_not_indexed() -> TestResult {
    let mut doc = blank(32)?;
    doc.cells.truncate(10);
    let report = validate_map(&doc);
    assert!(
        report
            .errors
            .iter()
            .any(|error| matches!(error, MapValidationError::CellGridMismatch { .. })),
        "got {:?}",
        report.errors
    );
    Ok(())
}

#[test]
fn f5_to_game_map_rejects_out_of_bounds_entities() -> TestResult {
    let mut doc = blank(32)?;
    doc.watchtowers.push(watchtower(u32::MAX, (4, 4), 30, 120));
    assert!(
        doc.to_game_map().is_err(),
        "entity id overflow must be typed"
    );

    let mut doc = blank(32)?;
    doc.resources.push(MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (40, 40),
        amount: 1,
    });
    assert!(
        doc.to_game_map().is_err(),
        "out-of-bounds cell must be typed"
    );
    Ok(())
}

#[test]
fn f5_placement_near_u32_max_reports_out_of_bounds() -> TestResult {
    let map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1)))?;
    assert!(map.can_place_building(u32::MAX, u32::MAX, 3, 3).is_err());
    Ok(())
}

#[test]
fn f6_every_authorable_tenth_round_trips() -> TestResult {
    let mut doc = blank(16)?;
    doc.watchtowers = (1..=u16::MAX)
        .map(|tenths| watchtower(u32::from(tenths), (4, 4), tenths, tenths))
        .collect();
    let map = doc.to_game_map()?;
    let back = MapDocument::from_game_map(&map, MapMetadata::default())?;
    let restored: Vec<u16> = back
        .watchtowers
        .iter()
        .map(|tower| tower.capture_radius_tenths)
        .collect();
    let original: Vec<u16> = (1..=u16::MAX).collect();
    assert_eq!(restored, original);
    Ok(())
}

#[test]
fn f6_radii_that_are_not_tenths_are_still_rejected() -> TestResult {
    let tower = Watchtower {
        id: EntityId(1),
        pos: SimVec2::from_i32(4, 4),
        capture_radius: SimScalar::from_num(1.23),
        reveal_radius: SimScalar::from_num(12),
        controlling_player: None,
    };
    let cells = vec![MapCell::default_ground(CliffLevel(1)); 32 * 32];
    let map = GameMap::from_cells(32, 32, cells, vec![], vec![tower])?;
    assert!(MapDocument::from_game_map(&map, MapMetadata::default()).is_err());
    Ok(())
}

#[test]
fn f10_removing_a_structure_restores_the_cell_flags_it_found() -> TestResult {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1)))?;
    // Buildable but not walkable, for example a pad over a chasm.
    map.cell_mut(5, 5)?.flags = CellFlags::BUILDABLE;
    let speed = CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE | CellFlags::SPEED_ZONE;
    map.cell_mut(6, 5)?.flags = speed;

    map.place_structure(EntityId(1), 5, 5, 2, 2)?;
    map.remove_structure(EntityId(1))?;

    assert_eq!(
        map.cell(5, 5)?.flags,
        CellFlags::BUILDABLE,
        "fabricated pathability"
    );
    assert_eq!(map.cell(6, 5)?.flags, speed);
    assert_eq!(map.cell(5, 6)?.occupying_structure, None);
    Ok(())
}

fn ramp_document() -> Result<MapDocument, Box<dyn std::error::Error>> {
    let mut doc = blank(32)?;
    for x in 0..32 {
        for y in 16..32 {
            doc.cell_mut(x, y).ok_or("cell")?.level = CliffLevel(2);
        }
    }
    let ramp = doc.cell_mut(10, 15).ok_or("cell")?;
    ramp.ramp = Some(RampDir::North);
    ramp.flags = CellFlags::GROUND_PATHABLE;
    Ok(doc)
}

#[test]
fn f11_control_a_legal_ramp_has_no_landing_error() -> TestResult {
    let report = validate_map(&ramp_document()?);
    assert!(!report.errors.iter().any(|error| matches!(
        error,
        MapValidationError::RampUpperLandingMismatch { .. }
            | MapValidationError::RampLowerLandingMismatch { .. }
    )));
    Ok(())
}

#[test]
fn f11_upper_landing_must_be_ground_pathable() -> TestResult {
    let mut doc = ramp_document()?;
    doc.cell_mut(10, 16).ok_or("cell")?.flags = CellFlags::empty();
    let report = validate_map(&doc);
    assert!(
        report
            .errors
            .contains(&MapValidationError::RampUpperLandingMismatch { x: 10, y: 15 }),
        "got {:?}",
        report.errors
    );
    Ok(())
}

#[test]
fn f11_lower_landing_must_be_ground_pathable() -> TestResult {
    let mut doc = ramp_document()?;
    doc.cell_mut(10, 14).ok_or("cell")?.flags = CellFlags::empty();
    let report = validate_map(&doc);
    assert!(
        report
            .errors
            .contains(&MapValidationError::RampLowerLandingMismatch { x: 10, y: 15 }),
        "got {:?}",
        report.errors
    );
    Ok(())
}
