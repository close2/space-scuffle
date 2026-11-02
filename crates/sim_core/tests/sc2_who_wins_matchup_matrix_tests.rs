//! # Comprehensive StarCraft II "Who Wins" Matchup Matrix Integration Tests
//!
//! Verifies 35+ distinct canonical StarCraft II: Legacy of the Void (Patch 5.0.14)
//! unit vs. unit, squad vs. squad, mass swarm (100 Zerglings vs. X), and upgrade
//! breakpoint battles across the Automaton Directorate, Cosmic Enclave, and Sylvan Swarm.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::cast_precision_loss,
    clippy::panic
)]

use sim_core::{ActiveHazardZone, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder};

fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

fn create_test_world(size: u32) -> SimWorld {
    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(size, size, default_cell).expect("invariant: valid map");
    SimWorld::new(map, 0xBEEF_CAFE)
}

fn run_battle_until_resolution(
    world: &mut SimWorld,
    max_ticks: u32,
    p0: PlayerId,
    p1: PlayerId,
) -> (usize, usize) {
    for tick in 0..max_ticks {
        world.step(&empty_bundle(tick));
        let p0_alive = world
            .entities
            .values()
            .filter(|e| e.owner == p0 && e.is_alive())
            .count();
        let p1_alive = world
            .entities
            .values()
            .filter(|e| e.owner == p1 && e.is_alive())
            .count();
        if p0_alive == 0 || p1_alive == 0 {
            return (p0_alive, p1_alive);
        }
    }
    let p0_alive = world
        .entities
        .values()
        .filter(|e| e.owner == p0 && e.is_alive())
        .count();
    let p1_alive = world
        .entities
        .values()
        .filter(|e| e.owner == p1 && e.is_alive())
        .count();
    (p0_alive, p1_alive)
}

fn spawn_grid_squad(
    world: &mut SimWorld,
    unit_slug: &str,
    player: PlayerId,
    cols: usize,
    count: usize,
    start_x: f32,
    start_y: f32,
    spacing: f32,
    target_pos: SimVec2,
) -> Vec<EntityId> {
    let unit_id = world
        .catalog
        .id_by_slug(unit_slug)
        .unwrap_or_else(|| panic!("valid slug: {unit_slug}"));
    let mut ids = Vec::new();
    for i in 0..count {
        let col = i % cols;
        let row = i / cols;
        let x = start_x + (col as f32) * spacing;
        let y = start_y + (row as f32) * spacing;
        let pos = SimVec2::new(SimScalar::from_f32_lossy(x), SimScalar::from_f32_lossy(y));
        let id = world
            .spawn_unit(unit_id, player, pos)
            .expect("spawn grid unit");
        world
            .entities
            .get_mut(&id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: target_pos });
        ids.push(id);
    }
    ids
}

fn spawn_100_zerglings(
    world: &mut SimWorld,
    player: PlayerId,
    start_x: f32,
    start_y: f32,
    target_pos: SimVec2,
) {
    let zergling_id = world
        .catalog
        .id_by_slug("razor_mantis")
        .expect("razor_mantis");
    for row in 0..10 {
        for col in 0..10 {
            let x = start_x + (col as f32) * 0.7;
            let y = start_y + (row as f32) * 0.7;
            let pos = SimVec2::new(SimScalar::from_f32_lossy(x), SimScalar::from_f32_lossy(y));
            let id = world
                .spawn_unit(zergling_id, player, pos)
                .expect("spawn zergling");
            world
                .entities
                .get_mut(&id)
                .unwrap()
                .order_queue
                .push_back(UnitOrder::AttackMove { target: target_pos });
        }
    }
}

// ==============================================================================================
// SUITES 1–8: ICONIC GROUND DUELS
// ==============================================================================================

/// Suite 1A: 1 Stalker vs 1 Roach (Stalker Range 6.0 vs Roach Range 4.0).
/// Stalker wins 1v1 due to range advantage and +5 vs Armored.
#[test]
fn test_suite_1a_1_stalker_vs_1_roach_duel() {
    let mut world = create_test_world(64);
    let p_stalker = PlayerId(0);
    let p_roach = PlayerId(1);

    let stalker_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let roach_pos = SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(32));

    let stalker_type = world
        .catalog
        .id_by_slug("warp_strider")
        .expect("warp_strider");
    let roach_type = world
        .catalog
        .id_by_slug("rhino_beetle")
        .expect("rhino_beetle");

    let s_id = world
        .spawn_unit(stalker_type, p_stalker, stalker_pos)
        .expect("stalker");
    let r_id = world
        .spawn_unit(roach_type, p_roach, roach_pos)
        .expect("roach");

    world
        .entities
        .get_mut(&s_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: roach_pos });
    world
        .entities
        .get_mut(&r_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: stalker_pos,
        });

    let (s_alive, r_alive) = run_battle_until_resolution(&mut world, 400, p_stalker, p_roach);
    assert_eq!(s_alive, 1, "Stalker must survive 1v1 duel against Roach");
    assert_eq!(
        r_alive, 0,
        "Roach must be destroyed in 1v1 duel against Stalker"
    );
}

/// Suite 1B: 2 Roaches vs 1 Stalker (Equal 50 Gas Cost).
/// 2 Roaches focus fire and easily defeat the single Stalker.
#[test]
fn test_suite_1b_2_roaches_vs_1_stalker() {
    let mut world = create_test_world(64);
    let p_stalker = PlayerId(0);
    let p_roach = PlayerId(1);

    let stalker_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let stalker_type = world
        .catalog
        .id_by_slug("warp_strider")
        .expect("warp_strider");
    let s_id = world
        .spawn_unit(stalker_type, p_stalker, stalker_pos)
        .expect("stalker");
    world
        .entities
        .get_mut(&s_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(32)),
        });

    spawn_grid_squad(
        &mut world,
        "rhino_beetle",
        p_roach,
        1,
        2,
        16.0,
        31.5,
        1.0,
        stalker_pos,
    );

    let (s_alive, r_alive) = run_battle_until_resolution(&mut world, 300, p_stalker, p_roach);
    assert_eq!(s_alive, 0, "Stalker must die against 2 Roaches");
    assert_eq!(r_alive, 2, "Both Roaches must survive against 1 Stalker");
}

/// Suite 2A: 1 Immortal vs 1 Roach (Immortal 50 vs Armored 3-shots Roach).
#[test]
fn test_suite_2a_1_immortal_vs_1_roach() {
    let mut world = create_test_world(64);
    let p_immortal = PlayerId(0);
    let p_roach = PlayerId(1);

    let imm_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let roach_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));

    let imm_type = world
        .catalog
        .id_by_slug("void_goliath")
        .expect("void_goliath");
    let roach_type = world
        .catalog
        .id_by_slug("rhino_beetle")
        .expect("rhino_beetle");

    let imm_id = world
        .spawn_unit(imm_type, p_immortal, imm_pos)
        .expect("immortal");
    let r_id = world
        .spawn_unit(roach_type, p_roach, roach_pos)
        .expect("roach");

    world
        .entities
        .get_mut(&imm_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: roach_pos });
    world
        .entities
        .get_mut(&r_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: imm_pos });

    let (imm_alive, r_alive) = run_battle_until_resolution(&mut world, 200, p_immortal, p_roach);
    assert_eq!(imm_alive, 1, "Immortal must survive against 1 Roach");
    assert_eq!(r_alive, 0, "Roach must be destroyed in 3 shots by Immortal");

    let imm_ent = world.entities.get(&imm_id).unwrap();
    assert_eq!(
        imm_ent.hp,
        SimScalar::from_num(200),
        "Immortal HP should remain at full 200"
    );
}

