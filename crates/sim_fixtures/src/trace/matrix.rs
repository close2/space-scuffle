//! Whole-matrix mode of the trace tool: every battle on every seed, as one table, and the
//! comparison of two such tables.
//!
//! # Responsibility
//! [`run_matrix`] plays every battle of a matchup matrix on each selected seed with
//! [`BattleRun`], judges each run with the shared expectation check ([`Expected::holds`]), and
//! writes `<out>/matrix.csv` (one row per battle and seed, [`CSV_HEADER`]) and `<out>/matrix.md`
//! (per-battle summary, the expectation failures, and with `--compare` the battles that moved
//! against an earlier `matrix.csv`, or the line "bit-identical to <file>").
//!
//! # How it fits
//! The measurement step of every package of plan `2026-11-04-combat-sc2-alignment` (D-5): a rule
//! or data change is run on the whole matrix and compared with the previous step's table. Battles
//! are played by the same [`BattleRun`] and judged by the same [`Expected::holds`] as the matrix
//! test `sc2_who_wins_matchup_matrix_tests.rs`, so a seed of record fails here exactly when it
//! fails there. No per-tick trace is kept: 50 battles on 20 seeds would write gigabytes, and
//! `--battle` mode ([`super::cli::run`]) traces any one battle in depth.
//!
//! # Invariants
//! - Rows are in fixture order, then seed order, whatever the number of worker threads: each
//!   run is a pure function of battle, catalog and seed, and results are placed by index.
//! - Every value in `matrix.csv` is printed exactly (survivors and ticks as integers, lead hull as
//!   the exact decimal form of the fixed-point `SimScalar`), so two tables are compared as text:
//!   a battle "moved" iff one of its rows differs or exists in one table only.
//! - A battle name that would break the CSV (a comma, quote or line break) is an error, never a
//!   silently shifted column.
//! - The blocking columns ([`super::blocking`], plan `2026-11-09-friendly-blocking` WP1) are a
//!   measurement of the same run, not part of its outcome: two tables are compared on the outcome
//!   columns only, and a table written before those columns existed ([`LEGACY_CSV_HEADER`]) is
//!   still read, with no blocking figures.

use super::blocking::{BlockingMeter, BlockingMetrics};
use super::cli::{
    create_dir, load_catalogs, load_matrix, read, source_line, write, RunError, TraceArgs,
};
use crate::matchup::{Battle, BattleRun, Expected, Matrix, Outcome};
use sim_core::catalog::RuntimeCatalogs;
use sim_types::SimScalar;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The header line of `matrix.csv`.
pub const CSV_HEADER: &str = "battle,seed,of_record,end_tick,blue_survivors,red_survivors,\
blue_lead_hull,red_lead_hull,expectation_met,blocked_attacker_ticks,reversals,stall_giveups";

/// The header of a `matrix.csv` written before the blocking columns, still read by `--compare`.
pub const LEGACY_CSV_HEADER: &str = "battle,seed,of_record,end_tick,blue_survivors,\
red_survivors,blue_lead_hull,red_lead_hull,expectation_met";

/// Outcome columns of `matrix.csv`, the whole of a legacy row.
const OUTCOME_COLUMNS: usize = 9;

/// Columns of `matrix.csv`: the outcome, then the three blocking counters.
const CSV_COLUMNS: usize = 12;

/// One battle played on one seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixRow {
    /// The battle's name.
    pub battle: String,
    /// The world seed.
    pub seed: u64,
    /// Whether the seed is one of the fixture's seeds of record (those the matrix test plays).
    pub of_record: bool,
    /// Ticks played when the battle stopped (a side gone, or the tick limit).
    pub end_tick: u32,
    /// Survivors and lead-unit hull at the end.
    pub outcome: Outcome,
    /// Whether the outcome meets the battle's expectation ([`Expected::holds`]).
    pub expectation_met: bool,
    /// Both sides' blocking counters over the run; `None` for a row read from a legacy table.
    pub blocking: Option<BlockingMetrics>,
}

