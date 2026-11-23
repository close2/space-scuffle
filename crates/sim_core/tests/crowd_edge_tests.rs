//! # Integration Test: A Queue That Stands (B-103)
//!
//! Plan `2026-11-19-crowd-edge-steering`, WP4 as Amendment 3 restates it: a tick spent queued
//! behind a friend that holds its ground is not a stalled tick, so an attack order is kept (D-4,
//! the owner's answer to Q0089), while a plain `Move` into a blocked spot is still given up
//! (CE-9). Plan `2026-11-19-crowd-edge-steering-2` D-2: a spent surround walk has no timed retry;
//! it re-arms when the side that closed last opens for a whole step (CE-10), when the unit gains
//! ground (CE-8) or when its target changes, and never on the slivers contact eases a pressed unit
//! by (CE-11), so a queued chaser stands (CE-7). D-1 of that plan: a walk does not end on the room
//! its own retreat made (CE-12), while the retreat that leads out of a pocket is still taken
//! (CE-13) and a walk with no retreat ends as before (CE-14).
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
use sim_fixtures::trace::blocking::{
    attackers_in_range, is_reversal, BlockingMeter, STANDING_DRIFT,
};
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
                first_strike_delay_ticks: 0,
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
/// 95 % of its ticks after it first stands. Under the timed retry it walked every 24 ticks
/// (standing on 386 of 580 ticks); with only the retry removed and no D-4 it would give up 250
/// ticks after its first stop.
#[test]
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

/// CE-8 (plan `2026-11-19-crowd-edge-steering-2` D-2, a guard): in the corridor two short-ranged
/// shooters fire at the pillar side by side and a third queues behind them until its surround walk
/// is spent, both sides closed by the corridor's walls and the pair. One of the pair is then
/// killed: the queued unit gains ground, which re-arms its walk (`flank_spent` clears) within six
/// ticks, and it reaches range within 200 ticks. In the open walkers layout of
/// `second_line_tests.rs` no walk is spent within 600 ticks, so the corridor stands in for it.
#[test]
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
    let (mut rearmed, mut rearmed_at, mut reached) = (false, None, None);
    while world.current_tick < killed_at + 200 && reached.is_none() {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        if !rearmed && !world.entities[&queued].movement.flank_spent {
            rearmed = true;
            rearmed_at = Some(world.current_tick);
        }
        if edge_to_pillar(&world, queued) <= 0.5 + 1e-4 {
            reached = Some(world.current_tick);
        }
    }
    let report = format!(
        "spent until tick {killed_at}, re-armed at {rearmed_at:?}; at tick {} it is {} from \
         range; {:?}",
        world.current_tick,
        edge_to_pillar(&world, queued) - 0.5,
        world.entities[&queued].movement
    );
    assert!(rearmed, "{report}");
    assert!(
        rearmed_at.is_some_and(|tick| tick <= killed_at + 6),
        "the walk must re-arm within six ticks of the way opening: {report}"
    );
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

/// The edge-to-edge gap between two units, world units.
fn edge_gap(world: &SimWorld, a: EntityId, b: EntityId) -> f64 {
    f64::from(
        (world.entities[&a].position - world.entities[&b].position)
            .length()
            .to_f32(),
    ) - 0.75
}

/// The CE-10 world: the world, its meter, the chaser and the friends beside it (below, above).
type BoxedChaser = (SimWorld, BlockingMeter, EntityId, [EntityId; 2]);

/// The CE-10 layout on open ground: two long-ranged shooters hold and fire at the pillar, a
/// short-ranged chaser ordered onto the pillar stands in the notch behind them, and two friends
/// hold beside the chaser, one on each side, so both sides of its surround walk close and the
/// walk is spent (on tick 26). Returns the world, the meter, the chaser and the two friends.
fn boxed_chaser() -> Result<BoxedChaser, Box<dyn std::error::Error>> {
    let mut world = scratch_world(GameMap::new(
        64,
        64,
        MapCell::default_ground(CliffLevel(0)),
    )?)?;
    let pillar = spawn(&mut world, PILLAR, RED, at(40.0, 32.0))?;
    let shooters = [
        spawn(&mut world, SHOOTER, BLUE, at(35.8, 31.55))?,
        spawn(&mut world, SHOOTER, BLUE, at(35.8, 32.45))?,
    ];
    let chaser = spawn(&mut world, SHORT, BLUE, at(35.2, 32.0))?;
    let beside = [
        spawn(&mut world, SHORT, BLUE, at(35.05, 31.25))?,
        spawn(&mut world, SHORT, BLUE, at(35.05, 32.75))?,
    ];
    let mut meter = BlockingMeter::new();
    let mut commands = order(&shooters, UnitOrder::HoldPosition);
    commands.extend(order(&beside, UnitOrder::HoldPosition));
    commands.extend(order(
        &[chaser],
        UnitOrder::AttackTarget {
            target: pillar,
            origin: AttackOrigin::Explicit,
        },
    ));
    step(&mut world, &mut meter, commands);
    Ok((world, meter, chaser, beside))
}

