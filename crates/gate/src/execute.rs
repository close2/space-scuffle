//! Processes the runner starts: Cargo steps, the test build, and the test executables.
//!
//! # Responsibility
//! Runs one command with its output in a log file ([`run_logged`]); builds exactly the selected
//! test targets and reads their executables from Cargo's JSON messages ([`build_targets`]); runs
//! those executables on the lanes of a [`Scheduler`] the way `cargo test` starts them
//! ([`run_targets`]), measuring wall and CPU time and each process tree's memory peak, killing a
//! tree past its time limit or on a stop request (plan 2026-11-11-faster-gates §3); and judges a
//! failed target's immediate rerun ([`mark_flakes`]).
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
//!   Progress lines are printed whole under the lanes' mutex; only their order varies.
//! - A killed target's whole process tree is killed, found through parent pids (`memory.rs`).
//! - Child CPU time is read from `/proc/self/stat` around each reap, and reaps are serialized,
//!   so each target is charged exactly its own process tree. Where `/proc` is missing the CPU
//!   column is empty, never zero.

use crate::catalog::TestClass;
use crate::memory::{kill_tree, MemorySource, Sampler, KILL_GRACE};
use crate::schedule::{Job, Lane, Scheduler};
use crate::workspace::{TargetId, TargetKind, Workspace};
use crate::GateError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufRead, BufReader, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
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
        /// The tests that failed in the first run (`timed out under lanes` for a timeout).
        tests: Vec<String>,
    },
    /// It ran past its time limit and its process tree was killed (plan §3.4); a failure.
    TimedOut,
    /// A stop request ended the run before it started or while it ran.
    Skipped,
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
            Self::TimedOut => "TIMEOUT",
            Self::Skipped => "SKIP",
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
    /// Where and when it ran on the lanes, and its memory; `None` when it never started.
    pub lane: Option<LaneRecord>,
}

/// One executable's run on the lanes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneRecord {
    /// The lane it ran on.
    pub lane: Lane,
    /// When it started, from the start of the lane run.
    pub start: Duration,
    /// The peak anonymous memory of its process tree in MiB, if a scan saw it.
    pub peak_anon_mib: Option<u64>,
    /// Its estimate when it was scheduled, in MiB.
    pub estimate_mib: Option<u64>,
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

/// The arguments a test executable gets: `--test-threads=<n>` when the catalog caps the job,
/// else none, so libtest runs one thread per CPU as under `cargo test`.
#[must_use]
pub fn test_arguments(job: &Job) -> Vec<String> {
    job.test_threads
        .map(|threads| vec![format!("--test-threads={threads}")])
        .unwrap_or_default()
}

/// The file name of a target's log.
fn log_name(target: &TargetId) -> String {
    format!("{}.log", target.to_string().replace('/', "__"))
}

/// The limit for a target with no recorded wall time (plan §3.4).
pub const UNKNOWN_TIME_LIMIT: Duration = Duration::from_hours(2);

/// The margin a target's limit always adds to its recorded wall time (plan §3.4).
pub const TIME_LIMIT_MARGIN: Duration = Duration::from_mins(15);

/// The factor a target's limit applies to its recorded wall time (plan §3.4).
pub const TIME_LIMIT_FACTOR: u32 = 4;

/// The time limit of a target whose last recorded wall time is `expected` (zero when unknown),
/// or the catalog's `timeout_s` when it sets one.
///
/// The limit frees a lane from a hang; it does not police speed. Today the longest executable
/// takes 605 s, and a GPU binary runs many children that may each wait up to 290 s for the GPU
/// lock and then run 120 s, so the limit is `max(4 × expected, expected + 15 min)`, and two
/// hours for a target never timed.
#[must_use]
pub fn time_limit(expected: Duration, catalog: Option<Duration>) -> Duration {
    if let Some(limit) = catalog {
        return limit;
    }
    if expected.is_zero() {
        return UNKNOWN_TIME_LIMIT;
    }
    (expected * TIME_LIMIT_FACTOR).max(expected + TIME_LIMIT_MARGIN)
}

/// The file beside the gate lock where `cargo gate stop` asks the holder to stop.
#[must_use]
pub fn stop_path(lock: &Path) -> PathBuf {
    let mut name = lock.as_os_str().to_os_string();
    name.push(".stop");
    PathBuf::from(name)
}

/// The text of a stop request naming the holder `pid`.
#[must_use]
pub fn stop_request(pid: u32) -> String {
    format!("pid={pid}\n")
}

/// Whether the stop request at `path` names `pid`.
#[must_use]
pub fn stop_requested(path: &Path, pid: u32) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| text.trim() == stop_request(pid).trim())
}

