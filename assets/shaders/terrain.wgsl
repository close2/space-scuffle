#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}

// -----------------------------------------------------------------------------
// Bindings @group(2) @binding(100..109) exactly matching TerrainExtension
// -----------------------------------------------------------------------------

struct TerrainParams {
    map_size: vec2<f32>,
    brush_center: vec2<f32>,
    brush_radius: f32,
    brush_shape: u32,
    overlay_flags: u32,
    ground_tiles_per_unit: f32,
    cliff_tiles_per_unit: f32,
    hex_contrast: f32,
    regular_tiling_mask: u32,
    _pad: u32,
    guardrail_emissive: vec4<f32>,
};

@group(2) @binding(100) var<uniform> params: TerrainParams;
@group(2) @binding(101) var albedo_array: texture_2d_array<f32>;
@group(2) @binding(102) var albedo_sampler: sampler;
@group(2) @binding(103) var normal_array: texture_2d_array<f32>;
@group(2) @binding(104) var normal_sampler: sampler;
@group(2) @binding(105) var orm_array: texture_2d_array<f32>;
@group(2) @binding(106) var orm_sampler: sampler;
@group(2) @binding(107) var splat_texture: texture_2d<f32>;
@group(2) @binding(108) var splat_sampler: sampler;
@group(2) @binding(109) var overlay_texture: texture_2d<f32>;

// -----------------------------------------------------------------------------
// Noise & Pseudo-Random Utilities (Jarzynski & Olano 2020)
// -----------------------------------------------------------------------------

fn pcg2d(v: vec2<u32>) -> vec2<u32> {
    var q = v * 1664525u + 1013904223u;
    q.x += q.y * 1664525u;
    q.y += q.x * 1664525u;
    q = q ^ (q >> vec2<u32>(16u, 16u));
    q.x += q.y * 1664525u;
    q.y += q.x * 1664525u;
    return q ^ (q >> vec2<u32>(16u, 16u));
}

fn hash2(p: vec2<i32>) -> vec2<f32> {
    return vec2<f32>(pcg2d(bitcast<vec2<u32>>(p))) / 4294967295.0;
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let v00 = hash2(i).x;
    let v10 = hash2(i + vec2<i32>(1, 0)).x;
    let v01 = hash2(i + vec2<i32>(0, 1)).x;
    let v11 = hash2(i + vec2<i32>(1, 1)).x;

    return mix(mix(v00, v10, u.x), mix(v01, v11, u.x), u.y);
}

fn macro_variation(xz: vec2<f32>) -> f32 {
    return 0.6 * value_noise(xz / 23.0) + 0.4 * value_noise(xz / 7.3);
}

// -----------------------------------------------------------------------------
// Hex-Tiling (Mikkelsen 2022, "Practical Real-Time Hex-Tiling", JCGT 11(2))
// -----------------------------------------------------------------------------

struct HexSample {
    albedo: vec4<f32>,
    normal_ts: vec3<f32>,
    orm: vec3<f32>,
};

