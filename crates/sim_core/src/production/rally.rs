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
use crate::movement::MovementInvariant;
use crate::production::rally_slots::RallyAbility;
use crate::production::ProductionInvariant;
use crate::weapon_timing::SimulationDiagnostic;
use crate::{Entity, SimWorld};
use sim_types::{
    AttackOrigin, AutoRallySpec, EntityId, FollowKind, PlayerId, RallyRoute, RallyTarget,
    ResourceNodeId, SimScalar, SimVec2, Tick, UnitOrder, UnitTypeId,
};

/// The route of `slot` in `rallies`, sorted by slot, if it holds one.
fn route_of(rallies: &[RallyRoute], slot: u8) -> Option<&RallyRoute> {
    rallies
        .binary_search_by_key(&slot, |route| route.slot)
        .ok()
        .and_then(|index| rallies.get(index))
}

/// The route of `slot` in `rallies`, sorted by slot, for writing, if it holds one.
fn route_of_mut(rallies: &mut [RallyRoute], slot: u8) -> Option<&mut RallyRoute> {
    rallies
        .binary_search_by_key(&slot, |route| route.slot)
        .ok()
        .and_then(|index| rallies.get_mut(index))
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

/// What `SetRally` does to one recipient's route (plan `2026-11-25-rallies` D-3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RallyWrite {
    /// The target is the recipient itself: the slot's route is removed.
    Clear(u8),
    /// The slot's route becomes the one target (no Shift, or an empty route).
    Set(u8),
    /// Shift: the target is appended to the slot's route.
    Append(u8),
    /// Shift on a route that holds its cap of waypoints: not taken (B-215).
    Full,
}

impl SimWorld {
    /// Admits `SetRally` (D-3): every capable recipient whose rally ability has a slot that
    /// takes `target` (the named `slot`, or for `None` the first that takes it) writes that
    /// slot's route. Without `queue` the route becomes `[target]`; with `queue` (Shift) the
    /// target is appended while the route holds fewer than the ability's `max_waypoints`, and a
    /// full route does not take it (B-215). An empty route is set as if without Shift. A target
    /// equal to the recipient itself clears the slot's route, with or without Shift.
    ///
    /// A capable recipient is an alive, completed, owned and unloaded entity whose archetype
    /// trains units (S5: every production structure has a rally). Its routes are checked before
    /// any recipient is written, so a refusal changes nothing.
    ///
    /// # Errors
    /// The selection's own refusals; [`GameCommandError::NoCapableProducer`] when no recipient
    /// can rally; [`GameCommandError::NoRallySlotTakes`] when some can but none has a slot that
    /// takes the target; [`GameCommandError::RallyWaypointsFull`] when every recipient whose slot
    /// takes it holds a full route; [`GameCommandError::RallyOutsideMap`] for a point off the
    /// map; [`GameCommandError::InvalidRallyTarget`] for a unit that does not exist or is dead;
    /// [`GameCommandError::UnknownResourceNode`] for a node not on the map;
    /// [`GameCommandError::Invariant`] for a recipient whose routes break their invariants.
    pub(crate) fn admit_set_rally(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        target: RallyTarget,
        slot: Option<u8>,
        queue: bool,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.validate_selection(player, recipients)?;
        self.validate_rally_target(target)?;
        let (mut capable, mut taking, mut full) = (0, 0, 0);
        for &id in recipients {
            if self.rally_ability_of(id).is_none() {
                continue;
            }
            capable += 1;
            match self.rally_write(id, slot, target, queue)? {
                Some(RallyWrite::Full) => full += 1,
                Some(_) => taking += 1,
                None => {}
            }
        }
        if capable == 0 {
            return Err(GameCommandError::NoCapableProducer);
        }
        if taking == 0 {
            return Err(if full > 0 {
                GameCommandError::RallyWaypointsFull
            } else {
                GameCommandError::NoRallySlotTakes
            });
        }
        for &id in recipients {
            let write = self.rally_write(id, slot, target, queue)?;
            let Some(entity) = self.entities.get_mut(&id) else {
                continue;
            };
            match write {
                Some(RallyWrite::Clear(chosen)) => clear_route(&mut entity.rallies, chosen),
                Some(RallyWrite::Set(chosen)) => {
                    set_route(&mut entity.rallies, chosen, vec![target]);
                }
                Some(RallyWrite::Append(chosen)) => {
                    if let Some(route) = route_of_mut(&mut entity.rallies, chosen) {
                        route.waypoints.push(target);
                    }
                }
                Some(RallyWrite::Full) | None => {}
            }
        }
        Ok(GameCommandReceipt::Accepted { recipients: taking })
    }

