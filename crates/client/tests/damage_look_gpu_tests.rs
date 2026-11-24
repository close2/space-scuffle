//! The bubble's low-health look on the real renderer (plan `2026-11-09-effect-visualization`
//! Amendment 33, B-120).
//!
//! # Responsibility
//! Owns the proof that a critically damaged bubble (`hp_ratio` below 0.35 with
//! `FLAG_CRT_GLITCH`) reads as damage and not as a rendering fault: its media is the same picture
//! as the pristine bubble's, dimmed by a flicker of its own colours, with no hue split. The old
//! look sampled the red and blue channels of the media at sideways offsets (a chromatic
//! aberration), which drew a rainbow speckle along every edge of the unit's art.
//!
//! # How it fits
//! The pixel test re-executes this test binary through `support/gpu_run.rs` (charter §7, C21), as
//! `dome_fill_tests.rs` does. Its child is the real graphical client in Visual Lab mode, so the
//! catalog, the media, the shared meshes, the material pipeline and the shipped WGSL are all
//! production code. The child spawns one unit sphere itself, holds one media frame, writes its
//! uniform through `BubbleUniformGpu::from_instance` and `sync_bubble_material` (the path a live
//! bubble takes) at full health and then critically damaged, and pictures each.
//!
//! The measured shots turn `FLAG_CRACKS` off, so the comparison sees the low-health look of
//! `FLAG_CRT_GLITCH` alone; the look shots keep the production flags, cracks included, for the
//! evidence. With `SPACE_SCUFFLE_DAMAGE_EVIDENCE_DIR` set, the parent writes a contact sheet of
//! every shot, cropped round the bubble, and the full first look shot there.
//!
//! # Invariants verified
//! - Inside the bubble's media, each colour channel of a critical shot lines up with the same
//!   channel of the pristine shot at the same horizontal shift: no channel is displaced from the
//!   others (normalised cross-correlation, so dimming does not change the match).
//! - No critical shot is brighter than the brown-out's ceiling allows or darker than its floor,
//!   the darkest is clearly dimmed, and the shots, at least two beats apart, spread: it flickers.

#![allow(clippy::expect_used, clippy::panic)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use client::bubble_shader::{
    still_frame_lanes, sync_bubble_material, BubbleSharedMeshes, BubbleUniformGpu,
    FlipbookRegistry, FLAG_CRACKS,
};
use client::player_presentation::PlayerPalette;
use client::runtime_battle::{BattleRenderStatus, DEFAULT_BATTLE_SCENARIO};
use client::{BubbleDomeMaterial, CatalogResource, ClientMode, RtsCamera};
use serde::{Deserialize, Serialize};
use sim_core::catalog::UnitArchetype;
use sim_types::enums::{AnimState, ShapeKind, WeaponVisualKind};
use sim_types::ids::{EntityId, PlayerId};
use sim_types::snapshot::BubbleRenderInstance;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying the child's output directory.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_DAMAGE_CHILD_OUT";
/// Environment variable naming where the parent writes its evidence sheet, when set.
const EVIDENCE_VAR: &str = "SPACE_SCUFFLE_DAMAGE_EVIDENCE_DIR";
/// Name of the libtest entry that the pixel test launches as its child.
const CHILD_TEST: &str = "windowed_damage_child";
/// File the child writes the bubble's screen geometry to.
const GEOMETRY_FILE: &str = "geometry.ron";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// Consecutive ready frames between writing a uniform and capturing it.
const SETTLE_FRAMES: u32 = 8;
/// Least time between two shots, seconds of the app's clock (the shader's `globals.time`): two
/// beats of the brown-out's 7 a second, so consecutive critical shots sample independent levels
/// whatever the frame rate.
const SHOT_SPACING_S: f64 = 0.3;

/// Health of the pristine shot.
const PRISTINE_HP: f32 = 1.0;
/// Health of the critical shots: below the shader's critical threshold of 0.35.
const CRITICAL_HP: f32 = 0.3;
/// Health of the worn shot: cracked and scanlined, above the critical threshold.
const WORN_HP: f32 = 0.5;

