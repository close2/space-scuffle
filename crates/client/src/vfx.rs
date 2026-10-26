//! Particle mist wake, combat weapon VFX, and death shatter explosions.
//!
//! # Architectural Responsibilities
//! This module renders all dynamic combat VFX:
//! - **Trailing Mist & Sparks Wake:** Directional vapor wake trailing moving bubbles opposite to velocity.
//! - **Hitscan Tracers:** Instant ballistic lines with muzzle flash and impact ripples.
//! - **Traveling Projectiles:** Plasma grenade bolts and missiles with tail glow.
//! - **Continuous Beams & Lightning:** Continuous lasers and crackling jagged lightning arcs.
//! - **Melee Crescent Slashes:** Psi-blade sweep arcs across contact boundaries.
//! - **Death Glass Shatter:** 16 refractive glass shards violently exploding outward with physics momentum.

#![allow(dead_code)]

use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef, ShaderType};
use sim_types::ids::TeamId;
use sim_types::snapshot::CombatEventVisual;
use std::sync::atomic::{AtomicU32, Ordering};

/// GPU uniform buffer struct matching `struct MistWakeUniform` in `mist_wake.wgsl`.
// PERFORMANCE RATIONALE:
// Why: WebGPU uniform buffers require strict 16-byte alignment and explicit std140 padding.
// What: Fields total exactly 32 bytes (16B Vec4 + 4x 4B fields).
// Trade-off: Explicit manual padding for zero runtime alignment faults.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
pub struct MistWakeUniformGpu {
    /// Base tint color (RGB) + flow_angle_rad in W.
    pub base_color: Vec4,
    /// Normalized particle lifetime progress [0.0, 1.0].
    pub age: f32,
    /// Particulate kind / mode:
    /// 0 = colorful iridescent nebula wake + slipstream
    /// 1 = golden-magenta thruster overdrive
    /// 2 = iridescent micro-bubble cluster + star-sparks
    pub speed_boosted: u32,
    /// Random noise and rotation seed.
    pub seed: f32,
    /// Aspect ratio of billboard or flow speed factor.
    pub aspect: f32,
}

impl Default for MistWakeUniformGpu {
    fn default() -> Self {
        Self {
            base_color: Vec4::new(0.18, 0.90, 1.0, 0.0),
            age: 0.0,
            speed_boosted: 0,
            seed: 0.0,
            aspect: 1.0,
        }
    }
}

/// Custom Bevy 3D material implementing the volumetric additive mist wake shader.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct MistWakeMaterial {
    /// GPU uniform buffer for mist wake particle.
    #[uniform(0)]
    pub uniform: MistWakeUniformGpu,
}

impl Material for MistWakeMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/mist_wake.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn depth_bias(&self) -> f32 {
        -4.0
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline<Self>,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::render::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// GPU uniform representation for procedural volumetric combat visual effects.
// PERFORMANCE RATIONALE:
// Why: WebGPU uniform buffers require strict 16-byte alignment and explicit std140 packing.
// What: Fields total exactly 32 bytes (16B Vec4 + 4x 4B primitive fields).
// Trade-off: Explicit manual packing ensures zero runtime WebGPU alignment validation panics.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
pub struct CombatFxUniformGpu {
    /// Primary HDR color (RGB) + Secondary parameter / angle (W).
    pub primary_color: Vec4,
    /// Normalized effect lifetime progress [0.0, 1.0].
    pub age: f32,
    /// Effect classification kind:
    /// 0 = FX_LASER_BEAM (Continuous high-energy laser with muzzle & impact flares)
    /// 1 = FX_HITSCAN_TRACER (Supersonic kinetic/pulse tracer with Mach shock rings)
    /// 2 = FX_LIGHTNING_ARC (3-phase domain-warped jagged electric arc)
    /// 3 = FX_MELEE_SLASH (Sweeping crescent psi-blade with trailing energy veil)
    /// 4 = FX_PLASMA_PROJECTILE (Blazing comet-tail plasma orb)
    /// 5 = FX_IMPACT_STARBURST (8-pointed radial starburst & refractive shockwave)
    pub fx_kind: u32,
    /// Randomization seed for turbulence, branching, and sparks.
    pub seed: f32,
    /// Geometric aspect ratio (length / width for ribbons, radius for discs).
    pub aspect: f32,
}

impl Default for CombatFxUniformGpu {
    fn default() -> Self {
        Self {
            primary_color: Vec4::new(1.0, 1.0, 1.0, 0.0),
            age: 0.0,
            fx_kind: 0,
            seed: 0.0,
            aspect: 1.0,
        }
    }
}

/// Custom Bevy 3D material implementing the procedural volumetric combat VFX shader.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct CombatFxMaterial {
    /// GPU uniform buffer for combat VFX.
    #[uniform(0)]
    pub uniform: CombatFxUniformGpu,
}

