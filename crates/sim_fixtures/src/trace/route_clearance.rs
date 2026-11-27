//! Route-clearance meter of the route trace (plan `2026-11-29-wall-clearance` §6.1, WP0): how
//! close walking ground bodies come to walls, how long they stay there, what turns them round at
//! a wall, and which routes lead them through a corner.
//!
//! # Responsibility
//! After each tick, for every ground body that walks, [`ClearanceMeter::observe`] asks
//! `Pathfinder::wall_contact` for the nearest wall within the body's terrain radius plus
//! [`CLEARANCE_REACH`] under its physical request (true terrain, every structure, no vision), and
//! measures its **clearance**, the distance to that wall less the terrain radius. It then counts:
//! - **M-1 touching**: walking unit-ticks with a clearance of at most [`WALL_PROBE_SLACK`];
//! - **M-2 within the margin**: walking unit-ticks with a clearance below
//!   `sim_map::CORNER_SAFETY_MARGIN`, the route's designed clearance, and their **episodes**,
//!   the maximal runs of consecutive M-2 unit-ticks of one unit;
//! - **M-3 reversals at a wall**: two consecutive achieved steps that reverse
//!   ([`super::blocking::is_reversal`]) where either tick is M-2, split into proposal-caused and
//!   contact-caused as plan `2026-11-19-crowd-edge-steering` D-5 does;
//! - **M-4 ticks lost** per episode: its length less the ticks its net displacement takes at the
//!   archetype's top speed, never below zero;
//! - **M-5a cutting route updates**: route changes after which the first leg, from the body to
//!   its first waypoint, fails `Pathfinder::segment_is_passable` at the terrain radius less
//!   [`WALL_PROBE_SLACK`] while the body itself is no deeper than that slack; the leg's least
//!   clearance sampled every [`DEPTH_SAMPLE_STEP`] is reported beside it, for reading;
//! - **M-5b held cutting legs**: walking unit-ticks whose held first leg fails the same test, as
//!   episodes, with walks, chases and follows apart ([`TaskKind`]);
//! - the **depth check** of the plan's H-W1: the deepest any observed body lies inside a wall.
//!
//! Each M-2 tick is also classed as the wall-grazing evidence's probe classed it ([`WallClass`]):
//! the proposed step points into the wall (**aim**), collision pushed the body further into it
//! than it proposed (**push**), or neither (**ride**). The probe's counts at clearances below
//! [`EVIDENCE_TOUCH`] and [`EVIDENCE_NEAR`] are kept beside M-1 and M-2 for continuity.
//! Every figure is given per reporting region (named boxes, then "elsewhere"), for the whole
//! map, and per owner.
//!
//! # How it fits
//! A meter of [`super::route::run_route_trace`], beside the wall-overlap meter
//! ([`super::route_overlap`]), whose pattern it follows. Its outputs are new files
//! (`clearance.csv`, `clearance_episodes.csv`, `clearance_routes.csv`, `clearance_summary.md`);
//! the trace's other outputs do not depend on it. The scratch probe of evidence
//! `2026-11-29-wall-grazing` (`~/scratch/wall-grazing/probe/`), ported and tested. The world is
//! read in [`ClearanceMeter::observe`]; the accounting is the pure [`ClearanceMeter::record`],
//! which the tests drive with hand-made samples.
//!
//! # Invariants
//! - The meter only reads the world; its queries use the pathfinder's own scratch buffers, which
//!   no simulation state reads, so recording a battle does not change it.
//! - Floats are used for reporting only; the predicates are the simulation's own fixed-point
//!   ones.
//! - Samples are recorded in tick order; an episode ends at a tick that is not M-2 or at a gap in
//!   the unit's ticks.
//! - Regions are reporting boxes only; they never change what is measured.

use super::blocking::{is_reversal, REVERSAL_MIN_STEP};
use super::route::RouteBox;
use super::{attack_target, to_f64};
use sim_core::SimWorld;
use sim_map::{PathRequestWithVision, CORNER_SAFETY_MARGIN};
use sim_types::{EntityId, MoverType, SimScalar, SimVec2, UnitOrder};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The tangent tolerance, world units (2⁻¹⁴): `sim_core`'s steering probe slack
/// (`movement::tuning::WALL_PROBE_SLACK`), which is not exported, named here for measurement. A
/// body nearer a wall than this only touches it (plan §4.6).
pub const WALL_PROBE_SLACK: f64 = 1.0 / 16_384.0;

/// [`WALL_PROBE_SLACK`] in fixed point, for the segment test of M-5a and M-5b.
const WALL_PROBE_SLACK_FIXED: SimScalar = SimScalar::from_raw(1 << 18);

/// How far beyond the terrain radius the nearest wall is looked for, world units (the probe's
/// reach). A body with no wall within it has a clearance of at least this much.
pub const CLEARANCE_REACH: f64 = 1.5;

/// How far beyond the terrain radius each sample of a cutting leg looks for a wall, world units
/// (the probe's sampling reach).
const SAMPLE_REACH: f64 = 0.5;

/// Spacing of the samples along a cutting leg, world units (the probe's).
pub const DEPTH_SAMPLE_STEP: f64 = 0.02;

/// The evidence's "touching" clearance, world units: its counts are reported beside M-1.
pub const EVIDENCE_TOUCH: f64 = 0.005;

/// The evidence's "near" clearance, world units (its episode listing): reported beside M-2.
pub const EVIDENCE_NEAR: f64 = 0.01;

/// Normal component of a step, world units per tick, beyond which the probe's classes call it
/// into the wall ([`WallClass`]).
pub const CLASS_TOLERANCE: f64 = 0.001;

