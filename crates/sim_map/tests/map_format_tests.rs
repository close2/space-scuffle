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
    WATCHTOWER_ENTITY_ID_BASE,
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
    let file = doc.encode();
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
    let encoded_file = doc.encode();
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

    let file = doc.encode();
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
fn default_arena_file_equals_code_arena() {
    let path = map_path("default_arena.scmap.ron");
    let file = MapFile::load(&path).expect("load default_arena failed");
    let doc = file.decode().expect("decode failed");
    let file_map = doc.to_game_map().expect("to_game_map failed");
    let code_map = GameMap::default_arena();

    assert_eq!(file_map.width(), code_map.width());
    assert_eq!(file_map.height(), code_map.height());

    // Element-wise comparison of all cells
    assert_eq!(file_map.cells(), code_map.cells());

    // Element-wise comparison of all resources
    assert_eq!(file_map.resources().len(), code_map.resources().len());
    for (fr, cr) in file_map.resources().iter().zip(code_map.resources().iter()) {
        assert_eq!(fr.cell, cr.cell);
        assert_eq!(fr.kind, cr.kind);
        assert_eq!(fr.remaining, cr.remaining);
        assert_eq!(fr.pos, cr.pos);
    }

    // Element-wise comparison of watchtowers:
    // EXCEPT watchtower entity id: default_arena() uses EntityId(100) while to_game_map assigns
    // WATCHTOWER_ENTITY_ID_BASE + map_id by design (§2.2).
    assert_eq!(file_map.watchtowers().len(), code_map.watchtowers().len());
    for (fw, cw) in file_map
        .watchtowers()
        .iter()
        .zip(code_map.watchtowers().iter())
    {
        assert_eq!(fw.id, EntityId(WATCHTOWER_ENTITY_ID_BASE + 21)); // map entity id 21
        assert_eq!(cw.id, EntityId(100)); // code arena default
        assert_eq!(fw.pos, cw.pos);
        assert_eq!(fw.capture_radius, cw.capture_radius);
        assert_eq!(fw.reveal_radius, cw.reveal_radius);
        assert_eq!(fw.controlling_player, cw.controlling_player);
    }
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
    .encode();
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
    .encode();
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
    .encode();
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
    .encode();
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
    .encode();
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
    .encode();
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
    .encode();

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
    .encode();

    file.save_atomic(&map_path)
        .expect("save_atomic overwrite must succeed");

    let loaded = MapFile::load(&map_path).expect("must parse overwritten file");
    assert_eq!(loaded.metadata.title, "Overwritten");

    let _ = std::fs::remove_file(&map_path);
}
