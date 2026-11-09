//! Ground/Air behavior through real catalog admission, full ticks and exact placement.
//! Fixtures select authored capabilities rather than identity strings and preserve
//! captured supply lifecycle. Direct phase tests isolate displacement boundaries.

use sim_core::catalog::RuntimeCatalogs;
use sim_core::movement::{DisplacementAdmissionError, MovementAdmissionError};
use sim_core::transport::TransportAdmissionError;
use sim_core::{SimWorld, UpgradeCatalog};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, EntityId, MoverType, PlayerCommand,
    PlayerId, PlayerSlot, ShapeKind, SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
struct Fixture {
    world: SimWorld,
    ground: UnitTypeId,
    air: UnitTypeId,
}
impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog");
        let mut catalog = RuntimeCatalogs::load_from_directory(root)?.units;
        let select = |mover| {
            catalog
                .archetypes()
                .find(|a| {
                    a.mover_type == mover
                        && !matches!(a.shape, ShapeKind::BuildingDome { .. })
                        && !a.is_immobile_by_default()
                })
                .map(|a| a.unit_type_id)
        };
        let ground = select(MoverType::Ground).ok_or("no Ground actor")?;
        let air = select(MoverType::Air).ok_or("no Air actor")?;
        for id in [ground, air] {
            let a = catalog.get_mut(id)?;
            a.max_speed = SimScalar::from_num(0.25);
            a.collision_radius = SimScalar::from_num(0.375);
            a.weapon = None;
            a.ground_weapon = None;
            a.air_weapon = None;
        }
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = SimWorld::with_catalogs(map, 45, catalog, UpgradeCatalog::default());
        world.vision_aware_pathfinding = false;
        Ok(Self { world, ground, air })
    }
    fn spawn(
        &mut self,
        kind: UnitTypeId,
        owner: u8,
        x: f64,
        y: f64,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        Ok(self.world.spawn_unit(kind, PlayerId(owner), point(x, y))?)
    }
    fn step(&mut self, commands: Vec<PlayerCommand>) {
        self.world.step(&TickBundle {
            tick: Tick(self.world.current_tick),
            commands,
            state_checksum: None,
        });
    }
}
fn point(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}
fn command(ids: Vec<EntityId>, target: SimVec2, attack: bool) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(0),
        entities: ids,
        order: if attack {
            UnitOrder::AttackMove { target }
        } else {
            UnitOrder::Move { target }
        },
        queue: false,
    }
}

#[test]
fn real_move_and_attack_move_cross_ground_barriers_only_in_air() -> TestResult {
    for attack in [false, true] {
        let mut f = Fixture::new()?;
        for y in 0..32 {
            f.world.map.cell_mut(12, y)?.flags = CellFlags::UNPASSABLE_CLIFF;
        }
        f.world.map.place_structure(EntityId(9999), 17, 10, 2, 2)?;
        f.world.pathfinder.recompute_clearance(&f.world.map);
        let ground = f.spawn(f.ground, 0, 8.5, 10.5)?;
        let air = f.spawn(f.air, 0, 8.5, 10.5)?;
        f.step(vec![command(vec![ground, air], point(22.5, 10.5), attack)]);
        assert_eq!(f.world.movement_outcomes().len(), 1);
        for _ in 0..100 {
            f.step(vec![]);
        }
        assert_eq!(f.world.entities[&air].position, point(22.5, 10.5));
        assert!(f.world.entities[&ground].position.x < SimScalar::from_num(12));
        assert_eq!(f.world.entities[&air].current_funnel_waypoints, []);
        assert!(
            f.world.diagnostics().is_empty(),
            "{:?}",
            f.world.diagnostics()
        );
        f.world.validate_supply_ledger()?;
    }
    Ok(())
}