/// The route's designed clearance, `sim_map::CORNER_SAFETY_MARGIN`, in world units.
#[must_use]
pub fn margin() -> f64 {
    to_f64(CORNER_SAFETY_MARGIN)
}

/// The reporting regions of evidence `2026-11-29-wall-grazing` §1 on the showcase map, world
/// units, bounds inclusive: the channel of ramp W, the channel of ramp E and the central block.
#[must_use]
pub fn clearance_boxes() -> Vec<RouteBox> {
    let named = |name: &str, min: (f64, f64), max: (f64, f64)| RouteBox {
        name: name.to_owned(),
        min,
        max,
    };
    vec![
        named("ramp_w_channel", (24.5, 6.5), (30.5, 13.0)),
        named("ramp_e_channel", (33.0, 51.0), (39.5, 57.5)),
        named("central_block", (24.0, 24.0), (39.0, 39.0)),
    ]
}

/// What put a body within the margin, as the evidence's probe classed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallClass {
    /// The proposed step points into the wall by more than [`CLASS_TOLERANCE`].
    Aim,
    /// The achieved step goes further into the wall than the proposed one by more than
    /// [`CLASS_TOLERANCE`], or the unit proposed no step: collision put it there.
    Push,
    /// Neither: the body walks along the wall.
    Ride,
}

impl WallClass {
    /// The class of a step: `proposal` and `step` (world units per tick) against `normal`, the
    /// unit normal pointing from the wall to the body.
    #[must_use]
    pub fn of(proposal: Option<(f64, f64)>, step: (f64, f64), normal: (f64, f64)) -> Self {
        let dot = |v: (f64, f64)| v.0 * normal.0 + v.1 * normal.1;
        match proposal.map(dot) {
            Some(into) if into < -CLASS_TOLERANCE => Self::Aim,
            Some(into) if dot(step) >= into - CLASS_TOLERANCE => Self::Ride,
            _ => Self::Push,
        }
    }

    /// The class's name in the outputs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Aim => "aim",
            Self::Push => "push",
            Self::Ride => "ride",
        }
    }
}

/// What a unit's motion follows, read from its orders: M-5b reports each apart (plan D-2.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskKind {
    /// A walk to a point: no attack target and no follow order.
    Walk,
    /// A pursuit of an attack target (the order's, or the acquired one).
    Chase,
    /// A follow order (`UnitOrder::Follow`).
    Follow,
}

impl TaskKind {
    /// Every kind, in report order.
    pub const ALL: [Self; 3] = [Self::Walk, Self::Chase, Self::Follow];

    /// The kind's name in the outputs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Walk => "walk",
            Self::Chase => "chase",
            Self::Follow => "follow",
        }
    }

    /// The kind's position in [`Self::ALL`] and in the per-kind arrays of [`RouteFigures`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Walk => 0,
            Self::Chase => 1,
            Self::Follow => 2,
        }
    }
}

/// A route change seen on one unit-tick.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteUpdate {
    /// Waypoints on the new route.
    pub waypoints: usize,
    /// Its first waypoint, world units.
    pub first: (f64, f64),
    /// Whether its first leg cuts a wall (M-5a).
    pub cutting: bool,
    /// For a cutting leg, its least clearance sampled every [`DEPTH_SAMPLE_STEP`] (negative inside
    /// a wall) and where, world units.
    pub sampled: Option<(f64, (f64, f64))>,
}

/// One observed ground unit-tick: the input of the accounting.
#[derive(Debug, Clone, PartialEq)]
pub struct ClearanceSample {
    /// The tick just stepped.
    pub tick: u32,
    /// Raw entity ID.
    pub entity: u32,
    /// Owner slot.
    pub owner: u8,
    /// Archetype slug.
    pub unit: String,
    /// What its motion follows.
    pub task: TaskKind,
    /// Centre after the tick, world units.
    pub position: (f64, f64),
    /// Achieved step of the tick (`position − pos_at_tick_start`), world units per tick.
    pub step: (f64, f64),
    /// Proposed step (`SimWorld::proposed_step`), world units per tick, if it proposed one.
    pub proposal: Option<(f64, f64)>,
    /// Clearance at the terrain radius, world units (negative inside a wall); `None` when no wall
    /// lies within [`CLEARANCE_REACH`] of the disc.
    pub clearance: Option<f64>,
    /// Unit normal from the nearest wall point to the centre (the edge's open-side normal for a
    /// centre on the wall); zero without a wall.
    pub normal: (f64, f64),
    /// The archetype's top speed, world units per tick.
    pub top_speed: f64,
    /// Whether the held first leg cuts a wall (the M-5a test on the held route). Tested on
    /// walking unit-ticks and on route changes only; `false` otherwise.
    pub held_cut: bool,
    /// The route change of this tick, if the route changed.
    pub route_update: Option<RouteUpdate>,
}

impl ClearanceSample {
    /// Whether the unit walked this tick: a non-zero proposal, or an achieved step longer than
    /// [`REVERSAL_MIN_STEP`].
    #[must_use]
    pub fn walking(&self) -> bool {
        self.proposal.is_some_and(|p| p != (0.0, 0.0))
            || self.step.0.hypot(self.step.1) > REVERSAL_MIN_STEP
    }

    /// Whether the unit-tick is M-2: walking with a clearance below the margin.
    #[must_use]
    pub fn within_margin(&self) -> bool {
        self.walking() && self.clearance.is_some_and(|c| c < margin())
    }
}

