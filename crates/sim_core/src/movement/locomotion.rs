//! Per-actor locomotion: following routes, arriving and chasing targets.
//!
//! Owns tick phase 5. Each actor prepares one [`MotionProposal`] from a tick-start snapshot of
//! its neighbours, and scalar pose edits and route or order edits commit together after all
//! fallible work. Earlier successful actors stay committed when a later actor is diagnosed.
//! Preparation runs in named stages over one [`MotionDraft`]: route upkeep, the arrival
//! decision, the progress check and the step, with a range check first for pursuit.
//!
//! # The rules of an order's life
//! - A route is checked when something may have invalidated it, not every tick: on a spaced
//!   schedule, when the owner's knowledge of the map changes, and when a step is refused.
//! - A unit is done when it reaches the destination, or when it touches a body that already
//!   settled on the same destination. Standing still proves nothing about a neighbour.
//! - A unit that stops getting closer rebuilds its route, and in the end gives the order up.
//! - An unreachable destination is replaced by the nearest reachable point and reported.
//! - A unit with a finite deceleration brakes at it into its destination and into its weapon's
//!   range, and glides to rest at it when its order ends without arriving (B-39, Q0067).
//!
//! Pursuit obeys the same route, steering and contact rules. Only its goal moves. A player's
//! attack order whose target its player has lost to fog is no pursuit: the unit walks, as on a
//! move order, to where the target was last seen, and the order ends there (plan
//! `2026-11-24-weapon-validation` Amendment 2, B-195; Q0106 part 1 (a)).
//!
//! A follow (plan `2026-11-26-follow-order`) is a pursuit of a unit with a dead band instead of a
//! weapon band: its submodule [`follow`] walks a follower to its target's tick-start position,
//! rests it on touch, and walks it again once the target has moved past the slop.
//!
//! # Invariants
//! - No route search runs twice for one actor within [`REPATH_COOLDOWN_TICKS`].
//! - Everything that decides an outcome is read from the tick-start snapshot or the map, so the
//!   result does not depend on the order actors are processed in beyond the committed pose.
//! - A unit whose archetype is flagged `turn_before_move` translates by its own motion on a tick
//!   only if, after that tick's hull turn, its heading equals the direction of its step (plan
//!   `2026-11-05-turn-before-move` D-1, Q0050). While the step lies further from its heading than
//!   one moving tick's turn, it turns in place at its stationary rate: it keeps its order and
//!   route, and its velocity is zero. Collision pushes are not its own motion and are not gated. Every other unit moves
//!   along its path at once while its hull follows.

use super::contact::{probe_permits, settle_on_terrain, slide_along_wall};
use super::grid::BodyGrid;
use super::intent::{intent_heading, IntentInput, SideMemory};
use super::math::{
    checked_dot, checked_vec_add, checked_vec_scale, checked_vec_sub, difference_product,
    direction, distance, within_distance,
};
use super::ring::{RingAim, RingChase};
use super::tuning::{
    ABANDONED_TARGET_TICKS, ARRIVAL_WITNESS_REACH, FLANK_REVERSE_TICKS, PURSUIT_ROUTE_DRIFT,
    REPATH_COOLDOWN_TICKS, ROUTE_CHECK_PERIOD, ROUTE_START_TOLERANCE, STALL_ABANDON_TICKS,
    STALL_ARRIVAL_REACH, STALL_PROGRESS, STALL_REPATH_TICKS, STANDING_DRIFT, STEERING_LOOKAHEAD,
    TERRAIN_GUARD_HORIZON, WAIT_THRESHOLD, WAYPOINT_HANDOFF_MARGIN,
};
use super::{
    MotionPolicy, MovementInvariant, MovementLayer, MovementMathError, MovementWorkspace,
    RouteOutcome, RouteOutcomeKind,
};
use crate::catalog::{ContactRules, MovementPipeline};
use crate::entity::{Entity, MovementMemory, SlotClaim};
use crate::steering::{
    check_cluster_arrival_with, remove_blocked_velocity, steer_body, surround_side, ContactBody,
    SteeringObstacle,
};
use crate::{SimWorld, WeaponPhase};
use sim_map::{MapError, PathRequestWithVision, PathResult, Pathfinder, PlayerVisionMap, SimMap};
use sim_types::{
    AllowedMovement, AnimState, AttackOrigin, EntityId, MoverType, PerPlayer, PlayerSlot, SimAngle,
    SimScalar, SimVec2, Tick, UnitOrder, MAX_PLAYERS,
};
use std::ops::ControlFlow;

mod follow;

/// An actor validated for this tick, with the rates it moves at.
#[derive(Clone, Copy)]
pub(crate) struct MotionEntry {
    pub(crate) id: EntityId,
    pub(crate) policy: MotionPolicy,
    /// Effective top speed in world units per tick, including upgrades and buffs.
    pub(crate) speed: SimScalar,
    /// This tick's own-motion budget in world units per tick: [`Self::speed`] limited by the
    /// archetype's acceleration from the speed achieved on the tick before (plan
    /// `2026-11-05-sc2-deviation-fixes` WP12). A walking unit lowers it further to brake into
    /// its destination ([`braking_speed`]).
    pub(crate) step: SimScalar,
    /// Speed lost per tick when braking, world units per tick per tick, or `None` when the
    /// archetype stops within one tick (its `deceleration` is at least its `max_speed`).
    pub(crate) deceleration: Option<SimScalar>,
    /// Authored priority for friendly contact.
    pub(crate) push_priority: u8,
    /// Hull rate in binary angle units per tick (the archetype's `turn_rate`).
    pub(crate) turn_rate: u16,
    /// Whether the hull must face a step before the unit takes it.
    pub(crate) turn_before_move: bool,
    /// Whether a blocked attacker walks round its own crowd (the archetype's `surround_walk`).
    pub(crate) surround_walk: bool,
    /// Auto-acquisition scan range in world units, edge to edge: how far a surround walk without
    /// a target goes before it ends.
    pub(crate) scan_range: SimScalar,
    /// Whether the actor is a registered structure (`map.structures()` holds its id, plan 49's
    /// foundation identity). A registered structure is no body: it is neither a steering obstacle
    /// nor a collision body, and meets units only through its footprint at their terrain radius
    /// (plan `2026-11-18-structure-contact` D-1, B-131).
    pub(crate) foundation: bool,
}

/// The nearest reachable point last found, so a group costs one region scan, not one per unit.
#[derive(Clone, Copy)]
pub(super) struct ReachCache {
    tick: u32,
    goal: SimVec2,
    /// The radius the point was found for: the mesh answers per disc size.
    radius: SimScalar,
    owner: u8,
    reachable: SimVec2,
}

/// Every edit one actor's motion makes this tick, applied together in [`Self::commit`].
struct MotionProposal {
    position: SimVec2,
    velocity: SimVec2,
    /// The facing this motion asks for; the hull turns towards it within its rate on commit.
    heading: SimAngle,
    anim: AnimState,
    clear_route: bool,
    replacement: Option<Vec<SimVec2>>,
    remove_prefix: usize,
    pop_order: bool,
    retarget: Option<SimVec2>,
    clear_target: bool,
    memory: MovementMemory,
}

impl MotionProposal {
    fn new(actor: &Entity, policy: MotionPolicy) -> Self {
        let mut memory = actor.movement;
        memory.repath_cooldown = memory.repath_cooldown.saturating_sub(1);
        memory.abandoned_ticks = memory.abandoned_ticks.saturating_sub(1);
        if memory.abandoned_ticks == 0 {
            memory.abandoned_target = None;
        }
        Self {
            position: actor.position,
            velocity: actor.velocity,
            heading: actor.heading,
            anim: actor.current_anim,
            clear_route: policy.layer == MovementLayer::Air,
            replacement: None,
            remove_prefix: 0,
            pop_order: false,
            retarget: None,
            clear_target: false,
            memory,
        }
    }

    fn route<'a>(&'a self, actor: &'a Entity) -> &'a [SimVec2] {
        let route = match self.replacement.as_ref() {
            Some(route) => route.as_slice(),
            None if self.clear_route => &[],
            None => actor.current_funnel_waypoints.as_slice(),
        };
        &route[self.remove_prefix.min(route.len())..]
    }

    fn drop_route(&mut self) {
        self.clear_route = true;
        self.replacement = None;
        self.remove_prefix = 0;
        self.memory.corner_round = 0;
    }

    /// Consumes the route's first waypoint. The sense chosen to round a body parked on it is
    /// forgotten, unless the next waypoint lies within `reach` of it: the two are points of one
    /// arc round the same corner (plan Amendment 2), and the body is still to be rounded.
    fn pass_waypoint(&mut self, actor: &Entity, reach: SimScalar) {
        let same_arc = match *self.route(actor) {
            [passed, next, ..] => within_distance(passed, next, i128::from(reach.raw())),
            _ => false,
        };
        self.remove_prefix += 1;
        if !same_arc {
            self.memory.corner_round = 0;
        }
    }

    /// Ends a move order on arrival at `destination`.
    fn finish(&mut self, destination: SimVec2) {
        self.discard_order();
        self.anim = AnimState::Idle;
        self.memory.restart();
        self.memory.arrived_at = Some(destination);
    }

    fn discard_order(&mut self) {
        self.velocity = SimVec2::ZERO;
        self.pop_order = true;
        self.drop_route();
    }

    /// Ends an attack order whose target is gone or out of reach. The order beneath it, an
    /// attack-move the acquired target interrupted, starts afresh as after a new order: its
    /// progress is measured toward its own goal, not from the chase's best distance to the target,
    /// which a walk to a far goal never beats (B-112).
    fn end_attack(&mut self) {
        self.discard_order();
        self.memory.restart();
    }

    /// Ends an attack order on a target this unit cannot get to, and leaves that target alone.
    fn abandon_target(&mut self, target: EntityId) {
        self.discard_order();
        self.clear_target = true;
        self.memory.restart();
        self.memory.abandoned_target = Some(target);
        self.memory.abandoned_ticks = ABANDONED_TARGET_TICKS;
    }

    /// Installs a freshly searched route, dropping its first point if that is where we stand.
    fn adopt_route(
        &mut self,
        position: SimVec2,
        waypoints: Vec<SimVec2>,
    ) -> Result<(), MovementMathError> {
        self.clear_route = false;
        self.remove_prefix = 0;
        self.memory.corner_round = 0;
        if let Some(&first) = waypoints.first() {
            if distance(first, position)? <= ROUTE_START_TOLERANCE {
                self.remove_prefix = 1;
            }
        }
        self.replacement = Some(waypoints);
        Ok(())
    }

    /// Applies every edit to `actor`. A unit this motion translates turns its hull at the moving
    /// rate `turn_rate` (binary units per tick) this tick; any other keeps the stationary budget
    /// the phase started with (plan `2026-11-05-turn-before-move` D-10). Nothing spends the
    /// budget before this, the actor's one motion commit of the tick.
    fn commit(self, actor: &mut Entity, turn_rate: u16) {
        if self.position != actor.position {
            actor.hull_turn_budget = turn_rate;
        }
        actor.pos_at_tick_start = actor.position;
        actor.position = self.position;
        actor.velocity = self.velocity;
        actor.turn_hull_towards(self.heading);
        actor.current_anim = self.anim;
        actor.movement = self.memory;
        if let Some(route) = self.replacement {
            actor.current_funnel_waypoints = route;
        } else if self.clear_route {
            actor.current_funnel_waypoints.clear();
        }
        if self.remove_prefix > 0 {
            let count = self.remove_prefix.min(actor.current_funnel_waypoints.len());
            actor.current_funnel_waypoints.drain(..count);
        }
        if let Some(destination) = self.retarget {
            match actor.order_queue.front_mut() {
                Some(UnitOrder::Move { target } | UnitOrder::AttackMove { target }) => {
                    *target = destination;
                }
                // A walk to a lost target's last seen point keeps the reachable point it found.
                Some(UnitOrder::AttackTarget { target, .. } | UnitOrder::Follow { target, .. }) => {
                    if let Some(seen) = actor.movement.last_seen.as_mut() {
                        if seen.target == *target {
                            seen.point = destination;
                        }
                    }
                }
                _ => {}
            }
            if actor.has_task_order() {
                actor.task.goal = Some(destination);
            }
        }
        if self.clear_target {
            actor.current_target = None;
        }
        if self.pop_order {
            // Movement ends a task's leg, never the task: the task decides what comes next.
            if actor.has_task_order() {
                actor.task.goal = None;
            } else {
                actor.order_queue.pop_front();
            }
        }
    }
}

pub(crate) type CastRequest = (EntityId, u8, Option<SimVec2>, Option<EntityId>);

#[expect(
    clippy::large_enum_variant,
    reason = "a proposal is built and moved once per actor per tick; boxing it would allocate there"
)]
enum MotionAction {
    Commit(MotionProposal),
    Pursue(EntityId, AttackOrigin),
    Cast(CastRequest),
    /// The unit follows its front order's target ([`follow`]); prepared in entity order.
    Follow(EntityId),
}

/// The result of asking for a route.
enum RouteSearch {
    /// The goal can be reached.
    Found(PathResult),
    /// The goal cannot be reached; this route ends at the nearest point that can.
    Retargeted(SimVec2, PathResult),
    /// Nothing nearer can be reached from here.
    Blocked,
}

/// Everything the step toward a waypoint reads.
struct StepContext<'a> {
    map: &'a SimMap,
    policy: MotionPolicy,
    speed: SimScalar,
    body: ContactBody,
    neighbours: &'a [SteeringObstacle],
    /// The facing a `turn_before_move` unit must reach before it steps; `None` for every other
    /// unit, whose steps are never gated by facing.
    hull: Option<HullGate>,
    /// For a unit that may walk round a crowd blocking it ([`surround_walk`]), the target it
    /// approaches (`None` for an attack-move without one); `None` for every other unit.
    #[expect(
        clippy::option_option,
        reason = "whether the unit walks, then the target it walks for, which an attack-move lacks"
    )]
    surround: Option<Option<EntityId>>,
    /// The unit's auto-acquisition scan range in world units, edge to edge.
    scan_range: SimScalar,
    /// Under the solver pipeline, the side rule's horizon factor (`contact.side_horizon_factor`,
    /// plan `2026-11-20-contact-solver` I-3a): the step steers by intent and contact is the
    /// solver's. `None` under the legacy pipeline.
    intent: Option<u8>,
    /// The near reach's margin beyond the radius sum, world units ([`near_margin`]): a
    /// neighbour beyond it was listed without its ground test ([`Self::admits`]).
    near_margin: SimScalar,
}

impl StepContext<'_> {
    /// Whether `obstacle`, one of [`Self::neighbours`], lies within the near reach: the reach
    /// every rule but the side rule reads (the legacy pipeline's whole reach).
    fn is_near(&self, obstacle: &SteeringObstacle) -> bool {
        let radii = i128::from(self.body.radius.raw()) + i128::from(obstacle.radius.raw());
        within_distance(
            self.body.pos,
            obstacle.pos,
            radii + i128::from(self.near_margin.raw()),
        )
    }

    /// Whether `obstacle`, one of [`Self::neighbours`], is a neighbour by the ground test that
    /// [`gather_neighbours`] defers for bodies beyond the near reach: always for an air unit or a
    /// near body, else when the ground between the two is connected. Only the side rule reads
    /// bodies beyond the near reach.
    fn admits(&self, obstacle: &SteeringObstacle) -> bool {
        self.policy.layer == MovementLayer::Air
            || self.is_near(obstacle)
            || on_connected_ground(self.map, self.body.pos, obstacle.pos)
    }
}

/// What gates a `turn_before_move` unit's step: its tick-start heading and its moving hull rate.
#[derive(Clone, Copy)]
struct HullGate {
    /// The hull's facing at the start of the tick.
    heading: SimAngle,
    /// The hull rate of a tick on which the unit translates (`turn_rate`, not the stationary
    /// rate the tick's budget starts at), in binary angle units per tick.
    rate: u16,
}

/// Whether a hull facing `heading` can face `direction` within `rate` binary angle units, so a
/// `turn_before_move` unit may take a step in that direction this tick.
///
/// The boundary is inclusive: a remaining turn equal to the rate fits (Q0050: rotate in place
/// only when the angle exceeds one tick's turn). A rate of half a turn (32 768) allows any step.
fn hull_allows_step(heading: SimAngle, direction: SimAngle, rate: u16) -> bool {
    heading.diff(direction).unsigned_abs() <= rate
}

/// Applies a `turn_before_move` unit's hull gate to the step `proposal` holds from `origin`.
///
/// The hull is asked to face the step's velocity. When that facing lies beyond one tick's turn,
/// the step is voided: the unit stays at `origin` with zero velocity and turns in place, still
/// walking. Every step-derived edit made after this reads the unmoved position and does nothing.
fn gate_step_by_facing(hull: Option<HullGate>, origin: SimVec2, proposal: &mut MotionProposal) {
    let Some(gate) = hull else {
        return;
    };
    let step = proposal.velocity;
    if step == SimVec2::ZERO {
        return;
    }
    let direction = SimAngle::atan2(step.y, step.x);
    proposal.heading = direction;
    if !hull_allows_step(gate.heading, direction, gate.rate) {
        proposal.position = origin;
        proposal.velocity = SimVec2::ZERO;
        proposal.anim = AnimState::Walk;
    }
}

/// `point` moved, if need be, into the terrain-radius domain of `policy`'s body on `map`.
fn clamp_into_domain(policy: MotionPolicy, map: &SimMap, point: SimVec2) -> SimVec2 {
    let r = policy.terrain_radius();
    let clamp = |value: SimScalar, extent: u32| {
        let high = SimScalar::from_num(extent).saturating_sub(r);
        if high < r {
            SimScalar::from_num(extent).saturating_mul(SimScalar::HALF)
        } else {
            value.clamp(r, high)
        }
    };
    SimVec2::new(clamp(point.x, map.width()), clamp(point.y, map.height()))
}

