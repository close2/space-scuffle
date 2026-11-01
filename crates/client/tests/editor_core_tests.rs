//! Pure logic integration tests for the Space Scuffle map editor core.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §6.5 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this test suite validates editor commands, undo/redo history limits, drag stroke deduplication,
//! brush footprint geometry with zero per-frame allocation, ramp landing inference with symmetry,
//! entity tool workflows, town hall start footprint catalog resolution, and symmetry transformations
//! with pure-logic tests that require zero GPU or display server access.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::manual_range_contains,
    clippy::assert_is_empty
)]

use client::editor::commands::{
    CellDelta, ChangeSet, EditCommand, History, MapEntity, MAX_UNDO_COMMANDS,
};
use client::editor::document::EditorDocument;
use client::editor::file_browser::{FileBrowserMode, FileBrowserState};
use client::editor::stroke::StrokeRecorder;
use client::editor::symmetry::SymmetryMode;
use client::editor::tools::{
    box_fill_command, brush_footprint, check_ramp_placement, delete_entity_command,
    erase_entities_command, infer_ramp, move_entity_command, paint_brush_stamp, pick_entity,
    place_entity_command, place_ramp_command, resolve_start_footprint, AuthorableFlag, BrushShape,
    EntityToolKind, PaintOp, RampPlacement, RampPlacementError, StartFootprintError,
    DEFAULT_GAS_AMOUNT, DEFAULT_MINERAL_AMOUNT, DEFAULT_WATCHTOWER_CAPTURE_RADIUS_TENTHS,
    DEFAULT_WATCHTOWER_REVEAL_RADIUS_TENTHS,
};
use client::editor::ui::{
    apply_unsaved_choice, EditorToolState, NewMapModalState, PendingUnsavedAction,
    UnsavedChangesModalState, UnsavedChoice, UnsavedOutcome,
};
use client::terrain::TerrainState;
use sim_core::catalog::UnitCatalog;
use sim_core::scenario::StartLoadoutCatalog;
use sim_map::grid::{
    CellFlags, CliffLevel, RampDir, ResourceKind, TerrainKind, MAX_RAMP_RUN_CELLS,
};
use sim_map::map_format::{
    MapDocument, MapEntityId, MapMetadata, MapResource, MapStartLocation, MapWatchtower,
};
use sim_map::validation::CellRect;

fn fixture_document(width: u32, height: u32) -> MapDocument {
    let metadata = MapMetadata {
        title: "Test Map".to_string(),
        author: "Editor Test Suite".to_string(),
        description: "Fixture map for editor core validation".to_string(),
        theme: "orbital_foundry".to_string(),
    };
    MapDocument::new_blank(width, height, metadata).expect("invariant: valid fixture dimensions")
}

#[test]
fn test_history_paint_cells_roundtrip() {
    let mut doc = fixture_document(16, 16);
    let initial_doc = doc.clone();

    let mut history = History::new();
    let mut cs = ChangeSet::empty();

    let cell_before_a = doc.cell(2, 3).expect("cell exists").clone();
    let mut cell_after_a = cell_before_a.clone();
    cell_after_a.level = CliffLevel(3);

    let cell_before_b = doc.cell(4, 5).expect("cell exists").clone();
    let mut cell_after_b = cell_before_b.clone();
    cell_after_b.terrain = TerrainKind::MetalPlate;

    let cmd = EditCommand::PaintCells {
        label: "Paint Test",
        deltas: vec![
            CellDelta {
                x: 2,
                y: 3,
                before: cell_before_a,
                after: cell_after_a.clone(),
            },
            CellDelta {
                x: 4,
                y: 5,
                before: cell_before_b,
                after: cell_after_b.clone(),
            },
        ],
    };

    history.execute(cmd, &mut doc, &mut cs);

    assert_eq!(
        cs.cells,
        Some(CellRect {
            min: (2, 3),
            max_inclusive: (4, 5)
        })
    );
    assert!(!cs.entities_changed);
    assert!(!cs.metadata_changed);
    assert_ne!(doc, initial_doc);
    assert_eq!(doc.cell(2, 3).expect("cell exists").level, CliffLevel(3));
    assert_eq!(
        doc.cell(4, 5).expect("cell exists").terrain,
        TerrainKind::MetalPlate
    );

    let painted_doc = doc.clone();

    // 1. Undo restores bit-for-bit exact initial document
    let mut cs_undo = ChangeSet::empty();
    let undone = history.undo(&mut doc, &mut cs_undo);
    assert!(undone);
    assert_eq!(doc, initial_doc);
    assert_eq!(
        cs_undo.cells,
        Some(CellRect {
            min: (2, 3),
            max_inclusive: (4, 5)
        })
    );

    // 2. Redo restores bit-for-bit exact painted document
    let mut cs_redo = ChangeSet::empty();
    let redone = history.redo(&mut doc, &mut cs_redo);
    assert!(redone);
    assert_eq!(doc, painted_doc);
    assert_eq!(
        cs_redo.cells,
        Some(CellRect {
            min: (2, 3),
            max_inclusive: (4, 5)
        })
    );
}

#[test]
fn test_history_entities_lifecycle_roundtrip() {
    let mut doc = fixture_document(16, 16);
    let initial_doc = doc.clone();

    let mut history = History::new();

    let res = MapEntity::Resource(MapResource {
        id: MapEntityId(10),
        kind: ResourceKind::Mineral,
        cell: (3, 3),
        amount: 1500,
    });
    let tower = MapEntity::Watchtower(MapWatchtower {
        id: MapEntityId(20),
        cell: (8, 8),
        capture_radius_tenths: 30,
        reveal_radius_tenths: 120,
    });
    let start = MapEntity::StartLocation(MapStartLocation {
        id: MapEntityId(30),
        player_slot: 0,
        cell: (1, 1),
        footprint: (5, 5),
    });

    let add_cmd = EditCommand::AddEntities {
        entities: vec![res.clone(), tower.clone(), start.clone()],
    };

    let mut cs = ChangeSet::empty();
    history.execute(add_cmd, &mut doc, &mut cs);
    assert!(cs.entities_changed);
    assert_eq!(doc.resources.len(), 1);
    assert_eq!(doc.watchtowers.len(), 1);
    assert_eq!(doc.start_locations.len(), 1);

    let doc_with_entities = doc.clone();

    // Undo add entities
    let mut cs_undo = ChangeSet::empty();
    assert!(history.undo(&mut doc, &mut cs_undo));
    assert!(cs_undo.entities_changed);
    assert_eq!(doc, initial_doc);

    // Redo add entities
    let mut cs_redo = ChangeSet::empty();
    assert!(history.redo(&mut doc, &mut cs_redo));
    assert_eq!(doc, doc_with_entities);

    // Test RemoveEntities
    let remove_cmd = EditCommand::RemoveEntities {
        entities: vec![res, tower, start],
    };
    let mut cs_rm = ChangeSet::empty();
    history.execute(remove_cmd, &mut doc, &mut cs_rm);
    assert!(cs_rm.entities_changed);
    assert_eq!(doc, initial_doc);

    // Undo remove entities restores them
    assert!(history.undo(&mut doc, &mut ChangeSet::empty()));
    assert_eq!(doc, doc_with_entities);
}

