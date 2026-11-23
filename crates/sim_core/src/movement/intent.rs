//! Intent steering of plan `2026-11-20-contact-solver` (I-1 to I-4, Amendment 7 item 3): the
//! heading a unit wants before contact is resolved.
//!
//! # Responsibility
//! [`intent_heading`] turns a unit's direction to its next route point into the heading it
//! proposes this tick: its preferred velocity plus a **summed tangential time-to-collision term**
//! over every body it would collide with within its horizon (Weiss et al. 2019, §3.5: only the
//! tangential part of the predicted contact correction, so units divert sideways and are not
//! pushed back in a dense flow). The term has no memory and no partner: each candidate adds the
//! sideways velocity that would clear it by the predicted contact, weighted linearly by how soon
//! that contact comes, and the bent heading is capped at the horizon's angle.
//!
//! # How it fits
//! The solver pipeline's `step_toward` calls it instead of legacy steering and scales the heading
//! by the unit's step (I-1, with today's braking, I-2); the contact solver then separates
//! whatever contact the steps made. It reads only the tick-start neighbour snapshot, so the
//! result does not depend on the order units are processed in.
//!
//! # The term and its constants (all derived, none tuned)
//! For a candidate `b` with offset `p` (unit to body), relative velocity `v = v_pref − v_b` and
//! reach `R = r + r_b + margin`:
//! - **Time to collision** `τ`, ticks: the first root of `|p − τ v| = R`; `0` for a pair already
//!   in contact and closing (the term does not switch off at contact).
//! - **Horizon** `H = h · (r + r_b) / speed` (I-3a): a body farther than `h` radius sums of the
//!   unit's own travel is no candidate. `h` is `contact.side_horizon_factor`.
//! - **Planned stop** (Sunshine-Hill, *Game AI Pro 3* §19.7.3): a collision the unit would reach
//!   only after travelling farther than its planned stop (`stop` world units: the rest of its
//!   route, less the weapon's reach for a chase) is skipped: the unit stops short of it.
//! - **Weight** `w = 1 − τ / H`: linear, `1` at contact and `0` at the horizon, so a candidate
//!   enters and leaves the sum continuously. It is the first-order stand-in for Weiss's
//!   `exp(−τ²/τ₀)` and has no constant of its own beyond `H`.
//! - **Contribution** `w · (R − m) / max(τ, 1)` along the unit normal of `v` away from `b`, where
//!   `m = |v × p| / |v|` is the miss distance of the straight course: the sideways speed that
//!   clears `R` by the contact. The floor of one tick is the simulation's own resolution: no
//!   offset can be made faster than in the next step.
//! - **Cap** `asin(1 / h)` on the angle between the heading and the straight way (I-3a's 30° for
//!   `h = 2`): the angle a body of radius sum `r + r_b` subtends from the horizon's distance
//!   `h · (r + r_b)`, so no turn is larger than the one that clears a body met at the horizon;
//!   it also keeps a crowd of candidates from turning the unit round.
//!
//! # Invariants
//! - **No memory, no id:** the heading is a function of the snapshot; sums of Q32.32 values are
//!   exact integer additions, so it does not depend on the neighbours' order.
//! - **Exact head-on (I-3b):** a candidate dead ahead in relative motion (`v × p = 0`) is passed on
//!   the right; both bodies of a head-on pair do so, which separates them without any id.
//! - **Antisymmetry:** off the tie the side `sign(v × p)` is the same for both bodies of a pair
//!   (swapping them negates `p` and `v`), so they pass on consistent sides.
//! - Friends moving with the unit (I-4), idle friends that step aside (L6) and the chase target are
//!   never candidates.
//! - The heading is a unit vector, or zero when the unit stands on its point.

use super::math::{checked_dot, checked_vec_add, checked_vec_scale, checked_vec_sub, direction};
use super::tuning::{COMOVING_MIN_COSINE, STEERING_CORRIDOR_MARGIN};
use super::MovementMathError;
use crate::steering::{ContactBody, SteeringObstacle};
use sim_types::{EntityId, SimScalar, SimVec2};

/// Everything [`intent_heading`] reads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IntentInput<'a> {
    /// The unit.
    pub body: ContactBody,
    /// Its next route point (or gap point), world units.
    pub next: SimVec2,
    /// Its step this tick, world units per tick (accelerated and braked).
    pub speed: SimScalar,
    /// The travel left before the unit's planned stop, world units.
    pub stop: SimScalar,
    /// A body never steered round: the chase target.
    pub ignored: Option<EntityId>,
    /// The tick-start neighbours.
    pub neighbours: &'a [SteeringObstacle],
    /// `contact.side_horizon_factor` (I-3a), at least 1.
    pub horizon_factor: u8,
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

