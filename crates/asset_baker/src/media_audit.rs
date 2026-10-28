//! Read-only, stable-order audit of exact catalog media and content duplication.
//!
//! Structural availability and artistic provenance are separate. Existing own-file
//! art is preserved and marked unapproved; no historical source identity is guessed
//! from a filename. This first migration stage deliberately cannot certify legacy
//! media, and strict audit remains failing until a reviewed provenance publisher exists.

use crate::media::{load_unit_media, LoadedUnitMedia, MediaLoadError};
use serde::Serialize;
use sim_core::UnitCatalog;
use sim_types::AnimState;
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

/// Failure to publish audit evidence or satisfy strict media acceptance.
#[derive(Debug, thiserror::Error)]
pub enum MediaAuditError {
    /// Report output could not be created or fully written; existing reports are never overwritten.
    #[error("cannot write media audit {path}: {source}")]
    Io {
        /// Requested report destination.
        path: PathBuf,
        /// Underlying filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// The report could not be serialized before file creation.
    #[error("cannot serialize media audit {path}: {source}")]
    Serialization {
        /// Requested report destination.
        path: PathBuf,
        /// Serialization failure.
        #[source]
        source: serde_json::Error,
    },
    /// The bundle still contains missing or unverified media.
    #[error("strict media audit failed: installed media lacks verified provenance or required own files")]
    Unapproved,
}

/// Explicit audit result; no variant claims artistic approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MediaAuditStatus {
    /// Own files are structurally valid but lack verified build provenance.
    LegacyUnverified,
    /// Own PNG bytes exactly duplicate the explicitly named concept atlas.
    KnownConceptDuplicate,
    /// A required own-file member is absent.
    MissingAsset,
    /// The own-file pair is unreadable or fails structural admission.
    MalformedAsset,
}

/// One catalog identity's inspectable availability and provenance evidence.
#[derive(Debug, Clone, Serialize)]
pub struct MediaAuditRecord {
    /// External catalog identity; records are sorted by this field.
    pub slug: String,
    /// Availability/provenance outcome, independent of artistic judgment.
    pub status: MediaAuditStatus,
    /// Exact-path failure, when structural admission failed.
    pub failure: Option<String>,
    /// BLAKE3 digest of the encoded PNG bytes actually decoded, if admitted.
    pub png_digest: Option<String>,
    /// BLAKE3 digest of the actual parsed metadata buffer, if admitted.
    pub metadata_digest: Option<String>,
    /// Full decoded strip dimensions in pixels, if admitted.
    pub dimensions: Option<[u32; 2]>,
    /// Observed min/max alpha; opaque pixels are reported, never replaced.
    pub alpha_range: Option<[u8; 2]>,
    /// Explicitly present action slices, in enum order.
    pub actions: Vec<AnimState>,
    /// Groups of action names sharing the same frame range, for review.
    pub shared_action_ranges: Vec<Vec<AnimState>>,
}

impl MediaAuditRecord {
    /// Builds evidence from bytes admitted by the exact loader; `concept_digest`
    /// is supplied only from successfully read explicit concept PNG bytes.
    #[must_use]
    pub fn loaded(slug: &str, media: &LoadedUnitMedia, concept_digest: Option<&str>) -> Self {
        let mut ranges: BTreeMap<(u32, u32), Vec<AnimState>> = BTreeMap::new();
        for (&action, slice) in &media.metadata.actions {
            ranges
                .entry((slice.start_frame, slice.frame_count))
                .or_default()
                .push(action);
        }
        Self {
            slug: slug.into(),
            status: if concept_digest == Some(media.png_digest.as_str()) {
                MediaAuditStatus::KnownConceptDuplicate
            } else {
                MediaAuditStatus::LegacyUnverified
            },
            failure: None,
            png_digest: Some(media.png_digest.clone()),
            metadata_digest: Some(media.metadata_digest.clone()),
            dimensions: Some([media.metadata.frame_width, media.strip_height]),
            alpha_range: Some(media.alpha_range),
            actions: media.metadata.actions.keys().copied().collect(),
            shared_action_ranges: ranges
                .into_values()
                .filter(|range| range.len() > 1)
                .collect(),
        }
    }

