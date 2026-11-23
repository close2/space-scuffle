//! Intent steering of plan `2026-11-20-contact-solver` (I-1, I-2, I-4; rule S of Amendments 14 and
//! 15): the velocity a unit wants before contact is resolved.
//!
//! # Responsibility
//! [`intent_velocity`] turns a unit's direction to its next route point into the velocity it
//! proposes this tick, as a fraction of its step: its preferred velocity, corrected by a smooth,
//! memoryless velocity term over the bodies it would meet within its horizon (rule S; Guy &
//! Karamouzas, *Game AI Pro 2* ch. 19, eq. 19.5; Karamouzas, Skinner & Guy 2014; Weiss et al.
//! 2019). Each such body pushes the unit along the predicted line of centres at collision, by just
//! the approach along it, so the push is continuous through contact (where the line of centres is
//! the solver's own normal) and never fights the solver's radial correction.
//!
//! # How it fits
//! The solver pipeline's `step_toward` calls it instead of legacy steering and scales the result
//! by the unit's step (I-1, with today's braking, I-2); the contact solver then separates whatever
//! contact the steps made. It reads only the tick-start neighbour snapshot and keeps no memory,
//! so the result does not depend on the order units are processed in, nor on anything but this
//! tick's snapshot.
//!
//! # The term (every constant derived, none tuned; fixed before any measurement)
//! For a candidate `b` with offset `p` (unit to body), relative preferred velocity
//! `v = v_pref − v_b` and reach `R = r + r_b + margin`:
//! - **Time to collision** `τ` (ticks): the first root of `|p − τv| = R`; `0` in contact while
//!   closing. A body the unit does not close on is no candidate.
//! - **Horizon** `H = h · (r + r_b) / speed` with `h = contact.side_horizon_factor` (I-3a).
//! - **Weight** `w = 1 − τ / H`: linear, 1 at contact and 0 at the horizon.
//! - **Direction** `n`: the unit vector from the body to the unit at the predicted collision,
//!   `−(p − τv) / |p − τv|`; at contact, the line of centres.
//! - **Size:** the approach along `n`, `max(0, −v·n)`: only the closing part is removed.
//! - **Responsibility** `ρ`: ½ against a body that moves under an order and is not anchored (it
//!   runs the same rule and takes the other half; van den Berg et al. 2011), 1 against a standing,
//!   engaged or holding body or an enemy that stands.
//! - **Right-hand bias (I-3b), near head-on only:** `ρ · w · (|v| / h) · (1 − m / R)⁺` along the
//!   right normal of `v`, where `m = |v × p| / |v|` is the miss distance of the straight course.
//!   `|v| / h` is the sideways speed whose turn `asin(1/h)` (I-3a's bound) clears a body met at
//!   the horizon; the factor `(1 − m / R)` takes it from full at dead ahead to nothing once the
//!   straight course misses. Both bodies of a pair turn right in their own frames (the right
//!   normal of `−v` is the opposite world direction), so the bias is antisymmetric.
//! - **Combining:** the corrections are averaged over the candidates (Jacobi), never
//!   renormalised.
//! - **Clamps:** no component backward along the preferred direction; at most the unit's step;
//!   a turn from the preferred direction of at most `asin(1/h)`.
//!
//! # Invariants
//! - **No memory and no id:** the result is a function of the snapshot; corrections are summed in
//!   Q32.32, which is exact integer addition, before one division by their count.
//! - **Continuous through contact:** direction and size are continuous in the bodies' positions
//!   as the gap closes to zero and below.
//! - Friends moving with the unit (I-4), idle friends that step aside (L6) and the chase target
//!   are never candidates.
//! - The result has length at most 1 (a fraction of the step), or is zero at the point.

use super::math::{checked_dot, checked_vec_add, checked_vec_scale, checked_vec_sub, direction};
use super::tuning::{COMOVING_MIN_COSINE, STEERING_CORRIDOR_MARGIN};
use super::MovementMathError;
use crate::steering::{ContactBody, SteeringObstacle};
use sim_types::{EntityId, SimScalar, SimVec2};

