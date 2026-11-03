//! # Authoritative Upgrade Catalog & In-Memory Upgrade Registry
//!
//! Provides deterministic indexed lookup for weapon/armor scaling upgrades and unit-specific tech upgrades.
//!
//! ## Invariants Maintained
//! 1. All upgrades are strictly data-driven loaded from `assets/catalog/upgrades.ron`.
//! 2. Zero hardcoded upgrade slugs in engine logic.
//! 3. Deterministic upgrade IDs and effects.
//! 4. Authored upgrades name their faction and weapon or armor classes by slug; loading resolves
//!    them against a [`FactionCatalog`] into [`UpgradeDef`] and [`UpgradeEffect`], which carry
//!    only the numeric IDs, and fails with a typed error naming the upgrade for an unknown slug.

use crate::catalog::FactionCatalog;
use crate::error::{SimCoreError, SimResult};
use serde::{Deserialize, Serialize};
use sim_types::{FactionId, SimScalar, UpgradeEffect};
use std::collections::BTreeMap;
use std::path::Path;

/// Ability slug under which `UnlockAbility` effects address the overclock capability.
pub const OVERCLOCK_ABILITY_SLUG: &str = "overclock";

/// Unique upgrade identifier type.
pub type UpgradeId = u32;

/// The authored shape of `assets/catalog/upgrades.ron`.
#[derive(Debug, Deserialize)]
struct UpgradeCatalogFile {
    upgrades: Vec<UpgradeDefFile>,
}

/// An authored upgrade, whose faction and effect classes are still slugs.
#[derive(Debug, Deserialize)]
struct UpgradeDefFile {
    upgrade_id: UpgradeId,
    slug: String,
    name: String,
    display_name: String,
    faction: String,
    level: u8,
    mineral_cost: u32,
    gas_cost: u32,
    research_ticks: u32,
    researched_at: String,
    #[serde(default)]
    requires_structures: Vec<String>,
    effects: Vec<UpgradeEffectDef>,
}

/// An authored [`UpgradeEffect`]: the same shape, with weapon and armor classes named by slug.
#[derive(Debug, Deserialize)]
enum UpgradeEffectDef {
    WeaponDamageLevel {
        weapon_class: String,
        level: u8,
    },
    ArmorLevel {
        armor_class: String,
        level: u8,
    },
    ShieldLevel {
        level: u8,
    },
    ModifyUnitStat {
        target_slug: String,
        #[serde(default)]
        max_hp_delta: SimScalar,
        #[serde(default)]
        move_speed_delta: SimScalar,
        #[serde(default)]
        sight_delta: SimScalar,
        #[serde(default)]
        weapon_range_delta: SimScalar,
        #[serde(default)]
        armor_delta: SimScalar,
        #[serde(default)]
        attack_cooldown_delta: i32,
        #[serde(default)]
        bonus_damage_delta: SimScalar,
    },
    UnlockAbility {
        target_slug: String,
        ability_slug: String,
    },
}

impl UpgradeEffectDef {
    /// Resolves this effect of the upgrade `upgrade` against `factions`.
    fn resolve(self, factions: &FactionCatalog, upgrade: &str) -> SimResult<UpgradeEffect> {
        Ok(match self {
            Self::WeaponDamageLevel {
                weapon_class,
                level,
            } => UpgradeEffect::WeaponDamageLevel {
                weapon_class: factions.resolve_weapon_class(upgrade, &weapon_class)?,
                level,
            },
            Self::ArmorLevel { armor_class, level } => UpgradeEffect::ArmorLevel {
                armor_class: factions.resolve_armor_class(upgrade, &armor_class)?,
                level,
            },
            Self::ShieldLevel { level } => UpgradeEffect::ShieldLevel { level },
            Self::ModifyUnitStat {
                target_slug,
                max_hp_delta,
                move_speed_delta,
                sight_delta,
                weapon_range_delta,
                armor_delta,
                attack_cooldown_delta,
                bonus_damage_delta,
            } => UpgradeEffect::ModifyUnitStat {
                target_slug,
                max_hp_delta,
                move_speed_delta,
                sight_delta,
                weapon_range_delta,
                armor_delta,
                attack_cooldown_delta,
                bonus_damage_delta,
            },
            Self::UnlockAbility {
                target_slug,
                ability_slug,
            } => UpgradeEffect::UnlockAbility {
                target_slug,
                ability_slug,
            },
        })
    }
}

impl UpgradeDefFile {
    /// Resolves this upgrade's faction and effect classes against `factions`.
    fn resolve(self, factions: &FactionCatalog) -> SimResult<UpgradeDef> {
        let faction = factions.resolve_faction(&self.slug, &self.faction)?;
        let effects = self
            .effects
            .into_iter()
            .map(|effect| effect.resolve(factions, &self.slug))
            .collect::<SimResult<Vec<_>>>()?;
        Ok(UpgradeDef {
            upgrade_id: self.upgrade_id,
            slug: self.slug,
            name: self.name,
            display_name: self.display_name,
            faction,
            level: self.level,
            mineral_cost: self.mineral_cost,
            gas_cost: self.gas_cost,
            research_ticks: self.research_ticks,
            researched_at: self.researched_at,
            requires_structures: self.requires_structures,
            effects,
        })
    }
}

