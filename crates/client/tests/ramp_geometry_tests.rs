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
//! - A synthetic fixture (T0) of ramp "stations", each a ramp run inside its own box of low and
//!   high ground: protruding ramps in all four directions with run lengths 1 to
//!   `MAX_RAMP_RUN_CELLS` and widths 1 to 4, ramps beside the abyss, and a ramp whose head lies
//!   on a chunk border. The map size is not a multiple of
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

use std::path::PathBuf;

use bevy::prelude::{Vec2, Vec3};
use client::resolve_assets_dir;
use client::terrain::mesher::{
    build_chunk_mesh, MesherScratch, ABYSS_WALL_BOTTOM_Y, CHUNK_DUAL_CELLS, RAIL_WIDTH,
};
use sim_map::grid::{
    CellFlags, CliffLevel, GameMap, MapCell, RampDir, RampProfile, TerrainKind, MAX_RAMP_RUN_CELLS,
};
use sim_map::map_format::{MapDocument, MapFile, MapMetadata};
use sim_map::validation::{validate_map, MapValidationError};
use sim_types::{SimScalar, SimVec2};

/// Height tolerance of the top-surface check (T1).
const SURFACE_TOLERANCE: f32 = 1e-4;

/// Shrink of a footprint before the exclusivity check (T2), and the closure distance (T3).
const EDGE_EPSILON: f32 = 1e-3;

// -----------------------------------------------------------------------------
// Maps
// -----------------------------------------------------------------------------

fn load_shipped_map(filename: &str) -> GameMap {
    let direct = resolve_assets_dir().join("maps").join(filename);
    let path = if direct.exists() {
        direct
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/maps")
            .join(filename)
    };
    MapFile::load(&path)
        .unwrap_or_else(|e| panic!("failed to load {}: {e}", path.display()))
        .decode()
        .unwrap_or_else(|e| panic!("failed to decode {}: {e}", path.display()))
        .to_game_map()
        .unwrap_or_else(|e| panic!("failed to build GameMap from {}: {e}", path.display()))
}

/// What lies beside a ramp run across its lateral edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    /// Ground at the ramp's low level: a protruding ramp.
    Low,
    /// Bottomless abyss.
    Abyss,
}

/// One ramp run inside its own box of ground.
#[derive(Debug, Clone, Copy)]
struct Station {
    dir: RampDir,
    run: u32,
    width: u32,
    low: u8,
    left: Side,
    right: Side,
}

/// Cells of ground around a station's ramp on every side.
const STATION_MARGIN: u32 = 3;

impl Station {
    /// Box size `(along the run, across it)` in cells.
    fn box_size(&self) -> (u32, u32) {
        (
            self.run + 2 * STATION_MARGIN,
            self.width + 2 * STATION_MARGIN,
        )
    }

    /// Box size in world cells `(x, y)`.
    fn world_size(&self) -> (u32, u32) {
        let (along, across) = self.box_size();
        match self.dir {
            RampDir::North | RampDir::South => (across, along),
            _ => (along, across),
        }
    }

    /// World cell of box-local cell `(along, across)` for a box whose lower-left cell is `origin`.
    fn world_cell(&self, origin: (u32, u32), along: u32, across: u32) -> (u32, u32) {
        let (len, _) = self.box_size();
        match self.dir {
            RampDir::North => (origin.0 + across, origin.1 + along),
            RampDir::South => (origin.0 + across, origin.1 + len - 1 - along),
            RampDir::East => (origin.0 + along, origin.1 + across),
            RampDir::West => (origin.0 + len - 1 - along, origin.1 + across),
            _ => panic!("stations use cardinal ramps only"),
        }
    }

    /// The cell at run coordinate `u` (0 = the run's first cell) and lateral coordinate `v`
    /// (0 = the first lateral cell of the ramp).
    fn cell(&self, u: i64, v: i64) -> MapCell {
        let run = i64::from(self.run);
        let width = i64::from(self.width);
        let in_run = (0..run).contains(&u);
        let low = self.low;
        if in_run && (0..width).contains(&v) {
            return ramp_cell(low, self.dir);
        }
        if in_run {
            let side = if v < 0 { self.left } else { self.right };
            return match side {
                Side::Low => ground(low),
                Side::Abyss => abyss(),
            };
        }
        if u < 0 {
            ground(low)
        } else {
            ground(low + 1)
        }
    }
}

fn ground(level: u8) -> MapCell {
    MapCell {
        terrain_kind: if level.is_multiple_of(2) {
            TerrainKind::Grass
        } else {
            TerrainKind::MetalPlate
        },
        cliff_level: CliffLevel(level),
        flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
        ramp: None,
        occupying_structure: None,
    }
}

fn abyss() -> MapCell {
    MapCell {
        terrain_kind: TerrainKind::Abyss,
        cliff_level: CliffLevel(0),
        flags: CellFlags::UNPASSABLE_CLIFF,
        ramp: None,
        occupying_structure: None,
    }
}