/// Suite 2B: 1 Immortal vs 2 Roaches.
#[test]
fn test_suite_2b_1_immortal_vs_2_roaches() {
    let mut world = create_test_world(64);
    let p_immortal = PlayerId(0);
    let p_roach = PlayerId(1);

    let imm_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let imm_type = world
        .catalog
        .id_by_slug("void_goliath")
        .expect("void_goliath");
    let imm_id = world
        .spawn_unit(imm_type, p_immortal, imm_pos)
        .expect("immortal");
    world
        .entities
        .get_mut(&imm_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32)),
        });

    spawn_grid_squad(
        &mut world,
        "rhino_beetle",
        p_roach,
        1,
        2,
        15.0,
        31.5,
        1.0,
        imm_pos,
    );

    let (imm_alive, r_alive) = run_battle_until_resolution(&mut world, 250, p_immortal, p_roach);
    assert_eq!(imm_alive, 1, "Immortal must defeat 2 Roaches");
    assert_eq!(r_alive, 0, "Both Roaches must be destroyed");
}

/// Suite 3: 1 Immortal vs 1 Siege Tank (Tank Mode).
/// Immortal deals 49 net damage per shot vs Tank hull and destroys it in 4 shots.
#[test]
fn test_suite_3_immortal_vs_tank_mode() {
    let mut world = create_test_world(64);
    let p_immortal = PlayerId(0);
    let p_tank = PlayerId(1);

    let imm_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let tank_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));

    let imm_type = world
        .catalog
        .id_by_slug("void_goliath")
        .expect("void_goliath");
    let tank_type = world
        .catalog
        .id_by_slug("railgun_crawler")
        .expect("railgun_crawler");

    let imm_id = world
        .spawn_unit(imm_type, p_immortal, imm_pos)
        .expect("immortal");
    let t_id = world.spawn_unit(tank_type, p_tank, tank_pos).expect("tank");

    world
        .entities
        .get_mut(&imm_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: tank_pos });
    world
        .entities
        .get_mut(&t_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: imm_pos });

    let (imm_alive, t_alive) = run_battle_until_resolution(&mut world, 200, p_immortal, p_tank);
    assert_eq!(
        imm_alive, 1,
        "Immortal must destroy Siege Tank in Tank Mode"
    );
    assert_eq!(t_alive, 0, "Siege Tank must be destroyed");

    let imm_ent = world.entities.get(&imm_id).unwrap();
    assert_eq!(
        imm_ent.hp,
        SimScalar::from_num(200),
        "Immortal takes shield damage only"
    );
}

/// Suite 4: 1 Marauder vs 1 Roach (Heavy Armored Ground Duel).
/// Marauder deals 20 vs Armored (8 shots to kill Roach); Roach deals 16 (9 shots to kill Marauder).
#[test]
fn test_suite_4_1_marauder_vs_1_roach() {
    let mut world = create_test_world(64);
    let p_marauder = PlayerId(0);
    let p_roach = PlayerId(1);

    let marauder_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let roach_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));

    let marauder_type = world
        .catalog
        .id_by_slug("shock_golem")
        .expect("shock_golem");
    let roach_type = world
        .catalog
        .id_by_slug("rhino_beetle")
        .expect("rhino_beetle");

    let m_id = world
        .spawn_unit(marauder_type, p_marauder, marauder_pos)
        .expect("marauder");
    let r_id = world
        .spawn_unit(roach_type, p_roach, roach_pos)
        .expect("roach");

    world
        .entities
        .get_mut(&m_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: roach_pos });
    world
        .entities
        .get_mut(&r_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: marauder_pos,
        });

    let (m_alive, r_alive) = run_battle_until_resolution(&mut world, 300, p_marauder, p_roach);
    assert_eq!(m_alive, 1, "Marauder must defeat Roach in armored duel");
    assert_eq!(r_alive, 0, "Roach must be destroyed");
}

/// Suite 5: Hydralisk vs Marines (1v1, 1v2, 1v3).
#[test]
fn test_suite_5_hydralisk_vs_marines() {
    // 5A: 1 Hydra vs 1 Marine -> Hydra wins
    {
        let mut world = create_test_world(64);
        let p_hydra = PlayerId(0);
        let p_marine = PlayerId(1);

        let h_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let m_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));

        let h_type = world
            .catalog
            .id_by_slug("thorn_viper")
            .expect("thorn_viper");
        let m_type = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

        let h_id = world.spawn_unit(h_type, p_hydra, h_pos).expect("hydra");
        let m_id = world.spawn_unit(m_type, p_marine, m_pos).expect("marine");

        world
            .entities
            .get_mut(&h_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: m_pos });
        world
            .entities
            .get_mut(&m_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: h_pos });

        let (h_alive, m_alive) = run_battle_until_resolution(&mut world, 150, p_hydra, p_marine);
        assert_eq!(h_alive, 1, "1 Hydra must defeat 1 Marine");
        assert_eq!(m_alive, 0);
    }

    // 5B: 1 Hydra vs 2 Marines -> Hydra wins surviving with ~15..30 HP
    {
        let mut world = create_test_world(64);
        let p_hydra = PlayerId(0);
        let p_marine = PlayerId(1);

        let h_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let h_type = world
            .catalog
            .id_by_slug("thorn_viper")
            .expect("thorn_viper");
        let h_id = world.spawn_unit(h_type, p_hydra, h_pos).expect("hydra");
        world
            .entities
            .get_mut(&h_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "pulse_bot",
            p_marine,
            1,
            2,
            15.0,
            31.6,
            0.8,
            h_pos,
        );

        let (h_alive, m_alive) = run_battle_until_resolution(&mut world, 200, p_hydra, p_marine);
        assert_eq!(
            h_alive, 1,
            "1 Hydra must defeat 2 Marines in equal-mineral fight"
        );
        assert_eq!(m_alive, 0);
    }

    // 5C: 1 Hydra vs 3 Marines -> 3 Marines win
    {
        let mut world = create_test_world(64);
        let p_hydra = PlayerId(0);
        let p_marine = PlayerId(1);

        let h_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let h_type = world
            .catalog
            .id_by_slug("thorn_viper")
            .expect("thorn_viper");
        let h_id = world.spawn_unit(h_type, p_hydra, h_pos).expect("hydra");
        world
            .entities
            .get_mut(&h_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "pulse_bot",
            p_marine,
            1,
            3,
            15.0,
            31.2,
            0.8,
            h_pos,
        );

        let (h_alive, m_alive) = run_battle_until_resolution(&mut world, 200, p_hydra, p_marine);
        assert_eq!(h_alive, 0, "Hydra must die against 3 Marines");
        assert!(m_alive >= 1, "At least 1 Marine must survive");
    }
}

/// Suite 6: Roach vs Hydralisk (1v1, 2v1, 1v2).
#[test]
fn test_suite_6_roach_vs_hydralisk() {
    // 6A: 1 Roach vs 1 Hydra -> Roach wins with ~20..35 HP
    {
        let mut world = create_test_world(64);
        let p_roach = PlayerId(0);
        let p_hydra = PlayerId(1);

        let r_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let h_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));

        let r_type = world
            .catalog
            .id_by_slug("rhino_beetle")
            .expect("rhino_beetle");
        let h_type = world
            .catalog
            .id_by_slug("thorn_viper")
            .expect("thorn_viper");

        let r_id = world.spawn_unit(r_type, p_roach, r_pos).expect("roach");
        let h_id = world.spawn_unit(h_type, p_hydra, h_pos).expect("hydra");

        world
            .entities
            .get_mut(&r_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: h_pos });
        world
            .entities
            .get_mut(&h_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: r_pos });

        let (r_alive, h_alive) = run_battle_until_resolution(&mut world, 250, p_roach, p_hydra);
        assert_eq!(r_alive, 1, "Roach must defeat Hydralisk in 1v1 duel");
        assert_eq!(h_alive, 0);
    }

    // 6B: 2 Roaches vs 1 Hydra -> Roaches win
    {
        let mut world = create_test_world(64);
        let p_roach = PlayerId(0);
        let p_hydra = PlayerId(1);

        let h_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));
        let h_type = world
            .catalog
            .id_by_slug("thorn_viper")
            .expect("thorn_viper");
        let h_id = world.spawn_unit(h_type, p_hydra, h_pos).expect("hydra");
        world
            .entities
            .get_mut(&h_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "rhino_beetle",
            p_roach,
            1,
            2,
            10.0,
            31.5,
            1.0,
            h_pos,
        );

        let (r_alive, h_alive) = run_battle_until_resolution(&mut world, 200, p_roach, p_hydra);
        assert_eq!(r_alive, 2, "Both Roaches must survive against 1 Hydra");
        assert_eq!(h_alive, 0);
    }

    // 6C: 1 Roach vs 2 Hydras -> 2 Hydras win
    {
        let mut world = create_test_world(64);
        let p_roach = PlayerId(0);
        let p_hydra = PlayerId(1);

        let r_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let r_type = world
            .catalog
            .id_by_slug("rhino_beetle")
            .expect("rhino_beetle");
        let r_id = world.spawn_unit(r_type, p_roach, r_pos).expect("roach");
        world
            .entities
            .get_mut(&r_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "thorn_viper",
            p_hydra,
            1,
            2,
            15.0,
            31.5,
            1.0,
            r_pos,
        );

        let (r_alive, h_alive) = run_battle_until_resolution(&mut world, 200, p_roach, p_hydra);
        assert_eq!(r_alive, 0, "Roach must die against 2 Hydras");
        assert!(h_alive >= 1, "At least 1 Hydra must survive");
    }
}

