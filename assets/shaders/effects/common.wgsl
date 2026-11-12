// Shared functions and records of the effect renderer (plan 2026-11-09-effect-visualization
// §5.1): ramp lookup, noise breakup and erosion step, soft depth, flipbook frame blend and
// premultiplication.
// The records match client::effects::instances byte for byte.
#define_import_path space_scuffle::effects::common

struct SpriteInstance {
    center: vec3<f32>,
    half_size: f32,
    axis: vec3<f32>,
    stretch: f32,
    layer_a: u32,
    layer_b: u32,
    frame_mix: f32,
    ramp_row: u32,
    intensity: f32,
    erosion: f32,
    soft_depth: f32,
    ring_inner: f32,
    rotation: f32,
    orient: u32,
    ramp_scale: f32,
    flags: u32,
    noise_offset_x: f32,
    noise_offset_y: f32,
    noise_scale: f32,
    noise_amount: f32,
    clip_axis_x: f32,
    clip_axis_z: f32,
    clip_min: f32,
    clip_max: f32,
    softness: f32,
    shade: f32,
    noise_layer: u32,
    shape: u32,
    strip_repeats: f32,
    strip_offset: f32,
    taper_tail: f32,
    taper_head: f32,
    front_edge: f32,
    front_seed: u32,
    // With FLAG_ARC, the share of the turn a ring draws.
    arc: f32,
    // A distortion disc's lens bend towards its rim.
    lens: f32,
}

struct DecalInstance {
    center: vec3<f32>,
    yaw: f32,
    half_extents: vec3<f32>,
    grazing: f32,
    layer_a: u32,
    layer_b: u32,
    frame_mix: f32,
    ramp_row: u32,
    intensity: f32,
    erosion: f32,
    ramp_scale: f32,
    clip_min: f32,
    clip_max: f32,
    noise_offset_x: f32,
    noise_offset_y: f32,
    noise_scale: f32,
    noise_amount: f32,
    softness: f32,
    shade: f32,
    noise_layer: u32,
    clip_axis_x: f32,
    clip_axis_z: f32,
    clip_feather: f32,
    relief: f32,
    sheen: f32,
    gloss: f32,
    value_noise: f32,
    dry_below: f32,
    outline_noise: f32,
    outline_fill: f32,
    padding_a: f32,
    padding_b: f32,
}

const ORIENT_CAMERA: u32 = 0u;
const ORIENT_GROUND: u32 = 1u;
const ORIENT_VERTICAL: u32 = 2u;
const ORIENT_STRETCHED: u32 = 3u;
const ORIENT_UPRIGHT: u32 = 4u;
const ORIENT_RIBBON: u32 = 5u;
const ORIENT_RIBBON_GROUND: u32 = 6u;

const SHAPE_TEXTURE: u32 = 0u;
const SHAPE_RADIAL: u32 = 1u;
const SHAPE_FRONT: u32 = 2u;

// SpriteInstance::flags bits.
const FLAG_MIRROR: u32 = 1u;
const FLAG_POLAR: u32 = 2u;
const FLAG_ARC: u32 = 4u;


// Colour of `ramps` row `row` at luminance `x` (0..1), linearly interpolated between texels.
// The ramp atlas is Rgba32Float and not filterable, so the two texels are loaded and mixed.
fn ramp_lookup(ramps: texture_2d<f32>, row: u32, x: f32) -> vec4<f32> {
    let width = textureDimensions(ramps).x;
    let position = clamp(x, 0.0, 1.0) * f32(width - 1u);
    let low = u32(floor(position));
    let high = min(low + 1u, width - 1u);
    let a = textureLoad(ramps, vec2<i32>(i32(low), i32(row)), 0);
    let b = textureLoad(ramps, vec2<i32>(i32(high), i32(row)), 0);
    return mix(a, b, position - floor(position));
}

// Survival of a texel of erosion order `order` (low erodes first) under `threshold` (0 keeps
// everything, 1 erodes everything), with a soft step `softness` wide so the edge feathers
// instead of stepping. Without breakup noise a zero threshold keeps everything; with it the
// noise already shapes the edge at a zero threshold.
fn erosion_step(order: f32, threshold: f32, softness: f32, noisy: bool) -> f32 {
    if (threshold <= 0.0 && !noisy) {
        return 1.0;
    }
    return smoothstep(threshold, threshold + max(softness, 1e-3), order);
}