impl MatrixRow {
    /// The row as one line of `matrix.csv`, without the line break; an absent hull is empty,
    /// and so are the blocking cells of a row without blocking figures.
    #[must_use]
    pub fn csv_line(&self) -> String {
        let blocking = self.blocking.map_or_else(
            || ",,".to_owned(),
            |b| {
                format!(
                    "{},{},{}",
                    b.blocked_attacker_ticks, b.reversals, b.stall_giveups
                )
            },
        );
        format!("{},{blocking}", self.outcome_line())
    }

    /// The outcome columns of the row, the part two tables are compared on.
    #[must_use]
    pub fn outcome_line(&self) -> String {
        let hull = |value: Option<SimScalar>| value.map_or_else(String::new, |v| v.0.to_string());
        format!(
            "{},{},{},{},{},{},{},{},{}",
            self.battle,
            self.seed,
            self.of_record,
            self.end_tick,
            self.outcome.blue,
            self.outcome.red,
            hull(self.outcome.blue_lead_hull),
            hull(self.outcome.red_lead_hull),
            self.expectation_met
        )
    }

    /// Parses one line of `matrix.csv` (not the header): [`CSV_COLUMNS`] cells, or
    /// [`OUTCOME_COLUMNS`] for a legacy table, whose row has no blocking figures.
    ///
    /// # Errors
    /// A description of the first malformed cell.
    pub fn from_csv_line(line: &str) -> Result<Self, String> {
        let cells: Vec<&str> = line.split(',').collect();
        if cells.len() != CSV_COLUMNS && cells.len() != OUTCOME_COLUMNS {
            return Err(format!("{} cells, expected {CSV_COLUMNS}", cells.len()));
        }
        let blocking = match cells.get(OUTCOME_COLUMNS..) {
            Some(["", "", ""]) => None,
            Some(["", _, _]) => return Err("blocking cells: all empty or all filled".to_owned()),
            Some([ticks, reversals, giveups]) => Some(BlockingMetrics {
                blocked_attacker_ticks: cell(ticks, "blocked_attacker_ticks")?,
                reversals: cell(reversals, "reversals")?,
                stall_giveups: cell(giveups, "stall_giveups")?,
            }),
            _ => None,
        };
        Ok(Self {
            battle: cells[0].to_owned(),
            seed: cell(cells[1], "seed")?,
            of_record: cell(cells[2], "of_record")?,
            end_tick: cell(cells[3], "end_tick")?,
            outcome: Outcome {
                blue: cell(cells[4], "blue_survivors")?,
                red: cell(cells[5], "red_survivors")?,
                blue_lead_hull: hull_cell(cells[6], "blue_lead_hull")?,
                red_lead_hull: hull_cell(cells[7], "red_lead_hull")?,
            },
            expectation_met: cell(cells[8], "expectation_met")?,
            blocking,
        })
    }
}

/// Parses the cell `text` of column `column`.
fn cell<T: std::str::FromStr>(text: &str, column: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{column}: cannot read `{text}`"))
}

/// Parses a lead-hull cell: empty for a dead lead unit, otherwise an exact decimal.
fn hull_cell(text: &str, column: &str) -> Result<Option<SimScalar>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    let value = text
        .parse()
        .map_err(|_| format!("{column}: cannot read `{text}`"))?;
    Ok(Some(SimScalar(value)))
}

/// `matrix.csv` for `rows`, header first.
#[must_use]
pub fn matrix_csv(rows: &[MatrixRow]) -> String {
    let mut out = String::from(CSV_HEADER);
    out.push('\n');
    for row in rows {
        out.push_str(&row.csv_line());
        out.push('\n');
    }
    out
}

