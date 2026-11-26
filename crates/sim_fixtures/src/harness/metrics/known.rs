//! The meters the known answers of plan `2026-11-27-gated-movement`'s families read (§5.2, `WP0a`).
//!
//! # Responsibility
//! [`KnownMeter`] watches one scene of a new family after every tick, beside the scene's
//! [`super::ScenarioMeter`], and keeps what [`super::super::answers::judge`] needs and the base
//! columns do not hold ([`KnownRow`]):
//! - **H-1** (`held_moved`, every family): unit-ticks, and the largest displacement, of a unit
//!   that at the tick's start was holding or had its attack target in its band, proposed no step
//!   this tick, and was displaced by more than 2⁻²⁰ ([`moved_by_others`]);
//! - **H-2** (`enemy_moved`, every family): the same for an enemy (owner 1) that proposed no
//!   motion (holding, engaged, or without an order), and the largest overlap of two bodies of
//!   different owners at the end of any tick;
//! - **deep-queue**: friend overlap per rank (the rear rank of the pair), as M8 counts it;
//! - **wedge**: whether the walker's centre crossed the segment between the two bodies' centres,
//!   and its largest overlap with either;
//! - **hallway**: per pair of walkers from opposite files, the changes of the side (the sign of
//!   their lateral offset) while they pass, i.e. while their centres are within their radius
//!   sum along the corridor;
//! - **stutter**: per move-back and attacker, the lag from the tick whose bundle carried the
//!   order to the first step along it above `v / 8`, the displacement along it over the move
//!   (short when below `0.9 · v · (ticks − 1)`), and per attack order whether the first release
//!   came within one weapon period of the attacker first standing in its band;
//! - **surround-moving**: the first tick an attacker stood in its band of the walking target,
//!   the most attackers in it within the following 250 ticks, and the ticks until they first
//!   numbered `min(k, C)`;
//! - **lanes**: how many walkers crossed the wall through each gap, and the area of the convex
//!   hull of the walkers' discs at the end as a multiple of their hexagonally packed area
//!   (`n · 2√3 · r²`).
//!
//! # How it fits
//! [`super::super::run_scenario`] feeds it after every tick; the row's last columns are its
//! [`KnownRow::cells`] with the scene's verdict. Like the rest of the meter it reads the world
//! and nothing else, converting positions to `f64` for measurement only; the tick-start state of
//! H-1 and H-2 and the rule [`moved_by_others`] are the battle meter's
//! ([`crate::trace::contact_checks`]), so the harness and the matrix count the same thing.
//!
//! # Invariants
//! - The meter only reads the world.
//! - Observation `n` (from 1) is the state after the step that played tick `n − 1`'s bundle, so a
//!   cue of tick `t` is first visible at observation `t + 1`, and a lag of 1 is a step taken by the
//!   very tick that carried the order.
//! - A unit's first observation counts nothing: it has no tick-start state.

use super::{lattice, point, to_f64, Point, NEAR_GAP};
use crate::harness::answers::{
    Judgement, KnownAnswer, LaneUse, FILL_TICKS, FRIEND_MOVING_BOUND, FRIEND_STANDING_BOUND,
    STUTTER_SHARE,
};
use crate::harness::corpus::{Cue, CueOrder, Role, Scenario};
use crate::trace::contact_checks::{moved_by_others, standing_of, FirstContactPairs, HELD_EPSILON};
use sim_core::SimWorld;
use sim_types::EntityId;

