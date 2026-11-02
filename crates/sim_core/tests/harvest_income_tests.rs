//! The harvester state machine and income, through `PlayerCommand` on a flat map.
//!
//! # Responsibility
//! Checks the rules documented in `economy::harvest`: gather, carry, return, repeat, what
//! happens when a node or depot goes away, and how gas differs.
//!
//! # Invariants
//! Mechanics are reached by capability: the worker archetype is found by its harvester spec.
//! Setup may place entities directly; behaviour is driven only by commands.

use sim_core::economy::{GameCommandError, GameCommandReceipt, TaskEventKind};
use sim_core::{Cargo, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell, ResourceKind, ResourceNode};
use sim_types::{
    EntityId, HarvesterSpec, PlayerCommand, PlayerId, ResourceNodeId, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder, UnitTypeId,
};
use std::error::Error;

const ME: PlayerId = PlayerId(0);
const MINERAL: ResourceNodeId = ResourceNodeId(0);
const SECOND_MINERAL: ResourceNodeId = ResourceNodeId(1);
const GAS: ResourceNodeId = ResourceNodeId(2);

type TestResult = Result<(), Box<dyn Error>>;

fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

fn node(kind: ResourceKind, cell: (i32, i32), remaining: u32) -> ResourceNode {
    ResourceNode {
        pos: at(cell.0, cell.1),
        kind,
        remaining,
        cell: (cell.0.unsigned_abs(), cell.1.unsigned_abs()),
    }
}

struct Fixture {
    world: SimWorld,
    worker: EntityId,
    hall: EntityId,
    spec: HarvesterSpec,
}

fn fixture(mineral: u32) -> Result<Fixture, Box<dyn Error>> {
    let map = GameMap::from_cells(
        40,
        40,
        vec![MapCell::default_ground(CliffLevel(0)); 1600],
        vec![
            node(ResourceKind::Mineral, (12, 4), mineral),
            node(ResourceKind::Mineral, (13, 6), 20),
            node(ResourceKind::Gas, (20, 20), 100),
        ],
        Vec::new(),
    )?;
    let mut world = SimWorld::new(map, 11);
    let (worker_type, spec) = world
        .catalog
        .archetypes()
        .find_map(|a| a.capabilities.harvester.map(|spec| (a.unit_type_id, spec)))
        .ok_or("no harvester in the catalog")?;
    let hall_type = depot(&world)?;
    let hall = world.spawn_structure(hall_type, ME, 2, 2)?;
    let worker = world.spawn_unit(worker_type, ME, at(10, 4))?;
    world.player_resources[0].minerals = 0;
    world.player_resources[0].gas = 0;
    Ok(Fixture {
        world,
        worker,
        hall,
        spec,
    })
}

fn depot(world: &SimWorld) -> Result<UnitTypeId, Box<dyn Error>> {
    Ok(world
        .catalog
        .archetypes()
        .find(|a| a.is_resource_dropoff && a.requires_structures.is_empty())
        .map(|a| a.unit_type_id)
        .ok_or("no depot in the catalog")?)
}

impl Fixture {
    fn order(&mut self, entities: Vec<EntityId>, order: UnitOrder) {
        let tick = self.world.current_tick;
        self.world.step(&TickBundle {
            tick: Tick(tick),
            commands: vec![PlayerCommand {
                player_id: ME,
                entities,
                order,
                queue: false,
            }],
            state_checksum: None,
        });
    }

    fn idle(&mut self, ticks: u32) {
        for _ in 0..ticks {
            let tick = self.world.current_tick;
            self.world.step(&TickBundle {
                tick: Tick(tick),
                commands: Vec::new(),
                state_checksum: None,
            });
            assert_eq!(self.world.diagnostics(), []);
        }
    }

    fn minerals(&self) -> u32 {
        self.world.player_resources[0].minerals
    }

    fn remaining(&self, node: ResourceNodeId) -> u32 {
        self.world.map.resources()[node.0 as usize].remaining
    }

    fn until(&mut self, limit: u32, done: impl Fn(&Self) -> bool) -> TestResult {
        for _ in 0..limit {
            if done(self) {
                return Ok(());
            }
            self.idle(1);
        }
        Err("condition not reached in time".into())
    }
}