/// Suite 7: 1 Zealot vs 1 Roach (Un-kited Melee Duel).
/// Zealot deals 14 damage per swing (11 swings to kill Roach). Zealot wins with ~15..35 HP.
#[test]
fn test_suite_7_1_zealot_vs_1_roach_un_kited() {
    let mut world = create_test_world(64);
    let p_zealot = PlayerId(0);
    let p_roach = PlayerId(1);

    let z_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let r_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));

    let z_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
    let r_type = world
        .catalog
        .id_by_slug("rhino_beetle")
        .expect("rhino_beetle");

    let z_id = world.spawn_unit(z_type, p_zealot, z_pos).expect("zealot");
    let r_id = world.spawn_unit(r_type, p_roach, r_pos).expect("roach");

    world
        .entities
        .get_mut(&z_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: r_pos });
    world
        .entities
        .get_mut(&r_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: z_pos });

    let (z_alive, r_alive) = run_battle_until_resolution(&mut world, 300, p_zealot, p_roach);
    assert_eq!(
        z_alive, 1,
        "Zealot must defeat Roach in un-kited engagement"
    );
    assert_eq!(r_alive, 0, "Roach must be destroyed");
}

/// Suite 8: 1 Zealot vs 1 Stalker (Light vs Ranged Duel).
/// In un-kited A-move, Zealot closes the gap and destroys Stalker, surviving with ~30..50 HP.
#[test]
fn test_suite_8_1_zealot_vs_1_stalker_un_kited() {
    let mut world = create_test_world(64);
    let p_zealot = PlayerId(0);
    let p_stalker = PlayerId(1);

    let z_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let s_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));

    let z_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
    let s_type = world
        .catalog
        .id_by_slug("warp_strider")
        .expect("warp_strider");

    let z_id = world.spawn_unit(z_type, p_zealot, z_pos).expect("zealot");
    let s_id = world.spawn_unit(s_type, p_stalker, s_pos).expect("stalker");

    world
        .entities
        .get_mut(&z_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: s_pos });
    world
        .entities
        .get_mut(&s_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: z_pos });

    let (z_alive, s_alive) = run_battle_until_resolution(&mut world, 300, p_zealot, p_stalker);
    assert_eq!(z_alive, 1, "Zealot must defeat Stalker in un-kited battle");
    assert_eq!(s_alive, 0, "Stalker must be destroyed");
}

// ==============================================================================================
// SUITES 9–15: SPLASH, ARTILLERY & HEAVYWEIGHT BENCHMARKS
// ==============================================================================================

/// Suite 9A: 1 Hellion vs 4 Zerglings (14 Flame Splash vs Light).
/// Hellion 3-shots all 4 clumped Zerglings with splash.
#[test]
fn test_suite_9a_1_hellion_vs_4_zerglings() {
    let mut world = create_test_world(64);
    let p_hellion = PlayerId(0);
    let p_lings = PlayerId(1);

    let h_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let h_type = world
        .catalog
        .id_by_slug("napalm_wheeler")
        .expect("napalm_wheeler");
    let h_id = world.spawn_unit(h_type, p_hellion, h_pos).expect("hellion");
    world
        .entities
        .get_mut(&h_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(32)),
        });

    spawn_grid_squad(
        &mut world,
        "razor_mantis",
        p_lings,
        2,
        4,
        16.0,
        31.8,
        0.4,
        h_pos,
    );

    let (h_alive, l_alive) = run_battle_until_resolution(&mut world, 300, p_hellion, p_lings);
    assert_eq!(
        h_alive, 1,
        "Hellion must defeat 4 Zerglings using flame splash"
    );
    assert_eq!(l_alive, 0, "All 4 Zerglings must be destroyed");
}

/// Suite 9B: 1 Hellion vs 1 Roach (Flame vs Armored).
/// Hellion deals minimal damage to Armored Roach; Roach destroys Hellion.
#[test]
fn test_suite_9b_1_hellion_vs_1_roach() {
    let mut world = create_test_world(64);
    let p_hellion = PlayerId(0);
    let p_roach = PlayerId(1);

    let h_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let r_pos = SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(32));

    let h_type = world
        .catalog
        .id_by_slug("napalm_wheeler")
        .expect("napalm_wheeler");
    let r_type = world
        .catalog
        .id_by_slug("rhino_beetle")
        .expect("rhino_beetle");

    let h_id = world.spawn_unit(h_type, p_hellion, h_pos).expect("hellion");
    let r_id = world.spawn_unit(r_type, p_roach, r_pos).expect("roach");

    world
        .entities
        .get_mut(&h_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: r_pos });
    world
        .entities
        .get_mut(&r_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: h_pos });

    let (h_alive, r_alive) = run_battle_until_resolution(&mut world, 300, p_hellion, p_roach);
    assert_eq!(h_alive, 0, "Hellion must die against Armored Roach");
    assert_eq!(r_alive, 1, "Roach must destroy Hellion");
}

/// Suite 10: 2 Hellions vs 8 Zerglings.
/// Concentrated flame splash roasts advancing Zergling pack.
#[test]
fn test_suite_10_hellions_flame_splash_vs_zerglings() {
    let mut world = create_test_world(64);
    let p_hellion = PlayerId(0);
    let p_lings = PlayerId(1);

    spawn_grid_squad(
        &mut world,
        "napalm_wheeler",
        p_hellion,
        1,
        2,
        10.0,
        31.5,
        1.0,
        SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(32)),
    );

    spawn_grid_squad(
        &mut world,
        "razor_mantis",
        p_lings,
        2,
        8,
        15.0,
        31.0,
        0.6,
        SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32)),
    );

    let (h_alive, l_alive) = run_battle_until_resolution(&mut world, 250, p_hellion, p_lings);
    assert!(
        h_alive >= 1,
        "Hellions must defeat Zerglings with flame splash"
    );
    assert_eq!(l_alive, 0, "All Zerglings must be destroyed");
}

/// Suite 11A: 2 Banelings Detonate on 5 Clumped Marines (Light Infantry).
/// Banelings deal 35 splash vs Light; both Banelings detonate, wiping out all 5 Marines.
#[test]
fn test_suite_11a_2_banelings_wipe_5_marines() {
    let mut world = create_test_world(64);
    let p_banelings = PlayerId(0);
    let p_marines = PlayerId(1);

    let b1 = world
        .catalog
        .id_by_slug("volatile_tick")
        .expect("volatile_tick");
    let m1 = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // 2 Banelings at (12, 32)
    let b_id1 = world
        .spawn_unit(
            b1,
            p_banelings,
            SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(31.8)),
        )
        .unwrap();
    let b_id2 = world
        .spawn_unit(
            b1,
            p_banelings,
            SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(32.2)),
        )
        .unwrap();

    // 5 Marines tightly clumped at (14, 32)
    for i in 0..5 {
        let m_pos = SimVec2::new(
            SimScalar::from_num(14),
            SimScalar::from_f32_lossy(31.6 + (i as f32) * 0.2),
        );
        let mid = world.spawn_unit(m1, p_marines, m_pos).unwrap();
        world
            .entities
            .get_mut(&mid)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::HoldPosition);
    }

    world
        .entities
        .get_mut(&b_id1)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(32)),
        });
    world
        .entities
        .get_mut(&b_id2)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(32)),
        });

    let (b_alive, m_alive) = run_battle_until_resolution(&mut world, 100, p_banelings, p_marines);
    assert_eq!(
        b_alive, 0,
        "Both Banelings must detonate and die (suicide_on_attack: true)"
    );
    assert_eq!(
        m_alive, 0,
        "All 5 Marines must be wiped out by 2 Baneling detonations"
    );
}

