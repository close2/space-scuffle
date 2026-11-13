//! Checked movement through real catalog admission, commands and public phase APIs.
//! Generic capability-selected fixtures exercise exact overflow/atomicity boundaries;
//! no asset identity, visual stand-in or altered simulation loop establishes acceptance.
use sim_core::catalog::RuntimeCatalogs;
use sim_core::entity::{ActiveBuff, EntityAdmissionError};
use sim_core::movement::{MovementInvariant, MovementMathError};
use sim_core::{SimCoreError, SimWorld, UnitCatalog, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, EntityId, MoverType, OverclockSpec,
    PlayerCommand, PlayerId, PlayerSlot, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitStatDeltas, UnitTypeId, UpgradeEffect,
};
type TestResult = Result<(), Box<dyn std::error::Error>>;
struct Fixture {
    world: SimWorld,
    air: UnitTypeId,
    ground: UnitTypeId,
}
fn point(x: i32, y: i32) -> SimVec2 {
    SimVec2::from_i32(x, y)
}
impl Fixture {
    fn new(width: u32, height: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let loaded = RuntimeCatalogs::load_from_directory(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog"),
        )?;
        let mut catalog = loaded.units;
        let select = |layer| {
            catalog
                .archetypes()
                .find(|a| {
                    a.mover_type == layer
                        && !a.is_immobile_by_default()
                        && !matches!(a.shape, ShapeKind::BuildingDome { .. })
                })
                .map(|a| a.unit_type_id)
                .ok_or("missing mobile capability")
        };
        let air = select(MoverType::Air)?;
        let ground = select(MoverType::Ground)?;
        for id in [air, ground] {
            let a = catalog.get_mut(id)?;
            a.max_speed = SimScalar::from_num(0.125);
            // Instant, so a first tick covers the whole speed (acceleration is pinned in
            // `acceleration_tests.rs`).
            a.acceleration = a.max_speed;
            a.deceleration = a.max_speed;
            a.collision_radius = SimScalar::from_num(0.375);
            a.weapon = None;
            a.ground_weapon = None;
            a.air_weapon = None;
        }
        let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = SimWorld::with_catalogs(map, 50, catalog, loaded.upgrades);
        world.vision_aware_pathfinding = false;
        Ok(Self { world, air, ground })
    }
    fn spawn(
        &mut self,
        kind: UnitTypeId,
        pos: SimVec2,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        Ok(self.world.spawn_unit(kind, PlayerId(0), pos)?)
    }
    fn ability(
        &mut self,
        spec: OverclockSpec,
        energy: SimScalar,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.world.catalog.get_mut(self.air)?.abilities = vec![AbilitySpec {
            name: "numeric admission fixture".into(),
            target_kind: AbilityTargetKind::InstantSelf,
            energy_cost: energy,
            cast_range: SimScalar::ZERO,
            cooldown_ticks: 20,
            effect: AbilityEffectSpec::Overclock(spec),
            leave_name: None,
            cast_backswing_ticks: 0,
            single_caster: true,
            target_filter: None,
            origin: None,
        }];
        Ok(())
    }
}
fn buff(id: u8, speed: SimScalar) -> ActiveBuff {
    ActiveBuff {
        buff_id: id,
        remaining_ticks: 100,
        speed_multiplier: speed,
        attack_rate_multiplier: SimScalar::ONE,
    }
}
fn overclock(speed: SimScalar) -> OverclockSpec {
    OverclockSpec {
        hp_cost: SimScalar::ONE,
        duration_ticks: 50,
        speed_multiplier: speed,
        attack_rate_multiplier: SimScalar::ONE,
    }
}

