//! Route groups: ground units ordered together keep one lane (plan `2026-11-10-group-routing`).
//!
//! **Responsibility.** A group is one admitted `Move` or `AttackMove` command, or one scenario
//! initial order, with at least two ground recipients (D-1). This module forms groups, binds
//! their members by order ordinal ([`Entity::route_orders_issued`]), and drops memberships
//! whose order has been consumed.
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

use crate::SimWorld;
use sim_map::navmesh::lanes::LaneWord;
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2, UnitOrder};

use super::{MotionPolicy, MovementLayer};

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
    /// The lane polyline in world units: the leader's position when the lane was computed,
    /// then its route's waypoints to the group's target.
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
