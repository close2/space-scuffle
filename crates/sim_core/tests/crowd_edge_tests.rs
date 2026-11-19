//! # Integration Test: A Queue That Stands (B-103)
//!
//! Plan `2026-11-19-crowd-edge-steering`, WP4 as Amendment 3 restates it. A tick spent queued
//! behind a friend that holds its ground is not a stalled tick, so an attack order is kept (D-4,
//! the owner's answer to Q0089), while a plain `Move` into a blocked spot is still given up
//! (CE-9). CE-7 and CE-8 belong to D-3, the re-arm of a spent walk without the timed retry, which
//! Amendment 3 moved to a new plan; they are kept here, ignored, with their figures.
//!
//! The tests use scratch archetypes written in this file (C5), so a rebalance of the shipped
//! catalog cannot break them. Positions are converted to `f64` for measurement only.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::panic
)]

use sim_core::catalog::UnitCatalog;
use sim_core::movement::RouteOutcomeKind;
use sim_core::SimWorld;
use sim_fixtures::trace::blocking::{BlockingMeter, STANDING_DRIFT};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackOrigin, EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder,
};

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
            lore: "Scratch archetype of crowd_edge_tests.rs.",
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

/// A 64 × 64 map whose only ground is a corridor two cells wide, rows 31 and 32, with the
/// pillar plugging it at (40, 32).
fn corridor() -> Result<SimWorld, Box<dyn std::error::Error>> {
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
    Ok(world)
}

fn order(units: &[EntityId], order: UnitOrder) -> Vec<PlayerCommand> {
    vec![PlayerCommand {
        player_id: BLUE,
        entities: units.to_vec(),
        order,
        queue: false,
    }]
}

/// Whether `unit` stands this tick: an achieved step of at most `STANDING_DRIFT`.
fn standing(world: &SimWorld, unit: EntityId) -> bool {
    f64::from(world.entities[&unit].velocity.length().to_f32()) <= STANDING_DRIFT
}

/// CE-7 (D-4): in the corridor, two short-ranged shooters fire at the pillar side by side, filling
/// its face, and a chaser ordered onto the pillar queues behind them. Over 600 ticks, beyond the 250-tick stall rule and the
/// 50-tick abandoned-target wait, the chaser never gives its target up and stands on at least
/// 95 % of its ticks after it first stands. Before D-3 and D-4 it walked every 24 ticks; with
/// only the timed retry removed it would give up 250 ticks after its first stop.
#[test]
#[ignore = "crowd-edge Amendment 3: needs D-3 (moved to a new plan); with D-4 alone the chaser keeps its target but stands on 386 of 580 ticks, walking on the timed retry (B-103)"]
fn a_queued_chaser_keeps_its_target() -> TestResult {
    let mut world = corridor()?;
    let pillar = EntityId(1);
    let front = [
        spawn(&mut world, SHORT, BLUE, at(36.6, 31.45))?,
        spawn(&mut world, SHORT, BLUE, at(36.6, 32.55))?,
    ];
    let chaser = spawn(&mut world, SHORT, BLUE, at(34.5, 32.0))?;
    let mut meter = BlockingMeter::new();
    let mut commands = order(
        &front,
        UnitOrder::AttackMove {
            target: at(55.0, 32.0),
        },
    );
    commands.extend(order(
        &[chaser],
        UnitOrder::AttackTarget {
            target: pillar,
            origin: AttackOrigin::Explicit,
        },
    ));
    step(&mut world, &mut meter, commands);
    let mut since_stand: Option<(u32, u32)> = None;
    let mut abandoned: Vec<sim_core::movement::RouteOutcome> = Vec::new();
    while world.current_tick < 600 {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        abandoned.extend(world.route_outcomes().iter().copied().filter(|outcome| {
            outcome.entity == chaser
                && matches!(outcome.kind, RouteOutcomeKind::PursuitAbandoned { .. })
        }));
        let stands = standing(&world, chaser);
        match since_stand.as_mut() {
            Some((stood, of)) => {
                *stood += u32::from(stands);
                *of += 1;
            }
            None if stands && world.current_tick > 20 => since_stand = Some((1, 1)),
            None => {}
        }
    }
    let report = format!(
        "abandoned {abandoned:?}; (standing, of) since it first stood {since_stand:?}; {:?}; \
         order {:?}",
        meter.of_owner(BLUE),
        world.entities[&chaser].order_queue.front()
    );
    assert!(abandoned.is_empty(), "{report}");
    assert!(
        matches!(
            world.entities[&chaser].order_queue.front(),
            Some(UnitOrder::AttackTarget { .. })
        ),
        "{report}"
    );
    let (stood, of) = since_stand.ok_or_else(|| report.clone())?;
    assert!(stood * 100 >= of * 95, "{report}");
    Ok(())
}

