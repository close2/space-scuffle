//! A body drawn tangent to a cliff wall is not cut by the cliff, on the real shaders (plan
//! `docs/plans/2026-11-30-terrain-and-air-indicators.md` §5.2, G-1; B-243).
//!
//! # Responsibility
//! The simulation holds a ground body at clearance 0 from a cliff wall, so a unit drawn at its
//! terrain radius touches the wall's plane. The drawn cliff must leave that unit whole: no foot,
//! bevel or wall row may stand in front of it on the low side. This test renders the production
//! client and checks that every pixel of the unit's silhouette next to the wall shows the unit.
//!
//! # How it works
//! One offscreen child of this binary, through `support/gpu_run.rs` (charter §7: radv with the
//! shared lock per child locally, lavapipe in CI), boots the production client with a renderer on
//! the shipped battle, kills the mobile units, turns the smog volume off
//! (`LabState::opt_smog_volume`), and stages a straight cliff wall `x = c`
//! (`support/cliff_contact_stage.rs`). The ground unit with the smallest drawn radius among
//! those drawn at their terrain radius is written tangent to the wall on the low side, not
//! selected. The RTS camera has no yaw, so with its target on the wall at
//! `MIN_CAMERA_DISTANCE` the whole wall projects onto the canvas's centre column `x_c`. The child
//! captures `B` (unit at the wall) and then `A` (unit written [`AWAY_CELLS`] cells away), each
//! after the same settle frames with the clock stopped, and writes the drawn sphere and the
//! silhouette `Σ`: the pixels where the bubble is drawn. `bubble_dome.wgsl` draws a unit's bubble
//! as a screen-space disc of its radius at its centre's depth (it discards the perspective
//! silhouette's elliptical "ears"), so `Σ` is the pixels whose camera ray crosses the plane
//! through the centre, facing the camera, within the radius of the centre.
//!
//! # Invariants verified (by the parent)
//! 1. Tangency (precondition): the drawn sphere's nearest point to the plane `x = c` lies on the
//!    low side within [`TANGENCY_TOLERANCE`] of it.
//! 2. No cut: in the band of `Σ` from [`COLUMN_MARGIN`] to `w` columns from `x_c` on the low
//!    side, `w` the projected width of [`BAND_WIDTH`] world units at the sphere's depth, at least
//!    [`MIN_CHANGED_SHARE`] of the pixels differ between `A` and `B` by at least
//!    [`MIN_CHANNEL_DELTA`] in some channel. An unchanged pixel is one where terrain in front of
//!    the unit hides it.
//! 3. Control: the same share holds in `Σ` beyond the band, so a bubble that changes too few
//!    pixels anywhere cannot pass assertion 2 for the wrong reason.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;
#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/cliff_contact_stage.rs"]
mod cliff_contact_stage;
#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_battle.rs"]
mod gpu_battle;
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
#[path = "support/unit_cycling.rs"]
mod unit_cycling;

use std::path::{Path, PathBuf};

use battle_harness::{Harness, LOCAL_PLAYER};
use bevy::prelude::*;
use client::camera::MIN_CAMERA_DISTANCE;
use client::scene_controller::RenderEntityMap;
use client::RtsCamera;
use cliff_contact_stage::Wall;
use serde::{Deserialize, Serialize};
use sim_types::{EntityId, SimVec2};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying the child's output directory.
const OUT_VAR: &str = "SPACE_SCUFFLE_CLIFF_CONTACT_OUT";
/// Environment variable that keeps the captures and masks in a directory for review.
const KEEP_VAR: &str = "SPACE_SCUFFLE_CLIFF_CONTACT_KEEP";
/// Environment variable that makes the child capture the review poses as well.
const REVIEW_VAR: &str = "SPACE_SCUFFLE_CLIFF_CONTACT_REVIEW";
/// Capture size.
const SIZE: UVec2 = UVec2::new(1280, 720);
/// The child.
const CHILD: &str = "cliff_contact_child";
/// File of the child's [`Report`].
const REPORT_FILE: &str = "report.ron";
/// Cells the unit is written away from the wall for capture `A`.
const AWAY_CELLS: i32 = 20;
/// Frames between a staged change and its capture: the camera's look height eases over several
/// frames (`LOOK_AT_SMOOTHING_SECS`), and both captures wait the same.
const SETTLE_FRAMES: usize = 40;
/// Tangency tolerance, world units: one step of the simulation's fixed point.
const TANGENCY_TOLERANCE: f32 = 1.0 / 4096.0;
/// Width of the checked band beside the wall, world units: at least the foot's spread plus the
/// wall jitter of the mesher (`FOOT_WIDTH + WALL_JITTER_AMPLITUDE`).
const BAND_WIDTH: f32 = 0.30;
/// Columns beside the wall's projection left out for rasterisation and multisampling.
const COLUMN_MARGIN: u32 = 2;
/// Least share of the checked pixels that must change between `A` and `B`.
const MIN_CHANGED_SHARE: f64 = 0.99;
/// Least change of a pixel in some channel, out of 255.
const MIN_CHANNEL_DELTA: u8 = 2;

