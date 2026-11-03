//! Lossless authored production durations and deterministic FIFO completion.
//!
//! Real catalog RON feeds commands and normal simulation ticks. Wide countdowns
//! use explicit near-completion fixtures after verifying enqueue and decrement,
//! avoiding billions of irrelevant ticks while exercising actual completion.

use sim_core::{SimWorld, UnitCatalog, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, ShapeKind, Tick, TickBundle, UnitOrder, UnitTypeId,
};
use std::fmt::Write;
use std::path::Path;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Fixture {
    world: SimWorld,
    producer: EntityId,
    product: UnitTypeId,
}

impl Fixture {
    fn new(duration: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog");
        let source = std::fs::read_to_string(directory.join("units.ron"))?;
        // Rewrite only integer duration values in actual authored definitions.
        // Editing every duration avoids relying on creature identity or file order.
        let marker = "build_time_ticks:";
        let mut edited = String::new();
        let mut rest = source.as_str();
        let mut replacements = 0;
        while let Some(start) = rest.find(marker) {
            let end_of_key = start + marker.len();
            edited.push_str(&rest[..end_of_key]);
            write!(edited, " {duration}")?;
            let end_of_value = rest[end_of_key..]
                .find(',')
                .ok_or("duration has no comma")?;
            rest = &rest[end_of_key + end_of_value..];
            replacements += 1;
        }
        assert!(
            replacements > 0,
            "real catalog must define production durations"
        );
        edited.push_str(rest);
        let mut catalog = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &edited)?;
        assert!(catalog.archetypes().all(|a| a.build_time_ticks == duration));
        catalog.load_structures_from_ron_str(&std::fs::read_to_string(
            directory.join("structures.ron"),
        )?)?;
        let (producer_type, product) = catalog
            .archetypes()
            .filter(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
            .find_map(|producer| {
                producer.trains_units.iter().find_map(|slug| {
                    catalog
                        .get_by_slug(slug)
                        .filter(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }))
                        .map(|product| (producer.unit_type_id, product.unit_type_id))
                })
            })
            .ok_or("catalog has no building with a trainable unit")?;
        // Isolate timing from resource affordability while retaining loaded duration.
        let archetype = catalog.get_mut(product)?;
        archetype.mineral_cost = 0;
        archetype.gas_cost = 0;
        assert_eq!(archetype.build_time_ticks, duration);
        let map = GameMap::new(24, 24, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = SimWorld::with_catalogs(map, 73, catalog, UpgradeCatalog::default());
        let producer = world.spawn_structure(producer_type, PlayerId(0), 5, 5)?;
        Ok(Self {
            world,
            producer,
            product,
        })
    }

    fn bundle(&self, produce: bool) -> TickBundle {
        TickBundle {
            tick: Tick(self.world.current_tick + 1),
            commands: if produce {
                vec![PlayerCommand {
                    player_id: PlayerId(0),
                    entities: vec![self.producer],
                    order: UnitOrder::Produce {
                        unit_type: self.product,
                    },
                    queue: false,
                }]
            } else {
                vec![]
            },
            state_checksum: None,
        }
    }

    fn step(&mut self, produce: bool) {
        self.world.step(&self.bundle(produce));
    }

    fn remaining(&self) -> Option<u32> {
        self.world.entities[&self.producer]
            .production_queue
            .front()
            .map(|job| job.remaining_ticks)
    }
}

#[test]
fn real_ron_duration_survives_enqueue_and_first_update() -> TestResult {
    for duration in [0, 1, 65_535, 65_536, 65_537, u32::MAX] {
        let mut fixture = Fixture::new(duration)?;
        fixture.world.process_commands(&fixture.bundle(true));
        assert_eq!(fixture.remaining(), Some(duration));
        assert_eq!(fixture.world.entities.len(), 1);
        fixture.step(false);
        if duration <= 1 {
            assert_eq!(fixture.remaining(), None);
            assert_eq!(fixture.world.entities.len(), 2);
        } else {
            assert_eq!(fixture.remaining(), Some(duration - 1));
            assert_eq!(fixture.world.entities.len(), 1);
        }
        let mut same_tick = Fixture::new(duration)?;
        same_tick.step(true);
        assert_eq!(same_tick.remaining(), fixture.remaining());
        assert_eq!(same_tick.world.entities, fixture.world.entities);
    }
    Ok(())
}

