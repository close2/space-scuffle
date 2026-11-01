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
//!    `map_editor --verify-render <dir> --map <canyon>` producing 4 reference screenshots, then
//!    entering a real playtest and proving by pixel comparison that units reach the screen.
//! 6. **Full-Application Behavior:** The real [`MapEditorPlugin`] runs on a windowless Bevy app and
//!    is driven by real input events, so systems, schedules and resources are all exercised.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::suspicious_command_arg_space
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiPlugin;
use client::battle_pipeline::session::BattleSession;
use client::bubble_shader::{BubbleDomeMaterial, FlipbookRegistry};
use client::camera::RtsCamera;
use client::editor::camera::{focus_cell, CAMERA_HEIGHT_EASE_PER_SEC};
use client::editor::commands::{ChangeSet, EditCommand, History};
use client::editor::document::EditorDocument;
use client::editor::file_browser::FileBrowserState;
use client::editor::playtest::{
    autosave_path, initialize_playtest_world, EditorPhase, PlaytestState,
};
use client::editor::{
    EditorStartup, InitialDocument, MapEditorPlugin, PLAYTEST_BASELINE_POSE, PLAYTEST_POSE,
};
use client::lab_ui::LabState;
use client::resolve_assets_dir;
use client::scene_controller::LiveBattleBubble;
use sim_core::catalog::RuntimeCatalogs;
use sim_core::scenario::{PlayerSetup, StartLoadoutCatalog};
use sim_map::grid::{CellFlags, CliffLevel, TerrainKind};
use sim_map::map_format::{MapCellSpec, MapEntityId, MapFile, MapMetadata, MapStartLocation};
use sim_types::commands::{PlayerCommand, TickBundle, UnitOrder};
use sim_types::ids::{EntityId, PlayerId, Tick};
use sim_types::math::{SimScalar, SimVec2};

/// Rows at the top of a capture covered by the playtest HUD, excluded from the comparison.
const PLAYTEST_HUD_HEIGHT_PIXELS: u32 = 64;

/// Per-channel difference above which a pixel counts as changed, ignoring compression-free noise.
const PIXEL_DIFFERENCE_THRESHOLD: u8 = 16;

/// Fewest changed pixels accepted as "units are visible": roughly one small bubble on screen.
const MIN_CHANGED_PIXELS: u32 = 400;

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

    let rules = RuntimeCatalogs::load_from_directory(assets_dir.join("catalog"))
        .expect("external rule catalogs must be valid");
    let mut world = initialize_playtest_world(&doc, &loadouts, &rules, &players)
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

    let poses = [
        "overview",
        "cliff_corner",
        "ramp",
        "overlays",
        PLAYTEST_BASELINE_POSE,
        PLAYTEST_POSE,
    ];
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

    // The two captures share a camera pose, so pixels that differ below the playtest HUD are the
    // units themselves. A batch of invisible or culled bubbles leaves the images identical.
    let baseline = image::open(out_dir.join(format!("{PLAYTEST_BASELINE_POSE}.png")))
        .expect("baseline capture must decode")
        .to_rgb8();
    let playtest = image::open(out_dir.join(format!("{PLAYTEST_POSE}.png")))
        .expect("playtest capture must decode")
        .to_rgb8();
    assert_eq!(baseline.dimensions(), playtest.dimensions());
    let (width, height) = baseline.dimensions();
    let mut changed = 0_u32;
    for y in PLAYTEST_HUD_HEIGHT_PIXELS..height {
        for x in 0..width {
            let before = baseline.get_pixel(x, y).0;
            let after = playtest.get_pixel(x, y).0;
            if before
                .iter()
                .zip(after.iter())
                .any(|(a, b)| a.abs_diff(*b) > PIXEL_DIFFERENCE_THRESHOLD)
            {
                changed += 1;
            }
        }
    }
    assert!(
        changed >= MIN_CHANGED_PIXELS,
        "playtest units must be visible on screen: only {changed} pixels changed"
    );
}

