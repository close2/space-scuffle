//! The slot pass of plan `2026-11-20-contact-solver` (§3.1 step 2, §3.4 G-1 to G-7, WP4): the
//! concave as a goal under the solver pipeline.
//!
//! # Responsibility
//! Before any pursuer steps, [`SimWorld::assign_slots`] decides for every chase of the tick
//! whether its unit keeps, frees or chooses a slot on its weapon arc round the target, or stands
//! because no slot is free (G-6). It writes the kept and admitted claims into each unit's
//! [`MovementMemory::slot`](crate::entity::MovementMemory) and lists the units that stand; the
//! chase then walks to its claim's point instead of the target (G-7).
//!
//! # How it fits
//! It reads only the tick-start snapshot the movement phase built (bodies, grids, tick-start
//! positions) and the actors, which no pursuer has committed yet; the arc geometry, the choice and
//! the resolution of contested claims are [`super::concave`]'s.
//!
//! # Invariants
//! - **Order independence (G-4).** Fates are decided from the snapshot alone; claims kept this
//!   tick are collected before any new claim is chosen; new claims are resolved by
//!   [`resolve_claims`]' sort, whose last key is the unit's id. Nothing depends on the order the
//!   pursuers are listed in.
//! - Only the solver pipeline runs the pass; under the legacy pipeline no claim is ever written.
//! - Distances and angles are Q32.32 world units and binary angles; no float.

use super::concave::{
    blocking_interval, choose_slot, claim_fate, resolve_claims, slot_point, ArcInterval, ClaimFate,
    SlotRequest,
};
use super::locomotion::MotionEntry;
use super::math::{difference_product, within_distance};
use super::{MovementInvariant, MovementMathError};
use crate::entity::SlotClaim;
use crate::SimWorld;
use sim_types::{AttackOrigin, EntityId, SimAngle, SimScalar, SimVec2};

/// A claim kept this tick, which blocks its arc for every other claimant of its target (G-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeptClaim {
    /// The claimant.
    pub unit: EntityId,
    /// The target the slot is round.
    pub target: EntityId,
    /// The slot's centre, world units.
    pub point: SimVec2,
    /// The claimant's radius, world units.
    pub radius: SimScalar,
}

/// One chase's geometry for the slot pass, world units and raw centre separations.
#[derive(Clone, Copy)]
pub(crate) struct Chase {
    entry: MotionEntry,
    target: EntityId,
    origin: AttackOrigin,
    /// The target's tick-start position.
    centre: SimVec2,
    /// The pursuer's position (its tick-start position: no pursuer has committed yet).
    position: SimVec2,
    /// The arc's radius `band_max − r/2` (G-2).
    rho: SimScalar,
    in_band: bool,
    leash: SimScalar,
}

impl SimWorld {
    /// The slot pass (G-1 to G-6) over this tick's `pursuits`, before any of them steps. Under the
    /// legacy pipeline it clears the stand list and does nothing else.
    pub(super) fn assign_slots(&mut self, pursuits: &[(MotionEntry, EntityId, AttackOrigin)]) {
        // PERFORMANCE RATIONALE:
        // Why: The pass runs every tick over every chase; per-tick vectors would allocate in the
        // hot loop.
        // What: Its requests, kept claims, stand list, wanting chases and arc intervals live in
        // the movement workspace and are cleared, not freed, each tick.
        // Trade-off: Memory stays at the busiest tick's high-water mark.
        let mut requests = std::mem::take(&mut self.movement.slot_requests);
        let mut kept = std::mem::take(&mut self.movement.slot_kept);
        let mut stand = std::mem::take(&mut self.movement.slot_stand);
        let mut wanting = std::mem::take(&mut self.movement.slot_wanting);
        let mut intervals = std::mem::take(&mut self.movement.slot_intervals);
        requests.clear();
        kept.clear();
        stand.clear();
        wanting.clear();
        if self.catalog.movement_rules().contact().pipeline
            == crate::catalog::MovementPipeline::Solver
        {
            for &(entry, target, origin) in pursuits {
                match self.slot_fate(entry, target, origin, &mut kept) {
                    Ok(Some(chase)) => wanting.push(chase),
                    Ok(None) => {}
                    Err(error) => self.report_movement_invariant(error),
                }
            }
            for chase in &wanting {
                match self.slot_request(chase, &kept, &mut intervals) {
                    Ok(Some(request)) => requests.push(request),
                    Ok(None) => stand.push(chase.entry.id),
                    Err(error) => self.report_movement_invariant(error),
                }
            }
            if let Err(source) = resolve_claims(&mut requests) {
                self.report_movement_invariant(MovementInvariant::Arithmetic {
                    entity: requests.first().map_or(EntityId(0), |r| r.unit),
                    operation: "slot claims",
                    source,
                });
                requests.clear();
            }
            for request in &requests {
                if request.admitted {
                    if let Some(actor) = self.entities.get_mut(&request.unit) {
                        actor.movement.slot = Some(SlotClaim {
                            target: request.target,
                            point: request.point,
                        });
                    }
                } else {
                    // A refused claimant tries again next tick and stands meanwhile (G-4, G-6).
                    stand.push(request.unit);
                }
            }
            stand.sort_unstable();
        }
        self.movement.slot_requests = requests;
        self.movement.slot_kept = kept;
        self.movement.slot_stand = stand;
        self.movement.slot_wanting = wanting;
        self.movement.slot_intervals = intervals;
    }