/// Suite 11B: 2 Banelings vs 1 Armored Marauder.
/// Baneling deals only 15 damage per blast vs Armored. Both Banelings die; Marauder survives with ~95 HP!
#[test]
fn test_suite_11b_2_banelings_vs_armored_marauder() {
    let mut world = create_test_world(64);
    let p_banelings = PlayerId(0);
    let p_marauder = PlayerId(1);

    let b_type = world
        .catalog
        .id_by_slug("volatile_tick")
        .expect("volatile_tick");
    let m_type = world
        .catalog
        .id_by_slug("shock_golem")
        .expect("shock_golem");

    let b_id1 = world
        .spawn_unit(
            b_type,
            p_banelings,
            SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(31.8)),
        )
        .unwrap();
    let b_id2 = world
        .spawn_unit(
            b_type,
            p_banelings,
            SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(32.2)),
        )
        .unwrap();

    let m_id = world
        .spawn_unit(
            m_type,
            p_marauder,
            SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(32)),
        )
        .unwrap();
    world
        .entities
        .get_mut(&m_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::HoldPosition);

    world
        .entities
        .get_mut(&b_id1)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(32)),
        });
    world
        .entities
        .get_mut(&b_id2)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(32)),
        });

    let (b_alive, m_alive) = run_battle_until_resolution(&mut world, 100, p_banelings, p_marauder);
    assert_eq!(b_alive, 0, "Both Banelings must detonate and die");
    assert_eq!(
        m_alive, 1,
        "Armored Marauder must survive 2 Baneling blasts"
    );

    let marauder_hp = world.entities.get(&m_id).unwrap().hp.to_f32();
    assert!(
        (85.0..=105.0).contains(&marauder_hp),
        "Marauder should have ~95 HP remaining, got {marauder_hp}"
    );
}

/// Suite 12: Colossus vs Swarm vs Marauders vs Vikings.
#[test]
fn test_suite_12_colossus_matchups() {
    // 12A: 1 Colossus sweeps 6 Marines with thermal lance splash
    {
        let mut world = create_test_world(64);
        let p_col = PlayerId(0);
        let p_marines = PlayerId(1);

        let col_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let col_type = world
            .catalog
            .id_by_slug("tripod_annihilator")
            .expect("tripod_annihilator");
        let col_id = world
            .spawn_unit(col_type, p_col, col_pos)
            .expect("colossus");
        world
            .entities
            .get_mut(&col_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "pulse_bot",
            p_marines,
            2,
            6,
            16.0,
            31.4,
            0.6,
            col_pos,
        );

        let (c_alive, m_alive) = run_battle_until_resolution(&mut world, 350, p_col, p_marines);
        assert_eq!(
            c_alive, 1,
            "Colossus must vaporize Marines with thermal lance splash"
        );
        assert_eq!(m_alive, 0);
    }

    // 12B: 1 Colossus vs 3 Marauders -> Marauders deal 60 vs Armored and kill Colossus
    {
        let mut world = create_test_world(64);
        let p_col = PlayerId(0);
        let p_marauders = PlayerId(1);

        let col_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let col_type = world
            .catalog
            .id_by_slug("tripod_annihilator")
            .expect("tripod_annihilator");
        let col_id = world
            .spawn_unit(col_type, p_col, col_pos)
            .expect("colossus");
        world
            .entities
            .get_mut(&col_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "shock_golem",
            p_marauders,
            1,
            3,
            15.0,
            31.0,
            1.0,
            col_pos,
        );

        let (c_alive, m_alive) = run_battle_until_resolution(&mut world, 300, p_col, p_marauders);
        assert_eq!(
            c_alive, 0,
            "Colossus must fall to 3 Armored-killer Marauders"
        );
        assert!(m_alive >= 1, "Marauders must survive");
    }

    // 12C: 1 Viking shoots down Colossus (CliffWalker) from Range 9.0 taking 0 damage
    {
        let mut world = create_test_world(64);
        let p_viking = PlayerId(0);
        let p_col = PlayerId(1);

        let vik_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let col_pos = SimVec2::new(SimScalar::from_num(18), SimScalar::from_num(32));

        let vik_type = world
            .catalog
            .id_by_slug("vector_interceptor")
            .expect("vector_interceptor");
        let col_type = world
            .catalog
            .id_by_slug("tripod_annihilator")
            .expect("tripod_annihilator");

        let v_id = world
            .spawn_unit(vik_type, p_viking, vik_pos)
            .expect("viking");
        let c_id = world
            .spawn_unit(col_type, p_col, col_pos)
            .expect("colossus");

        world
            .entities
            .get_mut(&v_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::attack(c_id));
        world
            .entities
            .get_mut(&c_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::HoldPosition);

        let (v_alive, c_alive) = run_battle_until_resolution(&mut world, 1000, p_viking, p_col);
        assert_eq!(
            v_alive, 1,
            "Viking must shoot down towering CliffWalker Colossus"
        );
        assert_eq!(c_alive, 0, "Colossus must be destroyed");

        let vik_ent = world.entities.get(&v_id).unwrap();
        assert_eq!(
            vik_ent.hp,
            SimScalar::from_num(125),
            "Viking takes 0 damage"
        );
    }
}

/// Suite 13: Psionic Storm AoE vs Clumped Infantry.
#[test]
fn test_suite_13_psionic_storm_hazard_wipes_swarm() {
    let mut world = create_test_world(64);
    let p_caster = PlayerId(0);
    let p_swarm = PlayerId(1);

    let storm_center = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));

    // Spawn 8 Zerglings tightly held inside storm zone
    for i in 0..8 {
        let x = 14.6 + (i as f32 % 3.0) * 0.4;
        let y = 31.6 + (i as f32 / 3.0) * 0.4;
        let z_id = world
            .spawn_unit(
                world.catalog.id_by_slug("razor_mantis").unwrap(),
                p_swarm,
                SimVec2::new(SimScalar::from_f32_lossy(x), SimScalar::from_f32_lossy(y)),
            )
            .unwrap();
        world
            .entities
            .get_mut(&z_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::HoldPosition);
    }

    world.active_hazards.push(ActiveHazardZone {
        id: 1,
        owner: p_caster,
        center: storm_center,
        radius: SimScalar::from_num(3),
        delay_ticks: 0,
        remaining_ticks: 60,
        interval_ticks: 10,
        ticks_until_next_interval: 1,
        damage_per_interval: SimScalar::from_num(20),
        targets_friendly: false,
    });

    for t in 0..70 {
        world.step(&empty_bundle(t));
    }
    let s_alive = world
        .entities
        .values()
        .filter(|e| e.owner == p_swarm && e.is_alive())
        .count();
    assert_eq!(
        s_alive, 0,
        "All 8 Zerglings held in storm must be wiped out"
    );
}

/// Suite 14: 1 Thor vs 1 Ultralisk (Heavyweight Duel).
/// Thor (400 HP, Range 7.0, 60 dmg/cycle) shoots approaching Ultralisk and wins with ~80..130 HP left.
#[test]
fn test_suite_14_thor_vs_ultralisk_heavyweight_duel() {
    let mut world = create_test_world(64);
    let p_thor = PlayerId(0);
    let p_ultra = PlayerId(1);

    let thor_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let ultra_pos = SimVec2::new(SimScalar::from_num(18), SimScalar::from_num(32));

    let thor_type = world
        .catalog
        .id_by_slug("colossus_mech")
        .expect("colossus_mech");
    let ultra_type = world
        .catalog
        .id_by_slug("stag_behemoth")
        .expect("stag_behemoth");

    let t_id = world.spawn_unit(thor_type, p_thor, thor_pos).expect("thor");
    let u_id = world
        .spawn_unit(ultra_type, p_ultra, ultra_pos)
        .expect("ultralisk");

    world
        .entities
        .get_mut(&t_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: ultra_pos });
    world
        .entities
        .get_mut(&u_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: thor_pos });

    let (t_alive, u_alive) = run_battle_until_resolution(&mut world, 400, p_thor, p_ultra);
    assert_eq!(t_alive, 1, "Thor must defeat un-upgraded Ultralisk");
    assert_eq!(u_alive, 0, "Ultralisk must be destroyed");
}

