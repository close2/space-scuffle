//! Route-quality trace of a scenario battle: where units walk, and how directly they turn corners.
//!
//! # Responsibility
//! Plays a scenario file exactly as the client starts it ([`load_scenario_battle`], which applies
//! the scenario's initial orders through `sim_core::scenario::apply_initial_orders`), records every
//! mobile unit's position and first route waypoint per tick ([`RouteSample`]), and measures each
//! unit's first contiguous pass through named boxes of the map ([`PassMetrics`]):
//! - **walked/chord**: path length walked inside the box over the straight distance between the
//!   pass's first and last point;
//! - **total turn**: the sum of absolute heading changes, with the heading sampled every
//!   [`TURN_SAMPLE_TICKS`] ticks;
//! - **first-waypoint variants**: how many different first waypoints the unit's route had during
//!   the pass.
//!
//! It also records the battle's outcome: the winner and the tick the match ended; each owner's
//! blocking counters ([`super::blocking`], plan `2026-11-09-friendly-blocking` WP1); and, at
//! [`IN_RANGE_TICK`], how many of each owner's attackers have a target inside their weapon's
//! band (that plan's §7 acceptance); and each owner's wall overlap ([`super::route_overlap`],
//! plan `2026-11-11-contact-and-missiles` WP2), listed per unit-tick in `overlap.csv`.
//!
//! # How it fits
//! The instrument of plan `2026-11-09-sc2-pathfinding` (§3.1, WP1). The example
//! `crates/sim_core/examples/route_trace.rs` only forwards its arguments to [`RouteTraceArgs::parse`]
//! and [`run_route_trace`]; the metrics are pure functions tested here on hand-made trajectories.
//! Positions are converted to `f64` for measurement only; nothing flows back into the simulation.
//!
//! # Invariants
//! - The trace only reads the world; recording a battle does not change it.
//! - A pass is the first maximal run of consecutive recorded ticks during which the unit's centre
//!   lies in the box (bounds inclusive). A unit that dies or leaves ends its pass.
//! - A simulation diagnostic during the run is an error, as in the matrix tool.

use super::blocking::{attackers_in_range, BlockingMeter, BlockingMetrics};
use super::cli::RunError;
use super::route_groups::{groups_markdown, lanes_csv, GroupMeter};
use super::route_overlap::{
    overlap_csv, overlap_markdown, OverlapMeter, OverlapMetrics, OverlapSample,
};
use sim_core::catalog::{MovementRules, RuntimeCatalogs};
use sim_core::scenario::{
    apply_initial_orders, build_world_with_catalogs, InitialOrder, ScenarioError, ScenarioManifest,
    StartLoadoutCatalog,
};
use sim_core::SimWorld;
use sim_map::MapFile;
use sim_types::{MatchResult, PlayerId, SimScalar, Tick, TickBundle};
use std::path::{Path, PathBuf};

/// Ticks between the heading samples of the total-turn metric (plan §3.1).
pub const TURN_SAMPLE_TICKS: usize = 3;

/// The tick after which attackers in range are counted (plan `2026-11-09-friendly-blocking` §7).
pub const IN_RANGE_TICK: u32 = 900;

/// A segment shorter than this, in world units, has no heading and is skipped by the turn metric.
const STILL_SEGMENT: f64 = 1e-6;

