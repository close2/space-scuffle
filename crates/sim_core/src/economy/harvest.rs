//! The harvester state machine and income.
//!
//! Driven only by the archetype's `HarvesterSpec`: any unit with one gathers, whatever it is.
//!
//! # The cycle
//! A `Harvest` order alternates two states, chosen by whether the unit holds cargo:
//! - **Empty:** walk to the node, stay for `harvest_rate_ticks`, then take up to
//!   `harvest_capacity` units from it. The node is drawn down at that moment.
//! - **Carrying:** walk to the nearest completed friendly depot. Income is credited on
//!   arrival, not when gathered, so cargo lost with its carrier was never income.
//!
//! # Rules
//! - Workers gathering or returning walk through other units, as mineral walking does in
//!   `StarCraft` II, so a crowded line never jams on itself. A node has no worker limit.
//! - An exhausted node sends its worker to the nearest live node of the same kind within
//!   [`NEXT_NODE_RADIUS`]. With none left the order ends with an event.
//! - Resource nodes are not obstacles, and the map keeps exhausted ones so identifiers hold.
//! - Gas yields only through a completed extractor owned by the harvester's player, standing
//!   over the node. Workers gather from the extractor's edge.
//! - A unit holding cargo delivers before it gathers again, so cargo of the other kind is
//!   never thrown away.
//! - With no depot left, the order ends and the cargo stays on the unit.

use super::{Approach, Footprint, GameCommandError, GameCommandReceipt, TaskEventKind};
use crate::entity::Cargo;
use crate::movement::math::squared_distance;
use crate::production::ProductionInvariant;
use crate::SimWorld;
use sim_types::{
    AnimState, EntityId, HarvesterSpec, PlayerId, ResourceKind, ResourceNodeId, SimScalar, SimVec2,
    UnitOrder,
};

/// How far an exhausted node's workers look for the next one, in world units.
const NEXT_NODE_RADIUS: SimScalar = SimScalar::from_raw(12 << 32);

impl SimWorld {
    /// The footprint a player's worker gathers from at `node`, if it can gather there now.
    fn gather_site(&self, player: PlayerId, node: ResourceNodeId) -> Option<Footprint> {
        let resource = self.map.resources().get(node.0 as usize)?;
        match resource.kind {
            ResourceKind::Mineral => Some(Footprint::cell(resource.cell)),
            ResourceKind::Gas => {
                let structure = self
                    .map
                    .cell(resource.cell.0, resource.cell.1)
                    .ok()?
                    .occupying_structure?;
                let extractor = self.entities.get(&structure)?;
                let working = extractor.owner == player
                    && extractor.is_alive()
                    && extractor.is_completed
                    && self
                        .catalog
                        .get(extractor.unit_type_id)
                        .is_ok_and(|arch| arch.capabilities.is_gas_extractor);
                working.then(|| {
                    self.map
                        .structures()
                        .get(&structure)
                        .map(Footprint::of_structure)
                })?
            }
        }
    }

    /// Admits `Harvest` for every capable selected unit.
    pub(crate) fn admit_harvest(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        target: ResourceNodeId,
        queue: bool,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.validate_selection(player, recipients)?;
        let node = self
            .map
            .resources()
            .get(target.0 as usize)
            .ok_or(GameCommandError::UnknownResourceNode(target))?;
        if node.remaining == 0 {
            return Err(GameCommandError::ResourceExhausted(target));
        }
        if self.gather_site(player, target).is_none() {
            return Err(GameCommandError::ExtractorRequired(target));
        }
        let mut count = 0;
        for &id in recipients {
            if self.harvester_spec(id).is_some() {
                self.commit_task_order(id, UnitOrder::Harvest { target }, queue)?;
                count += 1;
            }
        }
        if count == 0 {
            return Err(GameCommandError::NoCapableHarvester);
        }
        Ok(GameCommandReceipt::Accepted { recipients: count })
    }

