//! Full-featured egui UI for the Space Scuffle map editor.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.3 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this module provides the interactive user interface: top menu bar, left toolbox, right
//! inspector/validation panel with cell jumping, bottom status bar, and modal confirmation dialogs.
//!
//! # Invariants Maintained
//! 1. **Zero Per-Frame Allocations:** Scratch buffers for brush coordinates and cell footprints
//!    are reused across render frames.
//! 2. **Shortcuts & Pointer Decoupling:** Global shortcuts and viewport clicks are strictly
//!    suppressed while egui windows or text inputs capture keyboard or pointer focus.
//! 3. **Validation Debounce:** Live validation is debounced by [`VALIDATION_DEBOUNCE_SECS`] (0.4s)
//!    following edits, but runs synchronously before document saves and playtest transitions.
//! 4. **Unsaved Edits Guard:** Closing, opening, or creating maps with unsaved changes prompts
//!    an explicit Save / Discard / Cancel modal. `dirty` stays set until the document is really
//!    saved or replaced, and the guarded action resumes only after its edits are resolved.

use std::path::Path;

use bevy::app::AppExit;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use sim_map::grid::{CellFlags, CliffLevel, RampDir, TerrainKind};
use sim_map::map_format::{
    MapDocument, MapEntityId, MapMetadata, MAX_MAP_DIMENSION, MIN_MAP_DIMENSION,
};
use sim_map::validation::CellRect;

