// Effect decals: boxes projected onto the scene through the depth prepass
// (client::effects::render). Each box is drawn by its back faces with no depth test; the
// fragment reconstructs the scene's world position, keeps it only inside the box, samples the
// texture in the box's local XZ at the mip its screen footprint needs, breaks its erosion up
// with noise of its own phase and fades on faces steeper than the decal's grazing cosine. A
// decal with a disc outline is bounded by a disc of its half length with a shallow noise edge
// and reads its texture's middle inside it (review round 5). The pipeline with `DARKEN_PASS`
// draws only decals with `darken` set and outputs the factor the scene is multiplied by, so such
// a mark darkens the ground, its shadows included, and never lightens it (WP8 review 2); the
// other pipeline draws every other decal.
#import bevy_render::view::View
#import space_scuffle::effects::common::{
    DecalInstance, ramp_lookup, erosion_step, breakup_order, self_shade, noise_sample, clip_fade,
    derivative_mip_level, flipbook_sample, premultiply, darkening_factor, NOWHERE,
}

// Liquid material (DecalInstance::relief > 0): the element's shading is the liquid's
// thickness, varied by low-frequency noise and lit by a key light from the upper left of the
// screen (the units' studio light).
// Texels of the thickness field's finite differences, at the sampled mip.
const LIQUID_STEP_TEXELS: f32 = 1.5;
// Mips above the footprint's at which the relief is read: only broad swells and the thick
// rim catch the light, not every speck of the texture.
const LIQUID_RELIEF_MIPS: f32 = 3.0;
// Pooling noise: this multiple of the breakup noise's scale (a few large pools and swells
// across a stain; review round 4).
const LIQUID_NOISE_SCALE: f32 = 1.0;
// How far the noise moves the ramp input and the thickness (at value_noise 1).
const LIQUID_VALUE_SWING: f32 = 1.3;
const LIQUID_THICKNESS_SWING: f32 = 0.6;
// Diffuse light on the relief: the factor's range around the flat surface's 1.
const LIQUID_DIFFUSE: f32 = 0.9;
const LIQUID_DIFFUSE_MIN: f32 = 0.45;
const LIQUID_DIFFUSE_MAX: f32 = 1.7;
// Share of a clipped mark's end feather that lies beyond the line's end.
const CLIP_FEATHER_OUTSIDE: f32 = 0.6;
// Elevation of the key light over the ground: 30 degrees (its cosine and sine).
const LIQUID_KEY_COS: f32 = 0.8660254;
const LIQUID_KEY_SIN: f32 = 0.5;
// Colour of the specular sheen (wet acid catching a white key light: pale lime-white).
const LIQUID_SHEEN_COLOR: vec3<f32> = vec3<f32>(0.80, 1.00, 0.62);
// Share of the relief light a dry crust keeps (review round 4): the liquid's swells stay
// shaded as it dries; only the specular sheen dries away.
const LIQUID_DRY_RELIEF: f32 = 0.75;
// The relief's strength across the stain, from flat pools to large swells, by the pooling
// noise (review round 4: round 3's relief was uniform, "green coral").
const LIQUID_POOL_RELIEF: f32 = 0.2;
const LIQUID_SWELL_RELIEF: f32 = 1.6;
// The wet rim (review round 4): a band along every edge of the liquid (its outline and the
// edges of the holes it dries into), this wide in erosion order beyond the erosion step,
// darker and more saturated than the body by this colour factor at every age.
const LIQUID_RIM_ORDER: f32 = 0.10;
// The outline's band: the coverage read this many mips above the footprint (blurred over about
// a tenth of a unit at gameplay zooms), darkened where it falls between these two values, so
// the band lies inside the outline where the liquid is still opaque.
const LIQUID_RIM_MIPS: f32 = 3.5;
const LIQUID_RIM_COVERAGE: vec2<f32> = vec2<f32>(0.70, 0.995);
const LIQUID_RIM_DARK: vec3<f32> = vec3<f32>(0.24, 0.44, 0.24);
// The wet rim inside a disc outline (review round 5): this wide, as a fraction of the half
// length, darkest at the edge.
const LIQUID_OUTLINE_RIM: f32 = 0.09;
// Contrast of the disc outline's edge noise around its middle, so the edge uses the whole
// depth `outline_noise` allows instead of the noise's narrow middle range.
const OUTLINE_NOISE_CONTRAST: f32 = 2.0;
// Frequency of the disc outline's edge noise against the breakup noise's scale: many small
// irregularities within the shallow depth, so the edge reads as liquid and not as a smooth
// ellipse with a few soft bulges.
const OUTLINE_NOISE_FREQUENCY: f32 = 3.0;