/// Reads the rows of the `matrix.csv` text `text`, which came from `path`.
///
/// # Errors
/// [`RunError::Csv`] if the header is neither [`CSV_HEADER`] nor [`LEGACY_CSV_HEADER`], or a
/// row is malformed.
pub fn parse_matrix_csv(path: &Path, text: &str) -> Result<Vec<MatrixRow>, RunError> {
    let error = |line: usize, reason: String| RunError::Csv {
        path: path.to_owned(),
        line,
        reason,
    };
    let mut lines = text.lines();
    if !matches!(lines.next(), Some(CSV_HEADER | LEGACY_CSV_HEADER)) {
        return Err(error(1, format!("the header is not `{CSV_HEADER}`")));
    }
    lines
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(index, line)| {
            MatrixRow::from_csv_line(line).map_err(|reason| error(index + 2, reason))
        })
        .collect()
}

/// The comparison of a whole-matrix run with an earlier `matrix.csv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// The earlier `matrix.csv`.
    pub file: PathBuf,
    /// Its rows.
    pub before: Vec<MatrixRow>,
    /// Battles with a row that differs or exists in one table only: in the order of this run,
    /// then those only in the earlier table, by name. Empty when the tables are identical.
    pub moved: Vec<String>,
}

impl Comparison {
    /// Compares `after` with the rows `before` read from `file`.
    #[must_use]
    pub fn new(file: PathBuf, before: Vec<MatrixRow>, after: &[MatrixRow]) -> Self {
        let (old, new) = (outcome_lines(&before), outcome_lines(after));
        let mut moved = Vec::new();
        let mut seen = BTreeSet::new();
        for row in after.iter().chain(&before) {
            if !seen.insert(row.battle.as_str()) {
                continue;
            }
            if seeds_moved(&old, &new, &row.battle) > 0 {
                moved.push(row.battle.clone());
            }
        }
        // Battles only in the earlier table come after this run's, by name.
        let only_before: BTreeSet<String> = before
            .iter()
            .filter(|row| !after.iter().any(|a| a.battle == row.battle))
            .map(|row| row.battle.clone())
            .collect();
        moved.retain(|name| !only_before.contains(name));
        moved.extend(only_before);
        Self {
            file,
            before,
            moved,
        }
    }
}

/// Each row's outcome columns, by battle and seed.
fn outcome_lines(rows: &[MatrixRow]) -> BTreeMap<(String, u64), String> {
    rows.iter()
        .map(|row| ((row.battle.clone(), row.seed), row.outcome_line()))
        .collect()
}

/// Seeds of `battle` whose row differs between `old` and `new`, or exists in one only.
fn seeds_moved(
    old: &BTreeMap<(String, u64), String>,
    new: &BTreeMap<(String, u64), String>,
    battle: &str,
) -> usize {
    let keys: BTreeSet<&(String, u64)> = old
        .keys()
        .chain(new.keys())
        .filter(|(name, _)| name == battle)
        .collect();
    keys.into_iter()
        .filter(|key| old.get(*key) != new.get(*key))
        .count()
}

/// What a whole-matrix run wrote.
#[derive(Debug, Clone)]
pub struct MatrixOutput {
    /// The directory holding `matrix.csv` and `matrix.md`.
    pub directory: PathBuf,
    /// Every run, in fixture order then seed order.
    pub rows: Vec<MatrixRow>,
    /// The comparison with `--compare`'s file, if one was given.
    pub comparison: Option<Comparison>,
}

