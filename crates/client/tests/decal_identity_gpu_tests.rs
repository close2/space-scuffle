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

use bevy::math::{UVec2, Vec2, Vec3};
use client::effects::binding::BoundValues;
use client::effects::catalog::schema::{Binding, EventKind};
use client::effects::catalog::{EffectCatalog, Pool, EFFECTS_CATALOG_PATH};
use client::effects::lab::{
    lab_app, LabCrowd, LabCrowdMember, LabFogSweep, LabGround, LabProgram, LabShot, LabSubject,
    LabSurface, LabTake, LabZoom,
};
use client::effects::runtime::EffectRuntime;
use client::ResolvedAssetsRoot;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying the child's output directory.
const OUT_VAR: &str = "SPACE_SCUFFLE_DECAL_IDENTITY_OUT";
/// Environment variable naming a directory the base pictures are copied into.
const SAVE_VAR: &str = "SPACE_SCUFFLE_DECAL_IDENTITY_SAVE";
/// Capture size.
const SIZE: UVec2 = UVec2::new(1280, 720);
/// Captures per render.
pub const SHOTS: usize = 16;
/// Length of the take, seconds.
pub const TAKE_SECONDS: f32 = 9.0;
/// Age of the first capture, seconds.
const FIRST_SHOT: f32 = 0.3;
/// Time step of the take, seconds (particles integrate the same steps on both renders).
const STEP: f32 = 0.08;
/// Grid spacing of the scene's effects, world units.
const SPACING: f32 = 2.6;
/// Length of a line effect's line, world units.
const LINE_LENGTH: f32 = 2.6;
/// Search point spacing of a line, world units.
const LINE_SPACING: f32 = 0.4;
/// Search points before the first one a fogged line shows.
const FOGGED_POINTS: u32 = 3;
/// Ages of the later waves of spawns, seconds.
const WAVES: [f32; 2] = [3.0, 6.0];
/// Age at which the held look is released, seconds.
const RELEASE_AT: f32 = 4.5;
/// The camera's distance grows by this share of the default per second.
const DOLLY: f32 = 0.25;
/// The tightened decal budget of the scene's vocabulary copy.
const DECAL_BUDGET: u32 = 160;
/// The tightened class caps (`Major`, `Minor`, `Ambient`).
const CLASS_CAPS: (u32, u32, u32) = (2, 6, 2);
/// File the child writes the runtime's counters into.
const STATS_FILE: &str = "stats.txt";

/// The two renders' file prefixes: one child process each.
const RENDERS: [&str; 2] = ["a", "b"];
/// Environment variable carrying the child's render prefix.
const RENDER_VAR: &str = "SPACE_SCUFFLE_DECAL_IDENTITY_RENDER";

/// The shipped vocabulary.
fn shipped() -> EffectCatalog {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads")
}

/// Writes the shipped vocabulary with the scene's tightened budget into `dir`; returns its path.
fn tightened_vocabulary(dir: &Path) -> PathBuf {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let text = std::fs::read_to_string(root.as_path().join(EFFECTS_CATALOG_PATH))
        .expect("the shipped vocabulary reads");
    let decals = regex_replace(&text, "decals: ", &format!("decals: {DECAL_BUDGET},"));
    let caps = format!(
        "classes: {{ Major: {}, Minor: {}, Ambient: {} }},",
        CLASS_CAPS.0, CLASS_CAPS.1, CLASS_CAPS.2
    );
    let edited = regex_replace(&decals, "classes: ", &caps);
    let path = dir.join("effects.ron");
    std::fs::write(&path, edited).expect("the vocabulary copy writes");
    path
}

