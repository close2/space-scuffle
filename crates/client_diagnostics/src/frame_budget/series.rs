//! An interleaved series of `frame_budget` runs and its verdict on a loaded shared host (plan
//! `2026-11-19-retained-effect-instances` D-14).
//!
//! # Responsibility
//! Owns the `frame_budget_series` tool: it runs the arms (B, the shipped client; S, the splash
//! stream; P, the plan §10.3 stress) as child `frame_budget` processes in a balanced order
//! ([`balanced_order`]: B S P, then P S B, repeated), one `with-gpu-lock` command per child under
//! the WP14 recipe (a headless `KWin` pinned to the given CPUs), records the one-minute load at
//! each run's start and end, and writes a RON [`SeriesReport`] after every run with every run's
//! figures, the pairs of each arm against B ([`pair_statistics`]) and the verdict
//! ([`verdict`]).
//!
//! # How it fits
//! Wall time on this host varies by a factor of two between identical runs, while CPU time does
//! not include time spent waiting for a core. The verdict therefore takes the wall-time route
//! only with at least [`MIN_QUIET_PAIRS`] quiet pairs (load at most [`QUIET_LOAD`] at both runs'
//! starts and ends): the median over pairs of `p50(arm) / p50(B)` at most [`WALL_P50_BAR`] and
//! of the p99 ratio at most [`WALL_P99_BAR`] (plan `2026-11-09-effect-visualization` §10.3).
//! Otherwise, with at least [`MIN_CPU_PAIRS`] pairs, it judges the median over pairs of the
//! difference in process CPU time per frame against [`CPU_SHARE_BAR`] of B's wall p50 (the
//! series' median, or [`RECORDED_B_P50_MS`] if that is higher). The statistics are pure
//! functions, tested on fixed inputs; the runner only gathers their inputs.
//!
//! # Invariants
//! - The tool never holds the GPU lock between runs and refuses to start inside it.
//! - A run that fails is recorded as failed and leaves its pair out; it is never retried
//!   silently.
//! - The report file is rewritten after every run, so an interrupted series keeps its runs.

use super::report::FrameBudgetReport;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Highest one-minute load at which a run counts as quiet: this user's CPU count.
pub const QUIET_LOAD: f64 = 16.0;

/// Quiet pairs a wall-time verdict needs.
pub const MIN_QUIET_PAIRS: usize = 4;

/// Pairs a CPU-time verdict needs.
pub const MIN_CPU_PAIRS: usize = 6;

/// Largest median ratio of the p50 frame interval of an arm to B's (§10.3: +5 %).
pub const WALL_P50_BAR: f64 = 1.05;

/// Largest median ratio of the p99 frame interval of an arm to B's (§10.3: +10 %).
pub const WALL_P99_BAR: f64 = 1.10;

/// Share of B's p50 frame interval that an arm's added CPU time per frame may reach.
pub const CPU_SHARE_BAR: f64 = 0.05;

/// B's p50 frame interval recorded by WP14 (`frame_budget_wp14.md`), milliseconds: the floor of
/// the CPU-time bar's base.
pub const RECORDED_B_P50_MS: f64 = 11.86;

/// The lock file `with-gpu-lock` takes.
const GPU_LOCK_FILE: &str = "/run/lock/gpu.lock";

/// The arms the series knows, by name, with the `frame_budget` options each adds.
pub const ARMS: [(&str, &[&str]); 3] = [
    ("B", &[]),
    ("S", &["--effect-stress", "200"]),
    (
        "P",
        &[
            "--effect-stress",
            "200",
            "--effect-mix",
            "--mix-shots",
            "0",
            "--mix-deaths",
            "0",
        ],
    ),
];

/// One run of the order: the arm (an index into the series' arms) and its pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    /// Index into the series' arms; 0 is the base arm B.
    pub arm: usize,
    /// The pair the run belongs to, from 0.
    pub pair: u32,
}

/// The balanced order of `arms` arms over `pairs` pairs: every pair runs each arm once, the
/// even pairs in the given order and the odd pairs reversed (B S P, P S B, …), so a slow drift
/// of the host's load weighs on every arm alike.
#[must_use]
pub fn balanced_order(arms: usize, pairs: u32) -> Vec<Slot> {
    let mut order = Vec::with_capacity(arms * pairs as usize);
    for pair in 0..pairs {
        if pair % 2 == 0 {
            order.extend((0..arms).map(|arm| Slot { arm, pair }));
        } else {
            order.extend((0..arms).rev().map(|arm| Slot { arm, pair }));
        }
    }
    order
}

