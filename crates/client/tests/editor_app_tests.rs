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
    clippy::needless_pass_by_value,
    clippy::cast_possible_truncation,
    clippy::suspicious_command_arg_space
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PrimaryWindow, WindowCloseRequested};
use bevy_egui::EguiPlugin;
use client::battle_pipeline::session::BattleSession;
use client::battle_selection::BattleSelection;
use client::bubble_shader::{BubbleDomeMaterial, BubbleInstance, FlipbookRegistry};
use client::camera::RtsCamera;
use client::editor::camera::{focus_cell, CAMERA_HEIGHT_EASE_PER_SEC};
use client::editor::commands::{ChangeSet, EditCommand, History};
use client::editor::document::EditorDocument;
use client::editor::file_browser::FileBrowserState;
use client::editor::playtest::{
    autosave_path, initialize_playtest_world, EditorPhase, PlaytestState,
};
use client::editor::tools::PaintOp;
use client::editor::{
    editor_ui_system, editor_window_plugin, EditorStartup, EditorTool, EditorToolState,
    InitialDocument, LoadDocumentRequest, LosPreviewState, MapEditorPlugin, PendingUnsavedAction,
    SymmetryMode, UnsavedChangesModalState, PLAYTEST_BASELINE_POSE, PLAYTEST_POSE,
};
use client::lab_ui::LabState;
use client::resolve_assets_dir;
use client::scene_controller::LiveBattleBubble;
use client::terrain::{
    TerrainChunks, TerrainFieldImages, TerrainMaterial, TerrainParamsState, TerrainState,
    CHUNK_DUAL_CELLS, OVERLAY_FLAG_BUILDABILITY, OVERLAY_FLAG_GRID, OVERLAY_FLAG_PATHABILITY,
};
use sim_core::catalog::RuntimeCatalogs;
use sim_core::scenario::{PlayerSetup, StartLoadoutCatalog};
use sim_map::grid::{CellFlags, CliffLevel, RampDir, ResourceKind, TerrainKind};
use sim_map::map_format::{
    MapCellSpec, MapEntityId, MapFile, MapMetadata, MapResource, MapStartLocation,
};
use sim_map::validation::validate_map;
use sim_types::commands::{PlayerCommand, TickBundle, UnitOrder};
use sim_types::enums::ShapeKind;
use sim_types::ids::{EntityId, PlayerId, Tick};
use sim_types::math::{SimScalar, SimVec2};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

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

    let mut directories = vec![editor_dir];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory).expect("read editor dir") {
            let path = entry.expect("valid entry").path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
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
    if gpu_run::gpu_tests_opted_out() {
        return;
    }

    let temp_dir = tempfile::tempdir().expect("create temp dir for map editor screenshots");
    let out_dir = temp_dir.path();

    let poses = [
        "overview",
        "cliff_corner",
        "ramp",
        "overlays",
        PLAYTEST_BASELINE_POSE,
        PLAYTEST_POSE,
    ];

    // Leftovers from an earlier run: valid PNGs, but far too small to pass the checks below. A
    // run that accepts them instead of capturing fresh output must fail this test.
    for pose in poses {
        image::RgbImage::new(4, 4)
            .save(out_dir.join(format!("{pose}.png")))
            .expect("seed stale capture");
    }

    // The editor window is 1600x900, which must fit on the virtual screen.
    let mut cmd = gpu_run::virtual_gpu_command_on_screen(
        Path::new(env!("CARGO_BIN_EXE_map_editor")),
        Some("1600x900x24"),
    );
    cmd.arg("--verify-render").arg(out_dir);
    cmd.arg("--map")
        .arg("assets/maps/showcase_canyon.scmap.ron");
    gpu_run::assert_passes_validation(&mut cmd, "map_editor --verify-render");

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
                })
                .set(editor_window_plugin()),
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

    /// Moves the OS cursor to `position` (logical pixels) for both Bevy and egui.
    fn move_cursor(&mut self, position: Vec2) {
        let window = self.window();
        self.app
            .world_mut()
            .query::<&mut Window>()
            .single_mut(self.app.world_mut())
            .set_cursor_position(Some(position));
        self.app.world_mut().send_event(CursorMoved {
            window,
            position,
            delta: None,
        });
        self.step(1);
    }

    /// Removes the cursor from the window, as dragging outside it does.
    fn cursor_leaves_window(&mut self) {
        self.app
            .world_mut()
            .query::<&mut Window>()
            .single_mut(self.app.world_mut())
            .set_cursor_position(None);
        self.step(1);
    }

    /// Returns the window position showing the center of cell `(x, y)`.
    fn cell_on_screen(&mut self, x: u32, y: u32) -> Vec2 {
        let terrain = self.app.world().resource::<TerrainState>();
        let ground = Vec3::new(
            x as f32 + 0.5,
            terrain.map.terrain_altitude_at(SimVec2::new(
                SimScalar::from_num(x as f32 + 0.5),
                SimScalar::from_num(y as f32 + 0.5),
            )),
            y as f32 + 0.5,
        );
        let world = self.app.world_mut();
        let (camera, transform) = world
            .query_filtered::<(&Camera, &GlobalTransform), With<RtsCamera>>()
            .single(world);
        camera
            .world_to_viewport(transform, ground)
            .expect("cell must be in front of the editor camera")
    }

    fn send_mouse(&mut self, button: MouseButton, state: ButtonState) {
        let window = self.window();
        self.app.world_mut().send_event(MouseButtonInput {
            button,
            state,
            window,
        });
        self.step(1);
    }

    fn press_left(&mut self) {
        self.send_mouse(MouseButton::Left, ButtonState::Pressed);
    }

    fn release_left(&mut self) {
        self.send_mouse(MouseButton::Left, ButtonState::Released);
    }

    /// Selects cliff level 2 and presses the primary button over cell `(x, y)` with the Brush.
    ///
    /// Blank maps are level 1, so the tool's default level would paint nothing.
    fn begin_stroke_at(&mut self, x: u32, y: u32) {
        self.tap_key(KeyCode::Digit3);
        let position = self.cell_on_screen(x, y);
        self.move_cursor(position);
        self.press_left();
    }

    fn cell_level(&self, x: u32, y: u32) -> u8 {
        self.doc().doc.cell(x, y).expect("cell in bounds").level.0
    }

    /// Writes `doc` beside the fixture map and opens it through the real load path.
    fn open(&mut self, doc: &EditorDocument, file_name: &str) -> PathBuf {
        let path = self
            .path()
            .parent()
            .expect("fixture has a directory")
            .join(file_name);
        doc.doc.encode().save_atomic(&path).expect("write map");
        self.request_open(path.clone());
        path
    }

    fn request_open(&mut self, path: PathBuf) {
        self.app
            .world_mut()
            .send_event(LoadDocumentRequest::Open(path));
        self.step(3);
    }

    fn terrain_level(&self, x: u32, y: u32) -> u8 {
        self.app
            .world()
            .resource::<TerrainState>()
            .map
            .cell(x, y)
            .expect("cell in bounds")
            .cliff_level
            .0
    }

    /// Returns the text on the status bar.
    fn status(&self) -> String {
        self.app
            .world()
            .resource::<EditorToolState>()
            .last_status_message
            .clone()
            .unwrap_or_default()
    }

    /// Drags the primary button from cell `from` to cell `to`.
    fn drag_cells(&mut self, from: (u32, u32), to: (u32, u32)) {
        let start = self.cell_on_screen(from.0, from.1);
        self.move_cursor(start);
        self.press_left();
        let end = self.cell_on_screen(to.0, to.1);
        self.move_cursor(end);
        self.release_left();
        self.step(1);
    }

    /// Selects the Eraser and clicks cell `(x, y)` once.
    fn erase_at(&mut self, x: u32, y: u32) {
        self.tap_key(KeyCode::KeyT);
        let position = self.cell_on_screen(x, y);
        self.move_cursor(position);
        self.press_left();
        self.release_left();
    }

    fn history(&self) -> &History {
        self.app.world().resource::<History>()
    }

    /// Delivers the operating system's close request, as clicking the window's X does.
    fn close_window(&mut self) {
        let window = self.window();
        self.app
            .world_mut()
            .send_event(WindowCloseRequested { window });
        self.step(2);
    }

    /// Returns the live window count; Bevy exits and tears down once this reaches zero.
    fn window_count(&mut self) -> usize {
        self.app
            .world_mut()
            .query::<&Window>()
            .iter(self.app.world())
            .count()
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
    let session = world.resource::<BattleSession>();
    let sim_world = session
        .world()
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
        .resource::<BattleSession>()
        .world()
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
    let status = harness.status();
    assert!(status.contains("Cannot playtest"), "got: {status}");
    assert!(status.contains("validation"), "got: {status}");
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
    assert!(
        harness.status().contains("Cannot playtest"),
        "missing media must reach the status bar"
    );

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

#[test]
fn window_close_with_unsaved_work_asks_instead_of_closing() {
    let mut harness = EditorHarness::new();
    harness.make_dirty();

    harness.close_window();

    assert_eq!(harness.window_count(), 1, "the window must stay open");
    assert_eq!(harness.app.should_exit(), None);
    let modal = harness.app.world().resource::<UnsavedChangesModalState>();
    assert!(modal.is_open);
    assert_eq!(modal.pending_action, Some(PendingUnsavedAction::Quit));
}

#[test]
fn a_failed_playtest_battle_keeps_the_editor_open_for_the_unsaved_prompt() {
    let mut harness = EditorHarness::new();
    harness.make_dirty();
    harness.tap_key(KeyCode::F5);
    harness.step(4);
    assert_eq!(harness.phase(), EditorPhase::Playtesting);

    // A world that disagrees with its published snapshot is a fatal pipeline error.
    harness
        .app
        .world_mut()
        .resource_mut::<BattleSession>()
        .world_mut()
        .expect("playtest world")
        .current_tick += 7;
    // Exit events expire after two frames, so each frame is checked.
    for _ in 0..3 {
        harness.step(1);
        assert_eq!(harness.app.should_exit(), None, "the editor must stay open");
    }
    let failure = harness
        .app
        .world()
        .resource::<LabState>()
        .battle_failure
        .clone();
    assert!(failure.is_some(), "the HUD must have something to report");

    harness.tap_key(KeyCode::Escape);
    harness.step(2);
    assert_eq!(harness.phase(), EditorPhase::Editing);
    assert!(harness.doc().dirty, "unsaved work must survive");

    harness.close_window();
    let modal = harness.app.world().resource::<UnsavedChangesModalState>();
    assert!(modal.is_open, "closing must still ask about unsaved work");
    assert_eq!(harness.app.should_exit(), None);
}

#[test]
fn window_close_during_playtest_returns_to_the_unsaved_prompt() {
    let mut harness = EditorHarness::new();
    harness.make_dirty();
    harness.tap_key(KeyCode::F5);
    harness.step(4);
    assert_eq!(harness.phase(), EditorPhase::Playtesting);

    harness.close_window();
    harness.step(2);

    assert_eq!(harness.window_count(), 1, "the window must stay open");
    assert_eq!(harness.app.should_exit(), None);
    assert_eq!(harness.phase(), EditorPhase::Editing);
    let modal = harness.app.world().resource::<UnsavedChangesModalState>();
    assert!(modal.is_open, "the prompt only renders while editing");
    assert_eq!(modal.pending_action, Some(PendingUnsavedAction::Quit));
}

#[test]
fn window_close_exits_cleanly_when_there_is_nothing_to_save() {
    let mut harness = EditorHarness::new();
    harness.close_window();
    assert_eq!(harness.app.should_exit(), Some(AppExit::Success));
}

#[test]
fn window_close_during_a_clean_playtest_exits() {
    let mut harness = EditorHarness::new();
    harness.tap_key(KeyCode::F5);
    harness.step(4);
    harness.close_window();
    assert_eq!(harness.app.should_exit(), Some(AppExit::Success));
}

#[test]
fn stroke_released_over_a_ui_panel_keeps_its_undo_entry() {
    let mut harness = EditorHarness::new();
    harness.begin_stroke_at(32, 32);
    assert_eq!(harness.cell_level(32, 32), 2, "the stroke must paint live");

    // The left toolbox panel covers this window position. egui samples the pointer once per
    // frame, so give it a frame to report that it owns the pointer.
    harness.move_cursor(Vec2::new(100.0, 450.0));
    harness.step(1);
    assert_eq!(
        harness
            .app
            .world()
            .resource::<EditorToolState>()
            .hovered_cell,
        None,
        "the panel must own the pointer for this test to mean anything"
    );
    harness.release_left();
    harness.step(1);

    assert!(
        !harness
            .app
            .world()
            .resource::<EditorToolState>()
            .is_painting
    );
    assert_eq!(
        harness.history().undo_len(),
        1,
        "the stroke must be undoable"
    );
    harness.send_key(KeyCode::ControlLeft, ButtonState::Pressed);
    harness.step(1);
    harness.tap_key(KeyCode::KeyZ);
    assert_eq!(
        harness.cell_level(32, 32),
        1,
        "undo must restore the stroke"
    );
}

#[test]
fn stroke_released_outside_the_window_keeps_its_undo_entry() {
    let mut harness = EditorHarness::new();
    harness.begin_stroke_at(32, 32);
    harness.cursor_leaves_window();
    harness.release_left();
    harness.step(1);

    assert!(
        !harness
            .app
            .world()
            .resource::<EditorToolState>()
            .is_painting
    );
    assert_eq!(harness.history().undo_len(), 1);
}

#[test]
fn undo_and_playtest_are_blocked_during_an_active_stroke() {
    let mut harness = EditorHarness::new();
    harness.make_dirty();
    assert_eq!(harness.history().undo_len(), 1);
    harness.begin_stroke_at(32, 32);

    harness.send_key(KeyCode::ControlLeft, ButtonState::Pressed);
    harness.step(1);
    harness.tap_key(KeyCode::KeyZ);
    harness.send_key(KeyCode::ControlLeft, ButtonState::Released);
    harness.step(1);
    assert_eq!(
        harness.history().undo_len(),
        1,
        "undo during a live stroke would corrupt the recorded before-states"
    );

    harness.tap_key(KeyCode::F5);
    harness.step(2);
    assert_eq!(harness.phase(), EditorPhase::Editing);

    harness.release_left();
    harness.step(1);
    assert_eq!(harness.history().undo_len(), 2);
}

#[test]
fn ctrl_shift_z_redoes_and_ctrl_z_undoes() {
    let mut harness = EditorHarness::new();
    harness.make_dirty();

    harness.send_key(KeyCode::ControlLeft, ButtonState::Pressed);
    harness.step(1);
    harness.tap_key(KeyCode::KeyZ);
    assert_eq!(harness.history().undo_len(), 0);
    assert_eq!(harness.history().redo_len(), 1);

    harness.send_key(KeyCode::ShiftLeft, ButtonState::Pressed);
    harness.step(1);
    harness.tap_key(KeyCode::KeyZ);
    assert_eq!(harness.history().redo_len(), 0, "Ctrl+Shift+Z must redo");
    assert_eq!(harness.history().undo_len(), 1);

    harness.send_key(KeyCode::ShiftLeft, ButtonState::Released);
    harness.step(1);
    harness.tap_key(KeyCode::KeyZ);
    harness.tap_key(KeyCode::KeyY);
    assert_eq!(harness.history().undo_len(), 1, "Ctrl+Y must still redo");
}

#[test]
fn symmetric_brush_updates_the_mirrored_terrain() {
    for (mode, mirror) in [
        (SymmetryMode::MirrorX, (43, 32)),
        (SymmetryMode::MirrorY, (20, 31)),
        (SymmetryMode::Rotate180, (43, 31)),
    ] {
        let mut harness = EditorHarness::new();
        harness
            .app
            .world_mut()
            .resource_mut::<EditorToolState>()
            .symmetry = mode;

        harness.begin_stroke_at(20, 32);
        harness.release_left();
        harness.step(1);

        assert_eq!(harness.cell_level(20, 32), 2, "{mode:?}: primary document");
        assert_eq!(
            harness.terrain_level(20, 32),
            2,
            "{mode:?}: primary terrain"
        );
        assert_eq!(
            harness.cell_level(mirror.0, mirror.1),
            2,
            "{mode:?}: mirrored document"
        );
        assert_eq!(
            harness.terrain_level(mirror.0, mirror.1),
            2,
            "{mode:?}: the renderer must show what the document holds"
        );
    }
}

#[test]
fn opening_a_different_size_map_rebuilds_terrain_for_that_size() {
    let mut harness = EditorHarness::new();
    harness.make_dirty();
    let replacement = playable_document(96, 80);

    harness.open(&replacement, "larger.scmap.ron");

    assert_eq!(harness.doc().doc.width, 96);
    assert!(
        !harness.doc().dirty,
        "a freshly opened map has no unsaved work"
    );
    assert_eq!(harness.history().undo_len(), 0);

    let expected_chunks = 96_u32.div_ceil(CHUNK_DUAL_CELLS) * 80_u32.div_ceil(CHUNK_DUAL_CELLS);
    let world = harness.app.world();
    let chunks = world.resource::<TerrainChunks>();
    assert_eq!(chunks.chunks.len() as u32, expected_chunks);
    assert_eq!(chunks.cols, 96_u32.div_ceil(CHUNK_DUAL_CELLS));

    let fields = world.resource::<TerrainFieldImages>();
    let images = world.resource::<Assets<Image>>();
    for handle in [&fields.splat, &fields.overlay] {
        let image = images.get(handle).expect("field image exists");
        assert_eq!(image.size(), UVec2::new(96, 80), "field texture size");
    }
    assert_eq!(
        world.resource::<TerrainParamsState>().map_size,
        Vec2::new(96.0, 80.0)
    );
    let los = world.resource::<LosPreviewState>();
    assert_eq!(los.vision_map.width(), 96);

    let world = harness.app.world_mut();
    let rendered = world
        .query_filtered::<Entity, With<MeshMaterial3d<TerrainMaterial>>>()
        .iter(world)
        .count();
    assert_eq!(
        rendered as u32, expected_chunks,
        "no chunk of the previous map may survive"
    );

    // Cells beyond the old 64x64 bounds must be editable and rendered.
    harness.begin_stroke_at(66, 40);
    harness.release_left();
    harness.step(1);
    assert_eq!(harness.cell_level(66, 40), 2);
    assert_eq!(harness.terrain_level(66, 40), 2);
    assert_eq!(harness.history().undo_len(), 1);
}

#[test]
fn opening_a_map_with_an_unknown_theme_is_refused_and_reported() {
    let mut harness = EditorHarness::new();
    harness.make_dirty();
    let edited = harness.doc().doc.clone();
    let mut unknown = playable_document(96, 80);
    unknown.doc.metadata.theme = "theme_absent_from_catalog".to_string();

    harness.open(&unknown, "unknown_theme.scmap.ron");

    assert_eq!(
        harness.doc().doc,
        edited,
        "a failed open must keep the document"
    );
    assert!(harness.doc().dirty);
    assert_eq!(harness.history().undo_len(), 1, "and its undo history");
    let message = harness
        .app
        .world()
        .resource::<EditorToolState>()
        .last_status_message
        .clone()
        .expect("the failure must be reported");
    assert!(
        message.contains("theme_absent_from_catalog"),
        "got: {message}"
    );
}

#[test]
fn opening_a_corrupt_file_is_refused_and_reported() {
    let mut harness = EditorHarness::new();
    let authored = harness.doc().doc.clone();
    let corrupt = harness.path().with_file_name("corrupt.scmap.ron");
    fs::write(&corrupt, "(not a map").expect("write corrupt file");

    harness.request_open(corrupt);

    assert_eq!(harness.doc().doc, authored);
    let browser = harness.app.world().resource::<FileBrowserState>();
    assert!(browser.is_open, "the open dialog must stay available");
    assert!(browser.error_banner.is_some(), "the failure must be shown");
}

#[test]
fn eraser_click_on_abyss_changes_nothing() {
    let mut fixture = playable_document(64, 64);
    let abyss = PaintOp::Terrain(TerrainKind::Abyss);
    let cell = fixture.doc.cell_mut(32, 32).expect("cell in bounds");
    *cell = abyss.apply_to_cell(cell);
    let mut harness = EditorHarness::with_document(&fixture);

    harness.erase_at(32, 32);

    assert_eq!(validate_map(&harness.doc().doc).errors, Vec::new());
    assert_eq!(harness.history().undo_len(), 0);
    assert!(!harness.doc().dirty);
}

#[test]
fn eraser_never_makes_a_mirrored_abyss_cell_traversable() {
    let mut fixture = playable_document(64, 64);
    let abyss = PaintOp::Terrain(TerrainKind::Abyss);
    let mirrored = fixture.doc.cell_mut(43, 32).expect("cell in bounds");
    *mirrored = abyss.apply_to_cell(mirrored);
    let primary = fixture.doc.cell_mut(20, 32).expect("cell in bounds");
    primary.flags.insert(CellFlags::LOS_BLOCKER);
    let mut harness = EditorHarness::with_document(&fixture);
    harness
        .app
        .world_mut()
        .resource_mut::<EditorToolState>()
        .symmetry = SymmetryMode::MirrorX;

    harness.erase_at(20, 32);

    assert_eq!(validate_map(&harness.doc().doc).errors, Vec::new());
    let primary = harness.doc().doc.cell(20, 32).expect("cell in bounds");
    assert!(!primary.flags.contains(CellFlags::LOS_BLOCKER));
    assert_eq!(harness.history().undo_len(), 1);
}

#[test]
fn eraser_mirrors_cell_edits_under_symmetry() {
    let mut fixture = playable_document(64, 64);
    for (x, y) in [(20, 32), (43, 32)] {
        let cell = fixture.doc.cell_mut(x, y).expect("cell in bounds");
        cell.flags.insert(CellFlags::LOS_BLOCKER);
    }
    let mut harness = EditorHarness::with_document(&fixture);
    harness
        .app
        .world_mut()
        .resource_mut::<EditorToolState>()
        .symmetry = SymmetryMode::MirrorX;

    harness.erase_at(20, 32);

    for (x, y) in [(20, 32), (43, 32)] {
        let flags = harness.doc().doc.cell(x, y).expect("cell in bounds").flags;
        assert!(
            !flags.contains(CellFlags::LOS_BLOCKER),
            "({x}, {y}) must be erased"
        );
    }
    assert_eq!(harness.history().undo_len(), 1, "one click is one edit");
}

#[test]
fn idle_frames_do_not_mark_the_document_or_terrain_changed() {
    #[derive(Resource, Default)]
    struct ChangeCounts {
        doc: u32,
        terrain: u32,
        params: u32,
    }

    fn count_changes(
        doc: Res<'_, EditorDocument>,
        terrain: Res<'_, TerrainState>,
        params: Res<'_, TerrainParamsState>,
        mut counts: ResMut<'_, ChangeCounts>,
    ) {
        counts.doc += u32::from(doc.is_changed());
        counts.terrain += u32::from(terrain.is_changed());
        counts.params += u32::from(params.is_changed());
    }

    let mut harness = EditorHarness::new();
    harness
        .app
        .init_resource::<ChangeCounts>()
        .add_systems(Update, count_changes.after(editor_ui_system));
    harness.step(5);
    *harness.app.world_mut().resource_mut::<ChangeCounts>() = ChangeCounts::default();

    harness.step(10);

    let counts = harness.app.world().resource::<ChangeCounts>();
    assert_eq!(
        counts.doc, 0,
        "markers rebuild whenever the document changes"
    );
    assert_eq!(
        counts.terrain, 0,
        "markers rebuild whenever the terrain changes"
    );
    assert_eq!(
        counts.params, 0,
        "params changes re-upload the terrain material"
    );
}

/// A map with a level 2 plateau on rows `16..28` and level 1 ground from row 28 onward.
///
/// The editor camera looks toward -Y, so the plateau is behind its foot rather than hiding it.
fn plateau_document() -> EditorDocument {
    let mut fixture = playable_document(64, 64);
    for y in 16..28 {
        for x in 20..44 {
            fixture.doc.cell_mut(x, y).expect("cell in bounds").level = CliffLevel(2);
        }
    }
    fixture
}

#[test]
fn ramp_tool_drag_places_the_whole_rectangle() {
    let mut harness = EditorHarness::with_document(&plateau_document());
    harness.tap_key(KeyCode::KeyR);

    harness.drag_cells((30, 28), (33, 29));

    for y in 28..=29 {
        for x in 30..=33 {
            let cell = harness.doc().doc.cell(x, y).expect("cell in bounds");
            assert_eq!(cell.ramp, Some(RampDir::South), "ramp at ({x}, {y})");
        }
    }
    assert_eq!(harness.history().undo_len(), 1, "one drag is one edit");
    assert_eq!(
        harness
            .app
            .world()
            .resource::<TerrainState>()
            .map
            .cell(33, 29)
            .expect("in bounds")
            .ramp,
        Some(RampDir::South),
        "the renderer must show the ramp"
    );
}

#[test]
fn ramp_tool_click_still_places_a_single_cell_ramp() {
    let mut harness = EditorHarness::with_document(&plateau_document());
    harness.tap_key(KeyCode::KeyR);

    harness.drag_cells((30, 28), (30, 28));

    let cell = harness.doc().doc.cell(30, 28).expect("cell in bounds");
    assert_eq!(cell.ramp, Some(RampDir::South));
}

#[test]
fn ramp_tool_reports_a_rejected_drag() {
    let mut harness = EditorHarness::with_document(&plateau_document());
    harness.tap_key(KeyCode::KeyR);

    // Spans the cliff edge, so its cells have mixed levels.
    harness.drag_cells((30, 26), (33, 29));

    assert!(
        harness.status().starts_with("Ramp error"),
        "got: {}",
        harness.status()
    );
    assert_eq!(harness.history().undo_len(), 0);
}

fn document_with_mineral(cell: (u32, u32)) -> EditorDocument {
    let mut fixture = playable_document(64, 64);
    fixture.doc.resources.push(MapResource {
        id: MapEntityId(10),
        kind: ResourceKind::Mineral,
        cell,
        amount: 1500,
    });
    fixture
}

#[test]
fn select_tool_drag_moves_an_entity_as_one_undoable_edit() {
    let mut harness = EditorHarness::with_document(&document_with_mineral((30, 30)));
    harness.tap_key(KeyCode::KeyQ);

    harness.drag_cells((30, 30), (34, 28));

    assert_eq!(harness.doc().doc.resources[0].cell, (34, 28));
    assert_eq!(harness.history().undo_len(), 1);
    assert!(harness.doc().dirty);
    let selected = harness
        .app
        .world()
        .resource::<EditorToolState>()
        .selected_entity_id;
    assert_eq!(selected, Some(MapEntityId(10)));

    harness.send_key(KeyCode::ControlLeft, ButtonState::Pressed);
    harness.step(1);
    harness.tap_key(KeyCode::KeyZ);
    assert_eq!(harness.doc().doc.resources[0].cell, (30, 30));
}

#[test]
fn select_tool_click_selects_without_moving() {
    let mut harness = EditorHarness::with_document(&document_with_mineral((30, 30)));
    harness.tap_key(KeyCode::KeyQ);

    harness.drag_cells((30, 30), (30, 30));

    assert_eq!(harness.doc().doc.resources[0].cell, (30, 30));
    assert_eq!(harness.history().undo_len(), 0);
    assert!(!harness.doc().dirty);
    let selected = harness
        .app
        .world()
        .resource::<EditorToolState>()
        .selected_entity_id;
    assert_eq!(selected, Some(MapEntityId(10)));
}

#[test]
fn select_tool_drag_keeps_the_grab_offset_of_a_multi_cell_base() {
    let mut harness = EditorHarness::new();
    harness.tap_key(KeyCode::KeyQ);
    assert_eq!(harness.doc().doc.start_locations[0].cell, (8, 8));

    // Grabbed by its center, cell (10, 10), and dropped two cells over.
    harness.drag_cells((10, 10), (12, 11));

    assert_eq!(harness.doc().doc.start_locations[0].cell, (10, 9));
}

#[test]
fn select_tool_drag_released_over_a_panel_cancels_the_move() {
    let mut harness = EditorHarness::with_document(&document_with_mineral((30, 30)));
    harness.tap_key(KeyCode::KeyQ);
    let start = harness.cell_on_screen(30, 30);
    harness.move_cursor(start);
    harness.press_left();
    harness.move_cursor(Vec2::new(100.0, 450.0));
    harness.step(1);
    harness.release_left();
    harness.step(1);

    assert_eq!(harness.doc().doc.resources[0].cell, (30, 30));
    assert_eq!(harness.history().undo_len(), 0);

    // The cancelled drag must not revive on the next click elsewhere.
    harness.drag_cells((40, 40), (41, 41));
    assert_eq!(harness.doc().doc.resources[0].cell, (30, 30));
}

/// Phase 7 exit criterion: edit, save, playtest, return and reopen.
#[test]
fn edit_save_playtest_return_and_reopen_round_trip() {
    let mut harness = EditorHarness::new();

    // Edit with a real brush stroke.
    harness.begin_stroke_at(32, 32);
    harness.release_left();
    harness.step(1);
    assert!(harness.doc().dirty);
    assert_eq!(harness.history().undo_len(), 1);
    let edited = harness.doc().doc.clone();

    // Save with the real shortcut.
    harness.send_key(KeyCode::ControlLeft, ButtonState::Pressed);
    harness.step(1);
    harness.tap_key(KeyCode::KeyS);
    harness.send_key(KeyCode::ControlLeft, ButtonState::Released);
    harness.step(1);
    assert!(!harness.doc().dirty, "Ctrl+S must save");

    // Playtest: the edited map must really run.
    harness.tap_key(KeyCode::F5);
    harness.step(20);
    assert_eq!(harness.phase(), EditorPhase::Playtesting);
    assert!(harness.bubble_count() > 0);

    // Return: the document and undo history are as they were.
    harness.tap_key(KeyCode::Escape);
    harness.step(2);
    assert_eq!(harness.phase(), EditorPhase::Editing);
    assert_eq!(harness.doc().doc, edited);
    assert_eq!(harness.history().undo_len(), 1);
    assert!(!harness.doc().dirty);

    // Reopen from disk through the real load path.
    let path = harness.path();
    harness.request_open(path);
    assert_eq!(harness.doc().doc, edited, "the edit must survive on disk");
    assert_eq!(harness.terrain_level(32, 32), 2, "and reach the renderer");
    assert_eq!(harness.history().undo_len(), 0);

    // The reopened map must still playtest.
    harness.tap_key(KeyCode::F5);
    harness.step(10);
    assert_eq!(harness.phase(), EditorPhase::Playtesting);
    assert!(harness.bubble_count() > 0);
}

#[test]
fn tool_and_overlay_shortcuts_reach_their_state() {
    let mut harness = EditorHarness::new();

    for (key, tool) in [
        (KeyCode::KeyX, EditorTool::Box),
        (KeyCode::KeyR, EditorTool::Ramp),
        (KeyCode::KeyE, EditorTool::Entity),
        (KeyCode::KeyQ, EditorTool::Select),
        (KeyCode::KeyT, EditorTool::Eraser),
        (KeyCode::KeyB, EditorTool::Brush),
    ] {
        harness.tap_key(key);
        assert_eq!(
            harness
                .app
                .world()
                .resource::<EditorToolState>()
                .current_tool,
            tool,
            "{key:?}"
        );
    }

    let flags = |harness: &EditorHarness| {
        harness
            .app
            .world()
            .resource::<TerrainParamsState>()
            .overlay_flags
    };
    for (key, flag) in [
        (KeyCode::KeyG, OVERLAY_FLAG_GRID),
        (KeyCode::KeyP, OVERLAY_FLAG_PATHABILITY),
        (KeyCode::KeyU, OVERLAY_FLAG_BUILDABILITY),
    ] {
        let before = flags(&harness) & flag;
        harness.tap_key(key);
        assert_ne!(flags(&harness) & flag, before, "{key:?}");
    }

    harness.tap_key(KeyCode::KeyV);
    assert!(
        harness.app.world().resource::<LosPreviewState>().enabled,
        "V toggles LOS"
    );

    let radius = harness
        .app
        .world()
        .resource::<EditorToolState>()
        .brush_radius;
    harness.tap_key(KeyCode::BracketRight);
    assert_eq!(
        harness
            .app
            .world()
            .resource::<EditorToolState>()
            .brush_radius,
        radius + 1
    );
}

#[test]
fn playtest_click_selects_a_unit_and_right_click_commands_it() {
    let mut harness = EditorHarness::new();
    harness.tap_key(KeyCode::F5);
    harness.step(6);
    assert_eq!(harness.phase(), EditorPhase::Playtesting);

    // Frame player 0's base, then find a mobile unit of theirs on screen.
    let world = harness.app.world_mut();
    let mut camera = world.query::<&mut RtsCamera>();
    camera.single_mut(world).target = Vec2::new(12.0, 12.0);
    harness.step(3);

    let world = harness.app.world_mut();
    let (camera, camera_transform) = world
        .query_filtered::<(&Camera, &GlobalTransform), With<RtsCamera>>()
        .single(world);
    let (camera, camera_transform) = (camera.clone(), *camera_transform);
    let mut bubbles =
        world.query_filtered::<(&GlobalTransform, &BubbleInstance), With<LiveBattleBubble>>();
    let (unit, unit_position) = bubbles
        .iter(world)
        .filter(|(_, bubble)| {
            bubble.0.owner == PlayerId(0) && matches!(bubble.0.shape, ShapeKind::UnitSphere)
        })
        .find_map(|(transform, bubble)| {
            camera
                .world_to_viewport(&camera_transform, transform.translation())
                .ok()
                .map(|position| (bubble.0.entity_id, position))
        })
        .expect("player 0 must have a mobile unit in view");

    // Left click selects it, and the highlight survives snapshot republication.
    harness.move_cursor(unit_position);
    harness.press_left();
    harness.release_left();
    harness.step(3);
    let selected = harness
        .app
        .world()
        .resource::<BattleSelection>()
        .ids()
        .to_vec();
    assert_eq!(selected, vec![unit], "the clicked unit must be selected");
    let world = harness.app.world_mut();
    let mut drawn = world.query::<(&BubbleInstance, &MeshMaterial3d<BubbleDomeMaterial>)>();
    let (instance, material) = drawn
        .iter(world)
        .find(|(bubble, _)| bubble.0.entity_id == unit)
        .expect("selected unit is drawn");
    assert!(instance.0.selected);
    let material = world
        .resource::<Assets<BubbleDomeMaterial>>()
        .get(&material.0)
        .expect("bubble material exists");
    assert_eq!(
        material.uniform.selected, 1,
        "the shader must show the selection"
    );

    // Right click issues a real move through the production command queue.
    let outcomes_before = harness
        .app
        .world()
        .resource::<BattleSession>()
        .outcome_count;
    let target = harness.cell_on_screen(20, 20);
    harness.move_cursor(target);
    harness.send_mouse(MouseButton::Right, ButtonState::Pressed);
    harness.send_mouse(MouseButton::Right, ButtonState::Released);
    harness.step(4);

    let session = harness.app.world().resource::<BattleSession>();
    assert!(
        session.outcome_count > outcomes_before,
        "the order must reach the simulation; queue status: {:?}",
        session.commands.status()
    );
    let lab = harness.app.world().resource::<LabState>();
    assert_eq!(lab.battle_failure, None);
}
