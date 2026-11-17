//! The units' choice of effects: each archetype's presentation block resolved against the
//! vocabulary into dense tables (plan §5.3).
//!
//! # Responsibility
//! Owns [`UnitEffectTables`]: per [`UnitTypeId`], the effects of each weapon (by
//! [`WeaponKey`]: slot and mode index), of each status, of each mode, of each ability's area or
//! tether (by the archetype's ability index, plan D-8, WP10) and of death, with the
//! vocabulary's defaults filled in where the block names nothing.
//!
//! # How it fits
//! Built once per session from the unit catalog (`sim_core::catalog::ArchetypeEffects`, read
//! from `units.ron` and `structures.ron`) and the [`EffectCatalog`]. The event wiring looks an
//! event's [`EffectSource`] up here: one index and a scan of the unit's few weapons.
//!
//! # Invariants
//! - Every name in every block resolves, every mode name is one of the archetype's modes, every
//!   ability name is one of the archetype's abilities, and every effect is made for the kind of
//!   event its slot receives.

use super::error::EffectCatalogError;
use super::schema::EventKind;
use super::{ConcealmentDefaults, EffectCatalog, EffectId};
use sim_core::catalog::{ArchetypeEffects, EffectNode, SpellCatalog, UnitArchetype, UnitCatalog};
use sim_types::EffectId as SpellEffectId;
use sim_types::{AbilitySource, EffectSource, StatusKind, UnitTypeId, WeaponKey};

/// Event kinds a splash slot accepts.
const SPLASH_KINDS: &[EventKind] = &[EventKind::Splash, EventKind::Line];
/// Event kinds an impact or launch slot accepts.
const POINT_KINDS: &[EventKind] = &[EventKind::Point];
/// Event kinds a travel slot accepts: the shot from the muzzle to where it lands (plan WP11).
const SHOT_KINDS: &[EventKind] = &[EventKind::Shot];
/// Event kinds a death slot accepts.
const DEATH_KINDS: &[EventKind] = &[EventKind::Death];
/// Event kinds a status slot accepts.
const STATUS_KINDS: &[EventKind] = &[EventKind::Status];
/// Event kinds a mode slot accepts.
const MODE_KINDS: &[EventKind] = &[EventKind::Mode];
/// Event kinds an ability slot accepts: the area, the tether or the missile the ability makes.
const ABILITY_KINDS: &[EventKind] = &[EventKind::Area, EventKind::Link, EventKind::Missile];

/// Every status kind, in the order of [`status_index`].
pub const STATUS_ORDER: [StatusKind; 8] = [
    StatusKind::Overclock,
    StatusKind::Dash,
    StatusKind::Slowed,
    StatusKind::Barrier,
    StatusKind::DamageOverTime,
    StatusKind::Stunned,
    StatusKind::RangeReduced,
    StatusKind::Revealed,
];

/// Dense index of a status kind.
#[must_use]
pub const fn status_index(kind: StatusKind) -> usize {
    match kind {
        StatusKind::Overclock => 0,
        StatusKind::Dash => 1,
        StatusKind::Slowed => 2,
        StatusKind::Barrier => 3,
        StatusKind::DamageOverTime => 4,
        StatusKind::Stunned => 5,
        StatusKind::RangeReduced => 6,
        StatusKind::Revealed => 7,
    }
}

/// The effects of one weapon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponEffectIds {
    /// Slot and mode of the weapon.
    pub key: WeaponKey,
    /// At the muzzle.
    pub launch: Option<EffectId>,
    /// The shot from the muzzle to where it lands: a tracer's or a beam's path, a
    /// projectile's flight (a `Shot` effect).
    pub travel: Option<EffectId>,
    /// Where the shot lands.
    pub impact: Option<EffectId>,
    /// Each splash search.
    pub splash: Option<EffectId>,
    /// Seconds after an instant shot (a tracer or a beam) that its impact is laid: when the
    /// travel's `Critical` riding strip (its slug) reaches the target, so the round arrives
    /// before its hit (plan WP11 review 1, E1-1); 0 without one.
    pub impact_delay: f32,
}

