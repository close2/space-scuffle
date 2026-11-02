//! Retains player intent until the sole tick owner executes it. Recipients and
//! local sequence identities are captured at submission; execution revalidates
//! those same actors rather than silently substituting a later UI selection.

use std::collections::VecDeque;
use std::fmt::Write;

use bevy::prelude::Resource;
use sim_core::{movement::MovementLayer, Entity, SimWorld, UnitArchetype};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, Tick, TickBundle, UnitOrder, UnitTypeId,
};

/// Successful session-commit identity, independent of ticks and recycled entity IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionGeneration(u64);

impl SessionGeneration {
    /// Returns the first committed session identity.
    pub const FIRST: Self = Self(1);

    /// Returns a distinct replacement identity before any scene mutation.
    /// # Errors
    /// Exhaustion rejects replacement instead of aliasing an earlier session.
    pub fn next(self) -> Result<Self, SubmissionError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(SubmissionError::GenerationExhausted)
    }

    /// Returns the numeric identity for diagnostics and capture manifests.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Client-local ordered submission identity; never authoritative simulation state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubmissionSequence(pub u64);

/// Explicit intent admission/revalidation failure, separate from simulation outcomes.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SubmissionError {
    /// No successful loaded-session commit is available for controls.
    #[error("battle session is not ready")]
    NotReady,
    /// Local identity space cannot wrap safely.
    #[error("battle session generation exhausted")]
    GenerationExhausted,
    /// Local submission identity space cannot wrap safely.
    #[error("battle command submission sequence exhausted")]
    SequenceExhausted,
    /// A queued request must belong to the current committed session.
    #[error("command belongs to a replaced battle session")]
    StaleGeneration,
    /// The current client queue intentionally exposes only implemented controls.
    #[error("this order is not exposed by the battle command queue")]
    UnsupportedOrder,
    /// Unsupported/nonparticipant simulation owner.
    #[error("invalid command player {0:?}")]
    InvalidPlayer(PlayerId),
    /// No selected or capable recipients.
    #[error("no eligible recipients for this order")]
    EmptySelection,
    /// Selected entity disappeared before execution.
    #[error("command recipient {0:?} no longer exists")]
    MissingRecipient(EntityId),
    /// Ownership cannot be inferred from a prior selection.
    #[error("command recipient {0:?} is owned by another player")]
    ForeignRecipient(EntityId),
    /// A dead, transported or (for every order but cancelling) unfinished recipient.
    #[error("command recipient {0:?} is unavailable")]
    UnavailableRecipient(EntityId),
    /// Required authored mechanic is absent or currently inapplicable.
    #[error("command recipient {0:?} lacks the required active capability")]
    Capability(EntityId),
    /// A live actor's missing catalog definition is a fatal session invariant.
    #[error("command recipient {0:?} has no valid catalog definition")]
    MissingArchetype(EntityId),
    /// A direct attack needs a live, exposed hostile target.
    #[error("attack target {0:?} is unavailable or friendly")]
    InvalidTarget(EntityId),
    /// The pointer found neither a target nor walkable ground to send units to.
    #[error("no walkable ground at the order destination")]
    NoDestination,
    /// The match has ended, so nothing further can be ordered. Looking around still works.
    #[error("the match is over")]
    MatchOver,
    /// This player was defeated in a match that continues without them.
    #[error("{0:?} has been defeated")]
    PlayerDefeated(PlayerId),
    /// A `Build` names an identity the loaded catalog does not hold.
    #[error("structure {0:?} is not in the catalog")]
    InvalidStructure(UnitTypeId),
}

impl SubmissionError {
    /// Distinguishes corrupt client/world identities from ordinary player rejection.
    #[must_use]
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::GenerationExhausted | Self::SequenceExhausted | Self::MissingArchetype(_)
        )
    }
}

struct PendingCommand {
    generation: SessionGeneration,
    sequence: SubmissionSequence,
    command: PlayerCommand,
}