    /// Whether the pursuer `id` stands this tick for want of a free slot (G-6).
    pub(super) fn stands_for_a_slot(&self, id: EntityId) -> bool {
        self.movement.slot_stand.binary_search(&id).is_ok()
    }

    /// Decides the fate of `entry`'s claim (G-5) and whether it asks for a new slot (G-1): returns
    /// the chase when it does. A kept claim goes into `kept`; a freed one is cleared.
    fn slot_fate(
        &mut self,
        entry: MotionEntry,
        target: EntityId,
        origin: AttackOrigin,
        kept: &mut Vec<KeptClaim>,
    ) -> Result<Option<Chase>, MovementInvariant> {
        let Some(chase) = self.chase_geometry(entry, target, origin)? else {
            return Ok(None);
        };
        let fault = |source| MovementInvariant::Arithmetic {
            entity: entry.id,
            operation: "slot fate",
            source,
        };
        let claim = self.entities.get(&entry.id).and_then(|a| a.movement.slot);
        let radius = entry.policy.radius;
        let wants = match claim {
            Some(claim) => {
                let taken = self.claim_taken(&chase, claim.point).map_err(fault)?;
                let fate = claim_fate(
                    (claim.target, claim.point),
                    (target, chase.centre),
                    (chase.rho, radius),
                    chase.in_band,
                    taken,
                )
                .map_err(fault)?;
                match fate {
                    ClaimFate::Keep => {
                        kept.push(KeptClaim {
                            unit: entry.id,
                            target,
                            point: claim.point,
                            radius,
                        });
                        return Ok(None);
                    }
                    ClaimFate::Free | ClaimFate::Rechoose => {
                        if let Some(actor) = self.entities.get_mut(&entry.id) {
                            actor.movement.slot = None;
                        }
                        fate == ClaimFate::Rechoose && !chase.in_band
                    }
                }
            }
            None => !chase.in_band && self.queued_toward(&chase).map_err(fault)?,
        };
        Ok(wants.then_some(chase))
    }

    /// The chase's arc and band from the tick-start snapshot, or `None` when the unit has no
    /// weapon against the target or the arc would have no radius.
    fn chase_geometry(
        &self,
        entry: MotionEntry,
        target: EntityId,
        origin: AttackOrigin,
    ) -> Result<Option<Chase>, MovementInvariant> {
        let (Some(actor), Some(target_actor)) =
            (self.entities.get(&entry.id), self.entities.get(&target))
        else {
            return Ok(None);
        };
        let Some(band) = self.pursuit_band(entry, actor, target_actor, target, origin)? else {
            return Ok(None);
        };
        let centre = self.tick_start_position(target);
        let half_radius = i128::from(entry.policy.radius.raw()) / 2;
        let rho = i64::try_from(band.max - half_radius)
            .map(SimScalar::from_raw)
            .unwrap_or(SimScalar::MAX);
        if rho <= SimScalar::ZERO {
            return Ok(None);
        }
        let separation = super::checked_distance(actor.position, centre)
            .map(|d| i128::from(d.raw()))
            .unwrap_or(i128::MAX);
        Ok(Some(Chase {
            entry,
            target,
            origin,
            centre,
            position: actor.position,
            rho,
            in_band: separation <= band.max && separation >= band.min,
            leash: band.leash,
        }))
    }