#[test]
fn real_long_air_command_advances_without_squared_intermediate_overflow() -> TestResult {
    let mut f = Fixture::new(60_000, 4)?;
    let id = f.spawn(f.air, point(2, 2))?;
    let input = TickBundle {
        tick: Tick(0),
        commands: vec![PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![id],
            order: UnitOrder::Move {
                target: point(50_002, 2),
            },
            queue: false,
        }],
        state_checksum: None,
    };
    f.world.step(&input);
    assert!(f.world.movement_outcomes()[0].result.is_ok());
    assert_eq!(
        f.world.entities[&id].position,
        SimVec2::new(SimScalar::from_num(2.125), SimScalar::from_num(2))
    );
    assert!(
        f.world.diagnostics().is_empty(),
        "{:?}",
        f.world.diagnostics()
    );
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn corrupted_registered_air_foundation_never_authors_motion_or_flee() -> TestResult {
    for kind in 0..3 {
        let mut f = Fixture::new(64, 64)?;
        let dome = f
            .world
            .catalog
            .archetypes()
            .find(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
            .map(|a| a.unit_type_id)
            .ok_or("foundation capability")?;
        let a = f.world.catalog.get_mut(dome)?;
        a.mover_type = MoverType::Air;
        a.max_speed = SimScalar::from_num(0.25);
        let id = f.world.spawn_structure(dome, PlayerId(0), 10, 10)?;
        let target = f.spawn(f.ground, point(30, 30))?;
        let actor = f.world.entities.get_mut(&id).ok_or("foundation")?;
        actor.is_immobile = false;
        actor.unpushable = false;
        let order = match kind {
            0 => UnitOrder::Move {
                target: point(25, 25),
            },
            1 => UnitOrder::AttackMove {
                target: point(25, 25),
            },
            _ => UnitOrder::attack(target),
        };
        assert!(actor.issue_order(order, false)?);
        let before = actor.clone();
        let placement = f.world.map.structures().get(&id).cloned();
        assert!(matches!(
            f.world
                .compute_entity_flee_destination(id, point(0, 0), SimScalar::from_num(4)),
            Err(MovementInvariant::InvalidState { .. })
        ));
        f.world.step_movement_and_steering();
        assert_eq!(f.world.entities[&id], before);
        assert_eq!(f.world.map.structures().get(&id).cloned(), placement);
        assert_ne!(f.world.diagnostics(), []);
    }
    Ok(())
}

#[test]
fn invalid_ingress_preserves_every_movement_field_and_other_valid_actors_progress() -> TestResult {
    for case in 0..5 {
        let mut f = Fixture::new(64, 64)?;
        let id = f.spawn(f.air, point(8, 8))?;
        let healthy = f.spawn(f.ground, point(8, 20))?;
        for actor in [id, healthy] {
            f.world.try_issue_movement(
                PlayerId(0),
                &[actor],
                &UnitOrder::Move {
                    target: point(40, 25),
                },
                false,
            )?;
        }
        let actor = f.world.entities.get_mut(&id).ok_or("actor")?;
        actor.current_funnel_waypoints = vec![point(12, 12)];
        actor.pos_at_tick_start = point(3, 3);
        match case {
            0 => f.world.catalog.get_mut(f.air)?.max_speed = -SimScalar::ONE,
            1 => actor.active_buffs.push(buff(9, -SimScalar::ONE)),
            2 => {
                actor.active_buffs = vec![buff(8, SimScalar::MAX), buff(9, SimScalar::from_num(2))];
            }
            3 => actor.owner = PlayerId(u8::MAX),
            _ => {
                let slug = f.world.catalog.get(f.air)?.slug.clone();
                f.world.player_upgrades[PlayerSlot::ALL[0]]
                    .unit_stat_modifiers
                    .insert(
                        slug,
                        UnitStatDeltas {
                            move_speed_delta: -SimScalar::ONE,
                            ..Default::default()
                        },
                    );
            }
        }
        let before = f.world.entities[&id].clone();
        let healthy_before = f.world.entities[&healthy].position;
        f.world.step_movement_and_steering();
        assert_eq!(f.world.entities[&id], before);
        assert_ne!(f.world.entities[&healthy].position, healthy_before);
        assert_ne!(f.world.diagnostics(), []);
    }
    Ok(())
}

#[test]
fn late_pursuit_range_overflow_does_not_clear_route_or_pose_bookkeeping() -> TestResult {
    let mut f = Fixture::new(64, 64)?;
    let weapon = f
        .world
        .catalog
        .archetypes()
        .find_map(|a| a.weapon.clone().or_else(|| a.ground_weapon.clone()))
        .ok_or("weapon capability")?;
    f.world.catalog.get_mut(f.air)?.weapon = Some(weapon);
    let id = f.spawn(f.air, point(10, 10))?;
    let target = f.spawn(f.ground, point(30, 30))?;
    let actor = f.world.entities.get_mut(&id).ok_or("actor")?;
    assert!(actor.issue_order(UnitOrder::attack(target), false)?);
    actor.current_funnel_waypoints = vec![point(15, 15)];
    actor.pos_at_tick_start = point(4, 4);
    let slug = f.world.catalog.get(f.air)?.slug.clone();
    f.world.player_upgrades[PlayerSlot::ALL[0]]
        .unit_stat_modifiers
        .insert(
            slug,
            UnitStatDeltas {
                weapon_range_delta: SimScalar::MAX,
                ..Default::default()
            },
        );
    let before = f.world.entities[&id].clone();
    f.world.step_movement_and_steering();
    assert_eq!(f.world.entities[&id], before);
    assert_ne!(f.world.diagnostics(), []);
    Ok(())
}

#[test]
fn ordinary_queued_cast_rejection_is_consumed_but_malformed_request_is_preserved() -> TestResult {
    for case in 0..7 {
        let mut f = Fixture::new(64, 64)?;
        let mut spec = overclock(SimScalar::from_num(1.5));
        let mut energy_cost = SimScalar::from_num(2);
        if case == 3 {
            spec.speed_multiplier = -SimScalar::ONE;
        }
        if case == 4 {
            spec.attack_rate_multiplier = SimScalar::ZERO;
        }
        if case == 5 {
            energy_cost = -SimScalar::ONE;
        }
        if case == 6 {
            f.world.catalog.get_mut(f.air)?.max_speed = SimScalar::MAX;
            spec.speed_multiplier = SimScalar::from_num(2);
        }
        f.ability(spec, energy_cost)?;
        let id = f.spawn(f.air, point(10, 10))?;
        let actor = f.world.entities.get_mut(&id).ok_or("actor")?;
        actor.energy = SimScalar::from_num(10);
        match case {
            0 => actor.hp = SimScalar::ONE,
            1 => actor.energy = SimScalar::ZERO,
            2 => actor.ability_cooldowns[0] = 1,
            _ => {}
        }
        assert!(actor.issue_order(
            UnitOrder::CastAbility {
                ability_index: 0,
                target_pos: None,
                target_entity: None
            },
            false
        )?);
        actor.pos_at_tick_start = point(4, 4);
        actor.current_funnel_waypoints = vec![point(12, 12)];
        let before = actor.clone();
        f.world.step_movement_and_steering();
        let actor = &f.world.entities[&id];
        if case < 3 {
            assert!(actor.order_queue.is_empty());
            assert_eq!(actor.hp, before.hp);
            assert_eq!(actor.energy, before.energy);
            assert_eq!(actor.ability_cooldowns, before.ability_cooldowns);
            assert_eq!(f.world.diagnostics(), []);
        } else {
            assert_eq!(*actor, before);
            assert_ne!(f.world.diagnostics(), []);
        }
    }
    Ok(())
}

#[test]
fn ordered_speed_fold_and_direct_buff_admission_have_explicit_boundaries() -> TestResult {
    let mut f = Fixture::new(64, 64)?;
    let id = f.spawn(f.air, point(10, 10))?;
    let actor = f.world.entities.get_mut(&id).ok_or("actor")?;
    actor.active_buffs = vec![
        buff(8, SimScalar::MAX),
        buff(9, SimScalar::from_num(2)),
        buff(10, SimScalar::ZERO),
    ];
    assert_eq!(
        actor.speed_multiplier(),
        Err(MovementMathError::Unrepresentable)
    );
    let before = actor.clone();
    assert!(actor.apply_overclock(overclock(SimScalar::ONE)).is_err());
    assert_eq!(*actor, before);
    actor.active_buffs = vec![buff(8, SimScalar::from_raw(1)), buff(9, SimScalar::HALF)];
    assert_eq!(actor.speed_multiplier(), Ok(SimScalar::ZERO));
    actor.active_buffs = vec![buff(8, SimScalar::HALF), buff(9, SimScalar::from_num(2))];
    assert_eq!(actor.speed_multiplier(), Ok(SimScalar::ONE));
    actor.active_buffs.clear();
    let before = actor.clone();
    assert_eq!(
        actor.apply_overclock(overclock(-SimScalar::ONE)),
        Err(EntityAdmissionError::Movement(
            MovementMathError::NegativeMultiplier { index: 0 }
        ))
    );
    assert_eq!(*actor, before);
    let mut invalid = overclock(SimScalar::ONE);
    invalid.hp_cost = -SimScalar::ONE;
    assert_eq!(
        actor.apply_overclock(invalid),
        Err(EntityAdmissionError::NegativeCost)
    );
    assert_eq!(*actor, before);
    Ok(())
}

#[test]
fn rejected_speed_research_preserves_resources_research_and_live_vitals() -> TestResult {
    let mut f = Fixture::new(64, 64)?;
    f.spawn(f.air, point(10, 10))?;
    let mut upgrade = f
        .world
        .upgrade_catalog
        .upgrades()
        .next()
        .ok_or("upgrade")?
        .clone();
    let slug = upgrade.slug.clone();
    upgrade.effects = vec![
        UpgradeEffect::ShieldLevel { level: 3 },
        UpgradeEffect::ModifyUnitStat {
            target_slug: f.world.catalog.get(f.air)?.slug.clone(),
            max_hp_delta: SimScalar::ONE,
            move_speed_delta: -SimScalar::ONE,
            sight_delta: SimScalar::ZERO,
            weapon_range_delta: SimScalar::ZERO,
            armor_delta: SimScalar::ZERO,
            attack_cooldown_delta: 0,
            bonus_damage_delta: SimScalar::ZERO,
        },
    ];
    f.world.upgrade_catalog = UpgradeCatalog::from_definitions([upgrade])?;
    let entities = f.world.entities.clone();
    let players = f.world.player_upgrades.clone();
    let resources = f.world.player_resources;
    let checksum = f.world.compute_checksum();
    assert_eq!(
        f.world.research_upgrade(PlayerId(0), &slug),
        Err(SimCoreError::MovementMath(MovementMathError::NegativeSpeed))
    );
    assert_eq!(f.world.entities, entities);
    assert_eq!(f.world.player_upgrades, players);
    assert_eq!(f.world.player_resources, resources);
    assert_eq!(f.world.compute_checksum(), checksum);
    Ok(())
}

#[test]
fn authored_negative_speed_is_rejected_while_zero_is_valid() -> TestResult {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog/units.ron"),
    )?;
    let start = source.find("max_speed:").ok_or("authored speed field")? + "max_speed:".len();
    let end = start + source[start..].find(',').ok_or("speed field separator")?;
    let mut negative = source.clone();
    negative.replace_range(start..end, " -0.125");
    assert!(
        matches!(UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &negative),Err(SimCoreError::CatalogInvalidValue{field,..}) if field=="max_speed")
    );
    let mut zero = source;
    zero.replace_range(start..end, " 0.0");
    assert!(UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &zero).is_ok());
    Ok(())
}

