//! The harvester state machine and income.
//!
//! Driven only by the archetype's `HarvesterSpec`: any unit with one gathers, whatever it is.
//! How long a gather takes and what it yields are the resource's: the
//! [`ResourceHarvest`](crate::catalog::ResourceHarvest) of the node's kind in the catalog's
//! placement rules (plan `2026-11-22-harvest-timing` D-2).
//!
//! # The cycle
//! A `Harvest` order alternates two states, chosen by whether the unit holds cargo:
//! - **Empty:** walk to the node, wait in its line until it is free, stay for its kind's
//!   `time_ticks`, then take up to its `amount` units from it. The node is drawn down at that
//!   moment.
//! - **Carrying:** first wait at the node, holding the load, for its kind's
//!   `return_delay_ticks` (`ReturnDelay`, counted down in `UnitTask::return_ticks`; plan
//!   `2026-11-22-harvest-timing` D-1), then walk to the nearest completed friendly depot,
//!   chosen when the wait ends. Income is credited on arrival, not when gathered, so cargo lost
//!   with its carrier was never income.
//!
//! # Rules
//! - Workers gathering, waiting or returning walk through other units, as mineral walking does
//!   in `StarCraft` II, so a crowded line never jams on itself.
//! - The wait is uninterruptible (5.0.14 `WaitToReturn`, B-160): a non-queued order given
//!   during it is taken, but the rest of the wait is carried into the unit's action lock
//!   (`Entity::cast_lock_ticks`), so the new order starts when the wait would have ended; the
//!   cargo stays on the unit (plan `2026-11-23-harvest-occupancy` D-5). A queued order waits
//!   behind the `Harvest` order. `ReturnCargo` given outside the wait does not wait.
//!
//! # The line (plan `2026-11-23-harvest-occupancy` D-1 to D-3, B-61)
//! - **One gatherer per node.** A node is occupied by the alive unit whose front order is
//!   `Harvest` on it and whose `gather_ticks` is above 0, of any player. Nothing records the
//!   gatherer, so every way a gather ends (the take, a new order, death, loading) frees the node.
//! - **The hand-off** (plan `2026-11-23-harvest-handoff` D-1, B-162). A take that yields cargo
//!   records the node's release tick, the take's tick plus its kind's `handoff_ticks`, in
//!   [`ResourceLines`](super::ResourceLines), and the node counts as occupied until then, whoever
//!   took. So the next worker in line is granted it `handoff_ticks` after the take; with 0 it
//!   gathers from the tick of the take. A worker reaching a free node starts at once.
//! - **Claims.** An empty worker whose body touches its node does not start gathering in its own
//!   step: it halts, counts `task.wait_ticks` up and records a claim for the tick.
//!   [`SimWorld::resolve_resource_claims`], after every task step, grants each free node to its
//!   first claimant: most ticks waited, then the lowest `EntityId`. So arrival order decides,
//!   never iteration order, and `wait_ticks` is hashed state every peer checks.
//! - **Moving on.** A claimant not granted, of a kind its harvester acquires, moves to the
//!   nearest free, unclaimed node of the same kind within its acquire radius that has at least
//!   two fewer workers assigned than its own; it then keeps that node. Gas workers wait.
//! - An exhausted node sends its worker to the nearest live node of the same kind within its
//!   harvester's `acquire_radius`, if the harvester acquires that kind (`AcquireRadius` and
//!   `ResourceAcquire`, plan `2026-11-23-harvest-occupancy` D-4). With none, or a kind it does
//!   not acquire, the order ends with an event.
//! - What a distant node holds is private until the player sees it. An order to an exhausted
//!   node out of sight is taken, and the worker walks there to find out, so neither admission
//!   nor an event reveals it early.
//! - A worker gathers when its body touches the field (or the extractor) and delivers when it
//!   touches the depot (plan `2026-11-20-resource-footprints` D-7): contact, not a reach.
//! - A mined-out field leaves the map (its kind's `remove_when_empty`, D-8) and the mesh is
//!   rebuilt; a geyser stays. The map keeps every node in its list so identifiers hold.
//! - Gas yields only through a completed extractor owned by the harvester's player, standing
//!   over the node. Workers gather from the extractor's edge.
//!
//! # Inside the extractor (plan `2026-11-24-hidden-harvesters` D-2 to D-7, B-161)
//! - A worker granted a node whose gather site is a structure that hides its harvesters
//!   (`ArchetypeCapabilities::hides_harvesters`, 5.0.14 `Flags HideHarvesters`) is inside it for
//!   its gather: [`UnitTask::housed_in`](crate::entity::UnitTask::housed_in) names the
//!   structure from the grant to the tick before the take. It is then in the engine's `Hidden`
//!   state ([`Entity::is_hidden`](crate::entity::Entity::is_hidden)).
//! - It leaves at the take, on any new leg (`fresh_leg`), and when the structure dies
//!   ([`SimWorld::release_housed_harvesters`], in the reap phase). Its position never changes
//!   while it is inside, so it comes out where it went in (Q0107 defaults). A worker waiting
//!   in the line is outside, and a mineral field has no structure, so its gatherers are never
//!   inside.
//! - A unit holding cargo delivers before it gathers again, so cargo of the other kind is
//!   never thrown away.
//! - With no depot left, the order ends and the cargo stays on the unit.
//! - A node whose kind has no harvest in the catalog (rules not loaded,
//!   [`PlacementRules::NONE`](crate::catalog::PlacementRules::NONE)) cannot be gathered: the
//!   order ends with [`TaskEventKind::NotCapable`].

