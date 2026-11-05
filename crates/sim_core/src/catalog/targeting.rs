//! # Automatic Targeting Data
//!
//! The per-archetype facts automatic target acquisition reads (`crate::target_acquisition`):
//! how attractive a unit is as a target, whether it always counts as a threat, whether it
//! threatens others while idle, which of its weapons is primary, and how it breaks ties between
//! equally ranked targets.
//!
//! ## How it fits
//! The catalog loads these from `assets/catalog/units.ron` and `assets/catalog/structures.ron`
//! onto every [`super::UnitArchetype`]. They mirror `StarCraft` II's `CUnit`
//! `AttackTargetPriority`, the `CUnit` flag `AlwaysThreatens`, `CUnit` `DefaultAcquireLevel`,
//! the order of `CUnit` `WeaponArray` and `CWeapon` `AcquirePrioritization`. No mechanism
//! branches on an identity: each reads only these values.
//!
//! ## Invariants Maintained
//! 1. Every enum here names one engine behaviour, never a unit.
//! 2. A default exists only where it is `StarCraft` II's engine default, and its doc says so.

use serde::{Deserialize, Serialize};

/// How a unit picks among candidates equal on every other criterion once it has lost its
/// previous target (`StarCraft` II `CWeapon` `AcquirePrioritization`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcquireTieBreak {
    /// The candidate closest to the unit, centre to centre (`ByDistance`, the engine default:
    /// `core` `WeaponData` sets no `AcquirePrioritization`).
    Closest,
    /// The candidate that needs the least turning of the facing that fires at it: the turret's
    /// for a turreted weapon, the hull's otherwise (`ByAngle`).
    LeastTurning,
    /// The candidate closest to where the unit's previous target last was
    /// (`ByDistanceFromTarget`).
    ClosestToPrevious,
}

/// Which of a unit's weapons `StarCraft` II treats as primary: the first entry of its
/// `WeaponArray`. A unit prefers targets its primary weapon can attack over targets only its
/// other weapon can, when threat and attack target priority are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimaryWeapon {
    /// The unit has one weapon, or one weapon serves both planes, so the criterion never
    /// separates its candidates.
    Either,
    /// The ground weapon (`ground_weapon`, or the legacy `weapon` slot) is primary.
    Ground,
    /// The air weapon (`air_weapon`) is primary.
    Air,
}

/// When a unit counts as a threat to an enemy whose weapons it could hit (`StarCraft` II
/// `CUnit` `DefaultAcquireLevel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AcquireLevel {
    /// Always, while it is armed against the enemy (`Offensive`, the engine default: `core`
    /// `UnitData` `CUnit` default).
    #[default]
    Offensive,
    /// Only while it is attack-moving (`Defensive`, the workers' level).
    Defensive,
}
