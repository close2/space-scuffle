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
    FootprintShape, PlayerCommand, PlayerId, PlayerSlot, ShapeKind, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder, UnitTypeId,
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
const HALL_CONTOUR: FootprintShape = FootprintShape {
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
