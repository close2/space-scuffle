//! Route groups: ground units ordered together keep one lane (plan `2026-11-10-group-routing`).
//!
//! **Responsibility.** A group is one admitted `Move` or `AttackMove` command, or one scenario
//! initial order, with at least two ground recipients (D-1). This module forms groups, binds
//! their members by order ordinal ([`Entity::route_orders_issued`]), and drops memberships
//! whose order has been consumed.
//!
//! The lane rule (D-3, D-4) lives here too: once per tick, after reconciliation, each group
//! without a lane takes its leader's route as its lane; a member's route search then compares
//! its own route with the shortest route in the group's lane and adopts the lane when it is
//! within the tolerance.
//!
//! **Place in the system.** Groups are authoritative state of [`SimWorld`] and are hashed
//! (checksum schema 27). Movement admission (`issue_movement_prevalidated`) and the scenario's
//! initial orders form them; the movement phase reconciles them once per tick before any actor
//! moves. They exist only while the catalog's group-routing tolerance
//! ([`crate::catalog::MovementRules`]) is above 0: at 0 no group is ever formed, which is
//! `StarCraft` II's per-unit routing.
//!
//! **Invariants.**
//! - `SimWorld::route_groups` is sorted by [`RouteGroupId`] and ids are never reused.
//! - Each group's members are sorted by [`EntityId`] without duplicates, and no group is empty
//!   after reconciliation.
//! - A membership's ordinal is the ordinal of the order its command enqueued; it is kept while
//!   that order is active or still queued, never after.

use crate::{Entity, SimWorld};
use sim_map::navmesh::lanes::LaneWord;
use sim_map::{PathRequestWithVision, PathResult, Pathfinder, SimMap};
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2, Tick, UnitOrder};

use super::{MotionPolicy, MovementLayer, RouteOutcome, RouteOutcomeKind};

/// Identity of a route group, assigned in command order from 1 and never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RouteGroupId(pub u32);

/// The order kind a group was formed by. `Move` and `AttackMove` never share a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteGroupKind {
    /// A `Move` command.
    Move,
    /// An `AttackMove` command or scenario initial order.
    AttackMove,
}

/// A group's stored lane: its leader's route, computed once (D-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupLane {
    /// The lane polyline in world units: the leader's route's waypoints, which start at its
    /// position when the lane was computed and end at the group's target.
    pub polyline: Vec<SimVec2>,
    /// The lane word of [`Self::polyline`] (plan D-2).
    pub word: LaneWord,
    /// The member whose route this is (a diagnostic; the lane does not depend on it later).
    pub leader: EntityId,
    /// The leader's radius in world units when the lane was searched.
    pub radius: SimScalar,
}

/// Whether a group has its lane yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaneState {
    /// No member has had the group's order active at the start of a movement phase yet.
    Pending,
    /// The leader's search found no route to the target itself: members route alone.
    None,
    /// The lane the members follow when it is within tolerance.
    Lane(GroupLane),
}

/// One route group (D-1, D-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteGroup {
    /// Identity, in command order.
    pub id: RouteGroupId,
    /// The player whose command formed the group.
    pub player: PlayerId,
    /// The command's order kind.
    pub kind: RouteGroupKind,
    /// The command's target in world units.
    pub target: SimVec2,
    /// `(member, ordinal of the order the command enqueued for it)`, sorted by member.
    pub members: Vec<(EntityId, u32)>,
    /// The group's lane.
    pub lane: LaneState,
}

impl RouteGroup {
    /// The ordinal `member` holds in this group, if it is a member.
    #[must_use]
    pub fn ordinal_of(&self, member: EntityId) -> Option<u32> {
        self.members
            .binary_search_by_key(&member, |(id, _)| *id)
            .ok()
            .map(|index| self.members[index].1)
    }
}

/// Whether the order of ordinal `ordinal` has left the queue of a unit whose active route
/// ordinal is `active`: wrapping comparison, exact while fewer than 2^31 route orders separate
/// them.
#[must_use]
pub(crate) fn ordinal_consumed(ordinal: u32, active: u32) -> bool {
    let ahead = active.wrapping_sub(ordinal);
    ahead != 0 && ahead < 1 << 31
}

/// Whether `entity`'s active route order is the one of `ordinal` and is still queued.
#[must_use]
fn membership_is_active(entity: &Entity, ordinal: u32) -> bool {
    entity.active_route_ordinal() == ordinal
        && entity.order_queue.iter().any(crate::entity::is_route_order)
}

