//! 3D Volumetric Smog Field and 10 Bouncing Bubbles Subsystem.
//!
//! # Architectural Charter
//! In accordance with the Stage 1 architectural blueprint and `AGENTS.md`, this module implements:
//! 1. **10 Bouncing Bubbles Kinematics:** 10 mobile units (`BouncingSmogBubble`) spawned dynamically from
//!    `CatalogResource` (with zero hardcoded creature slugs), executing elastic 3D wall and sphere-sphere bouncing.
//! 2. **Bubble-Driven 3D Eulerian Smog Grid:** `$64 \times 24 \times 48 = 73,728$` voxels simulated with zero per-frame
//!    heap allocations. Every bubble is simultaneously a smog generator (injecting density and chromatic tint)
//!    and a moving obstacle/influencer (evacuating core density, compressing bow waves, injecting momentum drag,
//!    and shedding wake vortices).
//! 3. **3D Volumetric Raymarching Material:** Front-to-back Beer-Lambert raymarching shader sampling 3D density
//!    and velocity textures with sub-voxel analytical spherical cavity carving and bow-wave rim compression.
//! 4. **Persistent GPU Upload:** [`gpu_upload`] writes each step into textures and a uniform buffer that live
//!    for the whole run. After startup the smog assets are never modified.
//!
//! # Failure Policy
//! Startup and step failures are typed [`SmogError`] values reported through the media failure path.
//! Nothing here substitutes another unit's media, a placeholder texture or an invented identity.

mod bricks;
pub mod fluid;
pub mod gpu_upload;
pub mod kinematics;
pub mod material;
pub mod math;

pub use fluid::{
    BubbleSmogSource, SmogGrid3d, SmogStepMetrics, GRID_NX, GRID_NY, GRID_NZ, SMOG_DENSITY_MAX,
    TOTAL_VOXELS,
};
pub use gpu_upload::{SmogGpuFrame, SMOG_UPLOAD_LOG_TARGET};
pub use kinematics::{
    bubble_team_color, generate_bouncing_bubbles, initial_bouncing_bubbles,
    step_bouncing_kinematics, BouncingBubbleState, BouncingSmogBubble, SmogArenaBounds,
    BOUNCING_BUBBLE_COUNT, MAX_BOUNCING_BUBBLES,
};
pub use material::{
    CavitySlots, SmogVolumeMarker, SmogVolumeMaterial, SmogVolumeUniformGpu,
    MAX_SMOG_SHADER_BUBBLES,
};

use crate::battle_pipeline::clock::SIM_TICK_PERIOD;
use crate::battle_pipeline::BattleSet;
use crate::bubble_shader::{
    BubbleDomeMaterial, BubbleInstance, BubbleSharedMeshes, BubbleUniformGpu, FlipbookRegistry,
};
use crate::scene_controller::{SWARM_MAX_X, SWARM_MAX_Z, SWARM_MIN_X, SWARM_MIN_Z};
use crate::CatalogResource;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::{ExtractSchedule, RenderApp};
use sim_types::enums::{AnimState, ShapeKind};
use sim_types::ids::UnitTypeId;
use sim_types::snapshot::BubbleRenderInstance;
use sim_types::EntityId;

/// Minimum corner of the Visual Lab smog volume in Bevy world units.
pub const LAB_VOLUME_MIN: Vec3 = Vec3::new(-6.5, 0.0, -4.5);

/// Maximum corner of the Visual Lab smog volume in Bevy world units.
pub const LAB_VOLUME_MAX: Vec3 = Vec3::new(6.5, 2.8, 4.5);

/// Gap between the swarm's bounce range and the smog volume walls, in world units.
const SWARM_VOLUME_MARGIN: f32 = 2.0;

/// Half of the live battle smog window along world X, in world units.
const LIVE_BATTLE_HALF_EXTENT_X: f32 = 14.0;

/// Half of the live battle smog window along world Z, in world units.
const LIVE_BATTLE_HALF_EXTENT_Z: f32 = 11.0;

/// Height of the live battle smog window, in world units. Spans both cliff levels.
const LIVE_BATTLE_EXTENT_Y: f32 = 4.8;

/// Gap kept between the live battle smog window and the map edge, in world units.
const LIVE_BATTLE_MAP_MARGIN: f32 = 0.5;

