// Effect cards: sprites, rings, ribbons and particles drawn from one retained instance buffer
// (client::effects::render). Premultiplied output; soft depth against the depth prepass; noise
// breakup with a phase per card; an optional clip window along an event line. The pipeline with
// `DARKEN_PASS` draws only cards flagged `FLAG_DARKEN` and outputs the factor the target is
// multiplied by; the other draws every other card.
#import bevy_render::view::View
#import space_scuffle::effects::common::{
    SpriteInstance, ORIENT_GROUND, ORIENT_VERTICAL, ORIENT_STRETCHED, ORIENT_UPRIGHT, ORIENT_RIBBON,
    ORIENT_RIBBON_GROUND, FLAG_MIRROR, FLAG_POLAR, FLAG_ARC, FLAG_DARKEN, SHAPE_RADIAL, SHAPE_FRONT, SHAPE_MEMBRANE, ramp_lookup,
    erosion_step, breakup_order, self_shade, noise_sample, clip_fade, soft_depth_fade,
    flipbook_sample, premultiply, card_mip_level, derivative_mip_level, darkening_factor, NOWHERE,
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
// Weight of a torn front's second, finer octave in the bite of its leading edge; the mix is
// re-contrasted by `FRONT_OCTAVE_NORM` (1 / sqrt(0.65^2 + 0.35^2)) so it keeps one octave's spread.
const FRONT_SECOND_OCTAVE: f32 = 0.35;
const FRONT_OCTAVE_NORM: f32 = 1.355;
// A torn front's guaranteed crests (client::effects::front, which mirrors this placement and
// proves its gaps): 12 crests, each jittered by up to 7 degrees, 2.5 degrees half wide (one
// footprint ray always falls on one), with a 1.5-degree soft edge; angles in turns.
const FRONT_CRESTS: u32 = 12u;
const CREST_JITTER: f32 = 0.019444444;
const CREST_HALF_WIDTH: f32 = 0.0069444444;
const CREST_SOFT: f32 = 0.0041666667;
// An arc's head (FLAG_ARC): a soft end 0.05 of a turn (18 degrees) wide, centred on the bound
// share so the drawn share stays within 3 % of it (a wavering band applies its mask after its
// intensity, so the whole fade shows however much it is thickened). Freshly turned earth is lit
// just behind it: its shading rises by up to `ARC_HEAD_LIGHT`, falling off over about a
// twentieth of a turn (WP8 review 2, B2-2: round 2's 3.6-degree step read as a progress ring's
// cut end).
const ARC_HEAD: f32 = 0.05;
const ARC_HEAD_LIGHT: f32 = 0.6;
const ARC_HEAD_FALLOFF: f32 = 20.0;
// A wavering band (`warp`): its middle radius wanders by this share of `warp` either way, and
// its shading runs this far towards its position across the band (furrow to thrown soil).
const WAVER_RADIAL: f32 = 0.1;
const WAVER_ACROSS: f32 = 0.6;
// A wavering band's edges fade over this share of its width each (WP8 review 3, A3-1: a band
// cut within one pixel read as a drawn outline). Its mask is applied after the intensity, so a
// thickened band keeps the whole fade.
const WAVER_SOFT: f32 = 0.5;
// A membrane's look: its face keeps this share of the rim's shading and coverage, the rim
// rises with this power of how obliquely the shell is seen, and the outline fades over this
// share of the radius.
const MEMBRANE_FACE: f32 = 0.12;
const MEMBRANE_FALLOFF: f32 = 2.0;
const MEMBRANE_EDGE: f32 = 0.08;

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
    let darkening = (card.flags & FLAG_DARKEN) != 0u;
#ifdef DARKEN_PASS
    if (!darkening) {
        out.position = NOWHERE;
    }
#else
    if (darkening) {
        out.position = NOWHERE;
    }
#endif
    return out;
}

// The noise phase of `card`.
fn noise_offset(card: SpriteInstance) -> vec2<f32> {
    return vec2<f32>(card.noise_offset_x, card.noise_offset_y);
}