impl Material for CombatFxMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/combat_fx.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn depth_bias(&self) -> f32 {
        -25.0
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline<Self>,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::render::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Bevy ECS Event wrapping authoritative [`CombatEventVisual`] data.
#[derive(Event, Clone, Debug)]
pub struct CombatVisualEvent(pub CombatEventVisual);

/// Classification of trailing particulate wake emissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MistParticleKind {
    /// Soft expanding radial vapor puff.
    VaporPuff,
    /// Tiny upward drifting micro-bubble.
    MicroBubble,
    /// High-velocity glowing overclock spark streak.
    OverclockSpark,
}

/// Component for trailing particulate wake elements.
#[derive(Component, Debug)]
pub struct MistWakeParticle {
    /// Particulate classification kind.
    pub kind: MistParticleKind,
    /// Linear movement velocity.
    pub velocity: Vec3,
    /// Remaining particle lifetime timer.
    pub lifetime: Timer,
    /// Initial particle scale.
    pub initial_scale: f32,
    /// Target particle scale at end of lifetime.
    pub target_scale: f32,
}

/// Component for in-flight physical projectiles.
#[derive(Component, Debug)]
pub struct ProjectileVisual {
    /// Linear flight velocity per second.
    pub velocity: Vec3,
    /// Lifetime timer.
    pub lifetime: Timer,
    /// Procedural combat VFX material instance handle.
    pub material_handle: Handle<CombatFxMaterial>,
}

/// Component for sustained continuous laser or lightning beams.
#[derive(Component, Debug)]
pub struct BeamVisualComponent {
    /// Origin coordinate.
    pub from: Vec3,
    /// Impact coordinate.
    pub to: Vec3,
    /// If true, renders jagged lightning arc; if false, straight laser beam.
    pub is_lightning: bool,
    /// Remaining duration timer.
    pub lifetime: Timer,
    /// Procedural combat VFX material instance handle.
    pub material_handle: Handle<CombatFxMaterial>,
}

/// Component for melee crescent slash arcs.
#[derive(Component, Debug)]
pub struct MeleeSlashComponent {
    /// Expansion and fade timer.
    pub lifetime: Timer,
    /// Target sweep radius.
    pub radius: f32,
    /// Procedural combat VFX material instance handle.
    pub material_handle: Handle<CombatFxMaterial>,
}

/// Component for exploding death glass shards.
#[derive(Component, Debug)]
pub struct ShatterShard {
    /// Linear explosion velocity vector.
    pub velocity: Vec3,
    /// Angular rotational velocity.
    pub angular_velocity: Vec3,
    /// Lifespan timer.
    pub lifetime: Timer,
    /// Procedural combat VFX material instance handle.
    pub material_handle: Handle<CombatFxMaterial>,
}