    /// Whether a body of the tick-start snapshot, other than the unit and its target, now
    /// blocks the claimed `point` (G-5: taken).
    fn claim_taken(&self, chase: &Chase, point: SimVec2) -> Result<bool, MovementMathError> {
        let angle = angle_from(chase.centre, point);
        let mut taken = false;
        let mut fault = None;
        self.for_each_snapshot_body_near(chase, |other_pos, other_radius| {
            if taken || fault.is_some() {
                return;
            }
            match blocking_interval(
                chase.centre,
                chase.rho,
                chase.entry.policy.radius,
                other_pos,
                other_radius,
            ) {
                Ok(interval) => taken = interval.is_some_and(|i| i.blocks(angle)),
                Err(error) => fault = Some(error),
            }
        });
        fault.map_or(Ok(taken), Err)
    }

    /// Calls `visit` with the position and radius of every body of the chase's class in the
    /// tick-start snapshot that could block its arc, except the unit and its target.
    fn for_each_snapshot_body_near(
        &self,
        chase: &Chase,
        mut visit: impl FnMut(SimVec2, SimScalar),
    ) {
        let class = chase.entry.policy.body_class();
        let obstacles = &self.movement.obstacles[class];
        let reach = chase.rho + chase.entry.policy.radius + self.movement.largest_radius[class];
        self.movement.grids[class].for_each_near(chase.centre, reach, |index| {
            let body = &obstacles[index];
            if body.id != chase.entry.id && body.id != chase.target {
                visit(body.pos, body.radius);
            }
        });
    }

    /// G-1's trigger: a friend that holds its ground lies within one step of the unit's edge on
    /// the side toward the target.
    fn queued_toward(&self, chase: &Chase) -> Result<bool, MovementMathError> {
        let entry = chase.entry;
        let class = entry.policy.body_class();
        let obstacles = &self.movement.obstacles[class];
        let owner = self.entities.get(&entry.id).map(|a| a.owner);
        let reach = entry.policy.radius + self.movement.largest_radius[class] + entry.step;
        let ahead = super::math::direction(chase.centre, chase.position)?;
        let mut queued = false;
        let mut fault = None;
        self.movement.grids[class].for_each_near(chase.position, reach, |index| {
            let body = &obstacles[index];
            if queued || fault.is_some() || body.id == entry.id || Some(body.owner) != owner {
                return;
            }
            if !body.anchored || body.yields_to(body.owner, entry.push_priority) {
                return;
            }
            let limit = i128::from(entry.policy.radius.raw())
                + i128::from(body.radius.raw())
                + i128::from(entry.step.raw());
            if !within_distance(chase.position, body.pos, limit) {
                return;
            }
            match difference_product(ahead, body.pos, chase.position, false) {
                Ok(along) => queued = along > 0,
                Err(error) => fault = Some(error),
            }
        });
        fault.map_or(Ok(queued), Err)
    }

