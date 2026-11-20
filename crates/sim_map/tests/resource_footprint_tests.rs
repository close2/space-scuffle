//! Resource node footprints in `sim_map`: occupancy, placement, removal and the navigation mesh
//! (plan `2026-11-20-resource-footprints` WP4).
//!
//! # Responsibility
//! Proves D-4, D-5 (M1, M2, M4, P1), D-6 and D-9 through the public API: placing a node marks its
//! cells and removal restores them exactly; placement refuses each defect with a typed error; a
//! standing node is closed to every route request whatever the requester knows; routes round a
//! mineral line and pass its notches at a worker's radius; the geyser's recessed sides admit a
//! disc the hull would refuse; an extractor placed over a geyser leaves the geyser when it goes;
//! the mesh is independent of placement order and returns to the terrain mesh on removal; and
//! `validate_resource_footprints` reports each defect.
//!
//! # How it fits
//! A test table stands in for the catalog (which `sim_map` cannot read): a 2×1 box kind and the
//! 5.0.14 geyser's 16-gon kind, as `placement_rules.ron` authors them. The digest pin uses the
//! shipped showcase map.
//!
//! # Invariants
//! The digest test embeds the shipped map bytes so it also runs under `wasm32-wasip1`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test maps are at most 64 cells a side; a broken law must fail the test loudly"
)]

use sim_map::validation::validate_resource_footprints;
use sim_map::{
    CellFlags, CliffLevel, FootprintOccupant, GameMap, MapCell, MapError, MapFile,
    MapValidationError, NavMesh, PathRequest, PathRequestWithVision, Pathfinder,
    PlacementFailureReason, PlayerVisionMap, ResourceFootprint, ResourceFootprintFailure,
    ResourceFootprints, ResourceKind, ResourceNode, StructureFootprint, TriangleKind,
};
use sim_types::{EntityId, FootprintShape, QuarterOutline, ResourceNodeId, SimScalar, SimVec2};

/// The 5.0.14 geyser outline (void Footprint:18-19) in quarter cells.
fn geyser_outline() -> FootprintShape {
    FootprintShape::Outline(
        QuarterOutline::new(&[
            (-2, -4),
            (-3, -5),
            (-5, -3),
            (-4, -2),
            (-4, 2),
            (-5, 3),
            (-3, 5),
            (-2, 4),
            (2, 4),
            (3, 5),
            (5, 3),
            (4, 2),
            (4, -2),
            (5, -3),
            (3, -5),
            (2, -4),
        ])
        .expect("invariant: sixteen vertices fit"),
    )
}

/// The test table: minerals a 2×1 box anchored on the west cell, gas the 3×3 16-gon centred on
/// its anchor.
fn table() -> ResourceFootprints {
    ResourceFootprints::new(vec![
        (
            ResourceKind::Mineral,
            ResourceFootprint {
                cells: (0, 0, 2, 1),
                shape: FootprintShape::full(2, 1),
                remove_when_empty: true,
            },
        ),
        (
            ResourceKind::Gas,
            ResourceFootprint {
                cells: (-1, -1, 3, 3),
                shape: geyser_outline(),
                remove_when_empty: false,
            },
        ),
    ])
}

/// A node of `kind` anchored at `cell`, not placed.
fn node(kind: ResourceKind, cell: (u32, u32)) -> ResourceNode {
    let half = |v: u32| SimScalar::from_raw((i64::from(v) << 32) + (1 << 31));
    ResourceNode {
        pos: SimVec2::new(half(cell.0), half(cell.1)),
        kind,
        remaining: 1_500,
        cell,
        placed: None,
    }
}

/// A flat open `side` × `side` map with `nodes`.
fn flat(side: u32, nodes: Vec<ResourceNode>) -> GameMap {
    let cells = vec![MapCell::default_ground(CliffLevel(1)); (side * side) as usize];
    GameMap::from_cells(side, side, cells, nodes, Vec::new()).expect("invariant: a valid size")
}

