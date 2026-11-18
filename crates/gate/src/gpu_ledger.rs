//! The GPU run ledger of one gate run: where the GPU test harness writes it, and the table of the
//! slowest GPU children the result report prints.
//!
//! # Responsibility
//! The GPU test harness (`crates/client/tests/support/gpu_run.rs`) appends one tab-separated
//! line per GPU child to the file named by [`LEDGER_VAR`]: its label, driver, display, how long
//! it waited for the shared GPU lock (`lock_wait_s`), how long it ran (`run_s`), its exit and the
//! phases of its run. A child that waited out its lock wait without running is recorded with
//! `exit=lock-timeout`. This module
//! - decides the ledger of a run ([`GpuLedger::for_run`]): the file the variable already names,
//!   or `gpu-ledger.tsv` in the run's log directory (plan `2026-11-17-gpu-test-readback`, WP4);
//! - reads the lines this run appended ([`GpuLedger::children`]), parsing each into a
//!   [`GpuChild`];
//! - renders the five slowest children and the run's lock waits ([`render_slowest`]), which
//!   [`crate::RunReport::render`] prints after the result table, so GPU lane time and lock
//!   contention (B-136) are visible in every gate log.
//!
//! # How it fits
//! The gate depends on no workspace crate (charter §5), so it reads the ledger as text and
//! repeats the variable's name instead of importing it. The lane runner passes the path to every
//! test executable it starts ([`crate::LaneRun::gpu_ledger`]).
//!
//! # Invariants
//! - Only the lines appended during this run count: [`GpuLedger::start`] is the file's length
//!   when the run began, so an inherited ledger that already holds other runs adds nothing old.
//! - A run with no GPU child renders nothing.
//! - Seconds are kept as whole milliseconds, parsed exactly from the ledger's decimal text, so
//!   the order of the table never depends on float rounding.
//! - A ledger line that does not parse is counted and named in the table's heading, never
//!   dropped silently.

use crate::GateError;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};

/// The variable naming the GPU run ledger, read by the GPU test harness.
pub const LEDGER_VAR: &str = "SPACE_SCUFFLE_GPU_RUN_LEDGER";

/// The ledger's file name in the run's log directory when the variable is unset.
pub const LEDGER_FILE: &str = "gpu-ledger.tsv";

/// How many of the slowest children the report shows.
pub const SLOWEST_SHOWN: usize = 5;

/// The ledger's `exit=` value of a child that never ran because its lock wait expired.
pub const LOCK_TIMEOUT_EXIT: &str = "lock-timeout";

/// One GPU child, as one ledger line records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuChild {
    /// The label the test gave the run.
    pub label: String,
    /// The driver it rendered on (`radv`, `lavapipe`, or `none` for a lock self-test).
    pub driver: String,
    /// Its display (`offscreen`, `kwin`, `xvfb-sw`, `xvfb`, `host`, `none`).
    pub display: String,
    /// Its wait for the shared GPU lock in milliseconds; `None` for a child that takes no lock.
    pub lock_wait_ms: Option<u64>,
    /// Its run time in milliseconds, from spawn to exit; 0 for a child that never ran.
    pub run_ms: u64,
    /// Its exit code, `signal`, or [`LOCK_TIMEOUT_EXIT`].
    pub exit: String,
    /// Its longest phase and that phase's milliseconds, when the line records phases.
    pub slowest_phase: Option<(String, u64)>,
}

impl GpuChild {
    /// Parses one ledger line: tab-separated `key=value` fields. `None` when `label`, `driver`,
    /// `display`, `run_s` or `exit` is missing or a time is not a decimal number of seconds. A
    /// trailing line break is ignored.
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim_end_matches(['\n', '\r']);
        let field = |key: &str| {
            line.split('\t').find_map(|item| {
                item.split_once('=')
                    .filter(|(name, _)| *name == key)
                    .map(|(_, value)| value)
            })
        };
        let lock_wait_ms = match field("lock_wait_s") {
            None | Some("-") => None,
            Some(text) => Some(seconds_to_ms(text)?),
        };
        Some(Self {
            label: field("label")?.to_string(),
            driver: field("driver")?.to_string(),
            display: field("display")?.to_string(),
            lock_wait_ms,
            run_ms: seconds_to_ms(field("run_s")?)?,
            exit: field("exit")?.to_string(),
            slowest_phase: field("phases").and_then(slowest_phase),
        })
    }

    /// Whether the child waited out its lock wait and never ran.
    #[must_use]
    pub fn timed_out_waiting(&self) -> bool {
        self.exit == LOCK_TIMEOUT_EXIT
    }
}