    /// The claim `chase` asks for (G-2, G-3): the free angle nearest its bearing among the
    /// snapshot's bodies and the claims kept this tick, or `None` when none is free.
    fn slot_request(
        &self,
        chase: &Chase,
        kept: &[KeptClaim],
        intervals: &mut Vec<ArcInterval>,
    ) -> Result<Option<SlotRequest>, MovementInvariant> {
        let fault = |source| MovementInvariant::Arithmetic {
            entity: chase.entry.id,
            operation: "slot choice",
            source,
        };
        let radius = chase.entry.policy.radius;
        intervals.clear();
        let mut failed = None;
        self.for_each_snapshot_body_near(chase, |pos, other_radius| {
            match blocking_interval(chase.centre, chase.rho, radius, pos, other_radius) {
                Ok(Some(interval)) => intervals.push(interval),
                Ok(None) => {}
                Err(error) => failed = Some(error),
            }
        });
        if let Some(error) = failed {
            return Err(fault(error));
        }
        for claim in kept
            .iter()
            .filter(|c| c.target == chase.target && c.unit != chase.entry.id)
        {
            if let Some(interval) =
                blocking_interval(chase.centre, chase.rho, radius, claim.point, claim.radius)
                    .map_err(fault)?
            {
                intervals.push(interval);
            }
        }
        let bearing = angle_from(chase.centre, chase.position);
        let leash_origin = self.leash_origin_of(chase);
        let chosen = choose_slot(bearing, intervals, |angle| {
            slot_point(chase.centre, chase.rho, angle).is_ok_and(|point| {
                leash_origin.is_none_or(|origin| {
                    within_distance(origin, point, i128::from(chase.leash.raw()))
                }) && self.arc_stays_on_ground(chase, bearing, angle)
            })
        });
        let Some(angle) = chosen else {
            return Ok(None);
        };
        let point = slot_point(chase.centre, chase.rho, angle).map_err(fault)?;
        let distance = super::checked_distance(chase.position, chase.centre)
            .ok_or_else(|| fault(MovementMathError::Unrepresentable))?;
        Ok(Some(SlotRequest {
            unit: chase.entry.id,
            target: chase.target,
            centre: chase.centre,
            bearing,
            angle,
            point,
            rho: chase.rho,
            radius,
            distance,
            admitted: false,
        }))
    }

    /// Whether the arc of the slot circle from `bearing` to `angle`, the way the unit walks round
    /// the target to its slot, stays inside the unit's terrain domain (G-3: a slot outside the
    /// domain, or one behind terrain such as a plugged corridor's far side, is no candidate).
    /// The arc is sampled every half radius of the unit, both ends included.
    fn arc_stays_on_ground(&self, chase: &Chase, bearing: SimAngle, angle: SimAngle) -> bool {
        /// Binary angle units per radian: a full turn of 65,536 units over 2π, rounded down.
        const UNITS_PER_RADIAN: i128 = 10_430;
        let turn = i32::from(bearing.diff(angle));
        // Half a radius of arc, in binary units: (r / 2) / rho radians.
        let step = (i128::from(chase.entry.policy.radius.raw()) * UNITS_PER_RADIAN
            / (2 * i128::from(chase.rho.raw()).max(1)))
        .clamp(1, 4_096);
        let step = i32::try_from(step).unwrap_or(4_096);
        let samples = turn.abs() / step + 1;
        (0..=samples).all(|k| {
            let offset = i16::try_from(turn * k / samples).unwrap_or(0);
            let at = SimAngle::from_raw(bearing.raw().wrapping_add_signed(offset));
            slot_point(chase.centre, chase.rho, at)
                .is_ok_and(|point| chase.entry.policy.contains(&self.map, point))
        })
    }

    /// Where an automatic chase's leash is measured from (G-7): the recorded origin, or the
    /// unit's position on a chase's first tick; `None` for an ordered attack, which has no leash.
    fn leash_origin_of(&self, chase: &Chase) -> Option<SimVec2> {
        (chase.origin == AttackOrigin::Acquired).then(|| {
            self.entities
                .get(&chase.entry.id)
                .and_then(|a| a.movement.leash_origin)
                .unwrap_or(chase.position)
        })
    }

    /// The position `id` had when this tick's movement phase began, world units: from the
    /// phase's record, or the entity's position when it takes no part in movement.
    pub(super) fn tick_start_position(&self, id: EntityId) -> SimVec2 {
        let starts = &self.movement.tick_start;
        match starts.binary_search_by_key(&id, |&(entity, _)| entity) {
            Ok(index) => starts[index].1,
            Err(_) => self
                .entities
                .get(&id)
                .map_or(SimVec2::ZERO, |entity| entity.position),
        }
    }
}

/// The binary angle of `point` seen from `centre`.
fn angle_from(centre: SimVec2, point: SimVec2) -> SimAngle {
    SimAngle::atan2(
        point.y.saturating_sub(centre.y),
        point.x.saturating_sub(centre.x),
    )
}
