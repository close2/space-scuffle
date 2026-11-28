//! A transport picks up one of its owner's ground units: the carrier-side `UnitOrder::LoadUnit`.
//!
//! # Responsibility
//! Checks the carrier's side of loading through `PlayerCommand` (plan
//! `2026-12-03-movement-work-on-solver` WP-T1, B-247): in `StarCraft` II 5.0.14 a transport's
//! right click on one of its owner's units loads it (`CAbilTransport` `AllowSmartCmd 1`, `core`
//! AbilData:1192–1204), the transport travelling to it. Covered here: every admission refusal, a
//! pickup of a standing and of a walking passenger, a full carrier ending the order, a passenger
//! that dies or boards another carrier meanwhile, an air carrier's goal kept inside its domain,
//! the ground-carrier path under a test catalog, and a pickup queued behind a move.
//!
//! # How it fits
//! `load_orders_tests` covers the passengers' side (`UnitOrder::Load`), whose reach and capacity
//! rules the carrier's side shares; `transport_ownership_tests` covers the transaction both use.
//!
//! # Invariants
//! Carriers and passengers are found by capability (`transport_capacity`, the mover type),
//! never by identity.

use sim_core::economy::{GameCommandError, GameCommandReceipt, TaskEventKind};
use sim_core::transport::TransportAdmissionError;
use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, MoverType, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
    UnitTypeId,
};
use std::error::Error;

const ME: PlayerId = PlayerId(0);
const THEM: PlayerId = PlayerId(1);

type TestResult = Result<(), Box<dyn Error>>;

/// The centre of cell `(x, y)`, in world units.
fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

/// The first archetype `wanted` accepts.
fn kind_where(
    world: &SimWorld,
    wanted: impl Fn(&UnitArchetype) -> bool,
) -> Result<UnitTypeId, Box<dyn Error>> {
    Ok(world
        .catalog
        .archetypes()
        .find(|arch| wanted(arch))
        .map(|arch| arch.unit_type_id)
        .ok_or("the shipped catalog has no such role")?)
}

/// A mobile unit that is no carrier and no worker, of `mover`, filling one slot.
fn soldier_kind(world: &SimWorld, mover: MoverType) -> Result<UnitTypeId, Box<dyn Error>> {
    kind_where(world, |arch| {
        arch.mover_type == mover
            && arch.max_speed > SimScalar::ZERO
            && !arch.is_immobile_by_default()
            && arch.capabilities.transport_capacity == 0
            && arch.capabilities.harvester.is_none()
            && arch.capabilities.hangar.is_none()
            && arch.supply_cost == 1
            && arch.selectable
            && arch.order_targetable
    })
}

struct Fixture {
    world: SimWorld,
    carrier: EntityId,
    soldier: UnitTypeId,
    capacity: u32,
}

/// A flat 48 by 48 world with one shipped air carrier of `ME` at cell (24, 24).
fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 47);
    let carrier_type = kind_where(&world, |arch| {
        arch.capabilities.transport_capacity > 0 && arch.mover_type == MoverType::Air
    })?;
    let capacity = world
        .catalog
        .get(carrier_type)?
        .capabilities
        .transport_capacity;
    let carrier = world.spawn_unit(carrier_type, ME, at(24, 24))?;
    let soldier = soldier_kind(&world, MoverType::Ground)?;
    Ok(Fixture {
        world,
        carrier,
        soldier,
        capacity,
    })
}

impl Fixture {
    /// Runs one tick with `commands`, asserting it raised no diagnostic.
    fn step(&mut self, commands: Vec<PlayerCommand>) {
        let tick = self.world.current_tick;
        self.world.step(&TickBundle {
            tick: Tick(tick),
            commands,
            state_checksum: None,
        });
        assert_eq!(self.world.diagnostics(), []);
    }

    /// Gives `entities` of `ME` `order` in one tick.
    fn command(&mut self, entities: Vec<EntityId>, order: UnitOrder, queue: bool) {
        self.step(vec![PlayerCommand {
            player_id: ME,
            entities,
            order,
            queue,
        }]);
    }

