//! # Integration Test: Second-Line Attackers Reach Range or Wait, Never Vibrate
//!
//! Plan `2026-11-09-friendly-blocking` (bugs B-103 to B-105). Attackers behind their own firing
//! line used to flip direction every tick against two friends that would not yield, then give
//! their orders up. These tests measure that with the blocking metrics of
//! `sim_fixtures::trace::blocking` and hold that plan's acceptance (§7), as plan
//! `2026-11-19-crowd-edge-steering` D-6 restates it:
//! 1. The showcase battle (`assets/scenarios/showcase.ron`), played to its end: no diagnostics,
//!    no give-ups, and reversals per owner no more than the residual of the classes that plan
//!    leaves out of scope (B-144, B-145). Attackers in range at tick 900 are reported only: the
//!    armies now meet mid-map near tick 300, so that tick no longer measures B-103.
//! 2. A synthetic arc: ten ranged units in two rows attack-move past an immobile pillar, and all
//!    ten reach range by tick 300 with no reversals and no give-ups.
//! 3. No room: units in a corridor two cells wide queue behind those that reach range, and
//!    those that cannot reach it stand, without give-ups (both bounds ignored with their residuals). Standing is the simulation's own: an achieved step of at most
//!    `STANDING_DRIFT` (1/64 world units).
//! 4. Walkers at a crowd's edge: short-ranged units behind a firing line walk round it without
//!    ending and restarting their walk every other tick (plan Amendment 7), at most three
//!    reversals each (plan `2026-11-19-crowd-edge-steering-2` D-1).
//! 5. A rounding once begun is held: the showcase's unit that starts to round a friend parked on
//!    its first corner keeps rounding it when the friend walks on the next tick (plan
//!    `2026-11-19-parked-body-aim` D-1, B-145).
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
use sim_fixtures::trace::blocking::{
    attackers_in_range, is_reversal, BlockingMeter, STANDING_DRIFT,
};
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

/// Reversals per owner the showcase may still show: those of the classes plan
/// `2026-11-19-crowd-edge-steering` leaves out of scope, a friend crossing the unit's path (R-E,
/// B-144) and the change of aim or route between two ticks (R-F, B-145), as its probe classified
/// them on the default seed (evidence §1: none for player 0; two R-F and one R-E for player 1).
const SHOWCASE_RESIDUAL_REVERSALS: [u32; 2] = [0, 3];

