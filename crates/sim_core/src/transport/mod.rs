//! Sole carrier ownership, exact load/unload admission and relation validation.
//!
//! Passengers retain identity, economy and passive state but have no world-space
//! participation. A single passenger-side link is authoritative; inverse queries
//! derive membership in stable entity order. Public transactions validate once,
//! prepare without mutation, then commit under exclusive world access. This is a
//! low-level lifecycle API; [`orders`] builds the player's range and capacity rules on it.

mod lifecycle;
mod orders;
#[cfg(test)]
mod tests;

use crate::movement::{DisplacementAdmissionError, MovementInvariant};
use crate::{Entity, SimWorld, SimulationDiagnostic};
use sim_types::{EntityId, ShapeKind, SimVec2, Tick};
use thiserror::Error;

/// Corrupt existing identity or carrier relation; never an ordinary refused request.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransportInvariant {
    /// A participant's stored identity, definition or lifecycle is invalid.
    #[error("transport entity {entity:?}: {reason}")]
    Entity {
        /// Participant whose stored state is invalid.
        entity: EntityId,
        /// Stable description of the failed contract.
        reason: &'static str,
    },
    /// A stored passenger edge cannot be maintained.
    #[error("transport passenger {passenger:?}, carrier {carrier:?}: {reason}")]
    Link {
        /// Actor holding the authoritative carrier reference.
        passenger: EntityId,
        /// Referenced carrier identity, including missing references.
        carrier: EntityId,
        /// Stable description of the failed relation.
        reason: &'static str,
    },
}

/// An ordinary load/unload refusal or a diagnosed corrupt-state failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransportAdmissionError {
    /// The requested identity is absent; role follows the requested argument.
    #[error("transport participant {0:?} does not exist")]
    MissingEntity(EntityId),
    /// A participant cannot carry itself.
    #[error("transport passenger and carrier are the same entity {0:?}")]
    SameEntity(EntityId),
    /// An existing dead actor cannot board or be unloaded.
    #[error("transport participant {0:?} is destroyed")]
    DeadEntity(EntityId),
    /// Both participants must have completed construction.
    #[error("transport participant {0:?} is incomplete")]
    IncompleteEntity(EntityId),
    /// Valid owners differ; loading never transfers ownership.
    #[error("transport passenger {passenger:?} and carrier {carrier:?} have different owners")]
    DifferentOwners {
        /// Requested passenger.
        passenger: EntityId,
        /// Requested carrier.
        carrier: EntityId,
    },
    /// Loading cannot replace an existing relation or nest a carrier.
    #[error("transport participant {0:?} is already carried")]
    AlreadyLoaded(EntityId),
    /// An actor carrying passengers cannot itself board.
    #[error("transport passenger {0:?} already carries passengers")]
    HasPassengers(EntityId),
    /// A registered foundation cannot leave its map occupancy behind.
    #[error("transport passenger {0:?} is a registered foundation")]
    Foundation(EntityId),
    /// Paid production or channel state cannot be discarded by boarding.
    #[error("transport passenger {0:?} has paid production or an active/pending channel")]
    Busy(EntityId),
    /// An unload request needs an existing relation.
    #[error("transport passenger {0:?} is not carried")]
    NotLoaded(EntityId),
    /// Exact endpoint preparation failed; no nearest-free replacement is attempted.
    #[error(transparent)]
    Destination(#[from] DisplacementAdmissionError),
    /// Existing malformed state also enters the simulation diagnostic channel.
    #[error(transparent)]
    Invariant(#[from] TransportInvariant),
}

/// Which exclusive relation transaction committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportAction {
    /// Passenger became absent from world-space participation.
    Load,
    /// Passenger returned at the exact validated world-space endpoint.
    Unload,
}

/// Derived evidence of one successful transaction, excluded from checksums.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportReceipt {
    /// Passenger whose sole carrier reference changed.
    pub passenger: EntityId,
    /// Actual carrier involved, including the former carrier on unload.
    pub carrier: EntityId,
    /// Applied relation change.
    pub action: TransportAction,
}

#[derive(Default)]
pub(crate) struct TransportWorkspace {
    // Sorted (death root, member) pairs; rebuilt before each lifecycle operation.
    death_members: Vec<(EntityId, EntityId)>,
}

