//! Rally points of production structures (plan `2026-11-23-production-queue` D-3 to D-7, B-60).
//!
//! # Responsibility
//! A production structure holds at most one [`RallyTarget`] in `Entity::rally`. This module owns
//! everything about it:
//! - **Setting it** (`SetRally`, D-3): admitted through the economy dispatcher for every
//!   recipient that is alive, completed, owned, unloaded and of an archetype that trains units
//!   (the capability; no identity is named). A rally on the recipient itself clears it.
//! - **Resolving it** ([`SimWorld::rally_point`], D-5): one function, so the exit order, the new
//!   unit's order and the player view agree.
//! - **The new unit's order** (D-4): after a spawn, the child takes the order its producer's rally
//!   means, through the admission a player's order takes. An ordinary refusal of that order (a
//!   point outside the child's terrain-radius domain, say) leaves the child idle, as a refused
//!   player order would; an invariant failure is reported as a diagnostic. This is the rule, not
//!   a silent fallback.
//! - **The town hall's auto-rally** (D-7): when an entity whose archetype carries an
//!   [`AutoRallySpec`] is created (a scenario's or fixture's `spawn_structure`, or a foundation
//!   laid by `Build`), its rally is set to a resource node, as 5.0.14's
//!   `CommandStructureAutoRally` does from the town halls' `CreateSet` effects (voidmulti
//!   `EffectData` 2507–2537).
//!
//! # How it fits
//! The searches read the map's resource nodes and the archetypes' capabilities, both run-time
//! data. Admission writes only the recipients' `rally`; the spawn writes only the child's orders
//! and, when the rally's unit is gone, clears the producer's rally (S7).
//!
//! # Invariants
//! - The chosen node is standing, has resources left, is of a kind in the spec's set, and its
//!   footprint's cells come within the spec's radius of the structure's centre.
//! - Among those, the node whose centre is farthest from the structure's centre wins
//!   (`TSFarthestDistance`, `MaxCount 1`), the lower node id breaking a tie.
//! - Distances compare squared in raw fixed point, so the choice is exact on every target.
//! - No node qualifies: the rally is left as it was (`None` on a new structure).

use crate::economy::{GameCommandError, GameCommandReceipt};
use crate::SimWorld;
use sim_types::{
    AutoRallySpec, EntityId, PlayerId, RallyTarget, ResourceNodeId, SimScalar, SimVec2, UnitOrder,
};

/// The squared distance between two points in raw Q32.32 units squared, exact in `i128`.
fn squared_raw(a: SimVec2, b: SimVec2) -> i128 {
    let dx = i128::from(a.x.raw()) - i128::from(b.x.raw());
    let dy = i128::from(a.y.raw()) - i128::from(b.y.raw());
    dx * dx + dy * dy
}

/// The squared distance in raw units from `point` to the box of cells starting at `cell` with
/// `width` × `height` cells; zero inside the box.
fn squared_raw_to_cells(point: SimVec2, cell: (u32, u32), width: u8, height: u8) -> i128 {
    let axis = |value: SimScalar, start: u32, extent: u8| {
        let low = i128::from(start) << 32;
        let high = (i128::from(start) + i128::from(extent)) << 32;
        let raw = i128::from(value.raw());
        (low - raw).max(raw - high).max(0)
    };
    let dx = axis(point.x, cell.0, width);
    let dy = axis(point.y, cell.1, height);
    dx * dx + dy * dy
}

impl SimWorld {
    /// The node `spec` chooses for a structure centred at `centre` (D-7), if any.
    pub(crate) fn auto_rally_node(
        &self,
        centre: SimVec2,
        spec: AutoRallySpec,
    ) -> Option<ResourceNodeId> {
        let reach = i128::from(spec.radius.raw());
        let reach = reach * reach;
        let mut best: Option<(i128, ResourceNodeId)> = None;
        for (index, node) in (0_u32..).zip(self.map.resources()) {
            let Some(placed) = node.placed else {
                continue;
            };
            if node.remaining == 0 || !spec.kinds.contains(node.kind) {
                continue;
            }
            let cells = placed.footprint;
            if squared_raw_to_cells(centre, cells.top_left, cells.width, cells.height) > reach {
                continue;
            }
            let distance = squared_raw(centre, node.pos);
            // Nodes are visited in id order, so a strictly farther node replaces the best and a
            // tie keeps the lower id.
            if best.is_none_or(|(farthest, _)| distance > farthest) {
                best = Some((distance, ResourceNodeId(index)));
            }
        }
        best.map(|(_, node)| node)
    }