/// CE-10 (plan `2026-11-19-crowd-edge-steering-2` D-2): a chaser whose walk is spent because
/// both its sides closed, the side it kept closed by a friend holding beside it, stays spent for
/// 40 ticks while that friend stands; then the friend is ordered away across the chaser's way,
/// and the walk re-arms on the tick the side frees a whole step, and not before. Under the timed
/// retry it re-armed 24 ticks after it was spent, whatever the friend did.
#[test]
fn a_spent_walk_re_arms_when_its_closed_side_opens() -> TestResult {
    const SPEED: f64 = 0.126;
    let (mut world, mut meter, chaser, beside) = boxed_chaser()?;
    while !world.entities[&chaser].movement.flank_spent {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        assert!(world.current_tick < 100, "the walk was never spent");
    }
    let spent_at = world.current_tick;
    let memory = world.entities[&chaser].movement;
    assert!(
        memory.flank_reversed,
        "spent by its two closed sides: {memory:?}"
    );
    // The walk's sides are across the way to the pillar (+x): `flank_left` is +y. The friend
    // below the chaser (lower y) closes the right side, the friend above it the left.
    let (kept, other) = if memory.flank_left {
        (beside[1], beside[0])
    } else {
        (beside[0], beside[1])
    };
    while world.current_tick < spent_at + 40 {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        assert!(
            world.entities[&chaser].movement.flank_spent,
            "re-armed at tick {} while its closed side stands closed: {:?}",
            world.current_tick, world.entities[&chaser].movement
        );
    }
    let away = world.entities[&kept].position.y - world.entities[&chaser].position.y;
    let target = at(35.05, if away > SimScalar::ZERO { 44.0 } else { 20.0 });
    // Each tick decides from the positions it starts with: (tick, gap at its start).
    let mut gaps = Vec::new();
    let mut commands = order(&[kept], UnitOrder::Move { target });
    let mut rearmed_at = None;
    while world.current_tick < spent_at + 140 && rearmed_at.is_none() {
        gaps.push((world.current_tick + 1, edge_gap(&world, chaser, kept)));
        step(&mut world, &mut meter, std::mem::take(&mut commands));
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        if !world.entities[&chaser].movement.flank_spent {
            rearmed_at = Some(world.current_tick);
        }
    }
    // A whole step free: the side keeps all but `STANDING_DRIFT` of a step of `SPEED` (the
    // friend leaves straight across the chaser's way, so the edge gap is the room on that side).
    let opened_at = gaps
        .iter()
        .find(|(_, gap)| *gap >= SPEED - STANDING_DRIFT)
        .map(|(tick, _)| *tick);
    let report = format!(
        "spent at {spent_at}, re-armed at {rearmed_at:?}, side whole from {opened_at:?}; (tick, \
         gap at its start) to the kept side's friend {gaps:?}; other friend {other:?}; {:?}",
        world.entities[&chaser].movement
    );
    assert!(opened_at.is_some(), "{report}");
    assert_eq!(rearmed_at, opened_at, "{report}");
    assert!(
        world.entities[&chaser].movement.flank_walked > SimScalar::ZERO,
        "the re-armed walk walks: {report}"
    );
    Ok(())
}

