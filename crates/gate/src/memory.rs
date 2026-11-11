//! Memory of running test executables: process trees, their anonymous memory, and the slice's.
//!
//! # Responsibility
//! Reads `/proc` and the cgroup v2 hierarchy for the lane runtime (plan
//! 2026-11-11-faster-gates §3.2 to §3.4): the process table ([`ProcessTable`]: pid, parent,
//! process group, state, `RssAnon`), the descendants of a process whatever their process group,
//! the gate's cgroup limit (walking up to the first ancestor with a numeric `memory.high` or
//! `memory.max`, else `MemTotal`), and that cgroup's live `anon`. A [`Sampler`] thread scans once
//! a second and keeps each running target's peak and the slice's peak.
//!
//! # How it fits
//! `execute.rs` registers each executable's pid with the sampler, reads its peak when it ends,
//! passes the live reading to the scheduler's admission, and kills a timed-out or stopped
//! target's whole tree through [`kill_tree`]. The parsers are pure functions over text, so the
//! unit tests run on fixture text.
//!
//! # Invariants
//! - A target's peak is the largest sum of `RssAnon` over its process and every descendant seen
//!   in one scan; descendants are found through parent pids, so children that start their own
//!   process group (`xvfb-run`, GNU `timeout`) still count.
//! - Every figure is in MiB; a missing reading is `None`, never zero.
//! - Signals go through `kill(1)`: the workspace forbids `unsafe`.

use crate::workspace::TargetId;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

/// One process of the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessInfo {
    /// Its parent's pid.
    pub ppid: u32,
    /// Its process group.
    pub pgid: u32,
    /// Its state letter (`R`, `S`, `Z`, ...).
    pub state: char,
    /// Its anonymous resident memory in KiB, if readable.
    pub rss_anon_kib: Option<u64>,
}

/// Every process of the host, by pid.
pub type ProcessTable = BTreeMap<u32, ProcessInfo>;

/// Parses `/proc/<pid>/stat` text into (ppid, pgid, state).
#[must_use]
pub fn parse_stat(stat: &str) -> Option<(u32, u32, char)> {
    // The command name may contain spaces and parentheses: fields resume after the last `)`.
    let rest = stat.get(stat.rfind(')')? + 1..)?;
    let mut fields = rest.split_whitespace();
    let state = fields.next()?.chars().next()?;
    let ppid = fields.next()?.parse().ok()?;
    let pgid = fields.next()?.parse().ok()?;
    Some((ppid, pgid, state))
}

/// The `RssAnon` line of `/proc/<pid>/status` text, in KiB.
#[must_use]
pub fn parse_rss_anon_kib(status: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        line.strip_prefix("RssAnon:")?
            .trim()
            .strip_suffix("kB")?
            .trim()
            .parse()
            .ok()
    })
}

/// The `anon` figure of a cgroup's `memory.stat` text, in bytes.
#[must_use]
pub fn parse_memory_stat_anon(stat: &str) -> Option<u64> {
    stat.lines()
        .find_map(|line| line.strip_prefix("anon ")?.trim().parse().ok())
}

/// The `MemTotal` of `/proc/meminfo` text, in bytes.
#[must_use]
pub fn parse_mem_total(meminfo: &str) -> Option<u64> {
    meminfo.lines().find_map(|line| {
        let kib: u64 = line
            .strip_prefix("MemTotal:")?
            .trim()
            .strip_suffix("kB")?
            .trim()
            .parse()
            .ok()?;
        kib.checked_mul(1024)
    })
}

/// The cgroup v2 path of `/proc/self/cgroup` text (the `0::` line), relative to the hierarchy.
#[must_use]
pub fn parse_own_cgroup(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("0::"))
        .map(|path| path.trim().trim_start_matches('/').to_string())
}