    /// Sets the auto-rally of the newly created structure `id` when its archetype has one (D-7).
    /// A missing entity or archetype, or no qualifying node, leaves the rally untouched; the
    /// caller has just created the entity, so neither is missing in practice.
    pub(crate) fn apply_auto_rally(&mut self, id: EntityId) {
        let Some(entity) = self.entities.get(&id) else {
            return;
        };
        let Some(spec) = self
            .catalog
            .get(entity.unit_type_id)
            .ok()
            .and_then(|arch| arch.capabilities.auto_rally)
        else {
            return;
        };
        let Some(node) = self.auto_rally_node(entity.position, spec) else {
            return;
        };
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.rally = Some(RallyTarget::Node(node));
        }
    }
}

impl SimWorld {
    /// Admits `SetRally` (D-3): every capable recipient's rally becomes `target`, or is cleared
    /// when `target` is the recipient itself. Shift (`queue`) is ignored: there are no rally
    /// waypoints (B-165).
    ///
    /// A capable recipient is an alive, completed, owned and unloaded entity whose archetype
    /// trains units (S5: every production structure has a rally).
    ///
    /// # Errors
    /// The selection's own refusals; [`GameCommandError::NoCapableProducer`] when no recipient
    /// can rally; [`GameCommandError::RallyOutsideMap`] for a point off the map;
    /// [`GameCommandError::InvalidRallyTarget`] for a unit that does not exist, is dead or is
    /// another player's; [`GameCommandError::UnknownResourceNode`] for a node not on the map.
    pub(crate) fn admit_set_rally(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        target: RallyTarget,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.validate_selection(player, recipients)?;
        match target {
            RallyTarget::Point(point) => {
                let inside = |value: SimScalar, extent: u32| {
                    value >= SimScalar::ZERO && value <= SimScalar::from_num(extent)
                };
                if !inside(point.x, self.map.width()) || !inside(point.y, self.map.height()) {
                    return Err(GameCommandError::RallyOutsideMap(point));
                }
            }
            RallyTarget::Unit(unit) => {
                let valid = self
                    .entities
                    .get(&unit)
                    .is_some_and(|entity| entity.owner == player && entity.is_alive());
                if !valid {
                    return Err(GameCommandError::InvalidRallyTarget(unit));
                }
            }
            RallyTarget::Node(node) => {
                if self.map.resources().get(node.0 as usize).is_none() {
                    return Err(GameCommandError::UnknownResourceNode(node));
                }
            }
        }
        let mut count = 0;
        for &id in recipients {
            if !self.can_rally(id) {
                continue;
            }
            if let Some(entity) = self.entities.get_mut(&id) {
                entity.rally = (target != RallyTarget::Unit(id)).then_some(target);
                count += 1;
            }
        }
        if count == 0 {
            return Err(GameCommandError::NoCapableProducer);
        }
        Ok(GameCommandReceipt::Accepted { recipients: count })
    }

    /// Whether `id` can hold a rally: alive, completed, unloaded, and of an archetype that
    /// trains units (D-3). Ownership is the selection's check.
    fn can_rally(&self, id: EntityId) -> bool {
        self.entities.get(&id).is_some_and(|entity| {
            entity.is_alive()
                && entity.is_completed
                && !entity.is_loaded_in_transport()
                && self
                    .catalog
                    .get(entity.unit_type_id)
                    .is_ok_and(|arch| !arch.trains_units.is_empty())
        })
    }