#[test]
fn test_history_modify_entity_roundtrip() {
    let mut doc = fixture_document(16, 16);
    let res = MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (4, 4),
        amount: 1500,
    };
    doc.resources.push(res.clone());
    let initial_doc = doc.clone();

    let mut history = History::new();
    let mut modified = res.clone();
    modified.cell = (6, 6);
    modified.amount = 1200;

    let mod_cmd = EditCommand::ModifyEntity {
        before: MapEntity::Resource(res),
        after: MapEntity::Resource(modified),
    };

    let mut cs = ChangeSet::empty();
    history.execute(mod_cmd, &mut doc, &mut cs);
    assert!(cs.entities_changed);
    assert_eq!(doc.resources[0].cell, (6, 6));
    assert_eq!(doc.resources[0].amount, 1200);

    let modified_doc = doc.clone();

    // Undo restores original
    assert!(history.undo(&mut doc, &mut ChangeSet::empty()));
    assert_eq!(doc, initial_doc);

    // Redo restores modified
    assert!(history.redo(&mut doc, &mut ChangeSet::empty()));
    assert_eq!(doc, modified_doc);
}

#[test]
fn test_history_modify_entities_batch_roundtrip() {
    let mut doc = fixture_document(16, 16);
    let res_a = MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (2, 2),
        amount: 1500,
    };
    let res_b = MapResource {
        id: MapEntityId(2),
        kind: ResourceKind::Mineral,
        cell: (13, 13),
        amount: 1500,
    };
    doc.resources.push(res_a.clone());
    doc.resources.push(res_b.clone());
    let initial_doc = doc.clone();

    let mut history = History::new();
    let mut mod_a = res_a.clone();
    mod_a.cell = (3, 3);
    let mut mod_b = res_b.clone();
    mod_b.cell = (12, 12);

    let cmd = EditCommand::ModifyEntities {
        modifications: vec![
            (MapEntity::Resource(res_a), MapEntity::Resource(mod_a)),
            (MapEntity::Resource(res_b), MapEntity::Resource(mod_b)),
        ],
    };

    history.execute(cmd, &mut doc, &mut ChangeSet::empty());
    assert_eq!(doc.resources[0].cell, (3, 3));
    assert_eq!(doc.resources[1].cell, (12, 12));
    let after_doc = doc.clone();

    assert!(history.undo(&mut doc, &mut ChangeSet::empty()));
    assert_eq!(doc, initial_doc);

    assert!(history.redo(&mut doc, &mut ChangeSet::empty()));
    assert_eq!(doc, after_doc);
}

#[test]
fn test_history_set_metadata_roundtrip() {
    let mut doc = fixture_document(16, 16);
    let initial_doc = doc.clone();

    let mut history = History::new();
    let new_meta = MapMetadata {
        title: "Altered Canyon".to_string(),
        author: "Chief Architect".to_string(),
        description: "Updated description text.".to_string(),
        theme: "arid_badlands".to_string(),
    };

    let cmd = EditCommand::SetMetadata {
        before: doc.metadata.clone(),
        after: new_meta.clone(),
    };

    let mut cs = ChangeSet::empty();
    history.execute(cmd, &mut doc, &mut cs);
    assert!(cs.metadata_changed);
    assert_eq!(doc.metadata, new_meta);

    let meta_doc = doc.clone();

    // Undo restores initial metadata
    assert!(history.undo(&mut doc, &mut ChangeSet::empty()));
    assert_eq!(doc, initial_doc);

    // Redo restores new metadata
    assert!(history.redo(&mut doc, &mut ChangeSet::empty()));
    assert_eq!(doc, meta_doc);
}

#[test]
fn test_history_redo_cleared_on_new_command() {
    let mut doc = fixture_document(16, 16);
    let mut history = History::new();

    let cmd1 = EditCommand::SetMetadata {
        before: doc.metadata.clone(),
        after: MapMetadata {
            title: "First".to_string(),
            ..doc.metadata.clone()
        },
    };
    history.execute(cmd1, &mut doc, &mut ChangeSet::empty());
    assert_eq!(history.undo_len(), 1);

    assert!(history.undo(&mut doc, &mut ChangeSet::empty()));
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 1);
    assert!(history.can_redo());

    let cmd2 = EditCommand::SetMetadata {
        before: doc.metadata.clone(),
        after: MapMetadata {
            title: "Second".to_string(),
            ..doc.metadata.clone()
        },
    };
    history.execute(cmd2, &mut doc, &mut ChangeSet::empty());
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.redo_len(), 0);
    assert!(!history.can_redo());
}

#[test]
fn test_history_command_count_eviction() {
    let mut doc = fixture_document(16, 16);
    let mut history = History::with_limits(3, 1_000_000);

    for i in 1..=4 {
        let cmd = EditCommand::SetMetadata {
            before: doc.metadata.clone(),
            after: MapMetadata {
                title: format!("Title {i}"),
                ..doc.metadata.clone()
            },
        };
        history.execute(cmd, &mut doc, &mut ChangeSet::empty());
    }

    assert_eq!(history.undo_len(), 3);
    assert_eq!(history.total_deltas(), 0);

    // Undoing 3 commands succeeds
    assert!(history.undo(&mut doc, &mut ChangeSet::empty()));
    assert_eq!(doc.metadata.title, "Title 3");
    assert!(history.undo(&mut doc, &mut ChangeSet::empty()));
    assert_eq!(doc.metadata.title, "Title 2");
    assert!(history.undo(&mut doc, &mut ChangeSet::empty()));
    assert_eq!(doc.metadata.title, "Title 1");

    // 4th undo fails because command 1 was evicted
    assert!(!history.undo(&mut doc, &mut ChangeSet::empty()));
}

