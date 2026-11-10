//! The per-instance records the effect renderer uploads: one card or one decal box each.
//!
//! # Responsibility
//! Defines [`SpriteInstance`] (sprites, rings and particles share it) and [`DecalInstance`],
//! byte-for-byte the WGSL structs of `assets/shaders/effects/common.wgsl`, and the
//! orientation codes the sprite shader understands.
//!
//! # How it fits
//! The runtime fills retained vectors of these every frame; extraction copies them into the
//! retained GPU storage buffers in place.
//!
//! # Invariants
//! - Layouts match WGSL's storage layout: every `[f32; 3]` is followed by one 4-byte scalar,
//!   and each struct is a multiple of 16 bytes (checked by unit tests).

use bytemuck::{Pod, Zeroable};

/// The card faces the camera.
pub const ORIENT_CAMERA: u32 = 0;
/// The card lies on the ground plane.
pub const ORIENT_GROUND: u32 = 1;
/// The card stands upright with its width along `axis`.
pub const ORIENT_VERTICAL: u32 = 2;
/// The card faces the camera with its length along `axis`, stretched by `stretch`.
pub const ORIENT_STRETCHED: u32 = 3;

/// One card: a sprite, a ring or a particle.
// PERFORMANCE RATIONALE:
// Why: Thousands of cards are written and uploaded every frame; one draw per run of cards
// needs them in one contiguous GPU array the vertex shader indexes by instance.
// What: A `Pod` record of five 16-byte rows matching the WGSL struct, written into a retained
// vector and copied to the storage buffer with one `write_buffer`.
// Trade-off: Fields are packed for alignment rather than grouped by meaning, and orientation
// and shape are codes rather than an enum.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default, Pod, Zeroable)]
pub struct SpriteInstance {
    /// Centre in world space.
    pub center: [f32; 3],
    /// Half the card's height in world units (half its width before stretching).
    pub half_size: f32,
    /// Direction for `ORIENT_VERTICAL` and `ORIENT_STRETCHED` (world space, unit or zero).
    pub axis: [f32; 3],
    /// Length factor along `axis` for `ORIENT_STRETCHED` (1 elsewhere).
    pub stretch: f32,
    /// Texture array layer of the current flipbook frame.
    pub layer_a: u32,
    /// Layer of the next frame.
    pub layer_b: u32,
    /// Blend from `layer_a` to `layer_b`, 0..1.
    pub frame_mix: f32,
    /// Row of the ramp atlas.
    pub ramp_row: u32,
    /// Opacity and brightness factor (curve times retirement fade).
    pub intensity: f32,
    /// Erosion threshold 0..1 (0 none).
    pub erosion: f32,
    /// Soft-depth distance in world units (0 is a hard intersection).
    pub soft_depth: f32,
    /// Inner radius of a ring as a fraction of the card's half size; 0 for a plain card.
    pub ring_inner: f32,
    /// Roll in radians.
    pub rotation: f32,
    /// One of the `ORIENT_*` codes.
    pub orient: u32,
    /// Factor on the texture's shading before the ramp lookup (particles cool over life).
    pub ramp_scale: f32,
    /// Unused; keeps the record a multiple of 16 bytes.
    pub padding: f32,
}

/// One decal box.
// PERFORMANCE RATIONALE:
// Why: As for cards: decals are drawn by one instanced draw from a retained storage buffer.
// What: A `Pod` record of four 16-byte rows matching the WGSL struct.
// Trade-off: Two padding words.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default, Pod, Zeroable)]
pub struct DecalInstance {
    /// Box centre in world space.
    pub center: [f32; 3],
    /// Rotation about world up, radians.
    pub yaw: f32,
    /// Half extents of the box (x, up, z), world units.
    pub half_extents: [f32; 3],
    /// Upward-normal cosine below which the mark fades out.
    pub grazing: f32,
    /// Texture array layer of the current frame.
    pub layer_a: u32,
    /// Layer of the next frame.
    pub layer_b: u32,
    /// Blend between the two.
    pub frame_mix: f32,
    /// Row of the ramp atlas.
    pub ramp_row: u32,
    /// Opacity factor.
    pub intensity: f32,
    /// Erosion threshold 0..1.
    pub erosion: f32,
    /// Unused.
    pub padding: [f32; 2],
}

/// A point light requested this frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LightInstance {
    /// Position in world space.
    pub position: [f32; 3],
    /// Linear RGB colour.
    pub color: [f32; 3],
    /// Intensity in lumens (curve applied).
    pub intensity: f32,
    /// Range in world units.
    pub range: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_whole_sixteen_byte_rows() {
        assert_eq!(size_of::<SpriteInstance>(), 80);
        assert_eq!(size_of::<DecalInstance>(), 64);
        assert_eq!(std::mem::offset_of!(SpriteInstance, axis), 16);
        assert_eq!(std::mem::offset_of!(SpriteInstance, layer_a), 32);
        assert_eq!(std::mem::offset_of!(SpriteInstance, intensity), 48);
        assert_eq!(std::mem::offset_of!(SpriteInstance, rotation), 64);
        assert_eq!(std::mem::offset_of!(DecalInstance, half_extents), 16);
        assert_eq!(std::mem::offset_of!(DecalInstance, layer_a), 32);
        assert_eq!(std::mem::offset_of!(DecalInstance, intensity), 48);
    }
}
