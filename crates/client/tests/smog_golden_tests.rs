//! Bit-exact characterisation goldens for the smog fluid solver, the bouncing-bubble kinematics and
//! the two smog Bevy systems.
//!
//! # Responsibility
//! Pins the exact output of [`SmogGrid3d::step`], [`step_bouncing_kinematics`],
//! `setup_smog_volume_system` and `update_smog_field_system` so that the structural splits of
//! `crates/client/src/smog/{mod,fluid,kinematics}.rs` (Phase 9, WP7, step 9.2) are provably
//! behaviour-preserving. Every `f32` is hashed by its raw bits (`to_bits`), so a reordered float
//! expression, a reordered solver stage or a reordered collision pass changes the hash and fails
//! the test.
//!
//! # Fit
//! The fluid and kinematics goldens run on the CPU only, through the public smog API: the fluid
//! fields are read back by sampling every voxel centre and by the packed RGBA8 upload buffers. The
//! system goldens run the production `SmogPlugin` in a headless `App` with a manual clock, with no
//! window and no GPU. They complement the semantic tests in `smog_bouncing_bubbles_tests.rs`, which
//! accept many fields; the goldens accept exactly one.
//!
//! # Coverage
//! - Fluid: the 10-bubble showcase (vorticity confinement on), a 24-source mix and a 48-source
//!   crowd (confinement off, crowd wake radius) with non-emitting, stationary and slow sources,
//!   nonzero buoyancy and zero dissipation.
//! - Kinematics: the 10-bubble showcase, a non-planar set, the 400-bubble spatial-hash path and the
//!   600-bubble all-pairs fallback beyond [`client::smog::MAX_BOUNCING_BUBBLES`].
//! - Systems: the Visual Lab startup and showcase frames (textures, noise, material, bubbles,
//!   uniform), a Visual Lab preview bubble, the swarm tab and the live battle tab (window
//!   placement, velocity scale, trajectory pre-warm and cavity slots).
//!
//! # Invariants
//! - The golden constants were recorded on `main` at `9dbf715`, before any WP7 edit.
//! - A golden may only change together with a deliberate, documented change in smog output.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::unwrap_used,
    clippy::expect_used
)]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use client::smog::{
    bubble_team_color, generate_bouncing_bubbles, initial_bouncing_bubbles,
    step_bouncing_kinematics, BouncingBubbleState, BouncingSmogBubble, BubbleSmogSource,
    SmogArenaBounds, SmogConfig, SmogError, SmogGpuFrame, SmogGrid3d, SmogPlugin, SmogStats,
    SmogStepMetrics, SmogTextures, GRID_NX, GRID_NY, GRID_NZ, LAB_VOLUME_MAX, LAB_VOLUME_MIN,
};
use client::ResolvedAssetsRoot;

/// Fluid steps run per fluid scenario.
const FLUID_STEPS: usize = 120;

/// Kinematics steps run per kinematics scenario.
const KINEMATICS_STEPS: usize = 120;

/// Every how many fluid steps the whole field is hashed (metrics are hashed every step).
const FIELD_CHECKPOINT: usize = 30;

/// Fixed fluid and kinematics step, in seconds.
const STEP_DT: f32 = 1.0 / 60.0;

/// 64-bit FNV-1a, written out so the golden does not depend on a hasher whose algorithm may
/// change between Rust or crate releases.
struct Fnv64(u64);

