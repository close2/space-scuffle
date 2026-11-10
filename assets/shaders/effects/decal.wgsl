// Effect decals: boxes projected onto the scene through the depth prepass
// (client::effects::render). Each box is drawn by its back faces with no depth test; the
// fragment reconstructs the scene's world position, keeps it only inside the box, samples the
// texture in the box's local XZ and fades on faces steeper than the decal's grazing cosine.
#import bevy_render::view::View
#import space_scuffle::effects::common::{
    DecalInstance, ramp_lookup, erosion_step, flipbook_sample, premultiply,
}

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
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let decal = decals[in.instance];
    let depth = textureLoad(depth_prepass, vec2<i32>(floor(in.position.xy)), 0);
    let uv = (in.position.xy - view.viewport.xy) / view.viewport.zw;
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world_h = view.world_from_clip * ndc;
    let world = world_h.xyz / world_h.w;
    let normal = normalize(cross(dpdy(world), dpdx(world)));
    let local = rotate_yaw(world - decal.center, -decal.yaw) / decal.half_extents;
    let texel = flipbook_sample(
        atlas, atlas_sampler, local.xz * 0.5 + 0.5, decal.layer_a, decal.layer_b, decal.frame_mix, 0.0,
    );
    let clipped = local.x < decal.clip_min || local.x > decal.clip_max;
    let inside = all(abs(local) <= vec3<f32>(1.0)) && depth > 0.0 && !clipped;
    let grazing = clamp((abs(normal.y) - decal.grazing) / max(1.0 - decal.grazing, 1e-3), 0.0, 1.0);
    let height_fade = 1.0 - smoothstep(0.7, 1.0, abs(local.y));
    let coverage = texel.g * erosion_step(texel.b, decal.erosion) * decal.intensity * grazing * height_fade;
    if (!inside || coverage <= 0.0) {
        discard;
    }
    return premultiply(ramp_lookup(ramps, decal.ramp_row, texel.r * decal.ramp_scale), coverage);
}
