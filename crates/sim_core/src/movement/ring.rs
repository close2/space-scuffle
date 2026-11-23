//! The ring rule of plan `2026-11-20-contact-solver` Amendment 7, item 2: where an attacker
//! chasing a target under the solver pipeline heads, read from the tick-start snapshot alone.
//!
//! # Responsibility
//! [`SimWorld::ring_aim`] looks at the **ring** round a chase target, the circle on which the
//! unit's weapon just reaches it (the band's outer edge, where `StarCraft` II attackers stop), and
//! decides one of three things ([`RingAim`]):
//! - the unit's own bearing point on the ring is free: it chases straight in and stops in range;
//! - its bearing point is blocked, but another angle is free: it walks to the gap, the nearest
//!   such angle, ties clockwise (G-3), keeping last tick's gap while that stays free;
//! - no angle is free: it stands, keeps its order and accrues no stall (G-6, Q0089).
//!
//! # How it fits
//! The pursuit (`locomotion::prepare_pursuit`) asks it once per chase and tick, before the chase
//! steps. Occupancy is **geometry, not class**: a body blocks the ring when its disc reaches it,
//! whatever its owner, order or state (Amendment 7, item 2). There are no claims and no
//! contention: a choice is a function of the snapshot and the unit's own position, so it does not
//! depend on the order pursuers are processed in.
//!
//! # Invariants
//! - Reads only the tick-start bodies of the movement snapshot, the target's tick-start position
//!   and the static map; never a body's class or another unit's decision this tick.
//! - A free angle is admissible only when the unit's terrain disc can walk the ring to it from
//!   its own bearing point (so a slot behind a wall, or on a plugged corridor's far side, is
//!   none), and, for an automatic chase, when it lies within the leash.
//! - The rule applies only to a unit outside its band whose own bearing point is admissible;
//!   otherwise the chase runs as it always has.
//! - Distances are Q32.32 world units, angles `SimAngle` binary units; no float.

use super::concave::{blocking_interval, choose_slot, is_free, slot_point, ArcInterval};
use super::contact::contact_request;
use super::locomotion::MotionEntry;
use super::math::within_distance;
use super::{MotionPolicy, MovementInvariant, MovementLayer, MovementMathError};
use crate::SimWorld;
use sim_map::{Pathfinder, SimMap};
use sim_types::{EntityId, SimAngle, SimScalar, SimVec2};

/// What the ring rule decides for one chase this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RingAim {
    /// The unit's bearing point is free, or the rule does not apply: chase straight in.
    Straight,
    /// The bearing point is blocked; the nearest free angle and its point on the ring.
    Gap(SimAngle, SimVec2),
    /// No free angle on the ring: stand, keep the order, accrue no stall.
    Stand,
}

/// One chase as the ring rule sees it.
#[derive(Clone, Copy)]
pub(crate) struct RingChase {
    /// The pursuer.
    pub entry: MotionEntry,
    /// Its position (its tick-start position: no pursuer has committed when it is asked).
    pub position: SimVec2,
    /// The chased target.
    pub target: EntityId,
    /// The band's outer edge: the furthest centre separation the weapon reaches, raw units.
    pub band_max: i128,
    /// An automatic chase's leash: where it is measured from and its radius, world units.
    pub leash: Option<(SimVec2, SimScalar)>,
    /// The gap point the unit aimed at last tick on this target's ring, if any.
    pub kept: Option<SimVec2>,
}

/// Binary angle units per radian: a full turn of 65,536 units over 2π, rounded down.
const UNITS_PER_RADIAN: i128 = 10_430;

/// Half a turn in binary angle units: the furthest the ring walk goes on either side.
const HALF_TURN_UNITS: i32 = 32_768;

