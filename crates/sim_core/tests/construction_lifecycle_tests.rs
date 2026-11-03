//! Structure construction through `PlayerCommand`: admission, payment, raising and loss.
//!
//! # Responsibility
//! Checks the rules documented in `production::construction` on the public tick API, using
//! shipped catalog definitions on a flat map.
//!
//! # Invariants
//! Setup may place entities and set a bank directly. The behaviour under test is reached only
//! through commands.

use sim_core::economy::{GameCommandError, GameCommandReceipt, TaskEventKind};
use sim_core::production::construction::{BuildRejection, CONSTRUCTION_CANCEL_REFUND_PERCENT};
use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CliffLevel, GameMap, MapCell, ResourceKind, ResourceNode};
use sim_types::{
    AnimState, EntityId, PlayerCommand, PlayerId, PlayerSlot, ShapeKind, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder, UnitTypeId,
};
use std::error::Error;

const ME: PlayerId = PlayerId(0);
const THEM: PlayerId = PlayerId(1);

type TestResult = Result<(), Box<dyn Error>>;

fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

fn id_of(world: &SimWorld, slug: &str) -> Result<UnitTypeId, Box<dyn Error>> {
    Ok(world.catalog.id_by_slug(slug).ok_or(slug.to_owned())?)
}

/// The first harvester-capable builder archetype in the catalog.
fn builder_type(world: &SimWorld) -> Result<UnitTypeId, Box<dyn Error>> {
    Ok(world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.can_construct)
        .map(|a| a.unit_type_id)
        .ok_or("no builder in the catalog")?)
}

/// A flat world with one worker, its hall, and a gas node at cell (20, 20).
fn fixture() -> Result<(SimWorld, EntityId, UnitTypeId), Box<dyn Error>> {
    let map = GameMap::from_cells(
        40,
        40,
        vec![MapCell::default_ground(CliffLevel(0)); 1600],
        vec![ResourceNode {
            pos: at(20, 20),
            kind: ResourceKind::Gas,
            remaining: 500,
            cell: (20, 20),
        }],
        Vec::new(),
    )?;
    let mut world = sim_fixtures::world(map, 7);
    let builder = builder_type(&world)?;
    let hall = id_of(&world, "foundry_core_dome")?;
    world.spawn_structure(hall, ME, 2, 2)?;
    let worker = world.spawn_unit(builder, ME, at(12, 12))?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = 1000;
    world.player_resources[PlayerSlot::ALL[0]].gas = 100;
    let dome = id_of(&world, "capacitor_dome")?;
    Ok((world, worker, dome))
}

fn order(world: &mut SimWorld, player: PlayerId, entities: Vec<EntityId>, order: UnitOrder) {
    let tick = world.current_tick;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands: vec![PlayerCommand {
            player_id: player,
            entities,
            order,
            queue: false,
        }],
        state_checksum: None,
    });
}

fn idle(world: &mut SimWorld, ticks: u32) {
    for _ in 0..ticks {
        let tick = world.current_tick;
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
        assert_eq!(world.diagnostics(), []);
    }
}

fn foundation_of(world: &SimWorld, unit_type: UnitTypeId) -> Option<EntityId> {
    world
        .entities
        .values()
        .find(|e| e.unit_type_id == unit_type)
        .map(|e| e.id)
}

fn build(unit_type: UnitTypeId, x: u32, y: u32) -> UnitOrder {
    UnitOrder::Build {
        structure_type: unit_type,
        cell_x: x,
        cell_y: y,
    }
}

fn walk_until_laid(
    world: &mut SimWorld,
    unit_type: UnitTypeId,
) -> Result<EntityId, Box<dyn Error>> {
    for _ in 0..600 {
        if let Some(id) = foundation_of(world, unit_type) {
            return Ok(id);
        }
        idle(world, 1);
    }
    Err("builder never laid the foundation".into())
}

fn cost(world: &SimWorld, unit_type: UnitTypeId) -> Result<UnitArchetype, Box<dyn Error>> {
    Ok(world.catalog.get(unit_type)?.clone())
}

