//! # Integration Tests: Tactical Spells, Active Abilities & Hazard Zones
//!
//! Verifies:
//! 1. Overclock (Stimpack): HP deduction, speed multiplier, cooldown, and refusal unless life
//!    exceeds the cost (Q0053).
//! 2. Siege Mode: Transformation channeling, mobility lock, long-range splash cannon, and unsiege toggle;
//!    entering and leaving each take their own time plus a seeded random extra (WP11, B-4).
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
use sim_types::AttributeBonuses;
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, AttributeFlags, EntityId, OverclockSpec,
    PlayerId, SimAngle, SimScalar, SimVec2, SplashSpec, Tick, TickBundle, UnitOrder, WeaponSpec,
    WeaponVisualKind,
};

fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// Test 10: Overclock (Stimpack) HP deduction, stat multipliers, and refusal at low life.
#[test]
fn test_overclock_stim_hp_cost_and_stat_multipliers() {
    let width = 32;
    let height = 32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x1111);

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
        leave_name: None,
    };

    world.catalog.get_mut(pulse_bot_id).unwrap().abilities = vec![ability];

    // Initial state: 45 HP, speed multiplier 1.0
    assert_eq!(
        world.entities.get(&unit_id).unwrap().hp,
        SimScalar::from_num(45)
    );
    assert_eq!(
        world
            .entities
            .get(&unit_id)
            .unwrap()
            .speed_multiplier()
            .expect("valid fixture movement buff fold"),
        SimScalar::ONE
    );

    // Cast Overclock ability slot 0
    let cast_success = world.execute_ability(unit_id, 0, None, None);
    assert!(cast_success, "Overclock cast must succeed");

    // HP must be 35, speed multiplier must be 1.5
    let ent = world.entities.get(&unit_id).unwrap();
    assert_eq!(ent.hp, SimScalar::from_num(35), "HP must be deducted by 10");
    assert_eq!(
        ent.speed_multiplier()
            .expect("valid fixture movement buff fold"),
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
        ent_after
            .speed_multiplier()
            .expect("valid fixture movement buff fold"),
        SimScalar::ONE,
        "Overclock buff must expire after 250 ticks"
    );

    // Q0053: a stim needs more life than it costs. At 5 life a 10-life stim is refused and
    // changes nothing; the unit stays alive at 5.
    world.entities.get_mut(&unit_id).unwrap().hp = SimScalar::from_num(5);
    world.entities.get_mut(&unit_id).unwrap().ability_cooldowns[0] = 0; // reset cooldown

    let cast_low_hp = world.execute_ability(unit_id, 0, None, None);
    assert!(
        !cast_low_hp,
        "Overclock cast with less life than its cost must be refused"
    );
    let ent_low = world.entities.get(&unit_id).unwrap();
    assert_eq!(
        ent_low.hp,
        SimScalar::from_num(5),
        "a refused stim costs no life"
    );
    assert_eq!(
        ent_low.ability_cooldowns[0], 0,
        "a refused stim starts no cooldown"
    );
    assert!(
        ent_low.active_buffs.is_empty(),
        "a refused stim adds no buff"
    );
}

