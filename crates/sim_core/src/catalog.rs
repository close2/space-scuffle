//! # Unit & Structure Archetype Catalog
//!
//! Strongly-typed, data-driven catalog defining static vitals, movement attributes,
//! costs, geometry, weapon envelopes, and mechanical capabilities for all units and
//! building domes in Space Scuffle.
//!
//! ## Architectural Charter & Invariants Maintained
//! 1. **Zero Hardcoded Creature / Building Identifiers:** Archetypes are loaded dynamically
//!    from declarative data (`assets/catalog/units.ron`). Adding or rebalancing units
//!    requires zero Rust code changes and zero recompilation.
//! 2. **Deterministic Fixed-Point Parity:** All combat ranges, speeds, and timings are stored
//!    as [`SimScalar`] and [`SimAngle`].
//! 3. **O(1) Indexed Lookups:** Archetypes are registered by [`UnitTypeId`] in a contiguous slab.
//! 4. **Capability-Driven Simulation:** Capabilities (overclock/stim, harvesting, resource dropoff)
//!    are modeled as explicit optional specs, enabling data-driven behaviors.
//! 5. **Factions and Classes Are Data:** Definitions name their faction, weapon class and armor
//!    class by slug; loading resolves each against the catalog's [`FactionCatalog`]
//!    (`assets/catalog/factions.ron`) and fails with a typed error naming the definition when a
//!    slug is unknown. The catalog keeps that [`FactionCatalog`] for presentation lookups.

use crate::error::{SimCoreError, SimResult};
use crate::upgrades::{UpgradeCatalog, OVERCLOCK_ABILITY_SLUG};
use serde::{Deserialize, Serialize};
use sim_map::MAX_SIGHT_RADIUS_CELLS;
use sim_types::{
    default_projectile_lifetime_ticks, AbilitySpec, AnimState, ArchetypeCapabilities, ArmorClassId,
    AttributeFlags, FactionId, HarvesterSpec, MoverType, OverclockSpec,
    ProjectileTargetLossBehavior, ShapeKind, SilhouetteClass, SimAngle, SimScalar, UnitModeSpec,
    UnitTypeId, UpgradeEffect, WeaponVisualKind,
};
pub use sim_types::{SplashSpec, WeaponSpec};
mod factions;
mod runtime;
mod validation;

pub use factions::{ArmorClassDef, FactionCatalog, FactionDef, WeaponClassDef};
pub use runtime::{CatalogLoadError, RuntimeCatalogs};

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

/// Static archetype specification defining baseline vitals, movement, capabilities, and abilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitArchetype {
    /// Unique archetype identifier.
    pub unit_type_id: UnitTypeId,
    /// Canonical internal slug name (e.g. "`starter_infantry`").
    pub slug: String,
    /// Canonical name (matches slug for backwards compatibility).
    pub name: String,
    /// Faction this archetype belongs to, resolved from its slug in `factions.ron`.
    pub faction: FactionId,
    /// Human-readable official display name (e.g. "Aegis Pulse Bot").
    pub display_name: String,
    /// Lore background description.
    pub lore_description: String,
    /// `StarCraft` II mechanical inspiration / analogue.
    pub sc2_inspiration: Option<String>,
    /// Whether this unit is featured in the Visual Lab hero showcase.
    pub showcase_hero: bool,
    /// Sorting order in the hero showcase lineup (if featured).
    pub showcase_order: Option<u8>,
    /// Overclock / stim combat buff parameters (if capable).
    pub overclock: Option<OverclockSpec>,
    /// Resource harvesting parameters (if capable).
    pub harvester: Option<HarvesterSpec>,
    /// Whether this structure serves as a resource dropoff node.
    pub is_resource_dropoff: bool,
    /// Bundle of optional mechanical capabilities.
    pub capabilities: ArchetypeCapabilities,
    /// Generative Veo 3.1 video prompts indexed by animation state name.
    pub veo_prompts: BTreeMap<String, String>,
    /// 3D base geometry (Sphere vs Hemisphere Dome).
    pub shape: ShapeKind,
    /// Rim silhouette collar classification.
    pub silhouette: SilhouetteClass,
    /// Locomotion classification (Ground, Air, `CliffWalker`).
    pub mover_type: MoverType,
    /// Armor classification and category bitflags.
    pub attributes: AttributeFlags,
    /// Maximum hull hit points.
    pub max_hp: SimScalar,
    /// Maximum plasma shield points.
    pub max_shields: SimScalar,
    /// Maximum energy points for abilities.
    pub max_energy: SimScalar,
    /// Hull life armor value reducing non-shield damage.
    pub life_armor: SimScalar,
    /// Shield armor value reducing shield damage.
    pub shield_armor: SimScalar,
    /// Passive health regeneration per tick.
    pub hp_regen_per_tick: SimScalar,
    /// Passive shield regeneration per tick (active after 250 ticks without taking damage).
    pub shield_regen_per_tick: SimScalar,
    /// Passive energy regeneration per tick.
    pub energy_regen_per_tick: SimScalar,
    /// Maximum linear translation speed per tick.
    pub max_speed: SimScalar,
    /// Acceleration increase per tick.
    pub acceleration: SimScalar,
    /// Deceleration braking decrease per tick.
    pub deceleration: SimScalar,
    /// Hull angular tracking speed per tick.
    pub turn_rate: SimAngle,
    /// Hard circle-circle collision radius in world units.
    pub collision_radius: SimScalar,
    /// Soft Boid repulsion boundary radius in world units.
    pub separation_radius: SimScalar,
    /// Integer priority mass used to resolve collision pushes (higher pushes lower).
    pub push_priority: u8,
    /// Line-of-sight sight reveal radius in world units.
    pub sight_radius: SimScalar,
    /// Auto-acquisition threat scanning radius in world units.
    pub scan_range: SimScalar,
    /// Mineral resource cost.
    pub mineral_cost: u32,
    /// Vespene gas resource cost.
    pub gas_cost: u32,
    /// Supply food consumed.
    pub supply_cost: u32,
    /// Supply capacity provided.
    pub supply_provided: u32,
    /// Construction / training duration in ticks (at 25 Hz).
    pub build_time_ticks: u32,
    /// Primary ground-engagement weapon configuration.
    #[serde(default)]
    pub ground_weapon: Option<WeaponSpec>,
    /// Specialized anti-air weapon configuration, if equipped.
    #[serde(default)]
    pub air_weapon: Option<WeaponSpec>,
    /// Primary weapon configuration, if equipped (mirrors `ground_weapon` for backwards compatibility).
    #[serde(default)]
    pub weapon: Option<WeaponSpec>,
    /// Whether this entity detects cloaked / burrowed units.
    pub is_detector: bool,
    /// Active tactical abilities and spells.
    #[serde(default)]
    pub abilities: Vec<AbilitySpec>,
    /// Prerequisite structure slugs that must exist and be completed before construction can start.
    #[serde(default)]
    pub requires_structures: Vec<String>,
    /// Unit slugs that can be trained/constructed at this dome.
    #[serde(default)]
    pub trains_units: Vec<String>,
    /// Upgrade slugs that can be researched at this dome.
    #[serde(default)]
    pub researches_upgrades: Vec<String>,
    /// Armor class whose `ArmorLevel` upgrades raise this hull's life armor, resolved from its
    /// slug in `factions.ron`; `None` for a hull no upgrade raises.
    #[serde(default)]
    pub armor_class: Option<ArmorClassId>,
    /// Declarative transformation stances and operational modes (e.g. Siege Mode, Burrowed Mode).
    #[serde(default)]
    pub modes: Vec<UnitModeSpec>,
}

