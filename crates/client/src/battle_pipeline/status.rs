//! Maps retained typed command evidence into cached player-facing status. These
//! messages describe admission only; they never imply arrival or completion.
//! Exact identities and invariant details remain in the original typed notices.

use std::fmt::Write;

use sim_core::{movement::MovementAdmissionError, production::ProductionAdmissionError};

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
        ProductionAdmissionError::Invariant(_) => "the battle cannot process this order",
    };
    output.push_str(reason);
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
    }
}

#[cfg(test)]
mod tests;
