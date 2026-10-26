//! # Integration Test: Stutter-Step Kiting & Backswing Cancellation
//!
//! Verifies that issuing a Move command during `WeaponPhase::Backswing`
//! immediately aborts the follow-through animation, shifts the weapon into `Cooldown`,
//! and allows the unit to translate away from a melee chaser while maintaining full DPS.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::{SimWorld, WeaponPhase};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder};

#[test]
fn test_stutter_step_backswing_cancellation() {
    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0x1234_5678);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    // Place Marine at (10.0, 10.0) and Zealot at (14.0, 10.0)
    let marine_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10));
    let zealot_pos = SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(10));

    let pulse_bot_id = world
        .catalog
        .id_by_slug("pulse_bot")
        .expect("pulse_bot in catalog");
    let psi_grey_id = world
        .catalog
        .id_by_slug("psi_grey")
        .expect("psi_grey in catalog");

    let marine_id = world
        .spawn_unit(pulse_bot_id, player_blue, marine_pos)
        .expect("invariant: spawn Marine");
    let zealot_id = world
        .spawn_unit(psi_grey_id, player_red, zealot_pos)
        .expect("invariant: spawn Zealot");

    // Marine orders AttackTarget(Zealot)
    let attack_cmd = PlayerCommand {
        player_id: player_blue,
        entities: vec![marine_id],
        order: UnitOrder::AttackTarget { target: zealot_id },
        queue: false,
    };
    let bundle_0 = TickBundle {
        tick: Tick(0),
        commands: vec![attack_cmd],
        state_checksum: None,
    };

    // Tick 1: Process command, target in range, enter Windup (remaining_ticks: 2)
    world.step(&bundle_0);
    {
        let marine = world.entities.get(&marine_id).expect("marine exists");
        assert!(
            matches!(
                marine.weapon_phase,
                WeaponPhase::Windup {
                    remaining_ticks: 2,
                    ..
                }
            ),
            "Expected Windup with 2 ticks, got {:?}",
            marine.weapon_phase
        );
    }

    let empty_bundle = |tick: u32| TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    };

    // Tick 2: Windup decrements: 2 -> 1
    world.step(&empty_bundle(1));
    {
        let marine = world.entities.get(&marine_id).expect("marine exists");
        assert!(
            matches!(
                marine.weapon_phase,
                WeaponPhase::Windup {
                    remaining_ticks: 1,
                    ..
                }
            ),
            "Expected Windup with 1 tick, got {:?}",
            marine.weapon_phase
        );
    }

    // Tick 3: Windup finishes (1 -> 0), shot fires, Zealot damaged, enters Backswing
    world.step(&empty_bundle(2));
    {
        let zealot = world.entities.get(&zealot_id).expect("zealot exists");
        assert_eq!(
            zealot.shields,
            SimScalar::from_num(44), // 50 - 6 = 44
            "Zealot shields should take 6 damage"
        );
        let marine = world.entities.get(&marine_id).expect("marine exists");
        assert!(
            matches!(marine.weapon_phase, WeaponPhase::Backswing { .. }),
            "Marine should be in Backswing immediately after firing, got {:?}",
            marine.weapon_phase
        );
    }

    // Tick 4: Player issues Move(0.0, 10.0), cancelling backswing immediately!
    let move_cmd = PlayerCommand {
        player_id: player_blue,
        entities: vec![marine_id],
        order: UnitOrder::Move {
            target: SimVec2::new(SimScalar::ZERO, SimScalar::from_num(10)),
        },
        queue: false,
    };
    let bundle_move = TickBundle {
        tick: Tick(3),
        commands: vec![move_cmd],
        state_checksum: None,
    };

    world.step(&bundle_move);
    {
        let marine = world.entities.get(&marine_id).expect("marine exists");
        let speed = world
            .catalog
            .get(marine.unit_type_id)
            .expect("pulse_bot archetype")
            .max_speed;
        // Backswing must be cancelled, weapon in Cooldown, and Marine moved west by speed
        assert!(
            matches!(marine.weapon_phase, WeaponPhase::Cooldown),
            "Marine weapon phase should transition to Cooldown on move cancellation, got {:?}",
            marine.weapon_phase
        );
        let expected_x = SimScalar::from_num(10) - speed;
        assert_eq!(
            marine.position.x, expected_x,
            "Marine should translate west"
        );
    }

    // Advance 2 more ticks (ticks 5 and 6)
    world.step(&empty_bundle(4));
    world.step(&empty_bundle(5));

    {
        let marine = world.entities.get(&marine_id).expect("marine exists");
        let speed = world
            .catalog
            .get(marine.unit_type_id)
            .expect("pulse_bot archetype")
            .max_speed;
        let expected_x = SimScalar::from_num(10) - speed * 3;
        assert_eq!(
            marine.position.x, expected_x,
            "Marine should move 3 ticks of speed west"
        );
    }
}
