//! # Integration Test: SC2 Damage Math, Shields & 10s Regen Delay
//!
//! Verifies:
//! 1. Exact SC2 shield absorption, shield armor mitigation, and unmitigated HP spillover.
//! 2. Attribute bonus damage against Armored and Light targets.
//! 3. Multi-hit armor reduction (e.g. Zealot dual attack applying armor reduction twice).
//! 4. Minimum 0.5 HP damage clamping.
//! 5. Circular concentric splash falloff (100% / 50% / 25%).
//! 6. 10-second (250-tick) shield regeneration delay after sustaining damage.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::{
    calculate_hit_damage, calculate_splash_damage, SplashSpec, UnitArchetype, UnitCatalog,
    WeaponSpec, MIN_DAMAGE_CLAMP,
};
use sim_types::{
    ArchetypeCapabilities, AttributeFlags, Faction, MoverType, ShapeKind, SilhouetteClass,
    SimAngle, SimScalar, SimVec2, Tick, TickBundle, UnitTypeId, WeaponVisualKind,
};

fn dummy_target_archetype(
    attributes: AttributeFlags,
    life_armor: SimScalar,
    shield_armor: SimScalar,
) -> UnitArchetype {
    UnitArchetype {
        unit_type_id: UnitTypeId(99),
        slug: "dummy_target".to_string(),
        name: "dummy_target".to_string(),
        faction: Faction::AutomatonDirectorate,
        display_name: "Dummy Target".to_string(),
        lore_description: "Target dummy for unit tests".to_string(),
        sc2_inspiration: None,
        showcase_hero: false,
        showcase_order: None,
        overclock: None,
        harvester: None,
        is_resource_dropoff: false,
        capabilities: ArchetypeCapabilities::default(),
        veo_prompts: std::collections::BTreeMap::new(),
        shape: ShapeKind::UnitSphere,
        silhouette: SilhouetteClass::LightProngs,
        mover_type: MoverType::Ground,
        attributes,
        max_hp: SimScalar::from_num(100),
        max_shields: SimScalar::from_num(50),
        max_energy: SimScalar::ZERO,
        life_armor,
        shield_armor,
        hp_regen_per_tick: SimScalar::ZERO,
        shield_regen_per_tick: SimScalar::from_num(1),
        energy_regen_per_tick: SimScalar::ZERO,
        max_speed: SimScalar::ZERO,
        acceleration: SimScalar::ZERO,
        deceleration: SimScalar::ZERO,
        turn_rate: SimAngle::ZERO,
        collision_radius: SimScalar::HALF,
        separation_radius: SimScalar::HALF,
        push_priority: 10,
        sight_radius: SimScalar::from_num(8),
        scan_range: SimScalar::from_num(8),
        mineral_cost: 0,
        gas_cost: 0,
        supply_cost: 0,
        supply_provided: 0,
        build_time_ticks: 0,
        weapon: None,
        ground_weapon: None,
        air_weapon: None,
        abilities: Vec::new(),
        is_detector: false,
        requires_structures: Vec::new(),
        trains_units: Vec::new(),
        researches_upgrades: Vec::new(),
        armor_class: None,
        modes: Vec::new(),
    }
}

fn dummy_weapon_spec(
    base: SimScalar,
    bonus_armored: SimScalar,
    bonus_light: SimScalar,
) -> WeaponSpec {
    WeaponSpec {
        name: "TestGun".to_string(),
        visual_kind: WeaponVisualKind::Hitscan,
        target_flags: AttributeFlags::all(),
        min_range: SimScalar::ZERO,
        max_range: SimScalar::from_num(5),
        firing_arc: SimAngle::from_degrees(180),
        has_turret: false,
        turret_turn_rate: SimAngle::ZERO,
        base_damage: base,
        bonus_armored,
        bonus_light,
        attacks_count: 1,
        period_ticks: 20,
        damage_point_ticks: 2,
        backswing_ticks: 4,
        splash: None,
        projectile_speed: None,
        weapon_class: None,
        upgrade_bonus_per_level: SimScalar::ZERO,
        bonus_upgrade_per_level: SimScalar::ZERO,
        target_loss_behavior: sim_types::ProjectileTargetLossBehavior::Fizzle,
        max_projectile_lifetime_ticks: sim_types::DEFAULT_PROJECTILE_LIFETIME_TICKS,
    }
}

