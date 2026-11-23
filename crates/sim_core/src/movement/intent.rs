//! Intent steering of plan `2026-11-20-contact-solver` (I-1 to I-4): the heading a unit wants
//! before contact is resolved.
//!
//! # Responsibility
//! [`intent_heading`] turns a unit's direction to its next route point into the heading it
//! proposes this tick: straight at the point, bent by at most one **time-to-collision side rule**
//! against the earliest oncoming or crossing body that does not give way (I-3). The side chosen
//! for a body is kept, as a one-pair commitment in [`SideMemory`], until the pair stops closing
//! (I-3c). Friends moving with the unit are not threats (I-4), nor are idle friends that step
//! aside (L6), nor bodies already in contact (the contact solver's job).
//!
//! # How it fits
//! The solver pipeline's `step_toward` calls it instead of legacy steering and scales the heading
//! by the unit's step (I-1, with today's braking, I-2); the contact solver then separates
//! whatever contact the steps made. It reads only the tick-start neighbour snapshot, so the
//! result does not depend on the order units are processed in.
//!
//! # Invariants
//! - **Exact head-on tie (I-3b):** a threat dead ahead in relative motion (`cross = 0`) is passed
//!   on the right; both bodies of a head-on pair do so, which separates them without any id.
//! - **Antisymmetry:** off the tie the geometric side `cross(v_rel, p) < 0` is the same for both
//!   bodies of a pair (swapping the pair negates `p` and `v_rel`, which keeps the cross product).
//! - The heading is a unit vector, or zero when the unit stands on its point.
//! - All arithmetic is checked Q32.32 with one square root per threat for its time and one for the
//!   turn; no trigonometric table.

use super::math::{checked_dot, checked_vec_add, checked_vec_scale, checked_vec_sub, direction};
use super::tuning::{COMOVING_MIN_COSINE, STANDING_DRIFT, STEERING_CORRIDOR_MARGIN};
use super::MovementMathError;
use crate::steering::{ContactBody, SteeringObstacle};
use sim_types::{EntityId, SimScalar, SimVec2};

/// The side rule's one-pair commitment (I-3): the body passed and the side it is passed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct SideMemory {
    /// The committed partner, if any.
    pub partner: Option<EntityId>,
    /// Passes the partner on the unit's left when true.
    pub left: bool,
}

/// What intent decided this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Intent {
    /// The unit heading to propose, or zero at the point.
    pub heading: SimVec2,
    /// The commitment after this tick.
    pub side: SideMemory,
    /// The body steered round this tick, if any.
    pub threat: Option<EntityId>,
}

/// Everything [`intent_heading`] reads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IntentInput<'a> {
    /// The unit.
    pub body: ContactBody,
    /// Its next route point (or slot point), world units.
    pub next: SimVec2,
    /// Its step this tick, world units per tick (accelerated and braked).
    pub speed: SimScalar,
    /// A body never steered round: the chase target.
    pub ignored: Option<EntityId>,
    /// The tick-start neighbours.
    pub neighbours: &'a [SteeringObstacle],
    /// `contact.side_horizon_factor` (I-3a).
    pub horizon_factor: u8,
    /// The commitment the unit carries into this tick.
    pub side: SideMemory,
}

