// Effect cards: sprites, rings, ribbons and particles drawn from one retained instance buffer
// (client::effects::render). Premultiplied output; soft depth against the depth prepass; noise
// breakup with a phase per card; an optional clip window along an event line.
#import bevy_render::view::View
#import space_scuffle::effects::common::{
    SpriteInstance, ORIENT_GROUND, ORIENT_VERTICAL, ORIENT_STRETCHED, ORIENT_UPRIGHT, ORIENT_RIBBON,
    ORIENT_RIBBON_GROUND, FLAG_MIRROR, FLAG_POLAR, SHAPE_RADIAL, SHAPE_FRONT, ramp_lookup,
    erosion_step, breakup_order, self_shade, noise_sample, clip_fade, soft_depth_fade,
    flipbook_sample, premultiply, card_mip_level, derivative_mip_level,
}

@group(0) @binding(0) var<uniform> view: View;
#ifdef MULTISAMPLED
@group(0) @binding(1) var depth_prepass: texture_depth_multisampled_2d;
#else
@group(0) @binding(1) var depth_prepass: texture_depth_2d;
#endif
@group(1) @binding(0) var<storage, read> cards: array<SpriteInstance>;
@group(1) @binding(1) var atlas: texture_2d_array<f32>;
@group(1) @binding(2) var atlas_sampler: sampler;
@group(1) @binding(3) var ramps: texture_2d<f32>;

const TAU: f32 = 6.28318530718;
// World units over which a clipped card fades out at a clip end (a fade, not a cut edge).
const CLIP_EDGE: f32 = 0.1;
// Contrast of the noise that wavers a pressure front's trailing edge.
const FRONT_WAVER_CONTRAST: f32 = 2.2;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) view_z: f32,
    @location(2) mip: f32,
    @location(3) world_xz: vec2<f32>,
    @location(4) @interpolate(flat) instance: u32,
}

// Corner `index` (0..6) of a two-triangle quad in -1..1.
fn quad_corner(index: u32) -> vec2<f32> {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    return corners[index];
}

// `point` rolled by the angle whose cosine and sine are `c` and `s` about `pivot`.
fn roll_about(point: vec2<f32>, pivot: vec2<f32>, c: f32, s: f32) -> vec2<f32> {
    let p = point - pivot;
    return vec2<f32>(p.x * c - p.y * s, p.x * s + p.y * c) + pivot;
}

@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput {
    let card = cards[instance_index];
    let corner = quad_corner(vertex_index % 6u);
    let c = cos(card.rotation);
    let s = sin(card.rotation);
    var pivot = vec2<f32>(0.0, 0.0);
    if (card.orient == ORIENT_UPRIGHT || card.orient == ORIENT_VERTICAL) {
        // Standing cards lean about the point where they meet the ground.
        pivot = vec2<f32>(0.0, -card.stretch);
    }
    let rolled = roll_about(corner, pivot, c, s);
    var right = view.world_from_view[0].xyz;
    var up = view.world_from_view[1].xyz;
    var along = card.half_size;
    if (card.orient == ORIENT_GROUND) {
        right = vec3<f32>(1.0, 0.0, 0.0);
        up = vec3<f32>(0.0, 0.0, -1.0);
    } else if (card.orient == ORIENT_VERTICAL) {
        right = normalize(vec3<f32>(card.axis.x, 0.0, card.axis.z) + vec3<f32>(1e-6, 0.0, 0.0));
        up = vec3<f32>(0.0, 1.0, 0.0);
    } else if (card.orient == ORIENT_UPRIGHT) {
        let view_right = view.world_from_view[0].xyz;
        right = normalize(vec3<f32>(view_right.x, 0.0, view_right.z) + vec3<f32>(1e-6, 0.0, 0.0));
        up = vec3<f32>(0.0, 1.0, 0.0);
    } else if (card.orient == ORIENT_STRETCHED || card.orient == ORIENT_RIBBON) {
        let to_camera = normalize(view.world_position - card.center);
        let side = cross(card.axis, to_camera);
        if (dot(side, side) > 1e-8) {
            up = card.axis;
            right = normalize(side);
            along = card.half_size * card.stretch;
        }
    } else if (card.orient == ORIENT_RIBBON_GROUND) {
        // Flat on the ground: the width runs horizontally across the strip's axis.
        let side = cross(vec3<f32>(0.0, 1.0, 0.0), card.axis);
        if (dot(side, side) > 1e-8) {
            up = card.axis;
            right = normalize(side);
            along = card.half_size * card.stretch;
        }
    }
    let world = card.center + right * rolled.x * card.half_size + up * rolled.y * along;
    var out: VertexOutput;
    out.position = view.clip_from_world * vec4<f32>(world, 1.0);
    var u = corner.x * 0.5 + 0.5;
    if ((card.flags & FLAG_MIRROR) != 0u) {
        u = 1.0 - u;
    }
    out.uv = vec2<f32>(u, 0.5 - corner.y * 0.5);
    out.view_z = (view.view_from_world * vec4<f32>(world, 1.0)).z;
    let pixels = card.half_size * view.viewport.w * view.clip_from_view[1][1] / max(out.position.w, 1e-3);
    out.mip = card_mip_level(f32(textureDimensions(atlas).x), pixels);
    out.world_xz = world.xz;
    out.instance = instance_index;
    return out;
}

