//! Diagonal cliffs in the running world: no ground body stands beyond a cut cell's foot (plan
//! `docs/plans/2026-12-02-diagonal-cliff-walkability.md` §6.2; bug B-251; owner question Q0139).
//!
//! # Responsibility
//! These tests play `SimWorld::step` on the shipped showcase canyon and on the diagonal fixture
//! of `sim_map`'s test support, and read every body's centre against the cuts that support finds
//! with its independent statement of the rule:
//! - **T-G2** (`a_unit_ordered_to_a_step_corner_stops_behind_the_foot`): a ground unit of each
//!   distinct catalog terrain radius ordered into a cut cell's outer corner (an eighth of a cell
//!   inside `C`) never stands in the outer triangle and ends at least its terrain radius behind
//!   the foot if it ends in the cut cell.
//! - **T-G3** (`crowds_pressed_on_a_diagonal_stay_behind_it`): twenty of the smallest bodies
//!   attack-moving along a diagonal in a crowd stay, every tick, at least their terrain radius
//!   less the contact rule's rounding allowance behind every foot of a cut cell they stand in.
//! - **T-G4** (`spawns_and_blinks_avoid_the_outer_triangle`): a Blink aimed at `C` or at the
//!   deepest point the whole-cell rule admits, a death spawn there, and production egress towards
//!   a rally inside the corner place no body beyond the foot.
//!
//! All three need the clip (WP2) and are `#[ignore]`d until it lands; their red outputs on `main`
//! are in the plan's evidence file, §2.
//!
//! Units are chosen from the shipped catalog by property (mover, shape, speed, radii, what a
//! structure trains), never by name (charter §3.3).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    reason = "a broken law must fail the test loudly"
)]

#[path = "../../sim_map/tests/support/diagonal.rs"]
mod diagonal;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

use sim_core::SimWorld;
use sim_map::{DiagonalCut, GameMap, MapFile};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, DeathSpawnSpec, EntityId, MoverType,
    PlayerCommand, PlayerId, PlayerSlot, RallyTarget, ShapeKind, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder, UnitTypeId,
};

use diagonal::{
    at_least_behind, cell_centre, diagonal_fixture, in_cell, in_outer_triangle, independent_cuts,
    low_corner, raw_point, usable,
};

type TestResult = Result<(), Box<dyn Error>>;

/// The contact rule's rounding allowance (`WALL_CONTACT_TOLERANCE`, 2⁻²⁰ world units): how much
/// nearer a wall than its terrain radius a pressed body may stand (the bound
/// `wall_contact_tests.rs` uses).
const TOLERANCE: SimScalar = SimScalar::from_raw(1 << 12);

const ME: PlayerId = PlayerId(0);

/// The shipped showcase canyon, through the real map loader.
fn showcase_canyon() -> Result<GameMap, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/maps/showcase_canyon.scmap.ron");
    Ok(MapFile::load(&path)?.decode()?.to_game_map()?)
}

/// Every mobile ground archetype's terrain radius, with the lowest archetype id that has it.
fn ground_bodies(world: &SimWorld) -> BTreeMap<SimScalar, UnitTypeId> {
    let mut bodies = BTreeMap::new();
    for a in world.catalog.archetypes() {
        if matches!(a.shape, ShapeKind::UnitSphere)
            && a.mover_type != MoverType::Air
            && a.max_speed > SimScalar::ZERO
        {
            let slot = bodies.entry(a.terrain_radius).or_insert(a.unit_type_id);
            if a.unit_type_id < *slot {
                *slot = a.unit_type_id;
            }
        }
    }
    bodies
}

/// The terrain radius of `kind`.
fn terrain_radius(world: &SimWorld, kind: UnitTypeId) -> SimScalar {
    world
        .catalog
        .get(kind)
        .expect("invariant: a registered archetype")
        .terrain_radius
}

