// Production movement-wake ribbons. Coordinates follow path distance rather than cards.
// Premultiplied output: src.rgb + (1 - src.a) * dst.rgb, with coverage applied once.
#import bevy_pbr::mesh_view_bindings::{view, lights}
#import bevy_render::view::ColorGrading
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::{tone_mapping, screen_space_dither}
#import bevy_render::maths::powsafe
#endif
#import bevy_pbr::view_transformations::depth_ndc_to_view_z
#ifdef DEPTH_PREPASS
#import bevy_pbr::prepass_utils::prepass_depth
#endif

// Rust GPU types use exactly these 64/32/80-byte strides. World coordinates are
// reconstructed once; the mesh transform remains only a culling/sorting anchor.
struct EmitterHeader {
    control: vec4<u32>,
    origin_radius: vec4<f32>,
    offset_boost: vec4<f32>,
    capacity: vec4<u32>,
};
struct TrailSample {
    position_age: vec4<f32>,
    distance: vec4<f32>,
};
struct MistWakeUniform {
    cool: vec4<f32>,
    warm: vec4<f32>,
    boost: vec4<f32>,
    // Immutable reference illuminance in lux, followed by three reserved lanes.
    light: vec4<f32>,
    geometry: vec4<f32>,
};
@group(2) @binding(0) var<storage, read> emitters: array<EmitterHeader>;
@group(2) @binding(1) var<storage, read> samples: array<TrailSample>;
@group(2) @binding(2) var<storage, read> mist: MistWakeUniform;

struct VertexInput {
    @location(0) topology: vec4<u32>,
};
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv_b: vec2<f32>,
    @location(4) color: vec4<f32>,
};
struct GeometryVertex {
    position: vec3<f32>,
    normal: vec3<f32>,
    uv: vec2<f32>,
    uv_b: vec2<f32>,
    color: vec4<f32>,
};
fn normalized_or(value: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    let reciprocal = 1.0 / sqrt(dot(value, value));
    if reciprocal > 0.0 && reciprocal < 3.402823e38 {
        return value * reciprocal;
    }
    return fallback;
}
fn geometry_vertex(topology: vec4<u32>, camera: vec3<f32>) -> GeometryVertex {
    let header = emitters[topology.w];
    let count = header.control.y;
    var out: GeometryVertex;
    out.position = header.origin_radius.xyz;
    out.normal = vec3<f32>(0.0, 1.0, 0.0);
    out.uv = vec2<f32>(0.0);
    out.uv_b = vec2<f32>(0.0);
    out.color = vec4<f32>(0.0);
    if count == 0u || header.control.w == 0u { return out; }
    let knot = min(topology.x, count - 1u);
    let sample = samples[header.control.x + knot];
    let before = samples[header.control.x + max(knot, 1u) - 1u];
    let after = samples[header.control.x + min(knot + 1u, count - 1u)];
    let tangent = normalized_or(after.position_age.xyz - before.position_age.xyz, vec3<f32>(1.0, 0.0, 0.0));
    let center = sample.position_age.xyz + header.offset_boost.xyz;
    let facing = normalized_or(camera - center, vec3<f32>(0.0));
    let side = normalized_or(cross(tangent, facing), vec3<f32>(0.0, 0.0, 1.0));
    let age = clamp(sample.position_age.w / mist.geometry.x, 0.0, 1.0);
    let tail = sqrt(1.0 - age);
    let strand = f32(topology.z);
    let width_factor = select(0.20, 1.0, topology.z == 0u);
    let offset = select((strand * 2.0 - 3.0) * 0.30, 0.0, topology.z == 0u);
    let radius = header.origin_radius.w;
    let phase = sample.distance.x / radius;
    let curl = sin(phase * 1.9 + strand * 2.1) * age * 0.18;
    let width = radius * mist.geometry.y * width_factor * (0.65 + age * 0.45) * tail;
    let across = f32(topology.y) * 2.0 - 1.0;
    out.position = center + side * (across * width + radius * (offset + curl) * tail)
        + vec3<f32>(0.0, age * radius * 0.12, 0.0);
    out.normal = facing;
    out.uv = vec2<f32>(phase, across);
    out.uv_b = vec2<f32>(age, header.offset_boost.w);
    let coverage = select(0.0, select(0.95, 1.0, topology.z == 0u), topology.x < count);
    out.color = vec4<f32>(strand, tangent.x, tangent.z, coverage);
    return out;
}
@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    let geometry = geometry_vertex(in.topology, view.world_position);
    var out: VertexOutput;
    out.world_position = vec4<f32>(geometry.position, 1.0);
    out.position = view.clip_from_world * out.world_position;
    out.world_normal = geometry.normal;
    out.uv = geometry.uv;
    out.uv_b = geometry.uv_b;
    out.color = geometry.color;
    return out;
}

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    return fract(dot(q, q + 45.32));
}
fn noise(p: vec2<f32>) -> f32 {
    let base = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(base), hash21(base + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash21(base + vec2<f32>(0.0, 1.0)), hash21(base + vec2<f32>(1.0, 1.0)), u.x), u.y);
}
// Bevy owns per-view visibility/layer eligibility and normalized world-space direction.
// RGB is already linear color times lux. This authored response is dimensionless;
// it intentionally does not apply physical camera exposure a second time.
fn mist_direct_light(world_normal: vec3<f32>, reference_lux: f32) -> vec3<f32> {
    var direct = vec3<f32>(0.0);
    for (var index = 0u; index < lights.n_directional_lights; index += 1u) {
        let light = lights.directional_lights[index];
        if light.skip != 0u { continue; }
        direct += (light.color.rgb / reference_lux)
            * max(dot(world_normal, light.direction_to_light), 0.0);
    }
    return direct;
}

