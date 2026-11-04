//! Strongly-typed Google Gemini and Vertex AI Veo Video Generation client.
//!
//! # Architectural Charter
//! In accordance with `AGENTS.md` and
//! `docs/06-factions-and-video-style-guide.md`,
//! this module is the transport for generating authentic unit and structure animation clips
//! with Google's Veo models: it starts, polls and downloads one operation at a time, classifies
//! finished operations, and checks a key for free (`veo-preflight`). The generation parameters
//! every start request sends are data, [`VeoRequestSettings`] in `assets/media/veo_request.ron`. Which clips a run pays for,
//! and in what order, is decided by `veo_plan` and carried out by `veo_run`.
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
//! - `Debug` output redacts it, and every text that came from the network is passed through
//!   [`VeoClient::redact`] before it reaches an error, a log, a clip record or the ledger.
//!
//! # Paid Work Is Never Lost or Repeated
//! Each generation costs real money, so failures are handled to protect it:
//! - A video reaches its destination only by an atomic rename, after `ffprobe` has read every
//!   packet of it. An HTTP error page or a truncated download therefore cannot enter the
//!   clips directory.
//! - Every request has a timeout. Polling and downloads retry transient faults a bounded
//!   number of times. `:predictLongRunning` is retried only if the request cannot have started
//!   work, because any other retry could pay twice, and never after a quota refusal
//!   ([`BakerError::VeoQuotaExhausted`]), which ends the run.
//! - A reported operation error is returned as a finished [`VeoOperation`] with its `error`,
//!   so that the caller can record its code; the ledger (`ledger`) keeps each operation name,
//!   and a rerun resumes it instead of paying again.

#![allow(clippy::cast_possible_truncation, clippy::doc_markdown)]

use crate::error::BakerError;
use crate::ffmpeg_importer::{probe_video, DEFAULT_FFPROBE_PATH};
use crate::ledger::{ApiMode, OperationOutcome};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
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

/// Characters of a quota refusal's message kept in the ledger.
const QUOTA_REASON_LIMIT: usize = 200;

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

/// Configuration parameters for Veo generation, as sent in the start request's `parameters`
/// object and recorded in the ledger's `request_started` line. A `None` field is not sent, and
/// a field missing from an older ledger line reads as `None`.
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
    /// Whether the service also generates a sound track (`generateAudio`). `None` sends no
    /// field, and the service then generates sound; `Some(false)` asks for a silent clip,
    /// which is billed at a lower rate.
    #[serde(rename = "generateAudio", skip_serializing_if = "Option::is_none")]
    pub generate_audio: Option<bool>,
    /// What the model should keep out of the clip (`negativePrompt`), as plain words separated
    /// by commas. `None` sends no field. Vertex documents it in the `parameters` object of
    /// `:predictLongRunning` for the Veo models, with no model restriction.
    #[serde(
        rename = "negativePrompt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub negative_prompt: Option<String>,
}

impl Default for VeoParameters {
    /// The parameters of the pilot request (2026-11-04): 16:9, 4 s, one sample, and no
    /// `generateAudio` field. A paid run sends [`VeoRequestSettings::parameters`] instead.
    fn default() -> Self {
        Self {
            aspect_ratio: Some("16:9".to_string()),
            duration_seconds: Some(4),
            sample_count: Some(1),
            generate_audio: None,
            negative_prompt: None,
        }
    }
}

/// The request settings file's path relative to the asset root.
pub const VEO_REQUEST_FILE: &str = "media/veo_request.ron";