impl UnitArchetype {
    /// Whether this unit can raise `structure`. The one rule admission and the card share.
    #[must_use]
    pub fn can_raise(&self, structure: &Self) -> bool {
        self.capabilities.can_construct
            && self.faction == structure.faction
            && matches!(structure.shape, ShapeKind::BuildingDome { .. })
    }

    /// Returns whether this archetype is immobile by default (e.g. Building Dome or zero max speed).
    #[must_use]
    #[inline]
    pub fn is_immobile_by_default(&self) -> bool {
        matches!(self.shape, ShapeKind::BuildingDome { .. }) || self.max_speed <= SimScalar::ZERO
    }

    /// Looks up a declarative mode specification by canonical mode name.
    #[must_use]
    pub fn get_mode(&self, name: &str) -> Option<&UnitModeSpec> {
        self.modes.iter().find(|m| m.mode_name == name)
    }

    /// Selects the optimal weapon against a target with specified locomotion and optional siege stance weapon.
    ///
    /// Against [`MoverType::CliffWalker`] (e.g., Colossus), the target can be hit by both ground and air
    /// weapons. The unit selects whichever weapon yields higher DPS:
    /// `DPS = (base_damage * attacks_count * 1000) / period_ticks`. Ties are broken in favor of ground weapon.
    #[must_use]
    pub fn weapon_against<'a>(
        &'a self,
        target_mover: MoverType,
        is_sieged: Option<&'a WeaponSpec>,
    ) -> Option<&'a WeaponSpec> {
        let ground = if let Some(siege) = is_sieged {
            Some(siege)
        } else {
            self.ground_weapon.as_ref().or(self.weapon.as_ref())
        };
        let air = self.air_weapon.as_ref();

        match target_mover {
            MoverType::Ground | MoverType::CliffJumper => ground,
            MoverType::Air => air,
            MoverType::CliffWalker => match (ground, air) {
                (Some(g), Some(a)) => {
                    let dps_g = (g.base_damage.raw() * i64::from(g.attacks_count) * 1000)
                        / i64::from(g.period_ticks.max(1));
                    let dps_a = (a.base_damage.raw() * i64::from(a.attacks_count) * 1000)
                        / i64::from(a.period_ticks.max(1));
                    if dps_a > dps_g {
                        Some(a)
                    } else {
                        Some(g)
                    }
                }
                (Some(g), None) => Some(g),
                (None, Some(a)) => Some(a),
                (None, None) => None,
            },
        }
    }

    /// Selects weapon for target locomotion and attribute flags.
    #[must_use]
    pub fn weapon_for_target(
        &self,
        target_mover: MoverType,
        _target_attributes: AttributeFlags,
    ) -> Option<&WeaponSpec> {
        self.weapon_against(target_mover, None)
    }
    /// Lore background description.
    #[must_use]
    pub fn lore(&self) -> &str {
        &self.lore_description
    }

    /// `StarCraft` II mechanical inspiration / analogue.
    #[must_use]
    pub fn sc2_analogue(&self) -> Option<&str> {
        self.sc2_inspiration.as_deref()
    }

    /// Resolves the Veo 3.1 action generation prompt for a specific animation state.
    #[must_use]
    pub fn veo_prompt(&self, state: AnimState) -> Option<&str> {
        let key = format!("{state:?}");
        self.veo_prompts.get(&key).map(String::as_str)
    }
}

// PERFORMANCE RATIONALE:
// Why: Static archetype definitions prevent repetitive heap allocation and parsing during ticks.
// What: Archetypes are stored in a contiguous, flat `Vec<Option<UnitArchetype>>` indexed by `UnitTypeId` in O(1) time.
// Trade-off: Deserialized once at engine startup into immutable lookup tables.

/// Registry containing all known unit and structure archetypes indexed by [`UnitTypeId`].
#[derive(Debug, Clone)]
pub struct UnitCatalog {
    archetypes: Vec<Option<UnitArchetype>>,
    slug_index: OnceLock<Vec<(String, usize)>>,
    factions: FactionCatalog,
}

impl UnitCatalog {
    /// Deserializes a catalog from a raw RON string, resolving faction and class slugs against
    /// `factions`, which the catalog keeps (see [`Self::factions`]).
    ///
    /// # Errors
    /// Returns [`SimCoreError`] for malformed RON, duplicate identities, blank slugs,
    /// numeric values that cannot be safely converted to simulation types, or a faction or
    /// class slug `factions` does not define ([`SimCoreError::UnknownFaction`],
    /// [`SimCoreError::UnknownWeaponClass`], [`SimCoreError::UnknownArmorClass`]).
    pub fn from_ron_str(factions: &FactionCatalog, ron_data: &str) -> SimResult<Self> {
        let catalog_file: CatalogFile = ron::from_str(ron_data)
            .map_err(|e| SimCoreError::RonDeserializationError(e.to_string()))?;

        let mut catalog = Self {
            archetypes: vec![None; 128],
            slug_index: OnceLock::new(),
            factions: factions.clone(),
        };
        catalog.register_authored_batch(catalog_file.units.into_iter().map(|def| {
            def.validate()?;
            def.into_archetype(factions)
        }))?;
        catalog.slug_index();
        Ok(catalog)
    }

