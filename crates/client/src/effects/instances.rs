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
/// [`SpriteInstance::flags`] bit: the texture is mirrored horizontally.
pub const FLAG_MIRROR: u32 = 1;
/// [`SpriteInstance::flags`] bit: a ring wraps the texture's middle band around itself.
pub const FLAG_POLAR: u32 = 2;
/// The card stands upright and turns about world up to face the camera.
pub const ORIENT_UPRIGHT: u32 = 4;
/// A ribbon: the card faces the camera about its length along `axis` (as
/// [`ORIENT_STRETCHED`]), the texture's u axis runs along that length and repeats, and the
/// strip narrows to a point over its tapers.
pub const ORIENT_RIBBON: u32 = 5;
/// A ribbon lying flat on the ground, its width horizontal across `axis`.
pub const ORIENT_RIBBON_GROUND: u32 = 6;
/// [`SpriteInstance::shape`]: the texture element.
pub const SHAPE_TEXTURE: u32 = 0;
/// [`SpriteInstance::shape`]: a soft radial glow computed in the shader.
pub const SHAPE_RADIAL: u32 = 1;
/// [`SpriteInstance::shape`]: a ring computed as a pressure front from the breakup noise.
pub const SHAPE_FRONT: u32 = 2;
/// `noise_layer` of an instance without breakup noise.
pub const NO_NOISE: u32 = u32::MAX;

/// One card: a sprite, a ring or a particle.
// PERFORMANCE RATIONALE:
// Why: Thousands of cards are written and uploaded every frame; one draw per run of cards
// needs them in one contiguous GPU array the vertex shader indexes by instance.
// What: A `Pod` record of nine 16-byte rows matching the WGSL struct, written into a retained
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
    /// Length factor along `axis` for `ORIENT_STRETCHED`; for `ORIENT_UPRIGHT` and
    /// `ORIENT_VERTICAL` the height of the roll's pivot below the centre, in half sizes (the
    /// point where the card meets the ground); unused elsewhere.
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
    /// `FLAG_*` bits.
    pub flags: u32,
    /// Offset of the breakup noise in noise tiles (the instance's own phase).
    pub noise_offset: [f32; 2],
    /// Noise repeats across the card.
    pub noise_scale: f32,
    /// Weight of the noise in the erosion order, 0..1.
    pub noise_amount: f32,
    /// World XZ direction the card is clipped along; zero for no clip.
    pub clip_axis: [f32; 2],
    /// Smallest `dot(world.xz, clip_axis)` drawn.
    pub clip_min: f32,
    /// Largest `dot(world.xz, clip_axis)` drawn.
    pub clip_max: f32,
    /// Width of the erosion threshold's soft step.
    pub softness: f32,
    /// Self-shading of thin parts, 0..1.
    pub shade: f32,
    /// Texture array layer of the breakup noise, or [`NO_NOISE`].
    pub noise_layer: u32,
    /// One of the `SHAPE_*` codes.
    pub shape: u32,
    /// A ribbon's texture repeats over its drawn length (u along the strip); 0 elsewhere.
    pub strip_repeats: f32,
    /// A ribbon's texture offset along the strip, in repeats (its scroll).
    pub strip_offset: f32,
    /// Share of a ribbon's drawn length over which it narrows at its tail (start).
    pub taper_tail: f32,
    /// Share of a ribbon's drawn length over which it narrows at its head (end).
    pub taper_head: f32,
}

/// One decal box.
// PERFORMANCE RATIONALE:
// Why: As for cards: decals are drawn by one instanced draw from a retained storage buffer.
// What: A `Pod` record of nine 16-byte rows matching the WGSL struct.
// Trade-off: Fields are packed for alignment rather than grouped by meaning, and the noise
// offset is two scalars in WGSL (a `vec2` there would need 8-byte alignment).
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
    /// Factor on the texture's shading before the ramp lookup.
    pub ramp_scale: f32,
    /// Smallest `dot(world.xz, clip_axis)` drawn: a mark clipped to its line ends there.
    pub clip_min: f32,
    /// Largest `dot(world.xz, clip_axis)` drawn.
    pub clip_max: f32,
    /// Offset of the breakup noise in noise tiles (the instance's own phase).
    pub noise_offset: [f32; 2],
    /// Noise repeats across the box.
    pub noise_scale: f32,
    /// Weight of the noise in the erosion order, 0..1.
    pub noise_amount: f32,
    /// Width of the erosion threshold's soft step.
    pub softness: f32,
    /// Self-shading of thin parts, 0..1.
    pub shade: f32,
    /// Texture array layer of the breakup noise, or [`NO_NOISE`].
    pub noise_layer: u32,
    /// World XZ direction the mark is clipped along; zero for no clip.
    pub clip_axis: [f32; 2],
    /// World units over which a clipped mark feathers out, across each clip end (60 % of it beyond the end).
    pub clip_feather: f32,
    /// Liquid relief: slope scale of the thickness field lit as a wet surface; 0 for none.
    pub relief: f32,
    /// Strength of the liquid's specular sheen at full wetness.
    pub sheen: f32,
    /// Specular exponent of the sheen (higher is a tighter highlight).
    pub gloss: f32,
    /// Weight of low-frequency noise in the liquid's value and thickness, 0..1.
    pub value_noise: f32,
    /// Ramp factor at and below which the liquid is dry (no sheen, no relief light); the
    /// sheen grows linearly from there to a ramp factor of 1.
    pub dry_below: f32,
    /// Depth of a disc outline's edge noise as a fraction of the half length; 0 with
    /// `outline_fill` 0 for none (the texture's own outline).
    pub outline_noise: f32,
    /// Share of the texture's half width read across a disc outline's radius; 0 for no disc
    /// outline.
    pub outline_fill: f32,
    /// Padding to a whole 16-byte row; always zero.
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
        assert_eq!(size_of::<SpriteInstance>(), 144);
        assert_eq!(std::mem::offset_of!(SpriteInstance, strip_repeats), 128);
        assert_eq!(size_of::<DecalInstance>(), 144);
        assert_eq!(std::mem::offset_of!(DecalInstance, outline_noise), 128);
        assert_eq!(std::mem::offset_of!(DecalInstance, clip_feather), 104);
        assert_eq!(std::mem::offset_of!(DecalInstance, dry_below), 124);
        assert_eq!(std::mem::offset_of!(DecalInstance, clip_axis), 96);
        assert_eq!(std::mem::offset_of!(SpriteInstance, noise_offset), 80);
        assert_eq!(std::mem::offset_of!(SpriteInstance, clip_axis), 96);
        assert_eq!(std::mem::offset_of!(SpriteInstance, softness), 112);
        assert_eq!(std::mem::offset_of!(DecalInstance, noise_offset), 68);
        assert_eq!(std::mem::offset_of!(DecalInstance, softness), 84);
        assert_eq!(std::mem::offset_of!(DecalInstance, clip_min), 60);
        assert_eq!(std::mem::offset_of!(SpriteInstance, axis), 16);
        assert_eq!(std::mem::offset_of!(SpriteInstance, layer_a), 32);
        assert_eq!(std::mem::offset_of!(SpriteInstance, intensity), 48);
        assert_eq!(std::mem::offset_of!(SpriteInstance, rotation), 64);
        assert_eq!(std::mem::offset_of!(DecalInstance, half_extents), 16);
        assert_eq!(std::mem::offset_of!(DecalInstance, layer_a), 32);
        assert_eq!(std::mem::offset_of!(DecalInstance, intensity), 48);
    }
}
