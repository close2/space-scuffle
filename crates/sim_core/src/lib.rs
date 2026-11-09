//! # Sim Core: Deterministic RTS Simulation Engine
//!
//! `sim_core` implements the authoritative, deterministic simulation tick loop for
//! **Space Scuffle**. It models `StarCraft` II-grade RTS combat mechanics, including:
//! - 4-phase weapon state machines with animation backswing cancellation.
//! - Independent turret tracking decoupled from hull navigation.
//! - Shield absorption, armor reduction, spillover damage, and 10-second shield regen delays.
//! - Local steering with Boid separation, concave firing arc sliding, and push priority yielding.
//! - Dynamic cliff-aware line-of-sight target acquisition.
//!
//! ## Invariants Maintained:
//! 1. All calculations use deterministic fixed-point math (`SimScalar`, `SimVec2`, `SimAngle`).
//! 2. Entity iteration order is strictly sorted by `EntityId` to guarantee identical checksums.
//! 3. Simulation ticks run at an invariant rate of 25 Hz (40 ms per tick).
//! 4. 100% platform-independent determinism across `x86_64`, `aarch64`, and `wasm32`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(clippy::unreachable)]

pub mod catalog;
pub mod checksum;
pub mod combat;
pub(crate) mod combat_resolution;
pub mod economy;
pub mod entity;
pub mod error;
pub mod match_state;
pub mod movement;
pub mod player_view;
pub mod production;
pub mod published_actions;
pub mod scenario;
pub mod steering;
pub(crate) mod target_acquisition;
pub mod tech_tree;
pub mod transport;
pub mod upgrades;
pub mod weapon_timing;
pub mod world;

pub use catalog::{SplashSpec, SplashZone, UnitArchetype, UnitCatalog, WeaponSpec};
pub use checksum::compute_world_checksum;
pub use combat::{
    calculate_hit_damage, calculate_hit_damage_upgraded, calculate_splash_damage,
    calculate_splash_damage_upgraded, charge_random_delay, is_in_weapon_range, is_threat_to,
    is_within_firing_arc, step_entity_weapon, DamageResolution, TargetPriorityKey, WeaponTarget,
    MIN_DAMAGE_CLAMP, RELEASE_RANGE_SLOP,
};
pub use entity::{
    ActiveBuff, ActiveProjectile, BarrierState, Cargo, Entity, MovementMemory, PendingStrikes,
    UnitTask, WeaponPhase,
};
pub use error::{SimCoreError, SimResult};
pub use published_actions::ActionSet;
pub use steering::{
    check_cluster_arrival, compute_concave_arc_steering, compute_push_displacement,
    compute_unified_local_steering, resolve_push_interaction, PushEntityParams, SpatialHashGrid,
    SteeringObstacle, GRID_HEIGHT, GRID_WIDTH, SPATIAL_CELL_SIZE,
};
pub use tech_tree::TechTreeValidator;
pub use upgrades::{UpgradeCatalog, UpgradeDef, UpgradeId};
pub use weapon_timing::{
    AttackRate, InvalidAttackRate, InvalidBuffAttackRate, SimulationDiagnostic,
};
pub use world::{ActiveHazardZone, PlayerResources, SimWorld};

/// The shipped catalogs, embedded for this crate's own unit tests only.
///
/// Production code loads catalogs from disk (charter §3.3). Other crates' tests use the
/// `sim_fixtures` crate, which this crate's unit tests cannot depend on without linking a
/// second copy of `sim_core`.
#[cfg(test)]
#[allow(clippy::expect_used)]
pub(crate) mod test_support {
    use crate::catalog::{FactionCatalog, RuntimeCatalogs};
    use crate::{SimWorld, UnitCatalog, UpgradeCatalog};

    /// The shipped faction and class identities.
    pub(crate) fn shipped_factions() -> FactionCatalog {
        FactionCatalog::from_ron_str(include_str!("../../../assets/catalog/factions.ron"))
            .expect("invariant: the shipped factions.ron is valid")
    }

    /// The shipped unit, structure and upgrade catalogs.
    pub(crate) fn shipped_catalogs() -> RuntimeCatalogs {
        let factions = shipped_factions();
        let units = UnitCatalog::from_ron_str_bundle(
            &factions,
            include_str!("../../../assets/catalog/units.ron"),
            include_str!("../../../assets/catalog/structures.ron"),
        )
        .expect("invariant: the shipped units.ron and structures.ron are valid");
        let upgrades = UpgradeCatalog::from_ron_str(
            &factions,
            include_str!("../../../assets/catalog/upgrades.ron"),
        )
        .expect("invariant: the shipped upgrades.ron is valid");
        RuntimeCatalogs { units, upgrades }
    }

    /// A world on `map` with the shipped catalogs.
    pub(crate) fn world(map: sim_map::SimMap, seed: u64) -> SimWorld {
        let catalogs = shipped_catalogs();
        SimWorld::with_catalogs(map, seed, catalogs.units, catalogs.upgrades)
    }
}
