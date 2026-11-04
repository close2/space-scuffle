//! The flipbook clock on a real GPU: the shipped shader picks the same layer as the Rust mirror.
//!
//! # Responsibility
//! Proves decisions D-2, D-3 and D-6 of `docs/plans/2026-11-04-battle-flipbook-animation.md` on
//! the real renderer: a bubble material whose lanes never change shows, at every clock value, the
//! atlas layer that `client::flipbook_clock::flipbook_layer` computes, and with frame blending the
//! mix that `flipbook_sample` describes. The integer frame function must agree with the Rust
//! reference exactly at every frame boundary, on `radv` and on `lavapipe`.
//!
//! # How it fits
//! The pixel test re-executes this test binary on the virtual GPU through `support/gpu_run.rs`,
//! as `combat_fx_slot_gpu_tests.rs` does, so the harness locks the physical GPU for the child only.
//! The child runs Bevy's renderer with the production `BubbleDomeMaterialPlugin` (material, clock
//! buffer and its in-place upload) and the shipped `bubble_dome.wgsl`, on a black stage with two
//! bubbles: one plain and one with frame blending. Their atlas is a fixture whose layers are solid,
//! distinct colours, so a pixel names its layer. Only `FlipbookClock` changes between shots; the
//! child writes the centre pixel of each bubble per clock value, and the parent judges them.
//!
//! # Invariants verified
//! - The fragment reads the clock and every flipbook lane, and binding 3 is a read-only storage
//!   buffer of `FlipbookClockGpu`'s size (shader IR).
//! - Without blending, the layer on screen equals `flipbook_layer` at all 64 clock values: both
//!   sides of every frame boundary in two cycles, mid-frame values, and the wrap of the `u32` clock.
//! - With blending, the colour on screen is the linear mix of the layer and the next one by
//!   `FlipbookSample::blend_weight`, within 2/255 per channel.
//! - The material is never modified while the clock moves: the clock reaches the GPU only through
//!   the shared buffer written in place.

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

use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use client::bubble_shader::{
    BubbleDomeMaterial, BubbleDomeMaterialPlugin, BubbleUniformGpu, FLAG_FRAME_BLEND,
};
use client::flipbook_clock::{
    flipbook_layer, flipbook_sample, FlipbookClock, FlipbookClockGpu, FlipbookLanes, FlipbookMode,
    FlipbookSpan, ANIM_BLEND_BIT, CLOCK_UNITS_PER_TICK,
};
use client::render_compatibility::{run_render_app, RenderCompatibilityPlugin};
use client::ResolvedAssetsRoot;
use naga::ShaderStage;
use serde::{Deserialize, Serialize};
use shader_ir::production_module;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying the child's output directory.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_FLIPBOOK_CLOCK_CHILD_OUT";
/// Name of the libtest entry that the pixel test launches as its child.
const CHILD_TEST: &str = "flipbook_clock_child";
/// File the child writes its [`ClockReport`] to.
const REPORT_FILE: &str = "clock.ron";

/// Layers of the fixture atlas.
const LAYERS: u32 = 12;
/// Edge of one fixture layer, in texels. Each layer is one solid colour.
const LAYER_EDGE: u32 = 4;
/// First layer of the played slice. Not 0, so the base lane is exercised.
const SPAN_BASE: u32 = 1;
/// Layers of the played slice.
const SPAN_COUNT: u32 = 10;
/// Clock units of one pass: seven ticks and five units, so frame boundaries fall unevenly.
const SPAN_PERIOD: u32 = 7 * CLOCK_UNITS_PER_TICK + 5;
/// Clock time at which the loop shows its first layer; not a multiple of the period, so the
/// canonical offset is not zero.
const PHASE_ZERO_AT: u32 = 123_457;
/// Cycles, counted from [`PHASE_ZERO_AT`], whose frame boundaries are shot.
const SHOT_CYCLES: [u32; 2] = [0, 5];

/// Window size, in pixels.
const WINDOW_SIZE: (f32, f32) = (320.0, 180.0);
/// Distance from the camera to the bubbles' plane, in world units.
const CAMERA_DISTANCE: f32 = 6.0;
/// Centres of the plain and the blending bubble, in world units.
const BUBBLE_CENTRES: [Vec3; 2] = [Vec3::new(-1.3, 0.0, 0.0), Vec3::new(1.3, 0.0, 0.0)];

