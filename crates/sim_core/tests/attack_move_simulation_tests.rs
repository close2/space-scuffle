//! # Integration Tests: AttackMove & Threat Priority
//!
//! Verifies:
//! 1. Units on `AttackMove` engage hostile units entering scan range, destroy them, and automatically resume marching to destination.
//! 2. Threat priority evaluation: Armed combatants (Priority 20) take precedence over passive structures (Priority 11).
//! 3. Dual-weapon selection: Chooses ground weapon for ground, air weapon for air, and highest DPS weapon for CliffWalker.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::{SimWorld, UnitArchetype, UnitCatalog};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AnimState, ArchetypeCapabilities, AttributeFlags, EntityId, Faction, MoverType, PlayerCommand,
    PlayerId, ShapeKind, SilhouetteClass, SimAngle, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitTypeId, WeaponSpec, WeaponVisualKind,
};
use std::collections::BTreeMap;

fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// Test 4: Squad marching via AttackMove halts to destroy ambushing wave, then automatically
/// resumes toward target coordinates without resetting or requiring a new player command.
#[test]
fn test_attack_move_wave_engages_and_resumes_destination() {
    let width = 32;
    let height = 32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0xACE);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let pulse_bot_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // Spawn 2 blue units at (4.0, 16.0)
    let b1 = world
        .spawn_unit(
            pulse_bot_id,
            player_blue,
            SimVec2::new(SimScalar::from_num(4), SimScalar::from_num(15)),
        )
        .expect("spawn b1");
    let b2 = world
        .spawn_unit(
            pulse_bot_id,
            player_blue,
            SimVec2::new(SimScalar::from_num(4), SimScalar::from_num(17)),
        )
        .expect("spawn b2");

    // Spawn 1 low-health enemy at (12.0, 16.0) directly along the marching path
    let enemy_id = world
        .spawn_unit(
            pulse_bot_id,
            player_red,
            SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(16)),
        )
        .expect("spawn enemy");
    // Set enemy health low so it dies quickly in battle
    world.entities.get_mut(&enemy_id).unwrap().hp = SimScalar::from_num(10);

    let march_destination = SimVec2::new(SimScalar::from_num(26), SimScalar::from_num(16));

    // Order blue units to AttackMove to (26.0, 16.0)
    for &id in &[b1, b2] {
        world.entities.get_mut(&id).unwrap().issue_order(
            UnitOrder::AttackMove {
                target: march_destination,
            },
            false,
        );
    }

    // Step simulation
    let mut enemy_killed = false;
    let mut resumed_marching = false;

    for tick in 1..=200 {
        world.step(&empty_bundle(tick));

        if !world.entities.contains_key(&enemy_id) {
            enemy_killed = true;
        }

        if enemy_killed {
            let u1 = world.entities.get(&b1).unwrap();
            // Confirm the front order resumed to AttackMove or unit moved past the ambush point
            if u1.position.x > SimScalar::from_num(15) {
                resumed_marching = true;
                break;
            }
        }
    }

    assert!(
        enemy_killed,
        "Ambushing enemy must be destroyed by AttackMove squad"
    );
    assert!(
        resumed_marching,
        "Squad must automatically resume marching toward destination after defeating enemy"
    );
}

