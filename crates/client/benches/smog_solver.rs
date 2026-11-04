//! Release-mode micro-benchmark of the smog fluid solver, [`SmogGrid3d::step`] (finding C-18).
//!
//! # Responsibility
//! Times `SmogGrid3d::step` alone, step by step, over five fixed scenarios built from the public
//! smog API, and prints one Markdown table row per scenario (mean, p50 and p95 milliseconds per
//! step) so results can be pasted into the C-18 evidence file and compared commit by commit.
//!
//! # How it fits
//! This is the fast, GPU-free, **relative** measure used inside the C-18 chain of
//! `docs/plans/2026-11-04-open-findings-c17-c18-p9-gaps.md` (WP6 records the baseline; WP7, WP8
//! and WP9 compare against it). It is not the figure of record: the solver's cost in the game
//! depends on the memory traffic of Bevy's other threads, the render thread and the compositor,
//! which this binary does not reproduce. The figures of record are the interleaved A/B
//! `frame_budget` runs on the frame-budget path (the plan's D-6).
//!
//! # Scenarios
//! - `empty`: the live-battle window with no source (the cost of an idle field).
//! - `showcase`: the Visual Lab volume with the ten bouncing showcase bubbles (confinement on).
//! - `crowd_48`: the Visual Lab volume with 48 bouncing bubbles (above the 32-source limit, so
//!   confinement is skipped and the crowd wake radius applies).
//! - `battle_240`: the live-battle window (28 × 4.8 × 22 world units) with 240 sources marching
//!   in rows, half towards +x and half towards −x, wrapping at the window's edges.
//! - `after_fight`: `battle_240` for 30 simulated seconds, then no source for 30 seconds. The two
//!   halves are reported as separate rows; the second is the cost of a field after a fight.
//!
//! Every step uses `dt = 1/60` s and the default [`SmogConfig`]. Only `step` is timed: moving
//! the sources and stepping the bubble kinematics happen outside the timed region.
//!
//! # Arguments
//! - `--steps N` (default 600): timed steps of every scenario except `after_fight`, whose halves
//!   are fixed at 30 s each (1,800 steps).
//! - `--contend K` (default 1): run `K - 1` more grids on their own threads, each looping the
//!   same scenario while the first is measured, to reproduce the memory contention the solver
//!   meets in the game (plan §5.2). Only the first grid is reported. Pin the process with
//!   `taskset` so the threads stay on the cores being measured.
//!
//! # Invariants
//! - Wall-clock time is reported, never asserted; the binary fails only on a bad argument or a
//!   solver error.
//! - Percentiles are nearest-rank over the per-step durations of the reported grid.
//! - No unit identity appears here: the sources are anonymous spheres.
//!
//! Run with `cargo bench -p client --bench smog_solver [-- --steps N --contend K]`.

use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use bevy::math::Vec3;
use client::smog::{
    bubble_team_color, generate_bouncing_bubbles, initial_bouncing_bubbles,
    step_bouncing_kinematics, BouncingBubbleState, BubbleSmogSource, SmogConfig, SmogError,
    SmogGrid3d, LAB_VOLUME_MAX, LAB_VOLUME_MIN,
};

/// Fixed step, in seconds: one frame at 60 frames per second.
const STEP_DT: f32 = 1.0 / 60.0;

/// Timed steps per scenario when `--steps` is not given (10 simulated seconds).
const DEFAULT_STEPS: usize = 600;

/// Steps of each half of `after_fight`: 30 simulated seconds at [`STEP_DT`].
const AFTER_FIGHT_HALF_STEPS: usize = 1_800;

/// Lower corner of the benchmark's live-battle window, in world units.
const BATTLE_WINDOW_MIN: Vec3 = Vec3::ZERO;

