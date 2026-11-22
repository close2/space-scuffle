//! The movement harness's metrics M1 to M8, measured from outside the simulation.
//!
//! # Responsibility
//! [`ScenarioMeter`] watches one scene ([`super::corpus::Scenario`]) after every tick and
//! accumulates, from the units' positions alone (plan `2026-11-20-contact-solver` §6.3):
//! - **M1** visible reversals: two consecutive steps both at least `v / 4` long whose cosine is
//!   below −0.5, per moving unit-tick (a step above `v / 8`); M1-hi for steps of at least `v / 2`;
//!   and the legacy meter's count (cosine below −0.3, both steps above 0.005);
//! - **M2** wasted path over each one-second window: path length minus net displacement;
//! - **M3** standing waste over the scene's last 250 ticks, per unit;
//! - **M4** progress: one-second windows under an unchanged order in which the potential `Φ` (the
//!   way left to the goal, through the opening of a walled scene, or to the weapon band) rises by
//!   more than the unit's radius;
//! - **M5** completion, deadlocks and give-ups; **M6** in-range counts; **M7** throughput through
//!   an opening; **M8** friend overlap per near pair-tick, enemy overlap with the pillar and the
//!   depth of any disc in blocked cells or past the map edge.
//!
//! # How it fits
//! [`super::run_scenario`] feeds the meter; [`ScenarioMeter::finish`] gives the scene's
//! [`ScenarioRow`], a line of the harness's CSV, which [`super::report`] aggregates. Positions and
//! lengths are converted to `f64` here for measurement only; nothing flows back into the world.
//!
//! # Invariants
//! - The meter only reads the world.
//! - A unit's first observed tick has no previous step, so it never counts a reversal.
//! - Every count only grows; maxima only rise.

use super::corpus::{Answer, Lattice, Role, Scenario, LATTICE};
use sim_core::movement::RouteOutcomeKind;
use sim_core::SimWorld;
use sim_map::CellFlags;
use sim_types::{EntityId, SimScalar, SimVec2, UnitOrder};

/// Ticks per second, the length of a window of M2 and M4.
pub const WINDOW_TICKS: u32 = 25;

/// Ticks at the end of a scene over which M3 measures standing waste (10 s).
pub const TAIL_TICKS: u32 = 250;

/// Ticks at the end of a scene a deadlocked unit has not moved for (M5).
pub const DEADLOCK_TICKS: u32 = 50;

/// Gap between body edges, world units, within which two friends are a near pair (M8).
pub const NEAR_GAP: f64 = 0.25;

/// Overlap share of the radius sum above which a moving near pair counts (M8, D-α's 5 %).
pub const MOVING_OVERLAP: f64 = 0.05;

/// Overlap share of the radius sum above which a standing near pair counts (M8, D-α's 1 %).
pub const STANDING_OVERLAP: f64 = 0.01;

