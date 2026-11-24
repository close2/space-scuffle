//! The movement evaluation harness of plan `2026-11-20-contact-solver` (§6, WP1).
//!
//! # Responsibility
//! Plays generated scenes with known answers ([`corpus`]) and point-symmetric battles
//! ([`mirror`]) on the simulation as it stands, measures what the units do from outside it
//! ([`metrics`]), and aggregates the measurements into the plan's tables and the verdicts of its
//! pre-registered thresholds T1 to T9 ([`report`]). Families: `gap`, `arc`, `cross`, `corridor`,
//! `corner` (B-152's rounded obstacles) and `mirror`; each scene family also runs relabelled, its
//! spawn order permuted, which permutes the entity ids and nothing else.
//!
//! # How it fits
//! The binary is `crates/sim_core/examples/movement_harness.rs`, a thin `main` over [`cli`]. The
//! harness is test support: it builds worlds from the shipped catalogs embedded in this crate, or
//! from scratch archetypes, and never changes the simulation. [`RunOptions::rules`] carries the
//! movement rules a run plays under, so one binary measures every pipeline the rules select.
//!
//! # Invariants
//! - A scene's row is a function of `(family, seed, relabel, rules)`: rows do not depend on the
//!   number of jobs or the order they finish in, and are written in seed order.
//! - Seeds are given by the caller; the ranges of §6.1 (development 1 to 1,000, acceptance
//!   100,001 to 101,000, held-out 9,000,001 to 9,001,000) are the caller's discipline.

pub mod corpus;
pub mod metrics;
pub mod mirror;
pub mod report;
pub mod room;

use corpus::{Lattice, Role, Scenario};
use metrics::{ScenarioMeter, ScenarioRow};
use sim_core::catalog::{MovementPipeline, MovementRules};
use sim_core::{SimWorld, UnitCatalog};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, ResourceNode};
use sim_types::{
    AttackOrigin, EntityId, PlayerCommand, PlayerId, ResourceKind, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// A scenario family of §6.2 (and the `corner` family of §7 WP1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    /// A wall with a gap; passable iff the gap is wider than a unit.
    Gap,
    /// Attackers round an immobile enemy pillar.
    Arc,
    /// Two friendly groups crossing.
    Cross,
    /// Attackers queued in a corridor plugged by an enemy pillar.
    Corridor,
    /// A gap whose mouth is bounded by rounded resource footprints (B-152).
    Corner,
    /// Point-symmetric battles of shipped armies.
    Mirror,
}

impl Family {
    /// Every family, in report order.
    pub const ALL: [Self; 6] = [
        Self::Gap,
        Self::Arc,
        Self::Cross,
        Self::Corridor,
        Self::Corner,
        Self::Mirror,
    ];

    /// The family's name in arguments, files and CSV rows.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gap => "gap",
            Self::Arc => "arc",
            Self::Cross => "cross",
            Self::Corridor => "corridor",
            Self::Corner => "corner",
            Self::Mirror => "mirror",
        }
    }

    /// The family's index, mixed into its scenes' seeds.
    #[must_use]
    pub const fn index(self) -> u64 {
        match self {
            Self::Gap => 0,
            Self::Arc => 1,
            Self::Cross => 2,
            Self::Corridor => 3,
            Self::Corner => 4,
            Self::Mirror => 5,
        }
    }

    /// The family named `name`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|family| family.name() == name)
    }
}

/// Why a scene or a run could not be played.
#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    /// The scene could not be generated.
    #[error("scene: {0}")]
    Scene(&'static str),
    /// The world could not be built or a unit spawned.
    #[error("world: {0}")]
    World(String),
    /// A file could not be read or written.
    #[error("io: {0}")]
    Io(String),
    /// The command line is malformed.
    #[error("usage: {0}")]
    Usage(String),
}

/// What a run plays under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOptions {
    /// The movement rules of every world of the run.
    pub rules: MovementRules,
}

impl Default for RunOptions {
    /// The shipped `movement_rules.ron`.
    fn default() -> Self {
        Self {
            rules: shipped_rules(),
        }
    }
}