/// The figures of one finished run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    /// Arm name.
    pub arm: String,
    /// Pair index.
    pub pair: u32,
    /// One-minute load when the run started.
    pub load_start: f64,
    /// One-minute load when the run ended.
    pub load_end: f64,
    /// The child's report, or why the run failed.
    pub outcome: Result<RunFigures, String>,
}

/// What one run's report says, over its whole window, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RunFigures {
    /// Frames measured.
    pub frames: usize,
    /// p50 frame interval.
    pub interval_p50: f64,
    /// p99 frame interval.
    pub interval_p99: f64,
    /// p50 main-world update.
    pub update_p50: f64,
    /// Mean process CPU time per frame.
    pub cpu_mean: f64,
    /// p50 process CPU time per frame.
    pub cpu_p50: f64,
}

impl RunFigures {
    /// The figures of `report`'s whole window.
    #[must_use]
    pub fn of(report: &FrameBudgetReport) -> Self {
        let all = &report.summary.all;
        Self {
            frames: all.frames,
            interval_p50: all.interval_ms.p50,
            interval_p99: all.interval_ms.p99,
            update_p50: all.update_ms.p50,
            cpu_mean: all.process_cpu_ms.mean,
            cpu_p50: all.process_cpu_ms.p50,
        }
    }
}

impl RunRecord {
    /// Whether the load stayed at most [`QUIET_LOAD`] at the run's start and end.
    #[must_use]
    pub fn quiet(&self) -> bool {
        self.load_start <= QUIET_LOAD && self.load_end <= QUIET_LOAD
    }
}

/// One pair of an arm against B.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PairStat {
    /// Pair index.
    pub pair: u32,
    /// Both runs quiet.
    pub quiet: bool,
    /// `p50(arm) / p50(B)` of the frame interval.
    pub p50_ratio: f64,
    /// `p99(arm) / p99(B)` of the frame interval.
    pub p99_ratio: f64,
    /// Mean process CPU per frame of the arm less B's, milliseconds.
    pub cpu_diff: f64,
}

/// The pairs of arm `arm` against arm `base` in `runs`: every pair index where both ran and
/// both succeeded, in pair order.
#[must_use]
pub fn pair_statistics(runs: &[RunRecord], base: &str, arm: &str) -> Vec<PairStat> {
    let find = |name: &str, pair: u32| {
        runs.iter()
            .find(|run| run.arm == name && run.pair == pair)
            .and_then(|run| run.outcome.as_ref().ok().map(|figures| (run, *figures)))
    };
    let mut pairs: Vec<u32> = runs
        .iter()
        .filter(|run| run.arm == arm)
        .map(|run| run.pair)
        .collect();
    pairs.sort_unstable();
    pairs.dedup();
    pairs
        .into_iter()
        .filter_map(|pair| {
            let (base_run, base_figures) = find(base, pair)?;
            let (arm_run, arm_figures) = find(arm, pair)?;
            Some(PairStat {
                pair,
                quiet: base_run.quiet() && arm_run.quiet(),
                p50_ratio: arm_figures.interval_p50 / base_figures.interval_p50,
                p99_ratio: arm_figures.interval_p99 / base_figures.interval_p99,
                cpu_diff: arm_figures.cpu_mean - base_figures.cpu_mean,
            })
        })
        .collect()
}

/// The median of `values` (the mean of the middle two for an even count); `None` when empty.
#[must_use]
pub fn median(values: &[f64]) -> Option<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let count = sorted.len();
    match count {
        0 => None,
        _ if count % 2 == 1 => Some(sorted[count / 2]),
        _ => Some(f64::midpoint(sorted[count / 2 - 1], sorted[count / 2])),
    }
}

/// How an arm was judged.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Verdict {
    /// Judged on wall time over the quiet pairs.
    Wall {
        /// Quiet pairs judged.
        quiet_pairs: usize,
        /// Median `p50(arm) / p50(B)`.
        median_p50_ratio: f64,
        /// Median `p99(arm) / p99(B)`.
        median_p99_ratio: f64,
        /// Both medians within their bars.
        met: bool,
    },
    /// Judged on process CPU time per frame over every pair.
    Cpu {
        /// Pairs judged.
        pairs: usize,
        /// Quiet pairs among them (fewer than [`MIN_QUIET_PAIRS`]).
        quiet_pairs: usize,
        /// Median added CPU time per frame, milliseconds.
        median_cpu_diff: f64,
        /// The bar, [`CPU_SHARE_BAR`] of the base p50, milliseconds.
        bar: f64,
        /// The median within the bar.
        met: bool,
    },
    /// Too few pairs for either route.
    Insufficient {
        /// Pairs available.
        pairs: usize,
        /// Quiet pairs among them.
        quiet_pairs: usize,
    },
}

