//! The gate lock: one `cargo gate` run at a time for the whole user, across all worktrees.
//!
//! # Responsibility
//! Owns [`GateLock`], an exclusive `flock(2)` on one file (by default
//! `$HOME/scratch/gate.lock`, overridden by [`LOCK_PATH_VAR`]) that a gate tier takes before its
//! first build or test step and releases when it ends. While it waits it prints one line naming
//! the holder; once it holds the lock it records itself as the holder ([`Holder`]).
//!
//! # Why
//! Four concurrent gates in separate target directories once pushed the user's cgroup over its
//! `memory.high` limit (44 GiB, no swap): the kernel throttled the whole slice and the server
//! froze (owner's incident report, 2026-11-05; blueprint
//! `docs/plans/2026-11-04-tiered-test-gates.md`, Amendment 2). Each gate is already sized for
//! the whole machine, so gates must run one after another, not side by side.
//!
//! # How it fits
//! `src/main.rs` takes the lock for `quick`, `package`, `full` and `merge`, and never for
//! `explain`, `timings` or `suspects`, which build and run nothing. After taking it, `main.rs`
//! sets [`HELD_VAR`] to the lock's path in its own environment, so every process the gate
//! starts inherits it. A `cargo gate` started inside a gate (a test that runs the tool, say)
//! sees that marker for the same path and runs without locking instead of waiting forever for
//! its own parent. `flock(1)` takes part in the same protocol, so
//! `flock ~/scratch/gate.lock cargo test --workspace` serializes with gates too, and a gate
//! started under such a `flock` finds its ancestor in `/proc/locks` and runs inside that hold.
//!
//! # Invariants
//! - The lock is held from [`GateLock::acquire`] until the [`GateLock`] is dropped; the kernel
//!   releases it if the process dies, so a crashed gate never leaves a stale lock.
//! - The holder record names the process that holds the lock or is empty: it is written only
//!   after the lock is taken and cleared before it is released.
//! - A gate that takes the lock deletes any stop request beside it (`<lock>.stop`): a request
//!   names the holder it was written for, which has ended. It warns, without killing anything,
//!   when processes of the previous holder's process group are still alive (plan
//!   2026-11-11-faster-gates §3.4).
//! - A nested call skips the lock only when the marker names the very file it would lock, or
//!   when `/proc/locks` names one of its ancestors as the holder (a gate started under
//!   `flock ~/scratch/gate.lock`); it never skips a lock another process tree holds.

use std::ffi::OsStr;
use std::fmt;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::GateError;

/// Variable that overrides the lock file's path.
pub const LOCK_PATH_VAR: &str = "SPACE_SCUFFLE_GATE_LOCK";

/// Variable a gate that holds the lock sets to the lock's path, for the processes it starts.
pub const HELD_VAR: &str = "SPACE_SCUFFLE_GATE_LOCK_HELD";

/// The lock file's path below `$HOME` when [`LOCK_PATH_VAR`] is unset.
const DEFAULT_RELATIVE_PATH: &str = "scratch/gate.lock";

/// Who holds the lock: written into the lock file, read back by a waiting gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holder {
    /// Process ID of the gate.
    pub pid: u32,
    /// What it runs: a tier name, or `merge <branch>`.
    pub tier: String,
    /// The directory it was started in (its worktree).
    pub worktree: PathBuf,
    /// When it took the lock, in seconds since the Unix epoch.
    pub started_unix_s: u64,
    /// Its process group, which its test executables share; `None` in records written before
    /// the field existed or where `/proc` is missing.
    pub pgid: Option<u32>,
}

impl Holder {
    /// The holder record of this process, running `tier` in `worktree`, as of now.
    #[must_use]
    pub fn this_process(tier: &str, worktree: &Path) -> Self {
        Self {
            pid: std::process::id(),
            tier: tier.to_string(),
            worktree: worktree.to_path_buf(),
            started_unix_s: unix_now(),
            pgid: crate::memory::own_process_group(),
        }
    }