/// One scene's measured line of the harness's CSV (the columns of [`ScenarioRow::HEADER`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScenarioRow {
    /// Family name.
    pub family: String,
    /// Seed.
    pub seed: u64,
    /// Whether the spawn order was permuted.
    pub relabel: bool,
    /// Drawn parameters, `key=value` joined by `;`.
    pub params: String,
    /// Units under test (the pillar excluded).
    pub units: u32,
    /// Ticks played.
    pub ticks: u32,
    /// Units expected to complete or reach range.
    pub expected: u32,
    /// Units that completed their order (gap, cross, corner) or stand in range at the end (arc,
    /// corridor).
    pub done: u32,
    /// Units with room that have not completed and have not moved for [`DEADLOCK_TICKS`].
    pub deadlocked: u32,
    /// Orders given up (`NoProgress`, `PursuitAbandoned`).
    pub giveups: u32,
    /// M1: visible reversals.
    pub rev_amp: u32,
    /// M1-hi: reversals of steps of at least half the unit's speed.
    pub rev_amp_hi: u32,
    /// M2: path length over whole windows, world units.
    pub path: f64,
    /// M2: path length minus net displacement over those windows, world units.
    pub wasted: f64,
    /// The legacy reversal meter's count (cosine below −0.3, both steps above 0.005).
    pub rev_legacy: u32,
    /// Moving unit-ticks, the denominator of M1.
    pub moving_ticks: u32,
    /// M4: windows whose potential rose by more than the unit's radius.
    pub progress_viol: u32,
    /// M4: active windows.
    pub progress_windows: u32,
    /// Walled scenes: units east of the wall at the end.
    pub through: u32,
    /// Ring scenes: attackers in range at the end.
    pub in_range: u32,
    /// M7: units per second through the opening between the first and the last.
    pub throughput: f64,
    /// M8: largest friend overlap share while either moves.
    pub fo_move_max: f64,
    /// M8: largest friend overlap share while both stand.
    pub fo_rest_max: f64,
    /// M8: moving near pair-ticks above [`MOVING_OVERLAP`].
    pub fo_move_over: u32,
    /// M8: standing near pair-ticks above [`STANDING_OVERLAP`].
    pub fo_rest_over: u32,
    /// M8: near pair-ticks in which either moves.
    pub pair_ticks_move: u32,
    /// M8: near pair-ticks in which both stand.
    pub pair_ticks_rest: u32,
    /// M8: largest overlap share of an attacker with the enemy pillar.
    pub enemy_ov_max: f64,
    /// M8: deepest a terrain disc reached into blocked cells or past the map edge, world units.
    pub terrain_pen_max: f64,
    /// Mean tick of completion (or of first reaching range) over the units that did; 0 if none.
    pub mean_done_tick: f64,
    /// M3: path length minus net displacement over the last [`TAIL_TICKS`], summed over units.
    pub standing_wasted: f64,
    /// Simulation diagnostics raised while playing the scene.
    pub diagnostics: u32,
    /// Ticks on which the contact solver ran (0 under the legacy pipeline).
    pub contact_ticks: u32,
    /// Averaged iterations the contact solver ran, summed over the scene's ticks.
    pub contact_iterations: u32,
    /// Ticks by iterations used: entry `k` counts the ticks on which the solver ran `k`
    /// iterations (plan `2026-11-20-contact-solver` Amendment 2, 4: the early stop's effect).
    pub iteration_ticks: Vec<u32>,
}

impl ScenarioRow {
    /// The CSV header, in [`Self::to_csv`] order.
    pub const HEADER: &'static str = "family,seed,relabel,params,units,ticks,expected,done,\
        deadlocked,giveups,rev_amp,rev_amp_hi,path,wasted,rev_legacy,moving_ticks,progress_viol,\
        progress_windows,through,in_range,throughput,fo_move_max,fo_rest_max,fo_move_over,\
        fo_rest_over,pair_ticks_move,pair_ticks_rest,enemy_ov_max,terrain_pen_max,mean_done_tick,\
        standing_wasted,diagnostics,contact_ticks,contact_iterations,iteration_ticks";

    /// The row as one CSV line, without a line end.
    #[must_use]
    pub fn to_csv(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{:.4},{:.4},{},{},{},{},{},{},{:.4},{:.5},{:.5},\
             {},{},{},{},{:.5},{:.5},{:.1},{:.4},{},{},{},{}",
            self.family,
            self.seed,
            u8::from(self.relabel),
            self.params,
            self.units,
            self.ticks,
            self.expected,
            self.done,
            self.deadlocked,
            self.giveups,
            self.rev_amp,
            self.rev_amp_hi,
            self.path,
            self.wasted,
            self.rev_legacy,
            self.moving_ticks,
            self.progress_viol,
            self.progress_windows,
            self.through,
            self.in_range,
            self.throughput,
            self.fo_move_max,
            self.fo_rest_max,
            self.fo_move_over,
            self.fo_rest_over,
            self.pair_ticks_move,
            self.pair_ticks_rest,
            self.enemy_ov_max,
            self.terrain_pen_max,
            self.mean_done_tick,
            self.standing_wasted,
            self.diagnostics,
            self.contact_ticks,
            self.contact_iterations,
            self.iteration_ticks
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("/"),
        )
    }
}

/// A point or vector in world units, for measurement.
pub type Point = (f64, f64);

/// `value` in world units as `f64`.
#[expect(
    clippy::cast_precision_loss,
    reason = "measurement only: a Q32.32 raw value converts with at most 2^-52 relative error"
)]
#[must_use]
pub fn to_f64(value: SimScalar) -> f64 {
    value.raw() as f64 / 4_294_967_296.0
}

/// `vector` in world units.
#[must_use]
pub fn point(vector: SimVec2) -> Point {
    (to_f64(vector.x), to_f64(vector.y))
}

/// `value` sixty-fourths in world units.
#[expect(
    clippy::cast_precision_loss,
    reason = "scene coordinates are below 2^20 sixty-fourths, exact in f64"
)]
#[must_use]
pub fn lattice(value: Lattice) -> f64 {
    value as f64 / LATTICE as f64
}

