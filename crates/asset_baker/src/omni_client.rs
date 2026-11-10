//! The Gemini Omni video client: Google's `gemini-omni-*` models on Vertex AI's Interactions
//! API, for the trial of Veo plan `2026-11-04-phase-8-5-veo-media.md`, Amendment 2.
//!
//! # Responsibility
//! - Builds one Interactions request per generation:
//!   `POST https://aiplatform.googleapis.com/v1beta1/projects/{project}/locations/global/interactions`
//!   with `{"model", "input": [text part, image parts], "response_format": {"type": "video",
//!   "aspect_ratio", "resolution", "duration": "4s"}, "generation_config": {"video_config":
//!   {"task": "image_to_video"}, "seed"?}, "background": true, "store": true}`. Image parts are
//!   inline (`{"type": "image", "data": <base64>, "mime_type"}`); the start frame comes first and
//!   the end frame second, and the text states their roles (Google documents the roles in the
//!   prompt text, not in a field).
//! - Polls `GET .../interactions/{id}` until the interaction's `status` is terminal, and finds
//!   the video in the finished interaction (`{"type": "video", "data" | "uri", "mime_type"}`,
//!   anywhere in its `steps` or `outputs`).
//! - [`OmniClient::preflight`]: two free GETs, the model resource and an interaction id that
//!   does not exist, which show whether the key may call the Interactions API at all.
//!
//! # How it fits
//! The transport is the Veo client's ([`VeoClient`]): the same credentials, the key only in a
//! curl configuration on stdin, HTTPS to the trusted hosts only (the global host
//! `aiplatform.googleapis.com` is one of them), no redirect followed with the key, and every
//! network text redacted. [`OmniClient`] implements [`VideoService`], so `generate-veo --model
//! gemini-omni-1.2-flash` runs the same plan, ledger, budget, review and bake as Veo.
//!
//! # Invariants
//! - Omni has no `negativePrompt`, `sampleCount` or `generateAudio` field. The clip's negative
//!   prompt is folded into the text as one sentence, audio is the model's default (on), and
//!   the ledger records the folded text and the parameters actually sent
//!   ([`VideoService::request_as_sent`]).
//! - A request that cannot be honoured (an end frame without a start frame, a duration outside
//!   Google's range, a silent clip, an unknown resolution) is refused before anything is sent.
//! - An interaction id enters a URL only if it is a plain resource name
//!   ([`BakerError::UnsafeInteractionId`]).

use crate::error::BakerError;
use crate::ledger::{ApiMode, OperationOutcome};
use crate::veo_client::{
    encode_image, GenerationImages, ProbeStatus, VeoApiErrorDetail, VeoClient, VeoOperation,
    VeoParameters, VideoSource,
};
use crate::video_service::VideoService;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

/// Prefix of every Gemini Omni model id.
pub const OMNI_MODEL_PREFIX: &str = "gemini-omni-";

/// The Gemini Omni model of the trial (GA on 2026-11-05).
pub const DEFAULT_OMNI_MODEL: &str = "gemini-omni-1.2-flash";

/// Vertex AI's global host, the only one Google documents for Gemini Omni.
pub const OMNI_HOST: &str = "aiplatform.googleapis.com";

/// The only location Google documents for Gemini Omni.
pub const OMNI_LOCATION: &str = "global";

/// API version of the Interactions API on Vertex AI.
const OMNI_API_VERSION: &str = "v1beta1";

/// Resolutions Google documents for Gemini Omni output.
pub const OMNI_RESOLUTIONS: [&str; 4] = ["360p", "720p", "1080p", "4k"];

/// Resolution of the trial's clips: the 1280×720 the pipeline bakes.
pub const DEFAULT_OMNI_RESOLUTION: &str = "720p";

/// Shortest clip Google documents, in seconds.
pub const OMNI_MIN_DURATION_SECS: u32 = 3;

/// Longest clip with a first or last frame, in seconds.
pub const OMNI_MAX_FRAMED_DURATION_SECS: u32 = 10;

/// Longest clip from text alone, in seconds.
pub const OMNI_MAX_DURATION_SECS: u32 = 15;

/// Default limit for polling one interaction. Omni renders a clip in about a minute or two;
/// the limit leaves room, and a run that hits it resumes the interaction next time.
pub const DEFAULT_OMNI_POLL_TIMEOUT: Duration = Duration::from_mins(15);

/// Interaction statuses after which nothing changes.
const TERMINAL_STATUSES: [&str; 4] = ["completed", "failed", "cancelled", "incomplete"];