    /// The record as one tab-separated line: `pid=…\ttier=…\tworktree=…\tstarted=…`, then
    /// `\tpgid=…` when the process group is known.
    #[must_use]
    pub fn to_record(&self) -> String {
        let pgid = self
            .pgid
            .map_or_else(String::new, |pgid| format!("\tpgid={pgid}"));
        format!(
            "pid={}\ttier={}\tworktree={}\tstarted={}{pgid}\n",
            self.pid,
            self.tier,
            self.worktree.display(),
            self.started_unix_s
        )
    }

    /// Parses a record written by [`Self::to_record`]; `None` for an empty or foreign file.
    #[must_use]
    pub fn parse(record: &str) -> Option<Self> {
        let line = record.lines().next()?;
        let mut pid = None;
        let mut tier = None;
        let mut worktree = None;
        let mut started = None;
        let mut pgid = None;
        for field in line.split('\t') {
            let (key, value) = field.split_once('=')?;
            match key {
                "pid" => pid = value.parse().ok(),
                "tier" => tier = Some(value.to_string()),
                "worktree" => worktree = Some(PathBuf::from(value)),
                "started" => started = value.parse().ok(),
                "pgid" => pgid = value.parse().ok(),
                _ => return None,
            }
        }
        Some(Self {
            pid: pid?,
            tier: tier?,
            worktree: worktree?,
            started_unix_s: started?,
            pgid,
        })
    }
}

/// Seconds since the Unix epoch; zero on a clock before 1970.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// The process `/proc/locks` names as holding a lock, with its command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockOwner {
    /// Its process ID.
    pub pid: u32,
    /// Its command line, arguments separated by spaces; empty if unreadable.
    pub command_line: String,
}

/// The one-line message a gate prints while it waits for the lock at `path`.
///
/// `holder` is the record read from the file, if any; `now_unix_s` dates its age. A lock taken
/// by `flock(1)` carries no record; then `owner`, the process `/proc/locks` names, is shown.
#[must_use]
pub fn waiting_message(
    path: &Path,
    holder: Option<&Holder>,
    owner: Option<&LockOwner>,
    now_unix_s: u64,
) -> String {
    let unrecorded = || match owner {
        Some(owner) => format!("pid {} (`{}`)", owner.pid, owner.command_line),
        None => "a process that left no holder record".to_string(),
    };
    let who = holder.map_or_else(unrecorded, |holder| {
        format!(
            "pid {} (cargo gate {} in {}, for {} s)",
            holder.pid,
            holder.tier,
            holder.worktree.display(),
            now_unix_s.saturating_sub(holder.started_unix_s)
        )
    });
    format!(
        "cargo gate: waiting for the gate lock {} held by {who}; one gate runs at a time \
         (charter §7)",
        path.display()
    )
}

/// The lock file's path: [`LOCK_PATH_VAR`] if set, else `$HOME/scratch/gate.lock`.
///
/// # Errors
/// [`GateError::Usage`] when neither the variable nor `HOME` is set.
pub fn lock_path() -> Result<PathBuf, GateError> {
    if let Some(path) = std::env::var_os(LOCK_PATH_VAR) {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| {
        GateError::Usage(format!(
            "HOME is not set; set {LOCK_PATH_VAR} to the gate lock"
        ))
    })?;
    Ok(PathBuf::from(home).join(DEFAULT_RELATIVE_PATH))
}

/// How a [`GateLock`] came to be held.
#[derive(Debug)]
enum Hold {
    /// This process took the lock through this open file.
    Owned(File),
    /// A parent gate holds it; this nested call takes nothing (see [`HELD_VAR`]).
    Inherited,
}

/// The held gate lock. Dropping it clears the holder record and releases the lock.
#[derive(Debug)]
pub struct GateLock {
    /// The lock file.
    path: PathBuf,
    /// Whether this process owns the lock or inherits it from a parent gate.
    hold: Hold,
}