/// Horizontal centre of the live battle smog window for a camera focus.
///
/// `focus` and the result are `(world x, world z)`. `map_size` is the map's `(width, height)` in
/// world units. The window stays on the map, and centres on an axis too short to contain it.
#[must_use]
pub fn live_battle_window_center(focus: Vec2, map_size: Vec2) -> Vec2 {
    let inset =
        Vec2::new(LIVE_BATTLE_HALF_EXTENT_X, LIVE_BATTLE_HALF_EXTENT_Z) + LIVE_BATTLE_MAP_MARGIN;
    let axis = |focus: f32, size: f32, inset: f32| {
        if size >= inset * 2.0 {
            focus.clamp(inset, size - inset)
        } else {
            size * 0.5
        }
    };
    Vec2::new(
        axis(focus.x, map_size.x, inset.x),
        axis(focus.y, map_size.y, inset.y),
    )
}

/// First [`EntityId`] of the showcase bubbles, which exist only in the client.
///
/// [`BubbleRenderInstance`] requires an identity, but these actors have no simulation entity. The
/// range sits at the top of `u32`, far beyond the upward-counting simulation allocator, so a
/// showcase identity can never alias a battle entity.
#[expect(
    clippy::cast_possible_truncation,
    reason = "fluid grid indices and counts stay far below the range of f32 and i32"
)]
pub const SHOWCASE_ENTITY_ID_BASE: u32 = u32::MAX - MAX_BOUNCING_BUBBLES as u32;

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
    /// No catalog identity has the media a smog actor must present.
    #[error("no admitted mobile unit has its own media for the smog showcase")]
    NoAdmittedActor,
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
        #[expect(clippy::expect_used, reason = "the message states the invariant")]
        let grid = SmogGrid3d::new(LAB_VOLUME_MIN, LAB_VOLUME_MAX)
            .expect("invariant: LAB_VOLUME_MIN is below LAB_VOLUME_MAX by over 1.0 on each axis");

        app.init_resource::<SmogConfig>()
            .init_resource::<SmogStats>()
            .init_resource::<SmogGpuFrame>()
            .insert_resource(grid)
            .add_systems(
                Startup,
                setup_smog_volume_system.after(crate::bubble_shader::setup_bubble_assets_system),
            )
            .add_systems(
                Update,
                (sync_bouncing_smog_bubbles_system, update_smog_field_system)
                    .chain()
                    .in_set(BattleSet::Present)
                    .after(crate::camera::camera_control_system),
            );
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(ExtractSchedule, gpu_upload::upload_smog_frame_system);
        }
    }
}

/// Startup system initializing the 3D textures, the volumetric bounding proxy entity,
/// running the pre-warm steps, and spawning the 10 mobile bouncing bubbles from admitted catalog identities.
///
/// Failures stop before any asset or entity is created, and reach the visible media diagnostic.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn setup_smog_volume_system(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut images: ResMut<'_, Assets<Image>>,
    mut materials: ResMut<'_, Assets<SmogVolumeMaterial>>,
    mut bubble_materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    mut grid: ResMut<'_, SmogGrid3d>,
    config: Res<'_, SmogConfig>,
    catalog: Res<'_, CatalogResource>,
    registry: Res<'_, FlipbookRegistry>,
    mut media_status: ResMut<'_, crate::media::ClientMediaStatus>,
    mut owners: crate::player_presentation::PresentationContext<'_>,
    shared_meshes: Res<'_, BubbleSharedMeshes>,
    mut lab_state: ResMut<'_, crate::LabState>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let admission = admitted_smog_actors(&catalog, &registry, &media_status)
        .and_then(|actors| Ok((prewarm_smog_field(&mut grid, &config)?, actors)));
    let (initial_states, mobile_archetypes) = match admission {
        Ok(admitted) => admitted,
        Err(error) => {
            crate::media::reject(error.into(), &mut media_status, &mut lab_state, &mut exit);
            return;
        }
    };

    // 2. Initialize persistent 3D textures with pre-warmed data
    let density_image = images.add(smog_field_image(grid.density_bytes().to_vec()));
    let velocity_image = images.add(smog_field_image(grid.velocity_bytes().to_vec()));

    // 3. Generate pre-baked tileable 32x32x32 3D wispy noise texture
    let noise_image = images.add(smog_noise_image());

    // 4. Initialize SmogVolumeMaterial and spawn volume bounding box entity
    let extent = LAB_VOLUME_MAX - LAB_VOLUME_MIN;
    let center = (LAB_VOLUME_MIN + LAB_VOLUME_MAX) * 0.5;
    let uniform = initial_smog_uniform(&initial_states);

    let mat_handle = materials.add(SmogVolumeMaterial {
        uniform,
        density_texture: density_image.clone(),
        velocity_texture: velocity_image.clone(),
        noise_texture: noise_image.clone(),
    });

    let box_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    commands.spawn((
        Mesh3d(box_mesh),
        MeshMaterial3d(mat_handle.clone()),
        Transform::from_translation(center).with_scale(extent),
        Visibility::default(),
        SmogVolumeMarker,
    ));

    commands.insert_resource(SmogGpuFrame {
        uniform,
        generation: 0,
    });
    commands.insert_resource(SmogTextures {
        density_image,
        velocity_image,
        noise_image,
        material_handle: mat_handle,
    });

    // 5. Spawn the 10 Bouncing Bubbles from admitted catalog identities (Zero hardcoded slugs)
    let sphere_mesh = shared_meshes.sphere_mesh.clone();

    for (i, state) in initial_states.iter().enumerate() {
        let owner = owners.palette.showcase(i);
        if let Err(error) = owners.palette.preview_style(owner) {
            owners.reject(error, &mut exit);
            return;
        }
        let unit_type_id = mobile_archetypes[i % mobile_archetypes.len()];
        let texture = registry.get_texture(unit_type_id);
        let meta = registry.get_meta(unit_type_id);

        let render_instance = showcase_render_instance(i, state, owner, unit_type_id);
        let bubble_uniform = BubbleUniformGpu::from_instance(
            &render_instance,
            showcase_style(i),
            crate::bubble_shader::still_frame_lanes(meta, render_instance.anim_state),
        );
        let bubble_mat = bubble_materials.add(BubbleDomeMaterial::new(bubble_uniform, texture));

        commands.spawn((
            BouncingSmogBubble {
                index: i,
                velocity: state.velocity,
                radius: state.radius,
                wall_bounces: state.wall_bounces,
                peer_bounces: state.peer_bounces,
            },
            BubbleInstance(render_instance),
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(bubble_mat),
            Transform::from_translation(state.position).with_scale(Vec3::splat(state.radius)),
            Visibility::default(),
        ));
    }
}