#[test]
fn public_steering_handles_extreme_directions_and_wide_obstacle_thresholds() -> TestResult {
    use sim_core::steering::{
        check_cluster_arrival_except, compute_concave_arc_steering, compute_unified_local_steering,
        SteeringObstacle,
    };
    let min = SimScalar::from_raw(i64::MIN);
    let max = SimScalar::MAX;
    let lower = SimVec2::new(min, min);
    let upper = SimVec2::new(max, max);
    let direct =
        compute_unified_local_steering(EntityId(1), lower, SimScalar::ONE, upper, None, &[])?;
    assert!(direct.x > SimScalar::ZERO && direct.x < SimScalar::ONE);
    assert_eq!(direct.x, direct.y);
    let left = compute_concave_arc_steering(lower, upper, true, true)?;
    assert_eq!(left, SimVec2::new(-direct.y, direct.x));
    assert_eq!(
        compute_concave_arc_steering(lower, upper, true, false)?,
        -left
    );
    assert_eq!(
        compute_concave_arc_steering(lower, upper, false, true)?,
        SimVec2::ZERO
    );
    let start = SimVec2::new(min, SimScalar::ZERO);
    let end = SimVec2::new(max, SimScalar::ZERO);
    let obstacle = SteeringObstacle {
        settled_on: Some(end),
        ..SteeringObstacle::new(EntityId(2), end, max, SimVec2::ZERO)
    };
    // Radius sum plus the existing lookahead exceeds one scalar, and the raw
    // obstacle difference exceeds i64. Both comparison and direction are valid.
    let detour = compute_unified_local_steering(EntityId(1), start, max, end, None, &[obstacle])?;
    assert!(detour.x > SimScalar::ZERO);
    assert!(check_cluster_arrival_except(
        Some(EntityId(1)),
        start,
        max,
        end,
        &[obstacle]
    )?);
    let invalid = SteeringObstacle {
        radius: SimScalar::ZERO,
        ..obstacle
    };
    assert_eq!(
        compute_unified_local_steering(EntityId(1), start, max, end, None, &[invalid]),
        Err(MovementMathError::InvalidDistance)
    );
    assert_eq!(
        check_cluster_arrival_except(Some(EntityId(1)), start, max, end, &[obstacle, invalid]),
        Err(MovementMathError::InvalidDistance)
    );
    Ok(())
}