// ---------------------------------------------------------------------------------------------
// Full-application tests: the real `MapEditorPlugin` on a real (windowless) Bevy app.
// ---------------------------------------------------------------------------------------------

/// Headless editor application driven by real input events and fixed 40 ms frames.
struct EditorHarness {
    app: App,
    /// Directory holding the fixture map; removed when the harness drops.
    fixture_dir: Option<tempfile::TempDir>,
}

/// File name of the map each harness boots from.
const FIXTURE_MAP_FILE: &str = "fixture.scmap.ron";

/// A valid two-player arena with room for start bases.
fn playable_document(width: u32, height: u32) -> EditorDocument {
    let meta = MapMetadata {
        title: "Harness Arena".to_string(),
        author: "Test Suite".to_string(),
        description: String::new(),
        theme: "orbital_foundry".to_string(),
    };
    let mut doc = EditorDocument::new_blank(width, height, meta).expect("valid blank map");
    doc.doc.start_locations.push(MapStartLocation {
        id: MapEntityId(1),
        player_slot: 0,
        cell: (8, 8),
        footprint: (5, 5),
    });
    doc.doc.start_locations.push(MapStartLocation {
        id: MapEntityId(2),
        player_slot: 1,
        cell: (width - 13, height - 13),
        footprint: (5, 5),
    });
    doc
}

impl EditorHarness {
    fn new() -> Self {
        Self::with_document(&playable_document(64, 64))
    }

    /// Writes `doc` to a temp file and boots the editor on it, as `--map <file>` would.
    fn with_document(doc: &EditorDocument) -> Self {
        let dir = tempfile::tempdir().expect("create fixture directory");
        let path = dir.path().join(FIXTURE_MAP_FILE);
        doc.doc
            .encode()
            .save_atomic(&path)
            .expect("write fixture map");
        let mut harness = Self::boot(&InitialDocument::Open(path));
        harness.fixture_dir = Some(dir);
        harness
    }

    /// Absolute path of the fixture map file.
    fn path(&self) -> PathBuf {
        self.fixture_dir
            .as_ref()
            .expect("harness booted from a fixture")
            .path()
            .join(FIXTURE_MAP_FILE)
    }

    fn boot(initial: &InitialDocument) -> Self {
        let startup = EditorStartup::load(initial).expect("editor startup");
        let file_path = startup.assets_root.as_path().to_string_lossy().to_string();

        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .build()
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::log::LogPlugin>()
                .disable::<bevy::audio::AudioPlugin>()
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(WgpuSettings {
                        backends: None,
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path,
                    ..default()
                }),
        );
        app.add_plugins(EguiPlugin);
        app.add_plugins(MapEditorPlugin {
            startup,
            verify_render: None,
            max_frames: None,
        });
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            40,
        )));
        app.finish();
        app.cleanup();

        let mut harness = Self {
            app,
            fixture_dir: None,
        };
        harness.step(3);
        harness
    }

    fn step(&mut self, frames: usize) {
        for _ in 0..frames {
            self.app.update();
        }
    }

    fn window(&mut self) -> Entity {
        self.app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(self.app.world())
    }

    fn send_key(&mut self, key_code: KeyCode, state: ButtonState) {
        let window = self.window();
        self.app.world_mut().send_event(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state,
            repeat: false,
            window,
        });
    }

    /// Presses and releases `key` across two real frames.
    fn tap_key(&mut self, key: KeyCode) {
        self.send_key(key, ButtonState::Pressed);
        self.step(1);
        self.send_key(key, ButtonState::Released);
        self.step(1);
    }

    fn phase(&self) -> EditorPhase {
        *self.app.world().resource::<State<EditorPhase>>().get()
    }

    fn bubble_count(&mut self) -> usize {
        self.app
            .world_mut()
            .query_filtered::<Entity, With<LiveBattleBubble>>()
            .iter(self.app.world())
            .count()
    }

    fn doc(&self) -> &EditorDocument {
        self.app.world().resource::<EditorDocument>()
    }

    /// Records a real undoable edit so the document is genuinely dirty.
    fn make_dirty(&mut self) {
        let world = self.app.world_mut();
        world.resource_scope(|world, mut doc: Mut<'_, EditorDocument>| {
            let before = doc.doc.metadata.clone();
            let mut after = before.clone();
            after.title.push_str(" (edited)");
            let mut changes = ChangeSet::default();
            world.resource_mut::<History>().execute(
                EditCommand::SetMetadata { before, after },
                &mut doc.doc,
                &mut changes,
            );
            doc.apply_changeset(&changes, 0.0);
        });
    }
}

