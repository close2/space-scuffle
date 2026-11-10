//! The units' choice of effects: each archetype's presentation block resolved against the
//! vocabulary into dense tables (plan §5.3).
//!
//! # Responsibility
//! Owns [`UnitEffectTables`]: per [`UnitTypeId`], the effects of each weapon (by
//! [`WeaponKey`]: slot and mode index), of each status, of each mode and of death, with the
//! vocabulary's defaults filled in where the block names nothing.
//!
//! # How it fits
//! Built once per session from the unit catalog (`sim_core::catalog::ArchetypeEffects`, read
//! from `units.ron` and `structures.ron`) and the [`EffectCatalog`]. The event wiring looks an
//! event's [`EffectSource`] up here: one index and a scan of the unit's few weapons.
//!
//! # Invariants
//! - Every name in every block resolves, every mode name is one of the archetype's modes, and
//!   every effect is made for the kind of event its slot receives.

use super::error::EffectCatalogError;
use super::schema::EventKind;
use super::{EffectCatalog, EffectId};
use sim_core::catalog::{ArchetypeEffects, UnitArchetype, UnitCatalog};
use sim_types::{EffectSource, StatusKind, UnitTypeId, WeaponKey};

/// Event kinds a splash slot accepts.
const SPLASH_KINDS: &[EventKind] = &[EventKind::Splash, EventKind::Line];
/// Event kinds an impact, launch or travel slot accepts.
const POINT_KINDS: &[EventKind] = &[EventKind::Point];
/// Event kinds a death slot accepts.
const DEATH_KINDS: &[EventKind] = &[EventKind::Death];
/// Event kinds a status slot accepts.
const STATUS_KINDS: &[EventKind] = &[EventKind::Status];
/// Event kinds a mode slot accepts.
const MODE_KINDS: &[EventKind] = &[EventKind::Mode];

/// Every status kind, in the order of [`status_index`].
const STATUS_ORDER: [StatusKind; 8] = [
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
const fn status_index(kind: StatusKind) -> usize {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponEffectIds {
    /// Slot and mode of the weapon.
    pub key: WeaponKey,
    /// At the muzzle.
    pub launch: Option<EffectId>,
    /// In flight.
    pub travel: Option<EffectId>,
    /// Where the shot lands.
    pub impact: Option<EffectId>,
    /// Each splash search.
    pub splash: Option<EffectId>,
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
}

/// One archetype's resolved block.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct UnitEntry {
    /// Weapons named by the block.
    weapons: Vec<WeaponEffectIds>,
    /// Modes named by the block.
    modes: Vec<ModeEffectIds>,
    /// Status effects by [`status_index`], defaults filled in.
    status: [Option<EffectId>; STATUS_ORDER.len()],
    /// Death effect, default filled in.
    death: Option<EffectId>,
}

/// Per-archetype effect choices, indexed by [`UnitTypeId`].
#[derive(Debug, Clone, Default, PartialEq, Eq, bevy::prelude::Resource)]
pub struct UnitEffectTables {
    /// Entries by unit type id; ids absent from the catalog have an empty entry.
    units: Vec<UnitEntry>,
    /// Default splash effect.
    default_splash: Option<EffectId>,
    /// Default impact effect.
    default_impact: Option<EffectId>,
}

impl UnitEffectTables {
    /// Resolves every archetype's presentation block in `units` against `effects`.
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
        for archetype in units.archetypes() {
            let entry = resolve_unit(archetype, effects)?;
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

    /// The mode effects of `unit` for mode index `mode`.
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
) -> Result<UnitEntry, EffectCatalogError> {
    let block: &ArchetypeEffects = &archetype.effects;
    let check = Checker {
        effects,
        unit: &archetype.slug,
    };
    let mut entry = UnitEntry::default();
    for weapon in &block.weapons {
        let mode = weapon
            .mode
            .as_deref()
            .map(|name| mode_index(archetype, name))
            .transpose()?;
        entry.weapons.push(WeaponEffectIds {
            key: WeaponKey {
                slot: weapon.weapon,
                mode,
            },
            launch: check.name("weapon launch", weapon.launch.as_ref(), POINT_KINDS)?,
            travel: check.name("weapon travel", weapon.travel.as_ref(), POINT_KINDS)?,
            impact: check.name("weapon impact", weapon.impact.as_ref(), POINT_KINDS)?,
            splash: check.name("weapon splash", weapon.splash.as_ref(), SPLASH_KINDS)?,
        });
    }
    for mode in &block.modes {
        entry.modes.push(ModeEffectIds {
            mode: mode_index(archetype, &mode.mode)?,
            enter: check.name("mode enter", mode.enter.as_ref(), MODE_KINDS)?,
            leave: check.name("mode leave", mode.leave.as_ref(), MODE_KINDS)?,
            transition: check.name("mode transition", mode.transition.as_ref(), MODE_KINDS)?,
            stance: check.name("mode stance", mode.stance.as_ref(), MODE_KINDS)?,
        });
    }
    for kind in STATUS_ORDER {
        let own = check.name("status", block.status.get(&kind), STATUS_KINDS)?;
        entry.status[status_index(kind)] =
            own.or_else(|| effects.defaults.status.get(&kind).copied());
    }
    entry.death = check
        .name("death", block.death.as_ref(), DEATH_KINDS)?
        .or(effects.defaults.death);
    for name in block.abilities.values() {
        check.name("ability", Some(name), &[EventKind::Area, EventKind::Link])?;
    }
    Ok(entry)
}
