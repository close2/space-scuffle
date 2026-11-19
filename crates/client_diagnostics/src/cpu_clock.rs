//! CPU-time clocks of the calling thread and of the whole process (plan
//! `2026-11-19-retained-effect-instances` D-14).
//!
//! # Responsibility
//! Owns [`thread_cpu_time`] and [`process_cpu_time`]: the CPU time the calling thread, or every
//! thread of the process, has spent running, read from the kernel's `CLOCK_THREAD_CPUTIME_ID`
//! and `CLOCK_PROCESS_CPUTIME_ID` through `rustix`'s safe wrapper (`std` has no CPU-time clock,
//! and this crate forbids `unsafe`).
//!
//! # How it fits
//! On the loaded shared host, wall time varies by a factor of two between identical runs while
//! CPU time does not include the time a thread waits for a core. The offline `effect_frame`
//! bench times its phases by the thread clock; `frame_budget`'s recorder records the process
//! clock per frame, from which `frame_budget_series` judges the CPU-time verdict.
//!
//! # Invariants
//! - Both clocks are monotone for the thread or process that reads them.
//! - Where the platform has no such clock, the functions return [`CpuClockError::Unsupported`];
//!   no caller substitutes wall time for it.

use std::time::Duration;

/// A CPU-time clock that cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CpuClockError {
    /// The platform has no per-thread or per-process CPU-time clock this crate can read.
    #[error(
        "this platform has no CPU-time clock (CLOCK_THREAD_CPUTIME_ID / CLOCK_PROCESS_CPUTIME_ID)"
    )]
    Unsupported,
    /// The kernel returned a negative time.
    #[error("the CPU-time clock returned a negative time")]
    Negative,
}

/// CPU time the calling thread has spent running since it started.
///
/// # Errors
/// [`CpuClockError::Unsupported`] off Unix; [`CpuClockError::Negative`] for an impossible reading.
pub fn thread_cpu_time() -> Result<Duration, CpuClockError> {
    read(Clock::Thread)
}

/// CPU time every thread of the process has spent running since it started.
///
/// # Errors
/// [`CpuClockError::Unsupported`] off Unix; [`CpuClockError::Negative`] for an impossible reading.
pub fn process_cpu_time() -> Result<Duration, CpuClockError> {
    read(Clock::Process)
}

/// Which CPU-time clock to read.
#[derive(Debug, Clone, Copy)]
enum Clock {
    /// `CLOCK_THREAD_CPUTIME_ID`.
    Thread,
    /// `CLOCK_PROCESS_CPUTIME_ID`.
    Process,
}

#[cfg(unix)]
fn read(clock: Clock) -> Result<Duration, CpuClockError> {
    use rustix::time::{clock_gettime, ClockId};
    let id = match clock {
        Clock::Thread => ClockId::ThreadCPUTime,
        Clock::Process => ClockId::ProcessCPUTime,
    };
    let time = clock_gettime(id);
    let seconds = u64::try_from(time.tv_sec).map_err(|_| CpuClockError::Negative)?;
    let nanos = u32::try_from(time.tv_nsec).map_err(|_| CpuClockError::Negative)?;
    Ok(Duration::new(seconds, nanos))
}

#[cfg(not(unix))]
const fn read(_: Clock) -> Result<Duration, CpuClockError> {
    Err(CpuClockError::Unsupported)
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "a failed test is a panic")]
mod tests {
    use super::*;

    #[test]
    fn both_clocks_advance_while_the_thread_works() {
        let thread_before = thread_cpu_time().expect("the thread clock reads");
        let process_before = process_cpu_time().expect("the process clock reads");
        let mut sum = 0_u64;
        for value in 0..2_000_000_u64 {
            sum = std::hint::black_box(sum.wrapping_add(value * value));
        }
        std::hint::black_box(sum);
        let thread_after = thread_cpu_time().expect("the thread clock reads");
        let process_after = process_cpu_time().expect("the process clock reads");
        assert!(thread_after > thread_before, "the thread clock advanced");
        assert!(process_after > process_before, "the process clock advanced");
        assert!(
            process_after.saturating_sub(process_before)
                >= thread_after.saturating_sub(thread_before) / 2,
            "the process clock counts the working thread"
        );
    }
}
