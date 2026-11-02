//! Exercises retained presentation through the real scenario, media loader and
//! live synchronizer. Named warmed phases also provide allocation-profiler scope;
//! capacity assertions alone are deliberately not treated as allocation evidence.

use std::time::Duration;

use bevy::prelude::*;
use client::battle_pipeline::{
    commands::SessionGeneration,
    driver::{advance_battle_ticks, publish_combat_events},
    session::{BattleSession, CommittedBattleMetadata},
};
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn presentation_app() -> Result<App, Box<dyn std::error::Error>> {
    let root = client::ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut simulation = definition.instantiate()?;
    let session = BattleSession::prepare(
        SessionGeneration::FIRST,
        &mut simulation,
        CommittedBattleMetadata::from_definition(&definition)?,
    )?;
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: root.as_path().to_string_lossy().into_owned(),
            ..default()
        },
    ))
    .add_event::<AppExit>()
    .add_event::<client::CombatVisualEvent>()
    .init_asset::<Mesh>()
    .init_asset::<Image>()
    .init_asset::<StandardMaterial>()
    .init_asset::<client::BubbleDomeMaterial>()
    .init_asset::<client::SelectionRingMaterial>()
    .init_resource::<client::RenderEntityMap>()
    .insert_resource(definition.palette.clone())
    .init_resource::<client::player_presentation::PresentationStatus>()
    .insert_resource(root)
    .insert_resource(client::CatalogResource(definition.catalogs.units.clone()))
    .insert_resource(session)
    .insert_resource(client::LabState {
        current_tab: client::LabTab::LiveBattle,
        sim_world: Some(simulation),
        battle_paused: true,
        simulation_speed: 1,
        ..default()
    })
    .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        Duration::from_millis(20),
    ))
    .add_systems(
        Startup,
        (
            client::setup_bubble_assets_system,
            client::setup_indicator_assets_system,
        ),
    )
    .add_systems(
        Update,
        (
            advance_battle_ticks,
            client::sync_live_battle_system,
            publish_combat_events,
        )
            .chain(),
    );
    for _ in 0..8 {
        app.update();
    }
    if let Some(failure) = &app.world().resource::<client::LabState>().battle_failure {
        return Err(failure.clone().into());
    }
    Ok(app)
}

