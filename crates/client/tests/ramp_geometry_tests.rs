//! Geometric properties of ramps in the terrain mesh (plan `docs/plans/2026-11-09-ramp-geometry.md`,
//! section 4).
//!
//! # Responsibility
//! Proves that the chunk meshes of [`build_chunk_mesh`] draw every ramp exactly where the
//! simulation puts it: the top surface over a ramp cell is the simulation's altitude (T1), nothing
//! but the ramp's own quad meets the inside of its footprint (T2), and every height step along a
//! ramp cell's edges is closed by a vertical wall (T3).
//!
//! # Fit
//! CPU only: no window, no GPU, no Bevy `App`. The semantic terrain tests in `terrain_tests.rs`
//! sample only cells away from ramps, and the bit-exact goldens in `mesher_golden_tests.rs` accept
//! one mesh without saying why it is right; these tests state the ramp invariants the mesher's
//! module documentation lists as R1 to R6.
//!
//! # Inputs
//! - The two shipped maps, loaded through the real `MapFile` loader.
//! - A synthetic fixture (T0, built in `support/terrain_mesh.rs`) of ramp "stations", each a ramp run inside its own box of low and
//!   high ground: protruding ramps in all four directions with run lengths 1 to
//!   `MAX_RAMP_RUN_CELLS` and widths 1 to 4, ramps beside the abyss, recessed and half-recessed
//!   ramps, ramps beside a plateau two levels up, two runs side by side with different low
//!   levels, and a ramp whose head lies on a chunk border. The map size is not a multiple of
//!   `CHUNK_DUAL_CELLS`, and the fixture passes the map validator's ramp checks.
//!
//! # Invariants checked
//! - T1: the highest mesh hit of a vertical ray over a ramp footprint (away from the guard rails)
//!   equals `GameMap::terrain_altitude_at` within `1e-4`.
//! - T2: no triangle other than the cell's ramp quad has an XZ projection that meets the open
//!   footprint shrunk by `1e-3` (and by the rail width across the run).
//! - T3: where a ramp edge and the surface beside it differ in height, a horizontal ray across the
//!   edge at any height between them meets a vertical triangle within `1e-3` of the edge.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/terrain_mesh.rs"]
mod terrain_mesh;

use bevy::prelude::{Vec2, Vec3};
use client::terrain::mesher::ABYSS_WALL_BOTTOM_Y;
use sim_map::grid::{GameMap, RampDir, RampProfile, MAX_RAMP_RUN_CELLS};
use sim_map::map_format::{MapDocument, MapMetadata};
use sim_map::validation::{validate_map, MapValidationError};
use sim_types::{SimScalar, SimVec2};
use terrain_mesh::{fixture_map, load_shipped_map, ramp_cells, MapMesh};

/// Height tolerance of the top-surface check (T1).
const SURFACE_TOLERANCE: f32 = 1e-4;

/// Shrink of a footprint before the exclusivity check (T2), and the closure distance (T3).
const EDGE_EPSILON: f32 = 1e-3;

// -----------------------------------------------------------------------------
// Maps
// -----------------------------------------------------------------------------

// The shipped maps, the T0 fixture and the mesh access live in `support/terrain_mesh.rs`.

#[test]
fn t0_fixture_is_a_valid_map_with_every_ramp_kind() {
    let map = fixture_map();
    let doc = MapDocument::from_game_map(
        &map,
        MapMetadata {
            title: "ramp geometry fixture".into(),
            author: "ramp_geometry_tests".into(),
            description: "ramp stations".into(),
            theme: "orbital_foundry".into(),
        },
    )
    .expect("the fixture is a valid map document");
    let report = validate_map(&doc);
    let ramp_errors: Vec<&MapValidationError> = report
        .errors
        .iter()
        .filter(|e| {
            matches!(
                e,
                MapValidationError::RampOnAbyss { .. }
                    | MapValidationError::RampAtTopLevel { .. }
                    | MapValidationError::RampNotPathable { .. }
                    | MapValidationError::RampBuildable { .. }
                    | MapValidationError::RampRunTooLong { .. }
                    | MapValidationError::RampUpperLandingMismatch { .. }
                    | MapValidationError::RampLowerLandingMismatch { .. }
                    | MapValidationError::RampRunLevelMismatch { .. }
            )
        })
        .collect();
    assert!(
        ramp_errors.is_empty(),
        "fixture ramp errors: {ramp_errors:?}"
    );

    // Every run length 1..=MAX occurs, in all four directions overall.
    let mut runs = [false; MAX_RAMP_RUN_CELLS as usize + 1];
    let mut dirs = Vec::new();
    for (x, y) in ramp_cells(&map) {
        let p = map.ramp_profile(x, y).expect("ramp cell");
        runs[p.run_len as usize] = true;
        if !dirs.contains(&p.dir) {
            dirs.push(p.dir);
        }
    }
    assert!(
        runs[1..].iter().all(|&r| r),
        "run lengths present: {runs:?}"
    );
    assert_eq!(dirs.len(), 4, "directions present: {dirs:?}");
}

