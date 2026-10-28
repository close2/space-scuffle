//! Prepares a separate sparse-player verification map through the real versioned
//! map/scenario serializers. Only declared owner slots and the map reference may
//! differ; the complete decoded document and scenario are checked after reversing
//! that remap. Shipped assets are never modified.

use crate::runtime_battle::{
    BattleDefinition, BattleInitialOrder, BattleLoadError, DEFAULT_BATTLE_SCENARIO,
};
use sim_map::{MapFile, MapFormatError};
use std::path::{Path, PathBuf};

/// Exact source/output paths used by subsequent native capture and external hashes.
#[derive(Debug)]
pub struct SparseFixture {
    /// Original shipped map path.
    pub source_map: PathBuf,
    /// Original scenario path.
    pub source_scenario: PathBuf,
    /// New versioned map, with player0→2 and player1→7 start slots only.
    pub map: PathBuf,
    /// New scenario referencing the generated map and remapped player/orders.
    pub scenario: PathBuf,
}

/// Typed preparation failures; no partial fixture is accepted as a native input.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    /// The real production input/output bundle did not load successfully.
    #[error(transparent)]
    Battle(#[from] BattleLoadError),
    /// Real versioned map decoding/encoding failed.
    #[error(transparent)]
    Map(#[from] MapFormatError),
    /// Scenario serialization failed.
    #[error(transparent)]
    Serialize(#[from] ron::Error),
    /// An exact filesystem operation failed.
    #[error("ownership fixture I/O at {path}: {source}")]
    Io {
        /// Exact attempted output path.
        path: PathBuf,
        /// Original filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// Source identities were unexpected or unrelated data changed during round trip.
    #[error("ownership fixture invariant: {0}")]
    Invariant(&'static str),
}

fn remap(slot: u8, inverse: bool) -> Result<u8, FixtureError> {
    match (inverse, slot) {
        (false, 0) | (true, 2) => Ok(if inverse { 0 } else { 2 }),
        (false, 1) | (true, 7) => Ok(if inverse { 1 } else { 7 }),
        _ => Err(FixtureError::Invariant(
            "fixture must have exactly the declared two owner slots",
        )),
    }
}

fn remap_scenario(
    scenario: &mut crate::runtime_battle::BattleScenarioFile,
    inverse: bool,
) -> Result<(), FixtureError> {
    for player in &mut scenario.players {
        player.player_slot = remap(player.player_slot, inverse)?;
    }
    for order in &mut scenario.initial_orders {
        match order {
            BattleInitialOrder::AttackMoveToStart {
                player_slot,
                target_player_slot,
            } => {
                *player_slot = remap(*player_slot, inverse)?;
                *target_player_slot = remap(*target_player_slot, inverse)?;
            }
        }
    }
    Ok(())
}

/// Creates a fresh sparse-owner fixture from the real default production battle.
/// `output` must not exist. Successful return means both generated files were
/// loaded through production APIs and full inverse-remap equality was verified.
/// Input/output SHA inventories are recorded by the native evidence controller.
/// # Errors
/// Rejects existing directories, unexpected participants, load errors or any change
/// outside the declared start/player/order ownership and scenario map reference.
pub fn prepare(assets_root: &Path, output: &Path) -> Result<SparseFixture, FixtureError> {
    let original = BattleDefinition::load(assets_root, DEFAULT_BATTLE_SCENARIO.as_ref())?;
    if original.scenario.players.len() != 2 || original.document.start_locations.len() != 2 {
        return Err(FixtureError::Invariant(
            "fixture requires two real starts and participants",
        ));
    }
    std::fs::create_dir(output).map_err(|source| FixtureError::Io {
        path: output.into(),
        source,
    })?;
    let output = output.canonicalize().map_err(|source| FixtureError::Io {
        path: output.into(),
        source,
    })?;
    let map = output.join("sparse-map.scmap.ron");
    let scenario = output.join("sparse-battle.ron");
    let mut document = original.document.clone();
    for start in &mut document.start_locations {
        start.player_slot = remap(start.player_slot, false)?;
    }
    document.encode().save_atomic(&map)?;
    let mut modified = original.scenario.clone();
    modified.map.clone_from(&map);
    remap_scenario(&mut modified, false)?;
    let text = ron::ser::to_string_pretty(&modified, ron::ser::PrettyConfig::default())?;
    std::fs::write(&scenario, text).map_err(|source| FixtureError::Io {
        path: scenario.clone(),
        source,
    })?;
    let reloaded = BattleDefinition::load(assets_root, &scenario)?;
    let mut reversed_map = MapFile::load(&map)?.decode()?;
    for start in &mut reversed_map.start_locations {
        start.player_slot = remap(start.player_slot, true)?;
    }
    if reversed_map != original.document {
        return Err(FixtureError::Invariant("nonownership map data changed"));
    }
    let mut reversed_scenario = reloaded.scenario;
    reversed_scenario.map.clone_from(&original.scenario.map);
    remap_scenario(&mut reversed_scenario, true)?;
    if reversed_scenario != original.scenario {
        return Err(FixtureError::Invariant("unrelated scenario data changed"));
    }
    Ok(SparseFixture {
        source_map: assets_root.join(&original.scenario.map),
        source_scenario: original.source,
        map,
        scenario,
    })
}
