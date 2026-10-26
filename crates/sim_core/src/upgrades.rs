//! # Authoritative Upgrade Catalog & In-Memory Upgrade Registry
//!
//! Provides fast O(1) indexed lookup for all weapon/armor scaling upgrades and unit-specific tech upgrades.
//!
//! ## Invariants Maintained
//! 1. All upgrades are strictly data-driven loaded from `assets/catalog/upgrades.ron`.
//! 2. Zero hardcoded upgrade slugs in engine logic.
//! 3. Deterministic upgrade IDs and effects.

use crate::error::{SimCoreError, SimResult};
use serde::{Deserialize, Serialize};
use sim_types::{Faction, UpgradeEffect};
use std::collections::BTreeMap;
use std::path::Path;

/// Unique upgrade identifier type.
pub type UpgradeId = u32;

/// Deserializable format for assets/catalog/upgrades.ron.
#[derive(Debug, Deserialize)]
pub struct UpgradeCatalogFile {
    /// List of declared upgrade definitions.
    pub upgrades: Vec<UpgradeDef>,
}

/// Declarative specification for an upgrade in Space Scuffle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeDef {
    /// Unique integer identifier.
    pub upgrade_id: UpgradeId,
    /// Canonical internal slug name.
    pub slug: String,
    /// Canonical internal name.
    pub name: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Owning faction classification.
    pub faction: Faction,
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
    #[serde(default)]
    pub requires_structures: Vec<String>,
    /// Immediate mechanical effects applied to player state upon completion.
    pub effects: Vec<UpgradeEffect>,
}

/// Read-only indexed catalog of all registered game upgrades.
#[derive(Debug, Clone, Default)]
pub struct UpgradeCatalog {
    upgrades_by_id: Vec<Option<UpgradeDef>>,
    upgrades_by_slug: BTreeMap<String, UpgradeId>,
}

impl UpgradeCatalog {
    /// Deserializes an upgrade catalog from raw RON text.
    ///
    /// # Errors
    /// Returns [`SimCoreError::RonDeserializationError`] if parsing fails.
    pub fn from_ron_str(ron_data: &str) -> SimResult<Self> {
        let catalog_file: UpgradeCatalogFile = ron::from_str(ron_data)
            .map_err(|e| SimCoreError::RonDeserializationError(e.to_string()))?;

        let mut catalog = Self {
            upgrades_by_id: vec![None; 128],
            upgrades_by_slug: BTreeMap::new(),
        };

        for def in catalog_file.upgrades {
            let id = def.upgrade_id;
            catalog.upgrades_by_slug.insert(def.slug.clone(), id);
            if id as usize >= catalog.upgrades_by_id.len() {
                catalog.upgrades_by_id.resize(id as usize + 32, None);
            }
            catalog.upgrades_by_id[id as usize] = Some(def);
        }

        Ok(catalog)
    }

    /// Loads an upgrade catalog from an on-disk RON file.
    ///
    /// # Errors
    /// Returns [`SimCoreError::IoError`] or [`SimCoreError::RonDeserializationError`].
    pub fn load_from_file(path: impl AsRef<Path>) -> SimResult<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| SimCoreError::IoError(e.to_string()))?;
        Self::from_ron_str(&content)
    }

    /// Creates the default upgrade catalog using the compile-time embedded `assets/catalog/upgrades.ron`.
    ///
    /// # Panics
    /// Panics if the compile-time embedded RON contains syntax errors.
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn default_catalog() -> Self {
        const EMBEDDED_UPGRADES: &str = include_str!("../../../assets/catalog/upgrades.ron");
        Self::from_ron_str(EMBEDDED_UPGRADES)
            .expect("invariant: embedded assets/catalog/upgrades.ron must be syntactically valid")
    }

    /// Looks up an upgrade by its integer [`UpgradeId`].
    #[must_use]
    pub fn get_by_id(&self, id: UpgradeId) -> Option<&UpgradeDef> {
        self.upgrades_by_id
            .get(id as usize)
            .and_then(Option::as_ref)
    }

    /// Looks up an upgrade by its canonical slug string.
    #[must_use]
    pub fn get_by_slug(&self, slug: &str) -> Option<&UpgradeDef> {
        self.upgrades_by_slug
            .get(slug)
            .and_then(|&id| self.get_by_id(id))
    }

    /// Iterates over all registered upgrade definitions.
    pub fn upgrades(&self) -> impl Iterator<Item = &UpgradeDef> {
        self.upgrades_by_id.iter().flatten()
    }
}
