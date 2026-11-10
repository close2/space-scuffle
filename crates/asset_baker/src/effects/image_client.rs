//! The Gemini image-model client for effect stills (plan `2026-11-09-effect-visualization.md`
//! §6.2 item 1, §6.6; Q0078).
//!
//! # Responsibility
//! - Builds `generateContent` requests for a Gemini image model ("Nano Banana") on Vertex AI:
//!   `POST https://us-central1-aiplatform.googleapis.com/v1/projects/{project}/locations/us-central1/publishers/google/models/{model}:generateContent`
//!   (or the global endpoint `aiplatform.googleapis.com` with `locations/global`, which Google
//!   documents for the newest image models), with
//!   `{"contents": [{"role": "user", "parts": [...]}], "generationConfig": {"responseModalities": ["IMAGE"], "imageConfig": {"aspectRatio": "1:1", "imageSize": "1K"}}}`.
//!   A text-to-image request sends one text part; the edit request of a difference-matte pair
//!   sends the picture on black as an `inlineData` part, then the edit text.
//! - Parses the image out of `candidates[].content.parts[].inlineData {mimeType, data}`
//!   (`inline_data` / `mime_type` are accepted too), and reports a block or finish reason when
//!   no image came back.
//! - [`ImageClient::preflight`]: a free `:countTokens` POST on the model, which proves the key
//!   may call it without generating an image.
//! - [`run_image_job`]: one element's paid requests with their ledger lines
//!   (`image_request_started`, then `image_downloaded`, `image_quota_refused` or
//!   `image_request_failed`).
//!
//! # How it fits
//! The transport is the Veo client's ([`VeoClient`]): the same credential resolution, the key
//! only in a curl configuration on stdin, HTTPS to the trusted hosts only, no redirects, and
//! every network text redacted. `asset_baker effect-image` checks the model's price and the
//! total budget (`media/veo_budget.ron`, which prices every image model id) before it reads a
//! credential; the ledger is the Veo ledger, so the budget sums both.
//!
//! # Invariants
//! - The `image_request_started` line is written and synced before the request is sent, so a
//!   crash cannot hide a spend. A quota refusal is recorded as free; any other failure is
//!   recorded and counted as paid.
//! - A returned image reaches its file only by an atomic rename, after it decoded.
//! - The model id and location are checked to be plain identifiers before they enter a URL.

use super::texture_set::ImageRole;
use super::{write_atomic, EffectError};
use crate::error::BakerError;
use crate::ledger::{sha256_hex, ApiMode, ImageRequestKind, Ledger, LedgerEvent};
use crate::prompts::{ComposedPrompt, ImageFormat};
use crate::veo_client::VeoClient;
use base64::prelude::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Default Vertex AI location of image requests.
pub const DEFAULT_IMAGE_LOCATION: &str = "us-central1";

/// The location Google documents for the global endpoint.
pub const GLOBAL_LOCATION: &str = "global";

/// Aspect ratio requested: effect cells are square.
pub const IMAGE_ASPECT_RATIO: &str = "1:1";

/// Default image size requested (`1K`, `2K` or `4K`, upper-case K).
pub const DEFAULT_IMAGE_SIZE: &str = "1K";

/// Image sizes the service accepts.
pub const IMAGE_SIZES: [&str; 3] = ["1K", "2K", "4K"];

/// Characters of a response kept in an error.
const REASON_LIMIT: usize = 300;

/// An image the model returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnedImage {
    /// The image bytes.
    pub bytes: Vec<u8>,
    /// Their format, read from the bytes.
    pub format: ImageFormat,
}

/// The result of [`ImageClient::preflight`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePreflightReport {
    /// The API the key selects.
    pub mode: ApiMode,
    /// The model checked.
    pub model: String,
    /// The `:countTokens` URL used (it never holds the key).
    pub url: String,
    /// HTTP status of the POST.
    pub http_status: u16,
    /// The `status` field of a Google error body, such as `PERMISSION_DENIED`.
    pub error_status: Option<String>,
}

impl ImagePreflightReport {
    /// Whether the key may call the model.
    #[must_use]
    pub fn callable(&self) -> bool {
        (200..300).contains(&self.http_status)
    }
}

/// The image-model client. See the module docs.
#[derive(Debug, Clone)]
pub struct ImageClient {
    transport: VeoClient,
    location: String,
    image_size: String,
}

/// Whether `text` is a plain identifier: 1 to 128 characters of `a-z`, `0-9`, `.`, `-`, `_`.
fn plain_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'))
}