#[test]
fn test_history_delta_limit_eviction() {
    let mut doc = fixture_document(16, 16);
    let mut history = History::with_limits(10, 5);

    let cell = doc.cell(0, 0).expect("cell exists").clone();

    // Command 1: 3 deltas
    let cmd1 = EditCommand::PaintCells {
        label: "Cmd 1",
        deltas: vec![
            CellDelta {
                x: 0,
                y: 0,
                before: cell.clone(),
                after: cell.clone(),
            },
            CellDelta {
                x: 1,
                y: 0,
                before: cell.clone(),
                after: cell.clone(),
            },
            CellDelta {
                x: 2,
                y: 0,
                before: cell.clone(),
                after: cell.clone(),
            },
        ],
    };
    history.execute(cmd1, &mut doc, &mut ChangeSet::empty());
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.total_deltas(), 3);

    // Command 2: 4 deltas (3 + 4 = 7 > 5 -> evicts cmd1)
    let mut cell_b = cell.clone();
    cell_b.level = CliffLevel(2);
    let cmd2 = EditCommand::PaintCells {
        label: "Cmd 2",
        deltas: vec![
            CellDelta {
                x: 0,
                y: 1,
                before: cell.clone(),
                after: cell.clone(),
            },
            CellDelta {
                x: 1,
                y: 1,
                before: cell.clone(),
                after: cell.clone(),
            },
            CellDelta {
                x: 2,
                y: 1,
                before: cell.clone(),
                after: cell.clone(),
            },
            CellDelta {
                x: 3,
                y: 1,
                before: cell,
                after: cell_b,
            },
        ],
    };
    history.execute(cmd2, &mut doc, &mut ChangeSet::empty());
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.total_deltas(), 4);
}

#[test]
fn test_stroke_dragging_deduplication() {
    let mut doc = fixture_document(16, 16);
    let initial_doc = doc.clone();

    let mut recorder = StrokeRecorder::new(16, 16);
    recorder.begin();

    let paint_op = PaintOp::Level(CliffLevel(3));

    // Drag: (1, 1) -> (2, 1) -> (1, 1)
    assert!(recorder.touch(&mut doc, 1, 1, |c| paint_op.apply_to_cell(c)));
    assert!(recorder.touch(&mut doc, 2, 1, |c| paint_op.apply_to_cell(c)));
    // Touching (1, 1) second time within same stroke is skipped
    assert!(!recorder.touch(&mut doc, 1, 1, |c| paint_op.apply_to_cell(c)));

    let cmd = recorder.finish("Stroke Level 3").expect("command produced");

    match &cmd {
        EditCommand::PaintCells { label, deltas } => {
            assert_eq!(*label, "Stroke Level 3");
            assert_eq!(deltas.len(), 2);
            assert_eq!(deltas[0].x, 1);
            assert_eq!(deltas[0].y, 1);
            assert_eq!(deltas[0].before.level, CliffLevel(1));
            assert_eq!(deltas[0].after.level, CliffLevel(3));

            assert_eq!(deltas[1].x, 2);
            assert_eq!(deltas[1].y, 1);
            assert_eq!(deltas[1].before.level, CliffLevel(1));
            assert_eq!(deltas[1].after.level, CliffLevel(3));
        }
        _ => panic!("expected PaintCells command"),
    }

    // Reverting cmd restores original document bit-for-bit
    let mut cs = ChangeSet::empty();
    cmd.revert(&mut doc, &mut cs);
    assert_eq!(doc, initial_doc);
}

#[test]
fn test_stroke_generation_wrap_around() {
    let mut doc = fixture_document(16, 16);
    let mut recorder = StrokeRecorder::new(16, 16);

    recorder.set_generation_for_test(u32::MAX);
    recorder.begin();

    assert_eq!(recorder.generation(), 1);

    let op = PaintOp::Level(CliffLevel(2));
    assert!(recorder.touch(&mut doc, 3, 3, |c| op.apply_to_cell(c)));
    assert!(!recorder.touch(&mut doc, 3, 3, |c| op.apply_to_cell(c)));
}

#[test]
fn test_brush_footprints_exact_cells() {
    let mut buf = Vec::new();

    // Radius 0: exactly 1 cell for both shapes
    brush_footprint(BrushShape::Square, 0, (5, 5), 16, 16, &mut buf);
    assert_eq!(buf, vec![(5, 5)]);

    brush_footprint(BrushShape::Circle, 0, (5, 5), 16, 16, &mut buf);
    assert_eq!(buf, vec![(5, 5)]);

    // Radius 1: exactly 9 cells (3x3) for both shapes
    brush_footprint(BrushShape::Square, 1, (5, 5), 16, 16, &mut buf);
    assert_eq!(buf.len(), 9);
    for dy in 4..=6 {
        for dx in 4..=6 {
            assert!(buf.contains(&(dx, dy)));
        }
    }

    let sq_1 = buf.clone();
    brush_footprint(BrushShape::Circle, 1, (5, 5), 16, 16, &mut buf);
    assert_eq!(buf.len(), 9);
    assert_eq!(sq_1, buf);

    // Radius 2: Square is 25 cells (5x5); Circle is 21 cells (5x5 minus 4 diagonal corners)
    brush_footprint(BrushShape::Square, 2, (5, 5), 16, 16, &mut buf);
    assert_eq!(buf.len(), 25);
    for dy in 3..=7 {
        for dx in 3..=7 {
            assert!(buf.contains(&(dx, dy)));
        }
    }

    brush_footprint(BrushShape::Circle, 2, (5, 5), 16, 16, &mut buf);
    assert_eq!(buf.len(), 21);
    // The 4 corners at dx=±2, dy=±2 must be excluded: (3, 3), (3, 7), (7, 3), (7, 7)
    assert!(!buf.contains(&(3, 3)));
    assert!(!buf.contains(&(3, 7)));
    assert!(!buf.contains(&(7, 3)));
    assert!(!buf.contains(&(7, 7)));

    // All remaining 21 cells must be included
    for dy in 3..=7 {
        for dx in 3..=7 {
            if (dx == 3 || dx == 7) && (dy == 3 || dy == 7) {
                continue;
            }
            assert!(buf.contains(&(dx, dy)));
        }
    }
}

