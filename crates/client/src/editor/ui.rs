//! Full-featured egui UI for the Space Scuffle map editor.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.3 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this module provides the interactive user interface: top menu bar, left toolbox, right
//! inspector/validation panel with cell jumping, bottom status bar, and modal confirmation dialogs.
//! [`editor_ui_system`] orchestrates one frame; `panels`, `dialogs` and `viewport` hold the parts.
//!
//! # Invariants Maintained
//! 1. **Zero Per-Frame Allocations:** Scratch buffers for brush coordinates and cell footprints
//!    are reused across render frames.
//! 2. **Shortcuts & Pointer Decoupling:** Global shortcuts and viewport clicks are strictly
//!    suppressed while egui windows or text inputs capture keyboard or pointer focus. Ending a
//!    drag is not: a stroke finishes wherever its button is released.
//! 3. **Stroke Integrity:** Undo, redo and playtest entry are blocked while a stroke is live,
//!    because its recorded before-states describe the document as the stroke found it.
//! 4. **Validation Debounce:** Live validation is debounced by [`VALIDATION_DEBOUNCE_SECS`] (0.4s)
//!    following edits, but runs synchronously before document saves and playtest transitions.
//! 5. **Unsaved Edits Guard:** Closing, opening, or creating maps with unsaved changes prompts
//!    an explicit Save / Discard / Cancel modal. `dirty` stays set until the document is really
//!    saved or replaced, and the guarded action resumes only after its edits are resolved.

use std::path::Path;

use bevy::app::AppExit;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::EguiContexts;
use sim_map::grid::{CliffLevel, RampDir};
use sim_map::map_format::{MapDocument, MapEntityId};

use crate::editor::app::{MapsDirectory, ThemeCatalogResource};
use crate::editor::camera::EditorCameraFocus;
use crate::editor::commands::{ChangeSet, History, MapEntity};
use crate::editor::document::EditorDocument;
use crate::editor::document_load::LoadDocumentRequest;
use crate::editor::file_browser::{FileBrowserMode, FileBrowserState};
use crate::editor::los_preview::{LosPreviewState, StartLoadoutCatalogResource};
use crate::editor::playtest::PlaytestEntry;
use crate::editor::stroke::StrokeRecorder;
use crate::editor::symmetry::SymmetryMode;
use crate::editor::tools::{
    delete_entity_command, BrushShape, EntityToolKind, PaintOp, MAX_BRUSH_RADIUS,
};
use crate::terrain::{
    update_overlay_image, TerrainFieldImages, TerrainParamsState, TerrainState,
    OVERLAY_FLAG_BUILDABILITY, OVERLAY_FLAG_GRID, OVERLAY_FLAG_PATHABILITY,
};

mod dialogs;
mod panels;
mod viewport;

pub use dialogs::{apply_unsaved_choice, UnsavedChoice, UnsavedOutcome};

/// Debounce period in seconds before re-running automatic map validation after an edit.
pub const VALIDATION_DEBOUNCE_SECS: f64 = 0.4;

/// Active authoring tool selected in the toolbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorTool {
    /// Brush tool for freehand painting with radius and shape.
    #[default]
    Brush,
    /// Box fill tool for rectangular regions.
    Box,
    /// Ramp placement tool with auto-inference or cardinal direction.
    Ramp,
    /// Entity placement tool for minerals, gas, towers, and start bases.
    Entity,
    /// Selection tool for inspecting and editing entity properties.
    Select,
    /// Eraser tool for removing entities or clearing painted flags.
    Eraser,
}

/// Ramp orientation mode in the toolbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RampToolMode {
    /// Auto-infer ascending ramp direction based on neighbouring cliff landings.
    #[default]
    Auto,
    /// Explicit cardinal ramp direction override.
    Manual(RampDir),
}

/// Pending action guarded by the unsaved changes modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingUnsavedAction {
    /// Create a new blank map.
    NewMap,
    /// Open an existing map file.
    OpenFile,
    /// Exit the application.
    Quit,
}

/// Entity being dragged by the Select tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityDrag {
    /// The entity under the pointer when the drag began.
    pub id: MapEntityId,
    /// Cell the pointer grabbed, which may be any cell of a multi-cell footprint.
    pub grab_cell: (u32, u32),
}