fn hex_sample(layer: i32, uv: vec2<f32>) -> HexSample {
    // Authored seamless panel seams must remain aligned. Random rotations are
    // appropriate for natural textures, but destroy manufactured surface continuity.
    if ((params.regular_tiling_mask & (1u << u32(layer))) != 0u) {
        var regular: HexSample;
        regular.albedo = textureSampleGrad(albedo_array, albedo_sampler, uv, layer, dpdx(uv), dpdy(uv));
        regular.normal_ts = normalize(textureSampleGrad(normal_array, normal_sampler, uv, layer, dpdx(uv), dpdy(uv)).rgb * 2.0 - 1.0);
        regular.orm = textureSampleGrad(orm_array, orm_sampler, uv, layer, dpdx(uv), dpdy(uv)).rgb;
        return regular;
    }
    let r = mat2x2<f32>(1.0, 0.0, -0.57735027, 1.15470054);
    let uv_skew = r * uv;
    let base = floor(uv_skew);
    let f = uv_skew - base;

    var v0: vec2<f32>;
    var v1: vec2<f32>;
    var v2: vec2<f32>;
    var w: vec3<f32>;

    if (f.x + f.y < 1.0) {
        v0 = base;
        v1 = base + vec2<f32>(1.0, 0.0);
        v2 = base + vec2<f32>(0.0, 1.0);
        w = vec3<f32>(1.0 - f.x - f.y, f.x, f.y);
    } else {
        v0 = base + vec2<f32>(1.0, 1.0);
        v1 = base + vec2<f32>(1.0, 0.0);
        v2 = base + vec2<f32>(0.0, 1.0);
        w = vec3<f32>(f.x + f.y - 1.0, 1.0 - f.y, 1.0 - f.x);
    }

    let dpdx = dpdx(uv);
    let dpdy = dpdy(uv);

    // Vertex 0 sample
    let h0 = hash2(vec2<i32>(v0));
    let theta0 = h0.x * 6.2831853;
    let cos0 = cos(theta0);
    let sin0 = sin(theta0);
    let uv0 = vec2<f32>(cos0 * uv.x - sin0 * uv.y, sin0 * uv.x + cos0 * uv.y) + h0;
    let dpdx0 = vec2<f32>(cos0 * dpdx.x - sin0 * dpdx.y, sin0 * dpdx.x + cos0 * dpdx.y);
    let dpdy0 = vec2<f32>(cos0 * dpdy.x - sin0 * dpdy.y, sin0 * dpdy.x + cos0 * dpdy.y);
    let alb0 = textureSampleGrad(albedo_array, albedo_sampler, uv0, layer, dpdx0, dpdy0);
    let n_raw0 = textureSampleGrad(normal_array, normal_sampler, uv0, layer, dpdx0, dpdy0).rgb * 2.0 - 1.0;
    let n0 = vec3<f32>(cos0 * n_raw0.x + sin0 * n_raw0.y, -sin0 * n_raw0.x + cos0 * n_raw0.y, n_raw0.z);
    let orm0 = textureSampleGrad(orm_array, orm_sampler, uv0, layer, dpdx0, dpdy0).rgb;

    // Vertex 1 sample
    let h1 = hash2(vec2<i32>(v1));
    let theta1 = h1.x * 6.2831853;
    let cos1 = cos(theta1);
    let sin1 = sin(theta1);
    let uv1 = vec2<f32>(cos1 * uv.x - sin1 * uv.y, sin1 * uv.x + cos1 * uv.y) + h1;
    let dpdx1 = vec2<f32>(cos1 * dpdx.x - sin1 * dpdx.y, sin1 * dpdx.x + cos1 * dpdx.y);
    let dpdy1 = vec2<f32>(cos1 * dpdy.x - sin1 * dpdy.y, sin1 * dpdy.x + cos1 * dpdy.y);
    let alb1 = textureSampleGrad(albedo_array, albedo_sampler, uv1, layer, dpdx1, dpdy1);
    let n_raw1 = textureSampleGrad(normal_array, normal_sampler, uv1, layer, dpdx1, dpdy1).rgb * 2.0 - 1.0;
    let n1 = vec3<f32>(cos1 * n_raw1.x + sin1 * n_raw1.y, -sin1 * n_raw1.x + cos1 * n_raw1.y, n_raw1.z);
    let orm1 = textureSampleGrad(orm_array, orm_sampler, uv1, layer, dpdx1, dpdy1).rgb;

    // Vertex 2 sample
    let h2 = hash2(vec2<i32>(v2));
    let theta2 = h2.x * 6.2831853;
    let cos2 = cos(theta2);
    let sin2 = sin(theta2);
    let uv2 = vec2<f32>(cos2 * uv.x - sin2 * uv.y, sin2 * uv.x + cos2 * uv.y) + h2;
    let dpdx2 = vec2<f32>(cos2 * dpdx.x - sin2 * dpdx.y, sin2 * dpdx.x + cos2 * dpdx.y);
    let dpdy2 = vec2<f32>(cos2 * dpdy.x - sin2 * dpdy.y, sin2 * dpdy.x + cos2 * dpdy.y);
    let alb2 = textureSampleGrad(albedo_array, albedo_sampler, uv2, layer, dpdx2, dpdy2);
    let n_raw2 = textureSampleGrad(normal_array, normal_sampler, uv2, layer, dpdx2, dpdy2).rgb * 2.0 - 1.0;
    let n2 = vec3<f32>(cos2 * n_raw2.x + sin2 * n_raw2.y, -sin2 * n_raw2.x + cos2 * n_raw2.y, n_raw2.z);
    let orm2 = textureSampleGrad(orm_array, orm_sampler, uv2, layer, dpdx2, dpdy2).rgb;

    // Sharpen weights by contrast and bias by sample luminance
    let lum = vec3<f32>(
        dot(alb0.rgb, vec3<f32>(0.299, 0.587, 0.114)),
        dot(alb1.rgb, vec3<f32>(0.299, 0.587, 0.114)),
        dot(alb2.rgb, vec3<f32>(0.299, 0.587, 0.114)),
    );
    var w_sharp = pow(w, vec3<f32>(params.hex_contrast)) * (vec3<f32>(0.5) + lum);
    let sum_w = w_sharp.x + w_sharp.y + w_sharp.z + 1e-5;
    w_sharp = w_sharp / sum_w;

    var result: HexSample;
    result.albedo = alb0 * w_sharp.x + alb1 * w_sharp.y + alb2 * w_sharp.z;
    result.normal_ts = normalize(n0 * w_sharp.x + n1 * w_sharp.y + n2 * w_sharp.z);
    result.orm = orm0 * w_sharp.x + orm1 * w_sharp.y + orm2 * w_sharp.z;
    return result;
}