/// Shared mesh and material assets for combat VFX.
#[derive(Resource)]
pub struct VfxSharedAssets {
    /// Mist wake particle sphere mesh.
    pub mist_mesh: Handle<Mesh>,
    /// Hitscan tracer beam cylinder mesh.
    pub tracer_mesh: Handle<Mesh>,
    /// Plasma projectile sphere mesh.
    pub projectile_mesh: Handle<Mesh>,
    /// Laser beam cylinder mesh.
    pub beam_mesh: Handle<Mesh>,
    /// Melee crescent torus wedge mesh.
    pub melee_slash_mesh: Handle<Mesh>,
    /// Death shatter wedge prism mesh.
    pub shard_mesh: Handle<Mesh>,
    /// Blue team emissive weapon material.
    pub blue_beam_mat: Handle<StandardMaterial>,
    /// Red team emissive weapon material.
    pub red_beam_mat: Handle<StandardMaterial>,
    /// Mist wake material.
    pub mist_mat: Handle<StandardMaterial>,
    /// Hyper-boosted spark material.
    pub spark_mat: Handle<StandardMaterial>,
}

/// Sets up shared VFX mesh and material resources.
pub fn setup_vfx_assets_system(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
) {
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    let assets = VfxSharedAssets {
        mist_mesh: quad.clone(),
        tracer_mesh: quad.clone(),
        projectile_mesh: quad.clone(),
        beam_mesh: quad.clone(),
        melee_slash_mesh: quad.clone(),
        shard_mesh: quad,
        blue_beam_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.7, 1.0),
            emissive: LinearRgba::rgb(0.5, 2.0, 4.0),
            unlit: true,
            ..default()
        }),
        red_beam_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.25, 0.15),
            emissive: LinearRgba::rgb(4.0, 0.6, 0.3),
            unlit: true,
            ..default()
        }),
        mist_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(0.8, 0.9, 1.0, 0.35),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        spark_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.95, 0.4),
            emissive: LinearRgba::rgb(4.0, 3.5, 1.0),
            unlit: true,
            ..default()
        }),
    };
    commands.insert_resource(assets);
}

// PERFORMANCE RATIONALE:
// Why: Uncapped particle emission when 200 bubbles are moving causes rapid frame rate drops.
// What: Shards, beams, and mist particles use strict hard lifetimes (0.15s - 0.75s) and are
// automatically culled and despawned in batches.
// Trade-off: Particles do not persist indefinitely, which aligns with RTS visual readability.

/// Spawns a central detonation starburst and 16 crystalline glass shards collapsing outward when a bubble or dome is destroyed.
pub fn spawn_death_shatter(
    commands: &mut Commands<'_, '_>,
    assets: &VfxSharedAssets,
    combat_fx_materials: &mut Assets<CombatFxMaterial>,
    origin: Vec3,
    radius: f32,
    team_id: TeamId,
) {
    let team_hdr = match team_id.0 {
        1 => Vec4::new(0.2, 0.75, 1.0, 0.0),
        2 => Vec4::new(1.0, 0.35, 0.15, 0.0),
        _ => Vec4::new(0.9, 0.9, 0.9, 0.0),
    };

    // 1. Central Detonation Starburst + Refractive Shockwave Ring (Mode 5)
    let starburst_mat = combat_fx_materials.add(CombatFxMaterial {
        uniform: CombatFxUniformGpu {
            primary_color: team_hdr,
            age: 0.0,
            fx_kind: 5,
            seed: (origin.x * 17.3 + origin.z * 31.7).fract().abs(),
            aspect: 1.0,
        },
    });

    commands.spawn((
        Mesh3d(assets.shard_mesh.clone()),
        MeshMaterial3d(starburst_mat.clone()),
        Transform::from_translation(origin).with_scale(Vec3::splat(radius * 3.2)),
        BeamVisualComponent {
            from: origin,
            to: origin,
            is_lightning: false,
            lifetime: Timer::from_seconds(0.65, TimerMode::Once),
            material_handle: starburst_mat,
        },
    ));

    // 2. 16 Explosive Crystalline Glass Shards (Mode 5 crystalline shard silhouette)
    for i in 0..16 {
        let phi = (i as f32) * 2.399_963_2; // Golden ratio spiral
        let theta = (i as f32) * 1.618_034;
        let dir = Vec3::new(
            theta.sin() * phi.cos(),
            theta.cos().abs() + 0.25, // Upward explosion bias
            theta.sin() * phi.sin(),
        )
        .normalize();

        let shard_mat = combat_fx_materials.add(CombatFxMaterial {
            uniform: CombatFxUniformGpu {
                primary_color: team_hdr,
                age: 0.0,
                fx_kind: 5,
                seed: (phi * 7.1 + theta * 13.3).fract().abs(),
                aspect: 1.0,
            },
        });

        commands.spawn((
            Mesh3d(assets.shard_mesh.clone()),
            MeshMaterial3d(shard_mat.clone()),
            Transform::from_translation(origin + dir * (radius * 0.4))
                .with_scale(Vec3::splat(radius.clamp(0.5, 2.0))),
            ShatterShard {
                velocity: dir * (radius * 7.0 + 3.0),
                angular_velocity: Vec3::new(phi * 4.0, theta * 3.0, 2.5),
                lifetime: Timer::from_seconds(0.75, TimerMode::Once),
                material_handle: shard_mat,
            },
        ));
    }
}

