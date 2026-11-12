//! The lane runtime with real processes: overlap, memory peaks of process trees, timeouts, the
//! stop request, reruns, progress lines and `lanes.tsv`.
//!
//! # Responsibility
//! Proves at the process level what `src/execute.rs`, `src/memory.rs` and `src/schedule.rs`
//! promise for test lanes (plan `docs/plans/2026-11-11-faster-gates.md` §3, §6.4): lanes run
//! executables at once, each writing only its own log; a target's peak counts a grandchild in
//! another process group; a target past its limit is killed with its whole tree; a stop request
//! kills what runs and skips the rest; a failed target is rerun alone after the lanes; progress
//! lines are whole lines and `lanes.tsv` lists every target.
//!
//! # How it fits
//! The test executables are this test binary itself: each "target" is a small shell script that
//! re-runs this binary's [`fake_entry`] test with a behaviour in [`FAKE_VAR`] (sleep, allocate
//! and touch memory, start a grandchild in a new process group, write a marker, exit with a
//! status), the way the GPU harness re-runs its own test children. [`gate::run_targets`] drives
//! them on a synthetic one-crate workspace in a temporary directory. No GPU and no gate run.
//!
//! # Invariants
//! - Every fake ends by itself within a minute even if the runtime under test failed to kill it,
//!   and every test checks that no process it started outlives it.

#![allow(clippy::expect_used, clippy::panic)]