impl RunOptions {
    /// The shipped rules with the contact pipeline `pipeline`.
    ///
    /// # Errors
    /// [`HarnessError::Usage`] when the shipped contact rules are invalid.
    pub fn with_pipeline(pipeline: MovementPipeline) -> Result<Self, HarnessError> {
        let shipped = shipped_rules();
        let contact = sim_core::catalog::ContactRules {
            pipeline,
            ..shipped.contact()
        };
        let rules = shipped
            .with_contact(contact)
            .map_err(|e| HarnessError::Usage(e.to_string()))?;
        Ok(Self { rules })
    }
}

/// The shipped movement rules.
///
/// # Panics
/// Panics if the shipped `movement_rules.ron` is invalid (a broken commit).
#[must_use]
pub fn shipped_rules() -> MovementRules {
    crate::shipped_catalogs().units.movement_rules()
}

/// A world point from sixty-fourths.
fn at(position: (Lattice, Lattice)) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_raw(position.0 << 26),
        SimScalar::from_raw(position.1 << 26),
    )
}

/// The map of `scenario`: open ground, its blocked cells, and its geysers to be placed with the
/// shipped footprint.
fn build_map(scenario: &Scenario) -> Result<GameMap, HarnessError> {
    let (width, height) = scenario.map.size;
    let mut cells = vec![MapCell::default_ground(CliffLevel(0)); (width * height) as usize];
    for &(x, y) in &scenario.map.blocked {
        if let Some(cell) = cells.get_mut((y * width + x) as usize) {
            cell.flags = CellFlags::empty();
        }
    }
    let resources = scenario
        .map
        .geysers
        .iter()
        .map(|&(x, y)| ResourceNode {
            pos: SimVec2::new(
                SimScalar::from_num(x) + SimScalar::HALF,
                SimScalar::from_num(y) + SimScalar::HALF,
            ),
            kind: ResourceKind::Gas,
            remaining: 2250,
            cell: (x, y),
            placed: None,
        })
        .collect();
    GameMap::from_cells(width, height, cells, resources, Vec::new())
        .map_err(|e| HarnessError::World(e.to_string()))
}

/// The world of `scenario` under `options`, and its units' ids in spawn order.
///
/// # Errors
/// [`HarnessError::World`] when the map, the scratch catalog or a spawn is refused.
pub fn build_world(
    scenario: &Scenario,
    options: &RunOptions,
) -> Result<(SimWorld, Vec<EntityId>), HarnessError> {
    let world_error = |e: &dyn std::fmt::Display| HarnessError::World(e.to_string());
    let mut world =
        crate::try_world(build_map(scenario)?, scenario.seed).map_err(|e| world_error(&e))?;
    let factions = crate::shipped_factions();
    let faction = factions
        .factions()
        .next()
        .map(|f| f.slug.clone())
        .ok_or(HarnessError::Scene("the shipped catalog has no faction"))?;
    let entries: Vec<String> = scenario
        .archetypes
        .iter()
        .enumerate()
        .map(|(index, arch)| {
            let id = u16::try_from(index + 1).unwrap_or(u16::MAX);
            arch.ron(&format!("harness_unit_{id}"), id, &faction)
        })
        .collect();
    let mut catalog =
        UnitCatalog::from_ron_str(&factions, &format!("(units: [{}])", entries.join(",")))
            .map_err(|e| world_error(&e))?;
    catalog.set_movement_rules(options.rules);
    world.catalog = catalog;
    let mut ids = Vec::with_capacity(scenario.spawns.len());
    for spawn in &scenario.spawns {
        let kind = sim_types::UnitTypeId(u16::try_from(spawn.archetype + 1).unwrap_or(u16::MAX));
        let id = world
            .spawn_unit(kind, PlayerId(spawn.owner), at(spawn.at))
            .map_err(|e| world_error(&e))?;
        ids.push(id);
    }
    Ok((world, ids))
}