use crate::editor::app::ThemeCatalogResource;
use crate::editor::camera::EditorCameraFocus;
use crate::editor::commands::{ChangeSet, EditCommand, History, MapEntity};
use crate::editor::document::EditorDocument;
use crate::editor::file_browser::{FileBrowserAction, FileBrowserMode, FileBrowserState};
use crate::editor::los_preview::{LosPreviewState, StartLoadoutCatalogResource};
use crate::editor::playtest::PlaytestEntry;
use crate::editor::stroke::StrokeRecorder;
use crate::editor::symmetry::SymmetryMode;
use crate::editor::tools::{
    box_fill_command, check_ramp_placement, delete_entity_command, erase_entities_command,
    infer_ramp, paint_brush_stamp, pick_entity, place_entity_command, place_ramp_command,
    resolve_start_footprint, AuthorableFlag, BrushShape, EntityToolKind, PaintOp, MAX_BRUSH_RADIUS,
};
use crate::terrain::{
    pick_terrain, update_overlay_image, TerrainFieldImages, TerrainParamsState, TerrainState,
    OVERLAY_FLAG_BRUSH, OVERLAY_FLAG_BUILDABILITY, OVERLAY_FLAG_GRID, OVERLAY_FLAG_PATHABILITY,
    OVERLAY_FLAG_VALIDATION,
};

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
pub fn editor_ui_system(
    ctx: EditorUiContext<'_, '_>,
    state: EditorUiState<'_>,
    terrain: EditorUiTerrain<'_>,
) {
    let EditorUiContext {
        time,
        mut contexts,
        keyboard,
        mouse_buttons,
        windows,
        camera_query,
        mut exit_events,
        mut focus_events,
    } = ctx;

    let EditorUiState {
        mut doc,
        mut history,
        mut tool_state,
        mut file_browser,
        mut new_map_modal,
        mut unsaved_modal,
        mut los_state,
        mut playtest,
    } = state;

    let EditorUiTerrain {
        mut terrain_state,
        mut terrain_params,
        field_images,
        mut images,
        theme_catalog,
        loadouts,
        unit_catalog,
    } = terrain;

    let now_secs = time.elapsed_secs_f64();
    let maps_dir = crate::resolve_assets_dir().join("maps");

    // 1. Debounced Validation Check
    if !doc.is_validation_current()
        && (now_secs - doc.last_edit_at_secs >= VALIDATION_DEBOUNCE_SECS)
    {
        doc.revalidate();
        if let Some(ref fi) = field_images {
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

    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };

    let wants_keyboard = ctx.wants_keyboard_input();
    let wants_pointer = ctx.wants_pointer_input() || ctx.is_pointer_over_area();

    // 4. Global Shortcuts (only when keyboard not captured by text fields)
    if !wants_keyboard {
        let ctrl =
            keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
        let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

        if ctrl && keyboard.just_pressed(KeyCode::KeyS) {
            handle_save_action(&mut doc, &mut file_browser, &maps_dir, &mut tool_state);
        } else if ctrl && keyboard.just_pressed(KeyCode::KeyZ) {
            let mut cs = ChangeSet::default();
            if history.undo(&mut doc.doc, &mut cs) {
                doc.apply_changeset(&cs, now_secs);
                doc.sync_terrain(&cs, &mut terrain_state);
                tool_state.last_status_message = Some("Undo edit".into());
            }
        } else if (ctrl && keyboard.just_pressed(KeyCode::KeyY))
            || (ctrl && shift && keyboard.just_pressed(KeyCode::KeyZ))
        {
            let mut cs = ChangeSet::default();
            if history.redo(&mut doc.doc, &mut cs) {
                doc.apply_changeset(&cs, now_secs);
                doc.sync_terrain(&cs, &mut terrain_state);
                tool_state.last_status_message = Some("Redo edit".into());
            }
        } else if keyboard.just_pressed(KeyCode::F5) {
            handle_playtest_entry(
                &mut doc,
                &mut playtest,
                &mut tool_state,
                &maps_dir,
                &mut focus_events,
            );
        } else if keyboard.just_pressed(KeyCode::KeyB) {
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

    // 5. Render Top Menu Bar
    egui::TopBottomPanel::top("top_menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New…").clicked() {
                    ui.close_menu();
                    if doc.dirty {
                        unsaved_modal.is_open = true;
                        unsaved_modal.pending_action = Some(PendingUnsavedAction::NewMap);
                    } else {
                        new_map_modal.is_open = true;
                    }
                }
                if ui.button("Open…").clicked() {
                    ui.close_menu();
                    if doc.dirty {
                        unsaved_modal.is_open = true;
                        unsaved_modal.pending_action = Some(PendingUnsavedAction::OpenFile);
                    } else {
                        file_browser.open(FileBrowserMode::Open, &maps_dir, None);
                    }
                }
                if ui.button("Save (Ctrl+S)").clicked() {
                    ui.close_menu();
                    handle_save_action(&mut doc, &mut file_browser, &maps_dir, &mut tool_state);
                }
                if ui.button("Save As…").clicked() {
                    ui.close_menu();
                    file_browser.open(
                        FileBrowserMode::Save,
                        &maps_dir,
                        doc.path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|f| f.to_str()),
                    );
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.close_menu();
                    if doc.dirty {
                        unsaved_modal.is_open = true;
                        unsaved_modal.pending_action = Some(PendingUnsavedAction::Quit);
                    } else {
                        exit_events.send(AppExit::Success);
                    }
                }
            });

            ui.menu_button("Edit", |ui| {
                let can_undo = history.can_undo();
                let can_redo = history.can_redo();
                if ui
                    .add_enabled(can_undo, egui::Button::new("Undo (Ctrl+Z)"))
                    .clicked()
                {
                    ui.close_menu();
                    let mut cs = ChangeSet::default();
                    if history.undo(&mut doc.doc, &mut cs) {
                        doc.apply_changeset(&cs, now_secs);
                        doc.sync_terrain(&cs, &mut terrain_state);
                    }
                }
                if ui
                    .add_enabled(can_redo, egui::Button::new("Redo (Ctrl+Y)"))
                    .clicked()
                {
                    ui.close_menu();
                    let mut cs = ChangeSet::default();
                    if history.redo(&mut doc.doc, &mut cs) {
                        doc.apply_changeset(&cs, now_secs);
                        doc.sync_terrain(&cs, &mut terrain_state);
                    }
                }
            });

            ui.menu_button("View", |ui| {
                let mut grid = (terrain_params.overlay_flags & OVERLAY_FLAG_GRID) != 0;
                if ui.checkbox(&mut grid, "Grid (G)").changed() {
                    terrain_params.overlay_flags ^= OVERLAY_FLAG_GRID;
                }
                let mut path = (terrain_params.overlay_flags & OVERLAY_FLAG_PATHABILITY) != 0;
                if ui.checkbox(&mut path, "Pathability (P)").changed() {
                    terrain_params.overlay_flags ^= OVERLAY_FLAG_PATHABILITY;
                }
                let mut build = (terrain_params.overlay_flags & OVERLAY_FLAG_BUILDABILITY) != 0;
                if ui.checkbox(&mut build, "Buildability (U)").changed() {
                    terrain_params.overlay_flags ^= OVERLAY_FLAG_BUILDABILITY;
                }
                let mut los = los_state.enabled;
                if ui.checkbox(&mut los, "LOS Preview (V)").changed() {
                    los_state.enabled = los;
                }
                let mut val = (terrain_params.overlay_flags & OVERLAY_FLAG_VALIDATION) != 0;
                if ui.checkbox(&mut val, "Validation Highlights").changed() {
                    terrain_params.overlay_flags ^= OVERLAY_FLAG_VALIDATION;
                }
            });

            ui.menu_button("Symmetry", |ui| {
                for mode in [
                    SymmetryMode::None,
                    SymmetryMode::MirrorX,
                    SymmetryMode::MirrorY,
                    SymmetryMode::Rotate180,
                ] {
                    if ui
                        .selectable_label(tool_state.symmetry == mode, format!("{mode:?}"))
                        .clicked()
                    {
                        tool_state.symmetry = mode;
                        ui.close_menu();
                    }
                }
            });

            if ui.button("Playtest (F5)").clicked() {
                handle_playtest_entry(
                    &mut doc,
                    &mut playtest,
                    &mut tool_state,
                    &maps_dir,
                    &mut focus_events,
                );
            }
        });
    });

    // 6. Render Left Toolbox Panel
    egui::SidePanel::left("left_toolbox")
        .resizable(false)
        .default_width(220.0)
        .show(ctx, |ui| {
            ui.heading("Tools");
            ui.add_space(4.0);

            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut tool_state.current_tool, EditorTool::Brush, "Brush (B)");
                ui.selectable_value(&mut tool_state.current_tool, EditorTool::Box, "Box (X)");
                ui.selectable_value(&mut tool_state.current_tool, EditorTool::Ramp, "Ramp (R)");
                ui.selectable_value(
                    &mut tool_state.current_tool,
                    EditorTool::Entity,
                    "Entity (E)",
                );
                ui.selectable_value(
                    &mut tool_state.current_tool,
                    EditorTool::Select,
                    "Select (Q)",
                );
                ui.selectable_value(
                    &mut tool_state.current_tool,
                    EditorTool::Eraser,
                    "Eraser (T)",
                );
            });

            ui.separator();

            match tool_state.current_tool {
                EditorTool::Brush | EditorTool::Box => {
                    if tool_state.current_tool == EditorTool::Brush {
                        ui.label("Brush Shape:");
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut tool_state.brush_shape,
                                BrushShape::Circle,
                                "Circle",
                            );
                            ui.selectable_value(
                                &mut tool_state.brush_shape,
                                BrushShape::Square,
                                "Square",
                            );
                        });

                        ui.add_space(4.0);
                        ui.label(format!("Radius: {} ([ / ])", tool_state.brush_radius));
                        let mut r = tool_state.brush_radius;
                        if ui
                            .add(egui::Slider::new(&mut r, 1..=MAX_BRUSH_RADIUS))
                            .changed()
                        {
                            tool_state.brush_radius = r;
                        }
                        ui.separator();
                    }

                    ui.label("Paint Mode:");
                    ui.group(|ui| {
                        ui.label("Cliff Level (1–4):");
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Level(CliffLevel(0)),
                                "0",
                            );
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Level(CliffLevel(1)),
                                "1",
                            );
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Level(CliffLevel(2)),
                                "2",
                            );
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Level(CliffLevel(3)),
                                "3",
                            );
                        });

                        ui.add_space(4.0);
                        ui.label("Terrain Surface:");
                        ui.horizontal_wrapped(|ui| {
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Terrain(TerrainKind::Dirt),
                                "Dirt",
                            );
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Terrain(TerrainKind::Grass),
                                "Grass",
                            );
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Terrain(TerrainKind::MetalPlate),
                                "Metal",
                            );
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Terrain(TerrainKind::Abyss),
                                "Abyss",
                            );
                        });

                        ui.add_space(4.0);
                        ui.label("Authorable Flags:");
                        ui.horizontal_wrapped(|ui| {
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Flag {
                                    flag: AuthorableFlag::GroundPathable,
                                    value: true,
                                },
                                "+Path",
                            );
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Flag {
                                    flag: AuthorableFlag::GroundPathable,
                                    value: false,
                                },
                                "-Path",
                            );
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Flag {
                                    flag: AuthorableFlag::Buildable,
                                    value: true,
                                },
                                "+Build",
                            );
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Flag {
                                    flag: AuthorableFlag::Buildable,
                                    value: false,
                                },
                                "-Build",
                            );
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Flag {
                                    flag: AuthorableFlag::LosBlocker,
                                    value: true,
                                },
                                "+LosBlock",
                            );
                            ui.selectable_value(
                                &mut tool_state.paint_op,
                                PaintOp::Flag {
                                    flag: AuthorableFlag::LosBlocker,
                                    value: false,
                                },
                                "-LosBlock",
                            );
                        });
                    });
                }
                EditorTool::Ramp => {
                    ui.label("Ramp Direction:");
                    ui.selectable_value(
                        &mut tool_state.ramp_mode,
                        RampToolMode::Auto,
                        "Auto Infer",
                    );
                    ui.selectable_value(
                        &mut tool_state.ramp_mode,
                        RampToolMode::Manual(RampDir::North),
                        "North",
                    );
                    ui.selectable_value(
                        &mut tool_state.ramp_mode,
                        RampToolMode::Manual(RampDir::South),
                        "South",
                    );
                    ui.selectable_value(
                        &mut tool_state.ramp_mode,
                        RampToolMode::Manual(RampDir::East),
                        "East",
                    );
                    ui.selectable_value(
                        &mut tool_state.ramp_mode,
                        RampToolMode::Manual(RampDir::West),
                        "West",
                    );
                }
                EditorTool::Entity => {
                    ui.label("Entity Kind:");
                    let is_mineral = matches!(tool_state.entity_kind, EntityToolKind::Mineral);
                    if ui.selectable_label(is_mineral, "Mineral Cluster").clicked() {
                        tool_state.entity_kind = EntityToolKind::Mineral;
                    }
                    let is_gas = matches!(tool_state.entity_kind, EntityToolKind::Gas);
                    if ui.selectable_label(is_gas, "Plasma Vent").clicked() {
                        tool_state.entity_kind = EntityToolKind::Gas;
                    }
                    let is_tower = matches!(tool_state.entity_kind, EntityToolKind::Watchtower);
                    if ui.selectable_label(is_tower, "Sensor Tower").clicked() {
                        tool_state.entity_kind = EntityToolKind::Watchtower;
                    }
                    let is_start =
                        matches!(tool_state.entity_kind, EntityToolKind::StartLocation { .. });
                    if ui.selectable_label(is_start, "Start Base").clicked() {
                        let slug = &tool_state.player_loadouts[tool_state.player_slot as usize];
                        let fp = if let (Some(loads), Some(units)) = (&loadouts, &unit_catalog) {
                            resolve_start_footprint(slug, &loads.0, &units.0).unwrap_or((5, 5))
                        } else {
                            (5, 5)
                        };
                        tool_state.entity_kind = EntityToolKind::StartLocation {
                            slot: tool_state.player_slot,
                            footprint: fp,
                        };
                    }

                    if is_start {
                        ui.add_space(4.0);
                        ui.label("Player Slot (0–7):");
                        let mut slot = tool_state.player_slot;
                        if ui.add(egui::Slider::new(&mut slot, 0..=7)).changed() {
                            tool_state.player_slot = slot;
                            let slug = &tool_state.player_loadouts[slot as usize];
                            let fp = if let (Some(loads), Some(units)) = (&loadouts, &unit_catalog)
                            {
                                resolve_start_footprint(slug, &loads.0, &units.0).unwrap_or((5, 5))
                            } else {
                                (5, 5)
                            };
                            tool_state.entity_kind = EntityToolKind::StartLocation {
                                slot,
                                footprint: fp,
                            };
                        }
                    }
                }
                EditorTool::Select => {
                    ui.label("Click any entity in viewport to inspect properties.");
                }
                EditorTool::Eraser => {
                    ui.label("Click cells or entities to erase them.");
                }
            }

            ui.separator();
            ui.label("Playtest Slot Loadouts:");
            if let Some(loads) = &loadouts {
                for slot in 0..2 {
                    ui.horizontal(|ui| {
                        ui.label(format!("P{slot}:"));
                        let current_slug = &mut tool_state.player_loadouts[slot];
                        egui::ComboBox::from_id_salt(format!("loadout_slot_{slot}"))
                            .selected_text(current_slug.as_str())
                            .show_ui(ui, |ui| {
                                for l in &loads.loadouts {
                                    ui.selectable_value(current_slug, l.slug.clone(), &l.slug);
                                }
                            });
                    });
                }
            }
        });

    // 7. Render Right Inspector & Validation Panel
    egui::SidePanel::right("right_inspector")
        .resizable(true)
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.heading("Map Inspector");
            ui.separator();

            // Metadata editor
            ui.collapsing("Map Metadata", |ui| {
                let mut title = doc.doc.metadata.title.clone();
                if ui
                    .horizontal(|ui| {
                        ui.label("Title:");
                        ui.text_edit_singleline(&mut title)
                    })
                    .inner
                    .changed()
                {
                    let mut after = doc.doc.metadata.clone();
                    after.title = title;
                    let cmd = EditCommand::SetMetadata {
                        before: doc.doc.metadata.clone(),
                        after,
                    };
                    let mut cs = ChangeSet::default();
                    history.execute(cmd, &mut doc.doc, &mut cs);
                    doc.apply_changeset(&cs, now_secs);
                }

                let mut author = doc.doc.metadata.author.clone();
                if ui
                    .horizontal(|ui| {
                        ui.label("Author:");
                        ui.text_edit_singleline(&mut author)
                    })
                    .inner
                    .changed()
                {
                    let mut after = doc.doc.metadata.clone();
                    after.author = author;
                    let cmd = EditCommand::SetMetadata {
                        before: doc.doc.metadata.clone(),
                        after,
                    };
                    let mut cs = ChangeSet::default();
                    history.execute(cmd, &mut doc.doc, &mut cs);
                    doc.apply_changeset(&cs, now_secs);
                }

                if let Some(catalog) = &theme_catalog {
                    let mut theme = doc.doc.metadata.theme.clone();
                    ui.horizontal(|ui| {
                        ui.label("Theme:");
                        egui::ComboBox::from_id_salt("theme_selector")
                            .selected_text(&theme)
                            .show_ui(ui, |ui| {
                                for t in &catalog.themes {
                                    if ui.selectable_label(theme == t.slug, &t.slug).clicked() {
                                        theme.clone_from(&t.slug);
                                    }
                                }
                            });
                    });
                    if theme != doc.doc.metadata.theme {
                        let mut after = doc.doc.metadata.clone();
                        after.theme = theme;
                        let cmd = EditCommand::SetMetadata {
                            before: doc.doc.metadata.clone(),
                            after,
                        };
                        let mut cs = ChangeSet::default();
                        history.execute(cmd, &mut doc.doc, &mut cs);
                        doc.apply_changeset(&cs, now_secs);
                    }
                }
            });

            ui.separator();

            // Selected entity inspector
            ui.collapsing("Selected Entity", |ui| {
                if let Some(ent_id) = tool_state.selected_entity_id {
                    let mut entity_found = false;

                    // Mineral / Gas
                    if let Some(res) = doc.doc.resources.iter().find(|r| r.id == ent_id).cloned() {
                        entity_found = true;
                        ui.label(format!("ID: {}", ent_id.0));
                        ui.label(format!("Type: {:?}", res.kind));
                        ui.label(format!("Cell: ({}, {})", res.cell.0, res.cell.1));
                        let mut amount = res.amount;
                        if ui
                            .add(egui::Slider::new(&mut amount, 100..=50000).text("Amount"))
                            .changed()
                        {
                            let mut after = res.clone();
                            after.amount = amount;
                            let cmd = EditCommand::ModifyEntity {
                                before: MapEntity::Resource(res),
                                after: MapEntity::Resource(after),
                            };
                            let mut cs = ChangeSet::default();
                            history.execute(cmd, &mut doc.doc, &mut cs);
                            doc.apply_changeset(&cs, now_secs);
                        }
                    }

                    // Watchtower
                    if let Some(tower) =
                        doc.doc.watchtowers.iter().find(|w| w.id == ent_id).cloned()
                    {
                        entity_found = true;
                        ui.label(format!("ID: {}", ent_id.0));
                        ui.label("Type: Sensor Watchtower");
                        ui.label(format!("Cell: ({}, {})", tower.cell.0, tower.cell.1));
                        let mut cap = tower.capture_radius_tenths;
                        if ui
                            .add(
                                egui::Slider::new(&mut cap, 10..=100)
                                    .text("Capture Radius (tenths)"),
                            )
                            .changed()
                        {
                            let mut after = tower.clone();
                            after.capture_radius_tenths = cap;
                            let cmd = EditCommand::ModifyEntity {
                                before: MapEntity::Watchtower(tower),
                                after: MapEntity::Watchtower(after),
                            };
                            let mut cs = ChangeSet::default();
                            history.execute(cmd, &mut doc.doc, &mut cs);
                            doc.apply_changeset(&cs, now_secs);
                        }
                    }

                    // Start location
                    if let Some(start) = doc
                        .doc
                        .start_locations
                        .iter()
                        .find(|s| s.id == ent_id)
                        .cloned()
                    {
                        entity_found = true;
                        ui.label(format!("ID: {}", ent_id.0));
                        ui.label("Type: Start Location");
                        ui.label(format!("Cell: ({}, {})", start.cell.0, start.cell.1));
                        let mut slot = start.player_slot;
                        if ui
                            .add(egui::Slider::new(&mut slot, 0..=7).text("Player Slot"))
                            .changed()
                        {
                            let mut after = start.clone();
                            after.player_slot = slot;
                            let cmd = EditCommand::ModifyEntity {
                                before: MapEntity::StartLocation(start),
                                after: MapEntity::StartLocation(after),
                            };
                            let mut cs = ChangeSet::default();
                            history.execute(cmd, &mut doc.doc, &mut cs);
                            doc.apply_changeset(&cs, now_secs);
                        }
                    }

                    if entity_found {
                        ui.add_space(4.0);
                        if ui.button("Delete Entity (Delete)").clicked() {
                            if let Some(ent) = find_entity_by_id(&doc.doc, ent_id) {
                                let cmd = delete_entity_command(&ent);
                                let mut cs = ChangeSet::default();
                                history.execute(cmd, &mut doc.doc, &mut cs);
                                doc.apply_changeset(&cs, now_secs);
                                tool_state.selected_entity_id = None;
                            }
                        }
                    } else {
                        ui.label("Entity no longer exists.");
                    }
                } else {
                    ui.label("No entity selected.");
                }
            });

            ui.separator();

            // Validation list: errors then warnings with jump buttons
            ui.heading(format!(
                "Validation ({}/{} errors/warnings)",
                doc.validation.errors.len(),
                doc.validation.warnings.len()
            ));

            egui::ScrollArea::vertical().show(ui, |ui| {
                for err in &doc.validation.errors {
                    let loc = err.location();
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(255, 80, 80), "✖");
                        if ui.button("→").on_hover_text("Jump to cell").clicked() {
                            focus_events.send(EditorCameraFocus { cell: loc.min });
                        }
                        ui.label(format!("{err}"));
                    });
                }
                for warn in &doc.validation.warnings {
                    let loc = warn.location();
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(255, 200, 50), "⚠");
                        if ui.button("→").on_hover_text("Jump to cell").clicked() {
                            focus_events.send(EditorCameraFocus { cell: loc.min });
                        }
                        ui.label(format!("{warn}"));
                    });
                }
            });
        });

    // 8. Render Bottom Status Bar
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            // Hovered cell info
            if let Some((x, y)) = tool_state.hovered_cell {
                if let Some(spec) = doc.doc.cell(x, y) {
                    ui.label(format!(
                        "Cell: ({x}, {y}) | Lvl: {} | {:?} | Ramp: {:?} | Flags: {:?}",
                        spec.level.0, spec.terrain, spec.ramp, spec.flags
                    ));
                }
            } else {
                ui.label("Cell: (-- , --)");
            }

            ui.separator();

            // Tool hint or last status error
            if let Some(ref msg) = tool_state.last_status_message {
                ui.colored_label(egui::Color32::from_rgb(255, 220, 100), msg);
            } else {
                ui.label("Ready");
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Validation summary
                if doc.validation.errors.is_empty() {
                    ui.colored_label(egui::Color32::from_rgb(100, 255, 100), "✔ Valid");
                } else {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 80, 80),
                        format!("✖ {} Errors", doc.validation.errors.len()),
                    );
                }

                ui.separator();

                // Revision / Dirty marker
                let dirty_marker = if doc.dirty { "*" } else { "" };
                ui.label(format!("Rev: {}{}", doc.revision, dirty_marker));
            });
        });
    });

    // 9. Modals: File Browser, New Map, Unsaved Changes
    if let Some(action) = file_browser.render(ctx, &maps_dir) {
        match action {
            FileBrowserAction::Load(path) => {
                match sim_map::map_format::MapFile::load(&path).and_then(|f| f.decode()) {
                    Ok(loaded_doc) => {
                        *doc = EditorDocument::new(loaded_doc, Some(path.clone()));
                        history.clear();
                        if let Ok(gm) = doc.doc.to_game_map() {
                            terrain_state.replace_map(gm);
                        }
                        tool_state
                            .stroke_recorder
                            .resize(doc.doc.width, doc.doc.height);
                        tool_state.last_status_message = Some(format!("Opened {}", path.display()));
                    }
                    Err(e) => {
                        file_browser.error_banner = Some(format!("Failed to open map: {e}"));
                        file_browser.is_open = true;
                    }
                }
            }
            FileBrowserAction::Save(path) => {
                let map_file = doc.doc.encode();
                match map_file.save_atomic(&path) {
                    Ok(()) => {
                        doc.mark_saved(path.clone());
                        tool_state.last_status_message = Some(format!("Saved {}", path.display()));
                        let outcome = resume_pending_action(
                            &mut unsaved_modal,
                            &mut file_browser,
                            &maps_dir,
                            &mut new_map_modal,
                        );
                        if outcome == UnsavedOutcome::Exit {
                            exit_events.send(AppExit::Success);
                        }
                    }
                    Err(e) => {
                        file_browser.error_banner = Some(format!("Failed to save map: {e}"));
                        file_browser.is_open = true;
                    }
                }
            }
            FileBrowserAction::Cancel => unsaved_modal.pending_action = None,
        }
    }

    render_new_map_modal(
        ctx,
        &mut new_map_modal,
        &mut doc,
        &mut history,
        &mut terrain_state,
        theme_catalog.as_ref(),
        &mut tool_state,
    );

    render_unsaved_changes_modal(
        ctx,
        &mut unsaved_modal,
        &mut doc,
        &mut file_browser,
        &maps_dir,
        &mut new_map_modal,
        &mut tool_state,
        &mut exit_events,
    );

    // 10. Viewport 3D Interaction: Picking, Brush Strokes, Box Fill, Entity Operations
    if !wants_pointer {
        handle_viewport_tool_interaction(
            &mouse_buttons,
            &windows,
            &camera_query,
            &mut doc,
            &mut history,
            &mut tool_state,
            &mut terrain_state,
            &mut terrain_params,
            now_secs,
        );
    } else {
        // Clear brush overlay highlight when pointer is over UI
        terrain_params.overlay_flags &= !OVERLAY_FLAG_BRUSH;
        tool_state.hovered_cell = None;
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
            playtest.state.error_banner = Some(format!("Cannot playtest: {error}"));
        }
    }
}

