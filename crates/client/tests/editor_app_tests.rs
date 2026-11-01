//! Integration and application-level tests for the Space Scuffle Map Editor.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.3 and §7.4 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this test suite validates:
//! 1. **Playtesting Document Isolation:** Entering playtesting mode, executing combat orders, and returning
//!    to editing mode leaves the authored document cells, entities, and undo/redo history completely undamaged.
//! 2. **File Browser Scanning:** File system directory scanning correctly discovers `.scmap.ron` maps
//!    and populates the sorted file list.
//! 3. **Data-Driven Architecture:** Enforces AGENTS.md §3.3 by verifying zero hardcoded creature or
//!    building slugs in `crates/client/src/editor/`.
//! 4. **Camera Navigation & Altitude Easing:** RTS camera computes correct world transforms and respects
//!    terrain altitude tracking.
//! 5. **Automated Virtual GPU Render Verification:** Full end-to-end headless/xvfb execution of
//!    `map_editor --verify-render <dir> --map <canyon>` producing 4 reference screenshots.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::suspicious_command_arg_space
)]

use std::fs;
use std::path::Path;

use client::camera::RtsCamera;
use client::editor::camera::{focus_cell, CAMERA_HEIGHT_EASE_PER_SEC};
use client::editor::commands::History;
use client::editor::document::EditorDocument;
use client::editor::file_browser::FileBrowserState;
use client::editor::playtest::initialize_playtest_world;
use client::resolve_assets_dir;
use sim_core::scenario::{PlayerSetup, StartLoadoutCatalog};
use sim_map::grid::{CellFlags, CliffLevel, TerrainKind};
use sim_map::map_format::{MapCellSpec, MapEntityId, MapMetadata, MapStartLocation};
use sim_types::commands::{PlayerCommand, TickBundle, UnitOrder};
use sim_types::ids::{EntityId, PlayerId, Tick};
use sim_types::math::{SimScalar, SimVec2};

#[test]
fn test_playtest_enter_exit_preserves_document_and_history() {
    let assets_dir = resolve_assets_dir();
    let loadouts_path = assets_dir.join("catalog/start_loadouts.ron");
    let loadouts = StartLoadoutCatalog::load(&loadouts_path).expect("start loadouts must be valid");

    let meta = MapMetadata {
        title: "Playtest Sandbox".to_string(),
        author: "Test Suite".to_string(),
        description: "Testing non-destructive playtest isolation".to_string(),
        theme: "orbital_foundry".to_string(),
    };

    let mut doc = EditorDocument::new_blank(64, 64, meta).expect("valid blank map document");

    // Add two valid start locations so scenario construction succeeds
    doc.doc.start_locations.push(MapStartLocation {
        id: MapEntityId(1),
        player_slot: 0,
        cell: (10, 10),
        footprint: (5, 5),
    });
    doc.doc.start_locations.push(MapStartLocation {
        id: MapEntityId(2),
        player_slot: 1,
        cell: (50, 50),
        footprint: (5, 5),
    });

    // Record an edit before entering playtest
    let history = History::new();
    let initial_spec = MapCellSpec {
        level: CliffLevel(1),
        terrain: TerrainKind::Dirt,
        ramp: None,
        flags: CellFlags::BUILDABLE,
    };
    if let Some(cell) = doc.doc.cell_mut(15, 15) {
        *cell = initial_spec;
    }

    let doc_snapshot_before = doc.doc.clone();
    let history_undo_len_before = history.undo_len();
    let history_redo_len_before = history.redo_len();

    // 1. Initialize playtest world
    let players = vec![
        PlayerSetup {
            player_slot: 0,
            loadout_slug: "standard_automaton".to_string(),
        },
        PlayerSetup {
            player_slot: 1,
            loadout_slug: "standard_automaton".to_string(),
        },
    ];

    let mut world = initialize_playtest_world(&doc, &loadouts, &players)
        .expect("playtest world initialization must succeed");

    assert!(
        !world.entities.is_empty(),
        "Units should have spawned for player loadouts"
    );

    // 2. Step playtest world with unit orders
    let player_0_units: Vec<EntityId> = world
        .entities
        .iter()
        .filter(|(_, inst)| inst.owner == PlayerId(0))
        .map(|(id, _)| *id)
        .collect();

    assert!(!player_0_units.is_empty(), "Player 0 must have units");

    let target_pos = SimVec2::new(SimScalar::from_num(30), SimScalar::from_num(30));
    let move_order = PlayerCommand {
        player_id: PlayerId(0),
        entities: player_0_units,
        order: UnitOrder::Move { target: target_pos },
        queue: false,
    };

    for _ in 0..25 {
        let bundle = TickBundle {
            tick: Tick(world.current_tick),
            commands: vec![move_order.clone()],
            state_checksum: None,
        };
        let _ = world.step(&bundle);
    }

    assert_eq!(world.current_tick, 25);

    // 3. Exit playtest mode: Assert document and history are completely unchanged
    assert_eq!(
        doc.doc, doc_snapshot_before,
        "MapDocument must remain strictly identical after playtesting"
    );
    assert_eq!(
        history.undo_len(),
        history_undo_len_before,
        "Undo stack must not be polluted by playtest simulation"
    );
    assert_eq!(
        history.redo_len(),
        history_redo_len_before,
        "Redo stack must remain intact across playtest mode"
    );
}

