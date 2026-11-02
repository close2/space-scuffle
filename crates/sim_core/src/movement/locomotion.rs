//! Per-actor locomotion: following routes, arriving and chasing targets.
//!
//! Owns tick phase 5. Each actor prepares one [`MotionProposal`] from a tick-start snapshot of
//! its neighbours, and scalar pose edits and route or order edits commit together after all
//! fallible work. Earlier successful actors stay committed when a later actor is diagnosed.
//!
//! # The rules of an order's life
//! - A route is checked when something may have invalidated it, not every tick: on a spaced
//!   schedule, when the owner's knowledge of the map changes, and when a step is refused.
//! - A unit is done when it reaches the destination, or when it touches a body that already
//!   settled on the same destination. Standing still proves nothing about a neighbour.
//! - A unit that stops getting closer rebuilds its route, and in the end gives the order up.
//! - An unreachable destination is replaced by the nearest reachable point and reported.
//!
//! Pursuit obeys the same route, steering and contact rules. Only its goal moves.
//!
//! # Invariants
//! - No route search runs twice for one actor within [`REPATH_COOLDOWN_TICKS`].
//! - Everything that decides an outcome is read from the tick-start snapshot or the map, so the
//!   result does not depend on the order actors are processed in beyond the committed pose.

use super::grid::BodyGrid;
use super::math::{
    checked_dot, checked_normalize, checked_vec_add, checked_vec_scale, checked_vec_sub, direction,
    distance, within_distance,
};
use super::tuning::{
    ABANDONED_TARGET_TICKS, PURSUIT_LEASH_SCAN_RANGES, PURSUIT_ROUTE_DRIFT, REPATH_COOLDOWN_TICKS,
    ROUTE_CHECK_PERIOD, ROUTE_START_TOLERANCE, STALL_ABANDON_TICKS, STALL_ARRIVAL_REACH,
    STALL_PROGRESS, STALL_REPATH_TICKS, STEERING_LOOKAHEAD, TERRAIN_GUARD_HORIZON,
    WAYPOINT_HANDOFF_MARGIN,
};
use super::{
    MotionPolicy, MovementInvariant, MovementLayer, MovementMathError, RouteOutcome,
    RouteOutcomeKind,
};
use crate::entity::{Entity, MovementMemory};
use crate::steering::{
    check_cluster_arrival_with, compute_unified_local_steering, remove_blocked_velocity,
    ContactBody, SteeringObstacle,
};
use crate::{SimWorld, WeaponPhase};
use sim_map::{
    MapError, PathRequestWithVision, PathResult, Pathfinder, PlayerVisionMap, SimMap, UnitClearance,
};
use sim_types::{AnimState, AttackOrigin, EntityId, SimAngle, SimScalar, SimVec2, Tick, UnitOrder};

/// An actor validated for this tick, with the rates it moves at.
#[derive(Clone, Copy)]
pub(crate) struct MotionEntry {
    pub(crate) id: EntityId,
    pub(crate) policy: MotionPolicy,
    /// Effective speed in world units per tick, including upgrades and buffs.
    pub(crate) speed: SimScalar,
    /// Authored priority for friendly contact.
    pub(crate) push_priority: u8,
}

/// The nearest reachable point last found, so a group costs one region scan, not one per unit.
#[derive(Clone, Copy)]
pub(super) struct ReachCache {
    tick: u32,
    goal: SimVec2,
    clearance: UnitClearance,
    owner: u8,
    reachable: SimVec2,
}

/// Every edit one actor's motion makes this tick, applied together in [`Self::commit`].
struct MotionProposal {
    position: SimVec2,
    velocity: SimVec2,
    heading: SimAngle,
    turret_angle: SimAngle,
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
            turret_angle: actor.turret_angle,
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

    fn replace_route(&mut self, route: Vec<SimVec2>) {
        self.clear_route = false;
        self.remove_prefix = 0;
        self.replacement = Some(route);
    }

    fn drop_route(&mut self) {
        self.clear_route = true;
        self.replacement = None;
        self.remove_prefix = 0;
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
        if let Some(&first) = waypoints.first() {
            if distance(first, position)? <= ROUTE_START_TOLERANCE {
                self.remove_prefix = 1;
            }
        }
        self.replacement = Some(waypoints);
        Ok(())
    }

