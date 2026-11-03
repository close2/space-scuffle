//! Counts the heap allocations of the live battle synchroniser's steady state from outside the
//! process, under valgrind (remediation Phase 6 exit 1, layer 3 of
//! `docs/plans/2026-11-03-close-phase-6-7-8-exits.md` D-2; finding C-15).
//!
//! **Responsibility.** Charter §3.3 says battle synchronisation allocates nothing in steady state.
//! The workspace forbids `unsafe_code`, so no counting `#[global_allocator]` can be installed
//! (owner question Q0011). Instead the parent test re-executes this test binary under
//! `valgrind --tool=dhat`, which replaces `malloc` from outside the process and records every
//! heap block with the call stack that allocated it. Every Rust allocation reaches `malloc` through
//! `std::alloc::System`, so the record is complete for the child process.
//!
//! **Why the window is scoped by call stack.** Whole-process totals (memcheck's
//! `total heap usage`) differ by about two blocks between identical runs: Bevy builds each
//! schedule's dependency graph with `std`'s randomly seeded `RandomState` during setup. That
//! noise is outside the frames under test, so the parent counts only the blocks whose stack passes
//! through `measured_steady_frames`, the non-inlined function that runs the measured frames.
//!
//! **How it works.** Each child builds the production live synchroniser on the real default battle
//! with `MinimalPlugins` (the app of `pipeline_presentation_tests.rs`), with every schedule on the
//! single-threaded executor, warms it, runs `F` paused frames and exits. The parent runs each child
//! at two frame counts and compares the window counts `W(F)` through the difference
//! `W(LONG_FRAMES) - W(SHORT_FRAMES)`, the allocations of `LONG_FRAMES - SHORT_FRAMES` frames:
//!
//! - `live_sync_frames` adds the client's battle systems (`advance_battle_ticks`,
//!   `sync_live_battle_system`, `publish_combat_events`);
//! - `baseline_frames` is the same app without them, so its difference is Bevy's own per-frame
//!   work (time, events, the schedule runner, asset bookkeeping), which is reported, not asserted;
//! - `planted_allocation_is_counted` is the full app plus one test system that allocates once per
//!   frame: the positive control, without which a counter that sees nothing would pass.
//!
//! **Invariants asserted.** Two runs of the live child at the same frame count report the same
//! window count (the measurement is deterministic); the full app's difference equals the baseline's (the client's
//! sync systems add no allocation per frame); the planted control adds exactly one allocation per
//! frame; and no recorded stack reached valgrind's depth limit, so no window block can hide in a
//! truncated stack.
//!
//! **Scope and limits.** It counts the paused steady state of the live synchroniser, not the whole
//! client frame: the renderer, egui, the multi-threaded executor and the simulation tick are
//! outside it. A missing `valgrind` fails the parent test with an instruction to install it; there
//! is no skip (charter §1.2). The children return at once unless `SPACE_SCUFFLE_STEADY_FRAMES` is
//! set, so a plain `cargo test` runs them as no-ops.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use bevy::ecs::schedule::ExecutorKind;
use bevy::prelude::*;
use client::battle_pipeline::{
    commands::SessionGeneration,
    driver::{advance_battle_ticks, publish_combat_events},
    session::{BattleSession, CommittedBattleMetadata},
};
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Environment variable that turns a child test on and carries its measured frame count.
const FRAMES_ENV: &str = "SPACE_SCUFFLE_STEADY_FRAMES";
/// Frames run before the measured window, so first-use growth (event queues, render-entity
/// buffers, schedule initialisation) is outside it.
const WARM_FRAMES: u32 = 64;
/// The shorter measured window, in frames.
const SHORT_FRAMES: u32 = 64;
/// The longer measured window, in frames.
const LONG_FRAMES: u32 = 128;
/// Name of the function whose frames delimit the measured window in a recorded call stack.
const WINDOW_FUNCTION: &str = "measured_steady_frames";
/// Call-stack depth DHAT records. The deepest stack of a child is about 115 frames; a stack that
/// reaches this limit is truncated and fails the test.
const STACK_DEPTH: usize = 256;