/// Monotonically advancing emission counter for high-entropy golden ratio angle dispersal.
static MIST_EMISSION_COUNTER: AtomicU32 = AtomicU32::new(1);

/// Spawns aerodynamic air-suction wake particles when moving (leaving stationary bubbles crystal-clear).
pub fn emit_mist_wake(
    commands: &mut Commands<'_, '_>,
    assets: &VfxSharedAssets,
    mist_materials: &mut Assets<MistWakeMaterial>,
    bubble_pos: Vec3,
    velocity: Vec2,
    radius: f32,
    speed_boosted: bool,
    is_lab_preview: bool,
) {
    let speed = velocity.length();

    // When stationary, do not spawn external puffs to keep the glass sphere razor-sharp and perfectly round
    if speed <= 0.0005 {
        return;
    }

    // Moving bubble: Aerodynamic Air Suction Slipstream
    let world_speed = if speed < 0.5 { speed * 25.0 } else { speed };
    let forward_dir = Vec3::new(velocity.x, 0.0, velocity.y).normalize();
    let trail_dir = -forward_dir;
    let right_dir = Vec3::new(-trail_dir.z, 0.0, trail_dir.x);
    let up_dir = Vec3::Y;

    let forward_screen = Vec2::new(velocity.x, -velocity.y).normalize_or_zero();
    let trail_screen = -forward_screen;
    let flow_angle = -trail_screen.y.atan2(trail_screen.x);

    let lab_conveyor = if is_lab_preview {
        trail_dir * (world_speed * 1.25)
    } else {
        Vec3::ZERO
    };

    let counter = MIST_EMISSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let seed_salt = ((counter as f32) * 0.618_034).fract();

    // 2 sweated mist wisps per emission on the rear surface, pulled inward by air suction
    let side_sign = if counter % 2 == 0 { 1.0 } else { -1.0 };
    let lateral_vec = right_dir * (side_sign * radius * 0.22)
        + up_dir * (((seed_salt * std::f32::consts::TAU).sin()) * radius * 0.15);
    let spawn_pos = bubble_pos + trail_dir * (radius * 0.78) + lateral_vec;

    // Air suction: pulls lateral mist inward toward the wake centerline + slight forward slipstream drag
    let suction_inward = -lateral_vec * 2.2;
    let forward_suction = forward_dir * (world_speed * 0.18);
    let velocity_vec = lab_conveyor + suction_inward + forward_suction;

    let initial_scale = radius * 2.2;
    let target_scale = radius * 1.45;

    let mat = mist_materials.add(MistWakeMaterial {
        uniform: MistWakeUniformGpu {
            base_color: Vec4::new(0.15, 0.88, 1.0, flow_angle),
            age: 0.0,
            speed_boosted: u32::from(speed_boosted),
            seed: seed_salt,
            aspect: 1.25,
        },
    });

    commands.spawn((
        Mesh3d(assets.mist_mesh.clone()),
        MeshMaterial3d(mat),
        Transform::from_translation(spawn_pos).with_scale(Vec3::splat(initial_scale)),
        MistWakeParticle {
            kind: MistParticleKind::VaporPuff,
            velocity: velocity_vec,
            lifetime: Timer::from_seconds(0.95, TimerMode::Once),
            initial_scale,
            target_scale,
        },
    ));
}