/// Frames between a clock change and its capture.
const SETTLE_FRAMES: u32 = 3;
/// Captures allowed while the pipeline compiles, before the child gives up.
const MAX_WARM_UP_ATTEMPTS: u32 = 240;
/// Largest difference, in levels of 255 per channel, between a capture and its expected colour.
const CHANNEL_TOLERANCE: i32 = 2;

/// One shot: the clock value and the centre pixel of each bubble.
#[derive(Debug, Serialize, Deserialize)]
struct ClockShot {
    /// Clock value presented, in clock units.
    now: u32,
    /// Centre pixel of the plain bubble, sRGB.
    plain: [u8; 3],
    /// Centre pixel of the blending bubble, sRGB.
    blended: [u8; 3],
}

/// What the child saw, for the parent to check.
#[derive(Debug, Serialize, Deserialize)]
struct ClockReport {
    /// Every shot, in order.
    shots: Vec<ClockShot>,
    /// Modification events of bubble materials seen after warm-up.
    material_modifications: u32,
}

/// sRGB colour of fixture layer `layer`. The red channels are 20 levels apart, so the nearest
/// layer to a captured pixel is unambiguous.
fn layer_colour(layer: u32) -> [u8; 3] {
    assert!(layer < LAYERS, "the fixture has {LAYERS} layers");
    [20 + 20 * layer, 230 - 18 * layer, 60 + (layer * 53) % 140].map(|level| {
        u8::try_from(level).expect("invariant: every level of a fixture layer is below 256")
    })
}

/// The played slice.
fn span() -> FlipbookSpan {
    FlipbookSpan::new(SPAN_BASE, SPAN_COUNT, SPAN_PERIOD).expect("invariant: a valid fixture span")
}

/// Lanes of the plain (`blend == false`) or the blending bubble.
fn lanes(blend: bool) -> FlipbookLanes {
    FlipbookLanes::looping(span(), PHASE_ZERO_AT, blend)
}

/// Every clock value shot, in order.
///
/// Both sides of every frame boundary in two cycles (40 values), the middle of every frame in
/// the same cycles (20 values, where blending mixes evenly), and four values across the wrap of
/// the `u32` clock.
fn clock_values() -> Vec<u32> {
    let mut values = Vec::new();
    for cycle in SHOT_CYCLES {
        let start = PHASE_ZERO_AT + cycle * SPAN_PERIOD;
        for k in 0..SPAN_COUNT {
            let boundary = start + (k * SPAN_PERIOD).div_ceil(SPAN_COUNT);
            values.extend([
                boundary - 1,
                boundary,
                boundary + SPAN_PERIOD / SPAN_COUNT / 2,
            ]);
        }
    }
    values.extend([u32::MAX - 1, u32::MAX, 0, 1]);
    values
}

/// Linear value of an sRGB level.
fn decode(level: u8) -> f64 {
    let c = f64::from(level) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB level of a linear value.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to [0, 255] before conversion"
)]
fn encode(linear: f64) -> u8 {
    let c = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Largest per-channel difference between two colours, in levels.
fn channel_distance(a: [u8; 3], b: [u8; 3]) -> i32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (i32::from(*x) - i32::from(y)).abs())
        .max()
        .expect("invariant: three channels")
}

/// The fixture layer whose colour is nearest to `pixel`.
fn nearest_layer(pixel: [u8; 3]) -> u32 {
    (0..LAYERS)
        .min_by_key(|layer| channel_distance(layer_colour(*layer), pixel))
        .expect("invariant: the fixture has layers")
}

/// The colour the blending bubble must show at `now`.
fn expected_blend(now: u32) -> [u8; 3] {
    let sample = flipbook_sample(now, &lanes(true));
    let weight = f64::from(sample.blend_weight());
    let (from, to) = (layer_colour(sample.layer), layer_colour(sample.next_layer));
    [0, 1, 2].map(|c| encode(decode(from[c]) * (1.0 - weight) + decode(to[c]) * weight))
}

