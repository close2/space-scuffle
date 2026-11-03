//! Strongly-typed Google Gemini and Vertex AI Veo Video Generation client.
//!
//! # Architectural Charter
//! In accordance with `AGENTS.md` and
//! `docs/06-factions-and-video-style-guide.md`,
//! this module provides an automated client for generating authentic 3D unit animation
//! clips (`Idle`, `Walk`, `Attack`) using Google's Veo video generation models.
//!
//! # Dual-API Support (Gemini API & Vertex AI Express Mode)
//! - **Vertex AI Express Mode (`AQ.` prefix):** Uses the Vertex AI endpoints with API key
//!   authentication (`projects/{project}/locations/us-central1/publishers/google/models/{model}`).
//!   Polls via the `fetchPredictOperation` RPC with `{"operationName": "{operation_name}"}`.
//! - **Gemini Developer API (`AIza` prefix):** Uses `generativelanguage.googleapis.com/v1beta`
//!   and polls GET `v1beta/{operation_name}`.
//!
//! # Credential Handling Invariants
//! The API key is a paid credential, so this module maintains the following:
//! - It never appears in a process argument list or a URL. Process arguments are visible to
//!   other local users. The key travels only as an `x-goog-api-key` header, inside a curl
//!   configuration written to curl's standard input (`curl --config -`).
//! - It is sent only over HTTPS and only to [`TRUSTED_API_HOSTS`].
//! - curl never follows redirects itself. Each redirect hop is validated here and fetched
//!   without the key, even if a hop returns to a trusted host.
//! - `Debug` output redacts it, and error text that came from the network is scrubbed of it.
//!
//! # Paid Work Is Never Lost or Repeated
//! Each generation costs real money, so failures are handled to protect it:
//! - A video reaches its destination only by an atomic rename, after `ffprobe` has read every
//!   packet of it. An HTTP error page or a truncated download therefore cannot enter the
//!   cache, and the cache is validated the same way before it is reused.
//! - An operation name is recorded in `<unit>_<state>.operation.json` before polling starts,
//!   and a rerun resumes it instead of paying again. The record is removed only once the clip
//!   is installed or the API reports that the operation failed.
//! - Every request has a timeout. Polling and downloads retry transient faults a bounded
//!   number of times. `:predictLongRunning` is retried only if the request cannot have started
//!   work, because any other retry could pay twice.
//!
//! # WebGPU Texture Dimensions & Invariants
//! Generated `.mp4` video clips are downloaded into `assets/videos/<unit>_<state>.mp4`
//! and then transcoded via [`crate::ffmpeg_importer::bake_multi_action_videos_with_states`]
//! into vertical strip PNGs where total height is guaranteed:
//! `strip_height = total_frames * frame_size <= 16384` px.

#![allow(
    clippy::cast_possible_truncation,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use crate::error::BakerError;
use crate::ffmpeg_importer::{
    bake_multi_action_videos_with_states, probe_video, DEFAULT_FFPROBE_PATH,
};
use crate::manifest::FlipbookAtlasMeta;
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use sim_types::enums::AnimState;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Default Veo model identifier for video generation.
pub const DEFAULT_VEO_MODEL: &str = "veo-3.1-fast-generate-001";

/// Prefix that identifies a Vertex AI Express Mode key.
const VERTEX_KEY_PREFIX: &str = "AQ.";

/// Default polling interval in seconds for asynchronous video operations.
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;

/// Default maximum polling duration in seconds (5 minutes).
pub const DEFAULT_POLL_TIMEOUT_SECS: u64 = 300;

/// The only hosts that may receive the API key. A download URI on any other host is refused.
pub const TRUSTED_API_HOSTS: [&str; 2] = [
    "generativelanguage.googleapis.com",
    "us-central1-aiplatform.googleapis.com",
];

/// Default number of consecutive transient faults retried before giving up.
pub const DEFAULT_MAX_RETRIES: u32 = 5;

/// Default delay before the first retry, in seconds.
pub const DEFAULT_RETRY_BASE_DELAY_SECS: u64 = 2;

/// Upper bound on the delay between retries.
pub const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

/// Maximum number of redirects followed for one download.
pub const MAX_REDIRECT_HOPS: usize = 5;

/// Time limit for establishing a connection, in seconds.
const CONNECT_TIMEOUT_SECS: u64 = 30;

/// Time limit for one JSON API request, which may upload a keyframe image.
const API_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Time limit for one video download hop.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

/// curl exit code for an HTTP status of 400 or above under `--fail-with-body`.
const CURL_EXIT_HTTP_ERROR: i32 = 22;

/// Number of response-body bytes kept in an error message.
const ERROR_BODY_LIMIT: usize = 2048;

/// Replacement text for the API key in diagnostics.
const REDACTED: &str = "[REDACTED]";

/// curl `--write-out` format: the HTTP status, then the redirect target if there is one.
const CURL_WRITE_OUT: &str = "%{http_code}\n%{redirect_url}\n";

/// Represents an image input payload formatted for Google's video generation models.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VeoImageInput {
    /// Base64-encoded raw image byte content.
    #[serde(rename = "bytesBase64Encoded")]
    pub bytes_base64_encoded: String,
    /// MIME type of the image (e.g. `"image/jpeg"` or `"image/png"`).
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

/// An individual generation instance passed to `:predictLongRunning`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VeoInstance {
    /// Text prompt describing character motion, visual aesthetics, and lighting.
    pub prompt: String,
    /// Optional reference keyframe image for Image-to-Video generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<VeoImageInput>,
}

/// Configuration parameters for Veo generation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VeoParameters {
    /// Aspect ratio of generated video (typically `"1:1"` or `"16:9"`).
    #[serde(rename = "aspectRatio", skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    /// Duration of video in seconds (typically 5 or 8).
    #[serde(rename = "durationSeconds", skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u32>,
    /// Number of candidate videos to sample (typically 1).
    #[serde(rename = "sampleCount", skip_serializing_if = "Option::is_none")]
    pub sample_count: Option<u32>,
}

