//! One follower's motion per tick: the follow order's goal, rest, resume and end (plan
//! `2026-11-26-follow-order` D-3 to D-5; `StarCraft` II 5.0.14's `move` on a unit).
//!
//! # Responsibility
//! Owns what a unit whose front order is `UnitOrder::Follow` does in the movement phase: it
//! walks toward its target's tick-start position, comes to rest when it touches the target or a
//! follower already resting on it, stays at rest until the target has moved more than the
//! movement rules' `follow_range_slop` from where it stood, and gives the order up when the
//! target is gone. Admission belongs to [`crate::movement`], acquisition to
//! [`crate::target_acquisition`].
//!
//! # How it fits
//! A child of [`super`], the locomotion module: `walking_goal` hands a follower here, and the
//! walk reuses the pursuit's draft, route upkeep, steering and stall measure, with a dead band
//! where a pursuit has a weapon band and no ring (D-3). Contact is the solver's: a resting
//! follower only reports itself as an idle body ([`Entity::stands_idle`], D-7), so no
//! `movement/solver*` code knows of follows.
//!
//! # Invariants
//! - Every decision reads the tick-start snapshot: the target's position (`pursued_motion`),
//!   the bodies' [`SteeringObstacle::resting_on`], and the follower's own memory, so the answer
//!   does not depend on the order actors are processed in.
//! - A follow never forms a route group, and adds no tuning constant: the rest band is the
//!   arrival touch rule (the radius sum plus [`ARRIVAL_TOUCH_MARGIN`]), the stall limit is a
//!   move's ([`STALL_ABANDON_TICKS`]), and the slop is authored data.
//! - A follow ends only when its target is gone, dead, carried or housed, or, with an order
//!   queued behind it, when the follower rests by its target (D-5). A stall or an unreachable
//!   target rests the follower instead, because its target may come back (D-4 rules 3 and 4).

use super::{
    braking_speed, clamp_into_domain, drop_passed_waypoints, friend_holds_ahead, gather_neighbours,
    near_margin, note_queued_progress, route_request, search_route, step_toward, MotionDraft,
    MotionEntry, MotionProposal, MotionScene, NeighbourIndex, RouteSearch, Stage,
};
use crate::entity::Entity;
use crate::movement::math::{
    checked_dot, checked_vec_add, checked_vec_scale, checked_vec_sub, direction, distance,
    within_distance,
};
use crate::movement::tuning::{
    ARRIVAL_TOUCH_MARGIN, PURSUIT_ROUTE_DRIFT, REPATH_COOLDOWN_TICKS, STALL_ABANDON_TICKS,
};
use crate::movement::{MotionPolicy, MovementInvariant, MovementLayer};
use crate::steering::{FollowRest, SteeringObstacle};
use crate::SimWorld;
use sim_types::{AnimState, EntityId, SimScalar, SimVec2, Tick, UnitOrder};

/// What a follower does this tick, decided from the tick-start snapshot before it walks.
enum FollowStep {
    /// The target is gone, dead, carried or housed: the follow ends (D-5).
    End,
    /// The follower rests by its target, anchored where the target stood when the rest began,
    /// world units (D-4).
    Rest(SimVec2),
    /// The follower walks toward the target's tick-start position, world units (D-3).
    Chase(SimVec2),
}

impl MotionProposal {
    /// Ends a follow: the order is popped and the movement memory starts afresh, as for an attack
    /// whose target is gone. The next queued order, if any, starts next tick.
    fn end_follow(&mut self) {
        self.discard_order();
        self.memory.restart();
    }
}

impl SimWorld {
    /// The rest `actor`'s follow keeps this tick, for its steering obstacle (D-4): the front
    /// order is a follow, the follower rests ([`crate::MovementMemory::follow_rest`]), and the
    /// target, still followable, stands within `follow_range_slop` of the rest's anchor. A
    /// follower whose rest ends this tick is no witness, so a whole group resumes together.
    pub(super) fn resting_on(&self, actor: &Entity) -> Option<FollowRest> {
        let Some(&UnitOrder::Follow { target, .. }) = actor.order_queue.front() else {
            return None;
        };
        let anchor = actor.movement.follow_rest?;
        let followed = self.followable(target)?;
        let (at, _) = self.pursued_motion(target, followed);
        let slop = self.catalog.movement_rules().follow_range_slop();
        within_distance(at, anchor, i128::from(slop.raw())).then_some(FollowRest { target, anchor })
    }

    /// `target` when it can be followed: alive and neither carried nor housed (D-5).
    fn followable(&self, target: EntityId) -> Option<&Entity> {
        self.entities
            .get(&target)
            .filter(|entity| entity.is_alive() && !entity.is_hidden())
    }