/// The target `actor` fights: its current target, or else the target of its attack order. A
/// target lost to fog, which the unit walks away from to its last seen point, is none (plan
/// `2026-11-24-weapon-validation` Amendment 2): the unit is a walker, not engaged.
pub(super) fn engaged_target(actor: &Entity) -> Option<EntityId> {
    actor
        .current_target
        .or(match actor.order_queue.front() {
            Some(UnitOrder::AttackTarget { target, .. }) => Some(*target),
            _ => None,
        })
        .filter(|&id| actor.movement.lost_target_point(id).is_none())
}

fn arithmetic(id: EntityId, source: MovementMathError) -> MovementInvariant {
    MovementInvariant::Arithmetic {
        entity: id,
        operation: "prepare locomotion",
        source,
    }
}

/// Whether two ground points lie on terrain that connects them directly.
pub(super) fn on_connected_ground(map: &SimMap, a: SimVec2, b: SimVec2) -> bool {
    map.world_to_cell(a).ok() == map.world_to_cell(b).ok()
        || map.ground_segment_is_connected(a, b, |_, _| true)
}

/// The tick-start bodies one actor reads its neighbours from.
struct NeighbourIndex<'a> {
    grid: &'a BodyGrid,
    obstacles: &'a [SteeringObstacle],
    largest_radius: SimScalar,
}

/// The margin beyond the radius sum within which a body is a neighbour under the legacy
/// pipeline, world units: the steering lookahead and two steps at `entry`'s speed. Under the
/// solver pipeline it is the **near** reach, inside which neighbours are read by every rule.
fn near_margin(entry: MotionEntry) -> SimScalar {
    STEERING_LOOKAHEAD + entry.speed + entry.speed
}

/// Collects the bodies that can affect `entry` this tick from the tick-start snapshot.
///
/// Bodies across a cliff are left out: ground units cannot touch through terrain. Under the
/// solver pipeline (`horizon` set) the reach adds the side rule's horizon, `h · (r + r_b)` world
/// units for the largest `r_b` (plan `2026-11-20-contact-solver` WP3, I-3a); a body beyond the
/// near reach ([`near_margin`]) is listed without its ground test, which
/// [`StepContext::admits`] makes for the side rule, the only rule that reads such far bodies;
/// every other rule reads the near reach alone ([`StepContext::is_near`]).
// PERFORMANCE RATIONALE:
// Why: Scanning every body per unit is O(N^2): 160,000 pair tests per tick at 400 units. Under
// the solver the horizon widens the reach about fivefold in area, and the ground test of each
// far body walks the cells between the two (evidence §5.3: the gather tripled its share).
// What: Read candidates from a grid built once at the start of the phase, and copy the few that
// pass an exact distance test into one retained buffer. A far body's ground test is deferred to
// the rule that can act on it: the side rule, for a body that is a threat or the committed
// partner (plan Amendment 3, item 1, option 4).
// Trade-off: Copies ~56-byte records per neighbour, in exchange for flat scans downstream and
// O(N) work per tick; `out` stops growing once the densest crowd has been seen. Every rule that
// reads far bodies must ask [`StepContext::admits`]; the rules that do not read them are bounded
// by the near reach.
fn gather_neighbours(
    index: &NeighbourIndex<'_>,
    map: &SimMap,
    (entry, position): (MotionEntry, SimVec2),
    horizon: Option<u8>,
    out: &mut Vec<SteeringObstacle>,
) {
    out.clear();
    let horizon_reach = horizon.map_or(SimScalar::ZERO, |factor| {
        (entry.policy.radius + index.largest_radius).saturating_mul(SimScalar::from_num(factor))
    });
    let near = near_margin(entry);
    let margin = near + horizon_reach;
    let reach = entry.policy.radius + index.largest_radius + margin;
    index.grid.for_each_near(position, reach, |body| {
        let obstacle = &index.obstacles[body];
        if obstacle.id == entry.id {
            return;
        }
        let radii = i128::from(entry.policy.radius.raw()) + i128::from(obstacle.radius.raw());
        if !within_distance(position, obstacle.pos, radii + i128::from(margin.raw())) {
            return;
        }
        let deferred = horizon_reach > SimScalar::ZERO
            && !within_distance(position, obstacle.pos, radii + i128::from(near.raw()));
        if entry.policy.layer == MovementLayer::Air
            || deferred
            || on_connected_ground(map, position, obstacle.pos)
        {
            out.push(*obstacle);
        }
    });
}

/// Moves `from` as far toward `to` as terrain allows, sliding along the larger axis first.
///
/// A step refused at a corner keeps most of its value along its dominant axis, while the other
/// axis can walk a unit along a wall and away from the way through.
pub(super) fn slide_target(
    map: &SimMap,
    policy: MotionPolicy,
    from: SimVec2,
    to: SimVec2,
) -> Option<SimVec2> {
    if policy.permits_segment(map, from, to) {
        return Some(to);
    }
    let along_x = SimVec2::new(to.x, from.y);
    let along_y = SimVec2::new(from.x, to.y);
    let x_dominant = to.x.raw().abs_diff(from.x.raw()) >= to.y.raw().abs_diff(from.y.raw());
    let (first, second) = if x_dominant {
        (along_x, along_y)
    } else {
        (along_y, along_x)
    };
    if policy.permits_segment(map, from, first) {
        Some(first)
    } else if policy.permits_segment(map, from, second) {
        Some(second)
    } else {
        None
    }
}

/// Runs one route search, escalating from the exact goal to the nearest reachable point.
fn search_route(
    pathfinder: &mut Pathfinder,
    map: &SimMap,
    request: &PathRequestWithVision<'_>,
    cache: &mut Option<ReachCache>,
    tick: u32,
    owner: u8,
) -> RouteSearch {
    // Physics only tests a unit's centre, so a body can stand where its disc does not fit:
    // spawned in a gap, or pushed into one. The mesh lets it leave (plan D-3: the start triangle
    // is entered whatever its width), so no narrower retry is needed.
    let request = *request;
    match pathfinder.find_path_with_vision(map, &request) {
        Ok(path) => RouteSearch::Found(path),
        Err(MapError::InvalidGoal(_) | MapError::NoPathFound { .. }) => {
            let cached = cache
                .filter(|c| {
                    (c.tick, c.goal, c.radius, c.owner)
                        == (tick, request.goal, request.radius, owner)
                })
                .map(|c| c.reachable);
            let reachable = match cached {
                Some(point) => Some(point),
                None => pathfinder
                    .nearest_reachable_point(map, &request)
                    .ok()
                    .flatten(),
            };
            let Some(reachable) = reachable else {
                return RouteSearch::Blocked;
            };
            *cache = Some(ReachCache {
                tick,
                goal: request.goal,
                radius: request.radius,
                owner,
                reachable,
            });
            let retargeted = PathRequestWithVision {
                goal: reachable,
                ..request
            };
            pathfinder
                .find_path_with_vision(map, &retargeted)
                .map_or(RouteSearch::Blocked, |path| {
                    RouteSearch::Retargeted(reachable, path)
                })
        }
        Err(_) => RouteSearch::Blocked,
    }
}

/// The route query for `actor`, seeing the map through the owner's `knowledge`, if any.
fn route_request<'a>(
    actor: &Entity,
    policy: MotionPolicy,
    goal: SimVec2,
    knowledge: Option<(&'a PlayerVisionMap, &'a Vec<bool>)>,
) -> PathRequestWithVision<'a> {
    PathRequestWithVision {
        start: actor.position,
        goal,
        radius: policy.terrain_radius(),
        vision: knowledge.map(|(vision, _)| vision),
        block_unexplored_ramps: false,
        known_structures: knowledge.map(|(_, known)| known.as_slice()),
    }
}

/// Whether a stored route still works from `position`.
///
/// The first leg asks only whether the unit can physically walk it. The remaining legs ask the
/// mesh predicate whether the unit's disc fits along them. A body pushed into a narrow gap has a
/// first leg its disc would not fit, and that must not cost it the route it is still able to
/// follow.
fn route_is_usable(
    map: &SimMap,
    pathfinder: &mut Pathfinder,
    request: &PathRequestWithVision<'_>,
    policy: MotionPolicy,
    position: SimVec2,
    route: &[SimVec2],
) -> bool {
    let Some(&first) = route.first() else {
        return false;
    };
    if !policy.permits_segment(map, position, first) {
        return false;
    }
    // Legs from waypoints where the disc does not fit, up to the first where it does, lead a
    // wedged body out (plan D-3); they are held to the terrain only, as the search held them.
    let mut wedged = true;
    route.windows(2).all(|leg| {
        if wedged {
            wedged = !pathfinder.segment_is_passable(map, request, leg[0], leg[0]);
        }
        if wedged {
            policy.permits_segment(map, leg[0], leg[1])
        } else {
            pathfinder.segment_is_passable(map, request, leg[0], leg[1])
        }
    })
}

/// Records how much of its route the unit still has to walk, returning how long it has not
/// gained ground.
///
/// Remaining length, not distance to the next waypoint, because a unit can pop waypoints and be
/// handed them back forever without getting anywhere. Only real ground gained clears the count.
fn note_progress(
    memory: &mut MovementMemory,
    position: SimVec2,
    route: &[SimVec2],
    goal: SimVec2,
) -> Result<u16, MovementMathError> {
    let legs = u16::try_from(route.len()).unwrap_or(u16::MAX);
    if memory.progress_legs != legs {
        memory.progress_legs = legs;
        memory.progress_tail = SimScalar::ZERO;
        for leg in route.windows(2) {
            memory.progress_tail = memory
                .progress_tail
                .checked_add(distance(leg[0], leg[1])?)
                .ok_or(MovementMathError::Unrepresentable)?;
        }
    }
    let next = route.first().copied().unwrap_or(goal);
    let remaining = distance(position, next)?
        .checked_add(memory.progress_tail)
        .ok_or(MovementMathError::Unrepresentable)?;
    let gained = memory.progress_best.is_none_or(|best| {
        remaining
            .checked_add(STALL_PROGRESS)
            .is_some_and(|required| required <= best)
    });
    if gained {
        memory.progress_best = Some(remaining);
        memory.stalled_ticks = 0;
    } else {
        memory.stalled_ticks = memory.stalled_ticks.saturating_add(1);
    }
    Ok(memory.stalled_ticks)
}

/// [`note_progress`] for a unit that may be queued: on a `queued` tick that gained nothing, the
/// stall count stays where it was and the best distance is kept (plan
/// `2026-11-19-crowd-edge-steering` D-4). A tick that gains ground re-arms a spent surround walk
/// on the side it last walked: the crowd in front of the unit has moved (plan
/// `2026-11-19-crowd-edge-steering-2` D-2). Returns the stall count.
fn note_queued_progress(
    memory: &mut MovementMemory,
    position: SimVec2,
    route: &[SimVec2],
    goal: SimVec2,
    queued: bool,
) -> Result<u16, MovementMathError> {
    let before = memory.stalled_ticks;
    let gained = note_progress(memory, position, route, goal)? == 0;
    if gained && memory.flank_spent {
        end_surround_walk(memory);
    }
    if queued && !gained {
        memory.stalled_ticks = before;
    }
    Ok(memory.stalled_ticks)
}

