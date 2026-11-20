//! The contact solver of plan `2026-11-20-contact-solver` (C-1 to C-8): one deterministic,
//! position-based pass that separates bodies after every unit has moved.
//!
//! # Responsibility
//! [`contact_shares`] answers, for two bodies in contact, which of them gives way and by how much
//! (C-2, the push table restated as inverse-mass weights). [`ContactSolver`] holds one tick's
//! bodies and the pairs that can touch, and [`ContactSolver::solve`] runs the plan's Jacobi
//! iterations over them: each iteration computes every pair's correction from the positions at
//! its start, averages each body's corrections over the pairs that move it, and passes the
//! averaged target through a terrain step (C-6); an iteration in which nothing moved ends the
//! loop. A final pass over the hard pairs (enemies, and pairs in which one body may not move)
//! sums its corrections without averaging (C-1b).
//!
//! # How it fits
//! The movement phase fills the solver from the bodies' tick-start and committed positions and
//! gives it the terrain step (`slide_target`, `permits_segment` from the tick start and
//! `settle_on_terrain`, as the legacy collision phase applies a correction). Nothing here reads
//! the world, so every rule is tested on plain values.
//!
//! # Invariants
//! - **Order independence (C-1a).** A pair is stored with its lower-id body first, so its
//!   correction does not depend on where either body sits in the list; corrections are summed in
//!   Q32.32, which is exact integer addition; the division by a body's contact count and the
//!   terrain step read that body alone; the early stop is a global test. The result is a
//!   function of the set of bodies.
//! - A body with a share of zero in a pair is never moved by it; a fixed body is never moved.
//! - An overlap of at most the dead band (`dead_band_permille` of the radius sum) is never
//!   corrected (C-4).
//! - Checked arithmetic: a pair whose correction cannot be represented is reported as a typed
//!   [`MovementInvariant`] and contributes nothing.
#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "plan 2026-11-20-contact-solver WP2 builds this part as tested library code; the \
                  solver pipeline calls it from WP3 on"
    )
)]

use super::contact::settle_on_terrain;
use super::grid::BodyGrid;
use super::locomotion::slide_target;
use super::math::{checked_vec_add, checked_vec_scale, checked_vec_sub, direction};
use super::{MotionPolicy, MovementInvariant, MovementMathError};
use sim_map::{Pathfinder, SimMap};
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2};

/// What contact needs to know about one body to decide who gives way (C-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts about one body, read together by the share table"
)]
pub struct ContactParty {
    /// Owning player: bodies of one owner are friends.
    pub owner: PlayerId,
    /// Authored priority for friendly contact.
    pub push_priority: u8,
    /// Nothing at all can move this body: it is immobile or unpushable.
    pub fixed: bool,
    /// Hold Position, immobile or unpushable (`Entity::cannot_be_pushed`).
    pub holding: bool,
    /// Stands fighting (anchored in combat).
    pub engaged: bool,
    /// Has nothing to do: no order, `Idle` or `Stop`.
    pub idle: bool,
    /// Its committed step this tick is longer than `STANDING_DRIFT` (1/64 world units).
    pub moving: bool,
}

impl ContactParty {
    /// Holds its ground against friends: holding or engaged (C-2's "anchored").
    #[must_use]
    pub const fn anchored(&self) -> bool {
        self.holding || self.engaged
    }

    /// An idle body that stood this tick, which a moving friend may push aside (C-2 row 6).
    #[must_use]
    pub const fn idle_standing(&self) -> bool {
        self.idle && !self.moving && !self.anchored()
    }
}

/// How a pair in contact shares its overlap (C-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairShares {
    /// The two bodies never correct each other: both fixed, or two holding bodies.
    None,
    /// The first body takes `.0` of the overlap and the second `.1`; each is 0, ½ or 1.
    Weights(SimScalar, SimScalar),
    /// Two moving friends under D-β off: each takes the share of its own approach along the line
    /// of centres this tick, a body that did not close taking none ([`approach_shares`]).
    ByApproach,
}

