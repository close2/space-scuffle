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
use sim_map::navmesh::predicates::{incircle, orient2d, LatticePoint, NAV_MESH_MAX_DIMENSION};
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
    let expected = 2 * i64::from(mesh.width()) * i64::from(mesh.height());
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
        .place_structure(a, 10, 10, 3, 3)
        .expect("invariant: open ground");
    forward
        .place_structure(b, 13, 10, 2, 2)
        .expect("invariant: open ground");
    let mut backward = terrain.clone();
    backward
        .place_structure(b, 13, 10, 2, 2)
        .expect("invariant: open ground");
    backward
        .place_structure(a, 10, 10, 3, 3)
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
    map.place_structure(EntityId(77), 20, 20, 2, 2)
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
const SHOWCASE_MESH_DIGEST: u64 = 0x46d3_38ba_8cd1_c537;

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
        if map.place_structure(EntityId(5), x, y, w, h).is_ok() {
            let placed = mesh_of(&map);
            check_mesh_laws(&placed).map_err(TestCaseError::fail)?;
            check_labels(&map, &placed).map_err(TestCaseError::fail)?;
            map.remove_structure(EntityId(5)).map_err(|e| TestCaseError::fail(e.to_string()))?;
        }
        prop_assert_eq!(mesh_of(&map), fresh);
    }
}