/// The unit normal of `along` on its right (`left == false`) or its left.
fn normal(along: SimVec2, left: bool) -> SimVec2 {
    if left {
        SimVec2::new(-along.y, along.x)
    } else {
        SimVec2::new(along.y, -along.x)
    }
}

/// Whether `obstacle` is a friend moving with the unit, whose heading is `heading` (I-4): its
/// last displacement points within 60° of the heading.
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

/// The predicted contact with one candidate, before its weight.
struct Contact {
    /// Ticks to collision; 0 when already in contact and closing.
    time: SimScalar,
    /// The horizon `H`, ticks.
    horizon: SimScalar,
    /// Offset from the unit to the body, world units.
    offset: SimVec2,
    /// The unit's preferred velocity less the body's, world units per tick.
    relative: SimVec2,
    /// `|relative|²`.
    speed_sq: SimScalar,
    /// Radius sum plus the clearance margin, world units.
    reach: SimScalar,
}

/// The predicted contact with `obstacle` within the horizon and before the planned stop, if any.
fn contact_with(
    input: &IntentInput<'_>,
    preferred: SimVec2,
    obstacle: &SteeringObstacle,
) -> Result<Option<Contact>, MovementMathError> {
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
    // I-3a: H = h · (r + r_b) / speed ticks.
    let horizon = SimScalar::from_num(input.horizon_factor)
        .checked_mul(body.radius.saturating_add(obstacle.radius))
        .and_then(|span| span.checked_div(input.speed))
        .unwrap_or(SimScalar::MAX);
    // The planned stop: the unit's own travel to the contact exceeds what it has left.
    let travel = time.saturating_mul(input.speed);
    if time > horizon || travel > input.stop {
        return Ok(None);
    }
    Ok(Some(Contact {
        time,
        horizon,
        offset,
        relative,
        speed_sq,
        reach,
    }))
}

/// The weighted sideways velocity that clears `contact` (see the module documentation).
fn tangential(contact: &Contact) -> Result<SimVec2, MovementMathError> {
    let speed = root(contact.speed_sq)?;
    let along = SimVec2::new(
        div(contact.relative.x, speed)?,
        div(contact.relative.y, speed)?,
    );
    let side = cross(contact.relative, contact.offset)?;
    let miss = div(side.abs(), speed)?;
    let need = sub(contact.reach, miss)?;
    if need <= SimScalar::ZERO {
        return Ok(SimVec2::ZERO);
    }
    let weight = sub(SimScalar::ONE, div(contact.time, contact.horizon)?)?
        .clamp(SimScalar::ZERO, SimScalar::ONE);
    let magnitude = mul(weight, div(need, contact.time.max(SimScalar::ONE))?)?;
    // A body on the left of the relative motion is passed on the right, one on the right on the
    // left; dead ahead (`side = 0`) on the right (I-3b).
    checked_vec_scale(normal(along, side < SimScalar::ZERO), magnitude)
}

/// The summed tangential term over the neighbours. A neighbour counts when it is not passed over
/// ([`passed_over`]), is not a co-moving friend (I-4), and `admits` it (the neighbour test
/// deferred for far bodies, see `locomotion::gather_neighbours`).
// PERFORMANCE RATIONALE:
// Why: Under the solver the neighbour list is about five times longer than the legacy one, and
// the co-moving test normalises each friend's velocity (a root and two divisions), most of it
// for bodies that are not on a collision course at all (evidence §5.3).
// What: The cheap geometry of [`contact_with`] runs first; the co-moving test and the deferred
// ground test run only for a body on a collision course. Both are exclusions that never fail
// (a unit normal of a finite velocity is always representable), so the sum is the same as
// testing in the order of the rules.
// Trade-off: The rule's order in code differs from its order in the plan's text.
fn summed_term(
    input: &IntentInput<'_>,
    (straight, preferred): (SimVec2, SimVec2),
    admits: &impl Fn(&SteeringObstacle) -> bool,
) -> Result<SimVec2, MovementMathError> {
    let mut sum = SimVec2::ZERO;
    for obstacle in input.neighbours {
        if passed_over(input, obstacle) {
            continue;
        }
        let Some(contact) = contact_with(input, preferred, obstacle)? else {
            continue;
        };
        if (obstacle.owner == input.body.owner && co_moving(straight, obstacle)?)
            || !admits(obstacle)
        {
            continue;
        }
        sum = checked_vec_add(sum, tangential(&contact)?)?;
    }
    Ok(sum)
}