impl Fnv64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.bytes(&v.to_bits().to_le_bytes());
    }

    fn vec3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }

    fn quat(&mut self, q: Quat) {
        self.f32(q.x);
        self.f32(q.y);
        self.f32(q.z);
        self.f32(q.w);
    }

    fn metrics(&mut self, m: &SmogStepMetrics) {
        self.f32(m.total_density);
        self.f32(m.peak_density);
        self.f32(m.max_speed);
        self.f32(m.max_vorticity);
        self.u64(m.active_sources as u64);
    }

    fn state(&mut self, s: &BouncingBubbleState) {
        self.u64(s.index as u64);
        self.vec3(s.position);
        self.vec3(s.velocity);
        self.f32(s.radius);
        self.f32(s.heading_angle_rad);
        self.u64(s.wall_bounces);
        self.u64(s.peer_bounces);
    }

    /// Hashes every voxel's density, velocity, glow and vorticity, and both upload buffers.
    fn grid(&mut self, grid: &SmogGrid3d) {
        self.vec3(grid.bounds_min());
        self.vec3(grid.bounds_max());
        for iz in 0..GRID_NZ {
            for iy in 0..GRID_NY {
                for ix in 0..GRID_NX {
                    let p = grid.grid_to_world(ix, iy, iz);
                    self.f32(grid.sample_density_trilinear(p));
                    self.vec3(grid.sample_velocity_trilinear(p));
                    self.f32(grid.sample_glow_trilinear(p));
                    self.f32(grid.sample_vorticity_magnitude(p));
                }
            }
        }
        self.bytes(&grid.rgba_density_bytes);
        self.bytes(&grid.rgba_velocity_bytes);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// How a fluid scenario derives a source from a kinematics state.
#[derive(Clone, Copy)]
enum SourceMix {
    /// Every bubble emits and influences at full strength, as the showcase pre-warm does.
    Showcase,
    /// Every fourth bubble only displaces, every fifth stands still, every seventh crawls.
    Mixed,
}

fn source_for(index: usize, state: &BouncingBubbleState, mix: SourceMix) -> BubbleSmogSource {
    let mut source = BubbleSmogSource {
        position: state.position,
        velocity: state.velocity,
        radius: state.radius,
        team_color: bubble_team_color(index),
        emission_strength: 1.0,
        influence_strength: 1.0,
    };
    if let SourceMix::Mixed = mix {
        if index.is_multiple_of(4) {
            source.emission_strength = 0.0;
            source.influence_strength = 1.7;
        }
        if index.is_multiple_of(5) {
            source.velocity = Vec3::ZERO;
        } else if index.is_multiple_of(7) {
            source.velocity *= 0.04;
        }
    }
    source
}

/// Runs kinematics and fluid together for [`FLUID_STEPS`] and hashes metrics and fields.
fn fluid_scenario_hash(
    mut states: Vec<BouncingBubbleState>,
    config: &SmogConfig,
    mix: SourceMix,
) -> Result<u64, SmogError> {
    let mut grid = SmogGrid3d::new(LAB_VOLUME_MIN, LAB_VOLUME_MAX)?;
    let mut sources = Vec::with_capacity(states.len());
    let mut h = Fnv64::new();
    for step in 1..=FLUID_STEPS {
        step_bouncing_kinematics(&mut states, &config.bounds, STEP_DT)?;
        sources.clear();
        sources.extend(
            states
                .iter()
                .enumerate()
                .map(|(index, state)| source_for(index, state, mix)),
        );
        let metrics = grid.step(&sources, config, STEP_DT)?;
        h.metrics(&metrics);
        if step % FIELD_CHECKPOINT == 0 {
            h.grid(&grid);
        }
    }
    Ok(h.finish())
}

fn crowd_config() -> SmogConfig {
    SmogConfig {
        emission_rate: 3.0,
        wake_influence: 2.2,
        dissipation_rate: 0.0,
        vorticity_confinement: 0.8,
        buoyancy: 0.3,
        velocity_damping: 0.25,
        ..SmogConfig::default()
    }
}

const GOLDEN_FLUID_SHOWCASE: u64 = 0xb42e_8fd5_0690_ef08;
const GOLDEN_FLUID_MIXED_24: u64 = 0x2e2d_8269_5e27_898d;
const GOLDEN_FLUID_CROWD_48: u64 = 0x6ab3_c16e_837d_e74b;

/// The showcase: ten emitting bubbles with confinement on, default configuration.
#[test]
fn fluid_showcase_golden() -> Result<(), SmogError> {
    let config = SmogConfig::default();
    let states = initial_bouncing_bubbles(&config.bounds)?.to_vec();
    let hash = fluid_scenario_hash(states, &config, SourceMix::Showcase)?;
    assert_eq!(
        hash, GOLDEN_FLUID_SHOWCASE,
        "fluid showcase golden: {hash:#018x}"
    );
    Ok(())
}

/// Twenty-four mixed sources: confinement still on, every source branch exercised.
#[test]
fn fluid_mixed_sources_golden() -> Result<(), SmogError> {
    let config = crowd_config();
    let states = generate_bouncing_bubbles(24, &config.bounds)?;
    let hash = fluid_scenario_hash(states, &config, SourceMix::Mixed)?;
    assert_eq!(
        hash, GOLDEN_FLUID_MIXED_24,
        "fluid mixed golden: {hash:#018x}"
    );
    Ok(())
}

/// Forty-eight mixed sources: past the 32-source limit, so the crowd wake radius applies and
/// confinement is skipped.
#[test]
fn fluid_crowd_golden() -> Result<(), SmogError> {
    let config = crowd_config();
    let states = generate_bouncing_bubbles(48, &config.bounds)?;
    let hash = fluid_scenario_hash(states, &config, SourceMix::Mixed)?;
    assert_eq!(
        hash, GOLDEN_FLUID_CROWD_48,
        "fluid crowd golden: {hash:#018x}"
    );
    Ok(())
}

/// Steps kinematics for [`KINEMATICS_STEPS`] and hashes the event counts and every state.
fn kinematics_hash(
    mut states: Vec<BouncingBubbleState>,
    bounds: &SmogArenaBounds,
) -> Result<u64, SmogError> {
    let mut h = Fnv64::new();
    let mut total_walls = 0;
    for _ in 0..KINEMATICS_STEPS {
        let (walls, peers) = step_bouncing_kinematics(&mut states, bounds, STEP_DT)?;
        total_walls += walls;
        h.u64(walls);
        h.u64(peers);
        for state in &states {
            h.state(state);
        }
    }
    assert!(total_walls > 0, "the scenario must reach a wall");
    assert!(
        states.iter().any(|state| state.peer_bounces > 0),
        "the scenario must resolve a contact"
    );
    Ok(h.finish())
}

const GOLDEN_KINEMATICS_SHOWCASE: u64 = 0x6f2f_7e74_b1d3_130d;
const GOLDEN_KINEMATICS_NON_PLANAR: u64 = 0x0344_7f94_b623_00ee;
const GOLDEN_KINEMATICS_GRID_400: u64 = 0xe9db_6f7d_fd52_d328;
const GOLDEN_KINEMATICS_FALLBACK_600: u64 = 0xd093_7643_a076_3ad5;

#[test]
fn kinematics_showcase_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let states = initial_bouncing_bubbles(&bounds)?.to_vec();
    let hash = kinematics_hash(states, &bounds)?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_SHOWCASE,
        "kinematics showcase golden: {hash:#018x}"
    );
    Ok(())
}

