//! End-to-end tests of the gate lock through the real `gate` binary.
//!
//! # Responsibility
//! Proves what `src/gate_lock.rs` promises at the process level (blueprint
//! `docs/plans/2026-11-04-tiered-test-gates.md`, Amendment 2): a tier waits while another holder
//! has the lock and prints one line naming it; it proceeds once the lock is released; a gate
//! started inside a gate (marker [`HELD_VAR`] naming the same file) or under `flock` on the lock
//! does not wait for its ancestor; and `explain`, which builds nothing, never takes the lock.
//! It also proves `cargo gate stop` (plan `docs/plans/2026-11-11-faster-gates.md` §3.4): with
//! no holder it says so; with one it writes a request the holder obeys, ending red without a
//! stamp; a stale request is removed by the next holder; and a new holder warns about live
//! processes of the previous holder's group.
//!
//! # How it fits
//! Each test uses a private lock file, pointed to by [`LOCK_PATH_VAR`], so it never touches the
//! user's real `~/scratch/gate.lock`. A competing holder is a separate `flock(1)` process, a
//! sibling of the gate child, because a holder in the child's own ancestry is deliberately not
//! waited for. The binary runs in an empty directory that is no Git repository, so once it has
//! the lock it stops at once with an error (exit 2) instead of starting a build.
//!
//! # Invariants
//! - Every child is bounded: a test that cannot observe what it waits for fails within
//!   [`PATIENCE`] and kills the child, never hangs.
//! - Every child runs in its own process group, and dropping it kills the whole group, so no
//!   grandchild (the `sleep` under `flock`, a `gate` under a `flock` wrapper) outlives its test
//!   while holding a descriptor it inherited, such as the real gate lock of a `cargo gate` run.

#![allow(clippy::expect_used, clippy::panic)]

use gate::{
    group_members, read_process_table, stale_group_warning, stop_path, stop_request, GateLock,
    Holder, HELD_VAR, LOCK_PATH_VAR,
};
use std::ffi::OsStr;
use std::fs::{OpenOptions, TryLockError};
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// How long a child gets to show the behaviour a test waits for.
const PATIENCE: Duration = Duration::from_secs(30);

/// Exit status of the gate outside a Git repository, once it has passed the lock.
const NOT_A_REPOSITORY: i32 = 2;

/// How long the holder's `sleep` lasts if nothing kills it: a backstop, not a timing. Every test
/// is done with its holder well inside it, and a holder that was never dropped (a test process
/// killed from outside) still lets go of what it inherited within a minute.
const HOLDER_BACKSTOP_SECONDS: &str = "60";

/// The `flock(1)` program; util-linux ships it on every host the charter names.
fn flock_program() -> &'static Path {
    ["/usr/bin/flock", "/bin/flock"]
        .into_iter()
        .map(Path::new)
        .find(|path| path.exists())
        .unwrap_or_else(|| panic!("flock(1) from util-linux is required (charter §7)"))
}

/// A `gate` child, possibly under a wrapper, with its standard error read on a thread.
struct GateChild {
    child: Child,
    stderr: Receiver<String>,
}

impl GateChild {
    /// Starts `[wrapper…] gate <args>` in `dir`, with the lock at `lock` and the marker `marker`.
    fn start(
        wrapper: &[&OsStr],
        args: &[&str],
        dir: &Path,
        lock: &Path,
        marker: Option<&Path>,
    ) -> Self {
        let gate = Path::new(env!("CARGO_BIN_EXE_gate"));
        let mut command = match wrapper.split_first() {
            Some((program, rest)) => {
                let mut command = Command::new(program);
                command.args(rest).arg(gate);
                command
            }
            None => Command::new(gate),
        };
        command
            .args(args)
            .current_dir(dir)
            .process_group(0)
            .env(LOCK_PATH_VAR, lock)
            .env_remove(HELD_VAR)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if let Some(marker) = marker {
            command.env(HELD_VAR, marker);
        }
        let mut child = command
            .spawn()
            .expect("invariant: the gate binary was built and flock(1) exists");
        let stderr = child
            .stderr
            .take()
            .expect("invariant: stderr was piped above");
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stderr: receiver,
        }
    }

    /// The next line of standard error within `wait`, if any.
    fn next_line(&self, wait: Duration) -> Option<String> {
        self.stderr.recv_timeout(wait).ok()
    }

    /// The exit status within `wait`, killing the child if it is still running.
    fn exit_within(&mut self, wait: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        kill_group(&mut self.child);
        None
    }

    /// Fails unless the child exits within [`PATIENCE`] as a gate outside a repository does,
    /// without ever printing the waiting line.
    fn assert_ran_without_waiting(&mut self, why: &str) {
        let status = self.exit_within(PATIENCE);
        let lines: Vec<String> = self.stderr.try_iter().collect();
        assert_eq!(
            status.and_then(|status| status.code()),
            Some(NOT_A_REPOSITORY),
            "{why}: {lines:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("waiting for the gate lock")),
            "{why}: {lines:?}"
        );
    }
}