/// Reads `exe` once, sequentially, and discards the bytes, so the executable pages in at disk
/// streaming speed rather than one random fault at a time (plan §3.1, L-6).
fn pre_read(exe: &Path) {
    // PERFORMANCE RATIONALE:
    // Why: an executable paged in by faults from the rotational scratch disk costs about 80 ms
    // per random read; three lanes faulting at once compete for the same head.
    // What: one sequential read of the whole file into a reused 4 MiB buffer, discarded.
    // Trade-off: the bytes are read once more than strictly needed, at streaming speed, which
    // is charged to the target's wall time.
    let Ok(mut file) = File::open(exe) else {
        return;
    };
    let mut buffer = vec![0_u8; 4 << 20];
    while matches!(file.read(&mut buffer), Ok(read) if read > 0) {}
}

/// What every lane needs to start an executable.
pub struct LaneRun<'a> {
    /// The repository root.
    pub root: &'a Path,
    /// The workspace, for each target's package directory.
    pub workspace: &'a Workspace,
    /// The built executables.
    pub executables: &'a BTreeMap<TargetId, PathBuf>,
    /// Where each target's log, and `lanes.tsv`, go.
    pub log_dir: &'a Path,
    /// Where the live slice reading comes from.
    pub memory: MemorySource,
    /// The stop request file `cargo gate stop` writes, if this run can be stopped.
    pub stop_file: Option<PathBuf>,
}

/// What a lane run produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaneOutcome {
    /// One result per job, in finishing order; unstarted jobs of a stopped run are
    /// [`Outcome::Skipped`].
    pub results: Vec<TargetResult>,
    /// Wall time from the first start to the last finish.
    pub wall: Duration,
    /// The largest live slice reading, in MiB.
    pub slice_peak_mib: Option<u64>,
    /// Whether a stop request ended the run.
    pub stopped: bool,
}

/// Shared state of a lane run.
struct Lanes {
    scheduler: Scheduler,
    results: Vec<TargetResult>,
    /// Whether the "holding for memory" line was printed since the last start.
    memory_note: bool,
}

/// The per-job context of one lane.
struct Running<'a> {
    run: &'a LaneRun<'a>,
    sampler: &'a Sampler,
    reap: &'a Mutex<()>,
    stop: &'a AtomicBool,
    origin: Instant,
}