/// The effects of one mode, by the archetype's mode index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeEffectIds {
    /// Index in the archetype's `modes`.
    pub mode: u8,
    /// Channel into the mode.
    pub enter: Option<EffectId>,
    /// Channel out of the mode.
    pub leave: Option<EffectId>,
    /// Either channel.
    pub transition: Option<EffectId>,
    /// Held in the mode.
    pub stance: Option<EffectId>,
    /// Laid at its `repeat` period while the unit moves in the mode (a trail).
    pub moving: Option<EffectId>,
}

/// One archetype's resolved block.
#[derive(Debug, Clone, Default, PartialEq)]
struct UnitEntry {
    /// Weapons named by the block.
    weapons: Vec<WeaponEffectIds>,
    /// Modes named by the block.
    modes: Vec<ModeEffectIds>,
    /// Status effects by [`status_index`], defaults filled in.
    status: [Option<EffectId>; STATUS_ORDER.len()],
    /// Death effect, default filled in.
    death: Option<EffectId>,
    /// Area, tether or missile effects, by index in the archetype's `abilities`.
    abilities: Vec<AbilityLook>,
    /// The archetype's bubble radius in world units (its collision radius, which the
    /// simulation's snapshot draws the bubble with): where its shots leave its bubble.
    muzzle: f32,
}

/// The look one ability names, with what a missile's impact binds.
#[derive(Debug, Clone, Copy, PartialEq)]
struct AbilityLook {
    /// Index in the archetype's `abilities`.
    ability: u8,
    /// The effect.
    effect: EffectId,
    /// For a missile: its impact's extent ([`MissileExtent`]).
    extent: MissileExtent,
}

/// What a spell missile's impact covers, read from the spell catalog (plan WP11, F1): the
/// radius of the first search its ability's effect tree runs (`EffectFacts::targeting_radius`)
/// and the duration of the longest status that tree applies.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MissileExtent {
    /// Radius in world units; 0 without a search.
    pub radius: f32,
    /// Seconds; 0 without a status.
    pub seconds: f32,
}

/// Per-archetype effect choices, indexed by [`UnitTypeId`].
#[derive(Debug, Clone, Default, PartialEq, bevy::prelude::Resource)]
pub struct UnitEffectTables {
    /// Entries by unit type id; ids absent from the catalog have an empty entry.
    units: Vec<UnitEntry>,
    /// Default splash effect.
    default_splash: Option<EffectId>,
    /// Default impact effect.
    default_impact: Option<EffectId>,
    /// How concealed units look, from the vocabulary's defaults.
    concealment: ConcealmentDefaults,
}

impl UnitEffectTables {
    /// Resolves every archetype's presentation block in `units` against `effects`.
    ///
    /// Every mode of every archetype gets an entry: a burrowed mode (`burrowed: true`) starts
    /// from the vocabulary's `concealment` defaults (`burrow_enter`, `burrow_leave`,
    /// `burrowed`), any other mode that anchors its unit (`is_immobile`) from its `mode`
    /// defaults (`transition`, `anchored`), and each slot the unit's block names replaces its
    /// default.
    ///
    /// # Errors
    /// [`EffectCatalogError::UnknownUnitEffect`], [`EffectCatalogError::UnknownUnitMode`] or
    /// [`EffectCatalogError::WrongEventKind`].
    pub fn build(units: &UnitCatalog, effects: &EffectCatalog) -> Result<Self, EffectCatalogError> {
        let defaults = &effects.defaults;
        let mut tables = Self {
            units: Vec::new(),
            default_splash: defaults.splash,
            default_impact: defaults.impact,
            concealment: defaults.concealment,
        };
        let check = Checker {
            effects,
            unit: "defaults",
        };
        check.kind("splash", defaults.splash, SPLASH_KINDS)?;
        check.kind("impact", defaults.impact, POINT_KINDS)?;
        check.kind("death", defaults.death, DEATH_KINDS)?;
        for id in defaults.status.values() {
            check.kind("status", Some(*id), STATUS_KINDS)?;
        }
        let concealment = &defaults.concealment;
        for (slot, id) in [
            ("concealment burrow_enter", concealment.burrow_enter),
            ("concealment burrow_leave", concealment.burrow_leave),
            ("concealment burrowed", concealment.burrowed),
            ("concealment cloaked_own", concealment.cloaked_own),
            ("concealment cloaked_enemy", concealment.cloaked_enemy),
            ("concealment detected", concealment.detected),
            ("mode transition", defaults.anchor_transition),
            ("mode anchored", defaults.anchored),
        ] {
            check.kind(slot, id, MODE_KINDS)?;
        }
        for archetype in units.archetypes() {
            let entry = resolve_unit(archetype, effects, units.spells())?;
            let index = usize::from(archetype.unit_type_id.0);
            if tables.units.len() <= index {
                tables.units.resize(index + 1, UnitEntry::default());
            }
            tables.units[index] = entry;
        }
        Ok(tables)
    }

