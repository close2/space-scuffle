// assets/shaders/bubble_dome.wgsl
// Unified shader for Mobile Unit Spheres and Building Hemisphere Domes in Space Scuffle.
// Implements Plan 19 Perfectly Round 3D Crystal Sphere Optics:
// - Camera-to-sphere-center orthonormal basis invariant across all fragments of the sphere
// - Perspective-correct silhouette horizon: r = sqrt(1.0 - n_dot_v^2) reaches 1.0 at 360-degree boundary
// - Studio crescent rim specular highlights (upper-left key & lower-right bounce) leaving center 100% crystal clear
// - Directional indicator re-engineered as a sleek rim arc confined strictly to outer Fresnel annulus [0.86, 0.98]
// - Dynamic video texture array sampling with spherical lens magnification
// - Voronoi spiderweb glass fractures & CRT scanlines

#import bevy_pbr::mesh_view_bindings::{view, globals}
#import bevy_pbr::forward_io::VertexOutput

struct BubbleUniform {
    team_color: vec4<f32>,
    world_pos: vec3<f32>,
    radius: f32,
    shape_kind: u32,             // 0 = UnitSphere, 1 = BuildingDome
    heading_angle_rad: f32,
    aim_angle_rad: f32,
    is_attacking: u32,
    anim_state: u32,
    anim_progress: f32,          // Normalized progress (0.0..1.0)
    hp_ratio: f32,               // 1.0 = pristine, <0.75 = cracks, <0.35 = critical
    shield_ratio: f32,           // 0.0..1.0 outer sheen
    shield_hit_flash: f32,       // Decaying ripple intensity
    hull_hit_angle_rad: f32,     // Direction of last damage impact
    hull_hit_flash: f32,         // Decaying impact flare
    build_progress: f32,         // Vertical construction scanline fill (0.0..1.0)
    selected: u32,
    cloaked: u32,
    feature_flags: u32,          // Bitmask controlling modular optics (offset 88)
    frame_index: u32,            // Exact active frame in texture array (offset 92)
};

@group(2) @binding(0) var<uniform> bubble: BubbleUniform;
@group(2) @binding(1) var video_texture: texture_2d_array<f32>;
@group(2) @binding(2) var video_sampler: sampler;

// Modular Feature Bitflags
const FLAG_CLOAKED: u32 = 1u;         // 1 << 0
const FLAG_SELECTED: u32 = 2u;        // 1 << 1
const FLAG_GLARE: u32 = 4u;           // 1 << 2: Studio crescent rim specular reflections
const FLAG_FRESNEL: u32 = 8u;         // 1 << 3: Iridescent thin-film Fresnel rim
const FLAG_INNER_WARP: u32 = 16u;     // 1 << 4: View-aligned spherical lens warp
const FLAG_CRACKS: u32 = 32u;         // 1 << 5: Voronoi glass cracks & electrical arcing
const FLAG_CRT_GLITCH: u32 = 64u;     // 1 << 6: CRT scanlines & chromatic aberration
const FLAG_SPEED_BOOSTED: u32 = 128u; // 1 << 7: Hyper wake glow
const FLAG_FACING_STRIPE: u32 = 256u; // 1 << 8: Rim-confined facing indicator
const FLAG_ATTACK_APERTURE: u32 = 512u;// 1 << 9: Target-lock weapon aperture

// Procedural 2D hash for Voronoi glass cracks
fn hash2(p: vec2<f32>) -> vec2<f32> {
    let q = vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)));
    return fract(sin(q) * 43758.5453);
}