/// Everything [`intent_velocity`] reads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IntentInput<'a> {
    /// The unit.
    pub body: ContactBody,
    /// Its next route point (or gap point), world units.
    pub next: SimVec2,
    /// Its step this tick, world units per tick (accelerated and braked).
    pub speed: SimScalar,
    /// A body never steered round: the chase target.
    pub ignored: Option<EntityId>,
    /// The tick-start neighbours.
    pub neighbours: &'a [SteeringObstacle],
    /// `contact.side_horizon_factor`, `h` (I-3a), at least 1.
    pub horizon_factor: u8,
}

fn mul(a: SimScalar, b: SimScalar) -> Result<SimScalar, MovementMathError> {
    a.checked_mul(b).ok_or(MovementMathError::Unrepresentable)
}

fn div(a: SimScalar, b: SimScalar) -> Result<SimScalar, MovementMathError> {
    a.checked_div(b).ok_or(MovementMathError::Unrepresentable)
}

fn sub(a: SimScalar, b: SimScalar) -> Result<SimScalar, MovementMathError> {
    a.checked_sub(b).ok_or(MovementMathError::Unrepresentable)
}

fn root(value: SimScalar) -> Result<SimScalar, MovementMathError> {
    value
        .max(SimScalar::ZERO)
        .sqrt()
        .map_err(|_| MovementMathError::Unrepresentable)
}

/// `a × b`, the z component of the cross product, checked.
fn cross(a: SimVec2, b: SimVec2) -> Result<SimScalar, MovementMathError> {
    let x = i128::from(a.x.raw()) * i128::from(b.y.raw());
    let y = i128::from(a.y.raw()) * i128::from(b.x.raw());
    let raw = x.checked_sub(y).ok_or(MovementMathError::Unrepresentable)? >> 32;
    i64::try_from(raw)
        .map(SimScalar::from_raw)
        .map_err(|_| MovementMathError::Unrepresentable)
}

/// The right normal of the unit vector `u`.
fn right_of(u: SimVec2) -> SimVec2 {
    SimVec2::new(u.y, -u.x)
}

/// Whether `obstacle` is a friend moving with the unit, whose direction is `heading` (I-4): its
/// last displacement points within 60° of it.
fn co_moving(heading: SimVec2, obstacle: &SteeringObstacle) -> Result<bool, MovementMathError> {
    if obstacle.velocity == SimVec2::ZERO {
        return Ok(false);
    }
    let along = direction(obstacle.velocity, SimVec2::ZERO)?;
    Ok(checked_dot(heading, along)? >= COMOVING_MIN_COSINE)
}

/// Whether `obstacle` is excluded before any geometry: the unit itself, the ignored body (the
/// chase target), or an idle friend that steps aside (L6).
fn passed_over(input: &IntentInput<'_>, obstacle: &SteeringObstacle) -> bool {
    let body = input.body;
    obstacle.id == body.id
        || Some(obstacle.id) == input.ignored
        || obstacle.yields_to(body.owner, body.push_priority)
}

/// Whether `obstacle` runs the same rule this tick and so takes half the avoidance: it moves
/// under an order and does not hold its ground.
fn reciprocal(obstacle: &SteeringObstacle) -> bool {
    !obstacle.idle && !obstacle.anchored && !obstacle.is_standing()
}

