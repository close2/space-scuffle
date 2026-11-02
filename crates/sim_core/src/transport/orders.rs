//! Player load and unload orders over the sole-carrier transaction.
//!
//! The lifecycle in the parent module knows nothing of range or capacity. These orders add
//! both, driven by the `transport_capacity` capability.
//!
//! # Rules
//! - `Load` goes to the passengers, who walk to the carrier and board within
//!   [`BOARDING_REACH`] of its edge. A carrier holding still is not required, only reached.
//! - Only ground units board. A passenger fills one slot per supply it costs, at least one.
//!   Capacity is checked at admission and again at the door.
//! - `UnloadAll` goes to the carrier, which stops and sets everyone down on free ground
//!   around itself, passengers in ID order. To drop elsewhere, queue it after a `Move`.
//! - A carrier that finds no ground for a passenger keeps trying for
//!   [`UNLOAD_PATIENCE_TICKS`], then gives up with the rest aboard.

use super::{TransportAdmissionError, TransportInvariant};
use crate::economy::{Footprint, GameCommandError, GameCommandReceipt, TaskEventKind};
use crate::movement::{checked_distance, MovementLayer};
use crate::scenario::ring_perimeter;
use crate::SimWorld;
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2, UnitOrder};

/// How far from a carrier's edge a passenger can board, in world units.
const BOARDING_REACH: SimScalar = SimScalar::from_raw(1 << 32);

/// Ticks a carrier keeps looking for ground to set passengers on.
const UNLOAD_PATIENCE_TICKS: u16 = 50;

/// Cell rings around the carrier searched for a place to set a passenger down.
const UNLOAD_SEARCH_RINGS: i32 = 4;

impl SimWorld {
    /// Passenger slots of `carrier`, or `None` if it is no carrier.
    fn carrier_capacity(&self, carrier: EntityId) -> Option<u32> {
        let entity = self.entities.get(&carrier)?;
        let arch = self.catalog.get(entity.unit_type_id).ok()?;
        (arch.capabilities.transport_capacity > 0).then_some(arch.capabilities.transport_capacity)
    }

    /// Slots `passenger` fills.
    fn passenger_slots(&self, passenger: EntityId) -> u32 {
        self.entities
            .get(&passenger)
            .and_then(|entity| self.catalog.get(entity.unit_type_id).ok())
            .map_or(1, |arch| arch.supply_cost.max(1))
    }

    /// Slots of `carrier` already taken.
    fn carrier_slots_used(&self, carrier: EntityId) -> u32 {
        self.entities
            .values()
            .filter(|entity| entity.transport_carrier() == Some(carrier))
            .map(|entity| self.passenger_slots(entity.id))
            .sum()
    }

    /// Whether this entity walks and may therefore board.
    fn can_board(&self, id: EntityId) -> bool {
        self.entities.get(&id).is_some_and(|entity| {
            !entity.is_immobile
                && self.catalog.get(entity.unit_type_id).is_ok_and(|arch| {
                    MovementLayer::from_mover(arch.mover_type) == MovementLayer::Ground
                })
        })
    }

    /// Admits `Load`: every boarding unit of the selection walks to `carrier`.
    pub(crate) fn admit_load(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        carrier: EntityId,
        queue: bool,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.validate_selection(player, recipients)?;
        let target = self
            .entities
            .get(&carrier)
            .ok_or(TransportAdmissionError::MissingEntity(carrier))?;
        if target.owner != player {
            return Err(GameCommandError::ForeignRecipient(carrier));
        }
        if !target.is_alive() {
            return Err(TransportAdmissionError::DeadEntity(carrier).into());
        }
        if target.is_loaded_in_transport() {
            return Err(TransportAdmissionError::AlreadyLoaded(carrier).into());
        }
        if !target.is_completed {
            return Err(TransportAdmissionError::IncompleteEntity(carrier).into());
        }
        let capacity = self
            .carrier_capacity(carrier)
            .ok_or(GameCommandError::NotACarrier(carrier))?;
        let mut passengers: Vec<EntityId> = recipients
            .iter()
            .copied()
            .filter(|&id| id != carrier && self.can_board(id))
            .collect();
        passengers.sort_unstable();
        passengers.dedup();
        if passengers.is_empty() {
            return Err(GameCommandError::NotACarrier(carrier));
        }
        let needed: u32 = passengers.iter().map(|&id| self.passenger_slots(id)).sum();
        let free = capacity.saturating_sub(self.carrier_slots_used(carrier));
        if needed > free {
            return Err(GameCommandError::CarrierFull {
                carrier,
                free,
                needed,
            });
        }
        let count = passengers.len();
        for id in passengers {
            self.commit_task_order(id, UnitOrder::Load { carrier }, queue)?;
        }
        Ok(GameCommandReceipt::Accepted { recipients: count })
    }

    /// Admits `UnloadAll` for every selected carrier holding passengers.
    pub(crate) fn admit_unload(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        queue: bool,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.validate_selection(player, recipients)?;
        let mut count = 0;
        for &id in recipients {
            if self.has_transport_passengers_prevalidated(id) {
                self.commit_task_order(id, UnitOrder::UnloadAll, queue)?;
                count += 1;
            }
        }
        if count == 0 {
            return Err(GameCommandError::NothingToUnload);
        }
        Ok(GameCommandReceipt::Accepted { recipients: count })
    }

