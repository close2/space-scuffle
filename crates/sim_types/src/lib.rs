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
//! 3. **Decoupled Presentation Boundary:** Fixed-point values become floating-point only in
//!    presentation output (`RenderSnapshot`, `PlayerView`), and floats enter only as authored
//!    data converted at load, shielding the simulation from rendering details.
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
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, ArchetypeCapabilities, AttackDashSpec,
    AttackOrigin, BarrierSpec, HarvesterSpec, OverclockSpec, PlayerCommand, TickBundle,
    TickInputBundle, UnitModeSpec, UnitOrder,
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
    AttackSwing, BubbleRenderInstance, CombatEventVisual, MatchResult, PlayerStatusView,
    RenderSnapshot, ResourceNodeView,
};
pub use upgrades::{
    PlayerUpgradeState, UnitStatDeltas, UpgradeApplicationError, UpgradeEffect, UpgradeStat,
};
pub use weapon::{
    default_projectile_lifetime_ticks, Attribute, AttributeBonuses, ProjectileTargetLossBehavior,
    SplashLine, SplashLineAxis, SplashLineOrigin, SplashShape, SplashSpec, SplashZone,
    StructureStrike, TurretIdle, WeaponSpec, DEFAULT_PROJECTILE_LIFETIME_TICKS,
};

/// Simulation ticks per second of game time: the one fixed tick rate of every match (25 Hz,
/// 40 ms of game time per tick).
///
/// Every duration the simulation stores is a whole number of ticks; this constant converts
/// rates and durations authored in seconds (catalog energy drain, turret turn rates given per
/// second, projectile flight times shown to the player) into ticks. It is a `u16` because every
/// stored duration (cooldowns, build times, timeouts) is a `u16` tick count, and a `u16`
/// converts losslessly to `u32`, `i32`, `f32` and `f64`.
///
/// `DESIGN.md` states this rate, and check C17 (`crates/workspace_checks/tests/charter.rs`)
/// fails if the two disagree.
pub const TICKS_PER_SECOND: u16 = 25;