/// Suite 15: Ultralisk with Chitinous Plating (+2 Armor -> 4 Armor) vs Marines.
#[test]
fn test_suite_15_ultralisk_chitinous_plating_vs_marines() {
    let mut world = create_test_world(64);
    let p_ultra = PlayerId(0);
    let p_marines = PlayerId(1);

    // Apply chitinous plating upgrade to Player 0
    world
        .research_upgrade(p_ultra, "chitinous_plating")
        .expect("chitinous_plating");

    let ultra_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let ultra_type = world
        .catalog
        .id_by_slug("stag_behemoth")
        .expect("stag_behemoth");
    let u_id = world
        .spawn_unit(ultra_type, p_ultra, ultra_pos)
        .expect("ultralisk");
    world
        .entities
        .get_mut(&u_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32)),
        });

    spawn_grid_squad(
        &mut world,
        "pulse_bot",
        p_marines,
        2,
        8,
        15.0,
        31.0,
        0.6,
        ultra_pos,
    );

    let (u_alive, m_alive) = run_battle_until_resolution(&mut world, 300, p_ultra, p_marines);
    assert_eq!(
        u_alive, 1,
        "Plated Ultralisk must slaughter Marines taking minimal damage"
    );
    assert_eq!(m_alive, 0);

    let u_hp = world.entities.get(&u_id).unwrap().hp.to_f32();
    assert!(
        u_hp > 350.0,
        "Ultralisk should retain >350 HP due to 4 Armor, had {u_hp}"
    );
}

// ==============================================================================================
// SUITES 16–22: AIR, SPINES & HARASSMENT
// ==============================================================================================

/// Suite 16: Mutalisk vs Marines (1v1 & 1v2).
#[test]
fn test_suite_16_mutalisk_vs_marines() {
    // 16A: 1 Mutalisk vs 1 Marine -> Mutalisk wins
    {
        let mut world = create_test_world(64);
        let p_muta = PlayerId(0);
        let p_marine = PlayerId(1);

        let muta_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let mar_pos = SimVec2::new(SimScalar::from_num(13), SimScalar::from_num(32));

        let muta_type = world.catalog.id_by_slug("sonic_bat").expect("sonic_bat");
        let mar_type = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

        let mid = world.spawn_unit(muta_type, p_muta, muta_pos).expect("muta");
        let mar_id = world
            .spawn_unit(mar_type, p_marine, mar_pos)
            .expect("marine");

        world
            .entities
            .get_mut(&mid)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: mar_pos });
        world
            .entities
            .get_mut(&mar_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: muta_pos });

        let (muta_alive, mar_alive) =
            run_battle_until_resolution(&mut world, 200, p_muta, p_marine);
        assert_eq!(muta_alive, 1, "1 Mutalisk must defeat 1 Marine");
        assert_eq!(mar_alive, 0);
    }

    // 16B: 1 Mutalisk vs 2 Marines -> 2 Marines win
    {
        let mut world = create_test_world(64);
        let p_muta = PlayerId(0);
        let p_marine = PlayerId(1);

        let muta_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let muta_type = world.catalog.id_by_slug("sonic_bat").expect("sonic_bat");
        let mid = world.spawn_unit(muta_type, p_muta, muta_pos).expect("muta");
        world
            .entities
            .get_mut(&mid)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(13), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "pulse_bot",
            p_marine,
            1,
            2,
            13.0,
            31.6,
            0.8,
            muta_pos,
        );

        let (muta_alive, mar_alive) =
            run_battle_until_resolution(&mut world, 250, p_muta, p_marine);
        assert_eq!(muta_alive, 0, "Mutalisk must fall to 2 Marines");
        assert!(mar_alive >= 1, "At least 1 Marine must survive");
    }
}

/// Suite 17: Void Ray vs Mutalisk (Light Air Duel).
#[test]
fn test_suite_17_void_ray_vs_mutalisk() {
    let mut world = create_test_world(64);
    let p_ray = PlayerId(0);
    let p_muta = PlayerId(1);

    let ray_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let muta_pos = SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(32));

    let ray_type = world
        .catalog
        .id_by_slug("prismatic_saucer")
        .expect("prismatic_saucer");
    let muta_type = world.catalog.id_by_slug("sonic_bat").expect("sonic_bat");

    let r_id = world
        .spawn_unit(ray_type, p_ray, ray_pos)
        .expect("void ray");
    let m_id = world.spawn_unit(muta_type, p_muta, muta_pos).expect("muta");

    world
        .entities
        .get_mut(&r_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: muta_pos });
    world
        .entities
        .get_mut(&m_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: ray_pos });

    let (r_alive, m_alive) = run_battle_until_resolution(&mut world, 250, p_ray, p_muta);
    assert_eq!(r_alive, 1, "Void Ray must defeat Mutalisk");
    assert_eq!(m_alive, 0);
}

/// Suite 18: Void Ray vs Corruptor (Armored-Melt Laser).
#[test]
fn test_suite_18_void_ray_vs_corruptor() {
    let mut world = create_test_world(64);
    let p_ray = PlayerId(0);
    let p_corruptor = PlayerId(1);

    let ray_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let corr_pos = SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(32));

    let ray_type = world
        .catalog
        .id_by_slug("prismatic_saucer")
        .expect("prismatic_saucer");
    let corr_type = world
        .catalog
        .id_by_slug("sky_leviathan")
        .expect("sky_leviathan");

    let r_id = world
        .spawn_unit(ray_type, p_ray, ray_pos)
        .expect("void ray");
    let c_id = world
        .spawn_unit(corr_type, p_corruptor, corr_pos)
        .expect("corruptor");

    world
        .entities
        .get_mut(&r_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: corr_pos });
    world
        .entities
        .get_mut(&c_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: ray_pos });

    let (r_alive, c_alive) = run_battle_until_resolution(&mut world, 300, p_ray, p_corruptor);
    assert_eq!(
        r_alive, 1,
        "Void Ray with bonus vs Armored must melt Corruptor"
    );
    assert_eq!(c_alive, 0);
}

/// Suite 19: Viking vs Brood Lord (Air Superiority vs Flying Siege).
#[test]
fn test_suite_19_viking_vs_brood_lord() {
    let mut world = create_test_world(64);
    let p_vik = PlayerId(0);
    let p_bl = PlayerId(1);

    let vik_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let bl_pos = SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(32));

    let vik_type = world
        .catalog
        .id_by_slug("vector_interceptor")
        .expect("vector_interceptor");
    let bl_type = world
        .catalog
        .id_by_slug("spore_matriarch")
        .expect("spore_matriarch");

    let v_id = world.spawn_unit(vik_type, p_vik, vik_pos).expect("viking");
    let b_id = world.spawn_unit(bl_type, p_bl, bl_pos).expect("brood lord");

    world
        .entities
        .get_mut(&v_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove { target: bl_pos });
    world
        .entities
        .get_mut(&b_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::HoldPosition);

    let (v_alive, b_alive) = run_battle_until_resolution(&mut world, 700, p_vik, p_bl);
    assert_eq!(
        v_alive, 1,
        "Viking must shoot down Brood Lord taking 0 damage"
    );
    assert_eq!(b_alive, 0);

    let v_hp = world.entities.get(&v_id).unwrap().hp;
    assert_eq!(v_hp, SimScalar::from_num(125), "Viking takes zero damage");
}

