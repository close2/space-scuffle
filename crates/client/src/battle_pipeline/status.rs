//! Maps retained typed command evidence into cached player-facing status. Command
//! messages describe admission only; they never imply arrival or completion. Task
//! event messages describe how a unit's work ended. Exact identities and invariant
//! details remain in the original typed notices, so no message names an entity.
//!
//! Every mapping is an exhaustive match, so a new refusal needs a sentence to compile.
//! The `write_*` functions append; callers clear cached output before replacement.

use std::fmt::Write;

use sim_core::{
    economy::{GameCommandError, GameCommandReceipt, TaskEventKind},
    movement::{DisplacementAdmissionError, MovementAdmissionError},
    production::{construction::BuildRejection, ProductionAdmissionError},
    transport::TransportAdmissionError,
};
use sim_map::PlacementFailureReason;

use super::{commands::SubmissionError, driver::AdmissionOutcome};

/// Appends a pluralized queued/submitted count; callers clear cached output first.
pub(super) fn write_order_count(output: &mut String, count: usize, action: &str) {
    let suffix = if count == 1 { "" } else { "s" };
    let _ = write!(output, "{count} order{suffix} {action}");
}

/// Returns a player-facing cause without exposing retained diagnostic identities.
pub(super) fn submission_reason(error: &SubmissionError) -> &'static str {
    match error {
        SubmissionError::NotReady => "the battle is not ready",
        SubmissionError::GenerationExhausted | SubmissionError::SequenceExhausted => {
            "the battle cannot process another order"
        }
        SubmissionError::StaleGeneration => "the battle was replaced",
        SubmissionError::UnsupportedOrder => "this order is unavailable",
        SubmissionError::InvalidPlayer(_) => "this player is unavailable",
        SubmissionError::EmptySelection => "select units that can perform this order",
        SubmissionError::MissingRecipient(_) => "a selected unit no longer exists",
        SubmissionError::ForeignRecipient(_) => "a selected unit belongs to another player",
        SubmissionError::UnavailableRecipient(_) => "a selected unit is unavailable",
        SubmissionError::Capability(_) => "a selected unit cannot perform this order",
        SubmissionError::MissingArchetype(_) => "the battle cannot process this order",
        SubmissionError::InvalidTarget(_) => "the target is unavailable or friendly",
        SubmissionError::NoDestination => "there is no walkable ground there",
        SubmissionError::MatchOver => "the match is over",
        SubmissionError::PlayerDefeated(_) => "you have been defeated",
        SubmissionError::InvalidStructure(_) => "this structure is unavailable",
    }
}

fn movement_reason(error: &MovementAdmissionError) -> &'static str {
    match error {
        MovementAdmissionError::InvalidPlayer(_) => "this player is unavailable",
        MovementAdmissionError::EmptySelection => "select units to move",
        MovementAdmissionError::MissingRecipient(_) => "a selected unit no longer exists",
        MovementAdmissionError::ForeignRecipient(_) => "a selected unit belongs to another player",
        MovementAdmissionError::DeadRecipient(_) => "a selected unit was destroyed",
        MovementAdmissionError::LoadedRecipient(_) => "a selected unit is being transported",
        MovementAdmissionError::OutsideDomain(_) => "the destination is outside the map",
        MovementAdmissionError::UnsupportedOrder => "this movement order is unavailable",
        MovementAdmissionError::Invariant(_) => "the battle cannot process this order",
    }
}

fn write_production_reason(output: &mut String, error: &ProductionAdmissionError) {
    let reason = match error {
        ProductionAdmissionError::InvalidPlayer(_) => "this player is unavailable",
        ProductionAdmissionError::UnknownProduct(_) => "the requested product is unavailable",
        ProductionAdmissionError::EmptySelection => "select a producer",
        ProductionAdmissionError::MissingRecipient(_) => "a selected producer no longer exists",
        ProductionAdmissionError::ForeignRecipient(_) => {
            "a selected producer belongs to another player"
        }
        ProductionAdmissionError::DeadRecipient(_) => "a selected producer was destroyed",
        ProductionAdmissionError::LoadedRecipient(_) => "a selected producer is being transported",
        ProductionAdmissionError::NoEligibleProducer => {
            "no selected completed producer can make this product"
        }
        ProductionAdmissionError::UnsupportedProductionWorkflow(_) => {
            "this product requires a different production method"
        }
        ProductionAdmissionError::MissingPrerequisite(_) => "a required structure is missing",
        ProductionAdmissionError::Minerals {
            required,
            available,
        } => {
            let _ = write!(
                output,
                "not enough minerals (need {required}, have {available})"
            );
            return;
        }
        ProductionAdmissionError::Gas {
            required,
            available,
        } => {
            let _ = write!(output, "not enough gas (need {required}, have {available})");
            return;
        }
        ProductionAdmissionError::Supply {
            used,
            required,
            capacity,
        } => {
            let _ = write!(
                output,
                "not enough supply ({used} used, {required} needed, {capacity} capacity)"
            );
            return;
        }
        ProductionAdmissionError::UnknownUpgrade(_) => "this research does not exist",
        ProductionAdmissionError::NoEligibleResearcher(_) => {
            "no selected completed structure can research this"
        }
        ProductionAdmissionError::UpgradeAlreadyResearched(_) => {
            "this research is already complete"
        }
        ProductionAdmissionError::UpgradeAlreadyQueued(_) => "this research is already in progress",
        ProductionAdmissionError::UpgradeTierLocked(_) => {
            "the previous tier must be researched first"
        }
        ProductionAdmissionError::Invariant(_) => "the battle cannot process this order",
    };
    output.push_str(reason);
}

