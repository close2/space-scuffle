//! Replacing the open map document and everything derived from it.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.2 and `AGENTS.md`,
//! this module owns the single path that swaps the active document. File > Open and File > New
//! both end here, so chunks, GPU field textures, undo history, the stroke recorder and the LOS
//! buffer can never disagree about the map's dimensions.
//!
//! # Invariants Maintained
//! 1. **Everything Sized to the Map Is Rebuilt:** Chunk grids, splat and overlay textures and shader
//!    parameters are sized at creation, so terrain is respawned under a fresh [`EditorTerrainRoot`]
//!    rather than patched in place.
//! 2. **Transactional Replacement:** The candidate is decoded and its terrain spawned before anything
//!    live changes. On failure the current document, history and terrain are untouched.
//! 3. **No Silent Failures:** A failure is reported through the dialog that requested it and on the
//!    status bar.
//! 4. **No Silent Substitution:** A map naming an unknown theme is refused, never given another's.

use std::path::{Path, PathBuf};

use asset_baker::TerrainThemeError;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use sim_map::map_format::{MapDocument, MapFile, MapFormatError};

use crate::assets::ResolvedAssetsRoot;
use crate::editor::app::ThemeCatalogResource;
use crate::editor::camera::EditorCameraFocus;
use crate::editor::commands::History;
use crate::editor::document::EditorDocument;
use crate::editor::file_browser::FileBrowserState;
use crate::editor::los_preview::LosPreviewState;
use crate::editor::ui::{EditorToolState, NewMapModalState};
use crate::terrain::{spawn_terrain_owned, TerrainMaterial, TerrainParamsState};

/// Marks the root entity that owns one document's terrain chunks, lighting and backdrop.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorTerrainRoot;

/// Request to replace the active document, handled by [`apply_document_load_system`].
#[derive(Event, Debug)]
pub enum LoadDocumentRequest {
    /// Read and decode the `.scmap.ron` file at this path.
    Open(PathBuf),
    /// Switch to this document, already built in memory.
    Replace(Box<EditorDocument>),
}

