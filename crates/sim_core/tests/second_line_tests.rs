//! # Integration Test: Second-Line Attackers Reach Range or Wait, Never Vibrate
//!
//! Plan `2026-11-09-friendly-blocking` (bugs B-103 to B-105). Attackers behind their own firing
//! line used to flip direction every tick against two friends that would not yield, then give
//! their orders up. These tests measure that with the blocking metrics of
//! `sim_fixtures::trace::blocking` and hold the plan's acceptance (§7):
//! 1. The showcase battle (`assets/scenarios/showcase.ron`): at tick 900 every attacker of both
//!    sides has its target in range; no step reversals and no give-ups up to tick 1,300.
//! 2. A synthetic arc: ten ranged units in two rows attack-move past an immobile pillar, and all
//!    ten reach range by tick 300 with no reversals and no give-ups.
//! 3. No room: units in a corridor two cells wide queue behind those that reach range, and
//!    those that cannot reach it stand still, without reversals.
//!
//! The synthetic tests use scratch archetypes written in this file (C5), so a rebalance of the
//! shipped catalog cannot break them.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::panic
)]

use sim_core::catalog::UnitCatalog;
use sim_core::scenario::load_scenario_battle;
use sim_core::SimWorld;
use sim_fixtures::trace::blocking::{attackers_in_range, BlockingMeter};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The scratch shooter: radius 0.375, speed 0.126, scan 5.5, period 15, push priority 10.
const SHOOTER: &str = "line_shooter";
/// The scratch shooter's short-ranged twin, for the corridor.
const SHORT: &str = "line_short";
/// The scratch immobile enemy: radius 2.5, life far beyond what the test can deal.
const PILLAR: &str = "line_pillar";

const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// A scratch archetype in the catalog's RON form: radius, speed and the weapon's `max_range`
/// in world units; `None` for an unarmed one.
fn scratch_unit(slug: &str, id: u16, radius: f32, speed: f32, range: Option<f32>) -> String {
    let weapon = range.map_or_else(
        || "None".to_owned(),
        |range| {
            format!(
                r#"Some((
                name: "Scratch Rifle",
                visual_kind: Hitscan,
                target_flags: (bits: 255),
                min_range: 0.0,
                max_range: {range:.2},
                firing_arc_deg: 11.25,
                acquire_arc_deg: 0.0,
                has_turret: false,
                turret_turn_rate_deg_per_tick: 0.0,
                turret_idle: Hold,
                base_damage: 6.0,
                attribute_bonuses: [],
                attacks_count: 1,
                strike_interval_ticks: 0,
                period_ticks: 15,
                damage_point_ticks: 1,
                backswing_ticks: 13,
                random_delay_ticks: (0.0, 0.0),
                splash: None,
                projectile_speed: None,
                weapon_class: None,
                upgrade_bonus_per_level: 0.0,
                bonus_upgrade_per_level: 0.0,
            ))"#
            )
        },
    );
    format!(
        r#"(
            slug: "{slug}",
            unit_type_id: {id},
            faction: "automaton_directorate",
            display_name: "{slug}",
            lore: "Scratch archetype of second_line_tests.rs.",
            sc2_analogue: "None",
            showcase_hero: false,
            showcase_order: None,
            shape: UnitSphere,
            silhouette: LightProngs,
            mover_type: Ground,
            attributes: (bits: 1),
            max_hp: 100000.0,
            max_shields: 0.0,
            max_energy: 0.0,
            start_energy: 0.0,
            life_armor: 0.0,
            shield_armor: 0.0,
            hp_regen_per_tick: 0.0,
            shield_regen_per_tick: 0.0,
            energy_regen_per_tick: 0.0,
            max_speed: {speed:.3},
            acceleration: 3.136,
            deceleration: {speed:.3},
            turn_rate_deg_per_tick: 55.99,
            collision_radius: {radius:.3},
            separation_radius: {radius:.3},
            push_priority: 10,
            sight_radius: 9.0,
            scan_range: 5.5,
            attack_target_priority: 20,
            primary_weapon: Either,
            acquire_tie_break: Closest,
            mineral_cost: 50,
            gas_cost: 0,
            supply_cost: 1,
            supply_provided: 0,
            build_time_ticks: 100,
            weapon: {weapon},
            is_detector: false,
        )"#
    )
}

/// A world on `map` with the scratch shooters and pillar.
fn scratch_world(map: GameMap) -> Result<SimWorld, Box<dyn std::error::Error>> {
    let mut world = sim_fixtures::world(map, 0x5EC0_17D5);
    let units = format!(
        "(units: [{}, {}, {}])",
        scratch_unit(SHOOTER, 1, 0.375, 0.126, Some(5.0)),
        scratch_unit(SHORT, 2, 0.375, 0.126, Some(0.5)),
        scratch_unit(PILLAR, 3, 2.5, 0.0, None),
    );
    world.catalog = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &units)?;
    Ok(world)
}

fn at(x: f32, y: f32) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn spawn(
    world: &mut SimWorld,
    slug: &str,
    owner: PlayerId,
    pos: SimVec2,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    let kind = world.catalog.id_by_slug(slug).ok_or("scratch slug")?;
    Ok(world.spawn_unit(kind, owner, pos)?)
}

/// Steps `world` once with `commands`, then feeds the tick to `meter`.
fn step(world: &mut SimWorld, meter: &mut BlockingMeter, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
    meter.observe(world);
}

fn attack_move(units: &[EntityId], target: SimVec2) -> Vec<PlayerCommand> {
    vec![PlayerCommand {
        player_id: BLUE,
        entities: units.to_vec(),
        order: UnitOrder::AttackMove { target },
        queue: false,
    }]
}