#[test]
fn test_brush_footprint_buffer_reuse_zero_alloc() {
    let mut buf = Vec::with_capacity(64);
    brush_footprint(BrushShape::Circle, 2, (5, 5), 16, 16, &mut buf);
    let initial_ptr = buf.as_ptr();
    let initial_cap = buf.capacity();
    assert_eq!(buf.len(), 21);

    // Subsequent call with the same radius reuses existing heap buffer without reallocating
    brush_footprint(BrushShape::Circle, 2, (8, 8), 16, 16, &mut buf);
    assert_eq!(buf.as_ptr(), initial_ptr);
    assert_eq!(buf.capacity(), initial_cap);
    assert_eq!(buf.len(), 21);
}

#[test]
fn test_brush_footprints_boundary_clipping() {
    let mut buf = Vec::new();
    // Clipping at (0, 0)
    brush_footprint(BrushShape::Square, 2, (0, 0), 16, 16, &mut buf);
    assert_eq!(buf.len(), 9); // only dx in 0..=2, dy in 0..=2
    for &(x, y) in &buf {
        assert!(x <= 2);
        assert!(y <= 2);
    }

    // Clipping at (15, 15)
    brush_footprint(BrushShape::Circle, 2, (15, 15), 16, 16, &mut buf);
    for &(x, y) in &buf {
        assert!(x >= 13 && x < 16);
        assert!(y >= 13 && y < 16);
    }
}

#[test]
fn test_stroke_rotate180_real_brush_stroke() {
    let mut doc = fixture_document(16, 16);
    let initial_doc = doc.clone();

    let mut recorder = StrokeRecorder::new(16, 16);
    let mut history = History::new();
    let mut buf = Vec::new();

    recorder.begin();
    let op = PaintOp::Level(CliffLevel(3));

    // Real brush stamp at (2, 3) with radius 0 and Rotate180
    paint_brush_stamp(
        &mut recorder,
        &mut doc,
        BrushShape::Circle,
        0,
        (2, 3),
        &op,
        SymmetryMode::Rotate180,
        &mut buf,
    );

    let cmd = recorder
        .finish("Symmetric Stroke")
        .expect("command generated");
    let mut cs = ChangeSet::empty();
    history.push_applied(cmd);

    // Both (2, 3) and its 180° rotation (13, 12) must be modified to level 3
    assert_eq!(doc.cell(2, 3).expect("cell").level, CliffLevel(3));
    assert_eq!(doc.cell(13, 12).expect("cell").level, CliffLevel(3));

    // Undo restores both cells back to initial document
    assert!(history.undo(&mut doc, &mut cs));
    assert_eq!(doc, initial_doc);
    assert_eq!(doc.cell(2, 3).expect("cell").level, CliffLevel(1));
    assert_eq!(doc.cell(13, 12).expect("cell").level, CliffLevel(1));
}

#[test]
fn test_stroke_mirror_x_axis_touched_once() {
    let mut doc = fixture_document(17, 16); // Width 17: axis is exactly x = 8
    let mut recorder = StrokeRecorder::new(17, 16);
    let mut buf = Vec::new();

    recorder.begin();
    let op = PaintOp::Level(CliffLevel(2));

    // Stamp centered on the vertical symmetry axis (x = 8, y = 5) with radius 0
    paint_brush_stamp(
        &mut recorder,
        &mut doc,
        BrushShape::Square,
        0,
        (8, 5),
        &op,
        SymmetryMode::MirrorX,
        &mut buf,
    );

    let cmd = recorder.finish("Axis Stamp").expect("command generated");
    match cmd {
        EditCommand::PaintCells { deltas, .. } => {
            // Must have exactly 1 delta because the axis cell was only touched once!
            assert_eq!(deltas.len(), 1);
            assert_eq!(deltas[0].x, 8);
            assert_eq!(deltas[0].y, 5);
        }
        _ => panic!("expected PaintCells"),
    }
}

#[test]
fn test_mirrored_ramp_placement_yields_mirrored_direction() {
    let mut doc = fixture_document(16, 16);
    // Plateau at level 2 on North (+Y)
    for y in 8..16 {
        for x in 0..16 {
            doc.cell_mut(x, y).expect("cell").level = CliffLevel(2);
        }
    }

    let rect = CellRect {
        min: (2, 5),
        max_inclusive: (4, 7),
    };
    let placement = RampPlacement {
        dir: RampDir::North,
        low_level: CliffLevel(1),
    };

    // Placing with MirrorX: mirrored rect is x in (16-1-4=11 ..= 16-1-2=13), y in 5..=7
    let cmd = place_ramp_command(
        &doc,
        rect,
        placement,
        SymmetryMode::MirrorX,
        "Symmetric North Ramp",
    )
    .expect("valid symmetric ramp");

    cmd.apply(&mut doc, &mut ChangeSet::empty());

    // Primary ramp (x: 2..=4, y: 5..=7) has RampDir::North
    for y in 5..=7 {
        for x in 2..=4 {
            let c = doc.cell(x, y).expect("cell");
            assert_eq!(c.ramp, Some(RampDir::North));
            assert_eq!(c.level, CliffLevel(1));
        }
    }

    // Mirrored ramp (x: 11..=13, y: 5..=7) also has RampDir::North (North unchanged under MirrorX)
    for y in 5..=7 {
        for x in 11..=13 {
            let c = doc.cell(x, y).expect("cell");
            assert_eq!(c.ramp, Some(RampDir::North));
            assert_eq!(c.level, CliffLevel(1));
        }
    }

    // Now test Rotate180 where mirrored direction inverts:
    // With Rotate180, North would become South. But South upper landing does not exist on doc!
    // place_ramp_command must return typed error RampPlacementError::NoUpperLanding!
    let rot_res = place_ramp_command(
        &doc,
        rect,
        placement,
        SymmetryMode::Rotate180,
        "Rotate Ramp",
    );
    assert_eq!(rot_res, Err(RampPlacementError::NoUpperLanding));
}

