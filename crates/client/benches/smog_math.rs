//! Release-mode micro-benchmark of the smog's host-independent math (owner question Q0014).
//!
//! # Responsibility
//! Measures what routing the smog's transcendental functions through the pure-Rust `libm` crate
//! (`client::smog::math`) costs, in two parts, and prints Markdown tables:
//!
//! - `calls`: nanoseconds per call of the host's `f32::{exp, sin, cos, sin_cos, atan2, powf}`
//!   (the C math library) against `client::smog::math`, over fixed argument sweeps matching each
//!   smog call site's range.
//! - `startup`: wall time of the smog startup system, `setup_smog_volume_system` (the pre-warm,
//!   the 32³ noise texture and the showcase bubbles), in a headless application built as the
//!   smog goldens build it, timed by two marker systems ordered just before and after it. Each run
//!   builds a fresh application; the median of the runs is reported.
//!
//! # How it fits
//! Plan `docs/plans/2026-11-04-libm-smog-math.md`, D-6, instruments 1 and 2. The `calls` table
//! is informational: it explains a regression, it does not decide one. The `startup` figure is
//! compared between two builds (before and after the routing) against the plan's stop rule of
//! 50 ms; the per-frame cost is decided by the interleaved `frame_budget` runs, not here.
//!
//! # Invariants
//! - Wall-clock time is reported, never asserted; the binary fails only on a bad argument or a
//!   startup error.
//! - Every argument and result passes through `std::hint::black_box`, so neither side is folded
//!   at compile time or hoisted out of the loop.
//! - The startup application reads the real asset root (`assets/`), as the production client
//!   does; no unit identity is named here.
//!
//! Run with `cargo bench -p client --bench smog_math [-- --only calls|startup --runs N]`.

use std::error::Error;
use std::hint::black_box;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use client::smog::math;

/// Arguments per sweep.
const SWEEP: usize = 4_096;

/// Timed repetitions of every sweep; the median repetition is reported.
const CALL_ROUNDS: usize = 301;

/// Startup runs when `--runs` is not given.
const DEFAULT_STARTUP_RUNS: usize = 15;

/// `SWEEP` evenly spaced arguments from `lo` to `hi`.
#[expect(
    clippy::cast_precision_loss,
    reason = "sweep indices stay far below 2^24"
)]
fn sweep(lo: f32, hi: f32) -> Vec<f32> {
    (0..SWEEP)
        .map(|i| lo + (hi - lo) * (i as f32 / (SWEEP - 1) as f32))
        .collect()
}

/// Median nanoseconds per call of `f` over `args`, from [`CALL_ROUNDS`] timed passes.
#[expect(
    clippy::cast_precision_loss,
    reason = "durations and counts stay far below 2^52"
)]
fn ns_per_call(args: &[f32], f: impl Fn(f32) -> f32) -> f64 {
    let mut rounds: Vec<f64> = (0..CALL_ROUNDS)
        .map(|_| {
            let start = Instant::now();
            let mut acc = 0.0_f32;
            for &x in args {
                acc += f(black_box(x));
            }
            black_box(acc);
            start.elapsed().as_nanos() as f64 / args.len() as f64
        })
        .collect();
    rounds.sort_by(f64::total_cmp);
    rounds[rounds.len() / 2]
}

/// One row of the `calls` table: the host's function against `client::smog::math`'s.
fn call_row(
    name: &str,
    range: &str,
    args: &[f32],
    host: impl Fn(f32) -> f32,
    ours: impl Fn(f32) -> f32,
) {
    let host_ns = ns_per_call(args, host);
    let ours_ns = ns_per_call(args, ours);
    println!(
        "| `{name}` | {range} | {host_ns:.2} | {ours_ns:.2} | {:+.2} |",
        ours_ns - host_ns
    );
}

/// The `calls` table.
fn measure_calls() {
    println!("| Function | Arguments | Host ns/call | `smog::math` ns/call | Difference |");
    println!("|---|---|---:|---:|---:|");
    let decay = sweep(-1.0, 0.0);
    call_row("exp", "[-1, 0]", &decay, f32::exp, math::exp);
    let phase = sweep(-40.0, 40.0);
    call_row("sin", "[-40, 40]", &phase, f32::sin, math::sin);
    call_row("cos", "[-40, 40]", &phase, f32::cos, math::cos);
    let half_angle = sweep(-std::f32::consts::PI, std::f32::consts::PI);
    call_row(
        "sin_cos",
        "[-π, π]",
        &half_angle,
        |x| {
            let (s, c) = x.sin_cos();
            s + c
        },
        |x| {
            let (s, c) = math::sin_cos(x);
            s + c
        },
    );
    call_row(
        "atan2",
        "unit circle",
        &half_angle,
        |a| black_box(a * 0.5 + 0.3).atan2(black_box(1.1 - a * 0.2)),
        |a| math::atan2(black_box(a * 0.5 + 0.3), black_box(1.1 - a * 0.2)),
    );
    let base = sweep(0.0, 1.0);
    call_row(
        "powf",
        "[0, 1]^1.3",
        &base,
        |x| x.powf(1.3),
        |x| math::powf(x, 1.3),
    );
}

