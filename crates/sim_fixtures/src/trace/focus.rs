//! Targeting measures: when each unit first takes a target, how many enemies it attacks, and how
//! many units of a side share one target.
//!
//! # Responsibility
//! Reduces a trace's per-unit samples ([`UnitRow`]) to one side's [`FocusStats`]: the mean tick
//! of each unit's first target, the mean number of distinct enemies each unit targeted, and per
//! tick the *focus*, the number of the side's units whose target is the enemy that most of them
//! target. These are the measures plan `2026-11-04-combat-sc2-alignment` WP14 step 1 asks for to
//! explain how scan range changes which enemies a side concentrates its fire on.
//!
//! # How it fits
//! [`super::summary::BattleSummary::from_trace`] calls [`focus_stats`] for each side, and
//! [`super::report`] prints the result per seed and over seeds. Target switches are counted per
//! tick by the trace itself ([`super::TeamTick::target_switches`]); this module adds the
//! per-unit and per-target view.
//!
//! # Invariants
//! - A unit's target is the trace's [`super::UnitSample::target`]: the living enemy it is
//!   attacking, so "units firing at an enemy" means units whose target that enemy is.
//! - Only ticks after tick 0 count: tick 0 is the state before the first step.
//! - Focus is averaged over the ticks on which at least one unit of the side holds a target; a
//!   side that never targets has zero focus and no first-target tick.
//! - The result is a pure function of the rows, independent of their order within a tick.

use super::{Team, UnitRow};
use sim_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};

/// One side's targeting measures over a battle.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FocusStats {
    /// Units of the side that appear in the trace.
    pub units: u32,
    /// Units of the side that held a target on some tick after tick 0.
    pub units_with_target: u32,
    /// Mean, over the units that held a target, of the first tick on which they held one;
    /// `None` when no unit did.
    pub mean_first_target_tick: Option<f64>,
    /// Mean number of distinct enemies each unit of the side targeted (units that never held a
    /// target count as 0).
    pub mean_distinct_targets: f64,
    /// Ticks after tick 0 on which at least one unit of the side held a target.
    pub focus_ticks: u32,
    /// Mean over [`Self::focus_ticks`] of the number of the side's units targeting the enemy
    /// that most of them target.
    pub mean_focus: f64,
    /// Largest number of the side's units targeting one enemy on one tick.
    pub max_focus: u32,
    /// Mean over [`Self::focus_ticks`] of the focus divided by the side's units holding a target.
    pub mean_focus_share: f64,
}

/// The targeting measures of `team` from the per-unit `rows` of one battle.
#[must_use]
pub fn focus_stats(rows: &[UnitRow], team: Team) -> FocusStats {
    // Per unit: the first tick with a target and the enemies it targeted.
    let mut per_unit: BTreeMap<EntityId, (Option<u32>, BTreeSet<EntityId>)> = BTreeMap::new();
    // Per tick: how many of the side's units target each enemy.
    let mut per_tick: BTreeMap<u32, BTreeMap<EntityId, u32>> = BTreeMap::new();
    for row in rows.iter().filter(|row| row.sample.team == team) {
        let unit = per_unit.entry(row.sample.id).or_default();
        let Some(target) = row.sample.target.filter(|_| row.tick > 0) else {
            continue;
        };
        unit.0 = Some(unit.0.map_or(row.tick, |first| first.min(row.tick)));
        unit.1.insert(target);
        *per_tick
            .entry(row.tick)
            .or_default()
            .entry(target)
            .or_default() += 1;
    }

    let mut stats = FocusStats {
        units: count_u32(per_unit.len()),
        ..FocusStats::default()
    };
    let firsts: Vec<f64> = per_unit
        .values()
        .filter_map(|(first, _)| first.map(f64::from))
        .collect();
    stats.units_with_target = count_u32(firsts.len());
    if !firsts.is_empty() {
        stats.mean_first_target_tick =
            Some(firsts.iter().sum::<f64>() / f64::from(stats.units_with_target));
    }
    if stats.units > 0 {
        let distinct: u32 = per_unit
            .values()
            .map(|(_, targets)| count_u32(targets.len()))
            .sum();
        stats.mean_distinct_targets = f64::from(distinct) / f64::from(stats.units);
    }

    let (mut focus_sum, mut share_sum) = (0.0, 0.0);
    for counts in per_tick.values() {
        let focus = counts.values().copied().max().unwrap_or(0);
        let holders: u32 = counts.values().sum();
        stats.focus_ticks += 1;
        stats.max_focus = stats.max_focus.max(focus);
        focus_sum += f64::from(focus);
        share_sum += f64::from(focus) / f64::from(holders);
    }
    if stats.focus_ticks > 0 {
        stats.mean_focus = focus_sum / f64::from(stats.focus_ticks);
        stats.mean_focus_share = share_sum / f64::from(stats.focus_ticks);
    }
    stats
}