/// `heading` turned back toward `straight` so that the angle between them is at most
/// `asin(1 / h)`, on the side it lies (a heading exactly opposite turns right).
fn capped(straight: SimVec2, heading: SimVec2, factor: u8) -> Result<SimVec2, MovementMathError> {
    let sine = div(SimScalar::ONE, SimScalar::from_num(factor.max(1)))?;
    let cosine = root(sub(SimScalar::ONE, mul(sine, sine)?)?)?;
    if checked_dot(straight, heading)? >= cosine {
        return Ok(heading);
    }
    let left = cross(straight, heading)? > SimScalar::ZERO;
    checked_vec_add(
        checked_vec_scale(straight, cosine)?,
        checked_vec_scale(normal(straight, left), sine)?,
    )
}

/// The heading the unit proposes this tick (see the module documentation). `admits` completes
/// the neighbour test of a listed body (`true` for every body when the list is exact).
///
/// # Errors
/// An unrepresentable intermediate of the term's geometry.
pub(crate) fn intent_heading(
    input: &IntentInput<'_>,
    admits: impl Fn(&SteeringObstacle) -> bool,
) -> Result<SimVec2, MovementMathError> {
    let straight = direction(input.next, input.body.pos)?;
    if straight == SimVec2::ZERO || input.speed <= SimScalar::ZERO {
        return Ok(straight);
    }
    let preferred = checked_vec_scale(straight, input.speed)?;
    let term = summed_term(input, (straight, preferred), &admits)?;
    if term == SimVec2::ZERO {
        return Ok(straight);
    }
    let bent = direction(checked_vec_add(preferred, term)?, SimVec2::ZERO)?;
    if bent == SimVec2::ZERO {
        // The term exactly cancels the preferred velocity: pass on the right (I-3b).
        return capped(straight, normal(straight, false), input.horizon_factor);
    }
    capped(straight, bent, input.horizon_factor)
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
            stop: SimScalar::MAX,
            ignored: None,
            neighbours,
            horizon_factor: 2,
        }
    }

    fn heading(input: &IntentInput<'_>) -> Result<SimVec2, MovementMathError> {
        intent_heading(input, |_| true)
    }

    /// A body the deferred neighbour test refuses adds nothing.
    #[test]
    fn a_body_the_neighbour_test_refuses_is_no_candidate() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let seen = [body(2, 1, at(11.5, 10.1), at(-0.125, 0.0))];
        let input = input(a, at(20.0, 10.0), &seen);
        assert_ne!(heading(&input)?, SimVec2::X_AXIS);
        assert_eq!(intent_heading(&input, |_| false)?, SimVec2::X_AXIS);
        Ok(())
    }

    /// I-3b: two bodies exactly head-on both pass on their right, and the reflected scene decides
    /// the same way (mirror consistency).
    #[test]
    fn a_head_on_pair_passes_right_to_right_and_mirrors() -> Result<(), MovementMathError> {
        let speed = SimScalar::from_num(0.125);
        let east = SimVec2::new(speed, SimScalar::ZERO);
        let west = SimVec2::new(-speed, SimScalar::ZERO);
        let a = unit(1, at(10.0, 10.0));
        let b = unit(2, at(12.5, 10.0));
        let ha = heading(&input(a, at(20.0, 10.0), &[body(2, 0, b.pos, west)]))?;
        let hb = heading(&input(b, at(0.0, 10.0), &[body(1, 0, a.pos, east)]))?;
        // A heads east and turns right (south); B heads west and turns right (north).
        assert!(
            ha.y < SimScalar::ZERO && hb.y > SimScalar::ZERO,
            "{ha:?} {hb:?}"
        );
        let mirror = |p: SimVec2| SimVec2::new(at(22.5, 20.0).x - p.x, at(22.5, 20.0).y - p.y);
        let reflected = heading(&input(
            unit(1, mirror(a.pos)),
            mirror(at(20.0, 10.0)),
            &[body(2, 0, mirror(b.pos), east)],
        ))?;
        // The reflected heading is A's negated, up to the last bits of Q32.32 products.
        let off = |a: SimScalar, b: SimScalar| (a.raw() + b.raw()).unsigned_abs();
        assert!(off(reflected.x, ha.x) <= 64, "{reflected:?} {ha:?}");
        assert!(off(reflected.y, ha.y) <= 64, "{reflected:?} {ha:?}");
        Ok(())
    }

    /// Two equally urgent bodies placed symmetrically about the way cancel: the unit goes
    /// straight (contact queues it). One of them alone bends the heading away from it.
    #[test]
    fn symmetric_threats_cancel_and_one_alone_bends() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let left = body(2, 0, at(11.8, 10.6), SimVec2::ZERO);
        let right = body(3, 0, at(11.8, 9.4), SimVec2::ZERO);
        // Up to the last bits of Q32.32 products, which round toward negative infinity.
        let both = heading(&input(a, at(20.0, 10.0), &[left, right]))?;
        assert!(both.y.raw().unsigned_abs() <= 64, "{both:?}");
        let alone = heading(&input(a, at(20.0, 10.0), &[left]))?;
        assert!(alone.y < SimScalar::ZERO, "{alone:?}");
        Ok(())
    }

    /// The term is a sum: the neighbours' order changes nothing, to the last bit.
    #[test]
    fn the_heading_does_not_depend_on_the_neighbours_order() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let bodies = [
            body(2, 1, at(11.9, 10.3), at(-0.1, 0.0)),
            body(3, 0, at(12.4, 9.2), SimVec2::ZERO),
            body(4, 1, at(11.0, 11.4), at(0.0, -0.1)),
        ];
        let reversed = [bodies[2], bodies[0], bodies[1]];
        assert_eq!(
            heading(&input(a, at(20.0, 10.0), &bodies))?,
            heading(&input(a, at(20.0, 10.0), &reversed))?
        );
        Ok(())
    }

    /// I-4 and L6: a friend moving the same way and an idle friend that steps aside are no
    /// candidates; an enemy in the same place is.
    #[test]
    fn co_moving_and_yielding_friends_are_no_candidates() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let ahead = at(11.5, 10.2);
        let co = [body(2, 0, ahead, at(0.1, 0.0))];
        let idle = [SteeringObstacle {
            owner: PlayerId(0),
            push_priority: 10,
            ..SteeringObstacle::new(EntityId(3), ahead, SimScalar::HALF, SimVec2::ZERO)
        }];
        let enemy = [body(4, 1, ahead, SimVec2::ZERO)];
        assert_eq!(heading(&input(a, at(20.0, 10.0), &co))?, SimVec2::X_AXIS);
        assert_eq!(heading(&input(a, at(20.0, 10.0), &idle))?, SimVec2::X_AXIS);
        assert_ne!(heading(&input(a, at(20.0, 10.0), &enemy))?, SimVec2::X_AXIS);
        Ok(())
    }

    /// The planned stop: a standing body the unit would reach only after its stop is ignored.
    #[test]
    fn a_collision_beyond_the_planned_stop_is_ignored() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let seen = [body(2, 0, at(12.0, 10.1), SimVec2::ZERO)];
        let far = input(a, at(20.0, 10.0), &seen);
        assert_ne!(heading(&far)?, SimVec2::X_AXIS);
        // The contact lies about 0.95 ahead; a unit stopping after 0.5 never reaches it.
        let short = IntentInput {
            stop: SimScalar::HALF,
            ..far
        };
        assert_eq!(heading(&short)?, SimVec2::X_AXIS);
        Ok(())
    }

    /// I-3a: the turn never exceeds `asin(1 / h)`, however many bodies push the same way, and a
    /// body beyond the horizon adds nothing. A body already in contact and closing still counts.
    #[test]
    fn the_deflection_never_exceeds_the_horizon_bound() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let crowd: Vec<SteeringObstacle> = (0..6)
            .map(|i| body(2 + i, 1, at(11.0 + 0.2 * f64::from(i), 10.4), SimVec2::ZERO))
            .collect();
        let bent = heading(&input(a, at(30.0, 10.0), &crowd))?;
        // sin of the turn = |cross(x̂, heading)|; asin(1/2) is 30°, sin 0.5.
        assert!(
            cross(SimVec2::X_AXIS, bent)?.abs() <= SimScalar::HALF,
            "{bent:?}"
        );
        assert!(bent.y < SimScalar::ZERO, "{bent:?}");
        // h = 2, r + r_b = 1, speed 0.125: H = 16 ticks, 2 world units of travel; R = 1.05.
        let beyond = [body(2, 1, at(10.0 + 1.05 + 2.2, 10.0), SimVec2::ZERO)];
        assert_eq!(
            heading(&input(a, at(30.0, 10.0), &beyond))?,
            SimVec2::X_AXIS
        );
        let touching = [body(2, 1, at(10.9, 10.2), SimVec2::ZERO)];
        assert!(heading(&input(a, at(30.0, 10.0), &touching))?.y < SimScalar::ZERO);
        Ok(())
    }
}