/// The figures of one region, the whole map or one owner.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ClearanceFigures {
    /// Walking unit-ticks.
    pub walking: u32,
    /// M-1: walking unit-ticks with a clearance of at most [`WALL_PROBE_SLACK`].
    pub touching: u32,
    /// Walking unit-ticks with a clearance below [`EVIDENCE_TOUCH`].
    pub under_touch: u32,
    /// Walking unit-ticks with a clearance below [`EVIDENCE_NEAR`].
    pub under_near: u32,
    /// M-2: walking unit-ticks with a clearance below the margin.
    pub within_margin: u32,
    /// M-2 unit-ticks per class: aim, push, ride.
    pub margin_classes: [u32; 3],
    /// Unit-ticks below [`EVIDENCE_TOUCH`] per class: aim, push, ride (the evidence's table).
    pub touch_classes: [u32; 3],
    /// Episodes that started here.
    pub episodes: u32,
    /// The longest episode that started here: `(length, entity, last tick)`.
    pub longest: Option<(u32, u32, u32)>,
    /// M-3: reversals at a wall.
    pub reversals: u32,
    /// M-3 reversals whose proposed steps reversed too.
    pub proposal_reversals: u32,
    /// M-4: ticks lost over the episodes that started here.
    pub ticks_lost: f64,
}

impl ClearanceFigures {
    fn count_tick(&mut self, sample: &ClearanceSample, class: Option<WallClass>) {
        self.walking += 1;
        let Some(clearance) = sample.clearance else {
            return;
        };
        self.touching += u32::from(clearance <= WALL_PROBE_SLACK);
        self.under_touch += u32::from(clearance < EVIDENCE_TOUCH);
        self.under_near += u32::from(clearance < EVIDENCE_NEAR);
        if let Some(class) = class {
            self.within_margin += 1;
            self.margin_classes[class_index(class)] += 1;
            if clearance < EVIDENCE_TOUCH {
                self.touch_classes[class_index(class)] += 1;
            }
        }
    }

    fn count_episode(&mut self, episode: &Episode) {
        self.episodes += 1;
        self.ticks_lost += episode.ticks_lost;
        if self
            .longest
            .is_none_or(|(length, ..)| episode.length > length)
        {
            self.longest = Some((episode.length, episode.entity, episode.end_tick));
        }
    }
}

const fn class_index(class: WallClass) -> usize {
    match class {
        WallClass::Aim => 0,
        WallClass::Push => 1,
        WallClass::Ride => 2,
    }
}

/// What an episode counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpisodeKind {
    /// A run of M-2 unit-ticks.
    Margin,
    /// A run of M-5b unit-ticks (walking with a held cutting leg).
    HeldCut,
}

/// A maximal run of consecutive unit-ticks of one unit (M-2 or M-5b).
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    /// What it counts.
    pub kind: EpisodeKind,
    /// Raw entity ID.
    pub entity: u32,
    /// Owner slot.
    pub owner: u8,
    /// Archetype slug.
    pub unit: String,
    /// The unit's task at the first tick.
    pub task: TaskKind,
    /// Index of the region of the first tick (the number of boxes for "elsewhere").
    pub region: usize,
    /// First tick.
    pub start_tick: u32,
    /// Last tick.
    pub end_tick: u32,
    /// Ticks in the run.
    pub length: u32,
    /// Centre before the first tick's step, world units.
    pub from: (f64, f64),
    /// Centre after the last tick, world units.
    pub to: (f64, f64),
    /// The archetype's top speed, world units per tick.
    pub top_speed: f64,
    /// M-4: `max(0, length − |to − from| ÷ top_speed)` for a margin episode; zero otherwise.
    pub ticks_lost: f64,
    /// The least clearance in the run, world units.
    pub min_clearance: f64,
    /// Ticks per class (aim, push, ride); zero for a held-cut episode.
    pub classes: [u32; 3],
}

impl Episode {
    fn open(kind: EpisodeKind, sample: &ClearanceSample, region: usize) -> Self {
        Self {
            kind,
            entity: sample.entity,
            owner: sample.owner,
            unit: sample.unit.clone(),
            task: sample.task,
            region,
            start_tick: sample.tick,
            end_tick: sample.tick,
            length: 0,
            from: (
                sample.position.0 - sample.step.0,
                sample.position.1 - sample.step.1,
            ),
            to: sample.position,
            top_speed: sample.top_speed,
            ticks_lost: 0.0,
            min_clearance: f64::INFINITY,
            classes: [0; 3],
        }
    }

    fn extend(&mut self, sample: &ClearanceSample, class: Option<WallClass>) {
        self.length += 1;
        self.end_tick = sample.tick;
        self.to = sample.position;
        if let Some(clearance) = sample.clearance {
            self.min_clearance = self.min_clearance.min(clearance);
        }
        if let Some(class) = class {
            self.classes[class_index(class)] += 1;
        }
    }

    fn close(mut self) -> Self {
        if self.kind == EpisodeKind::Margin && self.top_speed > 0.0 {
            let net = (self.to.0 - self.from.0).hypot(self.to.1 - self.from.1);
            self.ticks_lost = (f64::from(self.length) - net / self.top_speed).max(0.0);
        }
        self
    }
}

/// One unit-tick listed in `clearance.csv`: M-2, holding a cutting leg, or reversing at a wall.
#[derive(Debug, Clone, PartialEq)]
pub struct ClearanceRow {
    /// The sample.
    pub sample: ClearanceSample,
    /// Index of its region.
    pub region: usize,
    /// Its class, for an M-2 tick.
    pub class: Option<WallClass>,
    /// `Some(proposal-caused)` for a counted M-3 reversal.
    pub reversal: Option<bool>,
}

