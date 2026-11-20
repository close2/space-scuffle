//! Aggregation of the movement harness's CSVs: the tables of plan `2026-11-20-contact-solver` §5
//! and the verdicts of its thresholds T1 to T9 (§6.5).
//!
//! # Responsibility
//! [`load_run`] reads one run directory (`<family>.csv`, `<family>-relabel.csv`, `mirror.csv`);
//! [`aggregate`] reduces a family's rows to the aggregates of §6.3 with their 95 % bootstrap
//! intervals (§6.4: scenarios resampled with replacement, 2,000 resamples); [`sign_test`] and
//! [`wilson`] are the tests of M9 and M10; [`render`] prints the tables of several runs side by
//! side and [`verdicts`] judges every run after the first against the first ("today").
//!
//! # How it fits
//! The example `movement_harness --aggregate` calls [`render`]. Every random choice of the
//! bootstrap comes from a seeded [`SplitMix64`], so the same CSVs always give the same tables. All
//! of it is `f64` measurement code outside the simulation.
//!
//! # Invariants
//! - The noise rule (§6.4): a relative threshold ("not above today's") fails only when the value
//!   lies above today's aggregate and above the upper end of today's 95 % bootstrap interval.

use super::corpus::SplitMix64;
use super::HarnessError;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// One CSV line, by column name.
pub type Row = BTreeMap<String, String>;

/// Bootstrap resamples (§6.4).
pub const RESAMPLES: usize = 2000;

/// The families of the scene corpus, in report order.
pub const FAMILIES: [&str; 5] = ["gap", "arc", "cross", "corridor", "corner"];

/// One run directory's rows: per family the original and relabelled rows, and the mirror rows.
#[derive(Debug, Clone, Default)]
pub struct Run {
    /// A label for the tables.
    pub label: String,
    /// Per family, the original rows.
    pub scenes: BTreeMap<String, Vec<Row>>,
    /// Per family, the relabelled rows.
    pub relabelled: BTreeMap<String, Vec<Row>>,
    /// The mirror rows.
    pub mirror: Vec<Row>,
}

/// Parses CSV text with a header line into rows.
#[must_use]
pub fn parse_csv(text: &str) -> Vec<Row> {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let names: Vec<&str> = header.split(',').collect();
    lines
        .map(|line| {
            names
                .iter()
                .zip(line.split(','))
                .map(|(name, value)| ((*name).to_owned(), value.to_owned()))
                .collect()
        })
        .collect()
}

/// Reads the run in `dir`, labelled `label`; a missing file is an empty family.
///
/// # Errors
/// [`HarnessError::Io`] when a file exists but cannot be read.
pub fn load_run(label: &str, dir: &Path) -> Result<Run, HarnessError> {
    let read = |name: &str| -> Result<Vec<Row>, HarnessError> {
        let path = dir.join(name);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| HarnessError::Io(format!("{}: {e}", path.display())))?;
        Ok(parse_csv(&text))
    };
    let mut run = Run {
        label: label.to_owned(),
        ..Run::default()
    };
    for family in FAMILIES {
        run.scenes
            .insert(family.to_owned(), read(&format!("{family}.csv"))?);
        run.relabelled
            .insert(family.to_owned(), read(&format!("{family}-relabel.csv"))?);
    }
    run.mirror = read("mirror.csv")?;
    Ok(run)
}

/// A column of `row` as a number; 0 when absent or malformed.
#[must_use]
pub fn number(row: &Row, column: &str) -> f64 {
    row.get(column).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

/// One `key=value` of a row's `params` column as a number.
#[must_use]
pub fn param(row: &Row, key: &str) -> Option<f64> {
    row.get("params")?
        .split(';')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| value.parse().ok())
}

/// `Σ numerator / Σ denominator · scale` over `rows`; NaN when the denominator is zero.
#[must_use]
pub fn rate(rows: &[&Row], numerator: &str, denominator: &str, scale: f64) -> f64 {
    let below: f64 = rows.iter().map(|r| number(r, denominator)).sum();
    if below == 0.0 {
        return f64::NAN;
    }
    scale * rows.iter().map(|r| number(r, numerator)).sum::<f64>() / below
}