fn render_new_map_modal(
    ctx: &egui::Context,
    state: &mut NewMapModalState,
    doc: &mut EditorDocument,
    history: &mut History,
    terrain: &mut TerrainState,
    theme_catalog: Option<&Res<'_, ThemeCatalogResource>>,
    tool_state: &mut EditorToolState,
) {
    if !state.is_open {
        return;
    }

    egui::Window::new("New Map")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            if let Some(ref err) = state.error_banner {
                ui.colored_label(egui::Color32::from_rgb(255, 80, 80), err);
            }

            ui.horizontal(|ui| {
                ui.label("Width:");
                ui.add(egui::Slider::new(
                    &mut state.width,
                    MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION,
                ));
            });
            ui.horizontal(|ui| {
                ui.label("Height:");
                ui.add(egui::Slider::new(
                    &mut state.height,
                    MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION,
                ));
            });
            ui.horizontal(|ui| {
                ui.label("Title:");
                ui.text_edit_singleline(&mut state.title);
            });

            if let Some(catalog) = theme_catalog {
                ui.horizontal(|ui| {
                    ui.label("Theme:");
                    egui::ComboBox::from_id_salt("new_map_theme")
                        .selected_text(&state.theme)
                        .show_ui(ui, |ui| {
                            for t in &catalog.themes {
                                ui.selectable_value(&mut state.theme, t.slug.clone(), &t.slug);
                            }
                        });
                });
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Create").clicked() {
                    let meta = MapMetadata {
                        title: state.title.clone(),
                        author: "Anonymous".to_string(),
                        description: String::new(),
                        theme: state.theme.clone(),
                    };
                    match EditorDocument::new_blank(state.width, state.height, meta) {
                        Ok(new_doc) => {
                            *doc = new_doc;
                            history.clear();
                            if let Ok(gm) = doc.doc.to_game_map() {
                                terrain.replace_map(gm);
                            }
                            tool_state.stroke_recorder.resize(state.width, state.height);
                            tool_state.last_status_message = Some("Created new blank map".into());
                            state.is_open = false;
                        }
                        Err(e) => {
                            state.error_banner = Some(format!("Error: {e}"));
                        }
                    }
                }
                if ui.button("Cancel").clicked() {
                    state.is_open = false;
                }
            });
        });
}