    /// The point `producer`'s rally resolves to now, in world units (D-5): the point itself, the
    /// node's centre, or where the target unit stands while it is alive and not carried. `None`
    /// without a rally, for a rally to a unit that is dead, gone or carried, and for a node not on
    /// the map. The exit order, the new unit's order and the player view all read this one
    /// resolution, so they agree.
    #[must_use]
    pub fn rally_point(&self, producer: EntityId) -> Option<SimVec2> {
        match self.entities.get(&producer)?.rally? {
            RallyTarget::Point(point) => Some(point),
            RallyTarget::Node(node) => self.map.resources().get(node.0 as usize).map(|n| n.pos),
            RallyTarget::Unit(unit) => self
                .entities
                .get(&unit)
                .filter(|entity| entity.is_alive() && !entity.is_loaded_in_transport())
                .map(|entity| entity.position),
        }
    }

    /// Gives `child`, just made by `producer`, the order `producer`'s rally means (D-4), through
    /// the admission a player's order takes, so every rule of that order applies. Returns the
    /// order the child took, if any.
    ///
    /// | Rally | Order |
    /// |---|---|
    /// | none | none |
    /// | a point | `Move` to it |
    /// | a node | `Harvest` when admission accepts the child for it; else `Move` to its centre |
    /// | a unit, alive and not carried | `Load` when it is the player's and admission accepts the child as its passenger; else `Move` to where it stands now |
    /// | a unit, alive but carried | none; the rally stays |
    /// | a unit, dead or gone | none, and the rally is cleared |
    ///
    /// An ordinary refusal of the child's order leaves the child idle, as a refused player order
    /// would; an invariant failure is reported as a diagnostic and leaves it idle too.
    pub(super) fn give_rally_order(
        &mut self,
        producer: EntityId,
        child: EntityId,
    ) -> Option<UnitOrder> {
        let rally = self.entities.get(&producer)?.rally?;
        let owner = self.entities.get(&child)?.owner;
        let recipients = [child];
        let target = match rally {
            RallyTarget::Point(point) => point,
            RallyTarget::Node(node) => match self.admit_harvest(owner, &recipients, node, false) {
                Ok(_) => return Some(UnitOrder::Harvest { target: node }),
                Err(GameCommandError::Invariant(source)) => {
                    self.report_production_failure(source);
                    return None;
                }
                Err(_) => self.map.resources().get(node.0 as usize)?.pos,
            },
            RallyTarget::Unit(unit) => {
                let Some(target) = self.entities.get(&unit).filter(|e| e.is_alive()) else {
                    // "As if there'd never been a rally point" (S7).
                    if let Some(entity) = self.entities.get_mut(&producer) {
                        entity.rally = None;
                    }
                    return None;
                };
                if target.is_loaded_in_transport() {
                    return None;
                }
                let (position, own) = (target.position, target.owner == owner);
                if own {
                    match self.admit_load(owner, &recipients, unit, false) {
                        Ok(_) => return Some(UnitOrder::Load { carrier: unit }),
                        Err(GameCommandError::Invariant(source)) => {
                            self.report_production_failure(source);
                            return None;
                        }
                        Err(_) => {}
                    }
                }
                position
            }
        };
        // Movement admission reports its own invariant failures as diagnostics.
        let order = UnitOrder::Move { target };
        self.issue_movement_prevalidated(owner, &recipients, &order, false)
            .ok()
            .map(|_| order)
    }
}

#[cfg(test)]
mod tests {
    use super::{squared_raw, squared_raw_to_cells};
    use sim_types::{SimScalar, SimVec2};

    fn point(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    #[test]
    fn the_box_distance_is_zero_inside_and_the_gap_outside() {
        let one = 1_i128 << 32;
        assert_eq!(squared_raw_to_cells(point(7.0, 14.5), (6, 14), 2, 1), 0);
        // (8.5, 8.5) to the box [6, 8] × [14, 15]: the nearest point is (8, 14), 0.5 and 5.5 away.
        let expected = (one / 2) * (one / 2) + (11 * one / 2) * (11 * one / 2);
        assert_eq!(
            squared_raw_to_cells(point(8.5, 8.5), (6, 14), 2, 1),
            expected
        );
        assert_eq!(
            squared_raw(point(1.0, 2.0), point(4.0, 6.0)),
            25 * one * one
        );
    }
}
