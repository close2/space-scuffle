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

    /// A paid operation was started, but its name could not be saved for resuming.
    #[error("Veo operation '{operation}' was started but could not be recorded at '{path}': {source}. Record the name by hand, because rerunning would pay for a new generation")]
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
}