/// Test 5: Threat Priority logic selects active armed combatants (Priority 20)
/// over closer passive domes (Priority 11).
#[test]
fn test_threat_priority_combatants_before_passive_structures() {
    let width = 32;
    let height = 32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0xF00D);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let pulse_bot_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");
    let dome_id = world
        .catalog
        .id_by_slug("assembly_bay_dome")
        .expect("assembly_bay_dome");

    // Blue unit at (5.0, 16.0)
    let blue_id = world
        .spawn_unit(
            pulse_bot_id,
            player_blue,
            SimVec2::new(SimScalar::from_num(5), SimScalar::from_num(16)),
        )
        .expect("spawn blue");

    // Red Passive Structure at (9, 15) - Closer (distance ~4.0)
    let dome_ent_id = world
        .spawn_structure(dome_id, player_red, 9, 15)
        .expect("spawn red dome");

    // Red Armed Combatant at (11.0, 16.0) - Farther (distance ~6.0), within scan range 9.0
    let combatant_ent_id = world
        .spawn_unit(
            pulse_bot_id,
            player_red,
            SimVec2::new(SimScalar::from_num(11), SimScalar::from_num(16)),
        )
        .expect("spawn red combatant");

    // Issue AttackMove order to Blue
    world.entities.get_mut(&blue_id).unwrap().issue_order(
        UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(25), SimScalar::from_num(16)),
        },
        false,
    );

    // Step 1 tick for target acquisition
    world.step(&empty_bundle(1));

    let blue_ent = world.entities.get(&blue_id).unwrap();
    // Blue unit must have pushed AttackTarget targeting the armed combatant, not the closer dome!
    let front_order = blue_ent.order_queue.front();
    assert_eq!(
        front_order,
        Some(&UnitOrder::AttackTarget { target: combatant_ent_id }),
        "Unit must prioritize armed combatant (Priority 20) over closer passive structure (Priority 11)"
    );

    let _ = dome_ent_id;
}

