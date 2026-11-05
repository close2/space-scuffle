//! Lane scheduling of test executables: sequential by default, a dedicated GPU lane above one.
//!
//! # Responsibility
//! Decides which selected test executable starts next and on which lane (blueprint §5.3). It
//! holds no threads and no clock: the runner asks [`Scheduler::take`] for work whenever a lane
//! may be free and reports each finished job with [`Scheduler::finish`].
//!
//! # How it fits
//! The runner's threads share one scheduler behind a mutex. Keeping the decision here, as plain
//! data, lets the unit tests replay whole runs with invented durations and check the lane rules.
//!
//! # Invariants
//! - With `jobs = 1` every job runs alone, one after another, as `cargo test` runs them.
//! - With `jobs > 1` and at least one `gpu` job, one lane is reserved for `gpu` jobs and the
//!   other `jobs - 1` lanes run everything else: at most one GPU binary runs at a time, and no
//!   general lane ever waits for one. With no `gpu` job, all lanes are general.
//! - A `valgrind` job occupies two general lanes (it runs its own children at up to one per
//!   core), or all general lanes if there is only one.
//! - Each queue runs longest-first by expected duration, ties in target order, so the tail of a
//!   run is short; a general job that does not fit waits at the head (no overtaking), so a
//!   two-lane job cannot starve.

use crate::catalog::TestClass;
use crate::workspace::TargetId;
use std::collections::VecDeque;
use std::time::Duration;

/// One test executable to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    /// The target.
    pub target: TargetId,
    /// Its class, which decides its lane and width.
    pub class: TestClass,
    /// Its wall time in the last stamp, or zero when unknown.
    pub expected: Duration,
    /// Its `--test-threads` cap from the catalog, or `None` for libtest's default.
    pub test_threads: Option<u32>,
}

/// Where a started job runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// The dedicated GPU lane.
    Gpu,
    /// General lanes; the number is how many the job occupies.
    General(usize),
}

/// The scheduler's state.
#[derive(Clone, Debug)]
pub struct Scheduler {
    /// Jobs for the dedicated GPU lane (empty when there is none).
    gpu: VecDeque<Job>,
    /// Every other job.
    general: VecDeque<Job>,
    /// General lanes in total.
    general_lanes: usize,
    /// General lanes free now.
    free: usize,
    /// Whether the GPU lane is running a job.
    gpu_busy: bool,
    /// Jobs started and not yet finished.
    running: usize,
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
    /// A scheduler for `jobs` lanes (zero is read as one).
    #[must_use]
    pub fn new(lanes: usize, work: Vec<Job>) -> Self {
        let lanes = lanes.max(1);
        let dedicated = lanes > 1 && work.iter().any(|job| job.class == TestClass::Gpu);
        let (gpu, general): (Vec<Job>, Vec<Job>) = work
            .into_iter()
            .partition(|job| dedicated && job.class == TestClass::Gpu);
        let general_lanes = if dedicated { lanes - 1 } else { lanes };
        Self {
            gpu: longest_first(gpu),
            general: longest_first(general),
            general_lanes,
            free: general_lanes,
            gpu_busy: false,
            running: 0,
        }
    }

    /// The general lanes `job` needs.
    fn width(&self, job: &Job) -> usize {
        if job.class == TestClass::Valgrind {
            self.general_lanes.min(2)
        } else {
            1
        }
    }

    /// The next job that can start now, and its lane; `None` when every startable lane is busy
    /// or no work is left.
    pub fn take(&mut self) -> Option<(Job, Lane)> {
        if !self.gpu_busy {
            if let Some(job) = self.gpu.pop_front() {
                self.gpu_busy = true;
                self.running += 1;
                return Some((job, Lane::Gpu));
            }
        }
        let width = self.width(self.general.front()?);
        if width > self.free {
            return None;
        }
        let job = self.general.pop_front()?;
        self.free -= width;
        self.running += 1;
        Some((job, Lane::General(width)))
    }

    /// Records that a job started on `lane` has finished.
    pub fn finish(&mut self, lane: Lane) {
        match lane {
            Lane::Gpu => self.gpu_busy = false,
            Lane::General(width) => self.free = (self.free + width).min(self.general_lanes),
        }
        self.running = self.running.saturating_sub(1);
    }

