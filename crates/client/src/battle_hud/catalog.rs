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
//! - An identity is unpresentable exactly when
//!   [`ClientMediaStatus::presentable`](crate::media::ClientMediaStatus::presentable) refuses it.

use std::collections::BTreeMap;

use sim_core::upgrades::{UpgradeCatalog, OVERCLOCK_ABILITY_SLUG};
use sim_core::{UnitArchetype, UnitCatalog};
use sim_types::enums::ShapeKind;
use sim_types::{SimScalar, UnitTypeId, UpgradeEffect};

use crate::battle_pipeline::commands::is_armed;

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
        Self {
            archetypes: units
                .archetypes()
                .map(|archetype| {
                    let id = archetype.unit_type_id;
                    let commands = commands_of(archetype, units, upgrades, unpresentable(id));
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
        }
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
        overclock_unlock: overclock_unlock(archetype, upgrades),
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

/// The upgrade whose effects unlock `archetype`'s overclock, if any.
fn overclock_unlock(archetype: &UnitArchetype, upgrades: &UpgradeCatalog) -> Option<u32> {
    upgrades
        .upgrades()
        .find(|upgrade| {
            upgrade.effects.iter().any(|effect| {
                matches!(
                    effect,
                    UpgradeEffect::UnlockAbility { target_slug, ability_slug }
                        if *target_slug == archetype.slug && ability_slug == OVERCLOCK_ABILITY_SLUG
                )
            })
        })
        .map(|upgrade| upgrade.upgrade_id)
}

const fn is_structure(archetype: &UnitArchetype) -> bool {
    matches!(archetype.shape, ShapeKind::BuildingDome { .. })
}
