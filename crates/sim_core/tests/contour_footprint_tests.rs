//! # Integration Test: structures meet units through their 5.0.14 footprint shape (B-134, B-138)
//!
//! # Responsibility
//! Plan `2026-11-18-contour-footprints`: a structure keeps its footprint cells for placement and
//! meets ground units through its pathing shape, a box with 45° chamfered corners.
//! - C-1: the centre rule lets a unit's centre cross a cut corner of an occupied cell, and
//!   refuses a segment through the shape;
//! - C-2: a worker builds a one-cell structure (H-1, D-9: construction approaches the edge of
//!   its site, never its middle).
//!
//! # How it fits
//! Plays `SimWorld` on small synthetic maps with the shipped catalogs. Structures are scratch
//! copies of shipped structures chosen by `ShapeKind::BuildingDome` and size, given the shape a
//! test needs; bodies are scratch-registered as in `terrain_radius_tests` (charter §3.3: no
//! slug is named).
//!
//! # Invariants
//! - Every tick of `step` ends with no simulation diagnostic.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, FootprintShape, PlayerCommand, PlayerId, PlayerSlot, ShapeKind, SimScalar, SimVec2,
    Tick, TickBundle, UnitOrder, UnitTypeId,
};

const ME: PlayerId = PlayerId(0);

fn s(value: f64) -> SimScalar {
    SimScalar::from_num(value)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(s(x), s(y))
}

fn flat(size: u32) -> GameMap {
    GameMap::new(size, size, MapCell::default_ground(CliffLevel(1)))
        .expect("invariant: a valid size")
}

fn world_on(map: GameMap) -> SimWorld {
    sim_fixtures::world(map, 0xB_134)
}

fn free_id(world: &SimWorld) -> UnitTypeId {
    (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("invariant: a free unit type id")
}

/// Registers a scratch copy of the first shipped structure of footprint `size` that `keep`
/// accepts, with `cells` footprint cells and `shape`; returns its id.
fn scratch_structure(
    world: &mut SimWorld,
    size: u8,
    cells: u8,
    shape: FootprintShape,
    keep: impl Fn(&sim_core::UnitArchetype) -> bool,
) -> UnitTypeId {
    let mut arch = world
        .catalog
        .archetypes()
        .find(|a| a.shape.footprint() == Some((size, size)) && keep(a))
        .expect("invariant: the shipped catalog has such a structure")
        .clone();
    let free = free_id(world);
    arch.unit_type_id = free;
    "contour_scratch_structure".clone_into(&mut arch.slug);
    "contour_scratch_structure".clone_into(&mut arch.name);
    arch.shape = ShapeKind::BuildingDome {
        footprint_w: cells,
        footprint_h: cells,
        shape,
    };
    world.catalog.register(arch);
    free
}

/// The 5.0.14 5×5 contour: corners cut by 1.5.
const HALL_CONTOUR: FootprintShape = FootprintShape::Box {
    half_width_q: 10,
    half_height_q: 10,
    chamfer_q: 6,
};

fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = world.current_tick;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    });
    assert_eq!(world.diagnostics(), [], "tick {tick}");
}

/// C-1 (D-8): a unit's centre may cross the cut corner of a 5×5 contour's occupied corner cell,
/// which lies outside the shape; a segment through the shape is refused.
#[test]
fn a_centre_may_cross_a_cut_corner_cell() {
    let mut world = world_on(flat(32));
    let hall = scratch_structure(&mut world, 5, 5, HALL_CONTOUR, |_| true);
    world
        .spawn_structure(hall, ME, 10, 10)
        .expect("open ground");
    // Cells (10, 10) to (14, 14); the south-west chamfer runs on x + y = 21.5. The segment
    // below runs on x + y = 20.1, through the occupied corner cell (10, 10).
    assert!(SimWorld::is_position_pathable(
        &world.map,
        at(9.5, 10.6),
        at(10.6, 9.5)
    ));
    // Through the shape.
    assert!(!SimWorld::is_position_pathable(
        &world.map,
        at(9.5, 12.5),
        at(11.5, 12.5)
    ));
    // Onto the chamfer itself: the shape is closed, so a centre on its contour is refused.
    assert!(!SimWorld::is_position_pathable(
        &world.map,
        at(9.5, 10.5),
        at(10.75, 10.75)
    ));
}

