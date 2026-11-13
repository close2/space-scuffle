//! Authored spell data: effect-tree nodes, statuses, target filters and effect contexts.
//!
//! # Responsibility
//! Declares the serde shapes of `assets/catalog/ability_effects.ron` and
//! `assets/catalog/statuses.ron` (plan `2026-11-10-spellcaster-mechanics`, Amendment 1, D-1,
//! D-3, D-20) and the plain values the effect engine stores in hashed state: the identities
//! [`EffectId`] and [`StatusId`], and the target context [`EffectContext`] (D-15).
//!
//! # How it fits
//! `sim_core::catalog` parses these shapes, resolves every string key, flattens the trees into
//! an arena and validates them; the engine in `sim_core::effects` evaluates the arena. Nothing
//! here names a spell, a unit or a status: they are catalog data (charter §3.3, C5).
//!
//! # Invariants
//! - Every node kind is the counterpart of a `StarCraft` II editor effect class (`CEffectSet`,
//!   `CEffectEnumArea`, `CEffectCreatePersistent`, `CEffectLaunchMissile`, `CEffectDamage`,
//!   `CEffectApplyBehavior`, `CEffectRemoveBehavior`, `CEffectModifyUnit`, `CEffectCancelOrder`,
//!   and a target-launching missile for [`DisplaceDef`]).
//! - Times are 25 Hz ticks as fixed-point values, distances world units, speeds world units per
//!   tick. No float appears in an authored rule value.
//! - Omitted fields take the neutral default, so a filter written `()` admits every unit.

use crate::ids::{EntityId, PlayerId};
use crate::math::{SimScalar, SimVec2};
use crate::presentation::{AbilitySource, StatusKind};
use crate::weapon::Attribute;
use serde::{Deserialize, Serialize};

/// Dense identity of one node of the flattened effect arena, in catalog order (named nodes in
/// name order, then inline nodes depth-first under their named parent). Stable for a catalog.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct EffectId(pub u16);

/// Dense identity of one status of `statuses.ron`, in name order. Stable for a catalog.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct StatusId(pub u16);

/// Which owners a [`TargetFilter`] admits, relative to the context's owner (the caster's owner
/// at the cast).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FilterAlliance {
    /// Every owner, the caster itself included.
    #[default]
    Any,
    /// Every owner, but not the casting unit itself.
    AnyButSelf,
    /// Units of another owner only.
    Enemy,
    /// Units of the context's owner only, the caster included.
    Friendly,
}

/// A locomotion plane a [`TargetFilter`] admits.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TargetPlane {
    /// Ground and cliff-walking units.
    Ground,
    /// Air units.
    Air,
}

/// How a [`TargetFilter`] treats structures.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StructureRule {
    /// Structures and units alike.
    #[default]
    Include,
    /// Units only.
    Exclude,
    /// Structures only.
    Only,
}

fn both_planes() -> Vec<TargetPlane> {
    vec![TargetPlane::Ground, TargetPlane::Air]
}

/// A unit predicate: the counterpart of the editor's search filters and unit validators.
///
/// Dead, hidden (loaded) and invulnerable units never pass any filter; the filter narrows
/// further. Omitted fields admit everything.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetFilter {
    /// Owners admitted, relative to the context's owner.
    #[serde(default)]
    pub alliance: FilterAlliance,
    /// Planes admitted; both when omitted.
    #[serde(default = "both_planes")]
    pub planes: Vec<TargetPlane>,
    /// Whether structures are admitted, excluded or required.
    #[serde(default)]
    pub structures: StructureRule,
    /// A status key (`statuses.ron`) the unit must not carry (the game's `NoYoink`-style
    /// validators).
    #[serde(default)]
    pub without_status: Option<String>,
    /// Attributes none of which the unit may have (the game's `-;Massive`-style filter
    /// exclusions); empty admits every unit.
    #[serde(default)]
    pub without_attributes: Vec<Attribute>,
}

impl Default for TargetFilter {
    fn default() -> Self {
        Self {
            alliance: FilterAlliance::Any,
            planes: both_planes(),
            structures: StructureRule::Include,
            without_status: None,
            without_attributes: Vec::new(),
        }
    }
}

/// Where a persistent sits or a missile flies to.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectAnchor {
    /// The context's point (a unit target's position at the moment the node runs).
    TargetPoint,
    /// The context's unit, followed while it lives.
    TargetUnit,
}

/// How a missile is presented; the simulation treats both alike.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MissileView {
    /// A projectile drawn at the missile's position.
    #[default]
    Projectile,
    /// A tether drawn from the caster to the target.
    Tether,
}