/// The route figures of a run (M-5a, M-5b).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RouteFigures {
    /// Route changes seen.
    pub updates: u32,
    /// M-5a: route changes whose first leg cuts a wall.
    pub cutting_updates: u32,
    /// The least sampled clearance of a cutting leg, world units.
    pub deepest_cut: Option<f64>,
    /// M-5b walking unit-ticks with a held cutting leg, per [`TaskKind`].
    pub held_cut_ticks: [u32; 3],
    /// M-5b episodes per [`TaskKind`].
    pub held_cut_episodes: [u32; 3],
    /// The longest M-5b episode per [`TaskKind`]: `(length, entity, last tick)`.
    pub held_cut_longest: [Option<(u32, u32, u32)>; 3],
}

/// A unit's state between ticks.
#[derive(Debug, Clone)]
struct UnitState {
    tick: u32,
    step: (f64, f64),
    proposal: Option<(f64, f64)>,
    within_margin: bool,
    margin_run: Option<Episode>,
    cut_run: Option<Episode>,
}

/// Accumulates the clearance figures tick by tick ([`Self::observe`]), then hands them over as a
/// [`ClearanceReport`] ([`Self::finish`]).
#[derive(Debug, Clone)]
pub struct ClearanceMeter {
    boxes: Vec<RouteBox>,
    units: BTreeMap<u32, UnitState>,
    routes: BTreeMap<EntityId, Vec<SimVec2>>,
    regions: Vec<ClearanceFigures>,
    whole: ClearanceFigures,
    owners: BTreeMap<u8, ClearanceFigures>,
    route_figures: RouteFigures,
    episodes: Vec<Episode>,
    rows: Vec<ClearanceRow>,
    updates: Vec<(ClearanceSample, RouteUpdate)>,
    deepest: Option<(f64, u32, u32, (f64, f64))>,
}

impl ClearanceMeter {
    /// A meter reporting on `boxes` (in order, then "elsewhere").
    #[must_use]
    pub fn new(boxes: Vec<RouteBox>) -> Self {
        let regions = vec![ClearanceFigures::default(); boxes.len() + 1];
        Self {
            boxes,
            units: BTreeMap::new(),
            routes: BTreeMap::new(),
            regions,
            whole: ClearanceFigures::default(),
            owners: BTreeMap::new(),
            route_figures: RouteFigures::default(),
            episodes: Vec::new(),
            rows: Vec::new(),
            updates: Vec::new(),
            deepest: None,
        }
    }

    /// The index of the region of `point` (world units): the first box that contains it, else
    /// the number of boxes ("elsewhere").
    #[must_use]
    pub fn region_of(&self, point: (f64, f64)) -> usize {
        self.boxes
            .iter()
            .position(|area| area.contains(point))
            .unwrap_or(self.boxes.len())
    }

    /// Measures every ground body of `world` after tick `tick` and records it.
    pub fn observe(&mut self, world: &mut SimWorld, tick: u32) {
        for body in ground_bodies(world) {
            let sample = self.measure(world, tick, body);
            self.record(sample);
        }
    }

    /// The sample of one body: its clearance, its held leg, and its route change.
    fn measure(&mut self, world: &mut SimWorld, tick: u32, body: Body) -> ClearanceSample {
        let wall = nearest_wall(world, body.position, body.radius, CLEARANCE_REACH);
        let clearance = wall.map(|(clearance, _)| clearance);
        let shallow = clearance.is_none_or(|c| c >= -WALL_PROBE_SLACK);
        let first = body.route.first().copied();
        let changed = !body.route.is_empty()
            && self
                .routes
                .get(&body.id)
                .is_none_or(|old| *old != body.route);
        let walking = body.proposal.is_some_and(|p| p != (0.0, 0.0))
            || body.step.0.hypot(body.step.1) > REVERSAL_MIN_STEP;
        let held_cut = shallow
            && (walking || changed)
            && first.is_some_and(|to| leg_cuts(world, body.position, to, body.radius));
        let route_update = first.filter(|_| changed).map(|to| RouteUpdate {
            waypoints: body.route.len(),
            first: point(to),
            cutting: held_cut,
            sampled: held_cut
                .then(|| sampled_depth(world, body.position, to, body.radius))
                .flatten(),
        });
        let route_len = body.route.len();
        if route_len == 0 {
            self.routes.remove(&body.id);
        } else {
            self.routes.insert(body.id, body.route);
        }
        ClearanceSample {
            tick,
            entity: body.id.0,
            owner: body.owner,
            unit: body.unit,
            task: body.task,
            position: point(body.position),
            step: body.step,
            proposal: body.proposal,
            clearance,
            normal: wall.map_or((0.0, 0.0), |(_, normal)| normal),
            top_speed: body.top_speed,
            held_cut,
            route_update,
        }
    }