@group(0) @binding(0) var<uniform> view: View;
#ifdef MULTISAMPLED
@group(0) @binding(1) var depth_prepass: texture_depth_multisampled_2d;
#else
@group(0) @binding(1) var depth_prepass: texture_depth_2d;
#endif
@group(1) @binding(0) var<storage, read> decals: array<DecalInstance>;
@group(1) @binding(1) var atlas: texture_2d_array<f32>;
@group(1) @binding(2) var atlas_sampler: sampler;
@group(1) @binding(3) var ramps: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) instance: u32,
}

// Corner `index` (0..36) of a cube in -1..1, as twelve triangles (corner bits: x 1, y 2, z 4).
fn cube_corner(index: u32) -> vec3<f32> {
    var faces = array<vec3<u32>, 12>(
        vec3<u32>(0u, 2u, 1u), vec3<u32>(1u, 2u, 3u),
        vec3<u32>(4u, 5u, 6u), vec3<u32>(5u, 7u, 6u),
        vec3<u32>(0u, 1u, 4u), vec3<u32>(1u, 5u, 4u),
        vec3<u32>(2u, 6u, 3u), vec3<u32>(3u, 6u, 7u),
        vec3<u32>(0u, 4u, 2u), vec3<u32>(2u, 4u, 6u),
        vec3<u32>(1u, 3u, 5u), vec3<u32>(3u, 7u, 5u),
    );
    let face = faces[index / 3u];
    var corner = face.x;
    if (index % 3u == 1u) {
        corner = face.y;
    } else if (index % 3u == 2u) {
        corner = face.z;
    }
    return vec3<f32>(
        select(-1.0, 1.0, (corner & 1u) != 0u),
        select(-1.0, 1.0, (corner & 2u) != 0u),
        select(-1.0, 1.0, (corner & 4u) != 0u),
    );
}

// `local` rotated by `yaw` about world up.
fn rotate_yaw(local: vec3<f32>, yaw: f32) -> vec3<f32> {
    let c = cos(yaw);
    let s = sin(yaw);
    return vec3<f32>(local.x * c + local.z * s, local.y, -local.x * s + local.z * c);
}

