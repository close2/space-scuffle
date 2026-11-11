//! Labels and capabilities of every identity a command card can show.
//!
//! # Responsibility
//! Turns the unit and upgrade catalogs into the plain facts a HUD needs: display names, what each
//! identity can do, and whether its media would let it appear at all.
//!
//! # How it fits
//! [`HudModel`](super::model::HudModel) builds one [`CommandCatalog`] per session generation, so
//! drawing never searches the catalogs or checks media. Whether an entry is usable *now* is not
//! here: that is a fact of the match, and it comes from
//! [`PlayerView::options`](sim_types::PlayerView::options).
//!
//! # Invariants
//! - Every string comes from a catalog's `display_name`. No identity is named in code.
//! - Capabilities are read from optional archetype components, never from a type's identity.
//! - Mode entries come from `ToggleMode` abilities whose `SwitchMode` effect names one of their
//!   archetype's modes ([`mode_of_ability`]). A mode name is interned as a [`ModeId`] and compared
//!   only with other catalog strings, never with a literal.
//! - Cast entries come from targeted effect-tree abilities ([`cast_of_ability`]), interned as a
//!   [`CastId`] by name and target kind, in catalog order, the same way.
//! - An identity is unpresentable exactly when
//!   [`ClientMediaStatus::presentable`](crate::media::ClientMediaStatus::presentable) refuses it.

use std::collections::BTreeMap;

use sim_core::upgrades::{UpgradeCatalog, OVERCLOCK_ABILITY_SLUG};
use sim_core::{UnitArchetype, UnitCatalog};
use sim_types::enums::ShapeKind;
use sim_types::{SimScalar, UnitTypeId, UpgradeEffect};

use crate::battle_pipeline::commands::{
    cast_of_ability, is_armed, mode_of_ability, CastKey, CastTarget,
};

/// What one catalog identity is called and can do.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent capability a card entry depends on"
)]
pub struct ArchetypeCommands {
    /// Display name.
    pub label: String,
    /// Whether it can walk or fly.
    pub mobile: bool,
    /// Whether it carries any weapon.
    pub armed: bool,
    /// Whether it can gather resources.
    pub harvester: bool,
    /// Whether it has an overclock ability, researched or not.
    pub overclock: bool,
    /// Catalog ID of the upgrade that unlocks its overclock, if one must be researched.
    pub overclock_unlock: Option<u32>,
    /// Its mode abilities, one per mode, in `ModeId` order.
    pub modes: Vec<ModeCommand>,
    /// Its targeted effect-tree abilities, one per cast, in `CastId` order.
    pub casts: Vec<CastCommand>,
    /// Whether it can transport other units.
    pub carrier: bool,
    /// Whether it is a structure.
    pub structure: bool,
    /// Whether it has a shield to show.
    pub shielded: bool,
    /// Mobile units it can train, in catalog order.
    pub trains: Vec<UnitTypeId>,
    /// Catalog IDs of the upgrades it can research, in catalog order.
    pub researches: Vec<u32>,
    /// Structures this unit can raise, in catalog order. Empty for all but builders.
    pub raises: Vec<UnitTypeId>,
    /// Why this identity cannot be shown, if it cannot.
    pub unpresentable: Option<String>,
}

/// A mode of the catalog, interned in catalog order (by archetype, then by ability).
///
/// Archetypes that declare the same mode name share one ID, and so one pair of card entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModeId(pub u16);

/// One archetype's way into and out of one mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeCommand {
    /// The mode.
    pub mode: ModeId,
    /// Index of the archetype's first ability that switches it.
    pub ability_index: u8,
    /// Index of the mode in the archetype's `modes`, as `ModeStateView::settled_mode` reports it.
    pub mode_index: u8,
    /// Catalog ID of the upgrade that unlocks the ability, if one must be researched.
    pub unlock: Option<u32>,
}

/// A targeted cast of the catalog, interned in catalog order (by archetype, then by ability).
///
/// Archetypes that declare an ability of the same name and target kind share one ID, and so
/// one card entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CastId(pub u16);