/// Where a [`DisplaceDef`] moves its unit.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DisplaceDestination {
    /// The context's origin: the ability's placed point (`AbilitySpec::origin`).
    #[default]
    Origin,
}

/// The damage responses of a [`DamageDef`] (`CEffectDamage` `ResponseFlags`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageResponses {
    /// The victim retaliates against the context's caster.
    #[serde(default)]
    pub acquire: bool,
    /// An idle victim that cannot retaliate flees from the context's source point.
    #[serde(default)]
    pub flee: bool,
}

/// `Set`: runs 1 to 8 children in authored order (`CEffectSet`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetDef {
    /// Children, run depth-first in this order.
    pub children: Vec<EffectNodeDef>,
    /// Unit validator checked before the node runs.
    #[serde(default)]
    pub require: Option<TargetFilter>,
}

/// `Search`: runs `child` once per unit within `radius` of its centre (`CEffectEnumArea`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchDef {
    /// Search radius in world units, measured to unit centres; `0 < radius ≤ 16`.
    pub radius: SimScalar,
    /// Units admitted.
    #[serde(default)]
    pub filter: TargetFilter,
    /// Greatest number of units the child runs on, nearest first; all when omitted.
    #[serde(default)]
    pub max_count: Option<u16>,
    /// The node run for each unit found.
    pub child: Box<EffectNodeDef>,
    /// Unit validator checked before the node runs.
    #[serde(default)]
    pub require: Option<TargetFilter>,
}

/// `Persistent`: an instance that runs `initial` at once and `periodic` at `count` pulses
/// (`CEffectCreatePersistent`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentDef {
    /// Where the persistent sits.
    pub anchor: EffectAnchor,
    /// Node run when the persistent is created, in the same walk.
    #[serde(default)]
    pub initial: Option<Box<EffectNodeDef>>,
    /// Node run at each pulse.
    #[serde(default)]
    pub periodic: Option<Box<EffectNodeDef>>,
    /// Exact ticks between pulses (pulse k fires `round_half_up(k × period)` ticks after
    /// creation); at least 1 when `count ≥ 1`.
    #[serde(default)]
    pub period_ticks: SimScalar,
    /// Number of periodic pulses, `0..=1024`.
    #[serde(default)]
    pub count: u16,
    /// Unit validator checked before the node runs.
    #[serde(default)]
    pub require: Option<TargetFilter>,
}

/// `LaunchMissile`: a missile from the caster to the target that runs `impact` on arrival
/// (`CEffectLaunchMissile`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchMissileDef {
    /// The point (fixed) or the unit (homing) the missile flies to.
    pub to: EffectAnchor,
    /// Speed in world units per tick, `0.01..=16`.
    pub speed: SimScalar,
    /// Presentation of the missile.
    #[serde(default)]
    pub view: MissileView,
    /// Node run at the target on arrival.
    pub impact: Box<EffectNodeDef>,
    /// Unit validator checked before the node runs.
    #[serde(default)]
    pub require: Option<TargetFilter>,
}

/// `Damage`: spell damage to the target unit, shields first, no armour (`CEffectDamage`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageDef {
    /// Damage points, non-negative. 0 deals nothing and only raises the responses.
    pub amount: SimScalar,
    /// Extra damage per attribute the target has.
    #[serde(default)]
    pub attribute_bonus: Vec<(Attribute, SimScalar)>,
    /// The victim's responses.
    #[serde(default)]
    pub responses: DamageResponses,
    /// Unit validator checked before the node runs.
    #[serde(default)]
    pub require: Option<TargetFilter>,
}

/// `ApplyStatus` or `RemoveStatus`: a status key of `statuses.ron`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusRefDef {
    /// The status key.
    pub status: String,
    /// Unit validator checked before the node runs.
    #[serde(default)]
    pub require: Option<TargetFilter>,
}

/// One vital's change of a [`ModifyVitalsDef`]: `change + change_fraction × current`, then the
/// vital is clamped to `0..=max`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VitalChange {
    /// Points added (negative to remove).
    #[serde(default)]
    pub change: SimScalar,
    /// Fraction of the current value added, `-1..=1`.
    #[serde(default)]
    pub change_fraction: SimScalar,
}

/// `ModifyVitals`: changes life, shields or energy of the target unit (`CEffectModifyUnit`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModifyVitalsDef {
    /// Change to life.
    #[serde(default)]
    pub life: Option<VitalChange>,
    /// Change to shields.
    #[serde(default)]
    pub shields: Option<VitalChange>,
    /// Change to energy.
    #[serde(default)]
    pub energy: Option<VitalChange>,
    /// Unit validator checked before the node runs.
    #[serde(default)]
    pub require: Option<TargetFilter>,
}

