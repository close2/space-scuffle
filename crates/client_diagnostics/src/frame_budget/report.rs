//! The frame-budget report: per-frame rows, the run's environment, and its statistics.
//!
//! # Responsibility
//! Owns the on-disk report of one `frame_budget` run ([`FrameBudgetReport`], RON), the statistics
//! computed from its rows ([`FrameStatistics`], nearest-rank percentiles), the selection of the
//! combat window, and the Markdown summary the tool prints.
//!
//! # How it fits
//! The recorder ([`super::recorder`]) fills [`FrameSample`] rows while the real client runs. When
//! the run ends, [`super::run`] hands them to [`FrameBudgetReport::new`] with the environment it
//! collected, writes the RON file and prints [`FrameBudgetReport::markdown`]. WP6 of the plan
//! `docs/plans/2026-11-03-close-phase-6-7-8-exits.md` takes the medians over runs of these
//! statistics.
//!
//! # Invariants
//! - Durations are stored per frame in whole microseconds and reported in milliseconds.
//! - Percentiles use the nearest-rank definition: the value at rank `ceil(p / 100 * n)` of the
//!   sorted sample, so every reported percentile is a value that was measured. An empty sample is
//!   an error, never a zero.
//! - The combat window starts at the first frame with a combat event and ends before the first
//!   frame whose tick is `combat_window_ticks` or more later.
//! - Format version 2 (`WP5b`) names the display path of every run; a version-1 report, which
//!   cannot say where it presented, is refused on load.
//! - GPU pass timings, when present, are sorted by path; their sum counts only passes that are
//!   not nested inside another recorded pass, and it covers instrumented passes only
//!   ([`GPU_PASSES_NOTE`]).

use super::display::DisplayPath;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// One measured frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameSample {
    /// Zero-based index of the frame within the measured window.
    pub frame: u32,
    /// Wall time since the previous frame's `First` schedule, in microseconds: what the player sees.
    pub interval_us: u64,
    /// Wall time from this frame's `First` schedule to its `Last` schedule, in microseconds: the
    /// main world's CPU work.
    pub update_us: u64,
    /// Wall time around the battle's tick driver (`BattleSet::AdvanceTicks`), in microseconds.
    pub simulation_us: u64,
    /// Simulation ticks the driver executed this frame.
    pub ticks_executed: u32,
    /// Simulation tick after this frame.
    pub tick: u32,
    /// Mobile units alive in the presented snapshot (dying units excluded).
    pub live_units: u32,
    /// Of those, the units whose centre projects inside the real camera's view.
    pub units_in_view: u32,
    /// Combat events the ticks of this frame published.
    pub combat_events: u32,
}

/// Where and how a run was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEnvironment {
    /// Output of `git rev-parse HEAD` in the tool's source tree, or why it is unavailable.
    pub commit: String,
    /// Adapter name Bevy rendered on.
    pub adapter: String,
    /// Driver name and driver information (the Mesa version on Mesa drivers).
    pub driver: String,
    /// Logical CPUs available to the process.
    pub cpus: usize,
    /// `/proc/loadavg` when the tool started.
    pub load_before: String,
    /// `/proc/loadavg` when the measured window ended.
    pub load_after: String,
    /// Primary window size in physical pixels, `(width, height)`.
    pub resolution: (u32, u32),
    /// Present mode of the primary window.
    pub present_mode: String,
    /// The display path the run declared, checked against the environment before it started.
    pub display_path: DisplayPath,
    /// `MESA_VK_WSI_DEBUG`, `VK_ICD_FILENAMES`, `WGPU_BACKEND`, `DISPLAY` and `WAYLAND_DISPLAY`
    /// as the process saw them (`"unset"` when unset).
    pub graphics_environment: Vec<(String, String)>,
}

/// What the Markdown summary says of GPU pass timings, every time it prints them.
pub const GPU_PASSES_NOTE: &str = "instrumented GPU passes only; the smog volume, egui, \
                                   post-processing and present are not timed";