    /// Gives `entities` of `ME` the game command `order` in one tick, and returns its result.
    fn order(
        &mut self,
        entities: Vec<EntityId>,
        order: UnitOrder,
        queue: bool,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.command(entities, order, queue);
        self.world
            .game_command_outcomes()
            .first()
            .map_or(Err(GameCommandError::UnsupportedOrder), |outcome| {
                outcome.result.clone()
            })
    }

    /// The carrier's pickup of `passenger`, given now.
    fn pick_up(
        &mut self,
        carrier: EntityId,
        passenger: EntityId,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.order(vec![carrier], UnitOrder::LoadUnit { passenger }, false)
    }

    /// Runs `ticks` ticks with no command; true if `kind` was reported for `entity` meanwhile.
    fn idle_watching(&mut self, ticks: u32, entity: EntityId, kind: &TaskEventKind) -> bool {
        let mut seen = false;
        for _ in 0..ticks {
            self.step(Vec::new());
            seen |= self
                .world
                .task_events()
                .iter()
                .any(|event| event.entity == entity && event.kind == *kind);
        }
        seen
    }

    fn idle(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.step(Vec::new());
        }
    }

    /// Ticks until `passenger` is carried by `carrier`, at most `limit`; how many it took.
    fn ticks_until_aboard(&mut self, passenger: EntityId, carrier: EntityId, limit: u32) -> u32 {
        for tick in 0..limit {
            if self.carrier_of(passenger) == Some(carrier) {
                return tick;
            }
            self.step(Vec::new());
        }
        limit
    }

    fn carrier_of(&self, id: EntityId) -> Option<EntityId> {
        self.world
            .entities
            .get(&id)
            .and_then(sim_core::Entity::transport_carrier)
    }

    fn front_order(&self, id: EntityId) -> Option<UnitOrder> {
        self.world.entities.get(&id)?.order_queue.front().cloned()
    }

    fn kill(&mut self, id: EntityId) {
        if let Some(entity) = self.world.entities.get_mut(&id) {
            entity.hp = SimScalar::ZERO;
        }
    }
}

/// The selection's and the passenger's refusals, each with its typed reason, and nothing ordered
/// by any.
#[test]
fn each_admission_refusal_is_typed_and_orders_nothing() -> TestResult {
    let mut f = fixture()?;
    let carrier = f.carrier;
    let soldier = f.world.spawn_unit(f.soldier, ME, at(10, 24))?;
    let other = f.world.spawn_unit(f.soldier, ME, at(12, 24))?;
    let foreign = f.world.spawn_unit(f.soldier, THEM, at(10, 30))?;
    let flyer_kind = soldier_kind(&f.world, MoverType::Air)?;
    let flyer = f.world.spawn_unit(flyer_kind, ME, at(10, 20))?;

    // No carrier in the selection.
    assert_eq!(
        f.order(
            vec![other],
            UnitOrder::LoadUnit { passenger: soldier },
            false
        ),
        Err(GameCommandError::NotACarrier(other))
    );
    // The passenger does not exist, is another player's, or is the carrier itself.
    let missing = EntityId(9_999);
    assert_eq!(
        f.pick_up(carrier, missing),
        Err(GameCommandError::Transport(
            TransportAdmissionError::MissingEntity(missing)
        ))
    );
    assert_eq!(
        f.pick_up(carrier, foreign),
        Err(GameCommandError::Transport(
            TransportAdmissionError::DifferentOwners {
                passenger: foreign,
                carrier,
            }
        ))
    );
    assert_eq!(
        f.pick_up(carrier, carrier),
        Err(GameCommandError::Transport(
            TransportAdmissionError::SameEntity(carrier)
        ))
    );
    // Only a walker boards.
    assert_eq!(
        f.pick_up(carrier, flyer),
        Err(GameCommandError::NotACarrier(carrier))
    );
    // A unit inside a structure cannot be the target of an order.
    let depot = EntityId(9_998);
    f.world
        .entities
        .get_mut(&other)
        .ok_or("other")?
        .task
        .housed_in = Some(depot);
    assert_eq!(
        f.pick_up(carrier, other),
        Err(GameCommandError::UntargetableTarget(other))
    );
    f.world
        .entities
        .get_mut(&other)
        .ok_or("other")?
        .task
        .housed_in = None;
    // A destroyed passenger.
    f.kill(other);
    assert_eq!(
        f.pick_up(carrier, other),
        Err(GameCommandError::Transport(
            TransportAdmissionError::DeadEntity(other)
        ))
    );
    assert!(f.world.entities[&carrier].order_queue.is_empty());
    Ok(())
}

