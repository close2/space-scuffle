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
//! WP4 (B-101): a corner waypoint keeps the unit's own radius plus the corner margin from the
//! corner, whatever its radius tier.
//!
//! Plan `2026-11-29-wall-clearance` WP0: the scenes of its cases A (a body pushed off its leg, so
//! that the leg cuts a corner ahead; B-241) and D (a body resting on a face at rounding depth
//! beside its arc; B-240), checked against the route-clearance meter. The plan's tests of WP1 and
//! WP2 on these scenes and three more are ignored until its window W opens (plan §5.2).
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
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, PathRequest, Pathfinder};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};
use std::error::Error;

/// Radius 0.375 in world units: the smallest infantry body.
const SMALL_RADIUS: SimScalar = SimScalar::from_raw(3 << 29);

type TestResult = Result<(), Box<dyn Error>>;

/// The scratch walker: radius 0.375, 0.126 world units per tick, instant acceleration.
const WALKER: &str = "route_walker";
/// The wide scratch walker: radius 0.5, otherwise the same as [`WALKER`].
const WIDE: &str = "route_wide";
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
    let units = format!(
        "(units: [{}, {}])",
        scratch_unit(WALKER, 1, 0.375),
        scratch_unit(WIDE, 2, 0.5)
    );
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
                radius: SMALL_RADIUS,
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

