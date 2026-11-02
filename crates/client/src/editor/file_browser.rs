//! File browser modal dialog for opening and saving map documents.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.3 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this module provides the modal file selection dialog for loading and saving `.scmap.ron` files
//! within `assets/maps/`.
//!
//! # Invariants Maintained
//! 1. **Strict File Extension:** Ensures saved files always possess the `.scmap.ron` canonical extension.
//! 2. **Overwrite Safety:** Explicitly prompts the user for confirmation before overwriting an existing map file.
//! 3. **Typed Error Display:** Displays load/save errors directly within the modal using clear error banners.
//! 4. **Deterministic Ordering:** Discovered map files are sorted alphabetically.

use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use bevy_egui::egui;

/// Mode of operation for the file browser modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBrowserMode {
    /// Browse and select an existing map file to open.
    Open,
    /// Specify a map file path to save the current document.
    Save,
}

/// Action produced by the file browser modal during UI interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBrowserAction {
    /// Load map document from the specified absolute file path.
    Load(PathBuf),
    /// Save current map document to the specified absolute file path.
    Save(PathBuf),
    /// Dialog was closed or canceled by the user.
    Cancel,
}

/// ECS Resource managing the state of the file browser modal dialog.
#[derive(Resource, Debug, Clone)]
pub struct FileBrowserState {
    /// Whether the file browser modal window is currently visible.
    pub is_open: bool,
    /// Active mode (Open vs. Save).
    pub mode: FileBrowserMode,
    /// Alphabetically sorted list of map file names found in `assets/maps/`.
    pub file_list: Vec<String>,
    /// Currently selected file name from `file_list`.
    pub selected_file: Option<String>,
    /// User input string for the map file name.
    pub filename_input: String,
    /// Inline error banner message, if an operation failed.
    pub error_banner: Option<String>,
    /// Whether the user is being prompted to confirm overwriting an existing file.
    pub overwrite_confirm: bool,
}

impl Default for FileBrowserState {
    fn default() -> Self {
        Self {
            is_open: false,
            mode: FileBrowserMode::Open,
            file_list: Vec::new(),
            selected_file: None,
            filename_input: String::new(),
            error_banner: None,
            overwrite_confirm: false,
        }
    }
}

impl FileBrowserState {
    /// Opens the dialog in the specified mode, populating the file list from `maps_dir`.
    pub fn open(&mut self, mode: FileBrowserMode, maps_dir: &Path, default_name: Option<&str>) {
        self.is_open = true;
        self.mode = mode;
        self.error_banner = None;
        self.overwrite_confirm = false;
        self.refresh_list(maps_dir);

        if let Some(name) = default_name {
            self.filename_input = name.to_string();
            self.selected_file = Some(name.to_string());
        } else if let Some(first) = self.file_list.first() {
            if mode == FileBrowserMode::Open {
                self.filename_input = first.clone();
                self.selected_file = Some(first.clone());
            }
        }
    }

    /// Closes the dialog and resets error/confirmation flags.
    pub fn close(&mut self) {
        self.is_open = false;
        self.error_banner = None;
        self.overwrite_confirm = false;
    }