/// Bubbles lifted off the floor with vertical velocity: the 3D (not planar) contact branch.
#[test]
fn kinematics_non_planar_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let mut states = initial_bouncing_bubbles(&bounds)?.to_vec();
    for (i, state) in states.iter_mut().enumerate() {
        state.position.y = bounds.min.y + state.radius + 0.3 + (i % 3) as f32 * 0.5;
        state.velocity.y = if i % 2 == 0 { 1.7 } else { -2.3 };
    }
    let hash = kinematics_hash(states, &bounds)?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_NON_PLANAR,
        "kinematics non-planar golden: {hash:#018x}"
    );
    Ok(())
}

#[test]
fn kinematics_spatial_grid_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let states = generate_bouncing_bubbles(400, &bounds)?;
    let hash = kinematics_hash(states, &bounds)?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_GRID_400,
        "kinematics grid golden: {hash:#018x}"
    );
    Ok(())
}

#[test]
fn kinematics_all_pairs_fallback_golden() -> Result<(), SmogError> {
    let bounds = SmogArenaBounds::default();
    let states = generate_bouncing_bubbles(600, &bounds)?;
    let hash = kinematics_hash(states, &bounds)?;
    assert_eq!(
        hash, GOLDEN_KINEMATICS_FALLBACK_600,
        "kinematics fallback golden: {hash:#018x}"
    );
    Ok(())
}

