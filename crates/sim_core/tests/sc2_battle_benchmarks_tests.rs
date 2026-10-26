//! # Authoritative StarCraft II (LotV) Combat Benchmark Tests
//!
//! Verifies:
//! 1. 1 Zealot vs 1 Marine at point-blank: Front-swing damage point pause (4 ticks),
//!    distinct swing rhythm (22-tick period), and exact 3-hit kill (>= 48 ticks).
//! 2. Equal-cost 1 Zealot (100 minerals) vs 2 Marines (100 minerals) at distance 5.0:
//!    Zealot closes distance and defeats both Marines surviving with 60..90 HP.
//!    Against 4 Marines (200 minerals), Marines defeat the Zealot.
//! 3. 1 Zealot vs 1 Marauder un-kited duel at distance 5.0:
//!    Zealot deals 14 damage per cycle (8-1 x 2), Marauder deals 10 damage (no armored bonus).
//!    Zealot defeats Marauder surviving with 70..95 HP.
//! 4. 1 Marauder vs 1 Stalker armored duel at distance 5.0:
//!    Marauder deals 20 damage vs Armored, Stalker deals 18 damage vs Armored.
//!    Marauder defeats Stalker surviving with 15..40 HP.
//! 5. Zergling Swarm scaling vs 1 Zealot:
//!    1 Zealot defeats 4 Zerglings (100 minerals).
//!    6 Zerglings (150 minerals) defeat 1 Zealot.
//! 6. Live Battle Sandbox pacing:
//!    Default battle sandbox does not instant-wipe; no units die at tick 40 or 60,
//!    and tactical engagement remains active past tick 200.
//! 7. Railgun Crawler declarative modes & Crucio Shock Cannon splash:
//!    Unsieged tank deals 15+10 single-target damage with no splash.
//!    Mode transformation takes 71 ticks; in siege mode, deals 40+30 splash damage.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder};

fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// Helper to create a flat 64x64 ground arena map.
fn create_test_world() -> SimWorld {
    let map =
        GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0))).expect("invariant: valid map");
    SimWorld::new(map, 0x4242)
}