/// Parses decimal seconds such as `12.34` or `7` into whole milliseconds, exactly; digits past
/// the third decimal are cut. `None` for anything else.
#[must_use]
pub fn seconds_to_ms(text: &str) -> Option<u64> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let millis: String = fraction
        .chars()
        .chain(std::iter::repeat('0'))
        .take(3)
        .collect();
    whole
        .parse::<u64>()
        .ok()?
        .checked_mul(1_000)?
        .checked_add(millis.parse::<u64>().ok()?)
}

/// The longest phase of a ledger `phases=` value (`launch 2.1 s, render device 6.0 s, ...`):
/// its name and milliseconds. The first of equal phases wins. `None` for `-` or no phase.
#[must_use]
pub fn slowest_phase(phases: &str) -> Option<(String, u64)> {
    let mut slowest: Option<(String, u64)> = None;
    for phase in phases.split(", ") {
        let Some((name, seconds)) = phase
            .strip_suffix(" s")
            .and_then(|rest| rest.rsplit_once(' '))
        else {
            continue;
        };
        let Some(ms) = seconds_to_ms(seconds) else {
            continue;
        };
        if slowest.as_ref().is_none_or(|(_, longest)| ms > *longest) {
            slowest = Some((name.to_string(), ms));
        }
    }
    slowest
}

/// The ledger of one gate run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuLedger {
    /// The ledger file.
    pub path: PathBuf,
    /// The file's length in bytes when the run began; this run's lines follow it.
    pub start: u64,
}

impl GpuLedger {
    /// The ledger of a run whose logs go to `log_dir`: the file `inherited` names (the value of
    /// [`LEDGER_VAR`] in the gate's own environment), or [`LEDGER_FILE`] in `log_dir`.
    #[must_use]
    pub fn for_run(inherited: Option<OsString>, log_dir: &Path) -> Self {
        let path = inherited
            .filter(|value| !value.is_empty())
            .map_or_else(|| log_dir.join(LEDGER_FILE), PathBuf::from);
        let start = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
        Self { path, start }
    }

    /// The children this run recorded, in the order they finished, and the number of lines that
    /// did not parse. A ledger that was never written (no GPU child ran) has none.
    ///
    /// # Errors
    /// [`GateError::Io`] when the file exists but cannot be read.
    pub fn children(&self) -> Result<(Vec<GpuChild>, usize), GateError> {
        let io = |source| GateError::Io {
            path: self.path.clone(),
            source,
        };
        let mut file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), 0))
            }
            Err(error) => return Err(io(error)),
        };
        file.seek(SeekFrom::Start(self.start)).map_err(io)?;
        let mut text = String::new();
        file.read_to_string(&mut text).map_err(io)?;
        Ok(parse_ledger(&text))
    }
}

/// Parses ledger text into children, in line order, and counts the non-empty lines that do not
/// parse.
#[must_use]
pub fn parse_ledger(text: &str) -> (Vec<GpuChild>, usize) {
    let mut children = Vec::new();
    let mut unreadable = 0;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        match GpuChild::parse(line) {
            Some(child) => children.push(child),
            None => unreadable += 1,
        }
    }
    (children, unreadable)
}

/// The GPU children of one gate run, for its report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuChildren {
    /// The ledger they were read from.
    pub ledger: PathBuf,
    /// Every child this run recorded, in the order they finished.
    pub children: Vec<GpuChild>,
    /// The number of this run's ledger lines that did not parse.
    pub unreadable: usize,
}

impl GpuChildren {
    /// Reads this run's children from `ledger`.
    ///
    /// # Errors
    /// As [`GpuLedger::children`].
    pub fn read(ledger: &GpuLedger) -> Result<Self, GateError> {
        let (children, unreadable) = ledger.children()?;
        Ok(Self {
            ledger: ledger.path.clone(),
            children,
            unreadable,
        })
    }

    /// The report's GPU block ([`render_slowest`]); empty when no GPU child ran.
    #[must_use]
    pub fn render(&self) -> String {
        render_slowest(&self.children, self.unreadable, &self.ledger)
    }
}

/// Milliseconds as seconds with one decimal.
fn secs(ms: u64) -> String {
    format!("{}.{}", ms / 1_000, (ms % 1_000) / 100)
}

