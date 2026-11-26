//! A reused render entity on a real GPU: its pixels are a fresh bubble's, bit for bit (plan
//! `2026-11-29-render-entity-pool` R-8).
//!
//! # Responsibility
//! Proves on the real bubble shader, the real indicator materials and the shipped media that a
//! unit drawn by the parked render entity of a unit that left looks exactly as it would on a
//! freshly spawned one. Two offscreen children of this binary run the production client
//! (`support/battle_harness.rs` with a renderer) on the default battle with every mobile unit
//! killed, the smog volume off (`LabState::opt_smog_volume`, the production toggle), and two
//! subject pairs in one view (`support/reuse_stage.rs`): a dirty ground decoy (cloaked, selected,
//! at 30 % life, attacking a durable dummy) replaced by an air unit, and a dirty air decoy
//! replaced by a ground unit, each new unit selected and at 60 % life, arriving after 5 ticks
//! hidden. Child `reused` lets the arrivals take the decoys' parked entities; child `fresh`
//! releases the parked entities first, so they are spawned. Each child captures three frames
//! with the clock stopped: `decoys` (the decoys' last frame), `before` (the frame before the
//! arrivals) and `after` (the arrivals drawn).
//!
//! # How it fits
//! The children re-execute this binary through `support/gpu_run.rs` (charter §7): radv with the
//! shared lock locally, lavapipe in CI. They render into an offscreen canvas
//! (`client::offscreen`), the window having no surface. The headless half, field by field, is
//! `render_pool_tests.rs` (R-3).
//!
//! # Invariants verified
//! - The two children's `after` captures are bit-identical (the effect lab's decal identity test
//!   shows presentation is deterministic run to run on one driver).
//! - Controls: in each child at least [`FOOTPRINT_SHARE`] of each arrival's footprint changes from
//!   `before` to `after` (the test sees the bubbles), and the `decoys` capture differs from the
//!   `after` capture in each footprint (stale state would be visible).
//! - The `reused` child reused two entities and the `fresh` child spawned two.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
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
#[path = "support/gpu_run.rs"]
mod gpu_run;
#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/reuse_stage.rs"]
mod reuse_stage;
#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/unit_cycling.rs"]
mod unit_cycling;

use std::path::{Path, PathBuf};

use battle_harness::Harness;
use bevy::prelude::*;
use bevy::render::view::screenshot::save_to_disk;
use client::battle_selection::BattleSelection;
use client::offscreen::{CaptureTarget, OffscreenCanvas};
use client::scene_controller::RenderEntityMap;
use client::{ClientMode, RtsCamera};
use reuse_stage::{Arrival, Decoy, Kinds, Script};
use serde::{Deserialize, Serialize};
use sim_types::{EntityId, SimVec2};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying a child's output directory.
const OUT_VAR: &str = "SPACE_SCUFFLE_RENDER_POOL_OUT";
/// Environment variable that keeps both children's captures in a directory for review.
const KEEP_VAR: &str = "SPACE_SCUFFLE_RENDER_POOL_KEEP";
/// Capture size.
const SIZE: UVec2 = UVec2::new(1280, 720);
/// The child whose arrivals reuse parked entities.
const REUSED_CHILD: &str = "render_pool_reused_child";
/// The child whose arrivals are spawned fresh.
const FRESH_CHILD: &str = "render_pool_fresh_child";
/// The captures each child writes, in order.
const SHOTS: [&str; 3] = ["decoys", "before", "after"];
/// File of a child's [`Report`].
const REPORT_FILE: &str = "report.ron";
/// Ticks the decoys fight before they leave.
const DECOY_TICKS: u32 = 30;
/// Ticks between the decoys' departure and the arrivals.
const HIDDEN_TICKS: u32 = 5;
/// Least share of an arrival's footprint whose pixels change from `before` to `after`.
const FOOTPRINT_SHARE: f64 = 0.2;
/// Frames a capture may take to reach the disk.
const CAPTURE_FRAMES: usize = 120;
/// Frames the battle gets to load its media.
const READY_FRAMES: usize = 600;

