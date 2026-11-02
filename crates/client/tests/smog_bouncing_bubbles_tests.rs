//! Comprehensive integration tests for 3D Volumetric Smog, 10 Bouncing Bubbles,
//! and WGSL shader/uniform data alignment.

#![allow(clippy::cast_precision_loss)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

use bevy::prelude::*;
use client::smog::{
    bubble_team_color, generate_bouncing_bubbles, initial_bouncing_bubbles,
    step_bouncing_kinematics, BouncingSmogBubble, BubbleSmogSource, SmogArenaBounds, SmogConfig,
    SmogError, SmogGrid3d, SmogPlugin, SmogStats, SmogVolumeUniformGpu, BOUNCING_BUBBLE_COUNT,
    SMOG_DENSITY_MAX,
};
use client::ResolvedAssetsRoot;
use std::mem::{align_of, offset_of, size_of};

fn resolve_root() -> Result<ResolvedAssetsRoot, Box<dyn std::error::Error>> {
    Ok(ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .canonicalize()?,
    )?)
}

#[test]
fn test_shipped_smog_uniform_and_wgsl_offsets_match() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Rust Layout Verification
    assert_eq!(
        size_of::<SmogVolumeUniformGpu>(),
        608,
        "SmogVolumeUniformGpu must be exactly 608 bytes"
    );
    assert_eq!(
        align_of::<SmogVolumeUniformGpu>(),
        16,
        "SmogVolumeUniformGpu must be 16-byte aligned"
    );

    assert_eq!(offset_of!(SmogVolumeUniformGpu, volume_min), 0);
    assert_eq!(offset_of!(SmogVolumeUniformGpu, volume_max), 16);
    assert_eq!(offset_of!(SmogVolumeUniformGpu, base_tint_time), 32);
    assert_eq!(offset_of!(SmogVolumeUniformGpu, warm_tint_anisotropy), 48);
    assert_eq!(offset_of!(SmogVolumeUniformGpu, light_dir_intensity), 64);
    assert_eq!(offset_of!(SmogVolumeUniformGpu, control_params), 80);
    assert_eq!(offset_of!(SmogVolumeUniformGpu, bubble_pos_radius), 96);
    assert_eq!(offset_of!(SmogVolumeUniformGpu, bubble_vel_boost), 352);

    // 2. Parse Real WGSL Declaration via Naga
    let root = resolve_root()?;
    let shader_source = std::fs::read_to_string(root.as_path().join("shaders/smog_volume.wgsl"))?;
    let (_, body) = shader_source
        .split_once("struct SmogVolumeUniform {")
        .ok_or("missing real SmogVolumeUniform declaration in WGSL")?;
    let (body, _) = body
        .split_once("};")
        .ok_or("unterminated real SmogVolumeUniform declaration in WGSL")?;

    let declaration = format!("struct SmogVolumeUniform {{ {body} }};");
    let module = naga::front::wgsl::parse_str(&declaration)?;
    let uniform_type = module
        .types
        .iter()
        .find(|(_, ty)| ty.name.as_deref() == Some("SmogVolumeUniform"))
        .ok_or("parsed SmogVolumeUniform declaration missing")?;

    let naga::TypeInner::Struct { members, span } = &uniform_type.1.inner else {
        return Err("SmogVolumeUniform is not a struct in Naga".into());
    };

    assert_eq!(*span as usize, size_of::<SmogVolumeUniformGpu>());

    macro_rules! expected_offsets {
        ($($field:ident),+ $(,)?) => {
            [$( (stringify!($field), offset_of!(SmogVolumeUniformGpu, $field)) ),+]
        };
    }

    let expected = expected_offsets![
        volume_min,
        volume_max,
        base_tint_time,
        warm_tint_anisotropy,
        light_dir_intensity,
        control_params,
        bubble_pos_radius,
        bubble_vel_boost,
    ];

    assert_eq!(members.len(), expected.len());
    for (member, (name, offset)) in members.iter().zip(expected) {
        assert_eq!(member.name.as_deref(), Some(name));
        assert_eq!(
            member.offset as usize, offset,
            "WGSL/Rust offset mismatch for {name}"
        );
    }

    Ok(())
}

#[test]
fn test_ten_bubbles_kinematics_and_six_face_containment() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let mut bubbles = initial_bouncing_bubbles(&bounds)?;

    assert_eq!(bubbles.len(), BOUNCING_BUBBLE_COUNT);

    let dt = 1.0 / 60.0;
    let mut total_wall_bounces = 0u64;
    let mut total_peer_bounces = 0u64;

    // Simulate 600 frames (10 seconds)
    for frame in 0..600 {
        let (wb, pb) = step_bouncing_kinematics(&mut bubbles, &bounds, dt)?;
        total_wall_bounces += wb;
        total_peer_bounces += pb;

        for (i, bubble) in bubbles.iter().enumerate() {
            assert!(
                bounds.contains_sphere(bubble.position, bubble.radius),
                "Frame {}: Bubble {} (pos={:?}, r={}) escaped arena bounds",
                frame,
                i,
                bubble.position,
                bubble.radius
            );
            let speed = bubble.velocity.length();
            assert!(
                (0.05..=10.0).contains(&speed),
                "Frame {frame}: Bubble {i} speed {speed} deviated outside reasonable physical bounds"
            );
            assert!(bubble.heading_angle_rad.is_finite());
        }
    }

    assert!(
        total_wall_bounces >= 20,
        "Over 600 frames, bubbles must actively reflect off walls (got {total_wall_bounces})"
    );
    assert!(
        total_peer_bounces >= 5,
        "Over 600 frames, pairwise bubble collisions must occur (got {total_peer_bounces})"
    );

    Ok(())
}

