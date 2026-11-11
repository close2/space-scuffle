//! # Integration Test: disc contact with terrain (plan `2026-11-11-contact-and-missiles` WP3)
//!
//! A ground body's disc never goes deeper into a wall, whether it walks or is pushed (plan D-1);
//! a move that would press it in slides along the wall or round its corner instead (D-2). These
//! tests play `SimWorld::step` on small synthetic maps and on the shipped showcase, and measure
//! each body's depth (`radius − distance to the nearest wall`, `sim_map`'s
//! `Pathfinder::wall_contact` under the physical request) after every tick:
//! - T-B1: a crowd rounding a corner never presses a body deeper;
//! - T-B2: a hostile push at 45° into a straight wall slides the body along it;
//! - T-B3: a body overlapping a wall walks past the wall's end without being thrown;
//! - T-B4: a body spawned into a wall leaves along its route, and a push does not deepen it;
//! - T-B5: the showcase's pulse bot 7 rounds the ramp E corner (bug B-113's reproduction);
//! - T-B6: walking a route's legs never needs a projection, on both shipped maps;
//! - T-B9: a body touching a wall's end walks round it without near-stalling (steering's disc
//!   probe and its slide along the wall, risk R-3).
//!
//! Units are picked from the shipped catalog by radius, never by name (charter §3.3), except
//! that T-B5 follows the entity the bug was traced on.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use sim_core::SimWorld;
use sim_fixtures::trace::route::{start_battle, total_turn_degrees, RouteTraceArgs};
use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapFile, PathRequest, PathRequestWithVision,
    Pathfinder,
};
use sim_types::{
    EntityId, MoverType, PlayerCommand, PlayerId, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitTypeId,
};

/// The rounding allowance of the rule (`WALL_CONTACT_TOLERANCE`, 2⁻²⁰ world units).
const TOLERANCE: f64 = 1.0 / 1_048_576.0;

fn s(value: f64) -> SimScalar {
    SimScalar::from_num(value)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(s(x), s(y))
}

/// A flat level-1 map of `size` × `size` with the cells for which `wall` holds made cliffs.
fn walled(size: u32, wall: impl Fn(u32, u32) -> bool) -> GameMap {
    let mut map = GameMap::new(size, size, MapCell::default_ground(CliffLevel(1)))
        .expect("invariant: a valid size");
    for y in 0..size {
        for x in 0..size {
            if wall(x, y) {
                map.cell_mut(x, y).expect("inside").flags = CellFlags::UNPASSABLE_CLIFF;
            }
        }
    }
    map
}

/// A mobile ground archetype of collision radius 0.375 (the marine's size).
fn small_body(world: &SimWorld) -> UnitTypeId {
    world
        .catalog
        .archetypes()
        .filter(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .find(|a| {
            a.mover_type == MoverType::Ground
                && a.max_speed > SimScalar::ZERO
                && a.collision_radius == s(0.375)
        })
        .map(|a| a.unit_type_id)
        .expect("invariant: the shipped catalog has a 0.375 ground unit")
}

fn spawn(world: &mut SimWorld, owner: u8, position: SimVec2) -> EntityId {
    let kind = small_body(world);
    world
        .spawn_unit(kind, PlayerId(owner), position)
        .expect("invariant: a free spot")
}

fn order(world: &mut SimWorld, owner: u8, entities: Vec<EntityId>, order: UnitOrder) {
    step_with(
        world,
        vec![PlayerCommand {
            player_id: PlayerId(owner),
            entities,
            order,
            queue: false,
        }],
    );
}

fn step_with(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands,
        state_checksum: None,
    });
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

fn step(world: &mut SimWorld) {
    step_with(world, Vec::new());
}

/// The depth of `id`'s disc into the walls, in world units.
fn depth(world: &mut SimWorld, id: EntityId) -> f64 {
    let entity = &world.entities[&id];
    let radius = world
        .catalog
        .get(entity.unit_type_id)
        .expect("registered")
        .collision_radius;
    let p = entity.position;
    let request = PathRequestWithVision {
        start: p,
        goal: p,
        radius,
        vision: None,
        block_unexplored_ramps: false,
        known_structures: None,
    };
    let distance = world
        .pathfinder
        .wall_contact(&world.map, &request, p)
        .map_or(f64::from(radius.to_f32()), |contact| {
            #[expect(clippy::cast_precision_loss, reason = "a test measure")]
            let squared = contact.distance_sq as f64;
            squared.sqrt() / 4_294_967_296.0
        });
    (f64::from(radius.to_f32()) - distance).max(0.0)
}

