//! Test-only fixtures built from the shipped data.
//!
//! # Responsibility
//! Gives tests, benches of other crates and diagnostics tests the worlds and catalogs that the
//! simulation used to construct from data compiled into `sim_core`: the shipped faction, unit,
//! structure and upgrade catalogs, the shipped start loadouts, a world on any map with those catalogs, a
//! scenario world, the fixed battle sandbox, the "who wins" matchup matrix ([`matchup`]) and the
//! battle trace tool of the battle 33 investigation ([`trace`]). The files under `assets/catalog/` are embedded
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

pub mod matchup;
mod sandbox;
pub mod trace;

pub use sandbox::{default_battle_sandbox, default_battle_sandbox_on, SandboxError};

use sim_core::catalog::{FactionCatalog, MovementRules, RuntimeCatalogs, SpellCatalog};
use sim_core::scenario::{
    build_world_with_catalogs, PlayerSetup, ScenarioError, StartLoadoutCatalog,
};
use sim_core::{SimWorld, UnitCatalog, UpgradeCatalog};
use sim_map::{MapDocument, SimMap};

/// The shipped `assets/catalog/factions.ron`.
pub const FACTIONS_RON: &str = include_str!("../../../assets/catalog/factions.ron");
/// The shipped `assets/catalog/units.ron`.
pub const UNITS_RON: &str = include_str!("../../../assets/catalog/units.ron");
/// The shipped `assets/catalog/structures.ron`.
pub const STRUCTURES_RON: &str = include_str!("../../../assets/catalog/structures.ron");
/// The shipped `assets/catalog/upgrades.ron`.
pub const UPGRADES_RON: &str = include_str!("../../../assets/catalog/upgrades.ron");
/// The shipped `assets/catalog/movement_rules.ron`.
pub const MOVEMENT_RULES_RON: &str = include_str!("../../../assets/catalog/movement_rules.ron");
/// The shipped `assets/catalog/statuses.ron`.
pub const STATUSES_RON: &str = include_str!("../../../assets/catalog/statuses.ron");
/// The shipped `assets/catalog/ability_effects.ron`.
pub const ABILITY_EFFECTS_RON: &str = include_str!("../../../assets/catalog/ability_effects.ron");
/// The shipped `assets/catalog/start_loadouts.ron`.
pub const START_LOADOUTS_RON: &str = include_str!("../../../assets/catalog/start_loadouts.ron");

/// The shipped faction, weapon-class and armor-class identities.
///
/// # Panics
/// Panics if the shipped `factions.ron` does not parse or is inconsistent.
#[must_use]
pub fn shipped_factions() -> FactionCatalog {
    FactionCatalog::from_ron_str(FACTIONS_RON)
        .expect("invariant: the shipped factions.ron is valid")
}

/// The shipped unit, structure and upgrade catalogs, validated exactly as
/// [`RuntimeCatalogs::load_from_directory`] validates them on disk.
///
/// # Panics
/// Panics if the shipped catalog files do not parse or reference each other inconsistently.
#[must_use]
pub fn shipped_catalogs() -> RuntimeCatalogs {
    let factions = shipped_factions();
    let mut units = UnitCatalog::from_ron_str_bundle(&factions, UNITS_RON, STRUCTURES_RON)
        .expect("invariant: the shipped units.ron and structures.ron are valid");
    let upgrades = UpgradeCatalog::from_ron_str(&factions, UPGRADES_RON)
        .expect("invariant: the shipped upgrades.ron is valid");
    units
        .validate_upgrade_references(&upgrades)
        .expect("invariant: the shipped unit catalog names only shipped upgrades");
    units.set_movement_rules(
        MovementRules::from_ron_str(MOVEMENT_RULES_RON)
            .expect("invariant: the shipped movement_rules.ron is valid"),
    );
    units
        .set_spell_catalog(
            SpellCatalog::from_ron_strs(STATUSES_RON, ABILITY_EFFECTS_RON)
                .expect("invariant: the shipped statuses.ron and ability_effects.ron are valid"),
        )
        .expect("invariant: the shipped abilities name only shipped effects and statuses");
    units
        .validate_weapon_hit_effects(&upgrades)
        .expect("invariant: the shipped upgrades name only shipped weapon hit effects");
    RuntimeCatalogs { units, upgrades }
}

/// The shipped spell tables with the scratch statuses and effects of `statuses_ron` and
/// `effects_ron` (each a whole `statuses.ron` or `ability_effects.ron` text) appended, flattened
/// and validated as one catalog.
///
/// A test installs it with `UnitCatalog::set_spell_catalog`, which requires every shipped
/// ability's effect to resolve: the shipped tables stay in, so the scratch entries must use
/// names of their own.
///
/// # Errors
/// The scratch text's parse error, or the first rule of the merged catalog it violates.
///
/// # Panics
/// Panics if the shipped `statuses.ron` or `ability_effects.ron` does not parse.
pub fn spells_with(
    statuses_ron: &str,
    effects_ron: &str,
) -> Result<SpellCatalog, sim_core::SimCoreError> {
    let mut statuses = SpellCatalog::parse_statuses(STATUSES_RON)
        .expect("invariant: the shipped statuses.ron parses");
    let mut effects = SpellCatalog::parse_effects(ABILITY_EFFECTS_RON)
        .expect("invariant: the shipped ability_effects.ron parses");
    statuses.extend(SpellCatalog::parse_statuses(statuses_ron)?);
    effects.extend(SpellCatalog::parse_effects(effects_ron)?);
    Ok(SpellCatalog::from_defs(statuses, effects)?)
}

/// Removes the random firing delay (plan `2026-11-05-sc2-deviation-fixes` WP11, B-3) from every
/// weapon of `catalog`, mode overrides included, for tests that pin a weapon's exact period,
/// which the delay spreads by a few ticks a shot.
///
/// # Panics
/// Never: every id it looks up comes from the catalog's own archetypes.
pub fn without_random_delays(catalog: &mut UnitCatalog) {
    let ids: Vec<_> = catalog.archetypes().map(|a| a.unit_type_id).collect();
    for id in ids {
        let arch = catalog
            .get_mut(id)
            .expect("invariant: the id was read from this catalog");
        let modes = arch
            .modes
            .iter_mut()
            .filter_map(|mode| mode.ground_weapon_override.as_mut()?.as_mut());
        let blocks = [
            &mut arch.weapon,
            &mut arch.ground_weapon,
            &mut arch.air_weapon,
        ];
        for weapon in blocks.into_iter().flatten().chain(modes) {
            weapon.random_delay_min = sim_types::SimScalar::ZERO;
            weapon.random_delay_max = sim_types::SimScalar::ZERO;
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use sim_map::{CliffLevel, GameMap, MapCell};

    #[test]
    fn shipped_catalogs_resolve_every_slug_back_to_its_archetype() {
        let catalogs = shipped_catalogs();
        let mut count = 0_usize;
        for archetype in catalogs.units.archetypes() {
            count += 1;
            assert_eq!(
                catalogs.units.id_by_slug(&archetype.slug),
                Some(archetype.unit_type_id),
                "slug {} must resolve to its own archetype",
                archetype.slug
            );
        }
        assert!(count > 0, "the shipped catalog defines no archetype");
    }

    #[test]
    fn world_starts_empty_at_tick_zero_with_the_requested_seed(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?;
        let built = world(map, 42);
        assert!(built.entities.is_empty(), "a fixture world spawns nothing");
        assert_eq!(built.current_tick, 0);
        assert_eq!(built.rng_seed, 42);
        Ok(())
    }
}