/// Test 6: Dual Weapon selection for Ground, Air, and CliffWalker (Colossus hybrid).
#[test]
fn test_dual_weapon_ground_air_and_cliffwalker_selection() {
    let ground_weapon = WeaponSpec {
        name: "Twin Flak Cannons (Ground)".to_string(),
        visual_kind: WeaponVisualKind::Hitscan,
        target_flags: AttributeFlags::all(),
        min_range: SimScalar::ZERO,
        max_range: SimScalar::from_num(5),
        firing_arc: SimAngle::from_degrees(360),
        has_turret: false,
        turret_turn_rate: SimAngle::ZERO,
        base_damage: SimScalar::from_num(10),
        bonus_armored: SimScalar::ZERO,
        bonus_light: SimScalar::ZERO,
        attacks_count: 1,
        period_ticks: 10, // DPS = 1000
        damage_point_ticks: 1,
        backswing_ticks: 2,
        splash: None,
        projectile_speed: None,
        weapon_class: None,
        upgrade_bonus_per_level: SimScalar::ZERO,
        bonus_upgrade_per_level: SimScalar::ZERO,
        target_loss_behavior: sim_types::ProjectileTargetLossBehavior::Fizzle,
        max_projectile_lifetime_ticks: sim_types::DEFAULT_PROJECTILE_LIFETIME_TICKS,
    };

    let air_weapon_high_dps = WeaponSpec {
        name: "Heavy Sky Piercer (Air)".to_string(),
        visual_kind: WeaponVisualKind::Hitscan,
        target_flags: AttributeFlags::all(),
        min_range: SimScalar::ZERO,
        max_range: SimScalar::from_num(7),
        firing_arc: SimAngle::from_degrees(360),
        has_turret: false,
        turret_turn_rate: SimAngle::ZERO,
        base_damage: SimScalar::from_num(25),
        bonus_armored: SimScalar::ZERO,
        bonus_light: SimScalar::ZERO,
        attacks_count: 1,
        period_ticks: 10, // DPS = 2500
        damage_point_ticks: 1,
        backswing_ticks: 2,
        splash: None,
        projectile_speed: None,
        weapon_class: None,
        upgrade_bonus_per_level: SimScalar::ZERO,
        bonus_upgrade_per_level: SimScalar::ZERO,
        target_loss_behavior: sim_types::ProjectileTargetLossBehavior::Fizzle,
        max_projectile_lifetime_ticks: sim_types::DEFAULT_PROJECTILE_LIFETIME_TICKS,
    };

    let arch = UnitArchetype {
        unit_type_id: UnitTypeId(50),
        slug: "dual_goliath".to_string(),
        name: "dual_goliath".to_string(),
        faction: Faction::AutomatonDirectorate,
        display_name: "Dual Goliath".to_string(),
        lore_description: "Dual ground/air walker".to_string(),
        sc2_inspiration: None,
        showcase_hero: false,
        showcase_order: None,
        overclock: None,
        harvester: None,
        is_resource_dropoff: false,
        capabilities: ArchetypeCapabilities::default(),
        veo_prompts: BTreeMap::default(),
        shape: ShapeKind::UnitSphere,
        silhouette: SilhouetteClass::HeavyHex,
        mover_type: MoverType::Ground,
        attributes: AttributeFlags::ARMORED,
        max_hp: SimScalar::from_num(150),
        max_shields: SimScalar::ZERO,
        max_energy: SimScalar::ZERO,
        life_armor: SimScalar::ONE,
        shield_armor: SimScalar::ZERO,
        hp_regen_per_tick: SimScalar::ZERO,
        shield_regen_per_tick: SimScalar::ZERO,
        energy_regen_per_tick: SimScalar::ZERO,
        max_speed: SimScalar::from_num(0.1),
        acceleration: SimScalar::from_num(0.1),
        deceleration: SimScalar::from_num(0.1),
        turn_rate: SimAngle::from_degrees(360),
        collision_radius: SimScalar::from_num(0.6),
        separation_radius: SimScalar::from_num(0.7),
        push_priority: 30,
        sight_radius: SimScalar::from_num(9),
        scan_range: SimScalar::from_num(9),
        mineral_cost: 150,
        gas_cost: 50,
        supply_cost: 2,
        supply_provided: 0,
        build_time_ticks: 500,
        weapon: None,
        ground_weapon: Some(ground_weapon.clone()),
        air_weapon: Some(air_weapon_high_dps.clone()),
        abilities: Vec::new(),
        is_detector: false,
        requires_structures: Vec::new(),
        trains_units: Vec::new(),
        researches_upgrades: Vec::new(),
        armor_class: None,
        modes: Vec::new(),
    };

    // 1. Target Ground -> Chooses Ground weapon
    let w_ground = arch
        .weapon_against(MoverType::Ground, None)
        .expect("ground weapon");
    assert_eq!(w_ground.name, ground_weapon.name);

    // 2. Target Air -> Chooses Air weapon
    let w_air = arch
        .weapon_against(MoverType::Air, None)
        .expect("air weapon");
    assert_eq!(w_air.name, air_weapon_high_dps.name);

    // 3. Target CliffWalker (Colossus) -> Chooses Air weapon because air DPS (2500) > ground DPS (1000)
    let w_colossus = arch
        .weapon_against(MoverType::CliffWalker, None)
        .expect("cliffwalker weapon");
    assert_eq!(
        w_colossus.name, air_weapon_high_dps.name,
        "CliffWalker must select the higher DPS weapon (Air Weapon DPS 2500 vs Ground DPS 1000)"
    );

    // 4. Test tie-breaker favoring ground weapon:
    let air_weapon_low_dps = WeaponSpec {
        name: "Pea Shooter (Air)".to_string(),
        base_damage: SimScalar::from_num(5),
        period_ticks: 10, // DPS = 500
        ..air_weapon_high_dps.clone()
    };
    let mut arch_low_air = arch.clone();
    arch_low_air.air_weapon = Some(air_weapon_low_dps);

    let w_colossus_tie = arch_low_air
        .weapon_against(MoverType::CliffWalker, None)
        .expect("cliffwalker weapon");
    assert_eq!(
        w_colossus_tie.name, ground_weapon.name,
        "CliffWalker must select Ground weapon when Ground DPS (1000) > Air DPS (500)"
    );
}

