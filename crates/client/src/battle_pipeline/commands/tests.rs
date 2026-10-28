//! Verifies captured recipients, queue ordering and generation replacement without
//! advancing the simulation from submission or no-tick presentation frames.

use super::*;
use sim_types::SimVec2;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fixture(
) -> Result<(SimWorld, BattleCommandQueue, EntityId, PlayerId), Box<dyn std::error::Error>> {
    let world = SimWorld::new_default_battle_sandbox();
    let actor = world
        .entities
        .values()
        .find(|e| !e.is_immobile)
        .ok_or("mobile fixture absent")?;
    let (id, owner) = (actor.id, actor.owner);
    let mut queue = BattleCommandQueue::default();
    queue.activate(SessionGeneration::FIRST)?;
    Ok((world, queue, id, owner))
}

fn command(id: EntityId, owner: PlayerId, order: UnitOrder, queued: bool) -> PlayerCommand {
    PlayerCommand {
        player_id: owner,
        entities: vec![id],
        order,
        queue: queued,
    }
}

fn bundle() -> TickBundle {
    TickBundle {
        tick: Tick(0),
        commands: Vec::new(),
        state_checksum: None,
    }
}

#[test]
fn submit_never_steps_and_prepared_bundle_preserves_order_and_queue_flag() -> TestResult {
    let (world, mut queue, id, owner) = fixture()?;
    let checksum = world.compute_checksum();
    let a = queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
    let b = queue.submit(
        &world,
        command(
            id,
            owner,
            UnitOrder::Move {
                target: SimVec2::new(SimScalar::from_i32(4), SimScalar::from_i32(4)),
            },
            true,
        ),
    )?;
    assert_eq!(world.current_tick, 0);
    assert_eq!(world.compute_checksum(), checksum);
    assert_eq!(queue.pending_len(), 2);
    let mut input = bundle();
    let mut sequences = Vec::new();
    let mut rejected = Vec::new();
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(sequences, [a, b]);
    assert_eq!(input.commands.len(), 2);
    assert!(!input.commands[0].queue);
    assert!(input.commands[1].queue);
    assert_eq!(input.commands[0].order, UnitOrder::HoldPosition);
    assert_eq!(queue.pending_len(), 0);
    assert_eq!(rejected, []);
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(input.commands, []);
    assert_eq!(sequences, []);
    Ok(())
}

#[test]
fn dead_or_foreign_captured_actor_is_rejected_without_substitution() -> TestResult {
    for foreign in [false, true] {
        let (mut world, mut queue, id, owner) = fixture()?;
        let sequence = queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
        let actor = world.entities.get_mut(&id).ok_or("actor absent")?;
        if foreign {
            actor.owner = PlayerId(owner.0 + 1);
        } else {
            actor.hp = SimScalar::ZERO;
        }
        let mut input = bundle();
        let mut sequences = Vec::new();
        let mut rejected = Vec::new();
        queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
        assert_eq!(input.commands, []);
        assert_eq!(sequences, []);
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].sequence, sequence);
        assert_eq!(
            rejected[0].reason,
            if foreign {
                SubmissionError::ForeignRecipient(id)
            } else {
                SubmissionError::UnavailableRecipient(id)
            }
        );
        assert_eq!(world.current_tick, 0);
    }
    Ok(())
}

#[test]
fn replacement_discards_pending_old_generation_and_identity_overflow_is_typed() -> TestResult {
    let (world, mut queue, id, owner) = fixture()?;
    queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
    queue.activate(SessionGeneration::FIRST.next()?)?;
    assert_eq!(queue.pending_len(), 0);
    queue.next_sequence = u64::MAX;
    assert_eq!(
        queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false)),
        Err(SubmissionError::SequenceExhausted)
    );
    assert_eq!(queue.pending_len(), 0);
    assert_eq!(
        SessionGeneration(u64::MAX).next(),
        Err(SubmissionError::GenerationExhausted)
    );
    Ok(())
}