#[test]
fn test_ten_bubbles_continuous_smog_generation() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let bubbles = initial_bouncing_bubbles(&bounds)?;
    let mut grid = SmogGrid3d::new(Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 2.8, 4.5))?;

    let config = SmogConfig {
        enabled: true,
        bouncing_active: true,
        bounds,
        emission_rate: 2.0,
        wake_influence: 1.5,
        dissipation_rate: 0.02,
        vorticity_confinement: 0.25,
        buoyancy: 0.05,
        velocity_damping: 0.05,
    };

    // Convert all 10 bubbles to sources with varied chromatic team colors
    let mut sources = [BubbleSmogSource {
        position: Vec3::ZERO,
        velocity: Vec3::ZERO,
        radius: 0.5,
        team_color: Vec3::ONE,
        emission_strength: 1.5,
        influence_strength: 1.0,
    }; BOUNCING_BUBBLE_COUNT];

    for i in 0..BOUNCING_BUBBLE_COUNT {
        sources[i] = BubbleSmogSource {
            position: bubbles[i].position,
            velocity: bubbles[i].velocity,
            radius: bubbles[i].radius,
            team_color: Vec3::new(
                0.2 + (i as f32) * 0.07,
                0.8 - (i as f32) * 0.05,
                0.5 + (i as f32) * 0.04,
            ),
            emission_strength: 1.5,
            influence_strength: 1.0,
        };
    }

    let mut prev_total = 0.0;
    for _ in 0..18 {
        let metrics = grid.step(&sources, &config, 1.0 / 60.0)?;
        assert!(
            metrics.total_density > prev_total,
            "Total smog density must continuously increase during 10-bubble emission"
        );
        prev_total = metrics.total_density;
    }

    // Verify density is positive across individual bubble lateral shoulders,
    // while the core remains hollow to preserve creature flipbook visibility.
    for b in &bubbles {
        let s_hat = Vec3::new(-b.velocity.z, 0.0, b.velocity.x).normalize_or_zero();
        let shoulder_sample = grid.sample_density_trilinear(b.position + s_hat * (b.radius * 0.86));
        assert!(
            shoulder_sample > 0.005,
            "Smog density at bubble shoulder {:?} must be non-zero (got {})",
            b.position + s_hat * (b.radius * 0.86),
            shoulder_sample
        );
        let center_sample = grid.sample_density_trilinear(b.position);
        assert!(
            center_sample < 0.01,
            "Smog density at bubble center {:?} must remain clear for creature flipbook (got {})",
            b.position,
            center_sample
        );
    }

    Ok(())
}

#[test]
fn test_moving_bubble_influences_and_perturbs_existing_smog() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let bubbles = initial_bouncing_bubbles(&bounds)?;
    let mut grid = SmogGrid3d::new(Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 2.8, 4.5))?;

    let config = SmogConfig {
        enabled: true,
        bouncing_active: true,
        bounds,
        emission_rate: 2.0,
        wake_influence: 2.5,
        dissipation_rate: 0.0,
        vorticity_confinement: 0.3,
        buoyancy: 0.0,
        velocity_damping: 0.02,
    };

    // Step 1: Bubbles 0..4 emit a dense smog plume across the region
    let emitting_sources: Vec<_> = bubbles[0..5]
        .iter()
        .map(|b| BubbleSmogSource {
            position: b.position,
            velocity: b.velocity,
            radius: b.radius,
            team_color: Vec3::new(0.3, 0.7, 0.9),
            emission_strength: 5.0,
            influence_strength: 0.5,
        })
        .collect();

    for _ in 0..60 {
        grid.step(&emitting_sources, &config, 1.0 / 60.0)?;
    }

    let pre_influence_metrics = grid.step(&[], &config, 1.0 / 60.0)?;
    assert!(
        pre_influence_metrics.total_density > 10.0,
        "Smog plume should be established"
    );

    // Step 2: Now send Bubble 5 straight through the dense shoulder plume of Bubble 3 with ZERO emission
    let s_hat_b3 =
        Vec3::new(-bubbles[3].velocity.z, 0.0, bubbles[3].velocity.x).normalize_or_zero();
    let target_center = bubbles[3].position + s_hat_b3 * (bubbles[3].radius * 0.86);
    let pre_center_density = grid.sample_density_trilinear(target_center);
    assert!(
        pre_center_density > 0.05,
        "Target region must contain pre-existing smog (got {pre_center_density})"
    );

    let moving_traverser = [BubbleSmogSource {
        position: target_center,
        velocity: Vec3::new(3.8, 0.0, 0.0),
        radius: 0.60,
        team_color: Vec3::ONE,
        emission_strength: 0.0, // Strictly isolated influence!
        influence_strength: 3.0,
    }];

    grid.step(&moving_traverser, &config, 1.0 / 30.0)?;

    // Verify physical obstacle evacuation
    let post_center_density = grid.sample_density_trilinear(target_center);
    assert!(
        post_center_density < pre_center_density,
        "Pre-existing smog inside moving bubble core must be displaced (pre: {pre_center_density}, post: {post_center_density})"
    );

    // Verify momentum drag along velocity vector (+X)
    let fluid_velocity_at_center = grid.sample_velocity_trilinear(target_center);
    assert!(
        fluid_velocity_at_center.x > 0.02,
        "Fluid velocity along bubble path must be dragged in +X direction (got {fluid_velocity_at_center:?})"
    );

    // Verify wake vortex generation
    let wake_pos = target_center - Vec3::new(0.7, 0.2, 0.0);
    let wake_vorticity = grid.sample_vorticity_magnitude(wake_pos);
    assert!(
        wake_vorticity > 0.001,
        "Wake vorticity behind moving bubble must be non-zero (got {wake_vorticity})"
    );

    Ok(())
}

#[test]
fn test_full_bevy_schedule_ten_bubbles_and_smog_grid() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(bouncing_lab())?;

    // Step 60 frames in headless Bevy
    for _ in 0..60 {
        app.update();
    }

    // Verify exactly 10 BouncingSmogBubble entities are active
    let bubble_count = app
        .world_mut()
        .query::<&BouncingSmogBubble>()
        .iter(app.world())
        .count();
    assert_eq!(
        bubble_count, BOUNCING_BUBBLE_COUNT,
        "Must spawn and maintain exactly 10 BouncingSmogBubble entities"
    );

    // Verify SmogStats advanced (frame 0 has dt = 0.0 in Bevy Time)
    let stats = *app.world().resource::<SmogStats>();
    assert_eq!(
        stats.step_count, 59,
        "Smog simulation must advance 59 steps"
    );
    assert!(
        stats.total_density > 0.0,
        "Smog field total density must be positive after 60 frames"
    );
    assert_eq!(
        stats.active_bubbles, BOUNCING_BUBBLE_COUNT,
        "All 10 bubbles must register as active smog sources"
    );

    Ok(())
}

