//! Reproducible real-render capture harness for the production movement-wake plugin.
//!
//! The same catalog, image loader, terrain renderer, material and emitter code run here
//! as in the game. Capture time freezes during readback; pipeline and save failures exit.

#[path = "benchmark_capture.rs"]
mod benchmark_capture;
#[path = "reset_capture.rs"]
pub(super) mod owner_reset;

use super::completion::CaptureCompletion;
pub use super::completion::CaptureVerificationError;
pub use super::view_light_capture::ViewLightCase;
use super::{MistMotion, MistPool, MistStats, MistWakePlugin};
use crate::{
    setup_bubble_assets_system, BubbleDomeMaterial, BubbleInstance, BubbleSharedMeshes,
    BubbleUniformGpu, CatalogResource, FlipbookRegistry, ResolvedAssetsRoot,
};
use asset_baker::{TerrainTheme, TerrainThemeCatalog};
use bevy::prelude::*;
use bevy::render::render_resource::{
    CachedPipelineState, PipelineCache, PipelineCacheError, PipelineDescriptor,
};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::render::{Render, RenderApp};
use bevy::time::TimeUpdateStrategy;
use clap::{Parser, ValueEnum};
use sim_core::catalog::UnitCatalog;
use sim_map::{map_format::MapFile, GameMap};
use sim_types::{AnimState, BubbleRenderInstance, EntityId, ShapeKind, WeaponVisualKind};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

/// Reproducible camera/movement configuration for an actual GPU capture.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum CaptureCase {
    /// Normal movement at close inspection distance.
    Close,
    /// Overclock accent at close inspection distance.
    Boost,
    /// Ninety-degree turn with a retained trailing path.
    Turn,
    /// Stop and natural tail dissipation.
    Stop,
    /// Fully stationary control with no continuing emission.
    Stationary,
    /// Several units over the actual shipped terrain.
    Gameplay,
    /// Four hundred simultaneous production emitters.
    Crowd,
    /// Stationary shell and media inspection with authored optical settings.
    Shell,
    /// Several stationary catalog shells at gameplay scale.
    ShellGameplay,
}

/// Snapshot signal held constant or recovered during an optical verification sequence.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum ShellState {
    /// Undamaged, unselected baseline.
    Healthy,
    /// Tactical selection rim.
    Selected,
    /// Weapon aiming aperture.
    Attacking,
    /// Peak shield ripple.
    ShieldHit,
    /// Peak directional hull impact.
    HullHit,
    /// Shield and hull response decaying across the capture sequence.
    Recovering,
    /// Critical health cracks and scanlines.
    Damaged,
}

/// Generic shape capability used to select real catalog media for shell captures.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum ShellShape {
    /// Mobile unit sphere.
    Sphere,
    /// Grounded building dome.
    Dome,
}

/// Command-line settings for the production rendering verification executable.
#[derive(Debug, Parser, Resource)]
pub struct CaptureOptions {
    /// Destination for three newly captured PNGs and the run metrics.
    #[arg(long)]
    pub out: PathBuf,
    /// Reproducible shot type.
    #[arg(long, value_enum, default_value_t = CaptureCase::Close)]
    pub case: CaptureCase,
    /// Fixed presentation frequency used for trajectory sampling.
    #[arg(long, default_value_t = 60)]
    pub fps: u32,
    /// Multisample count, either one or four.
    #[arg(long, default_value_t = 4)]
    pub msaa: u32,
    /// Optical snapshot signal for shell cases.
    #[arg(long, value_enum, default_value_t = ShellState::Healthy)]
    pub shell_state: ShellState,
    /// Select real catalog spheres or building domes for shell cases.
    #[arg(long, value_enum, default_value_t = ShellShape::Sphere)]
    pub shell_shape: ShellShape,
    /// Multiplier of actual catalog radius for scale invariance verification.
    #[arg(long, default_value_t = 1.0)]
    pub radius_scale: f32,
    /// Exercise the camera's HDR postprocess tonemapping path.
    #[arg(long)]
    pub hdr: bool,
    /// Benchmark 0, 100 or 400 emitters while retaining the same 400 moving actors.
    #[arg(long)]
    pub benchmark_emitters: Option<u16>,
    /// Real wall seconds measured after warmup; requires benchmark mode, range 10..60.
    #[arg(long, default_value_t = 10)]
    pub benchmark_seconds: u16,
    /// Verify exactly one 400-owner replacement with actual per-frame GPU queue evidence.
    #[arg(long)]
    pub verify_owner_reset: bool,
    /// Verify actual per-view lighting on identical frozen histories.
    #[arg(long, value_enum)]
    pub verify_view_lights: Option<ViewLightCase>,
    /// Inject invalid lux after genuine held callbacks; requires stalled lighting mode.
    #[arg(long)]
    pub verify_invalid_view_light: bool,
}