/// A 3D RGBA8 image of the fluid grid's size holding `data`, sampled linearly and clamped at the
/// volume edges. `data` is one of the grid's upload buffers.
#[expect(
    clippy::cast_possible_truncation,
    reason = "fluid grid indices and counts stay far below the range of f32 and i32"
)]
fn smog_field_image(data: Vec<u8>) -> Image {
    // PERFORMANCE RATIONALE:
    // Why: These textures change every frame, so a retained main-world copy is never read again.
    // What: Render-world usage drops ~1.66 MB of CPU bytes after the first extraction; later frames
    // reach the same textures through `gpu_upload`.
    // Trade-off: The main world can no longer read these images back.
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
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::ClampToEdge,
        address_mode_v: bevy::image::ImageAddressMode::ClampToEdge,
        address_mode_w: bevy::image::ImageAddressMode::ClampToEdge,
        mag_filter: bevy::image::ImageFilterMode::Linear,
        min_filter: bevy::image::ImageFilterMode::Linear,
        mipmap_filter: bevy::image::ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// Edge length, in texels, of the tileable wispy noise texture.
const NOISE_DIM: usize = 32;

/// The tileable `NOISE_DIM`-cubed RGBA8 wispy noise texture, sampled linearly and repeating.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "fluid grid indices and counts stay far below the range of f32 and i32"
)]
fn smog_noise_image() -> Image {
    let mut noise_data = Vec::with_capacity(NOISE_DIM * NOISE_DIM * NOISE_DIM * 4);
    for z in 0..NOISE_DIM {
        let wz = (z as f32) * (std::f32::consts::TAU / NOISE_DIM as f32);
        for y in 0..NOISE_DIM {
            let wy = (y as f32) * (std::f32::consts::TAU / NOISE_DIM as f32);
            for x in 0..NOISE_DIM {
                let wx = (x as f32) * (std::f32::consts::TAU / NOISE_DIM as f32);
                noise_data.extend_from_slice(&smog_noise_texel(wx, wy, wz));
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
    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::Repeat,
        address_mode_v: bevy::image::ImageAddressMode::Repeat,
        address_mode_w: bevy::image::ImageAddressMode::Repeat,
        mag_filter: bevy::image::ImageFilterMode::Linear,
        min_filter: bevy::image::ImageFilterMode::Linear,
        mipmap_filter: bevy::image::ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// One noise texel at phase coordinates `(wx, wy, wz)`, each in `[0, TAU)`: R and G are ridged
/// wisps, B and A smooth modulation.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "fluid grid indices and counts stay far below the range of f32 and i32"
)]
fn smog_noise_texel(wx: f32, wy: f32, wz: f32) -> [u8; 4] {
    use math::{cos, powf, sin};
    let s1 = sin(wx + cos(wy)) * cos(wy + cos(wz)) + sin(wz + cos(wx)) * 0.5;
    let s2 = sin(2.0 * wx - wy + sin(wz)) * cos(2.0 * wz - wx);
    let s3 = sin(3.0 * wy + 2.0 * wz - wx) * cos(2.0 * wx + 2.0 * wy);
    let ridge1 = powf((1.0 - (s1 * 0.60).abs()).clamp(0.0, 1.0), 1.25);
    let ridge2 = powf((1.0 - (s2 * 0.75).abs()).clamp(0.0, 1.0), 1.35);
    let ridge3 = powf((1.0 - (s3 * 0.80).abs()).clamp(0.0, 1.0), 1.40);
    let r = (0.50 * ridge1 + 0.32 * ridge2 + 0.18 * ridge3).clamp(0.0, 1.0);

    let g1 = cos(wx - wz) * sin(2.0 * wy + wx);
    let g2 = sin(2.0 * wz - wy) * cos(2.0 * wx + wz);
    let g = (0.60 * powf((1.0 - (g1 * 0.75).abs()).clamp(0.0, 1.0), 1.30)
        + 0.40 * powf((1.0 - (g2 * 0.75).abs()).clamp(0.0, 1.0), 1.40))
    .clamp(0.0, 1.0);

    let b = (0.5 + 0.45 * (sin(wy - 2.0 * wz) * cos(2.0 * wx + wy))).clamp(0.0, 1.0);
    let a = (0.5 + 0.45 * (cos(2.0 * wz - wx) * sin(wy + 2.0 * wz))).clamp(0.0, 1.0);

    [
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
        (a * 255.0) as u8,
    ]
}

/// The volume uniform for the first frame: the Visual Lab volume, the shading constants and the
/// pre-warmed bubbles in the analytic cavity slots (the first [`MAX_SMOG_SHADER_BUBBLES`]).
#[expect(
    clippy::cast_precision_loss,
    reason = "fluid grid indices and counts stay far below the range of f32 and i32"
)]
fn initial_smog_uniform(initial_states: &[BouncingBubbleState]) -> SmogVolumeUniformGpu {
    let volume_min = LAB_VOLUME_MIN.extend(0.4);
    let volume_max = LAB_VOLUME_MAX.extend(1.15);

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

    SmogVolumeUniformGpu {
        volume_min,
        volume_max,
        base_tint_time: Vec4::new(0.28, 0.46, 0.68, 1.5),
        warm_tint_anisotropy: Vec4::new(1.0, 0.82, 0.52, 0.45),
        light_dir_intensity: Vec4::new(0.45, 0.75, 0.48, 1.85),
        control_params: Vec4::new(BOUNCING_BUBBLE_COUNT as f32, 1.85, 1.0, 1.0),
        bubble_pos_radius: packed_pos_radius,
        bubble_vel_boost: packed_vel_boost,
    }
}