impl SimWorld {
    pub(crate) fn motion_entry(&self, id: EntityId) -> Result<MotionEntry, MovementInvariant> {
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "movement actor is absent",
            })?;
        let policy = MotionPolicy::resolve(&self.catalog, &self.map, actor)?;
        let foundation = self.map.structures().contains_key(&id);
        if foundation && !actor.is_immobile {
            return Err(MovementInvariant::InvalidState {
                entity: id,
                reason: "registered foundation cannot author self-motion without atomic relocation",
            });
        }
        if !policy.contains(&self.map, actor.position) {
            return Err(MovementInvariant::InvalidState {
                entity: id,
                reason: "actor is outside its current terrain-radius movement domain",
            });
        }
        if !actor.is_immobile {
            policy.validate_origin(&self.map, actor)?;
        }
        let arch = self
            .catalog
            .get(actor.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(id))?;
        let speed = self
            .effective_max_speed(arch, actor.owner)
            .and_then(|base| {
                base.checked_mul(actor.speed_multiplier()?)
                    .ok_or(MovementMathError::Unrepresentable)
            })
            .map(|speed| actor.status_modifiers().move_speed(speed))
            .map_err(|e| arithmetic(id, e))?;
        let step = accelerated_speed(speed, arch.acceleration, arch.max_speed, actor.velocity);
        let deceleration = (arch.deceleration < arch.max_speed).then_some(arch.deceleration);
        Ok(MotionEntry {
            id,
            policy,
            speed,
            step,
            deceleration,
            push_priority: arch.push_priority,
            turn_rate: arch.turn_rate.raw(),
            turn_before_move: arch.turn_before_move,
            surround_walk: arch.surround_walk,
            scan_range: arch.scan_range,
            foundation,
        })
    }

    /// Whether this unit stands fighting, so friendly traffic should flow around it.
    pub(crate) fn is_anchored_in_combat(&self, actor: &Entity) -> bool {
        let front_attacks = || {
            matches!(
                actor.order_queue.front(),
                Some(UnitOrder::AttackTarget { .. } | UnitOrder::AttackMove { .. })
            )
        };
        let target_lives = || {
            engaged_target(actor)
                .is_some_and(|id| self.entities.get(&id).is_some_and(Entity::is_alive))
        };
        matches!(
            actor.weapon_phase,
            WeaponPhase::RotatingToTarget { .. }
                | WeaponPhase::Windup { .. }
                | WeaponPhase::Backswing { .. }
        ) || (actor.velocity == SimVec2::ZERO && front_attacks() && target_lives())
    }

    /// Whether `actor` stands engaged, so friendly traffic does not push it: under the legacy
    /// pipeline [`Self::is_anchored_in_combat`]; under the solver pipeline, in range of a live
    /// target it attacks (plan `2026-11-20-contact-solver` Amendment 7, item 1). With hold,
    /// immobile and unpushable this is the one definition of anchored, and under the solver
    /// only contact reads it: the contact shares (C-2) and the idle-yield test of intent, which
    /// asks whether contact would push the body aside.
    pub(crate) fn engaged(&self, entry: MotionEntry, actor: &Entity) -> bool {
        if self
            .catalog
            .movement_rules()
            .contact()
            .pipeline
            .runs_solver()
        {
            self.in_range_of_live_target(entry, actor)
        } else {
            self.is_anchored_in_combat(actor)
        }
    }

    /// Whether `actor` stands within its weapon's band of a live target: its current target, or
    /// the target of its attack order.
    fn in_range_of_live_target(&self, entry: MotionEntry, actor: &Entity) -> bool {
        let Some((id, target)) = engaged_target(actor).and_then(|id| {
            self.entities
                .get(&id)
                .filter(|t| t.is_alive())
                .map(|t| (id, t))
        }) else {
            return false;
        };
        let Ok(Some(band)) = self.pursuit_band(entry, actor, target, id, AttackOrigin::Explicit)
        else {
            return false;
        };
        distance(actor.position, target.position)
            .map(|d| i128::from(d.raw()))
            .is_ok_and(|d| d <= band.max && d >= band.min)
    }

    /// Deviation D-δ (Q0095, plan Amendment 15, N): when the catalog's
    /// `contact.nudge_engaged_along_ring` is on, the unit vector from `actor`'s target to it if
    /// `actor` is a melee attacker in range of a live target that allies may nudge along its
    /// ring: never on Hold Position (nor immobile or unpushable), and only under the `Solver`
    /// pipeline: under `Gated` no ally ever nudges an attacking unit (C′-1 of plan
    /// `2026-11-27-gated-movement`, Q0121 (a)), whatever the switch says. `None` otherwise.
    pub(crate) fn nudge_radial(&self, entry: MotionEntry, actor: &Entity) -> Option<SimVec2> {
        let rules = self.catalog.movement_rules().contact();
        if !rules.nudge_engaged_along_ring
            || rules.pipeline != MovementPipeline::Solver
            || actor.cannot_be_pushed()
            || !self.in_range_of_live_target(entry, actor)
        {
            return None;
        }
        let target = engaged_target(actor).and_then(|id| self.entities.get(&id))?;
        let arch = self.catalog.get(actor.unit_type_id).ok()?;
        let target_arch = self.catalog.get(target.unit_type_id).ok()?;
        let siege = actor
            .is_sieged
            .then_some(actor.siege_weapon.as_deref())
            .flatten();
        let melee = arch
            .weapon_against(target_arch.mover_type, siege)
            .is_some_and(|weapon| weapon.melee);
        if !melee {
            return None;
        }
        let radial = direction(actor.position, target.position).ok()?;
        (radial != SimVec2::ZERO).then_some(radial)
    }

    fn obstacle_of(&self, entry: MotionEntry, actor: &Entity) -> SteeringObstacle {
        let idle = matches!(actor.order_queue.front(), None | Some(UnitOrder::Idle));
        SteeringObstacle {
            id: entry.id,
            pos: actor.position,
            radius: entry.policy.radius,
            velocity: actor.velocity,
            owner: actor.owner,
            push_priority: entry.push_priority,
            anchored: actor.cannot_be_pushed() || self.engaged(entry, actor),
            idle: actor.stands_idle(),
            settled_on: actor.movement.arrived_at.filter(|_| idle),
            resting_on: self.resting_on(actor),
        }
    }

    /// Validates this tick's actors and rebuilds the snapshot every actor steers against.
    fn prepare_movement_snapshot(&mut self, entries: &mut Vec<MotionEntry>) {
        let mut ids = std::mem::take(&mut self.movement.phase_ids);
        ids.clear();
        ids.extend(
            self.entities
                .values()
                .filter(|e| e.takes_part_in_movement())
                .map(|e| e.id),
        );
        // One steering list, grid and largest radius per collision class of the catalog (plan
        // `2026-11-25-collision-classes` D-3); the catalog is fixed for a world, so the tables
        // are sized once and only cleared afterwards.
        let classes = self.catalog.collision_class_count();
        self.movement.obstacles.resize_with(classes, Vec::new);
        self.movement.grids.resize_with(classes, Default::default);
        self.movement.largest_radius.clear();
        self.movement
            .largest_radius
            .resize(classes, SimScalar::ZERO);
        for class in &mut self.movement.obstacles {
            class.clear();
        }
        // `ids` follow the entity table's order, so the record is sorted by id.
        self.movement.tick_start.clear();
        self.movement.attacking_at_start.clear();
        let gated = self.catalog.movement_rules().contact().pipeline == MovementPipeline::Gated;
        self.movement.ring_cache_used = 0;
        self.movement.ring_tagged.clear();
        for &id in &ids {
            if let Some(actor) = self.entities.get(&id) {
                self.movement
                    .tick_start
                    .push((id, actor.position, actor.velocity));
            }
            match self.motion_entry(id) {
                Ok(entry) => {
                    // A body without collision (hidden, mining, or a status's suppression) is
                    // no obstacle anyone steers around, and a registered structure is no body:
                    // units meet it through its footprint, as they meet terrain (D-2 B2).
                    if let Some(actor) = self
                        .entities
                        .get(&id)
                        .filter(|a| !entry.foundation && !a.passes_through_units())
                    {
                        let obstacle = self.obstacle_of(entry, actor);
                        // C′-1 (Amendment 3, item 1): under `Gated`, "attacking" is read here,
                        // from the snapshot, once per tick, before any body moves.
                        if gated && obstacle.anchored && !actor.cannot_be_pushed() {
                            self.movement.attacking_at_start.push(id);
                        }
                        let class = entry.policy.class.index();
                        self.movement.obstacles[class].push(obstacle);
                        self.movement.largest_radius[class] =
                            self.movement.largest_radius[class].max(entry.policy.radius);
                    }
                    entries.push(entry);
                }
                Err(error) => self.report_movement_invariant(error),
            }
        }
        self.movement.phase_ids = ids;
        let (width, height) = (self.map.width(), self.map.height());
        for class in 0..classes {
            let positions = self.movement.obstacles[class].iter().map(|o| o.pos);
            self.movement.grids[class].rebuild(width, height, positions);
        }
        let navigation = self.pathfinder.nav_version();
        if navigation != self.movement.seen_nav_version {
            self.movement.seen_nav_version = navigation;
            self.movement.recheck_routes = PerPlayer::from_array([true; MAX_PLAYERS]);
        }
    }

    pub(crate) fn advance_checked_movement(&mut self) {
        // PERFORMANCE RATIONALE:
        // Why: Proposals must not clone an Entity or allocate an ID vector per actor.
        // What: Rebuild retained validated entries/obstacles once and borrow each
        // actor while preparing compact scalar edits and optional new path ownership.
        // Trade-off: Buff/radius validation is phase-entry work; scratch capacities
        // grow at population high-water marks, while pathfinding retains its costs.
        self.reconcile_route_groups();
        self.compute_group_lanes();
        let mut entries = std::mem::take(&mut self.movement.entries);
        entries.clear();
        let mut pursuits = std::mem::take(&mut self.movement.pursuits);
        pursuits.clear();
        let mut casts = std::mem::take(&mut self.movement.casts);
        casts.clear();
        self.prepare_movement_snapshot(&mut entries);
        self.movement.proposed_steps.clear();
        self.movement.probes = super::MovementProbes::default();
        self.movement.enemy_pushes.clear();
        // The determinism probe (`set_movement_order_rotation`): the visit starts `offset` places
        // in and wraps round; offset 0 visits in entity order, as shipped.
        for &entry in super::rotated(&entries, self.movement.order_rotation) {
            match self.prepare_actor_motion(entry) {
                Ok(MotionAction::Commit(proposal)) => self.commit_motion(entry, proposal),
                Ok(MotionAction::Pursue(target, origin)) => {
                    pursuits.push((entry, target, origin));
                }
                Ok(MotionAction::Cast(cast)) => casts.push((entry, cast)),
                Ok(MotionAction::Follow(target)) => match self.follow_motion(entry, target) {
                    Ok(proposal) => self.commit_motion(entry, proposal),
                    Err(error) => self.report_movement_invariant(error),
                },
                Err(error) => self.report_movement_invariant(error),
            }
        }
        for (entry, (_, ability, pos, target)) in casts.drain(..) {
            let Some(origin) = self.entities.get(&entry.id).map(|actor| actor.position) else {
                self.report_movement_invariant(MovementInvariant::InvalidState {
                    entity: entry.id,
                    reason: "prepared cast actor disappeared",
                });
                continue;
            };
            match self.try_execute_ability_prevalidated(entry.id, ability, pos, target) {
                Ok(_) => {
                    if let Some(actor) = self.entities.get_mut(&entry.id) {
                        actor.pos_at_tick_start = origin;
                        if entry.policy.layer == MovementLayer::Air {
                            actor.current_funnel_waypoints.clear();
                        }
                        actor.order_queue.pop_front();
                    }
                }
                Err(source) => self.report_movement_invariant(source),
            }
        }
        for (entry, target, origin) in pursuits.drain(..) {
            match self.prepare_pursuit(entry, target, origin) {
                Ok(proposal) => self.commit_motion(entry, proposal),
                Err(error) => self.report_movement_invariant(error),
            }
        }
        // PERFORMANCE RATIONALE:
        // Why: The reversal meter looks up each unit's proposed step once per tick, and a map
        // keyed by entity would allocate a node per mover every tick.
        // What: Keep the steps in one retained vector, walks and pursuits appended in entity
        // order each, and sort it once by entity so lookups are binary searches.
        // Trade-off: An O(N log N) sort of an almost sorted vector per tick, for no allocation
        // in steady state and O(log N) reads.
        self.movement
            .proposed_steps
            .sort_unstable_by_key(|&(id, _)| id);
        self.movement.entries = entries;
        self.movement.entries_are_fresh = true;
        self.movement.pursuits = pursuits;
        self.movement.casts = casts;
        self.movement.recheck_routes = PerPlayer::from_array([false; MAX_PLAYERS]);
    }

    /// Settles `proposal` on the terrain, records its step as this tick's proposed step of
    /// `entry` ([`SimWorld::proposed_step`]) and commits it to the actor.
    fn commit_motion(&mut self, entry: MotionEntry, mut proposal: MotionProposal) {
        self.settle_proposal(entry, &mut proposal);
        if let Some(actor) = self.entities.get_mut(&entry.id) {
            self.movement
                .proposed_steps
                .push((entry.id, proposal.velocity));
            proposal.commit(actor, entry.turn_rate);
        }
    }

    /// Settles the end of a walking proposal on the terrain before it is committed (plan
    /// `2026-11-11-contact-and-missiles` D-3, B-113): whatever steering mode produced it, the
    /// step may not press the body's disc deeper into a wall. A step that would slides along the
    /// wall, or the body stays where it is, as a refused step does.
    fn settle_proposal(&mut self, entry: MotionEntry, proposal: &mut MotionProposal) {
        let Some(from) = self.entities.get(&entry.id).map(|actor| actor.position) else {
            return;
        };
        match settle_on_terrain(
            &self.map,
            &mut self.pathfinder,
            entry.policy,
            from,
            from,
            proposal.position,
        ) {
            Ok(Some(end)) => proposal.position = end,
            Ok(None) => proposal.position = from,
            Err(source) => {
                proposal.position = from;
                self.report_movement_invariant(MovementInvariant::Arithmetic {
                    entity: entry.id,
                    operation: "wall contact",
                    source,
                });
            }
        }
    }

    /// Whether this tick is one on which `id`'s route gets looked at.
    fn route_check_due(&self, id: EntityId, owner: PlayerSlot) -> bool {
        self.movement.recheck_routes[owner]
            || self
                .current_tick
                .wrapping_add(id.0)
                .is_multiple_of(ROUTE_CHECK_PERIOD)
    }

    /// Prepares one actor's motion for this tick: a committed proposal, or a pursuit or cast to
    /// run after every walking actor has been prepared.
    ///
    /// Walking runs in stages over one [`MotionDraft`]: route upkeep, the arrival decision, the
    /// progress check and the step. A stage that settles the order ends the walk early.
    fn prepare_actor_motion(
        &mut self,
        entry: MotionEntry,
    ) -> Result<MotionAction, MovementInvariant> {
        let id = entry.id;
        let policy = entry.policy;
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "validated actor disappeared",
            })?;
        let proposal = MotionProposal::new(actor, policy);
        let (proposal, target) = match self.walking_goal(entry, actor, proposal)? {
            ControlFlow::Continue(walk) => walk,
            ControlFlow::Break(action) => return Ok(action),
        };
        if !policy.contains(&self.map, target) {
            return Err(MovementInvariant::InvalidState {
                entity: id,
                reason: "active movement order is outside the current terrain-radius domain",
            });
        }
        // `MotionEntry` creation already rejected owners without a slot (`effective_max_speed`).
        let owner = actor
            .owner
            .slot()
            .ok_or_else(|| arithmetic(id, MovementMathError::InvalidOwner(actor.owner)))?;
        // Fresh knowledge of the map cannot wait out a cooldown: the route may lead into a wall.
        let map_news = self.movement.recheck_routes[owner];
        let cooled_down = proposal.memory.repath_cooldown == 0 || map_news;
        let check_route = self.route_check_due(id, owner);
        let knowledge = self
            .vision_aware_pathfinding
            .then_some((&self.vision_maps[owner], &self.known_blocked_cells[owner]));
        let mut draft = MotionDraft {
            entry,
            actor,
            proposal,
            goal: target,
            request: route_request(actor, policy, target, knowledge),
            tick: Tick(self.current_tick),
            ground: policy.layer == MovementLayer::Ground,
            cooled_down,
        };
        let mut scene = MotionScene {
            map: &self.map,
            pathfinder: &mut self.pathfinder,
            movement: &mut self.movement,
            current_tick: self.current_tick,
            route_groups: &self.route_groups,
            lane_tolerance: self.catalog.movement_rules().lane_tolerance_permille(),
            contact: self.catalog.movement_rules().contact(),
        };
        let mut neighbours = std::mem::take(&mut scene.movement.neighbours);
        let walked = draft.walk(&mut scene, &mut neighbours, check_route, map_news);
        scene.movement.neighbours = neighbours;
        walked?;
        Ok(MotionAction::Commit(draft.proposal))
    }

    /// Settles an actor that does not walk this tick, or finds the point its order walks to.
    ///
    /// Breaks with the finished action for a unit that holds, winds up a weapon that does not fire
    /// while moving ([`Self::wind_up_holds`]), pursues, casts or has nothing to do. Continues with
    /// the proposal and the goal in world units.
    fn walking_goal(
        &self,
        entry: MotionEntry,
        actor: &Entity,
        mut proposal: MotionProposal,
    ) -> Result<ControlFlow<MotionAction, (MotionProposal, SimVec2)>, MovementInvariant> {
        let id = entry.id;
        if actor.is_action_locked() {
            // Stunned or locked after a cast (plan `2026-11-10-spellcaster-mechanics` D-7.4):
            // the unit stands, keeping its orders for when the lock ends.
            proposal.clear_route = false;
            proposal.velocity = SimVec2::ZERO;
            if proposal.anim == AnimState::Walk {
                proposal.anim = AnimState::Idle;
            }
            return Ok(ControlFlow::Break(MotionAction::Commit(proposal)));
        }
        if actor.is_on_hold() || actor.is_immobile || actor.channeling_remaining_ticks > 0 {
            // A cast queued behind an anchored unit's orders runs when it reaches the front
            // (plan `2026-11-09-client-mode-commands` D-6b): a sieged tank told to focus and
            // then, with Shift, to unsiege, unsieges once the target is gone. A channelling unit
            // finishes its channel first, as the cast's own admission requires.
            if actor.channeling_remaining_ticks == 0 {
                if let Some(&UnitOrder::CastAbility {
                    ability_index,
                    target_pos,
                    target_entity,
                }) = actor.order_queue.front()
                {
                    return Ok(ControlFlow::Break(MotionAction::Cast((
                        id,
                        ability_index,
                        target_pos,
                        target_entity,
                    ))));
                }
            }
            proposal.clear_route = false;
            if actor.is_immobile || actor.channeling_remaining_ticks > 0 {
                proposal.velocity = SimVec2::ZERO;
            } else {
                self.coast(entry, actor, &mut proposal)?;
            }
            return Ok(ControlFlow::Break(MotionAction::Commit(proposal)));
        }
        if self.wind_up_holds(actor)? {
            proposal.velocity = SimVec2::ZERO;
            return Ok(ControlFlow::Break(MotionAction::Commit(proposal)));
        }
        let target = match actor.order_queue.front() {
            Some(UnitOrder::Move { target } | UnitOrder::AttackMove { target }) => *target,
            Some(_) if actor.has_task_order() && actor.task.goal.is_some() => {
                actor.task.goal.unwrap_or(actor.position)
            }
            Some(&UnitOrder::AttackTarget { target, origin }) => {
                match self.last_seen_goal(entry, actor, target, origin) {
                    Some(goal) => goal,
                    None => return Ok(ControlFlow::Break(MotionAction::Pursue(target, origin))),
                }
            }
            // A follow walks by its own rules (plan `2026-11-26-follow-order` D-3), unless its
            // enemy target is lost to fog: then it walks, as a move, to where the target was
            // last seen, and ends there (D-5).
            Some(&UnitOrder::Follow { target, .. }) => {
                match self.follow_lost_goal(entry, actor, target) {
                    Some(goal) => goal,
                    None => return Ok(ControlFlow::Break(MotionAction::Follow(target))),
                }
            }
            Some(&UnitOrder::CastAbility {
                ability_index,
                target_pos,
                target_entity,
            }) => {
                // An effect ability out of range walks toward its target first (plan
                // `2026-11-10-spellcaster-mechanics` D-7.1); the goal stays inside the domain.
                match self.cast_approach_goal(actor, ability_index, target_pos, target_entity) {
                    Some(goal) => clamp_into_domain(entry.policy, &self.map, goal),
                    None => {
                        return Ok(ControlFlow::Break(MotionAction::Cast((
                            id,
                            ability_index,
                            target_pos,
                            target_entity,
                        ))))
                    }
                }
            }
            _ => {
                self.coast(entry, actor, &mut proposal)?;
                if proposal.velocity == SimVec2::ZERO && proposal.anim == AnimState::Walk {
                    proposal.anim = AnimState::Idle;
                }
                return Ok(ControlFlow::Break(MotionAction::Commit(proposal)));
            }
        };
        Ok(ControlFlow::Continue((proposal, target)))
    }

    /// The point a unit under its player's attack order walks to instead of pursuing `target`:
    /// where its player last saw the target, once the target is lost to fog and while pursuit
    /// would still chase it (plan `2026-11-24-weapon-validation` Amendment 2, B-195; Q0106 part
    /// 1 (a)). The point is clamped into the unit's domain, as a cast's approach is. The walk is
    /// a move order's: it stops there and the order ends ([`MotionProposal::finish`] pops it).
    /// `None` while the target is seen, never seen under the order, or gone; pursuit decides then.
    /// Acquisition writes the sighting in phase 4 ([`MovementMemory::sight`]), so the answer does
    /// not depend on the order actors move in.
    fn last_seen_goal(
        &self,
        entry: MotionEntry,
        actor: &Entity,
        target: EntityId,
        origin: AttackOrigin,
    ) -> Option<SimVec2> {
        let point = actor
            .movement
            .lost_target_point(target)
            .filter(|_| origin == AttackOrigin::Explicit)?;
        self.is_entity_available_for_combat(actor.owner, target)
            .then(|| clamp_into_domain(entry.policy, &self.map, point))
    }

    /// Whether `actor`'s swing holds it this tick: it is in its wind-up and the weapon of the
    /// swing does not fire while moving (plan `2026-11-23-turret-wind-up` D-6, B-90, B-45;
    /// `StarCraft` II's `AllowedMovement`, where only `Moving` fires on the move). The mount plays
    /// no part. `Slowing` holds like `Stopped` (B-178).
    ///
    /// The weapon of the swing is the one the combat phase resolves for it (Amendment 2): against
    /// the plane of the swing's bound target; a sieged unit's siege weapon first. A swing with no
    /// weapon to resolve holds, the engine default. A swing whose target is gone holds nothing
    /// (plan `2026-11-25-wind-up-and-hold-facing` D-2, B-207): in a full tick
    /// [`SimWorld::end_lost_swings`] has already ended it, and any other lost swing, before the
    /// movement phase; only a direct call of the movement phase can still find one. Both
    /// movement pipelines call this through [`Self::walking_goal`], before pursuit.
    fn wind_up_holds(&self, actor: &Entity) -> Result<bool, MovementInvariant> {
        let WeaponPhase::Windup { target_id, .. } = actor.weapon_phase else {
            return Ok(false);
        };
        let Some(target) = self.entities.get(&target_id) else {
            return Ok(false);
        };
        let arch = self
            .catalog
            .get(actor.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(actor.id))?;
        // The plane the weapon phase's target snapshot gives a target of unknown type.
        let plane = self
            .catalog
            .get(target.unit_type_id)
            .map_or(MoverType::Ground, |target| target.mover_type);
        Ok(arch
            .weapon_against(plane, actor.active_siege_weapon())
            .is_none_or(|weapon| weapon.allowed_movement != AllowedMovement::Moving))
    }

    /// Settles a unit with no walk this tick, whose order ended without arriving or which holds
    /// (B-39, the owner's answer to Q0067): a unit with a finite deceleration glides on along its
    /// last velocity, losing its deceleration each tick until it rests; terrain that refuses the
    /// glide stops it at once. Any other unit rests now; its animation is the caller's.
    fn coast(
        &self,
        entry: MotionEntry,
        actor: &Entity,
        proposal: &mut MotionProposal,
    ) -> Result<(), MovementInvariant> {
        let id = entry.id;
        let fault = |e| arithmetic(id, e);
        let mut glide = None;
        // Only a unit that walked last tick has momentum of its own: the velocity of one that
        // stood is what a push gave it, and a push does not carry on.
        let walked = actor.current_anim == AnimState::Walk && actor.velocity != SimVec2::ZERO;
        if let Some(deceleration) = entry.deceleration.filter(|_| walked) {
            let speed = distance(actor.velocity, SimVec2::ZERO).map_err(fault)?;
            let left = speed.saturating_sub(deceleration);
            if left > SimScalar::ZERO {
                let heading = direction(actor.velocity, SimVec2::ZERO).map_err(fault)?;
                let step = checked_vec_scale(heading, left).map_err(fault)?;
                let to = checked_vec_add(actor.position, step).map_err(fault)?;
                if entry.policy.permits_segment(&self.map, actor.position, to) {
                    glide = Some((to, step));
                }
            }
        }
        if let Some((to, step)) = glide {
            proposal.position = to;
            proposal.velocity = step;
        } else {
            proposal.velocity = SimVec2::ZERO;
        }
        Ok(())
    }

    /// Prepares one pursuer's motion toward `target_id`: aim when in range, otherwise chase.
    ///
    /// The range check runs first and needs the whole world. Under the solver pipeline the ring
    /// rule then decides whether the unit stands for want of room (plan
    /// `2026-11-20-contact-solver` Amendment 7, item 2). The chase then runs in stages over a
    /// [`MotionDraft`], like walking, with the target's position as the goal.
    fn prepare_pursuit(
        &mut self,
        entry: MotionEntry,
        target_id: EntityId,
        origin: AttackOrigin,
    ) -> Result<MotionProposal, MovementInvariant> {
        let (mut proposal, mut pursuit) = match self.start_pursuit(entry, target_id, origin)? {
            PursuitStart::Settled(proposal) => return Ok(proposal),
            PursuitStart::Chase(proposal, pursuit) => (proposal, pursuit),
        };
        if self
            .catalog
            .movement_rules()
            .contact()
            .pipeline
            .runs_solver()
        {
            let position = self
                .entities
                .get(&entry.id)
                .map_or(SimVec2::ZERO, |a| a.position);
            let leash = (pursuit.origin == AttackOrigin::Acquired).then(|| {
                (
                    proposal.memory.leash_origin.unwrap_or(position),
                    pursuit.leash,
                )
            });
            let kept = proposal.memory.slot.filter(|gap| gap.target == target_id);
            let aim = self.ring_aim(RingChase {
                entry,
                position,
                target: target_id,
                band_max: pursuit.max,
                leash,
                kept: kept.map(|gap| gap.point),
            })?;
            proposal.memory.slot = None;
            match aim {
                RingAim::Stand => {
                    // G-6: no free angle on the ring; the order stays and no stall accrues
                    // (Q0089).
                    proposal.velocity = SimVec2::ZERO;
                    proposal.anim = AnimState::Idle;
                    return Ok(proposal);
                }
                RingAim::Straight => {}
                RingAim::Gap(_, point) => {
                    // A unit walking to a gap measures its progress to the gap point; a new
                    // gap starts that measure afresh.
                    if kept.is_none_or(|gap| gap.point != point) {
                        proposal.memory.progress_best = None;
                    }
                    proposal.memory.slot = Some(SlotClaim {
                        target: target_id,
                        point,
                    });
                    pursuit.gap = Some(point);
                }
            }
        }
        self.chase_target(entry, target_id, proposal, &pursuit)
    }

    /// The pursuit's first stage: a proposal that settles the tick when the target is gone, the
    /// unit has no weapon against it, or it stands in range and aims; otherwise the chase's
    /// facts.
    fn start_pursuit(
        &self,
        entry: MotionEntry,
        target_id: EntityId,
        origin: AttackOrigin,
    ) -> Result<PursuitStart, MovementInvariant> {
        let id = entry.id;
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "validated pursuer disappeared",
            })?;
        let mut proposal = MotionProposal::new(actor, entry.policy);
        if !self.is_entity_available_for_combat(actor.owner, target_id) {
            proposal.end_attack();
            self.coast(entry, actor, &mut proposal)?;
            return Ok(PursuitStart::Settled(proposal));
        }
        let target = self
            .entities
            .get(&target_id)
            .ok_or(MovementInvariant::InvalidState {
                entity: target_id,
                reason: "available target disappeared",
            })?;
        let Some(pursuit) = self.pursuit_band(entry, actor, target, target_id, origin)? else {
            proposal.end_attack();
            self.coast(entry, actor, &mut proposal)?;
            return Ok(PursuitStart::Settled(proposal));
        };
        let (target_at, _) = self.pursued_motion(target_id, target);
        if pursuit.aim_if_in_range(id, actor, target_at, &mut proposal)? {
            return Ok(PursuitStart::Settled(proposal));
        }
        Ok(PursuitStart::Chase(proposal, pursuit))
    }

    /// The chase toward `target_id` as a [`MotionDraft`], once the unit is out of range.
    fn chase_target(
        &mut self,
        entry: MotionEntry,
        target_id: EntityId,
        proposal: MotionProposal,
        pursuit: &Pursuit,
    ) -> Result<MotionProposal, MovementInvariant> {
        let id = entry.id;
        let policy = entry.policy;
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "validated pursuer disappeared",
            })?;
        let goal = if let Some(point) = pursuit.gap {
            point
        } else {
            let target = self
                .entities
                .get(&target_id)
                .ok_or(MovementInvariant::InvalidState {
                    entity: target_id,
                    reason: "available target disappeared",
                })?;
            self.pursued_motion(target_id, target).0
        };
        // `MotionEntry` creation already rejected owners without a slot (`effective_max_speed`).
        let owner = actor
            .owner
            .slot()
            .ok_or_else(|| arithmetic(id, MovementMathError::InvalidOwner(actor.owner)))?;
        let knowledge = self
            .vision_aware_pathfinding
            .then_some((&self.vision_maps[owner], &self.known_blocked_cells[owner]));
        let mut draft = MotionDraft {
            entry,
            actor,
            goal,
            request: route_request(actor, policy, goal, knowledge),
            tick: Tick(self.current_tick),
            ground: policy.layer == MovementLayer::Ground,
            cooled_down: proposal.memory.repath_cooldown == 0,
            proposal,
        };
        let mut scene = MotionScene {
            map: &self.map,
            pathfinder: &mut self.pathfinder,
            movement: &mut self.movement,
            current_tick: self.current_tick,
            route_groups: &self.route_groups,
            lane_tolerance: self.catalog.movement_rules().lane_tolerance_permille(),
            contact: self.catalog.movement_rules().contact(),
        };
        let mut neighbours = std::mem::take(&mut scene.movement.neighbours);
        let chased = draft.chase(&mut scene, &mut neighbours, pursuit);
        scene.movement.neighbours = neighbours;
        chased?;
        Ok(draft.proposal)
    }

    /// Where a pursuer sees its target, world units, and the target's last displacement, world
    /// units per tick: under the solver pipeline the target's tick-start position and motion
    /// (plan `2026-11-20-contact-solver` WP5, §5.4), so a pursuer processed after its target does
    /// not see where the target went this tick; under the legacy pipeline the live entity.
    fn pursued_motion(&self, target_id: EntityId, target: &Entity) -> (SimVec2, SimVec2) {
        if self
            .catalog
            .movement_rules()
            .contact()
            .pipeline
            .runs_solver()
        {
            self.tick_start_motion(target_id)
        } else {
            (target.position, target.velocity)
        }
    }

    /// The range band `actor` attacks `target` from, or `None` when it has no weapon against it.
    ///
    /// Range is measured between body centres, so both radii are added to the weapon's reach.
    fn pursuit_band(
        &self,
        entry: MotionEntry,
        actor: &Entity,
        target: &Entity,
        target_id: EntityId,
        origin: AttackOrigin,
    ) -> Result<Option<Pursuit>, MovementInvariant> {
        let id = entry.id;
        let target_policy = MotionPolicy::resolve(&self.catalog, &self.map, target)?;
        let arch = self
            .catalog
            .get(actor.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(id))?;
        let target_arch = self
            .catalog
            .get(target.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(target_id))?;
        let Some(weapon) = arch.weapon_against(
            target_arch.mover_type,
            actor
                .is_sieged
                .then_some(actor.siege_weapon.as_deref())
                .flatten(),
        ) else {
            return Ok(None);
        };
        let delta = self
            .player_upgrades
            .get(actor.owner)
            .ok_or_else(|| arithmetic(id, MovementMathError::InvalidOwner(actor.owner)))?
            .unit_stat_modifiers
            .get(&arch.slug)
            .map_or(SimScalar::ZERO, |m| m.weapon_range_delta);
        let max = weapon
            .max_range
            .checked_add(delta)
            .map(|range| actor.status_modifiers().weapon_range(range, weapon.melee))
            .ok_or_else(|| arithmetic(id, MovementMathError::Unrepresentable))?;
        if max < SimScalar::ZERO || weapon.min_range < SimScalar::ZERO {
            return Err(arithmetic(id, MovementMathError::InvalidDistance));
        }
        let radii = i128::from(entry.policy.radius.raw()) + i128::from(target_policy.radius.raw());
        let max = i128::from(max.raw()) + radii;
        let min = if weapon.min_range > SimScalar::ZERO {
            i128::from(weapon.min_range.raw()) + radii
        } else {
            0
        };
        Ok(Some(Pursuit {
            target: target_id,
            origin,
            target_moving: self.pursued_motion(target_id, target).1 != SimVec2::ZERO,
            has_turret: weapon.has_turret,
            max,
            min,
            leash: self.catalog.movement_rules().acquire_leash_radius(),
            gap: None,
        }))
    }
}