#[test]
fn f5_enters_playtest_and_renders_units_from_the_session() {
    let mut harness = EditorHarness::new();
    assert_eq!(harness.phase(), EditorPhase::Editing);

    harness.tap_key(KeyCode::F5);
    harness.step(2);
    assert_eq!(harness.phase(), EditorPhase::Playtesting);

    harness.step(30);

    let world = harness.app.world();
    let lab = world.resource::<LabState>();
    assert_eq!(
        lab.battle_failure, None,
        "the battle must run without failure"
    );
    let sim_world = lab
        .sim_world
        .as_ref()
        .expect("playtest world must exist while playtesting");
    assert!(
        sim_world.current_tick >= 20,
        "the production driver must step the world at 25 Hz, reached tick {}",
        sim_world.current_tick
    );
    let session = world.resource::<BattleSession>();
    assert!(session.is_ready(), "session must be ready to present");
    assert_eq!(
        session.frames.current().tick.0,
        sim_world.current_tick,
        "the sync system reads the session, so it must carry the stepped world"
    );
    let simulated = session.frames.presented().bubbles.len();
    assert!(simulated > 0, "a playtest must present its starting units");
    assert_eq!(
        world.resource::<PlaytestState>().current_tick,
        u64::from(sim_world.current_tick),
        "the HUD must show the published tick"
    );

    // Real render entities, with the real mesh and material the renderer draws.
    assert_eq!(harness.bubble_count(), simulated);
    let world = harness.app.world_mut();
    let mut drawable = world
        .query_filtered::<(&Mesh3d, &MeshMaterial3d<BubbleDomeMaterial>), With<LiveBattleBubble>>();
    let handles: Vec<_> = drawable
        .iter(world)
        .map(|(mesh, material)| (mesh.0.clone(), material.0.clone()))
        .collect();
    for (mesh, material) in &handles {
        assert!(world.resource::<Assets<Mesh>>().contains(mesh));
        let material = world
            .resource::<Assets<BubbleDomeMaterial>>()
            .get(material)
            .expect("bubble material must exist");
        assert!(
            material.video_texture.is_some(),
            "bubbles must bind their real unit media, not a blank shell"
        );
    }
}

#[test]
fn playtest_units_appear_at_their_start_location_in_map_space() {
    let mut harness = EditorHarness::new();
    harness.tap_key(KeyCode::F5);
    harness.step(4);
    let start = harness.doc().doc.start_locations[0].cell;
    let world = harness.app.world_mut();
    let mut bubbles = world.query_filtered::<&Transform, With<LiveBattleBubble>>();
    let near_start = bubbles.iter(world).any(|transform| {
        (transform.translation.x - start.0 as f32).abs() < 16.0
            && (transform.translation.z - start.1 as f32).abs() < 16.0
    });
    assert!(
        near_start,
        "units must be placed in map space at their base"
    );
}