/// The value of a `memory.high` or `memory.max` file, in bytes; `None` for `max`.
#[must_use]
pub fn parse_limit(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

/// Bytes to MiB, rounded down.
#[must_use]
pub fn mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

/// Every descendant of `root` in `table`, `root` excluded, found through parent pids.
#[must_use]
pub fn descendants(table: &ProcessTable, root: u32) -> BTreeSet<u32> {
    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (pid, info) in table {
        children.entry(info.ppid).or_default().push(*pid);
    }
    let mut out = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(pid) = pending.pop() {
        for child in children.get(&pid).into_iter().flatten() {
            if *child != root && out.insert(*child) {
                pending.push(*child);
            }
        }
    }
    out
}

/// The anonymous memory of `root` and its descendants in `table`, in KiB.
#[must_use]
pub fn tree_rss_anon_kib(table: &ProcessTable, root: u32) -> u64 {
    std::iter::once(root)
        .chain(descendants(table, root))
        .filter_map(|pid| table.get(&pid).and_then(|info| info.rss_anon_kib))
        .sum()
}

/// Reads the process table from `/proc`; empty where `/proc` is missing.
#[must_use]
pub fn read_process_table() -> ProcessTable {
    let mut table = ProcessTable::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return table;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some((ppid, pgid, state)) = parse_stat(&stat) else {
            continue;
        };
        let rss_anon_kib = std::fs::read_to_string(entry.path().join("status"))
            .ok()
            .and_then(|status| parse_rss_anon_kib(&status));
        table.insert(
            pid,
            ProcessInfo {
                ppid,
                pgid,
                state,
                rss_anon_kib,
            },
        );
    }
    table
}

/// The process group of this process, from `/proc/self/stat`.
#[must_use]
pub fn own_process_group() -> Option<u32> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    parse_stat(&stat).map(|(_, pgid, _)| pgid)
}

/// The live processes of group `pgid` (zombies excluded), by pid.
#[must_use]
pub fn group_members(table: &ProcessTable, pgid: u32) -> Vec<u32> {
    table
        .iter()
        .filter(|(_, info)| info.pgid == pgid && info.state != 'Z')
        .map(|(pid, _)| *pid)
        .collect()
}

/// The cgroup whose limit governs the gate: the first of `own` and its ancestors, under
/// `hierarchy`, whose `memory.high` or `memory.max` is a number, with that limit in bytes.
/// `read` returns a file's text.
pub fn find_limited_cgroup(
    hierarchy: &Path,
    own: &str,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Option<(PathBuf, u64)> {
    let mut current = PathBuf::from(own);
    loop {
        let dir = hierarchy.join(&current);
        let limit = ["memory.high", "memory.max"]
            .iter()
            .filter_map(|name| read(&dir.join(name)).and_then(|text| parse_limit(&text)))
            .min();
        if let Some(limit) = limit {
            return Some((dir, limit));
        }
        if !current.pop() || current.as_os_str().is_empty() {
            return None;
        }
    }
}

/// Where the slice's live reading and limit come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemorySource {
    /// The cgroup directory whose `memory.stat` `anon` is the live reading; `None` without
    /// cgroup v2.
    pub cgroup: Option<PathBuf>,
    /// Its limit in bytes, or `MemTotal` when no ancestor has one; `None` when neither exists.
    pub limit_bytes: Option<u64>,
}

/// The hierarchy cgroup v2 is mounted at.
const CGROUP_ROOT: &str = "/sys/fs/cgroup";

impl MemorySource {
    /// Finds the gate's cgroup and limit on this host (plan §3.2).
    #[must_use]
    pub fn discover() -> Self {
        let read = |path: &Path| std::fs::read_to_string(path).ok();
        let own = read(Path::new("/proc/self/cgroup")).and_then(|text| parse_own_cgroup(&text));
        let mem_total = read(Path::new("/proc/meminfo")).and_then(|text| parse_mem_total(&text));
        let Some(own) = own.filter(|_| Path::new(CGROUP_ROOT).join("cgroup.controllers").exists())
        else {
            return Self {
                cgroup: None,
                limit_bytes: mem_total,
            };
        };
        match find_limited_cgroup(Path::new(CGROUP_ROOT), &own, &read) {
            Some((dir, limit)) => Self {
                cgroup: Some(dir),
                limit_bytes: Some(limit),
            },
            None => Self {
                cgroup: Some(Path::new(CGROUP_ROOT).join(own)),
                limit_bytes: mem_total,
            },
        }
    }

    /// The live `anon` of the cgroup, in MiB.
    #[must_use]
    pub fn live_anon_mib(&self) -> Option<u64> {
        let stat = std::fs::read_to_string(self.cgroup.as_ref()?.join("memory.stat")).ok()?;
        parse_memory_stat_anon(&stat).map(mib)
    }
}

/// What the sampler has seen.
#[derive(Clone, Debug, Default)]
struct Samples {
    /// Running targets and their root pid.
    running: BTreeMap<TargetId, u32>,
    /// The peak of each target's tree, in KiB.
    peaks_kib: BTreeMap<TargetId, u64>,
    /// The latest live reading, in MiB.
    live_mib: Option<u64>,
    /// The largest live reading, in MiB.
    slice_peak_mib: Option<u64>,
    /// The latest process table.
    table: ProcessTable,
    /// Set to end the sampler thread.
    stop: bool,
}