#[test]
fn build_is_neither_free_nor_instant() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    let price = cost(&world, dome)?;
    let (minerals, gas) = (
        world.player_resources[PlayerSlot::ALL[0]].minerals,
        world.player_resources[PlayerSlot::ALL[0]].gas,
    );
    let (x, y) = (24, 12);

    order(&mut world, ME, vec![worker], build(dome, x, y));
    assert!(matches!(
        world.game_command_outcomes()[0].result,
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    ));
    assert_eq!(
        foundation_of(&world, dome),
        None,
        "the builder must walk first"
    );
    assert_eq!(
        world.player_resources[PlayerSlot::ALL[0]].minerals,
        minerals,
        "paid only on arrival"
    );

    let site = walk_until_laid(&mut world, dome)?;
    assert_eq!(
        world.player_resources[PlayerSlot::ALL[0]].minerals,
        minerals - price.mineral_cost
    );
    assert_eq!(
        world.player_resources[PlayerSlot::ALL[0]].gas,
        gas - price.gas_cost
    );

    let entity = &world.entities[&site];
    assert!(!entity.is_completed);
    assert_eq!(entity.captured_supply_provided, 0);
    assert_eq!(entity.hp, entity.max_hp / SimScalar::from_num(10));
    assert_eq!(entity.current_anim, AnimState::Constructing);
    let supply = world.player_resources[PlayerSlot::ALL[0]].supply_max;
    world.validate_supply_ledger()?;

    let ticks = price.build_time_ticks;
    idle(&mut world, ticks + 5);
    let entity = &world.entities[&site];
    assert!(entity.is_completed);
    assert!(entity.construction.is_none());
    assert_eq!(entity.hp, entity.max_hp);
    assert_eq!(entity.current_anim, AnimState::Idle);
    assert_eq!(
        world.player_resources[PlayerSlot::ALL[0]].supply_max,
        supply + price.supply_provided
    );
    assert!(world.entities[&worker].order_queue.is_empty());
    world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn progress_needs_exactly_the_catalog_build_time() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    let total = world.catalog.get(dome)?.build_time_ticks;
    order(&mut world, ME, vec![worker], build(dome, 14, 12));
    let site = walk_until_laid(&mut world, dome)?;
    let mut worked = 0;
    while !world.entities[&site].is_completed {
        let before = world.entities[&site]
            .construction
            .ok_or("record")?
            .remaining_ticks;
        idle(&mut world, 1);
        let after = world.entities[&site]
            .construction
            .map_or(0, |record| record.remaining_ticks);
        assert!(before - after <= 1, "one worked tick per tick");
        worked += before - after;
        assert!(world.current_tick < 5000);
    }
    assert_eq!(worked, total);
    Ok(())
}

#[test]
fn rejections_change_nothing() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    let price = cost(&world, dome)?;
    let locked = world
        .catalog
        .archetypes()
        .find(|a| a.slug == "fusion_reactor_dome")
        .map(|a| a.unit_type_id)
        .ok_or("shipped structure")?;
    let unit = builder_type(&world)?;
    let enemy = world.spawn_unit(unit, THEM, at(30, 30))?;
    let mute = world.spawn_unit(id_of(&world, "pulse_bot")?, ME, at(30, 12))?;
    let cases: [(Vec<EntityId>, UnitOrder, PlayerId, u32); 7] = [
        (vec![], build(dome, 14, 14), ME, 1000),
        (vec![enemy], build(dome, 14, 14), ME, 1000),
        (vec![mute], build(dome, 14, 14), ME, 1000),
        (vec![worker], build(dome, 39, 39), ME, 1000),
        (vec![worker], build(dome, u32::MAX, 0), ME, 1000),
        (vec![worker], build(locked, 14, 14), ME, 100_000),
        (
            vec![worker],
            build(dome, 14, 14),
            ME,
            price.mineral_cost - 1,
        ),
    ];
    for (selection, command, player, bank) in cases {
        world.player_resources[PlayerSlot::ALL[0]].minerals = bank;
        let entities = world.entities.clone();
        let economy = world.player_resources;
        order(&mut world, player, selection, command);
        assert!(world.game_command_outcomes()[0].result.is_err());
        assert_eq!(world.player_resources, economy);
        assert_eq!(
            world.entities.keys().collect::<Vec<_>>(),
            entities.keys().collect::<Vec<_>>()
        );
        assert!(world.entities[&worker].order_queue.is_empty());
    }
    Ok(())
}

