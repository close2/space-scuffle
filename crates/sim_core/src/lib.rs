//! # Sim Core: Deterministic RTS Simulation Engine
//!
//! `sim_core` implements the authoritative, deterministic simulation tick loop for
//! **Space Scuffle**. It models StarCraft II-grade RTS combat mechanics, including:
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
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::manual_let_else,
    clippy::cast_lossless,
    clippy::module_name_repetitions
)]

pub mod catalog;
pub mod checksum;
pub mod combat;
pub mod entity;
pub mod error;
pub mod steering;
pub mod tech_tree;
pub mod upgrades;
pub mod world;

pub use catalog::{SplashSpec, UnitArchetype, UnitCatalog, WeaponSpec};
pub use checksum::compute_world_checksum;
pub use combat::{
    calculate_hit_damage, calculate_hit_damage_upgraded, calculate_splash_damage,
    calculate_splash_damage_upgraded, compute_threat_priority, is_in_weapon_range,
    is_within_firing_arc, step_entity_weapon, DamageResolution, TargetPriorityKey,
    MIN_DAMAGE_CLAMP,
};
pub use entity::{ActiveBuff, ActiveProjectile, Entity, WeaponPhase};
pub use error::{SimCoreError, SimResult};
pub use steering::{
    check_cluster_arrival, compute_concave_arc_steering, compute_push_displacement,
    compute_unified_local_steering, resolve_push_interaction, PushEntityParams, SpatialHashGrid,
    SteeringObstacle, GRID_HEIGHT, GRID_WIDTH, SPATIAL_CELL_SIZE,
};
pub use tech_tree::TechTreeValidator;
pub use upgrades::{UpgradeCatalog, UpgradeDef, UpgradeId};
pub use world::{PlayerResources, SimWorld};
