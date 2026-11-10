//! End-to-end contact and arrival behaviour, driven only through `PlayerCommand`.
//!
//! # Responsibility
//! Plays the movement situations a player sees: units meeting head-on, crossing, queuing
//! through chokes and over ramps, giving short orders, and clicking somewhere unreachable.
//! Each test asserts the observable result, never an internal steering weight.
//!
//! # How it fits
//! This suite covers remediation Phase 4 (M-1 to M-18). Reproductions r03, r04, r07 and r09 from
//! the plan's Appendix A appear here unchanged in setup. Lower-level arithmetic and legacy
//! boundary behaviour live in `checked_movement_tests.rs`.
//!
//! # Invariants
//! - Units are chosen by capability from the shipped catalog; no archetype is named here.
//! - Every test ends by asserting `world.diagnostics()` is empty, so a run that reports
//!   corruption cannot pass by also happening to end in the right place.

use sim_core::movement::RouteOutcomeKind;
use sim_core::SimWorld;
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, MapFile, RampDir};
use sim_types::{
    EntityId, MoverType, PlayerCommand, PlayerId, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SEED: u64 = 7;
const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn flat(side: u32) -> Result<GameMap, Box<dyn std::error::Error>> {
    Ok(GameMap::new(
        side,
        side,
        MapCell::default_ground(CliffLevel(0)),
    )?)
}

fn canyon() -> Result<GameMap, Box<dyn std::error::Error>> {
    Ok(MapFile::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/maps/showcase_canyon.scmap.ron"),
    )?
    .decode()?
    .to_game_map()?)
}

/// A map split by a solid wall at column `wall_x`, open only at rows in `gap`.
fn wall_with_gap(
    side: u32,
    wall_x: u32,
    gap: std::ops::Range<u32>,
) -> Result<GameMap, Box<dyn std::error::Error>> {
    let mut map = flat(side)?;
    for y in 0..side {
        if !gap.contains(&y) {
            map.cell_mut(wall_x, y)?.flags = CellFlags::empty();
        }
    }
    Ok(map)
}

/// The appendix map: a wall with no gap at all.
fn walled(side: u32) -> Result<GameMap, Box<dyn std::error::Error>> {
    wall_with_gap(side, side / 2, 0..0)
}

/// A mobile ground combat unit whose collision radius satisfies `fits`.
fn ground_unit(world: &SimWorld, fits: impl Fn(SimScalar) -> bool) -> Option<UnitTypeId> {
    world
        .catalog
        .archetypes()
        .filter(|a| {
            a.mover_type == MoverType::Ground
                && !a.is_immobile_by_default()
                && !matches!(a.shape, ShapeKind::BuildingDome { .. })
                && a.max_speed > SimScalar::ZERO
                && a.weapon.is_some()
                && fits(a.collision_radius)
        })
        .min_by_key(|a| (a.collision_radius, a.unit_type_id))
        .map(|a| a.unit_type_id)
}

/// The appendix's standard infantry body: the smallest mobile armed ground unit.
fn small_unit(world: &SimWorld) -> Result<UnitTypeId, Box<dyn std::error::Error>> {
    ground_unit(world, |_| true).ok_or_else(|| "catalog has no ground unit".into())
}

fn spawn(
    world: &mut SimWorld,
    kind: UnitTypeId,
    owner: PlayerId,
    position: SimVec2,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    Ok(world.spawn_unit(kind, owner, position)?)
}

