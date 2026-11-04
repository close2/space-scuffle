//! Inference of weapon events from two consecutive samples of one unit.
//!
//! # Responsibility
//! The simulation publishes no "shot released" or "swing cancelled" event. It does publish each
//! unit's [`WeaponPhase`] and weapon cooldown after every tick. This module reads them back:
//! given a unit's sample at the end of the previous tick and at the end of this one, it says
//! whether a swing started, continued, released or was cancelled.
//!
//! # How it fits
//! [`super::BattleTrace`] calls [`infer`] once per living unit per tick. Two rules, read from
//! `step_ready_weapon`, `step_windup` and `step_entity_weapon` in `sim_core/src/combat.rs`:
//!
//! - **Releases come from the cooldown, not from the phases.** A weapon's cooldown
//!   (`Entity::weapon_cooldown_remaining`) is otherwise only ever counted down by one, or left
//!   alone on a tick the combat phase skips the weapon (no weapon against the target's layer,
//!   channelling, cargo, an unfinished unit). A release charges it with `period − damage point`.
//!   So a shot was released in the tick **iff the cooldown after the tick is greater than
//!   before it** ([`released`]). This holds whatever phases the release passes through: a
//!   wind-up that reaches its backswing or cooldown, a swing that starts and releases in one
//!   tick (damage point 0, plan `2026-11-04-combat-sc2-alignment` D-3), or a release straight
//!   out of a backswing (D-2).
//! - **Starts and cancellations come from the phases.** A swing starts as
//!   `Windup { target, remaining = damage point }`; each tick a wind-up with `remaining >= 2`
//!   becomes `Windup { target, remaining − 1 }`; a wind-up that neither continued nor released
//!   was cancelled, and if the next phase is again a wind-up a new swing started in the same tick.
//!   A backswing the next swing cuts short (D-2) is therefore a start, like any other.
//!
//! # Invariants
//! - Exactly one [`SwingEvent`] per call; a cancelled swing that restarts in the same tick is
//!   reported as [`SwingEvent::CancelledAndRestarted`], so both halves are counted.
//! - A unit that died this tick is never passed in: its unfinished swing is not a cancellation.
//! - **Bound of the release rule.** A release is seen only if the cooldown it charges exceeds the
//!   cooldown before the tick. A swing starts only once the cooldown has run out, so the cooldown
//!   before a release is 0, or 1 when the swing starts out of a backswing on the tick its cooldown
//!   ends (D-2). The rule therefore needs `period − damage point >= 1` for every resolved period
//!   (upgrades and Stimpack applied), and `period >= 2` for a damage point of 0. Every shipped
//!   weapon meets it: the shortest resolved period is 9 ticks (no weapon with a shorter period
//!   under Stimpack or an `attack_cooldown_delta`), and no damage point reaches its period. A rule that counted a cooldown standing still as a
//!   release would misread every tick the combat phase skips a weapon, so a standing cooldown is
//!   never a release.

use super::UnitSample;
use sim_core::WeaponPhase;
use sim_types::EntityId;