#[test]
fn test_mirrored_start_placement_creates_slot_1() {
    let doc = fixture_document(16, 16);

    let cmd = place_entity_command(
        &doc,
        EntityToolKind::StartLocation {
            slot: 0,
            footprint: (5, 5),
        },
        (2, 2),
        SymmetryMode::Rotate180,
    );

    match cmd {
        EditCommand::AddEntities { entities } => {
            assert_eq!(entities.len(), 2);
            match (&entities[0], &entities[1]) {
                (MapEntity::StartLocation(p), MapEntity::StartLocation(m)) => {
                    assert_eq!(p.player_slot, 0);
                    assert_eq!(p.cell, (2, 2));

                    assert_eq!(m.player_slot, 1);
                    assert_eq!(m.cell, (9, 9));
                    assert_eq!(m.footprint, (5, 5));
                }
                _ => panic!("expected two StartLocations"),
            }
        }
        _ => panic!("expected AddEntities command"),
    }
}

#[test]
fn test_symmetry_erase_removes_both() {
    let mut doc = fixture_document(16, 16);
    // Add two minerals at symmetric locations
    let res_a = MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (3, 3),
        amount: 1500,
    };
    let res_b = MapResource {
        id: MapEntityId(2),
        kind: ResourceKind::Mineral,
        cell: (12, 12),
        amount: 1500,
    };
    doc.resources.push(res_a);
    doc.resources.push(res_b);

    // Erase footprint covers (3, 3); with Rotate180 symmetry it covers (12, 12) too
    let footprint = [(3, 3)];
    let cmd = erase_entities_command(&doc, &footprint, SymmetryMode::Rotate180)
        .expect("erase command generated");

    match &cmd {
        EditCommand::RemoveEntities { entities } => {
            assert_eq!(entities.len(), 2);
            assert_eq!(entities[0].id(), MapEntityId(1));
            assert_eq!(entities[1].id(), MapEntityId(2));
        }
        _ => panic!("expected RemoveEntities"),
    }

    cmd.apply(&mut doc, &mut ChangeSet::empty());
    assert!(doc.resources.is_empty());
}

#[test]
fn test_move_entity_with_symmetry() {
    let mut doc = fixture_document(16, 16);
    let res_a = MapResource {
        id: MapEntityId(1),
        kind: ResourceKind::Mineral,
        cell: (2, 2),
        amount: 1500,
    };
    let res_b = MapResource {
        id: MapEntityId(2),
        kind: ResourceKind::Mineral,
        cell: (13, 13),
        amount: 1500,
    };
    doc.resources.push(res_a.clone());
    doc.resources.push(res_b.clone());

    // Moving res_a to (3, 3) with Rotate180 also moves partner res_b to (12, 12)
    let cmd = move_entity_command(
        &doc,
        &MapEntity::Resource(res_a),
        (3, 3),
        SymmetryMode::Rotate180,
    );

    match &cmd {
        EditCommand::ModifyEntities { modifications } => {
            assert_eq!(modifications.len(), 2);
            assert_eq!(modifications[0].1.cell(), (3, 3));
            assert_eq!(modifications[1].1.cell(), (12, 12));
        }
        _ => panic!("expected ModifyEntities batch command"),
    }

    cmd.apply(&mut doc, &mut ChangeSet::empty());
    assert_eq!(doc.resources[0].cell, (3, 3));
    assert_eq!(doc.resources[1].cell, (12, 12));
}

#[test]
fn test_start_footprint_catalog_resolution() {
    let loadouts_path = client::resolve_assets_dir().join("catalog/start_loadouts.ron");
    let loadouts =
        StartLoadoutCatalog::load(&loadouts_path).expect("start_loadouts.ron must exist");
    let unit_catalog = UnitCatalog::default();

    // Query standard_automaton: town hall must have 5x5 footprint
    let fp = resolve_start_footprint("standard_automaton", &loadouts, &unit_catalog)
        .expect("standard_automaton must resolve");
    assert_eq!(fp, (5, 5));

    // Unknown loadout returns typed error
    let err = resolve_start_footprint("unknown_nonexistent_loadout", &loadouts, &unit_catalog);
    assert_eq!(
        err,
        Err(StartFootprintError::UnknownLoadout(
            "unknown_nonexistent_loadout".to_string()
        ))
    );
}

#[test]
fn test_ramp_inference_four_cardinal_directions() {
    // 1. North ascending ramp (+Y)
    let mut doc_n = fixture_document(16, 16);
    // Upper landing at level 2 for y in 8..=15
    for y in 8..16 {
        for x in 0..16 {
            doc_n.cell_mut(x, y).expect("cell exists").level = CliffLevel(2);
        }
    }
    let rect_n = CellRect {
        min: (6, 5),
        max_inclusive: (9, 7),
    };
    let inferred_n = infer_ramp(&doc_n, rect_n).expect("north ramp inferred");
    assert_eq!(
        inferred_n,
        RampPlacement {
            dir: RampDir::North,
            low_level: CliffLevel(1),
        }
    );

    // 2. South ascending ramp (-Y)
    let mut doc_s = fixture_document(16, 16);
    // Upper landing at level 2 for y in 0..=7
    for y in 0..=7 {
        for x in 0..16 {
            doc_s.cell_mut(x, y).expect("cell exists").level = CliffLevel(2);
        }
    }
    let rect_s = CellRect {
        min: (6, 8),
        max_inclusive: (9, 10),
    };
    let inferred_s = infer_ramp(&doc_s, rect_s).expect("south ramp inferred");
    assert_eq!(
        inferred_s,
        RampPlacement {
            dir: RampDir::South,
            low_level: CliffLevel(1),
        }
    );

    // 3. East ascending ramp (+X)
    let mut doc_e = fixture_document(16, 16);
    // Upper landing at level 2 for x in 8..=15
    for y in 0..16 {
        for x in 8..16 {
            doc_e.cell_mut(x, y).expect("cell exists").level = CliffLevel(2);
        }
    }
    let rect_e = CellRect {
        min: (5, 6),
        max_inclusive: (7, 9),
    };
    let inferred_e = infer_ramp(&doc_e, rect_e).expect("east ramp inferred");
    assert_eq!(
        inferred_e,
        RampPlacement {
            dir: RampDir::East,
            low_level: CliffLevel(1),
        }
    );

    // 4. West ascending ramp (-X)
    let mut doc_w = fixture_document(16, 16);
    // Upper landing at level 2 for x in 0..=7
    for y in 0..16 {
        for x in 0..=7 {
            doc_w.cell_mut(x, y).expect("cell exists").level = CliffLevel(2);
        }
    }
    let rect_w = CellRect {
        min: (8, 6),
        max_inclusive: (10, 9),
    };
    let inferred_w = infer_ramp(&doc_w, rect_w).expect("west ramp inferred");
    assert_eq!(
        inferred_w,
        RampPlacement {
            dir: RampDir::West,
            low_level: CliffLevel(1),
        }
    );
}