/// One archetype's way to one cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CastCommand {
    /// The cast.
    pub cast: CastId,
    /// Index of the archetype's first ability for it.
    pub ability_index: u8,
    /// Catalog ID of the upgrade that unlocks the ability, if one must be researched.
    pub unlock: Option<u32>,
}

/// The name of one cast and what arms it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastLabels {
    /// The ability's catalog `name`, the entry's label.
    pub name: String,
    /// The ability on the first archetype that declares it, which a cast mode carries.
    pub key: CastKey,
    /// Whether it aims at a point or a unit.
    pub target: CastTarget,
}

/// The names of one mode and of its two commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeLabels {
    /// The mode's catalog name, which the simulation's ability effect names.
    pub mode_name: String,
    /// The command that enters it: the ability's `name`.
    pub enter: String,
    /// The command that leaves it: the ability's `leave_name`.
    pub leave: String,
}

/// What one upgrade is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeCommand {
    /// Display name.
    pub label: String,
}

/// Labels and capabilities of every identity a card can show, from catalog data alone.
#[derive(Debug, Default)]
pub struct CommandCatalog {
    archetypes: BTreeMap<UnitTypeId, ArchetypeCommands>,
    upgrades: BTreeMap<u32, UpgradeCommand>,
    /// Every mode, indexed by [`ModeId`]; labels from the first archetype that declares it.
    modes: Vec<ModeLabels>,
    /// Every cast, indexed by [`CastId`]; labels from the first archetype that declares it.
    casts: Vec<CastLabels>,
}

impl CommandCatalog {
    /// Derives the catalog of one session.
    ///
    /// `unpresentable` returns why an identity cannot be shown, or `None` if it can. It is a
    /// parameter so that this derivation needs no running application.
    #[must_use]
    pub fn build(
        units: &UnitCatalog,
        upgrades: &UpgradeCatalog,
        unpresentable: impl Fn(UnitTypeId) -> Option<String>,
    ) -> Self {
        let modes = intern_modes(units);
        let casts = intern_casts(units);
        Self {
            archetypes: units
                .archetypes()
                .map(|archetype| {
                    let id = archetype.unit_type_id;
                    let mut commands = commands_of(archetype, units, upgrades, unpresentable(id));
                    commands.modes = mode_commands(archetype, upgrades, &modes);
                    commands.casts = cast_commands(archetype, upgrades, &casts);
                    (id, commands)
                })
                .collect(),
            upgrades: upgrades
                .upgrades()
                .map(|upgrade| {
                    let label = upgrade.display_name.clone();
                    (upgrade.upgrade_id, UpgradeCommand { label })
                })
                .collect(),
            modes,
            casts,
        }
    }

    /// The name and key of `cast`, if the catalog declares it.
    #[must_use]
    pub fn cast(&self, cast: CastId) -> Option<&CastLabels> {
        self.casts.get(usize::from(cast.0))
    }

    /// Every cast's ID, in catalog order.
    pub fn cast_ids(&self) -> impl Iterator<Item = CastId> {
        (0..self.casts.len()).filter_map(|index| u16::try_from(index).ok().map(CastId))
    }

    /// The names of `mode`, if the catalog declares it.
    #[must_use]
    pub fn mode(&self, mode: ModeId) -> Option<&ModeLabels> {
        self.modes.get(usize::from(mode.0))
    }

    /// Every mode's ID, in catalog order.
    pub fn mode_ids(&self) -> impl Iterator<Item = ModeId> {
        (0..self.modes.len()).filter_map(|index| u16::try_from(index).ok().map(ModeId))
    }

    /// The facts of `unit_type`, if the catalog holds it.
    #[must_use]
    pub fn archetype(&self, unit_type: UnitTypeId) -> Option<&ArchetypeCommands> {
        self.archetypes.get(&unit_type)
    }

    /// The display name of `unit_type`, or an empty string for an identity outside the catalog.
    ///
    /// The tick driver stops a battle that publishes such an identity, so a HUD never shows one.
    #[must_use]
    pub fn label(&self, unit_type: UnitTypeId) -> &str {
        self.archetype(unit_type)
            .map_or("", |archetype| archetype.label.as_str())
    }