fn resolve_root() -> Result<ResolvedAssetsRoot, Box<dyn std::error::Error>> {
    Ok(ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .canonicalize()?,
    )?)
}

/// Headless application with the production smog plugin, real catalog, palette and media, and a
/// manual clock advancing [`STEP_DT`] per update.
fn production_smog_app(lab: client::LabState) -> Result<App, Box<dyn std::error::Error>> {
    let root = resolve_root()?;
    let definition = client::runtime_battle::BattleDefinition::load(
        root.as_path(),
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        STEP_DT,
    )));
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
    app.edit_schedule(Update, |schedule| {
        schedule.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
    });
    Ok(app)
}

/// A mobile battle unit sphere moving at `velocity` world units per tick along world X and Z.
fn battle_unit(entity_id: u32, pos: Vec3, velocity: [f32; 2]) -> client::BubbleInstance {
    client::BubbleInstance(sim_types::snapshot::BubbleRenderInstance {
        entity_id: sim_types::EntityId(entity_id),
        shape: sim_types::enums::ShapeKind::UnitSphere,
        silhouette: sim_types::enums::SilhouetteClass::LightProngs,
        unit_type: sim_types::ids::UnitTypeId(1),
        owner: sim_types::PlayerId(1),
        world_pos: [pos.x, pos.z, pos.y],
        ground_z: 0.0,
        radius: 0.45,
        velocity,
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

/// Hashes the published uniform, stats, field, volume proxy and every showcase bubble.
fn hash_smog_app(app: &mut App, h: &mut Fnv64) {
    let world = app.world_mut();
    let frame = world.resource::<SmogGpuFrame>();
    h.bytes(bytemuck::bytes_of(&frame.uniform));
    h.u64(frame.generation);
    let stats = *world.resource::<SmogStats>();
    h.f32(stats.total_density);
    h.f32(stats.peak_density);
    h.f32(stats.max_speed);
    h.f32(stats.max_vorticity);
    h.u64(stats.wall_bounces);
    h.u64(stats.peer_bounces);
    h.u64(stats.active_bubbles as u64);
    h.u64(stats.step_count);
    h.u64(world.resource::<client::LabState>().smog_bubble_count as u64);
    h.grid(world.resource::<SmogGrid3d>());

    let mut volumes =
        world.query_filtered::<(&Transform, &Visibility), With<client::SmogVolumeMarker>>();
    for (transform, visibility) in volumes.iter(world) {
        h.vec3(transform.translation);
        h.vec3(transform.scale);
        h.bytes(format!("{visibility:?}").as_bytes());
    }

    let mut bubbles: Vec<_> = world
        .query::<(
            &BouncingSmogBubble,
            &Transform,
            &client::BubbleInstance,
            &Visibility,
            &MeshMaterial3d<client::bubble_shader::BubbleDomeMaterial>,
        )>()
        .iter(world)
        .map(|(bubble, transform, instance, visibility, material)| {
            (
                bubble.clone(),
                *transform,
                instance.0.clone(),
                *visibility,
                material.0.clone(),
            )
        })
        .collect();
    bubbles.sort_by_key(|(bubble, ..)| bubble.index);
    let materials = world.resource::<Assets<client::bubble_shader::BubbleDomeMaterial>>();
    for (bubble, transform, instance, visibility, material) in &bubbles {
        h.u64(bubble.index as u64);
        h.vec3(bubble.velocity);
        h.f32(bubble.radius);
        h.u64(bubble.wall_bounces);
        h.u64(bubble.peer_bounces);
        h.vec3(transform.translation);
        h.quat(transform.rotation);
        h.vec3(transform.scale);
        h.bytes(format!("{instance:?}").as_bytes());
        h.bytes(format!("{visibility:?}").as_bytes());
        let material = materials.get(material).expect("bubble material exists");
        h.bytes(bytemuck::bytes_of(&material.uniform));
        h.bytes(format!("{:?}", material.alpha_mode).as_bytes());
        h.bytes(format!("{:?}", material.video_texture.is_some()).as_bytes());
    }
}

/// Hashes the three 3D textures and the volume material created at startup.
fn hash_smog_startup_assets(app: &App, h: &mut Fnv64) {
    let world = app.world();
    let textures = world.resource::<SmogTextures>();
    let images = world.resource::<Assets<Image>>();
    for handle in [
        &textures.density_image,
        &textures.velocity_image,
        &textures.noise_image,
    ] {
        let image = images.get(handle).expect("smog image exists");
        h.bytes(&image.data);
        h.bytes(format!("{:?}", image.texture_descriptor).as_bytes());
        h.bytes(format!("{:?}", image.sampler).as_bytes());
        h.bytes(format!("{:?}", image.asset_usage).as_bytes());
    }
    let materials = world.resource::<Assets<client::smog::SmogVolumeMaterial>>();
    let material = materials
        .get(&textures.material_handle)
        .expect("smog material exists");
    h.bytes(bytemuck::bytes_of(&material.uniform));
}

/// Guards a system golden against hashing a scenario that stopped stepping smog: returns the
/// number of sources the last step saw.
fn require_smog_activity(app: &App) -> usize {
    let stats = app.world().resource::<SmogStats>();
    assert!(
        stats.step_count > 0,
        "the scenario must step the smog field"
    );
    assert!(
        stats.total_density > 0.0,
        "the scenario must generate smog (total {})",
        stats.total_density
    );
    stats.active_bubbles
}

const GOLDEN_SYSTEMS_STARTUP: u64 = 0xc22d_27e6_2102_f857;
const GOLDEN_SYSTEMS_VISUAL_LAB: u64 = 0x6eba_f3b3_5763_21f6;
const GOLDEN_SYSTEMS_SWARM: u64 = 0xb535_0265_8fff_6213;
const GOLDEN_SYSTEMS_LIVE_BATTLE: u64 = 0xf9f1_5ce0_99f2_8320;

/// Startup assets, as `setup_smog_volume_system` creates them before any frame steps them.
#[test]
fn smog_startup_assets_golden() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState {
        smog_bouncing_active: true,
        ..default()
    })?;
    app.update();
    let mut h = Fnv64::new();
    hash_smog_startup_assets(&app, &mut h);
    hash_smog_app(&mut app, &mut h);
    let hash = h.finish();
    assert_eq!(hash, GOLDEN_SYSTEMS_STARTUP, "startup golden: {hash:#018x}");
    Ok(())
}

/// Visual Lab showcase with a preview bubble, through both systems for 40 frames.
#[test]
fn smog_visual_lab_systems_golden() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState {
        smog_bouncing_active: true,
        smog_emission_rate: 4.1,
        smog_wake_influence: 2.6,
        ..default()
    })?;
    app.update();
    let preview = Vec3::new(1.2, 0.6, -0.4);
    app.world_mut().spawn((
        client::scene_controller::LabPreviewBubble,
        battle_unit(77, preview, [0.0, 0.0]),
        Transform::from_translation(preview),
        Visibility::Inherited,
    ));
    let mut h = Fnv64::new();
    for frame in 1..=40 {
        app.update();
        if frame % 10 == 0 {
            hash_smog_app(&mut app, &mut h);
        }
    }
    assert_eq!(
        require_smog_activity(&app),
        client::BOUNCING_BUBBLE_COUNT + 1
    );
    let hash = h.finish();
    assert_eq!(
        hash, GOLDEN_SYSTEMS_VISUAL_LAB,
        "visual lab golden: {hash:#018x}"
    );
    Ok(())
}