/// Q0053 / B-33: Stimpack is refused unless the unit's life is strictly greater than its life
/// cost ("stim can never kill a unit it therefore can't be activated with too little hp").
/// At cost - 1 and at exactly the cost the stim changes nothing (no life, no buff, no
/// cooldown); at cost + 1 it stims and leaves the unit at 1 life.
#[test]
fn stim_is_refused_unless_life_exceeds_its_cost() {
    let map =
        GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0))).expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x5353);
    let pulse_bot_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");
    let spec = OverclockSpec {
        hp_cost: SimScalar::from_num(10),
        duration_ticks: 250,
        speed_multiplier: SimScalar::from_num(1.5),
        attack_rate_multiplier: SimScalar::from_num(1.5),
    };
    world.catalog.get_mut(pulse_bot_id).unwrap().abilities = vec![AbilitySpec {
        name: "Combat Overclock".to_string(),
        target_kind: AbilityTargetKind::InstantSelf,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 250,
        cast_range: SimScalar::ZERO,
        effect: AbilityEffectSpec::Overclock(spec),
        leave_name: None,
    }];

    for (life, admitted) in [(9, false), (10, false), (11, true)] {
        let unit_id = world
            .spawn_unit(
                pulse_bot_id,
                PlayerId(0),
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
            )
            .expect("spawn unit");
        world.entities.get_mut(&unit_id).unwrap().hp = SimScalar::from_num(life);
        let before = world.entities.get(&unit_id).unwrap().clone();

        // The entity's own API and the world's ability cast apply the same rule.
        let mut probe = before.clone();
        assert_eq!(
            probe.apply_overclock(spec).expect("valid overclock spec"),
            admitted,
            "Entity::apply_overclock at {life} life"
        );
        assert_eq!(
            world.execute_ability(unit_id, 0, None, None),
            admitted,
            "cast at {life} life"
        );
        let after = world.entities.get(&unit_id).unwrap();
        if admitted {
            assert_eq!(
                after.hp,
                SimScalar::ONE,
                "cost + 1 life stims down to 1 life"
            );
            assert_eq!(after.active_buffs.len(), 1, "the stim buff is applied");
            assert_ne!(after.ability_cooldowns[0], 0, "the cooldown starts");
            assert_eq!(probe.hp, SimScalar::ONE);
        } else {
            assert_eq!(
                after, &before,
                "refused at {life} life: the unit is unchanged"
            );
            assert_eq!(
                probe, before,
                "refused at {life} life: apply_overclock changes nothing"
            );
        }
    }
}

/// Test 11: Siege Mode transform channel, immobility lock, long-range splash, and unsiege toggle.
#[test]
fn test_siege_mode_toggle_immobility_and_long_range_splash() {
    let width = 32;
    let height = 32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x2222);

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
        acquire_arc: SimAngle::from_degrees(360),
        has_turret: true,
        turret_turn_rate: SimAngle::from_degrees(999),
        turret_idle: sim_types::TurretIdle::Hold,
        base_damage: SimScalar::from_num(40),
        attribute_bonuses: AttributeBonuses::NONE,
        attacks_count: 1,
        strike_interval_ticks: 0,
        followup_max_distance: None,
        period_ticks: 60,
        damage_point_ticks: 2,
        backswing_ticks: 4,
        random_delay_min: SimScalar::ZERO,
        random_delay_max: SimScalar::ZERO,
        splash: Some(SplashSpec {
            zones: vec![
                sim_types::SplashZone {
                    radius: SimScalar::HALF,
                    fraction: SimScalar::ONE,
                },
                sim_types::SplashZone {
                    radius: SimScalar::ONE,
                    fraction: SimScalar::HALF,
                },
                sim_types::SplashZone {
                    radius: SimScalar::from_num(1.5),
                    fraction: SimScalar::from_num(0.25),
                },
            ],
            shape: sim_types::SplashShape::Circle,
            friendly_fire: false,
        }),
        projectile_speed: None,
        weapon_class: None,
        upgrade_bonus_per_level: SimScalar::ZERO,
        bonus_upgrade_per_level: SimScalar::ZERO,
        target_loss_behavior: sim_types::ProjectileTargetLossBehavior::Fizzle,
        max_projectile_lifetime_ticks: sim_types::DEFAULT_PROJECTILE_LIFETIME_TICKS,
        suicide_on_attack: false,
        structure_strike: None,
    };

    let ability = AbilitySpec {
        name: "Siege Mode".to_string(),
        target_kind: AbilityTargetKind::ToggleMode,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 75,
        cast_range: SimScalar::ZERO,
        effect: AbilityEffectSpec::SiegeModeToggle {
            cast_ticks: 5, // 5 ticks for test speed
            sieged_weapon: Box::new(sieged_weapon.clone()),
        },
        leave_name: None,
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
    world
        .entities
        .get_mut(&tank_id)
        .unwrap()
        .issue_order(UnitOrder::attack(enemy_primary), false)
        .expect("valid authored fixture order or buff");

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
    let mut world = sim_fixtures::world(map, 0x3333);

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
        leave_name: None,
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
    let mut world = sim_fixtures::world(map, 0x4444);

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
        effect: AbilityEffectSpec::SpawnHazardZone {
            radius: SimScalar::from_num(3),
            delay_ticks: 0,
            duration_ticks: 60,
            interval_ticks: 10,
            damage_per_interval: SimScalar::from_num(14),
            friendly_fire: true,
        },
        leave_name: None,
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
    let mut world = sim_fixtures::world(map, 0x5555);

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
        leave_name: None,
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
        Some(&UnitOrder::AttackTarget {
            target: red_id,
            origin: sim_types::AttackOrigin::Acquired,
        }),
        "Blue unit must acquire revealed cloaked unit once covered by allied detector"
    );
}

