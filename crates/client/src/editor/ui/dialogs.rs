//! File browser handling and the New Map and Unsaved Changes dialogs.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.3, this module completes the actions dialogs
//! produce. Opening and creating maps are delegated to
//! [`apply_document_load_system`](crate::editor::document_load::apply_document_load_system).
//!
//! # Invariants Maintained
//! 1. **Dirty Means Unsaved:** `dirty` stays set until the document is really saved or replaced.
//! 2. **Guarded Actions Resume Once:** An action held by the unsaved changes prompt continues
//!    only after its edits are resolved, and a failed save never loses the prompt.

use std::path::Path;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_egui::egui;
use sim_map::map_format::{MapMetadata, MAX_MAP_DIMENSION, MIN_MAP_DIMENSION};

use super::{
    handle_save_action, EditorToolState, EditorUiContext, EditorUiState, EditorUiTerrain,
    NewMapModalState, PendingUnsavedAction, SaveOutcome, UnsavedChangesModalState,
};
use crate::editor::app::ThemeCatalogResource;
use crate::editor::document::EditorDocument;
use crate::editor::document_load::LoadDocumentRequest;
use crate::editor::file_browser::{FileBrowserAction, FileBrowserMode, FileBrowserState};

/// Draws every modal dialog and completes the actions they produce.
pub(super) fn render_dialogs(
    ctx: &egui::Context,
    input: &mut EditorUiContext<'_, '_>,
    state: &mut EditorUiState<'_>,
    terrain: &EditorUiTerrain<'_>,
    maps_dir: &Path,
) {
    handle_file_browser(ctx, input, state, maps_dir);
    render_new_map_modal(
        ctx,
        &mut state.new_map_modal,
        terrain.theme_catalog.as_ref(),
        &mut input.load_requests,
    );
    render_unsaved_changes_modal(
        ctx,
        &mut state.unsaved_modal,
        &mut state.doc,
        &mut state.file_browser,
        maps_dir,
        &mut state.new_map_modal,
        &mut state.tool_state,
        &mut input.exit_events,
    );
}

fn handle_file_browser(
    ctx: &egui::Context,
    input: &mut EditorUiContext<'_, '_>,
    state: &mut EditorUiState<'_>,
    maps_dir: &Path,
) {
    let EditorUiContext {
        exit_events,
        load_requests,
        ..
    } = input;
    let EditorUiState {
        doc,
        tool_state,
        file_browser,
        new_map_modal,
        unsaved_modal,
        ..
    } = state;
    if let Some(action) = file_browser.render(ctx, maps_dir) {
        match action {
            FileBrowserAction::Load(path) => {
                load_requests.send(LoadDocumentRequest::Open(path));
            }
            FileBrowserAction::Save(path) => {
                let map_file = doc.doc.encode();
                match map_file.save_atomic(&path) {
                    Ok(()) => {
                        doc.mark_saved(path.clone());
                        tool_state.last_status_message = Some(format!("Saved {}", path.display()));
                        let outcome = resume_pending_action(
                            unsaved_modal,
                            file_browser,
                            maps_dir,
                            new_map_modal,
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
}

fn render_new_map_modal(
    ctx: &egui::Context,
    state: &mut NewMapModalState,
    theme_catalog: Option<&Res<'_, ThemeCatalogResource>>,
    load_requests: &mut EventWriter<'_, LoadDocumentRequest>,
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
                            load_requests.send(LoadDocumentRequest::Replace(Box::new(new_doc)));
                            state.error_banner = None;
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

#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
fn render_unsaved_changes_modal(
    ctx: &egui::Context,
    modal: &mut UnsavedChangesModalState,
    doc: &mut ResMut<'_, EditorDocument>,
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