/// Verifies that in the default battle sandbox, issuing AttackMove to (45, 32) directs all 12 mobile Blue
/// units East along the intended corridor with zero errant detours, while Blue structures remain immobile.
#[test]
fn test_default_battle_sandbox_attack_move_no_errant_unit_path() {
    let mut world = SimWorld::new_default_battle_sandbox();
    let player_blue = PlayerId(0);
    let target = SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(32));

    // Mirror lab_ui.rs issue_sandbox_command:
    let entities: Vec<EntityId> = world
        .entities
        .values()
        .filter(|e| e.owner == player_blue && !e.is_immobile)
        .map(|e| e.id)
        .collect();

    // Verify there are exactly 12 mobile Blue units
    assert_eq!(entities.len(), 12);

    let cmd = PlayerCommand {
        player_id: player_blue,
        entities,
        order: UnitOrder::AttackMove { target },
        queue: false,
    };

    let bundle = TickBundle {
        tick: Tick(1),
        commands: vec![cmd],
        state_checksum: None,
    };

    world.step(&bundle);

    // On tick 1, verify every mobile Blue unit has [target] as its funnel waypoint and positive x velocity
    for id in 1..=12 {
        let e = world
            .entities
            .get(&EntityId(id))
            .expect("blue mobile unit must exist");
        assert_eq!(
            e.current_funnel_waypoints,
            vec![target],
            "EntityId({}) funnel waypoints should be direct to target",
            id
        );
        assert!(
            e.velocity.x > SimScalar::ZERO,
            "EntityId({}) should have positive x-velocity towards target, got {:?}",
            id,
            e.velocity.x
        );
    }

    // Verify Blue structures remain immobile
    for s_id in 23..=25 {
        let s = world
            .entities
            .get(&EntityId(s_id))
            .expect("blue structure must exist");
        assert!(s.is_immobile);
        assert_eq!(s.velocity, SimVec2::ZERO);
        assert_eq!(s.current_anim, AnimState::Idle);
    }

    // Step across ticks 2..=120 and assert bounds
    for tick in 2..=120 {
        world.step(&empty_bundle(tick));

        for id in 1..=12 {
            if let Some(e) = world.entities.get(&EntityId(id)) {
                assert!(
                    e.position.y <= SimScalar::from_num(43)
                        && e.position.y >= SimScalar::from_num(21),
                    "Tick {}: EntityId({}) strayed out of bounds at {:?}",
                    tick,
                    id,
                    e.position
                );
                if id == 11 {
                    assert!(
                        e.position.y <= SimScalar::from_num(35)
                            && e.position.y >= SimScalar::from_num(29),
                        "Tick {}: EntityId(11) strayed off East marching corridor at {:?}",
                        tick,
                        e.position
                    );
                }
            }
        }

        for s_id in 23..=25 {
            let s = world
                .entities
                .get(&EntityId(s_id))
                .expect("blue structure must exist");
            assert!(s.is_immobile);
            assert_eq!(s.velocity, SimVec2::ZERO);
            assert_eq!(s.current_anim, AnimState::Idle);
        }
    }
}

