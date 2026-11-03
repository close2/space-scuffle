//! Faction, weapon-class and armour-class identities loaded from `assets/catalog/factions.ron`.
//!
//! # Responsibility
//! Owns the [`FactionCatalog`]: the data that says which factions exist and which weapon and
//! armour classes upgrades scale. Units, structures and upgrades name these by slug; this module
//! validates the identities and resolves each slug to the small numeric ID the simulation stores
//! ([`FactionId`], [`WeaponClassId`], [`ArmorClassId`]).
//!
//! # How it fits
//! [`super::RuntimeCatalogs::load_from_directory`] loads this file first, because the unit,
//! structure and upgrade loaders resolve their references against it. The resolved
//! [`super::UnitCatalog`] keeps a copy, so presentation code reads a faction's display name and
//! badge colour through `catalog.factions()` instead of naming the faction in Rust
//! (charter §3.3).
//!
//! # Invariants
//! - Within each table, IDs are unique and slugs are unique and non-blank.
//! - Every weapon and armour class names a faction that the same file defines.
//! - IDs are authored, never derived from file order. Class IDs are hashed into the simulation
//!   checksum (a weapon's class, and each player's researched levels keyed by class), so the
//!   shipped values equal the tags the former Rust enums had: re-recording a replay is never the
//!   fix for a changed ID.
//! - The colour is 8-bit sRGB, so no float enters the simulation crates.

use crate::error::{SimCoreError, SimResult};
use serde::Deserialize;
use sim_types::{ArmorClassId, FactionId, WeaponClassId};
use std::collections::BTreeMap;

/// One faction: its identity, the name players see, and its interface accent colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactionDef {
    /// Stable numeric identity stored on every archetype and upgrade of this faction.
    pub faction_id: FactionId,
    /// Name other catalogs use to refer to this faction.
    pub slug: String,
    /// Name shown to players, for example on the Visual Lab's faction badge.
    pub display_name: String,
    /// Badge and accent colour as 8-bit sRGB `(red, green, blue)`.
    pub accent_srgb: (u8, u8, u8),
    /// One-sentence background shown with the faction.
    pub lore: String,
}

/// One weapon class: the group of weapons a `WeaponDamageLevel` upgrade raises together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponClassDef {
    /// Stable numeric identity; hashed into the simulation checksum.
    pub class_id: WeaponClassId,
    /// Name weapons and upgrades use to refer to this class.
    pub slug: String,
    /// Faction whose upgrades and weapons use this class.
    pub faction: FactionId,
    /// Name shown to players.
    pub display_name: String,
}

/// One armour class: the group of hulls an `ArmorLevel` upgrade raises together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmorClassDef {
    /// Stable numeric identity; hashed into the simulation checksum.
    pub class_id: ArmorClassId,
    /// Name units and upgrades use to refer to this class.
    pub slug: String,
    /// Faction whose upgrades and units use this class.
    pub faction: FactionId,
    /// Name shown to players.
    pub display_name: String,
}

/// Every faction, weapon class and armour class, with lookups by ID and by slug.
///
/// Slug lookups are exact (case-sensitive). Iteration is in ascending ID order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactionCatalog {
    factions: BTreeMap<FactionId, FactionDef>,
    weapon_classes: BTreeMap<WeaponClassId, WeaponClassDef>,
    armor_classes: BTreeMap<ArmorClassId, ArmorClassDef>,
    faction_slugs: BTreeMap<String, FactionId>,
    weapon_class_slugs: BTreeMap<String, WeaponClassId>,
    armor_class_slugs: BTreeMap<String, ArmorClassId>,
}

/// The authored shape of `factions.ron`.
#[derive(Debug, Deserialize)]
struct FactionCatalogFile {
    factions: Vec<FactionDefFile>,
    weapon_classes: Vec<ClassDefFile>,
    armor_classes: Vec<ClassDefFile>,
}

/// An authored faction.
#[derive(Debug, Deserialize)]
struct FactionDefFile {
    faction_id: u8,
    slug: String,
    display_name: String,
    accent_srgb: (u8, u8, u8),
    lore: String,
}