/// The selected carrier's refusals: unfinished, anchored, without room; and a passenger already
/// carried. Nothing is ordered by any.
#[test]
fn each_carrier_refusal_is_typed_and_orders_nothing() -> TestResult {
    let mut f = fixture()?;
    let carrier = f.carrier;
    let soldier = f.world.spawn_unit(f.soldier, ME, at(10, 24))?;
    // An unfinished carrier, then an anchored one.
    f.world
        .entities
        .get_mut(&carrier)
        .ok_or("carrier")?
        .is_completed = false;
    assert_eq!(
        f.pick_up(carrier, soldier),
        Err(GameCommandError::Transport(
            TransportAdmissionError::IncompleteEntity(carrier)
        ))
    );
    f.world
        .entities
        .get_mut(&carrier)
        .ok_or("carrier")?
        .is_completed = true;
    f.world
        .entities
        .get_mut(&carrier)
        .ok_or("carrier")?
        .is_immobile = true;
    assert_eq!(
        f.pick_up(carrier, soldier),
        Err(GameCommandError::NotACarrier(carrier))
    );
    f.world
        .entities
        .get_mut(&carrier)
        .ok_or("carrier")?
        .is_immobile = false;
    assert!(f.world.entities[&carrier].order_queue.is_empty());

    // A passenger already carried, and a carrier with no room left.
    let mut crowd = Vec::new();
    for index in 0..f.capacity {
        let (x, y) = (i32::try_from(index % 4)?, i32::try_from(index / 4)?);
        crowd.push(f.world.spawn_unit(f.soldier, ME, at(22 + x, 21 + y))?);
    }
    assert!(f
        .order(crowd.clone(), UnitOrder::Load { carrier }, false)
        .is_ok());
    f.idle(200);
    assert!(crowd.iter().all(|&id| f.carrier_of(id) == Some(carrier)));
    assert_eq!(
        f.pick_up(carrier, crowd[0]),
        Err(GameCommandError::Transport(
            TransportAdmissionError::AlreadyLoaded(crowd[0])
        ))
    );
    assert_eq!(
        f.pick_up(carrier, soldier),
        Err(GameCommandError::CarrierFull {
            carrier,
            free: 0,
            needed: 1,
        })
    );
    assert!(f.world.entities[&carrier].order_queue.is_empty());
    assert!(f.world.entities[&soldier].order_queue.is_empty());
    Ok(())
}

/// The carrier travels to a standing passenger and loads it; the order then ends.
#[test]
fn a_carrier_picks_up_a_standing_passenger() -> TestResult {
    let mut f = fixture()?;
    let carrier = f.carrier;
    let soldier = f.world.spawn_unit(f.soldier, ME, at(8, 30))?;
    assert_eq!(
        f.pick_up(carrier, soldier),
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    );
    assert_eq!(
        f.front_order(carrier),
        Some(UnitOrder::LoadUnit { passenger: soldier })
    );
    assert_eq!(f.carrier_of(soldier), None, "the pickup takes the flight");
    let start = f.world.entities[&soldier].position;
    let ticks = f.ticks_until_aboard(soldier, carrier, 400);
    assert!(ticks < 400, "the passenger boarded");
    assert!(
        ticks > 20,
        "the carrier flew there first, took {ticks} ticks"
    );
    assert_eq!(
        f.world.entities[&soldier].position, start,
        "the passenger stood still: the carrier came to it"
    );
    assert_eq!(
        f.front_order(carrier),
        None,
        "the order ended with the load"
    );
    Ok(())
}

