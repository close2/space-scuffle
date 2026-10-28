//! Exercises exact scheduling debt, overflow atomicity and bounded catch-up.

use super::*;

type TestResult = Result<(), BattleClockError>;

#[test]
fn ordinary_frames_retain_fraction_and_consume_only_executed_ticks() -> TestResult {
    let mut clock = BattleTickClock::default();
    assert_eq!(clock.observe(Duration::from_millis(10), 1)?.ticks, 0);
    assert_eq!(clock.fraction(), 0.25);
    assert_eq!(clock.observe(Duration::from_millis(70), 1)?.ticks, 2);
    clock.consume_tick()?;
    clock.consume_tick()?;
    assert_eq!(clock.debt(), Duration::ZERO);
    assert_eq!(clock.consume_tick(), Err(BattleClockError::NoTickAvailable));
    Ok(())
}

#[test]
fn long_stall_discards_scaled_wall_time_but_preserves_bounded_tick_debt() -> TestResult {
    let mut clock = BattleTickClock::default();
    let frame = clock.observe(Duration::from_secs(1), 4)?;
    assert_eq!(
        frame,
        ClockFrame {
            ticks: 5,
            entered_debt_limit: true
        }
    );
    assert_eq!(
        clock.diagnostics().discarded_last_frame,
        Duration::from_millis(3750)
    );
    for _ in 0..frame.ticks {
        clock.consume_tick()?;
    }
    assert_eq!(clock.debt(), Duration::from_millis(50));
    assert_eq!(clock.fraction(), 1.0);
    assert_eq!(clock.observe(Duration::ZERO, 1)?.ticks, 1);
    clock.consume_tick()?;
    assert_eq!(clock.debt(), Duration::from_millis(10));
    assert_eq!(clock.diagnostics().limited_frames, 1);
    assert_eq!(clock.diagnostics().discarded_last_frame, Duration::ZERO);
    Ok(())
}

#[test]
fn invalid_speed_and_duration_overflows_leave_clock_unchanged() -> TestResult {
    let mut clock = BattleTickClock::default();
    clock.observe(Duration::from_millis(10), 1)?;
    let before = clock.clone();
    assert_eq!(
        clock.observe(Duration::from_millis(20), 5),
        Err(BattleClockError::Speed(5))
    );
    assert_eq!(
        clock.observe(Duration::MAX, 4),
        Err(BattleClockError::Overflow)
    );
    assert_eq!(
        clock.observe(Duration::MAX, 1),
        Err(BattleClockError::Overflow)
    );
    assert_eq!(clock.debt(), before.debt());
    assert_eq!(clock.diagnostics(), before.diagnostics());
    Ok(())
}

#[test]
fn diagnostic_overflow_rejects_before_retained_debt_mutates() {
    let mut clock = BattleTickClock::default();
    clock.diagnostics.discarded_total = Duration::MAX;
    assert_eq!(
        clock.observe(Duration::from_secs(1), 1),
        Err(BattleClockError::Overflow)
    );
    assert_eq!(clock.debt(), Duration::ZERO);
    clock.diagnostics.discarded_total = Duration::ZERO;
    clock.diagnostics.limited_frames = u64::MAX;
    assert_eq!(
        clock.observe(Duration::from_secs(1), 1),
        Err(BattleClockError::Overflow)
    );
    assert_eq!(clock.debt(), Duration::ZERO);
}

#[test]
fn policy_rejects_tick_rate_changes_empty_budgets_and_too_small_debt() {
    for (policy, expected) in [
        (
            BattleClockPolicy {
                tick_period: Duration::from_millis(20),
                ..Default::default()
            },
            BattleClockError::TickPeriod,
        ),
        (
            BattleClockPolicy {
                ticks_per_frame: 0,
                ..Default::default()
            },
            BattleClockError::TickBudget,
        ),
        (
            BattleClockPolicy {
                maximum_debt: Duration::from_millis(39),
                ..Default::default()
            },
            BattleClockError::MaximumDebt,
        ),
    ] {
        assert_eq!(BattleTickClock::new(policy).err(), Some(expected));
    }
}

#[test]
fn speed_zero_pauses_even_after_a_budget_limited_frame() -> TestResult {
    let mut clock = BattleTickClock::default();
    let frame = clock.observe(Duration::from_secs(1), 1)?;
    for _ in 0..frame.ticks {
        clock.consume_tick()?;
    }
    assert_eq!(clock.debt(), Duration::from_millis(50));
    let total = clock.diagnostics().discarded_total;
    assert_eq!(clock.observe(Duration::MAX, 0)?.ticks, 0);
    assert_eq!(clock.debt(), Duration::ZERO);
    assert_eq!(clock.diagnostics().discarded_last_frame, Duration::ZERO);
    assert_eq!(clock.diagnostics().discarded_total, total);
    assert_eq!(clock.observe(Duration::ZERO, 1)?.ticks, 0);
    Ok(())
}
