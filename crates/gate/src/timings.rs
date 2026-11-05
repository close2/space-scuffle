//! `cargo gate timings`: what the recorded times and flakes say about the catalog.
//!
//! # Responsibility
//! Reads the wall times recorded in stamps and reports every `fast` target whose median over its
//! last ten runs exceeds 30 s, and every `slow` target whose median is under 10 s (blueprint
//! §4.4), so the classes follow the code rather than a table written once. It also reports every
//! target that flaked twice within a week ([`repeated_flakes`], blueprint §3.5), and words the
//! "time saved" line a selective tier prints against the last `full` stamp ([`time_saved`],
//! blueprint §8).
//!
//! # How it fits
//! The runner records each target's wall time and each flake in the stamp it writes; this module
//! only reads them. Changing a class is a catalog edit that a person makes after reading the
//! report; a repeated flake is a bug to fix.
//!
//! # Invariants
//! - Only the ten most recent runs of a target count, newest stamps last in the input.
//! - The class judged is the one recorded in the target's most recent run.
//! - `gpu` and `valgrind` targets are never reported: their class is about resources, not time.
//! - The saving is measured in test run time only (the sum of the targets' wall times); build,
//!   clippy and catalog commands are excluded on both sides.

use crate::catalog::TestClass;
use crate::stamp::Stamp;
use crate::tier::Tier;
use std::collections::BTreeMap;
use std::time::Duration;

/// A `fast` target above this median is reported, in milliseconds.
pub const FAST_LIMIT_MS: u64 = 30_000;
/// A `slow` target below this median is reported, in milliseconds.
pub const SLOW_FLOOR_MS: u64 = 10_000;
/// How many recent runs the median covers.
pub const WINDOW: usize = 10;

/// One target whose time contradicts its class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassFinding {
    /// The target, spelt as in the catalog.
    pub target: String,
    /// Its recorded class.
    pub class: TestClass,
    /// Its median wall time over the window, in milliseconds.
    pub median_ms: u64,
    /// How many runs the median covers.
    pub runs: usize,
}

/// The median of `values` (upper median for an even count); zero for none.
fn median(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    values.get(values.len() / 2).copied().unwrap_or(0)
}

/// Every target of `stamps` (oldest first) whose median time contradicts its class.
#[must_use]
pub fn class_report(stamps: &[Stamp]) -> Vec<ClassFinding> {
    let mut runs: BTreeMap<&str, (TestClass, Vec<u64>)> = BTreeMap::new();
    for stamp in stamps {
        for target in &stamp.targets {
            let entry = runs
                .entry(target.target.as_str())
                .or_insert((target.class, Vec::new()));
            entry.0 = target.class;
            entry.1.push(target.wall_ms);
        }
    }
    runs.into_iter()
        .filter_map(|(target, (class, times))| {
            let mut recent: Vec<u64> = times.iter().rev().take(WINDOW).copied().collect();
            let count = recent.len();
            let median_ms = median(&mut recent);
            let wrong = match class {
                TestClass::Fast => median_ms > FAST_LIMIT_MS,
                TestClass::Slow => median_ms < SLOW_FLOOR_MS,
                TestClass::Gpu | TestClass::Valgrind => false,
            };
            wrong.then(|| ClassFinding {
                target: target.to_string(),
                class,
                median_ms,
                runs: count,
            })
        })
        .collect()
}

/// The window in which repeated flakes of one target count, in seconds (one week).
pub const FLAKE_WINDOW_S: u64 = 7 * 24 * 60 * 60;
/// How many flakes of one target within [`FLAKE_WINDOW_S`] make it a bug to fix.
pub const FLAKE_LIMIT: usize = 2;

/// A target that flaked at least [`FLAKE_LIMIT`] times within [`FLAKE_WINDOW_S`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlakeFinding {
    /// The target, spelt as in the catalog.
    pub target: String,
    /// Its flakes within the window.
    pub count: usize,
}

