//! The compiler-crash retry: a build or clippy step that failed only because rustc panicked is
//! run once more on a cleared incremental cache.
//!
//! # Responsibility
//! Recognises a compiler crash in a step's log ([`compiler_crashed`]), names the incremental
//! caches a Cargo invocation used ([`incremental_dirs`]), and runs a step with at most one retry
//! ([`run_with_crash_retry`]): when the first attempt fails and its log contains
//! [`COMPILER_PANIC`], the log is kept beside the step's as `<name>.crash.log`, the caches are
//! deleted, and the step runs again (blueprint `docs/plans/2026-11-04-tiered-test-gates.md`,
//! Amendment 1, decision D5).
//!
//! # How it fits
//! `runner.rs` wraps every fixed Cargo step (clippy, the test build, doc tests, catalog
//! commands) in [`run_with_crash_retry`] and records [`StepRun::retried`] in the result table
//! and the stamp. Rustc 1.99 panicked on corrupted incremental caches in target directories that
//! consecutive gates share; deleting `debug/incremental` and rerunning was green each time.
//!
//! # Invariants
//! - A step runs at most twice. A second crash, or any failure without the crash text, is
//!   returned as it is: the retry never turns a real failure green.
//! - Only incremental caches are deleted, never compiled dependencies, so the retry costs a
//!   rebuild of the workspace crates, not of the whole dependency tree.
//! - Incremental compilation stays on: the warm rebuild is what keeps `quick` and `package`
//!   fast (D5).

use crate::GateError;
use std::path::{Path, PathBuf};

/// The text rustc prints when it panics: `error: the compiler unexpectedly panicked. This is a
/// bug`, the same for rustc and clippy-driver.
pub const COMPILER_PANIC: &str = "the compiler unexpectedly panicked";

/// Whether the log at `log` records a compiler crash. A missing or unreadable log records none.
#[must_use]
pub fn compiler_crashed(log: &Path) -> bool {
    std::fs::read(log).is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(COMPILER_PANIC))
}

/// The value of `--target` in Cargo arguments `args`, before any `--` (whose tail belongs to the
/// compiler, as in `clippy ... -- -D warnings`).
fn target_triple(args: &[String]) -> Option<&str> {
    let mut words = args.iter().take_while(|arg| *arg != "--");
    while let Some(arg) = words.next() {
        if arg == "--target" {
            return words.next().map(String::as_str);
        }
        if let Some(triple) = arg.strip_prefix("--target=") {
            return Some(triple);
        }
    }
    None
}

/// The incremental caches a Cargo invocation with arguments `args` may have used, under
/// `target_dir`: always `debug/incremental` (clippy, the test build and doc tests use the dev
/// profile), plus `release/incremental` for `--release`; with `--target <triple>`, the same
/// directories under `<triple>/` as well, where Cargo puts cross-compiled crates.
#[must_use]
pub fn incremental_dirs(target_dir: &Path, args: &[String]) -> Vec<PathBuf> {
    let cargo_args = args.iter().take_while(|arg| *arg != "--");
    let release = cargo_args
        .clone()
        .any(|arg| arg == "--release" || arg == "-r");
    let mut profiles = vec!["debug"];
    if release {
        profiles.push("release");
    }
    let mut roots = vec![target_dir.to_path_buf()];
    if let Some(triple) = target_triple(args) {
        roots.push(target_dir.join(triple));
    }
    roots
        .iter()
        .flat_map(|root| {
            profiles
                .iter()
                .map(move |profile| root.join(profile).join("incremental"))
        })
        .collect()
}

/// How a step ran under [`run_with_crash_retry`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepRun<T> {
    /// The value of the last attempt.
    pub value: T,
    /// Whether the first attempt crashed the compiler and the step ran a second time.
    pub retried: bool,
}

/// The file the crashed first attempt's log is kept in: `clippy.log` becomes `clippy.crash.log`.
#[must_use]
pub fn crash_log(log: &Path) -> PathBuf {
    let stem = log
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    log.with_file_name(format!("{stem}.crash.log"))
}