#[test]
fn test_ramp_inference_diagnostics() {
    let mut doc = fixture_document(16, 16);

    // 1. MixedLevels inside rect
    doc.cell_mut(5, 5).expect("cell").level = CliffLevel(2);
    let rect_mixed = CellRect {
        min: (4, 4),
        max_inclusive: (6, 6),
    };
    assert_eq!(
        infer_ramp(&doc, rect_mixed),
        Err(RampPlacementError::MixedLevels)
    );

    // 2. NoUpperLanding: completely flat map
    let mut flat_doc = fixture_document(16, 16);
    let rect_flat = CellRect {
        min: (4, 4),
        max_inclusive: (6, 6),
    };
    assert_eq!(
        infer_ramp(&flat_doc, rect_flat),
        Err(RampPlacementError::NoUpperLanding)
    );

    // 3. Ambiguous: multiple higher landings
    for x in 4..=6 {
        flat_doc.cell_mut(x, 7).expect("cell").level = CliffLevel(2);
    }
    for y in 4..=6 {
        flat_doc.cell_mut(7, y).expect("cell").level = CliffLevel(2);
    }
    assert_eq!(
        infer_ramp(&flat_doc, rect_flat),
        Err(RampPlacementError::Ambiguous)
    );

    // 4. RunTooLong: ramp run > MAX_RAMP_RUN_CELLS (8)
    let mut long_doc = fixture_document(16, 16);
    for x in 0..16 {
        long_doc.cell_mut(x, 12).expect("cell").level = CliffLevel(2);
    }
    // Rect height = 11 - 2 + 1 = 10 cells > 8
    let long_rect = CellRect {
        min: (6, 2),
        max_inclusive: (7, 11),
    };
    assert_eq!(
        infer_ramp(&long_doc, long_rect),
        Err(RampPlacementError::RunTooLong {
            len: 10,
            max: MAX_RAMP_RUN_CELLS
        })
    );
}

#[test]
fn test_symmetry_involution() {
    let modes = [
        SymmetryMode::None,
        SymmetryMode::MirrorX,
        SymmetryMode::MirrorY,
        SymmetryMode::Rotate180,
    ];
    let test_coords = [(0, 0), (15, 15), (7, 8), (2, 13), (15, 0), (0, 15)];

    for mode in modes {
        for coord in test_coords {
            let mirrored = mode.mirror_cell(coord, 16, 16);
            let roundtrip = mode.mirror_cell(mirrored, 16, 16);
            assert_eq!(
                roundtrip, coord,
                "Failed involution for {mode:?} at {coord:?}"
            );
        }
    }
}

#[test]
fn test_symmetry_ramp_tables() {
    // MirrorX: East <-> West, North/South unchanged
    assert_eq!(
        SymmetryMode::MirrorX.mirror_ramp(RampDir::East),
        RampDir::West
    );
    assert_eq!(
        SymmetryMode::MirrorX.mirror_ramp(RampDir::West),
        RampDir::East
    );
    assert_eq!(
        SymmetryMode::MirrorX.mirror_ramp(RampDir::North),
        RampDir::North
    );
    assert_eq!(
        SymmetryMode::MirrorX.mirror_ramp(RampDir::South),
        RampDir::South
    );

    // MirrorY: North <-> South, East/West unchanged
    assert_eq!(
        SymmetryMode::MirrorY.mirror_ramp(RampDir::North),
        RampDir::South
    );
    assert_eq!(
        SymmetryMode::MirrorY.mirror_ramp(RampDir::South),
        RampDir::North
    );
    assert_eq!(
        SymmetryMode::MirrorY.mirror_ramp(RampDir::East),
        RampDir::East
    );
    assert_eq!(
        SymmetryMode::MirrorY.mirror_ramp(RampDir::West),
        RampDir::West
    );

    // Rotate180: North <-> South and East <-> West
    assert_eq!(
        SymmetryMode::Rotate180.mirror_ramp(RampDir::North),
        RampDir::South
    );
    assert_eq!(
        SymmetryMode::Rotate180.mirror_ramp(RampDir::South),
        RampDir::North
    );
    assert_eq!(
        SymmetryMode::Rotate180.mirror_ramp(RampDir::East),
        RampDir::West
    );
    assert_eq!(
        SymmetryMode::Rotate180.mirror_ramp(RampDir::West),
        RampDir::East
    );
}

