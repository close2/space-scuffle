//! Unit tasks: the orders a unit carries out over many ticks at a place.
//!
//! `Harvest`, `ReturnCargo`, `Build`, `Construct`, `Load` and `UnloadAll` all have the same
//! shape: walk somewhere, do something there, perhaps repeat. This module owns that shape.
//!
//! # How a task runs
//! Tick phase 2b, [`SimWorld::update_unit_tasks`], looks at each unit whose front order is a
//! task and decides what it does this tick. To move, the task stores a goal in
//! [`UnitTask::goal`], and the ordinary movement phase walks there exactly as it would walk a
//! `Move` order. The difference is what arrival means: movement ends the *leg*, not the order,
//! and the task decides what comes next. So a task never pathfinds or steers for itself.
//!
//! # Command admission
//! Every command here is checked completely before anything changes, and answers with one
//! [`GameCommandOutcome`] per command. Mixed selections are normal: units that cannot take
//! the order are left alone, provided one can.
//!
//! # Invariants
//! - A unit that cannot finish a task ends it with a [`TaskEvent`] saying why; no task can
//!   spin forever, since legs that end out of reach are bounded by [`MAX_TASK_LEGS`].
//! - Outcomes and events are derived output, excluded from checksums and never read back.
//! - Task iteration follows entity ID order, so simultaneous workers resolve identically on
//!   every peer.

mod harvest;

use crate::entity::UnitTask;
use crate::movement::math::squared_distance;
use crate::production::construction::BuildRejection;
use crate::production::{ProductionAdmissionError, ProductionInvariant};
use crate::scenario::ring_perimeter;
use crate::transport::TransportAdmissionError;
use crate::{Entity, SimWorld, UnitArchetype};
use sim_map::{CellFlags, PlacedStructure};
use sim_types::{
    AnimState, EntityId, PlayerCommand, PlayerId, PlayerSlot, ResourceNodeId, SimScalar, SimVec2,
    Tick, UnitOrder,
};
use thiserror::Error;

/// Times a task may set off for a place before it concludes the place cannot be reached.
pub(crate) const MAX_TASK_LEGS: u8 = 3;

/// How far beyond its own radius a unit can work on something, in world units.
const TASK_REACH: SimScalar = SimScalar::from_raw(3 << 30);

