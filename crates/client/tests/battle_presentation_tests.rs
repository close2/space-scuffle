//! Checks truthful participant, roster and selected-actor UI bindings using the
//! real map/catalog loader, including sparse owners and changed entity counts.

use bevy::prelude::*;
use client::battle_presentation::{refresh_battle_presentation, BattlePresentation};
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use sim_types::PlayerId;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn definition() -> Result<BattleDefinition, client::runtime_battle::BattleLoadError> {
    BattleDefinition::load(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
        DEFAULT_BATTLE_SCENARIO.as_ref(),
    )
}

#[test]
fn actual_rosters_sparse_owners_and_retained_labels_follow_the_session() -> TestResult {
    let mut definition = definition()?;
    // Remap the copied declarative session before actual initialization, so the
    // sparse owner also receives its captured supply and valid start/initial orders.
    definition.scenario.players[1].player_slot = 3;
    for start in &mut definition.document.start_locations {
        if start.player_slot == 1 {
            start.player_slot = 3;
        }
    }
    for order in &mut definition.scenario.initial_orders {
        let client::runtime_battle::BattleInitialOrder::AttackMoveToStart {
            player_slot,
            target_player_slot,
        } = order;
        if *player_slot == 1 {
            *player_slot = 3;
        }
        if *target_player_slot == 1 {
            *target_player_slot = 3;
        }
    }
    let mut world = definition.instantiate()?;
    let total = world.entities.len();
    let title = definition.document.metadata.title.clone();
    let session = client::battle_pipeline::session::BattleSession::prepare(
        client::battle_pipeline::commands::SessionGeneration::FIRST,
        &mut world,
        client::battle_pipeline::session::CommittedBattleMetadata::from_definition(&definition)?,
    )?;
    let mut app = App::new();
    app.insert_resource(session.with_world(world))
        .insert_resource(definition)
        .insert_resource(client::LabState { ..default() })
        .init_resource::<BattlePresentation>()
        .add_event::<AppExit>()
        .add_systems(Update, refresh_battle_presentation);
    app.update();
    let presentation = app.world().resource::<BattlePresentation>();
    assert_eq!(presentation.map_title, title);
    assert_eq!(
        presentation
            .players
            .iter()
            .map(|player| player.owner)
            .collect::<Vec<_>>(),
        [PlayerId(0), PlayerId(3)]
    );
    assert_eq!(presentation.target, Some(PlayerId(3)));
    assert_eq!(
        presentation
            .players
            .iter()
            .flat_map(|player| &player.roster)
            .map(|entry| entry.count)
            .sum::<usize>(),
        total
    );
    let labels: Vec<_> = presentation
        .players
        .iter()
        .flat_map(|player| &player.roster)
        .map(|entry| (entry.label.as_ptr(), entry.label.clone()))
        .collect();
    app.update();
    for (entry, (pointer, text)) in app
        .world()
        .resource::<BattlePresentation>()
        .players
        .iter()
        .flat_map(|player| &player.roster)
        .zip(labels)
    {
        assert_eq!(
            entry.label.as_ptr(),
            pointer,
            "unchanged labels retain storage"
        );
        assert_eq!(entry.label, text);
    }
    let mut session = app
        .world_mut()
        .resource_mut::<client::battle_pipeline::session::BattleSession>();
    let world = session.world_mut().ok_or("world absent")?;
    let victim = *world.entities.keys().next().ok_or("empty roster")?;
    world.entities.get_mut(&victim).ok_or("victim absent")?.hp = sim_types::SimScalar::ZERO;
    world.reap_dead_and_emit_shatter();
    world.validate_supply_ledger()?;
    world.current_tick += 1;
    app.update();
    assert_eq!(
        app.world()
            .resource::<BattlePresentation>()
            .players
            .iter()
            .flat_map(|player| &player.roster)
            .map(|entry| entry.count)
            .sum::<usize>(),
        total - 1
    );
    Ok(())
}

