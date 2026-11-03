//! Combat weapon VFX and death shatter explosions.
//!
//! # Architectural Responsibilities
//! This module renders all dynamic combat VFX:
//! - **Hitscan Tracers:** Instant ballistic lines with muzzle flash and impact ripples.
//! - **Traveling Projectiles:** Plasma grenade bolts and missiles with tail glow.
//! - **Continuous Beams & Lightning:** Continuous lasers and crackling jagged lightning arcs.
//! - **Melee Crescent Slashes:** Psi-blade sweep arcs across contact boundaries.
//! - **Death Glass Shatter:** 16 refractive glass shards violently exploding outward with physics momentum.

use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};
use sim_types::snapshot::CombatEventVisual;

mod gpu_layout {
    //! Holds one `ShaderType` struct, so the lint expectation below covers nothing else.
    #![expect(
        dead_code,
        reason = "the ShaderType derive emits layout checks nothing calls"
    )]

    use bevy::prelude::*;
    use bevy::render::render_resource::ShaderType;

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
        /// 0 = `FX_LASER_BEAM` (Continuous high-energy laser with muzzle & impact flares)
        /// 1 = `FX_HITSCAN_TRACER` (Supersonic kinetic/pulse tracer with Mach shock rings)
        /// 2 = `FX_LIGHTNING_ARC` (3-phase domain-warped jagged electric arc)
        /// 3 = `FX_MELEE_SLASH` (Sweeping crescent psi-blade with trailing energy veil)
        /// 4 = `FX_PLASMA_PROJECTILE` (Blazing comet-tail plasma orb)
        /// 5 = `FX_IMPACT_STARBURST` (8-pointed radial starburst & refractive shockwave)
        pub fx_kind: u32,
        /// Randomization seed for turbulence, branching, and sparks.
        pub seed: f32,
        /// Geometric aspect ratio (length / width for ribbons, radius for discs).
        pub aspect: f32,
    }
}
pub use gpu_layout::CombatFxUniformGpu;

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

/// Additive combat light geometry that must never cast an opaque mesh shadow.
///
/// Includes the current shader-card shatter fragments. Real opaque debris must use
/// its own material and component instead of this light-effect classification.
#[derive(Component, Debug)]
#[require(NotShadowCaster)]
pub struct CombatLightEffect;

/// Exact source ownership copied from the admitted visual event. This survives
/// source death and remains independent of palette hues and material handles.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatEffectOwner(pub sim_types::PlayerId);

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
}

/// Sets up shared VFX mesh and material resources.
pub fn setup_vfx_assets_system(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
) {
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    let assets = VfxSharedAssets {
        tracer_mesh: quad.clone(),
        projectile_mesh: quad.clone(),
        beam_mesh: quad.clone(),
        melee_slash_mesh: quad.clone(),
        shard_mesh: quad,
    };
    commands.insert_resource(assets);
}

// PERFORMANCE RATIONALE:
// Why: Uncapped particle emission when 200 bubbles are moving causes rapid frame rate drops.
// What: Shards and beams use strict hard lifetimes (0.15s - 0.75s) and are
// automatically culled and despawned in batches.
// Trade-off: Particles do not persist indefinitely, which aligns with RTS visual readability.

