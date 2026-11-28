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
//! - `LoadUnit` is the carrier's side of `Load` (`StarCraft` II 5.0.14 `CAbilTransport`
//!   `AllowSmartCmd 1`; plan `2026-12-03-movement-work-on-solver` WP-T1, B-247): it goes to
//!   the selection's completed, mobile carriers with room for the passenger, and each pursues
//!   the passenger as a task, which the movement phase walks to the task's goal like any other.
//!   The passenger keeps its own orders until it boards. The same reach and capacity rules
//!   apply at the door, and the pickup ends once the passenger is gone, dead, carried or no
//!   longer fits.

use super::{TransportAdmissionError, TransportInvariant};
use crate::economy::{Footprint, GameCommandError, GameCommandReceipt, TaskEventKind};
use crate::movement::{checked_distance, MovementLayer};
use crate::scenario::ring_perimeter;
use crate::{Entity, SimWorld};
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
    pub(crate) fn carrier_slots_used(&self, carrier: EntityId) -> u32 {
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

    /// Admits `LoadUnit`: every selected carrier that can take `passenger` sets off for it.
    ///
    /// The passenger must be a live, uncarried, completed walker of `player`'s outside any
    /// structure, carrying nobody itself: the passenger `admit_load` would accept. A selected
    /// carrier takes the order when it is completed, mobile and has room for the passenger's
    /// slots; other members, and the passenger itself, are left alone. With no carrier that
    /// takes it, the first selected carrier's reason is the refusal.
    ///
    /// # Errors
    /// The selection's own refusals (`validate_selection`); `NotACarrier` naming the first
    /// recipient when nothing selected carries; the passenger's typed transport refusal
    /// (`MissingEntity`, `DifferentOwners`, `DeadEntity`, `IncompleteEntity`, `AlreadyLoaded`,
    /// `HasPassengers`, `SameEntity` when the passenger is the only carrier);
    /// `UntargetableTarget` for a unit inside a structure; `NotACarrier` naming the carrier for
    /// a passenger that does not walk or a carrier that cannot move; `IncompleteEntity` for an
    /// unfinished carrier; `CarrierFull` for one without room.
    pub(crate) fn admit_load_unit(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        passenger: EntityId,
        queue: bool,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.validate_selection(player, recipients)?;
        let mut carriers: Vec<EntityId> = recipients
            .iter()
            .copied()
            .filter(|&id| self.carrier_capacity(id).is_some())
            .collect();
        carriers.sort_unstable();
        carriers.dedup();
        if carriers.is_empty() {
            let named = recipients.first().copied().unwrap_or(passenger);
            return Err(GameCommandError::NotACarrier(named));
        }
        carriers.retain(|&id| id != passenger);
        let first = *carriers
            .first()
            .ok_or(TransportAdmissionError::SameEntity(passenger))?;
        self.validate_pickup_passenger(player, first, passenger)?;
        let mut refusal = None;
        carriers.retain(|&id| match self.pickup_refusal(id, passenger) {
            Some(reason) => {
                refusal.get_or_insert(reason);
                false
            }
            None => true,
        });
        if carriers.is_empty() {
            return Err(refusal.unwrap_or(GameCommandError::NotACarrier(first)));
        }
        let count = carriers.len();
        for id in carriers {
            self.commit_task_order(id, UnitOrder::LoadUnit { passenger }, queue)?;
        }
        Ok(GameCommandReceipt::Accepted { recipients: count })
    }

    /// Refuses `passenger` as the unit a pickup by `player`'s `carrier` is for, unless
    /// `admit_load` would let it board.
    fn validate_pickup_passenger(
        &self,
        player: PlayerId,
        carrier: EntityId,
        passenger: EntityId,
    ) -> Result<(), GameCommandError> {
        let target = self
            .entities
            .get(&passenger)
            .ok_or(TransportAdmissionError::MissingEntity(passenger))?;
        if target.owner != player {
            return Err(TransportAdmissionError::DifferentOwners { passenger, carrier }.into());
        }
        if !target.is_alive() {
            return Err(TransportAdmissionError::DeadEntity(passenger).into());
        }
        if target.is_loaded_in_transport() {
            return Err(TransportAdmissionError::AlreadyLoaded(passenger).into());
        }
        if target.is_housed() {
            return Err(GameCommandError::UntargetableTarget(passenger));
        }
        if !target.is_completed {
            return Err(TransportAdmissionError::IncompleteEntity(passenger).into());
        }
        if self.has_transport_passengers_prevalidated(passenger) {
            return Err(TransportAdmissionError::HasPassengers(passenger).into());
        }
        if !self.can_board(passenger) {
            return Err(GameCommandError::NotACarrier(carrier));
        }
        Ok(())
    }

    /// Why the selected `carrier` cannot set off for `passenger` now, if it cannot: it is
    /// unfinished, cannot move, or lacks room for the passenger's slots.
    fn pickup_refusal(&self, carrier: EntityId, passenger: EntityId) -> Option<GameCommandError> {
        let entity = self.entities.get(&carrier)?;
        if !entity.is_completed {
            return Some(TransportAdmissionError::IncompleteEntity(carrier).into());
        }
        let mobile = !entity.is_immobile
            && self
                .catalog
                .get(entity.unit_type_id)
                .is_ok_and(|arch| arch.max_speed > SimScalar::ZERO);
        if !mobile {
            return Some(GameCommandError::NotACarrier(carrier));
        }
        let capacity = self.carrier_capacity(carrier)?;
        let free = capacity.saturating_sub(self.carrier_slots_used(carrier));
        let needed = self.passenger_slots(passenger);
        (needed > free).then_some(GameCommandError::CarrierFull {
            carrier,
            free,
            needed,
        })
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
        if !self.within_boarding_reach(actor, target) {
            let to = target.position;
            self.pursue_unit(id, to);
            return;
        }
        self.halt_task(id);
        let needed = self.passenger_slots(id);
        if self.carrier_slots_used(carrier) + needed > capacity {
            self.end_task(id, Some(TaskEventKind::CarrierUnavailable));
            return;
        }
        self.board_at_the_door(id, id, carrier);
    }

    /// One tick of a `LoadUnit` order at the front of carrier `id`'s queue (plan
    /// `2026-12-03-movement-work-on-solver` WP-T1): the mirror of [`Self::step_load`], with
    /// the carrier pursuing the passenger.
    ///
    /// The order ends, with nothing reported, once the passenger is gone, dead, another
    /// player's or carried (by this carrier or another). It ends with
    /// [`TaskEventKind::CarrierUnavailable`] once the passenger no longer fits, checked every
    /// tick, so a carrier filled on the way stops (rallies plan S6, "remove the move
    /// command"). While the passenger is inside a structure the carrier keeps its leg and
    /// waits for it.
    pub(crate) fn step_load_unit(&mut self, id: EntityId, passenger: EntityId) {
        let Some(actor) = self.entities.get(&id) else {
            return;
        };
        let Some(capacity) = self.carrier_capacity(id) else {
            self.end_task(id, Some(TaskEventKind::NotCapable));
            return;
        };
        let target = self.entities.get(&passenger).filter(|target| {
            target.is_alive() && target.owner == actor.owner && !target.is_loaded_in_transport()
        });
        let Some(target) = target else {
            self.end_task(id, None);
            return;
        };
        if self.carrier_slots_used(id) + self.passenger_slots(passenger) > capacity {
            self.end_task(id, Some(TaskEventKind::CarrierUnavailable));
            return;
        }
        if target.is_housed() {
            return;
        }
        if !self.within_boarding_reach(actor, target) {
            let to = target.position;
            self.pursue_unit(id, to);
            return;
        }
        self.halt_task(id);
        if self.board_at_the_door(id, passenger, id) {
            self.end_task(id, None);
        }
    }

    /// Whether `a` and `b` are within [`BOARDING_REACH`] of each other's edge.
    fn within_boarding_reach(&self, a: &Entity, b: &Entity) -> bool {
        let radii = self
            .catalog
            .get(a.unit_type_id)
            .ok()
            .zip(self.catalog.get(b.unit_type_id).ok())
            .map_or(SimScalar::ZERO, |(a, b)| {
                a.collision_radius + b.collision_radius
            });
        checked_distance(a.position, b.position).is_some_and(|gap| gap <= radii + BOARDING_REACH)
    }

    /// Loads `passenger` into `carrier` once they meet, for the task of `actor` (one of the
    /// two); true if it boarded. A refusal ends `actor`'s task with
    /// [`TaskEventKind::CarrierUnavailable`], and a corrupt relation is also diagnosed.
    fn board_at_the_door(
        &mut self,
        actor: EntityId,
        passenger: EntityId,
        carrier: EntityId,
    ) -> bool {
        match self.load_transport_prevalidated(passenger, carrier) {
            Ok(_) => true,
            Err(TransportAdmissionError::Invariant(source)) => {
                self.report_transport_invariant(source);
                self.end_task(actor, Some(TaskEventKind::CarrierUnavailable));
                false
            }
            Err(_) => {
                self.end_task(actor, Some(TaskEventKind::CarrierUnavailable));
                false
            }
        }
    }

    /// Keeps a pursuing unit's task goal on a unit that may have moved, now at `target_at`: a
    /// walking passenger's on its carrier, a carrier's on the passenger it picks up. The goal
    /// is re-aimed only once the target is more than [`BOARDING_REACH`] from it.
    fn pursue_unit(&mut self, id: EntityId, target_at: SimVec2) {
        let Some(actor) = self.entities.get(&id) else {
            return;
        };
        let stale = actor.task.goal.is_none_or(|goal| {
            checked_distance(goal, target_at).is_none_or(|gap| gap > BOARDING_REACH)
        });
        if !stale {
            return;
        }
        if actor.task.goal.is_none() && actor.task.legs >= crate::economy::MAX_TASK_LEGS {
            self.end_task(id, Some(TaskEventKind::Unreachable));
            return;
        }
        let was_walking = actor.task.goal.is_some();
        match self.pursuit_goal(actor, target_at) {
            Some(goal) => {
                self.set_task_goal(id, goal);
                if was_walking {
                    // Following a moving unit is one journey, not a failed attempt.
                    if let Some(actor) = self.entities.get_mut(&id) {
                        actor.task.legs = actor.task.legs.saturating_sub(1);
                    }
                }
            }
            None => self.end_task(id, Some(TaskEventKind::Unreachable)),
        }
    }

    /// Where `actor` goes to reach a unit at `target_at`. A flyer aims at the point itself,
    /// moved into its domain; a walker aims at the point's cell, or the nearest free cell
    /// around it, as a worker approaches a one-cell target ([`Self::approach_cell`]).
    fn pursuit_goal(&self, actor: &Entity, target_at: SimVec2) -> Option<SimVec2> {
        let arch = self.catalog.get(actor.unit_type_id).ok()?;
        if MovementLayer::from_mover(arch.mover_type) == MovementLayer::Air {
            return Some(self.clamp_into_map(target_at, arch.terrain_radius));
        }
        let cell = self.map.world_to_cell(target_at).ok()?;
        self.approach_cell(actor.position, Footprint::cell(cell))
    }

    /// `point` moved, per axis, to at least `radius` (world units) inside the map's edges: the
    /// domain of a body of that terrain radius, which the movement phase keeps every body in.
    /// A map narrower than the body's diameter puts it on the middle line.
    fn clamp_into_map(&self, point: SimVec2, radius: SimScalar) -> SimVec2 {
        let clamp = |value: SimScalar, extent: u32| {
            let high = SimScalar::from_num(extent).saturating_sub(radius);
            if high < radius {
                SimScalar::from_num(extent).saturating_mul(SimScalar::HALF)
            } else {
                value.clamp(radius, high)
            }
        };
        SimVec2::new(
            clamp(point.x, self.map.width()),
            clamp(point.y, self.map.height()),
        )
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

#[cfg(test)]
mod tests {
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{MoverType, PlayerId, SimScalar, SimVec2};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn v(x: f32, y: f32) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    /// A point is moved per axis to the radius inside each edge, a point inside stays, and a
    /// map narrower than the body puts it on the middle line.
    #[test]
    fn clamp_into_map_keeps_a_body_inside_every_edge() -> TestResult {
        let map = GameMap::new(20, 3, MapCell::default_ground(CliffLevel(0)))?;
        let world = crate::test_support::world(map, 5);
        let radius = SimScalar::from_num(2);
        assert_eq!(world.clamp_into_map(v(-3.0, 1.0), radius), v(2.0, 1.5));
        assert_eq!(world.clamp_into_map(v(25.0, 1.0), radius), v(18.0, 1.5));
        let one = SimScalar::ONE;
        assert_eq!(world.clamp_into_map(v(7.25, 1.0), one), v(7.25, 1.0));
        assert_eq!(world.clamp_into_map(v(19.5, 2.5), one), v(19.0, 2.0));
        Ok(())
    }

    /// A flyer's pursuit goal is the target's point clamped into its domain; a walker's is the
    /// target's free cell centre, never the raw point.
    #[test]
    fn pursuit_goal_depends_on_the_pursuer_layer() -> TestResult {
        let map = GameMap::new(24, 24, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = crate::test_support::world(map, 5);
        let kind = |world: &crate::SimWorld, mover| {
            world
                .catalog
                .archetypes()
                .find(|a| a.mover_type == mover && a.max_speed > SimScalar::ZERO)
                .map(|a| (a.unit_type_id, a.terrain_radius))
                .ok_or("the shipped catalog has a mobile unit of each layer")
        };
        let (flyer, terrain) = kind(&world, MoverType::Air)?;
        let (walker, _) = kind(&world, MoverType::Ground)?;
        let flyer = world.spawn_unit(flyer, PlayerId(0), v(12.0, 12.0))?;
        let walker = world.spawn_unit(walker, PlayerId(0), v(14.0, 12.0))?;
        let edge = v(0.25, 6.7);
        let flown = world.pursuit_goal(&world.entities[&flyer], edge);
        assert_eq!(flown, Some(SimVec2::new(terrain, edge.y)));
        let walked = world.pursuit_goal(&world.entities[&walker], edge);
        assert_eq!(walked, Some(v(0.5, 6.5)));
        Ok(())
    }
}
