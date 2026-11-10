//! Verifies adjacent-tick pose/membership and exactly-once event eligibility with
//! actual extracted catalog records, including no-tick and paused presentation.

use super::*;
use sim_core::SimWorld;
use sim_types::{PlayerId, SimScalar};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fixture() -> Result<(BattleFrameState, SimWorld), Box<dyn std::error::Error>> {
    let mut world = sim_fixtures::default_battle_sandbox()?;
    let mut state = BattleFrameState::default();
    world.write_render_snapshot(state.initialize(SessionGeneration::FIRST)?);
    state.finish_initialization()?;
    Ok((state, world))
}

fn event() -> CombatEventVisual {
    CombatEventVisual::HitscanTracer {
        from: [0.0; 3],
        to: [1.0; 3],
        owner: PlayerId(0),
        source: sim_types::EffectSource::default(),
    }
}

#[test]
fn no_tick_frames_interpolate_spatial_pose_but_keep_previous_discrete_state() -> TestResult {
    let (mut state, mut world) = fixture()?;
    let id = state
        .current()
        .bubbles
        .first()
        .ok_or("fixture absent")?
        .entity_id;
    let previous = state.current().bubbles[0].clone();
    let actor = world.entities.get_mut(&id).ok_or("actor absent")?;
    actor.position.x += SimScalar::from_i32(2);
    actor.hp = SimScalar::ONE;
    world.current_tick += 1;
    world.write_render_snapshot(state.begin_tick()?);
    state.finish_tick()?;
    state.present(0.25)?;
    assert_eq!(
        state.presented().bubbles[0].world_pos[0],
        previous.world_pos[0] + 0.5
    );
    assert_eq!(state.presented().bubbles[0].hp_ratio, previous.hp_ratio);
    state.present(0.75)?;
    assert_eq!(
        state.presented().bubbles[0].world_pos[0],
        previous.world_pos[0] + 1.5
    );
    assert_eq!(state.presented().tick, Tick(0));
    state.present(1.0)?;
    assert_eq!(state.presented().bubbles, state.current().bubbles);
    Ok(())
}

#[test]
fn pause_is_exact_at_zero_delta_and_resume_holds_until_a_new_tick() -> TestResult {
    let (mut state, mut world) = fixture()?;
    let id = state
        .current()
        .bubbles
        .first()
        .ok_or("fixture absent")?
        .entity_id;
    world
        .entities
        .get_mut(&id)
        .ok_or("actor absent")?
        .position
        .x += SimScalar::ONE;
    world.current_tick += 1;
    world.write_render_snapshot(state.begin_tick()?);
    state.finish_tick()?;
    state.present(0.25)?;
    state.hold_exact();
    state.present(0.0)?;
    let exact = state.current().bubbles[0].world_pos;
    assert_eq!(state.presented().bubbles[0].world_pos, exact);
    state.present(0.1)?;
    assert_eq!(state.presented().bubbles[0].world_pos, exact);
    world.current_tick += 1;
    world.write_render_snapshot(state.begin_tick()?);
    state.finish_tick()?;
    state.present(0.0)?;
    assert_eq!(state.presented().bubbles[0].world_pos, exact);
    Ok(())
}

#[test]
fn every_catchup_event_waits_for_its_displayed_tick_and_drains_once() -> TestResult {
    let (mut state, mut world) = fixture()?;
    for tick in 1..=3 {
        world.current_tick = tick;
        world.pending_events.push(event());
        world.pending_events.push(event());
        world.write_render_snapshot(state.begin_tick()?);
        state.finish_tick()?;
    }
    state.present(0.5)?;
    let mut drained = Vec::new();
    while let Some(event) = state.pop_presented_event() {
        drained.push((event.tick, event.within_tick_sequence));
    }
    assert_eq!(
        drained,
        [(Tick(1), 0), (Tick(1), 1), (Tick(2), 0), (Tick(2), 1)]
    );
    assert_eq!(state.pending_event_count(), 2);
    state.hold_exact();
    state.present(0.0)?;
    assert_eq!(state.pop_presented_event().map(|e| e.tick), Some(Tick(3)));
    assert_eq!(state.pop_presented_event().map(|e| e.tick), Some(Tick(3)));
    assert_eq!(state.pop_presented_event(), None);
    state.present(0.0)?;
    assert_eq!(state.pop_presented_event(), None);
    Ok(())
}