#[test]
fn harvesting_earns_income_and_draws_the_node_down() -> TestResult {
    let mut f = fixture(1000)?;
    let worker = f.worker;
    f.order(vec![worker], UnitOrder::Harvest { target: MINERAL });
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    );
    let capacity = f.spec.harvest_capacity;

    f.until(800, |f| f.world.entities[&f.worker].cargo.is_some())?;
    assert_eq!(f.minerals(), 0, "cargo is not income until delivered");
    assert_eq!(f.remaining(MINERAL), 1000 - capacity);
    assert_eq!(
        f.world.entities[&worker].cargo,
        Some(Cargo {
            kind: ResourceKind::Mineral,
            amount: capacity
        })
    );

    f.until(800, |f| f.minerals() > 0)?;
    assert_eq!(f.minerals(), capacity);
    assert_eq!(f.world.entities[&worker].cargo, None);

    f.until(1600, |f| f.minerals() >= 3 * capacity)?;
    assert_eq!(
        f.minerals()
            + f.remaining(MINERAL)
            + f.world.entities[&worker].cargo.map_or(0, |c| c.amount),
        1000,
        "resources are conserved"
    );
    Ok(())
}

#[test]
fn gathering_takes_the_authored_time() -> TestResult {
    let mut f = fixture(1000)?;
    let worker = f.worker;
    f.order(vec![worker], UnitOrder::Harvest { target: MINERAL });
    f.until(400, |f| f.world.entities[&f.worker].task.gather_ticks > 0)?;
    let started = f.world.current_tick;
    f.until(400, |f| f.world.entities[&f.worker].cargo.is_some())?;
    assert_eq!(
        f.world.current_tick - started,
        u32::from(f.spec.harvest_rate_ticks)
    );
    Ok(())
}

#[test]
fn only_harvesters_take_the_order_and_mixed_selections_work() -> TestResult {
    let mut f = fixture(1000)?;
    let soldier_type = f
        .world
        .catalog
        .id_by_slug("pulse_bot")
        .ok_or("shipped unit")?;
    let soldier = f.world.spawn_unit(soldier_type, ME, at(10, 8))?;
    f.order(vec![soldier], UnitOrder::Harvest { target: MINERAL });
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::NoCapableHarvester)
    );
    let worker = f.worker;
    f.order(
        vec![soldier, worker],
        UnitOrder::Harvest { target: MINERAL },
    );
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    );
    assert!(f.world.entities[&soldier].order_queue.is_empty());
    Ok(())
}

#[test]
fn unknown_and_exhausted_nodes_are_refused() -> TestResult {
    let mut f = fixture(1000)?;
    let worker = f.worker;
    let target = ResourceNodeId(99);
    f.order(vec![worker], UnitOrder::Harvest { target });
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::UnknownResourceNode(target))
    );
    f.world.map.resources_mut()[0].remaining = 0;
    f.order(vec![worker], UnitOrder::Harvest { target: MINERAL });
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::ResourceExhausted(MINERAL))
    );
    Ok(())
}

#[test]
fn an_exhausted_node_sends_workers_to_the_next() -> TestResult {
    let capacity = fixture(1)?.spec.harvest_capacity;
    let mut f = fixture(capacity)?;
    let worker = f.worker;
    f.order(vec![worker], UnitOrder::Harvest { target: MINERAL });
    f.until(2000, |f| f.remaining(MINERAL) == 0 && f.minerals() > 0)?;
    f.until(2000, |f| {
        f.world.entities[&f.worker].order_queue.front()
            == Some(&UnitOrder::Harvest {
                target: SECOND_MINERAL,
            })
    })?;
    let total = 20 + capacity;
    f.until(8000, |f| f.minerals() == total)?;
    f.until(100, |f| f.world.entities[&f.worker].order_queue.is_empty())?;
    assert_eq!(f.remaining(SECOND_MINERAL), 0);
    Ok(())
}

#[test]
fn a_partial_last_load_is_delivered_and_the_order_then_ends() -> TestResult {
    let mut f = fixture(3)?;
    f.world.map.resources_mut()[1].remaining = 0;
    let worker = f.worker;
    f.order(vec![worker], UnitOrder::Harvest { target: MINERAL });
    let mut exhausted = false;
    for _ in 0..1600 {
        f.idle(1);
        exhausted |= f
            .world
            .task_events()
            .iter()
            .any(|event| event.kind == TaskEventKind::ResourceExhausted);
        if exhausted {
            break;
        }
    }
    assert!(exhausted);
    assert_eq!(f.minerals(), 3);
    assert!(f.world.entities[&worker].order_queue.is_empty());
    assert!(!f.world.entities[&worker].is_mineral_walking);
    Ok(())
}