/// Which app a child builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    /// The production live synchroniser and battle driver.
    LiveSync,
    /// The same app without the client's battle systems.
    Baseline,
    /// The live synchroniser plus one planted allocation per frame.
    Planted,
}

impl Variant {
    /// Name of the child test that builds this variant, for `--exact`.
    fn child_test(self) -> &'static str {
        match self {
            Self::LiveSync => "live_sync_frames",
            Self::Baseline => "baseline_frames",
            Self::Planted => "planted_allocation_is_counted",
        }
    }
}

/// The positive control: one heap allocation, freed in the same frame.
fn planted_allocation() {
    drop(std::hint::black_box(Vec::<u8>::with_capacity(64)));
}

/// Builds the app of `variant` on the real default battle, paused, with a fixed time step.
fn steady_state_app(variant: Variant) -> Result<App, Box<dyn std::error::Error>> {
    let root = client::ResolvedAssetsRoot::from_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut simulation = definition.instantiate()?;
    let session = BattleSession::prepare(
        SessionGeneration::FIRST,
        &mut simulation,
        CommittedBattleMetadata::from_definition(&definition)?,
    )?;
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: root.as_path().to_string_lossy().into_owned(),
            ..default()
        },
    ))
    .add_event::<AppExit>()
    .add_event::<client::CombatVisualEvent>()
    .init_asset::<Mesh>()
    .init_asset::<Image>()
    .init_asset::<StandardMaterial>()
    .init_asset::<client::BubbleDomeMaterial>()
    .init_asset::<client::SelectionRingMaterial>()
    .init_resource::<client::RenderEntityMap>()
    .init_resource::<client::battle_selection::BattleSelection>()
    .insert_resource(definition.palette.clone())
    .init_resource::<client::player_presentation::PresentationStatus>()
    .insert_resource(root)
    .insert_resource(client::CatalogResource(definition.catalogs.units.clone()))
    .insert_resource(session.with_world(simulation))
    .insert_resource(client::LabState {
        current_tab: client::LabTab::LiveBattle,
        battle_paused: true,
        simulation_speed: 1,
        ..default()
    })
    .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        Duration::from_millis(20),
    ))
    .add_systems(
        Startup,
        (
            client::setup_bubble_assets_system,
            client::setup_indicator_assets_system,
        ),
    );
    if variant != Variant::Baseline {
        app.add_systems(
            Update,
            (
                advance_battle_ticks,
                client::sync_live_battle_system,
                publish_combat_events,
            )
                .chain(),
        );
    }
    if variant == Variant::Planted {
        app.add_systems(Update, planted_allocation);
    }
    // The multi-threaded executor allocates on every frame for its own task spawning (about 18
    // blocks a frame in the baseline, all inside bevy_tasks and async_executor), and the number
    // varies with thread timing. It is Bevy's, not the client's, so the children run on the
    // single-threaded executor, which runs the same systems in the same order without tasks.
    for (_, schedule) in app.world_mut().resource_mut::<Schedules>().iter_mut() {
        schedule.set_executor_kind(ExecutorKind::SingleThreaded);
    }
    Ok(app)
}

// PERFORMANCE RATIONALE:
// Why: Whole-process allocation records mix media decoding and ECS setup with the steady state.
// What: Keep the measured window in a named, non-inlined function, so the parent can attribute
// each recorded allocation to the window by its call stack.
// Trade-off: A function boundary kept solely for measurement scope.
#[inline(never)]
fn measured_steady_frames(app: &mut App, frames: u32) {
    for _ in 0..frames {
        app.update();
    }
}