/// Revalidation evidence for an intent omitted from an actual execution bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RejectedSubmission {
    /// Session originally targeted by this request.
    pub generation: SessionGeneration,
    /// Local identity correlating the request with its original UI/controller action.
    pub sequence: SubmissionSequence,
    /// Intended owner, retained even when recipients changed ownership.
    pub player: PlayerId,
    /// Exact client rejection; no authoritative success receipt is fabricated.
    pub reason: SubmissionError,
}

/// Persistent pending commands and cached human-readable submission status.
#[derive(Resource, Default)]
pub struct BattleCommandQueue {
    generation: Option<SessionGeneration>,
    next_sequence: u64,
    pending: VecDeque<PendingCommand>,
    status: String,
}

impl BattleCommandQueue {
    /// Commits a new session identity and discards only replaced-session intent.
    /// # Errors
    /// Rejects a reused or older identity without discarding pending intent.
    pub fn activate(&mut self, generation: SessionGeneration) -> Result<(), SubmissionError> {
        if self.generation.is_some_and(|current| generation <= current) {
            return Err(SubmissionError::StaleGeneration);
        }
        self.generation = Some(generation);
        self.pending.clear();
        self.status.clear();
        Ok(())
    }

    /// Returns the successfully committed identity, or None before readiness.
    #[must_use]
    pub fn generation(&self) -> Option<SessionGeneration> {
        self.generation
    }

    /// Inspects captured requests in FIFO order without mutating or consuming them.
    pub fn pending_commands(&self) -> impl Iterator<Item = (SubmissionSequence, &PlayerCommand)> {
        self.pending
            .iter()
            .map(|pending| (pending.sequence, &pending.command))
    }

    /// Returns the pending count without consuming intent on paused/no-tick frames.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Returns retained status text; it changes only on submissions/execution.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Records an immediate UI/controller rejection without changing the simulation.
    pub fn reject(&mut self, error: &SubmissionError) {
        self.status.clear();
        let _ = write!(
            self.status,
            "Order rejected: {}",
            super::status::submission_reason(error)
        );
    }

    /// Captures a complete command in FIFO order without advancing the world.
    /// # Errors
    /// Rejects absent session, invalid recipients/capabilities or identity exhaustion.
    pub fn submit(
        &mut self,
        world: &SimWorld,
        command: PlayerCommand,
    ) -> Result<SubmissionSequence, SubmissionError> {
        let generation = self.generation.ok_or(SubmissionError::NotReady)?;
        validate(world, &command)?;
        let next = self
            .next_sequence
            .checked_add(1)
            .ok_or(SubmissionError::SequenceExhausted)?;
        let sequence = SubmissionSequence(next);
        self.pending.push_back(PendingCommand {
            generation,
            sequence,
            command,
        });
        self.next_sequence = next;
        self.status.clear();
        super::status::write_order_count(&mut self.status, self.pending.len(), "queued");
        Ok(sequence)
    }

    /// Selects current owned capable actors for the existing roster-level controls.
    /// Selection allocation belongs to a new user action, not frame synchronization.
    /// # Errors
    /// Returns typed errors for unsupported controls, missing definitions or no actors.
    pub fn submit_roster_order(
        &mut self,
        world: &SimWorld,
        player: PlayerId,
        order: UnitOrder,
        queued: bool,
    ) -> Result<SubmissionSequence, SubmissionError> {
        accepts_orders(world, player)?;
        supported(&order)?;
        let subject = order_subject(world, player, &order)?;
        let mut recipients = Vec::new();
        for entity in world
            .entities
            .values()
            .filter(|e| e.owner == player && can_receive_order(e, &order))
        {
            let archetype = world
                .catalog
                .get(entity.unit_type_id)
                .map_err(|_| SubmissionError::MissingArchetype(entity.id))?;
            if can_execute(entity, archetype, &order, subject) {
                recipients.push(entity.id);
            }
        }
        self.submit(
            world,
            PlayerCommand {
                player_id: player,
                entities: recipients,
                order,
                queue: queued,
            },
        )
    }