/// Updates and advances mist wake particles with billboard camera alignment, lifetime aging, and exponential aerodynamic drag.
pub fn update_mist_wake_system(
    mut commands: Commands<'_, '_>,
    time: Res<'_, Time>,
    camera_query: Query<'_, '_, &Transform, (With<Camera3d>, Without<MistWakeParticle>)>,
    mut mist_materials: ResMut<'_, Assets<MistWakeMaterial>>,
    mut query: Query<
        '_,
        '_,
        (
            Entity,
            &mut Transform,
            &mut MistWakeParticle,
            &MeshMaterial3d<MistWakeMaterial>,
        ),
    >,
) {
    let dt = time.delta_secs();
    let cam_rotation = camera_query
        .get_single()
        .map_or(Quat::IDENTITY, |t| t.rotation);

    for (entity, mut transform, mut particle, mat_handle) in &mut query {
        particle.lifetime.tick(time.delta());
        if particle.lifetime.finished() {
            commands.entity(entity).despawn();
        } else {
            // Exponential aerodynamic air damping bringing mist quickly to a rest in ambient air
            let drag = (-2.2 * dt).exp();
            particle.velocity *= drag;

            if particle.kind == MistParticleKind::MicroBubble {
                particle.velocity.y += 0.25 * dt;
            }

            transform.translation += particle.velocity * dt;
            let progress = particle.lifetime.fraction();
            let scale = particle.initial_scale
                + (particle.target_scale - particle.initial_scale) * progress;
            transform.scale = Vec3::splat(scale);
            transform.rotation = cam_rotation;

            if let Some(mat) = mist_materials.get_mut(&mat_handle.0) {
                mat.uniform.age = progress;
            }
        }
    }
}

/// Canonical alias for the visual effects update system.
pub use update_mist_wake_system as update_vfx_system;

/// Updates in-flight projectiles with lifetime aging, material uniform updates, and velocity alignment.
pub fn update_projectiles_system(
    mut commands: Commands<'_, '_>,
    time: Res<'_, Time>,
    camera_query: Query<'_, '_, &Transform, (With<Camera3d>, Without<ProjectileVisual>)>,
    mut fx_materials: ResMut<'_, Assets<CombatFxMaterial>>,
    mut query: Query<'_, '_, (Entity, &mut Transform, &mut ProjectileVisual)>,
) {
    let dt = time.delta_secs();
    let cam_rotation = camera_query
        .get_single()
        .map_or(Quat::IDENTITY, |t| t.rotation);

    for (entity, mut transform, mut proj) in &mut query {
        proj.lifetime.tick(time.delta());
        if proj.lifetime.finished() {
            commands.entity(entity).despawn();
        } else {
            transform.translation += proj.velocity * dt;
            let progress = proj.lifetime.fraction();
            if let Some(mat) = fx_materials.get_mut(&proj.material_handle) {
                mat.uniform.age = progress;
            }

            if proj.velocity.length_squared() > 0.01 {
                transform.rotation = Quat::from_rotation_arc(-Vec3::X, proj.velocity.normalize());
            } else {
                transform.rotation = cam_rotation;
            }
        }
    }
}