#[test]
fn test_paint_op_side_effects() {
    let mut doc = fixture_document(16, 16);

    // 1. Painting cliff level clears any existing ramp
    let cell_idx = doc.cell_mut(5, 5).expect("cell exists");
    cell_idx.ramp = Some(RampDir::North);
    cell_idx.level = CliffLevel(1);

    let op_level = PaintOp::Level(CliffLevel(2));
    let after_level = op_level.apply_to_cell(doc.cell(5, 5).expect("cell"));
    assert_eq!(after_level.level, CliffLevel(2));
    assert_eq!(after_level.ramp, None);

    // 2. Painting Terrain(Abyss) clears BUILDABLE, GROUND_PATHABLE, and ramp
    let mut initial_cell = doc.cell(5, 5).expect("cell").clone();
    initial_cell.flags = CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE | CellFlags::SPEED_ZONE;
    initial_cell.ramp = Some(RampDir::East);

    let op_abyss = PaintOp::Terrain(TerrainKind::Abyss);
    let after_abyss = op_abyss.apply_to_cell(&initial_cell);
    assert_eq!(after_abyss.terrain, TerrainKind::Abyss);
    assert_eq!(after_abyss.ramp, None);
    assert!(!after_abyss.flags.contains(CellFlags::BUILDABLE));
    assert!(!after_abyss.flags.contains(CellFlags::GROUND_PATHABLE));
    assert!(after_abyss.flags.contains(CellFlags::SPEED_ZONE));

    // 3. Painting non-Abyss on top of Abyss restores default BUILDABLE | GROUND_PATHABLE
    let op_dirt = PaintOp::Terrain(TerrainKind::Dirt);
    let after_dirt = op_dirt.apply_to_cell(&after_abyss);
    assert_eq!(after_dirt.terrain, TerrainKind::Dirt);
    assert!(after_dirt.flags.contains(CellFlags::BUILDABLE));
    assert!(after_dirt.flags.contains(CellFlags::GROUND_PATHABLE));
}

#[test]
fn test_editor_document_sync_terrain() {
    let doc = fixture_document(16, 16);
    let mut editor_doc = EditorDocument::new(doc, None);
    assert_eq!(editor_doc.revision, 0);
    assert!(!editor_doc.dirty);
    assert!(editor_doc.is_validation_current());

    let mut terrain_state = TerrainState::new(
        editor_doc
            .doc
            .to_game_map()
            .expect("invariant: valid game map"),
    );

    // Mutate a 2x2 area
    let rect = CellRect {
        min: (2, 2),
        max_inclusive: (3, 3),
    };
    let op = PaintOp::Level(CliffLevel(3));
    let cmd = box_fill_command(
        &editor_doc.doc,
        rect,
        &op,
        SymmetryMode::None,
        "Fill Level 3",
    );

    let mut cs = ChangeSet::empty();
    cmd.apply(&mut editor_doc.doc, &mut cs);
    editor_doc.apply_changeset(&cs, 1.25);

    assert_eq!(editor_doc.revision, 1);
    assert!(editor_doc.dirty);
    assert_eq!(editor_doc.last_edit_at_secs, 1.25);
    assert!(!editor_doc.is_validation_current());

    // Sync to terrain incrementally
    editor_doc.sync_terrain(&cs, &mut terrain_state);

    for y in 2..=3 {
        for x in 2..=3 {
            let map_cell = terrain_state
                .map
                .cell(x, y)
                .expect("cell exists in terrain map");
            assert_eq!(map_cell.cliff_level, CliffLevel(3));
        }
    }

    editor_doc.revalidate();
    assert!(editor_doc.is_validation_current());
}

#[test]
fn test_entity_tool_selection_and_erasure() {
    let mut doc = fixture_document(16, 16);

    // Place mineral
    let place_res = place_entity_command(&doc, EntityToolKind::Mineral, (3, 3), SymmetryMode::None);
    place_res.apply(&mut doc, &mut ChangeSet::empty());

    // Place start location for slot 0
    let place_start = place_entity_command(
        &doc,
        EntityToolKind::StartLocation {
            slot: 0,
            footprint: (5, 5),
        },
        (5, 5),
        SymmetryMode::None,
    );
    place_start.apply(&mut doc, &mut ChangeSet::empty());

    assert_eq!(doc.resources.len(), 1);
    assert_eq!(doc.start_locations.len(), 1);

    // Picking: mineral at (3, 3)
    let picked_res = pick_entity(&doc, (3, 3)).expect("mineral picked");
    assert_eq!(picked_res.cell(), (3, 3));

    // Picking: start location footprint cell (7, 7)
    let picked_start = pick_entity(&doc, (7, 7)).expect("start picked");
    match picked_start {
        MapEntity::StartLocation(ref s) => {
            assert_eq!(s.player_slot, 0);
            assert_eq!(s.cell, (5, 5));
        }
        _ => panic!("expected start location"),
    }

    // Move start location to (6, 6)
    let move_cmd = move_entity_command(&doc, &picked_start, (6, 6), SymmetryMode::None);
    move_cmd.apply(&mut doc, &mut ChangeSet::empty());
    assert_eq!(doc.start_locations[0].cell, (6, 6));

    // Erase entities within footprint
    let footprint = [(3, 3), (3, 4)];
    let erase_cmd = erase_entities_command(&doc, &footprint, SymmetryMode::None)
        .expect("erase command produced");
    erase_cmd.apply(&mut doc, &mut ChangeSet::empty());
    assert_eq!(doc.resources.len(), 0);
    assert_eq!(doc.start_locations.len(), 1);

    // Delete start location
    let del_cmd = delete_entity_command(&MapEntity::StartLocation(doc.start_locations[0].clone()));
    del_cmd.apply(&mut doc, &mut ChangeSet::empty());
    assert_eq!(doc.start_locations.len(), 0);
}

#[test]
fn test_explicit_ramp_check_and_placement() {
    let mut doc = fixture_document(16, 16);
    for y in 8..16 {
        for x in 0..16 {
            doc.cell_mut(x, y).expect("cell exists").level = CliffLevel(2);
        }
    }

    let rect = CellRect {
        min: (6, 5),
        max_inclusive: (9, 7),
    };

    // Explicit override North succeeds
    let placement = check_ramp_placement(&doc, rect, RampDir::North).expect("valid north ramp");
    assert_eq!(
        placement,
        RampPlacement {
            dir: RampDir::North,
            low_level: CliffLevel(1),
        }
    );

    // Explicit override South fails (landings inverted)
    assert!(check_ramp_placement(&doc, rect, RampDir::South).is_err());

    // Place ramp command
    let cmd = place_ramp_command(
        &doc,
        rect,
        placement,
        SymmetryMode::None,
        "Place North Ramp",
    )
    .expect("valid ramp command");
    cmd.apply(&mut doc, &mut ChangeSet::empty());

    for y in 5..=7 {
        for x in 6..=9 {
            let cell = doc.cell(x, y).expect("cell exists");
            assert_eq!(cell.ramp, Some(RampDir::North));
            assert_eq!(cell.level, CliffLevel(1));
            assert_eq!(cell.flags, CellFlags::GROUND_PATHABLE);
        }
    }
}

