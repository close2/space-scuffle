//! Review captures of ground units on a straight diagonal cliff (plan
//! `docs/plans/2026-12-01-straight-diagonal-cliffs.md` §5.2, row 4; owner question Q0139):
//! pictures for the owner, not a test of the gate.
//!
//! # Responsibility
//! When `SPACE_SCUFFLE_DIAGONAL_REVIEW_OUT` names a directory, the ignored parent
//! [`diagonal_cliff_review`] runs one offscreen GPU child through `support/gpu_run.rs` (charter
//! §7) that boots the production client on the shipped battle (`support/gpu_battle.rs`), kills
//! the mobile units and, for each terrain radius of [`REVIEW_RADII`], captures `brink_<r>` into
//! `<directory>/<driver>/`: a ground unit of that terrain radius at the outer corner of a cut
//! cell of the battle map's longest straight diagonal (its centre at clearance 0 from both of
//! the cell's cliff edges, the deepest point the simulation lets it reach, plan D-8), and the
//! same unit type at the centre of the next cut cell along the diagonal. The diagonal is found
//! with the plan's independent analysis (`support/terrain_mesh.rs`), and the unit by catalog
//! property and media, never by slug (charter C5).
//!
//! Without the variable the parent does nothing, and both tests are ignored, so the gate runs
//! nothing here. The figures for the same poses are T-K's (`diagonal_cliff_tests.rs`).

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
#[path = "support/terrain_mesh.rs"]
mod terrain_mesh;
#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/unit_cycling.rs"]
mod unit_cycling;

use battle_harness::{Harness, LOCAL_PLAYER};
use bevy::prelude::*;
use client::runtime_battle::DEFAULT_BATTLE_SCENARIO;
use sim_types::{MoverType, ShapeKind, SimScalar, UnitTypeId};
use terrain_mesh::DiagonalCut;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the directory the review captures go to; the review parent does
/// nothing without it.
const REVIEW_VAR: &str = "SPACE_SCUFFLE_DIAGONAL_REVIEW_OUT";

/// Environment variable carrying the review child's output directory.
const REVIEW_CHILD_VAR: &str = "SPACE_SCUFFLE_DIAGONAL_REVIEW_CHILD_OUT";

/// The review child.
const REVIEW_CHILD: &str = "diagonal_cliff_review_child";

/// Review capture size.
const REVIEW_SIZE: UVec2 = UVec2::new(1280, 720);

/// Camera distance of the review captures, world units: both units and the chamfer in one view.
const REVIEW_DISTANCE: f32 = 8.0;

/// Frames between a staged change and its capture (the camera's look height eases).
const REVIEW_SETTLE_FRAMES: usize = 40;

/// The terrain radii the plan's row 4 shows (D-8's table): the smallest, which overhangs the
/// most, and the one whose centre stands on the chamfer's toe.
const REVIEW_RADII: [f32; 2] = [0.3125, 0.5];

/// The ground unit sphere with media whose terrain radius is `radius`, the one with the smallest
/// drawn radius (ties by type id).
fn unit_of_radius(harness: &Harness, radius: SimScalar) -> UnitTypeId {
    let assets = battle_harness::assets();
    harness
        .world()
        .catalog
        .archetypes()
        .filter(|a| {
            a.mover_type == MoverType::Ground
                && a.selectable
                && a.max_speed > SimScalar::ZERO
                && matches!(a.shape, ShapeKind::UnitSphere)
                && a.terrain_radius == radius
                && battle_harness::presentable(&assets, &a.slug)
        })
        .min_by_key(|a| (a.drawn_radius(), a.unit_type_id))
        .unwrap_or_else(|| panic!("the catalog ships a ground unit of terrain radius {radius}"))
        .unit_type_id
}

/// The two cut cells the review stages: the middle cut of the battle map's longest straight
/// diagonal and the next one along it.
fn staged_cuts(harness: &Harness) -> (DiagonalCut, DiagonalCut) {
    let runs = terrain_mesh::diagonal_runs(&harness.world().map);
    let run = runs
        .iter()
        .max_by_key(|run| run.cuts.len())
        .filter(|run| run.cuts.len() >= 2)
        .expect("the battle map has a straight diagonal of at least two cuts");
    let middle = run.cuts.len() / 2 - 1;
    (run.cuts[middle], run.cuts[middle + 1])
}

/// The centre of cut cell `cut`, world XZ.
fn cell_centre(cut: DiagonalCut) -> Vec2 {
    Vec2::new(cut.cell.0 as f32 + 0.5, cut.cell.1 as f32 + 0.5)
}

/// The review child: for each radius, a unit at a cut cell's outer corner and one at the next
/// cut cell's centre.
fn run_review_child() {
    let Some(directory) = std::env::var_os(REVIEW_CHILD_VAR).map(std::path::PathBuf::from) else {
        println!("{REVIEW_CHILD_VAR} is not set: this child runs only under its parent");
        return;
    };
    let mut harness = gpu_battle::boot_on_gpu(DEFAULT_BATTLE_SCENARIO, REVIEW_SIZE);
    gpu_battle::ready(&mut harness);
    unit_cycling::kill_mobile_units(harness.world_mut());
    gpu_battle::run_ticks(&mut harness, 2);
    let (corner_cut, centre_cut) = staged_cuts(&harness);
    println!("staged cuts: corner {corner_cut:?}, centre {centre_cut:?}");
    for radius in REVIEW_RADII {
        let sim_radius = SimScalar::from_num(radius);
        let unit_type = unit_of_radius(&harness, sim_radius);
        let dir = Vec2::new(corner_cut.dir.0 as f32, corner_cut.dir.1 as f32);
        // At clearance 0 from both cliff edges of the cell: its centre `0.5 - r` from the cell's
        // centre along each axis towards the corner.
        let corner = cell_centre(corner_cut) + dir * (0.5 - radius);
        let centre = cell_centre(centre_cut);
        let mut staged = Vec::new();
        for at in [corner, centre] {
            let id = harness
                .world_mut()
                .spawn_unit(unit_type, LOCAL_PLAYER, battle_harness::sim_position(at))
                .expect("a ground unit can be placed on its plateau");
            harness
                .world_mut()
                .entities
                .get_mut(&id)
                .expect("the staged unit")
                .position = battle_harness::sim_position(at);
            staged.push(id);
        }
        gpu_battle::run_ticks(&mut harness, 1);
        {
            let world = harness.app.world_mut();
            let mut camera = world.query::<&mut client::RtsCamera>().single_mut(world);
            camera.target = (corner + centre) * 0.5;
            camera.distance = REVIEW_DISTANCE;
        }
        harness.step(REVIEW_SETTLE_FRAMES);
        println!("brink_{radius}: unit type {unit_type:?} at {corner} and {centre}");
        gpu_battle::capture(&mut harness, &directory, &format!("brink_{radius}"));
        for id in staged {
            harness
                .world_mut()
                .entities
                .get_mut(&id)
                .expect("the staged unit")
                .hp = SimScalar::ZERO;
        }
        gpu_battle::run_ticks(&mut harness, 2);
    }
}

#[test]
#[ignore = "a GPU child of diagonal_cliff_review"]
fn diagonal_cliff_review_child() {
    run_review_child();
}

/// The review captures of plan §5.2 row 4: set [`REVIEW_VAR`] to a directory and run this
/// ignored test; the captures land in `<directory>/<driver>/`.
#[test]
#[ignore = "review captures only: run with SPACE_SCUFFLE_DIAGONAL_REVIEW_OUT set"]
fn diagonal_cliff_review() -> TestResult {
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