    /// Admits `ReturnCargo` for every selected unit holding cargo.
    pub(crate) fn admit_return_cargo(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        queue: bool,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.validate_selection(player, recipients)?;
        let mut count = 0;
        for &id in recipients {
            if self.entities.get(&id).is_some_and(|e| e.cargo.is_some()) {
                self.commit_task_order(id, UnitOrder::ReturnCargo, queue)?;
                count += 1;
            }
        }
        if count == 0 {
            return Err(GameCommandError::NoCargo);
        }
        Ok(GameCommandReceipt::Accepted { recipients: count })
    }

    fn harvester_spec(&self, id: EntityId) -> Option<HarvesterSpec> {
        let entity = self.entities.get(&id)?;
        self.catalog
            .get(entity.unit_type_id)
            .ok()?
            .capabilities
            .harvester
    }

    /// One tick of a `Harvest` order at the front of `id`'s queue.
    pub(crate) fn step_harvest(&mut self, id: EntityId, target: ResourceNodeId) {
        let Some(spec) = self.harvester_spec(id) else {
            self.end_task(id, Some(TaskEventKind::NotCapable));
            return;
        };
        let Some(actor) = self.entities.get_mut(&id) else {
            return;
        };
        actor.is_mineral_walking = true;
        let owner = actor.owner;
        let position = actor.position;
        if actor.cargo.is_some() {
            actor.task.gather_ticks = 0;
            self.step_delivery(id, owner, position);
            return;
        }
        let Some(node) = self.map.resources().get(target.0 as usize) else {
            self.end_task(id, Some(TaskEventKind::UnknownResourceNode));
            return;
        };
        if node.remaining == 0 {
            let kind = node.kind;
            match self.next_node(owner, kind, position) {
                Some(next) => self.replace_task_order(id, UnitOrder::Harvest { target: next }),
                None => self.end_task(id, Some(TaskEventKind::ResourceExhausted)),
            }
            return;
        }
        let Some(site) = self.gather_site(owner, target) else {
            self.end_task(id, Some(TaskEventKind::ExtractorLost));
            return;
        };
        self.step_gather(id, target, site, spec);
    }

    fn step_gather(
        &mut self,
        id: EntityId,
        target: ResourceNodeId,
        site: Footprint,
        spec: HarvesterSpec,
    ) {
        let gathering = self
            .entities
            .get(&id)
            .is_some_and(|actor| actor.task.gather_ticks > 0);
        if !gathering {
            match self.approach_footprint(id, site) {
                Approach::Walking => {}
                Approach::Unreachable => self.end_task(id, Some(TaskEventKind::Unreachable)),
                Approach::Arrived => {
                    if let Some(actor) = self.entities.get_mut(&id) {
                        actor.task.gather_ticks = spec.harvest_rate_ticks.max(1);
                        actor.current_anim = AnimState::Cast;
                    }
                }
            }
            return;
        }
        let Some(actor) = self.entities.get_mut(&id) else {
            return;
        };
        actor.task.gather_ticks -= 1;
        if actor.task.gather_ticks > 0 {
            return;
        }
        let Some(node) = self.map.resources_mut().get_mut(target.0 as usize) else {
            return;
        };
        let amount = spec.harvest_capacity.min(node.remaining);
        node.remaining -= amount;
        let kind = node.kind;
        if let Some(actor) = self.entities.get_mut(&id) {
            actor.current_anim = AnimState::Idle;
            actor.task.last_node = Some(target);
            actor.task.depot = None;
            if amount > 0 {
                actor.cargo = Some(Cargo { kind, amount });
            }
        }
    }