/// A command that cannot be carried out, or corrupt state found while admitting it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GameCommandError {
    /// The match has ended; the world no longer takes commands.
    #[error("the match is over")]
    MatchOver,
    /// This player has been defeated and no longer commands anything.
    #[error("{0:?} has been defeated")]
    PlayerDefeated(PlayerId),
    /// The order is not one this command path handles.
    #[error("order is not supported here")]
    UnsupportedOrder,
    /// Player slot is outside the world's supported slots.
    #[error("unknown player {0:?}")]
    InvalidPlayer(PlayerId),
    /// No entity was selected.
    #[error("select at least one unit")]
    EmptySelection,
    /// A selected entity does not exist.
    #[error("selected entity {0:?} does not exist")]
    MissingRecipient(EntityId),
    /// A selected entity is not owned by the issuing player.
    #[error("selected entity {0:?} is not owned by this player")]
    ForeignRecipient(EntityId),
    /// A selected entity is destroyed.
    #[error("selected entity {0:?} is destroyed")]
    DeadRecipient(EntityId),
    /// A selected entity is inside a carrier.
    #[error("selected entity {0:?} is carried")]
    LoadedRecipient(EntityId),
    /// A structure cannot be started; see the reason.
    #[error(transparent)]
    Build(#[from] BuildRejection),
    /// Nothing selected can raise this structure.
    #[error("no selected unit can build this")]
    NoCapableBuilder,
    /// The entity is not an unfinished foundation of this player.
    #[error("{0:?} is not one of your foundations")]
    NotAFoundation(EntityId),
    /// Nothing selected can gather.
    #[error("no selected unit can harvest")]
    NoCapableHarvester,
    /// No resource node has this identifier.
    #[error("unknown resource node {0:?}")]
    UnknownResourceNode(ResourceNodeId),
    /// The node has nothing left.
    #[error("resource node {0:?} is exhausted")]
    ResourceExhausted(ResourceNodeId),
    /// A gas node yields only through this player's completed extractor over it.
    #[error("resource node {0:?} needs your completed extractor")]
    ExtractorRequired(ResourceNodeId),
    /// Nothing selected is carrying cargo.
    #[error("no selected unit carries cargo")]
    NoCargo,
    /// Cancelling a queued job addresses exactly one structure.
    #[error("select exactly one structure to cancel a queued job")]
    SingleRecipientRequired,
    /// The structure's queue has no job at this position.
    #[error("{producer:?} has no queued job {job_index}")]
    NoSuchJob {
        /// Selected structure.
        producer: EntityId,
        /// Requested queue position.
        job_index: u32,
    },
    /// The entity is no carrier, or cannot carry what was asked.
    #[error("{0:?} cannot carry this")]
    NotACarrier(EntityId),
    /// The passengers would not fit.
    #[error("{carrier:?} has {free} free slots, {needed} are needed")]
    CarrierFull {
        /// Requested carrier.
        carrier: EntityId,
        /// Slots still free.
        free: u32,
        /// Slots the selection needs.
        needed: u32,
    },
    /// The carrier holds nobody to unload.
    #[error("no selected carrier holds passengers")]
    NothingToUnload,
    /// The transport lifecycle refused the operation.
    #[error(transparent)]
    Transport(#[from] TransportAdmissionError),
    /// A production refusal surfaced through this path.
    #[error(transparent)]
    Production(#[from] ProductionAdmissionError),
    /// Authoritative state is malformed; consumers must stop and report the diagnostic.
    #[error(transparent)]
    Invariant(#[from] ProductionInvariant),
}

/// What an accepted command did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameCommandReceipt {
    /// This many units took the order. Nothing has been paid or done yet.
    Accepted {
        /// Units that took the order.
        recipients: usize,
    },
    /// A cancellation returned these resources.
    Refunded {
        /// Minerals returned.
        minerals: u32,
        /// Gas returned.
        gas: u32,
    },
}

/// Result of one command, correlated to its position in the tick bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameCommandOutcome {
    /// Tick the command was processed in.
    pub tick: Tick,
    /// Index in the exact `TickBundle.commands` processed in that tick.
    pub command_index: usize,
    /// Issuing player.
    pub player: PlayerId,
    /// Receipt or reason. An ordinary rejection does not stop the simulation.
    pub result: Result<GameCommandReceipt, GameCommandError>,
}

/// How a unit task ended, or a milestone in one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEventKind {
    /// The unit cannot do what its order asks; only direct order injection can cause this.
    NotCapable,
    /// The unit could not get to where the task is.
    Unreachable,
    /// This node and every usable node of its kind nearby have run out.
    ResourceExhausted,
    /// The order's node does not exist.
    UnknownResourceNode,
    /// The extractor over a gas node was lost.
    ExtractorLost,
    /// No completed depot remains; cargo stays on the unit.
    NoDepot,
    /// The structure could no longer be started when its builder arrived.
    BuildAbandoned(BuildRejection),
    /// Bodies stayed on the site until the builder's patience ran out.
    SiteObstructed,
    /// The foundation is gone.
    FoundationLost,
    /// The named foundation finished.
    StructureCompleted {
        /// The building that just became usable.
        foundation: EntityId,
    },
    /// The carrier is gone, or stopped being able to take this unit.
    CarrierUnavailable,
    /// No legal ground around the carrier remained for its passengers.
    NoUnloadSpace,
}

/// One unit's task event during a tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEvent {
    /// Executed tick that produced this event.
    pub tick: Tick,
    /// Unit concerned. It may be gone by the time a consumer reads this event.
    pub entity: EntityId,
    /// Player the unit belonged to when the event happened, so a consumer can filter by viewer.
    pub owner: PlayerId,
    /// What happened.
    pub kind: TaskEventKind,
}