    fn commit(self, actor: &mut Entity) {
        actor.pos_at_tick_start = actor.position;
        actor.position = self.position;
        actor.velocity = self.velocity;
        actor.heading = self.heading;
        actor.turret_angle = self.turret_angle;
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
    let mut request = *request;
    let mut found = pathfinder.find_path_with_vision(map, &request);
    if matches!(found, Err(MapError::InvalidStart(_))) && request.clearance != UnitClearance::Small
    {
        // Physics only tests a unit's centre, so a wide body can stand where its tier cannot
        // start: spawned in a gap, or pushed into one. It leaves on the narrowest tier.
        request.clearance = UnitClearance::Small;
        found = pathfinder.find_path_with_vision(map, &request);
    }
    match found {
        Ok(path) => RouteSearch::Found(path),
        Err(MapError::InvalidGoal(_) | MapError::NoPathFound { .. }) => {
            let cached = cache
                .filter(|c| {
                    (c.tick, c.goal, c.clearance, c.owner)
                        == (tick, request.goal, request.clearance, owner)
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
                clearance: request.clearance,
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
        clearance: UnitClearance::for_radius(policy.radius),
        vision: knowledge.map(|(vision, _)| vision),
        block_unexplored_ramps: false,
        known_structures: knowledge.map(|(_, known)| known.as_slice()),
    }
}

/// Whether a stored route still works from `position`.
///
/// The first leg asks only whether the unit can physically walk it. The remaining legs ask for
/// this tier's clearance. A body pushed into a narrow cell has a first leg the tier would refuse,
/// and that must not cost it the route it is still able to follow.
fn route_is_usable(
    map: &SimMap,
    pathfinder: &Pathfinder,
    request: &PathRequestWithVision<'_>,
    policy: MotionPolicy,
    position: SimVec2,
    route: &[SimVec2],
) -> bool {
    route.first().is_some_and(|&first| {
        policy.permits_segment(map, position, first)
            && route
                .windows(2)
                .all(|leg| pathfinder.segment_is_passable(map, request, leg[0], leg[1]))
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

/// Replaces a corner someone has parked on with a point the unit can actually walk to.
///
/// A route corner hugs the obstacle it turns around, so a body standing on it can only be passed
/// on the outside of the turn. Stepping the corner outward once, from where the unit first meets
/// the body, picks that side for good: recomputing it every tick would swap sides as soon as the
/// unit rounded the body.
fn sidestep_occupied_corner(
    context: &StepContext<'_>,
    pathfinder: &Pathfinder,
    request: &PathRequestWithVision<'_>,
    route: &[SimVec2],
) -> Result<Option<Vec<SimVec2>>, MovementMathError> {
    let &[first, subsequent, ..] = route else {
        return Ok(None);
    };
    let (body, policy) = (context.body, context.policy);
    let occupant = context.neighbours.iter().find(|obstacle| {
        !obstacle.yields_to(body.owner, body.push_priority)
            && within_distance(
                obstacle.pos,
                first,
                i128::from(policy.radius.raw()) + i128::from(obstacle.radius.raw()),
            )
    });
    let Some(occupant) = occupant else {
        return Ok(None);
    };
    let reach = policy.radius + occupant.radius + WAYPOINT_HANDOFF_MARGIN;
    if distance(body.pos, first)? > reach {
        return Ok(None);
    }
    let incoming = direction(first, body.pos)?;
    let outgoing = direction(subsequent, first)?;
    let outside = checked_normalize(checked_vec_sub(incoming, outgoing)?)?;
    if outside == SimVec2::ZERO {
        return Ok(None);
    }
    let aim = checked_vec_add(first, checked_vec_scale(outside, reach)?)?;
    let walkable = policy.permits_segment(context.map, body.pos, aim)
        && (policy.layer == MovementLayer::Air
            || pathfinder.segment_is_passable(context.map, request, aim, subsequent));
    Ok(walkable.then(|| {
        std::iter::once(aim)
            .chain(route[1..].iter().copied())
            .collect()
    }))
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
            .map_err(|e| arithmetic(id, e))?;
        Ok(MotionEntry {
            id,
            policy,
            speed,
            push_priority: arch.push_priority,
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
                .filter(|e| e.is_alive() && !e.is_loaded_in_transport())
                .map(|e| e.id),
        );
        self.movement.largest_radius = [SimScalar::ZERO; 2];
        for layer in &mut self.movement.obstacles {
            layer.clear();
        }
        for &id in &ids {
            match self.motion_entry(id) {
                Ok(entry) => {
                    if let Some(actor) = self.entities.get(&id).filter(|a| !a.is_mineral_walking) {
                        let obstacle = self.obstacle_of(entry, actor);
                        let layer = entry.policy.layer.index();
                        self.movement.obstacles[layer].push(obstacle);
                        self.movement.largest_radius[layer] =
                            self.movement.largest_radius[layer].max(entry.policy.radius);
                    }
                    entries.push(entry);
                }
                Err(error) => self.report_movement_invariant(error),
            }
        }
        self.movement.phase_ids = ids;
        let (width, height) = (self.map.width(), self.map.height());
        for layer in 0..2 {
            let positions = self.movement.obstacles[layer].iter().map(|o| o.pos);
            self.movement.grids[layer].rebuild(width, height, positions);
        }
        let navigation = self.pathfinder.nav_version();
        if navigation != self.movement.seen_nav_version {
            self.movement.seen_nav_version = navigation;
            self.movement.recheck_routes = [true; 8];
        }
    }

    pub(crate) fn advance_checked_movement(&mut self) {
        // PERFORMANCE RATIONALE:
        // Why: Proposals must not clone an Entity or allocate an ID vector per actor.
        // What: Rebuild retained validated entries/obstacles once and borrow each
        // actor while preparing compact scalar edits and optional new path ownership.
        // Trade-off: Buff/radius validation is phase-entry work; scratch capacities
        // grow at population high-water marks, while pathfinding retains its costs.
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
                        proposal.commit(actor);
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
                        proposal.commit(actor);
                    }
                }
                Err(error) => self.report_movement_invariant(error),
            }
        }
        self.movement.entries = entries;
        self.movement.entries_are_fresh = true;
        self.movement.pursuits = pursuits;
        self.movement.casts = casts;
        self.movement.recheck_routes = [false; 8];
    }

    /// Whether this tick is one on which `id`'s route gets looked at.
    fn route_check_due(&self, id: EntityId, owner: usize) -> bool {
        self.movement.recheck_routes[owner]
            || self
                .current_tick
                .wrapping_add(id.0)
                .is_multiple_of(ROUTE_CHECK_PERIOD)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one order's route, arrival and step decisions share the prepared proposal"
    )]
    fn prepare_actor_motion(
        &mut self,
        entry: MotionEntry,
    ) -> Result<MotionAction, MovementInvariant> {
        let id = entry.id;
        let policy = entry.policy;
        let speed = entry.speed;
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "validated actor disappeared",
            })?;
        let mut proposal = MotionProposal::new(actor, policy);
        if actor.is_on_hold() || actor.is_immobile || actor.channeling_remaining_ticks > 0 {
            proposal.clear_route = false;
            proposal.velocity = SimVec2::ZERO;
            return Ok(MotionAction::Commit(proposal));
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
            return Ok(MotionAction::Commit(proposal));
        }
        let mut target = match actor.order_queue.front() {
            Some(UnitOrder::Move { target } | UnitOrder::AttackMove { target }) => *target,
            Some(_) if actor.has_task_order() && actor.task.goal.is_some() => {
                actor.task.goal.unwrap_or(actor.position)
            }
            Some(UnitOrder::AttackTarget { target, origin }) => {
                return Ok(MotionAction::Pursue(*target, *origin))
            }
            Some(UnitOrder::CastAbility {
                ability_index,
                target_pos,
                target_entity,
            }) => {
                return Ok(MotionAction::Cast((
                    id,
                    *ability_index,
                    *target_pos,
                    *target_entity,
                )))
            }
            _ => {
                proposal.velocity = SimVec2::ZERO;
                if proposal.anim == AnimState::Walk {
                    proposal.anim = AnimState::Idle;
                }
                return Ok(MotionAction::Commit(proposal));
            }
        };
        if !policy.contains(&self.map, target) {
            return Err(MovementInvariant::InvalidState {
                entity: id,
                reason: "active movement order is outside the current full-radius domain",
            });
        }
        let owner = usize::from(actor.owner.0);
        let tick = Tick(self.current_tick);
        let ground = policy.layer == MovementLayer::Ground;
        // Fresh knowledge of the map cannot wait out a cooldown: the route may lead into a wall.
        let map_news = self.movement.recheck_routes[owner];
        let cooled_down = proposal.memory.repath_cooldown == 0 || map_news;
        let check_route = self.route_check_due(id, owner);
        let knowledge = self
            .vision_aware_pathfinding
            .then_some((&self.vision_maps[owner], &self.known_blocked_cells[owner]));
        let request = route_request(actor, policy, target, knowledge);
        if ground
            && cooled_down
            && check_route
            && !proposal.route(actor).is_empty()
            && !route_is_usable(
                &self.map,
                &self.pathfinder,
                &request,
                policy,
                actor.position,
                proposal.route(actor),
            )
        {
            proposal.drop_route();
        }
        if proposal.route(actor).is_empty() {
            if distance(actor.position, target).map_err(|e| arithmetic(id, e))? <= speed
                && policy.permits_segment(&self.map, actor.position, target)
            {
                proposal.position = target;
                proposal.finish(target);
                return Ok(MotionAction::Commit(proposal));
            }
            if ground && cooled_down {
                proposal.memory.repath_cooldown = REPATH_COOLDOWN_TICKS;
                self.movement.path_searches += 1;
                let search = search_route(
                    &mut self.pathfinder,
                    &self.map,
                    &request,
                    &mut self.movement.reach_cache,
                    self.current_tick,
                    actor.owner.0,
                );
                let path = match search {
                    RouteSearch::Found(path) => path,
                    RouteSearch::Retargeted(reachable, path) => {
                        self.movement.route_outcomes.push(RouteOutcome {
                            tick,
                            entity: id,
                            kind: RouteOutcomeKind::Retargeted {
                                requested: target,
                                reachable,
                            },
                        });
                        target = reachable;
                        proposal.retarget = Some(reachable);
                        path
                    }
                    RouteSearch::Blocked => {
                        self.movement.route_outcomes.push(RouteOutcome {
                            tick,
                            entity: id,
                            kind: RouteOutcomeKind::Unreachable,
                        });
                        proposal.finish(target);
                        return Ok(MotionAction::Commit(proposal));
                    }
                };
                proposal
                    .adopt_route(actor.position, path.waypoints)
                    .map_err(|e| arithmetic(id, e))?;
            }
        }
        let layer = policy.layer.index();
        let mut neighbours = std::mem::take(&mut self.movement.neighbours);
        gather_neighbours(
            &NeighbourIndex {
                grid: &self.movement.grids[layer],
                obstacles: &self.movement.obstacles[layer],
                largest_radius: self.movement.largest_radius[layer],
            },
            &self.map,
            entry,
            actor.position,
            &mut neighbours,
        );
        let context = StepContext {
            map: &self.map,
            policy,
            speed,
            body: ContactBody {
                id,
                owner: actor.owner,
                push_priority: entry.push_priority,
                pos: actor.position,
                radius: policy.radius,
            },
            neighbours: &neighbours,
        };
        drop_passed_waypoints(&context, &self.pathfinder, &request, actor, &mut proposal)
            .map_err(|e| arithmetic(id, e))?;
        let on_last_leg = proposal.route(actor).len() <= 1;
        if on_last_leg
            && check_cluster_arrival_with(
                Some(id),
                actor.position,
                policy.radius,
                target,
                &neighbours,
                |witness| {
                    witness.owner == actor.owner
                        && (!ground || on_connected_ground(&self.map, witness.pos, target))
                },
            )
            .map_err(|e| arithmetic(id, e))?
        {
            proposal.finish(target);
            self.movement.neighbours = neighbours;
            return Ok(MotionAction::Commit(proposal));
        }
        if let Some(route) =
            sidestep_occupied_corner(&context, &self.pathfinder, &request, proposal.route(actor))
                .map_err(|e| arithmetic(id, e))?
        {
            proposal.replace_route(route);
        }
        let next = proposal.route(actor).first().copied().unwrap_or(target);
        let gap = distance(actor.position, next).map_err(|e| arithmetic(id, e))?;
        if speed > SimScalar::ZERO {
            if map_news {
                proposal.memory.progress_best = None;
            }
            let route = proposal.route(actor);
            let mut memory = proposal.memory;
            let stalled = note_progress(&mut memory, actor.position, route, target)
                .map_err(|e| arithmetic(id, e))?;
            proposal.memory = memory;
            let stuck_at_goal =
                on_last_leg && stalled >= STALL_REPATH_TICKS && gap <= STALL_ARRIVAL_REACH;
            if stalled >= STALL_ABANDON_TICKS || stuck_at_goal {
                if !stuck_at_goal {
                    self.movement.route_outcomes.push(RouteOutcome {
                        tick,
                        entity: id,
                        kind: RouteOutcomeKind::NoProgress,
                    });
                }
                proposal.finish(target);
                self.movement.neighbours = neighbours;
                return Ok(MotionAction::Commit(proposal));
            }
            if ground && cooled_down && stalled > 0 && stalled.is_multiple_of(STALL_REPATH_TICKS) {
                proposal.drop_route();
            }
        }
        let refused =
            step_toward(&context, &mut proposal, next, gap, None).map_err(|e| arithmetic(id, e))?;
        if refused
            && ground
            && cooled_down
            && !proposal.route(actor).is_empty()
            && !policy.permits_segment(&self.map, actor.position, next)
        {
            proposal.drop_route();
        }
        if proposal.position == next && proposal.route(actor).first() == Some(&next) {
            proposal.remove_prefix += 1;
        }
        if proposal.route(actor).is_empty()
            && proposal.position == next
            && distance(next, target).map_err(|e| arithmetic(id, e))? <= speed
            && policy.permits_segment(&self.map, next, target)
        {
            proposal.position = target;
            proposal.finish(target);
        }
        self.movement.neighbours = neighbours;
        Ok(MotionAction::Commit(proposal))
    }

