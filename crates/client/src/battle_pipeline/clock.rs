//! Bounds client wall-time debt with checked integer durations. Dropped scheduling
//! time causes observable slowdown, never skipped authoritative simulation ticks.
//! The simulation tick rate itself remains independent of display-frame grouping.

use std::time::Duration;

use bevy::prelude::Resource;

/// Authoritative duration of one simulation tick: 40 milliseconds, or 25 Hz.
pub const SIM_TICK_PERIOD: Duration = Duration::from_millis(40);

/// Validated interactive scheduling limits; durations are scaled simulation time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BattleClockPolicy {
    /// Authoritative tick duration; the current simulation contract is 40 milliseconds.
    pub tick_period: Duration,
    /// Maximum complete simulation updates executed during one display frame.
    pub ticks_per_frame: u32,
    /// Maximum retained scaled wall-time debt before reporting discarded time.
    pub maximum_debt: Duration,
}

impl Default for BattleClockPolicy {
    fn default() -> Self {
        Self {
            tick_period: SIM_TICK_PERIOD,
            ticks_per_frame: 5,
            maximum_debt: Duration::from_millis(250),
        }
    }
}

/// Checked scheduling failure; no clock state changes when a frame is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BattleClockError {
    /// The configured period must match the authoritative 25 Hz simulation.
    #[error("battle clock requires the authoritative 40ms tick period")]
    TickPeriod,
    /// A display frame must permit at least one complete simulation tick.
    #[error("battle clock requires a nonzero per-frame tick budget")]
    TickBudget,
    /// Retained debt must allow at least one complete tick.
    #[error("maximum retained debt is shorter than one simulation tick")]
    MaximumDebt,
    /// The current client supports integral speeds 0 through 4.
    #[error("unsupported battle speed {0}; expected 0 through 4")]
    Speed(u32),
    /// Scaled duration, accumulated debt or diagnostic totals cannot be represented.
    #[error("battle scheduling duration or diagnostic counter overflow")]
    Overflow,
    /// A caller tried consuming a tick without sufficient accumulated time.
    #[error("battle clock tick consumed without available scheduling debt")]
    NoTickAvailable,
}

/// Read-only scheduling diagnostics, updated once per eligible display frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockDiagnostics {
    /// Exact scaled scheduling duration discarded by the most recent frame.
    pub discarded_last_frame: Duration,
    /// Total scaled scheduling duration discarded in this session.
    pub discarded_total: Duration,
    /// Number of frames that exceeded the configured debt limit.
    pub limited_frames: u64,
}

/// One prepared frame's bounded work and limit-transition notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockFrame {
    /// Complete updates available within this frame's configured budget.
    pub ticks: u32,
    /// True only when entering a consecutive series of debt-limited frames.
    pub entered_debt_limit: bool,
}

/// Persistent integer wall-time accumulator for the sole client tick owner.
#[derive(Resource, Clone, Debug, Default)]
pub struct BattleTickClock {
    policy: BattleClockPolicy,
    debt: Duration,
    diagnostics: ClockDiagnostics,
    limiting: bool,
}

impl BattleTickClock {
    /// Creates a clock after validating scheduling limits against the 25 Hz world.
    ///
    /// # Errors
    /// Rejects a different tick period, empty frame budget or insufficient debt cap.
    pub fn new(policy: BattleClockPolicy) -> Result<Self, BattleClockError> {
        if policy.tick_period != SIM_TICK_PERIOD {
            return Err(BattleClockError::TickPeriod);
        }
        if policy.ticks_per_frame == 0 {
            return Err(BattleClockError::TickBudget);
        }
        if policy.maximum_debt < policy.tick_period {
            return Err(BattleClockError::MaximumDebt);
        }
        Ok(Self {
            policy,
            ..Self::default()
        })
    }

    /// Adds a frame's elapsed wall time after integer speed scaling.
    ///
    /// No simulation tick is consumed here. Excess scaled time is recorded exactly;
    /// callers must execute each returned update and then call `consume_tick`.
    /// # Errors
    /// Unsupported speed or any checked duration/diagnostic overflow leaves state unchanged.
    pub fn observe(&mut self, delta: Duration, speed: u32) -> Result<ClockFrame, BattleClockError> {
        if speed > 4 {
            return Err(BattleClockError::Speed(speed));
        }
        if speed == 0 {
            self.reset_phase();
            return Ok(ClockFrame {
                ticks: 0,
                entered_debt_limit: false,
            });
        }
        let scaled = delta.checked_mul(speed).ok_or(BattleClockError::Overflow)?;
        let sum = self
            .debt
            .checked_add(scaled)
            .ok_or(BattleClockError::Overflow)?;
        let discarded = sum.saturating_sub(self.policy.maximum_debt);
        let debt = sum.min(self.policy.maximum_debt);
        let limiting = !discarded.is_zero();
        let total = self
            .diagnostics
            .discarded_total
            .checked_add(discarded)
            .ok_or(BattleClockError::Overflow)?;
        let limited_frames = self
            .diagnostics
            .limited_frames
            .checked_add(u64::from(limiting))
            .ok_or(BattleClockError::Overflow)?;
        let ticks = u32::try_from(
            (debt.as_nanos() / self.policy.tick_period.as_nanos())
                .min(u128::from(self.policy.ticks_per_frame)),
        )
        .map_err(|_| BattleClockError::Overflow)?;
        let frame = ClockFrame {
            ticks,
            entered_debt_limit: limiting && !self.limiting,
        };
        self.debt = debt;
        self.diagnostics = ClockDiagnostics {
            discarded_last_frame: discarded,
            discarded_total: total,
            limited_frames,
        };
        self.limiting = limiting;
        Ok(frame)
    }

    /// Removes exactly one executed tick's scheduling duration.
    /// # Errors
    /// Returns `NoTickAvailable` without mutation if no full tick was scheduled.
    pub fn consume_tick(&mut self) -> Result<(), BattleClockError> {
        self.debt = self
            .debt
            .checked_sub(self.policy.tick_period)
            .ok_or(BattleClockError::NoTickAvailable)?;
        Ok(())
    }

    /// Discards interpolation phase when pausing/suspending, keeping slowdown evidence.
    pub fn reset_phase(&mut self) {
        self.debt = Duration::ZERO;
        self.limiting = false;
        self.diagnostics.discarded_last_frame = Duration::ZERO;
    }

    /// Returns retained scaled scheduling debt, including budget-limited whole ticks.
    #[must_use]
    pub fn debt(&self) -> Duration {
        self.debt
    }

    /// Returns current session slowdown diagnostics without allocating text.
    #[must_use]
    pub fn diagnostics(&self) -> ClockDiagnostics {
        self.diagnostics
    }

    /// Returns the interpolation fraction, clamped to 1 while whole-tick debt remains.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.debt >= self.policy.tick_period {
            1.0
        } else {
            self.debt.as_secs_f32() / self.policy.tick_period.as_secs_f32()
        }
    }
}

#[cfg(test)]
mod tests;