/// One body that will collide with the unit within its horizon.
#[derive(Debug, Clone, Copy)]
struct Threat {
    id: EntityId,
    /// Ticks to collision.
    time: SimScalar,
    /// Offset from the unit to the body, world units.
    offset: SimVec2,
    /// The unit's preferred velocity less the body's, world units per tick.
    relative: SimVec2,
    /// The body's last displacement, world units per tick.
    velocity: SimVec2,
    /// Radius sum plus the clearance margin, world units.
    reach: SimScalar,
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

fn sub(a: SimScalar, b: SimScalar) -> Result<SimScalar, MovementMathError> {
    a.checked_sub(b).ok_or(MovementMathError::Unrepresentable)
}

/// Whether `obstacle`, a friend, is passed like one moving with the unit, whose heading is
/// `heading` (I-4): its last displacement points within 60° of the heading, or it stands
/// (moved at most `STANDING_DRIFT`) without being anchored, waiting rather than holding or firing
/// (plan Amendment 5, 1: no committed side against a waiting friend, which contact handles).
fn co_moving(heading: SimVec2, obstacle: &SteeringObstacle) -> Result<bool, MovementMathError> {
    let drift = i128::from(STANDING_DRIFT.raw());
    if super::math::within_distance(obstacle.velocity, SimVec2::ZERO, drift) {
        return Ok(!obstacle.anchored);
    }
    let along = direction(obstacle.velocity, SimVec2::ZERO)?;
    Ok(checked_dot(heading, along)? >= COMOVING_MIN_COSINE)
}

/// The time to collision with `obstacle` within the horizon, if it is a threat (I-3).
fn threat_of(
    input: &IntentInput<'_>,
    preferred: SimVec2,
    obstacle: &SteeringObstacle,
) -> Result<Option<Threat>, MovementMathError> {
    let body = input.body;
    let offset = checked_vec_sub(obstacle.pos, body.pos)?;
    let relative = checked_vec_sub(preferred, obstacle.velocity)?;
    let reach = body
        .radius
        .checked_add(obstacle.radius)
        .and_then(|sum| sum.checked_add(STEERING_CORRIDOR_MARGIN))
        .ok_or(MovementMathError::Unrepresentable)?;
    let gap_sq = sub(checked_dot(offset, offset)?, mul(reach, reach)?)?;
    let closing = checked_dot(offset, relative)?;
    if gap_sq <= SimScalar::ZERO || closing <= SimScalar::ZERO {
        // Already in contact (the solver's job), or not closing.
        return Ok(None);
    }
    let speed_sq = checked_dot(relative, relative)?;
    let discriminant = sub(mul(closing, closing)?, mul(speed_sq, gap_sq)?)?;
    if speed_sq <= SimScalar::ZERO || discriminant < SimScalar::ZERO {
        return Ok(None);
    }
    let root = discriminant
        .sqrt()
        .map_err(|_| MovementMathError::Unrepresentable)?;
    let time = sub(closing, root)?
        .checked_div(speed_sq)
        .ok_or(MovementMathError::Unrepresentable)?;
    // I-3a: H = h · (r + r_b) / speed ticks.
    let horizon = SimScalar::from_num(input.horizon_factor)
        .checked_mul(body.radius.saturating_add(obstacle.radius))
        .and_then(|span| span.checked_div(input.speed))
        .unwrap_or(SimScalar::MAX);
    if time > horizon {
        return Ok(None);
    }
    Ok(Some(Threat {
        id: obstacle.id,
        time,
        offset,
        relative,
        velocity: obstacle.velocity,
        reach,
    }))
}

/// Whether `obstacle` is excluded before any geometry: the unit itself, the ignored body (the
/// chase target), or an idle friend that steps aside (L6).
fn passed_over(input: &IntentInput<'_>, obstacle: &SteeringObstacle) -> bool {
    let body = input.body;
    obstacle.id == body.id
        || Some(obstacle.id) == input.ignored
        || obstacle.yields_to(body.owner, body.push_priority)
}

/// The earliest threat among the neighbours (I-3) and whether the committed partner still
/// closes. A neighbour counts when it is not passed over ([`passed_over`]), is not a co-moving
/// friend (I-4), and `admits` it (the neighbour test deferred for far bodies, see
/// `locomotion::gather_neighbours`).
// PERFORMANCE RATIONALE:
// Why: Under the solver the neighbour list is about five times longer than the legacy one, and
// the co-moving test normalises each friend's velocity (a root and two divisions), most of it
// for bodies that are not on a collision course at all (evidence §5.3).
// What: The cheap geometry of [`threat_of`] runs first; the co-moving test and the deferred
// ground test run only for a body that is a threat or the committed partner. Both tests are
// exclusions that never fail (a unit normal of a finite velocity is always representable), so
// the threat and the partner found are the same as testing in the order of the rules.
// Trade-off: The rule's order in code differs from its order in the plan's text.
fn scan_threats(
    input: &IntentInput<'_>,
    (straight, preferred): (SimVec2, SimVec2),
    admits: &impl Fn(&SteeringObstacle) -> bool,
) -> Result<(Option<Threat>, bool), MovementMathError> {
    let mut earliest: Option<Threat> = None;
    let mut partner_closes = false;
    for obstacle in input.neighbours {
        if passed_over(input, obstacle) {
            continue;
        }
        let partner = Some(obstacle.id) == input.side.partner;
        let threat = threat_of(input, preferred, obstacle);
        if !partner && matches!(threat, Ok(None)) {
            continue;
        }
        if (obstacle.owner == input.body.owner && co_moving(straight, obstacle)?)
            || !admits(obstacle)
        {
            continue;
        }
        if partner {
            let offset = checked_vec_sub(obstacle.pos, input.body.pos)?;
            let relative = checked_vec_sub(preferred, obstacle.velocity)?;
            partner_closes = checked_dot(offset, relative)? > SimScalar::ZERO;
        }
        if let Some(threat) = threat? {
            let earlier = earliest.is_none_or(|best| {
                threat.time < best.time || (threat.time == best.time && threat.id < best.id)
            });
            if earlier {
                earliest = Some(threat);
            }
        }
    }
    Ok((earliest, partner_closes))
}

/// The heading the unit proposes this tick and its commitment (see the module documentation).
/// `admits` completes the neighbour test of a listed body (`true` for every body when the list
/// is exact).
///
/// # Errors
/// An unrepresentable intermediate of the side rule's geometry.
pub(crate) fn intent_heading(
    input: &IntentInput<'_>,
    admits: impl Fn(&SteeringObstacle) -> bool,
) -> Result<Intent, MovementMathError> {
    let straight = direction(input.next, input.body.pos)?;
    let mut decision = Intent {
        heading: straight,
        side: input.side,
        threat: None,
    };
    if straight == SimVec2::ZERO || input.speed <= SimScalar::ZERO {
        return Ok(decision);
    }
    let preferred = checked_vec_scale(straight, input.speed)?;
    let (earliest, partner_closes) = scan_threats(input, (straight, preferred), &admits)?;
    let Some(threat) = earliest else {
        // Commitment ends when the partner no longer closes or has left the neighbours (I-3).
        if !partner_closes {
            decision.side = SideMemory::default();
        }
        return Ok(decision);
    };
    let left = if Some(threat.id) == input.side.partner {
        input.side.left
    } else {
        // The threat to the right of the relative motion is passed on the left; dead ahead
        // (`cross = 0`) on the right (I-3b).
        cross(threat.relative, threat.offset)? < SimScalar::ZERO
    };
    decision.side = SideMemory {
        partner: Some(threat.id),
        left,
    };
    decision.threat = Some(threat.id);
    decision.heading = tangent_heading(&threat, left)?.unwrap_or(straight);
    Ok(decision)
}

/// The heading that turns the threat's relative velocity onto the tangent of its disc on the
/// chosen side: `t̂ = p̂ cos α ± p̂⊥ sin α` with `sin α = R / |p|`, heading
/// `normalize(v_b + |v_rel| t̂)`. `None` when that velocity is zero.
fn tangent_heading(threat: &Threat, left: bool) -> Result<Option<SimVec2>, MovementMathError> {
    let distance = super::checked_distance(threat.offset, SimVec2::ZERO)
        .ok_or(MovementMathError::Unrepresentable)?;
    let toward = direction(threat.offset, SimVec2::ZERO)?;
    let sine = threat
        .reach
        .checked_div(distance)
        .ok_or(MovementMathError::Unrepresentable)?
        .min(SimScalar::ONE);
    let cosine = sub(SimScalar::ONE, mul(sine, sine)?)?
        .max(SimScalar::ZERO)
        .sqrt()
        .map_err(|_| MovementMathError::Unrepresentable)?;
    let normal = if left {
        SimVec2::new(-toward.y, toward.x)
    } else {
        SimVec2::new(toward.y, -toward.x)
    };
    let tangent = checked_vec_add(
        checked_vec_scale(toward, cosine)?,
        checked_vec_scale(normal, sine)?,
    )?;
    let speed = super::checked_distance(threat.relative, SimVec2::ZERO)
        .ok_or(MovementMathError::Unrepresentable)?;
    let velocity = checked_vec_add(threat.velocity, checked_vec_scale(tangent, speed)?)?;
    let heading = direction(velocity, SimVec2::ZERO)?;
    Ok((heading != SimVec2::ZERO).then_some(heading))
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

    fn input(
        body: ContactBody,
        next: SimVec2,
        neighbours: &[SteeringObstacle],
        side: SideMemory,
    ) -> IntentInput<'_> {
        IntentInput {
            body,
            next,
            speed: SimScalar::from_num(0.125),
            ignored: None,
            neighbours,
            horizon_factor: 2,
            side,
        }
    }

