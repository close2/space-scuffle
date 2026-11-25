//! Rallies of production structures: where a structure sends the units it makes.
//!
//! # Responsibility
//! Defines the rally's data contract shared by the simulation, the catalog and the client
//! (plans `2026-11-23-production-queue` D-3 and `2026-11-25-rallies` D-2):
//! - [`RallyTarget`], one waypoint: a point, a unit or a resource node;
//! - [`RallyRoute`], the waypoints one slot of a structure holds, in the order the new unit
//!   walks them;
//! - [`RallySpec`], [`RallySlotSpec`] and [`RallyUsers`], a structure's rally ability as catalog
//!   data (D-1): its slots, which units use each and which targets each takes, and the
//!   waypoint cap.
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

const fn default_true() -> bool {
    true
}

/// Which units use a rally slot (5.0.14 `CAbilRally` `InfoArray` `UseFilters`; plan
/// `2026-11-25-rallies` D-1). A worker is a unit with the harvest capability.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RallyUsers {
    /// Every unit (no filter).
    #[default]
    All,
    /// Workers only (`Worker;-`).
    Workers,
    /// Every unit but workers (`-;Worker`).
    NonWorkers,
}

/// One rally of a production structure (one `InfoArray` entry of 5.0.14's `CAbilRally`; plan
/// `2026-11-25-rallies` D-1).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RallySlotSpec {
    /// The card entry's text, from data (the button face's name); not empty.
    pub label: String,
    /// Which units use this slot (`UseFilters`).
    #[serde(default)]
    pub used_by: RallyUsers,
    /// Unit slugs that never use it (`UseValidators`, e.g. `NotQueen`); each an existing unit,
    /// none twice.
    #[serde(default)]
    pub excludes: Vec<String>,
    /// Whether it may be set on a resource node (`SetValidators NotResourcesOrEnemyTargetType`
    /// makes it `false`).
    #[serde(default = "default_true")]
    pub on_resources: bool,
    /// Whether a right click (the smart command) on the ground sets it (`SetOnGround`). The
    /// card's entry for the slot sets it on the ground either way.
    #[serde(default = "default_true")]
    pub smart_on_ground: bool,
}

/// A production structure's rally ability (5.0.14 `CAbilRally`; plan `2026-11-25-rallies`
/// D-1): its slots and how many waypoints one slot's route holds.
///
/// An archetype that trains units and carries no spec has the default ability: one slot used by
/// every unit, taking every target, of one waypoint.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RallySpec {
    /// Most targets one slot's route holds; Shift appends until then (B-165). At least 1.
    pub max_waypoints: u8,
    /// The slots in `InfoArray` order; slot `i` is the card's `Rally{i+1}`. At least one, at
    /// most `u8::MAX`.
    pub slots: Vec<RallySlotSpec>,
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

    /// D-1: a slot authored with its label alone takes every unit and every target.
    #[test]
    fn a_slot_defaults_to_every_unit_and_every_target() -> Result<(), Box<dyn std::error::Error>> {
        let spec: super::RallySpec =
            ron::from_str(r#"(max_waypoints: 4, slots: [(label: "Set Rally Point")])"#)?;
        assert_eq!(spec.max_waypoints, 4);
        let slot = &spec.slots[0];
        assert_eq!(slot.used_by, super::RallyUsers::All);
        assert!(slot.excludes.is_empty() && slot.on_resources && slot.smart_on_ground);
        assert!(ron::from_str::<super::RallySpec>(
            r#"(max_waypoints: 4, slots: [(label: "x", colour: 1)])"#
        )
        .is_err());
        Ok(())
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