impl CaptureOptions {
    fn shell_case(&self) -> bool {
        matches!(self.case, CaptureCase::Shell | CaptureCase::ShellGameplay)
    }
}

#[derive(Resource, Clone)]
struct PipelineStatus {
    ready: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    mist_shader: Handle<Shader>,
    bubble_shader: Handle<Shader>,
    require_mist: bool,
}

#[derive(Resource)]
struct CaptureTerrain {
    map: GameMap,
    theme: TerrainTheme,
}

/// Stable ordering boundary for opt-in capture controllers.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum CaptureSystems {
    /// Actual catalog bubble motion and media synchronization.
    Motion,
}

#[derive(Resource, Default)]
pub(super) struct CaptureProgress {
    frames: u32,
    pub(super) running: bool,
    pub(super) elapsed_frames: u32,
    pub(super) image: usize,
    pub(super) pending: bool,
    pub(super) complete: bool,
}

#[derive(Component)]
struct CaptureBubble {
    origin: Vec3,
    index: usize,
}

/// Runs the real GPU harness and returns its process exit result.
///
/// # Errors
/// Returns errors for invalid settings, real catalog/map/theme loading, or output setup.
pub fn run_capture(options: CaptureOptions) -> Result<AppExit, Box<dyn std::error::Error>> {
    if ![30, 60, 144].contains(&options.fps) || ![1, 4].contains(&options.msaa) {
        return Err("capture fps must be 30, 60, or 144; MSAA must be 1 or 4".into());
    }
    if !options.radius_scale.is_finite() || !(0.25..=4.0).contains(&options.radius_scale) {
        return Err("radius scale must be finite and between 0.25 and 4".into());
    }
    if options
        .benchmark_emitters
        .is_some_and(|count| ![0, 100, 400].contains(&count))
        || (options.benchmark_emitters.is_some() && options.case != CaptureCase::Crowd)
        || !(10..=60).contains(&options.benchmark_seconds)
    {
        return Err(
            "benchmark requires crowd case, 0/100/400 emitters and 10..60 measured seconds".into(),
        );
    }
    owner_reset::validate(&options)?;
    super::view_light_capture::validate(&options)?;
    let view_light_case = options.verify_view_lights;
    let verify_owner_reset = options.verify_owner_reset;
    let capture_hdr = options.hdr;
    let benchmarking = options.benchmark_emitters.is_some();
    let require_mist = !options.shell_case() && options.benchmark_emitters != Some(0);
    std::fs::create_dir_all(&options.out)?;
    let root = ResolvedAssetsRoot::resolve()?;
    let file_path = root.bevy_file_path()?.to_owned();
    let units = std::fs::read_to_string(root.as_path().join("catalog/units.ron"))?;
    let structures = std::fs::read_to_string(root.as_path().join("catalog/structures.ron"))?;
    let catalog = UnitCatalog::from_ron_str_bundle(&units, &structures)?;
    let palette = crate::player_presentation::PlayerPalette::load(
        root.as_path(),
        catalog
            .archetypes()
            .filter(|arch| arch.showcase_hero)
            .count(),
    )?;
    let document = MapFile::load(&root.as_path().join("maps/default_arena.scmap.ron"))?.decode()?;
    let themes = TerrainThemeCatalog::load(&root.as_path().join("catalog/terrain_themes.ron"))?;
    let theme = themes
        .get(&document.metadata.theme)
        .ok_or("map theme is absent from actual theme catalog")?
        .clone();
    let map = document.to_game_map()?;
    let dt = Duration::from_secs_f64(1.0 / f64::from(options.fps));
    let completion = CaptureCompletion::default();
    let mut app = App::new();
    app.add_plugins(
        crate::presentation::GamePresentationPlugins
            .set(AssetPlugin {
                file_path,
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Space Scuffle — Production Mist Verification".into(),
                    resolution: bevy::window::WindowResolution::new(1280.0, 720.0)
                        .with_scale_factor_override(1.0),
                    present_mode: bevy::window::PresentMode::AutoNoVsync,
                    ..default()
                }),
                ..default()
            }),
    )
    .insert_resource(completion.clone())
    .insert_resource(root.clone())
    .insert_resource(CatalogResource(catalog))
    .insert_resource(palette)
    .init_resource::<crate::player_presentation::PresentationStatus>()
    .insert_resource(CaptureTerrain { map, theme })
    .insert_resource(options)
    .insert_resource(TimeUpdateStrategy::ManualDuration(dt))
    .init_resource::<CaptureProgress>()
    .add_plugins((
        crate::terrain::TerrainPlugin,
        crate::render_compatibility::RenderCompatibilityPlugin,
        MaterialPlugin::<BubbleDomeMaterial>::default(),
        MistWakePlugin,
        crate::bubble_optics::BubbleOpticsPlugin,
    ))
    .add_systems(
        Startup,
        (
            setup_bubble_assets_system,
            setup_capture_scene.after(super::setup_mist),
        )
            .chain(),
    )
    .add_systems(Update, move_capture_bubbles.in_set(CaptureSystems::Motion))
    .add_systems(Last, capture_frames);
    if std::env::var_os("SPACE_SCUFFLE_TRACE_MIST_ORDER").is_some() {
        app.sub_app_mut(RenderApp).add_systems(
            Render,
            trace_mist_phase_order
                .in_set(bevy::render::RenderSet::PhaseSort)
                .after(
                    bevy::render::render_phase::sort_phase_system::<
                        bevy::core_pipeline::core_3d::Transparent3d,
                    >,
                ),
        );
    }
    if let Some(mode) = view_light_case {
        super::view_light_capture::install(&mut app, root.as_path(), mode)?;
    }
    if benchmarking {
        benchmark_capture::install(&mut app);
    }
    if verify_owner_reset {
        owner_reset::install(&mut app);
    }
    let mist_shader = app
        .world()
        .resource::<AssetServer>()
        .load("shaders/mist_wake.wgsl");
    let bubble_shader = app
        .world()
        .resource::<AssetServer>()
        .load("shaders/bubble_dome.wgsl");
    let status = PipelineStatus {
        ready: Arc::new(AtomicBool::new(false)),
        failed: Arc::new(AtomicBool::new(false)),
        mist_shader,
        bubble_shader,
        require_mist,
    };
    let output = super::output_evidence::OutputEvidence::new(capture_hdr, require_mist);
    app.insert_resource(output.clone());
    app.insert_resource(status.clone());
    app.sub_app_mut(RenderApp)
        .insert_resource(status)
        .insert_resource(output)
        .add_systems(Render, inspect_pipelines);
    let exit = crate::render_compatibility::run_render_app(&mut app);
    Ok(completion.finish(exit)?)
}