/// The cuts of `map`, by cell.
fn cuts_by_cell(map: &GameMap) -> BTreeMap<(u32, u32), DiagonalCut> {
    independent_cuts(map)
        .into_iter()
        .map(|c| (c.cell, c))
        .collect()
}

/// Every way the body of terrain radius `r` at `p` breaks the clip: standing in an outer
/// triangle, or in a cut cell closer than `r − slack` behind its foot.
fn violation(
    cuts: &BTreeMap<(u32, u32), DiagonalCut>,
    p: SimVec2,
    r: SimScalar,
    slack: SimScalar,
) -> Option<String> {
    let cell = (
        u32::try_from(p.x.raw() >> 32).ok()?,
        u32::try_from(p.y.raw() >> 32).ok()?,
    );
    let cut = cuts.get(&cell)?;
    if in_outer_triangle(cut, p) {
        return Some(format!("in the outer triangle of {:?}", cut.cell));
    }
    (in_cell(cut, p) && !at_least_behind(cut, p, r - slack))
        .then(|| format!("closer than r behind the foot of {:?}", cut.cell))
}

/// One tick of `world` with `commands`.
fn step_with(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands,
        state_checksum: None,
    });
}

/// `order` from player 0 to `entities`, on the next tick.
fn command(world: &mut SimWorld, entities: Vec<EntityId>, order: UnitOrder) {
    step_with(
        world,
        vec![PlayerCommand {
            player_id: ME,
            entities,
            order,
            queue: false,
        }],
    );
}

/// A usable cell of level `cut.la` that is no cut cell, `steps` cells behind `cut` along its
/// diagonal (towards `O`).
fn cell_behind(map: &GameMap, cut: &DiagonalCut, steps: i64) -> Option<(u32, u32)> {
    let x = i64::from(cut.cell.0) - steps * i64::from(cut.dir.0);
    let y = i64::from(cut.cell.1) - steps * i64::from(cut.dir.1);
    let (ux, uy) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
    let level = map.cell(ux, uy).ok()?.cliff_level.0;
    (usable(map, x, y)
        && i64::from(level) == i64::from(cut.la)
        && diagonal::independent_cut_of(map, ux, uy).is_none())
    .then_some((ux, uy))
}

/// The deepest point the whole-cell rule admits for a body of terrain radius `r` in cut cell
/// `cut`: `r` from both of its cliff edges, `C − r·(dx, dy)`.
fn deepest_point(cut: &DiagonalCut, r: SimScalar) -> SimVec2 {
    let c = low_corner(cut);
    raw_point(
        c.x.raw() - i64::from(cut.dir.0) * r.raw(),
        c.y.raw() - i64::from(cut.dir.1) * r.raw(),
    )
}

// -------------------------------------------------------------------------------------------------
// T-G2
// -------------------------------------------------------------------------------------------------

/// The goal of T-G2's order: an eighth of a cell inside cut `cut`'s outer corner `C`, along its
/// diagonal, so in `T(X)`. `C` itself is also the corner of the diagonal low cell, whose centre a
/// move to `C` is clamped to (`Pathfinder::nearest_reachable_point` takes the goal's cell), so an
/// order to `C` exactly would test the low side instead of the corner.
fn corner_goal(cut: &DiagonalCut) -> SimVec2 {
    let c = low_corner(cut);
    let eighth = 1_i64 << 29;
    raw_point(
        c.x.raw() - i64::from(cut.dir.0) * eighth,
        c.y.raw() - i64::from(cut.dir.1) * eighth,
    )
}

