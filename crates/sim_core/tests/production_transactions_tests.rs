//! Public production transactions using actual external catalog identities.
//!
//! Fixtures alter authored numeric/capability fields, never branch on unit names.
//! Tests prove atomic rejection, captured supply lifetime and connected retry exits.

use sim_core::production::{ProductionAdmissionError as Rejection, ProductionInvariant};
use sim_core::{ActiveHazardZone, SimWorld, UnitCatalog, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{EntityId, PlayerId, ShapeKind, SimScalar, SimVec2, Tick, TickBundle, UnitTypeId};

type TestResult = Result<(), Box<dyn std::error::Error>>;
struct Fixture {
    world: SimWorld,
    producer: EntityId,
    producer_type: UnitTypeId,
    product: UnitTypeId,
}
impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog");
        let mut catalog = sim_core::catalog::RuntimeCatalogs::load_from_directory(directory)?.units;
        let (producer_type, product) = catalog
            .archetypes()
            .filter(|a| a.supply_provided > 0)
            .find_map(|a| {
                a.trains_units.iter().find_map(|slug| {
                    catalog
                        .get_by_slug(slug)
                        .filter(|p| !matches!(p.shape, ShapeKind::BuildingDome { .. }))
                        .map(|p| (a.unit_type_id, p.unit_type_id))
                })
            })
            .ok_or("no authored producer pair")?;
        let a = catalog.get_mut(product)?;
        a.mineral_cost = 7;
        a.gas_cost = 3;
        a.supply_cost = 2;
        a.supply_provided = 0;
        a.build_time_ticks = 1;
        a.collision_radius = SimScalar::from_num(0.375);
        a.weapon = None;
        a.ground_weapon = None;
        a.air_weapon = None;
        let a = catalog.get_mut(producer_type)?;
        a.supply_provided = 20;
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = SimWorld::with_catalogs(map, 4321, catalog, UpgradeCatalog::default());
        let producer = world.spawn_structure(producer_type, PlayerId(0), 10, 10)?;
        Ok(Self {
            world,
            producer,
            producer_type,
            product,
        })
    }
    fn admit(&mut self) -> Result<(), Rejection> {
        self.world
            .try_queue_production(PlayerId(0), &[self.producer], self.product)?;
        Ok(())
    }
    fn tick(&mut self) {
        self.world.step(&TickBundle {
            tick: Tick(self.world.current_tick + 1),
            commands: vec![],
            state_checksum: None,
        });
    }
}
fn point(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y))
}

#[test]
fn rejected_admissions_preserve_authoritative_state() -> TestResult {
    let mut f = Fixture::new()?;
    for (owner, ids, product) in [
        (PlayerId(9), vec![f.producer], f.product),
        (PlayerId(0), vec![], f.product),
        (PlayerId(0), vec![EntityId(999)], f.product),
        (PlayerId(1), vec![f.producer], f.product),
        (PlayerId(0), vec![f.producer], UnitTypeId(u16::MAX)),
        (PlayerId(0), vec![f.producer], f.producer_type),
    ] {
        let before = f.world.compute_checksum();
        assert!(f.world.try_queue_production(owner, &ids, product).is_err());
        assert_eq!(before, f.world.compute_checksum());
        assert_eq!(f.world.diagnostics(), []);
    }
    for field in 0..3 {
        let mut f = Fixture::new()?;
        match field {
            0 => f.world.player_resources[0].minerals = 0,
            1 => f.world.player_resources[0].gas = 0,
            _ => f.world.catalog.get_mut(f.product)?.supply_cost = 21,
        }
        let before = f.world.compute_checksum();
        assert!(f.admit().is_err());
        assert_eq!(before, f.world.compute_checksum());
    }
    let mut f = Fixture::new()?;
    let second = f.world.spawn_unit(f.product, PlayerId(1), point(25, 25))?;
    let before = f.world.compute_checksum();
    assert_eq!(
        f.world
            .try_queue_production(PlayerId(0), &[f.producer, second], f.product),
        Err(Rejection::ForeignRecipient(second))
    );
    assert_eq!(before, f.world.compute_checksum());
    f.world
        .catalog
        .get_mut(f.producer_type)?
        .trains_units
        .clear();
    let before = f.world.compute_checksum();
    assert_eq!(f.admit(), Err(Rejection::NoEligibleProducer));
    assert_eq!(before, f.world.compute_checksum());
    Ok(())
}