/// The verdict on `pairs` of an arm against B, whose runs had the wall p50s `base_p50s`
/// (milliseconds): wall time with at least [`MIN_QUIET_PAIRS`] quiet pairs, else CPU time with at
/// least [`MIN_CPU_PAIRS`] pairs, else [`Verdict::Insufficient`].
#[must_use]
pub fn verdict(pairs: &[PairStat], base_p50s: &[f64]) -> Verdict {
    let quiet: Vec<&PairStat> = pairs.iter().filter(|pair| pair.quiet).collect();
    let column = |rows: &[&PairStat], field: fn(&PairStat) -> f64| {
        median(&rows.iter().map(|pair| field(pair)).collect::<Vec<_>>())
    };
    if quiet.len() >= MIN_QUIET_PAIRS {
        let p50 = column(&quiet, |pair| pair.p50_ratio).unwrap_or(f64::INFINITY);
        let p99 = column(&quiet, |pair| pair.p99_ratio).unwrap_or(f64::INFINITY);
        return Verdict::Wall {
            quiet_pairs: quiet.len(),
            median_p50_ratio: p50,
            median_p99_ratio: p99,
            met: p50 <= WALL_P50_BAR && p99 <= WALL_P99_BAR,
        };
    }
    if pairs.len() >= MIN_CPU_PAIRS {
        let all: Vec<&PairStat> = pairs.iter().collect();
        let diff = column(&all, |pair| pair.cpu_diff).unwrap_or(f64::INFINITY);
        let base = median(base_p50s).map_or(RECORDED_B_P50_MS, |p50| p50.max(RECORDED_B_P50_MS));
        let bar = CPU_SHARE_BAR * base;
        return Verdict::Cpu {
            pairs: pairs.len(),
            quiet_pairs: quiet.len(),
            median_cpu_diff: diff,
            bar,
            met: diff <= bar,
        };
    }
    Verdict::Insufficient {
        pairs: pairs.len(),
        quiet_pairs: quiet.len(),
    }
}

/// One arm judged against B.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmComparison {
    /// Arm name.
    pub arm: String,
    /// Its pairs against B.
    pub pairs: Vec<PairStat>,
    /// The verdict.
    pub verdict: Verdict,
}

/// What a series wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesReport {
    /// Arms, base first.
    pub arms: Vec<String>,
    /// Pairs asked for.
    pub pairs_requested: u32,
    /// Options added to every child.
    pub extra: Vec<String>,
    /// Every run so far, in order.
    pub runs: Vec<RunRecord>,
    /// Median wall p50 of the base arm's successful runs, milliseconds.
    pub base_p50_median: Option<f64>,
    /// Each arm but the base against it.
    pub comparisons: Vec<ArmComparison>,
}

impl SeriesReport {
    /// Recomputes the comparisons from the runs.
    pub fn judge(&mut self) {
        let Some(base) = self.arms.first().cloned() else {
            return;
        };
        let base_p50s: Vec<f64> = self
            .runs
            .iter()
            .filter(|run| run.arm == base)
            .filter_map(|run| run.outcome.as_ref().ok().map(|f| f.interval_p50))
            .collect();
        self.base_p50_median = median(&base_p50s);
        self.comparisons = self
            .arms
            .iter()
            .skip(1)
            .map(|arm| {
                let pairs = pair_statistics(&self.runs, &base, arm);
                ArmComparison {
                    arm: arm.clone(),
                    verdict: verdict(&pairs, &base_p50s),
                    pairs,
                }
            })
            .collect();
    }
}

/// Command line of `frame_budget_series`.
#[derive(clap::Parser, Debug, Clone)]
#[command(name = "frame_budget_series")]
#[command(about = "Runs frame_budget arms interleaved and judges them on a loaded host")]
pub struct SeriesOptions {
    /// Arms, comma-separated, base first, from B, S and P.
    #[arg(long, value_delimiter = ',', default_value = "B,S,P")]
    pub arms: Vec<String>,
    /// Pairs: every arm runs once per pair.
    #[arg(long, default_value_t = 6)]
    pub pairs: u32,
    /// Display path of the children; only `kwin-virtual` (a headless `KWin` per run) is run.
    #[arg(long, value_enum)]
    pub display_path: super::display::DisplayPath,
    /// CPUs the children are pinned to, as `taskset -c` takes them.
    #[arg(long, default_value = "32-47")]
    pub cpus: String,
    /// The series report (RON); the children's reports and logs go beside it.
    #[arg(long)]
    pub out: PathBuf,
    /// The `frame_budget` binary; defaults to the one beside this binary.
    #[arg(long)]
    pub frame_budget: Option<PathBuf>,
    /// Scenario of every run.
    #[arg(long, default_value = "400")]
    pub scenario: String,
    /// Further options passed to every child, such as `--decal-source=cpu` (one per flag).
    #[arg(long = "extra", allow_hyphen_values = true)]
    pub extra: Vec<String>,
    /// Seconds a child may run, lock wait included, before it is stopped.
    #[arg(long, default_value_t = 1500)]
    pub child_timeout: u32,
}

