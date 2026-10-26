//! # Integration Tests: Tactical Spells, Active Abilities & Hazard Zones
//!
//! Verifies:
//! 1. Overclock (Stimpack): HP deduction, speed multiplier, cooldown, and 1 HP minimum safety clamp.
//! 2. Siege Mode: Transformation channeling, mobility lock, long-range splash cannon, and unsiege toggle.
//! 3. Blink Teleport: Instant Euclidean relocation across unpathable cliff chasms and rejection of invalid terrain.
//! 4. Psionic Storm: Continuous AoE hazard zone with strict non-stacking damage invariant.
//! 5. Tactical Cloak & Detector Reveal: Invisibility under enemy line-of-sight and detection reveal.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::SimWorld;
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, AttributeFlags, OverclockSpec, PlayerId,
    SimAngle, SimScalar, SimVec2, SplashSpec, Tick, TickBundle, UnitOrder, WeaponSpec,
    WeaponVisualKind,
};

fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// Test 10: Overclock (Stimpack) HP deduction, stat multipliers, and 1 HP minimum clamp.
#[test]
fn test_overclock_stim_hp_cost_and_stat_multipliers() {
    let width = 32;
    let height = 32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0x1111);

    let player_blue = PlayerId(0);
    let pulse_bot_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    let unit_id = world
        .spawn_unit(
            pulse_bot_id,
            player_blue,
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn unit");

    // Give unit an explicit Overclock ability in its capabilities
    let overclock_spec = OverclockSpec {
        hp_cost: SimScalar::from_num(10),
        duration_ticks: 250,
        speed_multiplier: SimScalar::from_num(1.5),
        attack_rate_multiplier: SimScalar::from_num(1.5),
    };

    let ability = AbilitySpec {
        name: "Combat Overclock".to_string(),
        target_kind: AbilityTargetKind::InstantSelf,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 250,
        cast_range: SimScalar::ZERO,
        effect: AbilityEffectSpec::Overclock(overclock_spec),
    };

    world.catalog.get_mut(pulse_bot_id).unwrap().abilities = vec![ability];

    // Initial state: 45 HP, speed multiplier 1.0
    assert_eq!(
        world.entities.get(&unit_id).unwrap().hp,
        SimScalar::from_num(45)
    );
    assert_eq!(
        world.entities.get(&unit_id).unwrap().speed_multiplier(),
        SimScalar::ONE
    );

    // Cast Overclock ability slot 0
    let cast_success = world.execute_ability(unit_id, 0, None, None);
    assert!(cast_success, "Overclock cast must succeed");

    // HP must be 35, speed multiplier must be 1.5
    let ent = world.entities.get(&unit_id).unwrap();
    assert_eq!(ent.hp, SimScalar::from_num(35), "HP must be deducted by 10");
    assert_eq!(
        ent.speed_multiplier(),
        SimScalar::from_num(1.5),
        "Speed must be boosted by 1.5x"
    );

    // Fast-forward 250 ticks
    for tick in 1..=250 {
        world.step(&empty_bundle(tick));
    }

    // Buff must have expired cleanly
    let ent_after = world.entities.get(&unit_id).unwrap();
    assert_eq!(
        ent_after.speed_multiplier(),
        SimScalar::ONE,
        "Overclock buff must expire after 250 ticks"
    );

    // Test safety clamp: reduce HP to 5. Casting Overclock (10 HP cost) must not kill the unit;
    // it must clamp to at least 1 HP minimum.
    world.entities.get_mut(&unit_id).unwrap().hp = SimScalar::from_num(5);
    world.entities.get_mut(&unit_id).unwrap().ability_cooldowns[0] = 0; // reset cooldown

    let cast_low_hp = world.execute_ability(unit_id, 0, None, None);
    assert!(
        cast_low_hp,
        "Overclock cast with low HP must succeed without killing unit"
    );
    let ent_low = world.entities.get(&unit_id).unwrap();
    assert_eq!(ent_low.hp, SimScalar::ONE, "HP must be clamped to 1 HP min");
    assert!(ent_low.is_alive(), "Unit must remain alive");
}