/// A point in sixteenths of a cell.
fn at16(x: i64, y: i64) -> SimVec2 {
    SimVec2::new(SimScalar::from_raw(x << 28), SimScalar::from_raw(y << 28))
}

fn mesh_of(map: &GameMap) -> NavMesh {
    NavMesh::build(map).expect("invariant: small maps build")
}

#[test]
fn placing_marks_the_cells_and_removal_restores_them() {
    let mut map = flat(
        16,
        vec![
            node(ResourceKind::Mineral, (3, 4)),
            node(ResourceKind::Gas, (10, 10)),
        ],
    );
    let before: Vec<MapCell> = map.cells().to_vec();
    map.place_resource_footprints(&table())
        .expect("invariant: open ground");
    let mineral_cells = [(3, 4), (4, 4)];
    for y in 0..16 {
        for x in 0..16 {
            let cell = map.cell(x, y).expect("on the map");
            let mineral = mineral_cells.contains(&(x, y));
            let gas = (9..=11).contains(&x) && (9..=11).contains(&y);
            let expected = if mineral {
                Some(ResourceNodeId(0))
            } else if gas {
                Some(ResourceNodeId(1))
            } else {
                None
            };
            assert_eq!(cell.occupying_resource, expected, "({x}, {y})");
            assert_eq!(
                cell.flags.contains(CellFlags::OCCUPIED_BY_RESOURCE),
                expected.is_some()
            );
            assert_eq!(
                cell.flags
                    .intersects(CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE),
                expected.is_none()
            );
        }
    }
    // The field's position is its footprint's centre, (4, 4.5); the geyser's its cell centre.
    let field = &map.resources()[0];
    assert_eq!(field.pos, at16(64, 72));
    let placed = field.placed.expect("the field stands");
    assert_eq!(
        (
            placed.footprint.top_left,
            placed.footprint.width,
            placed.footprint.height
        ),
        ((3, 4), 2, 1)
    );
    assert_eq!(map.resources()[1].pos, at16(168, 168));
    // Removal restores every cell's flags and occupant exactly.
    map.remove_resource_footprint(ResourceNodeId(0))
        .expect("invariant: placed above");
    map.remove_resource_footprint(ResourceNodeId(1))
        .expect("invariant: placed above");
    assert_eq!(map.cells(), before.as_slice());
    assert!(map.resources().iter().all(|r| r.placed.is_none()));
    assert_eq!(
        map.remove_resource_footprint(ResourceNodeId(0)),
        Err(MapError::ResourceFootprint {
            node: ResourceNodeId(0),
            reason: ResourceFootprintFailure::NotPlaced
        })
    );
}

