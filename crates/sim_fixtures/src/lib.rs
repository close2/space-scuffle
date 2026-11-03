//! Test-only fixtures built from the shipped data.
//!
//! # Responsibility
//! Gives tests, benches of other crates and diagnostics tests the worlds and catalogs that the
//! simulation used to construct from data compiled into `sim_core`: the shipped unit, structure
//! and upgrade catalogs, the shipped start loadouts, a world on any map with those catalogs, a
//! scenario world, and the fixed battle sandbox. The files under `assets/catalog/` are embedded
//! with `include_str!`, so the golden replay runs under wasmtime without filesystem access.
//!
//! # How it fits
//! Production code never embeds the catalogs (charter §3.3, check C6): the client, the asset
//! baker and the benches load them from disk at run time through
//! [`sim_core::catalog::RuntimeCatalogs::load_from_directory`]. This crate is only ever a
//! dev-dependency, which check C1 verifies on the Cargo graph. `sim_core`'s own unit tests
//! cannot use it (that would link a second copy of `sim_core`), so they have the equivalent
//! `sim_core::test_support` module.
//!
//! # Invariants
//! - Every function builds from the same bytes as the files the game loads, so a test that
//!   passes here passes against the shipped data.
//! - A shipped catalog that fails to parse panics the calling test with an `invariant:` message;
//!   it is a broken commit, never a recoverable condition.
#![expect(
    clippy::expect_used,
    reason = "test support: a broken shipped catalog must fail the calling test loudly"
)]

mod sandbox;

pub use sandbox::{default_battle_sandbox, default_battle_sandbox_on, SandboxError};

use sim_core::catalog::RuntimeCatalogs;
use sim_core::scenario::{
    build_world_with_catalogs, PlayerSetup, ScenarioError, StartLoadoutCatalog,
};
use sim_core::{SimWorld, UnitCatalog, UpgradeCatalog};
use sim_map::{MapDocument, SimMap};

/// The shipped `assets/catalog/units.ron`.
pub const UNITS_RON: &str = include_str!("../../../assets/catalog/units.ron");
/// The shipped `assets/catalog/structures.ron`.
pub const STRUCTURES_RON: &str = include_str!("../../../assets/catalog/structures.ron");
/// The shipped `assets/catalog/upgrades.ron`.
pub const UPGRADES_RON: &str = include_str!("../../../assets/catalog/upgrades.ron");
/// The shipped `assets/catalog/start_loadouts.ron`.
pub const START_LOADOUTS_RON: &str = include_str!("../../../assets/catalog/start_loadouts.ron");

/// The shipped unit, structure and upgrade catalogs, validated exactly as
/// [`RuntimeCatalogs::load_from_directory`] validates them on disk.
///
/// # Panics
/// Panics if the shipped catalog files do not parse or reference each other inconsistently.
#[must_use]
pub fn shipped_catalogs() -> RuntimeCatalogs {
    let units = UnitCatalog::from_ron_str_bundle(UNITS_RON, STRUCTURES_RON)
        .expect("invariant: the shipped units.ron and structures.ron are valid");
    let upgrades = UpgradeCatalog::from_ron_str(UPGRADES_RON)
        .expect("invariant: the shipped upgrades.ron is valid");
    units
        .validate_upgrade_references(&upgrades)
        .expect("invariant: the shipped unit catalog names only shipped upgrades");
    RuntimeCatalogs { units, upgrades }
}

/// A world on `map` with the shipped catalogs and the deterministic RNG `seed`.
///
/// Replaces the former `SimWorld::new`; the world is identical to the one that built.
///
/// # Panics
/// Panics if the shipped catalogs are invalid (see [`shipped_catalogs`]).
#[must_use]
pub fn world(map: SimMap, seed: u64) -> SimWorld {
    let catalogs = shipped_catalogs();
    SimWorld::with_catalogs(map, seed, catalogs.units, catalogs.upgrades)
}

/// The shipped start loadouts.
///
/// # Panics
/// Panics if the shipped `start_loadouts.ron` does not parse.
#[must_use]
pub fn shipped_loadouts() -> StartLoadoutCatalog {
    StartLoadoutCatalog::from_ron_str(START_LOADOUTS_RON)
        .expect("invariant: the shipped start_loadouts.ron is valid")
}

/// Builds the scenario world for `doc`, `loadouts` and `players` with the shipped catalogs.
///
/// Replaces the former `sim_core::scenario::build_world`: it is
/// [`build_world_with_catalogs`] with [`shipped_catalogs`].
///
/// # Errors
/// Returns every [`ScenarioError`] that [`build_world_with_catalogs`] returns.
///
/// # Panics
/// Panics if the shipped catalogs are invalid (see [`shipped_catalogs`]).
pub fn scenario_world(
    doc: &MapDocument,
    loadouts: &StartLoadoutCatalog,
    players: &[PlayerSetup],
    seed: u64,
) -> Result<SimWorld, ScenarioError> {
    let catalogs = shipped_catalogs();
    build_world_with_catalogs(
        doc,
        loadouts,
        players,
        seed,
        catalogs.units,
        catalogs.upgrades,
    )
}
