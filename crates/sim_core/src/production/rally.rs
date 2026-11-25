//! Rally points of production structures (plans `2026-11-23-production-queue` D-3 to D-7, B-60,
//! and `2026-11-25-rallies` D-2 and D-3).
//!
//! # Responsibility
//! A production structure holds one [`RallyRoute`] per rally slot that has a rally, in
//! `Entity::rallies`. This module owns everything about them:
//! - **Setting them** (`SetRally`, D-3): admitted through the economy dispatcher for every
//!   recipient that is alive, completed, owned, unloaded and of an archetype that trains units
//!   (the capability; no identity is named). The command names a slot, or none for the smart
//!   command, and each recipient writes the slot that takes the target. A rally on the
//!   recipient itself clears that slot's route.
//! - **Resolving them** ([`SimWorld::rally_point`], D-5): one function, so the exit order, the
//!   new unit's order and the player view agree.
//! - **The new unit's order** (D-4): after a spawn, the child takes the order its producer's
//!   rally means, through the admission a player's order takes. An ordinary refusal of that
//!   order (a point outside the child's terrain-radius domain, say) leaves the child idle, as a
//!   refused player order would; an invariant failure is reported as a diagnostic. This is the
//!   rule, not a silent fallback.
//! - **The town hall's auto-rally** (D-7): when an entity whose archetype carries an
//!   [`AutoRallySpec`] is created (a scenario's or fixture's `spawn_structure`, or a foundation
//!   laid by `Build`), its rally is set to a resource node, as 5.0.14's
//!   `CommandStructureAutoRally` does from the town halls' `CreateSet` effects (voidmulti
//!   `EffectData` 2507–2537).
//!
//! # How it fits
//! The searches read the map's resource nodes and the archetypes' capabilities, both run-time
//! data. Admission writes only the recipients' `rallies`; the spawn writes only the child's
//! orders and, when the rally's unit is gone, removes the producer's route (S7).
//!
//! # Invariants
//! - A structure's routes are sorted by slot, at most one per slot, none empty, every slot
//!   below its rally ability's slot count and every route within its waypoint cap
//!   ([`validate_routes`]). Admission and the spawn check them before reading, and report a
//!   violation as a [`ProductionInvariant`].
//! - Every slot decision (which slot a command sets, which slot a new unit uses, how many
//!   slots and waypoints a structure has) is read from the archetype's rally ability through
//!   `rally_slots`, never from an identity.
//! - The auto-rally's chosen node is standing, has resources left, is of a kind in the spec's
//!   set, and its footprint's cells come within the spec's radius of the structure's centre.
//! - Among those, the node whose centre is farthest from the structure's centre wins
//!   (`TSFarthestDistance`, `MaxCount 1`), the lower node id breaking a tie.
//! - Distances compare squared in raw fixed point, so the choice is exact on every target.
//! - No node qualifies: the rally is left as it was (no route on a new structure).

use crate::economy::{GameCommandError, GameCommandReceipt};
use crate::production::rally_slots::RallyAbility;
use crate::production::ProductionInvariant;
use crate::{Entity, SimWorld};
use sim_types::{
    AutoRallySpec, EntityId, PlayerId, RallyRoute, RallyTarget, ResourceNodeId, SimScalar, SimVec2,
    UnitOrder, UnitTypeId,
};

/// The route of `slot` in `rallies`, sorted by slot, if it holds one.
fn route_of(rallies: &[RallyRoute], slot: u8) -> Option<&RallyRoute> {
    rallies
        .binary_search_by_key(&slot, |route| route.slot)
        .ok()
        .and_then(|index| rallies.get(index))
}

/// Makes `waypoints` the route of `slot` in `rallies`, keeping them sorted by slot.
fn set_route(rallies: &mut Vec<RallyRoute>, slot: u8, waypoints: Vec<RallyTarget>) {
    match rallies.binary_search_by_key(&slot, |route| route.slot) {
        Ok(index) => {
            if let Some(route) = rallies.get_mut(index) {
                route.waypoints = waypoints;
            }
        }
        Err(index) => rallies.insert(index, RallyRoute { slot, waypoints }),
    }
}

