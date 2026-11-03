//! Construction fill of a dome: shipped mesh, compiled shader, uniform packing and real pixels.
//!
//! # Responsibility
//! Owns the proof of remediation finding C-11 and step 5.8: a foundation fills from rim to apex in
//! proportion to its construction, and a finished building shows none of this (decision D-11).
//!
//! # How it fits
//! The pixel test re-executes this test binary on the virtual GPU. Its child is the real graphical
//! client in Visual Lab mode, so the catalog, the media loader, the shared meshes, the material
//! pipeline and the shipped WGSL are all production code. Only the staging is local: the Lab's own
//! actors change media frame with time, which would hide the fill, and no live command can raise a
//! foundation yet. So the child spawns one dome and one unit sphere itself and writes their
//! uniforms through `BubbleUniformGpu::from_instance` and `sync_bubble_material`, the path a live
//! bubble takes. `lab_scene_tests.rs` covers the Lab's slider up to the uniform.
//!
//! # Invariants verified
//! - The shared dome mesh is a full sphere with its poles on Z, so `uv.y` cannot express height.
//! - The fragment entry point takes the fill height from `construction_height`, never from `uv.y`.
//! - Only `AnimState::Constructing` reaches the fill branch, so a queue never paints a dome.
//! - On screen, a surface height above the progress shows the unbuilt grid and a height below it
//!   shows the finished media, for domes and for unit spheres alike.

#![allow(clippy::expect_used, clippy::panic)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;
#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/shader_ir.rs"]
mod shader_ir;

use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use client::bubble_shader::{
    sync_bubble_material, BubbleSharedMeshes, BubbleUniformGpu, FlipbookRegistry,
};
use client::player_presentation::PlayerPalette;
use client::runtime_battle::{BattleDefinition, BattleRenderStatus, DEFAULT_BATTLE_SCENARIO};
use client::{BubbleDomeMaterial, CatalogResource, ClientMode, RtsCamera};
use naga::ShaderStage;
use serde::{Deserialize, Serialize};
use shader_ir::production_module;
use sim_core::catalog::UnitArchetype;
use sim_types::enums::{AnimState, ShapeKind, WeaponVisualKind};
use sim_types::ids::{EntityId, PlayerId};
use sim_types::snapshot::BubbleRenderInstance;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying the child's output directory.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_DOME_CHILD_OUT";
/// Name of the libtest entry that the pixel test launches as its child.
const CHILD_TEST: &str = "windowed_dome_child";
/// File the child writes the screen position of each sampled surface band to.
const BANDS_FILE: &str = "bands.ron";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// Consecutive ready frames between writing a uniform and capturing it.
const SETTLE_FRAMES: u32 = 8;

/// Identifier of a shot taken with the fill complete, which every other shot is compared with.
const FINISHED: &str = "finished";
/// Identifier of the shot of a foundation at [`EARLY_PROGRESS`].
const EARLY: &str = "early";
/// Identifier of the shot of a foundation at [`LATE_PROGRESS`].
const LATE: &str = "late";
/// Identifier of the shot of a finished building whose queue is at [`EARLY_PROGRESS`].
const PRODUCING: &str = "producing";

/// Construction share of the [`EARLY`] shot.
const EARLY_PROGRESS: f32 = 0.2;
/// Construction share of the [`LATE`] shot.
const LATE_PROGRESS: f32 = 0.6;

/// Every shot the child takes, with the animation state and progress it stages.
const SHOTS: [(&str, AnimState, f32); 4] = [
    (FINISHED, AnimState::Constructing, 1.0),
    (EARLY, AnimState::Constructing, EARLY_PROGRESS),
    (LATE, AnimState::Constructing, LATE_PROGRESS),
    (PRODUCING, AnimState::Idle, EARLY_PROGRESS),
];

/// Shares of a dome's height to sample. The top of the fill band stays clear of the apex, where
/// the [`FINISHED`] shot itself carries the bright scan line.
const DOME_SHARES: [f32; 3] = [0.1, 0.4, 0.8];
/// Shares of a sphere's height to sample. A camera above the ground sees little of a sphere's
/// lower half, so these start at its equator.
const SPHERE_SHARES: [f32; 2] = [0.45, 0.8];
/// Smallest cosine between surface normal and view that is sampled. Grazing pixels are dominated
/// by the rim, which is the same in every shot.
const MIN_VIEW_COSINE: f32 = 0.4;
/// Step between sampled azimuths, in degrees.
const AZIMUTH_STEP_DEGREES: usize = 3;