impl SimWorld {
    /// Validates all stored carrier edges in deterministic entity-key order.
    ///
    /// Dead members remain valid until controlled reap. Existing linked members
    /// must be completed, same-owner, nonnested, correctly keyed and catalogued;
    /// passengers may own neither foundations nor paid/channel work. Complexity
    /// is O(N log N), with no inverse scan per passenger and no cached trust bit.
    ///
    /// # Errors
    /// Returns the first malformed stored relation without repair or mutation.
    pub fn validate_transport_links(&self) -> Result<(), TransportInvariant> {
        #[cfg(test)]
        self.transport_validation_count
            .set(self.transport_validation_count.get().saturating_add(1));
        for (&id, passenger) in &self.entities {
            let Some(carrier_id) = passenger.transport_carrier() else {
                continue;
            };
            let invalid = |reason| TransportInvariant::Link {
                passenger: id,
                carrier: carrier_id,
                reason,
            };
            self.validate_transport_identity(id)?;
            let carrier = self
                .entities
                .get(&carrier_id)
                .ok_or_else(|| invalid("carrier is missing"))?;
            self.validate_transport_identity(carrier_id)?;
            if id == carrier_id {
                return Err(invalid("self-carriage is invalid"));
            }
            if carrier.is_loaded_in_transport() {
                return Err(invalid("nested cargo is invalid"));
            }
            if passenger.owner != carrier.owner {
                return Err(invalid("stored owners differ"));
            }
            if !passenger.is_completed || !carrier.is_completed {
                return Err(invalid("linked participant is incomplete"));
            }
            if self.map.structures().contains_key(&id) || self.transport_is_dome(id)? {
                return Err(invalid("passenger owns or requires a foundation"));
            }
            if passenger_busy(passenger) {
                return Err(invalid("passenger has paid production or channel state"));
            }
        }
        Ok(())
    }

    /// Whether `carrier` has any registered passenger, including dead members
    /// awaiting reap. This rare inverse query scans in deterministic ID order.
    ///
    /// # Errors
    /// Returns malformed graph or requested-carrier identity evidence.
    pub fn has_transport_passengers(&self, carrier: EntityId) -> Result<bool, TransportInvariant> {
        self.validate_transport_links()?;
        self.validate_transport_identity(carrier)?;
        Ok(self.has_transport_passengers_prevalidated(carrier))
    }

    pub(crate) fn has_transport_passengers_prevalidated(&self, carrier: EntityId) -> bool {
        self.entities
            .values()
            .any(|actor| actor.transport_carrier() == Some(carrier))
    }

    /// Loads one eligible same-owner passenger without inventing boarding range,
    /// capacity or catalog capabilities. Carrier work is retained. Passenger
    /// orders/route/target clear and velocity becomes zero; other fields persist.
    ///
    /// # Errors
    /// Refusals preserve all authoritative fields. Corruption emits one diagnostic.
    pub fn try_load_unit_into_transport(
        &mut self,
        passenger: EntityId,
        carrier: EntityId,
    ) -> Result<TransportReceipt, TransportAdmissionError> {
        let result = self
            .validate_transport_links()
            .map_err(Into::into)
            .and_then(|()| self.load_transport_prevalidated(passenger, carrier));
        self.report_transport_result(&result);
        result
    }

    /// Compatibility adapter for typed loading. Ordinary rejection returns false;
    /// corruption emits diagnostic evidence. Prefer `try_load_unit_into_transport`.
    pub fn load_unit_into_transport(&mut self, unit_id: EntityId, transport_id: EntityId) -> bool {
        self.try_load_unit_into_transport(unit_id, transport_id)
            .is_ok()
    }

    fn load_transport_prevalidated(
        &mut self,
        passenger: EntityId,
        carrier: EntityId,
    ) -> Result<TransportReceipt, TransportAdmissionError> {
        let actor = self.transport_requested_participant(passenger)?;
        let source = self.transport_requested_participant(carrier)?;
        if passenger == carrier {
            return Err(TransportAdmissionError::SameEntity(passenger));
        }
        for entity in [actor, source] {
            if !entity.is_alive() {
                return Err(TransportAdmissionError::DeadEntity(entity.id));
            }
            if !entity.is_completed {
                return Err(TransportAdmissionError::IncompleteEntity(entity.id));
            }
        }
        if actor.owner != source.owner {
            return Err(TransportAdmissionError::DifferentOwners { passenger, carrier });
        }
        for entity in [actor, source] {
            if entity.is_loaded_in_transport() {
                return Err(TransportAdmissionError::AlreadyLoaded(entity.id));
            }
        }
        if self.has_transport_passengers_prevalidated(passenger) {
            return Err(TransportAdmissionError::HasPassengers(passenger));
        }
        if self.map.structures().contains_key(&passenger) {
            return Err(TransportAdmissionError::Foundation(passenger));
        }
        if self.transport_is_dome(passenger)? {
            return Err(TransportInvariant::Entity {
                entity: passenger,
                reason: "dome foundation is missing",
            }
            .into());
        }
        if passenger_busy(actor) {
            return Err(TransportAdmissionError::Busy(passenger));
        }
        let actor = self
            .entities
            .get_mut(&passenger)
            .ok_or(TransportInvariant::Entity {
                entity: passenger,
                reason: "prepared passenger disappeared before commit",
            })?;
        actor.transport_carrier = Some(carrier);
        actor.order_queue.clear();
        actor.abandon_task_leg();
        actor.velocity = SimVec2::ZERO;
        actor.current_target = None;
        actor.current_funnel_waypoints.clear();
        Ok(TransportReceipt {
            passenger,
            carrier,
            action: TransportAction::Load,
        })
    }