#[test]
fn test_four_hundred_units_smog_generation_and_spatial_hash_performance() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let count = 400;
    let mut bubbles = generate_bouncing_bubbles(count, &bounds)?;

    assert_eq!(bubbles.len(), 400);

    let mut grid = SmogGrid3d::new(Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 2.8, 4.5))?;
    let config = SmogConfig {
        enabled: true,
        bouncing_active: true,
        bounds,
        emission_rate: 1.35,
        wake_influence: 2.6,
        dissipation_rate: 0.32,
        vorticity_confinement: 0.40,
        buoyancy: 0.05,
        velocity_damping: 0.05,
    };

    let dt = 1.0 / 60.0;
    let start_time = std::time::Instant::now();

    // Step 20 frames of both 400-bubble kinematics (spatial hash grid) and 400-source 3D fluid simulation
    let mut sources = Vec::with_capacity(count);
    for _ in 0..20 {
        let _ = step_bouncing_kinematics(&mut bubbles, &bounds, dt)?;

        sources.clear();
        for b in &bubbles {
            let team_color = bubble_team_color(b.index);
            sources.push(BubbleSmogSource {
                position: b.position,
                velocity: b.velocity,
                radius: b.radius,
                team_color,
                emission_strength: config.emission_rate,
                influence_strength: config.wake_influence,
            });
        }

        let metrics = grid.step(&sources, &config, dt)?;
        assert_eq!(metrics.active_sources, 400);
        assert!(metrics.total_density > 0.0);
    }

    let elapsed = start_time.elapsed();
    // 20 frames should execute comfortably within a few seconds in debug mode
    assert!(
        elapsed.as_secs_f32() < 8.0,
        "400-unit simulation (20 frames) took too long: {:.2}s",
        elapsed.as_secs_f32()
    );

    // Invariant 1: All 400 bubbles remain strictly contained within 3D arena bounds
    for (i, b) in bubbles.iter().enumerate() {
        assert!(
            bounds.contains_sphere(b.position, b.radius),
            "Bubble {i} at pos={:?}, r={} escaped arena bounds",
            b.position,
            b.radius
        );
        assert!(b.velocity.length() > 0.01);
    }

    // Invariant 2: Positive density and glow generated in the fluid volume
    let sample_center = grid.sample_density_trilinear(Vec3::new(0.0, 2.0, 0.0));
    let sample_glow = grid.sample_glow_trilinear(Vec3::new(0.0, 2.0, 0.0));
    assert!(
        sample_center > 0.0 || sample_glow >= 0.0,
        "Smog field must contain valid fluid density/glow"
    );

    Ok(())
}

#[test]
fn test_bubble_direction_change_preserves_existing_world_space_mist() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let mut grid = SmogGrid3d::new(Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 2.8, 4.5))?;
    let config = SmogConfig {
        enabled: true,
        bouncing_active: true,
        bounds,
        emission_rate: 3.6,
        wake_influence: 4.2,
        dissipation_rate: 0.38,
        vorticity_confinement: 1.1,
        buoyancy: 0.008,
        velocity_damping: 0.40,
    };

    let dt = 1.0 / 30.0;
    let initial_vel = Vec3::new(3.5, 0.0, 0.0);
    let mut pos = Vec3::new(-2.0, 0.7, 0.0);
    let radius = 0.55;

    // Step 1: Step a single bubble moving +X for 30 frames
    for _ in 0..30 {
        pos += initial_vel * dt;
        let source = BubbleSmogSource {
            position: pos,
            velocity: initial_vel,
            radius,
            team_color: Vec3::new(0.98, 0.82, 0.22),
            emission_strength: config.emission_rate,
            influence_strength: config.wake_influence,
        };
        grid.step(&[source], &config, dt)?;
    }

    // Measure the mist density deposited along its historical +X path trailing the bubble (x in [0.7, 1.3])
    let sample_points = [
        Vec3::new(1.3, 0.7, 0.0),
        Vec3::new(1.1, 0.7, 0.0),
        Vec3::new(0.9, 0.7, 0.0),
        Vec3::new(0.7, 0.7, 0.0),
        Vec3::new(1.3, 0.7, 0.20),
        Vec3::new(1.1, 0.7, 0.20),
        Vec3::new(0.9, 0.7, 0.20),
        Vec3::new(0.7, 0.7, 0.20),
        Vec3::new(1.3, 0.7, -0.20),
        Vec3::new(1.1, 0.7, -0.20),
        Vec3::new(0.9, 0.7, -0.20),
        Vec3::new(0.7, 0.7, -0.20),
    ];

    let pre_flip_density: f32 = sample_points
        .iter()
        .map(|&p| grid.sample_density_trilinear(p))
        .sum();
    assert!(
        pre_flip_density > 0.05,
        "Mist must be deposited along the +X trajectory (got {pre_flip_density})"
    );

    // Step 2: Instantaneously flip velocity to +Z (e.g. wall bounce or turn) and step 2 frames
    let flip_vel = Vec3::new(0.0, 0.0, 3.5);
    for _ in 0..2 {
        pos += flip_vel * dt;
        let source = BubbleSmogSource {
            position: pos,
            velocity: flip_vel,
            radius,
            team_color: Vec3::new(0.98, 0.82, 0.22),
            emission_strength: config.emission_rate,
            influence_strength: config.wake_influence,
        };
        grid.step(&[source], &config, dt)?;
    }

    // Measure mist density at the exact same world-space locations along the old +X path
    let post_flip_density: f32 = sample_points
        .iter()
        .map(|&p| grid.sample_density_trilinear(p))
        .sum();

    assert!(
        post_flip_density >= pre_flip_density * 0.45,
        "World-space mist must remain in place along old +X trajectory when velocity flips to +Z: pre={pre_flip_density}, post={post_flip_density} (ratio: {:.2})",
        post_flip_density / pre_flip_density
    );

    Ok(())
}

#[test]
fn test_200_bubble_swarm_step_performance() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds {
        min: Vec3::new(4.0, 0.0, 6.0),
        max: Vec3::new(28.0, 2.8, 24.0),
    };
    let count = 200;
    let mut bubbles = generate_bouncing_bubbles(count, &bounds)?;
    assert_eq!(bubbles.len(), 200);

    let mut grid = SmogGrid3d::new(Vec3::new(4.0, 0.0, 6.0), Vec3::new(28.0, 2.8, 24.0))?;
    let config = SmogConfig {
        enabled: true,
        bouncing_active: true,
        bounds,
        emission_rate: 4.2,
        wake_influence: 4.2,
        dissipation_rate: 0.65,
        vorticity_confinement: 1.0,
        buoyancy: 0.008,
        velocity_damping: 0.40,
    };

    let dt = 1.0 / 60.0;
    let start_time = std::time::Instant::now();

    let mut sources = Vec::with_capacity(count);
    for _ in 0..30 {
        let _ = step_bouncing_kinematics(&mut bubbles, &bounds, dt)?;

        sources.clear();
        for b in &bubbles {
            let team_color = bubble_team_color(b.index);
            sources.push(BubbleSmogSource {
                position: b.position,
                velocity: b.velocity,
                radius: b.radius,
                team_color,
                emission_strength: config.emission_rate,
                influence_strength: config.wake_influence,
            });
        }

        let metrics = grid.step(&sources, &config, dt)?;
        assert_eq!(metrics.active_sources, 200);
        assert!(metrics.total_density > 0.0);
        assert!(metrics.peak_density <= SMOG_DENSITY_MAX);
        assert!(metrics.max_speed < 50.0);
    }

    let elapsed = start_time.elapsed();
    assert!(
        elapsed.as_secs_f32() < 8.0,
        "200-unit swarm simulation (30 frames) took too long: {:.2}s",
        elapsed.as_secs_f32()
    );

    Ok(())
}