/// What a child reports besides its captures.
#[derive(Debug, Serialize, Deserialize)]
struct Report {
    /// Window-pixel footprint of each arrival: `[x0, y0, x1, y1]`.
    footprints: Vec<[u32; 4]>,
    /// Entities the pool reused and spawned while the arrivals came.
    reused: u32,
    spawned: u32,
}

// --- Child ---------------------------------------------------------------------------------------

/// Steps until the battle is ready, with its media loaded and its first snapshot drawn.
fn ready(harness: &mut Harness) {
    for _ in 0..READY_FRAMES {
        let world = harness.app.world();
        let images = world.resource::<Assets<Image>>();
        let registry = world.resource::<client::FlipbookRegistry>();
        let loaded = !registry.textures_by_id.is_empty()
            && registry
                .textures_by_id
                .values()
                .all(|image| images.contains(image));
        if loaded && !world.resource::<RenderEntityMap>().is_empty() {
            return;
        }
        harness.step(1);
    }
    panic!("the battle never became ready");
}

/// Runs `ticks` ticks, one a frame, and pauses.
fn run_ticks(harness: &mut Harness, ticks: u32) {
    let goal = harness.world().current_tick + ticks;
    harness.resume();
    while harness.world().current_tick < goal {
        harness.step(1);
    }
    harness.pause();
}

/// Captures the canvas into `directory/name.png` with the clock stopped: virtual time is paused
/// while the capture travels, so every frame until it lands draws the same scene.
fn capture(harness: &mut Harness, directory: &Path, name: &str) {
    let path = directory.join(format!("{name}.png"));
    harness
        .app
        .world_mut()
        .resource_mut::<Time<Virtual>>()
        .pause();
    let target = harness.app.world().resource::<CaptureTarget>().clone();
    harness
        .app
        .world_mut()
        .spawn(target.screenshot())
        .observe(save_to_disk(path.clone()));
    for _ in 0..CAPTURE_FRAMES {
        if path.is_file() {
            break;
        }
        harness.step(1);
    }
    assert!(path.is_file(), "the capture {name} reached the disk");
    harness
        .app
        .world_mut()
        .resource_mut::<Time<Virtual>>()
        .unpause();
}

