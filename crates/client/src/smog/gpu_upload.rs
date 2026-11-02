//! Render-world upload of the animated smog field into persistent GPU resources.
//!
//! # Responsibility
//! This module owns the single path by which per-frame smog state reaches the GPU. The main world
//! publishes a [`SmogGpuFrame`] and fills the byte slabs of [`SmogGrid3d`]; during extraction,
//! [`upload_smog_frame_system`] writes those bytes into the existing 3D textures and uniform buffer.
//!
//! # Why assets are not mutated
//! `Assets::get_mut` emits `AssetEvent::Modified`, which makes Bevy destroy and recreate a texture
//! and rebuild a material's bind group. Smog changes every frame, so it bypasses the asset pipeline
//! after startup: the [`Image`] and [`SmogVolumeMaterial`] assets are never modified again.
//!
//! # Invariants
//! - One upload writes both textures and the uniform, or nothing, so they describe the same step.
//! - An upload is retried until the render assets exist, then runs once per published generation.
//! - The uniform buffer is the one Bevy created for binding 0, so bind groups never change.

use super::{SmogGrid3d, SmogTextures, SmogVolumeMaterial, SmogVolumeUniformGpu, GRID_NX, GRID_NY};
use super::{GRID_NZ, TOTAL_VOXELS};
use bevy::pbr::PreparedMaterial;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    Buffer, Extent3d, ImageCopyTexture, ImageDataLayout, Origin3d, OwnedBindingResource, Texture,
    TextureAspect,
};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::Extract;

/// Bytes per voxel of the `Rgba8Unorm` density and velocity textures.
const BYTES_PER_VOXEL: u32 = 4;

/// Binding index of the uniform declared on [`SmogVolumeMaterial`].
const UNIFORM_BINDING: u32 = 0;

/// Log target of the per-upload debug record, read by the GPU integration test.
pub const SMOG_UPLOAD_LOG_TARGET: &str = "client::smog::gpu_upload";

/// Latest smog uniform and the step it belongs to, published by the main world.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct SmogGpuFrame {
    /// Uniform that binding 0 must hold when the current grid bytes are drawn.
    pub uniform: SmogVolumeUniformGpu,
    /// Count of published smog steps. Zero means the startup asset data is still current.
    pub generation: u64,
}

/// Writes the published smog step into the persistent GPU textures and uniform buffer.
///
/// Runs in `ExtractSchedule` so it can read [`SmogGrid3d`] in place.
// PERFORMANCE RATIONALE:
// Why: Mutating `Assets<Image>` per frame destroyed and recreated two 3D textures (~1.66 MB) and
// rebuilt one bind group every frame, and copying the slabs into the render world would add a
// second per-frame copy.
// What: Extraction already has exclusive main-world access, so `Queue::write_texture` copies the
// slabs straight into wgpu's staging memory, with no allocation or asset event in this crate.
// Trade-off: Extraction blocks the main world for that memcpy, about 1.66 MB per frame.
pub(crate) fn upload_smog_frame_system(
    frame: Extract<'_, '_, Res<'_, SmogGpuFrame>>,
    grid: Extract<'_, '_, Res<'_, SmogGrid3d>>,
    textures: Extract<'_, '_, Option<Res<'_, SmogTextures>>>,
    images: Res<'_, RenderAssets<GpuImage>>,
    materials: Res<'_, RenderAssets<PreparedMaterial<SmogVolumeMaterial>>>,
    queue: Res<'_, RenderQueue>,
    mut uploaded_generation: Local<'_, u64>,
) {
    if frame.generation == *uploaded_generation {
        return;
    }
    let Some(textures) = textures.as_ref() else {
        return;
    };
    // Render assets are prepared after the first extraction, so absence here is a normal wait.
    let (Some(density), Some(velocity), Some(material)) = (
        images.get(&textures.density_image),
        images.get(&textures.velocity_image),
        materials.get(textures.material_handle.id()),
    ) else {
        return;
    };
    let Some(uniform_buffer) = uniform_buffer_of(material) else {
        error!("Smog material binding {UNIFORM_BINDING} is not the uniform buffer");
        return;
    };

    write_grid_texture(&queue, &density.texture, &grid.rgba_density_bytes);
    write_grid_texture(&queue, &velocity.texture, &grid.rgba_velocity_bytes);
    queue.write_buffer(uniform_buffer, 0, bytemuck::bytes_of(&frame.uniform));

    *uploaded_generation = frame.generation;
    debug!(
        target: SMOG_UPLOAD_LOG_TARGET,
        "uploaded smog generation {}", frame.generation
    );
}

/// Returns the buffer Bevy created for binding 0 of the prepared material.
fn uniform_buffer_of(material: &PreparedMaterial<SmogVolumeMaterial>) -> Option<&Buffer> {
    material
        .bindings
        .iter()
        .find_map(|(index, resource)| match resource {
            OwnedBindingResource::Buffer(buffer) if *index == UNIFORM_BINDING => Some(buffer),
            _ => None,
        })
}

/// Replaces the whole contents of one grid-sized `Rgba8Unorm` 3D texture.
fn write_grid_texture(queue: &RenderQueue, texture: &Texture, bytes: &[u8]) {
    debug_assert_eq!(bytes.len(), TOTAL_VOXELS * BYTES_PER_VOXEL as usize);
    queue.write_texture(
        ImageCopyTexture {
            texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        bytes,
        ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(GRID_NX as u32 * BYTES_PER_VOXEL),
            rows_per_image: Some(GRID_NY as u32),
        },
        Extent3d {
            width: GRID_NX as u32,
            height: GRID_NY as u32,
            depth_or_array_layers: GRID_NZ as u32,
        },
    );
}
