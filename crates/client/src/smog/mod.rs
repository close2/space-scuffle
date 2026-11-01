//! 3D Volumetric Smog Field and 10 Bouncing Bubbles Subsystem.
//!
//! # Architectural Charter
//! In accordance with the Stage 1 architectural blueprint and `AGENTS.md`, this module implements:
//! 1. **10 Bouncing Bubbles Kinematics:** 10 mobile units (`BouncingSmogBubble`) spawned dynamically from
//!    `CatalogResource` (with zero hardcoded creature slugs), executing elastic 3D wall and sphere-sphere bouncing.
//! 2. **Bubble-Driven 3D Eulerian Smog Grid:** $64 \times 24 \times 48 = 73,728$ voxels simulated with zero per-frame
//!    heap allocations. Every bubble is simultaneously a smog generator (injecting density and chromatic tint)
//!    and a moving obstacle/influencer (evacuating core density, compressing bow waves, injecting momentum drag,
//!    and shedding wake vortices).
//! 3. **3D Volumetric Raymarching Material:** Front-to-back Beer-Lambert raymarching shader sampling 3D density
//!    and velocity textures with sub-voxel analytical spherical cavity carving and bow-wave rim compression.

pub mod fluid;
pub mod kinematics;
pub mod material;

pub use fluid::{
    BubbleSmogSource, SmogGrid3d, SmogStepMetrics, GRID_NX, GRID_NY, GRID_NZ, SMOG_DENSITY_MAX,
    TOTAL_VOXELS,
};
pub use kinematics::{
    bubble_team_color, generate_bouncing_bubbles, initial_bouncing_bubbles,
    step_bouncing_kinematics, BouncingBubbleState, BouncingSmogBubble, SmogArenaBounds,
    BOUNCING_BUBBLE_COUNT,
};
pub use material::{
    SmogVolumeMarker, SmogVolumeMaterial, SmogVolumeUniformGpu, MAX_SMOG_SHADER_BUBBLES,
};

use crate::bubble_shader::{
    BubbleDomeMaterial, BubbleInstance, BubbleSharedMeshes, BubbleUniformGpu, FlipbookRegistry,
};
use crate::CatalogResource;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use sim_types::enums::ShapeKind;
use sim_types::snapshot::BubbleRenderInstance;
use sim_types::{EntityId, PlayerId};

/// Strongly-typed error conditions originating in the smog subsystem.
#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum SmogError {
    /// Arena bounds are invalid or non-finite.
    #[error("invalid arena bounds: min {min} >= max {max}")]
    InvalidBounds {
        /// Attempted minimum corner.
        min: Vec3,
        /// Attempted maximum corner.
        max: Vec3,
    },
    /// Simulation delta time is negative or non-finite.
    #[error("invalid simulation delta time: {0}")]
    InvalidDeltaTime(f32),
    /// Grid coordinate out of bounds.
    #[error("grid coordinate out of bounds: ({x}, {y}, {z})")]
    OutOfBounds {
        /// Discrete X index.
        x: usize,
        /// Discrete Y index.
        y: usize,
        /// Discrete Z index.
        z: usize,
    },
    /// Source configuration violated physical constraints.
    #[error("invalid source configuration: {0}")]
    InvalidSource(&'static str),
}

/// Runtime configuration parameters controlling 3D smog generation, dissipation, and bubble kinematics.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct SmogConfig {
    /// Global toggle enabling or disabling the 3D smog volume.
    pub enabled: bool,
    /// Toggle enabling or disabling the 10-bubble wall-bouncing kinematics update.
    pub bouncing_active: bool,
    /// 3D arena boundary box.
    pub bounds: SmogArenaBounds,
    /// Smog density emission rate multiplier.
    pub emission_rate: f32,
    /// Wake physical displacement and momentum drag strength.
    pub wake_influence: f32,
    /// Exponential density dissipation rate per second.
    pub dissipation_rate: f32,
    /// Vorticity confinement force multiplier.
    pub vorticity_confinement: f32,
    /// Upward thermal buoyancy velocity in world units per second.
    pub buoyancy: f32,
    /// Fluid velocity damping coefficient per second.
    pub velocity_damping: f32,
}

impl Default for SmogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bouncing_active: true,
            bounds: SmogArenaBounds::default(),
            emission_rate: 5.0,
            wake_influence: 3.4,
            dissipation_rate: 0.96,
            vorticity_confinement: 1.0,
            buoyancy: -0.18,
            velocity_damping: 0.40,
        }
    }
}