/// Why a series could not run.
#[derive(Debug, thiserror::Error)]
pub enum SeriesError {
    /// An arm name is not B, S or P, or B is not first.
    #[error("unknown arm {0:?}, or the base arm B is not first (arms are B, S and P)")]
    Arm(String),
    /// The display path is not supported.
    #[error("the series runs its children under kwin-virtual only")]
    DisplayPath,
    /// The tool runs inside `with-gpu-lock`, which every child takes itself.
    #[error("frame_budget_series is running inside with-gpu-lock; run it without the lock")]
    InsideLock,
    /// The `frame_budget` binary was not found.
    #[error("no frame_budget binary at {0}")]
    MissingBinary(PathBuf),
    /// The report could not be written or a directory created.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The failure.
        #[source]
        source: std::io::Error,
    },
    /// The report could not be serialized.
    #[error("series report: {0}")]
    Serialize(#[from] ron::Error),
}

/// The one-minute load average.
fn load_average() -> f64 {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse().ok())
        .unwrap_or(f64::INFINITY)
}

/// Whether an ancestor of this process holds the GPU lock file open (as `with-gpu-lock`'s
/// shell does while its command runs).
fn inside_gpu_lock() -> bool {
    let lock = Path::new(GPU_LOCK_FILE);
    let mut pid = parent_of("self");
    while let Some(current) = pid.filter(|pid| *pid > 1) {
        let held = std::fs::read_dir(format!("/proc/{current}/fd")).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| std::fs::read_link(entry.path()).is_ok_and(|target| target == lock))
        });
        if held {
            return true;
        }
        pid = parent_of(&current.to_string());
    }
    false
}

/// The parent process id of `/proc/<process>`.
fn parent_of(process: &str) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{process}/stat")).ok()?;
    let after_name = stat.rsplit_once(") ")?.1;
    after_name.split_whitespace().nth(1)?.parse().ok()
}

/// Runs the series and returns its report, written to `options.out` after every run.
///
/// # Errors
/// [`SeriesError`] for a bad arm or display path, a run inside the lock, a missing binary, or a
/// report that cannot be written. A failed child is recorded in the report, not an error.
pub fn run_series(options: &SeriesOptions) -> Result<SeriesReport, SeriesError> {
    if options.display_path != super::display::DisplayPath::KwinVirtual {
        return Err(SeriesError::DisplayPath);
    }
    let arms: Vec<(&str, &[&str])> = options
        .arms
        .iter()
        .map(|name| {
            ARMS.iter()
                .find(|(known, _)| known == name)
                .copied()
                .ok_or_else(|| SeriesError::Arm(name.clone()))
        })
        .collect::<Result<_, _>>()?;
    if arms.first().map(|(name, _)| *name) != Some("B") {
        return Err(SeriesError::Arm(options.arms.join(",")));
    }
    if inside_gpu_lock() {
        return Err(SeriesError::InsideLock);
    }
    let binary = options.frame_budget.clone().unwrap_or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("frame_budget")))
            .unwrap_or_else(|| PathBuf::from("frame_budget"))
    });
    if !binary.is_file() {
        return Err(SeriesError::MissingBinary(binary));
    }
    let directory = options
        .out
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    std::fs::create_dir_all(&directory).map_err(|source| SeriesError::Io {
        path: directory.clone(),
        source,
    })?;
    let mut report = SeriesReport {
        arms: arms.iter().map(|(name, _)| (*name).to_owned()).collect(),
        pairs_requested: options.pairs,
        extra: options.extra.clone(),
        runs: Vec::new(),
        base_p50_median: None,
        comparisons: Vec::new(),
    };
    for slot in balanced_order(arms.len(), options.pairs) {
        let (name, arm_options) = arms[slot.arm];
        let label = format!("{name}-{:02}", slot.pair);
        let load_start = load_average();
        eprintln!("frame_budget_series: {label} starts at load {load_start:.1}");
        let outcome = run_child(options, &binary, &directory, &label, arm_options);
        let load_end = load_average();
        match &outcome {
            Ok(figures) => eprintln!(
                "frame_budget_series: {label} p50 {:.2} ms, p99 {:.2} ms, CPU {:.2} ms/frame, \
                 load {load_end:.1}",
                figures.interval_p50, figures.interval_p99, figures.cpu_mean
            ),
            Err(error) => eprintln!("frame_budget_series: {label} failed: {error}"),
        }
        report.runs.push(RunRecord {
            arm: name.to_owned(),
            pair: slot.pair,
            load_start,
            load_end,
            outcome,
        });
        report.judge();
        save(&report, &options.out)?;
    }
    Ok(report)
}

