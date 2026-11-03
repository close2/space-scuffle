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
#![deny(clippy::unreachable)]

pub mod commands;
pub mod enums;
pub mod errors;
pub mod ids;
pub mod math;
pub mod player_view;
mod sin_table;
pub mod snapshot;
pub mod upgrades;
pub mod weapon;

// Flat convenient re-exports
pub use commands::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, ArchetypeCapabilities, AttackOrigin,
    HarvesterSpec, OverclockSpec, PlayerCommand, TickBundle, TickInputBundle, UnitModeSpec,
    UnitOrder,
};
pub use enums::{
    AnimState, AttributeFlags, MoverType, ResourceKind, ShapeKind, SilhouetteClass,
    VisibilityState, WeaponVisualKind,
};
pub use errors::MathError;
pub use ids::{
    ArmorClassId, EntityId, FactionId, PerPlayer, PlayerId, PlayerSlot, PlayerSlotError,
    ResourceNodeId, TeamId, Tick, UnitTypeId, WeaponClassId, MAX_PLAYERS,
};
pub use math::{SimAngle, SimScalar, SimVec2};
pub use player_view::{
    CarrierView, CommandOptionView, CommandSubject, OptionLock, PlayerView, QueuedItem,
    QueuedJobView, SiteRefusal, StructureQueueView, VisibleNodeView,
};
pub use snapshot::{
    BubbleRenderInstance, CombatEventVisual, MatchResult, PlayerStatusView, RenderSnapshot,
    ResourceNodeView,
};
pub use upgrades::{
    PlayerUpgradeState, UnitStatDeltas, UpgradeApplicationError, UpgradeEffect, UpgradeStat,
};
pub use weapon::{
    default_projectile_lifetime_ticks, ProjectileTargetLossBehavior, SplashSpec, WeaponSpec,
    DEFAULT_PROJECTILE_LIFETIME_TICKS,
};