/// Steps `ticks` ticks and asserts that no body of `ids` goes deeper into a wall on any of them.
fn step_never_deeper(world: &mut SimWorld, ids: &[EntityId], ticks: u32, label: &str) {
    let mut before: Vec<f64> = ids.iter().map(|&id| depth(world, id)).collect();
    for _ in 0..ticks {
        step(world);
        for (index, &id) in ids.iter().enumerate() {
            let now = depth(world, id);
            assert!(
                now <= before[index] + TOLERANCE,
                "{label}: {id:?} went from depth {} to {now} on tick {} at {:?}",
                before[index],
                world.current_tick,
                world.entities[&id].position
            );
            before[index] = now;
        }
    }
}

/// T-B1: eight friends attack-move round the north corner of an L-shaped cliff. On every tick
/// no body's depth rises.
#[test]
fn a_crowd_never_presses_a_body_inside_its_radius_of_a_corner() {
    let map = walled(32, |x, y| {
        ((14..18).contains(&x) && (6..26).contains(&y))
            || ((18..26).contains(&x) && (6..10).contains(&y))
    });
    let mut world = sim_fixtures::world(map, 0xC0_4E5);
    let mut crowd = Vec::new();
    for row in 0..2_u32 {
        for column in 0..4_u32 {
            let x = 9.0 + 0.8 * f64::from(column);
            let y = 22.0 + 0.8 * f64::from(row);
            crowd.push(spawn(&mut world, 0, at(x, y)));
        }
    }
    order(
        &mut world,
        0,
        crowd.clone(),
        UnitOrder::AttackMove {
            target: at(22.0, 16.0),
        },
    );
    step_never_deeper(&mut world, &crowd, 300, "crowd");
    let arrived = crowd
        .iter()
        .filter(|id| world.entities[id].position.x > s(18.0))
        .count();
    assert!(
        arrived >= 6,
        "the crowd got round the corner: {arrived} of 8"
    );
}

/// T-B2: a hostile body overlapping a body that touches a straight wall pushes it at 45° into
/// the wall. The body stays at its radius from the wall and moves by the push's part along it.
#[test]
fn a_push_into_a_straight_wall_slides_along_it() {
    let map = walled(20, |x, _| x >= 10);
    let mut world = sim_fixtures::world(map, 0xC0_4E6);
    let radius = 0.375;
    let pushed = spawn(&mut world, 0, at(10.0 - radius, 10.0));
    let pusher = spawn(&mut world, 1, at(10.0 - radius - 0.45, 10.45));
    world
        .entities
        .get_mut(&pusher)
        .expect("spawned")
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("hold admitted");
    let start = world.entities[&pushed].position;
    step_never_deeper(&mut world, &[pushed], 1, "pushed");
    let end = world.entities[&pushed].position;
    assert!(
        end.x <= s(10.0 - radius + TOLERANCE),
        "at most touching the wall: {end:?}"
    );
    assert!(end.y < start.y, "slid south along the wall: {end:?}");
}

/// T-B3 (variant F's regression): a body overlapping a straight wall by 0.355 walks along it past
/// its end at (10, 12). It moves at most its speed a tick, and never deeper.
#[test]
fn a_body_passing_a_wall_end_is_never_thrown() {
    let map = walled(24, |x, y| x >= 10 && y < 12);
    let mut world = sim_fixtures::world(map, 0xC0_4E7);
    let body = spawn(&mut world, 0, at(9.98, 6.0));
    let kind = world.entities[&body].unit_type_id;
    let speed = world.catalog.get(kind).expect("registered").max_speed;
    order(
        &mut world,
        0,
        vec![body],
        UnitOrder::Move {
            target: at(9.98, 18.0),
        },
    );
    let mut previous = world.entities[&body].position;
    for _ in 0..80 {
        step_never_deeper(&mut world, &[body], 1, "walker");
        let now = world.entities[&body].position;
        assert!(
            now.distance(previous) <= speed + s(TOLERANCE),
            "never thrown: {previous:?} to {now:?}"
        );
        previous = now;
    }
    assert!(previous.y > s(12.0), "walked past the wall's end");
}

