//! Strongly-typed error domain for the asset baker pipeline.
//!
//! This module defines [`BakerError`], the exhaustive error enumeration representing
//! all failure modes encountered during video extraction, image processing,
//! procedural synthesis, atlas packing, and metadata serialization.

use std::path::PathBuf;
use thiserror::Error;

/// Exhaustive error enumeration for the asset baking pipeline.
#[derive(Debug, Error)]
pub enum BakerError {
    /// The retired batch command cannot truthfully identify unit-specific source media.
    #[error("explicit unit-specific sources are required; generate-defaults no longer copies one concept movie or substitutes procedural art. Use bake-unit-videos with authentic clips for one explicitly named unit, and audit-media to inspect the installed bundle")]
    ExplicitSourcesRequired,
    /// An I/O error occurred while interacting with the filesystem.
    #[error("I/O error while accessing '{path}': {source}")]
    Io {
        /// Filesystem path involved in the error.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The external FFmpeg binary was not found at the configured location.
    #[error("FFmpeg executable not found at '{path}'. Ensure ffmpeg is installed.")]
    FfmpegNotFound {
        /// Expected path to the FFmpeg executable.
        path: PathBuf,
    },

    /// FFmpeg process terminated with a non-zero exit code or stderr output.
    #[error("FFmpeg execution failed (exit code: {code:?}): {stderr}")]
    FfmpegExecutionFailed {
        /// Process exit status code, if available.
        code: Option<i32>,
        /// Captured standard error output from FFmpeg.
        stderr: String,
    },

    /// An error occurred during image decoding, encoding, or pixel manipulation.
    #[error("Image processing error for '{path}': {source}")]
    ImageProcessing {
        /// Filesystem path of the image being processed.
        path: PathBuf,
        /// Underlying image library error.
        #[source]
        source: image::ImageError,
    },

    /// An error occurred while serializing RON metadata.
    #[error("Metadata serialization error: {0}")]
    RonSerError(#[from] ron::Error),

    /// An error occurred while deserializing RON metadata.
    #[error("Metadata deserialization error: {0}")]
    RonDeError(#[from] ron::error::SpannedError),

    /// Atlas metadata or procedural frame validation invariants were violated.
    #[error("Validation failed for atlas '{name}': {reason}")]
    ValidationFailed {
        /// Name of the atlas that failed validation.
        name: String,
        /// Human-readable explanation of the validation failure.
        reason: String,
    },

    /// An empty frame sequence was encountered for a specified animation state.
    #[error("Empty frame sequence received for action '{action:?}'")]
    EmptyFrames {
        /// Animation state that lacked frames.
        action: sim_types::enums::AnimState,
    },

    /// An unrecognized archetype name was supplied to the procedural generator.
    #[error("Unknown archetype '{name}'. Valid archetypes: {valid:?}")]
    UnknownArchetype {
        /// Supplied archetype name.
        name: String,
        /// List of recognized archetype names.
        valid: Vec<String>,
    },

    /// An AI keyframe source image was missing or could not be located.
    #[error("Missing AI keyframe for archetype '{archetype}' at '{path}'")]
    MissingKeyframe {
        /// Archetype name.
        archetype: String,
        /// Expected keyframe file path.
        path: PathBuf,
    },

    /// Gemini or Vertex API key could not be resolved from environment or configuration.
    #[error(
        "Gemini API key not found in GEMINI_API_KEY, GOOGLE_API_KEY, .env, or tmp/api-key.txt"
    )]
    ApiKeyMissing,

    /// Procedural output was aimed at a runtime asset bundle, which only real media may enter.
    #[error("refusing to write procedural output to '{path}': it is inside the runtime asset bundle '{bundle}'. Procedural atlases are test fixtures only")]
    RuntimePathRefused {
        /// The refused output directory.
        path: PathBuf,
        /// Root of the runtime bundle that contains it.
        bundle: PathBuf,
    },

    /// A Vertex AI key needs a Google Cloud project, and none was configured.
    #[error("a Vertex AI API key requires GOOGLE_CLOUD_PROJECT to be set, in the environment or in .env")]
    ProjectIdMissing,

    /// The API key contains control characters and cannot be sent as an HTTP header value.
    #[error("the configured API key contains control characters and was not used")]
    InvalidApiKey,

    /// A URL named a host that may not receive requests from this client.
    #[error("refusing to contact untrusted URL '{url}': {reason}")]
    UntrustedHost {
        /// The refused URL, without any query string.
        url: String,
        /// Why the URL was refused.
        reason: String,
    },

    /// A download redirect violated the redirect policy.
    #[error("refusing download redirect to '{url}': {reason}")]
    RedirectRefused {
        /// The refused redirect target, without any query string.
        url: String,
        /// Why the redirect was refused.
        reason: String,
    },

    /// An HTTP request finished with a non-success status.
    #[error("Veo request failed with HTTP {status}: {body}")]
    VeoHttpStatus {
        /// HTTP status code.
        status: u16,
        /// Start of the response body, with the API key removed.
        body: String,
    },

    /// Polling stopped after repeated transient faults. The operation itself may still finish.
    #[error("gave up polling Veo operation '{operation}' after {attempts} failed attempts: {last_error}. The operation may still be running, and a rerun resumes it")]
    VeoTransientExhausted {
        /// Resource name of the operation.
        operation: String,
        /// Number of consecutive failed attempts.
        attempts: u32,
        /// Description of the final fault.
        last_error: String,
    },

    /// The API reported that an operation ended without a usable result.
    #[error("Veo operation '{operation}' failed: {message}")]
    VeoOperationFailed {
        /// Resource name of the operation.
        operation: String,
        /// Failure reported by the API.
        message: String,
    },

    /// A paid operation was started, but its name could not be written to the ledger.
    #[error("Veo operation '{operation}' was started but could not be recorded in the ledger '{path}': {source}. Record the name by hand, because rerunning would pay for a new generation")]
    OperationNotRecorded {
        /// Resource name of the operation.
        operation: String,
        /// Path of the record that could not be written.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A file is not a complete, playable video.
    #[error("'{path}' is not a valid video: {reason}")]
    VideoValidationFailed {
        /// Path of the rejected file.
        path: PathBuf,
        /// Why the file was rejected.
        reason: String,
    },

    /// A file in the video cache is not a valid video.
    #[error("cached video '{path}' is invalid: {reason}. It was not reused and no new generation was paid for. Delete the file to regenerate it")]
    CachedVideoInvalid {
        /// Path of the rejected cache entry.
        path: PathBuf,
        /// Why the file was rejected.
        reason: String,
    },

    /// Google Veo or Vertex AI API returned an error response.
    #[error("Veo API error: {message}")]
    VeoApiError {
        /// Descriptive error message returned by the API.
        message: String,
    },

    /// A long-running prediction operation exceeded the maximum polling timeout.
    #[error("Veo long-running operation timed out: {operation}")]
    VeoTimeout {
        /// Identifier or resource name of the timed-out operation.
        operation: String,
    },

    /// An external curl process invocation failed.
    #[error("Curl execution failed (exit code: {code:?}): {stderr}")]
    CurlExecutionFailed {
        /// Process exit status code, if available.
        code: Option<i32>,
        /// Captured standard error output from curl.
        stderr: String,
    },

    /// An error occurred during JSON serialization or deserialization.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// A directory given as the asset bundle does not hold the rule catalogs.
    #[error("'{path}' is not an asset bundle: it has no '{marker}'")]
    NotAnAssetBundle {
        /// The directory that was given or searched.
        path: PathBuf,
        /// The file every asset bundle contains, relative to its root.
        marker: &'static str,
    },

    /// No asset bundle was found by the automatic search and none was given.
    #[error("no asset bundle found; pass --assets <dir> or set SPACE_SCUFFLE_ASSETS. Searched {searched:?}")]
    AssetBundleNotFound {
        /// Every candidate directory inspected, in search order.
        searched: Vec<PathBuf>,
    },

    /// The current directory, where the automatic search starts, could not be determined.
    #[error("cannot determine the current directory to search for the asset bundle: {source}")]
    CurrentDirectory {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The rule catalogs in the asset bundle could not be loaded.
    #[error(transparent)]
    Catalog(#[from] sim_core::catalog::CatalogLoadError),

    /// A data file of the asset bundle (media style, budget) is not valid.
    #[error("invalid data file '{path}': {reason}")]
    DataFile {
        /// The file.
        path: PathBuf,
        /// What is wrong.
        reason: String,
    },

    /// The framing policy of `media_style.ron` overrides an identity the catalog does not
    /// hold exactly.
    #[error("invalid data file '{path}': the framing override '{slug}' is not a catalog identity")]
    UnknownFramingOverride {
        /// The media style file.
        path: PathBuf,
        /// The unknown slug.
        slug: String,
    },

    /// A state was requested that the identity can never publish, so media for it is useless.
    #[error("'{slug}' does not publish the {state} action; it publishes {published:?}")]
    StateNotPublished {
        /// Catalog slug.
        slug: String,
        /// The requested state.
        state: String,
        /// The states it publishes.
        published: Vec<String>,
    },

    /// Prompts or keyframes of clips a run would pay for are not acceptable.
    #[error("prompt validation failed; nothing was spent:{}", crate::prompts::render_problems(.problems))]
    PromptsInvalid {
        /// Every problem found.
        problems: Vec<crate::prompts::PromptProblem>,
    },

    /// A spending limit refused the run.
    #[error(transparent)]
    Budget(#[from] crate::budget::BudgetError),

    /// The ledger could not be read or written.
    #[error(transparent)]
    Ledger(#[from] crate::ledger::LedgerError),

    /// The service refused a start request for quota. Nothing was spent; the run ended.
    #[error("Veo refused the request for quota (HTTP {http_status}, status {status:?}, reason {reason:?}); the run stopped and nothing more was started. Wait for the quota window before rerunning")]
    VeoQuotaExhausted {
        /// HTTP status of the refusal.
        http_status: u16,
        /// The error's `status` field.
        status: Option<String>,
        /// The error's reason or message.
        reason: Option<String>,
    },

    /// The run met a result that needs a person's attention and stopped after recording it.
    #[error("the Veo run stopped: {reason}")]
    VeoRunStopped {
        /// What happened.
        reason: String,
    },

    /// The run finished, but some clips failed or were skipped.
    #[error(
        "the Veo run finished with {failed} failed and {skipped} skipped clip(s); see the ledger"
    )]
    VeoRunIncomplete {
        /// Clips whose operation failed with an already-known failure kind.
        failed: u32,
        /// Clips skipped because their Idle clip was not available to chain from.
        skipped: u32,
    },

    /// The run would regenerate clips without the explicit flag.
    #[error("the run would regenerate {clips:?}, whose earlier attempts produced no usable clip; record the reason (review --reject, or the failure in the ledger) and pass --allow-regeneration")]
    RegenerationNotAllowed {
        /// `<slug> <state>` of each regeneration.
        clips: Vec<String>,
    },

    /// A clip offered for installation is not one the ledger recorded as downloaded.
    #[error("'{path}' has no `downloaded` line for {slug} {state} in the ledger; only a clip the pipeline downloaded or imported can be installed")]
    ClipNotInLedger {
        /// The clip.
        path: PathBuf,
        /// Catalog slug.
        slug: String,
        /// Animation state.
        state: String,
    },

    /// A clip offered for installation was rejected in review.
    #[error("'{path}' was rejected in review and cannot be installed")]
    ClipRejected {
        /// The clip.
        path: PathBuf,
    },

    /// An installed clip would be replaced without `--replace`.
    #[error("'{path}' is already installed; pass --replace to replace it")]
    ClipAlreadyInstalled {
        /// The installed clip.
        path: PathBuf,
    },

    /// An imported clip names an operation the ledger never recorded for that clip.
    #[error("the ledger names no operation '{operation}' for {slug} {state}; an imported clip must come from a recorded operation")]
    OperationNotInLedger {
        /// The operation given.
        operation: String,
        /// Catalog slug.
        slug: String,
        /// Animation state.
        state: String,
    },

    /// `--import` was given without the operation that produced the clip.
    #[error("--import needs --operation <name>: the recorded operation that produced the clip")]
    ImportNeedsOperation,

    /// A review action found no clip.
    #[error("{slug} {state} has no clip awaiting review")]
    NothingToReview {
        /// Catalog slug.
        slug: String,
        /// Animation state.
        state: String,
    },

    /// A command-line value is not acceptable.
    #[error("invalid argument: {reason}")]
    InvalidArgument {
        /// What is wrong.
        reason: String,
    },
}
