//! Provenance records of baked atlases, the data-defined required set, and their verification
//! (plan `docs/plans/2026-11-04-phase-8-5-veo-media.md`, decision D-6).
//!
//! # Responsibility
//! - **Format.** [`MediaProvenance`] is `assets/textures/<slug>.provenance.ron`: for each baked
//!   state, the clip under `assets/videos`, its SHA-256, its frame range in the atlas and its
//!   [`ClipOrigin`] and the bar colour of a letterboxed clip; the bake parameters, including
//!   the framing; and the SHA-256 of the atlas PNG and its manifest.
//! - **Writing.** [`bake_with_provenance`] bakes clips into an atlas and writes the record in
//!   the same step, so a baked atlas never lacks one. [`veo_origin_from_ledger`] finds the
//!   ledger line that downloaded a clip, which is how a paid clip's origin is known.
//! - **Required set.** [`RequiredSet`] is `assets/media/required_set.ron`: the identities that
//!   `audit-media --strict` requires to be verified, each with a [`Tier`]. Identities live in
//!   data only (charter §3.3).
//! - **Verification.** [`verify_provenance`] returns every [`UnverifiedReason`] that keeps an
//!   identity's installed media from being `Verified`; `media_audit` turns an empty list into
//!   that status.
//!
//! # How it fits
//! `asset_baker bake-unit-videos` calls [`bake_with_provenance`]; `asset_baker audit-media`
//! loads the required set and the ledger and calls `media_audit::audit_media`, which calls
//! [`verify_provenance`] for each identity whose atlas loaded.
//!
//! # Invariants
//! - Verification checks committed bytes against recorded hashes; it never re-bakes, so an
//!   FFmpeg upgrade cannot change a verdict (the version is recorded for the record only).
//! - A clip is named by its file name within the videos directory, never by a path, so a
//!   record cannot point outside the bundle.
//! - `LegacyVeo` is real Veo output whose request was not recorded; it says so instead of
//!   inventing an operation. `Veo` cites the ledger line that downloaded the clip.
//! - Written files end every line in `\n` on every host.
//! - Fields added after format version 1 (`framing`, `bar_rgb`) are defaulted, so every
//!   version 1 file still loads and says truthfully what it was: a centre crop without bars.

use crate::error::BakerError;
use crate::ffmpeg_importer::FrameSpec;
use crate::framing::BakeFraming;
use crate::ledger::{sha256_hex, LedgerEvent, LedgerRecord, NumberedRecord};
use crate::manifest::FlipbookAtlasMeta;
use crate::media::{media_paths, LoadedUnitMedia};
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use sim_core::UnitCatalog;
use sim_types::enums::AnimState;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Version of the provenance file format written by this module.
pub const PROVENANCE_FORMAT_VERSION: u32 = 1;

/// Version of the required-set file format read by this module.
pub const REQUIRED_SET_FORMAT_VERSION: u32 = 1;

/// The required set's path relative to the asset root.
pub const REQUIRED_SET_FILE: &str = "media/required_set.ron";

/// The FFmpeg executable the baker runs (the same one `ffmpeg_importer` uses).
const FFMPEG_PATH: &str = "/usr/bin/ffmpeg";

/// Largest strip height in pixels, as in `ffmpeg_importer`; used to record the frame budget.
const MAX_STRIP_HEIGHT: u32 = 16_384;

/// Failures to read, write or produce provenance records and the required set.
#[derive(Debug, thiserror::Error)]
pub enum ProvenanceError {
    /// A file could not be read or written.
    #[error("provenance I/O error at '{path}': {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A file is not valid RON of the expected shape.
    #[error("cannot parse '{path}': {source}")]
    Parse {
        /// The file.
        path: PathBuf,
        /// Parser error.
        #[source]
        source: ron::error::SpannedError,
    },
    /// A record could not be encoded.
    #[error("cannot encode provenance for '{name}': {source}")]
    Encode {
        /// The atlas name.
        name: String,
        /// Encoder error.
        #[source]
        source: ron::Error,
    },
    /// The slug cannot name a file.
    #[error("invalid identity {slug:?} for a provenance file")]
    InvalidSlug {
        /// The rejected slug.
        slug: String,
    },
    /// A clip to bake is not a file directly inside the videos directory, so the record could
    /// not name it.
    #[error(
        "clip '{clip}' for {state:?} is not a file directly inside the videos directory '{videos}'"
    )]
    ClipOutsideVideos {
        /// The state of the clip.
        state: AnimState,
        /// The clip.
        clip: PathBuf,
        /// The videos directory.
        videos: PathBuf,
    },
    /// Two clips were given for one state.
    #[error("two clips were given for {state:?}")]
    DuplicateState {
        /// The repeated state.
        state: AnimState,
    },
    /// The FFmpeg version could not be read.
    #[error("cannot read the FFmpeg version from '{path}': {reason}")]
    FfmpegVersion {
        /// The executable.
        path: PathBuf,
        /// What went wrong.
        reason: String,
    },
    /// Baking itself failed.
    #[error(transparent)]
    Bake(#[from] BakerError),
    /// The required set is not usable.
    #[error("required set '{source_name}' is invalid: {reason}")]
    RequiredSetInvalid {
        /// Where it came from.
        source_name: String,
        /// What is wrong with it.
        reason: String,
    },
}