/// T-B4: a body spawned 0.2 into a wall is not moved out by force; pushed towards the wall by a
/// hostile walker it goes no deeper, and ordered away it leaves along its route.
#[test]
fn a_wedged_spawn_leaves_but_never_goes_deeper() {
    let map = walled(24, |x, _| x >= 10);
    let mut world = sim_fixtures::world(map, 0xC0_4E8);
    let radius = 0.375;
    let wedged = spawn(&mut world, 0, at(10.0 - radius + 0.2, 6.0));
    let walker = spawn(&mut world, 1, at(6.0, 6.0));
    assert!((depth(&mut world, wedged) - 0.2).abs() < 1e-6);
    order(
        &mut world,
        1,
        vec![walker],
        UnitOrder::Move {
            target: at(9.9, 6.0),
        },
    );
    step_never_deeper(&mut world, &[wedged], 40, "pushed while wedged");
    order(
        &mut world,
        0,
        vec![wedged],
        UnitOrder::Move {
            target: at(4.0, 12.0),
        },
    );
    step_never_deeper(&mut world, &[wedged], 60, "leaving");
    assert!(depth(&mut world, wedged) == 0.0, "left the wall");
}

/// T-B5 (bug B-113's reproduction): in the shipped showcase the crowd pushed pulse bot 7 to
/// 0.30 from the ramp E corner (35, 52), inside its radius of 0.375, and it then turned 454°
/// stepping back and forth. Played to tick 800, it stays at least its radius from the corner
/// and turns at most 180° over trace ticks 740 to 800.
#[test]
fn showcase_pulse_bot_rounds_the_ramp_e_corner() {
    let args = RouteTraceArgs::parse(
        [
            "--scenario",
            "scenarios/showcase.ron",
            "--out",
            "unused-route-trace-output",
        ]
        .map(str::to_owned),
    )
    .expect("valid arguments");
    let mut world = start_battle(&args).expect("the showcase starts");
    let bot = EntityId(7);
    let corner = at(35.0, 52.0);
    let mut track = Vec::new();
    for tick in 0..800_u32 {
        step(&mut world);
        let Some(entity) = world.entities.get(&bot) else {
            break;
        };
        let radius = world
            .catalog
            .get(entity.unit_type_id)
            .expect("registered")
            .collision_radius;
        let gap = entity.position.distance(corner);
        assert!(
            gap >= radius - s(TOLERANCE),
            "trace tick {tick}: {:?} is {gap:?} from the corner",
            entity.position
        );
        if tick >= 740 {
            track.push((
                f64::from(entity.position.x.to_f32()),
                f64::from(entity.position.y.to_f32()),
            ));
        }
    }
    // The bot must still be alive at the corner, or the test would pass by its death.
    assert_eq!(
        track.len(),
        60,
        "pulse bot 7 lives through ticks 740 to 800"
    );
    let turn = total_turn_degrees(&track);
    assert!(turn <= 180.0, "turned {turn:.0}° over ticks 740 to 800");
}

/// T-B9 (plan D-3, risk R-3; the showcase's pulse bot 18 at the ramp E corner): a body touching
/// a wall's end from below the line its route takes round it, ordered to a point behind the
/// wall, walks round the end without near-stalling. Heading straight for its next point would
/// press its disc into the corner, which the settle cuts to nothing; steering's disc probe slides
/// the step along the corner instead. Until the body is round the end its step never shrinks (it
/// accelerates from rest to its speed), it is round within 20 ticks, and it never goes deeper.
#[test]
fn a_body_heading_past_a_wall_end_walks_round_it_without_stalling() {
    // The wall is every cell west of x = 10 and south of y = 12: its end is the vertex (10, 12).
    let map = walled(24, |x, y| x < 10 && y < 12);
    let mut world = sim_fixtures::world(map, 0xC0_4E9);
    let radius = 0.375;
    // Touching the vertex, 0.072 below the line y = 12 + radius + margin the route rounds it on.
    let start = at(10.126, 12.0 + (radius * radius - 0.126 * 0.126_f64).sqrt());
    let body = spawn(&mut world, 0, start);
    let kind = world.entities[&body].unit_type_id;
    let speed = f64::from(
        world
            .catalog
            .get(kind)
            .expect("registered")
            .max_speed
            .to_f32(),
    );
    order(
        &mut world,
        0,
        vec![body],
        UnitOrder::Move {
            target: at(4.0, 13.0),
        },
    );
    let mut previous = world.entities[&body].position;
    let mut last_step = 0.0_f64;
    let mut rounded = None;
    for tick in 0..60_u32 {
        step_never_deeper(&mut world, &[body], 1, "rounding");
        let now = world.entities[&body].position;
        let moved = f64::from(now.distance(previous).to_f32());
        if rounded.is_none() {
            // The unit accelerates from rest, so its step grows to its speed; a stall shrinks it.
            assert!(
                moved >= last_step.min(speed) - 1e-3,
                "tick {tick}: stepped {moved:.4} after {last_step:.4} at {now:?}"
            );
        }
        if rounded.is_none() && now.x < s(9.5) {
            rounded = Some(tick);
        }
        previous = now;
        last_step = moved;
    }
    let rounded = rounded.expect("the body got round the wall's end");
    assert!(rounded <= 20, "round the end by tick {rounded}");
}