    #[expect(
        clippy::too_many_lines,
        reason = "range check, route upkeep and step share one prepared proposal"
    )]
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
            proposal.discard_order();
            return Ok(proposal);
        }
        let target = self
            .entities
            .get(&target_id)
            .ok_or(MovementInvariant::InvalidState {
                entity: target_id,
                reason: "available target disappeared",
            })?;
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
            proposal.discard_order();
            return Ok(proposal);
        };
        let delta = self.player_upgrades[usize::from(actor.owner.0)]
            .unit_stat_modifiers
            .get(&arch.slug)
            .map_or(SimScalar::ZERO, |m| m.weapon_range_delta);
        let max = weapon
            .max_range
            .checked_add(delta)
            .ok_or_else(|| arithmetic(id, MovementMathError::Unrepresentable))?;
        if max < SimScalar::ZERO || weapon.min_range < SimScalar::ZERO {
            return Err(arithmetic(id, MovementMathError::InvalidDistance));
        }
        let radii = i128::from(policy.radius.raw()) + i128::from(target_policy.radius.raw());
        let max = i128::from(max.raw()) + radii;
        let min = if weapon.min_range > SimScalar::ZERO {
            i128::from(weapon.min_range.raw()) + radii
        } else {
            0
        };
        let separation = i128::from(
            distance(actor.position, target.position)
                .map_err(|e| arithmetic(id, e))?
                .raw(),
        );
        if separation <= max && separation >= min {
            proposal.velocity = SimVec2::ZERO;
            let aim = direction(target.position, actor.position).map_err(|e| arithmetic(id, e))?;
            let angle = SimAngle::atan2(aim.y, aim.x);
            if weapon.has_turret {
                proposal.turret_angle = angle;
            } else {
                proposal.heading = angle;
            }
            proposal.anim = if matches!(
                actor.weapon_phase,
                WeaponPhase::Windup { .. } | WeaponPhase::Backswing { .. }
            ) {
                AnimState::Attack
            } else {
                AnimState::Idle
            };
            return Ok(proposal);
        }

        let goal = target.position;
        let ground = policy.layer == MovementLayer::Ground;
        let cooled_down = proposal.memory.repath_cooldown == 0;
        let owner = usize::from(actor.owner.0);
        let knowledge = self
            .vision_aware_pathfinding
            .then_some((&self.vision_maps[owner], &self.known_blocked_cells[owner]));
        let request = route_request(actor, policy, goal, knowledge);
        let stale = proposal
            .route(actor)
            .last()
            .is_some_and(|&end| !within_distance(end, goal, i128::from(PURSUIT_ROUTE_DRIFT.raw())));
        if ground && cooled_down && stale {
            proposal.drop_route();
        }
        let layer = policy.layer.index();
        let mut neighbours = std::mem::take(&mut self.movement.neighbours);
        gather_neighbours(
            &NeighbourIndex {
                grid: &self.movement.grids[layer],
                obstacles: &self.movement.obstacles[layer],
                largest_radius: self.movement.largest_radius[layer],
            },
            &self.map,
            entry,
            actor.position,
            &mut neighbours,
        );
        let context = StepContext {
            map: &self.map,
            policy,
            speed: entry.speed,
            body: ContactBody {
                id,
                owner: actor.owner,
                push_priority: entry.push_priority,
                pos: actor.position,
                radius: policy.radius,
            },
            neighbours: &neighbours,
        };
        drop_passed_waypoints(&context, &self.pathfinder, &request, actor, &mut proposal)
            .map_err(|e| arithmetic(id, e))?;
        if let Some(route) =
            sidestep_occupied_corner(&context, &self.pathfinder, &request, proposal.route(actor))
                .map_err(|e| arithmetic(id, e))?
        {
            proposal.replace_route(route);
        }
        let next = proposal.route(actor).first().copied().unwrap_or(goal);
        let gap = distance(actor.position, next).map_err(|e| arithmetic(id, e))?;
        let tick = Tick(self.current_tick);
        let abandoned = |outcomes: &mut Vec<RouteOutcome>, proposal: &mut MotionProposal| {
            outcomes.push(RouteOutcome {
                tick,
                entity: id,
                kind: RouteOutcomeKind::PursuitAbandoned { target: target_id },
            });
            proposal.abandon_target(target_id);
        };
        if entry.speed > SimScalar::ZERO {
            if target.velocity != SimVec2::ZERO {
                // A moving goal says nothing about whether this unit is stuck.
                proposal.memory.progress_best = None;
            }
            let route = proposal.route(actor);
            let mut memory = proposal.memory;
            let stalled = note_progress(&mut memory, actor.position, route, goal)
                .map_err(|e| arithmetic(id, e))?;
            proposal.memory = memory;
            if stalled >= STALL_ABANDON_TICKS {
                abandoned(&mut self.movement.route_outcomes, &mut proposal);
                self.movement.neighbours = neighbours;
                return Ok(proposal);
            }
        }
        let refused = step_toward(&context, &mut proposal, next, gap, Some(target_id))
            .map_err(|e| arithmetic(id, e))?;
        if proposal.position == next && proposal.route(actor).first() == Some(&next) {
            proposal.remove_prefix += 1;
        }
        if refused && ground && cooled_down {
            proposal.memory.repath_cooldown = REPATH_COOLDOWN_TICKS;
            self.movement.path_searches += 1;
            let search = search_route(
                &mut self.pathfinder,
                &self.map,
                &request,
                &mut self.movement.reach_cache,
                self.current_tick,
                actor.owner.0,
            );
            let path = match search {
                RouteSearch::Found(path) => Some(path),
                RouteSearch::Retargeted(reachable, path) => {
                    within_distance(reachable, goal, max).then_some(path)
                }
                RouteSearch::Blocked => None,
            };
            let leash = arch
                .scan_range
                .checked_mul(PURSUIT_LEASH_SCAN_RANGES)
                .unwrap_or(SimScalar::MAX);
            match path {
                Some(path) if origin == AttackOrigin::Explicit || path.total_length <= leash => {
                    if actor.current_funnel_waypoints.is_empty() {
                        // Leaving the straight line for a detour lengthens the way on purpose.
                        proposal.memory.progress_best = None;
                    }
                    proposal
                        .adopt_route(actor.position, path.waypoints)
                        .map_err(|e| arithmetic(id, e))?;
                }
                _ => abandoned(&mut self.movement.route_outcomes, &mut proposal),
            }
        }
        self.movement.neighbours = neighbours;
        Ok(proposal)
    }
}