/// The usage text printed on an argument error.
pub const USAGE: &str = "usage: route_trace --scenario <file> --out <dir> [--assets <dir>] \
[--ticks <n>] [--box <name>=<x0>,<y0>,<x1>,<y1>]... [--lane-line <y>] [--order <ron>]... \
[--lane-tolerance <permille>] [--seed <n>]
  --scenario <file>  scenario path relative to the assets directory, e.g. scenarios/showcase.ron
  --out <dir>        output directory for units.csv, passes.csv and summary.md
  --assets <dir>     assets directory (default: the repository's assets/)
  --ticks <n>        ticks to play at most (default 900); the run stops when the match ends
  --box ...          a measuring box in world units, bounds inclusive (repeatable; default: the
                     three showcase boxes of plan 2026-11-09-sc2-pathfinding §3.1)
  --lane-line <y>    also write lanes.csv: the x where each unit first crosses y (world units)
  --order <ron>      an initial order replacing the scenario's own (repeatable), e.g.
                     \"AttackMoveToStart(player_slot: 1, target_player_slot: 0)\"; built in memory
  --lane-tolerance   group-routing tolerance in permille replacing the catalog's
  --seed <n>         the world's random seed replacing the scenario's
  Always writes groups.md (route groups, their lanes and GroupLane events) and overlap.csv
  (every unit-tick on which a ground body's disc overlaps a wall).";

/// A named axis-aligned box of the map in world units, bounds inclusive.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteBox {
    /// Name used in the outputs.
    pub name: String,
    /// Lowest x and y, world units.
    pub min: (f64, f64),
    /// Highest x and y, world units.
    pub max: (f64, f64),
}

impl RouteBox {
    /// Whether `point` (world units) lies in the box, bounds included.
    #[must_use]
    pub fn contains(&self, point: (f64, f64)) -> bool {
        (self.min.0..=self.max.0).contains(&point.0) && (self.min.1..=self.max.1).contains(&point.1)
    }
}

/// The three boxes of the showcase map measured by plan `2026-11-09-sc2-pathfinding` §3.1: the
/// foot of player 0's ramp W, the south-east corner of the central plateau, and the foot of ramp
/// E, in world units.
#[must_use]
pub fn showcase_boxes() -> Vec<RouteBox> {
    let named = |name: &str, min: (f64, f64), max: (f64, f64)| RouteBox {
        name: name.to_owned(),
        min,
        max,
    };
    vec![
        named("ramp_w_foot", (26.0, 7.0), (34.0, 14.0)),
        named("plateau_se", (35.0, 21.0), (41.0, 29.0)),
        named("ramp_e_foot", (31.0, 49.0), (39.0, 57.0)),
    ]
}

/// One mobile unit at the end of one tick.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteSample {
    /// The tick just stepped.
    pub tick: u32,
    /// Raw entity ID.
    pub entity: u32,
    /// Owner slot.
    pub owner: u8,
    /// Archetype slug, read from the catalog.
    pub unit: String,
    /// Centre, world units.
    pub position: (f64, f64),
    /// First waypoint of the stored route, world units, if the unit has a route.
    pub first_waypoint: Option<(f64, f64)>,
    /// Waypoints left on the stored route.
    pub route_len: usize,
}

/// The measures of one unit's first pass through one box.
#[derive(Debug, Clone, PartialEq)]
pub struct PassMetrics {
    /// First tick of the pass.
    pub entry_tick: u32,
    /// Last tick of the pass.
    pub exit_tick: u32,
    /// Path length walked inside the box, world units.
    pub walked: f64,
    /// Straight distance between the pass's first and last point, world units.
    pub chord: f64,
    /// Sum of absolute heading changes, degrees.
    pub total_turn_degrees: f64,
    /// Distinct first waypoints the unit's route had during the pass.
    pub waypoint_variants: usize,
}

impl PassMetrics {
    /// Walked over chord, or `None` for a pass whose ends coincide.
    #[must_use]
    pub fn ratio(&self) -> Option<f64> {
        (self.chord > STILL_SEGMENT).then(|| self.walked / self.chord)
    }
}

/// Measures a trajectory: `points` are consecutive per-tick positions and `waypoints` the first
/// route waypoint at each of them (same length). Returns `None` for fewer than two points.
#[must_use]
pub fn measure_pass(
    points: &[(f64, f64)],
    waypoints: &[Option<(f64, f64)>],
    entry_tick: u32,
) -> Option<PassMetrics> {
    let (&first, &last) = (points.first()?, points.last()?);
    if points.len() < 2 {
        return None;
    }
    let walked = points.windows(2).map(|leg| length(leg[0], leg[1])).sum();
    let mut variants: Vec<(f64, f64)> = Vec::new();
    for waypoint in waypoints.iter().flatten() {
        if !variants.contains(waypoint) {
            variants.push(*waypoint);
        }
    }
    let exit_tick = entry_tick + u32::try_from(points.len() - 1).unwrap_or(u32::MAX);
    Some(PassMetrics {
        entry_tick,
        exit_tick,
        walked,
        chord: length(first, last),
        total_turn_degrees: total_turn_degrees(points),
        waypoint_variants: variants.len(),
    })
}