#[test]
fn widened_blink_and_flee_queries_preserve_exact_admission_and_payments() -> TestResult {
    let mut f = Fixture::new(60_000, 4)?;
    f.world.catalog.get_mut(f.air)?.abilities = vec![AbilitySpec {
        name: "wide teleport fixture".into(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::from_num(2),
        cast_range: SimScalar::from_num(50_000),
        cooldown_ticks: 20,
        effect: AbilityEffectSpec::BlinkTeleport {
            max_range: SimScalar::from_num(50_000),
            cooldown_ticks: 20,
        },
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    }];
    let id = f.spawn(f.air, point(2, 2))?;
    f.world.entities.get_mut(&id).ok_or("actor")?.energy = SimScalar::from_num(10);
    let before = f.world.entities[&id].clone();
    assert!(!f.world.execute_ability(id, 0, Some(point(50_003, 2)), None));
    assert_eq!(f.world.entities[&id], before);
    assert!(f.world.execute_ability(id, 0, Some(point(50_002, 2)), None));
    assert_eq!(f.world.entities[&id].position, point(50_002, 2));
    assert_eq!(f.world.entities[&id].energy, SimScalar::from_num(8));
    assert_eq!(f.world.entities[&id].ability_cooldowns[0], 20);
    let before = f.world.entities[&id].clone();
    let threat = SimVec2::new(SimScalar::from_raw(i64::MIN), SimScalar::from_raw(i64::MIN));
    let goal = f
        .world
        .compute_entity_flee_destination(id, threat, SimScalar::MAX)?
        .ok_or("valid autonomous boundary goal")?;
    let radius = f.world.catalog.get(f.air)?.collision_radius;
    assert_eq!(
        goal,
        SimVec2::new(
            SimScalar::from_num(60_000) - radius,
            SimScalar::from_num(4) - radius
        )
    );
    assert_eq!(f.world.entities[&id], before);
    assert_eq!(f.world.diagnostics(), []);
    Ok(())
}

#[test]
fn late_collision_pose_failure_preserves_actor_and_keeps_valid_neighbor_work() -> TestResult {
    let mut f = Fixture::new(32, 32)?;
    let a = f.spawn(f.air, point(8, 8))?;
    let b = f.spawn(
        f.air,
        SimVec2::new(SimScalar::from_num(8.5), SimScalar::from_num(8)),
    )?;
    f.world
        .entities
        .get_mut(&a)
        .ok_or("actor")?
        .pos_at_tick_start = SimVec2::new(SimScalar::from_raw(i64::MIN), SimScalar::ZERO);
    let before = f.world.entities[&a].clone();
    let neighbor = f.world.entities[&b].position;
    f.world.resolve_collisions_and_push();
    assert_eq!(f.world.entities[&a], before);
    assert_ne!(f.world.entities[&b].position, neighbor);
    assert!(f.world.diagnostics().iter().any(|d|matches!(d,sim_core::SimulationDiagnostic::MovementInvariant{source:MovementInvariant::Arithmetic{entity,operation:"collision application",source:MovementMathError::Unrepresentable},..}if *entity==a)));
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn typed_overclock_and_refresh_preserve_order_and_existing_buff_position() -> TestResult {
    let mut f = Fixture::new(32, 32)?;
    let id = f.spawn(f.air, point(8, 8))?;
    let actor = f.world.entities.get_mut(&id).ok_or("actor")?;
    assert!(actor.issue_order(
        UnitOrder::Move {
            target: point(20, 20)
        },
        false
    )?);
    actor.active_buffs = vec![
        buff(7, SimScalar::ONE),
        buff(1, SimScalar::from_num(1.5)),
        buff(9, SimScalar::ONE),
    ];
    let order = actor.order_queue.clone();
    assert!(actor.apply_overclock(overclock(SimScalar::from_num(2)))?);
    assert_eq!(
        actor
            .active_buffs
            .iter()
            .map(|b| b.buff_id)
            .collect::<Vec<_>>(),
        vec![7, 1, 9]
    );
    assert_eq!(actor.active_buffs[1].remaining_ticks, 50);
    assert_eq!(actor.speed_multiplier()?, SimScalar::from_num(2));
    assert_eq!(actor.order_queue, order);
    actor.active_buffs[0].speed_multiplier = -SimScalar::ONE;
    let before = actor.clone();
    assert_eq!(
        actor.apply_overclock(overclock(SimScalar::from_num(1.5))),
        Err(EntityAdmissionError::Movement(
            MovementMathError::NegativeMultiplier { index: 0 }
        ))
    );
    assert_eq!(*actor, before);
    Ok(())
}

#[test]
fn an_immediate_stim_issued_to_an_entity_outside_the_world_is_rejected() -> TestResult {
    // B-32 (plan `2026-11-05-sc2-deviation-fixes` WP3): only the world knows the catalog's
    // Overclock spec and whether its research is done, so an immediate `UseStim` given to an
    // entity directly is refused with a typed error and changes nothing. The world's command
    // path (`PlayerCommand`) is the one way to stim.
    let mut f = Fixture::new(32, 32)?;
    let id = f.spawn(f.air, point(8, 8))?;
    let actor = f.world.entities.get_mut(&id).ok_or("actor")?;
    let before = actor.clone();
    let result = actor.issue_order(UnitOrder::UseStim, false);
    assert_eq!(result, Err(EntityAdmissionError::StimOutsideWorld));
    assert_eq!(*actor, before);
    Ok(())
}

#[test]
fn authored_negative_capability_and_ability_values_are_rejected_at_load() -> TestResult {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog/units.ron"),
    )?;
    for field in ["speed_multiplier", "hp_cost", "energy_cost", "cast_range"] {
        let label = format!("{field}:");
        let start = source.find(&label).ok_or("authored field")? + label.len();
        let end = start + source[start..].find(',').ok_or("field separator")?;
        let mut invalid = source.clone();
        invalid.replace_range(start..end, " -1.0");
        assert!(
            matches!(UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &invalid),Err(SimCoreError::CatalogInvalidValue{field:actual,..})if actual.contains(field)),
            "field {field}"
        );
    }
    Ok(())
}