impl Drop for GateChild {
    fn drop(&mut self) {
        kill_group(&mut self.child);
    }
}

/// Kills every process in the group `child` leads, then reaps `child`.
///
/// `child` must have been spawned with `process_group(0)`, so its process ID is the group ID.
/// Killing only `child` would orphan its descendants, which keep every descriptor they
/// inherited. `kill(1)` signals the group because the workspace forbids `unsafe`, which a direct
/// `libc::kill` would need. A group that has already exited makes `kill` fail harmlessly.
fn kill_group(child: &mut Child) {
    let group = format!("-{}", child.id());
    let _ = Command::new("kill")
        .args(["-KILL", "--", &group])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.wait();
}

/// Live (not zombie) processes whose process group is `group`, read from `/proc`.
fn live_members_of_group(group: u32) -> Vec<u32> {
    let group = group.to_string();
    let entries = std::fs::read_dir("/proc")
        .unwrap_or_else(|error| panic!("/proc is required to observe process groups: {error}"));
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            // `stat` reads `pid (comm) state ppid pgrp ...`; `comm` may hold spaces and
            // parentheses, so the fields are counted after its last closing parenthesis.
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                return false;
            };
            let Some((_, after_comm)) = stat.rsplit_once(')') else {
                return false;
            };
            let fields: Vec<&str> = after_comm.split_whitespace().collect();
            fields.first() != Some(&"Z") && fields.get(2) == Some(&group.as_str())
        })
        .collect()
}

/// Another process tree holding the lock: `flock -o <lock> sleep`, a sibling of the gate
/// children, never their ancestor. `-o` keeps the lock out of `sleep`, so killing `flock`
/// releases the lock; dropping this kills the whole process group, `sleep` included, so nothing
/// either inherited outlives the test.
struct OtherHolder {
    child: Child,
}

impl OtherHolder {
    /// Takes `lock` in a new `flock` process and writes the holder record a gate running `full`
    /// in `worktree` would write.
    fn take(lock: &Path, worktree: &Path) -> Self {
        let child = Command::new(flock_program())
            .arg("-o")
            .arg(lock)
            .args(["sleep", HOLDER_BACKSTOP_SECONDS])
            .process_group(0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("invariant: flock(1) exists, checked by flock_program");
        let holder = Self { child };
        let deadline = Instant::now() + PATIENCE;
        loop {
            let probe = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(lock)
                .expect("invariant: the temporary directory is writable");
            match probe.try_lock() {
                Err(TryLockError::WouldBlock) => break,
                Ok(()) => drop(probe),
                Err(TryLockError::Error(error)) => panic!("cannot probe the lock: {error}"),
            }
            assert!(Instant::now() < deadline, "flock did not take the lock");
            std::thread::sleep(Duration::from_millis(20));
        }
        let record = Holder {
            pid: holder.pid(),
            tier: "full".to_string(),
            worktree: worktree.to_path_buf(),
            started_unix_s: 0,
            pgid: None,
        };
        std::fs::write(lock, record.to_record()).expect("invariant: the lock file is writable");
        holder
    }

    /// The holder's process ID.
    fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for OtherHolder {
    fn drop(&mut self) {
        kill_group(&mut self.child);
    }
}

/// A private lock file and an empty working directory outside any Git repository.
fn sandbox() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().expect("invariant: the temporary directory is writable");
    let lock = dir.path().join("gate.lock");
    let work = dir.path().join("work");
    std::fs::create_dir(&work).expect("invariant: the temporary directory is writable");
    (dir, lock, work)
}

#[test]
fn a_tier_waits_for_the_holder_names_it_and_runs_after_release() {
    let (_dir, lock, work) = sandbox();
    let held = OtherHolder::take(&lock, &work);
    let mut gate = GateChild::start(&[], &["package"], &work, &lock, None);
    let line = gate
        .next_line(PATIENCE)
        .expect("the waiting gate printed no line");
    assert!(
        line.starts_with("cargo gate: waiting for the gate lock")
            && line.contains(&format!("pid {} (cargo gate full in", held.pid())),
        "unexpected waiting line: {line}"
    );
    assert!(
        gate.exit_within(Duration::from_secs(1)).is_none(),
        "the gate ran while another holder had the lock"
    );
    let mut gate = GateChild::start(&[], &["package"], &work, &lock, None);
    gate.next_line(PATIENCE)
        .expect("the second waiting gate printed no line");
    drop(held);
    let status = gate
        .exit_within(PATIENCE)
        .expect("the gate did not proceed after the lock was released");
    assert_eq!(status.code(), Some(NOT_A_REPOSITORY));
}

#[test]
fn a_gate_inside_a_gate_does_not_wait_for_its_parent() {
    let (_dir, lock, work) = sandbox();
    // The holder is not the child's ancestor here, so only the marker can let it through.
    let _parent = OtherHolder::take(&lock, &work);
    GateChild::start(&[], &["quick"], &work, &lock, Some(&lock))
        .assert_ran_without_waiting("a nested gate waited for the lock its parent holds");
}

#[test]
fn a_gate_started_under_flock_on_the_lock_runs_inside_that_hold() {
    let (_dir, lock, work) = sandbox();
    let wrapper = [flock_program().as_os_str(), lock.as_os_str()];
    GateChild::start(&wrapper, &["package"], &work, &lock, None)
        .assert_ran_without_waiting("a gate under `flock` on its own lock waited for flock");
}

#[test]
fn a_lock_held_by_the_process_that_started_the_gate_is_not_waited_for() {
    let (_dir, lock, work) = sandbox();
    let _held = GateLock::acquire(&lock, None, &Holder::this_process("full", &work), |_| {})
        .expect("invariant: the private lock is free");
    GateChild::start(&[], &["package"], &work, &lock, None)
        .assert_ran_without_waiting("a gate waited for the lock its parent process holds");
}

#[test]
fn explain_never_takes_the_lock() {
    let (_dir, lock, work) = sandbox();
    let _held = OtherHolder::take(&lock, &work);
    let mut explain = GateChild::start(&[], &["explain"], &work, &lock, None);
    let status = explain.exit_within(PATIENCE);
    let lines: Vec<String> = explain.stderr.try_iter().collect();
    assert!(status.is_some(), "explain waited for the gate lock");
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("waiting for the gate lock")),
        "{lines:?}"
    );
}