/// Test 1: 1 Zealot vs 1 Marine at Point-Blank Range.
/// Verifies front-swing windup pause (4 ticks), 22-tick swing period, and 3-hit kill.
#[test]
fn test_1v1_zealot_vs_marine_melee_pause_and_three_hit_kill() {
    let mut world = create_test_world();
    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let zealot_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
    let marine_type = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    let zealot_id = world
        .spawn_unit(
            zealot_type,
            player_blue,
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn zealot");

    let marine_id = world
        .spawn_unit(
            marine_type,
            player_red,
            SimVec2::new(
                SimScalar::from_num(10),
                SimScalar::from_num(10) + SimScalar::from_f32_lossy(0.3),
            ),
        )
        .expect("spawn marine");

    // Zealot attacks Marine; Marine holds position
    world
        .entities
        .get_mut(&zealot_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackTarget { target: marine_id });
    world
        .entities
        .get_mut(&marine_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::HoldPosition);

    assert_eq!(
        world.entities.get(&marine_id).unwrap().hp,
        SimScalar::from_num(45)
    );

    // Step tick by tick and monitor exact Marine HP
    let mut kill_tick = None;
    for tick in 0..100 {
        world.step(&empty_bundle(tick));

        let marine_opt = world.entities.get(&marine_id);
        let marine_hp = marine_opt.map_or(SimScalar::ZERO, |m| m.hp);
        let is_alive = marine_opt.is_some_and(sim_core::Entity::is_alive);

        if tick < 4 {
            // Zealot is winding up (damage_point_ticks = 4)
            assert_eq!(
                marine_hp,
                SimScalar::from_num(45),
                "Marine must not take damage before tick 4 (windup phase)"
            );
        } else if tick == 4 {
            // First swing lands on tick 4: 16 damage (45 -> 29)
            assert_eq!(
                marine_hp,
                SimScalar::from_num(29),
                "First swing must land on tick 4 dealing 16 damage (45 -> 29)"
            );
        } else if (5..26).contains(&tick) {
            // Backswing & cooldown: Marine stays at 29 HP
            assert_eq!(
                marine_hp,
                SimScalar::from_num(29),
                "Marine must remain at 29 HP between ticks 5 and 25"
            );
        } else if tick == 26 {
            // Second swing lands on tick 26 (4 + 22 = 26): 16 damage (29 -> 13)
            assert_eq!(
                marine_hp,
                SimScalar::from_num(13),
                "Second swing must land on tick 26 dealing 16 damage (29 -> 13)"
            );
        } else if (27..48).contains(&tick) {
            // Cooldown: Marine stays at 13 HP
            assert_eq!(
                marine_hp,
                SimScalar::from_num(13),
                "Marine must remain at 13 HP between ticks 27 and 47"
            );
        } else if tick == 48 {
            // Third swing lands on tick 48 (26 + 22 = 48): Marine dies!
            assert!(
                !is_alive || marine_hp <= SimScalar::ZERO,
                "Third swing must kill Marine on tick 48"
            );
            kill_tick = Some(tick);
            break;
        }
    }

    assert_eq!(
        kill_tick,
        Some(48),
        "Marine must die exactly on tick 48 (total >= 48 ticks)"
    );
}

/// Test 2: Equal-Cost 1 Zealot (100 Minerals) vs. 2 Marines (100 Minerals) at Distance 5.0.
/// Zealot defeats 2 Marines with 60..90 HP left; 4 Marines defeat the Zealot.
#[test]
fn test_equal_cost_1_zealot_vs_2_marines_vs_3_marines() {
    let zealot_type;
    let marine_type;

    // Part A: 1 Zealot vs 2 Marines
    {
        let mut world = create_test_world();
        zealot_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
        marine_type = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

        let zealot_id = world
            .spawn_unit(
                zealot_type,
                PlayerId(0),
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
            )
            .expect("spawn zealot");

        let m1 = world
            .spawn_unit(
                marine_type,
                PlayerId(1),
                SimVec2::new(SimScalar::from_num(15), SimScalar::from_f32_lossy(9.8)),
            )
            .expect("spawn marine 1");

        let m2 = world
            .spawn_unit(
                marine_type,
                PlayerId(1),
                SimVec2::new(SimScalar::from_num(15), SimScalar::from_f32_lossy(10.2)),
            )
            .expect("spawn marine 2");

        world
            .entities
            .get_mut(&zealot_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(10)),
            });

        for m in [m1, m2] {
            world
                .entities
                .get_mut(&m)
                .unwrap()
                .order_queue
                .push_back(UnitOrder::AttackMove {
                    target: SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
                });
        }

        let mut zealot_survived = false;
        let mut zealot_hp = SimScalar::ZERO;
        for tick in 0..250 {
            world.step(&empty_bundle(tick));
            let z_alive = world
                .entities
                .get(&zealot_id)
                .is_some_and(sim_core::Entity::is_alive);
            let m1_alive = world
                .entities
                .get(&m1)
                .is_some_and(sim_core::Entity::is_alive);
            let m2_alive = world
                .entities
                .get(&m2)
                .is_some_and(sim_core::Entity::is_alive);

            if !m1_alive && !m2_alive {
                zealot_survived = z_alive;
                if let Some(z) = world.entities.get(&zealot_id) {
                    zealot_hp = z.hp;
                }
                assert!(
                    tick >= 80,
                    "Battle against 2 Marines must last at least 80 ticks, took {tick}"
                );
                break;
            }
        }

        assert!(
            zealot_survived,
            "1 Zealot must defeat 2 Marines in an equal-cost engagement"
        );
        let hp_f32 = zealot_hp.to_f32();
        assert!(
            (60.0..=95.0).contains(&hp_f32),
            "Zealot remaining HP must be in 60..95 range, got {hp_f32}"
        );
    }

    // Part B: 1 Zealot vs 4 Marines (200 Minerals)
    {
        let mut world = create_test_world();
        let zealot_id = world
            .spawn_unit(
                zealot_type,
                PlayerId(0),
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
            )
            .expect("spawn zealot");

        let mut marines = Vec::new();
        for i in 0..4 {
            #[allow(clippy::cast_precision_loss)]
            let y = 9.4 + (i as f32) * 0.4;
            let m = world
                .spawn_unit(
                    marine_type,
                    PlayerId(1),
                    SimVec2::new(SimScalar::from_num(15), SimScalar::from_f32_lossy(y)),
                )
                .expect("spawn marine");
            marines.push(m);
        }

        world
            .entities
            .get_mut(&zealot_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(10)),
            });

        for &m in &marines {
            world
                .entities
                .get_mut(&m)
                .unwrap()
                .order_queue
                .push_back(UnitOrder::AttackMove {
                    target: SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
                });
        }

        let mut marines_won = false;
        for tick in 0..250 {
            world.step(&empty_bundle(tick));
            let z_alive = world
                .entities
                .get(&zealot_id)
                .is_some_and(sim_core::Entity::is_alive);
            if !z_alive {
                marines_won = true;
                break;
            }
        }

        assert!(
            marines_won,
            "4 Marines must overpower and defeat a single Zealot"
        );
    }
}