#[test]
fn selection_order_and_reservations_are_exact() -> TestResult {
    let mut f = Fixture::new()?;
    let second = f
        .world
        .spawn_structure(f.producer_type, PlayerId(0), 20, 20)?;
    let receipt =
        f.world
            .try_queue_production(PlayerId(0), &[second, second, f.producer], f.product)?;
    assert_eq!(receipt.producer, second);
    assert_eq!(f.world.entities[&second].production_queue.len(), 1);
    assert!(f.world.entities[&f.producer].production_queue.is_empty());
    assert_eq!(f.world.player_resources[0].minerals, 993);
    assert_eq!(f.world.player_resources[0].gas, 497);
    assert_eq!(f.world.player_resources[0].supply_used, 2);
    for _ in 0..19 {
        f.admit()?;
    }
    let before = f.world.compute_checksum();
    assert!(matches!(f.admit(), Err(Rejection::Supply { .. })));
    assert_eq!(before, f.world.compute_checksum());
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn setup_failure_does_not_consume_ids_or_supply() -> TestResult {
    let mut f = Fixture::new()?;
    for (x, y) in [(10, 10), (u32::MAX, u32::MAX)] {
        let before = f.world.compute_checksum();
        assert!(f
            .world
            .spawn_structure(f.producer_type, PlayerId(0), x, y)
            .is_err());
        assert_eq!(before, f.world.compute_checksum());
    }
    for id in [f.producer.0, u32::MAX] {
        f.world.next_entity_id = id;
        let before = f.world.compute_checksum();
        assert!(f
            .world
            .spawn_unit(f.product, PlayerId(0), point(1, 1))
            .is_err());
        assert_eq!(before, f.world.compute_checksum());
    }
    f.world.next_entity_id = 50;
    let before = f.world.compute_checksum();
    assert!(f
        .world
        .spawn_unit(f.product, PlayerId(9), point(1, 1))
        .is_err());
    assert_eq!(before, f.world.compute_checksum());
    Ok(())
}

#[test]
fn captured_supply_survives_catalog_edits_and_provider_loss() -> TestResult {
    let mut f = Fixture::new()?;
    f.world.catalog.get_mut(f.producer_type)?.supply_provided = 190;
    let second = f
        .world
        .spawn_structure(f.producer_type, PlayerId(0), 20, 20)?;
    assert_eq!(f.world.player_resources[0].supply_provided_total, 210);
    assert_eq!(f.world.player_resources[0].supply_max, 200);
    f.admit()?;
    f.admit()?;
    let unit = f.world.spawn_unit(f.product, PlayerId(0), point(2, 2))?;
    f.world.catalog.get_mut(f.product)?.supply_cost = 99;
    f.world.entities.get_mut(&unit).ok_or("unit absent")?.hp = SimScalar::ZERO;
    f.world
        .entities
        .get_mut(&f.producer)
        .ok_or("producer absent")?
        .hp = SimScalar::ZERO;
    f.world.reap_dead_and_emit_shatter();
    assert_eq!(f.world.diagnostics(), []);
    assert_eq!(f.world.player_resources[0].supply_used, 0);
    assert_eq!(f.world.player_resources[0].supply_provided_total, 190);
    assert_eq!(f.world.player_resources[0].supply_max, 190);
    assert!(f.world.entities.contains_key(&second));
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn dead_and_hazard_killed_producers_never_finish_or_leak_supply() -> TestResult {
    for hazard in [false, true] {
        let mut f = Fixture::new()?;
        f.admit()?;
        f.world.catalog.get_mut(f.producer_type)?.hp_regen_per_tick = SimScalar::from_i32(5);
        let position = f.world.entities[&f.producer].position;
        if hazard {
            f.world.active_hazards.push(ActiveHazardZone {
                id: 1,
                owner: PlayerId(1),
                center: position,
                radius: SimScalar::from_i32(2),
                delay_ticks: 0,
                remaining_ticks: 2,
                interval_ticks: 1,
                ticks_until_next_interval: 0,
                damage_per_interval: SimScalar::from_i32(100_000),
                targets_friendly: true,
            });
        } else {
            f.world
                .entities
                .get_mut(&f.producer)
                .ok_or("producer absent")?
                .hp = SimScalar::ZERO;
        }
        let next = f.world.next_entity_id;
        f.tick();
        assert!(f.world.entities.is_empty());
        assert_eq!(f.world.next_entity_id, next);
        assert_eq!(f.world.player_resources[0].supply_used, 0);
        assert_eq!(f.world.player_resources[0].supply_provided_total, 0);
        assert_eq!(f.world.player_resources[0].minerals, 993);
        assert_eq!(f.world.diagnostics(), []);
        f.world.validate_supply_ledger()?;
    }
    Ok(())
}

#[test]
fn clear_but_disconnected_plateau_is_not_a_production_exit() -> TestResult {
    let mut f = Fixture::new()?;
    let foundation = f.world.map.structures()[&f.producer].clone();
    for y in 0..32 {
        for x in 0..32 {
            if f.world.map.cell(x, y)?.occupying_structure.is_none() {
                f.world.map.cell_mut(x, y)?.cliff_level = CliffLevel(1);
            }
        }
    }
    f.admit()?;
    let next = f.world.next_entity_id;
    for _ in 0..3 {
        f.world.update_production_and_buffs();
    }
    assert_eq!(f.world.entities.len(), 1);
    assert_eq!(f.world.next_entity_id, next);
    assert_eq!(
        f.world.entities[&f.producer]
            .production_queue
            .front()
            .ok_or("lost job")?
            .remaining_ticks,
        0
    );
    assert_eq!(f.world.production_blocks().len(), 1);
    assert_eq!(f.world.diagnostics(), []);
    // Restore the adjacent lower boundary. No remote destination is introduced.
    let y = foundation.top_left.1 - 1;
    for x in foundation.top_left.0.saturating_sub(1)
        ..=foundation.top_left.0 + u32::from(foundation.width)
    {
        f.world.map.cell_mut(x, y)?.cliff_level = CliffLevel(0);
    }
    f.world.update_production_and_buffs();
    assert_eq!(f.world.entities.len(), 2);
    assert!(f.world.entities[&f.producer].production_queue.is_empty());
    assert_eq!(f.world.player_resources[0].supply_used, 2);
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn corrupt_ready_job_and_allocator_emit_evidence_without_losing_job() -> TestResult {
    for mutation in 0..3 {
        let mut f = Fixture::new()?;
        f.admit()?;
        match mutation {
            0 => {
                f.world
                    .entities
                    .get_mut(&f.producer)
                    .ok_or("producer absent")?
                    .production_queue
                    .front_mut()
                    .ok_or("job absent")?
                    .product = UnitTypeId(u16::MAX);
            }
            1 => f.world.next_entity_id = u32::MAX,
            _ => f.world.player_resources[0].supply_used += 1,
        }
        let economy = f.world.player_resources[0];
        let next = f.world.next_entity_id;
        f.world.update_production_and_buffs();
        assert_eq!(f.world.entities.len(), 1);
        assert_eq!(f.world.entities[&f.producer].production_queue.len(), 1);
        assert_eq!(f.world.player_resources[0], economy);
        assert_eq!(f.world.next_entity_id, next);
        assert_ne!(f.world.diagnostics(), []);
    }
    let mut f = Fixture::new()?;
    f.world
        .entities
        .get_mut(&f.producer)
        .ok_or("producer absent")?
        .unit_type_id = UnitTypeId(u16::MAX);
    let before = f.world.compute_checksum();
    assert!(matches!(
        f.admit(),
        Err(Rejection::Invariant(
            ProductionInvariant::MissingArchetype { .. }
        ))
    ));
    assert_eq!(before, f.world.compute_checksum());
    Ok(())
}

#[test]
fn catalog_reference_validation_is_complete_bundle_only() -> TestResult {
    let mut f = Fixture::new()?;
    let slug = f.world.catalog.get(f.product)?.slug.clone();
    f.world.catalog.get_mut(f.producer_type)?.trains_units =
        vec![slug.clone(), slug.to_uppercase()];
    assert!(f.world.catalog.validate_production_references().is_err());
    for invalid in ["", "missing-test-reference"] {
        f.world.catalog.get_mut(f.producer_type)?.trains_units = vec![invalid.to_owned()];
        assert!(f.world.catalog.validate_production_references().is_err());
    }
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog/units.ron"),
    )?;
    assert!(UnitCatalog::from_ron_str(&source).is_ok());
    Ok(())
}

#[test]
fn replay_keeps_transaction_and_supply_state_identical() -> TestResult {
    let mut a = Fixture::new()?;
    let mut b = Fixture::new()?;
    for tick in 0..2000 {
        if tick % 400 == 0 {
            a.admit()?;
            b.admit()?;
        }
        a.tick();
        b.tick();
        assert_eq!(a.world.compute_checksum(), b.world.compute_checksum());
        assert_eq!(a.world.entities, b.world.entities);
    }
    a.world.validate_supply_ledger()?;
    b.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn mutated_producer_definition_fails_before_countdown_changes() -> TestResult {
    for change in 0..4 {
        let mut f = Fixture::new()?;
        f.world.catalog.get_mut(f.product)?.build_time_ticks = 10;
        f.admit()?;
        match change {
            0 => {
                f.world
                    .entities
                    .get_mut(&f.producer)
                    .ok_or("producer absent")?
                    .unit_type_id = UnitTypeId(u16::MAX);
            }
            1 => f
                .world
                .catalog
                .get_mut(f.producer_type)?
                .trains_units
                .clear(),
            2 => f.world.catalog.get_mut(f.producer_type)?.collision_radius = SimScalar::ZERO,
            _ => {
                f.world.catalog.get_mut(f.producer_type)?.shape = ShapeKind::BuildingDome {
                    footprint_w: 0,
                    footprint_h: 1,
                }
            }
        }
        f.world.update_production_and_buffs();
        assert_eq!(
            f.world.entities[&f.producer]
                .production_queue
                .front()
                .ok_or("lost paid job")?
                .remaining_ticks,
            10
        );
        assert_eq!(f.world.player_resources[0].supply_used, 2);
        assert_ne!(f.world.diagnostics(), []);
        assert_eq!(f.world.production_blocks(), []);
    }
    Ok(())
}

#[test]
fn real_structure_ring_preserves_paid_ready_job_until_removal() -> TestResult {
    for radius in [SimScalar::from_num(0.375), SimScalar::from_num(1.125)] {
        let mut f = Fixture::new()?;
        f.world.catalog.get_mut(f.product)?.collision_radius = radius;
        let blocker_type = f
            .world
            .catalog
            .archetypes()
            .find(|a| {
                a.unit_type_id != f.producer_type
                    && matches!(a.shape, ShapeKind::BuildingDome { .. })
            })
            .ok_or("no second structure")?
            .unit_type_id;
        let arch = f.world.catalog.get_mut(blocker_type)?;
        arch.shape = ShapeKind::BuildingDome {
            footprint_w: 1,
            footprint_h: 1,
        };
        arch.supply_provided = 0;
        arch.collision_radius = SimScalar::from_num(0.4);
        let placed = f.world.map.structures()[&f.producer].clone();
        let min_x = placed.top_left.0 - 1;
        let min_y = placed.top_left.1 - 1;
        let max_x = placed.top_left.0 + u32::from(placed.width);
        let max_y = placed.top_left.1 + u32::from(placed.height);
        let mut blockers = Vec::new();
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if x == min_x || x == max_x || y == min_y || y == max_y {
                    blockers.push(f.world.spawn_structure(blocker_type, PlayerId(0), x, y)?);
                }
            }
        }
        f.admit()?;
        let next = f.world.next_entity_id;
        let economy = f.world.player_resources[0];
        for _ in 0..3 {
            f.world.update_production_and_buffs();
        }
        assert_eq!(f.world.next_entity_id, next);
        assert_eq!(f.world.player_resources[0], economy);
        assert_eq!(f.world.entities[&f.producer].production_queue.len(), 1);
        assert_eq!(f.world.production_blocks().len(), 1);
        assert_eq!(f.world.diagnostics(), []);
        for id in blockers {
            f.world.entities.get_mut(&id).ok_or("blocker absent")?.hp = SimScalar::ZERO;
        }
        f.world.reap_dead_and_emit_shatter();
        f.world.update_production_and_buffs();
        assert_eq!(f.world.entities.len(), 2);
        assert_eq!(f.world.entities[&f.producer].production_queue.len(), 0);
        assert_eq!(f.world.player_resources[0].minerals, economy.minerals);
        f.world.validate_supply_ledger()?;
    }
    Ok(())
}

#[test]
fn two_mobile_producers_contend_for_one_exit_in_stable_id_order() -> TestResult {
    let mut f = Fixture::new()?;
    let source_type = f
        .world
        .catalog
        .archetypes()
        .find(|a| a.unit_type_id != f.product && !matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .ok_or("no second mobile")?
        .unit_type_id;
    let slug = f.world.catalog.get(f.product)?.slug.clone();
    let source = f.world.catalog.get_mut(source_type)?;
    source.collision_radius = SimScalar::from_num(0.1);
    source.supply_provided = 20;
    source.supply_cost = 0;
    source.trains_units = vec![slug];
    let center = SimVec2::new(SimScalar::from_num(3.5), SimScalar::from_num(3.5));
    let first = f.world.spawn_unit(source_type, PlayerId(0), center)?;
    let second = f.world.spawn_unit(source_type, PlayerId(0), center)?;
    for y in 2..=4 {
        for x in 2..=4 {
            if (x == 3 && y == 3) || (x == 2 && y == 2) {
                continue;
            }
            f.world.spawn_unit(
                f.product,
                PlayerId(0),
                SimVec2::new(
                    SimScalar::from_num(x) + SimScalar::HALF,
                    SimScalar::from_num(y) + SimScalar::HALF,
                ),
            )?;
        }
    }
    for producer in [first, second] {
        f.world
            .try_queue_production(PlayerId(0), &[producer], f.product)?;
    }
    let next = f.world.next_entity_id;
    f.world.update_production_and_buffs();
    assert_eq!(f.world.next_entity_id, next + 1);
    assert_eq!(f.world.entities[&first].production_queue.len(), 0);
    assert_eq!(f.world.entities[&second].production_queue.len(), 1);
    assert_eq!(
        f.world.production_blocks(),
        [sim_core::production::ProductionBlock::ExitObstructed { producer: second }]
    );
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn air_training_ignores_ground_walls_but_respects_air_occupants() -> TestResult {
    let mut f = Fixture::new()?;
    f.world.catalog.get_mut(f.product)?.mover_type = sim_types::MoverType::Air;
    let placed = f.world.map.structures()[&f.producer].clone();
    let mut blockers = Vec::new();
    for y in placed.top_left.1 - 1..=placed.top_left.1 + u32::from(placed.height) {
        for x in placed.top_left.0 - 1..=placed.top_left.0 + u32::from(placed.width) {
            if f.world.map.cell(x, y)?.occupying_structure.is_none() {
                f.world
                    .map
                    .cell_mut(x, y)?
                    .flags
                    .remove(sim_map::CellFlags::GROUND_PATHABLE);
                blockers.push(f.world.spawn_unit(
                    f.product,
                    PlayerId(1),
                    SimVec2::new(
                        SimScalar::from_num(x) + SimScalar::HALF,
                        SimScalar::from_num(y) + SimScalar::HALF,
                    ),
                )?);
            }
        }
    }
    f.admit()?;
    let next = f.world.next_entity_id;
    f.world.update_production_and_buffs();
    assert_eq!(f.world.next_entity_id, next);
    assert_eq!(f.world.production_blocks().len(), 1);
    for id in blockers {
        f.world
            .entities
            .get_mut(&id)
            .ok_or("air blocker absent")?
            .hp = SimScalar::ZERO;
    }
    f.world.reap_dead_and_emit_shatter();
    f.world.update_production_and_buffs();
    assert_eq!(f.world.next_entity_id, next + 1);
    assert_eq!(f.world.entities[&f.producer].production_queue.len(), 0);
    assert_eq!(f.world.diagnostics(), []);
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn prerequisite_and_incomplete_selection_reject_without_debit() -> TestResult {
    let mut f = Fixture::new()?;
    f.world
        .entities
        .get_mut(&f.producer)
        .ok_or("producer absent")?
        .is_completed = false;
    let before = f.world.compute_checksum();
    assert_eq!(f.admit(), Err(Rejection::NoEligibleProducer));
    assert_eq!(before, f.world.compute_checksum());
    f.world
        .entities
        .get_mut(&f.producer)
        .ok_or("producer absent")?
        .is_completed = true;
    let prerequisite = f
        .world
        .catalog
        .archetypes()
        .find(|a| {
            a.unit_type_id != f.producer_type && matches!(a.shape, ShapeKind::BuildingDome { .. })
        })
        .ok_or("no prerequisite")?
        .clone();
    f.world.catalog.get_mut(f.product)?.requires_structures = vec![prerequisite.slug];
    let before = f.world.compute_checksum();
    assert_eq!(
        f.admit(),
        Err(Rejection::MissingPrerequisite(prerequisite.unit_type_id))
    );
    assert_eq!(before, f.world.compute_checksum());
    f.world
        .spawn_structure(prerequisite.unit_type_id, PlayerId(0), 22, 22)?;
    f.admit()?;
    Ok(())
}

#[test]
fn ramp_orientation_and_map_edge_exits_use_actual_connectivity() -> TestResult {
    let mut f = Fixture::new()?;
    let placed = f.world.map.structures()[&f.producer].clone();
    for y in placed.top_left.1..placed.top_left.1 + u32::from(placed.height) {
        for x in placed.top_left.0..placed.top_left.0 + u32::from(placed.width) {
            f.world.map.cell_mut(x, y)?.cliff_level = CliffLevel(1);
        }
    }
    let y = placed.top_left.1 - 1;
    for x in placed.top_left.0..placed.top_left.0 + u32::from(placed.width) {
        let cell = f.world.map.cell_mut(x, y)?;
        cell.ramp = Some(sim_map::RampDir::East);
        cell.flags = sim_map::CellFlags::GROUND_PATHABLE | sim_map::CellFlags::RAMP;
    }
    f.admit()?;
    f.world.update_production_and_buffs();
    assert_eq!(f.world.entities.len(), 1);
    assert_eq!(f.world.production_blocks().len(), 1);
    for x in placed.top_left.0..placed.top_left.0 + u32::from(placed.width) {
        f.world.map.cell_mut(x, y)?.ramp = Some(sim_map::RampDir::North);
    }
    f.world.update_production_and_buffs();
    assert_eq!(f.world.entities.len(), 2);
    assert_eq!(f.world.diagnostics(), []);
    let child = f
        .world
        .entities
        .values()
        .find(|entity| entity.id != f.producer)
        .ok_or("no child")?;
    let (x, y) = f.world.map.world_to_cell(child.position)?;
    assert_eq!(f.world.map.cell(x, y)?.ramp, Some(sim_map::RampDir::North));

    let mut f = Fixture::new()?;
    f.world
        .entities
        .get_mut(&f.producer)
        .ok_or("producer absent")?
        .hp = SimScalar::ZERO;
    f.world.reap_dead_and_emit_shatter();
    f.producer = f
        .world
        .spawn_structure(f.producer_type, PlayerId(0), 0, 0)?;
    f.admit()?;
    f.world.update_production_and_buffs();
    assert_eq!(f.world.entities.len(), 2);
    assert_eq!(f.world.diagnostics(), []);
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn command_outcomes_preserve_input_indices_and_are_derived() -> TestResult {
    let mut f = Fixture::new()?;
    let bundle = TickBundle {
        tick: Tick(1),
        commands: vec![
            sim_types::PlayerCommand {
                player_id: PlayerId(9),
                entities: vec![f.producer],
                order: sim_types::UnitOrder::Produce {
                    unit_type: f.product,
                },
                queue: false,
            },
            sim_types::PlayerCommand {
                player_id: PlayerId(0),
                entities: vec![],
                order: sim_types::UnitOrder::Produce {
                    unit_type: f.product,
                },
                queue: false,
            },
        ],
        state_checksum: None,
    };
    let before = f.world.compute_checksum();
    f.world.process_commands(&bundle);
    assert_eq!(f.world.compute_checksum(), before);
    assert_eq!(f.world.production_outcomes().len(), 2);
    assert_eq!(f.world.production_outcomes()[0].command_index, 0);
    assert_eq!(
        f.world.production_outcomes()[0].result,
        Err(Rejection::InvalidPlayer(PlayerId(9)))
    );
    assert_eq!(f.world.production_outcomes()[1].command_index, 1);
    assert_eq!(
        f.world.production_outcomes()[1].result,
        Err(Rejection::EmptySelection)
    );
    assert_eq!(f.world.drain_production_outcomes().count(), 2);
    assert_eq!(f.world.production_outcomes(), []);
    assert_eq!(f.world.compute_checksum(), before);
    f.world.process_commands(&bundle);
    f.tick();
    assert_eq!(f.world.production_outcomes(), []);
    Ok(())
}

#[test]
fn checked_supply_and_vitals_overflow_reject_before_spawn() -> TestResult {
    let mut f = Fixture::new()?;
    f.world.catalog.get_mut(f.product)?.supply_provided = u32::MAX;
    let before = f.world.compute_checksum();
    assert!(f
        .world
        .spawn_unit(f.product, PlayerId(0), point(2, 2))
        .is_err());
    assert_eq!(f.world.compute_checksum(), before);
    f.world.catalog.get_mut(f.product)?.supply_provided = 0;
    f.world.catalog.get_mut(f.product)?.max_hp = SimScalar::from_raw(i64::MAX);
    let slug = f.world.catalog.get(f.product)?.slug.clone();
    let modifier = f.world.player_upgrades[0]
        .unit_stat_modifiers
        .entry(slug)
        .or_default();
    modifier.max_hp_delta = SimScalar::ONE;
    let before = f.world.compute_checksum();
    assert!(f
        .world
        .spawn_unit(f.product, PlayerId(0), point(2, 2))
        .is_err());
    assert_eq!(f.world.compute_checksum(), before);
    Ok(())
}

#[test]
fn fractional_mobile_producer_keeps_paid_job_until_its_full_exit_is_clear() -> TestResult {
    let mut f = Fixture::new()?;
    let source_type = f
        .world
        .catalog
        .archetypes()
        .find(|a| a.unit_type_id != f.product && !matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .ok_or("no second mobile")?
        .unit_type_id;
    let wall_type = f
        .world
        .catalog
        .archetypes()
        .find(|a| {
            a.unit_type_id != f.producer_type && matches!(a.shape, ShapeKind::BuildingDome { .. })
        })
        .ok_or("no wall definition")?
        .unit_type_id;
    let slug = f.world.catalog.get(f.product)?.slug.clone();
    let source = f.world.catalog.get_mut(source_type)?;
    source.collision_radius = SimScalar::from_num(0.1);
    source.supply_cost = 0;
    source.supply_provided = 10;
    source.trains_units = vec![slug];
    let wall = f.world.catalog.get_mut(wall_type)?;
    wall.shape = ShapeKind::BuildingDome {
        footprint_w: 1,
        footprint_h: 1,
    };
    wall.supply_provided = 0;
    wall.collision_radius = SimScalar::from_num(0.4);
    let blocker = f.world.spawn_structure(wall_type, PlayerId(0), 2, 2)?;
    let origin = SimVec2::new(SimScalar::from_num(3.1), SimScalar::from_num(2.5));
    let producer = f.world.spawn_unit(source_type, PlayerId(0), origin)?;
    f.world
        .try_queue_production(PlayerId(0), &[producer], f.product)?;
    let next = f.world.next_entity_id;
    let economy = f.world.player_resources[0];
    for _ in 0..3 {
        f.world.update_production_and_buffs();
    }
    assert_eq!(f.world.next_entity_id, next);
    assert_eq!(f.world.player_resources[0], economy);
    assert_eq!(f.world.entities[&producer].position, origin);
    assert_eq!(
        f.world.entities[&producer]
            .production_queue
            .front()
            .ok_or("lost paid job")?
            .remaining_ticks,
        0
    );
    assert_eq!(
        f.world.production_blocks(),
        [sim_core::production::ProductionBlock::ExitObstructed { producer }]
    );
    assert_eq!(f.world.diagnostics(), []);
    f.world.entities.get_mut(&blocker).ok_or("wall absent")?.hp = SimScalar::ZERO;
    f.world.reap_dead_and_emit_shatter();
    f.world.update_production_and_buffs();
    assert_eq!(f.world.next_entity_id, next + 1);
    assert_eq!(f.world.entities[&producer].production_queue.len(), 0);
    assert_eq!(f.world.player_resources[0].minerals, economy.minerals);
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn extreme_mobile_source_radius_rejects_before_perimeter_enumeration() -> TestResult {
    let mut f = Fixture::new()?;
    let source_type = f
        .world
        .catalog
        .archetypes()
        .find(|a| a.unit_type_id != f.product && !matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .ok_or("no second mobile")?
        .unit_type_id;
    let slug = f.world.catalog.get(f.product)?.slug.clone();
    let source = f.world.catalog.get_mut(source_type)?;
    source.collision_radius = SimScalar::from_num(0.1);
    source.supply_cost = 0;
    source.supply_provided = 10;
    source.trains_units = vec![slug];
    let producer = f.world.spawn_unit(source_type, PlayerId(0), point(3, 3))?;
    f.world.catalog.get_mut(source_type)?.collision_radius = SimScalar::from_raw(i64::MAX);
    let before = f.world.compute_checksum();
    assert!(matches!(
        f.world
            .try_queue_production(PlayerId(0), &[producer], f.product),
        Err(Rejection::Invariant(ProductionInvariant::EntityState {
            reason: "mobile producer footprint outside map",
            ..
        }))
    ));
    assert_eq!(f.world.compute_checksum(), before);
    f.world.catalog.get_mut(source_type)?.collision_radius = SimScalar::from_num(0.1);
    f.world.catalog.get_mut(f.product)?.build_time_ticks = 10;
    f.world
        .try_queue_production(PlayerId(0), &[producer], f.product)?;
    f.world.catalog.get_mut(source_type)?.collision_radius = SimScalar::from_raw(i64::MAX);
    let next = f.world.next_entity_id;
    let economy = f.world.player_resources[0];
    f.world.update_production_and_buffs();
    assert_eq!(
        f.world.entities[&producer]
            .production_queue
            .front()
            .ok_or("lost job")?
            .remaining_ticks,
        10
    );
    assert_eq!(f.world.next_entity_id, next);
    assert_eq!(f.world.player_resources[0], economy);
    assert_eq!(f.world.production_blocks(), []);
    Ok(())
}