#[test]
fn wide_jobs_complete_once_at_the_actual_countdown_boundary() -> TestResult {
    for duration in [65_535, 65_536, 65_537, u32::MAX] {
        let mut fixture = Fixture::new(duration)?;
        fixture.step(true);
        assert_eq!(fixture.remaining(), Some(duration - 1));
        // Test-only near-completion fixture: the omitted interval is not simulated.
        fixture
            .world
            .entities
            .get_mut(&fixture.producer)
            .ok_or("producer absent")?
            .production_queue
            .front_mut()
            .ok_or("job absent")?
            .remaining_ticks = 2;
        fixture.step(false);
        assert_eq!(fixture.remaining(), Some(1));
        assert_eq!(fixture.world.entities.len(), 1);
        fixture.step(false);
        assert_eq!(fixture.remaining(), None);
        assert_eq!(fixture.world.entities.len(), 2);
        let trained = fixture
            .world
            .entities
            .values()
            .find(|e| e.id != fixture.producer)
            .ok_or("completion did not spawn")?;
        assert_eq!(trained.unit_type_id, fixture.product);
        assert_eq!(trained.owner, PlayerId(0));
        assert!(trained.is_completed);
        assert!(fixture.world.entities[&fixture.producer].is_completed);
        fixture.step(false);
        fixture.step(false);
        assert_eq!(fixture.remaining(), None);
        assert_eq!(fixture.world.entities.len(), 2);
    }
    Ok(())
}

#[test]
fn fifo_advances_only_one_job_and_zero_jobs_do_not_drain_the_queue() -> TestResult {
    let mut fixture = Fixture::new(65_536)?;
    fixture.step(false);
    assert_eq!(fixture.remaining(), None);
    assert_eq!(fixture.world.entities.len(), 1);
    fixture.step(true);
    let queue = &mut fixture
        .world
        .entities
        .get_mut(&fixture.producer)
        .ok_or("producer absent")?
        .production_queue;
    queue.front_mut().ok_or("job absent")?.remaining_ticks = 1;
    fixture
        .world
        .catalog
        .get_mut(fixture.product)?
        .build_time_ticks = 3;
    fixture
        .world
        .try_queue_production(PlayerId(0), &[fixture.producer], fixture.product)?;
    fixture.step(false);
    assert_eq!(fixture.remaining(), Some(3));
    assert_eq!(fixture.world.entities.len(), 2);
    fixture.step(false);
    assert_eq!(fixture.remaining(), Some(2));
    let queue = &mut fixture
        .world
        .entities
        .get_mut(&fixture.producer)
        .ok_or("producer absent")?
        .production_queue;
    queue.front_mut().ok_or("job absent")?.remaining_ticks = 0;
    fixture
        .world
        .catalog
        .get_mut(fixture.product)?
        .build_time_ticks = 0;
    fixture
        .world
        .try_queue_production(PlayerId(0), &[fixture.producer], fixture.product)?;
    fixture.step(false);
    assert_eq!(fixture.remaining(), Some(0));
    assert_eq!(fixture.world.entities.len(), 3);
    fixture.step(false);
    assert_eq!(fixture.remaining(), None);
    assert_eq!(fixture.world.entities.len(), 4);
    Ok(())
}

#[test]
fn wide_timer_replay_matches_and_high_bits_affect_world_checksum() -> TestResult {
    let mut first = Fixture::new(65_537)?;
    let mut second = Fixture::new(65_537)?;
    for tick in 0..32 {
        first.step(tick == 0);
        second.step(tick == 0);
        assert_eq!(first.world.entities, second.world.entities);
        assert_eq!(
            first.world.compute_checksum(),
            second.world.compute_checksum()
        );
    }
    first
        .world
        .entities
        .get_mut(&first.producer)
        .ok_or("producer absent")?
        .production_queue
        .front_mut()
        .ok_or("job absent")?
        .remaining_ticks = 1;
    second
        .world
        .entities
        .get_mut(&second.producer)
        .ok_or("producer absent")?
        .production_queue
        .front_mut()
        .ok_or("job absent")?
        .remaining_ticks = 65_537;
    assert_ne!(
        first.world.compute_checksum(),
        second.world.compute_checksum()
    );
    Ok(())
}