#[test]
fn absent_session_and_unsupported_capability_are_explicit() -> TestResult {
    let (world, _, id, owner) = fixture()?;
    let mut queue = BattleCommandQueue::default();
    assert_eq!(
        queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false)),
        Err(SubmissionError::NotReady)
    );
    queue.activate(SessionGeneration::FIRST)?;
    assert_eq!(
        queue.submit(&world, command(id, owner, UnitOrder::ReturnCargo, false)),
        Err(SubmissionError::UnsupportedOrder)
    );
    assert_eq!(queue.pending_len(), 0);
    Ok(())
}

#[test]
fn reused_generation_does_not_discard_captured_intent() -> TestResult {
    let (world, mut queue, id, owner) = fixture()?;
    queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
    assert_eq!(
        queue.activate(SessionGeneration::FIRST),
        Err(SubmissionError::StaleGeneration)
    );
    assert_eq!(queue.pending_len(), 1);
    Ok(())
}

#[test]
fn stationary_armed_actor_can_target_hostile_but_cannot_attack_move() -> TestResult {
    let (mut world, mut queue, _, _) = fixture()?;
    let actor = world
        .entities
        .values()
        .find(|entity| {
            world
                .catalog
                .get(entity.unit_type_id)
                .is_ok_and(|a| a.weapon.is_some() || a.ground_weapon.is_some())
        })
        .ok_or("armed actor absent")?;
    let (id, owner) = (actor.id, actor.owner);
    let target = world
        .entities
        .values()
        .find(|e| e.owner != owner)
        .ok_or("enemy absent")?
        .id;
    world
        .entities
        .get_mut(&id)
        .ok_or("actor absent")?
        .is_immobile = true;
    queue.submit(
        &world,
        command(id, owner, UnitOrder::AttackTarget { target }, false),
    )?;
    assert_eq!(
        queue.submit(
            &world,
            command(
                id,
                owner,
                UnitOrder::AttackMove {
                    target: SimVec2::ZERO
                },
                false
            )
        ),
        Err(SubmissionError::Capability(id))
    );
    assert_eq!(
        queue.submit(
            &world,
            command(id, owner, UnitOrder::AttackTarget { target: id }, false)
        ),
        Err(SubmissionError::InvalidTarget(id))
    );
    world.entities.get_mut(&target).ok_or("target absent")?.hp = SimScalar::ZERO;
    let mut input = bundle();
    let mut sequences = Vec::new();
    let mut rejected = Vec::new();
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(input.commands, []);
    assert_eq!(rejected[0].reason, SubmissionError::InvalidTarget(target));
    Ok(())
}

#[test]
fn prepared_sequence_indices_match_real_production_and_movement_outcomes() -> TestResult {
    use sim_types::{RenderSnapshot, UnitTypeId};
    let (mut world, mut queue, mover, owner) = fixture()?;
    let producer = world
        .entities
        .values()
        .find(|entity| {
            entity.owner == owner
                && world
                    .catalog
                    .get(entity.unit_type_id)
                    .is_ok_and(|a| !a.trains_units.is_empty())
        })
        .ok_or("authored producer absent")?
        .id;
    // Invalid product admission is deliberately left to the authoritative phase.
    let production = queue.submit(
        &world,
        command(
            producer,
            owner,
            UnitOrder::Produce {
                unit_type: UnitTypeId(u16::MAX),
            },
            true,
        ),
    )?;
    let movement = queue.submit(
        &world,
        command(
            mover,
            owner,
            UnitOrder::Move {
                target: SimVec2::new(SimScalar::from_i32(4), SimScalar::from_i32(4)),
            },
            false,
        ),
    )?;
    let mut input = bundle();
    let mut sequences = Vec::new();
    let mut rejected = Vec::new();
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    let mut snapshot = RenderSnapshot::default();
    world.step_into(&input, &mut snapshot);
    let production_outcomes: Vec<_> = world.drain_production_outcomes().collect();
    let movement_outcomes: Vec<_> = world.drain_movement_outcomes().collect();
    assert_eq!(production_outcomes.len(), 1);
    assert_eq!(movement_outcomes.len(), 1);
    assert_eq!(sequences[production_outcomes[0].command_index], production);
    assert_eq!(sequences[movement_outcomes[0].command_index], movement);
    assert_eq!(production_outcomes[0].tick, Tick(1));
    assert_eq!(movement_outcomes[0].tick, Tick(1));
    assert!(production_outcomes[0].result.is_err());
    assert_eq!(world.diagnostics(), []);
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    world.step_into(&input, &mut snapshot);
    assert_eq!(world.production_outcomes(), []);
    assert_eq!(world.movement_outcomes(), []);
    assert_eq!(production_outcomes.len(), 1);
    assert_eq!(movement_outcomes.len(), 1);
    Ok(())
}