/// The Veo generation parameters every paid start request sends: `media/veo_request.ron`.
///
/// They are data, not a flag, so that a change (such as dropping `generate_audio` if the
/// service refuses it, or what `negative_prompt` keeps out of every clip) is one reviewed edit
/// that every later run follows.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VeoRequestSettings {
    /// Aspect ratio of the generated video, such as `"16:9"`.
    pub aspect_ratio: String,
    /// Length of the generated video, in seconds.
    pub duration_seconds: u32,
    /// Videos generated per request. Must be 1: the budget counts one clip per request and the
    /// run downloads one video per operation.
    pub sample_count: u32,
    /// `Some(false)` asks for a silent clip, `Some(true)` for sound; `None` sends no field.
    pub generate_audio: Option<bool>,
    /// Content every clip must avoid, sent as `negativePrompt`; `None` (or absent) sends no
    /// field. A present value must not be blank.
    #[serde(default)]
    pub negative_prompt: Option<String>,
}

impl VeoRequestSettings {
    /// Reads and checks the settings file at `path`.
    ///
    /// # Errors
    /// [`BakerError::Io`] if it cannot be read, or [`BakerError::DataFile`] if it is not valid
    /// RON of this shape, names an empty aspect ratio, a zero duration, or a sample count
    /// other than 1.
    pub fn load(path: &Path) -> Result<Self, BakerError> {
        let text = std::fs::read_to_string(path).map_err(|source| BakerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let invalid = |reason: String| BakerError::DataFile {
            path: path.to_path_buf(),
            reason,
        };
        let settings: Self = ron::from_str(&text).map_err(|error| invalid(error.to_string()))?;
        if settings.aspect_ratio.trim().is_empty() {
            return Err(invalid("aspect_ratio is empty".into()));
        }
        if settings.duration_seconds == 0 {
            return Err(invalid("duration_seconds is 0".into()));
        }
        if settings
            .negative_prompt
            .as_deref()
            .is_some_and(|text| text.trim().is_empty())
        {
            return Err(invalid("negative_prompt is blank".into()));
        }
        if settings.sample_count != 1 {
            return Err(invalid(format!(
                "sample_count is {}; the budget and the run handle exactly 1 video per request",
                settings.sample_count
            )));
        }
        Ok(settings)
    }

    /// The parameters object these settings send.
    #[must_use]
    pub fn parameters(&self) -> VeoParameters {
        VeoParameters {
            aspect_ratio: Some(self.aspect_ratio.clone()),
            duration_seconds: Some(self.duration_seconds),
            sample_count: Some(self.sample_count),
            generate_audio: self.generate_audio,
            negative_prompt: self.negative_prompt.clone(),
        }
    }
}

impl std::fmt::Display for VeoParameters {
    /// One line naming every field sent, in the API's spelling, for logs and dry runs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut fields = Vec::new();
        if let Some(value) = &self.aspect_ratio {
            fields.push(format!("aspectRatio {value}"));
        }
        if let Some(value) = self.duration_seconds {
            fields.push(format!("durationSeconds {value}"));
        }
        if let Some(value) = self.sample_count {
            fields.push(format!("sampleCount {value}"));
        }
        if let Some(value) = self.generate_audio {
            fields.push(format!("generateAudio {value}"));
        }
        if let Some(value) = &self.negative_prompt {
            fields.push(format!("negativePrompt {value:?}"));
        }
        f.write_str(&fields.join(", "))
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
    parameters: VeoParameters,
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
            parameters: VeoParameters::default(),
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

