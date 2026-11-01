//! Top menu bar, left toolbox, right inspector and bottom status bar of the map editor.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.3, this module draws the docked egui panels. It
//! holds no state of its own: every control reads and edits the resources gathered by
//! [`editor_ui_system`](super::editor_ui_system).
//!
//! # Invariants Maintained
//! 1. **Undoable Edits Only:** Document changes go through [`History`](crate::editor::commands::History),
//!    with continuous controls coalesced into one entry per gesture.
//! 2. **No Edits During a Stroke:** Undo and Redo stay disabled while a brush stroke is live.

use std::path::Path;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_egui::egui;
use sim_map::grid::{CliffLevel, RampDir, TerrainKind};

use super::{
    find_entity_by_id, handle_playtest_entry, handle_save_action, EditorTool, EditorToolState,
    EditorUiContext, EditorUiState, EditorUiTerrain, PendingUnsavedAction, RampToolMode,
};
use crate::editor::camera::EditorCameraFocus;
use crate::editor::commands::{ChangeSet, EditCommand, History, MapEntity};
use crate::editor::document::EditorDocument;
use crate::editor::file_browser::FileBrowserMode;
use crate::editor::symmetry::SymmetryMode;
use crate::editor::tools::{
    delete_entity_command, resolve_start_footprint, AuthorableFlag, BrushShape, EntityToolKind,
    PaintOp, MAX_BRUSH_RADIUS,
};
use crate::terrain::{
    OVERLAY_FLAG_BUILDABILITY, OVERLAY_FLAG_GRID, OVERLAY_FLAG_PATHABILITY, OVERLAY_FLAG_VALIDATION,
};

/// Draws the top menu bar.
pub(super) fn render_menu_bar(
    ctx: &egui::Context,
    input: &mut EditorUiContext<'_, '_>,
    state: &mut EditorUiState<'_>,
    terrain: &mut EditorUiTerrain<'_>,
    maps_dir: &Path,
    now_secs: f64,
) {
    egui::TopBottomPanel::top("top_menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            render_file_menu(ui, &mut input.exit_events, state, maps_dir);
            render_edit_menu(ui, state, terrain, now_secs);
            render_view_menu(ui, state, terrain);
            render_symmetry_menu(ui, state);

            let EditorUiState {
                doc,
                tool_state,
                playtest,
                ..
            } = state;
            let focus_events = &mut input.focus_events;
            if ui.button("Playtest (F5)").clicked() {
                handle_playtest_entry(doc, playtest, tool_state, maps_dir, focus_events);
            }
        });
    });
}

fn render_file_menu(
    ui: &mut egui::Ui,
    exit_events: &mut EventWriter<'_, AppExit>,
    state: &mut EditorUiState<'_>,
    maps_dir: &Path,
) {
    let EditorUiState {
        doc,
        tool_state,
        file_browser,
        new_map_modal,
        unsaved_modal,
        ..
    } = state;
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
                file_browser.open(FileBrowserMode::Open, maps_dir, None);
            }
        }
        if ui.button("Save (Ctrl+S)").clicked() {
            ui.close_menu();
            handle_save_action(doc, file_browser, maps_dir, tool_state);
        }
        if ui.button("Save As…").clicked() {
            ui.close_menu();
            file_browser.open(
                FileBrowserMode::Save,
                maps_dir,
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
}

fn render_edit_menu(
    ui: &mut egui::Ui,
    state: &mut EditorUiState<'_>,
    terrain: &mut EditorUiTerrain<'_>,
    now_secs: f64,
) {
    let EditorUiState {
        doc,
        history,
        tool_state,
        ..
    } = state;
    let terrain_state = &mut terrain.terrain_state;
    ui.menu_button("Edit", |ui| {
        let can_undo = history.can_undo() && !tool_state.is_painting;
        let can_redo = history.can_redo() && !tool_state.is_painting;
        if ui
            .add_enabled(can_undo, egui::Button::new("Undo (Ctrl+Z)"))
            .clicked()
        {
            ui.close_menu();
            let mut cs = ChangeSet::default();
            if history.undo(&mut doc.doc, &mut cs) {
                doc.apply_changeset(&cs, now_secs);
                doc.sync_terrain(&cs, terrain_state);
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
                doc.sync_terrain(&cs, terrain_state);
            }
        }
    });
}

fn render_view_menu(
    ui: &mut egui::Ui,
    state: &mut EditorUiState<'_>,
    terrain: &mut EditorUiTerrain<'_>,
) {
    let los_state = &mut state.los_state;
    let terrain_params = &mut terrain.terrain_params;
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
}

fn render_symmetry_menu(ui: &mut egui::Ui, state: &mut EditorUiState<'_>) {
    let tool_state = &mut state.tool_state;
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
}

/// Draws the left toolbox panel.
pub(super) fn render_toolbox(
    ctx: &egui::Context,
    state: &mut EditorUiState<'_>,
    terrain: &EditorUiTerrain<'_>,
) {
    let tool_state = &mut state.tool_state;
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
                EditorTool::Brush | EditorTool::Box => render_paint_options(ui, tool_state),
                EditorTool::Ramp => render_ramp_options(ui, tool_state),
                EditorTool::Entity => render_entity_options(ui, tool_state, terrain),
                EditorTool::Select => {
                    ui.label("Click any entity in viewport to inspect properties.");
                }
                EditorTool::Eraser => {
                    ui.label("Click cells or entities to erase them.");
                }
            }

            ui.separator();
            render_loadout_selectors(ui, tool_state, terrain);
        });
}