fn tick(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

fn order(world: &mut SimWorld, player: PlayerId, entities: &[EntityId], order: UnitOrder) {
    tick(
        world,
        vec![PlayerCommand {
            player_id: player,
            entities: entities.to_vec(),
            order,
            queue: false,
        }],
    );
}

fn run(world: &mut SimWorld, ticks: u32) {
    for _ in 0..ticks {
        tick(world, Vec::new());
    }
}

fn distance(world: &SimWorld, a: EntityId, b: EntityId) -> f64 {
    let delta = world.entities[&a].position - world.entities[&b].position;
    f64::from(delta.x.to_f32()).hypot(f64::from(delta.y.to_f32()))
}

fn distance_to(world: &SimWorld, a: EntityId, point: SimVec2) -> f64 {
    let delta = world.entities[&a].position - point;
    f64::from(delta.x.to_f32()).hypot(f64::from(delta.y.to_f32()))
}

fn radius_of(world: &SimWorld, id: EntityId) -> Result<f64, Box<dyn std::error::Error>> {
    let kind = world.entities[&id].unit_type_id;
    Ok(f64::from(
        world.catalog.get(kind)?.collision_radius.to_f32(),
    ))
}

fn busy(world: &SimWorld, ids: &[EntityId]) -> usize {
    ids.iter()
        .filter(|id| !world.entities[id].order_queue.is_empty())
        .count()
}

/// Runs until no unit in `ids` holds an order, failing with the leftover count at `limit`.
fn run_until_settled(world: &mut SimWorld, ids: &[EntityId], limit: u32) -> Result<u32, String> {
    for elapsed in 0..limit {
        assert!(world.diagnostics().is_empty(), "{:?}", world.diagnostics());
        if busy(world, ids) == 0 {
            return Ok(elapsed);
        }
        tick(world, Vec::new());
    }
    Err(format!(
        "{}/{} units still hold an order after {limit} ticks",
        busy(world, ids),
        ids.len()
    ))
}

fn grid_of(
    world: &mut SimWorld,
    kind: UnitTypeId,
    owner: PlayerId,
    count: u32,
    columns: u32,
    origin: (f64, f64),
    spacing: f64,
) -> Result<Vec<EntityId>, Box<dyn std::error::Error>> {
    (0..count)
        .map(|index| {
            let position = at(
                origin.0 + f64::from(index % columns) * spacing,
                origin.1 + f64::from(index / columns) * spacing,
            );
            spawn(world, kind, owner, position)
        })
        .collect()
}

/// Appendix A r03 (M-1), extended: they must also finish by passing each other.
#[test]
fn head_on_friendlies_keep_contact_and_pass() -> TestResult {
    let mut world = sim_fixtures::world(flat(64)?, SEED);
    let kind = small_unit(&world)?;
    let a = spawn(&mut world, kind, BLUE, at(20.0, 30.0))?;
    let b = spawn(&mut world, kind, BLUE, at(40.0, 30.0))?;
    let contact = radius_of(&world, a)? + radius_of(&world, b)?;
    order(
        &mut world,
        BLUE,
        &[a],
        UnitOrder::Move {
            target: at(40.0, 30.0),
        },
    );
    order(
        &mut world,
        BLUE,
        &[b],
        UnitOrder::Move {
            target: at(20.0, 30.0),
        },
    );
    let mut closest = f64::MAX;
    for _ in 0..500 {
        run(&mut world, 1);
        closest = closest.min(distance(&world, a, b));
    }
    assert!(
        closest > contact * 0.8,
        "contact {contact}, closest {closest}"
    );
    assert!(distance_to(&world, a, at(40.0, 30.0)) < 0.5);
    assert!(distance_to(&world, b, at(20.0, 30.0)) < 0.5);
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// M-16: the mirror-exact case must separate, whichever unit has the lower identity.
#[test]
fn exact_head_on_tie_separates_to_opposite_world_sides() -> TestResult {
    for swapped in [false, true] {
        let mut world = sim_fixtures::world(flat(64)?, SEED);
        let kind = small_unit(&world)?;
        let (west, east) = (at(25.0, 30.0), at(35.0, 30.0));
        let first = spawn(&mut world, kind, BLUE, if swapped { east } else { west })?;
        let second = spawn(&mut world, kind, BLUE, if swapped { west } else { east })?;
        let (a, b) = if swapped {
            (second, first)
        } else {
            (first, second)
        };
        tick(
            &mut world,
            vec![
                PlayerCommand {
                    player_id: BLUE,
                    entities: vec![a],
                    order: UnitOrder::Move { target: east },
                    queue: false,
                },
                PlayerCommand {
                    player_id: BLUE,
                    entities: vec![b],
                    order: UnitOrder::Move { target: west },
                    queue: false,
                },
            ],
        );
        let mut widest = 0.0_f32;
        for _ in 0..200 {
            run(&mut world, 1);
            let gap = world.entities[&a].position.y - world.entities[&b].position.y;
            widest = widest.max(gap.to_f32().abs());
        }
        assert!(widest > 0.5, "units never sidestepped: {widest}");
        assert_eq!(world.diagnostics(), []);
    }
    Ok(())
}

#[test]
fn crossing_streams_do_not_interpenetrate_and_all_arrive() -> TestResult {
    let mut world = sim_fixtures::world(flat(64)?, SEED);
    let kind = small_unit(&world)?;
    let east = grid_of(&mut world, kind, BLUE, 6, 2, (10.0, 28.0), 1.5)?;
    let north = grid_of(&mut world, kind, BLUE, 6, 3, (26.0, 10.0), 1.5)?;
    tick(
        &mut world,
        vec![
            PlayerCommand {
                player_id: BLUE,
                entities: east.clone(),
                order: UnitOrder::Move {
                    target: at(48.0, 30.0),
                },
                queue: false,
            },
            PlayerCommand {
                player_id: BLUE,
                entities: north.clone(),
                order: UnitOrder::Move {
                    target: at(27.0, 50.0),
                },
                queue: false,
            },
        ],
    );
    let all: Vec<EntityId> = east.iter().chain(&north).copied().collect();
    let contact = 2.0 * radius_of(&world, east[0])?;
    let mut closest = f64::MAX;
    for _ in 0..800 {
        run(&mut world, 1);
        for (index, &a) in all.iter().enumerate() {
            for &b in &all[index + 1..] {
                closest = closest.min(distance(&world, a, b));
            }
        }
    }
    assert!(
        closest > contact * 0.7,
        "contact {contact}, closest {closest}"
    );
    assert_eq!(busy(&world, &all), 0);
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// Appendix A r04 (M-2): stationary squadmates are not a settled group.
#[test]
fn short_order_on_a_stationary_pair_moves_them() -> TestResult {
    let mut world = sim_fixtures::world(flat(32)?, SEED);
    let kind = small_unit(&world)?;
    let a = spawn(&mut world, kind, BLUE, at(10.0, 10.0))?;
    let b = spawn(&mut world, kind, BLUE, at(10.8, 10.0))?;
    order(
        &mut world,
        BLUE,
        &[a, b],
        UnitOrder::Move {
            target: at(10.4, 11.5),
        },
    );
    run(&mut world, 100);
    let moved = [a, b]
        .iter()
        .map(|id| (world.entities[id].position.y - SimScalar::from_num(10)).to_f32())
        .fold(0.0_f32, f32::max);
    assert!(moved > 0.5, "units barely moved: {moved}");
    assert_eq!(busy(&world, &[a, b]), 0, "both must finish");
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// M-15: strangers near the goal are not the mover's group, so they cannot end its order.
#[test]
fn unrelated_idle_friendly_on_the_line_does_not_end_a_move() -> TestResult {
    let mut world = sim_fixtures::world(flat(32)?, SEED);
    let kind = small_unit(&world)?;
    let goal = at(20.0, 10.0);
    let stranger = spawn(&mut world, kind, BLUE, at(19.0, 10.0))?;
    let mover = spawn(&mut world, kind, BLUE, at(10.0, 10.0))?;
    order(&mut world, BLUE, &[mover], UnitOrder::Move { target: goal });
    run_until_settled(&mut world, &[mover], 400)?;
    assert!(
        distance_to(&world, mover, goal) < 0.2,
        "stopped {} short of the goal",
        distance_to(&world, mover, goal)
    );
    assert!(world.entities.contains_key(&stranger));
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// M-15: walking past a building next to the goal is not arriving.
#[test]
fn building_beside_the_goal_does_not_end_a_move() -> TestResult {
    let mut world = sim_fixtures::world(flat(32)?, SEED);
    let kind = small_unit(&world)?;
    let dome = world
        .catalog
        .archetypes()
        .find(|a| matches!(a.shape, ShapeKind::BuildingDome { footprint_w: 2, .. }))
        .ok_or("catalog needs a two-cell structure")?
        .unit_type_id;
    let building = world.spawn_structure(dome, BLUE, 18, 11)?;
    let goal = world.entities[&building].position + at(2.5, 0.0);
    let mover = spawn(&mut world, kind, BLUE, at(10.0, 12.0))?;
    order(&mut world, BLUE, &[mover], UnitOrder::Move { target: goal });
    run_until_settled(&mut world, &[mover], 400)?;
    assert!(
        distance_to(&world, mover, goal) < 0.2,
        "stopped {} short of the goal",
        distance_to(&world, mover, goal)
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// A group that shares a destination still settles on contact instead of crowding its centre.
#[test]
fn a_group_settles_around_a_shared_destination() -> TestResult {
    let mut world = sim_fixtures::world(flat(64)?, SEED);
    let kind = small_unit(&world)?;
    let ids = grid_of(&mut world, kind, BLUE, 40, 8, (5.0, 5.0), 1.0)?;
    let goal = at(40.0, 40.0);
    order(&mut world, BLUE, &ids, UnitOrder::Move { target: goal });
    run_until_settled(&mut world, &ids, 1500)?;
    for &id in &ids {
        assert!(
            distance_to(&world, id, goal) < 6.0,
            "{id:?} settled {} from the goal",
            distance_to(&world, id, goal)
        );
    }
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// Appendix A r07 (M-4): an enemy walking into a holder goes around it.
#[test]
fn enemy_cannot_shove_a_holding_unit() -> TestResult {
    let mut world = sim_fixtures::world(flat(32)?, SEED);
    let small = small_unit(&world)?;
    let large = ground_unit(&world, |r| r > SimScalar::from_num(0.5)).ok_or("catalog body")?;
    let holder = spawn(&mut world, small, BLUE, at(15.0, 15.0))?;
    let mover = spawn(&mut world, large, RED, at(10.0, 15.0))?;
    order(&mut world, BLUE, &[holder], UnitOrder::HoldPosition);
    order(
        &mut world,
        RED,
        &[mover],
        UnitOrder::Move {
            target: at(25.0, 15.0),
        },
    );
    let start = world.entities[&holder].position;
    let mut farthest = 0.0_f64;
    for _ in 0..300 {
        run(&mut world, 1);
        let Some(unit) = world.entities.get(&holder) else {
            break;
        };
        let delta = unit.position - start;
        farthest = farthest.max(f64::from(delta.length().to_f32()));
    }
    assert!(farthest < 0.01, "holder displaced {farthest}");
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// A hold reached through the queue anchors the unit exactly like an immediate one.
#[test]
fn queued_hold_is_not_pushed_by_friend_or_foe() -> TestResult {
    let mut world = sim_fixtures::world(flat(32)?, SEED);
    let kind = small_unit(&world)?;
    let holder = spawn(&mut world, kind, BLUE, at(12.0, 15.0))?;
    tick(
        &mut world,
        vec![
            PlayerCommand {
                player_id: BLUE,
                entities: vec![holder],
                order: UnitOrder::Move {
                    target: at(15.0, 15.0),
                },
                queue: false,
            },
            PlayerCommand {
                player_id: BLUE,
                entities: vec![holder],
                order: UnitOrder::HoldPosition,
                queue: true,
            },
        ],
    );
    run_until(&mut world, 200, |w| {
        matches!(
            w.entities[&holder].order_queue.front(),
            Some(UnitOrder::HoldPosition)
        )
    })?;
    assert!(world.entities[&holder].cannot_be_pushed());
    let anchor = world.entities[&holder].position;
    let friend = spawn(&mut world, kind, BLUE, at(11.0, 15.0))?;
    let foe = spawn(&mut world, kind, RED, at(19.0, 15.0))?;
    tick(
        &mut world,
        vec![
            PlayerCommand {
                player_id: BLUE,
                entities: vec![friend],
                order: UnitOrder::Move {
                    target: at(25.0, 15.0),
                },
                queue: false,
            },
            PlayerCommand {
                player_id: RED,
                entities: vec![foe],
                order: UnitOrder::Move {
                    target: at(5.0, 15.0),
                },
                queue: false,
            },
        ],
    );
    for _ in 0..200 {
        run(&mut world, 1);
        let Some(unit) = world.entities.get(&holder) else {
            break;
        };
        assert_eq!(unit.position, anchor);
    }
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

fn run_until(
    world: &mut SimWorld,
    limit: u32,
    done: impl Fn(&SimWorld) -> bool,
) -> Result<(), String> {
    for _ in 0..limit {
        if done(world) {
            return Ok(());
        }
        tick(world, Vec::new());
    }
    Err(format!("condition not met within {limit} ticks"))
}

/// Appendix A r09 (M-3). The units walk to the nearest reachable point and stop there: at speed
/// 0.09 that is about 545 ticks away, so the appendix's 500-tick horizon is extended.
#[test]
fn unreachable_order_reports_retargets_and_terminates() -> TestResult {
    let mut world = sim_fixtures::world(walled(64)?, SEED);
    let kind = small_unit(&world)?;
    let ids = grid_of(&mut world, kind, BLUE, 20, 5, (5.0, 5.0), 1.5)?;
    let searches_before = world.path_search_count();
    order(
        &mut world,
        BLUE,
        &ids,
        UnitOrder::Move {
            target: at(50.0, 50.0),
        },
    );
    let retargeted = world
        .route_outcomes()
        .iter()
        .filter(|o| matches!(o.kind, RouteOutcomeKind::Retargeted { .. }))
        .count();
    assert_eq!(retargeted, 20, "every unit must report the new destination");
    // Every unit settles; the bound is generous. Units that stand waiting behind the crowd at the
    // wall are not pushed through it (plan 2026-11-09-friendly-blocking D-4), so the last one
    // settles on tick 900 by the stall rule.
    run_until_settled(&mut world, &ids, 1_200)?;
    for &id in &ids {
        let x = world.entities[&id].position.x;
        assert!(x < SimScalar::from_num(32), "{id:?} crossed the wall");
        assert!(distance_to(&world, id, at(31.5, 50.5)) < 6.0);
    }
    assert!(
        world.path_search_count() - searches_before < 120,
        "searches must be bounded: {}",
        world.path_search_count() - searches_before
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// A click on impassable ground behaves like a click beside it.
#[test]
fn order_into_a_blocked_cell_stops_beside_it() -> TestResult {
    let mut world = sim_fixtures::world(walled(32)?, SEED);
    let kind = small_unit(&world)?;
    let unit = spawn(&mut world, kind, BLUE, at(10.5, 10.5))?;
    order(
        &mut world,
        BLUE,
        &[unit],
        UnitOrder::Move {
            target: at(16.5, 10.5),
        },
    );
    run_until_settled(&mut world, &[unit], 300)?;
    assert!(distance_to(&world, unit, at(15.5, 10.5)) < 0.2);
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// A unit that cannot path from where it stands, boxed in completely, ends its order.
#[test]
fn fully_enclosed_unit_ends_its_order_with_a_typed_outcome() -> TestResult {
    let mut map = flat(16)?;
    for (x, y) in [
        (4, 4),
        (5, 4),
        (6, 4),
        (4, 5),
        (6, 5),
        (4, 6),
        (5, 6),
        (6, 6),
    ] {
        map.cell_mut(x, y)?.flags = CellFlags::empty();
    }
    let mut world = sim_fixtures::world(map, SEED);
    let kind = small_unit(&world)?;
    let unit = spawn(&mut world, kind, BLUE, at(5.5, 5.5))?;
    order(
        &mut world,
        BLUE,
        &[unit],
        UnitOrder::Move {
            target: at(12.5, 12.5),
        },
    );
    assert!(world
        .route_outcomes()
        .iter()
        .any(|o| o.entity == unit && matches!(o.kind, RouteOutcomeKind::Unreachable)));
    run_until_settled(&mut world, &[unit], 5)?;
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// M-5: a corner waypoint with a parked unit on it can still be passed.
#[test]
fn route_continues_past_a_blocker_on_its_corner_waypoint() -> TestResult {
    let mut world = sim_fixtures::world(wall_with_gap(32, 16, 12..16)?, SEED);
    let kind = small_unit(&world)?;
    let unit = spawn(&mut world, kind, BLUE, at(8.5, 24.5))?;
    let goal = at(24.5, 24.5);
    order(&mut world, BLUE, &[unit], UnitOrder::Move { target: goal });
    run(&mut world, 1);
    let route = world.entities[&unit].current_funnel_waypoints.clone();
    assert!(route.len() >= 2, "goal must need a corner: {route:?}");
    let corner = route[0];
    assert_ne!(corner, goal);
    let blocker = spawn(&mut world, kind, BLUE, corner)?;
    order(&mut world, BLUE, &[blocker], UnitOrder::HoldPosition);
    run_until_settled(&mut world, &[unit], 800)?;
    assert!(
        distance_to(&world, unit, goal) < 0.2,
        "gave up {} from the goal",
        distance_to(&world, unit, goal)
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// M-6: a walled-in mover reports zero achieved velocity and eventually gives up.
#[test]
fn stuck_unit_reports_no_motion_and_abandons_its_order() -> TestResult {
    let mut world = sim_fixtures::world(wall_with_gap(32, 16, 15..17)?, SEED);
    let kind = small_unit(&world)?;
    let plug = [
        spawn(&mut world, kind, BLUE, at(16.5, 15.4))?,
        spawn(&mut world, kind, BLUE, at(16.5, 16.6))?,
        spawn(&mut world, kind, BLUE, at(15.5, 16.0))?,
    ];
    order(&mut world, BLUE, &plug, UnitOrder::HoldPosition);
    let unit = spawn(&mut world, kind, BLUE, at(13.5, 16.0))?;
    order(
        &mut world,
        BLUE,
        &[unit],
        UnitOrder::Move {
            target: at(24.5, 16.0),
        },
    );
    run(&mut world, 120);
    let before = world.entities[&unit].position;
    run(&mut world, 1);
    let moved = world.entities[&unit].position - before;
    assert_eq!(
        world.entities[&unit].velocity, moved,
        "velocity must be the displacement achieved, not the one intended"
    );
    let mut reported = false;
    for _ in 0..400 {
        tick(&mut world, Vec::new());
        reported |= world
            .route_outcomes()
            .iter()
            .any(|o| o.entity == unit && matches!(o.kind, RouteOutcomeKind::NoProgress));
    }
    assert!(reported, "the abandoned order must be reported");
    assert_eq!(busy(&world, &[unit]), 0);
    assert!(world.entities[&unit].position.x < SimScalar::from_num(17));
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// M-7: melee chases a target on a plateau up the ramp rather than along the cliff.
#[test]
fn melee_pursuit_takes_the_ramp_and_reaches_the_enemy() -> TestResult {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    for y in 16..32 {
        for x in 0..32 {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(1);
        }
    }
    for y in 13..16 {
        for x in 26..29 {
            let cell = map.cell_mut(x, y)?;
            cell.ramp = Some(RampDir::North);
            cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
        }
    }
    let mut world = sim_fixtures::world(map, SEED);
    let melee = world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == MoverType::Ground
                && a.max_speed > SimScalar::ZERO
                && !a.is_immobile_by_default()
                && a.weapon
                    .as_ref()
                    .is_some_and(|w| w.max_range <= SimScalar::ONE && w.projectile_speed.is_none())
        })
        .ok_or("catalog needs a melee unit")?
        .unit_type_id;
    let attacker = spawn(&mut world, melee, BLUE, at(6.5, 12.5))?;
    let target = spawn(&mut world, melee, RED, at(6.5, 20.5))?;
    order(&mut world, RED, &[target], UnitOrder::HoldPosition);
    run(&mut world, 2);
    let hp = world.entities[&target].hp;
    order(&mut world, BLUE, &[attacker], UnitOrder::attack(target));
    run_until(&mut world, 1200, |w| {
        w.entities.get(&target).is_none_or(|t| t.hp < hp)
    })?;
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// The leash: automatic targeting does not march a unit around the map to a stranded enemy.
#[test]
fn acquired_attack_without_a_short_route_is_dropped() -> TestResult {
    let mut map = flat(48)?;
    for x in 0..48 {
        map.cell_mut(x, 24)?.flags = CellFlags::empty();
    }
    map.cell_mut(47, 24)?.flags = CellFlags::GROUND_PATHABLE;
    let mut world = sim_fixtures::world(map, SEED);
    let kind = world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == MoverType::Ground
                && !a.is_immobile_by_default()
                && a.max_speed > SimScalar::ZERO
                && a.weapon
                    .as_ref()
                    .is_some_and(|w| w.max_range <= SimScalar::ONE && w.projectile_speed.is_none())
        })
        .ok_or("catalog needs a melee unit")?
        .unit_type_id;
    let idle = spawn(&mut world, kind, BLUE, at(5.5, 22.5))?;
    let target = spawn(&mut world, kind, RED, at(5.5, 26.5))?;
    order(&mut world, RED, &[target], UnitOrder::HoldPosition);
    run(&mut world, 40);
    assert!(
        distance_to(&world, idle, at(5.5, 22.5)) < 3.0,
        "idle unit left for a long detour"
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// An explicit attack on something unreachable ends rather than pressing against the wall.
#[test]
fn explicit_attack_on_an_unreachable_target_ends_with_a_report() -> TestResult {
    let mut world = sim_fixtures::world(walled(32)?, SEED);
    let kind = small_unit(&world)?;
    let attacker = spawn(&mut world, kind, BLUE, at(4.5, 10.5))?;
    let target = spawn(&mut world, kind, RED, at(27.5, 10.5))?;
    order(&mut world, BLUE, &[attacker], UnitOrder::attack(target));
    let mut reported = false;
    for _ in 0..600 {
        tick(&mut world, Vec::new());
        reported |= world.route_outcomes().iter().any(|o| {
            o.entity == attacker
                && matches!(o.kind, RouteOutcomeKind::PursuitAbandoned { target: t } if t == target)
        });
        if reported {
            break;
        }
    }
    assert!(reported);
    assert_eq!(busy(&world, &[attacker]), 0);
    assert!(world.entities[&attacker].position.x < SimScalar::from_num(16));
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// A two-cell gap, for each body size that fits it: the whole group gets through and finishes.
#[test]
fn group_passes_a_two_cell_choke() -> TestResult {
    for pick_large in [false, true] {
        let mut world = sim_fixtures::world(wall_with_gap(48, 24, 22..24)?, SEED);
        let kind = if pick_large {
            ground_unit(&world, |r| r >= SimScalar::from_num(0.75)).ok_or("catalog body")?
        } else {
            small_unit(&world)?
        };
        let ids = grid_of(&mut world, kind, BLUE, 16, 4, (6.0, 16.0), 2.2)?;
        let goal = at(38.0, 23.0);
        order(&mut world, BLUE, &ids, UnitOrder::Move { target: goal });
        run_until_settled(&mut world, &ids, 3000)?;
        for &id in &ids {
            assert!(
                world.entities[&id].position.x > SimScalar::from_num(24),
                "{id:?} stopped before the choke at {:?}",
                world.entities[&id].position
            );
        }
        assert_eq!(world.diagnostics(), []);
    }
    Ok(())
}

/// Routes agree with physics: a disc of radius 0.5 fits a one-cell gap.
#[test]
fn radius_half_unit_routes_through_a_one_cell_gap() -> TestResult {
    let mut world = sim_fixtures::world(wall_with_gap(32, 16, 15..16)?, SEED);
    let kind = ground_unit(&world, |r| r == SimScalar::HALF).ok_or("catalog body")?;
    let unit = spawn(&mut world, kind, BLUE, at(8.5, 8.5))?;
    order(
        &mut world,
        BLUE,
        &[unit],
        UnitOrder::Move {
            target: at(24.5, 8.5),
        },
    );
    run_until_settled(&mut world, &[unit], 800)?;
    assert!(distance_to(&world, unit, at(24.5, 8.5)) < 0.2);
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// A body wider than its surroundings is not trapped: it leaves by the only way out.
#[test]
fn large_unit_spawned_in_a_one_cell_gap_can_leave() -> TestResult {
    let mut world = sim_fixtures::world(wall_with_gap(32, 16, 15..16)?, SEED);
    let kind = ground_unit(&world, |r| r > SimScalar::from_num(0.6)).ok_or("catalog body")?;
    let unit = spawn(&mut world, kind, BLUE, at(16.5, 15.5))?;
    order(
        &mut world,
        BLUE,
        &[unit],
        UnitOrder::Move {
            target: at(8.5, 15.5),
        },
    );
    run_until_settled(&mut world, &[unit], 400)?;
    assert!(distance_to(&world, unit, at(8.5, 15.5)) < 0.2);
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// Ramps carry a whole squad, and nobody ends the order on the wrong level.
#[test]
fn squad_climbs_a_ramp_and_reaches_its_target() -> TestResult {
    let mut world = sim_fixtures::world(canyon()?, SEED);
    let kind = small_unit(&world)?;
    let source = world
        .map
        .cell_to_world_center(10, 30)
        .map_err(|e| format!("{e:?}"))?;
    let ids = (0..12)
        .map(|index| {
            let step = SimVec2::new(
                SimScalar::from_num(f64::from(index % 4)),
                SimScalar::from_num(f64::from(index / 4)),
            );
            spawn(&mut world, kind, BLUE, source + step)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let goal = world
        .map
        .cell_to_world_center(50, 30)
        .map_err(|e| format!("{e:?}"))?;
    order(&mut world, BLUE, &ids, UnitOrder::Move { target: goal });
    run_until_settled(&mut world, &ids, 4000)?;
    for &id in &ids {
        assert!(
            distance_to(&world, id, goal) < 6.0,
            "{id:?} settled {} from the goal",
            distance_to(&world, id, goal)
        );
    }
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// M-13: friendly contact must not cap a unit below its effective speed.
#[test]
fn buffed_unit_keeps_its_speed_through_friendly_contact() -> TestResult {
    let mut world = sim_fixtures::world(flat(64)?, SEED);
    let kind = small_unit(&world)?;
    // Instant acceleration, so the first tick shows the buffed speed (acceleration itself is
    // pinned in `acceleration_tests.rs`).
    let archetype = world.catalog.get_mut(kind)?;
    archetype.acceleration = archetype.max_speed;
    let fast = spawn(&mut world, kind, BLUE, at(10.0, 30.0))?;
    let beside = spawn(&mut world, kind, BLUE, at(10.0, 30.6))?;
    let base = world.catalog.get(kind)?.max_speed;
    world
        .entities
        .get_mut(&fast)
        .ok_or("spawned")?
        .active_buffs
        .push(sim_core::ActiveBuff {
            buff_id: 1,
            remaining_ticks: 500,
            speed_multiplier: SimScalar::from_num(2),
            attack_rate_multiplier: SimScalar::ONE,
        });
    let start = world.entities[&fast].position;
    tick(
        &mut world,
        vec![
            PlayerCommand {
                player_id: BLUE,
                entities: vec![fast],
                order: UnitOrder::Move {
                    target: at(40.0, 30.0),
                },
                queue: false,
            },
            PlayerCommand {
                player_id: BLUE,
                entities: vec![beside],
                order: UnitOrder::Move {
                    target: at(40.0, 30.6),
                },
                queue: false,
            },
        ],
    );
    let travelled = (world.entities[&fast].position - start).length();
    assert!(
        travelled > base * SimScalar::from_num(1.5),
        "travelled {travelled} in one tick, base speed {base}"
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// M-17: a cliff between two bodies means they are not touching.
#[test]
fn units_on_either_side_of_a_cliff_do_not_push_each_other() -> TestResult {
    let mut map = flat(32)?;
    for y in 16..32 {
        for x in 0..32 {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(1);
        }
    }
    let mut world = sim_fixtures::world(map, SEED);
    let kind = small_unit(&world)?;
    let below = spawn(&mut world, kind, BLUE, at(10.0, 15.8))?;
    let above = spawn(&mut world, kind, BLUE, at(10.0, 16.2))?;
    let (first, second) = (
        world.entities[&below].position,
        world.entities[&above].position,
    );
    run(&mut world, 20);
    assert_eq!(world.entities[&below].position, first);
    assert_eq!(world.entities[&above].position, second);
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// M-10: walking in a cell too tight for this tier keeps the route instead of rebuilding it.
#[test]
fn route_survives_cells_narrower_than_the_tier() -> TestResult {
    let mut map = flat(32)?;
    for x in 10..20 {
        map.cell_mut(x, 14)?.flags = CellFlags::empty();
        map.cell_mut(x, 16)?.flags = CellFlags::empty();
    }
    let mut world = sim_fixtures::world(map, SEED);
    let kind = ground_unit(&world, |r| r > SimScalar::from_num(0.6)).ok_or("catalog body")?;
    let unit = spawn(&mut world, kind, BLUE, at(11.5, 15.5))?;
    order(
        &mut world,
        BLUE,
        &[unit],
        UnitOrder::Move {
            target: at(25.5, 15.5),
        },
    );
    let searches = world.path_search_count();
    run(&mut world, 120);
    assert!(
        world.path_search_count() - searches <= 3,
        "route rebuilt {} times",
        world.path_search_count() - searches
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// M-9: a corner on the way to a target costs a step, not a long crawl along the wall.
#[test]
fn pursuit_rounds_a_single_blocked_cell_promptly() -> TestResult {
    let mut map = flat(24)?;
    map.cell_mut(6, 6)?.flags = CellFlags::empty();
    let mut world = sim_fixtures::world(map, SEED);
    let kind = world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == MoverType::Ground
                && !a.is_immobile_by_default()
                && a.max_speed > SimScalar::ZERO
                && a.weapon
                    .as_ref()
                    .is_some_and(|w| w.max_range <= SimScalar::ONE && w.projectile_speed.is_none())
        })
        .ok_or("catalog needs a melee unit")?
        .unit_type_id;
    let attacker = spawn(&mut world, kind, BLUE, at(5.9, 3.0))?;
    let target = spawn(&mut world, kind, RED, at(6.3, 9.5))?;
    order(&mut world, RED, &[target], UnitOrder::HoldPosition);
    run(&mut world, 1);
    let hp = world.entities[&target].hp;
    order(&mut world, BLUE, &[attacker], UnitOrder::attack(target));
    let speed = world.catalog.get(kind)?.max_speed;
    let allowed = (SimScalar::from_num(12) / speed).saturating_to_u32();
    run_until(&mut world, allowed, |w| {
        w.entities.get(&target).is_none_or(|t| t.hp < hp)
    })?;
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// M-14: a light unit walking into a heavy idle unit is the one that gives way.
#[test]
fn low_priority_mover_does_not_shove_a_high_priority_idler() -> TestResult {
    use sim_core::{compute_push_displacement, PushEntityParams};
    let mover = PushEntityParams::new(EntityId(1), at(5.0, 5.0), at(0.1, 0.0), BLUE, 1, false);
    let idler = PushEntityParams::new(EntityId(2), at(5.5, 5.0), SimVec2::ZERO, BLUE, 5, false);
    let (moved, shoved) =
        compute_push_displacement(mover, idler, SimScalar::HALF, SimScalar::from_num(0.75))?;
    assert_eq!(shoved, SimVec2::ZERO);
    assert!(moved.x < SimScalar::ZERO);
    Ok(())
}

/// M-14: two movers in contact separate along the line of centres, with no sideways wedge.
#[test]
fn moving_pair_separates_radially() -> TestResult {
    use sim_core::{compute_push_displacement, PushEntityParams};
    let a = PushEntityParams::new(EntityId(1), at(5.0, 5.0), at(0.1, 0.0), BLUE, 3, false);
    let b = PushEntityParams::new(EntityId(2), at(5.5, 5.0), at(0.1, 0.0), BLUE, 3, false);
    let (first, second) =
        compute_push_displacement(a, b, SimScalar::HALF, SimScalar::from_num(0.75))?;
    assert_eq!(first.y, SimScalar::ZERO);
    assert_eq!(second.y, SimScalar::ZERO);
    assert_eq!(first.x, -second.x);
    Ok(())
}

/// Invariant D-1.4 of plan `2026-11-09-friendly-blocking` (bug B-103), over eight layouts: units
/// ordered through a closed line of friends holding position do not vibrate. They stand and wait
/// instead of stepping into the line and being pushed back out every tick.
#[test]
fn units_blocked_by_a_holding_line_never_reverse() -> TestResult {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        f64::from(u32::try_from(state >> 40).unwrap_or(0)) / f64::from(1_u32 << 24)
    };
    for layout in 0..8 {
        let mut world = sim_fixtures::world(
            GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?,
            SEED,
        );
        let kind = small_unit(&world)?;
        let radius = world.catalog.get(kind)?.collision_radius.to_f32();
        let spacing = 2.0 * f64::from(radius);
        let line: Vec<EntityId> = (0..7)
            .map(|i| {
                spawn(
                    &mut world,
                    kind,
                    BLUE,
                    at(30.0, 28.0 + spacing * f64::from(i)),
                )
            })
            .collect::<Result<_, _>>()?;
        order(&mut world, BLUE, &line, UnitOrder::HoldPosition);
        let movers: Vec<EntityId> = (0..4)
            .map(|i| {
                let x = 26.0 + next();
                let y = 29.0 + 1.2 * f64::from(i) + 0.4 * next();
                spawn(&mut world, kind, BLUE, at(x, y))
            })
            .collect::<Result<_, _>>()?;
        order(
            &mut world,
            BLUE,
            &movers,
            UnitOrder::Move {
                target: at(40.0, 30.5),
            },
        );
        let mut meter = sim_fixtures::trace::blocking::BlockingMeter::new();
        for _ in 0..200 {
            tick(&mut world, Vec::new());
            meter.observe(&world);
        }
        // A unit nudged by a squadmate's contact correction may settle back once, by under 0.01
        // a tick; a vibrating unit reverses tick after tick.
        for &unit in &movers {
            assert!(
                meter.reversals_of(unit) <= 1,
                "layout {layout}, unit {unit:?}"
            );
        }
        assert_eq!(world.diagnostics(), []);
    }
    Ok(())
}