/// An authored weapon or armour class, whose faction is still a slug.
#[derive(Debug, Deserialize)]
struct ClassDefFile {
    class_id: u8,
    slug: String,
    faction: String,
    display_name: String,
}

/// Inserts `value` under `id` and `slug`, rejecting a blank slug or a reused ID or slug.
///
/// `raw` is the authored number behind `id`, used in error messages.
fn insert_unique<Id: Ord + Copy, T>(
    kind: &'static str,
    by_id: &mut BTreeMap<Id, T>,
    by_slug: &mut BTreeMap<String, Id>,
    (raw, id, slug, value): (u8, Id, String, T),
) -> SimResult<()> {
    if slug.trim().is_empty() {
        return Err(SimCoreError::CatalogInvalidValue {
            definition: format!("{kind} {raw}"),
            field: "slug".to_owned(),
            reason: "must not be blank",
        });
    }
    if by_id.contains_key(&id) {
        return Err(SimCoreError::CatalogDuplicateId {
            kind,
            id: u32::from(raw),
        });
    }
    if by_slug.contains_key(&slug) {
        return Err(SimCoreError::CatalogDuplicateSlug { kind, slug });
    }
    by_slug.insert(slug, id);
    by_id.insert(id, value);
    Ok(())
}

impl FactionCatalog {
    /// Parses and validates the text of a `factions.ron` file.
    ///
    /// # Errors
    /// [`SimCoreError::RonDeserializationError`] for malformed RON;
    /// [`SimCoreError::CatalogDuplicateId`] or [`SimCoreError::CatalogDuplicateSlug`] for a reused
    /// identity within one table; [`SimCoreError::CatalogInvalidValue`] for a blank slug; and
    /// [`SimCoreError::UnknownFaction`] for a class whose faction is not defined in the file.
    pub fn from_ron_str(ron_data: &str) -> SimResult<Self> {
        let file: FactionCatalogFile = ron::from_str(ron_data)
            .map_err(|e| SimCoreError::RonDeserializationError(e.to_string()))?;
        let mut catalog = Self::default();
        for faction in file.factions {
            let def = FactionDef {
                faction_id: FactionId(faction.faction_id),
                slug: faction.slug.clone(),
                display_name: faction.display_name,
                accent_srgb: faction.accent_srgb,
                lore: faction.lore,
            };
            insert_unique(
                "faction",
                &mut catalog.factions,
                &mut catalog.faction_slugs,
                (faction.faction_id, def.faction_id, faction.slug, def),
            )?;
        }
        for class in file.weapon_classes {
            let def = WeaponClassDef {
                class_id: WeaponClassId(class.class_id),
                faction: catalog.resolve_faction(&class.slug, &class.faction)?,
                slug: class.slug.clone(),
                display_name: class.display_name,
            };
            insert_unique(
                "weapon class",
                &mut catalog.weapon_classes,
                &mut catalog.weapon_class_slugs,
                (class.class_id, def.class_id, class.slug, def),
            )?;
        }
        for class in file.armor_classes {
            let def = ArmorClassDef {
                class_id: ArmorClassId(class.class_id),
                faction: catalog.resolve_faction(&class.slug, &class.faction)?,
                slug: class.slug.clone(),
                display_name: class.display_name,
            };
            insert_unique(
                "armor class",
                &mut catalog.armor_classes,
                &mut catalog.armor_class_slugs,
                (class.class_id, def.class_id, class.slug, def),
            )?;
        }
        Ok(catalog)
    }

    /// The faction with `id`, if defined.
    #[must_use]
    pub fn faction(&self, id: FactionId) -> Option<&FactionDef> {
        self.factions.get(&id)
    }

    /// The faction whose slug is exactly `slug`, if defined.
    #[must_use]
    pub fn faction_by_slug(&self, slug: &str) -> Option<&FactionDef> {
        self.faction_slugs
            .get(slug)
            .and_then(|id| self.factions.get(id))
    }

    /// The weapon class with `id`, if defined.
    #[must_use]
    pub fn weapon_class(&self, id: WeaponClassId) -> Option<&WeaponClassDef> {
        self.weapon_classes.get(&id)
    }

