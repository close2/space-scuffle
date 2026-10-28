//! Shared premultiplied mist material and its explicitly packed GPU lighting contract.
//!
//! All emitters share persistent buffer bindings. Immutable slot topology identifies
//! samples; complete packet uploads own age, flow, immutable light reference and animation time.

use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef, ShaderType};

/// GPU uniform matching `MistWakeUniform`; each vector occupies one 16-byte lane.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
pub struct MistWakeUniformGpu {
    /// Cool body tint in linear RGB; W is soft-intersection distance in world units.
    pub cool: Vec4,
    /// Warm iridescent ridge tint in linear RGB; W is presentation time in seconds.
    pub warm: Vec4,
    /// Boost accent tint in linear RGB; W is global extinction/opacity.
    pub boost: Vec4,
    /// Immutable reference illuminance in lux in X; YZW are initialized reserved lanes.
    pub light: Vec4,
}

impl Default for MistWakeUniformGpu {
    fn default() -> Self {
        Self {
            cool: Vec4::new(0.4, 0.7, 0.82, 0.4),
            warm: Vec4::new(0.82, 0.64, 0.9, 0.0),
            boost: Vec4::new(1.0, 0.69, 0.32, 0.32),
            light: Vec4::new(super::lighting::default_reference_lux(), 0.0, 0.0, 0.0),
        }
    }
}

/// One shared, depth-tested, premultiplied material for all movement-wake ribbons.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct MistWakeMaterial {
    /// Persistent read-only emitter headers; allocated once for the validated capacity.
    #[storage(0, read_only, buffer)]
    pub headers: bevy::render::render_resource::Buffer,
    /// Persistent read-only chronological world-space history samples.
    #[storage(1, read_only, buffer)]
    pub samples: bevy::render::render_resource::Buffer,
    /// Persistent art/time/geometry frame block, published with the same packet.
    #[storage(2, read_only, buffer)]
    pub frame: bevy::render::render_resource::Buffer,
}

impl Material for MistWakeMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/mist_wake.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/mist_wake.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline<Self>,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        layout: &bevy::render::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout
            .0
            .get_layout(&[super::ribbon::ATTRIBUTE_MIST_VERTEX.at_shader_location(0)])?];
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}