/// The `q` quantile of `values` as `sorted[min(n − 1, ⌊q n⌋)]`; NaN when empty.
#[must_use]
pub fn quantile(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    if sorted.is_empty() {
        return f64::NAN;
    }
    let index = index_of(q * count(sorted.len())).min(sorted.len() - 1);
    sorted[index]
}

/// `n` as `f64`.
fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// `⌊value⌋` as an index.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a quantile position is a small non-negative number"
)]
fn index_of(value: f64) -> usize {
    value.max(0.0) as usize
}

/// The 95 % bootstrap interval of `stat` over `rows` (§6.4), from a stream seeded with 7.
pub fn bootstrap(rows: &[&Row], stat: impl Fn(&[&Row]) -> f64) -> (f64, f64) {
    if rows.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    let mut rng = SplitMix64::new(7);
    let mut sample = Vec::with_capacity(rows.len());
    let mut sims: Vec<f64> = (0..RESAMPLES)
        .map(|_| {
            sample.clear();
            for _ in 0..rows.len() {
                let pick = usize::try_from(rng.below(rows.len() as u64)).unwrap_or(0);
                sample.push(rows[pick]);
            }
            stat(&sample)
        })
        .collect();
    sims.sort_by(f64::total_cmp);
    (sims[RESAMPLES * 25 / 1000], sims[RESAMPLES * 975 / 1000])
}

/// The two-sided exact sign test of paired rows on `column`: `(p, positive, negative)`, ties
/// dropped (M9).
#[must_use]
pub fn sign_test(original: &[Row], relabelled: &[Row], column: &str) -> (f64, u32, u32) {
    let (mut positive, mut negative) = (0_u32, 0_u32);
    for (a, b) in original.iter().zip(relabelled) {
        let (x, y) = (number(a, column), number(b, column));
        if x > y {
            positive += 1;
        } else if x < y {
            negative += 1;
        }
    }
    let n = positive + negative;
    if n == 0 {
        return (1.0, 0, 0);
    }
    let k = positive.min(negative);
    // Σ_{i ≤ k} C(n, i) / 2^n, in logarithms so large n do not overflow.
    let ln_half = -f64::from(n) * std::f64::consts::LN_2;
    let mut ln_choose = 0.0_f64;
    let mut tail = 0.0_f64;
    for i in 0..=k {
        if i > 0 {
            ln_choose += (f64::from(n - i + 1) / f64::from(i)).ln();
        }
        tail += (ln_choose + ln_half).exp();
    }
    ((2.0 * tail).min(1.0), positive, negative)
}