impl GateLock {
    /// Takes the lock at `path` for `holder`, waiting as long as another process holds it.
    ///
    /// `held_marker` is the value of [`HELD_VAR`] in this process: when it names `path`, a
    /// parent gate holds the lock and this call returns at once without locking. Before it
    /// blocks, `on_wait` receives the [`waiting_message`] once.
    ///
    /// # Errors
    /// [`GateError::Io`] when the file cannot be created, locked or written.
    pub fn acquire(
        path: &Path,
        held_marker: Option<&OsStr>,
        holder: &Holder,
        on_wait: impl FnOnce(&str),
    ) -> Result<Self, GateError> {
        let inherited = || Self {
            path: path.to_path_buf(),
            hold: Hold::Inherited,
        };
        if held_marker.is_some_and(|marker| Path::new(marker) == path) {
            return Ok(inherited());
        }
        let io = |source| GateError::Io {
            path: path.to_path_buf(),
            source,
        };
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        // Not truncated on open: the file is the current holder's record until the lock is ours.
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(io)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                let owner = lock_owner(&file);
                // `flock ~/scratch/gate.lock cargo gate …` holds the lock in an ancestor that set
                // no marker; waiting for it would wait forever.
                if owner.as_ref().is_some_and(|owner| is_ancestor(owner.pid)) {
                    return Ok(inherited());
                }
                let mut record = String::new();
                // An unreadable record only weakens the message; the wait is unaffected.
                if file.read_to_string(&mut record).is_err() {
                    record.clear();
                }
                on_wait(&waiting_message(
                    path,
                    Holder::parse(&record).as_ref(),
                    owner.as_ref(),
                    unix_now(),
                ));
                file.lock().map_err(io)?;
            }
            Err(TryLockError::Error(source)) => return Err(io(source)),
        }
        let mut previous = String::new();
        // An unreadable record only loses the warning about the previous holder's processes.
        if file.seek(SeekFrom::Start(0)).is_err() || file.read_to_string(&mut previous).is_err() {
            previous.clear();
        }
        warn_about_previous_group(Holder::parse(&previous).as_ref(), holder);
        // A stop request beside the lock was written for an earlier holder: stale.
        match std::fs::remove_file(crate::execute::stop_path(path)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(io(source)),
        }
        file.set_len(0).map_err(io)?;
        file.seek(SeekFrom::Start(0)).map_err(io)?;
        file.write_all(holder.to_record().as_bytes()).map_err(io)?;
        file.flush().map_err(io)?;
        Ok(Self {
            path: path.to_path_buf(),
            hold: Hold::Owned(file),
        })
    }

    /// The lock file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether a parent gate holds the lock and this one took nothing.
    #[must_use]
    pub fn is_inherited(&self) -> bool {
        matches!(self.hold, Hold::Inherited)
    }
}

/// Prints a warning when processes of `previous`'s process group are still alive: a gate killed
/// by its PID alone leaves its test executables running (plan §3.4). Nothing is killed.
fn warn_about_previous_group(previous: Option<&Holder>, current: &Holder) {
    let Some((pid, pgid)) = previous.and_then(|p| p.pgid.map(|pgid| (p.pid, pgid))) else {
        return;
    };
    if Some(pgid) == current.pgid {
        return;
    }
    let table = crate::memory::read_process_table();
    let alive = crate::memory::group_members(&table, pgid);
    if let Some(message) = stale_group_warning(pid, pgid, &alive) {
        eprintln!("{message}");
    }
}

