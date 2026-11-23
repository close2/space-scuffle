//! Production queues and supply: which job is in production and what supply it holds (plan
//! `2026-11-23-production-queue` D-1, D-2; B-59).
//!
//! Every world here is built from the shipped catalog: the producer is the first structure that
//! provides supply and trains a mobile product, and every price, time and supply is the
//! catalog's. No identity is named. After every tick the supply ledger and the queue invariants
//! are checked through the world's diagnostics.

use sim_core::production::{
    ProductionAdmissionError, ProductionBlock, ProductionInvariant, ProductionJob,
};
use sim_core::{SimWorld, SimulationDiagnostic, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, PlayerSlot, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A world on open ground with one completed producer of the shipped catalog.
struct Fixture {
    world: SimWorld,
    producer: EntityId,
    product: UnitTypeId,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog");
        let catalog = sim_core::catalog::RuntimeCatalogs::load_from_directory(directory)?.units;
        let (producer_type, product) = catalog
            .archetypes()
            .filter(|a| a.supply_provided > 0)
            .find_map(|a| {
                a.trains_units.iter().find_map(|slug| {
                    catalog
                        .get_by_slug(slug)
                        .filter(|p| {
                            !matches!(p.shape, ShapeKind::BuildingDome { .. })
                                && p.supply_cost > 0
                                && p.build_time_ticks > 1
                        })
                        .map(|p| (a.unit_type_id, p.unit_type_id))
                })
            })
            .ok_or("no authored producer of a mobile product with supply")?;
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = SimWorld::with_catalogs(map, 2311, catalog, UpgradeCatalog::default())?;
        let producer = world.spawn_structure(producer_type, PlayerId(0), 12, 12)?;
        Ok(Self {
            world,
            producer,
            product,
        })
    }

    /// The product's price, supply and production time, from the catalog.
    fn product_costs(&self) -> Result<(u32, u32, u32, u32), Box<dyn std::error::Error>> {
        let arch = self.world.catalog.get(self.product)?;
        Ok((
            arch.mineral_cost,
            arch.gas_cost,
            arch.supply_cost,
            arch.build_time_ticks,
        ))
    }

    /// A bank that pays for `jobs` products.
    fn fund(&mut self, jobs: u32) -> TestResult {
        let (minerals, gas, _, _) = self.product_costs()?;
        let bank = &mut self.world.player_resources[PlayerSlot::ALL[0]];
        bank.minerals = minerals * jobs;
        bank.gas = gas * jobs;
        Ok(())
    }

    fn admit(&mut self) -> TestResult {
        self.world
            .try_queue_production(PlayerId(0), &[self.producer], self.product)?;
        Ok(())
    }

    fn queue(&self) -> Vec<ProductionJob> {
        self.world.entities[&self.producer]
            .production_queue
            .iter()
            .copied()
            .collect()
    }

    fn supply_used(&self) -> u32 {
        self.world.player_resources[PlayerSlot::ALL[0]].supply_used
    }

    /// One tick, then the ledger and the queue invariants (P-1).
    fn tick(&mut self) -> TestResult {
        self.world.step(&TickBundle {
            tick: Tick(self.world.current_tick + 1),
            commands: vec![],
            state_checksum: None,
        });
        assert_eq!(
            self.world.diagnostics(),
            [],
            "tick {}",
            self.world.current_tick
        );
        self.world.validate_supply_ledger()?;
        Ok(())
    }
}