/// The share `k / n` with its 95 % Wilson interval `(share, low, high)` (M10).
#[must_use]
pub fn wilson(successes: u32, trials: u32) -> (f64, f64, f64) {
    if trials == 0 {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    let z = 1.96_f64;
    let (hits, total) = (f64::from(successes), f64::from(trials));
    let share = hits / total;
    let denominator = 1.0 + z * z / total;
    let centre = (share + z * z / (2.0 * total)) / denominator;
    let half =
        z * (share * (1.0 - share) / total + z * z / (4.0 * total * total)).sqrt() / denominator;
    (share, centre - half, centre + half)
}

/// One family's aggregates (§6.3), by name; intervals as `(low, high)` under `<name>_ci`.
#[derive(Debug, Clone, Default)]
pub struct Aggregates {
    /// Point values.
    pub values: BTreeMap<&'static str, f64>,
    /// 95 % bootstrap intervals.
    pub intervals: BTreeMap<&'static str, (f64, f64)>,
}

impl Aggregates {
    /// The value of `name`, NaN when absent.
    #[must_use]
    pub fn get(&self, name: &str) -> f64 {
        self.values.get(name).copied().unwrap_or(f64::NAN)
    }
}

/// The aggregates of `family` over `rows` (§6.3).
#[must_use]
pub fn aggregate(family: &str, rows: &[Row]) -> Aggregates {
    let all: Vec<&Row> = rows.iter().collect();
    let mut out = Aggregates::default();
    let m1 = |s: &[&Row]| rate(s, "rev_amp", "moving_ticks", 1000.0);
    let m2 = |s: &[&Row]| rate(s, "wasted", "path", 1.0);
    out.values.insert("M1", m1(&all));
    out.intervals.insert("M1", bootstrap(&all, m1));
    out.values
        .insert("M1hi", rate(&all, "rev_amp_hi", "moving_ticks", 1000.0));
    out.values
        .insert("legacy", all.iter().map(|r| number(r, "rev_legacy")).sum());
    out.values.insert("M2", m2(&all));
    out.intervals.insert("M2", bootstrap(&all, m2));
    let units: f64 = all.iter().map(|r| number(r, "units")).sum();
    let standing: f64 = all.iter().map(|r| number(r, "standing_wasted")).sum();
    out.values.insert(
        "M3",
        if units > 0.0 {
            standing / units
        } else {
            f64::NAN
        },
    );
    out.values
        .insert("M4", rate(&all, "progress_viol", "progress_windows", 100.0));
    if matches!(family, "gap" | "cross" | "corner") {
        completion(&all, &mut out);
    }
    if family == "gap" {
        gap_extras(&all, &mut out);
    }
    if matches!(family, "arc" | "corridor") {
        let fills: Vec<f64> = all
            .iter()
            .filter(|r| number(r, "expected") > 0.0)
            .map(|r| number(r, "in_range") / number(r, "expected"))
            .collect();
        out.values
            .insert("fill_mean", fills.iter().sum::<f64>() / count(fills.len()));
        out.values.insert("fill_p5", quantile(&fills, 0.05));
        out.values
            .insert("giveups", all.iter().map(|r| number(r, "giveups")).sum());
    }
    overlap(&all, &mut out);
    out
}

/// M5 over the rows with room (`expected > 0`).
fn completion(all: &[&Row], out: &mut Aggregates) {
    let passable: Vec<&Row> = all
        .iter()
        .copied()
        .filter(|r| number(r, "expected") > 0.0)
        .collect();
    let sum = |column: &str| passable.iter().map(|r| number(r, column)).sum::<f64>();
    out.values.insert("deadlocks", sum("deadlocked"));
    out.values.insert("giveups", sum("giveups"));
    let complete = passable
        .iter()
        .filter(|r| number(r, "done") >= number(r, "expected"))
        .count();
    out.values.insert(
        "complete_scen",
        100.0 * count(complete) / count(passable.len().max(1)),
    );
    out.values
        .insert("done_units", 100.0 * sum("done") / sum("expected").max(1.0));
    out.values.insert(
        "diagnostics",
        all.iter().map(|r| number(r, "diagnostics")).sum(),
    );
}

/// The gap family's wrong passes (§6.6 C-a), throughput η and its monotonicity in `w` (M7).
fn gap_extras(all: &[&Row], out: &mut Aggregates) {
    let small_may_pass = |r: &Row| -> f64 {
        let (Some(w), Some(radius), Some(n)) = (param(r, "w"), param(r, "r"), param(r, "n")) else {
            return 0.0;
        };
        let small = (radius * 0.75 * 64.0).round() / 64.0;
        if param(r, "mixed") == Some(1.0) && w > 2.0 * small {
            (n / 2.0).floor()
        } else {
            0.0
        }
    };
    let wrong: f64 = all
        .iter()
        .filter(|r| number(r, "expected") == 0.0)
        .map(|r| (number(r, "through") - small_may_pass(r)).max(0.0))
        .sum();
    out.values.insert("impassable_passed", wrong);
    let mut etas = Vec::new();
    let mut by_radius: BTreeMap<u64, BTreeMap<u64, Vec<f64>>> = BTreeMap::new();
    for r in all {
        if number(r, "expected") == 0.0 || number(r, "through") < 2.0 {
            continue;
        }
        let (Some(w), Some(radius), Some(v)) = (param(r, "w"), param(r, "r"), param(r, "v")) else {
            continue;
        };
        let lanes = ((w - 2.0 * radius) / (2.0 * radius)).floor().max(0.0) + 1.0;
        let model = lanes * 25.0 * v / (2.0 * radius);
        etas.push(number(r, "throughput") / model);
        by_radius
            .entry(radius.to_bits())
            .or_default()
            .entry(w.to_bits())
            .or_default()
            .push(number(r, "throughput"));
    }
    out.values.insert("eta_med", quantile(&etas, 0.5));
    out.values.insert("eta_p5", quantile(&etas, 0.05));
    let mut drops = 0.0;
    for widths in by_radius.values() {
        let mut ordered: Vec<(f64, &Vec<f64>)> = widths
            .iter()
            .map(|(w, v)| (f64::from_bits(*w), v))
            .collect();
        ordered.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in ordered.windows(2) {
            if median_drop_is_significant(pair[0].1, pair[1].1) {
                drops += 1.0;
            }
        }
    }
    out.values.insert("mono_drops", drops);
}

/// Whether the median of `wider` lies below that of `narrower` beyond its bootstrap interval:
/// the upper 97.5 % point of the resampled difference is negative (T6), stream seeded with 11.
fn median_drop_is_significant(narrower: &[f64], wider: &[f64]) -> bool {
    if narrower.is_empty() || wider.is_empty() {
        return false;
    }
    let mut rng = SplitMix64::new(11);
    let mut resample = |values: &[f64]| -> f64 {
        let sample: Vec<f64> = (0..values.len())
            .map(|_| values[usize::try_from(rng.below(values.len() as u64)).unwrap_or(0)])
            .collect();
        quantile(&sample, 0.5)
    };
    let mut diffs: Vec<f64> = (0..RESAMPLES)
        .map(|_| {
            let high = resample(wider);
            high - resample(narrower)
        })
        .collect();
    diffs.sort_by(f64::total_cmp);
    diffs[RESAMPLES * 975 / 1000] < 0.0
}

/// M8's shares and maxima.
fn overlap(all: &[&Row], out: &mut Aggregates) {
    let sum = |column: &str| all.iter().map(|r| number(r, column)).sum::<f64>();
    let max = |column: &str| {
        all.iter()
            .map(|r| number(r, column))
            .fold(0.0_f64, f64::max)
    };
    out.values.insert(
        "fo_move_share",
        100.0 * sum("fo_move_over") / sum("pair_ticks_move").max(1.0),
    );
    out.values.insert(
        "fo_rest_share",
        100.0 * sum("fo_rest_over") / sum("pair_ticks_rest").max(1.0),
    );
    out.values
        .insert("fo_max", max("fo_move_max").max(max("fo_rest_max")));
    out.values.insert("enemy_max", max("enemy_ov_max"));
    out.values.insert("terrain_max", max("terrain_pen_max"));
}

/// The mirror's blue wins, red wins and draws.
#[must_use]
pub fn mirror_counts(rows: &[Row]) -> (u32, u32, u32) {
    let (mut blue, mut red, mut draws) = (0, 0, 0);
    for row in rows {
        match row.get("winner").map(String::as_str) {
            Some("0") => blue += 1,
            Some("1") => red += 1,
            _ => draws += 1,
        }
    }
    (blue, red, draws)
}

/// The rows of each table: label, aggregate name, decimals.
const TABLE: [(&str, &str, usize); 22] = [
    ("M1 per 1,000 moving unit-ticks", "M1", 2),
    ("M1-hi", "M1hi", 2),
    ("M2 wasted share", "M2", 4),
    ("M3 standing waste per unit", "M3", 3),
    ("M4 progress violations %", "M4", 2),
    ("Deadlocked units (passable)", "deadlocks", 0),
    ("Give-ups", "giveups", 0),
    ("Scenarios complete %", "complete_scen", 1),
    ("Units complete %", "done_units", 2),
    ("Impassable: wrong passes", "impassable_passed", 0),
    ("η median", "eta_med", 3),
    ("η 5th percentile", "eta_p5", 3),
    ("Monotonicity drops", "mono_drops", 0),
    ("Fill mean", "fill_mean", 3),
    ("Fill 5th percentile", "fill_p5", 3),
    (
        "Friend overlap >5 % moving, % of near pair-ticks",
        "fo_move_share",
        2,
    ),
    ("Friend overlap >1 % standing, %", "fo_rest_share", 2),
    ("Friend overlap max", "fo_max", 3),
    ("Enemy overlap max", "enemy_max", 4),
    ("Terrain penetration max", "terrain_max", 5),
    ("Relabel sign test p (M1)", "rel_p1", 3),
    ("Relabel sign test p (completion tick)", "rel_p2", 3),
];

/// Every family's aggregates of `run`, with the relabelling tests as `rel_p1` and `rel_p2`.
#[must_use]
pub fn family_aggregates(run: &Run) -> BTreeMap<String, Aggregates> {
    let mut out = BTreeMap::new();
    for (family, rows) in &run.scenes {
        if rows.is_empty() {
            continue;
        }
        let mut aggregates = aggregate(family, rows);
        if let Some(relabelled) = run.relabelled.get(family).filter(|r| !r.is_empty()) {
            aggregates
                .values
                .insert("rel_p1", sign_test(rows, relabelled, "rev_amp").0);
            aggregates
                .values
                .insert("rel_p2", sign_test(rows, relabelled, "mean_done_tick").0);
        }
        out.insert(family.clone(), aggregates);
    }
    out
}

/// The tables of `runs` side by side, one per family, then the mirror's.
#[must_use]
pub fn render(runs: &[Run]) -> String {
    let all: Vec<BTreeMap<String, Aggregates>> = runs.iter().map(family_aggregates).collect();
    let mut text = String::new();
    for family in FAMILIES {
        if all.iter().all(|a| !a.contains_key(family)) {
            continue;
        }
        let _ = writeln!(text, "\n**{family}**\n");
        let labels: Vec<&str> = runs.iter().map(|r| r.label.as_str()).collect();
        let _ = writeln!(text, "| Metric | {} |", labels.join(" | "));
        let _ = writeln!(text, "|---|{}", "---:|".repeat(labels.len()));
        for (name, key, decimals) in TABLE {
            let cells: Vec<String> = all
                .iter()
                .map(|a| a.get(family).map_or(f64::NAN, |agg| agg.get(key)))
                .map(|v| format!("{v:.decimals$}"))
                .collect();
            if cells.iter().all(|c| c == "NaN") {
                continue;
            }
            let _ = writeln!(text, "| {name} | {} |", cells.join(" | "));
            if let Some(intervals) = interval_cells(&all, family, key) {
                let _ = writeln!(text, "| {name} 95 % CI | {} |", intervals.join(" | "));
            }
        }
    }
    let _ = writeln!(
        text,
        "\n**mirror** (blue / red / draws; blue share of decisive [95 % Wilson])\n"
    );
    let _ = writeln!(text, "| Run | Mirror |\n|---|---|");
    for run in runs.iter().filter(|r| !r.mirror.is_empty()) {
        let (blue, red, draws) = mirror_counts(&run.mirror);
        let (p, low, high) = wilson(blue, blue + red);
        let _ = writeln!(
            text,
            "| {} | {blue} / {red} / {draws}; {p:.3} [{low:.3}, {high:.3}] |",
            run.label
        );
    }
    text
}

/// The bootstrap intervals of `key` in every run's `family`, if it has them.
fn interval_cells(
    all: &[BTreeMap<String, Aggregates>],
    family: &str,
    key: &str,
) -> Option<Vec<String>> {
    if !matches!(key, "M1" | "M2") {
        return None;
    }
    Some(
        all.iter()
            .map(|a| {
                a.get(family)
                    .and_then(|agg| agg.intervals.get(key))
                    .map_or_else(|| "—".to_owned(), |(lo, hi)| format!("[{lo:.4}, {hi:.4}]"))
            })
            .collect(),
    )
}

/// One threshold's verdict on one family: `None` when the threshold does not apply to it.
fn verdict(
    threshold: &str,
    family: &str,
    new: &Aggregates,
    today: Option<&Aggregates>,
) -> Option<(bool, String)> {
    let not_above = |key: &str| -> bool {
        today.is_none_or(|t| {
            let ceiling = t
                .intervals
                .get(key)
                .map_or(t.get(key), |iv| iv.1.max(t.get(key)));
            new.get(key).partial_cmp(&ceiling) != Some(std::cmp::Ordering::Greater)
        })
    };
    let le = |key: &str, bound: f64| new.get(key) <= bound;
    let ge = |key: &str, bound: f64| new.get(key) >= bound;
    let walk = matches!(family, "gap" | "cross" | "corner");
    match threshold {
        "T1" => Some((
            le("M1", 1.0) && le("M1hi", 0.25) && not_above("M1"),
            format!("M1 {:.2}, M1-hi {:.2}", new.get("M1"), new.get("M1hi")),
        )),
        "T2" => Some((
            not_above("M2") && le("M3", 0.5),
            format!("M2 {:.4}, M3 {:.3}", new.get("M2"), new.get("M3")),
        )),
        "T3" => Some((le("M4", 1.0), format!("M4 {:.2} %", new.get("M4")))),
        "T4" if walk => Some((
            le("deadlocks", 0.0)
                && le("giveups", 0.0)
                && ge("complete_scen", 99.5)
                && (family != "gap" || le("impassable_passed", 0.0)),
            format!(
                "deadlocks {}, give-ups {}, complete {:.1} %",
                new.get("deadlocks"),
                new.get("giveups"),
                new.get("complete_scen")
            ),
        )),
        "T5" if family == "arc" => Some((
            ge("fill_mean", 0.90) && ge("fill_p5", 0.7476),
            format!(
                "fill {:.3}, 5th {:.3}",
                new.get("fill_mean"),
                new.get("fill_p5")
            ),
        )),
        "T6" if family == "gap" => Some((
            ge("eta_med", 0.5) && ge("eta_p5", 0.25) && le("mono_drops", 0.0),
            format!(
                "η {:.3}, 5th {:.3}, drops {}",
                new.get("eta_med"),
                new.get("eta_p5"),
                new.get("mono_drops")
            ),
        )),
        "T7" => Some((
            le("enemy_max", 0.005)
                && le("terrain_max", 1.0 / 1024.0)
                && le("fo_move_share", 0.1)
                && le("fo_rest_share", 0.1)
                && le("fo_max", 0.15),
            format!(
                "enemy {:.4}, terrain {:.5}, friends {:.2} % / {:.2} %, max {:.3}",
                new.get("enemy_max"),
                new.get("terrain_max"),
                new.get("fo_move_share"),
                new.get("fo_rest_share"),
                new.get("fo_max")
            ),
        )),
        "T8" => Some((
            ge("rel_p1", 0.01) && ge("rel_p2", 0.01),
            format!("p {:.3} / {:.3}", new.get("rel_p1"), new.get("rel_p2")),
        )),
        _ => None,
    }
}

/// The verdicts of T1 to T9 for every run after the first, judged against the first.
#[must_use]
pub fn verdicts(runs: &[Run]) -> String {
    let mut text = String::new();
    let Some((today, rest)) = runs.split_first() else {
        return text;
    };
    let base = family_aggregates(today);
    for run in rest {
        let new = family_aggregates(run);
        let _ = writeln!(
            text,
            "\n**Verdicts: {} against {}**\n",
            run.label, today.label
        );
        let _ = writeln!(
            text,
            "| Threshold | Family | Verdict | Values |\n|---|---|---|---|"
        );
        for threshold in ["T1", "T2", "T3", "T4", "T5", "T6", "T7", "T8"] {
            for (family, aggregates) in &new {
                if let Some((pass, values)) =
                    verdict(threshold, family, aggregates, base.get(family))
                {
                    let word = if pass { "holds" } else { "**fails**" };
                    let _ = writeln!(text, "| {threshold} | {family} | {word} | {values} |");
                }
            }
        }
        if !run.mirror.is_empty() {
            let (blue, red, draws) = mirror_counts(&run.mirror);
            let (p, low, high) = wilson(blue, blue + red);
            let word = if low <= 0.5 && 0.5 <= high {
                "holds"
            } else {
                "**fails**"
            };
            let _ = writeln!(
                text,
                "| T9 | mirror | {word} | {p:.3} [{low:.3}, {high:.3}], draws {draws} |"
            );
        }
    }
    text
}
