//! Checked runtime asset discovery and the shared root used by game asset consumers.
//!
//! Graphical startup resolves one canonical directory containing the real shader marker.
//! Explicit overrides never fall back. Bevy and the bubble loader share that same root;
//! legacy editor/embedding callers retain their existing discovery until separately migrated.

use bevy::prelude::Resource;
use std::path::{Path, PathBuf};
use thiserror::Error;

const ASSET_ROOT_MARKER: &str = "shaders/bubble_dome.wgsl";

/// Structural reason that a supplied directory cannot identify the game's asset root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidAssetsRoot {
    /// An explicit override supplied an empty path.
    #[error("the asset directory path is empty")]
    EmptyPath,
    /// No filesystem entry exists at the supplied directory path.
    #[error("the asset directory does not exist")]
    MissingDirectory,
    /// The supplied path is a file or another non-directory entry.
    #[error("the asset root is not a directory")]
    NotDirectory,
    /// The root lacks the required shader marker.
    #[error("required marker shaders/bubble_dome.wgsl does not exist")]
    MissingMarker,
    /// A marker entry exists but cannot identify an actual shader file.
    #[error("required marker shaders/bubble_dome.wgsl is not a regular file")]
    MarkerNotFile,
}

/// Actionable startup failure while selecting a runtime asset root.
#[derive(Debug, Error)]
pub enum AssetsDirError {
    /// A supplied root fails structural validation.
    #[error("invalid asset directory {path:?}: {reason}")]
    InvalidRoot {
        /// Directory being validated, in filesystem coordinates.
        path: PathBuf,
        /// Structural validation failure.
        reason: InvalidAssetsRoot,
    },
    /// Inspection or canonicalization failed for a relevant filesystem path.
    #[error("cannot inspect asset path {path:?}: {source}")]
    Io {
        /// Filesystem path whose operation failed.
        path: PathBuf,
        /// Original filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// Automatic discovery could not determine the current directory.
    #[error("cannot determine current directory for asset discovery: {source}")]
    CurrentDirectory {
        /// Original current-directory lookup error.
        #[source]
        source: std::io::Error,
    },
    /// None of the searched candidates contained the shader marker.
    #[error("no asset root containing shaders/bubble_dome.wgsl found; searched {searched:?}")]
    NotFound {
        /// Candidate asset directories, in discovery order.
        searched: Vec<PathBuf>,
    },
    /// Bevy's string-based asset configuration cannot represent this filesystem path.
    #[error("Bevy requires a Unicode asset directory path, but received {path:?}")]
    NonUnicodePath {
        /// Canonical path which cannot be passed to Bevy without corruption.
        path: PathBuf,
    },
}

/// Canonical validated asset root shared by Bevy and manual texture imports.
#[derive(Debug, Clone, Resource)]
pub struct ResolvedAssetsRoot(PathBuf);

impl ResolvedAssetsRoot {
    /// Resolves the root using the process environment and current directory.
    ///
    /// # Errors
    /// Returns [`AssetsDirError`] when discovery or validation fails.
    pub fn resolve() -> Result<Self, AssetsDirError> {
        try_resolve_assets_dir().map(Self)
    }

    /// Validates a specific directory and stores its canonical absolute path.
    ///
    /// # Errors
    /// Returns [`AssetsDirError`] if the root or required marker is invalid or inaccessible.
    pub fn from_directory(path: &Path) -> Result<Self, AssetsDirError> {
        validate_root(path).map(Self)
    }