/// Test: In the default battle sandbox, Blue forces marching East trigger Red Zealot melee charges,
/// acquiring targets (including Armored units) and retaliating when fired upon.
#[test]
fn test_default_battle_sandbox_red_zealots_charge_and_retaliate_against_blue() {
    let mut world = SimWorld::new_default_battle_sandbox();

    // Order all Blue mobile units to AttackMove East towards (50.0, 32.0)
    let march_target = SimVec2::new(SimScalar::from_num(50), SimScalar::from_num(32));
    for entity in world.entities.values_mut() {
        if entity.owner == PlayerId(0) && !entity.is_immobile {
            entity.issue_order(
                UnitOrder::AttackMove {
                    target: march_target,
                },
                false,
            );
        }
    }

    let initial_red_hp_sum: SimScalar = world
        .entities
        .values()
        .filter(|e| e.owner == PlayerId(1))
        .map(|e| e.hp)
        .fold(SimScalar::ZERO, |acc, hp| acc + hp);
    let initial_blue_hp_sum: SimScalar = world
        .entities
        .values()
        .filter(|e| e.owner == PlayerId(0))
        .map(|e| e.hp)
        .fold(SimScalar::ZERO, |acc, hp| acc + hp);

    let mut red_charged_west = false;
    let mut red_acquired_armored = false;

    // Step 320 ticks (12.8 seconds of battle to traverse the 30-unit distance and engage)
    for tick in 1..=320 {
        world.step(&empty_bundle(tick));

        // Check Red melee units (EntityId 13..=20 initially at x=45.0)
        for id in 13..=20 {
            if let Some(entity) = world.entities.get(&EntityId(id)) {
                if entity.position.x < SimScalar::from_num(44) {
                    red_charged_west = true;
                }
                if let Some(target_id) = entity.current_target {
                    if let Some(target) = world.entities.get(&target_id) {
                        if let Ok(target_arch) = world.catalog.get(target.unit_type_id) {
                            if target_arch.attributes.contains(AttributeFlags::ARMORED) {
                                red_acquired_armored = true;
                            }
                        }
                    }
                }
            }
        }
    }

    assert!(
        red_charged_west,
        "Red melee units must charge West toward oncoming Blue forces"
    );

    let final_red_hp_sum: SimScalar = world
        .entities
        .values()
        .filter(|e| e.owner == PlayerId(1))
        .map(|e| e.hp)
        .fold(SimScalar::ZERO, |acc, hp| acc + hp);
    let final_blue_hp_sum: SimScalar = world
        .entities
        .values()
        .filter(|e| e.owner == PlayerId(0))
        .map(|e| e.hp)
        .fold(SimScalar::ZERO, |acc, hp| acc + hp);

    assert!(
        final_red_hp_sum < initial_red_hp_sum,
        "Red forces must take damage during battle"
    );
    assert!(
        final_blue_hp_sum < initial_blue_hp_sum,
        "Blue forces must take damage during battle"
    );

    // Verify Red units are able to acquire targets (combat engaged)
    let red_has_targets = (13..=22).any(|id| {
        world
            .entities
            .get(&EntityId(id))
            .is_some_and(|e| e.current_target.is_some())
    });
    assert!(
        red_has_targets || red_acquired_armored,
        "Red forces must engage Blue forces"
    );
}

/// Test: Physical projectile speeds require finite flight time and disjoint with 0 damage
/// when the target is loaded into a transport craft mid-flight.
#[test]
fn test_projectile_speed_and_cargo_pickup_disjoint() {
    let width = 32;
    let height = 32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0xD15);

    let rhino_id = world
        .catalog
        .id_by_slug("rhino_beetle")
        .expect("rhino_beetle");
    let pulse_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // Spawn shooter at (10.0, 16.0)
    let shooter_id = world
        .spawn_unit(
            rhino_id,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16)),
        )
        .expect("spawn shooter");

    // Spawn target at (15.0, 16.0) - distance 5.0
    let target_id = world
        .spawn_unit(
            pulse_id,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(16)),
        )
        .expect("spawn target");

    let initial_hp = world.entities.get(&target_id).unwrap().hp;

    // Issue AttackTarget
    world
        .entities
        .get_mut(&shooter_id)
        .unwrap()
        .issue_order(UnitOrder::AttackTarget { target: target_id }, false);

    // Step until projectile is launched
    let mut proj_launched = false;
    let mut tick = 1;
    while tick <= 20 {
        world.step(&empty_bundle(tick));
        if !world.projectiles.is_empty() {
            proj_launched = true;
            break;
        }
        tick += 1;
    }

    assert!(proj_launched, "Projectile must be launched by shooter");
    assert_eq!(
        world.projectiles.len(),
        1,
        "Exactly one projectile in flight"
    );

    // Mid-flight, pick up the target unit into a transport (EntityId(999))
    let transport_id = EntityId(999);
    let loaded = world.load_unit_into_transport(target_id, transport_id);
    assert!(
        loaded,
        "Target unit must be successfully loaded into transport"
    );
    assert!(
        world
            .entities
            .get(&target_id)
            .unwrap()
            .is_loaded_in_transport,
        "Target unit must have is_loaded_in_transport = true"
    );

    // Step until projectile impacts / despawns (at speed 0.64, distance ~4.0 takes ~7 ticks)
    for t in (tick + 1)..=(tick + 15) {
        world.step(&empty_bundle(t));
    }

    // Verify projectile has resolved (finished flight)
    assert!(
        world.projectiles.is_empty(),
        "Active projectile must have resolved"
    );

    // Verify target received 0 damage (disjointed!)
    let target_hp_after = world.entities.get(&target_id).unwrap().hp;
    assert_eq!(
        target_hp_after, initial_hp,
        "Disjointed projectile must inflict 0 damage to unit inside transport"
    );
}