use gate::{
    lanes_tsv, mark_flakes, read_process_table, rerun_alone, run_targets, stop_request, GateError,
    Job, LaneOutcome, LaneRun, MemoryLimits, MemorySource, Outcome, Package, Scheduler, Target,
    TargetId, TestClass, Workspace,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The variable that turns this binary into a fake test executable, with its behaviour.
const FAKE_VAR: &str = "GATE_LANE_FAKE";

/// The variable that makes [`progress_lines_are_whole_lines_and_lanes_tsv_lists_every_target`]
/// run its lanes instead of re-running itself.
const PRINT_VAR: &str = "GATE_LANE_PRINT";

/// What a fake does, parsed from `key=value;...`.
#[derive(Clone, Debug, Default)]
struct Fake {
    name: String,
    sleep_ms: u64,
    alloc_mib: usize,
    grandchild_mib: usize,
    marker: Option<PathBuf>,
    fail_once: Option<PathBuf>,
    exit: i32,
}

impl Fake {
    fn parse(text: &str) -> Self {
        let mut fake = Self::default();
        for pair in text.split(';').filter(|p| !p.is_empty()) {
            let (key, value) = pair
                .split_once('=')
                .expect("invariant: fakes write key=value");
            match key {
                "name" => fake.name = value.to_string(),
                "sleep_ms" => fake.sleep_ms = value.parse().unwrap_or(0),
                "alloc_mib" => fake.alloc_mib = value.parse().unwrap_or(0),
                "grandchild_mib" => fake.grandchild_mib = value.parse().unwrap_or(0),
                "marker" => fake.marker = Some(PathBuf::from(value)),
                "fail_once" => fake.fail_once = Some(PathBuf::from(value)),
                "exit" => fake.exit = value.parse().unwrap_or(0),
                other => panic!("unknown fake key {other}"),
            }
        }
        fake
    }

    fn spec(&self) -> String {
        let mut spec = format!(
            "name={};sleep_ms={};alloc_mib={};grandchild_mib={};exit={}",
            self.name, self.sleep_ms, self.alloc_mib, self.grandchild_mib, self.exit
        );
        if let Some(marker) = &self.marker {
            let _ = write!(spec, ";marker={}", marker.display());
        }
        if let Some(path) = &self.fail_once {
            let _ = write!(spec, ";fail_once={}", path.display());
        }
        spec
    }
}

/// Plays the fake named in [`FAKE_VAR`]; a no-op in a normal test run.
#[test]
fn fake_entry() {
    let Ok(spec) = std::env::var(FAKE_VAR) else {
        return;
    };
    let fake = Fake::parse(&spec);
    println!("fake {} running", fake.name);
    let mut held: Vec<u8> = Vec::new();
    if fake.alloc_mib > 0 {
        // `vec![1; n]` writes every byte, so every page is resident.
        held = vec![1_u8; fake.alloc_mib << 20];
    }
    let mut grandchild = None;
    if fake.grandchild_mib > 0 {
        let child_spec = Fake {
            name: format!("{}-grandchild", fake.name),
            sleep_ms: fake.sleep_ms,
            alloc_mib: fake.grandchild_mib,
            ..Fake::default()
        };
        let exe = std::env::current_exe().expect("invariant: a running binary has a path");
        let child = Command::new(exe)
            .args(["fake_entry", "--exact", "--nocapture", "--test-threads=1"])
            .env(FAKE_VAR, child_spec.spec())
            .process_group(0)
            .spawn()
            .expect("invariant: this binary can start itself");
        grandchild = Some(child);
    }
    if let Some(marker) = &fake.marker {
        let mut pids = format!("{}\n", std::process::id());
        if let Some(child) = &grandchild {
            let _ = writeln!(pids, "{}", child.id());
        }
        std::fs::write(marker, pids).expect("invariant: the marker's directory exists");
    }
    std::thread::sleep(Duration::from_millis(fake.sleep_ms));
    if let Some(mut child) = grandchild {
        child.wait().ok();
    }
    std::hint::black_box(&held);
    if let Some(path) = &fake.fail_once {
        if !path.exists() {
            std::fs::write(path, "failed once").expect("invariant: the directory exists");
            println!("test fake ... FAILED");
            std::process::exit(1);
        }
    }
    println!("fake {} done", fake.name);
    std::process::exit(fake.exit);
}

/// A one-crate workspace in a temporary directory, with a script per fake target.
struct Sandbox {
    dir: tempfile::TempDir,
    logs: PathBuf,
    workspace: Workspace,
    executables: BTreeMap<TargetId, PathBuf>,
}

impl Sandbox {
    fn new(fakes: &[Fake]) -> Self {
        Self::with(|_| fakes.to_vec())
    }

    /// A sandbox whose fakes may name paths inside it.
    fn with(fakes: impl FnOnce(&Path) -> Vec<Fake>) -> Self {
        let dir = tempfile::tempdir().expect("invariant: a temporary directory can be made");
        let root = dir.path();
        let fakes = fakes(root);
        std::fs::create_dir_all(root.join("crates/fake")).expect("invariant: writable");
        std::fs::create_dir_all(root.join("logs")).expect("invariant: writable");
        let exe = std::env::current_exe().expect("invariant: a running binary has a path");
        let mut executables = BTreeMap::new();
        let mut targets = Vec::new();
        for fake in &fakes {
            let id = TargetId::test("fake", &fake.name);
            let script = root.join(format!("{}.sh", fake.name));
            let text = format!(
                "#!/bin/sh\nexport {FAKE_VAR}='{}'\nexec '{}' fake_entry --exact --nocapture --test-threads=1\n",
                fake.spec(),
                exe.display()
            );
            std::fs::write(&script, text).expect("invariant: writable");
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .expect("invariant: the script is ours");
            executables.insert(id.clone(), script);
            targets.push(Target {
                id,
                src_path: format!("crates/fake/tests/{}.rs", fake.name),
            });
        }
        let mut workspace = Workspace::default();
        workspace.packages.insert(
            "fake".to_string(),
            Package {
                name: "fake".to_string(),
                version: "0.1.0".to_string(),
                dir: "crates/fake".to_string(),
                dependencies: BTreeMap::new(),
                targets,
            },
        );
        Self {
            logs: root.join("logs"),
            dir,
            workspace,
            executables,
        }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root().join(name)
    }

    fn lane_run(&self, stop_file: Option<PathBuf>) -> LaneRun<'_> {
        LaneRun {
            root: self.root(),
            workspace: &self.workspace,
            executables: &self.executables,
            log_dir: &self.logs,
            memory: MemorySource {
                cgroup: None,
                limit_bytes: None,
            },
            stop_file,
        }
    }
}

fn job(name: &str, seconds: u64, estimate_mib: Option<u64>) -> Job {
    Job {
        target: TargetId::test("fake", name),
        class: TestClass::Slow,
        expected: Duration::from_secs(seconds),
        test_threads: None,
        estimate_mib,
        exclusive: false,
        timeout: Some(Duration::from_secs(120)),
    }
}

fn fake(name: &str, sleep_ms: u64) -> Fake {
    Fake {
        name: name.to_string(),
        sleep_ms,
        ..Fake::default()
    }
}

fn result_of<'a>(outcome: &'a LaneOutcome, name: &str) -> &'a gate::TargetResult {
    outcome
        .results
        .iter()
        .find(|r| r.target == TargetId::test("fake", name))
        .unwrap_or_else(|| panic!("{name} has no result"))
}

/// Whether `pid` is alive (present and not a zombie).
fn alive(pid: u32) -> bool {
    read_process_table()
        .get(&pid)
        .is_some_and(|info| info.state != 'Z')
}

/// The pids a fake wrote into its marker.
fn marker_pids(path: &Path) -> Vec<u32> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

