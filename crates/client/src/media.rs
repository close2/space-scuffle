//! Exact-media startup registration and allocation-free request admission.
//!
//! The CPU loader admits only an identity's own files. This module publishes GPU
//! handles together with immutable admission evidence, retains unavailable entries
//! for diagnostics, and makes runtime requests fail explicitly. Existing legacy art
//! remains visible but unapproved; this status is never an authenticity certificate.

use crate::bubble_shader::FlipbookRegistry;
use asset_baker::{
    media::{load_unit_media, LoadedUnitMedia, MediaLoadError, TextureLimits},
    media_audit::{MediaAuditRecord, MediaAuditStatus},
    FlipbookAtlasMeta,
};
use bevy::{
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{
            Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
        },
    },
};
use sim_core::UnitCatalog;
use sim_types::{AnimState, UnitTypeId};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

/// Media-failure lifecycle policy shared with the native runner.
///
/// Ordinary windows retain the diagnostic until the user closes them. Automated
/// bounded runs fail immediately. Shared evidence survives Bevy runner ownership
/// transfer so closing a failed ordinary game still returns a nonzero exit.
#[derive(Resource, Clone, Default)]
pub struct MediaFailurePolicy {
    fail_fast: bool,
    failed: Arc<AtomicBool>,
}

impl MediaFailurePolicy {
    /// Creates a policy; `fail_fast` is true only for automated bounded runners.
    #[must_use]
    pub fn new(fail_fast: bool) -> Self {
        Self {
            fail_fast,
            failed: Arc::default(),
        }
    }

    /// Preserves an existing failure exit and turns a failed presentation's otherwise
    /// successful close into a nonzero exit, after orderly renderer shutdown.
    #[must_use]
    pub fn final_exit(&self, exit: AppExit) -> AppExit {
        if self.failed.load(Ordering::Acquire) && matches!(exit, AppExit::Success) {
            AppExit::error()
        } else {
            exit
        }
    }
}

/// An attempted presentation that cannot bind its own admitted media.
#[derive(Debug, thiserror::Error)]
pub enum ClientMediaError {
    /// A simulation identity was never present in the loaded catalog.
    #[error("no media admission exists for unit type {unit_type:?}")]
    UnknownIdentity {
        /// Requested runtime unit identity.
        unit_type: UnitTypeId,
    },
    /// A requested own-file pair or animation action is unavailable.
    #[error("cannot present {slug}: {source}")]
    Unavailable {
        /// Requested external catalog slug or explicit concept identity.
        slug: String,
        /// Original typed loader/action failure.
        #[source]
        source: Arc<MediaLoadError>,
    },
    /// Public registry mutation broke the relationship to validated bytes/metadata.
    #[error("media registry binding for {slug} differs from its admitted own-file pair")]
    BindingChanged {
        /// Identity whose handle or metadata was substituted.
        slug: String,
    },
    /// The smog presentation could not start or continue.
    #[error(transparent)]
    Smog(#[from] crate::smog::SmogError),
}

struct Admitted {
    metadata: FlipbookAtlasMeta,
    metadata_path: PathBuf,
    texture: Handle<Image>,
}

struct Entry {
    audit: MediaAuditRecord,
    admission: Result<Admitted, Arc<MediaLoadError>>,
}

/// Persistent availability, provenance evidence and first fatal presentation error.
///
/// Consumers must stop progression and expose `failure()` when a request fails.
/// Missing unrequested catalog media is retained for inspection, never substituted.
#[derive(Resource)]
pub struct ClientMediaStatus {
    entries: BTreeMap<UnitTypeId, Entry>,
    concept: Entry,
    failure: Option<ClientMediaError>,
    failure_text: Option<String>,
    summary: String,
    policy: MediaFailurePolicy,
}

impl ClientMediaStatus {
    /// First fatal request error, retained until this asset bundle is replaced.
    #[must_use]
    pub fn failure(&self) -> Option<&ClientMediaError> {
        self.failure.as_ref()
    }

    /// Cached first-failure message, retained across simulation resets and tab changes.
    #[must_use]
    pub fn failure_text(&self) -> Option<&str> {
        self.failure_text.as_deref()
    }

    pub(crate) fn retain_failure(&self, lab: &mut crate::LabState) -> bool {
        if let Some(message) = &self.failure_text {
            lab.battle_paused = true;
            if lab.battle_failure.is_none() {
                lab.battle_failure = Some(message.clone());
            }
            true
        } else {
            false
        }
    }

    /// Cached truthful availability/provenance summary for UI presentation.
    #[must_use]
    pub fn summary_text(&self) -> &str {
        &self.summary
    }

    /// Audit evidence for an external catalog identity, including unavailable ones.
    #[must_use]
    pub fn audit(&self, unit_type: UnitTypeId) -> Option<&MediaAuditRecord> {
        self.entries.get(&unit_type).map(|entry| &entry.audit)
    }

    /// Validates the exact registered pair and explicitly authored requested action.
    /// Performs no I/O or allocations on successful steady-state requests.
    ///
    /// # Errors
    /// Returns a typed missing identity/file/action or changed-binding failure.
    pub fn require(
        &self,
        registry: &FlipbookRegistry,
        unit_type: UnitTypeId,
        action: AnimState,
    ) -> Result<(), ClientMediaError> {
        let entry = self
            .entries
            .get(&unit_type)
            .ok_or(ClientMediaError::UnknownIdentity { unit_type })?;
        Self::require_entry(
            entry,
            registry.manifests_by_id.get(&unit_type),
            registry.textures_by_id.get(&unit_type),
            action,
        )
    }

