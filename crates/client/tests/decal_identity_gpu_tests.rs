//! The decal identity scene on a real GPU (plan `2026-11-19-retained-effect-instances` WP1, D-9,
//! D-10).
//!
//! # Responsibility
//! One child of this binary runs the production effect lab (`client::effects::lab`) offscreen
//! through `support/gpu_run.rs` (radv with the shared lock, or lavapipe), at 1280×720 under the
//! battle camera on the bright ground, with a copy of the shipped vocabulary whose decal budget
//! and class caps are tightened, and a [`LabCrowd`] that exercises the inputs of a lasting decal
//! the shipped vocabulary lets a picture show (independent review, round 1: no shipped tether has
//! a decal, the released look's decals end before its release, and no line is merged here; those
//! paths are pinned by the decal table's dirty-coverage unit tests instead):
//! - every shipped effect with a decal layer (found from the catalog, none named), on a grid,
//!   spawned or held by its event kind, and spawned again in two later waves so that the
//!   tightened decal budget retires marks and the tightened class caps retire details;
//! - two lines whose first search points are fogged (`line_seen`);
//! - a held area whose fog clip sweeps across it every frame (`set_fog_clip`);
//! - a held status or mode look released mid-take (`release`);
//! - a tether whose two ends move apart (`update_held_line`), and a held look that moves
//!   (`update_held`);
//! - a lasting mark whose anchor moves (`move_anchor`);
//! - a camera that steps outward, so level of detail turns small decals off.
//!
//! The take is captured [`SHOTS`] times over [`TAKE_SECONDS`] s, and the whole take is rendered
//! twice, by two child processes (a second take in one process starts at another absolute
//! `Time<Virtual>`, whose rounding moves the lab's steps by nanoseconds). The parent asserts that both renders are bit-identical, frame by frame
//! (D-9: presentation is deterministic run to run on one driver), and that the scene did what it
//! is meant to (marks and details retired, decals skipped by level of detail). With
//! [`SAVE_VAR`] set to a directory it copies the first render's PNGs there, under the driver's
//! name, as the base pictures the later work packages are compared with (WP1 evidence).
//!
//! # Invariants verified
//! As listed; the stats line is printed for the evidence.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

#[allow(dead_code, reason = "shared with the catalog tests")]
#[path = "support/effect_subjects.rs"]
mod effect_subjects;

#[allow(dead_code, reason = "shared with the decal aging tests")]
#[path = "support/decal_identity_scene.rs"]
mod decal_identity_scene;

use client::effects::lab::{lab_app, LabSurface};
use client::effects::runtime::EffectRuntime;
use client::ResolvedAssetsRoot;
use decal_identity_scene::{scene, tightened_vocabulary, SHOTS, SIZE};
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying the child's output directory.
const OUT_VAR: &str = "SPACE_SCUFFLE_DECAL_IDENTITY_OUT";
/// Environment variable naming a directory the base pictures are copied into.
const SAVE_VAR: &str = "SPACE_SCUFFLE_DECAL_IDENTITY_SAVE";
/// File the child writes the runtime's counters into.
const STATS_FILE: &str = "stats.txt";
/// The two renders' file prefixes: one child process each.
const RENDERS: [&str; 2] = ["a", "b"];
/// Environment variable carrying the child's render prefix.
const RENDER_VAR: &str = "SPACE_SCUFFLE_DECAL_IDENTITY_RENDER";