#[test]
fn losing_the_depot_ends_the_order_and_keeps_the_cargo() -> TestResult {
    let mut f = fixture(1000)?;
    let (worker, hall) = (f.worker, f.hall);
    f.order(vec![worker], UnitOrder::Harvest { target: MINERAL });
    f.until(800, |f| f.world.entities[&f.worker].cargo.is_some())?;
    f.world.entities.get_mut(&hall).ok_or("hall")?.hp = SimScalar::ZERO;
    f.idle(1);
    f.idle(1);
    assert!(f.world.entities[&worker].order_queue.is_empty());
    assert!(f.world.entities[&worker].cargo.is_some());
    assert_eq!(f.minerals(), 0);
    Ok(())
}

#[test]
fn return_cargo_delivers_then_resumes_the_last_node() -> TestResult {
    let mut f = fixture(1000)?;
    let worker = f.worker;
    f.order(vec![worker], UnitOrder::ReturnCargo);
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::NoCargo)
    );
    f.order(vec![worker], UnitOrder::Harvest { target: MINERAL });
    f.until(800, |f| f.world.entities[&f.worker].cargo.is_some())?;
    f.order(vec![worker], UnitOrder::Move { target: at(14, 14) });
    f.idle(20);
    assert!(
        f.world.entities[&worker].cargo.is_some(),
        "cargo outlives orders"
    );
    f.order(vec![worker], UnitOrder::ReturnCargo);
    f.until(800, |f| f.minerals() > 0)?;
    f.idle(2);
    assert_eq!(
        f.world.entities[&worker].order_queue.front(),
        Some(&UnitOrder::Harvest { target: MINERAL })
    );
    Ok(())
}

#[test]
fn a_new_order_interrupts_gathering_cleanly() -> TestResult {
    let mut f = fixture(1000)?;
    let worker = f.worker;
    f.order(vec![worker], UnitOrder::Harvest { target: MINERAL });
    f.until(400, |f| f.world.entities[&f.worker].task.gather_ticks > 0)?;
    f.order(vec![worker], UnitOrder::Stop);
    let entity = &f.world.entities[&worker];
    assert_eq!(entity.task.gather_ticks, 0);
    assert_eq!(entity.task.goal, None);
    assert!(!entity.is_mineral_walking);
    assert_eq!(f.remaining(MINERAL), 1000);
    Ok(())
}

#[test]
fn workers_walk_through_each_other_while_harvesting() -> TestResult {
    let mut f = fixture(1000)?;
    let worker_type = f.world.entities[&f.worker].unit_type_id;
    let mut crew = vec![f.worker];
    for y in 0..4 {
        crew.push(f.world.spawn_unit(worker_type, ME, at(9, 3 + y))?);
    }
    f.order(crew, UnitOrder::Harvest { target: MINERAL });
    f.until(2000, |f| f.minerals() >= 5 * f.spec.harvest_capacity)?;
    Ok(())
}

#[test]
fn gas_needs_a_completed_extractor_of_ones_own() -> TestResult {
    let mut f = fixture(1000)?;
    let worker = f.worker;
    f.order(vec![worker], UnitOrder::Harvest { target: GAS });
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::ExtractorRequired(GAS))
    );

    let extractor = f
        .world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.is_gas_extractor)
        .map(|a| a.unit_type_id)
        .ok_or("no extractor in the catalog")?;
    // The rival keeps a second building, or losing the extractor would end the match.
    let rival_hall = depot(&f.world)?;
    f.world.spawn_structure(rival_hall, PlayerId(1), 30, 30)?;
    let foreign = f.world.spawn_structure(extractor, PlayerId(1), 19, 19)?;
    f.order(vec![worker], UnitOrder::Harvest { target: GAS });
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::ExtractorRequired(GAS))
    );

    f.world.entities.get_mut(&foreign).ok_or("extractor")?.hp = SimScalar::ZERO;
    f.idle(1);
    let own = f.world.spawn_structure(extractor, ME, 19, 19)?;
    f.order(vec![worker], UnitOrder::Harvest { target: GAS });
    assert!(f.world.game_command_outcomes()[0].result.is_ok());
    f.until(3000, |f| f.world.player_resources[0].gas > 0)?;
    assert_eq!(f.minerals(), 0);

    // A lost extractor stops the work.
    f.until(400, |f| f.world.entities[&f.worker].cargo.is_none())?;
    f.world.entities.get_mut(&own).ok_or("extractor")?.hp = SimScalar::ZERO;
    let mut lost = false;
    for _ in 0..400 {
        f.idle(1);
        lost |= f
            .world
            .task_events()
            .iter()
            .any(|event| event.kind == TaskEventKind::ExtractorLost);
    }
    assert!(lost);
    Ok(())
}