fn inspect_pipelines(
    cache: Res<'_, PipelineCache>,
    status: Res<'_, PipelineStatus>,
    output: Res<'_, super::output_evidence::OutputEvidence>,
) {
    let mut pending = cache.waiting_pipelines().next().is_some();
    let mut mist_ready = false;
    let mut bubble_ready = false;
    for pipeline in cache.pipelines() {
        match &pipeline.state {
            CachedPipelineState::Err(
                PipelineCacheError::ShaderNotLoaded(_)
                | PipelineCacheError::ShaderImportNotYetAvailable,
            )
            | CachedPipelineState::Queued
            | CachedPipelineState::Creating(_) => pending = true,
            CachedPipelineState::Err(error) => {
                error!("TERMINAL PIPELINE FAILURE: {error}");
                status.failed.store(true, Ordering::Release);
            }
            CachedPipelineState::Ok(_) => {
                if let PipelineDescriptor::RenderPipelineDescriptor(descriptor) =
                    &pipeline.descriptor
                {
                    if descriptor
                        .fragment
                        .as_ref()
                        .is_some_and(|fragment| fragment.shader.id() == status.mist_shader.id())
                    {
                        if let Err(error) = output.observe(descriptor) {
                            error!("{error}");
                            status.failed.store(true, Ordering::Release);
                            return;
                        }
                        mist_ready = true;
                    }
                    if descriptor
                        .fragment
                        .as_ref()
                        .is_some_and(|fragment| fragment.shader.id() == status.bubble_shader.id())
                    {
                        bubble_ready = true;
                    }
                }
            }
        }
    }
    status.ready.store(
        !pending && bubble_ready && (!status.require_mist || mist_ready),
        Ordering::Release,
    );
}