/// The presentation style of showcase bubble `index`, from its team colour.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "fluid grid indices and counts stay far below the range of f32 and i32"
)]
fn showcase_style(index: usize) -> crate::player_presentation::PlayerStyle {
    let tc = bubble_team_color(index);
    let linear_tc = math::srgb_to_linear(tc);
    crate::player_presentation::PlayerStyle {
        srgb: [tc.x, tc.y, tc.z],
        linear: Vec4::new(linear_tc.red, linear_tc.green, linear_tc.blue, 1.0),
        ui: bevy_egui::egui::Color32::from_rgb(
            (tc.x * 255.0) as u8,
            (tc.y * 255.0) as u8,
            (tc.z * 255.0) as u8,
        ),
    }
}

/// The render instance of showcase bubble `index` at its pre-warmed `state`, walking at full
/// health under the client-only identity `SHOWCASE_ENTITY_ID_BASE + index`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "fluid grid indices and counts stay far below the range of f32 and i32"
)]
fn showcase_render_instance(
    index: usize,
    state: &BouncingBubbleState,
    owner: sim_types::PlayerId,
    unit_type_id: UnitTypeId,
) -> BubbleRenderInstance {
    BubbleRenderInstance {
        entity_id: EntityId(SHOWCASE_ENTITY_ID_BASE + index as u32),
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
        anim_state: AnimState::Walk,
        attack_swing: None,
        hp_ratio: 1.0,
        shield_ratio: 1.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: Some(1.0),
        selected: false,
        concealment: sim_types::Concealment::None,
        speed_boosted: false,
        stance: sim_types::StanceView::default(),
        status: sim_types::StatusFlags::default(),
        barrier_ratio: 0.0,
    }
}

