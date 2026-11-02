//! CPU tests of the real sole tick path. Media callbacks here isolate lifecycle
//! failures; separate `runtime_media` integration tests use the actual asset loader.

use super::*;
use crate::battle_pipeline::session::CommittedBattleMetadata;
use sim_types::{PlayerCommand, SimScalar, SimVec2, UnitOrder, UnitTypeId};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fixture() -> Result<(SimWorld, BattleSession), PipelineError> {
    let mut world = SimWorld::new_default_battle_sandbox();
    let session = BattleSession::prepare(
        SessionGeneration::FIRST,
        &mut world,
        CommittedBattleMetadata::default(),
    )?;
    Ok((world, session))
}

fn mobile_order(world: &SimWorld) -> Result<PlayerCommand, Box<dyn std::error::Error>> {
    let actor = world
        .entities
        .values()
        .find(|actor| !actor.is_immobile)
        .ok_or("mobile actor missing")?;
    Ok(PlayerCommand {
        player_id: actor.owner,
        entities: vec![actor.id],
        order: UnitOrder::Move {
            target: SimVec2::new(SimScalar::from_i32(4), SimScalar::from_i32(4)),
        },
        queue: false,
    })
}

#[test]
fn paused_intent_waits_for_one_real_tick_and_no_tick_frames_do_not_reexecute() -> TestResult {
    let (mut world, mut session) = fixture()?;
    let sequence = session.commands.submit(&world, mobile_order(&world)?)?;
    let checksum = world.compute_checksum();
    session.advance(&mut world, Duration::from_secs(10), true, true, 4, |_| {
        Ok(())
    })?;
    assert_eq!(world.current_tick, 0);
    assert_eq!(world.compute_checksum(), checksum);
    assert_eq!(session.commands.pending_len(), 1);
    session.advance(
        &mut world,
        Duration::from_millis(40),
        true,
        false,
        1,
        |_| Ok(()),
    )?;
    assert_eq!(world.current_tick, 1);
    assert_eq!(session.commands.pending_len(), 0);
    assert_eq!(session.notices.len(), 1);
    assert_eq!(session.notices[0].sequence, sequence);
    let checksum = world.compute_checksum();
    session.advance(
        &mut world,
        Duration::from_millis(10),
        true,
        false,
        1,
        |_| Ok(()),
    )?;
    assert_eq!(world.current_tick, 1);
    assert_eq!(world.compute_checksum(), checksum);
    assert_eq!(session.notices.len(), 0);
    assert_eq!(session.outcome_count, 1);
    Ok(())
}

