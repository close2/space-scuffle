//! Strongly-typed Google Gemini and Vertex AI Veo Video Generation client.
//!
//! # Architectural Charter
//! In accordance with [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md) and
//! [06-factions-and-video-style-guide.md](file:///home/cl/projects/space-scuffle/docs/06-factions-and-video-style-guide.md),
//! this module provides an automated client for generating authentic 3D unit animation
//! clips (`Idle`, `Walk`, `Attack`) using Google's Veo video generation models.
//!
//! # Dual-API Support (Gemini API & Vertex AI Express Mode)
//! - **Vertex AI Express Mode (`AQ.` prefix):** Uses the Vertex AI endpoints with API key
//!   authentication (`projects/{project}/locations/us-central1/publishers/google/models/{model}`).
//!   Polls via the `fetchPredictOperation` RPC with `{"operationName": "{operation_name}"}`.
//! - **Gemini Developer API (`AIza` prefix):** Uses `generativelanguage.googleapis.com/v1beta`
//!   and polls GET `v1beta/{operation_name}?key={api_key}`.
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
use crate::ffmpeg_importer::bake_multi_action_videos_with_states;
use crate::manifest::FlipbookAtlasMeta;
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use sim_types::enums::AnimState;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Default Veo model identifier for video generation.
pub const DEFAULT_VEO_MODEL: &str = "veo-3.1-fast-generate-001";

/// Default Google Cloud project number placeholder when `GOOGLE_CLOUD_PROJECT` is not set.
pub const DEFAULT_PROJECT_ID: &str = "000000000000";

/// Default polling interval in seconds for asynchronous video operations.
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;

/// Default maximum polling duration in seconds (5 minutes).
pub const DEFAULT_POLL_TIMEOUT_SECS: u64 = 300;

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
#[derive(Debug, Clone)]
pub struct VeoCredentials {
    /// Secret API key token.
    pub api_key: String,
    /// Associated Google Cloud project ID or project number.
    pub project_id: String,
}