/// What the child reports besides its captures.
#[derive(Debug, Serialize, Deserialize)]
struct Report {
    /// The wall's edge `c`, world units, and its low side (`+1`: `x > c`).
    edge: f32,
    low_side: i32,
    /// The drawn sphere in `B`: centre and radius, world units.
    centre: [f32; 3],
    radius: f32,
    /// The canvas column onto which the wall projects, at the sphere's depth.
    wall_column: f32,
    /// Projected width of [`BAND_WIDTH`] world units at the sphere's depth, pixels.
    band_pixels: f32,
}

// --- Child ---------------------------------------------------------------------------------------

/// Writes unit `id` to `at`, runs one tick so the snapshot carries it, and lets the frame settle.
fn place(harness: &mut Harness, id: EntityId, at: SimVec2) {
    harness
        .world_mut()
        .entities
        .get_mut(&id)
        .expect("the staged unit")
        .position = at;
    gpu_battle::run_ticks(harness, 1);
    harness.step(SETTLE_FRAMES);
}

/// The drawn bubble of unit `id`: its world centre and radius (the bubble mesh is a unit sphere
/// scaled by the radius).
fn drawn_sphere(harness: &mut Harness, id: EntityId) -> (Vec3, f32) {
    let world = harness.app.world();
    let entity = world
        .resource::<RenderEntityMap>()
        .get(id)
        .expect("the unit is drawn");
    let transform = world
        .get::<GlobalTransform>(entity)
        .expect("a transform")
        .compute_transform();
    (transform.translation, transform.scale.x)
}

/// Whether `ray` passes within `radius` of `centre` where it crosses the plane through `centre`
/// facing the camera (`forward`, the camera's view direction): the test `bubble_dome.wgsl`
/// applies to every fragment of a unit sphere, which draws the bubble as a screen-space disc of
/// its radius at its centre's depth and discards the perspective silhouette's "ears" beyond it.
fn in_drawn_disc(ray: Ray3d, forward: Vec3, centre: Vec3, radius: f32) -> bool {
    let along = ray.direction.dot(forward);
    if along <= 0.0 {
        return false;
    }
    let t = (centre - ray.origin).dot(forward) / along;
    (ray.origin + ray.direction * t).distance(centre) <= radius
}

