//! Lane scheduling of test executables: exclusive jobs alone first, then a GPU lane and general
//! lanes within a memory budget.
//!
//! # Responsibility
//! Decides which selected test executable starts next and on which lane (blueprint §5.3, plan
//! 2026-11-11-faster-gates §3.1 and §3.3). It holds no threads and no clock: the runner asks
//! [`Scheduler::take`] for work whenever a lane may be free, passing the live memory reading,
//! and reports each finished job with [`Scheduler::finish`].
//!
//! # How it fits
//! The runner's threads share one scheduler behind a mutex; the memory sampler (`memory.rs`)
//! supplies the live reading. Keeping the decision here, as plain data, lets the unit tests
//! replay whole runs with invented durations, peaks and readings and check the lane rules.
//!
//! # Invariants
//! - L-1: with `jobs = 1` every job runs alone, one after another, as `cargo test` runs them.
//!   With `jobs > 1` and at least one `gpu` job, one lane runs `gpu` jobs one at a time and the
//!   other `jobs - 1` lanes run everything else; with no `gpu` job all lanes are general.
//! - L-2: an exclusive job (every `valgrind` job, every job with no memory estimate, every
//!   target marked `exclusive`) runs with nothing else running. Exclusive jobs are queued ahead
//!   of all others, so they run one after another while the lanes are still idle.
//! - L-3: each queue runs longest-first by expected duration, ties in target order; a job that
//!   does not fit the memory budget may be overtaken by a later one that does.
//! - Admission (§3.3): a job starts when its lane is free and either nothing is running, or
//!   the running jobs' estimates plus its own stay within the budget and the live reading plus
//!   its estimate stays within the ceiling.
//! - L-4: when nothing is running, the next job starts whatever its estimate and the live
//!   reading say, so a run can become sequential but never deadlocks.

use crate::catalog::TestClass;
use crate::workspace::TargetId;
use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

/// One test executable to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    /// The target.
    pub target: TargetId,
    /// Its class, which decides its lane.
    pub class: TestClass,
    /// Its wall time in the last stamp, or zero when unknown.
    pub expected: Duration,
    /// Its `--test-threads` cap from the catalog, or `None` for libtest's default.
    pub test_threads: Option<u32>,
    /// Its memory estimate in MiB (the largest recorded peak of its process tree), or `None`
    /// when this host never measured it.
    pub estimate_mib: Option<u64>,
    /// Whether the catalog marks it `exclusive`.
    pub exclusive: bool,
    /// Its time limit; `None` for no limit.
    pub timeout: Option<Duration>,
}

impl Job {
    /// Whether the job must run with nothing else running (L-2).
    #[must_use]
    pub fn runs_alone(&self) -> bool {
        self.exclusive || self.class == TestClass::Valgrind || self.estimate_mib.is_none()
    }
}

/// Where a started job runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lane {
    /// Alone, with every lane idle (L-2).
    Exclusive,
    /// The dedicated GPU lane.
    Gpu,
    /// The general lane of this number, from 1.
    General(usize),
}

impl fmt::Display for Lane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exclusive => f.write_str("X"),
            Self::Gpu => f.write_str("GPU"),
            Self::General(number) => write!(f, "L{number}"),
        }
    }
}

/// The memory limits of admission, in MiB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryLimits {
    /// The most the running jobs' estimates may sum to.
    pub budget_mib: u64,
    /// The most the live reading plus a starting job's estimate may reach; `None` when no live
    /// reading exists (no cgroup v2).
    pub ceiling_mib: Option<u64>,
}

impl MemoryLimits {
    /// No limit at all: every job fits.
    pub const UNLIMITED: Self = Self {
        budget_mib: u64::MAX,
        ceiling_mib: None,
    };

    /// Limits with the budget capped at the ceiling, as the plan's §3.2 requires.
    #[must_use]
    pub fn new(budget_mib: u64, ceiling_mib: Option<u64>) -> Self {
        Self {
            budget_mib: ceiling_mib.map_or(budget_mib, |ceiling| budget_mib.min(ceiling)),
            ceiling_mib,
        }
    }
}

