//! Weapon specifications, firing geometry envelopes, and splash profiles.

use crate::enums::{AttributeFlags, WeaponVisualKind};
use crate::ids::WeaponClassId;
use crate::math::{SimAngle, SimScalar};
use serde::{Deserialize, Serialize};

/// One unit attribute a weapon can deal bonus damage against: an index of `StarCraft` II's
/// `CEffectDamage` `AttributeBonus` array, and one bit of [`AttributeFlags`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Attribute {
    /// [`AttributeFlags::LIGHT`].
    Light,
    /// [`AttributeFlags::ARMORED`].
    Armored,
    /// [`AttributeFlags::BIOLOGICAL`].
    Biological,
    /// [`AttributeFlags::MECHANICAL`].
    Mechanical,
    /// [`AttributeFlags::PSIONIC`].
    Psionic,
    /// [`AttributeFlags::MASSIVE`].
    Massive,
    /// [`AttributeFlags::STRUCTURE`].
    Structure,
    /// [`AttributeFlags::HEROIC`].
    Heroic,
}

impl Attribute {
    /// Every attribute, in declaration order, which is also the order of
    /// [`AttributeBonuses`]' entries and of their checksum encoding.
    pub const ALL: [Self; 8] = [
        Self::Light,
        Self::Armored,
        Self::Biological,
        Self::Mechanical,
        Self::Psionic,
        Self::Massive,
        Self::Structure,
        Self::Heroic,
    ];

    /// The attribute's bit in a unit's [`AttributeFlags`].
    #[must_use]
    pub const fn flag(self) -> AttributeFlags {
        match self {
            Self::Light => AttributeFlags::LIGHT,
            Self::Armored => AttributeFlags::ARMORED,
            Self::Biological => AttributeFlags::BIOLOGICAL,
            Self::Mechanical => AttributeFlags::MECHANICAL,
            Self::Psionic => AttributeFlags::PSIONIC,
            Self::Massive => AttributeFlags::MASSIVE,
            Self::Structure => AttributeFlags::STRUCTURE,
            Self::Heroic => AttributeFlags::HEROIC,
        }
    }

    /// Position of the attribute in [`Self::ALL`].
    const fn index(self) -> usize {
        self as usize
    }
}

/// Bonus damage per strike against each [`Attribute`], in damage points; 0 for an attribute
/// the weapon has no bonus against (`StarCraft` II's `AttributeBonus`, whose default is 0).
///
/// A strike adds the bonus of every attribute its target has, so a weapon with bonuses against
/// two attributes deals both to a target with both, as the game's damage effect does.
// PERFORMANCE RATIONALE:
// Why: A weapon spec is cloned whenever combat resolves a weapon (every armed unit, every tick),
// and a list of attribute–bonus pairs would allocate on each clone.
// What: A fixed array indexed by attribute, `Copy`, at most one value per attribute by
// construction.
// Trade-off: Eight scalars per weapon where most weapons use none or one, in exchange for no
// allocation and no duplicate-attribute state to guard against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeBonuses([SimScalar; 8]);

impl AttributeBonuses {
    /// No bonus against any attribute.
    pub const NONE: Self = Self([SimScalar::ZERO; 8]);

    /// The bonus against `attribute`.
    #[must_use]
    pub const fn get(&self, attribute: Attribute) -> SimScalar {
        self.0[attribute.index()]
    }

    /// These bonuses with the bonus against `attribute` set to `bonus`.
    #[must_use]
    pub const fn with(mut self, attribute: Attribute, bonus: SimScalar) -> Self {
        self.0[attribute.index()] = bonus;
        self
    }