/// Lists the mobile catalog identities that can present smog actors, in catalog order.
///
/// An actor walks, so its own media must admit the walk action. Known copies of the concept video
/// are excluded because they would show the same art under several identities.
///
/// # Errors
/// Returns [`SmogError::NoAdmittedActor`] when no identity qualifies. Nothing is substituted.
fn admitted_smog_actors(
    catalog: &CatalogResource,
    registry: &FlipbookRegistry,
    media: &crate::media::ClientMediaStatus,
) -> Result<Vec<UnitTypeId>, SmogError> {
    let actors: Vec<UnitTypeId> = catalog
        .archetypes()
        .filter(|archetype| !matches!(archetype.shape, ShapeKind::BuildingDome { .. }))
        .map(|archetype| archetype.unit_type_id)
        .filter(|&unit_type| {
            media.require(registry, unit_type, AnimState::Walk).is_ok()
                && media.audit(unit_type).is_none_or(|audit| {
                    audit.status
                        != asset_baker::media_audit::MediaAuditStatus::KnownConceptDuplicate
                })
        })
        .collect();
    if actors.is_empty() {
        return Err(SmogError::NoAdmittedActor);
    }
    Ok(actors)
}

/// Runs the startup warm-up so the first drawn frame already shows developed smog.
///
/// # Errors
/// Returns the first [`SmogError`] from bubble initialization, kinematics or the fluid step.
fn prewarm_smog_field(
    grid: &mut SmogGrid3d,
    config: &SmogConfig,
) -> Result<[BouncingBubbleState; BOUNCING_BUBBLE_COUNT], SmogError> {
    const PREWARM_DT_SECS: f32 = 1.0 / 30.0;
    const PREWARM_STEPS: usize = 26;

    let mut states = initial_bouncing_bubbles(&config.bounds)?;
    for _ in 0..PREWARM_STEPS {
        step_bouncing_kinematics(&mut states, &config.bounds, PREWARM_DT_SECS)?;
        let mut sources = [BubbleSmogSource::default(); BOUNCING_BUBBLE_COUNT];
        for (index, state) in states.iter().enumerate() {
            sources[index] = BubbleSmogSource {
                position: state.position,
                velocity: state.velocity,
                radius: state.radius,
                team_color: bubble_team_color(index),
                emission_strength: 1.0,
                influence_strength: 1.0,
            };
        }
        grid.step(&sources, config, PREWARM_DT_SECS)?;
    }
    Ok(states)
}

/// Per-frame kinematics system updating the 10 bouncing bubbles with elastic reflections and peer collisions.
// PERFORMANCE RATIONALE:
// Why: Allocating a `Vec<BouncingBubbleState>` on every frame causes unnecessary heap churn.
// What: We maintain a pre-allocated `Local<Vec<BouncingBubbleState>>` scratch vector.
// Trade-off: Zero per-frame allocations during steady-state simulation.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn sync_bouncing_smog_bubbles_system(
    time: Res<'_, Time>,
    config: Res<'_, SmogConfig>,
    mut lab_state: ResMut<'_, crate::LabState>,
    mut media_status: ResMut<'_, crate::media::ClientMediaStatus>,
    mut exit: EventWriter<'_, AppExit>,
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
    if media_status.retain_failure(&mut lab_state) {
        return;
    }
    let is_visible = lab_state.current_tab == crate::LabTab::VisualLab
        && lab_state.smog_bouncing_active
        && lab_state.opt_smog_volume;

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
    // The heading comes from the render instance, which holds the last heading the kinematics
    // produced (the spawn heading before the first step). Reading it back from the transform's
    // rotation would call the host's `atan2` (`Quat::to_euler`) and would return the negated
    // heading, because the rotation is about -heading.
    for (bubble, transform, instance, _) in query.iter() {
        scratch.push(BouncingBubbleState {
            index: bubble.index,
            position: transform.translation,
            velocity: bubble.velocity,
            radius: bubble.radius,
            heading_angle_rad: instance.0.heading_angle_rad,
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
        Err(error) => {
            crate::media::reject(error.into(), &mut media_status, &mut lab_state, &mut exit);
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
            transform.rotation = math::quat_from_rotation_y(-state.heading_angle_rad);
            bubble.velocity = state.velocity;
            bubble.wall_bounces = state.wall_bounces;
            bubble.peer_bounces = state.peer_bounces;

            instance.0.world_pos = [state.position.x, state.position.z, state.position.y];
            instance.0.velocity = [state.velocity.x, state.velocity.z];
            instance.0.heading_angle_rad = state.heading_angle_rad;
        }
    }
}

/// Bubbles the smog field reads: every bubble except the volume proxy, with the markers that
/// tell showcase, preview and battle bubbles apart.
type SmogBubbleQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Transform,
        &'static BubbleInstance,
        Option<&'static BouncingSmogBubble>,
        Option<&'static crate::scene_controller::LabPreviewBubble>,
        Option<&'static Visibility>,
    ),
    Without<SmogVolumeMarker>,
