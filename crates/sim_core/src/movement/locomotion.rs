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
//! Pursuit obeys the same route, steering and contact rules. Only its goal moves.
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

use super::grid::BodyGrid;
use super::math::{
    checked_dot, checked_vec_add, checked_vec_scale, checked_vec_sub, difference_product,
    direction, distance, within_distance,
};
use super::tuning::{
    ABANDONED_TARGET_TICKS, FLANK_RETRY_TICKS, FLANK_REVERSE_TICKS, PURSUIT_LEASH_SCAN_RANGES,
    PURSUIT_ROUTE_DRIFT, REPATH_COOLDOWN_TICKS, ROUTE_CHECK_PERIOD, ROUTE_START_TOLERANCE,
    STALL_ABANDON_TICKS, STALL_ARRIVAL_REACH, STALL_PROGRESS, STALL_REPATH_TICKS, STANDING_DRIFT,
    STEERING_LOOKAHEAD, TERRAIN_GUARD_HORIZON, WAIT_THRESHOLD, WAYPOINT_HANDOFF_MARGIN,
};
use super::{
    MotionPolicy, MovementInvariant, MovementLayer, MovementMathError, MovementWorkspace,
    RouteOutcome, RouteOutcomeKind,
};
use crate::entity::{Entity, MovementMemory};
use crate::steering::{
    check_cluster_arrival_with, remove_blocked_velocity, steer_body, surround_side, ContactBody,
    SteeringObstacle,
};
use crate::{SimWorld, WeaponPhase};
use sim_map::{MapError, PathRequestWithVision, PathResult, Pathfinder, PlayerVisionMap, SimMap};
use sim_types::{
    AnimState, AttackOrigin, EntityId, PerPlayer, PlayerSlot, SimAngle, SimScalar, SimVec2, Tick,
    UnitOrder, MAX_PLAYERS,
};
use std::ops::ControlFlow;

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
            if let Some(UnitOrder::Move { target } | UnitOrder::AttackMove { target }) =
                actor.order_queue.front_mut()
            {
                *target = destination;
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

enum MotionAction {
    Commit(MotionProposal),
    Pursue(EntityId, AttackOrigin),
    Cast(CastRequest),
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

/// Collects the bodies that can affect `entry` this tick from the tick-start snapshot.
///
/// Bodies across a cliff are left out: ground units cannot touch through terrain.
// PERFORMANCE RATIONALE:
// Why: Scanning every body per unit is O(N^2): 160,000 pair tests per tick at 400 units.
// What: Read candidates from a grid built once at the start of the phase, and copy the few that
// pass an exact distance test into one retained buffer.
// Trade-off: Copies ~56-byte records per neighbour, in exchange for flat scans downstream and
// O(N) work per tick; `out` stops growing once the densest crowd has been seen.
fn gather_neighbours(
    index: &NeighbourIndex<'_>,
    map: &SimMap,
    entry: MotionEntry,
    position: SimVec2,
    out: &mut Vec<SteeringObstacle>,
) {
    out.clear();
    let margin = STEERING_LOOKAHEAD + entry.speed + entry.speed;
    let reach = entry.policy.radius + index.largest_radius + margin;
    index.grid.for_each_near(position, reach, |body| {
        let obstacle = &index.obstacles[body];
        if obstacle.id == entry.id {
            return;
        }
        let touching = i128::from(entry.policy.radius.raw())
            + i128::from(obstacle.radius.raw())
            + i128::from(margin.raw());
        if within_distance(position, obstacle.pos, touching)
            && (entry.policy.layer == MovementLayer::Air
                || on_connected_ground(map, position, obstacle.pos))
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
        radius: policy.radius,
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
        if self.map.structures().contains_key(&id) && !actor.is_immobile {
            return Err(MovementInvariant::InvalidState {
                entity: id,
                reason: "registered foundation cannot author self-motion without atomic relocation",
            });
        }
        if !policy.contains(&self.map, actor.position) {
            return Err(MovementInvariant::InvalidState {
                entity: id,
                reason: "actor is outside its current full-radius movement domain",
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
            actor
                .current_target
                .or(match actor.order_queue.front() {
                    Some(UnitOrder::AttackTarget { target, .. }) => Some(*target),
                    _ => None,
                })
                .is_some_and(|id| self.entities.get(&id).is_some_and(Entity::is_alive))
        };
        matches!(
            actor.weapon_phase,
            WeaponPhase::RotatingToTarget { .. }
                | WeaponPhase::Windup { .. }
                | WeaponPhase::Backswing { .. }
        ) || (actor.velocity == SimVec2::ZERO && front_attacks() && target_lives())
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
            anchored: actor.cannot_be_pushed() || self.is_anchored_in_combat(actor),
            idle: actor.is_idle(),
            settled_on: actor.movement.arrived_at.filter(|_| idle),
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
        self.movement.largest_radius = [SimScalar::ZERO; super::BODY_CLASSES];
        for layer in &mut self.movement.obstacles {
            layer.clear();
        }
        for &id in &ids {
            match self.motion_entry(id) {
                Ok(entry) => {
                    // A body without collision (mining, or a status's suppression) is no
                    // obstacle anyone steers around.
                    if let Some(actor) = self.entities.get(&id).filter(|a| {
                        !a.is_mineral_walking && !a.status_modifiers().suppress_collision
                    }) {
                        let obstacle = self.obstacle_of(entry, actor);
                        let class = entry.policy.body_class();
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
        for class in 0..super::BODY_CLASSES {
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
        for &entry in &entries {
            match self.prepare_actor_motion(entry) {
                Ok(MotionAction::Commit(proposal)) => {
                    if let Some(actor) = self.entities.get_mut(&entry.id) {
                        proposal.commit(actor, entry.turn_rate);
                    }
                }
                Ok(MotionAction::Pursue(target, origin)) => {
                    pursuits.push((entry, target, origin));
                }
                Ok(MotionAction::Cast(cast)) => casts.push((entry, cast)),
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
                Ok(proposal) => {
                    if let Some(actor) = self.entities.get_mut(&entry.id) {
                        proposal.commit(actor, entry.turn_rate);
                    }
                }
                Err(error) => self.report_movement_invariant(error),
            }
        }
        self.movement.entries = entries;
        self.movement.entries_are_fresh = true;
        self.movement.pursuits = pursuits;
        self.movement.casts = casts;
        self.movement.recheck_routes = PerPlayer::from_array([false; MAX_PLAYERS]);
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
                reason: "active movement order is outside the current full-radius domain",
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
        };
        let mut neighbours = std::mem::take(&mut scene.movement.neighbours);
        let walked = draft.walk(&mut scene, &mut neighbours, check_route, map_news);
        scene.movement.neighbours = neighbours;
        walked?;
        Ok(MotionAction::Commit(draft.proposal))
    }

    /// Settles an actor that does not walk this tick, or finds the point its order walks to.
    ///
    /// Breaks with the finished action for a unit that holds, winds up a fixed weapon, pursues,
    /// casts or has nothing to do. Continues with the proposal and the goal in world units.
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
        let arch = self
            .catalog
            .get(actor.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(id))?;
        let has_turret = actor
            .siege_weapon
            .as_ref()
            .or(arch.ground_weapon.as_ref())
            .or(arch.weapon.as_ref())
            .is_some_and(|w| w.has_turret);
        if matches!(actor.weapon_phase, WeaponPhase::Windup { .. }) && !has_turret {
            proposal.velocity = SimVec2::ZERO;
            return Ok(ControlFlow::Break(MotionAction::Commit(proposal)));
        }
        let target = match actor.order_queue.front() {
            Some(UnitOrder::Move { target } | UnitOrder::AttackMove { target }) => *target,
            Some(_) if actor.has_task_order() && actor.task.goal.is_some() => {
                actor.task.goal.unwrap_or(actor.position)
            }
            Some(UnitOrder::AttackTarget { target, origin }) => {
                return Ok(ControlFlow::Break(MotionAction::Pursue(*target, *origin)))
            }
            Some(UnitOrder::CastAbility {
                ability_index,
                target_pos,
                target_entity,
            }) => {
                return Ok(ControlFlow::Break(MotionAction::Cast((
                    id,
                    *ability_index,
                    *target_pos,
                    *target_entity,
                ))))
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
    /// The range check runs first and needs the whole world. The chase then runs in stages over a
    /// [`MotionDraft`], like walking, with the target's position as the goal.
    fn prepare_pursuit(
        &mut self,
        entry: MotionEntry,
        target_id: EntityId,
        origin: AttackOrigin,
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
        let mut proposal = MotionProposal::new(actor, policy);
        if !self.is_entity_available_for_combat(actor.owner, target_id) {
            proposal.end_attack();
            self.coast(entry, actor, &mut proposal)?;
            return Ok(proposal);
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
            return Ok(proposal);
        };
        if pursuit.aim_if_in_range(id, actor, target.position, &mut proposal)? {
            return Ok(proposal);
        }

        let goal = target.position;
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
        };
        let mut neighbours = std::mem::take(&mut scene.movement.neighbours);
        let chased = draft.chase(&mut scene, &mut neighbours, &pursuit);
        scene.movement.neighbours = neighbours;
        chased?;
        Ok(draft.proposal)
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
                .then_some(actor.siege_weapon.as_ref())
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
            target_moving: target.velocity != SimVec2::ZERO,
            has_turret: weapon.has_turret,
            max,
            min,
            scan_range: arch.scan_range,
        }))
    }
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
    /// The pursuer's scan range in world units, which sizes the leash of an automatic chase.
    scan_range: SimScalar,
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
}

impl MotionScene<'_> {
    /// Collects the tick-start bodies near `position` of `entry`'s body class into `out`.
    fn gather_neighbours(
        &self,
        entry: MotionEntry,
        position: SimVec2,
        out: &mut Vec<SteeringObstacle>,
    ) {
        let class = entry.policy.body_class();
        gather_neighbours(
            &NeighbourIndex {
                grid: &self.movement.grids[class],
                obstacles: &self.movement.obstacles[class],
                largest_radius: self.movement.largest_radius[class],
            },
            self.map,
            entry,
            position,
            out,
        );
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
        let context = StepContext {
            surround: (self.entry.surround_walk && attack_moving).then_some(None),
            ..self.step_context(scene.map, neighbours)
        };
        let on_last_leg = match self.decide_arrival(&context, scene.pathfinder)? {
            ControlFlow::Continue(on_last_leg) => on_last_leg,
            ControlFlow::Break(()) => return Ok(()),
        };
        let (next, gap) = self.next_waypoint(&context)?;
        let routes = &mut scene.movement.route_outcomes;
        if self
            .check_progress(routes, on_last_leg, gap, map_news)?
            .is_break()
        {
            return Ok(());
        }
        self.propose_step(&context, next, gap)
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
            && policy.permits_segment(scene.map, actor.position, self.goal)
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
        if on_last_leg
            && check_cluster_arrival_with(
                Some(id),
                actor.position,
                self.entry.policy.radius,
                goal,
                context.neighbours,
                |witness| {
                    witness.owner == actor.owner
                        && (!ground || on_connected_ground(context.map, witness.pos, goal))
                },
            )
            .map_err(self.fault())?
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
                let aim = aim_past_parked_body(context, corner, subsequent, &mut sense)
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
    fn check_progress(
        &mut self,
        outcomes: &mut Vec<RouteOutcome>,
        on_last_leg: bool,
        gap: SimScalar,
        map_news: bool,
    ) -> Result<Stage, MovementInvariant> {
        if self.entry.speed > SimScalar::ZERO {
            if map_news || self.proposal.memory.flank_active {
                // A surround walk gains ground round the crowd, not along the route (D-3).
                self.proposal.memory.progress_best = None;
            }
            let route = self.proposal.route(self.actor);
            let mut memory = self.proposal.memory;
            let stalled = note_progress(&mut memory, self.actor.position, route, self.goal)
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
        next: SimVec2,
        gap: SimScalar,
    ) -> Result<(), MovementInvariant> {
        let (policy, actor, map) = (self.entry.policy, self.actor, context.map);
        let refused =
            step_toward(context, &mut self.proposal, next, gap, None).map_err(self.fault())?;
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
            && policy.permits_segment(map, next, self.goal)
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
        // Brake to stop on the edge of the weapon's band rather than within a tick (B-39).
        let reach = SimScalar::from_raw(i64::try_from(pursuit.max).unwrap_or(i64::MAX));
        self.brake(reach)?;
        scene.gather_neighbours(self.entry, self.actor.position, neighbours);
        let context = StepContext {
            surround: self.entry.surround_walk.then_some(Some(pursuit.target)),
            ..self.step_context(scene.map, neighbours)
        };
        drop_passed_waypoints(
            &context,
            scene.pathfinder,
            &self.request,
            self.actor,
            &mut self.proposal,
        )
        .map_err(self.fault())?;
        let (next, gap) = self.next_waypoint(&context)?;
        let routes = &mut scene.movement.route_outcomes;
        if self.check_pursuit_progress(routes, pursuit)?.is_break() {
            return Ok(());
        }
        let refused = step_toward(
            &context,
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
    fn check_pursuit_progress(
        &mut self,
        outcomes: &mut Vec<RouteOutcome>,
        pursuit: &Pursuit,
    ) -> Result<Stage, MovementInvariant> {
        if self.entry.speed > SimScalar::ZERO {
            if pursuit.target_moving || self.proposal.memory.flank_active {
                // A moving goal says nothing about whether this unit is stuck, and a surround
                // walk gains ground round the crowd, not toward the target (D-3).
                self.proposal.memory.progress_best = None;
            }
            let route = self.proposal.route(self.actor);
            let mut memory = self.proposal.memory;
            let stalled = note_progress(&mut memory, self.actor.position, route, self.goal)
                .map_err(self.fault())?;
            self.proposal.memory = memory;
            if stalled >= STALL_ABANDON_TICKS {
                self.abandon_pursuit(outcomes, pursuit.target);
                return Ok(Stage::Break(()));
            }
        }
        Ok(Stage::Continue(()))
    }

    /// Searches a new route after terrain refused a chasing step, and gives the target up when
    /// no route reaches firing range, or an automatic chase's route outruns its leash.
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
        let leash = pursuit
            .scan_range
            .checked_mul(PURSUIT_LEASH_SCAN_RANGES)
            .unwrap_or(SimScalar::MAX);
        match path {
            Some(path)
                if pursuit.origin == AttackOrigin::Explicit || path.total_length <= leash =>
            {
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

    /// Reports the chase of `target` as abandoned and leaves that target alone for a while.
    fn abandon_pursuit(&mut self, outcomes: &mut Vec<RouteOutcome>, target: EntityId) {
        outcomes.push(RouteOutcome {
            tick: self.tick,
            entity: self.entry.id,
            kind: RouteOutcomeKind::PursuitAbandoned { target },
        });
        self.proposal.abandon_target(target);
    }
}

/// The body parked on `corner`, if any: a tick-start neighbour standing still that will not step
/// aside for this unit and touches the corner point with its own radius and the unit's.
///
/// A moving body is not parked: it clears the corner by itself, and steering spaces the unit from
/// it like any other flockmate.
fn parked_on_corner<'n>(
    context: &StepContext<'n>,
    corner: SimVec2,
) -> Option<&'n SteeringObstacle> {
    let (body, policy) = (context.body, context.policy);
    context.neighbours.iter().find(|obstacle| {
        obstacle.velocity == SimVec2::ZERO
            && !obstacle.yields_to(body.owner, body.push_priority)
            && within_distance(
                obstacle.pos,
                corner,
                i128::from(policy.radius.raw()) + i128::from(obstacle.radius.raw()),
            )
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
/// side rule would send it back. The route is never written: this is a steering aim.
fn aim_past_parked_body(
    context: &StepContext<'_>,
    corner: SimVec2,
    subsequent: SimVec2,
    sense: &mut i8,
) -> Result<Option<SimVec2>, MovementMathError> {
    let (body, policy) = (context.body, context.policy);
    let Some(occupant) = parked_on_corner(context, corner) else {
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
    Ok(policy
        .permits_segment(context.map, body.pos, aim)
        .then_some(aim))
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
/// the body and the obstacle the corner turns around.
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
        let near = gap <= handoff && parked_on_corner(context, first).is_none();
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
/// does not step into the gap and get pushed back out, which vibrated (bug B-103).
///
/// Returns whether terrain refused the step as planned, so the caller can check its route.
fn step_toward(
    context: &StepContext<'_>,
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
    if gap <= speed && policy.permits_segment(map, origin, next) {
        proposal.position = next;
        proposal.velocity = checked_vec_sub(next, origin)?;
        proposal.anim = AnimState::Walk;
        gate_step_by_facing(hull, origin, proposal);
        return Ok(false);
    }
    let heading = steer_body(body, next, ignored, neighbours)?;
    let mut velocity = checked_vec_scale(heading, speed)?;
    if policy.layer == MovementLayer::Ground {
        velocity = keep_terrain_view(context, next, gap, velocity)?;
    }
    let mut velocity = remove_blocked_velocity(body, velocity, neighbours)?;
    if let Some(target) = context.surround {
        velocity = surround_walk(context, proposal, next, velocity, target)?;
    }
    if is_waiting(velocity, speed) {
        proposal.velocity = SimVec2::ZERO;
        proposal.anim = AnimState::Idle;
        return Ok(false);
    }
    let wanted = checked_vec_add(origin, velocity)?;
    let refused = !policy.permits_segment(map, origin, wanted);
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
/// range), the walk is spent: the unit waits [`FLANK_RETRY_TICKS`] and then walks afresh. Only a
/// free straight way in pauses a walk in progress, and the unit then steps forward; the sides
/// tried and the distance walked count for the whole approach to one target, and a walk toward
/// another target, or under a new order, starts afresh. Returns the step to take; zero means
/// wait (plan Amendment 4 records the changes to D-2 and D-3).
fn surround_walk(
    context: &StepContext<'_>,
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
        // stepping in and out of contact every tick (friendly-blocking plan Amendment 7).
        let straight = remove_blocked_velocity(body, checked_vec_scale(ahead, speed)?, neighbours)?;
        forward_free = checked_dot(straight, ahead)? >= speed.saturating_sub(STANDING_DRIFT);
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
        memory.flank_blocked_ticks = 0;
        return Ok(velocity);
    }
    if memory.flank_spent {
        // A spent walk waits, then looks again: the crowd round a target changes as units die
        // and leave, and a unit that never looked again would stand behind it for good.
        memory.flank_blocked_ticks = memory.flank_blocked_ticks.saturating_add(1);
        if memory.flank_blocked_ticks < FLANK_RETRY_TICKS {
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
    let left = SimVec2::new(-ahead.y, ahead.x);
    let across = if memory.flank_left {
        left
    } else {
        checked_vec_sub(SimVec2::ZERO, left)?
    };
    let step = remove_blocked_velocity(body, checked_vec_scale(across, speed)?, neighbours)?;
    let open = !is_waiting(step, speed)
        && policy.permits_segment(map, body.pos, checked_vec_add(body.pos, step)?);
    if open {
        memory.flank_blocked_ticks = 0;
        memory.flank_walked = memory.flank_walked.saturating_add(step.length());
        // A chase walks at most one circle round its target (D-3). An attack-move without a
        // target walks to bring an enemy ahead into scan; one that has walked its scan range
        // sideways and still sees none would only stretch the line (Amendment 4).
        let bound = match target {
            Some(_) => distance(body.pos, next)?.saturating_mul(SimScalar::TWO_PI),
            None => context.scan_range,
        };
        if memory.flank_walked >= bound {
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

/// Clears the surround walk for a new target, keeping the side last walked.
fn end_surround_walk(memory: &mut MovementMemory) {
    memory.flank_active = false;
    memory.flank_reversed = false;
    memory.flank_spent = false;
    memory.flank_blocked_ticks = 0;
    memory.flank_walked = SimScalar::ZERO;
}

/// Whether a friend of the unit that stands ([`SteeringObstacle::is_standing`]) and will not step aside
/// lies within one step of its edge, on the side toward `ahead` (a unit vector).
fn friend_blocks_ahead(
    context: &StepContext<'_>,
    ahead: SimVec2,
) -> Result<bool, MovementMathError> {
    let (body, speed) = (context.body, context.speed);
    for obstacle in context.neighbours {
        if obstacle.id == body.id
            || obstacle.owner != body.owner
            || !obstacle.is_standing()
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
    let loses_view = context.policy.permits_segment(map, origin, stepped)
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
    use super::hull_allows_step;
    use sim_types::SimAngle;

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