/// The id the preflight asks for, which no interaction has.
const PREFLIGHT_MISSING_ID: &str = "preflight-no-such-interaction";

/// Whether `model` is a Gemini Omni model id.
#[must_use]
pub fn is_omni_model(model: &str) -> bool {
    model.starts_with(OMNI_MODEL_PREFIX)
}

/// The prompt text an Omni request sends: the clip's prompt, the roles of the pictures, and
/// the clip's negative prompt folded into one sentence (Omni has no `negativePrompt` field).
#[must_use]
pub fn compose_omni_prompt(
    prompt: &str,
    images: GenerationImages<'_>,
    negative_prompt: Option<&str>,
) -> String {
    let mut text = prompt.trim_end().to_string();
    match (images.start.is_some(), images.end.is_some()) {
        (true, true) => text.push_str(
            "\n\nThe first image is the first frame of the video and the second image is its last frame: the video starts exactly on the first image and ends exactly on the second.",
        ),
        (true, false) => text.push_str("\n\nThe image is the first frame of the video."),
        _ => {}
    }
    if let Some(negative) = negative_prompt.map(str::trim).filter(|n| !n.is_empty()) {
        text.push_str("\n\nKeep all of these out of the video: ");
        text.push_str(negative.trim_end_matches('.'));
        text.push('.');
    }
    text
}

/// Gemini Omni client on Vertex AI. See the module docs.
#[derive(Debug, Clone)]
pub struct OmniClient {
    transport: VeoClient,
    model: String,
    resolution: String,
    poll_timeout: Duration,
}

impl OmniClient {
    /// Wraps `transport` (whose credentials and curl it uses) for the Omni `model`, at
    /// [`DEFAULT_OMNI_RESOLUTION`].
    ///
    /// # Errors
    /// [`BakerError::NotAnOmniModel`] for another model id, [`BakerError::OmniNeedsVertex`]
    /// for a Gemini API key, [`BakerError::ProjectIdMissing`] without a project.
    pub fn new(transport: VeoClient, model: &str) -> Result<Self, BakerError> {
        if !is_omni_model(model) || !is_plain_segment(model) {
            return Err(BakerError::NotAnOmniModel {
                model: model.to_string(),
            });
        }
        if !transport.is_vertex_ai() {
            return Err(BakerError::OmniNeedsVertex);
        }
        if transport.project_id().is_none() {
            return Err(BakerError::ProjectIdMissing);
        }
        Ok(Self {
            transport,
            model: model.to_string(),
            resolution: DEFAULT_OMNI_RESOLUTION.to_string(),
            poll_timeout: DEFAULT_OMNI_POLL_TIMEOUT,
        })
    }

    /// Sets the output resolution, one of [`OMNI_RESOLUTIONS`].
    ///
    /// # Errors
    /// [`BakerError::InvalidArgument`] for any other value.
    pub fn with_resolution(mut self, resolution: &str) -> Result<Self, BakerError> {
        if !OMNI_RESOLUTIONS.contains(&resolution) {
            return Err(BakerError::InvalidArgument {
                reason: format!("resolution '{resolution}' is not one of {OMNI_RESOLUTIONS:?}"),
            });
        }
        self.resolution = resolution.to_string();
        Ok(self)
    }

    /// Overrides the polling limit of one interaction.
    #[must_use]
    pub fn with_poll_timeout(mut self, timeout: Duration) -> Self {
        self.poll_timeout = timeout;
        self
    }

    /// The resolution every request asks for.
    #[must_use]
    pub fn resolution(&self) -> &str {
        &self.resolution
    }

    fn project(&self) -> Result<&str, BakerError> {
        self.transport
            .project_id()
            .ok_or(BakerError::ProjectIdMissing)
    }

    /// The URL that creates an interaction.
    ///
    /// # Errors
    /// [`BakerError::ProjectIdMissing`].
    pub fn interactions_url(&self) -> Result<String, BakerError> {
        Ok(format!(
            "https://{OMNI_HOST}/{OMNI_API_VERSION}/projects/{}/locations/{OMNI_LOCATION}/interactions",
            self.project()?
        ))
    }