/// Whether a unit can walk straight from where it stands to `end` without being stopped by a body
/// that will not yield. Bodies that do yield are no reason to take a longer way.
fn leg_is_free_of_bodies(
    context: &StepContext<'_>,
    end: SimVec2,
) -> Result<bool, MovementMathError> {
    let body = context.body;
    let length = distance(body.pos, end)?;
    let along = direction(end, body.pos)?;
    for obstacle in context.neighbours {
        if obstacle.yields_to(body.owner, body.push_priority) {
            continue;
        }
        let offset = checked_vec_sub(obstacle.pos, body.pos)?;
        let reach = checked_dot(offset, along)?.clamp(SimScalar::ZERO, length);
        let nearest = checked_vec_add(body.pos, checked_vec_scale(along, reach)?)?;
        let touching = i128::from(body.radius.raw()) + i128::from(obstacle.radius.raw());
        if within_distance(nearest, obstacle.pos, touching) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Drops leading waypoints the unit no longer needs to touch.
///
/// A corner is a means of reaching the next point. Once that point is in clear view from here,
/// and the unit is near the corner or already past it, the corner has done its work. Being near is
/// measured in body widths because another unit may stand exactly on the corner.
fn drop_passed_waypoints(
    context: &StepContext<'_>,
    pathfinder: &Pathfinder,
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
        if gap > handoff && !past {
            break;
        }
        let in_view = policy.permits_segment(map, actor.position, subsequent)
            && (policy.layer == MovementLayer::Air
                || pathfinder.segment_is_passable(map, request, actor.position, subsequent));
        if !in_view || !leg_is_free_of_bodies(context, subsequent)? {
            break;
        }
        proposal.remove_prefix += 1;
    }
    Ok(())
}

/// Takes one step toward `next`, `gap` world units away, avoiding `neighbours`.
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
    } = *context;
    let origin = body.pos;
    if gap <= speed && policy.permits_segment(map, origin, next) {
        proposal.position = next;
        proposal.velocity = checked_vec_sub(next, origin)?;
        proposal.anim = AnimState::Walk;
        return Ok(false);
    }
    let heading =
        compute_unified_local_steering(body.id, origin, policy.radius, next, ignored, neighbours)?;
    let mut velocity = checked_vec_scale(heading, speed)?;
    if policy.layer == MovementLayer::Ground {
        velocity = keep_terrain_view(context, next, gap, velocity)?;
    }
    let mut velocity = remove_blocked_velocity(body, velocity, neighbours)?;
    if ignored.is_some() {
        velocity = flank_if_blocked(context, proposal, next, velocity)?;
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
    Ok(refused)
}