// Computes Voronoi fracture distance
fn voronoi_cracks(uv: vec2<f32>) -> f32 {
    let g = floor(uv * 8.0);
    let f = fract(uv * 8.0);
    var min_dist = 1.0;

    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let neighbor = vec2<f32>(f32(x), f32(y));
            let point = hash2(g + neighbor);
            let diff = neighbor + point - f;
            min_dist = min(min_dist, length(diff));
        }
    }
    return smoothstep(0.04, 0.01, min_dist);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let view_dir = normalize(view.world_position - in.world_position.xyz);
    let normal = normalize(in.world_normal);

    // Exact sphere center in world space (invariant across all fragments of the sphere!)
    let sphere_center = in.world_position.xyz - normal * bubble.radius;
    let cam_forward = normalize(sphere_center - view.world_position);
    let cam_right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), cam_forward));
    let cam_up = cross(cam_forward, cam_right);

    let disk = vec2<f32>(dot(normal, cam_right), -dot(normal, cam_up));
    let n_dot_v = max(dot(normal, view_dir), 0.0);
    let r2 = clamp(1.0 - n_dot_v * n_dot_v, 0.0, 1.0);
    let r = sqrt(r2);
    let disk_dir = select(vec2<f32>(1.0, 0.0), disk / max(length(disk), 0.0001), length(disk) > 0.0001);

    // Optical glass lens refraction and magnification
    let z_lens = sqrt(max(1.0 - r2, 0.0));
    var lens_uv = vec2<f32>(0.5, 0.5) + disk * mix(0.48, 0.35, z_lens);

    // Facing-aware horizontal mirroring
    let heading_dir = vec3<f32>(cos(bubble.heading_angle_rad), 0.0, sin(bubble.heading_angle_rad));
    if (dot(heading_dir, cam_right) < 0.0) {
        lens_uv.x = 1.0 - lens_uv.x;
    }

    // Lens aperture vignette & edge depth darkening (kicks in only near glass rim r > 0.85)
    let lens_mask = 1.0 - smoothstep(0.88, 0.99, r);
    let glass_depth_tint = 1.0 - smoothstep(0.80, 0.99, r) * 0.40;

    // CRT Scanlines & Glitch Aberration
    var scanline_mult = 1.0;
    var uv_offset = vec2<f32>(0.0);
    if ((bubble.feature_flags & FLAG_CRT_GLITCH) != 0u) {
        if (bubble.hp_ratio < 0.50) {
            scanline_mult = 0.85 + 0.15 * sin(lens_uv.y * 240.0);
        }
        if (bubble.hp_ratio < 0.35) {
            let glitch_jitter = sin(globals.time * 40.0) * 0.015;
            uv_offset = vec2<f32>(0.02 + glitch_jitter, 0.0);
        }
    }

    // Texture Array Sampling
    let layer_index = i32(bubble.frame_index);
    let sample_r = textureSample(video_texture, video_sampler, lens_uv + uv_offset, layer_index).r;
    let sample_g = textureSample(video_texture, video_sampler, lens_uv, layer_index).g;
    let sample_b = textureSample(video_texture, video_sampler, lens_uv - uv_offset, layer_index).b;
    var video_color = pow(vec3<f32>(sample_r, sample_g, sample_b), vec3<f32>(0.9)) * 1.45 * scanline_mult * lens_mask * glass_depth_tint;

    // Progressive Construction Scanline Fill
    if (bubble.anim_state == 0u) { // Constructing
        if (in.uv.y > bubble.build_progress) {
            let grid_line = step(0.92, fract(in.uv.y * 20.0)) + step(0.92, fract(in.uv.x * 20.0));
            video_color = vec3<f32>(0.1, 0.6, 1.0) * grid_line * 2.0;
        } else if (abs(in.uv.y - bubble.build_progress) < 0.02) {
            video_color += vec3<f32>(1.0, 0.9, 0.4) * 3.0;
        }
    }

    // 2. Crystal-Clear Center with Steep Iridescent Fresnel Rim
    let F = pow(r, 3.8);
    var rim_color = vec3<f32>(0.0);
    if ((bubble.feature_flags & FLAG_FRESNEL) != 0u) {
        let iridescence = vec3<f32>(
            0.5 + 0.5 * cos(6.28318 * (F + 0.0)),
            0.5 + 0.5 * cos(6.28318 * (F + 0.333)),
            0.5 + 0.5 * cos(6.28318 * (F + 0.667))
        );
        rim_color = mix(bubble.team_color.rgb, iridescence, 0.40) * F * 3.2;
    }

    // 3. Studio Crescent Rim Specular Highlights (Center r < 0.65 is 100% CLEAR!)
    var specular = 0.0;
    if ((bubble.feature_flags & FLAG_GLARE) != 0u) {
        // Upper-left key light crescent
        let l1 = normalize(cam_up * 0.70 - cam_right * 0.55 + cam_forward * 0.45);
        let H1 = normalize(view_dir + l1);
        let spec1 = pow(max(dot(normal, H1), 0.0), 36.0);
        let rim_mask1 = smoothstep(0.68, 0.88, r) * (1.0 - smoothstep(0.95, 1.0, r));
        let glare1 = spec1 * rim_mask1 * 2.4;

        // Lower-right ambient bounce crescent
        let l2 = normalize(-cam_up * 0.60 + cam_right * 0.50 + cam_forward * 0.35);
        let H2 = normalize(view_dir + l2);
        let spec2 = pow(max(dot(normal, H2), 0.0), 20.0);
        let rim_mask2 = smoothstep(0.78, 0.92, r) * (1.0 - smoothstep(0.97, 1.0, r));
        let glare2 = spec2 * rim_mask2 * 0.85;

        specular = glare1 + glare2;
    }

    // 4. Directional Indicator: Sleek Rim Arc Confined Strictly to Outer Fresnel Annulus [0.86, 0.98]
    // NEVER draw lines across the interior (r < 0.85) of the sphere!
    var direction_stripe = vec3<f32>(0.0);
    if (bubble.shape_kind == 0u && (bubble.feature_flags & FLAG_FACING_STRIPE) != 0u) {
        let heading_2d = normalize(vec2<f32>(dot(heading_dir, cam_right), -dot(heading_dir, cam_up)));
        let align = dot(disk_dir, heading_2d);

        let rim_annulus = smoothstep(0.86, 0.93, r) * (1.0 - smoothstep(0.96, 0.995, r));
        let angular_arc = smoothstep(0.65, 0.96, align);
        let pulse = sin(globals.time * 5.0 - (1.0 - align) * 4.0) * 0.25 + 0.75;
        let nose_accent = angular_arc * rim_annulus * (1.4 + 0.6 * pulse);

        direction_stripe = mix(bubble.team_color.rgb, vec3<f32>(1.0, 1.0, 1.0), 0.40) * nose_accent * 2.8;
    }

    // 5. Target-Lock Weapon Aperture on Glass Surface
    var aim_glow = vec3<f32>(0.0);
    if (bubble.is_attacking == 1u && (bubble.feature_flags & FLAG_ATTACK_APERTURE) != 0u) {
        let aim_dir = vec3<f32>(cos(bubble.aim_angle_rad), 0.0, sin(bubble.aim_angle_rad));
        let aim_dot = dot(normal, aim_dir);
        let ring_aperture = smoothstep(0.035, 0.008, abs(aim_dot - 0.72));
        let core_aperture = smoothstep(0.94, 0.99, aim_dot);
        aim_glow = vec3<f32>(1.0, 0.35, 0.15) * (ring_aperture * 3.2 + core_aperture * 1.8);
    }

    // 6. Progressive Damage Cracks & Electrical Arcing
    var crack_glow = vec3<f32>(0.0);
    if ((bubble.feature_flags & FLAG_CRACKS) != 0u && bubble.hp_ratio < 0.75) {
        let crack_mask = voronoi_cracks(in.uv * 3.0);
        if (bubble.hp_ratio < 0.35) {
            let flicker = sin(globals.time * 60.0) * 0.5 + 0.5;
            crack_glow = vec3<f32>(1.0, 0.25, 0.05) * crack_mask * (1.5 + flicker * 2.0);
        } else {
            video_color *= (1.0 - crack_mask * 0.7);
        }
    }

    // 7. Whole-Bubble Luminous Hit Surges
    var shield_surge = vec3<f32>(0.0);
    if (bubble.shield_hit_flash > 0.0) {
        let shield_pulse = sin(globals.time * 50.0) * 0.15 + 0.85;
        shield_surge = vec3<f32>(0.2, 0.85, 1.0) * bubble.shield_hit_flash * shield_pulse * (2.8 + pow(r, 2.5) * 4.0);
    }

    var hull_surge = vec3<f32>(0.0);
    if (bubble.hull_hit_flash > 0.0) {
        let hull_pulse = sin(globals.time * 60.0) * 0.20 + 0.80;
        hull_surge = vec3<f32>(1.0, 0.32, 0.08) * bubble.hull_hit_flash * hull_pulse * (3.6 + pow(r, 2.0) * 3.2);
    }

    // 8. Tactical Selection Outer Rim Aura
    var selection_glow = vec3<f32>(0.0);
    let is_selected = ((bubble.feature_flags & FLAG_SELECTED) != 0u) || (bubble.selected == 1u);
    if (is_selected) {
        let pulse = 0.85 + 0.15 * sin(6.0 * globals.time);
        selection_glow = vec3<f32>(0.25, 0.85, 1.0) * pulse * pow(r, 4.0) * 3.5;
    }

    // 9. Final Composite & Cloak Interference
    var final_rgb = video_color + rim_color + vec3<f32>(specular) + direction_stripe + aim_glow + crack_glow + shield_surge + hull_surge + selection_glow;
    var final_alpha = 0.90 + F * 0.10;

    let is_cloaked = ((bubble.feature_flags & FLAG_CLOAKED) != 0u) || (bubble.cloaked == 1u);
    if (is_cloaked) {
        final_alpha = 0.15 + F * 0.25;
        final_rgb += vec3<f32>(0.1, 0.3, 0.5) * sin(globals.time * 10.0 + in.world_position.y * 5.0) * 0.5;
    }

    return vec4<f32>(final_rgb, final_alpha);
}