/// The shipped Siege Mode crawler in a world seeded with `seed`, its mode re-timed: entering
/// takes `cast` ticks and leaving `leave`, each plus a random extra of up to `extra` ticks
/// (plan `2026-11-05-sc2-deviation-fixes` WP11, B-4).
fn timed_crawler(seed: u64, cast: u16, leave: u16, extra: f32) -> (SimWorld, EntityId) {
    let map =
        GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0))).expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, seed);
    let crawler = world
        .catalog
        .id_by_slug("railgun_crawler")
        .expect("railgun_crawler");
    let mode = &mut world.catalog.get_mut(crawler).unwrap().modes[0];
    mode.cast_ticks = cast;
    mode.leave_cast_ticks = leave;
    mode.cast_random_delay_max_ticks = SimScalar::from_num(extra);
    mode.leave_random_delay_max_ticks = SimScalar::from_num(extra);
    let tank = world
        .spawn_unit(
            crawler,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16)),
        )
        .expect("spawn tank");
    (world, tank)
}

/// Casts the mode toggle of `tank` and returns the channel it starts, in ticks.
fn toggle(world: &mut SimWorld, tank: EntityId) -> u16 {
    assert!(
        world.execute_ability(tank, 0, None, None),
        "the toggle is admitted"
    );
    world.entities[&tank].channeling_remaining_ticks
}

#[test]
fn b4_siege_mode_enters_and_leaves_with_their_own_times() {
    let (mut world, tank) = timed_crawler(7, 10, 4, 0.0);
    let rng = world.rng.get_word_pos();
    assert_eq!(toggle(&mut world, tank), 10, "entering takes cast_ticks");
    for tick in 0..10 {
        world.step(&empty_bundle(tick));
    }
    assert!(world.entities[&tank].is_sieged, "sieged after cast_ticks");
    assert_eq!(
        toggle(&mut world, tank),
        4,
        "leaving takes leave_cast_ticks"
    );
    for tick in 10..14 {
        world.step(&empty_bundle(tick));
    }
    assert!(
        !world.entities[&tank].is_sieged,
        "unsieged after leave_cast_ticks"
    );
    assert_eq!(world.rng.get_word_pos(), rng, "no random extra, no draw");
}

#[test]
fn b4_siege_mode_adds_a_seeded_random_delay() {
    let channels: Vec<(u16, u16)> = (0..40)
        .map(|seed| {
            let (mut world, tank) = timed_crawler(seed, 10, 4, 3.0);
            let rng = world.rng.get_word_pos();
            let enter = toggle(&mut world, tank);
            assert!(
                world.rng.get_word_pos() > rng,
                "the extra comes from SimWorld::rng"
            );
            for tick in 0..u32::from(enter) {
                world.step(&empty_bundle(tick));
            }
            assert!(world.entities[&tank].is_sieged);
            (enter, toggle(&mut world, tank))
        })
        .collect();
    assert!(
        channels
            .iter()
            .all(|&(enter, leave)| (10..=13).contains(&enter) && (4..=7).contains(&leave)),
        "each direction lasts its time plus 0 to 3 ticks: {channels:?}"
    );
    let distinct: std::collections::BTreeSet<_> = channels.iter().map(|c| c.0).collect();
    assert!(
        distinct.len() >= 3,
        "the extra varies with the seed: {distinct:?}"
    );
    let replay: Vec<u16> = (0..40)
        .map(|seed| {
            let (mut world, tank) = timed_crawler(seed, 10, 4, 3.0);
            toggle(&mut world, tank)
        })
        .collect();
    assert_eq!(
        replay,
        channels.iter().map(|c| c.0).collect::<Vec<_>>(),
        "the same seed draws the same extra"
    );
}

// ---------------------------------------------------------------------------------------------
// Read-only mode queries for the client (plan `2026-11-09-client-mode-commands` WP1a, D-3, D-4).
// ---------------------------------------------------------------------------------------------