>;

/// The smog volume's bounding proxy entity.
type SmogVolumeQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Visibility, &'static mut Transform),
    (With<SmogVolumeMarker>, Without<BubbleInstance>),
>;

/// Per-frame fluid field update system: collects sources, steps `SmogGrid3d`, and publishes the
/// packed uniform in [`SmogGpuFrame`] for the render-world upload.
// PERFORMANCE RATIONALE:
// Why: Re-allocating source lists per frame creates garbage.
// What: We reuse a `Local<Vec<BubbleSmogSource>>` scratch buffer, achieving zero allocations per frame.
// Trade-off: The buffer keeps its peak capacity for the life of the app, and the system must
// clear it before each use instead of starting from a fresh list.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub(crate) fn update_smog_field_system(
    time: Res<'_, Time>,
    config: Res<'_, SmogConfig>,
    mut lab_state: ResMut<'_, crate::LabState>,
    mut media_status: ResMut<'_, crate::media::ClientMediaStatus>,
    mut exit: EventWriter<'_, AppExit>,
    mut stats: ResMut<'_, SmogStats>,
    mut grid: ResMut<'_, SmogGrid3d>,
    mut gpu_frame: ResMut<'_, SmogGpuFrame>,
    mut sources_scratch: Local<'_, Vec<BubbleSmogSource>>,
    mut volume_query: SmogVolumeQuery<'_, '_>,
    bubble_query: SmogBubbleQuery<'_, '_>,
    camera_query: Query<'_, '_, &crate::camera::RtsCamera>,
    terrain: Option<Res<'_, crate::terrain::TerrainState>>,
) {
    if media_status.retain_failure(&mut lab_state) {
        return;
    }
    let enabled = lab_state.opt_smog_volume;

    let current_tab = lab_state.current_tab;
    let target = smog_volume_target(
        current_tab,
        terrain.as_deref(),
        camera_query.iter().next(),
        &bubble_query,
    );
    let enabled = enabled && target.is_some();
    if let Some((target_min, target_max)) = target {
        grid.set_bounds_quantized(target_min, target_max);
    }

    let volume = SmogVolumeWindow::of(&grid);
    sync_volume_proxy(&mut volume_query, &volume, enabled);

    if !enabled {
        return;
    }

    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let mut effective_config = config.clone();
    effective_config.emission_rate = lab_state.smog_emission_rate;
    effective_config.wake_influence = lab_state.smog_wake_influence;

    sources_scratch.clear();
    let cavities = collect_smog_sources(
        &bubble_query,
        current_tab,
        lab_state.speed,
        &volume,
        &mut sources_scratch,
    );

    // First-frame trajectory pre-warm for LiveBattle and SwarmStressTest:
    // If smog is empty and units are moving, run 15 historical sub-steps backwards along velocity
    // to establish clean wake slipstreams immediately on transition.
    if matches!(
        current_tab,
        crate::LabTab::LiveBattle | crate::LabTab::SwarmStressTest
    ) && stats.total_density < 0.5
        && sources_scratch.iter().any(|s| s.velocity.length() > 0.2)
    {
        if let Err(error) =
            prewarm_wake_trajectories(&mut grid, &sources_scratch, &effective_config)
        {
            crate::media::reject(error.into(), &mut media_status, &mut lab_state, &mut exit);
            return;
        }
    }

    // Step 3D fluid simulation
    let metrics = match grid.step(&sources_scratch, &effective_config, dt) {
        Ok(m) => m,
        Err(error) => {
            crate::media::reject(error.into(), &mut media_status, &mut lab_state, &mut exit);
            return;
        }
    };

    lab_state.smog_bubble_count = sources_scratch.len();

    stats.total_density = metrics.total_density;
    stats.peak_density = metrics.peak_density;
    stats.max_speed = metrics.max_speed;
    stats.max_vorticity = metrics.max_vorticity;
    stats.active_bubbles = sources_scratch.len();
    stats.step_count += 1;

    publish_smog_frame(&mut gpu_frame, &volume, &cavities, dt);
}