fn ramp_cell(low: u8, dir: RampDir) -> MapCell {
    MapCell {
        terrain_kind: TerrainKind::Dirt,
        cliff_level: CliffLevel(low),
        flags: CellFlags::GROUND_PATHABLE.union(CellFlags::RAMP),
        ramp: Some(dir),
        occupying_structure: None,
    }
}

/// The stations of the fixture. The first one is placed so that its head lies on a chunk border.
fn stations() -> Vec<Station> {
    use RampDir::{East, North, South, West};
    use Side::{Abyss, Low};
    let dirs = [North, East, South, West];
    let mut list = vec![Station {
        dir: East,
        run: 3,
        width: 2,
        low: 1,
        left: Low,
        right: Low,
    }];
    for run in 1..=u32::from(MAX_RAMP_RUN_CELLS) {
        list.push(Station {
            dir: dirs[run as usize % 4],
            run,
            width: 1 + run % 4,
            low: (run % 3) as u8,
            left: Low,
            right: Low,
        });
    }
    list.push(Station {
        dir: North,
        run: 4,
        width: 2,
        low: 1,
        left: Abyss,
        right: Abyss,
    });
    list.push(Station {
        dir: West,
        run: 2,
        width: 3,
        low: 0,
        left: Abyss,
        right: Low,
    });
    list
}

/// Width of the fixture map in cells (not a multiple of `CHUNK_DUAL_CELLS`).
const FIXTURE_WIDTH: u32 = 77;

/// Builds the T0 fixture: stations packed in shelves on level-0 ground, one cell apart.
fn fixture_map() -> GameMap {
    let list = stations();
    // Shelf packing; the first station starts at x = 10 so that its head (the last ramp cell,
    // x = 10 + margin + run - 1 = 15) and its upper landing (x = 16) lie in different chunks.
    let mut origins = Vec::with_capacity(list.len());
    let (mut x, mut y, mut shelf_height) = (10, 1, 0);
    for st in &list {
        let (w, h) = st.world_size();
        if x + w + 1 > FIXTURE_WIDTH {
            x = 1;
            y += shelf_height + 1;
            shelf_height = 0;
        }
        origins.push((x, y));
        x += w + 1;
        shelf_height = shelf_height.max(h);
    }
    let mut height = y + shelf_height + 1;
    if height % CHUNK_DUAL_CELLS == 0 {
        height += 1;
    }
    assert_ne!(FIXTURE_WIDTH % CHUNK_DUAL_CELLS, 0);

    let mut map = GameMap::new(FIXTURE_WIDTH, height, ground(0)).expect("fixture size is valid");
    for (st, &origin) in list.iter().zip(&origins) {
        let (along_len, across_len) = st.box_size();
        for along in 0..along_len {
            for across in 0..across_len {
                let (cx, cy) = st.world_cell(origin, along, across);
                let u = i64::from(along) - i64::from(STATION_MARGIN);
                let v = i64::from(across) - i64::from(STATION_MARGIN);
                *map.cell_mut(cx, cy)
                    .expect("station boxes lie inside the map") = st.cell(u, v);
            }
        }
    }

    let head = map.ramp_profile(15, origins[0].1 + STATION_MARGIN);
    assert!(
        head.is_some_and(|p| p.index + 1 == p.run_len)
            && map.ramp_profile(16, origins[0].1 + 3).is_none(),
        "the first station's head must be the last cell of chunk column 0"
    );
    map
}

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

/// Every triangle of every chunk of a map, bucketed by the cells its XZ bounding box overlaps.
struct MapMesh {
    tris: Vec<[Vec3; 3]>,
    width: u32,
    buckets: Vec<Vec<u32>>,
}