/// The report's GPU block: the [`SLOWEST_SHOWN`] slowest children by run time (label, driver,
/// display, `run_s`, `lock_wait_s`, slowest phase), then one line on the run's lock waits.
/// Empty when `children` is empty and every line parsed.
#[must_use]
pub fn render_slowest(children: &[GpuChild], unreadable: usize, ledger: &Path) -> String {
    let mut out = String::new();
    if children.is_empty() && unreadable == 0 {
        return out;
    }
    let mut slowest: Vec<&GpuChild> = children.iter().collect();
    // Longest run first; equal runs by label, so two runs print the same table.
    slowest.sort_by(|a, b| b.run_ms.cmp(&a.run_ms).then_with(|| a.label.cmp(&b.label)));
    slowest.truncate(SLOWEST_SHOWN);
    let _ = writeln!(
        out,
        "\nGPU children, slowest first ({} of {}; ledger {}):",
        slowest.len(),
        children.len(),
        ledger.display()
    );
    if unreadable > 0 {
        let _ = writeln!(out, "  {unreadable} ledger line(s) could not be read");
    }
    if !slowest.is_empty() {
        let _ = writeln!(
            out,
            "  {:>8} {:>11}  {:<8} {:<9} {:<34} label",
            "run_s", "lock_wait_s", "driver", "display", "slowest phase"
        );
    }
    for child in slowest {
        let wait = child.lock_wait_ms.map_or_else(|| "-".to_string(), secs);
        let phase = child.slowest_phase.as_ref().map_or_else(
            || "-".to_string(),
            |(name, ms)| format!("{name} ({} s)", secs(*ms)),
        );
        let _ = writeln!(
            out,
            "  {:>8} {wait:>11}  {:<8} {:<9} {phase:<34} {}",
            secs(child.run_ms),
            child.driver,
            child.display,
            child.label
        );
    }
    out.push_str(&lock_line(children));
    out
}