fn render_paint_options(ui: &mut egui::Ui, tool_state: &mut EditorToolState) {
    if tool_state.current_tool == EditorTool::Brush {
        ui.label("Brush Shape:");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut tool_state.brush_shape, BrushShape::Circle, "Circle");
            ui.selectable_value(&mut tool_state.brush_shape, BrushShape::Square, "Square");
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
            ui.selectable_value(&mut tool_state.paint_op, PaintOp::Level(CliffLevel(0)), "0");
            ui.selectable_value(&mut tool_state.paint_op, PaintOp::Level(CliffLevel(1)), "1");
            ui.selectable_value(&mut tool_state.paint_op, PaintOp::Level(CliffLevel(2)), "2");
            ui.selectable_value(&mut tool_state.paint_op, PaintOp::Level(CliffLevel(3)), "3");
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

fn render_ramp_options(ui: &mut egui::Ui, tool_state: &mut EditorToolState) {
    ui.label("Ramp Direction:");
    ui.selectable_value(&mut tool_state.ramp_mode, RampToolMode::Auto, "Auto Infer");
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

fn render_entity_options(
    ui: &mut egui::Ui,
    tool_state: &mut EditorToolState,
    terrain: &EditorUiTerrain<'_>,
) {
    let EditorUiTerrain {
        loadouts,
        unit_catalog,
        ..
    } = terrain;
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
    let is_start = matches!(tool_state.entity_kind, EntityToolKind::StartLocation { .. });
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
            let fp = if let (Some(loads), Some(units)) = (&loadouts, &unit_catalog) {
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

fn render_loadout_selectors(
    ui: &mut egui::Ui,
    tool_state: &mut EditorToolState,
    terrain: &EditorUiTerrain<'_>,
) {
    let loadouts = &terrain.loadouts;
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
}

/// Draws the right inspector panel with metadata, selection and validation sections.
pub(super) fn render_inspector(
    ctx: &egui::Context,
    input: &mut EditorUiContext<'_, '_>,
    state: &mut EditorUiState<'_>,
    terrain: &EditorUiTerrain<'_>,
    now_secs: f64,
) {
    let EditorUiState {
        doc,
        history,
        tool_state,
        ..
    } = state;
    egui::SidePanel::right("right_inspector")
        .resizable(true)
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.heading("Map Inspector");
            ui.separator();
            render_metadata_section(ui, doc, history, terrain, now_secs);
            ui.separator();
            render_selected_entity_section(ui, doc, history, tool_state, now_secs);
            ui.separator();
            render_validation_section(ui, doc, &mut input.focus_events);
        });
}

fn render_metadata_section(
    ui: &mut egui::Ui,
    doc: &mut ResMut<'_, EditorDocument>,
    history: &mut History,
    terrain: &EditorUiTerrain<'_>,
    now_secs: f64,
) {
    let theme_catalog = &terrain.theme_catalog;
    ui.collapsing("Map Metadata", |ui| {
        let mut title = doc.doc.metadata.title.clone();
        let response = ui
            .horizontal(|ui| {
                ui.label("Title:");
                ui.text_edit_singleline(&mut title)
            })
            .inner;
        if response.changed() {
            let mut after = doc.doc.metadata.clone();
            after.title = title;
            let cmd = EditCommand::SetMetadata {
                before: doc.doc.metadata.clone(),
                after,
            };
            let mut cs = ChangeSet::default();
            history.execute_merged(cmd, &mut doc.doc, &mut cs);
            doc.apply_changeset(&cs, now_secs);
        }
        if response.lost_focus() {
            history.seal_merge();
        }

        let mut author = doc.doc.metadata.author.clone();
        let response = ui
            .horizontal(|ui| {
                ui.label("Author:");
                ui.text_edit_singleline(&mut author)
            })
            .inner;
        if response.changed() {
            let mut after = doc.doc.metadata.clone();
            after.author = author;
            let cmd = EditCommand::SetMetadata {
                before: doc.doc.metadata.clone(),
                after,
            };
            let mut cs = ChangeSet::default();
            history.execute_merged(cmd, &mut doc.doc, &mut cs);
            doc.apply_changeset(&cs, now_secs);
        }
        if response.lost_focus() {
            history.seal_merge();
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
}

fn render_selected_entity_section(
    ui: &mut egui::Ui,
    doc: &mut ResMut<'_, EditorDocument>,
    history: &mut History,
    tool_state: &mut EditorToolState,
    now_secs: f64,
) {
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
                let response = ui.add(egui::Slider::new(&mut amount, 100..=50000).text("Amount"));
                if response.changed() {
                    let mut after = res.clone();
                    after.amount = amount;
                    let cmd = EditCommand::ModifyEntity {
                        before: MapEntity::Resource(res),
                        after: MapEntity::Resource(after),
                    };
                    let mut cs = ChangeSet::default();
                    history.execute_merged(cmd, &mut doc.doc, &mut cs);
                    doc.apply_changeset(&cs, now_secs);
                }
                if response.drag_stopped() {
                    history.seal_merge();
                }
            }

            // Watchtower
            if let Some(tower) = doc.doc.watchtowers.iter().find(|w| w.id == ent_id).cloned() {
                entity_found = true;
                ui.label(format!("ID: {}", ent_id.0));
                ui.label("Type: Sensor Watchtower");
                ui.label(format!("Cell: ({}, {})", tower.cell.0, tower.cell.1));
                let mut cap = tower.capture_radius_tenths;
                let response =
                    ui.add(egui::Slider::new(&mut cap, 10..=100).text("Capture Radius (tenths)"));
                if response.changed() {
                    let mut after = tower.clone();
                    after.capture_radius_tenths = cap;
                    let cmd = EditCommand::ModifyEntity {
                        before: MapEntity::Watchtower(tower),
                        after: MapEntity::Watchtower(after),
                    };
                    let mut cs = ChangeSet::default();
                    history.execute_merged(cmd, &mut doc.doc, &mut cs);
                    doc.apply_changeset(&cs, now_secs);
                }
                if response.drag_stopped() {
                    history.seal_merge();
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
                let response = ui.add(egui::Slider::new(&mut slot, 0..=7).text("Player Slot"));
                if response.changed() {
                    let mut after = start.clone();
                    after.player_slot = slot;
                    let cmd = EditCommand::ModifyEntity {
                        before: MapEntity::StartLocation(start),
                        after: MapEntity::StartLocation(after),
                    };
                    let mut cs = ChangeSet::default();
                    history.execute_merged(cmd, &mut doc.doc, &mut cs);
                    doc.apply_changeset(&cs, now_secs);
                }
                if response.drag_stopped() {
                    history.seal_merge();
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
}

fn render_validation_section(
    ui: &mut egui::Ui,
    doc: &ResMut<'_, EditorDocument>,
    focus_events: &mut EventWriter<'_, EditorCameraFocus>,
) {
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
}

/// Draws the bottom status bar.
pub(super) fn render_status_bar(ctx: &egui::Context, state: &EditorUiState<'_>) {
    let EditorUiState {
        doc, tool_state, ..
    } = state;
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
}