    /// The faction, weapon-class and armor-class identities this catalog was resolved against.
    #[must_use]
    pub fn factions(&self) -> &FactionCatalog {
        &self.factions
    }

    /// Loads additional structures from a raw RON string and registers them into the catalog.
    ///
    /// # Errors
    /// Returns [`SimCoreError`] for malformed or invalid definitions. The catalog
    /// remains unchanged if any addition fails validation or conflicts with an identity.
    pub fn load_structures_from_ron_str(&mut self, structures_ron: &str) -> SimResult<()> {
        let catalog_file: StructureCatalogFile = ron::from_str(structures_ron)
            .map_err(|e| SimCoreError::RonDeserializationError(e.to_string()))?;

        // Stage the complete addition so a later malformed definition cannot leave
        // callers with an unexpectedly partially modified catalog.
        let mut staged = self.clone();
        let factions = &self.factions;
        staged.register_authored_batch(catalog_file.structures.into_iter().map(|def| {
            def.validate()?;
            def.into_archetype(factions)
        }))?;
        staged.slug_index();
        *self = staged;
        Ok(())
    }

    /// Deserializes and consolidates both units and structures RON strings into a unified catalog,
    /// resolving faction and class slugs against `factions`.
    ///
    /// # Errors
    /// Returns [`SimCoreError`] if either file is malformed, contains unsafe values, names an
    /// unknown faction or class, or reuses an ID or ASCII-insensitive slug within the combined
    /// catalog.
    pub fn from_ron_str_bundle(
        factions: &FactionCatalog,
        units_ron: &str,
        structures_ron: &str,
    ) -> SimResult<Self> {
        let mut catalog = Self::from_ron_str(factions, units_ron)?;
        catalog.load_structures_from_ron_str(structures_ron)?;
        catalog.validate_production_references()?;
        Ok(catalog)
    }