/// Button pressed in the unsaved changes dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsavedChoice {
    /// Save the document, then continue with the guarded action.
    Save,
    /// Abandon the edits and continue with the guarded action.
    Discard,
    /// Keep editing and abandon the guarded action.
    Cancel,
}

/// What the application must do after an unsaved changes decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsavedOutcome {
    /// Keep running.
    Continue,
    /// The user chose to quit, and their document is resolved.
    Exit,
}

/// Continues the action the unsaved changes prompt was guarding, once its edits are resolved.
fn resume_pending_action(
    modal: &mut UnsavedChangesModalState,
    file_browser: &mut FileBrowserState,
    maps_dir: &Path,
    new_map: &mut NewMapModalState,
) -> UnsavedOutcome {
    match modal.pending_action.take() {
        Some(PendingUnsavedAction::NewMap) => new_map.is_open = true,
        Some(PendingUnsavedAction::OpenFile) => {
            file_browser.open(FileBrowserMode::Open, maps_dir, None);
        }
        Some(PendingUnsavedAction::Quit) => return UnsavedOutcome::Exit,
        None => {}
    }
    UnsavedOutcome::Continue
}

/// Applies the user's decision in the unsaved changes dialog.
///
/// - [`UnsavedChoice::Save`] resumes the guarded action once the document is on disk. An untitled
///   document keeps the action pending until the Save As dialog finishes. A failed save keeps
///   the prompt open so the edits are not lost.
/// - [`UnsavedChoice::Discard`] resumes the action but leaves [`EditorDocument::dirty`] set: the
///   edits still exist until a new document replaces them, and that dialog can be cancelled.
/// - [`UnsavedChoice::Cancel`] abandons the action.
pub fn apply_unsaved_choice(
    choice: UnsavedChoice,
    modal: &mut UnsavedChangesModalState,
    doc: &mut EditorDocument,
    file_browser: &mut FileBrowserState,
    maps_dir: &Path,
    new_map: &mut NewMapModalState,
    tool_state: &mut EditorToolState,
) -> UnsavedOutcome {
    match choice {
        UnsavedChoice::Save => match handle_save_action(doc, file_browser, maps_dir, tool_state) {
            SaveOutcome::Saved => {
                modal.is_open = false;
                resume_pending_action(modal, file_browser, maps_dir, new_map)
            }
            SaveOutcome::AwaitingPath => {
                modal.is_open = false;
                UnsavedOutcome::Continue
            }
            SaveOutcome::Failed => UnsavedOutcome::Continue,
        },
        UnsavedChoice::Discard => {
            modal.is_open = false;
            resume_pending_action(modal, file_browser, maps_dir, new_map)
        }
        UnsavedChoice::Cancel => {
            modal.is_open = false;
            modal.pending_action = None;
            UnsavedOutcome::Continue
        }
    }
}

