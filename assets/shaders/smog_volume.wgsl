// assets/shaders/smog_volume.wgsl
// 3D Volumetric Raymarching Smog Shader with Hardware-Trilinear Wispy Detail and Dissipating Glow.
// Implements 28-step Beer-Lambert volumetric raymarching through 3D density and velocity textures,
// O(1) hardware-sampled tileable 3D wispy noise texture, empty-space skipping, multi-color spectrum,
// Henyey-Greenstein anisotropic scattering, and depth prepass intersection.

#import bevy_pbr::mesh_view_bindings::{view, globals}
#import bevy_pbr::forward_io::VertexOutput
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif
#import bevy_pbr::view_transformations::depth_ndc_to_view_z
#ifdef DEPTH_PREPASS
#import bevy_pbr::prepass_utils::prepass_depth
#endif

struct SmogVolumeUniform {
    volume_min: vec4<f32>,           // XYZ: volume min, W: soft depth fade distance
    volume_max: vec4<f32>,           // XYZ: volume max, W: global extinction scale
    base_tint_time: vec4<f32>,       // RGB: ambient smog tint, W: anim time in seconds
    warm_tint_anisotropy: vec4<f32>, // RGB: warm scattering tint, W: Henyey-Greenstein g
    light_dir_intensity: vec4<f32>,  // XYZ: light direction, W: light intensity
    control_params: vec4<f32>,       // X: active bubbles count, Y: bow-wave rim strength, Z: cavity strength, W: enabled
    bubble_pos_radius: array<vec4<f32>, 16>, // XYZ: center, W: radius
    bubble_vel_boost: array<vec4<f32>, 16>,  // XYZ: velocity, W: speed boost / wake factor
};

@group(2) @binding(0) var<uniform> smog: SmogVolumeUniform;
@group(2) @binding(1) var smog_density_tex: texture_3d<f32>;
@group(2) @binding(2) var smog_density_sampler: sampler;
@group(2) @binding(3) var smog_velocity_tex: texture_3d<f32>;
@group(2) @binding(4) var smog_velocity_sampler: sampler;
@group(2) @binding(5) var smog_noise_tex: texture_3d<f32>;
@group(2) @binding(6) var smog_noise_sampler: sampler;

const PI: f32 = 3.141592653589793;

// Computes intersection distances [t_enter, t_exit] of a ray with an Axis-Aligned Bounding Box.
fn intersect_aabb(ray_origin: vec3<f32>, ray_dir: vec3<f32>, box_min: vec3<f32>, box_max: vec3<f32>) -> vec2<f32> {
    let inv_dir = 1.0 / ray_dir;
    let t0 = (box_min - ray_origin) * inv_dir;
    let t1 = (box_max - ray_origin) * inv_dir;
    let t_min = min(t0, t1);
    let t_max = max(t0, t1);
    let t_enter = max(max(t_min.x, t_min.y), t_min.z);
    let t_exit = min(min(t_max.x, t_max.y), t_max.z);
    return vec2<f32>(t_enter, t_exit);
}

// Henyey-Greenstein anisotropic phase scattering function.
fn hg_phase(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = 1.0 + g2 - 2.0 * g * cos_theta;
    return (1.0 - g2) / (4.0 * PI * pow(max(denom, 0.001), 1.5));
}

// Curated harmonious bioluminescent base palette matching SHOWCASE_BUBBLE_PALETTE in kinematics.rs.
fn bubble_palette_color(index: u32) -> vec3<f32> {
    switch index {
        case 0u: { return vec3<f32>(0.98, 0.82, 0.22); }
        case 1u: { return vec3<f32>(0.18, 0.88, 0.96); }
        case 2u: { return vec3<f32>(0.20, 0.92, 0.52); }
        case 3u: { return vec3<f32>(0.72, 0.36, 0.95); }
        case 4u: { return vec3<f32>(0.96, 0.32, 0.58); }
        case 5u: { return vec3<f32>(0.24, 0.58, 0.98); }
        case 6u: { return vec3<f32>(0.98, 0.55, 0.18); }
        case 7u: { return vec3<f32>(0.28, 0.88, 0.75); }
        case 8u: { return vec3<f32>(0.95, 0.26, 0.38); }
        case 9u: { return vec3<f32>(0.60, 0.52, 0.96); }
        default: {
            let hue = (f32(index) * 0.137) % 1.0;
            let tau = 6.2831853;
            return clamp(vec3<f32>(
                0.25 + 0.65 * abs(sin(hue * tau)),
                0.30 + 0.60 * abs(sin((hue + 0.33) * tau)),
                0.35 + 0.60 * abs(sin((hue + 0.67) * tau))
            ), vec3<f32>(0.05), vec3<f32>(0.95));
        }
    }
}