#[test]
fn three_lanes_overlap_and_each_target_writes_only_its_own_log() -> Result<(), GateError> {
    let names = ["a", "b", "c"];
    let sandbox = Sandbox::new(&names.map(|n| fake(n, 1_500)));
    let jobs = names.iter().map(|n| job(n, 2, Some(10))).collect();
    let started = Instant::now();
    let outcome = run_targets(
        &sandbox.lane_run(None),
        Scheduler::new(3, jobs, MemoryLimits::UNLIMITED),
    );
    assert!(
        started.elapsed() < Duration::from_millis(4_000),
        "three 1.5 s targets on three lanes take about 1.5 s, not {:?}",
        started.elapsed()
    );
    assert_eq!(outcome.results.len(), 3);
    for name in names {
        let result = result_of(&outcome, name);
        assert_eq!(result.outcome, Outcome::Passed, "{name}");
        let log = std::fs::read_to_string(&result.log).map_err(|source| GateError::Io {
            path: result.log.clone(),
            source,
        })?;
        assert!(log.contains(&format!("fake {name} done")), "{log}");
        for other in names.iter().filter(|o| **o != name) {
            assert!(
                !log.contains(&format!("fake {other} ")),
                "{name}'s log: {log}"
            );
        }
    }
    let lanes: std::collections::BTreeSet<String> = outcome
        .results
        .iter()
        .filter_map(|r| r.lane.as_ref().map(|l| l.lane.to_string()))
        .collect();
    assert_eq!(lanes.len(), 3, "{lanes:?}");
    Ok(())
}

#[test]
fn the_peak_of_a_process_tree_counts_a_grandchild_in_another_process_group() {
    let sandbox = Sandbox::new(&[Fake {
        grandchild_mib: 300,
        ..fake("tree", 3_000)
    }]);
    let outcome = run_targets(
        &sandbox.lane_run(None),
        Scheduler::new(1, vec![job("tree", 3, Some(10))], MemoryLimits::UNLIMITED),
    );
    let result = result_of(&outcome, "tree");
    assert_eq!(result.outcome, Outcome::Passed);
    let peak = result.lane.as_ref().and_then(|l| l.peak_anon_mib);
    assert!(peak.is_some_and(|mib| mib >= 250), "peak {peak:?}");
}

#[test]
fn a_target_past_its_timeout_is_killed_with_its_whole_tree_and_reported_timed_out() {
    let sandbox = Sandbox::with(|root| {
        vec![Fake {
            grandchild_mib: 1,
            marker: Some(root.join("pids")),
            ..fake("hang", 50_000)
        }]
    });
    let marker = sandbox.path("pids");
    let mut hang = job("hang", 1, Some(10));
    hang.timeout = Some(Duration::from_secs(2));
    let started = Instant::now();
    let outcome = run_targets(
        &sandbox.lane_run(None),
        Scheduler::new(1, vec![hang], MemoryLimits::UNLIMITED),
    );
    assert_eq!(result_of(&outcome, "hang").outcome, Outcome::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
    let pids = marker_pids(&marker);
    assert_eq!(
        pids.len(),
        2,
        "the fake and its grandchild wrote their pids"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while pids.iter().any(|pid| alive(*pid)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    for pid in pids {
        assert!(
            !alive(pid),
            "process {pid} of the killed tree is still alive"
        );
    }
}

#[test]
fn a_stop_request_kills_running_targets_skips_the_rest_and_writes_no_stamp() {
    let names = ["s1", "s2", "s3", "s4"];
    let sandbox = Sandbox::new(&names.map(|n| fake(n, 40_000)));
    let stop = sandbox.path("gate.lock.stop");
    let jobs = names.iter().map(|n| job(n, 40, Some(10))).collect();
    let writer = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1_500));
            std::fs::write(&stop, stop_request(std::process::id()))
        })
    };
    let started = Instant::now();
    let outcome = run_targets(
        &sandbox.lane_run(Some(stop.clone())),
        Scheduler::new(2, jobs, MemoryLimits::UNLIMITED),
    );
    writer
        .join()
        .expect("invariant: the writer thread does not panic")
        .expect("invariant: the sandbox is writable");
    assert!(outcome.stopped);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(outcome.results.len(), 4);
    assert!(outcome
        .results
        .iter()
        .all(|r| r.outcome == Outcome::Skipped));
    let ran = outcome.results.iter().filter(|r| r.lane.is_some()).count();
    assert_eq!(ran, 2, "two were running, two never started");
    // A request naming another process is not ours.
    std::fs::write(&stop, stop_request(1)).expect("invariant: writable");
    let other = run_targets(
        &sandbox.lane_run(Some(stop)),
        Scheduler::new(
            1,
            vec![{
                let mut quick = job("s1", 1, Some(10));
                quick.timeout = Some(Duration::from_secs(1));
                quick
            }],
            MemoryLimits::UNLIMITED,
        ),
    );
    assert!(!other.stopped);
}