/// `CancelOrders`: clears the target's orders, hold, target, route and weapon swing
/// (`CEffectCancelOrder`); a mode channel continues.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelOrdersDef {
    /// Whether a burrowed mode is left at once, with no channel (`InstantUnburrow`).
    #[serde(default)]
    pub force_unburrow: bool,
    /// Unit validator checked before the node runs.
    #[serde(default)]
    pub require: Option<TargetFilter>,
}

/// `Displace`: flies the target unit in a straight line to `to` over `flight_ticks`, then runs
/// `on_land` on it (a missile that launches the target unit).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplaceDef {
    /// The destination.
    #[serde(default)]
    pub to: DisplaceDestination,
    /// Flight time in ticks, `1..=250`.
    pub flight_ticks: u16,
    /// Whether a tether is presented from the caster during the flight.
    #[serde(default)]
    pub tether: bool,
    /// Node run on the unit when it lands.
    #[serde(default)]
    pub on_land: Option<Box<EffectNodeDef>>,
    /// Unit validator checked before the node runs.
    #[serde(default)]
    pub require: Option<TargetFilter>,
}

/// One authored effect node, written inline or referenced by name with [`Self::Use`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectNodeDef {
    /// Runs children in order.
    Set(SetDef),
    /// Runs a child per unit found around a point.
    Search(SearchDef),
    /// Creates a pulsing instance.
    Persistent(PersistentDef),
    /// Launches a missile.
    LaunchMissile(LaunchMissileDef),
    /// Deals spell damage.
    Damage(DamageDef),
    /// Applies a status.
    ApplyStatus(StatusRefDef),
    /// Removes a status.
    RemoveStatus(StatusRefDef),
    /// Changes vitals.
    ModifyVitals(ModifyVitalsDef),
    /// Cancels the target's orders.
    CancelOrders(CancelOrdersDef),
    /// Moves the target unit.
    Displace(DisplaceDef),
    /// A reference to a named node, resolved and erased at load.
    Use(String),
}

/// One named entry of `ability_effects.ron`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectDef {
    /// Unique key other entries, statuses and abilities name it by.
    pub name: String,
    /// The node.
    pub node: EffectNodeDef,
}

fn one() -> SimScalar {
    SimScalar::ONE
}

/// The default floor of a reduced weapon range: 0.1 world units, the game's melee range
/// (plan SU-2). Computed by division so no float enters.
fn default_range_floor() -> SimScalar {
    SimScalar::ONE.saturating_div(SimScalar::from_i32(10))
}

/// A timed per-unit effect of `statuses.ron` (`CBehaviorBuff`; plan D-3, D-20).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent modification flags of one buff, each read by a different system"
)]
pub struct StatusSpec {
    /// Unique key effects name it by.
    pub name: String,
    /// Exact lifetime in ticks from the (re)application; positive.
    pub duration_ticks: SimScalar,
    /// Exact ticks between pulses; present exactly when `periodic_effect` is, at least 1.
    #[serde(default)]
    pub period_ticks: Option<SimScalar>,
    /// Effect key run on the bearer at the first application.
    #[serde(default)]
    pub initial_effect: Option<String>,
    /// Effect key run on the bearer at each pulse.
    #[serde(default)]
    pub periodic_effect: Option<String>,
    /// Movement speed multiplier, `0..=1`.
    #[serde(default = "one")]
    pub move_speed_factor: SimScalar,
    /// Range change of ranged (non-melee) weapons, world units.
    #[serde(default)]
    pub ranged_weapon_range_delta: SimScalar,
    /// Range change of every weapon, world units.
    #[serde(default)]
    pub weapon_range_delta: SimScalar,
    /// The least range a reduction leaves, world units.
    #[serde(default = "default_range_floor")]
    pub range_floor: SimScalar,
    /// The bearer cannot move, attack or cast.
    #[serde(default)]
    pub stun: bool,
    /// The bearer is seen through cloak and burrow.
    #[serde(default)]
    pub reveal: bool,
    /// The bearer does not detect.
    #[serde(default)]
    pub suppress_detection: bool,
    /// The bearer neither pushes nor is pushed.
    #[serde(default)]
    pub suppress_collision: bool,
    /// The bearer cannot board a transport.
    #[serde(default)]
    pub suppress_boarding: bool,
    /// The bearer cannot blink.
    #[serde(default)]
    pub suppress_blink: bool,
    /// Presentation flags the status raises.
    #[serde(default)]
    pub presentation: Vec<StatusKind>,
}

