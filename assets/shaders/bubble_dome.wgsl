// assets/shaders/bubble_dome.wgsl
// Unified shader for Mobile Unit Spheres and Building Hemisphere Domes in Space Scuffle.
// Implements Plan 20:
// - Camera view-plane basis from view.world_from_view matrix (screen_right, screen_up, screen_forward)
// - Screen-space 1:1 circular disc projection discarding perspective-stretched elliptical ears
// - Zero-tilt character orientation and studio rim specular highlights
// - Directional indicator re-engineered as a sleek rim arc confined strictly to outer Fresnel annulus [0.86, 0.98]
// - Dynamic video texture array sampling with spherical lens magnification
// - Flipbook layer chosen from the shared flipbook clock and per-bubble lanes, with frame blending
// - Voronoi spiderweb glass fractures & CRT scanlines

// - Effect shell (plan 2026-11-09-effect-visualization WP8, binding 4): the vertex stage sinks the
//   bubble into the ground (the terrain's depth hides what is below it); the fragment stage stains
//   it with soil near the ground, tints it, lights its outline and veils its media. All zero is
//   the bubble as it was.

#import bevy_pbr::mesh_view_bindings::{view, globals}
#import bevy_pbr::forward_io::{Vertex, VertexOutput}
#import bevy_pbr::mesh_functions
#import bevy_pbr::view_transformations::position_world_to_clip
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

struct BubbleUniform {
    team_color: vec4<f32>,
    world_pos: vec3<f32>,
    radius: f32,
    shape_kind: u32,             // 0 = UnitSphere, 1 = BuildingDome
    heading_angle_rad: f32,
    aim_angle_rad: f32,
    is_attacking: u32,
    anim_state: u32,
    anim_origin: u32,            // Flipbook clock time of elapsed zero (offset 52)
    hp_ratio: f32,               // 1.0 = pristine, <0.75 = cracks, <0.35 = critical
    shield_ratio: f32,           // 0.0..1.0 outer sheen
    shield_hit_flash: f32,       // Decaying ripple intensity
    hull_hit_angle_rad: f32,     // Direction of last damage impact
    hull_hit_flash: f32,         // Decaying impact flare
    build_progress: f32,         // Built share of the shell's height while anim_state is Constructing (0.0..1.0)
    selected: u32,
    cloaked: u32,
    feature_flags: u32,          // Bitmask controlling modular optics (offset 88)
    frame_base: u32,             // First layer of the slice, or the fixed layer (offset 92)
    optics_media: vec4<f32>,     // exposure stops, F0, Fresnel power, reflection strength
    optics_glare: vec4<f32>,     // roughness, glare strength, hit strength, angular width
    optics_signals: vec4<f32>,   // selection strength, alpha, facing strength, attack strength
    optics_light: vec4<f32>,     // world direction toward light, normalized illuminance
    anim_period: u32,            // Clock units of one pass over the slice (offset 160)
    frame_count: u32,            // Layers in the slice
    anim_mode: u32,              // Mode in bits 0-1 (ANIM_MODE_*), blend request in bit 2
    anim_offset: u32,            // Loop phase offset in clock units, below anim_period
};

// The shared flipbook clock (client/src/flipbook_clock.rs), written in place once per frame.
struct FlipbookClock {
    now: u32,                    // Presented time in 1/1024 simulation tick
    pad0: u32,
    pad1: u32,
    pad2: u32,
};

@group(2) @binding(0) var<uniform> bubble: BubbleUniform;
@group(2) @binding(1) var video_texture: texture_2d_array<f32>;
@group(2) @binding(2) var video_sampler: sampler;
@group(2) @binding(3) var<storage, read> flipbook_clock: FlipbookClock;

// client::bubble_shader::ShellUniformGpu: the effects' modulation of this bubble.
struct ShellUniform {
    tint: vec4<f32>,             // linear RGB, share of the media replaced
    rim: vec4<f32>,              // linear RGB of the outline light, strength
    shape: vec4<f32>,            // sink (world units), veil (0..1), soil height (world units), ground y
    driven: vec4<f32>,           // x = 1 when effects drive the concealment look
};

@group(2) @binding(4) var<uniform> shell: ShellUniform;

