//! Weapon specifications, firing geometry envelopes, and splash profiles.

use crate::enums::{AttributeFlags, WeaponVisualKind};
use crate::ids::WeaponClassId;
use crate::math::{SimAngle, SimScalar, SimVec2};
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

/// Where the search points of a [`SplashLine`] are laid out from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplashLineOrigin {
    /// The attacker's position when it released the attack: the `SourcePoint` of the game's
    /// `CEffectCreatePersistent` (the Hellion's and the Lurker's lines).
    Attacker,
    /// The point the attack struck: the `TargetPoint` of the persistent (the Colossus' sweep).
    Target,
}

/// The direction in which a [`SplashLine`]'s offsets lie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplashLineAxis {
    /// Along the attack, from the attacker towards its target: the persistent's forward axis
    /// (its `PeriodicOffsetArray` `0,-d,0` entries; the game's forward is `-y`).
    Along,
    /// Across the attack, a quarter turn counter-clockwise from [`SplashLineAxis::Along`]:
    /// the persistent's `d,0,0` entries.
    Across,
}

/// A line of area searches: one search point per offset, each the centre of the splash's
/// zones. `StarCraft` II builds such a line from a `CEffectCreatePersistent` whose
/// `PeriodicOffsetArray` places one `CEffectEnumArea` per period (bug B-18).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplashLine {
    /// Where the offsets are measured from.
    pub origin: SplashLineOrigin,
    /// The direction the offsets lie in.
    pub axis: SplashLineAxis,
    /// Signed distances in world units from the origin along the axis, one search point each,
    /// in the order the game's persistent visits them.
    pub offsets: Vec<SimScalar>,
    /// Ticks between successive searches, the persistent's `PeriodicPeriodArray` (bug B-85):
    /// the first search lands on the release tick and search `k` lands `k × period_ticks`
    /// ticks later. 0 lands every search on the release tick (the Hellion's flame; the
    /// Colossus' sweep by the owner's answer to Q0064).
    #[serde(default)]
    pub period_ticks: u16,
}

/// The shape of a weapon's area.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplashShape {
    /// One search centred on the impact point: what a damage effect's own `AreaArray` does
    /// when no persistent places searches elsewhere, so this is the engine default.
    #[default]
    Circle,
    /// A line of searches (see [`SplashLine`]).
    Line(SplashLine),
}

/// Where a weapon's area is centred: `StarCraft` II's `ImpactLocation` of its damage effect.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplashImpact {
    /// On the struck target (the engine default, `TargetUnitOrPoint`, `core` EffectData:66): the
    /// target is struck directly and the area spares it.
    #[default]
    Target,
    /// On the attacker at release (`CasterUnit`, the Baneling's bursts, `liberty`
    /// EffectData:1759): no direct strike; the target is struck by the area like any other unit
    /// in it.
    Attacker,
}

/// Which cliff levels a weapon's area reaches: `StarCraft` II's `SearchFlags` `SameCliff`.
///
/// A position's levels are read from the map under it (`sim_map::GameMap::cliff_span_at`): a
/// ramp counts as both of its landings (owner answer Q0099 1(c), plan
/// `2026-11-24-owner-answers-cliff-and-vision` D-1), and an area on the air plane and a unit in
/// the air ignore cliff levels (Q0099 2(b), that plan's D-2). The level is read at the search
/// point (Q0099 3(a)).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplashCliff {
    /// Only units on the cliff level of the search point that reaches them (`SameCliff 1`, the
    /// default of every `CEffectDamage`, `core` EffectData:67).
    #[default]
    Same,
    /// Units on every level (`SameCliff 0`; a `CEffectEnumArea` search, which does not set it).
    Any,
}