#[test]
fn test_authorable_flags_and_constants() {
    assert_eq!(
        AuthorableFlag::Buildable.to_cell_flags(),
        CellFlags::BUILDABLE
    );
    assert_eq!(
        AuthorableFlag::GroundPathable.to_cell_flags(),
        CellFlags::GROUND_PATHABLE
    );
    assert_eq!(
        AuthorableFlag::LosBlocker.to_cell_flags(),
        CellFlags::LOS_BLOCKER
    );
    assert_eq!(
        AuthorableFlag::SpeedZone.to_cell_flags(),
        CellFlags::SPEED_ZONE
    );

    assert_eq!(MAX_UNDO_COMMANDS, 256);
    assert_eq!(DEFAULT_MINERAL_AMOUNT, 1500);
    assert_eq!(DEFAULT_GAS_AMOUNT, 2500);
    assert_eq!(DEFAULT_WATCHTOWER_CAPTURE_RADIUS_TENTHS, 30);
    assert_eq!(DEFAULT_WATCHTOWER_REVEAL_RADIUS_TENTHS, 120);
}

/// Everything [`apply_unsaved_choice`] touches, starting from a dirty document with a guarded action.
struct UnsavedFlow {
    modal: UnsavedChangesModalState,
    doc: EditorDocument,
    browser: FileBrowserState,
    new_map: NewMapModalState,
    tools: EditorToolState,
    dir: tempfile::TempDir,
}

impl UnsavedFlow {
    fn new(pending: PendingUnsavedAction, saved_path: bool) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = saved_path.then(|| dir.path().join("flow.scmap.ron"));
        let mut doc = EditorDocument::new(fixture_document(32, 32), path);
        doc.dirty = true;
        Self {
            modal: UnsavedChangesModalState {
                is_open: true,
                pending_action: Some(pending),
            },
            doc,
            browser: FileBrowserState::default(),
            new_map: NewMapModalState::default(),
            tools: EditorToolState::new(32, 32),
            dir,
        }
    }

    fn choose(&mut self, choice: UnsavedChoice) -> UnsavedOutcome {
        apply_unsaved_choice(
            choice,
            &mut self.modal,
            &mut self.doc,
            &mut self.browser,
            self.dir.path(),
            &mut self.new_map,
            &mut self.tools,
        )
    }
}

#[test]
fn test_unsaved_discard_keeps_dirty_until_the_document_is_replaced() {
    let mut flow = UnsavedFlow::new(PendingUnsavedAction::OpenFile, true);
    assert_eq!(
        flow.choose(UnsavedChoice::Discard),
        UnsavedOutcome::Continue
    );
    assert!(
        flow.browser.is_open,
        "discard must continue to the open dialog"
    );
    assert!(
        flow.doc.dirty,
        "cancelling the open dialog must leave the edits flagged as unsaved"
    );

    let mut flow = UnsavedFlow::new(PendingUnsavedAction::NewMap, true);
    flow.choose(UnsavedChoice::Discard);
    assert!(flow.new_map.is_open);
    assert!(flow.doc.dirty);
}

#[test]
fn test_unsaved_discard_quit_exits() {
    let mut flow = UnsavedFlow::new(PendingUnsavedAction::Quit, true);
    assert_eq!(flow.choose(UnsavedChoice::Discard), UnsavedOutcome::Exit);
}

#[test]
fn test_unsaved_save_resumes_the_guarded_action() {
    let mut flow = UnsavedFlow::new(PendingUnsavedAction::Quit, true);
    assert_eq!(flow.choose(UnsavedChoice::Save), UnsavedOutcome::Exit);
    assert!(!flow.doc.dirty);
    assert_eq!(flow.modal.pending_action, None);

    let mut flow = UnsavedFlow::new(PendingUnsavedAction::OpenFile, true);
    assert_eq!(flow.choose(UnsavedChoice::Save), UnsavedOutcome::Continue);
    assert!(flow.browser.is_open, "save must resume the open dialog");
    assert_eq!(flow.browser.mode, FileBrowserMode::Open);

    let mut flow = UnsavedFlow::new(PendingUnsavedAction::NewMap, true);
    flow.choose(UnsavedChoice::Save);
    assert!(flow.new_map.is_open, "save must resume the new map dialog");
}

#[test]
fn test_unsaved_save_of_untitled_map_defers_the_action_to_the_save_dialog() {
    let mut flow = UnsavedFlow::new(PendingUnsavedAction::Quit, false);
    assert_eq!(flow.choose(UnsavedChoice::Save), UnsavedOutcome::Continue);
    assert!(flow.browser.is_open);
    assert_eq!(flow.browser.mode, FileBrowserMode::Save);
    assert_eq!(
        flow.modal.pending_action,
        Some(PendingUnsavedAction::Quit),
        "the action must wait for the save dialog"
    );
}

#[test]
fn test_unsaved_failed_save_keeps_the_prompt_and_the_action() {
    let mut flow = UnsavedFlow::new(PendingUnsavedAction::Quit, true);
    flow.doc.path = Some(flow.dir.path().join("missing_dir/flow.scmap.ron"));
    assert_eq!(flow.choose(UnsavedChoice::Save), UnsavedOutcome::Continue);
    assert!(flow.modal.is_open, "a failed save must not lose the prompt");
    assert_eq!(flow.modal.pending_action, Some(PendingUnsavedAction::Quit));
    assert!(flow.doc.dirty);
    assert!(flow.tools.last_status_message.is_some());
}

#[test]
fn test_unsaved_cancel_abandons_the_action_and_keeps_the_edits() {
    let mut flow = UnsavedFlow::new(PendingUnsavedAction::Quit, true);
    assert_eq!(flow.choose(UnsavedChoice::Cancel), UnsavedOutcome::Continue);
    assert!(!flow.modal.is_open);
    assert_eq!(flow.modal.pending_action, None);
    assert!(flow.doc.dirty);
}