/// Raw Q32.32 coordinates of `v`, widened for exact products.
fn raw(v: SimVec2) -> (i128, i128) {
    (i128::from(v.x.raw()), i128::from(v.y.raw()))
}

/// The point of `polyline` nearest `p` and the index of the leg it lies on (D-4 step 2).
///
/// Exact in `i128` on raw Q32.32 values; the projection is rounded toward zero, so the point is
/// the same on every target. Ties go to the lowest leg index. `None` for a polyline with fewer
/// than two points.
fn join_point(polyline: &[SimVec2], p: SimVec2) -> Option<(usize, SimVec2)> {
    let (px, py) = raw(p);
    let mut best: Option<(i128, usize, SimVec2)> = None;
    for (k, leg) in polyline.windows(2).enumerate() {
        let ((ax, ay), (bx, by)) = (raw(leg[0]), raw(leg[1]));
        let (dx, dy) = (bx - ax, by - ay);
        let length_sq = dx * dx + dy * dy;
        let along = (px - ax) * dx + (py - ay) * dy;
        let (qx, qy) = if length_sq == 0 || along <= 0 {
            (ax, ay)
        } else if along >= length_sq {
            (bx, by)
        } else {
            // |d| < 2^42 and 0 < along < length_sq < 2^85 on any supported map, so the
            // products stay below 2^127.
            (ax + dx * along / length_sq, ay + dy * along / length_sq)
        };
        let distance_sq = (qx - px) * (qx - px) + (qy - py) * (qy - py);
        if best.as_ref().is_none_or(|(d, _, _)| distance_sq < *d) {
            // The point lies on a leg between two representable points, so it is representable.
            let q = SimVec2::new(
                SimScalar::from_raw(i64::try_from(qx).ok()?),
                SimScalar::from_raw(i64::try_from(qy).ok()?),
            );
            best = Some((distance_sq, k, q));
        }
    }
    best.map(|(_, k, q)| (k, q))
}

/// The group and lane of `actor`'s active route order, when it is a grouped `Move` or
/// `AttackMove` at the front of the queue whose group has a lane to `goal`.
fn lane_of<'g>(
    groups: &'g [RouteGroup],
    actor: &Entity,
    goal: SimVec2,
) -> Option<(RouteGroupId, &'g GroupLane)> {
    if !actor
        .order_queue
        .front()
        .is_some_and(crate::entity::is_route_order)
    {
        return None;
    }
    let active = actor.active_route_ordinal();
    groups.iter().find_map(|group| match &group.lane {
        LaneState::Lane(lane)
            if group.target == goal && group.ordinal_of(actor.id) == Some(active) =>
        {
            Some((group.id, lane))
        }
        _ => None,
    })
}

/// Whether a lane route of length `lane` is within `tolerance` permille of the own route's
/// length `own`: `lane × 1000 ≤ own × (1000 + tolerance)`, exact (D-4 step 6). Equality adopts.
#[must_use]
pub(crate) fn within_tolerance(lane: SimScalar, own: SimScalar, tolerance: u16) -> bool {
    i128::from(lane.raw()) * 1000 <= i128::from(own.raw()) * (1000 + i128::from(tolerance))
}

/// Everything a member's grouped route search reads besides its own route (D-4).
pub(super) struct GroupedSearch<'s, 'r> {
    /// The world's live route groups.
    pub groups: &'s [RouteGroup],
    /// The catalog's lane tolerance in permille; 0 disables the rule.
    pub tolerance: u16,
    /// The pathfinder, for the separation test, lane words and the restricted search.
    pub pathfinder: &'s mut Pathfinder,
    /// The map.
    pub map: &'s SimMap,
    /// The member's own route request (its position, radius, goal and knowledge).
    pub request: &'s PathRequestWithVision<'r>,
    /// Route events of this tick.
    pub events: &'s mut Vec<RouteOutcome>,
    /// The world's search counter.
    pub searches: &'s mut u64,
    /// The executing tick.
    pub tick: Tick,
}