/// The child: renders the identity scene's take once and writes the runtime's counters.
#[test]
#[ignore = "launched by the decal identity test through gpu_run, offscreen"]
fn decal_identity_child() {
    let out = PathBuf::from(std::env::var_os(OUT_VAR).expect("the parent names the directory"));
    let vocabulary = tightened_vocabulary(&out);
    let render = std::env::var(RENDER_VAR).expect("the parent names the render");
    let (program, crowd) = scene(out.clone(), &render);
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let mut app = lab_app(root, program, Some(vocabulary), SIZE, LabSurface::Offscreen)
        .expect("the lab app builds");
    app.insert_resource(crowd);
    // The run consumes the world, so the counters are copied out every frame.
    let shared = std::sync::Arc::new(std::sync::Mutex::new(None));
    let sink = shared.clone();
    app.add_systems(
        bevy::app::Last,
        move |runtime: Option<bevy::ecs::system::Res<'_, EffectRuntime>>| {
            if let (Some(runtime), Ok(mut stats)) = (runtime, sink.lock()) {
                *stats = Some(runtime.stats);
            }
        },
    );
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
    let stats = shared
        .lock()
        .ok()
        .and_then(|stats| *stats)
        .expect("the runtime ran");
    std::fs::write(
        out.join(format!("{render}_{STATS_FILE}")),
        format!(
            "marks_retired {}\nretired {}\nlod_skipped {}\nmarks_refused {}\ncritical_overflow {}\n",
            stats.marks_retired,
            stats.retired,
            stats.lod_skipped,
            stats.marks_refused,
            stats.critical_overflow
        ),
    )
    .expect("the stats write");
}

/// Reads counter `name` from the child's stats file.
fn stat(directory: &Path, name: &str) -> u64 {
    let text = std::fs::read_to_string(directory.join(format!("a_{STATS_FILE}")))
        .expect("the child's stats");
    text.lines()
        .find_map(|line| {
            let (key, value) = line.split_once(' ')?;
            (key == name).then(|| value.parse().ok()).flatten()
        })
        .unwrap_or_else(|| panic!("no {name} in the stats"))
}

/// D-9: the identity scene renders the same pictures in two runs, and the scene
/// exercises mark and detail retirement and level of detail. Saves the base with [`SAVE_VAR`].
#[test]
fn the_identity_scene_renders_the_same_pictures_twice() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut driver = None;
    for render in RENDERS {
        let mut command = gpu_run::offscreen_gpu_command(&std::env::current_exe()?);
        command.args([
            "--ignored",
            "--exact",
            "--nocapture",
            "decal_identity_child",
        ]);
        command.env(OUT_VAR, directory.path());
        command.env(RENDER_VAR, render);
        driver = command.driver();
        command.assert_passes_validation(&format!("decal identity scene, render {render}"));
    }
    let driver = driver.map_or("unknown", gpu_run::GpuDriver::name);
    let mut lit_frames = 0;
    for shot in 0..SHOTS {
        let first = image::open(directory.path().join(format!("a_{shot:02}.png")))?.to_rgb8();
        let second = image::open(directory.path().join(format!("b_{shot:02}.png")))?.to_rgb8();
        let differing = first
            .pixels()
            .zip(second.pixels())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            differing, 0,
            "{driver}: capture {shot} differs between the two renders in {differing} pixels"
        );
        if shot > 0 {
            let previous =
                image::open(directory.path().join(format!("a_{:02}.png", shot - 1)))?.to_rgb8();
            lit_frames += usize::from(previous != first);
        }
    }
    assert!(lit_frames > 0, "{driver}: the scene changes over the take");
    let (marks_retired, retired, lod_skipped) = (
        stat(directory.path(), "marks_retired"),
        stat(directory.path(), "retired"),
        stat(directory.path(), "lod_skipped"),
    );
    println!(
        "{driver}: decal identity scene, {SHOTS} captures twice, bit-identical; marks retired \
         {marks_retired}, details retired {retired}, LOD skipped {lod_skipped}, marks refused {}, \
         critical overflow {}",
        stat(directory.path(), "marks_refused"),
        stat(directory.path(), "critical_overflow"),
    );
    assert!(
        marks_retired > 0,
        "the tightened decal budget retires marks"
    );
    assert!(retired > 0, "the tightened class caps retire details");
    assert!(
        lod_skipped > 0,
        "the camera stepping outward skips small layers"
    );
    if let Some(save) = std::env::var_os(SAVE_VAR) {
        let target = PathBuf::from(save).join(driver);
        std::fs::create_dir_all(&target)?;
        for shot in 0..SHOTS {
            let name = format!("a_{shot:02}.png");
            std::fs::copy(directory.path().join(&name), target.join(&name))?;
        }
        let stats = format!("a_{STATS_FILE}");
        std::fs::copy(directory.path().join(&stats), target.join(STATS_FILE))?;
    }
    Ok(())
}
