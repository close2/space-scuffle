//! Rallies of production structures: where a structure sends the units it makes.
//!
//! # Responsibility
//! Defines the rally's data contract shared by the simulation, the catalog and the client
//! (plans `2026-11-23-production-queue` D-3 and `2026-11-25-rallies` D-2):
//! - [`RallyTarget`], one waypoint: a point, a unit or a resource node;
//! - [`RallyRoute`], the waypoints one slot of a structure holds, in the order the new unit
//!   walks them.
//!
//! # How it fits
//! `sim_core` stores a structure's routes in `Entity::rallies`, hashes them, and turns a route
//! into the new unit's orders when it leaves the structure. The client sends a
//! [`RallyTarget`] in `UnitOrder::SetRally`.
//!
//! # Invariants
//! A route is never empty; the simulation keeps its routes sorted by slot, one per slot, and no
//! longer than its archetype's waypoint cap (checked in `sim_core`'s production invariants).

use crate::ids::{EntityId, ResourceNodeId};
use crate::math::SimVec2;
use serde::{Deserialize, Serialize};

/// Where a production structure sends the units it makes (`StarCraft` II's rally, 5.0.14
/// `CAbilRally`, plan `2026-11-23-production-queue` D-3): one waypoint of a [`RallyRoute`].
///
/// The simulation resolves it to a point (world units) when a unit leaves the structure, when
/// the exit is chosen and when the player view publishes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RallyTarget {
    /// A point on the map, world units.
    Point(SimVec2),
    /// A unit: new units go where it stands when they leave, or board it when it is the
    /// player's carrier.
    Unit(EntityId),
    /// A resource node: workers harvest it, other units go to its centre.
    Node(ResourceNodeId),
}

/// The rally one slot of a production structure holds (plan `2026-11-25-rallies` D-2): its
/// waypoints in the order a new unit walks them.
///
/// `slot` indexes the structure's rally slots (`InfoArray` order of 5.0.14's `CAbilRally`;
/// slot `i` is the card's `Rally{i+1}`). `waypoints` is never empty in a stored route.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RallyRoute {
    /// The rally slot this route belongs to, below the structure's slot count.
    pub slot: u8,
    /// The targets, first walked first; at least one, at most the structure's waypoint cap.
    pub waypoints: Vec<RallyTarget>,
}

impl RallyRoute {
    /// A route of slot `slot` with the one waypoint `target`.
    #[must_use]
    pub fn single(slot: u8, target: RallyTarget) -> Self {
        Self {
            slot,
            waypoints: vec![target],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RallyRoute, RallyTarget};
    use crate::ids::{EntityId, ResourceNodeId};

    #[test]
    fn a_single_route_holds_its_one_waypoint() {
        let route = RallyRoute::single(1, RallyTarget::Node(ResourceNodeId(3)));
        assert_eq!(route.slot, 1);
        assert_eq!(route.waypoints, [RallyTarget::Node(ResourceNodeId(3))]);
    }

    #[test]
    fn routes_round_trip_through_ron() -> Result<(), Box<dyn std::error::Error>> {
        let route = RallyRoute {
            slot: 0,
            waypoints: vec![
                RallyTarget::Unit(EntityId(7)),
                RallyTarget::Node(ResourceNodeId(2)),
            ],
        };
        let text = ron::to_string(&route)?;
        assert_eq!(ron::from_str::<RallyRoute>(&text)?, route);
        Ok(())
    }
}