#[test]
fn test_unified_single_slipstream_and_live_battle_quantized_bounds() -> Result<(), SmogError> {
    // Part 1: Verify unified single slipstream (no empty centerline trench) and aerodynamic inward suction
    let mut grid = SmogGrid3d::new(Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 2.8, 4.5))?;
    let config = SmogConfig {
        enabled: true,
        bouncing_active: true,
        bounds: SmogArenaBounds::default(),
        emission_rate: 3.5,
        wake_influence: 3.5,
        dissipation_rate: 0.10,
        vorticity_confinement: 1.0,
        buoyancy: 0.0,
        velocity_damping: 0.20,
    };

    let bubble_pos = Vec3::new(0.0, 1.4, 0.0);
    let bubble_vel = Vec3::new(3.0, 0.0, 0.0); // Moving along +X at 3.0 m/s
    let radius = 0.80;

    let source = BubbleSmogSource {
        position: bubble_pos,
        velocity: bubble_vel,
        radius,
        team_color: Vec3::new(0.3, 0.8, 0.9),
        emission_strength: 1.0,
        influence_strength: 1.0,
    };

    let dt = 1.0 / 60.0;
    for _ in 0..15 {
        grid.step(&[source], &config, dt)?;
    }

    // Dead-center rear sample (q_par = -0.90, q_lat = 0.0)
    let centerline_rear = bubble_pos - Vec3::new(radius * 0.90, 0.0, 0.0);
    let centerline_density = grid.sample_density_trilinear(centerline_rear);

    // Lateral shoulder samples (q_par = -0.60, q_lat = 0.80)
    let left_shoulder = bubble_pos + Vec3::new(-radius * 0.60, 0.0, radius * 0.80);
    let right_shoulder = bubble_pos + Vec3::new(-radius * 0.60, 0.0, -radius * 0.80);
    let left_density = grid.sample_density_trilinear(left_shoulder);
    let right_density = grid.sample_density_trilinear(right_shoulder);

    assert!(
        centerline_density > 0.02,
        "Centerline wake behind moving bubble must have substantial smoke (got {centerline_density}), verifying no empty trench is carved"
    );
    assert!(
        left_density > 0.01 && right_density > 0.01,
        "Lateral shoulders must have positive smoke: left={left_density}, right={right_density}"
    );

    // Inward streamline suction verification:
    // Left shoulder is at +Z (s_lat > 0), inward suction pulls towards -Z (uz < 0)
    // Right shoulder is at -Z (s_lat < 0), inward suction pulls towards +Z (uz > 0)
    let vel_left = grid.sample_velocity_trilinear(left_shoulder);
    let vel_right = grid.sample_velocity_trilinear(right_shoulder);
    assert!(
        vel_left.z < 0.0,
        "Left slipstream streamline must converge inward toward centerline (uz < 0, got {})",
        vel_left.z
    );
    assert!(
        vel_right.z > 0.0,
        "Right slipstream streamline must converge inward toward centerline (uz > 0, got {})",
        vel_right.z
    );

    // Part 2: Verify LiveBattle bounds quantization and slab shifting at y = 3.0..5.8 (28x2.8x22m volume)
    let live_min = Vec3::new(2.0, 3.0, 5.0);
    let live_max = Vec3::new(30.0, 5.8, 27.0);
    let mut battle_grid = SmogGrid3d::new(live_min, live_max)?;

    // Inject smoke in battle grid
    let battle_source = BubbleSmogSource {
        position: Vec3::new(16.0, 4.2, 16.0),
        velocity: Vec3::new(2.5, 0.0, 0.0),
        radius: 0.80,
        team_color: Vec3::new(0.9, 0.4, 0.2),
        emission_strength: 1.85,
        influence_strength: 1.35,
    };
    for _ in 0..10 {
        battle_grid.step(&[battle_source], &config, dt)?;
    }

    let probe_pos = Vec3::new(15.2, 4.2, 16.0);
    let pre_shift_density = battle_grid.sample_density_trilinear(probe_pos);
    assert!(
        pre_shift_density > 0.02,
        "Pre-shift density at probe pos must be positive (got {pre_shift_density})"
    );

    // Shift camera by exactly 1 voxel cell in +X direction (28.0 / 96 = 0.29167m)
    let cell_shift_x = 28.0 / 96.0;
    let shifted_min = live_min + Vec3::new(cell_shift_x, 0.0, 0.0);
    let shifted_max = live_max + Vec3::new(cell_shift_x, 0.0, 0.0);
    battle_grid.set_bounds_quantized(shifted_min, shifted_max);

    // World-space density at probe_pos must be preserved by in-place slab shifting
    let post_shift_density = battle_grid.sample_density_trilinear(probe_pos);
    assert!(
        (post_shift_density - pre_shift_density).abs() < 0.015,
        "World-space mist must be preserved across quantized camera pan: pre={pre_shift_density}, post={post_shift_density}"
    );

    Ok(())
}