    /// Prepares one follower's motion toward `target` for this tick (D-3 to D-5).
    pub(super) fn follow_motion(
        &mut self,
        entry: MotionEntry,
        target: EntityId,
    ) -> Result<MotionProposal, MovementInvariant> {
        let step = self.follow_step(entry, target)?;
        let actor = self.follower(entry.id)?;
        let mut proposal = MotionProposal::new(actor, entry.policy);
        match step {
            FollowStep::End => {
                proposal.end_follow();
                self.settle_follower(entry, actor, &mut proposal)?;
                Ok(proposal)
            }
            FollowStep::Rest(anchor) => {
                Self::rest_follower(actor, &mut proposal, anchor);
                Ok(proposal)
            }
            FollowStep::Chase(goal) => {
                if proposal.memory.follow_rest.take().is_some() {
                    // The target moved on: the walk starts afresh.
                    proposal.memory.progress_best = None;
                    proposal.memory.stalled_ticks = 0;
                }
                self.chase_followed(entry, target, goal, proposal)
            }
        }
    }

    /// The validated follower `id`.
    fn follower(&self, id: EntityId) -> Result<&Entity, MovementInvariant> {
        self.entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "validated follower disappeared",
            })
    }

    /// Decides the follower's tick: the end rows of D-5, then the rest it keeps or comes to
    /// (D-4), else a chase.
    fn follow_step(
        &mut self,
        entry: MotionEntry,
        target: EntityId,
    ) -> Result<FollowStep, MovementInvariant> {
        let Some(followed) = self.followable(target) else {
            return Ok(FollowStep::End);
        };
        let actor = self.follower(entry.id)?;
        let (target_at, _) = self.pursued_motion(target, followed);
        let target_radius = MotionPolicy::resolve(&self.catalog, &self.map, followed)?.radius;
        let (position, anchor) = (actor.position, actor.movement.follow_rest);
        if let Some(anchor) = anchor {
            let slop = self.catalog.movement_rules().follow_range_slop();
            if within_distance(target_at, anchor, i128::from(slop.raw())) {
                return Ok(FollowStep::Rest(anchor));
            }
        }
        let reach = i128::from(entry.policy.radius.raw())
            + i128::from(target_radius.raw())
            + i128::from(ARRIVAL_TOUCH_MARGIN.raw());
        if within_distance(position, target_at, reach) {
            return Ok(FollowStep::Rest(target_at));
        }
        Ok(self
            .resting_follower_touched(entry, position, target)
            .map_or(FollowStep::Chase(target_at), FollowStep::Rest))
    }

    /// D-4 rule 2: the anchor of the rest the follower at `position` joins, if it touches, within
    /// the arrival margin, a tick-start body of its collision class resting on `target`. The
    /// follower inherits that body's anchor (the lowest id's, when it touches several), so every
    /// rest of a group traces back to a follower that touched the target, and the group resumes
    /// together once the target leaves (D-8).
    fn resting_follower_touched(
        &mut self,
        entry: MotionEntry,
        position: SimVec2,
        target: EntityId,
    ) -> Option<SimVec2> {
        let class = entry.policy.class.index();
        let mut neighbours = std::mem::take(&mut self.movement.neighbours);
        if let (Some(grid), Some(obstacles), Some(&largest_radius)) = (
            self.movement.grids.get(class),
            self.movement.obstacles.get(class),
            self.movement.largest_radius.get(class),
        ) {
            gather_neighbours(
                &NeighbourIndex {
                    grid,
                    obstacles,
                    largest_radius,
                },
                &self.map,
                (entry, position),
                None,
                &mut neighbours,
            );
        } else {
            neighbours.clear();
        }
        debug_assert!(near_margin(entry) >= ARRIVAL_TOUCH_MARGIN);
        let joined = neighbours
            .iter()
            .filter_map(|body: &SteeringObstacle| {
                let rest = body.resting_on.filter(|rest| rest.target == target)?;
                let reach = i128::from(entry.policy.radius.raw())
                    + i128::from(body.radius.raw())
                    + i128::from(ARRIVAL_TOUCH_MARGIN.raw());
                within_distance(position, body.pos, reach).then_some((body.id, rest.anchor))
            })
            .min_by_key(|&(id, _)| id)
            .map(|(_, anchor)| anchor);
        self.movement.neighbours = neighbours;
        joined
    }

    /// Rests the follower (D-4), or ends the follow when an order is queued behind it (D-5): the
    /// queue goes on. `anchor` is where the target stood when the rest began. A rest is an
    /// arrival: the follower stands at once, as a move that arrives does
    /// ([`MotionProposal::finish`]); only a follow that ends without arriving glides.
    fn rest_follower(actor: &Entity, proposal: &mut MotionProposal, anchor: SimVec2) {
        proposal.position = actor.position;
        if actor.order_queue.len() > 1 {
            proposal.end_follow();
        } else {
            proposal.drop_route();
            proposal.velocity = SimVec2::ZERO;
            proposal.memory.follow_rest = Some(anchor);
        }
        if proposal.anim == AnimState::Walk {
            proposal.anim = AnimState::Idle;
        }
    }

    /// Lets a follower that does not walk glide to rest (B-39): it keeps its walking animation
    /// while it glides and plays `Idle` once it stands.
    fn settle_follower(
        &self,
        entry: MotionEntry,
        actor: &Entity,
        proposal: &mut MotionProposal,
    ) -> Result<(), MovementInvariant> {
        self.coast(entry, actor, proposal)?;
        if proposal.velocity == SimVec2::ZERO && proposal.anim == AnimState::Walk {
            proposal.anim = AnimState::Idle;
        }
        Ok(())
    }

    /// The walk toward `target_at`, the target's tick-start position, as a [`MotionDraft`] (D-3).
    /// A walk that reaches the nearest reachable point of an unreachable target, or stalls, rests
    /// the follower instead (D-4 rules 3 and 4).
    fn chase_followed(
        &mut self,
        entry: MotionEntry,
        target: EntityId,
        target_at: SimVec2,
        proposal: MotionProposal,
    ) -> Result<MotionProposal, MovementInvariant> {
        let id = entry.id;
        let policy = entry.policy;
        let followed = self
            .followable(target)
            .ok_or(MovementInvariant::InvalidState {
                entity: target,
                reason: "followed target disappeared",
            })?;
        let touch =
            policy.radius + MotionPolicy::resolve(&self.catalog, &self.map, followed)?.radius;
        let moving = self.pursued_motion(target, followed).1 != SimVec2::ZERO;
        // A field borrow, apart from the pathfinder and the workspace the walk borrows mutably.
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "validated follower disappeared",
            })?;
        let goal = clamp_into_domain(policy, &self.map, target_at);
        // `MotionEntry` creation already rejected owners without a slot (`effective_max_speed`).
        let owner = actor.owner.slot().ok_or(MovementInvariant::InvalidState {
            entity: id,
            reason: "follower owner has no slot",
        })?;
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
        let walked = draft.follow(&mut scene, &mut neighbours, (target, touch, moving));
        scene.movement.neighbours = neighbours;
        let rests = walked?.is_break();
        let mut proposal = draft.proposal;
        if rests {
            Self::rest_follower(self.follower(id)?, &mut proposal, target_at);
        }
        Ok(proposal)
    }
}