    /// The weapon class whose slug is exactly `slug`, if defined.
    #[must_use]
    pub fn weapon_class_by_slug(&self, slug: &str) -> Option<&WeaponClassDef> {
        self.weapon_class_slugs
            .get(slug)
            .and_then(|id| self.weapon_classes.get(id))
    }

    /// The armour class with `id`, if defined.
    #[must_use]
    pub fn armor_class(&self, id: ArmorClassId) -> Option<&ArmorClassDef> {
        self.armor_classes.get(&id)
    }

    /// The armour class whose slug is exactly `slug`, if defined.
    #[must_use]
    pub fn armor_class_by_slug(&self, slug: &str) -> Option<&ArmorClassDef> {
        self.armor_class_slugs
            .get(slug)
            .and_then(|id| self.armor_classes.get(id))
    }

    /// Every faction in ascending [`FactionId`] order.
    pub fn factions(&self) -> impl Iterator<Item = &FactionDef> {
        self.factions.values()
    }

    /// Every weapon class in ascending [`WeaponClassId`] order.
    pub fn weapon_classes(&self) -> impl Iterator<Item = &WeaponClassDef> {
        self.weapon_classes.values()
    }

    /// Every armour class in ascending [`ArmorClassId`] order.
    pub fn armor_classes(&self) -> impl Iterator<Item = &ArmorClassDef> {
        self.armor_classes.values()
    }

    /// Resolves the faction slug `slug` that the definition `definition` names.
    ///
    /// # Errors
    /// [`SimCoreError::UnknownFaction`] naming `definition` and `slug` when no faction has it.
    pub fn resolve_faction(&self, definition: &str, slug: &str) -> SimResult<FactionId> {
        self.faction_slugs
            .get(slug)
            .copied()
            .ok_or_else(|| SimCoreError::UnknownFaction {
                definition: definition.to_owned(),
                slug: slug.to_owned(),
            })
    }

    /// Resolves the weapon-class slug `slug` that the definition `definition` names.
    ///
    /// # Errors
    /// [`SimCoreError::UnknownWeaponClass`] naming `definition` and `slug` when no class has it.
    pub fn resolve_weapon_class(&self, definition: &str, slug: &str) -> SimResult<WeaponClassId> {
        self.weapon_class_slugs
            .get(slug)
            .copied()
            .ok_or_else(|| SimCoreError::UnknownWeaponClass {
                definition: definition.to_owned(),
                slug: slug.to_owned(),
            })
    }

