//! Player load and unload orders: range, capacity and setting passengers down.
//!
//! # Responsibility
//! Checks `UnitOrder::Load` and `UnitOrder::UnloadAll` through `PlayerCommand`, over the
//! carrier transaction whose own invariants `transport_ownership_tests` already covers.
//!
//! # Invariants
//! Carriers are found by the `transport_capacity` capability, never by identity.

use sim_core::economy::{GameCommandError, GameCommandReceipt, TaskEventKind};
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};
use std::error::Error;

const ME: PlayerId = PlayerId(0);

type TestResult = Result<(), Box<dyn Error>>;

fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

struct Fixture {
    world: SimWorld,
    carrier: EntityId,
    soldier: UnitTypeId,
    capacity: u32,
}

fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let map = GameMap::new(40, 40, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 31);
    let (carrier_type, capacity) = world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.transport_capacity > 0)
        .map(|a| (a.unit_type_id, a.capabilities.transport_capacity))
        .ok_or("no carrier in the catalog")?;
    let carrier = world.spawn_unit(carrier_type, ME, at(20, 20))?;
    let soldier = world
        .catalog
        .id_by_slug("pulse_bot")
        .ok_or("shipped unit")?;
    Ok(Fixture {
        world,
        carrier,
        soldier,
        capacity,
    })
}

impl Fixture {
    fn order(&mut self, entities: Vec<EntityId>, order: UnitOrder, queue: bool) {
        let tick = self.world.current_tick;
        self.world.step(&TickBundle {
            tick: Tick(tick),
            commands: vec![PlayerCommand {
                player_id: ME,
                entities,
                order,
                queue,
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

    fn aboard(&self) -> usize {
        self.world
            .entities
            .values()
            .filter(|e| e.transport_carrier() == Some(self.carrier))
            .count()
    }
}

#[test]
fn passengers_walk_to_the_carrier_and_board() -> TestResult {
    let mut f = fixture()?;
    let a = f.world.spawn_unit(f.soldier, ME, at(10, 20))?;
    let b = f.world.spawn_unit(f.soldier, ME, at(10, 22))?;
    let carrier = f.carrier;
    f.order(vec![a, b], UnitOrder::Load { carrier }, false);
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Ok(GameCommandReceipt::Accepted { recipients: 2 })
    );
    assert_eq!(f.aboard(), 0, "boarding takes the walk");
    f.idle(300);
    assert_eq!(f.aboard(), 2);
    Ok(())
}

#[test]
fn passengers_follow_a_moving_carrier() -> TestResult {
    let mut f = fixture()?;
    let a = f.world.spawn_unit(f.soldier, ME, at(4, 20))?;
    let carrier = f.carrier;
    f.order(vec![a], UnitOrder::Load { carrier }, false);
    f.order(vec![carrier], UnitOrder::Move { target: at(16, 8) }, false);
    f.idle(600);
    assert_eq!(f.aboard(), 1);
    Ok(())
}

#[test]
fn capacity_counts_supply_and_is_checked() -> TestResult {
    let mut f = fixture()?;
    let mut crowd = Vec::new();
    for index in 0..=f.capacity {
        let (x, y) = (i32::try_from(index % 6)?, i32::try_from(index / 6)?);
        crowd.push(f.world.spawn_unit(f.soldier, ME, at(16 + x, 18 + y))?);
    }
    let carrier = f.carrier;
    f.order(crowd.clone(), UnitOrder::Load { carrier }, false);
    assert!(matches!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::CarrierFull { needed, .. }) if needed == f.capacity + 1
    ));
    assert!(f.world.entities[&crowd[0]].order_queue.is_empty());

    crowd.pop();
    f.order(crowd, UnitOrder::Load { carrier }, false);
    f.idle(200);
    assert_eq!(f.aboard(), usize::try_from(f.capacity)?);
    Ok(())
}

#[test]
fn only_a_carrier_can_be_boarded_and_only_by_walkers() -> TestResult {
    let mut f = fixture()?;
    let a = f.world.spawn_unit(f.soldier, ME, at(10, 20))?;
    let b = f.world.spawn_unit(f.soldier, ME, at(12, 20))?;
    f.order(vec![a], UnitOrder::Load { carrier: b }, false);
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::NotACarrier(b))
    );
    let carrier = f.carrier;
    f.order(vec![carrier], UnitOrder::Load { carrier }, false);
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::NotACarrier(carrier))
    );
    Ok(())
}

#[test]
fn unload_all_sets_everyone_down_near_the_carrier() -> TestResult {
    let mut f = fixture()?;
    let carrier = f.carrier;
    f.order(vec![carrier], UnitOrder::UnloadAll, false);
    assert_eq!(
        f.world.game_command_outcomes()[0].result,
        Err(GameCommandError::NothingToUnload)
    );
    let a = f.world.spawn_unit(f.soldier, ME, at(18, 20))?;
    let b = f.world.spawn_unit(f.soldier, ME, at(18, 22))?;
    f.order(vec![a, b], UnitOrder::Load { carrier }, false);
    f.idle(100);
    assert_eq!(f.aboard(), 2);

    // Queued after a move, the unload happens at the destination.
    let landing = at(30, 30);
    f.order(vec![carrier], UnitOrder::Move { target: landing }, false);
    f.order(vec![carrier], UnitOrder::UnloadAll, true);
    f.idle(400);
    assert_eq!(f.aboard(), 0);
    assert!(f.world.entities[&carrier].order_queue.is_empty());
    let reach = SimScalar::from_num(6);
    for id in [a, b] {
        assert!(f.world.entities[&id].position.distance(landing) < reach);
    }
    Ok(())
}

#[test]
fn a_carrier_over_nowhere_keeps_its_passengers_and_reports_it() -> TestResult {
    let mut f = fixture()?;
    let carrier = f.carrier;
    let a = f.world.spawn_unit(f.soldier, ME, at(18, 20))?;
    f.order(vec![a], UnitOrder::Load { carrier }, false);
    f.idle(60);
    assert_eq!(f.aboard(), 1);
    // Fill every cell near the carrier with bodies' worth of structure footprints.
    let filler = f
        .world
        .catalog
        .id_by_slug("capacitor_dome")
        .ok_or("shipped structure")?;
    for y in (8..32).step_by(2) {
        for x in (8..32).step_by(2) {
            f.world.spawn_structure(filler, ME, x, y)?;
        }
    }
    f.order(vec![carrier], UnitOrder::UnloadAll, false);
    let mut reported = false;
    for _ in 0..80 {
        f.idle(1);
        reported |= f
            .world
            .task_events()
            .iter()
            .any(|event| event.kind == TaskEventKind::NoUnloadSpace);
    }
    assert!(reported);
    assert_eq!(f.aboard(), 1);
    Ok(())
}