#[test]
fn typed_rejections_name_the_cause() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = 0;
    order(&mut world, ME, vec![worker], build(dome, 14, 14));
    assert!(matches!(
        &world.game_command_outcomes()[0].result,
        Err(GameCommandError::Build(BuildRejection::Minerals {
            available: 0,
            ..
        }))
    ));

    world.player_resources[PlayerSlot::ALL[0]].minerals = 1000;
    let unit = world.spawn_unit(id_of(&world, "pulse_bot")?, ME, at(30, 12))?;
    order(&mut world, ME, vec![unit], build(dome, 14, 14));
    assert_eq!(
        world.game_command_outcomes()[0].result,
        Err(GameCommandError::NoCapableBuilder)
    );

    let bay = id_of(&world, "machining_plant_dome")?;
    let prerequisite = id_of(&world, "assembly_bay_dome")?;
    order(&mut world, ME, vec![worker], build(bay, 14, 14));
    assert_eq!(
        world.game_command_outcomes()[0].result,
        Err(GameCommandError::Build(
            BuildRejection::MissingPrerequisite(prerequisite)
        ))
    );
    Ok(())
}

#[test]
fn the_money_is_checked_again_on_arrival() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    order(&mut world, ME, vec![worker], build(dome, 30, 30));
    world.player_resources[PlayerSlot::ALL[0]].minerals = 0;
    for _ in 0..600 {
        idle(&mut world, 1);
        if let Some(event) = world.task_events().first() {
            assert!(matches!(
                event.kind,
                TaskEventKind::BuildAbandoned(BuildRejection::Minerals { .. })
            ));
            assert_eq!(foundation_of(&world, dome), None);
            assert_eq!(world.player_resources[PlayerSlot::ALL[0]].minerals, 0);
            return Ok(());
        }
    }
    Err("order never ended".into())
}

#[test]
fn a_site_is_refused_if_it_buries_a_node_and_an_extractor_needs_gas() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    let extractor = id_of(&world, "plasma_siphon_dome")?;
    order(&mut world, ME, vec![worker], build(dome, 20, 20));
    assert_eq!(
        world.game_command_outcomes()[0].result,
        Err(GameCommandError::Build(BuildRejection::CoversResource))
    );
    order(&mut world, ME, vec![worker], build(extractor, 25, 25));
    assert_eq!(
        world.game_command_outcomes()[0].result,
        Err(GameCommandError::Build(BuildRejection::ExtractorNeedsGas))
    );
    order(&mut world, ME, vec![worker], build(extractor, 19, 19));
    assert!(world.game_command_outcomes()[0].result.is_ok());
    Ok(())
}

#[test]
fn the_builder_steps_off_a_site_it_stands_on() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    order(&mut world, ME, vec![worker], build(dome, 12, 12));
    let site = walk_until_laid(&mut world, dome)?;
    let (cx, cy) = world.map.world_to_cell(world.entities[&worker].position)?;
    let cell = world.map.cell(cx, cy)?;
    assert_ne!(
        cell.occupying_structure,
        Some(site),
        "worker must not be buried"
    );
    Ok(())
}

#[test]
fn a_blocked_site_waits_then_gives_up() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    let squatter = world.spawn_unit(id_of(&world, "pulse_bot")?, ME, at(25, 12))?;
    order(&mut world, ME, vec![squatter], UnitOrder::HoldPosition);
    order(&mut world, ME, vec![worker], build(dome, 25, 12));
    for _ in 0..600 {
        idle(&mut world, 1);
        if world
            .task_events()
            .iter()
            .any(|event| event.kind == TaskEventKind::SiteObstructed)
        {
            assert_eq!(foundation_of(&world, dome), None);
            return Ok(());
        }
    }
    Err("order never ended".into())
}

