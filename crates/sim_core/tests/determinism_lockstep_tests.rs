//! # Integration Test: 2,000-Tick Multi-Instance Determinism Checksum
//!
//! Verifies bit-for-bit lockstep determinism across two completely independent
//! [`SimWorld`] instances running a 2,000-tick battle simulation with identical commands.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder};

#[test]
fn test_2000_tick_multi_instance_lockstep_determinism() {
    let seed = 0xDEAD_BEEF_CAFE_BABE;

    let create_world = || {
        let default_cell = MapCell::default_ground(CliffLevel(0));
        let map = GameMap::new(64, 64, default_cell).expect("valid map");
        let mut world = SimWorld::new(map, seed);

        let pulse_bot_id = world
            .catalog
            .id_by_slug("pulse_bot")
            .expect("pulse_bot in catalog");
        let rhino_beetle_id = world
            .catalog
            .id_by_slug("rhino_beetle")
            .expect("rhino_beetle in catalog");
        let psi_grey_id = world
            .catalog
            .id_by_slug("psi_grey")
            .expect("psi_grey in catalog");
        let railgun_crawler_id = world
            .catalog
            .id_by_slug("railgun_crawler")
            .expect("railgun_crawler in catalog");
        let foundry_core_id = world
            .catalog
            .id_by_slug("foundry_core_dome")
            .expect("foundry_core_dome in catalog");

        // Spawn 20 Blue units (Player 0)
        let blue = PlayerId(0);
        for i in 0..16i32 {
            let y_off = SimScalar::from_num(i * 2 - 15);
            let pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32) + y_off);
            let _ = world.spawn_unit(pulse_bot_id, blue, pos);
        }
        for i in 0..4i32 {
            let y_off = SimScalar::from_num(i * 4 - 6);
            let pos = SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(32) + y_off);
            let _ = world.spawn_unit(rhino_beetle_id, blue, pos);
        }

        // Spawn 20 Red units (Player 1)
        let red = PlayerId(1);
        for i in 0..16i32 {
            let y_off = SimScalar::from_num(i * 2 - 15);
            let pos = SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(32) + y_off);
            let _ = world.spawn_unit(psi_grey_id, red, pos);
        }
        for i in 0..4i32 {
            let y_off = SimScalar::from_num(i * 4 - 6);
            let pos = SimVec2::new(SimScalar::from_num(48), SimScalar::from_num(32) + y_off);
            let _ = world.spawn_unit(railgun_crawler_id, red, pos);
        }

        // Spawn Bases
        let _ = world.spawn_structure(foundry_core_id, blue, 5, 5);
        let _ = world.spawn_structure(foundry_core_id, red, 54, 54);

        world
    };

    let mut world_a = create_world();
    let mut world_b = create_world();

    assert_eq!(
        world_a.last_checksum(),
        world_b.last_checksum(),
        "Initial state checksums must match identically"
    );

    // Pre-scripted player commands at specific ticks
    let build_scripted_bundle = |tick: u32| -> TickBundle {
        let mut commands = Vec::new();

        if tick == 10 {
            // Player 0 orders all mobile units to AttackMove East towards (45.0, 32.0)
            commands.push(PlayerCommand {
                player_id: PlayerId(0),
                entities: (1..=20).map(sim_types::EntityId).collect(),
                order: UnitOrder::AttackMove {
                    target: SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(32)),
                },
                queue: false,
            });
        } else if tick == 15 {
            // Player 1 orders all mobile units to AttackMove West towards (15.0, 32.0)
            commands.push(PlayerCommand {
                player_id: PlayerId(1),
                entities: (21..=40).map(sim_types::EntityId).collect(),
                order: UnitOrder::AttackMove {
                    target: SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32)),
                },
                queue: false,
            });
        } else if tick == 75 {
            // Player 0 activates Stimpack on Marines
            commands.push(PlayerCommand {
                player_id: PlayerId(0),
                entities: (1..=16).map(sim_types::EntityId).collect(),
                order: UnitOrder::UseStim,
                queue: false,
            });
        } else if tick == 250 {
            // Tactical retreat micro
            commands.push(PlayerCommand {
                player_id: PlayerId(0),
                entities: (1..=20).map(sim_types::EntityId).collect(),
                order: UnitOrder::Move {
                    target: SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32)),
                },
                queue: false,
            });
        } else if tick == 350 {
            // Counter-attack
            commands.push(PlayerCommand {
                player_id: PlayerId(0),
                entities: (1..=20).map(sim_types::EntityId).collect(),
                order: UnitOrder::AttackMove {
                    target: SimVec2::new(SimScalar::from_num(50), SimScalar::from_num(32)),
                },
                queue: false,
            });
        }

        TickBundle {
            tick: Tick(tick),
            commands,
            state_checksum: None,
        }
    };

    // Execute 2,000 continuous simulation ticks
    for tick in 1..=2000 {
        let bundle = build_scripted_bundle(tick);

        let snap_a = world_a.step(&bundle);
        let snap_b = world_b.step(&bundle);

        assert_eq!(
            snap_a.checksum, snap_b.checksum,
            "Checksum desync detected at tick {tick}: world_a={:#X}, world_b={:#X}",
            snap_a.checksum, snap_b.checksum
        );

        if tick % 100 == 0 {
            assert_eq!(
                world_a.entities.len(),
                world_b.entities.len(),
                "Entity count mismatch at tick {tick}"
            );
        }
    }

    // Final comprehensive state verification at tick 2,000
    assert_eq!(world_a.entities.len(), world_b.entities.len());
    for (id, ent_a) in &world_a.entities {
        let ent_b = world_b
            .entities
            .get(id)
            .expect("entity must exist in both worlds");
        assert_eq!(
            ent_a.position, ent_b.position,
            "Position mismatch for entity {:?}",
            id
        );
        assert_eq!(
            ent_a.velocity, ent_b.velocity,
            "Velocity mismatch for entity {:?}",
            id
        );
        assert_eq!(
            ent_a.heading, ent_b.heading,
            "Heading mismatch for entity {:?}",
            id
        );
        assert_eq!(
            ent_a.turret_angle, ent_b.turret_angle,
            "Turret angle mismatch for entity {:?}",
            id
        );
        assert_eq!(ent_a.hp, ent_b.hp, "HP mismatch for entity {:?}", id);
        assert_eq!(
            ent_a.shields, ent_b.shields,
            "Shields mismatch for entity {:?}",
            id
        );
        assert_eq!(
            ent_a.weapon_phase, ent_b.weapon_phase,
            "Weapon phase mismatch for entity {:?}",
            id
        );
    }
}