/// A count as `u32`, saturating; a battle's units and targets are far below `u32::MAX`.
fn count_u32(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{Activity, UnitSample};
    use sim_core::WeaponPhase;
    use sim_types::{PlayerId, SimScalar, SimVec2, UnitTypeId};

    /// A sample of unit `id` of `team` at `tick` attacking `target`.
    fn row(tick: u32, id: u32, team: Team, target: Option<u32>) -> UnitRow {
        UnitRow {
            tick,
            sample: UnitSample {
                id: EntityId(id),
                team,
                unit_type: UnitTypeId(1),
                owner: PlayerId(u8::try_from(team.index()).unwrap_or(0)),
                position: SimVec2::ZERO,
                velocity: SimVec2::ZERO,
                hp: SimScalar::from_num(10),
                shields: SimScalar::ZERO,
                phase: WeaponPhase::Ready,
                cooldown: 0,
                target: target.map(EntityId),
                activity: Activity::Idle,
            },
        }
    }

    #[test]
    fn focus_counts_the_units_on_the_most_targeted_enemy() {
        let blue = Team::Blue;
        let rows = [
            // Tick 0 is ignored even though a unit holds a target.
            row(0, 1, blue, Some(10)),
            row(0, 2, blue, None),
            row(0, 3, blue, None),
            row(0, 10, Team::Red, Some(1)),
            // Tick 1: two on enemy 10, one on 11 -> focus 2 of 3.
            row(1, 1, blue, Some(10)),
            row(1, 2, blue, Some(10)),
            row(1, 3, blue, Some(11)),
            row(1, 10, Team::Red, Some(1)),
            // Tick 2: nobody holds a target -> not a focus tick.
            row(2, 1, blue, None),
            row(2, 2, blue, None),
            row(2, 3, blue, None),
            // Tick 3: one unit on enemy 12 -> focus 1 of 1.
            row(3, 1, blue, Some(12)),
            row(3, 2, blue, None),
        ];
        let stats = focus_stats(&rows, blue);
        assert_eq!(stats.units, 3);
        assert_eq!(stats.units_with_target, 3);
        // First targets on ticks 1, 1 and 1.
        assert_eq!(stats.mean_first_target_tick, Some(1.0));
        // Unit 1: 10 and 12; unit 2: 10; unit 3: 11.
        assert!((stats.mean_distinct_targets - 4.0 / 3.0).abs() < 1e-12);
        assert_eq!(stats.focus_ticks, 2);
        assert!((stats.mean_focus - 1.5).abs() < 1e-12);
        assert_eq!(stats.max_focus, 2);
        assert!((stats.mean_focus_share - f64::midpoint(2.0 / 3.0, 1.0)).abs() < 1e-12);
    }

    #[test]
    fn first_target_ticks_are_averaged_over_the_units_that_had_one() {
        let red = Team::Red;
        let rows = [
            row(1, 5, red, None),
            row(1, 6, red, None),
            row(1, 7, red, None),
            row(2, 5, red, Some(1)),
            row(2, 6, red, None),
            row(2, 7, red, None),
            row(6, 5, red, Some(2)),
            row(6, 6, red, Some(2)),
        ];
        let stats = focus_stats(&rows, red);
        assert_eq!(stats.units, 3);
        assert_eq!(stats.units_with_target, 2);
        assert_eq!(stats.mean_first_target_tick, Some(4.0));
        // Unit 5: 1 and 2; unit 6: 2; unit 7: none.
        assert!((stats.mean_distinct_targets - 1.0).abs() < 1e-12);
        assert_eq!(stats.max_focus, 2);
    }

    #[test]
    fn a_side_that_never_targets_has_no_focus() {
        let rows = [row(1, 1, Team::Blue, None), row(1, 9, Team::Red, Some(1))];
        let stats = focus_stats(&rows, Team::Blue);
        assert_eq!(stats.units, 1);
        assert_eq!(stats.units_with_target, 0);
        assert_eq!(stats.mean_first_target_tick, None);
        assert_eq!(stats.focus_ticks, 0);
        assert!(stats.mean_focus.abs() < 1e-12);
    }
}