/// Spawns a central detonation starburst and 16 crystalline glass shards collapsing outward when a bubble or dome is destroyed.
#[expect(
    clippy::cast_precision_loss,
    reason = "presentation values are bounded by frame, atlas and window sizes"
)]
pub fn spawn_death_shatter(
    commands: &mut Commands<'_, '_>,
    assets: &VfxSharedAssets,
    combat_fx_materials: &mut Assets<CombatFxMaterial>,
    origin: Vec3,
    radius: f32,
    style: crate::player_presentation::PlayerStyle,
    owner: sim_types::PlayerId,
) {
    let team_hdr = style.linear.truncate().extend(0.0);

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
        CombatLightEffect,
        CombatEffectOwner(owner),
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
        let theta = (i as f32) * std::f32::consts::GOLDEN_RATIO;
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
            CombatLightEffect,
            CombatEffectOwner(owner),
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

/// Tilt of the melee slash card about world X, in radians, so it lies toward the RTS camera.
const MELEE_CARD_TILT_RAD: f32 = -0.6108;

/// Rotation of a projectile card: local -X follows `velocity`, and the face turns to the camera.
///
/// `position` and `camera_position` are world space. A shot along the view axis leaves the face
/// direction free, so any perpendicular is used. `velocity` must be nonzero.
#[must_use]
pub fn projectile_card_rotation(velocity: Vec3, position: Vec3, camera_position: Vec3) -> Quat {
    let tail = -velocity.normalize();
    let to_camera = (camera_position - position).normalize_or_zero();
    let toward_camera = to_camera.reject_from_normalized(tail);
    let face = toward_camera
        .try_normalize()
        .unwrap_or_else(|| tail.any_orthonormal_vector());
    Quat::from_mat3(&Mat3::from_cols(tail, face.cross(tail), face))
}

/// Updates in-flight projectiles with lifetime aging, material uniform updates, and velocity alignment.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn update_projectiles_system(
    mut commands: Commands<'_, '_>,
    time: Res<'_, Time>,
    camera_query: Query<'_, '_, &Transform, (With<Camera3d>, Without<ProjectileVisual>)>,
    mut fx_materials: ResMut<'_, Assets<CombatFxMaterial>>,
    mut query: Query<'_, '_, (Entity, &mut Transform, &mut ProjectileVisual)>,
) {
    let dt = time.delta_secs();
    let (cam_rotation, cam_pos) = camera_query
        .get_single()
        .map_or((Quat::IDENTITY, Vec3::Y * 20.0), |t| {
            (t.rotation, t.translation)
        });

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
                transform.rotation =
                    projectile_card_rotation(proj.velocity, transform.translation, cam_pos);
            } else {
                transform.rotation = cam_rotation;
            }
        }
    }
}

/// Updates continuous beam and lightning ribbon components with lifetime aging and billboard camera orientation.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
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
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
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
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
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

            if shard.velocity == Vec3::ZERO {
                transform.rotation = cam_rotation;
            } else {
                shard.velocity += gravity * dt;
                transform.translation += shard.velocity * dt;
                transform.rotate(Quat::from_euler(
                    EulerRot::XYZ,
                    shard.angular_velocity.x * dt,
                    shard.angular_velocity.y * dt,
                    shard.angular_velocity.z * dt,
                ));
            }
        }
    }
}

/// Snapshot `[x, y, altitude]` coordinates as a Bevy world position (Y up).
fn world_point(position: &[f32; 3]) -> Vec3 {
    Vec3::new(position[0], position[2], position[1])
}

/// A ribbon card stretched between two world points and turned to face the camera.
struct RibbonPlacement {
    /// Midpoint of the two ends, in world space.
    center: Vec3,
    /// Rotation taking the card's local Y along the ribbon with its face toward the camera.
    rotation: Quat,
    /// Distance between the two ends, in world units.
    length: f32,
}

impl RibbonPlacement {
    /// Places a ribbon from `from` to `to` facing `camera_position`, all in world space.
    ///
    /// Returns `None` unless the ends are more than 0.01 world units apart (a NaN length
    /// included), because closer ends have no direction.
    fn between(from: Vec3, to: Vec3, camera_position: Vec3) -> Option<Self> {
        let delta = to - from;
        let length = delta.length();
        if length > 0.01 {
            let center = from + delta * 0.5;
            let dir = delta / length;
            let to_cam = (camera_position - center).normalize_or_zero();
            let right = dir.cross(to_cam).normalize_or_zero();
            let normal = right.cross(dir).normalize_or_zero();
            Some(Self {
                center,
                rotation: Quat::from_mat3(&Mat3::from_cols(right, dir, normal)),
                length,
            })
        } else {
            None
        }
    }
}

/// Spawns combat effects for admitted visual events, one method per event kind.
struct CombatEffectSpawner<'a, 'w, 's> {
    /// Command queue the effect entities are spawned through.
    commands: &'a mut Commands<'w, 's>,
    /// Shared effect meshes.
    assets: &'a VfxSharedAssets,
    /// Store that receives one new material per effect.
    materials: &'a mut Assets<CombatFxMaterial>,
    /// Camera position in world space, which ribbons and projectile cards face.
    camera_position: Vec3,
}

