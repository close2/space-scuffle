//! # Integration Test: Friendly Push Priority vs. Hold Position & Enemy Hard-Blocking
//!
//! Verifies:
//! 1. Moving friendly unit pushes an Idle friendly unit aside laterally.
//! 2. Friendly unit on `HoldPosition` acts as an immovable obstacle.
//! 3. Enemy unit hard-blocks with reciprocal rigid collision.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::{compute_push_displacement, PushEntityParams, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};

#[test]
fn test_idle_friendly_displaced_laterally() {
    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("valid map");
    let mut world = SimWorld::new(map, 0x1111_2222);

    let player = PlayerId(0);
    let pulse_bot_id = world
        .catalog
        .id_by_slug("pulse_bot")
        .expect("pulse_bot in catalog");
    let marine_a = world
        .spawn_unit(
            pulse_bot_id,
            player,
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn marine a");
    let marine_b = world
        .spawn_unit(
            pulse_bot_id,
            player,
            SimVec2::new(SimScalar::from_num(10.5), SimScalar::from_num(10)),
        )
        .expect("spawn marine b (idle)");

    // Marine A orders Move to (20.0, 10.0) (heading East)
    let move_cmd = PlayerCommand {
        player_id: player,
        entities: vec![marine_a],
        order: UnitOrder::Move {
            target: SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(10)),
        },
        queue: false,
    };
    let bundle = TickBundle {
        tick: Tick(0),
        commands: vec![move_cmd],
        state_checksum: None,
    };

    world.step(&bundle);

    let ent_b = world.entities.get(&marine_b).expect("marine b exists");
    // Idle Marine B must have been displaced laterally along Y (orthogonal to East movement)
    assert_ne!(
        ent_b.position.y,
        SimScalar::from_num(10),
        "Idle friendly unit must yield laterally when pushed by moving ally"
    );
}

#[test]
fn test_hold_position_friendly_hard_blocks() {
    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("valid map");
    let mut world = SimWorld::new(map, 0x3333_4444);

    let player = PlayerId(0);
    let pulse_bot_id = world
        .catalog
        .id_by_slug("pulse_bot")
        .expect("pulse_bot in catalog");
    let marine_a = world
        .spawn_unit(
            pulse_bot_id,
            player,
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn marine a");
    let marine_b = world
        .spawn_unit(
            pulse_bot_id,
            player,
            SimVec2::new(SimScalar::from_num(10.5), SimScalar::from_num(10)),
        )
        .expect("spawn marine b (holding)");

    // Anchor Marine B on Hold Position
    {
        let ent_b = world.entities.get_mut(&marine_b).expect("marine b exists");
        ent_b.issue_order(UnitOrder::HoldPosition, false);
    }

    let initial_pos_b = world.entities.get(&marine_b).expect("b exists").position;

    // Marine A attempts to move through Marine B towards East
    let move_cmd = PlayerCommand {
        player_id: player,
        entities: vec![marine_a],
        order: UnitOrder::Move {
            target: SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(10)),
        },
        queue: false,
    };
    let bundle = TickBundle {
        tick: Tick(0),
        commands: vec![move_cmd],
        state_checksum: None,
    };

    world.step(&bundle);

    let ent_b = world.entities.get(&marine_b).expect("marine b exists");
    // HoldPosition Marine B must NOT have moved at all
    assert_eq!(
        ent_b.position, initial_pos_b,
        "Friendly unit on HoldPosition must never be displaced by moving allies"
    );
}

#[test]
fn test_enemy_unit_hard_collision_reciprocal() {
    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("valid map");
    let mut world = SimWorld::new(map, 0x5555_6666);

    let blue = PlayerId(0);
    let red = PlayerId(1);

    let pulse_bot_id = world
        .catalog
        .id_by_slug("pulse_bot")
        .expect("pulse_bot in catalog");
    let psi_grey_id = world
        .catalog
        .id_by_slug("psi_grey")
        .expect("psi_grey in catalog");

    let marine_a = world
        .spawn_unit(
            pulse_bot_id,
            blue,
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn marine a");
    let zealot_b = world
        .spawn_unit(
            psi_grey_id,
            red,
            SimVec2::new(SimScalar::from_num(10.5), SimScalar::from_num(10)),
        )
        .expect("spawn enemy zealot b");

    let move_cmd = PlayerCommand {
        player_id: blue,
        entities: vec![marine_a],
        order: UnitOrder::Move {
            target: SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(10)),
        },
        queue: false,
    };
    let bundle = TickBundle {
        tick: Tick(0),
        commands: vec![move_cmd],
        state_checksum: None,
    };

    world.step(&bundle);

    let ent_a = world.entities.get(&marine_a).expect("marine a exists");
    let ent_b = world.entities.get(&zealot_b).expect("zealot b exists");

    // Distance between marine and enemy zealot must maintain at least combined radius
    let dist = ent_a.position.distance(ent_b.position);
    let combined = SimScalar::from_num(0.375) + SimScalar::from_num(0.5); // 0.875

    assert!(
        dist >= combined - SimScalar::from_num(0.01),
        "Enemy units must hard-block and maintain collision boundary, dist={}, combined={}",
        dist,
        combined
    );
}

#[test]
fn test_pure_displacement_math() {
    let pos_a = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10));
    let vel_a = SimVec2::new(SimScalar::from_num(1), SimScalar::ZERO);
    let pos_b = SimVec2::new(SimScalar::from_num(10.5), SimScalar::from_num(10));
    let vel_b = SimVec2::ZERO;

    let a = PushEntityParams {
        id: EntityId(1),
        pos: pos_a,
        vel: vel_a,
        owner: PlayerId(0),
        push_priority: 10,
        cannot_be_pushed: false,
        is_holding: false,
    };
    let b = PushEntityParams {
        id: EntityId(2),
        pos: pos_b,
        vel: vel_b,
        owner: PlayerId(0),
        push_priority: 10,
        cannot_be_pushed: false,
        is_holding: false,
    };

    // Moving friendly collides with Idle friendly:
    let (d_a, d_b) =
        compute_push_displacement(a, b, SimScalar::from_num(0.5), SimScalar::from_num(0.8));

    assert_eq!(
        d_a,
        SimVec2::ZERO,
        "Moving ally keeps full forward translation"
    );
    assert_ne!(
        d_b.y,
        SimScalar::ZERO,
        "Idle ally must be displaced sideways"
    );
}