    /// Sets the generation parameters every start request sends, normally
    /// [`VeoRequestSettings::parameters`] of the bundle's `media/veo_request.ron`. Without
    /// this call the client sends [`VeoParameters::default`].
    #[must_use]
    pub fn with_parameters(mut self, parameters: VeoParameters) -> Self {
        self.parameters = parameters;
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

    /// The API the key selects.
    #[must_use]
    pub fn mode(&self) -> ApiMode {
        if self.is_vertex_ai() {
            ApiMode::VertexExpress
        } else {
            ApiMode::GeminiApi
        }
    }

    /// The generation parameters every start request sends (see [`Self::with_parameters`]).
    #[must_use]
    pub fn parameters(&self) -> VeoParameters {
        self.parameters.clone()
    }

    /// The ffprobe executable used to validate downloads.
    #[must_use]
    pub fn ffprobe_path(&self) -> &Path {
        &self.ffprobe_path
    }

    /// Replaces every occurrence of the API key in `text`. Every text that came from the
    /// network or from curl passes through here before it reaches an error, a log or the ledger.
    #[must_use]
    pub fn redact(&self, text: &str) -> String {
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
            parameters: Some(self.parameters()),
        };

        let json_body = serde_json::to_string(&request_body)?;
        let url = self.build_initiate_url()?;

        let resp_json = self.with_retries(request_started_no_work, || {
            self.api_request(&url, Some(&json_body), API_REQUEST_TIMEOUT)
                .map_err(as_quota_refusal)
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
        let mut operation: VeoOperation = serde_json::from_value(json)?;
        if let Some(error) = &mut operation.error {
            // A reported error is the operation's terminal result, whatever `done` says.
            error.message = self.redact(&error.message);
            operation.done = true;
            if operation.name.is_empty() {
                operation.name = operation_name.to_string();
            }
        }
        Ok(operation)
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
            message: self.redact(&format!(
                "Could not find video bytes or URI in operation response: {resp_val}"
            )),
        })
    }

    /// Classifies a finished operation for the ledger (plan D-3, D-4).
    ///
    /// Inline bytes win over a URI, an HTTPS URI over a `gs://` one; a response with neither
    /// and a filtered count is `Filtered`. Every text that came from the service is redacted.
    #[must_use]
    pub fn classify_operation(&self, operation: &VeoOperation) -> OperationOutcome {
        if let Some(error) = &operation.error {
            return OperationOutcome::Failed {
                code: error.code,
                message: self.redact(&error.message),
            };
        }
        let Some(response) = &operation.response else {
            return OperationOutcome::Failed {
                code: None,
                message: "the finished operation carried no response".to_string(),
            };
        };
        if find_base64_video(response).is_some() {
            return OperationOutcome::VideoInline;
        }
        if let Some(uri) = find_video_uri(response) {
            let uri = self.redact(without_query(&uri));
            return if uri.starts_with("gs://") {
                OperationOutcome::GcsUri { uri }
            } else {
                OperationOutcome::VideoUri { uri }
            };
        }
        let filtered = response
            .get("raiMediaFilteredCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if filtered > 0 {
            let reasons = response.get("raiMediaFilteredReasons").map_or_else(
                || format!("{filtered} result(s) filtered"),
                ToString::to_string,
            );
            return OperationOutcome::Filtered {
                reason: self.redact(&reasons),
            };
        }
        OperationOutcome::Failed {
            code: None,
            message: "the finished operation returned no video".to_string(),
        }
    }

    /// The operation as JSON for the clip record, with inline video bytes replaced by their
    /// length and every string redacted.
    #[must_use]
    pub fn operation_record(&self, operation: &VeoOperation) -> serde_json::Value {
        let mut value = serde_json::json!({
            "name": operation.name,
            "done": operation.done,
            "response": operation.response,
            "error": operation.error,
        });
        strip_record(&mut value, &|text| self.redact(text));
        value
    }