/// Removes the route of `slot` from `rallies`, if it holds one.
fn clear_route(rallies: &mut Vec<RallyRoute>, slot: u8) {
    rallies.retain(|route| route.slot != slot);
}

/// Checks the route invariants of `entity` (plan `2026-11-25-rallies` D-2) against a rally
/// ability of `slots` slots and `max_waypoints` waypoints per route.
///
/// # Errors
/// [`ProductionInvariant::EntityState`] naming the first broken rule.
pub(crate) fn validate_routes(
    entity: &Entity,
    slots: u8,
    max_waypoints: u8,
) -> Result<(), ProductionInvariant> {
    let fail = |reason| ProductionInvariant::EntityState {
        entity: entity.id,
        reason,
    };
    for (index, route) in entity.rallies.iter().enumerate() {
        if index > 0
            && entity
                .rallies
                .get(index - 1)
                .is_some_and(|prior| prior.slot >= route.slot)
        {
            return Err(fail("rally routes are not sorted by slot, one per slot"));
        }
        if route.waypoints.is_empty() {
            return Err(fail("a rally route is empty"));
        }
        if route.slot >= slots {
            return Err(fail(
                "a rally route's slot is beyond the rally ability's slots",
            ));
        }
        if route.waypoints.len() > usize::from(max_waypoints) {
            return Err(fail("a rally route holds more waypoints than its cap"));
        }
    }
    Ok(())
}

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
            set_route(
                &mut entity.rallies,
                spec.slot,
                vec![RallyTarget::Node(node)],
            );
        }
    }
}

impl SimWorld {
    /// Admits `SetRally` (D-3): every capable recipient whose rally ability has a slot that
    /// takes `target` (the named `slot`, or for `None` the first that takes it) makes `target`
    /// that slot's route, or clears the route when `target` is the recipient itself. Shift is
    /// not read yet: a rally holds one waypoint (B-165).
    ///
    /// A capable recipient is an alive, completed, owned and unloaded entity whose archetype
    /// trains units (S5: every production structure has a rally). Its routes are checked before
    /// any recipient is written, so a refusal changes nothing.
    ///
    /// # Errors
    /// The selection's own refusals; [`GameCommandError::NoCapableProducer`] when no recipient
    /// can rally; [`GameCommandError::NoRallySlotTakes`] when some can but none has a slot that
    /// takes the target; [`GameCommandError::RallyOutsideMap`] for a point off the map;
    /// [`GameCommandError::InvalidRallyTarget`] for a unit that does not exist, is dead or is
    /// another player's; [`GameCommandError::UnknownResourceNode`] for a node not on the map;
    /// [`GameCommandError::Invariant`] for a recipient whose routes break their invariants.
    pub(crate) fn admit_set_rally(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        target: RallyTarget,
        slot: Option<u8>,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.validate_selection(player, recipients)?;
        self.validate_rally_target(player, target)?;
        let (mut capable, mut taking) = (0, 0);
        for &id in recipients {
            let Some(ability) = self.rally_ability_of(id) else {
                continue;
            };
            capable += 1;
            if ability.slot_taking(slot, target).is_some() {
                if let Some(entity) = self.entities.get(&id) {
                    validate_routes(entity, ability.slot_count(), ability.max_waypoints())?;
                }
                taking += 1;
            }
        }
        if capable == 0 {
            return Err(GameCommandError::NoCapableProducer);
        }
        if taking == 0 {
            return Err(GameCommandError::NoRallySlotTakes);
        }
        for &id in recipients {
            let Some(chosen) = self
                .rally_ability_of(id)
                .and_then(|ability| ability.slot_taking(slot, target))
            else {
                continue;
            };
            if let Some(entity) = self.entities.get_mut(&id) {
                if target == RallyTarget::Unit(id) {
                    clear_route(&mut entity.rallies, chosen);
                } else {
                    set_route(&mut entity.rallies, chosen, vec![target]);
                }
            }
        }
        Ok(GameCommandReceipt::Accepted { recipients: taking })
    }