/// The tick count within which a builder lays a one-cell foundation it was ordered to build, from
/// `start`, or `None`.
fn ticks_to_lay_one_cell(start: SimVec2) -> Option<u32> {
    let mut world = world_on(flat(40));
    let builder = world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.can_construct && a.harvester.is_some())
        .expect("invariant: the shipped catalog has a harvester-capable builder")
        .clone();
    let faction = builder.faction;
    let tower = scratch_structure(&mut world, 2, 1, FootprintShape::full(1, 1), |a| {
        a.faction == faction && a.requires_structures.is_empty() && a.mineral_cost > 0
    });
    let worker = world
        .spawn_unit(builder.unit_type_id, ME, start)
        .expect("spawn");
    world.player_resources[PlayerSlot::ALL[0]].minerals = 1_000;
    world.player_resources[PlayerSlot::ALL[0]].gas = 1_000;
    step(
        &mut world,
        vec![PlayerCommand {
            player_id: ME,
            entities: vec![worker],
            order: UnitOrder::Build {
                structure_type: tower,
                cell_x: 20,
                cell_y: 12,
            },
            queue: false,
        }],
    );
    (1..=200).find(|_| {
        step(&mut world, Vec::new());
        world.entities.values().any(|e| e.unit_type_id == tower)
    })
}

/// C-2 (H-1, D-9): a harvester-capable builder builds a scratch one-cell structure within 200
/// ticks, whether it walks to the site or starts standing on it. Construction approaches and
/// leaves a site by its edge: the middle of a one-cell site is the site itself, so a builder
/// sent there never gets off it.
#[test]
fn a_worker_builds_a_one_cell_structure() {
    assert!(
        ticks_to_lay_one_cell(at(12.5, 12.5)).is_some(),
        "walking to the site"
    );
    assert!(
        ticks_to_lay_one_cell(at(20.5, 12.5)).is_some(),
        "standing on the site"
    );
}

// ---------------------------------------------------------------------------------------------
// WP6: the 5.0.14 shapes on shipped structures (C-3 to C-8).
// ---------------------------------------------------------------------------------------------

/// The contact rule's rounding allowance, 2⁻²⁰ world units (`WALL_CONTACT_TOLERANCE`).
const TOLERANCE: f64 = 1.0 / 1_048_576.0;

/// Registers a clone of the first shipped mobile ground unit under a free id: unarmed, with
/// collision radius `collision` and terrain radius `terrain`, speed 0.2 per tick reached at once
/// and no turning delay (as `terrain_radius_tests::register`).
fn register_body(world: &mut SimWorld, slug: &str, collision: f64, terrain: f64) -> UnitTypeId {
    let mut arch = world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == sim_types::MoverType::Ground
                && a.max_speed > SimScalar::ZERO
                && a.shape.footprint().is_none()
        })
        .expect("invariant: the shipped catalog has a mobile ground unit")
        .clone();
    arch.unit_type_id = free_id(world);
    slug.clone_into(&mut arch.slug);
    slug.clone_into(&mut arch.name);
    arch.collision_radius = s(collision);
    arch.separation_radius = s(collision);
    arch.terrain_radius = s(terrain);
    arch.max_speed = s(0.2);
    arch.acceleration = s(10.0);
    arch.deceleration = s(10.0);
    arch.turn_rate = sim_types::SimAngle::from_degrees(180);
    arch.stationary_turn_rate = sim_types::SimAngle::from_degrees(180);
    arch.turn_before_move = false;
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
    arch.capabilities.overclock = None;
    arch.capabilities.barrier = None;
    arch.capabilities.attack_dash = None;
    let id = arch.unit_type_id;
    world.catalog.register(arch);
    id
}

/// The first shipped unarmed structure whose footprint is `size` cells with shape `shape`.
fn shipped_structure(world: &SimWorld, size: u8, shape: FootprintShape) -> UnitTypeId {
    world
        .catalog
        .archetypes()
        .find(|a| {
            a.shape
                == ShapeKind::BuildingDome {
                    footprint_w: size,
                    footprint_h: size,
                    shape,
                }
                && a.weapon.is_none()
                && a.ground_weapon.is_none()
                && a.air_weapon.is_none()
        })
        .map(|a| a.unit_type_id)
        .expect("invariant: the shipped catalog has such a structure")
}

/// The 5.0.14 2×2 contour: corners cut by 0.5.
const DEPOT_CONTOUR: FootprintShape = FootprintShape::Box {
    half_width_q: 4,
    half_height_q: 4,
    chamfer_q: 2,
};