/// Runs one child: returns at once without `FRAMES_ENV`, otherwise warms the app of `variant`
/// and runs the requested number of paused frames.
fn run_child(variant: Variant) -> TestResult {
    let Some(frames) = std::env::var_os(FRAMES_ENV) else {
        println!(
            "{}: {FRAMES_ENV} is not set, so this child does nothing; \
             steady_state_frames_allocate_nothing runs it under valgrind",
            variant.child_test()
        );
        return Ok(());
    };
    let frames: u32 = frames
        .to_str()
        .ok_or_else(|| format!("{FRAMES_ENV} is not UTF-8"))?
        .parse()?;
    let mut app = steady_state_app(variant)?;
    for _ in 0..WARM_FRAMES {
        app.update();
    }
    if let Some(failure) = &app.world().resource::<client::LabState>().battle_failure {
        return Err(failure.clone().into());
    }
    if variant != Variant::Baseline
        && app
            .world()
            .resource::<client::RenderEntityMap>()
            .0
            .is_empty()
    {
        return Err("the live synchroniser presented nothing after warm-up".into());
    }
    measured_steady_frames(&mut app, frames);
    let tick = app
        .world()
        .resource::<BattleSession>()
        .frames
        .current()
        .tick;
    if tick != sim_types::ids::Tick(0) {
        return Err(format!("the battle must stay paused, but reached tick {tick:?}").into());
    }
    Ok(())
}

#[test]
fn live_sync_frames() -> TestResult {
    run_child(Variant::LiveSync)
}

#[test]
fn baseline_frames() -> TestResult {
    run_child(Variant::Baseline)
}

#[test]
fn planted_allocation_is_counted() -> TestResult {
    run_child(Variant::Planted)
}

/// The parts of a DHAT profile (`dhatFileVersion` 2, JSON) this test reads.
#[derive(serde::Deserialize)]
struct DhatProfile {
    /// Program points: one per distinct allocation stack.
    pps: Vec<ProgramPoint>,
    /// Frame table: the text of every stack frame, indexed by `ProgramPoint::fs`.
    ftbl: Vec<String>,
}

/// One allocation stack of a DHAT profile.
#[derive(serde::Deserialize)]
struct ProgramPoint {
    /// Total blocks allocated at this stack over the whole run.
    tbk: u64,
    /// The stack, innermost frame first, as indices into `DhatProfile::ftbl`.
    fs: Vec<usize>,
}

/// What one child's profile says about the measured window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowCount {
    /// Blocks allocated with `WINDOW_FUNCTION` on the stack.
    window: u64,
    /// Blocks allocated over the whole process.
    total: u64,
}

/// Counts the blocks of `profile` allocated inside the measured window. Fails if a stack reached
/// the recorded depth limit. DHAT names only frames of allocating stacks, so a window that
/// allocates nothing leaves no trace of `WINDOW_FUNCTION`; the planted control proves the scoping
/// sees the window when it does allocate.
fn window_count(profile: &DhatProfile) -> Result<WindowCount, String> {
    let window_frames: Vec<bool> = profile
        .ftbl
        .iter()
        .map(|frame| frame.contains(WINDOW_FUNCTION))
        .collect();
    let mut count = WindowCount {
        window: 0,
        total: 0,
    };
    for point in &profile.pps {
        if point.fs.len() >= STACK_DEPTH {
            return Err(format!(
                "a stack of {} frames reached the depth limit {STACK_DEPTH}",
                point.fs.len()
            ));
        }
        count.total += point.tbk;
        if point
            .fs
            .iter()
            .any(|&frame| window_frames.get(frame).copied().unwrap_or(false))
        {
            count.window += point.tbk;
        }
    }
    Ok(count)
}

/// Reads a DHAT profile. DHAT writes JSON; every JSON document it writes is also valid RON, so the
/// workspace's existing `ron` reader parses it without a new dependency.
fn read_profile(path: &Path) -> Result<DhatProfile, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let value: ron::Value = ron::from_str(&text)
        .map_err(|error| format!("{} is not a DHAT profile: {error}", path.display()))?;
    value
        .into_rust()
        .map_err(|error| format!("{} has no `pps`/`ftbl`: {error}", path.display()))
}

/// One child run under valgrind.
#[derive(Clone, Copy, Debug)]
struct Run {
    variant: Variant,
    frames: u32,
    repeat: u32,
}