    /// Scans `maps_dir` for all files ending in `.scmap.ron`, sorting them alphabetically.
    pub fn refresh_list(&mut self, maps_dir: &Path) {
        self.file_list.clear();
        if let Ok(entries) = fs::read_dir(maps_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if file_name.ends_with(".scmap.ron") {
                        self.file_list.push(file_name);
                    }
                }
            }
        }
        self.file_list.sort();
    }

    /// Renders the file browser modal window, returning an action when a choice is made.
    #[expect(
        clippy::too_many_lines,
        reason = "a single ordered sequence that shares its local state"
    )]
    pub fn render(&mut self, ctx: &egui::Context, maps_dir: &Path) -> Option<FileBrowserAction> {
        if !self.is_open {
            return None;
        }

        let title = match self.mode {
            FileBrowserMode::Open => "Open Map",
            FileBrowserMode::Save => "Save Map As",
        };

        let mut action = None;
        let mut open_window = true;

        egui::Window::new(title)
            .open(&mut open_window)
            .collapsible(false)
            .resizable(true)
            .default_width(480.0)
            .default_height(360.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                if let Some(ref err) = self.error_banner {
                    ui.colored_label(egui::Color32::from_rgb(240, 80, 80), format!("⚠ {err}"));
                    ui.add_space(4.0);
                }

                ui.label(format!("Directory: {}", maps_dir.display()));
                ui.add_space(4.0);

                // File list scroll area
                ui.group(|ui| {
                    ui.set_height(180.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for file_name in &self.file_list {
                            let is_selected = self.selected_file.as_deref() == Some(file_name);
                            if ui.selectable_label(is_selected, file_name).clicked() {
                                self.selected_file = Some(file_name.clone());
                                self.filename_input.clone_from(file_name);
                                self.overwrite_confirm = false;
                                self.error_banner = None;
                            }
                        }
                    });
                });

                ui.add_space(8.0);

                // Filename input
                ui.horizontal(|ui| {
                    ui.label("File name:");
                    let response = ui.text_edit_singleline(&mut self.filename_input);
                    if response.changed() {
                        self.overwrite_confirm = false;
                        self.error_banner = None;
                    }
                });

                ui.add_space(8.0);

                // Overwrite prompt if active
                if self.overwrite_confirm {
                    ui.group(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(250, 180, 50),
                            format!("File '{}' already exists. Overwrite?", self.filename_input),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Yes, Overwrite").clicked() {
                                let mut name = self.filename_input.trim().to_string();
                                if !name.ends_with(".scmap.ron") {
                                    name.push_str(".scmap.ron");
                                }
                                let target_path = maps_dir.join(&name);
                                self.close();
                                action = Some(FileBrowserAction::Save(target_path));
                            }
                            if ui.button("Cancel").clicked() {
                                self.overwrite_confirm = false;
                            }
                        });
                    });
                    ui.add_space(4.0);
                }

                // Action buttons
                ui.horizontal(|ui| {
                    match self.mode {
                        FileBrowserMode::Open => {
                            if ui.button("Open").clicked() {
                                let trimmed = self.filename_input.trim();
                                if trimmed.is_empty() {
                                    self.error_banner =
                                        Some("Please select or enter a file name.".into());
                                } else {
                                    let mut name = trimmed.to_string();
                                    if !name.ends_with(".scmap.ron") {
                                        name.push_str(".scmap.ron");
                                    }
                                    let target_path = maps_dir.join(&name);
                                    if target_path.exists() {
                                        self.close();
                                        action = Some(FileBrowserAction::Load(target_path));
                                    } else {
                                        self.error_banner =
                                            Some(format!("File '{name}' not found."));
                                    }
                                }
                            }
                        }
                        FileBrowserMode::Save => {
                            if !self.overwrite_confirm && ui.button("Save").clicked() {
                                let trimmed = self.filename_input.trim();
                                if trimmed.is_empty() {
                                    self.error_banner = Some("Please enter a file name.".into());
                                } else {
                                    let mut name = trimmed.to_string();
                                    if !name.ends_with(".scmap.ron") {
                                        name.push_str(".scmap.ron");
                                    }
                                    let target_path = maps_dir.join(&name);
                                    if target_path.exists() {
                                        self.overwrite_confirm = true;
                                    } else {
                                        self.close();
                                        action = Some(FileBrowserAction::Save(target_path));
                                    }
                                }
                            }
                        }
                    }

                    if ui.button("Cancel").clicked() {
                        self.close();
                        action = Some(FileBrowserAction::Cancel);
                    }
                });
            });

        if !open_window && action.is_none() {
            self.close();
            action = Some(FileBrowserAction::Cancel);
        }

        action
    }
}