/// Suite 20: Brood Lord Dominance vs Ground-Only Units.
#[test]
fn test_suite_20_brood_lord_vs_ground_only() {
    let mut world = create_test_world(64);
    let p_bl = PlayerId(0);
    let p_ground = PlayerId(1);

    let bl_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let roach_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));

    let bl_type = world
        .catalog
        .id_by_slug("spore_matriarch")
        .expect("spore_matriarch");
    let roach_type = world
        .catalog
        .id_by_slug("rhino_beetle")
        .expect("rhino_beetle");

    let b_id = world.spawn_unit(bl_type, p_bl, bl_pos).expect("brood lord");
    let r_id = world
        .spawn_unit(roach_type, p_ground, roach_pos)
        .expect("roach");

    world
        .entities
        .get_mut(&b_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::attack(r_id));
    world
        .entities
        .get_mut(&r_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::HoldPosition);

    let (b_alive, r_alive) = run_battle_until_resolution(&mut world, 400, p_bl, p_ground);
    assert_eq!(
        b_alive, 1,
        "Brood Lord destroys ground-only Roach taking 0 damage"
    );
    assert_eq!(r_alive, 0);
}

/// Suite 21: Lurker Subterranean Spines vs Ground Squad.
#[test]
fn test_suite_21_lurker_spines_vs_ground() {
    let mut world = create_test_world(64);
    let p_lurker = PlayerId(0);
    let p_marines = PlayerId(1);

    let lurker_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
    let lurker_type = world
        .catalog
        .id_by_slug("burrow_scorpion")
        .expect("burrow_scorpion");
    let l_id = world
        .spawn_unit(lurker_type, p_lurker, lurker_pos)
        .expect("lurker");
    world
        .entities
        .get_mut(&l_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackMove {
            target: SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(32)),
        });

    spawn_grid_squad(
        &mut world,
        "pulse_bot",
        p_marines,
        2,
        6,
        16.0,
        31.4,
        0.6,
        lurker_pos,
    );

    let (l_alive, m_alive) = run_battle_until_resolution(&mut world, 250, p_lurker, p_marines);
    assert_eq!(
        l_alive, 1,
        "Lurker spines must decimate light ground infantry"
    );
    assert_eq!(m_alive, 0);
}

/// Suite 22: Reaper vs Workers vs Stalker.
#[test]
fn test_suite_22_reaper_vs_workers_vs_stalker() {
    // 22A: 1 Reaper vs 2 Workers -> Reaper bonus vs Light destroys workers
    {
        let mut world = create_test_world(64);
        let p_reaper = PlayerId(0);
        let p_worker = PlayerId(1);

        let reap_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let reap_type = world.catalog.id_by_slug("jet_jumper").expect("jet_jumper");
        let r_id = world
            .spawn_unit(reap_type, p_reaper, reap_pos)
            .expect("reaper");
        world
            .entities
            .get_mut(&r_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "welder_drone",
            p_worker,
            1,
            2,
            14.0,
            31.5,
            1.0,
            reap_pos,
        );

        let (r_alive, w_alive) = run_battle_until_resolution(&mut world, 200, p_reaper, p_worker);
        assert_eq!(r_alive, 1, "Reaper must defeat 2 workers");
        assert_eq!(w_alive, 0);
    }

    // 22B: 1 Reaper vs 1 Stalker -> Stalker destroys Reaper
    {
        let mut world = create_test_world(64);
        let p_reaper = PlayerId(0);
        let p_stalker = PlayerId(1);

        let reap_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let stalk_pos = SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(32));

        let reap_type = world.catalog.id_by_slug("jet_jumper").expect("jet_jumper");
        let stalk_type = world
            .catalog
            .id_by_slug("warp_strider")
            .expect("warp_strider");

        let r_id = world
            .spawn_unit(reap_type, p_reaper, reap_pos)
            .expect("reaper");
        let s_id = world
            .spawn_unit(stalk_type, p_stalker, stalk_pos)
            .expect("stalker");

        world
            .entities
            .get_mut(&r_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: stalk_pos });
        world
            .entities
            .get_mut(&s_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: reap_pos });

        let (r_alive, s_alive) = run_battle_until_resolution(&mut world, 200, p_reaper, p_stalker);
        assert_eq!(r_alive, 0, "Reaper must die against Armored Stalker");
        assert_eq!(s_alive, 1, "Stalker must survive");
    }
}

// ==============================================================================================
// SUITES 23–25: THE THREE FAMOUS BREAKPOINT DUELS
// ==============================================================================================

/// Suite 23: Breakpoint #1 — +1 Ground Weapons Zealot vs 6 Zerglings.
/// At 0/0: Zealot deals 16 (takes 3 swings to kill 35-HP Zergling); 6 Zerglings win.
/// At +1 Attack: Zealot deals 18 (2-shots Zerglings); Zealot wins!
#[test]
fn test_suite_23_breakpoint_1_zealot_vs_6_zerglings() {
    // 23A: 0/0 Zealot vs 6 Zerglings -> Zerglings win
    {
        let mut world = create_test_world(64);
        let p_zealot = PlayerId(0);
        let p_lings = PlayerId(1);

        let z_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let z_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
        let z_id = world.spawn_unit(z_type, p_zealot, z_pos).expect("zealot");
        world
            .entities
            .get_mut(&z_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(13), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "razor_mantis",
            p_lings,
            6,
            6,
            15.0,
            32.0,
            7.0,
            z_pos,
        );

        let (z_alive, l_alive) = run_battle_until_resolution(&mut world, 600, p_zealot, p_lings);
        assert_eq!(z_alive, 0, "0/0 Zealot must be defeated by 6 Zerglings");
        assert!(l_alive >= 1, "Zerglings must survive");
    }

    // 23B: +1 Attack Zealot vs 6 Zerglings -> Zealot wins!
    {
        let mut world = create_test_world(64);
        let p_zealot = PlayerId(0);
        let p_lings = PlayerId(1);

        // Research +1 Ground Weapons for Protoss (Player 0)
        world
            .research_upgrade(p_zealot, "ground_weapons_1")
            .expect("ground_weapons_1");

        let z_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let z_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
        let z_id = world.spawn_unit(z_type, p_zealot, z_pos).expect("zealot");
        world
            .entities
            .get_mut(&z_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(13), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "razor_mantis",
            p_lings,
            6,
            6,
            15.0,
            32.0,
            7.0,
            z_pos,
        );

        let (z_alive, l_alive) = run_battle_until_resolution(&mut world, 600, p_zealot, p_lings);
        assert_eq!(
            z_alive, 1,
            "+1 Attack Zealot must 2-shot and defeat all 6 Zerglings"
        );
        assert_eq!(l_alive, 0, "All 6 Zerglings must be destroyed");
    }
}

/// Suite 24: Breakpoint #2 — +1 Missile Weapons Roach vs 5 Zerglings.
/// At 0/0: Roach deals 16 (takes 3 shots to kill 35-HP Zergling); 5 Zerglings win.
/// At +1 Missile: Roach deals 18 (2-shots Zerglings); Roach wins!
#[test]
fn test_suite_24_breakpoint_2_roach_vs_5_zerglings() {
    // 24A: 0/0 Roach vs 5 Zerglings -> Zerglings win
    {
        let mut world = create_test_world(64);
        let p_roach = PlayerId(0);
        let p_lings = PlayerId(1);

        let r_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let r_type = world
            .catalog
            .id_by_slug("rhino_beetle")
            .expect("rhino_beetle");
        let r_id = world.spawn_unit(r_type, p_roach, r_pos).expect("roach");
        world
            .entities
            .get_mut(&r_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(13), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "razor_mantis",
            p_lings,
            5,
            5,
            15.0,
            32.0,
            7.5,
            r_pos,
        );

        let (r_alive, l_alive) = run_battle_until_resolution(&mut world, 600, p_roach, p_lings);
        assert_eq!(r_alive, 0, "0/0 Roach must fall to 5 Zerglings");
        assert!(l_alive >= 1, "Zerglings must survive");
    }

    // 24B: +1 Missile Roach vs 5 Zerglings -> Roach wins!
    {
        let mut world = create_test_world(64);
        let p_roach = PlayerId(0);
        let p_lings = PlayerId(1);

        // Research +1 Missile Attacks for Zerg (Player 0)
        world
            .research_upgrade(p_roach, "missile_attacks_1")
            .expect("missile_attacks_1");

        let r_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32));
        let r_type = world
            .catalog
            .id_by_slug("rhino_beetle")
            .expect("rhino_beetle");
        let r_id = world.spawn_unit(r_type, p_roach, r_pos).expect("roach");
        world
            .entities
            .get_mut(&r_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(13), SimScalar::from_num(32)),
            });

        spawn_grid_squad(
            &mut world,
            "razor_mantis",
            p_lings,
            5,
            5,
            15.0,
            32.0,
            7.5,
            r_pos,
        );

        let (r_alive, l_alive) = run_battle_until_resolution(&mut world, 600, p_roach, p_lings);
        assert_eq!(
            r_alive, 1,
            "+1 Missile Roach must 2-shot and defeat 5 Zerglings"
        );
        assert_eq!(l_alive, 0, "All 5 Zerglings must be destroyed");
    }
}