/// The fluid grid's current world-space box, read once per frame after any re-centring.
struct SmogVolumeWindow {
    min: Vec3,
    max: Vec3,
    extent: Vec3,
    center: Vec3,
}

impl SmogVolumeWindow {
    fn of(grid: &SmogGrid3d) -> Self {
        let actual_min = grid.bounds_min();
        let actual_max = grid.bounds_max();
        let actual_extent = actual_max - actual_min;
        let actual_center = (actual_min + actual_max) * 0.5;
        Self {
            min: actual_min,
            max: actual_max,
            extent: actual_extent,
            center: actual_center,
        }
    }
}

/// The world-space `(min, max)` box the smog volume should cover on `current_tab`, or `None` when
/// the tab has no smog window yet.
fn smog_volume_target(
    current_tab: crate::LabTab,
    terrain: Option<&crate::terrain::TerrainState>,
    camera: Option<&crate::camera::RtsCamera>,
    bubble_query: &SmogBubbleQuery<'_, '_>,
) -> Option<(Vec3, Vec3)> {
    match current_tab {
        crate::LabTab::VisualLab => Some((LAB_VOLUME_MIN, LAB_VOLUME_MAX)),
        crate::LabTab::SwarmStressTest => Some((
            Vec3::new(
                SWARM_MIN_X - SWARM_VOLUME_MARGIN,
                0.0,
                SWARM_MIN_Z - SWARM_VOLUME_MARGIN,
            ),
            Vec3::new(
                SWARM_MAX_X + SWARM_VOLUME_MARGIN,
                2.8,
                SWARM_MAX_Z + SWARM_VOLUME_MARGIN,
            ),
        )),
        // The window comes from the loaded map, so it does not exist before a map does.
        crate::LabTab::LiveBattle => {
            terrain.map(|terrain| live_battle_volume(terrain, camera, bubble_query))
        }
    }
}

/// The live battle smog window: centred on the camera focus and kept on the map, snapped to whole
/// voxels, with its floor at the lowest visible mobile unit near the window (or below the camera).
#[expect(
    clippy::cast_precision_loss,
    reason = "fluid grid indices and counts stay far below the range of f32 and i32"
)]
fn live_battle_volume(
    terrain: &crate::terrain::TerrainState,
    camera: Option<&crate::camera::RtsCamera>,
    bubble_query: &SmogBubbleQuery<'_, '_>,
) -> (Vec3, Vec3) {
    let map_size = Vec2::new(terrain.map.width() as f32, terrain.map.height() as f32);
    let focus = camera.map_or(map_size * 0.5, |cam| cam.target);
    let center = live_battle_window_center(focus, map_size);
    let cam_ground_y = camera.map_or(0.0, |cam| (cam.look_at_y - 1.2).max(0.0));

    // Find the lowest ground_z of visible mobile units inside the horizontal camera window
    // (treating 0.0 low ground as a valid first-class altitude).
    let mut min_mobile_ground_y: Option<f32> = None;
    for (transform, instance, bouncing, preview, vis) in bubble_query {
        if matches!(vis, Some(Visibility::Hidden))
            || bouncing.is_some()
            || preview.is_some()
            || instance.0.shape != ShapeKind::UnitSphere
        {
            continue;
        }
        let p = transform.translation;
        if (p.x - center.x).abs() <= LIVE_BATTLE_HALF_EXTENT_X + 1.5
            && (p.z - center.y).abs() <= LIVE_BATTLE_HALF_EXTENT_Z + 1.5
        {
            let gz = instance.0.ground_z.max(0.0);
            min_mobile_ground_y = Some(min_mobile_ground_y.map_or(gz, |cur| cur.min(gz)));
        }
    }

    let ground_y = min_mobile_ground_y.unwrap_or(cam_ground_y).max(0.0);

    let step_x = LIVE_BATTLE_HALF_EXTENT_X * 2.0 / GRID_NX as f32;
    let step_y = LIVE_BATTLE_EXTENT_Y / GRID_NY as f32;
    let step_z = LIVE_BATTLE_HALF_EXTENT_Z * 2.0 / GRID_NZ as f32;

    let center_x = (center.x / step_x).round() * step_x;
    let base_y = (ground_y / step_y).floor() * step_y;
    let center_z = (center.y / step_z).round() * step_z;

    (
        Vec3::new(
            center_x - LIVE_BATTLE_HALF_EXTENT_X,
            base_y,
            center_z - LIVE_BATTLE_HALF_EXTENT_Z,
        ),
        Vec3::new(
            center_x + LIVE_BATTLE_HALF_EXTENT_X,
            base_y + LIVE_BATTLE_EXTENT_Y,
            center_z + LIVE_BATTLE_HALF_EXTENT_Z,
        ),
    )
}