/// How a shot is staged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Full health, production flags: the reference of every comparison.
    Pristine,
    /// Critical, `FLAG_CRACKS` off: measured against [`Kind::Pristine`].
    Measured,
    /// Critical, production flags: evidence only.
    Look,
    /// Worn ([`WORN_HP`]), production flags: evidence only.
    Worn,
}

/// Every shot the child takes, in order.
const SHOTS: [(&str, Kind); 14] = [
    ("pristine", Kind::Pristine),
    ("worn", Kind::Worn),
    ("measured_0", Kind::Measured),
    ("measured_1", Kind::Measured),
    ("measured_2", Kind::Measured),
    ("measured_3", Kind::Measured),
    ("measured_4", Kind::Measured),
    ("measured_5", Kind::Measured),
    ("look_0", Kind::Look),
    ("look_1", Kind::Look),
    ("look_2", Kind::Look),
    ("look_3", Kind::Look),
    ("look_4", Kind::Look),
    ("look_5", Kind::Look),
];

/// Where the sphere stands (behind the Lab lineup, where `dome_fill_tests.rs` stands its own).
const CENTRE: Vec3 = Vec3::new(-2.4, 0.85, -3.5);
/// Its radius in world units.
const RADIUS: f32 = 0.85;

/// Share of the bubble's screen radius whose pixels are compared: inside the lens vignette
/// (from 0.88) and the Fresnel and glare rim, so only the media is measured.
const INTERIOR_SHARE: f32 = 0.7;
/// Largest horizontal shift, in pixels, searched when lining a channel up. The old look shifted
/// red and blue by about 4 % of the bubble's radius each way plus a jitter, a few pixels here.
const MAX_SHIFT_PX: i32 = 8;
/// The darkest of the critical shots must lie below this mean luma ratio to the pristine shot:
/// the bubble reads as dimmed. The old look (worn scanlines alone) measured 0.947 to 0.952 on
/// radv on 2026-11-19, so it fails this bound as well as the hue check; six independent beats of
/// the brown-out (light 0.35 to 0.9, compressed towards 1 by tone mapping) all above it are
/// vanishingly unlikely.
const DIMMED_BELOW: f32 = 0.92;
/// No critical shot may be brighter than this ratio: the brown-out never gives full light. The
/// old look (0.95) would pass this bound; the darkest-shot bound and the hue check fail it.
const NEVER_ABOVE: f32 = 0.97;
/// Nor darker than this: the brown-out keeps at least 0.35 of the media's light (more after tone
/// mapping), so a darker shot means some other darkening.
const DIMMED_FLOOR: f32 = 0.3;
/// The six critical shots' ratios must spread by at least this much: the look flickers, it is
/// not a constant dimming (B-120 review round 1; measured 0.11 for the first brown-out). Six
/// independent beats all within it are about a thousandth as likely.
const FLICKER_SPREAD: f64 = 0.02;

/// The warning windowed runs logged on lavapipe before Mesa 25. It concerns the swapchain, not
/// this shader, so it gives this test no reason to fail.
const MISSING_COLORSPACE_EXTENSION: gpu_run::AllowedValidationId = gpu_run::AllowedValidationId {
    id: "Unable to find extension: VK_EXT_swapchain_colorspace",
    reason: "raised by wgpu_hal at swapchain setup; unrelated to the bubble material",
};

/// Where the bubble is on screen, in physical window pixels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Geometry {
    /// The sphere's centre.
    centre: (f32, f32),
    /// Its radius.
    radius: f32,
}

/// Progress of the child through [`SHOTS`].
#[derive(Resource, Default)]
struct Stage {
    /// Output directory.
    directory: PathBuf,
    /// Material handle and template instance of the sphere.
    staged: Option<(Handle<BubbleDomeMaterial>, BubbleRenderInstance)>,
    /// Index into [`SHOTS`].
    shot: usize,
    /// Ready frames rendered since the shot's uniform was written.
    settled_frames: u32,
    /// Whether a screenshot was requested and has not arrived.
    capture_pending: bool,
    /// The app's clock at the last screenshot request, seconds.
    last_shot_at: f64,
}