#[test]
fn placement_refuses_overlap_bounds_terrain_and_a_second_placement() {
    let refused = |nodes: Vec<ResourceNode>, edit: &dyn Fn(&mut GameMap)| {
        let mut map = flat(16, nodes);
        edit(&mut map);
        map.place_resource_footprints(&table())
    };
    let none = |_: &mut GameMap| {};
    let fail = |node: u32, reason| {
        Err(MapError::ResourceFootprint {
            node: ResourceNodeId(node),
            reason,
        })
    };
    // Out of bounds: a field on the east edge, a geyser on the south edge.
    assert_eq!(
        refused(vec![node(ResourceKind::Mineral, (15, 3))], &none),
        fail(0, ResourceFootprintFailure::OutOfBounds)
    );
    assert_eq!(
        refused(vec![node(ResourceKind::Gas, (5, 0))], &none),
        fail(0, ResourceFootprintFailure::OutOfBounds)
    );
    // Overlap: a field whose east cell is the next field's anchor.
    assert_eq!(
        refused(
            vec![
                node(ResourceKind::Mineral, (3, 3)),
                node(ResourceKind::Mineral, (4, 3)),
            ],
            &none
        ),
        fail(1, ResourceFootprintFailure::OverlapsNode(ResourceNodeId(0)))
    );
    // Terrain: an unbuildable cell, a ramp, another cliff level.
    let unbuildable = |map: &mut GameMap| {
        map.cell_mut(4, 3)
            .expect("on the map")
            .flags
            .remove(CellFlags::BUILDABLE);
    };
    assert_eq!(
        refused(vec![node(ResourceKind::Mineral, (3, 3))], &unbuildable),
        fail(0, ResourceFootprintFailure::NotOpenGround { x: 4, y: 3 })
    );
    let ramp = |map: &mut GameMap| {
        let cell = map.cell_mut(4, 3).expect("on the map");
        cell.flags.insert(CellFlags::RAMP);
    };
    assert_eq!(
        refused(vec![node(ResourceKind::Mineral, (3, 3))], &ramp),
        fail(0, ResourceFootprintFailure::NotOpenGround { x: 4, y: 3 })
    );
    let cliff = |map: &mut GameMap| {
        map.cell_mut(4, 3).expect("on the map").cliff_level = CliffLevel(2);
    };
    assert_eq!(
        refused(vec![node(ResourceKind::Mineral, (3, 3))], &cliff),
        fail(
            0,
            ResourceFootprintFailure::CliffLevelMismatch { x: 4, y: 3 }
        )
    );
    // A structure in the way.
    let structure = |map: &mut GameMap| {
        map.place_structure(EntityId(9), (4, 2), StructureFootprint::square(2, 2))
            .expect("invariant: open ground");
    };
    assert_eq!(
        refused(vec![node(ResourceKind::Mineral, (3, 3))], &structure),
        fail(0, ResourceFootprintFailure::OverlapsStructure(EntityId(9)))
    );
    // No footprint for the kind.
    let mut map = flat(16, vec![node(ResourceKind::Gas, (5, 5))]);
    assert_eq!(
        map.place_resource_footprints(&ResourceFootprints::NONE),
        fail(0, ResourceFootprintFailure::NoFootprint)
    );
    // A second placement.
    map.place_resource_footprints(&table())
        .expect("invariant: open ground");
    assert_eq!(
        map.place_resource_footprints(&table()),
        fail(0, ResourceFootprintFailure::AlreadyPlaced)
    );
    // Structures may not be placed on a node's cell.
    assert_eq!(
        map.place_structure(EntityId(3), (5, 5), StructureFootprint::square(1, 1)),
        Err(MapError::PlacementBlocked {
            cell_x: 5,
            cell_y: 5,
            reason: PlacementFailureReason::OccupiedByResource(ResourceNodeId(0))
        })
    );
}

#[test]
fn a_node_is_closed_to_every_request() {
    // A corridor 12 cells wide and 3 tall; six fields fill its middle row.
    let side = 12;
    let cells = vec![MapCell::default_ground(CliffLevel(1)); (side * 3) as usize];
    let nodes = (0..6)
        .map(|i| node(ResourceKind::Mineral, (2 * i, 1)))
        .collect();
    let mut map =
        GameMap::from_cells(side, 3, cells, nodes, Vec::new()).expect("invariant: a valid size");
    // Row 0 is cut from row 2 only by the fields: with them standing no route crosses.
    let request = |vision, known| PathRequestWithVision {
        start: at16(8, 8),
        goal: at16(8, 40),
        radius: SimScalar::from_raw(1 << 30),
        vision,
        block_unexplored_ramps: false,
        known_structures: known,
    };
    let mut open = Pathfinder::new(&map);
    assert!(open.find_route_mesh(&map, &request(None, None)).is_ok());
    map.place_resource_footprints(&table())
        .expect("invariant: open ground");
    let mut pathfinder = Pathfinder::new(&map);
    let vision = PlayerVisionMap::new(side, 3);
    let nothing_known = vec![false; (side * 3) as usize];
    for (label, req) in [
        ("all known", request(None, None)),
        ("unexplored vision", request(Some(&vision), None)),
        ("no structure known", request(None, Some(&nothing_known))),
    ] {
        assert!(
            pathfinder.find_route_mesh(&map, &req).is_err(),
            "{label}: a route crossed the fields"
        );
    }
    let mesh = mesh_of(&map);
    let centre = mesh.locate(at16(16, 24)).expect("on the map");
    assert_eq!(
        mesh.label(centre).kind,
        TriangleKind::Resource(ResourceNodeId(0))
    );
}