/// Placement of an ability's origin near the caster (plan D-6; `StarCraft` II's placeholder
/// `CEffectCreateUnit` with `SpawnOffset` and `SpawnRange`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginPlacement {
    /// Distance in world units ahead of the caster along its heading where the search starts.
    pub forward_offset: SimScalar,
    /// Greatest distance in world units from that start a placement may take.
    pub search_range: SimScalar,
}

/// What an effect node acts on (plan D-15).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectTarget {
    /// A point in world units.
    Point(SimVec2),
    /// A unit.
    Unit(EntityId),
}

/// The locations an effect node runs with: the counterpart of the editor's caster, source,
/// target and origin (plan D-15). Hashed wherever an instance stores it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectContext {
    /// The caster's owner at the cast; alliance filters are relative to it.
    pub owner: PlayerId,
    /// The casting unit, if any.
    pub caster: Option<EntityId>,
    /// The ability that started the tree.
    pub ability: AbilitySource,
    /// The point captured at the cast: the ability's placed origin, else the caster's position.
    pub origin: SimVec2,
    /// Where the current target was produced from (a search's centre, a missile's contact
    /// point); the flee origin of damage responses.
    pub source_point: SimVec2,
    /// What the node acts on.
    pub target: EffectTarget,
}

impl EffectContext {
    /// The same context acting on `target`, produced from `source_point`.
    #[must_use]
    pub const fn retarget(self, target: EffectTarget, source_point: SimVec2) -> Self {
        Self {
            source_point,
            target,
            ..self
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn omitted_filter_fields_admit_everything() {
        let filter: TargetFilter = ron::from_str("()").expect("invariant: a valid empty filter");
        assert_eq!(filter, TargetFilter::default());
        assert_eq!(filter.planes, vec![TargetPlane::Ground, TargetPlane::Air]);
    }

    #[test]
    fn a_nested_tree_parses_with_inline_nodes_and_references() {
        let text = r#"(name: "root", node: LaunchMissile((
            to: TargetPoint, speed: 0.672, view: Projectile,
            impact: Search((radius: 2.25, filter: (alliance: Enemy, structures: Exclude),
                child: Set((children: [ApplyStatus((status: "s")), Use("other"),
                    Damage((amount: 0.0, responses: (acquire: true, flee: true)))])))))))"#;
        let def: EffectDef = ron::from_str(text).expect("invariant: a valid tree");
        let missile = match def.node {
            EffectNodeDef::LaunchMissile(missile) => Some(missile),
            _ => None,
        }
        .expect("invariant: the root is a missile");
        let search = match *missile.impact {
            EffectNodeDef::Search(search) => Some(search),
            _ => None,
        }
        .expect("invariant: the impact is a search");
        assert_eq!(search.filter.alliance, FilterAlliance::Enemy);
        assert_eq!(search.filter.structures, StructureRule::Exclude);
        let set = match *search.child {
            EffectNodeDef::Set(set) => Some(set),
            _ => None,
        }
        .expect("invariant: the child is a set");
        assert_eq!(set.children.len(), 3);
        assert_eq!(set.children[1], EffectNodeDef::Use("other".to_owned()));
    }

    #[test]
    fn status_defaults_are_neutral() {
        let spec: StatusSpec =
            ron::from_str(r#"(name: "s", duration_ticks: 11.901786)"#).expect("invariant: valid");
        assert_eq!(spec.move_speed_factor, SimScalar::ONE);
        assert_eq!(spec.range_floor, default_range_floor());
        assert!(!spec.stun && !spec.reveal && spec.period_ticks.is_none());
    }

    #[test]
    fn retarget_keeps_owner_caster_ability_and_origin() {
        let context = EffectContext {
            owner: PlayerId(1),
            caster: Some(EntityId(4)),
            ability: AbilitySource::default(),
            origin: SimVec2::from_i32(1, 2),
            source_point: SimVec2::from_i32(3, 4),
            target: EffectTarget::Point(SimVec2::from_i32(5, 6)),
        };
        let moved = context.retarget(EffectTarget::Unit(EntityId(9)), SimVec2::from_i32(7, 8));
        assert_eq!(moved.owner, context.owner);
        assert_eq!(moved.caster, context.caster);
        assert_eq!(moved.origin, context.origin);
        assert_eq!(moved.target, EffectTarget::Unit(EntityId(9)));
        assert_eq!(moved.source_point, SimVec2::from_i32(7, 8));
    }
}