    /// Whether every job has started and finished.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.gpu.is_empty() && self.general.is_empty() && self.running == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(name: &str, class: TestClass, seconds: u64) -> Job {
        Job {
            target: TargetId::test("c", name),
            class,
            expected: Duration::from_secs(seconds),
            test_threads: None,
        }
    }

    /// Replays a run with each job taking its expected time; returns `(start, end, lane, name)`
    /// per job and checks the lane limits at every start.
    fn replay(lanes: usize, work: Vec<Job>) -> Vec<(u64, u64, Lane, String)> {
        let mut scheduler = Scheduler::new(lanes, work);
        let mut now = 0u64;
        let mut running: Vec<(u64, Lane, Job)> = Vec::new();
        let mut log = Vec::new();
        while !scheduler.is_done() {
            while let Some((job, lane)) = scheduler.take() {
                let end = now + job.expected.as_secs();
                log.push((now, end, lane, job.target.to_string()));
                running.push((end, lane, job));
                let gpus = running.iter().filter(|(_, l, _)| *l == Lane::Gpu).count();
                let general: usize = running
                    .iter()
                    .map(|(_, l, _)| match l {
                        Lane::General(w) => *w,
                        Lane::Gpu => 0,
                    })
                    .sum();
                assert!(gpus <= 1, "two GPU jobs at once");
                assert!(general + gpus <= lanes.max(1), "lanes oversubscribed");
            }
            running.sort_by_key(|(end, _, _)| *end);
            if running.is_empty() {
                break;
            }
            let (end, lane, _) = running.remove(0);
            now = end;
            scheduler.finish(lane);
        }
        log
    }

    #[test]
    fn one_lane_runs_everything_in_sequence_longest_first() {
        let log = replay(
            1,
            vec![
                job("a", TestClass::Fast, 5),
                job("g", TestClass::Gpu, 20),
                job("v", TestClass::Valgrind, 10),
            ],
        );
        let order: Vec<&str> = log.iter().map(|(_, _, _, n)| n.as_str()).collect();
        assert_eq!(order, ["c/g", "c/v", "c/a"]);
        assert_eq!(log.last().map(|entry| entry.1), Some(35));
        assert!(log.iter().all(|(_, _, lane, _)| *lane == Lane::General(1)));
    }

    #[test]
    fn gpu_jobs_get_their_own_lane_and_never_block_general_lanes() {
        let log = replay(
            3,
            vec![
                job("g1", TestClass::Gpu, 30),
                job("g2", TestClass::Gpu, 30),
                job("a", TestClass::Fast, 10),
                job("b", TestClass::Fast, 10),
                job("c", TestClass::Slow, 10),
            ],
        );
        let gpu_starts: Vec<u64> = log
            .iter()
            .filter(|(_, _, lane, _)| *lane == Lane::Gpu)
            .map(|entry| entry.0)
            .collect();
        assert_eq!(gpu_starts, [0, 30], "GPU jobs run one at a time");
        let general_end = log
            .iter()
            .filter(|(_, _, lane, _)| *lane != Lane::Gpu)
            .map(|entry| entry.1)
            .max();
        assert_eq!(
            general_end,
            Some(20),
            "two general lanes finish three jobs by t=20"
        );
    }

    #[test]
    fn valgrind_takes_two_general_lanes() {
        let log = replay(
            3,
            vec![
                job("v", TestClass::Valgrind, 10),
                job("a", TestClass::Fast, 5),
                job("b", TestClass::Fast, 5),
            ],
        );
        assert_eq!(
            log[0].2,
            Lane::General(2),
            "no GPU job: three general lanes"
        );
        let a_start = log.iter().find(|e| e.3 == "c/a").map(|e| e.0);
        assert_eq!(a_start, Some(0), "the third lane runs alongside valgrind");
        let two = replay(
            2,
            vec![
                job("g", TestClass::Gpu, 1),
                job("v", TestClass::Valgrind, 1),
            ],
        );
        assert!(
            two.iter().any(|e| e.2 == Lane::General(1)),
            "one general lane is all it gets"
        );
    }

    #[test]
    fn zero_lanes_mean_one() {
        let log = replay(
            0,
            vec![job("a", TestClass::Fast, 1), job("b", TestClass::Fast, 1)],
        );
        assert_eq!(log[1].0, 1);
    }
}
