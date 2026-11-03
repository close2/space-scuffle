//! CPU contract tests using the real authored scenario and normal tick driver.
//! Media callback isolation here is not GPU or screenshot acceptance evidence.

use super::*;
use client::battle_pipeline::{
    commands::SessionGeneration,
    session::{BattleSession, CommittedBattleMetadata},
};
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fixture() -> Result<(SimWorld, TransportState, BattleSession), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let definition = BattleDefinition::load(&root, DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut simulation = definition.instantiate()?;
    let state = TransportState::select(
        &simulation,
        &TransportCaptureSpec {
            owner: 0,
            passenger_ordinal: 2,
            carrier_ordinal: 3,
            destination: [14, 14],
        },
    )?;
    let metadata = CommittedBattleMetadata::from_definition(&definition)?;
    let mut pipeline = BattleSession::prepare(SessionGeneration::FIRST, &mut simulation, metadata)?;
    pipeline.advance(&mut simulation, Duration::ZERO, true, true, 1, |_| Ok(()))?;
    Ok((simulation, state, pipeline))
}

#[test]
fn real_scenario_transport_requires_exact_published_tick_and_membership() -> TestResult {
    let (mut simulation, mut state, mut pipeline) = fixture()?;
    let selected = (state.passenger, state.carrier);
    let (_, repeated, _) = fixture()?;
    assert_eq!(selected, (repeated.passenger, repeated.carrier));
    state.verify_simulation(&simulation, Goal::CargoBefore)?;
    state.apply(&mut simulation, Goal::CargoLoaded)?;
    assert!(state
        .verify_simulation(&simulation, Goal::CargoLoaded)
        .is_err());
    assert!(pipeline
        .frames
        .presented()
        .bubbles
        .iter()
        .any(|actor| actor.entity_id == state.passenger));
    pipeline.advance(
        &mut simulation,
        Duration::from_millis(40),
        true,
        false,
        1,
        |_| Ok(()),
    )?;
    assert_eq!(simulation.current_tick, 1);
    state.verify_simulation(&simulation, Goal::CargoLoaded)?;
    // Resume interpolation legitimately retains previous membership until paused hold-exact.
    assert!(pipeline
        .frames
        .presented()
        .bubbles
        .iter()
        .any(|actor| actor.entity_id == state.passenger));
    pipeline.advance(&mut simulation, Duration::ZERO, true, true, 1, |_| Ok(()))?;
    assert_eq!(pipeline.frames.presented().tick.0, 1);
    let loaded: BTreeSet<_> = pipeline
        .frames
        .presented()
        .bubbles
        .iter()
        .map(|actor| actor.entity_id)
        .collect();
    assert_eq!(loaded, state.expected_members);
    assert_eq!(
        state
            .initial_members
            .difference(&loaded)
            .copied()
            .collect::<Vec<_>>(),
        vec![state.passenger]
    );
    state.apply(&mut simulation, Goal::CargoUnloaded)?;
    assert!(state
        .verify_simulation(&simulation, Goal::CargoUnloaded)
        .is_err());
    pipeline.advance(
        &mut simulation,
        Duration::from_millis(40),
        true,
        false,
        1,
        |_| Ok(()),
    )?;
    pipeline.advance(&mut simulation, Duration::ZERO, true, true, 1, |_| Ok(()))?;
    state.verify_simulation(&simulation, Goal::CargoUnloaded)?;
    assert_eq!(pipeline.frames.presented().tick.0, 2);
    let unloaded: BTreeSet<_> = pipeline
        .frames
        .presented()
        .bubbles
        .iter()
        .map(|actor| actor.entity_id)
        .collect();
    assert_eq!(unloaded, state.initial_members);
    assert_eq!(
        unloaded.difference(&loaded).copied().collect::<Vec<_>>(),
        vec![state.passenger]
    );
    assert_eq!(
        state.receipt.map(|receipt| receipt.action),
        Some(sim_core::transport::TransportAction::Unload)
    );
    Ok(())
}

#[test]
fn rejected_real_transaction_and_missing_selector_cannot_create_capture_evidence() -> TestResult {
    let (mut simulation, mut state, _) = fixture()?;
    let before = simulation.compute_checksum();
    state.carrier = state.passenger;
    assert!(state.apply(&mut simulation, Goal::CargoLoaded).is_err());
    assert!(state.receipt.is_none());
    assert_eq!(simulation.compute_checksum(), before);
    assert!(TransportState::select(
        &simulation,
        &TransportCaptureSpec {
            owner: 0,
            passenger_ordinal: usize::MAX,
            carrier_ordinal: 0,
            destination: [12, 10],
        }
    )
    .is_err());
    Ok(())
}

#[test]
fn selected_zero_speed_or_missing_definition_fails_without_ordinal_replacement() -> TestResult {
    let (mut simulation, state, _) = fixture()?;
    let kind = simulation.entities[&state.passenger].unit_type_id;
    simulation.catalog.get_mut(kind)?.max_speed = sim_types::SimScalar::ZERO;
    let spec = TransportCaptureSpec {
        owner: 0,
        passenger_ordinal: 2,
        carrier_ordinal: 3,
        destination: [14, 14],
    };
    let error = TransportState::select(&simulation, &spec)
        .err()
        .ok_or("zero speed must reject selected actor")?;
    assert!(error
        .to_string()
        .contains("no positive authored movement speed"));
    simulation
        .entities
        .get_mut(&state.passenger)
        .ok_or("selected passenger")?
        .unit_type_id = sim_types::UnitTypeId(u16::MAX);
    assert!(TransportState::select(&simulation, &spec).is_err());
    Ok(())
}

/// Characterises the check order of `verify`: membership, then the fixed camera pose,
/// then the viewport pixels. Each stage is made to fail in turn.
#[test]
fn verify_checks_membership_then_camera_pose_then_viewport() -> TestResult {
    let (simulation, mut state, pipeline) = fixture()?;
    let mut world = World::new();
    world.insert_resource(pipeline.with_world(simulation));
    let region = [0.0, 0.0, 1.0, 1.0];
    let resolution = [1280, 720];
    let pose = ([11.0, 10.0], 22.0, [11.0, 20.0, 18.0]);
    let error = state
        .verify(&mut world, Goal::CargoBefore, &[], region, resolution, pose)
        .err()
        .ok_or("a frame with no rendered members must fail")?;
    assert!(
        error.to_string().contains("ECS membership differs"),
        "{error}"
    );
    for id in state.expected_members.clone() {
        world.spawn(client::LiveBattleBubble { entity_id: id });
    }
    state.camera_pose = Some(([0.0, 0.0], 22.0, [11.0, 20.0, 18.0]));
    let error = state
        .verify(&mut world, Goal::CargoBefore, &[], region, resolution, pose)
        .err()
        .ok_or("a moved camera must fail")?;
    assert!(error.to_string().contains("camera changed"), "{error}");
    state.camera_pose = Some(pose);
    let error = state
        .verify(&mut world, Goal::CargoBefore, &[], region, resolution, pose)
        .err()
        .ok_or("an unprojected carrier must fail")?;
    assert!(
        error.to_string().contains("carrier is not visibly inside"),
        "{error}"
    );
    Ok(())
}