// The mesh vertex of bevy_pbr (mesh.wgsl) without skinning or morph targets, which bubbles never
// have, moved down by the shell's sink. With no sink it computes exactly what Bevy's does.
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
#endif
#ifdef VERTEX_POSITIONS
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.world_position.y -= shell.shape.x;
    out.position = position_world_to_clip(out.world_position.xyz);
#endif
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex.instance_index);
#endif
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(vertex.instance_index, world_from_local[3]);
#endif
    return out;
}

// Modular Feature Bitflags
const FLAG_CLOAKED: u32 = 1u;         // 1 << 0
const FLAG_SELECTED: u32 = 2u;        // 1 << 1
const FLAG_GLARE: u32 = 4u;           // 1 << 2: Studio crescent rim specular reflections
const FLAG_FRESNEL: u32 = 8u;         // 1 << 3: Iridescent thin-film Fresnel rim
const FLAG_INNER_WARP: u32 = 16u;     // 1 << 4: View-aligned spherical lens warp
const FLAG_CRACKS: u32 = 32u;         // 1 << 5: Voronoi glass cracks & electrical arcing
const FLAG_CRT_GLITCH: u32 = 64u;     // 1 << 6: CRT scanlines & chromatic aberration
const FLAG_SPEED_BOOSTED: u32 = 128u; // 1 << 7: Pulsing rim trailing a boosted unit
const FLAG_FACING_STRIPE: u32 = 256u; // 1 << 8: Rim-confined facing indicator
const FLAG_ATTACK_APERTURE: u32 = 512u;// 1 << 9: Target-lock weapon aperture
const FLAG_FRAME_BLEND: u32 = 1024u;   // 1 << 10: Blend consecutive flipbook layers of a loop

// Flipbook modes in bits 0-1 of anim_mode (client::flipbook_clock::FlipbookMode)
const ANIM_MODE_FIXED: u32 = 0u;      // always frame_base
const ANIM_MODE_LOOP: u32 = 1u;       // cycles frame_count layers once per anim_period
const ANIM_MODE_ONCE_HOLD: u32 = 2u;  // plays once, then holds the last layer
const ANIM_MODE_ONCE: u32 = 3u;       // plays once, then shows frame_base again
const ANIM_MODE_MASK: u32 = 3u;
const ANIM_BLEND_BIT: u32 = 4u;       // bit 2: this loop asks for frame blending

// The layers a bubble shows at one clock time.
struct FlipbookSample {
    layer: u32,
    next_layer: u32,          // layer blended in; equals layer when not blending
    blend_numerator: u32,     // weight of next_layer = blend_numerator / blend_denominator
    blend_denominator: u32,   // 0 when not blending
};

// Mirror of client::flipbook_clock::flipbook_sample, operation for operation. Every value is a
// u32 and WGSL u32 arithmetic wraps, as the Rust mirror's wrapping operations do, so radv,
// lavapipe and the CPU choose the same layer at every frame boundary. Floats near k/count would not.
fn flipbook_select(
    now: u32,
    origin: u32,
    base: u32,
    period: u32,
    count: u32,
    mode_bits: u32,
    offset: u32,
) -> FlipbookSample {
    var sample = FlipbookSample(base, base, 0u, 0u);
    let mode = mode_bits & ANIM_MODE_MASK;
    if (mode == ANIM_MODE_FIXED || period == 0u || count == 0u) {
        return sample;
    }
    let elapsed = now - origin;
    if (mode == ANIM_MODE_LOOP) {
        let phase = ((elapsed % period) + offset) % period;
        let scaled = phase * count;
        let step = scaled / period;
        sample.layer = base + step;
        sample.next_layer = sample.layer;
        if ((mode_bits & ANIM_BLEND_BIT) != 0u) {
            sample.next_layer = base + (step + 1u) % count;
            sample.blend_numerator = scaled % period;
            sample.blend_denominator = period;
        }
        return sample;
    }
    if (mode != ANIM_MODE_ONCE_HOLD && elapsed >= period) {
        return sample;
    }
    let clamped = min(elapsed, period - 1u);
    sample.layer = base + clamped * count / period;
    sample.next_layer = sample.layer;
    return sample;
}