    /// Deserializes the units file at `path`, resolving slugs against `factions`.
    ///
    /// # Errors
    /// Returns [`SimCoreError::IoError`] if the file cannot be read, or any error of
    /// [`Self::from_ron_str`].
    pub fn load_from_file(factions: &FactionCatalog, path: impl AsRef<Path>) -> SimResult<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| SimCoreError::IoError(e.to_string()))?;
        Self::from_ron_str(factions, &content)
    }

    /// Alias for [`Self::load_from_file`].
    ///
    /// # Errors
    /// Returns [`SimCoreError::IoError`] if the file cannot be read, or any error of
    /// [`Self::from_ron_str`].
    pub fn from_file(factions: &FactionCatalog, path: impl AsRef<Path>) -> SimResult<Self> {
        Self::load_from_file(factions, path)
    }

    /// Registers or replaces an archetype at its numeric ID for programmatic editing.
    ///
    /// Unlike authored file loading, this compatibility API permits duplicate slugs;
    /// slug lookup then chooses the lowest storage ID, matching historical behavior.
    pub fn register(&mut self, archetype: UnitArchetype) {
        self.slug_index.take();
        let idx = archetype.unit_type_id.0 as usize;
        if idx >= self.archetypes.len() {
            self.archetypes.resize(idx + 1, None);
        }
        self.archetypes[idx] = Some(archetype);
    }

    fn register_authored_batch(
        &mut self,
        definitions: impl IntoIterator<Item = SimResult<UnitArchetype>>,
    ) -> SimResult<()> {
        let mut ids: BTreeSet<_> = self
            .archetypes
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| entry.as_ref().map(|_| slot))
            .collect();
        let mut slugs: BTreeSet<_> = self
            .archetypes()
            .map(|entry| entry.slug.to_ascii_lowercase())
            .collect();
        for definition in definitions {
            let archetype = definition?;
            if !ids.insert(usize::from(archetype.unit_type_id.0)) {
                return Err(SimCoreError::CatalogDuplicateId {
                    kind: "unit or structure",
                    id: u32::from(archetype.unit_type_id.0),
                });
            }
            if !slugs.insert(archetype.slug.to_ascii_lowercase()) {
                return Err(SimCoreError::CatalogDuplicateSlug {
                    kind: "unit or structure",
                    slug: archetype.slug,
                });
            }
            self.register(archetype);
        }
        Ok(())
    }

    /// Looks up an archetype by [`UnitTypeId`].
    ///
    /// # Errors
    /// Returns [`SimCoreError::ArchetypeNotFound`] if the archetype is not registered.
    pub fn get(&self, id: UnitTypeId) -> SimResult<&UnitArchetype> {
        let idx = id.0 as usize;
        self.archetypes
            .get(idx)
            .and_then(|opt| opt.as_ref())
            .ok_or(SimCoreError::ArchetypeNotFound(id.0))
    }

    /// Looks up a mutable reference to an archetype by [`UnitTypeId`].
    ///
    /// Invalidates the derived slug index before returning, so edits to identity
    /// fields cannot leave a stale lookup. The next slug query rebuilds the index.
    ///
    /// # Errors
    /// Returns [`SimCoreError::ArchetypeNotFound`] if the archetype is not registered.
    pub fn get_mut(&mut self, id: UnitTypeId) -> SimResult<&mut UnitArchetype> {
        self.slug_index.take();
        let idx = id.0 as usize;
        self.archetypes
            .get_mut(idx)
            .and_then(|opt| opt.as_mut())
            .ok_or(SimCoreError::ArchetypeNotFound(id.0))
    }

    /// Looks up an archetype by ASCII-insensitive slug with an allocation-free query.
    ///
    /// The first query following a programmatic edit rebuilds the derived index;
    /// loaded catalogs have the index prepared before being returned to the caller.
    #[must_use]
    pub fn get_by_slug(&self, slug: &str) -> Option<&UnitArchetype> {
        let index = self.slug_index();
        let offset = index.partition_point(|(candidate, _)| compare_slugs(candidate, slug).is_lt());
        let (candidate, slot) = index.get(offset)?;
        if !candidate.eq_ignore_ascii_case(slug) {
            return None;
        }
        self.archetypes.get(*slot).and_then(Option::as_ref)
    }

    fn slug_index(&self) -> &[(String, usize)] {
        // PERFORMANCE RATIONALE:
        // Why: Slug resolution occurs in combat upgrade and scenario queries.
        // What: Rebuild a sorted index once after edits, then binary-search folded
        // ASCII bytes without allocating a normalized query string.
        // Trade-off: O(N log N) rebuild and owned slug copies in exchange for O(log N)
        // steady-state lookup. OnceLock keeps this per-catalog cache Send + Sync.
        self.slug_index.get_or_init(|| {
            let mut index: Vec<_> = self
                .archetypes
                .iter()
                .enumerate()
                .filter_map(|(slot, archetype)| archetype.as_ref().map(|a| (a.slug.clone(), slot)))
                .collect();
            index.sort_by(|(left, left_slot), (right, right_slot)| {
                compare_slugs(left, right).then_with(|| left_slot.cmp(right_slot))
            });
            index
        })
    }

    /// Resolves a [`UnitTypeId`] by canonical slug.
    #[must_use]
    pub fn id_by_slug(&self, slug: &str) -> Option<UnitTypeId> {
        self.get_by_slug(slug).map(|arch| arch.unit_type_id)
    }

    /// Looks up an archetype by internal slug name or official display name.
    #[must_use]
    pub fn get_by_name(&self, name: &str) -> Option<&UnitArchetype> {
        self.archetypes.iter().flatten().find(|arch| {
            arch.slug.eq_ignore_ascii_case(name)
                || arch.name.eq_ignore_ascii_case(name)
                || arch.display_name.eq_ignore_ascii_case(name)
        })
    }

    /// Validates production/prerequisite references after all unit and structure files are loaded.
    /// Partial legacy parsers deliberately do not impose this complete-bundle boundary.
    ///
    /// # Errors
    /// Returns a typed authored field error for blank, duplicate, dangling or nonstructure prerequisites.
    pub fn validate_production_references(&self) -> SimResult<()> {
        for archetype in self.archetypes() {
            for (field, references) in [
                ("trains_units", &archetype.trains_units),
                ("requires_structures", &archetype.requires_structures),
            ] {
                let mut seen = BTreeSet::new();
                for reference in references {
                    let fail = |reason| SimCoreError::CatalogInvalidValue {
                        definition: archetype.slug.clone(),
                        field: field.to_owned(),
                        reason,
                    };
                    if reference.trim().is_empty() {
                        return Err(fail("reference must not be blank"));
                    }
                    let target = self.get_by_slug(reference).ok_or_else(|| {
                        fail("reference does not resolve in the complete catalog")
                    })?;
                    if !seen.insert(target.unit_type_id) {
                        return Err(fail("duplicate reference under catalog lookup semantics"));
                    }
                    if field == "requires_structures"
                        && !matches!(target.shape, ShapeKind::BuildingDome { .. })
                    {
                        return Err(fail("prerequisite must reference a structure"));
                    }
                }
            }
        }
        Ok(())
    }

    /// Checks every reference between this catalog and `upgrades`.
    ///
    /// Each archetype's `researches_upgrades` must name a real upgrade, and each upgrade's
    /// `researched_at` and `requires_structures` must name structures in this catalog. Without
    /// this, a typo in any of the three files loads cleanly and only surfaces as a research
    /// option that can never be taken.
    ///
    /// # Errors
    /// Returns [`SimCoreError::CatalogInvalidValue`] naming the first unresolved reference.
    pub fn validate_upgrade_references(&self, upgrades: &UpgradeCatalog) -> SimResult<()> {
        let unresolved =
            |definition: &str, field: &str, reason| SimCoreError::CatalogInvalidValue {
                definition: definition.to_owned(),
                field: field.to_owned(),
                reason,
            };
        for archetype in self.archetypes() {
            for reference in &archetype.researches_upgrades {
                if upgrades.get_by_slug(reference).is_none() {
                    return Err(unresolved(
                        &archetype.slug,
                        "researches_upgrades",
                        "reference does not resolve in the upgrade catalog",
                    ));
                }
            }
        }
        let is_structure = |slug: &str| {
            self.get_by_slug(slug)
                .is_some_and(|target| matches!(target.shape, ShapeKind::BuildingDome { .. }))
        };
        for upgrade in upgrades.upgrades() {
            if !is_structure(&upgrade.researched_at) {
                return Err(unresolved(
                    &upgrade.slug,
                    "researched_at",
                    "must reference a structure in the unit catalog",
                ));
            }
            if !upgrade
                .requires_structures
                .iter()
                .all(|slug| is_structure(slug))
            {
                return Err(unresolved(
                    &upgrade.slug,
                    "requires_structures",
                    "must reference structures in the unit catalog",
                ));
            }
            for effect in &upgrade.effects {
                self.validate_upgrade_effect(&upgrade.slug, effect)?;
            }
        }
        self.validate_upgraded_sight(upgrades)
    }

    /// Rejects an effect the simulation would have nothing to apply to.
    fn validate_upgrade_effect(&self, upgrade: &str, effect: &UpgradeEffect) -> SimResult<()> {
        let unresolved = |field: &str, reason| SimCoreError::CatalogInvalidValue {
            definition: upgrade.to_owned(),
            field: field.to_owned(),
            reason,
        };
        match effect {
            UpgradeEffect::ModifyUnitStat { target_slug, .. }
                if self.get_by_slug(target_slug).is_none() =>
            {
                Err(unresolved(
                    "ModifyUnitStat.target_slug",
                    "must reference an archetype in the unit catalog",
                ))
            }
            UpgradeEffect::UnlockAbility {
                target_slug,
                ability_slug,
            } => {
                let target = self.get_by_slug(target_slug).ok_or_else(|| {
                    unresolved(
                        "UnlockAbility.target_slug",
                        "must reference an archetype in the unit catalog",
                    )
                })?;
                let exists = (ability_slug == OVERCLOCK_ABILITY_SLUG && target.overclock.is_some())
                    || target.abilities.iter().any(|a| a.name == *ability_slug);
                if exists {
                    Ok(())
                } else {
                    Err(unresolved(
                        "UnlockAbility.ability_slug",
                        "must name an ability the target archetype has",
                    ))
                }
            }
            _ => Ok(()),
        }
    }

    /// Rejects research that would carry a unit past the sight radius vision supports.
    fn validate_upgraded_sight(&self, upgrades: &UpgradeCatalog) -> SimResult<()> {
        let mut sight_gains: BTreeMap<&str, SimScalar> = BTreeMap::new();
        for effect in upgrades.upgrades().flat_map(|upgrade| &upgrade.effects) {
            if let UpgradeEffect::ModifyUnitStat {
                target_slug,
                sight_delta,
                ..
            } = effect
            {
                if *sight_delta > SimScalar::ZERO {
                    *sight_gains.entry(target_slug).or_default() += *sight_delta;
                }
            }
        }
        for (slug, gain) in sight_gains {
            let upgraded = self
                .get_by_slug(slug)
                .map_or(gain, |archetype| archetype.sight_radius + gain);
            if upgraded > SimScalar::from_num(MAX_SIGHT_RADIUS_CELLS) {
                return Err(SimCoreError::CatalogInvalidValue {
                    definition: slug.to_owned(),
                    field: "sight_delta".to_owned(),
                    reason: "researched sight must not exceed the maximum vision supports",
                });
            }
        }
        Ok(())
    }

    /// Iterates over all registered archetypes in the catalog.
    pub fn archetypes(&self) -> impl Iterator<Item = &UnitArchetype> {
        self.archetypes.iter().flatten()
    }
}