/// Suite 25: Breakpoint #3 — Combat Shield + Overclock Marines vs Zealot.
#[test]
fn test_suite_25_breakpoint_3_combat_shield_marines_vs_zealot() {
    // 25A: 2 Marines (45 HP, base) vs 1 Zealot -> Zealot wins with ~70 HP
    {
        let mut world = create_test_world(64);
        let p_marines = PlayerId(0);
        let p_zealot = PlayerId(1);

        let z_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));
        let z_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
        let z_id = world.spawn_unit(z_type, p_zealot, z_pos).expect("zealot");
        world
            .entities
            .get_mut(&z_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(9), SimScalar::from_num(32)),
            });

        let m_type = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");
        let m1 = world
            .spawn_unit(
                m_type,
                p_marines,
                SimVec2::new(SimScalar::from_num(9), SimScalar::from_f32_lossy(30.2)),
            )
            .unwrap();
        let m2 = world
            .spawn_unit(
                m_type,
                p_marines,
                SimVec2::new(SimScalar::from_num(9), SimScalar::from_f32_lossy(33.8)),
            )
            .unwrap();
        world
            .entities
            .get_mut(&m1)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: z_pos });
        world
            .entities
            .get_mut(&m2)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: z_pos });

        let (m_alive, z_alive) = run_battle_until_resolution(&mut world, 350, p_marines, p_zealot);
        assert_eq!(m_alive, 0, "Base Marines must fall to 1 Zealot");
        assert_eq!(z_alive, 1, "Zealot must survive");
    }

    // 25B: 2 Marines with Combat Shield (55 HP) and Overclock (+50% attack rate) -> Marines win!
    {
        let mut world = create_test_world(64);
        let p_marines = PlayerId(0);
        let p_zealot = PlayerId(1);

        // Research combat shield for Player 0 (boosts Marine HP to 55)
        world
            .research_upgrade(p_marines, "combat_shield")
            .expect("combat_shield");

        let z_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32));
        let z_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
        let z_id = world.spawn_unit(z_type, p_zealot, z_pos).expect("zealot");
        world
            .entities
            .get_mut(&z_id)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove {
                target: SimVec2::new(SimScalar::from_num(9), SimScalar::from_num(32)),
            });

        let m_type = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");
        let m1 = world
            .spawn_unit(
                m_type,
                p_marines,
                SimVec2::new(
                    SimScalar::from_f32_lossy(9.5),
                    SimScalar::from_f32_lossy(31.5),
                ),
            )
            .unwrap();
        let m2 = world
            .spawn_unit(
                m_type,
                p_marines,
                SimVec2::new(
                    SimScalar::from_f32_lossy(8.0),
                    SimScalar::from_f32_lossy(32.5),
                ),
            )
            .unwrap();
        world
            .entities
            .get_mut(&m1)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: z_pos });
        world
            .entities
            .get_mut(&m2)
            .unwrap()
            .order_queue
            .push_back(UnitOrder::AttackMove { target: z_pos });

        // Activate Overclock on both Marines with full Combat Shield HP
        let marine_arch = world
            .catalog
            .get(world.catalog.id_by_slug("pulse_bot").unwrap())
            .unwrap();
        let oc_spec = marine_arch.overclock.unwrap();
        world
            .entities
            .get_mut(&m1)
            .unwrap()
            .apply_overclock(oc_spec)
            .expect("valid authored fixture order or buff");
        world.entities.get_mut(&m1).unwrap().hp = SimScalar::from_num(55);
        world
            .entities
            .get_mut(&m2)
            .unwrap()
            .apply_overclock(oc_spec)
            .expect("valid authored fixture order or buff");
        world.entities.get_mut(&m2).unwrap().hp = SimScalar::from_num(55);

        let (m_alive, z_alive) = run_battle_until_resolution(&mut world, 400, p_marines, p_zealot);
        assert!(
            m_alive >= 1,
            "Combat Shield + Overclock Marines must shred the Zealot"
        );
        assert_eq!(z_alive, 0, "Zealot must be destroyed before wiping Marines");
    }
}

// ==============================================================================================
// SUITES 26–35: MASS SWARM & SUPPLY-EQUAL BENCHMARKS
// ==============================================================================================

/// Suite 26: 100 Zerglings vs. 6 Colossi (`tripod_annihilator`).
/// 6 Colossi dual thermal lance sweep vaporizes all 100 Zerglings. Colossi win!
#[test]
fn test_suite_26_100_zerglings_vs_6_colossi() {
    let mut world = create_test_world(96);
    let p_swarm = PlayerId(0);
    let p_col = PlayerId(1);

    let col_target = SimVec2::new(SimScalar::from_num(40), SimScalar::from_num(48));
    let swarm_target = SimVec2::new(SimScalar::from_num(24), SimScalar::from_num(48));

    // Spawn 100 Zerglings in 10x10 formation
    spawn_100_zerglings(&mut world, p_swarm, 24.0, 44.5, col_target);

    // Spawn 6 Colossi
    spawn_grid_squad(
        &mut world,
        "tripod_annihilator",
        p_col,
        2,
        6,
        40.0,
        46.0,
        2.0,
        swarm_target,
    );

    let (s_alive, c_alive) = run_battle_until_resolution(&mut world, 700, p_swarm, p_col);
    assert_eq!(
        s_alive, 0,
        "100 Zerglings must be completely vaporized by 6 Colossi"
    );
    assert!(c_alive >= 5, "At least 5 Colossi must survive");
}

/// Suite 27: 100 Zerglings vs. 8 Ultralisks (`stag_behemoth`).
/// 8 Ultralisks (500 HP, 2 Armor, 35 cleave splash) slaughter all 100 Zerglings.
#[test]
fn test_suite_27_100_zerglings_vs_8_ultralisks() {
    let mut world = create_test_world(96);
    let p_swarm = PlayerId(0);
    let p_ultra = PlayerId(1);

    let ultra_target = SimVec2::new(SimScalar::from_num(50), SimScalar::from_num(48));
    let swarm_target = SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(48));

    spawn_100_zerglings(&mut world, p_swarm, 16.0, 44.5, ultra_target);
    spawn_grid_squad(
        &mut world,
        "stag_behemoth",
        p_ultra,
        2,
        8,
        50.0,
        45.0,
        2.0,
        swarm_target,
    );

    let (s_alive, u_alive) = run_battle_until_resolution(&mut world, 500, p_swarm, p_ultra);
    assert_eq!(
        s_alive, 0,
        "100 Zerglings must be cleaved down by 8 Ultralisks"
    );
    assert!(u_alive >= 6, "At least 6 Ultralisks must survive");
}

/// Suite 28: 100 Zerglings vs. 10 Immortals (`void_goliath`) — THE FAMOUS REVERSAL!
/// Immortals have 0 splash and attack slowly; 100 Zerglings overwhelm and destroy all 10 Immortals!
#[test]
fn test_suite_28_100_zerglings_vs_10_immortals_reversal() {
    let mut world = create_test_world(96);
    let p_swarm = PlayerId(0);
    let p_imm = PlayerId(1);

    let imm_target = SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(48));
    let swarm_target = SimVec2::new(SimScalar::from_num(18), SimScalar::from_num(48));

    spawn_100_zerglings(&mut world, p_swarm, 16.0, 44.5, imm_target);
    spawn_grid_squad(
        &mut world,
        "void_goliath",
        p_imm,
        2,
        10,
        45.0,
        44.0,
        1.5,
        swarm_target,
    );

    let (s_alive, i_alive) = run_battle_until_resolution(&mut world, 600, p_swarm, p_imm);
    assert_eq!(
        i_alive, 0,
        "10 Immortals with zero splash must be overrun and destroyed by 100 Zerglings"
    );
    assert!(
        s_alive >= 10,
        "A substantial portion of Zerglings must survive"
    );
}