    /// The weapon entry of `source`, if its unit's block names one.
    fn weapon(&self, source: EffectSource) -> Option<&WeaponEffectIds> {
        self.units
            .get(usize::from(source.unit_type.0))?
            .weapons
            .iter()
            .find(|weapon| weapon.key == source.weapon)
    }

    /// The splash effect of `source`, or the default.
    #[must_use]
    pub fn splash(&self, source: EffectSource) -> Option<EffectId> {
        self.weapon(source)
            .and_then(|weapon| weapon.splash)
            .or(self.default_splash)
    }

    /// The travel effect of `source`'s shot (a `Shot` effect), if its unit's block names one;
    /// there is no default.
    #[must_use]
    pub fn travel(&self, source: EffectSource) -> Option<EffectId> {
        self.weapon(source).and_then(|weapon| weapon.travel)
    }

    /// Whether `source`'s unit names the looks of `source`'s weapon in its block (plan WP11):
    /// then the effect system draws that weapon's shots, and the legacy combat card of its
    /// event kind is not drawn, even for a slot the entry leaves empty (a weapon whose whole
    /// look is its splash, such as a flame jet, draws no tracer).
    #[must_use]
    pub fn names_weapon(&self, source: EffectSource) -> bool {
        self.weapon(source).is_some()
    }

    /// Where a shot of a unit of type `unit` leaves its bubble: the bubble's radius in world
    /// units (0 for a type the catalog does not have).
    #[must_use]
    pub fn muzzle(&self, unit: UnitTypeId) -> f32 {
        self.units
            .get(usize::from(unit.0))
            .map_or(0.0, |entry| entry.muzzle)
    }

    /// Seconds after an instant shot of `source` that its impact is laid (see
    /// [`WeaponEffectIds::impact_delay`]).
    #[must_use]
    pub fn impact_delay(&self, source: EffectSource) -> f32 {
        self.weapon(source)
            .map_or(0.0, |weapon| weapon.impact_delay)
    }

    /// The impact effect of `source`, or the default.
    #[must_use]
    pub fn impact(&self, source: EffectSource) -> Option<EffectId> {
        self.weapon(source)
            .and_then(|weapon| weapon.impact)
            .or(self.default_impact)
    }

    /// The death effect of `unit`, or the default.
    #[must_use]
    pub fn death(&self, unit: UnitTypeId) -> Option<EffectId> {
        self.units
            .get(usize::from(unit.0))
            .and_then(|entry| entry.death)
    }

    /// The status effect `kind` of `unit`, or the default.
    #[must_use]
    pub fn status(&self, unit: UnitTypeId, kind: StatusKind) -> Option<EffectId> {
        self.units
            .get(usize::from(unit.0))
            .and_then(|entry| entry.status[status_index(kind)])
    }

    /// Whether any unit names a `moving` trail, so the trail system has work at all.
    #[must_use]
    pub fn has_trails(&self) -> bool {
        self.units
            .iter()
            .any(|entry| entry.modes.iter().any(|mode| mode.moving.is_some()))
    }