    /// Accounts one unit-tick. Samples must come in tick order; within a tick, in any order.
    pub fn record(&mut self, sample: ClearanceSample) {
        let region = self.region_of(sample.position);
        if let Some(clearance) = sample.clearance {
            let depth = -clearance;
            if depth > 0.0 && self.deepest.is_none_or(|(deepest, ..)| depth > deepest) {
                self.deepest = Some((depth, sample.entity, sample.tick, sample.position));
            }
        }
        if let Some(update) = sample.route_update.clone() {
            self.count_update(&sample, update);
        }
        let mut state = self.take_state(&sample);
        let walking = sample.walking();
        let within = sample.within_margin();
        let class = within.then(|| WallClass::of(sample.proposal, sample.step, sample.normal));
        let reversal = state.as_ref().and_then(|last| {
            (last.tick + 1 == sample.tick
                && (last.within_margin || within)
                && is_reversal(last.step, sample.step))
            .then(|| {
                last.proposal
                    .zip(sample.proposal)
                    .is_some_and(|(before, now)| is_reversal(before, now))
            })
        });
        if walking {
            self.figures(region, sample.owner, |f| f.count_tick(&sample, class));
        }
        if let Some(proposal) = reversal {
            self.figures(region, sample.owner, |f| {
                f.reversals += 1;
                f.proposal_reversals += u32::from(proposal);
            });
        }
        let held = walking && sample.held_cut;
        if held {
            self.route_figures.held_cut_ticks[sample.task.index()] += 1;
        }
        let (margin_run, cut_run) = state
            .as_mut()
            .map_or((None, None), |s| (s.margin_run.take(), s.cut_run.take()));
        let margin_run = self.advance(margin_run, within, EpisodeKind::Margin, &sample, class);
        let cut_run = self.advance(cut_run, held, EpisodeKind::HeldCut, &sample, None);
        self.units.insert(
            sample.entity,
            UnitState {
                tick: sample.tick,
                step: sample.step,
                proposal: sample.proposal,
                within_margin: within,
                margin_run,
                cut_run,
            },
        );
        if within || held || reversal.is_some() {
            self.rows.push(ClearanceRow {
                sample,
                region,
                class,
                reversal,
            });
        }
    }

    /// The unit's state from its previous tick; runs broken by a gap are closed first.
    fn take_state(&mut self, sample: &ClearanceSample) -> Option<UnitState> {
        let mut state = self.units.remove(&sample.entity)?;
        if state.tick + 1 != sample.tick {
            for run in [state.margin_run.take(), state.cut_run.take()]
                .into_iter()
                .flatten()
            {
                self.close(run);
            }
        }
        Some(state)
    }

    /// Extends `run` with `sample` when `active`, opens one when none is open, and closes it
    /// otherwise.
    fn advance(
        &mut self,
        run: Option<Episode>,
        active: bool,
        kind: EpisodeKind,
        sample: &ClearanceSample,
        class: Option<WallClass>,
    ) -> Option<Episode> {
        if !active {
            if let Some(run) = run {
                self.close(run);
            }
            return None;
        }
        let mut run =
            run.unwrap_or_else(|| Episode::open(kind, sample, self.region_of(sample.position)));
        run.extend(sample, class);
        Some(run)
    }

    fn close(&mut self, run: Episode) {
        let run = run.close();
        match run.kind {
            EpisodeKind::Margin => {
                let region = run.region;
                self.figures(region, run.owner, |f| f.count_episode(&run));
            }
            EpisodeKind::HeldCut => {
                let task = run.task.index();
                let figures = &mut self.route_figures;
                figures.held_cut_episodes[task] += 1;
                if figures.held_cut_longest[task].is_none_or(|(length, ..)| run.length > length) {
                    figures.held_cut_longest[task] = Some((run.length, run.entity, run.end_tick));
                }
            }
        }
        self.episodes.push(run);
    }

    fn count_update(&mut self, sample: &ClearanceSample, update: RouteUpdate) {
        let figures = &mut self.route_figures;
        figures.updates += 1;
        if update.cutting {
            figures.cutting_updates += 1;
            if let Some((depth, _)) = update.sampled {
                if figures.deepest_cut.is_none_or(|deepest| depth < deepest) {
                    figures.deepest_cut = Some(depth);
                }
            }
        }
        self.updates.push((sample.clone(), update));
    }

    /// Applies `change` to the figures of `region`, of the whole map and of `owner`.
    fn figures(&mut self, region: usize, owner: u8, change: impl Fn(&mut ClearanceFigures)) {
        if let Some(figures) = self.regions.get_mut(region) {
            change(figures);
        }
        change(&mut self.whole);
        change(self.owners.entry(owner).or_default());
    }

    /// Closes every open run and hands the figures over.
    #[must_use]
    pub fn finish(mut self) -> ClearanceReport {
        let open: Vec<UnitState> = std::mem::take(&mut self.units).into_values().collect();
        for state in open {
            for run in [state.margin_run, state.cut_run].into_iter().flatten() {
                self.close(run);
            }
        }
        self.episodes
            .sort_by_key(|e| (e.start_tick, e.entity, e.kind == EpisodeKind::HeldCut));
        let mut names: Vec<String> = self.boxes.iter().map(|b| b.name.clone()).collect();
        names.push("elsewhere".to_owned());
        ClearanceReport {
            regions: names.into_iter().zip(self.regions).collect(),
            whole: self.whole,
            owners: self.owners.into_iter().collect(),
            routes: self.route_figures,
            episodes: self.episodes,
            rows: self.rows,
            updates: self.updates,
            deepest: self.deepest,
        }
    }
}

/// What the meter measured over a run.
#[derive(Debug, Clone, PartialEq)]
pub struct ClearanceReport {
    /// Per region, in box order then "elsewhere": its name and figures.
    pub regions: Vec<(String, ClearanceFigures)>,
    /// The whole map.
    pub whole: ClearanceFigures,
    /// Per owner slot, ascending.
    pub owners: Vec<(u8, ClearanceFigures)>,
    /// Route updates and held cutting legs.
    pub routes: RouteFigures,
    /// Every episode, in start tick then entity order.
    pub episodes: Vec<Episode>,
    /// Every unit-tick listed in `clearance.csv`, in tick order.
    pub rows: Vec<ClearanceRow>,
    /// Every route change, in tick order, with its sample.
    pub updates: Vec<(ClearanceSample, RouteUpdate)>,
    /// The deepest body inside a wall: `(depth, entity, tick, position)`, world units.
    pub deepest: Option<(f64, u32, u32, (f64, f64))>,
}