/// Mean GPU time of one pass Bevy instruments with timestamp queries, over the measured window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuPassTiming {
    /// Bevy's diagnostic path, such as `render/main_opaque_pass_3d/elapsed_gpu`.
    pub path: String,
    /// Distinct readings taken in the window.
    pub samples: u32,
    /// Mean of those readings, in milliseconds.
    pub mean_ms: f64,
    /// Largest reading, in milliseconds.
    pub max_ms: f64,
}

impl GpuPassTiming {
    /// The span this timing belongs to: its path without the trailing `/elapsed_gpu`.
    fn span(&self) -> &str {
        self.path
            .strip_suffix("/elapsed_gpu")
            .unwrap_or(self.path.as_str())
    }

    /// Whether this pass is a span inside another pass of `passes`, so that adding it would
    /// count its time twice: either its path lies below the other's, or it is one of Bevy's
    /// [`DETACHED_SPANS`] and its container was recorded.
    fn is_nested_in(&self, passes: &[Self]) -> bool {
        let span = self.span();
        passes.iter().any(|outer| {
            let below = span
                .strip_prefix(outer.span())
                .is_some_and(|rest| rest.starts_with('/'));
            let detached = DETACHED_SPANS
                .iter()
                .any(|&(prefix, container)| span.starts_with(prefix) && outer.span() == container);
            below || detached
        })
    }
}

/// Render spans whose GPU work lies inside another span although their path is not below it:
/// `(prefix of the inner span, the containing span)`.
///
/// Bevy 0.15's shadow node (`bevy_pbr::render::light::ShadowPassNode`) opens the time span
/// `shadows` on its command encoder, then records each light's pass, such as `shadow pass
/// directional light 0 cascade 2`, on a command buffer of its own, built on another thread. The
/// recorder nests a span only under a span open on the same thread, so the cascade's path is
/// `render/shadow pass …`, not `render/shadows/shadow pass …`. Its command buffers are submitted
/// between the two timestamps of `shadows`, so its time is part of that span's. The first run on
/// the RX 580 (2026-11-03) confirmed it: `shadows` 0.511 ms, its cascades 0.369 ms together.
pub const DETACHED_SPANS: [(&str, &str); 1] = [("render/shadow pass ", "render/shadows")];

/// Nearest-rank percentiles of the frame interval, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct IntervalPercentiles {
    /// 50th percentile.
    pub p50: f64,
    /// 95th percentile.
    pub p95: f64,
    /// 99th percentile.
    pub p99: f64,
    /// Largest value.
    pub max: f64,
}

/// Nearest-rank 50th and 95th percentiles, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CentralPercentiles {
    /// 50th percentile.
    pub p50: f64,
    /// 95th percentile.
    pub p95: f64,
}

/// Statistics over a run of frames.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameStatistics {
    /// Frames the statistics cover.
    pub frames: usize,
    /// Simulation ticks executed over those frames.
    pub ticks: u64,
    /// Frame interval.
    pub interval_ms: IntervalPercentiles,
    /// Main-world update time.
    pub update_ms: CentralPercentiles,
    /// Simulation time.
    pub simulation_ms: CentralPercentiles,
}

/// Statistics over the whole window and over the combat window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReportSummary {
    /// Every measured frame.
    pub all: FrameStatistics,
    /// Frames of the combat window, or `None` if no combat event was measured.
    pub combat: Option<FrameStatistics>,
    /// Simulation tick of the first combat event, if any.
    pub first_combat_tick: Option<u32>,
}

/// The report of one run, as written to `--out`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameBudgetReport {
    /// Report schema version, [`REPORT_FORMAT_VERSION`].
    pub format_version: u32,
    /// Scenario name from the spec, such as `"400"`.
    pub scenario: String,
    /// Mobile units the scenario places.
    pub expected_units: u32,
    /// Frames rendered before measurement.
    pub warmup_frames: u32,
    /// The window's length: `"<n> ticks"` or `"<n> frames"`.
    pub window: String,
    /// Ticks of the combat window.
    pub combat_window_ticks: u32,
    /// Where and how the run was made.
    pub environment: RunEnvironment,
    /// Statistics.
    pub summary: ReportSummary,
    /// Per-pass GPU timings of a `--render-diagnostics` run, sorted by path; `None` otherwise.
    pub gpu_passes: Option<Vec<GpuPassTiming>>,
    /// Sum of the means of the passes not nested in another pass, in milliseconds; `None`
    /// without GPU timings. Instrumented passes only ([`GPU_PASSES_NOTE`]).
    pub gpu_pass_mean_sum_ms: Option<f64>,
    /// Every measured frame, in order.
    pub frames: Vec<FrameSample>,
}

