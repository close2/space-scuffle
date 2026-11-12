//! Declarative live-game initialization and presentation of the same map the simulation owns.
//!
//! The filesystem boundary prepares complete typed sessions before ECS replacement.
//! Reset uses the selected scenario and injected catalog bundle; no flat sandbox or
//! hardcoded creature formation can substitute for an invalid authored battle.

mod environment;
mod scene;
mod verification;

pub use environment::SceneEnvironmentPlugin;

pub use scene::{
    manage_battle_scene, sync_scene_camera, update_map_presentation, BattleSceneRoot, MapObject,
    MapObjectSnapshot, MapPresentation,
};
pub use verification::{BattleRenderStatus, BattleRenderVerificationPlugin};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use asset_baker::{TerrainTheme, TerrainThemeCatalog, TerrainThemeError};
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use sim_core::catalog::{CatalogLoadError, RuntimeCatalogs};
use sim_core::scenario::{
    apply_initial_orders, build_world_with_catalogs, start_approaches, PlayerSetup, ScenarioError,
    StartLoadoutCatalog,
};
use sim_core::SimWorld;
use sim_map::{MapDocument, MapFile, MapFormatError};
use sim_types::{PlayerId, SimVec2};

/// Default declarative game session, resolved beneath the validated assets root.
pub const DEFAULT_BATTLE_SCENARIO: &str = "scenarios/default_battle.ron";

/// The playable skirmish, resolved beneath the validated assets root.
pub const SKIRMISH_SCENARIO: &str = "scenarios/skirmish.ron";

/// Versioned runtime scenario selecting a map, players and initial game orders.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct BattleScenarioFile {
    /// Scenario schema version; currently one.
    pub format_version: u32,
    /// Map path relative to the validated assets root, or an explicit absolute path.
    pub map: PathBuf,
    /// Explicit deterministic seed used for each reset of this session.
    pub seed: u64,
    /// Player slots and declarative starting-loadout identities.
    pub players: Vec<PlayerSetup>,
    /// Initial commands applied only after all authored entities are placed.
    pub initial_orders: Vec<BattleInitialOrder>,
}

/// Capability-driven starting orders with map-relative destinations, shared with the simulation's
/// scenario loader.
pub use sim_core::scenario::InitialOrder as BattleInitialOrder;

/// A fully loaded immutable battle definition and its external rule snapshot.
#[derive(Resource)]
pub struct BattleDefinition {
    /// Absolute source path reported by diagnostics and render verification.
    pub source: PathBuf,
    /// Versioned, validated runtime configuration.
    pub scenario: BattleScenarioFile,
    /// Map document shared by simulation construction and map presentation.
    pub document: MapDocument,
    /// External units, structures and upgrades supplied to every reset.
    pub catalogs: RuntimeCatalogs,
    /// Starting formations loaded from the external loadout catalog.
    pub loadouts: StartLoadoutCatalog,
    /// Terrain theme selected by the map metadata.
    pub theme: TerrainTheme,
    /// Authored map-object sprite definitions used by the game scene.
    pub visuals: MapVisualCatalog,
    /// Validated ownership palette prepared from the same external visual catalog.
    pub palette: crate::player_presentation::PlayerPalette,
}

/// Authored mineral, gas and observation-tower presentation descriptors.
#[derive(Debug, Clone, Deserialize)]
pub struct MapVisualCatalog {
    /// Mineral cluster image and world size.
    pub mineral: MapVisualEntry,
    /// Gas vent image and world size.
    pub gas: MapVisualEntry,
    /// Observation tower image and world size.
    pub watchtower: MapVisualEntry,
}

/// A real authored sprite referenced by the map-object catalog.
#[derive(Debug, Clone, Deserialize)]
pub struct MapVisualEntry {
    /// Asset path relative to the shared Bevy assets root.
    pub sprite: String,
    /// Height/width of the billboard in world units.
    pub size: f32,
}