/// Area damage: a list of zones with their own radii and fractions, searched around one point
/// (a circle) or around each point of a line, read per weapon from the catalog.
///
/// What this models of `StarCraft` II, and what it does not: the zones and fractions are the
/// weapon's `AreaArray` (bug B-16 fixed), and a line weapon's search points are its
/// persistent's offsets (bug B-18). A circle is searched around the struck target or, by
/// [`SplashSpec::impact`], around the attacker (bug B-83), and it opens only below a target
/// radius when [`SplashSpec::target_radius_below`] says so (bug B-84). A unit found by several
/// searches of one strike takes that strike once, at the fraction of the nearest search point:
/// the game's damage effects check `noMarkers` against the attack's marker. A zone reaches a
/// unit whose edge lies within it, and a target-centred circle with `offset_by_unit_radius` is
/// centred on the target's edge facing the attacker (owner answer to Q0063, bug B-17 fixed). A
/// line's searches land together, where the game spaces some of them in time (bug B-85), and
/// no area is an arc (bug B-19). An area keeps to the cliff level of each search point unless
/// [`SplashSpec::cliff`] says it reaches every level, and an attacker-centred area may strike
/// its target directly across a cliff ([`SplashSpec::strike_target_on_other_level`], bug B-170
/// fixed). See the `DESIGN.md` bug register.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplashSpec {
    /// The zones, radii strictly increasing; a unit takes the fraction of the innermost zone
    /// that reaches it, and nothing beyond the last.
    pub zones: Vec<SplashZone>,
    /// Where the zones are searched: around the impact point, or around each point of a line.
    #[serde(default)]
    pub shape: SplashShape,
    /// If true, splash affects allied units as well.
    #[serde(default)]
    pub friendly_fire: bool,
    /// Whether the area is centred on the struck target's edge facing the attacker instead of
    /// its centre: `StarCraft` II's `SearchFlags` `OffsetByUnitRadius`, which every weapon damage
    /// effect inherits from `DU_WEAP` (`core` EffectData:128) and the four weapons patch 1.1.0
    /// moved to "the center of the target" clear (owner answer to Q0063). A line ignores it: its
    /// searches are `CEffectEnumArea` effects, which do not set the flag.
    #[serde(default)]
    pub offset_by_unit_radius: bool,
    /// Where the area is centred: on the struck target (the default) or on the attacker's
    /// position at release. An attacker-centred area is a circle, not offset by a radius, on an
    /// instant weapon, and it is the only damage of its shot (plan `2026-11-23-splash-centres`
    /// D-1).
    #[serde(default)]
    pub impact: SplashImpact,
    /// The area opens only when the struck target's collision radius, in world units, is below
    /// this; a target of this radius or more takes the strike alone. `StarCraft` II's
    /// `CEffectSwitch` between a splashing and a non-splashing damage effect on
    /// `TargetRadiusSmall`/`TargetRadiusLarge` (`liberty` ValidatorData:824–833). A hit on a
    /// point has radius zero, so it always opens. `None`: the area always opens.
    #[serde(default)]
    pub target_radius_below: Option<SimScalar>,
    /// Which cliff levels the area reaches: with [`SplashCliff::Same`] (the default) a unit is
    /// struck only from a search point on its own level, at that point's fraction; the primary
    /// target, struck directly, is struck on any level (plan `2026-11-23-splash-cliff-levels`
    /// D-1).
    #[serde(default)]
    pub cliff: SplashCliff,
    /// When the target stands on another cliff level than the attacker, the target takes this
    /// area's strike directly, at the first zone's fraction, once. `StarCraft` II's
    /// `BanelingVolatileBurstDirectFallbackSet` (`voidmulti` EffectData:2327–2358) on
    /// `CliffLevelNotEqual` (ValidatorData:894–898). Meaningful only for an attacker-centred area
    /// that keeps to its level, which validation requires (D-3).
    #[serde(default)]
    pub strike_target_on_other_level: bool,
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

    /// Writes the centres of this splash's searches into `points`, replacing its contents.
    ///
    /// `attacker` is where the attacker released the attack, `impact` where it struck, and
    /// `direction` the unit vector of the attack from attacker to target, all in world units.
    /// A circle searches once, at `impact`; a line once per offset.
    pub fn search_points(
        &self,
        attacker: SimVec2,
        impact: SimVec2,
        direction: SimVec2,
        points: &mut Vec<SimVec2>,
    ) {
        points.clear();
        match self.line_frame(attacker, impact, direction) {
            None => points.push(impact),
            Some((line, origin, axis)) => {
                points.extend(line.offsets.iter().map(|&offset| origin + axis * offset));
            }
        }
    }

    /// For a line splash, the line with the point its offsets are measured from and the unit
    /// axis they lie along, for an attack as [`Self::search_points`] takes it; `None` for a
    /// circle.
    #[must_use]
    pub fn line_frame(
        &self,
        attacker: SimVec2,
        impact: SimVec2,
        direction: SimVec2,
    ) -> Option<(&SplashLine, SimVec2, SimVec2)> {
        let SplashShape::Line(line) = &self.shape else {
            return None;
        };
        let origin = match line.origin {
            SplashLineOrigin::Attacker => attacker,
            SplashLineOrigin::Target => impact,
        };
        let axis = match line.axis {
            SplashLineAxis::Along => direction,
            SplashLineAxis::Across => SimVec2::new(-direction.y, direction.x),
        };
        Some((line, origin, axis))
    }
}

/// Behavior of an in-flight homing projectile when its intended target entity becomes unavailable
/// (e.g. loaded into a transport, cloaked without detection, expired hallucination, or destroyed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectileTargetLossBehavior {
    /// `PointFallback 0` (e.g. `StarCraft` II's `NeuralParasiteLaunchMissile`, `liberty`
    /// EffectData:1013): once the target fails the weapon's filters, the missile is removed
    /// without landing, so neither its strike nor its area lands anywhere.
    Fizzle,
    /// 5.0.14's `PointFallback` (`core` EffectData:97), the default of every attack missile:
    /// once the target fails the weapon's filters (it dies, is loaded, turns invulnerable, or
    /// cloaks or burrows undetected), the missile flies on to the last point it tracked, in
    /// world units, and lands there; its strike reaches no unit, its area strikes around the
    /// point. Permanent: a target that becomes available again is not re-acquired (plan
    /// `2026-11-23-vision-and-detection` D-1).
    #[default]
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