/// Retained buffers and derived output of the task phase; excluded from checksums.
#[derive(Default)]
pub(crate) struct EconomyWorkspace {
    pub outcomes: Vec<GameCommandOutcome>,
    pub events: Vec<TaskEvent>,
    pub passengers: Vec<EntityId>,
    ids: Vec<EntityId>,
}

/// A rectangle of grid cells, a building's or a resource node's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Footprint {
    /// Minimum X cell.
    pub left: u32,
    /// Minimum Y cell.
    pub bottom: u32,
    /// Width in cells.
    pub width: u32,
    /// Height in cells.
    pub height: u32,
}

impl Footprint {
    pub(crate) fn of_structure(placed: &PlacedStructure) -> Self {
        Self {
            left: placed.top_left.0,
            bottom: placed.top_left.1,
            width: u32::from(placed.width),
            height: u32::from(placed.height),
        }
    }

    pub(crate) const fn cell(cell: (u32, u32)) -> Self {
        Self {
            left: cell.0,
            bottom: cell.1,
            width: 1,
            height: 1,
        }
    }

    pub(crate) const fn contains_cell(self, cell: (u32, u32)) -> bool {
        cell.0 >= self.left
            && cell.0 - self.left < self.width
            && cell.1 >= self.bottom
            && cell.1 - self.bottom < self.height
    }

    pub(crate) fn centre(self) -> SimVec2 {
        let centre = |start: u32, extent: u32| {
            SimScalar::from_num(start) + SimScalar::from_num(extent) * SimScalar::HALF
        };
        SimVec2::new(
            centre(self.left, self.width),
            centre(self.bottom, self.height),
        )
    }

    /// Distance from `point` to the rectangle in world units; zero inside it.
    pub(crate) fn gap(self, point: SimVec2) -> Option<SimScalar> {
        let axis = |value: SimScalar, start: u32, extent: u32| {
            let low = i128::from(start) << 32;
            let high = (i128::from(start) + i128::from(extent)) << 32;
            let raw = i128::from(value.raw());
            (low - raw).max(raw - high).max(0)
        };
        let dx = i64::try_from(axis(point.x, self.left, self.width)).ok()?;
        let dy = i64::try_from(axis(point.y, self.bottom, self.height)).ok()?;
        crate::movement::checked_distance(
            SimVec2::ZERO,
            SimVec2::new(SimScalar::from_raw(dx), SimScalar::from_raw(dy)),
        )
    }
}

/// Where a walk to some footprint stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Approach {
    /// The unit can work on the footprint from where it stands.
    Arrived,
    /// A leg is under way.
    Walking,
    /// Legs keep ending out of reach, or the footprint has no walkable edge.
    Unreachable,
}

impl SimWorld {
    /// Results of this tick's economy, construction, transport and refused commands.
    /// Derived output: cleared at the start of every tick and never read by the simulation.
    #[must_use]
    pub fn game_command_outcomes(&self) -> &[GameCommandOutcome] {
        &self.economy.outcomes
    }