#[allow(clippy::too_many_arguments)]
fn setup_capture_scene(
    mut commands: Commands<'_, '_>,
    options: Res<'_, CaptureOptions>,
    terrain: Res<'_, CaptureTerrain>,
    root: Res<'_, ResolvedAssetsRoot>,
    catalog: Res<'_, CatalogResource>,
    registry: Res<'_, FlipbookRegistry>,
    mut media_status: ResMut<'_, crate::media::ClientMediaStatus>,
    shared: Res<'_, BubbleSharedMeshes>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut terrain_materials: ResMut<'_, Assets<crate::terrain::TerrainMaterial>>,
    mut standard: ResMut<'_, Assets<StandardMaterial>>,
    mut images: ResMut<'_, Assets<Image>>,
    mut bubbles: ResMut<'_, Assets<BubbleDomeMaterial>>,
    pool: Option<Res<'_, MistPool>>,
    mut presentation: crate::player_presentation::PresentationContext<'_>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if let Err(error) = crate::terrain::spawn_terrain(
        &mut commands,
        &mut meshes,
        &mut terrain_materials,
        &mut standard,
        &mut images,
        &terrain.theme,
        terrain.map.clone(),
        root.as_path(),
    ) {
        error!("Capture real terrain failed: {error}");
        exit.send(AppExit::error());
        return;
    }
    let available: Vec<_> = catalog
        .0
        .archetypes()
        .filter(|a| {
            let eligible = if options.shell_case() {
                match options.shell_shape {
                    ShellShape::Sphere => a.showcase_hero && a.shape == ShapeKind::UnitSphere,
                    ShellShape::Dome => matches!(a.shape, ShapeKind::BuildingDome { .. }),
                }
            } else {
                a.showcase_hero && a.shape == ShapeKind::UnitSphere
            };
            eligible && registry.textures_by_id.contains_key(&a.unit_type_id)
        })
        .collect();
    if available.is_empty() {
        error!("No real showcase unit textures loaded");
        exit.send(AppExit::error());
        return;
    }
    let animation = if options.shell_case() {
        AnimState::Idle
    } else {
        AnimState::Walk
    };
    for arch in &available {
        if let Err(error) = media_status.require(&registry, arch.unit_type_id, animation) {
            error!("Capture media admission failed: {error}");
            media_status.record_failure(error);
            exit.send(AppExit::error());
            return;
        }
    }
    let count = match options.case {
        CaptureCase::Crowd => 400,
        CaptureCase::Gameplay | CaptureCase::ShellGameplay => 9,
        _ => 1,
    };
    let columns = if count == 400 { 20 } else { 3 };
    let shot_radius = if options.shell_case() {
        available[0].collision_radius.to_f32() * options.radius_scale
    } else if options.benchmark_emitters.is_some() {
        available
            .iter()
            .map(|arch| arch.collision_radius.to_f32())
            .fold(0.0_f32, f32::max)
    } else {
        0.85
    };
    for index in 0..count {
        let arch = available[index % available.len()];
        let (x, z) = if count == 1 {
            (12.0, 12.0)
        } else if count == 400 {
            (
                4.0 + (index % columns) as f32 * 2.5,
                4.0 + (index / columns) as f32 * 2.5,
            )
        } else {
            (
                10.0 + (index % columns) as f32 * 4.0,
                8.0 + (index / columns) as f32 * 4.0,
            )
        };
        let radius = if options.shell_case() {
            arch.collision_radius.to_f32() * options.radius_scale
        } else if options.benchmark_emitters.is_some() {
            arch.collision_radius.to_f32()
        } else {
            0.85
        };
        let ground = terrain
            .map
            .terrain_altitude_scalar(sim_types::SimVec2::new(
                sim_types::SimScalar::from_num(x),
                sim_types::SimScalar::from_num(z),
            ))
            .to_f32();
        let center_height = if arch.shape == ShapeKind::UnitSphere {
            radius
        } else {
            0.0
        };
        let position = Vec3::new(x, ground + center_height, z);
        let instance = BubbleRenderInstance {
            entity_id: EntityId(index as u32),
            shape: arch.shape,
            silhouette: arch.silhouette,
            unit_type: arch.unit_type_id,
            owner: presentation.palette.primary(),
            world_pos: [x, z, ground],
            ground_z: ground,
            radius,
            velocity: [2.5, 0.0],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: false,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: animation,
            anim_progress: 0.0,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: false,
            cloaked: false,
            speed_boosted: options.case == CaptureCase::Boost,
        };
        let style = match presentation.palette.preview_style(instance.owner) {
            Ok(style) => style,
            Err(error) => {
                presentation.reject(error, &mut exit);
                return;
            }
        };
        let material = bubbles.add(BubbleDomeMaterial {
            uniform: BubbleUniformGpu::from_instance(
                &instance,
                registry.get_meta(arch.unit_type_id),
                style,
            ),
            video_texture: registry.get_texture(arch.unit_type_id),
            alpha_mode: AlphaMode::Blend,
        });
        commands.spawn((
            Mesh3d(if arch.shape == ShapeKind::UnitSphere {
                shared.sphere_mesh.clone()
            } else {
                shared.dome_mesh.clone()
            }),
            MeshMaterial3d(material),
            Transform::from_translation(position).with_scale(Vec3::splat(radius)),
            BubbleInstance(instance),
            CaptureBubble {
                origin: position,
                index,
            },
            MistMotion {
                conveyor_velocity: Some(Vec3::X * 2.5),
                enabled: !options.shell_case()
                    && options
                        .benchmark_emitters
                        .map_or(true, |count| index < usize::from(count)),
            },
        ));
    }
    let shot_ground = terrain
        .map
        .terrain_altitude_scalar(sim_types::SimVec2::new(
            sim_types::SimScalar::from_i32(12),
            sim_types::SimScalar::from_i32(12),
        ))
        .to_f32();
    let close_target = if options.shell_case() {
        let height = if options.shell_shape == ShellShape::Sphere {
            shot_radius
        } else {
            shot_radius * 0.4
        };
        Vec3::new(12.0, shot_ground + height, 12.0)
    } else {
        Vec3::new(10.6, shot_ground + 0.8, 12.0)
    };
    let (eye, target) = match options.case {
        CaptureCase::Crowd => (Vec3::new(32.0, 64.0, 69.0), Vec3::new(27.0, 3.0, 27.0)),
        CaptureCase::Gameplay | CaptureCase::ShellGameplay => (
            Vec3::new(20.0, shot_ground + 19.0, 31.0),
            Vec3::new(13.0, shot_ground + 0.5, 12.0),
        ),
        _ => (
            close_target + Vec3::new(1.0, 3.5, 6.5) * (shot_radius / 0.85),
            close_target,
        ),
    };
    let camera_transform = if options.case == CaptureCase::Crowd {
        let (lowest_ground, highest_ground) = terrain
            .map
            .cells()
            .iter()
            .map(|cell| cell.cliff_level.height_f32())
            .fold((0.0_f32, 0.0_f32), |(minimum, maximum), height| {
                (minimum.min(height), maximum.max(height))
            });
        let Some(pool) = pool else {
            error!("Capture mist pool was not initialized");
            exit.send(AppExit::error());
            return;
        };
        // Circle histories remain inside their radius regardless of age. Straight
        // capture paths remain inside speed times the exact final capture time.
        let motion_extent = if options.benchmark_emitters.is_some() {
            1.0
        } else {
            2.5 * (14.0 / 6.0)
        };
        let camera_fit = super::benchmark::crowd_camera_bounds(
            Vec2::splat(4.0),
            Vec2::splat(51.5),
            lowest_ground,
            highest_ground,
            shot_radius,
            super::emission::maximum_origin_offset(shot_radius)
                + super::ribbon::maximum_sample_excursion(shot_radius, &pool.style),
            motion_extent,
        )
        .and_then(|(minimum, maximum)| {
            super::benchmark::fit_camera(
                minimum,
                maximum,
                1280.0 / 720.0,
                std::f32::consts::FRAC_PI_4,
            )
        });
        match camera_fit {
            Ok(transform) => transform,
            Err(error) => {
                error!("{error}");
                exit.send(AppExit::error());
                return;
            }
        }
    } else {
        Transform::from_translation(eye).looking_at(target, Vec3::Y)
    };
    commands.spawn((
        Camera3d::default(),
        Camera {
            hdr: options.hdr,
            ..default()
        },
        camera_transform,
        if options.msaa == 4 {
            Msaa::Sample4
        } else {
            Msaa::Off
        },
    ));
}