/// The answer columns of one scene (module documentation); every column not of the scene's
/// family keeps its default.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KnownRow {
    /// H-1: unit-ticks a holding or engaged unit was displaced by others.
    pub held_moved_ticks: u32,
    /// H-1: the largest such displacement, world units.
    pub held_moved_max: f64,
    /// H-2: unit-ticks an enemy that proposed no motion was displaced.
    pub enemy_moved_ticks: u32,
    /// H-2: the largest such displacement, world units.
    pub enemy_moved_max: f64,
    /// H-2: the largest overlap of two bodies of different owners, share of the radius sum.
    pub enemy_pair_ov_max: f64,
    /// Deep-queue: per rank, the largest friend overlap share of a near pair whose rear unit is
    /// of that rank.
    pub rank_fo_max: Vec<f64>,
    /// Deep-queue: per rank, near pair-ticks above D-α's bound (5 % moving, 1 % standing).
    pub rank_fo_over: Vec<u32>,
    /// Deep-queue: per rank, near pair-ticks.
    pub rank_pair_ticks: Vec<u32>,
    /// Wedge: 1 when the walker crossed between the two bodies.
    pub wedge_passed: u32,
    /// Wedge: the walker's largest overlap share with either body.
    pub wedge_ov_max: f64,
    /// Hallway: side changes over every passing pair.
    pub side_changes: u32,
    /// Stutter: move-backs observed.
    pub stutter_windows: u32,
    /// Stutter: the largest lag, ticks, over every attacker and move-back.
    pub stutter_lag_max: u32,
    /// Stutter: attacker move-backs whose displacement fell short.
    pub stutter_short: u32,
    /// Stutter: attacker attack orders whose fire resumed late, or not at all, once in range.
    pub stutter_fire_late: u32,
    /// Surround-moving: the observation at which an attacker first stood in its band.
    pub first_contact: Option<u32>,
    /// Surround-moving: ticks from first contact until `min(k, C)` attackers stood in their band,
    /// if within [`FILL_TICKS`].
    pub fill_time: Option<u32>,
    /// Surround-moving: the most attackers in their band within [`FILL_TICKS`] of first contact.
    pub fill_max: u32,
    /// Lanes: walkers through each gap.
    pub lanes: LaneUse,
    /// Lanes: the final hull of the walkers' discs over their packed area.
    pub hull_ratio: f64,
    /// H-1's first-contact exclusion (plan Amendment 3, item 6): unit-ticks of
    /// [`Self::held_moved_ticks`] whose unit was, at the tick's start, in a pair that overlapped
    /// beyond the dead band when it first met ([`FirstContactPairs`]).
    pub held_moved_first_contact_ticks: u32,
    /// H-1 as judged: [`Self::held_moved_ticks`] less the excluded ones.
    pub held_moved_clean_ticks: u32,
    /// H-1 as judged: the largest displacement of [`Self::held_moved_clean_ticks`].
    pub held_moved_clean_max: f64,
    /// H-2 as repaired (Amendment 3, item 4): unit-ticks, of either side, of a unit that proposed
    /// no motion and that enemy pairs moved ([`SimWorld::enemy_pair_push`]), outside the
    /// exclusion. The original count, [`Self::enemy_moved_ticks`], stays reported.
    pub enemy_pair_moved_ticks: u32,
    /// H-2 as repaired: the largest such displacement, world units.
    pub enemy_pair_moved_max: f64,
}

impl KnownRow {
    /// The CSV header of the answer columns, in [`Self::cells`] order.
    pub const HEADER: &'static str = "held_moved_ticks,held_moved_max,enemy_moved_ticks,\
        enemy_moved_max,enemy_pair_ov_max,rank_fo_max,rank_fo_over,rank_pair_ticks,wedge_passed,\
        wedge_ov_max,side_changes,stutter_windows,stutter_lag_max,stutter_short,stutter_fire_late,\
        first_contact,fill_time,fill_max,lanes,hull_ratio,answer,hard,answer_failed,\
        held_moved_first_contact_ticks,held_moved_clean_ticks,held_moved_clean_max,\
        enemy_pair_moved_ticks,enemy_pair_moved_max";