impl GroupedSearch<'_, '_> {
    /// The route `actor` walks given its own route `own` to its order's goal `goal` (D-4).
    ///
    /// `own` unless the actor's active route order belongs to a group with a lane and the same
    /// target, the actor is not separated from the lane (step 2), its own route is in another
    /// lane (step 4), the lane admits it (step 5) and the lane route is within the tolerance
    /// (step 6).
    pub fn decide(&mut self, actor: &Entity, goal: SimVec2, own: PathResult) -> PathResult {
        if self.tolerance == 0 || self.groups.is_empty() {
            return own;
        }
        let Some((group, lane)) = lane_of(self.groups, actor, goal) else {
            return own;
        };
        let start = self.request.start;
        let Some((leg, join)) = join_point(&lane.polyline, start) else {
            return own;
        };
        let point_request = PathRequestWithVision {
            radius: SimScalar::ZERO,
            ..*self.request
        };
        if !self
            .pathfinder
            .segment_is_passable(self.map, &point_request, start, join)
        {
            // Separated: a wall or a known structure lies between the member and the lane.
            return own;
        }
        let mut target_path = Vec::with_capacity(lane.polyline.len() + 1 - leg);
        target_path.push(start);
        target_path.push(join);
        target_path.extend_from_slice(&lane.polyline[leg + 1..]);
        let target = self.pathfinder.lane_word(&target_path);
        if self.pathfinder.lane_word(&own.waypoints) == target {
            return own;
        }
        *self.searches += 1;
        let restricted = self
            .pathfinder
            .find_path_in_lane(self.map, self.request, &target)
            .ok();
        let adopted = restricted.as_ref().is_some_and(|route| {
            within_tolerance(route.total_length, own.total_length, self.tolerance)
        });
        self.events.push(RouteOutcome {
            tick: self.tick,
            entity: actor.id,
            kind: RouteOutcomeKind::GroupLane {
                group,
                own_length: own.total_length,
                lane_length: restricted.as_ref().map(|route| route.total_length),
                adopted,
            },
        });
        match restricted {
            Some(route) if adopted => route,
            _ => own,
        }
    }
}

impl SimWorld {
    /// The route groups alive in this world, sorted by id (plan `2026-11-10-group-routing`).
    #[must_use]
    pub fn route_groups(&self) -> &[RouteGroup] {
        &self.route_groups
    }

    /// The id the next route group will get.
    #[must_use]
    pub const fn next_route_group(&self) -> RouteGroupId {
        RouteGroupId(self.next_route_group)
    }

    /// Forms a route group from `recipients`, which have just been given `order` (D-1).
    ///
    /// Only ground recipients join; with fewer than two, or with the catalog's tolerance at 0,
    /// nothing is formed. Each member is bound to the ordinal of the route order the command
    /// just enqueued for it, the last one it received. `recipients` may be in any order.
    pub(crate) fn form_route_group(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        order: &UnitOrder,
    ) {
        if !self.catalog.movement_rules().groups_enabled() {
            return;
        }
        let (kind, target) = match *order {
            UnitOrder::Move { target } => (RouteGroupKind::Move, target),
            UnitOrder::AttackMove { target } => (RouteGroupKind::AttackMove, target),
            _ => return,
        };
        let mut members: Vec<(EntityId, u32)> = recipients
            .iter()
            .filter_map(|id| {
                let entity = self.entities.get(id)?;
                let policy = MotionPolicy::resolve(&self.catalog, &self.map, entity).ok()?;
                (policy.layer == MovementLayer::Ground
                    && entity.order_queue.back().is_some_and(|o| o == order))
                .then(|| (*id, entity.route_orders_issued.wrapping_sub(1)))
            })
            .collect();
        members.sort_unstable();
        members.dedup_by_key(|(id, _)| *id);
        if members.len() < 2 {
            return;
        }
        let id = RouteGroupId(self.next_route_group);
        self.next_route_group = self.next_route_group.wrapping_add(1);
        self.route_groups.push(RouteGroup {
            id,
            player,
            kind,
            target,
            members,
            lane: LaneState::Pending,
        });
    }

    /// Drops every membership whose entity is gone, dead or loaded, or whose order has been
    /// consumed, then every group left empty (D-1). Runs at the start of the movement phase.
    pub(crate) fn reconcile_route_groups(&mut self) {
        if self.route_groups.is_empty() {
            return;
        }
        let entities = &self.entities;
        for group in &mut self.route_groups {
            group.members.retain(|&(id, ordinal)| {
                entities.get(&id).is_some_and(|entity| {
                    entity.is_alive()
                        && !entity.is_loaded_in_transport()
                        && !ordinal_consumed(ordinal, entity.active_route_ordinal())
                })
            });
        }
        self.route_groups.retain(|group| !group.members.is_empty());
    }

