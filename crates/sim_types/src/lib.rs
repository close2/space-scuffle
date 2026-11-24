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
pub mod effects;
pub mod enums;
pub mod errors;
pub mod footprint;
pub mod ids;
pub mod math;
pub mod placement;
pub mod player_view;
pub mod presentation;
pub mod resource_kind_set;
mod sin_table;
pub mod snapshot;
pub mod upgrades;
pub mod weapon;

// Flat convenient re-exports
pub use commands::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, ArchetypeCapabilities, AttackCommand,
    AttackDashSpec, AttackOrigin, AutoRallySpec, BarrierSpec, HarvesterSpec, OverclockSpec,
    PlayerCommand, RallyTarget, StanceOrders, TickBundle, TickInputBundle, UnitModeSpec, UnitOrder,
};
pub use effects::{
    CancelOrdersDef, DamageDef, DamageResponses, DisplaceDef, DisplaceDestination, EffectAnchor,
    EffectContext, EffectDef, EffectId, EffectNodeDef, EffectTarget, FilterAlliance,
    LaunchMissileDef, MissileView, ModifyVitalsDef, OriginPlacement, PersistentDef, SearchDef,
    SetDef, StatusId, StatusRefDef, StatusSpec, StructureRule, TargetFilter, TargetPlane,
    VitalChange,
};
pub use enums::{
    AnimState, AttributeFlags, MoverType, ResourceKind, ShapeKind, SilhouetteClass,
    VisibilityState, WeaponVisualKind,
};
pub use errors::MathError;
pub use footprint::{
    FootprintShape, QuarterOutline, FOOTPRINT_QUARTERS_PER_CELL, QUARTER_OUTLINE_CAPACITY,
};
pub use ids::{
    ArmorClassId, EntityId, FactionId, PerPlayer, PlayerId, PlayerSlot, PlayerSlotError,
    ResourceNodeId, TeamId, Tick, UnitTypeId, WeaponClassId, MAX_PLAYERS,
};
pub use math::{SimAngle, SimScalar, SimVec2};
pub use placement::{PlacementFlag, PlacementFlags};
pub use player_view::{
    AbilityLockView, CarrierView, CastStateView, CommandOptionView, CommandSubject, ModeStateView,
    OptionLock, PlayerView, QueuedItem, QueuedJobView, RallyView, SiteRefusal, StructureQueueView,
    VisibleNodeView,
};
pub use presentation::{
    AbilitySource, AreaView, Concealment, EffectSource, LineView, LinkView, ModeTransitionView,
    SpellMissileView, StanceView, StatusFlags, StatusKind, StatusView, StrikeEnds, WeaponKey,
    WeaponSlot,
};
pub use resource_kind_set::ResourceKindSet;
pub use snapshot::{
    AttackSwing, BubbleRenderInstance, CombatEventVisual, MatchResult, PlayerStatusView,
    RenderSnapshot, ResourceNodeView,
};
pub use upgrades::{
    PlayerUpgradeState, UnitStatDeltas, UpgradeApplicationError, UpgradeEffect, UpgradeStat,
};
pub use weapon::{
    default_projectile_lifetime_ticks, AllowedMovement, Attribute, AttributeBonuses,
    ProjectileTargetLossBehavior, SplashCliff, SplashImpact, SplashLine, SplashLineAxis,
    SplashLineOrigin, SplashShape, SplashSpec, SplashZone, StructureStrike, TurretIdle, WeaponSpec,
    DEFAULT_PROJECTILE_LIFETIME_TICKS,
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