// -----------------------------------------------------------------------------
// Mesh access
// -----------------------------------------------------------------------------

/// Möller–Trumbore: distance along `dir` at which the ray from `origin` meets triangle `t`.
fn ray_hit(origin: Vec3, dir: Vec3, t: &[Vec3; 3]) -> Option<f32> {
    let [v0, v1, v2] = *t;
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - v0;
    let u = s.dot(p) * inv;
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    let tol = 1e-5;
    if u < -tol || v < -tol || u + v > 1.0 + tol {
        return None;
    }
    Some(e2.dot(q) * inv)
}

fn ramp_height(profile: RampProfile, fx: f32, fy: f32) -> f32 {
    profile
        .height_at(SimScalar::from_num(fx), SimScalar::from_num(fy))
        .to_f32()
}

/// Lateral and run margins `(x, z)` of the footprint checks for a ramp direction: the guard
/// rails ride on the edges parallel to the run.
fn footprint_margins(dir: RampDir) -> (f32, f32) {
    terrain_mesh::footprint_margins(dir, EDGE_EPSILON)
}

// -----------------------------------------------------------------------------
// T1: the top surface over a ramp is the simulation's altitude
// -----------------------------------------------------------------------------

/// Returns one line per ramp cell whose top surface differs from the simulation.
fn top_surface_failures(map: &GameMap, mesh: &MapMesh) -> Vec<String> {
    const GRID: u32 = 8;
    let mut failures = Vec::new();
    for (x, y) in ramp_cells(map) {
        let profile = map.ramp_profile(x, y).expect("ramp cell");
        let (mx, mz) = footprint_margins(profile.dir);
        let mut wrong = 0;
        let mut worst = (0.0_f32, 0.0_f32, 0.0_f32);
        for j in 0..GRID {
            for i in 0..GRID {
                let fx = mx + (1.0 - 2.0 * mx) * (i as f32 + 0.5) / GRID as f32;
                let fz = mz + (1.0 - 2.0 * mz) * (j as f32 + 0.5) / GRID as f32;
                let (px, pz) = (x as f32 + fx, y as f32 + fz);
                let sim = map.terrain_altitude_at(SimVec2::new(
                    SimScalar::from_num(px),
                    SimScalar::from_num(pz),
                ));
                let top = mesh.top_at(x, y, px, pz).unwrap_or(f32::NEG_INFINITY);
                let d = top - sim;
                if d.abs() > SURFACE_TOLERANCE || !d.is_finite() {
                    wrong += 1;
                    if d.is_nan() || d.abs() > worst.0.abs() {
                        worst = (d, fx, fz);
                    }
                }
            }
        }
        if wrong > 0 {
            failures.push(format!(
                "ramp cell ({x},{y}) {:?} {}/{}: {wrong}/64 samples off, worst {:+.4} at ({:.3},{:.3})",
                profile.dir, profile.index, profile.run_len, worst.0, worst.1, worst.2
            ));
        }
    }
    failures
}