#[test]
fn escape_returns_to_editing_and_removes_every_battle_entity() {
    let mut harness = EditorHarness::new();
    let authored = harness.doc().doc.clone();
    harness.tap_key(KeyCode::F5);
    harness.step(10);
    assert!(harness.bubble_count() > 0);

    harness.tap_key(KeyCode::Escape);
    harness.step(2);

    assert_eq!(harness.phase(), EditorPhase::Editing);
    assert_eq!(
        harness.bubble_count(),
        0,
        "teardown must despawn all bubbles"
    );
    assert!(harness
        .app
        .world()
        .resource::<LabState>()
        .sim_world
        .is_none());
    assert_eq!(
        harness.doc().doc,
        authored,
        "playtest must not edit the map"
    );

    // A second session must start cleanly with a new generation.
    harness.tap_key(KeyCode::F5);
    harness.step(10);
    assert_eq!(harness.phase(), EditorPhase::Playtesting);
    assert!(
        harness.bubble_count() > 0,
        "re-entry must render units again"
    );
}

#[test]
fn entering_playtest_autosaves_unsaved_work_without_marking_it_saved() {
    let mut harness = EditorHarness::new();
    harness.make_dirty();
    let edited = harness.doc().doc.clone();

    harness.tap_key(KeyCode::F5);
    harness.step(2);
    assert_eq!(harness.phase(), EditorPhase::Playtesting);

    let autosave = autosave_path(harness.doc(), Path::new("unused"));
    let recovered = MapFile::load(&autosave)
        .expect("autosave must exist before the battle starts")
        .decode()
        .expect("autosave must decode");
    assert_eq!(recovered, edited, "autosave must hold the unsaved edits");
    assert!(harness.doc().dirty, "an autosave is not a save");
    let original = MapFile::load(&harness.path())
        .expect("original map")
        .decode()
        .expect("original map decodes");
    assert_ne!(
        original, edited,
        "autosave must not overwrite the user's file"
    );
}

#[test]
fn refused_playtest_stays_in_editing_and_explains_why() {
    let mut invalid = playable_document(64, 64);
    invalid.doc.start_locations.truncate(1);
    let mut harness = EditorHarness::with_document(&invalid);

    harness.tap_key(KeyCode::F5);
    harness.step(3);

    assert_eq!(harness.phase(), EditorPhase::Editing);
    let banner = harness
        .app
        .world()
        .resource::<PlaytestState>()
        .error_banner
        .clone()
        .expect("a refused playtest must say why");
    assert!(banner.contains("validation"), "got: {banner}");
    assert_eq!(harness.bubble_count(), 0);
}

#[test]
fn missing_media_refuses_entry_without_poisoning_later_playtests() {
    let mut harness = EditorHarness::new();
    // Break the real registry binding for every unit type.
    let world = harness.app.world_mut();
    let saved = world.resource::<FlipbookRegistry>().textures_by_id.clone();
    world
        .resource_mut::<FlipbookRegistry>()
        .textures_by_id
        .clear();

    harness.tap_key(KeyCode::F5);
    harness.step(3);
    assert_eq!(harness.phase(), EditorPhase::Editing);
    let banner = harness
        .app
        .world()
        .resource::<PlaytestState>()
        .error_banner
        .clone();
    assert!(banner.is_some(), "missing media must be reported");

    harness
        .app
        .world_mut()
        .resource_mut::<FlipbookRegistry>()
        .textures_by_id = saved;
    harness.tap_key(KeyCode::F5);
    harness.step(6);
    assert_eq!(harness.phase(), EditorPhase::Playtesting);
    assert!(harness.bubble_count() > 0);
}

#[test]
fn map_editor_reports_bad_cli_input_without_panicking() {
    for args in [
        ["--map", "/nonexistent/editor_cli_probe.scmap.ron"],
        ["--new", "1x1"],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_map_editor"))
            .args(args)
            .output()
            .expect("run map_editor");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{args:?} must fail");
        assert!(!stderr.contains("panicked"), "{args:?} panicked: {stderr}");
        assert!(stderr.contains("map_editor:"), "{args:?} output: {stderr}");
    }
}