use super::{Approach, Footprint, GameCommandError, GameCommandReceipt, TaskEventKind};
use crate::entity::Cargo;
use crate::movement::math::squared_distance;
use crate::production::ProductionInvariant;
use crate::SimWorld;
use sim_map::PlacedFootprint;
use sim_types::{
    AnimState, EntityId, HarvesterSpec, PlayerId, ResourceKind, ResourceNodeId, SimVec2, UnitOrder,
    VisibilityState,
};

/// A worker touching its node without gathering this tick: it waits its turn (D-2).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Claim {
    /// The node it claims.
    node: ResourceNodeId,
    /// The waiting worker.
    worker: EntityId,
    /// How long a gather takes at the node, ticks: its kind's `harvest_time_ticks`.
    time_ticks: u16,
    /// The structure the worker goes inside if it is granted the node: the gather site's
    /// structure when it hides its harvesters (plan `2026-11-24-hidden-harvesters` D-2).
    housing: Option<EntityId>,
}

/// What a tick with claims knows of one node (D-1): derived from the tick's task units, never
/// stored across ticks.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NodeTally {
    /// A unit is gathering at it (`gather_ticks > 0`), has just been granted it, or it is in its
    /// hand-off (before its release tick).
    occupied: bool,
    /// A claim on it has been resolved this tick, or a mover has been sent to it.
    claimed: bool,
    /// Alive units whose front order is `Harvest` on it, wherever they are.
    assigned: u32,
}