/// The staircase map of `clearance_field_tests::f7_no_tier_passes_between_structures_touching_
/// at_a_corner` with two shipped 2×2 contours corner to corner at cells (10, 10) and (12, 12):
/// a wall along the diagonal x = y whose only gap is the `1/√2` band between the chamfers.
fn depot_wall_world() -> SimWorld {
    let side = 24_u32;
    let mut map = flat(side);
    for i in (0..10).chain(14..side) {
        map.cell_mut(i, i).expect("on the map").flags = sim_map::CellFlags::empty();
        if i + 1 < side {
            map.cell_mut(i + 1, i).expect("on the map").flags = sim_map::CellFlags::empty();
        }
    }
    let mut world = world_on(map);
    let depot = shipped_structure(&world, 2, DEPOT_CONTOUR);
    for cell in [10, 12] {
        world
            .spawn_structure(depot, PlayerId(1), cell, cell)
            .expect("open ground");
    }
    world
}

fn move_to(id: EntityId, target: SimVec2) -> PlayerCommand {
    PlayerCommand {
        player_id: ME,
        entities: vec![id],
        order: UnitOrder::Move { target },
        queue: false,
    }
}

fn xy(p: SimVec2) -> (f64, f64) {
    (f64::from(p.x.to_f32()), f64::from(p.y.to_f32()))
}

