// assets/shaders/mist_wake.wgsl
// Saturated Bioluminescent Volumetric Slipstream Vapor Shader for Space Scuffle.
// Implements:
// - 2D Rotational FBM (OCTAVE_ROT mat2x2 rotation between octaves to eliminate grid stripes)
// - 2-Stage Domain Warping for turbulent curling tendrils
// - Elongated slipstream wake coordinate distortion
// - Noise-perturbed radial window (eliminating all circular borders)
// - 3-stop saturated spectral gradient: Electric Cyan -> Deep Neon Violet-Blue -> Vivid Arcane Magenta
// - Additive blending (SrcAlpha, One): outputs (hdr_color, alpha) with zero alpha-squaring

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

// Rotational FBM Matrix (36.87 degrees rotation between octaves to eliminate grid stripes!)
const OCTAVE_ROT: mat2x2<f32> = mat2x2<f32>(0.80, -0.60, 0.60, 0.80);

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var pos = p;
    for (var i = 0; i < 4; i++) {
        v += a * noise2d(pos);
        pos = OCTAVE_ROT * pos * 2.02 + vec2<f32>(17.3, 31.7);
        a *= 0.5;
    }
    return v;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // 1. Center and rotate coordinates along trail vector
    let uv = (in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let r2 = dot(uv, uv);
    if (r2 > 1.0) {
        discard;
    }

    let angle = mist.base_color.w;
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let rotated_p = vec2<f32>(
        uv.x * cos_a - uv.y * sin_a,
        uv.x * sin_a + uv.y * cos_a
    );

    let along_trail = rotated_p.x;   // Negative = forward, Positive = trailing wake
    let across_trail = rotated_p.y;  // Lateral distance from center axis

    // 2. 2-Stage Domain Warping for Curling Tendrils
    let t = globals.time * 0.35;
    let flow_p = vec2<f32>(along_trail * 1.15 - t * 0.8, across_trail * 1.35) + vec2<f32>(mist.seed * 13.7, mist.seed * 29.3);
    let warp_x = fbm(flow_p) - 0.5;
    let warp_y = fbm(flow_p + vec2<f32>(5.2, 1.3)) - 0.5;
    let warped_uv = rotated_p + vec2<f32>(warp_x, warp_y) * (0.32 + 0.22 * mist.age);

    let w = fbm(flow_p + 2.4 * vec2<f32>(warp_x, warp_y));
    let billow = 1.0 - abs(w * 2.0 - 1.0);
    let wisp_structure = smoothstep(0.24, 0.80, w * 0.60 + billow * 0.40);

    // 3. Elongated Slipstream Wake Falloff & Noise-Perturbed Radial Window
    let noisy_r2 = r2 + (w - 0.5) * 0.55;
    let radial_window = pow(clamp(1.0 - max(noisy_r2, r2 * 0.85), 0.0, 1.0), 2.5);

    let aniso_dist2 = warped_uv.x * warped_uv.x * 0.55 + warped_uv.y * warped_uv.y * 2.6;
    let gaussian_core = exp(-aniso_dist2 * 3.4);
    let vapor_density = gaussian_core * radial_window * wisp_structure;

    // 4. Saturated 3-Stop Spectral Bioluminescent Color Grading
    // Electric Cyan -> Deep Neon Violet-Blue -> Vivid Arcane Magenta
    let col_cyan    = vec3<f32>(0.0, 0.92, 1.0);
    let col_violet  = vec3<f32>(0.25, 0.18, 1.0);
    let col_magenta = vec3<f32>(0.95, 0.12, 0.88);
    let col_amber   = vec3<f32>(1.0, 0.62, 0.15);
    let col_core    = vec3<f32>(0.85, 0.98, 1.0);

    let color_noise = clamp((w - 0.25) / 0.5 + across_trail * 0.3, 0.0, 1.0);
    var spectral_color = select(
        mix(col_cyan, col_violet, color_noise * 2.0),
        mix(col_violet, col_magenta, (color_noise - 0.5) * 2.0),
        color_noise > 0.5
    );

    // Subtle cyan-white highlight on the sharpest turbulent ridges
    let ridge = billow * gaussian_core;
    spectral_color += col_core * (pow(ridge, 4.0) * 0.25);
    spectral_color *= 1.35;

    // Boosted thruster overdrive shifts toward fiery golden-amber
    if (mist.speed_boosted == 1u) {
        spectral_color = mix(spectral_color, col_amber * 1.5, 0.60);
    }

    // 5. Additive Alpha (BlendFactor::One, BlendFactor::One in Bevy 0.15)
    // Hardware adds dst.rgb + src.rgb, so fragment shader premultiplies by alpha!
    let life_fade = pow(clamp(1.0 - mist.age, 0.0, 1.0), 1.3) * smoothstep(0.0, 0.10, mist.age);
    let alpha = clamp(vapor_density * life_fade * 0.44, 0.0, 0.46);

    if (alpha < 0.002) {
        discard;
    }

    return vec4<f32>(spectral_color * alpha, alpha);
}