/// Modal state for creating a new map.
#[derive(Resource, Debug, Clone)]
pub struct NewMapModalState {
    /// Whether the new map dialog is currently open.
    pub is_open: bool,
    /// Map width in simulation cells.
    pub width: u32,
    /// Map height in simulation cells.
    pub height: u32,
    /// Title of the map.
    pub title: String,
    /// Terrain theme identifier slug.
    pub theme: String,
    /// Error banner if parameters violate bounds.
    pub error_banner: Option<String>,
}

impl Default for NewMapModalState {
    fn default() -> Self {
        Self {
            is_open: false,
            width: 64,
            height: 64,
            title: "New Map".to_string(),
            theme: "orbital_foundry".to_string(),
            error_banner: None,
        }
    }
}

/// Modal state guarding destructive actions when unsaved changes exist.
#[derive(Resource, Debug, Default, Clone)]
pub struct UnsavedChangesModalState {
    /// Whether the unsaved changes warning dialog is open.
    pub is_open: bool,
    /// Action that triggered the prompt.
    pub pending_action: Option<PendingUnsavedAction>,
}

/// Authoring tool parameters, brush sizes, and active selection state.
#[derive(Resource, Debug)]
pub struct EditorToolState {
    /// Currently active authoring tool.
    pub current_tool: EditorTool,
    /// Active paint operation.
    pub paint_op: PaintOp,
    /// Active symmetry reflection mode.
    pub symmetry: SymmetryMode,
    /// Brush footprint shape profile.
    pub brush_shape: BrushShape,
    /// Brush footprint radius in cells.
    pub brush_radius: u8,
    /// Ramp orientation placement rule.
    pub ramp_mode: RampToolMode,
    /// Entity kind to spawn when using the Entity tool.
    pub entity_kind: EntityToolKind,
    /// Player slot index for start location authoring.
    pub player_slot: u8,
    /// Currently selected entity ID, if any.
    pub selected_entity_id: Option<MapEntityId>,
    /// Simulation cell coordinate under the mouse cursor.
    pub hovered_cell: Option<(u32, u32)>,
    /// Last error or status message displayed on the bottom bar.
    pub last_status_message: Option<String>,
    /// Anchor cell for the Box fill tool drag operation.
    pub box_start_cell: Option<(u32, u32)>,
    /// Anchor cell for the Ramp tool drag operation.
    pub ramp_start_cell: Option<(u32, u32)>,
    /// Active entity move started by the Select tool.
    pub entity_drag: Option<EntityDrag>,
    /// Whether a brush stroke drag is currently active.
    pub is_painting: bool,
    /// Live stroke recorder for freehand brush painting.
    pub stroke_recorder: StrokeRecorder,
    /// Pre-allocated reusable buffer for brush coordinate footprints.
    pub scratch_coords: Vec<(u32, u32)>,
    /// Selected playtest loadout slug per player slot.
    pub player_loadouts: [String; 8],
}

impl EditorToolState {
    /// Creates a new editor tool state for a map of dimensions `width × height`.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            current_tool: EditorTool::Brush,
            paint_op: PaintOp::Level(CliffLevel(1)),
            symmetry: SymmetryMode::None,
            brush_shape: BrushShape::Circle,
            brush_radius: 1,
            ramp_mode: RampToolMode::Auto,
            entity_kind: EntityToolKind::Mineral,
            player_slot: 0,
            selected_entity_id: None,
            hovered_cell: None,
            last_status_message: None,
            box_start_cell: None,
            ramp_start_cell: None,
            entity_drag: None,
            is_painting: false,
            stroke_recorder: StrokeRecorder::new(width, height),
            scratch_coords: Vec::with_capacity(256),
            player_loadouts: std::array::from_fn(|_| "standard_automaton".to_string()),
        }
    }
}

impl Default for EditorToolState {
    fn default() -> Self {
        Self::new(64, 64)
    }
}