#[test]
fn swarm_stress_test_200_bubbles_populates_smog_field_with_default_5_0_and_3_4(
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Assert default parameters
    assert!((SmogConfig::default().emission_rate - 5.0).abs() < 1e-5);
    assert!((SmogConfig::default().wake_influence - 3.4).abs() < 1e-5);
    assert!((client::lab_ui::LabState::default().smog_emission_rate - 5.0).abs() < 1e-5);
    assert!((client::lab_ui::LabState::default().smog_wake_influence - 3.4).abs() < 1e-5);
    assert!(client::lab_ui::LabState::default().swarm_active);

    // 2. Initialize headless Bevy App
    let root = resolve_root()?;
    let definition = client::runtime_battle::BattleDefinition::load(
        root.as_path(),
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_event::<AppExit>();
    app.init_resource::<client::RenderEntityMap>();
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<StandardMaterial>>();
    app.init_resource::<Assets<client::bubble_shader::BubbleDomeMaterial>>();
    app.init_resource::<Assets<client::smog::SmogVolumeMaterial>>();
    app.insert_resource(definition.palette.clone());
    app.init_resource::<client::player_presentation::PresentationStatus>();
    app.insert_resource(root);
    app.insert_resource(client::CatalogResource(definition.catalogs.units.clone()));
    app.insert_resource(client::lab_ui::LabState {
        current_tab: client::LabTab::SwarmStressTest,
        swarm_active: true,
        ..Default::default()
    });
    app.add_systems(Startup, client::bubble_shader::setup_bubble_assets_system);
    app.add_plugins(SmogPlugin);
    app.add_systems(Update, client::lab_ui::sync_swarm_stress_system);

    // Run ticks to spawn the 200 swarm bubbles and step the smog field
    for _ in 0..10 {
        app.update();
    }

    let stats = app.world().resource::<SmogStats>();
    assert_eq!(
        stats.active_bubbles, 200,
        "200 SwarmBubble entities must be active in SwarmStressTest"
    );
    assert!(
        stats.total_density > 100.0,
        "Total smog density must exceed 100.0 (got {})",
        stats.total_density
    );

    Ok(())
}

#[test]
fn stationary_bubble_generates_downward_settling_mist_motion() -> Result<(), SmogError> {
    let mut grid = SmogGrid3d::new(Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 2.8, 4.5))?;
    let config = SmogConfig::default();
    assert!(
        config.buoyancy < -0.05,
        "Default SmogConfig buoyancy must be negative for downward vapor settling (got {})",
        config.buoyancy
    );

    let bubble_pos = Vec3::new(0.0, 1.35, 0.0);
    let radius = 0.55;
    let stationary_source = BubbleSmogSource {
        position: bubble_pos,
        velocity: Vec3::ZERO,
        radius,
        team_color: Vec3::new(0.25, 0.78, 0.95),
        emission_strength: 1.0,
        influence_strength: 1.0,
    };

    let dt = 1.0 / 30.0;
    let mut last_metrics = client::smog::SmogStepMetrics::default();
    for _ in 0..25 {
        last_metrics = grid.step(&[stationary_source], &config, dt)?;
    }

    assert!(
        last_metrics.total_density > 0.1,
        "Stationary bubble must emit positive mist density (got {})",
        last_metrics.total_density
    );
    assert!(
        last_metrics.max_speed > 0.05,
        "Stationary bubble mist must actively move rather than freeze (max_speed={})",
        last_metrics.max_speed
    );

    let shell_sample_pos = bubble_pos + Vec3::new(radius * 0.88, 0.0, 0.0);
    let shell_vel = grid.sample_velocity_trilinear(shell_sample_pos);
    assert!(
        shell_vel.y < -0.02,
        "Fluid velocity around stationary bubble shell must settle downwards (uy < -0.02, got {})",
        shell_vel.y
    );

    let lower_shell_pos = bubble_pos + Vec3::new(radius * 0.88, -radius * 0.45, 0.0);
    let upper_shell_pos = bubble_pos + Vec3::new(radius * 0.88, radius * 0.45, 0.0);
    let lower_shell_density = grid.sample_density_trilinear(lower_shell_pos);
    let upper_shell_density = grid.sample_density_trilinear(upper_shell_pos);
    assert!(
        lower_shell_density > 0.01 && lower_shell_density > upper_shell_density,
        "Downward settling must advect mist into the lower hemisphere shell (lower={lower_shell_density}, upper={upper_shell_density})"
    );

    Ok(())
}

fn make_test_battle_unit(
    entity_id: u32,
    pos: Vec3,
    ground_z: f32,
    radius: f32,
) -> client::BubbleInstance {
    client::BubbleInstance(sim_types::snapshot::BubbleRenderInstance {
        entity_id: sim_types::EntityId(entity_id),
        shape: sim_types::enums::ShapeKind::UnitSphere,
        silhouette: sim_types::enums::SilhouetteClass::LightProngs,
        unit_type: sim_types::ids::UnitTypeId(1),
        owner: sim_types::PlayerId(1),
        world_pos: [pos.x, pos.z, pos.y],
        ground_z,
        radius,
        velocity: [0.12, 0.0],
        heading_angle_rad: 0.0,
        aim_angle_rad: 0.0,
        is_attacking: false,
        weapon_visual: sim_types::enums::WeaponVisualKind::Hitscan,
        attack_target_pos: None,
        anim_state: sim_types::enums::AnimState::Walk,
        anim_progress: 0.0,
        hp_ratio: 1.0,
        shield_ratio: 1.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: Some(1.0),
        selected: false,
        cloaked: false,
        speed_boosted: false,
    })
}