fn compare_slugs(left: &str, right: &str) -> std::cmp::Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
}

// -----------------------------------------------------------------------------
// Declarative Deserialization Intermediate Data Schema
// -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CatalogFile {
    units: Vec<UnitArchetypeDef>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum AttributeFlagsDef {
    Bits { bits: u16 },
    Num(u16),
    Named(AttributeFlags),
}

impl AttributeFlagsDef {
    fn into_flags(self) -> AttributeFlags {
        match self {
            Self::Bits { bits } => AttributeFlags::from_bits_truncate(bits),
            Self::Num(num) => AttributeFlags::from_bits_truncate(num),
            Self::Named(flags) => flags,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OverclockSpecDef {
    hp_cost: f32,
    duration_ticks: u16,
    speed_multiplier: f32,
    attack_rate_multiplier: f32,
}

#[derive(Debug, Deserialize)]
struct HarvesterSpecDef {
    harvest_capacity: u32,
    harvest_rate_ticks: u16,
}

#[derive(Debug, Default, Deserialize)]
struct CapabilitiesDef {
    #[serde(default)]
    overclock: Option<OverclockSpecDef>,
    #[serde(default)]
    harvester: Option<HarvesterSpecDef>,
    #[serde(default)]
    is_resource_dropoff: bool,
    #[serde(default)]
    can_construct: bool,
    #[serde(default)]
    transport_capacity: u32,
    #[serde(default)]
    abilities: Vec<AbilitySpec>,
    #[serde(default)]
    modes: Vec<UnitModeSpecDef>,
}

#[derive(Debug, Deserialize)]
#[allow(clippy::option_option)]
struct UnitModeSpecDef {
    mode_name: String,
    #[serde(default)]
    cast_ticks: u16,
    #[serde(default)]
    is_immobile: bool,
    #[serde(default)]
    unpushable: bool,
    #[serde(default)]
    is_cloaked: bool,
    #[serde(default)]
    mover_type_override: Option<MoverType>,
    #[serde(default)]
    sight_radius_override: Option<f32>,
    #[serde(default)]
    ground_weapon_override: Option<Option<WeaponSpecDef>>,
    #[serde(default)]
    air_weapon_override: Option<Option<WeaponSpecDef>>,
    #[serde(default)]
    energy_cost_per_second: f32,
}

impl UnitModeSpecDef {
    fn into_mode_spec(
        self,
        factions: &FactionCatalog,
        definition: &str,
    ) -> SimResult<UnitModeSpec> {
        let resolve = |weapon: Option<Option<WeaponSpecDef>>| {
            weapon
                .map(|w_opt| {
                    w_opt
                        .map(|w| UnitArchetypeDef::convert_weapon(w, factions, definition))
                        .transpose()
                })
                .transpose()
        };
        Ok(UnitModeSpec {
            mode_name: self.mode_name,
            cast_ticks: self.cast_ticks,
            is_immobile: self.is_immobile,
            unpushable: self.unpushable,
            is_cloaked: self.is_cloaked,
            mover_type_override: self.mover_type_override,
            sight_radius_override: self.sight_radius_override.map(SimScalar::from_f32_lossy),
            ground_weapon_override: resolve(self.ground_weapon_override)?,
            air_weapon_override: resolve(self.air_weapon_override)?,
            energy_cost_per_second: SimScalar::from_f32_lossy(self.energy_cost_per_second),
        })
    }
}

#[derive(Debug, Deserialize)]
struct SplashSpecDef {
    inner_radius: f32,
    middle_radius: f32,
    outer_radius: f32,
    #[serde(default)]
    friendly_fire: bool,
}

#[derive(Debug, Deserialize)]
struct WeaponSpecDef {
    name: String,
    visual_kind: WeaponVisualKind,
    target_flags: AttributeFlagsDef,
    min_range: f32,
    max_range: f32,
    firing_arc_deg: f32,
    has_turret: bool,
    turret_turn_rate_deg: f32,
    base_damage: f32,
    bonus_armored: f32,
    bonus_light: f32,
    attacks_count: u8,
    period_ticks: u16,
    damage_point_ticks: u16,
    backswing_ticks: u16,
    splash: Option<SplashSpecDef>,
    projectile_speed: Option<f32>,
    #[serde(default)]
    weapon_class: Option<String>,
    #[serde(default)]
    upgrade_bonus_per_level: f32,
    #[serde(default)]
    bonus_upgrade_per_level: f32,
    #[serde(default)]
    target_loss_behavior: ProjectileTargetLossBehavior,
    #[serde(default = "default_projectile_lifetime_ticks")]
    max_projectile_lifetime_ticks: u16,
    #[serde(default)]
    pub suicide_on_attack: bool,
}

#[derive(Debug, Deserialize)]
struct UnitArchetypeDef {
    slug: String,
    unit_type_id: u16,
    faction: String,
    display_name: String,
    lore: String,
    sc2_analogue: String,
    showcase_hero: bool,
    showcase_order: Option<u8>,
    shape: ShapeKind,
    silhouette: SilhouetteClass,
    mover_type: MoverType,
    attributes: AttributeFlagsDef,
    max_hp: f32,
    max_shields: f32,
    max_energy: f32,
    life_armor: f32,
    shield_armor: f32,
    hp_regen_per_tick: f32,
    shield_regen_per_tick: f32,
    energy_regen_per_tick: f32,
    max_speed: f32,
    acceleration: f32,
    deceleration: f32,
    turn_rate_deg: f32,
    collision_radius: f32,
    separation_radius: f32,
    push_priority: u8,
    sight_radius: f32,
    scan_range: f32,
    mineral_cost: u32,
    gas_cost: u32,
    supply_cost: u32,
    supply_provided: u32,
    build_time_ticks: u32,
    #[serde(default)]
    ground_weapon: Option<WeaponSpecDef>,
    #[serde(default)]
    air_weapon: Option<WeaponSpecDef>,
    #[serde(default)]
    weapon: Option<WeaponSpecDef>,
    is_detector: bool,
    #[serde(default)]
    capabilities: CapabilitiesDef,
    #[serde(default)]
    abilities: Vec<AbilitySpec>,
    #[serde(default)]
    veo_prompts: BTreeMap<AnimState, String>,
    #[serde(default)]
    requires_structures: Vec<String>,
    #[serde(default)]
    trains_units: Vec<String>,
    #[serde(default)]
    researches_upgrades: Vec<String>,
    #[serde(default)]
    armor_class: Option<String>,
    #[serde(default)]
    modes: Vec<UnitModeSpecDef>,
}

impl UnitArchetypeDef {
    /// Converts an authored firing arc in degrees to the maximum aim deviation in binary angle.
    ///
    /// `is_within_firing_arc` compares the shortest angular difference, which never exceeds 180
    /// degrees, so any arc of 180 degrees or more is unrestricted. Binary angles wrap at 360
    /// degrees, so an authored 360 must not pass through `from_degrees`, which would map it to
    /// an arc of zero: the strictest possible value rather than the most permissive.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "validated finite and below 180 here, and arcs are authored in whole degrees"
    )]
    fn firing_half_angle(degrees: f32) -> SimAngle {
        if degrees >= 180.0 {
            SimAngle::DEG_180
        } else {
            SimAngle::from_degrees(degrees as i32)
        }
    }

    /// Converts an authored weapon of the definition `definition`, resolving its class slug.
    fn convert_weapon(
        w: WeaponSpecDef,
        factions: &FactionCatalog,
        definition: &str,
    ) -> SimResult<WeaponSpec> {
        let weapon_class = w
            .weapon_class
            .as_deref()
            .map(|slug| factions.resolve_weapon_class(definition, slug))
            .transpose()?;
        Ok(WeaponSpec {
            name: w.name,
            visual_kind: w.visual_kind,
            target_flags: w.target_flags.into_flags(),
            min_range: SimScalar::from_f32_lossy(w.min_range),
            max_range: SimScalar::from_f32_lossy(w.max_range),
            firing_arc: Self::firing_half_angle(w.firing_arc_deg),
            has_turret: w.has_turret,
            turret_turn_rate: {
                if !w.has_turret && w.turret_turn_rate_deg <= 0.0 {
                    SimAngle::PI
                } else {
                    let deg = if w.turret_turn_rate_deg > 360.0 {
                        f64::from(w.turret_turn_rate_deg) / 25.0
                    } else {
                        f64::from(w.turret_turn_rate_deg)
                    };
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    SimAngle::from_raw(((deg * 65536.0) / 360.0).round() as u16)
                }
            },
            base_damage: SimScalar::from_f32_lossy(w.base_damage),
            bonus_armored: SimScalar::from_f32_lossy(w.bonus_armored),
            bonus_light: SimScalar::from_f32_lossy(w.bonus_light),
            attacks_count: w.attacks_count,
            period_ticks: w.period_ticks,
            damage_point_ticks: w.damage_point_ticks,
            backswing_ticks: w.backswing_ticks,
            splash: w.splash.map(|s| SplashSpec {
                inner_radius: SimScalar::from_f32_lossy(s.inner_radius),
                middle_radius: SimScalar::from_f32_lossy(s.middle_radius),
                outer_radius: SimScalar::from_f32_lossy(s.outer_radius),
                friendly_fire: s.friendly_fire,
            }),
            projectile_speed: w.projectile_speed.map(SimScalar::from_f32_lossy),
            weapon_class,
            upgrade_bonus_per_level: SimScalar::from_f32_lossy(w.upgrade_bonus_per_level),
            bonus_upgrade_per_level: SimScalar::from_f32_lossy(w.bonus_upgrade_per_level),
            target_loss_behavior: w.target_loss_behavior,
            max_projectile_lifetime_ticks: w.max_projectile_lifetime_ticks,
            suicide_on_attack: w.suicide_on_attack,
        })
    }

    /// The authored hull turn rate (`turn_rate_deg`, degrees per tick) clamped to `0..=180`.
    fn turn_rate(&self) -> SimAngle {
        let deg = self.turn_rate_deg.clamp(0.0, 180.0);
        if deg >= 180.0 {
            SimAngle::PI
        } else {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "clamped to 0..180 above, and rates are authored in whole degrees"
            )]
            SimAngle::from_degrees(deg as i32)
        }
    }

    /// The capability bundle, abilities and modes of this definition.
    ///
    /// Abilities and modes authored under `capabilities` take precedence over the legacy
    /// top-level lists. Mode weapon overrides resolve their class against `factions`.
    fn capabilities(
        &mut self,
        factions: &FactionCatalog,
    ) -> SimResult<(ArchetypeCapabilities, Vec<AbilitySpec>, Vec<UnitModeSpec>)> {
        let authored = std::mem::take(&mut self.capabilities);
        let overclock = authored.overclock.map(|o| OverclockSpec {
            hp_cost: SimScalar::from_f32_lossy(o.hp_cost),
            duration_ticks: o.duration_ticks,
            speed_multiplier: SimScalar::from_f32_lossy(o.speed_multiplier),
            attack_rate_multiplier: SimScalar::from_f32_lossy(o.attack_rate_multiplier),
        });
        let harvester = authored.harvester.map(|h| HarvesterSpec {
            harvest_capacity: h.harvest_capacity,
            harvest_rate_ticks: h.harvest_rate_ticks,
        });
        let abilities = if authored.abilities.is_empty() {
            std::mem::take(&mut self.abilities)
        } else {
            authored.abilities
        };
        let authored_modes = if authored.modes.is_empty() {
            std::mem::take(&mut self.modes)
        } else {
            authored.modes
        };
        let modes = authored_modes
            .into_iter()
            .map(|mode| mode.into_mode_spec(factions, &self.slug))
            .collect::<SimResult<Vec<_>>>()?;
        let capabilities = ArchetypeCapabilities {
            overclock,
            harvester,
            is_resource_dropoff: authored.is_resource_dropoff,
            is_gas_extractor: false,
            can_construct: authored.can_construct,
            transport_capacity: authored.transport_capacity,
            abilities: abilities.clone(),
            modes: modes.clone(),
        };
        Ok((capabilities, abilities, modes))
    }

    /// Converts this definition, resolving its faction, armor class and weapon classes against
    /// `factions`.
    fn into_archetype(mut self, factions: &FactionCatalog) -> SimResult<UnitArchetype> {
        let faction = factions.resolve_faction(&self.slug, &self.faction)?;
        let armor_class = self
            .armor_class
            .as_deref()
            .map(|slug| factions.resolve_armor_class(&self.slug, slug))
            .transpose()?;
        let (capabilities, abilities, modes) = self.capabilities(factions)?;
        let convert = |weapon: Option<WeaponSpecDef>| {
            weapon
                .map(|w| Self::convert_weapon(w, factions, &self.slug))
                .transpose()
        };
        let weapon = convert(self.weapon.take())?;
        let ground_weapon = convert(self.ground_weapon.take())?.or_else(|| weapon.clone());
        let air_weapon = convert(self.air_weapon.take())?;
        let veo_prompts = std::mem::take(&mut self.veo_prompts)
            .into_iter()
            .map(|(state, prompt)| (format!("{state:?}"), prompt))
            .collect();
        let turn_rate = self.turn_rate();

        Ok(UnitArchetype {
            unit_type_id: UnitTypeId(self.unit_type_id),
            name: self.slug.clone(),
            slug: self.slug,
            faction,
            display_name: self.display_name,
            lore_description: self.lore,
            sc2_inspiration: Some(self.sc2_analogue),
            showcase_hero: self.showcase_hero,
            showcase_order: self.showcase_order,
            overclock: capabilities.overclock,
            harvester: capabilities.harvester,
            is_resource_dropoff: capabilities.is_resource_dropoff,
            capabilities,
            veo_prompts,
            shape: self.shape,
            silhouette: self.silhouette,
            mover_type: self.mover_type,
            attributes: self.attributes.into_flags(),
            max_hp: SimScalar::from_f32_lossy(self.max_hp),
            max_shields: SimScalar::from_f32_lossy(self.max_shields),
            max_energy: SimScalar::from_f32_lossy(self.max_energy),
            life_armor: SimScalar::from_f32_lossy(self.life_armor),
            shield_armor: SimScalar::from_f32_lossy(self.shield_armor),
            hp_regen_per_tick: SimScalar::from_f32_lossy(self.hp_regen_per_tick),
            shield_regen_per_tick: SimScalar::from_f32_lossy(self.shield_regen_per_tick),
            energy_regen_per_tick: SimScalar::from_f32_lossy(self.energy_regen_per_tick),
            max_speed: SimScalar::from_f32_lossy(self.max_speed),
            acceleration: SimScalar::from_f32_lossy(self.acceleration),
            deceleration: SimScalar::from_f32_lossy(self.deceleration),
            turn_rate,
            collision_radius: SimScalar::from_f32_lossy(self.collision_radius),
            separation_radius: SimScalar::from_f32_lossy(self.separation_radius),
            push_priority: self.push_priority,
            sight_radius: SimScalar::from_f32_lossy(self.sight_radius),
            scan_range: SimScalar::from_f32_lossy(self.scan_range),
            mineral_cost: self.mineral_cost,
            gas_cost: self.gas_cost,
            supply_cost: self.supply_cost,
            supply_provided: self.supply_provided,
            build_time_ticks: self.build_time_ticks,
            ground_weapon,
            air_weapon,
            weapon,
            is_detector: self.is_detector,
            abilities,
            requires_structures: self.requires_structures,
            trains_units: self.trains_units,
            researches_upgrades: self.researches_upgrades,
            armor_class,
            modes,
        })
    }
}