/// One ground body as the meter reads it from the world.
struct Body {
    id: EntityId,
    owner: u8,
    unit: String,
    task: TaskKind,
    position: SimVec2,
    step: (f64, f64),
    proposal: Option<(f64, f64)>,
    radius: SimScalar,
    top_speed: f64,
    route: Vec<SimVec2>,
}

/// Every living, mobile, unloaded and undisplaced ground body of `world`, in entity order (the
/// bodies the wall-overlap meter measures).
fn ground_bodies(world: &SimWorld) -> Vec<Body> {
    let mut bodies = Vec::new();
    for (id, entity) in &world.entities {
        if entity.is_immobile
            || !entity.is_alive()
            || entity.is_loaded_in_transport()
            || entity.displacement.is_some()
        {
            continue;
        }
        let Ok(arch) = world.catalog.get(entity.unit_type_id) else {
            continue;
        };
        if arch.mover_type != MoverType::Ground {
            continue;
        }
        let task = if matches!(entity.order_queue.front(), Some(UnitOrder::Follow { .. })) {
            TaskKind::Follow
        } else if attack_target(entity).is_some() {
            TaskKind::Chase
        } else {
            TaskKind::Walk
        };
        let step = entity.position - entity.pos_at_tick_start;
        bodies.push(Body {
            id: *id,
            owner: entity.owner.0,
            unit: arch.slug.clone(),
            task,
            position: entity.position,
            step: point(step),
            proposal: world.proposed_step(*id).map(point),
            radius: arch.terrain_radius,
            top_speed: to_f64(arch.max_speed),
            route: entity.current_funnel_waypoints.clone(),
        });
    }
    bodies
}

/// The physical request of a body's disc of `radius` at `at`: true terrain, every structure, no
/// vision.
fn physical(at: SimVec2, radius: SimScalar) -> PathRequestWithVision<'static> {
    PathRequestWithVision {
        start: at,
        goal: at,
        radius,
        vision: None,
        block_unexplored_ramps: false,
        known_structures: None,
    }
}

/// The clearance of a disc of terrain radius `radius` at `at`, world units, and the unit normal
/// from the nearest wall point to `at`; `None` when no wall lies within `radius + reach`.
fn nearest_wall(
    world: &mut SimWorld,
    at: SimVec2,
    radius: SimScalar,
    reach: f64,
) -> Option<(f64, (f64, f64))> {
    let request = physical(at, radius + SimScalar::from_num(reach));
    let contact = world.pathfinder.wall_contact(&world.map, &request, at)?;
    #[expect(
        clippy::cast_precision_loss,
        reason = "reporting only: the clearance drives no simulation decision"
    )]
    let distance = (contact.distance_sq as f64).sqrt() / 4_294_967_296.0;
    let away = point(at - contact.point);
    let length = away.0.hypot(away.1);
    let normal = if length > 0.0 {
        (away.0 / length, away.1 / length)
    } else {
        point(contact.normal)
    };
    Some((distance - to_f64(radius), normal))
}

/// Whether the leg from `from` to `to` fails the disc of terrain radius `radius` less
/// [`WALL_PROBE_SLACK`]: the exact segment predicate of routes and movement, no sampling.
fn leg_cuts(world: &mut SimWorld, from: SimVec2, to: SimVec2, radius: SimScalar) -> bool {
    let swept = radius.saturating_sub(WALL_PROBE_SLACK_FIXED);
    !world
        .pathfinder
        .segment_is_passable(&world.map, &physical(from, swept), from, to)
}

/// The least clearance of a disc of terrain radius `radius` along the leg from `from` to `to`,
/// sampled every [`DEPTH_SAMPLE_STEP`], and where, world units; `None` when no wall lies within
/// [`SAMPLE_REACH`] of any sample.
fn sampled_depth(
    world: &mut SimWorld,
    from: SimVec2,
    to: SimVec2,
    radius: SimScalar,
) -> Option<(f64, (f64, f64))> {
    let (a, b) = (point(from), point(to));
    let length = (b.0 - a.0).hypot(b.1 - a.1);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a sample count of a leg on the map, far below u32::MAX"
    )]
    let samples = (length / DEPTH_SAMPLE_STEP).ceil().max(1.0) as u32;
    let mut least: Option<(f64, (f64, f64))> = None;
    for k in 0..=samples {
        let s = f64::from(k) / f64::from(samples);
        let at = (a.0 + (b.0 - a.0) * s, a.1 + (b.1 - a.1) * s);
        let fixed = SimVec2::new(SimScalar::from_num(at.0), SimScalar::from_num(at.1));
        if let Some((clearance, _)) = nearest_wall(world, fixed, radius, SAMPLE_REACH) {
            if least.is_none_or(|(best, _)| clearance < best) {
                least = Some((clearance, at));
            }
        }
    }
    least
}

fn point(v: SimVec2) -> (f64, f64) {
    (to_f64(v.x), to_f64(v.y))
}