/// The warning about the live processes `alive` of the previous holder `pid`'s group `pgid`,
/// or `None` when none is alive.
#[must_use]
pub fn stale_group_warning(pid: u32, pgid: u32, alive: &[u32]) -> Option<String> {
    if alive.is_empty() {
        return None;
    }
    let pids: Vec<String> = alive.iter().map(ToString::to_string).collect();
    Some(format!(
        "cargo gate: the previous gate (pid {pid}) left processes of its group {pgid} running: \
         {}. They no longer hold the lock but still use memory and the GPU; end them with \
         `kill -TERM -- -{pgid}`",
        pids.join(" ")
    ))
}

/// The gate holding the lock at `path`, without waiting: `None` when no process holds it.
///
/// # Errors
/// [`GateError::Io`] when the file exists but cannot be opened or read.
pub fn current_holder(path: &Path) -> Result<Option<Holder>, GateError> {
    let io = |source| GateError::Io {
        path: path.to_path_buf(),
        source,
    };
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(io(source)),
    };
    match file.try_lock_shared() {
        // Free: nobody holds it. The shared lock ends when `file` closes.
        Ok(()) => Ok(None),
        Err(TryLockError::WouldBlock) => {
            let mut record = String::new();
            file.read_to_string(&mut record).map_err(io)?;
            Ok(Holder::parse(&record))
        }
        Err(TryLockError::Error(source)) => Err(io(source)),
    }
}