/// Swarm tab: the swarm window, the swarm velocity scale and filter, and the trajectory pre-warm.
#[test]
fn smog_swarm_systems_golden() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState {
        current_tab: client::LabTab::SwarmStressTest,
        ..default()
    })?;
    for index in 0..30u32 {
        let column = index % 6;
        let row = index / 6;
        let pos = Vec3::new(
            client::scene_controller::SWARM_MIN_X + 1.0 + column as f32 * 2.3,
            0.45,
            client::scene_controller::SWARM_MIN_Z + 1.0 + row as f32 * 1.9,
        );
        let velocity = [0.9 + 0.1 * column as f32, -0.6 + 0.2 * row as f32];
        app.world_mut().spawn((
            battle_unit(5000 + index, pos, velocity),
            Transform::from_translation(pos),
            Visibility::Inherited,
        ));
    }
    let far = Vec3::new(client::scene_controller::SWARM_MAX_X + 40.0, 0.45, 0.0);
    app.world_mut().spawn((
        battle_unit(6000, far, [1.0, 0.0]),
        Transform::from_translation(far),
        Visibility::Inherited,
    ));
    let mut h = Fnv64::new();
    for frame in 1..=12 {
        app.update();
        if frame % 4 == 0 {
            hash_smog_app(&mut app, &mut h);
        }
    }
    assert_eq!(
        require_smog_activity(&app),
        30,
        "the far unit is filtered out"
    );
    let hash = h.finish();
    assert_eq!(hash, GOLDEN_SYSTEMS_SWARM, "swarm golden: {hash:#018x}");
    Ok(())
}