@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput {
    let decal = decals[instance_index];
    let local = cube_corner(vertex_index % 36u) * decal.half_extents;
    let world = decal.center + rotate_yaw(local, decal.yaw);
    var out: VertexOutput;
    out.position = view.clip_from_world * vec4<f32>(world, 1.0);
    out.instance = instance_index;
#ifdef DARKEN_PASS
    if (decal.darken <= 0.0) {
        out.position = NOWHERE;
    }
#else
    if (decal.darken > 0.0) {
        out.position = NOWHERE;
    }
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let decal = decals[in.instance];
    let depth = textureLoad(depth_prepass, vec2<i32>(floor(in.position.xy)), 0);
    let screen_uv = (in.position.xy - view.viewport.xy) / view.viewport.zw;
    let ndc = vec4<f32>(screen_uv.x * 2.0 - 1.0, 1.0 - screen_uv.y * 2.0, depth, 1.0);
    let world_h = view.world_from_clip * ndc;
    let world = world_h.xyz / world_h.w;
    let normal = normalize(cross(dpdy(world), dpdx(world)));
    let local = rotate_yaw(world - decal.center, -decal.yaw) / decal.half_extents;
    // A disc outline (DecalInstance::outline_fill > 0) reads the texture's middle, so the
    // texture's own outline lies outside the disc that bounds the mark.
    let disc = decal.outline_fill > 0.0;
    let uv = local.xz * (0.5 * select(1.0, decal.outline_fill, disc)) + 0.5;
    let radius = length(local.xz);
    let radius_step = max(fwidth(radius), 1e-4);
    // Derivatives in uniform control flow, before anything is discarded: the mip that matches
    // the decal's footprint on screen keeps its eroded edges from stair-stepping.
    let level = derivative_mip_level(f32(textureDimensions(atlas).x), dpdx(uv), dpdy(uv));
    let texel = flipbook_sample(atlas, atlas_sampler, uv, decal.layer_a, decal.layer_b, decal.frame_mix, level);
    let noisy = decal.noise_amount > 0.0;
    var order = texel.b;
    let noise_offset = vec2<f32>(decal.noise_offset_x, decal.noise_offset_y);
    if (noisy) {
        let noise_uv = uv * decal.noise_scale + noise_offset;
        let noise = noise_sample(atlas, atlas_sampler, noise_uv, decal.noise_layer, level + log2(max(decal.noise_scale, 1.0)));
        order = breakup_order(order, noise, decal.noise_amount);
    }
    // The liquid's thickness, slope and pooling noise (nothing is read for a decal that is no liquid).
    let liquid = liquid_surface(decal, uv, texel.r, level, noise_offset, noisy);
    var clip = 1.0;
    let clip_axis = vec2<f32>(decal.clip_axis_x, decal.clip_axis_z);
    if (dot(clip_axis, clip_axis) > 0.0) {
        // Feathered across each clip end: CLIP_FEATHER_OUTSIDE of the fade lies beyond the end,
        // so the mark is still about two thirds strong at the end itself and ends there to
        // the eye, while its tip fades out instead of being cut.
        let outside = CLIP_FEATHER_OUTSIDE * decal.clip_feather;
        clip = clip_fade(
            dot(world.xz, clip_axis),
            decal.clip_min - outside,
            decal.clip_max + outside,
            decal.clip_feather,
        );
    }
    let inside = all(abs(local) <= vec3<f32>(1.0)) && depth > 0.0;
    let grazing = clamp((abs(normal.y) - decal.grazing) / max(1.0 - decal.grazing, 1e-3), 0.0, 1.0);
    let height_fade = 1.0 - smoothstep(0.7, 1.0, abs(local.y));
    let survive = erosion_step(order, decal.erosion, decal.softness, noisy);
    // The disc outline's edge (unbounded without one) and its anti-aliased mask.
    var edge = 2.0;
    if (disc) {
        edge = outline_edge(decal, local.xz, radius, level, noise_offset);
    }
    let outline = 1.0 - smoothstep(edge - radius_step, edge, radius);
    // An intensity above 1 thickens thin parts; the coverage itself never exceeds 1.
    let coverage = min(texel.g * survive * decal.intensity * grazing * height_fade * clip, 1.0) * outline;
    if (!inside || coverage <= 0.0) {
        discard;
    }
    var shading = self_shade(texel.r, texel.g, decal.shade);
    if (decal.relief <= 0.0) {
        let colour = ramp_lookup(ramps, decal.ramp_row, shading * decal.ramp_scale);
#ifdef DARKEN_PASS
        return darkening_factor(colour, coverage);
#else
        return premultiply(colour, coverage);
#endif
    }
    // Liquid: value pooled by the noise, the relief lit from the upper left while wet, and a
    // specular sheen that dries away with the ramp curve.
    shading = clamp(shading * (1.0 + LIQUID_VALUE_SWING * decal.value_noise * (liquid.w - 0.5)), 0.0, 1.0);
    let wet = clamp((decal.ramp_scale - decal.dry_below) / max(1.0 - decal.dry_below, 1e-3), 0.0, 1.0);
    let swell = mix(LIQUID_POOL_RELIEF, LIQUID_SWELL_RELIEF, smoothstep(0.3, 0.7, liquid.w));
    let relief = decal.relief * swell;
    let local_normal = normalize(vec3<f32>(-relief * liquid.x, 1.0, -relief * liquid.y));
    let surface_normal = rotate_yaw(local_normal, decal.yaw);
    // The key light comes from the upper left of the screen: on the ground, half to the
    // screen's left and half away from the camera, low over the ground. A flat film does not
    // mirror it towards the camera, so the sheen lies only on slopes facing it (the thick rim,
    // the noise's swells) and never washes the whole stain.
    let right = view.world_from_view[0].xyz;
    let away = -view.world_from_view[2].xyz;
    let ground_left = normalize(vec3<f32>(-right.x, 0.0, -right.z) + 1e-6);
    let ground_away = normalize(vec3<f32>(away.x, 0.0, away.z) + 1e-6);
    let key_heading = normalize(ground_left + ground_away);
    let key = normalize(key_heading * LIQUID_KEY_COS + vec3<f32>(0.0, LIQUID_KEY_SIN, 0.0));
    let eye = normalize(view.world_position - world);
    let diffuse = clamp(
        1.0 + LIQUID_DIFFUSE * mix(LIQUID_DRY_RELIEF, 1.0, wet) * (dot(surface_normal, key) - key.y),
        LIQUID_DIFFUSE_MIN,
        LIQUID_DIFFUSE_MAX,
    );
    let mirrored = reflect(-eye, surface_normal);
    let specular = pow(max(dot(mirrored, key), 0.0), decal.gloss) * decal.sheen * wet;
    let base = premultiply(ramp_lookup(ramps, decal.ramp_row, shading * decal.ramp_scale), coverage);
    // The wet rim: the band of erosion order just inside the erosion step, and a band inside the
    // texture's own outline or the disc outline, darkened and saturated.
    let step_top = decal.erosion + max(decal.softness, 1e-3);
    let blurred = flipbook_sample(
        atlas, atlas_sampler, uv, decal.layer_a, decal.layer_b, decal.frame_mix, level + LIQUID_RIM_MIPS,
    ).g;
    let rim = max(
        max(
            1.0 - smoothstep(step_top, step_top + LIQUID_RIM_ORDER, order),
            1.0 - smoothstep(LIQUID_RIM_COVERAGE.x, LIQUID_RIM_COVERAGE.y, blurred),
        ),
        smoothstep(edge - LIQUID_OUTLINE_RIM, edge, radius),
    );
    let rimmed = base.rgb * mix(vec3<f32>(1.0), LIQUID_RIM_DARK, rim);
    let liquid_colour = vec4<f32>(rimmed * diffuse + LIQUID_SHEEN_COLOR * specular * coverage * (1.0 - rim), base.a);
#ifdef DARKEN_PASS
    // The catalog refuses a darkening liquid; were one drawn here, it would leave the ground as it is.
    return vec4<f32>(1.0);
#else
    return liquid_colour;
#endif
}

