//! Crowd scenes of the route-clearance meter (plan `2026-11-29-wall-clearance` Amendment 1, the
//! owner's answer to Q0134): large groups walking through narrow sections and chokepoints and
//! round corners, measured by [`super::route_clearance`].
//!
//! # Responsibility
//! [`crowd_scenes`] enumerates the scenes: every [`Geometry`] (three chokepoint widths, a
//! convex 90° corner, a hairpin round a thin wall's end, and a 45° diagonal wall of one-cell steps) with every group size of [`GROUP_SIZES`] and every seed asked for. A group is a block of
//! units of three scratch radii (0.375, 0.5 and 0.625 world units, cycled), all of one owner,
//! each given its own `Move` to its start point translated across the obstacle, so the group keeps
//! its formation's spacing at the goal. [`play_crowd_scene`] plays one scene under a
//! [`RunOptions`] (the arms S, R and L of plan `2026-11-27-gated-movement` are rule sets) with the
//! clearance meter watching every tick, and returns a [`CrowdRow`]: the meter's whole-map figures
//! and the scene's known answers:
//! - **every move ends**: by the tick bound every unit's move has ended, none given up for lack
//!   of progress (every chokepoint is wider than the widest body, every goal is reachable). A
//!   move may end short of its goal, against friends resting at or near it (the arrival rule);
//!   the row reports how many units ended within [`ARRIVAL`] of their goals and how many stopped
//!   short, as figures, not as answers;
//! - **no body deeper than the tolerance**: the meter's deepest body is at most [`DEPTH_BOUND`]
//!   (plan §4.6, H-W1).
//!
//! # How it fits
//! The owner asked that wall grazing, corner bumping and any later clearance change be judged on
//! dedicated crowd tests, never on the showcase alone (Q0134's answer). These scenes are those
//! tests; the runner is `crates/sim_core/examples/crowd_trace.rs`. They build worlds from scratch
//! archetypes (as the movement harness does, [`ScratchArchetype`]), so a rebalance of the shipped
//! catalog cannot move them, and name no shipped unit.
//!
//! # Invariants
//! - A scene's row is a function of `(geometry, size, seed, rules)`.
//! - Spawn positions are on the walkable side of the obstacle and at least [`SPACING`] apart less
//!   twice the jitter, which keeps every pair of the widest bodies apart.
//! - The scenes only read the world through the meter; nothing is changed but by the opening
//!   orders.

use super::route_clearance::{ClearanceFigures, ClearanceMeter, RouteFigures, TaskKind};
use crate::harness::corpus::{ScratchArchetype, SplitMix64};
use crate::harness::{HarnessError, RunOptions};
use sim_core::movement::RouteOutcomeKind;
use sim_core::{SimWorld, UnitCatalog};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};
use std::fmt::Write as _;

/// Map width and height in cells.
pub const MAP_SIZE: (u32, u32) = (48, 32);

/// The group sizes every geometry is played with.
pub const GROUP_SIZES: [u32; 3] = [12, 24, 48];

/// Distance between neighbouring spawns of a group, world units: the widest pair (2 × 0.625) and
/// a quarter unit of room.
pub const SPACING: f64 = 1.5;

/// Largest spawn jitter along each axis, world units.
pub const JITTER: f64 = 0.1;

/// A unit has arrived when its centre lies within this of its goal, world units.
pub const ARRIVAL: f64 = 0.5;

/// The depth validity bound of plan §4.6 (H-W1), world units: `2⁻¹⁴ − (1 + √2) · 2⁻¹⁶`.
pub const DEPTH_BOUND: f64 = 2.4e-5;

/// The scratch radii of a group, sixty-fourths of a world unit (0.375, 0.5 and 0.625), cycled
/// over its units.
const RADII: [i64; 3] = [24, 32, 40];

/// Top speed of every scratch body, world units per tick.
const SPEED: &str = "0.15";

/// Ticks per unit of the longest straight run from a start to its goal: the bound allows three
/// times the free walk at [`SPEED`].
const TICKS_PER_UNIT: f64 = 3.0 / 0.15;

/// Ticks added to every bound for the queue at the obstacle.
const QUEUE_TICKS: u32 = 400;