// A 32-bit integer hash (PCG output permutation): the same as `client::effects::front::crest_hash`.
fn crest_hash(value: u32) -> u32 {
    let state = value * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

// The top 24 bits of `value` as a fraction in [0, 1).
fn crest_unit(value: u32) -> f32 {
    return f32(value >> 8u) / 16777216.0;
}

// How much `angle` (turns) lies on one of the guaranteed crests of the torn front drawn with
// `seed`: 1 within a crest's half width, 0 beyond its soft edge (client::effects::front).
fn front_crest(seed: u32, angle: f32) -> f32 {
    let rotation = crest_unit(crest_hash(seed ^ 0x9E3779B9u));
    var nearest = 1.0;
    for (var index = 0u; index < FRONT_CRESTS; index++) {
        let jitter = (2.0 * crest_unit(crest_hash(seed + index * 0x85EBCA6Bu)) - 1.0) * CREST_JITTER;
        let centre = fract((f32(index) + rotation) / f32(FRONT_CRESTS) + jitter + 1.0);
        let d = abs(fract(angle - centre + 1.0));
        nearest = min(nearest, min(d, 1.0 - d));
    }
    return 1.0 - smoothstep(CREST_HALF_WIDTH, CREST_HALF_WIDTH + CREST_SOFT, nearest);
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
    // the noise (two octaves on their own rows, the same whole repeats around the circle, so no
    // seam) is low, by up to `front_edge` of the band's width, except on its guaranteed crests,
    // where it stays on the radius; the crests' rotation and jitter come from the card's own
    // seed, so every 45 degrees has dust on the reach and no two hits share an outline
    // (Amendment 8). A round front (0) keeps its whole edge on the radius.
    var leading = 1.0;
    if (card.front_edge > 0.0) {
        let bite_mix = mix(
            noise_sample(atlas, atlas_sampler, vec2<f32>(angle * repeats, 0.71) + offset.yx, card.noise_layer, level),
            noise_sample(atlas, atlas_sampler, vec2<f32>(angle * repeats * 2.0, 0.29) + offset, card.noise_layer, level),
            FRONT_SECOND_OCTAVE,
        );
        let bite_raw = (bite_mix - 0.5) * FRONT_OCTAVE_NORM + 0.5;
        let bite = clamp((bite_raw - 0.5) * FRONT_WAVER_CONTRAST + 0.5, 0.0, 1.0);
        let crest = front_crest(card.front_seed, angle);
        leading = 1.0 - width * card.front_edge * (1.0 - max(smoothstep(0.0, 1.0, bite), crest));
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

// A thin membrane round a body (`SHAPE_MEMBRANE`, plan WP9): (shading, coverage, erosion order,
// mask). The card is the outline of a sphere seen from the camera. Light crosses more of a thin
// shell the more obliquely it is seen, so the membrane is faint where it faces the camera and
// densest just inside its outline; the outline itself fades over `MEMBRANE_EDGE` of the radius
// rather than ending on a drawn circle. The order rises towards the outline, so the layer's
// erosion opens the face first and the rim last.
fn membrane_texel(radius: f32, edge: f32) -> vec4<f32> {
    let r = min(radius, 1.0);
    let facing = sqrt(max(1.0 - r * r, 0.0));
    let rim = pow(1.0 - facing, MEMBRANE_FALLOFF);
    let mask = 1.0 - smoothstep(1.0 - max(MEMBRANE_EDGE, edge * 1.5), 1.0, radius);
    return vec4<f32>(MEMBRANE_FACE + (1.0 - MEMBRANE_FACE) * rim, MEMBRANE_FACE + (1.0 - MEMBRANE_FACE) * rim, rim, mask);
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
    var shading = texel.r;
    if (card.ring_inner > 0.0 && card.warp > 0.0) {
        let band = waver_band(card, centred, radius, mip);
        let soft = max(WAVER_SOFT * (band.y - band.x), edge);
        mask = (1.0 - smoothstep(band.y - soft, band.y, radius)) * smoothstep(band.x, band.x + soft, radius);
        // Dark furrow on the inner edge, lighter thrown soil on the outer one.
        let across = clamp((radius - band.x) / max(band.y - band.x, 1e-4), 0.0, 1.0);
        shading = mix(texel.r, across, WAVER_ACROSS);
    } else if (card.ring_inner > 0.0) {
        let outer = 1.0 - smoothstep(1.0 - edge, 1.0, radius);
        let inner = smoothstep(card.ring_inner - edge, card.ring_inner, radius);
        mask = outer * inner;
    }
    if (card.ring_inner > 0.0) {
        if ((card.flags & FLAG_ARC) != 0u && card.arc < 1.0) {
            let arc = arc_texel(centred, card.arc);
            mask *= arc.x;
            shading = min(shading + arc.y, 1.0);
        }
    }
    return vec4<f32>(shading, texel.g, texel.b, mask);
}

// A wavering ring band (`warp` > 0): (inner edge, outer edge) as fractions of the card's half
// size at this angle. Two reads of the card's breakup noise round the turn (whole repeats, so
// no seam) vary the band's width by up to `warp` either way and its middle radius by
// `WAVER_RADIAL` of `warp`, so the band reads as dug ground and not as a drawn circle.
fn waver_band(card: SpriteInstance, centred: vec2<f32>, radius: f32, mip: f32) -> vec2<f32> {
    let turn = atan2(centred.y, centred.x) / TAU + 0.5;
    let repeats = max(round(card.noise_scale), 1.0);
    let level = mip + log2(repeats);
    let offset = noise_offset(card);
    let width_noise = noise_sample(atlas, atlas_sampler, vec2<f32>(turn * repeats, 0.23) + offset, card.noise_layer, level);
    let radial_noise = noise_sample(atlas, atlas_sampler, vec2<f32>(turn * repeats * 0.5, 0.61) + offset.yx, card.noise_layer, level);
    let width = 1.0 - card.ring_inner;
    let half_width = 0.5 * width * (1.0 + (width_noise - 0.5) * 2.0 * card.warp);
    let wobble = (radial_noise - 0.5) * 2.0 * WAVER_RADIAL * card.warp;
    // Room for the widest band and the largest wobble inside the card.
    let middle = 1.0 - 0.5 * width * (1.0 + card.warp) - WAVER_RADIAL * card.warp + wobble;
    return vec2<f32>(middle - half_width, middle + half_width);
}

// An arc of a ring drawn from its start angle round to `share` of the turn: (mask, head
// light). The arc's head is a soft step `ARC_HEAD` of a turn wide and the ring is lit a little
// just behind it, so a closing seal has a leading point and no hard cut.
fn arc_texel(centred: vec2<f32>, share: f32) -> vec2<f32> {
    let turn = atan2(centred.y, centred.x) / TAU + 0.5;
    let mask = 1.0 - smoothstep(share - 0.5 * ARC_HEAD, share + 0.5 * ARC_HEAD, turn);
    let behind = max(share - turn, 0.0);
    let light = ARC_HEAD_LIGHT * exp(-behind * ARC_HEAD_FALLOFF) * mask;
    return vec2<f32>(mask, light);
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
    } else if (card.shape == SHAPE_MEMBRANE) {
        texel = membrane_texel(radius, edge);
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
    // An intensity above 1 thickens thin parts; the coverage itself never exceeds 1. A wavering
    // band's mask (its soft edges) is applied after it, so thickening does not sharpen them.
    let wavering = !ribbon && card.ring_inner > 0.0 && card.warp > 0.0;
    let thickened = select(texel.a, 1.0, wavering);
    let outline = select(1.0, texel.a, wavering);
    let coverage = min(texel.g * survive * thickened * card.intensity * soft * clip, 1.0) * outline;
    if (coverage <= 0.0) {
        discard;
    }
    let shading = self_shade(texel.r, texel.g, card.shade);
    let colour = ramp_lookup(ramps, card.ramp_row, shading * card.ramp_scale);
#ifdef DARKEN_PASS
    return darkening_factor(colour, coverage);
#else
    return premultiply(colour, coverage);
#endif
}