/// Whether a disc of the policy's terrain radius may sweep from `a` to `b`: the centre rule and,
/// for a ground body on a meshed map, the mesh's disc predicate (plan
/// `2026-11-11-contact-and-missiles` D-7). `a == b` asks whether the disc fits at `a`.
fn disc_sweeps(
    map: &SimMap,
    pathfinder: &mut Pathfinder,
    policy: MotionPolicy,
    a: SimVec2,
    b: SimVec2,
) -> bool {
    if !policy.permits_segment(map, a, b) {
        return false;
    }
    if policy.layer != MovementLayer::Ground || pathfinder.navmesh().is_none() {
        return true;
    }
    let request = contact_request(a, policy.terrain_radius());
    pathfinder.segment_is_passable(map, &request, a, b)
}

/// The walk round the ring from the unit's bearing, sampled every half radius of the unit and
/// extended lazily, one side at a time: how far each side stays on the unit's ground.
struct RingWalk {
    centre: SimVec2,
    rho: SimScalar,
    bearing: SimAngle,
    /// Binary units between samples.
    step: i32,
    /// Per side (clockwise, counter-clockwise): the last sample index reached, and whether the
    /// segment to the next one failed.
    reached: [(i32, bool); 2],
}

impl RingWalk {
    fn new(centre: SimVec2, rho: SimScalar, bearing: SimAngle, radius: SimScalar) -> Self {
        // Half a radius of arc, (r / 2) / ρ radians, in binary units.
        let step = (i128::from(radius.raw()) * UNITS_PER_RADIAN
            / (2 * i128::from(rho.raw()).max(1)))
        .clamp(1, 4_096);
        Self {
            centre,
            rho,
            bearing,
            step: i32::try_from(step).unwrap_or(4_096),
            reached: [(0, false); 2],
        }
    }

    /// The point `offset` binary units from the bearing (positive counter-clockwise).
    fn point(&self, offset: i32) -> Result<SimVec2, MovementMathError> {
        let offset = u16::try_from(offset.rem_euclid(65_536)).unwrap_or(0);
        let wrapped = self.bearing.raw().wrapping_add(offset);
        slot_point(self.centre, self.rho, SimAngle::from_raw(wrapped))
    }

    /// Whether the unit can walk the ring from its bearing to the angle `turn` binary units away
    /// (negative clockwise) and stand there.
    fn reaches(
        &mut self,
        turn: i32,
        map: &SimMap,
        pathfinder: &mut Pathfinder,
        policy: MotionPolicy,
    ) -> Result<bool, MovementMathError> {
        let side = usize::from(turn > 0);
        let sign = if turn > 0 { 1 } else { -1 };
        let whole = turn.abs() / self.step;
        let (mut index, mut failed) = self.reached[side];
        while index < whole && !failed {
            let from = self.point(sign * index * self.step)?;
            let to = self.point(sign * ((index + 1) * self.step).min(HALF_TURN_UNITS))?;
            if disc_sweeps(map, pathfinder, policy, from, to) {
                index += 1;
            } else {
                failed = true;
            }
        }
        self.reached[side] = (index, failed);
        if index < whole {
            return Ok(false);
        }
        let from = self.point(sign * whole * self.step)?;
        let to = self.point(turn)?;
        Ok(disc_sweeps(map, pathfinder, policy, from, to))
    }
}

/// The binary angle of `point` seen from `centre`.
pub(crate) fn angle_from(centre: SimVec2, point: SimVec2) -> SimAngle {
    SimAngle::atan2(
        point.y.saturating_sub(centre.y),
        point.x.saturating_sub(centre.x),
    )
}

/// The ring rule over the blocked `intervals` (see [`RingAim`]): `admissible` says whether an
/// angle can be walked to and lies within the leash.
pub(crate) fn ring_choice(
    bearing: SimAngle,
    intervals: &[ArcInterval],
    centre: SimVec2,
    rho: SimScalar,
    admissible: impl FnMut(SimAngle) -> bool,
) -> Result<RingAim, MovementMathError> {
    match choose_slot(bearing, intervals, admissible) {
        None => Ok(RingAim::Stand),
        Some(angle) if angle == bearing => Ok(RingAim::Straight),
        Some(angle) => Ok(RingAim::Gap(angle, slot_point(centre, rho, angle)?)),
    }
}