    /// The facts of the upgrade with catalog ID `upgrade`, if the catalog holds it.
    #[must_use]
    pub fn upgrade(&self, upgrade: u32) -> Option<&UpgradeCommand> {
        self.upgrades.get(&upgrade)
    }
}

fn commands_of(
    archetype: &UnitArchetype,
    units: &UnitCatalog,
    upgrades: &UpgradeCatalog,
    unpresentable: Option<String>,
) -> ArchetypeCommands {
    ArchetypeCommands {
        label: archetype.display_name.clone(),
        mobile: !archetype.is_immobile_by_default(),
        armed: is_armed(archetype),
        harvester: archetype.capabilities.harvester.is_some(),
        overclock: archetype.overclock.is_some(),
        overclock_unlock: unlock_of(archetype, OVERCLOCK_ABILITY_SLUG, upgrades),
        modes: Vec::new(),
        casts: Vec::new(),
        carrier: archetype.capabilities.transport_capacity > 0,
        structure: is_structure(archetype),
        shielded: archetype.max_shields > SimScalar::ZERO,
        // Structures in a production list are raised by builders, not trained.
        trains: archetype
            .trains_units
            .iter()
            .filter_map(|slug| units.get_by_slug(slug))
            .filter(|product| !is_structure(product))
            .map(|product| product.unit_type_id)
            .collect(),
        researches: archetype
            .researches_upgrades
            .iter()
            .filter_map(|slug| upgrades.get_by_slug(slug))
            .map(|upgrade| upgrade.upgrade_id)
            .collect(),
        raises: units
            .archetypes()
            .filter(|structure| archetype.can_raise(structure))
            .map(|structure| structure.unit_type_id)
            .collect(),
        unpresentable,
    }
}

/// The upgrade whose effects unlock `archetype`'s ability named `ability`, if any.
fn unlock_of(archetype: &UnitArchetype, ability: &str, upgrades: &UpgradeCatalog) -> Option<u32> {
    upgrades
        .upgrades()
        .find(|upgrade| {
            upgrade.effects.iter().any(|effect| {
                matches!(
                    effect,
                    UpgradeEffect::UnlockAbility { target_slug, ability_slug }
                        if *target_slug == archetype.slug && ability_slug == ability
                )
            })
        })
        .map(|upgrade| upgrade.upgrade_id)
}

/// Every mode a mode ability switches, in catalog order, labelled by the first archetype that
/// declares it.
fn intern_modes(units: &UnitCatalog) -> Vec<ModeLabels> {
    let mut modes: Vec<ModeLabels> = Vec::new();
    for archetype in units.archetypes() {
        for ability in &archetype.abilities {
            let Some(mode_name) = mode_of_ability(archetype, ability) else {
                continue;
            };
            if modes.iter().all(|mode| mode.mode_name != mode_name) {
                modes.push(ModeLabels {
                    mode_name: mode_name.to_owned(),
                    enter: ability.name.clone(),
                    leave: ability.leave_name.clone().unwrap_or_default(),
                });
            }
        }
    }
    modes
}

/// `archetype`'s mode commands, one per mode (its first ability for that mode), in `ModeId`
/// order.
fn mode_commands(
    archetype: &UnitArchetype,
    upgrades: &UpgradeCatalog,
    modes: &[ModeLabels],
) -> Vec<ModeCommand> {
    let mut commands: Vec<ModeCommand> = Vec::new();
    for (ability, ability_index) in archetype.abilities.iter().zip(0_u8..) {
        let Some(mode_name) = mode_of_ability(archetype, ability) else {
            continue;
        };
        let interned = modes
            .iter()
            .position(|mode| mode.mode_name == mode_name)
            .and_then(|index| u16::try_from(index).ok());
        let mode_index = archetype
            .modes
            .iter()
            .position(|mode| mode.mode_name == mode_name)
            .and_then(|index| u8::try_from(index).ok());
        let (Some(id), Some(mode_index)) = (interned, mode_index) else {
            continue;
        };
        if commands.iter().any(|command| command.mode == ModeId(id)) {
            continue;
        }
        commands.push(ModeCommand {
            mode: ModeId(id),
            ability_index,
            mode_index,
            unlock: unlock_of(archetype, &ability.name, upgrades),
        });
    }
    commands.sort_unstable_by_key(|command| command.mode);
    commands
}

