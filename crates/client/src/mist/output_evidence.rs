//! Capture-only evidence of the real compiled mist material's output contract.
//!
//! The render observer records the successful pipeline descriptor once, sharing it
//! with screenshot callbacks. Shader arithmetic tests do not substitute for this
//! actual target/define/blend inspection or for native images.

use bevy::prelude::*;
use bevy::render::render_resource::{BlendState, RenderPipelineDescriptor, ShaderDefVal};
use bevy::render::view::ViewTarget;
use serde::Serialize;
use std::sync::{Arc, Mutex};

/// Shares successful material-pipeline metadata between render inspection and image callbacks.
#[derive(Clone, Resource)]
pub(super) struct OutputEvidence {
    hdr: bool,
    required: bool,
    observed: Arc<Mutex<Option<DescriptorEvidence>>>,
}

#[derive(Serialize)]
struct DescriptorEvidence {
    shader_definitions: Vec<String>,
    target_format: String,
    blend: String,
    samples: u32,
    depth_write: bool,
}

/// Explicit failure to establish or persist the captured output contract.
#[derive(Debug, thiserror::Error)]
pub(super) enum EvidenceError {
    #[error("mist output evidence lock poisoned")]
    /// Shared metadata cannot be read consistently after a panicking lock owner.
    Poisoned,
    #[error("mist pipeline has no fragment color target")]
    /// The inspected descriptor contains no fragment color attachment.
    MissingTarget,
    #[error(
        "actual mist pipeline does not match HDR={hdr}, premultiplied blend and read-only depth"
    )]
    /// Actual attachment, shader branch, blend or depth writes contradict the requested path.
    Contract { hdr: bool },
    #[error("no successful actual mist pipeline was recorded before capture")]
    /// A required mist capture has no successfully inspected material pipeline.
    MissingPipeline,
    #[error("serializing actual mist output evidence: {0}")]
    /// RON encoding of actual descriptor evidence failed.
    Serialize(#[from] ron::Error),
    #[error("writing actual mist output evidence: {0}")]
    /// Writing the evidence artifact failed.
    Write(#[from] std::io::Error),
}

impl OutputEvidence {
    /// Creates capture evidence for a requested camera path; `required` rejects absent mist pipelines.
    pub(super) fn new(hdr: bool, required: bool) -> Self {
        Self {
            hdr,
            required,
            observed: Arc::new(Mutex::new(None)),
        }
    }

    /// Validates and retains the first successfully compiled mist descriptor.
    ///
    /// Returns an explicit error for inaccessible metadata or an incompatible output contract.
    pub(super) fn observe(
        &self,
        descriptor: &RenderPipelineDescriptor,
    ) -> Result<(), EvidenceError> {
        let mut observed = self.observed.lock().map_err(|_| EvidenceError::Poisoned)?;
        if observed.is_some() {
            return Ok(());
        }
        let fragment = descriptor
            .fragment
            .as_ref()
            .ok_or(EvidenceError::MissingTarget)?;
        let target = fragment
            .targets
            .first()
            .and_then(Option::as_ref)
            .ok_or(EvidenceError::MissingTarget)?;
        let tonemap = fragment.shader_defs.iter().any(|def| match def {
            ShaderDefVal::Bool(name, value) => name == "TONEMAP_IN_SHADER" && *value,
            _ => false,
        });
        let depth_write = descriptor
            .depth_stencil
            .as_ref()
            .is_some_and(|state| state.depth_write_enabled);
        if tonemap == self.hdr
            || (target.format == ViewTarget::TEXTURE_FORMAT_HDR) != self.hdr
            || (!self.hdr && !target.format.is_srgb())
            || target.blend != Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING)
            || depth_write
        {
            return Err(EvidenceError::Contract { hdr: self.hdr });
        }
        *observed = Some(DescriptorEvidence {
            shader_definitions: fragment
                .shader_defs
                .iter()
                .map(|def| format!("{def:?}"))
                .collect(),
            target_format: format!("{:?}", target.format),
            blend: format!("{:?}", target.blend),
            samples: descriptor.multisample.count,
            depth_write,
        });
        Ok(())
    }

    /// Writes actual descriptor metadata into the capture directory.
    ///
    /// Returns an error if required evidence is absent, poisoned, or cannot be serialized/written.
    pub(super) fn write(&self, directory: &std::path::Path) -> Result<(), EvidenceError> {
        let observed = self.observed.lock().map_err(|_| EvidenceError::Poisoned)?;
        if self.required && observed.is_none() {
            return Err(EvidenceError::MissingPipeline);
        }
        std::fs::write(
            directory.join("mist-output-pipeline.ron"),
            ron::ser::to_string_pretty(&*observed, ron::ser::PrettyConfig::default())?,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::*;

    fn descriptor(hdr: bool) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            fragment: Some(FragmentState {
                shader: Handle::default(),
                shader_defs: if hdr {
                    vec![]
                } else {
                    vec!["TONEMAP_IN_SHADER".into()]
                },
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: if hdr {
                        ViewTarget::TEXTURE_FORMAT_HDR
                    } else {
                        TextureFormat::Rgba8UnormSrgb
                    },
                    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            label: None,
            layout: vec![],
            push_constant_ranges: vec![],
            vertex: VertexState {
                shader: Handle::default(),
                shader_defs: vec![],
                entry_point: "vertex".into(),
                buffers: vec![],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            zero_initialize_workgroup_memory: true,
        }
    }

    #[test]
    fn metadata_admission_rejects_wrong_output_contract_and_missing_pipeline(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        for hdr in [false, true] {
            let evidence = OutputEvidence::new(hdr, true);
            assert!(matches!(
                evidence.write(directory.path()),
                Err(EvidenceError::MissingPipeline)
            ));
            assert!(matches!(
                evidence.observe(&descriptor(!hdr)),
                Err(EvidenceError::Contract { .. })
            ));
            let mut wrong_blend = descriptor(hdr);
            let fragment = wrong_blend
                .fragment
                .as_mut()
                .ok_or("fixture fragment absent")?;
            let target = fragment.targets[0]
                .as_mut()
                .ok_or("fixture target absent")?;
            target.blend = Some(BlendState::ALPHA_BLENDING);
            assert!(matches!(
                evidence.observe(&wrong_blend),
                Err(EvidenceError::Contract { .. })
            ));
            evidence.observe(&descriptor(hdr))?;
            evidence.write(directory.path())?;
            assert!(directory.path().join("mist-output-pipeline.ron").exists());
        }
        Ok(())
    }
}
