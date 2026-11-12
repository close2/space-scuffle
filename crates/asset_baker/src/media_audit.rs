//! Read-only, stable-order audit of exact catalog media, its provenance and content duplication.
//!
//! # Responsibility
//! For every catalog identity, admits its own atlas pair with the exact loader and classifies
//! it: missing, malformed, a byte copy of the concept atlas, verified, or legacy (unverified,
//! with every reason). `Verified` means the provenance record (`provenance`) matches the
//! committed atlas, manifest and clips, every published action has its own frame range from its
//! own clip, no range repeats a concept frame, and every paid clip has its ledger line
//! (plan `docs/plans/2026-11-04-phase-8-5-veo-media.md`, D-6). Every shipped mode atlas
//! (`<slug>-<mode>`, plan `docs/plans/2026-11-09-effect-visualization.md` WP13) is checked the
//! same way against its record and the ledger, including that no clip it ships has a standing
//! review rejection ([`verify_mode_provenance`]).
//!
//! # How it fits
//! `asset_baker audit-media` writes the report and, with `--strict`, fails unless every
//! identity of the data-defined required set (`media/required_set.ron`) is `Verified` and every
//! shipped mode atlas verifies.
//! Identities outside the set are reported, never failed. The client builds the same records
//! without provenance checks ([`MediaAuditRecord::loaded`]) for its own diagnostics.
//!
//! # Invariants
//! - Structural availability and provenance are separate; no status claims artistic approval.
//! - Nothing is rewritten, re-baked or guessed from a file name; FFmpeg is never run.
//! - Entries are sorted by slug; every reason is kept.

use crate::ledger::NumberedRecord;
use crate::media::media_paths;
use crate::media::{load_unit_media, LoadedUnitMedia, MediaLoadError};
use crate::mode_media::ModeAtlasId;
use crate::provenance::{
    frame_hashes, provenance_path, verify_mode_provenance, verify_provenance, ModeVerifyContext,
    RequiredSet, Tier, UnverifiedReason, VerifyContext,
};
use serde::Serialize;
use sim_core::UnitCatalog;
use sim_types::AnimState;
use std::{
    collections::{BTreeMap, BTreeSet},
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
    /// Identities of the required set are not verified.
    #[error("strict media audit failed: {} required identit{} not verified: {}", identities.len(), if identities.len() == 1 { "y is" } else { "ies are" }, identities.join("; "))]
    Unverified {
        /// One `<slug> (<status>): <reasons>` line per unverified required identity.
        identities: Vec<String>,
    },
}

/// Explicit audit result; no variant claims artistic approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MediaAuditStatus {
    /// Own files match a provenance record that passed every check (see the module docs).
    Verified,
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
    /// Why a loaded identity is not `Verified`; empty when it is, or when provenance was not
    /// checked (client diagnostics, missing or malformed media, concept copies).
    pub unverified: Vec<UnverifiedReason>,
    /// The identity's tier in the required set, if the set lists it.
    pub required_tier: Option<Tier>,
}

impl MediaAuditRecord {
    /// Builds evidence from bytes admitted by the exact loader, without provenance checks;
    /// `concept_digest` is supplied only from successfully read explicit concept PNG bytes.
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
            unverified: Vec::new(),
            required_tier: None,
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
            unverified: Vec::new(),
            required_tier: None,
        }
    }

    /// Applies provenance verification: `Verified` when `reasons` is empty, otherwise legacy
    /// with the reasons. A concept copy keeps its status.
    #[must_use]
    pub fn with_verification(mut self, reasons: Vec<UnverifiedReason>) -> Self {
        if self.status == MediaAuditStatus::LegacyUnverified && reasons.is_empty() {
            self.status = MediaAuditStatus::Verified;
        }
        self.unverified = reasons;
        self
    }

    /// One line naming the identity, its status and why it is not verified.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut line = format!("{} ({:?})", self.slug, self.status);
        if !self.unverified.is_empty() {
            let reasons: Vec<String> = self.unverified.iter().map(ToString::to_string).collect();
            line.push_str(": ");
            line.push_str(&reasons.join(", "));
        }
        line
    }
}

/// Deterministic audit report over the external catalog, without generating files.
#[derive(Debug, Serialize)]
pub struct MediaAuditReport {
    /// Version of this report's machine-readable schema.
    pub format_version: u32,
    /// Digest algorithm of `png_digest` and `metadata_digest`; provenance uses SHA-256.
    pub digest_algorithm: &'static str,
    /// Whether every identity of the required set is `Verified`.
    pub strict_accepted: bool,
    /// Whether a ledger was supplied to check `Veo` origins.
    pub ledger_supplied: bool,
    /// Failure to load the explicit concept asset, retained rather than hidden.
    pub concept_failure: Option<String>,
    /// Stable slug-ordered outcomes for every catalog entry.
    pub entries: Vec<MediaAuditRecord>,
    /// Slug groups with identical admitted PNG bytes, excluding singleton groups.
    pub duplicate_groups: Vec<Vec<String>>,
    /// Every shipped mode atlas (`<slug>-<mode>`) in slug then mode order, with every reason
    /// it is not verified ([`verify_mode_provenance`]).
    pub mode_entries: Vec<ModeAuditRecord>,
}

/// The audit of one shipped mode atlas (plan `2026-11-09-effect-visualization.md` WP13).
#[derive(Debug, Clone, Serialize)]
pub struct ModeAuditRecord {
    /// The atlas name, `<slug>-<mode>`.
    pub atlas: String,
    /// Every reason it is not verified; empty when it is.
    pub unverified: Vec<UnverifiedReason>,
}