/// Plays every battle of `args`'s fixture on every selected seed, and writes `matrix.csv` and
/// `matrix.md` into `args.out`.
///
/// # Errors
/// [`RunError::Usage`] without `--all-battles`; [`RunError::Csv`] for a battle name the CSV
/// cannot hold or a malformed `--compare` file; [`RunError::Battle`] for a battle that fails to
/// set up or raises a simulation diagnostic; and the I/O, fixture and catalog errors.
pub fn run_matrix(args: &TraceArgs) -> Result<MatrixOutput, RunError> {
    if !args.all_battles {
        return Err(RunError::Usage(
            "whole-matrix mode needs --all-battles".into(),
        ));
    }
    let matrix = load_matrix(args)?;
    let csv_path = args.out.join("matrix.csv");
    for (index, battle) in matrix.battles.iter().enumerate() {
        if battle.name.contains([',', '"', '\n', '\r']) {
            return Err(RunError::Csv {
                path: csv_path,
                line: index + 2,
                reason: format!(
                    "battle name `{}` holds a comma, quote or line break",
                    battle.name
                ),
            });
        }
    }
    let catalogs = load_catalogs(args)?;
    let seeds = args.seed_list(&matrix.seeds);
    let rows = play_matrix(&matrix, &catalogs, &seeds, args.jobs)?;
    let comparison = match &args.compare {
        Some(file) => {
            let before = parse_matrix_csv(file, &read(file)?)?;
            Some(Comparison::new(file.clone(), before, &rows))
        }
        None => None,
    };
    create_dir(&args.out)?;
    write(&csv_path, &matrix_csv(&rows))?;
    let preamble = format!(
        "{} Seeds per battle: {} ({} of record).",
        source_line(args),
        seeds.len(),
        seeds
            .iter()
            .filter(|seed| matrix.seeds.contains(seed))
            .count()
    );
    write(
        &args.out.join("matrix.md"),
        &matrix_markdown(&matrix, &preamble, &rows, comparison.as_ref()),
    )?;
    Ok(MatrixOutput {
        directory: args.out.clone(),
        rows,
        comparison,
    })
}

/// Plays every battle of `matrix` with `catalogs` on every one of `seeds`, `jobs` battles at a
/// time, and returns the rows in fixture order then seed order.
///
/// # Errors
/// [`RunError::Battle`] for the first run (in that order) that failed.
pub fn play_matrix(
    matrix: &Matrix,
    catalogs: &RuntimeCatalogs,
    seeds: &[u64],
    jobs: usize,
) -> Result<Vec<MatrixRow>, RunError> {
    let runs: Vec<(&Battle, u64)> = matrix
        .battles
        .iter()
        .flat_map(|battle| seeds.iter().map(move |&seed| (battle, seed)))
        .collect();
    // PERFORMANCE RATIONALE:
    // Why: the whole matrix on 20 seeds is 1000 battles, some with over a hundred units; in
    // series that takes many minutes even in a release build.
    // What: worker threads claim runs from a shared counter; each result is stored with its
    // index and the rows are put back in run order afterwards.
    // Trade-off: one catalog clone per run and a sort, in exchange for a run that scales with
    // `--jobs`. Each run is independent and deterministic, so the output does not depend on it.
    let next = AtomicUsize::new(0);
    let mut results: Vec<(usize, Result<MatrixRow, RunError>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..jobs.max(1))
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&(battle, seed)) = runs.get(index) else {
                            break done;
                        };
                        done.push((index, play_one(matrix, battle, catalogs, seed)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| match worker.join() {
                Ok(done) => done,
                Err(payload) => std::panic::resume_unwind(payload),
            })
            .collect()
    });
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, row)| row).collect()
}

/// Plays `battle` of `matrix` on `seed` to its end.
fn play_one(
    matrix: &Matrix,
    battle: &Battle,
    catalogs: &RuntimeCatalogs,
    seed: u64,
) -> Result<MatrixRow, RunError> {
    let failed = |source: RunError| RunError::Battle {
        battle: battle.name.clone(),
        seed,
        source: Box::new(source),
    };
    let mut run = BattleRun::new(battle, catalogs.clone(), seed, matrix.jitter_for(seed))
        .map_err(|source| failed(source.into()))?;
    let mut meter = BlockingMeter::new();
    while !run.is_finished() {
        let diagnostics = run.step();
        meter.observe(&run.world);
        if !diagnostics.is_empty() {
            return Err(failed(RunError::Diagnostics {
                seed,
                tick: run.ticks_played(),
                diagnostics: format!("{diagnostics:?}"),
            }));
        }
    }
    let outcome = Outcome::of(&run);
    Ok(MatrixRow {
        battle: battle.name.clone(),
        seed,
        of_record: matrix.seeds.contains(&seed),
        end_tick: run.ticks_played(),
        outcome,
        expectation_met: battle.expect.holds(&outcome),
        blocking: Some(meter.total()),
    })
}