/// The window-pixel square round the bubble of unit `id`, half a side of `half` world units.
fn footprint(harness: &mut Harness, id: EntityId, half: f32) -> [u32; 4] {
    let world = harness.app.world();
    let entity = world
        .resource::<RenderEntityMap>()
        .get(id)
        .expect("the arrival is drawn");
    let centre = world
        .get::<GlobalTransform>(entity)
        .expect("a transform")
        .translation();
    let corners = [
        centre + Vec3::new(-half, -half, -half),
        centre + Vec3::new(half, half, half),
        centre + Vec3::new(-half, half, half),
        centre + Vec3::new(half, -half, -half),
    ];
    let points: Vec<Vec2> = corners
        .iter()
        .map(|corner| harness.screen_position(*corner))
        .collect();
    let (min, max) = points.iter().fold(
        (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
        |(min, max), point| (min.min(*point), max.max(*point)),
    );
    let clamp = |value: f32, edge: u32| value.clamp(0.0, (edge - 1) as f32) as u32;
    [
        clamp(min.x, SIZE.x),
        clamp(min.y, SIZE.y),
        clamp(max.x, SIZE.x),
        clamp(max.y, SIZE.y),
    ]
}

/// The two subject pairs at the centre of the map: a ground decoy replaced by an air unit, and an
/// air decoy replaced by a ground unit, each arrival where its decoy stood.
fn scripts(centre: SimVec2) -> [Script; 2] {
    let left = centre + SimVec2::from_i32(-5, 0);
    let right = centre + SimVec2::from_i32(5, 0);
    // The second pair's owners are the first's swapped, so the pairs mix owners both ways.
    [
        Script {
            decoy: Decoy::Ground,
            arrival: Arrival::Air,
            hidden_ticks: HIDDEN_TICKS,
            decoy_at: left,
            arrival_at: left,
            decoy_owner: reuse_stage::LOCAL,
            arrival_owner: reuse_stage::ENEMY,
        },
        Script {
            decoy: Decoy::Air,
            arrival: Arrival::Ground,
            hidden_ticks: HIDDEN_TICKS,
            decoy_at: right,
            arrival_at: right,
            decoy_owner: reuse_stage::ENEMY,
            arrival_owner: reuse_stage::LOCAL,
        },
    ]
}

/// Replaces the selection with `ids`.
fn select(harness: &mut Harness, ids: impl IntoIterator<Item = EntityId>) {
    let mut selection = harness.app.world_mut().resource_mut::<BattleSelection>();
    selection.clear();
    selection.extend(ids);
}

/// Runs one child: `fresh` releases the parked entities before the arrivals.
fn run_child(fresh: bool) {
    let Some(directory) = std::env::var_os(OUT_VAR).map(PathBuf::from) else {
        println!("{OUT_VAR} is not set: this child runs only under its parent");
        return;
    };
    let mut harness = Harness::boot_on_gpu(
        ClientMode::LiveBattle,
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO,
    );
    let canvas = OffscreenCanvas::install(&mut harness.app, SIZE).expect("a canvas");
    {
        let world = harness.app.world_mut();
        let mut cameras = world.query_filtered::<&mut Camera, With<RtsCamera>>();
        for mut camera in cameras.iter_mut(world) {
            camera.target = CaptureTarget::Image(canvas.image.clone()).camera_target();
        }
        world.resource_mut::<client::LabState>().opt_smog_volume = false;
    }
    ready(&mut harness);
    let assets = battle_harness::assets();
    let kinds = Kinds::find(harness.world_mut(), &assets);
    unit_cycling::kill_mobile_units(harness.world_mut());
    run_ticks(&mut harness, 2);
    let centre = unit_cycling::map_centre(harness.world());
    harness.look_at(Vec2::new(centre.x.to_f32(), centre.y.to_f32()));
    let scripts = scripts(centre);
    let staged: Vec<_> = scripts
        .iter()
        .map(|script| script.stage_decoy(harness.world_mut(), &kinds))
        .collect();
    // The sandbox selection takes any unit; both decoys draw selected.
    select(&mut harness, staged.iter().map(|staged| staged.decoy));
    run_ticks(&mut harness, DECOY_TICKS);
    capture(&mut harness, &directory, SHOTS[0]);
    for staged in &staged {
        Script::kill_decoy(harness.world_mut(), *staged);
    }
    run_ticks(&mut harness, HIDDEN_TICKS);
    capture(&mut harness, &directory, SHOTS[1]);
    if fresh {
        let world = harness.app.world_mut();
        world.resource_scope(|world, mut map: Mut<'_, RenderEntityMap>| {
            let mut commands = world.commands();
            map.release_parked(&mut commands);
        });
        world.flush();
    }
    let before = harness.app.world().resource::<RenderEntityMap>().stats();
    let arrivals: Vec<EntityId> = scripts
        .iter()
        .map(|script| script.arrive(harness.world_mut(), &kinds))
        .collect();
    select(&mut harness, arrivals.iter().copied());
    run_ticks(&mut harness, 1);
    let after = harness.app.world().resource::<RenderEntityMap>().stats();
    capture(&mut harness, &directory, SHOTS[2]);
    let footprints = arrivals
        .iter()
        .map(|id| footprint(&mut harness, *id, 1.5))
        .collect();
    let report = Report {
        footprints,
        reused: after.reused - before.reused,
        spawned: after.spawned - before.spawned,
    };
    std::fs::write(
        directory.join(REPORT_FILE),
        ron::to_string(&report).expect("a report serializes"),
    )
    .expect("the report is written");
}

#[test]
#[ignore = "a GPU child: render_pool_gpu_tests' parent runs it"]
fn render_pool_reused_child() {
    run_child(false);
}

#[test]
#[ignore = "a GPU child: render_pool_gpu_tests' parent runs it"]
fn render_pool_fresh_child() {
    run_child(true);
}