/// A 40 × 40 flat world on the shipped catalog with one `slug` of player 0 at (10, 16).
fn one_unit(slug: &str, seed: u64) -> (SimWorld, EntityId) {
    let map =
        GameMap::new(40, 40, MapCell::default_ground(CliffLevel(0))).expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, seed);
    let unit_type = world.catalog.id_by_slug(slug).expect("shipped unit");
    let unit = world
        .spawn_unit(
            unit_type,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16)),
        )
        .expect("spawn unit");
    (world, unit)
}

/// Steps `world` through `ticks` ticks without commands.
fn run(world: &mut SimWorld, ticks: u32) {
    for _ in 0..ticks {
        let tick = world.current_tick;
        world.step(&empty_bundle(tick));
    }
}

/// The settled mode's name and whether the unit will be anchored, for `unit` of `world`.
fn settled(world: &SimWorld, unit: EntityId) -> (Option<String>, bool) {
    let entity = &world.entities[&unit];
    let archetype = world.catalog.get(entity.unit_type_id).expect("archetype");
    (
        entity
            .settled_mode(archetype)
            .map(|mode| mode.mode_name.clone()),
        entity.immobile_after_channel(archetype),
    )
}

/// Asserts that the read-only admission and a real cast of ability 0 agree; returns the verdict.
fn admission_agrees(world: &mut SimWorld, unit: EntityId) -> bool {
    let predicted = world.would_admit_cast(unit, 0, None);
    let cast = world.execute_ability(unit, 0, None, None);
    assert_eq!(
        predicted, cast,
        "would_admit_cast agrees with execute_ability"
    );
    cast
}

#[test]
fn would_admit_cast_agrees_with_execute_ability_on_every_refusal() {
    const UPGRADES: &str = r#"(upgrades: [(
        upgrade_id: 1, slug: "learn_siege", name: "learn_siege", display_name: "Learn Siege",
        faction: "automaton_directorate", level: 1, mineral_cost: 0, gas_cost: 0,
        research_ticks: 1, researched_at: "foundry_core_dome",
        effects: [UnlockAbility(target_slug: "railgun_crawler", ability_slug: "Siege Mode")],
    )])"#;

    // Admitted, then refused while the channel runs.
    let (mut world, tank) = one_unit("railgun_crawler", 3);
    assert!(admission_agrees(&mut world, tank), "a fresh tank may siege");
    assert!(
        !admission_agrees(&mut world, tank),
        "refused while channelling"
    );

    // Dead.
    let (mut world, tank) = one_unit("railgun_crawler", 3);
    world.entities.get_mut(&tank).unwrap().hp = SimScalar::ZERO;
    assert!(!admission_agrees(&mut world, tank), "refused when dead");

    // Carried.
    let (mut world, tank) = one_unit("railgun_crawler", 3);
    let carrier_type = world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.transport_capacity > 0)
        .map(|a| a.unit_type_id)
        .expect("a carrier in the catalog");
    let carrier = world
        .spawn_unit(
            carrier_type,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(11), SimScalar::from_num(16)),
        )
        .expect("spawn carrier");
    assert!(
        world.load_unit_into_transport(tank, carrier),
        "the tank boards"
    );
    assert!(!admission_agrees(&mut world, tank), "refused when carried");

    // Cooldown.
    let (mut world, tank) = one_unit("railgun_crawler", 3);
    world.entities.get_mut(&tank).unwrap().ability_cooldowns[0] = 5;
    assert!(!admission_agrees(&mut world, tank), "refused on cooldown");

    // Energy.
    let (mut world, tank) = one_unit("railgun_crawler", 3);
    let crawler = world.entities[&tank].unit_type_id;
    world.catalog.get_mut(crawler).unwrap().abilities[0].energy_cost = SimScalar::from_num(10);
    world.entities.get_mut(&tank).unwrap().energy = SimScalar::from_num(9);
    assert!(
        !admission_agrees(&mut world, tank),
        "refused without energy"
    );

    // Unknown ability index.
    let (world, tank) = one_unit("railgun_crawler", 3);
    assert!(!world.would_admit_cast(tank, 3, None), "no such ability");

    // Locked until researched.
    let (mut world, tank) = one_unit("railgun_crawler", 3);
    world.upgrade_catalog = sim_core::upgrades::UpgradeCatalog::from_ron_str(
        &sim_fixtures::shipped_factions(),
        UPGRADES,
    )
    .unwrap();
    assert!(!admission_agrees(&mut world, tank), "refused while locked");
    world.research_upgrade(PlayerId(0), "learn_siege").unwrap();
    assert!(
        admission_agrees(&mut world, tank),
        "admitted once researched"
    );
}

