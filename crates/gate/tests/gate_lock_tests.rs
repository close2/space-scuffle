//! End-to-end tests of the gate lock through the real `gate` binary.
//!
//! # Responsibility
//! Proves what `src/gate_lock.rs` promises at the process level (blueprint
//! `docs/plans/2026-11-04-tiered-test-gates.md`, Amendment 2): a tier waits while another holder
//! has the lock and prints one line naming it; it proceeds once the lock is released; a gate
//! started inside a gate (marker [`HELD_VAR`] naming the same file) or under `flock` on the lock
//! does not wait for its ancestor; and `explain`, which builds nothing, never takes the lock.
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

#![allow(clippy::expect_used, clippy::panic)]

use gate::{GateLock, Holder, HELD_VAR, LOCK_PATH_VAR};
use std::ffi::OsStr;
use std::fs::{OpenOptions, TryLockError};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// How long a child gets to show the behaviour a test waits for.
const PATIENCE: Duration = Duration::from_secs(30);

/// Exit status of the gate outside a Git repository, once it has passed the lock.
const NOT_A_REPOSITORY: i32 = 2;

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
        let _ = self.child.kill();
        let _ = self.child.wait();
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
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Another process tree holding the lock: `flock -o <lock> sleep`, a sibling of the gate
/// children, never their ancestor. `-o` keeps the lock out of `sleep`, so dropping this kills
/// `flock` and so releases the lock.
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
            .args(["sleep", "600"])
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
        let _ = self.child.kill();
        let _ = self.child.wait();
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