/// The shares of the pair `(a, b)`, first matching row wins (plan `2026-11-20-contact-solver`
/// C-2, the push table of plan `2026-11-09-friendly-blocking` D-4 as inverse-mass weights).
/// `moving_friends_push` is D-β: two moving friends push by priority when true.
///
/// | Case | a | b |
/// |---|---|---|
/// | both fixed | none | |
/// | one fixed | 0 | 1 |
/// | enemies, both holding | none | |
/// | enemies, one holding | 0 | 1 |
/// | enemies | ½ | ½ |
/// | friends, both holding | none | |
/// | friends, one anchored (holding or engaged), the other not, or holding against engaged | 0 | 1 |
/// | friends, both engaged | ½ | ½ |
/// | friends, one moving, the other idle and standing | 0 | 1 by priority, else 1 | 0 |
/// | friends, both moving, D-β on | the lower priority takes all; equal ½ ½ | |
/// | friends, both moving, D-β off | by approach | |
/// | friends, otherwise | ½ | ½ |
#[must_use]
pub fn contact_shares(a: &ContactParty, b: &ContactParty, moving_friends_push: bool) -> PairShares {
    let (zero, half, one) = (SimScalar::ZERO, SimScalar::HALF, SimScalar::ONE);
    let weights = |x, y| PairShares::Weights(x, y);
    match (a.fixed, b.fixed) {
        (true, true) => return PairShares::None,
        (true, false) => return weights(zero, one),
        (false, true) => return weights(one, zero),
        (false, false) => {}
    }
    if a.owner != b.owner {
        return match (a.holding, b.holding) {
            (true, true) => PairShares::None,
            (true, false) => weights(zero, one),
            (false, true) => weights(one, zero),
            (false, false) => weights(half, half),
        };
    }
    match (a.holding, b.holding) {
        (true, true) => return PairShares::None,
        (true, false) => return weights(zero, one),
        (false, true) => return weights(one, zero),
        (false, false) => {}
    }
    match (a.engaged, b.engaged) {
        (true, true) => return weights(half, half),
        (true, false) => return weights(zero, one),
        (false, true) => return weights(one, zero),
        (false, false) => {}
    }
    if a.moving && b.idle_standing() {
        return if a.push_priority >= b.push_priority {
            weights(zero, one)
        } else {
            weights(one, zero)
        };
    }
    if b.moving && a.idle_standing() {
        return if b.push_priority >= a.push_priority {
            weights(one, zero)
        } else {
            weights(zero, one)
        };
    }
    if a.moving && b.moving {
        if !moving_friends_push {
            return PairShares::ByApproach;
        }
        return match a.push_priority.cmp(&b.push_priority) {
            std::cmp::Ordering::Less => weights(one, zero),
            std::cmp::Ordering::Greater => weights(zero, one),
            std::cmp::Ordering::Equal => weights(half, half),
        };
    }
    weights(half, half)
}

/// The D-β-off shares of two moving friends: each body's approach along the line of centres this
/// tick, as a share of their sum; a body that did not close takes none, and if neither closed they
/// share evenly. `normal` points from `b` to `a`; `step_a` and `step_b` are the bodies' committed
/// steps, world units per tick.
///
/// # Errors
/// An unrepresentable product or quotient.
pub fn approach_shares(
    normal: SimVec2,
    step_a: SimVec2,
    step_b: SimVec2,
) -> Result<(SimScalar, SimScalar), MovementMathError> {
    let along = |step: SimVec2| super::math::checked_dot(step, normal);
    let closing_a = SimScalar::ZERO
        .saturating_sub(along(step_a)?)
        .max(SimScalar::ZERO);
    let closing_b = along(step_b)?.max(SimScalar::ZERO);
    let sum = closing_a
        .checked_add(closing_b)
        .ok_or(MovementMathError::Unrepresentable)?;
    if sum == SimScalar::ZERO {
        return Ok((SimScalar::HALF, SimScalar::HALF));
    }
    let share = |part: SimScalar| {
        part.checked_div(sum)
            .ok_or(MovementMathError::Unrepresentable)
    };
    Ok((share(closing_a)?, share(closing_b)?))
}

/// One body of the solver's tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SolverBody {
    /// The body's entity.
    pub id: EntityId,
    /// Who gives way to whom.
    pub party: ContactParty,
    /// Its layer, collision and terrain radii and body class: only bodies of one class touch.
    pub policy: MotionPolicy,
    /// Where the body stood when the tick began, world units.
    pub start: SimVec2,
    /// Where the body stands now, world units: its committed proposal, then each iteration's
    /// result.
    pub position: SimVec2,
    /// Passes through other bodies (mineral walking, suppressed collision): in no pair.
    pub ghost: bool,
}

/// One pair that may touch, lower id first, with its shares.
#[derive(Debug, Clone, Copy)]
struct Pair {
    a: u32,
    b: u32,
    share_a: SimScalar,
    share_b: SimScalar,
    /// Enemies, or one share zero: constraints D-α never relaxes (C-1b).
    hard: bool,
    radius_sum: SimScalar,
    /// The overlap at or below which the pair is left alone (C-4), world units.
    dead_band: SimScalar,
}

/// One body's corrections within an iteration.
#[derive(Debug, Clone, Copy, Default)]
struct Accumulator {
    sum: SimVec2,
    count: u32,
}

/// The tunable values the solver runs with, read from `movement_rules.ron` (§3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SolverSettings {
    /// Jacobi iterations (C-3).
    pub iterations: u8,
    /// Dead band in thousandths of the radius sum (C-4).
    pub dead_band_permille: u16,
    /// D-β.
    pub moving_friends_push: bool,
}

/// What one solve did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct SolveReport {
    /// Averaged iterations run before the early stop (at most the setting).
    pub iterations: u8,
    /// Pairs in the last list built.
    pub pairs: usize,
    /// Times the pair list was rebuilt within the solve (Verlet rule).
    pub rebuilds: u8,
}

/// Extra reach beyond the radius sum within which a pair is listed, world units (C-1: 1/4).
///
/// The list is a Verlet list (plan Amendment 1, 2 (a)): it is rebuilt when any body's net
/// correction since the last build exceeds half this slack. A pair left out was at least the
/// radius sum plus the slack apart at the build, and each of its bodies has since moved by at most
/// half the slack, so it still cannot touch: the list holds every pair in contact.
const PAIR_SLACK: SimScalar = SimScalar::from_raw(1 << 30);