/// How a pursuit's first stage ended ([`SimWorld::start_pursuit`]).
enum PursuitStart {
    /// The tick is settled: the proposal is final.
    Settled(MotionProposal),
    /// The unit is out of range: chase.
    Chase(MotionProposal, Pursuit),
}

/// One chase's fixed facts: whom, why, and from how far the pursuer may fire.
struct Pursuit {
    target: EntityId,
    origin: AttackOrigin,
    /// Whether the target moved last tick, which voids the pursuer's stall evidence.
    target_moving: bool,
    /// Whether the weapon aims a turret rather than the whole body.
    has_turret: bool,
    /// Furthest centre separation the weapon reaches, in raw fixed-point units.
    max: i128,
    /// Nearest centre separation the weapon reaches, in raw fixed-point units; 0 when unbounded.
    min: i128,
    /// The leash of an automatic chase, world units: 5.0.14's `AcquireLeashRadius`, measured
    /// from where the chase began (B-142).
    leash: SimScalar,
    /// Under the solver pipeline, the free point of the ring the unit walks to when its own
    /// bearing is blocked (plan `2026-11-20-contact-solver` Amendment 7, item 2): the chase's
    /// goal, which it brakes onto and measures its progress against.
    gap: Option<SimVec2>,
}

impl Pursuit {
    /// Stops and aims at `goal` when `actor` stands within the band, and reports whether it did.
    fn aim_if_in_range(
        &self,
        id: EntityId,
        actor: &Entity,
        goal: SimVec2,
        proposal: &mut MotionProposal,
    ) -> Result<bool, MovementInvariant> {
        let separation = i128::from(
            distance(actor.position, goal)
                .map_err(|e| arithmetic(id, e))?
                .raw(),
        );
        if separation > self.max || separation < self.min {
            return Ok(false);
        }
        proposal.velocity = SimVec2::ZERO;
        let aim = direction(goal, actor.position).map_err(|e| arithmetic(id, e))?;
        // A turret is turned by the weapon phase alone, at its own rate; a hull weapon asks the
        // hull to face the goal, which it does within its rate when the proposal commits.
        if !self.has_turret {
            proposal.heading = SimAngle::atan2(aim.y, aim.x);
        }
        proposal.anim = if matches!(
            actor.weapon_phase,
            WeaponPhase::Windup { .. } | WeaponPhase::Backswing { .. }
        ) {
            AnimState::Attack
        } else {
            AnimState::Idle
        };
        Ok(true)
    }
}

/// Whether a motion stage settled the order (`Break`) or the next stage should run (`Continue`).
type Stage = ControlFlow<()>;

/// The parts of the world a motion stage reads or changes, borrowed apart from the entity table.
struct MotionScene<'a> {
    map: &'a SimMap,
    pathfinder: &'a mut Pathfinder,
    movement: &'a mut MovementWorkspace,
    current_tick: u32,
    /// The world's route groups (plan `2026-11-10-group-routing`).
    route_groups: &'a [super::groups::RouteGroup],
    /// The catalog's lane tolerance in permille; 0 switches group routing off.
    lane_tolerance: u16,
    /// The catalog's contact rules: which pipeline runs, and its side horizon.
    contact: ContactRules,
}

impl MotionScene<'_> {
    /// Collects the tick-start bodies near `position` of `entry`'s collision class into `out`.
    fn gather_neighbours(
        &self,
        entry: MotionEntry,
        position: SimVec2,
        out: &mut Vec<SteeringObstacle>,
    ) {
        let class = entry.policy.class.index();
        gather_neighbours(
            &NeighbourIndex {
                grid: &self.movement.grids[class],
                obstacles: &self.movement.obstacles[class],
                largest_radius: self.movement.largest_radius[class],
            },
            self.map,
            (entry, position),
            self.intent(),
            out,
        );
    }

    /// The side rule's horizon factor under the solver pipeline, `None` under the legacy one.
    fn intent(&self) -> Option<u8> {
        self.contact
            .pipeline
            .runs_solver()
            .then_some(self.contact.side_horizon_factor)
    }
}

/// One actor's motion while it is prepared: who moves, toward what, and the edits decided so far.
///
/// The actor is read as it stood at the start of the tick; every stage writes only `proposal`,
/// which [`MotionProposal::commit`] applies after all fallible work. Stage order is part of the
/// simulation's rules: a checked operation that fails first decides the reported error.
struct MotionDraft<'a> {
    entry: MotionEntry,
    actor: &'a Entity,
    proposal: MotionProposal,
    /// Where the unit is headed this tick, in world units. A retarget replaces it.
    goal: SimVec2,
    /// The route query toward the goal the order named, seen through the owner's knowledge.
    request: PathRequestWithVision<'a>,
    tick: Tick,
    ground: bool,
    /// Whether a route search may run this tick.
    cooled_down: bool,
}