    /// The URL of interaction `id`: a full resource name (`projects/.../interactions/...`)
    /// under the API root, a bare id under the project's interactions.
    ///
    /// # Errors
    /// [`BakerError::UnsafeInteractionId`] for an id that is not a plain resource name,
    /// [`BakerError::ProjectIdMissing`].
    pub fn interaction_url(&self, id: &str) -> Result<String, BakerError> {
        let plain = !id.is_empty()
            && !id.starts_with('/')
            && !id.ends_with('/')
            && id.split('/').all(is_plain_segment);
        if !plain {
            return Err(BakerError::UnsafeInteractionId {
                id: self.transport.redact(id),
            });
        }
        if id.starts_with("projects/") {
            Ok(format!("https://{OMNI_HOST}/{OMNI_API_VERSION}/{id}"))
        } else if id.contains('/') {
            Err(BakerError::UnsafeInteractionId {
                id: self.transport.redact(id),
            })
        } else {
            Ok(format!("{}/{id}", self.interactions_url()?))
        }
    }

    /// The URL of the model resource the preflight reads.
    ///
    /// # Errors
    /// [`BakerError::ProjectIdMissing`].
    pub fn model_url(&self) -> Result<String, BakerError> {
        Ok(format!(
            "https://{OMNI_HOST}/{OMNI_API_VERSION}/projects/{}/locations/{OMNI_LOCATION}/publishers/google/models/{}",
            self.project()?,
            self.model
        ))
    }

    /// The parameters a request sends and the ledger records: aspect ratio, duration,
    /// resolution and seed; no negative prompt (folded into the text), sample count or audio
    /// switch, which Omni does not have.
    ///
    /// # Errors
    /// [`BakerError::InvalidArgument`] for a missing aspect ratio or duration, a duration
    /// outside Google's range for the pictures sent, or a silent clip (`generate_audio:
    /// Some(false)`), which Omni cannot be asked for.
    pub fn parameters_as_sent(
        &self,
        images: GenerationImages<'_>,
        parameters: &VeoParameters,
    ) -> Result<VeoParameters, BakerError> {
        let invalid = |reason: String| BakerError::InvalidArgument { reason };
        let aspect_ratio = parameters
            .aspect_ratio
            .clone()
            .ok_or_else(|| invalid("Omni needs an aspect ratio".into()))?;
        let duration = parameters
            .duration_seconds
            .ok_or_else(|| invalid("Omni needs a duration".into()))?;
        let framed = images.start.is_some() || images.end.is_some();
        let max = if framed {
            OMNI_MAX_FRAMED_DURATION_SECS
        } else {
            OMNI_MAX_DURATION_SECS
        };
        if !(OMNI_MIN_DURATION_SECS..=max).contains(&duration) {
            return Err(invalid(format!(
                "Omni clips last {OMNI_MIN_DURATION_SECS} to {max} s here; {duration} s was asked"
            )));
        }
        if parameters.generate_audio == Some(false) {
            return Err(invalid(
                "Omni has no switch for a silent clip; set generate_audio to None".into(),
            ));
        }
        Ok(VeoParameters {
            aspect_ratio: Some(aspect_ratio),
            duration_seconds: Some(duration),
            sample_count: None,
            generate_audio: None,
            negative_prompt: None,
            seed: parameters.seed,
            resolution: Some(self.resolution.clone()),
        })
    }

    /// The JSON body of a start request.
    ///
    /// # Errors
    /// [`BakerError::LastFrameWithoutStart`] for an end frame alone, [`BakerError::Io`] or
    /// [`BakerError::UnsupportedImageFormat`] for a picture that cannot be sent, and
    /// [`BakerError::InvalidArgument`] for parameters without an aspect ratio or duration.
    pub fn request_body(
        &self,
        prompt_as_sent: &str,
        images: GenerationImages<'_>,
        parameters_as_sent: &VeoParameters,
    ) -> Result<Value, BakerError> {
        if images.end.is_some() && images.start.is_none() {
            return Err(BakerError::LastFrameWithoutStart);
        }
        let missing = |what: &str| BakerError::InvalidArgument {
            reason: format!("an Omni request needs its {what}"),
        };
        let mut input = vec![json!({ "type": "text", "text": prompt_as_sent })];
        for path in [images.start, images.end].into_iter().flatten() {
            let image = encode_image(path)?;
            input.push(json!({
                "type": "image",
                "data": image.bytes_base64_encoded,
                "mime_type": image.mime_type,
            }));
        }
        let duration = parameters_as_sent
            .duration_seconds
            .ok_or_else(|| missing("duration"))?;
        let mut body = json!({
            "model": self.model,
            "input": input,
            "response_format": {
                "type": "video",
                "aspect_ratio": parameters_as_sent.aspect_ratio.clone().ok_or_else(|| missing("aspect ratio"))?,
                "resolution": parameters_as_sent.resolution.clone().unwrap_or_else(|| self.resolution.clone()),
                "duration": format!("{duration}s"),
            },
            "background": true,
            "store": true,
        });
        let mut generation_config = serde_json::Map::new();
        if images.start.is_some() {
            generation_config.insert("video_config".into(), json!({ "task": "image_to_video" }));
        }
        if let Some(seed) = parameters_as_sent.seed {
            generation_config.insert("seed".into(), json!(seed));
        }
        if !generation_config.is_empty() {
            body["generation_config"] = Value::Object(generation_config);
        }
        Ok(body)
    }