impl Default for VeoParameters {
    fn default() -> Self {
        Self {
            aspect_ratio: Some("16:9".to_string()),
            duration_seconds: Some(4),
            sample_count: Some(1),
        }
    }
}

/// Outgoing request payload sent to the `:predictLongRunning` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VeoGenerateRequest {
    /// List of generation instances (usually a single element).
    pub instances: Vec<VeoInstance>,
    /// Video generation parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<VeoParameters>,
}

/// Structured response payload for Veo video generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VeoGenerateVideoResponse {
    /// Generated video assets.
    #[serde(default)]
    pub videos: Vec<VeoVideoOutput>,
    /// Number of media samples filtered due to Responsible AI (RAI) safety criteria.
    #[serde(rename = "raiMediaFilteredCount", default)]
    pub rai_media_filtered_count: Option<u32>,
}

/// An individual video output record returned by Veo models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VeoVideoOutput {
    /// Base64 encoded MP4 video bytes.
    #[serde(rename = "bytesBase64Encoded")]
    pub bytes_base64_encoded: Option<String>,
    /// Video download URI.
    pub uri: Option<String>,
    /// Google Cloud Storage URI.
    #[serde(rename = "gcsUri")]
    pub gcs_uri: Option<String>,
    /// MIME type of the video file (typically `"video/mp4"`).
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// Operation status returned by `:predictLongRunning` or during polling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VeoOperation {
    /// Resource name of the long-running operation.
    #[serde(default)]
    pub name: String,
    /// Whether the operation has finished execution.
    #[serde(default)]
    pub done: bool,
    /// Completed response payload containing video metadata or base64 bytes.
    #[serde(default)]
    pub response: Option<serde_json::Value>,
    /// Error information if the operation or request failed.
    #[serde(default)]
    pub error: Option<VeoApiErrorDetail>,
}

/// API error details payload returned by Google Generative Language or Vertex AI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VeoApiErrorDetail {
    /// HTTP status code or RPC code.
    #[serde(default)]
    pub code: Option<i64>,
    /// Descriptive error message.
    #[serde(default)]
    pub message: String,
    /// Error status enum string (e.g. `"PERMISSION_DENIED"`).
    #[serde(default)]
    pub status: Option<String>,
}

/// Resolved API credentials and configuration for Google Veo video generation.
///
/// `Debug` is implemented by hand so that the key cannot reach logs.
#[derive(Clone)]
pub struct VeoCredentials {
    /// Secret API key token.
    pub api_key: String,
    /// Google Cloud project ID or project number. Vertex AI requires it, the Gemini API does not.
    pub project_id: Option<String>,
}

impl std::fmt::Debug for VeoCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VeoCredentials")
            .field("api_key", &REDACTED)
            .field("project_id", &self.project_id)
            .finish()
    }
}

/// One curl invocation.
struct CurlRequest<'a> {
    /// Target URL, which never contains the key.
    url: &'a str,
    /// File holding a JSON body to POST. `None` issues a GET.
    post_body: Option<&'a Path>,
    /// Whether to send the API key. Only ever true for a [`TRUSTED_API_HOSTS`] URL.
    authenticated: bool,
    /// File that receives the response body.
    output: &'a Path,
    /// Limit for the whole transfer.
    max_time: Duration,
}

/// The parts of a completed curl invocation that callers act on.
struct CurlResponse {
    /// HTTP status code.
    status: u16,
    /// Target of a redirect response, or empty.
    redirect_url: String,
}

/// A generation that was paid for and has not yet been installed as a clip.
///
/// Stored as `<unit>_<state>.operation.json` beside the clip. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingOperation {
    /// Resource name of the long-running operation.
    pub operation_name: String,
    /// Model that the operation was started with, which Vertex AI polling needs.
    pub model: String,
}

/// Whether a retry might succeed, assuming that repeating the request is harmless.
fn is_transient(error: &BakerError) -> bool {
    match error {
        BakerError::VeoHttpStatus { status, .. } => {
            matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
        }
        // Proxy and host lookup, connection refused, partial transfer, timeout, TLS handshake,
        // empty reply, send failure and receive failure.
        BakerError::CurlExecutionFailed { code, .. } => {
            matches!(code, Some(5..=7 | 18 | 28 | 35 | 52 | 55 | 56))
        }
        _ => false,
    }
}

/// Whether a failed generation request certainly did not start paid work.
fn request_started_no_work(error: &BakerError) -> bool {
    match error {
        BakerError::VeoHttpStatus { status, .. } => *status == 429,
        // Host lookup and connection failures happen before any request bytes are sent.
        BakerError::CurlExecutionFailed { code, .. } => matches!(code, Some(5..=7)),
        _ => false,
    }
}

/// File name stem for an animation state.
fn state_slug(state: AnimState) -> &'static str {
    match state {
        AnimState::Idle => "idle",
        AnimState::Walk => "walk",
        AnimState::Attack => "attack",
        AnimState::Constructing => "constructing",
        AnimState::Cast => "cast",
        AnimState::Die => "die",
    }
}