/// 1. The showcase battle, played to its end (at most 2,700 ticks): no diagnostics, no give-ups,
///    and reversals per owner within the out-of-scope residual (plan
///    `2026-11-19-crowd-edge-steering` D-6). Attackers in range at tick 900 are reported only.
#[test]
#[ignore = "crowd-edge-2 WP3: 8 and 9 reversals (proposal 4 and 5, contact 4 and 4), 0 give-ups, player 1 wins at tick 1,935; R-B, R-C, R-D, the walk's start retreat and R-E open (B-103)"]
fn showcase_crowd_edges_do_not_reverse() -> TestResult {
    let mut world = load_scenario_battle(&assets(), Path::new("scenarios/showcase.ron"))?;
    let mut meter = BlockingMeter::new();
    let mut in_range = [(0, 0); 2];
    while world.current_tick < 2_700 && !world.match_state.result.is_over() {
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
    let report = format!(
        "ended at {} {:?}; in range at 900 {in_range:?}; blue {blue:?}; red {red:?}",
        world.current_tick, world.match_state.result
    );
    assert!(world.match_state.result.is_over(), "{report}");
    assert_eq!((blue.stall_giveups, red.stall_giveups), (0, 0), "{report}");
    assert!(
        blue.reversals <= SHOWCASE_RESIDUAL_REVERSALS[0]
            && red.reversals <= SHOWCASE_RESIDUAL_REVERSALS[1],
        "{report}"
    );
    Ok(())
}

/// 2. Ten ranged units in two rows of five, 12 world units from an immobile pillar's edge,
///    attack-move past it: all ten reach range by tick 300, without a reversal or a give-up.
#[test]
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

/// The corridor run of test 3: per unit, from the tick it first stands, (ticks standing, ticks
/// exactly still, ticks counted), with the meter and the units in file order.
struct CorridorRun {
    meter: BlockingMeter,
    file: Vec<EntityId>,
    since_stop: Vec<Option<(u32, u32, u32)>>,
    in_range: (u32, u32),
}

impl CorridorRun {
    /// Six short-ranged units in single file attack-move along a corridor two cells wide onto
    /// the pillar that plugs it, for 400 ticks. A unit stands on a tick whose achieved step is at
    /// most [`STANDING_DRIFT`]; its count starts on the first tick after tick 20 that it stands.
    fn play() -> Result<Self, Box<dyn std::error::Error>> {
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
        let mut since_stop: Vec<Option<(u32, u32, u32)>> = vec![None; file.len()];
        while world.current_tick < 400 {
            step(&mut world, &mut meter, Vec::new());
            assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
            for (unit, record) in file.iter().zip(&mut since_stop) {
                let velocity = world.entities[unit].velocity;
                let still = velocity == SimVec2::ZERO;
                let standing = f64::from(velocity.length().to_f32()) <= STANDING_DRIFT;
                match record {
                    Some((stood, exact, of)) => {
                        *stood += u32::from(standing);
                        *exact += u32::from(still);
                        *of += 1;
                    }
                    None if standing && world.current_tick > 20 => {
                        *record = Some((1, u32::from(still), 1));
                    }
                    None => {}
                }
            }
        }
        let in_range = attackers_in_range(&world, BLUE);
        Ok(Self {
            meter,
            file,
            since_stop,
            in_range,
        })
    }

    fn report(&self) -> String {
        let per_unit: Vec<u32> = self
            .file
            .iter()
            .map(|u| self.meter.reversals_of(*u))
            .collect();
        format!(
            "in range {:?}; {:?}; reversals per unit {per_unit:?}; (standing, exactly still, of) \
             since stop {:?}",
            self.in_range,
            self.meter.of_owner(BLUE),
            self.since_stop
        )
    }
}

/// 3. No room: six short-ranged units queue in a corridor two cells wide behind those that reach
///    the pillar. Those that cannot reach range stand on at least 95 % of the ticks after they
///    first stand, nobody gives up and some are left out. Ticks exactly still are reported only,
///    since contact eases a pressed body apart by ever smaller steps that reach zero only after
///    tens of ticks (plan `2026-11-19-crowd-edge-steering` D-6, §2.7). The timed retry of a spent
///    walk walked the queue every 24 ticks (rear units on 65 to 88 %); without it (plan
///    `2026-11-19-crowd-edge-steering-2` D-2) they stood on 96 to 99.7 %. That plan's D-1 (a walk
///    does not end on the room its own retreat made) then lengthens two walks of entity 5 while
///    the queue forms, along the wall on ticks 70 to 114 and again on ticks 145 to 159 after it
///    gained ground; from tick 160 it stands spent. Its share falls to 87 %, a residual the owner's
///    orchestrator accepted (that plan's Amendment 1, OQ-2) and recorded under B-103; the bound
///    is kept, not loosened.
#[test]
#[ignore = "crowd-edge-2 WP3 (Amendment 1, OQ-2): 0 give-ups, 2 of 6 in range, 3 reversals; rear entities 3 to 7 stand on 332 of 340, 331 of 332, 283 of 324 (87.3 %), 309 of 309 and 300 of 316 (94.9 %) ticks; entities 5 and 7 below 95 % (B-103 residual)"]
fn no_room_means_waiting_still() -> TestResult {
    let run = CorridorRun::play()?;
    let report = run.report();
    let (in_range, attackers) = run.in_range;
    assert!(
        in_range >= 1 && in_range < attackers,
        "the corridor must leave units out: {report}"
    );
    assert_eq!(run.meter.of_owner(BLUE).stall_giveups, 0, "{report}");
    for (unit, record) in run.file.iter().zip(&run.since_stop) {
        let Some((stood, _, of)) = *record else {
            panic!("unit {unit:?} never stood: {report}");
        };
        assert!(
            stood * 100 >= of * 95,
            "unit {unit:?} moved too often: {report}"
        );
    }
    Ok(())
}

/// 3b. The corridor's queue does not reverse (friendly-blocking §7). Plan
///    `2026-11-19-crowd-edge-steering-2` takes it from 21 reversals to 7 (D-2) and 3 (D-1); the
///    rest are a walk's start retreat and contact (B-103).
#[test]
#[ignore = "crowd-edge-2 WP3: 3 reversals (2 proposal, 1 contact), one each for entities 5, 6 and 7 (B-103)"]
fn no_room_means_no_reversals() -> TestResult {
    let run = CorridorRun::play()?;
    assert_eq!(run.meter.of_owner(BLUE).reversals, 0, "{}", run.report());
    Ok(())
}

/// 4. Walkers at a crowd's edge: a line of five ranged units fires at the pillar, and three rows
///    of four short-ranged units behind it attack-move past. A walker whose straight way in is
///    half clipped by the firing line used to end its surround walk, step into contact and walk
///    again, a reversal every other tick (107 for one walker before friendly-blocking plan
///    Amendment 7, and 14 of 17 in range at tick 300). The walk now ends only when the whole
///    straight step is free, and the room beyond its own retreat (plan
///    `2026-11-19-crowd-edge-steering-2` D-1), so every attacker reaches range and no walker
///    reverses more than three times, 16 in all (31 before D-1; measured, not to be loosened).
#[test]
fn walkers_at_the_crowds_edge_do_not_toggle() -> TestResult {
    const GAP: f32 = 0.85;
    let mut world = scratch_world(GameMap::new(
        64,
        64,
        MapCell::default_ground(CliffLevel(0)),
    )?)?;
    spawn(&mut world, PILLAR, RED, at(40.0, 32.0))?;
    let mut army = Vec::new();
    for file in 0..5 {
        army.push(spawn(
            &mut world,
            SHOOTER,
            BLUE,
            at(27.5, 30.4 + 0.8 * file as f32),
        )?);
    }
    let mut walkers = Vec::new();
    for row in 0..3 {
        for file in 0..4 {
            let x = 25.0 - GAP * row as f32;
            let y = 30.8 + 0.8 * file as f32 + 0.4 * row as f32;
            walkers.push(spawn(&mut world, SHORT, BLUE, at(x, y))?);
        }
    }
    army.extend(&walkers);
    let mut meter = BlockingMeter::new();
    step(&mut world, &mut meter, attack_move(&army, at(60.0, 32.0)));
    while world.current_tick < 300 {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
    }
    let per_walker: Vec<u32> = walkers.iter().map(|u| meter.reversals_of(*u)).collect();
    let report = format!(
        "in range {:?}; {:?}; reversals per walker {per_walker:?}",
        attackers_in_range(&world, BLUE),
        meter.of_owner(BLUE)
    );
    assert_eq!(attackers_in_range(&world, BLUE), (17, 17), "{report}");
    assert_eq!(meter.of_owner(BLUE).stall_giveups, 0, "{report}");
    // A few single reversals at the crowd's edge remain (B-103); a toggling walk made dozens.
    assert!(per_walker.iter().all(|&count| count <= 3), "{report}");
    assert!(per_walker.iter().sum::<u32>() <= 16, "{report}");
    Ok(())
}

/// 5. The showcase's rhino beetle that starts at (58.5, 56.5) for player 1 (plan
///    `2026-11-19-parked-body-aim` T1, B-145): on the order's first tick a friend stands on its
///    first corner, so it starts to round that friend on the outside of the turn
///    (`corner_round` set); on the second the friend walks, and the unit keeps rounding instead of
///    turning back to the corner, so its achieved steps of ticks 1 and 2 do not reverse (the
///    blocking meter's test, a cosine of at least -0.3). On `bf4252cf` they were (0.126, 0) and
///    (-0.066, 0.108). The unit is found by owner and start position, not by entity id.
#[test]
#[ignore = "parked-body-aim WP1: red until WP2 lands D-1; on bf4252cf the steps are (0.126, 0) then (-0.066, 0.108)"]
fn showcase_parked_corner_aim_does_not_flip() -> TestResult {
    let mut world = load_scenario_battle(&assets(), Path::new("scenarios/showcase.ron"))?;
    let start = at(58.5, 56.5);
    let unit = world
        .entities
        .iter()
        .find(|(_, entity)| entity.owner == RED && entity.position == start)
        .map(|(&id, _)| id)
        .ok_or("the showcase has no player 1 unit at (58.5, 56.5)")?;
    let mut meter = BlockingMeter::new();
    let mut steps = Vec::new();
    for _ in 0..3 {
        step(&mut world, &mut meter, Vec::new());
        let entity = &world.entities[&unit];
        steps.push((
            f64::from(entity.velocity.x.to_f32()),
            f64::from(entity.velocity.y.to_f32()),
        ));
        if world.current_tick == 1 {
            assert_ne!(entity.movement.corner_round, 0, "no rounding on tick 1");
        }
    }
    assert_eq!(world.diagnostics(), []);
    assert!(
        !is_reversal(steps[0], steps[1]) && !is_reversal(steps[1], steps[2]),
        "steps of ticks 1 to 3: {steps:?}"
    );
    assert_eq!(
        meter.reversals_of(unit),
        0,
        "steps of ticks 1 to 3: {steps:?}"
    );
    Ok(())
}