/// Half the height of the bright scan line, plus room for the rasterized ring. A ring nearer the
/// progress than this is neither built nor unbuilt.
const SCAN_LINE_MARGIN: f32 = 0.05;
/// Largest mean difference from the finished shot of a built ring, in levels of 255. Measured on
/// lavapipe: at most 0.02, and 24 for the built rings the old `uv.y` fill left unbuilt.
const BUILT_LIMIT: f32 = 2.0;
/// Smallest mean difference from the finished shot of an unbuilt ring, in levels of 255. Measured
/// on lavapipe: at least 56, and at most 2.2 for the unbuilt rings the old fill drew as built.
const UNBUILT_FLOOR: f32 = 10.0;

/// The warning windowed runs logged on lavapipe before Mesa 25. It concerns the swapchain, not this
/// shader, so it gives this test no reason to fail.
const MISSING_COLORSPACE_EXTENSION: gpu_run::AllowedValidationId = gpu_run::AllowedValidationId {
    id: "Unable to find extension: VK_EXT_swapchain_colorspace",
    reason: "raised by wgpu_hal at swapchain setup; unrelated to the bubble material",
};

/// One shape on the stage.
#[derive(Debug, Clone, Copy)]
struct Subject {
    /// Name used in failure messages and in the band file.
    name: &'static str,
    /// Whether this is the dome, otherwise it is the unit sphere.
    dome: bool,
    /// World position of the mesh origin.
    centre: Vec3,
    /// Shell radius in world units.
    radius: f32,
}

/// Where the shapes stand: behind the Lab lineup, at the sizes it gives them.
const SUBJECTS: [Subject; 2] = [
    Subject {
        name: "dome",
        dome: true,
        centre: Vec3::new(2.4, 0.0, -3.5),
        radius: 1.35,
    },
    Subject {
        name: "sphere",
        dome: false,
        centre: Vec3::new(-2.4, 0.85, -3.5),
        radius: 0.85,
    },
];

impl Subject {
    /// Shares of this subject's height to sample.
    fn shares(self) -> &'static [f32] {
        if self.dome {
            &DOME_SHARES
        } else {
            &SPHERE_SHARES
        }
    }

    /// Height of a surface point above the centre, in radii, for a share of the fillable height.
    ///
    /// A dome stands on its centre, so only its upper half counts.
    fn normal_height(self, share: f32) -> f32 {
        if self.dome {
            share
        } else {
            2.0 * share - 1.0
        }
    }
}

/// Screen pixels of a horizontal ring on one subject's surface.
#[derive(Debug, Serialize, Deserialize)]
struct Band {
    /// [`Subject::name`].
    subject: String,
    /// Share of the fillable height this ring stands at.
    share: f32,
    /// Physical window pixels `(x, y)` of the camera-facing part of the ring.
    pixels: Vec<(u32, u32)>,
}

/// Progress of the child through [`SHOTS`].
#[derive(Resource, Default)]
struct Stage {
    /// Output directory.
    directory: PathBuf,
    /// Material handle and template instance of each entry of [`SUBJECTS`].
    staged: Vec<(Handle<BubbleDomeMaterial>, BubbleRenderInstance)>,
    /// Index into [`SHOTS`].
    shot: usize,
    /// Ready frames rendered since the shot's uniforms were written.
    settled_frames: u32,
    /// Whether a screenshot was requested and has not arrived.
    capture_pending: bool,
}

/// Builds the snapshot record of an idle bubble of `archetype` standing at `subject`.
fn template_instance(archetype: &UnitArchetype, subject: &Subject) -> BubbleRenderInstance {
    BubbleRenderInstance {
        entity_id: EntityId(1),
        shape: archetype.shape,
        silhouette: archetype.silhouette,
        unit_type: archetype.unit_type_id,
        owner: PlayerId(0),
        world_pos: [subject.centre.x, subject.centre.z, subject.centre.y],
        ground_z: 0.0,
        radius: subject.radius,
        velocity: [0.0, 0.0],
        heading_angle_rad: 0.0,
        aim_angle_rad: 0.0,
        is_attacking: false,
        weapon_visual: WeaponVisualKind::Hitscan,
        attack_target_pos: None,
        anim_state: AnimState::Idle,
        anim_progress: 0.5,
        hp_ratio: 1.0,
        shield_ratio: 1.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: None,
        selected: false,
        cloaked: false,
        speed_boosted: false,
    }
}

/// Returns the first archetype of the wanted shape that has shipped media.
fn presentable<'a>(
    catalog: &'a CatalogResource,
    registry: &FlipbookRegistry,
    dome: bool,
) -> &'a UnitArchetype {
    catalog
        .0
        .archetypes()
        .find(|archetype| {
            matches!(archetype.shape, ShapeKind::BuildingDome { .. }) == dome
                && registry.get_texture(archetype.unit_type_id).is_some()
                && registry.get_meta(archetype.unit_type_id).is_some()
        })
        .unwrap_or_else(|| panic!("the catalog must have a presentable shape (dome: {dome})"))
}

