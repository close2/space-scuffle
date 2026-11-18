//! Integration tests of the constrained Delaunay navigation mesh (sc2 pathfinding WP5).
//!
//! # Responsibility
//! Checks the mesh's construction laws through the public API: exact predicates on adversarial
//! input, a valid counter-clockwise triangulation that tiles the map, the constrained Delaunay
//! property, every constraint run present as an edge, labels that agree with the terrain cell by
//! cell, independence from structure insertion order, exact restoration after a structure is
//! removed, and a pinned digest of the showcase mesh.
//!
//! # How it fits
//! The mesh is latent (plan `docs/plans/2026-11-09-sc2-pathfinding.md`, WP5): the search and the
//! segment predicate are tested in `funnel_tests.rs`, `pathfinding_properties.rs` and
//! `reachability_tests.rs`.
//!
//! # Invariants
//! The digest test embeds the shipped map bytes so it also runs under `wasm32-wasip1`, where the
//! file system is not mapped: `cargo test -p sim_map --test navmesh_tests --target wasm32-wasip1
//! -- digest`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::panic,
    clippy::many_single_char_names,
    reason = "test maps are at most 64 cells a side, so every index fits every integer type; \
              a broken mesh law must fail the test loudly"
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/arena.rs"]
mod arena;

use proptest::collection::vec;
use proptest::prelude::*;
use sim_map::navmesh::predicates::{
    incircle, orient2d, LatticePoint, LATTICE_PER_CELL, NAV_MESH_MAX_DIMENSION,
};
use sim_map::{CellFlags, CliffLevel, EdgeKind, GameMap, MapCell, MapFile, NavMesh, TriangleKind};
use sim_types::{EntityId, SimScalar, SimVec2};

const NONE: u32 = u32::MAX;

fn mesh_of(map: &GameMap) -> NavMesh {
    NavMesh::build(map).expect("invariant: test maps are far below the mesh size limit")
}

fn masked_map(side: u32, blocked: &[bool]) -> GameMap {
    let mut map = GameMap::new(side, side, MapCell::default_ground(CliffLevel(1)))
        .expect("invariant: a positive side is a valid map size");
    for (cell, &is_blocked) in map.cells_mut().iter_mut().zip(blocked) {
        if is_blocked {
            cell.flags = CellFlags::empty();
        }
    }
    map
}

/// Topology, orientation, tiling and constrained Delaunay laws of any mesh.
fn check_mesh_laws(mesh: &NavMesh) -> Result<(), String> {
    let mut doubled_area = 0i64;
    for t in 0..mesh.triangle_count() as u32 {
        let v = mesh.triangle_vertices(t).map(|i| mesh.vertex(i));
        let area = orient2d(v[0], v[1], v[2]);
        if area <= 0 {
            return Err(format!("triangle {t} is not counter-clockwise: {v:?}"));
        }
        doubled_area += area;
        let neighbours = mesh.triangle_neighbors(t);
        let kinds = mesh.edge_kinds(t);
        for k in 0..3 {
            let (p, q) = (
                mesh.triangle_vertices(t)[(k + 1) % 3],
                mesh.triangle_vertices(t)[(k + 2) % 3],
            );
            let n = neighbours[k];
            if n == NONE {
                if kinds[k] != EdgeKind::Wall {
                    return Err(format!("hull edge of {t} is not a wall"));
                }
                continue;
            }
            let nv = mesh.triangle_vertices(n);
            let Some(j) = (0..3).find(|&j| nv[j] != p && nv[j] != q) else {
                return Err(format!("{t} and {n} share more than an edge"));
            };
            if mesh.triangle_neighbors(n)[j] != t || mesh.edge_kinds(n)[j] != kinds[k] {
                return Err(format!("edge {t}/{k} and {n}/{j} disagree"));
            }
            if !nv.contains(&p) || !nv.contains(&q) {
                return Err(format!("{n} does not contain the edge it borders"));
            }
            if kinds[k] == EdgeKind::Free && incircle(v[0], v[1], v[2], mesh.vertex(nv[j])) > 0 {
                return Err(format!(
                    "unconstrained edge {t}/{k} is not locally Delaunay"
                ));
            }
        }
    }
    // Doubled area in squared lattice units (quarter cells).
    let per_cell = i64::from(LATTICE_PER_CELL);
    let expected = 2 * per_cell * per_cell * i64::from(mesh.width()) * i64::from(mesh.height());
    if doubled_area != expected {
        return Err(format!(
            "triangles cover {doubled_area}/2, map is {expected}/2"
        ));
    }
    Ok(())
}