/// Sends a chasing unit sideways along whatever blocks its way to `next`.
///
/// Attackers pile up behind those already fighting, and steering alone leaves them shuffling
/// between neighbours. A blocked attacker instead walks along the crowd to the side it chose,
/// and turns back the other way only when that side closes too, which is how a surround forms.
fn flank_if_blocked(
    context: &StepContext<'_>,
    proposal: &mut MotionProposal,
    next: SimVec2,
    velocity: SimVec2,
) -> Result<SimVec2, MovementMathError> {
    let StepContext {
        speed,
        body,
        neighbours,
        ..
    } = *context;
    let walking_pace = i128::from(speed.raw()) >> 1;
    if !within_distance(velocity, SimVec2::ZERO, walking_pace) {
        return Ok(velocity);
    }
    let ahead = direction(next, body.pos)?;
    let left = checked_vec_scale(SimVec2::new(-ahead.y, ahead.x), speed)?;
    let along = |to_left: bool| -> Result<SimVec2, MovementMathError> {
        let step = if to_left {
            left
        } else {
            checked_vec_sub(SimVec2::ZERO, left)?
        };
        remove_blocked_velocity(body, step, neighbours)
    };
    let side = proposal.memory.flank_left;
    let preferred = along(side)?;
    if !within_distance(preferred, SimVec2::ZERO, walking_pace) {
        return Ok(preferred);
    }
    proposal.memory.flank_left = !side;
    let other = along(!side)?;
    Ok(if within_distance(other, SimVec2::ZERO, walking_pace) {
        velocity
    } else {
        other
    })
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