fn assert_top_surface(name: &str, map: &GameMap) {
    let mesh = MapMesh::build(map);
    let failures = top_surface_failures(map, &mesh);
    assert!(
        failures.is_empty(),
        "{name}: {} ramp cells with a wrong top surface:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn t1_top_surface_is_the_simulation_altitude_showcase_canyon() {
    assert_top_surface(
        "showcase_canyon",
        &load_shipped_map("showcase_canyon.scmap.ron"),
    );
}

#[test]
fn t1_top_surface_is_the_simulation_altitude_default_arena() {
    assert_top_surface(
        "default_arena",
        &load_shipped_map("default_arena.scmap.ron"),
    );
}

#[test]
fn t1_top_surface_is_the_simulation_altitude_fixture() {
    assert_top_surface("fixture", &fixture_map());
}

// -----------------------------------------------------------------------------
// T2: only the ramp quad meets the ramp footprint
// -----------------------------------------------------------------------------

/// Separating-axis test: does the XZ projection of triangle `t` meet the open box
/// `(min, max)`? Touching counts as separated.
fn projection_meets_open_box(t: &[Vec3; 3], min: Vec2, max: Vec2) -> bool {
    let pts = t.map(|p| Vec2::new(p.x, p.z));
    let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
    let mut axes = vec![Vec2::X, Vec2::Y];
    for k in 0..3 {
        let e = pts[(k + 1) % 3] - pts[k];
        if e.length_squared() > 1e-12 {
            axes.push(e.perp().normalize());
        }
    }
    axes.iter().all(|&axis| {
        let (t_min, t_max) = interval(&pts, axis);
        let (b_min, b_max) = interval(&corners, axis);
        t_max > b_min && t_min < b_max
    })
}

fn interval(pts: &[Vec2], axis: Vec2) -> (f32, f32) {
    pts.iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
            let d = p.dot(axis);
            (lo.min(d), hi.max(d))
        })
}

/// Whether `t` is one of the two triangles of the ramp quad of cell `(x, y)`: its three vertices
/// sit on the footprint's corners at the ramp's corner heights.
fn is_own_ramp_quad(t: &[Vec3; 3], x: u32, y: u32, profile: RampProfile) -> bool {
    t.iter().all(|p| {
        let fx = p.x - x as f32;
        let fz = p.z - y as f32;
        let corner = |f: f32| f.abs() < 1e-6 || (f - 1.0).abs() < 1e-6;
        corner(fx) && corner(fz) && (p.y - ramp_height(profile, fx, fz)).abs() < 1e-5
    })
}

fn footprint_intruders(map: &GameMap, mesh: &MapMesh) -> Vec<String> {
    let mut failures = Vec::new();
    for (x, y) in ramp_cells(map) {
        let profile = map.ramp_profile(x, y).expect("ramp cell");
        let (mx, mz) = footprint_margins(profile.dir);
        let min = Vec2::new(x as f32 + mx, y as f32 + mz);
        let max = Vec2::new(x as f32 + 1.0 - mx, y as f32 + 1.0 - mz);
        let intruders: Vec<&[Vec3; 3]> = mesh
            .near(x, y)
            .filter(|t| projection_meets_open_box(t, min, max))
            .filter(|t| !is_own_ramp_quad(t, x, y, profile))
            .collect();
        if let Some(first) = intruders.first() {
            failures.push(format!(
                "ramp cell ({x},{y}) {:?} {}/{}: {} foreign triangles, e.g. {first:?}",
                profile.dir,
                profile.index,
                profile.run_len,
                intruders.len()
            ));
        }
    }
    failures
}