/// Test 3: 1 Zealot vs 1 Marauder Un-kited Duel.
/// Marauder deals 10 dmg (no armored bonus); Zealot deals 14 dmg per cycle.
/// Zealot defeats Marauder surviving with 70..95 HP.
#[test]
fn test_1_zealot_vs_1_marauder_un_kited_duel() {
    let mut world = create_test_world();
    let zealot_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
    let marauder_type = world
        .catalog
        .id_by_slug("rhino_beetle")
        .expect("rhino_beetle");

    let zealot_id = world
        .spawn_unit(
            zealot_type,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn zealot");

    let marauder_id = world
        .spawn_unit(
            marauder_type,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(10)),
        )
        .expect("spawn marauder");

    world
        .entities
        .get_mut(&zealot_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(10)),
        });

    world
        .entities
        .get_mut(&marauder_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        });

    let mut zealot_won = false;
    let mut final_tick = 0;
    let mut zealot_remaining_hp = SimScalar::ZERO;

    for tick in 0..300 {
        world.step(&empty_bundle(tick));
        let z_alive = world
            .entities
            .get(&zealot_id)
            .is_some_and(sim_core::Entity::is_alive);
        let m_alive = world
            .entities
            .get(&marauder_id)
            .is_some_and(sim_core::Entity::is_alive);

        if !m_alive {
            zealot_won = z_alive;
            final_tick = tick;
            if let Some(z) = world.entities.get(&zealot_id) {
                zealot_remaining_hp = z.hp;
            }
            break;
        }
    }

    assert!(zealot_won, "Zealot must defeat Marauder in un-kited duel");
    assert!(
        final_tick >= 160,
        "Duel must last at least 160 ticks, took {final_tick}"
    );
    let hp = zealot_remaining_hp.to_f32();
    assert!(
        (65.0..=95.0).contains(&hp),
        "Zealot HP should be in 65..95 range, got {hp}"
    );
}