impl MotionDraft<'_> {
    fn fault(&self) -> impl Fn(MovementMathError) -> MovementInvariant {
        let id = self.entry.id;
        move |source| arithmetic(id, source)
    }

    /// What the step toward a waypoint reads, over this tick's `neighbours`.
    fn step_context<'n>(
        &self,
        map: &'n SimMap,
        neighbours: &'n [SteeringObstacle],
        intent: Option<u8>,
    ) -> StepContext<'n> {
        StepContext {
            map,
            policy: self.entry.policy,
            speed: self.entry.step,
            body: ContactBody {
                id: self.entry.id,
                owner: self.actor.owner,
                push_priority: self.entry.push_priority,
                pos: self.actor.position,
                radius: self.entry.policy.radius,
            },
            neighbours,
            hull: self.hull_gate(),
            surround: None,
            scan_range: self.entry.scan_range,
            intent,
            near_margin: near_margin(self.entry),
        }
    }

    /// The hull gate of a `turn_before_move` actor, read from its tick-start heading.
    fn hull_gate(&self) -> Option<HullGate> {
        self.entry.turn_before_move.then_some(HullGate {
            heading: self.actor.heading,
            rate: self.entry.turn_rate,
        })
    }

    /// Whether this actor may move straight onto `goal` this tick. A `turn_before_move` actor
    /// facing too far away instead turns towards it in place, still walking, and the proposal
    /// keeps its order; the same gate as [`gate_step_by_facing`], for the snap onto a goal
    /// within one step.
    fn may_snap_to(&mut self, goal: SimVec2) -> Result<bool, MovementInvariant> {
        let Some(gate) = self.hull_gate() else {
            return Ok(true);
        };
        let step = checked_vec_sub(goal, self.actor.position).map_err(self.fault())?;
        if step == SimVec2::ZERO {
            return Ok(true);
        }
        let direction = SimAngle::atan2(step.y, step.x);
        self.proposal.heading = direction;
        if hull_allows_step(gate.heading, direction, gate.rate) {
            return Ok(true);
        }
        self.proposal.velocity = SimVec2::ZERO;
        self.proposal.anim = AnimState::Walk;
        Ok(false)
    }

    /// Walks a move order: route upkeep, the arrival decision, the progress check, the step.
    fn walk(
        &mut self,
        scene: &mut MotionScene<'_>,
        neighbours: &mut Vec<SteeringObstacle>,
        check_route: bool,
        map_news: bool,
    ) -> Result<(), MovementInvariant> {
        self.brake(SimScalar::ZERO)?;
        if self.upkeep_route(scene, check_route)?.is_break() {
            return Ok(());
        }
        scene.gather_neighbours(self.entry, self.actor.position, neighbours);
        let attack_moving = matches!(
            self.actor.order_queue.front(),
            Some(UnitOrder::AttackMove { .. })
        );
        let intent = scene.intent();
        let context = StepContext {
            surround: (self.entry.surround_walk && attack_moving && intent.is_none())
                .then_some(None),
            ..self.step_context(scene.map, neighbours, intent)
        };
        let on_last_leg = match self.decide_arrival(&context, scene.pathfinder)? {
            ControlFlow::Continue(on_last_leg) => on_last_leg,
            ControlFlow::Break(()) => return Ok(()),
        };
        let (next, gap) = self.next_waypoint(&context, scene.pathfinder)?;
        // An attack-move queued behind a friend that holds its ground is not stalled (D-4); a
        // plain move into a blocked spot still is.
        // Under the solver the stall rule reads no class of another body (plan Amendment 7,
        // item 1): a unit with no room stands by the ring rule instead.
        let queued = attack_moving
            && intent.is_none()
            && friend_holds_ahead(
                &context,
                direction(next, self.actor.position).map_err(self.fault())?,
            )
            .map_err(self.fault())?;
        let routes = &mut scene.movement.route_outcomes;
        let reported = routes.len();
        if self
            .check_progress(routes, on_last_leg, gap, map_news, queued)?
            .is_break()
        {
            // Under the solver a give-up is no arrival: the unit is no witness of the group-arrival
            // chain (WP3b), so a clump's chain never runs back through units that gave up.
            let gave_up = routes[reported..]
                .iter()
                .any(|outcome| outcome.kind == RouteOutcomeKind::NoProgress);
            if gave_up && context.intent.is_some() {
                self.proposal.memory.arrived_at = None;
            }
            return Ok(());
        }
        self.propose_step(&context, scene.pathfinder, next, gap)
    }

    /// Lowers this tick's step so a unit with a finite deceleration can stop `short` world units
    /// before its goal (zero for a walk's destination, the weapon's reach between centres for a
    /// chase), measuring the way left along its stored route and on to the goal.
    fn brake(&mut self, short: SimScalar) -> Result<(), MovementInvariant> {
        let Some(deceleration) = self.entry.deceleration else {
            return Ok(());
        };
        let mut remaining = SimScalar::ZERO;
        let mut from = self.actor.position;
        let route = self.proposal.route(self.actor);
        for &point in route.iter().chain(std::iter::once(&self.goal)) {
            remaining = remaining
                .checked_add(distance(from, point).map_err(self.fault())?)
                .ok_or_else(|| arithmetic(self.entry.id, MovementMathError::Unrepresentable))?;
            from = point;
        }
        let remaining = remaining.saturating_sub(short).max(SimScalar::ZERO);
        let limit = braking_speed(remaining, deceleration).map_err(self.fault())?;
        self.entry.step = self.entry.step.min(limit);
        Ok(())
    }

    /// Route upkeep: drops a stored route that no longer works, finishes a unit already within
    /// one step of its goal, and searches a new route when the unit has none.
    fn upkeep_route(
        &mut self,
        scene: &mut MotionScene<'_>,
        check_route: bool,
    ) -> Result<Stage, MovementInvariant> {
        let (id, policy, actor) = (self.entry.id, self.entry.policy, self.actor);
        if self.ground
            && self.cooled_down
            && check_route
            && !self.proposal.route(actor).is_empty()
            && !route_is_usable(
                scene.map,
                scene.pathfinder,
                &self.request,
                policy,
                actor.position,
                self.proposal.route(actor),
            )
        {
            self.proposal.drop_route();
        }
        if !self.proposal.route(actor).is_empty() {
            return Ok(Stage::Continue(()));
        }
        if distance(actor.position, self.goal).map_err(self.fault())? <= self.entry.step
            && probe_permits(
                scene.map,
                scene.pathfinder,
                policy,
                actor.position,
                self.goal,
            )
        {
            if self.may_snap_to(self.goal)? {
                self.proposal.position = self.goal;
                self.proposal.finish(self.goal);
            }
            return Ok(Stage::Break(()));
        }
        if !(self.ground && self.cooled_down) {
            return Ok(Stage::Continue(()));
        }
        self.proposal.memory.repath_cooldown = REPATH_COOLDOWN_TICKS;
        scene.movement.path_searches += 1;
        let search = search_route(
            scene.pathfinder,
            scene.map,
            &self.request,
            &mut scene.movement.reach_cache,
            scene.current_tick,
            actor.owner.0,
        );
        let path = match search {
            // A member of a route group may take its group's lane instead (group-routing D-4).
            RouteSearch::Found(path) => super::groups::GroupedSearch {
                groups: scene.route_groups,
                tolerance: scene.lane_tolerance,
                pathfinder: scene.pathfinder,
                map: scene.map,
                request: &self.request,
                events: &mut scene.movement.route_outcomes,
                searches: &mut scene.movement.path_searches,
                tick: self.tick,
            }
            .decide(actor, self.goal, path),
            RouteSearch::Retargeted(reachable, path) => {
                scene.movement.route_outcomes.push(RouteOutcome {
                    tick: self.tick,
                    entity: id,
                    kind: RouteOutcomeKind::Retargeted {
                        requested: self.goal,
                        reachable,
                    },
                });
                self.goal = reachable;
                self.proposal.retarget = Some(reachable);
                path
            }
            RouteSearch::Blocked => {
                scene.movement.route_outcomes.push(RouteOutcome {
                    tick: self.tick,
                    entity: id,
                    kind: RouteOutcomeKind::Unreachable,
                });
                self.proposal.finish(self.goal);
                return Ok(Stage::Break(()));
            }
        };
        self.proposal
            .adopt_route(actor.position, path.waypoints)
            .map_err(self.fault())?;
        Ok(Stage::Continue(()))
    }

    /// Arrival decision: passes waypoints already done with, then finishes a unit on its last leg
    /// that touches a friendly body already settled on the same goal.
    ///
    /// Continues with whether the unit is on its last leg.
    fn decide_arrival(
        &mut self,
        context: &StepContext<'_>,
        pathfinder: &mut Pathfinder,
    ) -> Result<ControlFlow<(), bool>, MovementInvariant> {
        let (id, actor, goal, ground) = (self.entry.id, self.actor, self.goal, self.ground);
        drop_passed_waypoints(
            context,
            pathfinder,
            &self.request,
            actor,
            &mut self.proposal,
        )
        .map_err(self.fault())?;
        let on_last_leg = self.proposal.route(actor).len() <= 1;
        let priority = self.entry.push_priority;
        let witness = |witness: &SteeringObstacle| {
            witness.owner == actor.owner
                && (!ground || on_connected_ground(context.map, witness.pos, goal))
        };
        let arrived = |reach, unpushable_only: bool| {
            check_cluster_arrival_with(
                Some(id),
                actor.position,
                self.entry.policy.radius,
                goal,
                context.neighbours,
                (reach, |seen: &SteeringObstacle| {
                    (!unpushable_only || seen.push_priority > priority) && witness(seen)
                }),
            )
        };
        // Under the solver an arrival this unit cannot push (C-2 row 6: a higher priority) is a
        // witness wherever it stands (WP3b, B-158): the unit gains nothing by pressing on. Only
        // arrivals are witnesses, since a give-up records none (`check_progress`).
        if on_last_leg
            && (arrived(Some(ARRIVAL_WITNESS_REACH), false).map_err(self.fault())?
                || (context.intent.is_some() && arrived(None, true).map_err(self.fault())?))
        {
            self.proposal.finish(goal);
            return Ok(ControlFlow::Break(()));
        }
        Ok(ControlFlow::Continue(on_last_leg))
    }

    /// The point to step toward, the route's first waypoint or else the goal, and its distance
    /// from the unit in world units.
    ///
    /// The route is the pathfinder's, never rewritten here: a body parked on a corner is an
    /// obstacle for steering, as anywhere else on the route (plan `2026-11-09-sc2-pathfinding`
    /// D-9 A1). When a body is parked on the corner, the unit aims past it on the outside of the
    /// turn instead ([`aim_past_parked_body`], D-9 A3).
    fn next_waypoint(
        &mut self,
        context: &StepContext<'_>,
        pathfinder: &mut Pathfinder,
    ) -> Result<(SimVec2, SimScalar), MovementInvariant> {
        let actor = self.actor;
        let next = match *self.proposal.route(actor) {
            [corner, subsequent, ..] => {
                // A rounded corner is a run of points on an arc (plan Amendment 2); the side to
                // pass a parked body on is judged against where the route goes after the arc,
                // its first point beyond a body width of the corner.
                let reach =
                    self.entry.policy.radius + self.entry.policy.radius + WAYPOINT_HANDOFF_MARGIN;
                let subsequent = self.proposal.route(actor)[1..]
                    .iter()
                    .copied()
                    .find(|&point| !within_distance(point, corner, i128::from(reach.raw())))
                    .unwrap_or(subsequent);
                let mut sense = self.proposal.memory.corner_round;
                let aim = aim_past_parked_body(context, pathfinder, corner, subsequent, &mut sense)
                    .map_err(self.fault())?;
                self.proposal.memory.corner_round = sense;
                aim.unwrap_or(corner)
            }
            [last] => last,
            [] => self.goal,
        };
        let gap = distance(actor.position, next).map_err(self.fault())?;
        Ok((next, gap))
    }

    /// Progress check: gives the order up when the unit has stopped gaining ground, finishes it
    /// when the unit is stuck within reach of its goal, and drops the route on a stall to repath.
    /// A `queued` tick, spent behind a friend that holds its ground, does not count as stalled
    /// (plan `2026-11-19-crowd-edge-steering` D-4).
    fn check_progress(
        &mut self,
        outcomes: &mut Vec<RouteOutcome>,
        on_last_leg: bool,
        gap: SimScalar,
        map_news: bool,
        queued: bool,
    ) -> Result<Stage, MovementInvariant> {
        if self.entry.speed > SimScalar::ZERO {
            if map_news || self.proposal.memory.flank_active {
                // A surround walk gains ground round the crowd, not along the route (D-3).
                self.proposal.memory.progress_best = None;
            }
            let route = self.proposal.route(self.actor);
            let mut memory = self.proposal.memory;
            let stalled =
                note_queued_progress(&mut memory, self.actor.position, route, self.goal, queued)
                    .map_err(self.fault())?;
            self.proposal.memory = memory;
            let stuck_at_goal =
                on_last_leg && stalled >= STALL_REPATH_TICKS && gap <= STALL_ARRIVAL_REACH;
            if stalled >= STALL_ABANDON_TICKS || stuck_at_goal {
                if !stuck_at_goal {
                    outcomes.push(RouteOutcome {
                        tick: self.tick,
                        entity: self.entry.id,
                        kind: RouteOutcomeKind::NoProgress,
                    });
                }
                self.proposal.finish(self.goal);
                return Ok(Stage::Break(()));
            }
            if self.ground
                && self.cooled_down
                && stalled > 0
                && stalled.is_multiple_of(STALL_REPATH_TICKS)
            {
                self.proposal.drop_route();
            }
        }
        Ok(Stage::Continue(()))
    }

    /// The step: moves toward `next`, `gap` world units away, drops a route terrain refused,
    /// and finishes a unit whose step also reaches the goal.
    fn propose_step(
        &mut self,
        context: &StepContext<'_>,
        pathfinder: &mut Pathfinder,
        next: SimVec2,
        gap: SimScalar,
    ) -> Result<(), MovementInvariant> {
        let (policy, actor, map) = (self.entry.policy, self.actor, context.map);
        let refused = step_toward(context, pathfinder, &mut self.proposal, next, gap, None)
            .map_err(self.fault())?;
        if refused
            && self.ground
            && self.cooled_down
            && !self.proposal.route(actor).is_empty()
            && !policy.permits_segment(map, actor.position, next)
        {
            self.proposal.drop_route();
        }
        self.pass_reached_waypoint(next);
        if self.proposal.route(actor).is_empty()
            && self.proposal.position == next
            && distance(next, self.goal).map_err(self.fault())? <= self.entry.step
            && probe_permits(map, pathfinder, policy, next, self.goal)
        {
            self.proposal.position = self.goal;
            self.proposal.finish(self.goal);
        }
        Ok(())
    }

    /// Consumes the route's first point when this tick's step lands exactly on it.
    fn pass_reached_waypoint(&mut self, next: SimVec2) {
        if self.proposal.position == next && self.proposal.route(self.actor).first() == Some(&next)
        {
            let reach =
                self.entry.policy.radius + self.entry.policy.radius + WAYPOINT_HANDOFF_MARGIN;
            self.proposal.pass_waypoint(self.actor, reach);
        }
    }

    /// Chases `pursuit`'s target: route upkeep, waypoint handling, the stall check, the step and,
    /// when terrain refused the step, a fresh route within the leash.
    fn chase(
        &mut self,
        scene: &mut MotionScene<'_>,
        neighbours: &mut Vec<SteeringObstacle>,
        pursuit: &Pursuit,
    ) -> Result<(), MovementInvariant> {
        let stale = self.proposal.route(self.actor).last().is_some_and(|&end| {
            !within_distance(end, self.goal, i128::from(PURSUIT_ROUTE_DRIFT.raw()))
        });
        if self.ground && self.cooled_down && stale {
            self.proposal.drop_route();
        }
        // Brake to stop on the edge of the weapon's band rather than within a tick (B-39), or on
        // the gap point, which lies on that edge.
        let reach = if pursuit.gap.is_some() {
            SimScalar::ZERO
        } else {
            SimScalar::from_raw(i64::try_from(pursuit.max).unwrap_or(i64::MAX))
        };
        self.brake(reach)?;
        scene.gather_neighbours(self.entry, self.actor.position, neighbours);
        let intent = scene.intent();
        let context = StepContext {
            surround: (self.entry.surround_walk && intent.is_none())
                .then_some(Some(pursuit.target)),
            ..self.step_context(scene.map, neighbours, intent)
        };
        drop_passed_waypoints(
            &context,
            scene.pathfinder,
            &self.request,
            self.actor,
            &mut self.proposal,
        )
        .map_err(self.fault())?;
        let (next, gap) = self.next_waypoint(&context, scene.pathfinder)?;
        // A chase queued behind a friend that holds its ground is not stalled (D-4). Under the
        // solver the stall rule reads no class of another body (plan Amendment 7, item 1).
        let queued = context.intent.is_none()
            && friend_holds_ahead(
                &context,
                direction(next, self.actor.position).map_err(self.fault())?,
            )
            .map_err(self.fault())?;
        let routes = &mut scene.movement.route_outcomes;
        if self.beyond_leash(pursuit) {
            self.abandon_pursuit(routes, pursuit.target);
            return Ok(());
        }
        if self
            .check_pursuit_progress(routes, pursuit, queued)?
            .is_break()
        {
            return Ok(());
        }
        let refused = step_toward(
            &context,
            scene.pathfinder,
            &mut self.proposal,
            next,
            gap,
            Some(pursuit.target),
        )
        .map_err(self.fault())?;
        self.pass_reached_waypoint(next);
        if refused && self.ground && self.cooled_down {
            self.reroute_pursuit(scene, pursuit)?;
        }
        Ok(())
    }

    /// Progress check for a chase: gives the target up when the pursuer stopped gaining ground.
    /// A `queued` tick, spent behind a friend that holds its ground, does not count as stalled
    /// (plan `2026-11-19-crowd-edge-steering` D-4).
    fn check_pursuit_progress(
        &mut self,
        outcomes: &mut Vec<RouteOutcome>,
        pursuit: &Pursuit,
        queued: bool,
    ) -> Result<Stage, MovementInvariant> {
        if self.entry.speed > SimScalar::ZERO {
            if pursuit.target_moving || self.proposal.memory.flank_active {
                // A moving goal says nothing about whether this unit is stuck, and a surround
                // walk gains ground round the crowd, not toward the target (D-3).
                self.proposal.memory.progress_best = None;
            }
            let route = self.proposal.route(self.actor);
            let mut memory = self.proposal.memory;
            let stalled =
                note_queued_progress(&mut memory, self.actor.position, route, self.goal, queued)
                    .map_err(self.fault())?;
            self.proposal.memory = memory;
            if stalled >= STALL_ABANDON_TICKS {
                self.abandon_pursuit(outcomes, pursuit.target);
                return Ok(Stage::Break(()));
            }
        }
        Ok(Stage::Continue(()))
    }

    /// Whether an automatic chase has taken the pursuer beyond its leash: farther than the leash
    /// radius from where the chase began, routed or direct alike (5.0.14's `AcquireLeashRadius`,
    /// plan `2026-11-18-contour-footprints` Amendment 4, B-142). The first tick of a chase records
    /// where it began; an ordered attack never has a leash and clears it.
    fn beyond_leash(&mut self, pursuit: &Pursuit) -> bool {
        if pursuit.origin != AttackOrigin::Acquired {
            self.proposal.memory.leash_origin = None;
            return false;
        }
        let origin = *self
            .proposal
            .memory
            .leash_origin
            .get_or_insert(self.actor.position);
        !within_distance(self.actor.position, origin, i128::from(pursuit.leash.raw()))
    }

    /// Searches a new route after terrain refused a chasing step, and gives the target up when
    /// no route reaches firing range.
    fn reroute_pursuit(
        &mut self,
        scene: &mut MotionScene<'_>,
        pursuit: &Pursuit,
    ) -> Result<(), MovementInvariant> {
        self.proposal.memory.repath_cooldown = REPATH_COOLDOWN_TICKS;
        scene.movement.path_searches += 1;
        let search = search_route(
            scene.pathfinder,
            scene.map,
            &self.request,
            &mut scene.movement.reach_cache,
            scene.current_tick,
            self.actor.owner.0,
        );
        let path = match search {
            RouteSearch::Found(path) => Some(path),
            RouteSearch::Retargeted(reachable, path) => {
                within_distance(reachable, self.goal, pursuit.max).then_some(path)
            }
            RouteSearch::Blocked => None,
        };
        match path {
            Some(path) => {
                if self.actor.current_funnel_waypoints.is_empty() {
                    // Leaving the straight line for a detour lengthens the way on purpose.
                    self.proposal.memory.progress_best = None;
                }
                self.proposal
                    .adopt_route(self.actor.position, path.waypoints)
                    .map_err(self.fault())?;
            }
            _ => self.abandon_pursuit(&mut scene.movement.route_outcomes, pursuit.target),
        }
        Ok(())
    }

    /// Reports the chase of `target` as abandoned, leaves that target alone for a while, and ends
    /// the chase's leash.
    fn abandon_pursuit(&mut self, outcomes: &mut Vec<RouteOutcome>, target: EntityId) {
        self.proposal.memory.leash_origin = None;
        outcomes.push(RouteOutcome {
            tick: self.tick,
            entity: self.entry.id,
            kind: RouteOutcomeKind::PursuitAbandoned { target },
        });
        self.proposal.abandon_target(target);
    }
}

