//! Processes the runner starts: Cargo steps, the test build, and the test executables.
//!
//! # Responsibility
//! Runs one command with its output in a log file ([`run_logged`]); builds exactly the selected
//! test targets and reads their executables from Cargo's JSON messages ([`build_targets`]); runs
//! those executables on the lanes of a [`Scheduler`] the way `cargo test` starts them
//! ([`run_targets`]), measuring wall and CPU time; and judges a failed target's immediate rerun
//! ([`mark_flakes`]).
//!
//! # How it fits
//! `runner.rs` decides what to run per tier; this module is the only place that spawns Cargo or
//! a test binary. It never wraps anything in `with-gpu-lock`: GPU test children take the lock
//! themselves, one child at a time (charter §7).
//!
//! # Invariants
//! - A test executable runs with its package root as working directory and with
//!   `CARGO_MANIFEST_DIR`, `CARGO_PKG_NAME` and `CARGO` set as Cargo sets them, never with the
//!   `gate` package's own values inherited from `cargo run`.
//! - Every process writes to its own log file; nothing a test prints is lost or interleaved.
//! - Child CPU time is read from `/proc/self/stat` around each reap, and reaps are serialized,
//!   so each target is charged exactly its own process tree. Where `/proc` is missing the CPU
//!   column is empty, never zero.

use crate::catalog::TestClass;
use crate::schedule::{Job, Scheduler};
use crate::workspace::{TargetId, TargetKind, Workspace};
use crate::GateError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// How one test executable ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Exit status zero.
    Passed,
    /// A non-zero exit; the names of the tests that reported `FAILED`, if any.
    Failed {
        /// Failing test names, from the log.
        tests: Vec<String>,
    },
    /// The executable could not be started or was not built.
    Error(String),
    /// It failed, then passed when rerun at once (blueprint §3.5): not green, and a finding.
    Flaky {
        /// The tests that failed in the first run.
        tests: Vec<String>,
    },
}

impl Outcome {
    /// The verdict word of result tables: `PASS`, `FAIL`, `ERROR` or `FLAKY`.
    #[must_use]
    pub fn verdict(&self) -> &'static str {
        match self {
            Self::Passed => "PASS",
            Self::Failed { .. } => "FAIL",
            Self::Error(_) => "ERROR",
            Self::Flaky { .. } => "FLAKY",
        }
    }
}

/// One test executable's result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetResult {
    /// The target.
    pub target: TargetId,
    /// Its class.
    pub class: TestClass,
    /// Wall time.
    pub wall: Duration,
    /// CPU time of it and its children, where the host reports it.
    pub cpu: Option<Duration>,
    /// How it ended.
    pub outcome: Outcome,
    /// Its log file.
    pub log: PathBuf,
}

/// Environment variable prefixes `cargo run` sets for the `gate` binary itself, which must not
/// reach the processes the gate starts.
const INHERITED_CARGO_VARS: [&str; 6] = [
    "CARGO_PKG_",
    "CARGO_BIN_",
    "CARGO_CRATE_",
    "CARGO_MANIFEST_",
    "CARGO_PRIMARY_PACKAGE",
    "CARGO_RUSTC_CURRENT_DIR",
];

/// Removes the `gate` package's own Cargo variables from `command`'s environment.
pub fn clean_env(command: &mut Command) {
    for (key, _) in std::env::vars_os() {
        let text = key.to_string_lossy();
        if INHERITED_CARGO_VARS
            .iter()
            .any(|prefix| text.starts_with(prefix))
        {
            command.env_remove(&key);
        }
    }
}