/// Where one clip came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ClipOrigin {
    /// A clip generated by this pipeline, recorded in the ledger.
    Veo {
        /// Model identifier of the request.
        model: String,
        /// Operation resource name that produced the clip.
        operation: String,
        /// SHA-256 of the prompt text, lowercase hex.
        prompt_sha256: String,
        /// SHA-256 of the keyframe image sent, if any.
        keyframe_sha256: Option<String>,
        /// One-based line of the ledger's `downloaded` record of this clip.
        ledger_line: usize,
    },
    /// A clip committed by the original Veo work (plan 06), for which no operation was kept.
    LegacyVeo {
        /// Full SHA-1 of the commit that added the clip, lowercase hex.
        commit: String,
    },
}

/// One baked state: its clip, its frames and its origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateProvenance {
    /// File name of the clip inside the bundle's `videos` directory.
    pub clip: String,
    /// SHA-256 of the clip, lowercase hex.
    pub clip_sha256: String,
    /// First frame of the state in the atlas.
    pub start_frame: u32,
    /// Number of frames of the state in the atlas.
    pub frame_count: u32,
    /// Where the clip came from.
    pub origin: ClipOrigin,
    /// The bar colour drawn around this clip's frames, as sRGB bytes measured from the clip's
    /// own border; `None` for a centre crop or a clip that filled the square. Absent in
    /// records written before letterboxing existed.
    #[serde(default)]
    pub bar_rgb: Option<[u8; 3]>,
}

/// How the atlas was baked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BakeParameters {
    /// Side of each square frame, in pixels.
    pub frame_size: u32,
    /// Playback rate, in frames per second.
    pub fps: u32,
    /// Most frames each state could keep within the texture height limit.
    pub max_frames_per_state: u32,
    /// First line of `ffmpeg -version`.
    pub ffmpeg_version: String,
    /// How each clip frame was fitted into the square. Absent in records written before
    /// letterboxing existed, which were all centre crops.
    #[serde(default)]
    pub framing: BakeFraming,
}

/// The provenance record of one atlas, `<slug>.provenance.ron`. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaProvenance {
    /// [`PROVENANCE_FORMAT_VERSION`] when written.
    pub format_version: u32,
    /// Identity (atlas name).
    pub name: String,
    /// SHA-256 of the atlas PNG, lowercase hex.
    pub atlas_sha256: String,
    /// SHA-256 of the `.flipbook.ron` manifest, lowercase hex.
    pub manifest_sha256: String,
    /// How the atlas was baked.
    pub bake: BakeParameters,
    /// Every baked state.
    pub states: BTreeMap<AnimState, StateProvenance>,
}