/// WP4 (B-101): a unit's corner waypoint keeps its own radius plus the 0.05 corner margin from
/// the corner it turns around: 0.55 for a body of radius 0.5, 0.425 for one of 0.375, though the
/// two route on the same radius tier.
#[test]
fn corner_waypoints_keep_each_units_own_radius_from_the_corner() -> TestResult {
    let mut world = corner_world()?;
    let goal = at(COLUMN_GOAL.0, COLUMN_GOAL.1);
    let vertex = (16.0, 16.0);
    for (slug, expected) in [(WIDE, 0.55), (WALKER, 0.425)] {
        let kind = world.catalog.id_by_slug(slug).ok_or("scratch unit")?;
        let unit = world.spawn_unit(kind, BLUE, at(10.5, 19.5))?;
        order(&mut world, &[unit], UnitOrder::Move { target: goal });
        let corner = point(*route(&world, unit).first().ok_or("a corner")?);
        let gap = (corner.0 - vertex.0).hypot(corner.1 - vertex.1);
        assert!(
            (gap - expected).abs() < 1e-4,
            "{slug}: corner {corner:?} is {gap} from the vertex, expected {expected}"
        );
    }
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// WP9 (B-102): routes come from the navigation mesh at the unit's own radius, so a body of
/// radius 0.5 rounding a corner keeps its whole disc clear of it: on every tick of the walk its
/// centre stays at least its radius from the corner vertex, and it arrives.
#[test]
fn a_wide_unit_keeps_its_body_clear_of_the_corner_it_rounds() -> TestResult {
    let mut world = corner_world()?;
    let goal = at(COLUMN_GOAL.0, COLUMN_GOAL.1);
    let kind = world.catalog.id_by_slug(WIDE).ok_or("scratch unit")?;
    let unit = world.spawn_unit(kind, BLUE, at(10.5, 19.5))?;
    order(&mut world, &[unit], UnitOrder::Move { target: goal });
    let mut closest = f64::INFINITY;
    for _ in 0..200 {
        let p = point(world.entities[&unit].position);
        closest = closest.min((p.0 - 16.0).hypot(p.1 - 16.0));
        tick(&mut world, Vec::new());
    }
    assert!(
        closest >= 0.5 - 1e-3,
        "the body came {closest} from the corner"
    );
    let end = point(world.entities[&unit].position);
    assert!(
        (end.0 - COLUMN_GOAL.0).hypot(end.1 - COLUMN_GOAL.1) < 0.1,
        "arrived at {end:?}"
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

// Scenes of plan `2026-11-29-wall-clearance` (WP0; evidence `2026-11-29-wall-grazing` cases A and
// D). WP0 commits the scenes with checks of their own geometry only, read by the route-clearance
// meter (`sim_fixtures::trace::route_clearance`); the route-following assertions on them are the
// red tests of WP1 and WP2, which stay on the plan's branch until its window.

/// Raw fixed-point depth of the case D scene, 2⁻²² world units: below the contact rule's 2⁻²⁰ and
/// the tangent tolerance 2⁻¹⁴, so the body only rests on the wall, at rounding depth.
const ROUNDING_DEPTH: SimScalar = SimScalar::from_raw(1 << 10);

/// Case D's scene (B-240): a walker whose route is the arc round the corner (16, 16) of
/// [`corner_world`] and then [`COLUMN_GOAL`], resting on the face y = 16 just before the corner,
/// its disc [`ROUNDING_DEPTH`] inside the face. Returns the walker and the route it holds.
fn rounding_depth_scene(world: &mut SimWorld) -> Result<(EntityId, Vec<SimVec2>), Box<dyn Error>> {
    let goal = at(COLUMN_GOAL.0, COLUMN_GOAL.1);
    let resting = SimVec2::new(
        SimScalar::from_num(15.8),
        SimScalar::from_num(16) + SMALL_RADIUS - ROUNDING_DEPTH,
    );
    let (unit, route) = routed_then_moved(world, at(10.5, 19.5), resting, goal, None)?;
    world
        .entities
        .get_mut(&unit)
        .ok_or("unit")?
        .pos_at_tick_start = resting;
    Ok((unit, route))
}

/// A 32×32 map with a solid block, cells x 16–19 and y 16–31: its south-west corner at (16, 16)
/// is convex, and open ground lies south of it and west of it.
fn block_world() -> Result<SimWorld, Box<dyn Error>> {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    for y in 16..32 {
        for x in 16..20 {
            map.cell_mut(x, y)?.flags = CellFlags::empty();
        }
    }
    world_on(map)
}

/// Orders a walker from `from` to `goal`, then places it at `to` as a push would (its position
/// at the tick's start too). Returns the walker and the route it holds.
fn ordered_then_placed(
    world: &mut SimWorld,
    from: SimVec2,
    to: SimVec2,
    goal: SimVec2,
) -> Result<(EntityId, Vec<SimVec2>), Box<dyn Error>> {
    let unit = spawn(world, from)?;
    order(world, &[unit], UnitOrder::Move { target: goal });
    let held = route(world, unit);
    let entity = world.entities.get_mut(&unit).ok_or("unit")?;
    entity.position = to;
    entity.pos_at_tick_start = to;
    Ok((unit, held))
}

/// The goal of case A's scene, east of the block, on the walker's straight route.
const PUSHED_GOAL: (f64, f64) = (28.0, 15.0);

/// Case A's scene (B-241): on [`block_world`], a walker ordered from (4, 15) to [`PUSHED_GOAL`]
/// holds the straight route along y = 15, a unit clear of the block. A push moves it north to
/// (8, 16.4), west of the block: the leg from there to the goal passes 0.16 from the corner
/// (16, 16), inside its radius, though its centre line stays south of the block. Returns the
/// walker and the route it holds.
fn pushed_off_leg_scene(world: &mut SimWorld) -> Result<(EntityId, Vec<SimVec2>), Box<dyn Error>> {
    let goal = at(PUSHED_GOAL.0, PUSHED_GOAL.1);
    ordered_then_placed(world, at(4.0, 15.0), at(8.0, 16.4), goal)
}

/// The scenes are what they claim, as the route-clearance meter reads them: the case D walker
/// touches the face (M-1) at a depth below the tangent tolerance, beside the arc it holds; the
/// case A walker holds a straight route whose leg from where the push left it cuts the corner
/// (M-5a), from open ground (no wall within the margin).
#[test]
fn the_wall_clearance_scenes_are_what_they_claim() -> TestResult {
    use sim_fixtures::trace::route_clearance::{ClearanceMeter, WALL_PROBE_SLACK};
    let mut world = corner_world()?;
    let (unit, held) = rounding_depth_scene(&mut world)?;
    assert!(held.len() >= 3, "an arc and the goal: {held:?}");
    let mut meter = ClearanceMeter::new(Vec::new());
    let now = world.current_tick;
    meter.observe(&mut world, now);
    let report = meter.finish();
    let (sample, _) = report.updates.first().ok_or("the held route is seen")?;
    assert_eq!(sample.entity, unit.0);
    let clearance = sample.clearance.ok_or("the face is near")?;
    assert!(
        clearance < 0.0 && -clearance < WALL_PROBE_SLACK,
        "{clearance}"
    );

    let mut world = block_world()?;
    let (unit, held) = pushed_off_leg_scene(&mut world)?;
    assert_eq!(held, [at(PUSHED_GOAL.0, PUSHED_GOAL.1)], "a straight route");
    let mut meter = ClearanceMeter::new(Vec::new());
    let now = world.current_tick;
    meter.observe(&mut world, now);
    let report = meter.finish();
    let (sample, update) = report.updates.first().ok_or("the held route is seen")?;
    assert_eq!(sample.entity, unit.0);
    assert!(
        update.cutting,
        "the leg from the pushed body cuts the corner"
    );
    // No wall within the meter's reach counts as clear (`None`).
    assert!(
        sample.clearance.is_none_or(|c| c > 0.05),
        "{:?}",
        sample.clearance
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

// Tests of plan `2026-11-29-wall-clearance` WP1 (B-240) and WP2 (B-241). All but two fail on the
// plan's base; the two that pass already (the narrow gap and the fresh-route property) guard what
// WP2 must keep. All are ignored until their package opens in window W (plan §5.2), so no gate runs
// them before; the package removes its tests' `ignore`, makes them pass, and records each red run
// in the evidence (`docs/evidence/2026-11-29-wall-clearance.md`).

/// The step rule's slack, 2⁻¹⁴ world units (`movement::tuning::WALL_PROBE_SLACK`, not exported).
const PROBE_SLACK: SimScalar = SimScalar::from_raw(1 << 18);

/// Whether the step rule lets a ground body of terrain radius `radius` walk from `from` to `to`,
/// as steering's `probe_permits` asks it (contact plan D-3, R-3), from the public predicates: the
/// centre rule, then the disc at `radius` along the leg, else the disc of its clearance at `from`
/// less [`PROBE_SLACK`]. A centre on the wall keeps the centre rule alone.
fn step_rule_permits(world: &mut SimWorld, radius: SimScalar, from: SimVec2, to: SimVec2) -> bool {
    use sim_map::PathRequestWithVision;
    let request = |r: SimScalar| PathRequestWithVision {
        start: from,
        goal: from,
        radius: r,
        vision: None,
        block_unexplored_ramps: false,
        known_structures: None,
    };
    if !SimWorld::is_position_pathable(&world.map, from, to) {
        return false;
    }
    if from == to
        || world
            .pathfinder
            .segment_is_passable(&world.map, &request(radius), from, to)
    {
        return true;
    }
    let allowed = world
        .pathfinder
        .wall_contact(&world.map, &request(radius), from)
        .map_or(radius, |contact| {
            let root = contact.distance_sq.max(0).unsigned_abs().isqrt();
            SimScalar::from_raw(i64::try_from(root).unwrap_or(i64::MAX)).min(radius)
        });
    let swept = allowed.saturating_sub(PROBE_SLACK);
    swept <= SimScalar::ZERO
        || world
            .pathfinder
            .segment_is_passable(&world.map, &request(swept), from, to)
}

/// The step `unit` took in the tick just played, world units.
fn achieved_step(world: &SimWorld, unit: EntityId) -> (f64, f64) {
    let entity = &world.entities[&unit];
    let (now, before) = (point(entity.position), point(entity.pos_at_tick_start));
    (now.0 - before.0, now.1 - before.1)
}

/// WP1 red (case D, B-240): a walker resting on the face at rounding depth beside the corner
/// keeps its arc: after the tick, its next aim is a point of the arc whose leg the step rule lets
/// it walk, not the goal through the corner, and over the next 40 ticks no step reverses the one
/// before.
#[test]
#[ignore = "red until plan 2026-11-29-wall-clearance WP1 (B-240), window W"]
fn a_body_resting_on_a_wall_at_rounding_depth_keeps_the_arc_round_the_corner() -> TestResult {
    use sim_fixtures::trace::blocking::is_reversal;
    let mut world = corner_world()?;
    let (unit, held) = rounding_depth_scene(&mut world)?;
    let goal = at(COLUMN_GOAL.0, COLUMN_GOAL.1);
    tick(&mut world, Vec::new());
    let now = route(&world, unit);
    let next = *now.first().ok_or("a route")?;
    assert!(
        next != goal && held.contains(&next),
        "the arc {held:?} was handed off: route {now:?}"
    );
    let position = world.entities[&unit].position;
    assert!(
        step_rule_permits(&mut world, SMALL_RADIUS, position, next),
        "the leg from {position:?} to {next:?} cuts the corner"
    );
    let mut last = achieved_step(&world, unit);
    for _ in 0..40 {
        tick(&mut world, Vec::new());
        let step = achieved_step(&world, unit);
        assert!(
            !is_reversal(last, step),
            "tick {}: {last:?} then {step:?}",
            world.current_tick
        );
        last = step;
    }
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// WP1 red (B-240, plan D-1): a walker really wedged in the face beside the corner, its disc 0.1
/// inside it, hands its arc off only along a leg the disc that fits where it stands can walk.
#[test]
#[ignore = "red until plan 2026-11-29-wall-clearance WP1 (B-240), window W"]
fn a_wedged_body_hands_off_only_along_a_leg_its_fitting_disc_can_walk() -> TestResult {
    let mut world = corner_world()?;
    let goal = at(COLUMN_GOAL.0, COLUMN_GOAL.1);
    let wedged = at(15.8, 16.275);
    let (unit, _) = routed_then_moved(&mut world, at(10.5, 19.5), wedged, goal, None)?;
    world
        .entities
        .get_mut(&unit)
        .ok_or("unit")?
        .pos_at_tick_start = wedged;
    for _ in 0..10 {
        let before = world.entities[&unit].position;
        tick(&mut world, Vec::new());
        let position = world.entities[&unit].position;
        if let Some(&next) = route(&world, unit).first() {
            assert!(
                step_rule_permits(&mut world, SMALL_RADIUS, position, next),
                "tick {}: from {position:?} (was {before:?}) the leg to {next:?} goes deeper",
                world.current_tick
            );
        }
    }
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// The route-check period of the movement tuning (`ROUTE_CHECK_PERIOD`, not exported), ticks.
const ROUTE_CHECK_PERIOD: u32 = 8;

/// WP2 red (case A, B-241): a walker pushed off its leg so that the leg cuts the corner ahead
/// drops it within [`ROUTE_CHECK_PERIOD`] ticks, takes a route that rounds the corner, never
/// brings its disc onto the corner's vertex, and arrives.
#[test]
#[ignore = "red until plan 2026-11-29-wall-clearance WP2 (B-241), window W"]
fn a_body_pushed_off_its_leg_replans_before_it_reaches_the_corner() -> TestResult {
    let mut world = block_world()?;
    let (unit, held) = pushed_off_leg_scene(&mut world)?;
    let goal = point(at(PUSHED_GOAL.0, PUSHED_GOAL.1));
    let vertex = (16.0, 16.0);
    let rounding = 0.375 + 0.05 + 1e-3;
    let (mut dropped, mut rounds) = (None, false);
    let mut closest = f64::INFINITY;
    for elapsed in 1..=300_u32 {
        tick(&mut world, Vec::new());
        let now = route(&world, unit);
        if dropped.is_none() && now != held {
            dropped = Some(elapsed);
        }
        rounds |= now.iter().any(|&p| {
            let p = point(p);
            (p.0 - vertex.0).hypot(p.1 - vertex.1) <= rounding
        });
        let p = point(world.entities[&unit].position);
        closest = closest.min((p.0 - vertex.0).hypot(p.1 - vertex.1));
    }
    let dropped = dropped.ok_or("the cutting route is never dropped")?;
    assert!(
        dropped <= ROUTE_CHECK_PERIOD,
        "dropped after {dropped} ticks"
    );
    assert!(rounds, "no route after the push rounds the corner");
    assert!(
        closest >= 0.375 - 1.0 / 16_384.0,
        "the body came {closest} from the corner's vertex"
    );
    let end = point(world.entities[&unit].position);
    assert!(
        (end.0 - goal.0).hypot(end.1 - goal.1) < 0.5,
        "ended at {end:?}"
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// WP2 red (B-241, plan D-2 item 2): a walker resting on the corner's vertex, whose straight leg
/// to the goal its disc cannot walk though the leg's centre line passes, has its step refused
/// and drops that route at once.
#[test]
#[ignore = "red until plan 2026-11-29-wall-clearance WP2 (B-241), window W"]
fn a_refused_step_whose_leg_the_disc_cannot_walk_drops_the_route() -> TestResult {
    let mut world = block_world()?;
    let goal = at(PUSHED_GOAL.0, PUSHED_GOAL.1);
    let touching = at(15.625, 15.98);
    let (unit, held) = ordered_then_placed(&mut world, at(4.0, 15.0), touching, goal)?;
    assert_eq!(held, [goal], "a straight route");
    assert!(
        !step_rule_permits(&mut world, SMALL_RADIUS, touching, goal),
        "the premise: the disc cannot walk the leg"
    );
    tick(&mut world, Vec::new());
    let now = route(&world, unit);
    assert!(now != held, "the cutting route {held:?} is kept");
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// A 32×32 map split by a one-cell wall at x = 16, with a one-cell gap at y = 8: the gap is 1
/// world unit wide.
fn gap_world() -> Result<SimWorld, Box<dyn Error>> {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    for y in (0..32).filter(|&y| y != 8) {
        map.cell_mut(16, y)?.flags = CellFlags::empty();
    }
    world_on(map)
}

/// WP2 (B-241, the documented intent of `route_is_usable`): a walker pushed into a narrow gap,
/// its disc 0.075 inside the gap's south side, keeps a route it can follow: on every tick it
/// holds a route to its goal, and it walks out of the gap and arrives.
#[test]
#[ignore = "WP2 test of plan 2026-11-29-wall-clearance (B-241), window W; passes on the base"]
fn a_body_pushed_into_a_narrow_gap_keeps_a_route_it_can_follow() -> TestResult {
    let mut world = gap_world()?;
    let goal = at(24.5, 8.5);
    let pushed = at(16.5, 8.3);
    let (unit, held) = ordered_then_placed(&mut world, at(12.5, 8.5), pushed, goal)?;
    assert_eq!(held, [goal], "a straight route through the gap");
    let mut arrived = None;
    for elapsed in 1..=200_u32 {
        tick(&mut world, Vec::new());
        let p = point(world.entities[&unit].position);
        if (p.0 - 24.5).hypot(p.1 - 8.5) < 0.1 {
            arrived = Some(elapsed);
            break;
        }
        let now = route(&world, unit);
        assert_eq!(
            now.last(),
            Some(&goal),
            "tick {}: route {now:?} at {p:?}",
            world.current_tick
        );
    }
    let arrived = arrived.ok_or("the walker does not arrive")?;
    // 8 units at 0.126 a tick is 64 ticks; the rest is the way out of the gap.
    assert!(arrived <= 90, "arrived after {arrived} ticks");
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// WP2 red (plan D-2 item 5): a freshly searched route's first leg passes the held-leg test from
/// its start. Starts are drawn by a seeded generator beside the walls of the showcase canyon map
/// (both showcase scenarios play on it) and of this file's corner and block maps, tangent to the
/// nearest wall, at rounding depth inside it, and 0.05 and 0.1 inside it; goals anywhere a centre
/// may stand. For bodies of radius 0.375 and 0.5, the first leg of every route the pathfinder
/// returns passes [`step_rule_permits`] from its start.
#[test]
#[ignore = "WP2 test of plan 2026-11-29-wall-clearance (B-241), window W; passes on the base"]
fn a_fresh_route_passes_its_own_held_leg_test() -> TestResult {
    use rand::{Rng, SeedableRng};
    use sim_fixtures::trace::route::{start_battle, RouteTraceArgs};
    let args = RouteTraceArgs::parse(
        ["--scenario", "scenarios/showcase.ron", "--out", "unused"].map(str::to_owned),
    )?;
    let mut worlds = vec![start_battle(&args)?, corner_world()?, block_world()?];
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0x0011_EF7E);
    let depths = [
        SimScalar::ZERO,
        ROUNDING_DEPTH,
        SimScalar::from_num(0.05),
        SimScalar::from_num(0.1),
    ];
    let (mut searched, mut failures) = (0_u32, Vec::new());
    for world in &mut worlds {
        let (w, h) = (world.map.width(), world.map.height());
        let draw = |rng: &mut rand_chacha::ChaCha8Rng| {
            at(
                rng.gen_range::<f64, _>(1.0..f64::from(w) - 1.0),
                rng.gen_range::<f64, _>(1.0..f64::from(h) - 1.0),
            )
        };
        for case in 0..400_u32 {
            let radius = if case % 2 == 0 {
                SMALL_RADIUS
            } else {
                SimScalar::from_num(0.5)
            };
            let depth = depths[usize::try_from(case / 2 % 4)?];
            let (near, goal) = (draw(&mut rng), draw(&mut rng));
            let Some(start) = beside_wall(world, near, radius, depth) else {
                continue;
            };
            if !SimWorld::is_position_pathable(&world.map, goal, goal) {
                continue;
            }
            let request = PathRequest {
                start,
                goal,
                radius,
            };
            let Ok(path) = world.pathfinder.find_path(&world.map, &request) else {
                continue;
            };
            let Some(&first) = path.waypoints.iter().find(|&&p| p != start) else {
                continue;
            };
            searched += 1;
            if !step_rule_permits(world, radius, start, first) {
                failures.push((radius, depth, start, first));
            }
        }
    }
    assert!(searched >= 300, "only {searched} routes searched");
    assert!(
        failures.is_empty(),
        "{} of {searched} fresh first legs fail the held-leg test, e.g. {:?}",
        failures.len(),
        &failures[..failures.len().min(5)]
    );
    Ok(())
}

/// The point `depth` inside the wall nearest `near` (within 3 world units) for a body of terrain
/// radius `radius`: on the line from the wall's nearest point to `near`, `radius − depth` from the
/// wall. `None` when no wall is near, `near` lies on the wall, or the point is not walkable.
fn beside_wall(
    world: &mut SimWorld,
    near: SimVec2,
    radius: SimScalar,
    depth: SimScalar,
) -> Option<SimVec2> {
    use sim_map::PathRequestWithVision;
    if !SimWorld::is_position_pathable(&world.map, near, near) {
        return None;
    }
    let request = PathRequestWithVision {
        start: near,
        goal: near,
        radius: radius + SimScalar::from_num(3),
        vision: None,
        block_unexplored_ramps: false,
        known_structures: None,
    };
    let contact = world.pathfinder.wall_contact(&world.map, &request, near)?;
    let (wall, away) = (point(contact.point), point(near));
    let (dx, dy) = (away.0 - wall.0, away.1 - wall.1);
    let length = dx.hypot(dy);
    if length <= 0.0 {
        return None;
    }
    let offset = f64::from(radius.to_f32()) - f64::from(depth.to_f32());
    let start = SimVec2::new(
        contact.point.x + SimScalar::from_num(dx / length * offset),
        contact.point.y + SimScalar::from_num(dy / length * offset),
    );
    SimWorld::is_position_pathable(&world.map, start, start).then_some(start)
}
