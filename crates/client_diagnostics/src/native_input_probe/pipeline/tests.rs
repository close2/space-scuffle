//! Recorder-only state/CSV/error regressions; real Vulkan/OS-input verification
//! remains separate and is never inferred from these CPU fixtures.

use super::*;
use client::battle_pipeline::{commands::SessionGeneration, session::CommittedBattleMetadata};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn session() -> Result<(sim_core::SimWorld, BattleSession), Box<dyn std::error::Error>> {
    let mut world = sim_core::SimWorld::new_default_battle_sandbox();
    let mut session = BattleSession::prepare(
        SessionGeneration::FIRST,
        &mut world,
        CommittedBattleMetadata::default(),
    )?;
    let mut next = session.frames.current().clone();
    next.tick.0 += 1;
    for bubble in next.bubbles.iter_mut().take(2) {
        bubble.velocity = [1.0, 0.0];
    }
    *session.frames.begin_tick()? = next;
    session.frames.finish_tick()?;
    session.frames.present(1.0)?;
    Ok((world, session))
}

#[test]
fn tracking_retains_stopped_actor_and_records_actual_disappearance() -> TestResult {
    let (_world, mut session) = session()?;
    let directory = tempfile::tempdir()?;
    let mut app = App::new();
    install(
        &mut app,
        &directory.path().join("trace.csv"),
        Arc::default(),
    )?;
    let mut trace = app.world_mut().resource_mut::<Trace>();
    observe_identity(&mut trace, &session);
    let actor = trace.actor.ok_or("moving fixture actor missing")?;
    let mut next = session.frames.current().clone();
    next.tick.0 += 1;
    next.bubbles
        .iter_mut()
        .find(|bubble| bubble.entity_id == actor)
        .ok_or("actor missing")?
        .velocity = [0.0; 2];
    *session.frames.begin_tick()? = next;
    session.frames.finish_tick()?;
    session.frames.present(1.0)?;
    observe_identity(&mut trace, &session);
    assert_eq!(trace.actor, Some(actor));
    assert_eq!(trace.changes, 1);
    let mut next = session.frames.current().clone();
    next.tick.0 += 1;
    next.bubbles.retain(|bubble| bubble.entity_id != actor);
    *session.frames.begin_tick()? = next;
    session.frames.finish_tick()?;
    session.frames.present(1.0)?;
    assert_eq!(observe_identity(&mut trace, &session), Some(actor));
    assert_ne!(trace.actor, Some(actor));
    assert_eq!(trace.changes, 2);
    Ok(())
}

#[test]
fn generation_change_clears_old_receipt_even_if_entity_ids_are_reused() -> TestResult {
    let (_world, session) = session()?;
    let directory = tempfile::tempdir()?;
    let mut app = App::new();
    install(
        &mut app,
        &directory.path().join("trace.csv"),
        Arc::default(),
    )?;
    let mut trace = app.world_mut().resource_mut::<Trace>();
    observe_identity(&mut trace, &session);
    trace.last_outcome = Some((5, 10, 0, 0, true));
    let mut world = sim_core::SimWorld::new_default_battle_sandbox();
    let replacement = BattleSession::prepare(
        SessionGeneration::FIRST.next()?,
        &mut world,
        CommittedBattleMetadata::default(),
    )?;
    observe_identity(&mut trace, &replacement);
    assert_eq!(trace.generation, Some(2));
    assert_eq!(trace.actor, None);
    assert_eq!(trace.last_outcome, None);
    Ok(())
}

#[test]
fn csv_columns_keep_actual_component_evidence_and_failures_survive_resource_drop() -> TestResult {
    let (world, session) = session()?;
    let actor = session
        .frames
        .presented()
        .bubbles
        .first()
        .ok_or("actor missing")?
        .clone();
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("trace.csv");
    let failed = Arc::new(AtomicBool::new(false));
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_event::<AppExit>()
        .add_event::<bevy::input::mouse::MouseButtonInput>()
        .init_resource::<client::runtime_battle::BattleRenderStatus>()
        .init_resource::<Assets<client::terrain::TerrainMaterial>>()
        .init_resource::<Assets<Image>>()
        .insert_resource(session)
        .insert_resource(client::LabState {
            sim_world: Some(world),
            ..default()
        });
    app.world_mut().spawn((
        client::LiveBattleBubble {
            entity_id: actor.entity_id,
        },
        client::BubbleInstance(actor.clone()),
        GlobalTransform::from_translation(Vec3::new(11.0, 12.0, 13.0)),
    ));
    install(&mut app, &path, failed.clone())?;
    app.update();
    let text = std::fs::read_to_string(&path)?;
    let mut lines = text.lines();
    let header: Vec<_> = lines.next().ok_or("header missing")?.split(',').collect();
    let row: Vec<_> = lines.next().ok_or("row missing")?.split(',').collect();
    assert_eq!(header.len(), row.len());
    let value = |name: &str| -> Result<&str, Box<dyn std::error::Error>> {
        Ok(row[header
            .iter()
            .position(|column| *column == name)
            .ok_or("column missing")?])
    };
    assert_eq!(value("transform_x")?, "11");
    assert_eq!(
        value("instance_x")?.parse::<f32>()?.to_bits(),
        actor.world_pos[0].to_bits()
    );
    app.world_mut().resource_mut::<Trace>().writer = BufWriter::new(File::open(&path)?);
    app.update();
    assert!(app
        .world()
        .resource::<Events<AppExit>>()
        .iter_current_update_events()
        .any(|exit| matches!(exit, AppExit::Error(_))));
    drop(app);
    assert!(failed.load(Ordering::Acquire));
    Ok(())
}
