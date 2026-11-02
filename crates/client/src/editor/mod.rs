//! Pure-logic map editor core subsystem and interactive Bevy application.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §6, §7 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this module provides the pure-logic domain model, command history, stroke recording,
//! symmetry transformations, holographic markers, egui UI, and live playtest execution for the
//! Space Scuffle map editor.
//!
//! # Invariants Maintained
//! 1. **Pure Logic & Determinism:** Editor core commands and history maintain bit-for-bit determinism
//!    and lossless undo/redo.
//! 2. **Lossless Undo/Redo:** Every editing operation is encapsulated in an [`EditCommand`],
//!    guaranteeing exact restoration on undo and redo.
//! 3. **Live Document Feedback:** Drag strokes modify the [`EditorDocument`] immediately
//!    for real-time inspection while preventing redundant per-cell delta allocations.
//! 4. **Safe Symmetry Invariants:** Geometric mirrors and 180° rotations are strictly self-inverse
//!    and preserve terrain ramp orientations and player slot partner rules.
//! 5. **Clean Playtest Boundaries:** Playtesting initializes a dedicated deterministic [`SimWorld`](sim_core::world::SimWorld)
//!    and cleanly despawns all combat bubbles and visual FX when returning to editing.

pub mod app;
pub mod camera;
pub mod commands;
pub mod document;
pub mod document_load;
pub mod file_browser;
pub mod los_preview;
pub mod markers;
pub mod playtest;
pub mod startup;
pub mod stroke;
pub mod symmetry;
pub mod tools;
pub mod ui;
pub mod verification;

pub use app::{
    editor_window_close_system, editor_window_plugin, MapEditorPlugin, MapsDirectory,
    ThemeCatalogResource,
};
pub use camera::{editor_camera_system, focus_cell, EditorCameraFocus, CAMERA_HEIGHT_EASE_PER_SEC};
pub use commands::{
    CellDelta, ChangeSet, EditCommand, History, MapEntity, MAX_UNDO_CELL_DELTAS, MAX_UNDO_COMMANDS,
};
pub use document::EditorDocument;
pub use document_load::{
    apply_document_load_system, DocumentLoadError, EditorTerrainRoot, EditorTerrainSpawner,
    LoadDocumentRequest,
};
pub use file_browser::{FileBrowserAction, FileBrowserMode, FileBrowserState};
pub use los_preview::{
    resolve_los_preview_sight_radius, update_los_preview_system, LosPreviewState,
    StartLoadoutCatalogResource,
};
pub use markers::{
    reconcile_marker_entities, setup_marker_assets, sync_marker_entities_system,
    update_marker_billboards_system, MapEntityVisualCatalog, MapMarkerComponent, MarkerAssets,
    MarkerEntities, MarkerSpriteEntry, StartLocationVisualEntry,
};
pub use playtest::{
    autosave_path, initialize_playtest_world, playtest_exit_system, playtest_hud_system,
    playtest_input_system, playtest_metadata, playtest_players, sync_playtest_status_system,
    teardown_playtest_system, EditorPhase, EditorRulesResource, PlaytestEntry, PlaytestEntryError,
    PlaytestState, AUTOSAVE_SUFFIX, PLAYTEST_SEED,
};
pub use startup::{EditorStartup, EditorStartupError, InitialDocument};
pub use stroke::StrokeRecorder;
pub use symmetry::SymmetryMode;
pub use tools::{
    box_fill_command, brush_footprint, check_ramp_placement, delete_entity_command,
    erase_cell_command, erase_entities_command, infer_ramp, move_entity_command, paint_brush_stamp,
    pick_entity, place_entity_command, place_ramp_command, resolve_start_footprint, AuthorableFlag,
    BrushShape, EntityToolKind, PaintOp, RampPlacement, RampPlacementError, StartFootprintError,
    DEFAULT_GAS_AMOUNT, DEFAULT_MINERAL_AMOUNT, DEFAULT_WATCHTOWER_CAPTURE_RADIUS_TENTHS,
    DEFAULT_WATCHTOWER_REVEAL_RADIUS_TENTHS, MAX_BRUSH_RADIUS,
};
pub use ui::{
    apply_unsaved_choice, editor_ui_system, EditorTool, EditorToolState, EntityDrag,
    NewMapModalState, PendingUnsavedAction, RampToolMode, UnsavedChangesModalState, UnsavedChoice,
    UnsavedOutcome, VALIDATION_DEBOUNCE_SECS,
};
pub use verification::{
    check_render_pipelines, get_verify_poses, update_playtest_verification_system,
    update_render_verification_system, CaptureStep, PipelineSync, VerifyOutputDir,
    VerifyOutputError, VerifyPose, VerifyRenderState, EXIT_CODE_VERIFICATION_FAILURE,
    PLAYTEST_BASELINE_POSE, PLAYTEST_POSE, POSE_SETTLING_FRAMES, VERIFY_WARMUP_TIMEOUT_FRAMES,
};