fn assert_footprints_exclusive(name: &str, map: &GameMap) {
    let mesh = MapMesh::build(map);
    let failures = footprint_intruders(map, &mesh);
    assert!(
        failures.is_empty(),
        "{name}: {} ramp footprints with foreign geometry:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn t2_only_the_ramp_quad_meets_its_footprint_showcase_canyon() {
    assert_footprints_exclusive(
        "showcase_canyon",
        &load_shipped_map("showcase_canyon.scmap.ron"),
    );
}

#[test]
fn t2_only_the_ramp_quad_meets_its_footprint_default_arena() {
    assert_footprints_exclusive(
        "default_arena",
        &load_shipped_map("default_arena.scmap.ron"),
    );
}

#[test]
fn t2_only_the_ramp_quad_meets_its_footprint_fixture() {
    assert_footprints_exclusive("fixture", &fixture_map());
}

// -----------------------------------------------------------------------------
// T3: every height step along a ramp edge is closed by a wall
// -----------------------------------------------------------------------------

/// One half of a ramp cell edge, in cell-local coordinates.
#[derive(Debug, Clone, Copy)]
struct HalfEdge {
    /// Start and end in cell-local `(fx, fy)`.
    a: Vec2,
    b: Vec2,
    /// Unit vector out of the ramp cell across the edge, in world XZ.
    out: Vec2,
    /// Offset of the neighbour cell.
    offset: (i32, i32),
}

fn half_edges() -> [HalfEdge; 8] {
    let edge = |a: Vec2, b: Vec2, out: Vec2, offset| {
        let m = (a + b) * 0.5;
        [
            HalfEdge {
                a,
                b: m,
                out,
                offset,
            },
            HalfEdge {
                a: m,
                b,
                out,
                offset,
            },
        ]
    };
    let [s0, s1] = edge(
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(0.0, -1.0),
        (0, -1),
    );
    let [e0, e1] = edge(
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(1.0, 0.0),
        (1, 0),
    );
    let [n0, n1] = edge(
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(0.0, 1.0),
        (0, 1),
    );
    let [w0, w1] = edge(
        Vec2::new(0.0, 1.0),
        Vec2::new(0.0, 0.0),
        Vec2::new(-1.0, 0.0),
        (-1, 0),
    );
    [s0, s1, e0, e1, n0, n1, w0, w1]
}

/// Surface height beside half-edge `h` of ramp cell `(x, y)` at cell-local point `f` on it: the
/// neighbouring ramp's own edge, else the highest mesh floor just across the edge (the abyss
/// wall bottom where there is none).
fn neighbour_surface(map: &GameMap, mesh: &MapMesh, x: u32, y: u32, h: &HalfEdge, f: Vec2) -> f32 {
    let nx = i64::from(x) + i64::from(h.offset.0);
    let ny = i64::from(y) + i64::from(h.offset.1);
    let inside =
        (0..i64::from(map.width())).contains(&nx) && (0..i64::from(map.height())).contains(&ny);
    if !inside {
        return ABYSS_WALL_BOTTOM_Y;
    }
    let (nx, ny) = (nx as u32, ny as u32);
    if let Some(np) = map.ramp_profile(nx, ny) {
        let nf = f - Vec2::new(h.offset.0 as f32, h.offset.1 as f32);
        return ramp_height(np, nf.x, nf.y);
    }
    let p = Vec2::new(x as f32, y as f32) + f + h.out * 0.02;
    mesh.top_at(nx, ny, p.x, p.y).unwrap_or(ABYSS_WALL_BOTTOM_Y)
}

fn open_steps(map: &GameMap, mesh: &MapMesh) -> Vec<String> {
    let mut failures = Vec::new();
    for (x, y) in ramp_cells(map) {
        let profile = map.ramp_profile(x, y).expect("ramp cell");
        for h in half_edges() {
            for s in [0.2_f32, 0.5, 0.8] {
                let f = h.a.lerp(h.b, s);
                let edge_h = ramp_height(profile, f.x, f.y);
                let beside = neighbour_surface(map, mesh, x, y, &h, f);
                let (lo, hi) = (edge_h.min(beside), edge_h.max(beside));
                if hi - lo <= 2.0 * EDGE_EPSILON {
                    continue;
                }
                let edge_point = Vec2::new(x as f32, y as f32) + f;
                for q in [0.25_f32, 0.5, 0.75] {
                    let height = lo + (hi - lo) * q;
                    let start = edge_point - h.out * 0.25;
                    let origin = Vec3::new(start.x, height, start.y);
                    let dir = Vec3::new(h.out.x, 0.0, h.out.y);
                    let closed = mesh.near(x, y).any(|t| {
                        let normal = (t[1] - t[0]).cross(t[2] - t[0]);
                        normal.length_squared() > 0.0
                            && normal.normalize().y.abs() < 1e-3
                            && ray_hit(origin, dir, t)
                                .is_some_and(|d| (d - 0.25).abs() <= EDGE_EPSILON)
                    });
                    if !closed {
                        failures.push(format!(
                            "ramp cell ({x},{y}) {:?} {}/{}: open step at {edge_point} height {height:.3} (ramp edge {edge_h:.3}, beside {beside:.3})",
                            profile.dir, profile.index, profile.run_len
                        ));
                    }
                }
            }
        }
    }
    failures
}

fn assert_steps_closed(name: &str, map: &GameMap) {
    let mesh = MapMesh::build(map);
    let failures = open_steps(map, &mesh);
    assert!(
        failures.is_empty(),
        "{name}: {} open steps along ramp edges:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn t3_ramp_edge_steps_are_closed_showcase_canyon() {
    assert_steps_closed(
        "showcase_canyon",
        &load_shipped_map("showcase_canyon.scmap.ron"),
    );
}

#[test]
fn t3_ramp_edge_steps_are_closed_default_arena() {
    assert_steps_closed(
        "default_arena",
        &load_shipped_map("default_arena.scmap.ron"),
    );
}

#[test]
fn t3_ramp_edge_steps_are_closed_fixture() {
    assert_steps_closed("fixture", &fixture_map());
}