    /// Every attribute with its bonus, in [`Attribute::ALL`] order, zeros included.
    pub fn entries(&self) -> impl Iterator<Item = (Attribute, SimScalar)> + '_ {
        Attribute::ALL
            .into_iter()
            .map(|attribute| (attribute, self.get(attribute)))
    }

    /// The positive bonuses that apply to a target with `attributes`.
    pub fn against(&self, attributes: AttributeFlags) -> impl Iterator<Item = SimScalar> + '_ {
        self.entries()
            .filter(move |&(attribute, bonus)| {
                bonus > SimScalar::ZERO && attributes.contains(attribute.flag())
            })
            .map(|(_, bonus)| bonus)
    }
}

/// One zone of an area weapon: one entry of `StarCraft` II's `CEffectDamage` `AreaArray`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplashZone {
    /// The zone's reach in world units, measured from the impact point to the struck unit's
    /// centre (`AreaArray` `Radius`).
    pub radius: SimScalar,
    /// The share of the strike's damage a unit in this zone takes, `0 < fraction <= 1`
    /// (`AreaArray` `Fraction`).
    pub fraction: SimScalar,
}

/// Circular area damage around an impact point: a list of zones with their own radii and
/// fractions, read per weapon from the catalog.
///
/// What this models of `StarCraft` II, and what it does not: the zones and fractions are the
/// weapon's `AreaArray` (bug B-16 fixed). Distance is measured centre to centre, where the
/// game's area searches may reach a unit's edge (`SearchFlags` `OffsetByUnitRadius`; bug B-17,
/// owner question Q0063), and every area is a full circle, where some of the game's are lines
/// or arcs (bugs B-18 and B-19). See the `DESIGN.md` bug register.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplashSpec {
    /// The zones, radii strictly increasing; a unit takes the fraction of the innermost zone
    /// that reaches it, and nothing beyond the last.
    pub zones: Vec<SplashZone>,
    /// If true, splash affects allied units as well.
    #[serde(default)]
    pub friendly_fire: bool,
}

impl SplashSpec {
    /// The fraction of the strike a unit `distance` world units from the impact point takes:
    /// the innermost zone whose radius reaches it (a zone includes its edge), or `None` beyond
    /// the last zone.
    #[must_use]
    pub fn fraction_at(&self, distance: SimScalar) -> Option<SimScalar> {
        self.zones
            .iter()
            .find(|zone| distance <= zone.radius)
            .map(|zone| zone.fraction)
    }

    /// The largest radius of any zone in world units, zero for a splash with no zone: no unit
    /// farther than this from the impact point is struck.
    #[must_use]
    pub fn reach(&self) -> SimScalar {
        self.zones
            .iter()
            .map(|zone| zone.radius)
            .max()
            .unwrap_or(SimScalar::ZERO)
    }
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
pub const DEFAULT_PROJECTILE_LIFETIME_TICKS: u16 = 5 * crate::TICKS_PER_SECOND;

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
    /// Bonus damage per strike against each attribute the target has (`AttributeBonus`).
    pub attribute_bonuses: AttributeBonuses,
    /// Number of distinct damage strikes applied per attack cycle (e.g. Zealot = 2).
    pub attacks_count: u8,
    /// Ticks between consecutive strikes of one attack: the `PeriodicPeriodArray` of the
    /// weapon's `CEffectCreatePersistent` in `StarCraft` II. The first strike lands on the
    /// release tick and strike `k` (from 0) lands `k × strike_interval_ticks` ticks later. 0 lands
    /// every strike on the release tick; a single-strike weapon authors 0.
    pub strike_interval_ticks: u16,
    /// Greatest centre-to-centre distance, in world units, between attacker and target at which
    /// a strike after the first still lands (a `CValidatorLocationCompareRange` on the strike's
    /// effect in `StarCraft` II, such as the Zealot's 2.1). `None`: no distance condition.
    pub followup_max_distance: Option<SimScalar>,
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
    /// Weapon class whose `WeaponDamageLevel` upgrades scale this weapon, resolved from its
    /// slug in `assets/catalog/factions.ron`; `None` for a weapon no upgrade scales.
    #[serde(default)]
    pub weapon_class: Option<WeaponClassId>,
    /// Damage increase per upgrade level.
    #[serde(default)]
    pub upgrade_bonus_per_level: SimScalar,
    /// Increase per upgrade level of every attribute bonus that applies to a strike: each
    /// bonus that applies gains this much per level (`StarCraft` II upgrades each
    /// `AttributeBonus` entry of a damage effect, for example `liberty` UpgradeData:679).
    #[serde(default)]
    pub bonus_upgrade_per_level: SimScalar,
    /// Behavior when the target entity becomes unavailable mid-flight.
    #[serde(default)]
    pub target_loss_behavior: ProjectileTargetLossBehavior,
    /// Maximum in-flight lifetime in ticks before missile or seeking mine fuels out and expires (default: 125 = 5.0s at 25 Hz).
    #[serde(default = "default_projectile_lifetime_ticks")]
    pub max_projectile_lifetime_ticks: u16,
    /// If true, firing this weapon causes the attacking unit to detonate and destroy itself (e.g. Baneling).
    #[serde(default)]
    pub suicide_on_attack: bool,
}