/// `clearance.csv`: one row per M-2 unit-tick, unit-tick with a held cutting leg, or M-3
/// reversal. Clearance and steps in world units (per tick), six decimals.
#[must_use]
pub fn clearance_csv(report: &ClearanceReport) -> String {
    let mut out = String::from(
        "tick,entity,owner,unit,task,region,x,y,step_x,step_y,proposal_x,proposal_y,clearance,\
         class,touching,held_cut,reversal\n",
    );
    for row in &report.rows {
        let s = &row.sample;
        let (px, py) = s.proposal.map_or((String::new(), String::new()), |p| {
            (format!("{:.6}", p.0), format!("{:.6}", p.1))
        });
        let clearance = s.clearance.map_or_else(String::new, |c| format!("{c:.6}"));
        let touching = s.walking() && s.clearance.is_some_and(|c| c <= WALL_PROBE_SLACK);
        let reversal = row
            .reversal
            .map_or("", |proposal| if proposal { "proposal" } else { "contact" });
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{:.4},{:.4},{:.6},{:.6},{px},{py},{clearance},{},{touching},{},{reversal}",
            s.tick,
            s.entity,
            s.owner,
            s.unit,
            s.task.name(),
            region_name(report, row.region),
            s.position.0,
            s.position.1,
            s.step.0,
            s.step.1,
            row.class.map_or("", WallClass::name),
            s.held_cut,
        );
    }
    out
}

/// `clearance_episodes.csv`: one row per episode, margin and held-cut alike.
#[must_use]
pub fn episodes_csv(report: &ClearanceReport) -> String {
    let mut out = String::from(
        "kind,entity,owner,unit,task,region,start_tick,end_tick,length,from_x,from_y,to_x,to_y,\
         ticks_lost,min_clearance,aim,push,ride\n",
    );
    for e in &report.episodes {
        let kind = match e.kind {
            EpisodeKind::Margin => "margin",
            EpisodeKind::HeldCut => "held_cut",
        };
        let min = if e.min_clearance.is_finite() {
            format!("{:.6}", e.min_clearance)
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "{kind},{},{},{},{},{},{},{},{},{:.4},{:.4},{:.4},{:.4},{:.2},{min},{},{},{}",
            e.entity,
            e.owner,
            e.unit,
            e.task.name(),
            region_name(report, e.region),
            e.start_tick,
            e.end_tick,
            e.length,
            e.from.0,
            e.from.1,
            e.to.0,
            e.to.1,
            e.ticks_lost,
            e.classes[0],
            e.classes[1],
            e.classes[2],
        );
    }
    out
}

/// `clearance_routes.csv`: one row per route change, with the M-5a verdict of its first leg.
#[must_use]
pub fn routes_csv(report: &ClearanceReport) -> String {
    let mut out = String::from(
        "tick,entity,owner,unit,task,x,y,clearance,waypoints,first_x,first_y,cutting,\
         sampled_depth,at_x,at_y\n",
    );
    for (s, u) in &report.updates {
        let clearance = s.clearance.map_or_else(String::new, |c| format!("{c:.6}"));
        let (depth, ax, ay) = u.sampled.map_or_else(
            || (String::new(), String::new(), String::new()),
            |(depth, at)| {
                (
                    format!("{depth:.4}"),
                    format!("{:.3}", at.0),
                    format!("{:.3}", at.1),
                )
            },
        );
        let _ = writeln!(
            out,
            "{},{},{},{},{},{:.4},{:.4},{clearance},{},{:.4},{:.4},{},{depth},{ax},{ay}",
            s.tick,
            s.entity,
            s.owner,
            s.unit,
            s.task.name(),
            s.position.0,
            s.position.1,
            u.waypoints,
            u.first.0,
            u.first.1,
            u.cutting,
        );
    }
    out
}

fn region_name(report: &ClearanceReport, region: usize) -> &str {
    report.regions.get(region).map_or("", |(name, _)| name)
}

/// `clearance_summary.md`: the figures per region and the whole map, per owner, the route
/// figures, and the depth check.
#[must_use]
pub fn clearance_markdown(report: &ClearanceReport) -> String {
    let mut out = format!(
        "Margin {:.3}; touching at clearance ≤ {WALL_PROBE_SLACK:.3e} (M-1); evidence counts below \
         {EVIDENCE_TOUCH} and {EVIDENCE_NEAR}. World units, 25 Hz ticks.\n\n",
        margin()
    );
    out.push_str(&figures_table(
        "Region",
        report
            .regions
            .iter()
            .map(|(name, figures)| (name.clone(), *figures))
            .chain([("whole map".to_owned(), report.whole)]),
    ));
    out.push('\n');
    out.push_str(&figures_table(
        "Owner",
        report.owners.iter().map(|(o, f)| (o.to_string(), *f)),
    ));
    out.push('\n');
    out.push_str(&routes_markdown(&report.routes));
    let deepest = report.deepest.map_or_else(
        || "none".to_owned(),
        |(depth, entity, tick, (x, y))| {
            format!("{depth:.3e} (entity {entity}, tick {tick}, ({x:.2}, {y:.2}))")
        },
    );
    let _ = writeln!(out, "\nDeepest body inside a wall (H-W1): {deepest}.");
    out
}