/// The retained buffers of one tick's contact solve (see the module documentation).
// PERFORMANCE RATIONALE:
// Why: The solver runs every tick over every body, up to fourteen times; allocating its pair list
// and accumulators per tick would put hundreds of allocations in the hot loop.
// What: The body list, pair list, accumulators, grid and neighbour scratch live in the movement
// workspace and are cleared, not freed, each tick; pairs are flat records indexed by body slot.
// Trade-off: Memory stays at the densest tick's high-water mark, in exchange for no allocation in
// steady state and contiguous scans per iteration.
#[derive(Debug, Default)]
pub(crate) struct ContactSolver {
    /// This tick's bodies, in the caller's order.
    pub bodies: Vec<SolverBody>,
    pairs: Vec<Pair>,
    /// Each body's position when the pair list was last built.
    anchors: Vec<SimVec2>,
    sums: Vec<Accumulator>,
    grid: BodyGrid,
    near: Vec<usize>,
}

impl ContactSolver {
    /// Builds the pairs that may touch this tick: same class, neither a ghost, within the radius
    /// sum plus [`PAIR_SLACK`] of each other, `connected` (ground on both sides, as the caller
    /// judges it), with shares from [`contact_shares`]. Each pair is stored lower id first.
    fn build_pairs(
        &mut self,
        map_size: (u32, u32),
        settings: SolverSettings,
        connected: &mut impl FnMut(&SolverBody, &SolverBody) -> bool,
        report: &mut impl FnMut(MovementInvariant),
    ) {
        self.pairs.clear();
        self.anchors.clear();
        self.anchors.extend(self.bodies.iter().map(|b| b.position));
        let mut largest = [SimScalar::ZERO; super::BODY_CLASSES];
        for body in &self.bodies {
            let class = body.policy.body_class();
            largest[class] = largest[class].max(body.policy.radius);
        }
        self.grid.rebuild(
            map_size.0,
            map_size.1,
            self.bodies.iter().map(|b| b.position),
        );
        let mut near = std::mem::take(&mut self.near);
        for index in 0..self.bodies.len() {
            let a = self.bodies[index];
            if a.ghost {
                continue;
            }
            let class = a.policy.body_class();
            let reach = a.policy.radius + largest[class] + PAIR_SLACK;
            near.clear();
            let bodies = &self.bodies;
            self.grid.for_each_near(a.position, reach, |other| {
                let b = &bodies[other];
                let limit = i128::from(a.policy.radius.raw())
                    + i128::from(b.policy.radius.raw())
                    + i128::from(PAIR_SLACK.raw());
                if other > index
                    && !b.ghost
                    && b.policy.body_class() == class
                    && super::math::within_distance(a.position, b.position, limit)
                {
                    near.push(other);
                }
            });
            for &other in &near {
                let b = self.bodies[other];
                if connected(&a, &b) {
                    self.add_pair(index, other, settings, report);
                }
            }
        }
        self.near = near;
    }

    /// Adds the pair of slots `first` and `second` with its shares, lower id first.
    fn add_pair(
        &mut self,
        first: usize,
        second: usize,
        settings: SolverSettings,
        report: &mut impl FnMut(MovementInvariant),
    ) {
        let (low, high) = if self.bodies[first].id < self.bodies[second].id {
            (first, second)
        } else {
            (second, first)
        };
        let (a, b) = (self.bodies[low], self.bodies[high]);
        let fault = |source| MovementInvariant::Arithmetic {
            entity: a.id,
            operation: "contact solver pair",
            source,
        };
        let (share_a, share_b) =
            match contact_shares(&a.party, &b.party, settings.moving_friends_push) {
                PairShares::None => return,
                PairShares::Weights(x, y) => (x, y),
                PairShares::ByApproach => {
                    let shares = pair_normal(&a, &b).and_then(|normal| {
                        let step = |body: &SolverBody| checked_vec_sub(body.position, body.start);
                        approach_shares(normal, step(&a)?, step(&b)?)
                    });
                    match shares {
                        Ok(shares) => shares,
                        Err(source) => return report(fault(source)),
                    }
                }
            };
        let Some(radius_sum) = a.policy.radius.checked_add(b.policy.radius) else {
            return report(fault(MovementMathError::Unrepresentable));
        };
        let band = i128::from(radius_sum.raw()) * i128::from(settings.dead_band_permille) / 1000;
        let dead_band = SimScalar::from_raw(i64::try_from(band).unwrap_or(i64::MAX));
        self.pairs.push(Pair {
            a: u32::try_from(low).unwrap_or(u32::MAX),
            b: u32::try_from(high).unwrap_or(u32::MAX),
            share_a,
            share_b,
            hard: a.party.owner != b.party.owner
                || share_a == SimScalar::ZERO
                || share_b == SimScalar::ZERO,
            radius_sum,
            dead_band,
        });
    }

    /// The pairs built, for tests and the cost report.
    pub(crate) fn pair_count(&self) -> usize {
        self.pairs.len()
    }