#[test]
fn live_battle_low_ground_and_high_ground_units_preserve_true_world_y(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root()?;
    let definition = client::runtime_battle::BattleDefinition::load(
        root.as_path(),
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_event::<AppExit>();
    app.init_resource::<client::RenderEntityMap>();
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<StandardMaterial>>();
    app.init_resource::<Assets<client::bubble_shader::BubbleDomeMaterial>>();
    app.init_resource::<Assets<client::smog::SmogVolumeMaterial>>();
    app.insert_resource(definition.palette.clone());
    app.init_resource::<client::player_presentation::PresentationStatus>();
    app.insert_resource(root);
    app.insert_resource(client::CatalogResource(definition.catalogs.units.clone()));
    app.insert_resource(client::lab_ui::LabState {
        current_tab: client::LabTab::LiveBattle,
        opt_smog_volume: true,
        ..Default::default()
    });
    app.add_systems(Startup, client::bubble_shader::setup_bubble_assets_system);
    app.add_plugins(SmogPlugin);
    app.insert_resource(client::terrain::TerrainState::new(sim_map::GameMap::new(
        64,
        64,
        sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
    )?));

    // Spawn an RtsCamera with look_at_y set to high-ground altitude (3.0 + 1.2 = 4.2)
    // while looking at units on both the lower level (ground_z = 0.0) and upper level (ground_z = 3.0).
    app.world_mut().spawn(client::camera::RtsCamera {
        target: Vec2::new(24.0, 24.0),
        look_at_y: 4.2,
        ..Default::default()
    });

    // Lower-level unit (CliffLevel(0), ground_z = 0.0, y = 0.45)
    app.world_mut().spawn((
        make_test_battle_unit(9001, Vec3::new(22.0, 0.45, 24.0), 0.0, 0.45),
        Transform::from_translation(Vec3::new(22.0, 0.45, 24.0)).with_scale(Vec3::splat(0.45)),
        Visibility::Inherited,
    ));

    // Upper-level unit (CliffLevel(1), ground_z = 3.0, y = 3.45)
    app.world_mut().spawn((
        make_test_battle_unit(9002, Vec3::new(26.0, 3.45, 24.0), 3.0, 0.45),
        Transform::from_translation(Vec3::new(26.0, 3.45, 24.0)).with_scale(Vec3::splat(0.45)),
        Visibility::Inherited,
    ));

    for _ in 0..8 {
        app.update();
    }

    let grid = app.world().resource::<SmogGrid3d>();
    let b_min = grid.bounds_min();
    let b_max = grid.bounds_max();

    assert!(
        (b_min.y - 0.0).abs() < 1e-4 && (b_max.y - 4.8).abs() < 1e-4,
        "LiveBattle volume vertical bounds must span [0.0, 4.8] to encompass both low and high ground (got [{}..{}])",
        b_min.y,
        b_max.y
    );

    // Verify the published GPU uniform matches grid bounds bit-for-bit and preserves true world Y
    let uniform = app.world().resource::<client::smog::SmogGpuFrame>().uniform;

    assert_eq!(uniform.volume_min.truncate(), b_min);
    assert_eq!(uniform.volume_max.truncate(), b_max);
    assert!((uniform.control_params.x - 2.0).abs() < 1e-4);

    let y0 = uniform.bubble_pos_radius[0].y;
    let y1 = uniform.bubble_pos_radius[1].y;
    let low_y = y0.min(y1);
    let high_y = y0.max(y1);

    assert!(
        (low_y - 0.45).abs() < 1e-4,
        "Lower-level unit must stay at true world y=0.45, never displaced to high ground (got {low_y})"
    );
    assert!(
        (high_y - 3.45).abs() < 1e-4,
        "Upper-level unit must stay at true world y=3.45 (got {high_y})"
    );

    // Verify smog density is injected at the lower-level unit's true altitude (y ≈ 0.45), not floating above it at y = 3.45
    let low_rear_density = grid.sample_density_trilinear(Vec3::new(21.55, 0.45, 24.0));
    let displaced_above_low_unit = grid.sample_density_trilinear(Vec3::new(22.0, 3.45, 24.0));
    assert!(
        low_rear_density > 0.02,
        "Lower-level unit must generate smog at its true y=0.45 altitude (got {low_rear_density})"
    );
    assert!(
        displaced_above_low_unit < 1e-4,
        "Zero smog should float at y=3.45 directly above the lower-level unit at x=22.0 (got {displaced_above_low_unit})"
    );

    Ok(())
}

/// Visual Lab state with the 10-bubble showcase switched on.
fn bouncing_lab() -> client::LabState {
    client::LabState {
        smog_bouncing_active: true,
        ..default()
    }
}

/// Headless application with the production smog plugin, real catalog, palette and media.
fn production_smog_app(lab: client::LabState) -> Result<App, Box<dyn std::error::Error>> {
    let root = resolve_root()?;
    let definition = client::runtime_battle::BattleDefinition::load(
        root.as_path(),
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_event::<AppExit>();
    app.init_resource::<client::RenderEntityMap>();
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<StandardMaterial>>();
    app.init_resource::<Assets<client::bubble_shader::BubbleDomeMaterial>>();
    app.init_resource::<Assets<client::smog::SmogVolumeMaterial>>();
    app.insert_resource(definition.palette.clone());
    app.init_resource::<client::player_presentation::PresentationStatus>();
    app.insert_resource(root);
    app.insert_resource(client::CatalogResource(definition.catalogs.units.clone()));
    app.insert_resource(lab);
    app.add_systems(Startup, client::bubble_shader::setup_bubble_assets_system);
    app.add_plugins(SmogPlugin);
    Ok(app)
}

/// Per-type counts of `AssetEvent::Modified`, the signal that re-prepares a render asset.
#[derive(Resource, Default)]
struct ModifiedAssetCounts {
    images: usize,
    smog_materials: usize,
}

/// Flushes asset events as a full app does, and counts modifications.
fn track_asset_modifications(app: &mut App) {
    app.add_event::<AssetEvent<Image>>();
    app.add_event::<AssetEvent<client::smog::SmogVolumeMaterial>>();
    app.init_resource::<ModifiedAssetCounts>();
    app.add_systems(
        First,
        (
            Assets::<Image>::asset_events,
            Assets::<client::smog::SmogVolumeMaterial>::asset_events,
        ),
    );
    app.add_systems(
        Last,
        |mut images: EventReader<'_, '_, AssetEvent<Image>>,
         mut materials: EventReader<'_, '_, AssetEvent<client::smog::SmogVolumeMaterial>>,
         mut counts: ResMut<'_, ModifiedAssetCounts>| {
            counts.images += images
                .read()
                .filter(|event| matches!(event, AssetEvent::Modified { .. }))
                .count();
            counts.smog_materials += materials
                .read()
                .filter(|event| matches!(event, AssetEvent::Modified { .. }))
                .count();
        },
    );
}

/// C-2: a modified image is destroyed and recreated by Bevy's render asset pipeline, and a modified
/// material rebuilds its bind group. Neither may happen per frame while the field animates.
#[test]
fn smog_stepping_never_modifies_image_or_material_assets() -> Result<(), Box<dyn std::error::Error>>
{
    let mut app = production_smog_app(bouncing_lab())?;
    track_asset_modifications(&mut app);

    for _ in 0..60 {
        app.update();
    }

    let steps = app.world().resource::<SmogStats>().step_count;
    assert!(steps >= 58, "smog must keep stepping (got {steps} steps)");
    let counts = app.world().resource::<ModifiedAssetCounts>();
    assert_eq!(
        (counts.images, counts.smog_materials),
        (0, 0),
        "image and smog material Modified events over {steps} steps"
    );
    assert_eq!(
        app.world()
            .resource::<client::smog::SmogGpuFrame>()
            .generation,
        steps,
        "every step must publish one GPU upload generation"
    );
    Ok(())
}

/// Replaces the assets root with one holding no flipbooks, keeping the real catalog and palette.
fn without_any_media(app: &mut App) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    std::fs::create_dir(directory.path().join("shaders"))?;
    std::fs::create_dir(directory.path().join("textures"))?;
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/shaders/bubble_dome.wgsl"),
        directory.path().join("shaders/bubble_dome.wgsl"),
    )?;
    app.insert_resource(ResolvedAssetsRoot::from_directory(directory.path())?);
    Ok(directory)
}

fn has_error_exit(app: &App) -> bool {
    app.world()
        .resource::<Events<AppExit>>()
        .iter_current_update_events()
        .any(|event| matches!(event, AppExit::Error(_)))
}