/// Upper corner of the benchmark's live-battle window, in world units: the size of the game's
/// live-battle smog window (28 × 4.8 × 22, `smog/mod.rs`).
const BATTLE_WINDOW_MAX: Vec3 = Vec3::new(28.0, 4.8, 22.0);

/// Marching sources of `battle_240` and of the first half of `after_fight`.
const BATTLE_SOURCES: usize = 240;

/// Sources per marching row, spread along z.
const BATTLE_ROW_LENGTH: usize = 20;

/// Bubbles of `crowd_48`.
const CROWD_SOURCES: usize = 48;

/// Radius of a marching source, in world units (a battle unit's bubble).
const BATTLE_RADIUS: f32 = 0.45;

/// Marching speed, in world units per second.
const BATTLE_SPEED: f32 = 2.5;

/// Spacing between marching rows along x, in world units.
const BATTLE_ROW_SPACING: f32 = 2.2;

/// The benchmark's scenarios, in table order.
#[derive(Clone, Copy, Debug)]
enum Scenario {
    Empty,
    Showcase,
    Crowd48,
    Battle240,
    AfterFight,
}

impl Scenario {
    const ALL: [Self; 5] = [
        Self::Empty,
        Self::Showcase,
        Self::Crowd48,
        Self::Battle240,
        Self::AfterFight,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Showcase => "showcase",
            Self::Crowd48 => "crowd_48",
            Self::Battle240 => "battle_240",
            Self::AfterFight => "after_fight",
        }
    }

    /// The scenario's phases: (row label, source count, steps).
    fn phases(self, steps: usize) -> Vec<(&'static str, usize, usize)> {
        match self {
            Self::Empty => vec![("empty", 0, steps)],
            Self::Showcase => vec![("showcase", client::BOUNCING_BUBBLE_COUNT, steps)],
            Self::Crowd48 => vec![("crowd_48", CROWD_SOURCES, steps)],
            Self::Battle240 => vec![("battle_240", BATTLE_SOURCES, steps)],
            Self::AfterFight => vec![
                (
                    "after_fight (fight, 0-30 s)",
                    BATTLE_SOURCES,
                    AFTER_FIGHT_HALF_STEPS,
                ),
                ("after_fight (after, 30-60 s)", 0, AFTER_FIGHT_HALF_STEPS),
            ],
        }
    }
}

/// How the sources of a scenario move between steps.
enum Movers {
    /// Wall-bouncing bubbles stepped by the production kinematics, in the Visual Lab volume.
    Bouncing(Vec<BouncingBubbleState>),
    /// Rows marching along ±x at [`BATTLE_SPEED`], wrapping inside the battle window.
    Marching(Vec<BubbleSmogSource>),
}

/// One grid running one scenario, with its sources and their motion.
struct Run {
    grid: SmogGrid3d,
    config: SmogConfig,
    movers: Movers,
    sources: Vec<BubbleSmogSource>,
}

impl Run {
    fn new(scenario: Scenario) -> Result<Self, SmogError> {
        let config = SmogConfig::default();
        let (bounds_min, bounds_max, movers) = match scenario {
            Scenario::Empty => (
                BATTLE_WINDOW_MIN,
                BATTLE_WINDOW_MAX,
                Movers::Marching(Vec::new()),
            ),
            Scenario::Showcase => (
                LAB_VOLUME_MIN,
                LAB_VOLUME_MAX,
                Movers::Bouncing(initial_bouncing_bubbles(&config.bounds)?.to_vec()),
            ),
            Scenario::Crowd48 => (
                LAB_VOLUME_MIN,
                LAB_VOLUME_MAX,
                Movers::Bouncing(generate_bouncing_bubbles(CROWD_SOURCES, &config.bounds)?),
            ),
            Scenario::Battle240 | Scenario::AfterFight => (
                BATTLE_WINDOW_MIN,
                BATTLE_WINDOW_MAX,
                Movers::Marching(marching_rows(BATTLE_SOURCES)),
            ),
        };
        Ok(Self {
            grid: SmogGrid3d::new(bounds_min, bounds_max)?,
            config,
            movers,
            sources: Vec::with_capacity(BATTLE_SOURCES),
        })
    }

