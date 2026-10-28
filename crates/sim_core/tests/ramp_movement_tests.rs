//! Exercises real world movement over loaded ramp terrain and hostile side faces.
//! Catalog capabilities select mobile ground units; deterministic runs must reach
//! the opposite landing without steering or collision movement crossing a cliff.

use sim_core::SimWorld;
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, MapFile, RampDir};
use sim_types::{EntityId, MoverType, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn center(x: u32, y: u32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

#[test]
fn physical_motion_rejects_ramp_side_exits_and_intermediate_obstacles() -> TestResult {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1)))?;
    for y in 5..8 {
        let ramp = map.cell_mut(7, y)?;
        ramp.ramp = Some(RampDir::North);
        ramp.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
    }
    map.cell_mut(7, 8)?.cliff_level = CliffLevel(2);
    assert!(SimWorld::is_position_pathable(
        &map,
        center(7, 4),
        center(7, 8)
    ));
    assert!(SimWorld::is_position_pathable(
        &map,
        center(7, 8),
        center(7, 4)
    ));
    for level in 0..=3 {
        map.cell_mut(8, 6)?.cliff_level = CliffLevel(level);
        assert!(!SimWorld::is_position_pathable(
            &map,
            center(7, 6),
            center(8, 6)
        ));
        assert!(!SimWorld::is_position_pathable(
            &map,
            center(8, 6),
            center(7, 6)
        ));
    }
    map.place_structure(EntityId(1), 4, 2, 1, 1)?;
    assert!(!SimWorld::is_position_pathable(
        &map,
        center(2, 2),
        center(6, 2)
    ));
    Ok(())
}

fn loaded_world() -> Result<(SimWorld, EntityId), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/maps/default_arena.scmap.ron");
    let map = MapFile::load(&path)?.decode()?.to_game_map()?;
    let mut world = SimWorld::new(map, 417);
    let archetype = world
        .catalog
        .archetypes()
        .find(|a| a.mover_type == MoverType::Ground && !a.is_immobile_by_default())
        .ok_or("catalog must supply a mobile ground archetype")?
        .unit_type_id;
    world.vision_aware_pathfinding = false;
    let id = world.spawn_unit(archetype, PlayerId(0), center(31, 28))?;
    world
        .entities
        .get_mut(&id)
        .ok_or("spawned entity must exist")?
        .issue_order(
            UnitOrder::Move {
                target: center(31, 37),
            },
            false,
        )?;
    Ok((world, id))
}

#[test]
fn real_move_orders_cross_loaded_ramps_deterministically() -> TestResult {
    let (mut first, id) = loaded_world()?;
    let (mut second, second_id) = loaded_world()?;
    assert_eq!(id, second_id);
    let mut previous = first.entities.get(&id).ok_or("entity missing")?.position;
    let mut crossed = false;
    for tick in 0..2048 {
        let bundle = TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        };
        first.step(&bundle);
        second.step(&bundle);
        assert_eq!(first.last_checksum(), second.last_checksum(), "tick {tick}");
        let position = first.entities.get(&id).ok_or("entity missing")?.position;
        assert!(SimWorld::is_position_pathable(
            &first.map, previous, position
        ));
        if position.y >= SimScalar::from_num(30) && position.y < SimScalar::from_num(34) {
            assert!(
                position.x >= SimScalar::from_num(30) && position.x < SimScalar::from_num(34),
                "unit crossed a forbidden side wall"
            );
        }
        crossed |= position.y >= SimScalar::from_num(34);
        previous = position;
    }
    assert!(crossed, "real movement must reach the upper landing");
    assert!(previous.distance(center(31, 37)) < SimScalar::ONE);
    Ok(())
}

#[test]
fn arrival_snaps_cannot_cross_a_ramp_side_wall() -> TestResult {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1)))?;
    for y in 5..8 {
        let ramp = map.cell_mut(7, y)?;
        ramp.ramp = Some(RampDir::North);
        ramp.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
    }
    map.cell_mut(7, 8)?.cliff_level = CliffLevel(2);
    // Isolate the side neighbor from every legal ramp entrance. The requested
    // target is physically close but belongs to a disconnected terrain surface.
    for y in 0..16 {
        for x in 0..16 {
            if x != 7 && (x, y) != (8, 6) {
                map.cell_mut(x, y)?.flags = CellFlags::UNPASSABLE_CLIFF;
            }
        }
    }
    let mut world = SimWorld::new(map, 418);
    world.vision_aware_pathfinding = false;
    let archetype = world
        .catalog
        .archetypes()
        .find(|a| a.mover_type == MoverType::Ground && !a.is_immobile_by_default())
        .ok_or("catalog must supply a mobile ground archetype")?;
    let unit_type = archetype.unit_type_id;
    let epsilon = archetype.max_speed.min(SimScalar::ONE) / SimScalar::from_num(8);
    let start = SimVec2::new(
        SimScalar::from_num(8) - epsilon,
        SimScalar::from_num(6) + SimScalar::HALF,
    );
    let target = SimVec2::new(SimScalar::from_num(8) + epsilon, start.y);
    let id = world.spawn_unit(unit_type, PlayerId(0), start)?;
    world
        .entities
        .get_mut(&id)
        .ok_or("entity missing")?
        .issue_order(UnitOrder::Move { target }, false)?;
    for tick in 0..32 {
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
        let position = world.entities.get(&id).ok_or("entity missing")?.position;
        assert!(
            position.x < SimScalar::from_num(8),
            "arrival snap crossed the ramp side"
        );
    }
    // The next-waypoint arrival branch must also reject a stale illegal waypoint.
    let entity = world.entities.get_mut(&id).ok_or("entity missing")?;
    entity.position = start;
    entity.current_funnel_waypoints = vec![target];
    world.step(&TickBundle {
        tick: Tick(32),
        commands: Vec::new(),
        state_checksum: None,
    });
    assert!(world.entities.get(&id).ok_or("entity missing")?.position.x < SimScalar::from_num(8));
    Ok(())
}
