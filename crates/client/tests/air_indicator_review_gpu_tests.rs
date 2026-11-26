//! Review captures of the air shadow and tether over a cliff (plan
//! `docs/plans/2026-11-30-terrain-and-air-indicators.md` §6.2, WP2): pictures for the owner, not a
//! test of the gate.
//!
//! # Responsibility
//! When `SPACE_SCUFFLE_AIR_REVIEW_OUT` names a directory, the ignored parent
//! [`air_indicator_review`] runs one offscreen GPU child through `support/gpu_run.rs` (charter
//! §7) that boots the production client on the shipped battle (`support/gpu_battle.rs`), kills
//! the mobile units and captures, into `<directory>/<driver>/`:
//! - `before`, `over`, `after`: the air unit of I-1 (`support/air_crossing.rs`) written two cells
//!   before the crossing's cliff, over its edge, and three cells past it;
//! - `radii`: one selectable air unit with media of each drawn radius, in a row over flat ground.
//!
//! Without the variable the parent does nothing, and both tests are ignored, so the gate runs
//! nothing here. The assertions on the same pose are `air_indicator_tests.rs` (U-2, I-1).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/air_crossing.rs"]
mod air_crossing;
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

use air_crossing::{smallest_air_unit, Crossing, PAST_CELLS, START_CELLS};
use battle_harness::{Harness, LOCAL_PLAYER};
use bevy::prelude::*;
use client::runtime_battle::DEFAULT_BATTLE_SCENARIO;
use sim_types::{EntityId, MoverType, ShapeKind, SimScalar, UnitTypeId};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the directory the review captures go to; the review parent does
/// nothing without it.
const REVIEW_VAR: &str = "SPACE_SCUFFLE_AIR_REVIEW_OUT";

/// Environment variable carrying the review child's output directory.
const REVIEW_CHILD_VAR: &str = "SPACE_SCUFFLE_AIR_REVIEW_CHILD_OUT";

/// The review child.
const REVIEW_CHILD: &str = "air_indicator_review_child";

/// Review capture size.
const REVIEW_SIZE: UVec2 = UVec2::new(1280, 720);

/// Camera distance of the review captures, world units: the unit, its shadow and the cliff in one
/// view.
const REVIEW_DISTANCE: f32 = 12.0;

/// Frames between a staged change and its capture (the camera's look height eases).
const REVIEW_SETTLE_FRAMES: usize = 40;

/// Spacing of the air units of the radius line-up, world units.
const LINE_UP_SPACING: f32 = 3.0;

/// Rows between the crossing and the radius line-up.
const LINE_UP_OFFSET: f32 = 4.0;

/// Points the camera at `target` from [`REVIEW_DISTANCE`] and lets it settle.
fn review_camera(harness: &mut Harness, target: Vec2) {
    {
        let world = harness.app.world_mut();
        let mut camera = world.query::<&mut client::RtsCamera>().single_mut(world);
        camera.target = target;
        camera.distance = REVIEW_DISTANCE;
    }
    harness.step(REVIEW_SETTLE_FRAMES);
}

/// Writes unit `id` to `at`, runs one tick so the snapshot carries it, and lets it settle.
fn review_place(harness: &mut Harness, id: EntityId, at: Vec2) {
    harness
        .world_mut()
        .entities
        .get_mut(&id)
        .expect("the staged unit")
        .position = battle_harness::sim_position(at);
    gpu_battle::run_ticks(harness, 1);
    harness.step(REVIEW_SETTLE_FRAMES);
}