#[test]
fn losing_the_builder_pauses_and_another_can_resume() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    let total = world.catalog.get(dome)?.build_time_ticks;
    let second = world.spawn_unit(builder_type(&world)?, ME, at(10, 20))?;
    order(&mut world, ME, vec![worker], build(dome, 14, 12));
    let site = walk_until_laid(&mut world, dome)?;
    idle(&mut world, 30);

    // Called away: the builder leaves, and progress stops where it stood.
    let moved = UnitOrder::Move { target: at(30, 30) };
    order(&mut world, ME, vec![worker], moved);
    idle(&mut world, 10);
    let paused = world.entities[&site]
        .construction
        .ok_or("record")?
        .remaining_ticks;
    idle(&mut world, 100);
    assert_eq!(
        world.entities[&site]
            .construction
            .ok_or("record")?
            .remaining_ticks,
        paused
    );
    assert!(paused < total);
    assert!(world.entities[&site].is_alive());

    order(
        &mut world,
        ME,
        vec![second],
        UnitOrder::Construct { foundation: site },
    );
    assert!(world.game_command_outcomes()[0].result.is_ok());
    idle(&mut world, paused + 300);
    assert!(world.entities[&site].is_completed);
    Ok(())
}

#[test]
fn a_task_event_names_its_owner_and_drains_once() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    order(&mut world, ME, vec![worker], build(dome, 14, 12));
    let site = walk_until_laid(&mut world, dome)?;
    idle(&mut world, 5);
    order(&mut world, ME, vec![site], UnitOrder::CancelConstruction);
    let mut events: Vec<_> = world.drain_task_events().collect();
    for _ in 0..5 {
        idle(&mut world, 1);
        events.extend(world.drain_task_events());
    }
    assert_eq!(world.task_events(), []);
    let [event] = events.as_slice() else {
        return Err(format!("expected the builder's one event, got {events:?}").into());
    };
    assert_eq!(
        (event.entity, event.owner, &event.kind),
        (worker, ME, &TaskEventKind::FoundationLost)
    );
    Ok(())
}

#[test]
fn a_dead_builder_leaves_the_foundation_standing() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    order(&mut world, ME, vec![worker], build(dome, 14, 12));
    let site = walk_until_laid(&mut world, dome)?;
    idle(&mut world, 5);
    world.entities.get_mut(&worker).ok_or("worker")?.hp = SimScalar::ZERO;
    idle(&mut world, 2);
    let left = world.entities[&site]
        .construction
        .ok_or("record")?
        .remaining_ticks;
    idle(&mut world, 50);
    assert_eq!(
        world.entities[&site]
            .construction
            .ok_or("record")?
            .remaining_ticks,
        left
    );
    Ok(())
}

#[test]
fn cancelling_refunds_the_documented_share_and_clears_the_site() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    let price = cost(&world, dome)?;
    order(&mut world, ME, vec![worker], build(dome, 14, 12));
    let site = walk_until_laid(&mut world, dome)?;
    let bank = world.player_resources[PlayerSlot::ALL[0]].minerals;
    order(&mut world, ME, vec![site], UnitOrder::CancelConstruction);
    let refund = price.mineral_cost * CONSTRUCTION_CANCEL_REFUND_PERCENT / 100;
    assert_eq!(
        world.game_command_outcomes()[0].result,
        Ok(GameCommandReceipt::Refunded {
            minerals: refund,
            gas: price.gas_cost * CONSTRUCTION_CANCEL_REFUND_PERCENT / 100,
        })
    );
    assert_eq!(
        world.player_resources[PlayerSlot::ALL[0]].minerals,
        bank + refund
    );
    assert!(!world.entities.contains_key(&site));
    assert!(!world.map.structures().contains_key(&site));
    assert!(world.map.can_place_building(14, 12, 2, 2).is_ok());
    idle(&mut world, 2);
    assert!(world.entities[&worker].order_queue.is_empty());
    world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn cancel_is_all_or_nothing_and_only_for_foundations() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    let hall = world
        .entities
        .values()
        .find(|e| {
            matches!(
                world.catalog.get(e.unit_type_id).map(|a| a.shape),
                Ok(ShapeKind::BuildingDome { .. })
            )
        })
        .map(|e| e.id)
        .ok_or("hall")?;
    order(&mut world, ME, vec![worker], build(dome, 14, 12));
    let site = walk_until_laid(&mut world, dome)?;
    let bank = world.player_resources[PlayerSlot::ALL[0]];
    order(
        &mut world,
        ME,
        vec![site, hall],
        UnitOrder::CancelConstruction,
    );
    assert_eq!(
        world.game_command_outcomes()[0].result,
        Err(GameCommandError::NotAFoundation(hall))
    );
    assert_eq!(world.player_resources[PlayerSlot::ALL[0]], bank);
    assert!(world.entities.contains_key(&site));
    Ok(())
}