#[test]
fn command_rejection_is_atomic_and_duplicate_queue_recipients_commit_once() -> TestResult {
    let mut f = Fixture::new()?;
    let small = f.spawn(f.ground, 0, 5.0, 5.0)?;
    let large = f.spawn(f.air, 0, 8.0, 8.0)?;
    f.world.catalog.get_mut(f.air)?.collision_radius = SimScalar::from_num(2);
    let before = f.world.entities.clone();
    let checksum = f.world.compute_checksum();
    let invalid = UnitOrder::Move {
        target: point(1.0, 5.0),
    };
    assert_eq!(
        f.world
            .try_issue_movement(PlayerId(0), &[small, large], &invalid, false),
        Err(MovementAdmissionError::OutsideDomain(large))
    );
    assert_eq!(f.world.entities, before);
    assert_eq!(f.world.compute_checksum(), checksum);
    let valid = UnitOrder::AttackMove {
        target: point(20.0, 20.0),
    };
    let receipt = f
        .world
        .try_issue_movement(PlayerId(0), &[small, small, large], &valid, true)?;
    assert_eq!(receipt.recipient_count, 2);
    assert_eq!(f.world.entities[&small].order_queue.len(), 1);
    assert_eq!(f.world.entities[&large].order_queue.len(), 1);
    Ok(())
}

#[test]
fn movement_outcomes_retain_original_command_indices_and_clear_each_step() -> TestResult {
    let mut f = Fixture::new()?;
    let id = f.spawn(f.air, 0, 5.0, 5.0)?;
    let mut reject = command(vec![id], point(0.0, 5.0), false);
    reject.player_id = PlayerId(7);
    let produce = PlayerCommand {
        player_id: PlayerId(0),
        entities: vec![id],
        order: UnitOrder::Produce {
            unit_type: f.ground,
        },
        queue: false,
    };
    f.step(vec![
        produce,
        reject,
        command(vec![id], point(10.0, 5.0), false),
    ]);
    assert_eq!(f.world.production_outcomes()[0].command_index, 0);
    let outcomes: Vec<_> = f.world.drain_movement_outcomes().collect();
    assert_eq!(
        outcomes
            .iter()
            .map(|o| (o.tick, o.command_index))
            .collect::<Vec<_>>(),
        vec![(Tick(1), 1), (Tick(1), 2)]
    );
    assert!(outcomes[0].result.is_err());
    assert!(outcomes[1].result.is_ok());
    assert_eq!(f.world.diagnostics(), []);
    f.step(vec![]);
    assert_eq!(f.world.movement_outcomes(), []);
    Ok(())
}

#[test]
fn different_layers_overlap_without_push_but_unequal_same_layer_radii_are_found() -> TestResult {
    for reverse in [false, true] {
        let mut f = Fixture::new()?;
        let ground = f.spawn(f.ground, 0, 8.0, 8.0)?;
        let air = f.spawn(f.air, 1, 8.0, 8.0)?;
        let before = f.world.entities.clone();
        f.world.resolve_collisions_and_push();
        assert_eq!(f.world.entities, before);
        f.world.catalog.get_mut(f.ground)?.collision_radius = SimScalar::from_num(0.2);
        f.world.catalog.get_mut(f.air)?.mover_type = MoverType::Ground;
        f.world.catalog.get_mut(f.air)?.collision_radius = SimScalar::from_num(2.1);
        f.world.entities.get_mut(&ground).ok_or("ground")?.position =
            point(if reverse { 8.01 } else { 5.99 }, 8.0);
        f.world.entities.get_mut(&air).ok_or("air")?.position =
            point(if reverse { 5.99 } else { 8.01 }, 8.0);
        let before = (
            f.world.entities[&ground].position,
            f.world.entities[&air].position,
        );
        f.world.resolve_collisions_and_push();
        assert_ne!(
            (
                f.world.entities[&ground].position,
                f.world.entities[&air].position
            ),
            before
        );
        assert_eq!(f.world.diagnostics(), []);
    }
    Ok(())
}