/// The assets directory the game ships.
fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// 1. The showcase battle: every attacker of both sides in range at tick 900; no reversals and
///    no give-ups up to tick 1,300 (plan §7).
#[test]
#[ignore = "friendly-blocking WP2 (no reversals) and WP3 (all in range, no give-ups) make this pass"]
fn showcase_second_line_reaches_range() -> TestResult {
    let mut world = load_scenario_battle(&assets(), Path::new("scenarios/showcase.ron"))?;
    let mut meter = BlockingMeter::new();
    let mut in_range = [(0, 0); 2];
    while world.current_tick < 1_300 && !world.match_state.result.is_over() {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        if world.current_tick == 900 {
            in_range = [
                attackers_in_range(&world, BLUE),
                attackers_in_range(&world, RED),
            ];
        }
    }
    let (blue, red) = (meter.of_owner(BLUE), meter.of_owner(RED));
    let report = format!("in range at 900 {in_range:?}; blue {blue:?}; red {red:?}");
    for (hit, of) in in_range {
        assert!(of > 0 && hit == of, "{report}");
    }
    assert_eq!((blue.reversals, red.reversals), (0, 0), "{report}");
    assert_eq!((blue.stall_giveups, red.stall_giveups), (0, 0), "{report}");
    Ok(())
}

/// 2. Ten ranged units in two rows of five, 12 world units from an immobile pillar's edge,
///    attack-move past it: all ten reach range by tick 300, without a reversal or a give-up.
#[test]
#[ignore = "friendly-blocking WP3 (the surround walk) makes this pass"]
fn ranged_block_reaches_the_arc() -> TestResult {
    let mut world = scratch_world(GameMap::new(
        64,
        64,
        MapCell::default_ground(CliffLevel(0)),
    )?)?;
    let pillar = spawn(&mut world, PILLAR, RED, at(40.0, 32.0))?;
    let mut line = Vec::new();
    for row in 0..2 {
        for file in 0..5 {
            let x = 25.5 - 0.8 * row as f32;
            let y = 30.4 + 0.8 * file as f32 + 0.4 * row as f32;
            line.push(spawn(&mut world, SHOOTER, BLUE, at(x, y))?);
        }
    }
    let mut meter = BlockingMeter::new();
    step(&mut world, &mut meter, attack_move(&line, at(60.0, 32.0)));
    let mut all_in_range_at = None;
    while world.current_tick < 300 {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        if all_in_range_at.is_none() && attackers_in_range(&world, BLUE) == (10, 10) {
            all_in_range_at = Some(world.current_tick);
        }
    }
    let blue = meter.of_owner(BLUE);
    let report = format!(
        "in range at 300 {:?}, all in range at {all_in_range_at:?}; {blue:?}; pillar alive {}",
        attackers_in_range(&world, BLUE),
        world.entities[&pillar].is_alive()
    );
    assert!(all_in_range_at.is_some(), "{report}");
    assert_eq!(attackers_in_range(&world, BLUE), (10, 10), "{report}");
    assert_eq!((blue.reversals, blue.stall_giveups), (0, 0), "{report}");
    Ok(())
}

/// 3. Six short-ranged units in single file attack-move along a corridor two cells wide onto a
///    pillar that plugs it: those that cannot reach range stand still on at least 95 % of the
///    ticks after they first stop, and nobody reverses.
#[test]
#[ignore = "friendly-blocking WP2 (waiting) makes this pass"]
fn no_room_means_waiting_still() -> TestResult {
    let mut map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..64 {
        for x in 0..64 {
            if !(31..33).contains(&y) {
                map.cell_mut(x, y)?.flags = CellFlags::empty();
            }
        }
    }
    let mut world = scratch_world(map)?;
    spawn(&mut world, PILLAR, RED, at(40.0, 32.0))?;
    let file: Vec<EntityId> = (0..6)
        .map(|i| spawn(&mut world, SHORT, BLUE, at(30.0 - 0.8 * i as f32, 32.0)))
        .collect::<Result<_, _>>()?;
    let mut meter = BlockingMeter::new();
    step(&mut world, &mut meter, attack_move(&file, at(55.0, 32.0)));
    // Per unit: ticks still and ticks counted since its first stop, once it has stopped.
    let mut since_stop: Vec<Option<(u32, u32)>> = vec![None; file.len()];
    while world.current_tick < 400 {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        for (unit, record) in file.iter().zip(&mut since_stop) {
            let still = world.entities[unit].velocity == SimVec2::ZERO;
            match record {
                Some((stood, of)) => {
                    *stood += u32::from(still);
                    *of += 1;
                }
                None if still && world.current_tick > 20 => *record = Some((1, 1)),
                None => {}
            }
        }
    }
    let (in_range, attackers) = attackers_in_range(&world, BLUE);
    let blue = meter.of_owner(BLUE);
    let report =
        format!("in range {in_range} of {attackers}; {blue:?}; still since stop {since_stop:?}");
    assert!(
        in_range >= 1 && in_range < attackers,
        "the corridor must leave units out: {report}"
    );
    assert_eq!(blue.reversals, 0, "{report}");
    // Units in range stand to fire, and the others wait: every one stands once it has stopped.
    for (unit, record) in file.iter().zip(&since_stop) {
        let Some((stood, of)) = *record else {
            panic!("unit {unit:?} never stopped: {report}");
        };
        assert!(
            stood * 100 >= of * 95,
            "unit {unit:?} moved too often: {report}"
        );
    }
    Ok(())
}