    /// Preserves a typed loader failure as stable human-readable audit evidence.
    #[must_use]
    pub fn failed(slug: &str, error: &MediaLoadError) -> Self {
        let missing = matches!(error, MediaLoadError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound);
        Self {
            slug: slug.into(),
            status: if missing {
                MediaAuditStatus::MissingAsset
            } else {
                MediaAuditStatus::MalformedAsset
            },
            failure: Some(error.to_string()),
            png_digest: None,
            metadata_digest: None,
            dimensions: None,
            alpha_range: None,
            actions: Vec::new(),
            shared_action_ranges: Vec::new(),
        }
    }
}

/// Deterministic audit report over the external catalog, without generating files.
#[derive(Debug, Serialize)]
pub struct MediaAuditReport {
    /// Version of this report's machine-readable schema.
    pub format_version: u32,
    /// Digest algorithm used for all content identity fields.
    pub digest_algorithm: &'static str,
    /// Whether every entry has verified provenance and required media.
    /// Legacy reports remain false; hashes alone never establish approval.
    pub strict_accepted: bool,
    /// Failure to load the explicit concept asset, retained rather than hidden.
    pub concept_failure: Option<String>,
    /// Stable slug-ordered outcomes for every catalog entry.
    pub entries: Vec<MediaAuditRecord>,
    /// Slug groups with identical admitted PNG bytes, excluding singleton groups.
    pub duplicate_groups: Vec<Vec<String>>,
}

impl MediaAuditReport {
    /// Writes JSON to a new report path, preserving any existing file unchanged.
    ///
    /// # Errors
    /// Returns exact-path serialization/I/O errors, including an existing destination.
    pub fn write_json(&self, path: &Path) -> Result<(), MediaAuditError> {
        let bytes =
            serde_json::to_vec_pretty(self).map_err(|source| MediaAuditError::Serialization {
                path: path.to_owned(),
                source,
            })?;
        let io_error = |source| MediaAuditError::Io {
            path: path.to_owned(),
            source,
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(io_error)?;
        file.write_all(&bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)
    }

    /// Requires verified media/provenance; legacy structural validity is insufficient.
    ///
    /// # Errors
    /// Returns [`MediaAuditError::Unapproved`] for an unaccepted report.
    pub fn require_verified(&self) -> Result<(), MediaAuditError> {
        if self.strict_accepted {
            Ok(())
        } else {
            Err(MediaAuditError::Unapproved)
        }
    }
}

/// Audits `assets_root/textures` against the supplied external catalog.
///
/// Reads actual bytes once per pair, retains per-entry errors, and never rewrites
/// assets or invokes FFmpeg. The optional concept pair is inspected explicitly;
/// its failure is evidence, not permission to substitute another asset.
#[must_use]
pub fn audit_media(assets_root: &Path, catalog: &UnitCatalog) -> MediaAuditReport {
    let textures = assets_root.join("textures");
    let concept = load_unit_media(&textures, "concept_video", None);
    let concept_failure = concept.as_ref().err().map(ToString::to_string);
    let concept_digest = concept.as_ref().ok().map(|media| media.png_digest.as_str());
    let mut entries: Vec<_> = catalog
        .archetypes()
        .map(
            |archetype| match load_unit_media(&textures, &archetype.slug, None) {
                Ok(media) => MediaAuditRecord::loaded(&archetype.slug, &media, concept_digest),
                Err(error) => MediaAuditRecord::failed(&archetype.slug, &error),
            },
        )
        .collect();
    entries.sort_by(|left, right| left.slug.cmp(&right.slug));
    let mut duplicates: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for entry in &entries {
        if let Some(digest) = &entry.png_digest {
            duplicates
                .entry(digest)
                .or_default()
                .push(entry.slug.clone());
        }
    }
    let mut duplicate_groups: Vec<_> = duplicates
        .into_values()
        .filter(|group| group.len() > 1)
        .collect();
    duplicate_groups.sort();
    MediaAuditReport {
        format_version: 1,
        digest_algorithm: "BLAKE3-256 lowercase hex",
        strict_accepted: false,
        concept_failure,
        entries,
        duplicate_groups,
    }
}