/// D-1: the first job admitted to an idle producer is in production at once and the jobs behind
/// it are not; when the first leaves, the next is in production from the update it first counts
/// down. Only the job in production holds supply (WP2; WP1 still reserved it for all three).
#[test]
fn the_first_job_is_in_production_and_the_rest_wait_their_turn() -> TestResult {
    let mut f = Fixture::new()?;
    f.fund(3)?;
    let (_, _, supply, build_time) = f.product_costs()?;
    let before = f.supply_used();
    for _ in 0..3 {
        f.admit()?;
    }
    let started: Vec<bool> = f.queue().iter().map(|job| job.in_production).collect();
    assert_eq!(started, [true, false, false]);
    assert_eq!(f.supply_used(), before + supply);
    // The first job counts down `build_time` updates and leaves on the last.
    for _ in 0..build_time {
        f.tick()?;
    }
    let queue = f.queue();
    assert_eq!(queue.len(), 2, "the first job left on its last update");
    assert!(
        !queue[0].in_production,
        "the next job has not counted down yet"
    );
    assert_eq!(queue[0].remaining_ticks, queue[0].total_ticks);
    f.tick()?;
    let queue = f.queue();
    assert!(queue[0].in_production, "it starts on its first countdown");
    assert_eq!(queue[0].remaining_ticks, queue[0].total_ticks - 1);
    assert!(!queue[1].in_production);
    Ok(())
}

/// What a test reads beside the queue: the cap, the blocks, and a second producer.
impl Fixture {
    fn supply_max(&self) -> u32 {
        self.world.player_resources[PlayerSlot::ALL[0]].supply_max
    }

    fn room(&self) -> u32 {
        self.supply_max() - self.supply_used()
    }

    /// Places units of the product, each holding its captured supply, until less than
    /// `room + supply` is left below the cap, so `room` fits and `room + supply` does not.
    fn fill_supply_to(&mut self, room: u32) -> TestResult {
        let (_, _, supply, _) = self.product_costs()?;
        let mut cell = 0;
        while self.room() >= room + supply {
            let (x, y) = (2 + cell % 28, 26 + cell / 28);
            self.world.spawn_unit(
                self.product,
                PlayerId(0),
                SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y)),
            )?;
            cell += 1;
        }
        Ok(())
    }

    fn blocks(&self) -> Vec<ProductionBlock> {
        self.world.production_blocks().to_vec()
    }

    /// A second completed producer of the same type, which also provides its supply.
    fn second_producer(&mut self) -> Result<EntityId, Box<dyn std::error::Error>> {
        let producer_type = self.world.entities[&self.producer].unit_type_id;
        Ok(self
            .world
            .spawn_structure(producer_type, PlayerId(0), 2, 2)?)
    }

    fn admit_at(&mut self, producer: EntityId) -> TestResult {
        self.world
            .try_queue_production(PlayerId(0), &[producer], self.product)?;
        Ok(())
    }

    fn queue_of(&self, producer: EntityId) -> Vec<ProductionJob> {
        self.world.entities[&producer]
            .production_queue
            .iter()
            .copied()
            .collect()
    }

    /// One tick with `commands`, then the ledger and the queue invariants.
    fn tick_with(&mut self, commands: Vec<PlayerCommand>) -> TestResult {
        self.world.step(&TickBundle {
            tick: Tick(self.world.current_tick + 1),
            commands,
            state_checksum: None,
        });
        assert_eq!(
            self.world.diagnostics(),
            [],
            "tick {}",
            self.world.current_tick
        );
        self.world.validate_supply_ledger()?;
        Ok(())
    }

    fn cancel(&mut self, job_index: u32) -> TestResult {
        self.tick_with(vec![PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![self.producer],
            order: UnitOrder::CancelProduction { job_index },
            queue: false,
        }])?;
        let outcome = self
            .world
            .game_command_outcomes()
            .first()
            .ok_or("no outcome")?;
        assert!(outcome.result.is_ok(), "{outcome:?}");
        Ok(())
    }

    fn units_of_product(&self) -> usize {
        self.world
            .entities
            .values()
            .filter(|entity| entity.unit_type_id == self.product)
            .count()
    }
}

/// D-1, D-2 (B-59): three admissions hold one product's supply, that of the job in production.
#[test]
fn queued_jobs_hold_no_supply() -> TestResult {
    let mut f = Fixture::new()?;
    f.fund(3)?;
    let (_, _, supply, _) = f.product_costs()?;
    let before = f.supply_used();
    for _ in 0..3 {
        f.admit()?;
    }
    assert_eq!(f.supply_used(), before + supply);
    let held: Vec<u32> = f.queue().iter().map(|job| job.reserved_supply).collect();
    assert_eq!(held, [supply, 0, 0]);
    f.tick()?;
    assert_eq!(f.supply_used(), before + supply);
    Ok(())
}

