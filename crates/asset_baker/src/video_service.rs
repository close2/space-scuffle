//! The video-generation service a paid run talks to: Veo ([`VeoClient`]) or Gemini Omni
//! ([`crate::omni_client::OmniClient`]), behind one trait (Veo plan
//! `2026-11-04-phase-8-5-veo-media.md`, Amendment 2).
//!
//! # Responsibility
//! [`VideoService`] is everything `veo_run` needs from a service: the model id and API mode the
//! ledger records, the prompt text and parameters a request sends exactly as the ledger must
//! record them, starting, polling and resuming one generation, classifying a finished one,
//! the redacted record kept beside each clip, and saving its validated video.
//!
//! # How it fits
//! `generate-veo` builds a [`VeoClient`], and for a `gemini-omni-*` model wraps it in an
//! `OmniClient`, which reuses its transport (the key only in a curl configuration on stdin,
//! HTTPS to the trusted hosts only, no redirects followed with the key, every network text
//! redacted). `veo_run` holds a `&dyn VideoService`, so the plan, the ledger, the budget, the
//! review and the bake are the same for both services.
//!
//! # Invariants
//! - [`VideoService::request_as_sent`] is the single source of the prompt text and the
//!   parameters both sent and written to the ledger's `request_started` line: what the ledger
//!   says was asked is what was asked.
//! - [`VideoService::resume`] polls only an operation of its own service; an operation of the
//!   other service is refused before anything is sent ([`BakerError::ResumeOnOtherService`]).

use crate::error::BakerError;
use crate::ledger::{ApiMode, OperationOutcome};
use crate::omni_client::is_omni_model;
use crate::veo_client::{GenerationImages, VeoClient, VeoOperation, VeoParameters};
use std::path::Path;

/// A video-generation service a paid run can use. See the module docs.
pub trait VideoService {
    /// The model id every ledger line of the run records.
    fn model(&self) -> &str;

    /// The API the key selects.
    fn mode(&self) -> ApiMode;

    /// Whether requests may carry an end frame and a seed (Vertex AI only).
    fn accepts_end_frame_and_seed(&self) -> bool;

    /// The ffprobe executable that validates downloads.
    fn ffprobe_path(&self) -> &Path;

    /// `text` with the API key replaced.
    fn redact(&self, text: &str) -> String;

    /// The prompt text and parameters a request for `prompt` with `images` and the clip's
    /// `parameters` sends, as the ledger records them.
    ///
    /// # Errors
    /// [`BakerError::InvalidArgument`] for parameters the service cannot honour; nothing is
    /// sent.
    fn request_as_sent(
        &self,
        prompt: &str,
        images: GenerationImages<'_>,
        parameters: &VeoParameters,
    ) -> Result<(String, VeoParameters), BakerError>;

    /// Starts one paid generation with the text and parameters of
    /// [`Self::request_as_sent`].
    ///
    /// # Errors
    /// The service's start errors, including [`BakerError::VeoQuotaExhausted`] for a quota
    /// refusal, which cost nothing.
    fn start(
        &self,
        prompt_as_sent: &str,
        images: GenerationImages<'_>,
        parameters_as_sent: &VeoParameters,
    ) -> Result<VeoOperation, BakerError>;

    /// Polls a started operation until it finishes.
    ///
    /// # Errors
    /// Timeouts, exhausted transient retries and permanent HTTP errors.
    fn poll(&self, operation: &str) -> Result<VeoOperation, BakerError>;

    /// Polls an operation the ledger recorded as started on `model`.
    ///
    /// # Errors
    /// [`BakerError::ResumeOnOtherService`] when `model` belongs to the other service, and the
    /// errors of [`Self::poll`].
    fn resume(&self, model: &str, operation: &str) -> Result<VeoOperation, BakerError>;

    /// What a finished operation returned, for the ledger.
    fn classify(&self, operation: &VeoOperation) -> OperationOutcome;

    /// The operation as redacted JSON, with inline media replaced by its length, for the
    /// record kept beside the clip.
    fn record(&self, operation: &VeoOperation) -> serde_json::Value;

    /// Writes the finished operation's video to `destination_mp4` after validating it.
    ///
    /// # Errors
    /// [`BakerError::VeoApiError`] when the operation holds no video, a download or
    /// validation error, or [`BakerError::Io`].
    fn save_video(
        &self,
        operation: &VeoOperation,
        destination_mp4: &Path,
    ) -> Result<(), BakerError>;
}

impl VideoService for VeoClient {
    fn model(&self) -> &str {
        VeoClient::model(self)
    }

    fn mode(&self) -> ApiMode {
        VeoClient::mode(self)
    }

    fn accepts_end_frame_and_seed(&self) -> bool {
        self.is_vertex_ai()
    }

    fn ffprobe_path(&self) -> &Path {
        VeoClient::ffprobe_path(self)
    }

    fn redact(&self, text: &str) -> String {
        VeoClient::redact(self, text)
    }

    fn request_as_sent(
        &self,
        prompt: &str,
        _images: GenerationImages<'_>,
        parameters: &VeoParameters,
    ) -> Result<(String, VeoParameters), BakerError> {
        Ok((prompt.to_string(), parameters.clone()))
    }

    fn start(
        &self,
        prompt_as_sent: &str,
        images: GenerationImages<'_>,
        parameters_as_sent: &VeoParameters,
    ) -> Result<VeoOperation, BakerError> {
        self.initiate_generation_with(prompt_as_sent, images, parameters_as_sent)
    }

    fn poll(&self, operation: &str) -> Result<VeoOperation, BakerError> {
        self.poll_operation(operation)
    }

    fn resume(&self, model: &str, operation: &str) -> Result<VeoOperation, BakerError> {
        if is_omni_model(model) {
            return Err(BakerError::ResumeOnOtherService {
                operation: operation.to_string(),
                model: model.to_string(),
                client_model: VeoClient::model(self).to_string(),
            });
        }
        self.clone().with_model(model).poll_operation(operation)
    }

    fn classify(&self, operation: &VeoOperation) -> OperationOutcome {
        self.classify_operation(operation)
    }

    fn record(&self, operation: &VeoOperation) -> serde_json::Value {
        self.operation_record(operation)
    }

    fn save_video(
        &self,
        operation: &VeoOperation,
        destination_mp4: &Path,
    ) -> Result<(), BakerError> {
        self.save_operation_video(operation, destination_mp4)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::veo_client::VeoCredentials;

    fn vertex_client() -> VeoClient {
        VeoClient::with_credentials(VeoCredentials {
            api_key: "AQ.test-key".to_string(),
            project_id: Some("project".to_string()),
        })
    }

    #[test]
    fn veo_sends_and_records_the_prompt_and_parameters_unchanged() {
        let client = vertex_client();
        let parameters = VeoParameters {
            negative_prompt: Some("stripes".into()),
            ..VeoParameters::default()
        };
        let (text, sent) = client
            .request_as_sent("a walk", GenerationImages::none(), &parameters)
            .expect("invariant: Veo accepts any parameters");
        assert_eq!(text, "a walk");
        assert_eq!(sent, parameters);
    }

    #[test]
    fn veo_refuses_to_resume_an_omni_interaction_before_sending() {
        let client = vertex_client();
        let result = client.resume("gemini-omni-1.2-flash", "interactions/abc");
        assert!(matches!(
            result,
            Err(BakerError::ResumeOnOtherService { model, .. }) if model == "gemini-omni-1.2-flash"
        ));
    }
}