    /// Sends `order` to the members of `selection` that can carry it out.
    ///
    /// `selection` is client state and may name units that died, changed owner or lack the
    /// capability, so those are left out here rather than rejecting the whole group. Allocation
    /// belongs to the user action, not to frame synchronization.
    /// # Errors
    /// Returns [`SubmissionError::EmptySelection`] if no member can carry out `order`, or the
    /// other typed errors of [`Self::submit`].
    pub fn submit_selection(
        &mut self,
        world: &SimWorld,
        player: PlayerId,
        selection: &[EntityId],
        order: UnitOrder,
        queued: bool,
    ) -> Result<SubmissionSequence, SubmissionError> {
        accepts_orders(world, player)?;
        supported(&order)?;
        let subject = order_subject(world, player, &order)?;
        let mut recipients = Vec::with_capacity(selection.len());
        for id in selection {
            let Some(entity) = world
                .entities
                .get(id)
                .filter(|entity| entity.owner == player && can_receive_order(entity, &order))
            else {
                continue;
            };
            let archetype = world
                .catalog
                .get(entity.unit_type_id)
                .map_err(|_| SubmissionError::MissingArchetype(*id))?;
            if can_execute(entity, archetype, &order, subject) {
                recipients.push(*id);
            }
        }
        self.submit(
            world,
            PlayerCommand {
                player_id: player,
                entities: recipients,
                order,
                queue: queued,
            },
        )
    }

    /// Prepares one exact pre-step bundle, retaining scratch allocation for later ticks.
    /// `sequences[index]` correlates each authoritative command outcome with submission.
    /// Rejections are appended to caller-owned evidence before another tick can clear it.
    /// # Errors
    /// An uncommitted session or fatal catalog invariant rejects preparation before
    /// dequeuing intent. Ordinary queued errors become rejection evidence.
    pub fn prepare_tick(
        &mut self,
        world: &SimWorld,
        bundle: &mut TickBundle,
        sequences: &mut Vec<SubmissionSequence>,
        rejected: &mut Vec<RejectedSubmission>,
    ) -> Result<(), SubmissionError> {
        let generation = self.generation.ok_or(SubmissionError::NotReady)?;
        let previous_rejections = rejected.len();
        // Preserve the entire pending batch when a corrupted live definition makes
        // execution unsafe; ordinary actor/capability changes remain per-order rejections.
        for pending in &self.pending {
            if let Err(error) = validate(world, &pending.command) {
                if error.is_fatal() {
                    return Err(error);
                }
            }
        }
        bundle.tick = Tick(world.current_tick);
        bundle.state_checksum = None;
        bundle.commands.clear();
        sequences.clear();
        while let Some(pending) = self.pending.pop_front() {
            let result = if pending.generation == generation {
                validate(world, &pending.command)
            } else {
                Err(SubmissionError::StaleGeneration)
            };
            if let Err(reason) = result {
                self.reject(&reason);
                rejected.push(RejectedSubmission {
                    generation: pending.generation,
                    sequence: pending.sequence,
                    player: pending.command.player_id,
                    reason,
                });
            } else {
                sequences.push(pending.sequence);
                bundle.commands.push(pending.command);
            }
        }
        if !bundle.commands.is_empty() {
            self.status.clear();
            super::status::write_order_count(&mut self.status, bundle.commands.len(), "submitted");
            if let Some(last) = rejected
                .last()
                .filter(|_| rejected.len() > previous_rejections)
            {
                let _ = write!(
                    self.status,
                    "; {} rejected: {}",
                    rejected.len() - previous_rejections,
                    super::status::submission_reason(&last.reason)
                );
            }
        }
        Ok(())
    }
}

