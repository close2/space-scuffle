//! `cargo gate timings`: targets whose measured run time contradicts their class.
//!
//! # Responsibility
//! Reads the wall times recorded in stamps and reports every `fast` target whose median over its
//! last ten runs exceeds 30 s, and every `slow` target whose median is under 10 s (blueprint
//! §4.4), so the classes follow the code rather than a table written once.
//!
//! # How it fits
//! The runner records each target's wall time in the stamp it writes; this module only reads
//! them. Changing a class is a catalog edit that a person makes after reading the report.
//!
//! # Invariants
//! - Only the ten most recent runs of a target count, newest stamps last in the input.
//! - The class judged is the one recorded in the target's most recent run.
//! - `gpu` and `valgrind` targets are never reported: their class is about resources, not time.

use crate::catalog::TestClass;
use crate::stamp::Stamp;
use std::collections::BTreeMap;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stamp::StampTarget;
    use crate::tier::Tier;

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
    fn only_the_last_ten_runs_count() {
        let mut stamps: Vec<Stamp> = (0..20)
            .map(|_| stamp(&[("a/x", TestClass::Fast, 60_000)]))
            .collect();
        stamps.extend((0..10).map(|_| stamp(&[("a/x", TestClass::Fast, 1_000)])));
        assert_eq!(class_report(&stamps), Vec::new());
    }
}