/// The process holding `file`'s `flock`, from `/proc/locks`, or `None` where it cannot be named.
#[cfg(unix)]
fn lock_owner(file: &File) -> Option<LockOwner> {
    use std::os::unix::fs::MetadataExt as _;
    let metadata = file.metadata().ok()?;
    let (major, minor) = split_device(metadata.dev());
    let locks = std::fs::read_to_string("/proc/locks").ok()?;
    let pid = flock_holder_pid(&locks, major, minor, metadata.ino())?;
    let command_line = std::fs::read(format!("/proc/{pid}/cmdline"))
        .map(|raw| {
            raw.split(|byte| *byte == 0)
                .filter(|part| !part.is_empty())
                .map(|part| String::from_utf8_lossy(part).into_owned())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    Some(LockOwner { pid, command_line })
}

/// Without Unix metadata the holder cannot be named.
#[cfg(not(unix))]
fn lock_owner(_file: &File) -> Option<LockOwner> {
    None
}

/// Splits a Linux `st_dev` into `(major, minor)`, as glibc's `major` and `minor` macros do.
#[must_use]
pub fn split_device(device: u64) -> (u64, u64) {
    let major = ((device & 0x0000_0000_000f_ff00) >> 8) | ((device & 0xffff_f000_0000_0000) >> 32);
    let minor = (device & 0x0000_0000_0000_00ff) | ((device & 0x0000_0fff_fff0_0000) >> 12);
    (major, minor)
}

/// The PID holding a `FLOCK` on the file `major:minor:inode` in `/proc/locks` text.
///
/// Lines look like `3: FLOCK  ADVISORY  WRITE 2533986 08:10:87952619 0 EOF`, with the device in
/// hexadecimal and the inode in decimal. Waiters (`3: -> FLOCK ...`) and other lock kinds are
/// skipped.
#[must_use]
pub fn flock_holder_pid(locks: &str, major: u64, minor: u64, inode: u64) -> Option<u32> {
    locks.lines().find_map(|line| {
        let mut fields = line.split_whitespace().skip(1);
        if fields.next()? != "FLOCK" {
            return None;
        }
        let pid: u32 = fields.nth(2)?.parse().ok()?;
        let mut file = fields.next()?.split(':');
        let line_major = u64::from_str_radix(file.next()?, 16).ok()?;
        let line_minor = u64::from_str_radix(file.next()?, 16).ok()?;
        let line_inode: u64 = file.next()?.parse().ok()?;
        (line_major == major && line_minor == minor && line_inode == inode).then_some(pid)
    })
}

/// The parent PID in the text of `/proc/<pid>/stat`, read after the last `)` because the
/// command name may itself contain spaces and parentheses.
#[must_use]
pub fn stat_parent_pid(stat: &str) -> Option<u32> {
    let (_, after_name) = stat.rsplit_once(')')?;
    after_name.split_whitespace().nth(1)?.parse().ok()
}

/// Whether `pid` is a strict ancestor of this process. This process itself is not: another
/// thread of it may hold the lock, and that one is worth waiting for.
#[cfg(unix)]
fn is_ancestor(pid: u32) -> bool {
    let mut current = std::os::unix::process::parent_id();
    // More steps than any real process tree, so broken input cannot loop forever.
    for _ in 0..4_096 {
        if current == 0 {
            return false;
        }
        if current == pid {
            return true;
        }
        match std::fs::read_to_string(format!("/proc/{current}/stat"))
            .ok()
            .as_deref()
            .and_then(stat_parent_pid)
        {
            Some(parent) => current = parent,
            None => return false,
        }
    }
    false
}

/// Without `/proc` no ancestor can be named.
#[cfg(not(unix))]
fn is_ancestor(_pid: u32) -> bool {
    false
}

impl Drop for GateLock {
    fn drop(&mut self) {
        if let Hold::Owned(file) = &self.hold {
            // Clear the record while the lock is still held, so the next holder never reads a
            // stale one; a failure only leaves a stale record, which the next holder overwrites.
            if file.set_len(0).is_err() {
                eprintln!(
                    "cargo gate: could not clear the holder record in {}",
                    self.path.display()
                );
            }
            // Closing the file, right after this, releases the `flock`.
        }
    }
}

impl fmt::Display for GateLock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.hold {
            Hold::Owned(_) => write!(f, "gate lock {}", self.path.display()),
            Hold::Inherited => write!(
                f,
                "gate lock {} (held by the parent gate)",
                self.path.display()
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn holder(tier: &str) -> Holder {
        Holder {
            pid: 4242,
            tier: tier.to_string(),
            worktree: PathBuf::from("/home/u/repo/.claude/worktrees/agent-x"),
            started_unix_s: 1_000,
            pgid: None,
        }
    }

    #[test]
    fn the_holder_record_round_trips() {
        let record = holder("merge feature").to_record();
        assert_eq!(Holder::parse(&record), Some(holder("merge feature")));
        assert_eq!(Holder::parse(""), None);
        assert_eq!(Holder::parse("something else\n"), None);
    }

    #[test]
    fn the_waiting_message_names_the_holder_or_says_there_is_no_record() {
        let path = Path::new("/home/u/scratch/gate.lock");
        assert_eq!(
            waiting_message(path, Some(&holder("package")), None, 1_042),
            "cargo gate: waiting for the gate lock /home/u/scratch/gate.lock held by pid 4242 \
             (cargo gate package in /home/u/repo/.claude/worktrees/agent-x, for 42 s); one gate \
             runs at a time (charter §7)"
        );
        assert!(waiting_message(path, None, None, 0).contains("left no holder record"));
        let owner = LockOwner {
            pid: 7,
            command_line: "flock /home/u/scratch/gate.lock cargo test".to_string(),
        };
        assert!(waiting_message(path, None, Some(&owner), 0)
            .contains("held by pid 7 (`flock /home/u/scratch/gate.lock cargo test`)"));
    }

    #[test]
    fn proc_locks_and_stat_lines_name_the_holder_and_the_parent() {
        let locks = "1: FLOCK  ADVISORY  WRITE 2299716 08:10:87983988 0 EOF\n\
                     16: FLOCK  ADVISORY  WRITE 2338785 08:10:91510516 0 EOF\n\
                     16: -> FLOCK  ADVISORY  WRITE 2339012 08:10:91510516 0 EOF\n";
        assert_eq!(
            flock_holder_pid(locks, 8, 0x10, 91_510_516),
            Some(2_338_785)
        );
        assert_eq!(flock_holder_pid(locks, 8, 0x10, 1), None);
        assert_eq!(split_device(0x0810), (8, 0x10));
        assert_eq!(stat_parent_pid("42 (a (b) c) S 17 42 42 0"), Some(17));
        assert_eq!(stat_parent_pid("garbage"), None);
    }

    #[test]
    fn a_free_lock_is_taken_at_once_and_records_its_holder() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(|source| GateError::Io {
            path: PathBuf::from("tempdir"),
            source,
        })?;
        let path = dir.path().join("nested/gate.lock");
        let lock = GateLock::acquire(&path, None, &holder("package"), |message| {
            panic!("a free lock must not wait: {message}")
        })?;
        assert!(!lock.is_inherited());
        let record = std::fs::read_to_string(&path).map_err(|source| GateError::Io {
            path: path.clone(),
            source,
        })?;
        assert_eq!(Holder::parse(&record), Some(holder("package")));
        drop(lock);
        let record = std::fs::read_to_string(&path).map_err(|source| GateError::Io {
            path: path.clone(),
            source,
        })?;
        assert!(
            record.is_empty(),
            "released lock left the record {record:?}"
        );
        Ok(())
    }

    #[test]
    fn two_gates_serialize_and_the_second_names_the_first() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(|source| GateError::Io {
            path: PathBuf::from("tempdir"),
            source,
        })?;
        let path = dir.path().join("gate.lock");
        let first = GateLock::acquire(&path, None, &holder("full"), |_| {})?;
        let (sender, receiver) = mpsc::channel();
        let second = std::thread::spawn({
            let path = path.clone();
            move || {
                let waited = sender.clone();
                let lock = GateLock::acquire(&path, None, &holder("package"), move |message| {
                    waited
                        .send(format!("waiting: {message}"))
                        .expect("invariant: the test thread keeps the receiver alive");
                });
                sender
                    .send("acquired".to_string())
                    .expect("invariant: the test thread keeps the receiver alive");
                lock.map(|lock| lock.is_inherited())
            }
        });
        let message = receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("invariant: the second gate reports its wait within 10 s");
        assert!(
            message.starts_with("waiting: ") && message.contains("pid 4242 (cargo gate full"),
            "{message}"
        );
        assert!(
            receiver.recv_timeout(Duration::from_millis(300)).is_err(),
            "the second gate took the lock while the first held it"
        );
        drop(first);
        let acquired = receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("invariant: the second gate takes the lock once it is released");
        assert_eq!(acquired, "acquired");
        let inherited = second
            .join()
            .expect("invariant: the second gate's thread does not panic")?;
        assert!(!inherited);
        Ok(())
    }

    #[test]
    fn a_nested_gate_under_the_marker_does_not_wait_for_its_parent() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(|source| GateError::Io {
            path: PathBuf::from("tempdir"),
            source,
        })?;
        let path = dir.path().join("gate.lock");
        let parent = GateLock::acquire(&path, None, &holder("full"), |_| {})?;
        let nested = GateLock::acquire(&path, Some(path.as_os_str()), &holder("quick"), |m| {
            panic!("a nested gate must not wait for its parent: {m}")
        })?;
        assert!(nested.is_inherited());
        drop(nested);
        let record = std::fs::read_to_string(&path).map_err(|source| GateError::Io {
            path: path.clone(),
            source,
        })?;
        assert_eq!(
            Holder::parse(&record),
            Some(holder("full")),
            "a nested gate must leave the parent's record"
        );
        drop(parent);
        Ok(())
    }

    #[test]
    fn a_marker_for_another_file_does_not_skip_the_lock() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(|source| GateError::Io {
            path: PathBuf::from("tempdir"),
            source,
        })?;
        let path = dir.path().join("gate.lock");
        let other = dir.path().join("other.lock");
        let lock = GateLock::acquire(&path, Some(other.as_os_str()), &holder("quick"), |_| {})?;
        assert!(!lock.is_inherited());
        Ok(())
    }
}
