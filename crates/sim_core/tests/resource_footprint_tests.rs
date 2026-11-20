//! Resource node footprints in the world (plan `2026-11-20-resource-footprints` WP5).
//!
//! # Responsibility
//! Proves the world half of D-4 and D-5's `sim_core` consumers with the shipped catalog: a world
//! places every node of its map by its kind's footprint and refuses a map whose nodes overlap;
//! nothing spawns on a node (scenario rings and the hall region, production exits); a unit's
//! centre never enters a field (C1) and no unit is placed on one (C2); a site on a field says
//! `CoversResource` and an extractor must cover exactly one geyser (P2); a node's footprint is
//! hashed (D-11).
//!
//! # How it fits
//! Worlds come from `sim_fixtures` with the shipped catalogs; nodes are chosen by kind from the
//! map, never by slug. Behaviour is reached through public world APIs and `PlayerCommand`s.
//!
//! # Invariants
//! Positions are fixed-point world units; cells are grid cells; sites are named by their minimum
//! corner.

#![allow(
    clippy::expect_used,
    reason = "a broken world law must fail the test loudly"
)]

use std::error::Error;

use sim_core::{SimCoreError, SimWorld};
use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapError, MapFile, ResourceFootprintFailure,
    ResourceNode,
};
use sim_types::{
    EntityId, PlayerId, ResourceKind, ResourceNodeId, ShapeKind, SimScalar, SimVec2, SiteRefusal,
    UnitTypeId,
};

type TestResult = Result<(), Box<dyn Error>>;

const SHOWCASE: &str = include_str!("../../../assets/maps/showcase_canyon.scmap.ron");

fn showcase() -> Result<GameMap, Box<dyn Error>> {
    Ok(MapFile::from_ron_str(SHOWCASE)?.decode()?.to_game_map()?)
}

/// A node of `kind` anchored at `cell`, not placed.
fn node(kind: ResourceKind, cell: (u32, u32)) -> ResourceNode {
    let centre = |v: u32| SimScalar::from_num(v) + SimScalar::HALF;
    ResourceNode {
        pos: SimVec2::new(centre(cell.0), centre(cell.1)),
        kind,
        remaining: 1_500,
        cell,
        placed: None,
    }
}

/// A flat open 32 × 32 map with `nodes`.
fn flat(nodes: Vec<ResourceNode>) -> GameMap {
    GameMap::from_cells(
        32,
        32,
        vec![MapCell::default_ground(CliffLevel(0)); 32 * 32],
        nodes,
        Vec::new(),
    )
    .expect("invariant: 32 × 32 cells")
}

/// A point in sixteenths of a cell.
fn at16(x: i64, y: i64) -> SimVec2 {
    SimVec2::new(SimScalar::from_raw(x << 28), SimScalar::from_raw(y << 28))
}

/// Whether the disc of `radius` at `centre` meets any standing node's outline.
fn on_a_node(world: &SimWorld, centre: SimVec2, radius: SimScalar) -> bool {
    world.map.resources().iter().any(|node| {
        node.placed
            .is_some_and(|placed| placed.footprint.capsule_meets(centre, centre, radius))
    })
}

#[test]
fn a_world_places_its_nodes() -> TestResult {
    let world = sim_fixtures::try_world(showcase()?, 1)?;
    assert_eq!(world.map.resources().len(), 20);
    for (index, node) in (0_u32..).zip(world.map.resources()) {
        let placed = node.placed.ok_or("every node stands")?.footprint;
        let size = match node.kind {
            ResourceKind::Mineral => (2, 1),
            ResourceKind::Gas => (3, 3),
        };
        assert_eq!((placed.width, placed.height), size, "{:?}", node.kind);
        for y in placed.top_left.1..placed.top_left.1 + u32::from(placed.height) {
            for x in placed.top_left.0..placed.top_left.0 + u32::from(placed.width) {
                let cell = world.map.cell(x, y)?;
                assert_eq!(cell.occupying_resource, Some(ResourceNodeId(index)));
                assert!(cell.flags.contains(CellFlags::OCCUPIED_BY_RESOURCE));
            }
        }
        // A field's position is the centre of its 2×1: its anchor's east edge.
        if node.kind == ResourceKind::Mineral {
            assert_eq!(
                node.pos,
                SimVec2::new(
                    SimScalar::from_num(node.cell.0 + 1),
                    SimScalar::from_num(node.cell.1) + SimScalar::HALF
                )
            );
        }
    }
    Ok(())
}