/// What one unit's weapon did during one tick, read from its samples before and after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwingEvent {
    /// No swing was in progress, and none started or released.
    Idle,
    /// A new swing started against `target`.
    Started {
        /// The target the swing is bound to.
        target: EntityId,
    },
    /// The swing in progress continued its wind-up.
    WindingUp,
    /// A shot was released at `target`.
    Released {
        /// The target of the shot: the swing's bound target, otherwise the unit's attack target;
        /// `None` if neither is known.
        target: Option<EntityId>,
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

/// Whether a weapon whose cooldown went from `before` to `after` (ticks left, at the end of two
/// consecutive ticks) released a shot in the second tick: the cooldown rose.
#[must_use]
pub const fn released(before: u16, after: u16) -> bool {
    after > before
}

/// The swing event that explains a unit going from sample `before` to sample `after` in one
/// tick.
#[must_use]
pub fn infer(before: &UnitSample, after: &UnitSample) -> SwingEvent {
    let bound = match before.phase {
        WeaponPhase::Windup {
            target_id,
            remaining_ticks,
        } => Some((target_id, remaining_ticks)),
        _ => None,
    };
    if released(before.cooldown, after.cooldown) {
        let in_backswing = match after.phase {
            WeaponPhase::Backswing { target_id, .. } => Some(target_id),
            _ => None,
        };
        let target = bound
            .map(|(target, _)| target)
            .or(in_backswing)
            .or(after.target)
            .or(before.target);
        return SwingEvent::Released { target };
    }
    let Some((bound, remaining_ticks)) = bound else {
        return match after.phase {
            WeaponPhase::Windup { target_id, .. } => SwingEvent::Started { target: target_id },
            _ => SwingEvent::Idle,
        };
    };
    match after.phase {
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
        _ => SwingEvent::Cancelled { target: bound },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{Activity, Team};
    use sim_types::{PlayerId, SimScalar, SimVec2, UnitTypeId};

    const A: EntityId = EntityId(7);
    const B: EntityId = EntityId(9);

    const RELEASED_AT_A: SwingEvent = SwingEvent::Released { target: Some(A) };

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

    /// A unit attacking `A` whose weapon is in `phase` with `cooldown` ticks left.
    fn at(phase: WeaponPhase, cooldown: u16) -> UnitSample {
        UnitSample {
            id: EntityId(1),
            team: Team::Red,
            unit_type: UnitTypeId(0),
            owner: PlayerId(1),
            position: SimVec2::ZERO,
            velocity: SimVec2::ZERO,
            hp: SimScalar::ONE,
            shields: SimScalar::ZERO,
            phase,
            cooldown,
            target: Some(A),
            activity: Activity::Engaged,
        }
    }

    /// The event of a weapon going from `before` to `after`, each a phase and a cooldown.
    fn event(before: (WeaponPhase, u16), after: (WeaponPhase, u16)) -> SwingEvent {
        infer(&at(before.0, before.1), &at(after.0, after.1))
    }

    #[test]
    fn a_ready_weapon_that_enters_its_wind_up_starts_a_swing() {
        assert_eq!(
            event((WeaponPhase::Ready, 0), (windup(A, 4), 0)),
            SwingEvent::Started { target: A }
        );
        assert_eq!(
            event(
                (WeaponPhase::RotatingToTarget { target_id: A }, 0),
                (windup(A, 4), 0)
            ),
            SwingEvent::Started { target: A }
        );
    }

    #[test]
    fn phases_outside_a_wind_up_with_a_falling_cooldown_are_idle() {
        let idle = [
            WeaponPhase::Ready,
            WeaponPhase::Cooldown,
            backswing(A),
            WeaponPhase::RotatingToTarget { target_id: A },
        ];
        for before in &idle {
            for after in &idle {
                for (from, to) in [(5, 4), (1, 0), (0, 0)] {
                    assert_eq!(
                        event((before.clone(), from), (after.clone(), to)),
                        SwingEvent::Idle,
                        "{before:?} ({from}) -> {after:?} ({to})"
                    );
                }
            }
        }
    }

    #[test]
    fn a_cooldown_that_does_not_run_is_not_a_release() {
        // The combat phase skips a weapon it cannot step (no weapon against the target's layer,
        // channelling, cargo, an unfinished unit): its cooldown then stands still.
        assert_eq!(
            event((WeaponPhase::Cooldown, 5), (WeaponPhase::Cooldown, 5)),
            SwingEvent::Idle
        );
        assert_eq!(
            event((backswing(A), 1), (backswing(A), 1)),
            SwingEvent::Idle
        );
    }

    #[test]
    fn a_wind_up_counting_down_on_the_same_target_continues() {
        assert_eq!(
            event((windup(A, 4), 0), (windup(A, 3), 0)),
            SwingEvent::WindingUp
        );
        assert_eq!(
            event((windup(A, 2), 0), (windup(A, 1), 0)),
            SwingEvent::WindingUp
        );
    }

    #[test]
    fn a_wind_up_that_reaches_its_backswing_or_cooldown_released() {
        assert_eq!(event((windup(A, 1), 0), (backswing(A), 13)), RELEASED_AT_A);
        // A weapon without a backswing goes straight to its cooldown.
        assert_eq!(
            event((windup(A, 1), 0), (WeaponPhase::Cooldown, 13)),
            RELEASED_AT_A
        );
        // Damage point 0 before the cadence fix: the wind-up holds `remaining = 0` for one tick.
        assert_eq!(event((windup(A, 0), 0), (backswing(A), 22)), RELEASED_AT_A);
    }

    #[test]
    fn a_swing_that_starts_and_releases_in_one_tick_is_counted() {
        // Damage point 0 after the cadence fix (plan D-3): no wind-up is ever published.
        assert_eq!(
            event((WeaponPhase::Ready, 0), (WeaponPhase::Cooldown, 21)),
            RELEASED_AT_A
        );
        assert_eq!(
            event((WeaponPhase::Ready, 0), (backswing(A), 21)),
            RELEASED_AT_A
        );
    }

    #[test]
    fn a_backswing_cut_short_by_the_next_swing_is_a_start() {
        // The backswing rule of plan D-2: the cooldown runs out during the backswing.
        assert_eq!(
            event((backswing(A), 1), (windup(B, 1), 0)),
            SwingEvent::Started { target: B }
        );
    }

    #[test]
    fn a_backswing_cut_short_by_an_immediate_release_is_a_release() {
        assert_eq!(
            event((backswing(A), 1), (backswing(B), 10)),
            SwingEvent::Released { target: Some(B) }
        );
    }

    #[test]
    fn a_release_names_the_bound_target_before_the_attack_target() {
        // The wind-up was bound to B while the unit's order names A.
        assert_eq!(
            event((windup(B, 1), 0), (WeaponPhase::Cooldown, 9)),
            SwingEvent::Released { target: Some(B) }
        );
        // Without a wind-up or a backswing, the attack target is the one known.
        let mut before = at(WeaponPhase::Ready, 0);
        let mut after = at(WeaponPhase::Cooldown, 9);
        assert_eq!(infer(&before, &after), RELEASED_AT_A);
        // The shot killed its target, so after the tick the unit has none: the earlier one counts.
        after.target = None;
        assert_eq!(infer(&before, &after), RELEASED_AT_A);
        before.target = None;
        assert_eq!(
            infer(&before, &after),
            SwingEvent::Released { target: None }
        );
    }

    #[test]
    fn a_wind_up_that_falls_back_to_ready_was_cancelled() {
        // Target died or left the band at release, or the swing lost its target mid-way.
        assert_eq!(
            event((windup(A, 1), 0), (WeaponPhase::Ready, 0)),
            SwingEvent::Cancelled { target: A }
        );
        assert_eq!(
            event((windup(A, 3), 0), (WeaponPhase::Ready, 0)),
            SwingEvent::Cancelled { target: A }
        );
        // A wind-up dropped to its cooldown without the cooldown being charged released nothing.
        assert_eq!(
            event((windup(A, 1), 0), (WeaponPhase::Cooldown, 0)),
            SwingEvent::Cancelled { target: A }
        );
        assert_eq!(
            event((windup(A, 3), 0), (WeaponPhase::Cooldown, 0)),
            SwingEvent::Cancelled { target: A }
        );
    }

    #[test]
    fn a_wind_up_that_restarts_was_cancelled_and_restarted() {
        assert_eq!(
            event((windup(A, 2), 0), (windup(B, 4), 0)),
            SwingEvent::CancelledAndRestarted {
                cancelled: A,
                started: B
            }
        );
        // Same target but the countdown jumped back: a new swing.
        assert_eq!(
            event((windup(A, 2), 0), (windup(A, 4), 0)),
            SwingEvent::CancelledAndRestarted {
                cancelled: A,
                started: A
            }
        );
        // Damage point 0: a wind-up at 0 followed by another at 0 is a cancel and a restart.
        assert_eq!(
            event((windup(A, 0), 0), (windup(A, 0), 0)),
            SwingEvent::CancelledAndRestarted {
                cancelled: A,
                started: A
            }
        );
    }

    #[test]
    fn a_backswing_without_a_charged_cooldown_is_not_a_release() {
        assert_eq!(
            event((windup(A, 1), 0), (backswing(B), 0)),
            SwingEvent::Cancelled { target: A }
        );
    }
}