    /// The answer columns and `verdict` as CSV cells: lists joined by `/`, an absent tick empty,
    /// the failed parts joined by `+`.
    #[must_use]
    pub fn cells(&self, verdict: &Judgement) -> String {
        let list = |values: &[u32]| {
            values
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("/")
        };
        let shares = self
            .rank_fo_max
            .iter()
            .map(|v| format!("{v:.5}"))
            .collect::<Vec<_>>()
            .join("/");
        let tick = |value: Option<u32>| value.map_or_else(String::new, |v| v.to_string());
        format!(
            "{},{:.9},{},{:.9},{:.5},{shares},{},{},{},{:.5},{},{},{},{},{},{},{},{},{}/{},{:.3},{},{},{},\
             {},{},{:.9},{},{:.9}",
            self.held_moved_ticks,
            self.held_moved_max,
            self.enemy_moved_ticks,
            self.enemy_moved_max,
            self.enemy_pair_ov_max,
            list(&self.rank_fo_over),
            list(&self.rank_pair_ticks),
            self.wedge_passed,
            self.wedge_ov_max,
            self.side_changes,
            self.stutter_windows,
            self.stutter_lag_max,
            self.stutter_short,
            self.stutter_fire_late,
            tick(self.first_contact),
            tick(self.fill_time),
            self.fill_max,
            self.lanes.0[0],
            self.lanes.0[1],
            self.hull_ratio,
            u8::from(verdict.holds),
            u8::from(verdict.hard),
            verdict.failed.join("+"),
            self.held_moved_first_contact_ticks,
            self.held_moved_clean_ticks,
            self.held_moved_clean_max,
            self.enemy_pair_moved_ticks,
            self.enemy_pair_moved_max,
        )
    }
}

/// One unit as the meter follows it.
#[derive(Debug, Clone)]
struct Unit {
    id: EntityId,
    owner: u8,
    role: Role,
    radius: f64,
    speed: f64,
    /// The unit's rank in a deep queue.
    rank: Option<usize>,
    /// Position at the latest observation.
    at: Point,
    /// Step of the latest observation.
    step: Option<Point>,
    /// Tick-start state for H-1 and H-2: held, proposed none, weapon cooldown; `None` before the
    /// first observation and while dead.
    before: Option<(bool, bool, u16)>,
}

/// One attacker's progress through a stutter cue.
#[derive(Debug, Clone, Copy, Default)]
struct CueState {
    /// Move-back: position when the cue was given.
    start: Point,
    /// Move-back: lag of the first step along the order.
    lag: Option<u32>,
    /// Attack: the observation at which the attacker first stood in its band.
    in_band: Option<u32>,
    /// Attack: whether its first release since then has been seen.
    fired: bool,
}

/// The stutter family's open cue.
#[derive(Debug, Clone)]
struct OpenCue {
    cue: Cue,
    /// The observation at which the next cue opens; `None` for the last.
    ends: Option<u32>,
    /// Per attacker (index into the meter's units).
    states: Vec<(usize, CueState)>,
}

/// The meter of the module documentation.
#[derive(Debug, Clone)]
pub struct KnownMeter {
    answer: KnownAnswer,
    units: Vec<Unit>,
    observed: u32,
    row: KnownRow,
    /// Wedge: the walker and the two bodies (indices into `units`).
    wedge: Option<(usize, [usize; 2])>,
    /// Hallway: the east-bound and the west-bound walkers, and each pair's last side.
    files: (Vec<usize>, Vec<usize>, Vec<Option<bool>>),
    /// Stutter: the script, the cue being watched, the target and the attackers' band, world
    /// units.
    script: Vec<Cue>,
    open: Option<OpenCue>,
    target: Option<(usize, f64)>,
    /// Surround-moving: `min(k, C)`.
    wanted: u32,
    /// H-1's and H-2's first-contact exclusion, as of the latest observation.
    first: FirstContactPairs,
}