/// Contextual system parameters for the editor UI (time, input, windows, cameras, events).
#[derive(SystemParam)]
pub struct EditorUiContext<'w, 's> {
    /// Global simulation and app clock time.
    pub time: Res<'w, Time>,
    /// Immediate-mode egui contexts for window rendering.
    pub contexts: EguiContexts<'w, 's>,
    /// Keyboard input state.
    pub keyboard: Res<'w, ButtonInput<KeyCode>>,
    /// Mouse button input state.
    pub mouse_buttons: Res<'w, ButtonInput<MouseButton>>,
    /// Primary application window query.
    pub windows: Query<'w, 's, &'static Window>,
    /// Camera query providing raycasting and viewport projection.
    pub camera_query: Query<
        'w,
        's,
        (
            &'static Camera,
            &'static GlobalTransform,
            &'static mut crate::camera::RtsCamera,
        ),
    >,
    /// Writer for exiting the application gracefully.
    pub exit_events: EventWriter<'w, AppExit>,
    /// Writer for camera jump and focus events.
    pub focus_events: EventWriter<'w, EditorCameraFocus>,
    /// Writer asking to replace the active document.
    pub load_requests: EventWriter<'w, LoadDocumentRequest>,
}

/// Document, tool, history, modal, and playtest state for the editor UI.
#[derive(SystemParam)]
pub struct EditorUiState<'w> {
    /// Active map document and validation report.
    pub doc: ResMut<'w, EditorDocument>,
    /// Undo/redo history stack.
    pub history: ResMut<'w, History>,
    /// Active tool, brush, and symmetry parameters.
    pub tool_state: ResMut<'w, EditorToolState>,
    /// Modal file browser state for open/save operations.
    pub file_browser: ResMut<'w, FileBrowserState>,
    /// Modal dialog state for creating new maps.
    pub new_map_modal: ResMut<'w, NewMapModalState>,
    /// Guard modal for handling unsaved edits on file/quit actions.
    pub unsaved_modal: ResMut<'w, UnsavedChangesModalState>,
    /// Dynamic line-of-sight preview calculations and state.
    pub los_state: ResMut<'w, LosPreviewState>,
    /// Validation, preparation and commit of a playtest session.
    pub playtest: PlaytestEntry<'w>,
}

/// Terrain rendering, field images, and catalog references for the editor UI.
#[derive(SystemParam)]
pub struct EditorUiTerrain<'w> {
    /// Authoritative terrain cell and cliff geometry state.
    pub terrain_state: ResMut<'w, TerrainState>,
    /// GPU uniform parameters for terrain shader rendering.
    pub terrain_params: ResMut<'w, TerrainParamsState>,
    /// Texture image handles for terrain heightfield and overlays.
    pub field_images: Option<Res<'w, TerrainFieldImages>>,
    /// Image asset store for mutating overlay textures.
    pub images: ResMut<'w, Assets<Image>>,
    /// Catalog of visual themes (canyon, orbital foundry, etc.).
    pub theme_catalog: Option<Res<'w, ThemeCatalogResource>>,
    /// Starting army loadout compositions.
    pub loadouts: Option<Res<'w, StartLoadoutCatalogResource>>,
    /// Dynamic unit catalog definitions.
    pub unit_catalog: Option<Res<'w, crate::CatalogResource>>,
}

/// Primary editor UI and interactive viewport tool system.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn editor_ui_system(
    mut input: EditorUiContext<'_, '_>,
    mut state: EditorUiState<'_>,
    mut terrain: EditorUiTerrain<'_>,
    maps_directory: Res<'_, MapsDirectory>,
) {
    let now_secs = input.time.elapsed_secs_f64();
    let maps_dir = maps_directory.as_path();

    refresh_validation(&mut state, &mut terrain, now_secs);

    let Some(ctx) = input.contexts.try_ctx_mut() else {
        return;
    };
    // A cheap reference-counted handle that ends the borrow of `input`.
    let ctx = ctx.clone();

    if !ctx.wants_keyboard_input() {
        handle_global_shortcuts(&mut input, &mut state, &mut terrain, maps_dir, now_secs);
    }

    panels::render_menu_bar(
        &ctx,
        &mut input,
        &mut state,
        &mut terrain,
        maps_dir,
        now_secs,
    );
    panels::render_toolbox(&ctx, &mut state, &terrain);
    panels::render_inspector(&ctx, &mut input, &mut state, &terrain, now_secs);
    panels::render_status_bar(&ctx, &state);
    dialogs::render_dialogs(&ctx, &mut input, &mut state, &terrain, maps_dir);
    viewport::update_viewport(&ctx, &input, &mut state, &mut terrain, now_secs);
}

