// Effect cards: sprites, rings and particles drawn from one retained instance buffer
// (client::effects::render). Premultiplied output; soft depth against the depth prepass.
#import bevy_render::view::View
#import space_scuffle::effects::common::{
    SpriteInstance, ORIENT_GROUND, ORIENT_VERTICAL, ORIENT_STRETCHED, FLAG_MIRROR, FLAG_POLAR,
    ramp_lookup, erosion_step, soft_depth_fade, flipbook_sample, premultiply, card_mip_level,
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

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) view_z: f32,
    @location(2) mip: f32,
    @location(3) @interpolate(flat) instance: u32,
}

// Corner `index` (0..6) of a two-triangle quad in -1..1.
fn quad_corner(index: u32) -> vec2<f32> {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    return corners[index];
}

@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32, @builtin(instance_index) instance_index: u32) -> VertexOutput {
    let card = cards[instance_index];
    let corner = quad_corner(vertex_index % 6u);
    let c = cos(card.rotation);
    let s = sin(card.rotation);
    let rolled = vec2<f32>(corner.x * c - corner.y * s, corner.x * s + corner.y * c);
    var right = view.world_from_view[0].xyz;
    var up = view.world_from_view[1].xyz;
    var along = card.half_size;
    if (card.orient == ORIENT_GROUND) {
        right = vec3<f32>(1.0, 0.0, 0.0);
        up = vec3<f32>(0.0, 0.0, -1.0);
    } else if (card.orient == ORIENT_VERTICAL) {
        right = normalize(vec3<f32>(card.axis.x, 0.0, card.axis.z) + vec3<f32>(1e-6, 0.0, 0.0));
        up = vec3<f32>(0.0, 1.0, 0.0);
    } else if (card.orient == ORIENT_STRETCHED) {
        let to_camera = normalize(view.world_position - card.center);
        let side = cross(card.axis, to_camera);
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
    out.instance = instance_index;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let card = cards[in.instance];
    let centred = in.uv * 2.0 - 1.0;
    let radius = length(centred);
    let edge = max(fwidth(radius), 1e-4);
    var uv = in.uv;
    if (card.ring_inner > 0.0 && (card.flags & FLAG_POLAR) != 0u) {
        // The element's middle band wrapped around the ring. v runs across the band with the
        // element's densest middle row at the outer edge, which marks the radius; u runs along
        // the angle, mirrored back and forth four times over the middle 70 % of the element, so
        // the ring has no seam and never reaches the element's empty borders. Sampling at the
        // card's own mip (not by derivatives) keeps the mirror turns free of mip pops.
        let across = clamp((radius - card.ring_inner) / max(1.0 - card.ring_inner, 1e-4), 0.0, 1.0);
        let angle = atan2(centred.y, centred.x) / 6.28318530718 + 0.5;
        let mirrored = abs(fract(angle * 2.0) * 2.0 - 1.0);
        uv = vec2<f32>(mix(0.15, 0.85, mirrored), mix(0.3, 0.5, across));
    }
    let texel = flipbook_sample(atlas, atlas_sampler, uv, card.layer_a, card.layer_b, card.frame_mix, in.mip);
    var shape = 1.0;
    if (card.ring_inner > 0.0) {
        let outer = 1.0 - smoothstep(1.0 - edge, 1.0, radius);
        let inner = smoothstep(card.ring_inner - edge, card.ring_inner, radius);
        shape = outer * inner;
    }
    let scene = textureLoad(depth_prepass, vec2<i32>(floor(in.position.xy)), 0);
    let soft = soft_depth_fade(scene, in.view_z, card.soft_depth, view.clip_from_view);
    let coverage = texel.g * erosion_step(texel.b, card.erosion) * shape * card.intensity * soft;
    if (coverage <= 0.0) {
        discard;
    }
    let colour = ramp_lookup(ramps, card.ramp_row, texel.r * card.ramp_scale);
    return premultiply(colour, coverage);
}