    /// The model resource URL: the free resource the preflight reads (plan D-5).
    ///
    /// # Errors
    /// [`BakerError::ProjectIdMissing`] for a Vertex AI key without a project.
    pub fn build_model_url(&self) -> Result<String, BakerError> {
        if self.is_vertex_ai() {
            self.vertex_model_url()
        } else {
            Ok(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}",
                self.model
            ))
        }
    }

    /// One authenticated, free GET of the model resource on the generation transport.
    ///
    /// A refusal (4xx, 5xx) is a report, not an error, so that its status can be shown.
    ///
    /// # Errors
    /// [`BakerError::ProjectIdMissing`], or a transport failure ([`BakerError::Io`],
    /// [`BakerError::CurlExecutionFailed`], [`BakerError::InvalidApiKey`]).
    pub fn preflight(&self) -> Result<PreflightReport, BakerError> {
        let url = self.build_model_url()?;
        ensure_trusted_host(&url)?;
        let response_file = tempfile::NamedTempFile::new().map_err(|source| BakerError::Io {
            path: std::env::temp_dir(),
            source,
        })?;
        let (http_status, error_status) = match self.run_curl(&CurlRequest {
            url: &url,
            post_body: None,
            authenticated: true,
            output: response_file.path(),
            max_time: API_REQUEST_TIMEOUT,
        }) {
            Ok(response) => (response.status, None),
            Err(BakerError::VeoHttpStatus { status, body }) => {
                (status, google_error(&body).and_then(|e| e.status))
            }
            Err(other) => return Err(other),
        };
        Ok(PreflightReport {
            mode: self.mode(),
            model: self.model.clone(),
            http_status,
            error_status,
        })
    }
}

/// Replaces inline video bytes in an operation record by their length and redacts strings.
fn strip_record(value: &mut serde_json::Value, redact: &dyn Fn(&str) -> String) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == "bytesBase64Encoded" {
                    let length = child.as_str().map_or(0, str::len);
                    *child = serde_json::Value::String(format!(
                        "<{length} base64 characters, saved as the clip>"
                    ));
                } else {
                    strip_record(child, redact);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                strip_record(item, redact);
            }
        }
        serde_json::Value::String(text) => *text = redact(text),
        _ => {}
    }
}

/// The result of [`VeoClient::preflight`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightReport {
    /// The API the key selects.
    pub mode: ApiMode,
    /// The model read.
    pub model: String,
    /// HTTP status of the GET.
    pub http_status: u16,
    /// The `status` field of a Google error body, such as `PERMISSION_DENIED`.
    pub error_status: Option<String>,
}

impl PreflightReport {
    /// Whether the key could read the model resource.
    #[must_use]
    pub fn model_visible(&self) -> bool {
        (200..300).contains(&self.http_status)
    }
}

/// The parts of a Google API error body this pipeline acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GoogleError {
    status: Option<String>,
    message: String,
    reason: Option<String>,
}

/// Parses `{"error": {"status", "message", "details": [{"reason"}]}}` from an error body.
fn google_error(body: &str) -> Option<GoogleError> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = json.get("error")?;
    let text = |value: Option<&serde_json::Value>| {
        value
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
    };
    let reason = error
        .get("details")
        .and_then(serde_json::Value::as_array)
        .and_then(|details| details.iter().find_map(|d| text(d.get("reason"))));
    Some(GoogleError {
        status: text(error.get("status")),
        message: text(error.get("message")).unwrap_or_default(),
        reason,
    })
}

/// Turns a 429 whose body reports an exhausted quota into [`BakerError::VeoQuotaExhausted`].
///
/// Google reports quota exhaustion as HTTP 429 with `status: RESOURCE_EXHAUSTED`, also for
/// per-minute request limits; any of those ends the run, because waiting costs nothing. A 429
/// without that status or the word "quota" stays an ordinary, retryable 429.
fn as_quota_refusal(error: BakerError) -> BakerError {
    let BakerError::VeoHttpStatus { status: 429, body } = &error else {
        return error;
    };
    let parsed = google_error(body);
    let exhausted = parsed.as_ref().is_some_and(|e| {
        e.status.as_deref() == Some("RESOURCE_EXHAUSTED")
            || e.message.to_lowercase().contains("quota")
    }) || body.to_lowercase().contains("quota");
    if !exhausted {
        return error;
    }
    let (status, reason) = parsed.map_or((None, None), |e| {
        let message: String = e.message.chars().take(QUOTA_REASON_LIMIT).collect();
        (e.status, e.reason.or(Some(message)))
    });
    BakerError::VeoQuotaExhausted {
        http_status: 429,
        status,
        reason,
    }
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