// -----------------------------------------------------------------------------
// Triplanar Cliff Mapping (Golus 2017)
// -----------------------------------------------------------------------------

struct TriplanarSample {
    albedo: vec4<f32>,
    world_normal: vec3<f32>,
    orm: vec3<f32>,
};

fn triplanar_cliff_sample(wp: vec3<f32>, n_geo: vec3<f32>) -> TriplanarSample {
    let uv_scale = params.cliff_tiles_per_unit;

    // Weights from absolute geometric normal raised to power 4
    var blend_w = pow(abs(n_geo), vec3<f32>(4.0));
    let sum_w = blend_w.x + blend_w.y + blend_w.z + 1e-5;
    blend_w = blend_w / sum_w;

    let uv_x = vec2<f32>(wp.z, wp.y) * uv_scale;
    let uv_y = vec2<f32>(wp.x, wp.z) * uv_scale;
    let uv_z = vec2<f32>(wp.x, wp.y) * uv_scale;

    let cliff_layer = 3i;

    let alb_x = textureSample(albedo_array, albedo_sampler, uv_x, cliff_layer);
    let alb_y = textureSample(albedo_array, albedo_sampler, uv_y, cliff_layer);
    let alb_z = textureSample(albedo_array, albedo_sampler, uv_z, cliff_layer);

    let orm_x = textureSample(orm_array, orm_sampler, uv_x, cliff_layer).rgb;
    let orm_y = textureSample(orm_array, orm_sampler, uv_y, cliff_layer).rgb;
    let orm_z = textureSample(orm_array, orm_sampler, uv_z, cliff_layer).rgb;

    let n_ts_x = textureSample(normal_array, normal_sampler, uv_x, cliff_layer).rgb * 2.0 - 1.0;
    let n_ts_y = textureSample(normal_array, normal_sampler, uv_y, cliff_layer).rgb * 2.0 - 1.0;
    let n_ts_z = textureSample(normal_array, normal_sampler, uv_z, cliff_layer).rgb * 2.0 - 1.0;

    // World normal projection with sign correction
    let sign_x = sign(n_geo.x);
    let sign_y = sign(n_geo.y);
    let sign_z = sign(n_geo.z);

    let n_proj_x = normalize(vec3<f32>(n_ts_x.z * sign_x, n_ts_x.y, n_ts_x.x));
    let n_proj_y = normalize(vec3<f32>(n_ts_y.x, n_ts_y.z * sign_y, n_ts_y.y));
    let n_proj_z = normalize(vec3<f32>(n_ts_z.x, n_ts_z.y, n_ts_z.z * sign_z));

    var result: TriplanarSample;
    result.albedo = alb_x * blend_w.x + alb_y * blend_w.y + alb_z * blend_w.z;
    result.world_normal = normalize(n_proj_x * blend_w.x + n_proj_y * blend_w.y + n_proj_z * blend_w.z);
    result.orm = orm_x * blend_w.x + orm_y * blend_w.y + orm_z * blend_w.z;
    return result;
}