    /// The lane pass (D-3): every group without a lane that has an active member takes its
    /// leader's route to the target as its lane, once. Reads tick-start positions; runs after
    /// reconciliation, before any actor moves.
    pub(crate) fn compute_group_lanes(&mut self) {
        if !self.catalog.movement_rules().groups_enabled() {
            return;
        }
        for index in 0..self.route_groups.len() {
            if self.route_groups[index].lane != LaneState::Pending {
                continue;
            }
            let Some((leader, radius)) = self.group_leader(&self.route_groups[index]) else {
                continue;
            };
            let lane = self.search_lane(leader, radius, self.route_groups[index].target);
            self.route_groups[index].lane = lane;
        }
    }

    /// The active ground member nearest the active members' centroid, with its radius; ties go
    /// to the smallest id. `None` when no member has the group's order active.
    fn group_leader(&self, group: &RouteGroup) -> Option<(EntityId, SimScalar)> {
        let mut active: Vec<(EntityId, SimVec2, SimScalar)> = Vec::new();
        for &(id, ordinal) in &group.members {
            let Some(entity) = self.entities.get(&id) else {
                continue;
            };
            let Ok(policy) = MotionPolicy::resolve(&self.catalog, &self.map, entity) else {
                continue;
            };
            if policy.layer == MovementLayer::Ground && membership_is_active(entity, ordinal) {
                active.push((id, entity.position, policy.radius));
            }
        }
        let count = i128::try_from(active.len()).ok().filter(|n| *n > 0)?;
        let (sum_x, sum_y) = active.iter().fold((0_i128, 0_i128), |(x, y), (_, p, _)| {
            let (px, py) = raw(*p);
            (x + px, y + py)
        });
        // Division rounds toward zero, the same on every target.
        let (cx, cy) = (sum_x / count, sum_y / count);
        active
            .iter()
            .min_by_key(|(id, p, _)| {
                let (px, py) = raw(*p);
                ((px - cx) * (px - cx) + (py - cy) * (py - cy), *id)
            })
            .map(|(id, _, radius)| (*id, *radius))
    }

    /// The leader's own route to `target` as a lane, or [`LaneState::None`] when it has no route
    /// to the target itself. Counted in [`Self::path_search_count`].
    fn search_lane(&mut self, leader: EntityId, radius: SimScalar, target: SimVec2) -> LaneState {
        let Some(entity) = self.entities.get(&leader) else {
            return LaneState::None;
        };
        let knowledge = entity
            .owner
            .slot()
            .filter(|_| self.vision_aware_pathfinding);
        let request = PathRequestWithVision {
            start: entity.position,
            goal: target,
            radius,
            vision: knowledge.map(|slot| &self.vision_maps[slot]),
            block_unexplored_ramps: false,
            known_structures: knowledge.map(|slot| self.known_blocked_cells[slot].as_slice()),
        };
        self.movement.path_searches += 1;
        match self.pathfinder.find_path_with_vision(&self.map, &request) {
            Ok(route) => {
                let word = self.pathfinder.lane_word(&route.waypoints);
                LaneState::Lane(GroupLane {
                    polyline: route.waypoints,
                    word,
                    leader,
                    radius,
                })
            }
            Err(_) => LaneState::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumption_is_a_wrapping_comparison() {
        assert!(!ordinal_consumed(5, 5));
        assert!(ordinal_consumed(4, 5));
        assert!(!ordinal_consumed(6, 5));
        assert!(ordinal_consumed(u32::MAX, 0));
        assert!(!ordinal_consumed(0, u32::MAX));
    }

    #[test]
    fn ordinal_lookup_uses_the_sorted_members() {
        let group = RouteGroup {
            id: RouteGroupId(1),
            player: PlayerId(0),
            kind: RouteGroupKind::Move,
            target: SimVec2::ZERO,
            members: vec![(EntityId(2), 7), (EntityId(5), 0)],
            lane: LaneState::Pending,
        };
        assert_eq!(group.ordinal_of(EntityId(5)), Some(0));
        assert_eq!(group.ordinal_of(EntityId(2)), Some(7));
        assert_eq!(group.ordinal_of(EntityId(3)), None);
    }
}