#[test]
fn routes_round_a_mineral_line_and_through_its_notches() {
    // The showcase's slot 0 line (D-10) on a flat map: front fields at y 14 (x 3, 6, 9, 12) and
    // back fields at y 15 (x 4, 7, 10). The back field at (4, 15) is reached from the south
    // through the one-cell notch at x 5 between the front fields at x 3 and 6.
    let nodes = [
        (3, 14),
        (6, 14),
        (9, 14),
        (12, 14),
        (4, 15),
        (7, 15),
        (10, 15),
    ]
    .into_iter()
    .map(|cell| node(ResourceKind::Mineral, cell))
    .collect();
    let mut map = flat(24, nodes);
    map.place_resource_footprints(&table())
        .expect("invariant: open ground");
    let mut pathfinder = Pathfinder::new(&map);
    // The notch cell's centre (5.5, 14.5): 0.5 from both front fields and from the back field.
    let notch = at16(88, 232);
    let from_south = at16(88, 120);
    let route = |pathfinder: &mut Pathfinder, radius: SimScalar| {
        pathfinder.find_route_mesh(
            &map,
            &PathRequest {
                start: from_south,
                goal: notch,
                radius,
            }
            .into(),
        )
    };
    let worker = SimScalar::from_raw(3 << 29); // 0.375
    let reached = route(&mut pathfinder, worker).expect("a worker-sized disc reaches the notch");
    assert_eq!(reached.waypoints.last(), Some(&notch));
    // A disc wider than the notch (diameter 1.125 > 1) cannot stand in it.
    assert!(route(&mut pathfinder, SimScalar::from_raw(9 << 28)).is_err());
    // North of the line, a route from the west end to the east end rounds the line and never
    // enters a field.
    let start = at16(16, 280);
    let goal = at16(16 * 20, 200);
    let around = pathfinder
        .find_route_mesh(
            &map,
            &PathRequest {
                start,
                goal,
                radius: worker,
            }
            .into(),
        )
        .expect("the line can be rounded");
    for leg in around.waypoints.windows(2) {
        assert!(
            map.footprint_occupants_touched(leg[0], leg[1], worker)
                .all(|(_, footprint)| !footprint.capsule_meets(leg[0], leg[1], worker)),
            "leg {leg:?} meets a field"
        );
    }
}

#[test]
fn the_geyser_outline_admits_a_disc_in_its_recess() {
    let mut map = flat(16, vec![node(ResourceKind::Gas, (8, 8))]);
    map.place_resource_footprints(&table())
        .expect("invariant: open ground");
    // The geyser's centre is (8.5, 8.5); its south side is recessed to y = 7.5 over x 8 to 9.
    let terrain = SimScalar::from_raw(5 << 28); // 0.3125
    let gap = SimScalar::from_raw(1 << 16); // 2^-16
    let placeable = |centre: SimVec2| {
        map.footprint_occupants_touched(centre, centre, terrain)
            .all(|(_, footprint)| !footprint.capsule_meets(centre, centre, terrain))
    };
    let half = SimScalar::from_raw(17 << 31); // 8.5
    let recess = SimScalar::from_raw(15 << 31); // 7.5
    let in_recess = SimVec2::new(half, recess - terrain - gap);
    assert!(placeable(in_recess), "a disc just outside the recess");
    assert!(!placeable(SimVec2::new(half, recess - terrain + gap)));
    // That disc reaches above the hull's line (y = 7.25): the hull (±1.25 cut by 0.5) would
    // refuse it.
    let hull = sim_map::PlacedFootprint::new(
        (7, 7),
        StructureFootprint {
            width: 3,
            height: 3,
            shape: FootprintShape::Box {
                half_width_q: 5,
                half_height_q: 5,
                chamfer_q: 2,
            },
        },
    );
    assert!(hull.capsule_meets(in_recess, in_recess, terrain));
}