/// The snapshot record of an idle, full-health bubble of `archetype` at [`CENTRE`].
fn template_instance(archetype: &UnitArchetype) -> BubbleRenderInstance {
    BubbleRenderInstance {
        entity_id: EntityId(1),
        shape: archetype.shape,
        silhouette: archetype.silhouette,
        unit_type: archetype.unit_type_id,
        owner: PlayerId(0),
        world_pos: [CENTRE.x, CENTRE.z, CENTRE.y],
        ground_z: 0.0,
        radius: RADIUS,
        velocity: [0.0, 0.0],
        heading_angle_rad: 0.0,
        aim_angle_rad: 0.0,
        is_attacking: false,
        weapon_visual: WeaponVisualKind::Hitscan,
        attack_target_pos: None,
        anim_state: AnimState::Idle,
        attack_swing: None,
        hp_ratio: PRISTINE_HP,
        shield_ratio: 1.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: None,
        selected: false,
        concealment: sim_types::Concealment::None,
        speed_boosted: false,
        stance: sim_types::StanceView::default(),
        status: sim_types::StatusFlags::default(),
        barrier_ratio: 0.0,
        housed: false,
    }
}

/// Spawns the sphere of the first unit with shipped media, through the shared mesh.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn spawn_subject(
    mut commands: Commands<'_, '_>,
    mut stage: ResMut<'_, Stage>,
    mut materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    shared: Option<Res<'_, BubbleSharedMeshes>>,
    registry: Option<Res<'_, FlipbookRegistry>>,
    catalog: Res<'_, CatalogResource>,
) {
    let (Some(shared), Some(registry), true) = (shared, registry, stage.staged.is_none()) else {
        return;
    };
    let archetype = catalog
        .0
        .archetypes()
        .find(|archetype| {
            matches!(archetype.shape, ShapeKind::UnitSphere)
                && registry.get_texture(archetype.unit_type_id).is_some()
                && registry.get_meta(archetype.unit_type_id).is_some()
        })
        .expect("the catalog has a unit with shipped media");
    let material = materials.add(BubbleDomeMaterial::new(
        BubbleUniformGpu::default(),
        registry.get_texture(archetype.unit_type_id),
    ));
    commands.spawn((
        Mesh3d(shared.sphere_mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(CENTRE).with_scale(Vec3::splat(RADIUS)),
    ));
    stage.staged = Some((material, template_instance(archetype)));
}

/// Packs `instance` for the shot `kind`, holding one media frame across every shot.
fn shot_uniform(
    instance: &BubbleRenderInstance,
    kind: Kind,
    registry: &FlipbookRegistry,
    palette: &PlayerPalette,
) -> BubbleUniformGpu {
    let style = palette
        .preview_style(instance.owner)
        .expect("invariant: player 0 is in the shipped palette");
    let frame = still_frame_lanes(registry.get_meta(instance.unit_type), AnimState::Idle);
    let hp_ratio = match kind {
        Kind::Pristine => PRISTINE_HP,
        Kind::Measured | Kind::Look => CRITICAL_HP,
        Kind::Worn => WORN_HP,
    };
    let mut uniform = BubbleUniformGpu::from_instance(
        &BubbleRenderInstance {
            hp_ratio,
            ..instance.clone()
        },
        style,
        frame,
    );
    if kind == Kind::Measured {
        uniform.feature_flags &= !FLAG_CRACKS;
    }
    uniform
}

/// Writes each shot's uniform, waits for the renderer, and requests one screenshot per shot;
/// after the last, writes the bubble's screen geometry and exits.
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "Bevy passes system parameters by value, each a declared data dependency"
)]
fn run_shots(
    mut commands: Commands<'_, '_>,
    mut stage: ResMut<'_, Stage>,
    mut materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    registry: Option<Res<'_, FlipbookRegistry>>,
    palette: Res<'_, PlayerPalette>,
    status: Res<'_, BattleRenderStatus>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform), With<RtsCamera>>,
    time: Res<'_, Time>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let (Some(registry), Some((handle, instance))) = (registry, stage.staged.clone()) else {
        return;
    };
    if stage.capture_pending {
        return;
    }
    let Some(&(name, kind)) = SHOTS.get(stage.shot) else {
        let (camera, transform) = cameras.single();
        let geometry = screen_geometry(camera, transform);
        let text = ron::to_string(&geometry).expect("invariant: the geometry is plain data");
        std::fs::write(stage.directory.join(GEOMETRY_FILE), text)
            .expect("the geometry file must be written");
        exit.send(AppExit::Success);
        return;
    };
    if !status.is_ready() {
        stage.settled_frames = 0;
        return;
    }
    if stage.settled_frames == 0 {
        let uniform = shot_uniform(&instance, kind, &registry, &palette);
        let texture = registry.get_texture(instance.unit_type);
        sync_bubble_material(&mut materials, &handle, &uniform, texture);
    }
    stage.settled_frames += 1;
    let now = time.elapsed_secs_f64();
    if stage.settled_frames < SETTLE_FRAMES || now - stage.last_shot_at < SHOT_SPACING_S {
        return;
    }
    stage.capture_pending = true;
    stage.last_shot_at = now;
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

