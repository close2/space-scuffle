//! CSV and Markdown output of battle traces, for one run and for many seeds of one battle.
//!
//! # Responsibility
//! Formats a [`BattleTrace`] as two CSV tables (per tick per side, and per tick per unit), a
//! [`BattleSummary`] as a Markdown page, and the summaries of many seeds of one battle as a
//! per-seed CSV and a Markdown page with the outcome distribution, the contact distribution and
//! the mean of every measure.
//!
//! # How it fits
//! [`super::cli`] writes these strings to files. The text is the evidence the investigation
//! quotes, so every column is named after the [`TeamTick`] field it holds.
//!
//! # Invariants
//! - Output is a pure function of its input: the same trace gives byte-identical files.
//! - Floating-point values are printed with fixed precision, so files compare cleanly.

use super::summary::{BattleSummary, SecondRow};
use super::{Activity, BattleTrace, Team, TeamTick};
use sim_core::WeaponPhase;
use std::collections::BTreeMap;

/// Appends formatted text to `out`.
fn push(out: &mut String, args: std::fmt::Arguments<'_>) {
    out.push_str(&std::fmt::format(args));
}

/// The per-side column names of [`ticks_csv`], in order.
const SIDE_COLUMNS: [&str; 19] = [
    "alive",
    "health",
    "damage_taken",
    "health_regained",
    "swings_started",
    "shots_released",
    "cancelled_target_gone",
    "cancelled_out_of_band",
    "cancelled_other",
    "wasted_shots",
    "target_switches",
    "engaged",
    "blocked",
    "flanking",
    "approaching",
    "walking",
    "idle",
    "pushed_engaged",
    "pushed_distance",
];