/// The per-battle figures of `matrix.md`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct BattleStats {
    seeds: usize,
    blue_wins: usize,
    red_wins: usize,
    unresolved: usize,
    both_gone: usize,
    survivors: [f64; 2],
    end_tick: f64,
    /// Mean lead hull over the seeds on which the lead unit lived, and that seed count.
    lead_hull: [(f64, usize); 2],
    failures_of_record: usize,
    failures: usize,
}

impl BattleStats {
    /// The figures of `battle`'s rows among `rows`.
    fn of(rows: &[MatrixRow], battle: &str) -> Self {
        let rows: Vec<&MatrixRow> = rows.iter().filter(|row| row.battle == battle).collect();
        let count = |test: &dyn Fn(&MatrixRow) -> bool| rows.iter().filter(|row| test(row)).count();
        let mean = |values: Vec<f64>| -> (f64, usize) {
            let n = values.len();
            if n == 0 {
                return (0.0, 0);
            }
            (values.iter().sum::<f64>() / f64::from(count_u32(n)), n)
        };
        let survivors = |pick: fn(&Outcome) -> usize| {
            mean(
                rows.iter()
                    .map(|row| f64::from(count_u32(pick(&row.outcome))))
                    .collect(),
            )
            .0
        };
        let hull = |pick: fn(&Outcome) -> Option<SimScalar>| {
            mean(
                rows.iter()
                    .filter_map(|row| pick(&row.outcome))
                    .map(|value| f64::from(value.to_f32()))
                    .collect(),
            )
        };
        Self {
            seeds: rows.len(),
            blue_wins: count(&|row| row.outcome.blue > 0 && row.outcome.red == 0),
            red_wins: count(&|row| row.outcome.red > 0 && row.outcome.blue == 0),
            unresolved: count(&|row| row.outcome.blue > 0 && row.outcome.red > 0),
            both_gone: count(&|row| row.outcome.blue == 0 && row.outcome.red == 0),
            survivors: [survivors(|o| o.blue), survivors(|o| o.red)],
            end_tick: mean(rows.iter().map(|row| f64::from(row.end_tick)).collect()).0,
            lead_hull: [hull(|o| o.blue_lead_hull), hull(|o| o.red_lead_hull)],
            failures_of_record: count(&|row| row.of_record && !row.expectation_met),
            failures: count(&|row| !row.expectation_met),
        }
    }

    /// Mean survivors, `blue – red`.
    fn survivors_cell(&self) -> String {
        format!("{:.2} – {:.2}", self.survivors[0], self.survivors[1])
    }

    /// Mean lead hull of one side, with the number of seeds it lived on; `—` if it never did.
    fn hull_cell(&self, side: usize) -> String {
        let (mean, seeds) = self.lead_hull[side];
        if seeds == 0 {
            "—".to_owned()
        } else {
            format!("{mean:.2} ({seeds})")
        }
    }
}

/// Appends formatted text to `out`.
fn push(out: &mut String, args: std::fmt::Arguments<'_>) {
    out.push_str(&std::fmt::format(args));
}