impl ImageClient {
    /// A client on `transport` (whose model is the image model) at `location`, requesting
    /// `image_size` images.
    ///
    /// # Errors
    /// [`EffectError::InvalidRecipe`] for a model id or location that is not a plain
    /// identifier, or an image size the service does not accept.
    pub fn new(
        transport: VeoClient,
        location: &str,
        image_size: &str,
    ) -> Result<Self, EffectError> {
        let invalid = |reason: String| EffectError::InvalidRecipe {
            path: PathBuf::from("--model"),
            reason,
        };
        if !plain_identifier(transport.model()) {
            return Err(invalid(format!(
                "model id '{}' is not a plain identifier",
                transport.model()
            )));
        }
        if !plain_identifier(location) {
            return Err(invalid(format!(
                "location '{location}' is not a plain identifier"
            )));
        }
        if !IMAGE_SIZES.contains(&image_size) {
            return Err(invalid(format!(
                "image size '{image_size}' is not one of {IMAGE_SIZES:?}"
            )));
        }
        Ok(Self {
            transport,
            location: location.to_string(),
            image_size: image_size.to_string(),
        })
    }

    /// The model id.
    #[must_use]
    pub fn model(&self) -> &str {
        self.transport.model()
    }

    /// The API the key selects.
    #[must_use]
    pub fn mode(&self) -> ApiMode {
        self.transport.mode()
    }

    /// Replaces the key in `text`.
    #[must_use]
    pub fn redact(&self, text: &str) -> String {
        self.transport.redact(text)
    }

    /// The URL of `method` (`generateContent`, `countTokens`) on the model.
    ///
    /// # Errors
    /// [`EffectError::Baker`] with [`BakerError::ProjectIdMissing`] for a Vertex AI key without
    /// a project.
    pub fn method_url(&self, method: &str) -> Result<String, EffectError> {
        let model = self.model();
        if !self.transport.is_vertex_ai() {
            return Ok(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:{method}"
            ));
        }
        let project = self
            .transport
            .project_id()
            .ok_or(BakerError::ProjectIdMissing)?;
        let location = &self.location;
        let host = if location == GLOBAL_LOCATION {
            "aiplatform.googleapis.com".to_string()
        } else {
            format!("{location}-aiplatform.googleapis.com")
        };
        Ok(format!(
            "https://{host}/v1/projects/{project}/locations/{location}/publishers/google/models/{model}:{method}"
        ))
    }

    /// The `generationConfig` of every image request.
    fn generation_config(&self) -> Value {
        json!({
            "responseModalities": ["IMAGE"],
            "imageConfig": {
                "aspectRatio": IMAGE_ASPECT_RATIO,
                "imageSize": self.image_size,
            },
        })
    }

    /// The body of a text-to-image request.
    #[must_use]
    pub fn generate_body(&self, prompt: &str) -> Value {
        json!({
            "contents": [{"role": "user", "parts": [{"text": prompt}]}],
            "generationConfig": self.generation_config(),
        })
    }

    /// The body of an edit request: the picture, then the edit text.
    ///
    /// # Errors
    /// [`EffectError::Baker`] with [`BakerError::UnsupportedImageFormat`] for bytes that are
    /// neither PNG nor JPEG.
    pub fn edit_body(&self, prompt: &str, image: &[u8]) -> Result<Value, EffectError> {
        let format =
            ImageFormat::sniff(image).ok_or_else(|| BakerError::UnsupportedImageFormat {
                path: PathBuf::from("<edit source>"),
            })?;
        Ok(json!({
            "contents": [{"role": "user", "parts": [
                {"inlineData": {"mimeType": format.mime_type(), "data": BASE64_STANDARD.encode(image)}},
                {"text": prompt},
            ]}],
            "generationConfig": self.generation_config(),
        }))
    }

    /// Generates an image from `prompt`. Paid.
    ///
    /// # Errors
    /// Transport failures as [`EffectError::Baker`] (a quota refusal is
    /// [`BakerError::VeoQuotaExhausted`]), and [`EffectError::ImageNotReturned`].
    pub fn generate(&self, prompt: &str) -> Result<ReturnedImage, EffectError> {
        let url = self.method_url("generateContent")?;
        let body = self.generate_body(prompt).to_string();
        let response = self.transport.post_paid_json(&url, &body)?;
        parse_image_response(&response, &|text| self.redact(text))
    }

    /// Renders the element of `image` on white by an edit request. Paid.
    ///
    /// # Errors
    /// As [`Self::generate`], and an unsupported source format.
    pub fn edit(&self, prompt: &str, image: &[u8]) -> Result<ReturnedImage, EffectError> {
        let url = self.method_url("generateContent")?;
        let body = self.edit_body(prompt, image)?.to_string();
        let response = self.transport.post_paid_json(&url, &body)?;
        parse_image_response(&response, &|text| self.redact(text))
    }

    /// A free `:countTokens` POST on the model: proves the key may call it without generating
    /// an image. A refusal is a report, not an error.
    ///
    /// # Errors
    /// [`BakerError::ProjectIdMissing`] and transport failures, as [`EffectError::Baker`].
    pub fn preflight(&self) -> Result<ImagePreflightReport, EffectError> {
        let url = self.method_url("countTokens")?;
        let body = json!({"contents": [{"role": "user", "parts": [{"text": "preflight"}]}]});
        let (http_status, error_status) = self.transport.post_status(&url, &body.to_string())?;
        Ok(ImagePreflightReport {
            mode: self.mode(),
            model: self.model().to_string(),
            url,
            http_status,
            error_status,
        })
    }
}