#[test]
fn fatal_catalog_revalidation_preserves_pending_batch_and_existing_output() -> TestResult {
    let (mut world, mut queue, id, owner) = fixture()?;
    queue.submit(&world, command(id, owner, UnitOrder::HoldPosition, false))?;
    world
        .entities
        .get_mut(&id)
        .ok_or("actor absent")?
        .unit_type_id = sim_types::UnitTypeId(u16::MAX);
    let mut input = bundle();
    input.tick = Tick(17);
    input
        .commands
        .push(command(id, owner, UnitOrder::Stop, false));
    let before = input.clone();
    let mut sequences = vec![SubmissionSequence(99)];
    let mut rejected = Vec::new();
    assert_eq!(
        queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected),
        Err(SubmissionError::MissingArchetype(id))
    );
    assert_eq!(input, before);
    assert_eq!(sequences, [SubmissionSequence(99)]);
    assert_eq!(rejected, []);
    assert_eq!(queue.pending_len(), 1);
    Ok(())
}

#[test]
fn target_availability_matches_simulation_at_submission_and_execution() -> TestResult {
    use sim_map::VisionSource;
    let (mut world, mut queue, id, owner) = fixture()?;
    let target = world
        .entities
        .values()
        .find(|e| e.owner != owner)
        .ok_or("hostile target absent")?
        .id;
    let target_pos = world.entities.get(&target).ok_or("target absent")?.position;
    let order = UnitOrder::AttackTarget { target };
    world
        .entities
        .get_mut(&target)
        .ok_or("target absent")?
        .is_invulnerable = true;
    assert!(!world.is_entity_available_for_combat(owner, target));
    assert_eq!(
        queue.submit(&world, command(id, owner, order.clone(), false)),
        Err(SubmissionError::InvalidTarget(target))
    );
    world
        .entities
        .get_mut(&target)
        .ok_or("target absent")?
        .is_invulnerable = false;
    world
        .entities
        .get_mut(&target)
        .ok_or("target absent")?
        .is_cloaked = true;
    for detector in [false, true] {
        let vision = &mut world.vision_maps[usize::from(owner.0)];
        vision.begin_tick_update();
        vision.apply_vision_source(
            &world.map,
            &VisionSource {
                pos: target_pos,
                mover_type: sim_types::MoverType::Air,
                sight_radius: SimScalar::from_i32(3),
                is_detector: detector,
            },
        );
        vision.end_tick_update();
        assert_eq!(
            world.is_entity_available_for_combat(owner, target),
            detector
        );
        let submitted = queue.submit(&world, command(id, owner, order.clone(), false));
        if detector {
            assert!(submitted.is_ok());
        } else {
            assert_eq!(submitted, Err(SubmissionError::InvalidTarget(target)));
        }
    }
    world.vision_maps[usize::from(owner.0)].begin_tick_update();
    world.vision_maps[usize::from(owner.0)].end_tick_update();
    let mut input = bundle();
    let mut sequences = Vec::new();
    let mut rejected = Vec::new();
    queue.prepare_tick(&world, &mut input, &mut sequences, &mut rejected)?;
    assert_eq!(input.commands, []);
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].reason, SubmissionError::InvalidTarget(target));
    world
        .entities
        .get_mut(&target)
        .ok_or("target absent")?
        .is_cloaked = false;
    assert!(world.is_entity_available_for_combat(owner, target));
    queue.submit(&world, command(id, owner, order, false))?;
    Ok(())
}