/// D-2: when the first job leaves, its unit holds its supply; the next job takes its own on the
/// update it starts, and not before.
#[test]
fn the_next_job_takes_its_supply_when_it_starts() -> TestResult {
    let mut f = Fixture::new()?;
    f.fund(2)?;
    let (_, _, supply, build_time) = f.product_costs()?;
    let before = f.supply_used();
    f.admit()?;
    f.admit()?;
    for _ in 0..build_time {
        f.tick()?;
    }
    assert_eq!(f.units_of_product(), 1);
    assert_eq!(
        f.supply_used(),
        before + supply,
        "the unit holds what the job held"
    );
    assert_eq!(f.queue()[0].reserved_supply, 0);
    f.tick()?;
    assert!(f.queue()[0].in_production);
    assert_eq!(f.queue()[0].reserved_supply, supply);
    assert_eq!(f.supply_used(), before + 2 * supply);
    Ok(())
}

/// P-3 (D-2, 5.0.14 `WaitForFood`): at the cap the next front job does not count down and its
/// producer publishes `SupplyWait`; on the update after a provider completes it starts and
/// counts. The job behind it neither counts nor holds supply.
#[test]
fn a_front_job_waits_for_supply_and_starts_when_it_fits() -> TestResult {
    let mut f = Fixture::new()?;
    f.fund(3)?;
    let (_, _, supply, build_time) = f.product_costs()?;
    f.fill_supply_to(supply)?;
    for _ in 0..3 {
        f.admit()?;
    }
    for _ in 0..build_time {
        f.tick()?;
    }
    assert_eq!(f.queue().len(), 2, "the first job is done");
    assert!(f.room() < supply, "its unit holds the last supply");
    let used = f.supply_used();
    for _ in 0..5 {
        f.tick()?;
        let queue = f.queue();
        assert!(!queue[0].in_production);
        assert_eq!(queue[0].remaining_ticks, queue[0].total_ticks);
        assert_eq!((queue[0].reserved_supply, queue[1].reserved_supply), (0, 0));
        assert_eq!(f.supply_used(), used);
        assert_eq!(
            f.blocks(),
            [ProductionBlock::SupplyWait {
                producer: f.producer
            }]
        );
    }
    // A provider completes (a setup structure is created completed).
    f.second_producer()?;
    f.tick()?;
    let queue = f.queue();
    assert!(queue[0].in_production);
    assert_eq!(queue[0].remaining_ticks, queue[0].total_ticks - 1);
    assert_eq!(queue[0].reserved_supply, supply);
    assert!(!queue[1].in_production);
    assert_eq!(queue[1].remaining_ticks, queue[1].total_ticks);
    assert_eq!(f.supply_used(), used + supply);
    assert_eq!(f.blocks(), []);
    Ok(())
}

/// D-2, rule 2: a job admitted to an idle producer starts at once and so needs its supply at
/// admission; a job admitted behind a running one is not checked against supply.
#[test]
fn admission_to_an_idle_producer_needs_supply_and_behind_a_job_does_not() -> TestResult {
    let mut f = Fixture::new()?;
    f.fund(3)?;
    let (_, _, supply, _) = f.product_costs()?;
    f.fill_supply_to(0)?;
    assert!(f.room() < supply);
    let before = f.world.compute_checksum();
    let refused = f
        .world
        .try_queue_production(PlayerId(0), &[f.producer], f.product);
    assert!(
        matches!(refused, Err(ProductionAdmissionError::Supply { .. })),
        "{refused:?}"
    );
    assert_eq!(before, f.world.compute_checksum());

    let mut f = Fixture::new()?;
    f.fund(3)?;
    f.fill_supply_to(supply)?;
    f.admit()?;
    assert!(f.room() < supply, "the running job took the last supply");
    let bank = f.world.player_resources[PlayerSlot::ALL[0]];
    f.admit()?;
    let (minerals, gas, _, _) = f.product_costs()?;
    let after = f.world.player_resources[PlayerSlot::ALL[0]];
    assert_eq!(after.minerals, bank.minerals - minerals);
    assert_eq!(after.gas, bank.gas - gas);
    assert_eq!(after.supply_used, bank.supply_used);
    assert_eq!(f.queue()[1].reserved_supply, 0);
    Ok(())
}