    /// The rally ability of `id` when it can hold a rally: alive, completed, unloaded, and of
    /// an archetype that trains units (D-3; S5: every production structure has a rally).
    /// Ownership is the selection's check.
    fn rally_ability_of(&self, id: EntityId) -> Option<RallyAbility<'_>> {
        let entity = self.entities.get(&id)?;
        let capable = entity.is_alive() && entity.is_completed && !entity.is_loaded_in_transport();
        let arch = self.catalog.get(entity.unit_type_id).ok()?;
        (capable && !arch.trains_units.is_empty()).then(|| RallyAbility::of(arch))
    }

    /// The rally slot a new unit of `product` made by `producer` uses (D-4), if any.
    pub(crate) fn rally_slot_for(&self, producer: EntityId, product: UnitTypeId) -> Option<u8> {
        let producer = self.entities.get(&producer)?;
        let arch = self.catalog.get(producer.unit_type_id).ok()?;
        let product = self.catalog.get(product).ok()?;
        RallyAbility::of(arch).slot_used_by(product)
    }

    /// Checks a rally target once per command (D-3): a point on the map, a live unit of
    /// `player`, or a node on the map.
    fn validate_rally_target(
        &self,
        player: PlayerId,
        target: RallyTarget,
    ) -> Result<(), GameCommandError> {
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
        Ok(())
    }

    /// The point the route of `producer`'s rally slot `slot` resolves to now, in world units
    /// (D-5; plan `2026-11-25-rallies` D-4): its first waypoint that resolves. `None` without a
    /// route in that slot, or when no waypoint resolves. The exit order, the new unit's order
    /// and the player view all read this one resolution, so they agree.
    #[must_use]
    pub fn rally_point(&self, producer: EntityId, slot: u8) -> Option<SimVec2> {
        let rallies = &self.entities.get(&producer)?.rallies;
        route_of(rallies, slot)?
            .waypoints
            .iter()
            .find_map(|&target| self.waypoint_point(target))
    }

    /// Where one waypoint resolves now, in world units: the point itself, the node's centre, or
    /// where the target unit stands while it is alive and not carried. `None` for a unit that is
    /// dead, gone or carried, and for a node not on the map.
    #[must_use]
    pub fn waypoint_point(&self, target: RallyTarget) -> Option<SimVec2> {
        match target {
            RallyTarget::Point(point) => Some(point),
            RallyTarget::Node(node) => self.map.resources().get(node.0 as usize).map(|n| n.pos),
            RallyTarget::Unit(unit) => self
                .entities
                .get(&unit)
                .filter(|entity| entity.is_alive() && !entity.is_loaded_in_transport())
                .map(|entity| entity.position),
        }
    }

    /// Gives `child`, just made by `producer`, the order the rally of the slot it uses means
    /// (D-4; plan `2026-11-25-rallies` D-4: no slot, or an empty one, gives none), through
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
    /// | a unit, dead or gone | none, and the route is removed |
    ///
    /// An ordinary refusal of the child's order leaves the child idle, as a refused player order
    /// would; an invariant failure is reported as a diagnostic and leaves it idle too.
    pub(super) fn give_rally_order(
        &mut self,
        producer: EntityId,
        child: EntityId,
    ) -> Option<UnitOrder> {
        let producer_entity = self.entities.get(&producer)?;
        let ability = RallyAbility::of(self.catalog.get(producer_entity.unit_type_id).ok()?);
        if let Err(source) = validate_routes(
            producer_entity,
            ability.slot_count(),
            ability.max_waypoints(),
        ) {
            self.report_production_failure(source);
            return None;
        }
        let child_entity = self.entities.get(&child)?;
        let owner = child_entity.owner;
        let slot = ability.slot_used_by(self.catalog.get(child_entity.unit_type_id).ok()?)?;
        let rally = *route_of(&producer_entity.rallies, slot)?
            .waypoints
            .first()?;
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
                        clear_route(&mut entity.rallies, slot);
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