/// Step 3 of WP4: the fragment selects its layer from the clock and every lane.
#[test]
fn the_fragment_selects_its_layer_from_the_clock_and_every_lane() {
    let shader = production_module("bubble_dome.wgsl");
    let fragment = shader.entry_point(ShaderStage::Fragment, "fragment");
    for member in [
        "now",
        "anim_origin",
        "anim_period",
        "frame_count",
        "anim_offset",
        "anim_mode",
        "frame_base",
    ] {
        assert!(
            shader.reads_global_member(fragment, member),
            "the fragment must read {member}"
        );
    }
    let clock = shader.global_at(2, 3);
    assert_eq!(
        clock.space,
        naga::AddressSpace::Storage {
            access: naga::StorageAccess::LOAD
        },
        "@binding(3) must be a read-only storage buffer"
    );
    let naga::TypeInner::Struct { span, .. } = shader.module.types[clock.ty].inner else {
        panic!("@binding(3) must hold a struct");
    };
    assert_eq!(span as usize, size_of::<FlipbookClockGpu>());
    for (name, value) in [
        ("ANIM_MODE_FIXED", FlipbookMode::Fixed as u32),
        ("ANIM_MODE_LOOP", FlipbookMode::Loop as u32),
        ("ANIM_MODE_ONCE_HOLD", FlipbookMode::OnceHold as u32),
        ("ANIM_MODE_ONCE", FlipbookMode::Once as u32),
        ("ANIM_MODE_MASK", FlipbookMode::MASK),
        ("ANIM_BLEND_BIT", ANIM_BLEND_BIT),
    ] {
        assert_eq!(
            shader.u32_constant(name),
            Some(value),
            "WGSL {name} must equal the Rust value"
        );
    }
}

/// The fixture is unambiguous: every layer is nearest to itself by a wide margin.
#[test]
fn the_fixture_layers_are_far_apart() {
    for a in 0..LAYERS {
        assert_eq!(nearest_layer(layer_colour(a)), a);
        for b in (0..LAYERS).filter(|b| *b != a) {
            assert!(channel_distance(layer_colour(a), layer_colour(b)) >= 18);
        }
    }
    assert_eq!(clock_values().len(), 64);
}

/// D-2 and D-6 on the real renderer: the GPU picks the Rust mirror's layer at every boundary.
#[test]
fn the_shader_picks_the_rust_layer_at_every_clock_value() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = run_child()?;
    let report: ClockReport = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    let expected_values = clock_values();
    assert_eq!(
        report.shots.iter().map(|shot| shot.now).collect::<Vec<_>>(),
        expected_values,
        "the child shot every clock value in order"
    );
    assert_eq!(
        report.material_modifications, 0,
        "a moving clock must not modify any material"
    );
    let mut failures = Vec::new();
    let (mut plain_worst, mut blend_worst) = (0, 0);
    for shot in &report.shots {
        let layer = flipbook_layer(shot.now, &lanes(false));
        let seen = nearest_layer(shot.plain);
        let plain_distance = channel_distance(shot.plain, layer_colour(layer));
        plain_worst = plain_worst.max(plain_distance);
        if seen != layer || plain_distance > CHANNEL_TOLERANCE {
            failures.push(format!(
                "plain at {}: expected layer {layer}, saw layer {seen} ({:?})",
                shot.now, shot.plain
            ));
        }
        let blend = expected_blend(shot.now);
        let blend_distance = channel_distance(shot.blended, blend);
        blend_worst = blend_worst.max(blend_distance);
        if blend_distance > CHANNEL_TOLERANCE {
            failures.push(format!(
                "blended at {}: expected {blend:?}, saw {:?}",
                shot.now, shot.blended
            ));
        }
    }
    eprintln!(
        "{} shots: plain within {plain_worst} levels, blended within {blend_worst} levels",
        report.shots.len()
    );
    assert!(failures.is_empty(), "GPU and Rust disagree: {failures:#?}");
    Ok(())
}

/// The warning windowed runs logged on lavapipe before Mesa 25. It concerns the swapchain, not this
/// shader, so it gives this test no reason to fail.
const MISSING_COLORSPACE_EXTENSION: gpu_run::AllowedValidationId = gpu_run::AllowedValidationId {
    id: "Unable to find extension: VK_EXT_swapchain_colorspace",
    reason: "raised by wgpu_hal at swapchain setup; unrelated to the bubble material",
};

