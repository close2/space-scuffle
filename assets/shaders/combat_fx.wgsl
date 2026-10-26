// assets/shaders/combat_fx.wgsl
// Procedural Volumetric Combat VFX Shader for Space Scuffle.
// Renders 6 distinct procedural energy modes:
// Mode 0 (FX_LASER_BEAM): Multi-layered plasma laser beam with white-hot filament core,
//                         traveling helical pulses, circular muzzle flash, and impact starburst.
// Mode 1 (FX_HITSCAN_TRACER): Supersonic railgun / pulse tracer with Mach shock rings and spark burst.
// Mode 2 (FX_LIGHTNING_ARC): 3-strand braided electric / psionic lightning arcs with 60 Hz dielectric strobe.
// Mode 3 (FX_MELEE_SLASH): Sweeping 140-degree crescent Psi-Blade energy wave (theta in [-1.35, +1.35],
//                          razor leading edge at r = 0.72, trailing plasma veil).
// Mode 4 (FX_PLASMA_PROJECTILE): Blazing comet-tail plasma orb with tapering turbulent comet tail and embers.
// Mode 5 (FX_IMPACT_STARBURST): 8-ray radial diffraction starburst, detonation flash, and expanding shockwave ring.

#import bevy_pbr::mesh_view_bindings::{view, globals}
#import bevy_pbr::forward_io::VertexOutput

struct CombatFxUniform {
    primary_color: vec4<f32>, // rgb = HDR team color, w = secondary parameter or slash orientation angle
    age: f32,                 // Normalized progress [0.0, 1.0]
    fx_kind: u32,             // 0=Laser, 1=Tracer, 2=Lightning, 3=Melee, 4=Projectile, 5=Starburst
    seed: f32,                // Random seed
    aspect: f32,              // Aspect ratio (length / width)
};

@group(2) @binding(0) var<uniform> fx: CombatFxUniform;

// Pseudo-Random Hash and Noise Primitives
fn hash11(p: f32) -> f32 {
    let q = fract(p * 0.1031);
    let d = q * (q + 33.33);
    return fract(d * (d + d));
}

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let d = dot(q, q + 45.32);
    return fract(d);
}

fn noise1d(x: f32) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(hash11(i), hash11(i + 1.0), u);
}