impl MediaProvenance {
    /// Writes the record as pretty RON with `\n` line ends.
    ///
    /// # Errors
    /// [`ProvenanceError::Encode`] or [`ProvenanceError::Io`].
    pub fn save(&self, path: &Path) -> Result<(), ProvenanceError> {
        let text =
            ron::ser::to_string_pretty(self, PrettyConfig::default().new_line("\n".to_string()))
                .map_err(|source| ProvenanceError::Encode {
                    name: self.name.clone(),
                    source,
                })?;
        std::fs::write(path, text + "\n").map_err(|source| ProvenanceError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Reads a record.
    ///
    /// # Errors
    /// [`ProvenanceError::Io`] or [`ProvenanceError::Parse`].
    pub fn load(path: &Path) -> Result<Self, ProvenanceError> {
        let text = std::fs::read_to_string(path).map_err(|source| ProvenanceError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        ron::from_str(&text).map_err(|source| ProvenanceError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }
}

/// The provenance file of `slug` in `textures_dir`.
///
/// # Errors
/// [`ProvenanceError::InvalidSlug`] for a slug that cannot name a portable file.
pub fn provenance_path(textures_dir: &Path, slug: &str) -> Result<PathBuf, ProvenanceError> {
    media_paths(textures_dir, slug).map_err(|_| ProvenanceError::InvalidSlug {
        slug: slug.to_string(),
    })?;
    Ok(textures_dir.join(format!("{slug}.provenance.ron")))
}

// --- Writing ---------------------------------------------------------------------------------

/// One clip to bake, with its origin.
#[derive(Debug, Clone)]
pub struct BakeClip {
    /// The state the clip shows.
    pub state: AnimState,
    /// The clip, a file directly inside the videos directory.
    pub path: PathBuf,
    /// Where it came from.
    pub origin: ClipOrigin,
}

/// Bakes `clips` into `<output_dir>/<slug>.{png,flipbook.ron}` and writes the provenance record
/// beside them, returning it.
///
/// Every clip must be a file directly inside `videos_dir` (the bundle's `videos` directory),
/// because the record names it by file name. `spec` gives the frame side in pixels, the rate in
/// frames per second and the framing, which the record keeps with each clip's bar colour.
/// Nothing is written if a clip is refused.
///
/// # Errors
/// [`ProvenanceError::ClipOutsideVideos`], [`ProvenanceError::DuplicateState`], any baking
/// error, [`ProvenanceError::FfmpegVersion`] or an I/O error.
pub fn bake_with_provenance(
    slug: &str,
    clips: &[BakeClip],
    videos_dir: &Path,
    output_dir: &Path,
    spec: FrameSpec,
) -> Result<MediaProvenance, ProvenanceError> {
    let names = clip_names(clips, videos_dir)?;
    let ffmpeg_version = ffmpeg_version(Path::new(FFMPEG_PATH))?;
    let pairs: Vec<(AnimState, &Path)> = clips
        .iter()
        .map(|clip| (clip.state, clip.path.as_path()))
        .collect();
    let crate::FramedBake { meta, bar_rgb } =
        crate::bake_framed_action_videos(slug, &pairs, output_dir, spec)?;
    let frame_size = spec.size;
    let (png, manifest) =
        media_paths(output_dir, slug).map_err(|_| ProvenanceError::InvalidSlug {
            slug: slug.to_string(),
        })?;
    let mut states = BTreeMap::new();
    for (clip, name) in clips.iter().zip(names) {
        let slice = meta
            .actions
            .get(&clip.state)
            .ok_or_else(|| BakerError::ValidationFailed {
                name: slug.to_string(),
                reason: format!("the baked manifest has no {:?} slice", clip.state),
            })?;
        states.insert(
            clip.state,
            StateProvenance {
                clip: name,
                clip_sha256: file_sha256(&clip.path)?,
                start_frame: slice.start_frame,
                frame_count: slice.frame_count,
                origin: clip.origin.clone(),
                bar_rgb: bar_rgb.get(&clip.state).copied().flatten(),
            },
        );
    }
    let state_count = u32::try_from(clips.len()).unwrap_or(u32::MAX).max(1);
    let provenance = MediaProvenance {
        format_version: PROVENANCE_FORMAT_VERSION,
        name: slug.to_string(),
        atlas_sha256: file_sha256(&png)?,
        manifest_sha256: file_sha256(&manifest)?,
        bake: BakeParameters {
            frame_size,
            fps: spec.fps,
            max_frames_per_state: (MAX_STRIP_HEIGHT / frame_size.max(1) / state_count).max(1),
            ffmpeg_version,
            framing: spec.framing,
        },
        states,
    };
    provenance.save(&provenance_path(output_dir, slug)?)?;
    Ok(provenance)
}

/// The file name of each clip, refusing one outside `videos_dir` or a repeated state.
fn clip_names(clips: &[BakeClip], videos_dir: &Path) -> Result<Vec<String>, ProvenanceError> {
    let videos = canonical(videos_dir)?;
    let mut seen = BTreeSet::new();
    clips
        .iter()
        .map(|clip| {
            if !seen.insert(clip.state) {
                return Err(ProvenanceError::DuplicateState { state: clip.state });
            }
            let outside = || ProvenanceError::ClipOutsideVideos {
                state: clip.state,
                clip: clip.path.clone(),
                videos: videos_dir.to_path_buf(),
            };
            let path = canonical(&clip.path)?;
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(outside)?;
            if path.parent() != Some(videos.as_path()) || !path.is_file() {
                return Err(outside());
            }
            Ok(name.to_string())
        })
        .collect()
}

fn canonical(path: &Path) -> Result<PathBuf, ProvenanceError> {
    std::fs::canonicalize(path).map_err(|source| ProvenanceError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn file_sha256(path: &Path) -> Result<String, ProvenanceError> {
    let bytes = std::fs::read(path).map_err(|source| ProvenanceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(sha256_hex(&bytes))
}

/// The first line of `<ffmpeg> -version`.
///
/// # Errors
/// [`ProvenanceError::FfmpegVersion`] if it cannot be run or prints nothing.
pub fn ffmpeg_version(ffmpeg: &Path) -> Result<String, ProvenanceError> {
    let failed = |reason: String| ProvenanceError::FfmpegVersion {
        path: ffmpeg.to_path_buf(),
        reason,
    };
    let output = Command::new(ffmpeg)
        .arg("-version")
        .output()
        .map_err(|error| failed(error.to_string()))?;
    if !output.status.success() {
        return Err(failed(format!("exit status {}", output.status)));
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .ok_or_else(|| failed("it printed no version".into()))
}

/// The origin of a clip with SHA-256 `clip_sha256` that the ledger downloaded for `slug`'s
/// `state`: the last such `downloaded` line, with the model and prompt of the request whose
/// operation produced it. `None` if the ledger never downloaded these bytes for that clip.
#[must_use]
pub fn veo_origin_from_ledger(
    ledger: &[NumberedRecord],
    slug: &str,
    state: AnimState,
    clip_sha256: &str,
) -> Option<ClipOrigin> {
    let (line, operation) =
        ledger
            .iter()
            .rev()
            .find_map(|numbered| match &numbered.record.event {
                LedgerEvent::Downloaded {
                    slug: s,
                    state: st,
                    sha256,
                    operation,
                    ..
                } if s == slug && *st == state && sha256 == clip_sha256 => {
                    Some((numbered.line, operation.clone()))
                }
                _ => None,
            })?;
    let request_id = ledger
        .iter()
        .find_map(|numbered| match &numbered.record.event {
            LedgerEvent::OperationNamed {
                operation: named,
                request_id,
                ..
            } if *named == operation => Some(request_id.clone()),
            _ => None,
        })?;
    ledger
        .iter()
        .find_map(|numbered| match &numbered.record.event {
            LedgerEvent::RequestStarted {
                request_id: started,
                model,
                prompt_sha256,
                keyframe_sha256,
                ..
            } if *started == request_id => Some(ClipOrigin::Veo {
                model: model.clone(),
                operation: operation.clone(),
                prompt_sha256: prompt_sha256.clone(),
                keyframe_sha256: keyframe_sha256.clone(),
                ledger_line: line,
            }),
            _ => None,
        })
}

// --- The required set ------------------------------------------------------------------------

/// How an identity of the required set is counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tier {
    /// Needed for the Phase 8 exit: everything a player fields or builds in the skirmish.
    Required,
    /// Added by the expansion batches, required once added.
    Expansion,
}

/// One identity the strict audit requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredIdentity {
    /// Exact catalog slug.
    pub slug: String,
    /// Its tier.
    pub tier: Tier,
}

/// The identities `audit-media --strict` requires to be verified (`media/required_set.ron`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredSet {
    /// [`REQUIRED_SET_FORMAT_VERSION`].
    pub format_version: u32,
    /// The identities, each once.
    pub identities: Vec<RequiredIdentity>,
}

impl RequiredSet {
    /// A set of `identities` at the current format version, not checked against a catalog.
    #[must_use]
    pub fn new(identities: Vec<RequiredIdentity>) -> Self {
        Self {
            format_version: REQUIRED_SET_FORMAT_VERSION,
            identities,
        }
    }

    /// Reads the set at `path` and checks it against `catalog`.
    ///
    /// # Errors
    /// [`ProvenanceError::Io`], or as [`Self::from_ron_str`].
    pub fn load(path: &Path, catalog: &UnitCatalog) -> Result<Self, ProvenanceError> {
        let text = std::fs::read_to_string(path).map_err(|source| ProvenanceError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text, &path.display().to_string(), catalog)
    }

    /// Parses a set and checks it against `catalog`.
    ///
    /// # Errors
    /// [`ProvenanceError::RequiredSetInvalid`] for malformed RON, another format version, an
    /// empty list, a repeated slug or a slug the catalog does not hold exactly.
    pub fn from_ron_str(text: &str, catalog: &UnitCatalog) -> Result<Self, ProvenanceError> {
        Self::parse(text, "<string>", catalog)
    }

    fn parse(
        text: &str,
        source_name: &str,
        catalog: &UnitCatalog,
    ) -> Result<Self, ProvenanceError> {
        let invalid = |reason: String| ProvenanceError::RequiredSetInvalid {
            source_name: source_name.to_string(),
            reason,
        };
        let set: Self = ron::from_str(text).map_err(|error| invalid(error.to_string()))?;
        if set.format_version != REQUIRED_SET_FORMAT_VERSION {
            return Err(invalid(format!(
                "format_version {} is not {REQUIRED_SET_FORMAT_VERSION}",
                set.format_version
            )));
        }
        if set.identities.is_empty() {
            return Err(invalid("it lists no identity".into()));
        }
        let mut seen = BTreeSet::new();
        for identity in &set.identities {
            if !seen.insert(identity.slug.as_str()) {
                return Err(invalid(format!("'{}' is listed twice", identity.slug)));
            }
            if catalog.get_by_slug(&identity.slug).is_none() {
                return Err(invalid(format!(
                    "'{}' is not a catalog identity",
                    identity.slug
                )));
            }
        }
        Ok(set)
    }

    /// The tier of `slug`, if the set lists it.
    #[must_use]
    pub fn tier_of(&self, slug: &str) -> Option<Tier> {
        self.identities
            .iter()
            .find(|identity| identity.slug == slug)
            .map(|identity| identity.tier)
    }
}

// --- Verification ----------------------------------------------------------------------------

/// Why an identity's installed media is not `Verified`. Each names what to fix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason")]
pub enum UnverifiedReason {
    /// No `<slug>.provenance.ron` exists.
    ProvenanceMissing,
    /// The provenance file cannot be read or parsed.
    ProvenanceUnreadable {
        /// The error.
        message: String,
    },
    /// The record is for another identity or format version.
    ProvenanceMismatch {
        /// What differs.
        message: String,
    },
    /// The atlas PNG's bytes differ from the recorded hash.
    AtlasHashMismatch {
        /// Recorded SHA-256.
        recorded: String,
        /// SHA-256 of the file now.
        actual: String,
    },
    /// The manifest's bytes differ from the recorded hash.
    ManifestHashMismatch {
        /// Recorded SHA-256.
        recorded: String,
        /// SHA-256 of the file now.
        actual: String,
    },
    /// A recorded clip name is not a plain file name.
    ClipNameInvalid {
        /// The state.
        state: AnimState,
        /// The recorded name.
        clip: String,
    },
    /// A recorded clip cannot be read.
    ClipUnreadable {
        /// The state.
        state: AnimState,
        /// The recorded name.
        clip: String,
        /// The error.
        message: String,
    },
    /// A clip's bytes differ from the recorded hash.
    ClipHashMismatch {
        /// The state.
        state: AnimState,
        /// The recorded name.
        clip: String,
        /// Recorded SHA-256.
        recorded: String,
        /// SHA-256 of the file now.
        actual: String,
    },
    /// The identity publishes a state that the atlas or the record lacks.
    PublishedActionMissing {
        /// The state.
        state: AnimState,
    },
    /// The manifest lists a state the record does not account for.
    UnrecordedAction {
        /// The state.
        state: AnimState,
    },
    /// The record lists a state the manifest lacks.
    RecordedActionMissingFromAtlas {
        /// The state.
        state: AnimState,
    },
    /// A state's frame range in the manifest differs from the record.
    FrameRangeMismatch {
        /// The state.
        state: AnimState,
    },
    /// Two states' frame ranges overlap.
    SharedFrameRange {
        /// The states.
        states: [AnimState; 2],
    },
    /// Two states were baked from the same clip bytes.
    SharedClip {
        /// The states.
        states: [AnimState; 2],
    },
    /// A frame of the state's range is a frame of the concept atlas.
    ConceptFrames {
        /// The state.
        state: AnimState,
    },
    /// A `LegacyVeo` origin does not name a full commit hash.
    InvalidLegacyCommit {
        /// The state.
        state: AnimState,
        /// The recorded value.
        commit: String,
    },
    /// A `Veo` origin cannot be checked because no ledger was supplied.
    LedgerNotSupplied {
        /// The state.
        state: AnimState,
    },
    /// The cited ledger line is not the `downloaded` record of this clip.
    LedgerLineMismatch {
        /// The state.
        state: AnimState,
        /// The cited line.
        line: usize,
        /// What is wrong with it.
        message: String,
    },
    /// The ledger records a review rejection of these clip bytes.
    RejectedInReview {
        /// The state.
        state: AnimState,
    },
}

impl std::fmt::Display for UnverifiedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProvenanceMissing => write!(f, "no provenance file"),
            Self::ProvenanceUnreadable { message } => write!(f, "provenance unreadable: {message}"),
            Self::ProvenanceMismatch { message } => write!(f, "provenance mismatch: {message}"),
            Self::AtlasHashMismatch { .. } => write!(f, "atlas bytes differ from the record"),
            Self::ManifestHashMismatch { .. } => write!(f, "manifest bytes differ from the record"),
            Self::ClipNameInvalid { state, clip } => {
                write!(f, "{state:?}: clip name '{clip}' is not a file name")
            }
            Self::ClipUnreadable {
                state,
                clip,
                message,
            } => write!(f, "{state:?}: clip '{clip}' unreadable: {message}"),
            Self::ClipHashMismatch { state, clip, .. } => {
                write!(f, "{state:?}: clip '{clip}' differs from the record")
            }
            Self::PublishedActionMissing { state } => {
                write!(f, "{state:?} is published but has no baked clip")
            }
            Self::UnrecordedAction { state } => {
                write!(f, "{state:?} is in the manifest but not in the record")
            }
            Self::RecordedActionMissingFromAtlas { state } => {
                write!(f, "{state:?} is in the record but not in the manifest")
            }
            Self::FrameRangeMismatch { state } => {
                write!(f, "{state:?}: frame range differs from the record")
            }
            Self::SharedFrameRange { states } => {
                write!(f, "{:?} and {:?} share frames", states[0], states[1])
            }
            Self::SharedClip { states } => {
                write!(f, "{:?} and {:?} come from one clip", states[0], states[1])
            }
            Self::ConceptFrames { state } => {
                write!(f, "{state:?} contains frames of the concept atlas")
            }
            Self::InvalidLegacyCommit { state, commit } => {
                write!(f, "{state:?}: '{commit}' is not a full commit hash")
            }
            Self::LedgerNotSupplied { state } => {
                write!(
                    f,
                    "{state:?}: Veo origin, but no ledger was supplied (--ledger)"
                )
            }
            Self::LedgerLineMismatch {
                state,
                line,
                message,
            } => write!(f, "{state:?}: ledger line {line}: {message}"),
            Self::RejectedInReview { state } => {
                write!(f, "{state:?}: the clip was rejected in review")
            }
        }
    }
}

/// What [`verify_provenance`] checks an identity against.
#[derive(Debug, Clone, Copy)]
pub struct VerifyContext<'a> {
    /// The bundle's `textures` directory.
    pub textures_dir: &'a Path,
    /// The bundle's `videos` directory.
    pub videos_dir: &'a Path,
    /// The states the simulation can publish for the identity.
    pub published: &'a [AnimState],
    /// The ledger, if supplied.
    pub ledger: Option<&'a [NumberedRecord]>,
    /// BLAKE3 hashes of the RGBA bytes of every frame of the concept atlas.
    pub concept_frames: &'a BTreeSet<[u8; 32]>,
}

/// Every reason why `media` of `slug` is not verified; empty when it is. See the module docs.
#[must_use]
pub fn verify_provenance(
    slug: &str,
    media: &LoadedUnitMedia,
    context: &VerifyContext<'_>,
) -> Vec<UnverifiedReason> {
    let path = match provenance_path(context.textures_dir, slug) {
        Ok(path) => path,
        Err(error) => {
            return vec![UnverifiedReason::ProvenanceUnreadable {
                message: error.to_string(),
            }]
        }
    };
    if !path.is_file() {
        return vec![UnverifiedReason::ProvenanceMissing];
    }
    let provenance = match MediaProvenance::load(&path) {
        Ok(provenance) => provenance,
        Err(error) => {
            return vec![UnverifiedReason::ProvenanceUnreadable {
                message: error.to_string(),
            }]
        }
    };
    let mut reasons = Vec::new();
    check_identity(slug, &provenance, &mut reasons);
    check_files(slug, &provenance, context.textures_dir, &mut reasons);
    check_actions(
        &provenance,
        &media.metadata,
        context.published,
        &mut reasons,
    );
    check_sharing(&provenance, &mut reasons);
    check_concept_frames(&provenance, media, context.concept_frames, &mut reasons);
    for (state, record) in &provenance.states {
        check_clip(*state, record, context.videos_dir, &mut reasons);
        check_origin(slug, *state, record, context.ledger, &mut reasons);
    }
    reasons
}

fn check_identity(slug: &str, provenance: &MediaProvenance, reasons: &mut Vec<UnverifiedReason>) {
    if provenance.name != slug {
        reasons.push(UnverifiedReason::ProvenanceMismatch {
            message: format!("it is for '{}'", provenance.name),
        });
    }
    if provenance.format_version != PROVENANCE_FORMAT_VERSION {
        reasons.push(UnverifiedReason::ProvenanceMismatch {
            message: format!("format_version {}", provenance.format_version),
        });
    }
}

fn check_files(
    slug: &str,
    provenance: &MediaProvenance,
    textures_dir: &Path,
    reasons: &mut Vec<UnverifiedReason>,
) {
    let Ok((png, manifest)) = media_paths(textures_dir, slug) else {
        return;
    };
    let actual = |path: &Path| file_sha256(path).unwrap_or_else(|error| error.to_string());
    let png_actual = actual(&png);
    if png_actual != provenance.atlas_sha256 {
        reasons.push(UnverifiedReason::AtlasHashMismatch {
            recorded: provenance.atlas_sha256.clone(),
            actual: png_actual,
        });
    }
    let manifest_actual = actual(&manifest);
    if manifest_actual != provenance.manifest_sha256 {
        reasons.push(UnverifiedReason::ManifestHashMismatch {
            recorded: provenance.manifest_sha256.clone(),
            actual: manifest_actual,
        });
    }
}

fn check_actions(
    provenance: &MediaProvenance,
    meta: &FlipbookAtlasMeta,
    published: &[AnimState],
    reasons: &mut Vec<UnverifiedReason>,
) {
    for state in published {
        if !provenance.states.contains_key(state) || !meta.actions.contains_key(state) {
            reasons.push(UnverifiedReason::PublishedActionMissing { state: *state });
        }
    }
    for state in meta.actions.keys() {
        if !provenance.states.contains_key(state) {
            reasons.push(UnverifiedReason::UnrecordedAction { state: *state });
        }
    }
    for (state, record) in &provenance.states {
        match meta.actions.get(state) {
            None => {
                reasons.push(UnverifiedReason::RecordedActionMissingFromAtlas { state: *state });
            }
            Some(slice)
                if slice.start_frame != record.start_frame
                    || slice.frame_count != record.frame_count =>
            {
                reasons.push(UnverifiedReason::FrameRangeMismatch { state: *state });
            }
            Some(_) => {}
        }
    }
    let ranges: Vec<(AnimState, u32, u32)> = meta
        .actions
        .iter()
        .map(|(state, slice)| {
            let end = slice.start_frame.saturating_add(slice.frame_count);
            (*state, slice.start_frame, end)
        })
        .collect();
    for (index, (a, a_start, a_end)) in ranges.iter().enumerate() {
        for (b, b_start, b_end) in ranges.iter().skip(index + 1) {
            if a_start < b_end && b_start < a_end {
                reasons.push(UnverifiedReason::SharedFrameRange { states: [*a, *b] });
            }
        }
    }
}

fn check_sharing(provenance: &MediaProvenance, reasons: &mut Vec<UnverifiedReason>) {
    let records: Vec<(&AnimState, &StateProvenance)> = provenance.states.iter().collect();
    for (index, (a, first)) in records.iter().enumerate() {
        for (b, second) in records.iter().skip(index + 1) {
            if first.clip_sha256 == second.clip_sha256 || first.clip == second.clip {
                reasons.push(UnverifiedReason::SharedClip { states: [**a, **b] });
            }
        }
    }
}

/// BLAKE3 hash of the RGBA bytes of each frame of `media`, in strip order.
#[must_use]
pub fn frame_hashes(media: &LoadedUnitMedia) -> Vec<[u8; 32]> {
    let frame_bytes = usize::try_from(media.metadata.frame_width)
        .unwrap_or(usize::MAX)
        .saturating_mul(usize::try_from(media.metadata.frame_height).unwrap_or(usize::MAX))
        .saturating_mul(4)
        .max(1);
    media
        .rgba
        .chunks(frame_bytes)
        .map(|frame| *blake3::hash(frame).as_bytes())
        .collect()
}

fn check_concept_frames(
    provenance: &MediaProvenance,
    media: &LoadedUnitMedia,
    concept_frames: &BTreeSet<[u8; 32]>,
    reasons: &mut Vec<UnverifiedReason>,
) {
    if concept_frames.is_empty() {
        return;
    }
    let frames = frame_hashes(media);
    for (state, slice) in &media.metadata.actions {
        if !provenance.states.contains_key(state) {
            continue;
        }
        let start = usize::try_from(slice.start_frame).unwrap_or(usize::MAX);
        let count = usize::try_from(slice.frame_count).unwrap_or(0);
        if frames
            .iter()
            .skip(start)
            .take(count)
            .any(|hash| concept_frames.contains(hash))
        {
            reasons.push(UnverifiedReason::ConceptFrames { state: *state });
        }
    }
}

fn check_clip(
    state: AnimState,
    record: &StateProvenance,
    videos_dir: &Path,
    reasons: &mut Vec<UnverifiedReason>,
) {
    let mut components = Path::new(&record.clip).components();
    let plain = matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    );
    if !plain {
        reasons.push(UnverifiedReason::ClipNameInvalid {
            state,
            clip: record.clip.clone(),
        });
        return;
    }
    match file_sha256(&videos_dir.join(&record.clip)) {
        Err(error) => reasons.push(UnverifiedReason::ClipUnreadable {
            state,
            clip: record.clip.clone(),
            message: error.to_string(),
        }),
        Ok(actual) if actual != record.clip_sha256 => {
            reasons.push(UnverifiedReason::ClipHashMismatch {
                state,
                clip: record.clip.clone(),
                recorded: record.clip_sha256.clone(),
                actual,
            });
        }
        Ok(_) => {}
    }
}