// The noise phase of `card`.
fn noise_offset(card: SpriteInstance) -> vec2<f32> {
    return vec2<f32>(card.noise_offset_x, card.noise_offset_y);
}

// A ring computed as a pressure front: (shading, coverage, erosion order, mask). The band's
// inner (trailing) edge wavers with the noise around the circumference; shading and coverage
// rise towards the outer (leading) edge, the grain breaks them up, and the noise's whole repeats
// around the circle keep it seamless. `level` is the noise mip.
fn front_texel(card: SpriteInstance, centred: vec2<f32>, radius: f32, edge: f32, level: f32) -> vec4<f32> {
    let width = 1.0 - card.ring_inner;
    let angle = atan2(centred.y, centred.x) / TAU + 0.5;
    let repeats = max(round(card.noise_scale), 1.0);
    let offset = noise_offset(card);
    // The noise's contrast stretched, so the band's width varies about twofold around the
    // circumference instead of reading as an even rim (review round 4).
    let waver_raw = noise_sample(atlas, atlas_sampler, vec2<f32>(angle * repeats, 0.37) + offset, card.noise_layer, level);
    let waver = clamp((waver_raw - 0.5) * FRONT_WAVER_CONTRAST + 0.5, 0.0, 1.0);
    let inner = card.ring_inner - width * card.noise_amount * (waver - 0.5) * 2.0;
    // A torn skirt (`front_edge` > 0): the leading edge is bitten back where a third read of
    // the noise (its own row, the same whole repeats around the circle, so no seam) is low, by
    // up to `front_edge` of the band's width, so its outline is a run of clumps whose farthest
    // parts stay on the radius and differs per instance. A round front (0) keeps its whole edge
    // on the radius.
    var leading = 1.0;
    if (card.front_edge > 0.0) {
        let bite_raw = noise_sample(
            atlas, atlas_sampler, vec2<f32>(angle * repeats, 0.71) + offset.yx, card.noise_layer, level,
        );
        let bite = clamp((bite_raw - 0.5) * FRONT_WAVER_CONTRAST + 0.5, 0.0, 1.0);
        leading = 1.0 - width * card.front_edge * (1.0 - smoothstep(0.0, 1.0, bite));
    }
    let across = clamp((radius - inner) / max(leading - inner, 1e-4), 0.0, 1.0);
    let grain = noise_sample(
        atlas, atlas_sampler, vec2<f32>(angle * repeats * 2.0, across * 0.5) + offset.yx, card.noise_layer, level,
    );
    let lead = across * across;
    let shading = lead * (0.55 + 0.45 * grain) * (0.8 + 0.4 * (waver - 0.5));
    let coverage = smoothstep(0.0, 0.5, across) * (0.55 + 0.45 * grain);
    let outer = 1.0 - smoothstep(leading - edge * 1.5, leading, radius);
    let trailing = smoothstep(inner - edge, inner + edge, radius);
    return vec4<f32>(shading, coverage, grain, outer * trailing);
}

// A soft radial glow: (shading, coverage, erosion order, mask). Shading and coverage fall off
// as Gaussians from the centre and the order rises towards it, so erosion eats the halo first.
fn radial_texel(radius: f32) -> vec4<f32> {
    let r2 = radius * radius;
    let mask = 1.0 - smoothstep(0.8, 1.0, radius);
    return vec4<f32>(exp(-r2 * 3.0), exp(-r2 * 2.5), 1.0 - radius, mask);
}

// Whether `card` is a ribbon.
fn is_ribbon(card: SpriteInstance) -> bool {
    return card.orient == ORIENT_RIBBON || card.orient == ORIENT_RIBBON_GROUND;
}

// A ribbon's own coordinates at card `uv`: x along the strip in texture repeats (scrolled),
// y across it (0..1), z the share of the drawn length from the tail (0) to the head (1).
fn strip_coordinates(card: SpriteInstance, uv: vec2<f32>) -> vec3<f32> {
    let from_tail = 1.0 - uv.y;
    return vec3<f32>(from_tail * card.strip_repeats + card.strip_offset, uv.x, from_tail);
}

// A ribbon's texel: (shading, coverage, erosion order, mask). The element is tiled along the
// strip (u along, v across), or for `SHAPE_RADIAL` a soft core is computed across it; the mask
// narrows the strip to a point over its tapers, rounded, with an anti-aliased edge. `mip` and
// `edge` are the strip texture's mip and the screen width of `across`, taken from derivatives
// in uniform control flow.
fn ribbon_texel(card: SpriteInstance, strip: vec3<f32>, across: f32, mip: f32, edge: f32) -> vec4<f32> {
    var texel: vec4<f32>;
    if (card.shape == SHAPE_RADIAL) {
        let a2 = across * across;
        texel = vec4<f32>(exp(-a2 * 3.0), exp(-a2 * 2.0), 1.0 - across, 1.0);
    } else {
        texel = flipbook_sample(atlas, atlas_sampler, strip.xy, card.layer_a, card.layer_b, card.frame_mix, mip);
    }
    var width = 1.0;
    if (card.taper_tail > 0.0) {
        width = min(width, clamp(strip.z / card.taper_tail, 0.0, 1.0));
    }
    if (card.taper_head > 0.0) {
        width = min(width, clamp((1.0 - strip.z) / card.taper_head, 0.0, 1.0));
    }
    // A rounded tip rather than a sharp wedge.
    width = sqrt(width);
    let mask = 1.0 - smoothstep(width - edge * 1.5, width, across);
    return vec4<f32>(texel.r, texel.g, texel.b, mask);
}