/// The review child: an air unit before, over and after the I-1 cliff, then one air unit of each
/// drawn radius with media in a row over flat ground.
fn run_review_child() {
    let Some(directory) = std::env::var_os(REVIEW_CHILD_VAR).map(std::path::PathBuf::from) else {
        println!("{REVIEW_CHILD_VAR} is not set: this child runs only under its parent");
        return;
    };
    let mut harness = gpu_battle::boot_on_gpu(DEFAULT_BATTLE_SCENARIO, REVIEW_SIZE);
    gpu_battle::ready(&mut harness);
    unit_cycling::kill_mobile_units(harness.world_mut());
    gpu_battle::run_ticks(&mut harness, 2);
    let crossing = Crossing::find(&harness.world().map, &[]);
    let edge = Vec2::new(crossing.x as f32 + 1.0, crossing.y as f32 + 0.5);
    let unit_type = smallest_air_unit(&harness);
    let start = crossing.at(crossing.x - START_CELLS);
    let id = harness
        .world_mut()
        .spawn_unit(unit_type, LOCAL_PLAYER, battle_harness::sim_position(start))
        .expect("an air unit can be placed over any cell");
    review_camera(&mut harness, edge);
    for (name, at) in [
        ("before", start),
        ("over", edge),
        ("after", crossing.at(crossing.x + 1 + PAST_CELLS)),
    ] {
        review_place(&mut harness, id, at);
        gpu_battle::capture(&mut harness, &directory, name);
    }
    harness
        .world_mut()
        .entities
        .get_mut(&id)
        .expect("the unit")
        .hp = SimScalar::ZERO;
    line_up(&mut harness, crossing, &directory);
}

/// One air unit of each drawn radius with media, in a row on the far side of the crossing.
fn line_up(harness: &mut Harness, crossing: Crossing, directory: &std::path::Path) {
    let assets = battle_harness::assets();
    let mut kinds: Vec<(SimScalar, UnitTypeId)> = harness
        .world()
        .catalog
        .archetypes()
        .filter(|a| {
            a.mover_type == MoverType::Air
                && a.selectable
                && matches!(a.shape, ShapeKind::UnitSphere)
                && battle_harness::presentable(&assets, &a.slug)
        })
        .map(|a| (a.drawn_radius(), a.unit_type_id))
        .collect();
    kinds.sort();
    kinds.dedup_by_key(|(radius, _)| *radius);
    // The row lies LINE_UP_OFFSET rows off the crossing, on the side the map has room for.
    let row = crossing.at(crossing.x - START_CELLS);
    let offset = if row.y > LINE_UP_OFFSET + 1.0 {
        -LINE_UP_OFFSET
    } else {
        LINE_UP_OFFSET
    };
    let span = LINE_UP_SPACING * (kinds.len() as f32 - 1.0);
    let width = harness.world().map.width() as f32;
    let first = Vec2::new(
        (row.x - span * 0.5).clamp(1.0, width - 1.0 - span),
        row.y + offset,
    );
    for (k, (_, unit_type)) in kinds.iter().enumerate() {
        let at = first + Vec2::new(LINE_UP_SPACING * k as f32, 0.0);
        harness
            .world_mut()
            .spawn_unit(*unit_type, LOCAL_PLAYER, battle_harness::sim_position(at))
            .expect("an air unit can be placed over any cell");
    }
    gpu_battle::run_ticks(harness, 1);
    review_camera(
        harness,
        first + Vec2::new(LINE_UP_SPACING * (kinds.len() as f32 - 1.0) * 0.5, 0.0),
    );
    gpu_battle::capture(harness, directory, "radii");
}

#[test]
#[ignore = "a GPU child of air_indicator_review"]
fn air_indicator_review_child() {
    run_review_child();
}

/// The WP2 review captures (plan §6.2): set [`REVIEW_VAR`] to a directory and run this ignored
/// test; the captures land in `<directory>/<driver>/`.
#[test]
#[ignore = "review captures only: run with SPACE_SCUFFLE_AIR_REVIEW_OUT set"]
fn air_indicator_review() -> TestResult {
    let Some(out) = std::env::var_os(REVIEW_VAR).map(std::path::PathBuf::from) else {
        println!("{REVIEW_VAR} is not set: nothing to capture");
        return Ok(());
    };
    let mut command = gpu_run::offscreen_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", REVIEW_CHILD]);
    let driver = command.driver().map_or("unknown", gpu_run::GpuDriver::name);
    let directory = out.join(driver);
    std::fs::create_dir_all(&directory)?;
    command.env(REVIEW_CHILD_VAR, &directory);
    command.assert_passes_validation(REVIEW_CHILD);
    Ok(())
}