const fn placement_reason(reason: PlacementFailureReason) -> &'static str {
    match reason {
        PlacementFailureReason::UnbuildableTerrain => "the ground there cannot be built on",
        PlacementFailureReason::CliffLevelMismatch => "the site spans different cliff levels",
        PlacementFailureReason::OverlapsRamp => "the site overlaps a ramp",
        PlacementFailureReason::OccupiedByStructure(_) => "a structure is in the way",
        PlacementFailureReason::OutOfBounds => "the site lies outside the map",
    }
}

/// Appends why a structure could not be started, at admission or on its builder's arrival.
pub fn write_build_rejection(output: &mut String, rejection: &BuildRejection) {
    let reason = match rejection {
        BuildRejection::UnknownStructure(_) => "this structure is unavailable",
        BuildRejection::NotAStructure(_) => "this is not a structure",
        BuildRejection::MissingPrerequisite(_) => "a required structure is missing",
        BuildRejection::Minerals {
            required,
            available,
        } => {
            let _ = write!(
                output,
                "not enough minerals (need {required}, have {available})"
            );
            return;
        }
        BuildRejection::Gas {
            required,
            available,
        } => {
            let _ = write!(output, "not enough gas (need {required}, have {available})");
            return;
        }
        BuildRejection::OutOfBounds => "the site lies outside the map",
        BuildRejection::Blocked(reason) => placement_reason(*reason),
        BuildRejection::RememberedStructure => "a structure was last seen on this site",
        BuildRejection::CoversResource => "the site covers a resource node",
        BuildRejection::ExtractorNeedsGas => "an extractor needs a gas node under it",
        BuildRejection::Invariant(_) => "the battle cannot process this order",
    };
    output.push_str(reason);
}

const fn displacement_reason(error: &DisplacementAdmissionError) -> &'static str {
    match error {
        DisplacementAdmissionError::RegisteredFoundation(_) => "a foundation cannot be moved",
        DisplacementAdmissionError::MissingEntity(_) => "a selected unit no longer exists",
        DisplacementAdmissionError::DeadEntity(_) => "a selected unit was destroyed",
        DisplacementAdmissionError::OutsideDomain(_) => "the destination is outside the map",
        DisplacementAdmissionError::Terrain(_) => "the ground there is blocked",
        DisplacementAdmissionError::Occupied(_) => "something is in the way",
        DisplacementAdmissionError::Invariant(_) => "the battle cannot process this order",
    }
}

const fn transport_reason(error: &TransportAdmissionError) -> &'static str {
    match error {
        TransportAdmissionError::MissingEntity(_) => "a selected unit no longer exists",
        TransportAdmissionError::SameEntity(_) => "a carrier cannot board itself",
        TransportAdmissionError::DeadEntity(_) => "a selected unit was destroyed",
        TransportAdmissionError::IncompleteEntity(_) => "the carrier is not finished",
        TransportAdmissionError::DifferentOwners { .. } => "the carrier belongs to another player",
        TransportAdmissionError::AlreadyLoaded(_) => "a selected unit is already being transported",
        TransportAdmissionError::HasPassengers(_) => "a carrier with passengers cannot board",
        TransportAdmissionError::Foundation(_) => "a foundation cannot board",
        TransportAdmissionError::Busy(_) => "a selected unit is busy",
        TransportAdmissionError::NotLoaded(_) => "the unit is not being transported",
        TransportAdmissionError::Destination(error) => displacement_reason(error),
        TransportAdmissionError::Invariant(_) => "the battle cannot process this order",
    }
}