/// The sum of absolute heading changes along `points`, in degrees, sampling every
/// [`TURN_SAMPLE_TICKS`] points (the last point is always a sample). Still segments are skipped.
#[must_use]
pub fn total_turn_degrees(points: &[(f64, f64)]) -> f64 {
    let mut samples: Vec<(f64, f64)> = points.iter().copied().step_by(TURN_SAMPLE_TICKS).collect();
    if let (Some(&last), Some(&sampled)) = (points.last(), samples.last()) {
        if last != sampled {
            samples.push(last);
        }
    }
    let headings: Vec<f64> = samples
        .windows(2)
        .filter(|leg| length(leg[0], leg[1]) > STILL_SEGMENT)
        .map(|leg| (leg[1].1 - leg[0].1).atan2(leg[1].0 - leg[0].0))
        .collect();
    headings
        .windows(2)
        .map(|pair| {
            let mut change = pair[1] - pair[0];
            while change > std::f64::consts::PI {
                change -= std::f64::consts::TAU;
            }
            while change < -std::f64::consts::PI {
                change += std::f64::consts::TAU;
            }
            change.abs().to_degrees()
        })
        .sum()
}

fn length(a: (f64, f64), b: (f64, f64)) -> f64 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

/// The first contiguous pass of `entity` through `area` in `samples` (ordered by tick), measured.
#[must_use]
pub fn first_pass(samples: &[RouteSample], entity: u32, area: &RouteBox) -> Option<PassMetrics> {
    let own = samples.iter().filter(|sample| sample.entity == entity);
    let mut points = Vec::new();
    let mut waypoints = Vec::new();
    let mut entry = None;
    let mut previous_tick = None;
    for sample in own {
        let inside = area.contains(sample.position);
        let consecutive = previous_tick.is_some_and(|tick: u32| tick + 1 == sample.tick);
        previous_tick = Some(sample.tick);
        if entry.is_some() && (!inside || !consecutive) {
            break;
        }
        if inside {
            entry.get_or_insert(sample.tick);
            points.push(sample.position);
            waypoints.push(sample.first_waypoint);
        }
    }
    measure_pass(&points, &waypoints, entry?)
}

/// The parsed command line of the route trace tool.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteTraceArgs {
    /// Scenario path relative to [`Self::assets`].
    pub scenario: PathBuf,
    /// Assets directory.
    pub assets: PathBuf,
    /// Ticks to play at most.
    pub ticks: u32,
    /// Output directory.
    pub out: PathBuf,
    /// Boxes to measure.
    pub boxes: Vec<RouteBox>,
    /// The horizontal line whose crossings `lanes.csv` records, world units.
    pub lane_line: Option<f64>,
    /// Initial orders replacing the scenario's, when not empty.
    pub orders: Vec<InitialOrder>,
    /// Group-routing tolerance replacing the catalog's, in permille.
    pub lane_tolerance: Option<u32>,
    /// The world's random seed replacing the scenario's (plan `2026-11-11-contact-and-missiles`
    /// WP2: the showcase draws its weapons' random delays from it).
    pub seed: Option<u64>,
}

/// Ticks played when `--ticks` is not given (plan §D-11).
pub const DEFAULT_TICKS: u32 = 900;