/// Re-runs validation once edits have settled for [`VALIDATION_DEBOUNCE_SECS`].
fn refresh_validation(
    state: &mut EditorUiState<'_>,
    terrain: &mut EditorUiTerrain<'_>,
    now_secs: f64,
) {
    let EditorUiState { doc, los_state, .. } = state;
    let EditorUiTerrain {
        terrain_state,
        field_images,
        images,
        ..
    } = terrain;
    if !doc.is_validation_current()
        && (now_secs - doc.last_edit_at_secs >= VALIDATION_DEBOUNCE_SECS)
    {
        doc.revalidate();
        if let Some(fi) = field_images {
            if let Some(img) = images.get_mut(&fi.overlay) {
                update_overlay_image(
                    &terrain_state.map,
                    Some(&doc.validation),
                    if los_state.enabled {
                        Some(&los_state.vision_map)
                    } else {
                        None
                    },
                    img,
                    None,
                );
            }
        }
    }
}

/// Handles keyboard shortcuts; only called while no text field owns the keyboard.
fn handle_global_shortcuts(
    input: &mut EditorUiContext<'_, '_>,
    state: &mut EditorUiState<'_>,
    terrain: &mut EditorUiTerrain<'_>,
    maps_dir: &Path,
    now_secs: f64,
) {
    let EditorUiContext {
        keyboard,
        focus_events,
        ..
    } = input;
    let EditorUiState {
        doc,
        history,
        tool_state,
        file_browser,
        los_state,
        playtest,
        ..
    } = state;
    let EditorUiTerrain {
        terrain_state,
        terrain_params,
        ..
    } = terrain;

    let ctrl = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    if ctrl && keyboard.just_pressed(KeyCode::KeyS) {
        handle_save_action(doc, file_browser, maps_dir, tool_state);
    } else if ctrl && !shift && keyboard.just_pressed(KeyCode::KeyZ) {
        if !tool_state.is_painting {
            let mut cs = ChangeSet::default();
            if history.undo(&mut doc.doc, &mut cs) {
                doc.apply_changeset(&cs, now_secs);
                doc.sync_terrain(&cs, terrain_state);
                tool_state.last_status_message = Some("Undo edit".into());
            }
        }
    } else if ctrl
        && (keyboard.just_pressed(KeyCode::KeyY) || (shift && keyboard.just_pressed(KeyCode::KeyZ)))
    {
        if !tool_state.is_painting {
            let mut cs = ChangeSet::default();
            if history.redo(&mut doc.doc, &mut cs) {
                doc.apply_changeset(&cs, now_secs);
                doc.sync_terrain(&cs, terrain_state);
                tool_state.last_status_message = Some("Redo edit".into());
            }
        }
    } else if keyboard.just_pressed(KeyCode::F5) && !tool_state.is_painting {
        handle_playtest_entry(doc, playtest, tool_state, maps_dir, focus_events);
    } else if keyboard.just_pressed(KeyCode::Delete) {
        if let Some(ent_id) = tool_state.selected_entity_id {
            let entity = find_entity_by_id(&doc.doc, ent_id);
            if let Some(ent) = entity {
                let cmd = delete_entity_command(&ent);
                let mut cs = ChangeSet::default();
                history.execute(cmd, &mut doc.doc, &mut cs);
                doc.apply_changeset(&cs, now_secs);
                tool_state.selected_entity_id = None;
                tool_state.last_status_message = Some("Deleted entity".into());
            }
        }
    } else {
        handle_tool_shortcuts(keyboard, tool_state, terrain_params, los_state);
    }
}