    /// How concealed units look: the vocabulary's `concealment` defaults.
    #[must_use]
    pub const fn concealment(&self) -> &ConcealmentDefaults {
        &self.concealment
    }

    /// The effect of the area or tether `source`'s ability makes, if its caster's block names
    /// one (no default: an ability without a look draws nothing, D-8).
    #[must_use]
    pub fn ability(&self, source: AbilitySource) -> Option<EffectId> {
        self.ability_look(source).map(|look| look.effect)
    }

    /// What the missile `source`'s ability launches covers where it lands (plan WP11, F1): the
    /// default extent for an ability without a look or without a search.
    #[must_use]
    pub fn missile_extent(&self, source: AbilitySource) -> MissileExtent {
        self.ability_look(source).map_or_default(|look| look.extent)
    }

    /// The look `source`'s ability names, if any.
    fn ability_look(&self, source: AbilitySource) -> Option<&AbilityLook> {
        self.units
            .get(usize::from(source.unit_type.0))?
            .abilities
            .iter()
            .find(|look| look.ability == source.ability)
    }

    /// The mode effects of `unit` for mode index `mode`, defaults filled in.
    #[must_use]
    pub fn mode(&self, unit: UnitTypeId, mode: u8) -> Option<ModeEffectIds> {
        self.units
            .get(usize::from(unit.0))?
            .modes
            .iter()
            .find(|entry| entry.mode == mode)
            .copied()
    }
}

/// Resolves names of one unit's block.
struct Checker<'a> {
    /// The vocabulary.
    effects: &'a EffectCatalog,
    /// Slug of the unit, or `defaults`.
    unit: &'a str,
}

impl Checker<'_> {
    /// Resolves an optional name for `slot`, checking its event kind.
    fn name(
        &self,
        slot: &str,
        name: Option<&String>,
        kinds: &'static [EventKind],
    ) -> Result<Option<EffectId>, EffectCatalogError> {
        let Some(name) = name else {
            return Ok(None);
        };
        let id =
            self.effects
                .effect_id(name)
                .ok_or_else(|| EffectCatalogError::UnknownUnitEffect {
                    path: self.effects.path.clone(),
                    unit: self.unit.to_owned(),
                    slot: slot.to_owned(),
                    name: name.clone(),
                })?;
        self.kind(slot, Some(id), kinds)?;
        Ok(Some(id))
    }

    /// Resolves a mode's `moving` slot: a `Mode` effect that states its `repeat` period.
    fn trail(&self, name: Option<&String>) -> Result<Option<EffectId>, EffectCatalogError> {
        let id = self.name("mode moving", name, MODE_KINDS)?;
        match id.and_then(|id| self.effects.effect(id)) {
            Some(effect) if effect.repeat.is_none() => {
                Err(EffectCatalogError::TrailWithoutRepeat {
                    unit: self.unit.to_owned(),
                    name: effect.name.clone(),
                })
            }
            _ => Ok(id),
        }
    }

    /// Fails unless effect `id` is made for one of `kinds`.
    fn kind(
        &self,
        slot: &str,
        id: Option<EffectId>,
        kinds: &'static [EventKind],
    ) -> Result<(), EffectCatalogError> {
        let Some(effect) = id.and_then(|id| self.effects.effect(id)) else {
            return Ok(());
        };
        if kinds.contains(&effect.event) {
            return Ok(());
        }
        Err(EffectCatalogError::WrongEventKind {
            unit: self.unit.to_owned(),
            slot: slot.to_owned(),
            name: effect.name.clone(),
            expected: kinds,
            found: effect.event,
        })
    }
}

/// The index of mode `name` in `archetype`'s modes.
fn mode_index(archetype: &UnitArchetype, name: &str) -> Result<u8, EffectCatalogError> {
    archetype
        .modes
        .iter()
        .position(|mode| mode.mode_name == name)
        .and_then(|index| u8::try_from(index).ok())
        .ok_or_else(|| EffectCatalogError::UnknownUnitMode {
            unit: archetype.slug.clone(),
            mode: name.to_owned(),
        })
}