// The explicit query tuple makes the production components used by this fixture reviewable.
#[allow(clippy::type_complexity)]
fn move_capture_bubbles(
    time: Res<'_, Time>,
    terrain: Res<'_, CaptureTerrain>,
    options: Res<'_, CaptureOptions>,
    mut progress: ResMut<'_, CaptureProgress>,
    mut query: Query<
        '_,
        '_,
        (
            &mut MistMotion,
            &mut BubbleInstance,
            &MeshMaterial3d<BubbleDomeMaterial>,
            &CaptureBubble,
            &mut Transform,
        ),
        With<CaptureBubble>,
    >,
    registry: Res<'_, FlipbookRegistry>,
    mut materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    benchmark: Option<Res<'_, benchmark_capture::Recorder>>,
    mut presentation: crate::player_presentation::PresentationContext<'_>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if progress.running
        && !progress.pending
        && time.delta_secs() > 0.0
        && !benchmark
            .as_ref()
            .is_some_and(|recorder| recorder.complete())
    {
        progress.elapsed_frames += 1;
    }
    let seconds = if progress.running {
        progress.elapsed_frames as f32 / options.fps as f32
    } else {
        time.elapsed_secs()
    };
    for (mut motion, mut bubble, handle, initial, mut transform) in &mut query {
        motion.conveyor_velocity = Some(
            if options.case == CaptureCase::Stationary && progress.running {
                Vec3::ZERO
            } else if options.case == CaptureCase::Turn && seconds >= 1.4 {
                Vec3::Z * 2.5
            } else if options.case == CaptureCase::Stop && seconds >= 1.5 {
                Vec3::ZERO
            } else {
                Vec3::X * 2.5
            },
        );
        if matches!(options.case, CaptureCase::Gameplay | CaptureCase::Crowd) {
            motion.conveyor_velocity = None;
            let direction = match initial.index % 4 {
                0 => Vec3::X,
                1 => Vec3::Z,
                2 => -Vec3::X,
                _ => -Vec3::Z,
            };
            let (position, direction) = if options.benchmark_emitters.is_some() {
                let sign = if initial.index % 2 == 0 { 1.0 } else { -1.0 };
                let angle =
                    seconds * 2.5 * sign + (initial.index % 4) as f32 * std::f32::consts::FRAC_PI_2;
                let (sin, cos) = angle.sin_cos();
                (
                    initial.origin + Vec3::new(cos, 0.0, sin),
                    Vec3::new(-sin * sign, 0.0, cos * sign),
                )
            } else {
                (initial.origin + direction * seconds * 2.5, direction)
            };
            let ground = terrain
                .map
                .terrain_altitude_scalar(sim_types::SimVec2::new(
                    sim_types::SimScalar::from_num(position.x),
                    sim_types::SimScalar::from_num(position.z),
                ))
                .to_f32();
            transform.translation = Vec3::new(position.x, ground + bubble.0.radius, position.z);
            bubble.0.world_pos = [position.x, position.z, ground];
            bubble.0.ground_z = ground;
            bubble.0.heading_angle_rad = direction.z.atan2(direction.x);
        }
        if options.shell_case() {
            motion.enabled = false;
            bubble.0.anim_state = AnimState::Idle;
            bubble.0.anim_progress = 0.0;
            bubble.0.selected = options.shell_state == ShellState::Selected;
            bubble.0.is_attacking = options.shell_state == ShellState::Attacking;
            // Aim the hit lobe toward the camera quadrant, independent of unit identity.
            bubble.0.hull_hit_angle_rad = 1.2;
            bubble.0.aim_angle_rad = 1.2;
            bubble.0.hp_ratio = if options.shell_state == ShellState::Damaged {
                0.2
            } else {
                1.0
            };
            let recovery = (1.0 - (seconds - 1.5).max(0.0)).clamp(0.0, 1.0);
            bubble.0.shield_hit_flash = match options.shell_state {
                ShellState::ShieldHit => 1.0,
                ShellState::Recovering => recovery,
                _ => 0.0,
            };
            bubble.0.hull_hit_flash = match options.shell_state {
                ShellState::HullHit => 1.0,
                ShellState::Recovering => recovery,
                _ => 0.0,
            };
        } else {
            bubble.0.anim_progress = (seconds * 1.5).fract();
        }
        let style = match presentation.palette.preview_style(bubble.0.owner) {
            Ok(style) => style,
            Err(error) => {
                presentation.reject(error, &mut exit);
                return;
            }
        };
        if let Some(material) = materials.get_mut(&handle.0) {
            material.uniform = BubbleUniformGpu::from_instance(
                &bubble.0,
                registry.get_meta(bubble.0.unit_type),
                style,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn capture_frames(
    mut commands: Commands<'_, '_>,
    options: Res<'_, CaptureOptions>,
    mut progress: ResMut<'_, CaptureProgress>,
    status: Res<'_, PipelineStatus>,
    stats: Res<'_, MistStats>,
    mut pool: ResMut<'_, MistPool>,
    mut clock: ResMut<'_, TimeUpdateStrategy>,
    mut virtual_time: ResMut<'_, Time<Virtual>>,
    mut presentation_time: ResMut<'_, Time>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform)>,
    bubbles: Query<'_, '_, (&BubbleInstance, &GlobalTransform)>,
    mut exit: EventWriter<'_, AppExit>,
    benchmark: Option<Res<'_, benchmark_capture::Recorder>>,
    reset_audit: Option<Res<'_, owner_reset::ResetAudit>>,
    ribbons: Query<
        '_,
        '_,
        (
            &bevy::render::primitives::Aabb,
            &GlobalTransform,
            &Visibility,
        ),
        With<super::MistRibbon>,
    >,
    view_lights: Option<Res<'_, super::view_light_capture::LightCapture>>,
) {
    progress.frames += 1;
    if view_lights.as_ref().is_some_and(|state| state.failed()) {
        exit.send(AppExit::error());
        return;
    }
    if let Some(audit) = &reset_audit {
        if let Err(error) = audit.failed() {
            error!("{error}");
            exit.send(AppExit::error());
            return;
        }
    }
    if status.failed.load(Ordering::Acquire)
        || (benchmark.is_none() && progress.frames > options.fps * 30)
    {
        error!("Capture failed or timed out waiting for actual rendering");
        exit.send(AppExit::error());
        return;
    }
    if progress.complete {
        if let Some(state) = &view_lights {
            if let Err(error) = state.finish(&options.out) {
                error!("{error}");
                exit.send(AppExit::error());
                return;
            }
        }
        if let Some(audit) = &reset_audit {
            if let Err(error) = audit.finish(&options.out) {
                error!("{error}");
                exit.send(AppExit::error());
                return;
            }
        }
        exit.send(AppExit::Success);
        return;
    }
    if !progress.running {
        if status.ready.load(Ordering::Acquire) && (!status.require_mist || stats.visible > 0) {
            progress.running = true;
            // GPU loading takes a machine-dependent number of warmup frames. Reset both
            // virtual and generic render time so Globals.time starts at the same epoch.
            *virtual_time = Time::<Virtual>::default();
            *presentation_time = virtual_time.as_generic();
            pool.clock = 0.0;
            for slot in &mut pool.slots {
                slot.history.reset();
            }
        }
        return;
    }
    if progress.pending {
        return;
    }
    if benchmark
        .as_ref()
        .is_some_and(|recorder| !recorder.complete())
    {
        return;
    }
    *clock = TimeUpdateStrategy::ManualDuration(
        if benchmark.is_some()
            || (options.verify_owner_reset && progress.image == 1)
            || (options.verify_view_lights.is_some() && progress.elapsed_frames >= options.fps * 2)
        {
            // The empty-history checkpoint must stay at the reset pose even if a
            // real pipeline temporarily becomes unavailable before readback starts.
            Duration::ZERO
        } else {
            Duration::from_secs_f64(1.0 / f64::from(options.fps))
        },
    );
    // Sixth-second targets are exact at every supported rate (30, 60, 144 Hz).
    // Movement advances this counter in Update; Last captures that same rendered pose.
    let target = if options.verify_view_lights.is_some() {
        options.fps * 2
    } else if options.verify_owner_reset {
        owner_reset::target_frame(progress.image, options.fps)
    } else {
        capture_target_frame(options.case, progress.image, options.fps)
    };
    if progress.elapsed_frames < target {
        return;
    }
    if !status.ready.load(Ordering::Acquire)
        || view_lights
            .as_ref()
            .is_some_and(|state| !state.ready(progress.image))
    {
        return;
    }
    if view_lights.is_some() {
        for (camera, camera_transform) in &cameras {
            let verified = (|| -> Result<(), super::benchmark::BenchmarkError> {
                for (bubble, transform) in &bubbles {
                    let radius = Vec3::splat(bubble.0.radius);
                    benchmark_capture::verify_bounds(
                        camera,
                        camera_transform,
                        transform.translation() - radius,
                        transform.translation() + radius,
                        Mat4::IDENTITY,
                    )?;
                }
                for (bounds, transform, visibility) in &ribbons {
                    if *visibility == Visibility::Visible {
                        benchmark_capture::verify_ribbon_bounds(
                            camera,
                            camera_transform,
                            bounds,
                            transform,
                        )?;
                    }
                }
                Ok(())
            })();
            if let Err(error) = verified {
                error!("Actual light-fixture frustum failed: {error}");
                exit.send(AppExit::error());
                return;
            }
        }
    }
    let frustum = if options.case == CaptureCase::Crowd {
        let verify =
            (|| -> Result<benchmark_capture::FrustumEvidence, super::benchmark::BenchmarkError> {
                let (camera, camera_transform) = cameras
                    .get_single()
                    .map_err(|_| super::benchmark::BenchmarkError::InvalidCamera)?;
                let mut actors = 0;
                let mut active_ribbons = 0;
                for (bubble, transform) in &bubbles {
                    let radius = Vec3::splat(bubble.0.radius);
                    benchmark_capture::verify_bounds(
                        camera,
                        camera_transform,
                        transform.translation() - radius,
                        transform.translation() + radius,
                        Mat4::IDENTITY,
                    )?;
                    actors += 1;
                }
                for (bounds, transform, visibility) in &ribbons {
                    if *visibility == Visibility::Visible {
                        benchmark_capture::verify_ribbon_bounds(
                            camera,
                            camera_transform,
                            bounds,
                            transform,
                        )?;
                        active_ribbons += 1;
                    }
                }
                if actors != 400 {
                    return Err(super::benchmark::BenchmarkError::InvalidCamera);
                }
                Ok(benchmark_capture::FrustumEvidence {
                    actors,
                    active_ribbons,
                })
            })();
        match verify {
            Ok(evidence) => Some(evidence),
            Err(error) => {
                error!("Crowd frustum validation failed: {error}");
                exit.send(AppExit::error());
                return;
            }
        }
    } else {
        None
    };
    let path = options.out.join(format!(
        "{:?}-{}-fps{}-msaa{}.png",
        options.case, progress.image, options.fps, options.msaa
    ));
    let measurements = if options.shell_case() {
        let Ok((camera, transform)) = cameras.get_single() else {
            error!("optical capture requires exactly one camera");
            exit.send(AppExit::error());
            return;
        };
        Some((
            camera.clone(),
            *transform,
            bubbles
                .iter()
                .map(|(bubble, transform)| {
                    (transform.translation(), bubble.0.radius, bubble.0.shape)
                })
                .collect::<Vec<_>>(),
        ))
    } else {
        None
    };
    if let Some(audit) = &reset_audit {
        if let Err(error) = audit.checkpoint(progress.image, pool.sequence) {
            error!("{error}");
            exit.send(AppExit::error());
            return;
        }
    }
    let presentation_seconds = presentation_time.elapsed_secs_f64();
    let image_index = progress.image;
    progress.pending = true;
    *clock = TimeUpdateStrategy::ManualDuration(Duration::ZERO);
    commands.spawn(Screenshot::primary_window()).observe(move |trigger: Trigger<'_, ScreenshotCaptured>, mut progress: ResMut<'_, CaptureProgress>, options: Res<'_, CaptureOptions>, stats: Res<'_, MistStats>, completion: Res<'_, CaptureCompletion>, audio: Res<'_, crate::presentation::AudioPolicy>, output: Res<'_, super::output_evidence::OutputEvidence>, mut exit: EventWriter<'_, AppExit>| {
        let save_result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let dynamic = trigger.event().0.clone().try_into_dynamic()?;
            if dynamic.width() != 1280 || dynamic.height() != 720 { return Err("unexpected screenshot dimensions".into()); }
            let pixels = dynamic.to_rgb8();
            pixels.save(&path)?;
            output.write(&options.out)?;
            if let Some((camera, transform, shells)) = &measurements {
                let values = super::shell_metrics::measure(&pixels, camera, transform, shells)?;
                std::fs::write(path.with_extension("csv"), values)?;
            }
            std::fs::write(options.out.join("metrics.txt"), format!("audio_policy={:?}\ncase={:?}\nfps={}\nmsaa={}\nframe={}\nsample_seconds={:.9}\nvirtual_seconds={:.9}\nhdr={}\nshell_state={:?}\nshell_shape={:?}\nradius_scale={}\ncapacity={}\noccupied={}\nactive_history_ribbons={}\nsamples={}\noverflows={}\nfrustum_evidence={:?}\n", *audio, options.case, options.fps, options.msaa, progress.elapsed_frames, f64::from(progress.elapsed_frames) / f64::from(options.fps), presentation_seconds, options.hdr, options.shell_state, options.shell_shape, options.radius_scale, stats.capacity, stats.occupied, stats.visible, stats.samples, stats.overflows, frustum.as_ref().map(|evidence| (evidence.actors, evidence.active_ribbons))))?;
            Ok(())
        })();
        if let Err(error) = save_result { error!("Screenshot failed: {error}"); exit.send(AppExit::error()); return; }
        if let Err(error) = completion.acknowledge(image_index) { error!("{error}"); exit.send(AppExit::error()); return; }
        info!("ACTUAL CAPTURE COMPLETE: {}", path.display());
        progress.image += 1; progress.pending = false; progress.complete = progress.image == 3;
    });
}

fn capture_target_frame(case: CaptureCase, image: usize, fps: u32) -> u32 {
    let sixths = if case == CaptureCase::Stop {
        [10, 15, 20]
    } else {
        [10, 12, 14]
    };
    sixths[image.min(2)] * (fps / 6)
}

// Diagnostic-only observer: at most sixteen actual sorted overlap phases, never a benchmark.
fn trace_mist_phase_order(
    phases: Res<
        '_,
        bevy::render::render_phase::ViewSortedRenderPhases<
            bevy::core_pipeline::core_3d::Transparent3d,
        >,
    >,
    mist: Res<'_, bevy::pbr::RenderMaterialInstances<super::MistWakeMaterial>>,
    shells: Res<'_, bevy::pbr::RenderMaterialInstances<BubbleDomeMaterial>>,
    mut reported: Local<'_, u32>,
) {
    use bevy::render::render_phase::PhaseItem;
    if *reported >= 16 {
        return;
    }
    for (view, phase) in phases.iter() {
        if !phase
            .items
            .iter()
            .any(|item| mist.0.contains_key(&item.main_entity()))
        {
            continue;
        }
        for (index, item) in phase.items.iter().enumerate() {
            let class = if mist.0.contains_key(&item.main_entity()) {
                "mist"
            } else if shells.0.contains_key(&item.main_entity()) {
                "shell"
            } else {
                "other"
            };
            info!("MIST_PHASE_ORDER sample={} view={:?} index={} class={} entity={:?} distance={} bits={:08x}",
                *reported, view, index, class, item.main_entity(), item.distance, item.distance.to_bits());
        }
        *reported += 1;
        if *reported >= 16 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{capture_target_frame, CaptureCase};

    #[test]
    fn capture_times_match_exactly_at_all_supported_rates() {
        for case in [CaptureCase::Close, CaptureCase::Stop] {
            for image in 0..3 {
                let reference = capture_target_frame(case, image, 30);
                for fps in [60, 144] {
                    assert_eq!(capture_target_frame(case, image, fps) * 30, reference * fps);
                }
            }
        }
        assert!(capture_target_frame(CaptureCase::Stop, 2, 60) > 171);
    }
}