// The erosion order of a texel whose baked order is `order` with breakup noise `noise` mixed
// in by `amount` (0 the baked order only, 1 the noise only).
fn breakup_order(order: f32, noise: f32, amount: f32) -> f32 {
    return mix(order, noise, clamp(amount, 0.0, 1.0));
}

// Shading darkened in thin parts: `shading` scaled towards `shading * density` by `shade`, so
// a volume's dense middle stays lit and its thin rim falls into shadow instead of glowing.
fn self_shade(shading: f32, density: f32, shade: f32) -> f32 {
    return shading * mix(1.0, clamp(density, 0.0, 1.0), clamp(shade, 0.0, 1.0));
}

// First channel of noise layer `layer` at `uv` (tiling; the sampler repeats), at mip `level`.
fn noise_sample(
    atlas: texture_2d_array<f32>,
    atlas_sampler: sampler,
    uv: vec2<f32>,
    layer: u32,
    level: f32,
) -> f32 {
    return textureSampleLevel(atlas, atlas_sampler, uv, i32(layer), level).r;
}

// Fade across a clip window [`low`, `high`] of a coordinate `d`, soft over `edge` world units.
fn clip_fade(d: f32, low: f32, high: f32, edge: f32) -> f32 {
    return smoothstep(low, low + edge, d) * (1.0 - smoothstep(high - edge, high, d));
}

// Mip level of a texture `cell` texels wide sampled at `uv` whose screen derivatives are `dx`
// and `dy`.
fn derivative_mip_level(cell: f32, dx: vec2<f32>, dy: vec2<f32>) -> f32 {
    let footprint = max(length(dx), length(dy)) * cell;
    return max(log2(max(footprint, 1e-6)), 0.0);
}

// View-space z (negative in front of the camera) of normalized device depth `ndc_depth`
// under `clip_from_view`, for perspective and orthographic projections.
fn view_z_from_ndc(ndc_depth: f32, clip_from_view: mat4x4<f32>) -> f32 {
    if (clip_from_view[3][3] == 1.0) {
        return (ndc_depth - clip_from_view[3][2]) / clip_from_view[2][2];
    }
    return -clip_from_view[3][2] / (ndc_depth + clip_from_view[2][2]);
}

// Soft-depth fade of a fragment at view z `fragment_z` in front of scene depth `scene_ndc`:
// 0 where it meets the scene, 1 at `distance` world units in front. A zero distance or an
// empty scene pixel (reverse-Z depth 0) does not fade.
fn soft_depth_fade(scene_ndc: f32, fragment_z: f32, distance: f32, clip_from_view: mat4x4<f32>) -> f32 {
    if (distance <= 0.0 || scene_ndc <= 0.0) {
        return 1.0;
    }
    let scene_z = view_z_from_ndc(scene_ndc, clip_from_view);
    return clamp((fragment_z - scene_z) / distance, 0.0, 1.0);
}

// The texel at `uv` of a flipbook: layer `layer_a` blended toward `layer_b` by `frame_mix`,
// sampled at mip `level`.
fn flipbook_sample(
    atlas: texture_2d_array<f32>,
    atlas_sampler: sampler,
    uv: vec2<f32>,
    layer_a: u32,
    layer_b: u32,
    frame_mix: f32,
    level: f32,
) -> vec4<f32> {
    let a = textureSampleLevel(atlas, atlas_sampler, uv, i32(layer_a), level);
    let b = textureSampleLevel(atlas, atlas_sampler, uv, i32(layer_b), level);
    return mix(a, b, frame_mix);
}

// Premultiplied output of ramp colour `ramp` at coverage `coverage`: the colour scaled by the
// coverage, opacity by the ramp's opacity (0 adds light, 1 covers what is behind).
fn premultiply(ramp: vec4<f32>, coverage: f32) -> vec4<f32> {
    return vec4<f32>(ramp.rgb * coverage, ramp.a * coverage);
}

// Mip level for a card `pixels` projected pixels wide sampled from cells `cell` texels wide.
fn card_mip_level(cell: f32, pixels: f32) -> f32 {
    return max(log2(cell / max(pixels, 1.0)), 0.0);
}