/// Spawns one bubble per [`SUBJECTS`] entry, through the shared meshes and the real media.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn spawn_subjects(
    mut commands: Commands<'_, '_>,
    mut stage: ResMut<'_, Stage>,
    mut materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    shared: Option<Res<'_, BubbleSharedMeshes>>,
    registry: Option<Res<'_, FlipbookRegistry>>,
    catalog: Res<'_, CatalogResource>,
) {
    let (Some(shared), Some(registry), true) = (shared, registry, stage.staged.is_empty()) else {
        return;
    };
    for subject in &SUBJECTS {
        let archetype = presentable(&catalog, &registry, subject.dome);
        let material = materials.add(BubbleDomeMaterial {
            uniform: BubbleUniformGpu::default(),
            video_texture: registry.get_texture(archetype.unit_type_id),
            alpha_mode: AlphaMode::Blend,
        });
        let mesh = if subject.dome {
            shared.dome_mesh.clone()
        } else {
            shared.sphere_mesh.clone()
        };
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(subject.centre).with_scale(Vec3::splat(subject.radius)),
        ));
        stage
            .staged
            .push((material, template_instance(archetype, subject)));
    }
}

/// Packs `instance` as `state` at `progress`, keeping one media frame across all shots.
///
/// The frame of the first shot is kept so that any pixel difference between shots is the fill.
fn shot_uniform(
    instance: &BubbleRenderInstance,
    state: AnimState,
    progress: f32,
    registry: &FlipbookRegistry,
    palette: &PlayerPalette,
) -> BubbleUniformGpu {
    let meta = registry.get_meta(instance.unit_type);
    let style = palette
        .preview_style(instance.owner)
        .expect("invariant: player 0 is in the shipped palette");
    let frame = BubbleUniformGpu::from_instance(
        &BubbleRenderInstance {
            anim_state: SHOTS[0].1,
            ..instance.clone()
        },
        meta,
        style,
    )
    .frame_index;
    BubbleUniformGpu {
        frame_index: frame,
        ..BubbleUniformGpu::from_instance(
            &BubbleRenderInstance {
                anim_state: state,
                build_or_queue_progress: Some(progress),
                ..instance.clone()
            },
            meta,
            style,
        )
    }
}

/// Writes each shot's uniforms, waits for the renderer, and requests one screenshot per shot.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
fn run_shots(
    mut commands: Commands<'_, '_>,
    mut stage: ResMut<'_, Stage>,
    mut materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    registry: Option<Res<'_, FlipbookRegistry>>,
    palette: Res<'_, PlayerPalette>,
    status: Res<'_, BattleRenderStatus>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform), With<RtsCamera>>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let Some(registry) = registry else {
        return;
    };
    if stage.staged.is_empty() || stage.capture_pending {
        return;
    }
    let Some(&(name, state, progress)) = SHOTS.get(stage.shot) else {
        let (camera, transform) = cameras.single();
        let bands = screen_bands(camera, transform);
        let text = ron::to_string(&bands).expect("invariant: bands are plain data");
        std::fs::write(stage.directory.join(BANDS_FILE), text).expect("band file must be written");
        exit.send(AppExit::Success);
        return;
    };
    if !status.is_ready() {
        stage.settled_frames = 0;
        return;
    }
    if stage.settled_frames == 0 {
        for (handle, instance) in &stage.staged {
            let uniform = shot_uniform(instance, state, progress, &registry, &palette);
            let texture = registry.get_texture(instance.unit_type);
            sync_bubble_material(&mut materials, handle, &uniform, texture);
        }
    }
    stage.settled_frames += 1;
    if stage.settled_frames < SETTLE_FRAMES {
        return;
    }
    stage.capture_pending = true;
    let path = stage.directory.join(format!("{name}.png"));
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<'_, ScreenshotCaptured>, mut stage: ResMut<'_, Stage>| {
            trigger
                .event()
                .0
                .clone()
                .try_into_dynamic()
                .expect("a screenshot converts to an image")
                .to_rgb8()
                .save(&path)
                .expect("a screenshot must be written");
            stage.shot += 1;
            stage.settled_frames = 0;
            stage.capture_pending = false;
        },
    );
}