/// What a turret does while its weapon has no target: `StarCraft` II's `CTurret` `Idle` with its
/// `YawIdleRate` (plan `2026-11-05-sc2-deviation-fixes` WP10, B-30). The `SC2Mapster` editor
/// reference (Data/Turrets) defines each value; rates are binary angles per tick. A turret's
/// default position is the hull's facing: no modelled turret sets a `YawStart` with an `Idle`
/// that returns to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurretIdle {
    /// "Stays at its current angle" (`Hold`), in the world frame; every hull weapon.
    #[default]
    Hold,
    /// "Resets to default position" (`Reset`): turns towards the hull's facing by at most `rate`
    /// per tick.
    Reset {
        /// Greatest turn per tick.
        rate: SimAngle,
    },
    /// "Resets to default position if the unit is moving" (`ResetIfMoving`): as `Reset`, on a
    /// tick the unit moved.
    ResetIfMoving {
        /// Greatest turn per tick.
        rate: SimAngle,
    },
    /// "Follows the movement of the unit" (`FollowMovement`): on a tick the unit moved, turns
    /// towards the hull's facing, which follows its movement, by at most `rate` per tick.
    FollowMovement {
        /// Greatest turn per tick.
        rate: SimAngle,
    },
    /// "Rotates continuously" (`Spin`): adds `step` every tick; a step above `DEG_180` is a
    /// clockwise turn (a negative `YawIdleRate`).
    Spin {
        /// Turn per tick, counter-clockwise.
        step: SimAngle,
    },
}