/// Creates `path` for writing, mapping the error.
fn create(path: &Path) -> Result<File, GateError> {
    File::create(path).map_err(|source| GateError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Runs `program args` in `dir` with `env`, output to `log`; returns whether it succeeded.
///
/// # Errors
/// [`GateError::Io`] if the log cannot be created; [`GateError::Spawn`] if the program cannot
/// start.
pub fn run_logged(
    program: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    dir: &Path,
    log: &Path,
) -> Result<bool, GateError> {
    let mut file = create(log)?;
    let _ = writeln!(file, "$ {program} {}\n", args.join(" "));
    let err = file.try_clone().map_err(|source| GateError::Io {
        path: log.to_path_buf(),
        source,
    })?;
    let mut command = Command::new(program);
    clean_env(&mut command);
    let status = command
        .args(args)
        .envs(env)
        .current_dir(dir)
        .stdout(file)
        .stderr(err)
        .status()
        .map_err(|source| GateError::Spawn {
            program: program.to_string(),
            source,
        })?;
    Ok(status.success())
}

/// The selected target a `compiler-artifact` message describes, and its executable.
fn artifact(message: &Value, workspace: &Workspace, root: &Path) -> Option<(TargetId, PathBuf)> {
    if message["reason"].as_str() != Some("compiler-artifact")
        || message["profile"]["test"].as_bool() != Some(true)
    {
        return None;
    }
    let executable = PathBuf::from(message["executable"].as_str()?);
    let manifest = Path::new(message["manifest_path"].as_str()?);
    let dir = manifest.parent()?.strip_prefix(root).ok()?;
    let dir = dir.to_string_lossy().replace('\\', "/");
    let package = workspace.packages.values().find(|p| p.dir == dir)?;
    let target = &message["target"];
    let name = target["name"].as_str()?;
    let kinds: Vec<&str> = target["kind"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let kind = if kinds.contains(&"bin") {
        TargetKind::Bin(name.to_string())
    } else if kinds.contains(&"test") {
        TargetKind::Test(name.to_string())
    } else if kinds
        .iter()
        .any(|k| k.contains("lib") || *k == "proc-macro")
    {
        TargetKind::Lib
    } else {
        return None;
    };
    Some((
        TargetId {
            package: package.name.clone(),
            kind,
        },
        executable,
    ))
}

/// The `cargo test --no-run` argument lists that build `targets`: one `--workspace` build when
/// `whole` is set, else one build per crate with exactly its targets' filters.
///
/// One build per crate avoids building unselected targets: Cargo applies `--lib` and `--test`
/// filters to every `-p` of an invocation.
#[must_use]
pub fn build_invocations(targets: &BTreeSet<TargetId>, whole: bool) -> Vec<Vec<String>> {
    let base = ["test", "--locked", "--no-run", "--message-format=json"].map(String::from);
    if whole {
        let mut args = base.to_vec();
        args.push("--workspace".to_string());
        return vec![args];
    }
    let mut by_crate: BTreeMap<&str, Vec<&TargetId>> = BTreeMap::new();
    for target in targets {
        by_crate.entry(&target.package).or_default().push(target);
    }
    by_crate
        .into_iter()
        .map(|(krate, list)| {
            let mut args = base.to_vec();
            args.extend(["-p".to_string(), krate.to_string()]);
            for target in list {
                args.extend(target.cargo_filter());
            }
            args
        })
        .collect()
}

/// Builds `targets` and returns each one's executable; output goes to `log`.
///
/// # Errors
/// [`GateError::Io`] / [`GateError::Spawn`] for the log or Cargo; a failed build is reported
/// as `Ok(None)` so the caller can print the log.
pub fn build_targets(
    root: &Path,
    workspace: &Workspace,
    targets: &BTreeSet<TargetId>,
    whole: bool,
    log: &Path,
) -> Result<Option<BTreeMap<TargetId, PathBuf>>, GateError> {
    let mut file = create(log)?;
    let cargo = crate::changes::cargo_program();
    let mut executables = BTreeMap::new();
    for args in build_invocations(targets, whole) {
        let _ = writeln!(file, "$ {cargo} {}", args.join(" "));
        let err = file.try_clone().map_err(|source| GateError::Io {
            path: log.to_path_buf(),
            source,
        })?;
        let mut command = Command::new(&cargo);
        clean_env(&mut command);
        let mut child = command
            .args(&args)
            .current_dir(root)
            .stdout(Stdio::piped())
            .stderr(err)
            .spawn()
            .map_err(|source| GateError::Spawn {
                program: cargo.clone(),
                source,
            })?;
        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                // Rustc prints its crash report to stderr, which reaches the log; should a
                // JSON message ever carry it instead, it goes to the log too, so the compiler
                // crash retry (`crash.rs`) sees it either way.
                if line.contains(crate::crash::COMPILER_PANIC) {
                    let _ = writeln!(file, "{line}");
                }
                let Ok(message) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if let Some((id, exe)) = artifact(&message, workspace, root) {
                    if targets.contains(&id) {
                        executables.insert(id, exe);
                    }
                }
            }
        }
        let status = child.wait().map_err(|source| GateError::Spawn {
            program: cargo.clone(),
            source,
        })?;
        if !status.success() {
            return Ok(None);
        }
    }
    Ok(Some(executables))
}

/// CPU time (user + system) of this process's reaped children, from `/proc/self/stat`.
///
/// Linux reports it in clock ticks of `USER_HZ`, which the kernel ABI fixes at 100 per second.
fn children_cpu() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after_name = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = after_name.split_whitespace().collect();
    // Fields after the name start at field 3 (state); cutime is field 16, cstime field 17.
    let cutime: u64 = fields.get(13)?.parse().ok()?;
    let cstime: u64 = fields.get(14)?.parse().ok()?;
    Some(Duration::from_millis((cutime + cstime) * 10))
}