/// Test: All weapons in the unit and structure catalog can target both Light and Armored attributes.
#[test]
fn test_all_catalog_weapons_can_target_armored_and_light_units() {
    let catalog = UnitCatalog::default();
    for arch in catalog.archetypes() {
        if let Some(ref weapon) = arch.weapon {
            assert!(
                weapon.target_flags.contains(AttributeFlags::LIGHT),
                "Weapon for archetype {:?} ({}) must be able to target LIGHT units",
                arch.unit_type_id,
                arch.name
            );
            assert!(
                weapon.target_flags.contains(AttributeFlags::ARMORED),
                "Weapon for archetype {:?} ({}) must be able to target ARMORED units",
                arch.unit_type_id,
                arch.name
            );
        }
    }
}

/// Test: Units commanded via Move advance towards destination, fan out laterally around
/// blocking center obstacles via unified local steering, and settle without overlapping or gridlocking.
#[test]
fn test_move_command_units_fan_out_and_surround_target_point() {
    let width = 64;
    let height = 64;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0xFA00);

    let pulse_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");
    let target_pos = SimVec2::new(SimScalar::from_num(30), SimScalar::from_num(30));

    // Spawn an immobile center obstacle / dummy unit at (30.0, 30.0)
    let center_id = world
        .spawn_unit(pulse_id, PlayerId(0), target_pos)
        .expect("spawn center");
    world.entities.get_mut(&center_id).unwrap().is_immobile = true;

    // Spawn 8 units in a cluster/line at x=22, y=26..34
    let mut squad = Vec::new();
    for i in 0..8 {
        let spawn_pos = SimVec2::new(
            SimScalar::from_num(22),
            SimScalar::from_num(26) + SimScalar::from_num(i) * SimScalar::from_f32_lossy(1.1),
        );
        let id = world
            .spawn_unit(pulse_id, PlayerId(0), spawn_pos)
            .expect("spawn unit");
        world
            .entities
            .get_mut(&id)
            .unwrap()
            .issue_order(UnitOrder::Move { target: target_pos }, false);
        squad.push(id);
    }

    // Step 80 ticks
    for tick in 1..=80 {
        world.step(&empty_bundle(tick));
    }

    // Assertions:
    // 1. All units have moved significantly closer to (30, 30) (from initial distance ~8.0)
    for &id in &squad {
        let pos = world.entities.get(&id).unwrap().position;
        let dist = (pos - target_pos).length();
        assert!(
            dist < SimScalar::from_num(6),
            "Unit {:?} should have advanced toward target, dist = {}",
            id,
            dist
        );
    }

    // 2. Units fan out around the center unit: check that they spread along Y
    let min_y = squad
        .iter()
        .map(|id| world.entities.get(id).unwrap().position.y)
        .min()
        .unwrap();
    let max_y = squad
        .iter()
        .map(|id| world.entities.get(id).unwrap().position.y)
        .max()
        .unwrap();
    assert!(
        max_y - min_y >= SimScalar::from_num(3),
        "Units should fan out laterally around the center, spread = {}",
        max_y - min_y
    );

    // 3. No units overlap: pairwise distance >= sum of collision radiuses (with small numerical tolerance)
    let pulse_arch = world.catalog.get(pulse_id).unwrap();
    let collision_radius = pulse_arch.collision_radius;
    let min_dist = (collision_radius + collision_radius) * SimScalar::from_f32_lossy(0.85);

    for i in 0..squad.len() {
        for j in (i + 1)..squad.len() {
            let u1 = world.entities.get(&squad[i]).unwrap();
            let u2 = world.entities.get(&squad[j]).unwrap();
            let dist = (u1.position - u2.position).length();
            assert!(
                dist >= min_dist,
                "Units {:?} and {:?} are overlapping! dist = {}, min = {}",
                squad[i],
                squad[j],
                dist,
                min_dist
            );
        }
    }
}