/// Projects the sphere's centre and its radius across the screen to physical window pixels.
fn screen_geometry(camera: &Camera, transform: &GlobalTransform) -> Geometry {
    let rectangle = camera
        .physical_viewport_rect()
        .expect("invariant: the shots rendered through this viewport");
    let size = rectangle.size().as_vec2();
    let clip_from_world = camera.clip_from_view() * transform.compute_matrix().inverse();
    let to_window = |point: Vec3| {
        let ndc = clip_from_world.project_point3(point);
        rectangle.min.as_vec2() + Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * size
    };
    let centre = to_window(CENTRE);
    let edge = to_window(CENTRE + transform.right() * RADIUS);
    Geometry {
        centre: (centre.x, centre.y),
        radius: centre.distance(edge),
    }
}

/// Window pixels inside [`INTERIOR_SHARE`] of the bubble, kept [`MAX_SHIFT_PX`] from the
/// picture's sides so every shifted read stays inside it.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "window coordinates are small and positive"
)]
fn interior(geometry: Geometry, width: u32, height: u32) -> Vec<(i32, i32)> {
    let reach = geometry.radius * INTERIOR_SHARE;
    let (cx, cy) = geometry.centre;
    let mut pixels = Vec::new();
    for y in (cy - reach).floor().max(0.0) as i32..=(cy + reach).ceil() as i32 {
        for x in (cx - reach).floor() as i32..=(cx + reach).ceil() as i32 {
            let inside = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) <= reach;
            let room =
                x >= MAX_SHIFT_PX && x + MAX_SHIFT_PX < width as i32 && y >= 0 && y < height as i32;
            if inside && room {
                pixels.push((x, y));
            }
        }
    }
    pixels
}

/// One channel's value at a window pixel.
#[allow(
    clippy::cast_sign_loss,
    reason = "callers pass pixels inside the picture"
)]
fn channel(image: &image::RgbImage, x: i32, y: i32, channel: usize) -> f32 {
    f32::from(image.get_pixel(x as u32, y as u32).0[channel])
}