/// A passenger walking away keeps its own order until it boards, and the carrier chases it.
#[test]
fn a_carrier_catches_a_walking_passenger_which_keeps_its_orders() -> TestResult {
    let mut f = fixture()?;
    let carrier = f.carrier;
    let soldier = f.world.spawn_unit(f.soldier, ME, at(16, 24))?;
    let away = UnitOrder::Move { target: at(2, 4) };
    f.command(vec![soldier], away.clone(), false);
    assert!(f.pick_up(carrier, soldier).is_ok());
    f.idle(5);
    assert_eq!(
        f.front_order(soldier),
        Some(away),
        "the passenger keeps its orders"
    );
    let ticks = f.ticks_until_aboard(soldier, carrier, 600);
    assert!(ticks < 600, "the carrier caught the passenger");
    assert_eq!(f.front_order(carrier), None);
    assert_eq!(
        f.front_order(soldier),
        None,
        "a carried unit holds no orders"
    );
    Ok(())
}

/// Two carriers sent for one unit: one loads it, and the other's order ends with nothing said.
#[test]
fn every_selected_carrier_goes_and_the_first_there_loads() -> TestResult {
    let mut f = fixture()?;
    let first = f.carrier;
    let kind = f.world.entities[&first].unit_type_id;
    let second = f.world.spawn_unit(kind, ME, at(40, 40))?;
    let soldier = f.world.spawn_unit(f.soldier, ME, at(20, 24))?;
    assert_eq!(
        f.order(
            vec![second, first, soldier],
            UnitOrder::LoadUnit { passenger: soldier },
            false
        ),
        Ok(GameCommandReceipt::Accepted { recipients: 2 })
    );
    assert_eq!(f.front_order(soldier), None, "the passenger takes no order");
    assert!(f.ticks_until_aboard(soldier, first, 300) < 300);
    f.idle(1);
    assert_eq!(f.front_order(second), None);
    assert_eq!(f.world.task_events(), []);
    Ok(())
}

/// A carrier filled by others on the way ends its pickup and says why (rallies plan S6: a full
/// transport's move command is removed).
#[test]
fn a_carrier_that_fills_up_on_the_way_ends_the_pickup() -> TestResult {
    let mut f = fixture()?;
    let carrier = f.carrier;
    let far = f.world.spawn_unit(f.soldier, ME, at(2, 2))?;
    let mut crowd = Vec::new();
    for index in 0..f.capacity {
        let (x, y) = (i32::try_from(index % 4)?, i32::try_from(index / 4)?);
        crowd.push(f.world.spawn_unit(f.soldier, ME, at(22 + x, 21 + y))?);
    }
    assert!(f.pick_up(carrier, far).is_ok());
    assert!(f
        .order(crowd.clone(), UnitOrder::Load { carrier }, false)
        .is_ok());
    let reported = f.idle_watching(300, carrier, &TaskEventKind::CarrierUnavailable);
    assert!(crowd.iter().all(|&id| f.carrier_of(id) == Some(carrier)));
    assert!(reported, "the full carrier reported the ended pickup");
    assert_eq!(f.front_order(carrier), None);
    assert_eq!(f.carrier_of(far), None);
    Ok(())
}

/// A passenger that dies before the carrier arrives ends the pickup, with nothing reported.
#[test]
fn a_passenger_that_dies_ends_the_pickup() -> TestResult {
    let mut f = fixture()?;
    let carrier = f.carrier;
    let soldier = f.world.spawn_unit(f.soldier, ME, at(4, 4))?;
    assert!(f.pick_up(carrier, soldier).is_ok());
    f.idle(10);
    assert!(f.world.entities[&carrier].task.goal.is_some(), "on its way");
    f.kill(soldier);
    f.idle(2);
    assert_eq!(f.front_order(carrier), None);
    assert!(f.world.entities[&carrier].task.goal.is_none());
    Ok(())
}

/// A passenger that boards another carrier meanwhile ends the pickup.
#[test]
fn a_passenger_boarding_another_carrier_ends_the_pickup() -> TestResult {
    let mut f = fixture()?;
    let carrier = f.carrier;
    let kind = f.world.entities[&carrier].unit_type_id;
    let near = f.world.spawn_unit(kind, ME, at(6, 6))?;
    let soldier = f.world.spawn_unit(f.soldier, ME, at(4, 4))?;
    assert!(f.pick_up(carrier, soldier).is_ok());
    assert!(f
        .order(vec![soldier], UnitOrder::Load { carrier: near }, false)
        .is_ok());
    assert!(f.ticks_until_aboard(soldier, near, 200) < 200);
    f.idle(1);
    assert_eq!(f.front_order(carrier), None);
    assert_eq!(f.carrier_of(soldier), Some(near));
    Ok(())
}