/// Test 11: Siege Mode transform channel, immobility lock, long-range splash, and unsiege toggle.
#[test]
fn test_siege_mode_toggle_immobility_and_long_range_splash() {
    let width = 32;
    let height = 32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0x2222);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let crawler_id = world
        .catalog
        .id_by_slug("railgun_crawler")
        .expect("railgun_crawler");
    let pulse_bot_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // Spawn Crawler at (10.0, 16.0)
    let tank_id = world
        .spawn_unit(
            crawler_id,
            player_blue,
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16)),
        )
        .expect("spawn tank");

    let sieged_weapon = WeaponSpec {
        name: "Shock Cannon".to_string(),
        visual_kind: WeaponVisualKind::Hitscan,
        target_flags: AttributeFlags::all(),
        min_range: SimScalar::from_num(2),
        max_range: SimScalar::from_num(13),
        firing_arc: SimAngle::from_degrees(360),
        has_turret: true,
        turret_turn_rate: SimAngle::from_degrees(999),
        base_damage: SimScalar::from_num(40),
        bonus_armored: SimScalar::ZERO,
        bonus_light: SimScalar::ZERO,
        attacks_count: 1,
        period_ticks: 60,
        damage_point_ticks: 2,
        backswing_ticks: 4,
        splash: Some(SplashSpec {
            inner_radius: SimScalar::HALF,
            middle_radius: SimScalar::ONE,
            outer_radius: SimScalar::from_num(1.5),
            friendly_fire: false,
        }),
        projectile_speed: None,
        weapon_class: None,
        upgrade_bonus_per_level: SimScalar::ZERO,
        bonus_upgrade_per_level: SimScalar::ZERO,
        target_loss_behavior: sim_types::ProjectileTargetLossBehavior::Fizzle,
        max_projectile_lifetime_ticks: sim_types::DEFAULT_PROJECTILE_LIFETIME_TICKS,
    };

    let ability = AbilitySpec {
        name: "Siege Mode".to_string(),
        target_kind: AbilityTargetKind::ToggleMode,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 75,
        cast_range: SimScalar::ZERO,
        effect: AbilityEffectSpec::SiegeModeToggle {
            cast_ticks: 5, // 5 ticks for test speed
            sieged_weapon: sieged_weapon.clone(),
        },
    };

    world.catalog.get_mut(crawler_id).unwrap().abilities = vec![ability];

    // Spawn primary enemy at (20.0, 16.0) - distance 10.0 (beyond mobile tank range 7.0, within siege range 13.0!)
    let enemy_primary = world
        .spawn_unit(
            pulse_bot_id,
            player_red,
            SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(16)),
        )
        .expect("spawn primary enemy");

    // Spawn secondary enemy at (20.5, 16.0) - distance 0.5 from primary target (inside splash inner radius 0.5!)
    let enemy_secondary = world
        .spawn_unit(
            pulse_bot_id,
            player_red,
            SimVec2::new(SimScalar::from_num(20.5), SimScalar::from_num(16)),
        )
        .expect("spawn secondary enemy");

    // Trigger Siege Mode transformation
    let cast = world.execute_ability(tank_id, 0, None, None);
    assert!(cast, "Siege Mode cast must succeed");

    // Verify unit is channeling and immobile
    assert!(world.entities.get(&tank_id).unwrap().is_immobile);
    assert!(world.entities.get(&tank_id).unwrap().unpushable);

    // Advance 5 ticks for channel completion
    for tick in 1..=5 {
        world.step(&empty_bundle(tick));
    }

    assert!(
        world.entities.get(&tank_id).unwrap().is_sieged,
        "Unit must now be sieged"
    );
    assert!(
        world.entities.get(&tank_id).unwrap().is_immobile,
        "Sieged unit must be immobile"
    );

    // Order sieged tank to attack primary enemy
    world.entities.get_mut(&tank_id).unwrap().issue_order(
        UnitOrder::AttackTarget {
            target: enemy_primary,
        },
        false,
    );

    let enemy_primary_hp_before = world.entities.get(&enemy_primary).unwrap().hp;
    let enemy_secondary_hp_before = world.entities.get(&enemy_secondary).unwrap().hp;

    // Advance ticks until weapon fires
    for tick in 6..=20 {
        world.step(&empty_bundle(tick));
        let hp1 = world.entities.get(&enemy_primary).unwrap().hp;
        if hp1 < enemy_primary_hp_before {
            break;
        }
    }

    let enemy_primary_hp_after = world.entities.get(&enemy_primary).unwrap().hp;
    let enemy_secondary_hp_after = world.entities.get(&enemy_secondary).unwrap().hp;

    assert!(
        enemy_primary_hp_after < enemy_primary_hp_before,
        "Primary target must sustain direct long-range siege damage"
    );
    assert!(
        enemy_secondary_hp_after < enemy_secondary_hp_before,
        "Secondary nearby unit must sustain circular splash damage from siege impact"
    );

    // Unsiege unit
    world.entities.get_mut(&tank_id).unwrap().ability_cooldowns[0] = 0;
    world.execute_ability(tank_id, 0, None, None);
    for tick in 21..=26 {
        world.step(&empty_bundle(tick));
    }

    assert!(
        !world.entities.get(&tank_id).unwrap().is_sieged,
        "Unit must unsiege cleanly"
    );
    assert!(
        !world.entities.get(&tank_id).unwrap().is_immobile,
        "Unsieged unit must be mobile again"
    );
}