    /// Moves the sources by one step and fills `self.sources` with the first `count` of them.
    fn advance_sources(&mut self, count: usize) -> Result<(), SmogError> {
        self.sources.clear();
        match &mut self.movers {
            Movers::Bouncing(states) => {
                step_bouncing_kinematics(states, &self.config.bounds, STEP_DT)?;
                self.sources
                    .extend(states.iter().take(count).enumerate().map(|(index, s)| {
                        BubbleSmogSource {
                            position: s.position,
                            velocity: s.velocity,
                            radius: s.radius,
                            team_color: bubble_team_color(index),
                            emission_strength: 1.0,
                            influence_strength: 1.0,
                        }
                    }));
            }
            Movers::Marching(rows) => {
                let width = BATTLE_WINDOW_MAX.x - BATTLE_WINDOW_MIN.x;
                for source in rows.iter_mut() {
                    let x = source.position.x + source.velocity.x * STEP_DT - BATTLE_WINDOW_MIN.x;
                    source.position.x = BATTLE_WINDOW_MIN.x + x.rem_euclid(width);
                }
                self.sources.extend(rows.iter().take(count).copied());
            }
        }
        Ok(())
    }

    /// Advances the sources and runs one timed solver step; returns its duration in ms.
    fn timed_step(&mut self, count: usize) -> Result<f64, SmogError> {
        self.advance_sources(count)?;
        let start = Instant::now();
        let metrics = self.grid.step(&self.sources, &self.config, STEP_DT)?;
        let elapsed = start.elapsed().as_secs_f64() * 1_000.0;
        std::hint::black_box(metrics);
        Ok(elapsed)
    }
}

/// `count` sources in rows of [`BATTLE_ROW_LENGTH`] along z, rows [`BATTLE_ROW_SPACING`] apart
/// along x; even rows march towards +x, odd rows towards −x.
#[expect(
    clippy::cast_precision_loss,
    reason = "row and column indices stay far below the range of f32"
)]
fn marching_rows(count: usize) -> Vec<BubbleSmogSource> {
    let depth = BATTLE_WINDOW_MAX.z - BATTLE_WINDOW_MIN.z;
    let width = BATTLE_WINDOW_MAX.x - BATTLE_WINDOW_MIN.x;
    let column_spacing = depth / BATTLE_ROW_LENGTH as f32;
    (0..count)
        .map(|index| {
            let row = index / BATTLE_ROW_LENGTH;
            let column = index % BATTLE_ROW_LENGTH;
            let direction = if row.is_multiple_of(2) { 1.0 } else { -1.0 };
            let x = (BATTLE_ROW_SPACING * (row as f32 + 0.5)).rem_euclid(width);
            BubbleSmogSource {
                position: Vec3::new(
                    BATTLE_WINDOW_MIN.x + x,
                    BATTLE_WINDOW_MIN.y + BATTLE_RADIUS,
                    BATTLE_WINDOW_MIN.z + column_spacing * (column as f32 + 0.5),
                ),
                velocity: Vec3::new(direction * BATTLE_SPEED, 0.0, 0.0),
                radius: BATTLE_RADIUS,
                team_color: bubble_team_color(row % 2),
                emission_strength: 1.0,
                influence_strength: 1.0,
            }
        })
        .collect()
}

/// Mean, p50 and p95 of one phase, in ms per step.
struct Summary {
    mean: f64,
    p50: f64,
    p95: f64,
}