fn check_origin(
    slug: &str,
    state: AnimState,
    record: &StateProvenance,
    ledger: Option<&[NumberedRecord]>,
    reasons: &mut Vec<UnverifiedReason>,
) {
    match &record.origin {
        ClipOrigin::LegacyVeo { commit } => {
            let full_hash = commit.len() == 40
                && commit
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
            if !full_hash {
                reasons.push(UnverifiedReason::InvalidLegacyCommit {
                    state,
                    commit: commit.clone(),
                });
            }
        }
        ClipOrigin::Veo {
            operation,
            ledger_line,
            ..
        } => {
            let Some(ledger) = ledger else {
                reasons.push(UnverifiedReason::LedgerNotSupplied { state });
                return;
            };
            let mismatch = |message: &str| UnverifiedReason::LedgerLineMismatch {
                state,
                line: *ledger_line,
                message: message.to_string(),
            };
            match ledger.iter().find(|numbered| numbered.line == *ledger_line) {
                None => reasons.push(mismatch("no such line")),
                Some(numbered) => {
                    if let Some(problem) =
                        downloaded_problem(&numbered.record, slug, state, operation, record)
                    {
                        reasons.push(mismatch(problem));
                    }
                }
            }
            if ledger.iter().any(|numbered| {
                matches!(&numbered.record.event, LedgerEvent::RejectedInReview {
                    slug: s, state: st, sha256, ..
                } if s == slug && *st == state && *sha256 == record.clip_sha256)
            }) {
                reasons.push(UnverifiedReason::RejectedInReview { state });
            }
        }
    }
}

