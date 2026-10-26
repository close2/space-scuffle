// assets/shaders/selection_ring.wgsl
// High-tech holographic tactical selection reticle with sub-pixel anti-aliasing.
// Features: dual SDF concentric rings, 4 rotating swept-arc corner brackets,
// 12 radial compass tick marks, and an inner ambient energy haze.

#import bevy_pbr::mesh_view_bindings::{view, globals}
#import bevy_pbr::forward_io::VertexOutput

struct SelectionRingUniform {
    team_color: vec4<f32>,
    radius: f32,
    is_selected: u32,
    pulse_speed: f32,
    padding: f32,
};

@group(2) @binding(0) var<uniform> reticle: SelectionRingUniform;

const PI: f32 = 3.14159265359;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if (reticle.is_selected == 0u) {
        discard;
    }

    let p = (in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let r = length(p);
    if (r > 1.0) {
        discard;
    }

    let theta = atan2(p.y, p.x);

    // 1. Inner Thin Crisp Ring (r = 0.70)
    let ring_inner = smoothstep(0.016, 0.002, abs(r - 0.70));

    // 2. Outer Segmented Tactical Ring (r = 0.90)
    let ring_outer = smoothstep(0.020, 0.003, abs(r - 0.90));

    // 3. Four Rotating Swept-Arc Corner Brackets (r = 0.94)
    let rot_theta = theta - globals.time * 0.75;
    let quad_angle = abs(fract(rot_theta / (0.5 * PI) + 0.5) - 0.5) * (0.5 * PI);
    let bracket_mask = smoothstep(0.32, 0.22, quad_angle) * smoothstep(0.038, 0.004, abs(r - 0.94));

    // 4. Twelve Radial Compass Tick Marks
    let tick_angle = fract(theta / (PI / 6.0) + 0.5) - 0.5;
    let tick_mask = smoothstep(0.035, 0.008, abs(tick_angle * r)) * step(0.67, r) * step(r, 0.73);

    // 5. Inner Ambient Glow Haze
    let haze_inner = exp(-r * 3.5) * 0.22;

    // 6. Tactical Pulse Breathing
    let pulse = 0.85 + 0.15 * sin(globals.time * reticle.pulse_speed);

    // Calculate perceptual alpha
    let ring_alpha = clamp(ring_inner * 0.9 + ring_outer * 0.95 + bracket_mask * 1.0 + tick_mask * 0.9 + haze_inner * 0.35, 0.0, 0.95) * pulse;
    if (ring_alpha < 0.02) {
        discard;
    }

    // Composite Additive HDR Glow
    let total_intensity = (ring_inner * 1.6 + ring_outer * 2.0 + bracket_mask * 3.6 + tick_mask * 1.8 + haze_inner) * pulse;
    let final_color = reticle.team_color.rgb * total_intensity;

    return vec4<f32>(final_color, ring_alpha);
}