#[test]
fn alternate_real_loadout_changes_the_ui_roster_without_unit_constants() -> TestResult {
    let mut definition = definition()?;
    let alternative = definition
        .loadouts
        .loadouts
        .iter()
        .find(|loadout| loadout.slug != definition.scenario.players[0].loadout_slug)
        .ok_or("no alternate loadout")?
        .slug
        .clone();
    definition.scenario.players[0].loadout_slug = alternative;
    // A starting-worker loadout has no attack-move capability; this scenario
    // intentionally tests its idle roster rather than an invalid combat order.
    definition.scenario.initial_orders.clear();
    let mut world = definition.instantiate()?;
    let expected = world
        .entities
        .values()
        .filter(|entity| entity.owner == PlayerId(0))
        .count();
    let session = client::battle_pipeline::session::BattleSession::prepare(
        client::battle_pipeline::commands::SessionGeneration::FIRST,
        &mut world,
        client::battle_pipeline::session::CommittedBattleMetadata::from_definition(&definition)?,
    )?;
    let mut app = App::new();
    app.insert_resource(session.with_world(world))
        .insert_resource(definition)
        .insert_resource(client::LabState { ..default() })
        .init_resource::<BattlePresentation>()
        .add_event::<AppExit>()
        .add_systems(Update, refresh_battle_presentation);
    app.update();
    let presentation = app.world().resource::<BattlePresentation>();
    assert_eq!(
        presentation.players[0]
            .roster
            .iter()
            .map(|entry| entry.count)
            .sum::<usize>(),
        expected
    );
    let world = app
        .world()
        .resource::<client::battle_pipeline::session::BattleSession>()
        .world()
        .ok_or("world absent")?;
    for entry in &presentation.players[0].roster {
        assert_eq!(entry.name, world.catalog.get(entry.unit_type)?.display_name);
    }
    Ok(())
}

#[test]
fn selected_actor_identity_matches_the_actual_entity_after_selection_changes() -> TestResult {
    let definition = definition()?;
    let mut world = definition.instantiate()?;
    let actors: Vec<_> = world
        .catalog
        .archetypes()
        .filter(|actor| actor.showcase_hero)
        .take(2)
        .map(|actor| (actor.unit_type_id, actor.slug.clone()))
        .collect();
    assert_eq!(actors.len(), 2);
    let mut app = App::new();
    app.insert_resource(client::CatalogResource(world.catalog.clone()))
        .init_resource::<client::LabState>()
        .init_resource::<client::SelectionState>()
        .init_resource::<client::battle_selection::BattleSelection>()
        .add_systems(Update, client::lab_ui::sync_selected_actor_identity);
    for (index, (unit_type, _)) in actors.iter().enumerate() {
        let id = world.spawn_unit(*unit_type, PlayerId(0), sim_types::SimVec2::ZERO)?;
        let instance = world
            .build_render_snapshot()
            .bubbles
            .into_iter()
            .find(|bubble| bubble.entity_id == id)
            .ok_or("snapshot actor absent")?;
        app.world_mut().spawn((
            client::BubbleInstance(instance),
            client::HeroLineupIndex(index),
        ));
    }
    for (index, (unit_type, slug)) in actors.iter().enumerate() {
        app.world_mut()
            .resource_mut::<client::SelectionState>()
            .selected_hero_index = Some(index);
        app.update();
        assert_eq!(
            &app.world().resource::<client::LabState>().archetype_name,
            slug
        );
        let entity = app
            .world()
            .resource::<client::SelectionState>()
            .selected_entity
            .ok_or("selection absent")?;
        assert_eq!(
            app.world()
                .get::<client::BubbleInstance>(entity)
                .ok_or("actor absent")?
                .0
                .unit_type,
            *unit_type
        );
    }
    Ok(())
}

#[test]
fn candidate_definition_changes_do_not_relabel_committed_generation() -> TestResult {
    let definition = definition()?;
    let mut simulation = definition.instantiate()?;
    let session = client::battle_pipeline::session::BattleSession::prepare(
        client::battle_pipeline::commands::SessionGeneration::FIRST,
        &mut simulation,
        client::battle_pipeline::session::CommittedBattleMetadata::from_definition(&definition)?,
    )?;
    let title = session.metadata.map_title.clone();
    let labels: Vec<_> = session
        .metadata
        .players
        .iter()
        .map(|player| player.label.clone())
        .collect();
    let mut app = App::new();
    app.insert_resource(session.with_world(simulation))
        .insert_resource(definition)
        .insert_resource(client::LabState { ..default() })
        .init_resource::<BattlePresentation>()
        .add_event::<AppExit>()
        .add_systems(Update, refresh_battle_presentation);
    app.update();
    app.world_mut()
        .resource_mut::<BattleDefinition>()
        .document
        .metadata
        .title = "Uncommitted candidate title".into();
    app.world_mut()
        .resource_mut::<BattleDefinition>()
        .scenario
        .players
        .clear();
    app.update();
    let presentation = app.world().resource::<BattlePresentation>();
    assert_eq!(presentation.map_title, title);
    assert_eq!(
        presentation
            .players
            .iter()
            .map(|player| player.label.clone())
            .collect::<Vec<_>>(),
        labels
    );
    Ok(())
}