    /// Reads an interaction as an operation: `name` is its id (or `fallback_id`), `done` is a
    /// terminal status or an error, `response` the whole finished interaction, and `error` the
    /// interaction's error or, for a `failed`, `cancelled` or `incomplete` one without an
    /// error, its status. Every service text is redacted.
    #[must_use]
    pub fn parse_interaction(&self, json: Value, fallback_id: &str) -> VeoOperation {
        let text = |key: &str| json.get(key).and_then(Value::as_str).map(str::to_string);
        let name = text("id")
            .or_else(|| text("name"))
            .unwrap_or_else(|| fallback_id.to_string());
        let status = text("status").unwrap_or_default();
        let mut error = json
            .get("error")
            .filter(|e| !e.is_null())
            .map(|e| VeoApiErrorDetail {
                code: e.get("code").and_then(Value::as_i64),
                message: self.transport.redact(
                    e.get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error"),
                ),
                status: e.get("status").and_then(Value::as_str).map(str::to_string),
            });
        if error.is_none() && TERMINAL_STATUSES.contains(&status.as_str()) && status != "completed"
        {
            error = Some(VeoApiErrorDetail {
                code: None,
                message: format!("the interaction ended with status '{status}'"),
                status: Some(status.clone()),
            });
        }
        let done = error.is_some() || TERMINAL_STATUSES.contains(&status.as_str());
        VeoOperation {
            name,
            done,
            response: done.then_some(json),
            error,
        }
    }

    /// Two free GETs: the model resource and an interaction that does not exist. The second
    /// reaches the Interactions API with the key; `NOT_FOUND` there (or `INVALID_ARGUMENT`)
    /// means the key is accepted, `UNAUTHENTICATED` or `PERMISSION_DENIED` that it is not.
    ///
    /// # Errors
    /// [`BakerError::ProjectIdMissing`] or a transport failure.
    pub fn preflight(&self) -> Result<OmniPreflightReport, BakerError> {
        let model = self.transport.get_status(&self.model_url()?)?;
        let interactions = self
            .transport
            .get_status(&self.interaction_url(PREFLIGHT_MISSING_ID)?)?;
        let express_root = format!("https://{OMNI_HOST}/{OMNI_API_VERSION}");
        let express_model = self.transport.get_status(&format!(
            "{express_root}/publishers/google/models/{}",
            self.model
        ))?;
        let express_interactions = self.transport.get_status(&format!(
            "{express_root}/interactions/{PREFLIGHT_MISSING_ID}"
        ))?;
        // An empty object names no model and no input, so it cannot start paid work: the
        // service either refuses the key or refuses the body.
        let (http_status, error_status) = self
            .transport
            .post_status(&self.interactions_url()?, "{}")?;
        Ok(OmniPreflightReport {
            model_id: self.model.clone(),
            model,
            interactions,
            express_model,
            express_interactions,
            empty_create: ProbeStatus {
                http_status,
                error_status,
                message: None,
            },
        })
    }
}

/// The result of [`OmniClient::preflight`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmniPreflightReport {
    /// The model checked.
    pub model_id: String,
    /// The GET of the model resource.
    pub model: ProbeStatus,
    /// The GET of an interaction that does not exist.
    pub interactions: ProbeStatus,
    /// The GET of the model on express mode's project-less path
    /// (`v1beta1/publishers/google/models/{model}`), for diagnosis.
    pub express_model: ProbeStatus,
    /// The GET of a missing interaction on the project-less path (`v1beta1/interactions/{id}`),
    /// for diagnosis.
    pub express_interactions: ProbeStatus,
    /// A POST of an empty object to the create URL, which cannot start work: whether the
    /// create method accepts the key.
    pub empty_create: ProbeStatus,
}

impl OmniPreflightReport {
    /// Whether the key passed the Interactions API's authentication for both creating and
    /// reading: the empty create and the missing interaction were refused for their content
    /// (400, 404), not for the key.
    #[must_use]
    pub fn interactions_reachable(&self) -> bool {
        let passed = |probe: &ProbeStatus| matches!(probe.http_status, 200..=299 | 400 | 404);
        passed(&self.interactions) && passed(&self.empty_create)
    }
}