/// `text` with the one line that starts (after its indentation) with `key` replaced by `with`,
/// keeping the indentation. Panics unless exactly one line matches.
fn regex_replace(text: &str, key: &str, with: &str) -> String {
    let matching: Vec<usize> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim_start().starts_with(key))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "the shipped vocabulary has one `{key}` line in its budget"
    );
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            if index == matching[0] {
                let indent = &line[..line.len() - line.trim_start().len()];
                format!("{indent}{with}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The bound values a scene member is drawn with: a splash subject's own (the weapon that
/// names it), or a generic event's.
fn bound_of(name: &str, splashes: &[effect_subjects::SplashSubject]) -> BoundValues {
    if let Some(subject) = splashes.iter().find(|subject| subject.effect == name) {
        return subject.bound;
    }
    BoundValues::default()
        .with(Binding::SplashFull, 0.6)
        .with(Binding::SplashMid, 0.9)
        .with(Binding::SplashReach, 1.2)
        .with(Binding::LineLength, LINE_LENGTH)
        .with(Binding::LineSpacing, LINE_SPACING)
        .with(Binding::AreaRadius, 1.5)
        .with(Binding::AreaRemaining, 50.0)
        .with(Binding::AreaTotal, 60.0)
        .with(Binding::LinkLength, LINE_LENGTH)
        .with(Binding::UnitRadius, 0.5)
        .with(Binding::UnitHeight, 1.0)
        .with(Binding::BarrierRatio, 0.5)
}

/// A subject of effect `name` at `anchor`, with a line along +x for a line or tether event.
fn subject(name: &str, kind: EventKind, anchor: Vec3, bound: BoundValues) -> LabSubject {
    let line = matches!(kind, EventKind::Line | EventKind::Link | EventKind::Shot);
    let length = if kind == EventKind::Line {
        bound.get(Binding::LineLength).max(LINE_SPACING)
    } else {
        LINE_LENGTH
    };
    LabSubject {
        effect: name.to_owned(),
        bound,
        anchor,
        line_to: line.then(|| anchor + Vec3::X * length),
        line_seen: u64::MAX,
        fog_below_x: None,
        repeat: None,
    }
}

/// Whether effects of `kind` are held on a unit or a view rather than spawned once.
const fn held_kind(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Area
            | EventKind::Link
            | EventKind::Missile
            | EventKind::Status
            | EventKind::Mode
    )
}

/// The identity scene: the program's subject (the grid's first effect at the centre) and the
/// crowd.
fn scene(out: PathBuf, render: &str) -> (LabProgram, LabCrowd) {
    let catalog = shipped();
    let splashes = effect_subjects::splash_subjects();
    let mut decal_effects: Vec<(String, EventKind)> = catalog
        .effects
        .iter()
        .filter(|effect| {
            effect
                .layers
                .iter()
                .any(|layer| layer.block.pool() == Pool::Decals)
        })
        .map(|effect| (effect.name.clone(), effect.event))
        .collect();
    decal_effects.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(
        decal_effects.len() >= 4,
        "the shipped vocabulary has decal looks"
    );
    let columns = (decal_effects.len() as f32).sqrt().ceil() as usize;
    let cell = |index: usize| {
        let (column, row) = (index % columns, index / columns);
        let half = (columns as f32 - 1.0) * 0.5;
        Vec3::new(
            (column as f32 - half + 0.5) * SPACING,
            0.0,
            (row as f32 - half - 0.5) * SPACING,
        )
    };
    let member = |name: &str, kind: EventKind, anchor: Vec3, at: f32| LabCrowdMember {
        subject: subject(name, kind, anchor, bound_of(name, &splashes)),
        at,
        held: held_kind(kind),
        release_at: None,
        velocity: Vec3::ZERO,
        fog_sweep: None,
    };
    let mut members = Vec::new();
    // The grid, then two waves of the spawned looks beside their first spawn.
    for (index, (name, kind)) in decal_effects.iter().enumerate() {
        members.push(member(name, *kind, cell(index), 0.05 * index as f32));
    }
    for (wave, at) in WAVES.iter().enumerate() {
        let offset = Vec3::new(0.6, 0.0, 0.5) * (wave as f32 + 1.0);
        for (index, (name, kind)) in decal_effects.iter().enumerate() {
            if !held_kind(*kind) {
                members.push(member(
                    name,
                    *kind,
                    cell(index) + offset,
                    at + 0.03 * index as f32,
                ));
            }
        }
    }
    let first_of = |wanted: &[EventKind]| {
        decal_effects
            .iter()
            .find(|(_, kind)| wanted.contains(kind))
            .cloned()
    };
    let below = |row: f32| {
        Vec3::new(
            -2.0 * SPACING,
            0.0,
            (columns as f32 * 0.5 + row * 0.8) * SPACING,
        )
    };
    // Two lines whose first points are fogged.
    for (line, (name, kind)) in decal_effects
        .iter()
        .filter(|(_, kind)| *kind == EventKind::Line)
        .take(2)
        .enumerate()
    {
        let mut fogged = member(name, *kind, below(0.0) + Vec3::X * 4.0 * line as f32, 0.2);
        fogged.subject.line_seen = u64::MAX << FOGGED_POINTS;
        members.push(fogged);
    }
    // An area whose fog clip sweeps across it.
    if let Some((name, kind)) = first_of(&[EventKind::Area]) {
        let mut area = member(&name, kind, below(1.0), 0.1);
        let along = area.subject.anchor.x;
        area.fog_sweep = Some(LabFogSweep {
            axis: Vec2::X,
            from: along - 2.0,
            to: along + 2.0,
            speed: 0.45,
        });
        members.push(area);
    }
    // A held look released mid-take, and one that moves with its unit.
    if let Some((name, kind)) = first_of(&[EventKind::Status, EventKind::Mode]) {
        let mut released = member(&name, kind, below(1.0) + Vec3::X * 4.0, 0.1);
        released.release_at = Some(RELEASE_AT);
        members.push(released);
        let mut moving = member(&name, kind, below(1.0) + Vec3::X * 8.0, 0.1);
        moving.velocity = Vec3::new(0.0, 0.0, -0.3);
        members.push(moving);
    }
    // A tether between moving ends (any tether look: its line is what moves).
    if let Some(link) = catalog
        .effects
        .iter()
        .find(|effect| effect.event == EventKind::Link)
    {
        let mut tether = member(&link.name, EventKind::Link, below(2.0), 0.1);
        tether.velocity = Vec3::new(-0.2, 0.0, 0.0);
        members.push(tether);
    }
    // A lasting mark whose anchor moves (a held beam's burn follows the beam's end).
    if let Some((name, kind)) = first_of(&[EventKind::Splash, EventKind::Point]) {
        let mut burn = member(&name, kind, below(2.0) + Vec3::X * 6.0, 0.15);
        burn.velocity = Vec3::new(0.25, 0.0, 0.0);
        members.push(burn);
    }
    // The lab frames its subject: the first spawned look, at the scene's centre.
    let (main_name, main_kind) = first_of(&[
        EventKind::Splash,
        EventKind::Point,
        EventKind::Death,
        EventKind::Line,
    ])
    .expect("a spawned decal look");
    let main = subject(
        &main_name,
        main_kind,
        Vec3::ZERO,
        bound_of(&main_name, &splashes),
    );
    let takes = std::iter::once(render)
        .map(|render| LabTake {
            ground: LabGround::Bright,
            zoom: LabZoom::Default,
            effect: true,
            only: None,
            step: STEP,
            shots: (0..SHOTS)
                .map(|shot| LabShot {
                    age: shot_age(shot),
                    overlay: false,
                    name: format!("{render}_{shot:02}"),
                })
                .collect(),
        })
        .collect();
    let program = LabProgram {
        subject: main,
        takes,
        out,
    };
    (
        program,
        LabCrowd {
            members,
            dolly_per_second: DOLLY,
        },
    )
}

/// Age of capture `shot`, seconds.
fn shot_age(shot: usize) -> f32 {
    FIRST_SHOT + (TAKE_SECONDS - FIRST_SHOT) * shot as f32 / (SHOTS - 1) as f32
}

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