impl SimWorld {
    /// The ring rule for `chase` this tick (see the module documentation).
    ///
    /// # Errors
    /// An unrepresentable distance or point of the ring's geometry.
    pub(crate) fn ring_aim(&mut self, chase: RingChase) -> Result<RingAim, MovementInvariant> {
        let fault = |source| MovementInvariant::Arithmetic {
            entity: chase.entry.id,
            operation: "ring rule",
            source,
        };
        let separation =
            super::checked_distance(chase.position, self.tick_start_position(chase.target))
                .map_or(i128::MAX, |d| i128::from(d.raw()));
        let rho = i64::try_from(chase.band_max).map_or(SimScalar::MAX, SimScalar::from_raw);
        if rho <= SimScalar::ZERO || separation <= chase.band_max {
            return Ok(RingAim::Straight);
        }
        // PERFORMANCE RATIONALE:
        // Why: The rule runs for every chase every tick; a vector of blocked arcs per chase would
        // allocate in the hot loop.
        // What: The arcs go into one retained buffer of the movement workspace, cleared per chase.
        // Trade-off: Memory stays at the most crowded ring's high-water mark.
        let mut intervals = std::mem::take(&mut self.movement.ring_intervals);
        let aim = self
            .ring_aim_with(chase, rho, &mut intervals)
            .map_err(fault);
        self.movement.ring_intervals = intervals;
        aim
    }

    fn ring_aim_with(
        &mut self,
        chase: RingChase,
        rho: SimScalar,
        intervals: &mut Vec<ArcInterval>,
    ) -> Result<RingAim, MovementMathError> {
        let centre = self.tick_start_position(chase.target);
        let policy = chase.entry.policy;
        let radius = policy.radius;
        let class = policy.body_class();
        intervals.clear();
        let obstacles = &self.movement.obstacles[class];
        let reach = rho
            .saturating_add(radius)
            .saturating_add(self.movement.largest_radius[class]);
        let mut failed = None;
        self.movement.grids[class].for_each_near(centre, reach, |index| {
            let body = &obstacles[index];
            if body.id == chase.entry.id || body.id == chase.target || failed.is_some() {
                return;
            }
            match blocking_interval(centre, rho, radius, body.pos, body.radius) {
                Ok(Some(interval)) => intervals.push(interval),
                Ok(None) => {}
                Err(error) => failed = Some(error),
            }
        });
        if let Some(error) = failed {
            return Err(error);
        }
        let bearing = angle_from(centre, chase.position);
        let (map, pathfinder) = (&self.map, &mut self.pathfinder);
        let own = slot_point(centre, rho, bearing)?;
        let leashed = |point: SimVec2| {
            chase.leash.is_none_or(|(origin, radius)| {
                within_distance(origin, point, i128::from(radius.raw()))
            })
        };
        if !leashed(own) || !disc_sweeps(map, pathfinder, policy, own, own) {
            // The unit does not come at the ring along its bearing (terrain or the leash is in
            // the way): the chase runs as it always has.
            return Ok(RingAim::Straight);
        }
        if is_free(bearing, intervals) {
            return Ok(RingAim::Straight);
        }
        let mut walk = RingWalk::new(centre, rho, bearing, radius);
        let mut fault = None;
        let mut admissible = |angle: SimAngle| {
            let turn = i32::from(bearing.diff(angle));
            let point = slot_point(centre, rho, angle);
            match point.and_then(|p| Ok(leashed(p) && walk.reaches(turn, map, pathfinder, policy)?))
            {
                Ok(admitted) => admitted,
                Err(error) => {
                    fault = Some(error);
                    false
                }
            }
        };
        // Hysteresis: last tick's gap is kept while it stays free and reachable, so two gaps
        // equally near do not alternate. It is one remembered point, not a claim.
        let kept = chase
            .kept
            .map(|point| angle_from(centre, point))
            .filter(|&angle| is_free(angle, intervals) && admissible(angle));
        let aim = match kept {
            Some(angle) => RingAim::Gap(angle, slot_point(centre, rho, angle)?),
            None => ring_choice(bearing, intervals, centre, rho, &mut admissible)?,
        };
        fault.map_or(Ok(aim), Err)
    }