/// Live diagnostic telemetry and bounce event counters for the smog subsystem.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct SmogStats {
    /// Sum of all density values currently in the 3D grid.
    pub total_density: f32,
    /// Peak voxel density in the grid.
    pub peak_density: f32,
    /// Maximum fluid speed currently in the grid.
    pub max_speed: f32,
    /// Maximum fluid vorticity magnitude in the grid.
    pub max_vorticity: f32,
    /// Cumulative count of bubble-wall elastic reflections.
    pub wall_bounces: u64,
    /// Cumulative count of bubble-bubble elastic collisions.
    pub peer_bounces: u64,
    /// Current count of active emitting/influencing bubbles.
    pub active_bubbles: usize,
    /// Total simulation steps executed.
    pub step_count: u64,
}

/// ECS resource retaining persistent handles to dynamic 3D textures and materials.
#[derive(Resource, Clone)]
pub struct SmogTextures {
    /// Handle to the 3D RGBA8 density + chromatic tint texture.
    pub density_image: Handle<Image>,
    /// Handle to the 3D RGBA8 velocity + vorticity texture.
    pub velocity_image: Handle<Image>,
    /// Handle to the tileable 32x32x32 3D wispy noise texture.
    pub noise_image: Handle<Image>,
    /// Handle to the 3D volumetric material instance.
    pub material_handle: Handle<SmogVolumeMaterial>,
}

/// Bevy plugin managing 3D smog fluid simulation, material pipelines, and 10 bouncing bubbles.
pub struct SmogPlugin;

impl Plugin for SmogPlugin {
    fn build(&self, app: &mut App) {
        #[allow(clippy::expect_used)]
        let grid = SmogGrid3d::new(Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 2.8, 4.5))
            .expect("invariant: hardcoded default smog grid bounds -6.5..6.5, 0.0..2.8, -4.5..4.5 are strictly positive and finite");

        app.init_resource::<SmogConfig>()
            .init_resource::<SmogStats>()
            .insert_resource(grid)
            .add_systems(
                Startup,
                setup_smog_volume_system.after(crate::bubble_shader::setup_bubble_assets_system),
            )
            .add_systems(
                Update,
                (sync_bouncing_smog_bubbles_system, update_smog_field_system).chain(),
            );
    }
}