/// The Euclidean distance from `p` to the closed shape of `placed`, world units (0 inside).
fn gap_to_shape(placed: &sim_map::PlacedStructure, p: (f64, f64)) -> f64 {
    let footprint = placed.footprint();
    let vertices = footprint.contour();
    let count = vertices.len();
    let world = |v: sim_map::LatticePoint| (f64::from(v.x) / 4.0, f64::from(v.y) / 4.0);
    let mut inside = true;
    let mut nearest = f64::INFINITY;
    for i in 0..count {
        let (u, v) = (world(vertices[i]), world(vertices[(i + 1) % count]));
        let (dx, dy) = (v.0 - u.0, v.1 - u.1);
        let (wx, wy) = (p.0 - u.0, p.1 - u.1);
        if dx * wy - dy * wx < 0.0 {
            inside = false;
        }
        let t = ((wx * dx + wy * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        nearest = nearest.min((wx - t * dx).hypot(wy - t * dy));
    }
    if inside {
        0.0
    } else {
        nearest
    }
}

/// The least distance from `p` to any structure's shape on the map.
fn gap_to_structures(world: &SimWorld, p: SimVec2) -> f64 {
    world
        .map
        .structures()
        .values()
        .map(|placed| gap_to_shape(placed, xy(p)))
        .fold(f64::INFINITY, f64::min)
}

/// C-3, the owner's case end to end (Q0087, S10): two shipped 2×2 contours corner to corner. A
/// worker-sized body (`t` 0.3125, `r` 0.375) ordered across arrives within 400 ticks; a
/// Zergling-sized body (`t = r = 0.375`) never reaches the far side of the wall, the diagonal
/// x = y, and its order ends without a diagnostic.
#[test]
fn a_worker_passes_a_diagonal_depot_wall_and_a_zergling_does_not() {
    let (start, goal) = (at(3.5, 18.5), at(18.5, 3.5));
    let mut world = depot_wall_world();
    let worker = register_body(&mut world, "contour_worker", 0.375, 0.3125);
    let id = world.spawn_unit(worker, ME, start).expect("spawn");
    step(&mut world, vec![move_to(id, goal)]);
    let arrived = (0..400).find(|_| {
        step(&mut world, Vec::new());
        world.entities[&id].position.distance(goal) < s(0.1)
    });
    assert!(
        arrived.is_some(),
        "the worker-sized body did not arrive: {:?}",
        xy(world.entities[&id].position)
    );

    let mut world = depot_wall_world();
    let zergling = register_body(&mut world, "contour_zergling", 0.375, 0.375);
    let id = world.spawn_unit(zergling, ME, start).expect("spawn");
    step(&mut world, vec![move_to(id, goal)]);
    for _ in 0..400 {
        step(&mut world, Vec::new());
        let (x, y) = xy(world.entities[&id].position);
        assert!(
            x < y,
            "the Zergling-sized body crossed the wall at ({x}, {y})"
        );
    }
    assert!(
        world.entities[&id].order_queue.is_empty(),
        "the Zergling-sized body's order did not end"
    );
}

/// C-4: a Zergling-sized body standing at the gap's mouth, pushed by a friendly body walking
/// through it, never enters the gap: its distance to every chamfer stays at least `t` (less the
/// contact rule's rounding allowance), and it never crosses the wall.
#[test]
fn a_pushed_zergling_never_enters_the_gap() {
    let mut world = depot_wall_world();
    let zergling = register_body(&mut world, "contour_zergling", 0.375, 0.375);
    let pusher = register_body(&mut world, "contour_pusher", 0.375, 0.375);
    // The gap's north-west mouth is round (11.6, 12.4); the Zergling stands in it, the pusher
    // behind it on the 45° line.
    let held = world
        .spawn_unit(zergling, ME, at(11.1, 12.9))
        .expect("spawn");
    let walker = world.spawn_unit(pusher, ME, at(9.6, 14.4)).expect("spawn");
    step(&mut world, vec![move_to(walker, at(14.0, 10.0))]);
    for _ in 0..200 {
        step(&mut world, Vec::new());
        let p = world.entities[&held].position;
        let (x, y) = xy(p);
        assert!(
            x < y,
            "the Zergling-sized body crossed the wall at ({x}, {y})"
        );
        assert!(
            gap_to_structures(&world, p) >= 0.375 - TOLERANCE,
            "the Zergling-sized body is {} from a chamfer at ({x}, {y})",
            gap_to_structures(&world, p)
        );
    }
}

/// C-5 (D-8): a slim body walking diagonally past a shipped 5×5 contour's cut corner passes
/// inside the old square of its cells, keeps its terrain radius from the chamfer, and arrives.
/// Before the contour (full squares) it had to walk round the square's corner.
#[test]
fn a_unit_rounds_a_hall_corner_inside_its_old_square() {
    let mut world = world_on(flat(32));
    let hall = shipped_structure(&world, 5, HALL_CONTOUR);
    world
        .spawn_structure(hall, PlayerId(1), 10, 10)
        .expect("open ground");
    let slim = register_body(&mut world, "contour_slim", 0.5, 0.3125);
    let (start, goal) = (at(8.5, 12.1), at(12.1, 8.5));
    let id = world.spawn_unit(slim, ME, start).expect("spawn");
    step(&mut world, vec![move_to(id, goal)]);
    let mut inside_square = false;
    let mut arrived = false;
    for _ in 0..200 {
        step(&mut world, Vec::new());
        let p = world.entities[&id].position;
        let (x, y) = xy(p);
        inside_square |= (10.0..15.0).contains(&x) && (10.0..15.0).contains(&y);
        assert!(
            gap_to_structures(&world, p) >= 0.3125 - TOLERANCE,
            "({x}, {y}) is {} from the chamfer",
            gap_to_structures(&world, p)
        );
        if p.distance(goal) < s(0.1) {
            arrived = true;
            break;
        }
    }
    assert!(inside_square, "the body walked round the old square");
    assert!(arrived, "the body did not arrive");
}

/// The 5.0.14 Spore Crawler shape: a 1×1 box cut by 0.25, centred in 2×2 cells.
const SPORE_CONTOUR: FootprintShape = FootprintShape::Box {
    half_width_q: 2,
    half_height_q: 2,
    chamfer_q: 1,
};

/// C-6 (S6): a body sent onto a shipped Spore-sized structure's outer cells, to the point its
/// terrain radius below the shape, rests there: inside the 2×2 cells and `t` from the shape.
#[test]
fn units_stand_on_a_spore_analogues_outer_cells() {
    let mut world = world_on(flat(32));
    let kind = world
        .catalog
        .archetypes()
        .find(|a| {
            a.shape
                == ShapeKind::BuildingDome {
                    footprint_w: 2,
                    footprint_h: 2,
                    shape: SPORE_CONTOUR,
                }
        })
        .map(|a| a.unit_type_id)
        .expect("invariant: the shipped catalog has a Spore-sized structure");
    let placed = world
        .spawn_structure(kind, PlayerId(1), 10, 10)
        .expect("open ground");
    let slim = register_body(&mut world, "contour_slim", 0.5, 0.3125);
    let id = world.spawn_unit(slim, ME, at(11.0, 7.0)).expect("spawn");
    // Exactly `t` below the shape's south face (y = 10.5), on the structure's lower cells.
    step(&mut world, vec![move_to(id, at(11.0, 10.1875))]);
    for _ in 0..100 {
        step(&mut world, Vec::new());
    }
    let p = world.entities[&id].position;
    let (x, y) = xy(p);
    assert!(
        (10.0..12.0).contains(&x) && (10.0..12.0).contains(&y),
        "({x}, {y}) is not on the structure's cells"
    );
    let gap = gap_to_shape(&world.map.structures()[&placed], (x, y));
    assert!(
        (0.3125 - TOLERANCE..0.3125 + 0.01).contains(&gap),
        "({x}, {y}) rests {gap} from the shape"
    );
}

/// C-7 (B-138, D-9): the shipped one-cell structure is placed and built by a worker, and a body
/// walks round it.
#[test]
fn a_one_cell_sensor_analogue_is_placed_built_and_walked_round() {
    let mut world = world_on(flat(40));
    let tower = world
        .catalog
        .archetypes()
        .find(|a| a.shape.footprint() == Some((1, 1)))
        .expect("invariant: the shipped catalog has a one-cell structure")
        .clone();
    let builder = world
        .catalog
        .archetypes()
        .find(|a| {
            a.capabilities.can_construct && a.harvester.is_some() && a.faction == tower.faction
        })
        .expect("invariant: the one-cell structure's faction has a builder")
        .unit_type_id;
    let worker = world
        .spawn_unit(builder, ME, at(12.5, 12.5))
        .expect("spawn");
    world.player_resources[PlayerSlot::ALL[0]].minerals = 10_000;
    world.player_resources[PlayerSlot::ALL[0]].gas = 10_000;
    // Its prerequisites, spawned complete beside it.
    for slug in &tower.requires_structures {
        let kind = world
            .catalog
            .id_by_slug(slug)
            .expect("invariant: prerequisites are catalog structures");
        let (w, _) = world
            .catalog
            .get(kind)
            .expect("listed")
            .shape
            .footprint()
            .expect("a structure");
        let x = 2 + 6 * u32::try_from(world.map.structures().len()).unwrap_or(0);
        world.spawn_structure(kind, ME, x, 30).expect("open ground");
        assert!(w <= 5);
    }
    step(
        &mut world,
        vec![PlayerCommand {
            player_id: ME,
            entities: vec![worker],
            order: UnitOrder::Build {
                structure_type: tower.unit_type_id,
                cell_x: 20,
                cell_y: 12,
            },
            queue: false,
        }],
    );
    let laid = (0..300).find_map(|_| {
        step(&mut world, Vec::new());
        world
            .entities
            .values()
            .find(|e| e.unit_type_id == tower.unit_type_id)
            .map(|e| e.id)
    });
    let laid = laid.expect("the one-cell foundation was laid");
    let on_map = &world.map.structures()[&laid];
    assert_eq!(
        (on_map.top_left, on_map.width, on_map.height),
        ((20, 12), 1, 1)
    );
    // A body walks from one side of it to the other, round it.
    let slim = register_body(&mut world, "contour_slim", 0.5, 0.3125);
    let id = world.spawn_unit(slim, ME, at(17.5, 12.5)).expect("spawn");
    let goal = at(23.5, 12.5);
    step(&mut world, vec![move_to(id, goal)]);
    let arrived = (0..200).any(|_| {
        step(&mut world, Vec::new());
        world.entities[&id].position.distance(goal) < s(0.1)
    });
    assert!(
        arrived,
        "the body did not walk round the one-cell structure"
    );
}

/// C-8 (data property, S10): every shipped ground archetype routes through the 2×2 diagonal
/// corner gap exactly when `8t² ≤ 1` (`2t ≤ 1/√2`), read from its terrain radius; every
/// harvester passes, and the Zergling-sized radius 0.375 does not.
#[test]
fn every_shipped_ground_archetype_meets_the_corner_gap_by_its_terrain_radius() {
    let world = depot_wall_world();
    let mut pathfinder = sim_map::Pathfinder::new(&world.map);
    let (start, goal) = (at(3.5, 18.5), at(18.5, 3.5));
    let mut checked = 0;
    for arch in world.catalog.archetypes() {
        if arch.mover_type != sim_types::MoverType::Ground || arch.shape.footprint().is_some() {
            continue;
        }
        let t = arch.terrain_radius;
        let request = sim_map::PathRequest {
            start,
            goal,
            radius: t,
        };
        let routes = pathfinder.find_path(&world.map, &request).is_ok();
        let fits = 8.0 * f64::from(t.to_f32()).powi(2) <= 1.0;
        assert_eq!(
            routes,
            fits,
            "{} (terrain radius {})",
            arch.slug,
            t.to_f32()
        );
        if arch.harvester.is_some() {
            assert!(routes, "harvester {} does not pass the gap", arch.slug);
        }
        checked += 1;
    }
    assert!(checked > 0);
}

// ---------------------------------------------------------------------------------------------
// Amendment 4: an automatic chase gets 5.0.14's leash, 21 from where it began (B-142).
// ---------------------------------------------------------------------------------------------

/// The `PursuitAbandoned` outcomes of `id` in the tick just stepped.
fn abandoned(world: &SimWorld, id: EntityId) -> usize {
    world
        .route_outcomes()
        .iter()
        .filter(|o| {
            o.entity == id
                && matches!(
                    o.kind,
                    sim_core::movement::RouteOutcomeKind::PursuitAbandoned { .. }
                )
        })
        .count()
}

/// C-9 (Amendment 4, the showcase's case): a shipped worker standing in a shipped 5×5 hall's cut
/// south-west corner is called to help when an enemy shoots the hall from beyond its north-east
/// face. The attacker stands 10 to 21 from the worker, and the way round the hall is longer than
/// the old leash of twice the worker's scan range. The worker chases it round the hall and
/// reaches it, with no `PursuitAbandoned`.
#[test]
fn a_worker_called_from_a_cut_corner_reaches_an_attacker_behind_the_hall() {
    let mut world = world_on(flat(40));
    let hall = shipped_structure(&world, 5, HALL_CONTOUR);
    world
        .spawn_structure(hall, ME, 10, 10)
        .expect("open ground");
    let worker_kind = world
        .catalog
        .archetypes()
        .find(|a| {
            a.harvester.is_some()
                && a.weapon_against(sim_types::MoverType::Ground, None)
                    .is_some_and(|w| w.melee)
        })
        .expect("invariant: the shipped catalog has a worker with a melee weapon")
        .clone();
    let shooter_kind = world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == sim_types::MoverType::Ground
                && a.shape.footprint().is_none()
                && a.harvester.is_none()
                && a.weapon_against(sim_types::MoverType::Ground, None)
                    .is_some_and(|w| !w.melee && w.max_range < s(7.0))
        })
        .expect("invariant: the shipped catalog has a short-ranged ground shooter")
        .clone();
    let range = f64::from(
        shooter_kind
            .weapon_against(sim_types::MoverType::Ground, None)
            .expect("chosen for it")
            .max_range
            .to_f32(),
    );
    // On the hall's north-east diagonal, inside its range of the hall's edge.
    let offset = (range + 2.25) / std::f64::consts::SQRT_2;
    let shooter_at = at(12.5 + offset, 12.5 + offset);
    let worker_at = at(10.4, 10.4);
    let separation = f64::from(worker_at.distance(shooter_at).to_f32());
    assert!(
        (10.0..21.0).contains(&separation),
        "the attacker stands {separation} from the worker"
    );
    let worker = world
        .spawn_unit(worker_kind.unit_type_id, ME, worker_at)
        .expect("spawn");
    let shooter = world
        .spawn_unit(shooter_kind.unit_type_id, PlayerId(1), shooter_at)
        .expect("spawn");
    let hall_id = world
        .entities
        .values()
        .find(|e| e.unit_type_id == hall)
        .map(|e| e.id)
        .expect("spawned above");
    step(
        &mut world,
        vec![PlayerCommand {
            player_id: PlayerId(1),
            entities: vec![shooter],
            order: UnitOrder::HoldPosition,
            queue: false,
        }],
    );
    step(
        &mut world,
        vec![PlayerCommand {
            player_id: PlayerId(1),
            entities: vec![shooter],
            order: UnitOrder::attack(hall_id),
            queue: true,
        }],
    );
    let reach = f64::from(
        (worker_kind.collision_radius
            + shooter_kind.collision_radius
            + worker_kind
                .weapon_against(sim_types::MoverType::Ground, None)
                .expect("chosen for it")
                .max_range)
            .to_f32(),
    ) + 0.1;
    let mut gave_up = 0;
    let mut reached = false;
    for _ in 0..600 {
        step(&mut world, Vec::new());
        gave_up += abandoned(&world, worker);
        let (Some(w), Some(t)) = (world.entities.get(&worker), world.entities.get(&shooter)) else {
            break;
        };
        if f64::from(w.position.distance(t.position).to_f32()) <= reach {
            reached = true;
            break;
        }
    }
    assert_eq!(gave_up, 0, "the worker gave its chase up");
    assert!(reached, "the worker did not reach the attacker");
}