/// Definition of weapon capabilities, firing geometry, and timing envelopes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent authored facts of one weapon (turret, turret lock, turret tracking, attack-order options, channel, suicide, melee), each read by a different rule"
)]
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
    /// Engaged arc: the greatest aim error, a half angle in binary units, at which the weapon
    /// keeps firing at the target it is engaged with (`StarCraft` II `Arc` ÷ 2 + `ArcSlop`).
    /// `SimAngle::DEG_180` fires in every direction.
    pub firing_arc: SimAngle,
    /// Acquisition arc: the greatest aim error, a half angle in binary units, at which the
    /// weapon starts on a target it is not engaged with (`StarCraft` II `Arc` ÷ 2). Never wider
    /// than `firing_arc`.
    pub acquire_arc: SimAngle,
    /// Whether weapon rotates independently on a turret mount.
    pub has_turret: bool,
    /// Turret angular rotation speed per tick.
    pub turret_turn_rate: SimAngle,
    /// What the turret does while the weapon has no target; `TurretIdle::Hold` for a hull
    /// weapon.
    pub turret_idle: TurretIdle,
    /// Whether the weapon's unit may move while it attacks (`StarCraft` II's `AllowedMovement`):
    /// a wind-up holds its unit unless this is [`AllowedMovement::Moving`], and a weapon that
    /// fires outside an attack order does so while its unit's order walks it only when it is
    /// `Moving` (plan `2026-11-23-turret-wind-up` D-2, D-6). Independent of the mount.
    #[serde(default)]
    pub allowed_movement: AllowedMovement,
    /// Whether the weapon fires when its unit is in no attack order: the inverse of `StarCraft`
    /// II's `Options OnlyFireWhileInAttackOrder`, which every weapon sets by default (`core`
    /// WeaponData:20), so `false` is the engine default. Such a weapon works against the target
    /// acquisition tracks for it under its unit's other orders (plan `2026-11-23-turret-wind-up`
    /// D-2, D-4).
    #[serde(default)]
    pub fires_outside_attack_order: bool,
    /// Whether the weapon may work against a target other than its unit's attack order's: the
    /// inverse of `StarCraft` II's `Options OnlyFireAtAttackOrderTarget`, which every weapon sets
    /// by default (`core` WeaponData:21), so `false` is the engine default. Inside an attack order
    /// whose target it cannot work against this tick (out of its engagement band, or failing the
    /// weapon filter), such a weapon works against the enemy acquisition chose for it
    /// (`Entity::weapon_target`), while its unit is not walking or the weapon is
    /// [`AllowedMovement::Moving`]; the order's target keeps priority whenever it can be worked
    /// (plan `2026-11-24-fire-rules` D-1, D-4).
    #[serde(default)]
    pub fires_at_other_targets: bool,
    /// Whether the weapon's turret holds still while the weapon fires: `StarCraft` II's
    /// `LegacyOptions LockTurretWhileFiring` (only the Hellion's `InfernalFlameThrower` sets it,
    /// `liberty` WeaponData:590), `false` by default. The turret does not turn from the start of a
    /// swing to the end of its backswing; a new swing that starts out of the backswing ends the
    /// lock. Meaningful only with `has_turret`, which catalog validation requires (plan
    /// `2026-11-24-fire-rules` D-1, D-2).
    #[serde(default)]
    pub locks_turret_while_firing: bool,
    /// Whether the weapon's turret also aims, under any order, at the best enemy this weapon
    /// reaches from where its unit stands, whenever the weapon itself leaves the turret free (no
    /// target, or a target it cannot work this tick); nothing is fired, drawn or charged.
    /// `StarCraft` II 5.0.14 gets this from either of two sources, whose observable is the same:
    /// a hidden second weapon on the same turret, the unsieged Siege Tank's `90mmCannonsFake`
    /// (`Options Hidden 1`, both attack-order options cleared, a `CEffectDamage` without amount;
    /// `balancemulti` WeaponData:7–20, UnitData:1473), whose range, filters and turret equal the
    /// host weapon's; or the weapon itself, when its attack-order options are cleared while its
    /// movement gate blocks its fire as its unit walks, the Immortal's `PhaseDisruptors` and the
    /// Colossus' `ThermalLances` (`voidmulti` WeaponData:205–207, :256–257): their turrets aim at
    /// the nearest enemy in reach while the unit walks and fire once it stops. `false` by
    /// default. Meaningful only with `has_turret`, which catalog validation requires (plan
    /// `2026-11-26-turret-tracking-weapon` D-1, D-7, Amendment 2 D-A1).
    #[serde(default)]
    pub turret_tracks_any_enemy: bool,
    /// Base damage applied per hit.
    pub base_damage: SimScalar,
    /// Bonus damage per strike against each attribute the target has (`AttributeBonus`).
    pub attribute_bonuses: AttributeBonuses,
    /// Number of distinct damage strikes applied per attack cycle (e.g. Zealot = 2).
    pub attacks_count: u8,
    /// Ticks from the release to the first strike of an attack: the first entry of the
    /// `PeriodicPeriodArray` of the weapon's `CEffectCreatePersistent` in `StarCraft` II, which
    /// comes before the first period as well as between periods (the Liberator's
    /// `LiberatorMissileBurstPersistent`, 0.05 Normal s, 1 tick). 0, the engine default, lands
    /// the first strike on the release tick. A delayed first strike is validated when it falls
    /// due, as every later strike is (plan `2026-11-23-strike-delays` D-2).
    pub first_strike_delay_ticks: u16,
    /// Ticks between consecutive strikes of one attack: the later entries of the
    /// `PeriodicPeriodArray` of the weapon's `CEffectCreatePersistent` in `StarCraft` II. Strike
    /// `k` (from 0) lands `first_strike_delay_ticks + k × strike_interval_ticks` ticks after the
    /// release. 0 lands every strike together; a single-strike weapon authors 0.
    pub strike_interval_ticks: u16,
    /// Greatest centre-to-centre distance, in world units, between attacker and target at which
    /// a strike that lands after the release tick still lands (a `CValidatorLocationCompareRange`
    /// on the strike's effect in `StarCraft` II, such as the Zealot's 2.1). `None`: no distance
    /// condition.
    pub followup_max_distance: Option<SimScalar>,
    /// Whether the burst effect is channeled (`StarCraft` II `CEffectCreatePersistent` flag
    /// `Channeled`): an explicit order that replaces the attacker's orders between two strikes
    /// ends the burst, and its later strikes are lost (Q0058). Meaningful only on a weapon whose
    /// strikes land on separate ticks.
    pub channeled: bool,
    /// Total ticks between consecutive attack cycles (attack period).
    pub period_ticks: u16,
    /// Wind-up delay in ticks before damage is committed (`DamagePoint`).
    pub damage_point_ticks: u16,
    /// Post-shot follow-through animation ticks (can be cancelled by Move).
    pub backswing_ticks: u16,
    /// Least random delay added to each attack's cycle, in ticks; may be negative
    /// (`StarCraft` II `RandomDelayMin`). Each attack draws its delay uniformly from
    /// `random_delay_min..=random_delay_max` with the world's seeded generator.
    pub random_delay_min: SimScalar,
    /// Greatest random delay added to each attack's cycle, in ticks, at least
    /// `random_delay_min` (`StarCraft` II `RandomDelayMax`). Equal bounds add a fixed delay
    /// without a draw.
    pub random_delay_max: SimScalar,
    /// Area-of-effect splash specifications, if applicable.
    pub splash: Option<SplashSpec>,
    /// The missile's speed, world units per tick: its constant speed, or with
    /// [`Self::missile_movers`] its top speed, the greatest `max_speed` of their phases
    /// (validated; plan `2026-11-27-weapon-delivery` D-5). `None`: an instant weapon (hitscan,
    /// beam or melee), which launches no missile.
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
    /// The strike dealt to a structure instead of this weapon's own, if any. `None`, the engine
    /// default, strikes structures like any unit.
    #[serde(default)]
    pub structure_strike: Option<StructureStrike>,
    /// Whether this is a melee weapon (`StarCraft` II `WeaponData` `Options Melee`). Only
    /// reductions of ranged weapons (`RangedWeaponRange`, plan `2026-11-10-spellcaster-mechanics`
    /// D-14) read it.
    #[serde(default)]
    pub melee: bool,
    /// What the weapon's damage shows of its attacker to the player it strikes (plan
    /// `2026-11-23-vision-and-detection` D-4). Omitted, the engine default: a snapshot.
    #[serde(default)]
    pub attack_visibility: AttackVisibility,
    /// What the weapon's missile re-checks about its target in flight (plan
    /// `2026-11-24-weapon-validation` D-1). Omitted, the engine default: both checks. Read only
    /// for a weapon with a `projectile_speed`; catalog validation refuses a non-default value on
    /// an instant weapon.
    #[serde(default)]
    pub missile_validation: MissileValidation,
    /// What the weapon's missile release shows of its attacker to the player whose unit it is
    /// aimed at: `StarCraft` II's `CEffectLaunchMissile` `Visibility` (plan
    /// `2026-11-27-launch-and-spell-visibility` D-1). Omitted, the engine default: a snapshot
    /// (`core` EffectData:102), which no 5.0.14 weapon launch overrides. Read only for a weapon
    /// with a `projectile_speed`; catalog validation refuses a non-default value on an instant
    /// weapon.
    #[serde(default)]
    pub launch_visibility: AttackVisibility,
    /// The movers the weapon's missile may fly: `StarCraft` II's `CEffectLaunchMissile` `Movers`
    /// array, tried in order at the release (plan `2026-11-27-weapon-delivery` D-2). Empty, the
    /// default: the missile flies at the constant `projectile_speed`. Read only for a weapon
    /// with a `projectile_speed`; catalog validation refuses a list on an instant weapon, and
    /// requires the last mover to admit any distance.
    #[serde(default)]
    pub missile_movers: Vec<MissileMover>,
    /// The launch channel this weapon starts instead of striking (`StarCraft` II's
    /// `CEffectCreatePersistent` of `CEffectUseMagazine`; plan `2026-11-28-carrier-interceptors`
    /// D-4). `None`, the default, for every weapon that strikes. A launch weapon deals no damage:
    /// its release spends its unit's hangar, one docked member per try.
    #[serde(default)]
    pub magazine_launch: Option<MagazineLaunch>,
}