// Convert straight linear radiance before premultiplication. Non-HDR targets use
// Bevy's view grading/tonemapping and optional perceptual dither; the sRGB target
// performs the final transfer. HDR targets retain linear radiance for postprocess.
fn mist_output(color: vec3<f32>, opacity: f32, frag_coord: vec2<f32>, grading: ColorGrading) -> vec4<f32> {
    if opacity <= 0.0 { return vec4<f32>(0.0); }
    var output = vec4<f32>(color, opacity);
#ifdef TONEMAP_IN_SHADER
    output = tone_mapping(output, grading);
#ifdef DEBAND_DITHER
    var perceptual = powsafe(output.rgb, 1.0 / 2.2);
    perceptual += screen_space_dither(frag_coord);
    output = vec4<f32>(powsafe(perceptual, 2.2), opacity);
#endif
#endif
    return vec4<f32>(output.rgb * opacity, opacity);
}

@fragment
fn fragment(in: VertexOutput,
#ifdef MULTISAMPLED
    @builtin(sample_index) sample_index: u32,
#endif
) -> @location(0) vec4<f32> {
    let age = clamp(in.uv_b.x, 0.0, 1.0);
    let boosted = in.uv_b.y;
    let strand = in.color.x;
    let along = in.uv.x;
    let across = in.uv.y;
    let time = mist.warm.w;
    // Advect one coherent field along the path. Broad lobes carry the vapor body;
    // a finer nested field erodes their edges instead of drawing isolated circular puffs.
    let flow = vec2<f32>(along * 0.72 - time * 0.62, across * 1.6 + strand * 3.7);
    let broad = noise(flow * vec2<f32>(0.7, 0.9));
    let bend = sin(along * 1.25 - time * 0.8 + strand * 0.9) * 0.16
        + (broad - 0.5) * 0.32;
    let transverse = across + bend * (1.0 - across * across);
    let fine = noise(flow * vec2<f32>(3.1, 3.8));
    let edge = max(1.0 - abs(transverse), 0.0);
    let erosion = smoothstep(0.10, 0.56, edge + (fine - 0.5) * age * 0.36);
    let stream_phase = transverse * 9.0 + along * 1.15 + broad * 3.0 - time * 0.75;
    let stream = 0.5 + 0.5 * sin(stream_phase);
    let filament = pow(stream, 18.0);
    let is_filament = step(0.5, strand);
    let density = erosion * mix(
        (0.58 + broad * 0.48) * pow(edge, 0.40),
        (0.32 + filament * 1.3) * pow(edge, 0.70),
        is_filament);
    // Extinction is optical depth, not coverage multiplied by several unrelated fades.
    // Preserve the middle of the wake, then dissolve its oldest quarter into broken wisps.
    let fade = pow(1.0 - age, 0.55) * (1.0 - smoothstep(0.72, 1.0, age))
        * smoothstep(0.0, 0.045, age);
    var opacity = 1.0 - exp(-density * fade * mist.boost.w * in.color.a);
#ifdef DEPTH_PREPASS
#ifdef MULTISAMPLED
    let scene_depth = prepass_depth(in.position, sample_index);
#else
    let scene_depth = prepass_depth(in.position, 0u);
#endif
    let scene_distance = -depth_ndc_to_view_z(scene_depth);
    let particle_distance = -depth_ndc_to_view_z(in.position.z);
    opacity *= smoothstep(0.0, mist.cool.w, max(scene_distance - particle_distance, 0.0));
#endif
    if opacity < 0.002 { discard; }
    let tangent = normalize(vec3<f32>(in.color.y, 0.001, in.color.z));
    let side = normalize(cross(tangent, in.world_normal));
    let normal = normalize(in.world_normal * sqrt(max(1.0 - across * across, 0.01)) + side * across);
    let lighting = vec3<f32>(0.38) + 0.62 * mist_direct_light(normal, mist.light.x);
    let tint_phase = clamp(0.5 + 0.5 * sin(along * 0.55 + transverse * 1.5 + strand), 0.0, 1.0);
    var tint = mix(mist.cool.rgb, mist.warm.rgb, tint_phase * 0.30);
    tint = mix(tint, mist.boost.rgb, boosted * 0.45);
    // A shadowed translucent body provides depth against a pale deck. Separate thin
    // strands catch the scene light, so the silhouette has luminous directional detail.
    let body = tint * (vec3<f32>(0.35) + lighting * 0.42);
    let scattering = mix(tint, vec3<f32>(0.86, 0.94, 1.0), 0.45)
        * (vec3<f32>(0.65 + filament * 0.38) + lighting * 0.45);
    let color = mix(body, scattering, is_filament * 0.82 + filament * (1.0 - is_filament) * 0.22);
    return mist_output(color, opacity, in.position.xy, view.color_grading);
}