/// Every targeted cast of the catalog, by name and target kind, in catalog order, labelled and
/// keyed by the first archetype that declares it.
fn intern_casts(units: &UnitCatalog) -> Vec<CastLabels> {
    let mut casts: Vec<CastLabels> = Vec::new();
    for archetype in units.archetypes() {
        for (ability, ability_index) in archetype.abilities.iter().zip(0_u8..) {
            let Some(target) = cast_of_ability(ability) else {
                continue;
            };
            if casts
                .iter()
                .all(|cast| cast.name != ability.name || cast.target != target)
            {
                casts.push(CastLabels {
                    name: ability.name.clone(),
                    key: CastKey {
                        holder: archetype.unit_type_id,
                        ability_index,
                    },
                    target,
                });
            }
        }
    }
    casts
}

/// `archetype`'s cast commands, one per cast (its first ability for that cast), in `CastId`
/// order.
fn cast_commands(
    archetype: &UnitArchetype,
    upgrades: &UpgradeCatalog,
    casts: &[CastLabels],
) -> Vec<CastCommand> {
    let mut commands: Vec<CastCommand> = Vec::new();
    for (ability, ability_index) in archetype.abilities.iter().zip(0_u8..) {
        let Some(target) = cast_of_ability(ability) else {
            continue;
        };
        let Some(id) = casts
            .iter()
            .position(|cast| cast.name == ability.name && cast.target == target)
            .and_then(|index| u16::try_from(index).ok())
        else {
            continue;
        };
        if commands.iter().any(|command| command.cast == CastId(id)) {
            continue;
        }
        commands.push(CastCommand {
            cast: CastId(id),
            ability_index,
            unlock: unlock_of(archetype, &ability.name, upgrades),
        });
    }
    commands.sort_unstable_by_key(|command| command.cast);
    commands
}