#[test]
fn exact_unload_checks_footprint_layer_bounds_and_preserves_failed_state() -> TestResult {
    let mut f = Fixture::new()?;
    let ground = f.spawn(f.ground, 0, 4.0, 4.0)?;
    let air = f.spawn(f.air, 0, 4.0, 4.0)?;
    f.world.map.place_structure(EntityId(999), 10, 10, 1, 1)?;
    assert!(f.world.load_unit_into_transport(ground, air));
    let before = f.world.entities[&ground].clone();
    let checksum = f.world.compute_checksum();
    assert!(matches!(
        f.world
            .try_unload_unit_from_transport(ground, point(11.25, 10.5)),
        Err(TransportAdmissionError::Destination(
            DisplacementAdmissionError::Terrain(_)
        ))
    ));
    assert_eq!(f.world.entities[&ground], before);
    assert_eq!(f.world.compute_checksum(), checksum);
    assert!(matches!(
        f.world
            .try_unload_unit_from_transport(ground, point(0.25, 5.0)),
        Err(TransportAdmissionError::Destination(
            DisplacementAdmissionError::OutsideDomain(_)
        ))
    ));
    f.world
        .try_unload_unit_from_transport(ground, point(11.375, 10.5))?;
    assert!(f.world.load_unit_into_transport(air, ground));
    f.world
        .try_unload_unit_from_transport(air, point(10.5, 10.5))?;
    assert_eq!(f.world.entities[&air].position, point(10.5, 10.5));
    let second = f.spawn(f.air, 0, 15.0, 10.5)?;
    assert!(f.world.load_unit_into_transport(second, ground));
    assert!(
        matches!(f.world.try_unload_unit_from_transport(second,point(10.5,10.5)),Err(TransportAdmissionError::Destination(DisplacementAdmissionError::Occupied(id))) if id==air)
    );
    f.world
        .try_unload_unit_from_transport(second, point(11.25, 10.5))?; // exact circle tangency
    Ok(())
}