    /// Builds the pair list over [`Self::bodies`] and runs the K averaged iterations and the hard
    /// pass (C-1, C-1b), rebuilding the list by the Verlet rule ([`PAIR_SLACK`]). `connected`
    /// says whether two bodies may touch across the terrain between them; `terrain` takes a body,
    /// its current position and the averaged target, and returns where the body may go: the
    /// target, a slide along a wall, or `None` to stay (C-6).
    pub(crate) fn solve(
        &mut self,
        (settings, map_size): (SolverSettings, (u32, u32)),
        mut connected: impl FnMut(&SolverBody, &SolverBody) -> bool,
        mut terrain: impl FnMut(&SolverBody, SimVec2) -> Result<Option<SimVec2>, MovementMathError>,
        report: &mut impl FnMut(MovementInvariant),
    ) -> SolveReport {
        self.build_pairs(map_size, settings, &mut connected, report);
        let mut done = SolveReport::default();
        for _ in 0..settings.iterations {
            done.iterations += 1;
            self.accumulate(false, report);
            if !self.apply(true, &mut terrain, report) {
                break;
            }
            if self.list_is_stale() {
                self.build_pairs(map_size, settings, &mut connected, report);
                done.rebuilds = done.rebuilds.saturating_add(1);
            }
        }
        self.accumulate(true, report);
        self.apply(false, &mut terrain, report);
        done.pairs = self.pairs.len();
        done
    }

    /// Whether any body has moved more than half [`PAIR_SLACK`] since the list was built.
    fn list_is_stale(&self) -> bool {
        let half = i128::from(PAIR_SLACK.raw()) / 2;
        self.bodies
            .iter()
            .zip(&self.anchors)
            .any(|(body, &anchor)| !super::math::within_distance(body.position, anchor, half))
    }

    /// Sums every (hard, when `hard_only`) pair's corrections from the current positions.
    fn accumulate(&mut self, hard_only: bool, report: &mut impl FnMut(MovementInvariant)) {
        self.sums.clear();
        self.sums.resize(self.bodies.len(), Accumulator::default());
        for pair in &self.pairs {
            if hard_only && !pair.hard {
                continue;
            }
            let (ia, ib) = (pair.a as usize, pair.b as usize);
            let (a, b) = (&self.bodies[ia], &self.bodies[ib]);
            match pair_correction(pair, a, b) {
                Ok(None) => {}
                Ok(Some((delta_a, delta_b))) => {
                    let added = add_to(&mut self.sums, ia, delta_a, pair.share_a)
                        .and_then(|()| add_to(&mut self.sums, ib, delta_b, pair.share_b));
                    if let Err(source) = added {
                        report(MovementInvariant::Arithmetic {
                            entity: a.id,
                            operation: "contact solver sum",
                            source,
                        });
                    }
                }
                Err(source) => report(MovementInvariant::Arithmetic {
                    entity: a.id,
                    operation: "contact solver pair",
                    source,
                }),
            }
        }
    }

    /// Moves every body with corrections: by their mean when `average`, else by their sum, through
    /// `terrain`. Returns whether any body moved.
    fn apply(
        &mut self,
        average: bool,
        terrain: &mut impl FnMut(&SolverBody, SimVec2) -> Result<Option<SimVec2>, MovementMathError>,
        report: &mut impl FnMut(MovementInvariant),
    ) -> bool {
        let mut moved = false;
        for (body, acc) in self.bodies.iter_mut().zip(&self.sums) {
            if acc.count == 0 || body.party.fixed {
                continue;
            }
            let step = if average && acc.count > 1 {
                let count = SimScalar::from_num(acc.count);
                super::math::checked_vec_div(acc.sum, count)
            } else {
                Ok(acc.sum)
            };
            let result = step
                .and_then(|step| checked_vec_add(body.position, step))
                .and_then(|target| terrain(body, target));
            match result {
                Ok(Some(next)) if next != body.position => {
                    body.position = next;
                    moved = true;
                }
                Ok(_) => {}
                Err(source) => report(MovementInvariant::Arithmetic {
                    entity: body.id,
                    operation: "contact solver terrain step",
                    source,
                }),
            }
        }
        moved
    }
}

/// Whether two bodies may touch across the terrain between them (C-7): an air body always; a
/// ground body when both centres share a cell or the ground between them is connected, so a
/// cliff keeps them apart.
pub(crate) fn bodies_touch(map: &SimMap, a: &SolverBody, b: &SolverBody) -> bool {
    a.policy.layer != super::MovementLayer::Ground
        || map.world_to_cell(a.position).ok() == map.world_to_cell(b.position).ok()
        || map.ground_segment_is_connected(a.position, b.position, |_, _| true)
}

/// The terrain step of one correction (C-6), as the legacy collision phase applies one: the
/// target slides from the body's current position as far as terrain allows, the whole tick's
/// travel from `start` must stay walkable, and a ground body may not be pressed deeper into a
/// wall (`settle_on_terrain`). `None` when terrain refuses the correction: the body stays.
///
/// # Errors
/// An unrepresentable step of the wall contact rule.
pub(crate) fn terrain_step(
    map: &SimMap,
    pathfinder: &mut Pathfinder,
    policy: MotionPolicy,
    (start, current): (SimVec2, SimVec2),
    target: SimVec2,
) -> Result<Option<SimVec2>, MovementMathError> {
    let Some(accepted) = slide_target(map, policy, current, target)
        .filter(|&end| policy.permits_segment(map, start, end))
    else {
        return Ok(None);
    };
    settle_on_terrain(map, pathfinder, policy, start, current, accepted)
}