/// The report schema version this build writes and reads. Version 2 added the display path
/// and the GPU pass timings (plan `WP5b`).
pub const REPORT_FORMAT_VERSION: u32 = 2;

/// The first field of any report, read before the rest so an old version is refused by its
/// number rather than by a parse error about a field it lacks.
#[derive(Deserialize)]
struct VersionProbe {
    format_version: u32,
}

/// Why statistics or a report could not be produced.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    /// A percentile of no values was requested.
    #[error("a percentile of an empty sample is undefined")]
    EmptySample,
    /// A percentile outside `1..=100` was requested.
    #[error("percentile {0} is outside 1..=100")]
    PercentileOutOfRange(u32),
    /// The report could not be read or written.
    #[error("frame-budget report {path}: {source}")]
    Io {
        /// The report file.
        path: PathBuf,
        /// The failure.
        #[source]
        source: std::io::Error,
    },
    /// The report could not be serialized.
    #[error("frame-budget report could not be serialized: {0}")]
    Serialize(#[from] ron::Error),
    /// The report could not be parsed.
    #[error("frame-budget report {path}: {source}")]
    Parse {
        /// The report file.
        path: PathBuf,
        /// The parse failure.
        #[source]
        source: ron::error::SpannedError,
    },
    /// The report has another schema version.
    #[error("frame-budget report has format version {0}, expected {REPORT_FORMAT_VERSION}")]
    Version(u32),
}

/// The nearest-rank `percentile` (1 to 100) of `values`, which need not be sorted.
///
/// # Errors
/// [`ReportError::EmptySample`] for no values, [`ReportError::PercentileOutOfRange`] for a
/// percentile outside `1..=100`.
pub fn nearest_rank(values: &[u64], percentile: u32) -> Result<u64, ReportError> {
    if !(1..=100).contains(&percentile) {
        return Err(ReportError::PercentileOutOfRange(percentile));
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    if count == 0 {
        return Err(ReportError::EmptySample);
    }
    // rank = ceil(p * n / 100), between 1 and n because 1 <= p <= 100 and n >= 1.
    let rank = (percentile as usize * count).div_ceil(100);
    Ok(sorted[rank - 1])
}

/// Frames `[start, end)` of the combat window of `frames`, or `None` without a combat event.
///
/// The window starts at the first frame with a combat event, at tick `t`, and holds every
/// following frame whose tick is below `t + window_ticks`.
#[must_use]
pub fn combat_window(frames: &[FrameSample], window_ticks: u32) -> Option<(usize, usize)> {
    let start = frames.iter().position(|frame| frame.combat_events > 0)?;
    let end_tick = u64::from(frames[start].tick) + u64::from(window_ticks);
    let length = frames[start..]
        .iter()
        .take_while(|frame| u64::from(frame.tick) < end_tick)
        .count();
    Some((start, start + length))
}

/// Microseconds to milliseconds.
#[expect(
    clippy::cast_precision_loss,
    reason = "a frame time in microseconds is far below 2^52"
)]
fn milliseconds(microseconds: u64) -> f64 {
    microseconds as f64 / 1000.0
}

impl FrameStatistics {
    /// Statistics of `frames`.
    ///
    /// # Errors
    /// [`ReportError::EmptySample`] if `frames` is empty.
    pub fn of(frames: &[FrameSample]) -> Result<Self, ReportError> {
        let column = |field: fn(&FrameSample) -> u64| frames.iter().map(field).collect::<Vec<_>>();
        let interval = column(|frame| frame.interval_us);
        let update = column(|frame| frame.update_us);
        let simulation = column(|frame| frame.simulation_us);
        let at = |values: &[u64], p| nearest_rank(values, p).map(milliseconds);
        Ok(Self {
            frames: frames.len(),
            ticks: frames
                .iter()
                .map(|frame| u64::from(frame.ticks_executed))
                .sum(),
            interval_ms: IntervalPercentiles {
                p50: at(&interval, 50)?,
                p95: at(&interval, 95)?,
                p99: at(&interval, 99)?,
                max: at(&interval, 100)?,
            },
            update_ms: CentralPercentiles {
                p50: at(&update, 50)?,
                p95: at(&update, 95)?,
            },
            simulation_ms: CentralPercentiles {
                p50: at(&simulation, 50)?,
                p95: at(&simulation, 95)?,
            },
        })
    }
}

