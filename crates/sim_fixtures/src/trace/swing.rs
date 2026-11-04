//! Inference of weapon events from two consecutive weapon phases of one unit.
//!
//! # Responsibility
//! The simulation publishes no "shot released" or "swing cancelled" event. It does publish each
//! unit's [`WeaponPhase`] after every tick, and the weapon state machine
//! (`sim_core::combat::step_entity_weapon`) moves between phases in a fixed pattern. This module
//! reads that pattern back: given the phase at the end of the previous tick and at the end of
//! this one, it says whether a swing started, continued, released or was cancelled.
//!
//! # How it fits
//! [`super::BattleTrace`] calls [`infer`] once per living unit per tick. The rules mirror
//! `step_ready_weapon`, `step_windup` and `step_entity_weapon` in `sim_core/src/combat.rs`:
//! - a swing starts as `Windup { target, remaining = damage point }`;
//! - each tick a wind-up with `remaining >= 2` becomes `Windup { target, remaining - 1 }`;
//! - a wind-up with `remaining <= 1` either releases (the phase becomes `Backswing { target }`
//!   or `Cooldown`) or is cancelled (any other phase), because the release tick never leaves a
//!   wind-up behind;
//! - a wind-up with `remaining == 0` exists only for a weapon whose damage point is 0, which
//!   releases on the next tick (the "period + 1" reading of the evidence file, §2.5).
//!
//! # Invariants
//! - Exactly one [`SwingEvent`] per call; a cancelled swing that restarts in the same tick is
//!   reported as [`SwingEvent::CancelledAndRestarted`], so both halves are counted.
//! - A unit that died this tick is never passed in: its unfinished swing is not a cancellation.

use sim_core::WeaponPhase;
use sim_types::EntityId;

/// What one unit's weapon did during one tick, read from its phase before and after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwingEvent {
    /// No swing was in progress, and none started.
    Idle,
    /// A new swing started against `target`.
    Started {
        /// The target the swing is bound to.
        target: EntityId,
    },
    /// The swing in progress continued its wind-up.
    WindingUp,
    /// The swing in progress released its shot at `target`.
    Released {
        /// The target the swing was bound to.
        target: EntityId,
    },
    /// The swing against `target` ended without a shot.
    Cancelled {
        /// The target the swing was bound to.
        target: EntityId,
    },
    /// The swing against `cancelled` ended without a shot, and a new one against `started`
    /// began in the same tick.
    CancelledAndRestarted {
        /// The target the cancelled swing was bound to.
        cancelled: EntityId,
        /// The target the new swing is bound to.
        started: EntityId,
    },
}