    /// Admits the explicit concept-demo control without relabeling it as unit media.
    ///
    /// # Errors
    /// Returns the exact concept failure or an unavailable authored action.
    pub fn require_concept(
        &self,
        registry: &FlipbookRegistry,
        action: AnimState,
    ) -> Result<(), ClientMediaError> {
        Self::require_entry(
            &self.concept,
            registry.manifests.get("concept_video"),
            registry.textures.get("concept_video"),
            action,
        )
    }

    fn require_entry(
        entry: &Entry,
        metadata: Option<&FlipbookAtlasMeta>,
        texture: Option<&Handle<Image>>,
        action: AnimState,
    ) -> Result<(), ClientMediaError> {
        let admitted =
            entry
                .admission
                .as_ref()
                .map_err(|source| ClientMediaError::Unavailable {
                    slug: entry.audit.slug.clone(),
                    source: source.clone(),
                })?;
        if metadata != Some(&admitted.metadata) || texture != Some(&admitted.texture) {
            return Err(ClientMediaError::BindingChanged {
                slug: entry.audit.slug.clone(),
            });
        }
        asset_baker::media::require_action(&admitted.metadata, &admitted.metadata_path, action)
            .map_err(|source| ClientMediaError::Unavailable {
                slug: entry.audit.slug.clone(),
                source: Arc::new(source),
            })
    }

    /// Records the first failure; later systems must not hide or replace its cause.
    pub fn record_failure(&mut self, error: ClientMediaError) {
        self.policy.failed.store(true, Ordering::Release);
        if self.failure.is_none() {
            self.failure_text = Some(error.to_string());
            self.failure = Some(error);
        }
    }
}

pub(crate) fn load_registry(
    directory: &Path,
    catalog: &UnitCatalog,
    images: &mut Assets<Image>,
    limits: Option<TextureLimits>,
    policy: MediaFailurePolicy,
) -> (FlipbookRegistry, ClientMediaStatus) {
    let mut registry = FlipbookRegistry::default();
    let concept_media = load_unit_media(directory, "concept_video", limits);
    let concept_digest = concept_media
        .as_ref()
        .ok()
        .map(|media| media.png_digest.clone());
    let concept = register(
        directory,
        "concept_video",
        None,
        concept_media,
        None,
        images,
        &mut registry,
    );
    let mut entries = BTreeMap::new();
    for archetype in catalog.archetypes() {
        let media = load_unit_media(directory, &archetype.slug, limits);
        let entry = register(
            directory,
            &archetype.slug,
            Some(archetype.unit_type_id),
            media,
            concept_digest.as_deref(),
            images,
            &mut registry,
        );
        entries.insert(archetype.unit_type_id, entry);
    }
    let loaded = entries
        .values()
        .filter(|entry| entry.admission.is_ok())
        .count();
    let concept_copies = entries
        .values()
        .filter(|entry| entry.audit.status == MediaAuditStatus::KnownConceptDuplicate)
        .count();
    let summary = format!("Media unapproved: {loaded} exact legacy pairs ({concept_copies} concept copies); {} catalog entries unavailable. No verified build provenance.", entries.len() - loaded);
    info!("{summary}");
    (
        registry,
        ClientMediaStatus {
            entries,
            concept,
            failure: None,
            failure_text: None,
            summary,
            policy,
        },
    )
}

fn register(
    directory: &Path,
    slug: &str,
    unit_type: Option<UnitTypeId>,
    media: Result<LoadedUnitMedia, MediaLoadError>,
    concept_digest: Option<&str>,
    images: &mut Assets<Image>,
    registry: &mut FlipbookRegistry,
) -> Entry {
    let media = match media {
        Ok(media) => media,
        Err(error) => {
            return Entry {
                audit: MediaAuditRecord::failed(slug, &error),
                admission: Err(Arc::new(error)),
            }
        }
    };
    let audit = MediaAuditRecord::loaded(slug, &media, concept_digest);
    let mut array = Image::new(
        Extent3d {
            width: media.metadata.frame_width,
            height: media.metadata.frame_height,
            depth_or_array_layers: media.metadata.total_frames,
        },
        TextureDimension::D2,
        media.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    array.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    let texture = images.add(array);
    let portrait = images.add(Image::new(
        Extent3d {
            width: media.metadata.frame_width,
            height: media.strip_height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        media.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ));
    if let Some(id) = unit_type {
        registry.textures_by_id.insert(id, texture.clone());
        registry.manifests_by_id.insert(id, media.metadata.clone());
        registry.portrait_images_by_id.insert(id, portrait.clone());
    }
    registry.textures.insert(slug.into(), texture.clone());
    registry
        .manifests
        .insert(slug.into(), media.metadata.clone());
    registry.portrait_images.insert(slug.into(), portrait);
    Entry {
        audit,
        admission: Ok(Admitted {
            metadata: media.metadata,
            metadata_path: directory.join(format!("{slug}.flipbook.ron")),
            texture,
        }),
    }
}

/// Stops presentation after a typed media failure and preserves visible diagnostics.
/// The application error exit also makes native verification failures observable.
pub(crate) fn reject(
    error: ClientMediaError,
    status: &mut ClientMediaStatus,
    lab: &mut crate::LabState,
    exit: &mut EventWriter<'_, AppExit>,
) {
    error!("Media presentation failed: {error}");
    lab.battle_failure = Some(error.to_string());
    lab.battle_paused = true;
    status.record_failure(error);
    if status.policy.fail_fast {
        exit.send(AppExit::error());
    }
}