#[derive(Debug, Deserialize)]
struct StructureCatalogFile {
    structures: Vec<StructureArchetypeDef>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code, clippy::struct_excessive_bools)]
struct StructureArchetypeDef {
    structure_type_id: u32,
    slug: String,
    /// Canonical name.
    pub name: String,
    /// Faction slug, resolved against `factions.ron`.
    pub faction: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Lore background description.
    pub lore_description: String,
    /// `StarCraft` II mechanical inspiration / analogue.
    pub sc2_inspiration: Option<String>,
    /// Footprint width and height in grid cells.
    pub footprint: (u32, u32),
    /// Mineral resource cost.
    pub mineral_cost: u32,
    /// Vespene gas resource cost.
    pub gas_cost: u32,
    /// Construction duration in ticks (at 25 Hz).
    pub build_time_ticks: u32,
    /// Maximum hull hit points.
    pub max_hp: f64,
    /// Maximum plasma shield points.
    pub max_shields: f64,
    /// Maximum energy points.
    pub max_energy: f64,
    /// Hull hit points regenerated per tick.
    #[serde(default)]
    pub hp_regen_per_tick: f64,
    /// Shield points regenerated per tick once the out-of-combat delay has elapsed.
    #[serde(default)]
    pub shield_regen_per_tick: f64,
    /// Hull life armor.
    pub life_armor: f64,
    /// Shield armor.
    pub shield_armor: f64,
    /// Line-of-sight sight reveal radius.
    pub sight_radius: f64,
    /// Supply capacity provided.
    pub supply_provided: u32,
    /// Whether this structure serves as a resource dropoff node.
    pub is_resource_dropoff: bool,
    /// Whether this structure extracts vespene gas.
    pub is_gas_extractor: bool,
    /// Whether this structure detects cloaked / burrowed units.
    pub is_detector: bool,
    /// Prerequisite structure slugs required before construction can begin.
    #[serde(default)]
    pub requires_structures: Vec<String>,
    /// Unit slugs that can be trained/constructed at this dome.
    #[serde(default)]
    pub trains_units: Vec<String>,
    /// Upgrade slugs that can be researched at this dome.
    #[serde(default)]
    pub researches_upgrades: Vec<String>,
    /// Dedicated ground defense weapon (if armed).
    #[serde(default)]
    pub ground_weapon: Option<WeaponSpecDef>,
    /// Dedicated anti-air defense weapon (if armed).
    #[serde(default)]
    pub air_weapon: Option<WeaponSpecDef>,
    /// Active tactical abilities.
    #[serde(default)]
    pub abilities: Vec<AbilitySpec>,
    /// Whether this structure is featured in the Visual Lab hero showcase.
    #[serde(default)]
    pub showcase_hero: bool,
    /// Sorting order in the hero showcase lineup (if featured).
    #[serde(default)]
    pub showcase_order: Option<u8>,
    /// Generative Veo 3.1 video prompts.
    #[serde(default)]
    pub veo_prompts: BTreeMap<String, String>,
}

