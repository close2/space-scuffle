//! Weapon specifications, firing geometry envelopes, and splash profiles.

use crate::enums::{AttributeFlags, WeaponVisualKind};
use crate::math::{SimAngle, SimScalar};
use crate::upgrades::WeaponClass;
use serde::{Deserialize, Serialize};

/// Concentric circular splash falloff parameters (`StarCraft` II standard).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplashSpec {
    /// 100% full damage radius from impact center.
    pub inner_radius: SimScalar,
    /// 50% half damage radius from impact center.
    pub middle_radius: SimScalar,
    /// 25% quarter damage radius from impact center.
    pub outer_radius: SimScalar,
    /// If true, splash affects allied units as well.
    #[serde(default)]
    pub friendly_fire: bool,
}

/// Behavior of an in-flight homing projectile when its intended target entity becomes unavailable
/// (e.g. loaded into a transport, cloaked without detection, expired hallucination, or destroyed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectileTargetLossBehavior {
    /// Default SC2 homing projectile behavior: if the target becomes unavailable
    /// in flight (loaded into transport, cloaked/burrowed without detection,
    /// expired hallucination, stasis, or died), the projectile harmlessly fizzles/disjoints.
    #[default]
    Fizzle,
    /// Ballistic/artillery behavior: continues to the last known `target_pos`
    /// and detonates splash damage on the ground coordinate even if the original unit vanished.
    ImpactLastKnownPosition,
    /// Seeking missile behavior (SC1 Valkyrie / SC2 Mirage / Glaive-style retargeting):
    /// when the current target becomes unavailable in flight, scans spatial neighborhood within
    /// `search_radius` around `proj.current_pos` for the nearest available enemy matching
    /// `weapon_spec.target_flags` and dynamically re-locks `proj.target_id = Some(new_id)`.
    /// If no valid new target is found within `search_radius`, falls back to `Fizzle`.
    RetargetNearest {
        /// Radius around current projectile position to search for a new valid target.
        search_radius: SimScalar,
    },
}

/// Default in-flight missile lifetime timeout in ticks (125 ticks = 5.0 seconds at 25 Hz).
pub const DEFAULT_PROJECTILE_LIFETIME_TICKS: u16 = 125;

/// Returns the default projectile flight timeout in simulation ticks (125 ticks = 5.0s at 25 Hz).
#[must_use]
pub const fn default_projectile_lifetime_ticks() -> u16 {
    DEFAULT_PROJECTILE_LIFETIME_TICKS
}

/// Definition of weapon capabilities, firing geometry, and timing envelopes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponSpec {
    /// Descriptive name of the weapon system.
    pub name: String,
    /// Visual style rendered for this weapon attack.
    pub visual_kind: WeaponVisualKind,
    /// Target classification mask (Ground, Air, or Both).
    pub target_flags: AttributeFlags,
    /// Minimum firing range (e.g. Siege Tank siege mode min range).
    pub min_range: SimScalar,
    /// Maximum engagement range in world units.
    pub max_range: SimScalar,
    /// Maximum angular deviation allowed from target before firing (e.g. 15°).
    pub firing_arc: SimAngle,
    /// Whether weapon rotates independently on a turret mount.
    pub has_turret: bool,
    /// Turret angular rotation speed per tick.
    pub turret_turn_rate: SimAngle,
    /// Base damage applied per hit.
    pub base_damage: SimScalar,
    /// Bonus damage applied against [`AttributeFlags::ARMORED`] targets.
    pub bonus_armored: SimScalar,
    /// Bonus damage applied against [`AttributeFlags::LIGHT`] targets.
    pub bonus_light: SimScalar,
    /// Number of distinct damage strikes applied per attack cycle (e.g. Zealot = 2).
    pub attacks_count: u8,
    /// Total ticks between consecutive attack cycles (attack period).
    pub period_ticks: u16,
    /// Wind-up delay in ticks before damage is committed (`DamagePoint`).
    pub damage_point_ticks: u16,
    /// Post-shot follow-through animation ticks (can be cancelled by Move).
    pub backswing_ticks: u16,
    /// Area-of-effect splash specifications, if applicable.
    pub splash: Option<SplashSpec>,
    /// Projectile linear flight speed per tick (`None` for hitscan / continuous beams).
    pub projectile_speed: Option<SimScalar>,
    /// High-level weapon classification determining which scaling upgrade applies.
    #[serde(default)]
    pub weapon_class: Option<WeaponClass>,
    /// Damage increase per upgrade level.
    #[serde(default)]
    pub upgrade_bonus_per_level: SimScalar,
    /// Attribute bonus damage increase per upgrade level.
    #[serde(default)]
    pub bonus_upgrade_per_level: SimScalar,
    /// Behavior when the target entity becomes unavailable mid-flight.
    #[serde(default)]
    pub target_loss_behavior: ProjectileTargetLossBehavior,
    /// Maximum in-flight lifetime in ticks before missile or seeking mine fuels out and expires (default: 125 = 5.0s at 25 Hz).
    #[serde(default = "default_projectile_lifetime_ticks")]
    pub max_projectile_lifetime_ticks: u16,
}