#[test]
fn settled_mode_follows_both_channels_of_both_shipped_mode_units() {
    for slug in ["railgun_crawler", "burrow_scorpion"] {
        let (mut world, unit) = one_unit(slug, 11);
        let mode = world
            .catalog
            .get(world.entities[&unit].unit_type_id)
            .unwrap()
            .modes[0]
            .mode_name
            .clone();
        assert_eq!(
            settled(&world, unit),
            (None, false),
            "{slug}: default stance"
        );

        assert!(world.execute_ability(unit, 0, None, None));
        assert!(world.entities[&unit].channeling_remaining_ticks > 0);
        assert_eq!(world.entities[&unit].active_mode, None);
        assert_eq!(
            settled(&world, unit),
            (Some(mode.clone()), true),
            "{slug}: entering settles in the mode"
        );
        let enter = u32::from(world.entities[&unit].channeling_remaining_ticks);
        run(&mut world, enter);
        assert_eq!(
            world.entities[&unit].active_mode.as_deref(),
            Some(mode.as_str())
        );
        assert_eq!(
            settled(&world, unit),
            (Some(mode.clone()), true),
            "{slug}: in the mode"
        );

        assert!(world.execute_ability(unit, 0, None, None));
        assert!(
            world.entities[&unit].is_immobile,
            "anchored while it channels"
        );
        assert_eq!(
            settled(&world, unit),
            (None, false),
            "{slug}: leaving settles in no mode and walks afterwards"
        );
        let leave = u32::from(world.entities[&unit].channeling_remaining_ticks);
        run(&mut world, leave);
        assert_eq!(world.entities[&unit].active_mode, None);
        assert_eq!(
            settled(&world, unit),
            (None, false),
            "{slug}: back in default"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// A mode switch given without Shift replaces the unit's orders (plan
// `2026-11-09-client-mode-commands` WP1b, D-6a). StarCraft II's morphs are not `Transient`
// (`liberty` AbilData:1301, :1328), Stimpack is (`liberty` AbilData:1231).
// ---------------------------------------------------------------------------------------------

/// Steps `world` one tick with `order` from player 0 to `units`, queued when `queue`.
fn command(world: &mut SimWorld, units: &[EntityId], order: UnitOrder, queue: bool) {
    let tick = world.current_tick;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands: vec![sim_types::PlayerCommand {
            player_id: PlayerId(0),
            entities: units.to_vec(),
            order,
            queue,
        }],
        state_checksum: None,
    });
}

/// The cast of ability 0 without a target.
fn cast_first() -> UnitOrder {
    UnitOrder::CastAbility {
        ability_index: 0,
        target_pos: None,
        target_entity: None,
    }
}

/// A point on the row y = 16 of [`one_unit`]'s world.
fn row_point(x: i32) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(16))
}

/// A tank of player 0 walking east from (10, 16), and an enemy of player 1 holding at
/// (21, 16), outside tank mode's range and inside Siege Mode's.
fn walking_tank_and_target() -> (SimWorld, EntityId, EntityId) {
    let (mut world, tank) = one_unit("railgun_crawler", 5);
    let enemy_type = world.catalog.id_by_slug("pulse_bot").expect("shipped unit");
    let enemy = world
        .spawn_unit(enemy_type, PlayerId(1), row_point(21))
        .expect("spawn enemy");
    let tick = world.current_tick;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands: vec![sim_types::PlayerCommand {
            player_id: PlayerId(1),
            entities: vec![enemy],
            order: UnitOrder::HoldPosition,
            queue: false,
        }],
        state_checksum: None,
    });
    command(
        &mut world,
        &[tank],
        UnitOrder::Move {
            target: row_point(36),
        },
        false,
    );
    run(&mut world, 3);
    assert!(
        matches!(
            world.entities[&tank].order_queue.front(),
            Some(UnitOrder::Move { .. })
        ),
        "the tank walks"
    );
    (world, tank, enemy)
}

