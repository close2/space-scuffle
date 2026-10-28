//! Actual client simulation synchronization must surface typed runtime failures.
//!
//! This exercises the real step/sync system with loaded scenario/catalog/assets;
//! invalid authoritative buff state must stop the match and emit application error.

use bevy::app::AppExit;
use bevy::prelude::*;
use client::bubble_shader::{setup_bubble_assets_system, BubbleDomeMaterial};
use client::indicators::{setup_indicator_assets_system, SelectionRingMaterial};
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use client::vfx::CombatVisualEvent;
use sim_core::ActiveBuff;
use sim_types::SimScalar;

/// Loads the same catalog-derived palette dependencies required by normal client startup.
fn actual_definition_and_palette() -> Result<
    (
        client::ResolvedAssetsRoot,
        BattleDefinition,
        client::player_presentation::PlayerPalette,
    ),
    Box<dyn std::error::Error>,
> {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let root = client::ResolvedAssetsRoot::from_directory(&assets)?;
    let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let palette = client::player_presentation::PlayerPalette::load(
        root.as_path(),
        definition
            .catalogs
            .units
            .archetypes()
            .filter(|archetype| archetype.showcase_hero)
            .count(),
    )?;
    Ok((root, definition, palette))
}

#[test]
fn live_sync_reports_and_stops_on_invalid_simulation_rate() -> Result<(), Box<dyn std::error::Error>>
{
    let (root, definition, palette) = actual_definition_and_palette()?;
    let mut simulation = definition.instantiate()?;
    let invalid_entity = simulation
        .entities
        .values()
        .find(|entity| {
            simulation
                .catalog
                .get(entity.unit_type_id)
                .is_ok_and(|archetype| {
                    archetype.weapon.is_some() || archetype.ground_weapon.is_some()
                })
        })
        .ok_or("real battle has no armed entity")?
        .id;
    simulation
        .entities
        .get_mut(&invalid_entity)
        .ok_or("armed entity missing")?
        .active_buffs
        .push(ActiveBuff {
            buff_id: 42,
            remaining_ticks: 100,
            speed_multiplier: SimScalar::ONE,
            attack_rate_multiplier: SimScalar::ZERO,
        });
    let mut session = client::battle_pipeline::session::BattleSession::prepare(
        client::battle_pipeline::commands::SessionGeneration::FIRST,
        &mut simulation,
        client::battle_pipeline::session::CommittedBattleMetadata::from_definition(&definition)?,
    )?;
    session
        .clock
        .observe(std::time::Duration::from_millis(80), 1)?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_event::<AppExit>()
        .add_event::<CombatVisualEvent>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<BubbleDomeMaterial>>()
        .init_resource::<Assets<SelectionRingMaterial>>()
        .init_resource::<client::RenderEntityMap>()
        .insert_resource(session)
        .insert_resource(palette)
        .init_resource::<client::player_presentation::PresentationStatus>()
        .insert_resource(root)
        .insert_resource(client::CatalogResource(definition.catalogs.units.clone()))
        .insert_resource(client::LabState {
            current_tab: client::LabTab::LiveBattle,
            sim_world: Some(simulation),
            battle_paused: false,
            simulation_speed: 1,
            ..default()
        })
        .add_systems(
            Startup,
            (setup_bubble_assets_system, setup_indicator_assets_system),
        )
        .add_systems(
            Update,
            (
                client::battle_pipeline::driver::advance_battle_ticks,
                client::sync_live_battle_system,
                client::battle_pipeline::driver::publish_combat_events,
            )
                .chain(),
        );
    let mut exits = app.world().resource::<Events<AppExit>>().get_cursor();
    app.update();
    assert!(exits
        .read(app.world().resource::<Events<AppExit>>())
        .any(|event| matches!(event, AppExit::Error(_))));
    let lab = app.world().resource::<client::LabState>();
    assert!(lab.battle_paused);
    assert_eq!(
        app.world()
            .resource::<client::battle_pipeline::session::BattleSession>()
            .clock
            .debt(),
        std::time::Duration::ZERO
    );
    let message = lab
        .battle_failure
        .as_ref()
        .ok_or("failure evidence not retained for UI")?;
    assert!(message.contains("buff index 0 (ID 42)"));
    assert!(message.contains("raw value 0"));
    let simulation = lab.sim_world.as_ref().ok_or("world missing")?;
    assert_eq!(
        simulation.current_tick, 1,
        "failure must stop catch-up after its first tick"
    );
    assert!(
        simulation.diagnostics().is_empty(),
        "normal client must consume error evidence"
    );
    Ok(())
}