impl KnownMeter {
    /// A meter for `scenario`, whose spawns are the world's `ids` in spawn order; `None` for a
    /// scene without a known answer (the older families).
    #[must_use]
    pub fn new(world: &SimWorld, scenario: &Scenario, ids: &[EntityId]) -> Option<Self> {
        let answer = scenario.known?;
        let units: Vec<Unit> = scenario
            .spawns
            .iter()
            .zip(ids)
            .filter_map(|(spawn, &id)| {
                let entity = world.entities.get(&id)?;
                let arch = world.catalog.get(entity.unit_type_id).ok()?;
                let rank = match answer {
                    KnownAnswer::DeepQueue { front, pitch, .. } if spawn.role == Role::Attack => {
                        usize::try_from((front - spawn.at.0) / pitch.max(1)).ok()
                    }
                    _ => None,
                };
                Some(Unit {
                    id,
                    owner: spawn.owner,
                    role: spawn.role,
                    radius: to_f64(arch.collision_radius),
                    speed: to_f64(arch.max_speed),
                    rank,
                    at: point(entity.position),
                    step: None,
                    before: None,
                })
            })
            .collect();
        let mut meter = Self {
            answer,
            units,
            observed: 0,
            row: KnownRow::default(),
            wedge: None,
            files: (Vec::new(), Vec::new(), Vec::new()),
            script: scenario.script.clone(),
            open: None,
            target: None,
            wanted: 0,
            first: FirstContactPairs::new(),
        };
        meter.first.observe(world);
        meter.prepare(world, scenario);
        meter.record_states(world);
        Some(meter)
    }

    /// Sets up the family's own state.
    fn prepare(&mut self, world: &SimWorld, scenario: &Scenario) {
        let index_where = |pick: &dyn Fn(&Unit) -> bool| self.units.iter().position(pick);
        match self.answer {
            KnownAnswer::DeepQueue { ranks, .. } => {
                let ranks = usize::try_from(ranks).unwrap_or(0);
                self.row.rank_fo_max = vec![0.0; ranks];
                self.row.rank_fo_over = vec![0; ranks];
                self.row.rank_pair_ticks = vec![0; ranks];
            }
            KnownAnswer::Wedge { bodies, .. } => {
                let walker = index_where(&|u| matches!(u.role, Role::OwnMove { .. }));
                let body = |at| {
                    scenario
                        .spawns
                        .iter()
                        .zip(&self.units)
                        .position(|(spawn, _)| spawn.at == at)
                };
                if let (Some(walker), Some(a), Some(b)) = (walker, body(bodies[0]), body(bodies[1]))
                {
                    self.wedge = Some((walker, [a, b]));
                }
            }
            KnownAnswer::Hallway => self.prepare_files(scenario),
            KnownAnswer::Stutter { .. } => {
                let target = index_where(&|u| u.role == Role::Pillar);
                self.target = target.map(|t| (t, self.band_against(world, t)));
            }
            KnownAnswer::SurroundMoving { capacity, band } => {
                let target = index_where(&|u| matches!(u.role, Role::Circle { .. }));
                self.target = target.map(|t| (t, lattice(band)));
                let attackers = self.units.iter().filter(|u| u.role == Role::Attack).count();
                self.wanted = u32::try_from(attackers).unwrap_or(u32::MAX).min(capacity);
            }
            KnownAnswer::HoldLine | KnownAnswer::EnemyPress { .. } | KnownAnswer::Lanes { .. } => {}
        }
    }

    /// Hallway: splits the walkers into the east-bound and the west-bound file.
    fn prepare_files(&mut self, scenario: &Scenario) {
        let (mut east, mut west) = (Vec::new(), Vec::new());
        for (index, spawn) in scenario.spawns.iter().enumerate() {
            if let Role::OwnMove { goal } = spawn.role {
                if goal.0 > spawn.at.0 {
                    east.push(index);
                } else {
                    west.push(index);
                }
            }
        }
        self.files = (
            east.clone(),
            west.clone(),
            vec![None; east.len() * west.len()],
        );
    }