#[test]
fn test_sc2_shield_spillover_exact_formula() {
    // Target with 20 Shields (ShieldArmor = 1) and 100 HP (LifeArmor = 2)
    let arch = dummy_target_archetype(
        AttributeFlags::LIGHT | AttributeFlags::BIOLOGICAL,
        SimScalar::from_num(2),
        SimScalar::from_num(1),
    );
    let target_shields = SimScalar::from_num(20);
    let target_hp = SimScalar::from_num(100);

    // Strike with 30 raw damage
    let weapon = dummy_weapon_spec(SimScalar::from_num(30), SimScalar::ZERO, SimScalar::ZERO);
    let res = calculate_hit_damage(&weapon, &arch, target_shields, target_hp);

    // 1. Shield portion: 30 - 1 = 29 mitigated damage
    // 2. Shields absorb full 20 shields
    assert_eq!(res.shield_damage, SimScalar::from_num(20));

    // 3. Absorbed ratio = 20 / 29
    // Leftover raw = 30 * (1 - 20/29) = 30 * 9 / 29 = 270 / 29
    let leftover_raw = SimScalar::from_num(30)
        * (SimScalar::ONE - SimScalar::from_num(20) / SimScalar::from_num(29));
    let expected_life = (leftover_raw - SimScalar::from_num(2)).max(MIN_DAMAGE_CLAMP);

    assert_eq!(res.life_damage, expected_life);
    assert!(!res.killed);
}