/// Startup system initializing the 3D textures, the volumetric bounding proxy entity,
/// running 45 pre-warm steps, and dynamically spawning the 10 mobile bouncing bubbles from the unit catalog.
// PERFORMANCE RATIONALE:
// Why: 3D textures are created once at startup and mutated in-place every frame via `copy_from_slice`.
// What: Avoids destroying and reallocating GPU textures and bind groups per frame.
pub fn setup_smog_volume_system(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut images: ResMut<'_, Assets<Image>>,
    mut materials: ResMut<'_, Assets<SmogVolumeMaterial>>,
    mut bubble_materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    mut grid: ResMut<'_, SmogGrid3d>,
    config: Res<'_, SmogConfig>,
    catalog: Option<Res<'_, CatalogResource>>,
    registry: Option<Res<'_, FlipbookRegistry>>,
    media_status: Option<Res<'_, crate::media::ClientMediaStatus>>,
    _palette: Option<Res<'_, crate::player_presentation::PlayerPalette>>,
    shared_meshes: Option<Res<'_, BubbleSharedMeshes>>,
) {
    // 1. Initialize kinematics initial states and run 26-step warmup
    let mut initial_states = match initial_bouncing_bubbles(&config.bounds) {
        Ok(states) => states,
        Err(err) => {
            warn!("Failed to initialize bouncing bubble states: {err}");
            return;
        }
    };

    for _ in 0..26 {
        let _ = step_bouncing_kinematics(&mut initial_states, &config.bounds, 1.0 / 30.0);
        let mut prewarm_sources = [BubbleSmogSource::default(); BOUNCING_BUBBLE_COUNT];
        for (j, state) in initial_states.iter().enumerate() {
            let team_color = bubble_team_color(j);
            prewarm_sources[j] = BubbleSmogSource {
                position: state.position,
                velocity: state.velocity,
                radius: state.radius,
                team_color,
                emission_strength: 1.0,
                influence_strength: 1.0,
            };
        }
        let _ = grid.step(&prewarm_sources, &config, 1.0 / 30.0);
    }

    // 2. Initialize persistent 3D textures with pre-warmed data
    let make_3d_image = |data: Vec<u8>| {
        let size = Extent3d {
            width: GRID_NX as u32,
            height: GRID_NY as u32,
            depth_or_array_layers: GRID_NZ as u32,
        };
        let mut image = Image::new(
            size,
            TextureDimension::D3,
            data,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler =
            bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
                address_mode_u: bevy::image::ImageAddressMode::ClampToEdge,
                address_mode_v: bevy::image::ImageAddressMode::ClampToEdge,
                address_mode_w: bevy::image::ImageAddressMode::ClampToEdge,
                mag_filter: bevy::image::ImageFilterMode::Linear,
                min_filter: bevy::image::ImageFilterMode::Linear,
                mipmap_filter: bevy::image::ImageFilterMode::Linear,
                ..default()
            });
        image
    };

    let density_image = images.add(make_3d_image(grid.rgba_density_bytes.clone()));
    let velocity_image = images.add(make_3d_image(grid.rgba_velocity_bytes.clone()));

    // 3. Generate pre-baked tileable 32x32x32 3D wispy noise texture
    let make_3d_noise_image = || {
        const NOISE_DIM: usize = 32;
        let mut noise_data = Vec::with_capacity(NOISE_DIM * NOISE_DIM * NOISE_DIM * 4);
        for z in 0..NOISE_DIM {
            let wz = (z as f32) * (std::f32::consts::TAU / NOISE_DIM as f32);
            for y in 0..NOISE_DIM {
                let wy = (y as f32) * (std::f32::consts::TAU / NOISE_DIM as f32);
                for x in 0..NOISE_DIM {
                    let wx = (x as f32) * (std::f32::consts::TAU / NOISE_DIM as f32);

                    let s1 =
                        (wx + wy.cos()).sin() * (wy + wz.cos()).cos() + (wz + wx.cos()).sin() * 0.5;
                    let s2 = (2.0 * wx - wy + wz.sin()).sin() * (2.0 * wz - wx).cos();
                    let s3 = (3.0 * wy + 2.0 * wz - wx).sin() * (2.0 * wx + 2.0 * wy).cos();
                    let ridge1 = (1.0 - (s1 * 0.60).abs()).clamp(0.0, 1.0).powf(1.25);
                    let ridge2 = (1.0 - (s2 * 0.75).abs()).clamp(0.0, 1.0).powf(1.35);
                    let ridge3 = (1.0 - (s3 * 0.80).abs()).clamp(0.0, 1.0).powf(1.40);
                    let r = (0.50 * ridge1 + 0.32 * ridge2 + 0.18 * ridge3).clamp(0.0, 1.0);

                    let g1 = (wx - wz).cos() * (2.0 * wy + wx).sin();
                    let g2 = (2.0 * wz - wy).sin() * (2.0 * wx + wz).cos();
                    let g = (0.60 * (1.0 - (g1 * 0.75).abs()).clamp(0.0, 1.0).powf(1.30)
                        + 0.40 * (1.0 - (g2 * 0.75).abs()).clamp(0.0, 1.0).powf(1.40))
                    .clamp(0.0, 1.0);

                    let b = (0.5 + 0.45 * ((wy - 2.0 * wz).sin() * (2.0 * wx + wy).cos()))
                        .clamp(0.0, 1.0);
                    let a = (0.5 + 0.45 * ((2.0 * wz - wx).cos() * (wy + 2.0 * wz).sin()))
                        .clamp(0.0, 1.0);

                    noise_data.push((r * 255.0) as u8);
                    noise_data.push((g * 255.0) as u8);
                    noise_data.push((b * 255.0) as u8);
                    noise_data.push((a * 255.0) as u8);
                }
            }
        }

        let size = Extent3d {
            width: NOISE_DIM as u32,
            height: NOISE_DIM as u32,
            depth_or_array_layers: NOISE_DIM as u32,
        };
        let mut image = Image::new(
            size,
            TextureDimension::D3,
            noise_data,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler =
            bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
                address_mode_u: bevy::image::ImageAddressMode::Repeat,
                address_mode_v: bevy::image::ImageAddressMode::Repeat,
                address_mode_w: bevy::image::ImageAddressMode::Repeat,
                mag_filter: bevy::image::ImageFilterMode::Linear,
                min_filter: bevy::image::ImageFilterMode::Linear,
                mipmap_filter: bevy::image::ImageFilterMode::Linear,
                ..default()
            });
        image
    };

    let noise_image = images.add(make_3d_noise_image());

    // 4. Initialize SmogVolumeMaterial and spawn volume bounding box entity
    let volume_min = Vec4::new(-6.5, 0.0, -4.5, 0.4);
    let volume_max = Vec4::new(6.5, 2.8, 4.5, 1.15);
    let extent = Vec3::new(
        volume_max.x - volume_min.x,
        volume_max.y - volume_min.y,
        volume_max.z - volume_min.z,
    );
    let center = Vec3::new(
        (volume_min.x + volume_max.x) * 0.5,
        (volume_min.y + volume_max.y) * 0.5,
        (volume_min.z + volume_max.z) * 0.5,
    );

    let mut packed_pos_radius = [Vec4::ZERO; MAX_SMOG_SHADER_BUBBLES];
    let mut packed_vel_boost = [Vec4::ZERO; MAX_SMOG_SHADER_BUBBLES];
    for (idx, state) in initial_states
        .iter()
        .enumerate()
        .take(MAX_SMOG_SHADER_BUBBLES)
    {
        packed_pos_radius[idx] = Vec4::new(
            state.position.x,
            state.position.y,
            state.position.z,
            state.radius,
        );
        packed_vel_boost[idx] =
            Vec4::new(state.velocity.x, state.velocity.y, state.velocity.z, 1.0);
    }

    let uniform = SmogVolumeUniformGpu {
        volume_min,
        volume_max,
        base_tint_time: Vec4::new(0.28, 0.46, 0.68, 1.5),
        warm_tint_anisotropy: Vec4::new(1.0, 0.82, 0.52, 0.45),
        light_dir_intensity: Vec4::new(0.45, 0.75, 0.48, 1.85),
        control_params: Vec4::new(BOUNCING_BUBBLE_COUNT as f32, 1.85, 1.0, 1.0),
        bubble_pos_radius: packed_pos_radius,
        bubble_vel_boost: packed_vel_boost,
    };

    let mat_handle = materials.add(SmogVolumeMaterial {
        uniform,
        density_texture: density_image.clone(),
        velocity_texture: velocity_image.clone(),
        noise_texture: noise_image.clone(),
    });

    let box_mesh = meshes.add(Cuboid::new(extent.x, extent.y, extent.z));
    commands.spawn((
        Mesh3d(box_mesh),
        MeshMaterial3d(mat_handle.clone()),
        Transform::from_translation(center),
        Visibility::default(),
        SmogVolumeMarker,
    ));

    commands.insert_resource(SmogTextures {
        density_image,
        velocity_image,
        noise_image,
        material_handle: mat_handle,
    });

    // 4. Dynamically spawn the 10 Bouncing Bubbles from CatalogResource (Zero hardcoded slugs)
    let sphere_mesh = shared_meshes.as_ref().map_or_else(
        || meshes.add(Sphere::new(1.0).mesh().uv(32, 18)),
        |sm| sm.sphere_mesh.clone(),
    );

    let mobile_archetypes: Vec<_> = if let Some(cat) = &catalog {
        let admitted: Vec<_> = cat
            .archetypes()
            .filter(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }))
            .filter(|a| {
                registry
                    .as_ref()
                    .map_or(true, |r| r.get_texture(a.unit_type_id).is_some())
                    && media_status
                        .as_ref()
                        .and_then(|ms| ms.audit(a.unit_type_id))
                        .map_or(true, |audit| {
                            audit.status
                                != asset_baker::media_audit::MediaAuditStatus::KnownConceptDuplicate
                        })
            })
            .cloned()
            .collect();
        if admitted.is_empty() {
            cat.archetypes()
                .filter(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }))
                .cloned()
                .collect()
        } else {
            admitted
        }
    } else {
        Vec::new()
    };

    let fallback_texture = images.add(Image::new_fill(
        Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[255, 255, 255, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    ));

    for (i, state) in initial_states.iter().enumerate() {
        let owner = PlayerId((i % 2) as u8 + 1);

        let (unit_type_id, texture, meta) = if !mobile_archetypes.is_empty() {
            let arch = &mobile_archetypes[i % mobile_archetypes.len()];
            let tex = registry
                .as_ref()
                .and_then(|r| r.get_texture(arch.unit_type_id))
                .or_else(|| {
                    registry
                        .as_ref()
                        .and_then(|r| r.get_texture_by_name("concept_video"))
                })
                .or_else(|| {
                    registry
                        .as_ref()
                        .and_then(|r| r.textures_by_id.values().next().cloned())
                })
                .unwrap_or_else(|| fallback_texture.clone());
            let m = registry
                .as_ref()
                .and_then(|r| r.get_meta(arch.unit_type_id));
            (arch.unit_type_id, Some(tex), m)
        } else {
            (
                sim_types::ids::UnitTypeId(100 + i as u16),
                Some(fallback_texture.clone()),
                None,
            )
        };

        let tc = bubble_team_color(i);
        let linear_tc = Color::srgb(tc.x, tc.y, tc.z).to_linear();
        let style = crate::player_presentation::PlayerStyle {
            srgb: [tc.x, tc.y, tc.z],
            linear: Vec4::new(linear_tc.red, linear_tc.green, linear_tc.blue, 1.0),
            ui: bevy_egui::egui::Color32::from_rgb(
                (tc.x * 255.0) as u8,
                (tc.y * 255.0) as u8,
                (tc.z * 255.0) as u8,
            ),
        };

        let render_instance = BubbleRenderInstance {
            entity_id: EntityId(20_000 + i as u32),
            shape: ShapeKind::UnitSphere,
            silhouette: sim_types::enums::SilhouetteClass::LightProngs,
            unit_type: unit_type_id,
            owner,
            world_pos: [state.position.x, state.position.z, state.position.y],
            ground_z: 0.0,
            radius: state.radius,
            velocity: [state.velocity.x, state.velocity.z],
            heading_angle_rad: state.heading_angle_rad,
            aim_angle_rad: state.heading_angle_rad,
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
        };

        let bubble_uniform = BubbleUniformGpu::from_instance(&render_instance, meta, style);
        let bubble_mat = bubble_materials.add(BubbleDomeMaterial {
            uniform: bubble_uniform,
            video_texture: texture,
            alpha_mode: AlphaMode::Blend,
        });

        commands.spawn((
            BouncingSmogBubble {
                index: i,
                velocity: state.velocity,
                radius: state.radius,
                wall_bounces: state.wall_bounces,
                peer_bounces: state.peer_bounces,
            },
            crate::mist::MistMotion {
                conveyor_velocity: None,
                enabled: false,
            },
            BubbleInstance(render_instance),
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(bubble_mat),
            Transform::from_translation(state.position).with_scale(Vec3::splat(state.radius)),
            Visibility::default(),
        ));
    }
}