#[test]
fn blink_rejection_is_before_energy_cooldown_and_all_entity_mutation() -> TestResult {
    let mut f = Fixture::new()?;
    let id = f.spawn(f.ground, 0, 5.0, 5.0)?;
    f.world.catalog.get_mut(f.ground)?.abilities = vec![AbilitySpec {
        name: "placement test".into(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::from_num(3),
        cooldown_ticks: 17,
        cast_range: SimScalar::from_num(10),
        effect: AbilityEffectSpec::BlinkTeleport {
            max_range: SimScalar::from_num(10),
            cooldown_ticks: 17,
        },
        leave_name: None,
    }];
    f.world.entities.get_mut(&id).ok_or("actor")?.energy = SimScalar::from_num(20);
    f.world.map.place_structure(EntityId(999), 10, 5, 1, 1)?;
    let before = f.world.entities[&id].clone();
    let checksum = f.world.compute_checksum();
    assert!(!f
        .world
        .execute_ability(id, 0, Some(point(11.25, 5.5)), None));
    assert_eq!(f.world.entities[&id], before);
    assert_eq!(f.world.compute_checksum(), checksum);
    f.world.catalog.get_mut(f.ground)?.mover_type = MoverType::Air;
    assert!(f.world.execute_ability(id, 0, Some(point(10.5, 5.5)), None));
    assert_eq!(f.world.entities[&id].energy, SimScalar::from_num(17));
    assert_eq!(f.world.entities[&id].ability_cooldowns[0], 17);
    assert_eq!(f.world.entities[&id].position, point(10.5, 5.5));
    Ok(())
}

#[test]
fn policy_mutation_clears_air_routes_and_invalid_radius_is_diagnosed() -> TestResult {
    let mut f = Fixture::new()?;
    let id = f.spawn(f.ground, 0, 5.0, 5.0)?;
    f.world
        .entities
        .get_mut(&id)
        .ok_or("actor")?
        .current_funnel_waypoints = vec![point(5.0, 10.0)];
    f.world
        .entities
        .get_mut(&id)
        .ok_or("actor")?
        .order_queue
        .push_back(UnitOrder::Move {
            target: point(10.0, 5.0),
        });
    f.world.catalog.get_mut(f.ground)?.mover_type = MoverType::Air;
    f.world.step_movement_and_steering();
    assert_eq!(f.world.entities[&id].current_funnel_waypoints, []);
    assert!(f.world.entities[&id].position.x > SimScalar::from_num(5));
    assert_eq!(f.world.entities[&id].position.y, SimScalar::from_num(5));
    f.world.catalog.get_mut(f.ground)?.collision_radius = SimScalar::ZERO;
    let position = f.world.entities[&id].position;
    f.world.step_movement_and_steering();
    assert_eq!(f.world.entities[&id].position, position);
    assert_ne!(f.world.diagnostics(), []);
    Ok(())
}

#[test]
fn ground_origin_domain_mutation_is_diagnosed_before_move_or_flee() -> TestResult {
    for terrain in [false, true] {
        let mut f = Fixture::new()?;
        let id = f.spawn(f.air, 0, 1.0, 5.0)?;
        f.world.entities.get_mut(&id).ok_or("actor")?.issue_order(
            UnitOrder::Move {
                target: point(10.0, 5.0),
            },
            false,
        )?;
        f.world.catalog.get_mut(f.air)?.mover_type = MoverType::Ground;
        if terrain {
            f.world.map.cell_mut(1, 5)?.flags = CellFlags::UNPASSABLE_CLIFF;
        } else {
            f.world.catalog.get_mut(f.air)?.collision_radius = SimScalar::from_num(2);
        }
        let before = f.world.entities[&id].position;
        f.world.step_movement_and_steering();
        assert_eq!(f.world.entities[&id].position, before);
        assert_ne!(f.world.diagnostics(), []);
        assert!(f
            .world
            .compute_entity_flee_destination(id, point(0.0, 5.0), SimScalar::from_num(4))
            .is_err());
    }
    Ok(())
}

#[test]
fn specialized_cliff_movers_do_not_acquire_air_traversal() -> TestResult {
    for mover in [MoverType::CliffJumper, MoverType::CliffWalker] {
        let mut f = Fixture::new()?;
        f.world.catalog.get_mut(f.ground)?.mover_type = mover;
        for y in 0..32 {
            f.world.map.cell_mut(12, y)?.flags = CellFlags::UNPASSABLE_CLIFF;
        }
        f.world.pathfinder.recompute_clearance(&f.world.map);
        let id = f.spawn(f.ground, 0, 10.5, 10.5)?;
        f.step(vec![command(vec![id], point(14.5, 10.5), false)]);
        for _ in 0..30 {
            f.step(vec![]);
        }
        assert!(f.world.entities[&id].position.x < SimScalar::from_num(12));
    }
    Ok(())
}

#[test]
fn air_foundation_collision_preserves_registered_footprint_position() -> TestResult {
    let mut f = Fixture::new()?;
    let kind = f
        .world
        .catalog
        .archetypes()
        .find(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .map(|a| a.unit_type_id)
        .ok_or("no foundation")?;
    f.world.catalog.get_mut(kind)?.mover_type = MoverType::Air;
    let building = f.world.spawn_structure(kind, PlayerId(0), 15, 15)?;
    let center = f.world.entities[&building].position;
    f.world
        .spawn_unit(f.air, PlayerId(1), center + point(0.125, 0.0))?;
    let footprint = f
        .world
        .map
        .structures()
        .get(&building)
        .ok_or("foundation")?
        .clone();
    f.world.resolve_collisions_and_push();
    assert_eq!(
        f.world.entities[&building].position, center,
        "registered foundation may not move independently of map occupancy"
    );
    assert_eq!(f.world.map.structures().get(&building), Some(&footprint));
    Ok(())
}

#[test]
fn actual_air_hazard_escape_crosses_ground_only_obstruction() -> TestResult {
    let mut f = Fixture::new()?;
    let id = f.spawn(f.air, 0, 5.5, 8.5)?;
    for y in 0..32 {
        f.world.map.cell_mut(6, y)?.flags = CellFlags::UNPASSABLE_CLIFF;
    }
    f.world.pathfinder.recompute_clearance(&f.world.map);
    f.world.active_hazards.push(sim_core::ActiveHazardZone {
        id: 1,
        owner: PlayerId(1),
        center: point(5.0, 8.5),
        radius: SimScalar::ONE,
        delay_ticks: 0,
        remaining_ticks: 4,
        interval_ticks: 1,
        ticks_until_next_interval: 0,
        damage_per_interval: SimScalar::ONE,
        targets_friendly: false,
    });
    f.step(vec![]);
    assert!(
        matches!(f.world.entities[&id].order_queue.front(),Some(UnitOrder::Move{target}) if target.x>SimScalar::from_num(6))
    );
    for _ in 0..20 {
        f.step(vec![]);
    }
    assert!(f.world.entities[&id].position.x > SimScalar::from_num(6));
    assert_eq!(f.world.diagnostics(), []);
    Ok(())
}

#[test]
fn actual_air_attack_pursuit_crosses_cliff_and_keeps_ground_targetability() -> TestResult {
    let mut f = Fixture::new()?;
    let mut weapon = f
        .world
        .catalog
        .archetypes()
        .find_map(|a| a.ground_weapon.clone().or_else(|| a.weapon.clone()))
        .ok_or("authored weapon")?;
    weapon.max_range = SimScalar::from_num(0.5);
    weapon.min_range = SimScalar::ZERO;
    f.world.catalog.get_mut(f.air)?.ground_weapon = Some(weapon);
    f.world.catalog.get_mut(f.air)?.sight_radius = SimScalar::from_num(20);
    f.world.catalog.get_mut(f.ground)?.max_hp = SimScalar::from_num(100_000);
    for y in 0..32 {
        f.world.map.cell_mut(12, y)?.flags = CellFlags::UNPASSABLE_CLIFF;
    }
    f.world.pathfinder.recompute_clearance(&f.world.map);
    let attacker = f.spawn(f.air, 0, 8.5, 10.5)?;
    let victim = f.spawn(f.ground, 1, 16.5, 10.5)?;
    f.world
        .entities
        .get_mut(&victim)
        .ok_or("victim")?
        .issue_order(UnitOrder::HoldPosition, false)?;
    let before = f.world.entities[&victim].hp + f.world.entities[&victim].shields;
    f.step(vec![PlayerCommand {
        player_id: PlayerId(0),
        entities: vec![attacker],
        order: UnitOrder::attack(victim),
        queue: false,
    }]);
    for _ in 0..100 {
        f.step(vec![]);
    }
    assert!(f.world.entities[&attacker].position.x > SimScalar::from_num(12));
    assert!(f.world.entities[&victim].hp + f.world.entities[&victim].shields < before);
    assert_eq!(f.world.diagnostics(), []);
    Ok(())
}

#[test]
fn exact_air_boundary_snap_and_cross_layer_neighbor_do_not_end_order_early() -> TestResult {
    let mut f = Fixture::new()?;
    let id = f.spawn(f.air, 0, 29.5, 8.5)?;
    f.spawn(f.ground, 0, 30.0, 8.5)?;
    let target = point(31.625, 8.5);
    f.world
        .entities
        .get_mut(&id)
        .ok_or("actor")?
        .current_funnel_waypoints = vec![point(32.0, 8.5)];
    f.step(vec![command(vec![id], target, false)]);
    for _ in 0..20 {
        f.step(vec![]);
    }
    assert_eq!(f.world.entities[&id].position, target);
    let outside = SimVec2::new(SimScalar::from_raw(target.x.raw() + 1), target.y);
    assert_eq!(
        f.world.try_issue_movement(
            PlayerId(0),
            &[id],
            &UnitOrder::Move { target: outside },
            false
        ),
        Err(MovementAdmissionError::OutsideDomain(id))
    );
    Ok(())
}

#[test]
fn registered_foundation_placement_rejects_before_any_actor_or_map_mutation() -> TestResult {
    let mut f = Fixture::new()?;
    let kind = f
        .world
        .catalog
        .archetypes()
        .find(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .map(|a| a.unit_type_id)
        .ok_or("foundation")?;
    f.world.catalog.get_mut(kind)?.mover_type = MoverType::Air;
    f.world.catalog.get_mut(kind)?.abilities = vec![AbilitySpec {
        name: "foundation placement rejection".into(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::ONE,
        cooldown_ticks: 10,
        cast_range: SimScalar::from_num(20),
        effect: AbilityEffectSpec::BlinkTeleport {
            max_range: SimScalar::from_num(20),
            cooldown_ticks: 10,
        },
        leave_name: None,
    }];
    let id = f.world.spawn_structure(kind, PlayerId(0), 10, 10)?;
    f.world.entities.get_mut(&id).ok_or("foundation")?.energy = SimScalar::from_num(100);
    let before = f.world.compute_checksum();
    assert!(!f
        .world
        .execute_ability(id, 0, Some(point(22.0, 22.0)), None));
    assert_eq!(f.world.compute_checksum(), before);
    assert_eq!(
        f.world
            .try_unload_unit_from_transport(id, point(22.0, 22.0)),
        Err(TransportAdmissionError::Destination(
            DisplacementAdmissionError::RegisteredFoundation(id)
        ))
    );
    assert_eq!(f.world.compute_checksum(), before);
    Ok(())
}

#[test]
fn mixed_layer_replay_remains_deterministic_for_two_thousand_ticks() -> TestResult {
    let mut a = Fixture::new()?;
    let mut b = Fixture::new()?;
    for f in [&mut a, &mut b] {
        f.spawn(f.air, 0, 6.0, 6.0)?;
        f.spawn(f.air, 0, 6.5, 6.0)?;
        f.spawn(f.ground, 0, 6.0, 6.0)?;
    }
    for tick in 0..2000 {
        let commands = if tick % 200 == 0 {
            vec![command(
                a.world.entities.keys().copied().collect(),
                point(if tick % 400 == 0 { 24.0 } else { 6.0 }, 16.0),
                tick % 400 == 0,
            )]
        } else {
            vec![]
        };
        a.step(commands.clone());
        b.step(commands);
        assert_eq!(
            a.world.last_checksum(),
            b.world.last_checksum(),
            "tick {tick}"
        );
        assert_eq!(a.world.diagnostics(), []);
        assert_eq!(b.world.diagnostics(), []);
        let before = a.world.compute_checksum();
        a.world.drain_movement_outcomes().for_each(drop);
        assert_eq!(a.world.compute_checksum(), before);
    }
    Ok(())
}

#[test]
fn trained_air_actor_keeps_supply_capture_after_obstacle_crossing_and_death() -> TestResult {
    let mut f = Fixture::new()?;
    let producer_type = f
        .world
        .catalog
        .archetypes()
        .find(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }) && !a.trains_units.is_empty())
        .map(|a| a.unit_type_id)
        .ok_or("producer")?;
    let product_slug = f.world.catalog.get(f.air)?.slug.clone();
    let source = f.world.catalog.get_mut(producer_type)?;
    source.trains_units = vec![product_slug];
    source.supply_provided = 20;
    let product = f.world.catalog.get_mut(f.air)?;
    product.requires_structures.clear();
    product.build_time_ticks = 1;
    product.supply_cost = 1;
    for y in 0..32 {
        f.world.map.cell_mut(8, y)?.flags = CellFlags::UNPASSABLE_CLIFF;
    }
    let producer = f
        .world
        .spawn_structure(producer_type, PlayerId(0), 15, 15)?;
    f.world
        .try_queue_production(PlayerId(0), &[producer], f.air)?;
    f.world.update_production_and_buffs();
    let child = f
        .world
        .entities
        .values()
        .find(|e| e.unit_type_id == f.air)
        .map(|e| e.id)
        .ok_or("completed actor")?;
    let captured = f.world.entities[&child].captured_supply_used;
    assert_eq!(captured, 1);
    f.step(vec![command(vec![child], point(3.5, 15.5), false)]);
    for _ in 0..100 {
        f.step(vec![]);
    }
    assert_eq!(f.world.entities[&child].position, point(3.5, 15.5));
    f.world.validate_supply_ledger()?;
    let before = f.world.player_resources[PlayerSlot::ALL[0]].supply_used;
    f.world.catalog.get_mut(f.air)?.supply_cost = 99;
    f.world.entities.get_mut(&child).ok_or("child")?.hp = SimScalar::ZERO;
    f.world.reap_dead_and_emit_shatter();
    assert_eq!(
        f.world.player_resources[PlayerSlot::ALL[0]].supply_used,
        before - captured
    );
    f.world.validate_supply_ledger()?;
    Ok(())
}
