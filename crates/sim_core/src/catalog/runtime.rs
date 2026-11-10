//! Filesystem boundary for complete external simulation rule bundles.
//!
//! Loads the seven selected files in fixed order (`factions.ron` first, because the others name
//! its factions and classes; `movement_rules.ron`, then the spell tables `statuses.ron` and
//! `ability_effects.ron` last, checked against the abilities that name them) and returns a bundle only after all parse and authored-data checks
//! succeed. Errors preserve the failing file path; no embedded definitions or alternate
//! directories replace missing inputs.

use super::{FactionCatalog, MovementRules, SpellCatalog, UnitCatalog};
use crate::error::SimCoreError;
use crate::upgrades::UpgradeCatalog;
use std::path::{Path, PathBuf};

/// Unit/structure and upgrade definitions loaded from one selected catalog directory.
#[derive(Debug, Clone)]
pub struct RuntimeCatalogs {
    /// Consolidated unit and structure definitions, indexed for runtime lookup. It also holds
    /// the faction and class identities ([`UnitCatalog::factions`]).
    pub units: UnitCatalog,
    /// Authored upgrade definitions and their deterministic lookup indexes.
    pub upgrades: UpgradeCatalog,
}

/// Failure to read or validate a specific external rule catalog.
#[derive(Debug, thiserror::Error)]
pub enum CatalogLoadError {
    /// A selected catalog file could not be read as UTF-8 text.
    #[error("cannot read catalog {path}: {source}")]
    Io {
        /// Exact selected filesystem path.
        path: PathBuf,
        /// Underlying filesystem or UTF-8 decoding failure.
        #[source]
        source: std::io::Error,
    },
    /// A selected catalog file contains invalid syntax or authored definitions.
    #[error("invalid catalog {path}: {source}")]
    Invalid {
        /// Exact selected filesystem path.
        path: PathBuf,
        /// Typed parser or authored-data failure.
        #[source]
        source: SimCoreError,
    },
}

impl RuntimeCatalogs {
    /// Loads `factions.ron`, `units.ron`, `structures.ron`, `upgrades.ron`,
    /// `movement_rules.ron`, `statuses.ron` and `ability_effects.ron` from `directory`; the
    /// movement rules and the spell catalog travel in the unit catalog
    /// ([`UnitCatalog::movement_rules`], [`UnitCatalog::spells`]).
    ///
    /// The directory is the catalog directory itself, typically `asset_root/catalog`.
    /// Definitions are read at call time, so restarting the same binary observes edits.
    ///
    /// # Errors
    /// Returns [`CatalogLoadError`] naming the first unreadable or invalid file. A unit,
    /// structure or upgrade naming an unknown faction or class is invalid in its own file.
    pub fn load_from_directory(directory: impl AsRef<Path>) -> Result<Self, CatalogLoadError> {
        let directory = directory.as_ref();
        let factions_path = directory.join("factions.ron");
        let factions = FactionCatalog::from_ron_str(&read(&factions_path)?).map_err(|source| {
            CatalogLoadError::Invalid {
                path: factions_path,
                source,
            }
        })?;
        let units_path = directory.join("units.ron");
        let mut units =
            UnitCatalog::from_ron_str(&factions, &read(&units_path)?).map_err(|source| {
                CatalogLoadError::Invalid {
                    path: units_path.clone(),
                    source,
                }
            })?;
        let unit_slugs: std::collections::BTreeSet<_> =
            units.archetypes().map(|a| a.slug.clone()).collect();
        let structures_path = directory.join("structures.ron");
        units
            .load_structures_from_ron_str(&read(&structures_path)?)
            .map_err(|source| CatalogLoadError::Invalid {
                path: structures_path.clone(),
                source,
            })?;
        units
            .validate_production_references()
            .map_err(|source| {
                let in_unit_file = matches!(&source, SimCoreError::CatalogInvalidValue { definition, .. } if unit_slugs.contains(definition));
                CatalogLoadError::Invalid { path: if in_unit_file { units_path } else { structures_path }, source }
            })?;
        let upgrades_path = directory.join("upgrades.ron");
        let upgrades =
            UpgradeCatalog::from_ron_str(&factions, &read(&upgrades_path)?).map_err(|source| {
                CatalogLoadError::Invalid {
                    path: upgrades_path.clone(),
                    source,
                }
            })?;
        units
            .validate_upgrade_references(&upgrades)
            .map_err(|source| CatalogLoadError::Invalid {
                path: upgrades_path,
                source,
            })?;
        let rules_path = directory.join("movement_rules.ron");
        let rules = MovementRules::from_ron_str(&read(&rules_path)?).map_err(|source| {
            CatalogLoadError::Invalid {
                path: rules_path,
                source,
            }
        })?;
        units.set_movement_rules(rules);
        load_spells(directory, &mut units)?;
        Ok(Self { units, upgrades })
    }
}

/// Loads the spell tables and checks the abilities naming them. A failure names
/// `statuses.ron` when it lies in a status alone, `units.ron` when an ability is at fault, and
/// `ability_effects.ron` otherwise.
fn load_spells(directory: &Path, units: &mut UnitCatalog) -> Result<(), CatalogLoadError> {
    let statuses_path = directory.join("statuses.ron");
    let effects_path = directory.join("ability_effects.ron");
    let statuses = SpellCatalog::parse_statuses(&read(&statuses_path)?).map_err(|source| {
        CatalogLoadError::Invalid {
            path: statuses_path.clone(),
            source,
        }
    })?;
    let effects = SpellCatalog::parse_effects(&read(&effects_path)?).map_err(|source| {
        CatalogLoadError::Invalid {
            path: effects_path.clone(),
            source,
        }
    })?;
    let spells = SpellCatalog::from_defs(statuses, effects).map_err(|error| {
        let path = if error.is_status_only() {
            statuses_path
        } else {
            effects_path
        };
        CatalogLoadError::Invalid {
            path,
            source: error.into(),
        }
    })?;
    units
        .set_spell_catalog(spells)
        .map_err(|source| CatalogLoadError::Invalid {
            path: directory.join("units.ron"),
            source,
        })
}

fn read(path: &Path) -> Result<String, CatalogLoadError> {
    std::fs::read_to_string(path).map_err(|source| CatalogLoadError::Io {
        path: path.to_owned(),
        source,
    })
}