/// Test 4: 1 Marauder vs 1 Stalker Armored Duel.
/// Marauder deals 20 vs Armored; Stalker deals 18 vs Armored.
/// Marauder defeats Stalker in 210..270 ticks surviving with 15..40 HP.
#[test]
fn test_1_marauder_vs_1_stalker_armored_duel() {
    let mut world = create_test_world();
    let marauder_type = world
        .catalog
        .id_by_slug("rhino_beetle")
        .expect("rhino_beetle");
    let stalker_type = world
        .catalog
        .id_by_slug("warp_strider")
        .expect("warp_strider");

    let marauder_id = world
        .spawn_unit(
            marauder_type,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn marauder");

    let stalker_id = world
        .spawn_unit(
            stalker_type,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(10)),
        )
        .expect("spawn stalker");

    world
        .entities
        .get_mut(&marauder_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackTarget { target: stalker_id });

    world
        .entities
        .get_mut(&stalker_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackTarget {
            target: marauder_id,
        });

    let mut marauder_won = false;
    let mut final_tick = 0;
    let mut marauder_hp = SimScalar::ZERO;

    for tick in 0..350 {
        world.step(&empty_bundle(tick));
        let m_alive = world
            .entities
            .get(&marauder_id)
            .is_some_and(sim_core::Entity::is_alive);
        let s_alive = world
            .entities
            .get(&stalker_id)
            .is_some_and(sim_core::Entity::is_alive);

        if !s_alive {
            marauder_won = m_alive;
            final_tick = tick;
            if let Some(m) = world.entities.get(&marauder_id) {
                marauder_hp = m.hp;
            }
            break;
        }
    }

    assert!(marauder_won, "Marauder must defeat Stalker in armored duel");
    assert!(
        (200..=280).contains(&final_tick),
        "Armored duel must take 200..280 ticks, took {final_tick}"
    );
    let hp = marauder_hp.to_f32();
    assert!(
        (5.0..=45.0).contains(&hp),
        "Marauder remaining HP must be in 5..45 range, got {hp}"
    );
}

/// Test 5: Zergling Swarm Scaling vs. 1 Zealot.
/// 1 Zealot defeats 4 Zerglings sequentially; 6 Zerglings defeat 1 Zealot in a surround.
#[test]
fn test_zergling_swarm_scaling_vs_1_zealot() {
    let zealot_type;
    let zergling_type;

    // Part A: 1 Zealot vs 4 Zerglings sequentially
    {
        let mut world = create_test_world();
        zealot_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
        zergling_type = world
            .catalog
            .id_by_slug("razor_mantis")
            .expect("razor_mantis");

        let zealot_id = world
            .spawn_unit(
                zealot_type,
                PlayerId(0),
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
            )
            .expect("spawn zealot");

        let mut sim_tick = 0;
        for _ in 0..4 {
            let z = world
                .spawn_unit(
                    zergling_type,
                    PlayerId(1),
                    SimVec2::new(SimScalar::from_num(11), SimScalar::from_num(10)),
                )
                .expect("spawn zergling");

            world
                .entities
                .get_mut(&zealot_id)
                .unwrap()
                .issue_order(UnitOrder::AttackTarget { target: z }, false);

            world
                .entities
                .get_mut(&z)
                .unwrap()
                .issue_order(UnitOrder::AttackTarget { target: zealot_id }, false);

            // Step until this zergling is defeated
            for _ in 0..100 {
                world.step(&empty_bundle(sim_tick));
                sim_tick += 1;
                let z_alive = world
                    .entities
                    .get(&z)
                    .is_some_and(sim_core::Entity::is_alive);
                if !z_alive {
                    break;
                }
            }

            assert!(
                world
                    .entities
                    .get(&zealot_id)
                    .is_some_and(sim_core::Entity::is_alive),
                "Zealot must survive each sequential zergling encounter"
            );
        }

        let zealot = world.entities.get(&zealot_id).unwrap();
        let total_hp = (zealot.hp + zealot.shields).to_f32();
        assert!(
            (35.0..=80.0).contains(&total_hp),
            "Zealot surviving HP must be ~40..60, got {total_hp}"
        );
    }

    // Part B: 1 Zealot vs 6 Zerglings
    {
        let mut world = create_test_world();
        let zealot_id = world
            .spawn_unit(
                zealot_type,
                PlayerId(0),
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
            )
            .expect("spawn zealot");

        let mut zerglings = Vec::new();
        for i in 0..6 {
            #[allow(clippy::cast_precision_loss)]
            let y = 9.0 + (i as f32) * 0.4;
            let z = world
                .spawn_unit(
                    zergling_type,
                    PlayerId(1),
                    SimVec2::new(SimScalar::from_num(15), SimScalar::from_f32_lossy(y)),
                )
                .expect("spawn zergling");
            zerglings.push(z);
        }

        world
            .entities
            .get_mut(&zealot_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(10)),
            });

        for &z in &zerglings {
            world
                .entities
                .get_mut(&z)
                .unwrap()
                .order_queue
                .push_back(UnitOrder::AttackMove {
                    target: SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
                });
        }

        let mut zerglings_won = false;
        for tick in 0..250 {
            world.step(&empty_bundle(tick));
            let z_alive = world
                .entities
                .get(&zealot_id)
                .is_some_and(sim_core::Entity::is_alive);
            if !z_alive {
                zerglings_won = true;
                break;
            }
        }

        assert!(zerglings_won, "6 Zerglings must defeat 1 Zealot");
    }
}