/// Normalised cross-correlation of `shot`'s `channel` read `shift` pixels to the right with
/// `reference`'s same channel, over `pixels`: 1 for the same picture up to a gain and an offset.
#[allow(
    clippy::cast_precision_loss,
    reason = "a few thousand pixels sum exactly enough in f64"
)]
fn correlation(
    shot: &image::RgbImage,
    reference: &image::RgbImage,
    pixels: &[(i32, i32)],
    channel_index: usize,
    shift: i32,
) -> f64 {
    let pairs = || {
        pixels.iter().map(move |&(x, y)| {
            (
                f64::from(channel(shot, x + shift, y, channel_index)),
                f64::from(channel(reference, x, y, channel_index)),
            )
        })
    };
    let count = pixels.len() as f64;
    let (mean_a, mean_b) = pairs().fold((0.0, 0.0), |(a, b), (u, v)| (a + u, b + v));
    let (mean_a, mean_b) = (mean_a / count, mean_b / count);
    let (mut cross, mut var_a, mut var_b) = (0.0, 0.0, 0.0);
    for (u, v) in pairs() {
        cross += (u - mean_a) * (v - mean_b);
        var_a += (u - mean_a) * (u - mean_a);
        var_b += (v - mean_b) * (v - mean_b);
    }
    cross / (var_a * var_b).sqrt().max(f64::EPSILON)
}

/// The horizontal shift at which `shot`'s `channel` best matches the reference's.
fn best_shift(
    shot: &image::RgbImage,
    reference: &image::RgbImage,
    pixels: &[(i32, i32)],
    channel_index: usize,
) -> i32 {
    (-MAX_SHIFT_PX..=MAX_SHIFT_PX)
        .max_by(|&a, &b| {
            correlation(shot, reference, pixels, channel_index, a).total_cmp(&correlation(
                shot,
                reference,
                pixels,
                channel_index,
                b,
            ))
        })
        .unwrap_or(0)
}

/// Mean Rec. 709 luma over `pixels`, in levels of 255.
#[allow(
    clippy::cast_precision_loss,
    reason = "a few thousand pixels sum exactly enough in f64"
)]
fn mean_luma(image: &image::RgbImage, pixels: &[(i32, i32)]) -> f64 {
    let total: f64 = pixels
        .iter()
        .map(|&(x, y)| {
            0.2126 * f64::from(channel(image, x, y, 0))
                + 0.7152 * f64::from(channel(image, x, y, 1))
                + 0.0722 * f64::from(channel(image, x, y, 2))
        })
        .sum();
    total / pixels.len() as f64
}

/// Opens the capture `name` from `directory`.
fn capture(directory: &Path, name: &str) -> TestResult<image::RgbImage> {
    Ok(image::open(directory.join(format!("{name}.png")))?.to_rgb8())
}

/// Writes the evidence sheet: every shot cropped round the bubble and doubled, one row per
/// series (pristine and worn; measured; look), and the first look shot whole.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "window coordinates and sheet cells are small and positive"
)]
fn write_evidence(directory: &Path, geometry: Geometry) -> TestResult {
    let Some(target) = std::env::var_os(EVIDENCE_VAR) else {
        return Ok(());
    };
    let target = PathBuf::from(target);
    std::fs::create_dir_all(&target)?;
    let driver = gpu_run::selected_driver().driver.name();
    let side = (geometry.radius * 2.6).ceil() as u32;
    let left = (geometry.centre.0 - side as f32 / 2.0).max(0.0) as u32;
    let top = (geometry.centre.1 - side as f32 / 2.0).max(0.0) as u32;
    let cell = side * 2;
    let rows: [&[&str]; 3] = [
        &["pristine", "worn"],
        &[
            "measured_0",
            "measured_1",
            "measured_2",
            "measured_3",
            "measured_4",
            "measured_5",
        ],
        &["look_0", "look_1", "look_2", "look_3", "look_4", "look_5"],
    ];
    let mut sheet = image::RgbImage::new(cell * 6, cell * 3);
    for (row, names) in rows.iter().enumerate() {
        for (column, name) in names.iter().enumerate() {
            let shot = capture(directory, name)?;
            let crop = image::imageops::crop_imm(&shot, left, top, side, side).to_image();
            let doubled =
                image::imageops::resize(&crop, cell, cell, image::imageops::FilterType::Nearest);
            image::imageops::replace(
                &mut sheet,
                &doubled,
                i64::from(cell) * column as i64,
                i64::from(cell) * row as i64,
            );
        }
    }
    sheet.save(target.join(format!("b120_damage_sheet_{driver}.png")))?;
    std::fs::copy(
        directory.join("look_0.png"),
        target.join(format!("b120_damage_frame_{driver}.png")),
    )?;
    Ok(())
}