#[test]
fn dropping_the_holder_leaves_no_process_of_its_group() {
    let (_dir, lock, work) = sandbox();
    let held = OtherHolder::take(&lock, &work);
    let group = held.pid();
    // `flock` forks `sleep` once it has the lock, so the group soon holds both.
    let deadline = Instant::now() + PATIENCE;
    while live_members_of_group(group).len() < 2 {
        assert!(Instant::now() < deadline, "flock never started its sleep");
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(held);
    // A killed `sleep` stays a zombie until its new parent reaps it; zombies hold no
    // descriptors and are not counted, so only the signal's delivery is waited for.
    let deadline = Instant::now() + PATIENCE;
    loop {
        let survivors = live_members_of_group(group);
        if survivors.is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "processes {survivors:?} of the holder's group outlived it"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Runs `gate <args>` in `dir` with the lock at `lock` to completion, within [`PATIENCE`].
fn gate_output(args: &[&str], dir: &Path, lock: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_gate"))
        .args(args)
        .current_dir(dir)
        .env(LOCK_PATH_VAR, lock)
        .env_remove(HELD_VAR)
        .output()
        .expect("invariant: the gate binary was built")
}

#[test]
fn stop_with_no_holder_says_so_and_exits_zero() {
    let (_dir, lock, work) = sandbox();
    let output = gate_output(&["stop"], &work, &lock);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("no gate holds"), "{stdout}");
    assert!(
        !stop_path(&lock).exists(),
        "nothing to ask, nothing written"
    );
}

#[test]
fn a_stale_stop_request_is_removed_when_the_next_gate_takes_the_lock() {
    let (_dir, lock, work) = sandbox();
    std::fs::write(stop_path(&lock), stop_request(1)).expect("invariant: writable");
    let held = GateLock::acquire(&lock, None, &Holder::this_process("full", &work), |_| {})
        .expect("invariant: a free lock is taken at once");
    assert!(!stop_path(&lock).exists(), "the stale request is gone");
    drop(held);
}

#[test]
fn a_new_holder_warns_about_live_processes_of_the_previous_holders_group() {
    let (_dir, lock, work) = sandbox();
    let mut orphan = Command::new("sleep")
        .arg(HOLDER_BACKSTOP_SECONDS)
        .process_group(0)
        .spawn()
        .expect("invariant: sleep(1) exists");
    let group = orphan.id();
    let previous = Holder {
        pid: 999_999,
        tier: "full".to_string(),
        worktree: work.clone(),
        started_unix_s: 0,
        pgid: Some(group),
    };
    // A gate killed by its pid leaves its record behind (it never cleared it).
    std::fs::write(&lock, previous.to_record()).expect("invariant: writable");
    let table = read_process_table();
    let alive = group_members(&table, group);
    assert_eq!(alive, [group]);
    let warning = stale_group_warning(previous.pid, group, &alive)
        .expect("invariant: a live member makes a warning");
    assert!(
        warning.contains(&format!("kill -TERM -- -{group}")),
        "{warning}"
    );
    assert!(warning.contains(&group.to_string()), "{warning}");
    assert_eq!(stale_group_warning(previous.pid, group, &[]), None);
    let mut gate = GateChild::start(&[], &["package"], &work, &lock, None);
    let line = gate
        .next_line(PATIENCE)
        .expect("the new holder must warn about the previous holder's group");
    assert!(line.contains("left processes of its group"), "{line}");
    assert!(line.contains(&group.to_string()), "{line}");
    assert_eq!(
        gate.exit_within(PATIENCE).and_then(|status| status.code()),
        Some(NOT_A_REPOSITORY),
        "it warns and goes on, killing nothing"
    );
    assert!(
        read_process_table().contains_key(&group),
        "nothing was killed"
    );
    orphan.kill().ok();
    orphan.wait().ok();
}

/// A Git repository with one crate whose only test sleeps for a minute, and the catalog the gate
/// needs, committed so the tree is clean.
fn slow_repository(root: &Path) {
    let write = |path: &str, text: &str| {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().expect("invariant: a file has a parent"))
            .expect("invariant: writable");
        std::fs::write(full, text).expect("invariant: writable");
    };
    write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/slow\"]\nresolver = \"2\"\n",
    );
    write(
        "crates/slow/Cargo.toml",
        "[package]\nname = \"slow\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        "crates/slow/src/lib.rs",
        "//! A crate whose test waits.\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn waits() {\n        std::thread::sleep(std::time::Duration::from_secs(60));\n    }\n}\n",
    );
    write(
        "crates/gate/suites.ron",
        "SuiteCatalog(global: [], docs: [])\n",
    );
    write(".gitignore", "/target\n");
    let run = |program: &str, args: &[&str]| {
        let status = Command::new(program)
            .args(args)
            .current_dir(root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("invariant: git and cargo exist");
        assert!(status.success(), "{program} {args:?} failed");
    };
    run("cargo", &["generate-lockfile", "--offline"]);
    run("git", &["init", "--quiet"]);
    run("git", &["add", "-A"]);
    run(
        "git",
        &[
            "-c",
            "user.name=gate",
            "-c",
            "user.email=gate@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "slow",
        ],
    );
}

#[test]
fn stop_writes_a_request_naming_the_holder_and_the_holder_ends_red_without_a_stamp() {
    let (dir, lock, work) = sandbox();
    slow_repository(&work);
    let stamps = dir.path().join("stamps");
    let logs = dir.path().join("logs");
    let mut child = Command::new(env!("CARGO_BIN_EXE_gate"))
        .args(["full", "--stamp-dir"])
        .arg(&stamps)
        .arg("--log-dir")
        .arg(&logs)
        .current_dir(&work)
        .process_group(0)
        .env(LOCK_PATH_VAR, &lock)
        .env("CARGO_TARGET_DIR", dir.path().join("target"))
        .env_remove(HELD_VAR)
        .env_remove("SPACE_SCUFFLE_GATE_JOBS")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("invariant: the gate binary was built");
    let stdout = child.stdout.take().expect("invariant: stdout was piped");
    let (sender, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut seen = Vec::new();
    while !seen.iter().any(|line: &String| line.starts_with("[start]")) {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(line) = lines.recv_timeout(left) else {
            kill_group(&mut child);
            panic!("the gate never started its test: {seen:#?}");
        };
        seen.push(line);
    }
    let output = gate_output(&["stop"], &work, &lock);
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{said}");
    assert!(
        said.contains(&format!("asked gate {}", child.id())),
        "{said}"
    );
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("invariant: the child is ours") {
            break status;
        }
        if started.elapsed() > PATIENCE {
            kill_group(&mut child);
            panic!("the gate did not stop within {PATIENCE:?}");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(status.code(), Some(2), "a stopped gate exits with status 2");
    let rest: Vec<String> = lines.try_iter().collect();
    assert!(
        rest.iter().any(|line| line.contains("RED (stopped")),
        "{rest:#?}"
    );
    let written = std::fs::read_dir(&stamps).map_or(0, Iterator::count);
    assert_eq!(written, 0, "a stopped run writes no stamp");
    assert!(
        !stop_path(&lock).exists(),
        "the answered request is removed"
    );
}
