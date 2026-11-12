// Effect distortion discs (plan 2026-11-09-effect-visualization §5.1 "Distortion", WP7): each
// instance bends the opaque scene behind it. The view's transmission texture (Bevy's copy of
// the opaque scene, made before the transmissive phase) is sampled at an offset read from
// scrolling noise and blended back over the scene inside a soft disc, so the disc draws no
// colour of its own: where the offset is zero the copy lies exactly on what it copies.
// Records are client::effects::instances::SpriteInstance; the fields a disc uses are its
// centre, half size, orientation, intensity and the noise fields (`noise_amount` is the
// strength: the largest offset as a fraction of the disc's radius on screen).
#import bevy_render::view::View
#import space_scuffle::effects::common::{SpriteInstance, ORIENT_GROUND, noise_sample}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var scene_sampler: sampler;
@group(1) @binding(0) var<storage, read> discs: array<SpriteInstance>;
@group(1) @binding(1) var atlas: texture_2d_array<f32>;
@group(1) @binding(2) var atlas_sampler: sampler;
@group(1) @binding(3) var ramps: texture_2d<f32>;

// Share of the disc's radius from which its effect fades out towards its rim.
const FADE_FROM: f32 = 0.45;
// A lens disc (`lens` > 0) bends the scene up to its rim: it fades out only from this share
// of its radius, and its radial bend peaks at `LENS_PEAK` of it.
const LENS_RIM: f32 = 0.86;
const LENS_PEAK: f32 = 0.78;
// Offset between the two reads of the noise, in noise tiles, so the two axes are unrelated.
const SECOND_READ: vec2<f32> = vec2<f32>(0.37, 0.61);

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // The disc's radius in screen UV units (vertical), at its centre's depth.
    @location(1) radius_uv: f32,
    @location(2) @interpolate(flat) instance: u32,
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
    let disc = discs[instance_index];
    let corner = quad_corner(vertex_index % 6u);
    var right = view.world_from_view[0].xyz;
    var up = view.world_from_view[1].xyz;
    if (disc.orient == ORIENT_GROUND) {
        right = vec3<f32>(1.0, 0.0, 0.0);
        up = vec3<f32>(0.0, 0.0, -1.0);
    }
    let world = disc.center + (right * corner.x + up * corner.y) * disc.half_size;
    var out: VertexOutput;
    out.position = view.clip_from_world * vec4<f32>(world, 1.0);
    out.uv = vec2<f32>(corner.x * 0.5 + 0.5, 0.5 - corner.y * 0.5);
    let centre_clip = view.clip_from_world * vec4<f32>(disc.center, 1.0);
    out.radius_uv = disc.half_size * view.clip_from_view[1][1] * 0.5 / max(centre_clip.w, 1e-3);
    out.instance = instance_index;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let disc = discs[in.instance];
    let centred = in.uv * 2.0 - 1.0;
    let radius = length(centred);
    let lensed = disc.lens > 0.0;
    var mask = 1.0 - smoothstep(FADE_FROM, 1.0, radius);
    if (lensed) {
        mask = 1.0 - smoothstep(LENS_RIM, 1.0, radius);
    }
    if (mask <= 0.0) {
        discard;
    }
    let noise_uv = in.uv * disc.noise_scale + vec2<f32>(disc.noise_offset_x, disc.noise_offset_y);
    let along_x = noise_sample(atlas, atlas_sampler, noise_uv, disc.noise_layer, 0.0) - 0.5;
    let along_y = noise_sample(atlas, atlas_sampler, noise_uv.yx + SECOND_READ, disc.noise_layer, 0.0) - 0.5;
    let intensity = clamp(disc.intensity, 0.0, 1.0);
    let strength = disc.noise_amount * intensity * mask;
    // Screen UV y runs down; the aspect ratio keeps the offset round on screen.
    let aspect = view.viewport.w / max(view.viewport.z, 1.0);
    var offset = vec2<f32>(along_x * aspect, along_y) * 2.0 * strength * in.radius_uv;
    if (lensed) {
        // A glass body's refraction: the scene is read from further out along the radius, most
        // a little inside the rim and not at all at the centre or beyond the rim, so the body's
        // outline shows as a bent ring of what lies behind it and nothing of its own.
        let profile = smoothstep(0.0, LENS_PEAK, radius) * (1.0 - smoothstep(LENS_PEAK, 1.0, radius));
        let outward = centred / max(radius, 1e-4) * disc.lens * intensity * profile;
        offset += vec2<f32>(outward.x * aspect, outward.y) * in.radius_uv;
    }
    let screen_uv = (in.position.xy - view.viewport.xy) / view.viewport.zw;
    let bent = textureSampleLevel(scene, scene_sampler, clamp(screen_uv + offset, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    // Premultiplied: the bent scene over the scene, as opaque as the disc's mask.
    return vec4<f32>(bent.rgb * mask, mask);
}