/// Runs this test binary's child for `run` under DHAT, writing its profile into `directory`, and
/// returns the child's window count.
fn count_allocations(run: Run, directory: &Path) -> Result<WindowCount, String> {
    let binary = std::env::current_exe().map_err(|error| format!("current_exe: {error}"))?;
    let profile = directory.join(format!(
        "{}.{}.{}.json",
        run.variant.child_test(),
        run.frames,
        run.repeat
    ));
    let output = Command::new("valgrind")
        .arg("--tool=dhat")
        .arg(format!("--num-callers={STACK_DEPTH}"))
        .arg(format!("--dhat-out-file={}", profile.display()))
        .arg(&binary)
        .args([
            "--exact",
            run.variant.child_test(),
            "--nocapture",
            "--test-threads=1",
        ])
        .env(FRAMES_ENV, run.frames.to_string())
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => "valgrind is not installed. Install it (Debian or \
                 Ubuntu: `sudo apt-get install valgrind`); this test counts allocations with it \
                 and has no skip"
                .to_owned(),
            _ => format!("cannot start valgrind: {error}"),
        })?;
    if !output.status.success() {
        return Err(format!(
            "{run:?} failed under valgrind ({}):\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    window_count(&read_profile(&profile)?).map_err(|error| format!("{run:?}: {error}"))
}

/// Runs every child of `runs` under valgrind, at most `available_parallelism` at a time, and
/// returns their counts in the same order.
fn count_all(runs: &[Run], directory: &Path) -> Result<Vec<WindowCount>, String> {
    let width = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .clamp(1, runs.len().max(1));
    let mut counts = Vec::with_capacity(runs.len());
    for batch in runs.chunks(width) {
        let results: Vec<Result<WindowCount, String>> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .map(|&run| scope.spawn(move || count_allocations(run, directory)))
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err("a valgrind runner thread panicked".to_owned()))
                })
                .collect()
        });
        for result in results {
            counts.push(result?);
        }
    }
    Ok(counts)
}

/// The window counts of one variant: every run at the short window, then every run at the long.
#[derive(Debug)]
struct Counts {
    variant: Variant,
    short: Vec<WindowCount>,
    long: Vec<WindowCount>,
}

impl Counts {
    /// Allocations the longer window adds over `LONG_FRAMES - SHORT_FRAMES` more frames.
    fn difference(&self) -> Option<i128> {
        let short = self.short.first()?.window;
        let long = self.long.first()?.window;
        Some(i128::from(long) - i128::from(short))
    }

    /// Whether every run at each frame count reported the same window count.
    fn is_reproducible(&self) -> bool {
        let same =
            |runs: &[WindowCount]| runs.windows(2).all(|pair| pair[0].window == pair[1].window);
        same(&self.short) && same(&self.long)
    }

    /// One report line: window and whole-process counts of every run, and the difference.
    fn line(&self) -> String {
        let show = |runs: &[WindowCount]| {
            runs.iter()
                .map(|run| format!("{} (process {})", run.window, run.total))
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "  {:<9?} {SHORT_FRAMES} frames: {}; {LONG_FRAMES} frames: {}; difference {:?}",
            self.variant,
            show(&self.short),
            show(&self.long),
            self.difference()
        )
    }
}

/// Runs every variant's children and groups their counts. The live child runs twice at each frame
/// count, to show the asserted count is deterministic; the baseline and the planted control once.
/// Eight children fit two batches on a four-core CI runner.
fn measure_all_variants(directory: &Path) -> Result<Vec<Counts>, String> {
    let plan = [
        (Variant::LiveSync, 2),
        (Variant::Baseline, 1),
        (Variant::Planted, 1),
    ];
    let runs: Vec<Run> = plan
        .iter()
        .flat_map(|&(variant, repeats)| {
            [SHORT_FRAMES, LONG_FRAMES]
                .into_iter()
                .flat_map(move |frames| {
                    (0..repeats).map(move |repeat| Run {
                        variant,
                        frames,
                        repeat,
                    })
                })
        })
        .collect();
    let counts = count_all(&runs, directory)?;
    Ok(plan
        .iter()
        .map(|&(variant, _)| {
            let of = |frames: u32| {
                runs.iter()
                    .zip(&counts)
                    .filter(|(run, _)| run.variant == variant && run.frames == frames)
                    .map(|(_, &count)| count)
                    .collect()
            };
            Counts {
                variant,
                short: of(SHORT_FRAMES),
                long: of(LONG_FRAMES),
            }
        })
        .collect())
}