/// Whether `segment` is a non-empty run of ASCII letters, digits, `-`, `_` and `.`, other than
/// `.` and `..`.
fn is_plain_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Where the finished interaction's video is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FoundVideo {
    /// Inline base64 bytes.
    Inline(String),
    /// A URI (HTTPS or `gs://`).
    Uri(String),
}

/// Finds the first video content item (`"type": "video"` or a `video/` MIME type) with
/// `data` or a `uri`, anywhere in `value`.
fn find_video(value: &Value) -> Option<FoundVideo> {
    match value {
        Value::Object(map) => {
            let is_video = map.get("type").and_then(Value::as_str) == Some("video")
                || map
                    .get("mime_type")
                    .or_else(|| map.get("mimeType"))
                    .and_then(Value::as_str)
                    .is_some_and(|mime| mime.starts_with("video/"));
            if is_video {
                let field = |key: &str| {
                    map.get(key)
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty())
                        .map(str::to_string)
                };
                if let Some(data) = field("data") {
                    return Some(FoundVideo::Inline(data));
                }
                if let Some(uri) = field("uri").or_else(|| field("gcs_uri")) {
                    return Some(FoundVideo::Uri(uri));
                }
            }
            map.values().find_map(find_video)
        }
        Value::Array(items) => items.iter().find_map(find_video),
        _ => None,
    }
}

/// Replaces every inline `data` string by its length and redacts every other string.
fn strip_media(value: &mut Value, redact: &dyn Fn(&str) -> String) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                match child {
                    Value::String(text) if key == "data" => {
                        *child = Value::String(format!(
                            "<{} base64 characters, saved as the clip>",
                            text.len()
                        ));
                    }
                    _ => strip_media(child, redact),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_media(item, redact);
            }
        }
        Value::String(text) => *text = redact(text),
        _ => {}
    }
}

/// `uri` without its query string or fragment.
fn without_query(uri: &str) -> &str {
    uri.split(['?', '#']).next().unwrap_or(uri)
}

impl VideoService for OmniClient {
    fn model(&self) -> &str {
        &self.model
    }

    fn mode(&self) -> ApiMode {
        ApiMode::VertexExpress
    }

    fn accepts_end_frame_and_seed(&self) -> bool {
        true
    }

    fn ffprobe_path(&self) -> &Path {
        self.transport.ffprobe_path()
    }

    fn redact(&self, text: &str) -> String {
        self.transport.redact(text)
    }

    fn request_as_sent(
        &self,
        prompt: &str,
        images: GenerationImages<'_>,
        parameters: &VeoParameters,
    ) -> Result<(String, VeoParameters), BakerError> {
        let sent = self.parameters_as_sent(images, parameters)?;
        let text = compose_omni_prompt(prompt, images, parameters.negative_prompt.as_deref());
        Ok((text, sent))
    }

    fn start(
        &self,
        prompt_as_sent: &str,
        images: GenerationImages<'_>,
        parameters_as_sent: &VeoParameters,
    ) -> Result<VeoOperation, BakerError> {
        let body = self.request_body(prompt_as_sent, images, parameters_as_sent)?;
        let url = self.interactions_url()?;
        let response = self.transport.post_paid_json(&url, &body.to_string())?;
        let operation = self.parse_interaction(response, "");
        if operation.name.is_empty() {
            return Err(BakerError::VeoApiError {
                message: "the Interactions API returned no interaction id".into(),
            });
        }
        // An interaction that finished or failed already is returned as it is: the run records
        // its outcome like any finished operation.
        Ok(operation)
    }

    fn poll(&self, operation: &str) -> Result<VeoOperation, BakerError> {
        let url = self.interaction_url(operation)?;
        self.transport
            .poll_until_done(operation, &url, None, self.poll_timeout, &|json| {
                Ok(self.parse_interaction(json, operation))
            })
    }

    fn resume(&self, model: &str, operation: &str) -> Result<VeoOperation, BakerError> {
        if !is_omni_model(model) {
            return Err(BakerError::ResumeOnOtherService {
                operation: operation.to_string(),
                model: model.to_string(),
                client_model: self.model.clone(),
            });
        }
        self.poll(operation)
    }