#[test]
fn an_extractor_over_a_geyser_keeps_the_geyser() {
    let mut map = flat(16, vec![node(ResourceKind::Gas, (8, 8))]);
    map.place_resource_footprints(&table())
        .expect("invariant: open ground");
    let geyser_mesh = mesh_of(&map);
    let extractor = StructureFootprint {
        width: 3,
        height: 3,
        shape: FootprintShape::Box {
            half_width_q: 6,
            half_height_q: 6,
            chamfer_q: 3,
        },
    };
    // Anything but the node's exact cells is refused.
    assert_eq!(
        map.place_structure_over_resource(
            EntityId(5),
            ResourceNodeId(0),
            StructureFootprint::square(2, 2)
        ),
        Err(MapError::NotOverResource {
            structure: EntityId(5),
            node: ResourceNodeId(0)
        })
    );
    assert_eq!(
        map.place_structure(EntityId(5), (7, 7), extractor),
        Err(MapError::PlacementBlocked {
            cell_x: 7,
            cell_y: 7,
            reason: PlacementFailureReason::OccupiedByResource(ResourceNodeId(0))
        })
    );
    map.place_structure_over_resource(EntityId(5), ResourceNodeId(0), extractor)
        .expect("invariant: the geyser stands");
    let cell = map.cell(8, 8).expect("on the map");
    assert_eq!(cell.occupying_structure, Some(EntityId(5)));
    assert_eq!(cell.occupying_resource, Some(ResourceNodeId(0)));
    // The geyser's triangles stay the geyser's; the extractor's corners beyond it are its own.
    let mesh = mesh_of(&map);
    let centre = mesh.locate(at16(136, 136)).expect("on the map");
    assert_eq!(
        mesh.label(centre).kind,
        TriangleKind::Resource(ResourceNodeId(0))
    );
    // Between the extractor's south face (y 7) and the geyser's recess (y 7.5).
    let recess = mesh.locate(at16(136, 116)).expect("on the map");
    assert_eq!(
        mesh.label(recess).kind,
        TriangleKind::Structure(EntityId(5))
    );
    // Occupants of the cell: the extractor first, then the geyser.
    let occupants: Vec<FootprintOccupant> = map
        .footprint_occupants_touched(at16(136, 136), at16(136, 136), SimScalar::ONE)
        .map(|(occupant, _)| occupant)
        .collect();
    assert_eq!(
        occupants,
        [
            FootprintOccupant::Structure(EntityId(5)),
            FootprintOccupant::Resource(ResourceNodeId(0))
        ]
    );
    // A geyser under a structure is not removed.
    assert_eq!(
        map.remove_resource_footprint(ResourceNodeId(0)),
        Err(MapError::ResourceFootprint {
            node: ResourceNodeId(0),
            reason: ResourceFootprintFailure::UnderStructure
        })
    );
    map.remove_structure(EntityId(5))
        .expect("invariant: placed above");
    assert_eq!(mesh_of(&map), geyser_mesh);
    assert!(map.resources()[0].placed.is_some());
}

#[test]
fn removing_a_node_restores_the_terrain_mesh() {
    let nodes = vec![
        node(ResourceKind::Mineral, (3, 3)),
        node(ResourceKind::Gas, (10, 10)),
    ];
    let mut map = flat(16, nodes);
    let terrain = mesh_of(&map);
    map.place_resource_footprints(&table())
        .expect("invariant: open ground");
    let standing = mesh_of(&map);
    assert_ne!(standing.digest(), terrain.digest());
    map.remove_resource_footprint(ResourceNodeId(0))
        .expect("invariant: placed");
    map.remove_resource_footprint(ResourceNodeId(1))
        .expect("invariant: placed");
    let restored = mesh_of(&map);
    assert_eq!(restored, terrain);
    assert_eq!(restored.digest(), terrain.digest());
}

