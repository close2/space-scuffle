//! # Integration Test: Friendly Push Priority vs. Hold Position & Enemy Hard-Blocking
//!
//! Verifies:
//! 1. Moving friendly unit pushes an Idle friendly unit aside laterally.
//! 2. Friendly unit on `HoldPosition` acts as an immovable obstacle.
//! 3. Enemy unit hard-blocks with reciprocal rigid collision.
//! 4. A unit turning in place before it moves (`turn_before_move`, plan
//!    `2026-11-05-turn-before-move`) is pushed like a standing unit, and the push neither waits
//!    for its facing nor changes its heading.
//! 5. A push is not gated by facing: an idle `turn_before_move` unit is displaced sideways.
//!
//! Tests 4 and 5 use scratch archetypes written in this file, so a rebalance of the shipped
//! catalog cannot break them; the others read the shipped catalog.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::catalog::UnitCatalog;
use sim_core::{compute_push_displacement, PushEntityParams, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimAngle, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};

/// The scratch unit that turns before it moves: push priority 10, 20.16° per tick.
const PIVOT: &str = "push_pivot";
/// The scratch unit that pushes it: push priority 50, turns at 180° per tick.
const PUSHER: &str = "push_pusher";

/// One unarmed scratch unit, radius 0.5 and speed 0.15, in the catalog's RON form.
fn scratch_unit(
    slug: &str,
    id: u16,
    turn_rate: f32,
    priority: u8,
    turn_before_move: bool,
) -> String {
    format!(
        r#"(
            slug: "{slug}",
            unit_type_id: {id},
            faction: "automaton_directorate",
            display_name: "{slug}",
            lore: "Scratch archetype of push_priority_tests.rs.",
            sc2_analogue: "None",
            showcase_hero: false,
            showcase_order: None,
            shape: UnitSphere,
            silhouette: LightProngs,
            mover_type: Ground,
            attributes: (bits: 1),
            max_hp: 100.0,
            max_shields: 0.0,
            max_energy: 0.0,
            start_energy: 0.0,
            life_armor: 0.0,
            shield_armor: 0.0,
            hp_regen_per_tick: 0.0,
            shield_regen_per_tick: 0.0,
            energy_regen_per_tick: 0.0,
            max_speed: 0.15,
            acceleration: 0.15,
            deceleration: 0.15,
            turn_rate_deg_per_tick: {turn_rate:.2},
            turn_before_move: {turn_before_move},
            collision_radius: 0.5,
            separation_radius: 0.5,
            push_priority: {priority},
            sight_radius: 8.0,
            scan_range: 8.0,
            attack_target_priority: 20,
            primary_weapon: Either,
            acquire_tie_break: Closest,
            mineral_cost: 50,
            gas_cost: 0,
            supply_cost: 1,
            supply_provided: 0,
            build_time_ticks: 100,
            weapon: None,
            is_detector: false,
        )"#
    )
}

/// A flat world with the two scratch archetypes, a pivot and a pusher of player 0 at the given
/// points (world units). The pivot spawns facing +x.
fn pivot_and_pusher(
    pivot_at: (f32, f32),
    pusher_at: (f32, f32),
) -> Result<(SimWorld, EntityId, EntityId), Box<dyn std::error::Error>> {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 0x7777_8888);
    let units = format!(
        "(units: [{}, {}])",
        scratch_unit(PIVOT, 1, 20.16, 10, true),
        scratch_unit(PUSHER, 2, 180.0, 50, false),
    );
    world.catalog = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &units)?;
    let at = |(x, y): (f32, f32)| SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y));
    let pivot_type = world.catalog.id_by_slug(PIVOT).ok_or("pivot")?;
    let pusher_type = world.catalog.id_by_slug(PUSHER).ok_or("pusher")?;
    let pivot = world.spawn_unit(pivot_type, PlayerId(0), at(pivot_at))?;
    let pusher = world.spawn_unit(pusher_type, PlayerId(0), at(pusher_at))?;
    Ok((world, pivot, pusher))
}

/// A move order for `unit` of player 0 to (`x`, `y`), world units.
fn move_command(unit: EntityId, x: i32, y: i32) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(0),
        entities: vec![unit],
        order: UnitOrder::Move {
            target: SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y)),
        },
        queue: false,
    }
}

#[test]
fn a_unit_turning_in_place_is_pushed_like_a_standing_unit() -> Result<(), Box<dyn std::error::Error>>
{
    // The pivot faces +x and is ordered west, so it turns in place; the pusher walks east into it.
    let (mut world, pivot, pusher) = pivot_and_pusher((10.9, 10.2), (10.0, 10.0))?;
    let rate = world
        .catalog
        .get(world.catalog.id_by_slug(PIVOT).ok_or("pivot")?)?
        .turn_rate
        .raw();
    let start = world.entities.get(&pivot).ok_or("pivot")?.position;
    world.step(&TickBundle {
        tick: Tick(0),
        commands: vec![move_command(pivot, 2, 10), move_command(pusher, 30, 10)],
        state_checksum: None,
    });
    let after = world.entities.get(&pivot).ok_or("pivot")?;
    assert_ne!(after.position, start, "the turning pivot is pushed");
    assert_eq!(
        SimAngle::ZERO.diff(after.heading).unsigned_abs(),
        rate,
        "a push does not change the turn in place"
    );
    assert_eq!(
        after.order_queue.front(),
        Some(&UnitOrder::Move {
            target: SimVec2::new(SimScalar::from_num(2), SimScalar::from_num(10)),
        })
    );
    Ok(())
}

#[test]
fn a_push_is_not_gated_by_facing() -> Result<(), Box<dyn std::error::Error>> {
    // The idle pivot faces +x; the pusher walks north into it.
    let (mut world, pivot, pusher) = pivot_and_pusher((10.2, 10.9), (10.0, 10.0))?;
    let start = world.entities.get(&pivot).ok_or("pivot")?.position;
    world.step(&TickBundle {
        tick: Tick(0),
        commands: vec![move_command(pusher, 10, 30)],
        state_checksum: None,
    });
    let after = world.entities.get(&pivot).ok_or("pivot")?;
    assert_ne!(after.position, start, "the idle pivot is pushed");
    assert_eq!(after.heading, SimAngle::ZERO, "a push never turns the hull");
    Ok(())
}

#[test]
fn test_idle_friendly_displaced_laterally() {
    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("valid map");
    let mut world = sim_fixtures::world(map, 0x1111_2222);

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
    let mut world = sim_fixtures::world(map, 0x3333_4444);

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
        ent_b
            .issue_order(UnitOrder::HoldPosition, false)
            .expect("valid authored fixture order or buff");
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
    let mut world = sim_fixtures::world(map, 0x5555_6666);

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
fn test_pure_displacement_math() -> Result<(), Box<dyn std::error::Error>> {
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
        compute_push_displacement(a, b, SimScalar::from_num(0.5), SimScalar::from_num(0.8))?;

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
    Ok(())
}