fn figures_table(label: &str, rows: impl Iterator<Item = (String, ClearanceFigures)>) -> String {
    let mut out = format!(
        "| {label} | Walking | M-1 touching | < {EVIDENCE_TOUCH} | < {EVIDENCE_NEAR} | M-2 margin | \
         M-2 aim / push / ride | < {EVIDENCE_TOUCH} aim / push / ride | Episodes | Longest \
         (unit, last tick) | M-3 reversals (proposal / contact) | M-4 ticks lost |\n\
         |---|---:|---:|---:|---:|---:|---|---|---:|---|---|---:|\n"
    );
    for (name, f) in rows {
        let longest = f.longest.map_or_else(
            || "—".to_owned(),
            |(length, entity, tick)| format!("{length} ({entity}, {tick})"),
        );
        let classes = |c: [u32; 3]| format!("{} / {} / {}", c[0], c[1], c[2]);
        let _ = writeln!(
            out,
            "| {name} | {} | {} | {} | {} | {} | {} | {} | {} | {longest} | {} ({} / {}) | {:.1} |",
            f.walking,
            f.touching,
            f.under_touch,
            f.under_near,
            f.within_margin,
            classes(f.margin_classes),
            classes(f.touch_classes),
            f.episodes,
            f.reversals,
            f.proposal_reversals,
            f.reversals - f.proposal_reversals,
            f.ticks_lost
        );
    }
    out
}

fn routes_markdown(routes: &RouteFigures) -> String {
    let deepest = routes
        .deepest_cut
        .map_or_else(|| "—".to_owned(), |d| format!("{d:.3}"));
    let mut out = format!(
        "Route updates: {}. M-5a cutting updates: {} (least sampled clearance {deepest}).\n\n\
         | Task | M-5b held cutting ticks | Episodes | Longest (unit, last tick) |\n\
         |---|---:|---:|---|\n",
        routes.updates, routes.cutting_updates
    );
    for task in TaskKind::ALL {
        let i = task.index();
        let longest = routes.held_cut_longest[i].map_or_else(
            || "—".to_owned(),
            |(length, entity, tick)| format!("{length} ({entity}, {tick})"),
        );
        let _ = writeln!(
            out,
            "| {} | {} | {} | {longest} |",
            task.name(),
            routes.held_cut_ticks[i],
            routes.held_cut_episodes[i]
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(tick: u32, x: f64, step: (f64, f64), clearance: f64) -> ClearanceSample {
        ClearanceSample {
            tick,
            entity: 4,
            owner: 1,
            unit: "u".to_owned(),
            task: TaskKind::Walk,
            position: (x, 10.0),
            step,
            proposal: Some(step),
            clearance: Some(clearance),
            normal: (0.0, 1.0),
            top_speed: 0.1,
            held_cut: false,
            route_update: None,
        }
    }

    #[test]
    fn the_classes_are_the_probes() {
        let normal = (0.0, 1.0);
        assert_eq!(
            WallClass::of(Some((0.1, -0.01)), (0.1, 0.0), normal),
            WallClass::Aim
        );
        assert_eq!(
            WallClass::of(Some((0.1, 0.0)), (0.1, -0.01), normal),
            WallClass::Push
        );
        assert_eq!(WallClass::of(None, (0.1, 0.0), normal), WallClass::Push);
        assert_eq!(
            WallClass::of(Some((0.1, 0.0)), (0.1, 0.0), normal),
            WallClass::Ride
        );
    }

    #[test]
    fn a_gap_in_a_units_ticks_ends_its_episode() {
        let mut meter = ClearanceMeter::new(Vec::new());
        for tick in [0, 1, 2, 5, 6] {
            meter.record(sample(tick, f64::from(tick) * 0.1, (0.1, 0.0), 0.01));
        }
        let report = meter.finish();
        let lengths: Vec<(u32, u32)> = report
            .episodes
            .iter()
            .map(|e| (e.start_tick, e.length))
            .collect();
        assert_eq!(lengths, [(0, 3), (5, 2)]);
        assert_eq!(report.whole.episodes, 2);
        assert_eq!(report.whole.longest, Some((3, 4, 2)));
    }

    #[test]
    fn a_standing_unit_is_not_walking_and_never_within_the_margin() {
        let mut still = sample(0, 1.0, (0.0, 0.0), 0.0);
        still.proposal = None;
        assert!(!still.walking());
        assert!(!still.within_margin());
        still.proposal = Some((0.0, 0.001));
        assert!(still.walking());
        assert!(still.within_margin());
    }

    #[test]
    fn the_csvs_list_rows_episodes_and_updates() {
        let mut meter = ClearanceMeter::new(clearance_boxes());
        let mut first = sample(3, 25.0, (0.1, 0.0), 0.0);
        first.position = (25.0, 10.0);
        first.route_update = Some(RouteUpdate {
            waypoints: 2,
            first: (29.0, 11.5),
            cutting: true,
            sampled: Some((-0.25, (26.0, 11.6))),
        });
        first.held_cut = true;
        meter.record(first);
        let report = meter.finish();
        assert_eq!(
            clearance_csv(&report).lines().nth(1),
            Some(
                "3,4,1,u,walk,ramp_w_channel,25.0000,10.0000,0.100000,0.000000,0.100000,\
                 0.000000,0.000000,ride,true,true,"
            )
        );
        assert_eq!(
            routes_csv(&report).lines().nth(1),
            Some(
                "3,4,1,u,walk,25.0000,10.0000,0.000000,2,29.0000,11.5000,true,-0.2500,26.000,\
                 11.600"
            )
        );
        let episodes = episodes_csv(&report);
        assert_eq!(episodes.lines().count(), 3, "{episodes}");
        assert!(episodes.contains("\nmargin,4,1,u,walk,ramp_w_channel,3,3,1,"));
        assert!(episodes.contains("\nheld_cut,4,1,u,walk,ramp_w_channel,3,3,1,"));
        let summary = clearance_markdown(&report);
        assert!(summary.contains("M-5a cutting updates: 1 (least sampled clearance -0.250)"));
        assert!(summary.contains("| walk | 1 | 1 | 1 (4, 3) |"));
    }
}