impl MotionDraft<'_> {
    /// Walks a follower one tick: route upkeep with the pursuit's drift rule, braking to stop at
    /// touch, the stall measure and the step. `followed` is the target, the radius sum it is
    /// touched at (world units) and whether it moved last tick. Breaks when the follower should
    /// rest instead: it stalled for [`STALL_ABANDON_TICKS`], or nothing nearer its target can be
    /// reached (D-4 rules 3 and 4).
    fn follow(
        &mut self,
        scene: &mut MotionScene<'_>,
        neighbours: &mut Vec<SteeringObstacle>,
        (target, touch, moving): (EntityId, SimScalar, bool),
    ) -> Result<Stage, MovementInvariant> {
        let stale = self.proposal.route(self.actor).last().is_some_and(|&end| {
            !within_distance(end, self.goal, i128::from(PURSUIT_ROUTE_DRIFT.raw()))
        });
        if self.ground && self.cooled_down && stale {
            self.proposal.drop_route();
            if self.search_follow_route(scene)?.is_break() {
                return Ok(Stage::Break(()));
            }
        }
        self.brake(touch + ARRIVAL_TOUCH_MARGIN)?;
        scene.gather_neighbours(self.entry, self.actor.position, neighbours);
        self.brake_for_rests(neighbours, target)?;
        let context = self.step_context(scene.map, neighbours, scene.intent());
        drop_passed_waypoints(
            &context,
            scene.pathfinder,
            &self.request,
            self.actor,
            &mut self.proposal,
        )
        .map_err(self.fault())?;
        let (next, gap) = self.follow_waypoint(&context, scene.pathfinder, touch)?;
        // A walk queued behind a friend that holds its ground is not stalled (crowd-edge D-4);
        // under the solver the stall rule reads no class of another body.
        let queued = context.intent.is_none()
            && next != self.actor.position
            && friend_holds_ahead(
                &context,
                direction(next, self.actor.position).map_err(self.fault())?,
            )
            .map_err(self.fault())?;
        if moving {
            // A moving target says nothing about whether this unit is stuck.
            self.proposal.memory.progress_best = None;
        }
        let route = self.proposal.route(self.actor);
        let mut memory = self.proposal.memory;
        let stalled =
            note_queued_progress(&mut memory, self.actor.position, route, self.goal, queued)
                .map_err(self.fault())?;
        self.proposal.memory = memory;
        if stalled >= STALL_ABANDON_TICKS {
            return Ok(Stage::Break(()));
        }
        let refused = step_toward(
            &context,
            scene.pathfinder,
            &mut self.proposal,
            next,
            gap,
            Some(target),
        )
        .map_err(self.fault())?;
        self.pass_reached_waypoint(next);
        if refused && self.ground && self.cooled_down {
            return self.search_follow_route(scene);
        }
        Ok(Stage::Continue(()))
    }

    /// Lowers this tick's step so the follower stops at the rest band of a follower ahead of it
    /// that rests on `target` (D-4 rule 2), as it brakes into the band round the target itself:
    /// a follower joins the group's rest instead of walking into it and pushing it on. "Ahead"
    /// is any resting body on the near side of the line through the follower across its way to
    /// the goal.
    fn brake_for_rests(
        &mut self,
        neighbours: &[SteeringObstacle],
        target: EntityId,
    ) -> Result<(), MovementInvariant> {
        let position = self.actor.position;
        let heading = checked_vec_sub(self.goal, position).map_err(self.fault())?;
        for body in neighbours {
            if body.resting_on.is_none_or(|rest| rest.target != target) {
                continue;
            }
            let towards = checked_vec_sub(body.pos, position).map_err(self.fault())?;
            if checked_dot(towards, heading).map_err(self.fault())? <= SimScalar::ZERO {
                continue;
            }
            let edge = distance(position, body.pos)
                .map_err(self.fault())?
                .saturating_sub(self.entry.policy.radius + body.radius + ARRIVAL_TOUCH_MARGIN)
                .max(SimScalar::ZERO);
            let limit = match self.entry.deceleration {
                Some(deceleration) => braking_speed(edge, deceleration).map_err(self.fault())?,
                None => edge,
            };
            self.entry.step = self.entry.step.min(limit);
        }
        Ok(())
    }

    /// The point a follower steps toward and its distance, world units: the route's next
    /// waypoint, or on the last leg the point `touch` short of the goal on the line from it, so
    /// a step never carries the follower onto its target's centre.
    fn follow_waypoint(
        &mut self,
        context: &super::StepContext<'_>,
        pathfinder: &mut sim_map::Pathfinder,
        touch: SimScalar,
    ) -> Result<(SimVec2, SimScalar), MovementInvariant> {
        let (next, gap) = self.next_waypoint(context, pathfinder)?;
        if !self.proposal.route(self.actor).is_empty() {
            return Ok((next, gap));
        }
        if gap <= touch {
            return Ok((self.actor.position, SimScalar::ZERO));
        }
        let back = direction(self.actor.position, next).map_err(self.fault())?;
        let short = checked_vec_add(next, checked_vec_scale(back, touch).map_err(self.fault())?)
            .map_err(self.fault())?;
        let left = distance(self.actor.position, short).map_err(self.fault())?;
        Ok((short, left))
    }

    /// Searches a route toward the target and adopts it. Breaks when the follower should rest:
    /// nothing nearer can be reached, or the nearest reachable point is within a step (D-4
    /// rule 3). At most one search runs per tick and per [`REPATH_COOLDOWN_TICKS`].
    fn search_follow_route(
        &mut self,
        scene: &mut MotionScene<'_>,
    ) -> Result<Stage, MovementInvariant> {
        self.proposal.memory.repath_cooldown = REPATH_COOLDOWN_TICKS;
        self.cooled_down = false;
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
            RouteSearch::Found(path) => path,
            RouteSearch::Retargeted(reachable, path) => {
                let step = i128::from(self.entry.speed.raw());
                if within_distance(self.actor.position, reachable, step) {
                    return Ok(Stage::Break(()));
                }
                path
            }
            RouteSearch::Blocked => return Ok(Stage::Break(())),
        };
        if self.proposal.route(self.actor).is_empty() {
            // Leaving the straight line for a detour lengthens the way on purpose.
            self.proposal.memory.progress_best = None;
        }
        self.proposal
            .adopt_route(self.actor.position, path.waypoints)
            .map_err(self.fault())?;
        Ok(Stage::Continue(()))
    }
}