#[test]
fn a_mode_switch_without_shift_drops_the_walk_and_the_anchored_unit_fires() {
    let (mut world, tank, enemy) = walking_tank_and_target();
    let life = world.entities[&enemy].hp;
    command(&mut world, &[tank], cast_first(), false);
    let unit = &world.entities[&tank];
    assert!(
        unit.channeling_remaining_ticks > 0,
        "the switch is admitted"
    );
    assert!(unit.order_queue.is_empty(), "the walk is replaced");
    assert!(!unit.is_holding_position);
    run(&mut world, 200);
    assert!(world.entities[&tank].is_sieged, "sieged where it stood");
    assert!(
        world.entities.get(&enemy).is_none_or(|e| e.hp < life),
        "the sieged tank acquires and fires"
    );
    assert_eq!(world.diagnostics(), []);
}

#[test]
fn a_refused_mode_switch_leaves_the_orders() {
    let (mut world, tank, _) = walking_tank_and_target();
    world.entities.get_mut(&tank).unwrap().ability_cooldowns[0] = 50;
    command(&mut world, &[tank], cast_first(), false);
    let unit = &world.entities[&tank];
    assert_eq!(unit.channeling_remaining_ticks, 0, "the switch is refused");
    assert!(
        matches!(unit.order_queue.front(), Some(UnitOrder::Move { .. })),
        "a refused switch changes nothing"
    );
}

#[test]
fn a_transient_ability_leaves_the_orders() {
    let (mut world, bot) = one_unit("pulse_bot", 5);
    let bot_type = world.entities[&bot].unit_type_id;
    world.catalog.get_mut(bot_type).unwrap().abilities = vec![AbilitySpec {
        name: "Transient Overclock Test".to_string(),
        target_kind: AbilityTargetKind::InstantSelf,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 10,
        cast_range: SimScalar::ZERO,
        effect: AbilityEffectSpec::Overclock(OverclockSpec {
            hp_cost: SimScalar::from_num(10),
            duration_ticks: 100,
            speed_multiplier: SimScalar::from_num(1.5),
            attack_rate_multiplier: SimScalar::from_num(1.5),
        }),
        leave_name: None,
    }];
    command(
        &mut world,
        &[bot],
        UnitOrder::Move {
            target: row_point(36),
        },
        false,
    );
    command(&mut world, &[bot], cast_first(), false);
    let unit = &world.entities[&bot];
    assert!(!unit.active_buffs.is_empty(), "the overclock is admitted");
    assert!(
        matches!(unit.order_queue.front(), Some(UnitOrder::Move { .. })),
        "a transient ability keeps the walk"
    );
}

/// D-6b (plan `2026-11-09-client-mode-commands` WP1c): a switch queued behind an anchored
/// unit's focus order runs once the target is gone. Before, `walking_goal` stopped every
/// anchored unit before it read the queue, so the cast stayed at the front for ever.
#[test]
fn a_queued_switch_behind_a_focus_order_runs_on_an_anchored_unit() {
    let (mut world, tank) = one_unit("railgun_crawler", 9);
    let enemy_type = world.catalog.id_by_slug("pulse_bot").expect("shipped unit");
    let enemy = world
        .spawn_unit(enemy_type, PlayerId(1), row_point(20))
        .expect("spawn enemy");
    world.entities.get_mut(&enemy).unwrap().hp = SimScalar::from_num(1);
    world.entities.get_mut(&enemy).unwrap().max_hp = SimScalar::from_num(1);
    assert!(world.execute_ability(tank, 0, None, None));
    let enter = u32::from(world.entities[&tank].channeling_remaining_ticks);
    // Keep the target away while the tank sieges, then let the focus order kill it.
    world.entities.get_mut(&enemy).unwrap().position = row_point(36);
    run(&mut world, enter);
    assert!(world.entities[&tank].is_sieged);
    world.entities.get_mut(&enemy).unwrap().position = row_point(20);
    command(&mut world, &[tank], UnitOrder::attack(enemy), false);
    command(&mut world, &[tank], cast_first(), true);
    assert!(
        matches!(
            world.entities[&tank].order_queue.back(),
            Some(UnitOrder::CastAbility { .. })
        ),
        "the switch waits behind the focus order"
    );
    run(&mut world, 200);
    assert!(
        world.entities.get(&enemy).is_none_or(|e| !e.is_alive()),
        "the focus target died"
    );
    let unit = &world.entities[&tank];
    assert!(unit.order_queue.is_empty(), "the queued switch ran");
    assert_eq!(unit.active_mode, None, "the tank left Siege Mode");
    assert!(!unit.is_immobile, "and walks again");
    assert_eq!(world.diagnostics(), []);
}