/// Shows or hides the volume proxy and fits it to the grid's box.
fn sync_volume_proxy(
    volume_query: &mut SmogVolumeQuery<'_, '_>,
    volume: &SmogVolumeWindow,
    enabled: bool,
) {
    for (mut vis, mut transform) in volume_query {
        let target_vis = if enabled {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target_vis {
            *vis = target_vis;
        }
        transform.translation = volume.center;
        transform.scale = volume.extent;
    }
}

/// Appends one smog source per visible bubble that `current_tab` feeds into the field, in query
/// order, and offers each to the analytic cavity slots. `lab_speed` is the Visual Lab preview
/// speed, in world units per second.
///
/// Battle velocities arrive in world units per tick and are converted to world units per second;
/// battle and swarm bubbles outside the volume (with a margin) are skipped.
fn collect_smog_sources(
    bubble_query: &SmogBubbleQuery<'_, '_>,
    current_tab: crate::LabTab,
    lab_speed: f32,
    volume: &SmogVolumeWindow,
    sources: &mut Vec<BubbleSmogSource>,
) -> CavitySlots {
    let actual_min = volume.min;
    let actual_max = volume.max;
    let mut cavities = CavitySlots::default();

    for (transform, instance, bouncing, preview, visibility) in bubble_query {
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
                    Vec3::new(-lab_speed, 0.0, 0.0)
                } else {
                    Vec3::new(instance.0.velocity[0], 0.0, instance.0.velocity[1])
                }
            },
            |b| b.velocity,
        );

        if is_live_battle_unit {
            // Snapshot velocity is world units per tick; the fluid works in world units per second.
            vel /= SIM_TICK_PERIOD.as_secs_f32();

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

        sources.push(BubbleSmogSource {
            position: pos,
            velocity: vel,
            radius,
            team_color,
            emission_strength,
            influence_strength,
        });

        let wake_boost = 1.0 + (vel.length() * 0.35).min(2.0);
        cavities.offer(
            pos.distance_squared(volume.center),
            pos.extend(radius),
            vel.extend(wake_boost),
        );
    }
    cavities
}

/// Replays 15 sub-steps of `1/30` s with every source moved back along its velocity, oldest
/// first, so a freshly entered battle or swarm view starts with developed wakes.
///
/// # Errors
/// Returns the first [`SmogError`] from a fluid step.
#[expect(
    clippy::cast_precision_loss,
    reason = "fluid grid indices and counts stay far below the range of f32 and i32"
)]
fn prewarm_wake_trajectories(
    grid: &mut SmogGrid3d,
    sources: &[BubbleSmogSource],
    config: &SmogConfig,
) -> Result<(), SmogError> {
    let mut prewarm_sources = sources.to_vec();
    let prewarm_dt = 1.0 / 30.0;
    for step in (1..=15).rev() {
        let t_back = step as f32 * prewarm_dt;
        for (i, src) in sources.iter().enumerate() {
            prewarm_sources[i].position = src.position - src.velocity * t_back;
        }
        grid.step(&prewarm_sources, config, prewarm_dt)?;
    }
    Ok(())
}

/// Writes this step's volume box, animation time and cavity slots into the uniform and bumps the
/// generation the render-world upload watches.
#[expect(
    clippy::cast_precision_loss,
    reason = "fluid grid indices and counts stay far below the range of f32 and i32"
)]
fn publish_smog_frame(
    gpu_frame: &mut SmogGpuFrame,
    volume: &SmogVolumeWindow,
    cavities: &CavitySlots,
    dt: f32,
) {
    let actual_min = volume.min;
    let actual_max = volume.max;
    let uniform = &mut gpu_frame.uniform;
    uniform.volume_min = Vec4::new(actual_min.x, actual_min.y, actual_min.z, 0.4);
    uniform.volume_max = Vec4::new(actual_max.x, actual_max.y, actual_max.z, 1.15);
    uniform.base_tint_time.w += dt;
    uniform.control_params.x = cavities.len() as f32;
    uniform.control_params.w = 1.0;
    uniform.bubble_pos_radius = cavities.pos_radius();
    uniform.bubble_vel_boost = cavities.vel_boost();
    gpu_frame.generation += 1;
}