/// Handles the unmodified keys that switch tools, brushes and overlays.
fn handle_tool_shortcuts(
    keyboard: &ButtonInput<KeyCode>,
    tool_state: &mut EditorToolState,
    terrain_params: &mut ResMut<'_, TerrainParamsState>,
    los_state: &mut LosPreviewState,
) {
    if keyboard.just_pressed(KeyCode::KeyB) {
        tool_state.current_tool = EditorTool::Brush;
    } else if keyboard.just_pressed(KeyCode::KeyX) {
        tool_state.current_tool = EditorTool::Box;
    } else if keyboard.just_pressed(KeyCode::KeyR) {
        tool_state.current_tool = EditorTool::Ramp;
    } else if keyboard.just_pressed(KeyCode::KeyE) {
        tool_state.current_tool = EditorTool::Entity;
    } else if keyboard.just_pressed(KeyCode::KeyQ) {
        tool_state.current_tool = EditorTool::Select;
    } else if keyboard.just_pressed(KeyCode::KeyT) {
        tool_state.current_tool = EditorTool::Eraser;
    } else if keyboard.just_pressed(KeyCode::BracketLeft) {
        tool_state.brush_radius = tool_state.brush_radius.saturating_sub(1).max(1);
    } else if keyboard.just_pressed(KeyCode::BracketRight) {
        tool_state.brush_radius = (tool_state.brush_radius + 1).min(MAX_BRUSH_RADIUS);
    } else if keyboard.just_pressed(KeyCode::Digit1) {
        tool_state.paint_op = PaintOp::Level(CliffLevel(0));
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        tool_state.paint_op = PaintOp::Level(CliffLevel(1));
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        tool_state.paint_op = PaintOp::Level(CliffLevel(2));
    } else if keyboard.just_pressed(KeyCode::Digit4) {
        tool_state.paint_op = PaintOp::Level(CliffLevel(3));
    } else if keyboard.just_pressed(KeyCode::KeyG) {
        terrain_params.overlay_flags ^= OVERLAY_FLAG_GRID;
    } else if keyboard.just_pressed(KeyCode::KeyP) {
        terrain_params.overlay_flags ^= OVERLAY_FLAG_PATHABILITY;
    } else if keyboard.just_pressed(KeyCode::KeyU) {
        terrain_params.overlay_flags ^= OVERLAY_FLAG_BUILDABILITY;
    } else if keyboard.just_pressed(KeyCode::KeyV) {
        los_state.enabled = !los_state.enabled;
    }
}

fn find_entity_by_id(doc: &MapDocument, id: MapEntityId) -> Option<MapEntity> {
    if let Some(r) = doc.resources.iter().find(|r| r.id == id) {
        Some(MapEntity::Resource(r.clone()))
    } else if let Some(w) = doc.watchtowers.iter().find(|w| w.id == id) {
        Some(MapEntity::Watchtower(w.clone()))
    } else {
        doc.start_locations
            .iter()
            .find(|s| s.id == id)
            .map(|s| MapEntity::StartLocation(s.clone()))
    }
}

/// Result of attempting to save the current document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveOutcome {
    /// The document is on disk and no longer dirty.
    Saved,
    /// The document has no path, so the Save As dialog was opened.
    AwaitingPath,
    /// Writing failed; the reason is on the status bar.
    Failed,
}

fn handle_save_action(
    doc: &mut EditorDocument,
    file_browser: &mut FileBrowserState,
    maps_dir: &Path,
    tool_state: &mut EditorToolState,
) -> SaveOutcome {
    if let Some(path) = doc.path.clone() {
        doc.revalidate();
        let map_file = doc.doc.encode();
        match map_file.save_atomic(&path) {
            Ok(()) => {
                doc.mark_saved(path.clone());
                tool_state.last_status_message = Some(format!("Saved {}", path.display()));
                SaveOutcome::Saved
            }
            Err(e) => {
                tool_state.last_status_message = Some(format!("Save error: {e}"));
                SaveOutcome::Failed
            }
        }
    } else {
        file_browser.open(FileBrowserMode::Save, maps_dir, Some("untitled.scmap.ron"));
        SaveOutcome::AwaitingPath
    }
}

fn handle_playtest_entry(
    doc: &mut EditorDocument,
    playtest: &mut PlaytestEntry<'_>,
    tool_state: &mut EditorToolState,
    maps_dir: &Path,
    focus_events: &mut EventWriter<'_, EditorCameraFocus>,
) {
    match playtest.enter(doc, &tool_state.player_loadouts, maps_dir) {
        Ok(autosaved) => {
            if let Some(path) = autosaved {
                tool_state.last_status_message = Some(format!("Autosaved {}", path.display()));
            }
        }
        Err(error) => {
            if let Some(first_error) = doc.validation.errors.first() {
                focus_events.send(EditorCameraFocus {
                    cell: first_error.location().min,
                });
            }
            tool_state.last_status_message = Some(format!("Cannot playtest: {error}"));
        }
    }
}
