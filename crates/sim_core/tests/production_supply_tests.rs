//! Production queues and supply: which job is in production and what supply it holds (plan
//! `2026-11-23-production-queue` D-1, D-2; B-59).
//!
//! Every world here is built from the shipped catalog: the producer is the first structure that
//! provides supply and trains a mobile product, and every price, time and supply is the
//! catalog's. No identity is named. After every tick the supply ledger and the queue invariants
//! are checked through the world's diagnostics.

use sim_core::production::ProductionJob;
use sim_core::{SimWorld, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{EntityId, PlayerId, PlayerSlot, ShapeKind, Tick, TickBundle, UnitTypeId};

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

/// WP1 (D-1): the first job admitted to an idle producer is in production at once and the jobs
/// behind it are not; when the first leaves, the next is in production from the update it first
/// counts down. Supply is still reserved for every job in WP1.
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
    assert_eq!(f.supply_used(), before + 3 * supply);
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
