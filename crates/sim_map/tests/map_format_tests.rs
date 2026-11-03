//! Integration tests for the Space Scuffle map serialization format (`*.scmap.ron`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::manual_string_new,
    clippy::needless_raw_string_hashes
)]

use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapDocument, MapFile, MapFormatError, MapLayer,
    MapMetadata, RampDir, TerrainKind, Watchtower, CURRENT_MAP_FORMAT_VERSION,
};
use sim_types::{EntityId, SimScalar, SimVec2};

#[test]
fn blank_document_roundtrip() {
    let metadata = MapMetadata {
        title: "Test Map".to_string(),
        author: "Tester".to_string(),
        description: "Testing roundtrip".to_string(),
        theme: "orbital_foundry".to_string(),
    };
    let doc = MapDocument::new_blank(32, 32, metadata).expect("new_blank failed");
    let file = doc.encode().expect("a valid document encodes");
    let ron_str = file.to_ron_string().expect("to_ron_string failed");
    let loaded_file = MapFile::from_ron_str(&ron_str).expect("from_ron_str failed");
    let decoded_doc = loaded_file.decode().expect("decode failed");

    assert_eq!(doc, decoded_doc);
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
fn showcase_roundtrip_is_byte_identical() {
    let path = map_path("showcase_canyon.scmap.ron");
    let file = MapFile::load(&path).expect("load showcase_canyon failed");
    let raw_bytes = std::fs::read_to_string(&path).expect("read showcase_canyon failed");

    let doc = file.decode().expect("decode failed");
    let encoded_file = doc.encode().expect("a valid document encodes");
    let re_encoded_str = encoded_file.to_ron_string().expect("to_ron_string failed");

    assert_eq!(raw_bytes, re_encoded_str);
}

#[test]
fn row_zero_is_north() {
    let metadata = MapMetadata {
        title: "North Test".to_string(),
        author: "Tester".to_string(),
        description: "Checking north row convention".to_string(),
        theme: "orbital_foundry".to_string(),
    };
    let mut doc = MapDocument::new_blank(16, 16, metadata).expect("new_blank failed");
    // (x=0, y=15) is North-West cell. Row 0 in file is y = height - 1 = 15.
    doc.cell_mut(0, 15).expect("in bounds").level = CliffLevel(3);

    let file = doc.encode().expect("a valid document encodes");
    assert_eq!(file.levels.len(), 16);
    assert!(
        file.levels[0].starts_with('3'),
        "row 0 must represent northernmost row (y=15), starts with '3'"
    );
    assert!(
        file.levels[15].starts_with('1'),
        "row 15 must represent southernmost row (y=0), starts with '1'"
    );
}

#[test]
fn unsupported_version() {
    let ron_str = r#"
(
    format_version: 2,
    metadata: (title: "", author: "", description: "", theme: ""),
    width: 16, height: 16,
    levels: [], terrain: [], ramps: [], flags: [],
    resources: [], watchtowers: [], start_locations: [],
)
"#;
    let err = MapFile::from_ron_str(ron_str).unwrap_err();
    assert!(matches!(
        err,
        MapFormatError::UnsupportedVersion {
            found: 2,
            supported: CURRENT_MAP_FORMAT_VERSION
        }
    ));
}

#[test]
fn legacy_scmapdata_ron_rejected() {
    let legacy_ron = r#"
(
    width: 16,
    height: 16,
    cells: [],
)
"#;
    let err = MapFile::from_ron_str(legacy_ron).unwrap_err();
    assert!(matches!(err, MapFormatError::RonSyntax(_)));
}

#[test]
fn dimensions_out_of_range_too_small() {
    let metadata = MapMetadata {
        title: "".to_string(),
        author: "".to_string(),
        description: "".to_string(),
        theme: "".to_string(),
    };
    let err = MapDocument::new_blank(15, 16, metadata).unwrap_err();
    assert!(matches!(
        err,
        MapFormatError::DimensionsOutOfRange {
            width: 15,
            height: 16
        }
    ));
}

#[test]
fn dimensions_out_of_range_too_large() {
    let metadata = MapMetadata {
        title: "".to_string(),
        author: "".to_string(),
        description: "".to_string(),
        theme: "".to_string(),
    };
    let err = MapDocument::new_blank(257, 16, metadata).unwrap_err();
    assert!(matches!(
        err,
        MapFormatError::DimensionsOutOfRange {
            width: 257,
            height: 16
        }
    ));
}

#[test]
fn row_count_error() {
    let mut file = MapDocument::new_blank(
        16,
        16,
        MapMetadata {
            title: "".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            theme: "".to_string(),
        },
    )
    .unwrap()
    .encode()
    .expect("a valid document encodes");
    file.levels.pop(); // now 15 rows instead of 16

    let err = file.decode().unwrap_err();
    assert!(matches!(
        err,
        MapFormatError::RowCount {
            layer: MapLayer::Levels,
            expected: 16,
            found: 15
        }
    ));
}

#[test]
fn row_length_error() {
    let mut file = MapDocument::new_blank(
        16,
        16,
        MapMetadata {
            title: "".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            theme: "".to_string(),
        },
    )
    .unwrap()
    .encode()
    .expect("a valid document encodes");
    file.terrain[0].push('D'); // now 17 columns in row 0

    let err = file.decode().unwrap_err();
    assert!(matches!(
        err,
        MapFormatError::RowLength {
            layer: MapLayer::Terrain,
            row: 0,
            expected: 16,
            found: 17
        }
    ));
}

#[test]
fn invalid_cell_char_levels() {
    let mut file = MapDocument::new_blank(
        16,
        16,
        MapMetadata {
            title: "".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            theme: "".to_string(),
        },
    )
    .unwrap()
    .encode()
    .expect("a valid document encodes");
    // In row 0 (y = 15), column 2 (x = 2), insert '4'
    file.levels[0].replace_range(2..3, "4");

    let err = file.decode().unwrap_err();
    assert!(matches!(
        err,
        MapFormatError::InvalidCellChar {
            layer: MapLayer::Levels,
            x: 2,
            y: 15,
            ch: '4'
        }
    ));
}

#[test]
fn invalid_cell_char_terrain() {
    let mut file = MapDocument::new_blank(
        16,
        16,
        MapMetadata {
            title: "".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            theme: "".to_string(),
        },
    )
    .unwrap()
    .encode()
    .expect("a valid document encodes");
    // In row 3 (y = 12), column 4 (x = 4), insert 'X'
    file.terrain[3].replace_range(4..5, "X");

    let err = file.decode().unwrap_err();
    assert!(matches!(
        err,
        MapFormatError::InvalidCellChar {
            layer: MapLayer::Terrain,
            x: 4,
            y: 12,
            ch: 'X'
        }
    ));
}

#[test]
fn invalid_cell_char_ramps() {
    let mut file = MapDocument::new_blank(
        16,
        16,
        MapMetadata {
            title: "".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            theme: "".to_string(),
        },
    )
    .unwrap()
    .encode()
    .expect("a valid document encodes");
    // In row 5 (y = 10), column 1 (x = 1), insert 'Q'
    file.ramps[5].replace_range(1..2, "Q");

    let err = file.decode().unwrap_err();
    assert!(matches!(
        err,
        MapFormatError::InvalidCellChar {
            layer: MapLayer::Ramps,
            x: 1,
            y: 10,
            ch: 'Q'
        }
    ));
}

#[test]
fn invalid_cell_char_flags() {
    let mut file = MapDocument::new_blank(
        16,
        16,
        MapMetadata {
            title: "".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            theme: "".to_string(),
        },
    )
    .unwrap()
    .encode()
    .expect("a valid document encodes");
    // In row 2 (y = 13), column 3 (x = 3), insert 'G' (invalid hex)
    file.flags[2].replace_range(3..4, "G");

    let err = file.decode().unwrap_err();
    assert!(matches!(
        err,
        MapFormatError::InvalidCellChar {
            layer: MapLayer::Flags,
            x: 3,
            y: 13,
            ch: 'G'
        }
    ));
}

#[test]
fn unsupported_diagonal_ramp() {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1))).unwrap();
    map.cells_mut()[(5 * 16 + 5) as usize].ramp = Some(RampDir::NorthEast);
    map.cells_mut()[(5 * 16 + 5) as usize]
        .flags
        .insert(CellFlags::RAMP);

    let err = MapDocument::from_game_map(
        &map,
        MapMetadata {
            title: "".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            theme: "".to_string(),
        },
    )
    .unwrap_err();

    assert!(matches!(
        err,
        MapFormatError::UnsupportedDiagonalRamp { x: 5, y: 5 }
    ));
}