/// Appends why the simulation refused an economy, construction or transport command.
pub fn write_game_reason(output: &mut String, error: &GameCommandError) {
    let reason = match error {
        GameCommandError::MatchOver => "the match is over",
        GameCommandError::PlayerDefeated(_) => "you have been defeated",
        GameCommandError::UnsupportedOrder => "this order is unavailable",
        GameCommandError::InvalidPlayer(_) => "this player is unavailable",
        GameCommandError::EmptySelection => "select units that can perform this order",
        GameCommandError::MissingRecipient(_) => "a selected unit no longer exists",
        GameCommandError::ForeignRecipient(_) => "a selected unit belongs to another player",
        GameCommandError::DeadRecipient(_) => "a selected unit was destroyed",
        GameCommandError::LoadedRecipient(_) => "a selected unit is being transported",
        GameCommandError::Build(rejection) => {
            write_build_rejection(output, rejection);
            return;
        }
        GameCommandError::NoCapableBuilder => "no selected unit can build this",
        GameCommandError::NotAFoundation(_) => "that is not one of your foundations",
        GameCommandError::NoCapableHarvester => "no selected unit can harvest",
        GameCommandError::UnknownResourceNode(_) => "that resource node is unavailable",
        GameCommandError::ResourceExhausted(_) => "that resource node is exhausted",
        GameCommandError::ExtractorRequired(_) => "this gas node needs your completed extractor",
        GameCommandError::NoCargo => "no selected unit carries cargo",
        GameCommandError::SingleRecipientRequired => {
            "select exactly one structure to cancel a queued job"
        }
        GameCommandError::NoSuchJob { .. } => "that job is no longer queued",
        GameCommandError::NotACarrier(_) => "that unit cannot carry this",
        GameCommandError::CarrierFull { free, needed, .. } => {
            let _ = write!(
                output,
                "not enough room in the carrier ({needed} needed, {free} free)"
            );
            return;
        }
        GameCommandError::NothingToUnload => "no selected carrier holds passengers",
        GameCommandError::Transport(error) => transport_reason(error),
        GameCommandError::Production(error) => {
            write_production_reason(output, error);
            return;
        }
        GameCommandError::Invariant(_) => "the battle cannot process this order",
    };
    output.push_str(reason);
}

/// Appends how a unit's task ended, or which milestone it reached.
pub fn write_task_event(output: &mut String, kind: &TaskEventKind) {
    let text = match kind {
        TaskEventKind::NotCapable => "a unit cannot carry out its order",
        TaskEventKind::Unreachable => "a unit could not reach its task",
        TaskEventKind::ResourceExhausted => "the resources here have run out",
        TaskEventKind::UnknownResourceNode => "that resource node is unavailable",
        TaskEventKind::ExtractorLost => "the extractor was lost",
        TaskEventKind::NoDepot => "no completed depot remains for the cargo",
        TaskEventKind::BuildAbandoned(rejection) => {
            output.push_str("construction abandoned: ");
            write_build_rejection(output, rejection);
            return;
        }
        TaskEventKind::SiteObstructed => "construction abandoned: the site stayed blocked",
        TaskEventKind::FoundationLost => "the foundation was lost",
        TaskEventKind::StructureCompleted { .. } => "construction complete",
        TaskEventKind::CarrierUnavailable => "the carrier is unavailable",
        TaskEventKind::NoUnloadSpace => "there is no room to set the passengers down",
    };
    output.push_str(text);
}

/// Appends the admission result; callers clear cached output before replacement.
pub(super) fn write_outcome(output: &mut String, outcome: &AdmissionOutcome) {
    match outcome {
        AdmissionOutcome::Movement(outcome) => match &outcome.result {
            Ok(receipt) => {
                let count = receipt.recipient_count;
                let suffix = if count == 1 { "" } else { "s" };
                let _ = write!(output, "Movement order accepted for {count} unit{suffix}");
            }
            Err(error) => {
                let _ = write!(
                    output,
                    "Movement order rejected: {}",
                    movement_reason(error)
                );
            }
        },
        AdmissionOutcome::Production(outcome) => match &outcome.result {
            Ok(_) => output.push_str("Production order accepted"),
            Err(error) => {
                output.push_str("Production order rejected: ");
                write_production_reason(output, error);
            }
        },
        AdmissionOutcome::Game(outcome) => match &outcome.result {
            Ok(GameCommandReceipt::Accepted { recipients }) => {
                let suffix = if *recipients == 1 { "" } else { "s" };
                let _ = write!(output, "Order accepted for {recipients} unit{suffix}");
            }
            Ok(GameCommandReceipt::Refunded { minerals, gas }) => {
                let _ = write!(output, "Cancelled: refunded {minerals} minerals, {gas} gas");
            }
            Err(error) => {
                output.push_str("Order rejected: ");
                write_game_reason(output, error);
            }
        },
    }
}

#[cfg(test)]
mod tests;