/// A seeded uniform draw in `[low, high)` (splitmix64), so the test needs no random crate.
fn draw(state: &mut u64, low: f64, high: f64) -> f64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    #[expect(clippy::cast_precision_loss, reason = "a test draw")]
    let unit = (z >> 11) as f64 / (1_u64 << 53) as f64;
    low + (high - low) * unit
}

/// Loads a shipped map.
fn shipped(name: &str) -> GameMap {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/maps")
        .join(name);
    MapFile::load(&path)
        .and_then(|file| file.decode())
        .and_then(|document| document.to_game_map())
        .expect("a shipped map")
}

/// T-B6 (plan D-4): on both shipped maps, a disc walking any leg of a route from a start clear
/// of walls keeps its whole radius from every wall, so following a route never
/// needs a projection. Seeded starts and goals, radii 0.375 to 1.
#[test]
fn walking_a_route_never_presses_a_disc_into_a_wall() {
    for name in ["default_arena.scmap.ron", "showcase_canyon.scmap.ron"] {
        let map = shipped(name);
        let mut pathfinder = Pathfinder::new(&map);
        let mut rng = 0x00C0_47AD_u64;
        let (width, height) = (f64::from(map.width()), f64::from(map.height()));
        let mut legs = 0;
        for _ in 0..200 {
            let start = at(
                draw(&mut rng, 1.0, width - 1.0),
                draw(&mut rng, 1.0, height - 1.0),
            );
            let goal = at(
                draw(&mut rng, 1.0, width - 1.0),
                draw(&mut rng, 1.0, height - 1.0),
            );
            let radius = s(draw(&mut rng, 0.375, 1.0));
            let Ok(route) = pathfinder.find_path(
                &map,
                &PathRequest {
                    start,
                    goal,
                    radius,
                },
            ) else {
                continue;
            };
            let clearance = |pathfinder: &mut Pathfinder, p: SimVec2| {
                let request = PathRequestWithVision {
                    start: p,
                    goal: p,
                    radius,
                    vision: None,
                    block_unexplored_ramps: false,
                    known_structures: None,
                };
                pathfinder
                    .wall_contact(&map, &request, p)
                    .map(|c| c.distance_sq)
            };
            // A wedged start's route may leave through ground narrower than the disc (the start
            // triangle rule of the pathfinding plan's D-3), which the contact rule lets it walk.
            if clearance(&mut pathfinder, start).is_some() {
                continue;
            }
            for (index, leg) in route.waypoints.windows(2).enumerate() {
                legs += 1;
                for k in 0..=20 {
                    let t = s(f64::from(k) / 20.0);
                    let p = leg[0] + (leg[1] - leg[0]) * t;
                    let radius_raw = i128::from(radius.raw());
                    let floor = radius_raw - (1 << 12);
                    if let Some(squared) = clearance(&mut pathfinder, p) {
                        assert!(
                            squared >= floor * floor,
                            "{name}: leg {index} of {:?} presses a disc of {radius:?} at {p:?}",
                            route.waypoints
                        );
                    }
                }
            }
        }
        assert!(legs >= 100, "{name}: {legs} legs walked");
    }
}