impl ModeAuditRecord {
    /// `<atlas>: <reasons>`.
    #[must_use]
    pub fn summary(&self) -> String {
        let reasons: Vec<String> = self.unverified.iter().map(ToString::to_string).collect();
        format!("{}: {}", self.atlas, reasons.join(", "))
    }
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

    /// The entries of the required set that are not `Verified`, in slug order.
    pub fn unverified_required(&self) -> impl Iterator<Item = &MediaAuditRecord> {
        self.entries.iter().filter(|entry| {
            entry.required_tier.is_some() && entry.status != MediaAuditStatus::Verified
        })
    }

    /// Requires every identity of the required set, and every shipped mode atlas, to be
    /// verified.
    ///
    /// # Errors
    /// Returns [`MediaAuditError::Unverified`] naming each one that is not, with its reasons.
    pub fn require_verified(&self) -> Result<(), MediaAuditError> {
        let identities: Vec<String> = self
            .unverified_required()
            .map(MediaAuditRecord::summary)
            .chain(
                self.mode_entries
                    .iter()
                    .filter(|entry| !entry.unverified.is_empty())
                    .map(ModeAuditRecord::summary),
            )
            .collect();
        if self.strict_accepted && identities.is_empty() {
            Ok(())
        } else {
            Err(MediaAuditError::Unverified { identities })
        }
    }
}

/// What the audit checks the bundle against.
#[derive(Debug, Clone, Copy)]
pub struct AuditInputs<'a> {
    /// The identities `--strict` requires, already checked against the catalog.
    pub required: &'a RequiredSet,
    /// The ledger, needed to verify `Veo` origins; without it they are reported unverified.
    pub ledger: Option<&'a [NumberedRecord]>,
}

/// Audits `assets_root/textures` (and the clips in `assets_root/videos`) against the catalog.
///
/// Reads actual bytes, retains per-entry errors, and never rewrites assets or invokes FFmpeg.
/// The optional concept pair is inspected explicitly; its failure is evidence, not permission
/// to substitute another asset.
#[must_use]
pub fn audit_media(
    assets_root: &Path,
    catalog: &UnitCatalog,
    inputs: &AuditInputs<'_>,
) -> MediaAuditReport {
    let textures = assets_root.join("textures");
    let videos = assets_root.join("videos");
    let concept = load_unit_media(&textures, "concept_video", None);
    let concept_failure = concept.as_ref().err().map(ToString::to_string);
    let concept_digest = concept.as_ref().ok().map(|media| media.png_digest.as_str());
    let concept_frames: BTreeSet<[u8; 32]> = concept
        .as_ref()
        .map(|media| frame_hashes(media).into_iter().collect())
        .unwrap_or_default();
    let mut entries: Vec<_> = catalog
        .archetypes()
        .map(|archetype| {
            let slug = archetype.slug.as_str();
            let mut record = match load_unit_media(&textures, slug, None) {
                Ok(media) => {
                    let published: Vec<AnimState> =
                        catalog.publishable_actions(archetype).iter().collect();
                    let context = VerifyContext {
                        textures_dir: &textures,
                        videos_dir: &videos,
                        published: &published,
                        ledger: inputs.ledger,
                        concept_frames: &concept_frames,
                    };
                    let record = MediaAuditRecord::loaded(slug, &media, concept_digest);
                    if record.status == MediaAuditStatus::KnownConceptDuplicate {
                        record
                    } else {
                        record.with_verification(verify_provenance(slug, &media, &context))
                    }
                }
                Err(error) => MediaAuditRecord::failed(slug, &error),
            };
            record.required_tier = inputs.required.tier_of(slug);
            record
        })
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
    let mode_entries = audit_mode_atlases(catalog, &textures, &videos, inputs.ledger);
    let strict_accepted = !inputs.required.identities.is_empty()
        && mode_entries.iter().all(|entry| entry.unverified.is_empty())
        && inputs.required.identities.iter().all(|identity| {
            entries
                .iter()
                .any(|e| e.slug == identity.slug && e.status == MediaAuditStatus::Verified)
        });
    MediaAuditReport {
        format_version: 2,
        digest_algorithm: "BLAKE3-256 lowercase hex",
        strict_accepted,
        ledger_supplied: inputs.ledger.is_some(),
        concept_failure,
        entries,
        duplicate_groups,
        mode_entries,
    }
}

/// The audit of every mode atlas the bundle ships: a catalog mode whose atlas or provenance
/// record exists in `textures`. A mode without media is not reported (it shows the default
/// atlas).
fn audit_mode_atlases(
    catalog: &UnitCatalog,
    textures: &Path,
    videos: &Path,
    ledger: Option<&[NumberedRecord]>,
) -> Vec<ModeAuditRecord> {
    let context = ModeVerifyContext {
        textures_dir: textures,
        videos_dir: videos,
        ledger,
    };
    let mut records: Vec<ModeAuditRecord> = catalog
        .archetypes()
        .flat_map(|archetype| {
            archetype
                .modes
                .iter()
                .map(move |mode| (archetype.slug.as_str(), mode.mode_name.as_str()))
        })
        .filter_map(|(slug, stance)| {
            let atlas = ModeAtlasId { slug, stance }.atlas_name();
            let shipped = media_paths(textures, &atlas).is_ok_and(|(png, _)| png.is_file())
                || provenance_path(textures, &atlas).is_ok_and(|path| path.is_file());
            shipped.then(|| ModeAuditRecord {
                unverified: verify_mode_provenance(slug, stance, &context),
                atlas,
            })
        })
        .collect();
    records.sort_by(|left, right| left.atlas.cmp(&right.atlas));
    records
}
