//! Per-battle measures derived from a [`BattleTrace`]'s per-tick records.
//!
//! # Responsibility
//! Reduces the records to the numbers the battle 33 investigation reports (plan D-2): survivors
//! and remaining health, the first tick each side had a unit in contact, how many units of each
//! side were in contact per tick (mean, maximum, the fraction of the living, and the histogram),
//! unit-seconds per [`Activity`], the totals of every counter, a one-second table, and each
//! side's targeting measures ([`super::focus`]).
//!
//! # How it fits
//! [`BattleTrace::summary`] builds a [`BattleSummary`]; [`super::report`] formats it. Nothing
//! here reads the world, so every measure is a pure function of the per-tick and per-unit
//! records and is tested on hand-made records.
//!
//! # Invariants
//! - The contact window starts at the first tick on which any unit of either side is in
//!   contact, and ends with the last record. A battle without contact has an empty window and
//!   zero contact measures.
//! - Tick 0 (the state before the first step) holds no events; totals start at tick 1.
//! - Seconds are 25 ticks (`sim_types::TICKS_PER_SECOND`, Faster game seconds): second `s`
//!   covers ticks `25 (s - 1) + 1` to `25 s`; the last second may be shorter.

use super::focus::{focus_stats, FocusStats};
use super::{Activity, BattleTrace, Team, TeamTick, TickRecord, UnitRow};
use sim_types::TICKS_PER_SECOND;

/// One side's contact measures over the contact window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ContactStats {
    /// Ticks in the window.
    pub window_ticks: u32,
    /// Mean number of units in contact per tick.
    pub mean_engaged: f64,
    /// Largest number of units in contact on one tick.
    pub max_engaged: u32,
    /// Mean of (units in contact ÷ units alive) over the window's ticks with a unit alive.
    pub mean_fraction: f64,
    /// `histogram[k]` is the number of window ticks with exactly `k` units in contact.
    pub histogram: Vec<u32>,
}

/// One side's counters summed over the battle.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TeamTotals {
    /// Life and shields lost, in hit points.
    pub damage_taken: f64,
    /// Life and shields regained, in hit points.
    pub health_regained: f64,
    /// Swings started.
    pub swings_started: u32,
    /// Shots or swings released.
    pub shots_released: u32,
    /// Swings cancelled because the target died or vanished.
    pub cancelled_target_gone: u32,
    /// Swings cancelled at release because the target was out of band.
    pub cancelled_out_of_band: u32,
    /// Swings cancelled for another reason.
    pub cancelled_other: u32,
    /// Instant shots wasted on a target already killed in the same tick.
    pub wasted_shots: u32,
    /// Target switches between two living enemies.
    pub target_switches: u32,
    /// Unit-ticks in contact while moving: pushed in contact.
    pub pushed_engaged: u32,
    /// Distance moved while in contact, in world units.
    pub pushed_distance: f64,
}

impl TeamTotals {
    /// Every cancelled swing.
    #[must_use]
    pub const fn cancelled(&self) -> u32 {
        self.cancelled_target_gone + self.cancelled_out_of_band + self.cancelled_other
    }

    fn add(&mut self, tick: &TeamTick) {
        self.damage_taken += tick.damage_taken;
        self.health_regained += tick.health_regained;
        self.swings_started += tick.swings_started;
        self.shots_released += tick.shots_released;
        self.cancelled_target_gone += tick.cancelled_target_gone;
        self.cancelled_out_of_band += tick.cancelled_out_of_band;
        self.cancelled_other += tick.cancelled_other;
        self.wasted_shots += tick.wasted_shots;
        self.target_switches += tick.target_switches;
        self.pushed_engaged += tick.pushed_engaged;
        self.pushed_distance += tick.pushed_distance;
    }
}

/// One side during one second.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SecondSide {
    /// Units alive at the end of the second.
    pub alive: u32,
    /// Mean units per [`Activity`] over the second's ticks, indexed by [`Activity::index`].
    pub activity: [f64; 6],
    /// The side's counters summed over the second.
    pub totals: TeamTotals,
}

/// Both sides during one second.
#[derive(Debug, Clone, PartialEq)]
pub struct SecondRow {
    /// The second, from 1.
    pub second: u32,
    /// Per side, indexed by `Team::index`.
    pub teams: [SecondSide; 2],
}