impl Running<'_> {
    /// Runs one test executable to completion, a timeout or a stop.
    fn run(&self, job: &Job, lane: Lane, exe: &Path) -> TargetResult {
        let log = self.run.log_dir.join(log_name(&job.target));
        let start = Instant::now();
        pre_read(exe);
        let (outcome, cpu) = match self.spawn_and_wait(job, exe, &log) {
            Ok((Ended::Exited(true), cpu)) => (Outcome::Passed, cpu),
            Ok((Ended::Exited(false), cpu)) => {
                let text = std::fs::read_to_string(&log).unwrap_or_default();
                let tests = failed_tests(&text);
                (Outcome::Failed { tests }, cpu)
            }
            Ok((Ended::TimedOut, cpu)) => (Outcome::TimedOut, cpu),
            Ok((Ended::Stopped, cpu)) => (Outcome::Skipped, cpu),
            Err(error) => (Outcome::Error(error.to_string()), None),
        };
        TargetResult {
            target: job.target.clone(),
            class: job.class,
            wall: start.elapsed(),
            cpu,
            outcome,
            log,
            lane: Some(LaneRecord {
                lane,
                start: start.saturating_duration_since(self.origin),
                peak_anon_mib: self.sampler.unwatch(&job.target),
                estimate_mib: job.estimate_mib,
            }),
        }
    }

    /// The command that runs `exe` as `cargo test` would for `job`.
    fn command(&self, job: &Job, exe: &Path, log: &Path) -> Result<Command, GateError> {
        let package = self
            .run
            .workspace
            .package(&job.target.package)
            .ok_or_else(|| GateError::Catalog(format!("no crate {}", job.target.package)))?;
        let dir = self.run.root.join(&package.dir);
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
            .args(test_arguments(job))
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
        Ok(command)
    }

    fn spawn_and_wait(
        &self,
        job: &Job,
        exe: &Path,
        log: &Path,
    ) -> Result<(Ended, Option<Duration>), GateError> {
        let spawn_error = |source| GateError::Spawn {
            program: exe.display().to_string(),
            source,
        };
        let mut child = self.command(job, exe, log)?.spawn().map_err(spawn_error)?;
        self.sampler.watch(&job.target, child.id());
        let started = Instant::now();
        let mut ended = None;
        loop {
            {
                let _reaping = self.reap.lock().unwrap_or_else(PoisonError::into_inner);
                let before = children_cpu();
                if let Some(status) = child.try_wait().map_err(spawn_error)? {
                    let cpu = before.zip(children_cpu()).map(|(b, a)| a.saturating_sub(b));
                    let ended = ended.unwrap_or(Ended::Exited(status.success()));
                    return Ok((ended, cpu));
                }
            }
            if ended.is_none() {
                let over = job.timeout.is_some_and(|limit| started.elapsed() > limit);
                if over || self.stop.load(Ordering::SeqCst) {
                    ended = Some(if over {
                        Ended::TimedOut
                    } else {
                        Ended::Stopped
                    });
                    kill_tree(child.id(), KILL_GRACE);
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

/// How an executable's run ended, before its log is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ended {
    /// It exited by itself, successfully or not.
    Exited(bool),
    /// Its time limit passed and its tree was killed.
    TimedOut,
    /// A stop request killed its tree.
    Stopped,
}

/// MiB as GiB with one decimal (rounded down), for progress lines and reports.
#[must_use]
pub fn gib(mib: u64) -> String {
    format!("{}.{} GiB", mib / 1024, (mib % 1024) * 10 / 1024)
}

/// The `[start]` line of a job.
fn start_line(job: &Job, lane: Lane) -> String {
    let estimate = job
        .estimate_mib
        .map_or_else(|| "unmeasured".to_string(), gib);
    format!(
        "[start] {lane} {} (est. {estimate}, {:.0} s)",
        job.target,
        job.expected.as_secs_f64()
    )
}

/// The line printed when a target finishes: `[n/total] PASS  12.3 s  5.1 GiB  L2 crate/target`.
fn finish_line(result: &TargetResult, done: usize, total: usize) -> String {
    let record = result.lane.as_ref();
    let peak = record
        .and_then(|r| r.peak_anon_mib)
        .map_or_else(|| "-".to_string(), gib);
    let lane = record.map_or_else(|| "-".to_string(), |r| r.lane.to_string());
    format!(
        "[{done}/{total}] {:<7} {:>7.1} s  {peak:>8}  {lane} {}",
        result.outcome.verdict(),
        result.wall.as_secs_f64(),
        result.target
    )
}

/// Runs every job of `scheduler` on its lanes; returns the results in finishing order.
///
/// Prints one whole line, under the lanes' mutex, when a target starts and when it ends. A
/// sampler thread measures each target's process tree and the slice once a second; a target
/// past its time limit has its whole tree killed; a stop request (see [`stop_path`]) naming this
/// process kills every running tree, starts nothing more and records the rest as skipped.
#[must_use]
pub fn run_targets(run: &LaneRun<'_>, scheduler: Scheduler) -> LaneOutcome {
    let total = scheduler.waiting();
    let sampler = Sampler::start(run.memory.clone(), SAMPLE_PERIOD);
    let reap = Mutex::new(());
    let stop = AtomicBool::new(false);
    let origin = Instant::now();
    let running = Running {
        run,
        sampler: &sampler,
        reap: &reap,
        stop: &stop,
        origin,
    };
    let state = Mutex::new(Lanes {
        scheduler,
        results: Vec::new(),
        memory_note: false,
    });
    let wake = Condvar::new();
    std::thread::scope(|scope| {
        let mut guard = state.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if !stop.load(Ordering::SeqCst)
                && run
                    .stop_file
                    .as_deref()
                    .is_some_and(|path| stop_requested(path, std::process::id()))
            {
                println!("stop requested: killing the running targets, starting nothing more");
                stop.store(true, Ordering::SeqCst);
                for job in guard.scheduler.drain_waiting() {
                    let skipped = skipped(&job, run.log_dir);
                    guard.results.push(skipped);
                }
            }
            while let Some((job, lane)) = guard.scheduler.take(sampler.live_mib()) {
                println!("{}", start_line(&job, lane));
                guard.memory_note = false;
                let (running, state, wake) = (&running, &state, &wake);
                let exe = run.executables.get(&job.target).cloned();
                scope.spawn(move || {
                    let result = match exe {
                        Some(exe) => running.run(&job, lane, &exe),
                        None => not_built(&job, running.run.log_dir),
                    };
                    let mut lanes = state.lock().unwrap_or_else(PoisonError::into_inner);
                    lanes.scheduler.finish(lane);
                    println!("{}", finish_line(&result, lanes.results.len() + 1, total));
                    lanes.results.push(result);
                    wake.notify_all();
                });
            }
            if guard.scheduler.held_for_memory() && !guard.memory_note {
                guard.memory_note = true;
                println!(
                    "memory: slice anon {}; holding new targets until running ones end",
                    sampler
                        .live_mib()
                        .map_or_else(|| "unknown".to_string(), gib)
                );
            }
            if guard.scheduler.is_done() {
                break;
            }
            guard = wake
                .wait_timeout(guard, WAKE_PERIOD)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    });
    let results = state
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .results;
    LaneOutcome {
        wall: origin.elapsed(),
        slice_peak_mib: sampler.slice_peak_mib(),
        stopped: stop.load(Ordering::SeqCst),
        results,
    }
}

/// How often the sampler scans `/proc` (plan §3.3).
const SAMPLE_PERIOD: Duration = Duration::from_secs(1);

/// How often the lane loop looks for a stop request and new memory readings while it waits.
const WAKE_PERIOD: Duration = Duration::from_millis(250);

/// The result of a job a stopped run never started.
fn skipped(job: &Job, log_dir: &Path) -> TargetResult {
    TargetResult {
        target: job.target.clone(),
        class: job.class,
        wall: Duration::ZERO,
        cpu: None,
        outcome: Outcome::Skipped,
        log: log_dir.join(log_name(&job.target)),
        lane: None,
    }
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
        lane: None,
    }
}

/// `lanes.tsv`: one line per target that ran, sorted by start: target, lane, start, end, wall,
/// CPU, peak, estimate (seconds and MiB) and outcome.
#[must_use]
pub fn lanes_tsv(results: &[TargetResult]) -> String {
    let mut rows: Vec<&TargetResult> = results.iter().filter(|r| r.lane.is_some()).collect();
    rows.sort_by_key(|r| (r.lane.as_ref().map(|l| l.start), r.target.clone()));
    let mut out = String::from(
        "target\tlane\tstart_s\tend_s\twall_s\tcpu_s\tpeak_mib\testimate_mib\toutcome\n",
    );
    let optional = |value: Option<u64>| value.map_or_else(|| "-".to_string(), |v| v.to_string());
    for result in rows {
        let Some(record) = &result.lane else {
            continue;
        };
        let end = record.start + result.wall;
        let cpu = result
            .cpu
            .map_or_else(|| "-".to_string(), |c| format!("{:.1}", c.as_secs_f64()));
        let _ = writeln!(
            out,
            "{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{cpu}\t{}\t{}\t{}",
            result.target,
            record.lane,
            record.start.as_secs_f64(),
            end.as_secs_f64(),
            result.wall.as_secs_f64(),
            optional(record.peak_anon_mib),
            optional(record.estimate_mib),
            result.outcome.verdict()
        );
    }
    out
}

/// The targets whose run overlapped `target`'s in `results`, by name.
#[must_use]
pub fn overlapping(results: &[TargetResult], target: &TargetId) -> Vec<String> {
    let span = |r: &TargetResult| r.lane.as_ref().map(|l| (l.start, l.start + r.wall));
    let Some((start, end)) = results.iter().find(|r| &r.target == target).and_then(span) else {
        return Vec::new();
    };
    results
        .iter()
        .filter(|r| &r.target != target)
        .filter(|r| span(r).is_some_and(|(s, e)| s < end && start < e))
        .map(|r| r.target.to_string())
        .collect()
}

/// Applies the immediate reruns of failed targets (blueprint §3.5): a target whose first run
/// failed and whose rerun passed becomes [`Outcome::Flaky`], keeping its first run's time and
/// log; a target whose rerun failed too stays failed. Targets without a rerun are unchanged.
pub fn mark_flakes(results: &mut [TargetResult], reruns: &[TargetResult]) {
    for result in results.iter_mut() {
        let tests = match &result.outcome {
            Outcome::Failed { tests } => tests.clone(),
            Outcome::TimedOut => vec!["timed out under lanes".to_string()],
            _ => continue,
        };
        let passed_again = reruns
            .iter()
            .any(|rerun| rerun.target == result.target && rerun.outcome == Outcome::Passed);
        if passed_again {
            result.outcome = Outcome::Flaky { tests };
        }
    }
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
            lane: None,
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
    fn a_capped_job_gets_its_test_thread_limit_and_others_get_nothing() {
        let mut job = Job {
            target: TargetId::test("app", "heavy_tests"),
            class: TestClass::Slow,
            expected: Duration::ZERO,
            test_threads: Some(2),
            estimate_mib: None,
            exclusive: false,
            timeout: None,
        };
        assert_eq!(test_arguments(&job), ["--test-threads=2"]);
        job.test_threads = None;
        assert_eq!(test_arguments(&job), Vec::<String>::new());
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