/// Updates continuous beam and lightning ribbon components with lifetime aging and billboard camera orientation.
pub fn update_beams_system(
    mut commands: Commands<'_, '_>,
    time: Res<'_, Time>,
    camera_query: Query<'_, '_, &Transform, (With<Camera3d>, Without<BeamVisualComponent>)>,
    mut fx_materials: ResMut<'_, Assets<CombatFxMaterial>>,
    mut query: Query<'_, '_, (Entity, &mut Transform, &mut BeamVisualComponent)>,
) {
    let cam_pos = camera_query
        .get_single()
        .map_or(Vec3::Y * 20.0, |t| t.translation);
    for (entity, mut transform, mut beam) in &mut query {
        beam.lifetime.tick(time.delta());
        if beam.lifetime.finished() {
            commands.entity(entity).despawn();
        } else {
            let progress = beam.lifetime.fraction();
            if let Some(mat) = fx_materials.get_mut(&beam.material_handle) {
                mat.uniform.age = progress;
            }

            let delta = beam.to - beam.from;
            let length = delta.length();
            if length > 0.01 {
                let dir = delta / length;
                let center = beam.from + delta * 0.5;
                let to_cam = (cam_pos - center).normalize_or_zero();
                let right = dir.cross(to_cam).normalize_or_zero();
                let normal = right.cross(dir).normalize_or_zero();
                transform.rotation = Quat::from_mat3(&Mat3::from_cols(right, dir, normal));
            }
        }
    }
}

/// Updates expanding crescent psi-blade slashes with lifetime aging and uniform progression.
pub fn update_melee_slashes_system(
    mut commands: Commands<'_, '_>,
    time: Res<'_, Time>,
    mut fx_materials: ResMut<'_, Assets<CombatFxMaterial>>,
    mut query: Query<'_, '_, (Entity, &mut Transform, &mut MeleeSlashComponent)>,
) {
    for (entity, mut transform, mut slash) in &mut query {
        slash.lifetime.tick(time.delta());
        if slash.lifetime.finished() {
            commands.entity(entity).despawn();
        } else {
            let progress = slash.lifetime.fraction();
            if let Some(mat) = fx_materials.get_mut(&slash.material_handle) {
                mat.uniform.age = progress;
            }
            let expansion = 1.0 + progress * 0.35;
            transform.scale = Vec3::splat(slash.radius * 2.6 * expansion);
        }
    }
}

/// Updates and advances death shatter glass shards under gravity with procedural volumetric starburst materials.
pub fn update_shatter_system(
    mut commands: Commands<'_, '_>,
    time: Res<'_, Time>,
    camera_query: Query<'_, '_, &Transform, (With<Camera3d>, Without<ShatterShard>)>,
    mut fx_materials: ResMut<'_, Assets<CombatFxMaterial>>,
    mut query: Query<'_, '_, (Entity, &mut Transform, &mut ShatterShard)>,
) {
    let dt = time.delta_secs();
    let gravity = Vec3::new(0.0, -18.0, 0.0);
    let cam_rotation = camera_query
        .get_single()
        .map_or(Quat::IDENTITY, |t| t.rotation);

    for (entity, mut transform, mut shard) in &mut query {
        shard.lifetime.tick(time.delta());
        if shard.lifetime.finished() {
            commands.entity(entity).despawn();
        } else {
            let progress = shard.lifetime.fraction();
            if let Some(mat) = fx_materials.get_mut(&shard.material_handle) {
                mat.uniform.age = progress;
            }

            if shard.velocity != Vec3::ZERO {
                shard.velocity += gravity * dt;
                transform.translation += shard.velocity * dt;
                transform.rotate(Quat::from_euler(
                    EulerRot::XYZ,
                    shard.angular_velocity.x * dt,
                    shard.angular_velocity.y * dt,
                    shard.angular_velocity.z * dt,
                ));
            } else {
                transform.rotation = cam_rotation;
            }
        }
    }
}