/// The obstacle a group walks through or round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Geometry {
    /// A wall across the map, cells x 22–25, with one opening of this many cells centred on the
    /// map's middle row: a narrow section four cells long.
    Choke(u32),
    /// A solid block over cells x ≥ 20, y ≤ 19: the group walks from the south-west to the
    /// north-east round its convex 90° corner at (20, 20).
    Corner,
    /// A thin wall, cells x 22–23, y 0–19: the group walks from the west to the east side,
    /// round the wall's end, a turn of about 180°.
    Hairpin,
    /// A diagonal wall at 45°, cells x 16–30 with `x − 9 ≤ y ≤ x − 6`: faces of one-cell steps,
    /// each a small convex corner. The group walks west to east across its line and must round
    /// one of its ends along a stepped face.
    Staircase,
}

impl Geometry {
    /// Every geometry, in report order.
    pub const ALL: [Self; 6] = [
        Self::Choke(2),
        Self::Choke(3),
        Self::Choke(5),
        Self::Corner,
        Self::Hairpin,
        Self::Staircase,
    ];

    /// The geometry's name in the outputs.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Choke(width) => format!("choke{width}"),
            Self::Corner => "corner90".to_owned(),
            Self::Hairpin => "hairpin".to_owned(),
            Self::Staircase => "staircase45".to_owned(),
        }
    }

    /// Whether cell `(x, y)` is solid.
    #[must_use]
    pub fn solid(self, x: u32, y: u32) -> bool {
        let middle = MAP_SIZE.1 / 2;
        match self {
            Self::Choke(width) => {
                let low = middle - width / 2;
                (22..26).contains(&x) && !(low..low + width).contains(&y)
            }
            Self::Corner => x >= 20 && y < 20,
            Self::Hairpin => (22..24).contains(&x) && y < 20,
            Self::Staircase => (16..31).contains(&x) && y + 6 <= x && x <= y + 9,
        }
    }

    /// The centre of the group's start block, and the translation from each start to its goal,
    /// world units.
    const fn route(self) -> ((f64, f64), (f64, f64)) {
        match self {
            Self::Choke(_) => ((9.0, 16.0), (30.0, 0.0)),
            Self::Corner => ((9.0, 9.0), (28.0, 17.0)),
            Self::Hairpin => ((11.0, 9.0), (26.0, 0.0)),
            Self::Staircase => ((8.0, 16.0), (32.0, 0.0)),
        }
    }
}

/// One crowd scene: a geometry, a group size and a seed (the spawn jitter and the world's seed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrowdScene {
    /// The obstacle.
    pub geometry: Geometry,
    /// Units in the group.
    pub size: u32,
    /// The seed.
    pub seed: u64,
}

/// Every scene for `seeds`: geometries, then sizes, then seeds.
#[must_use]
pub fn crowd_scenes(seeds: &[u64]) -> Vec<CrowdScene> {
    let mut scenes = Vec::new();
    for geometry in Geometry::ALL {
        for size in GROUP_SIZES {
            scenes.extend(seeds.iter().map(|&seed| CrowdScene {
                geometry,
                size,
                seed,
            }));
        }
    }
    scenes
}

/// A scene's units: archetype index into [`RADII`], start and goal, world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrowdSpawn {
    /// Index of the unit's radius in the scene's archetypes (0.375, 0.5, 0.625).
    pub archetype: usize,
    /// Start, world units.
    pub start: (f64, f64),
    /// Goal, world units.
    pub goal: (f64, f64),
}