/// The commands that start `scenario`, whose spawns have the world's `ids`.
#[must_use]
pub fn opening_commands(scenario: &Scenario, ids: &[EntityId]) -> Vec<PlayerCommand> {
    let pillar = scenario
        .spawns
        .iter()
        .zip(ids)
        .find(|(spawn, _)| spawn.role == Role::Pillar)
        .map(|(_, &id)| id);
    let mut group: Option<(SimVec2, Vec<EntityId>)> = None;
    let mut attackers = Vec::new();
    let mut commands = Vec::new();
    for (spawn, &id) in scenario.spawns.iter().zip(ids) {
        match spawn.role {
            Role::GroupMove { goal } => group
                .get_or_insert_with(|| (at(goal), Vec::new()))
                .1
                .push(id),
            Role::OwnMove { goal } => commands.push(PlayerCommand {
                player_id: PlayerId(spawn.owner),
                entities: vec![id],
                order: UnitOrder::Move { target: at(goal) },
                queue: false,
            }),
            Role::Attack => attackers.push(id),
            Role::Pillar => {}
        }
    }
    if let Some((target, entities)) = group {
        commands.push(PlayerCommand {
            player_id: PlayerId(0),
            entities,
            order: UnitOrder::Move { target },
            queue: false,
        });
    }
    if let (Some(target), false) = (pillar, attackers.is_empty()) {
        commands.push(PlayerCommand {
            player_id: PlayerId(0),
            entities: attackers,
            order: UnitOrder::AttackTarget {
                target,
                origin: AttackOrigin::Explicit,
            },
            queue: false,
        });
    }
    commands
}

/// Plays `scenario` under `options` for its bound and measures it.
///
/// # Errors
/// [`HarnessError::World`] when its world cannot be built.
pub fn run_scenario(
    scenario: &Scenario,
    options: &RunOptions,
) -> Result<ScenarioRow, HarnessError> {
    let (mut world, ids) = build_world(scenario, options)?;
    let mut meter = ScenarioMeter::new(&world, scenario, &ids);
    let mut commands = opening_commands(scenario, &ids);
    for _ in 0..scenario.ticks {
        world.step(&TickBundle {
            tick: Tick(world.current_tick),
            commands: std::mem::take(&mut commands),
            state_checksum: None,
        });
        meter.add_diagnostics(world.drain_diagnostics().count());
        meter.add_iterations(world.contact_iterations());
        meter.observe(&world);
    }
    Ok(meter.finish())
}

/// One family's CSV over `count` seeds from `from`, played on `jobs` threads, in seed order.
///
/// # Errors
/// The first [`HarnessError`] of any scene.
pub fn run_range(
    family: Family,
    (from, count): (u64, u64),
    relabel: bool,
    jobs: usize,
    options: &RunOptions,
) -> Result<String, HarnessError> {
    let seeds: Vec<u64> = (from..from.saturating_add(count)).collect();
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, Result<String, HarnessError>)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(&seed) = seeds.get(index) else {
                    break;
                };
                let line = if family == Family::Mirror {
                    mirror::run(seed, options).map(|row| row.to_csv())
                } else {
                    run_scenario(&corpus::generate(family, seed, relabel), options)
                        .map(|row| row.to_csv())
                };
                if let Ok(mut guard) = results.lock() {
                    guard.push((index, line));
                }
            });
        }
    });
    let mut results = results
        .into_inner()
        .map_err(|_| HarnessError::Io("a worker thread panicked".to_owned()))?;
    results.sort_by_key(|(index, _)| *index);
    let header = if family == Family::Mirror {
        mirror::MirrorRow::HEADER
    } else {
        ScenarioRow::HEADER
    };
    let mut text = format!("{header}\n");
    for (_, line) in results {
        text.push_str(&line?);
        text.push('\n');
    }
    Ok(text)
}

pub mod cli {
    //! The command line of `movement_harness`.
    //!
    //! ```text
    //! movement_harness --family gap --from 1 --count 1000 [--relabel] [--jobs 4] --out gap.csv
    //! movement_harness --aggregate today=<dir> [label=<dir> ...]
    //! ```
    //! The first form plays one family's range and writes its CSV; the second prints §5's tables
    //! of the runs in the directories and the verdicts of every run after the first against it.

    use super::{report, run_range, Family, HarnessError, RunOptions};
    use sim_core::catalog::MovementPipeline;
    use std::path::PathBuf;