/// The scheduler's state.
#[derive(Clone, Debug)]
pub struct Scheduler {
    /// Jobs that run alone, first.
    exclusive: VecDeque<Job>,
    /// Jobs for the dedicated GPU lane (empty when there is none).
    gpu: VecDeque<Job>,
    /// Every other job.
    general: VecDeque<Job>,
    /// Busy flag per general lane.
    lanes: Vec<bool>,
    /// Whether the GPU lane is running a job.
    gpu_busy: bool,
    /// Whether an exclusive job is running.
    exclusive_busy: bool,
    /// Estimates of the running jobs, by lane.
    running: Vec<(Lane, u64)>,
    /// The memory limits.
    limits: MemoryLimits,
    /// Whether the last [`Scheduler::take`] held back a job only because of memory.
    held_for_memory: bool,
}

/// Sorts longest-first, ties by target.
fn longest_first(mut jobs: Vec<Job>) -> VecDeque<Job> {
    jobs.sort_by(|a, b| {
        b.expected
            .cmp(&a.expected)
            .then_with(|| a.target.cmp(&b.target))
    });
    jobs.into()
}

impl Scheduler {
    /// A scheduler for `lanes` lanes (zero is read as one) within `limits`.
    #[must_use]
    pub fn new(lanes: usize, work: Vec<Job>, limits: MemoryLimits) -> Self {
        let lanes = lanes.max(1);
        let (exclusive, rest): (Vec<Job>, Vec<Job>) = work.into_iter().partition(Job::runs_alone);
        let dedicated = lanes > 1 && rest.iter().any(|job| job.class == TestClass::Gpu);
        let (gpu, general): (Vec<Job>, Vec<Job>) = rest
            .into_iter()
            .partition(|job| dedicated && job.class == TestClass::Gpu);
        let general_lanes = if dedicated { lanes - 1 } else { lanes };
        Self {
            exclusive: longest_first(exclusive),
            gpu: longest_first(gpu),
            general: longest_first(general),
            lanes: vec![false; general_lanes],
            gpu_busy: false,
            exclusive_busy: false,
            running: Vec::new(),
            limits,
            held_for_memory: false,
        }
    }

    /// Whether a job with `estimate` MiB fits beside the running jobs (§3.3), given the live
    /// reading.
    fn fits(&self, estimate: u64, live_mib: Option<u64>) -> bool {
        if self.running.is_empty() {
            return true;
        }
        let sum: u64 = self.running.iter().map(|(_, mib)| *mib).sum();
        let budget = sum.saturating_add(estimate) <= self.limits.budget_mib;
        let live = match (live_mib, self.limits.ceiling_mib) {
            (Some(live), Some(ceiling)) => live.saturating_add(estimate) <= ceiling,
            _ => true,
        };
        budget && live
    }

    /// Records `job` as started on `lane`.
    fn start(&mut self, job: Job, lane: Lane) -> (Job, Lane) {
        self.running.push((lane, job.estimate_mib.unwrap_or(0)));
        (job, lane)
    }

    /// The next job that can start now, and its lane; `None` when every startable lane is busy,
    /// no waiting job fits the memory limits, or no work is left. `live_mib` is the slice's
    /// current `anon` in MiB, if known.
    pub fn take(&mut self, live_mib: Option<u64>) -> Option<(Job, Lane)> {
        self.held_for_memory = false;
        if self.exclusive_busy {
            return None;
        }
        if !self.exclusive.is_empty() {
            if !self.running.is_empty() {
                return None;
            }
            let job = self.exclusive.pop_front()?;
            self.exclusive_busy = true;
            return Some(self.start(job, Lane::Exclusive));
        }
        if !self.gpu_busy {
            if let Some(estimate) = self.gpu.front().map(|job| job.estimate_mib.unwrap_or(0)) {
                if self.fits(estimate, live_mib) {
                    let job = self.gpu.pop_front()?;
                    self.gpu_busy = true;
                    return Some(self.start(job, Lane::Gpu));
                }
                self.held_for_memory = true;
            }
        }
        let free = self.lanes.iter().position(|busy| !busy)?;
        let index = self
            .general
            .iter()
            .position(|job| self.fits(job.estimate_mib.unwrap_or(0), live_mib));
        let Some(index) = index else {
            self.held_for_memory |= !self.general.is_empty();
            return None;
        };
        let job = self.general.remove(index)?;
        self.lanes[free] = true;
        Some(self.start(job, Lane::General(free + 1)))
    }