/// Runs `attempt`, which writes its output to `log`; if `succeeded` rejects its value and the
/// log contains [`COMPILER_PANIC`], keeps the log as [`crash_log`], deletes every directory of
/// `caches`, and runs `attempt` once more. Returns the last attempt's value.
///
/// # Errors
/// Whatever `attempt` returns; [`GateError::Io`] if the log cannot be moved or a cache cannot be
/// deleted.
pub fn run_with_crash_retry<T>(
    log: &Path,
    caches: &[PathBuf],
    mut attempt: impl FnMut() -> Result<T, GateError>,
    succeeded: impl Fn(&T) -> bool,
) -> Result<StepRun<T>, GateError> {
    let value = attempt()?;
    if succeeded(&value) || !compiler_crashed(log) {
        return Ok(StepRun {
            value,
            retried: false,
        });
    }
    let kept = crash_log(log);
    std::fs::rename(log, &kept).map_err(|source| GateError::Io {
        path: log.to_path_buf(),
        source,
    })?;
    for cache in caches {
        match std::fs::remove_dir_all(cache) {
            Ok(()) => println!("  deleted {}", cache.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(GateError::Io {
                    path: cache.clone(),
                    source,
                })
            }
        }
    }
    println!(
        "  the compiler crashed (log kept as {}); retrying once on a cleared incremental cache",
        kept.display()
    );
    Ok(StepRun {
        value: attempt()?,
        retried: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn io(path: &Path) -> impl FnOnce(std::io::Error) -> GateError + '_ {
        move |source| GateError::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(String::from).collect()
    }

    const CRASH: &str =
        "thread 'rustc' panicked at compiler/rustc_query_system/src/dep_graph/graph.rs\n\
                         error: the compiler unexpectedly panicked. This is a bug\n";

    /// A fake step: writes `outputs[n]` to `log` on its `n`-th attempt and passes when the text
    /// is `ok`.
    struct FakeStep<'a> {
        log: &'a Path,
        outputs: &'a [&'a str],
        calls: Cell<usize>,
    }

    impl FakeStep<'_> {
        fn attempt(&self) -> Result<bool, GateError> {
            let n = self.calls.get();
            self.calls.set(n + 1);
            let text = self.outputs.get(n).copied().unwrap_or("ok");
            std::fs::write(self.log, text).map_err(io(self.log))?;
            Ok(text == "ok")
        }
    }

    /// A target directory with a populated `debug/incremental` and a compiled dependency.
    fn target_dir(root: &Path) -> Result<(PathBuf, PathBuf), GateError> {
        let incremental = root.join("debug/incremental/client-1abc");
        std::fs::create_dir_all(&incremental).map_err(io(&incremental))?;
        let deps = root.join("debug/deps");
        std::fs::create_dir_all(&deps).map_err(io(&deps))?;
        Ok((root.join("debug/incremental"), deps))
    }

    #[test]
    fn a_crash_is_retried_once_on_a_cleared_cache_and_can_turn_green() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(io(Path::new("tempdir")))?;
        let (incremental, deps) = target_dir(dir.path())?;
        let log = dir.path().join("clippy.log");
        let step = FakeStep {
            log: &log,
            outputs: &[CRASH, "ok"],
            calls: Cell::new(0),
        };
        let caches = incremental_dirs(dir.path(), &args("clippy --workspace -- -D warnings"));
        let run = run_with_crash_retry(&log, &caches, || step.attempt(), |passed| *passed)?;
        assert_eq!(
            run,
            StepRun {
                value: true,
                retried: true
            }
        );
        assert_eq!(step.calls.get(), 2);
        assert!(!incremental.exists(), "the incremental cache is deleted");
        assert!(deps.exists(), "compiled dependencies are kept");
        let kept =
            std::fs::read_to_string(dir.path().join("clippy.crash.log")).map_err(io(&log))?;
        assert!(kept.contains(COMPILER_PANIC), "the crashed log is kept");
        Ok(())
    }

    #[test]
    fn a_second_crash_stays_red_and_runs_no_third_time() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(io(Path::new("tempdir")))?;
        target_dir(dir.path())?;
        let log = dir.path().join("build.log");
        let step = FakeStep {
            log: &log,
            outputs: &[CRASH, CRASH, "ok"],
            calls: Cell::new(0),
        };
        let caches = incremental_dirs(dir.path(), &[]);
        let run = run_with_crash_retry(&log, &caches, || step.attempt(), |passed| *passed)?;
        assert_eq!(
            run,
            StepRun {
                value: false,
                retried: true
            }
        );
        assert_eq!(step.calls.get(), 2);
        assert!(
            compiler_crashed(&log),
            "the second crash's log is the step's log"
        );
        Ok(())
    }

    #[test]
    fn a_failure_without_a_crash_is_not_retried() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(io(Path::new("tempdir")))?;
        let (incremental, _) = target_dir(dir.path())?;
        let log = dir.path().join("clippy.log");
        let step = FakeStep {
            log: &log,
            outputs: &["error: this function has too many lines (101/100)", "ok"],
            calls: Cell::new(0),
        };
        let caches = incremental_dirs(dir.path(), &[]);
        let run = run_with_crash_retry(&log, &caches, || step.attempt(), |passed| *passed)?;
        assert_eq!(
            run,
            StepRun {
                value: false,
                retried: false
            }
        );
        assert_eq!(step.calls.get(), 1);
        assert!(incremental.exists(), "nothing is deleted");
        assert!(!dir.path().join("clippy.crash.log").exists());
        Ok(())
    }

    #[test]
    fn a_passing_step_is_not_retried_even_if_its_log_mentions_a_crash() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(io(Path::new("tempdir")))?;
        let log = dir.path().join("doc-tests.log");
        let step = FakeStep {
            log: &log,
            outputs: &["ok"],
            calls: Cell::new(0),
        };
        std::fs::write(&log, CRASH).map_err(io(&log))?;
        let run = run_with_crash_retry(&log, &[], || step.attempt(), |passed| *passed)?;
        assert!(!run.retried);
        assert_eq!(step.calls.get(), 1);
        Ok(())
    }

    #[test]
    fn the_caches_follow_the_profile_and_the_target() {
        let root = Path::new("/t");
        let dirs = |text: &str| -> Vec<String> {
            incremental_dirs(root, &args(text))
                .iter()
                .map(|p| p.display().to_string())
                .collect()
        };
        assert_eq!(
            dirs("clippy --all-targets --workspace -- -D warnings"),
            ["/t/debug/incremental"]
        );
        assert_eq!(
            dirs("test -p sim_core --test golden_replay_tests --locked --release"),
            ["/t/debug/incremental", "/t/release/incremental"]
        );
        assert_eq!(
            dirs("test -p sim_core --target wasm32-wasip1 --locked --release"),
            [
                "/t/debug/incremental",
                "/t/release/incremental",
                "/t/wasm32-wasip1/debug/incremental",
                "/t/wasm32-wasip1/release/incremental",
            ]
        );
        assert_eq!(
            dirs("check -p sim_core --target=wasm32-unknown-unknown"),
            [
                "/t/debug/incremental",
                "/t/wasm32-unknown-unknown/debug/incremental",
            ]
        );
        assert_eq!(
            dirs("clippy -- --release --target x"),
            ["/t/debug/incremental"],
            "words after `--` belong to the compiler"
        );
    }

    #[test]
    fn crash_logs_sit_beside_the_step_log() {
        assert_eq!(
            crash_log(Path::new("/l/clippy.log")),
            Path::new("/l/clippy.crash.log")
        );
        assert!(!compiler_crashed(Path::new("/nonexistent/build.log")));
    }
}