/// Resolved specification for an upgrade in Space Scuffle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpgradeDef {
    /// Unique integer identifier.
    pub upgrade_id: UpgradeId,
    /// Canonical internal slug name.
    pub slug: String,
    /// Canonical internal name.
    pub name: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Owning faction, resolved from its slug in `factions.ron`.
    pub faction: FactionId,
    /// Tier level (1..=3).
    pub level: u8,
    /// Mineral resource cost.
    pub mineral_cost: u32,
    /// Vespene gas resource cost.
    pub gas_cost: u32,
    /// Duration of research in ticks (at 25 Hz).
    pub research_ticks: u32,
    /// Slug of the structure dome where this upgrade is researched.
    pub researched_at: String,
    /// Prerequisite structure slugs required before research can begin.
    pub requires_structures: Vec<String>,
    /// Immediate mechanical effects applied to player state upon completion.
    pub effects: Vec<UpgradeEffect>,
}

/// Read-only indexed catalog of all registered game upgrades.
#[derive(Debug, Clone, Default)]
pub struct UpgradeCatalog {
    upgrades_by_id: BTreeMap<UpgradeId, UpgradeDef>,
    upgrades_by_slug: BTreeMap<String, UpgradeId>,
}

impl UpgradeCatalog {
    /// Deserializes an upgrade catalog from raw RON text, resolving faction and class slugs
    /// against `factions`.
    ///
    /// # Errors
    /// Returns [`SimCoreError`] for malformed RON, duplicate IDs/slugs, blank slugs, or a
    /// faction or class slug `factions` does not define ([`SimCoreError::UnknownFaction`],
    /// [`SimCoreError::UnknownWeaponClass`], [`SimCoreError::UnknownArmorClass`]).
    pub fn from_ron_str(factions: &FactionCatalog, ron_data: &str) -> SimResult<Self> {
        let catalog_file: UpgradeCatalogFile = ron::from_str(ron_data)
            .map_err(|e| SimCoreError::RonDeserializationError(e.to_string()))?;
        let definitions = catalog_file
            .upgrades
            .into_iter()
            .map(|def| def.resolve(factions))
            .collect::<SimResult<Vec<_>>>()?;
        Self::from_definitions(definitions)
    }

    /// Builds a catalog from already resolved definitions, as tools and tests edit them.
    ///
    /// # Errors
    /// Returns [`SimCoreError`] for duplicate IDs/slugs or blank slugs.
    pub fn from_definitions(definitions: impl IntoIterator<Item = UpgradeDef>) -> SimResult<Self> {
        let mut catalog = Self::default();

        for def in definitions {
            let id = def.upgrade_id;
            if def.slug.trim().is_empty() {
                return Err(SimCoreError::CatalogInvalidValue {
                    definition: def.name,
                    field: "slug".to_owned(),
                    reason: "must not be blank",
                });
            }
            if catalog.upgrades_by_id.contains_key(&id) {
                return Err(SimCoreError::CatalogDuplicateId {
                    kind: "upgrade",
                    id,
                });
            }
            if catalog.upgrades_by_slug.contains_key(&def.slug) {
                return Err(SimCoreError::CatalogDuplicateSlug {
                    kind: "upgrade",
                    slug: def.slug,
                });
            }
            catalog.upgrades_by_slug.insert(def.slug.clone(), id);
            // Sparse authored u32 IDs must not trigger allocation proportional to
            // their numeric value. Ordered storage also preserves ID-order iteration.
            catalog.upgrades_by_id.insert(id, def);
        }

        Ok(catalog)
    }

    /// Loads an upgrade catalog from an on-disk RON file, resolving slugs against `factions`.
    ///
    /// # Errors
    /// Returns [`SimCoreError::IoError`] if the file cannot be read, or any error of
    /// [`Self::from_ron_str`].
    pub fn load_from_file(factions: &FactionCatalog, path: impl AsRef<Path>) -> SimResult<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| SimCoreError::IoError(e.to_string()))?;
        Self::from_ron_str(factions, &content)
    }

    /// Looks up an upgrade by its integer [`UpgradeId`].
    #[must_use]
    pub fn get_by_id(&self, id: UpgradeId) -> Option<&UpgradeDef> {
        self.upgrades_by_id.get(&id)
    }

    /// Looks up an upgrade by its canonical slug string.
    #[must_use]
    pub fn get_by_slug(&self, slug: &str) -> Option<&UpgradeDef> {
        self.upgrades_by_slug
            .get(slug)
            .and_then(|&id| self.get_by_id(id))
    }

    /// Whether any upgrade unlocks `ability_slug` for the archetype `target_slug`.
    #[must_use]
    pub fn unlocks_ability(&self, target_slug: &str, ability_slug: &str) -> bool {
        self.upgrades().any(|upgrade| {
            upgrade.effects.iter().any(|effect| {
                matches!(
                    effect,
                    UpgradeEffect::UnlockAbility { target_slug: target, ability_slug: ability }
                        if target == target_slug && ability == ability_slug
                )
            })
        })
    }

    /// Iterates over all registered upgrade definitions.
    pub fn upgrades(&self) -> impl Iterator<Item = &UpgradeDef> {
        self.upgrades_by_id.values()
    }
}