    /// Returns a living passenger at exactly `destination` in world units.
    /// Intervening terrain and carrier distance do not affect this low-level API.
    /// The carrier is not exempted from destination occupancy.
    ///
    /// # Errors
    /// Refusals preserve link, pose, work and economy. Corruption emits one diagnostic.
    pub fn try_unload_unit_from_transport(
        &mut self,
        passenger: EntityId,
        destination: SimVec2,
    ) -> Result<TransportReceipt, TransportAdmissionError> {
        let result = self
            .validate_transport_links()
            .map_err(Into::into)
            .and_then(|()| self.unload_transport_prevalidated(passenger, destination));
        self.report_transport_result(&result);
        result
    }

    /// Unloads a unit from a transport craft at the specified landing position.
    pub fn unload_unit_from_transport(&mut self, unit_id: EntityId, unload_pos: SimVec2) -> bool {
        self.try_unload_unit_from_transport(unit_id, unload_pos)
            .is_ok()
    }

    fn unload_transport_prevalidated(
        &mut self,
        passenger: EntityId,
        destination: SimVec2,
    ) -> Result<TransportReceipt, TransportAdmissionError> {
        let actor = self.transport_requested_participant(passenger)?;
        if self.map.structures().contains_key(&passenger) {
            return Err(DisplacementAdmissionError::RegisteredFoundation(passenger).into());
        }
        let carrier = actor
            .transport_carrier()
            .ok_or(TransportAdmissionError::NotLoaded(passenger))?;
        let source = self.transport_requested_participant(carrier)?;
        for entity in [actor, source] {
            if !entity.is_alive() {
                return Err(TransportAdmissionError::DeadEntity(entity.id));
            }
            if !entity.is_completed {
                return Err(TransportAdmissionError::IncompleteEntity(entity.id));
            }
        }
        self.validate_displacement_destination_prevalidated(passenger, destination)?;
        let actor = self
            .entities
            .get_mut(&passenger)
            .ok_or(TransportInvariant::Entity {
                entity: passenger,
                reason: "prepared passenger disappeared before unload commit",
            })?;
        actor.transport_carrier = None;
        actor.position = destination;
        actor.velocity = SimVec2::ZERO;
        actor.order_queue.clear();
        actor.current_funnel_waypoints.clear();
        // A swing frozen in the hold was bound to orders the unit no longer has.
        if matches!(
            actor.weapon_phase,
            crate::entity::WeaponPhase::Windup { .. }
        ) {
            actor.weapon_phase = crate::entity::WeaponPhase::Ready;
        }
        Ok(TransportReceipt {
            passenger,
            carrier,
            action: TransportAction::Unload,
        })
    }

    fn transport_requested_participant(
        &self,
        id: EntityId,
    ) -> Result<&Entity, TransportAdmissionError> {
        let actor = self
            .entities
            .get(&id)
            .ok_or(TransportAdmissionError::MissingEntity(id))?;
        self.validate_transport_identity(id)?;
        Ok(actor)
    }

    fn validate_transport_identity(&self, id: EntityId) -> Result<(), TransportInvariant> {
        let invalid = |reason| TransportInvariant::Entity { entity: id, reason };
        let actor = self
            .entities
            .get(&id)
            .ok_or_else(|| invalid("participant is missing"))?;
        if actor.id != id {
            return Err(invalid("map key and entity identity differ"));
        }
        if usize::from(actor.owner.0) >= self.player_resources.len() {
            return Err(invalid("owner slot is invalid"));
        }
        self.catalog
            .get(actor.unit_type_id)
            .map_err(|_| invalid("participant archetype is missing"))?;
        Ok(())
    }

    fn transport_is_dome(&self, id: EntityId) -> Result<bool, TransportInvariant> {
        let actor = self.entities.get(&id).ok_or(TransportInvariant::Entity {
            entity: id,
            reason: "participant is missing",
        })?;
        let arch =
            self.catalog
                .get(actor.unit_type_id)
                .map_err(|_| TransportInvariant::Entity {
                    entity: id,
                    reason: "participant archetype is missing",
                })?;
        Ok(matches!(arch.shape, ShapeKind::BuildingDome { .. }))
    }

    pub(crate) fn report_transport_invariant(&mut self, source: TransportInvariant) {
        self.tick_diagnostics
            .push(SimulationDiagnostic::TransportInvariant {
                tick: Tick(self.current_tick),
                source,
            });
    }

    fn report_transport_result(
        &mut self,
        result: &Result<TransportReceipt, TransportAdmissionError>,
    ) {
        match result {
            Err(TransportAdmissionError::Invariant(source)) => {
                self.report_transport_invariant(source.clone());
            }
            Err(TransportAdmissionError::Destination(DisplacementAdmissionError::Invariant(
                source,
            ))) => {
                self.report_movement_invariant(source.clone());
            }
            _ => {}
        }
    }
}

fn passenger_busy(entity: &Entity) -> bool {
    !entity.production_queue.is_empty()
        || entity.channeling_remaining_ticks != 0
        || entity.pending_ability.is_some()
}

impl From<TransportInvariant> for MovementInvariant {
    fn from(source: TransportInvariant) -> Self {
        Self::Transport(source)
    }
}