    /// The attackers' band against the unit `target`: the first attacker's weapon range plus
    /// both radii, world units; 0 without one.
    fn band_against(&self, world: &SimWorld, target: usize) -> f64 {
        let target_radius = self.units[target].radius;
        self.units
            .iter()
            .filter(|u| u.role == Role::Attack)
            .find_map(|unit| {
                let entity = world.entities.get(&unit.id)?;
                let arch = world.catalog.get(entity.unit_type_id).ok()?;
                let weapon = arch.weapon.as_ref()?;
                Some(to_f64(weapon.max_range) + unit.radius + target_radius)
            })
            .unwrap_or(0.0)
    }

    /// Records every living unit's tick-start state for the next observation.
    fn record_states(&mut self, world: &SimWorld) {
        for unit in &mut self.units {
            unit.before = world
                .entities
                .get(&unit.id)
                .filter(|e| e.is_alive())
                .and_then(|entity| {
                    let arch = world.catalog.get(entity.unit_type_id).ok()?;
                    let standing = standing_of(world, entity, arch);
                    Some((
                        standing.held,
                        standing.proposed_none,
                        entity.weapon_cooldown_remaining,
                    ))
                });
        }
    }

    /// Feeds the tick the world just played.
    pub fn observe(&mut self, world: &SimWorld) {
        self.observed += 1;
        self.open_cue_if_due();
        let mut shots = Vec::new();
        for index in 0..self.units.len() {
            let Some(entity) = world
                .entities
                .get(&self.units[index].id)
                .filter(|e| e.is_alive())
            else {
                continue;
            };
            let now = point(entity.position);
            let proposed = world.proposed_step(entity.id).map(point);
            let unit = &mut self.units[index];
            let moved = (now.0 - unit.at.0, now.1 - unit.at.1);
            if let Some((held, proposed_none, cooldown)) = unit.before {
                let excluded = self.first.involves(unit.id);
                let by_others = moved_by_others(held, proposed, moved);
                if by_others > HELD_EPSILON {
                    self.row.held_moved_ticks += 1;
                    self.row.held_moved_max = self.row.held_moved_max.max(by_others);
                    if excluded {
                        self.row.held_moved_first_contact_ticks += 1;
                    } else {
                        self.row.held_moved_clean_ticks += 1;
                        self.row.held_moved_clean_max =
                            self.row.held_moved_clean_max.max(by_others);
                    }
                }
                let by_others = moved_by_others(proposed_none, proposed, moved);
                if unit.owner == 1 && by_others > HELD_EPSILON {
                    self.row.enemy_moved_ticks += 1;
                    self.row.enemy_moved_max = self.row.enemy_moved_max.max(by_others);
                }
                let pushed = to_f64(world.enemy_pair_push(unit.id)) > HELD_EPSILON;
                if pushed && !excluded && by_others > HELD_EPSILON {
                    self.row.enemy_pair_moved_ticks += 1;
                    self.row.enemy_pair_moved_max = self.row.enemy_pair_moved_max.max(by_others);
                }
                if entity.weapon_cooldown_remaining > cooldown {
                    shots.push(index);
                }
            }
            let previous = unit.at;
            unit.step = Some(moved);
            unit.at = now;
            self.observe_lane(index, previous);
        }
        self.observe_pairs();
        self.observe_wedge();
        self.observe_hallway();
        self.observe_cue(&shots);
        self.observe_fill();
        self.record_states(world);
        self.first.observe(world);
    }

    /// H-2's enemy overlap and deep-queue's per-rank friend overlap over every pair.
    fn observe_pairs(&mut self) {
        for a in 0..self.units.len() {
            for b in a + 1..self.units.len() {
                let (one, two) = (&self.units[a], &self.units[b]);
                let sum = one.radius + two.radius;
                let gap = distance(one.at, two.at);
                let overlap = (sum - gap) / sum;
                if one.owner != two.owner {
                    self.row.enemy_pair_ov_max = self.row.enemy_pair_ov_max.max(overlap);
                    continue;
                }
                let (Some(ra), Some(rb)) = (one.rank, two.rank) else {
                    continue;
                };
                if gap >= sum + NEAR_GAP {
                    continue;
                }
                let still = |u: &Unit| u.step.is_none_or(|s| s.0.hypot(s.1) <= u.speed / 8.0);
                let bound = if still(one) && still(two) {
                    FRIEND_STANDING_BOUND
                } else {
                    FRIEND_MOVING_BOUND
                };
                let rank = ra.max(rb);
                if let (Some(max), Some(over), Some(ticks)) = (
                    self.row.rank_fo_max.get_mut(rank),
                    self.row.rank_fo_over.get_mut(rank),
                    self.row.rank_pair_ticks.get_mut(rank),
                ) {
                    *max = max.max(overlap);
                    *over += u32::from(overlap > bound);
                    *ticks += 1;
                }
            }
        }
    }