/// How a launch weapon's release spends its unit's hangar (plan `2026-11-28-carrier-interceptors`
/// D-4): `StarCraft` II's channelled persistent that runs the hangar's `CEffectUseMagazine` a
/// number of times (`PeriodCount`, `PeriodicPeriodArray`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MagazineLaunch {
    /// Launch tries per release (`PeriodCount`), at least 1. A try with no docked member
    /// launches nothing but is spent.
    pub tries: u8,
    /// Ticks between tries (`PeriodicPeriodArray`), at least 1; the first try is on the release
    /// tick.
    pub interval_ticks: u16,
}

/// One entry of a missile launch's `Movers` array (`StarCraft` II `CEffectLaunchMissile`): the
/// speed phases a missile flies when the distance at its release is at most `up_to_range`
/// (plan `2026-11-27-weapon-delivery` D-2). A missile chooses its mover once, at the release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissileMover {
    /// Greatest edge-to-edge distance, world units, between attacker and target at the release
    /// for which this mover is chosen (`IfRangeLTE`, inclusive): the centre distance less both
    /// collision radii, as the weapon's range check measures it. `None` admits any distance and
    /// is allowed only on the last mover.
    pub up_to_range: Option<SimScalar>,
    /// The mover's `MotionPhases`, flown in order; at least one.
    pub phases: Vec<MissilePhase>,
}

/// One `MotionPhases` entry of a missile mover, reduced to its speed profile: the path's shape
/// (`Throw`, `Guidance`) is not modelled, only how fast the missile covers it (plan
/// `2026-11-27-weapon-delivery` D-2, D-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissilePhase {
    /// Speed the phase starts at, world units per tick (`Speed`); `None` carries the speed the
    /// missile has when the phase starts (zero on the first phase).
    pub speed: Option<SimScalar>,
    /// Change of speed per tick, world units per tick squared (`Acceleration`); not negative.
    pub acceleration: SimScalar,
    /// Greatest speed in the phase, world units per tick (`MaxSpeed`); positive. A speed
    /// carried in above it drops to it on the phase's first tick.
    pub max_speed: SimScalar,
    /// Where the phase ends and the next begins (`Outro`, plan `2026-11-28-missile-movers-b52`
    /// D-2): `None` on the last phase and only there.
    pub ends: Option<PhaseEnd>,
}

/// The end of a missile phase: `StarCraft` II's `Outro`, whose sign picks the reference point
/// ("source-relative outros are positive and target-relative outros are negative"; plan
/// `2026-11-28-missile-movers-b52` D-2). Its value is positive, in world units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseEnd {
    /// The phase ends once the missile has flown this path since its release, world units (a
    /// positive `Outro`; the midpoint of its blend, plan `2026-11-27-weapon-delivery` D-3).
    Flown(SimScalar),
    /// The phase ends once the missile's remaining path to its impact is at most this many world
    /// units (a negative `Outro`, its magnitude). The remaining path is the centre distance to the
    /// aim point less the target's radius, the quantity the impact test reads.
    Remaining(SimScalar),
}

/// What a weapon's missile re-checks about its target in flight: `StarCraft` II's
/// `CEffectLaunchMissile` flags (`core` EffectData:95–109). A check that fails makes the
/// missile lose its target, and `target_loss_behavior` decides what it does then.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissileValidation {
    /// `ValidateWeapon` (:101): the target must keep passing the weapon's filters (alive,
    /// unloaded, vulnerable, seen and, if concealed, detected). `false`: only its existence
    /// is followed, and the filters are checked at impact instead.
    pub weapon: bool,
    /// `ValidateTeleport` (:100): a target that teleports after the launch is lost.
    pub teleport: bool,
}

impl Default for MissileValidation {
    /// The engine default of every `CEffectLaunchMissile`: both checks on.
    fn default() -> Self {
        Self {
            weapon: true,
            teleport: true,
        }
    }
}

