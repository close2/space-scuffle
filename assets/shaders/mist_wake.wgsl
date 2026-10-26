// assets/shaders/mist_wake.wgsl
// Colorful glowing iridescent mist wake and slipstream shader for Space Scuffle.
// Renders swept-back comet-tail slipstreams with electric cyan, neon violet-magenta,
// pearlescent pink-white, and fiery golden-amber outer rims with longitudinal speed-streaks,
// procedural SDF micro-bubbles, and glittering star-sparks.

#import bevy_pbr::mesh_view_bindings::{view, globals}
#import bevy_pbr::forward_io::VertexOutput

struct MistWakeUniform {
    base_color: vec4<f32>,      // rgb = base tint, w = flow_angle_rad
    age: f32,                   // Normalized particle lifetime [0.0, 1.0]
    speed_boosted: u32,         // 0 = colorful nebula, 1 = thruster overdrive, 2 = micro-bubbles
    seed: f32,                  // Random seed
    aspect: f32,                // Aspect ratio / elongation
};

@group(2) @binding(0) var<uniform> mist: MistWakeUniform;

// 2D Hash & Noise primitives
fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let d = dot(q, q + 45.32);
    return fract(d);
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

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    let shift = vec2<f32>(100.0);
    var pos = p;
    for (var i = 0; i < 4; i++) {
        v += a * noise2d(pos);
        pos = pos * 2.0 + shift;
        a *= 0.5;
    }
    return v;
}

// Inigo Quilez 2-Stage Domain Warping
fn pattern(p: vec2<f32>, time: f32) -> f32 {
    let t = time * 0.20;
    let q = vec2<f32>(
        fbm(p + vec2<f32>(0.0, t)),
        fbm(p + vec2<f32>(5.2, 1.3 - t))
    );
    let r = vec2<f32>(
        fbm(p + 2.8 * q + vec2<f32>(1.7, 9.2 + t * 0.5)),
        fbm(p + 2.8 * q + vec2<f32>(8.3, 2.8 - t * 0.5))
    );
    return fbm(p + 2.8 * r);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // 1. Center and rotate coordinates along trail vector
    let uv = (in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let angle = mist.base_color.w;
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let rotated_p = vec2<f32>(
        uv.x * cos_a - uv.y * sin_a,
        uv.x * sin_a + uv.y * cos_a
    );

    let along_trail = rotated_p.x;   // Negative = forward, Positive = trailing wake
    let across_trail = rotated_p.y;  // Lateral distance from center axis

    let r2 = dot(uv, uv);
    if (r2 > 1.0) {
        discard;
    }

    // 2. Teardrop / Comet-Tail Envelope
    // Broad at front (-0.5), tapering smoothly toward back (+1.0)
    let taper = smoothstep(1.0, -0.4, along_trail);
    let lateral_envelope = exp(-pow(across_trail * (1.2 + along_trail * 1.5), 2.0) * 3.5);
    let tail_envelope = taper * lateral_envelope * (1.0 - r2);

    // 3. 2-Stage Domain-Warped FBM Turbulence
    let noise_coord = vec2<f32>(along_trail * 2.0, across_trail * 3.5) + vec2<f32>(mist.seed * 17.3, mist.seed * 31.7);
    let turbulence = pattern(noise_coord, globals.time);

    // 4. Longitudinal Speed-Streaks / Slipstream Filaments
    let streak_coord = vec2<f32>(along_trail * 1.5 - globals.time * 3.0, across_trail * 12.0);
    let streaks = pow(noise2d(streak_coord), 2.2) * smoothstep(0.1, 0.8, taper);

    // 5. Procedural Iridescent Micro-Bubbles
    // Grid of tiny circular soap bubbles drifting inside the wake
    var bubble_intensity = 0.0;
    var bubble_rainbow = vec3<f32>(0.0);
    if (mist.speed_boosted != 1u) {
        let b_grid = fract(vec2<f32>(along_trail * 3.0, across_trail * 4.0) + vec2<f32>(mist.seed * 5.0, globals.time * 0.1)) - 0.5;
        let b_dist = length(b_grid);
        let b_ring = abs(b_dist - 0.22);
        if (b_ring < 0.06 && b_dist < 0.28) {
            bubble_intensity = smoothstep(0.06, 0.01, b_ring) * (1.0 - mist.age);
            // Rainbow rim tinting
            let hue = fract(b_dist * 8.0 + mist.seed);
            bubble_rainbow = vec3<f32>(
                0.5 + 0.5 * cos(6.28 * (hue + 0.0)),
                0.5 + 0.5 * cos(6.28 * (hue + 0.33)),
                0.5 + 0.5 * cos(6.28 * (hue + 0.67))
            );
        }
    }

    // 6. Glittering Golden & Cyan Star-Sparks
    let spark_noise = noise2d(vec2<f32>(along_trail * 8.0 - globals.time * 5.0, across_trail * 16.0) + mist.seed * 43.1);
    let spark_intensity = smoothstep(0.92, 0.99, spark_noise) * 2.5 * (1.0 - mist.age);

    // 7. Rich Multicolored Iridescent Color Composition (Gemini Reference Image)
    // Core: Electric Cyan + Neon Violet-Magenta + Pearlescent Pink-White
    // Rim: Warm Golden-Amber / Orange-Coral Ribbons
    let col_cyan    = vec3<f32>(0.18, 0.90, 1.0);
    let col_magenta = vec3<f32>(0.88, 0.28, 1.0);
    let col_pink    = vec3<f32>(1.0, 0.80, 0.98);
    let col_amber   = vec3<f32>(1.0, 0.60, 0.16);
    let col_gold    = vec3<f32>(1.0, 0.82, 0.30);

    let lateral_dist = abs(across_trail);
    var core_color = mix(col_cyan, col_magenta, smoothstep(0.2, 0.7, turbulence));
    core_color = mix(core_color, col_pink, smoothstep(0.7, 0.95, turbulence));

    // Outer ribbons get fiery golden-amber
    var final_color = mix(core_color, col_amber, smoothstep(0.25, 0.75, lateral_dist));

    // Speed streaks add intense neon highlights
    final_color += streaks * col_cyan * 1.5;

    // Sparks add gold / cyan glitter
    let spark_col = mix(col_gold, col_cyan, step(0.5, hash21(uv + mist.seed)));
    final_color += spark_intensity * spark_col;

    // Micro-bubbles add rainbow rings
    final_color += bubble_intensity * bubble_rainbow * 2.0;

    // Overdrive mode shifts toward golden-orange thruster fire
    if (mist.speed_boosted == 1u) {
        final_color = mix(final_color, col_amber * 1.8, 0.6);
    }

    // 8. Additive Alpha & Lifetime Fade
    let life_fade = smoothstep(1.0, 0.60, mist.age) * smoothstep(0.0, 0.08, mist.age);
    let condensation_core = exp(-r2 * 6.0);
    let density = (turbulence * 0.7 + streaks * 0.3) * tail_envelope * life_fade
        + condensation_core * (1.0 - smoothstep(0.0, 0.55, mist.age)) * 0.28;
    let alpha = clamp(density * 0.45, 0.0, 0.70);

    if (alpha < 0.015) {
        discard;
    }

    return vec4<f32>(final_color * alpha, alpha);
}