#[test]
fn removed_entities_wait_until_current_tick_and_reset_discards_old_events() -> TestResult {
    let (mut state, mut world) = fixture()?;
    let id = state
        .current()
        .bubbles
        .first()
        .ok_or("fixture absent")?
        .entity_id;
    world.entities.get_mut(&id).ok_or("actor absent")?.hp = SimScalar::ZERO;
    world.reap_dead_and_emit_shatter();
    world.current_tick += 1;
    world.write_render_snapshot(state.begin_tick()?);
    state.finish_tick()?;
    state.present(0.5)?;
    assert!(state.presented().bubbles.iter().any(|b| b.entity_id == id));
    state.present(1.0)?;
    assert!(!state.presented().bubbles.iter().any(|b| b.entity_id == id));
    assert!(state.pending_event_count() > 0);
    world.write_render_snapshot(state.initialize(SessionGeneration::FIRST.next()?)?);
    state.finish_initialization()?;
    state.present(0.0)?;
    assert_eq!(state.pending_event_count(), 0);
    assert_eq!(state.pop_presented_event(), None);
    Ok(())
}

#[test]
fn heading_and_aim_take_shortest_arc_across_wrap() {
    let a = 179_f32.to_radians();
    let b = (-179_f32).to_radians();
    assert!((angle(a, b, 0.5) - std::f32::consts::PI).abs() < 1e-6);
    assert!((angle(b, a, 0.5) + std::f32::consts::PI).abs() < 1e-6);
}

#[test]
fn initialization_and_tick_publication_are_explicit_boundaries() -> TestResult {
    let mut state = BattleFrameState::default();
    assert_eq!(
        state.finish_initialization(),
        Err(TimelineError::NoInitialization)
    );
    assert!(matches!(
        state.begin_tick(),
        Err(TimelineError::NotInitialized)
    ));
    assert_eq!(state.present(0.0), Err(TimelineError::NotInitialized));
    let mut world = sim_fixtures::default_battle_sandbox()?;
    world.write_render_snapshot(state.initialize(SessionGeneration::FIRST)?);
    assert!(matches!(
        state.begin_tick(),
        Err(TimelineError::NotInitialized)
    ));
    assert_eq!(state.present(0.0), Err(TimelineError::NotInitialized));
    state.finish_initialization()?;
    assert_eq!(
        state.finish_initialization(),
        Err(TimelineError::NoInitialization)
    );
    assert_eq!(state.finish_tick(), Err(TimelineError::NoPreparedTick));
    state.begin_tick()?;
    assert!(matches!(
        state.begin_tick(),
        Err(TimelineError::TickInProgress)
    ));
    assert_eq!(state.present(0.0), Err(TimelineError::TickInProgress));
    assert_eq!(
        state.finish_initialization(),
        Err(TimelineError::TickInProgress)
    );
    assert_eq!(
        state.finish_tick(),
        Err(TimelineError::TickSequence {
            expected: 1,
            actual: 0
        })
    );
    Ok(())
}

#[test]
fn exhausted_tick_preflight_preserves_world_pending_commands_and_event_evidence() -> TestResult {
    use super::super::commands::BattleCommandQueue;
    use sim_types::{PlayerCommand, UnitOrder};
    let mut world = sim_fixtures::default_battle_sandbox()?;
    world.current_tick = u32::MAX;
    let mut state = BattleFrameState::default();
    world.pending_events.push(event());
    world.write_render_snapshot(state.initialize(SessionGeneration::FIRST)?);
    state.finish_initialization()?;
    state.present(1.0)?;
    world.pending_events.push(event());
    let actor = world
        .entities
        .values()
        .find(|e| !e.is_immobile)
        .ok_or("mobile fixture absent")?;
    let mut queue = BattleCommandQueue::default();
    queue.activate(SessionGeneration::FIRST)?;
    queue.submit(
        &world,
        PlayerCommand {
            player_id: actor.owner,
            entities: vec![actor.id],
            order: UnitOrder::HoldPosition,
            queue: false,
        },
    )?;
    let checksum = world.compute_checksum();
    let current = state.current().clone();
    let presented = state.presented().clone();
    let pending = world.pending_events.clone();
    assert!(matches!(
        state.begin_tick(),
        Err(TimelineError::TickExhausted)
    ));
    assert_eq!(state.current(), &current);
    assert_eq!(state.presented(), &presented);
    assert_eq!(state.pending_event_count(), 1);
    assert_eq!(queue.pending_len(), 1);
    assert_eq!(world.current_tick, u32::MAX);
    assert_eq!(world.compute_checksum(), checksum);
    assert_eq!(world.pending_events, pending);
    Ok(())
}