    /// What `SetRally` does to recipient `id` (D-3), or `None` when it cannot rally or has no
    /// slot that takes `target`. Read-only, so admission can count before it writes.
    ///
    /// # Errors
    /// [`GameCommandError::Invariant`] when the recipient's routes break their invariants.
    fn rally_write(
        &self,
        id: EntityId,
        slot: Option<u8>,
        target: RallyTarget,
        queue: bool,
    ) -> Result<Option<RallyWrite>, GameCommandError> {
        let (Some(ability), Some(entity)) = (self.rally_ability_of(id), self.entities.get(&id))
        else {
            return Ok(None);
        };
        let Some(chosen) = ability.slot_taking(slot, target) else {
            return Ok(None);
        };
        validate_routes(entity, ability.slot_count(), ability.max_waypoints())?;
        if target == RallyTarget::Unit(id) {
            return Ok(Some(RallyWrite::Clear(chosen)));
        }
        let held = route_of(&entity.rallies, chosen).map_or(0, |route| route.waypoints.len());
        Ok(Some(if !queue || held == 0 {
            RallyWrite::Set(chosen)
        } else if held < usize::from(ability.max_waypoints()) {
            RallyWrite::Append(chosen)
        } else {
            RallyWrite::Full
        }))
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

    /// Checks a rally target once per command (D-3): a point on the map, a live unit of any
    /// player (D-6), or a node on the map.
    fn validate_rally_target(&self, target: RallyTarget) -> Result<(), GameCommandError> {
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
                // Any player's live unit (plan `2026-11-25-rallies` D-6, B-166); visibility is
                // not checked, as for a player's attack order.
                let valid = self.entities.get(&unit).is_some_and(Entity::is_alive);
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

    /// Gives `child`, just made by `producer`, the orders the route of the rally slot it uses
    /// means (plan `2026-11-25-rallies` D-4, D-5): no slot, or an empty one, gives none.
    ///
    /// 1. **Prune.** Every `Unit` waypoint whose unit is dead or gone is removed from the route
    ///    (S10, S11); a route left empty is removed and gives no order ("as if there'd never
    ///    been a rally point", S9).
    /// 2. **Issue.** Each remaining waypoint, in order, gives the order it means through the
    ///    admission a player's order takes, the first order issued replacing the child's orders
    ///    and every later one queued:
    ///
    /// | Waypoint | Order |
    /// |---|---|
    /// | a point | `Move` to it |
    /// | a node | `Harvest` when admission accepts the child for it; else `Move` to its centre |
    /// | a unit of the child's player, not carried | `Load` when admission accepts the child as its passenger; else `Move` to where it stands now (B-163) |
    /// | another player's unit, not carried | `AttackTarget { origin: Explicit }` when the child can attack it (S11, D-6); else `Move` to where it stands now (Q0105's rule, B-187) |
    /// | a unit, carried | none this time; the waypoint stays |
    ///
    /// A refused order is skipped, so the next waypoint's order becomes the first issued, as a
    /// refused player order would leave the unit. An invariant failure is reported as a
    /// diagnostic and ends the issuing. Orders queued after a `Harvest` never run, since the
    /// harvest does not end; that is the game's route that ends in minerals (S10 post 25).
    // PERFORMANCE RATIONALE:
    // Why: A spawn issues up to `max_waypoints` admissions and must not allocate.
    // What: The route is read by index, one waypoint copied at a time, rather than cloned.
    // Trade-off: The route is looked up again per waypoint, O(slots) each, on a small cap.
    pub(super) fn give_rally_order(&mut self, producer: EntityId, child: EntityId) {
        let Some(slot) = self.child_rally_slot(producer, child) else {
            return;
        };
        if !self.prune_rally_route(producer, slot) {
            return;
        }
        let mut issued = false;
        for index in 0.. {
            let Some(target) = self
                .entities
                .get(&producer)
                .and_then(|entity| route_of(&entity.rallies, slot))
                .and_then(|route| route.waypoints.get(index))
                .copied()
            else {
                break;
            };
            match self.issue_waypoint_order(child, target, issued) {
                Ok(taken) => issued |= taken,
                Err(source) => {
                    self.report_production_failure(source);
                    break;
                }
            }
        }
    }

    /// The rally slot `child` uses at `producer` (D-4), after checking the producer's route
    /// invariants; a violation is reported as a diagnostic and gives no slot.
    fn child_rally_slot(&mut self, producer: EntityId, child: EntityId) -> Option<u8> {
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
        let child_type = self.entities.get(&child)?.unit_type_id;
        ability.slot_used_by(self.catalog.get(child_type).ok()?)
    }

    /// Removes every `Unit` waypoint of `producer`'s route in `slot` whose unit is dead or gone,
    /// and the route itself when nothing is left (D-5). Whether a route remains.
    fn prune_rally_route(&mut self, producer: EntityId, slot: u8) -> bool {
        let Some(route) = self
            .entities
            .get_mut(&producer)
            .and_then(|entity| route_of_mut(&mut entity.rallies, slot))
        else {
            return false;
        };
        let mut waypoints = std::mem::take(&mut route.waypoints);
        waypoints.retain(|waypoint| match *waypoint {
            RallyTarget::Unit(unit) => self.entities.get(&unit).is_some_and(Entity::is_alive),
            RallyTarget::Point(_) | RallyTarget::Node(_) => true,
        });
        let Some(entity) = self.entities.get_mut(&producer) else {
            return false;
        };
        if waypoints.is_empty() {
            clear_route(&mut entity.rallies, slot);
            return false;
        }
        if let Some(route) = route_of_mut(&mut entity.rallies, slot) {
            route.waypoints = waypoints;
        }
        true
    }

    /// Issues `child` the order one waypoint means (D-5), queued behind its orders when `queue`.
    /// Whether the child took an order; a refusal is not an error.
    ///
    /// A unit waypoint the child neither boards (one's own carrier) nor attacks (an enemy it can
    /// attack) is followed when the unit is mobile (plan `2026-11-26-follow-order` D-9; B-163,
    /// B-166): 5.0.14's rally on a unit issues a follow. A structure stays a point move.
    ///
    /// # Errors
    /// The [`ProductionInvariant`] a harvest or load admission reports.
    fn issue_waypoint_order(
        &mut self,
        child: EntityId,
        target: RallyTarget,
        queue: bool,
    ) -> Result<bool, ProductionInvariant> {
        let Some(owner) = self.entities.get(&child).map(|entity| entity.owner) else {
            return Ok(false);
        };
        let recipients = [child];
        let point = match target {
            RallyTarget::Point(point) => point,
            RallyTarget::Node(node) => match self.admit_harvest(owner, &recipients, node, queue) {
                Ok(_) => return Ok(true),
                Err(GameCommandError::Invariant(source)) => return Err(source),
                Err(_) => match self.map.resources().get(node.0 as usize) {
                    Some(node) => node.pos,
                    None => return Ok(false),
                },
            },
            RallyTarget::Unit(unit) => {
                let Some(target) = self
                    .entities
                    .get(&unit)
                    .filter(|entity| entity.is_alive() && !entity.is_loaded_in_transport())
                else {
                    return Ok(false);
                };
                let (position, own) = (target.position, target.owner == owner);
                let mobile = self
                    .catalog
                    .get(target.unit_type_id)
                    .is_ok_and(|archetype| !archetype.is_immobile_by_default());
                if own {
                    match self.admit_load(owner, &recipients, unit, queue) {
                        Ok(_) => return Ok(true),
                        Err(GameCommandError::Invariant(source)) => return Err(source),
                        Err(_) => {}
                    }
                } else if let Some(taken) = self.issue_rally_attack(owner, child, unit, queue) {
                    return Ok(taken);
                }
                if mobile {
                    // Movement admission reports its own invariant failures as diagnostics.
                    return Ok(self
                        .issue_follow_prevalidated(
                            owner,
                            &recipients,
                            unit,
                            FollowKind::Move,
                            queue,
                        )
                        .is_ok_and(|receipt| receipt.recipient_count > 0));
                }
                position
            }
        };
        // Movement admission reports its own invariant failures as diagnostics.
        let order = UnitOrder::Move { target: point };
        Ok(self
            .issue_movement_prevalidated(owner, &recipients, &order, queue)
            .is_ok())
    }

    /// Gives `child` of `owner` an explicit attack on the enemy `unit` when the player's own
    /// admission rule takes it ([`SimWorld::takes_order`]; plan `2026-11-25-rallies` D-6),
    /// queued when `queue`. `Some(true)` when issued; `Some(false)` when the entity did not take
    /// it, or its issue failed an invariant, which is reported as a diagnostic; `None` when the child
    /// cannot attack the unit, so the caller moves it there instead.
    fn issue_rally_attack(
        &mut self,
        owner: PlayerId,
        child: EntityId,
        unit: EntityId,
        queue: bool,
    ) -> Option<bool> {
        let order = UnitOrder::AttackTarget {
            target: unit,
            origin: AttackOrigin::Explicit,
        };
        if !self.takes_order(owner, child, &order) {
            return None;
        }
        let entity = self.entities.get_mut(&child)?;
        match entity.issue_order(order, queue) {
            Ok(taken) => Some(taken),
            Err(source) => {
                self.tick_diagnostics
                    .push(SimulationDiagnostic::MovementInvariant {
                        tick: Tick(self.current_tick),
                        source: MovementInvariant::Admission {
                            entity: child,
                            source,
                        },
                    });
                Some(false)
            }
        }
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