/// The first image in a `generateContent` response.
///
/// # Errors
/// [`EffectError::ImageNotReturned`] with the block or finish reason (redacted) when the
/// response holds no decodable PNG or JPEG.
pub fn parse_image_response(
    response: &Value,
    redact: &dyn Fn(&str) -> String,
) -> Result<ReturnedImage, EffectError> {
    let parts = response
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|candidate| {
            candidate
                .pointer("/content/parts")
                .and_then(Value::as_array)
        })
        .flatten();
    for part in parts {
        let Some(inline) = part.get("inlineData").or_else(|| part.get("inline_data")) else {
            continue;
        };
        let Some(data) = inline.get("data").and_then(Value::as_str) else {
            continue;
        };
        let Ok(bytes) = BASE64_STANDARD.decode(data) else {
            continue;
        };
        if let Some(format) = ImageFormat::sniff(&bytes) {
            return Ok(ReturnedImage { bytes, format });
        }
    }
    let reason = response
        .pointer("/promptFeedback/blockReason")
        .or_else(|| response.pointer("/candidates/0/finishReason"))
        .and_then(Value::as_str)
        .map_or_else(
            || {
                let text = response.to_string();
                format!(
                    "no image part in {}",
                    text.chars().take(REASON_LIMIT).collect::<String>()
                )
            },
            ToString::to_string,
        );
    Err(EffectError::ImageNotReturned {
        reason: redact(&reason),
    })
}

/// One element's image requests.
#[derive(Debug, Clone)]
pub struct ImageJob<'a> {
    /// Texture set.
    pub set: &'a str,
    /// Element.
    pub element: &'a str,
    /// The text-to-image prompt.
    pub prompt: &'a ComposedPrompt,
    /// The on-white edit prompt, for a difference-matte pair.
    pub edit_prompt: Option<&'a ComposedPrompt>,
    /// Directory the images are written to.
    pub out_dir: &'a Path,
}

/// An image a job saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedImage {
    /// Which picture.
    pub role: ImageRole,
    /// Where it was written.
    pub path: PathBuf,
    /// Its SHA-256.
    pub sha256: String,
}

/// Runs one element's paid requests: the picture on black and, for a pair, its edit on white,
/// each with its ledger lines (see the module docs). Stops at the first failure.
///
/// # Errors
/// [`EffectError::Ledger`] if a line cannot be written (before the request is sent for a
/// start line), and the request's own failure after it was recorded.
pub fn run_image_job(
    client: &ImageClient,
    ledger: &mut Ledger,
    job: &ImageJob<'_>,
) -> Result<Vec<SavedImage>, EffectError> {
    let black = one_request(client, ledger, job, None)?;
    let mut saved = vec![black];
    if let Some(edit) = job.edit_prompt {
        let source = std::fs::read(&saved[0].path).map_err(|source| EffectError::Io {
            path: saved[0].path.clone(),
            source,
        })?;
        let white = one_request(client, ledger, job, Some((edit, &source)))?;
        saved.push(white);
    }
    Ok(saved)
}