// The texel of a texture card at `uv` (a polar ring wraps the element's middle band around
// itself), with the ring mask in the fourth component.
fn texture_texel(card: SpriteInstance, uv_in: vec2<f32>, centred: vec2<f32>, radius: f32, edge: f32, mip: f32) -> vec4<f32> {
    var uv = uv_in;
    if (card.ring_inner > 0.0 && (card.flags & FLAG_POLAR) != 0u) {
        // The element's middle band wrapped around the ring. v runs across the band with the
        // element's densest middle row at the outer edge, which marks the radius; u runs along
        // the angle, mirrored back and forth four times over the middle 70 % of the element, so
        // the ring has no seam and never reaches the element's empty borders.
        let across = clamp((radius - card.ring_inner) / max(1.0 - card.ring_inner, 1e-4), 0.0, 1.0);
        let angle = atan2(centred.y, centred.x) / TAU + 0.5;
        let mirrored = abs(fract(angle * 2.0) * 2.0 - 1.0);
        uv = vec2<f32>(mix(0.15, 0.85, mirrored), mix(0.3, 0.5, across));
    }
    let texel = flipbook_sample(atlas, atlas_sampler, uv, card.layer_a, card.layer_b, card.frame_mix, mip);
    var mask = 1.0;
    if (card.ring_inner > 0.0) {
        let outer = 1.0 - smoothstep(1.0 - edge, 1.0, radius);
        let inner = smoothstep(card.ring_inner - edge, card.ring_inner, radius);
        mask = outer * inner;
    }
    return vec4<f32>(texel.r, texel.g, texel.b, mask);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let card = cards[in.instance];
    let centred = in.uv * 2.0 - 1.0;
    let radius = length(centred);
    let edge = max(fwidth(radius), 1e-4);
    let noise_level = in.mip + log2(max(card.noise_scale, 1.0));
    var texel: vec4<f32>;
    let ribbon = is_ribbon(card);
    // Derivatives of a ribbon's strip coordinates, taken for every card so they stay in
    // uniform control flow.
    let strip = strip_coordinates(card, in.uv);
    let across = abs(in.uv.x * 2.0 - 1.0);
    let strip_mip = derivative_mip_level(f32(textureDimensions(atlas).x), dpdx(strip.xy), dpdy(strip.xy));
    let across_edge = max(fwidth(across), 1e-4);
    if (ribbon) {
        texel = ribbon_texel(card, strip, across, strip_mip, across_edge);
    } else if (card.shape == SHAPE_FRONT && card.ring_inner > 0.0) {
        texel = front_texel(card, centred, radius, edge, noise_level);
    } else if (card.shape == SHAPE_RADIAL) {
        texel = radial_texel(radius);
    } else {
        texel = texture_texel(card, in.uv, centred, radius, edge, in.mip);
    }
    let noisy = card.noise_amount > 0.0;
    var order = texel.b;
    if (noisy) {
        // A ribbon's noise repeats and scrolls with its strip, so it flows along it.
        var noise_uv = in.uv;
        var level = noise_level;
        if (ribbon) {
            noise_uv = strip.xy;
            level = strip_mip + log2(max(card.noise_scale, 1.0));
        }
        let noise = noise_sample(
            atlas, atlas_sampler, noise_uv * card.noise_scale + noise_offset(card), card.noise_layer, level,
        );
        order = breakup_order(order, noise, card.noise_amount);
    }
    var clip = 1.0;
    let clip_axis = vec2<f32>(card.clip_axis_x, card.clip_axis_z);
    if (dot(clip_axis, clip_axis) > 0.0) {
        clip = clip_fade(dot(in.world_xz, clip_axis), card.clip_min, card.clip_max, CLIP_EDGE);
    }
    let scene = textureLoad(depth_prepass, vec2<i32>(floor(in.position.xy)), 0);
    let soft = soft_depth_fade(scene, in.view_z, card.soft_depth, view.clip_from_view);
    let survive = erosion_step(order, card.erosion, card.softness, noisy);
    // An intensity above 1 thickens thin parts; the coverage itself never exceeds 1.
    let coverage = min(texel.g * survive * texel.a * card.intensity * soft * clip, 1.0);
    if (coverage <= 0.0) {
        discard;
    }
    let shading = self_shade(texel.r, texel.g, card.shade);
    let colour = ramp_lookup(ramps, card.ramp_row, shading * card.ramp_scale);
    return premultiply(colour, coverage);
}