/// Per-frame kinematics system updating the 10 bouncing bubbles with elastic reflections and peer collisions.
// PERFORMANCE RATIONALE:
// Why: Allocating a `Vec<BouncingBubbleState>` on every frame causes unnecessary heap churn.
// What: We maintain a pre-allocated `Local<Vec<BouncingBubbleState>>` scratch vector.
// Trade-off: Zero per-frame allocations during steady-state simulation.
pub fn sync_bouncing_smog_bubbles_system(
    time: Res<'_, Time>,
    config: Res<'_, SmogConfig>,
    lab_state: Option<Res<'_, crate::LabState>>,
    mut stats: ResMut<'_, SmogStats>,
    mut scratch: Local<'_, Vec<BouncingBubbleState>>,
    mut query: Query<
        '_,
        '_,
        (
            &mut BouncingSmogBubble,
            &mut Transform,
            &mut BubbleInstance,
            &mut Visibility,
        ),
    >,
) {
    let in_active_tab = lab_state
        .as_ref()
        .map_or(true, |l| l.current_tab == crate::LabTab::VisualLab);
    let bouncing_active = lab_state.as_ref().map_or(config.bouncing_active, |l| {
        l.smog_bouncing_active && l.opt_smog_volume
    });

    let is_visible = in_active_tab && bouncing_active;

    for (_, _, _, mut vis) in &mut query {
        let target_vis = if is_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target_vis {
            *vis = target_vis;
        }
    }

    if !is_visible {
        return;
    }

    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    // Harvest current states into pre-allocated scratch
    scratch.clear();
    for (bubble, transform, _, _) in query.iter() {
        scratch.push(BouncingBubbleState {
            index: bubble.index,
            position: transform.translation,
            velocity: bubble.velocity,
            radius: bubble.radius,
            heading_angle_rad: transform.rotation.to_euler(EulerRot::YXZ).0,
            wall_bounces: bubble.wall_bounces,
            peer_bounces: bubble.peer_bounces,
        });
    }

    if scratch.is_empty() {
        return;
    }

    // Step kinematics
    let (wb, pb) = match step_bouncing_kinematics(&mut scratch, &config.bounds, dt) {
        Ok(counts) => counts,
        Err(err) => {
            warn!("step_bouncing_kinematics failed: {err}");
            return;
        }
    };

    stats.wall_bounces += wb;
    stats.peer_bounces += pb;

    // Write back updated kinematics to ECS components
    // PERFORMANCE RATIONALE:
    // Why: scratch.iter().find(|s| s.index == bubble.index) was an O(N^2) linear scan across N bubbles every frame.
    // What: We use direct scratch[bubble.index] O(1) array indexing with bounds check and index-matching fallback.
    // Trade-off: None; strictly O(1) per bubble entity.
    for (mut bubble, mut transform, mut instance, _) in &mut query {
        let state_opt =
            if bubble.index < scratch.len() && scratch[bubble.index].index == bubble.index {
                Some(&scratch[bubble.index])
            } else {
                scratch.iter().find(|s| s.index == bubble.index)
            };

        if let Some(state) = state_opt {
            transform.translation = state.position;
            transform.rotation = Quat::from_rotation_y(-state.heading_angle_rad);
            bubble.velocity = state.velocity;
            bubble.wall_bounces = state.wall_bounces;
            bubble.peer_bounces = state.peer_bounces;

            instance.0.world_pos = [state.position.x, state.position.z, state.position.y];
            instance.0.velocity = [state.velocity.x, state.velocity.z];
            instance.0.heading_angle_rad = state.heading_angle_rad;
        }
    }
}

