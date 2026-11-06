//! Behaviour of the test fixtures: the sandbox scene and the embedded shipped data.
//!
//! # Responsibility
//! Pins that the battle sandbox is built whole or not at all, that its scene is unchanged from
//! the constructor it replaced, and that the embedded catalogs are the files the game loads.
//!
//! # How it fits
//! Every other crate's tests build worlds through `sim_fixtures`; these tests make sure those
//! worlds are what the shipped data and the former `sim_core` constructors produced.
//!
//! # Invariants
//! - The sandbox reports a spawn it cannot make; it never returns a world with entities missing.
//! - The embedded catalogs equal `RuntimeCatalogs::load_from_directory("assets/catalog")`.

use sim_core::catalog::RuntimeCatalogs;
use sim_fixtures::{default_battle_sandbox, default_battle_sandbox_on, SandboxError};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{PlayerId, ShapeKind};
use std::error::Error;
use std::path::PathBuf;

/// Checksum of the default sandbox's spawned state, recorded from
/// `SimWorld::new_default_battle_sandbox` before Phase 9 WP10 moved it here. Re-recorded for
/// checksum schema 10 (plan 2026-11-04-battle-flipbook-animation WP2), which stopped encoding
/// each entity's constant-zero `anim_progress`; the spawned state itself did not change.
/// Re-recorded for plan 2026-11-04-combat-sc2-alignment WP9: a new entity's hashed
/// `ticks_since_last_damage` starts at `u16::MAX` instead of 250; nothing else changed.
/// Re-recorded for checksum schema 11 (plan 2026-11-05-sc2-deviation-fixes WP4, B-11), which
/// encodes each entity's `previous_target_position`, `None` at spawn.
/// Re-recorded for checksum schema 12 (plan 2026-11-05-sc2-deviation-fixes WP5, B-2), which
/// encodes each weapon's strike spacing and each entity's `pending_strikes`, `None` at spawn.
/// Re-recorded for checksum schema 13 (WP6, B-14), which encodes each weapon's bonus against
/// every attribute in place of its Armored and Light bonuses.
/// Re-recorded for checksum schema 14 (work package 8a, B-16), which encodes each splash as its
/// list of zones in place of three fixed radii.
/// Re-recorded for checksum schema 15 (work package 8b, B-18), which encodes each splash's shape.
const DEFAULT_SANDBOX_CHECKSUM: u64 = 9_874_857_617_800_651_092;

fn shipped_catalog_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog")
}

#[test]
fn sandbox_on_a_map_too_small_for_its_scene_is_an_error() -> Result<(), Box<dyn Error>> {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let result = default_battle_sandbox_on(map);
    assert!(
        matches!(result, Err(SandboxError::Spawn { .. })),
        "a scene whose entities lie outside a 32x32 map must fail, got {:?}",
        result.map(|world| world.entities.len())
    );
    Ok(())
}

#[test]
fn default_sandbox_spawns_both_armies_and_bases_whole() -> Result<(), Box<dyn Error>> {
    let world = default_battle_sandbox()?;
    let count = |owner: PlayerId, structure: bool| {
        world
            .entities
            .values()
            .filter(|e| e.owner == owner)
            .filter(|e| {
                world
                    .catalog
                    .get(e.unit_type_id)
                    .is_ok_and(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }) == structure)
            })
            .count()
    };
    assert_eq!(
        count(PlayerId(0), false),
        12,
        "ten ranged and two armoured units"
    );
    assert_eq!(
        count(PlayerId(1), false),
        10,
        "eight melee and two turreted units"
    );
    assert_eq!(
        count(PlayerId(0), true),
        3,
        "command, supply and production"
    );
    assert_eq!(
        count(PlayerId(1), true),
        3,
        "command, supply and production"
    );
    assert_eq!(world.last_checksum(), world.compute_checksum());
    assert_eq!(world.last_checksum(), DEFAULT_SANDBOX_CHECKSUM);
    Ok(())
}

#[test]
fn embedded_catalogs_are_the_shipped_files() -> Result<(), Box<dyn Error>> {
    let embedded = sim_fixtures::shipped_catalogs();
    let on_disk = RuntimeCatalogs::load_from_directory(shipped_catalog_dir())?;
    let slugs = |catalogs: &RuntimeCatalogs| {
        catalogs
            .units
            .archetypes()
            .map(|a| (a.unit_type_id, a.slug.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(slugs(&embedded), slugs(&on_disk));
    assert_eq!(
        embedded
            .upgrades
            .upgrades()
            .map(|u| u.upgrade_id)
            .collect::<Vec<_>>(),
        on_disk
            .upgrades
            .upgrades()
            .map(|u| u.upgrade_id)
            .collect::<Vec<_>>()
    );
    let loadouts = sim_core::scenario::StartLoadoutCatalog::load(
        shipped_catalog_dir().join("start_loadouts.ron"),
    )?;
    assert_eq!(sim_fixtures::shipped_loadouts(), loadouts);
    Ok(())
}