impl RouteTraceArgs {
    /// Parses the arguments after the program name.
    ///
    /// # Errors
    /// [`RouteTraceError::Usage`] for an unknown flag, a missing or malformed value, or a missing
    /// `--scenario` or `--out`.
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Self, RouteTraceError> {
        let usage = RouteTraceError::Usage;
        let mut args = args.into_iter();
        let (mut scenario, mut out, mut boxes) = (None, None, Vec::new());
        let (mut lane_line, mut orders, mut lane_tolerance) = (None, Vec::new(), None);
        let mut seed = None;
        let mut assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut ticks = DEFAULT_TICKS;
        while let Some(flag) = args.next() {
            let value = args
                .next()
                .ok_or_else(|| usage(format!("{flag} needs a value")))?;
            match flag.as_str() {
                "--scenario" => scenario = Some(PathBuf::from(value)),
                "--out" => out = Some(PathBuf::from(value)),
                "--assets" => assets = PathBuf::from(value),
                "--ticks" => {
                    ticks = value
                        .parse()
                        .map_err(|_| usage(format!("--ticks needs a tick count, got {value}")))?;
                }
                "--box" => boxes.push(parse_box(&value).ok_or_else(|| {
                    usage(format!(
                        "--box needs <name>=<x0>,<y0>,<x1>,<y1>, got {value}"
                    ))
                })?),
                "--lane-line" => {
                    lane_line = Some(value.parse().map_err(|_| {
                        usage(format!("--lane-line needs a y coordinate, got {value}"))
                    })?);
                }
                "--order" => {
                    orders.push(ron::from_str(&value).map_err(|error| {
                        usage(format!("--order needs an initial order: {error}"))
                    })?);
                }
                "--lane-tolerance" => {
                    lane_tolerance = Some(value.parse().map_err(|_| {
                        usage(format!("--lane-tolerance needs permille, got {value}"))
                    })?);
                }
                "--seed" => {
                    seed = Some(value.parse().map_err(|_| {
                        usage(format!("--seed needs an unsigned integer, got {value}"))
                    })?);
                }
                _ => return Err(usage(format!("unknown argument {flag}"))),
            }
        }
        Ok(Self {
            scenario: scenario.ok_or_else(|| usage("--scenario is required".into()))?,
            assets,
            ticks,
            out: out.ok_or_else(|| usage("--out is required".into()))?,
            boxes: if boxes.is_empty() {
                showcase_boxes()
            } else {
                boxes
            },
            lane_line,
            orders,
            lane_tolerance,
            seed,
        })
    }
}

fn parse_box(text: &str) -> Option<RouteBox> {
    let (name, bounds) = text.split_once('=')?;
    let values: Vec<f64> = bounds
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let &[x0, y0, x1, y1] = values.as_slice() else {
        return None;
    };
    (!name.is_empty() && x0 <= x1 && y0 <= y1).then(|| RouteBox {
        name: name.to_owned(),
        min: (x0, y0),
        max: (x1, y1),
    })
}