impl CrowdScene {
    /// The units of the scene: a block of columns × rows, [`SPACING`] apart, centred on the
    /// geometry's start centre, each jittered by at most [`JITTER`] per axis.
    #[must_use]
    pub fn spawns(&self) -> Vec<CrowdSpawn> {
        let (centre, shift) = self.geometry.route();
        let columns = (1..=self.size)
            .find(|c| c * c >= self.size)
            .unwrap_or(self.size);
        let rows = self.size.div_ceil(columns);
        let mut draw = SplitMix64::new(self.seed ^ 0xC20D_5CE7);
        let mut jitter = || {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a draw below 201 is exact in f64"
            )]
            let step = draw.below(201) as f64;
            (step - 100.0) / 100.0 * JITTER
        };
        (0..self.size)
            .map(|index| {
                let (column, row) = (index % columns, index / columns);
                let x = centre.0 + (f64::from(column) - f64::from(columns - 1) / 2.0) * SPACING;
                let y = centre.1 + (f64::from(row) - f64::from(rows - 1) / 2.0) * SPACING;
                let start = (x + jitter(), y + jitter());
                CrowdSpawn {
                    archetype: usize::try_from(index).unwrap_or(0) % RADII.len(),
                    start,
                    goal: (start.0 + shift.0, start.1 + shift.1),
                }
            })
            .collect()
    }

    /// The tick bound: three times the free walk of the translation, plus [`QUEUE_TICKS`].
    #[must_use]
    pub fn tick_bound(&self) -> u32 {
        let (_, shift) = self.geometry.route();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a positive tick count far below u32::MAX"
        )]
        let walk = (shift.0.hypot(shift.1) * TICKS_PER_UNIT).ceil() as u32;
        walk + QUEUE_TICKS
    }

    /// The scene's world under `options` and its units' ids in spawn order.
    ///
    /// # Errors
    /// [`HarnessError::World`] when the map, the scratch catalog or a spawn is refused.
    pub fn build(&self, options: &RunOptions) -> Result<(SimWorld, Vec<EntityId>), HarnessError> {
        let world_error = |e: &dyn std::fmt::Display| HarnessError::World(e.to_string());
        let (width, height) = MAP_SIZE;
        let mut map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
            .map_err(|e| world_error(&e))?;
        for y in 0..height {
            for x in 0..width {
                if self.geometry.solid(x, y) {
                    map.cell_mut(x, y).map_err(|e| world_error(&e))?.flags = CellFlags::empty();
                }
            }
        }
        let mut world = crate::try_world(map, self.seed).map_err(|e| world_error(&e))?;
        let factions = crate::shipped_factions();
        let faction = factions
            .factions()
            .next()
            .map(|f| f.slug.clone())
            .ok_or(HarnessError::Scene("the shipped catalog has no faction"))?;
        let entries: Vec<String> = (1_u16..)
            .zip(RADII)
            .map(|(id, radius)| {
                let arch = ScratchArchetype {
                    radius,
                    speed: SPEED,
                    range: None,
                    priority: 10,
                };
                arch.ron(&format!("crowd_unit_{id}"), id, &faction)
            })
            .collect();
        let mut catalog =
            UnitCatalog::from_ron_str(&factions, &format!("(units: [{}])", entries.join(",")))
                .map_err(|e| world_error(&e))?;
        catalog.set_movement_rules(options.rules);
        world.catalog = catalog;
        let mut ids = Vec::new();
        for spawn in self.spawns() {
            let kind = sim_types::UnitTypeId(u16::try_from(spawn.archetype + 1).unwrap_or(1));
            ids.push(
                world
                    .spawn_unit(kind, PlayerId(0), at(spawn.start))
                    .map_err(|e| world_error(&e))?,
            );
        }
        Ok((world, ids))
    }
}

fn at(point: (f64, f64)) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(point.0), SimScalar::from_num(point.1))
}

/// What one crowd scene measured.
#[derive(Debug, Clone, PartialEq)]
pub struct CrowdRow {
    /// The scene.
    pub scene: CrowdScene,
    /// Ticks played.
    pub ticks: u32,
    /// Units within [`ARRIVAL`] of their goals at the end.
    pub arrived: u32,
    /// The tick by which every unit's move had ended, if all did.
    pub all_ended_at: Option<u32>,
    /// The meter's whole-map figures.
    pub figures: ClearanceFigures,
    /// The meter's route figures.
    pub routes: RouteFigures,
    /// The deepest body inside a wall, world units (0 when none).
    pub deepest: f64,
    /// Orders given up for lack of progress (route outcomes `NoProgress`).
    pub giveups: u32,
    /// The largest distance of a unit from its goal at the end, world units.
    pub worst_miss: f64,
    /// Units outside [`ARRIVAL`] of their goals at the end whose move has ended (no order left):
    /// they stopped short, against friends resting at or near their goals.
    pub stopped_short: u32,
}

impl CrowdRow {
    /// The CSV header of [`Self::csv`].
    pub const HEADER: &'static str = "geometry,size,seed,ticks,arrived,all_ended_at,walking,\
m1_touching,m2_margin,episodes,longest_episode,m3_reversals,m3_proposal,m4_ticks_lost,\
route_updates,m5a_cutting,m5b_walk_ticks,m5b_walk_longest,deepest,giveups,worst_miss,stopped_short,\
all_end,depth_ok";