    /// What the command line asks for.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Command {
        /// Play one family's seeds.
        Play {
            /// The family.
            family: Family,
            /// First seed and number of seeds.
            range: (u64, u64),
            /// Whether to relabel.
            relabel: bool,
            /// Worker threads.
            jobs: usize,
            /// Where to write the CSV.
            out: PathBuf,
            /// The contact pipeline to play under; the shipped one when `None`.
            pipeline: Option<MovementPipeline>,
        },
        /// Aggregate run directories, each `label=dir`.
        Aggregate(Vec<(String, PathBuf)>),
    }

    /// Parses `args` (without the program name).
    ///
    /// # Errors
    /// [`HarnessError::Usage`] for a malformed command line.
    pub fn parse(args: &[String]) -> Result<Command, HarnessError> {
        let usage = |what: &str| HarnessError::Usage(what.to_owned());
        if args.first().map(String::as_str) == Some("--aggregate") {
            let runs = args[1..]
                .iter()
                .map(|arg| match arg.split_once('=') {
                    Some((label, dir)) => (label.to_owned(), PathBuf::from(dir)),
                    None => (arg.clone(), PathBuf::from(arg)),
                })
                .collect::<Vec<_>>();
            return if runs.is_empty() {
                Err(usage("--aggregate needs at least one directory"))
            } else {
                Ok(Command::Aggregate(runs))
            };
        }
        let (mut family, mut from, mut count, mut relabel, mut jobs, mut out) =
            (None, 1_u64, 1_u64, false, 1_usize, None);
        let mut pipeline = None;
        let mut iter = args.iter();
        while let Some(flag) = iter.next() {
            let mut value = || {
                iter.next()
                    .cloned()
                    .ok_or_else(|| usage(&format!("{flag} needs a value")))
            };
            match flag.as_str() {
                "--family" => family = Family::parse(&value()?),
                "--from" => from = value()?.parse().map_err(|_| usage("--from takes a seed"))?,
                "--count" => {
                    count = value()?
                        .parse()
                        .map_err(|_| usage("--count takes a number"))?;
                }
                "--jobs" => {
                    jobs = value()?
                        .parse()
                        .map_err(|_| usage("--jobs takes a number"))?;
                }
                "--out" => out = Some(PathBuf::from(value()?)),
                "--relabel" => relabel = true,
                "--pipeline" => {
                    pipeline = Some(match value()?.as_str() {
                        "legacy" => MovementPipeline::Legacy,
                        "solver" => MovementPipeline::Solver,
                        _ => return Err(usage("--pipeline legacy|solver")),
                    });
                }
                other => return Err(usage(&format!("unknown argument {other}"))),
            }
        }
        Ok(Command::Play {
            family: family.ok_or_else(|| usage("--family gap|arc|cross|corridor|corner|mirror"))?,
            range: (from, count),
            relabel,
            jobs,
            out: out.ok_or_else(|| usage("--out <file.csv>"))?,
            pipeline,
        })
    }

    /// Runs `command` and returns what to print.
    ///
    /// # Errors
    /// Any [`HarnessError`] of the run, or of reading and writing its files.
    pub fn execute(command: &Command, options: &RunOptions) -> Result<String, HarnessError> {
        match command {
            Command::Play {
                family,
                range,
                relabel,
                jobs,
                out,
                pipeline,
            } => {
                let chosen = match pipeline {
                    Some(pipeline) => RunOptions::with_pipeline(*pipeline)?,
                    None => *options,
                };
                let text = run_range(*family, *range, *relabel, *jobs, &chosen)?;
                std::fs::write(out, &text)
                    .map_err(|e| HarnessError::Io(format!("{}: {e}", out.display())))?;
                Ok(format!(
                    "{} rows of {} written to {}",
                    text.lines().count().saturating_sub(1),
                    family.name(),
                    out.display()
                ))
            }
            Command::Aggregate(dirs) => {
                let runs = dirs
                    .iter()
                    .map(|(label, dir)| report::load_run(label, dir))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!(
                    "{}{}",
                    report::render(&runs),
                    report::verdicts(&runs)
                ))
            }
        }
    }
}

/// The CSV file of `family` (relabelled or not) in the run directory `dir`.
#[must_use]
pub fn csv_path(dir: &std::path::Path, family: Family, relabel: bool) -> PathBuf {
    let suffix = if relabel { "-relabel" } else { "" };
    dir.join(format!("{}{suffix}.csv", family.name()))
}