// One media sample of layer `layer`, with the CRT channel split by `uv_offset`.
fn sample_media(uv: vec2<f32>, uv_offset: vec2<f32>, layer: u32) -> vec3<f32> {
    let sample_r = textureSample(video_texture, video_sampler, uv + uv_offset, layer).r;
    let sample_g = textureSample(video_texture, video_sampler, uv, layer).g;
    let sample_b = textureSample(video_texture, video_sampler, uv - uv_offset, layer).b;
    return vec3<f32>(sample_r, sample_g, sample_b);
}

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
    return 1.0 - smoothstep(0.01, 0.04, min_dist);
}

// Share of a shell's height below this fragment: 0 at the base, 1 at the top. A dome stands on its
// centre, so only its upper half counts. UVs cannot give this: Bevy's UV sphere has its poles on Z.
fn construction_height(world_y: f32, center_y: f32, radius: f32, shape_kind: u32) -> f32 {
    let height = (world_y - center_y) / max(radius, 0.001);
    if (shape_kind == 1u) {
        return clamp(height, 0.0, 1.0);
    }
    return clamp(0.5 * (height + 1.0), 0.0, 1.0);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let view_dir = normalize(view.world_position - in.world_position.xyz);
    let normal = normalize(in.world_normal);

    // 1. Camera View-Plane Basis (Constant across viewport from view.world_from_view matrix)
    let screen_right = normalize(view.world_from_view[0].xyz);
    let screen_up = normalize(view.world_from_view[1].xyz);
    let screen_forward = -normalize(view.world_from_view[2].xyz);

    // Exact sphere center in world space
    let sphere_center = in.world_position.xyz - normal * bubble.radius;
    let p_rel = in.world_position.xyz - view.world_position;
    let c_rel = sphere_center - view.world_position;

    // Optical depth along camera optical axis
    let z_p = max(dot(p_rel, screen_forward), 0.001);
    let z_c = max(dot(c_rel, screen_forward), 0.001);

    // Project fragment onto camera plane at depth z_c (plane through sphere center perpendicular to screen_forward)
    let p_screen_at_zc = p_rel * (z_c / z_p) - c_rel;
    let screen_disk = vec2<f32>(
        dot(p_screen_at_zc, screen_right),
        -dot(p_screen_at_zc, screen_up)
    ) / max(bubble.radius, 0.001);

    var disk: vec2<f32>;
    var r2: f32;
    var r: f32;

    if (bubble.shape_kind == 0u) {
        // Mobile Units: Discard perspective-stretched elliptical ears outside true 1:1 screen-space circle!
        let screen_r2 = dot(screen_disk, screen_disk);
        if (screen_r2 > 1.0) {
            discard;
        }
        disk = screen_disk;
        r2 = screen_r2;
        r = sqrt(r2);
    } else {
        // Building Domes: Grounded 3D hemisphere projection
        disk = vec2<f32>(dot(normal, screen_right), -dot(normal, screen_up));
        let n_dot_v = max(dot(normal, view_dir), 0.0);
        r2 = clamp(1.0 - n_dot_v * n_dot_v, 0.0, 1.0);
        r = sqrt(r2);
    }

    let disk_dir = select(vec2<f32>(1.0, 0.0), disk / max(length(disk), 0.0001), length(disk) > 0.0001);

    // Optical glass lens refraction and magnification
    let z_lens = sqrt(max(1.0 - r2, 0.0));
    // With FLAG_INNER_WARP the media magnifies toward the centre like a spherical lens; without it
    // the media maps flat across the disc.
    var lens_scale = 0.48;
    if ((bubble.feature_flags & FLAG_INNER_WARP) != 0u) {
        lens_scale = mix(0.48, 0.35, z_lens);
    }
    var lens_uv = vec2<f32>(0.5, 0.5) + disk * lens_scale;

    // Facing-aware horizontal mirroring (aligned with screen_right)
    let heading_dir = vec3<f32>(cos(bubble.heading_angle_rad), 0.0, sin(bubble.heading_angle_rad));
    if (dot(heading_dir, screen_right) < 0.0) {
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

    // Texture Array Sampling: the flipbook layer comes from the shared clock and this bubble's
    // lanes, which change only with its action, never with time (plan D-2).
    let flipbook = flipbook_select(
        flipbook_clock.now,
        bubble.anim_origin,
        bubble.frame_base,
        bubble.anim_period,
        bubble.frame_count,
        bubble.anim_mode,
        bubble.anim_offset,
    );
    var media = sample_media(lens_uv, uv_offset, flipbook.layer);
    // Frame blending (plan D-6): a loop that asks for it, on a material with the feature on, mixes
    // in the next layer by the sub-frame time. The condition reads only the bubble uniform, so the
    // three extra samples stay in uniform control flow and cost nothing for other bubbles.
    let blend_frames = (bubble.feature_flags & FLAG_FRAME_BLEND) != 0u
        && (bubble.anim_mode & ANIM_MODE_MASK) == ANIM_MODE_LOOP
        && (bubble.anim_mode & ANIM_BLEND_BIT) != 0u
        && bubble.anim_period != 0u
        && bubble.frame_count != 0u;
    if (blend_frames) {
        let weight = f32(flipbook.blend_numerator) / f32(flipbook.blend_denominator);
        media = mix(media, sample_media(lens_uv, uv_offset, flipbook.next_layer), weight);
    }
    var video_color = media * exp2(bubble.optics_media.x) * scanline_mult * lens_mask * glass_depth_tint;

    // Progressive Construction Scanline Fill
    if (bubble.anim_state == 0u) { // Constructing
        let fill_height = construction_height(in.world_position.y, sphere_center.y, bubble.radius, bubble.shape_kind);
        if (fill_height > bubble.build_progress) {
            let grid_line = step(0.92, fract(fill_height * 20.0)) + step(0.92, fract(in.uv.x * 20.0));
            video_color = vec3<f32>(0.1, 0.6, 1.0) * min(grid_line, 1.0);
        } else if (abs(fill_height - bubble.build_progress) < 0.02) {
            video_color += vec3<f32>(1.0, 0.9, 0.4) * 0.8;
        }
    }

    // Linear-light reflection/transmission budget. Hardware sRGB texture sampling
    // already decoded the video; no second gamma lift is applied to the media.
    let view_cosine = sqrt(max(1.0 - r2, 0.0));
    let fresnel = bubble.optics_media.y + (1.0 - bubble.optics_media.y)
        * pow(1.0 - view_cosine, bubble.optics_media.z);
    var F = 0.0;
    var rim_color = vec3<f32>(0.0);
    if ((bubble.feature_flags & FLAG_FRESNEL) != 0u) {
        // Spectral reflection is confined to the grazing contour. Interior media
        // retains its authored color instead of being washed by a whole-shell dye.
        F = fresnel * smoothstep(0.93, 0.985, r);
        // Optical thickness varies around the shell, so neighboring grazing angles
        // reflect different wavelengths instead of producing a uniform colored border.
        let film_phase = (1.0 - view_cosine) * 5.4
            + dot(normal, bubble.optics_light.xyz) * 2.8
            + dot(normal, screen_up) * 1.4;
        let iridescence = 0.5 + 0.5 * cos(vec3<f32>(0.0, 2.0944, 4.1888) + film_phase);
        let film_color = mix(vec3<f32>(0.78, 0.86, 0.94), mix(bubble.team_color.rgb, iridescence, 0.55), 0.38);
        rim_color = mix(film_color, vec3<f32>(0.85, 0.93, 1.0),
            pow(max(dot(normal, bubble.optics_light.xyz), 0.0), 6.0) * 0.35)
            * bubble.optics_media.w;
    }

    // Compact glossy reflection responds to the actual scene light. The lobe is
    // distinct from the thin Fresnel edge instead of a second broad luminous annulus.
    var specular = 0.0;
    if ((bubble.feature_flags & FLAG_GLARE) != 0u) {
        let halfway = normalize(view_dir + bubble.optics_light.xyz);
        let exponent = max(2.0 / (bubble.optics_glare.x * bubble.optics_glare.x) - 2.0, 1.0);
        specular = pow(max(dot(normal, halfway), 0.0), exponent)
            * bubble.optics_glare.y * bubble.optics_light.w;
    }

    // 4. Directional Indicator: Sleek Rim Arc Confined Strictly to Outer Fresnel Annulus [0.86, 0.98]
    // NEVER draw lines across the interior (r < 0.85) of the sphere!
    var direction_stripe = vec3<f32>(0.0);
    if (bubble.shape_kind == 0u && (bubble.feature_flags & FLAG_FACING_STRIPE) != 0u) {
        let heading_2d = normalize(vec2<f32>(dot(heading_dir, screen_right), -dot(heading_dir, screen_up)));
        let align = dot(disk_dir, heading_2d);

        let rim_annulus = smoothstep(0.92, 0.96, r) * (1.0 - smoothstep(0.98, 0.998, r));
        let angular_arc = smoothstep(0.65, 0.96, align);
        let pulse = sin(globals.time * 5.0 - (1.0 - align) * 4.0) * 0.25 + 0.75;
        let nose_accent = angular_arc * rim_annulus * (0.7 + 0.3 * pulse);

        direction_stripe = mix(bubble.team_color.rgb, vec3<f32>(1.0, 1.0, 1.0), 0.40) * nose_accent * bubble.optics_signals.z;
    }

    // 5. Target-Lock Weapon Aperture on Glass Surface
    var aim_glow = vec3<f32>(0.0);
    if (bubble.is_attacking == 1u && (bubble.feature_flags & FLAG_ATTACK_APERTURE) != 0u) {
        let aim_dir = vec3<f32>(cos(bubble.aim_angle_rad), 0.0, sin(bubble.aim_angle_rad));
        let aim_dot = dot(normal, aim_dir);
        let ring_aperture = (1.0 - smoothstep(0.008, 0.035, abs(aim_dot - 0.72)));
        let core_aperture = smoothstep(0.94, 0.99, aim_dot);
        aim_glow = vec3<f32>(1.0, 0.35, 0.15) * (ring_aperture + core_aperture * 0.45) * bubble.optics_signals.w;
    }

    // 6. Progressive Damage Cracks & Electrical Arcing
    var crack_glow = vec3<f32>(0.0);
    if ((bubble.feature_flags & FLAG_CRACKS) != 0u && bubble.hp_ratio < 0.75) {
        let crack_mask = voronoi_cracks(in.uv * 3.0);
        if (bubble.hp_ratio < 0.35) {
            let flicker = sin(globals.time * 60.0) * 0.5 + 0.5;
            crack_glow = vec3<f32>(1.0, 0.25, 0.05) * crack_mask * (0.35 + flicker * 0.45);
        } else {
            video_color *= (1.0 - crack_mask * 0.7);
        }
    }

    // Damage signals consume a localized highlight budget, retaining media detail.
    // Shield snapshots have no impact angle, so their response is a narrow moving ring.
    var shield_surge = vec3<f32>(0.0);
    if (bubble.shield_hit_flash > 0.0) {
        let wave = 0.45 + (1.0 - bubble.shield_hit_flash) * 0.50;
        let ring = 1.0 - smoothstep(0.018, 0.060, abs(r - wave));
        shield_surge = vec3<f32>(0.2, 0.85, 1.0) * ring
            * bubble.shield_hit_flash * bubble.optics_glare.z;
    }
    var hull_surge = vec3<f32>(0.0);
    if (bubble.hull_hit_flash > 0.0) {
        let impact = vec3<f32>(cos(bubble.hull_hit_angle_rad), 0.0, sin(bubble.hull_hit_angle_rad));
        let lobe = pow(smoothstep(cos(bubble.optics_glare.w), 1.0, dot(normal, impact)), 3.0);
        hull_surge = vec3<f32>(1.0, 0.32, 0.08) * lobe
            * bubble.hull_hit_flash * bubble.optics_glare.z;
    }

    // 7. Speed Boost: a warm pulsing arc on the rim behind the direction of travel.
    var boost_glow = vec3<f32>(0.0);
    if (bubble.shape_kind == 0u && (bubble.feature_flags & FLAG_SPEED_BOOSTED) != 0u) {
        let heading_2d = normalize(vec2<f32>(dot(heading_dir, screen_right), -dot(heading_dir, screen_up)));
        let trailing = smoothstep(0.15, 0.90, -dot(disk_dir, heading_2d));
        let pulse = 0.75 + 0.25 * sin(globals.time * 14.0);
        boost_glow = vec3<f32>(1.0, 0.62, 0.14) * trailing * smoothstep(0.80, 0.96, r)
            * pulse * bubble.optics_signals.z;
    }

    // 8. Tactical Selection Outer Rim Aura
    var selection_glow = vec3<f32>(0.0);
    let is_selected = ((bubble.feature_flags & FLAG_SELECTED) != 0u) || (bubble.selected == 1u);
    if (is_selected) {
        let pulse = 0.85 + 0.15 * sin(6.0 * globals.time);
        selection_glow = vec3<f32>(0.25, 0.85, 1.0) * pulse * smoothstep(0.94, 0.985, r) * bubble.optics_signals.x;
    }

    // 9. Effect shell (plan 2026-11-09-effect-visualization WP8): the tint on the media keeps
    // the media's own shading. The soil stain (composited after the veil, so a veiled body keeps
    // its caked earth) is the tint's colour, cracked like dried mud, full at the ground and gone
    // at the stain's height along an edge the crack pattern breaks up.
    var soil_stain = 0.0;
    var soil_rgb = vec3<f32>(0.0);
    if (shell.tint.a > 0.0) {
        let shade = dot(video_color, vec3<f32>(0.2126, 0.7152, 0.0722));
        let tinted = shell.tint.rgb * (0.35 + 0.65 * min(shade * 2.0, 1.0));
        video_color = mix(video_color, tinted, shell.tint.a);
    }
    if (shell.shape.z > 0.0) {
        let cracks = voronoi_cracks(in.uv * 5.0);
        let above = in.world_position.y - shell.shape.w;
        let ragged = above + (voronoi_cracks(in.uv * 2.0) - 0.5) * 0.3 * shell.shape.z;
        soil_stain = 1.0 - smoothstep(0.0, shell.shape.z, ragged);
        soil_rgb = shell.tint.rgb * (1.0 - 0.45 * cracks);
    }

    // 10. Final Composite & Cloak Interference
    let signals = direction_stripe + aim_glow + crack_glow + shield_surge + hull_surge
        + boost_glow + selection_glow;
    let signal_peak = max(signals.r, max(signals.g, signals.b));
    let bounded_signals = signals / (1.0 + signal_peak);
    var final_rgb = video_color * (1.0 - F) + rim_color * F
        + vec3<f32>(specular) + bounded_signals;
    var final_alpha = mix(bubble.optics_signals.y, 1.0, F);

    // The shell's veil and outline light: a veiled bubble keeps only its glass (Fresnel edge,
    // glare and signals) and a soft outline light, so its owner sees a clear, hollow shape.
    if (shell.shape.y > 0.0 || shell.rim.a > 0.0) {
        let outline = smoothstep(0.78, 0.97, r) * (1.0 - smoothstep(0.985, 1.0, r));
        final_rgb = mix(final_rgb, final_rgb - video_color * (1.0 - F), shell.shape.y);
        final_alpha = mix(final_alpha, 0.12 + F * 0.45, shell.shape.y);
        final_rgb += shell.rim.rgb * shell.rim.a * outline;
        final_alpha = max(final_alpha, shell.rim.a * outline * 0.6);
    }
    if (soil_stain > 0.0) {
        final_rgb = mix(final_rgb, soil_rgb, soil_stain);
        final_alpha = mix(final_alpha, 0.8, soil_stain);
    }

    let is_cloaked = ((bubble.feature_flags & FLAG_CLOAKED) != 0u) || (bubble.cloaked == 1u);
    if (is_cloaked && shell.driven.x < 0.5) {
        final_alpha = 0.15 + F * 0.25;
        final_rgb += vec3<f32>(0.1, 0.3, 0.5) * sin(globals.time * 10.0 + in.world_position.y * 5.0) * 0.5;
    }

    var output = vec4<f32>(max(final_rgb, vec3<f32>(0.0)), final_alpha);
#ifdef TONEMAP_IN_SHADER
    // Non-HDR targets need the same grading/tonemapping contract as Bevy PBR.
    // HDR cameras tone-map later. The sRGB target performs the output encoding.
    output = tone_mapping(output, view.color_grading);
#endif
    return output;
}