#[test]
fn the_mesh_does_not_depend_on_placement_order() {
    let anchors = [
        (ResourceKind::Mineral, (2, 2)),
        (ResourceKind::Mineral, (5, 3)),
        (ResourceKind::Gas, (10, 4)),
        (ResourceKind::Mineral, (14, 9)),
        (ResourceKind::Gas, (5, 12)),
    ];
    let structures = [((8, 8), 2_u8), ((12, 13), 3), ((1, 6), 1)];
    let build = |node_order: &[usize], structure_order: &[usize], nodes_first: bool| {
        let nodes = node_order
            .iter()
            .map(|&i| node(anchors[i].0, anchors[i].1))
            .collect();
        let mut map = flat(18, nodes);
        let place_structures = |map: &mut GameMap| {
            for &i in structure_order {
                let (cell, size) = structures[i];
                map.place_structure(
                    EntityId(100 + i as u32),
                    cell,
                    StructureFootprint::square(size, size),
                )
                .expect("invariant: open ground");
            }
        };
        if !nodes_first {
            place_structures(&mut map);
        }
        map.place_resource_footprints(&table())
            .expect("invariant: open ground");
        if nodes_first {
            place_structures(&mut map);
        }
        // Node ids follow the order given; compare geometry, not labels' node ids.
        let mesh = mesh_of(&map);
        (0..mesh.triangle_count() as u32)
            .map(|t| {
                let corners = mesh.triangle_vertices(t).map(|v| mesh.vertex(v));
                let kind = match mesh.label(t).kind {
                    TriangleKind::Resource(n) => {
                        let anchor = map.resources()[n.0 as usize].cell;
                        format!("node at {anchor:?}")
                    }
                    other => format!("{other:?}"),
                };
                (corners.map(|c| (c.x, c.y)), kind)
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    let reference = build(&[0, 1, 2, 3, 4], &[0, 1, 2], true);
    assert_eq!(build(&[4, 2, 0, 3, 1], &[2, 0, 1], true), reference);
    assert_eq!(build(&[3, 1, 4, 0, 2], &[1, 2, 0], false), reference);
}

#[test]
fn validate_resource_footprints_reports_each_error() {
    let text = include_str!("../../../assets/maps/default_arena.scmap.ron");
    let shipped = MapFile::from_ron_str(text)
        .and_then(|file| file.decode())
        .expect("invariant: the shipped arena decodes");
    let no_ring = |_: ResourceKind, _: i64, _: i64| false;
    assert_eq!(
        validate_resource_footprints(&shipped, &table(), no_ring),
        Vec::new()
    );
    let start = shipped.start_locations[0].clone();
    let mut doc = shipped.clone();
    // A field overlapping the next, one under the first start, one enclosed by the map's
    // corner and two fields, and a node of a kind with no footprint.
    let base_id = doc.resources.iter().map(|r| r.id.0).max().unwrap_or(0) + 1;
    let mut add = |kind, cell| {
        let id = sim_map::MapEntityId(base_id + doc.resources.len() as u32);
        doc.resources.push(sim_map::MapResource {
            id,
            kind,
            cell,
            amount: 100,
        });
        id
    };
    let first = add(ResourceKind::Mineral, (30, 20));
    let overlapping = add(ResourceKind::Mineral, (31, 20));
    let under_start = add(ResourceKind::Mineral, (start.cell.0, start.cell.1));
    let enclosed = add(ResourceKind::Mineral, (0, 63));
    let _left = add(ResourceKind::Mineral, (2, 63));
    let _below = add(ResourceKind::Mineral, (0, 62));
    let _below_east = add(ResourceKind::Mineral, (2, 62));
    let errors = validate_resource_footprints(&doc, &table(), no_ring);
    let first_index = doc
        .resources
        .iter()
        .position(|r| r.id == first)
        .expect("added");
    assert!(
        errors.contains(&MapValidationError::ResourceFootprintInvalid {
            id: overlapping,
            reason: ResourceFootprintFailure::OverlapsNode(ResourceNodeId(first_index as u32)),
        })
    );
    assert!(
        errors.contains(&MapValidationError::ResourceFootprintUnderStart {
            id: under_start,
            start: start.id,
        })
    );
    assert!(
        errors.contains(&MapValidationError::ResourceEnclosed { id: enclosed }),
        "{errors:?}"
    );
    let gas_only = ResourceFootprints::new(
        table()
            .iter()
            .filter(|(kind, _)| *kind == ResourceKind::Gas)
            .copied()
            .collect(),
    );
    let missing = validate_resource_footprints(&shipped, &gas_only, no_ring);
    assert!(missing.iter().all(|e| matches!(
        e,
        MapValidationError::ResourceFootprintInvalid {
            reason: ResourceFootprintFailure::NoFootprint,
            ..
        }
    )));
    assert_eq!(
        missing.len(),
        shipped
            .resources
            .iter()
            .filter(|r| r.kind == ResourceKind::Mineral)
            .count()
    );
    // A ring that reaches every cell puts every start in every node's ring.
    let everywhere = |_: ResourceKind, _: i64, _: i64| true;
    let rings = validate_resource_footprints(&shipped, &table(), everywhere);
    assert_eq!(
        rings.len(),
        shipped.start_locations.len() * shipped.resources.len()
    );
    assert!(rings
        .iter()
        .all(|e| matches!(e, MapValidationError::StartInResourceRing { .. })));
}

/// The digest of the shipped showcase's mesh with its 20 nodes placed by the test table (the
/// shipped footprints of `placement_rules.ron`), computed by this test on the tree of plan
/// `2026-11-20-resource-footprints` WP4, where nodes first cut the mesh; no earlier value exists.
const SHOWCASE_RESOURCE_MESH_DIGEST: u64 = 0x376d_17a5_a368_0858;

#[test]
fn digest_of_the_showcase_mesh_with_its_nodes_is_pinned() {
    let text = include_str!("../../../assets/maps/showcase_canyon.scmap.ron");
    let mut map = MapFile::from_ron_str(text)
        .and_then(|file| file.decode())
        .and_then(|document| document.to_game_map())
        .expect("invariant: the shipped showcase map decodes");
    let terrain = mesh_of(&map);
    map.place_resource_footprints(&table())
        .expect("invariant: the shipped nodes fit");
    assert_eq!(map.resources().len(), 20);
    let mesh = mesh_of(&map);
    assert_ne!(mesh.digest(), terrain.digest());
    // Labels follow the outlines: a triangle is a node's exactly when its centroid lies inside
    // that node's outline.
    for t in 0..mesh.triangle_count() as u32 {
        let (sx, sy) = mesh
            .triangle_vertices(t)
            .map(|v| mesh.vertex(v))
            .iter()
            .fold((0, 0), |(x, y), p| (x + i64::from(p.x), y + i64::from(p.y)));
        let inside: Vec<ResourceNodeId> = (0_u32..)
            .zip(map.resources())
            .filter(|(_, r)| {
                r.placed
                    .is_some_and(|p| p.footprint.contains_open_scaled(sx, sy, 3))
            })
            .map(|(i, _)| ResourceNodeId(i))
            .collect();
        match mesh.label(t).kind {
            TriangleKind::Resource(node) => assert_eq!(inside, [node], "triangle {t}"),
            _ => assert!(inside.is_empty(), "triangle {t} lies in {inside:?}"),
        }
    }
    assert_eq!(
        mesh.digest(),
        SHOWCASE_RESOURCE_MESH_DIGEST,
        "digest {:#x}",
        mesh.digest()
    );
}
