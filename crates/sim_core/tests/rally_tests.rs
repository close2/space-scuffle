//! Rally points of production structures (plan `2026-11-23-production-queue` D-3 to D-7; B-60).
//!
//! The town hall's auto-rally is checked on the shipped showcase against a node computed here
//! from the map's nodes and the archetype's authored `auto_rally`, and on constructed maps for
//! the cases the showcase does not reach. No identity is named: the hall is whichever archetype
//! carries the capability.

use sim_core::scenario::load_scenario_world;
use sim_core::{SimWorld, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell, ResourceKind, ResourceNode};
use sim_types::{
    AutoRallySpec, EntityId, PlayerId, RallyTarget, ResourceNodeId, SimScalar, SimVec2, UnitTypeId,
};
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// The squared raw distance from `point` to the cells `[x, x + w] × [y, y + h]`.
fn squared_gap(point: SimVec2, cell: (u32, u32), size: (u8, u8)) -> i128 {
    let axis = |value: SimScalar, start: u32, extent: u8| {
        let raw = i128::from(value.raw());
        let low = i128::from(start) << 32;
        let high = (i128::from(start) + i128::from(extent)) << 32;
        (low - raw).max(raw - high).max(0)
    };
    let (dx, dy) = (axis(point.x, cell.0, size.0), axis(point.y, cell.1, size.1));
    dx * dx + dy * dy
}

fn squared(a: SimVec2, b: SimVec2) -> i128 {
    let dx = i128::from(a.x.raw()) - i128::from(b.x.raw());
    let dy = i128::from(a.y.raw()) - i128::from(b.y.raw());
    dx * dx + dy * dy
}

/// The node D-7 names for a hall at `centre`: standing, not empty, of an accepted kind, its
/// cells within the radius; the farthest centre, the lower id on a tie.
fn expected_node(world: &SimWorld, centre: SimVec2, spec: AutoRallySpec) -> Option<ResourceNodeId> {
    let reach = i128::from(spec.radius.raw()).pow(2);
    let mut candidates: Vec<(i128, u32)> = (0_u32..)
        .zip(world.map.resources())
        .filter_map(|(id, node)| {
            let placed = node.placed?.footprint;
            let near = squared_gap(centre, placed.top_left, (placed.width, placed.height)) <= reach;
            (near && node.remaining > 0 && spec.kinds.contains(node.kind))
                .then(|| (squared(centre, node.pos), id))
        })
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    candidates.first().map(|&(_, id)| ResourceNodeId(id))
}

/// The town halls of `world`: each entity whose archetype carries an auto-rally, with its spec.
fn halls(world: &SimWorld) -> Vec<(EntityId, AutoRallySpec)> {
    world
        .entities
        .values()
        .filter_map(|entity| {
            let spec = world
                .catalog
                .get(entity.unit_type_id)
                .ok()?
                .capabilities
                .auto_rally?;
            Some((entity.id, spec))
        })
        .collect()
}

fn node(x: u32, y: u32, kind: ResourceKind) -> ResourceNode {
    ResourceNode {
        pos: SimVec2::new(
            SimScalar::from_num(x) + SimScalar::HALF,
            SimScalar::from_num(y) + SimScalar::HALF,
        ),
        kind,
        remaining: 1500,
        cell: (x, y),
        placed: None,
    }
}

/// A 32 × 32 ground world with `nodes` and the shipped catalog, and a hall type.
fn constructed(
    nodes: Vec<ResourceNode>,
) -> Result<(SimWorld, UnitTypeId), Box<dyn std::error::Error>> {
    let catalogs =
        sim_core::catalog::RuntimeCatalogs::load_from_directory(assets().join("catalog"))?;
    let hall = catalogs
        .units
        .archetypes()
        .find(|arch| arch.capabilities.auto_rally.is_some())
        .ok_or("the shipped catalog has no auto-rallying structure")?
        .unit_type_id;
    let map = GameMap::from_cells(
        32,
        32,
        vec![MapCell::default_ground(CliffLevel(0)); 32 * 32],
        nodes,
        vec![],
    )?;
    let world = SimWorld::with_catalogs(map, 7, catalogs.units, UpgradeCatalog::default())?;
    Ok((world, hall))
}

/// WP1 (D-7, S6): every showcase town hall rallies to the farthest mineral field whose cells
/// come within the authored radius; the expected node is computed from the map.
#[test]
fn a_town_hall_rallies_to_the_farthest_mineral_field_in_reach() -> TestResult {
    let world = load_scenario_world(&assets(), Path::new("scenarios/showcase.ron"))?;
    let halls = halls(&world);
    assert_eq!(halls.len(), 2, "one hall per showcase start");
    for (hall, spec) in halls {
        let centre = world.entities[&hall].position;
        let expected = expected_node(&world, centre, spec).ok_or("no field in reach")?;
        let chosen = world.map.resources()[expected.0 as usize].kind;
        assert_eq!(chosen, ResourceKind::Mineral);
        assert_eq!(
            world.entities[&hall].rally,
            Some(RallyTarget::Node(expected)),
            "hall {hall:?}"
        );
    }
    Ok(())
}

/// WP1 (D-7): a hall with no field within its radius has no rally, and a geyser is never chosen,
/// even when it is in reach and farther than the field.
#[test]
fn a_hall_without_a_field_in_reach_has_no_rally_and_a_geyser_is_never_chosen() -> TestResult {
    // A hall of the shipped footprint at (2, 2); a field far away.
    let (mut world, hall_type) = constructed(vec![node(26, 26, ResourceKind::Mineral)])?;
    let hall = world.spawn_structure(hall_type, PlayerId(0), 2, 2)?;
    assert_eq!(world.entities[&hall].rally, None);
    // A field and a farther geyser, both within reach of a hall at (12, 12).
    let (mut world, hall_type) = constructed(vec![
        node(14, 19, ResourceKind::Mineral),
        node(20, 19, ResourceKind::Gas),
    ])?;
    let hall = world.spawn_structure(hall_type, PlayerId(0), 12, 12)?;
    let centre = world.entities[&hall].position;
    let geyser = world.map.resources()[1].clone();
    let field = world.map.resources()[0].clone();
    let spec = world
        .catalog
        .get(hall_type)?
        .capabilities
        .auto_rally
        .ok_or("hall without auto-rally")?;
    let reach = i128::from(spec.radius.raw()).pow(2);
    let geyser_cells = geyser.placed.ok_or("geyser not placed")?.footprint;
    assert!(
        squared_gap(
            centre,
            geyser_cells.top_left,
            (geyser_cells.width, geyser_cells.height)
        ) <= reach,
        "the geyser is in reach"
    );
    assert!(squared(centre, geyser.pos) > squared(centre, field.pos));
    assert_eq!(
        world.entities[&hall].rally,
        Some(RallyTarget::Node(ResourceNodeId(0)))
    );
    // A geyser alone gives no rally.
    let (mut world, hall_type) = constructed(vec![node(20, 19, ResourceKind::Gas)])?;
    let hall = world.spawn_structure(hall_type, PlayerId(0), 12, 12)?;
    assert_eq!(world.entities[&hall].rally, None);
    Ok(())
}