fn profiler_gate() -> TestResult {
    let Some(directory) = std::env::var_os("SPACE_SCUFFLE_ALLOCATION_GATE") else {
        return Ok(());
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::write(directory.join("ready"), std::process::id().to_string())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while !directory.join("release").exists() {
        if std::time::Instant::now() >= deadline {
            return Err("allocation profiler controller did not release the warmed fixture".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

// PERFORMANCE RATIONALE:
// Why: Whole-process allocation totals mix initial media decoding and ECS setup
// with the retained steady-state path being evaluated.
// What: Keep a named, non-inlined warmed phase for external call-stack profiling.
// Trade-off: The test retains a function boundary solely for measurement scope.
#[inline(never)]
fn measured_paused_presentation(app: &mut App) {
    for _ in 0..64 {
        app.update();
    }
}

#[test]
fn warmed_paused_path_preserves_real_world_and_exact_render_pose() -> TestResult {
    let mut app = presentation_app()?;
    let before = app
        .world()
        .resource::<client::LabState>()
        .sim_world
        .as_ref()
        .ok_or("world missing")?
        .compute_checksum();
    profiler_gate()?;
    measured_paused_presentation(&mut app);
    let session = app.world().resource::<BattleSession>();
    assert!(session.is_ready());
    assert_eq!(session.frames.presented(), session.frames.current());
    let snapshot = session.frames.presented().clone();
    // C-5: a paused battle changes nothing, so no material may be re-prepared. Production
    // AssetEvents runs in Last and the event queue retains a two-frame window.
    let asset_events = app
        .world()
        .resource::<Events<AssetEvent<client::BubbleDomeMaterial>>>();
    assert_eq!(
        asset_events.len(),
        0,
        "paused frames must not touch materials"
    );
    let world = app
        .world()
        .resource::<client::LabState>()
        .sim_world
        .as_ref()
        .ok_or("world missing")?;
    assert_eq!(world.current_tick, 0);
    assert_eq!(world.compute_checksum(), before);
    let mut query = app.world_mut().query::<(
        &client::LiveBattleBubble,
        &client::BubbleInstance,
        &Transform,
    )>();
    assert_eq!(query.iter(app.world()).count(), snapshot.bubbles.len());
    for (live, instance, transform) in query.iter(app.world()) {
        let expected = snapshot
            .bubbles
            .binary_search_by_key(&live.entity_id, |bubble| bubble.entity_id)
            .map(|index| &snapshot.bubbles[index])
            .map_err(|_| "render identity missing from published snapshot")?;
        assert_eq!(&instance.0, expected);
        let altitude = if matches!(expected.shape, sim_types::ShapeKind::BuildingDome { .. }) {
            0.0
        } else {
            expected.radius
        };
        assert_eq!(
            transform.translation,
            Vec3::new(
                expected.world_pos[0],
                expected.world_pos[2] + altitude,
                expected.world_pos[1]
            )
        );
    }
    Ok(())
}

/// C-5: `BubbleOpticsPlugin` owns the optics lanes of a shared uniform. Snapshot sync must neither
/// see them as a difference nor reset them, or the two systems re-prepare every material forever.
#[test]
fn snapshot_sync_preserves_optics_lanes_without_touching_materials() -> TestResult {
    let mut app = presentation_app()?;
    let light = Vec4::new(0.2, 0.9, 0.1, 0.7);
    let mut materials = app
        .world_mut()
        .resource_mut::<Assets<client::BubbleDomeMaterial>>();
    assert!(!materials.is_empty());
    for (_, material) in materials.iter_mut() {
        material.uniform.optics_light = light;
    }
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(
        app.world()
            .resource::<Events<AssetEvent<client::BubbleDomeMaterial>>>()
            .iter_current_update_events()
            .count(),
        0
    );
    for (_, material) in app
        .world()
        .resource::<Assets<client::BubbleDomeMaterial>>()
        .iter()
    {
        assert_eq!(material.uniform.optics_light, light);
    }
    Ok(())
}

// Same profiling boundary rationale as the paused phase above.
#[inline(never)]
fn measured_moving_no_tick_presentation(app: &mut App) {
    for _ in 0..16 {
        app.update();
    }
}

#[inline(never)]
fn measured_membership_tick(app: &mut App) {
    app.update();
}

#[inline(never)]
fn allocation_positive_control() -> Vec<u8> {
    std::hint::black_box(vec![17; 4096])
}

/// C-5: the shader reads no position, so a unit may only touch its material when it turns.
fn assert_only_turning_units_touched_materials(app: &App, before: &sim_types::RenderSnapshot) {
    let session = app.world().resource::<BattleSession>();
    let turning = before
        .bubbles
        .iter()
        .zip(&session.frames.presented().bubbles)
        .filter(|(a, b)| a.heading_angle_rad.to_bits() != b.heading_angle_rad.to_bits())
        .count();
    let touched = app
        .world()
        .resource::<Events<AssetEvent<client::BubbleDomeMaterial>>>()
        .len();
    assert!(
        turning < before.bubbles.len() && touched <= turning * 16,
        "{touched} material events over 16 no-tick frames, {turning} of {} bubbles turning",
        before.bubbles.len()
    );
}

#[test]
fn moving_no_tick_frames_interpolate_and_real_death_changes_membership() -> TestResult {
    let mut app = presentation_app()?;
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_paused = false;
    *app.world_mut()
        .resource_mut::<bevy::time::TimeUpdateStrategy>() =
        bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_millis(40));
    app.update();
    let before = app
        .world()
        .resource::<BattleSession>()
        .frames
        .presented()
        .clone();
    let tick = app
        .world()
        .resource::<BattleSession>()
        .frames
        .current()
        .tick;
    let checksum = app
        .world()
        .resource::<client::LabState>()
        .sim_world
        .as_ref()
        .ok_or("world missing")?
        .compute_checksum();
    *app.world_mut()
        .resource_mut::<bevy::time::TimeUpdateStrategy>() =
        bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_millis(1));
    profiler_gate()?;
    measured_moving_no_tick_presentation(&mut app);
    let session = app.world().resource::<BattleSession>();
    assert_only_turning_units_touched_materials(&app, &before);
    assert_eq!(session.frames.current().tick, tick);
    assert_eq!(
        app.world()
            .resource::<client::LabState>()
            .sim_world
            .as_ref()
            .ok_or("world missing")?
            .compute_checksum(),
        checksum
    );
    assert!(
        before
            .bubbles
            .iter()
            .zip(&session.frames.presented().bubbles)
            .any(|(a, b)| a.entity_id == b.entity_id
                && a.world_pos.map(f32::to_bits) != b.world_pos.map(f32::to_bits)),
        "actual authored initial movement must advance presentation on no-tick frames"
    );
    let victim = before.bubbles.first().ok_or("real roster empty")?.entity_id;
    {
        let mut lab = app.world_mut().resource_mut::<client::LabState>();
        let simulation = lab.sim_world.as_mut().ok_or("world missing")?;
        simulation
            .entities
            .get_mut(&victim)
            .ok_or("victim missing")?
            .hp = sim_types::SimScalar::ZERO;
        simulation.reap_dead_and_emit_shatter();
        simulation.validate_supply_ledger()?;
    }
    *app.world_mut()
        .resource_mut::<bevy::time::TimeUpdateStrategy>() =
        bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_millis(40));
    measured_membership_tick(&mut app);
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_paused = true;
    app.update();
    assert!(!app
        .world()
        .resource::<BattleSession>()
        .frames
        .presented()
        .bubbles
        .iter()
        .any(|bubble| bubble.entity_id == victim));
    assert!(!app
        .world()
        .resource::<client::RenderEntityMap>()
        .0
        .contains_key(&victim));
    Ok(())
}

#[test]
fn profiler_positive_control_exposes_an_actual_escaping_allocation() {
    let allocation = allocation_positive_control();
    assert_eq!(allocation.len(), 4096);
    assert_eq!(allocation[4095], 17);
}