/// One request and its ledger lines; `edit` is the edit prompt and the picture on black.
fn one_request(
    client: &ImageClient,
    ledger: &mut Ledger,
    job: &ImageJob<'_>,
    edit: Option<(&ComposedPrompt, &[u8])>,
) -> Result<SavedImage, EffectError> {
    let request_id = ledger.next_request_id();
    let (kind, role, prompt) = match edit {
        Some((prompt, _)) => (ImageRequestKind::OnWhiteEdit, ImageRole::OnWhite, prompt),
        None => (ImageRequestKind::OnBlack, ImageRole::OnBlack, job.prompt),
    };
    ledger.append(LedgerEvent::ImageRequestStarted {
        request_id: request_id.clone(),
        set: job.set.to_string(),
        element: job.element.to_string(),
        model: client.model().to_string(),
        mode: client.mode(),
        kind,
        prompt_sha256: sha256_hex(prompt.text.as_bytes()),
        prompt: prompt.text.clone(),
        source_sha256: edit.map(|(_, bytes)| sha256_hex(bytes)),
    })?;
    let result = match edit {
        Some((prompt, bytes)) => client.edit(&prompt.text, bytes),
        None => client.generate(&prompt.text),
    };
    let image = match result {
        Ok(image) => image,
        Err(error) => {
            record_failure(client, ledger, job, &request_id, &error)?;
            return Err(error);
        }
    };
    let saved = save_image(job, &request_id, role, &image)?;
    let (width, height) =
        image::image_dimensions(&saved.path).map_err(|source| EffectError::Image {
            path: saved.path.clone(),
            source,
        })?;
    ledger.append(LedgerEvent::ImageDownloaded {
        request_id,
        set: job.set.to_string(),
        element: job.element.to_string(),
        model: client.model().to_string(),
        bytes: image.bytes.len() as u64,
        sha256: saved.sha256.clone(),
        width,
        height,
        mime_type: image.format.mime_type().to_string(),
        path: saved.path.display().to_string(),
    })?;
    Ok(saved)
}

/// Records a failed request: free for a quota refusal, paid otherwise.
fn record_failure(
    client: &ImageClient,
    ledger: &mut Ledger,
    job: &ImageJob<'_>,
    request_id: &str,
    error: &EffectError,
) -> Result<(), EffectError> {
    let event = match error {
        EffectError::Baker(baker) => match baker.as_ref() {
            BakerError::VeoQuotaExhausted {
                http_status,
                status,
                reason,
            } => LedgerEvent::ImageQuotaRefused {
                request_id: request_id.to_string(),
                set: job.set.to_string(),
                element: job.element.to_string(),
                http_status: *http_status,
                status: status.clone(),
                reason: reason.clone(),
            },
            other => failed_event(client, job, request_id, &other.to_string()),
        },
        other => failed_event(client, job, request_id, &other.to_string()),
    };
    ledger.append(event)?;
    Ok(())
}

/// An `image_request_failed` event with the redacted error.
fn failed_event(
    client: &ImageClient,
    job: &ImageJob<'_>,
    request_id: &str,
    error: &str,
) -> LedgerEvent {
    LedgerEvent::ImageRequestFailed {
        request_id: request_id.to_string(),
        set: job.set.to_string(),
        element: job.element.to_string(),
        error: client.redact(error),
    }
}

/// Writes a returned image as `<set>.<element>.<request>.<role>.<ext>` after checking that it
/// decodes.
fn save_image(
    job: &ImageJob<'_>,
    request_id: &str,
    role: ImageRole,
    image: &ReturnedImage,
) -> Result<SavedImage, EffectError> {
    let role_name = match role {
        ImageRole::OnBlack => "on_black",
        ImageRole::OnWhite => "on_white",
    };
    let extension = match image.format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
    };
    let request = request_id.replace(['/', '\\', ':'], "-");
    let path = job.out_dir.join(format!(
        "{}.{}.{request}.{role_name}.{extension}",
        job.set, job.element
    ));
    image::load_from_memory(&image.bytes).map_err(|source| EffectError::Image {
        path: path.clone(),
        source,
    })?;
    write_atomic(&path, &image.bytes)?;
    Ok(SavedImage {
        role,
        path,
        sha256: sha256_hex(&image.bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_that_could_change_a_url_are_refused() {
        assert!(plain_identifier("gemini-3.1-flash-image"));
        for bad in ["", "a/b", "a?b", "A", "x y", "a#b", "a:b"] {
            assert!(!plain_identifier(bad), "{bad}");
        }
    }

    #[test]
    fn a_response_without_an_image_names_its_reason() {
        let blocked = json!({"promptFeedback": {"blockReason": "SAFETY"}});
        assert!(matches!(
            parse_image_response(&blocked, &|t| t.to_string()),
            Err(EffectError::ImageNotReturned { reason }) if reason == "SAFETY"
        ));
        let text_only = json!({"candidates": [{"content": {"parts": [{"text": "hi"}]}, "finishReason": "STOP"}]});
        assert!(matches!(
            parse_image_response(&text_only, &|t| t.to_string()),
            Err(EffectError::ImageNotReturned { reason }) if reason == "STOP"
        ));
    }
}