/// Projects rings of each subject's surface to physical window pixels, using the live camera.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "azimuths and window coordinates are small and positive"
)]
fn screen_bands(camera: &Camera, transform: &GlobalTransform) -> Vec<Band> {
    let rectangle = camera
        .physical_viewport_rect()
        .expect("invariant: the shots above rendered through this viewport");
    let size = rectangle.size().as_vec2();
    let clip_from_world = camera.clip_from_view() * transform.compute_matrix().inverse();
    let eye = transform.translation();
    let mut bands = Vec::new();
    for subject in SUBJECTS {
        for &share in subject.shares() {
            let height = subject.normal_height(share);
            let ring = (1.0 - height * height).sqrt();
            let pixels = (0..360)
                .step_by(AZIMUTH_STEP_DEGREES)
                .filter_map(|degrees| {
                    let (sin, cos) = (degrees as f32).to_radians().sin_cos();
                    let normal = Vec3::new(cos * ring, height, sin * ring);
                    let point = subject.centre + normal * subject.radius;
                    let facing = normal.dot((eye - point).normalize()) > MIN_VIEW_COSINE;
                    facing.then(|| {
                        let ndc = clip_from_world.project_point3(point);
                        let pixel = Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * size;
                        let window = rectangle.min + pixel.as_uvec2();
                        (window.x, window.y)
                    })
                })
                .collect();
            bands.push(Band {
                subject: subject.name.to_owned(),
                share,
                pixels,
            });
        }
    }
    bands
}

/// Mean absolute channel difference between two captures over `pixels`, in levels of 255.
#[allow(
    clippy::cast_precision_loss,
    reason = "a sum of a few thousand 8-bit differences fits an f32 exactly"
)]
fn mean_difference(a: &image::RgbImage, b: &image::RgbImage, pixels: &[(u32, u32)]) -> f32 {
    let total: u32 = pixels
        .iter()
        .flat_map(|&(x, y)| {
            let (left, right) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
            left.into_iter()
                .zip(right)
                .map(|(l, r)| u32::from(l.abs_diff(r)))
        })
        .sum();
    total as f32 / (pixels.len() * 3) as f32
}

/// Opens the capture `name` from `directory`.
fn capture(directory: &Path, name: &str) -> TestResult<image::RgbImage> {
    Ok(image::open(directory.join(format!("{name}.png")))?.to_rgb8())
}

/// Runs the child and returns the directory holding its captures.
fn run_child() -> TestResult<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    let (run, report) = command.run_validated("windowed dome child");
    let violations = report.violations(&[MISSING_COLORSPACE_EXTENSION]);
    assert!(
        violations.is_empty(),
        "{}: {violations:#?}",
        run.context("windowed dome child")
    );
    Ok(directory)
}

/// C-11: the blueprint predicted the poles on Z from Bevy's source. This reads the shipped mesh.
#[test]
fn the_dome_mesh_is_a_full_sphere_with_its_poles_on_z() -> TestResult {
    let root = client::ResolvedAssetsRoot::from_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .insert_resource(root)
        .insert_resource(CatalogResource(definition.catalogs.units.clone()))
        .add_systems(Startup, client::setup_bubble_assets_system);
    app.update();

    let meshes = app.world().resource::<Assets<Mesh>>();
    let handle = &app.world().resource::<BubbleSharedMeshes>().dome_mesh;
    let mesh = meshes.get(handle).ok_or("the dome mesh must exist")?;
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return Err("the dome mesh must have positions".into());
    };
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        return Err("the dome mesh must have UVs".into());
    };

    let at_row = |v: f32| {
        positions
            .iter()
            .zip(uvs)
            .filter(move |(_, uv)| (uv[1] - v).abs() < 1.0e-6)
            .map(|(position, _)| Vec3::from(*position))
    };
    for (row, pole) in [(0.0, Vec3::Z), (1.0, Vec3::NEG_Z)] {
        let mut vertices = at_row(row).peekable();
        assert!(vertices.peek().is_some(), "row {row} must have vertices");
        assert!(
            vertices.all(|vertex| (vertex - pole).length() < 1.0e-5),
            "row {row} must collapse to the pole {pole}"
        );
    }
    let heights = || at_row(0.5).map(|vertex| vertex.y);
    assert!(
        heights().fold(f32::MIN, f32::max) > 0.999 && heights().fold(f32::MAX, f32::min) < -0.999,
        "one UV row must span every height, so uv.y cannot express height"
    );
    let lowest = positions
        .iter()
        .map(|position| position[1])
        .fold(f32::MAX, f32::min);
    assert!(
        lowest < -0.999,
        "the dome keeps a lower half below the ground, down to {lowest}"
    );
    Ok(())
}