/// What an effect shows of its caster to the player it is aimed at or strikes: `StarCraft` II's
/// `Visibility` of `CEffectDamage` (`core` EffectData:62) and of `CEffectLaunchMissile` (`core`
/// EffectData:102), both `Snapshot` by default. A weapon carries one for its damage
/// ([`WeaponSpec::attack_visibility`]) and one for its missile's release
/// ([`WeaponSpec::launch_visibility`]); a spell's damage and missile nodes carry their own (plan
/// `2026-11-27-launch-and-spell-visibility` D-1). The caster is the attacker or the spell's
/// casting unit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttackVisibility {
    /// A snapshot in fog and on the minimap, never a target (the engine default). The player
    /// struck or aimed at keeps a still image of the caster where it acted from, never a target,
    /// until it sees that spot or the caster (plan `2026-11-24-attacker-snapshot` D-2), and at
    /// most `units.ron`'s `attack_snapshot_cap_ticks` after the strike, launch or spell damage
    /// that made it (plan Amendment 1). The minimap half waits for a minimap (B-204).
    #[default]
    Snapshot,
    /// The caster becomes visible, and so targetable, to the owner of each unit struck or aimed
    /// at for `reveal_ticks` (`AttackReveal`, `core` BehaviorData:237–240, 3 Normal s).
    Visible {
        /// How long one effect reveals the caster, in whole ticks.
        reveal_ticks: u16,
    },
    /// Nothing is shown.
    Hidden,
}

/// Whether a weapon's unit may move while it attacks: `StarCraft` II's `CWeapon`
/// `AllowedMovement` (plan `2026-11-23-turret-wind-up` D-1).
///
/// No `CWeapon` default names a value, so a weapon that names none has the engine's default, a
/// unit that stands to fire. Whether the weapon sits on a turret plays no part: the mount says
/// how a weapon aims, this says whether its unit may be moving when it fires.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AllowedMovement {
    /// The unit stands to fire (no `AllowedMovement`, the engine default): its wind-up holds it.
    #[default]
    Stopped,
    /// The unit may still be decelerating when it fires (`Slowing`). Treated as
    /// [`AllowedMovement::Stopped`] until the movement phase reports braking apart from pushes
    /// (bug B-178).
    Slowing,
    /// The unit fires while it moves (`Moving`): its wind-up does not hold it.
    Moving,
}

/// The strike a weapon deals to a structure in place of its usual one.
///
/// `StarCraft` II gives some weapons a separate damage effect whose search filter requires
/// `Structure`, while the usual effect excludes it: the Baneling's `VolatileBurstU2` (80, +5 a
/// level, `ArmorReduction` 0; `liberty` EffectData:1058-1063, UpgradeData:505) beside
/// `VolatileBurstU` (bug B-82). A structure struck by such a weapon takes this damage, with no
/// attribute bonus, instead of the weapon's base damage and bonuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureStrike {
    /// Damage per strike to a structure, in hit points.
    pub damage: SimScalar,
    /// Damage added per level of the weapon's damage upgrade, in hit points.
    pub upgrade_per_level: SimScalar,
    /// If true, neither life nor shield armour reduces it (`ArmorReduction` 0).
    pub ignores_armor: bool,
}

