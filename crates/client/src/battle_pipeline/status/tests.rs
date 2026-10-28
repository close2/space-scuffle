//! Checks ordinary status wording independently of typed simulation evidence.

use super::*;
use sim_core::{
    movement::{MovementOutcome, MovementReceipt},
    production::{ProductionJob, ProductionOutcome, ProductionReceipt},
};
use sim_types::{EntityId, PlayerId, Tick, UnitTypeId};

#[test]
fn queued_and_submitted_counts_are_pluralized_without_internal_ticks() {
    for (count, action, expected) in [
        (1, "queued", "1 order queued"),
        (2, "queued", "2 orders queued"),
        (1, "submitted", "1 order submitted"),
        (12, "submitted", "12 orders submitted"),
    ] {
        let mut text = String::new();
        write_order_count(&mut text, count, action);
        assert_eq!(text, expected);
    }
}

#[test]
fn movement_admission_reports_recipients_without_claiming_arrival() {
    for (recipient_count, expected) in [
        (1, "Movement order accepted for 1 unit"),
        (12, "Movement order accepted for 12 units"),
    ] {
        let outcome = AdmissionOutcome::Movement(MovementOutcome {
            tick: Tick(37),
            command_index: 5,
            player: PlayerId(0),
            result: Ok(MovementReceipt { recipient_count }),
        });
        let mut text = String::new();
        write_outcome(&mut text, &outcome);
        assert_eq!(text, expected);
    }
    let rejected = AdmissionOutcome::Movement(MovementOutcome {
        tick: Tick(37),
        command_index: 5,
        player: PlayerId(0),
        result: Err(MovementAdmissionError::OutsideDomain(EntityId(123))),
    });
    let mut text = String::new();
    write_outcome(&mut text, &rejected);
    assert_eq!(
        text,
        "Movement order rejected: the destination is outside the map"
    );
}

#[test]
fn production_admission_distinguishes_acceptance_and_resource_shortage() {
    let results = [
        (
            Ok(ProductionReceipt {
                producer: EntityId(123),
                job: ProductionJob {
                    product: UnitTypeId(60000),
                    remaining_ticks: 45,
                    reserved_supply: 2,
                },
            }),
            "Production order accepted",
        ),
        (
            Err(ProductionAdmissionError::Minerals {
                required: 100,
                available: 50,
            }),
            "Production order rejected: not enough minerals (need 100, have 50)",
        ),
        (
            Err(ProductionAdmissionError::Gas {
                required: 40,
                available: 10,
            }),
            "Production order rejected: not enough gas (need 40, have 10)",
        ),
        (
            Err(ProductionAdmissionError::Supply {
                used: 20,
                required: 2,
                capacity: 20,
            }),
            "Production order rejected: not enough supply (20 used, 2 needed, 20 capacity)",
        ),
    ];
    for (result, expected) in results {
        let mut text = String::new();
        write_outcome(
            &mut text,
            &AdmissionOutcome::Production(ProductionOutcome {
                tick: Tick(37),
                command_index: 5,
                player: PlayerId(0),
                result,
            }),
        );
        assert_eq!(text, expected);
    }
}

#[test]
fn ordinary_rejections_do_not_expose_debug_id_wrappers() {
    let id = EntityId(123);
    let cases = [
        (
            SubmissionError::MissingRecipient(id),
            "a selected unit no longer exists",
        ),
        (
            SubmissionError::ForeignRecipient(id),
            "a selected unit belongs to another player",
        ),
        (
            SubmissionError::InvalidTarget(id),
            "the target is unavailable or friendly",
        ),
        (
            SubmissionError::MissingArchetype(id),
            "the battle cannot process this order",
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(submission_reason(&error), expected);
    }
}