/// Adds `share` of `delta` to slot `index`'s sum, counting it, when the share is positive.
fn add_to(
    sums: &mut [Accumulator],
    index: usize,
    delta: SimVec2,
    share: SimScalar,
) -> Result<(), MovementMathError> {
    if share <= SimScalar::ZERO {
        return Ok(());
    }
    let slot = &mut sums[index];
    slot.sum = checked_vec_add(slot.sum, delta)?;
    slot.count += 1;
    Ok(())
}

/// The unit normal from `b` to `a`; for coincident centres, `−x` (C-1a: `a`, the lower id, moves
/// to `−x` and `b`, the larger id, to `+x`).
fn pair_normal(a: &SolverBody, b: &SolverBody) -> Result<SimVec2, MovementMathError> {
    let normal = direction(a.position, b.position)?;
    Ok(if normal == SimVec2::ZERO {
        SimVec2::new(-SimScalar::ONE, SimScalar::ZERO)
    } else {
        normal
    })
}

/// The pair's corrections for `a` and `b`, shares applied, or `None` within the dead band.
fn pair_correction(
    pair: &Pair,
    a: &SolverBody,
    b: &SolverBody,
) -> Result<Option<(SimVec2, SimVec2)>, MovementMathError> {
    let distance = super::checked_distance(a.position, b.position)
        .ok_or(MovementMathError::Unrepresentable)?;
    let overlap = pair.radius_sum.saturating_sub(distance);
    if overlap <= pair.dead_band {
        return Ok(None);
    }
    let normal = pair_normal(a, b)?;
    let push = |share: SimScalar| {
        overlap
            .checked_mul(share)
            .ok_or(MovementMathError::Unrepresentable)
            .and_then(|amount| checked_vec_scale(normal, amount))
    };
    let delta_a = push(pair.share_a)?;
    let delta_b = super::math::checked_vec_neg(push(pair.share_b)?)?;
    Ok(Some((delta_a, delta_b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn party(owner: u8) -> ContactParty {
        ContactParty {
            owner: PlayerId(owner),
            push_priority: 10,
            fixed: false,
            holding: false,
            engaged: false,
            idle: false,
            moving: false,
        }
    }

    const HALF: SimScalar = SimScalar::HALF;
    const ONE: SimScalar = SimScalar::ONE;
    const ZERO: SimScalar = SimScalar::ZERO;

    fn swap(shares: PairShares) -> PairShares {
        match shares {
            PairShares::Weights(a, b) => PairShares::Weights(b, a),
            other => other,
        }
    }

    /// Every row of C-2, both ways round.
    #[test]
    fn contact_shares_reproduce_every_row_of_the_table() {
        let fixed = ContactParty {
            fixed: true,
            holding: true,
            ..party(0)
        };
        let holding = ContactParty {
            holding: true,
            ..party(0)
        };
        let engaged = ContactParty {
            engaged: true,
            ..party(0)
        };
        let moving = ContactParty {
            moving: true,
            ..party(0)
        };
        let idle = ContactParty {
            idle: true,
            ..party(0)
        };
        let waiting = party(0);
        let enemy = party(1);
        let enemy_holding = ContactParty {
            holding: true,
            ..party(1)
        };
        let low = ContactParty {
            push_priority: 5,
            ..moving
        };
        let high_idle = ContactParty {
            push_priority: 20,
            ..idle
        };
        let rows: [(&ContactParty, &ContactParty, PairShares); 14] = [
            (&fixed, &fixed, PairShares::None),
            (&fixed, &moving, PairShares::Weights(ZERO, ONE)),
            (&fixed, &enemy, PairShares::Weights(ZERO, ONE)),
            (&enemy_holding, &holding, PairShares::None),
            (&enemy_holding, &moving, PairShares::Weights(ZERO, ONE)),
            (&enemy, &moving, PairShares::Weights(HALF, HALF)),
            (&holding, &holding, PairShares::None),
            (&holding, &engaged, PairShares::Weights(ZERO, ONE)),
            (&engaged, &engaged, PairShares::Weights(HALF, HALF)),
            (&engaged, &moving, PairShares::Weights(ZERO, ONE)),
            (&moving, &idle, PairShares::Weights(ZERO, ONE)),
            (&moving, &high_idle, PairShares::Weights(ONE, ZERO)),
            (&low, &moving, PairShares::Weights(ONE, ZERO)),
            (&waiting, &moving, PairShares::Weights(HALF, HALF)),
        ];
        for (a, b, expected) in rows {
            assert_eq!(contact_shares(a, b, true), expected, "{a:?} / {b:?}");
            assert_eq!(contact_shares(b, a, true), swap(expected), "{b:?} / {a:?}");
        }
        assert_eq!(
            contact_shares(&moving, &moving, true),
            PairShares::Weights(HALF, HALF)
        );
        assert_eq!(
            contact_shares(&moving, &moving, false),
            PairShares::ByApproach
        );
        assert_eq!(
            contact_shares(&moving, &idle, false),
            PairShares::Weights(ZERO, ONE)
        );
    }

    /// A small deterministic stream for generated crowds.
    struct Stream(u64);

    impl Stream {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }
        /// A value in `0..bound`.
        fn below(&mut self, bound: u64) -> u64 {
            self.next() % bound
        }
        /// A coordinate `low + k / 64` with `k` in `0..span · 64`.
        fn coordinate(&mut self, low: i64, span: i64) -> SimScalar {
            let k = i64::try_from(self.below(u64::try_from(span * 64).unwrap_or(1))).unwrap_or(0);
            SimScalar::from_raw((low << 32) + (k << 26))
        }
    }

    const SETTINGS: SolverSettings = SolverSettings {
        iterations: 14,
        dead_band_permille: 10,
        moving_friends_push: true,
    };

    fn ground(radius: SimScalar) -> MotionPolicy {
        MotionPolicy {
            layer: super::super::MovementLayer::Ground,
            radius,
            terrain: radius,
            burrowed: false,
        }
    }

    fn solver_body(
        id: u32,
        party: ContactParty,
        radius: f64,
        start: SimVec2,
        position: SimVec2,
    ) -> SolverBody {
        SolverBody {
            id: EntityId(id),
            party,
            policy: ground(SimScalar::from_num(radius)),
            start,
            position,
            ghost: false,
        }
    }

    fn at(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    /// A flat 40 × 40 world, with a wall along column `wall` if given.
    fn flat_world(wall: Option<u32>) -> Result<crate::SimWorld, Box<dyn std::error::Error>> {
        let mut map = sim_map::GameMap::new(
            40,
            40,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        if let Some(x) = wall {
            for y in 0..40 {
                map.cell_mut(x, y)?.flags = sim_map::CellFlags::empty();
            }
        }
        Ok(crate::test_support::world(map, 3))
    }

    /// Solves `bodies` on `world`'s terrain; returns the solver and every reported invariant.
    fn solve_on(
        world: &mut crate::SimWorld,
        bodies: &[SolverBody],
        settings: SolverSettings,
    ) -> (ContactSolver, Vec<MovementInvariant>) {
        let mut solver = ContactSolver::default();
        solver.bodies.extend_from_slice(bodies);
        let mut errors = Vec::new();
        let mut report = |e| errors.push(e);
        let map = &world.map;
        let size = (map.width(), map.height());
        let pathfinder = &mut world.pathfinder;
        solver.solve(
            (settings, size),
            |a, b| bodies_touch(map, a, b),
            |body, target| {
                terrain_step(
                    map,
                    pathfinder,
                    body.policy,
                    (body.start, body.position),
                    target,
                )
            },
            &mut report,
        );
        (solver, errors)
    }

    fn overlap_share(a: &SolverBody, b: &SolverBody) -> f64 {
        let sum = f64::from((a.policy.radius + b.policy.radius).to_f32());
        let d = f64::from(a.position.distance(b.position).to_f32());
        (sum - d) / sum
    }

    /// A generated crowd: random positions, radii, owners and flags, each with a step behind it.
    fn crowd(seed: u64, count: u32) -> Vec<SolverBody> {
        let mut rng = Stream(seed);
        let radii = [0.375, 0.5, 0.625, 0.75];
        (0..count)
            .map(|id| {
                let position = SimVec2::new(rng.coordinate(14, 12), rng.coordinate(14, 12));
                let step = SimVec2::new(rng.coordinate(0, 1) / 8, rng.coordinate(0, 1) / 8);
                let party = ContactParty {
                    owner: PlayerId(u8::from(rng.below(5) == 0)),
                    push_priority: [10_u8, 20][usize::from(rng.below(4) == 0)],
                    fixed: rng.below(20) == 0,
                    holding: rng.below(10) == 0,
                    engaged: rng.below(8) == 0,
                    idle: rng.below(3) == 0,
                    moving: rng.below(2) == 0,
                };
                let party = ContactParty {
                    holding: party.holding || party.fixed,
                    ..party
                };
                let radius = radii[usize::try_from(rng.below(4)).unwrap_or(0)];
                solver_body(id + 1, party, radius, position - step, position)
            })
            .collect()
    }

    /// C-1a: the same bodies in shuffled order give bit-identical positions, under D-β on and off.
    #[test]
    fn the_solution_does_not_depend_on_body_order() -> Result<(), Box<dyn std::error::Error>> {
        let mut world = flat_world(Some(30))?;
        for seed in 1..=30 {
            for push in [true, false] {
                let settings = SolverSettings {
                    moving_friends_push: push,
                    ..SETTINGS
                };
                let bodies = crowd(seed, 48);
                let mut shuffled = bodies.clone();
                let mut rng = Stream(seed ^ 0xABCD);
                for last in (1..shuffled.len()).rev() {
                    let pick = usize::try_from(rng.below(last as u64 + 1))?;
                    shuffled.swap(last, pick);
                }
                let (one, e1) = solve_on(&mut world, &bodies, settings);
                let (two, e2) = solve_on(&mut world, &shuffled, settings);
                assert_eq!((e1, e2), (vec![], vec![]));
                assert!(
                    one.pair_count() > 20,
                    "seed {seed}: {} pairs",
                    one.pair_count()
                );
                let mut a: Vec<_> = one.bodies.iter().map(|b| (b.id, b.position)).collect();
                let mut b: Vec<_> = two.bodies.iter().map(|b| (b.id, b.position)).collect();
                a.sort_by_key(|p| p.0);
                b.sort_by_key(|p| p.0);
                assert_eq!(a, b, "seed {seed}, D-β {push}");
            }
        }
        Ok(())
    }

    /// C-8: a fixed, holding or engaged body never moves under a moving friend, who leaves its
    /// disc; an enemy pair ends within the dead band.
    #[test]
    fn anchored_bodies_stay_and_enemies_separate() -> Result<(), Box<dyn std::error::Error>> {
        let mut world = flat_world(None)?;
        let still = party(0);
        let mover = ContactParty {
            moving: true,
            ..party(0)
        };
        for anchor in [
            ContactParty {
                fixed: true,
                holding: true,
                ..still
            },
            ContactParty {
                holding: true,
                ..still
            },
            ContactParty {
                engaged: true,
                ..still
            },
        ] {
            let bodies = [
                solver_body(1, anchor, 0.5, at(20.0, 20.0), at(20.0, 20.0)),
                solver_body(2, mover, 0.5, at(21.4, 20.1), at(20.6, 20.1)),
            ];
            let (solved, errors) = solve_on(&mut world, &bodies, SETTINGS);
            assert_eq!(errors, []);
            assert_eq!(solved.bodies[0].position, at(20.0, 20.0), "{anchor:?}");
            assert!(
                overlap_share(&solved.bodies[0], &solved.bodies[1]) <= 0.01,
                "{anchor:?}"
            );
        }
        let enemy = ContactParty {
            moving: true,
            ..party(1)
        };
        let bodies = [
            solver_body(1, mover, 0.5, at(19.8, 20.0), at(20.0, 20.0)),
            solver_body(2, enemy, 0.75, at(21.2, 20.0), at(20.9, 20.0)),
            solver_body(3, mover, 0.5, at(19.0, 20.0), at(19.2, 20.0)),
        ];
        let (solved, _) = solve_on(&mut world, &bodies, SETTINGS);
        assert!(overlap_share(&solved.bodies[0], &solved.bodies[1]) <= 0.01);
        Ok(())
    }

    /// C-6: a body pressed against a wall by a friend is never moved deeper into it, and a body
    /// across a cliff is in no pair.
    #[test]
    fn terrain_is_never_entered_and_cliffs_separate() -> Result<(), Box<dyn std::error::Error>> {
        let mut world = flat_world(Some(21))?;
        let mover = ContactParty {
            moving: true,
            ..party(0)
        };
        let bodies = [
            solver_body(1, party(0), 0.5, at(20.5, 20.0), at(20.5, 20.0)),
            solver_body(2, mover, 0.5, at(19.0, 20.0), at(19.9, 20.0)),
        ];
        let (solved, errors) = solve_on(&mut world, &bodies, SETTINGS);
        assert_eq!(errors, []);
        assert!(
            solved.bodies[0].position.x <= at(20.5, 0.0).x,
            "{:?}",
            solved.bodies[0].position
        );
        // The wall holds the first body, so the friend takes the rest of the overlap.
        assert!(overlap_share(&solved.bodies[0], &solved.bodies[1]) <= 0.01);
        let mut map = sim_map::GameMap::new(
            40,
            40,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        for y in 0..40 {
            for x in 20..40 {
                map.cell_mut(x, y)?.cliff_level = sim_map::CliffLevel(1);
            }
        }
        let mut world = crate::test_support::world(map, 4);
        let bodies = [
            solver_body(1, mover, 0.5, at(19.6, 20.0), at(19.6, 20.0)),
            solver_body(2, mover, 0.5, at(20.4, 20.0), at(20.4, 20.0)),
        ];
        let (solved, _) = solve_on(&mut world, &bodies, SETTINGS);
        assert_eq!(solved.pair_count(), 0);
        Ok(())
    }

    /// The C-3 crowd of `seed`: a 9 × 9 hexagonal crowd of radius 0.375 standing in contact, every
    /// body then stepped by up to `ρ · r` (0.56 · 0.375 = 0.21, the shipped catalog's worst pair's
    /// top speed) towards a random side, so no pair gains more than `ρ` of its radius sum.
    fn injected_crowd(seed: u64) -> Result<Vec<SolverBody>, Box<dyn std::error::Error>> {
        let (r, rho) = (0.375_f64, 0.56_f64);
        let mover = ContactParty {
            moving: true,
            ..party(0)
        };
        let mut rng = Stream(seed);
        let mut bodies = Vec::new();
        for row in 0..9_i32 {
            for column in 0..9_i32 {
                let x = 16.0 + 2.0 * r * (f64::from(column) + 0.5 * f64::from(row % 2));
                let y = 16.0 + 2.0 * r * 0.866_025_4 * f64::from(row);
                let angle = f64::from(u32::try_from(rng.below(360))?).to_radians();
                let reach = rho * r * f64::from(u32::try_from(rng.below(101))?) / 100.0;
                let id = u32::try_from(bodies.len())? + 1;
                let moved = at(x + reach * angle.cos(), y + reach * angle.sin());
                bodies.push(solver_body(id, mover, r, at(x, y), moved));
            }
        }
        Ok(bodies)
    }

    /// The largest friendly overlap share after solving `bodies` with `iterations`.
    fn worst_after(
        world: &mut crate::SimWorld,
        bodies: &[SolverBody],
        iterations: u8,
    ) -> Result<f64, Box<dyn std::error::Error>> {
        let settings = SolverSettings {
            iterations,
            ..SETTINGS
        };
        let (solved, errors) = solve_on(world, bodies, settings);
        if !errors.is_empty() {
            return Err(format!("{errors:?}").into());
        }
        Ok(solved
            .bodies
            .iter()
            .enumerate()
            .flat_map(|(i, a)| {
                solved.bodies[i + 1..]
                    .iter()
                    .map(move |b| overlap_share(a, b))
            })
            .fold(0.0_f64, f64::max))
    }

    /// The crowds K is derived on (plan Amendment 1, 2 (b)).
    const DERIVATION_SEEDS: std::ops::RangeInclusive<u64> = 1..=1000;
    /// The fresh, disjoint crowds that confirm K, never used to choose it.
    const CONFIRMATION_SEEDS: std::ops::RangeInclusive<u64> = 1_000_001..=1_001_000;

    /// C-3 (plan Amendment 1, 2 (b)): the shipped `contact.iterations` is the smallest count after
    /// which no friendly pair of any derivation crowd ([`injected_crowd`]) exceeds `δ_move` (5 %):
    /// it holds there and one iteration fewer does not; and it holds on the confirmation crowds.
    #[test]
    fn k_iterations_bring_injected_overlap_below_the_moving_bound(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut world = flat_world(None)?;
        let k = crate::test_support::shipped_catalogs()
            .units
            .movement_rules()
            .contact()
            .iterations;
        let mut fewer_fails = false;
        for seed in DERIVATION_SEEDS {
            let bodies = injected_crowd(seed)?;
            let worst = worst_after(&mut world, &bodies, k)?;
            assert!(
                worst <= 0.05,
                "derivation seed {seed}: worst overlap {worst:.4}"
            );
            fewer_fails |= worst_after(&mut world, &bodies, k - 1)? > 0.05;
        }
        assert!(fewer_fails, "K = {k} is not the smallest passing count");
        for seed in CONFIRMATION_SEEDS {
            let worst = worst_after(&mut world, &injected_crowd(seed)?, k)?;
            assert!(
                worst <= 0.05,
                "confirmation seed {seed}: worst overlap {worst:.4}"
            );
        }
        Ok(())
    }

    /// Scratch sweep of K over the derivation crowds (run by hand with `--ignored`).
    #[test]
    #[ignore = "derivation sweep, run by hand"]
    fn sweep_k_over_the_derivation_crowds() -> Result<(), Box<dyn std::error::Error>> {
        let mut world = flat_world(None)?;
        for k in 14_u8..=30 {
            let mut fails = 0;
            let mut worst = 0.0_f64;
            for seed in DERIVATION_SEEDS {
                let w = worst_after(&mut world, &injected_crowd(seed)?, k)?;
                fails += u32::from(w > 0.05);
                worst = worst.max(w);
            }
            println!("K {k}: {fails} of 1000 above 5 %, worst {worst:.4}");
            if fails == 0 {
                break;
            }
        }
        Ok(())
    }

    /// C-4: a crowd standing in contact whose overlaps are all within the dead band does not move.
    #[test]
    fn a_crowd_at_rest_within_the_dead_band_does_not_move() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut world = flat_world(None)?;
        let r = 0.5_f64;
        let mut bodies = Vec::new();
        for row in 0..6_i32 {
            for column in 0..6_i32 {
                // Spacing 0.995 of the diameter: an overlap of 0.5 % of the radius sum.
                let x = 15.0 + 0.995 * 2.0 * r * (f64::from(column) + 0.5 * f64::from(row % 2));
                let y = 15.0 + 0.995 * 2.0 * r * 0.866_025_4 * f64::from(row);
                let id = u32::try_from(bodies.len())? + 1;
                bodies.push(solver_body(id, party(0), r, at(x, y), at(x, y)));
            }
        }
        let (solved, errors) = solve_on(&mut world, &bodies, SETTINGS);
        assert_eq!(errors, []);
        for (before, after) in bodies.iter().zip(&solved.bodies) {
            assert_eq!(before.position, after.position);
        }
        Ok(())
    }

    #[test]
    fn approach_shares_follow_each_bodys_own_closing() -> Result<(), MovementMathError> {
        let normal = SimVec2::X_AXIS; // from b to a
        let toward_b = SimVec2::new(-SimScalar::from_num(0.1), SimScalar::ZERO);
        let away_from_b = SimVec2::new(SimScalar::from_num(0.1), SimScalar::ZERO);
        // Only a closes: a takes all.
        assert_eq!(
            approach_shares(normal, toward_b, SimVec2::ZERO)?,
            (ONE, ZERO)
        );
        // Both close equally: half each.
        assert_eq!(
            approach_shares(normal, toward_b, away_from_b)?,
            (HALF, HALF)
        );
        // Neither closes: even.
        assert_eq!(
            approach_shares(normal, away_from_b, SimVec2::ZERO)?,
            (HALF, HALF)
        );
        Ok(())
    }
}