    /// Resolves the armour-class slug `slug` that the definition `definition` names.
    ///
    /// # Errors
    /// [`SimCoreError::UnknownArmorClass`] naming `definition` and `slug` when no class has it.
    pub fn resolve_armor_class(&self, definition: &str, slug: &str) -> SimResult<ArmorClassId> {
        self.armor_class_slugs
            .get(slug)
            .copied()
            .ok_or_else(|| SimCoreError::UnknownArmorClass {
                definition: definition.to_owned(),
                slug: slug.to_owned(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_FACTIONS: &str = r#"(
        factions: [
            (faction_id: 4, slug: "north", display_name: "North", accent_srgb: (1, 2, 3), lore: "n"),
            (faction_id: 1, slug: "south", display_name: "South", accent_srgb: (4, 5, 6), lore: "s"),
        ],
        weapon_classes: [
            (class_id: 7, slug: "north_guns", faction: "north", display_name: "Guns"),
            (class_id: 2, slug: "south_guns", faction: "south", display_name: "Guns"),
        ],
        armor_classes: [
            (class_id: 0, slug: "north_plate", faction: "north", display_name: "Plate"),
        ],
    )"#;

    #[test]
    fn identities_resolve_by_slug_and_id_and_iterate_in_id_order() -> SimResult<()> {
        let catalog = FactionCatalog::from_ron_str(TWO_FACTIONS)?;
        let north = catalog.resolve_faction("test", "north")?;
        assert_eq!(north, FactionId(4));
        assert_eq!(
            catalog.faction(north).map(|f| f.accent_srgb),
            Some((1, 2, 3))
        );
        assert_eq!(
            catalog.faction_by_slug("south").map(|f| f.faction_id),
            Some(FactionId(1))
        );
        assert_eq!(
            catalog.resolve_weapon_class("test", "south_guns")?,
            WeaponClassId(2)
        );
        assert_eq!(
            catalog.weapon_class(WeaponClassId(7)).map(|c| c.faction),
            Some(north)
        );
        assert_eq!(
            catalog.resolve_armor_class("test", "north_plate")?,
            ArmorClassId(0)
        );
        let order: Vec<_> = catalog.factions().map(|f| f.slug.as_str()).collect();
        assert_eq!(order, ["south", "north"]);
        let order: Vec<_> = catalog.weapon_classes().map(|c| c.class_id).collect();
        assert_eq!(order, [WeaponClassId(2), WeaponClassId(7)]);
        assert!(
            catalog.faction_by_slug("North").is_none(),
            "slugs are exact"
        );
        Ok(())
    }

    #[test]
    fn unknown_references_name_the_referring_definition() -> SimResult<()> {
        let catalog = FactionCatalog::from_ron_str(TWO_FACTIONS)?;
        assert_eq!(
            catalog.resolve_faction("pulse", "west"),
            Err(SimCoreError::UnknownFaction {
                definition: "pulse".to_owned(),
                slug: "west".to_owned()
            })
        );
        assert_eq!(
            catalog.resolve_weapon_class("pulse", "north_plate"),
            Err(SimCoreError::UnknownWeaponClass {
                definition: "pulse".to_owned(),
                slug: "north_plate".to_owned()
            })
        );
        assert_eq!(
            catalog.resolve_armor_class("pulse", "north_guns"),
            Err(SimCoreError::UnknownArmorClass {
                definition: "pulse".to_owned(),
                slug: "north_guns".to_owned()
            })
        );
        Ok(())
    }

    #[test]
    fn a_class_must_name_a_faction_of_the_same_file() {
        let text = TWO_FACTIONS.replace(
            r#"slug: "south_guns", faction: "south""#,
            r#"slug: "south_guns", faction: "east""#,
        );
        assert_eq!(
            FactionCatalog::from_ron_str(&text),
            Err(SimCoreError::UnknownFaction {
                definition: "south_guns".to_owned(),
                slug: "east".to_owned()
            })
        );
    }

    #[test]
    fn reused_ids_and_slugs_and_blank_slugs_are_rejected_per_table() {
        let cases = [
            (
                TWO_FACTIONS.replace("faction_id: 1,", "faction_id: 4,"),
                SimCoreError::CatalogDuplicateId {
                    kind: "faction",
                    id: 4,
                },
            ),
            (
                TWO_FACTIONS.replace(r#"slug: "south","#, r#"slug: "north","#),
                SimCoreError::CatalogDuplicateSlug {
                    kind: "faction",
                    slug: "north".to_owned(),
                },
            ),
            (
                TWO_FACTIONS.replace("class_id: 2,", "class_id: 7,"),
                SimCoreError::CatalogDuplicateId {
                    kind: "weapon class",
                    id: 7,
                },
            ),
            (
                TWO_FACTIONS.replace(r#""south_guns""#, r#""north_guns""#),
                SimCoreError::CatalogDuplicateSlug {
                    kind: "weapon class",
                    slug: "north_guns".to_owned(),
                },
            ),
            (
                TWO_FACTIONS.replace(r#""north_plate""#, r#""  ""#),
                SimCoreError::CatalogInvalidValue {
                    definition: "armor class 0".to_owned(),
                    field: "slug".to_owned(),
                    reason: "must not be blank",
                },
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(FactionCatalog::from_ron_str(&text), Err(expected));
        }
    }

    #[test]
    fn a_weapon_class_and_an_armor_class_may_share_an_id() -> SimResult<()> {
        let text = TWO_FACTIONS.replace("class_id: 0,", "class_id: 7,");
        let catalog = FactionCatalog::from_ron_str(&text)?;
        assert!(catalog.weapon_class(WeaponClassId(7)).is_some());
        assert!(catalog.armor_class(ArmorClassId(7)).is_some());
        Ok(())
    }
}