impl CombatEffectSpawner<'_, '_, '_> {
    /// Spawns the effect of `event`, tinted with its source owner's `style`.
    fn spawn(
        &mut self,
        event: &CombatEventVisual,
        style: crate::player_presentation::PlayerStyle,
        owner: sim_types::PlayerId,
    ) {
        match event {
            CombatEventVisual::HitscanTracer { from, to, .. } => {
                self.hitscan_tracer(world_point(from), world_point(to), style, owner);
            }
            CombatEventVisual::ProjectileVisual {
                pos,
                vel,
                owner: _,
                duration_secs,
            } => {
                let lifetime = duration_secs.unwrap_or(0.35);
                self.projectile(world_point(pos), world_point(vel), lifetime, style, owner);
            }
            CombatEventVisual::BeamVisual {
                from,
                to,
                is_lightning,
                owner: _,
            } => {
                let (from, to) = (world_point(from), world_point(to));
                self.beam(from, to, *is_lightning, style, owner);
            }
            CombatEventVisual::MeleeSlashVisual {
                pos,
                angle_rad,
                radius,
                owner: _,
            } => self.melee_slash(pos, *angle_rad, *radius, style, owner),
            CombatEventVisual::BubbleShatter {
                pos,
                radius,
                is_dome: _,
                owner: _,
            } => spawn_death_shatter(
                self.commands,
                self.assets,
                self.materials,
                world_point(pos),
                *radius,
                style,
                owner,
            ),
        }
    }

    /// A 0.12 s supersonic tracer ribbon from `p1` to `p2` (world space).
    fn hitscan_tracer(
        &mut self,
        p1: Vec3,
        p2: Vec3,
        style: crate::player_presentation::PlayerStyle,
        owner: sim_types::PlayerId,
    ) {
        let Some(ribbon) = RibbonPlacement::between(p1, p2, self.camera_position) else {
            return;
        };
        let team_hdr = style.linear.truncate().extend(0.0);
        let mat = self.materials.add(CombatFxMaterial {
            uniform: CombatFxUniformGpu {
                primary_color: team_hdr,
                age: 0.0,
                fx_kind: 1, // Mode 1: Supersonic Hitscan / Railgun Tracer Bolt
                seed: (p1.x * 13.1 + p2.z * 19.7).fract().abs(),
                aspect: ribbon.length / 0.55,
            },
        });
        self.commands.spawn((
            CombatLightEffect,
            CombatEffectOwner(owner),
            Mesh3d(self.assets.tracer_mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(ribbon.center)
                .with_rotation(ribbon.rotation)
                .with_scale(Vec3::new(0.55, ribbon.length, 1.0)),
            BeamVisualComponent {
                from: p1,
                to: p2,
                is_lightning: false,
                lifetime: Timer::from_seconds(0.12, TimerMode::Once),
                material_handle: mat,
            },
        ));
    }

    /// A plasma projectile card at `p` flying at `v` (world units per second) for `lifetime`
    /// seconds.
    fn projectile(
        &mut self,
        p: Vec3,
        v: Vec3,
        lifetime: f32,
        style: crate::player_presentation::PlayerStyle,
        owner: sim_types::PlayerId,
    ) {
        let team_hdr = style.linear.truncate().extend(0.0);
        let mat = self.materials.add(CombatFxMaterial {
            uniform: CombatFxUniformGpu {
                primary_color: team_hdr,
                age: 0.0,
                fx_kind: 4, // Mode 4: Plasma projectile
                seed: (p.x * 29.3 + p.z * 41.7).fract().abs(),
                aspect: 1.0,
            },
        });
        let rot = if v.length_squared() > 0.01 {
            projectile_card_rotation(v, p, self.camera_position)
        } else {
            Quat::IDENTITY
        };
        self.commands.spawn((
            CombatLightEffect,
            CombatEffectOwner(owner),
            Mesh3d(self.assets.projectile_mesh.clone()),
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

    /// A continuous laser (0.25 s) or lightning arc (0.18 s) from `p1` to `p2` (world space).
    fn beam(
        &mut self,
        p1: Vec3,
        p2: Vec3,
        is_lightning: bool,
        style: crate::player_presentation::PlayerStyle,
        owner: sim_types::PlayerId,
    ) {
        let Some(ribbon) = RibbonPlacement::between(p1, p2, self.camera_position) else {
            return;
        };
        let team_hdr = style.linear.truncate().extend(0.0);
        let fx_kind = if is_lightning { 2 } else { 0 };
        let width = if is_lightning { 0.95 } else { 0.65 };
        let duration = if is_lightning { 0.18 } else { 0.25 };
        let mat = self.materials.add(CombatFxMaterial {
            uniform: CombatFxUniformGpu {
                primary_color: team_hdr,
                age: 0.0,
                fx_kind,
                seed: (p1.x * 37.7 + p2.z * 53.1).fract().abs(),
                aspect: ribbon.length / width,
            },
        });
        self.commands.spawn((
            CombatLightEffect,
            CombatEffectOwner(owner),
            Mesh3d(self.assets.beam_mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(ribbon.center)
                .with_rotation(ribbon.rotation)
                .with_scale(Vec3::new(width, ribbon.length, 1.0)),
            BeamVisualComponent {
                from: p1,
                to: p2,
                is_lightning,
                lifetime: Timer::from_seconds(duration, TimerMode::Once),
                material_handle: mat,
            },
        ));
    }

    /// A 0.20 s crescent slash at snapshot position `pos`, swept by `angle_rad` (radians), with
    /// `radius` in world units.
    fn melee_slash(
        &mut self,
        pos: &[f32; 3],
        angle_rad: f32,
        radius: f32,
        style: crate::player_presentation::PlayerStyle,
        owner: sim_types::PlayerId,
    ) {
        let p = Vec3::new(pos[0], pos[2] + 0.35, pos[1]);
        let mut team_hdr = style.linear.truncate().extend(0.0);
        team_hdr.w = angle_rad;
        let mat = self.materials.add(CombatFxMaterial {
            uniform: CombatFxUniformGpu {
                primary_color: team_hdr,
                age: 0.0,
                fx_kind: 3, // Mode 3: Sweeping Crescent Psi-Blade Melee Slash
                seed: (pos[0] * 19.3 + pos[1] * 23.7).fract().abs(),
                aspect: 1.0,
            },
        });
        // The shader sweeps the crescent by `primary_color.w`; rotating the card as well
        // would apply the angle twice and turn it edge-on at quarter turns.
        let rot = Quat::from_rotation_x(MELEE_CARD_TILT_RAD);
        self.commands.spawn((
            CombatLightEffect,
            CombatEffectOwner(owner),
            Mesh3d(self.assets.melee_slash_mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(p)
                .with_rotation(rot)
                .with_scale(Vec3::splat(radius * 2.6)),
            MeleeSlashComponent {
                lifetime: Timer::from_seconds(0.20, TimerMode::Once),
                radius,
                material_handle: mat,
            },
        ));
    }
}

/// Consumes and visualizes all combat events received from the authoritative simulation snapshot via procedural volumetric shader instances.
///
/// Each event's owner style is resolved against the live battle or the preview roles, and the
/// event is then spawned by its kind's [`CombatEffectSpawner`] method.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn process_combat_events_system(
    mut owners: crate::player_presentation::PresentationContext<'_>,
    lab: Res<'_, crate::LabState>,
    session: Res<'_, crate::battle_pipeline::session::BattleSession>,
    mut exit: EventWriter<'_, AppExit>,
    mut commands: Commands<'_, '_>,
    mut event_reader: EventReader<'_, '_, CombatVisualEvent>,
    vfx_assets: Res<'_, VfxSharedAssets>,
    camera_query: Query<'_, '_, &Transform, With<Camera3d>>,
    mut fx_materials: ResMut<'_, Assets<CombatFxMaterial>>,
) {
    let camera_position = camera_query
        .get_single()
        .map_or(Vec3::Y * 20.0, |t| t.translation);
    let mut spawner = CombatEffectSpawner {
        commands: &mut commands,
        assets: &vfx_assets,
        materials: &mut fx_materials,
        camera_position,
    };

    for event_wrapper in event_reader.read() {
        let owner = event_wrapper.0.owner();
        let resolved = if lab.current_tab == crate::LabTab::LiveBattle {
            owners.palette.battle_style(owner, &session.metadata)
        } else {
            owners.palette.preview_style(owner)
        };
        let style = match resolved {
            Ok(style) => style,
            Err(error) => {
                owners.reject(error, &mut exit);
                return;
            }
        };
        spawner.spawn(&event_wrapper.0, style, owner);
    }
}