fn sub(a: Point, b: Point) -> Point {
    (a.0 - b.0, a.1 - b.1)
}

fn norm(a: Point) -> f64 {
    a.0.hypot(a.1)
}

fn dist(a: Point, b: Point) -> f64 {
    norm(sub(a, b))
}

/// Whether the steps `before` and `now`, both at least `floor` long, point against each other with
/// a cosine below `cosine`: the reversal test of M1 and of the legacy meter.
#[must_use]
pub fn reverses(before: Point, now: Point, floor: f64, cosine: f64) -> bool {
    let (a, b) = (norm(before), norm(now));
    a >= floor
        && b >= floor
        && a > 0.0
        && b > 0.0
        && (before.0 * now.0 + before.1 * now.1) / (a * b) < cosine
}

/// One unit as the meter follows it.
#[derive(Debug, Clone)]
struct Track {
    id: EntityId,
    owner: u8,
    role: Role,
    radius: f64,
    terrain: f64,
    speed: f64,
    goal: Option<Point>,
    position: Point,
    step: Option<Point>,
    /// Position, potential and order at the start of the current window.
    window: (Point, f64, Option<UnitOrder>),
    window_path: f64,
    tail_start: Point,
    tail_path: f64,
    still_ticks: u32,
    done_tick: Option<u32>,
    gave_up: bool,
    crossed: Option<u32>,
}

/// The scene facts the meter judges against.
#[derive(Debug, Clone, Copy)]
struct Judge {
    answer: Answer,
    /// The pillar's id, centre and radius, for the ring families.
    pillar: Option<(EntityId, Point, f64)>,
    /// The attackers' band: weapon range plus both radii, world units.
    band: f64,
}

/// The metrics of one scene, fed after every tick (see the module documentation).
#[derive(Debug, Clone)]
pub struct ScenarioMeter {
    row: ScenarioRow,
    tracks: Vec<Track>,
    judge: Judge,
    ticks: u32,
    tick: u32,
    blocked: Vec<(u32, u32)>,
    /// Outlines of the placed resource footprints, world units, counter-clockwise.
    outlines: Vec<Vec<Point>>,
    map_size: Point,
}

impl ScenarioMeter {
    /// A meter for `scenario`, whose spawns are the world's `ids` in spawn order.
    #[must_use]
    pub fn new(world: &SimWorld, scenario: &Scenario, ids: &[EntityId]) -> Self {
        let pillar = scenario
            .spawns
            .iter()
            .zip(ids)
            .find(|(spawn, _)| spawn.role == Role::Pillar)
            .map(|(spawn, &id)| {
                let radius = lattice(scenario.archetypes[spawn.archetype].radius);
                (id, (lattice(spawn.at.0), lattice(spawn.at.1)), radius)
            });
        let mut band = 0.0;
        let mut tracks = Vec::new();
        for (spawn, &id) in scenario.spawns.iter().zip(ids) {
            if spawn.role == Role::Pillar {
                continue;
            }
            let Some(entity) = world.entities.get(&id) else {
                continue;
            };
            let Ok(archetype) = world.catalog.get(entity.unit_type_id) else {
                continue;
            };
            let radius = to_f64(archetype.collision_radius);
            if let (Some(weapon), Some((_, _, pillar_radius))) = (archetype.weapon.as_ref(), pillar)
            {
                band = to_f64(weapon.max_range) + radius + pillar_radius;
            }
            let goal = match spawn.role {
                Role::GroupMove { goal } | Role::OwnMove { goal } => {
                    Some((lattice(goal.0), lattice(goal.1)))
                }
                Role::Attack | Role::Pillar => None,
            };
            let position = point(entity.position);
            tracks.push(Track {
                id,
                owner: spawn.owner,
                role: spawn.role,
                radius,
                terrain: to_f64(archetype.terrain_radius),
                speed: to_f64(archetype.max_speed),
                goal,
                position,
                step: None,
                window: (position, 0.0, None),
                window_path: 0.0,
                tail_start: position,
                tail_path: 0.0,
                still_ticks: 0,
                done_tick: None,
                gave_up: false,
                crossed: None,
            });
        }
        let judge = Judge {
            answer: scenario.answer,
            pillar,
            band,
        };
        let mut meter = Self {
            row: ScenarioRow {
                family: scenario.family.name().to_owned(),
                seed: scenario.seed,
                relabel: scenario.relabel,
                params: scenario.params.clone(),
                units: u32::try_from(tracks.len()).unwrap_or(u32::MAX),
                ticks: scenario.ticks,
                expected: scenario.expected,
                ..ScenarioRow::default()
            },
            tracks,
            judge,
            ticks: scenario.ticks,
            tick: 0,
            blocked: blocked_cells(world),
            outlines: resource_outlines(world),
            map_size: (f64::from(world.map.width()), f64::from(world.map.height())),
        };
        meter.open_windows(world);
        meter
    }