    /// Whether every move ended by the bound with none given up (the first known answer).
    #[must_use]
    pub const fn all_end(&self) -> bool {
        self.all_ended_at.is_some() && self.giveups == 0
    }

    /// Whether no body lay deeper than [`DEPTH_BOUND`] (the second known answer).
    #[must_use]
    pub fn depth_ok(&self) -> bool {
        self.deepest <= DEPTH_BOUND
    }

    /// The row in the form of [`Self::HEADER`].
    #[must_use]
    pub fn csv(&self) -> String {
        let f = &self.figures;
        let walk = TaskKind::Walk.index();
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{:.1},{},{},{},{},{:.3e},{},{:.2},{},{},{}",
            self.scene.geometry.name(),
            self.scene.size,
            self.scene.seed,
            self.ticks,
            self.arrived,
            self.all_ended_at
                .map_or_else(String::new, |t| t.to_string()),
            f.walking,
            f.touching,
            f.within_margin,
            f.episodes,
            f.longest.map_or(0, |(length, ..)| length),
            f.reversals,
            f.proposal_reversals,
            f.ticks_lost,
            self.routes.updates,
            self.routes.cutting_updates,
            self.routes.held_cut_ticks[walk],
            self.routes.held_cut_longest[walk].map_or(0, |(length, ..)| length),
            self.deepest,
            self.giveups,
            self.worst_miss,
            self.stopped_short,
            self.all_end(),
            self.depth_ok(),
        )
    }
}

/// Plays `scene` under `options` to its tick bound, or until every unit has arrived and stands
/// still, with the clearance meter watching every tick.
///
/// # Errors
/// [`HarnessError::World`] when the world cannot be built, or a simulation diagnostic is raised.
pub fn play_crowd_scene(
    scene: &CrowdScene,
    options: &RunOptions,
) -> Result<CrowdRow, HarnessError> {
    let (mut world, ids) = scene.build(options)?;
    let spawns = scene.spawns();
    let commands: Vec<PlayerCommand> = ids
        .iter()
        .zip(&spawns)
        .map(|(id, spawn)| PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![*id],
            order: UnitOrder::Move {
                target: at(spawn.goal),
            },
            queue: false,
        })
        .collect();
    let mut meter = ClearanceMeter::new(Vec::new());
    let (mut ticks, mut all_ended_at, mut giveups) = (0, None, 0_u32);
    let bound = scene.tick_bound();
    while ticks < bound {
        world.step(&TickBundle {
            tick: Tick(ticks),
            commands: if ticks == 0 {
                commands.clone()
            } else {
                Vec::new()
            },
            state_checksum: None,
        });
        if let Some(diagnostic) = world.drain_diagnostics().next() {
            return Err(HarnessError::World(format!(
                "{} tick {ticks}: {diagnostic:?}",
                scene.geometry.name()
            )));
        }
        giveups += u32::try_from(
            world
                .route_outcomes()
                .iter()
                .filter(|o| matches!(o.kind, RouteOutcomeKind::NoProgress))
                .count(),
        )
        .unwrap_or(u32::MAX);
        meter.observe(&mut world, ticks);
        ticks += 1;
        let moving = ids
            .iter()
            .any(|id| world.entities.get(id).is_some_and(has_move));
        if moving {
            all_ended_at = None;
        } else if all_ended_at.is_none() {
            all_ended_at = Some(ticks);
        }
        if all_ended_at.is_some_and(|t| ticks >= t + 25) {
            break;
        }
    }
    let report = meter.finish();
    Ok(CrowdRow {
        scene: *scene,
        ticks,
        arrived: arrivals(&world, &ids, &spawns),
        all_ended_at,
        giveups,
        worst_miss: misses(&world, &ids, &spawns).fold(0.0, f64::max),
        stopped_short: u32::try_from(
            ids.iter()
                .zip(misses(&world, &ids, &spawns))
                .filter(|&(id, miss)| {
                    miss > ARRIVAL && world.entities.get(id).is_some_and(|e| !has_move(e))
                })
                .count(),
        )
        .unwrap_or(u32::MAX),
        figures: report.whole,
        routes: report.routes,
        deepest: report.deepest.map_or(0.0, |(depth, ..)| depth),
    })
}