/// Runs the child and returns the directory holding its report.
fn run_child() -> TestResult<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    let (run, report) = command.run_validated("flipbook clock child");
    let violations = report.violations(&[MISSING_COLORSPACE_EXTENSION]);
    assert!(
        violations.is_empty(),
        "{}: {violations:#?}",
        run.context("flipbook clock child")
    );
    Ok(directory)
}

/// The child's stage.
#[derive(Resource)]
struct Stage {
    directory: PathBuf,
    values: Vec<u32>,
    shots: Vec<ClockShot>,
    /// Clock value of the capture under way; `None` during warm-up.
    current: Option<u32>,
    warm: bool,
    warm_up_attempts: u32,
    settled_frames: u32,
    capture_pending: bool,
    /// Set by the screenshot observer.
    captured: Option<image::RgbImage>,
    /// Physical pixel at each bubble's centre, once the camera is placed.
    centres: Option<[(u32, u32); 2]>,
    material_modifications: u32,
}

/// The fixture atlas: [`LAYERS`] solid layers of [`layer_colour`].
fn fixture_atlas() -> Image {
    let texels = (LAYER_EDGE * LAYER_EDGE) as usize;
    let data: Vec<u8> = (0..LAYERS)
        .flat_map(|layer| {
            let [r, g, b] = layer_colour(layer);
            std::iter::repeat_n([r, g, b, 255], texels).flatten()
        })
        .collect();
    let mut atlas = Image::new(
        Extent3d {
            width: LAYER_EDGE,
            height: LAYER_EDGE,
            depth_or_array_layers: LAYERS,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    atlas.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    atlas
}

/// A bubble uniform that shows only its media: every optical effect off, fully opaque.
fn media_only_uniform(blend: bool) -> BubbleUniformGpu {
    BubbleUniformGpu {
        radius: 1.0,
        feature_flags: if blend { FLAG_FRAME_BLEND } else { 0 },
        optics_media: Vec4::new(0.0, 0.04, 5.0, 0.0),
        optics_glare: Vec4::new(0.3, 0.0, 0.0, 0.5),
        optics_signals: Vec4::new(0.0, 1.0, 0.0, 0.0),
        ..default()
    }
    .with_flipbook_lanes(lanes(blend))
}

/// Spawns the camera and the two bubbles.
fn setup_stage(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut images: ResMut<'_, Assets<Image>>,
    mut materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Tonemapping::None,
        DebandDither::Disabled,
        Msaa::Off,
        Transform::from_xyz(0.0, 0.0, CAMERA_DISTANCE).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let sphere = meshes.add(Sphere::new(1.0).mesh().uv(48, 32));
    let atlas = images.add(fixture_atlas());
    for (centre, blend) in BUBBLE_CENTRES.into_iter().zip([false, true]) {
        let material = materials.add(BubbleDomeMaterial::new(
            media_only_uniform(blend),
            Some(atlas.clone()),
        ));
        commands.spawn((
            Mesh3d(sphere.clone()),
            MeshMaterial3d(material),
            Transform::from_translation(centre),
        ));
    }
}

/// Physical pixel of each bubble's centre, once the camera's transform has propagated.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the centres are inside a 320 by 180 window"
)]
fn bubble_centres(
    camera: &Camera,
    transform: &GlobalTransform,
    scale: f32,
) -> Option<[(u32, u32); 2]> {
    let mut centres = [(0, 0); 2];
    for (pixel, centre) in centres.iter_mut().zip(BUBBLE_CENTRES) {
        let viewport = camera.world_to_viewport(transform, centre).ok()?;
        *pixel = (
            (viewport.x * scale).round() as u32,
            (viewport.y * scale).round() as u32,
        );
    }
    Some(centres)
}