    /// Starts every unit's progress window at the current tick.
    fn open_windows(&mut self, world: &SimWorld) {
        for index in 0..self.tracks.len() {
            let track = &self.tracks[index];
            let order = world
                .entities
                .get(&track.id)
                .and_then(|entity| entity.order_queue.front().cloned());
            let phi = self.potential(track, track.position);
            let track = &mut self.tracks[index];
            track.window = (track.position, phi, order);
            track.window_path = 0.0;
        }
    }

    /// The progress potential `Φ` (M4) of the `unit`-th unit under test (spawn order, the
    /// pillar skipped) if it stood at `at`, world units; NaN for an index out of range.
    #[must_use]
    pub fn potential_of(&self, unit: usize, at: Point) -> f64 {
        self.tracks
            .get(unit)
            .map_or(f64::NAN, |track| self.potential(track, at))
    }

    /// The progress potential `Φ` of `track` at `at`, world units (M4).
    fn potential(&self, track: &Track, at: Point) -> f64 {
        match (self.judge.answer, track.goal) {
            (Answer::Wall { wall, opening, .. }, Some(goal)) => {
                let opening = (lattice(opening.0), lattice(opening.1));
                if at.0 < f64::from(wall) + 0.5 {
                    dist(at, opening) + dist(opening, goal)
                } else {
                    dist(at, goal)
                }
            }
            (_, Some(goal)) => dist(at, goal),
            (_, None) => self.judge.pillar.map_or(0.0, |(_, centre, _)| {
                (dist(at, centre) - self.judge.band).max(0.0)
            }),
        }
    }