/// One candidate's correction (see the module documentation), or `None` when the unit does not
/// close on it within its horizon.
fn correction(
    input: &IntentInput<'_>,
    preferred: SimVec2,
    obstacle: &SteeringObstacle,
) -> Result<Option<SimVec2>, MovementMathError> {
    let body = input.body;
    let offset = checked_vec_sub(obstacle.pos, body.pos)?;
    let relative = checked_vec_sub(preferred, obstacle.velocity)?;
    let closing = checked_dot(offset, relative)?;
    let speed_sq = checked_dot(relative, relative)?;
    if closing <= SimScalar::ZERO || speed_sq <= SimScalar::ZERO {
        return Ok(None);
    }
    let reach = body
        .radius
        .checked_add(obstacle.radius)
        .and_then(|sum| sum.checked_add(STEERING_CORRIDOR_MARGIN))
        .ok_or(MovementMathError::Unrepresentable)?;
    let gap_sq = sub(checked_dot(offset, offset)?, mul(reach, reach)?)?;
    let time = if gap_sq <= SimScalar::ZERO {
        SimScalar::ZERO
    } else {
        let discriminant = sub(mul(closing, closing)?, mul(speed_sq, gap_sq)?)?;
        if discriminant < SimScalar::ZERO {
            return Ok(None);
        }
        div(sub(closing, root(discriminant)?)?, speed_sq)?
    };
    let horizon = SimScalar::from_num(input.horizon_factor)
        .checked_mul(body.radius.saturating_add(obstacle.radius))
        .and_then(|span| span.checked_div(input.speed))
        .unwrap_or(SimScalar::MAX);
    if time > horizon {
        return Ok(None);
    }
    let weight = sub(SimScalar::ONE, div(time, horizon)?)?.clamp(SimScalar::ZERO, SimScalar::ONE);
    let share = if reciprocal(obstacle) {
        SimScalar::HALF
    } else {
        SimScalar::ONE
    };
    // The line from the body to the unit at the predicted collision.
    let apart = checked_vec_sub(checked_vec_scale(relative, time)?, offset)?;
    let normal = direction(apart, SimVec2::ZERO)?;
    let approach = SimScalar::ZERO
        .saturating_sub(checked_dot(relative, normal)?)
        .max(SimScalar::ZERO);
    let push = checked_vec_scale(normal, mul(mul(weight, share)?, approach)?)?;
    // The right-hand bias near head-on (I-3b), from I-3a's sideways speed |v| / h.
    let speed = root(speed_sq)?;
    let along = SimVec2::new(div(relative.x, speed)?, div(relative.y, speed)?);
    let miss = div(cross(relative, offset)?.abs(), speed)?;
    let near = sub(SimScalar::ONE, div(miss, reach)?)?.max(SimScalar::ZERO);
    let sideways = div(speed, SimScalar::from_num(input.horizon_factor.max(1)))?;
    let bias = checked_vec_scale(
        right_of(along),
        mul(mul(mul(weight, share)?, sideways)?, near)?,
    )?;
    Ok(Some(checked_vec_add(push, bias)?))
}

/// The mean correction over the candidates. A neighbour counts when it is not passed over
/// ([`passed_over`]), is not a co-moving friend (I-4), and `admits` it (the neighbour test
/// deferred for far bodies, see `locomotion::gather_neighbours`).
// PERFORMANCE RATIONALE:
// Why: Under the solver the neighbour list is about five times longer than the legacy one, and
// the co-moving test normalises each friend's velocity (a root and two divisions), most of it
// for bodies the unit does not close on.
// What: The cheap geometry of [`correction`] runs first; the co-moving test and the deferred
// ground test run only for a body that yields a correction. Both are exclusions that never fail,
// so the mean is the same as testing in the order of the rules.
// Trade-off: The rule's order in code differs from its order in the plan's text.
fn mean_correction(
    input: &IntentInput<'_>,
    (straight, preferred): (SimVec2, SimVec2),
    admits: &impl Fn(&SteeringObstacle) -> bool,
) -> Result<SimVec2, MovementMathError> {
    let mut sum = SimVec2::ZERO;
    let mut count = 0_i32;
    for obstacle in input.neighbours {
        if passed_over(input, obstacle) {
            continue;
        }
        let Some(delta) = correction(input, preferred, obstacle)? else {
            continue;
        };
        if (obstacle.owner == input.body.owner && co_moving(straight, obstacle)?)
            || !admits(obstacle)
        {
            continue;
        }
        sum = checked_vec_add(sum, delta)?;
        count += 1;
    }
    if count <= 1 {
        return Ok(sum);
    }
    let count = SimScalar::from_num(count);
    Ok(SimVec2::new(div(sum.x, count)?, div(sum.y, count)?))
}