/// Reads an operation record. An unreadable record is an error rather than `None`, because
/// `None` means "start a new paid generation".
fn load_pending(path: &Path) -> Result<Option<PendingOperation>, BakerError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(BakerError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Atomically writes an operation record.
fn save_pending(path: &Path, record: &PendingOperation) -> Result<(), BakerError> {
    let not_recorded = |source| BakerError::OperationNotRecorded {
        operation: record.operation_name.clone(),
        path: path.to_path_buf(),
        source,
    };
    let parent = path.parent().unwrap_or(Path::new("."));
    let file = tempfile::NamedTempFile::new_in(parent).map_err(not_recorded)?;
    let json = serde_json::to_vec_pretty(record)?;
    std::fs::write(file.path(), json).map_err(not_recorded)?;
    file.persist(path)
        .map_err(|error| not_recorded(error.error))?;
    Ok(())
}

/// Removes an operation record that may already be absent.
fn clear_pending(path: &Path) -> Result<(), BakerError> {
    match std::fs::remove_file(path) {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => Err(BakerError::Io {
            path: path.to_path_buf(),
            source,
        }),
        Ok(()) | Err(_) => Ok(()),
    }
}

/// Returns the host of an `https://` URL, or `None` for any other scheme.
///
/// `None` is also returned for an explicit port or userinfo. Both are unnecessary for Google's
/// endpoints, and userinfo is the classic way to disguise the true host of a URL.
fn https_host(url: &str) -> Option<&str> {
    let authority = url
        .strip_prefix("https://")?
        .split(['/', '?', '#'])
        .next()?;
    (!authority.is_empty() && !authority.contains(['@', ':'])).then_some(authority)
}

/// Removes the query and fragment, which may hold signed tokens, for use in diagnostics.
fn without_query(url: &str) -> &str {
    url.split(['?', '#']).next().unwrap_or(url)
}

/// Accepts `url` only if it is HTTPS on one of the [`TRUSTED_API_HOSTS`].
fn ensure_trusted_host(url: &str) -> Result<(), BakerError> {
    let trusted = https_host(url).is_some_and(|host| {
        TRUSTED_API_HOSTS
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
    });
    if trusted {
        Ok(())
    } else {
        Err(BakerError::UntrustedHost {
            url: without_query(url).to_string(),
            reason: format!("only HTTPS on {TRUSTED_API_HOSTS:?} is allowed"),
        })
    }
}

/// Strongly-typed client for Google Veo Video Generation.
#[derive(Debug, Clone)]
pub struct VeoClient {
    credentials: VeoCredentials,
    model: String,
    curl_path: PathBuf,
    ffprobe_path: PathBuf,
    poll_interval: Duration,
    poll_timeout: Duration,
    max_retries: u32,
    retry_base_delay: Duration,
}

impl VeoClient {
    /// Creates a new `VeoClient` with default configuration and auto-discovered credentials.
    ///
    /// # Errors
    /// Returns [`BakerError::ApiKeyMissing`] if no key can be located.
    pub fn new_auto() -> Result<Self, BakerError> {
        let creds = resolve_credentials()?;
        Ok(Self::with_credentials(creds))
    }

    /// Creates a new `VeoClient` using explicit credentials and default options.
    #[must_use]
    pub fn with_credentials(credentials: VeoCredentials) -> Self {
        Self {
            credentials,
            model: DEFAULT_VEO_MODEL.to_string(),
            curl_path: PathBuf::from("/usr/bin/curl"),
            ffprobe_path: PathBuf::from(DEFAULT_FFPROBE_PATH),
            poll_interval: Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS),
            poll_timeout: Duration::from_secs(DEFAULT_POLL_TIMEOUT_SECS),
            max_retries: DEFAULT_MAX_RETRIES,
            retry_base_delay: Duration::from_secs(DEFAULT_RETRY_BASE_DELAY_SECS),
        }
    }

    /// Overrides the Veo model identifier (e.g. `"veo-3.0-generate-preview"`).
    #[must_use]
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// Overrides the polling interval.
    #[must_use]
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Overrides the maximum polling timeout.
    #[must_use]
    pub fn with_poll_timeout(mut self, timeout: Duration) -> Self {
        self.poll_timeout = timeout;
        self
    }

    /// Overrides how transient faults are retried: at most `max_retries` consecutive retries,
    /// waiting `base_delay` and then doubling it, up to [`MAX_RETRY_DELAY`].
    #[must_use]
    pub fn with_retry_policy(mut self, max_retries: u32, base_delay: Duration) -> Self {
        self.max_retries = max_retries;
        self.retry_base_delay = base_delay;
        self
    }

    /// Overrides the curl executable, by default `/usr/bin/curl`.
    #[must_use]
    pub fn with_curl_path(mut self, curl_path: &Path) -> Self {
        self.curl_path = curl_path.to_path_buf();
        self
    }

    /// Accessor for the configured model name.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Checks if the API key corresponds to a Vertex AI Express Mode key (`AQ.` prefix).
    #[must_use]
    pub fn is_vertex_ai(&self) -> bool {
        self.credentials.api_key.starts_with(VERTEX_KEY_PREFIX)
    }

    /// Replaces every occurrence of the API key in text that came from the network or curl.
    fn redact(&self, text: &str) -> String {
        if self.credentials.api_key.is_empty() {
            text.to_string()
        } else {
            text.replace(&self.credentials.api_key, REDACTED)
        }
    }

    /// Renders the curl configuration that carries the key, which is written to curl's stdin.
    fn curl_auth_config(&self) -> Result<String, BakerError> {
        let key = &self.credentials.api_key;
        // A newline would end the quoted value and start a new curl option.
        if key.is_empty() || key.chars().any(char::is_control) {
            return Err(BakerError::InvalidApiKey);
        }
        let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
        Ok(format!("header = \"x-goog-api-key: {escaped}\"\n"))
    }

    /// Runs curl once and reports the HTTP status.
    ///
    /// `-q` must stay first: it stops a user `.curlrc` from re-enabling redirects. `--proto`
    /// restricts the transfer to HTTPS. `-L` is deliberately absent, see the module docs.
    fn run_curl(&self, request: &CurlRequest<'_>) -> Result<CurlResponse, BakerError> {
        let io_error = |source| BakerError::Io {
            path: self.curl_path.clone(),
            source,
        };
        let auth_config = if request.authenticated {
            Some(self.curl_auth_config()?)
        } else {
            None
        };

        let mut command = Command::new(&self.curl_path);
        command
            .arg("-q")
            .arg("--silent")
            .arg("--show-error")
            .arg("--fail-with-body")
            .arg("--proto")
            .arg("=https")
            .arg("--connect-timeout")
            .arg(CONNECT_TIMEOUT_SECS.to_string())
            .arg("--max-time")
            .arg(format!("{:.3}", request.max_time.as_secs_f64()))
            .arg("--output")
            .arg(request.output)
            .arg("--write-out")
            .arg(CURL_WRITE_OUT);
        if auth_config.is_some() {
            command.arg("--config").arg("-");
        }
        if let Some(body) = request.post_body {
            let mut data_argument = std::ffi::OsString::from("@");
            data_argument.push(body);
            command
                .arg("--request")
                .arg("POST")
                .arg("--header")
                .arg("Content-Type: application/json")
                .arg("--data-binary")
                .arg(data_argument);
        }
        command
            .arg(request.url)
            .stdin(if auth_config.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(io_error)?;
        if let (Some(config), Some(mut stdin)) = (auth_config, child.stdin.take()) {
            use std::io::Write;
            match stdin.write_all(config.as_bytes()) {
                // curl exited without reading its config. Its exit status reports the cause.
                Err(source) if source.kind() != std::io::ErrorKind::BrokenPipe => {
                    return Err(io_error(source));
                }
                Ok(()) | Err(_) => {}
            }
        }
        let output = child.wait_with_output().map_err(io_error)?;

        if !output.status.success() && output.status.code() != Some(CURL_EXIT_HTTP_ERROR) {
            return Err(BakerError::CurlExecutionFailed {
                code: output.status.code(),
                stderr: self.redact(&String::from_utf8_lossy(&output.stderr)),
            });
        }

        let report = String::from_utf8_lossy(&output.stdout);
        let mut lines = report.lines();
        let status = lines.next().and_then(|line| line.trim().parse().ok());
        let Some(status) = status else {
            return Err(BakerError::VeoApiError {
                message: format!("curl reported no HTTP status: {}", self.redact(&report)),
            });
        };
        if status >= 400 {
            return Err(BakerError::VeoHttpStatus {
                status,
                body: self.error_body(request.output),
            });
        }
        Ok(CurlResponse {
            status,
            redirect_url: lines.next().unwrap_or_default().trim().to_string(),
        })
    }

    /// Reads the start of an error response for use in a message.
    fn error_body(&self, path: &Path) -> String {
        let bytes = std::fs::read(path).unwrap_or_default();
        let kept = &bytes[..bytes.len().min(ERROR_BODY_LIMIT)];
        self.redact(&String::from_utf8_lossy(kept))
    }

    /// Delay before retry number `retry`, counting from zero.
    fn retry_delay(&self, retry: u32) -> Duration {
        self.retry_base_delay
            .saturating_mul(2u32.saturating_pow(retry))
            .min(MAX_RETRY_DELAY)
    }

    /// Runs `operation`, retrying while `retryable` accepts the error and retries remain.
    fn with_retries<T>(
        &self,
        retryable: fn(&BakerError) -> bool,
        mut operation: impl FnMut() -> Result<T, BakerError>,
    ) -> Result<T, BakerError> {
        let mut retry = 0;
        loop {
            match operation() {
                Err(error) if retry < self.max_retries && retryable(&error) => {
                    eprintln!("    Transient Veo fault, retrying: {error}");
                    std::thread::sleep(self.retry_delay(retry));
                    retry += 1;
                }
                result => return result,
            }
        }
    }

    /// Sends one authenticated JSON API request and parses the JSON response.
    fn api_request(
        &self,
        url: &str,
        post_body: Option<&str>,
        max_time: Duration,
    ) -> Result<serde_json::Value, BakerError> {
        ensure_trusted_host(url)?;
        let temp_path = std::env::temp_dir();
        let temp_error = |source| BakerError::Io {
            path: temp_path.clone(),
            source,
        };

        // The body cannot use stdin, which carries the key, and it may be too large for argv.
        let body_file = post_body
            .map(|body| {
                let file = tempfile::NamedTempFile::new().map_err(temp_error)?;
                std::fs::write(file.path(), body).map_err(temp_error)?;
                Ok::<_, BakerError>(file)
            })
            .transpose()?;
        let response_file = tempfile::NamedTempFile::new().map_err(temp_error)?;

        let response = self.run_curl(&CurlRequest {
            url,
            post_body: body_file.as_ref().map(tempfile::NamedTempFile::path),
            authenticated: true,
            output: response_file.path(),
            max_time,
        })?;
        if !(200..300).contains(&response.status) {
            return Err(BakerError::VeoHttpStatus {
                status: response.status,
                body: self.error_body(response_file.path()),
            });
        }

        let bytes = std::fs::read(response_file.path()).map_err(temp_error)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Downloads `uri` to `output`, following redirects under the policy in the module docs.
    fn download(&self, uri: &str, output: &Path) -> Result<(), BakerError> {
        ensure_trusted_host(uri)?;
        let mut url = uri.to_string();
        let mut authenticated = true;

        for _ in 0..=MAX_REDIRECT_HOPS {
            let response = self.run_curl(&CurlRequest {
                url: &url,
                post_body: None,
                authenticated,
                output,
                max_time: DOWNLOAD_TIMEOUT,
            })?;
            if (200..300).contains(&response.status) {
                return Ok(());
            }
            if !(300..400).contains(&response.status) {
                return Err(BakerError::VeoHttpStatus {
                    status: response.status,
                    body: self.error_body(output),
                });
            }
            if https_host(&response.redirect_url).is_none() {
                return Err(BakerError::RedirectRefused {
                    url: without_query(&response.redirect_url).to_string(),
                    reason: format!("HTTP {} target is not plain HTTPS", response.status),
                });
            }
            url = response.redirect_url;
            authenticated = false;
        }

        Err(BakerError::RedirectRefused {
            url: without_query(&url).to_string(),
            reason: format!("more than {MAX_REDIRECT_HOPS} redirects"),
        })
    }

    /// Returns the Vertex AI model path, which needs a project.
    fn vertex_model_url(&self) -> Result<String, BakerError> {
        let project = self
            .credentials
            .project_id
            .as_deref()
            .ok_or(BakerError::ProjectIdMissing)?;
        Ok(format!(
            "https://us-central1-aiplatform.googleapis.com/v1/projects/{project}/locations/us-central1/publishers/google/models/{}",
            self.model
        ))
    }

    /// Builds the full initiate endpoint URL for `:predictLongRunning`.
    ///
    /// # Errors
    /// Returns [`BakerError::ProjectIdMissing`] for a Vertex AI key without a project.
    pub fn build_initiate_url(&self) -> Result<String, BakerError> {
        if self.is_vertex_ai() {
            Ok(format!("{}:predictLongRunning", self.vertex_model_url()?))
        } else {
            Ok(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:predictLongRunning",
                self.model
            ))
        }
    }

    /// Builds the polling endpoint URL.
    ///
    /// For Vertex AI, polling is performed via the `fetchPredictOperation` endpoint.
    /// For Gemini API, polling is performed via GET on the operation path.
    ///
    /// # Errors
    /// Returns [`BakerError::ProjectIdMissing`] for a Vertex AI key without a project.
    pub fn build_poll_url(&self, operation_name: &str) -> Result<String, BakerError> {
        if self.is_vertex_ai() {
            Ok(format!(
                "{}:fetchPredictOperation",
                self.vertex_model_url()?
            ))
        } else {
            let clean_op = operation_name.trim_start_matches('/');
            Ok(format!(
                "https://generativelanguage.googleapis.com/v1beta/{clean_op}"
            ))
        }
    }

    /// Submits a video generation request to the Veo model.
    ///
    /// # Arguments
    /// - `prompt`: Descriptive prompt detailing the action and character visual styling.
    /// - `reference_image`: Optional file path to a reference image keyframe.
    ///
    /// # Errors
    /// Returns [`BakerError::VeoApiError`] on HTTP failure or non-success API responses,
    /// [`BakerError::Io`] on filesystem read failures, [`BakerError::CurlExecutionFailed`]
    /// on curl execution failures, or [`BakerError::ProjectIdMissing`] for a Vertex AI key
    /// without a project.
    pub fn initiate_generation(
        &self,
        prompt: &str,
        reference_image: Option<&Path>,
    ) -> Result<VeoOperation, BakerError> {
        let image_input = if let Some(img_path) = reference_image {
            let bytes = std::fs::read(img_path).map_err(|source| BakerError::Io {
                path: img_path.to_path_buf(),
                source,
            })?;
            let b64 = BASE64_STANDARD.encode(&bytes);
            let mime = if img_path.extension().is_some_and(|ext| ext == "png") {
                "image/png".to_string()
            } else {
                "image/jpeg".to_string()
            };
            Some(VeoImageInput {
                bytes_base64_encoded: b64,
                mime_type: mime,
            })
        } else {
            None
        };

        let request_body = VeoGenerateRequest {
            instances: vec![VeoInstance {
                prompt: prompt.to_string(),
                image: image_input,
            }],
            parameters: Some(VeoParameters::default()),
        };

        let json_body = serde_json::to_string(&request_body)?;
        let url = self.build_initiate_url()?;

        let resp_json = self.with_retries(request_started_no_work, || {
            self.api_request(&url, Some(&json_body), API_REQUEST_TIMEOUT)
        })?;

        if let Some(err_val) = resp_json.get("error") {
            let msg = err_val
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown API error");
            return Err(BakerError::VeoApiError {
                message: self.redact(&format!("Veo initiate error: {msg}")),
            });
        }

        let resp_text = resp_json.to_string();
        let op: VeoOperation = serde_json::from_value(resp_json)?;
        if op.name.is_empty() && !op.done {
            return Err(BakerError::VeoApiError {
                message: self.redact(&format!(
                    "No operation name returned in API response: {resp_text}"
                )),
            });
        }

        Ok(op)
    }

    /// Polls a running operation until `"done": true`, returning the completed operation.
    ///
    /// Transient faults, including responses that are not JSON, are retried with exponential
    /// backoff. Only consecutive faults count towards the limit, and the configured timeout
    /// bounds the whole call, including any request in flight.
    ///
    /// # Errors
    /// Returns [`BakerError::VeoTimeout`] if the operation exceeds the configured timeout,
    /// [`BakerError::VeoTransientExhausted`] after too many consecutive transient faults,
    /// [`BakerError::VeoOperationFailed`] if the API reports that the operation failed, or
    /// [`BakerError::VeoHttpStatus`] for a permanent HTTP error. None of these stop the
    /// operation itself, except the failure that the API reports. Also returns
    /// [`BakerError::ProjectIdMissing`] for a Vertex AI key without a project.
    pub fn poll_operation(&self, operation_name: &str) -> Result<VeoOperation, BakerError> {
        let deadline = std::time::Instant::now() + self.poll_timeout;
        let url = self.build_poll_url(operation_name)?;
        let poll_body = self
            .is_vertex_ai()
            .then(|| serde_json::json!({ "operationName": operation_name }).to_string());
        let mut consecutive_faults = 0;

        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(BakerError::VeoTimeout {
                    operation: operation_name.to_string(),
                });
            }

            let attempt = self
                .api_request(
                    &url,
                    poll_body.as_deref(),
                    remaining.min(API_REQUEST_TIMEOUT),
                )
                .and_then(|json| self.parse_polled_operation(operation_name, json));

            let wait = match attempt {
                Ok(op) if op.done => return Ok(op),
                Ok(_) => {
                    consecutive_faults = 0;
                    self.poll_interval
                }
                Err(error) if is_transient(&error) || matches!(error, BakerError::JsonError(_)) => {
                    if consecutive_faults >= self.max_retries {
                        return Err(BakerError::VeoTransientExhausted {
                            operation: operation_name.to_string(),
                            attempts: consecutive_faults + 1,
                            last_error: error.to_string(),
                        });
                    }
                    eprintln!("    Transient Veo fault while polling, retrying: {error}");
                    consecutive_faults += 1;
                    self.retry_delay(consecutive_faults - 1)
                }
                Err(error) => return Err(error),
            };

            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            std::thread::sleep(wait.min(remaining));
        }
    }

    /// Interprets a successful polling response.
    ///
    /// HTTP failures were already reported by status, so an `error` member here is the
    /// operation's own terminal result.
    fn parse_polled_operation(
        &self,
        operation_name: &str,
        json: serde_json::Value,
    ) -> Result<VeoOperation, BakerError> {
        if let Some(err_val) = json.get("error") {
            let msg = err_val
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown operation error");
            return Err(BakerError::VeoOperationFailed {
                operation: operation_name.to_string(),
                message: self.redact(msg),
            });
        }
        Ok(serde_json::from_value(json)?)
    }

    /// Extracts video content from a completed [`VeoOperation`] and writes it to disk.
    ///
    /// Handles both inline base64 bytes (`bytesBase64Encoded`) and external video URIs.
    /// `destination_mp4` is written only by an atomic rename of a validated file, so on any
    /// failure an existing file there is left as it was.
    ///
    /// # Errors
    /// Returns [`BakerError::VeoOperationFailed`] if the content was filtered,
    /// [`BakerError::VeoApiError`] if no video data can be resolved,
    /// [`BakerError::VideoValidationFailed`] if the data is not a complete video,
    /// [`BakerError::VeoHttpStatus`] or [`BakerError::CurlExecutionFailed`] on a failed
    /// download, or [`BakerError::Io`] on filesystem failure.
    pub fn save_operation_video(
        &self,
        operation: &VeoOperation,
        destination_mp4: &Path,
    ) -> Result<(), BakerError> {
        if let Some(parent) = destination_mp4.parent() {
            std::fs::create_dir_all(parent).map_err(|source| BakerError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let Some(resp_val) = &operation.response else {
            return Err(BakerError::VeoApiError {
                message: "Completed operation contained no response payload".into(),
            });
        };

        // Check for Google Cloud Responsible AI (RAI) media filtering
        if let Ok(gen_resp) = serde_json::from_value::<VeoGenerateVideoResponse>(resp_val.clone()) {
            if gen_resp.rai_media_filtered_count.unwrap_or(0) > 0 && gen_resp.videos.is_empty() {
                return Err(BakerError::VeoOperationFailed {
                    operation: operation.name.clone(),
                    message: "Video generation was filtered by Google Cloud Responsible AI (RAI) safety checks. Modify the prompt to reduce aggression/violence cues.".into(),
                });
            }
        }

        let parent = destination_mp4.parent().unwrap_or(Path::new("."));
        let partial = tempfile::Builder::new()
            .prefix(".veo-download-")
            .suffix(".part")
            .tempfile_in(parent)
            .map_err(|source| BakerError::Io {
                path: parent.to_path_buf(),
                source,
            })?;

        let found = if let Some(b64_str) = find_base64_video(resp_val) {
            // 1. Inline base64 video bytes
            let bytes =
                BASE64_STANDARD
                    .decode(b64_str.trim())
                    .map_err(|e| BakerError::VeoApiError {
                        message: format!("Base64 decoding failed for video output: {e}"),
                    })?;
            std::fs::write(partial.path(), &bytes).map_err(|source| BakerError::Io {
                path: partial.path().to_path_buf(),
                source,
            })?;
            true
        } else if let Some(uri) = find_video_uri(resp_val) {
            // 2. Downloadable video URI
            self.with_retries(is_transient, || self.download(&uri, partial.path()))?;
            true
        } else {
            false
        };

        if found {
            probe_video(&self.ffprobe_path, partial.path()).map_err(|error| match error {
                BakerError::VideoValidationFailed { reason, .. } => {
                    BakerError::VideoValidationFailed {
                        path: destination_mp4.to_path_buf(),
                        reason,
                    }
                }
                other => other,
            })?;
            partial
                .persist(destination_mp4)
                .map_err(|error| BakerError::Io {
                    path: destination_mp4.to_path_buf(),
                    source: error.error,
                })?;
            return Ok(());
        }

        Err(BakerError::VeoApiError {
            message: format!("Could not find video bytes or URI in operation response: {resp_val}"),
        })
    }

    /// End-to-end generation and baking pipeline for a single unit.
    ///
    /// Generates videos for `Idle`, `Walk`, and `Attack`, polls for completion,
    /// saves each `.mp4`, and compiles them into a vertical strip PNG and `.flipbook.ron`.
    /// An interrupted run can be repeated: valid clips are reused and recorded operations are
    /// resumed, see the module docs. `catalog` is the unit catalog loaded at run time, and
    /// `dirs` names the keyframe, video-cache and output directories.
    ///
    /// # Errors
    /// Returns [`BakerError::UnknownArchetype`] before any API call or filesystem change if
    /// `unit_name` is not an exact slug of `catalog`. Returns [`BakerError::CachedVideoInvalid`] if
    /// a cached clip is invalid. It is neither reused nor replaced, because replacing it would
    /// silently spend money. Otherwise returns [`BakerError`] on API, download, or FFmpeg
    /// transcoding errors.
    pub fn generate_and_bake_unit(
        &self,
        catalog: &sim_core::UnitCatalog,
        unit_name: &str,
        dirs: &UnitMediaDirs<'_>,
        frame_size: u32,
        fps: u32,
    ) -> Result<FlipbookAtlasMeta, BakerError> {
        validate_catalog_slug(catalog, unit_name)?;

        std::fs::create_dir_all(dirs.videos).map_err(|source| BakerError::Io {
            path: dirs.videos.to_path_buf(),
            source,
        })?;

        let mut clips: Vec<(AnimState, PathBuf)> = Vec::new();
        for state in [AnimState::Idle, AnimState::Walk, AnimState::Attack] {
            let clip = self.obtain_clip(catalog, unit_name, state, dirs)?;
            clips.push((state, clip));
        }

        println!("==> Stitching clips for {unit_name} into multi-action flipbook atlas...");
        bake_multi_action_videos_with_states(unit_name, &clips, dirs.output, frame_size, fps)
    }

    /// Returns the path of a valid clip, reusing the cache or a recorded operation if possible.
    fn obtain_clip(
        &self,
        catalog: &sim_core::UnitCatalog,
        unit_name: &str,
        state: AnimState,
        dirs: &UnitMediaDirs<'_>,
    ) -> Result<PathBuf, BakerError> {
        let state_str = state_slug(state);
        let mp4_path = dirs.videos.join(format!("{unit_name}_{state_str}.mp4"));
        let record_path = dirs
            .videos
            .join(format!("{unit_name}_{state_str}.operation.json"));

        if mp4_path.exists() {
            probe_video(&self.ffprobe_path, &mp4_path).map_err(|error| match error {
                BakerError::VideoValidationFailed { reason, .. } => {
                    BakerError::CachedVideoInvalid {
                        path: mp4_path.clone(),
                        reason,
                    }
                }
                other => other,
            })?;
            println!(
                "    ==> Reusing existing valid video: {}",
                mp4_path.display()
            );
            clear_pending(&record_path)?;
            return Ok(mp4_path);
        }

        let result = match load_pending(&record_path)? {
            Some(record) => {
                println!(
                    "    Resuming recorded operation '{}'...",
                    record.operation_name
                );
                self.clone()
                    .with_model(&record.model)
                    .poll_operation(&record.operation_name)
            }
            None => self.start_operation(catalog, unit_name, state, dirs.keyframes, &record_path),
        }
        .and_then(|completed| {
            println!("    Saving generated video to {}...", mp4_path.display());
            self.save_operation_video(&completed, &mp4_path)
        });

        match result {
            Ok(()) => {
                clear_pending(&record_path)?;
                Ok(mp4_path)
            }
            Err(error @ BakerError::VeoOperationFailed { .. }) => {
                // The operation can never succeed, so a rerun must be free to start a new one.
                clear_pending(&record_path)?;
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    /// Starts a paid generation, records it, and waits for it to finish.
    fn start_operation(
        &self,
        catalog: &sim_core::UnitCatalog,
        unit_name: &str,
        state: AnimState,
        keyframes_dir: &Path,
        record_path: &Path,
    ) -> Result<VeoOperation, BakerError> {
        let state_str = state_slug(state);

        // Check if keyframe exists for this specific state or fallback
        let keyframe_path = keyframes_dir.join(format!("{unit_name}_{state_str}.jpg"));
        let active_keyframe = keyframes_dir.join(format!("{unit_name}_active.jpg"));
        let idle_keyframe = keyframes_dir.join(format!("{unit_name}_idle.jpg"));

        let ref_keyframe = if keyframe_path.exists() {
            Some(keyframe_path.as_path())
        } else if state == AnimState::Attack && active_keyframe.exists() {
            Some(active_keyframe.as_path())
        } else if idle_keyframe.exists() {
            Some(idle_keyframe.as_path())
        } else {
            None
        };

        let prompt = build_action_prompt(catalog, unit_name, state)?;

        println!("==> Initiating Veo video generation for {unit_name} [{state:?}]...");
        let op = self.initiate_generation(&prompt, ref_keyframe)?;

        if !op.name.is_empty() {
            save_pending(
                record_path,
                &PendingOperation {
                    operation_name: op.name.clone(),
                    model: self.model.clone(),
                },
            )?;
        }

        if op.done {
            Ok(op)
        } else {
            println!("    Polling operation '{}' until completion...", op.name);
            self.poll_operation(&op.name)
        }
    }
}

/// The directories one unit's Veo pipeline reads from and writes to.
#[derive(Clone, Copy, Debug)]
pub struct UnitMediaDirs<'a> {
    /// Reference keyframe images, named `<slug>_<state>.jpg`, `<slug>_active.jpg` or
    /// `<slug>_idle.jpg`.
    pub keyframes: &'a Path,
    /// Cache of downloaded `.mp4` clips and pending-operation records; created if missing.
    pub videos: &'a Path,
    /// Destination of the baked `.png` strip and `.flipbook.ron` manifest.
    pub output: &'a Path,
}

/// Looks up the catalog entry whose slug is exactly `slug`.
///
/// Matching is deliberately exact, including case. A slug selects paid generation and names
/// files, so a near match must be an error rather than a guess.
pub(crate) fn exact_catalog_archetype<'a>(
    slug: &str,
    catalog: &'a sim_core::UnitCatalog,
) -> Result<&'a sim_core::UnitArchetype, BakerError> {
    catalog
        .get_by_slug(slug)
        .filter(|archetype| archetype.slug == slug)
        .ok_or_else(|| BakerError::UnknownArchetype {
            name: slug.to_string(),
            valid: catalog.archetypes().map(|a| a.slug.clone()).collect(),
        })
}

/// Checks that `slug` exactly names a unit of `catalog` and is safe to use in a file name.
///
/// `catalog` is the unit catalog loaded at run time (see [`crate::load_catalogs`]).
///
/// # Errors
/// Returns [`BakerError::UnknownArchetype`] if `slug` is not an exact catalog slug, or
/// [`BakerError::ValidationFailed`] if a catalog slug cannot name a portable file.
pub fn validate_catalog_slug(
    catalog: &sim_core::UnitCatalog,
    slug: &str,
) -> Result<(), BakerError> {
    exact_catalog_archetype(slug, catalog)?;
    crate::media::media_paths(Path::new(""), slug).map_err(|error| {
        BakerError::ValidationFailed {
            name: slug.to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(())
}

/// Constructs a descriptive action prompt adhering to Space Scuffle's stylized art direction.
///
/// `unit_name` must be an exact slug of `catalog`, the unit catalog loaded at run time. There is
/// no generic prompt for unknown units, because money must not be spent on a clip that no
/// catalog unit can use.
///
/// # Errors
/// Returns [`BakerError::UnknownArchetype`] if `unit_name` is not an exact catalog slug.
pub fn build_action_prompt(
    catalog: &sim_core::UnitCatalog,
    unit_name: &str,
    state: AnimState,
) -> Result<String, BakerError> {
    let base_style = "Stylized hand-painted 3D character, Arcane and StarCraft II cinematic portrait aesthetic, dark slate-indigo radial studio backdrop (#0B101B), dramatic 3/4 isometric front hero framing, glowing cyan/amber energy cores, bold proportions.";

    let arch = exact_catalog_archetype(unit_name, catalog)?;

    let action_desc = match state {
        AnimState::Idle => "Idle animation: steady rhythmic breathing movement, subtle weapon glow pulse, head and radar scanning smoothly in place.",
        AnimState::Walk => "Walk animation: dynamic forward stride, rhythmic mechanical bobbing, ground dust kick-up at feet, leaning into movement.",
        AnimState::Attack => "Attack animation: sharp weapon recoil, intense bright muzzle flash and projectile emission filling the right side of frame, followed by fluid spring recovery.",
        AnimState::Constructing => "Constructing animation: progressive holographic wireframe build-up from ground to top with active seam welding sparks.",
        AnimState::Die => "Death animation: dramatic armor fracture, glowing internal reactor blowout with flying mechanical debris dissolving into smoke.",
        AnimState::Cast => "Active tactical combat animation.",
    };

    if let Some(prompt) = arch.veo_prompt(state) {
        return Ok(format!("{prompt} {action_desc} {base_style}"));
    }
    Ok(format!(
        "{}: {} {action_desc} {base_style}",
        arch.display_name, arch.lore_description
    ))
}

/// Recursively searches a JSON value for base64 encoded video content.
fn find_base64_video(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(b64) = map.get("bytesBase64Encoded").and_then(|v| v.as_str()) {
                if !b64.is_empty() {
                    return Some(b64.to_string());
                }
            }
            for v in map.values() {
                if let Some(found) = find_base64_video(v) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                if let Some(found) = find_base64_video(v) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

/// Recursively searches a JSON value for video download URLs.
fn find_video_uri(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(uri) = map.get("uri").and_then(|v| v.as_str()) {
                if !uri.is_empty() {
                    return Some(uri.to_string());
                }
            }
            if let Some(uri) = map.get("gcsUri").and_then(|v| v.as_str()) {
                if !uri.is_empty() {
                    return Some(uri.to_string());
                }
            }
            for v in map.values() {
                if let Some(found) = find_video_uri(v) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                if let Some(found) = find_video_uri(v) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

/// Resolves the API key and Google Cloud project from the process environment and from
/// configuration files relative to the working directory.
///
/// See [`resolve_credentials_from`] for the priority order.
///
/// # Errors
/// As [`resolve_credentials_from`], or [`BakerError::Io`] if the working directory is unknown.
pub fn resolve_credentials() -> Result<VeoCredentials, BakerError> {
    let root = std::env::current_dir().map_err(|source| BakerError::Io {
        path: PathBuf::from("."),
        source,
    })?;
    resolve_credentials_from(|name| std::env::var(name).ok(), &root)
}

/// Resolves credentials from explicit sources, so that nothing depends on process state.
///
/// Priority:
/// 1. `GEMINI_API_KEY` or `GOOGLE_API_KEY`, and `GOOGLE_CLOUD_PROJECT`, from `lookup`.
/// 2. `.env` in `search_root`, then in its parent.
/// 3. `tmp/api-key.txt` in `search_root`, then in its parent.
///
/// # Errors
/// Returns [`BakerError::ApiKeyMissing`] if no key can be found,
/// [`BakerError::ProjectIdMissing`] for a Vertex AI key without a project, or
/// [`BakerError::Io`] if a candidate file exists but cannot be read.
pub fn resolve_credentials_from(
    lookup: impl Fn(&str) -> Option<String>,
    search_root: &Path,
) -> Result<VeoCredentials, BakerError> {
    let present = |name: &str| lookup(name).filter(|value| !value.trim().is_empty());
    let mut api_key = present("GEMINI_API_KEY").or_else(|| present("GOOGLE_API_KEY"));
    let mut project_id = present("GOOGLE_CLOUD_PROJECT");

    let read_if_present = |path: &Path| match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(BakerError::Io {
            path: path.to_path_buf(),
            source,
        }),
    };

    let parent_root = search_root.join("..");
    for root in [search_root, parent_root.as_path()] {
        if api_key.is_some() && project_id.is_some() {
            break;
        }
        if let Some(content) = read_if_present(&root.join(".env"))? {
            api_key = api_key.or_else(|| parse_key_from_env_content(&content));
            project_id = project_id.or_else(|| parse_project_from_env_content(&content));
        }
    }

    for root in [search_root, parent_root.as_path()] {
        if api_key.is_some() && project_id.is_some() {
            break;
        }
        if let Some(content) = read_if_present(&root.join("tmp/api-key.txt"))? {
            api_key = api_key.or_else(|| parse_key_from_api_txt(&content));
            project_id = project_id.or_else(|| parse_project_from_api_txt(&content));
        }
    }

    let key = api_key.ok_or(BakerError::ApiKeyMissing)?;
    if key.starts_with(VERTEX_KEY_PREFIX) && project_id.is_none() {
        return Err(BakerError::ProjectIdMissing);
    }

    Ok(VeoCredentials {
        api_key: key,
        project_id,
    })
}

/// Parses an API key from `.env` format lines (e.g. `GEMINI_API_KEY=...` or `GOOGLE_API_KEY=...`).
#[must_use]
pub fn parse_key_from_env_content(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        let stripped = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        if let Some(val) = stripped.strip_prefix("GEMINI_API_KEY=") {
            let clean = val.trim().trim_matches('"').trim_matches('\'').trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
        if let Some(val) = stripped.strip_prefix("GOOGLE_API_KEY=") {
            let clean = val.trim().trim_matches('"').trim_matches('\'').trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }
    None
}

/// Parses project ID from `.env` format lines (e.g. `GOOGLE_CLOUD_PROJECT=...`).
#[must_use]
pub fn parse_project_from_env_content(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        let stripped = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        if let Some(val) = stripped.strip_prefix("GOOGLE_CLOUD_PROJECT=") {
            let clean = val.trim().trim_matches('"').trim_matches('\'').trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }
    None
}

/// Parses API key from `tmp/api-key.txt` style content (`Gemini API Key: ...`).
#[must_use]
pub fn parse_key_from_api_txt(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("Gemini API Key:") {
            let clean = val.trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }
    None
}

/// Parses project number from `tmp/api-key.txt` style content.
#[must_use]
pub fn parse_project_from_api_txt(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("project number:") {
            let clean = val.trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
        if let Some(val) = trimmed.strip_prefix("project name: projects/") {
            let clean = val.trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }
    None
}
