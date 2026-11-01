//! Custom Bevy 3D Volumetric Material and GPU uniform packing for 3D Bubble-Driven Smog.
//!
//! # Architectural Responsibilities
//! This module binds CPU simulation state and 3D fluid grid textures into the GPU-visible
//! [`SmogVolumeMaterial`] pipeline. It packs active bubble positions, radii, and velocities
//! directly into [`SmogVolumeUniformGpu`] for analytical sub-voxel cavity carving and bow-wave
//! rim compression during volumetric raymarching.
//!
//! # Invariants
//! - [`SmogVolumeUniformGpu`] memory layout is strictly 608 bytes and 16-byte aligned matching `smog_volume.wgsl`.
//! - All fields are strictly finite; zero NaN or Inf values are committed to GPU uniform buffers.
//! - Front-face culling is specialized so back-faces rasterize each covered pixel exactly once whether the camera
//!   is inside or outside the volume bounding box.

use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Face, ShaderRef, ShaderType};

/// Maximum number of active bubble spheres packed directly into the volumetric shader uniform
/// for sub-voxel spherical cavity carving and bow-wave compression.
pub const MAX_SMOG_SHADER_BUBBLES: usize = 16;

/// Marker component identifying the 3D volumetric smog bounding box entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(NotShadowCaster)]
pub struct SmogVolumeMarker;

// PERFORMANCE RATIONALE:
// Why: WebGPU uniform buffers require strict 16-byte alignment and std140 array stride rules.
// What: 6 header Vec4 lanes (96B) + 16 bubble_pos_radius Vec4s (256B) + 16 bubble_vel_boost Vec4s (256B)
// = 608 bytes total, with every field 16-byte aligned.
// Trade-off: Fixed 16-bubble analytical sub-voxel array alongside arbitrary-bubble 3D texture grid.
/// GPU uniform buffer struct matching `struct SmogVolumeUniform` in `assets/shaders/smog_volume.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
pub struct SmogVolumeUniformGpu {
    /// World-space minimum corner of the 3D smog volume (XYZ) + soft depth fade distance (W).
    pub volume_min: Vec4,
    /// World-space maximum corner of the 3D smog volume (XYZ) + global extinction scale (W).
    pub volume_max: Vec4,
    /// Base ambient smog tint (RGB) + animation time in seconds (W).
    pub base_tint_time: Vec4,
    /// Warm scattering highlight tint (RGB) + Henyey-Greenstein phase anisotropy g (W).
    pub warm_tint_anisotropy: Vec4,
    /// Primary directional light direction (XYZ) + normalized light intensity (W).
    pub light_dir_intensity: Vec4,
    /// Control parameters: X = active bubble count, Y = bow-wave rim strength, Z = cavity strength, W = enabled.
    pub control_params: Vec4,
    /// XYZ = bubble world center, W = bubble radius (for up to 16 bubbles).
    pub bubble_pos_radius: [Vec4; MAX_SMOG_SHADER_BUBBLES],
    /// XYZ = bubble velocity vector, W = speed boost / wake intensity factor.
    pub bubble_vel_boost: [Vec4; MAX_SMOG_SHADER_BUBBLES],
}

impl Default for SmogVolumeUniformGpu {
    fn default() -> Self {
        Self {
            volume_min: Vec4::new(-6.5, 0.0, -4.5, 0.4),
            volume_max: Vec4::new(6.5, 4.0, 4.5, 1.15),
            base_tint_time: Vec4::new(0.28, 0.46, 0.68, 0.0),
            warm_tint_anisotropy: Vec4::new(0.95, 0.85, 0.70, 0.45),
            light_dir_intensity: Vec4::new(0.577, 0.577, 0.577, 1.0),
            control_params: Vec4::new(0.0, 1.25, 1.0, 1.0),
            bubble_pos_radius: [Vec4::ZERO; MAX_SMOG_SHADER_BUBBLES],
            bubble_vel_boost: [Vec4::ZERO; MAX_SMOG_SHADER_BUBBLES],
        }
    }
}

/// Custom Bevy 3D material implementing the volumetric raymarching smog pipeline.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SmogVolumeMaterial {
    /// Volumetric uniform parameters and packed bubble sphere states.
    #[uniform(0)]
    pub uniform: SmogVolumeUniformGpu,
    /// 3D Texture carrying volumetric density and RGB chromatic tint.
    #[texture(1, dimension = "3d")]
    #[sampler(2)]
    pub density_texture: Handle<Image>,
    /// 3D Texture carrying 3D fluid velocity vector and vorticity magnitude.
    #[texture(3, dimension = "3d")]
    #[sampler(4)]
    pub velocity_texture: Handle<Image>,
    /// 3D Tileable Wispy Noise Texture (32x32x32 Rgba8Unorm, Repeat sampler) for O(1) filament detail.
    #[texture(5, dimension = "3d")]
    #[sampler(6)]
    pub noise_texture: Handle<Image>,
}

impl Material for SmogVolumeMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/smog_volume.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline<Self>,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::render::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = Some(Face::Front);
        if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
            depth_stencil.depth_write_enabled = false;
            depth_stencil.depth_compare = bevy::render::render_resource::CompareFunction::Always;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smog_volume_uniform_gpu_size_and_alignment() {
        assert_eq!(
            size_of::<SmogVolumeUniformGpu>(),
            608,
            "SmogVolumeUniformGpu must be exactly 608 bytes"
        );
        assert_eq!(
            align_of::<SmogVolumeUniformGpu>(),
            16,
            "SmogVolumeUniformGpu must be 16-byte aligned"
        );
    }

    #[test]
    fn test_smog_volume_uniform_defaults() {
        let uniform = SmogVolumeUniformGpu::default();
        assert_eq!(uniform.volume_min, Vec4::new(-6.5, 0.0, -4.5, 0.4));
        assert_eq!(uniform.volume_max, Vec4::new(6.5, 4.0, 4.5, 1.15));
        assert_eq!(uniform.base_tint_time, Vec4::new(0.28, 0.46, 0.68, 0.0));
        assert_eq!(uniform.control_params.w, 1.0);
        assert_eq!(uniform.bubble_pos_radius.len(), MAX_SMOG_SHADER_BUBBLES);
        assert_eq!(uniform.bubble_vel_boost.len(), MAX_SMOG_SHADER_BUBBLES);
    }
}