    /// Records that a job started on `lane` has finished.
    pub fn finish(&mut self, lane: Lane) {
        match lane {
            Lane::Exclusive => self.exclusive_busy = false,
            Lane::Gpu => self.gpu_busy = false,
            Lane::General(number) => {
                if let Some(busy) = self.lanes.get_mut(number.wrapping_sub(1)) {
                    *busy = false;
                }
            }
        }
        if let Some(position) = self.running.iter().position(|(l, _)| *l == lane) {
            self.running.remove(position);
        }
    }

    /// Whether the last [`Scheduler::take`] held a waiting job back only for memory.
    #[must_use]
    pub fn held_for_memory(&self) -> bool {
        self.held_for_memory
    }

    /// Removes every job not yet started, for a stopped run, and returns them in queue order.
    pub fn drain_waiting(&mut self) -> Vec<Job> {
        let mut out: Vec<Job> = self.exclusive.drain(..).collect();
        out.extend(self.gpu.drain(..));
        out.extend(self.general.drain(..));
        out
    }

    /// How many jobs have not started yet.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.exclusive.len() + self.gpu.len() + self.general.len()
    }

    /// Whether every job has started and finished.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.exclusive.is_empty()
            && self.gpu.is_empty()
            && self.general.is_empty()
            && self.running.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(name: &str, class: TestClass, seconds: u64, mib: Option<u64>) -> Job {
        Job {
            target: TargetId::test("c", name),
            class,
            expected: Duration::from_secs(seconds),
            test_threads: None,
            estimate_mib: mib,
            exclusive: false,
            timeout: None,
        }
    }

    /// One started job of a replay.
    #[derive(Clone, Debug)]
    struct Entry {
        start: u64,
        end: u64,
        lane: Lane,
        name: String,
    }

    /// Replays a run with each job taking its expected time and using its estimate; `live` gives
    /// the live reading at each time from the running estimates. Checks the lane limits, the
    /// budget and L-4 at every start.
    fn replay_with(
        lanes: usize,
        work: Vec<Job>,
        limits: MemoryLimits,
        live: &dyn Fn(u64, u64) -> Option<u64>,
    ) -> Vec<Entry> {
        let mut scheduler = Scheduler::new(lanes, work, limits);
        let mut now = 0u64;
        let mut running: Vec<(u64, Lane, Job)> = Vec::new();
        let mut log = Vec::new();
        while !scheduler.is_done() {
            loop {
                let sum: u64 = running
                    .iter()
                    .map(|(_, _, j)| j.estimate_mib.unwrap_or(0))
                    .sum();
                let Some((job, lane)) = scheduler.take(live(now, sum)) else {
                    break;
                };
                let alone_before = running.is_empty();
                let end = now + job.expected.as_secs();
                log.push(Entry {
                    start: now,
                    end,
                    lane,
                    name: job.target.to_string(),
                });
                running.push((end, lane, job));
                let gpus = running.iter().filter(|(_, l, _)| *l == Lane::Gpu).count();
                assert!(gpus <= 1, "two GPU jobs at once");
                assert!(running.len() <= lanes.max(1), "lanes oversubscribed");
                if running.iter().any(|(_, l, _)| *l == Lane::Exclusive) {
                    assert_eq!(running.len(), 1, "an exclusive job runs alone");
                }
                let total: u64 = running
                    .iter()
                    .map(|(_, _, j)| j.estimate_mib.unwrap_or(0))
                    .sum();
                assert!(
                    alone_before || total <= limits.budget_mib,
                    "budget exceeded"
                );
            }
            running.sort_by_key(|(end, _, _)| *end);
            assert!(!running.is_empty(), "L-4: an idle run must start its head");
            let (end, lane, _) = running.remove(0);
            now = end;
            scheduler.finish(lane);
        }
        log
    }

    fn replay(lanes: usize, work: Vec<Job>) -> Vec<Entry> {
        replay_with(lanes, work, MemoryLimits::UNLIMITED, &|_, _| None)
    }

    fn start_of(log: &[Entry], name: &str) -> Option<u64> {
        log.iter().find(|e| e.name == name).map(|e| e.start)
    }

    #[test]
    fn one_lane_is_still_the_sequential_run_of_cargo_test() {
        let log = replay(
            1,
            vec![
                job("a", TestClass::Fast, 5, Some(100)),
                job("g", TestClass::Gpu, 20, Some(100)),
                job("s", TestClass::Slow, 10, Some(100)),
            ],
        );
        let order: Vec<&str> = log.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(order, ["c/g", "c/s", "c/a"]);
        assert_eq!(log.last().map(|e| e.end), Some(35));
        assert!(log.iter().all(|e| e.lane == Lane::General(1)));
    }

    #[test]
    fn gpu_jobs_get_their_own_lane_and_never_block_general_lanes() {
        let log = replay(
            3,
            vec![
                job("g1", TestClass::Gpu, 30, Some(1)),
                job("g2", TestClass::Gpu, 30, Some(1)),
                job("a", TestClass::Fast, 10, Some(1)),
                job("b", TestClass::Fast, 10, Some(1)),
                job("c", TestClass::Slow, 10, Some(1)),
            ],
        );
        let gpu_starts: Vec<u64> = log
            .iter()
            .filter(|e| e.lane == Lane::Gpu)
            .map(|e| e.start)
            .collect();
        assert_eq!(gpu_starts, [0, 30], "GPU jobs run one at a time");
        let general_end = log
            .iter()
            .filter(|e| e.lane != Lane::Gpu)
            .map(|e| e.end)
            .max();
        assert_eq!(general_end, Some(20));
        assert!(log.iter().any(|e| e.lane == Lane::General(2)));
    }

    #[test]
    fn valgrind_runs_alone_with_no_other_job_even_the_gpu_lane() {
        let log = replay(
            3,
            vec![
                job("v", TestClass::Valgrind, 10, Some(1)),
                job("g", TestClass::Gpu, 5, Some(1)),
                job("a", TestClass::Fast, 5, Some(1)),
            ],
        );
        assert_eq!(log[0].name, "c/v");
        assert_eq!(log[0].lane, Lane::Exclusive);
        assert_eq!(start_of(&log, "c/g"), Some(10));
        assert_eq!(start_of(&log, "c/a"), Some(10));
    }

    #[test]
    fn exclusive_and_unmeasured_jobs_run_first_and_alone_then_the_lanes_open() {
        let mut marked = job("m", TestClass::Slow, 4, Some(1));
        marked.exclusive = true;
        let log = replay(
            3,
            vec![
                job("long", TestClass::Slow, 100, Some(1)),
                job("new", TestClass::Fast, 3, None),
                marked,
                job("b", TestClass::Fast, 1, Some(1)),
            ],
        );
        let firsts: Vec<(&str, u64, Lane)> = log
            .iter()
            .take(2)
            .map(|e| (e.name.as_str(), e.start, e.lane))
            .collect();
        assert_eq!(
            firsts,
            [("c/m", 0, Lane::Exclusive), ("c/new", 4, Lane::Exclusive)]
        );
        assert_eq!(start_of(&log, "c/long"), Some(7));
        assert_eq!(start_of(&log, "c/b"), Some(7));
    }

    #[test]
    fn a_job_waits_until_the_running_estimates_leave_room_in_the_budget() {
        let limits = MemoryLimits::new(10_000, None);
        let log = replay_with(
            3,
            vec![
                job("a", TestClass::Slow, 50, Some(6_000)),
                job("b", TestClass::Slow, 40, Some(6_000)),
            ],
            limits,
            &|_, _| None,
        );
        assert_eq!(start_of(&log, "c/a"), Some(0));
        assert_eq!(start_of(&log, "c/b"), Some(50));
    }

    #[test]
    fn a_job_above_the_budget_still_runs_when_nothing_else_does() {
        let limits = MemoryLimits::new(1_000, None);
        let log = replay_with(
            3,
            vec![
                job("huge", TestClass::Slow, 10, Some(5_000)),
                job("b", TestClass::Fast, 5, Some(10)),
            ],
            limits,
            &|_, _| None,
        );
        assert_eq!(start_of(&log, "c/huge"), Some(0));
        assert_eq!(start_of(&log, "c/b"), Some(10));
    }

    #[test]
    fn live_memory_above_the_ceiling_admits_nothing_new_but_an_idle_run_still_starts_its_head() {
        let limits = MemoryLimits::new(40_000, Some(56_000));
        let busy_slice = |_: u64, _: u64| Some(60_000);
        let log = replay_with(
            3,
            vec![
                job("a", TestClass::Slow, 10, Some(100)),
                job("b", TestClass::Slow, 10, Some(100)),
            ],
            limits,
            &busy_slice,
        );
        assert_eq!(start_of(&log, "c/a"), Some(0));
        assert_eq!(
            start_of(&log, "c/b"),
            Some(10),
            "nothing joins a full slice"
        );
        let mut scheduler = Scheduler::new(
            3,
            vec![
                job("a", TestClass::Slow, 10, Some(100)),
                job("b", TestClass::Slow, 10, Some(100)),
            ],
            limits,
        );
        assert!(scheduler.take(Some(60_000)).is_some());
        assert!(scheduler.take(Some(60_000)).is_none());
        assert!(scheduler.held_for_memory());
    }

    #[test]
    fn a_smaller_job_overtakes_one_that_does_not_fit() {
        let limits = MemoryLimits::new(10_000, None);
        let log = replay_with(
            3,
            vec![
                job("a", TestClass::Slow, 50, Some(6_000)),
                job("big", TestClass::Slow, 40, Some(6_000)),
                job("small", TestClass::Fast, 5, Some(1_000)),
            ],
            limits,
            &|_, _| None,
        );
        assert_eq!(start_of(&log, "c/small"), Some(0));
        assert_eq!(start_of(&log, "c/big"), Some(50));
    }

    #[test]
    fn every_job_runs_exactly_once_over_many_seeded_random_runs() {
        // A 64-bit linear congruential generator (Knuth's MMIX constants), seeded: the runs are
        // the same on every machine.
        let mut state: u64 = 0x005e_ed0f_9a7e;
        let mut next = move |bound: u64| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) % bound.max(1)
        };
        let classes = TestClass::ALL;
        for run in 0..500 {
            let count = 1 + next(12);
            let work: Vec<Job> = (0..count)
                .map(|i| {
                    let class = classes[usize::try_from(next(4)).unwrap_or(0)];
                    let mib = (next(5) > 0).then(|| next(20_000));
                    let mut j = job(&format!("t{i}"), class, 1 + next(50), mib);
                    j.exclusive = next(10) == 0;
                    j
                })
                .collect();
            let lanes = usize::try_from(1 + next(4)).unwrap_or(1);
            let limits = MemoryLimits::new(10_000 + next(30_000), Some(56_000));
            let noise = next(30_000);
            let log = replay_with(lanes, work.clone(), limits, &|now, sum| {
                Some(sum + (noise + now * 7) % 30_000)
            });
            let mut names: Vec<String> = log.iter().map(|e| e.name.clone()).collect();
            names.sort();
            let mut expected: Vec<String> = work.iter().map(|j| j.target.to_string()).collect();
            expected.sort();
            assert_eq!(names, expected, "run {run}: every job exactly once");
        }
    }

    #[test]
    fn zero_lanes_mean_one() {
        let log = replay(
            0,
            vec![
                job("a", TestClass::Fast, 1, Some(1)),
                job("b", TestClass::Fast, 1, Some(1)),
            ],
        );
        assert_eq!(log[1].start, 1);
    }

    #[test]
    fn the_budget_is_capped_at_the_ceiling_and_lanes_print_their_names() {
        assert_eq!(MemoryLimits::new(40_000, Some(30_000)).budget_mib, 30_000);
        assert_eq!(MemoryLimits::new(40_000, None).budget_mib, 40_000);
        let names: Vec<String> = [Lane::Exclusive, Lane::Gpu, Lane::General(2)]
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(names, ["X", "GPU", "L2"]);
    }
}