fn smog_failure(app: &App) -> Option<String> {
    let status = app.world().resource::<client::media::ClientMediaStatus>();
    assert_eq!(status.failure().is_some(), status.failure_text().is_some());
    status
        .failure()
        .filter(|error| matches!(error, client::media::ClientMediaError::Smog(_)))
        .map(ToString::to_string)
}

/// C-6: with no admitted actor the old startup fabricated unit types and bound a white texture.
#[test]
fn smog_without_admitted_media_fails_without_spawning_stand_ins(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState::default())?;
    app.insert_resource(client::media::MediaFailurePolicy::new(true));
    let _directory = without_any_media(&mut app)?;

    app.update();

    assert_eq!(
        app.world_mut()
            .query::<&BouncingSmogBubble>()
            .iter(app.world())
            .count(),
        0,
        "no stand-in actor may be spawned"
    );
    let failure = smog_failure(&app).ok_or("startup must record a typed smog failure")?;
    assert!(failure.contains("no admitted mobile"), "{failure}");
    assert_eq!(
        app.world().resource::<client::LabState>().battle_failure,
        Some(failure)
    );
    assert!(has_error_exit(&app), "a bounded run must exit with failure");
    Ok(())
}

/// C-6: invalid arena bounds were logged at `warn` and left a client with no bubbles.
#[test]
fn smog_startup_with_invalid_bounds_reports_a_typed_failure(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState::default())?;
    app.insert_resource(SmogConfig {
        bounds: SmogArenaBounds {
            min: Vec3::splat(4.0),
            max: Vec3::splat(-4.0),
        },
        ..default()
    });

    app.update();

    let failure = smog_failure(&app).ok_or("startup must record a typed smog failure")?;
    assert!(failure.contains("invalid arena bounds"), "{failure}");
    Ok(())
}

/// C-6: a failing kinematics step was logged at `warn` and the bubbles froze.
#[test]
fn smog_runtime_step_failure_reaches_the_media_failure_path(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(bouncing_lab())?;
    app.update();
    app.update();
    assert_eq!(smog_failure(&app), None);

    app.world_mut().resource_mut::<SmogConfig>().bounds = SmogArenaBounds {
        min: Vec3::splat(4.0),
        max: Vec3::splat(-4.0),
    };
    app.update();

    let failure = smog_failure(&app).ok_or("a failed step must record a typed failure")?;
    assert!(failure.contains("invalid arena bounds"), "{failure}");
    let steps = app.world().resource::<SmogStats>().step_count;
    app.update();
    assert_eq!(
        app.world().resource::<SmogStats>().step_count,
        steps,
        "a failed presentation must stop stepping"
    );
    Ok(())
}

/// C-6: bubbles are real catalog identities with their own media, owned by a palette preview role.
#[test]
fn smog_actors_use_admitted_identities_and_palette_owners() -> Result<(), Box<dyn std::error::Error>>
{
    let mut app = production_smog_app(client::LabState::default())?;
    app.update();
    let world = app.world_mut();
    let bubbles: Vec<_> = world
        .query::<(&BouncingSmogBubble, &client::BubbleInstance)>()
        .iter(world)
        .map(|(bubble, instance)| (bubble.index, instance.0.clone()))
        .collect();
    assert_eq!(bubbles.len(), BOUNCING_BUBBLE_COUNT);
    let world = app.world();
    let palette = world.resource::<client::player_presentation::PlayerPalette>();
    let catalog = world.resource::<client::CatalogResource>();
    let registry = world.resource::<client::FlipbookRegistry>();
    let media = world.resource::<client::media::ClientMediaStatus>();
    for (index, instance) in bubbles {
        assert_eq!(instance.owner, palette.showcase(index));
        assert!(catalog.get(instance.unit_type).is_ok());
        media.require(registry, instance.unit_type, instance.anim_state)?;
        assert!(
            instance.entity_id.0 >= client::smog::SHOWCASE_ENTITY_ID_BASE,
            "{:?} is inside the range the simulation allocates from",
            instance.entity_id
        );
    }
    Ok(())
}

/// Live battle app on a flat map of the given size, with one camera.
fn live_battle_smog_app(
    width: u32,
    height: u32,
    camera_target: Vec2,
) -> Result<App, Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState {
        current_tab: client::LabTab::LiveBattle,
        ..default()
    })?;
    let map = sim_map::GameMap::new(
        width,
        height,
        sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
    )?;
    app.insert_resource(client::terrain::TerrainState::new(map));
    app.world_mut().spawn(client::camera::RtsCamera {
        target: camera_target,
        look_at_y: 1.2,
        ..default()
    });
    Ok(app)
}

/// C-13: the window clamp was fixed to a 64x64 map, so the far side of a larger map had no smog.
#[test]
fn live_battle_volume_follows_the_camera_across_a_map_larger_than_64(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = live_battle_smog_app(128, 96, Vec2::new(100.0, 80.0))?;
    app.update();
    app.update();

    let grid = app.world().resource::<SmogGrid3d>();
    let center = (grid.bounds_min() + grid.bounds_max()) * 0.5;
    assert!((center.x - 100.0).abs() < 1.0, "center x {}", center.x);
    assert!((center.z - 80.0).abs() < 1.0, "center z {}", center.z);
    Ok(())
}

/// C-13: the same clamp must keep the volume on a small map instead of hanging off its edge.
#[test]
fn live_battle_volume_stays_inside_a_map_smaller_than_64() -> Result<(), Box<dyn std::error::Error>>
{
    let mut app = live_battle_smog_app(40, 36, Vec2::new(39.0, 35.0))?;
    app.update();
    app.update();

    let grid = app.world().resource::<SmogGrid3d>();
    assert!(
        grid.bounds_max().x <= 40.0 + 1.0 && grid.bounds_max().z <= 36.0 + 1.0,
        "volume reaches {} on a 40x36 map",
        grid.bounds_max()
    );
    Ok(())
}

/// A map narrower than the window centres the volume, rather than panicking on an inverted clamp.
#[test]
fn live_battle_volume_centres_on_a_map_narrower_than_its_window(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = live_battle_smog_app(20, 16, Vec2::new(3.0, 3.0))?;
    app.update();
    app.update();

    let grid = app.world().resource::<SmogGrid3d>();
    let center = (grid.bounds_min() + grid.bounds_max()) * 0.5;
    assert!((center.x - 10.0).abs() < 1.0, "center x {}", center.x);
    assert!((center.z - 8.0).abs() < 1.0, "center z {}", center.z);
    Ok(())
}

