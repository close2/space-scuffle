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
    MapPresentation,
};
pub use verification::{BattleRenderStatus, BattleRenderVerificationPlugin};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use asset_baker::{TerrainTheme, TerrainThemeCatalog, TerrainThemeError};
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use sim_core::catalog::{CatalogLoadError, RuntimeCatalogs};
use sim_core::scenario::{
    build_world_with_catalogs, ring_perimeter, PlayerSetup, ScenarioError, StartLoadoutCatalog,
};
use sim_core::SimWorld;
use sim_map::{CellFlags, MapDocument, MapFile, MapFormatError};
use sim_types::{PlayerId, SimScalar, SimVec2, UnitOrder};

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

/// Capability-driven starting orders with map-relative destinations.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum BattleInitialOrder {
    /// Mobile armed combatants attack toward a legal approach to another player's base.
    AttackMoveToStart {
        /// Owner whose combat-capable mobile units receive the order.
        player_slot: u8,
        /// Owner of the destination start location.
        target_player_slot: u8,
    },
}

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

    /// Builds the deterministic starting world and applies its declarative initial orders.
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
        let approaches = self.start_approaches(&world)?;
        for order in &self.scenario.initial_orders {
            match *order {
                BattleInitialOrder::AttackMoveToStart {
                    player_slot,
                    target_player_slot,
                } => {
                    if !approaches.contains_key(&PlayerId(player_slot)) {
                        return Err(BattleLoadError::Start(player_slot));
                    }
                    let target = *approaches
                        .get(&PlayerId(target_player_slot))
                        .ok_or(BattleLoadError::Start(target_player_slot))?;
                    let mut combatants = 0;
                    for entity in world.entities.values_mut() {
                        let archetype = world
                            .catalog
                            .get(entity.unit_type_id)
                            .map_err(ScenarioError::from)?;
                        if entity.owner == PlayerId(player_slot)
                            && !entity.is_immobile
                            && archetype.capabilities.harvester.is_none()
                            && (archetype.ground_weapon.is_some()
                                || archetype.air_weapon.is_some()
                                || archetype.weapon.is_some())
                        {
                            entity
                                .issue_order(UnitOrder::AttackMove { target }, false)
                                .map_err(sim_core::SimCoreError::from)
                                .map_err(ScenarioError::from)?;
                            combatants += 1;
                        }
                    }
                    if combatants == 0 {
                        return Err(BattleLoadError::NoCombatants(player_slot));
                    }
                }
            }
        }
        world.last_checksum = world.compute_checksum();
        Ok(world)
    }

    /// Legal map-relative base approach positions for runtime commands and camera focus.
    ///
    /// # Errors
    /// Returns [`BattleLoadError::Start`] if a selected base has no free perimeter cell.
    pub fn start_approaches(
        &self,
        world: &SimWorld,
    ) -> Result<BTreeMap<PlayerId, SimVec2>, BattleLoadError> {
        let mut positions = BTreeMap::new();
        for player in &self.scenario.players {
            let start = self
                .document
                .start_locations
                .iter()
                .find(|start| start.player_slot == player.player_slot)
                .ok_or(BattleLoadError::Start(player.player_slot))?;
            let x = i32::try_from(start.cell.0)
                .map_err(|_| BattleLoadError::Start(player.player_slot))?;
            let y = i32::try_from(start.cell.1)
                .map_err(|_| BattleLoadError::Start(player.player_slot))?;
            let position = ring_perimeter(
                x - 1,
                x + i32::from(start.footprint.0),
                y - 1,
                y + i32::from(start.footprint.1),
            )
            .find_map(|(cx, cy)| {
                let (Ok(cx), Ok(cy)) = (u32::try_from(cx), u32::try_from(cy)) else {
                    return None;
                };
                let cell = world.map.cell(cx, cy).ok()?;
                (cell.flags.contains(CellFlags::GROUND_PATHABLE)
                    && !cell
                        .flags
                        .intersects(CellFlags::OCCUPIED_BY_STRUCTURE | CellFlags::UNPASSABLE_CLIFF)
                    && cell.ramp.is_none())
                .then(|| {
                    SimVec2::new(
                        SimScalar::from_num(cx) + SimScalar::HALF,
                        SimScalar::from_num(cy) + SimScalar::HALF,
                    )
                })
            })
            .ok_or(BattleLoadError::Start(player.player_slot))?;
            positions.insert(PlayerId(player.player_slot), position);
        }
        Ok(positions)
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