#[cfg(test)]
mod tests {
    use super::{Attribute, AttributeBonuses, AttributeFlags, SimScalar, SplashSpec, SplashZone};

    #[test]
    fn every_attribute_has_its_own_flag_in_declaration_order() {
        let mut seen = AttributeFlags::empty();
        for (position, attribute) in Attribute::ALL.into_iter().enumerate() {
            assert_eq!(attribute.index(), position, "{attribute:?}");
            assert!(
                !seen.intersects(attribute.flag()),
                "{attribute:?} repeats a flag"
            );
            seen |= attribute.flag();
        }
        assert_eq!(seen, AttributeFlags::all());
    }

    #[test]
    fn bonuses_apply_once_per_attribute_the_target_has() {
        let six = SimScalar::from_i32(6);
        let ten = SimScalar::from_i32(10);
        let bonuses = AttributeBonuses::NONE
            .with(Attribute::Massive, six)
            .with(Attribute::Armored, ten);
        assert_eq!(bonuses.get(Attribute::Massive), six);
        assert_eq!(bonuses.get(Attribute::Light), SimScalar::ZERO);
        let sum = |flags| bonuses.against(flags).fold(SimScalar::ZERO, |a, b| a + b);
        assert_eq!(sum(AttributeFlags::MASSIVE), six);
        assert_eq!(
            sum(AttributeFlags::ARMORED | AttributeFlags::MASSIVE),
            six + ten
        );
        assert_eq!(sum(AttributeFlags::LIGHT), SimScalar::ZERO);
        assert_eq!(
            AttributeBonuses::NONE
                .against(AttributeFlags::all())
                .count(),
            0
        );
    }

    #[test]
    fn splash_fraction_is_the_innermost_reaching_zones() {
        let zone = |radius: i32, quarters: i32| SplashZone {
            radius: SimScalar::from_i32(radius),
            fraction: SimScalar::from_i32(quarters) / SimScalar::from_i32(4),
        };
        let splash = SplashSpec {
            zones: vec![zone(1, 4), zone(3, 1)],
            friendly_fire: false,
        };
        assert_eq!(splash.fraction_at(SimScalar::ZERO), Some(SimScalar::ONE));
        assert_eq!(splash.fraction_at(SimScalar::ONE), Some(SimScalar::ONE));
        assert_eq!(
            splash.fraction_at(SimScalar::from_i32(2)),
            Some(SimScalar::from_i32(1) / SimScalar::from_i32(4))
        );
        assert_eq!(splash.fraction_at(SimScalar::from_i32(4)), None);
        assert_eq!(splash.reach(), SimScalar::from_i32(3));
        let empty = SplashSpec {
            zones: Vec::new(),
            friendly_fire: false,
        };
        assert_eq!(empty.fraction_at(SimScalar::ZERO), None);
        assert_eq!(empty.reach(), SimScalar::ZERO);
    }
}