    /// Wedge: the walker's crossing between the bodies and its overlap with them.
    fn observe_wedge(&mut self) {
        let Some((walker, bodies)) = self.wedge else {
            return;
        };
        let unit = &self.units[walker];
        let (a, b) = (&self.units[bodies[0]], &self.units[bodies[1]]);
        for body in [a, b] {
            let sum = unit.radius + body.radius;
            let overlap = (sum - distance(unit.at, body.at)) / sum;
            self.row.wedge_ov_max = self.row.wedge_ov_max.max(overlap);
        }
        if let Some(step) = unit.step {
            let from = (unit.at.0 - step.0, unit.at.1 - step.1);
            if segments_cross((from, unit.at), (a.at, b.at)) {
                self.row.wedge_passed = 1;
            }
        }
    }

    /// Hallway: the side every passing pair keeps.
    fn observe_hallway(&mut self) {
        let (east, west, sides) = &mut self.files;
        for (i, &e) in east.iter().enumerate() {
            for (j, &w) in west.iter().enumerate() {
                let (one, two) = (&self.units[e], &self.units[w]);
                if (one.at.0 - two.at.0).abs() > one.radius + two.radius {
                    continue;
                }
                let lateral = one.at.1 - two.at.1;
                if lateral.abs() <= HELD_EPSILON {
                    continue;
                }
                let above = lateral > 0.0;
                if let Some(side) = sides.get_mut(i * west.len() + j) {
                    if side.is_some_and(|was| was != above) {
                        self.row.side_changes += 1;
                    }
                    *side = Some(above);
                }
            }
        }
    }

    /// Lanes: the gap the unit at `index` crossed the wall through, if it crossed this tick.
    fn observe_lane(&mut self, index: usize, previous: Point) {
        let KnownAnswer::Lanes { wall, gaps, .. } = self.answer else {
            return;
        };
        let unit = &self.units[index];
        let line = f64::from(wall) + 0.5;
        if !matches!(unit.role, Role::GroupMove { .. }) || previous.0 >= line || unit.at.0 < line {
            return;
        }
        let y = unit.at.1;
        for (lane, &(low, high)) in gaps.iter().enumerate() {
            if f64::from(low) - 0.5 <= y && y <= f64::from(high) + 0.5 {
                self.row.lanes.0[lane] += 1;
                return;
            }
        }
    }

    /// Surround-moving: first contact and the fill that follows it.
    fn observe_fill(&mut self) {
        let (KnownAnswer::SurroundMoving { .. }, Some((target, band))) = (self.answer, self.target)
        else {
            return;
        };
        let centre = self.units[target].at;
        let in_band = self
            .units
            .iter()
            .filter(|u| u.role == Role::Attack && u.before.is_some())
            .filter(|u| distance(u.at, centre) <= band + 1e-9)
            .count();
        let in_band = u32::try_from(in_band).unwrap_or(u32::MAX);
        if self.row.first_contact.is_none() && in_band > 0 {
            self.row.first_contact = Some(self.observed);
        }
        let Some(first) = self.row.first_contact else {
            return;
        };
        let since = self.observed - first;
        if since > FILL_TICKS {
            return;
        }
        self.row.fill_max = self.row.fill_max.max(in_band);
        if self.row.fill_time.is_none() && in_band >= self.wanted {
            self.row.fill_time = Some(since);
        }
    }