    /// Drains command results without discarding the buffer's capacity.
    pub fn drain_game_command_outcomes(&mut self) -> std::vec::Drain<'_, GameCommandOutcome> {
        self.economy.outcomes.drain(..)
    }

    /// Unit task events of the latest tick; derived output, cleared at the start of every tick.
    #[must_use]
    pub fn task_events(&self) -> &[TaskEvent] {
        &self.economy.events
    }

    /// Drains task events without discarding the buffer's capacity.
    ///
    /// Derived output, like [`Self::drain_game_command_outcomes`]: no checksum sees it.
    pub fn drain_task_events(&mut self) -> std::vec::Drain<'_, TaskEvent> {
        self.economy.events.drain(..)
    }

    pub(crate) fn clear_economy_outcomes(&mut self) {
        self.economy.outcomes.clear();
        self.economy.events.clear();
    }

    /// Admits one economy, construction, cancellation or transport command.
    pub(crate) fn admit_game_command(
        &mut self,
        cmd: &PlayerCommand,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        let (player, recipients) = (cmd.player_id, cmd.entities.as_slice());
        match cmd.order {
            UnitOrder::Build { .. } => self.admit_build(player, recipients, &cmd.order, cmd.queue),
            UnitOrder::Construct { foundation } => {
                self.admit_construct(player, recipients, foundation, cmd.queue)
            }
            UnitOrder::CancelConstruction => self.cancel_foundations(player, recipients),
            UnitOrder::Harvest { target } => {
                self.admit_harvest(player, recipients, target, cmd.queue)
            }
            UnitOrder::ReturnCargo => self.admit_return_cargo(player, recipients, cmd.queue),
            UnitOrder::CancelProduction { job_index } => {
                self.admit_cancel_production(player, recipients, job_index)
            }
            UnitOrder::Load { carrier } => self.admit_load(player, recipients, carrier, cmd.queue),
            UnitOrder::UnloadAll => self.admit_unload(player, recipients, cmd.queue),
            _ => Err(GameCommandError::UnsupportedOrder),
        }
    }

    fn admit_cancel_production(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        job_index: u32,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.validate_selection(player, recipients)?;
        let &[producer] = recipients else {
            return Err(GameCommandError::SingleRecipientRequired);
        };
        let job = self.cancel_production_job(producer, job_index)?.ok_or(
            GameCommandError::NoSuchJob {
                producer,
                job_index,
            },
        )?;
        Ok(GameCommandReceipt::Refunded {
            minerals: job.paid_minerals,
            gas: job.paid_gas,
        })
    }

    /// Refuses a selection unless every member exists, is alive, is owned and is in the world.
    ///
    /// Returns the player's slot, proven here, for callers that then touch per-player state.
    pub(crate) fn validate_selection(
        &self,
        player: PlayerId,
        recipients: &[EntityId],
    ) -> Result<PlayerSlot, GameCommandError> {
        let Some(slot) = player.slot() else {
            return Err(GameCommandError::InvalidPlayer(player));
        };
        if recipients.is_empty() {
            return Err(GameCommandError::EmptySelection);
        }
        for &id in recipients {
            let entity = self
                .entities
                .get(&id)
                .ok_or(GameCommandError::MissingRecipient(id))?;
            if entity.owner != player {
                return Err(GameCommandError::ForeignRecipient(id));
            }
            if !entity.is_alive() {
                return Err(GameCommandError::DeadRecipient(id));
            }
            if entity.is_loaded_in_transport() {
                return Err(GameCommandError::LoadedRecipient(id));
            }
        }
        Ok(slot)
    }

    /// The selected unit nearest `site` whose archetype `capable` accepts; lowest ID on a tie.
    pub(crate) fn nearest_recipient(
        &self,
        recipients: &[EntityId],
        site: Footprint,
        capable: impl Fn(&UnitArchetype) -> bool,
    ) -> Option<EntityId> {
        recipients
            .iter()
            .filter_map(|&id| {
                let entity = self.entities.get(&id)?;
                let arch = self.catalog.get(entity.unit_type_id).ok()?;
                capable(arch).then_some((site.gap(entity.position)?, id))
            })
            .min()
            .map(|(_, id)| id)
    }

    /// Gives `id` an admitted task order.
    pub(crate) fn commit_task_order(
        &mut self,
        id: EntityId,
        order: UnitOrder,
        queue: bool,
    ) -> Result<(), GameCommandError> {
        let entity = self
            .entities
            .get_mut(&id)
            .ok_or(GameCommandError::MissingRecipient(id))?;
        match entity.issue_order(order, queue) {
            Ok(true) => Ok(()),
            Ok(false) => Err(GameCommandError::LoadedRecipient(id)),
            Err(_) => Err(ProductionInvariant::EntityState {
                entity: id,
                reason: "task order refused by entity admission",
            }
            .into()),
        }
    }

    /// Phase 2b: every unit whose front order is a task acts on it.
    ///
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    // PERFORMANCE RATIONALE:
    // Why: Tasks mutate the world, so they cannot run while iterating `entities`.
    // What: Collect the working units into one retained ID list per tick.
    // Trade-off: An O(N) scan per tick, with no allocation after the population high-water mark.
    pub fn update_unit_tasks(&mut self) {
        let mut ids = std::mem::take(&mut self.economy.ids);
        ids.clear();
        ids.extend(
            self.entities
                .values()
                .filter(|e| e.is_alive() && !e.is_loaded_in_transport() && e.has_task_order())
                .map(|e| e.id),
        );
        for &id in &ids {
            let order = self
                .entities
                .get(&id)
                .and_then(|actor| actor.order_queue.front().cloned());
            match order {
                Some(UnitOrder::Harvest { target }) => self.step_harvest(id, target),
                Some(UnitOrder::ReturnCargo) => self.step_return_cargo(id),
                Some(UnitOrder::Build {
                    structure_type,
                    cell_x,
                    cell_y,
                }) => self.step_build(id, structure_type, cell_x, cell_y),
                Some(UnitOrder::Construct { foundation }) => self.step_construct(id, foundation),
                Some(UnitOrder::Load { carrier }) => self.step_load(id, carrier),
                Some(UnitOrder::UnloadAll) => self.step_unload(id),
                _ => {}
            }
        }
        self.economy.ids = ids;
    }

    /// The radius of `actor`, in world units.
    fn body_radius(&self, actor: &Entity) -> Option<SimScalar> {
        self.catalog
            .get(actor.unit_type_id)
            .ok()
            .map(|arch| arch.collision_radius)
    }

    /// Whether `actor` can work on `site` from where it stands.
    fn within_reach(&self, actor: &Entity, site: Footprint) -> bool {
        let Some(radius) = self.body_radius(actor) else {
            return false;
        };
        site.gap(actor.position)
            .is_some_and(|gap| gap <= radius + TASK_REACH)
    }

    /// Whether a ground unit can stand at the middle of this cell.
    fn cell_is_walkable(&self, x: u32, y: u32) -> bool {
        self.map.cell(x, y).is_ok_and(|cell| {
            cell.flags.contains(CellFlags::GROUND_PATHABLE)
                && !cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE)
        })
    }

    /// The walkable point nearest `from` at the edge of `site`, or the middle of a free cell.
    pub(crate) fn approach_point(&self, from: SimVec2, site: Footprint) -> Option<SimVec2> {
        if site.width == 1 && site.height == 1 && self.cell_is_walkable(site.left, site.bottom) {
            return Some(site.centre());
        }
        let bound = |value: u32| i32::try_from(value).ok();
        let left = bound(site.left)?;
        let bottom = bound(site.bottom)?;
        let right = bound(site.left + site.width)?;
        let top = bound(site.bottom + site.height)?;
        let mut best = None;
        for (x, y) in ring_perimeter(left - 1, right, bottom - 1, top) {
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                continue;
            };
            if !self.cell_is_walkable(x, y) {
                continue;
            }
            let point = Footprint::cell((x, y)).centre();
            let key = squared_distance(from, point);
            if best.is_none_or(|(nearest, _)| key < nearest) {
                best = Some((key, point));
            }
        }
        best.map(|(_, point)| point)
    }

    /// Stops `id` and forgets its leg: it is where it needs to be.
    pub(crate) fn halt_task(&mut self, id: EntityId) {
        if let Some(actor) = self.entities.get_mut(&id) {
            actor.task.goal = None;
            actor.task.legs = 0;
            actor.velocity = SimVec2::ZERO;
            actor.current_funnel_waypoints.clear();
        }
    }

    /// Sets `id` off toward `goal` on a fresh leg.
    pub(crate) fn set_task_goal(&mut self, id: EntityId, goal: SimVec2) {
        if let Some(actor) = self.entities.get_mut(&id) {
            actor.task.goal = Some(goal);
            actor.task.legs = actor.task.legs.saturating_add(1);
            actor.current_funnel_waypoints.clear();
            actor.movement.restart();
        }
    }

    /// Walks `id` to the edge of `site`, reporting how that stands this tick.
    pub(crate) fn approach_footprint(&mut self, id: EntityId, site: Footprint) -> Approach {
        let Some(actor) = self.entities.get(&id) else {
            return Approach::Unreachable;
        };
        if self.within_reach(actor, site) {
            self.halt_task(id);
            return Approach::Arrived;
        }
        if actor.task.goal.is_some() {
            return Approach::Walking;
        }
        if actor.task.legs >= MAX_TASK_LEGS {
            return Approach::Unreachable;
        }
        match self.approach_point(actor.position, site) {
            Some(goal) => {
                self.set_task_goal(id, goal);
                Approach::Walking
            }
            None => Approach::Unreachable,
        }
    }

    /// Ends the front task order of `id`, reporting `kind` if there is something to report.
    ///
    /// An unregistered `id` has no order to end and no owner to tell, so it reports nothing.
    pub(crate) fn end_task(&mut self, id: EntityId, kind: Option<TaskEventKind>) {
        let Some(actor) = self.entities.get_mut(&id) else {
            return;
        };
        actor.order_queue.pop_front();
        actor.abandon_task_leg();
        actor.velocity = SimVec2::ZERO;
        actor.current_funnel_waypoints.clear();
        actor.movement.restart();
        actor.current_anim = AnimState::Idle;
        let owner = actor.owner;
        if let Some(kind) = kind {
            self.economy.events.push(TaskEvent {
                tick: Tick(self.current_tick),
                entity: id,
                owner,
                kind,
            });
        }
    }

    /// Replaces the front task order of `id` with `order`, starting it from a clean leg.
    pub(crate) fn replace_task_order(&mut self, id: EntityId, order: UnitOrder) {
        if let Some(actor) = self.entities.get_mut(&id) {
            actor.order_queue.pop_front();
            actor.order_queue.push_front(order);
            actor.abandon_task_leg();
            actor.current_funnel_waypoints.clear();
            actor.movement.restart();
        }
    }
}