/// One CSV row per tick: the tick, then [`SIDE_COLUMNS`] for blue, then for red.
#[must_use]
pub fn ticks_csv(trace: &BattleTrace) -> String {
    let mut header = vec!["tick".to_owned()];
    for team in Team::BOTH {
        header.extend(SIDE_COLUMNS.iter().map(|c| format!("{}_{c}", team.label())));
    }
    let mut out = header.join(",");
    out.push('\n');
    for record in trace.ticks() {
        let mut cells = vec![record.tick.to_string()];
        for side in &record.teams {
            cells.extend(side_cells(side));
        }
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

fn side_cells(side: &TeamTick) -> Vec<String> {
    let mut cells = vec![
        side.alive.to_string(),
        format!("{:.3}", side.health),
        format!("{:.3}", side.damage_taken),
        format!("{:.3}", side.health_regained),
        side.swings_started.to_string(),
        side.shots_released.to_string(),
        side.cancelled_target_gone.to_string(),
        side.cancelled_out_of_band.to_string(),
        side.cancelled_other.to_string(),
        side.wasted_shots.to_string(),
        side.target_switches.to_string(),
    ];
    cells.extend(side.activity.iter().map(ToString::to_string));
    cells.push(side.pushed_engaged.to_string());
    cells.push(format!("{:.4}", side.pushed_distance));
    cells
}

/// One CSV row per living unit per tick: position, velocity, life, shields, weapon phase,
/// target and activity.
#[must_use]
pub fn units_csv(trace: &BattleTrace) -> String {
    let mut out = String::from(
        "tick,id,team,x,y,vx,vy,hp,shields,phase,phase_target,phase_remaining,target,activity\n",
    );
    for row in trace.unit_rows() {
        let s = &row.sample;
        let (phase, phase_target, remaining) = phase_cells(&s.phase);
        let cells = [
            row.tick.to_string(),
            s.id.0.to_string(),
            s.team.label().to_owned(),
            format!("{:.4}", s.position.x.to_f32()),
            format!("{:.4}", s.position.y.to_f32()),
            format!("{:.4}", s.velocity.x.to_f32()),
            format!("{:.4}", s.velocity.y.to_f32()),
            format!("{:.3}", s.hp.to_f32()),
            format!("{:.3}", s.shields.to_f32()),
            phase.to_owned(),
            phase_target,
            remaining,
            s.target.map_or_else(String::new, |t| t.0.to_string()),
            s.activity.label().to_owned(),
        ];
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

/// The phase name, its bound target and its remaining ticks, as CSV cells.
fn phase_cells(phase: &WeaponPhase) -> (&'static str, String, String) {
    match *phase {
        WeaponPhase::Ready => ("ready", String::new(), String::new()),
        WeaponPhase::RotatingToTarget { target_id } => {
            ("rotating", target_id.0.to_string(), String::new())
        }
        WeaponPhase::Windup {
            target_id,
            remaining_ticks,
        } => (
            "windup",
            target_id.0.to_string(),
            remaining_ticks.to_string(),
        ),
        WeaponPhase::Backswing {
            target_id,
            remaining_ticks,
        } => (
            "backswing",
            target_id.0.to_string(),
            remaining_ticks.to_string(),
        ),
        WeaponPhase::Cooldown => ("cooldown", String::new(), String::new()),
    }
}

/// Formats an optional tick.
fn tick_cell(tick: Option<u32>) -> String {
    tick.map_or_else(|| "none".to_owned(), |t| t.to_string())
}

/// A Markdown page for one run: outcome, contact, activity, totals and the one-second table.
#[must_use]
pub fn run_markdown(title: &str, summary: &BattleSummary) -> String {
    let mut out = format!("# {title}\n\n");
    push(
        &mut out,
        format_args!(
        "Ended on tick {} ({:.2} s). Survivors: blue {} of {}, red {} of {}. Health left: blue \
         {:.1} of {:.1}, red {:.1} of {:.1}.\n\n",
        summary.end_tick,
        f64::from(summary.end_tick) / 25.0,
        summary.survivors[0],
        summary.initial_units[0],
        summary.survivors[1],
        summary.initial_units[1],
        summary.remaining_health[0],
        summary.initial_health[0],
        summary.remaining_health[1],
        summary.initial_health[1],
    ),
    );
    out.push_str("## Per side\n\n| Measure | Blue | Red |\n|---|---|---|\n");
    let both = |f: &dyn Fn(usize) -> String| format!("| {} | {} |", f(0), f(1));
    let rows: Vec<(&str, String)> = vec![
        (
            "First tick in contact",
            both(&|i| tick_cell(summary.first_engaged_tick[i])),
        ),
        (
            "Contact window (ticks)",
            both(&|i| summary.contact[i].window_ticks.to_string()),
        ),
        (
            "Units in contact, mean",
            both(&|i| format!("{:.2}", summary.contact[i].mean_engaged)),
        ),
        (
            "Units in contact, max",
            both(&|i| summary.contact[i].max_engaged.to_string()),
        ),
        (
            "Fraction of the living in contact, mean",
            both(&|i| format!("{:.3}", summary.contact[i].mean_fraction)),
        ),
        (
            "Units in contact: ticks with k = 0, 1, 2, ...",
            both(&|i| join_counts(&summary.contact[i].histogram)),
        ),
    ];
    for (name, cells) in rows {
        push(&mut out, format_args!("| {name} {cells}\n"));
    }
    for activity in Activity::ALL {
        push(
            &mut out,
            format_args!(
                "| Unit-seconds {} | {:.2} | {:.2} |\n",
                activity.label(),
                summary.unit_seconds[0][activity.index()],
                summary.unit_seconds[1][activity.index()],
            ),
        );
    }
    for (name, value) in totals_rows() {
        push(
            &mut out,
            format_args!(
                "| {name} | {} | {} |\n",
                value(summary, 0),
                value(summary, 1)
            ),
        );
    }
    out.push_str("\n## Per second\n\n");
    out.push_str(&seconds_table(&summary.seconds));
    out
}

/// A total's label and how to print it for one side.
type TotalRow = (&'static str, fn(&BattleSummary, usize) -> String);

/// The totals printed per side, in order.
fn totals_rows() -> [TotalRow; 11] {
    [
        ("Health lost", |s, i| {
            format!("{:.1}", s.totals[i].damage_taken)
        }),
        ("Health regained", |s, i| {
            format!("{:.1}", s.totals[i].health_regained)
        }),
        ("Swings started", |s, i| {
            s.totals[i].swings_started.to_string()
        }),
        ("Shots released", |s, i| {
            s.totals[i].shots_released.to_string()
        }),
        ("Swings cancelled, target gone", |s, i| {
            s.totals[i].cancelled_target_gone.to_string()
        }),
        ("Swings cancelled, out of band", |s, i| {
            s.totals[i].cancelled_out_of_band.to_string()
        }),
        ("Swings cancelled, other", |s, i| {
            s.totals[i].cancelled_other.to_string()
        }),
        ("Shots wasted (target already dead)", |s, i| {
            s.totals[i].wasted_shots.to_string()
        }),
        ("Target switches", |s, i| {
            s.totals[i].target_switches.to_string()
        }),
        ("Unit-ticks pushed while in contact", |s, i| {
            s.totals[i].pushed_engaged.to_string()
        }),
        ("Distance moved while in contact", |s, i| {
            format!("{:.2}", s.totals[i].pushed_distance)
        }),
    ]
}

fn join_counts(counts: &[u32]) -> String {
    counts
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The one-second table of one run.
fn seconds_table(seconds: &[SecondRow]) -> String {
    let mut out = String::from(
        "| s | Blue alive | Red alive | Blue in contact | Red in contact | Red blocked | Red \
         flanking | Red approaching | Health lost by red | Health lost by blue | Blue shots | \
         Red swings | Red cancelled | Blue wasted |\n|---|---|---|---|---|---|---|---|---|---|---|\
         ---|---|---|\n",
    );
    for row in seconds {
        let [blue, red] = &row.teams;
        push(
            &mut out,
            format_args!(
            "| {} | {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.1} | {:.1} | {} | {} | {} \
             | {} |\n",
            row.second,
            blue.alive,
            red.alive,
            blue.activity[Activity::Engaged.index()],
            red.activity[Activity::Engaged.index()],
            red.activity[Activity::Blocked.index()],
            red.activity[Activity::Flanking.index()],
            red.activity[Activity::Approaching.index()],
            red.totals.damage_taken,
            blue.totals.damage_taken,
            blue.totals.shots_released,
            red.totals.shots_released,
            red.totals.cancelled(),
            blue.totals.wasted_shots,
        ),
        );
    }
    out
}

/// One seed's run of a battle.
#[derive(Debug, Clone)]
pub struct SeedResult {
    /// World seed.
    pub seed: u64,
    /// Spawn jitter the seed played with, in world units per axis.
    pub jitter: f32,
    /// The run's measures.
    pub summary: BattleSummary,
}

/// The per-seed CSV of many runs of one battle.
#[must_use]
pub fn seeds_csv(results: &[SeedResult]) -> String {
    let mut out = String::from(
        "seed,jitter,end_tick,blue_survivors,red_survivors,blue_health,red_health,\
         blue_first_contact,red_first_contact,blue_mean_engaged,red_mean_engaged,\
         blue_max_engaged,red_max_engaged,blue_mean_fraction,red_mean_fraction,\
         red_blocked_s,red_flanking_s,red_approaching_s,blue_shots,red_swings,\
         blue_cancelled,red_cancelled_gone,red_cancelled_out_of_band,red_cancelled_other,\
         blue_wasted,red_wasted,blue_switches,red_switches,blue_pushed_ticks,red_pushed_ticks\n",
    );
    for r in results {
        let s = &r.summary;
        let (b, red) = (&s.totals[0], &s.totals[1]);
        let cells = [
            r.seed.to_string(),
            format!("{:.2}", r.jitter),
            s.end_tick.to_string(),
            s.survivors[0].to_string(),
            s.survivors[1].to_string(),
            format!("{:.1}", s.remaining_health[0]),
            format!("{:.1}", s.remaining_health[1]),
            tick_cell(s.first_engaged_tick[0]),
            tick_cell(s.first_engaged_tick[1]),
            format!("{:.3}", s.contact[0].mean_engaged),
            format!("{:.3}", s.contact[1].mean_engaged),
            s.contact[0].max_engaged.to_string(),
            s.contact[1].max_engaged.to_string(),
            format!("{:.4}", s.contact[0].mean_fraction),
            format!("{:.4}", s.contact[1].mean_fraction),
            format!("{:.2}", s.unit_seconds[1][Activity::Blocked.index()]),
            format!("{:.2}", s.unit_seconds[1][Activity::Flanking.index()]),
            format!("{:.2}", s.unit_seconds[1][Activity::Approaching.index()]),
            b.shots_released.to_string(),
            red.shots_released.to_string(),
            b.cancelled().to_string(),
            red.cancelled_target_gone.to_string(),
            red.cancelled_out_of_band.to_string(),
            red.cancelled_other.to_string(),
            b.wasted_shots.to_string(),
            red.wasted_shots.to_string(),
            b.target_switches.to_string(),
            red.target_switches.to_string(),
            b.pushed_engaged.to_string(),
            red.pushed_engaged.to_string(),
        ];
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

/// Mean, sample standard deviation, minimum, median and maximum of `values`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spread {
    /// Arithmetic mean.
    pub mean: f64,
    /// Sample standard deviation (n − 1); 0 for fewer than two values.
    pub sd: f64,
    /// Smallest value.
    pub min: f64,
    /// Median (mean of the middle two for an even count).
    pub median: f64,
    /// Largest value.
    pub max: f64,
}

impl Spread {
    /// The spread of `values`; all zero when empty.
    #[must_use]
    pub fn of(values: &[f64]) -> Self {
        if values.is_empty() {
            return Self {
                mean: 0.0,
                sd: 0.0,
                min: 0.0,
                median: 0.0,
                max: 0.0,
            };
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let n = f64::from(u32::try_from(values.len()).unwrap_or(u32::MAX));
        let mean = values.iter().sum::<f64>() / n;
        let sd = if values.len() > 1 {
            (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
        } else {
            0.0
        };
        let mid = sorted.len() / 2;
        let median = if sorted.len().is_multiple_of(2) {
            f64::midpoint(sorted[mid - 1], sorted[mid])
        } else {
            sorted[mid]
        };
        Self {
            mean,
            sd,
            min: sorted[0],
            median,
            max: sorted[sorted.len() - 1],
        }
    }

    fn cells(&self, precision: usize) -> String {
        format!(
            "{:.p$} | {:.p$} | {:.p$} | {:.p$} | {:.p$}",
            self.mean,
            self.sd,
            self.min,
            self.median,
            self.max,
            p = precision
        )
    }
}

/// A Markdown page for many seeds of one battle: outcomes, survivor histograms, the contact
/// distribution, contact over time, and the spread of every measure.
#[must_use]
pub fn aggregate_markdown(title: &str, preamble: &str, results: &[SeedResult]) -> String {
    let mut out = format!("# {title}\n\n{preamble}\n\n");
    out.push_str(&outcome_section(results));
    out.push_str(&measures_section(results));
    out.push_str(&contact_histogram_section(results));
    out.push_str(&contact_over_time_section(results));
    out.push_str(
        "## Per seed\n\n| Seed | End tick | Blue left | Red left | Red first contact | \
                  Red in contact, mean | Red in contact, max | Blue in contact, mean | Red \
                  cancelled | Blue wasted |\n|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for r in results {
        let s = &r.summary;
        push(
            &mut out,
            format_args!(
                "| {} | {} | {} | {} | {} | {:.2} | {} | {:.2} | {} | {} |\n",
                r.seed,
                s.end_tick,
                s.survivors[0],
                s.survivors[1],
                tick_cell(s.first_engaged_tick[1]),
                s.contact[1].mean_engaged,
                s.contact[1].max_engaged,
                s.contact[0].mean_engaged,
                s.totals[1].cancelled(),
                s.totals[0].wasted_shots,
            ),
        );
    }
    out
}

fn outcome_section(results: &[SeedResult]) -> String {
    let wins = |winner: usize| {
        results
            .iter()
            .filter(|r| r.summary.survivors[winner] > 0 && r.summary.survivors[1 - winner] == 0)
            .count()
    };
    let unresolved = results.len() - wins(0) - wins(1);
    let mut out = format!(
        "## Outcomes\n\n{} seeds: blue wins {}, red wins {}, unresolved at the tick limit {}.\n\n",
        results.len(),
        wins(0),
        wins(1),
        unresolved
    );
    for team in Team::BOTH {
        let mut histogram: BTreeMap<u32, usize> = BTreeMap::new();
        for r in results {
            *histogram
                .entry(r.summary.survivors[team.index()])
                .or_default() += 1;
        }
        let cells: Vec<String> = histogram
            .iter()
            .map(|(survivors, seeds)| format!("{survivors}: {seeds}"))
            .collect();
        push(
            &mut out,
            format_args!(
                "- {} survivors (survivors: seeds): {}\n",
                team.label(),
                cells.join(", ")
            ),
        );
    }
    out.push('\n');
    out
}

/// A measure's label, the decimals to print, and how to read it from a summary for one side.
type MeasureRow = (&'static str, usize, fn(&BattleSummary, usize) -> f64);

fn measure_rows() -> [MeasureRow; 17] {
    [
        ("Survivors", 2, |s, i| f64::from(s.survivors[i])),
        ("Health left", 1, |s, i| s.remaining_health[i]),
        ("End tick", 1, |s, _| f64::from(s.end_tick)),
        ("First tick in contact", 1, |s, i| {
            s.first_engaged_tick[i].map_or(f64::NAN, f64::from)
        }),
        ("Units in contact, mean", 2, |s, i| {
            s.contact[i].mean_engaged
        }),
        ("Units in contact, max", 2, |s, i| {
            f64::from(s.contact[i].max_engaged)
        }),
        ("Fraction in contact, mean", 3, |s, i| {
            s.contact[i].mean_fraction
        }),
        ("Unit-seconds engaged", 1, |s, i| s.unit_seconds[i][0]),
        ("Unit-seconds blocked", 1, |s, i| s.unit_seconds[i][1]),
        ("Unit-seconds flanking", 1, |s, i| s.unit_seconds[i][2]),
        ("Unit-seconds approaching", 1, |s, i| s.unit_seconds[i][3]),
        ("Shots or swings released", 1, |s, i| {
            f64::from(s.totals[i].shots_released)
        }),
        ("Swings cancelled, target gone", 2, |s, i| {
            f64::from(s.totals[i].cancelled_target_gone)
        }),
        ("Swings cancelled, out of band", 2, |s, i| {
            f64::from(s.totals[i].cancelled_out_of_band)
        }),
        ("Shots wasted", 2, |s, i| {
            f64::from(s.totals[i].wasted_shots)
        }),
        ("Target switches", 2, |s, i| {
            f64::from(s.totals[i].target_switches)
        }),
        ("Unit-ticks pushed in contact", 1, |s, i| {
            f64::from(s.totals[i].pushed_engaged)
        }),
    ]
}

fn measures_section(results: &[SeedResult]) -> String {
    let mut out = String::from(
        "## Measures over seeds\n\n| Measure | Side | Mean | SD | Min | Median | Max |\n\
         |---|---|---|---|---|---|---|\n",
    );
    for (name, precision, read) in measure_rows() {
        for team in Team::BOTH {
            let values: Vec<f64> = results
                .iter()
                .map(|r| read(&r.summary, team.index()))
                .filter(|v| v.is_finite())
                .collect();
            push(
                &mut out,
                format_args!(
                    "| {name} | {} | {} |\n",
                    team.label(),
                    Spread::of(&values).cells(precision)
                ),
            );
        }
    }
    out.push('\n');
    out
}

fn contact_histogram_section(results: &[SeedResult]) -> String {
    let mut out = String::from(
        "## Units in contact per tick, all seeds pooled\n\nShare of contact-window ticks with \
         exactly k units of the side in contact.\n\n| k | Blue | Red |\n|---|---|---|\n",
    );
    let pooled = |side: usize| {
        let mut counts: Vec<u64> = Vec::new();
        for r in results {
            let histogram = &r.summary.contact[side].histogram;
            if counts.len() < histogram.len() {
                counts.resize(histogram.len(), 0);
            }
            for (slot, &ticks) in histogram.iter().enumerate() {
                counts[slot] += u64::from(ticks);
            }
        }
        counts
    };
    let (blue, red) = (pooled(0), pooled(1));
    let total = |counts: &[u64]| counts.iter().sum::<u64>().max(1);
    let share = |counts: &[u64], k: usize| {
        #[expect(
            clippy::cast_precision_loss,
            reason = "tick counts of a few hundred battles are far below 2^52"
        )]
        let value = counts.get(k).copied().unwrap_or(0) as f64 * 100.0 / total(counts) as f64;
        value
    };
    for k in 0..blue.len().max(red.len()) {
        push(
            &mut out,
            format_args!(
                "| {k} | {:.1} % | {:.1} % |\n",
                share(&blue, k),
                share(&red, k)
            ),
        );
    }
    out.push('\n');
    out
}

fn contact_over_time_section(results: &[SeedResult]) -> String {
    let mut out = String::from(
        "## Contact over time, mean over seeds\n\nEach second's mean over the seeds still fighting \
         in it (n).\n\n| s | n | Blue alive | Red alive | Blue in contact | Red in contact | Red \
         blocked | Red flanking | Red approaching |\n|---|---|---|---|---|---|---|---|---|\n",
    );
    let seconds = results
        .iter()
        .map(|r| r.summary.seconds.len())
        .max()
        .unwrap_or(0);
    for index in 0..seconds {
        let rows: Vec<&SecondRow> = results
            .iter()
            .filter_map(|r| r.summary.seconds.get(index))
            .collect();
        let n = f64::from(u32::try_from(rows.len()).unwrap_or(u32::MAX).max(1));
        let mean = |read: &dyn Fn(&SecondRow) -> f64| rows.iter().map(|r| read(r)).sum::<f64>() / n;
        push(
            &mut out,
            format_args!(
                "| {} | {} | {:.1} | {:.1} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |\n",
                index + 1,
                rows.len(),
                mean(&|r| f64::from(r.teams[0].alive)),
                mean(&|r| f64::from(r.teams[1].alive)),
                mean(&|r| r.teams[0].activity[Activity::Engaged.index()]),
                mean(&|r| r.teams[1].activity[Activity::Engaged.index()]),
                mean(&|r| r.teams[1].activity[Activity::Blocked.index()]),
                mean(&|r| r.teams[1].activity[Activity::Flanking.index()]),
                mean(&|r| r.teams[1].activity[Activity::Approaching.index()]),
            ),
        );
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_of_known_values() {
        let s = Spread::of(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
        assert!((s.mean - 5.0).abs() < 1e-12);
        assert!((s.sd - (32.0_f64 / 7.0).sqrt()).abs() < 1e-12);
        assert!((s.min - 2.0).abs() < 1e-12);
        assert!((s.median - 4.5).abs() < 1e-12);
        assert!((s.max - 9.0).abs() < 1e-12);
        let one = Spread::of(&[3.0]);
        assert!((one.sd).abs() < 1e-12 && (one.median - 3.0).abs() < 1e-12);
        assert!((Spread::of(&[]).mean).abs() < 1e-12);
    }

    #[test]
    fn every_side_column_has_a_cell() {
        assert_eq!(side_cells(&TeamTick::default()).len(), SIDE_COLUMNS.len());
    }
}