/// Nearest-rank summary of `samples` (sorted in place).
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "sample counts stay far below 2^52, and a rank is a small positive integer"
)]
fn summarise(samples: &mut [f64]) -> Result<Summary, Box<dyn Error>> {
    if samples.is_empty() {
        return Err("a phase must time at least one step".into());
    }
    samples.sort_by(f64::total_cmp);
    let rank = |p: f64| {
        let index = ((p * samples.len() as f64).ceil() as usize).saturating_sub(1);
        samples[index.min(samples.len() - 1)]
    };
    Ok(Summary {
        mean: samples.iter().sum::<f64>() / samples.len() as f64,
        p50: rank(0.50),
        p95: rank(0.95),
    })
}

/// Runs `scenario` on the measured grid while `contend - 1` other grids loop the same scenario
/// on their own threads, and prints one row per phase.
fn measure(scenario: Scenario, steps: usize, contend: usize) -> Result<(), Box<dyn Error>> {
    let phases = scenario.phases(steps);
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        let mut contenders = Vec::with_capacity(contend.saturating_sub(1));
        for _ in 1..contend {
            let phases = &phases;
            let stop = &stop;
            contenders.push(scope.spawn(move || -> Result<(), SmogError> {
                while !stop.load(Ordering::Relaxed) {
                    let mut run = Run::new(scenario)?;
                    for &(_, count, phase_steps) in phases {
                        for _ in 0..phase_steps {
                            if stop.load(Ordering::Relaxed) {
                                return Ok(());
                            }
                            run.timed_step(count)?;
                        }
                    }
                }
                Ok(())
            }));
        }

        let measured = (|| -> Result<(), Box<dyn Error>> {
            let mut run = Run::new(scenario)?;
            for &(label, count, phase_steps) in &phases {
                let mut samples = Vec::with_capacity(phase_steps);
                for _ in 0..phase_steps {
                    samples.push(run.timed_step(count)?);
                }
                let summary = summarise(&mut samples)?;
                println!(
                    "| {label} | {count} | {phase_steps} | {:.3} | {:.3} | {:.3} |",
                    summary.mean, summary.p50, summary.p95
                );
            }
            Ok(())
        })();
        stop.store(true, Ordering::Relaxed);

        for contender in contenders {
            contender
                .join()
                .map_err(|_| "a contending grid's thread panicked")??;
        }
        measured
    })
}

/// Parsed command line.
struct Arguments {
    steps: usize,
    contend: usize,
}

fn parse_arguments(args: impl IntoIterator<Item = String>) -> Result<Arguments, Box<dyn Error>> {
    let mut parsed = Arguments {
        steps: DEFAULT_STEPS,
        contend: 1,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            // `cargo bench` passes `--bench` to every bench binary.
            "--bench" => {}
            "--steps" => {
                let value = args.next().ok_or("--steps needs a number of steps")?;
                parsed.steps = value
                    .parse()
                    .map_err(|_| format!("--steps {value}: not a whole number of steps"))?;
                if parsed.steps == 0 {
                    return Err("--steps must be at least 1".into());
                }
            }
            "--contend" => {
                let value = args.next().ok_or("--contend needs a number of grids")?;
                parsed.contend = value
                    .parse()
                    .map_err(|_| format!("--contend {value}: not a whole number of grids"))?;
                if parsed.contend == 0 {
                    return Err("--contend must be at least 1 (the measured grid)".into());
                }
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`; expected --steps <n> or --contend <k>"
                )
                .into())
            }
        }
    }
    Ok(parsed)
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = parse_arguments(std::env::args().skip(1))?;
    println!(
        "Smog solver, `SmogGrid3d::step` only, dt = 1/60 s, default configuration; \
         {} grid(s) running, the first reported.",
        arguments.contend
    );
    println!();
    println!("| Scenario | Sources | Steps | Mean ms | p50 ms | p95 ms |");
    println!("|---|---:|---:|---:|---:|---:|");
    for scenario in Scenario::ALL {
        measure(scenario, arguments.steps, arguments.contend)
            .map_err(|error| format!("scenario {}: {error}", scenario.name()))?;
    }
    Ok(())
}