/// One line on the run's lock waits: the total, the longest and who waited it, and the children
/// that waited out their lock wait. Empty when no child took the lock.
fn lock_line(children: &[GpuChild]) -> String {
    let locked: Vec<&GpuChild> = children
        .iter()
        .filter(|child| child.lock_wait_ms.is_some())
        .collect();
    let Some(longest) = locked.iter().max_by(|a, b| {
        a.lock_wait_ms
            .cmp(&b.lock_wait_ms)
            .then_with(|| b.label.cmp(&a.label))
    }) else {
        return String::new();
    };
    let total: u64 = locked.iter().filter_map(|child| child.lock_wait_ms).sum();
    let mut line = format!(
        "  GPU lock: {} child(ren) waited {} s in total, longest {} s ({})",
        locked.len(),
        secs(total),
        secs(longest.lock_wait_ms.unwrap_or_default()),
        longest.label
    );
    let starved: Vec<&str> = children
        .iter()
        .filter(|child| child.timed_out_waiting())
        .map(|child| child.label.as_str())
        .collect();
    if !starved.is_empty() {
        let _ = write!(
            line,
            "; {} waited out the lock wait and never ran: {}",
            starved.len(),
            starved.join(", ")
        );
    }
    line.push('\n');
    line
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// A ledger line as the harness writes it.
    fn line(label: &str, wait: &str, run: &str, exit: &str, phases: &str) -> String {
        format!(
            "label={label}\tdriver=radv\tdisplay=offscreen\tlock=shared\tadapter=AMD\t\
             driver_info=Mesa\tpreload_s=0.10\tlock_wait_s={wait}\trun_s={run}\texit={exit}\t\
             phases={phases}\n"
        )
    }

    #[test]
    fn seconds_are_parsed_exactly_into_milliseconds() {
        assert_eq!(seconds_to_ms("12.34"), Some(12_340));
        assert_eq!(seconds_to_ms("7"), Some(7_000));
        assert_eq!(seconds_to_ms("0.0049"), Some(4));
        assert_eq!(seconds_to_ms("1."), Some(1_000));
        assert_eq!(seconds_to_ms("-1.0"), None);
        assert_eq!(seconds_to_ms(".5"), None);
        assert_eq!(seconds_to_ms("1.2e3"), None);
        assert_eq!(seconds_to_ms(""), None);
    }

    #[test]
    fn the_slowest_phase_is_read_from_the_phases_field() {
        assert_eq!(
            slowest_phase("launch 2.1 s, render device 6.0 s, startup and frames 30.2 s, after the last log record 0.1 s"),
            Some(("startup and frames".to_string(), 30_200))
        );
        assert_eq!(
            slowest_phase("launch 1.0 s, app build 1.0 s"),
            Some(("launch".to_string(), 1_000))
        );
        assert_eq!(slowest_phase("-"), None);
        assert_eq!(slowest_phase(""), None);
    }

    #[test]
    fn a_ledger_line_parses_into_a_child() {
        let child = GpuChild::parse(&line(
            "lab",
            "3.40",
            "81.25",
            "0",
            "launch 2.0 s, startup and frames 79.2 s",
        ))
        .expect("a well-formed line parses");
        assert_eq!(child.label, "lab");
        assert_eq!(child.driver, "radv");
        assert_eq!(child.display, "offscreen");
        assert_eq!(child.lock_wait_ms, Some(3_400));
        assert_eq!(child.run_ms, 81_250);
        assert_eq!(child.exit, "0");
        assert_eq!(
            child.slowest_phase,
            Some(("startup and frames".to_string(), 79_200))
        );
        let lavapipe = GpuChild::parse(&line("soft", "-", "5.00", "0", "-")).expect("parses");
        assert_eq!(lavapipe.lock_wait_ms, None);
        assert_eq!(lavapipe.slowest_phase, None);
        assert!(GpuChild::parse("label=x\tdriver=radv").is_none());
        assert!(GpuChild::parse(&line("bad", "1.0", "fast", "0", "-")).is_none());
    }

    /// T-9: a fixture ledger prints the five slowest children in order, with their lock waits,
    /// and the lock line names the longest wait and a child that waited out the lock.
    #[test]
    fn the_five_slowest_children_are_printed_in_order() {
        let ledger: String = [
            line("a", "0.10", "10.00", "0", "launch 10.0 s"),
            line(
                "b",
                "45.20",
                "80.00",
                "0",
                "launch 1.0 s, startup and frames 79.0 s",
            ),
            line("c", "0.00", "30.00", "0", "-"),
            line("d", "1.00", "50.00", "0", "-"),
            line("e", "2.00", "20.00", "0", "-"),
            line("f", "3.00", "60.00", "0", "-"),
            line("starved", "290.00", "0.00", LOCK_TIMEOUT_EXIT, "-"),
        ]
        .concat();
        let (children, unreadable) = parse_ledger(&ledger);
        assert_eq!(unreadable, 0);
        let text = render_slowest(&children, unreadable, Path::new("/logs/gpu-ledger.tsv"));
        assert!(
            text.contains("GPU children, slowest first (5 of 7; ledger /logs/gpu-ledger.tsv):"),
            "{text}"
        );
        let rows: Vec<&str> = text
            .lines()
            .filter(|row| row.starts_with("  ") && row.trim_start().starts_with(char::is_numeric))
            .collect();
        let labels: Vec<&str> = rows
            .iter()
            .filter_map(|row| row.split_whitespace().last())
            .collect();
        assert_eq!(labels, ["b", "f", "d", "c", "e"], "{text}");
        assert!(
            rows[0].contains("80.0") && rows[0].contains("45.2"),
            "{text}"
        );
        assert!(rows[0].contains("startup and frames (79.0 s)"), "{text}");
        assert!(
            text.contains(
                "GPU lock: 7 child(ren) waited 341.3 s in total, longest 290.0 s (starved); \
                 1 waited out the lock wait and never ran: starved"
            ),
            "{text}"
        );
    }

    /// T-9: a run without GPU children prints nothing; unreadable lines are said, not dropped.
    #[test]
    fn a_run_without_gpu_children_prints_nothing() {
        assert_eq!(render_slowest(&[], 0, Path::new("x")), "");
        let (children, unreadable) = parse_ledger("garbage\n\n");
        assert_eq!((children.len(), unreadable), (0, 1));
        let text = render_slowest(&children, unreadable, Path::new("x"));
        assert!(
            text.contains("1 ledger line(s) could not be read"),
            "{text}"
        );
    }

    /// A child that took no lock (lavapipe) has no wait, and a run of only those has no lock line.
    #[test]
    fn children_without_the_lock_have_no_lock_line() {
        let (children, _) = parse_ledger(&line("soft", "-", "5.00", "0", "-"));
        let text = render_slowest(&children, 0, Path::new("x"));
        assert!(text.contains("soft"), "{text}");
        assert!(!text.contains("GPU lock:"), "{text}");
    }

    /// The gate's run ledger: the inherited file when the variable is set, else the log
    /// directory's; only lines appended after the run began are read; a missing file is empty.
    #[test]
    fn the_run_reads_only_its_own_ledger_lines() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let fresh = GpuLedger::for_run(None, directory.path());
        assert_eq!(fresh.path, directory.path().join(LEDGER_FILE));
        assert_eq!(fresh.start, 0);
        assert_eq!(fresh.children()?, (Vec::new(), 0));
        assert_eq!(
            GpuLedger::for_run(Some(OsString::new()), directory.path()).path,
            directory.path().join(LEDGER_FILE),
            "an empty variable counts as unset"
        );

        let own = directory.path().join("own.tsv");
        std::fs::write(&own, line("old", "1.00", "9.00", "0", "-"))?;
        let inherited = GpuLedger::for_run(Some(own.clone().into_os_string()), directory.path());
        assert_eq!(inherited.path, own);
        let mut file = std::fs::OpenOptions::new().append(true).open(&own)?;
        std::io::Write::write_all(&mut file, line("new", "2.00", "3.00", "0", "-").as_bytes())?;
        let (children, unreadable) = inherited.children()?;
        assert_eq!(unreadable, 0);
        let labels: Vec<&str> = children.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["new"]);
        Ok(())
    }
}