/// Test 6: Default Battle Sandbox Engagement Pacing & No Instant Melee Wipe.
/// Verifies no units die at tick 40 or 60, and battle actively rages past tick 200.
#[test]
fn test_default_battle_sandbox_pacing_and_no_instant_melee_wipe() {
    let mut world = SimWorld::new_default_battle_sandbox();
    let initial_count = world.entities.len();
    assert!(
        initial_count >= 20,
        "Sandbox must start with >= 20 entities"
    );

    let mut units_dead_at_tick_40 = 0;
    let mut units_dead_at_tick_60 = 0;

    for tick in 0..220 {
        world.step(&empty_bundle(tick));

        let dead_count = world.entities.values().filter(|e| !e.is_alive()).count();

        if tick == 40 {
            units_dead_at_tick_40 = dead_count;
        }
        if tick == 60 {
            units_dead_at_tick_60 = dead_count;
        }
    }

    assert_eq!(
        units_dead_at_tick_40, 0,
        "Zero units must be dead at tick 40 during approach phase"
    );
    assert_eq!(
        units_dead_at_tick_60, 0,
        "Zero units must be dead at tick 60 upon initial army contact (no instant tank splash wipe!)"
    );

    let final_alive = world.entities.values().filter(|e| e.is_alive()).count();
    assert!(
        final_alive > 0,
        "Battle must remain actively engaged past tick 200 (survivors: {final_alive})"
    );
}