/// `velocity` clamped (see the module documentation): no backward component along `straight`,
/// at most `speed` long, and turned from `straight` by at most `asin(1/h)`.
fn clamped(
    velocity: SimVec2,
    straight: SimVec2,
    speed: SimScalar,
    factor: u8,
) -> Result<SimVec2, MovementMathError> {
    let along = checked_dot(velocity, straight)?;
    let mut v = if along < SimScalar::ZERO {
        checked_vec_sub(velocity, checked_vec_scale(straight, along)?)?
    } else {
        velocity
    };
    let length = root(checked_dot(v, v)?)?;
    if length > speed {
        v = checked_vec_scale(direction(v, SimVec2::ZERO)?, speed)?;
    }
    let length = root(checked_dot(v, v)?)?;
    if length <= SimScalar::ZERO {
        return Ok(v);
    }
    let sine = div(SimScalar::ONE, SimScalar::from_num(factor.max(1)))?;
    let cosine = root(sub(SimScalar::ONE, mul(sine, sine)?)?)?;
    let unit = direction(v, SimVec2::ZERO)?;
    if checked_dot(unit, straight)? >= cosine {
        return Ok(v);
    }
    let left = cross(straight, unit)? > SimScalar::ZERO;
    let side = if left {
        SimVec2::new(-straight.y, straight.x)
    } else {
        right_of(straight)
    };
    let turned = checked_vec_add(
        checked_vec_scale(straight, cosine)?,
        checked_vec_scale(side, sine)?,
    )?;
    checked_vec_scale(turned, length)
}