    /// Feeds the tick the world just played.
    pub fn observe(&mut self, world: &SimWorld) {
        self.tick += 1;
        for outcome in world.route_outcomes() {
            if matches!(
                outcome.kind,
                RouteOutcomeKind::NoProgress | RouteOutcomeKind::PursuitAbandoned { .. }
            ) {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == outcome.entity) {
                    track.gave_up = true;
                    self.row.giveups += 1;
                }
            }
        }
        for index in 0..self.tracks.len() {
            self.observe_unit(world, index);
        }
        self.observe_pairs();
        if self.tick.is_multiple_of(WINDOW_TICKS) {
            self.close_windows(world);
        }
        if self.tick + TAIL_TICKS == self.ticks {
            for track in &mut self.tracks {
                track.tail_start = track.position;
                track.tail_path = 0.0;
            }
        }
    }

    /// One unit's step: reversals, path, crossing, completion, terrain depth.
    fn observe_unit(&mut self, world: &SimWorld, index: usize) {
        let Some(entity) = world
            .entities
            .get(&self.tracks[index].id)
            .filter(|e| e.is_alive())
        else {
            return;
        };
        let now = point(entity.position);
        let idle = entity.order_queue.is_empty();
        let in_band = self.in_band(now);
        let wall = match self.judge.answer {
            Answer::Wall { wall, .. } => Some(f64::from(wall)),
            _ => None,
        };
        let depth = self.terrain_depth(now, self.tracks[index].terrain);
        let tick = self.tick;
        let track = &mut self.tracks[index];
        let step = sub(now, track.position);
        let length = norm(step);
        let speed = track.speed;
        if length > speed / 8.0 {
            self.row.moving_ticks += 1;
            track.still_ticks = 0;
        } else {
            track.still_ticks += 1;
        }
        if let Some(before) = track.step {
            if reverses(before, step, speed / 4.0, -0.5) {
                self.row.rev_amp += 1;
                if reverses(before, step, speed / 2.0, -0.5) {
                    self.row.rev_amp_hi += 1;
                }
            }
            if reverses(before, step, 0.005, -0.3) {
                self.row.rev_legacy += 1;
            }
        }
        track.step = Some(step);
        track.position = now;
        track.window_path += length;
        track.tail_path += length;
        if let Some(wall) = wall {
            if track.crossed.is_none() && now.0 > wall + 0.5 {
                track.crossed = Some(tick);
            }
        }
        let complete = match track.role {
            Role::GroupMove { .. } | Role::OwnMove { .. } => {
                idle && !track.gave_up && wall.is_none_or(|wall| now.0 > wall + 1.0)
            }
            Role::Attack => in_band,
            Role::Pillar => false,
        };
        if complete && track.done_tick.is_none() {
            track.done_tick = Some(tick);
        }
        self.row.terrain_pen_max = self.row.terrain_pen_max.max(depth);
        if let Some((_, centre, pillar_radius)) = self.judge.pillar {
            let sum = track.radius + pillar_radius;
            self.row.enemy_ov_max = self.row.enemy_ov_max.max((sum - dist(now, centre)) / sum);
        }
    }

    /// Whether `at` lies within the attackers' band of the pillar.
    fn in_band(&self, at: Point) -> bool {
        self.judge
            .pillar
            .is_some_and(|(_, centre, _)| dist(at, centre) <= self.judge.band + 1e-9)
    }

    /// How deep a terrain disc of radius `radius` at `at` reaches into blocked cells or past the
    /// map edge, world units (M8).
    fn terrain_depth(&self, at: Point, radius: f64) -> f64 {
        let edge =
            at.0.min(at.1)
                .min(self.map_size.0 - at.0)
                .min(self.map_size.1 - at.1);
        let mut depth = radius - edge;
        for &(x, y) in &self.blocked {
            let (x, y) = (f64::from(x), f64::from(y));
            if (at.0 - x - 0.5).abs() > radius + 1.0 || (at.1 - y - 0.5).abs() > radius + 1.0 {
                continue;
            }
            let nearest = (at.0.clamp(x, x + 1.0), at.1.clamp(y, y + 1.0));
            depth = depth.max(radius - dist(at, nearest));
        }
        for outline in &self.outlines {
            depth = depth.max(radius + signed_depth(outline, at));
        }
        depth.max(0.0)
    }

    /// M8 over every pair of friends.
    fn observe_pairs(&mut self) {
        for a in 0..self.tracks.len() {
            for b in a + 1..self.tracks.len() {
                let (one, two) = (&self.tracks[a], &self.tracks[b]);
                if one.owner != two.owner {
                    continue;
                }
                let sum = one.radius + two.radius;
                let gap = dist(one.position, two.position);
                if gap >= sum + NEAR_GAP {
                    continue;
                }
                let overlap = (sum - gap) / sum;
                let still = |track: &Track| track.step.is_none_or(|s| norm(s) <= track.speed / 8.0);
                if still(one) && still(two) {
                    self.row.pair_ticks_rest += 1;
                    self.row.fo_rest_max = self.row.fo_rest_max.max(overlap);
                    if overlap > STANDING_OVERLAP {
                        self.row.fo_rest_over += 1;
                    }
                } else {
                    self.row.pair_ticks_move += 1;
                    self.row.fo_move_max = self.row.fo_move_max.max(overlap);
                    if overlap > MOVING_OVERLAP {
                        self.row.fo_move_over += 1;
                    }
                }
            }
        }
    }

    /// Ends every unit's window: M2's waste and M4's progress test, then opens the next.
    fn close_windows(&mut self, world: &SimWorld) {
        for index in 0..self.tracks.len() {
            let track = &self.tracks[index];
            let alive = world
                .entities
                .get(&track.id)
                .is_some_and(sim_core::Entity::is_alive);
            if !alive {
                continue;
            }
            let order = world
                .entities
                .get(&track.id)
                .and_then(|entity| entity.order_queue.front().cloned());
            let phi = self.potential(track, track.position);
            let (start, phi_start, ref order_start) = track.window;
            let net = dist(track.position, start);
            self.row.path += track.window_path;
            self.row.wasted += (track.window_path - net).max(0.0);
            if order_start.is_some() && *order_start == order {
                self.row.progress_windows += 1;
                if phi - phi_start > track.radius {
                    self.row.progress_viol += 1;
                }
            }
        }
        self.open_windows(world);
    }

    /// The scene's row after its last tick.
    #[must_use]
    pub fn finish(mut self) -> ScenarioRow {
        let ring = matches!(self.judge.answer, Answer::Ring);
        let passable = match self.judge.answer {
            Answer::Wall { passable, .. } => passable,
            Answer::Moves | Answer::Ring => true,
        };
        let mut done_ticks = Vec::new();
        let mut crossings = Vec::new();
        for track in &self.tracks {
            self.row.standing_wasted +=
                (track.tail_path - dist(track.position, track.tail_start)).max(0.0);
            crossings.extend(track.crossed);
            self.row.through += u32::from(self.passed_wall(track));
            if ring {
                if self.in_band(track.position) {
                    self.row.in_range += 1;
                    self.row.done += 1;
                }
                if let Some(tick) = track.done_tick {
                    done_ticks.push(tick);
                }
                continue;
            }
            let complete = track.done_tick.is_some() && !track.gave_up;
            if complete {
                self.row.done += 1;
                done_ticks.extend(track.done_tick);
            } else if passable && track.still_ticks >= DEADLOCK_TICKS {
                self.row.deadlocked += 1;
            }
        }
        if !done_ticks.is_empty() {
            let sum: f64 = done_ticks.iter().map(|&t| f64::from(t)).sum();
            self.row.mean_done_tick =
                sum / f64::from(u32::try_from(done_ticks.len()).unwrap_or(u32::MAX));
        }
        crossings.sort_unstable();
        if let (Some(&first), Some(&last)) = (crossings.first(), crossings.last()) {
            if crossings.len() >= 2 && last > first {
                let count = f64::from(u32::try_from(crossings.len() - 1).unwrap_or(u32::MAX));
                self.row.throughput = count * f64::from(WINDOW_TICKS) / f64::from(last - first);
            }
        }
        self.row
    }

    /// Whether `track` stands east of the wall of a walled scene at the end.
    fn passed_wall(&self, track: &Track) -> bool {
        match self.judge.answer {
            Answer::Wall { wall, .. } => track.position.0 > f64::from(wall) + 0.5,
            Answer::Moves | Answer::Ring => false,
        }
    }

    /// Records the contact solver's iterations of the tick just played (0: the solver did not run).
    pub fn add_iterations(&mut self, iterations: u8) {
        if iterations == 0 {
            return;
        }
        let index = usize::from(iterations);
        let ticks = &mut self.row.iteration_ticks;
        if ticks.len() <= index {
            ticks.resize(index + 1, 0);
        }
        ticks[index] += 1;
        self.row.contact_ticks += 1;
        self.row.contact_iterations += u32::from(iterations);
    }

    /// Records simulation diagnostics the scene raised.
    pub fn add_diagnostics(&mut self, count: usize) {
        self.row.diagnostics = self
            .row
            .diagnostics
            .saturating_add(u32::try_from(count).unwrap_or(u32::MAX));
    }
}