/// Suite 29: 100 Zerglings vs. 12 Stalkers (`warp_strider`) — REVERSAL!
/// Stalkers have 0 splash and weak damage vs Light; 100 Zerglings surround and wipe out 12 Stalkers.
#[test]
fn test_suite_29_100_zerglings_vs_12_stalkers_reversal() {
    let mut world = create_test_world(96);
    let p_swarm = PlayerId(0);
    let p_stalker = PlayerId(1);

    let stalker_target = SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(48));
    let swarm_target = SimVec2::new(SimScalar::from_num(18), SimScalar::from_num(48));

    spawn_100_zerglings(&mut world, p_swarm, 16.0, 44.5, stalker_target);
    spawn_grid_squad(
        &mut world,
        "warp_strider",
        p_stalker,
        2,
        12,
        45.0,
        43.0,
        1.4,
        swarm_target,
    );

    let (s_alive, st_alive) = run_battle_until_resolution(&mut world, 600, p_swarm, p_stalker);
    assert_eq!(
        st_alive, 0,
        "12 Stalkers must be surrounded and destroyed by 100 Zerglings"
    );
    assert!(s_alive >= 15, "Substantial swarm survivors");
}

/// Suite 30: 20 Roaches vs. 10 Immortals (40 Supply Each).
/// Immortals deal 50 vs Armored (3-shotting Roaches). Immortals crush the Roaches!
#[test]
fn test_suite_30_20_roaches_vs_10_immortals_equal_supply() {
    let mut world = create_test_world(96);
    let p_roach = PlayerId(0);
    let p_imm = PlayerId(1);

    let imm_target = SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(48));
    let roach_target = SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(48));

    spawn_grid_squad(
        &mut world,
        "rhino_beetle",
        p_roach,
        4,
        20,
        20.0,
        44.0,
        1.2,
        imm_target,
    );
    spawn_grid_squad(
        &mut world,
        "void_goliath",
        p_imm,
        2,
        10,
        45.0,
        44.0,
        1.5,
        roach_target,
    );

    let (r_alive, i_alive) = run_battle_until_resolution(&mut world, 500, p_roach, p_imm);
    assert_eq!(r_alive, 0, "20 Roaches must be destroyed by 10 Immortals");
    assert!(i_alive >= 5, "At least half of Immortals must survive");
}

/// Suite 31: 15 Marauders vs. 15 Stalkers (30 Supply Each).
/// Marauders deal 20 vs Armored and destroy Stalkers in equal-supply battle.
#[test]
fn test_suite_31_15_marauders_vs_15_stalkers_equal_supply() {
    let mut world = create_test_world(96);
    let p_marauder = PlayerId(0);
    let p_stalker = PlayerId(1);

    let stalker_target = SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(48));
    let marauder_target = SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(48));

    spawn_grid_squad(
        &mut world,
        "shock_golem",
        p_marauder,
        3,
        15,
        20.0,
        44.0,
        1.2,
        stalker_target,
    );
    spawn_grid_squad(
        &mut world,
        "warp_strider",
        p_stalker,
        3,
        15,
        45.0,
        44.0,
        1.3,
        marauder_target,
    );

    let (m_alive, s_alive) = run_battle_until_resolution(&mut world, 500, p_marauder, p_stalker);
    assert_eq!(s_alive, 0, "15 Stalkers must be destroyed by 15 Marauders");
    assert!(m_alive >= 5, "Marauders must decisively win");
}

/// Suite 32: 30 Marines vs. 15 Banelings.
/// Baneling acid explosions wipe out the clumped Marines.
#[test]
fn test_suite_32_30_marines_vs_15_banelings() {
    let mut world = create_test_world(96);
    let p_marines = PlayerId(0);
    let p_banelings = PlayerId(1);

    let b_target = SimVec2::new(SimScalar::from_num(36), SimScalar::from_num(32));
    let m_target = SimVec2::new(SimScalar::from_num(30), SimScalar::from_num(32));

    spawn_grid_squad(
        &mut world,
        "pulse_bot",
        p_marines,
        5,
        30,
        30.0,
        30.0,
        0.8,
        b_target,
    );
    spawn_grid_squad(
        &mut world,
        "volatile_tick",
        p_banelings,
        3,
        15,
        36.0,
        31.0,
        0.8,
        m_target,
    );

    let (m_alive, _) = run_battle_until_resolution(&mut world, 300, p_marines, p_banelings);
    assert_eq!(
        m_alive, 0,
        "All 30 Marines must be destroyed by 15 Baneling detonations"
    );
}

/// Suite 33: 24 Marines vs. 12 Zealots.
/// In un-kited engagement, Zealots close distance and defeat Marines.
#[test]
fn test_suite_33_24_marines_vs_12_zealots() {
    let mut world = create_test_world(96);
    let p_marines = PlayerId(0);
    let p_zealots = PlayerId(1);

    let z_target = SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(48));
    let m_target = SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(48));

    spawn_grid_squad(
        &mut world,
        "pulse_bot",
        p_marines,
        4,
        24,
        20.0,
        44.0,
        0.8,
        z_target,
    );
    spawn_grid_squad(
        &mut world, "psi_grey", p_zealots, 3, 12, 45.0, 44.0, 1.2, m_target,
    );

    let (m_alive, z_alive) = run_battle_until_resolution(&mut world, 500, p_marines, p_zealots);
    assert_eq!(
        m_alive, 0,
        "Marines without kiting must be wiped out by Zealots"
    );
    assert!(z_alive >= 1, "Zealots must win the engagement");
}

/// Suite 34: 20 Hydralisks vs. 12 Mutalisks.
/// High-DPS Hydralisks focus down Mutalisks.
#[test]
fn test_suite_34_20_hydralisks_vs_12_mutalisks() {
    let mut world = create_test_world(96);
    let p_hydras = PlayerId(0);
    let p_mutas = PlayerId(1);

    let m_target = SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(48));
    let h_target = SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(48));

    spawn_grid_squad(
        &mut world,
        "thorn_viper",
        p_hydras,
        4,
        20,
        20.0,
        44.0,
        1.0,
        m_target,
    );
    spawn_grid_squad(
        &mut world,
        "sonic_bat",
        p_mutas,
        3,
        12,
        45.0,
        44.0,
        1.2,
        h_target,
    );

    let (h_alive, m_alive) = run_battle_until_resolution(&mut world, 400, p_hydras, p_mutas);
    assert_eq!(m_alive, 0, "Mutalisks must be shredded by 20 Hydralisks");
    assert!(h_alive >= 5, "Substantial Hydralisk survivors");
}

/// Suite 35: 12 Vikings vs. 6 Colossi (Air Superiority vs CliffWalker).
/// Vikings torpedo Colossi from Range 9.0 without taking damage.
#[test]
fn test_suite_35_12_vikings_vs_6_colossi() {
    let mut world = create_test_world(96);
    let p_vikings = PlayerId(0);
    let p_col = PlayerId(1);

    let col_target = SimVec2::new(SimScalar::from_num(40), SimScalar::from_num(48));
    let vik_target = SimVec2::new(SimScalar::from_num(28), SimScalar::from_num(48));

    spawn_grid_squad(
        &mut world,
        "vector_interceptor",
        p_vikings,
        3,
        12,
        28.0,
        44.0,
        1.5,
        col_target,
    );
    spawn_grid_squad(
        &mut world,
        "tripod_annihilator",
        p_col,
        2,
        6,
        40.0,
        44.0,
        2.0,
        vik_target,
    );

    let (v_alive, c_alive) = run_battle_until_resolution(&mut world, 900, p_vikings, p_col);
    assert_eq!(c_alive, 0, "All Colossi must be shot down by Vikings");
    assert_eq!(v_alive, 12, "All 12 Vikings take zero damage");
}