/// Per-frame fluid field update system: collects sources, steps `SmogGrid3d`, updates 3D textures,
/// and packs bubble spheres into `SmogVolumeMaterial.uniform`.
// PERFORMANCE RATIONALE:
// Why: Re-allocating source lists per frame creates garbage.
// What: We reuse a `Local<Vec<BubbleSmogSource>>` scratch buffer, achieving zero allocations per frame.
#[allow(clippy::type_complexity)]
pub(crate) fn update_smog_field_system(
    time: Res<'_, Time>,
    config: Res<'_, SmogConfig>,
    mut lab_state: Option<ResMut<'_, crate::LabState>>,
    mut stats: ResMut<'_, SmogStats>,
    mut grid: ResMut<'_, SmogGrid3d>,
    textures: Option<Res<'_, SmogTextures>>,
    mut images: ResMut<'_, Assets<Image>>,
    mut volume_materials: ResMut<'_, Assets<SmogVolumeMaterial>>,
    mut sources_scratch: Local<'_, Vec<BubbleSmogSource>>,
    mut volume_query: Query<
        '_,
        '_,
        (&mut Visibility, &mut Transform),
        (With<SmogVolumeMarker>, Without<BubbleInstance>),
    >,
    bubble_query: Query<
        '_,
        '_,
        (
            &Transform,
            &BubbleInstance,
            Option<&BouncingSmogBubble>,
            Option<&crate::scene_controller::LabPreviewBubble>,
            Option<&crate::mist::MistMotion>,
            Option<&Visibility>,
        ),
        Without<SmogVolumeMarker>,
    >,
    camera_query: Query<'_, '_, &crate::camera::RtsCamera>,
    controlled_fixture: Option<Res<'_, crate::shell_order_audit::fixture::ControlledFixture>>,
) {
    let in_controlled_fixture = controlled_fixture.is_some();
    let enabled = !in_controlled_fixture
        && lab_state
            .as_ref()
            .map_or(config.enabled, |l| l.opt_smog_volume && l.opt_mist_wake);

    let current_tab = lab_state
        .as_ref()
        .map_or(crate::LabTab::VisualLab, |l| l.current_tab);
    let (target_min, target_max) = match current_tab {
        crate::LabTab::VisualLab => (Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 2.8, 4.5)),
        crate::LabTab::SwarmStressTest => (Vec3::new(4.0, 0.0, 7.0), Vec3::new(28.0, 2.8, 24.0)),
        crate::LabTab::LiveBattle => {
            const LIVE_BATTLE_EXTENT_Y: f32 = 4.8;
            let (cam_x, cam_z, cam_ground_y) = if let Some(cam) = camera_query.iter().next() {
                (cam.target.x, cam.target.y, (cam.look_at_y - 1.2).max(0.0))
            } else {
                (16.0, 32.0, 0.0)
            };

            let clamped_x = cam_x.clamp(14.5, 49.5);
            let clamped_z = cam_z.clamp(11.5, 52.5);

            // Find the lowest ground_z of visible mobile units inside the horizontal camera window
            // (treating 0.0 low ground as a valid first-class altitude).
            let mut min_mobile_ground_y: Option<f32> = None;
            for (transform, instance, bouncing, preview, _, vis) in &bubble_query {
                if matches!(vis, Some(Visibility::Hidden))
                    || bouncing.is_some()
                    || preview.is_some()
                    || instance.0.shape != ShapeKind::UnitSphere
                {
                    continue;
                }
                let p = transform.translation;
                if (p.x - clamped_x).abs() <= 15.5 && (p.z - clamped_z).abs() <= 12.5 {
                    let gz = instance.0.ground_z.max(0.0);
                    min_mobile_ground_y = Some(min_mobile_ground_y.map_or(gz, |cur| cur.min(gz)));
                }
            }

            let ground_y = min_mobile_ground_y.unwrap_or(cam_ground_y).max(0.0);

            let step_x = 28.0 / GRID_NX as f32;
            let step_y = LIVE_BATTLE_EXTENT_Y / GRID_NY as f32;
            let step_z = 22.0 / GRID_NZ as f32;

            let center_x = (clamped_x / step_x).round() * step_x;
            let base_y = (ground_y / step_y).floor() * step_y;
            let center_z = (clamped_z / step_z).round() * step_z;

            (
                Vec3::new(center_x - 14.0, base_y, center_z - 11.0),
                Vec3::new(
                    center_x + 14.0,
                    base_y + LIVE_BATTLE_EXTENT_Y,
                    center_z + 11.0,
                ),
            )
        }
    };
    grid.set_bounds_quantized(target_min, target_max);

    let actual_min = grid.bounds_min();
    let actual_max = grid.bounds_max();
    let actual_extent = actual_max - actual_min;
    let actual_center = (actual_min + actual_max) * 0.5;
    for (mut vis, mut transform) in &mut volume_query {
        let target_vis = if enabled {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target_vis {
            *vis = target_vis;
        }
        transform.translation = actual_center;
        transform.scale = Vec3::new(
            actual_extent.x / 13.0,
            actual_extent.y / 2.8,
            actual_extent.z / 9.0,
        );
    }

    if !enabled {
        return;
    }

    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let mut effective_config = config.clone();
    if let Some(ref l) = lab_state {
        effective_config.emission_rate = l.smog_emission_rate;
        effective_config.wake_influence = l.smog_wake_influence;
    }

    sources_scratch.clear();
    let mut packed_pos_radius = [Vec4::ZERO; MAX_SMOG_SHADER_BUBBLES];
    let mut packed_vel_boost = [Vec4::ZERO; MAX_SMOG_SHADER_BUBBLES];
    let mut packed_count = 0usize;

    for (transform, instance, bouncing, preview, mist_motion, visibility) in &bubble_query {
        if matches!(visibility, Some(Visibility::Hidden)) {
            continue;
        }

        let is_live_battle_unit =
            bouncing.is_none() && preview.is_none() && current_tab == crate::LabTab::LiveBattle;
        let is_swarm_unit = bouncing.is_none()
            && preview.is_none()
            && current_tab == crate::LabTab::SwarmStressTest;

        // In LiveBattle, only include mobile unit spheres (skip static building domes)
        if is_live_battle_unit && instance.0.shape != ShapeKind::UnitSphere {
            continue;
        }

        let mut pos = transform.translation;
        let mut vel = bouncing.map_or_else(
            || {
                if preview.is_some() {
                    mist_motion
                        .and_then(|m| m.conveyor_velocity)
                        .unwrap_or_else(|| {
                            let spd = lab_state.as_ref().map_or(0.0, |l| l.speed);
                            Vec3::new(-spd, 0.0, 0.0)
                        })
                } else {
                    Vec3::new(instance.0.velocity[0], 0.0, instance.0.velocity[1])
                }
            },
            |b| b.velocity,
        );

        if is_live_battle_unit {
            // Scale velocity from sim displacement per tick to physical m/s
            const SIM_VELOCITY_SCALE: f32 = 20.0;
            vel *= SIM_VELOCITY_SCALE;

            // Anchor vertical source position to the unit's true world-space terrain level
            pos.y = instance.0.ground_z + (pos.y - instance.0.ground_z).clamp(0.35, 1.8);

            // Filter out units outside the active camera volume
            if pos.x < actual_min.x - 1.5
                || pos.x > actual_max.x + 1.5
                || pos.y < actual_min.y - 0.5
                || pos.y > actual_max.y + 0.5
                || pos.z < actual_min.z - 1.5
                || pos.z > actual_max.z + 1.5
            {
                continue;
            }
        } else if is_swarm_unit {
            // Swarm bubbles move at 1.5 m/s horizontally; scale velocity for rich wake trails
            vel *= 1.25;

            // Filter out units far outside the active swarm volume horizontally
            if pos.x < actual_min.x - 1.5
                || pos.x > actual_max.x + 1.5
                || pos.z < actual_min.z - 1.5
                || pos.z > actual_max.z + 1.5
            {
                continue;
            }
        }

        let radius = instance.0.radius;
        let bubble_idx = bouncing.map_or(instance.0.entity_id.0 as usize, |b| b.index);
        let team_color = bubble_team_color(bubble_idx);
        let (emission_strength, influence_strength) = (1.0_f32, 1.0_f32);

        sources_scratch.push(BubbleSmogSource {
            position: pos,
            velocity: vel,
            radius,
            team_color,
            emission_strength,
            influence_strength,
        });

        if packed_count < MAX_SMOG_SHADER_BUBBLES {
            packed_pos_radius[packed_count] = Vec4::new(pos.x, pos.y, pos.z, radius);
            let speed = vel.length();
            let wake_boost = 1.0 + (speed * 0.35).min(2.0);
            packed_vel_boost[packed_count] = Vec4::new(vel.x, vel.y, vel.z, wake_boost);
            packed_count += 1;
        }
    }

    // First-frame trajectory pre-warm for LiveBattle and SwarmStressTest:
    // If mist is empty and units are moving, run 15 historical sub-steps backwards along velocity
    // to establish clean wake slipstreams immediately on transition.
    if matches!(
        current_tab,
        crate::LabTab::LiveBattle | crate::LabTab::SwarmStressTest
    ) && stats.total_density < 0.5
        && sources_scratch.iter().any(|s| s.velocity.length() > 0.2)
    {
        let mut prewarm_sources = sources_scratch.clone();
        let prewarm_dt = 1.0 / 30.0;
        for step in (1..=15).rev() {
            let t_back = step as f32 * prewarm_dt;
            for (i, src) in sources_scratch.iter().enumerate() {
                prewarm_sources[i].position = src.position - src.velocity * t_back;
            }
            let _ = grid.step(&prewarm_sources, &effective_config, prewarm_dt);
        }
    }

    // Step 3D fluid simulation
    let metrics = match grid.step(&sources_scratch, &effective_config, dt) {
        Ok(m) => m,
        Err(err) => {
            warn!("SmogGrid3d::step failed: {err}");
            return;
        }
    };

    if let Some(ref mut l) = lab_state {
        l.smog_bubble_count = sources_scratch.len();
    }

    stats.total_density = metrics.total_density;
    stats.peak_density = metrics.peak_density;
    stats.max_speed = metrics.max_speed;
    stats.max_vorticity = metrics.max_vorticity;
    stats.active_bubbles = sources_scratch.len();
    stats.step_count += 1;

    // Update 3D GPU textures via in-place byte copy
    if let Some(tex) = &textures {
        if let Some(density_img) = images.get_mut(&tex.density_image) {
            density_img.data.copy_from_slice(&grid.rgba_density_bytes);
        }
        if let Some(vel_img) = images.get_mut(&tex.velocity_image) {
            vel_img.data.copy_from_slice(&grid.rgba_velocity_bytes);
        }

        // Update GPU Uniforms
        if let Some(material) = volume_materials.get_mut(&tex.material_handle) {
            material.uniform.volume_min = Vec4::new(actual_min.x, actual_min.y, actual_min.z, 0.4);
            material.uniform.volume_max = Vec4::new(actual_max.x, actual_max.y, actual_max.z, 1.15);
            material.uniform.base_tint_time.w += dt;
            material.uniform.control_params.x = packed_count as f32;
            material.uniform.control_params.w = if enabled { 1.0 } else { 0.0 };
            material.uniform.bubble_pos_radius = packed_pos_radius;
            material.uniform.bubble_vel_boost = packed_vel_boost;
        }
    }
}