#[test]
fn runtime_state_in_cell() {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1))).unwrap();
    map.cells_mut()[(4 * 16 + 4) as usize]
        .flags
        .insert(CellFlags::OCCUPIED_BY_STRUCTURE);

    let err = MapDocument::from_game_map(
        &map,
        MapMetadata {
            title: "".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            theme: "".to_string(),
        },
    )
    .unwrap_err();

    assert!(matches!(
        err,
        MapFormatError::RuntimeStateInCell { x: 4, y: 4 }
    ));
}

#[test]
fn inconsistent_ramp_flag() {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1))).unwrap();
    // Has ramp Some(North) but missing RAMP flag
    map.cells_mut()[(2 * 16 + 2) as usize].ramp = Some(RampDir::North);

    let err = MapDocument::from_game_map(
        &map,
        MapMetadata {
            title: "".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            theme: "".to_string(),
        },
    )
    .unwrap_err();

    assert!(matches!(
        err,
        MapFormatError::InconsistentRampFlag { x: 2, y: 2 }
    ));
}

#[test]
fn inconsistent_abyss_flag() {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1))).unwrap();
    // Terrain is Abyss but UNPASSABLE_CLIFF flag missing
    map.cells_mut()[(3 * 16 + 3) as usize].terrain_kind = TerrainKind::Abyss;

    let err = MapDocument::from_game_map(
        &map,
        MapMetadata {
            title: "".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            theme: "".to_string(),
        },
    )
    .unwrap_err();

    assert!(matches!(
        err,
        MapFormatError::InconsistentAbyssFlag { x: 3, y: 3 }
    ));
}