const fn is_structure(archetype: &UnitArchetype) -> bool {
    matches!(archetype.shape, ShapeKind::BuildingDome { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle_pipeline::commands::mode_of_ability;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn shipped() -> (UnitCatalog, UpgradeCatalog) {
        let catalogs = sim_fixtures::shipped_catalogs();
        (catalogs.units, catalogs.upgrades)
    }

    #[test]
    fn every_mode_ability_gets_one_mode_with_labels_from_data() -> TestResult {
        let (units, upgrades) = shipped();
        let catalog = CommandCatalog::build(&units, &upgrades, |_| None);
        let mut seen = 0;
        for archetype in units.archetypes() {
            for ability in &archetype.abilities {
                let Some(mode_name) = mode_of_ability(archetype, ability) else {
                    continue;
                };
                seen += 1;
                let commands = catalog
                    .archetype(archetype.unit_type_id)
                    .ok_or("archetype")?;
                let command = commands
                    .modes
                    .iter()
                    .find(|command| {
                        catalog
                            .mode(command.mode)
                            .is_some_and(|m| m.mode_name == mode_name)
                    })
                    .ok_or("a mode command")?;
                let labels = catalog.mode(command.mode).ok_or("labels")?;
                assert_eq!(labels.enter, ability.name);
                assert_eq!(Some(&labels.leave), ability.leave_name.as_ref());
                assert_eq!(
                    archetype.modes[usize::from(command.mode_index)].mode_name,
                    mode_name
                );
            }
        }
        assert!(seen >= 2, "the shipped catalog has mode abilities");
        Ok(())
    }

    /// A scratch point cast rooted at an effect name; the catalog reads no spell table.
    fn scratch_cast(
        name: &str,
        target_kind: sim_types::AbilityTargetKind,
    ) -> sim_types::AbilitySpec {
        sim_types::AbilitySpec {
            name: name.to_owned(),
            target_kind,
            energy_cost: SimScalar::ZERO,
            cooldown_ticks: 0,
            cast_range: SimScalar::ONE,
            effect: sim_types::AbilityEffectSpec::Effect("scratch".to_owned()),
            leave_name: None,
            cast_backswing_ticks: 0,
            single_caster: true,
            target_filter: None,
            origin: None,
        }
    }

    #[test]
    fn targeted_effect_abilities_become_casts_shared_by_name_and_target() -> TestResult {
        use sim_types::AbilityTargetKind::{TargetEntity, TargetPosition};
        let (mut units, upgrades) = shipped();
        // The shipped casts, so the count below holds whatever spells the catalog ships.
        let shipped_casts = CommandCatalog::build(&units, &upgrades, |_| None)
            .cast_ids()
            .count();
        let kinds: Vec<UnitTypeId> = units
            .archetypes()
            .filter(|archetype| archetype.abilities.is_empty())
            .map(|archetype| archetype.unit_type_id)
            .collect();
        let (first, second) = (*kinds.first().ok_or("one")?, *kinds.get(1).ok_or("two")?);
        units
            .get_mut(first)?
            .abilities
            .push(scratch_cast("Scratch zone", TargetPosition));
        {
            let second = units.get_mut(second)?;
            second
                .abilities
                .push(scratch_cast("Scratch bolt", TargetEntity));
            second
                .abilities
                .push(scratch_cast("Scratch zone", TargetPosition));
            second
                .abilities
                .push(scratch_cast("Scratch zone", TargetEntity));
        }
        let catalog = CommandCatalog::build(&units, &upgrades, |_| None);
        let zone = catalog.archetype(first).ok_or("first")?.casts[0];
        let shared = catalog
            .archetype(second)
            .ok_or("second")?
            .casts
            .iter()
            .find(|command| command.cast == zone.cast)
            .ok_or("the second archetype shares the zone")?;
        assert_eq!(shared.ability_index, 1, "its own index");
        let labels = catalog.cast(zone.cast).ok_or("labels")?;
        assert_eq!(labels.name, "Scratch zone");
        assert_eq!(labels.target, CastTarget::Point);
        assert_eq!(
            labels.key.holder, first,
            "keyed by the first declaring archetype"
        );
        assert_eq!(
            catalog.archetype(second).ok_or("second")?.casts.len(),
            3,
            "a unit cast of the same name is a cast of its own"
        );
        assert_eq!(
            catalog.cast_ids().count(),
            shipped_casts + 3,
            "three scratch casts: the shared point zone, the bolt and the unit zone"
        );
        Ok(())
    }

    #[test]
    fn archetypes_sharing_a_mode_share_one_id() -> TestResult {
        let (mut units, upgrades) = shipped();
        let holder = units
            .archetypes()
            .find(|archetype| {
                archetype
                    .abilities
                    .iter()
                    .any(|ability| mode_of_ability(archetype, ability).is_some())
            })
            .ok_or("a mode unit")?
            .clone();
        let other = units
            .archetypes()
            .find(|archetype| archetype.modes.is_empty() && archetype.abilities.is_empty())
            .ok_or("a unit without modes")?
            .unit_type_id;
        {
            let other = units.get_mut(other)?;
            other.modes.clone_from(&holder.modes);
            other.abilities.clone_from(&holder.abilities);
        }
        let catalog = CommandCatalog::build(&units, &upgrades, |_| None);
        let first = catalog
            .archetype(holder.unit_type_id)
            .ok_or("holder")?
            .modes[0];
        let second = catalog.archetype(other).ok_or("other")?.modes[0];
        assert_eq!(first.mode, second.mode, "one ModeId for one mode name");
        assert_eq!(catalog.mode_ids().count(), {
            let mut names: Vec<&str> = units
                .archetypes()
                .flat_map(|a| {
                    a.abilities
                        .iter()
                        .filter_map(move |b| mode_of_ability(a, b))
                })
                .collect();
            names.sort_unstable();
            names.dedup();
            names.len()
        });
        Ok(())
    }
}