@fragment
fn fragment(
    in: VertexOutput,
#ifdef MULTISAMPLED
    @builtin(sample_index) sample_index: u32,
#endif
) -> @location(0) vec4<f32> {
    if (smog.control_params.w < 0.5) {
        discard;
    }

    let ray_origin = view.world_position;
    let ray_dir = normalize(in.world_position.xyz - view.world_position);

    let march_max = smog.volume_max.xyz;
    let t_hit = intersect_aabb(ray_origin, ray_dir, smog.volume_min.xyz, march_max);
    let t_enter = max(t_hit.x, 0.0);
    var t_exit = t_hit.y;

    if (t_exit <= t_enter || t_exit <= 0.0) {
        discard;
    }

    var scene_distance = 1e6;
#ifdef DEPTH_PREPASS
#ifdef MULTISAMPLED
    let scene_depth = prepass_depth(in.position, sample_index);
#else
    let scene_depth = prepass_depth(in.position, 0u);
#endif
    if (scene_depth > 1e-6) {
        let view_z = -depth_ndc_to_view_z(scene_depth);
        let view_forward = normalize(-view.world_from_view[2].xyz);
        let ray_cos = max(dot(ray_dir, view_forward), 1e-4);
        scene_distance = view_z / ray_cos;
        t_exit = min(t_exit, max(0.0, scene_distance));
        if (t_exit <= t_enter + 1e-3) {
            discard;
        }
    }
#endif

    // 28-step front-to-back Beer-Lambert raymarching with O(1) hardware-sampled noise and empty-space skipping
    let num_steps = 28u;
    let step_size = (t_exit - t_enter) / f32(num_steps);
    var transmittance = 1.0;
    var accum_color = vec3<f32>(0.0);

    let light_dir = normalize(smog.light_dir_intensity.xyz);
    let cos_theta = dot(ray_dir, light_dir);
    let phase = hg_phase(cos_theta, smog.warm_tint_anisotropy.w);
    let extinction_scale = smog.volume_max.w;
    let inv_extent = 1.0 / (smog.volume_max.xyz - smog.volume_min.xyz);

    var t = t_enter + step_size * 0.5;

    for (var step = 0u; step < num_steps; step += 1u) {
        let sample_pos = ray_origin + ray_dir * t;

        let bubble_count = min(u32(smog.control_params.x), 16u);

        let uvw = clamp((sample_pos - smog.volume_min.xyz) * inv_extent, vec3<f32>(0.0), vec3<f32>(1.0));

        // Fast empty-space skip: sample density texture first before sampling velocity or noise
        let density_sample = textureSampleLevel(smog_density_tex, smog_density_sampler, uvw, 0.0);
        if (density_sample.a < 0.018) {
            t += step_size;
            continue;
        }

        // Soft boundary fade against container walls with tightened floor fade
        let wall_dist = min(uvw, vec3<f32>(1.0) - uvw);
        let horiz_fade = smoothstep(0.0, 0.08, min(wall_dist.x, wall_dist.z));
        let ceil_fade = smoothstep(0.0, 0.08, 1.0 - uvw.y);
        let floor_fade = smoothstep(0.0, 0.012, uvw.y);
        let wall_fade = horiz_fade * ceil_fade * floor_fade;

        // Sample velocity and two-octave domain-warped hardware-trilinear wispy noise
        let velocity_sample = textureSampleLevel(smog_velocity_tex, smog_velocity_sampler, uvw, 0.0);
        let u = (velocity_sample.xyz - vec3<f32>(0.5)) * 10.0;
        var glow = velocity_sample.w;

        let time = smog.base_tint_time.w;
        let noise_uvw1 = sample_pos * 0.36 - u * 0.16 + vec3<f32>(time * 0.04, time * 0.11, -time * 0.03);
        let n1 = textureSampleLevel(smog_noise_tex, smog_noise_sampler, noise_uvw1, 0.0);
        let warp = (n1.ba - vec2<f32>(0.5, 0.5)) * 0.28;
        let noise_uvw2 = sample_pos * 0.88 - u * 0.30 + vec3<f32>(warp.x - time * 0.05, warp.y * 0.5 + time * 0.19, -warp.x + time * 0.04);
        let n2 = textureSampleLevel(smog_noise_tex, smog_noise_sampler, noise_uvw2, 0.0);

        let filament = clamp(n1.r * 0.54 + n2.g * 0.46, 0.0, 1.0);
        let wisp_mask = mix(
            0.20 + 1.42 * filament * filament,
            0.72 + 0.56 * filament,
            clamp(glow * 0.65, 0.0, 0.72)
        );
        let clean_a = max(density_sample.a - 0.018, 0.0) / 0.982;
        let raw_d = pow(clean_a, 0.82);
        var density = raw_d * 2.45 * wisp_mask * wall_fade;

        // 3D Spherical Interior Clearance (100% invariant under direction/velocity changes and multi-tier altitudes)
        for (var b = 0u; b < bubble_count; b = b + 1u) {
            let b_pos_rad = smog.bubble_pos_radius[b];
            let b_rad = b_pos_rad.w;
            if (b_rad <= 0.01) {
                continue;
            }
            let r_3d = sample_pos - b_pos_rad.xyz;
            let dist_3d_sq = dot(r_3d, r_3d);
            let clear_rad = b_rad * 0.85;
            if (dist_3d_sq < clear_rad * clear_rad) {
                let dist_3d = sqrt(dist_3d_sq);
                density *= smoothstep(b_rad * 0.42, clear_rad, dist_3d);
            }
        }

        var gen_color = density_sample.rgb;

        // Zero-Mud strengthened chroma normalization (Section 9.9.2)
        let g_min = min(min(gen_color.r, gen_color.g), gen_color.b);
        let g_max = max(max(gen_color.r, gen_color.g), gen_color.b);
        let sat_color = clamp(
            (gen_color - vec3<f32>(g_min * 0.88)) / max(g_max - g_min * 0.88, 0.06),
            vec3<f32>(0.0),
            vec3<f32>(1.0)
        );
        let tail_perm = vec3<f32>(sat_color.b, sat_color.r, sat_color.g);
        let color_t = clamp(glow * 1.30 + filament * 0.42 + density_sample.a * 0.55 - 0.18, 0.0, 1.0);
        let base_hue = mix(tail_perm, sat_color, color_t);
        let core_glow = mix(sat_color, vec3<f32>(1.0, 0.96, 0.85), 0.35) * (glow * 1.65);
        let lit_radiance = base_hue * (0.72 + 0.25 * uvw.y + 0.25 * phase) + core_glow;
        let voxel_radiance = mix(sat_color * 0.58, lit_radiance, clamp(0.42 + 0.58 * glow, 0.0, 1.0));

        // Soft depth feathering against scene depth geometry (e.g. bubbles and terrain)
        let depth_fade = smoothstep(0.0, 0.35, scene_distance - t);
        let optical_depth = density * extinction_scale * step_size * depth_fade;
        let step_transmittance = exp(-optical_depth);
        let powder = 1.0 - exp(-optical_depth * 2.8);

        accum_color += transmittance * (1.0 - step_transmittance) * voxel_radiance * (0.80 + 0.20 * powder);
        transmittance *= step_transmittance;

        if (transmittance < 0.01) {
            break;
        }

        t += step_size;
    }

    let opacity = 1.0 - transmittance;
    if (opacity < 0.002) {
        discard;
    }

    let final_opacity = min(opacity * 0.84, 0.46);
    let unmultiplied_color = accum_color / max(opacity, 1e-4);
    var final_color = vec4<f32>(unmultiplied_color, final_opacity);

#ifdef TONEMAP_IN_SHADER
    final_color = tone_mapping(final_color, view.color_grading);
#endif

    return vec4<f32>(final_color.rgb * final_opacity, final_opacity);
}