/// Advances the stage by one frame: records the last capture, sets the next clock value, and
/// captures it after it has settled.
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "Bevy passes system parameters by value"
)]
fn run_shots(
    mut commands: Commands<'_, '_>,
    mut stage: ResMut<'_, Stage>,
    mut clock: ResMut<'_, FlipbookClock>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform)>,
    windows: Query<'_, '_, &Window>,
    mut material_events: EventReader<'_, '_, AssetEvent<BubbleDomeMaterial>>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let modified = material_events
        .read()
        .filter(|event| matches!(event, AssetEvent::Modified { .. }))
        .count();
    if stage.warm {
        stage.material_modifications += u32::try_from(modified).expect("invariant: few events");
    }
    if stage.centres.is_none() {
        let scale = windows.iter().next().map_or(1.0, Window::scale_factor);
        stage.centres = cameras
            .iter()
            .next()
            .and_then(|(camera, transform)| bubble_centres(camera, transform, scale));
        return;
    }
    if stage.capture_pending {
        return;
    }
    if let Some(image) = stage.captured.take() {
        let centres = stage
            .centres
            .expect("invariant: centres found before capturing");
        let [plain, blended] = centres.map(|(x, y)| image.get_pixel(x, y).0);
        match stage.current.take() {
            None if plain != [0, 0, 0] && blended != [0, 0, 0] => stage.warm = true,
            None => {
                stage.warm_up_attempts += 1;
                assert!(
                    stage.warm_up_attempts < MAX_WARM_UP_ATTEMPTS,
                    "the bubbles never drew"
                );
            }
            Some(now) => stage.shots.push(ClockShot {
                now,
                plain,
                blended,
            }),
        }
        stage.settled_frames = 0;
    }
    if stage.warm && stage.current.is_none() {
        let Some(&now) = stage.values.get(stage.shots.len()) else {
            let report = ClockReport {
                shots: std::mem::take(&mut stage.shots),
                material_modifications: stage.material_modifications,
            };
            let text = ron::to_string(&report).expect("invariant: the report is plain data");
            std::fs::write(stage.directory.join(REPORT_FILE), text).expect("the report is written");
            exit.send(AppExit::Success);
            return;
        };
        clock.set_if_neq(FlipbookClock { now });
        stage.current = Some(now);
    }
    stage.settled_frames += 1;
    if stage.settled_frames < SETTLE_FRAMES {
        return;
    }
    stage.capture_pending = true;
    commands.spawn(Screenshot::primary_window()).observe(
        |trigger: Trigger<'_, ScreenshotCaptured>, mut stage: ResMut<'_, Stage>| {
            let image = trigger
                .event()
                .0
                .clone()
                .try_into_dynamic()
                .expect("a screenshot converts to an image")
                .to_rgb8();
            stage.captured = Some(image);
            stage.capture_pending = false;
        },
    );
}

/// Builds a Bevy app with the real renderer, an invisible window and the production bubble
/// material plugin.
///
/// libtest runs every test on a spawned thread, so winit must be allowed to run off the main
/// thread here, as in `client::build_graphical_app_off_main_thread`.
fn child_app(directory: PathBuf) -> TestResult<App> {
    let root = ResolvedAssetsRoot::resolve()?;
    let mut window_backend = bevy::winit::WinitPlugin::<bevy::winit::WakeUp>::default();
    window_backend.run_on_any_thread = true;
    let mut app = App::new();
    app.add_plugins(
        client::presentation::GamePresentationPlugins
            .set(AssetPlugin {
                file_path: root.bevy_file_path()?.to_owned(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    resolution: WINDOW_SIZE.into(),
                    visible: false,
                    ..default()
                }),
                ..default()
            })
            .set(window_backend),
    )
    .add_plugins((RenderCompatibilityPlugin, BubbleDomeMaterialPlugin))
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(Stage {
        directory,
        values: clock_values(),
        shots: Vec::new(),
        current: None,
        warm: false,
        warm_up_attempts: 0,
        settled_frames: 0,
        capture_pending: false,
        captured: None,
        centres: None,
        material_modifications: 0,
    })
    .add_systems(Startup, setup_stage)
    .add_systems(Update, run_shots);
    Ok(app)
}

/// The child of [`the_shader_picks_the_rust_layer_at_every_clock_value`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn flipbook_clock_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent test names the directory");
    let mut app =
        child_app(Path::new(&directory).to_path_buf()).expect("the shipped assets must load");
    assert!(run_render_app(&mut app).is_success());
}