#[test]
fn a_foundation_can_be_destroyed_for_no_refund_and_does_not_fight() -> TestResult {
    let (mut world, worker, _) = fixture()?;
    let turret = world
        .catalog
        .archetypes()
        .find(|a| {
            matches!(a.shape, ShapeKind::BuildingDome { .. })
                && a.ground_weapon.is_some()
                && a.requires_structures.is_empty()
                && Some(a.faction) == world_faction(&world)
        })
        .map(|a| a.unit_type_id);
    let dome = turret.unwrap_or(id_of(&world, "capacitor_dome")?);
    world.catalog.get_mut(dome)?.requires_structures.clear();
    order(&mut world, ME, vec![worker], build(dome, 14, 12));
    let site = walk_until_laid(&mut world, dome)?;
    let enemy = world.spawn_unit(id_of(&world, "pulse_bot")?, THEM, at(17, 12))?;
    let target = world.entities[&enemy].hp;
    idle(&mut world, 40);
    assert_eq!(
        world.entities[&enemy].hp, target,
        "a foundation has no weapons yet"
    );
    assert!(world.entities[&site].hp < world.entities[&site].max_hp);
    let bank = world.player_resources[PlayerSlot::ALL[0]];
    world.entities.get_mut(&site).ok_or("site")?.hp = SimScalar::ZERO;
    idle(&mut world, 2);
    assert!(!world.entities.contains_key(&site));
    assert_eq!(
        world.player_resources[PlayerSlot::ALL[0]].minerals,
        bank.minerals
    );
    assert!(!matches!(
        world.entities[&worker].order_queue.front(),
        Some(UnitOrder::Construct { .. })
    ));
    Ok(())
}

/// The faction of the catalog's first builder, if it has one.
fn world_faction(world: &SimWorld) -> Option<sim_types::FactionId> {
    world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.can_construct)
        .map(|a| a.faction)
}

#[test]
fn snapshots_publish_construction_progress() -> TestResult {
    let (mut world, worker, dome) = fixture()?;
    order(&mut world, ME, vec![worker], build(dome, 14, 12));
    let site = walk_until_laid(&mut world, dome)?;
    let total = world.catalog.get(dome)?.build_time_ticks;
    idle(&mut world, total / 2);
    let snapshot = world.build_render_snapshot();
    let bubble = snapshot
        .bubbles
        .iter()
        .find(|bubble| bubble.entity_id == site)
        .ok_or("foundation is published")?;
    let progress = bubble
        .build_or_queue_progress
        .ok_or("progress is published")?;
    assert!((0.4..=0.6).contains(&progress), "{progress}");
    assert_eq!(bubble.anim_state, AnimState::Constructing);
    idle(&mut world, total);
    let snapshot = world.build_render_snapshot();
    let bubble = snapshot
        .bubbles
        .iter()
        .find(|bubble| bubble.entity_id == site)
        .ok_or("building is published")?;
    assert_eq!(bubble.build_or_queue_progress, None);
    assert_eq!(bubble.anim_state, AnimState::Idle);
    Ok(())
}