    /// Stutter: opens the cue whose bundle the step just observed carried, closing the one before.
    fn open_cue_if_due(&mut self) {
        let KnownAnswer::Stutter { .. } = self.answer else {
            return;
        };
        let due = self
            .script
            .iter()
            .position(|cue| cue.tick + 1 == self.observed);
        let Some(index) = due else {
            return;
        };
        self.close_cue(true);
        let cue = self.script[index];
        let ends = self.script.get(index + 1).map(|next| next.tick + 1);
        let states = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.role == Role::Attack && u.before.is_some())
            .map(|(i, u)| {
                (
                    i,
                    CueState {
                        start: u.at,
                        ..CueState::default()
                    },
                )
            })
            .collect();
        if matches!(cue.order, CueOrder::MoveBy { .. }) {
            self.row.stutter_windows += 1;
        }
        self.open = Some(OpenCue { cue, ends, states });
    }

    /// Stutter: the open cue's progress at this observation; `shots` are the units that
    /// released.
    fn observe_cue(&mut self, shots: &[usize]) {
        let (KnownAnswer::Stutter { period }, Some(open)) = (self.answer, self.open.as_mut())
        else {
            return;
        };
        let t = open.cue.tick;
        let band = self.target.map_or(0.0, |(_, band)| band);
        let centre = self.target.map(|(target, _)| self.units[target].at);
        for (index, state) in &mut open.states {
            let unit = &self.units[*index];
            match open.cue.order {
                CueOrder::MoveBy { offset } => {
                    let along = along(unit.step.unwrap_or((0.0, 0.0)), offset);
                    if state.lag.is_none() && along > unit.speed / 8.0 {
                        state.lag = Some(self.observed - t);
                    }
                }
                CueOrder::Attack => {
                    let inside = centre.is_some_and(|c| distance(unit.at, c) <= band + 1e-9);
                    if state.in_band.is_none() && inside {
                        state.in_band = Some(self.observed);
                    }
                    if let (Some(since), false) = (state.in_band, state.fired) {
                        if shots.contains(index) {
                            state.fired = true;
                            self.row.stutter_fire_late += u32::from(self.observed - since > period);
                        }
                    }
                }
            }
        }
    }

    /// Stutter: judges the open cue; `closed` when the next cue cut it off (rather than the
    /// scene ending).
    fn close_cue(&mut self, closed: bool) {
        let (KnownAnswer::Stutter { period }, Some(open)) = (self.answer, self.open.take()) else {
            return;
        };
        let t = open.cue.tick;
        let last = self.observed - 1;
        let ticks = open
            .ends
            .map_or(last.saturating_sub(t), |ends| ends - 1 - t);
        for (index, state) in &open.states {
            let unit = &self.units[*index];
            match open.cue.order {
                CueOrder::MoveBy { offset } => {
                    let lag = match (state.lag, closed) {
                        (Some(lag), _) => lag,
                        (None, true) => ticks + 1,
                        (None, false) => continue,
                    };
                    self.row.stutter_lag_max = self.row.stutter_lag_max.max(lag);
                    if closed {
                        let moved = (unit.at.0 - state.start.0, unit.at.1 - state.start.1);
                        let ideal = unit.speed * f64::from(ticks.saturating_sub(1));
                        if along(moved, offset) < STUTTER_SHARE * ideal {
                            self.row.stutter_short += 1;
                        }
                    }
                }
                CueOrder::Attack => {
                    if let (Some(since), false) = (state.in_band, state.fired) {
                        self.row.stutter_fire_late += u32::from(last - since >= period);
                    }
                }
            }
        }
    }

    /// The scene's answer columns after its last tick.
    #[must_use]
    pub fn finish(mut self) -> KnownRow {
        self.observed += 1;
        self.close_cue(false);
        if let KnownAnswer::Lanes { radius, .. } = self.answer {
            let centres: Vec<Point> = self
                .units
                .iter()
                .filter(|u| matches!(u.role, Role::GroupMove { .. }) && u.before.is_some())
                .map(|u| u.at)
                .collect();
            let r = lattice(radius);
            let count = f64::from(u32::try_from(centres.len()).unwrap_or(u32::MAX));
            let packed = count * 2.0 * 3.0_f64.sqrt() * r * r;
            if packed > 0.0 {
                self.row.hull_ratio = disc_hull_area(&centres, r) / packed;
            }
        }
        self.row
    }
}