/// The measures of one traced battle.
#[derive(Debug, Clone, PartialEq)]
pub struct BattleSummary {
    /// The last recorded tick: the tick the battle ended on.
    pub end_tick: u32,
    /// Units at the start, per side.
    pub initial_units: [u32; 2],
    /// Units alive at the end, per side.
    pub survivors: [u32; 2],
    /// Life plus shields at the start, per side, in hit points.
    pub initial_health: [f64; 2],
    /// Life plus shields of the survivors at the end, per side, in hit points.
    pub remaining_health: [f64; 2],
    /// First tick on which the side had a unit in contact.
    pub first_engaged_tick: [Option<u32>; 2],
    /// Contact measures per side over the common contact window.
    pub contact: [ContactStats; 2],
    /// Unit-seconds per side per [`Activity`].
    pub unit_seconds: [[f64; 6]; 2],
    /// Counters per side summed over the battle.
    pub totals: [TeamTotals; 2],
    /// One row per second.
    pub seconds: Vec<SecondRow>,
    /// Targeting measures per side ([`super::focus`]).
    pub focus: [FocusStats; 2],
}

impl BattleSummary {
    /// Derives the summary of `trace`.
    #[must_use]
    pub fn from_trace(trace: &BattleTrace) -> Self {
        Self::from_records(trace.initial_units(), trace.ticks(), trace.unit_rows())
    }

    /// Derives the summary of the per-tick `records` and per-unit `units` samples of a battle
    /// that started with `initial_units` per side.
    #[must_use]
    pub fn from_records(
        initial_units: [u32; 2],
        records: &[TickRecord],
        units: &[UnitRow],
    ) -> Self {
        let side = |i: usize| {
            let first = records.first().map(|r| r.teams[i]);
            let last = records.last().map(|r| r.teams[i]);
            (first.unwrap_or_default(), last.unwrap_or_default())
        };
        let (blue_first, blue_last) = side(0);
        let (red_first, red_last) = side(1);
        let first_engaged_tick = [0, 1].map(|i| {
            records
                .iter()
                .find(|r| r.teams[i].engaged() > 0)
                .map(|r| r.tick)
        });
        let window_start = first_engaged_tick.iter().flatten().min().copied();
        let window: &[TickRecord] = window_start.map_or(&[], |start| {
            let from = records.partition_point(|r| r.tick < start);
            &records[from..]
        });
        let events = records.get(1..).unwrap_or_default();
        Self {
            end_tick: records.last().map_or(0, |r| r.tick),
            initial_units,
            survivors: [blue_last.alive, red_last.alive],
            initial_health: [blue_first.health, red_first.health],
            remaining_health: [blue_last.health, red_last.health],
            first_engaged_tick,
            contact: [0, 1].map(|i| contact_stats(window, i)),
            unit_seconds: [0, 1].map(|i| unit_seconds(events, i)),
            totals: [0, 1].map(|i| totals(events, i)),
            seconds: seconds(events),
            focus: Team::BOTH.map(|team| focus_stats(units, team)),
        }
    }
}

/// Ticks per second as `f64`.
fn ticks_per_second() -> f64 {
    f64::from(TICKS_PER_SECOND)
}

fn contact_stats(window: &[TickRecord], side: usize) -> ContactStats {
    let mut stats = ContactStats::default();
    let mut engaged_sum = 0_u64;
    let (mut fraction_sum, mut fraction_ticks) = (0.0, 0_u32);
    for record in window {
        let tick = &record.teams[side];
        let engaged = tick.engaged();
        stats.window_ticks += 1;
        engaged_sum += u64::from(engaged);
        stats.max_engaged = stats.max_engaged.max(engaged);
        let slot = usize::try_from(engaged).unwrap_or(usize::MAX);
        if stats.histogram.len() <= slot {
            stats.histogram.resize(slot + 1, 0);
        }
        stats.histogram[slot] += 1;
        if tick.alive > 0 {
            fraction_sum += f64::from(engaged) / f64::from(tick.alive);
            fraction_ticks += 1;
        }
    }
    if stats.window_ticks > 0 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a battle's engaged unit-ticks are far below 2^52"
        )]
        let sum = engaged_sum as f64;
        stats.mean_engaged = sum / f64::from(stats.window_ticks);
    }
    if fraction_ticks > 0 {
        stats.mean_fraction = fraction_sum / f64::from(fraction_ticks);
    }
    stats
}

fn unit_seconds(events: &[TickRecord], side: usize) -> [f64; 6] {
    let mut sums = [0.0; 6];
    for record in events {
        for activity in Activity::ALL {
            sums[activity.index()] += f64::from(record.teams[side].activity[activity.index()]);
        }
    }
    sums.map(|ticks| ticks / ticks_per_second())
}

fn totals(events: &[TickRecord], side: usize) -> TeamTotals {
    let mut totals = TeamTotals::default();
    for record in events {
        totals.add(&record.teams[side]);
    }
    totals
}

fn seconds(events: &[TickRecord]) -> Vec<SecondRow> {
    events
        .chunks(usize::from(TICKS_PER_SECOND))
        .zip(1_u32..)
        .map(|(chunk, second)| SecondRow {
            second,
            teams: [0, 1].map(|side| second_side(chunk, side)),
        })
        .collect()
}

