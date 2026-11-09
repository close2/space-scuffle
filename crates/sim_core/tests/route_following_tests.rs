//! Route following at a corner: units pass corners by position and terrain view, and leave bodies
//! to steering.
//!
//! # Responsibility
//! Plan `2026-11-09-sc2-pathfinding` WP2 (D-9 A1 and A2, bug B-99): a route is what the pathfinder
//! produced, never a point invented for a parked body; a corner is passed once the unit is beyond
//! it and the next point is in terrain view, whoever stands on the next leg; a corner the unit has
//! not reached, or whose next leg terrain hides, is kept. A column of eight walks round a corner as
//! directly as plan §3.6.1 measured without the corner rewrite and the empty-leg rule.
//!
//! WP3 (D-9 A3, B-100): a unit whose corner has a body parked on it steers past the body on the
//! outside of the turn, keeps that side while it rounds the body, and never rewrites its route.
//!
//! # How it fits
//! Runs whole ticks through `SimWorld::step` with scratch archetypes written in this file, so a
//! rebalance of the shipped catalog cannot move these results. The trajectory metrics are those of
//! the route trace tool (`sim_fixtures::trace::route`).
//!
//! # Invariants
//! - Every test ends with no simulation diagnostic.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::catalog::UnitCatalog;
use sim_core::SimWorld;
use sim_fixtures::trace::route::{first_pass, RouteBox, RouteSample};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, PathRequest, Pathfinder, UnitClearance};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

/// The scratch walker: radius 0.375, 0.126 world units per tick, instant acceleration.
const WALKER: &str = "route_walker";
const BLUE: PlayerId = PlayerId(0);