    /// The nearest node of `kind` near `from` that `player` can gather from now.
    fn next_node(
        &self,
        player: PlayerId,
        kind: ResourceKind,
        from: SimVec2,
    ) -> Option<ResourceNodeId> {
        let reach = i128::from(NEXT_NODE_RADIUS.raw());
        self.map
            .resources()
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == kind && node.remaining > 0)
            .filter(|(_, node)| crate::movement::math::within_distance(from, node.pos, reach))
            .filter_map(|(index, node)| {
                let id = ResourceNodeId(u32::try_from(index).ok()?);
                self.gather_site(player, id)?;
                Some((squared_distance(from, node.pos), id))
            })
            .min()
            .map(|(_, id)| id)
    }

    /// The completed friendly depot nearest `from`; the lowest ID wins a tie.
    fn nearest_depot(&self, owner: PlayerId, from: SimVec2) -> Option<EntityId> {
        // PERFORMANCE RATIONALE:
        // Why: Scanning all entities per trip would make income O(N) per delivery.
        // What: Only structures can be depots, and the map holds them in ID order.
        // Trade-off: None for readers; a depot must be a placed structure to count.
        self.map
            .structures()
            .iter()
            .filter(|(id, _)| self.is_depot(**id, owner))
            .filter_map(|(id, placed)| Some((Footprint::of_structure(placed).gap(from)?, *id)))
            .min()
            .map(|(_, id)| id)
    }

    fn is_depot(&self, id: EntityId, owner: PlayerId) -> bool {
        self.entities.get(&id).is_some_and(|depot| {
            depot.owner == owner
                && depot.is_alive()
                && depot.is_completed
                && self
                    .catalog
                    .get(depot.unit_type_id)
                    .is_ok_and(|arch| arch.is_resource_dropoff)
        })
    }

    /// Walks a carrying unit to its depot and banks the cargo there; true once delivered.
    fn step_delivery(&mut self, id: EntityId, owner: PlayerId, position: SimVec2) -> bool {
        let remembered = self
            .entities
            .get(&id)
            .and_then(|actor| actor.task.depot)
            .filter(|&depot| self.is_depot(depot, owner));
        let depot = if let Some(depot) = remembered {
            depot
        } else {
            let Some(nearest) = self.nearest_depot(owner, position) else {
                self.end_task(id, Some(TaskEventKind::NoDepot));
                return false;
            };
            if let Some(actor) = self.entities.get_mut(&id) {
                actor.task.depot = Some(nearest);
                actor.task.goal = None;
            }
            nearest
        };
        let Some(site) = self
            .map
            .structures()
            .get(&depot)
            .map(Footprint::of_structure)
        else {
            return false;
        };
        match self.approach_footprint(id, site) {
            Approach::Walking => false,
            Approach::Unreachable => {
                self.end_task(id, Some(TaskEventKind::Unreachable));
                false
            }
            Approach::Arrived => self.bank_cargo(id, owner),
        }
    }

    /// Credits `id`'s cargo to its owner; on overflow the cargo stays and a diagnostic fires.
    fn bank_cargo(&mut self, id: EntityId, owner: PlayerId) -> bool {
        let Some(actor) = self.entities.get_mut(&id) else {
            return false;
        };
        let Some(cargo) = actor.cargo else {
            return false;
        };
        let resources = &mut self.player_resources[usize::from(owner.0)];
        let reserve = match cargo.kind {
            ResourceKind::Mineral => &mut resources.minerals,
            ResourceKind::Gas => &mut resources.gas,
        };
        let Some(total) = reserve.checked_add(cargo.amount) else {
            self.report_production_failure(ProductionInvariant::Arithmetic {
                operation: "harvest income",
            });
            return false;
        };
        *reserve = total;
        actor.cargo = None;
        actor.task.depot = None;
        true
    }

    /// One tick of a `ReturnCargo` order at the front of `id`'s queue.
    ///
    /// Once delivered the unit goes back to the node it last gathered from, if that still
    /// has anything; otherwise the order simply ends.
    pub(crate) fn step_return_cargo(&mut self, id: EntityId) {
        let Some(actor) = self.entities.get_mut(&id) else {
            return;
        };
        actor.is_mineral_walking = true;
        if actor.cargo.is_none() {
            self.end_task(id, None);
            return;
        }
        let owner = actor.owner;
        let position = actor.position;
        if !self.step_delivery(id, owner, position) {
            return;
        }
        let resume = self
            .entities
            .get(&id)
            .and_then(|actor| actor.task.last_node)
            .filter(|node| {
                self.map
                    .resources()
                    .get(node.0 as usize)
                    .is_some_and(|node| node.remaining > 0)
            });
        match resume {
            Some(target) => self.replace_task_order(id, UnitOrder::Harvest { target }),
            None => self.end_task(id, None),
        }
    }
}