/// D-2: cancelling a waiting job returns its minerals and gas and no supply, and the running
/// job keeps its countdown.
#[test]
fn cancelling_a_waiting_job_returns_only_its_price() -> TestResult {
    let mut f = Fixture::new()?;
    f.fund(2)?;
    f.admit()?;
    f.admit()?;
    f.tick()?;
    let bank = f.world.player_resources[PlayerSlot::ALL[0]];
    let running = f.queue()[0].remaining_ticks;
    let (minerals, gas, _, _) = f.product_costs()?;
    f.cancel(1)?;
    let after = f.world.player_resources[PlayerSlot::ALL[0]];
    assert_eq!(after.minerals, bank.minerals + minerals);
    assert_eq!(after.gas, bank.gas + gas);
    assert_eq!(after.supply_used, bank.supply_used);
    assert_eq!(f.queue().len(), 1);
    assert_eq!(f.queue()[0].remaining_ticks, running - 1);
    Ok(())
}

/// D-2: cancelling the job in production returns its price and its supply, and the next job
/// starts on the next update, the same tick's production phase, taking its own.
#[test]
fn cancelling_the_running_job_releases_its_supply_and_the_next_starts() -> TestResult {
    let mut f = Fixture::new()?;
    f.fund(2)?;
    let (minerals, gas, supply, _) = f.product_costs()?;
    let before = f.supply_used();
    f.admit()?;
    f.admit()?;
    f.tick()?;
    let bank = f.world.player_resources[PlayerSlot::ALL[0]];
    f.cancel(0)?;
    let after = f.world.player_resources[PlayerSlot::ALL[0]];
    assert_eq!(after.minerals, bank.minerals + minerals);
    assert_eq!(after.gas, bank.gas + gas);
    let queue = f.queue();
    assert_eq!(queue.len(), 1);
    assert!(
        queue[0].in_production,
        "the next job started in the same tick"
    );
    assert_eq!(queue[0].remaining_ticks, queue[0].total_ticks - 1);
    assert_eq!(f.supply_used(), before + supply);
    Ok(())
}

/// D-2, R-3: two front jobs that become ready to start on the same update compete for the last
/// supply in `EntityId` order: the lower producer starts, the other waits.
#[test]
fn two_producers_waiting_for_the_last_supply_start_in_entity_order() -> TestResult {
    let mut f = Fixture::new()?;
    f.fund(4)?;
    let (_, _, supply, build_time) = f.product_costs()?;
    let second = f.second_producer()?;
    assert!(f.producer < second);
    f.fill_supply_to(3 * supply)?;
    for producer in [f.producer, f.producer, second, second] {
        f.admit_at(producer)?;
    }
    let made = f.units_of_product();
    for _ in 0..build_time {
        f.tick()?;
    }
    assert_eq!(f.units_of_product(), made + 2);
    let room = f.room();
    assert!(
        room >= supply && room < 2 * supply,
        "one product's room left"
    );
    f.tick()?;
    assert!(f.queue_of(f.producer)[0].in_production);
    assert!(!f.queue_of(second)[0].in_production);
    assert_eq!(
        f.blocks(),
        [ProductionBlock::SupplyWait { producer: second }]
    );
    Ok(())
}

