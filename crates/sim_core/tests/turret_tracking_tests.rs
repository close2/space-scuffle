//! # Integration Test: Independent Turret Tracking While Retreating
//!
//! Verifies that a unit with `has_turret = true` (e.g. Siege Tank) rotates its turret
//! independently to aim and fire at a pursuer while its hull translates in the opposite direction.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::{SimWorld, WeaponPhase};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    PlayerCommand, PlayerId, SimAngle, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};

#[test]
fn test_independent_turret_tracking_while_retreating() {
    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0x9876_5432);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    // Tank at (20.0, 20.0)
    let tank_pos = SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(20));
    // Pursuer enemy at (20.0, 15.0) (5.0 units North, well within 7.0 weapon range)
    let enemy_pos = SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(15));

    let railgun_crawler_id = world
        .catalog
        .id_by_slug("railgun_crawler")
        .expect("railgun_crawler in catalog");
    let psi_grey_id = world
        .catalog
        .id_by_slug("psi_grey")
        .expect("psi_grey in catalog");

    let tank_id = world
        .spawn_unit(railgun_crawler_id, player_blue, tank_pos)
        .expect("invariant: spawn tank");
    let enemy_id = world
        .spawn_unit(psi_grey_id, player_red, enemy_pos)
        .expect("invariant: spawn enemy Zealot");

    // Orient Tank hull and turret South (+Y direction, 90°)
    {
        let tank = world.entities.get_mut(&tank_id).expect("tank exists");
        tank.heading = SimAngle::DEG_90;
        tank.turret_angle = SimAngle::DEG_90;
        // Set active attack target to enemy
        tank.current_target = Some(enemy_id);
    }

    // Issue Tank a Move order to (20.0, 30.0) (South)
    let move_cmd = PlayerCommand {
        player_id: player_blue,
        entities: vec![tank_id],
        order: UnitOrder::Move {
            target: SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(30)),
        },
        queue: false,
    };
    let bundle_0 = TickBundle {
        tick: Tick(0),
        commands: vec![move_cmd],
        state_checksum: None,
    };

    let tank_speed = world
        .catalog
        .get(railgun_crawler_id)
        .expect("railgun_crawler archetype")
        .max_speed;

    // Step tick 1: Tank moves South, turret begins rotating towards North (270°)
    world.step(&bundle_0);
    {
        let tank = world.entities.get(&tank_id).expect("tank exists");
        assert_eq!(
            tank.heading,
            SimAngle::DEG_90,
            "Hull must remain facing South"
        );
        assert_eq!(
            tank.position.y,
            SimScalar::from_num(20) + tank_speed,
            "Tank must translate South at full speed"
        );
        assert_ne!(
            tank.turret_angle,
            SimAngle::DEG_90,
            "Turret must begin independent rotation"
        );
    }

    let empty_bundle = |tick: u32| TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    };

    // Step simulation until turret aligns and fires (around tick 8-12)
    let mut fired = false;
    for tick in 1..20 {
        world.step(&empty_bundle(tick));
        let tank = world.entities.get(&tank_id).expect("tank exists");

        // Hull must ALWAYS continue translating South at full speed
        let expected_y = SimScalar::from_num(20) + tank_speed * SimScalar::from_num(tick + 1);
        assert_eq!(
            tank.position.y, expected_y,
            "Tank hull translation must never be interrupted by turret firing"
        );

        if matches!(
            tank.weapon_phase,
            WeaponPhase::Backswing { .. } | WeaponPhase::Cooldown
        ) {
            fired = true;
            break;
        }
    }

    assert!(
        fired,
        "Siege Tank must fire while continuing to retreat South at full speed"
    );

    // Advance 6 more ticks for the physical 90mm shell to travel and impact target
    for tick in 20..26 {
        world.step(&empty_bundle(tick));
    }

    // Verify enemy took damage from the retreating tank
    let enemy = world.entities.get(&enemy_id).expect("enemy exists");
    assert!(
        enemy.shields < SimScalar::from_num(50) || enemy.hp < SimScalar::from_num(100),
        "Enemy must sustain damage from the tank's independent turret strike"
    );
}