fn noise2d(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i + vec2<f32>(0.0, 0.0));
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm2d(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var pos = p;
    for (var i = 0; i < 3; i++) {
        v += a * noise2d(pos);
        pos = pos * 2.04 + vec2<f32>(12.3, 34.5);
        a *= 0.5;
    }
    return v;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = (in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let across = uv.x;
    let along = uv.y;
    let r2 = dot(uv, uv);
    let r = sqrt(r2);

    let team_hdr = fx.primary_color.rgb;
    let white_hot = vec3<f32>(1.0, 0.98, 0.92);

    var final_color = vec3<f32>(0.0);
    var final_alpha = 0.0;

    // -------------------------------------------------------------
    // MODE 0: HIGH-ENERGY CONTINUOUS LASER BEAM
    // -------------------------------------------------------------
    if (fx.fx_kind == 0u) {
        let core = exp(-across * across * 110.0) * 3.5;
        let corona_inner = exp(-across * across * 14.0) * 1.6;
        let corona_turb = exp(-across * across * 3.5) * 0.5 * fbm2d(vec2<f32>(along * 6.0, globals.time * 8.0));
        let corona = corona_inner + corona_turb;

        let pulse = sin(along * fx.aspect * 4.0 - globals.time * 45.0) * 0.25 + 0.75;

        // Circular muzzle flash (along < -0.72) and impact starburst (along > +0.72)
        let d_muzzle = length(vec2<f32>(across, along + 0.88));
        let d_impact = length(vec2<f32>(across, along - 0.88));
        let muzzle_flare = exp(-d_muzzle * d_muzzle * 38.0) * 4.5;
        let impact_flare = exp(-d_impact * d_impact * 26.0) * 5.5;

        let life_fade = smoothstep(1.0, 0.75, fx.age) * smoothstep(0.0, 0.05, fx.age);

        final_color = (white_hot * (core + muzzle_flare + impact_flare) + team_hdr * corona) * pulse;
        final_alpha = (core * 0.9 + corona * 0.7 + muzzle_flare + impact_flare) * life_fade;
    }

    // -------------------------------------------------------------
    // MODE 1: SUPERSONIC HITSCAN / RAILGUN TRACER BOLT
    // -------------------------------------------------------------
    else if (fx.fx_kind == 1u) {
        let bolt_head = mix(-0.95, 0.95, smoothstep(0.0, 0.35, fx.age));
        let bolt_tail = mix(-0.95, 0.95, smoothstep(0.20, 0.95, fx.age));

        let in_bolt = smoothstep(bolt_tail - 0.08, bolt_tail, along) * smoothstep(bolt_head + 0.04, bolt_head, along);
        let core = exp(-across * across * 85.0) * in_bolt * 4.0;
        let mach_rings = pow(max(0.0, cos((along - bolt_head) * 25.0)), 4.0) * exp(-across * across * 25.0) * in_bolt;

        let impact_burst = exp(-r2 * 24.0) * smoothstep(0.35, 0.95, fx.age) * (1.0 - fx.age) * 6.0;

        final_color = white_hot * (core + impact_burst) + team_hdr * (mach_rings * 2.0);
        final_alpha = (core * 0.85 + mach_rings * 0.5 + impact_burst) * (1.0 - fx.age);
    }

    // -------------------------------------------------------------
    // MODE 2: 3-PHASE BRAIDED LIGHTNING ARC
    // -------------------------------------------------------------
    else if (fx.fx_kind == 2u) {
        let t = globals.time * 30.0;
        let d1 = (noise1d(along * 8.0 + t + fx.seed * 11.0) - 0.5) * 0.55;
        let d2 = (noise1d(along * 14.0 - t * 1.3 + fx.seed * 23.0) - 0.5) * 0.40;
        let d3 = (noise1d(along * 22.0 + t * 0.7 + fx.seed * 37.0) - 0.5) * 0.25;

        let bolt1 = exp(-pow((across - d1) * 16.0, 2.0)) * 2.5;
        let bolt2 = exp(-pow((across - d2) * 18.0, 2.0)) * 1.8;
        let bolt3 = exp(-pow((across - d3) * 22.0, 2.0)) * 1.5;

        let strobe = step(0.15, hash11(floor(globals.time * 60.0) + fx.seed));
        let total_lightning = (bolt1 + bolt2 + bolt3) * strobe;

        let life = 1.0 - fx.age;
        final_color = mix(team_hdr, white_hot, total_lightning * 0.35);
        final_alpha = total_lightning * life * 0.9;
    }

    // -------------------------------------------------------------
    // MODE 3: SWEEPING CRESCENT PSI-BLADE MELEE SLASH
    // -------------------------------------------------------------
    else if (fx.fx_kind == 3u) {
        if (r2 > 1.0) {
            discard;
        }

        let slash_angle = fx.primary_color.w;
        let theta = atan2(uv.y, uv.x) - slash_angle;
        let normalized_theta = theta - 6.2831853 * floor((theta + 3.14159265) / 6.2831853);

        let crescent_mask = smoothstep(1.35, 0.70, abs(normalized_theta));
        if (crescent_mask <= 0.001) {
            discard;
        }

        let blade_edge = exp(-pow((r - 0.72) * 18.0, 2.0)) * 4.5;
        let trail_turb = fbm2d(vec2<f32>(r * 8.0, normalized_theta * 4.0 - fx.age * 12.0));
        let trailing_veil = smoothstep(0.35, 0.70, r) * trail_turb * 1.8;

        let life_fade = (1.0 - fx.age) * smoothstep(0.0, 0.05, fx.age);
        let intensity = (blade_edge + trailing_veil) * crescent_mask * life_fade;

        final_color = mix(team_hdr, white_hot, blade_edge * 0.5);
        final_alpha = intensity * 0.85;
    }

    // -------------------------------------------------------------
    // MODE 4: BLAZING COMET-TAIL PLASMA PROJECTILE ORB
    // -------------------------------------------------------------
    else if (fx.fx_kind == 4u) {
        let head_center = vec2<f32>(-0.22, 0.0);
        let head_dist = length(uv - head_center);
        let core = exp(-head_dist * head_dist * 45.0) * 5.0;
        let halo = exp(-head_dist * head_dist * 8.0) * 1.5;

        // Tapering comet tail trailing to +x with turbulent embers
        let tail_along = smoothstep(0.95, -0.22, uv.x);
        let tail_across = exp(-uv.y * uv.y * (16.0 + uv.x * 10.0));
        let tail_turb = fbm2d(vec2<f32>(uv.x * 5.0 - globals.time * 12.0, uv.y * 6.0));
        let tail = tail_along * tail_across * tail_turb * 1.5;

        let ember_noise = noise2d(vec2<f32>(uv.x * 12.0 - globals.time * 9.0, uv.y * 14.0) + fx.seed * 23.1);
        let embers = smoothstep(0.86, 0.99, ember_noise) * tail_along * 2.2;

        final_color = white_hot * (core + embers) + team_hdr * (halo + tail);
        final_alpha = clamp((core + halo * 0.7 + tail * 0.8 + embers) * (1.0 - fx.age * 0.5), 0.0, 1.0);
    }

    // -------------------------------------------------------------
    // MODE 5: 8-POINTED IMPACT STARBURST & REFRACTIVE SHOCKWAVE
    // -------------------------------------------------------------
    else if (fx.fx_kind == 5u) {
        if (r2 > 1.0) {
            discard;
        }

        let theta = atan2(uv.y, uv.x);
        let ray = pow(abs(cos(4.0 * theta)), 8.0) * exp(-r * 3.5) * 4.0;
        let core_burst = exp(-r2 * 36.0) * 6.0;

        let wave_r = smoothstep(0.0, 0.95, fx.age) * 0.85;
        let ring = exp(-pow((r - wave_r) * 18.0, 2.0)) * 2.5;

        let shard_silhouette = smoothstep(0.75, 0.25, abs(fract(theta * 1.27) - 0.5) * r);

        let life = (1.0 - fx.age) * (1.0 - fx.age);
        final_color = white_hot * (core_burst + ring * 0.5) + team_hdr * (ray + ring + shard_silhouette * 0.4);
        final_alpha = (core_burst * 0.9 + ray * 0.6 + ring * 0.7 + shard_silhouette * 0.3) * life;
    }

    if (final_alpha < 0.015) {
        discard;
    }

    return vec4<f32>(final_color * final_alpha, final_alpha);
}