/// Converts a count to `u32`, saturating; matrices hold far fewer runs.
fn count_u32(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The names of the battles in `rows`, in first-appearance order.
fn battle_names(rows: &[MatrixRow]) -> Vec<&str> {
    let mut seen = BTreeSet::new();
    rows.iter()
        .map(|row| row.battle.as_str())
        .filter(|name| seen.insert(*name))
        .collect()
}

/// `matrix.md`: totals, the per-battle table, the expectation failures, and the comparison.
#[must_use]
pub fn matrix_markdown(
    matrix: &Matrix,
    preamble: &str,
    rows: &[MatrixRow],
    comparison: Option<&Comparison>,
) -> String {
    let names = battle_names(rows);
    let failures: Vec<&MatrixRow> = rows.iter().filter(|row| !row.expectation_met).collect();
    let mut out = format!(
        "# Whole matrix\n\n{preamble}\n\n{} battles, {} runs. Expectation failures: {} on the \
         seeds of record, {} over all seeds.\n\n",
        names.len(),
        rows.len(),
        failures.iter().filter(|row| row.of_record).count(),
        failures.len()
    );
    if let Some(comparison) = comparison {
        out.push_str(&comparison_section(comparison, rows));
    }
    out.push_str(
        "## Battles\n\nMeans over the seeds; lead hull is the first unit's life, averaged over \
         the seeds it lived on (that count in brackets).\n\n\
         | Battle | Seeds | Blue wins | Red wins | Unresolved | Both gone | Survivors blue – red \
         | End tick | Blue lead hull | Red lead hull | Failures of record | Failures, all seeds |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for name in &names {
        let s = BattleStats::of(rows, name);
        push(
            &mut out,
            format_args!(
                "| {name} | {} | {} | {} | {} | {} | {} | {:.1} | {} | {} | {} | {} |\n",
                s.seeds,
                s.blue_wins,
                s.red_wins,
                s.unresolved,
                s.both_gone,
                s.survivors_cell(),
                s.end_tick,
                s.hull_cell(0),
                s.hull_cell(1),
                s.failures_of_record,
                s.failures
            ),
        );
    }
    out.push_str(&failures_section(matrix, &failures));
    out.push_str(&blocking_section(rows, comparison));
    out
}

/// Both sides' blocking counters summed over `rows`, or `None` if a row has none.
fn blocking_sum<'a>(rows: impl IntoIterator<Item = &'a MatrixRow>) -> Option<BlockingMetrics> {
    rows.into_iter()
        .try_fold(BlockingMetrics::default(), |sum, row| {
            row.blocking.map(|blocking| sum.plus(blocking))
        })
}

/// The per-battle blocking counters, summed over the seeds, with the earlier table's totals.
fn blocking_section(rows: &[MatrixRow], comparison: Option<&Comparison>) -> String {
    let mut out = String::from(
        "\n## Blocking\n\nBoth sides' counters of plan `2026-11-09-friendly-blocking` WP1, summed \
         over the seeds: unit-ticks out of range behind a friend that will not yield, step \
         reversals under an unchanged order (D-1.4), and orders given up for lack of progress.\n\n",
    );
    let cell = |sum: Option<BlockingMetrics>| {
        sum.map_or_else(
            || "— | — | —".to_owned(),
            |b| {
                format!(
                    "{} | {} | {}",
                    b.blocked_attacker_ticks, b.reversals, b.stall_giveups
                )
            },
        )
    };
    if let Some(comparison) = comparison {
        push(
            &mut out,
            format_args!(
                "All battles: {} before, {} now (blocked ticks | reversals | give-ups).\n\n",
                cell(blocking_sum(&comparison.before)),
                cell(blocking_sum(rows))
            ),
        );
    }
    out.push_str(
        "| Battle | Blocked attacker ticks | Reversals | Stall give-ups |\n|---|---|---|---|\n",
    );
    for name in battle_names(rows) {
        let sum = blocking_sum(rows.iter().filter(|row| row.battle == name));
        push(&mut out, format_args!("| {name} | {} |\n", cell(sum)));
    }
    out
}

/// The table of runs that broke their battle's expectation.
fn failures_section(matrix: &Matrix, failures: &[&MatrixRow]) -> String {
    let mut out = String::from("\n## Expectation failures\n\n");
    if failures.is_empty() {
        out.push_str("None: every run meets its battle's expectation.\n");
        return out;
    }
    out.push_str(
        "| Battle | Seed | Of record | Survivors blue – red | Lead hull blue / red | End tick | \
         Expected |\n|---|---|---|---|---|---|---|\n",
    );
    let hull = |value: Option<SimScalar>| value.map_or_else(|| "—".to_owned(), |v| v.to_string());
    for row in failures {
        let expected = matrix.battle(&row.battle).map_or_else(
            |_| "battle not in the fixture".to_owned(),
            |b| expected_cell(&b.expect),
        );
        push(
            &mut out,
            format_args!(
                "| {} | {} | {} | {} – {} | {} / {} | {} | {expected} |\n",
                row.battle,
                row.seed,
                if row.of_record { "yes" } else { "no" },
                row.outcome.blue,
                row.outcome.red,
                hull(row.outcome.blue_lead_hull),
                hull(row.outcome.red_lead_hull),
                row.end_tick
            ),
        );
    }
    out
}