    fn classify(&self, operation: &VeoOperation) -> OperationOutcome {
        if let Some(error) = &operation.error {
            return OperationOutcome::Failed {
                code: error.code,
                message: self.transport.redact(&error.message),
            };
        }
        let Some(response) = &operation.response else {
            return OperationOutcome::Failed {
                code: None,
                message: "the finished interaction carried no response".to_string(),
            };
        };
        match find_video(response) {
            Some(FoundVideo::Inline(_)) => OperationOutcome::VideoInline,
            Some(FoundVideo::Uri(uri)) => {
                let uri = self.transport.redact(without_query(&uri));
                if uri.starts_with("gs://") {
                    OperationOutcome::GcsUri { uri }
                } else {
                    OperationOutcome::VideoUri { uri }
                }
            }
            None => OperationOutcome::Failed {
                code: None,
                message: format!(
                    "the finished interaction (status '{}') returned no video",
                    response
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                ),
            },
        }
    }

    fn record(&self, operation: &VeoOperation) -> Value {
        let mut value = json!({
            "name": operation.name,
            "done": operation.done,
            "response": operation.response,
            "error": operation.error,
        });
        strip_media(&mut value, &|text| self.transport.redact(text));
        value
    }

    fn save_video(
        &self,
        operation: &VeoOperation,
        destination_mp4: &Path,
    ) -> Result<(), BakerError> {
        let found = operation.response.as_ref().and_then(find_video);
        match found {
            Some(FoundVideo::Inline(data)) => self
                .transport
                .store_video(VideoSource::Inline(&data), destination_mp4),
            Some(FoundVideo::Uri(uri)) if !uri.starts_with("gs://") => self
                .transport
                .store_video(VideoSource::Uri(&uri), destination_mp4),
            _ => Err(BakerError::VeoApiError {
                message: format!(
                    "the interaction '{}' holds no downloadable video",
                    self.transport.redact(&operation.name)
                ),
            }),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::veo_client::VeoCredentials;

    const KEY: &str = "AQ.secret-test-key";

    fn transport() -> VeoClient {
        VeoClient::with_credentials(VeoCredentials {
            api_key: KEY.to_string(),
            project_id: Some("demo-project".to_string()),
        })
    }

    fn client() -> OmniClient {
        OmniClient::new(transport(), DEFAULT_OMNI_MODEL).expect("a Vertex key and an Omni id")
    }

    fn png(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(b"rest-of-picture");
        std::fs::write(&path, bytes).expect("temp file");
        path
    }

    fn trial_parameters() -> VeoParameters {
        VeoParameters {
            negative_prompt: Some("stripes, zoom".into()),
            ..VeoParameters::default()
        }
    }

    #[test]
    fn only_omni_models_on_a_vertex_key_with_a_project_build_a_client() {
        assert!(matches!(
            OmniClient::new(transport(), "veo-3.1-fast-generate-001"),
            Err(BakerError::NotAnOmniModel { .. })
        ));
        assert!(matches!(
            OmniClient::new(transport(), "gemini-omni-1.2-flash/../x"),
            Err(BakerError::NotAnOmniModel { .. })
        ));
        let gemini_api = VeoClient::with_credentials(VeoCredentials {
            api_key: "AIza-key".into(),
            project_id: None,
        });
        assert!(matches!(
            OmniClient::new(gemini_api, DEFAULT_OMNI_MODEL),
            Err(BakerError::OmniNeedsVertex)
        ));
        let no_project = VeoClient::with_credentials(VeoCredentials {
            api_key: KEY.into(),
            project_id: None,
        });
        assert!(matches!(
            OmniClient::new(no_project, DEFAULT_OMNI_MODEL),
            Err(BakerError::ProjectIdMissing)
        ));
    }

    #[test]
    fn urls_use_the_global_host_and_location_and_never_the_key() {
        let client = client();
        let create = client.interactions_url().expect("project set");
        assert_eq!(
            create,
            "https://aiplatform.googleapis.com/v1beta1/projects/demo-project/locations/global/interactions"
        );
        assert_eq!(
            client.interaction_url("abc-123").expect("plain id"),
            format!("{create}/abc-123")
        );
        assert_eq!(
            client
                .interaction_url("projects/p/locations/global/interactions/x1")
                .expect("plain resource name"),
            "https://aiplatform.googleapis.com/v1beta1/projects/p/locations/global/interactions/x1"
        );
        assert!(client.model_url().expect("project").ends_with(
            "/projects/demo-project/locations/global/publishers/google/models/gemini-omni-1.2-flash"
        ));
        for url in [create, client.model_url().expect("project")] {
            assert!(!url.contains(KEY));
        }
    }

    #[test]
    fn unsafe_interaction_ids_never_reach_a_url() {
        let client = client();
        for id in [
            "", "../x", "a/../b", "a?key=1", "a b", "/abs", "x/y", "a#f", "trail/",
        ] {
            assert!(
                matches!(
                    client.interaction_url(id),
                    Err(BakerError::UnsafeInteractionId { .. })
                ),
                "{id:?} was accepted"
            );
        }
    }

    #[test]
    fn the_prompt_states_frame_roles_and_folds_the_negative_prompt() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pin = png(dir.path(), "pin.png");
        let both = GenerationImages {
            start: Some(&pin),
            end: Some(&pin),
        };
        let text = compose_omni_prompt("A mech marches.  ", both, Some(" stripes, zoom. "));
        assert!(text.starts_with("A mech marches.\n\nThe first image is the first frame"));
        assert!(text.ends_with("Keep all of these out of the video: stripes, zoom."));
        let start_only = GenerationImages {
            start: Some(&pin),
            end: None,
        };
        assert!(compose_omni_prompt("Walk.", start_only, None)
            .ends_with("The image is the first frame of the video."));
        assert_eq!(
            compose_omni_prompt("Walk.", GenerationImages::none(), Some("  ")),
            "Walk."
        );
    }

    #[test]
    fn parameters_as_sent_drop_what_omni_lacks_and_add_the_resolution() {
        let client = client()
            .with_resolution("360p")
            .expect("a documented resolution");
        let sent = client
            .parameters_as_sent(
                GenerationImages::none(),
                &VeoParameters {
                    seed: Some(7),
                    ..trial_parameters()
                },
            )
            .expect("valid parameters");
        assert_eq!(
            sent,
            VeoParameters {
                aspect_ratio: Some("16:9".into()),
                duration_seconds: Some(4),
                sample_count: None,
                generate_audio: None,
                negative_prompt: None,
                seed: Some(7),
                resolution: Some("360p".into()),
            }
        );
        assert!(client.clone().with_resolution("480p").is_err());
    }

    #[test]
    fn impossible_requests_are_refused_before_sending() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pin = png(dir.path(), "pin.png");
        let framed = GenerationImages {
            start: Some(&pin),
            end: Some(&pin),
        };
        let client = client();
        let long = VeoParameters {
            duration_seconds: Some(12),
            ..trial_parameters()
        };
        assert!(matches!(
            client.parameters_as_sent(framed, &long),
            Err(BakerError::InvalidArgument { .. })
        ));
        assert!(client
            .parameters_as_sent(GenerationImages::none(), &long)
            .is_ok());
        let silent = VeoParameters {
            generate_audio: Some(false),
            ..trial_parameters()
        };
        assert!(client
            .parameters_as_sent(GenerationImages::none(), &silent)
            .is_err());
        let end_only = GenerationImages {
            start: None,
            end: Some(&pin),
        };
        assert!(matches!(
            client.request_body("x", end_only, &trial_parameters()),
            Err(BakerError::LastFrameWithoutStart)
        ));
    }