/// The body parked on `corner`, if any: a tick-start neighbour that will not step aside for this
/// unit and touches the corner point with its own radius and the unit's, and, unless the unit is
/// already `rounding` a body at this corner, stands still.
///
/// A moving body does not start a rounding: it clears the corner by itself, and steering spaces
/// the unit from it like any other flockmate. A rounding once begun is held while any such body
/// touches the corner, moving or not: the decision to round, like its sense
/// ([`MovementMemory::corner_round`]), was taken against one snapshot and is not undone by the
/// next while its cause persists, or the unit would turn back to the corner as soon as the body
/// it rounds starts to walk (plan `2026-11-19-parked-body-aim` D-1, B-145).
fn parked_on_corner<'n>(
    context: &StepContext<'n>,
    corner: SimVec2,
    rounding: bool,
) -> Option<&'n SteeringObstacle> {
    let (body, policy) = (context.body, context.policy);
    context.neighbours.iter().find(|obstacle| {
        (rounding || obstacle.velocity == SimVec2::ZERO)
            && !obstacle.yields_to(body.owner, body.push_priority)
            && within_distance(
                obstacle.pos,
                corner,
                i128::from(policy.radius.raw()) + i128::from(obstacle.radius.raw()),
            )
            && context.is_near(obstacle)
    })
}

/// The point a unit steers toward instead of its corner waypoint when a body that will not yield
/// is parked on that corner ([`parked_on_corner`]), or `None` when no body is, the geometry is
/// degenerate, or terrain refuses the step toward it. Positions are world units.
///
/// A corner hugs the obstacle it turns around, so a body parked on it can only be passed on the
/// outside of the turn (plan `2026-11-09-sc2-pathfinding` D-9 A3). The aim is the tangent point,
/// seen from the unit, of the circle around the body of radius `r_body + r_self +`
/// [`WAYPOINT_HANDOFF_MARGIN`]; inside that circle it is a point along the circle's tangent at
/// the unit, one circle radius away.
///
/// Of the two candidates, the first aim is the one further onto the corner's side of the line
/// from the unit to the next waypoint `subsequent`. Before the corner, that line cuts the
/// obstacle and the corner lies between the line and the outside of the turn, so that candidate
/// is on the outside. Its sense round the body is then kept in `sense` (`1` counter-clockwise,
/// `-1` clockwise; [`MovementMemory::corner_round`]) and every later aim takes the candidate of
/// that sense: once the unit is beside the body the line no longer cuts the obstacle, and the
/// side rule would send it back. A held sense also holds the decision to round: while `sense` is
/// set, a body touching the corner keeps the unit rounding it even once it walks (plan
/// `2026-11-19-parked-body-aim` D-1). The route is never written: this is a steering aim.
fn aim_past_parked_body(
    context: &StepContext<'_>,
    pathfinder: &mut Pathfinder,
    corner: SimVec2,
    subsequent: SimVec2,
    sense: &mut i8,
) -> Result<Option<SimVec2>, MovementMathError> {
    let (body, policy) = (context.body, context.policy);
    let Some(occupant) = parked_on_corner(context, corner, *sense != 0) else {
        return Ok(None);
    };
    let away = direction(body.pos, occupant.pos)?;
    if away == SimVec2::ZERO {
        return Ok(None);
    }
    let reach = policy.radius + occupant.radius + WAYPOINT_HANDOFF_MARGIN;
    let normal = SimVec2::new(-away.y, away.x);
    let gap = distance(body.pos, occupant.pos)?;
    let (base, lateral) = if gap > reach {
        let cosine = reach
            .checked_div(gap)
            .ok_or(MovementMathError::Unrepresentable)?;
        let sine = SimScalar::ONE
            .checked_sub(cosine.saturating_mul(cosine))
            .ok_or(MovementMathError::Unrepresentable)?
            .sqrt()
            .map_err(|_| MovementMathError::Unrepresentable)?;
        let base = checked_vec_add(
            occupant.pos,
            checked_vec_scale(away, reach.saturating_mul(cosine))?,
        )?;
        (base, checked_vec_scale(normal, reach.saturating_mul(sine))?)
    } else {
        (body.pos, checked_vec_scale(normal, reach)?)
    };
    // `normal` turns `away` a quarter counter-clockwise, so `base + lateral` lies
    // counter-clockwise round the body from the unit.
    let counter_clockwise = checked_vec_add(base, lateral)?;
    let clockwise = checked_vec_sub(base, lateral)?;
    if *sense == 0 {
        let along = direction(subsequent, body.pos)?;
        let side = difference_product(along, corner, body.pos, true)?.signum();
        if side == 0 || along == SimVec2::ZERO {
            return Ok(None);
        }
        let onto_corner_side =
            |point: SimVec2| difference_product(along, point, body.pos, true).map(|v| v * side);
        *sense = if onto_corner_side(counter_clockwise)? >= onto_corner_side(clockwise)? {
            1
        } else {
            -1
        };
    }
    let aim = if *sense > 0 {
        counter_clockwise
    } else {
        clockwise
    };
    Ok(probe_permits(context.map, pathfinder, policy, body.pos, aim).then_some(aim))
}

/// Drops leading waypoints the unit no longer needs to touch.
///
/// A corner is a means of reaching the next point. Once that point is in terrain view from here,
/// and the unit is near the corner or already beyond it (in the half-plane past the corner along
/// the next leg), the corner has done its work. Being near is measured in body widths because
/// another unit may stand exactly on the corner. Bodies on the next leg do not keep the corner:
/// they are steering's to avoid, and a unit pushed past its corner must not turn back to it
/// (plan `2026-11-09-sc2-pathfinding` D-9 A2). A corner with a body parked on it is passed only
/// beyond it: until then the unit rounds the body on the outside ([`aim_past_parked_body`]), and
/// heading straight for the next point from beside the body would push it into the gap between
/// the body and the obstacle the corner turns around. A unit already rounding a body at the
/// corner (`MovementMemory::corner_round` set) keeps it while any non-yielding body touches the
/// corner, as [`aim_past_parked_body`] keeps aiming past it: the two must agree, or a held
/// rounding would hand its corner off by distance while still aiming past the body (plan
/// `2026-11-19-parked-body-aim` D-1).
fn drop_passed_waypoints(
    context: &StepContext<'_>,
    pathfinder: &mut Pathfinder,
    request: &PathRequestWithVision<'_>,
    actor: &Entity,
    proposal: &mut MotionProposal,
) -> Result<(), MovementMathError> {
    let (map, policy) = (context.map, context.policy);
    let handoff = context
        .speed
        .max(policy.radius + policy.radius + WAYPOINT_HANDOFF_MARGIN);
    while let [first, subsequent, ..] = *proposal.route(actor) {
        let gap = distance(actor.position, first)?;
        let past = gap <= TERRAIN_GUARD_HORIZON
            && checked_dot(
                checked_vec_sub(actor.position, first)?,
                checked_vec_sub(subsequent, first)?,
            )? >= SimScalar::ZERO;
        let near = gap <= handoff
            && parked_on_corner(context, first, proposal.memory.corner_round != 0).is_none();
        if !near && !past {
            break;
        }
        // A body wedged where it stands (its disc overlaps a wall) sees by terrain only, as its
        // route's first leg is held (plan D-3); otherwise the disc must fit along the leg. The
        // leg is asked first: when the disc fits along it, whether it fits where it stands is moot.
        let in_view = policy.permits_segment(map, actor.position, subsequent)
            && (policy.layer == MovementLayer::Air
                || pathfinder.segment_is_passable(map, request, actor.position, subsequent)
                || !pathfinder.segment_is_passable(map, request, actor.position, actor.position));
        if !in_view {
            break;
        }
        proposal.pass_waypoint(
            actor,
            policy.radius + policy.radius + WAYPOINT_HANDOFF_MARGIN,
        );
    }
    Ok(())
}

/// This tick's own-motion budget for a unit whose effective top speed is `top`, world units per
/// tick, having moved `previous` (world units) on the tick before.
///
/// `StarCraft` II's `Acceleration` (`CUnit`, Normal units per second squared, converted to world
/// units per tick per tick in the catalog) raises speed by at most that much per tick. An
/// `acceleration` at or above the archetype's `max_speed` reaches any speed within one tick,
/// so it is instant, as the game's `1000`; comparing with the archetype's speed rather than the
/// effective one keeps a buffed unit (Stimpack) instant too.
fn accelerated_speed(
    top: SimScalar,
    acceleration: SimScalar,
    archetype_speed: SimScalar,
    previous: SimVec2,
) -> SimScalar {
    if acceleration >= archetype_speed {
        return top;
    }
    top.min(previous.length().saturating_add(acceleration))
}

/// The highest speed from which a unit losing `deceleration` per tick still stops within
/// `remaining` world units: the root `v` of `v² ÷ (2 d) + v ÷ 2 = remaining`, the distance a
/// tick-by-tick braking run from `v` covers. It is at least `remaining` once `remaining` is at
/// most `deceleration`, so the unit always snaps onto its goal (the fixed-point root rounds
/// down, so this floor is applied explicitly).
fn braking_speed(
    remaining: SimScalar,
    deceleration: SimScalar,
) -> Result<SimScalar, MovementMathError> {
    let half = deceleration
        .checked_div(SimScalar::from_i32(2))
        .ok_or(MovementMathError::Unrepresentable)?;
    let root = deceleration
        .checked_mul(remaining)
        .and_then(|product| product.checked_mul(SimScalar::from_i32(2)))
        .and_then(|twice| twice.checked_add(half.checked_mul(half)?))
        .ok_or(MovementMathError::Unrepresentable)?
        .sqrt()
        .map_err(|_| MovementMathError::Unrepresentable)?;
    Ok(root.saturating_sub(half).max(remaining.min(deceleration)))
}

/// Takes one step toward `next`, `gap` world units away, avoiding `neighbours`.
///
/// A unit whose admissible step, after bodies that will not yield have taken their share, is
/// shorter than [`WAIT_THRESHOLD`] of its step waits: it neither moves nor turns, keeps its order
/// and plays `Idle`, and tries again next tick (plan `2026-11-09-friendly-blocking` D-1.3). It
/// does not step into the gap and get pushed back out, which vibrated (bug B-103). The same holds
/// for a step that the slide along a wall cuts below that share ([`after_wall_slide`]).
///
/// Returns whether terrain refused the step as planned, so the caller can check its route.
fn step_toward(
    context: &StepContext<'_>,
    pathfinder: &mut Pathfinder,
    proposal: &mut MotionProposal,
    next: SimVec2,
    gap: SimScalar,
    ignored: Option<EntityId>,
) -> Result<bool, MovementMathError> {
    let StepContext {
        map,
        policy,
        speed,
        body,
        neighbours,
        hull,
        ..
    } = *context;
    let origin = body.pos;
    if gap <= speed && probe_permits(map, pathfinder, policy, origin, next) {
        proposal.position = next;
        proposal.velocity = checked_vec_sub(next, origin)?;
        proposal.anim = AnimState::Walk;
        gate_step_by_facing(hull, origin, proposal);
        return Ok(false);
    }
    let heading = proposed_heading(context, proposal, next, ignored)?;
    let mut velocity = checked_vec_scale(heading, speed)?;
    if policy.layer == MovementLayer::Ground {
        velocity = keep_terrain_view(context, pathfinder, next, gap, velocity)?;
    }
    let mut crowded = false;
    if context.intent.is_none() {
        let unblocked = velocity;
        velocity = remove_blocked_velocity(body, velocity, neighbours)?;
        // Bodies that will not yield cut this step: the queue the waiting rule is for.
        crowded = velocity != unblocked;
        if let Some(target) = context.surround {
            velocity = surround_walk(context, pathfinder, proposal, next, velocity, target)?;
        }
        if is_waiting(velocity, speed) {
            proposal.velocity = SimVec2::ZERO;
            proposal.anim = AnimState::Idle;
            return Ok(false);
        }
    }
    // The probe is the disc's (plan `2026-11-11-contact-and-missiles` D-3, R-3): a step that
    // would press the disc into a wall slides along it instead, or waits on a sliver of slide
    // ([`after_wall_slide`]).
    let mut refused = false;
    if !probe_permits(
        map,
        pathfinder,
        policy,
        origin,
        checked_vec_add(origin, velocity)?,
    ) {
        let slid = slide_along_wall(map, pathfinder, policy, origin, velocity)?;
        match after_wall_slide(velocity, slid, speed, crowded) {
            WallSlide::Step {
                velocity: step,
                refused: cut,
            } => {
                velocity = step;
                refused = cut;
            }
            WallSlide::Wait { refused: cut } => {
                proposal.velocity = SimVec2::ZERO;
                proposal.anim = AnimState::Idle;
                return Ok(cut);
            }
        }
    }
    let wanted = checked_vec_add(origin, velocity)?;
    if let Some(position) = slide_target(map, policy, origin, wanted) {
        proposal.position = position;
        proposal.velocity = velocity;
        if velocity != SimVec2::ZERO {
            proposal.heading = SimAngle::atan2(velocity.y, velocity.x);
        }
        proposal.anim = AnimState::Walk;
    } else {
        proposal.velocity = SimVec2::ZERO;
    }
    gate_step_by_facing(hull, origin, proposal);
    Ok(refused)
}

/// The unit heading of a step toward `next`: legacy steering, or under the solver pipeline the
/// intent of plan `2026-11-20-contact-solver` (I-1 to I-4), whose one-pair side commitment is kept
/// in the proposal's memory.
fn proposed_heading(
    context: &StepContext<'_>,
    proposal: &mut MotionProposal,
    next: SimVec2,
    ignored: Option<EntityId>,
) -> Result<SimVec2, MovementMathError> {
    let Some(horizon_factor) = context.intent else {
        return steer_body(context.body, next, ignored, context.neighbours);
    };
    let decided = intent_heading(
        &IntentInput {
            body: context.body,
            next,
            speed: context.speed,
            ignored,
            neighbours: context.neighbours,
            horizon_factor,
            side: SideMemory {
                partner: proposal.memory.side_partner,
                left: proposal.memory.side_left,
            },
        },
        |obstacle| context.admits(obstacle),
    )?;
    proposal.memory.side_partner = decided.side.partner;
    proposal.memory.side_left = decided.side.left;
    Ok(decided.heading)
}

/// What a step that the disc probe refused becomes after [`slide_along_wall`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WallSlide {
    /// Take the step; it counts as refused when it keeps less than half the step asked for.
    Step {
        /// The step to take, world units per tick: the slide, or the step itself where no slide
        /// is admitted.
        velocity: SimVec2,
        /// Whether the step counts as refused, so the caller looks at its route.
        refused: bool,
    },
    /// Bodies block the step and the slide keeps less than [`WAIT_THRESHOLD`] of the budget:
    /// the unit waits.
    Wait {
        /// Whether the step counts as refused, as for [`WallSlide::Step`].
        refused: bool,
    },
}

/// Decides a refused step `velocity` given its slide along the wall `slid` (`None` when no slide
/// is admitted) and the unit's step budget `speed`, all in world units per tick; `crowded` says
/// whether bodies that will not yield cut the step before the wall did.
///
/// A slide that keeps at least half the step walks along the wall. One that keeps less meets the
/// wall nearly head-on and counts as refused, so the caller looks at its route (the half step at
/// which a blocked attacker starts its surround walk). In a crowd, a slide below the waiting
/// share of the budget is not taken at all: the unit waits, as one blocked by bodies does
/// (friendly-blocking D-1.3, R1), instead of creeping and turning by a sliver every tick
/// (evidence `2026-11-11-contact-and-missiles` §4a). A unit alone at a wall takes even a sliver:
/// it is heading into a corner it must round, and waiting would hold it there for good. A step that cannot slide keeps its velocity and is
/// refused; the centre rule's slide and the settle then complete it.
fn after_wall_slide(
    velocity: SimVec2,
    slid: Option<SimVec2>,
    speed: SimScalar,
    crowded: bool,
) -> WallSlide {
    let Some(slid) = slid else {
        return WallSlide::Step {
            velocity,
            refused: true,
        };
    };
    let half = velocity.length().saturating_mul(SimScalar::HALF);
    let refused = within_distance(slid, SimVec2::ZERO, i128::from(half.raw()));
    if crowded && is_waiting(slid, speed) {
        return WallSlide::Wait { refused };
    }
    WallSlide::Step {
        velocity: slid,
        refused,
    }
}