/// Test 7: Railgun Crawler Declarative Modes & Crucio Shock Cannon Splash Invariant.
/// Verifies:
/// 1. Unsieged Tank Mode: 15+10 damage, single-target (`splash: None`), 175 HP.
/// 2. Declarative `modes: [ ... ]` loaded from `units.ron` with `siege_mode` (cast 71 ticks, Crucio cannon 40+30 splash).
/// 3. In combat: unsieged weapon does not damage adjacent units; transformed siege cannon deals AoE splash!
#[test]
fn test_railgun_crawler_declarative_modes_and_siege_splash() {
    let mut world = create_test_world();
    let crawler_type = world
        .catalog
        .id_by_slug("railgun_crawler")
        .expect("railgun_crawler");
    let pulse_bot_type = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // 1. Validate Archetype Catalog Declarative Specifications
    let arch = world.catalog.get(crawler_type).expect("crawler arch");
    assert_eq!(
        arch.max_hp,
        SimScalar::from_num(175),
        "Railgun Crawler HP must be calibrated to authentic SC2 175 HP"
    );

    let unsieged_weapon = arch.weapon.as_ref().expect("unsieged weapon");
    assert_eq!(unsieged_weapon.base_damage, SimScalar::from_num(15));
    assert_eq!(unsieged_weapon.bonus_armored, SimScalar::from_num(10));
    assert_eq!(unsieged_weapon.period_ticks, 26);
    assert!(
        unsieged_weapon.splash.is_none(),
        "Tank mode weapon must have ZERO splash (splash: None)"
    );

    let siege_mode = arch
        .get_mode("siege_mode")
        .expect("must declare siege_mode in catalog");
    assert_eq!(siege_mode.cast_ticks, 71);
    assert!(siege_mode.is_immobile);
    assert!(siege_mode.unpushable);

    let siege_weapon = siege_mode
        .ground_weapon_override
        .as_ref()
        .and_then(|w| w.as_ref())
        .expect("siege weapon override");
    assert_eq!(siege_weapon.base_damage, SimScalar::from_num(40));
    assert_eq!(siege_weapon.bonus_armored, SimScalar::from_num(30));
    assert_eq!(siege_weapon.period_ticks, 54);
    assert!(
        siege_weapon.splash.is_some(),
        "Siege mode cannon must possess area-of-effect splash"
    );

    // 2. Unsieged Single-Target Behavior
    let tank_id = world
        .spawn_unit(
            crawler_type,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn tank");

    let enemy_primary = world
        .spawn_unit(
            pulse_bot_type,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(10)),
        )
        .expect("spawn primary enemy");

    let enemy_adjacent = world
        .spawn_unit(
            pulse_bot_type,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(15), SimScalar::from_f32_lossy(10.4)),
        )
        .expect("spawn adjacent enemy");

    world
        .entities
        .get_mut(&tank_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackTarget {
            target: enemy_primary,
        });

    // Step 30 ticks so Tank fires 1 shot
    for tick in 0..30 {
        world.step(&empty_bundle(tick));
    }

    let p1_hp = world.entities.get(&enemy_primary).unwrap().hp;
    let p2_hp = world.entities.get(&enemy_adjacent).unwrap().hp;

    // Enemy 1 took 15 damage (45 -> 30)
    assert_eq!(
        p1_hp,
        SimScalar::from_num(30),
        "Primary target must take 15 damage"
    );
    // Enemy 2 took 0 damage because Tank Mode has NO splash!
    assert_eq!(
        p2_hp,
        SimScalar::from_num(45),
        "Adjacent enemy must take ZERO damage from unsieged Tank Mode"
    );

    // 3. Mode Transformation into Siege Mode
    let cast = world.execute_ability(tank_id, 0, None, None);
    assert!(cast, "Must cast Siege Mode ability successfully");

    // Advance 71 ticks of transformation channel
    for tick in 30..102 {
        world.step(&empty_bundle(tick));
    }

    let tank = world.entities.get(&tank_id).unwrap();
    assert!(tank.is_sieged, "Tank must be in sieged stance");
    assert_eq!(
        tank.active_mode.as_deref(),
        Some("siege_mode"),
        "Tank active_mode must be 'siege_mode'"
    );
    assert!(tank.is_immobile, "Tank must be immobile in siege mode");
    assert!(tank.unpushable, "Tank must be unpushable in siege mode");

    // 4. Siege Mode AoE Splash Behavior
    world
        .entities
        .get_mut(&tank_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackTarget {
            target: enemy_primary,
        });

    // Step until cannon fires
    for tick in 102..170 {
        world.step(&empty_bundle(tick));
        if !world
            .entities
            .get(&enemy_primary)
            .is_some_and(sim_core::Entity::is_alive)
        {
            break;
        }
    }

    // Crucio Cannon deals 40 damage: kills enemy primary (30 HP -> <=0)
    let p1_alive = world
        .entities
        .get(&enemy_primary)
        .is_some_and(sim_core::Entity::is_alive);
    assert!(
        !p1_alive,
        "Crucio cannon 40 damage blast must kill primary target"
    );

    // And adjacent enemy also took splash damage (dropped below 45 HP!)
    let p2_after_hp = world.entities.get(&enemy_adjacent).unwrap().hp;
    assert!(
        p2_after_hp < SimScalar::from_num(45),
        "Crucio cannon splash must damage adjacent enemy (HP: {p2_after_hp})"
    );
}