/// The velocity the unit proposes this tick, as a fraction of its step (length at most 1; see the
/// module documentation). `admits` completes the neighbour test of a listed body (`true` for
/// every body when the list is exact).
///
/// # Errors
/// An unrepresentable intermediate of the term's geometry.
pub(crate) fn intent_velocity(
    input: &IntentInput<'_>,
    admits: impl Fn(&SteeringObstacle) -> bool,
) -> Result<SimVec2, MovementMathError> {
    let straight = direction(input.next, input.body.pos)?;
    if straight == SimVec2::ZERO || input.speed <= SimScalar::ZERO {
        return Ok(straight);
    }
    let preferred = checked_vec_scale(straight, input.speed)?;
    let mean = mean_correction(input, (straight, preferred), &admits)?;
    if mean == SimVec2::ZERO {
        return Ok(straight);
    }
    let velocity = clamped(
        checked_vec_add(preferred, mean)?,
        straight,
        input.speed,
        input.horizon_factor,
    )?;
    Ok(SimVec2::new(
        div(velocity.x, input.speed)?,
        div(velocity.y, input.speed)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_types::PlayerId;

    fn at(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    fn unit(id: u32, pos: SimVec2) -> ContactBody {
        ContactBody {
            id: EntityId(id),
            owner: PlayerId(0),
            push_priority: 10,
            pos,
            radius: SimScalar::HALF,
        }
    }

    /// A body of `owner` at `pos` moving `velocity` per tick, under an order (not idle).
    fn body(id: u32, owner: u8, pos: SimVec2, velocity: SimVec2) -> SteeringObstacle {
        SteeringObstacle {
            owner: PlayerId(owner),
            push_priority: 10,
            idle: false,
            ..SteeringObstacle::new(EntityId(id), pos, SimScalar::HALF, velocity)
        }
    }

    fn input(body: ContactBody, next: SimVec2, neighbours: &[SteeringObstacle]) -> IntentInput<'_> {
        IntentInput {
            body,
            next,
            speed: SimScalar::from_num(0.125),
            ignored: None,
            neighbours,
            horizon_factor: 2,
        }
    }

    fn velocity(input: &IntentInput<'_>) -> Result<SimVec2, MovementMathError> {
        intent_velocity(input, |_| true)
    }

    fn length(v: SimVec2) -> f64 {
        f64::from(v.x.to_f32()).hypot(f64::from(v.y.to_f32()))
    }

    /// Rule S (Amendment 14): the velocity is continuous as a nearly head-on standing body is
    /// approached through contact. Red before: the one-partner rule dropped the body at contact
    /// and jumped by 0.165 a tick.
    #[test]
    fn the_velocity_is_continuous_through_contact() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let speed = 0.125;
        let mut last: Option<SimVec2> = None;
        let mut worst = 0.0_f64;
        for k in 0..81 {
            let x = 10.0 + 1.05 + 0.04 - f64::from(k) * 0.001;
            let seen = [body(2, 1, at(x, 10.1), SimVec2::ZERO)];
            let v = velocity(&input(a, at(20.0, 10.0), &seen))?;
            if let Some(p) = last {
                worst = worst.max(length(checked_vec_sub(v, p)?) * speed);
            }
            last = Some(v);
        }
        assert!(worst < 0.01, "jump of {worst} a tick across contact");
        Ok(())
    }

    /// I-3b: two movers exactly head-on both pass on their right, and the reflected scene decides
    /// the same way. (Held before too, by the one-partner rule.)
    #[test]
    fn a_head_on_pair_passes_right_to_right_and_mirrors() -> Result<(), MovementMathError> {
        let speed = SimScalar::from_num(0.125);
        let east = SimVec2::new(speed, SimScalar::ZERO);
        let west = SimVec2::new(-speed, SimScalar::ZERO);
        let a = unit(1, at(10.0, 10.0));
        let b = unit(2, at(11.5, 10.0));
        let va = velocity(&input(a, at(20.0, 10.0), &[body(2, 0, b.pos, west)]))?;
        let vb = velocity(&input(b, at(0.0, 10.0), &[body(1, 0, a.pos, east)]))?;
        // A heads east and turns right (south); B heads west and turns right (north).
        assert!(
            va.y < SimScalar::ZERO && vb.y > SimScalar::ZERO,
            "{va:?} {vb:?}"
        );
        let mirror = |p: SimVec2| SimVec2::new(at(21.5, 20.0).x - p.x, at(21.5, 20.0).y - p.y);
        let reflected = velocity(&input(
            unit(1, mirror(a.pos)),
            mirror(at(20.0, 10.0)),
            &[body(2, 0, mirror(b.pos), east)],
        ))?;
        let off = |a: SimScalar, b: SimScalar| (a.raw() + b.raw()).unsigned_abs();
        assert!(
            off(reflected.x, va.x) <= 64 && off(reflected.y, va.y) <= 64,
            "{reflected:?} {va:?}"
        );
        Ok(())
    }

    /// No backward step when pressed onto an engaged body dead ahead: the approach is removed, the
    /// rest stands. (Held before too.)
    #[test]
    fn no_backward_step_onto_an_engaged_body() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let engaged = SteeringObstacle {
            anchored: true,
            ..body(2, 0, at(10.95, 10.0), SimVec2::ZERO)
        };
        let v = velocity(&input(a, at(20.0, 10.0), &[engaged]))?;
        assert!(v.x >= SimScalar::ZERO, "{v:?}");
        Ok(())
    }

    /// Relabelling invariance: swapping the ids of two bodies changes nothing. Red before: the
    /// one-partner rule broke an exact tie of two threats by id.
    #[test]
    fn relabelling_changes_nothing() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let first = [
            body(2, 1, at(11.8, 10.6), SimVec2::ZERO),
            body(3, 1, at(11.8, 9.4), SimVec2::ZERO),
        ];
        let second = [
            body(3, 1, at(11.8, 10.6), SimVec2::ZERO),
            body(2, 1, at(11.8, 9.4), SimVec2::ZERO),
        ];
        assert_eq!(
            velocity(&input(a, at(20.0, 10.0), &first))?,
            velocity(&input(a, at(20.0, 10.0), &second))?
        );
        Ok(())
    }

    /// The clamps: never backward, never above the step, never turned beyond `asin(1/h)`, however
    /// many bodies push; co-moving and yielding friends add nothing.
    #[test]
    fn the_clamps_hold_and_friends_moving_along_add_nothing() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let crowd: Vec<SteeringObstacle> = (0..6)
            .map(|i| body(2 + i, 1, at(11.0 + 0.2 * f64::from(i), 10.4), SimVec2::ZERO))
            .collect();
        let v = velocity(&input(a, at(30.0, 10.0), &crowd))?;
        assert!(v.x >= SimScalar::ZERO && length(v) <= 1.0 + 1e-6, "{v:?}");
        assert!(cross(SimVec2::X_AXIS, v)?.abs() <= SimScalar::HALF, "{v:?}");
        let co = [body(2, 0, at(11.5, 10.2), at(0.1, 0.0))];
        assert_eq!(velocity(&input(a, at(20.0, 10.0), &co))?, SimVec2::X_AXIS);
        Ok(())
    }
}