    /// Returns the canonical absolute filesystem path, without re-running discovery.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Returns the exact path accepted by Bevy's string-based asset configuration.
    ///
    /// # Errors
    /// Returns [`AssetsDirError::NonUnicodePath`] instead of silently altering path bytes.
    pub fn bevy_file_path(&self) -> Result<&str, AssetsDirError> {
        self.0
            .to_str()
            .ok_or_else(|| AssetsDirError::NonUnicodePath {
                path: self.0.clone(),
            })
    }
}

/// Chooses the texture directory for the bubble loader.
///
/// A supplied checked root is authoritative. Only legacy callers without the startup
/// resource use the previous resolver, preserving editor and embedding compatibility.
#[must_use]
pub fn texture_assets_dir(root: Option<&ResolvedAssetsRoot>) -> PathBuf {
    root.map_or_else(crate::resolve_assets_dir, |root| root.0.clone())
        .join("textures")
}

/// Resolves an absolute asset root from the process environment.
///
/// `SPACE_SCUFFLE_ASSETS`, when present, must validate exactly. Otherwise searches
/// ancestors of runtime `CARGO_MANIFEST_DIR`, then ancestors of the current directory.
///
/// # Errors
/// Returns [`AssetsDirError`] for invalid overrides, filesystem errors, or no valid root.
pub fn try_resolve_assets_dir() -> Result<PathBuf, AssetsDirError> {
    if let Some(path) = std::env::var_os("SPACE_SCUFFLE_ASSETS") {
        return discover_assets_dir(Some(Path::new(&path)), &[]);
    }
    let mut roots = Vec::new();
    if let Some(path) = std::env::var_os("CARGO_MANIFEST_DIR") {
        roots.push(PathBuf::from(path));
    }
    roots.push(
        std::env::current_dir().map_err(|source| AssetsDirError::CurrentDirectory { source })?,
    );
    discover_assets_dir(None, &roots)
}

/// Resolves an asset directory from explicit inputs without changing process-global state.
///
/// `explicit_override` is authoritative when supplied. Otherwise each search root and
/// its ancestors are inspected in order for an `assets` child with the required marker.
/// Paths are filesystem paths; successful results are canonical and absolute.
///
/// # Errors
/// Returns [`AssetsDirError`] for invalid overrides, unexpected I/O errors, or no match.
pub fn discover_assets_dir(
    explicit_override: Option<&Path>,
    search_roots: &[PathBuf],
) -> Result<PathBuf, AssetsDirError> {
    if let Some(path) = explicit_override {
        return validate_root(path);
    }
    let mut searched = Vec::new();
    for root in search_roots {
        for ancestor in root.ancestors() {
            let candidate = ancestor.join("assets");
            match validate_root(&candidate) {
                Ok(path) => return Ok(path),
                Err(AssetsDirError::InvalidRoot { .. }) => searched.push(candidate),
                Err(error) => return Err(error),
            }
        }
    }
    Err(AssetsDirError::NotFound { searched })
}

fn validate_root(path: &Path) -> Result<PathBuf, AssetsDirError> {
    let invalid = |reason| AssetsDirError::InvalidRoot {
        path: path.to_path_buf(),
        reason,
    };
    if path.as_os_str().is_empty() {
        return Err(invalid(InvalidAssetsRoot::EmptyPath));
    }
    let root_metadata = std::fs::metadata(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            invalid(InvalidAssetsRoot::MissingDirectory)
        } else {
            AssetsDirError::Io {
                path: path.to_path_buf(),
                source,
            }
        }
    })?;
    if !root_metadata.is_dir() {
        return Err(invalid(InvalidAssetsRoot::NotDirectory));
    }
    let marker = path.join(ASSET_ROOT_MARKER);
    let marker_metadata = std::fs::metadata(&marker).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            invalid(InvalidAssetsRoot::MissingMarker)
        } else {
            AssetsDirError::Io {
                path: marker.clone(),
                source,
            }
        }
    })?;
    if !marker_metadata.is_file() {
        return Err(invalid(InvalidAssetsRoot::MarkerNotFile));
    }
    std::fs::canonicalize(path).map_err(|source| AssetsDirError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_explicit_override_never_uses_discovery() {
        assert!(matches!(
            discover_assets_dir(Some(Path::new("")), &[PathBuf::from(".")]),
            Err(AssetsDirError::InvalidRoot {
                reason: InvalidAssetsRoot::EmptyPath,
                ..
            })
        ));
    }

    #[test]
    fn empty_search_list_reports_no_candidates() {
        assert!(matches!(
            discover_assets_dir(None, &[]),
            Err(AssetsDirError::NotFound { searched }) if searched.is_empty()
        ));
    }
}