/// D-2: losing supply while a job runs (a provider destroyed) does not stop it; it finishes on
/// its tick and supply may exceed the cap. The job behind it then waits.
#[test]
fn supply_lost_during_production_does_not_stop_the_running_job() -> TestResult {
    let mut f = Fixture::new()?;
    f.fund(2)?;
    let (_, _, supply, build_time) = f.product_costs()?;
    let provider = f.second_producer()?;
    f.fill_supply_to(supply)?;
    // Both jobs start while the second provider stands: the first at admission.
    f.admit()?;
    f.admit()?;
    f.world
        .entities
        .get_mut(&provider)
        .ok_or("provider absent")?
        .hp = SimScalar::ZERO;
    let made = f.units_of_product();
    for tick in 1..=build_time {
        f.tick()?;
        if tick < build_time {
            assert_eq!(f.queue()[0].remaining_ticks, build_time - tick);
        }
    }
    assert_eq!(
        f.units_of_product(),
        made + 1,
        "the running job finished on its tick"
    );
    assert!(
        f.supply_used() > f.supply_max(),
        "supply may exceed the cap"
    );
    f.tick()?;
    assert!(!f.queue()[0].in_production);
    assert_eq!(
        f.blocks(),
        [ProductionBlock::SupplyWait {
            producer: f.producer
        }]
    );
    Ok(())
}

/// P-2: where supply never binds, every job of a queue of five on each of two producers
/// completes on the tick it completed before the change (WP1's tree gives the same ticks): the
/// k-th job of a queue completes `k × build_time` ticks after admission.
#[test]
fn jobs_complete_on_the_same_ticks_when_supply_never_binds() -> TestResult {
    let mut f = Fixture::new()?;
    f.fund(10)?;
    let (_, _, _, build_time) = f.product_costs()?;
    let second = f.second_producer()?;
    for producer in [f.producer, second] {
        for _ in 0..5 {
            f.admit_at(producer)?;
        }
    }
    let mut completed = Vec::new();
    let mut left = (5, 5);
    for _ in 0..=5 * build_time {
        f.tick()?;
        let now = (f.queue_of(f.producer).len(), f.queue_of(second).len());
        if now.0 < left.0 {
            completed.push((0_u8, f.world.current_tick));
        }
        if now.1 < left.1 {
            completed.push((1, f.world.current_tick));
        }
        left = now;
        assert_eq!(f.blocks(), []);
    }
    let expected: Vec<(u8, u32)> = (1..=5_u32)
        .flat_map(|k| [(0, k * build_time), (1, k * build_time)])
        .collect();
    assert_eq!(completed, expected);
    Ok(())
}

/// D-1: a job behind the front in production, and a waiting job holding supply, are invariant
/// failures reported as diagnostics, never repaired.
#[test]
fn corrupt_queue_states_are_invariant_failures() -> TestResult {
    for corruption in 0..2 {
        let mut f = Fixture::new()?;
        f.fund(2)?;
        f.admit()?;
        f.admit()?;
        let producer = f.producer;
        let job = f
            .world
            .entities
            .get_mut(&producer)
            .ok_or("producer absent")?
            .production_queue
            .get_mut(1)
            .ok_or("no second job")?;
        if corruption == 0 {
            job.in_production = true;
        } else {
            // Keep the ledger balanced, so only the queue's own invariant can fail.
            job.reserved_supply = 1;
            f.world.player_resources[PlayerSlot::ALL[0]].supply_used += 1;
        }
        f.world.step(&TickBundle {
            tick: Tick(f.world.current_tick + 1),
            commands: vec![],
            state_checksum: None,
        });
        let reasons: Vec<&str> = f
            .world
            .diagnostics()
            .iter()
            .filter_map(|diagnostic| match diagnostic {
                SimulationDiagnostic::ProductionInvariant {
                    source: ProductionInvariant::EntityState { reason, .. },
                    ..
                } => Some(*reason),
                _ => None,
            })
            .collect();
        let expected = if corruption == 0 {
            "a job behind the front is in production"
        } else {
            "a job that has not started holds supply"
        };
        assert_eq!(reasons, [expected]);
    }
    Ok(())
}