/// What a report is made from, apart from its statistics.
#[derive(Debug, Clone)]
pub struct ReportInputs {
    /// Scenario name from the spec.
    pub scenario: String,
    /// Mobile units the scenario places.
    pub expected_units: u32,
    /// Frames rendered before measurement.
    pub warmup_frames: u32,
    /// The window's length, as text.
    pub window: String,
    /// Ticks of the combat window.
    pub combat_window_ticks: u32,
    /// Where and how the run was made.
    pub environment: RunEnvironment,
    /// Per-pass GPU timings, for a `--render-diagnostics` run.
    pub gpu_passes: Option<Vec<GpuPassTiming>>,
}

impl FrameBudgetReport {
    /// Builds the report of `frames`, computing its statistics.
    ///
    /// # Errors
    /// [`ReportError::EmptySample`] if `frames` is empty.
    pub fn new(inputs: ReportInputs, frames: Vec<FrameSample>) -> Result<Self, ReportError> {
        let all = FrameStatistics::of(&frames)?;
        let window = combat_window(&frames, inputs.combat_window_ticks);
        let combat = window
            .map(|(start, end)| FrameStatistics::of(&frames[start..end]))
            .transpose()?;
        let gpu_passes = inputs.gpu_passes.map(|mut passes| {
            passes.sort_by(|left, right| left.path.cmp(&right.path));
            passes
        });
        let gpu_pass_mean_sum_ms = gpu_passes.as_deref().map(|passes| {
            passes
                .iter()
                .filter(|pass| !pass.is_nested_in(passes))
                .map(|pass| pass.mean_ms)
                .sum()
        });
        Ok(Self {
            format_version: REPORT_FORMAT_VERSION,
            scenario: inputs.scenario,
            expected_units: inputs.expected_units,
            warmup_frames: inputs.warmup_frames,
            window: inputs.window,
            combat_window_ticks: inputs.combat_window_ticks,
            environment: inputs.environment,
            summary: ReportSummary {
                all,
                combat,
                first_combat_tick: window.map(|(start, _)| frames[start].tick),
            },
            gpu_passes,
            gpu_pass_mean_sum_ms,
            frames,
        })
    }

    /// Writes the report as RON to `path`.
    ///
    /// # Errors
    /// [`ReportError::Serialize`] or [`ReportError::Io`].
    pub fn save(&self, path: &Path) -> Result<(), ReportError> {
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::new())?;
        std::fs::write(path, text).map_err(|source| ReportError::Io {
            path: path.to_owned(),
            source,
        })
    }

    /// Reads a report written by [`Self::save`].
    ///
    /// # Errors
    /// [`ReportError::Io`], [`ReportError::Parse`] or [`ReportError::Version`].
    pub fn load(path: &Path) -> Result<Self, ReportError> {
        let text = std::fs::read_to_string(path).map_err(|source| ReportError::Io {
            path: path.to_owned(),
            source,
        })?;
        let parse_error = |source| ReportError::Parse {
            path: path.to_owned(),
            source,
        };
        let probe: VersionProbe = ron::from_str(&text).map_err(parse_error)?;
        if probe.format_version != REPORT_FORMAT_VERSION {
            return Err(ReportError::Version(probe.format_version));
        }
        ron::from_str(&text).map_err(parse_error)
    }

    /// A Markdown summary of the run: environment, then one table row per window.
    #[must_use]
    pub fn markdown(&self) -> String {
        let environment = &self.environment;
        let mut text = format!(
            "## Frame budget: scenario {} ({} units)\n\n\
             - Commit: `{}`\n- Adapter: {} ({})\n- Resolution: {}x{}, present mode {}\n\
             - Display path: `{}` ({})\n- Graphics environment: {}\n\
             - CPUs: {}; load before: {}; load after: {}\n\
             - Warm-up: {} frames; window: {}\n\n\
             | Window | Frames | Ticks | Interval p50 | p95 | p99 | max | Update p50 | p95 | \
             Simulation p50 | p95 |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
            self.scenario,
            self.expected_units,
            environment.commit,
            environment.adapter,
            environment.driver,
            environment.resolution.0,
            environment.resolution.1,
            environment.present_mode,
            environment.display_path,
            environment.display_path.description(),
            graphics_line(&environment.graphics_environment),
            environment.cpus,
            environment.load_before.trim(),
            environment.load_after.trim(),
            self.warmup_frames,
            self.window,
        );
        text.push_str(&table_row("whole", &self.summary.all));
        match (&self.summary.combat, self.summary.first_combat_tick) {
            (Some(combat), Some(tick)) => {
                text.push_str(&table_row(&format!("combat (from tick {tick})"), combat));
            }
            _ => text.push_str("\nNo combat event was measured.\n"),
        }
        if let (Some(passes), Some(sum)) = (&self.gpu_passes, self.gpu_pass_mean_sum_ms) {
            text.push_str(&gpu_table(passes, sum));
        }
        text
    }
}