#[test]
fn a_world_with_overlapping_nodes_is_refused() {
    let map = flat(vec![
        node(ResourceKind::Mineral, (3, 3)),
        node(ResourceKind::Mineral, (4, 3)),
    ]);
    let refused = sim_fixtures::try_world(map, 1).map(|_| ());
    assert_eq!(
        refused,
        Err(SimCoreError::ResourceFootprint(Box::new(
            MapError::ResourceFootprint {
                node: ResourceNodeId(1),
                reason: ResourceFootprintFailure::OverlapsNode(ResourceNodeId(0)),
            }
        )))
    );
}

#[test]
fn nothing_spawns_on_a_node() -> TestResult {
    // Scenario rings and the hall region: the showcase's starts, with their lines beside them.
    let doc = MapFile::from_ron_str(SHOWCASE)?.decode()?;
    let manifest = sim_core::scenario::ScenarioManifest::from_ron_str(include_str!(
        "../../../assets/scenarios/skirmish.ron"
    ))?;
    let mut world = sim_fixtures::scenario_world(
        &doc,
        &sim_fixtures::shipped_loadouts(),
        &manifest.players,
        1,
    )?;
    let bodies = |world: &SimWorld| -> Vec<(EntityId, SimVec2, SimScalar)> {
        world
            .entities
            .values()
            .filter(|entity| {
                world
                    .catalog
                    .get(entity.unit_type_id)
                    .is_ok_and(|arch| !matches!(arch.shape, ShapeKind::BuildingDome { .. }))
            })
            .map(|entity| {
                let radius = world
                    .catalog
                    .get(entity.unit_type_id)
                    .map_or(SimScalar::ZERO, |arch| arch.terrain_radius);
                (entity.id, entity.position, radius)
            })
            .collect()
    };
    let spawned = bodies(&world);
    assert!(spawned.len() >= 12, "the starts' workers");
    for &(id, pos, radius) in &spawned {
        assert!(!on_a_node(&world, pos, radius), "{id:?} spawned on a node");
    }
    // Production exits: each hall trains workers until its queue empties; none lands on a node.
    let halls: Vec<(EntityId, PlayerId)> = world
        .entities
        .values()
        .filter(|entity| {
            world
                .catalog
                .get(entity.unit_type_id)
                .is_ok_and(|arch| matches!(arch.shape, ShapeKind::BuildingDome { .. }))
        })
        .map(|entity| (entity.id, entity.owner))
        .collect();
    let worker = world
        .entities
        .values()
        .find(|entity| {
            world
                .catalog
                .get(entity.unit_type_id)
                .is_ok_and(|a| a.capabilities.harvester.is_some())
        })
        .map(|entity| entity.unit_type_id)
        .ok_or("the starts have workers")?;
    for slot in sim_types::PlayerSlot::ALL.iter().take(2) {
        world.player_resources[*slot].minerals = 10_000;
    }
    let commands: Vec<sim_types::PlayerCommand> = halls
        .iter()
        .flat_map(|&(hall, owner)| {
            (0..5).map(move |_| sim_types::PlayerCommand {
                player_id: owner,
                entities: vec![hall],
                order: sim_types::UnitOrder::Produce { unit_type: worker },
                queue: true,
            })
        })
        .collect();
    world.step(&sim_types::TickBundle {
        tick: sim_types::Tick(world.current_tick + 1),
        commands,
        state_checksum: None,
    });
    for _ in 0..2_000 {
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(world.current_tick + 1),
            commands: Vec::new(),
            state_checksum: None,
        });
    }
    let trained = bodies(&world);
    assert!(
        trained.len() >= spawned.len() + 10,
        "the halls trained their workers"
    );
    for &(id, pos, radius) in &trained {
        assert!(!on_a_node(&world, pos, radius), "{id:?} stands on a node");
    }
    Ok(())
}

#[test]
fn a_centre_never_enters_a_field() -> TestResult {
    let world = sim_fixtures::try_world(
        flat(vec![
            node(ResourceKind::Mineral, (10, 10)),
            node(ResourceKind::Gas, (20, 20)),
        ]),
        1,
    )?;
    // The field covers [10, 12) × [10, 11). A segment into it is refused; one along its south
    // face, a sixteenth below it, is not.
    let south = at16(176, 152);
    assert!(!world.is_position_pathable_from(south, at16(176, 168)));
    assert!(world.is_position_pathable_from(at16(144, 159), at16(208, 159)));
    // Touching the closed outline is entering it.
    assert!(!world.is_position_pathable_from(south, at16(176, 160)));
    // The geyser's recessed south side (y 19.5 over x 20 to 21) admits a centre in the cell
    // below its middle cell's centre, where the hull would not.
    assert!(world.is_position_pathable_from(at16(328, 300), at16(328, 311)));
    assert!(!world.is_position_pathable_from(at16(328, 300), at16(328, 312)));
    Ok(())
}

