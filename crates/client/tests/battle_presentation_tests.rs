//! Checks truthful participant, roster and selected-actor UI bindings using the
//! real map/catalog loader, including sparse owners and changed entity counts.

use bevy::prelude::*;
use client::battle_presentation::{refresh_battle_presentation, BattlePresentation};
use client::runtime_battle::{BattleDefinition, BattleInitialOrder, DEFAULT_BATTLE_SCENARIO};
use sim_types::PlayerId;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn definition() -> Result<BattleDefinition, client::runtime_battle::BattleLoadError> {
    BattleDefinition::load(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
        DEFAULT_BATTLE_SCENARIO.as_ref(),
    )
}

/// Moves every slot `from` that `orders` name to `to`: the commanded slot of each order, and the
/// targeted slot of a start order. A cell order's target is a map cell, which no slot remap moves.
fn remap_order_slots(orders: &mut [BattleInitialOrder], from: u8, to: u8) {
    let remap = |slot: &mut u8| {
        if *slot == from {
            *slot = to;
        }
    };
    for order in orders {
        match order {
            BattleInitialOrder::AttackMoveToStart {
                player_slot,
                target_player_slot,
            } => {
                remap(player_slot);
                remap(target_player_slot);
            }
            BattleInitialOrder::AttackMoveToCell { player_slot, .. }
            | BattleInitialOrder::EnterModes { player_slot } => remap(player_slot),
        }
    }
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
    remap_order_slots(&mut definition.scenario.initial_orders, 1, 3);
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

/// One tick of `world` and the snapshot it publishes.
fn tick(world: &mut sim_core::SimWorld, tick: &mut u32) -> sim_types::snapshot::RenderSnapshot {
    *tick += 1;
    let mut out = sim_types::snapshot::RenderSnapshot::default();
    world.step_into(
        &sim_types::TickBundle {
            tick: sim_types::Tick(*tick),
            commands: Vec::new(),
            state_checksum: None,
        },
        &mut out,
    );
    out
}

/// The death looks the client draws for `out`'s events of units of `kind`: the spawn the
/// effect system asks for each `BubbleShatter` (the unit's `death` effect), or the legacy
/// shatter where none replaces it. Each is one death drawn.
fn deaths_drawn(
    out: &sim_types::snapshot::RenderSnapshot,
    kind: sim_types::UnitTypeId,
    tables: &client::effects::catalog::UnitEffectTables,
    effects: &client::effects::catalog::EffectCatalog,
) -> usize {
    use sim_types::snapshot::CombatEventVisual;
    out.combat_events
        .iter()
        .filter(|event| {
            matches!(event, CombatEventVisual::BubbleShatter { unit_type, .. } if *unit_type == kind)
        })
        .filter(|event| {
            client::effects::systems::spawn_request(event, tables, effects).is_some_and(|request| {
                Some(request.effect) == tables.death(kind)
            }) || !client::effects::systems::replaces_shatter(Some(tables), kind)
        })
        .count()
}

/// H-19 (plan `2026-11-28-carrier-interceptors` D-11 item 3): a hangar's member that docks
/// leaves the snapshot with no death drawn, while a member that is killed draws its death. The
/// shipped hangar unit, found by property, launches at a passive air enemy; the enemy is
/// removed, the members fly home and dock; then a relaunched member is killed.
#[test]
#[allow(clippy::too_many_lines, reason = "one scenario, step by step")]
fn a_docking_member_draws_no_death_and_a_killed_one_does() -> TestResult {
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{MoverType, SimScalar, SimVec2, UnitOrder};
    let root = client::ResolvedAssetsRoot::from_directory(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let effects = client::effects::catalog::EffectCatalog::load(
        &root
            .as_path()
            .join(client::effects::catalog::EFFECTS_CATALOG_PATH),
        root.as_path(),
    )?;
    let catalogs = sim_fixtures::shipped_catalogs();
    let tables = client::effects::catalog::UnitEffectTables::build(&catalogs.units, &effects)?;
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world =
        sim_core::SimWorld::with_catalogs(map, 0x19, catalogs.units, catalogs.upgrades)?;
    let (host, member) = world
        .catalog
        .archetypes()
        .find_map(|arch| {
            arch.capabilities
                .hangar
                .map(|hangar| (arch.unit_type_id, hangar.member))
        })
        .ok_or("the shipped catalog has a hangar unit")?;
    assert!(
        tables.death(member).is_some()
            || !client::effects::systems::replaces_shatter(Some(&tables), member),
        "a member's death is drawn"
    );
    let mut air: Vec<_> = world
        .catalog
        .archetypes()
        .filter(|arch| arch.mover_type == MoverType::Air && arch.max_speed > SimScalar::ZERO)
        .filter(|arch| arch.capabilities.hangar.is_none() && arch.selectable)
        .filter(|arch| arch.weapon.is_some() || arch.air_weapon.is_some())
        .map(|arch| arch.unit_type_id)
        .collect();
    air.sort();
    let target_kind = *air.first().ok_or("a plain air unit")?;
    {
        let arch = world.catalog.get_mut(target_kind)?;
        arch.weapon = None;
        arch.ground_weapon = None;
        arch.air_weapon = None;
        arch.max_hp = SimScalar::from_i32(5000);
    }
    let (me, foe) = (PlayerId(0), PlayerId(1));
    let carrier = world.spawn_unit(host, me, SimVec2::from_i32(20, 32))?;
    let mut target = world.spawn_unit(target_kind, foe, SimVec2::from_i32(26, 32))?;
    world
        .entities
        .get_mut(&carrier)
        .ok_or("carrier")?
        .issue_order(UnitOrder::attack(target), false)?;
    let linked = |world: &sim_core::SimWorld| -> Vec<sim_types::EntityId> {
        world
            .entities
            .values()
            .filter(|e| e.hangar_link().is_some_and(|link| link.hangar == carrier))
            .map(|e| e.id)
            .collect()
    };
    let mut now = 0;
    while linked(&world).len() < 4 && now < 200 {
        tick(&mut world, &mut now);
    }
    let launched = linked(&world);
    assert_eq!(launched.len(), 4, "the start members are launched");
    // The target dies (its own death is of another kind): every member flies home and docks,
    // leaving the snapshot undrawn.
    world.entities.get_mut(&target).ok_or("target")?.hp = SimScalar::ZERO;
    let mut docked_deaths = 0;
    let mut out = tick(&mut world, &mut now);
    for _ in 0..300 {
        docked_deaths += deaths_drawn(&out, member, &tables, &effects);
        if linked(&world).is_empty() {
            break;
        }
        out = tick(&mut world, &mut now);
    }
    assert!(linked(&world).is_empty(), "every member docked");
    assert_eq!(docked_deaths, 0, "a dock draws no death");
    assert!(
        out.bubbles
            .iter()
            .all(|bubble| !launched.contains(&bubble.entity_id)),
        "docked members leave the snapshot"
    );
    // A new target: a member relaunches and is killed, and its death is drawn.
    target = world.spawn_unit(target_kind, foe, SimVec2::from_i32(26, 32))?;
    world
        .entities
        .get_mut(&carrier)
        .ok_or("carrier")?
        .issue_order(UnitOrder::attack(target), false)?;
    while linked(&world).is_empty() && now < 1000 {
        tick(&mut world, &mut now);
    }
    let victim = *linked(&world).first().ok_or("a relaunched member")?;
    world.entities.get_mut(&victim).ok_or("victim")?.hp = SimScalar::ZERO;
    let out = tick(&mut world, &mut now);
    assert_eq!(
        deaths_drawn(&out, member, &tables, &effects),
        1,
        "a killed member's death is drawn"
    );
    assert!(!world.entities.contains_key(&victim));
    Ok(())
}

/// B-18 (plan `2026-11-28-broodlings` D-12 item 2): a unit a missile creates is an ordinary
/// unit in the snapshot, and its timed life's expiry draws its death as any death does. The
/// shipped unit whose weapon has an impact spawn, found by property, attacks a passive ground
/// enemy; its spawned units appear in the snapshot and each draws its death when its clock ends.
#[test]
fn a_spawned_units_expiry_draws_its_death() -> TestResult {
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{MoverType, SimScalar, SimVec2, UnitOrder};
    let root = client::ResolvedAssetsRoot::from_directory(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let effects = client::effects::catalog::EffectCatalog::load(
        &root
            .as_path()
            .join(client::effects::catalog::EFFECTS_CATALOG_PATH),
        root.as_path(),
    )?;
    let catalogs = sim_fixtures::shipped_catalogs();
    let tables = client::effects::catalog::UnitEffectTables::build(&catalogs.units, &effects)?;
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world =
        sim_core::SimWorld::with_catalogs(map, 0x18, catalogs.units, catalogs.upgrades)?;
    let (lord, spawned) = world
        .catalog
        .archetypes()
        .find_map(|arch| {
            arch.ground_weapon
                .as_ref()
                .and_then(|w| w.impact_spawn)
                .map(|spawn| (arch.unit_type_id, spawn.unit))
        })
        .ok_or("the shipped catalog has a spawning unit")?;
    let mut ground: Vec<_> = world
        .catalog
        .archetypes()
        .filter(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
        .filter(|arch| arch.unit_type_id != spawned && arch.weapon.is_some())
        .map(|arch| arch.unit_type_id)
        .collect();
    ground.sort();
    let target_kind = *ground.first().ok_or("a plain ground unit")?;
    {
        let arch = world.catalog.get_mut(target_kind)?;
        arch.weapon = None;
        arch.ground_weapon = None;
        arch.air_weapon = None;
        arch.max_hp = SimScalar::from_i32(100_000);
    }
    let (me, foe) = (PlayerId(0), PlayerId(1));
    let attacker = world.spawn_unit(lord, me, SimVec2::from_i32(20, 32))?;
    let target = world.spawn_unit(target_kind, foe, SimVec2::from_i32(25, 32))?;
    world
        .entities
        .get_mut(&attacker)
        .ok_or("attacker")?
        .issue_order(UnitOrder::attack(target), false)?;
    let mut now = 0;
    let mut created = std::collections::BTreeSet::new();
    let mut drawn_in_snapshot = false;
    let mut deaths = 0;
    for _ in 0..400 {
        let out = tick(&mut world, &mut now);
        for entity in world.entities.values() {
            if entity.unit_type_id == spawned {
                created.insert(entity.id);
            }
        }
        drawn_in_snapshot |= out
            .bubbles
            .iter()
            .any(|bubble| created.contains(&bubble.entity_id));
        deaths += deaths_drawn(&out, spawned, &tables, &effects);
    }
    assert!(created.len() >= 4, "units were created: {}", created.len());
    assert!(drawn_in_snapshot, "created units are drawn as units");
    let gone = created
        .iter()
        .filter(|id| !world.entities.contains_key(id))
        .count();
    assert!(gone >= 2, "clocks ran out: {gone}");
    assert_eq!(deaths, gone, "every expiry draws its death");
    Ok(())
}