/// Each unit's distance from its goal, world units (infinite for a unit that is gone).
fn misses<'a>(
    world: &'a SimWorld,
    ids: &'a [EntityId],
    spawns: &'a [CrowdSpawn],
) -> impl Iterator<Item = f64> + 'a {
    ids.iter().zip(spawns).map(|(id, spawn)| {
        world.entities.get(id).map_or(f64::INFINITY, |entity| {
            let p = (
                f64::from(entity.position.x.to_f32()),
                f64::from(entity.position.y.to_f32()),
            );
            (p.0 - spawn.goal.0).hypot(p.1 - spawn.goal.1)
        })
    })
}

/// Whether `entity` still holds an order other than `Idle`.
fn has_move(entity: &sim_core::Entity) -> bool {
    !matches!(entity.order_queue.front(), None | Some(UnitOrder::Idle))
}

/// Units within [`ARRIVAL`] of their goals.
fn arrivals(world: &SimWorld, ids: &[EntityId], spawns: &[CrowdSpawn]) -> u32 {
    let near = misses(world, ids, spawns).filter(|&miss| miss <= ARRIVAL);
    u32::try_from(near.count()).unwrap_or(u32::MAX)
}

/// The Markdown table of `rows`: one line per geometry and size, summed over its seeds.
#[must_use]
pub fn crowd_markdown(rows: &[CrowdRow]) -> String {
    let mut out = String::from(
        "| Geometry | Size | Scenes | Every move ends | Depth ok | Arrived (stopped short) | Walking | M-1 | M-2 | Longest episode | \
         M-3 (proposal) | M-4 | M-5a | M-5b walk ticks (longest) | Deepest |\n\
         |---|---:|---:|---:|---:|---|---:|---:|---:|---:|---|---:|---:|---|---|\n",
    );
    for geometry in Geometry::ALL {
        for size in GROUP_SIZES {
            let mine: Vec<&CrowdRow> = rows
                .iter()
                .filter(|r| r.scene.geometry == geometry && r.scene.size == size)
                .collect();
            if mine.is_empty() {
                continue;
            }
            let sum = |f: &dyn Fn(&CrowdRow) -> u32| mine.iter().map(|r| f(r)).sum::<u32>();
            let walk = TaskKind::Walk.index();
            let longest = mine
                .iter()
                .filter_map(|r| r.figures.longest.map(|(l, ..)| l))
                .max()
                .unwrap_or(0);
            let held_longest = mine
                .iter()
                .filter_map(|r| r.routes.held_cut_longest[walk].map(|(l, ..)| l))
                .max()
                .unwrap_or(0);
            let deepest = mine.iter().map(|r| r.deepest).fold(0.0, f64::max);
            let lost: f64 = mine.iter().map(|r| r.figures.ticks_lost).sum();
            let _ = writeln!(
                out,
                "| {} | {size} | {} | {} | {} | {} ({}) | {} | {} | {} | {longest} | {} ({}) | {lost:.1} | {} | {} ({held_longest}) | {deepest:.1e} |",
                geometry.name(),
                mine.len(),
                sum(&|r| u32::from(r.all_end())),
                sum(&|r| u32::from(r.depth_ok())),
                sum(&|r| r.arrived),
                sum(&|r| r.stopped_short),
                sum(&|r| r.figures.walking),
                sum(&|r| r.figures.touching),
                sum(&|r| r.figures.within_margin),
                sum(&|r| r.figures.reversals),
                sum(&|r| r.figures.proposal_reversals),
                sum(&|r| r.routes.cutting_updates),
                sum(&|r| r.routes.held_cut_ticks[walk]),
            );
        }
    }
    out
}

/// The usage text of the crowd runner.
pub const CROWD_USAGE: &str = "usage: crowd_trace --out <dir> [--from <seed>] [--count <n>] \
[--pipeline legacy|solver] [--crowd-iterations <cap>]
  Plays every crowd scene (geometries x group sizes x seeds from..from+count, default 1 and 4)
  under the shipped movement rules, or with the contact pipeline and crowd cap given (the arms
  of plan 2026-11-27-gated-movement: S shipped, R solver with cap 151, L legacy), and writes
  crowd.csv (one row per scene) and crowd.md (summed per geometry and size).";