/// Consumes and visualizes all combat events received from the authoritative simulation snapshot via procedural volumetric shader instances.
pub fn process_combat_events_system(
    mut commands: Commands<'_, '_>,
    mut event_reader: EventReader<'_, '_, CombatVisualEvent>,
    vfx_assets: Res<'_, VfxSharedAssets>,
    camera_query: Query<'_, '_, &Transform, With<Camera3d>>,
    mut fx_materials: ResMut<'_, Assets<CombatFxMaterial>>,
) {
    let cam_pos = camera_query
        .get_single()
        .map_or(Vec3::Y * 20.0, |t| t.translation);

    for event_wrapper in event_reader.read() {
        match &event_wrapper.0 {
            CombatEventVisual::HitscanTracer { from, to, team_id } => {
                let p1 = Vec3::new(from[0], from[2], from[1]);
                let p2 = Vec3::new(to[0], to[2], to[1]);
                let delta = p2 - p1;
                let length = delta.length();
                if length > 0.01 {
                    let center = p1 + delta * 0.5;
                    let dir = delta / length;
                    let to_cam = (cam_pos - center).normalize_or_zero();
                    let right = dir.cross(to_cam).normalize_or_zero();
                    let normal = right.cross(dir).normalize_or_zero();
                    let rot = Quat::from_mat3(&Mat3::from_cols(right, dir, normal));

                    let team_hdr = match team_id.0 {
                        1 => Vec4::new(0.2, 0.75, 1.0, 0.0),
                        2 => Vec4::new(1.0, 0.35, 0.15, 0.0),
                        _ => Vec4::new(0.9, 0.9, 0.9, 0.0),
                    };

                    let mat = fx_materials.add(CombatFxMaterial {
                        uniform: CombatFxUniformGpu {
                            primary_color: team_hdr,
                            age: 0.0,
                            fx_kind: 1, // Mode 1: Supersonic Hitscan / Railgun Tracer Bolt
                            seed: (p1.x * 13.1 + p2.z * 19.7).fract().abs(),
                            aspect: length / 0.55,
                        },
                    });

                    commands.spawn((
                        Mesh3d(vfx_assets.tracer_mesh.clone()),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_translation(center)
                            .with_rotation(rot)
                            .with_scale(Vec3::new(0.55, length, 1.0)),
                        BeamVisualComponent {
                            from: p1,
                            to: p2,
                            is_lightning: false,
                            lifetime: Timer::from_seconds(0.12, TimerMode::Once),
                            material_handle: mat,
                        },
                    ));
                }
            }
            CombatEventVisual::ProjectileVisual {
                pos,
                vel,
                team_id,
                duration_secs,
            } => {
                let p = Vec3::new(pos[0], pos[2], pos[1]);
                let v = Vec3::new(vel[0], vel[2], vel[1]);
                let team_hdr = match team_id.0 {
                    1 => Vec4::new(0.2, 0.75, 1.0, 0.0),
                    2 => Vec4::new(1.0, 0.35, 0.15, 0.0),
                    _ => Vec4::new(0.9, 0.9, 0.9, 0.0),
                };
                let lifetime = duration_secs.unwrap_or(0.35);

                let mat = fx_materials.add(CombatFxMaterial {
                    uniform: CombatFxUniformGpu {
                        primary_color: team_hdr,
                        age: 0.0,
                        fx_kind: 4, // Mode 4: Plasma projectile
                        seed: (p.x * 29.3 + p.z * 41.7).fract().abs(),
                        aspect: 1.0,
                    },
                });

                let rot = if v.length_squared() > 0.01 {
                    Quat::from_rotation_arc(-Vec3::X, v.normalize())
                } else {
                    Quat::IDENTITY
                };

                commands.spawn((
                    Mesh3d(vfx_assets.projectile_mesh.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_translation(p)
                        .with_rotation(rot)
                        .with_scale(Vec3::splat(1.1)),
                    ProjectileVisual {
                        velocity: v,
                        lifetime: Timer::from_seconds(lifetime, TimerMode::Once),
                        material_handle: mat,
                    },
                ));
            }
            CombatEventVisual::BeamVisual {
                from,
                to,
                is_lightning,
                team_id,
            } => {
                let p1 = Vec3::new(from[0], from[2], from[1]);
                let p2 = Vec3::new(to[0], to[2], to[1]);
                let delta = p2 - p1;
                let length = delta.length();
                if length > 0.01 {
                    let center = p1 + delta * 0.5;
                    let dir = delta / length;
                    let to_cam = (cam_pos - center).normalize_or_zero();
                    let right = dir.cross(to_cam).normalize_or_zero();
                    let normal = right.cross(dir).normalize_or_zero();
                    let rot = Quat::from_mat3(&Mat3::from_cols(right, dir, normal));

                    let team_hdr = match team_id.0 {
                        1 => Vec4::new(0.2, 0.75, 1.0, 0.0),
                        2 => Vec4::new(1.0, 0.35, 0.15, 0.0),
                        _ => Vec4::new(0.9, 0.9, 0.9, 0.0),
                    };

                    let fx_kind = if *is_lightning { 2 } else { 0 };
                    let width = if *is_lightning { 0.95 } else { 0.65 };
                    let duration = if *is_lightning { 0.18 } else { 0.25 };

                    let mat = fx_materials.add(CombatFxMaterial {
                        uniform: CombatFxUniformGpu {
                            primary_color: team_hdr,
                            age: 0.0,
                            fx_kind,
                            seed: (p1.x * 37.7 + p2.z * 53.1).fract().abs(),
                            aspect: length / width,
                        },
                    });

                    commands.spawn((
                        Mesh3d(vfx_assets.beam_mesh.clone()),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_translation(center)
                            .with_rotation(rot)
                            .with_scale(Vec3::new(width, length, 1.0)),
                        BeamVisualComponent {
                            from: p1,
                            to: p2,
                            is_lightning: *is_lightning,
                            lifetime: Timer::from_seconds(duration, TimerMode::Once),
                            material_handle: mat,
                        },
                    ));
                }
            }
            CombatEventVisual::MeleeSlashVisual {
                pos,
                angle_rad,
                radius,
                team_id,
            } => {
                let p = Vec3::new(pos[0], pos[2] + 0.35, pos[1]);
                let mut team_hdr = match team_id.0 {
                    1 => Vec4::new(0.2, 0.75, 1.0, 0.0),
                    2 => Vec4::new(1.0, 0.35, 0.15, 0.0),
                    _ => Vec4::new(0.9, 0.9, 0.9, 0.0),
                };
                team_hdr.w = *angle_rad;

                let mat = fx_materials.add(CombatFxMaterial {
                    uniform: CombatFxUniformGpu {
                        primary_color: team_hdr,
                        age: 0.0,
                        fx_kind: 3, // Mode 3: Sweeping Crescent Psi-Blade Melee Slash
                        seed: (pos[0] * 19.3 + pos[1] * 23.7).fract().abs(),
                        aspect: 1.0,
                    },
                });

                let rot = Quat::from_rotation_x(-0.6108) * Quat::from_rotation_y(-angle_rad);

                commands.spawn((
                    Mesh3d(vfx_assets.melee_slash_mesh.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_translation(p)
                        .with_rotation(rot)
                        .with_scale(Vec3::splat(*radius * 2.6)),
                    MeleeSlashComponent {
                        lifetime: Timer::from_seconds(0.20, TimerMode::Once),
                        radius: *radius,
                        material_handle: mat,
                    },
                ));
            }
            CombatEventVisual::BubbleShatter {
                pos,
                radius,
                is_dome: _,
                team_id,
            } => {
                let p = Vec3::new(pos[0], pos[2], pos[1]);
                spawn_death_shatter(
                    &mut commands,
                    &vfx_assets,
                    &mut fx_materials,
                    p,
                    *radius,
                    *team_id,
                );
            }
        }
    }
}