/// Refuses every order once the match has ended or `player` has been defeated.
///
/// This is the simulation's own rule, checked here so a closed match answers at submission
/// instead of queueing commands the world will only refuse.
fn accepts_orders(world: &SimWorld, player: PlayerId) -> Result<(), SubmissionError> {
    if world.match_state.result.is_over() {
        return Err(SubmissionError::MatchOver);
    }
    if world.match_state.is_defeated(player) {
        return Err(SubmissionError::PlayerDefeated(player));
    }
    Ok(())
}

/// Whether the battle queue carries `order` at all. Exhaustive, so a new order cannot be
/// refused by accident: it fails to compile until someone decides here.
const fn supported(order: &UnitOrder) -> Result<(), SubmissionError> {
    match order {
        UnitOrder::Idle
        | UnitOrder::Move { .. }
        | UnitOrder::AttackMove { .. }
        | UnitOrder::AttackTarget { .. }
        | UnitOrder::HoldPosition
        | UnitOrder::Stop
        | UnitOrder::UseStim
        | UnitOrder::Produce { .. }
        | UnitOrder::Harvest { .. }
        | UnitOrder::ReturnCargo
        | UnitOrder::Build { .. }
        | UnitOrder::Construct { .. }
        | UnitOrder::CancelConstruction
        | UnitOrder::Research { .. }
        | UnitOrder::CancelProduction { .. }
        | UnitOrder::Load { .. }
        | UnitOrder::UnloadAll => Ok(()),
        // Abilities have no UI in Phase 5, so this order is refused deliberately.
        UnitOrder::CastAbility { .. } => Err(SubmissionError::UnsupportedOrder),
    }
}

/// Whether `archetype` carries any weapon. The one rule client admission and the card share.
#[must_use]
pub const fn is_armed(archetype: &UnitArchetype) -> bool {
    archetype.weapon.is_some()
        || archetype.ground_weapon.is_some()
        || archetype.air_weapon.is_some()
}

/// Whether `entity` has what `order` asks for. `subject` comes from [`order_subject`].
fn capable(
    entity: &Entity,
    archetype: &UnitArchetype,
    order: &UnitOrder,
    subject: Option<&UnitArchetype>,
) -> bool {
    let mobile = !entity.is_immobile && archetype.max_speed > SimScalar::ZERO;
    let armed = is_armed(archetype);
    match order {
        // These roster controls intentionally address movable actors only. This is
        // a UI selection policy, not a general simulation order-permission rule.
        UnitOrder::Move { .. } | UnitOrder::HoldPosition | UnitOrder::Stop | UnitOrder::Idle => {
            mobile
        }
        UnitOrder::AttackMove { .. } => mobile && armed,
        UnitOrder::AttackTarget { .. } => {
            armed || entity.is_sieged && entity.siege_weapon.is_some()
        }
        UnitOrder::UseStim => archetype.overclock.is_some_and(|spec| {
            entity.hp > SimScalar::ONE && spec.attack_rate_multiplier > SimScalar::ZERO
        }),
        // Production and research authorize the entire group/product transaction in sim_core.
        // Client admission does not fabricate a receipt or preempt its typed outcome.
        UnitOrder::Produce { .. } | UnitOrder::Research { .. } => true,
        UnitOrder::Harvest { .. } => archetype.capabilities.harvester.is_some(),
        UnitOrder::ReturnCargo => entity.cargo.is_some(),
        UnitOrder::Build { .. } | UnitOrder::Construct { .. } => {
            subject.is_some_and(|structure| archetype.can_raise(structure))
        }
        UnitOrder::CancelConstruction => entity.construction.is_some(),
        UnitOrder::CancelProduction { .. } => !entity.production_queue.is_empty(),
        UnitOrder::Load { carrier } => {
            mobile
                && entity.id != *carrier
                && MovementLayer::from_mover(archetype.mover_type) == MovementLayer::Ground
        }
        UnitOrder::UnloadAll => archetype.capabilities.transport_capacity > 0,
        UnitOrder::CastAbility { .. } => false,
    }
}