/// Every target recorded as a flake at least [`FLAKE_LIMIT`] times in stamps finished within
/// [`FLAKE_WINDOW_S`] before `now_unix_s`, sorted by target.
#[must_use]
pub fn repeated_flakes(stamps: &[Stamp], now_unix_s: u64) -> Vec<FlakeFinding> {
    let since = now_unix_s.saturating_sub(FLAKE_WINDOW_S);
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for stamp in stamps.iter().filter(|s| s.finished_unix_s >= since) {
        for target in &stamp.flakes {
            *counts.entry(target.as_str()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count >= FLAKE_LIMIT)
        .map(|(target, count)| FlakeFinding {
            target: target.to_string(),
            count,
        })
        .collect()
}

/// Milliseconds as minutes with one decimal, for reports.
fn minutes(ms: u64) -> String {
    format!("{:.1} min", Duration::from_millis(ms).as_secs_f64() / 60.0)
}

/// The "time saved" line of a `quick`, `package` or `merge` run whose targets took `run_ms`
/// of wall time, against the newest `full` stamp among `stamps` (oldest first); `None` for a
/// `full` run, which is the reference itself.
#[must_use]
pub fn time_saved(tier: Tier, stamps: &[Stamp], run_ms: u64) -> Option<String> {
    if tier == Tier::Full {
        return None;
    }
    let Some(full) = stamps.iter().rev().find(|stamp| stamp.tier == Tier::Full) else {
        return Some(
            "Time saved: unknown, no `full` stamp to compare with yet (run `cargo gate full`)."
                .to_string(),
        );
    };
    let full_ms: u64 = full.targets.iter().map(|target| target.wall_ms).sum();
    let commit = full.commit.get(..12).unwrap_or(&full.commit);
    Some(format!(
        "Time saved: {} of test run time against the last full run ({} for {} targets at {commit}); \
         this run's {} targets took {}.",
        minutes(full_ms.saturating_sub(run_ms)),
        minutes(full_ms),
        full.targets.len(),
        tier,
        minutes(run_ms),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stamp::StampTarget;

    fn stamp(entries: &[(&str, TestClass, u64)]) -> Stamp {
        Stamp {
            tier: Tier::Full,
            tree: String::new(),
            commit: String::new(),
            base: None,
            host: String::new(),
            finished_unix_s: 0,
            targets: entries
                .iter()
                .map(|(target, class, ms)| StampTarget {
                    target: (*target).into(),
                    class: *class,
                    wall_ms: *ms,
                    cpu_ms: None,
                    passed: true,
                })
                .collect(),
            commands: Vec::new(),
            flakes: Vec::new(),
            retries: Vec::new(),
            reused: None,
        }
    }

    #[test]
    fn slow_fast_targets_and_quick_slow_targets_are_reported() {
        let mut stamps = Vec::new();
        for _ in 0..3 {
            stamps.push(stamp(&[
                ("a/fast_but_long", TestClass::Fast, 45_000),
                ("a/fine", TestClass::Fast, 2_000),
                ("a/slow_but_short", TestClass::Slow, 3_000),
                ("a/gpu", TestClass::Gpu, 1_000),
            ]));
        }
        let report = class_report(&stamps);
        let names: Vec<&str> = report.iter().map(|f| f.target.as_str()).collect();
        assert_eq!(names, ["a/fast_but_long", "a/slow_but_short"]);
        assert_eq!(report[0].median_ms, 45_000);
        assert_eq!(report[0].runs, 3);
    }

    #[test]
    fn two_flakes_of_a_target_in_a_week_are_reported() {
        let day = 24 * 60 * 60;
        let mut stamps: Vec<Stamp> = [(1, "a/x"), (5, "a/x"), (6, "a/y"), (20, "a/z")]
            .iter()
            .map(|(age_days, target)| {
                let mut s = stamp(&[]);
                s.finished_unix_s = 30 * day - age_days * day;
                s.flakes = vec![(*target).to_string()];
                s
            })
            .collect();
        stamps.push({
            let mut old = stamp(&[]);
            old.finished_unix_s = 10 * day;
            old.flakes = vec!["a/z".to_string()];
            old
        });
        assert_eq!(
            repeated_flakes(&stamps, 30 * day),
            vec![FlakeFinding {
                target: "a/x".into(),
                count: 2
            }]
        );
    }

    #[test]
    fn time_saved_compares_with_the_newest_full_stamp() {
        assert_eq!(time_saved(Tier::Full, &[], 1_000), None);
        let none = time_saved(Tier::Package, &[], 60_000).unwrap_or_default();
        assert!(none.contains("no `full` stamp"), "{none}");
        let mut old = stamp(&[("a/x", TestClass::Fast, 600_000)]);
        old.commit = "0123456789abcdef".into();
        let mut new = stamp(&[
            ("a/x", TestClass::Fast, 1_200_000),
            ("a/y", TestClass::Slow, 600_000),
        ]);
        new.commit = "fedcba9876543210".into();
        let mut package = stamp(&[("a/x", TestClass::Fast, 1)]);
        package.tier = Tier::Package;
        let line = time_saved(Tier::Package, &[old, new, package], 300_000).unwrap_or_default();
        assert_eq!(
            line,
            "Time saved: 25.0 min of test run time against the last full run (30.0 min for 2 \
             targets at fedcba987654); this run's package targets took 5.0 min."
        );
    }

    #[test]
    fn only_the_last_ten_runs_count() {
        let mut stamps: Vec<Stamp> = (0..20)
            .map(|_| stamp(&[("a/x", TestClass::Fast, 60_000)]))
            .collect();
        stamps.extend((0..10).map(|_| stamp(&[("a/x", TestClass::Fast, 1_000)])));
        assert_eq!(class_report(&stamps), Vec::new());
    }
}