#[test]
fn a_blinked_or_unloaded_unit_is_never_placed_on_a_field() -> TestResult {
    let mut world = sim_fixtures::try_world(flat(vec![node(ResourceKind::Mineral, (10, 10))]), 1)?;
    let walker = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.mover_type == sim_types::MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !matches!(arch.shape, ShapeKind::BuildingDome { .. })
        })
        .ok_or("a ground walker")?
        .unit_type_id;
    let id = world.spawn_unit(walker, PlayerId(0), at16(64, 64))?;
    let radius = world.catalog.get(walker)?.terrain_radius;
    // On the field, and a disc that overlaps its east face: both refused (C2, as blink and unload
    // place).
    assert!(world
        .validate_displacement_destination(id, at16(176, 168))
        .is_err());
    let east = SimScalar::from_num(12) + radius - SimScalar::from_raw(1 << 16);
    assert!(world
        .validate_displacement_destination(
            id,
            SimVec2::new(east, SimScalar::from_num(10) + SimScalar::HALF)
        )
        .is_err());
    // A disc just clear of the face is placed.
    let clear = SimScalar::from_num(12) + radius + SimScalar::from_raw(1 << 16);
    assert!(world
        .validate_displacement_destination(
            id,
            SimVec2::new(clear, SimScalar::from_num(10) + SimScalar::HALF)
        )
        .is_ok());
    Ok(())
}

/// The shipped structure types of `size`, and the gas extractors.
fn structures(world: &SimWorld) -> (UnitTypeId, Vec<UnitTypeId>) {
    let production = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.shape.footprint() == Some((3, 3))
                && !arch.capabilities.is_gas_extractor
                && arch.placement_excludes.is_empty()
        })
        .expect("invariant: the shipped catalog has a plain 3×3 structure")
        .unit_type_id;
    let extractors = world
        .catalog
        .archetypes()
        .filter(|arch| arch.capabilities.is_gas_extractor)
        .map(|arch| arch.unit_type_id)
        .collect();
    (production, extractors)
}

#[test]
fn a_site_on_a_field_says_covers_resource() -> TestResult {
    let world = sim_fixtures::try_world(flat(vec![node(ResourceKind::Mineral, (10, 10))]), 1)?;
    let (production, extractors) = structures(&world);
    // Covering only the field's east cell (11, 10), then only its anchor (10, 10).
    assert_eq!(
        world.preview_site(PlayerId(0), production, 11, 8),
        Err(SiteRefusal::CoversResource)
    );
    assert_eq!(
        world.preview_site(PlayerId(0), production, 8, 9),
        Err(SiteRefusal::CoversResource)
    );
    for extractor in extractors {
        assert_eq!(
            world.preview_site(PlayerId(0), extractor, 10, 9),
            Err(SiteRefusal::ExtractorNeedsGas)
        );
    }
    Ok(())
}

#[test]
fn an_extractor_must_cover_exactly_one_geyser() -> TestResult {
    let mut world = sim_fixtures::try_world(flat(vec![node(ResourceKind::Gas, (10, 10))]), 1)?;
    let (production, extractors) = structures(&world);
    let first = *extractors.first().ok_or("the catalog has gas extractors")?;
    for &extractor in &extractors {
        // Exactly the geyser's 3×3, centred on its anchor.
        assert_eq!(world.preview_site(PlayerId(0), extractor, 9, 9), Ok(()));
        // Off by a cell, or on open ground: no geyser under it.
        for (x, y) in [(10, 9), (8, 9), (9, 10), (20, 20)] {
            assert_eq!(
                world.preview_site(PlayerId(0), extractor, x, y),
                Err(SiteRefusal::ExtractorNeedsGas),
                "({x}, {y})"
            );
        }
    }
    assert_eq!(
        world.preview_site(PlayerId(0), production, 9, 9),
        Err(SiteRefusal::CoversResource)
    );
    // Spawned over the geyser, an extractor holds the cells with it; spawned anywhere else it
    // is refused.
    let extractor = first;
    assert!(world
        .spawn_structure(extractor, PlayerId(0), 20, 20)
        .is_err());
    let placed = world.spawn_structure(extractor, PlayerId(0), 9, 9)?;
    let cell = world.map.cell(10, 10)?;
    assert_eq!(cell.occupying_structure, Some(placed));
    assert_eq!(cell.occupying_resource, Some(ResourceNodeId(0)));
    assert_eq!(
        world.preview_site(PlayerId(0), extractor, 9, 9),
        Err(SiteRefusal::KnownStructure)
    );
    Ok(())
}
