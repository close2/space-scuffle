//! Checks ordinary status wording independently of typed simulation evidence.

use super::*;
use sim_core::{
    economy::GameCommandOutcome,
    movement::{MovementOutcome, MovementReceipt},
    production::{ProductionInvariant, ProductionJob, ProductionOutcome, ProductionReceipt},
};
use sim_types::{EntityId, PlayerId, ResourceNodeId, Tick, UnitTypeId};

/// An identity whose digits would stand out in any message that leaked it.
const ID: EntityId = EntityId(987_654);
const INVARIANT: ProductionInvariant = ProductionInvariant::IdExhausted;

fn assert_presentable(text: &str, case: &str) {
    assert!(!text.is_empty(), "{case} has no message");
    assert!(!text.contains("987654"), "{case} leaks an id: {text}");
    assert!(!text.contains("EntityId"), "{case} leaks a type: {text}");
    assert!(!text.contains("UnitTypeId"), "{case} leaks a type: {text}");
}

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
                    item: sim_core::production::ProductionItem::Unit(UnitTypeId(60000)),
                    remaining_ticks: 45,
                    total_ticks: 45,
                    reserved_supply: 2,
                    paid_minerals: 0,
                    paid_gas: 0,
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

/// Position of `error` in [`game_errors`]. No wildcard: a new variant stops this compiling,
/// and the table test then fails until the new case is in the table.
const fn game_error_slot(error: &GameCommandError) -> usize {
    match error {
        GameCommandError::MatchOver => 0,
        GameCommandError::PlayerDefeated(_) => 1,
        GameCommandError::UnsupportedOrder => 2,
        GameCommandError::InvalidPlayer(_) => 3,
        GameCommandError::EmptySelection => 4,
        GameCommandError::MissingRecipient(_) => 5,
        GameCommandError::ForeignRecipient(_) => 6,
        GameCommandError::DeadRecipient(_) => 7,
        GameCommandError::LoadedRecipient(_) => 8,
        GameCommandError::Build(_) => 9,
        GameCommandError::NoCapableBuilder => 10,
        GameCommandError::NotAFoundation(_) => 11,
        GameCommandError::NoCapableHarvester => 12,
        GameCommandError::UnknownResourceNode(_) => 13,
        GameCommandError::ResourceExhausted(_) => 14,
        GameCommandError::ExtractorRequired(_) => 15,
        GameCommandError::NoCargo => 16,
        GameCommandError::SingleRecipientRequired => 17,
        GameCommandError::NoSuchJob { .. } => 18,
        GameCommandError::NotACarrier(_) => 19,
        GameCommandError::CarrierFull { .. } => 20,
        GameCommandError::NothingToUnload => 21,
        GameCommandError::Transport(_) => 22,
        GameCommandError::Production(_) => 23,
        GameCommandError::Invariant(_) => 24,
    }
}

fn game_errors() -> [GameCommandError; 25] {
    let node = ResourceNodeId(7);
    [
        GameCommandError::MatchOver,
        GameCommandError::PlayerDefeated(PlayerId(1)),
        GameCommandError::UnsupportedOrder,
        GameCommandError::InvalidPlayer(PlayerId(9)),
        GameCommandError::EmptySelection,
        GameCommandError::MissingRecipient(ID),
        GameCommandError::ForeignRecipient(ID),
        GameCommandError::DeadRecipient(ID),
        GameCommandError::LoadedRecipient(ID),
        GameCommandError::Build(BuildRejection::OutOfBounds),
        GameCommandError::NoCapableBuilder,
        GameCommandError::NotAFoundation(ID),
        GameCommandError::NoCapableHarvester,
        GameCommandError::UnknownResourceNode(node),
        GameCommandError::ResourceExhausted(node),
        GameCommandError::ExtractorRequired(node),
        GameCommandError::NoCargo,
        GameCommandError::SingleRecipientRequired,
        GameCommandError::NoSuchJob {
            producer: ID,
            job_index: 3,
        },
        GameCommandError::NotACarrier(ID),
        GameCommandError::CarrierFull {
            carrier: ID,
            free: 2,
            needed: 4,
        },
        GameCommandError::NothingToUnload,
        GameCommandError::Transport(TransportAdmissionError::MissingEntity(ID)),
        GameCommandError::Production(ProductionAdmissionError::EmptySelection),
        GameCommandError::Invariant(INVARIANT),
    ]
}

#[test]
fn every_game_command_error_has_a_presentable_message() {
    for (slot, error) in game_errors().iter().enumerate() {
        assert_eq!(game_error_slot(error), slot, "{error:?} is out of place");
        let mut text = String::new();
        write_game_reason(&mut text, error);
        assert_presentable(&text, &format!("{error:?}"));
    }
}

#[test]
fn every_build_rejection_has_a_presentable_message() {
    let unit = UnitTypeId(60_000);
    for rejection in [
        BuildRejection::UnknownStructure(unit),
        BuildRejection::NotAStructure(unit),
        BuildRejection::MissingPrerequisite(unit),
        BuildRejection::Minerals {
            required: 100,
            available: 50,
        },
        BuildRejection::Gas {
            required: 40,
            available: 10,
        },
        BuildRejection::OutOfBounds,
        BuildRejection::Blocked(PlacementFailureReason::UnbuildableTerrain),
        BuildRejection::Blocked(PlacementFailureReason::CliffLevelMismatch),
        BuildRejection::Blocked(PlacementFailureReason::OverlapsRamp),
        BuildRejection::Blocked(PlacementFailureReason::OccupiedByStructure(ID)),
        BuildRejection::Blocked(PlacementFailureReason::OutOfBounds),
        BuildRejection::CoversResource,
        BuildRejection::ExtractorNeedsGas,
        BuildRejection::Invariant(INVARIANT),
    ] {
        let mut text = String::new();
        write_build_rejection(&mut text, &rejection);
        assert_presentable(&text, &format!("{rejection:?}"));
        assert!(
            !text.contains("60000"),
            "{rejection:?} leaks a type: {text}"
        );
    }
}