/// Test names reported `FAILED` in a libtest log.
#[must_use]
pub fn failed_tests(log: &str) -> Vec<String> {
    let mut names: Vec<String> = log
        .lines()
        .filter_map(|line| {
            line.strip_prefix("test ")?
                .strip_suffix(" ... FAILED")
                .map(str::to_string)
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The file name of a target's log.
fn log_name(target: &TargetId) -> String {
    format!("{}.log", target.to_string().replace('/', "__"))
}

/// What every lane needs to start an executable.
struct Launch<'a> {
    root: &'a Path,
    workspace: &'a Workspace,
    log_dir: &'a Path,
    reap: &'a Mutex<()>,
}

impl Launch<'_> {
    /// Runs one test executable to completion.
    fn run(&self, job: &Job, exe: &Path) -> TargetResult {
        let log = self.log_dir.join(log_name(&job.target));
        let start = Instant::now();
        let (outcome, cpu) = match self.spawn_and_wait(job, exe, &log) {
            Ok((true, cpu)) => (Outcome::Passed, cpu),
            Ok((false, cpu)) => {
                let text = std::fs::read_to_string(&log).unwrap_or_default();
                let tests = failed_tests(&text);
                (Outcome::Failed { tests }, cpu)
            }
            Err(error) => (Outcome::Error(error.to_string()), None),
        };
        TargetResult {
            target: job.target.clone(),
            class: job.class,
            wall: start.elapsed(),
            cpu,
            outcome,
            log,
        }
    }

    fn spawn_and_wait(
        &self,
        job: &Job,
        exe: &Path,
        log: &Path,
    ) -> Result<(bool, Option<Duration>), GateError> {
        let package = self
            .workspace
            .package(&job.target.package)
            .ok_or_else(|| GateError::Catalog(format!("no crate {}", job.target.package)))?;
        let dir = self.root.join(&package.dir);
        let file = create(log)?;
        let err = file.try_clone().map_err(|source| GateError::Io {
            path: log.to_path_buf(),
            source,
        })?;
        let crate_name = match &job.target.kind {
            TargetKind::Lib => package.name.replace('-', "_"),
            TargetKind::Bin(name) | TargetKind::Test(name) => name.replace('-', "_"),
        };
        let mut library_path = exe
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
            .into_os_string();
        if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
            library_path.push(":");
            library_path.push(existing);
        }
        let mut command = Command::new(exe);
        clean_env(&mut command);
        command
            .current_dir(&dir)
            .env("CARGO", crate::changes::cargo_program())
            .env("CARGO_MANIFEST_DIR", &dir)
            .env("CARGO_MANIFEST_PATH", dir.join("Cargo.toml"))
            .env("CARGO_PKG_NAME", &package.name)
            .env("CARGO_PKG_VERSION", &package.version)
            .env("CARGO_CRATE_NAME", crate_name)
            .env("LD_LIBRARY_PATH", library_path)
            .stdout(file)
            .stderr(err);
        let mut child = command.spawn().map_err(|source| GateError::Spawn {
            program: exe.display().to_string(),
            source,
        })?;
        loop {
            {
                let _reaping = self.reap.lock().unwrap_or_else(PoisonError::into_inner);
                let before = children_cpu();
                let waited = child.try_wait().map_err(|source| GateError::Spawn {
                    program: exe.display().to_string(),
                    source,
                })?;
                if let Some(status) = waited {
                    let cpu = before.zip(children_cpu()).map(|(b, a)| a.saturating_sub(b));
                    return Ok((status.success(), cpu));
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

/// Shared state of a lane run.
struct Lanes {
    scheduler: Scheduler,
    results: Vec<TargetResult>,
}

/// Runs every job on the lanes of `scheduler`; `executables` maps targets to their binaries.
/// Prints one line per finished target. Results are in finishing order.
#[must_use]
pub fn run_targets(
    root: &Path,
    workspace: &Workspace,
    scheduler: Scheduler,
    executables: &BTreeMap<TargetId, PathBuf>,
    log_dir: &Path,
    total: usize,
) -> Vec<TargetResult> {
    let reap = Mutex::new(());
    let launch = Launch {
        root,
        workspace,
        log_dir,
        reap: &reap,
    };
    let state = Mutex::new(Lanes {
        scheduler,
        results: Vec::new(),
    });
    let wake = Condvar::new();
    std::thread::scope(|scope| {
        let mut guard = state.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            while let Some((job, lane)) = guard.scheduler.take() {
                let (launch, state, wake) = (&launch, &state, &wake);
                let exe = executables.get(&job.target).cloned();
                scope.spawn(move || {
                    let result = match exe {
                        Some(exe) => launch.run(&job, &exe),
                        None => not_built(&job, launch.log_dir),
                    };
                    let mut lanes = state.lock().unwrap_or_else(PoisonError::into_inner);
                    lanes.scheduler.finish(lane);
                    print_progress(&result, lanes.results.len() + 1, total);
                    lanes.results.push(result);
                    wake.notify_all();
                });
            }
            if guard.scheduler.is_done() {
                break;
            }
            guard = wake.wait(guard).unwrap_or_else(PoisonError::into_inner);
        }
    });
    state
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .results
}

/// The result of a selected target the build produced no executable for.
fn not_built(job: &Job, log_dir: &Path) -> TargetResult {
    TargetResult {
        target: job.target.clone(),
        class: job.class,
        wall: Duration::ZERO,
        cpu: None,
        outcome: Outcome::Error("the build produced no executable for it".to_string()),
        log: log_dir.join("build.log"),
    }
}

/// Applies the immediate reruns of failed targets (blueprint §3.5): a target whose first run
/// failed and whose rerun passed becomes [`Outcome::Flaky`], keeping its first run's time and
/// log; a target whose rerun failed too stays failed. Targets without a rerun are unchanged.
pub fn mark_flakes(results: &mut [TargetResult], reruns: &[TargetResult]) {
    for result in results.iter_mut() {
        let Outcome::Failed { tests } = &result.outcome else {
            continue;
        };
        let passed_again = reruns
            .iter()
            .any(|rerun| rerun.target == result.target && rerun.outcome == Outcome::Passed);
        if passed_again {
            result.outcome = Outcome::Flaky {
                tests: tests.clone(),
            };
        }
    }
}

/// One progress line: `[n/total] PASS  12.3 s  crate/target`.
fn print_progress(result: &TargetResult, done: usize, total: usize) {
    let verdict = result.outcome.verdict();
    println!(
        "[{done}/{total}] {verdict:<5} {:>7.1} s  {}",
        result.wall.as_secs_f64(),
        result.target
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn failed_test_names_are_read_from_the_log() {
        let log = "running 3 tests\ntest a::b ... ok\ntest a::c ... FAILED\ntest d ... FAILED\n\nfailures:\n    a::c\n";
        assert_eq!(failed_tests(log), ["a::c", "d"]);
    }

    fn result(target: &str, outcome: Outcome) -> TargetResult {
        TargetResult {
            target: TargetId::test("core", target),
            class: TestClass::Fast,
            wall: Duration::from_secs(1),
            cpu: None,
            outcome,
            log: PathBuf::from(format!("{target}.log")),
        }
    }

    #[test]
    fn a_failure_that_passes_on_its_rerun_is_flaky_and_one_that_fails_again_is_not() {
        let failed = || Outcome::Failed {
            tests: vec!["t".to_string()],
        };
        let mut results = vec![
            result("flaky", failed()),
            result("broken", failed()),
            result("green", Outcome::Passed),
            result("missing", Outcome::Error("not built".to_string())),
        ];
        let reruns = vec![result("flaky", Outcome::Passed), result("broken", failed())];
        mark_flakes(&mut results, &reruns);
        let verdicts: Vec<&str> = results.iter().map(|r| r.outcome.verdict()).collect();
        assert_eq!(verdicts, ["FLAKY", "FAIL", "PASS", "ERROR"]);
        assert_eq!(
            results[0].outcome,
            Outcome::Flaky {
                tests: vec!["t".to_string()]
            }
        );
        assert_eq!(results[0].log, PathBuf::from("flaky.log"));
    }

    #[test]
    fn builds_are_split_per_crate_with_exact_filters() {
        let targets = BTreeSet::from([
            TargetId::lib("core"),
            TargetId::test("core", "x_tests"),
            TargetId::test("app", "y_tests"),
            TargetId::bin("app", "tool"),
        ]);
        let builds = build_invocations(&targets, false);
        assert_eq!(builds.len(), 2);
        assert!(builds[0]
            .ends_with(&["-p", "app", "--bin", "tool", "--test", "y_tests"].map(String::from)));
        assert!(
            builds[1].ends_with(&["-p", "core", "--lib", "--test", "x_tests"].map(String::from))
        );
        let whole = build_invocations(&targets, true);
        assert_eq!(whole.len(), 1);
        assert_eq!(whole[0].last().map(String::as_str), Some("--workspace"));
    }

    #[test]
    fn artifacts_map_to_targets_by_manifest_and_kind() {
        let mut workspace = Workspace::default();
        workspace.packages.insert(
            "core".into(),
            crate::workspace::Package {
                name: "core".into(),
                version: "0.1.0".into(),
                dir: "crates/core".into(),
                dependencies: BTreeMap::new(),
                targets: Vec::new(),
            },
        );
        let message = |kind: &str, name: &str, test: bool| {
            json!({"reason": "compiler-artifact", "manifest_path": "/r/crates/core/Cargo.toml",
                   "profile": {"test": test}, "executable": "/t/deps/x-1",
                   "target": {"name": name, "kind": [kind]}})
        };
        let root = Path::new("/r");
        assert_eq!(
            artifact(&message("lib", "core", true), &workspace, root).map(|a| a.0),
            Some(TargetId::lib("core"))
        );
        assert_eq!(
            artifact(&message("test", "it", true), &workspace, root).map(|a| a.0),
            Some(TargetId::test("core", "it"))
        );
        assert_eq!(
            artifact(&message("lib", "core", false), &workspace, root),
            None
        );
    }

    #[test]
    fn child_cpu_time_is_readable_on_linux() {
        if Path::new("/proc/self/stat").exists() {
            assert!(children_cpu().is_some());
        }
    }
}