/// Typed failure preparing a live game; invalid input never silently substitutes a scene.
#[derive(Debug, thiserror::Error)]
pub enum BattleLoadError {
    /// Ownership colors or preview roles are invalid; no fallback palette is used.
    #[error(transparent)]
    Presentation(#[from] crate::player_presentation::PresentationError),
    /// An explicitly selected runtime file could not be read.
    #[error("cannot read battle asset {path}: {source}")]
    Io {
        /// Exact selected path.
        path: PathBuf,
        /// Original filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// An authored runtime file has invalid RON syntax or fields.
    #[error("invalid battle asset {path}: {source}")]
    Ron {
        /// Exact selected path.
        path: PathBuf,
        /// Original parse diagnostic.
        #[source]
        source: ron::error::SpannedError,
    },
    /// Scenario version is not supported by this runtime.
    #[error("unsupported battle scenario version {0}")]
    Version(u32),
    /// Player setup contains an invalid or repeated slot.
    #[error("invalid or duplicate battle player slot {0}")]
    PlayerSlot(u8),
    /// An initial command references an absent player or has no legal base approach.
    #[error("battle start has no legal approach for player slot {0}")]
    Start(u8),
    /// An attack command has no mobile armed units to command.
    #[error("battle player slot {0} has no mobile combatants for its initial order")]
    NoCombatants(u8),
    /// The map's requested theme is absent from the external catalog.
    #[error("map requests unknown terrain theme {0}")]
    Theme(String),
    /// A map-object sprite is invalid, missing or cannot be decoded.
    #[error("invalid map visual {path}: {reason}")]
    Visual {
        /// Authored asset path.
        path: PathBuf,
        /// Specific invalid property or decoder error.
        reason: String,
    },
    /// Real external unit or upgrade definitions could not be loaded.
    #[error(transparent)]
    Catalog(#[from] CatalogLoadError),
    /// Real map loading or decoding failed.
    #[error(transparent)]
    Map(#[from] MapFormatError),
    /// Map validation or deterministic starting placement failed.
    #[error(transparent)]
    Scenario(#[from] ScenarioError),
    /// Real terrain theme loading failed.
    #[error(transparent)]
    Terrain(#[from] TerrainThemeError),
}

impl BattleDefinition {
    /// Loads a complete session from real files beneath `assets_root`.
    ///
    /// # Errors
    /// Returns a typed path, map, catalog, visual or scenario validation failure.
    pub fn load(assets_root: &Path, scenario_path: &Path) -> Result<Self, BattleLoadError> {
        let source = assets_root.join(scenario_path);
        let scenario: BattleScenarioFile = read_ron(&source)?;
        if scenario.format_version != 1 {
            return Err(BattleLoadError::Version(scenario.format_version));
        }
        let mut slots = BTreeSet::new();
        for player in &scenario.players {
            if player.player_slot >= 8 || !slots.insert(player.player_slot) {
                return Err(BattleLoadError::PlayerSlot(player.player_slot));
            }
        }
        let document = MapFile::load(&assets_root.join(&scenario.map))?.decode()?;
        let catalogs = RuntimeCatalogs::load_from_directory(assets_root.join("catalog"))?;
        let loadouts = StartLoadoutCatalog::load(assets_root.join("catalog/start_loadouts.ron"))?;
        let themes = TerrainThemeCatalog::load(&assets_root.join("catalog/terrain_themes.ron"))?;
        let theme = themes
            .get(&document.metadata.theme)
            .ok_or_else(|| BattleLoadError::Theme(document.metadata.theme.clone()))?
            .clone();
        let visuals: MapVisualCatalog =
            read_ron(&assets_root.join("catalog/map_entity_visuals.ron"))?;
        let palette = crate::player_presentation::PlayerPalette::load(
            assets_root,
            catalogs
                .units
                .archetypes()
                .filter(|a| a.showcase_hero)
                .count(),
        )?;
        for visual in [&visuals.mineral, &visuals.gas, &visuals.watchtower] {
            let path = assets_root.join(&visual.sprite);
            if !visual.size.is_finite() || visual.size <= 0.0 {
                return Err(BattleLoadError::Visual {
                    path,
                    reason: "size must be finite and positive".into(),
                });
            }
            image::open(&path).map_err(|error| BattleLoadError::Visual {
                path,
                reason: error.to_string(),
            })?;
        }
        let definition = Self {
            source,
            scenario,
            document,
            catalogs,
            loadouts,
            theme,
            visuals,
            palette,
        };
        // Validate placements and starting orders before the application mutates ECS.
        definition.instantiate()?;
        Ok(definition)
    }

    /// Builds the deterministic starting world and applies its declarative initial orders through
    /// [`apply_initial_orders`], the code the route trace tool also starts battles with.
    ///
    /// # Errors
    /// Returns a typed scenario placement, missing-start or capability failure.
    pub fn instantiate(&self) -> Result<SimWorld, BattleLoadError> {
        let mut world = build_world_with_catalogs(
            &self.document,
            &self.loadouts,
            &self.scenario.players,
            self.scenario.seed,
            self.catalogs.units.clone(),
            self.catalogs.upgrades.clone(),
        )?;
        apply_initial_orders(
            &mut world,
            &self.document,
            &self.scenario.players,
            &self.scenario.initial_orders,
        )
        .map_err(BattleLoadError::from_order_error)?;
        Ok(world)
    }

    /// Legal map-relative base approach positions for runtime commands and camera focus
    /// ([`start_approaches`]).
    ///
    /// # Errors
    /// Returns [`BattleLoadError::Start`] if a selected base has no free perimeter cell.
    pub fn start_approaches(
        &self,
        world: &SimWorld,
    ) -> Result<BTreeMap<PlayerId, SimVec2>, BattleLoadError> {
        start_approaches(&self.document, &self.scenario.players, world)
            .map_err(BattleLoadError::from_order_error)
    }
}

impl BattleLoadError {
    /// Names a player's missing start or missing combatants as this error's own variants, so a
    /// caller can tell them from other scenario failures.
    fn from_order_error(error: ScenarioError) -> Self {
        match error {
            ScenarioError::NoStartApproach { slot } => Self::Start(slot),
            ScenarioError::NoCombatants { slot } => Self::NoCombatants(slot),
            other => Self::Scenario(other),
        }
    }
}

fn read_ron<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, BattleLoadError> {
    let source = std::fs::read_to_string(path).map_err(|source| BattleLoadError::Io {
        path: path.to_owned(),
        source,
    })?;
    ron::from_str(&source).map_err(|source| BattleLoadError::Ron {
        path: path.to_owned(),
        source,
    })
}