fn second_side(chunk: &[TickRecord], side: usize) -> SecondSide {
    let mut result = SecondSide {
        alive: chunk.last().map_or(0, |r| r.teams[side].alive),
        ..SecondSide::default()
    };
    for record in chunk {
        let tick = &record.teams[side];
        result.totals.add(tick);
        for (mean, count) in result.activity.iter_mut().zip(tick.activity) {
            *mean += f64::from(count);
        }
    }
    let ticks = f64::from(u32::try_from(chunk.len()).unwrap_or(u32::MAX).max(1));
    for mean in &mut result.activity {
        *mean /= ticks;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record with `alive` units per side, `engaged` of them in contact and the rest idle.
    fn record(tick: u32, alive: [u32; 2], engaged: [u32; 2]) -> TickRecord {
        let teams = [0, 1].map(|i| TeamTick {
            alive: alive[i],
            health: f64::from(alive[i]) * 10.0,
            activity: [engaged[i], 0, 0, 0, 0, alive[i] - engaged[i]],
            ..TeamTick::default()
        });
        TickRecord { tick, teams }
    }

    #[test]
    fn contact_measures_cover_the_window_from_the_first_contact_of_either_side() {
        let records = [
            record(0, [4, 2], [0, 0]),
            record(1, [4, 2], [0, 0]),
            record(2, [4, 2], [0, 1]),
            record(3, [4, 2], [2, 2]),
            record(4, [4, 1], [1, 1]),
        ];
        let summary = BattleSummary::from_records([4, 2], &records, &[]);
        assert_eq!(summary.first_engaged_tick, [Some(3), Some(2)]);
        let red = &summary.contact[1];
        assert_eq!(red.window_ticks, 3);
        assert!((red.mean_engaged - 4.0 / 3.0).abs() < 1e-12);
        assert_eq!(red.max_engaged, 2);
        assert_eq!(red.histogram, vec![0, 2, 1]);
        // Fractions 1/2, 2/2, 1/1.
        assert!((red.mean_fraction - 2.5 / 3.0).abs() < 1e-12);
        let blue = &summary.contact[0];
        assert_eq!(blue.histogram, vec![1, 1, 1]);
        assert_eq!(summary.survivors, [4, 1]);
        assert_eq!(summary.end_tick, 4);
        assert!((summary.initial_health[1] - 20.0).abs() < 1e-12);
        assert!((summary.remaining_health[1] - 10.0).abs() < 1e-12);
    }

    #[test]
    fn a_battle_without_contact_has_an_empty_window() {
        let records = [record(0, [1, 1], [0, 0]), record(1, [1, 1], [0, 0])];
        let summary = BattleSummary::from_records([1, 1], &records, &[]);
        assert_eq!(summary.first_engaged_tick, [None, None]);
        assert_eq!(summary.contact[1], ContactStats::default());
    }

    #[test]
    fn unit_seconds_and_totals_skip_tick_zero() {
        let mut records = vec![record(0, [1, 1], [1, 1])];
        for tick in 1..=50 {
            let mut next = record(tick, [1, 1], [1, 0]);
            next.teams[1].shots_released = 1;
            next.teams[1].cancelled_out_of_band = u32::from(tick == 7);
            records.push(next);
        }
        records[0].teams[1].shots_released = 99;
        let summary = BattleSummary::from_records([1, 1], &records, &[]);
        assert!((summary.unit_seconds[0][Activity::Engaged.index()] - 2.0).abs() < 1e-12);
        assert!((summary.unit_seconds[1][Activity::Idle.index()] - 2.0).abs() < 1e-12);
        assert_eq!(summary.totals[1].shots_released, 50);
        assert_eq!(summary.totals[1].cancelled(), 1);
    }

    #[test]
    fn seconds_are_twenty_five_ticks_and_the_last_may_be_short() {
        let mut records = vec![record(0, [3, 3], [0, 0])];
        for tick in 1..=30 {
            let engaged = if tick <= 25 { [0, 2] } else { [3, 1] };
            let mut next = record(tick, [3, 3], engaged);
            next.teams[0].damage_taken = 1.0;
            records.push(next);
        }
        let summary = BattleSummary::from_records([3, 3], &records, &[]);
        assert_eq!(summary.seconds.len(), 2);
        let first = &summary.seconds[0];
        assert_eq!(first.second, 1);
        assert!((first.teams[1].activity[0] - 2.0).abs() < 1e-12);
        assert!((first.teams[0].totals.damage_taken - 25.0).abs() < 1e-12);
        let last = &summary.seconds[1];
        assert!((last.teams[0].activity[0] - 3.0).abs() < 1e-12);
        assert!((last.teams[0].totals.damage_taken - 5.0).abs() < 1e-12);
        assert_eq!(last.teams[0].alive, 3);
    }
}