/// Test 12: Blink Teleport across cliff chasm and invalid terrain rejection.
#[test]
fn test_blink_teleport_across_cliff_chasm() {
    let width = 32;
    let height = 32;

    // Build map:
    // Low ground at x < 14
    // Unpassable Cliff Chasm at 14 <= x < 18
    // High ground at x >= 18
    let mut cells = vec![MapCell::default_ground(CliffLevel(0)); (width * height) as usize];
    for y in 0..height {
        for x in 14..18 {
            let idx = (y * width + x) as usize;
            cells[idx].flags.insert(CellFlags::UNPASSABLE_CLIFF);
            cells[idx].flags.remove(CellFlags::GROUND_PATHABLE);
        }
        for x in 18..width {
            let idx = (y * width + x) as usize;
            cells[idx] = MapCell::default_ground(CliffLevel(1));
        }
    }

    let map = GameMap::from_cells(width, height, cells, Vec::new(), Vec::new())
        .expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0x3333);

    let player_blue = PlayerId(0);
    let crawler_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // Spawn unit at (12.0, 16.0)
    let unit_id = world
        .spawn_unit(
            crawler_id,
            player_blue,
            SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(16)),
        )
        .expect("spawn unit");

    let ability = AbilitySpec {
        name: "Phase Blink".to_string(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 175,
        cast_range: SimScalar::from_num(8),
        effect: AbilityEffectSpec::BlinkTeleport {
            max_range: SimScalar::from_num(8),
            cooldown_ticks: 175,
        },
    };

    world.catalog.get_mut(crawler_id).unwrap().abilities = vec![ability];

    // 1. Attempt invalid blink into unpassable cliff chasm at (16.0, 16.0)
    let invalid_dest = SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(16));
    let fail_blink = world.execute_ability(unit_id, 0, Some(invalid_dest), None);
    assert!(
        !fail_blink,
        "Blink into unpassable cliff chasm must be rejected"
    );
    assert_eq!(
        world.entities.get(&unit_id).unwrap().position,
        SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(16)),
        "Unit position must not change on failed blink"
    );

    // 2. Successful blink across chasm onto high ground at (19.0, 16.0) (distance 7.0 <= 8.0)
    let valid_dest = SimVec2::new(SimScalar::from_num(19), SimScalar::from_num(16));
    let success_blink = world.execute_ability(unit_id, 0, Some(valid_dest), None);
    assert!(
        success_blink,
        "Blink across cliff chasm to valid high ground must succeed"
    );
    assert_eq!(
        world.entities.get(&unit_id).unwrap().position,
        valid_dest,
        "Unit must immediately materialize at teleport destination"
    );
    assert_eq!(
        world.entities.get(&unit_id).unwrap().ability_cooldowns[0],
        175,
        "Blink cooldown must be incurred"
    );
}