/// Strongly-typed client for Google Veo Video Generation.
#[derive(Debug, Clone)]
pub struct VeoClient {
    credentials: VeoCredentials,
    model: String,
    curl_path: PathBuf,
    poll_interval: Duration,
    poll_timeout: Duration,
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
            poll_interval: Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS),
            poll_timeout: Duration::from_secs(DEFAULT_POLL_TIMEOUT_SECS),
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

    /// Accessor for the configured model name.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Checks if the API key corresponds to a Vertex AI Express Mode key (`AQ.` prefix).
    #[must_use]
    pub fn is_vertex_ai(&self) -> bool {
        self.credentials.api_key.starts_with("AQ.")
    }

    /// Builds the full initiate endpoint URL for `:predictLongRunning`.
    #[must_use]
    pub fn build_initiate_url(&self) -> String {
        if self.is_vertex_ai() {
            format!(
                "https://us-central1-aiplatform.googleapis.com/v1/projects/{}/locations/us-central1/publishers/google/models/{}:predictLongRunning?key={}",
                self.credentials.project_id, self.model, self.credentials.api_key
            )
        } else {
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:predictLongRunning?key={}",
                self.model, self.credentials.api_key
            )
        }
    }

    /// Builds the polling endpoint URL.
    ///
    /// For Vertex AI, polling is performed via the `fetchPredictOperation` endpoint.
    /// For Gemini API, polling is performed via GET on the operation path.
    #[must_use]
    pub fn build_poll_url(&self, operation_name: &str) -> String {
        if self.is_vertex_ai() {
            format!(
                "https://us-central1-aiplatform.googleapis.com/v1/projects/{}/locations/us-central1/publishers/google/models/{}:fetchPredictOperation?key={}",
                self.credentials.project_id, self.model, self.credentials.api_key
            )
        } else {
            let clean_op = operation_name.trim_start_matches('/');
            format!(
                "https://generativelanguage.googleapis.com/v1beta/{}?key={}",
                clean_op, self.credentials.api_key
            )
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
    /// [`BakerError::Io`] on filesystem read failures, or [`BakerError::CurlExecutionFailed`]
    /// on curl execution failures.
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
        let url = self.build_initiate_url();

        let mut child = Command::new(&self.curl_path)
            .arg("-s")
            .arg("-X")
            .arg("POST")
            .arg("-H")
            .arg("Content-Type: application/json")
            .arg("--data-binary")
            .arg("@-")
            .arg(&url)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|source| BakerError::Io {
                path: self.curl_path.clone(),
                source,
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin
                .write_all(json_body.as_bytes())
                .map_err(|source| BakerError::Io {
                    path: self.curl_path.clone(),
                    source,
                })?;
        }

        let output = child.wait_with_output().map_err(|source| BakerError::Io {
            path: self.curl_path.clone(),
            source,
        })?;

        if !output.status.success() {
            return Err(BakerError::CurlExecutionFailed {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        let resp_text = String::from_utf8_lossy(&output.stdout);
        let resp_json: serde_json::Value = serde_json::from_str(&resp_text)?;

        if let Some(err_val) = resp_json.get("error") {
            let msg = err_val
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown API error");
            return Err(BakerError::VeoApiError {
                message: format!("Veo initiate error: {msg}"),
            });
        }

        let op: VeoOperation = serde_json::from_value(resp_json)?;
        if op.name.is_empty() && !op.done {
            return Err(BakerError::VeoApiError {
                message: format!("No operation name returned in API response: {resp_text}"),
            });
        }

        Ok(op)
    }

    /// Polls a running operation until `"done": true`, returning the completed operation.
    ///
    /// # Errors
    /// Returns [`BakerError::VeoTimeout`] if the operation exceeds the configured timeout,
    /// or [`BakerError::VeoApiError`] if polling yields an error status.
    pub fn poll_operation(&self, operation_name: &str) -> Result<VeoOperation, BakerError> {
        let start = std::time::Instant::now();
        let url = self.build_poll_url(operation_name);

        while start.elapsed() < self.poll_timeout {
            let output = if self.is_vertex_ai() {
                let poll_body = serde_json::json!({ "operationName": operation_name });
                Command::new(&self.curl_path)
                    .arg("-s")
                    .arg("-X")
                    .arg("POST")
                    .arg("-H")
                    .arg("Content-Type: application/json")
                    .arg("-d")
                    .arg(poll_body.to_string())
                    .arg(&url)
                    .output()
            } else {
                Command::new(&self.curl_path)
                    .arg("-s")
                    .arg("-H")
                    .arg("Content-Type: application/json")
                    .arg(&url)
                    .output()
            };

            let output = output.map_err(|source| BakerError::Io {
                path: self.curl_path.clone(),
                source,
            })?;

            if !output.status.success() {
                return Err(BakerError::CurlExecutionFailed {
                    code: output.status.code(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                });
            }

            let resp_text = String::from_utf8_lossy(&output.stdout);
            let resp_json: serde_json::Value = serde_json::from_str(&resp_text)?;

            if let Some(err_val) = resp_json.get("error") {
                let msg = err_val
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown polling error");
                return Err(BakerError::VeoApiError {
                    message: format!("Veo poll error: {msg}"),
                });
            }

            let op: VeoOperation = serde_json::from_value(resp_json)?;
            if op.done {
                return Ok(op);
            }

            std::thread::sleep(self.poll_interval);
        }

        Err(BakerError::VeoTimeout {
            operation: operation_name.to_string(),
        })
    }

    /// Extracts video content from a completed [`VeoOperation`] and writes it to disk.
    ///
    /// Handles both inline base64 bytes (`bytesBase64Encoded`) and external video URIs.
    ///
    /// # Errors
    /// Returns [`BakerError::VeoApiError`] if no video data can be resolved,
    /// or [`BakerError::Io`] on filesystem write failure.
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
                return Err(BakerError::VeoApiError {
                    message: "Video generation was filtered by Google Cloud Responsible AI (RAI) safety checks. Modify the prompt to reduce aggression/violence cues.".into(),
                });
            }
        }

        // 1. Check for inline base64 video bytes
        if let Some(b64_str) = find_base64_video(resp_val) {
            let bytes =
                BASE64_STANDARD
                    .decode(b64_str.trim())
                    .map_err(|e| BakerError::VeoApiError {
                        message: format!("Base64 decoding failed for video output: {e}"),
                    })?;
            std::fs::write(destination_mp4, &bytes).map_err(|source| BakerError::Io {
                path: destination_mp4.to_path_buf(),
                source,
            })?;
            return Ok(());
        }

        // 2. Check for downloadable video URI
        if let Some(uri) = find_video_uri(resp_val) {
            let output = Command::new(&self.curl_path)
                .arg("-L")
                .arg("-s")
                .arg("-H")
                .arg(format!("x-goog-api-key: {}", self.credentials.api_key))
                .arg("-o")
                .arg(destination_mp4)
                .arg(&uri)
                .output()
                .map_err(|source| BakerError::Io {
                    path: self.curl_path.clone(),
                    source,
                })?;

            if !output.status.success() {
                return Err(BakerError::CurlExecutionFailed {
                    code: output.status.code(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                });
            }

            let meta = std::fs::metadata(destination_mp4).map_err(|source| BakerError::Io {
                path: destination_mp4.to_path_buf(),
                source,
            })?;
            if meta.len() == 0 {
                return Err(BakerError::VeoApiError {
                    message: format!(
                        "Downloaded video file at {} was empty",
                        destination_mp4.display()
                    ),
                });
            }

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
    ///
    /// # Errors
    /// Returns [`BakerError`] on API, download, or FFmpeg transcoding errors.
    pub fn generate_and_bake_unit(
        &self,
        unit_name: &str,
        keyframes_dir: &Path,
        videos_dir: &Path,
        output_dir: &Path,
        frame_size: u32,
        fps: u32,
    ) -> Result<FlipbookAtlasMeta, BakerError> {
        std::fs::create_dir_all(videos_dir).map_err(|source| BakerError::Io {
            path: videos_dir.to_path_buf(),
            source,
        })?;

        let states = [AnimState::Idle, AnimState::Walk, AnimState::Attack];
        let mut clips: Vec<(AnimState, PathBuf)> = Vec::new();

        for state in states {
            let state_str = match state {
                AnimState::Idle => "idle",
                AnimState::Walk => "walk",
                AnimState::Attack => "attack",
                AnimState::Constructing => "constructing",
                AnimState::Cast => "cast",
                AnimState::Die => "die",
            };

            let mp4_path = videos_dir.join(format!("{unit_name}_{state_str}.mp4"));

            // Check if valid video already exists on disk (non-empty > 1000 bytes)
            if mp4_path.exists() && std::fs::metadata(&mp4_path).is_ok_and(|m| m.len() > 1000) {
                println!(
                    "    ==> Reusing existing valid video: {}",
                    mp4_path.display()
                );
                clips.push((state, mp4_path));
                continue;
            }

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

            let prompt = build_action_prompt(unit_name, state);

            println!("==> Initiating Veo video generation for {unit_name} [{state:?}]...");
            let op = self.initiate_generation(&prompt, ref_keyframe)?;

            let completed_op = if op.done {
                op
            } else {
                println!("    Polling operation '{}' until completion...", op.name);
                self.poll_operation(&op.name)?
            };

            println!("    Saving generated video to {}...", mp4_path.display());
            self.save_operation_video(&completed_op, &mp4_path)?;
            clips.push((state, mp4_path));
        }

        println!("==> Stitching clips for {unit_name} into multi-action flipbook atlas...");
        bake_multi_action_videos_with_states(unit_name, &clips, output_dir, frame_size, fps)
    }
}

/// Constructs a descriptive action prompt adhering to Space Scuffle's stylized art direction.
#[must_use]
pub fn build_action_prompt(unit_name: &str, state: AnimState) -> String {
    let base_style = "Stylized hand-painted 3D character, Arcane and StarCraft II cinematic portrait aesthetic, dark slate-indigo radial studio backdrop (#0B101B), dramatic 3/4 isometric front hero framing, glowing cyan/amber energy cores, bold proportions.";

    let catalog = sim_core::UnitCatalog::default();
    let arch_opt = catalog
        .get_by_slug(unit_name)
        .or_else(|| catalog.get_by_name(unit_name))
        .or_else(|| {
            let clean = unit_name.to_lowercase().replace(['-', ' '], "_");
            catalog
                .get_by_slug(&clean)
                .or_else(|| catalog.get_by_name(&clean))
        })
        .or_else(|| {
            let lower = unit_name.to_lowercase();
            catalog.archetypes().find(|a| {
                a.sc2_inspiration
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&lower)
                    || a.display_name.to_lowercase().contains(&lower)
            })
        });

    let action_desc = match state {
        AnimState::Idle => "Idle animation: steady rhythmic breathing movement, subtle weapon glow pulse, head and radar scanning smoothly in place.",
        AnimState::Walk => "Walk animation: dynamic forward stride, rhythmic mechanical bobbing, ground dust kick-up at feet, leaning into movement.",
        AnimState::Attack => "Attack animation: sharp weapon recoil, intense bright muzzle flash and projectile emission filling the right side of frame, followed by fluid spring recovery.",
        AnimState::Constructing => "Constructing animation: progressive holographic wireframe build-up from ground to top with active seam welding sparks.",
        AnimState::Die => "Death animation: dramatic armor fracture, glowing internal reactor blowout with flying mechanical debris dissolving into smoke.",
        AnimState::Cast => "Active tactical combat animation.",
    };

    if let Some(arch) = arch_opt {
        if let Some(prompt) = arch.veo_prompt(state) {
            return format!("{prompt} {action_desc} {base_style}");
        }
        return format!(
            "{}: {} {action_desc} {base_style}",
            arch.display_name, arch.lore_description
        );
    }

    format!("{unit_name}: Sci-fi tactical combat unit with heavy armored plating and vibrant energy emitters. {action_desc} {base_style}")
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

/// Resolves the API key and Google Cloud project from environment variables or configuration files.
///
/// Priority:
/// 1. `GEMINI_API_KEY` or `GOOGLE_API_KEY` environment variables.
/// 2. `/home/cl/projects/space-scuffle/.env` or `.env` in the working directory.
/// 3. `/home/cl/projects/space-scuffle/tmp/api-key.txt` or `tmp/api-key.txt`.
///
/// # Errors
/// Returns [`BakerError::ApiKeyMissing`] if no key can be found.
pub fn resolve_credentials() -> Result<VeoCredentials, BakerError> {
    let mut api_key = std::env::var("GEMINI_API_KEY")
        .or_else(|_| std::env::var("GOOGLE_API_KEY"))
        .ok()
        .filter(|k| !k.trim().is_empty());

    let mut project_id = std::env::var("GOOGLE_CLOUD_PROJECT")
        .ok()
        .filter(|p| !p.trim().is_empty());

    let env_paths = [
        PathBuf::from("/home/cl/projects/space-scuffle/.env"),
        PathBuf::from(".env"),
        PathBuf::from("../.env"),
    ];

    for path in &env_paths {
        if api_key.is_some() && project_id.is_some() {
            break;
        }
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if api_key.is_none() {
                    api_key = parse_key_from_env_content(&content);
                }
                if project_id.is_none() {
                    project_id = parse_project_from_env_content(&content);
                }
            }
        }
    }

    let txt_paths = [
        PathBuf::from("/home/cl/projects/space-scuffle/tmp/api-key.txt"),
        PathBuf::from("tmp/api-key.txt"),
        PathBuf::from("../tmp/api-key.txt"),
    ];

    for path in &txt_paths {
        if api_key.is_some() && project_id.is_some() {
            break;
        }
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if api_key.is_none() {
                    api_key = parse_key_from_api_txt(&content);
                }
                if project_id.is_none() {
                    project_id = parse_project_from_api_txt(&content);
                }
            }
        }
    }

    let key = api_key.ok_or(BakerError::ApiKeyMissing)?;
    let proj = project_id.unwrap_or_else(|| DEFAULT_PROJECT_ID.to_string());

    Ok(VeoCredentials {
        api_key: key,
        project_id: proj,
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