/// Why `record` is not the `downloaded` line of this clip, or `None` if it is.
fn downloaded_problem(
    record: &LedgerRecord,
    slug: &str,
    state: AnimState,
    operation: &str,
    provenance: &StateProvenance,
) -> Option<&'static str> {
    let LedgerEvent::Downloaded {
        slug: s,
        state: st,
        operation: op,
        sha256,
        ..
    } = &record.event
    else {
        return Some("not a downloaded record");
    };
    if s != slug || *st != state {
        Some("it downloaded another clip")
    } else if op != operation {
        Some("it names another operation")
    } else if *sha256 != provenance.clip_sha256 {
        Some("it downloaded other bytes")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips_through_ron() -> Result<(), Box<dyn std::error::Error>> {
        let record = MediaProvenance {
            format_version: PROVENANCE_FORMAT_VERSION,
            name: "probe".into(),
            atlas_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            bake: BakeParameters {
                frame_size: 8,
                fps: 15,
                max_frames_per_state: 2,
                ffmpeg_version: "ffmpeg version probe".into(),
                framing: BakeFraming::Letterbox {
                    feather_permille: 30,
                },
            },
            states: BTreeMap::from([(
                AnimState::Idle,
                StateProvenance {
                    clip: "probe_idle.mp4".into(),
                    clip_sha256: "c".repeat(64),
                    start_frame: 0,
                    frame_count: 2,
                    origin: ClipOrigin::Veo {
                        model: "m".into(),
                        operation: "o".into(),
                        prompt_sha256: "d".repeat(64),
                        keyframe_sha256: None,
                        ledger_line: 3,
                    },
                    bar_rgb: Some([44, 48, 74]),
                },
            )]),
        };
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("probe.provenance.ron");
        record.save(&path)?;
        assert_eq!(MediaProvenance::load(&path)?, record);
        Ok(())
    }

    #[test]
    fn a_clip_name_with_a_directory_is_refused() {
        let record = StateProvenance {
            clip: "../escape.mp4".into(),
            clip_sha256: String::new(),
            start_frame: 0,
            frame_count: 1,
            origin: ClipOrigin::LegacyVeo {
                commit: String::new(),
            },
            bar_rgb: None,
        };
        let mut reasons = Vec::new();
        check_clip(AnimState::Idle, &record, Path::new("."), &mut reasons);
        assert!(matches!(
            reasons.as_slice(),
            [UnverifiedReason::ClipNameInvalid { .. }]
        ));
    }

    #[test]
    fn touching_ranges_are_not_shared_but_overlapping_ones_are() {
        let slice = |start_frame, frame_count| crate::ActionSlice {
            start_frame,
            frame_count,
            looping: true,
            hold_last_frame: false,
        };
        let meta = |walk| FlipbookAtlasMeta {
            name: "probe".into(),
            frame_width: 1,
            frame_height: 1,
            fps: 1,
            total_frames: 4,
            layout: crate::FlipbookLayout::VerticalStrip,
            actions: BTreeMap::from([(AnimState::Idle, slice(0, 2)), (AnimState::Walk, walk)]),
        };
        let provenance = MediaProvenance {
            format_version: PROVENANCE_FORMAT_VERSION,
            name: "probe".into(),
            atlas_sha256: String::new(),
            manifest_sha256: String::new(),
            bake: BakeParameters {
                frame_size: 1,
                fps: 1,
                max_frames_per_state: 2,
                ffmpeg_version: String::new(),
                framing: BakeFraming::CenterCrop,
            },
            states: BTreeMap::new(),
        };
        let shared = |walk| {
            let mut reasons = Vec::new();
            check_actions(&provenance, &meta(walk), &[], &mut reasons);
            reasons
                .iter()
                .any(|r| matches!(r, UnverifiedReason::SharedFrameRange { .. }))
        };
        assert!(!shared(slice(2, 2)));
        assert!(shared(slice(1, 2)));
    }
}