/// Typed reason a document could not replace the active one.
#[derive(Debug, thiserror::Error)]
pub enum DocumentLoadError {
    /// The file could not be read or decoded, or its cells do not form a valid map.
    #[error(transparent)]
    Map(#[from] MapFormatError),
    /// The map names a terrain theme absent from the catalog.
    #[error("map requests unknown terrain theme {0}")]
    UnknownTheme(String),
    /// A texture required by the map's theme could not be loaded.
    #[error(transparent)]
    Terrain(#[from] TerrainThemeError),
}

/// Asset stores and catalogs needed to spawn a document's terrain.
#[derive(SystemParam)]
pub struct EditorTerrainSpawner<'w, 's> {
    commands: Commands<'w, 's>,
    meshes: ResMut<'w, Assets<Mesh>>,
    terrain_materials: ResMut<'w, Assets<TerrainMaterial>>,
    std_materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    themes: Res<'w, ThemeCatalogResource>,
    assets_root: Res<'w, ResolvedAssetsRoot>,
}

impl EditorTerrainSpawner<'_, '_> {
    /// Spawns terrain for `doc` under a new [`EditorTerrainRoot`] and returns that root.
    ///
    /// `overlay_flags` carries the active overlay toggles across a replacement; `None` uses the
    /// theme defaults. All terrain resources are replaced when commands are applied.
    ///
    /// # Errors
    /// Returns [`DocumentLoadError`] if the theme is unknown, the cells do not form a valid map,
    /// or a texture is missing. Nothing spawned for the attempt survives a failure.
    pub fn spawn(
        &mut self,
        doc: &MapDocument,
        overlay_flags: Option<u32>,
    ) -> Result<Entity, DocumentLoadError> {
        let theme_slug = &doc.metadata.theme;
        let theme = self
            .themes
            .get(theme_slug)
            .ok_or_else(|| DocumentLoadError::UnknownTheme(theme_slug.clone()))?;
        let game_map = doc.to_game_map()?;

        let root = self
            .commands
            .spawn((EditorTerrainRoot, Transform::IDENTITY, Visibility::Visible))
            .id();
        if let Err(error) = spawn_terrain_owned(
            &mut self.commands,
            &mut self.meshes,
            &mut self.terrain_materials,
            &mut self.std_materials,
            &mut self.images,
            theme,
            game_map,
            self.assets_root.as_path(),
            Some(root),
            overlay_flags,
        ) {
            self.commands.entity(root).despawn_recursive();
            return Err(error.into());
        }
        Ok(root)
    }

    /// Despawns `entity` and everything it owns.
    pub fn despawn(&mut self, entity: Entity) {
        self.commands.entity(entity).despawn_recursive();
    }
}

fn read_document(path: &Path) -> Result<EditorDocument, DocumentLoadError> {
    let doc = MapFile::load(path)?.decode()?;
    Ok(EditorDocument::new(doc, Some(path.to_path_buf())))
}

/// System replacing the active document when a [`LoadDocumentRequest`] arrives.
///
/// Only the latest request of a frame is honored; the UI cannot issue two in one frame.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn apply_document_load_system(
    mut requests: EventReader<'_, '_, LoadDocumentRequest>,
    mut spawner: EditorTerrainSpawner<'_, '_>,
    roots: Query<'_, '_, Entity, With<EditorTerrainRoot>>,
    terrain_params: Option<Res<'_, TerrainParamsState>>,
    mut doc: ResMut<'_, EditorDocument>,
    mut history: ResMut<'_, History>,
    mut tool_state: ResMut<'_, EditorToolState>,
    mut los_state: ResMut<'_, LosPreviewState>,
    mut file_browser: ResMut<'_, FileBrowserState>,
    mut new_map_modal: ResMut<'_, NewMapModalState>,
    mut focus_events: EventWriter<'_, EditorCameraFocus>,
) {
    let Some(request) = requests.read().last() else {
        return;
    };

    let candidate = match request {
        LoadDocumentRequest::Open(path) => read_document(path),
        LoadDocumentRequest::Replace(document) => Ok((**document).clone()),
    };
    let overlay_flags = terrain_params.map(|params| params.overlay_flags);
    let result = candidate.and_then(|candidate| {
        let root = spawner.spawn(&candidate.doc, overlay_flags)?;
        Ok((candidate, root))
    });

    let candidate = match result {
        Ok((candidate, root)) => {
            for old_root in roots.iter().filter(|&entity| entity != root) {
                spawner.despawn(old_root);
            }
            candidate
        }
        Err(error) => {
            let message = format!("Failed to load map: {error}");
            match request {
                LoadDocumentRequest::Open(_) => {
                    file_browser.error_banner = Some(message.clone());
                    file_browser.is_open = true;
                }
                LoadDocumentRequest::Replace(_) => {
                    new_map_modal.error_banner = Some(message.clone());
                    new_map_modal.is_open = true;
                }
            }
            tool_state.last_status_message = Some(message);
            return;
        }
    };

    let (width, height) = (candidate.doc.width, candidate.doc.height);
    tool_state.last_status_message = Some(candidate.path.as_ref().map_or_else(
        || "Created new blank map".to_string(),
        |path| format!("Opened {}", path.display()),
    ));
    *doc = candidate;
    history.clear();
    tool_state.stroke_recorder.resize(width, height);
    tool_state.is_painting = false;
    tool_state.box_start_cell = None;
    tool_state.ramp_start_cell = None;
    tool_state.entity_drag = None;
    tool_state.selected_entity_id = None;
    tool_state.hovered_cell = None;
    los_state.resize(width, height);
    los_state.last_scout_cell = None;
    los_state.last_revision = u64::MAX;
    focus_events.send(EditorCameraFocus {
        cell: (width / 2, height / 2),
    });
}
