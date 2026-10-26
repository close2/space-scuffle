//! # `sim_types` — Foundational Contract of Space Scuffle
//!
//! This crate defines the foundational types, deterministic fixed-point math,
//! gameplay identifiers, unit/building states, and rendering snapshot boundaries
//! for the Space Scuffle real-time strategy engine.
//!
//! ## Architectural Charter & Guarantees
//! 1. **100% Determinism:** Authoritative simulation types use fixed-point arithmetic
//!    (`SimScalar`, `SimVec2`, `SimAngle`). Floats (`f32`) are completely forbidden
//!    in authoritative state.
//! 2. **Platform Independence:** Math operations yield identical bit-level results
//!    across `x86_64`, `aarch64`, and `wasm32`.
//! 3. **Decoupled Snapshot Boundary:** Conversion to floating-point occurs strictly at
//!    the `RenderSnapshot` boundary, shielding the simulation from rendering details.
//! 4. **Zero Unchecked Failures:** All fallible operations return explicit `Result<T, MathError>`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod commands;
pub mod enums;
pub mod errors;
pub mod ids;
pub mod math;
mod sin_table;
pub mod snapshot;
pub mod upgrades;
pub mod weapon;

// Flat convenient re-exports
pub use commands::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, ArchetypeCapabilities, HarvesterSpec,
    OverclockSpec, PlayerCommand, TickBundle, TickInputBundle, UnitModeSpec, UnitOrder,
};
pub use enums::{
    AnimState, AttributeFlags, Faction, MoverType, ShapeKind, SilhouetteClass, VisibilityState,
    WeaponVisualKind,
};
pub use errors::MathError;
pub use ids::{EntityId, PlayerId, TeamId, Tick, UnitTypeId};
pub use math::{SimAngle, SimScalar, SimVec2};
pub use snapshot::{BubbleRenderInstance, CombatEventVisual, RenderSnapshot};
pub use upgrades::{ArmorClass, PlayerUpgradeState, UnitStatDeltas, UpgradeEffect, WeaponClass};
pub use weapon::{
    default_projectile_lifetime_ticks, ProjectileTargetLossBehavior, SplashSpec, WeaponSpec,
    DEFAULT_PROJECTILE_LIFETIME_TICKS,
};