/// CE-11 (plan `2026-11-19-crowd-edge-steering-2` D-2.3, a guard): six short-ranged units in
/// single file attack-move along the corridor onto the pillar that plugs it. The rear units' walks
/// are spent against the corridor's walls and the units ahead, and the friends behind press them,
/// so contact eases each off the wall and its friends by slivers. Over 400 ticks, a spent walk
/// re-arms only on a tick its unit gains ground: a sliver of room is not a side that opens. Under
/// a re-arm on any admissible across step above the waiting share (prototype K3) the queue
/// re-armed eight times without gaining ground.
#[test]
fn contact_creep_does_not_re_arm_a_walk() -> TestResult {
    let mut world = corridor()?;
    let file: Vec<EntityId> = (0..6)
        .map(|i| spawn(&mut world, SHORT, BLUE, at(30.0 - 0.8 * i as f32, 32.0)))
        .collect::<Result<_, _>>()?;
    let mut meter = BlockingMeter::new();
    let attack = UnitOrder::AttackMove {
        target: at(55.0, 32.0),
    };
    step(&mut world, &mut meter, order(&file, attack));
    let mut before: Vec<_> = file.iter().map(|u| world.entities[u].movement).collect();
    let (mut spent, mut on_ground, mut on_side) = (0, Vec::new(), Vec::new());
    while world.current_tick < 400 {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        for (unit, before) in file.iter().zip(&mut before) {
            let now = world.entities[unit].movement;
            spent += u32::from(!before.flank_spent && now.flank_spent);
            if before.flank_spent && !now.flank_spent {
                let gained = now.progress_best < before.progress_best
                    || now.flank_target != before.flank_target;
                let event = (world.current_tick, *unit);
                if gained {
                    on_ground.push(event);
                } else {
                    on_side.push(event);
                }
            }
            *before = now;
        }
    }
    let report = format!(
        "walks spent {spent}; re-armed on ground gained {on_ground:?}; without it {on_side:?}; \
         {:?}",
        meter.of_owner(BLUE)
    );
    assert!(spent > 0, "the corridor must spend walks: {report}");
    assert_eq!(on_side, [], "{report}");
    Ok(())
}

/// The walkers layout of `second_line_tests.rs` (its test 4): a line of five long-ranged shooters
/// fires at the pillar, and three rows of four short-ranged walkers behind it attack-move past.
/// Returns the world after the order's tick, the meter and the twelve walkers in row order.
fn walkers() -> Result<(SimWorld, BlockingMeter, Vec<EntityId>), Box<dyn std::error::Error>> {
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
    let attack = UnitOrder::AttackMove {
        target: at(60.0, 32.0),
    };
    step(&mut world, &mut meter, order(&army, attack));
    Ok((world, meter, walkers))
}

/// `v` in world units as an `f64` pair, for measurement.
fn pair(v: SimVec2) -> (f64, f64) {
    (f64::from(v.x.to_f32()), f64::from(v.y.to_f32()))
}

/// The part of `unit`'s proposed step this tick along the way from its position to the
/// pillar's centre, world units per tick: negative for a step back from the pillar.
fn along_to_pillar(world: &SimWorld, unit: EntityId) -> f64 {
    let step = pair(world.proposed_step(unit).unwrap_or(SimVec2::ZERO));
    let pos = pair(world.entities[&unit].position);
    let to = (40.0 - pos.0, 32.0 - pos.1);
    (step.0 * to.0 + step.1 * to.1) / to.0.hypot(to.1)
}

/// What one walker of [`walkers`] did over the first `ticks` ticks.
struct WalkerRun {
    /// Ticks on which a walk step was followed by a forward step reversing it
    /// ([`is_reversal`] of the two proposed steps).
    walk_then_forward: Vec<u32>,
    /// The part along the way to the pillar of the walker's first walk step, and its tick.
    first_walk_step: Option<(u32, f64)>,
    /// The first tick the walker is in range of the pillar (edge gap at most 0.5).
    in_range_at: Option<u32>,
}

/// Plays [`walkers`] for `ticks` ticks and records walker `index` (0 to 11, row order).
fn walker_run(index: usize, ticks: u32) -> Result<WalkerRun, Box<dyn std::error::Error>> {
    let (mut world, mut meter, walkers) = walkers()?;
    let unit = walkers[index];
    let mut run = WalkerRun {
        walk_then_forward: Vec::new(),
        first_walk_step: None,
        in_range_at: None,
    };
    let mut before: Option<((f64, f64), bool)> = None;
    while world.current_tick < ticks {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        let proposed = world.proposed_step(unit).unwrap_or(SimVec2::ZERO);
        let walking = world.entities[&unit].movement.flank_active;
        let now = pair(proposed);
        if let Some((earlier, walked)) = before {
            if walked && !walking && is_reversal(earlier, now) {
                run.walk_then_forward.push(world.current_tick);
            }
        }
        if walking && proposed != SimVec2::ZERO && run.first_walk_step.is_none() {
            run.first_walk_step = Some((world.current_tick, along_to_pillar(&world, unit)));
        }
        if run.in_range_at.is_none() && edge_to_pillar(&world, unit) <= 0.5 + 1e-4 {
            run.in_range_at = Some(world.current_tick);
        }
        before = Some((now, walking));
    }
    Ok(run)
}