/// The swing event that explains a weapon going from phase `before` to phase `after` in one tick.
#[must_use]
pub fn infer(before: &WeaponPhase, after: &WeaponPhase) -> SwingEvent {
    let WeaponPhase::Windup {
        target_id: bound,
        remaining_ticks,
    } = *before
    else {
        return match *after {
            WeaponPhase::Windup { target_id, .. } => SwingEvent::Started { target: target_id },
            _ => SwingEvent::Idle,
        };
    };
    match *after {
        WeaponPhase::Windup {
            target_id,
            remaining_ticks: left,
        } if remaining_ticks >= 2 && target_id == bound && left == remaining_ticks - 1 => {
            SwingEvent::WindingUp
        }
        WeaponPhase::Windup { target_id, .. } => SwingEvent::CancelledAndRestarted {
            cancelled: bound,
            started: target_id,
        },
        WeaponPhase::Backswing { target_id, .. } if target_id == bound => {
            SwingEvent::Released { target: bound }
        }
        WeaponPhase::Cooldown if remaining_ticks <= 1 => SwingEvent::Released { target: bound },
        _ => SwingEvent::Cancelled { target: bound },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: EntityId = EntityId(7);
    const B: EntityId = EntityId(9);

    fn windup(target_id: EntityId, remaining_ticks: u16) -> WeaponPhase {
        WeaponPhase::Windup {
            target_id,
            remaining_ticks,
        }
    }

    fn backswing(target_id: EntityId) -> WeaponPhase {
        WeaponPhase::Backswing {
            target_id,
            remaining_ticks: 6,
        }
    }

    #[test]
    fn a_ready_weapon_that_enters_its_wind_up_starts_a_swing() {
        assert_eq!(
            infer(&WeaponPhase::Ready, &windup(A, 4)),
            SwingEvent::Started { target: A }
        );
        assert_eq!(
            infer(
                &WeaponPhase::RotatingToTarget { target_id: A },
                &windup(A, 4)
            ),
            SwingEvent::Started { target: A }
        );
    }

    #[test]
    fn phases_outside_a_wind_up_are_idle() {
        let idle = [
            WeaponPhase::Ready,
            WeaponPhase::Cooldown,
            backswing(A),
            WeaponPhase::RotatingToTarget { target_id: A },
        ];
        for before in &idle {
            for after in &idle {
                assert_eq!(
                    infer(before, after),
                    SwingEvent::Idle,
                    "{before:?} -> {after:?}"
                );
            }
        }
    }

    #[test]
    fn a_wind_up_counting_down_on_the_same_target_continues() {
        assert_eq!(infer(&windup(A, 4), &windup(A, 3)), SwingEvent::WindingUp);
        assert_eq!(infer(&windup(A, 2), &windup(A, 1)), SwingEvent::WindingUp);
    }

    #[test]
    fn a_wind_up_that_reaches_its_backswing_or_cooldown_released() {
        assert_eq!(
            infer(&windup(A, 1), &backswing(A)),
            SwingEvent::Released { target: A }
        );
        // A weapon without a backswing goes straight to its cooldown.
        assert_eq!(
            infer(&windup(A, 1), &WeaponPhase::Cooldown),
            SwingEvent::Released { target: A }
        );
        // Damage point 0: the wind-up holds `remaining = 0` for one tick, then releases.
        assert_eq!(
            infer(&windup(A, 0), &backswing(A)),
            SwingEvent::Released { target: A }
        );
    }

    #[test]
    fn a_wind_up_that_falls_back_to_ready_was_cancelled() {
        // Target died or left the band at release, or the swing lost its target mid-way.
        assert_eq!(
            infer(&windup(A, 1), &WeaponPhase::Ready),
            SwingEvent::Cancelled { target: A }
        );
        assert_eq!(
            infer(&windup(A, 3), &WeaponPhase::Ready),
            SwingEvent::Cancelled { target: A }
        );
        // A mid-swing order may drop the wind-up to its cooldown; no shot was released.
        assert_eq!(
            infer(&windup(A, 3), &WeaponPhase::Cooldown),
            SwingEvent::Cancelled { target: A }
        );
    }

    #[test]
    fn a_wind_up_that_restarts_was_cancelled_and_restarted() {
        assert_eq!(
            infer(&windup(A, 2), &windup(B, 4)),
            SwingEvent::CancelledAndRestarted {
                cancelled: A,
                started: B
            }
        );
        // Same target but the countdown jumped back: a new swing.
        assert_eq!(
            infer(&windup(A, 2), &windup(A, 4)),
            SwingEvent::CancelledAndRestarted {
                cancelled: A,
                started: A
            }
        );
        // Damage point 0: a wind-up at 0 followed by another at 0 is a cancel and a restart.
        assert_eq!(
            infer(&windup(A, 0), &windup(A, 0)),
            SwingEvent::CancelledAndRestarted {
                cancelled: A,
                started: A
            }
        );
    }

    #[test]
    fn a_backswing_on_another_target_is_not_a_release_of_this_swing() {
        assert_eq!(
            infer(&windup(A, 1), &backswing(B)),
            SwingEvent::Cancelled { target: A }
        );
    }
}