#[test]
#[ignore = "diagonal-walk WP2 (B-251): red until the mesh walls each cut's foot"]
fn a_unit_ordered_to_a_step_corner_stops_behind_the_foot() -> TestResult {
    let map = showcase_canyon()?;
    let cuts = cuts_by_cell(&map);
    let (cut, start) = cuts
        .values()
        .find_map(|cut| cell_behind(&map, cut, 3).map(|start| (*cut, start)))
        .ok_or("premise: a cut of the canyon has a cell three steps behind it")?;
    let bodies = ground_bodies(&sim_fixtures::world(map.clone(), 0xD1A6));
    let mut failures = Vec::new();
    for (&r, &kind) in &bodies {
        let mut world = sim_fixtures::world(map.clone(), 0xD1A6);
        world.vision_aware_pathfinding = false;
        let id = world.spawn_unit(kind, ME, cell_centre(start.0, start.1))?;
        command(
            &mut world,
            vec![id],
            UnitOrder::Move {
                target: corner_goal(&cut),
            },
        );
        for tick in 0..300 {
            step_with(&mut world, Vec::new());
            let p = world.entities[&id].position;
            if let Some(why) = violation(&cuts, p, SimScalar::ZERO, SimScalar::ZERO) {
                failures.push(format!("r {}: tick {tick} at {p:?}: {why}", r.to_f32()));
                break;
            }
        }
        let end = world.entities[&id].position;
        if let Some(why) = violation(&cuts, end, r, SimScalar::ZERO) {
            failures.push(format!("r {}: ends at {end:?}: {why}", r.to_f32()));
        }
    }
    println!(
        "{} radii ordered to the corner of {:?}",
        bodies.len(),
        cut.cell
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

// -------------------------------------------------------------------------------------------------
// T-G3
// -------------------------------------------------------------------------------------------------

#[test]
#[ignore = "diagonal-walk WP2 (B-251): red until the mesh walls each cut's foot"]
fn crowds_pressed_on_a_diagonal_stay_behind_it() -> TestResult {
    // The fixture's feature (1): a level-1 staircase x + y <= 11 in x, y 1..=8, whose cut cells
    // lie on x + y = 11 with their foot on x + y = 12. The crowd walks along the line
    // x + y = 11 (cell centres a diagonal half cell behind the feet) from its north-west end to
    // its south-east end.
    let map = diagonal_fixture();
    let cuts = cuts_by_cell(&map);
    assert!(
        cuts.get(&(5, 6)).is_some_and(|c| c.dir == (1, 1)),
        "premise: the fixture's staircase (1) is cut"
    );
    let mut world = sim_fixtures::world(map, 0xC0_4D);
    world.vision_aware_pathfinding = false;
    let (&r, &kind) = ground_bodies(&world)
        .iter()
        .next()
        .ok_or("premise: the catalog has ground bodies")?;
    let mut crowd = Vec::new();
    for k in 0..20_i64 {
        // Four columns of five, 0.8 apart, from (1.5, 3.5) to (3.9, 6.7): x + y <= 10.6.
        let (col, row) = (k % 4, k / 4);
        let p = raw_point(
            (1_i64 << 32) + col * (4 << 30) * 4 / 5 + (1 << 31),
            (3_i64 << 32) + row * (4 << 30) * 4 / 5 + (1 << 31),
        );
        crowd.push(world.spawn_unit(kind, ME, p)?);
    }
    command(
        &mut world,
        crowd.clone(),
        UnitOrder::AttackMove {
            target: cell_centre(8, 2),
        },
    );
    let mut failures = Vec::new();
    for tick in 0..400 {
        step_with(&mut world, Vec::new());
        for &id in &crowd {
            let p = world.entities[&id].position;
            if let Some(why) = violation(&cuts, p, r, TOLERANCE) {
                failures.push(format!("tick {tick}: {id:?} at {p:?}: {why}"));
            }
        }
    }
    println!(
        "{} bodies of terrain radius {}: {} violations",
        crowd.len(),
        r.to_f32(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} violations, e.g.:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(())
}

// -------------------------------------------------------------------------------------------------
// T-G4
// -------------------------------------------------------------------------------------------------

/// A Blink of range 8 for archetype `kind` of `world`.
fn author_blink(world: &mut SimWorld, kind: UnitTypeId) -> TestResult {
    let range = SimScalar::from_num(8);
    world.catalog.get_mut(kind)?.abilities = vec![AbilitySpec {
        name: "Blink".to_owned(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 0,
        cast_range: range,
        effect: AbilityEffectSpec::BlinkTeleport {
            max_range: range,
            cooldown_ticks: 0,
        },
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    }];
    Ok(())
}

/// Every living body of `world` that breaks the clip, as text.
fn bodies_beyond(world: &SimWorld, cuts: &BTreeMap<(u32, u32), DiagonalCut>) -> Vec<String> {
    world
        .entities
        .values()
        .filter(|e| e.is_alive())
        .filter_map(|e| {
            let r = terrain_radius(world, e.unit_type_id);
            violation(cuts, e.position, r, SimScalar::ZERO)
                .map(|why| format!("{:?} at {:?}: {why}", e.id, e.position))
        })
        .collect()
}

/// A producer and the mobile ground non-harvester it trains, both from the shipped catalog.
fn producer_and_product(world: &SimWorld) -> Option<(UnitTypeId, UnitTypeId, (u32, u32))> {
    let mut found = Vec::new();
    for producer in world.catalog.archetypes() {
        let ShapeKind::BuildingDome {
            footprint_w,
            footprint_h,
            ..
        } = producer.shape
        else {
            continue;
        };
        for slug in &producer.trains_units {
            if let Some(product) = world.catalog.get_by_slug(slug).filter(|p| {
                matches!(p.shape, ShapeKind::UnitSphere)
                    && p.mover_type == MoverType::Ground
                    && p.capabilities.harvester.is_none()
            }) {
                found.push((
                    producer.unit_type_id,
                    product.unit_type_id,
                    (u32::from(footprint_w), u32::from(footprint_h)),
                ));
            }
        }
    }
    found.sort();
    found.into_iter().next()
}

/// A site for a structure of footprint `size` beside cut cell `cut`: every footprint cell of
/// `cut`'s level, no cut cell, within one cell of `cut`, and the map admits the building.
fn site_beside(map: &GameMap, cut: &DiagonalCut, size: (u32, u32)) -> Option<(u32, u32)> {
    let (x, y) = (i64::from(cut.cell.0), i64::from(cut.cell.1));
    let (w, h) = (i64::from(size.0), i64::from(size.1));
    for oy in y - h..=y + 1 {
        for ox in x - w..=x + 1 {
            let covers = |i: i64, j: i64| (ox..ox + w).contains(&i) && (oy..oy + h).contains(&j);
            let near = (ox - 1..=ox + w).contains(&x) && (oy - 1..=oy + h).contains(&y);
            if covers(x, y) || !near {
                continue;
            }
            let fits = (oy..oy + h).all(|j| {
                (ox..ox + w).all(|i| {
                    u32::try_from(i)
                        .ok()
                        .zip(u32::try_from(j).ok())
                        .is_some_and(|(ui, uj)| {
                            map.cell(ui, uj)
                                .is_ok_and(|c| i64::from(c.cliff_level.0) == i64::from(cut.la))
                                && diagonal::independent_cut_of(map, ui, uj).is_none()
                        })
                })
            });
            let (Ok(ux), Ok(uy)) = (u32::try_from(ox), u32::try_from(oy)) else {
                continue;
            };
            let (Ok(uw), Ok(uh)) = (u8::try_from(w), u8::try_from(h)) else {
                continue;
            };
            if fits && map.can_place_building(ux, uy, uw, uh).is_ok() {
                return Some((ux, uy));
            }
        }
    }
    None
}

#[test]
#[ignore = "diagonal-walk WP2 (B-251): red until placement clips the outer triangle"]
fn spawns_and_blinks_avoid_the_outer_triangle() -> TestResult {
    let map = diagonal_fixture();
    let cuts = cuts_by_cell(&map);
    let (cut, start) = cuts
        .values()
        .find_map(|cut| cell_behind(&map, cut, 2).map(|start| (*cut, start)))
        .ok_or("premise: a cut of the fixture has a cell two steps behind it")?;
    let mut failures = Vec::new();
    let probe = sim_fixtures::world(map.clone(), 0xB1_4C);
    let (&r, &kind) = ground_bodies(&probe)
        .iter()
        .next()
        .ok_or("premise: the catalog has ground bodies")?;

    // Blink, aimed at C and at the deepest point the whole-cell rule admits.
    for target in [low_corner(&cut), deepest_point(&cut, r)] {
        let mut world = sim_fixtures::world(map.clone(), 0xB1_4C);
        author_blink(&mut world, kind)?;
        let id = world.spawn_unit(kind, ME, cell_centre(start.0, start.1))?;
        let landed = world.execute_ability(id, 0, Some(target), None);
        for why in bodies_beyond(&world, &cuts) {
            failures.push(format!("Blink at {target:?} (landed {landed}): {why}"));
        }
    }

    // A death spawn at the deepest point: the host dies there and its units are placed.
    let mut world = sim_fixtures::world(map.clone(), 0xDE_A7);
    world.catalog.get_mut(kind)?.capabilities.death_spawn = Some(DeathSpawnSpec {
        unit: kind,
        count: 3,
        delay_ticks: 0,
        search_range: SimScalar::from_num(2),
        timed_life: None,
        disabled_while_constructing: false,
    });
    let host = world.spawn_unit(kind, ME, cell_centre(start.0, start.1))?;
    world
        .entities
        .get_mut(&host)
        .ok_or("the host exists")?
        .position = deepest_point(&cut, r);
    world.entities.get_mut(&host).ok_or("the host exists")?.hp = SimScalar::ZERO;
    for _ in 0..3 {
        step_with(&mut world, Vec::new());
        for why in bodies_beyond(&world, &cuts) {
            failures.push(format!("death spawn: {why}"));
        }
    }

    // Production egress beside the cut, rallied into the corner.
    let mut world = sim_fixtures::world(map.clone(), 0xE6_E5);
    world.vision_aware_pathfinding = false;
    let (producer_type, product, size) =
        producer_and_product(&world).ok_or("premise: a producer of a ground soldier")?;
    let site = site_beside(&map, &cut, size).ok_or("premise: a producer site beside the cut")?;
    {
        let arch = world.catalog.get_mut(product)?;
        arch.build_time_ticks = 1;
        arch.mineral_cost = 0;
        arch.gas_cost = 0;
        arch.supply_cost = 0;
    }
    let bank = &mut world.player_resources[PlayerSlot::ALL[0]];
    bank.minerals = 100_000;
    bank.gas = 100_000;
    let producer = world.spawn_structure(producer_type, ME, site.0, site.1)?;
    command(
        &mut world,
        vec![producer],
        UnitOrder::SetRally {
            target: RallyTarget::Point(corner_goal(&cut)),
            slot: None,
        },
    );
    for _ in 0..3 {
        command(
            &mut world,
            vec![producer],
            UnitOrder::Produce { unit_type: product },
        );
    }
    let mut made = 0_usize;
    for tick in 0..200 {
        step_with(&mut world, Vec::new());
        for why in bodies_beyond(&world, &cuts) {
            failures.push(format!("egress tick {tick}: {why}"));
        }
        made = world
            .entities
            .values()
            .filter(|e| e.unit_type_id == product && e.is_alive())
            .count();
    }
    println!(
        "cut {:?}, radius {}, producer at {site:?}: {made} units made",
        cut.cell,
        r.to_f32()
    );
    assert!(made > 0, "premise: the producer made units");
    failures.dedup();
    assert!(
        failures.is_empty(),
        "{} violations, e.g.:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(())
}