#[test]
fn non_tenth_radius() {
    let cells = vec![MapCell::default_ground(CliffLevel(1)); 256];
    let watchtowers = vec![Watchtower {
        id: EntityId(101),
        pos: SimVec2::new(SimScalar::from_num(5.5), SimScalar::from_num(5.5)),
        capture_radius: SimScalar::from_num(3.125), // not an exact tenth!
        reveal_radius: SimScalar::from_num(10),
        controlling_player: None,
    }];
    let map = GameMap::from_cells(16, 16, cells, Vec::new(), watchtowers).unwrap();

    let err = MapDocument::from_game_map(
        &map,
        MapMetadata {
            title: "".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            theme: "".to_string(),
        },
    )
    .unwrap_err();

    assert!(matches!(err, MapFormatError::NonTenthRadius { x: 5, y: 5 }));
}

#[test]
fn save_atomic_leaves_no_tmp_file() {
    let temp_dir = std::env::temp_dir();
    let map_path = temp_dir.join("test_atomic_save.scmap.ron");
    let tmp_path = temp_dir.join("test_atomic_save.scmap.ron.tmp");

    if map_path.exists() {
        let _ = std::fs::remove_file(&map_path);
    }
    if tmp_path.exists() {
        let _ = std::fs::remove_file(&tmp_path);
    }

    let file = MapDocument::new_blank(
        16,
        16,
        MapMetadata {
            title: "Atomic".to_string(),
            author: "Tester".to_string(),
            description: "Atomic save test".to_string(),
            theme: "orbital_foundry".to_string(),
        },
    )
    .unwrap()
    .encode()
    .expect("a valid document encodes");

    file.save_atomic(&map_path)
        .expect("save_atomic must succeed");

    assert!(map_path.exists(), "target map file must exist");
    assert!(!tmp_path.exists(), "temp file must be moved/cleaned up");

    let _ = std::fs::remove_file(&map_path);
}

#[test]
fn save_atomic_overwrites_existing() {
    let temp_dir = std::env::temp_dir();
    let map_path = temp_dir.join("test_atomic_overwrite.scmap.ron");

    std::fs::write(&map_path, "pre-existing content").expect("write pre-existing");

    let file = MapDocument::new_blank(
        16,
        16,
        MapMetadata {
            title: "Overwritten".to_string(),
            author: "Tester".to_string(),
            description: "Testing overwrite".to_string(),
            theme: "orbital_foundry".to_string(),
        },
    )
    .unwrap()
    .encode()
    .expect("a valid document encodes");

    file.save_atomic(&map_path)
        .expect("save_atomic overwrite must succeed");

    let loaded = MapFile::load(&map_path).expect("must parse overwritten file");
    assert_eq!(loaded.metadata.title, "Overwritten");

    let _ = std::fs::remove_file(&map_path);
}