/// Resolves one archetype's block, filling defaults for status and death.
fn resolve_unit(
    archetype: &UnitArchetype,
    effects: &EffectCatalog,
    spells: &SpellCatalog,
) -> Result<UnitEntry, EffectCatalogError> {
    let block: &ArchetypeEffects = &archetype.effects;
    let check = Checker {
        effects,
        unit: &archetype.slug,
    };
    let mut entry = UnitEntry {
        muzzle: archetype.collision_radius.to_f32(),
        ..UnitEntry::default()
    };
    for weapon in &block.weapons {
        let mode = weapon
            .mode
            .as_deref()
            .map(|name| mode_index(archetype, name))
            .transpose()?;
        let travel = check.name("weapon travel", weapon.travel.as_ref(), SHOT_KINDS)?;
        entry.weapons.push(WeaponEffectIds {
            key: WeaponKey {
                slot: weapon.weapon,
                mode,
            },
            launch: check.name("weapon launch", weapon.launch.as_ref(), POINT_KINDS)?,
            travel,
            impact_delay: travel
                .and_then(|id| effects.effect(id))
                .map_or(0.0, slug_arrival),
            impact: check.name("weapon impact", weapon.impact.as_ref(), POINT_KINDS)?,
            splash: check.name("weapon splash", weapon.splash.as_ref(), SPLASH_KINDS)?,
        });
    }
    let defaults = &effects.defaults;
    for (index, spec) in archetype.modes.iter().enumerate() {
        let Ok(mode) = u8::try_from(index) else {
            break;
        };
        let (enter, leave, transition, stance) = if spec.burrowed {
            let concealment = &defaults.concealment;
            (
                concealment.burrow_enter,
                concealment.burrow_leave,
                None,
                concealment.burrowed,
            )
        } else if spec.is_immobile {
            (None, None, defaults.anchor_transition, defaults.anchored)
        } else {
            (None, None, None, None)
        };
        entry.modes.push(ModeEffectIds {
            mode,
            enter,
            leave,
            transition,
            stance,
            moving: None,
        });
    }
    for mode in &block.modes {
        let index = mode_index(archetype, &mode.mode)?;
        let resolved = ModeEffectIds {
            mode: index,
            enter: check.name("mode enter", mode.enter.as_ref(), MODE_KINDS)?,
            leave: check.name("mode leave", mode.leave.as_ref(), MODE_KINDS)?,
            transition: check.name("mode transition", mode.transition.as_ref(), MODE_KINDS)?,
            stance: check.name("mode stance", mode.stance.as_ref(), MODE_KINDS)?,
            moving: check.trail(mode.moving.as_ref())?,
        };
        if let Some(slot) = entry.modes.iter_mut().find(|slot| slot.mode == index) {
            slot.enter = resolved.enter.or(slot.enter);
            slot.leave = resolved.leave.or(slot.leave);
            slot.transition = resolved.transition.or(slot.transition);
            slot.stance = resolved.stance.or(slot.stance);
            slot.moving = resolved.moving.or(slot.moving);
        }
    }
    for kind in STATUS_ORDER {
        let own = check.name("status", block.status.get(&kind), STATUS_KINDS)?;
        entry.status[status_index(kind)] =
            own.or_else(|| effects.defaults.status.get(&kind).copied());
    }
    entry.death = check
        .name("death", block.death.as_ref(), DEATH_KINDS)?
        .or(effects.defaults.death);
    resolve_abilities(archetype, &check, spells, &mut entry)?;
    Ok(entry)
}