/// Parses the crowd runner's arguments into the output directory, the seeds and the options.
///
/// # Errors
/// [`HarnessError::Usage`] for an unknown or malformed argument, or a missing `--out`.
pub fn parse_crowd_args(
    args: &[String],
) -> Result<(std::path::PathBuf, Vec<u64>, RunOptions), HarnessError> {
    let usage = |what: &str| HarnessError::Usage(format!("{what}\n{CROWD_USAGE}"));
    let (mut out, mut from, mut count) = (None, 1_u64, 4_u64);
    let (mut pipeline, mut cap) = (None, None);
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let value = iter
            .next()
            .ok_or_else(|| usage(&format!("{flag} needs a value")))?;
        match flag.as_str() {
            "--out" => out = Some(std::path::PathBuf::from(value)),
            "--from" => from = value.parse().map_err(|_| usage("--from takes a seed"))?,
            "--count" => count = value.parse().map_err(|_| usage("--count takes a number"))?,
            "--pipeline" => {
                pipeline = Some(match value.as_str() {
                    "legacy" => sim_core::catalog::MovementPipeline::Legacy,
                    "solver" => sim_core::catalog::MovementPipeline::Solver,
                    _ => return Err(usage("--pipeline legacy|solver")),
                });
            }
            "--crowd-iterations" => {
                cap = Some(
                    value
                        .parse()
                        .map_err(|_| usage("--crowd-iterations takes a cap"))?,
                );
            }
            other => return Err(usage(&format!("unknown argument {other}"))),
        }
    }
    let mut options = match pipeline {
        Some(pipeline) => RunOptions::with_pipeline(pipeline)?,
        None => RunOptions::default(),
    };
    if let Some(cap) = cap {
        options = options.with_crowd_iterations(cap)?;
    }
    let out = out.ok_or_else(|| usage("--out is required"))?;
    Ok((out, (from..from.saturating_add(count)).collect(), options))
}

/// Plays every scene of `seeds` under `options`, writes `crowd.csv` and `crowd.md` into `out`,
/// and returns the Markdown table.
///
/// # Errors
/// A scene that fails ([`play_crowd_scene`]) or a file that cannot be written.
pub fn run_crowd_trace(
    out: &std::path::Path,
    seeds: &[u64],
    options: &RunOptions,
) -> Result<String, HarnessError> {
    let io = |e: std::io::Error| HarnessError::Io(e.to_string());
    let rows = crowd_scenes(seeds)
        .iter()
        .map(|scene| play_crowd_scene(scene, options))
        .collect::<Result<Vec<_>, _>>()?;
    std::fs::create_dir_all(out).map_err(io)?;
    let mut csv = format!("{}\n", CrowdRow::HEADER);
    for row in &rows {
        csv.push_str(&row.csv());
        csv.push('\n');
    }
    std::fs::write(out.join("crowd.csv"), csv).map_err(io)?;
    let table = crowd_markdown(&rows);
    std::fs::write(out.join("crowd.md"), &table).map_err(io)?;
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spawn_and_goal_is_on_open_ground_and_spaced() {
        for scene in crowd_scenes(&[1, 2]) {
            let spawns = scene.spawns();
            assert_eq!(spawns.len(), scene.size as usize);
            for s in &spawns {
                for p in [s.start, s.goal] {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "points inside the map"
                    )]
                    let cell = (p.0.floor() as u32, p.1.floor() as u32);
                    assert!(
                        cell.0 < MAP_SIZE.0 && cell.1 < MAP_SIZE.1,
                        "{scene:?} {p:?} off the map"
                    );
                    assert!(
                        !scene.geometry.solid(cell.0, cell.1),
                        "{scene:?} {p:?} in a wall"
                    );
                }
            }
            for (i, a) in spawns.iter().enumerate() {
                for b in &spawns[i + 1..] {
                    let gap = (a.start.0 - b.start.0).hypot(a.start.1 - b.start.1);
                    assert!(gap >= SPACING - 2.0 * JITTER * std::f64::consts::SQRT_2 - 1e-9);
                }
            }
        }
    }

    #[test]
    fn the_narrowest_choke_is_wider_than_the_widest_body() {
        let narrowest = Geometry::ALL
            .iter()
            .filter_map(|g| match g {
                Geometry::Choke(width) => Some(*width),
                _ => None,
            })
            .min();
        #[expect(clippy::cast_precision_loss, reason = "small integers")]
        let widest = 2.0 * RADII.iter().copied().max().unwrap_or(0) as f64 / 64.0;
        assert!(narrowest.is_some_and(|w| f64::from(w) > widest));
    }
}