// -----------------------------------------------------------------------------
// Fragment Shader
// -----------------------------------------------------------------------------

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    let wp = in.world_position.xyz;
    let n_geo = normalize(in.world_normal);

    // Vertex attributes passed via UV channels:
    // ATTRIBUTE_UV_1 contains (ambient_occlusion, emissive_mask)
    let ao_vertex = in.uv_b.x;
    let emissive_mask = in.uv_b.y;

    // 1. Splat weights with domain warping
    let warp = (vec2<f32>(
        value_noise(wp.xz),
        value_noise(wp.xz + vec2<f32>(17.3, 31.7)),
    ) - 0.5) * 0.6;

    let splat_uv = (wp.xz + warp) / params.map_size;
    let splat_raw = textureSample(splat_texture, splat_sampler, splat_uv);

    // 2. Height-blend ground layers
    let uv_ground = wp.xz * params.ground_tiles_per_unit;

    // Sample candidate ground layers: 0 = grass, 1 = metal plate, 2 = dirt
    var w_ground = vec3<f32>(splat_raw.r, splat_raw.g, splat_raw.b);

    var samp0 = HexSample(vec4<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0));
    var samp1 = HexSample(vec4<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0));
    var samp2 = HexSample(vec4<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0));

    if (w_ground.x > 0.01) {
        samp0 = hex_sample(0i, uv_ground);
        let lum0 = dot(samp0.albedo.rgb, vec3<f32>(0.299, 0.587, 0.114));
        w_ground.x *= (0.5 + lum0);
    }
    if (w_ground.y > 0.01) {
        samp1 = hex_sample(1i, uv_ground);
        let lum1 = dot(samp1.albedo.rgb, vec3<f32>(0.299, 0.587, 0.114));
        w_ground.y *= (0.5 + lum1);
    }
    if (w_ground.z > 0.01) {
        samp2 = hex_sample(2i, uv_ground);
        let lum2 = dot(samp2.albedo.rgb, vec3<f32>(0.299, 0.587, 0.114));
        w_ground.z *= (0.5 + lum2);
    }

    let sum_ground = w_ground.x + w_ground.y + w_ground.z + 1e-5;
    w_ground = w_ground / sum_ground;

    // 3. Ground aggregate
    let ground_albedo = samp0.albedo * w_ground.x + samp1.albedo * w_ground.y + samp2.albedo * w_ground.z;
    let ground_normal_ts = normalize(samp0.normal_ts * w_ground.x + samp1.normal_ts * w_ground.y + samp2.normal_ts * w_ground.z);
    let ground_orm = samp0.orm * w_ground.x + samp1.orm * w_ground.y + samp2.orm * w_ground.z;

    // Ground top projection tangent frame: T = +X, B = +Z, N = +Y
    let ground_world_normal = normalize(vec3<f32>(ground_normal_ts.x, ground_normal_ts.z, ground_normal_ts.y));

    // 4. Cliff triplanar sample
    let cliff = triplanar_cliff_sample(wp, n_geo);

    // 5. Cliff vs Ground slope transition
    let cliff_w = smoothstep(0.55, 0.85, 1.0 - abs(n_geo.y));

    let blended_albedo = mix(ground_albedo.rgb, cliff.albedo.rgb, cliff_w);
    let blended_normal = normalize(mix(ground_world_normal, cliff.world_normal, cliff_w));
    let blended_orm = mix(ground_orm, cliff.orm, cliff_w);

    // 6. Macro variation
    var final_albedo = blended_albedo * mix(0.85, 1.15, macro_variation(wp.xz));

    // 7. Abyss void fade
    let abyss_depth = smoothstep(-6.0, 0.0, wp.y);
    final_albedo = final_albedo * abyss_depth;

    // 8. PBR material assignment
    pbr.material.base_color = vec4<f32>(final_albedo, 1.0);
    pbr.material.perceptual_roughness = blended_orm.g;
    pbr.material.metallic = blended_orm.b;
    pbr.diffuse_occlusion = vec3<f32>(blended_orm.r * ao_vertex);
    pbr.N = blended_normal;
    pbr.material.emissive = vec4<f32>(params.guardrail_emissive.rgb * emissive_mask, 1.0);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);

    // -------------------------------------------------------------------------
    // 9. Overlays applied AFTER lighting (unshadowed tactical information)
    // -------------------------------------------------------------------------
    let cell_coord = vec2<i32>(floor(wp.xz));
    let cell_valid = cell_coord.x >= 0 && cell_coord.y >= 0
        && cell_coord.x < i32(params.map_size.x) && cell_coord.y < i32(params.map_size.y);

    if (cell_valid) {
        let ov = textureLoad(overlay_texture, cell_coord, 0);

        // Pathability (bit 1): red tint where pathability is 0
        if ((params.overlay_flags & 2u) != 0u && ov.r < 0.5) {
            out.color = mix(out.color, vec4<f32>(0.9, 0.1, 0.1, 1.0), 0.35);
        }

        // Buildability (bit 2): amber tint where buildability is 0
        if ((params.overlay_flags & 4u) != 0u && ov.g < 0.5) {
            out.color = mix(out.color, vec4<f32>(0.95, 0.65, 0.1, 1.0), 0.35);
        }

        // LOS Visibility (bit 3): fog modulation
        if ((params.overlay_flags & 8u) != 0u) {
            out.color = vec4<f32>(out.color.rgb * mix(0.25, 1.0, ov.b), out.color.a);
        }

        // Validation Error (bit 5): pulsing red fill
        if ((params.overlay_flags & 32u) != 0u && ov.a > 0.5) {
            let pulse = 0.5 + 0.5 * sin(fract(wp.x * 0.5 + wp.z * 0.5) * 6.2831853);
            out.color = mix(out.color, vec4<f32>(1.0, 0.05, 0.05, 1.0), 0.40 * pulse);
        }
    }

    // Grid (bit 0): anti-aliased cell boundaries on flat floor surfaces
    if ((params.overlay_flags & 1u) != 0u && n_geo.y > 0.9) {
        let d = abs(fract(wp.xz) - 0.5);
        let fw = fwidth(wp.xz) * 1.25;
        let line = 1.0 - smoothstep(vec2<f32>(0.0, 0.0), fw, vec2<f32>(0.5, 0.5) - d);
        let grid_factor = max(line.x, line.y);
        out.color = mix(out.color, vec4<f32>(1.0, 1.0, 1.0, 1.0), grid_factor * 0.22);
    }

    // Brush Tool (bit 4): active footprint preview
    if ((params.overlay_flags & 16u) != 0u) {
        let delta = wp.xz - params.brush_center;
        let dist = select(length(delta), max(abs(delta.x), abs(delta.y)), params.brush_shape == 1u);
        if (dist <= params.brush_radius) {
            out.color = mix(out.color, vec4<f32>(0.0, 0.9, 1.0, 1.0), 0.18);
        }
        let edge_d = abs(dist - params.brush_radius);
        let fw_d = fwidth(dist) * 1.5;
        let aa = 1.0 - smoothstep(0.0, fw_d, edge_d - 0.05);
        out.color = mix(out.color, vec4<f32>(0.0, 1.0, 1.0, 1.0), aa * 0.75);
    }

    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