/// Test 13: Psionic Storm hazard zone AoE damage and non-stacking damage invariant.
#[test]
fn test_psionic_storm_hazard_zone_aoe_and_non_stacking() {
    let width = 32;
    let height = 32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0x4444);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let caster_type_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");
    let target_type_id = world
        .catalog
        .id_by_slug("rhino_beetle")
        .expect("rhino_beetle");

    let caster_id = world
        .spawn_unit(
            caster_type_id,
            player_blue,
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn caster");

    // Spawn 2 target enemies within storm radius at (15.0, 15.0) and (16.0, 15.0)
    let t1 = world
        .spawn_unit(
            target_type_id,
            player_red,
            SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(15)),
        )
        .expect("spawn t1");
    let t2 = world
        .spawn_unit(
            target_type_id,
            player_red,
            SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(15)),
        )
        .expect("spawn t2");

    let storm_ability = AbilitySpec {
        name: "Psionic Storm".to_string(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 50,
        cast_range: SimScalar::from_num(10),
        effect: AbilityEffectSpec::PsionicStormAoE {
            radius: SimScalar::from_num(3),
            delay_ticks: 0,
            duration_ticks: 60,
            interval_ticks: 10,
            damage_per_interval: SimScalar::from_num(14),
            friendly_fire: true,
        },
    };

    world.catalog.get_mut(caster_type_id).unwrap().abilities = vec![storm_ability];

    let storm_center = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(15));

    // Spawn TWO overlapping storms at the EXACT same center coordinate
    world.execute_ability(caster_id, 0, Some(storm_center), None);
    world
        .entities
        .get_mut(&caster_id)
        .unwrap()
        .ability_cooldowns[0] = 0;
    world.execute_ability(caster_id, 0, Some(storm_center), None);

    assert_eq!(
        world.active_hazards.len(),
        2,
        "Two active hazard zones must be present in the world"
    );

    let t1_hp_before = world.entities.get(&t1).unwrap().hp;
    let t2_hp_before = world.entities.get(&t2).unwrap().hp;

    // Advance 1 tick to trigger interval damage
    world.step(&empty_bundle(1));

    let t1_hp_after = world.entities.get(&t1).unwrap().hp;
    let t2_hp_after = world.entities.get(&t2).unwrap().hp;

    // Both units must take exactly 14 damage (from ONE storm pulse), NOT 28 damage from both storms!
    // Invariant: Non-Stacking! Multiple overlapping storms do not double-damage in the same tick.
    let t1_damage = t1_hp_before - t1_hp_after;
    let t2_damage = t2_hp_before - t2_hp_after;

    assert_eq!(
        t1_damage,
        SimScalar::from_num(14),
        "Unit inside overlapping storms must sustain exactly 14 damage due to non-stacking invariant"
    );
    assert_eq!(
        t2_damage,
        SimScalar::from_num(14),
        "Unit inside overlapping storms must sustain exactly 14 damage due to non-stacking invariant"
    );
}

/// Test 14: Tactical Cloaking invisibility under enemy line-of-sight and detector reveal.
#[test]
fn test_cloak_invisibility_and_detector_reveal() {
    let width = 32;
    let height = 32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0x5555);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let pulse_bot_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");
    let turret_id = world
        .catalog
        .id_by_slug("tesla_sentinel_dome")
        .expect("tesla_sentinel_dome");

    // Blue attacker at (10.0, 16.0)
    let blue_id = world
        .spawn_unit(
            pulse_bot_id,
            player_blue,
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16)),
        )
        .expect("spawn blue");

    // Red cloaked ghost at (13.0, 16.0) - in direct line of sight (distance 3.0)
    let red_id = world
        .spawn_unit(
            pulse_bot_id,
            player_red,
            SimVec2::new(SimScalar::from_num(13), SimScalar::from_num(16)),
        )
        .expect("spawn red");

    let cloak_ability = AbilitySpec {
        name: "Cloak".to_string(),
        target_kind: AbilityTargetKind::ToggleMode,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 25,
        cast_range: SimScalar::ZERO,
        effect: AbilityEffectSpec::CloakToggle {
            energy_cost_per_second: SimScalar::ZERO,
            is_burrow: false,
        },
    };

    world.catalog.get_mut(pulse_bot_id).unwrap().abilities = vec![cloak_ability];

    // Red unit activates Cloak
    world.execute_ability(red_id, 0, None, None);
    assert!(
        world.entities.get(&red_id).unwrap().is_cloaked,
        "Red unit must be cloaked"
    );

    // Update simulation
    world.step(&empty_bundle(1));

    // Blue unit cannot auto-acquire or target cloaked enemy without detection!
    assert_eq!(
        world.entities.get(&blue_id).unwrap().current_target,
        None,
        "Blue unit must not acquire cloaked enemy without a detector"
    );

    // Blue player deploys a detector (Tesla Sentinel Dome with is_detector = true) at (9, 15)
    let _ = world.spawn_structure(turret_id, player_blue, 9, 15);

    // Step next tick: detector reveals cloaked unit in detection radius
    world.step(&empty_bundle(2));

    // Blue unit now acquires the revealed cloaked enemy!
    let target = world.entities.get(&blue_id).unwrap().order_queue.front();
    assert_eq!(
        target,
        Some(&UnitOrder::AttackTarget { target: red_id }),
        "Blue unit must acquire revealed cloaked unit once covered by allied detector"
    );
}