/// Runs one child under the WP14 recipe and reads its report.
fn run_child(
    options: &SeriesOptions,
    binary: &Path,
    directory: &Path,
    label: &str,
    arm_options: &[&str],
) -> Result<RunFigures, String> {
    let out = directory.join(format!("{label}.ron"));
    let log = directory.join(format!("{label}.log"));
    let work = directory.join(format!("work-{label}"));
    let mut session = format!(
        "'{}' --display-path kwin-virtual --work-dir '{}' --scenario {} --out '{}'",
        binary.display(),
        work.display(),
        options.scenario,
        out.display()
    );
    for option in arm_options
        .iter()
        .copied()
        .chain(options.extra.iter().map(String::as_str))
    {
        session.push(' ');
        session.push_str(option);
    }
    let log_file =
        std::fs::File::create(&log).map_err(|error| format!("{}: {error}", log.display()))?;
    let log_err = log_file
        .try_clone()
        .map_err(|error| format!("{}: {error}", log.display()))?;
    let status = Command::new("timeout")
        .arg(options.child_timeout.to_string())
        .arg("with-gpu-lock")
        .args(["taskset", "-c", &options.cpus])
        .args([
            "kwin_wayland",
            "--virtual",
            "--xwayland",
            "--no-lockscreen",
            "--no-global-shortcuts",
            "--width",
            "1280",
            "--height",
            "720",
            "--socket",
            &format!("ss-series-{label}"),
            "--exit-with-session",
            &session,
        ])
        .env("WGPU_BACKEND", "vulkan")
        .env(
            "VK_ICD_FILENAMES",
            "/usr/share/vulkan/icd.d/radeon_icd.json",
        )
        .env(
            "GPU_LOCK_TIMEOUT",
            std::env::var("GPU_LOCK_TIMEOUT").unwrap_or_else(|_| "1200".to_owned()),
        )
        .stdout(log_file)
        .stderr(log_err)
        .status()
        .map_err(|error| format!("could not start the child: {error}"))?;
    if !status.success() {
        return Err(format!(
            "the child exited with {status}; see {}",
            log.display()
        ));
    }
    FrameBudgetReport::load(&out)
        .map(|report| RunFigures::of(&report))
        .map_err(|error| error.to_string())
}

/// Writes `report` to `path` as RON.
fn save(report: &SeriesReport, path: &Path) -> Result<(), SeriesError> {
    let text = ron::ser::to_string_pretty(report, ron::ser::PrettyConfig::new())?;
    std::fs::write(path, text).map_err(|source| SeriesError::Io {
        path: path.to_owned(),
        source,
    })
}

/// A Markdown summary of `report`.
#[must_use]
pub fn markdown(report: &SeriesReport) -> String {
    use std::fmt::Write as _;
    let mut text = String::from(
        "| Run | Load (start → end) | Frames | p50 | p99 | Update p50 | CPU mean | CPU p50 |\n\
         |---|---|---:|---:|---:|---:|---:|---:|\n",
    );
    for run in &report.runs {
        let label = format!("{}-{:02}", run.arm, run.pair);
        let _ = match &run.outcome {
            Ok(f) => writeln!(
                text,
                "| {label} | {:.1} → {:.1} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |",
                run.load_start,
                run.load_end,
                f.frames,
                f.interval_p50,
                f.interval_p99,
                f.update_p50,
                f.cpu_mean,
                f.cpu_p50
            ),
            Err(error) => writeln!(
                text,
                "| {label} | {:.1} → {:.1} | failed: {error} | | | | | |",
                run.load_start, run.load_end
            ),
        };
    }
    for comparison in &report.comparisons {
        let _ = writeln!(
            text,
            "\n{} against B: {} pairs; verdict {:?}",
            comparison.arm,
            comparison.pairs.len(),
            comparison.verdict
        );
    }
    text
}