/// The distance from `unit`'s edge to the pillar's, world units.
fn edge_to_pillar(world: &SimWorld, unit: EntityId) -> f64 {
    f64::from(
        (world.entities[&unit].position - at(40.0, 32.0))
            .length()
            .to_f32(),
    ) - 2.875
}

/// CE-8 (D-3, a guard): in the corridor two short-ranged shooters fire at the pillar side by side
/// and a third queues behind them until its surround walk is spent, both sides closed by the corridor's
/// walls and the pair. One of the pair is then killed: the queued unit gains ground, which re-arms its
/// walk (`flank_spent` clears), and it reaches range within 200 ticks. In the open walkers layout
/// of `second_line_tests.rs` no walk is spent within 600 ticks, so the corridor stands in for it.
#[test]
#[ignore = "crowd-edge Amendment 3: D-3's re-arm on ground gained moved to a new plan; with the timed retry the unit reaches range but its walk does not re-arm within 6 ticks (B-103)"]
fn a_spent_walk_resumes_when_the_way_opens() -> TestResult {
    let mut world = corridor()?;
    let front = spawn(&mut world, SHORT, BLUE, at(36.6, 31.45))?;
    let beside = spawn(&mut world, SHORT, BLUE, at(36.6, 32.55))?;
    let queued = spawn(&mut world, SHORT, BLUE, at(34.5, 32.0))?;
    let mut meter = BlockingMeter::new();
    let attack = UnitOrder::AttackMove {
        target: at(55.0, 32.0),
    };
    step(
        &mut world,
        &mut meter,
        order(&[front, beside, queued], attack),
    );
    while !world.entities[&queued].movement.flank_spent {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        assert!(
            world.current_tick < 600,
            "no walk was spent by tick 600: {:?} at {:?}",
            world.entities[&queued].movement,
            world.entities[&queued].position
        );
    }
    world.entities.get_mut(&front).ok_or("front")?.hp = SimScalar::ZERO;
    let killed_at = world.current_tick;
    let (mut rearmed, mut reached) = (false, None);
    while world.current_tick < killed_at + 200 && reached.is_none() {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        rearmed |= !world.entities[&queued].movement.flank_spent;
        if edge_to_pillar(&world, queued) <= 0.5 + 1e-4 {
            reached = Some(world.current_tick);
        }
    }
    let report = format!(
        "spent until tick {killed_at}; at tick {} it is {} from range; {:?}",
        world.current_tick,
        edge_to_pillar(&world, queued) - 0.5,
        world.entities[&queued].movement
    );
    assert!(rearmed, "{report}");
    assert!(reached.is_some(), "{report}");
    Ok(())
}

/// CE-9 (D-4 keeps `StarCraft` II's give-up for moves, S8, a guard): a plain `Move` along the
/// corridor into a spot two holding friends plug is given up with `NoProgress`, after the 250-tick
/// stall rule and not before.
#[test]
fn a_blocked_move_is_still_given_up() -> TestResult {
    let mut world = corridor()?;
    let holders = [
        spawn(&mut world, SHORT, BLUE, at(30.0, 31.45))?,
        spawn(&mut world, SHORT, BLUE, at(30.0, 32.55))?,
    ];
    let mover = spawn(&mut world, SHORT, BLUE, at(26.0, 32.0))?;
    let mut meter = BlockingMeter::new();
    let mut commands = order(&holders, UnitOrder::HoldPosition);
    commands.extend(order(
        &[mover],
        UnitOrder::Move {
            target: at(36.0, 32.0),
        },
    ));
    step(&mut world, &mut meter, commands);
    let mut given_up = None;
    while world.current_tick < 600 && given_up.is_none() {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        if world
            .route_outcomes()
            .iter()
            .any(|o| o.entity == mover && o.kind == RouteOutcomeKind::NoProgress)
        {
            given_up = Some(world.current_tick);
        }
    }
    let tick = given_up.ok_or_else(|| {
        format!(
            "the blocked move was never given up: mover at {:?}, memory {:?}, order {:?}, ",
            world.entities[&mover].position,
            world.entities[&mover].movement,
            world.entities[&mover].order_queue.front()
        )
    })?;
    assert!(tick > 250, "given up at tick {tick}, before the stall rule");
    assert!(
        world.entities[&mover].order_queue.is_empty()
            || matches!(
                world.entities[&mover].order_queue.front(),
                None | Some(UnitOrder::Idle)
            )
    );
    Ok(())
}