#[test]
fn test_attribute_bonus_damage() {
    let marauder_gun = dummy_weapon_spec(
        SimScalar::from_num(10),
        SimScalar::from_num(10),
        SimScalar::ZERO,
    );

    let armored_target =
        dummy_target_archetype(AttributeFlags::ARMORED, SimScalar::ZERO, SimScalar::ZERO);
    let light_target =
        dummy_target_archetype(AttributeFlags::LIGHT, SimScalar::ZERO, SimScalar::ZERO);

    let res_armored = calculate_hit_damage(
        &marauder_gun,
        &armored_target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(
        res_armored.life_damage,
        SimScalar::from_num(20),
        "Should deal 10 base + 10 armored bonus"
    );

    let res_light = calculate_hit_damage(
        &marauder_gun,
        &light_target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(
        res_light.life_damage,
        SimScalar::from_num(10),
        "Should deal only 10 base against Light"
    );
}

#[test]
fn test_multi_hit_zealot_armor_reduction() {
    let catalog = UnitCatalog::new_starter_roster();
    let zealot = catalog
        .get_by_slug("psi_grey")
        .expect("Psi-Blade Grey exists");
    let weapon = zealot.weapon.as_ref().expect("Zealot weapon exists");

    assert_eq!(
        weapon.attacks_count, 2,
        "Zealot must have 2 attacks per cycle"
    );
    assert_eq!(weapon.base_damage, SimScalar::from_num(8));

    // Target with 2 life armor, 0 shields
    let target = dummy_target_archetype(
        AttributeFlags::ARMORED,
        SimScalar::from_num(2),
        SimScalar::ZERO,
    );

    let mut total_dmg = SimScalar::ZERO;
    let mut cur_hp = SimScalar::from_num(100);

    for _ in 0..weapon.attacks_count {
        let res = calculate_hit_damage(weapon, &target, SimScalar::ZERO, cur_hp);
        total_dmg += res.life_damage;
        cur_hp -= res.life_damage;
    }

    // Each hit is 8 - 2 = 6, total = 12 (instead of 16 - 2 = 14)
    assert_eq!(
        total_dmg,
        SimScalar::from_num(12),
        "Armor must be subtracted from each attack strike"
    );
}

#[test]
fn test_minimum_damage_clamp() {
    let weak_gun = dummy_weapon_spec(SimScalar::from_num(1), SimScalar::ZERO, SimScalar::ZERO);
    let high_armor_target = dummy_target_archetype(
        AttributeFlags::ARMORED,
        SimScalar::from_num(10),
        SimScalar::ZERO,
    );

    let res = calculate_hit_damage(
        &weak_gun,
        &high_armor_target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(
        res.life_damage,
        SimScalar::from_num(0.5),
        "Damage must be clamped to minimum 0.5 HP"
    );
}

#[test]
fn test_circular_splash_falloff() {
    let weapon = dummy_weapon_spec(SimScalar::from_num(40), SimScalar::ZERO, SimScalar::ZERO);
    let splash = SplashSpec {
        inner_radius: SimScalar::from_num(1),
        middle_radius: SimScalar::from_num(2),
        outer_radius: SimScalar::from_num(3),
        friendly_fire: false,
    };
    let target = dummy_target_archetype(AttributeFlags::LIGHT, SimScalar::ZERO, SimScalar::ZERO);

    // Inner: 100% (40 damage)
    let res_inner = calculate_splash_damage(
        &weapon,
        &splash,
        SimScalar::from_num(0.5),
        &target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(res_inner.expect("hit").life_damage, SimScalar::from_num(40));

    // Middle: 50% (20 damage)
    let res_mid = calculate_splash_damage(
        &weapon,
        &splash,
        SimScalar::from_num(1.5),
        &target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(res_mid.expect("hit").life_damage, SimScalar::from_num(20));

    // Outer: 25% (10 damage)
    let res_outer = calculate_splash_damage(
        &weapon,
        &splash,
        SimScalar::from_num(2.5),
        &target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(res_outer.expect("hit").life_damage, SimScalar::from_num(10));

    // Miss: > outer_radius (None)
    let res_miss = calculate_splash_damage(
        &weapon,
        &splash,
        SimScalar::from_num(3.5),
        &target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert!(res_miss.is_none());
}

#[test]
fn test_shield_regeneration_250_tick_delay() {
    use sim_core::SimWorld;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::PlayerId;

    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0x5555_AAAA);

    let psi_grey_id = world
        .catalog
        .id_by_slug("psi_grey")
        .expect("psi_grey in catalog");
    let zealot_id = world
        .spawn_unit(
            psi_grey_id,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn zealot");

    // Damage Zealot shields from 50 down to 20
    {
        let zealot = world.entities.get_mut(&zealot_id).expect("zealot exists");
        zealot.apply_damage(SimScalar::from_num(30), SimScalar::ZERO, SimAngle::ZERO);
        assert_eq!(zealot.shields, SimScalar::from_num(20));
        assert_eq!(zealot.ticks_since_last_damage, 0);
    }

    let empty_bundle = |tick: u32| TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    };

    // Step 249 ticks: shields must remain exactly 20
    for tick in 1..=249 {
        world.step(&empty_bundle(tick));
        let zealot = world.entities.get(&zealot_id).expect("zealot exists");
        assert_eq!(
            zealot.shields,
            SimScalar::from_num(20),
            "Shields must not regenerate before 250 ticks without damage"
        );
    }

    // Step tick 250: shields begin regenerating
    world.step(&empty_bundle(250));
    {
        let zealot = world.entities.get(&zealot_id).expect("zealot exists");
        assert!(
            zealot.shields > SimScalar::from_num(20),
            "Shields must begin regenerating at tick 250 (10.0 seconds)"
        );
    }
}

/// Test 7: Verify that splash with friendly_fire == true damages nearby allied units,
/// while friendly_fire == false splash protects nearby allied units from harm.
#[test]
fn test_friendly_fire_splash_damages_allies_vs_enemy_only_splash() {
    use sim_core::SimWorld;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{PlayerId, UnitOrder};

    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0x9999);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let pulse_bot_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // Case A: Weapon with friendly_fire: true
    let friendly_fire_weapon = WeaponSpec {
        name: "Artillery Shell (Friendly Fire)".to_string(),
        visual_kind: WeaponVisualKind::Hitscan,
        target_flags: AttributeFlags::all(),
        min_range: SimScalar::ZERO,
        max_range: SimScalar::from_num(10),
        firing_arc: SimAngle::from_degrees(360),
        has_turret: false,
        turret_turn_rate: SimAngle::ZERO,
        base_damage: SimScalar::from_num(40),
        bonus_armored: SimScalar::ZERO,
        bonus_light: SimScalar::ZERO,
        attacks_count: 1,
        period_ticks: 30,
        damage_point_ticks: 1,
        backswing_ticks: 2,
        splash: Some(SplashSpec {
            inner_radius: SimScalar::ONE,
            middle_radius: SimScalar::from_num(1.5),
            outer_radius: SimScalar::from_num(2.0),
            friendly_fire: true,
        }),
        projectile_speed: None,
        weapon_class: None,
        upgrade_bonus_per_level: SimScalar::ZERO,
        bonus_upgrade_per_level: SimScalar::ZERO,
        target_loss_behavior: sim_types::ProjectileTargetLossBehavior::Fizzle,
        max_projectile_lifetime_ticks: sim_types::DEFAULT_PROJECTILE_LIFETIME_TICKS,
    };

    // Case B: Weapon with friendly_fire: false
    let enemy_only_weapon = WeaponSpec {
        name: "Disruptor Shell (Enemy Only)".to_string(),
        splash: Some(SplashSpec {
            inner_radius: SimScalar::ONE,
            middle_radius: SimScalar::from_num(1.5),
            outer_radius: SimScalar::from_num(2.0),
            friendly_fire: false,
        }),
        ..friendly_fire_weapon.clone()
    };

    let empty_bundle = |tick: u32| TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    };

    let mut dummy_arch =
        dummy_target_archetype(AttributeFlags::LIGHT, SimScalar::ZERO, SimScalar::ZERO);
    dummy_arch.max_shields = SimScalar::ZERO;
    world.catalog.register(dummy_arch);
    let dummy_id = UnitTypeId(99);

    // 1. Test friendly_fire: true
    {
        // Attacker (Blue) at (10.0, 15.0)
        let attacker = world
            .spawn_unit(
                pulse_bot_id,
                player_blue,
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(15)),
            )
            .expect("spawn attacker");
        // Primary target (Red) at (15.0, 15.0)
        let target_red = world
            .spawn_unit(
                dummy_id,
                player_red,
                SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(15)),
            )
            .expect("spawn red target");
        // Ally (Blue) at (15.5, 15.0) - within splash inner radius (dist = 0.5 <= 1.0)
        let ally_blue = world
            .spawn_unit(
                dummy_id,
                player_blue,
                SimVec2::new(SimScalar::from_num(15.5), SimScalar::from_num(15)),
            )
            .expect("spawn blue ally");

        // Give attacker the friendly_fire weapon
        world.catalog.get_mut(pulse_bot_id).unwrap().ground_weapon = Some(friendly_fire_weapon);

        world
            .entities
            .get_mut(&attacker)
            .unwrap()
            .issue_order(UnitOrder::AttackTarget { target: target_red }, false);

        let red_hp_before = world.entities.get(&target_red).unwrap().hp;
        let ally_hp_before = world.entities.get(&ally_blue).unwrap().hp;

        for tick in 1..=10 {
            world.step(&empty_bundle(tick));
            if world.entities.get(&target_red).unwrap().hp < red_hp_before {
                break;
            }
        }

        let red_hp_after = world.entities.get(&target_red).unwrap().hp;
        let ally_hp_after = world.entities.get(&ally_blue).unwrap().hp;

        assert!(
            red_hp_after < red_hp_before,
            "Primary red target must take direct hit damage"
        );
        assert!(
            ally_hp_after < ally_hp_before,
            "Blue ally must sustain splash damage when weapon friendly_fire == true"
        );

        // Despawn units
        world.entities.clear();
    }

    // 2. Test friendly_fire: false
    {
        // Attacker (Blue) at (10.0, 15.0)
        let attacker = world
            .spawn_unit(
                pulse_bot_id,
                player_blue,
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(15)),
            )
            .expect("spawn attacker 2");
        // Primary target (Red) at (15.0, 15.0)
        let target_red = world
            .spawn_unit(
                dummy_id,
                player_red,
                SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(15)),
            )
            .expect("spawn red target 2");
        // Ally (Blue) at (15.5, 15.0) - within splash inner radius (dist = 0.5 <= 1.0)
        let ally_blue = world
            .spawn_unit(
                dummy_id,
                player_blue,
                SimVec2::new(SimScalar::from_num(15.5), SimScalar::from_num(15)),
            )
            .expect("spawn blue ally 2");

        // Give attacker the enemy-only splash weapon
        world.catalog.get_mut(pulse_bot_id).unwrap().ground_weapon = Some(enemy_only_weapon);

        world
            .entities
            .get_mut(&attacker)
            .unwrap()
            .issue_order(UnitOrder::AttackTarget { target: target_red }, false);

        let red_hp_before = world.entities.get(&target_red).unwrap().hp;
        let ally_hp_before = world.entities.get(&ally_blue).unwrap().hp;

        for tick in 11..=20 {
            world.step(&empty_bundle(tick));
            if world.entities.get(&target_red).unwrap().hp < red_hp_before {
                break;
            }
        }

        let red_hp_after = world.entities.get(&target_red).unwrap().hp;
        let ally_hp_after = world.entities.get(&ally_blue).unwrap().hp;

        assert!(
            red_hp_after < red_hp_before,
            "Primary red target must take direct hit damage"
        );
        assert_eq!(
            ally_hp_after, ally_hp_before,
            "Blue ally must remain completely unharmed when weapon friendly_fire == false"
        );
    }
}