/// An expectation as text: survivor ranges, and hull ranges when recorded.
fn expected_cell(expect: &Expected) -> String {
    let mut text = format!(
        "blue {}–{}, red {}–{}",
        expect.blue.0, expect.blue.1, expect.red.0, expect.red.1
    );
    for (side, range) in [("blue", expect.blue_lead_hp), ("red", expect.red_lead_hp)] {
        if let Some((low, high)) = range {
            push(&mut text, format_args!(", {side} lead hull {low}–{high}"));
        }
    }
    for (side, alive) in [
        ("blue", expect.blue_lead_alive),
        ("red", expect.red_lead_alive),
    ] {
        if let Some(alive) = alive {
            let fate = if alive { "alive" } else { "dead" };
            push(&mut text, format_args!(", {side} lead {fate}"));
        }
    }
    text
}

/// The comparison with an earlier table: "bit-identical", or the moved battles before and after.
fn comparison_section(comparison: &Comparison, rows: &[MatrixRow]) -> String {
    let file = comparison.file.display();
    if comparison.moved.is_empty() {
        return format!(
            "Comparison: bit-identical to `{file}` (all {} rows match).\n\n",
            rows.len()
        );
    }
    let (old, new) = (outcome_lines(&comparison.before), outcome_lines(rows));
    let mut out = format!(
        "## Moved against `{file}`\n\nBattles moved: {}. Before and after are means over each \
         table's seeds.\n\n\
         | Battle | Seeds moved | Survivors before | after | End tick before | after | Lead hull \
         blue / red before | after | Failures of record / all before | after |\n\
         |---|---|---|---|---|---|---|---|---|---|\n",
        comparison.moved.len()
    );
    for name in &comparison.moved {
        let before = BattleStats::of(&comparison.before, name);
        let after = BattleStats::of(rows, name);
        push(
            &mut out,
            format_args!(
            "| {name} | {} | {} | {} | {:.1} | {:.1} | {} / {} | {} / {} | {} / {} | {} / {} |\n",
            seeds_moved(&old, &new, name),
            before.survivors_cell(),
            after.survivors_cell(),
            before.end_tick,
            after.end_tick,
            before.hull_cell(0),
            before.hull_cell(1),
            after.hull_cell(0),
            after.hull_cell(1),
            before.failures_of_record,
            before.failures,
            after.failures_of_record,
            after.failures
        ),
        );
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(battle: &str, seed: u64, blue: usize, hull: Option<SimScalar>) -> MatrixRow {
        MatrixRow {
            battle: battle.to_owned(),
            seed,
            of_record: seed == 1,
            end_tick: 120,
            outcome: Outcome {
                blue,
                red: 0,
                blue_lead_hull: hull,
                red_lead_hull: None,
            },
            expectation_met: true,
            blocking: Some(BlockingMetrics {
                blocked_attacker_ticks: 7,
                reversals: 3,
                stall_giveups: 1,
            }),
        }
    }

    #[test]
    fn a_row_survives_the_csv_exactly() -> Result<(), String> {
        // A hull with every fractional bit set has a long exact decimal form.
        let awkward = SimScalar::from_raw((37 << 32) | 0xFFFF_FFFF);
        let tiny = SimScalar::from_raw(1);
        for hull in [
            None,
            Some(SimScalar::from_i32(45)),
            Some(awkward),
            Some(tiny),
        ] {
            let original = row("a_b", 3_203_386_110, 2, hull);
            let line = original.csv_line();
            assert_eq!(MatrixRow::from_csv_line(&line)?, original, "{line}");
        }
        Ok(())
    }

    #[test]
    fn a_legacy_row_reads_without_blocking_figures_and_compares_on_its_outcome(
    ) -> Result<(), String> {
        let current = row("a_b", 5, 2, Some(SimScalar::from_i32(45)));
        let legacy = MatrixRow::from_csv_line(&current.outcome_line())?;
        assert_eq!(legacy.blocking, None);
        assert_eq!(legacy.outcome_line(), current.outcome_line());
        assert!(legacy.csv_line().ends_with(",,,"));
        assert_eq!(MatrixRow::from_csv_line(&legacy.csv_line())?, legacy);
        let comparison = Comparison::new(PathBuf::from("m.csv"), vec![legacy], &[current]);
        assert_eq!(comparison.moved, Vec::<String>::new());
        let path = Path::new("m.csv");
        assert!(parse_matrix_csv(path, &format!("{LEGACY_CSV_HEADER}\n")).is_ok());
        Ok(())
    }

    #[test]
    fn malformed_lines_and_headers_are_errors() {
        assert!(MatrixRow::from_csv_line("a,1,true,5,1,0,,,true,1,,").is_err());
        assert!(MatrixRow::from_csv_line("a,1,true,5,1,0,,,true,1,x,0").is_err());
        assert!(MatrixRow::from_csv_line("a,1,true,5,1,0,,").is_err());
        assert!(MatrixRow::from_csv_line("a,x,true,5,1,0,,,true").is_err());
        assert!(MatrixRow::from_csv_line("a,1,yes,5,1,0,,,true").is_err());
        assert!(MatrixRow::from_csv_line("a,1,true,5,1,0,4.x,,true").is_err());
        let path = Path::new("m.csv");
        assert!(matches!(
            parse_matrix_csv(path, "battle,seed\n"),
            Err(RunError::Csv { line: 1, .. })
        ));
        assert!(matches!(
            parse_matrix_csv(path, &format!("{CSV_HEADER}\na,1\n")),
            Err(RunError::Csv { line: 2, .. })
        ));
    }

    #[test]
    fn a_comparison_names_every_battle_with_a_changed_or_missing_row() {
        let before = vec![
            row("same", 1, 2, None),
            row("hull", 1, 2, Some(SimScalar::from_i32(10))),
            row("seeds", 1, 2, None),
            row("gone", 1, 2, None),
        ];
        let after = vec![
            row("same", 1, 2, None),
            row("hull", 1, 2, Some(SimScalar::from_i32(11))),
            row("seeds", 1, 2, None),
            row("seeds", 2, 2, None),
            row("new", 1, 2, None),
        ];
        let comparison = Comparison::new(PathBuf::from("m.csv"), before.clone(), &after);
        assert_eq!(comparison.moved, ["hull", "seeds", "new", "gone"]);
        let unchanged = Comparison::new(PathBuf::from("m.csv"), before.clone(), &before);
        assert_eq!(unchanged.moved, Vec::<String>::new());
    }

    #[test]
    fn battle_figures_count_wins_and_average_only_living_leads() {
        let mut rows = vec![
            row("b", 1, 2, Some(SimScalar::from_i32(10))),
            row("b", 2, 1, Some(SimScalar::from_i32(20))),
            row("b", 3, 0, None),
        ];
        rows[2].outcome.red = 1;
        rows[2].expectation_met = false;
        let stats = BattleStats::of(&rows, "b");
        assert_eq!(
            (
                stats.seeds,
                stats.blue_wins,
                stats.red_wins,
                stats.unresolved
            ),
            (3, 2, 1, 0)
        );
        assert!((stats.survivors[0] - 1.0).abs() < 1e-9);
        assert!((stats.lead_hull[0].0 - 15.0).abs() < 1e-9);
        assert_eq!(stats.lead_hull[0].1, 2);
        assert_eq!((stats.failures_of_record, stats.failures), (0, 1));
        assert_eq!(stats.hull_cell(1), "—");
    }
}