    #[test]
    fn a_pinned_request_has_the_documented_shape() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pin = png(dir.path(), "pin.png");
        let images = GenerationImages {
            start: Some(&pin),
            end: Some(&pin),
        };
        let client = client();
        let (text, sent) = client
            .request_as_sent("A dome hums.", images, &trial_parameters())
            .expect("valid request");
        let body = client
            .request_body(&text, images, &sent)
            .expect("valid body");
        assert_eq!(body["model"], "gemini-omni-1.2-flash");
        assert_eq!(body["background"], true);
        assert_eq!(body["store"], true);
        assert_eq!(body["input"][0]["type"], "text");
        assert_eq!(body["input"][0]["text"], Value::String(text));
        for index in [1, 2] {
            assert_eq!(body["input"][index]["type"], "image");
            assert_eq!(body["input"][index]["mime_type"], "image/png");
            assert!(body["input"][index]["data"]
                .as_str()
                .is_some_and(|data| !data.is_empty()));
        }
        assert_eq!(
            body["response_format"],
            json!({"type": "video", "aspect_ratio": "16:9", "resolution": "720p", "duration": "4s"})
        );
        assert_eq!(
            body["generation_config"],
            json!({"video_config": {"task": "image_to_video"}})
        );
        assert!(body.get("negativePrompt").is_none());
        assert!(!body.to_string().contains(KEY));
    }

    #[test]
    fn a_seed_goes_into_generation_config_and_text_only_has_no_task() {
        let client = client();
        let sent = client
            .parameters_as_sent(
                GenerationImages::none(),
                &VeoParameters {
                    seed: Some(42),
                    ..trial_parameters()
                },
            )
            .expect("valid parameters");
        let body = client
            .request_body("Walk.", GenerationImages::none(), &sent)
            .expect("valid body");
        assert_eq!(body["generation_config"], json!({"seed": 42}));
        assert_eq!(body["input"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn interactions_parse_by_status() {
        let client = client();
        let running = client.parse_interaction(
            json!({"id": "i-1", "status": "in_progress", "object": "interaction"}),
            "",
        );
        assert_eq!(running.name, "i-1");
        assert!(!running.done && running.response.is_none() && running.error.is_none());

        let finished = client.parse_interaction(
            json!({"id": "i-1", "status": "completed", "steps": [
                {"type": "thought"},
                {"type": "model_output", "content": [{"type": "video", "mime_type": "video/mp4", "data": "AAAA"}]}
            ]}),
            "",
        );
        assert!(finished.done && finished.error.is_none());
        assert_eq!(client.classify(&finished), OperationOutcome::VideoInline);

        let failed = client.parse_interaction(json!({"status": "failed"}), "i-2");
        assert_eq!(failed.name, "i-2");
        assert!(failed.done);
        assert!(matches!(
            client.classify(&failed),
            OperationOutcome::Failed { code: None, message } if message.contains("failed")
        ));

        let error = client.parse_interaction(
            json!({"id": "i-3", "error": {"code": 3, "message": format!("bad {KEY}")}}),
            "",
        );
        assert!(error.done);
        assert!(matches!(
            client.classify(&error),
            OperationOutcome::Failed { code: Some(3), message } if !message.contains(KEY)
        ));
    }

    #[test]
    fn uri_videos_classify_by_scheme_and_no_video_is_a_failure() {
        let client = client();
        let https = client.parse_interaction(
            json!({"id": "a", "status": "completed", "outputs": [
                {"type": "video", "uri": "https://aiplatform.googleapis.com/v1beta1/files/x:download?alt=media"}
            ]}),
            "",
        );
        assert_eq!(
            client.classify(&https),
            OperationOutcome::VideoUri {
                uri: "https://aiplatform.googleapis.com/v1beta1/files/x:download".into()
            }
        );
        let gcs = client.parse_interaction(
            json!({"id": "b", "status": "completed", "steps": [{"content": [
                {"mime_type": "video/mp4", "uri": "gs://bucket/clip.mp4"}
            ]}]}),
            "",
        );
        assert!(matches!(
            client.classify(&gcs),
            OperationOutcome::GcsUri { .. }
        ));
        let image_only = client.parse_interaction(
            json!({"id": "c", "status": "completed", "input": [{"type": "image", "data": "QUJD"}]}),
            "",
        );
        assert!(matches!(
            client.classify(&image_only),
            OperationOutcome::Failed { message, .. } if message.contains("no video")
        ));
    }

    #[test]
    fn records_hold_no_media_bytes_and_no_key() {
        let client = client();
        let finished = client.parse_interaction(
            json!({"id": "i", "status": "completed", "note": KEY, "steps": [
                {"content": [{"type": "video", "mime_type": "video/mp4", "data": "AAAABBBB"}]}
            ]}),
            "",
        );
        let record = client.record(&finished).to_string();
        assert!(!record.contains("AAAABBBB"));
        assert!(record.contains("<8 base64 characters, saved as the clip>"));
        assert!(!record.contains(KEY));
    }

    #[test]
    fn resume_refuses_a_veo_operation() {
        assert!(matches!(
            client().resume("veo-3.1-fast-generate-001", "projects/p/operations/o"),
            Err(BakerError::ResumeOnOtherService { .. })
        ));
    }

    #[test]
    fn the_preflight_reads_a_refusal_as_unreachable() {
        let probe = |http_status| ProbeStatus {
            http_status,
            error_status: None,
            message: None,
        };
        let report = |status| OmniPreflightReport {
            model_id: DEFAULT_OMNI_MODEL.into(),
            model: probe(200),
            interactions: probe(status),
            express_model: probe(404),
            express_interactions: probe(404),
            empty_create: probe(400),
        };
        let create_refused = OmniPreflightReport {
            empty_create: probe(401),
            ..report(404)
        };
        assert!(!create_refused.interactions_reachable());
        assert!(report(404).interactions_reachable());
        assert!(report(400).interactions_reachable());
        assert!(!report(401).interactions_reachable());
        assert!(!report(403).interactions_reachable());
        assert!(!report(500).interactions_reachable());
    }
}