/// One unarmed scratch ground unit of `radius` in the catalog's RON form.
fn scratch_unit(slug: &str, id: u16, radius: f32) -> String {
    format!(
        r#"(
            slug: "{slug}",
            unit_type_id: {id},
            faction: "automaton_directorate",
            display_name: "{slug}",
            lore: "Scratch archetype of route_following_tests.rs.",
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
            max_speed: 0.126,
            acceleration: 1.0,
            deceleration: 1.0,
            turn_rate_deg_per_tick: 180.0,
            turn_before_move: false,
            collision_radius: {radius:.3},
            separation_radius: {radius:.3},
            push_priority: 10,
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

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn point(v: SimVec2) -> (f64, f64) {
    (f64::from(v.x.to_f32()), f64::from(v.y.to_f32()))
}

/// A 32×32 map whose south-west quadrant, cells x 0–15 and y 0–15, is solid: one convex corner
/// at (16, 16). The world holds the scratch walker.
fn corner_world() -> Result<SimWorld, Box<dyn Error>> {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..16 {
        for x in 0..16 {
            map.cell_mut(x, y)?.flags = CellFlags::empty();
        }
    }
    world_on(map)
}

/// A 32×32 map split by a one-cell wall at x = 16 for y 0–15, open above it.
fn wall_world() -> Result<SimWorld, Box<dyn Error>> {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..16 {
        map.cell_mut(16, y)?.flags = CellFlags::empty();
    }
    world_on(map)
}

fn world_on(map: GameMap) -> Result<SimWorld, Box<dyn Error>> {
    let mut world = sim_fixtures::world(map, 0x5c2_9a7);
    let units = format!("(units: [{}])", scratch_unit(WALKER, 1, 0.375));
    world.catalog = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &units)?;
    Ok(world)
}

fn spawn(world: &mut SimWorld, position: SimVec2) -> Result<EntityId, Box<dyn Error>> {
    let kind = world.catalog.id_by_slug(WALKER).ok_or("walker")?;
    Ok(world.spawn_unit(kind, BLUE, position)?)
}

fn tick(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

fn order(world: &mut SimWorld, units: &[EntityId], order: UnitOrder) {
    tick(
        world,
        vec![PlayerCommand {
            player_id: BLUE,
            entities: units.to_vec(),
            order,
            queue: false,
        }],
    );
}

fn route(world: &SimWorld, unit: EntityId) -> Vec<SimVec2> {
    world.entities[&unit].current_funnel_waypoints.clone()
}

/// The route the pathfinder gives a Small-tier body from `start` to `goal` on `world`'s map.
fn searched(world: &SimWorld, start: SimVec2, goal: SimVec2) -> Option<Vec<SimVec2>> {
    let mut pathfinder = Pathfinder::new(&world.map);
    pathfinder
        .find_path(
            &world.map,
            &PathRequest {
                start,
                goal,
                clearance: UnitClearance::Small,
            },
        )
        .ok()
        .map(|path| path.waypoints)
}

/// A column of eight, two cells apart from the wall, walking east and round the corner at
/// (16, 16) to a goal south-east of it.
const COLUMN_GOAL: (f64, f64) = (24.5, 6.5);

fn column(world: &mut SimWorld) -> Result<Vec<EntityId>, Box<dyn Error>> {
    (0..8)
        .map(|index| spawn(world, at(2.0 + 0.8 * f64::from(index), 19.5)))
        .collect()
}

/// Every route a unit holds is what the pathfinder returned from where the unit stood when its
/// route was searched, or what is left of the route it held the tick before.
#[test]
fn no_route_holds_a_point_the_pathfinder_did_not_produce() -> TestResult {
    let mut world = corner_world()?;
    let units = column(&mut world)?;
    let goal = at(COLUMN_GOAL.0, COLUMN_GOAL.1);
    let mut before: Vec<(SimVec2, Vec<SimVec2>)> = units
        .iter()
        .map(|unit| (world.entities[unit].position, Vec::new()))
        .collect();
    order(&mut world, &units, UnitOrder::Move { target: goal });
    for _ in 0..400 {
        for (unit, (start, previous)) in units.iter().zip(&mut before) {
            let now = route(&world, *unit);
            let kept = previous.ends_with(&now);
            let fresh = searched(&world, *start, goal).is_some_and(|path| path.ends_with(&now));
            assert!(
                kept || fresh,
                "tick {}: unit {unit:?} holds {now:?}, neither left of {previous:?} nor searched \
                 from {start:?}",
                world.current_tick
            );
            *start = world.entities[unit].position;
            *previous = now;
        }
        tick(&mut world, Vec::new());
    }
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// Gives `unit` a route through the corner by ordering it from `from`, then moves it to `to` as a
/// push would, holding a flockmate at `blocker` when given. Returns the corner and the route.
fn routed_then_moved(
    world: &mut SimWorld,
    from: SimVec2,
    to: SimVec2,
    goal: SimVec2,
    blocker: Option<SimVec2>,
) -> Result<(EntityId, Vec<SimVec2>), Box<dyn Error>> {
    let unit = spawn(world, from)?;
    order(world, &[unit], UnitOrder::Move { target: goal });
    let first = route(world, unit);
    assert!(first.len() >= 2, "the goal needs a corner: {first:?}");
    world.entities.get_mut(&unit).ok_or("unit")?.position = to;
    if let Some(at) = blocker {
        let mate = spawn(world, at)?;
        order(world, &[mate], UnitOrder::HoldPosition);
    }
    Ok((unit, first))
}

/// A2: pushed beyond its corner with a flockmate standing on its next leg, a unit drops the
/// corner on that tick: the body is steering's problem, not the route's.
#[test]
fn a_unit_pushed_past_its_corner_drops_it_whoever_stands_on_the_next_leg() -> TestResult {
    let mut world = corner_world()?;
    let goal = at(COLUMN_GOAL.0, COLUMN_GOAL.1);
    // From (10.5, 19.5) the route turns at the corner (16.3, 16.3) towards the goal. The unit is
    // then pushed to (16.9, 15.6), beyond the corner, and a flockmate holds (17.55, 14.85) on the leg
    // from there to the goal.
    let (unit, first) = routed_then_moved(
        &mut world,
        at(10.5, 19.5),
        at(16.9, 15.6),
        goal,
        Some(at(17.55, 14.85)),
    )?;
    let corner = first[0];
    tick(&mut world, Vec::new());
    let now = route(&world, unit);
    assert!(
        !now.contains(&corner),
        "the corner {corner:?} must be dropped, route {now:?}"
    );
    assert_eq!(now.last(), Some(&goal));
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// A unit that has not reached its corner keeps it.
#[test]
fn a_unit_before_its_corner_keeps_it() -> TestResult {
    let mut world = corner_world()?;
    let goal = at(COLUMN_GOAL.0, COLUMN_GOAL.1);
    let (unit, first) = routed_then_moved(&mut world, at(10.5, 19.5), at(13.0, 18.5), goal, None)?;
    tick(&mut world, Vec::new());
    assert_eq!(route(&world, unit).first(), first.first());
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// A corner whose next point the unit cannot see across terrain is kept, even beyond it.
#[test]
fn a_corner_whose_next_leg_terrain_hides_is_kept() -> TestResult {
    let mut world = wall_world()?;
    let goal = at(24.5, 8.5);
    // From (12.5, 8.5) the route climbs to the west corner of the wall's top, crosses to its east
    // corner, and comes down. At (15.7, 15.0) the unit is beyond the west corner (its x is past
    // the corner's), but the wall stands between it and the east corner.
    let (unit, first) = routed_then_moved(&mut world, at(12.5, 8.5), at(15.7, 15.0), goal, None)?;
    assert!(first.len() >= 3, "two corners and the goal: {first:?}");
    let corner = first[0];
    assert!(SimScalar::from_num(15.7) >= corner.x, "{corner:?}");
    tick(&mut world, Vec::new());
    assert_eq!(route(&world, unit).first(), Some(&corner));
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// The column rounds the corner as directly as plan §3.6.1 measured without the rewrite and the
/// empty-leg rule (worst 1.113 and 148°), with margin: every unit's walked/chord through the
/// corner box is at most 1.15 and its total turn at most 180°.
#[test]
fn a_column_rounds_a_shared_corner_directly() -> TestResult {
    let mut world = corner_world()?;
    let units = column(&mut world)?;
    order(
        &mut world,
        &units,
        UnitOrder::Move {
            target: at(COLUMN_GOAL.0, COLUMN_GOAL.1),
        },
    );
    let mut samples = Vec::new();
    for _ in 0..500 {
        tick(&mut world, Vec::new());
        for unit in &units {
            let entity = &world.entities[unit];
            samples.push(RouteSample {
                tick: world.current_tick,
                entity: unit.0,
                owner: 0,
                unit: WALKER.to_owned(),
                position: point(entity.position),
                first_waypoint: entity.current_funnel_waypoints.first().copied().map(point),
                route_len: entity.current_funnel_waypoints.len(),
            });
        }
    }
    let area = RouteBox {
        name: "corner".into(),
        min: (13.0, 12.0),
        max: (20.0, 20.0),
    };
    let mut report = Vec::new();
    for unit in &units {
        let pass = first_pass(&samples, unit.0, &area).ok_or("every unit passes the corner")?;
        let ratio = pass.ratio().ok_or("a pass has length")?;
        report.push((unit.0, ratio, pass.total_turn_degrees));
    }
    assert!(
        report
            .iter()
            .all(|&(_, ratio, turn)| ratio <= 1.15 && turn <= 180.0),
        "(unit, walked/chord, total turn): {report:?}"
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// Stall repath period of the movement tuning (`STALL_REPATH_TICKS`), one second of 25 ticks.
const STALL_REPATH_TICKS: u32 = 25;

/// A3: with a body held exactly on its corner, a unit rounds it on the outside of the turn within
/// one stall period of reaching it, turning at most 120° in all, never swinging back to the other
/// side, and holding only the route the pathfinder gave it.
#[test]
fn a_unit_rounds_a_body_parked_on_its_corner_on_the_outside() -> TestResult {
    let mut world = corner_world()?;
    let goal = at(COLUMN_GOAL.0, COLUMN_GOAL.1);
    let unit = spawn(&mut world, at(10.5, 19.5))?;
    order(&mut world, &[unit], UnitOrder::Move { target: goal });
    let original = route(&world, unit);
    assert!(original.len() >= 2, "the goal needs a corner: {original:?}");
    let corner = original[0];
    let parked = spawn(&mut world, corner)?;
    order(&mut world, &[parked], UnitOrder::HoldPosition);
    let centre = point(world.entities[&parked].position);
    // The circle the unit rounds: both radii and the handoff margin, with a little slack.
    let reach = 0.375 + 0.375 + 0.1 + 0.05;
    let (mut contact, mut passed) = (None, None);
    let mut trajectory = Vec::new();
    let mut angles: Vec<f64> = Vec::new();
    for _ in 0..200 {
        tick(&mut world, Vec::new());
        let now = route(&world, unit);
        assert!(
            original.ends_with(&now),
            "tick {}: the route {now:?} is not what is left of {original:?}",
            world.current_tick
        );
        let position = point(world.entities[&unit].position);
        trajectory.push(position);
        let offset = (position.0 - centre.0, position.1 - centre.1);
        let near = offset.0.hypot(offset.1) <= reach;
        if near && contact.is_none() {
            contact = Some(world.current_tick);
        }
        if contact.is_some() && passed.is_none() {
            angles.push(offset.1.atan2(offset.0));
            // Passed: the corner is dropped and the unit is beyond the body along its next leg.
            let onward = (point(goal).0 - centre.0, point(goal).1 - centre.1);
            let beyond = offset.0 * onward.0 + offset.1 * onward.1 >= 0.0;
            if beyond && !now.contains(&corner) {
                passed = Some(world.current_tick);
            }
        }
    }
    let contact = contact.ok_or("the unit reaches the parked body")?;
    let passed = passed.ok_or("the unit passes the parked body")?;
    assert!(
        passed - contact <= STALL_REPATH_TICKS,
        "rounded in {} ticks",
        passed - contact
    );
    // The outside of this right turn is north-east of the body: the unit goes clockwise round
    // it, so the angle of its offset from the body only falls, give or take steering jitter.
    let mut lowest = f64::INFINITY;
    for angle in &angles {
        assert!(*angle <= lowest + 0.2, "swung back: angles {angles:?}");
        lowest = lowest.min(*angle);
    }
    let pass =
        sim_fixtures::trace::route::measure_pass(&trajectory, &vec![None; trajectory.len()], 0)
            .ok_or("a trajectory")?;
    assert!(
        pass.total_turn_degrees <= 120.0,
        "total turn {}",
        pass.total_turn_degrees
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}