/// The distance between two points.
fn distance(a: Point, b: Point) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// The length of `step` along the direction of the lattice offset `offset`.
fn along(step: Point, offset: (i64, i64)) -> f64 {
    let direction = (lattice(offset.0), lattice(offset.1));
    let length = direction.0.hypot(direction.1);
    if length == 0.0 {
        return 0.0;
    }
    (step.0 * direction.0 + step.1 * direction.1) / length
}

/// The cross product of `b − a` and `c − a`.
fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

/// Whether the segments `p` and `q` cross properly (each one's ends strictly on both sides of
/// the other's line).
#[must_use]
pub fn segments_cross(p: (Point, Point), q: (Point, Point)) -> bool {
    let (d1, d2) = (cross(q.0, q.1, p.0), cross(q.0, q.1, p.1));
    let (d3, d4) = (cross(p.0, p.1, q.0), cross(p.0, p.1, q.1));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// The area of the convex hull of discs of radius `r` at `centres`: the hull of the centres
/// grown by `r` (area + perimeter · r + π r²).
#[must_use]
pub fn disc_hull_area(centres: &[Point], r: f64) -> f64 {
    let hull = convex_hull(centres);
    let mut area = 0.0;
    let mut perimeter = 0.0;
    for (index, &a) in hull.iter().enumerate() {
        let b = hull[(index + 1) % hull.len()];
        area += a.0 * b.1 - a.1 * b.0;
        perimeter += distance(a, b);
    }
    if hull.is_empty() {
        return 0.0;
    }
    (area / 2.0).abs() + perimeter * r + std::f64::consts::PI * r * r
}

/// The convex hull of `points`, counter-clockwise (Andrew's monotone chain).
fn convex_hull(points: &[Point]) -> Vec<Point> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    sorted.dedup();
    if sorted.len() < 3 {
        return sorted;
    }
    let mut hull: Vec<Point> = Vec::with_capacity(2 * sorted.len());
    for pass in [sorted.clone(), sorted.iter().rev().copied().collect()] {
        let floor = hull.len();
        for p in pass {
            while hull.len() >= floor + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_cross_only_properly() {
        let vertical = ((0.0, -1.0), (0.0, 1.0));
        assert!(segments_cross(((-1.0, 0.0), (1.0, 0.0)), vertical));
        assert!(!segments_cross(((-1.0, 2.0), (1.0, 2.0)), vertical));
        assert!(!segments_cross(((-2.0, 0.0), (-1.0, 0.0)), vertical));
    }

    #[test]
    fn the_hull_of_one_disc_is_the_disc_and_of_a_square_of_discs_its_rounded_square() {
        let pi = std::f64::consts::PI;
        assert!((disc_hull_area(&[(3.0, 4.0)], 1.0) - pi).abs() < 1e-12);
        let square = [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0), (1.0, 1.0)];
        // 2 × 2 square, perimeter 8 · r, plus π r².
        let expected = 4.0 + 8.0 * 0.5 + pi * 0.25;
        assert!((disc_hull_area(&square, 0.5) - expected).abs() < 1e-12);
    }

    #[test]
    fn along_reads_the_component_in_the_offsets_direction() {
        assert!((along((0.3, 0.4), (64, 0)) - 0.3).abs() < 1e-12);
        assert!((along((0.3, 0.4), (0, -128)) + 0.4).abs() < 1e-12);
        assert!(along((0.3, 0.4), (0, 0)).abs() < 1e-12);
    }
}