fn render_unsaved_changes_modal(
    ctx: &egui::Context,
    modal: &mut UnsavedChangesModalState,
    doc: &mut EditorDocument,
    file_browser: &mut FileBrowserState,
    maps_dir: &Path,
    new_map: &mut NewMapModalState,
    tool_state: &mut EditorToolState,
    exit_events: &mut EventWriter<'_, AppExit>,
) {
    if !modal.is_open {
        return;
    }

    let mut choice = None;
    egui::Window::new("Unsaved Changes")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label("The current map document has unsaved modifications.");
            ui.label("Do you want to save your changes before proceeding?");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    choice = Some(UnsavedChoice::Save);
                }
                if ui.button("Discard").clicked() {
                    choice = Some(UnsavedChoice::Discard);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(UnsavedChoice::Cancel);
                }
            });
        });

    if let Some(choice) = choice {
        let outcome = apply_unsaved_choice(
            choice,
            modal,
            doc,
            file_browser,
            maps_dir,
            new_map,
            tool_state,
        );
        if outcome == UnsavedOutcome::Exit {
            exit_events.send(AppExit::Success);
        }
    }
}

fn handle_viewport_tool_interaction(
    mouse_buttons: &ButtonInput<MouseButton>,
    windows: &Query<'_, '_, &Window>,
    camera_query: &Query<'_, '_, (&Camera, &GlobalTransform, &mut crate::camera::RtsCamera)>,
    doc: &mut EditorDocument,
    history: &mut History,
    tool_state: &mut EditorToolState,
    terrain: &mut TerrainState,
    terrain_params: &mut TerrainParamsState,
    now_secs: f64,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        terrain_params.overlay_flags &= !OVERLAY_FLAG_BRUSH;
        tool_state.hovered_cell = None;
        return;
    };
    let Ok((camera, camera_transform, _)) = camera_query.get_single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    let hit = pick_terrain(&terrain.map, ray.origin, *ray.direction);

    // Update brush hover overlay
    let hit_cell = if let Some(ref h) = hit {
        tool_state.hovered_cell = Some(h.cell);
        terrain_params.brush_center = Vec2::new(h.cell.0 as f32 + 0.5, h.cell.1 as f32 + 0.5);
        terrain_params.brush_radius = tool_state.brush_radius as f32;
        terrain_params.brush_shape = match tool_state.brush_shape {
            BrushShape::Circle => 0,
            BrushShape::Square => 1,
        };
        terrain_params.overlay_flags |= OVERLAY_FLAG_BRUSH;
        h.cell
    } else {
        tool_state.hovered_cell = None;
        terrain_params.overlay_flags &= !OVERLAY_FLAG_BRUSH;
        return;
    };

    // Viewport mouse interactions based on active tool
    match tool_state.current_tool {
        EditorTool::Brush => {
            if mouse_buttons.just_pressed(MouseButton::Left) {
                tool_state.is_painting = true;
                tool_state.stroke_recorder.begin();
                paint_brush_stamp(
                    &mut tool_state.stroke_recorder,
                    &mut doc.doc,
                    tool_state.brush_shape,
                    tool_state.brush_radius,
                    hit_cell,
                    &tool_state.paint_op,
                    tool_state.symmetry,
                    &mut tool_state.scratch_coords,
                );
                sync_scratch_cells_to_terrain(&tool_state.scratch_coords, doc, terrain, now_secs);
            } else if mouse_buttons.pressed(MouseButton::Left) && tool_state.is_painting {
                paint_brush_stamp(
                    &mut tool_state.stroke_recorder,
                    &mut doc.doc,
                    tool_state.brush_shape,
                    tool_state.brush_radius,
                    hit_cell,
                    &tool_state.paint_op,
                    tool_state.symmetry,
                    &mut tool_state.scratch_coords,
                );
                sync_scratch_cells_to_terrain(&tool_state.scratch_coords, doc, terrain, now_secs);
            } else if mouse_buttons.just_released(MouseButton::Left) && tool_state.is_painting {
                tool_state.is_painting = false;
                if let Some(cmd) = tool_state.stroke_recorder.finish("Paint Brush") {
                    history.push_applied(cmd);
                }
            }
        }
        EditorTool::Box => {
            if mouse_buttons.just_pressed(MouseButton::Left) {
                tool_state.box_start_cell = Some(hit_cell);
            } else if mouse_buttons.just_released(MouseButton::Left) {
                if let Some(start_cell) = tool_state.box_start_cell.take() {
                    let rect = CellRect {
                        min: (start_cell.0.min(hit_cell.0), start_cell.1.min(hit_cell.1)),
                        max_inclusive: (start_cell.0.max(hit_cell.0), start_cell.1.max(hit_cell.1)),
                    };
                    let cmd = box_fill_command(
                        &doc.doc,
                        rect,
                        &tool_state.paint_op,
                        tool_state.symmetry,
                        "Box Fill",
                    );
                    let mut cs = ChangeSet::default();
                    history.execute(cmd, &mut doc.doc, &mut cs);
                    doc.apply_changeset(&cs, now_secs);
                    doc.sync_terrain(&cs, terrain);
                }
            }
        }
        EditorTool::Ramp => {
            if mouse_buttons.just_pressed(MouseButton::Left) {
                let rect = CellRect {
                    min: (hit_cell.0, hit_cell.1),
                    max_inclusive: (hit_cell.0, hit_cell.1),
                };
                let placement_res = match tool_state.ramp_mode {
                    RampToolMode::Auto => infer_ramp(&doc.doc, rect),
                    RampToolMode::Manual(dir) => check_ramp_placement(&doc.doc, rect, dir),
                };

                match placement_res {
                    Ok(placement) => {
                        match place_ramp_command(
                            &doc.doc,
                            rect,
                            placement,
                            tool_state.symmetry,
                            "Place Ramp",
                        ) {
                            Ok(cmd) => {
                                let mut cs = ChangeSet::default();
                                history.execute(cmd, &mut doc.doc, &mut cs);
                                doc.apply_changeset(&cs, now_secs);
                                doc.sync_terrain(&cs, terrain);
                                tool_state.last_status_message = Some("Placed ramp".into());
                            }
                            Err(e) => {
                                tool_state.last_status_message = Some(format!("Ramp error: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        tool_state.last_status_message = Some(format!("Ramp error: {e}"));
                    }
                }
            }
        }
        EditorTool::Entity => {
            if mouse_buttons.just_pressed(MouseButton::Left) {
                let cmd = place_entity_command(
                    &doc.doc,
                    tool_state.entity_kind,
                    hit_cell,
                    tool_state.symmetry,
                );
                let mut cs = ChangeSet::default();
                history.execute(cmd, &mut doc.doc, &mut cs);
                doc.apply_changeset(&cs, now_secs);
                tool_state.last_status_message = Some("Placed entity".into());
            }
        }
        EditorTool::Select => {
            if mouse_buttons.just_pressed(MouseButton::Left) {
                tool_state.selected_entity_id = pick_entity(&doc.doc, hit_cell).map(|e| e.id());
            }
        }
        EditorTool::Eraser => {
            if mouse_buttons.just_pressed(MouseButton::Left) {
                if let Some(cmd) =
                    erase_entities_command(&doc.doc, &[hit_cell], tool_state.symmetry)
                {
                    let mut cs = ChangeSet::default();
                    history.execute(cmd, &mut doc.doc, &mut cs);
                    doc.apply_changeset(&cs, now_secs);
                    tool_state.last_status_message = Some("Erased entity".into());
                } else if let Some(cell) = doc.doc.cell(hit_cell.0, hit_cell.1) {
                    let mut after = cell.clone();
                    after.flags = CellFlags::GROUND_PATHABLE | CellFlags::BUILDABLE;
                    after.ramp = None;
                    let deltas = vec![crate::editor::commands::CellDelta {
                        x: hit_cell.0,
                        y: hit_cell.1,
                        before: cell.clone(),
                        after,
                    }];
                    let cmd = EditCommand::PaintCells {
                        label: "Erase Cell",
                        deltas,
                    };
                    let mut cs = ChangeSet::default();
                    history.execute(cmd, &mut doc.doc, &mut cs);
                    doc.apply_changeset(&cs, now_secs);
                    doc.sync_terrain(&cs, terrain);
                }
            }
        }
    }
}

fn sync_scratch_cells_to_terrain(
    cells: &[(u32, u32)],
    doc: &mut EditorDocument,
    terrain: &mut TerrainState,
    now_secs: f64,
) {
    if cells.is_empty() {
        return;
    }
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0;
    let mut max_y = 0;

    for &(x, y) in cells {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    let rect = CellRect {
        min: (min_x, min_y),
        max_inclusive: (max_x, max_y),
    };

    terrain.apply_cells(rect, |x, y, target_cell| {
        if let Some(spec) = doc.doc.cell(x, y) {
            *target_cell = MapDocument::to_map_cell(spec);
        }
    });

    doc.revision = doc.revision.wrapping_add(1);
    doc.dirty = true;
    doc.last_edit_at_secs = now_secs;
}