// The radius of a disc outline's edge, in half lengths, at the box's local XZ point `local`
// (`radius` its length): 1 less the edge noise, so the edge wavers between
// 1 - 2 * outline_noise and 1. The noise is the breakup noise read on the circle of the
// direction to `local`, so the edge's waver follows the direction only and the edge is one
// closed line (review round 5: a Critical stain's visible edge on its reach).
fn outline_edge(decal: DecalInstance, local: vec2<f32>, radius: f32, level: f32, noise_offset: vec2<f32>) -> f32 {
    if (decal.noise_amount <= 0.0) {
        return 1.0 - decal.outline_noise;
    }
    let direction = local / max(radius, 1e-5);
    let scale = decal.noise_scale * OUTLINE_NOISE_FREQUENCY;
    let noise_uv = (direction * 0.5 + 0.5) * scale + noise_offset;
    let noise = noise_sample(atlas, atlas_sampler, noise_uv, decal.noise_layer, level + log2(max(scale, 1.0)));
    let waver = clamp(0.5 + OUTLINE_NOISE_CONTRAST * (noise - 0.5), 0.0, 1.0);
    return 1.0 - 2.0 * decal.outline_noise * waver;
}

// The liquid surface at `uv`: xy the thickness field's slope per world unit along the box's
// local x and z, z the thickness, w the low-frequency noise (0.5 without noise). The thickness
// is the element's shading `thickness` moved by the noise. Zero slope when the decal is no
// liquid; a decal that is no liquid reads nothing more (every read here takes an explicit mip,
// so it may sit in non-uniform control flow).
fn liquid_surface(
    decal: DecalInstance,
    uv: vec2<f32>,
    thickness: f32,
    level: f32,
    noise_offset: vec2<f32>,
    noisy: bool,
) -> vec4<f32> {
    if (decal.relief <= 0.0) {
        return vec4<f32>(0.0, 0.0, thickness, 0.5);
    }
    let cell = f32(textureDimensions(atlas).x);
    let relief_level = level + LIQUID_RELIEF_MIPS;
    let step_uv = LIQUID_STEP_TEXELS * exp2(relief_level) / cell;
    let du = vec2<f32>(step_uv, 0.0);
    let dv = vec2<f32>(0.0, step_uv);
    let noise_scale = decal.noise_scale * LIQUID_NOISE_SCALE;
    let noise_level = level + log2(max(noise_scale, 1.0));
    let field = vec4<f32>(
        flipbook_sample(atlas, atlas_sampler, uv + du, decal.layer_a, decal.layer_b, decal.frame_mix, relief_level).r,
        flipbook_sample(atlas, atlas_sampler, uv - du, decal.layer_a, decal.layer_b, decal.frame_mix, relief_level).r,
        flipbook_sample(atlas, atlas_sampler, uv + dv, decal.layer_a, decal.layer_b, decal.frame_mix, relief_level).r,
        flipbook_sample(atlas, atlas_sampler, uv - dv, decal.layer_a, decal.layer_b, decal.frame_mix, relief_level).r,
    );
    var noise = vec4<f32>(0.5);
    var centre_noise = 0.5;
    if (noisy) {
        let noise_uv = uv * noise_scale + noise_offset;
        let noise_du = du * noise_scale;
        let noise_dv = dv * noise_scale;
        noise = vec4<f32>(
            noise_sample(atlas, atlas_sampler, noise_uv + noise_du, decal.noise_layer, noise_level),
            noise_sample(atlas, atlas_sampler, noise_uv - noise_du, decal.noise_layer, noise_level),
            noise_sample(atlas, atlas_sampler, noise_uv + noise_dv, decal.noise_layer, noise_level),
            noise_sample(atlas, atlas_sampler, noise_uv - noise_dv, decal.noise_layer, noise_level),
        );
        centre_noise = noise_sample(atlas, atlas_sampler, noise_uv, decal.noise_layer, noise_level);
    }
    let swing = LIQUID_THICKNESS_SWING * decal.value_noise;
    let heights = field + swing * (noise - vec4<f32>(0.5));
    // One uv unit spans the box's full length (2 half extents) along each axis.
    let world_step = 2.0 * step_uv * vec2<f32>(decal.half_extents.x, decal.half_extents.z);
    let slope = vec2<f32>(heights.x - heights.y, heights.z - heights.w) / max(2.0 * world_step, vec2<f32>(1e-5));
    return vec4<f32>(slope, thickness + swing * (centre_noise - 0.5), centre_noise);
}