/// A live battle window exists only once a map does, so no map means no smog rather than a guess.
#[test]
fn live_battle_without_a_loaded_map_does_not_step_smog() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState {
        current_tab: client::LabTab::LiveBattle,
        ..default()
    })?;
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(app.world().resource::<SmogStats>().step_count, 0);
    let mut volumes = app
        .world_mut()
        .query_filtered::<&Visibility, With<client::SmogVolumeMarker>>();
    assert_eq!(volumes.single(app.world()), Visibility::Hidden);
    Ok(())
}

/// C-13: snapshot velocity is world units per tick and the tick rate is 25 Hz, not 20.
#[test]
fn live_battle_velocity_scale_comes_from_the_simulation_tick_rate(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = live_battle_smog_app(64, 64, Vec2::new(24.0, 24.0))?;
    app.world_mut().spawn((
        make_test_battle_unit(9001, Vec3::new(24.0, 0.45, 24.0), 0.0, 0.45),
        Transform::from_translation(Vec3::new(24.0, 0.45, 24.0)),
        Visibility::Inherited,
    ));
    app.update();
    app.update();

    let velocity = app
        .world()
        .resource::<client::smog::SmogGpuFrame>()
        .uniform
        .bubble_vel_boost[0];
    let per_tick = 0.12; // `make_test_battle_unit`
    let expected = per_tick / client::battle_pipeline::clock::SIM_TICK_PERIOD.as_secs_f32();
    assert!(
        (velocity.x - expected).abs() < 1e-4,
        "world units per second: got {}, expected {expected}",
        velocity.x
    );
    Ok(())
}

/// C-13: with more units than analytic cavities, the slots must go to the units nearest the view
/// centre. The old code kept whichever 16 came first in query order.
#[test]
fn live_battle_cavity_slots_go_to_the_units_nearest_the_view_center(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = live_battle_smog_app(64, 64, Vec2::new(24.0, 24.0))?;
    for index in 0..20u32 {
        // The first entity spawned is the farthest, the last is on the view centre.
        let offset = (20 - index) as f32 * 0.5;
        let position = Vec3::new(24.0 + offset, 0.45, 24.0);
        app.world_mut().spawn((
            make_test_battle_unit(9000 + index, position, 0.0, 0.45),
            Transform::from_translation(position),
            Visibility::Inherited,
        ));
    }
    app.update();
    app.update();

    let uniform = app.world().resource::<client::smog::SmogGpuFrame>().uniform;
    assert_eq!(
        uniform.control_params.x,
        client::MAX_SMOG_SHADER_BUBBLES as f32
    );
    let mut slots: Vec<f32> = uniform
        .bubble_pos_radius
        .iter()
        .map(|slot| (slot.x - 24.0) / 0.5)
        .collect();
    slots.sort_by(f32::total_cmp);
    let expected: Vec<f32> = (1..=16).map(|offset| offset as f32).collect();
    assert_eq!(slots, expected);
    Ok(())
}

/// C-13: smog reads the transforms the presentation stage wrote this frame. The probe is registered
/// after the plugin, so without ordering the single-threaded executor would run smog first.
#[test]
fn smog_runs_in_present_after_the_frame_is_published() -> Result<(), Box<dyn std::error::Error>> {
    use client::battle_pipeline::BattleSet;

    #[derive(Component)]
    struct Probe;

    let mut app = live_battle_smog_app(64, 64, Vec2::new(24.0, 24.0))?;
    let start = Vec3::new(24.0, 0.45, 24.0);
    app.world_mut().spawn((
        Probe,
        make_test_battle_unit(9001, start, 0.0, 0.45),
        Transform::from_translation(start),
        Visibility::Inherited,
    ));
    app.configure_sets(
        Update,
        (BattleSet::PreparePresentation, BattleSet::Present).chain(),
    );
    app.add_systems(
        Update,
        (|mut units: Query<'_, '_, &mut Transform, With<Probe>>| {
            for mut transform in &mut units {
                transform.translation.x += 0.25;
            }
        })
        .in_set(BattleSet::PreparePresentation),
    );
    app.edit_schedule(Update, |schedule| {
        schedule.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
    });

    app.update();
    app.update();

    let packed = app
        .world()
        .resource::<client::smog::SmogGpuFrame>()
        .uniform
        .bubble_pos_radius[0]
        .x;
    let published = app
        .world_mut()
        .query_filtered::<&Transform, With<Probe>>()
        .single(app.world())
        .translation
        .x;
    assert_eq!(packed, published);

    // An executor may still pick the right order by luck, so also require the declared membership.
    let schedules = app.world().resource::<Schedules>();
    let schedule = schedules.get(Update).ok_or("Update schedule")?;
    let graph = schedule.graph();
    let present = graph
        .system_sets()
        .find(|(_, set, _)| format!("{set:?}") == "Present")
        .map(|(id, _, _)| id)
        .ok_or("Present set")?;
    for name in [
        "sync_bouncing_smog_bubbles_system",
        "update_smog_field_system",
    ] {
        let system = schedule
            .systems()?
            .find(|(_, system)| system.name().ends_with(name))
            .map(|(id, _)| id)
            .ok_or(name)?;
        assert!(
            graph.hierarchy().graph().contains_edge(present, system),
            "{name} must be a member of BattleSet::Present"
        );
    }
    Ok(())
}

/// C-2 end to end: the real render world writes each published step to the GPU exactly once.
#[test]
fn smog_steps_reach_the_gpu_through_queue_writes() -> Result<(), Box<dyn std::error::Error>> {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let mut command =
        gpu_run::virtual_gpu_command(std::path::Path::new(env!("CARGO_BIN_EXE_client")));
    command.args(["--lab", "--max-frames", "60"]);
    command.env(
        "RUST_LOG",
        format!("warn,{}=debug", client::smog::SMOG_UPLOAD_LOG_TARGET),
    );
    let output = command.output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "client exited with {:?}:\n{}",
        output.status.code(),
        gpu_run::tail(&stderr)
    );
    let generations: Vec<u64> = stderr
        .lines()
        .filter_map(|line| {
            line.rsplit_once("uploaded smog generation ")?
                .1
                .trim()
                .parse()
                .ok()
        })
        .collect();
    assert!(
        generations.len() >= 30,
        "expected a GPU upload per rendered step, saw {}:\n{}",
        generations.len(),
        gpu_run::tail(&stderr)
    );
    assert!(
        generations.windows(2).all(|pair| pair[0] < pair[1]),
        "generations must strictly increase: {generations:?}"
    );
    Ok(())
}