impl MapMesh {
    fn build(map: &GameMap) -> Self {
        let mut tris = Vec::new();
        let mut scratch = MesherScratch::default();
        let cols = (map.width() + 1).div_ceil(CHUNK_DUAL_CELLS);
        let rows = (map.height() + 1).div_ceil(CHUNK_DUAL_CELLS);
        for cy in 0..rows {
            for cx in 0..cols {
                build_chunk_mesh(map, cx, cy, &mut scratch);
                for t in scratch.indices.as_chunks::<3>().0 {
                    tris.push(t.map(|i| Vec3::from(scratch.positions[i as usize])));
                }
            }
        }
        let (width, height) = (map.width(), map.height());
        let mut buckets = vec![Vec::new(); (width * height) as usize];
        for (i, t) in tris.iter().enumerate() {
            let min_x = t.iter().map(|p| p.x).fold(f32::INFINITY, f32::min) - 1e-3;
            let max_x = t.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max) + 1e-3;
            let min_z = t.iter().map(|p| p.z).fold(f32::INFINITY, f32::min) - 1e-3;
            let max_z = t.iter().map(|p| p.z).fold(f32::NEG_INFINITY, f32::max) + 1e-3;
            let x0 = min_x.floor().max(0.0) as u32;
            let x1 = (max_x.floor() as i64).clamp(0, i64::from(width) - 1) as u32;
            let z0 = min_z.floor().max(0.0) as u32;
            let z1 = (max_z.floor() as i64).clamp(0, i64::from(height) - 1) as u32;
            if x0 >= width || z0 >= height {
                continue;
            }
            for z in z0..=z1 {
                for x in x0..=x1 {
                    buckets[(z * width + x) as usize].push(i as u32);
                }
            }
        }
        Self {
            tris,
            width,
            buckets,
        }
    }

    /// Triangles whose bounding box overlaps cell `(x, y)`.
    fn near(&self, x: u32, y: u32) -> impl Iterator<Item = &[Vec3; 3]> + '_ {
        self.buckets[(y * self.width + x) as usize]
            .iter()
            .map(|&i| &self.tris[i as usize])
    }

    /// Highest altitude at which a vertical line through `(px, pz)` meets a non-vertical
    /// triangle near cell `(x, y)`.
    fn top_at(&self, x: u32, y: u32, px: f32, pz: f32) -> Option<f32> {
        self.near(x, y)
            .filter_map(|t| vertical_hit(px, pz, t))
            .reduce(f32::max)
    }
}

/// Height at which the vertical line through `(px, pz)` meets triangle `t`, edges included.
fn vertical_hit(px: f32, pz: f32, t: &[Vec3; 3]) -> Option<f32> {
    let [v0, v1, v2] = *t;
    let det = (v1.z - v2.z) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.z - v2.z);
    if det.abs() < 1e-9 {
        return None;
    }
    let w0 = ((v1.z - v2.z) * (px - v2.x) + (v2.x - v1.x) * (pz - v2.z)) / det;
    let w1 = ((v2.z - v0.z) * (px - v2.x) + (v0.x - v2.x) * (pz - v2.z)) / det;
    let w2 = 1.0 - w0 - w1;
    let e = -1e-6;
    (w0 >= e && w1 >= e && w2 >= e).then_some(w0 * v0.y + w1 * v1.y + w2 * v2.y)
}

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

fn ramp_cells(map: &GameMap) -> Vec<(u32, u32)> {
    let mut cells = Vec::new();
    for y in 0..map.height() {
        for x in 0..map.width() {
            if map.ramp_profile(x, y).is_some() {
                cells.push((x, y));
            }
        }
    }
    cells
}

fn ramp_height(profile: RampProfile, fx: f32, fy: f32) -> f32 {
    profile
        .height_at(SimScalar::from_num(fx), SimScalar::from_num(fy))
        .to_f32()
}

/// Lateral and run margins `(x, z)` of the footprint checks for a ramp direction: the guard
/// rails ride on the edges parallel to the run.
fn footprint_margins(dir: RampDir) -> (f32, f32) {
    let rail = RAIL_WIDTH + EDGE_EPSILON;
    match dir {
        RampDir::North | RampDir::South => (rail, EDGE_EPSILON),
        RampDir::East | RampDir::West => (EDGE_EPSILON, rail),
        _ => (rail, rail),
    }
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

/// Kinds of neighbour a closure check covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Closure {
    /// Only steps down from the ramp edge to a non-ramp neighbour.
    SideWalls,
}

fn open_steps(map: &GameMap, mesh: &MapMesh, scope: Closure) -> Vec<String> {
    let mut failures = Vec::new();
    for (x, y) in ramp_cells(map) {
        let profile = map.ramp_profile(x, y).expect("ramp cell");
        for h in half_edges() {
            let neighbour_is_ramp = {
                let nx = i64::from(x) + i64::from(h.offset.0);
                let ny = i64::from(y) + i64::from(h.offset.1);
                u32::try_from(nx)
                    .ok()
                    .zip(u32::try_from(ny).ok())
                    .is_some_and(|(nx, ny)| map.ramp_profile(nx, ny).is_some())
            };
            for s in [0.2_f32, 0.5, 0.8] {
                let f = h.a.lerp(h.b, s);
                let edge_h = ramp_height(profile, f.x, f.y);
                let beside = neighbour_surface(map, mesh, x, y, &h, f);
                match scope {
                    Closure::SideWalls if neighbour_is_ramp || beside > edge_h => continue,
                    Closure::SideWalls => {}
                }
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

fn assert_steps_closed(name: &str, map: &GameMap, scope: Closure) {
    let mesh = MapMesh::build(map);
    let failures = open_steps(map, &mesh, scope);
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
        Closure::SideWalls,
    );
}

#[test]
fn t3_ramp_edge_steps_are_closed_default_arena() {
    assert_steps_closed(
        "default_arena",
        &load_shipped_map("default_arena.scmap.ron"),
        Closure::SideWalls,
    );
}

#[test]
fn t3_ramp_edge_steps_are_closed_fixture() {
    assert_steps_closed("fixture", &fixture_map(), Closure::SideWalls);
}