/// Why a route trace failed.
#[derive(Debug, thiserror::Error)]
pub enum RouteTraceError {
    /// The arguments are invalid.
    #[error("{0}\n{USAGE}")]
    Usage(String),
    /// A file could not be written, or the simulation raised a diagnostic.
    #[error(transparent)]
    Run(#[from] RunError),
    /// The scenario could not be loaded or started.
    #[error(transparent)]
    Scenario(#[from] ScenarioError),
    /// The `--lane-tolerance` value is out of range.
    #[error("invalid lane tolerance: {0}")]
    Tolerance(#[from] sim_core::SimCoreError),
}

/// One unit's first pass through a box.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitPass {
    /// Raw entity ID.
    pub entity: u32,
    /// Owner slot.
    pub owner: u8,
    /// Archetype slug.
    pub unit: String,
    /// The pass's measures.
    pub metrics: PassMetrics,
}

/// The passes measured in one box, units in entity order.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxPasses {
    /// The box.
    pub area: RouteBox,
    /// Each unit's first pass through it.
    pub passes: Vec<UnitPass>,
}

/// What a route trace recorded.
#[derive(Debug, Clone)]
pub struct RouteTraceOutput {
    /// Every mobile unit per tick, in tick then entity order.
    pub samples: Vec<RouteSample>,
    /// Per box, in argument order, each unit's first pass.
    pub boxes: Vec<BoxPasses>,
    /// The match result when the run stopped.
    pub result: MatchResult,
    /// Ticks played.
    pub ticks_played: u32,
    /// Each owner's blocking counters over the run, by owner slot.
    pub blocking: Vec<(u8, BlockingMetrics)>,
    /// Per owner slot, `(attackers in range, attackers)` after [`IN_RANGE_TICK`] ticks; empty
    /// if the run ended earlier.
    pub in_range: Vec<(u8, (u32, u32))>,
    /// Route groups, `GroupLane` events and lane-line crossings (plan `2026-11-10-group-routing`).
    pub groups: GroupMeter,
    /// Each owner's wall-overlap figures, by owner slot (plan `2026-11-11-contact-and-missiles`
    /// WP2).
    pub overlap: Vec<(u8, OverlapMetrics)>,
    /// Every unit-tick on which a ground body's disc overlapped a wall.
    pub overlap_samples: Vec<OverlapSample>,
}

/// Starts the scenario as [`sim_core::scenario::load_scenario_battle`] does, with the initial
/// orders and the group-routing tolerance of `args` in place of the scenario's and catalog's
/// when given. Nothing is written under the assets directory.
///
/// # Errors
/// As `load_scenario_battle`, and an out-of-range tolerance.
pub fn start_battle(args: &RouteTraceArgs) -> Result<SimWorld, RouteTraceError> {
    let assets = &args.assets;
    let manifest = ScenarioManifest::from_ron_str(
        &std::fs::read_to_string(assets.join(&args.scenario)).map_err(ScenarioError::from)?,
    )?;
    let document = MapFile::load(&assets.join(&manifest.map))
        .and_then(|file| file.decode())
        .map_err(ScenarioError::from)?;
    let loadouts = StartLoadoutCatalog::load(assets.join("catalog/start_loadouts.ron"))?;
    let mut catalogs = RuntimeCatalogs::load_from_directory(assets.join("catalog"))
        .map_err(ScenarioError::from)?;
    if let Some(permille) = args.lane_tolerance {
        catalogs
            .units
            .set_movement_rules(MovementRules::with_lane_tolerance(permille)?);
    }
    let mut world = build_world_with_catalogs(
        &document,
        &loadouts,
        &manifest.players,
        args.seed.unwrap_or(manifest.seed),
        catalogs.units,
        catalogs.upgrades,
    )?;
    let orders = if args.orders.is_empty() {
        &manifest.initial_orders
    } else {
        &args.orders
    };
    apply_initial_orders(&mut world, &document, &manifest.players, orders)?;
    Ok(world)
}

/// Plays the scenario, measures the boxes and writes `units.csv`, `passes.csv` and `summary.md`.
///
/// # Errors
/// A scenario that fails to load, a simulation diagnostic, or a file that cannot be written.
pub fn run_route_trace(args: &RouteTraceArgs) -> Result<RouteTraceOutput, RouteTraceError> {
    let mut world = start_battle(args)?;
    let mut samples = Vec::new();
    let mut groups = GroupMeter::new(args.lane_line);
    let mut ticks_played = 0;
    let mut meter = BlockingMeter::new();
    let mut overlap = OverlapMeter::new();
    let mut in_range = Vec::new();
    let owners = scenario_owners(&world);
    while ticks_played < args.ticks && !world.match_state.result.is_over() {
        world.step(&TickBundle {
            tick: Tick(ticks_played),
            commands: Vec::new(),
            state_checksum: None,
        });
        let diagnostics: Vec<String> = world
            .drain_diagnostics()
            .map(|diagnostic| format!("{diagnostic:?}"))
            .collect();
        if !diagnostics.is_empty() {
            return Err(RunError::Diagnostics {
                seed: world.rng_seed,
                tick: ticks_played,
                diagnostics: diagnostics.join("; "),
            }
            .into());
        }
        record_samples(&world, ticks_played, &mut samples);
        meter.observe(&world);
        groups.observe(&world, ticks_played);
        overlap.observe(&mut world, ticks_played);
        ticks_played += 1;
        if ticks_played == IN_RANGE_TICK {
            in_range = owners
                .iter()
                .map(|&owner| (owner, attackers_in_range(&world, PlayerId(owner))))
                .collect();
        }
    }
    let boxes = measure_boxes(&samples, &args.boxes);
    let output = RouteTraceOutput {
        samples,
        boxes,
        result: world.match_state.result,
        ticks_played,
        blocking: owners
            .iter()
            .map(|&owner| (owner, meter.of_owner(PlayerId(owner))))
            .collect(),
        in_range,
        groups,
        overlap: owners
            .iter()
            .map(|&owner| (owner, overlap.of_owner(owner)))
            .collect(),
        overlap_samples: overlap.samples().to_vec(),
    };
    write_outputs(&args.out, &output)?;
    Ok(output)
}

/// The owner slots of the world's units, ascending.
fn scenario_owners(world: &SimWorld) -> Vec<u8> {
    let mut owners: Vec<u8> = world.entities.values().map(|e| e.owner.0).collect();
    owners.sort_unstable();
    owners.dedup();
    owners
}

fn record_samples(world: &SimWorld, tick: u32, out: &mut Vec<RouteSample>) {
    let point = |x: SimScalar, y: SimScalar| (f64::from(x.to_f32()), f64::from(y.to_f32()));
    for (id, entity) in &world.entities {
        if entity.is_immobile {
            continue;
        }
        let unit = world
            .catalog
            .get(entity.unit_type_id)
            .map_or_else(|_| "unknown".to_owned(), |archetype| archetype.slug.clone());
        out.push(RouteSample {
            tick,
            entity: id.0,
            owner: entity.owner.0,
            unit,
            position: point(entity.position.x, entity.position.y),
            first_waypoint: entity
                .current_funnel_waypoints
                .first()
                .map(|waypoint| point(waypoint.x, waypoint.y)),
            route_len: entity.current_funnel_waypoints.len(),
        });
    }
}

/// Each unit's first pass through each box, units in entity order.
#[must_use]
pub fn measure_boxes(samples: &[RouteSample], boxes: &[RouteBox]) -> Vec<BoxPasses> {
    let mut units: Vec<&RouteSample> = Vec::new();
    for sample in samples {
        if !units.iter().any(|known| known.entity == sample.entity) {
            units.push(sample);
        }
    }
    units.sort_by_key(|sample| sample.entity);
    boxes
        .iter()
        .map(|area| BoxPasses {
            area: area.clone(),
            passes: units
                .iter()
                .filter_map(|sample| {
                    first_pass(samples, sample.entity, area).map(|metrics| UnitPass {
                        entity: sample.entity,
                        owner: sample.owner,
                        unit: sample.unit.clone(),
                        metrics,
                    })
                })
                .collect(),
        })
        .collect()
}

fn write_outputs(out: &Path, output: &RouteTraceOutput) -> Result<(), RunError> {
    let io = |path: &Path| {
        let path = path.to_owned();
        move |source| RunError::Io { path, source }
    };
    std::fs::create_dir_all(out).map_err(io(out))?;
    let mut units = vec!["tick,entity,owner,unit,x,y,waypoint_x,waypoint_y,route_len".to_owned()];
    units.extend(output.samples.iter().map(|s| {
        let (wx, wy) = s
            .first_waypoint
            .map_or((String::new(), String::new()), |(x, y)| {
                (format!("{x:.4}"), format!("{y:.4}"))
            });
        format!(
            "{},{},{},{},{:.4},{:.4},{wx},{wy},{}",
            s.tick, s.entity, s.owner, s.unit, s.position.0, s.position.1, s.route_len
        )
    }));
    let units_path = out.join("units.csv");
    std::fs::write(&units_path, lines(&units)).map_err(io(&units_path))?;
    let passes_path = out.join("passes.csv");
    std::fs::write(&passes_path, passes_csv(&output.boxes)).map_err(io(&passes_path))?;
    let summary_path = out.join("summary.md");
    std::fs::write(&summary_path, summary_markdown(output)).map_err(io(&summary_path))?;
    let overlap_path = out.join("overlap.csv");
    std::fs::write(&overlap_path, overlap_csv(&output.overlap_samples))
        .map_err(io(&overlap_path))?;
    let groups_path = out.join("groups.md");
    let groups = groups_markdown(&output.groups.groups(), output.groups.events());
    std::fs::write(&groups_path, groups).map_err(io(&groups_path))?;
    if output.groups.has_lane_line() {
        let lanes_path = out.join("lanes.csv");
        std::fs::write(&lanes_path, lanes_csv(&output.groups.crossings()))
            .map_err(io(&lanes_path))?;
    }
    Ok(())
}

/// Joins `rows` with a newline after each.
fn lines(rows: &[String]) -> String {
    rows.iter().flat_map(|row| [row.as_str(), "\n"]).collect()
}

/// One line per box and unit pass.
#[must_use]
pub fn passes_csv(boxes: &[BoxPasses]) -> String {
    let mut rows = vec![
        "box,entity,owner,unit,entry_tick,exit_tick,walked,chord,ratio,total_turn_deg,variants"
            .to_owned(),
    ];
    for measured in boxes {
        rows.extend(measured.passes.iter().map(|pass| {
            let m = &pass.metrics;
            let ratio = m.ratio().map_or_else(String::new, |r| format!("{r:.3}"));
            format!(
                "{},{},{},{},{},{},{:.2},{:.2},{ratio},{:.0},{}",
                measured.area.name,
                pass.entity,
                pass.owner,
                pass.unit,
                m.entry_tick,
                m.exit_tick,
                m.walked,
                m.chord,
                m.total_turn_degrees,
                m.waypoint_variants
            )
        }));
    }
    lines(&rows)
}

/// Per box and owner: units measured, mean and maximum ratio and turn; then the outcome.
#[must_use]
pub fn summary_markdown(output: &RouteTraceOutput) -> String {
    let mut rows = vec![
        "| Box | Owner | Units | Ratio mean / max | Turn mean / max |".to_owned(),
        "|---|---:|---:|---|---|".to_owned(),
    ];
    for measured in &output.boxes {
        let mut owners: Vec<u8> = measured.passes.iter().map(|pass| pass.owner).collect();
        owners.sort_unstable();
        owners.dedup();
        for owner in owners {
            let mine: Vec<&PassMetrics> = measured
                .passes
                .iter()
                .filter(|pass| pass.owner == owner)
                .map(|pass| &pass.metrics)
                .collect();
            let ratios: Vec<f64> = mine.iter().filter_map(|m| m.ratio()).collect();
            let turns: Vec<f64> = mine.iter().map(|m| m.total_turn_degrees).collect();
            rows.push(format!(
                "| {} | {owner} | {} | {:.3} / {:.3} | {:.0}° / {:.0}° |",
                measured.area.name,
                mine.len(),
                mean(&ratios),
                max(&ratios),
                mean(&turns),
                max(&turns)
            ));
        }
    }
    let outcome = match output.result {
        MatchResult::InProgress => "in progress".to_owned(),
        MatchResult::Victory { winner, tick } => {
            format!("player {} wins at tick {}", winner.0, tick.0)
        }
        MatchResult::Draw { tick } => format!("draw at tick {}", tick.0),
    };
    rows.push(String::new());
    rows.push(format!(
        "Ticks played: {}. Outcome: {outcome}.",
        output.ticks_played
    ));
    rows.push(String::new());
    rows.push(format!(
        "| Owner | In range at tick {IN_RANGE_TICK} | Blocked attacker ticks | Reversals | Stall give-ups |"
    ));
    rows.push("|---:|---|---:|---:|---:|".to_owned());
    for &(owner, blocking) in &output.blocking {
        let in_range = output
            .in_range
            .iter()
            .find(|(slot, _)| *slot == owner)
            .map_or_else(|| "—".to_owned(), |(_, (hit, of))| format!("{hit} of {of}"));
        rows.push(format!(
            "| {owner} | {in_range} | {} | {} | {} |",
            blocking.blocked_attacker_ticks, blocking.reversals, blocking.stall_giveups
        ));
    }
    rows.push(String::new());
    rows.extend(overlap_markdown(&output.overlap));
    lines(&rows)
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / f64::from(u32::try_from(values.len()).unwrap_or(u32::MAX))
}

fn max(values: &[f64]) -> f64 {
    values.iter().copied().fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use super::{first_pass, measure_pass, parse_box, total_turn_degrees, RouteBox, RouteSample};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_straight_walk_has_ratio_one_and_no_turn() {
        let points: Vec<(f64, f64)> = (0..10).map(|i| (f64::from(i) * 0.1, 2.0)).collect();
        let metrics = measure_pass(&points, &[None; 10], 5);
        assert!(metrics
            .as_ref()
            .and_then(super::PassMetrics::ratio)
            .is_some_and(|r| close(r, 1.0)));
        assert!(metrics
            .as_ref()
            .is_some_and(|m| close(m.total_turn_degrees, 0.0)));
        assert_eq!(metrics.map(|m| (m.entry_tick, m.exit_tick)), Some((5, 14)));
    }

    #[test]
    fn a_right_angle_v_has_ratio_root_two_and_ninety_degrees() {
        let mut points: Vec<(f64, f64)> = (0..=3).map(|i| (f64::from(i), f64::from(i))).collect();
        points.extend((4..=6).map(|i| (f64::from(i), f64::from(6 - i))));
        let metrics = measure_pass(&points, &[None; 7], 0);
        let ratio = metrics.as_ref().and_then(super::PassMetrics::ratio);
        assert!(ratio.is_some_and(|r| close(r, 2.0_f64.sqrt())), "{ratio:?}");
        assert!(metrics.is_some_and(|m| close(m.total_turn_degrees, 90.0)));
    }

    #[test]
    fn a_zigzag_counts_every_change_of_heading() {
        // Samples every third point: east, north-east, east, north-east: three 45° changes.
        let points = [
            (0.0, 0.0),
            (0.5, 0.0),
            (1.0, 0.0),
            (1.5, 0.0),
            (2.0, 0.5),
            (2.5, 1.0),
            (3.0, 1.5),
            (3.5, 1.5),
            (4.0, 1.5),
            (4.5, 1.5),
            (5.0, 2.0),
        ];
        assert!(close(total_turn_degrees(&points), 135.0));
    }

    #[test]
    fn distinct_first_waypoints_are_counted_once() {
        let points = [(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0)];
        let waypoints = [Some((5.0, 5.0)), Some((5.0, 5.0)), None, Some((6.0, 5.0))];
        let metrics = measure_pass(&points, &waypoints, 0);
        assert_eq!(metrics.map(|m| m.waypoint_variants), Some(2));
    }

    #[test]
    fn only_the_first_contiguous_pass_is_measured() {
        let area = RouteBox {
            name: "b".into(),
            min: (0.0, 0.0),
            max: (2.0, 2.0),
        };
        let sample = |tick: u32, x: f64| RouteSample {
            tick,
            entity: 7,
            owner: 0,
            unit: "u".into(),
            position: (x, 1.0),
            first_waypoint: None,
            route_len: 0,
        };
        let samples: Vec<RouteSample> = [(0, -1.0), (1, 0.5), (2, 1.0), (3, 3.0), (4, 1.5)]
            .into_iter()
            .map(|(tick, x)| sample(tick, x))
            .collect();
        let metrics = first_pass(&samples, 7, &area);
        assert_eq!(metrics.map(|m| (m.entry_tick, m.exit_tick)), Some((1, 2)));
        assert!(first_pass(&samples, 8, &area).is_none());
    }

    #[test]
    fn boxes_parse_and_reject_malformed_text() {
        let parsed = parse_box("ramp=1,2,3.5,4");
        assert_eq!(
            parsed.map(|b| (b.min, b.max)),
            Some(((1.0, 2.0), (3.5, 4.0)))
        );
        for bad in [
            "ramp",
            "=1,2,3,4",
            "ramp=1,2,3",
            "ramp=3,2,1,4",
            "ramp=a,2,3,4",
        ] {
            assert!(parse_box(bad).is_none(), "{bad} must be refused");
        }
    }
}