/// A sampler thread: once per period it scans `/proc` and reads the slice's `anon`.
#[derive(Debug)]
pub struct Sampler {
    shared: Arc<Mutex<Samples>>,
    thread: Option<JoinHandle<()>>,
}

impl Sampler {
    /// Starts sampling every `period`, reading the live figure from `source`.
    #[must_use]
    pub fn start(source: MemorySource, period: Duration) -> Self {
        let shared = Arc::new(Mutex::new(Samples::default()));
        let worker = Arc::clone(&shared);
        let thread = std::thread::spawn(move || loop {
            let table = read_process_table();
            let live = source.live_anon_mib();
            let mut samples = worker.lock().unwrap_or_else(PoisonError::into_inner);
            if samples.stop {
                return;
            }
            sample_into(&mut samples, table, live);
            drop(samples);
            std::thread::sleep(period);
        });
        Self {
            shared,
            thread: Some(thread),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Samples> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts watching `target`, whose executable has `pid`.
    pub fn watch(&self, target: &TargetId, pid: u32) {
        self.lock().running.insert(target.clone(), pid);
    }

    /// Stops watching `target` and returns its peak in MiB, if any scan saw it.
    #[must_use]
    pub fn unwatch(&self, target: &TargetId) -> Option<u64> {
        let mut samples = self.lock();
        samples.running.remove(target);
        samples.peaks_kib.remove(target).map(|kib| kib / 1024)
    }

    /// The latest live reading of the slice, in MiB.
    #[must_use]
    pub fn live_mib(&self) -> Option<u64> {
        self.lock().live_mib
    }

    /// The largest live reading so far, in MiB.
    #[must_use]
    pub fn slice_peak_mib(&self) -> Option<u64> {
        self.lock().slice_peak_mib
    }

    /// The process table of the latest scan.
    #[must_use]
    pub fn table(&self) -> ProcessTable {
        self.lock().table.clone()
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.lock().stop = true;
        if let Some(thread) = self.thread.take() {
            // A panicked sampler has nothing left to report.
            thread.join().ok();
        }
    }
}

/// Folds one scan into `samples`.
fn sample_into(samples: &mut Samples, table: ProcessTable, live_mib: Option<u64>) {
    let running: Vec<(TargetId, u32)> = samples
        .running
        .iter()
        .map(|(target, pid)| (target.clone(), *pid))
        .collect();
    for (target, pid) in running {
        let kib = tree_rss_anon_kib(&table, pid);
        let peak = samples.peaks_kib.entry(target).or_default();
        *peak = (*peak).max(kib);
    }
    samples.live_mib = live_mib;
    samples.slice_peak_mib = match (samples.slice_peak_mib, live_mib) {
        (Some(peak), Some(live)) => Some(peak.max(live)),
        (peak, live) => peak.or(live),
    };
    samples.table = table;
}

/// Sends `signal` (`TERM`, `KILL`) to every pid of `pids` through `kill(1)`; returns whether
/// `kill` ran. Pids that are already gone are not an error.
pub fn signal(pids: &BTreeSet<u32>, signal: &str) -> bool {
    if pids.is_empty() {
        return true;
    }
    let mut command = Command::new("kill");
    command.arg(format!("-{signal}"));
    command.args(pids.iter().map(ToString::to_string));
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// The grace between `SIGTERM` and `SIGKILL` when a tree is killed (plan §3.4).
pub const KILL_GRACE: Duration = Duration::from_secs(10);

/// Kills `root` and its whole descendant tree: `SIGTERM` to all, then, after `grace`,
/// `SIGKILL` to every member still alive (zombies count as gone). Returns at once when the
/// whole tree has exited.
pub fn kill_tree(root: u32, grace: Duration) {
    let table = read_process_table();
    let mut tree = descendants(&table, root);
    tree.insert(root);
    signal(&tree, "TERM");
    let alive = |table: &ProcessTable, tree: &BTreeSet<u32>| -> BTreeSet<u32> {
        tree.iter()
            .copied()
            .filter(|pid| table.get(pid).is_some_and(|info| info.state != 'Z'))
            .collect()
    };
    let deadline = std::time::Instant::now() + grace;
    while std::time::Instant::now() < deadline {
        let table = read_process_table();
        // Children the tree started meanwhile are part of it too.
        tree.extend(descendants(&table, root));
        if alive(&table, &tree).is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let table = read_process_table();
    tree.extend(descendants(&table, root));
    signal(&alive(&table, &tree), "KILL");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(ppid: u32, pgid: u32, kib: u64) -> ProcessInfo {
        ProcessInfo {
            ppid,
            pgid,
            state: 'S',
            rss_anon_kib: Some(kib),
        }
    }

    #[test]
    fn rss_anon_and_slice_anon_are_read_from_their_files() {
        let status = "Name:\tx\nVmRSS:\t  9000 kB\nRssAnon:\t    4096 kB\nRssFile:\t 10 kB\n";
        assert_eq!(parse_rss_anon_kib(status), Some(4096));
        assert_eq!(parse_rss_anon_kib("Name:\tkthreadd\n"), None);
        let stat = "anon 26843545600\nfile 1048576\nkernel 0\n";
        assert_eq!(parse_memory_stat_anon(stat), Some(26_843_545_600));
        assert_eq!(parse_memory_stat_anon(stat).map(mib), Some(25_600));
        assert_eq!(
            parse_mem_total("MemTotal:       65859476 kB\nMemFree: 1 kB\n"),
            Some(65_859_476 * 1024)
        );
    }

    #[test]
    fn stat_lines_with_odd_command_names_parse() {
        assert_eq!(
            parse_stat("4242 (my (odd) proc) S 4000 4100 4000 0 -1 4194560"),
            Some((4000, 4100, 'S'))
        );
        assert_eq!(parse_stat("garbage"), None);
    }

    #[test]
    fn the_limit_is_found_by_walking_up_from_the_session_scope() {
        let files: BTreeMap<PathBuf, &str> = BTreeMap::from([
            (
                PathBuf::from("/cg/user.slice/user-1002.slice/session-5.scope/memory.high"),
                "max\n",
            ),
            (
                PathBuf::from("/cg/user.slice/user-1002.slice/session-5.scope/memory.max"),
                "max\n",
            ),
            (
                PathBuf::from("/cg/user.slice/user-1002.slice/memory.high"),
                "70866960384\n",
            ),
            (
                PathBuf::from("/cg/user.slice/user-1002.slice/memory.max"),
                "75161927680\n",
            ),
        ]);
        let read = |path: &Path| files.get(path).map(ToString::to_string);
        let own = parse_own_cgroup("0::/user.slice/user-1002.slice/session-5.scope\n");
        assert_eq!(
            own.as_deref(),
            Some("user.slice/user-1002.slice/session-5.scope")
        );
        let found = find_limited_cgroup(Path::new("/cg"), own.as_deref().unwrap_or(""), &read);
        assert_eq!(
            found,
            Some((
                PathBuf::from("/cg/user.slice/user-1002.slice"),
                70_866_960_384
            ))
        );
        let unlimited = |path: &Path| path.ends_with("memory.high").then(|| "max".to_string());
        assert_eq!(
            find_limited_cgroup(Path::new("/cg"), "a/b", &unlimited),
            None,
            "`max` everywhere falls back to MemTotal"
        );
    }

    #[test]
    fn descendants_cross_process_groups() {
        let table = ProcessTable::from([
            (10, info(1, 10, 100)),
            (11, info(10, 10, 200)),
            (12, info(11, 12, 300)), // xvfb-run started its own group
            (13, info(12, 12, 400)),
            (20, info(1, 20, 5_000)), // unrelated
        ]);
        assert_eq!(descendants(&table, 10), BTreeSet::from([11, 12, 13]));
        assert_eq!(tree_rss_anon_kib(&table, 10), 1_000);
        assert_eq!(tree_rss_anon_kib(&table, 99), 0);
        assert_eq!(group_members(&table, 12), [12, 13]);
    }

    #[test]
    fn samples_keep_each_targets_peak_and_the_slice_peak() {
        let target = TargetId::test("c", "t");
        let mut samples = Samples::default();
        samples.running.insert(target.clone(), 10);
        let small = ProcessTable::from([(10, info(1, 10, 2048))]);
        let big = ProcessTable::from([(10, info(1, 10, 1024)), (11, info(10, 11, 4096))]);
        sample_into(&mut samples, big, Some(30_000));
        sample_into(&mut samples, small, Some(20_000));
        assert_eq!(samples.peaks_kib.get(&target), Some(&5120));
        assert_eq!(samples.live_mib, Some(20_000));
        assert_eq!(samples.slice_peak_mib, Some(30_000));
    }
}