/// C-11: comparing `uv.y` with the progress swept the fill along the map's Y axis.
#[test]
fn the_fill_reads_height_not_uv() {
    let shader = production_module("bubble_dome.wgsl");
    let fragment = shader.entry_point(ShaderStage::Fragment, "fragment");
    assert!(
        shader.calls(fragment, "construction_height"),
        "the fragment entry point must call construction_height"
    );
    assert!(
        !shader.reads_argument_component(fragment, "uv", 1),
        "no fragment code may read uv.y, which does not follow height on this mesh"
    );
    assert!(
        shader.reads_argument_component(fragment, "uv", 0),
        "the probe must see a component the fragment does read"
    );
}

/// D-11: production progress shares `build_or_queue_progress` with construction, so only the
/// animation state may open the fill branch. The shader tests `anim_state == 0u`.
#[test]
fn only_a_constructing_dome_uses_its_progress() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let definition = BattleDefinition::load(&root, DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let style = definition.palette.preview_style(PlayerId(0))?;
    let archetype = definition
        .catalogs
        .units
        .archetypes()
        .find(|archetype| matches!(archetype.shape, ShapeKind::BuildingDome { .. }))
        .ok_or("the catalog must have a dome")?;
    let idle = template_instance(archetype, &SUBJECTS[0]);
    let producing = BubbleRenderInstance {
        build_or_queue_progress: Some(0.3),
        ..idle.clone()
    };
    let raising = BubbleRenderInstance {
        anim_state: AnimState::Constructing,
        ..producing.clone()
    };

    let packed = BubbleUniformGpu::from_instance(&producing, None, style);
    assert_ne!(packed.anim_state, 0, "a producing dome must skip the fill");

    let packed = BubbleUniformGpu::from_instance(&raising, None, style);
    assert_eq!(packed.anim_state, 0, "a foundation must enter the fill");
    assert_eq!(packed.build_progress, 0.3);

    let packed = BubbleUniformGpu::from_instance(&idle, None, style);
    assert_eq!(packed.build_progress, 1.0, "no job leaves nothing to fill");
    Ok(())
}

/// C-11 on the real renderer. Each shot is compared with the same scene at full progress, so the
/// result does not depend on the art: a built ring matches it and an unbuilt ring does not.
///
/// The rings are heights on the surface because a screen rectangle cannot show this defect. The
/// old fill also left the back of a dome unbuilt, and the back is the top of the screen: its upper
/// third measured 43 at 0.2 progress and 68 at 0.6, the same rise a correct fill gives by 1.0.
#[test]
fn a_foundation_fills_from_rim_to_apex_on_the_real_renderer() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = run_child()?;
    let finished = capture(directory.path(), FINISHED)?;
    let bands: Vec<Band> =
        ron::from_str(&std::fs::read_to_string(directory.path().join(BANDS_FILE))?)?;
    let mut failures = Vec::new();
    for (name, state, progress) in SHOTS {
        let shot = capture(directory.path(), name)?;
        for band in &bands {
            assert!(
                !band.pixels.is_empty(),
                "{} {} must be on screen",
                band.subject,
                band.share
            );
            let difference = mean_difference(&shot, &finished, &band.pixels);
            let filling = state == AnimState::Constructing;
            let built = !filling || band.share + SCAN_LINE_MARGIN < progress;
            let unbuilt = filling && band.share - SCAN_LINE_MARGIN > progress;
            if (built && difference > BUILT_LIMIT) || (unbuilt && difference < UNBUILT_FLOOR) {
                failures.push(format!(
                    "{name} {} at {}: difference {difference:.2}, built {built}",
                    band.subject, band.share
                ));
            }
        }
    }
    assert!(failures.is_empty(), "wrong fill: {failures:#?}");
    Ok(())
}

/// Builds what `client::build_graphical_app_with_options` builds for an invisible Visual Lab.
///
/// libtest runs every test on a spawned thread, even with `--test-threads=1`, and winit refuses
/// such a thread by default. That is the only reason this repeats the production assembly: the
/// builder has no way to set [`WinitPlugin::run_on_any_thread`], which a shipped client must not use.
fn windowed_client() -> TestResult<App> {
    Ok(client::build_graphical_app_off_main_thread(
        ClientMode::VisualLab,
        Some(CHILD_FRAME_LIMIT),
        DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?)
}

/// The child of [`a_foundation_fills_from_rim_to_apex_on_the_real_renderer`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn windowed_dome_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent test names the directory");
    let mut app = windowed_client().expect("the shipped assets must start the client");
    app.insert_resource(Stage {
        directory: directory.into(),
        ..default()
    })
    .add_systems(Update, (spawn_subjects, run_shots).chain());
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}