/// Whether a unit whose step budget is `speed` (world units per tick) and whose admissible step
/// is `velocity` waits this tick: a step no longer than [`WAIT_THRESHOLD`] of the budget.
fn is_waiting(velocity: SimVec2, speed: SimScalar) -> bool {
    let threshold = speed.saturating_mul(WAIT_THRESHOLD);
    within_distance(velocity, SimVec2::ZERO, i128::from(threshold.raw()))
}

/// Walks a blocked attacker round the crowd in its way to `next`, on the side it keeps (the
/// surround walk, plan `2026-11-09-friendly-blocking` D-2, bugs B-104 and B-105).
///
/// The walk starts when the forward part of the admissible step `velocity` is below half the
/// unit's step and a standing friend that will not step aside lies just ahead. The unit then steps
/// across its way to `next` at full speed, on the side chosen when it first walked for this
/// target ([`surround_side`]) and kept in [`MovementMemory::flank_left`] even as the nearest
/// blocker changes or the walk pauses. A side closed by a body or terrain for
/// [`FLANK_REVERSE_TICKS`] is given up for the other once; when that closes too, or the unit has
/// walked one full circle round `next` (D-3; for an attack-move without a target, its scan
/// range), the walk is spent and the unit waits. A walk spent because both sides closed walks
/// again when the side that closed last opens for a whole step ([`closed_side_reopens`]); any
/// spent walk walks again when the unit gains ground ([`note_queued_progress`]) or its target or
/// order changes (plan `2026-11-19-crowd-edge-steering-2` D-2). Only a free straight way in pauses
/// a walk in progress, and the unit then steps forward; the sides tried and the distance walked
/// count for the whole approach to one target, and a walk toward another target, or under a new
/// order, starts afresh. Returns the step to take; zero means wait (friendly-blocking plan
/// Amendment 4 records the changes to D-2 and D-3).
fn surround_walk(
    context: &StepContext<'_>,
    pathfinder: &mut Pathfinder,
    proposal: &mut MotionProposal,
    next: SimVec2,
    velocity: SimVec2,
    target: Option<EntityId>,
) -> Result<SimVec2, MovementMathError> {
    let StepContext {
        map,
        policy,
        speed,
        body,
        neighbours,
        ..
    } = *context;
    let memory = &mut proposal.memory;
    if memory.flank_target != target {
        end_surround_walk(memory);
        memory.flank_target = target;
    }
    let ahead = direction(next, body.pos)?;
    let half_pace = speed.saturating_mul(SimScalar::HALF);
    let mut forward_free = checked_dot(velocity, ahead)? >= half_pace;
    let mut velocity = velocity;
    if forward_free && memory.flank_active {
        // A walk in progress ends only when the straight way in is open, not when steering finds
        // a step that edges forward and back across the walk. Open means the whole straight step
        // is free, short by at most what contact nudges a standing body (`STANDING_DRIFT`): a
        // walk starts below half a step forward and ends at a full one, so a unit at the crowd's
        // edge, whose straight step is clipped to between the two, keeps walking instead of
        // stepping in and out of contact every tick (friendly-blocking plan Amendment 7). The
        // way must also hold the room the walk's own retreat made, so a walk that stepped back
        // round a body does not end on that room and step straight back into the same contact
        // (plan `2026-11-19-crowd-edge-steering-2` D-1).
        let reach = speed.saturating_add(memory.flank_retreat);
        let straight = remove_blocked_velocity(body, checked_vec_scale(ahead, reach)?, neighbours)?;
        forward_free = admits_whole_step(straight, ahead, reach)?;
    } else if forward_free && memory.flank_walked > SimScalar::ZERO {
        // A unit that has walked round this crowd goes in straight while its walk pauses:
        // steering's step can lead back across the walk, and the next blocked tick sends the unit
        // out along it again, a step reversed every tick.
        velocity = remove_blocked_velocity(body, checked_vec_scale(ahead, speed)?, neighbours)?;
        forward_free = checked_dot(velocity, ahead)? >= half_pace;
    }
    if !forward_free && !memory.flank_active && !friend_blocks_ahead(context, ahead)? {
        return Ok(velocity);
    }
    if forward_free {
        // The unit steps forward. The sides it tried stay tried for this target, so a crowd that
        // opens for a tick and closes again cannot send it back and forth for ever.
        memory.flank_active = false;
        memory.flank_retreat = SimScalar::ZERO;
        memory.flank_blocked_ticks = 0;
        return Ok(velocity);
    }
    if memory.flank_spent {
        // A spent walk waits until the crowd beside it changes: the side that closed against it
        // opens, it gains ground, or its target changes. A boxed-in unit stands (Q0074).
        if !closed_side_reopens(context, pathfinder, memory, ahead, next, target)? {
            return Ok(SimVec2::ZERO);
        }
        end_surround_walk(memory);
    }
    if !memory.flank_active {
        memory.flank_active = true;
        if memory.flank_walked == SimScalar::ZERO && !memory.flank_reversed {
            memory.flank_left = surround_side(body, next, target, neighbours)?;
        }
    }
    let across = across_on_side(ahead, memory.flank_left)?;
    let step = remove_blocked_velocity(body, checked_vec_scale(across, speed)?, neighbours)?;
    let open = !is_waiting(step, speed)
        && probe_permits(
            map,
            pathfinder,
            policy,
            body.pos,
            checked_vec_add(body.pos, step)?,
        );
    if open {
        memory.flank_blocked_ticks = 0;
        memory.flank_walked = memory.flank_walked.saturating_add(step.length());
        memory.flank_retreat = add_retreat(memory.flank_retreat, step, ahead)?;
        // A chase walks at most one circle round its target (D-3). An attack-move without a
        // target walks to bring an enemy ahead into scan; one that has walked its scan range
        // sideways and still sees none would only stretch the line (Amendment 4).
        if memory.flank_walked >= walk_bound(context, next, target)? {
            memory.flank_active = false;
            memory.flank_spent = true;
        }
        return Ok(step);
    }
    memory.flank_blocked_ticks = memory.flank_blocked_ticks.saturating_add(1);
    if memory.flank_blocked_ticks >= FLANK_REVERSE_TICKS {
        memory.flank_blocked_ticks = 0;
        if memory.flank_reversed {
            memory.flank_active = false;
            memory.flank_spent = true;
        } else {
            memory.flank_reversed = true;
            memory.flank_left = !memory.flank_left;
        }
    }
    Ok(SimVec2::ZERO)
}

/// The distance a surround walk toward `next` may walk before it is spent, in world units: one
/// circle round `next` for a chase of `target` (D-3), the unit's scan range for an attack-move
/// without one (friendly-blocking plan Amendment 4).
fn walk_bound(
    context: &StepContext<'_>,
    next: SimVec2,
    target: Option<EntityId>,
) -> Result<SimScalar, MovementMathError> {
    Ok(match target {
        Some(_) => distance(context.body.pos, next)?.saturating_mul(SimScalar::TWO_PI),
        None => context.scan_range,
    })
}

/// The unit vector across the way `ahead` (a unit vector) on the walk's side: left of it when
/// `left` is true.
fn across_on_side(ahead: SimVec2, left: bool) -> Result<SimVec2, MovementMathError> {
    let across = SimVec2::new(-ahead.y, ahead.x);
    if left {
        Ok(across)
    } else {
        checked_vec_sub(SimVec2::ZERO, across)
    }
}

/// Whether the admissible step `step` keeps the whole of a step of `reach` world units along the
/// unit vector `direction`, short by at most what contact nudges a standing body
/// ([`STANDING_DRIFT`]): the test that ends a walk in progress (friendly-blocking plan
/// Amendment 7) and re-arms a spent one ([`closed_side_reopens`]).
fn admits_whole_step(
    step: SimVec2,
    direction: SimVec2,
    reach: SimScalar,
) -> Result<bool, MovementMathError> {
    Ok(checked_dot(step, direction)? >= reach.saturating_sub(STANDING_DRIFT))
}

/// Whether a spent surround walk re-arms because the side that closed last opens (plan
/// `2026-11-19-crowd-edge-steering-2` D-2).
///
/// Only a walk spent by closed sides qualifies: one that walked its bound ([`walk_bound`])
/// re-arms only on ground gained or a new target, since an open side would start the same circle
/// again. The side is the one the walk kept, [`MovementMemory::flank_left`], which is the side
/// that closed last; the other closed earlier and the unit has walked away from it, so it says
/// nothing about the crowd. The side opens when the across step of a whole `speed` keeps all but
/// [`STANDING_DRIFT`] of itself past the bodies ([`admits_whole_step`]) and the terrain permits
/// it: a body eased off a wall or a friend by contact gains slivers, never a whole step, so contact
/// creep does not re-arm the walk.
fn closed_side_reopens(
    context: &StepContext<'_>,
    pathfinder: &mut Pathfinder,
    memory: &MovementMemory,
    ahead: SimVec2,
    next: SimVec2,
    target: Option<EntityId>,
) -> Result<bool, MovementMathError> {
    let StepContext {
        map,
        policy,
        speed,
        body,
        neighbours,
        ..
    } = *context;
    if memory.flank_walked >= walk_bound(context, next, target)? {
        return Ok(false);
    }
    let across = across_on_side(ahead, memory.flank_left)?;
    let step = remove_blocked_velocity(body, checked_vec_scale(across, speed)?, neighbours)?;
    Ok(admits_whole_step(step, across, speed)?
        && probe_permits(
            map,
            pathfinder,
            policy,
            body.pos,
            checked_vec_add(body.pos, step)?,
        ))
}

/// `retreat` plus the backward part of the walk step `step` along the way in `ahead` (a unit
/// vector), `max(0, -step · ahead)`, in world units; saturating (plan
/// `2026-11-19-crowd-edge-steering-2` D-1.1).
fn add_retreat(
    retreat: SimScalar,
    step: SimVec2,
    ahead: SimVec2,
) -> Result<SimScalar, MovementMathError> {
    let along = checked_dot(step, ahead)?;
    Ok(if along < SimScalar::ZERO {
        retreat.saturating_sub(along)
    } else {
        retreat
    })
}

/// Clears the surround walk for a new target, keeping the side last walked.
fn end_surround_walk(memory: &mut MovementMemory) {
    memory.flank_active = false;
    memory.flank_reversed = false;
    memory.flank_spent = false;
    memory.flank_blocked_ticks = 0;
    memory.flank_walked = SimScalar::ZERO;
    memory.flank_retreat = SimScalar::ZERO;
}

/// Whether a friend of the unit that stands ([`SteeringObstacle::is_standing`]) and will not step aside
/// lies within one step of its edge, on the side toward `ahead` (a unit vector).
fn friend_blocks_ahead(
    context: &StepContext<'_>,
    ahead: SimVec2,
) -> Result<bool, MovementMathError> {
    friend_ahead(context, ahead, SteeringObstacle::is_standing)
}

/// Whether a friend that holds its ground ([`SteeringObstacle::anchored`]: on hold, immobile, or
/// anchored in combat) lies within one step of the unit's edge, on the side toward `ahead`: a
/// tick the unit spends queued behind it is not a stalled tick (plan
/// `2026-11-19-crowd-edge-steering` D-4, Q0089 answered yes).
fn friend_holds_ahead(
    context: &StepContext<'_>,
    ahead: SimVec2,
) -> Result<bool, MovementMathError> {
    friend_ahead(context, ahead, |obstacle| obstacle.anchored)
}

