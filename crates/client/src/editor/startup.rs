//! Fallible map editor startup: catalogs, palette and the initial document.
//!
//! # Architectural Charter
//! In accordance with [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md) §1.1, this
//! module owns everything the editor must read from disk before a window exists. The binary loads
//! an [`EditorStartup`] first and reports a [`EditorStartupError`] on standard error; the
//! [`MapEditorPlugin`](crate::editor::MapEditorPlugin) then receives only validated data.
//!
//! # Invariants Maintained
//! 1. **No Panics on Bad Input:** Unreadable maps, malformed catalogs and out-of-range dimensions
//!    become typed errors, never a panic inside `Plugin::build`.
//! 2. **Authored Rules Only:** Playtests use the same external unit, structure and upgrade catalogs
//!    as the game client, so editor units resolve to their real media and ownership colors.
//! 3. **No Silent Substitution:** A map naming an unknown terrain theme is rejected rather than
//!    rendered with whichever theme happens to be listed first.

use std::path::{Path, PathBuf};

use asset_baker::{TerrainThemeCatalog, TerrainThemeError};
use sim_core::catalog::{CatalogLoadError, RuntimeCatalogs};
use sim_core::scenario::{ScenarioError, StartLoadoutCatalog};
use sim_map::map_format::{MapFile, MapFormatError, MapMetadata};

use crate::assets::{AssetsDirError, ResolvedAssetsRoot};
use crate::editor::document::EditorDocument;
use crate::editor::markers::MapEntityVisualCatalog;
use crate::player_presentation::{PlayerPalette, PresentationError};

/// Terrain theme assigned to maps created with `--new`.
const NEW_MAP_THEME: &str = "orbital_foundry";

/// Width and height in cells of the blank map used when no default map exists.
const FALLBACK_MAP_DIMENSION: u32 = 64;