/// Resolves the abilities of `archetype`'s block into `entry`, with each missile's extent read
/// from `spells`.
fn resolve_abilities(
    archetype: &UnitArchetype,
    check: &Checker<'_>,
    spells: &SpellCatalog,
    entry: &mut UnitEntry,
) -> Result<(), EffectCatalogError> {
    for (ability, name) in &archetype.effects.abilities {
        let index = archetype
            .abilities
            .iter()
            .position(|spec| spec.name == *ability)
            .and_then(|index| u8::try_from(index).ok())
            .ok_or_else(|| EffectCatalogError::UnknownUnitAbility {
                unit: archetype.slug.clone(),
                ability: ability.clone(),
            })?;
        if let Some(effect) = check.name("ability", Some(name), ABILITY_KINDS)? {
            let extent = archetype
                .abilities
                .get(usize::from(index))
                .map_or_else(MissileExtent::default, |spec| {
                    missile_extent(spells, &spec.effect)
                });
            entry.abilities.push(AbilityLook {
                ability: index,
                effect,
                extent,
            });
        }
    }
    Ok(())
}

/// When the `Critical` riding strip (a slug) of the shot `effect` reaches the far end of its
/// line on an instant shot: the strip's head rides the line with its layer's life, so it
/// arrives at the layer's end. Layers lasting a bound span (a projectile's flight) do not
/// count: a projectile's impact already waits for its flight. 0 without such a strip.
fn slug_arrival(effect: &super::Effect) -> f32 {
    effect
        .layers
        .iter()
        .filter(|layer| {
            layer.priority == super::schema::Priority::Critical
                && layer.lasts.is_none()
                && matches!(&layer.block, super::Block::Ribbon(look) if look.trail.is_some())
        })
        .map(|layer| layer.start + layer.duration)
        .fold(0.0_f32, f32::max)
}

/// Nodes a walk of an ability's effect tree visits at most: the catalog's trees are a few
/// dozen nodes, and a bound keeps a malformed tree from walking forever.
const MAX_WALKED_NODES: usize = 256;

/// What the ability effect `spec` covers where its missile lands (plan WP11, F1): the radius
/// of the first search its tree runs and the longest duration, in seconds, of the statuses its
/// tree applies, walking sets, searches, persistents, missiles and displacements. The default
/// for an ability that is not an effect tree.
fn missile_extent(spells: &SpellCatalog, spec: &sim_types::AbilityEffectSpec) -> MissileExtent {
    let sim_types::AbilityEffectSpec::Effect(name) = spec else {
        return MissileExtent::default();
    };
    let Some(root) = spells.effect_id(name) else {
        return MissileExtent::default();
    };
    let radius = spells
        .facts(root)
        .and_then(|facts| facts.targeting_radius)
        .map_or(0.0, sim_types::SimScalar::to_f32);
    let mut ticks = 0.0_f32;
    let mut stack = vec![root];
    let mut visited = 0;
    while let Some(id) = stack.pop() {
        visited += 1;
        if visited > MAX_WALKED_NODES {
            break;
        }
        let Some(node) = spells.node(id) else {
            continue;
        };
        push_children(spells, node.kind, &mut stack);
        if let EffectNode::ApplyStatus { status } = node.kind {
            if let Some(status) = spells.status(status) {
                ticks = ticks.max(status.spec.duration_ticks.to_f32());
            }
        }
    }
    MissileExtent {
        radius,
        seconds: ticks / f32::from(sim_types::TICKS_PER_SECOND),
    }
}

/// Pushes the children of a node of kind `kind` onto `stack`.
fn push_children(spells: &SpellCatalog, kind: EffectNode, stack: &mut Vec<SpellEffectId>) {
    match kind {
        EffectNode::Set { first, len } => {
            stack.extend((0..len).filter_map(|index| spells.set_child(first, index)));
        }
        EffectNode::Search { child, .. } => stack.push(child),
        EffectNode::Persistent {
            initial, periodic, ..
        } => stack.extend(initial.into_iter().chain(periodic)),
        EffectNode::LaunchMissile { impact, .. } => stack.push(impact),
        EffectNode::Displace { on_land, .. } => stack.extend(on_land),
        EffectNode::Damage { .. }
        | EffectNode::ApplyStatus { .. }
        | EffectNode::RemoveStatus { .. }
        | EffectNode::ModifyVitals { .. }
        | EffectNode::CancelOrders { .. } => {}
    }
}