    /// The intent over a list whose every body is a neighbour.
    fn intent_heading_all(input: &IntentInput<'_>) -> Result<Intent, MovementMathError> {
        intent_heading(input, |_| true)
    }

    /// A body the deferred neighbour test refuses is no threat, and a committed partner it
    /// refuses has left the neighbours, so the commitment ends (plan Amendment 3, item 1).
    #[test]
    fn a_body_the_neighbour_test_refuses_is_neither_threat_nor_partner(
    ) -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let oncoming = body(2, 1, at(11.5, 10.0), at(-0.125, 0.0));
        let seen = [oncoming];
        let input = input(a, at(20.0, 10.0), &seen, SideMemory::default());
        assert_eq!(intent_heading_all(&input)?.threat, Some(EntityId(2)));
        let refused = intent_heading(&input, |_| false)?;
        assert_eq!(refused.threat, None);
        assert_eq!(
            refused.heading,
            SimVec2::new(SimScalar::ONE, SimScalar::ZERO)
        );
        let committed = SideMemory {
            partner: Some(EntityId(2)),
            left: true,
        };
        let input = IntentInput {
            side: committed,
            ..input
        };
        assert_eq!(
            intent_heading(&input, |_| false)?.side,
            SideMemory::default()
        );
        Ok(())
    }

    fn cross_of(heading: SimVec2, axis: SimVec2) -> SimScalar {
        cross(axis, heading).unwrap_or(SimScalar::ZERO)
    }

    /// I-3b: two bodies exactly head-on both pass on their right, which separates them, and the
    /// reflected scene decides the same way (mirror consistency).
    #[test]
    fn a_head_on_pair_passes_right_to_right_and_mirrors() -> Result<(), MovementMathError> {
        let speed = SimScalar::from_num(0.125);
        let east = SimVec2::new(speed, SimScalar::ZERO);
        let west = SimVec2::new(-speed, SimScalar::ZERO);
        let a = unit(1, at(10.0, 10.0));
        let b = unit(2, at(12.5, 10.0));
        let seen_by_a = [body(2, 0, b.pos, west)];
        let seen_by_b = [body(1, 0, a.pos, east)];
        let ia = intent_heading_all(&input(a, at(20.0, 10.0), &seen_by_a, SideMemory::default()))?;
        let ib = intent_heading_all(&input(b, at(0.0, 10.0), &seen_by_b, SideMemory::default()))?;
        assert_eq!(ia.threat, Some(EntityId(2)));
        assert_eq!(ib.threat, Some(EntityId(1)));
        assert!(!ia.side.left && !ib.side.left, "{ia:?} {ib:?}");
        // A heads east and turns right (south); B heads west and turns right (north).
        assert!(
            ia.heading.y < SimScalar::ZERO && ib.heading.y > SimScalar::ZERO,
            "{ia:?} {ib:?}"
        );
        // Point reflection through (11.25, 10): the decisions reflect.
        let mirror = |p: SimVec2| SimVec2::new(at(22.5, 20.0).x - p.x, at(22.5, 20.0).y - p.y);
        let ra = unit(1, mirror(a.pos));
        let seen = [body(2, 0, mirror(b.pos), east)];
        let reflected = intent_heading_all(&input(
            ra,
            mirror(at(20.0, 10.0)),
            &seen,
            SideMemory::default(),
        ))?;
        assert!(!reflected.side.left);
        // The reflected heading is A's negated, up to the last bits of Q32.32 products, which
        // round towards negative infinity and so are not odd functions.
        let off = |a: SimScalar, b: SimScalar| (a.raw() + b.raw()).unsigned_abs();
        assert!(
            off(reflected.heading.x, ia.heading.x) <= 8,
            "{reflected:?} {ia:?}"
        );
        assert!(
            off(reflected.heading.y, ia.heading.y) <= 8,
            "{reflected:?} {ia:?}"
        );
        Ok(())
    }

    /// I-3: a crossing at 90° commits a side and keeps it while the pair closes, even when the
    /// geometry alone would now choose the other side.
    #[test]
    fn a_crossing_commits_a_side_and_keeps_it_while_closing() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        // B comes up from the south-east, crossing A's way east.
        let north = SimVec2::new(SimScalar::ZERO, SimScalar::from_num(0.125));
        let seen = [body(2, 0, at(11.5, 8.6), north)];
        let first = intent_heading_all(&input(a, at(20.0, 10.0), &seen, SideMemory::default()))?;
        let committed = first.side;
        assert_eq!(committed.partner, Some(EntityId(2)));
        // Same pair, the commitment flipped by hand: it is kept, not re-chosen.
        let flipped = SideMemory {
            partner: committed.partner,
            left: !committed.left,
        };
        let kept = intent_heading_all(&input(a, at(20.0, 10.0), &seen, flipped))?;
        assert_eq!(kept.side, flipped);
        // Once B has passed and no longer closes, the commitment ends.
        let gone = [body(2, 0, at(10.0, 12.0), north)];
        let ended = intent_heading_all(&input(a, at(20.0, 10.0), &gone, committed))?;
        assert_eq!(ended.side, SideMemory::default());
        assert_eq!(ended.heading, SimVec2::X_AXIS);
        Ok(())
    }

    /// I-4 and L6: a friend moving the same way and an idle friend that steps aside are not
    /// threats; an enemy in the same place is.
    #[test]
    fn co_moving_and_yielding_friends_are_not_threats() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        let ahead = at(11.5, 10.0);
        let same_way = SimVec2::new(SimScalar::from_num(0.1), SimScalar::ZERO);
        let co = [body(2, 0, ahead, same_way)];
        let idle = [SteeringObstacle {
            owner: PlayerId(0),
            push_priority: 10,
            ..SteeringObstacle::new(EntityId(3), ahead, SimScalar::HALF, SimVec2::ZERO)
        }];
        let enemy = [body(4, 1, ahead, SimVec2::ZERO)];
        for (neighbours, threat) in [
            (&co[..], None),
            (&idle[..], None),
            (&enemy[..], Some(EntityId(4))),
        ] {
            let decided =
                intent_heading_all(&input(a, at(20.0, 10.0), neighbours, SideMemory::default()))?;
            assert_eq!(decided.threat, threat);
        }
        Ok(())
    }

    /// I-3a: against a standing body dead ahead at the horizon, the turn is at most
    /// `asin(1 / h)`; beyond the horizon there is none.
    #[test]
    fn the_deflection_never_exceeds_the_horizon_bound() -> Result<(), MovementMathError> {
        let a = unit(1, at(10.0, 10.0));
        // h = 2, r + r_b = 1, speed 0.125: H = 16 ticks; reach R = 1.05; |p| = R + 16 · 0.125.
        for (gap, steered) in [(1.9, true), (1.5, true), (2.1, false)] {
            let seen = [body(2, 1, at(10.0 + 1.05 + gap, 10.0), SimVec2::ZERO)];
            let decided =
                intent_heading_all(&input(a, at(30.0, 10.0), &seen, SideMemory::default()))?;
            assert_eq!(decided.threat.is_some(), steered, "gap {gap}");
            // sin of the turn = |cross(x̂, heading)|; asin(1/2) is 30°, sin 0.5.
            let sine = cross_of(decided.heading, SimVec2::X_AXIS).abs();
            assert!(sine <= SimScalar::HALF, "gap {gap}: sin {sine}");
        }
        Ok(())
    }
}