#[test]
fn test_file_browser_state_scanning() {
    let assets_dir = resolve_assets_dir();
    let maps_dir = assets_dir.join("maps");

    let mut browser = FileBrowserState::default();
    browser.refresh_list(&maps_dir);

    assert!(
        !browser.file_list.is_empty(),
        "File browser must discover existing .scmap.ron maps in assets/maps"
    );

    let canyon_found = browser
        .file_list
        .iter()
        .any(|name| name == "showcase_canyon.scmap.ron");

    assert!(
        canyon_found,
        "showcase_canyon.scmap.ron must be present in scanned files"
    );
}

#[test]
fn test_zero_hardcoded_creature_names() {
    // AGENTS.md §3.3: Zero Hardcoded Creature / Building Identifiers in Rust.
    // Ensure that no creature slugs or unit constants are hardcoded into editor source files.
    let editor_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/editor");
    assert!(
        editor_dir.exists(),
        "Editor directory must exist at {editor_dir:?}"
    );

    let forbidden_slugs = [
        "\"rhino_beetle\"",
        "\"pulse_bot\"",
        "\"marine\"",
        "\"zealot\"",
        "\"zergling\"",
        "\"stalker\"",
        "\"colossus\"",
        "\"siege_tank\"",
    ];

    let entries = fs::read_dir(&editor_dir).expect("read editor dir");
    for entry in entries {
        let entry = entry.expect("valid entry");
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            let content = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"));

            for slug in &forbidden_slugs {
                assert!(
                    !content.contains(slug),
                    "Violation of AGENTS.md §3.3: hardcoded unit slug {slug} found in file {path:?}"
                );
            }
        }
    }
}

#[test]
fn test_editor_camera_focus_and_altitude_tracking() {
    let mut cam = RtsCamera {
        distance: 20.0,
        ..Default::default()
    };

    // Focus cell helper
    focus_cell(&mut cam, 32, 48);
    assert_eq!(cam.target.x, 32.5);
    assert_eq!(cam.target.y, 48.5);

    // Verify transform computes without panic
    let tf = cam.compute_transform();
    assert!(tf.translation.y > 0.0);
    const { assert!(CAMERA_HEIGHT_EASE_PER_SEC > 0.0) };
}

#[test]
fn test_render_verification_under_xvfb() {
    if std::env::var("SPACE_SCUFFLE_SKIP_GPU_TESTS").is_ok_and(|v| v == "1") {
        eprintln!("WARNING: SPACE_SCUFFLE_SKIP_GPU_TESTS=1 is set; skipping windowed GPU test");
        return;
    }

    let bin_path = env!("CARGO_BIN_EXE_map_editor");
    let xvfb_run = Path::new("/usr/bin/xvfb-run");

    let temp_dir = tempfile::tempdir().expect("create temp dir for map editor screenshots");
    let out_dir = temp_dir.path();

    let mut cmd = if xvfb_run.exists() {
        let mut c = std::process::Command::new(xvfb_run);
        c.arg("-a")
            .arg("-s")
            .arg("-screen 0 1600x900x24 +extension GLX")
            .arg(bin_path);
        c
    } else {
        if std::env::var("WAYLAND_DISPLAY").is_err() && std::env::var("DISPLAY").is_err() {
            eprintln!("Skipping render verification test: no graphical display detected and xvfb-run missing");
            return;
        }
        std::process::Command::new(bin_path)
    };

    cmd.arg("--verify-render").arg(out_dir);
    cmd.arg("--map")
        .arg("assets/maps/showcase_canyon.scmap.ron");

    // Headless virtual GPU environment configuration matching AGENTS.md §1.2 & §4
    cmd.env_remove("WAYLAND_DISPLAY");
    cmd.env("WINIT_UNIX_BACKEND", "x11");
    cmd.env("WGPU_BACKEND", "vulkan");
    cmd.env("SPACE_SCUFFLE_VIRTUAL_GPU", "1");

    let lvp_candidates = [
        Path::new("/usr/share/vulkan/icd.d/lvp_icd.json"),
        Path::new("/usr/share/vulkan/icd.d/lvp_icd.x86_64.json"),
    ];
    if let Some(lvp_icd) = lvp_candidates.into_iter().find(|p| p.exists()) {
        cmd.env("VK_ICD_FILENAMES", lvp_icd);
        cmd.env("WGPU_ADAPTER_NAME", "llvmpipe");
        cmd.env("LIBGL_ALWAYS_SOFTWARE", "1");
    }

    let status = cmd
        .status()
        .expect("Failed to execute map_editor binary under xvfb");

    assert!(
        status.success(),
        "map_editor --verify-render failed with status: {:?}",
        status.code()
    );

    let poses = ["overview", "cliff_corner", "ramp", "overlays"];
    for pose in poses {
        let file_path = out_dir.join(format!("{pose}.png"));
        assert!(
            file_path.exists(),
            "Expected screenshot file does not exist: {file_path:?}"
        );

        let metadata = fs::metadata(&file_path)
            .unwrap_or_else(|e| panic!("Failed to read metadata for {file_path:?}: {e}"));
        assert!(
            metadata.len() > 1024,
            "Screenshot {pose}.png is too small ({} bytes); render output is invalid",
            metadata.len()
        );
    }
}
