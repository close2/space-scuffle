//! Finding the asset bundle and loading its rule catalogs at run time.
//!
//! # Responsibility
//! Owns the one place the baker reads the unit catalog from: the `catalog/` directory of an
//! asset bundle chosen at run time. [`resolve_assets_root`] picks the bundle and
//! [`load_catalogs`] loads `units.ron`, `structures.ron` and `upgrades.ron` from it.
//!
//! # How it fits
//! The baker discovers archetypes, builds Veo prompts and synthesises procedural fixtures from
//! catalog data. Loading that data from disk on every run, rather than compiling it in, is what
//! lets an author add or rebalance a unit with no rebuild of any binary (charter §3.3). The
//! search follows the client's order (an explicit directory, then `SPACE_SCUFFLE_ASSETS`, then an
//! `assets` directory beside an ancestor of `CARGO_MANIFEST_DIR` or of the current directory), but
//! recognises a bundle by [`RUNTIME_BUNDLE_MARKER`], the catalog the baker needs, rather than by
//! the client's shader.
//!
//! # Invariants
//! - An explicitly named bundle is authoritative: if it is not a bundle the run fails; it never
//!   falls back to a search.
//! - Nothing is embedded: with no bundle on disk, the baker fails loudly instead of using a
//!   compiled-in roster.

use crate::procedural_generator::RUNTIME_BUNDLE_MARKER;
use crate::BakerError;
use sim_core::catalog::RuntimeCatalogs;
use std::path::{Path, PathBuf};

/// Environment variable naming the asset bundle, shared with the client.
pub const ASSETS_ENV_VAR: &str = "SPACE_SCUFFLE_ASSETS";

/// Chooses the asset bundle directory.
///
/// `explicit` (the `--assets` option) wins and must be a bundle. Otherwise
/// `SPACE_SCUFFLE_ASSETS` is used the same way. Otherwise the ancestors of the run-time
/// `CARGO_MANIFEST_DIR` (set by `cargo run`) and then of the current directory are searched, in
/// order, for an `assets` child that is a bundle.
///
/// # Errors
/// Returns [`BakerError::NotAnAssetBundle`] for an explicit or environment directory without
/// [`RUNTIME_BUNDLE_MARKER`], [`BakerError::AssetBundleNotFound`] when the search finds none, and
/// [`BakerError::CurrentDirectory`] if the current directory cannot be read.
pub fn resolve_assets_root(explicit: Option<&Path>) -> Result<PathBuf, BakerError> {
    if let Some(path) = explicit {
        return validated_bundle(path);
    }
    if let Some(path) = std::env::var_os(ASSETS_ENV_VAR) {
        return validated_bundle(Path::new(&path));
    }
    let mut roots = Vec::new();
    if let Some(path) = std::env::var_os("CARGO_MANIFEST_DIR") {
        roots.push(PathBuf::from(path));
    }
    roots.push(std::env::current_dir().map_err(|source| BakerError::CurrentDirectory { source })?);
    search_bundle(&roots)
}

/// The first `assets` child of an ancestor of a root, in order, that is a bundle.
///
/// # Errors
/// Returns [`BakerError::AssetBundleNotFound`] listing every candidate when none is a bundle.
pub fn search_bundle(roots: &[PathBuf]) -> Result<PathBuf, BakerError> {
    let mut searched = Vec::new();
    for root in roots {
        for ancestor in root.ancestors() {
            let candidate = ancestor.join("assets");
            if is_bundle(&candidate) {
                return Ok(candidate);
            }
            searched.push(candidate);
        }
    }
    Err(BakerError::AssetBundleNotFound { searched })
}

/// Loads the unit, structure and upgrade catalogs from `assets_root/catalog`.
///
/// The files are read on every call, so a later run observes every edit.
///
/// # Errors
/// Returns [`BakerError::Catalog`] naming the first file that cannot be read or is invalid.
pub fn load_catalogs(assets_root: &Path) -> Result<RuntimeCatalogs, BakerError> {
    Ok(RuntimeCatalogs::load_from_directory(
        assets_root.join("catalog"),
    )?)
}

/// `path` itself, if it is a bundle.
fn validated_bundle(path: &Path) -> Result<PathBuf, BakerError> {
    if is_bundle(path) {
        Ok(path.to_path_buf())
    } else {
        Err(BakerError::NotAnAssetBundle {
            path: path.to_path_buf(),
            marker: RUNTIME_BUNDLE_MARKER,
        })
    }
}

/// Whether `path` holds [`RUNTIME_BUNDLE_MARKER`] as a regular file.
fn is_bundle(path: &Path) -> bool {
    path.join(RUNTIME_BUNDLE_MARKER).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    fn bundle_in(parent: &Path) -> Result<PathBuf, Box<dyn Error>> {
        let assets = parent.join("assets");
        std::fs::create_dir_all(assets.join("catalog"))?;
        std::fs::write(assets.join(RUNTIME_BUNDLE_MARKER), "")?;
        Ok(assets)
    }

    #[test]
    fn an_explicit_directory_must_be_a_bundle_and_never_falls_back() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let assets = bundle_in(dir.path())?;
        assert_eq!(resolve_assets_root(Some(&assets))?, assets);
        assert!(matches!(
            resolve_assets_root(Some(dir.path())),
            Err(BakerError::NotAnAssetBundle { .. })
        ));
        Ok(())
    }

    #[test]
    fn the_search_finds_the_nearest_ancestor_bundle_in_root_order() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let outer = bundle_in(dir.path())?;
        let project = dir.path().join("project");
        let inner = bundle_in(&project)?;
        let deep = project.join("crates/tool");
        std::fs::create_dir_all(&deep)?;
        assert_eq!(search_bundle(std::slice::from_ref(&deep))?, inner);
        assert_eq!(search_bundle(&[dir.path().join("elsewhere"), deep])?, outer);
        Ok(())
    }

    #[test]
    fn a_failed_search_lists_every_candidate() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("a/b");
        match search_bundle(std::slice::from_ref(&root)) {
            Err(BakerError::AssetBundleNotFound { searched }) => {
                assert_eq!(searched.first(), Some(&root.join("assets")));
                assert_eq!(searched.len(), root.ancestors().count());
            }
            other => return Err(format!("expected AssetBundleNotFound, got {other:?}").into()),
        }
        Ok(())
    }
}