/// An air carrier aims at the passenger's position moved into its own domain: a passenger at the
/// map's edge is reached from inside the map.
#[test]
fn an_air_carrier_aims_inside_its_domain() -> TestResult {
    let mut f = fixture()?;
    let carrier = f.carrier;
    let edge = SimVec2::new(SimScalar::from_num(0.4), SimScalar::from_num(24));
    let soldier = f.world.spawn_unit(f.soldier, ME, edge)?;
    assert!(f.pick_up(carrier, soldier).is_ok());
    f.idle(1);
    let kind = f.world.entities[&carrier].unit_type_id;
    let radius = f.world.catalog.get(kind)?.collision_radius;
    let goal = f.world.entities[&carrier]
        .task
        .goal
        .ok_or("the carrier has a goal")?;
    assert_eq!(goal.x, radius, "clamped to the carrier's domain");
    assert_eq!(goal.y, edge.y);
    assert!(f.ticks_until_aboard(soldier, carrier, 400) < 400);
    Ok(())
}

/// A ground carrier, which nothing ships, walks to the passenger's cell, as a passenger walks to
/// a carrier, and loads it (a test-catalog edit gives a walker the capability).
#[test]
fn a_ground_carrier_walks_to_its_passenger() -> TestResult {
    let mut f = fixture()?;
    let walker = kind_where(&f.world, |arch| {
        arch.mover_type == MoverType::Ground
            && arch.max_speed > SimScalar::ZERO
            && arch.capabilities.transport_capacity == 0
            && arch.capabilities.harvester.is_none()
            && arch.supply_cost >= 2
    })?;
    f.world
        .catalog
        .get_mut(walker)?
        .capabilities
        .transport_capacity = 4;
    let carrier = f.world.spawn_unit(walker, ME, at(30, 30))?;
    let soldier = f.world.spawn_unit(f.soldier, ME, at(12, 30))?;
    assert!(f.pick_up(carrier, soldier).is_ok());
    f.idle(1);
    assert_eq!(
        f.world.entities[&carrier].task.goal,
        Some(at(12, 30)),
        "the passenger's free cell"
    );
    assert!(f.ticks_until_aboard(soldier, carrier, 600) < 600);
    assert_eq!(f.front_order(carrier), None);
    // A ground carrier with passengers is not itself a passenger.
    let other = f.world.spawn_unit(walker, ME, at(40, 40))?;
    assert_eq!(
        f.pick_up(other, carrier),
        Err(GameCommandError::Transport(
            TransportAdmissionError::HasPassengers(carrier)
        ))
    );
    Ok(())
}

/// A pickup queued behind a move waits for the move, then runs.
#[test]
fn a_queued_pickup_runs_after_the_move() -> TestResult {
    let mut f = fixture()?;
    let carrier = f.carrier;
    let soldier = f.world.spawn_unit(f.soldier, ME, at(24, 30))?;
    let detour = at(40, 8);
    f.command(vec![carrier], UnitOrder::Move { target: detour }, false);
    assert!(f
        .order(
            vec![carrier],
            UnitOrder::LoadUnit { passenger: soldier },
            true
        )
        .is_ok());
    assert_eq!(
        f.front_order(carrier),
        Some(UnitOrder::Move { target: detour })
    );
    f.idle(60);
    assert_eq!(f.carrier_of(soldier), None, "not while the move runs");
    let mut reached = false;
    for _ in 0..400 {
        f.step(Vec::new());
        let position = f.world.entities[&carrier].position;
        reached |= position.distance(detour) < SimScalar::from_num(2);
        if f.carrier_of(soldier) == Some(carrier) {
            break;
        }
    }
    assert!(reached, "the carrier went to the detour first");
    assert_eq!(f.carrier_of(soldier), Some(carrier));
    assert_eq!(f.front_order(carrier), None);
    Ok(())
}