impl SimWorld {
    /// The cells and shape a player's worker gathers from at `node`, if it can gather there
    /// now: a standing field's own footprint, or the completed extractor over a geyser (plan
    /// `2026-11-20-resource-footprints` D-7).
    fn gather_site(
        &self,
        player: PlayerId,
        node: ResourceNodeId,
    ) -> Option<(Footprint, PlacedFootprint)> {
        let resource = self.map.resources().get(node.0 as usize)?;
        match resource.kind {
            ResourceKind::Mineral => {
                let placed = resource.placed?.footprint;
                Some((Footprint::of_placed(&placed), placed))
            }
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
                        .map(|placed| (Footprint::of_structure(placed), placed.footprint()))
                })?
            }
        }
    }

    /// The structure a gatherer of `node` goes inside while it gathers: the live structure over
    /// the node when its archetype hides its harvesters (plan `2026-11-24-hidden-harvesters`
    /// D-1, D-2); `None` for a mineral field or an extractor that does not hide them.
    fn hiding_structure(&self, node: ResourceNodeId) -> Option<EntityId> {
        let resource = self.map.resources().get(node.0 as usize)?;
        let structure = self
            .map
            .cell(resource.cell.0, resource.cell.1)
            .ok()?
            .occupying_structure?;
        let site = self.entities.get(&structure)?;
        let hides = site.is_alive()
            && self
                .catalog
                .get(site.unit_type_id)
                .is_ok_and(|arch| arch.capabilities.hides_harvesters);
        hides.then_some(structure)
    }

    /// Whether `player` sees `cell` at this moment.
    fn sees_cell(&self, player: PlayerId, cell: (u32, u32)) -> bool {
        self.vision_maps
            .get(player)
            .is_some_and(|vision| vision.visibility_at(cell.0, cell.1) == VisibilityState::Visible)
    }

    /// Admits `Harvest` for every capable selected unit.
    ///
    /// An exhausted node is refused only if `player` can see that it is.
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
        if node.remaining == 0 && self.sees_cell(player, node.cell) {
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
            actor.task.housed_in = None;
            if actor.task.return_ticks > 0 {
                actor.task.return_ticks -= 1;
                return;
            }
            self.step_delivery(id, owner, position);
            return;
        }
        let Some(node) = self.map.resources().get(target.0 as usize) else {
            self.end_task(id, Some(TaskEventKind::UnknownResourceNode));
            return;
        };
        if node.remaining == 0 {
            let (kind, cell) = (node.kind, node.cell);
            if !self.sees_cell(owner, cell) {
                match self.approach_footprint(id, Footprint::cell(cell), Self::approach_cell) {
                    Approach::Walking => return,
                    Approach::Unreachable => {
                        self.end_task(id, Some(TaskEventKind::Unreachable));
                        return;
                    }
                    Approach::Arrived => {}
                }
            }
            match self.next_node(owner, &spec, kind, position) {
                Some(next) => self.replace_task_order(id, UnitOrder::Harvest { target: next }),
                None => self.end_task(id, Some(TaskEventKind::ResourceExhausted)),
            }
            return;
        }
        let Some((site, shape)) = self.gather_site(owner, target) else {
            self.end_task(id, Some(TaskEventKind::ExtractorLost));
            return;
        };
        self.step_gather(id, target, (site, shape));
    }

    /// One tick of gathering at `target`: walk to its site, gather for its kind's time, then
    /// take its kind's amount and start its kind's return delay.
    fn step_gather(
        &mut self,
        id: EntityId,
        target: ResourceNodeId,
        (site, shape): (Footprint, PlacedFootprint),
    ) {
        let harvest = self
            .map
            .resources()
            .get(target.0 as usize)
            .and_then(|node| self.catalog.placement_rules().harvest(node.kind))
            .copied();
        let Some(harvest) = harvest else {
            self.end_task(id, Some(TaskEventKind::NotCapable));
            return;
        };
        let gathering = self
            .entities
            .get(&id)
            .is_some_and(|actor| actor.task.gather_ticks > 0);
        if !gathering {
            match self.approach_shape(id, site, &shape) {
                Approach::Walking => {}
                Approach::Unreachable => self.end_task(id, Some(TaskEventKind::Unreachable)),
                Approach::Arrived => {
                    // It waits its turn: the line is resolved after every task step (D-2).
                    if let Some(actor) = self.entities.get_mut(&id) {
                        actor.task.wait_ticks = actor.task.wait_ticks.saturating_add(1);
                        actor.current_anim = AnimState::Idle;
                    }
                    let housing = self.hiding_structure(target);
                    self.economy.claims.push(Claim {
                        node: target,
                        worker: id,
                        time_ticks: harvest.time_ticks,
                        housing,
                    });
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
        // The take: the worker comes out where it went in (plan `2026-11-24-hidden-harvesters`
        // D-2, D-7).
        actor.task.housed_in = None;
        let Some(node) = self.map.resources_mut().get_mut(target.0 as usize) else {
            return;
        };
        let amount = harvest.amount.min(node.remaining);
        node.remaining -= amount;
        let kind = node.kind;
        if amount > 0 {
            // The line's next worker starts after the hand-off (plan `2026-11-23-harvest-handoff`
            // D-1); resolved after every step of the tick, so no step reads this write.
            let release = self
                .current_tick
                .saturating_add(u32::from(harvest.handoff_ticks));
            self.resource_lines.set_release_tick(target, release);
        }
        if node.remaining == 0 && amount > 0 {
            self.remove_if_empty(target, kind);
        }
        if let Some(actor) = self.entities.get_mut(&id) {
            actor.current_anim = AnimState::Idle;
            actor.task.last_node = Some(target);
            actor.task.depot = None;
            if amount > 0 {
                actor.cargo = Some(Cargo { kind, amount });
                actor.task.return_ticks = harvest.return_delay_ticks;
            }
        }
    }

    /// Removes a node of `kind` that has just emptied, if its kind leaves the map when empty
    /// (plan `2026-11-20-resource-footprints` D-8, `RemoveWhenEmpty`): its cells become open
    /// ground again and the mesh and clearance are rebuilt, as for a structure change. The node
    /// stays in the map's list with nothing left, so identifiers hold.
    fn remove_if_empty(&mut self, node: ResourceNodeId, kind: ResourceKind) {
        let removes = self
            .catalog
            .placement_rules()
            .resource_footprints()
            .get(kind)
            .is_some_and(|footprint| footprint.remove_when_empty);
        if removes && self.map.remove_resource_footprint(node).is_ok() {
            self.pathfinder.recompute_clearance(&self.map);
        }
    }

    /// The nearest node of `kind` within the harvester's acquire radius of `from` that `player`
    /// can gather from now; none if the harvester does not acquire `kind` (plan
    /// `2026-11-23-harvest-occupancy` D-4).
    fn next_node(
        &self,
        player: PlayerId,
        spec: &HarvesterSpec,
        kind: ResourceKind,
        from: SimVec2,
    ) -> Option<ResourceNodeId> {
        if !spec.acquires.contains(kind) {
            return None;
        }
        let reach = i128::from(spec.acquire_radius.raw());
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

    /// Resolves the tick's claims on resources, after every task step (plan
    /// `2026-11-23-harvest-occupancy` D-2, D-3): each free node is granted to its first claimant
    /// in the line's key, and the other claimants of a kind their harvester acquires may move on.
    ///
    /// Runs only on a tick with claims. Reads only state the tick's steps have left and decides
    /// in an explicit key of hashed state, so no iteration order decides who gathers or moves.
    pub(crate) fn resolve_resource_claims(&mut self) {
        if self.economy.claims.is_empty() {
            return;
        }
        let mut claims = std::mem::take(&mut self.economy.claims);
        let mut tally = std::mem::take(&mut self.economy.tally);
        let mut waiting = std::mem::take(&mut self.economy.waiting);
        self.tally_nodes(&mut tally);
        claims.sort_unstable_by_key(|claim| (claim.node, self.line_key(claim.worker)));
        waiting.clear();
        self.grant_claims(&claims, &mut tally, &mut waiting);
        waiting.sort_unstable_by_key(|claim| self.line_key(claim.worker));
        for claim in &waiting {
            self.move_on(*claim, &mut tally);
        }
        claims.clear();
        self.economy.claims = claims;
        self.economy.tally = tally;
        self.economy.waiting = waiting;
    }

    /// The line's key of `worker`: most ticks waited first, then the lowest `EntityId` (D-2).
    fn line_key(&self, worker: EntityId) -> (std::cmp::Reverse<u16>, EntityId) {
        let waited = self
            .entities
            .get(&worker)
            .map_or(0, |actor| actor.task.wait_ticks);
        (std::cmp::Reverse(waited), worker)
    }

    /// Fills `tally`, one entry per node of the map, with each node's occupancy and assigned
    /// count (D-1), from the tick's task units and each node's hand-off (plan
    /// `2026-11-23-harvest-handoff` D-1: a node is occupied until its release tick).
    fn tally_nodes(&self, tally: &mut Vec<NodeTally>) {
        tally.clear();
        tally.resize(self.map.resources().len(), NodeTally::default());
        for (index, node) in (0_u32..).zip(tally.iter_mut()) {
            node.occupied = self
                .resource_lines
                .in_handoff(ResourceNodeId(index), self.current_tick);
        }
        for id in &self.economy.ids {
            let Some(actor) = self.entities.get(id).filter(|actor| actor.is_alive()) else {
                continue;
            };
            let Some(UnitOrder::Harvest { target }) = actor.order_queue.front() else {
                continue;
            };
            if let Some(node) = tally.get_mut(target.0 as usize) {
                node.assigned = node.assigned.saturating_add(1);
                node.occupied |= actor.task.gather_ticks > 0;
            }
        }
    }

    /// Grants each unoccupied node to its first claimant of `claims` (sorted by node, then the
    /// line's key); every other claimant goes to `waiting`. Every claimed node is marked claimed.
    /// A granted worker goes inside its claim's housing structure, if any (plan
    /// `2026-11-24-hidden-harvesters` D-2).
    fn grant_claims(
        &mut self,
        claims: &[Claim],
        tally: &mut [NodeTally],
        waiting: &mut Vec<Claim>,
    ) {
        for &claim in claims {
            let Some(node) = tally.get_mut(claim.node.0 as usize) else {
                continue;
            };
            let first = !node.claimed;
            node.claimed = true;
            if !first || node.occupied {
                waiting.push(claim);
                continue;
            }
            node.occupied = true;
            if let Some(actor) = self.entities.get_mut(&claim.worker) {
                actor.task.gather_ticks = claim.time_ticks;
                actor.task.wait_ticks = 0;
                actor.task.housed_in = claim.housing;
                actor.current_anim = AnimState::Cast;
            }
        }
    }

    /// Lets a waiting claimant move on to a free node of the same kind with at least two fewer
    /// workers assigned (D-3), if its harvester acquires that kind; the move is booked in
    /// `tally` so the claimants decided after it see it.
    ///
    /// **Why two fewer.** Moving one worker from a node with `a` workers assigned to one with
    /// `b` leaves `a - 1` and `b + 1`. The counts become more even only if `b + 1 < a`, that is
    /// `b + 2 ≤ a`. With `b + 1 = a` the move only swaps which node is short, and the worker
    /// left behind then finds the same reason to move back: a line that moves on at one fewer
    /// oscillates (plan F-2, measured). So two is the smallest difference that cannot make a move
    /// oscillate, a derivation from the move's own effect on the two counts, not a tuned value.
    fn move_on(&mut self, claim: Claim, tally: &mut [NodeTally]) {
        let Some(spec) = self.harvester_spec(claim.worker) else {
            return;
        };
        let Some(actor) = self.entities.get(&claim.worker) else {
            return;
        };
        let (owner, from) = (actor.owner, actor.position);
        let Some(kind) = self
            .map
            .resources()
            .get(claim.node.0 as usize)
            .map(|node| node.kind)
        else {
            return;
        };
        if !spec.acquires.contains(kind) {
            return;
        }
        let own = tally
            .get(claim.node.0 as usize)
            .map_or(0, |node| node.assigned);
        let reach = i128::from(spec.acquire_radius.raw());
        let best = self
            .map
            .resources()
            .iter()
            .zip(tally.iter())
            .enumerate()
            .filter(|(_, (node, count))| {
                node.kind == kind
                    && node.remaining > 0
                    && !count.occupied
                    && !count.claimed
                    && count.assigned.saturating_add(2) <= own
            })
            .filter(|(_, (node, _))| crate::movement::math::within_distance(from, node.pos, reach))
            .filter_map(|(index, (node, _))| {
                let id = ResourceNodeId(u32::try_from(index).ok()?);
                self.gather_site(owner, id)?;
                Some((squared_distance(from, node.pos), id))
            })
            .min()
            .map(|(_, id)| id);
        let Some(target) = best else {
            return;
        };
        if let Some(node) = tally.get_mut(target.0 as usize) {
            node.assigned = node.assigned.saturating_add(1);
            node.claimed = true;
        }
        if let Some(node) = tally.get_mut(claim.node.0 as usize) {
            node.assigned = node.assigned.saturating_sub(1);
        }
        self.replace_task_order(claim.worker, UnitOrder::Harvest { target });
        if let Some(actor) = self.entities.get_mut(&claim.worker) {
            // Still harvesting: it walks through other units as before (R-5).
            actor.is_mineral_walking = true;
        }
    }

    /// Reap phase: brings out every gatherer whose housing structure is dead or gone, in
    /// entity-ID order, before the dead are removed (plan `2026-11-24-hidden-harvesters` D-5,
    /// Q0107's default (1a)). The worker survives where it went in, seen and damageable at the end
    /// of the tick, its gather lost; its next task step finds no gather site and ends the order
    /// with [`TaskEventKind::ExtractorLost`].
    // PERFORMANCE RATIONALE:
    // Why: The released workers cannot be changed while the entities are iterated, and a fresh
    //      list each tick would allocate.
    // What: One pass over the entities fills a retained list, then each listed worker is
    //      released; a tick without a housed worker whose structure died writes nothing.
    // Trade-off: An O(N) scan of the entities each tick, and the list's high-water capacity.
    pub(crate) fn release_housed_harvesters(&mut self) {
        let mut released = std::mem::take(&mut self.economy.released);
        released.clear();
        released.extend(self.entities.values().filter_map(|actor| {
            let structure = actor.housing()?;
            let standing = self
                .entities
                .get(&structure)
                .is_some_and(crate::entity::Entity::is_alive);
            (!standing).then_some(actor.id)
        }));
        for id in &released {
            if let Some(actor) = self.entities.get_mut(id) {
                actor.task.housed_in = None;
            }
        }
        self.economy.released = released;
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
        let Some((site, shape)) = self
            .map
            .structures()
            .get(&depot)
            .map(|placed| (Footprint::of_structure(placed), placed.footprint()))
        else {
            return false;
        };
        match self.approach_shape(id, site, &shape) {
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
        let Some(resources) = self.player_resources.get_mut(owner) else {
            self.report_production_failure(ProductionInvariant::InvalidPlayer { player: owner });
            return false;
        };
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