    /// One tick of a `Load` order at the front of `id`'s queue.
    pub(crate) fn step_load(&mut self, id: EntityId, carrier: EntityId) {
        let Some(actor) = self.entities.get(&id) else {
            return;
        };
        let target = self
            .entities
            .get(&carrier)
            .filter(|target| target.is_alive() && target.owner == actor.owner);
        let (Some(target), Some(capacity)) = (target, self.carrier_capacity(carrier)) else {
            self.end_task(id, Some(TaskEventKind::CarrierUnavailable));
            return;
        };
        let (from, to) = (actor.position, target.position);
        let radii = self
            .catalog
            .get(actor.unit_type_id)
            .ok()
            .zip(self.catalog.get(target.unit_type_id).ok())
            .map_or(SimScalar::ZERO, |(a, b)| {
                a.collision_radius + b.collision_radius
            });
        let close = checked_distance(from, to).is_some_and(|gap| gap <= radii + BOARDING_REACH);
        if !close {
            self.pursue_carrier(id, to);
            return;
        }
        self.halt_task(id);
        let needed = self.passenger_slots(id);
        if self.carrier_slots_used(carrier) + needed > capacity {
            self.end_task(id, Some(TaskEventKind::CarrierUnavailable));
            return;
        }
        match self.load_transport_prevalidated(id, carrier) {
            Ok(_) => {}
            Err(TransportAdmissionError::Invariant(source)) => {
                self.report_transport_invariant(source);
                self.end_task(id, Some(TaskEventKind::CarrierUnavailable));
            }
            Err(_) => self.end_task(id, Some(TaskEventKind::CarrierUnavailable)),
        }
    }

    /// Keeps a walking passenger's goal on a carrier that may have moved.
    fn pursue_carrier(&mut self, id: EntityId, carrier_at: SimVec2) {
        let Some(actor) = self.entities.get(&id) else {
            return;
        };
        let stale = actor.task.goal.is_none_or(|goal| {
            checked_distance(goal, carrier_at).is_none_or(|gap| gap > BOARDING_REACH)
        });
        if !stale {
            return;
        }
        if actor.task.goal.is_none() && actor.task.legs >= crate::economy::MAX_TASK_LEGS {
            self.end_task(id, Some(TaskEventKind::Unreachable));
            return;
        }
        let was_walking = actor.task.goal.is_some();
        let cell = self.map.world_to_cell(carrier_at).ok();
        let goal = cell.and_then(|cell| self.approach_point(actor.position, Footprint::cell(cell)));
        match goal {
            Some(goal) => {
                self.set_task_goal(id, goal);
                if was_walking {
                    // Following a moving carrier is one journey, not a failed attempt.
                    if let Some(actor) = self.entities.get_mut(&id) {
                        actor.task.legs = actor.task.legs.saturating_sub(1);
                    }
                }
            }
            None => self.end_task(id, Some(TaskEventKind::Unreachable)),
        }
    }

    /// One tick of an `UnloadAll` order at the front of `id`'s queue.
    pub(crate) fn step_unload(&mut self, id: EntityId) {
        // PERFORMANCE RATIONALE:
        // Why: Unloading mutates entities, so it cannot run while scanning for passengers.
        // What: Collect them into one retained buffer rather than a fresh list per tick.
        // Trade-off: A little workspace state in exchange for no allocation on retried ticks.
        let mut passengers = std::mem::take(&mut self.economy.passengers);
        passengers.clear();
        passengers.extend(
            self.entities
                .values()
                .filter(|entity| entity.transport_carrier() == Some(id) && entity.is_alive())
                .map(|entity| entity.id),
        );
        let finished = passengers.is_empty() || self.set_passengers_down(id, &passengers);
        self.economy.passengers = passengers;
        if finished {
            self.end_task(id, None);
            return;
        }
        let waited = self.entities.get_mut(&id).map_or(u16::MAX, |carrier| {
            carrier.task.wait_ticks = carrier.task.wait_ticks.saturating_add(1);
            carrier.task.wait_ticks
        });
        if waited > UNLOAD_PATIENCE_TICKS {
            self.end_task(id, Some(TaskEventKind::NoUnloadSpace));
        }
    }

    /// Sets `passengers` down around `carrier`; true if all of them found ground.
    fn set_passengers_down(&mut self, carrier: EntityId, passengers: &[EntityId]) -> bool {
        let Some(centre) = self.entities.get(&carrier).map(|c| c.position) else {
            return false;
        };
        let mut placed_all = true;
        for &passenger in passengers {
            match self.unload_spot(passenger, centre) {
                Some(spot) => match self.unload_transport_prevalidated(passenger, spot) {
                    Ok(_) => {}
                    Err(TransportAdmissionError::Invariant(source)) => {
                        self.report_transport_invariant(source);
                        placed_all = false;
                    }
                    Err(_) => placed_all = false,
                },
                None => placed_all = false,
            }
        }
        placed_all
    }

    /// The first legal cell centre for `passenger` in rings around `centre`.
    fn unload_spot(&self, passenger: EntityId, centre: SimVec2) -> Option<SimVec2> {
        let (x, y) = self.map.world_to_cell(centre).ok()?;
        let (x, y) = (i32::try_from(x).ok()?, i32::try_from(y).ok()?);
        (0..=UNLOAD_SEARCH_RINGS).find_map(|ring| {
            ring_perimeter(x - ring, x + ring, y - ring, y + ring).find_map(|(cx, cy)| {
                let cell = (u32::try_from(cx).ok()?, u32::try_from(cy).ok()?);
                let spot = Footprint::cell(cell).centre();
                self.validate_displacement_destination_prevalidated(passenger, spot)
                    .ok()
                    .map(|()| spot)
            })
        })
    }
}

impl From<TransportInvariant> for GameCommandError {
    fn from(source: TransportInvariant) -> Self {
        Self::Transport(source.into())
    }
}
