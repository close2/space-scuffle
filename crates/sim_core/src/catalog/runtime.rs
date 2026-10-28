//! Filesystem boundary for complete external simulation rule bundles.
//!
//! Loads the three selected files in fixed order and returns a bundle only after all
//! parse and authored-data checks succeed. Errors preserve the failing file path;
//! no embedded definitions or alternate directories replace missing inputs.

use super::UnitCatalog;
use crate::error::SimCoreError;
use crate::upgrades::UpgradeCatalog;
use std::path::{Path, PathBuf};

/// Unit/structure and upgrade definitions loaded from one selected catalog directory.
#[derive(Debug, Clone)]
pub struct RuntimeCatalogs {
    /// Consolidated unit and structure definitions, indexed for runtime lookup.
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
    /// Loads `units.ron`, `structures.ron`, and `upgrades.ron` from `directory`.
    ///
    /// The directory is the catalog directory itself, typically `asset_root/catalog`.
    /// Definitions are read at call time, so restarting the same binary observes edits.
    ///
    /// # Errors
    /// Returns [`CatalogLoadError`] naming the first unreadable or invalid file.
    pub fn load_from_directory(directory: impl AsRef<Path>) -> Result<Self, CatalogLoadError> {
        let directory = directory.as_ref();
        let units_path = directory.join("units.ron");
        let mut units = UnitCatalog::from_ron_str(&read(&units_path)?).map_err(|source| {
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
        let upgrades = UpgradeCatalog::from_ron_str(&read(&upgrades_path)?).map_err(|source| {
            CatalogLoadError::Invalid {
                path: upgrades_path,
                source,
            }
        })?;
        Ok(Self { units, upgrades })
    }
}

fn read(path: &Path) -> Result<String, CatalogLoadError> {
    std::fs::read_to_string(path).map_err(|source| CatalogLoadError::Io {
        path: path.to_owned(),
        source,
    })
}