/// Typed failure preparing the editor before its window opens.
#[derive(Debug, thiserror::Error)]
pub enum EditorStartupError {
    /// The shared `assets/` directory could not be located or validated.
    #[error(transparent)]
    Assets(#[from] AssetsDirError),
    /// The terrain theme catalog could not be read or parsed.
    #[error("cannot load terrain themes: {0}")]
    Themes(#[from] TerrainThemeError),
    /// The start loadout catalog could not be read or parsed.
    #[error("cannot load start loadouts: {0}")]
    Loadouts(#[from] ScenarioError),
    /// The external unit, structure or upgrade catalogs are invalid.
    #[error("cannot load rule catalogs: {0}")]
    Catalogs(#[from] CatalogLoadError),
    /// The map entity visual catalog could not be read or parsed.
    #[error("cannot load map entity visuals at {path}: {reason}")]
    Visuals {
        /// Exact attempted runtime path.
        path: PathBuf,
        /// Reader or parser diagnostic.
        reason: String,
    },
    /// The authored player palette is invalid.
    #[error("cannot load player palette: {0}")]
    Palette(#[from] PresentationError),
    /// The requested map could not be read, decoded, or created.
    #[error("cannot open map {path}: {source}")]
    Map {
        /// Exact attempted path; empty for a new blank map.
        path: PathBuf,
        /// Original map format failure.
        #[source]
        source: MapFormatError,
    },
    /// The map names a terrain theme absent from the catalog.
    #[error("map requests unknown terrain theme {0}")]
    UnknownTheme(String),
}

/// Document the editor opens on launch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InitialDocument {
    /// Open `assets/maps/showcase_canyon.scmap.ron`, or a blank map if it is absent.
    #[default]
    Default,
    /// Open the `.scmap.ron` file at this path.
    Open(PathBuf),
    /// Create an untitled blank map of `width × height` cells.
    Blank {
        /// Map width in cells.
        width: u32,
        /// Map height in cells.
        height: u32,
    },
}

/// Everything loaded from disk before the editor window opens.
#[derive(Clone)]
pub struct EditorStartup {
    /// Validated shared asset root.
    pub assets_root: ResolvedAssetsRoot,
    /// Available terrain themes.
    pub themes: TerrainThemeCatalog,
    /// Starting army compositions.
    pub loadouts: StartLoadoutCatalog,
    /// Map marker sprites.
    pub visuals: MapEntityVisualCatalog,
    /// External unit, structure and upgrade rules.
    pub catalogs: RuntimeCatalogs,
    /// Ownership colors for playtest participants.
    pub palette: PlayerPalette,
    /// The document to edit first.
    pub document: EditorDocument,
}

impl EditorStartup {
    /// Loads catalogs from the discovered asset root and resolves `initial`.
    ///
    /// # Errors
    /// Returns [`EditorStartupError`] when the asset root, any catalog or the map is invalid.
    pub fn load(initial: &InitialDocument) -> Result<Self, EditorStartupError> {
        Self::load_from(ResolvedAssetsRoot::resolve()?, initial)
    }

    /// Loads catalogs beneath `assets_root` and resolves `initial`.
    ///
    /// # Errors
    /// Returns [`EditorStartupError`] when any catalog or the map is invalid.
    pub fn load_from(
        assets_root: ResolvedAssetsRoot,
        initial: &InitialDocument,
    ) -> Result<Self, EditorStartupError> {
        let root = assets_root.as_path();
        let themes = TerrainThemeCatalog::load(&root.join("catalog/terrain_themes.ron"))?;
        let loadouts = StartLoadoutCatalog::load(root.join("catalog/start_loadouts.ron"))?;
        let catalogs = RuntimeCatalogs::load_from_directory(root.join("catalog"))?;

        let visuals_path = root.join("catalog/map_entity_visuals.ron");
        let visuals = MapEntityVisualCatalog::load(&visuals_path).map_err(|error| {
            EditorStartupError::Visuals {
                path: visuals_path.clone(),
                reason: error.to_string(),
            }
        })?;

        let showcase_count = catalogs
            .units
            .archetypes()
            .filter(|archetype| archetype.showcase_hero)
            .count();
        let palette = PlayerPalette::load(root, showcase_count)?;

        let document = resolve_document(initial, root)?;
        if themes.get(&document.doc.metadata.theme).is_none() {
            return Err(EditorStartupError::UnknownTheme(
                document.doc.metadata.theme.clone(),
            ));
        }

        Ok(Self {
            assets_root,
            themes,
            loadouts,
            visuals,
            catalogs,
            palette,
            document,
        })
    }
}

fn open_document(path: &Path) -> Result<EditorDocument, EditorStartupError> {
    let doc = MapFile::load(path)
        .and_then(|file| file.decode())
        .map_err(|source| EditorStartupError::Map {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(EditorDocument::new(doc, Some(path.to_path_buf())))
}

fn blank_document(
    width: u32,
    height: u32,
    metadata: MapMetadata,
) -> Result<EditorDocument, EditorStartupError> {
    EditorDocument::new_blank(width, height, metadata).map_err(|source| EditorStartupError::Map {
        path: PathBuf::new(),
        source,
    })
}

fn resolve_document(
    initial: &InitialDocument,
    root: &Path,
) -> Result<EditorDocument, EditorStartupError> {
    let new_map_metadata = || MapMetadata {
        theme: NEW_MAP_THEME.to_string(),
        ..MapMetadata::default()
    };
    match initial {
        InitialDocument::Open(path) => open_document(path),
        InitialDocument::Blank { width, height } => {
            blank_document(*width, *height, new_map_metadata())
        }
        InitialDocument::Default => {
            let default_path = root.join("maps/showcase_canyon.scmap.ron");
            if default_path.exists() {
                open_document(&default_path)
            } else {
                blank_document(
                    FALLBACK_MAP_DIMENSION,
                    FALLBACK_MAP_DIMENSION,
                    new_map_metadata(),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> Result<ResolvedAssetsRoot, AssetsDirError> {
        ResolvedAssetsRoot::resolve()
    }

    #[test]
    fn missing_map_is_a_typed_error_naming_the_path() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from("/nonexistent/editor_startup_probe.scmap.ron");
        let result = EditorStartup::load_from(root()?, &InitialDocument::Open(path.clone()));
        let Err(EditorStartupError::Map { path: reported, .. }) = result else {
            return Err("missing map must be a Map error".into());
        };
        assert_eq!(reported, path);
        Ok(())
    }

    #[test]
    fn out_of_range_blank_map_is_a_typed_error() -> Result<(), Box<dyn std::error::Error>> {
        let result = EditorStartup::load_from(
            root()?,
            &InitialDocument::Blank {
                width: 1,
                height: 1,
            },
        );
        assert!(matches!(result, Err(EditorStartupError::Map { .. })));
        Ok(())
    }

    #[test]
    fn unknown_theme_is_rejected_instead_of_substituted() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("unknown_theme.scmap.ron");
        let doc = sim_map::map_format::MapDocument::new_blank(
            32,
            32,
            MapMetadata {
                theme: "theme_absent_from_catalog".to_string(),
                ..MapMetadata::default()
            },
        )?;
        doc.encode().save_atomic(&path)?;
        let result = EditorStartup::load_from(root()?, &InitialDocument::Open(path));
        assert!(matches!(result, Err(EditorStartupError::UnknownTheme(_))));
        Ok(())
    }

    #[test]
    fn startup_loads_the_real_authored_rule_catalogs() -> Result<(), Box<dyn std::error::Error>> {
        let startup = EditorStartup::load_from(
            root()?,
            &InitialDocument::Blank {
                width: 32,
                height: 32,
            },
        )?;
        assert!(startup.catalogs.units.archetypes().next().is_some());
        assert_ne!(startup.loadouts.loadouts.len(), 0);
        Ok(())
    }
}