#[test]
fn queued_blink_commits_pre_cast_origin_with_exact_destination_and_payment() -> TestResult {
    let mut f = Fixture::new(32, 32)?;
    f.world.catalog.get_mut(f.air)?.abilities = vec![AbilitySpec {
        name: "queued origin fixture".into(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::from_num(2),
        cast_range: SimScalar::from_num(12),
        cooldown_ticks: 20,
        effect: AbilityEffectSpec::BlinkTeleport {
            max_range: SimScalar::from_num(12),
            cooldown_ticks: 20,
        },
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    }];
    let origin = point(8, 8);
    let destination = point(18, 8);
    let id = f.spawn(f.air, origin)?;
    let actor = f.world.entities.get_mut(&id).ok_or("actor")?;
    actor.energy = SimScalar::from_num(10);
    actor.pos_at_tick_start = point(1, 1);
    assert!(actor.issue_order(
        UnitOrder::CastAbility {
            ability_index: 0,
            target_pos: Some(destination),
            target_entity: None
        },
        false
    )?);
    f.world.step_movement_and_steering();
    let actor = &f.world.entities[&id];
    assert_eq!(actor.position, destination);
    assert_eq!(actor.pos_at_tick_start, origin);
    assert_eq!(actor.energy, SimScalar::from_num(8));
    assert_eq!(actor.ability_cooldowns[0], 20);
    assert_eq!(actor.order_queue.len(), 0);
    assert_eq!(f.world.diagnostics(), []);
    Ok(())
}

#[test]
fn direct_ability_missing_archetype_is_diagnosed_without_mutation() -> TestResult {
    let mut f = Fixture::new(32, 32)?;
    let id = f.spawn(f.air, point(8, 8))?;
    let missing = (0..=u16::MAX)
        .map(UnitTypeId)
        .find(|kind| f.world.catalog.get(*kind).is_err())
        .ok_or("unused archetype slot")?;
    f.world.entities.get_mut(&id).ok_or("actor")?.unit_type_id = missing;
    let before = f.world.entities[&id].clone();
    assert!(!f.world.execute_ability(id, 0, None, None));
    assert_eq!(f.world.entities[&id], before);
    assert!(
        matches!(f.world.diagnostics(),[sim_core::SimulationDiagnostic::MovementInvariant{source:MovementInvariant::MissingArchetype(actual),..}] if *actual==id)
    );
    Ok(())
}

#[test]
fn lookahead_and_arrival_thresholds_are_exact_at_their_boundaries() -> TestResult {
    use sim_core::steering::{
        check_cluster_arrival_except, compute_unified_local_steering, SteeringObstacle,
    };
    // The named constants are exact rationals: 0.35 and 0.15 world units.
    let lookahead = SimScalar::from_raw((35_i64 << 32) / 100);
    let touch_margin = SimScalar::from_raw((15_i64 << 32) / 100);
    let inside = SteeringObstacle::new(
        EntityId(2),
        SimVec2::new(
            SimScalar::ONE + lookahead / SimScalar::from_num(2),
            SimScalar::ZERO,
        ),
        SimScalar::HALF,
        SimVec2::ZERO,
    );
    let deflected = compute_unified_local_steering(
        EntityId(1),
        SimVec2::ZERO,
        SimScalar::HALF,
        point(8, 0),
        None,
        &[inside],
    )?;
    assert!(
        deflected.y < SimScalar::ZERO,
        "dead ahead passes on the right"
    );
    let at_boundary = SteeringObstacle {
        pos: SimVec2::new(SimScalar::ONE + lookahead, SimScalar::ZERO),
        ..inside
    };
    assert_eq!(
        compute_unified_local_steering(
            EntityId(1),
            SimVec2::ZERO,
            SimScalar::HALF,
            point(8, 0),
            None,
            &[at_boundary]
        )?,
        SimVec2::X_AXIS,
        "influence fades to nothing exactly at the lookahead"
    );
    let settled_point = SimVec2::new(SimScalar::ONE + touch_margin, SimScalar::ZERO);
    let settled = SteeringObstacle {
        pos: settled_point,
        settled_on: Some(settled_point),
        ..inside
    };
    assert!(check_cluster_arrival_except(
        Some(EntityId(1)),
        SimVec2::ZERO,
        SimScalar::HALF,
        settled.pos,
        &[settled]
    )?);
    let outside_point = settled.pos + SimVec2::new(SimScalar::EPSILON, SimScalar::ZERO);
    let outside = SteeringObstacle {
        pos: outside_point,
        settled_on: Some(outside_point),
        ..settled
    };
    assert!(!check_cluster_arrival_except(
        Some(EntityId(1)),
        SimVec2::ZERO,
        SimScalar::HALF,
        outside.pos,
        &[outside]
    )?);
    Ok(())
}

#[test]
fn unavailable_pursuit_preserves_legacy_animation_until_next_normal_phase() -> TestResult {
    let mut f = Fixture::new(32, 32)?;
    let id = f.spawn(f.air, point(8, 8))?;
    let actor = f.world.entities.get_mut(&id).ok_or("actor")?;
    assert!(actor.issue_order(UnitOrder::attack(EntityId(u32::MAX)), false)?);
    actor.current_anim = sim_types::AnimState::Walk;
    actor.velocity = SimVec2::X_AXIS;
    actor.current_funnel_waypoints = vec![point(10, 10)];
    f.world.step_movement_and_steering();
    let actor = &f.world.entities[&id];
    assert_eq!(actor.position, point(8, 8));
    assert_eq!(actor.velocity, SimVec2::ZERO);
    assert_eq!(actor.current_anim, sim_types::AnimState::Walk);
    assert_eq!(actor.order_queue.len(), 0);
    assert_eq!(actor.current_funnel_waypoints.len(), 0);
    f.world.step_movement_and_steering();
    assert_eq!(
        f.world.entities[&id].current_anim,
        sim_types::AnimState::Idle
    );
    assert_eq!(f.world.diagnostics(), []);
    Ok(())
}