fn has_edge(mesh: &NavMesh, a: LatticePoint, b: LatticePoint, kind: EdgeKind) -> bool {
    (0..mesh.triangle_count() as u32).any(|t| {
        let v = mesh.triangle_vertices(t);
        (0..3).any(|k| {
            let (p, q) = (mesh.vertex(v[(k + 1) % 3]), mesh.vertex(v[(k + 2) % 3]));
            ((p, q) == (a, b) || (p, q) == (b, a)) && mesh.edge_kinds(t)[k] == kind
        })
    })
}

fn check_runs(mesh: &NavMesh) -> Result<(), String> {
    for run in mesh.constraint_runs() {
        if !has_edge(mesh, run.from, run.to, run.kind) {
            return Err(format!("constraint run {run:?} is not a mesh edge"));
        }
    }
    Ok(())
}

/// Every cell centre lies in a triangle whose label is the cell's terrain.
fn check_labels(map: &GameMap, mesh: &NavMesh) -> Result<(), String> {
    for y in 0..map.height() {
        for x in 0..map.width() {
            let centre = map.cell_to_world_center(x, y).map_err(|e| e.to_string())?;
            let t = mesh.locate(centre).ok_or("cell centre off the mesh")?;
            let label = mesh.label(t);
            let cell = map.cell(x, y).map_err(|e| e.to_string())?;
            let usable = map.terrain_step_is_connected((x, y), (x, y));
            let expected = if !usable {
                TriangleKind::Blocked
            } else if let Some(id) = cell.occupying_structure {
                TriangleKind::Structure(id)
            } else {
                TriangleKind::Open
            };
            if label.kind != expected {
                return Err(format!(
                    "cell ({x}, {y}): {expected:?} but triangle says {label:?}"
                ));
            }
            if usable && (label.cliff_level != cell.cliff_level.0 || label.ramp != cell.ramp) {
                return Err(format!(
                    "cell ({x}, {y}) tag differs from triangle {label:?}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn predicates_are_exact_on_adversarial_input() {
    let p = LatticePoint::new;
    // Collinear points far apart and close together.
    assert_eq!(orient2d(p(0, 0), p(1023, 1023), p(512, 512)), 0);
    assert!(orient2d(p(0, 0), p(1024, 1023), p(512, 511)) < 0);
    // Cocircular lattice points: a square and a 3-4-5 set on the same circle.
    assert_eq!(incircle(p(0, 0), p(1024, 0), p(1024, 1024), p(0, 1024)), 0);
    assert_eq!(incircle(p(5, 0), p(0, 5), p(-5, 0), p(3, -4)), 0);
    assert!(incircle(p(5, 0), p(0, 5), p(-5, 0), p(3, -3)) > 0);
    assert!(incircle(p(5, 0), p(0, 5), p(-5, 0), p(4, -4)) < 0);
    // A sliver at the largest supported coordinate stays exact.
    let m = NAV_MESH_MAX_DIMENSION as i32;
    assert!(orient2d(p(0, 0), p(m, m - 1), p(m - 1, m - 2)) != 0);
    assert!(incircle(p(0, 0), p(m, 0), p(m, m), p(m - 1, 1)) > 0);
}

#[test]
fn shipped_maps_satisfy_every_mesh_law() {
    for file in [arena::DEFAULT_ARENA_FILE, arena::SHOWCASE_CANYON_FILE] {
        let map = arena::load_game_map(file);
        let mesh = mesh_of(&map);
        check_mesh_laws(&mesh).unwrap_or_else(|e| panic!("{file}: {e}"));
        check_runs(&mesh).unwrap_or_else(|e| panic!("{file}: {e}"));
        check_labels(&map, &mesh).unwrap_or_else(|e| panic!("{file}: {e}"));
    }
}

#[test]
fn an_open_map_is_two_triangles() {
    let map = masked_map(8, &[false; 64]);
    let mesh = mesh_of(&map);
    assert_eq!(mesh.vertex_count(), 4);
    assert_eq!(mesh.triangle_count(), 2);
}

#[test]
fn structure_order_does_not_matter_and_removal_restores_the_terrain_mesh() {
    let terrain = arena::default_arena();
    let base = mesh_of(&terrain);
    let (a, b) = (EntityId(9_001), EntityId(9_002));
    let mut forward = terrain.clone();
    forward
        .place_structure(a, (10, 10), sim_map::StructureFootprint::square(3, 3))
        .expect("invariant: open ground");
    forward
        .place_structure(b, (13, 10), sim_map::StructureFootprint::square(2, 2))
        .expect("invariant: open ground");
    let mut backward = terrain.clone();
    backward
        .place_structure(b, (13, 10), sim_map::StructureFootprint::square(2, 2))
        .expect("invariant: open ground");
    backward
        .place_structure(a, (10, 10), sim_map::StructureFootprint::square(3, 3))
        .expect("invariant: open ground");
    let with_both = mesh_of(&forward);
    assert_eq!(with_both, mesh_of(&backward));
    assert_ne!(with_both, base);
    check_mesh_laws(&with_both).unwrap_or_else(|e| panic!("{e}"));
    check_labels(&forward, &with_both).unwrap_or_else(|e| panic!("{e}"));
    forward
        .remove_structure(a)
        .expect("invariant: placed above");
    forward
        .remove_structure(b)
        .expect("invariant: placed above");
    let restored = mesh_of(&forward);
    assert_eq!(restored, base);
    assert_eq!(restored.digest(), base.digest());
}

#[test]
fn pathfinder_rebuilds_its_mesh_on_structure_change() {
    let mut map = arena::default_arena();
    let mut pathfinder = sim_map::Pathfinder::new(&map);
    let before = pathfinder.navmesh().map(NavMesh::digest);
    map.place_structure(
        EntityId(77),
        (20, 20),
        sim_map::StructureFootprint::square(2, 2),
    )
    .expect("invariant: open ground");
    pathfinder.recompute_clearance(&map);
    let placed = pathfinder.navmesh().map(NavMesh::digest);
    assert_ne!(before, placed);
    map.remove_structure(EntityId(77))
        .expect("invariant: placed above");
    pathfinder.recompute_clearance(&map);
    assert_eq!(pathfinder.navmesh().map(NavMesh::digest), before);
}

/// The showcase mesh digest, pinned. It must be identical on every target, including
/// `wasm32-wasip1`; a change to the construction must re-pin it deliberately.
///
/// Re-pinned for plan `2026-11-18-contour-footprints` `WP1a`: the digest now eats world coordinates
/// and reduced widths; the mesh is unchanged.
const SHOWCASE_MESH_DIGEST: u64 = 0x4a14_9269_337f_3464;

#[test]
fn digest_of_the_showcase_mesh_is_pinned() {
    let text = include_str!("../../../assets/maps/showcase_canyon.scmap.ron");
    let map = MapFile::from_ron_str(text)
        .and_then(|file| file.decode())
        .and_then(|document| document.to_game_map())
        .expect("invariant: the shipped showcase map decodes");
    let mesh = mesh_of(&map);
    assert_eq!(
        mesh.digest(),
        SHOWCASE_MESH_DIGEST,
        "digest {:#x}",
        mesh.digest()
    );
    assert_eq!(
        mesh_of(&map),
        mesh,
        "construction is deterministic within a run"
    );
}

#[test]
fn a_point_on_a_vertex_or_edge_locates_to_a_touching_triangle() {
    let map = arena::default_arena();
    let mesh = mesh_of(&map);
    for v in 0..mesh.vertex_count() as u32 {
        let p = mesh.vertex(v);
        let Some(t) = mesh.locate(p.to_sim()) else {
            panic!("vertex {p:?} is on the map");
        };
        let corners = mesh.triangle_vertices(t).map(|i| mesh.vertex(i));
        assert!((0..3).all(|k| orient2d(corners[(k + 1) % 3], corners[(k + 2) % 3], p) >= 0));
    }
    assert!(mesh
        .locate(SimVec2::new(-SimScalar::ONE, SimScalar::ONE))
        .is_none());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn random_masks_give_valid_constrained_delaunay_meshes(
        blocked in vec((0..4u8).prop_map(|roll| roll == 0), 20 * 20),
    ) {
        let map = masked_map(20, &blocked);
        let mesh = mesh_of(&map);
        check_mesh_laws(&mesh).map_err(TestCaseError::fail)?;
        check_runs(&mesh).map_err(TestCaseError::fail)?;
        check_labels(&map, &mesh).map_err(TestCaseError::fail)?;
    }

    #[test]
    fn rebuilding_after_place_and_remove_equals_a_fresh_build(
        blocked in vec((0..6u8).prop_map(|roll| roll == 0), 20 * 20),
        x in 1u32..16, y in 1u32..16, w in 1u8..4, h in 1u8..4,
    ) {
        let mut map = masked_map(20, &blocked);
        let fresh = mesh_of(&map);
        if map.place_structure(EntityId(5), (x, y), sim_map::StructureFootprint::square(w, h)).is_ok() {
            let placed = mesh_of(&map);
            check_mesh_laws(&placed).map_err(TestCaseError::fail)?;
            check_labels(&map, &placed).map_err(TestCaseError::fail)?;
            map.remove_structure(EntityId(5)).map_err(|e| TestCaseError::fail(e.to_string()))?;
        }
        prop_assert_eq!(mesh_of(&map), fresh);
    }
}

/// A seeded draw in `[low, high)` (`SplitMix64`), so the property below is reproducible on every
/// target without a random-number crate.
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

/// T-13 of plan `2026-11-18-inner-radius` (property): a smaller disc routes wherever a larger
/// one does. On both shipped maps, bare and with seeded 2 × 2 structures, 500 seeded start and
/// goal pairs are searched, each at one of the radius pairs of the terrain radius in turn
/// (0.3125 against 0.375, 0.375 against 0.5625, 0.5 against 0.625, 0.5625 against 1.0, in world
/// units; 125 pairs each, which keeps the target fast): whenever the larger disc has a route,
/// the smaller one has one too. The terrain radius only ever shrinks
/// a body against terrain, so this is what keeps it from losing a route the collision radius
/// had.
#[test]
fn a_smaller_disc_routes_wherever_a_larger_one_does() {
    const PAIRS: [(f64, f64); 4] = [
        (0.3125, 0.375),
        (0.375, 0.5625),
        (0.5, 0.625),
        (0.5625, 1.0),
    ];
    let mut rng = 0x1B_116_u64;
    let mut both = 0_u32;
    for file in [arena::DEFAULT_ARENA_FILE, arena::SHOWCASE_CANYON_FILE] {
        for with_structures in [false, true] {
            let mut map = arena::load_game_map(file);
            let (width, height) = (f64::from(map.width()), f64::from(map.height()));
            if with_structures {
                let mut placed = 0;
                for index in 0..24_u32 {
                    let x = draw(&mut rng, 1.0, width - 3.0) as u32;
                    let y = draw(&mut rng, 1.0, height - 3.0) as u32;
                    // A site on a cliff or ramp is refused; the others stand.
                    placed += u32::from(
                        map.place_structure(
                            EntityId(100 + index),
                            (x, y),
                            sim_map::StructureFootprint::square(2, 2),
                        )
                        .is_ok(),
                    );
                }
                assert!(placed >= 8, "{file}: only {placed} structures stand");
            }
            let mut pathfinder = sim_map::Pathfinder::new(&map);
            for index in 0..500 {
                let start = SimVec2::new(
                    SimScalar::from_num(draw(&mut rng, 0.0, width)),
                    SimScalar::from_num(draw(&mut rng, 0.0, height)),
                );
                let goal = SimVec2::new(
                    SimScalar::from_num(draw(&mut rng, 0.0, width)),
                    SimScalar::from_num(draw(&mut rng, 0.0, height)),
                );
                {
                    let (small, large) = PAIRS[index % PAIRS.len()];
                    let request = |radius: f64| sim_map::PathRequest {
                        start,
                        goal,
                        radius: SimScalar::from_num(radius),
                    };
                    if pathfinder.find_path(&map, &request(large)).is_ok() {
                        assert!(
                            pathfinder.find_path(&map, &request(small)).is_ok(),
                            "{file} (structures: {with_structures}): {start:?} to {goal:?} \
                             routes at {large} but not at {small}"
                        );
                        both += 1;
                    }
                }
            }
        }
    }
    assert!(both > 250, "only {both} searches found a route");
}

// ---------------------------------------------------------------------------------------------
// Contour footprints (plan `2026-11-18-contour-footprints` WP3, D-4).
// ---------------------------------------------------------------------------------------------

/// A seeded integer draw in `0..bound` (`SplitMix64`).
fn draw_below(state: &mut u64, bound: u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    z % bound
}

/// The 5.0.14 footprints of §2.3 as `(cells, half-width, half-height, chamfer)` in quarter
/// cells, plus a scratch 3×2.
const SHAPES: [(u8, u8, u16, u16, u16); 7] = [
    (5, 5, 10, 10, 6),
    (3, 3, 6, 6, 2),
    (3, 3, 6, 6, 3),
    (2, 2, 4, 4, 2),
    (2, 2, 2, 2, 1),
    (1, 1, 2, 2, 0),
    (3, 2, 6, 4, 2),
];

fn footprint(index: usize, square: bool) -> sim_map::StructureFootprint {
    let (width, height, hw, hh, c) = SHAPES[index % SHAPES.len()];
    if square {
        return sim_map::StructureFootprint::square(width, height);
    }
    sim_map::StructureFootprint {
        width,
        height,
        shape: sim_types::FootprintShape {
            half_width_q: hw,
            half_height_q: hh,
            chamfer_q: c,
        },
    }
}

/// Places up to `count` seeded structures of the §2.3 sizes on `map`, each where it fits;
/// `square` gives every one the full box of its cells. Returns how many were placed.
fn place_seeded(map: &mut GameMap, seed: u64, count: u32, square: bool) -> u32 {
    let mut state = seed;
    let mut placed = 0;
    for attempt in 0..count * 8 {
        if placed == count {
            break;
        }
        let shape = draw_below(&mut state, SHAPES.len() as u64) as usize;
        let x = draw_below(&mut state, u64::from(map.width() - 5)) as u32;
        let y = draw_below(&mut state, u64::from(map.height() - 5)) as u32;
        if map
            .place_structure(EntityId(20_000 + attempt), (x, y), footprint(shape, square))
            .is_ok()
        {
            placed += 1;
        }
    }
    placed
}

fn showcase_map() -> GameMap {
    let text = include_str!("../../../assets/maps/showcase_canyon.scmap.ron");
    MapFile::from_ron_str(text)
        .and_then(|file| file.decode())
        .and_then(|document| document.to_game_map())
        .expect("invariant: the shipped showcase map decodes")
}

/// The digest of the showcase mesh with 24 seeded square structures, computed by this same test
/// on the tree before the contour cut (WP2 of plan `2026-11-18-contour-footprints`, where
/// structures cut whole cells).
const SQUARE_STRUCTURES_MESH_DIGEST: u64 = 0xb220_7731_a3e3_10c8;

/// D-4's equivalence: a structure whose shape is the full box of its cells cuts exactly the mesh
/// the cell cut made.
#[test]
fn a_square_shape_cuts_the_mesh_as_cells_did() {
    let mut map = showcase_map();
    assert_eq!(place_seeded(&mut map, 0x5eed, 24, true), 24);
    let mesh = mesh_of(&map);
    check_mesh_laws(&mesh).unwrap_or_else(|e| panic!("{e}"));
    check_labels(&map, &mesh).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        mesh.digest(),
        SQUARE_STRUCTURES_MESH_DIGEST,
        "digest {:#x}",
        mesh.digest()
    );
}

/// The vertex sum of triangle `t` in lattice units: three times its centroid.
fn centroid_sum(mesh: &NavMesh, t: u32) -> (i64, i64) {
    mesh.triangle_vertices(t)
        .map(|v| mesh.vertex(v))
        .iter()
        .fold((0, 0), |(x, y), p| (x + i64::from(p.x), y + i64::from(p.y)))
}

/// Every structure triangle lies in one footprint cell and inside its structure's shape; every
/// other triangle's centroid is outside every shape.
fn check_contour_labels(map: &GameMap, mesh: &NavMesh) -> Result<(), String> {
    let per_cell = i64::from(LATTICE_PER_CELL);
    for t in 0..mesh.triangle_count() as u32 {
        let (sx, sy) = centroid_sum(mesh, t);
        let inside: Vec<EntityId> = map
            .structures()
            .values()
            .filter(|placed| placed.contains_open_scaled(sx, sy, 3))
            .map(|placed| placed.id)
            .collect();
        match mesh.label(t).kind {
            TriangleKind::Structure(id) => {
                if inside != [id] {
                    return Err(format!("structure triangle {t} of {id:?} is in {inside:?}"));
                }
                let corners = mesh.triangle_vertices(t).map(|v| mesh.vertex(v));
                let (cx, cy) = (sx.div_euclid(3 * per_cell), sy.div_euclid(3 * per_cell));
                for p in corners {
                    let (x, y) = (i64::from(p.x), i64::from(p.y));
                    if x < cx * per_cell
                        || x > (cx + 1) * per_cell
                        || y < cy * per_cell
                        || y > (cy + 1) * per_cell
                    {
                        return Err(format!("structure triangle {t} leaves cell ({cx}, {cy})"));
                    }
                }
            }
            TriangleKind::Open | TriangleKind::Blocked => {
                if !inside.is_empty() {
                    return Err(format!(
                        "triangle {t} lies in {inside:?} but is not labelled"
                    ));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn contour_structure_triangles_lie_in_one_cell() {
    let mut map = arena::default_arena();
    assert_eq!(place_seeded(&mut map, 0xc0de, 12, false), 12);
    let mesh = mesh_of(&map);
    check_mesh_laws(&mesh).unwrap_or_else(|e| panic!("{e}"));
    check_runs(&mesh).unwrap_or_else(|e| panic!("{e}"));
    check_contour_labels(&map, &mesh).unwrap_or_else(|e| panic!("{e}"));
}

/// D-8, plan §2.2: a 5×5 contour's corner cell centre is open ground (0.354 from its chamfer),
/// while a 3×3 contour's corner cell centre is inside its shape.
#[test]
fn a_cut_corner_is_open_ground() {
    let mut map = arena::default_arena();
    map.place_structure(EntityId(1), (10, 10), footprint(0, false))
        .expect("invariant: open ground");
    map.place_structure(EntityId(2), (20, 10), footprint(1, false))
        .expect("invariant: open ground");
    let mesh = mesh_of(&map);
    let label_at = |x: u32, y: u32| {
        let centre = map
            .cell_to_world_center(x, y)
            .expect("invariant: on the map");
        mesh.label(mesh.locate(centre).expect("invariant: on the map"))
            .kind
    };
    for (x, y) in [(10, 10), (14, 10), (10, 14), (14, 14)] {
        assert_eq!(
            label_at(x, y),
            TriangleKind::Open,
            "5x5 corner cell ({x}, {y})"
        );
    }
    assert_eq!(label_at(12, 12), TriangleKind::Structure(EntityId(1)));
    for (x, y) in [(20, 10), (22, 10), (20, 12), (22, 12)] {
        assert_eq!(
            label_at(x, y),
            TriangleKind::Structure(EntityId(2)),
            "3x3 corner cell ({x}, {y})"
        );
    }
}

#[test]
fn removing_a_contour_structure_restores_the_terrain_mesh() {
    let terrain = arena::default_arena();
    let base = mesh_of(&terrain);
    let mut map = terrain.clone();
    map.place_structure(EntityId(3), (12, 12), footprint(0, false))
        .expect("invariant: open ground");
    map.place_structure(EntityId(4), (17, 17), footprint(1, false))
        .expect("invariant: open ground");
    map.place_structure(EntityId(5), (20, 12), footprint(4, false))
        .expect("invariant: open ground");
    let placed = mesh_of(&map);
    assert_ne!(placed.digest(), base.digest());
    for id in [3, 4, 5] {
        map.remove_structure(EntityId(id))
            .expect("invariant: placed above");
    }
    let restored = mesh_of(&map);
    assert_eq!(restored, base);
    assert_eq!(restored.digest(), base.digest());
}

/// Every constraint run lies on a structure's contour, on one of its interior cell lines, or on
/// a cell edge (terrain).
fn check_run_geometry(map: &GameMap, mesh: &NavMesh) -> Result<(), String> {
    let per_cell = LATTICE_PER_CELL;
    for run in mesh.constraint_runs() {
        let (a, b) = (run.from, run.to);
        let axis = a.x == b.x || a.y == b.y;
        let on_cell_line =
            (a.x == b.x && a.x % per_cell == 0) || (a.y == b.y && a.y % per_cell == 0);
        if axis && on_cell_line {
            continue;
        }
        // Otherwise it must lie on some structure's contour.
        let on_contour = map.structures().values().any(|placed| {
            let (vertices, count) = placed.contour();
            (0..count).any(|i| {
                let (u, v) = (vertices[i], vertices[(i + 1) % count]);
                let collinear = |p: LatticePoint| {
                    i64::from(v.x - u.x) * i64::from(p.y - u.y)
                        == i64::from(v.y - u.y) * i64::from(p.x - u.x)
                };
                let within = |p: LatticePoint| {
                    p.x >= u.x.min(v.x)
                        && p.x <= u.x.max(v.x)
                        && p.y >= u.y.min(v.y)
                        && p.y <= u.y.max(v.y)
                };
                collinear(a) && collinear(b) && within(a) && within(b)
            })
        });
        if !on_contour {
            return Err(format!(
                "run {run:?} is neither a cell edge nor on a contour"
            ));
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Seeded layouts of up to 12 contour structures, touching, face-adjacent, diagonal and
    /// beside cliffs on the default arena: the build never fails, labels follow the shapes, and
    /// every constraint lies on a contour, a clipped cell line or a terrain edge.
    #[test]
    fn contour_meshes_build_for_seeded_layouts(seed in any::<u64>(), count in 1u32..=12) {
        let mut map = arena::default_arena();
        let mut state = seed;
        let mut placed = 0;
        let mut previous: Option<(u32, u32, u8, u8)> = None;
        for attempt in 0..count * 6 {
            if placed == count {
                break;
            }
            let shape = draw_below(&mut state, SHAPES.len() as u64) as usize;
            let print = footprint(shape, false);
            // Half the time, put the next structure flush against the previous one's east or
            // north face, or on its north-east corner.
            let cell = match (previous, draw_below(&mut state, 6)) {
                (Some((x, y, w, _)), 0) => (x + u32::from(w), y),
                (Some((x, y, _, h)), 1) => (x, y + u32::from(h)),
                (Some((x, y, w, h)), 2) => (x + u32::from(w), y + u32::from(h)),
                _ => (
                    draw_below(&mut state, u64::from(map.width() - 5)) as u32,
                    draw_below(&mut state, u64::from(map.height() - 5)) as u32,
                ),
            };
            if map.place_structure(EntityId(30_000 + attempt), cell, print).is_ok() {
                placed += 1;
                previous = Some((cell.0, cell.1, print.width, print.height));
            }
        }
        let mesh = NavMesh::build(&map).map_err(|e| TestCaseError::fail(e.to_string()))?;
        check_mesh_laws(&mesh).map_err(TestCaseError::fail)?;
        check_runs(&mesh).map_err(TestCaseError::fail)?;
        check_contour_labels(&map, &mesh).map_err(TestCaseError::fail)?;
        check_run_geometry(&map, &mesh).map_err(TestCaseError::fail)?;
    }
}

/// Funnel fallbacks to portal midpoints among seeded structures on the default arena: routes at
/// the shipped terrain radii between seeded points where the disc fits, as `(routed, fallbacks,
/// exact fits)`. An exact fit is a fallback a disc 1/4096 smaller avoids: a disc exactly as wide
/// as a bent channel, which TA* admits (`(2r)² ≤ w²`) and no polyline rounds under the strict
/// segment predicate.
fn fallback_counts(layouts: u64, square: bool) -> (u32, u32, u32) {
    let (mut routed, mut fallbacks, mut exact) = (0, 0, 0);
    for layout in 0..layouts {
        let mut state = layout.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut map = arena::default_arena();
        let count = 1 + (layout % 12) as u32;
        let mut placed = 0;
        for attempt in 0..count * 6 {
            if placed == count {
                break;
            }
            let shape = draw_below(&mut state, SHAPES.len() as u64) as usize;
            let cell = (
                draw_below(&mut state, u64::from(map.width() - 5)) as u32,
                draw_below(&mut state, u64::from(map.height() - 5)) as u32,
            );
            if map
                .place_structure(EntityId(40_000 + attempt), cell, footprint(shape, square))
                .is_ok()
            {
                placed += 1;
            }
        }
        let mut pathfinder = sim_map::Pathfinder::new(&map);
        for _ in 0..16 {
            let mut cell = || {
                let x = draw_below(&mut state, u64::from(map.width())) as u32;
                let y = draw_below(&mut state, u64::from(map.height())) as u32;
                map.cell_to_world_center(x, y).ok()
            };
            let (Some(start), Some(goal)) = (cell(), cell()) else {
                continue;
            };
            let radius = SHIPPED_TERRAIN_RADII
                [draw_below(&mut state, SHIPPED_TERRAIN_RADII.len() as u64) as usize];
            let request = sim_map::PathRequest {
                start,
                goal,
                radius: SimScalar::from_raw(radius << 28),
            };
            let fits = |p: SimVec2, pathfinder: &mut sim_map::Pathfinder| {
                pathfinder.segment_is_passable(&map, &request.into(), p, p)
            };
            if !fits(start, &mut pathfinder) || !fits(goal, &mut pathfinder) {
                continue;
            }
            let Ok(route) = pathfinder.find_route_mesh(&map, &request.into()) else {
                continue;
            };
            routed += 1;
            if route.fallback {
                fallbacks += 1;
                let smaller = sim_map::PathRequest {
                    radius: request.radius - SimScalar::from_raw(1 << 20),
                    ..request
                };
                exact += u32::from(
                    pathfinder
                        .find_route_mesh(&map, &smaller.into())
                        .is_ok_and(|route| !route.fallback),
                );
            }
        }
    }
    (routed, fallbacks, exact)
}

/// The shipped terrain radii, in sixteenths of a world unit: 0.3125 to 1.0.
const SHIPPED_TERRAIN_RADII: [i64; 9] = [5, 6, 8, 9, 10, 12, 14, 15, 16];

/// Plan `2026-11-18-contour-footprints` Amendment 1: the contours add no funnel fallback. On 150
/// seeded layouts of the §2.3 shapes, the same layouts with their contours fall back no more
/// often than with full squares, in total and outside exact fits. (Fallbacks between points
/// where the disc fits remain on terrain with or without structures; they are the funnel's
/// existing behaviour, B-141.)
#[test]
fn contours_add_no_funnel_fallbacks() {
    let (contour_routed, contour_fallbacks, contour_exact) = fallback_counts(150, false);
    let (square_routed, square_fallbacks, square_exact) = fallback_counts(150, true);
    eprintln!(
        "contour: {contour_fallbacks} of {contour_routed} ({contour_exact} exact fits); \
         square: {square_fallbacks} of {square_routed} ({square_exact} exact fits)"
    );
    assert!(contour_routed > 1_000 && square_routed > 1_000);
    assert!(contour_fallbacks <= square_fallbacks);
    assert!(contour_fallbacks - contour_exact <= square_fallbacks - square_exact);
}