/// CE-12 (plan `2026-11-19-crowd-edge-steering-2` D-1): walker 7 of the walkers layout (entity
/// 13, second row) starts its walk with a step back round the body ahead on its side (the
/// geometry of the showcase's pulse bot 4, first plan §2.3). Under the walk-end test alone, that
/// retreat freed the straight step, the walk ended and the walker stepped straight back into
/// the same contact: walk step then forward step reversed on ticks 91 and 94. A walk now ends
/// only on room beyond its retreat, so over 150 ticks no walk step is reversed by the forward
/// step after it, while the retreat itself is still taken (T1 kept).
#[test]
fn a_walk_does_not_end_on_the_room_its_retreat_made() -> TestResult {
    let run = walker_run(6, 150)?;
    let report = format!(
        "walk then forward reversed on {:?}; first walk step (tick, along) {:?}",
        run.walk_then_forward, run.first_walk_step
    );
    let (_, along) = run.first_walk_step.ok_or_else(|| report.clone())?;
    assert!(along < 0.0, "the retreat is still taken: {report}");
    assert!(run.walk_then_forward.is_empty(), "{report}");
    Ok(())
}

/// CE-13 (D-1, a guard for the first plan's trace T1): walker 6 of the walkers layout (entity 12)
/// is the unit whose way in leads out of a pocket between two firing friends by a step back; the
/// withdrawn rule that forbade the retreat left it pinned there. Its first walk step still goes
/// back, and it reaches range by tick 300.
#[test]
fn a_walk_out_of_a_pocket_still_reaches_range() -> TestResult {
    let run = walker_run(5, 300)?;
    let report = format!(
        "first walk step (tick, along) {:?}; in range at {:?}",
        run.first_walk_step, run.in_range_at
    );
    let (_, along) = run.first_walk_step.ok_or_else(|| report.clone())?;
    assert!(along < 0.0, "{report}");
    assert!(run.in_range_at.is_some(), "{report}");
    Ok(())
}

/// The arc reproduction's proposed steps over ticks 1 to 62, folded, on the tree before D-1
/// (plan `2026-11-19-crowd-edge-steering-2` WP2, `7fdc3f8d`).
const ARC_STEPS_BEFORE_ANY_RETREAT_COUNTS: u64 = 0xab96_d520_e2d6_74a3;

/// CE-14 (D-1, as the plan's evidence restates it): a walk with no retreat ends as before. In
/// the arc reproduction of `second_line_tests.rs` (ten long-ranged shooters in two rows
/// attack-move past the pillar), every proposed step of ticks 1 to 62 is what it was before
/// D-1, bit for bit: until then no walk that would end carries a retreat. On tick 63 the first
/// does (unit 8, whose walk stepped back on ticks 56 to 58 while the arc formed, the first
/// plan's trace T4), so its walk goes on where it used to end; the arc still forms by tick 96
/// with no reversal and no give-up.
#[test]
fn a_walk_without_retreat_ends_as_before() -> TestResult {
    let mut world = scratch_world(GameMap::new(
        64,
        64,
        MapCell::default_ground(CliffLevel(0)),
    )?)?;
    spawn(&mut world, PILLAR, RED, at(40.0, 32.0))?;
    let mut line = Vec::new();
    for row in 0..2 {
        for file in 0..5 {
            let x = 25.5 - 0.8 * row as f32;
            let y = 30.4 + 0.8 * file as f32 + 0.4 * row as f32;
            line.push(spawn(&mut world, SHOOTER, BLUE, at(x, y))?);
        }
    }
    let mut meter = BlockingMeter::new();
    let attack = UnitOrder::AttackMove {
        target: at(60.0, 32.0),
    };
    step(&mut world, &mut meter, order(&line, attack));
    let mut fold = 0_u64;
    let mut formed_at = None;
    while world.current_tick < 300 {
        step(&mut world, &mut meter, Vec::new());
        assert_eq!(world.diagnostics(), [], "tick {}", world.current_tick);
        if world.current_tick <= 62 {
            for unit in &line {
                let proposed = world.proposed_step(*unit).unwrap_or(SimVec2::ZERO);
                for raw in [proposed.x.raw(), proposed.y.raw()] {
                    fold = fold
                        .wrapping_mul(0x0100_0000_01B3)
                        .wrapping_add(u64::from_ne_bytes(raw.to_ne_bytes()));
                }
            }
        }
        if formed_at.is_none() && attackers_in_range(&world, BLUE) == (10, 10) {
            formed_at = Some(world.current_tick);
        }
    }
    let blue = meter.of_owner(BLUE);
    let report = format!("steps folded {fold:#x}; arc formed at {formed_at:?}; {blue:?}");
    assert_eq!(fold, ARC_STEPS_BEFORE_ANY_RETREAT_COUNTS, "{report}");
    assert!(formed_at.is_some_and(|tick| tick <= 96), "{report}");
    assert_eq!((blue.reversals, blue.stall_giveups), (0, 0), "{report}");
    Ok(())
}