/// The cells of `world`'s map without ground, for M8's depth. A resource footprint's cells are
/// left out: the node blocks by its outline ([`resource_outlines`]), not by its cells.
fn blocked_cells(world: &SimWorld) -> Vec<(u32, u32)> {
    let mut cells = Vec::new();
    for y in 0..world.map.height() {
        for x in 0..world.map.width() {
            if world.map.cell(x, y).is_ok_and(|cell| {
                !cell.flags.contains(CellFlags::GROUND_PATHABLE)
                    && !cell.flags.contains(CellFlags::OCCUPIED_BY_RESOURCE)
            }) {
                cells.push((x, y));
            }
        }
    }
    cells
}

/// The outlines of `world`'s placed resource footprints, world units.
fn resource_outlines(world: &SimWorld) -> Vec<Vec<Point>> {
    world
        .map
        .resources()
        .iter()
        .filter_map(|node| node.placed)
        .map(|placed| {
            placed
                .footprint
                .contour()
                .iter()
                .map(|v| (f64::from(v.x) / 4.0, f64::from(v.y) / 4.0))
                .collect()
        })
        .collect()
}

/// How far `at` lies inside the polygon `outline`, world units: the distance to its boundary,
/// positive inside and negative outside.
#[must_use]
pub fn signed_depth(outline: &[Point], at: Point) -> f64 {
    let mut nearest = f64::INFINITY;
    let mut inside = false;
    for (index, &a) in outline.iter().enumerate() {
        let b = outline[(index + 1) % outline.len()];
        let edge = sub(b, a);
        let length = edge.0 * edge.0 + edge.1 * edge.1;
        let along = if length > 0.0 {
            (((at.0 - a.0) * edge.0 + (at.1 - a.1) * edge.1) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        nearest = nearest.min(dist(at, (a.0 + along * edge.0, a.1 + along * edge.1)));
        if (a.1 > at.1) != (b.1 > at.1) && at.0 < a.0 + (at.1 - a.1) * edge.0 / edge.1 {
            inside = !inside;
        }
    }
    if inside {
        nearest
    } else {
        -nearest
    }
}