#[test]
fn invalid_fraction_and_reused_generation_preserve_published_state() -> TestResult {
    let (mut state, _) = fixture()?;
    state.present(1.0)?;
    let before = state.presented().clone();
    for fraction in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert_eq!(state.present(fraction), Err(TimelineError::InvalidFraction));
        assert_eq!(state.presented(), &before);
    }
    assert!(matches!(
        state.initialize(SessionGeneration::FIRST),
        Err(TimelineError::StaleGeneration)
    ));
    assert_eq!(state.presented(), &before);
    Ok(())
}

#[test]
fn discontinuous_radius_does_not_interpolate_and_warm_frames_reuse_storage() -> TestResult {
    let (mut state, mut world) = fixture()?;
    let before = state.current().bubbles[0].clone();
    world.current_tick += 1;
    let output = state.begin_tick()?;
    world.write_render_snapshot(output);
    output.bubbles[0].radius *= 2.0;
    output.bubbles[0].world_pos[0] += 8.0;
    state.finish_tick()?;
    state.present(0.5)?;
    assert_eq!(state.presented().bubbles[0], before);
    let pointer = state.presented().bubbles.as_ptr();
    let capacity = state.presented().bubbles.capacity();
    for _ in 0..64 {
        state.present(0.5)?;
        assert_eq!(state.presented().bubbles.as_ptr(), pointer);
        assert_eq!(state.presented().bubbles.capacity(), capacity);
    }
    state.present(1.0)?;
    assert_eq!(state.presented().bubbles[0], state.current().bubbles[0]);
    Ok(())
}

#[test]
fn newly_committed_events_wait_until_the_first_published_pose() -> TestResult {
    let (mut state, mut world) = fixture()?;
    state.present(1.0)?;
    world.pending_events.push(event());
    world.write_render_snapshot(state.initialize(SessionGeneration::FIRST.next()?)?);
    assert_eq!(state.pop_presented_event(), None);
    state.finish_initialization()?;
    assert_eq!(state.pending_event_count(), 1);
    assert_eq!(state.pop_presented_event(), None);
    state.present(1.0)?;
    assert_eq!(
        state.pop_presented_event().map(|event| event.generation),
        Some(SessionGeneration::FIRST.next()?)
    );
    Ok(())
}

#[test]
fn the_presented_instant_is_the_shown_tick_and_its_interpolation_fraction() -> TestResult {
    let (mut state, mut world) = fixture()?;
    assert_eq!(state.presented_instant(), (Tick(0), 0.0), "before any pose");
    state.present(1.0)?;
    assert_eq!(state.presented_instant(), (Tick(0), 0.0));
    world.current_tick += 1;
    world.write_render_snapshot(state.begin_tick()?);
    state.finish_tick()?;
    state.present(0.25)?;
    assert_eq!(state.presented_instant(), (Tick(0), 0.25), "between ticks");
    state.present(0.0)?;
    assert_eq!(state.presented_instant(), (Tick(0), 0.0));
    state.present(1.0)?;
    assert_eq!(
        state.presented_instant(),
        (Tick(1), 0.0),
        "the current tick exactly"
    );
    // A pause holds the current tick exactly, whatever fraction a frame asks for.
    state.hold_exact();
    state.present(0.5)?;
    assert_eq!(state.presented_instant(), (Tick(1), 0.0));
    Ok(())
}