#[test]
fn every_transport_refusal_has_a_presentable_message() {
    for error in [
        TransportAdmissionError::MissingEntity(ID),
        TransportAdmissionError::SameEntity(ID),
        TransportAdmissionError::DeadEntity(ID),
        TransportAdmissionError::IncompleteEntity(ID),
        TransportAdmissionError::DifferentOwners {
            passenger: ID,
            carrier: ID,
        },
        TransportAdmissionError::AlreadyLoaded(ID),
        TransportAdmissionError::HasPassengers(ID),
        TransportAdmissionError::Foundation(ID),
        TransportAdmissionError::Busy(ID),
        TransportAdmissionError::NotLoaded(ID),
        TransportAdmissionError::Destination(DisplacementAdmissionError::RegisteredFoundation(ID)),
        TransportAdmissionError::Destination(DisplacementAdmissionError::MissingEntity(ID)),
        TransportAdmissionError::Destination(DisplacementAdmissionError::DeadEntity(ID)),
        TransportAdmissionError::Destination(DisplacementAdmissionError::OutsideDomain(ID)),
        TransportAdmissionError::Destination(DisplacementAdmissionError::Terrain(ID)),
        TransportAdmissionError::Destination(DisplacementAdmissionError::Occupied(ID)),
    ] {
        assert_presentable(transport_reason(&error), &format!("{error:?}"));
    }
}

#[test]
fn every_task_event_has_a_presentable_message() {
    for kind in [
        TaskEventKind::NotCapable,
        TaskEventKind::Unreachable,
        TaskEventKind::ResourceExhausted,
        TaskEventKind::UnknownResourceNode,
        TaskEventKind::ExtractorLost,
        TaskEventKind::NoDepot,
        TaskEventKind::BuildAbandoned(BuildRejection::Blocked(
            PlacementFailureReason::OccupiedByStructure(ID),
        )),
        TaskEventKind::SiteObstructed,
        TaskEventKind::FoundationLost,
        TaskEventKind::StructureCompleted { foundation: ID },
        TaskEventKind::CarrierUnavailable,
        TaskEventKind::NoUnloadSpace,
    ] {
        let mut text = String::new();
        write_task_event(&mut text, &kind);
        assert_presentable(&text, &format!("{kind:?}"));
    }
}

#[test]
fn each_research_refusal_has_its_own_sentence() {
    let refusals = [
        ProductionAdmissionError::UnknownUpgrade(1),
        ProductionAdmissionError::NoEligibleResearcher(1),
        ProductionAdmissionError::UpgradeAlreadyResearched(1),
        ProductionAdmissionError::UpgradeAlreadyQueued(1),
        ProductionAdmissionError::UpgradeTierLocked(1),
    ];
    let mut sentences = refusals.map(|error| {
        let mut text = String::new();
        write_production_reason(&mut text, &error);
        text
    });
    sentences.sort();
    let total = sentences.len();
    let mut distinct = sentences.to_vec();
    distinct.dedup();
    assert_eq!(distinct.len(), total, "{sentences:?}");
}

#[test]
fn invariants_keep_the_one_wording() {
    let expected = "the battle cannot process this order";
    let mut text = String::new();
    write_game_reason(&mut text, &GameCommandError::Invariant(INVARIANT));
    assert_eq!(text, expected);
    text.clear();
    write_build_rejection(&mut text, &BuildRejection::Invariant(INVARIANT));
    assert_eq!(text, expected);
}

#[test]
fn game_outcomes_report_acceptance_refunds_and_refusals() {
    let cases = [
        (
            Ok(GameCommandReceipt::Accepted { recipients: 1 }),
            "Order accepted for 1 unit",
        ),
        (
            Ok(GameCommandReceipt::Accepted { recipients: 3 }),
            "Order accepted for 3 units",
        ),
        (
            Ok(GameCommandReceipt::Refunded {
                minerals: 75,
                gas: 20,
            }),
            "Cancelled: refunded 75 minerals, 20 gas",
        ),
        (
            Err(GameCommandError::NoCargo),
            "Order rejected: no selected unit carries cargo",
        ),
        (
            Err(GameCommandError::Build(BuildRejection::Gas {
                required: 40,
                available: 10,
            })),
            "Order rejected: not enough gas (need 40, have 10)",
        ),
    ];
    for (result, expected) in cases {
        let mut text = String::new();
        write_outcome(
            &mut text,
            &AdmissionOutcome::Game(GameCommandOutcome {
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
fn closed_match_rejections_read_as_game_over_not_as_a_fault() {
    for (error, expected) in [
        (SubmissionError::MatchOver, "the match is over"),
        (
            SubmissionError::PlayerDefeated(PlayerId(0)),
            "you have been defeated",
        ),
        (
            SubmissionError::InvalidStructure(UnitTypeId(1)),
            "this structure is unavailable",
        ),
    ] {
        assert_eq!(submission_reason(&error), expected);
    }
}