// --- Parent --------------------------------------------------------------------------------------

/// Reads capture `name` of the child that wrote `directory`.
fn shot(directory: &Path, name: &str) -> TestResult<image::RgbImage> {
    Ok(image::open(directory.join(format!("{name}.png")))?.to_rgb8())
}

/// Reads the report of the child that wrote `directory`.
fn report(directory: &Path) -> TestResult<Report> {
    Ok(ron::from_str(&std::fs::read_to_string(
        directory.join(REPORT_FILE),
    )?)?)
}

/// Pixels of `patch` that differ between `a` and `b`, and the patch's area.
fn changed(a: &image::RgbImage, b: &image::RgbImage, patch: [u32; 4]) -> (usize, usize) {
    let [x0, y0, x1, y1] = patch;
    let mut differing = 0;
    let mut area = 0;
    for y in y0..=y1 {
        for x in x0..=x1 {
            area += 1;
            differing += usize::from(a.get_pixel(x, y) != b.get_pixel(x, y));
        }
    }
    (differing, area)
}

/// Runs child `name` offscreen with its output in `directory`.
fn run(name: &str, directory: &Path) -> TestResult<Option<gpu_run::GpuDriver>> {
    let mut command = gpu_run::offscreen_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", name]);
    command.env(OUT_VAR, directory);
    let driver = command.driver();
    command.assert_passes_validation(name);
    Ok(driver)
}

/// R-8: the arrivals on reused entities render bit-identically to fresh ones.
#[test]
fn a_reused_render_entity_renders_as_a_fresh_one() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let reused = tempfile::tempdir()?;
    let fresh = tempfile::tempdir()?;
    let driver = run(REUSED_CHILD, reused.path())?;
    run(FRESH_CHILD, fresh.path())?;
    let driver = driver.map_or("unknown", gpu_run::GpuDriver::name);
    if let Some(keep) = std::env::var_os(KEEP_VAR) {
        for (label, directory) in [("reused", reused.path()), ("fresh", fresh.path())] {
            let target = PathBuf::from(&keep).join(driver).join(label);
            std::fs::create_dir_all(&target)?;
            for name in SHOTS {
                let file = format!("{name}.png");
                std::fs::copy(directory.join(&file), target.join(&file))?;
            }
        }
    }
    let (reused_report, fresh_report) = (report(reused.path())?, report(fresh.path())?);
    assert_eq!(
        (reused_report.reused, reused_report.spawned),
        (2, 0),
        "the reused child's arrivals took the parked entities"
    );
    assert_eq!(
        (fresh_report.reused, fresh_report.spawned),
        (0, 2),
        "the fresh child's arrivals were spawned"
    );
    assert_eq!(reused_report.footprints, fresh_report.footprints);
    for (label, directory, report) in [
        ("reused", reused.path(), &reused_report),
        ("fresh", fresh.path(), &fresh_report),
    ] {
        let (decoys, before, after) = (
            shot(directory, "decoys")?,
            shot(directory, "before")?,
            shot(directory, "after")?,
        );
        for (index, patch) in report.footprints.iter().enumerate() {
            let (differing, area) = changed(&before, &after, *patch);
            assert!(
                differing as f64 >= FOOTPRINT_SHARE * area as f64,
                "{driver} {label}: arrival {index} changed {differing} of {area} footprint pixels"
            );
            let (stale, _) = changed(&decoys, &after, *patch);
            assert!(
                stale > 0,
                "{driver} {label}: arrival {index} draws exactly what its decoy drew"
            );
        }
    }
    let (reused_after, fresh_after) = (shot(reused.path(), "after")?, shot(fresh.path(), "after")?);
    let differing = reused_after
        .pixels()
        .zip(fresh_after.pixels())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differing, 0,
        "{driver}: the reused and fresh arrivals differ in {differing} pixels"
    );
    println!(
        "{driver}: two arrivals on reused render entities render bit-identically to fresh ones \
         ({} x {} pixels)",
        SIZE.x, SIZE.y
    );
    Ok(())
}