impl StructureArchetypeDef {
    /// Converts a structure definition into a unified [`UnitArchetype`], resolving its faction
    /// and weapon classes against `factions`.
    pub fn into_archetype(self, factions: &FactionCatalog) -> SimResult<UnitArchetype> {
        let radius = SimScalar::from_num(f64::from(self.footprint.0) * 0.5);
        let faction = factions.resolve_faction(&self.slug, &self.faction)?;
        let convert = |weapon: Option<WeaponSpecDef>| {
            weapon
                .map(|w| UnitArchetypeDef::convert_weapon(w, factions, &self.slug))
                .transpose()
        };
        let ground_weapon = convert(self.ground_weapon)?;
        let air_weapon = convert(self.air_weapon)?;
        // `weapon` is the ground fallback of `weapon_against`, so an air weapon must never fill it.
        let weapon = ground_weapon.clone();

        let mut attributes = AttributeFlags::STRUCTURE | AttributeFlags::ARMORED;
        if self.max_shields > 0.0 {
            attributes |= AttributeFlags::PSIONIC;
        } else {
            attributes |= AttributeFlags::MECHANICAL;
        }

        let capabilities = ArchetypeCapabilities {
            overclock: None,
            harvester: None,
            is_resource_dropoff: self.is_resource_dropoff,
            is_gas_extractor: self.is_gas_extractor,
            can_construct: false,
            transport_capacity: 0,
            abilities: self.abilities.clone(),
            modes: Vec::new(),
        };

        Ok(UnitArchetype {
            unit_type_id: UnitTypeId(u16::try_from(self.structure_type_id).unwrap_or(u16::MAX)),
            name: self.slug.clone(),
            slug: self.slug,
            faction,
            display_name: self.display_name,
            lore_description: self.lore_description,
            sc2_inspiration: self.sc2_inspiration,
            showcase_hero: self.showcase_hero,
            showcase_order: self.showcase_order,
            overclock: None,
            harvester: None,
            is_resource_dropoff: self.is_resource_dropoff,
            capabilities,
            veo_prompts: self.veo_prompts,
            shape: ShapeKind::BuildingDome {
                footprint_w: u8::try_from(self.footprint.0).unwrap_or(u8::MAX),
                footprint_h: u8::try_from(self.footprint.1).unwrap_or(u8::MAX),
            },
            silhouette: SilhouetteClass::StructureBase,
            mover_type: MoverType::Ground,
            attributes,
            max_hp: SimScalar::from_num(self.max_hp),
            max_shields: SimScalar::from_num(self.max_shields),
            max_energy: SimScalar::from_num(self.max_energy),
            life_armor: SimScalar::from_num(self.life_armor),
            shield_armor: SimScalar::from_num(self.shield_armor),
            hp_regen_per_tick: SimScalar::from_num(self.hp_regen_per_tick),
            shield_regen_per_tick: SimScalar::from_num(self.shield_regen_per_tick),
            energy_regen_per_tick: SimScalar::ZERO,
            max_speed: SimScalar::ZERO,
            acceleration: SimScalar::ZERO,
            deceleration: SimScalar::ZERO,
            turn_rate: SimAngle::ZERO,
            collision_radius: radius,
            separation_radius: radius,
            push_priority: 255,
            sight_radius: SimScalar::from_num(self.sight_radius),
            scan_range: SimScalar::from_num(self.sight_radius),
            mineral_cost: self.mineral_cost,
            gas_cost: self.gas_cost,
            supply_cost: 0,
            supply_provided: self.supply_provided,
            build_time_ticks: self.build_time_ticks,
            ground_weapon,
            air_weapon,
            weapon,
            is_detector: self.is_detector,
            abilities: self.abilities,
            requires_structures: self.requires_structures,
            trains_units: self.trains_units,
            researches_upgrades: self.researches_upgrades,
            armor_class: None,
            modes: Vec::new(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_starter_roster_completeness_and_factions() {
        let catalog = crate::test_support::shipped_catalogs().units;

        // 10 archetypes registered (IDs 1 through 10)
        for id in 1..=10 {
            let arch = catalog
                .get(UnitTypeId(id))
                .expect("all starter archetypes must be registered");
            assert_ne!(arch.slug, "");
            assert_ne!(arch.display_name, "");
        }

        // Verify all archetypes in catalog have valid non-empty metadata and can be resolved
        for arch in catalog.archetypes() {
            assert_ne!(arch.slug, "");
            assert_ne!(arch.display_name, "");
            let looked_up = catalog
                .get_by_slug(&arch.slug)
                .expect("lookup by slug must succeed");
            assert_eq!(looked_up.unit_type_id, arch.unit_type_id);

            let looked_up_name = catalog
                .get_by_name(&arch.display_name)
                .expect("lookup by display name must succeed");
            assert_eq!(looked_up_name.unit_type_id, arch.unit_type_id);
        }
    }

    #[test]
    fn test_lookup_by_slug_and_display_name() {
        let catalog = crate::test_support::shipped_catalogs().units;
        let first = catalog
            .archetypes()
            .next()
            .expect("at least one archetype registered");

        let looked_up = catalog
            .get_by_slug(&first.slug)
            .expect("lookup by slug must succeed");
        assert_eq!(looked_up.unit_type_id, first.unit_type_id);

        let by_display = catalog
            .get_by_name(&first.display_name)
            .expect("lookup by display name must succeed");
        assert_eq!(by_display.unit_type_id, first.unit_type_id);

        let upper_slug = first.slug.to_uppercase();
        let case_insensitive = catalog
            .get_by_slug(&upper_slug)
            .expect("case insensitive slug must succeed");
        assert_eq!(case_insensitive.unit_type_id, first.unit_type_id);

        assert!(catalog.get_by_slug("non_existent_archetype_slug").is_none());
        assert_eq!(catalog.id_by_slug(&first.slug), Some(first.unit_type_id));
        assert_eq!(catalog.id_by_slug("unknown"), None);
    }

    #[test]
    fn test_veo_prompt_resolution() {
        let catalog = crate::test_support::shipped_catalogs().units;
        let first = catalog.archetypes().next().unwrap();
        let idle_prompt = first.veo_prompt(AnimState::Idle);
        assert!(idle_prompt.is_some());
        assert_ne!(idle_prompt.unwrap(), "");
    }
}
