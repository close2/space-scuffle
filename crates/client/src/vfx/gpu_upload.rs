//! Render-world upload of each combat effect's live shader data into its pooled material slot.
//!
//! # Responsibility
//! This module owns the only path by which a combat effect's per-frame shader data reaches the
//! GPU. The main world writes a [`CombatFxInstance`]; during extraction,
//! [`upload_combat_fx_uniforms_system`] copies each changed instance into the uniform buffer Bevy
//! created for binding 0 of the effect's prepared [`CombatFxMaterial`].
//!
//! # Why assets are not mutated
//! `Assets::get_mut` raises `AssetEvent::Modified`, and Bevy answers it by preparing the material
//! again: a new uniform buffer and a new bind group. Effects age every frame, so they bypass the
//! asset pipeline after a slot is created, the same way the smog field does
//! (`smog::gpu_upload`, finding C-2).
//!
//! # Invariants
//! - Only instances changed since the last extraction are written, so a paused frame writes nothing.
//! - A slot whose prepared material does not exist yet is skipped. Such a slot was created this
//!   frame with the effect's data as its initial contents, and the effect's next change is written
//!   on a later frame.
//! - The buffer written is the one Bevy created for binding 0, so bind groups never change.

use super::{CombatFxInstance, CombatFxMaterial};
use bevy::pbr::PreparedMaterial;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{Buffer, OwnedBindingResource};
use bevy::render::renderer::RenderQueue;
use bevy::render::Extract;

/// Binding index of the uniform declared on [`CombatFxMaterial`].
const UNIFORM_BINDING: u32 = 0;

/// Changed effects, read from the main world during extraction.
type ChangedEffects<'w, 's> = Query<
    'w,
    's,
    (
        &'static CombatFxInstance,
        &'static MeshMaterial3d<CombatFxMaterial>,
    ),
    Changed<CombatFxInstance>,
>;

/// Writes the shader data of every changed combat effect into its slot's GPU uniform buffer.
///
/// Runs in the render app's `ExtractSchedule`, so it reads the main world's components in place.
// PERFORMANCE RATIONALE:
// Why: Re-preparing a material per live effect per frame created one uniform buffer and one bind
// group each time (finding C-17), up to several hundred per frame in a large fight.
// What: Extraction writes the 32 changed bytes of each effect straight into the slot's existing
// buffer with `Queue::write_buffer`, iterating the query in place with no allocation here.
// Trade-off: One small staged write per changed effect per frame, and the asset's CPU copy of the
// uniform no longer follows the effect after its slot's first use.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub(crate) fn upload_combat_fx_uniforms_system(
    effects: Extract<'_, '_, ChangedEffects<'_, '_>>,
    materials: Res<'_, RenderAssets<PreparedMaterial<CombatFxMaterial>>>,
    queue: Res<'_, RenderQueue>,
) {
    for (instance, material) in &effects {
        // A slot created this frame is prepared after extraction, already holding this data.
        let Some(prepared) = materials.get(material.0.id()) else {
            continue;
        };
        let Some(buffer) = uniform_buffer_of(prepared) else {
            error!("Combat effect material binding {UNIFORM_BINDING} is not the uniform buffer");
            return;
        };
        queue.write_buffer(buffer, 0, bytemuck::bytes_of(&instance.uniform));
    }
}

/// Returns the buffer Bevy created for binding 0 of the prepared material.
fn uniform_buffer_of(material: &PreparedMaterial<CombatFxMaterial>) -> Option<&Buffer> {
    material
        .bindings
        .iter()
        .find_map(|(index, resource)| match resource {
            OwnedBindingResource::Buffer(buffer) if *index == UNIFORM_BINDING => Some(buffer),
            _ => None,
        })
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed test is a panic; each message states the invariant it relies on"
)]
mod tests {
    use super::super::CombatFxUniformGpu;
    use bevy::render::render_resource::encase::UniformBuffer;

    /// The upload writes `bytemuck` bytes into a buffer Bevy filled with `encase`'s std140
    /// layout, so the two encodings must agree byte for byte.
    #[test]
    fn the_uploaded_bytes_match_the_bind_group_layout() {
        let uniform = CombatFxUniformGpu {
            primary_color: bevy::math::Vec4::new(0.25, 0.5, 0.75, 1.5),
            age: 0.375,
            fx_kind: 5,
            seed: 0.125,
            aspect: 3.0,
        };
        let mut encoded = UniformBuffer::new(Vec::<u8>::new());
        encoded
            .write(&uniform)
            .expect("invariant: a 32-byte uniform encodes into a growable vector");
        assert_eq!(encoded.into_inner(), bytemuck::bytes_of(&uniform));
    }
}