/// The silhouette mask `Σ` of the unit as drawn: white where the camera ray through the pixel's
/// centre lands inside the bubble's disc ([`in_drawn_disc`]).
fn silhouette(harness: &mut Harness, centre: Vec3, radius: f32) -> image::GrayImage {
    let forward = {
        let world = harness.app.world_mut();
        world
            .query_filtered::<&GlobalTransform, With<RtsCamera>>()
            .single(world)
            .forward()
    };
    let mut mask = image::GrayImage::new(SIZE.x, SIZE.y);
    for y in 0..SIZE.y {
        for x in 0..SIZE.x {
            let ray = harness.pixel_ray(Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
            if in_drawn_disc(ray, *forward, centre, radius) {
                mask.put_pixel(x, y, image::Luma([255]));
            }
        }
    }
    mask
}

/// The review poses (plan §6.2): the unit at the wall from the side, and the ground unit with
/// the smallest terrain radius at the high side's brink (D-4).
fn review_poses(harness: &mut Harness, wall: Wall, directory: &Path) {
    let assets = battle_harness::assets();
    let brink_type = cliff_contact_stage::brink_unit(harness.world(), &assets);
    let radius = harness
        .world()
        .catalog
        .get(brink_type)
        .expect("the brink unit's archetype")
        .terrain_radius;
    let at = wall.high_brink(radius);
    let id = harness
        .world_mut()
        .spawn_unit(brink_type, LOCAL_PLAYER, at)
        .expect("the brink unit is placed");
    place(harness, id, at);
    gpu_battle::capture(harness, directory, "brink");
    let mut camera = harness
        .app
        .world_mut()
        .query::<&mut RtsCamera>()
        .single_mut(harness.app.world_mut());
    camera.target.x += 2.5 * wall.low_side as f32;
    camera.target.y -= 1.5;
    harness.step(SETTLE_FRAMES);
    gpu_battle::capture(harness, directory, "brink_side");
}

/// The child: stages the wall and the unit, captures `B` and `A`, and writes its report and `Σ`.
fn run_child() {
    let Some(directory) = std::env::var_os(OUT_VAR).map(PathBuf::from) else {
        println!("{OUT_VAR} is not set: this child runs only under its parent");
        return;
    };
    let mut harness =
        gpu_battle::boot_on_gpu(client::runtime_battle::DEFAULT_BATTLE_SCENARIO, SIZE);
    gpu_battle::ready(&mut harness);
    unit_cycling::kill_mobile_units(harness.world_mut());
    gpu_battle::run_ticks(&mut harness, 2);

    let wall = Wall::find(&harness.world().map);
    println!("staged wall: {wall:?}");
    let assets = battle_harness::assets();
    let unit_type = cliff_contact_stage::contact_unit(harness.world(), &assets);
    let radius = harness
        .world()
        .catalog
        .get(unit_type)
        .expect("the contact unit's archetype")
        .terrain_radius;
    let at = wall.low_contact(radius);
    let id = harness
        .world_mut()
        .spawn_unit(unit_type, LOCAL_PLAYER, at)
        .expect("the unit is placed at the wall");
    {
        let mut camera = harness
            .app
            .world_mut()
            .query::<&mut RtsCamera>()
            .single_mut(harness.app.world_mut());
        camera.target = Vec2::new(wall.c as f32, at.y.to_f32());
        camera.distance = MIN_CAMERA_DISTANCE;
    }
    place(&mut harness, id, at);
    gpu_battle::capture(&mut harness, &directory, "B");
    let (centre, drawn_radius) = drawn_sphere(&mut harness, id);
    let sigma = silhouette(&mut harness, centre, drawn_radius);
    sigma
        .save(directory.join("sigma.png"))
        .expect("the silhouette mask is written");
    let edge = wall.c as f32;
    let wall_column = harness
        .screen_position(Vec3::new(edge, centre.y, centre.z))
        .x;
    let band_end = harness
        .screen_position(Vec3::new(
            edge + BAND_WIDTH * wall.low_side as f32,
            centre.y,
            centre.z,
        ))
        .x;

    let away_rows = if at.y.to_f32() > AWAY_CELLS as f32 {
        -AWAY_CELLS
    } else {
        AWAY_CELLS
    };
    place(&mut harness, id, at + SimVec2::from_i32(0, away_rows));
    gpu_battle::capture(&mut harness, &directory, "A");
    if std::env::var_os(REVIEW_VAR).is_some() {
        review_poses(&mut harness, wall, &directory);
    }

    let report = Report {
        edge,
        low_side: wall.low_side,
        centre: centre.to_array(),
        radius: drawn_radius,
        wall_column,
        band_pixels: (band_end - wall_column).abs(),
    };
    std::fs::write(
        directory.join(REPORT_FILE),
        ron::to_string(&report).expect("a report serializes"),
    )
    .expect("the report is written");
}

#[test]
#[ignore = "a GPU child: cliff_contact_gpu_tests' parent runs it"]
fn cliff_contact_child() {
    run_child();
}

// --- Parent --------------------------------------------------------------------------------------

/// Reads capture `name` of the child that wrote `directory`.
fn shot(directory: &Path, name: &str) -> TestResult<image::RgbImage> {
    Ok(image::open(directory.join(format!("{name}.png")))?.to_rgb8())
}

/// Whether pixel `(x, y)` differs between `a` and `b` by at least [`MIN_CHANNEL_DELTA`] in some
/// channel.
fn changed(a: &image::RgbImage, b: &image::RgbImage, x: u32, y: u32) -> bool {
    a.get_pixel(x, y)
        .0
        .iter()
        .zip(b.get_pixel(x, y).0)
        .any(|(p, q)| p.abs_diff(q) >= MIN_CHANNEL_DELTA)
}

/// The changed and total pixel counts of `Σ` in the band beside the wall and beyond it, and a
/// mask of the unchanged pixels of `Σ`.
fn tally(
    report: &Report,
    sigma: &image::GrayImage,
    a: &image::RgbImage,
    b: &image::RgbImage,
) -> ([(usize, usize); 2], image::GrayImage) {
    let mut counts = [(0, 0); 2];
    let mut unchanged = image::GrayImage::new(SIZE.x, SIZE.y);
    for (x, y, value) in sigma.enumerate_pixels() {
        if value.0[0] == 0 {
            continue;
        }
        let from_wall = (x as f32 + 0.5 - report.wall_column) * report.low_side as f32;
        if from_wall < COLUMN_MARGIN as f32 {
            continue;
        }
        let slot = usize::from(from_wall > report.band_pixels);
        let differs = changed(a, b, x, y);
        counts[slot].0 += usize::from(differs);
        counts[slot].1 += 1;
        if !differs {
            unchanged.put_pixel(x, y, image::Luma([255]));
        }
    }
    (counts, unchanged)
}

/// G-1: a unit drawn tangent to a cliff wall on the low side is not cut by the cliff.
#[test]
fn a_body_drawn_tangent_to_a_wall_is_not_cut_by_the_cliff() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::offscreen_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD]);
    command.env(OUT_VAR, directory.path());
    let driver = command.driver().map_or("unknown", gpu_run::GpuDriver::name);
    command.assert_passes_validation(CHILD);

    let report: Report = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    let (a, b) = (shot(directory.path(), "A")?, shot(directory.path(), "B")?);
    let sigma = image::open(directory.path().join("sigma.png"))?.to_luma8();
    let (counts, unchanged) = tally(&report, &sigma, &a, &b);
    if let Some(keep) = std::env::var_os(KEEP_VAR) {
        let target = PathBuf::from(keep).join(driver);
        std::fs::create_dir_all(&target)?;
        for entry in std::fs::read_dir(directory.path())? {
            let entry = entry?;
            std::fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
        unchanged.save(target.join("unchanged.png"))?;
    }
    let pixels_per_unit = report.band_pixels / BAND_WIDTH;
    println!(
        "{driver}: wall column {:.1}, band {:.1} px ({pixels_per_unit:.1} px per world unit), \
         band {}/{} changed, beyond {}/{} changed",
        report.wall_column, report.band_pixels, counts[0].0, counts[0].1, counts[1].0, counts[1].1
    );

    // 1. Tangency: the sphere's nearest point to the wall's plane lies on the low side, on it.
    let gap = (report.centre[0] - report.edge) * report.low_side as f32 - report.radius;
    assert!(
        (0.0..=TANGENCY_TOLERANCE).contains(&gap),
        "{driver}: the drawn sphere is {gap} from the wall's plane, not tangent on the low side"
    );
    let share = |(changed, total): (usize, usize)| changed as f64 / total.max(1) as f64;
    // 3. Control first: the bubble is seen where no wall can hide it.
    assert!(
        counts[1].1 > 0 && share(counts[1]) >= MIN_CHANGED_SHARE,
        "{driver}: control: only {}/{} silhouette pixels beyond the band change",
        counts[1].0,
        counts[1].1
    );
    // 2. No cut beside the wall.
    assert!(
        counts[0].1 > 0 && share(counts[0]) >= MIN_CHANGED_SHARE,
        "{driver}: the cliff hides {} of {} silhouette pixels within {BAND_WIDTH} world units \
         of the wall ({:.1}% changed, {MIN_CHANGED_SHARE} required)",
        counts[0].1 - counts[0].0,
        counts[0].1,
        100.0 * share(counts[0])
    );
    Ok(())
}