#[test]
fn catchup_preserves_first_tick_rejections_before_later_ticks_clear_sim_outputs() -> TestResult {
    let (mut world, mut session) = fixture()?;
    let producer = world
        .entities
        .values()
        .find(|entity| {
            world
                .catalog
                .get(entity.unit_type_id)
                .is_ok_and(|a| !a.trains_units.is_empty())
        })
        .ok_or("producer missing")?;
    let sequence = session.commands.submit(
        &world,
        PlayerCommand {
            player_id: producer.owner,
            entities: vec![producer.id],
            order: UnitOrder::Produce {
                unit_type: UnitTypeId(u16::MAX),
            },
            queue: true,
        },
    )?;
    let movement = session.commands.submit(&world, mobile_order(&world)?)?;
    session.advance(
        &mut world,
        Duration::from_millis(120),
        true,
        false,
        1,
        |_| Ok(()),
    )?;
    assert_eq!(world.current_tick, 3);
    assert_eq!(world.production_outcomes(), []);
    assert_eq!(world.movement_outcomes(), []);
    assert_eq!(session.notices.len(), 2);
    assert_eq!(session.notices[0].sequence, sequence);
    assert_eq!(session.notices[1].sequence, movement);
    assert!(
        matches!(&session.notices[0].outcome, AdmissionOutcome::Production(outcome) if outcome.tick == Tick(1) && outcome.result.is_err())
    );
    assert_eq!(session.outcome_count, 2);
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

#[test]
fn exhaustion_precedes_command_dequeue_or_world_step() -> TestResult {
    let mut world = SimWorld::new_default_battle_sandbox();
    world.current_tick = u32::MAX;
    let mut session = BattleSession::prepare(
        SessionGeneration::FIRST,
        &mut world,
        CommittedBattleMetadata::default(),
    )?;
    session.commands.submit(&world, mobile_order(&world)?)?;
    let checksum = world.compute_checksum();
    assert!(matches!(
        session.advance(
            &mut world,
            Duration::from_millis(40),
            true,
            false,
            1,
            |_| Ok(())
        ),
        Err(PipelineError::Timeline(TimelineError::TickExhausted))
    ));
    assert_eq!(session.commands.pending_len(), 1);
    assert_eq!(world.current_tick, u32::MAX);
    assert_eq!(world.compute_checksum(), checksum);
    Ok(())
}

#[test]
fn suspended_tabs_drop_phase_but_preserve_intent_and_resume_exact_pose() -> TestResult {
    let (mut world, mut session) = fixture()?;
    session.advance(
        &mut world,
        Duration::from_millis(90),
        true,
        false,
        1,
        |_| Ok(()),
    )?;
    session.commands.submit(&world, mobile_order(&world)?)?;
    session.advance(&mut world, Duration::from_secs(3), false, false, 4, |_| {
        Ok(())
    })?;
    assert_eq!(world.current_tick, 2);
    assert_eq!(session.clock.debt(), Duration::ZERO);
    assert_eq!(session.frames.presented(), session.frames.current());
    session.advance(
        &mut world,
        Duration::from_millis(10),
        true,
        false,
        1,
        |_| Ok(()),
    )?;
    assert_eq!(world.current_tick, 2);
    assert_eq!(session.frames.presented(), session.frames.current());
    assert_eq!(session.commands.pending_len(), 1);
    Ok(())
}

#[test]
fn later_media_failure_retains_completed_ticks_outcomes_and_pending_events() -> TestResult {
    let (mut world, mut session) = fixture()?;
    session.advance(&mut world, Duration::ZERO, true, true, 1, |_| Ok(()))?;
    session.commands.submit(&world, mobile_order(&world)?)?;
    world
        .pending_events
        .push(sim_types::CombatEventVisual::HitscanTracer {
            from: [0.0; 3],
            to: [1.0; 3],
            owner: sim_types::PlayerId(0),
        });
    let result = session.advance(
        &mut world,
        Duration::from_millis(120),
        true,
        false,
        1,
        |snapshot| {
            if snapshot.tick == Tick(2) {
                Err(crate::media::ClientMediaError::UnknownIdentity {
                    unit_type: UnitTypeId(u16::MAX),
                })
            } else {
                Ok(())
            }
        },
    );
    assert!(matches!(result, Err(PipelineError::Media(_))));
    assert_eq!(world.current_tick, 2);
    assert_eq!(session.frames.current().tick, Tick(2));
    assert_eq!(session.frames.presented().tick, Tick(0));
    assert_eq!(session.notices.len(), 1);
    assert!(session.frames.pending_event_count() > 0);
    assert!(!session.is_ready());
    Ok(())
}

#[test]
fn reset_replaces_generation_and_old_commands_without_replaying_events() -> TestResult {
    let (mut world, mut session) = fixture()?;
    session.commands.submit(&world, mobile_order(&world)?)?;
    session.advance(&mut world, Duration::ZERO, true, true, 1, |_| Ok(()))?;
    let mut candidate = SimWorld::new_default_battle_sandbox();
    let replacement = BattleSession::prepare(
        session.next_generation()?,
        &mut candidate,
        CommittedBattleMetadata::default(),
    )?;
    assert_eq!(
        session.commands.pending_len(),
        1,
        "preparation does not mutate active session"
    );
    session = replacement;
    assert_eq!(
        session.commands.generation(),
        Some(SessionGeneration::FIRST.next()?)
    );
    assert_eq!(session.commands.pending_len(), 0);
    assert_eq!(session.frames.pending_event_count(), 0);
    assert_eq!(session.outcome_count, 0);
    assert!(!session.is_ready());
    Ok(())
}

#[test]
fn five_tick_budget_retains_time_and_delivers_delayed_combat_once() -> TestResult {
    let (mut world, mut session) = fixture()?;
    session.advance(&mut world, Duration::ZERO, true, true, 1, |_| Ok(()))?;
    world
        .pending_events
        .push(sim_types::CombatEventVisual::HitscanTracer {
            from: [0.0; 3],
            to: [1.0; 3],
            owner: sim_types::PlayerId(0),
        });
    session.advance(
        &mut world,
        Duration::from_millis(40),
        true,
        false,
        1,
        |_| Ok(()),
    )?;
    assert_eq!(session.frames.presented().tick, Tick(0));
    assert!(session.frames.pop_presented_event().is_none());
    session.advance(
        &mut world,
        Duration::from_millis(20),
        true,
        false,
        1,
        |_| Ok(()),
    )?;
    assert_eq!(world.current_tick, 1);
    assert!(session.frames.pop_presented_event().is_none());
    session.advance(
        &mut world,
        Duration::from_millis(20),
        true,
        false,
        1,
        |_| Ok(()),
    )?;
    assert_eq!(
        session.frames.pop_presented_event().map(|event| event.tick),
        Some(Tick(1))
    );
    assert!(session.frames.pop_presented_event().is_none());
    session.advance(&mut world, Duration::from_secs(1), true, false, 1, |_| {
        Ok(())
    })?;
    assert_eq!(world.current_tick, 7, "only five catch-up ticks execute");
    assert_eq!(session.clock.debt(), Duration::from_millis(50));
    assert_eq!(
        session.clock.diagnostics().discarded_last_frame,
        Duration::from_millis(750)
    );
    assert_eq!(session.frames.current().tick, Tick(7));
    Ok(())
}

#[test]
fn fatal_simulation_preserves_completed_snapshot_and_actual_outcomes() -> TestResult {
    let (mut world, mut session) = fixture()?;
    let actor = world
        .entities
        .values()
        .find(|entity| {
            world
                .catalog
                .get(entity.unit_type_id)
                .is_ok_and(|a| a.weapon.is_some() || a.ground_weapon.is_some())
        })
        .ok_or("armed actor missing")?
        .id;
    let producer = world
        .entities
        .values()
        .find(|entity| {
            world
                .catalog
                .get(entity.unit_type_id)
                .is_ok_and(|a| !a.trains_units.is_empty())
        })
        .ok_or("producer missing")?;
    session.commands.submit(
        &world,
        PlayerCommand {
            player_id: producer.owner,
            entities: vec![producer.id],
            order: UnitOrder::Produce {
                unit_type: UnitTypeId(u16::MAX),
            },
            queue: true,
        },
    )?;
    world
        .entities
        .get_mut(&actor)
        .ok_or("actor missing")?
        .active_buffs
        .push(sim_core::ActiveBuff {
            buff_id: 42,
            remaining_ticks: 50,
            speed_multiplier: SimScalar::ONE,
            attack_rate_multiplier: SimScalar::ZERO,
        });
    let result = session.advance(
        &mut world,
        Duration::from_millis(200),
        true,
        false,
        1,
        |_| Ok(()),
    );
    assert!(matches!(result, Err(PipelineError::Simulation(_))));
    assert_eq!(world.current_tick, 1);
    assert_eq!(session.frames.current().tick, Tick(1));
    assert_eq!(session.notices.len(), 1);
    assert_ne!(session.diagnostics, []);
    assert_eq!(world.diagnostics(), []);
    Ok(())
}