#[test]
fn a_failed_target_is_rerun_alone_after_the_lanes_finish() -> Result<(), GateError> {
    let sandbox = Sandbox::with(|root| {
        vec![
            Fake {
                fail_once: Some(root.join("failed-once")),
                ..fake("wobbly", 300)
            },
            fake("steady", 1_000),
        ]
    });
    let jobs = vec![job("wobbly", 1, Some(10)), job("steady", 1, Some(10))];
    let run = sandbox.lane_run(None);
    let first = run_targets(
        &run,
        Scheduler::new(3, jobs.clone(), MemoryLimits::UNLIMITED),
    );
    assert!(matches!(
        result_of(&first, "wobbly").outcome,
        Outcome::Failed { .. }
    ));
    let reruns = rerun_alone(&run, &jobs, &first.results)?
        .expect("invariant: one target failed, so there is a rerun");
    assert_eq!(reruns.results.len(), 1, "only the failed target reruns");
    let rerun = &reruns.results[0];
    assert_eq!(rerun.outcome, Outcome::Passed);
    assert!(rerun.log.starts_with(run.log_dir.join("rerun")));
    let mut results = first.results.clone();
    mark_flakes(&mut results, &reruns.results);
    assert!(results
        .iter()
        .any(|r| matches!(r.outcome, Outcome::Flaky { .. })));
    Ok(())
}

#[test]
fn progress_lines_are_whole_lines_and_lanes_tsv_lists_every_target() -> Result<(), GateError> {
    let names = ["p1", "p2", "p3", "p4"];
    if std::env::var_os(PRINT_VAR).is_some() {
        let sandbox = Sandbox::new(&names.map(|n| fake(n, 400)));
        let jobs = names.iter().map(|n| job(n, 1, Some(10))).collect();
        let outcome = run_targets(
            &sandbox.lane_run(None),
            Scheduler::new(3, jobs, MemoryLimits::UNLIMITED),
        );
        print!("TSV-BEGIN\n{}TSV-END\n", lanes_tsv(&outcome.results));
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|source| GateError::Io {
        path: PathBuf::from("current_exe"),
        source,
    })?;
    let output = Command::new(exe)
        .args([
            "progress_lines_are_whole_lines_and_lanes_tsv_lists_every_target",
            "--exact",
            "--nocapture",
        ])
        .env(PRINT_VAR, "1")
        .output()
        .map_err(|source| GateError::Spawn {
            program: "lane_run_tests".to_string(),
            source,
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    let starts: Vec<&str> = stdout.lines().filter(|l| l.contains("[start]")).collect();
    let finishes: Vec<&str> = stdout.lines().filter(|l| l.contains("/4] ")).collect();
    assert_eq!(starts.len(), 4, "{stdout}");
    assert_eq!(finishes.len(), 4, "{stdout}");
    for line in starts {
        assert!(
            line.starts_with("[start] L"),
            "a whole start line: {line:?}"
        );
        assert!(line.ends_with(" s)"), "a whole start line: {line:?}");
    }
    for line in finishes {
        assert!(line.starts_with('['), "a whole finish line: {line:?}");
        assert!(line.contains(" PASS "), "{line:?}");
        assert!(line.contains("fake/p"), "{line:?}");
    }
    let tsv = stdout
        .split("TSV-BEGIN\n")
        .nth(1)
        .and_then(|rest| rest.split("TSV-END").next())
        .unwrap_or_default();
    let rows: Vec<Vec<&str>> = tsv
        .lines()
        .skip(1)
        .map(|l| l.split('\t').collect())
        .collect();
    assert_eq!(rows.len(), 4, "{tsv}");
    assert!(rows.iter().all(|row| row.len() == 9), "{tsv}");
    let mut listed: Vec<&str> = rows.iter().map(|row| row[0]).collect();
    listed.sort_unstable();
    assert_eq!(listed, ["fake/p1", "fake/p2", "fake/p3", "fake/p4"]);
    let starts: Vec<f64> = rows.iter().filter_map(|row| row[2].parse().ok()).collect();
    assert!(
        starts.windows(2).all(|w| w[0] <= w[1]),
        "sorted by start: {tsv}"
    );
    Ok(())
}