/// When the marker systems ran, around `setup_smog_volume_system`.
#[derive(Resource, Default)]
struct StartupClock {
    before: Option<Instant>,
    after: Option<Instant>,
}

fn mark_before(mut clock: ResMut<'_, StartupClock>) {
    clock.before = Some(Instant::now());
}

fn mark_after(mut clock: ResMut<'_, StartupClock>) {
    clock.after = Some(Instant::now());
}

/// The headless smog application of `smog_golden_tests.rs` (real catalog, palette and media,
/// manual clock), with the two markers around the smog startup system.
fn startup_app() -> Result<App, Box<dyn Error>> {
    let root = client::ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .canonicalize()?,
    )?;
    let definition = client::runtime_battle::BattleDefinition::load(
        root.as_path(),
        client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )));
    app.add_event::<AppExit>();
    app.init_resource::<client::RenderEntityMap>();
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<StandardMaterial>>();
    app.init_resource::<Assets<client::bubble_shader::BubbleDomeMaterial>>();
    app.init_resource::<Assets<client::smog::SmogVolumeMaterial>>();
    app.insert_resource(definition.palette.clone());
    app.init_resource::<client::player_presentation::PresentationStatus>();
    app.insert_resource(root);
    app.insert_resource(client::CatalogResource(definition.catalogs.units.clone()));
    app.insert_resource(client::LabState {
        smog_bouncing_active: true,
        ..default()
    });
    app.init_resource::<StartupClock>();
    app.add_systems(Startup, client::bubble_shader::setup_bubble_assets_system);
    app.add_plugins(client::smog::SmogPlugin);
    app.add_systems(
        Startup,
        (
            mark_before
                .after(client::bubble_shader::setup_bubble_assets_system)
                .before(client::smog::setup_smog_volume_system),
            mark_after.after(client::smog::setup_smog_volume_system),
        ),
    );
    app.edit_schedule(Startup, |schedule| {
        schedule.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
    });
    Ok(app)
}

/// Milliseconds `setup_smog_volume_system` took in one fresh application.
fn startup_once() -> Result<f64, Box<dyn Error>> {
    let mut app = startup_app()?;
    app.update();
    let clock = app.world().resource::<StartupClock>();
    let (Some(before), Some(after)) = (clock.before, clock.after) else {
        return Err("the startup markers did not run".into());
    };
    if app
        .world()
        .get_resource::<client::smog::SmogTextures>()
        .is_none()
    {
        return Err("the smog startup system did not create its textures".into());
    }
    Ok(after.duration_since(before).as_secs_f64() * 1_000.0)
}

/// The `startup` table: every run, then the median.
fn measure_startup(runs: usize) -> Result<(), Box<dyn Error>> {
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        times.push(startup_once()?);
    }
    let listed: Vec<String> = times.iter().map(|ms| format!("{ms:.2}")).collect();
    times.sort_by(f64::total_cmp);
    println!("| Runs | Median ms | Min ms | Max ms | Every run (ms, in order) |");
    println!("|---:|---:|---:|---:|---|");
    println!(
        "| {runs} | {:.2} | {:.2} | {:.2} | {} |",
        times[times.len() / 2],
        times[0],
        times[times.len() - 1],
        listed.join(", ")
    );
    Ok(())
}

/// Parsed command line.
struct Arguments {
    calls: bool,
    startup: bool,
    runs: usize,
}

fn parse_arguments(args: impl IntoIterator<Item = String>) -> Result<Arguments, Box<dyn Error>> {
    let mut parsed = Arguments {
        calls: true,
        startup: true,
        runs: DEFAULT_STARTUP_RUNS,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            // `cargo bench` passes `--bench` to every bench binary.
            "--bench" => {}
            "--only" => match args.next().as_deref() {
                Some("calls") => parsed.startup = false,
                Some("startup") => parsed.calls = false,
                other => return Err(format!("--only {other:?}: expected calls or startup").into()),
            },
            "--runs" => {
                let value = args.next().ok_or("--runs needs a number of runs")?;
                parsed.runs = value
                    .parse()
                    .map_err(|_| format!("--runs {value}: not a whole number of runs"))?;
                if parsed.runs == 0 {
                    return Err("--runs must be at least 1".into());
                }
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`; expected --only <calls|startup> or --runs <n>"
                )
                .into())
            }
        }
    }
    Ok(parsed)
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = parse_arguments(std::env::args().skip(1))?;
    if arguments.calls {
        println!("Smog math, ns per call, median of {CALL_ROUNDS} passes over {SWEEP} arguments.");
        println!();
        measure_calls();
        println!();
    }
    if arguments.startup {
        println!("Smog startup, `setup_smog_volume_system` wall time, fresh application per run.");
        println!();
        measure_startup(arguments.runs)?;
    }
    Ok(())
}