/// Live battle tab on a 64x48 map: window placement from camera and units, tick-rate velocity
/// scale, vertical anchoring, the camera-volume filter, pre-warm and nearest-first cavity slots.
#[test]
fn smog_live_battle_systems_golden() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = production_smog_app(client::LabState {
        current_tab: client::LabTab::LiveBattle,
        ..default()
    })?;
    let map = sim_map::GameMap::new(
        64,
        48,
        sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
    )?;
    app.insert_resource(client::terrain::TerrainState::new(map));
    app.world_mut().spawn(client::camera::RtsCamera {
        target: Vec2::new(30.0, 21.0),
        look_at_y: 1.9,
        ..default()
    });
    for index in 0..22u32 {
        let pos = Vec3::new(
            22.0 + (index % 11) as f32 * 1.4,
            0.3 + (index % 3) as f32 * 0.9,
            17.0 + (index / 11) as f32 * 3.1,
        );
        let velocity = [
            0.04 + 0.01 * (index % 5) as f32,
            -0.03 + 0.02 * (index % 4) as f32,
        ];
        app.world_mut().spawn((
            battle_unit(9000 + index, pos, velocity),
            Transform::from_translation(pos),
            Visibility::Inherited,
        ));
    }
    let outside = Vec3::new(60.0, 0.45, 45.0);
    app.world_mut().spawn((
        battle_unit(9100, outside, [0.1, 0.1]),
        Transform::from_translation(outside),
        Visibility::Inherited,
    ));
    let mut h = Fnv64::new();
    for frame in 1..=12 {
        app.update();
        if frame % 4 == 0 {
            hash_smog_app(&mut app, &mut h);
        }
    }
    assert_eq!(
        require_smog_activity(&app),
        22,
        "the unit off the window is filtered out"
    );
    let uniform = app.world().resource::<SmogGpuFrame>().uniform;
    assert_eq!(
        uniform.control_params.x,
        client::MAX_SMOG_SHADER_BUBBLES as f32,
        "more units than cavity slots"
    );
    let hash = h.finish();
    assert_eq!(
        hash, GOLDEN_SYSTEMS_LIVE_BATTLE,
        "live battle golden: {hash:#018x}"
    );
    Ok(())
}