/// Whether `entity` is in play to hear `order`. Only cancelling reaches an unfinished foundation.
fn can_receive_order(entity: &Entity, order: &UnitOrder) -> bool {
    entity.is_alive()
        && !entity.is_loaded_in_transport()
        && (entity.is_completed || matches!(order, UnitOrder::CancelConstruction))
}

/// Whether `entity` can carry out `order`. `subject` comes from [`order_subject`].
fn can_execute(
    entity: &Entity,
    archetype: &UnitArchetype,
    order: &UnitOrder,
    subject: Option<&UnitArchetype>,
) -> bool {
    let reaches_target = !matches!(order, UnitOrder::AttackTarget { .. })
        || subject.is_some_and(|target| {
            archetype
                .weapon_against(
                    target.mover_type,
                    entity
                        .is_sieged
                        .then_some(entity.siege_weapon.as_ref())
                        .flatten(),
                )
                .is_some()
        });
    reaches_target && capable(entity, archetype, order, subject)
}

/// The archetype an order is aimed at: an attack's target, or the structure of `Build` or
/// `Construct`. Every other order has no subject.
fn order_subject<'w>(
    world: &'w SimWorld,
    player: PlayerId,
    order: &UnitOrder,
) -> Result<Option<&'w UnitArchetype>, SubmissionError> {
    let (aimed_at, entity) = match *order {
        UnitOrder::AttackTarget { target, .. } => (
            target,
            world.entities.get(&target).filter(|entity| {
                entity.owner != player && world.is_entity_available_for_combat(player, target)
            }),
        ),
        UnitOrder::Construct { foundation } => (
            foundation,
            world
                .entities
                .get(&foundation)
                .filter(|entity| entity.owner == player && entity.is_alive()),
        ),
        UnitOrder::Build { structure_type, .. } => {
            return world
                .catalog
                .get(structure_type)
                .map(Some)
                .map_err(|_| SubmissionError::InvalidStructure(structure_type));
        }
        UnitOrder::Idle
        | UnitOrder::Move { .. }
        | UnitOrder::AttackMove { .. }
        | UnitOrder::HoldPosition
        | UnitOrder::Stop
        | UnitOrder::UseStim
        | UnitOrder::Produce { .. }
        | UnitOrder::Harvest { .. }
        | UnitOrder::ReturnCargo
        | UnitOrder::CastAbility { .. }
        | UnitOrder::CancelConstruction
        | UnitOrder::Research { .. }
        | UnitOrder::CancelProduction { .. }
        | UnitOrder::Load { .. }
        | UnitOrder::UnloadAll => return Ok(None),
    };
    let entity = entity.ok_or(SubmissionError::InvalidTarget(aimed_at))?;
    world
        .catalog
        .get(entity.unit_type_id)
        .map(Some)
        .map_err(|_| SubmissionError::MissingArchetype(aimed_at))
}

fn validate(world: &SimWorld, command: &PlayerCommand) -> Result<(), SubmissionError> {
    accepts_orders(world, command.player_id)?;
    supported(&command.order)?;
    if usize::from(command.player_id.0) >= world.player_resources.len() {
        return Err(SubmissionError::InvalidPlayer(command.player_id));
    }
    if command.entities.is_empty() {
        return Err(SubmissionError::EmptySelection);
    }
    let subject = order_subject(world, command.player_id, &command.order)?;
    for id in &command.entities {
        let entity = world
            .entities
            .get(id)
            .ok_or(SubmissionError::MissingRecipient(*id))?;
        if entity.owner != command.player_id {
            return Err(SubmissionError::ForeignRecipient(*id));
        }
        if !can_receive_order(entity, &command.order) {
            return Err(SubmissionError::UnavailableRecipient(*id));
        }
        let archetype = world
            .catalog
            .get(entity.unit_type_id)
            .map_err(|_| SubmissionError::MissingArchetype(*id))?;
        if !can_execute(entity, archetype, &command.order, subject) {
            return Err(SubmissionError::Capability(*id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