/// `NAME=value` pairs of the recorded graphics environment, comma-separated.
fn graphics_line(environment: &[(String, String)]) -> String {
    environment
        .iter()
        .map(|(name, value)| format!("`{name}={value}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The Markdown table of GPU pass timings and their sum, with [`GPU_PASSES_NOTE`].
fn gpu_table(passes: &[GpuPassTiming], sum_ms: f64) -> String {
    let mut text = format!(
        "\n### GPU passes ({GPU_PASSES_NOTE})\n\n| Pass | Samples | Mean ms | Max ms |\n\
         |---|---:|---:|---:|\n"
    );
    for pass in passes {
        let nested = if pass.is_nested_in(passes) {
            " (nested)"
        } else {
            ""
        };
        // Writing into a String cannot fail.
        let _ = writeln!(
            text,
            "| {}{nested} | {} | {:.3} | {:.3} |",
            pass.path, pass.samples, pass.mean_ms, pass.max_ms
        );
    }
    let _ = writeln!(
        text,
        "\nSum of the means of the passes not nested in another: {sum_ms:.3} ms ({GPU_PASSES_NOTE})."
    );
    text
}

/// One Markdown table row of `statistics`, times in milliseconds.
fn table_row(label: &str, statistics: &FrameStatistics) -> String {
    format!(
        "| {label} | {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |\n",
        statistics.frames,
        statistics.ticks,
        statistics.interval_ms.p50,
        statistics.interval_ms.p95,
        statistics.interval_ms.p99,
        statistics.interval_ms.max,
        statistics.update_ms.p50,
        statistics.update_ms.p95,
        statistics.simulation_ms.p50,
        statistics.simulation_ms.p95,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(frame: u32, tick: u32, combat_events: u32) -> FrameSample {
        FrameSample {
            frame,
            interval_us: u64::from(frame + 1) * 1000,
            update_us: 500,
            simulation_us: 100,
            ticks_executed: 1,
            tick,
            live_units: 10,
            units_in_view: 10,
            combat_events,
        }
    }

    fn inputs() -> ReportInputs {
        ReportInputs {
            scenario: "0".into(),
            expected_units: 0,
            warmup_frames: 0,
            window: "2 frames".into(),
            combat_window_ticks: 3,
            environment: RunEnvironment {
                commit: "abc".into(),
                adapter: "test adapter".into(),
                driver: "test driver".into(),
                cpus: 16,
                load_before: "1.00 1.00 1.00 1/100 1".into(),
                load_after: "1.00 1.00 1.00 1/100 1".into(),
                resolution: (1280, 720),
                present_mode: "AutoNoVsync".into(),
                display_path: DisplayPath::XvfbSoftwarePresent,
                graphics_environment: vec![("DISPLAY".into(), ":99".into())],
            },
            gpu_passes: None,
        }
    }

    #[test]
    fn nearest_rank_matches_known_vectors() -> Result<(), ReportError> {
        // The textbook example: ranks ceil(0.05 * 5) = 1, ceil(0.3 * 5) = 2, ceil(0.4 * 5) = 2,
        // ceil(0.5 * 5) = 3 and ceil(1.0 * 5) = 5.
        let values = [35, 20, 15, 50, 40];
        assert_eq!(nearest_rank(&values, 5)?, 15);
        assert_eq!(nearest_rank(&values, 30)?, 20);
        assert_eq!(nearest_rank(&values, 40)?, 20);
        assert_eq!(nearest_rank(&values, 50)?, 35);
        assert_eq!(nearest_rank(&values, 100)?, 50);
        let hundred: Vec<u64> = (1..=100).rev().collect();
        assert_eq!(nearest_rank(&hundred, 95)?, 95);
        assert_eq!(nearest_rank(&hundred, 99)?, 99);
        assert_eq!(nearest_rank(&hundred, 1)?, 1);
        Ok(())
    }

    #[test]
    fn a_single_sample_is_every_percentile() -> Result<(), ReportError> {
        for percentile in [1, 50, 95, 99, 100] {
            assert_eq!(nearest_rank(&[7], percentile)?, 7);
        }
        Ok(())
    }

    #[test]
    fn an_empty_sample_or_a_bad_percentile_is_an_error() {
        assert!(matches!(
            nearest_rank(&[], 50),
            Err(ReportError::EmptySample)
        ));
        assert!(matches!(
            nearest_rank(&[1], 0),
            Err(ReportError::PercentileOutOfRange(0))
        ));
        assert!(matches!(
            nearest_rank(&[1], 101),
            Err(ReportError::PercentileOutOfRange(101))
        ));
        assert!(matches!(
            FrameStatistics::of(&[]),
            Err(ReportError::EmptySample)
        ));
    }

    #[test]
    fn the_combat_window_starts_at_the_first_event_and_spans_its_ticks() {
        let frames: Vec<FrameSample> = (0..20)
            .map(|index| sample(index, 100 + index, u32::from(index == 4 || index == 9)))
            .collect();
        // First event at frame 4, tick 104; ticks 104..=108 are below 104 + 5.
        assert_eq!(combat_window(&frames, 5), Some((4, 9)));
        // A window longer than the run ends with the run.
        assert_eq!(combat_window(&frames, 1000), Some((4, 20)));
        let quiet: Vec<FrameSample> = (0..5).map(|index| sample(index, index, 0)).collect();
        assert_eq!(combat_window(&quiet, 5), None);
    }

    #[test]
    fn a_report_round_trips_and_summarises_both_windows() -> Result<(), ReportError> {
        let frames: Vec<FrameSample> = (0..10)
            .map(|index| sample(index, index, u32::from(index >= 5)))
            .collect();
        let report = FrameBudgetReport::new(
            ReportInputs {
                scenario: "400".into(),
                expected_units: 400,
                warmup_frames: 180,
                window: "10 frames".into(),
                combat_window_ticks: 3,
                ..inputs()
            },
            frames,
        )?;
        assert_eq!(report.summary.all.frames, 10);
        assert_eq!(report.summary.all.ticks, 10);
        assert!((report.summary.all.interval_ms.max - 10.0).abs() < f64::EPSILON);
        assert!((report.summary.all.interval_ms.p50 - 5.0).abs() < f64::EPSILON);
        assert_eq!(report.summary.first_combat_tick, Some(5));
        assert_eq!(report.summary.combat.map(|combat| combat.frames), Some(3));
        let directory = tempfile::tempdir().map_err(|source| ReportError::Io {
            path: PathBuf::new(),
            source,
        })?;
        let path = directory.path().join("report.ron");
        report.save(&path)?;
        assert_eq!(FrameBudgetReport::load(&path)?, report);
        assert!(report.markdown().contains("| whole | 10 | 10 |"));
        assert!(report
            .markdown()
            .contains("- Display path: `xvfb-software-present`"));
        assert!(!report.markdown().contains(GPU_PASSES_NOTE));
        Ok(())
    }

    fn pass(path: &str, mean_ms: f64) -> GpuPassTiming {
        GpuPassTiming {
            path: path.into(),
            samples: 4,
            mean_ms,
            max_ms: mean_ms * 2.0,
        }
    }

    #[test]
    fn a_report_with_gpu_passes_round_trips_at_version_2() -> Result<(), Box<dyn std::error::Error>>
    {
        let frames: Vec<FrameSample> = (0..4).map(|index| sample(index, index, 0)).collect();
        let mut inputs = inputs();
        inputs.environment.display_path = DisplayPath::KwinVirtual;
        inputs.gpu_passes = Some(vec![
            pass("render/shadows/elapsed_gpu", 0.5),
            pass("render/main_opaque_pass_3d/elapsed_gpu", 2.0),
            pass("render/shadows/cascade_2/elapsed_gpu", 0.25),
            pass(
                "render/shadow pass directional light 0 cascade 1/elapsed_gpu",
                0.125,
            ),
        ]);
        let report = FrameBudgetReport::new(inputs, frames)?;
        assert_eq!(report.format_version, 2);
        let paths: Vec<&str> = report
            .gpu_passes
            .iter()
            .flatten()
            .map(|pass| pass.path.as_str())
            .collect();
        assert_eq!(
            paths,
            [
                "render/main_opaque_pass_3d/elapsed_gpu",
                "render/shadow pass directional light 0 cascade 1/elapsed_gpu",
                "render/shadows/cascade_2/elapsed_gpu",
                "render/shadows/elapsed_gpu",
            ],
            "passes are sorted by path"
        );
        // Neither a span below the shadow pass's path nor a cascade Bevy records on its own
        // command buffer (DETACHED_SPANS) is added a second time.
        assert_eq!(report.gpu_pass_mean_sum_ms, Some(2.5));

        let directory = tempfile::tempdir()?;
        let path = directory.path().join("report.ron");
        report.save(&path)?;
        assert_eq!(FrameBudgetReport::load(&path)?, report);
        let markdown = report.markdown();
        assert!(
            markdown.contains("- Display path: `kwin-virtual`"),
            "{markdown}"
        );
        assert!(markdown.contains(GPU_PASSES_NOTE), "{markdown}");
        assert!(
            markdown.contains("| render/main_opaque_pass_3d/elapsed_gpu | 4 | 2.000 | 4.000 |"),
            "{markdown}"
        );
        assert!(
            markdown.contains("| render/shadows/cascade_2/elapsed_gpu (nested) |"),
            "{markdown}"
        );
        assert!(markdown.contains("2.500 ms"), "{markdown}");
        Ok(())
    }

    #[test]
    fn a_detached_span_counts_when_its_container_is_missing() -> Result<(), ReportError> {
        let frames: Vec<FrameSample> = (0..2).map(|index| sample(index, index, 0)).collect();
        let mut inputs = inputs();
        inputs.gpu_passes = Some(vec![
            pass("render/main_opaque_pass_3d/elapsed_gpu", 2.0),
            pass(
                "render/shadow pass directional light 0 cascade 1/elapsed_gpu",
                0.125,
            ),
        ]);
        let report = FrameBudgetReport::new(inputs, frames)?;
        assert_eq!(report.gpu_pass_mean_sum_ms, Some(2.125));
        Ok(())
    }

    #[test]
    fn a_version_1_report_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let frames: Vec<FrameSample> = (0..2).map(|index| sample(index, index, 0)).collect();
        let report = FrameBudgetReport::new(inputs(), frames)?;
        let text = ron::ser::to_string_pretty(&report, ron::ser::PrettyConfig::new())?;
        // WP5's reports had version 1 and no display path or GPU passes.
        let version_1 = text
            .replace("format_version: 2", "format_version: 1")
            .lines()
            .filter(|line| !line.contains("display_path") && !line.contains("gpu_pass"))
            .collect::<Vec<_>>()
            .join("\n");
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("version-1.ron");
        std::fs::write(&path, version_1)?;
        let result = FrameBudgetReport::load(&path);
        assert!(matches!(result, Err(ReportError::Version(1))), "{result:?}");
        Ok(())
    }
}