/// Whether a friend of the unit for which `holds` is true, and that will not step aside, lies
/// within one step of its edge, on the side toward `ahead` (a unit vector).
fn friend_ahead(
    context: &StepContext<'_>,
    ahead: SimVec2,
    holds: impl Fn(&SteeringObstacle) -> bool,
) -> Result<bool, MovementMathError> {
    let (body, speed) = (context.body, context.speed);
    for obstacle in context.neighbours {
        if obstacle.id == body.id
            || obstacle.owner != body.owner
            || !holds(obstacle)
            || obstacle.yields_to(body.owner, body.push_priority)
        {
            continue;
        }
        let reach = i128::from(body.radius.raw())
            + i128::from(obstacle.radius.raw())
            + i128::from(speed.raw());
        if within_distance(body.pos, obstacle.pos, reach)
            && difference_product(ahead, obstacle.pos, body.pos, false)? > 0
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Undoes a sidestep that would lose sight of `next` across a cliff edge.
///
/// Only the stretch of leg just ahead is compared, which is all a single step can affect.
fn keep_terrain_view(
    context: &StepContext<'_>,
    pathfinder: &mut Pathfinder,
    next: SimVec2,
    gap: SimScalar,
    velocity: SimVec2,
) -> Result<SimVec2, MovementMathError> {
    let (map, origin) = (context.map, context.body.pos);
    let straight = direction(next, origin)?;
    let straight_velocity = checked_vec_scale(straight, context.speed)?;
    if velocity == straight_velocity {
        return Ok(velocity);
    }
    let horizon = checked_vec_add(
        origin,
        checked_vec_scale(straight, gap.min(TERRAIN_GUARD_HORIZON))?,
    )?;
    let stepped = checked_vec_add(origin, velocity)?;
    let loses_view = probe_permits(map, pathfinder, context.policy, origin, stepped)
        && map.ground_segment_is_connected(origin, horizon, |_, _| true)
        && !map.ground_segment_is_connected(stepped, horizon, |_, _| true);
    Ok(if loses_view {
        straight_velocity
    } else {
        velocity
    })
}

#[cfg(test)]
mod tests {
    use super::{after_wall_slide, hull_allows_step, WallSlide};
    use sim_types::{SimAngle, SimScalar, SimVec2};

    fn v(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    /// A flat 40 × 40 world with a 3×3 structure at cells (18, 18) and, beside its west face,
    /// two ground units of different collision radii; returns the world, the structure and the
    /// larger unit radius.
    fn structure_and_units(
    ) -> Result<(crate::SimWorld, sim_types::EntityId, SimScalar), Box<dyn std::error::Error>> {
        use sim_types::{MoverType, PlayerId, ShapeKind};
        let map = sim_map::GameMap::new(
            40,
            40,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let mut world = crate::test_support::world(map, 0x0B_131);
        let structure_type = world
            .catalog
            .archetypes()
            .find(|a| {
                matches!(
                    a.shape,
                    ShapeKind::BuildingDome {
                        footprint_w: 3,
                        footprint_h: 3,
                        ..
                    }
                )
            })
            .map(|a| a.unit_type_id)
            .ok_or("a 3x3 structure")?;
        let ground: Vec<_> = world
            .catalog
            .archetypes()
            .filter(|a| {
                !matches!(a.shape, ShapeKind::BuildingDome { .. })
                    && a.mover_type == MoverType::Ground
                    && a.max_speed > SimScalar::ZERO
            })
            .map(|a| a.unit_type_id)
            .take(2)
            .collect();
        let [small, large] = ground[..] else {
            return Err("two ground units".into());
        };
        for (kind, radius) in [(small, 0.375), (large, 0.625)] {
            let arch = world.catalog.get_mut(kind)?;
            arch.collision_radius = SimScalar::from_num(radius);
            arch.terrain_radius = SimScalar::from_num(radius);
        }
        let structure = world.spawn_structure(structure_type, PlayerId(0), 18, 18)?;
        let at = |x: f64, y: f64| SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y));
        world.spawn_unit(small, PlayerId(0), at(17.4, 19.5))?;
        world.spawn_unit(large, PlayerId(1), at(14.0, 19.5))?;
        Ok((world, structure, SimScalar::from_num(0.625)))
    }

    /// Plan `2026-11-18-structure-contact` D-2 B2: after the movement snapshot, the ground
    /// obstacles hold the two units and not the registered structure (whose half-width 1.5
    /// exceeds both unit radii), and the largest ground radius is the larger unit's.
    #[test]
    fn a_registered_structure_is_no_steering_obstacle() -> Result<(), Box<dyn std::error::Error>> {
        let (mut world, structure, largest) = structure_and_units()?;
        let mut entries = Vec::new();
        world.prepare_movement_snapshot(&mut entries);
        assert!(world.drain_diagnostics().next().is_none());
        assert_eq!(entries.len(), 3, "the structure is still validated");
        let ids: Vec<_> = world.movement.obstacles[0].iter().map(|o| o.id).collect();
        assert_eq!(ids.len(), 2, "the two units: {ids:?}");
        assert!(!ids.contains(&structure), "{ids:?}");
        assert_eq!(world.movement.largest_radius[0], largest);
        Ok(())
    }

    /// Review F-2 (plan `2026-11-18-inner-radius` T5): a cast approach's goal outside the map is
    /// moved into the body's terrain-radius domain, so a body of radius 0.5 and terrain radius
    /// 0.3125 is clamped to 0.3125 from each edge, not 0.5.
    #[test]
    fn a_cast_goal_is_clamped_into_the_terrain_radius_domain(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use super::{clamp_into_domain, MotionPolicy, MovementLayer};
        let map = sim_map::GameMap::new(
            20,
            16,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let policy = MotionPolicy {
            layer: MovementLayer::Ground,
            radius: SimScalar::from_num(0.5),
            terrain: SimScalar::from_num(0.3125),
            class: sim_types::CollisionClass::GROUND,
        };
        assert_eq!(
            clamp_into_domain(policy, &map, v(-3.0, 0.1)),
            v(0.3125, 0.3125)
        );
        assert_eq!(
            clamp_into_domain(policy, &map, v(25.0, 30.0)),
            v(20.0 - 0.3125, 16.0 - 0.3125)
        );
        assert_eq!(clamp_into_domain(policy, &map, v(0.4, 9.0)), v(0.4, 9.0));
        Ok(())
    }

    /// A slide along the wall that keeps most of the step is taken and is not refused.
    #[test]
    fn a_slide_keeping_most_of_the_step_walks_along_the_wall() {
        let speed = SimScalar::from_num(0.126);
        let decided = after_wall_slide(v(0.0, 0.126), Some(v(0.1, 0.0)), speed, true);
        assert_eq!(
            decided,
            WallSlide::Step {
                velocity: v(0.1, 0.0),
                refused: false
            }
        );
    }

    /// A slide below half the step but above the waiting share is taken and counts as refused.
    #[test]
    fn a_short_slide_is_taken_and_counts_as_refused() {
        let speed = SimScalar::from_num(0.126);
        let decided = after_wall_slide(v(0.0, 0.126), Some(v(0.03, 0.0)), speed, true);
        assert_eq!(
            decided,
            WallSlide::Step {
                velocity: v(0.03, 0.0),
                refused: true
            }
        );
    }

    /// A slide below one eighth of the budget is not crept along: the unit waits (the queue at
    /// the ramp W wall end, plan `2026-11-11-contact-and-missiles` evidence §4).
    #[test]
    fn a_sliver_of_slide_waits_instead_of_creeping() {
        let speed = SimScalar::from_num(0.126);
        // One eighth of 0.126 is 0.01575; 0.0149 and 0.0105 are the steps the evidence traced.
        for sliver in [0.0149, 0.0105, 0.0001] {
            let decided = after_wall_slide(v(0.0, 0.126), Some(v(sliver, 0.0)), speed, true);
            assert_eq!(decided, WallSlide::Wait { refused: true }, "{sliver}");
        }
        // A step that remove_blocked_velocity already cut short keeps its sliver unrefused.
        let decided = after_wall_slide(v(0.0, 0.02), Some(v(0.0149, 0.0)), speed, true);
        assert_eq!(decided, WallSlide::Wait { refused: false });
        // The share is of the budget: a slower budget walks the slide a faster one waits on.
        let slow = SimScalar::from_num(0.08);
        let decided = after_wall_slide(v(0.0, 0.08), Some(v(0.0149, 0.0)), slow, true);
        assert!(
            matches!(decided, WallSlide::Step { refused: true, .. }),
            "{decided:?}"
        );
    }

    /// A unit alone at a wall takes even a sliver of slide: it must round the corner it heads
    /// into, and waiting would hold it there (a rhino beetle at the ramp W wall end of
    /// `test_live_battle_scenario_dual_ramp_traversal`).
    #[test]
    fn a_sliver_alone_at_a_wall_is_taken() {
        let speed = SimScalar::from_num(0.126);
        let decided = after_wall_slide(v(0.0, 0.126), Some(v(0.0105, 0.0)), speed, false);
        assert_eq!(
            decided,
            WallSlide::Step {
                velocity: v(0.0105, 0.0),
                refused: true
            }
        );
    }

    /// A step with no admitted slide keeps its velocity and is refused.
    #[test]
    fn a_step_without_a_slide_is_refused_unchanged() {
        let speed = SimScalar::from_num(0.126);
        let decided = after_wall_slide(v(0.1, 0.05), None, speed, true);
        assert_eq!(
            decided,
            WallSlide::Step {
                velocity: v(0.1, 0.05),
                refused: true
            }
        );
    }

    /// A friend of radius 0.375 holding its ground at `pos`.
    fn holder(pos: SimVec2) -> crate::steering::SteeringObstacle {
        use sim_types::{EntityId, PlayerId};
        crate::steering::SteeringObstacle {
            owner: PlayerId(0),
            push_priority: 10,
            anchored: true,
            idle: false,
            ..crate::steering::SteeringObstacle::new(
                EntityId(2),
                pos,
                SimScalar::from_num(0.375),
                v(0.0, 0.0),
            )
        }
    }

    /// Runs `probe` on the step context of a unit of radius 0.375 and step 0.126 at (10, 10) on
    /// open ground, chasing entity 99 toward (20, 10), with `neighbours` round it: its way in is
    /// +x and its walk's sides are ±y (left is +y).
    fn on_walk_context<R>(
        neighbours: &[crate::steering::SteeringObstacle],
        probe: impl FnOnce(&super::StepContext<'_>, &mut sim_map::Pathfinder) -> R,
    ) -> Result<R, Box<dyn std::error::Error>> {
        use super::{MotionPolicy, MovementLayer, StepContext};
        use crate::steering::ContactBody;
        use sim_types::{EntityId, PlayerId};
        let map = sim_map::GameMap::new(
            40,
            40,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let mut world = crate::test_support::world(map, 0x0CE_2D2);
        let radius = SimScalar::from_num(0.375);
        let context = StepContext {
            map: &world.map,
            policy: MotionPolicy {
                layer: MovementLayer::Ground,
                radius,
                terrain: radius,
                class: sim_types::CollisionClass::GROUND,
            },
            speed: SimScalar::from_num(0.126),
            body: ContactBody {
                id: EntityId(1),
                owner: PlayerId(0),
                push_priority: 10,
                pos: v(10.0, 10.0),
                radius,
            },
            neighbours,
            hull: None,
            surround: Some(Some(EntityId(99))),
            scan_range: SimScalar::from_num(5.5),
            intent: None,
            near_margin: SimScalar::from_num(8),
        };
        Ok(probe(&context, &mut world.pathfinder))
    }

    /// A body of radius 0.375 at `pos` with tick-start velocity `velocity`, under an order (not
    /// idle) unless `idle`, so it does not yield to the unit of [`on_walk_context`] unless it is
    /// an idle, standing friend.
    fn corner_body(
        pos: SimVec2,
        velocity: SimVec2,
        idle: bool,
    ) -> crate::steering::SteeringObstacle {
        use sim_types::{EntityId, PlayerId};
        crate::steering::SteeringObstacle {
            owner: PlayerId(0),
            push_priority: 10,
            anchored: false,
            idle,
            ..crate::steering::SteeringObstacle::new(
                EntityId(2),
                pos,
                SimScalar::from_num(0.375),
                velocity,
            )
        }
    }

    /// Whether `parked_on_corner` finds `body` on the corner (10.6, 10) of the unit of
    /// [`on_walk_context`] (radius 0.375 at (10, 10)), with the unit `rounding` or not.
    fn finds_on_corner(
        body: crate::steering::SteeringObstacle,
        rounding: bool,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let neighbours = [body];
        on_walk_context(&neighbours, |context, _| {
            super::parked_on_corner(context, v(10.6, 10.0), rounding).is_some()
        })
    }

    /// A walking body on the corner starts no rounding, but keeps one begun (plan
    /// `2026-11-19-parked-body-aim` D-1): the standing test applies only when not `rounding`.
    #[test]
    fn a_walking_body_on_the_corner_is_kept_only_by_a_rounding_begun(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let walking = corner_body(v(10.9, 10.0), v(0.1, 0.0), false);
        assert!(!finds_on_corner(walking, false)?);
        assert!(finds_on_corner(walking, true)?);
        let standing = corner_body(v(10.9, 10.0), v(0.0, 0.0), false);
        assert!(finds_on_corner(standing, false)?);
        assert!(finds_on_corner(standing, true)?);
        Ok(())
    }

    /// An idle, standing friend yields (contact pushes it aside), so it is never rounded.
    #[test]
    fn a_yielding_body_is_never_parked() -> Result<(), Box<dyn std::error::Error>> {
        let idle = corner_body(v(10.9, 10.0), v(0.0, 0.0), true);
        assert!(!finds_on_corner(idle, false)?);
        assert!(!finds_on_corner(idle, true)?);
        Ok(())
    }

    /// A body that does not touch the corner with both radii (0.75) is never parked on it,
    /// standing or walking; a walking one just within that reach keeps a rounding begun.
    #[test]
    fn a_body_off_the_corner_is_never_parked() -> Result<(), Box<dyn std::error::Error>> {
        for velocity in [v(0.0, 0.0), v(0.1, 0.0)] {
            let off = corner_body(v(11.4, 10.0), velocity, false);
            assert!(!finds_on_corner(off, false)?);
            assert!(!finds_on_corner(off, true)?);
        }
        let within = corner_body(v(11.3, 10.0), v(0.1, 0.0), false);
        assert!(finds_on_corner(within, true)?);
        Ok(())
    }

    /// Whether the spent walk of the unit of [`on_walk_context`], having walked `walked` world
    /// units with its kept side -y (`flank_left` false), re-arms on that side; `blocker` is a
    /// friend holding its ground that far below the unit's edge, if any.
    fn reopens(walked: f64, blocker: Option<f64>) -> Result<bool, Box<dyn std::error::Error>> {
        use super::closed_side_reopens;
        use crate::entity::MovementMemory;
        use sim_types::EntityId;
        let neighbours: Vec<_> = blocker
            .map(|gap| holder(v(10.0, 10.0 - 0.75 - gap)))
            .into_iter()
            .collect();
        let memory = MovementMemory {
            flank_spent: true,
            flank_reversed: true,
            flank_left: false,
            flank_walked: SimScalar::from_num(walked),
            flank_target: Some(EntityId(99)),
            ..MovementMemory::default()
        };
        Ok(on_walk_context(&neighbours, |context, pathfinder| {
            closed_side_reopens(
                context,
                pathfinder,
                &memory,
                v(1.0, 0.0),
                v(20.0, 10.0),
                Some(EntityId(99)),
            )
        })??)
    }

    /// One tick of an active surround walk for the unit of [`on_walk_context`], whose steering
    /// step is the whole straight step (so only the walk's end test decides), with a friend
    /// holding its ground `gap` ahead of its edge on its way in and `retreat` world units of
    /// retreat on its walk. Returns the step and the memory after the tick.
    fn walk_tick(
        gap: f64,
        retreat: f64,
    ) -> Result<(SimVec2, crate::entity::MovementMemory), Box<dyn std::error::Error>> {
        use super::{surround_walk, MotionProposal};
        use crate::entity::MovementMemory;
        use sim_types::{AnimState, EntityId};
        let neighbours = [holder(v(10.0 + 0.75 + gap, 10.0))];
        let mut proposal = MotionProposal {
            position: v(10.0, 10.0),
            velocity: v(0.0, 0.0),
            heading: SimAngle::ZERO,
            anim: AnimState::Walk,
            clear_route: false,
            replacement: None,
            remove_prefix: 0,
            pop_order: false,
            retarget: None,
            clear_target: false,
            memory: MovementMemory {
                flank_active: true,
                flank_left: true,
                flank_walked: SimScalar::from_num(0.5),
                flank_retreat: SimScalar::from_num(retreat),
                flank_target: Some(EntityId(99)),
                ..MovementMemory::default()
            },
        };
        let step = on_walk_context(&neighbours, |context, pathfinder| {
            surround_walk(
                context,
                pathfinder,
                &mut proposal,
                v(20.0, 10.0),
                v(0.126, 0.0),
                Some(EntityId(99)),
            )
        })??;
        Ok((step, proposal.memory))
    }

    /// Plan `2026-11-19-crowd-edge-steering-2` D-1.2: with no retreat, a walk in progress ends
    /// exactly when the whole straight step is free less `STANDING_DRIFT` (0.126 - 1/64 =
    /// 0.110 375), as friendly-blocking Amendment 7 has it; with a retreat, the straight way must
    /// also hold the retreat. An ended walk steps straight in and clears its retreat; a walk
    /// that goes on keeps it.
    #[test]
    fn a_walk_ends_only_on_room_beyond_its_retreat() -> Result<(), Box<dyn std::error::Error>> {
        for (gap, retreat, ends) in [
            (0.110, 0.0, false),
            (0.1104, 0.0, true),
            (0.5, 0.0, true),
            (0.1104, 0.05, false),
            (0.160, 0.05, false),
            (0.1604, 0.05, true),
            (0.5, 0.05, true),
        ] {
            let (step, memory) = walk_tick(gap, retreat)?;
            assert_eq!(!memory.flank_active, ends, "gap {gap}, retreat {retreat}");
            if ends {
                assert_eq!(step, v(0.126, 0.0), "gap {gap}: the unit steps straight in");
                assert_eq!(
                    memory.flank_retreat,
                    SimScalar::ZERO,
                    "cleared by the forward step"
                );
            } else {
                assert_eq!(
                    memory.flank_retreat,
                    SimScalar::from_num(retreat),
                    "gap {gap}"
                );
                assert!(
                    step.y > SimScalar::ZERO,
                    "it walks on its left side: {step:?}"
                );
            }
        }
        Ok(())
    }

    /// D-1.1: the retreat sums only the backward parts of the walk's steps along the way in.
    #[test]
    fn the_retreat_sums_only_backward_parts() -> Result<(), Box<dyn std::error::Error>> {
        use super::add_retreat;
        let ahead = v(1.0, 0.0);
        let mut retreat = SimScalar::ZERO;
        for step in [
            v(-0.03, 0.12),
            v(0.05, 0.11),
            v(0.0, -0.126),
            v(-0.02, -0.12),
        ] {
            retreat = add_retreat(retreat, step, ahead)?;
        }
        assert_eq!(
            retreat,
            SimScalar::from_num(0.03).saturating_add(SimScalar::from_num(0.02))
        );
        assert_eq!(
            add_retreat(SimScalar::MAX, v(-1.0, 0.0), ahead)?,
            SimScalar::MAX,
            "saturating"
        );
        Ok(())
    }

    /// D-1.1: the retreat is cleared when the walk ends ([`super::end_surround_walk`]) and by a
    /// new order ([`crate::entity::MovementMemory::restart`]).
    #[test]
    fn the_retreat_is_cleared_by_the_end_of_the_walk_and_a_new_order() {
        use crate::entity::MovementMemory;
        let walking = MovementMemory {
            flank_active: true,
            flank_walked: SimScalar::from_num(1.0),
            flank_retreat: SimScalar::from_num(0.2),
            ..MovementMemory::default()
        };
        let mut ended = walking;
        super::end_surround_walk(&mut ended);
        assert_eq!(ended.flank_retreat, SimScalar::ZERO);
        let mut restarted = walking;
        restarted.restart();
        assert_eq!(restarted.flank_retreat, SimScalar::ZERO);
    }

    /// Plan `2026-11-19-crowd-edge-steering-2` D-2: a walk spent because its sides closed
    /// re-arms when the side it kept frees a whole step; one spent by walking its circle round
    /// the target (2π × 10 world units here) does not, however open its side, since re-arming it
    /// would start the same circle again.
    #[test]
    fn a_walk_spent_by_its_circle_does_not_re_arm_on_an_open_side(
    ) -> Result<(), Box<dyn std::error::Error>> {
        assert!(reopens(1.0, None)?, "closed sides, side now open");
        assert!(!reopens(62.9, None)?, "circle walked (2π × 10 = 62.83)");
        assert!(!reopens(70.0, None)?, "beyond the circle");
        Ok(())
    }

    /// D-2.3: the kept side opens only for a whole step. A friend holding a sliver below the
    /// unit's edge keeps it closed even where the sliver is above the waiting share (prototype
    /// K3's test); room for the whole step less `STANDING_DRIFT` opens it.
    #[test]
    fn a_closed_side_opens_only_for_a_whole_step() -> Result<(), Box<dyn std::error::Error>> {
        // The waiting share is 0.126 / 8 = 0.01575; the whole step less the drift is
        // 0.126 - 1/64 = 0.110 375.
        for sliver in [0.0, 0.01, 0.03, 0.08, 0.11] {
            assert!(!reopens(1.0, Some(sliver))?, "gap {sliver}");
        }
        for room in [0.111, 0.126, 0.5] {
            assert!(reopens(1.0, Some(room))?, "gap {room}");
        }
        Ok(())
    }

    /// The shipped slow hull rate, 20.16° per tick, in binary units.
    const RATE: u16 = 3_670;

    #[test]
    fn a_step_within_the_budget_is_allowed() {
        let heading = SimAngle::from_raw(1_000);
        for offset in [0, 1, RATE - 1, RATE] {
            let left = SimAngle::from_raw(1_000 + offset);
            let right = SimAngle::from_raw(1_000_u16.wrapping_sub(offset));
            assert!(hull_allows_step(heading, left, RATE), "+{offset}");
            assert!(hull_allows_step(heading, right, RATE), "-{offset}");
        }
    }

    #[test]
    fn a_step_beyond_the_budget_is_refused() {
        let heading = SimAngle::ZERO;
        assert!(!hull_allows_step(
            heading,
            SimAngle::from_raw(RATE + 1),
            RATE
        ));
        assert!(!hull_allows_step(
            heading,
            SimAngle::from_raw(0_u16.wrapping_sub(RATE + 1)),
            RATE
        ));
        assert!(!hull_allows_step(heading, SimAngle::DEG_180, RATE));
    }

    #[test]
    fn a_half_turn_budget_allows_any_step() {
        let half = SimAngle::DEG_180.raw();
        for heading in [0_u16, 1, 16_384, 32_767, 32_768, 65_535] {
            let heading = SimAngle::from_raw(heading);
            // Both signs of `diff`: a half turn reads as -32 768, its neighbours as ±32 767.
            for offset in [32_767_u16, 32_768, 32_769] {
                let direction = SimAngle::from_raw(heading.raw().wrapping_add(offset));
                assert!(
                    hull_allows_step(heading, direction, half),
                    "{heading:?} +{offset}"
                );
            }
        }
    }
}