#[cfg(test)]
mod tests {
    use super::{
        Attribute, AttributeBonuses, AttributeFlags, SimScalar, SimVec2, SplashCliff, SplashImpact,
        SplashLine, SplashLineAxis, SplashLineOrigin, SplashShape, SplashSpec, SplashZone,
    };

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
            shape: SplashShape::Circle,
            friendly_fire: false,
            offset_by_unit_radius: false,
            impact: SplashImpact::Target,
            target_radius_below: None,
            cliff: SplashCliff::Same,
            strike_target_on_other_level: false,
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
            shape: SplashShape::Circle,
            friendly_fire: false,
            offset_by_unit_radius: false,
            impact: SplashImpact::Target,
            target_radius_below: None,
            cliff: SplashCliff::Same,
            strike_target_on_other_level: false,
        };
        assert_eq!(empty.fraction_at(SimScalar::ZERO), None);
        assert_eq!(empty.reach(), SimScalar::ZERO);
    }

    #[test]
    fn b18_search_points_follow_the_shape() {
        let v = |x: i32, y: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
        let line = |origin, axis| SplashSpec {
            zones: vec![SplashZone {
                radius: SimScalar::HALF,
                fraction: SimScalar::ONE,
            }],
            shape: SplashShape::Line(SplashLine {
                origin,
                axis,
                offsets: vec![SimScalar::from_i32(-1), SimScalar::from_i32(2)],
                period_ticks: 0,
            }),
            friendly_fire: false,
            offset_by_unit_radius: false,
            impact: SplashImpact::Target,
            target_radius_below: None,
            cliff: SplashCliff::Same,
            strike_target_on_other_level: false,
        };
        let (attacker, impact, east) = (v(0, 0), v(5, 0), v(1, 0));
        let mut points = vec![v(9, 9)];
        let circle = SplashSpec {
            shape: SplashShape::Circle,
            ..line(SplashLineOrigin::Target, SplashLineAxis::Along)
        };
        circle.search_points(attacker, impact, east, &mut points);
        assert_eq!(points, vec![impact]);
        line(SplashLineOrigin::Attacker, SplashLineAxis::Along).search_points(
            attacker,
            impact,
            east,
            &mut points,
        );
        assert_eq!(points, vec![v(-1, 0), v(2, 0)]);
        line(SplashLineOrigin::Target, SplashLineAxis::Across).search_points(
            attacker,
            impact,
            east,
            &mut points,
        );
        assert_eq!(points, vec![v(5, -1), v(5, 2)]);
    }

    /// Plan `2026-11-23-splash-centres` D-1, D-3: a splash written without the two fields is
    /// centred on its target and always opens, the engine defaults.
    #[test]
    fn a_splash_defaults_to_its_target_and_no_condition() -> Result<(), ron::error::SpannedError> {
        assert_eq!(SplashImpact::default(), SplashImpact::Target);
        let splash: SplashSpec = ron::from_str("(zones: [])")?;
        assert_eq!(splash.impact, SplashImpact::Target);
        assert_eq!(splash.target_radius_below, None);
        Ok(())
    }

    /// Plan `2026-11-23-splash-cliff-levels` D-1, D-3: a splash written without the two cliff
    /// fields keeps to its search point's level (`SameCliff 1`, the default of every
    /// `CEffectDamage`) and strikes no target across a cliff.
    #[test]
    fn a_splash_defaults_to_its_level_and_no_other_level_strike(
    ) -> Result<(), ron::error::SpannedError> {
        assert_eq!(SplashCliff::default(), SplashCliff::Same);
        let splash: SplashSpec = ron::from_str("(zones: [])")?;
        assert_eq!(splash.cliff, SplashCliff::Same);
        assert!(!splash.strike_target_on_other_level);
        let splash: SplashSpec =
            ron::from_str("(zones: [], cliff: Any, strike_target_on_other_level: true)")?;
        assert_eq!(splash.cliff, SplashCliff::Any);
        assert!(splash.strike_target_on_other_level);
        Ok(())
    }

    /// Plan `2026-11-23-vision-and-detection` D-4: an omitted attack visibility reads as the
    /// engine default, a snapshot; the visible case carries its reveal in whole ticks.
    #[test]
    fn attack_visibility_defaults_to_a_snapshot_and_reads_its_cases(
    ) -> Result<(), ron::error::SpannedError> {
        use super::AttackVisibility;
        assert_eq!(AttackVisibility::default(), AttackVisibility::Snapshot);
        let visible: AttackVisibility = ron::from_str("Visible(reveal_ticks: 54)")?;
        assert_eq!(visible, AttackVisibility::Visible { reveal_ticks: 54 });
        let hidden: AttackVisibility = ron::from_str("Hidden")?;
        assert_eq!(hidden, AttackVisibility::Hidden);
        Ok(())
    }
    /// Plan `2026-11-24-weapon-validation` D-1: an omitted missile validation reads as the engine
    /// default, both checks on (`CEffectLaunchMissile`, `core` EffectData:100–101); an authored
    /// row reads each flag as written.
    #[test]
    fn missile_validation_defaults_to_both_checks_and_reads_both_cases(
    ) -> Result<(), ron::error::SpannedError> {
        use super::MissileValidation;
        let both = MissileValidation {
            weapon: true,
            teleport: true,
        };
        assert_eq!(MissileValidation::default(), both);
        let at_impact: MissileValidation = ron::from_str("(weapon: false, teleport: true)")?;
        assert_eq!(
            at_impact,
            MissileValidation {
                weapon: false,
                teleport: true,
            }
        );
        let follows_teleports: MissileValidation =
            ron::from_str("(weapon: true, teleport: false)")?;
        assert_eq!(
            follows_teleports,
            MissileValidation {
                weapon: true,
                teleport: false,
            }
        );
        Ok(())
    }

    /// A missile weapon with every field set, for the RON round trips below.
    fn missile_weapon() -> super::WeaponSpec {
        use super::{
            AllowedMovement, AttackVisibility, MissileValidation, ProjectileTargetLossBehavior,
            TurretIdle, WeaponSpec,
        };
        use super::{SimAngle, WeaponVisualKind};
        WeaponSpec {
            name: "scratch missile".to_owned(),
            visual_kind: WeaponVisualKind::Projectile,
            target_flags: AttributeFlags::all(),
            min_range: SimScalar::ZERO,
            max_range: SimScalar::from_i32(6),
            firing_arc: SimAngle::PI,
            acquire_arc: SimAngle::ZERO,
            has_turret: false,
            turret_turn_rate: SimAngle::ZERO,
            turret_idle: TurretIdle::Hold,
            allowed_movement: AllowedMovement::default(),
            fires_outside_attack_order: false,
            fires_at_other_targets: false,
            locks_turret_while_firing: false,
            turret_tracks_any_enemy: false,
            base_damage: SimScalar::from_i32(10),
            attribute_bonuses: AttributeBonuses::NONE,
            attacks_count: 1,
            first_strike_delay_ticks: 0,
            strike_interval_ticks: 0,
            followup_max_distance: None,
            channeled: false,
            period_ticks: 25,
            damage_point_ticks: 3,
            backswing_ticks: 9,
            random_delay_min: SimScalar::ZERO,
            random_delay_max: SimScalar::ZERO,
            splash: None,
            projectile_speed: Some(SimScalar::ONE),
            weapon_class: None,
            upgrade_bonus_per_level: SimScalar::ZERO,
            bonus_upgrade_per_level: SimScalar::ZERO,
            target_loss_behavior: ProjectileTargetLossBehavior::Fizzle,
            max_projectile_lifetime_ticks: 125,
            suicide_on_attack: false,
            structure_strike: None,
            melee: false,
            attack_visibility: AttackVisibility::Snapshot,
            missile_validation: MissileValidation::default(),
            launch_visibility: AttackVisibility::Snapshot,
            missile_movers: Vec::new(),
            magazine_launch: None,
        }
    }

    /// Plan `2026-11-28-carrier-interceptors` D-4: a weapon that leaves its magazine launch out
    /// strikes (`None`); an authored launch reads its tries and interval.
    #[test]
    fn magazine_launch_defaults_to_none_and_reads_its_fields(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use super::{MagazineLaunch, WeaponSpec};
        let text = ron::to_string(&missile_weapon())?;
        let field = "magazine_launch:None";
        assert!(text.contains(field), "premise: the field is written");
        let omitted: WeaponSpec = ron::from_str(&text.replace(&format!(",{field}"), ""))?;
        assert_eq!(omitted.magazine_launch, None);
        let read: WeaponSpec = ron::from_str(
            &text.replace(field, "magazine_launch:Some((tries:8,interval_ticks:7))"),
        )?;
        assert_eq!(
            read.magazine_launch,
            Some(MagazineLaunch {
                tries: 8,
                interval_ticks: 7
            })
        );
        Ok(())
    }

    /// Plan `2026-11-27-launch-and-spell-visibility` D-1: a weapon that leaves its launch
    /// visibility out reads the engine default of `CEffectLaunchMissile`, a snapshot (`core`
    /// EffectData:102); an authored value reads as written.
    #[test]
    fn launch_visibility_defaults_to_a_snapshot_and_reads_its_cases(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use super::{AttackVisibility, WeaponSpec};
        let text = ron::to_string(&missile_weapon())?;
        let field = "launch_visibility:Snapshot";
        assert!(text.contains(field), "premise: the field is written");
        let omitted: WeaponSpec = ron::from_str(&text.replace(&format!(",{field}"), ""))?;
        assert_eq!(omitted.launch_visibility, AttackVisibility::Snapshot);
        for (written, expected) in [
            ("launch_visibility:Hidden", AttackVisibility::Hidden),
            (
                "launch_visibility:Visible(reveal_ticks:54)",
                AttackVisibility::Visible { reveal_ticks: 54 },
            ),
        ] {
            let read: WeaponSpec = ron::from_str(&text.replace(field, written))?;
            assert_eq!(read.launch_visibility, expected, "{written}");
        }
        Ok(())
    }

    /// Plan `2026-11-27-weapon-delivery` D-2: a weapon that leaves its missile movers out
    /// flies its constant `projectile_speed` (an empty list); authored movers read in order,
    /// each with its range bound (`None` admits any distance) and its phases, a phase's
    /// omitted start speed and end read as `None`. Plan `2026-11-28-missile-movers-b52` D-2: a
    /// phase's end reads in both forms, `Flown(x)` and `Remaining(x)`.
    #[test]
    fn missile_movers_default_to_none_and_read_bands_and_phases(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use super::{MissileMover, MissilePhase, PhaseEnd, WeaponSpec};
        let text = ron::to_string(&missile_weapon())?;
        let field = "missile_movers:[]";
        assert!(text.contains(field), "premise: the field is written");
        let omitted: WeaponSpec = ron::from_str(&text.replace(&format!(",{field}"), ""))?;
        assert_eq!(omitted.missile_movers, Vec::new());
        let scalar = |raw: i64| format!("\"q32.32:v1:{raw}\"");
        let one = SimScalar::ONE.raw();
        let half = SimScalar::HALF.raw();
        let written = format!(
            "missile_movers:[(up_to_range:Some({two}),phases:[(speed:Some({half}),\
             acceleration:{zero},max_speed:{half},ends:Some(Flown({one}))),\
             (speed:None,acceleration:{half},max_speed:{two},ends:Some(Remaining({half}))),\
             (speed:None,acceleration:{zero},max_speed:{two},ends:None)]),\
             (up_to_range:None,phases:[(speed:None,acceleration:{half},max_speed:{two},\
             ends:None)])]",
            two = scalar(2 * one),
            half = scalar(half),
            zero = scalar(0),
            one = scalar(one),
        );
        let read: WeaponSpec = ron::from_str(&text.replace(field, &written))?;
        assert_eq!(
            read.missile_movers,
            vec![
                MissileMover {
                    up_to_range: Some(SimScalar::from_i32(2)),
                    phases: vec![
                        MissilePhase {
                            speed: Some(SimScalar::HALF),
                            acceleration: SimScalar::ZERO,
                            max_speed: SimScalar::HALF,
                            ends: Some(PhaseEnd::Flown(SimScalar::ONE)),
                        },
                        MissilePhase {
                            speed: None,
                            acceleration: SimScalar::HALF,
                            max_speed: SimScalar::from_i32(2),
                            ends: Some(PhaseEnd::Remaining(SimScalar::HALF)),
                        },
                        MissilePhase {
                            speed: None,
                            acceleration: SimScalar::ZERO,
                            max_speed: SimScalar::from_i32(2),
                            ends: None,
                        },
                    ],
                },
                MissileMover {
                    up_to_range: None,
                    phases: vec![MissilePhase {
                        speed: None,
                        acceleration: SimScalar::HALF,
                        max_speed: SimScalar::from_i32(2),
                        ends: None,
                    }],
                },
            ]
        );
        Ok(())
    }
}