/// A task record with its leg cleared, keeping only what outlives orders.
pub(crate) const fn fresh_leg(task: UnitTask) -> UnitTask {
    UnitTask {
        goal: None,
        legs: 0,
        gather_ticks: 0,
        wait_ticks: 0,
        depot: None,
        last_node: task.last_node,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    const SITE: Footprint = Footprint {
        left: 4,
        bottom: 6,
        width: 3,
        height: 2,
    };

    #[test]
    fn gap_is_zero_on_and_inside_the_rectangle() {
        for inside in [point(4.0, 6.0), point(5.5, 7.0), point(7.0, 8.0)] {
            assert_eq!(SITE.gap(inside), Some(SimScalar::ZERO));
        }
    }

    #[test]
    fn gap_measures_to_the_nearest_edge_or_corner() {
        assert_eq!(SITE.gap(point(2.0, 7.0)), Some(SimScalar::from_num(2)));
        assert_eq!(SITE.gap(point(5.0, 9.5)), Some(SimScalar::from_num(1.5)));
        assert_eq!(SITE.gap(point(10.0, 12.0)), Some(SimScalar::from_num(5)));
    }

    #[test]
    fn cell_containment_excludes_the_far_edges() {
        assert!(SITE.contains_cell((4, 6)));
        assert!(SITE.contains_cell((6, 7)));
        assert!(!SITE.contains_cell((7, 7)));
        assert!(!SITE.contains_cell((6, 8)));
        assert!(!SITE.contains_cell((3, 6)));
    }

    #[test]
    fn centre_lies_in_the_middle_of_the_cells() {
        assert_eq!(SITE.centre(), point(5.5, 7.0));
        assert_eq!(Footprint::cell((2, 3)).centre(), point(2.5, 3.5));
    }
}