/// A 16 x 16 blank document used by the `encode` rejection tests.
fn small_document() -> MapDocument {
    MapDocument::new_blank(
        16,
        16,
        MapMetadata {
            title: "Encode Rejection".to_string(),
            author: "Tester".to_string(),
            description: "A document encode must refuse".to_string(),
            theme: "orbital_foundry".to_string(),
        },
    )
    .expect("16 x 16 lies inside the supported range")
}

#[test]
fn encode_of_a_document_with_too_few_cells_does_not_panic() {
    let mut doc = small_document();
    doc.cells.pop();
    let outcome = std::panic::catch_unwind(|| doc.encode());
    assert!(
        matches!(
            outcome,
            Ok(Err(MapFormatError::CellCountMismatch {
                width: 16,
                height: 16,
                found: 255
            }))
        ),
        "encode must return CellCountMismatch, not panic: {outcome:?}"
    );
}

#[test]
fn encode_rejects_a_document_with_too_many_cells() {
    let mut doc = small_document();
    doc.cells.push(doc.cells[0].clone());
    assert!(matches!(
        doc.encode(),
        Err(MapFormatError::CellCountMismatch {
            width: 16,
            height: 16,
            found: 257
        })
    ));
}

#[test]
fn encode_rejects_dimensions_out_of_range() {
    let mut doc = small_document();
    doc.width = 8;
    doc.height = 32;
    assert!(matches!(
        doc.encode(),
        Err(MapFormatError::DimensionsOutOfRange {
            width: 8,
            height: 32
        })
    ));
}

#[test]
fn encode_rejects_a_diagonal_ramp() {
    let mut doc = small_document();
    doc.cell_mut(4, 5).expect("in bounds").ramp = Some(RampDir::NorthEast);
    assert!(matches!(
        doc.encode(),
        Err(MapFormatError::UnsupportedDiagonalRamp { x: 4, y: 5 })
    ));
}

#[test]
fn encode_rejects_a_level_above_three() {
    let mut doc = small_document();
    doc.cell_mut(4, 5).expect("in bounds").level = CliffLevel(4);
    assert!(matches!(
        doc.encode(),
        Err(MapFormatError::LevelOutOfRange {
            x: 4,
            y: 5,
            level: 4
        })
    ));
}

#[test]
fn encode_rejects_non_authorable_flags() {
    for (bit, raw) in [
        (CellFlags::OCCUPIED_BY_STRUCTURE, 0x10),
        (CellFlags::RAMP, 0x20),
        (CellFlags::UNPASSABLE_CLIFF, 0x40),
    ] {
        let mut doc = small_document();
        doc.cell_mut(4, 5).expect("in bounds").flags |= bit;
        let result = doc.encode();
        assert!(
            matches!(
                result,
                Err(MapFormatError::NonAuthorableFlags { x: 4, y: 5, bits }) if bits == raw
            ),
            "{bit:?}: {result:?}"
        );
    }
}

/// The first offending cell in row-major order from `y = 0` is reported, as `from_game_map`
/// reports it, whatever kind of error the later cells carry.
#[test]
fn encode_reports_the_first_offending_cell_in_row_major_order() {
    let mut doc = small_document();
    doc.cell_mut(0, 9).expect("in bounds").level = CliffLevel(9);
    doc.cell_mut(15, 3).expect("in bounds").ramp = Some(RampDir::SouthEast);
    assert!(matches!(
        doc.encode(),
        Err(MapFormatError::UnsupportedDiagonalRamp { x: 15, y: 3 })
    ));
}

/// `encode` and `from_game_map` reject the same diagonal ramp with the same variant.
#[test]
fn encode_and_from_game_map_agree_on_a_diagonal_ramp() {
    let mut doc = small_document();
    let mut map = doc.to_game_map().expect("a blank document converts");
    let cell = map.cell_mut(4, 5).expect("in bounds");
    cell.ramp = Some(RampDir::NorthWest);
    cell.flags |= CellFlags::RAMP;
    let from_map = MapDocument::from_game_map(&map, doc.metadata.clone());
    doc.cell_mut(4, 5).expect("in bounds").ramp = Some(RampDir::NorthWest);
    assert!(matches!(
        from_map,
        Err(MapFormatError::UnsupportedDiagonalRamp { x: 4, y: 5 })
    ));
    assert!(matches!(
        doc.encode(),
        Err(MapFormatError::UnsupportedDiagonalRamp { x: 4, y: 5 })
    ));
}