    /// The position `id` had when this tick's movement phase began, world units: from the
    /// phase's record, or the entity's position when it takes no part in movement.
    pub(crate) fn tick_start_position(&self, id: EntityId) -> SimVec2 {
        self.tick_start_motion(id).0
    }

    /// The position and last displacement `id` had when this tick's movement phase began, world
    /// units and world units per tick: from the phase's record, or the entity's current ones when
    /// it takes no part in movement (plan `2026-11-20-contact-solver` WP5).
    pub(crate) fn tick_start_motion(&self, id: EntityId) -> (SimVec2, SimVec2) {
        let starts = &self.movement.tick_start;
        match starts.binary_search_by_key(&id, |&(entity, _, _)| entity) {
            Ok(index) => (starts[index].1, starts[index].2),
            Err(_) => self
                .entities
                .get(&id)
                .map_or((SimVec2::ZERO, SimVec2::ZERO), |entity| {
                    (entity.position, entity.velocity)
                }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    /// A free bearing is chased straight; a blocked one aims at the nearest free angle; a full
    /// ring stands. The choice reads the intervals only.
    #[test]
    fn the_ring_rule_chases_aims_or_stands() -> Result<(), Box<dyn std::error::Error>> {
        let (centre, rho, r) = (SimVec2::ZERO, SimScalar::from_num(2), SimScalar::HALF);
        let west = blocking_interval(centre, rho, r, at(-2.0, 0.0), r)?.ok_or("west")?;
        let bearing_west = angle_from(centre, at(-9.0, 0.0));
        let bearing_north = angle_from(centre, at(0.0, 9.0));
        assert_eq!(
            ring_choice(bearing_north, &[west], centre, rho, |_| true)?,
            RingAim::Straight
        );
        let RingAim::Gap(angle, point) = ring_choice(bearing_west, &[west], centre, rho, |_| true)?
        else {
            return Err("a blocked bearing with room aims at a gap".into());
        };
        // The bearing sits on the interval's centre, so both ends are equally near and the tie
        // goes clockwise: a smaller angle than 180°, which lies at y > 0.
        assert!(point.y > SimScalar::ZERO, "{angle:?} {point:?}");
        let whole = ArcInterval {
            centre: SimAngle::from_raw(0),
            half: super::super::concave::HALF_TURN,
        };
        assert_eq!(
            ring_choice(bearing_west, &[whole], centre, rho, |_| true)?,
            RingAim::Stand
        );
        // Room that cannot be walked to is no room.
        assert_eq!(
            ring_choice(bearing_west, &[west], centre, rho, |a| a == bearing_west)?,
            RingAim::Stand
        );
        Ok(())
    }

    /// The walk's step is half the unit's radius of arc, at least one binary unit.
    #[test]
    fn the_ring_walk_samples_every_half_radius() {
        let walk = RingWalk::new(
            SimVec2::ZERO,
            SimScalar::from_num(4),
            SimAngle::from_raw(0),
            SimScalar::ONE,
        );
        // (1 / 2) / 4 radians = 0.125 rad ≈ 1,303 binary units.
        assert_eq!(walk.step, 1_303);
        let tiny = RingWalk::new(
            SimVec2::ZERO,
            SimScalar::from_num(1_000_000),
            SimAngle::from_raw(0),
            SimScalar::from_raw(1),
        );
        assert_eq!(tiny.step, 1);
    }
}