#[test]
fn steady_state_frames_allocate_nothing() -> TestResult {
    let directory = tempfile::tempdir()?;
    let counts = measure_all_variants(directory.path())?;
    let [live, baseline, planted] = counts.as_slice() else {
        return Err("invariant: measure_all_variants returns one entry per variant".into());
    };
    let report = format!(
        "blocks allocated inside {WINDOW_FUNCTION} after {WARM_FRAMES} warm frames:\n{}\n{}\n{}",
        live.line(),
        baseline.line(),
        planted.line()
    );
    println!("{report}");
    let (Some(live_difference), Some(baseline_difference), Some(planted_difference)) = (
        live.difference(),
        baseline.difference(),
        planted.difference(),
    ) else {
        return Err(format!("a variant has no run at one of the windows\n{report}").into());
    };
    let window = i128::from(LONG_FRAMES - SHORT_FRAMES);
    assert!(
        live.is_reproducible(),
        "identical runs must count identical window totals\n{report}"
    );
    assert_eq!(
        planted_difference - live_difference,
        window,
        "the planted control must add exactly one counted allocation per frame\n{report}"
    );
    assert_eq!(
        live_difference - baseline_difference,
        0,
        "the client's battle sync systems allocated {} times over {window} steady frames \
         beyond Bevy's own work\n{report}",
        live_difference - baseline_difference,
    );
    Ok(())
}

#[test]
fn window_count_attributes_blocks_by_stack_and_rejects_truncation() {
    let profile = DhatProfile {
        ftbl: vec![
            "[root]".to_owned(),
            "0x1: client::sync (scene_controller.rs:1)".to_owned(),
            format!("0x2: tests::{WINDOW_FUNCTION} (tests.rs:2)"),
            "0x3: tests::steady_state_app (tests.rs:3)".to_owned(),
        ],
        pps: vec![
            ProgramPoint {
                tbk: 5,
                fs: vec![1, 2],
            },
            ProgramPoint {
                tbk: 7,
                fs: vec![3],
            },
        ],
    };
    assert_eq!(
        window_count(&profile),
        Ok(WindowCount {
            window: 5,
            total: 12
        })
    );
    let truncated = DhatProfile {
        ftbl: profile.ftbl.clone(),
        pps: vec![ProgramPoint {
            tbk: 1,
            fs: vec![2; STACK_DEPTH],
        }],
    };
    assert!(window_count(&truncated).is_err());
    let quiet = DhatProfile {
        ftbl: vec![
            "[root]".to_owned(),
            "0x3: tests::setup (tests.rs:3)".to_owned(),
        ],
        pps: vec![ProgramPoint {
            tbk: 4,
            fs: vec![1],
        }],
    };
    assert_eq!(
        window_count(&quiet),
        Ok(WindowCount {
            window: 0,
            total: 4
        })
    );
}

#[test]
fn dhat_json_reads_through_ron() -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("profile.json");
    std::fs::write(
        &path,
        "{\"dhatFileVersion\":2\n,\"mode\":\"heap\",\"bklt\":true\n,\"pps\":\n \
         [{\"tb\":472,\"tbk\":3\n  ,\"acc\":[294,-2,211]\n  ,\"fs\":[1,2]\n  }\n ]\n\
         ,\"ftbl\":\n [\"[root]\"\n ,\"0x4: tests::measured_steady_frames (a.rs:1)\"\n ]\n}\n",
    )?;
    let profile = read_profile(&path)?;
    assert_eq!(
        window_count(&profile)?,
        WindowCount {
            window: 3,
            total: 3
        }
    );
    Ok(())
}
