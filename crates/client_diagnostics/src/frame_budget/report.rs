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

use serde::{Deserialize, Serialize};
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
    /// `MESA_VK_WSI_DEBUG`, `VK_ICD_FILENAMES` and `WGPU_BACKEND` as the process saw them.
    pub graphics_environment: Vec<(String, String)>,
}

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
    /// Every measured frame, in order.
    pub frames: Vec<FrameSample>,
}

/// The report schema version this build writes and reads.
pub const REPORT_FORMAT_VERSION: u32 = 1;

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
        let report: Self = ron::from_str(&text).map_err(|source| ReportError::Parse {
            path: path.to_owned(),
            source,
        })?;
        if report.format_version != REPORT_FORMAT_VERSION {
            return Err(ReportError::Version(report.format_version));
        }
        Ok(report)
    }

    /// A Markdown summary of the run: environment, then one table row per window.
    #[must_use]
    pub fn markdown(&self) -> String {
        let environment = &self.environment;
        let mut text = format!(
            "## Frame budget: scenario {} ({} units)\n\n\
             - Commit: `{}`\n- Adapter: {} ({})\n- Resolution: {}x{}, present mode {}\n\
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
        text
    }
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
                environment: RunEnvironment::default(),
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
        Ok(())
    }
}
