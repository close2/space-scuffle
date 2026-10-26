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