/// Runs the child and returns the directory holding its captures.
fn run_child() -> TestResult<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    let (run, report) = command.run_validated("windowed damage child");
    let violations = report.violations(&[MISSING_COLORSPACE_EXTENSION]);
    assert!(
        violations.is_empty(),
        "{}: {violations:#?}",
        run.context("windowed damage child")
    );
    Ok(directory)
}

/// B-120 on the real renderer: a critically damaged bubble shows its own picture, dimmed, with
/// every colour channel in place.
#[test]
fn a_critically_damaged_bubble_dims_its_own_colours_without_a_hue_split() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = run_child()?;
    let geometry: Geometry = ron::from_str(&std::fs::read_to_string(
        directory.path().join(GEOMETRY_FILE),
    )?)?;
    write_evidence(directory.path(), geometry)?;
    let pristine = capture(directory.path(), "pristine")?;
    let pixels = interior(geometry, pristine.width(), pristine.height());
    assert!(
        pixels.len() > 500,
        "the bubble's interior must be on screen: {geometry:?}, {} pixels",
        pixels.len()
    );
    let reference_luma = mean_luma(&pristine, &pixels);
    let mut failures = Vec::new();
    let mut ratios = Vec::new();
    for (name, kind) in SHOTS {
        if kind != Kind::Measured {
            continue;
        }
        let shot = capture(directory.path(), name)?;
        let shifts: Vec<i32> = (0..3)
            .map(|index| best_shift(&shot, &pristine, &pixels, index))
            .collect();
        let ratio = mean_luma(&shot, &pixels) / reference_luma;
        eprintln!("{name}: channel shifts (r, g, b) {shifts:?} px, luma ratio {ratio:.3}");
        if shifts.iter().any(|&shift| shift != shifts[1]) {
            failures.push(format!(
                "{name}: the channels line up at different shifts {shifts:?}: a hue split"
            ));
        }
        if !(f64::from(DIMMED_FLOOR)..f64::from(NEVER_ABOVE)).contains(&ratio) {
            failures.push(format!(
                "{name}: luma ratio {ratio:.3} to the pristine shot, outside \
                 [{DIMMED_FLOOR}, {NEVER_ABOVE})"
            ));
        }
        ratios.push(ratio);
    }
    let darkest = ratios.iter().copied().fold(f64::INFINITY, f64::min);
    let brightest = ratios.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    eprintln!("luma ratios from {darkest:.3} to {brightest:.3}");
    if darkest >= f64::from(DIMMED_BELOW) {
        failures.push(format!(
            "no critical shot is dimmed below {DIMMED_BELOW}: the darkest is {darkest:.3}"
        ));
    }
    if brightest - darkest < FLICKER_SPREAD {
        failures.push(format!(
            "the critical shots spread by {:.3}, under {FLICKER_SPREAD}: no flicker",
            brightest - darkest
        ));
    }
    assert!(failures.is_empty(), "the low-health look: {failures:#?}");
    Ok(())
}

/// Builds what `client::build_graphical_app_with_options` builds for an invisible Visual Lab
/// (libtest's threads need `run_on_any_thread`, as in `dome_fill_tests.rs`).
fn windowed_client() -> TestResult<App> {
    Ok(client::build_graphical_app_off_main_thread(
        ClientMode::VisualLab,
        Some(CHILD_FRAME_LIMIT),
        DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?)
}

/// The child of [`a_critically_damaged_bubble_dims_its_own_colours_without_a_hue_split`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn windowed_damage_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent test names the directory");
    let mut app = windowed_client().expect("the shipped assets must start the client");
    app.insert_resource(Stage {
        directory: directory.into(),
        ..default()
    })
    .add_systems(Update, (spawn_subject, run_shots).chain());
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}
