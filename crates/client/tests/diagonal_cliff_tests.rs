//! Straight diagonal cliffs: every 45° staircase draws as one straight chamfer through its low
//! cells' corners, wholly on the high side, and every void diagonal as a straight wall through
//! its edge midpoints (plan `docs/plans/2026-12-01-straight-diagonal-cliffs.md` §4.1, T-S1, T-S2,
//! T-S3, T-S4 and T-K; owner question Q0136, answer (c)).
//!
//! # Responsibility
//! These tests mesh whole maps with the production mesher and read the triangles back. They find
//! the diagonals to check with `support/terrain_mesh.rs`, which re-implements the plan's full-cut
//! rule (D-1) from the plan and the map's cells, never through the mesher's predicate, so a
//! mesher that cuts the wrong cells fails here instead of agreeing with itself.
//! - **T-S1** (`diagonal_runs_are_straight_*`): the wall vertices along each run lie on the run's
//!   lines: the foot on the line through the low cells' corners, the rim top
//!   `FOOT_WIDTH + RIM_WIDTH` behind it, the canonical rows `FOOT_WIDTH` behind it within the
//!   jitter, and the foot leaves no gap wider than a column spacing. Void runs are checked against
//!   the midpoint line moved by the void setback.
//! - **T-S2**, the fixture half (`full_cut_holds_once_per_cell_and_never_through_a_saddle`): the
//!   production predicate cuts the cells the analysis cuts, one direction per cell, and no cut's
//!   foot ends at a saddle. The table of 3x3 blocks is the unit test of the same name in
//!   `mesher.rs`.
//! - **T-S3** (`chamfer_floors_and_walls_share_their_edges_*`): the high floor stops on each
//!   cut's rim-top line, every floor vertex on a cut's rim-top line or foot line is a wall vertex
//!   bit for bit (no T-junction), and no floor triangle of a dual cell a cut touches is turned
//!   over.
//! - **T-S4** (`chunk_seams_through_a_run_are_watertight`): the chunk-seam comparison of
//!   `terrain_tests.rs` on the diagonal fixture, whose runs cross the seam `x = 15.5`.
//! - **T-K** (`brink_drop_under_a_body_is_bounded_*`): for every ground archetype's terrain radius
//!   from the real catalog, how far the drawn terrain under a body's centre lies below the cell's
//!   surface: at most `0.2` outside the cut cells (the terrain plan's brink limit), at most the
//!   cut's cliff height inside them, where it is reported (plan D-8, owner question Q0139). The
//!   quadrants an uncut cell's convex 45° half cut draws are reported, not limited: a body at
//!   such a corner already stands over the half cut's foot before this plan.
//! - **T-K′** (`brink_drop_where_the_simulation_admits_a_body_*`; plan
//!   `2026-12-02-diagonal-cliff-walkability` §6.3, B-251): T-K's measure at every centre the
//!   simulation admits (`sim_map`'s wall contact and segment predicate for that radius report it
//!   free), with the limit `0.2` in cut cells too, now that the simulation is clipped to the drawn
//!   diagonal (Q0139). It replaces T-K when the clip lands (that plan's WP2); until then T-K
//!   keeps guarding the uncut cells.
//!
//! Altitudes are compared exactly (`clippy::float_cmp` is allowed): the mesher writes every row
//! at a level's height or a fixed offset from it, bit for bit.
//!
//! # Tolerance
//! A mesh coordinate is a few `f32` operations on values below `M = max(width, height)`, so a
//! vertex's projection onto a line carries at most `16·ulp(M)`, `ulp(v) = 2^(⌊log₂ v⌋ − 23)`: for
//! a 64-cell map `τ = 1.22e-4`. Each test asserts its premise that every vertex lies within `M`
//! of the origin horizontally. The lines' constants are exact in `f64`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/terrain_mesh.rs"]
mod terrain_mesh;

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::Vec3;
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use client::terrain::mesher::{
    cliff_level_height, dual_corner_levels, full_cut, ABYSS_WALL_BOTTOM_Y, FOOT_RISE, FOOT_WIDTH,
    RIM_DROP, RIM_WIDTH, WALL_JITTER_AMPLITUDE, WALL_ROW_SPACING,
};
use sim_map::grid::{GameMap, CLIFF_LEVEL_HEIGHT};
use sim_map::{PathRequestWithVision, Pathfinder};
use sim_types::{MoverType, ShapeKind, SimScalar, SimVec2};
use terrain_mesh::{
    diagonal_cuts, diagonal_fixture, diagonal_runs, load_shipped_map, void_runs, DiagonalCut,
    MapMesh,
};

/// Rounding steps one vertex's projection may carry, as a multiple of the unit in the last place.
const ROUNDING_ULPS: f64 = 16.0;

/// The terrain plan's brink limit (plan `2026-11-30-terrain-and-air-indicators` D-4): how far the
/// drawn terrain under a body's centre may lie below its cell's surface, world units.
const BRINK_LIMIT: f32 = 0.2;

/// Steps per cell of the lattice of body centres T-K samples.
const BRINK_LATTICE: u32 = 16;

/// `1/√2` in `f64`.
const INV_SQRT2: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// Environment variable naming a directory: when set, T-S1 writes every measured wall vertex of
/// each map there as `<map>.csv` (kind, x, z, deviation), for the review capture's overlay.
const DEVIATIONS_VAR: &str = "SPACE_SCUFFLE_DIAGONAL_DEVIATIONS_OUT";

/// `τ` of `map`'s mesh, after asserting its premise: every vertex lies within `M` of the
/// origin horizontally, `M = max(width, height)` (the border dual cells reach half a cell past
/// the map, below `M`).
fn tolerance(map: &GameMap, mesh: &MapMesh) -> f64 {
    let m = map.width().max(map.height()) as f32;
    for p in mesh.tris.iter().flatten() {
        assert!(
            p.x.abs() <= m && p.z.abs() <= m,
            "premise: vertex {p} lies within {m} of the origin"
        );
    }
    ROUNDING_ULPS * 2.0_f64.powi(m.log2().floor() as i32 - 23)
}

/// The shipped maps and the fixture these tests read.
fn showcase_canyon() -> GameMap {
    load_shipped_map("showcase_canyon.scmap.ron")
}

fn default_arena() -> GameMap {
    load_shipped_map("default_arena.scmap.ron")
}

/// Whether triangle `t` is a floor: all three vertices at one altitude, bit for bit.
fn is_floor(t: &[Vec3; 3]) -> bool {
    t[0].y.to_bits() == t[1].y.to_bits() && t[1].y.to_bits() == t[2].y.to_bits()
}

// --- T-S1 ----------------------------------------------------------------------------------------

/// The lines one straight run must draw, in a frame along the run: `depth = n·x − line` (positive
/// towards the low side) and `along = u·x`.
#[derive(Debug, Clone, Copy)]
struct RunLines {
    /// What the run is, for failure messages.
    name: &'static str,
    /// Unit normal towards the low side.
    normal: (f64, f64),
    /// Unit vector along the run.
    along_axis: (f64, f64),
    /// The reference line's constant.
    line: f64,
    /// The interval of `along` the run covers.
    along: (f64, f64),
    /// Altitude of the wall's bottom rows, and of its top.
    y_foot: f32,
    y_top: f32,
    /// Whether the wall has a rim at `y_top`.
    rim: bool,
    /// Depth of the bottom rows and of the rim top.
    foot_depth: f64,
    rim_depth: f64,
    /// Depth around which canonical rows jitter, and by how much at most.
    row_depth: f64,
    row_jitter: f64,
    /// The widest gap between two foot vertices along the run.
    max_gap: f64,
}

impl RunLines {
    fn depth(&self, p: Vec3) -> f64 {
        self.normal.0 * f64::from(p.x) + self.normal.1 * f64::from(p.z) - self.line
    }

    fn along_of(&self, p: Vec3) -> f64 {
        self.along_axis.0 * f64::from(p.x) + self.along_axis.1 * f64::from(p.z)
    }

    /// Whether `p` lies in the corridor `K`: within the run's interval shrunk by half a cell at
    /// each end (the junctions are left out), from one cell behind the line to half a cell
    /// beyond it, and between the wall's bottom and top. Half a cell, not one, on the low side:
    /// a one-cell diagonal channel's opposite wall stands `1/√2` beyond the line.
    fn in_corridor(&self, p: Vec3) -> bool {
        let (a, d) = (self.along_of(p), self.depth(p));
        a >= self.along.0 + 0.5
            && a <= self.along.1 - 0.5
            && (-1.0..=0.5).contains(&d)
            && p.y >= self.y_foot
            && p.y <= self.y_top
    }
}

/// The lines of every ground run of `map` (D-1, D-3): foot through the low cells' corners, rim
/// top `FOOT_WIDTH + RIM_WIDTH` behind it, canonical rows `FOOT_WIDTH` behind it.
fn ground_run_lines(map: &GameMap) -> Vec<RunLines> {
    diagonal_runs(map)
        .iter()
        .map(|run| {
            let cut = run.first();
            RunLines {
                name: "ground run",
                normal: cut.normal(),
                along_axis: cut.along_axis(),
                line: cut.foot(),
                along: run.along(),
                y_foot: cliff_level_height(cut.lb),
                y_top: cliff_level_height(cut.t_hi),
                rim: cut.rim(),
                foot_depth: 0.0,
                rim_depth: -f64::from(FOOT_WIDTH + RIM_WIDTH),
                row_depth: -f64::from(FOOT_WIDTH),
                row_jitter: f64::from(WALL_JITTER_AMPLITUDE),
                // The largest column spacing along a chamfer: a cut cell's centre column to the
                // dual cell's centre (D-4).
                max_gap: INV_SQRT2,
            }
        })
        .collect()
}

/// The lines of every void run of `map` (D-6): the 45° wall through the void cells' edge
/// midpoints, every profile set back along its axis by the jitter's amplitude, so the lines move
/// by that setback over `√2`.
fn void_run_lines(map: &GameMap) -> Vec<RunLines> {
    let setback = f64::from(WALL_JITTER_AMPLITUDE) * INV_SQRT2;
    void_runs(map)
        .iter()
        .map(|run| RunLines {
            name: "void run",
            normal: run.normal(),
            along_axis: run.along_axis(),
            line: run.midpoint_line(),
            along: run.along(),
            y_foot: ABYSS_WALL_BOTTOM_Y,
            y_top: cliff_level_height(run.level),
            rim: true,
            foot_depth: -setback,
            rim_depth: -f64::from(WALL_JITTER_AMPLITUDE + RIM_WIDTH) * INV_SQRT2,
            row_depth: -setback,
            row_jitter: setback,
            // The two profiles of one 45° cut stand at its midpoints, each moved by the setback
            // along its own axis, so they are `√2·setback` closer than the midpoints' `1/√2` and
            // the gap across a corner as much wider.
            max_gap: INV_SQRT2 + std::f64::consts::SQRT_2 * f64::from(WALL_JITTER_AMPLITUDE),
        })
        .collect()
}

/// Whether `y` is the altitude of a canonical wall row of `lines`: a multiple of the row spacing
/// strictly between the foot's top (or the bottom) and the rim's bottom (or the top).
fn is_canonical_row(lines: &RunLines, y: f32) -> bool {
    let rows = y / WALL_ROW_SPACING;
    let foot_top = if lines.y_foot >= 0.0 {
        lines.y_foot + FOOT_RISE
    } else {
        lines.y_foot
    };
    let rim_bottom = if lines.rim {
        lines.y_top - RIM_DROP
    } else {
        lines.y_top
    };
    rows == rows.round() && y > foot_top && y < rim_bottom
}

/// T-S1's measure for one run: every failure, as text. Every vertex measured is appended to
/// `samples` as a CSV row `kind,x,z,deviation` (the deviation beyond its slack, world units).
fn chamfer_deviations(
    mesh: &MapMesh,
    lines: &RunLines,
    tau: f64,
    samples: &mut Vec<String>,
) -> Vec<String> {
    let mut failures = Vec::new();
    let mut foot_positions = Vec::new();
    let mut worst = 0.0_f64;
    for t in mesh.tris.iter().filter(|t| !is_floor(t)) {
        if !t.iter().all(|&p| lines.in_corridor(p)) {
            continue;
        }
        for &p in t {
            let depth = lines.depth(p);
            let (expected, slack, what) = if p.y == lines.y_foot {
                foot_positions.push(lines.along_of(p));
                (lines.foot_depth, 0.0, "foot")
            } else if p.y == lines.y_top && lines.rim {
                (lines.rim_depth, 0.0, "rim top")
            } else if is_canonical_row(lines, p.y) {
                (lines.row_depth, lines.row_jitter, "canonical row")
            } else {
                let mid = f64::midpoint(lines.rim_depth, lines.foot_depth.max(0.0));
                let half = 0.5 * (lines.foot_depth.max(0.0) - lines.rim_depth);
                (mid, half, "face")
            };
            let off = (depth - expected).abs() - slack;
            worst = worst.max(off.max(0.0));
            samples.push(format!("{},{},{},{}", lines.name, p.x, p.z, off.max(0.0)));
            if off > tau {
                failures.push(format!(
                    "{} along {:?}: {what} vertex {p} at depth {depth:.5}, expected {expected:.5} \
                     ± {slack:.5}",
                    lines.name, lines.along
                ));
            }
        }
    }
    // Continuity: the foot covers the corridor's interval without a gap wider than the largest
    // column spacing (D-4; for a void run, D-6's).
    foot_positions.push(lines.along.0 + 0.5);
    foot_positions.push(lines.along.1 - 0.5);
    foot_positions.sort_by(f64::total_cmp);
    let gap = foot_positions
        .windows(2)
        .map(|w| w[1] - w[0])
        .fold(0.0, f64::max);
    if gap > lines.max_gap + tau {
        failures.push(format!(
            "{} along {:?}: the foot leaves a gap of {gap:.5}",
            lines.name, lines.along
        ));
    }
    println!(
        "  {} along [{:.3}, {:.3}] at depth line {:.4}: largest chamfer deviation {worst:.3e}",
        lines.name, lines.along.0, lines.along.1, lines.line
    );
    failures
}

/// T-S1 on `map`: every ground and void run is straight; `expect_ground_runs` states whether the
/// map has ground runs at all.
fn assert_runs_are_straight(name: &str, map: &GameMap, expect_ground_runs: bool) {
    let mesh = MapMesh::build(map);
    let tau = tolerance(map, &mesh);
    let ground = ground_run_lines(map);
    let void = void_run_lines(map);
    println!(
        "{name}: {} ground runs, {} void runs, tau {tau:e}",
        ground.len(),
        void.len()
    );
    assert_eq!(
        !ground.is_empty(),
        expect_ground_runs,
        "{name}: ground runs {}",
        ground.len()
    );
    let mut samples = vec!["kind,x,z,deviation".to_owned()];
    let failures: Vec<String> = ground
        .iter()
        .chain(&void)
        .flat_map(|lines| chamfer_deviations(&mesh, lines, tau, &mut samples))
        .collect();
    if let Some(directory) = std::env::var_os(DEVIATIONS_VAR).map(std::path::PathBuf::from) {
        std::fs::create_dir_all(&directory).expect("the export directory is created");
        std::fs::write(directory.join(format!("{name}.csv")), samples.join("\n"))
            .expect("the export is written");
    }
    assert!(
        failures.is_empty(),
        "{name}: {} vertices off their run's lines, e.g.:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn diagonal_runs_are_straight_showcase_canyon() {
    assert_runs_are_straight("showcase_canyon", &showcase_canyon(), true);
}

#[test]
fn diagonal_runs_are_straight_default_arena() {
    assert_runs_are_straight("default_arena", &default_arena(), false);
}

#[test]
fn diagonal_runs_are_straight_diagonal_fixture() {
    assert_runs_are_straight("diagonal_fixture", &diagonal_fixture(), true);
}

// --- T-S2, the fixture half ----------------------------------------------------------------------

/// T-S2 (plan §4.1), on the fixture: the production predicate holds exactly where the
/// independent analysis finds a cut, in one direction per cell; and each cut's foot ends at
/// corners whose two cells on the cut's high side (`X` and the backing cell) are high and whose
/// third (`L`) is low, so no cut passes through a saddle.
#[test]
fn full_cut_holds_once_per_cell_and_never_through_a_saddle() {
    let map = diagonal_fixture();
    let expected: BTreeMap<(i32, i32), DiagonalCut> = diagonal_cuts(&map)
        .into_iter()
        .map(|c| (c.cell, c))
        .collect();
    let mut failures = Vec::new();
    for y in 0..map.height() as i32 {
        for x in 0..map.width() as i32 {
            let mut found = BTreeSet::new();
            for dir in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                for t in 0..=4 {
                    if full_cut(&map, (x, y), dir, t) {
                        found.insert((dir, t));
                    }
                }
            }
            let wanted: BTreeSet<((i32, i32), i32)> =
                expected.get(&(x, y)).map_or_else(BTreeSet::new, |c| {
                    ((c.lb + 1)..=c.t_hi).map(|t| (c.dir, t)).collect()
                });
            if found != wanted {
                failures.push(format!(
                    "cell ({x}, {y}): cut at {found:?}, expected {wanted:?}"
                ));
            }
            let directions: BTreeSet<(i32, i32)> = found.iter().map(|&(d, _)| d).collect();
            if directions.len() > 1 {
                failures.push(format!("cell ({x}, {y}) has cuts in {directions:?}"));
            }
        }
    }
    let level = |x: i32, y: i32| {
        u32::try_from(x)
            .ok()
            .zip(u32::try_from(y).ok())
            .and_then(|(ux, uy)| map.cell(ux, uy).ok())
            .map_or(-1, |c| i32::from(c.cliff_level.0))
    };
    for cut in expected.values() {
        let (x, y) = cut.cell;
        let (dx, dy) = cut.dir;
        let t = cut.lb + 1;
        // At P1 the backing cell is X - (0, dy) and L is X + (dx, 0); at P2, X - (dx, 0) and
        // X + (0, dy).
        for (backing, low) in [((x, y - dy), (x + dx, y)), ((x - dx, y), (x, y + dy))] {
            if level(x, y) < t || level(backing.0, backing.1) < t || level(low.0, low.1) >= t {
                failures.push(format!(
                    "cut {cut:?}: a foot end is a saddle or not a corner"
                ));
            }
        }
    }
    println!("diagonal_fixture: {} cut cells", expected.len());
    assert!(!expected.is_empty(), "the fixture has cuts");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// --- T-S3 ----------------------------------------------------------------------------------------

/// Bit patterns of a vertex, for exact comparison.
fn bits(p: Vec3) -> [u32; 3] {
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
}

/// The dual cells a cut touches, by their centres (grid points): the ones at its corner `C`, at
/// both ends of its foot, and at the corner opposite `C`.
fn touched_dual_cells(cut: &DiagonalCut) -> [(i32, i32); 4] {
    let (cx, cy) = cut.corner();
    let [p2, p1] = cut.foot_ends();
    [(cx, cy), p1, p2, (cx - cut.dir.0, cy - cut.dir.1)]
}

/// T-S3 on `map`: every failure, as text.
fn shared_edge_failures(map: &GameMap) -> Vec<String> {
    let mesh = MapMesh::build(map);
    let tau = tolerance(map, &mesh);
    let walls: BTreeSet<[u32; 3]> = mesh
        .tris
        .iter()
        .filter(|t| !is_floor(t))
        .flatten()
        .map(|&p| bits(p))
        .collect();
    let floors: Vec<&[Vec3; 3]> = mesh.tris.iter().filter(|t| is_floor(t)).collect();
    let rim = f64::from(FOOT_WIDTH + RIM_WIDTH);
    let mut failures = Vec::new();
    let mut checked = 0_usize;
    for cut in diagonal_cuts(map) {
        let (n, u, c) = (cut.normal(), cut.along_axis(), cut.foot());
        let along = |p: Vec3| u.0 * f64::from(p.x) + u.1 * f64::from(p.z);
        let depth = |p: Vec3| n.0 * f64::from(p.x) + n.1 * f64::from(p.z) - c;
        let [p2, p1] = cut.foot_ends();
        let (a0, a1) = (
            u.0 * f64::from(p2.0) + u.1 * f64::from(p2.1),
            u.0 * f64::from(p1.0) + u.1 * f64::from(p1.1),
        );
        let (y_foot, y_rim) = (cliff_level_height(cut.lb), cliff_level_height(cut.t_hi));
        let mut rim_vertices = 0_usize;
        for &p in floors.iter().copied().flatten() {
            if along(p) < a0 - tau || along(p) > a1 + tau {
                continue;
            }
            let on_foot = p.y == y_foot && depth(p).abs() <= tau;
            let on_rim = cut.rim() && p.y == y_rim && (depth(p) + rim).abs() <= tau;
            rim_vertices += usize::from(on_rim);
            if on_foot || on_rim {
                checked += 1;
                if !walls.contains(&bits(p)) {
                    let line = if on_foot { "foot" } else { "rim-top" };
                    failures.push(format!(
                        "cut {:?}: floor vertex {p} on the {line} line is no wall vertex",
                        cut.cell
                    ));
                }
            }
        }
        // The high floor stops at the chamfer's rim top, so the rim-top line carries floor
        // vertices: at least the anchor behind the cut cell's centre.
        if cut.rim() && rim_vertices == 0 {
            failures.push(format!(
                "cut {:?}: no floor vertex on its rim-top line",
                cut.cell
            ));
        }
        for (gx, gy) in touched_dual_cells(&cut) {
            let (signed, total) = floors
                .iter()
                .filter(|t| {
                    let centroid = (t[0] + t[1] + t[2]) / 3.0;
                    (centroid.x - gx as f32).abs() < 0.5 && (centroid.z - gy as f32).abs() < 0.5
                })
                .map(|t| (t[1] - t[0]).cross(t[2] - t[0]).y * 0.5)
                .fold((0.0_f32, 0.0_f32), |(s, a), area| {
                    (s + area, a + area.abs())
                });
            if f64::from(total - signed) > tau * 4.0 {
                failures.push(format!(
                    "cut {:?}: dual cell ({gx}, {gy}) has floor turned over ({signed} of {total})",
                    cut.cell
                ));
            }
        }
    }
    println!("  {checked} floor vertices on a cut's lines checked");
    failures
}

#[test]
fn chamfer_floors_and_walls_share_their_edges_showcase_canyon() {
    let failures = shared_edge_failures(&showcase_canyon());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn chamfer_floors_and_walls_share_their_edges_diagonal_fixture() {
    let failures = shared_edge_failures(&diagonal_fixture());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// --- T-S4 ----------------------------------------------------------------------------------------

/// T-S4 (plan §4.1, a guard): the fixture's chunk seams are watertight; its runs 11 and 14 put
/// cut centres on the seam `x = 15.5`, run 11 at two stacked levels.
#[test]
fn chunk_seams_through_a_run_are_watertight() {
    let map = diagonal_fixture();
    let on_seam: Vec<(i32, i32)> = diagonal_cuts(&map)
        .iter()
        .map(|c| c.cell)
        .filter(|&(x, _)| x == 15)
        .collect();
    assert!(
        on_seam.len() >= 3,
        "premise: cut centres on the seam x = 15.5, found {on_seam:?}"
    );
    let leaks = terrain_mesh::leaky_chunk_seams(&map);
    assert!(leaks.is_empty(), "{}", leaks.join("\n"));
}

// --- T-K -----------------------------------------------------------------------------------------

/// Every distinct terrain radius of a ground unit archetype (a mobile `UnitSphere`, not a
/// structure's dome) in the shipped catalog, read through the runtime loader (charter §3.3: no
/// archetype is named), with how many archetypes have it.
fn ground_radii() -> BTreeMap<u32, usize> {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let definition = BattleDefinition::load(&assets, DEFAULT_BATTLE_SCENARIO.as_ref())
        .expect("the shipped battle loads");
    let mut radii = BTreeMap::new();
    for archetype in definition.catalogs.units.archetypes() {
        if archetype.mover_type != MoverType::Air
            && matches!(archetype.shape, ShapeKind::UnitSphere)
        {
            *radii
                .entry(archetype.terrain_radius.to_f32().to_bits())
                .or_insert(0) += 1;
        }
    }
    radii
}

/// The cliff edges around cell `(x, y)`: every cell edge in its 3x3 block between two cells of
/// different marching levels where neither is a ramp, as segments `(a, b)` in world XZ. A body
/// whose disc reaches none of them stands clear of every cliff (the simulation's walls are the
/// cells' edges, `sim_map::pathfinding`).
fn cliff_edges_around(map: &GameMap, x: i32, y: i32) -> Vec<((f32, f32), (f32, f32))> {
    let level = |cx: i32, cy: i32| -> Option<i32> {
        let (Ok(ux), Ok(uy)) = (u32::try_from(cx), u32::try_from(cy)) else {
            return Some(-1);
        };
        let Ok(cell) = map.cell(ux, uy) else {
            return Some(-1);
        };
        if map.ramp_profile(ux, uy).is_some() {
            None
        } else if cell.terrain_kind == sim_map::grid::TerrainKind::Abyss {
            Some(-1)
        } else {
            Some(i32::from(cell.cliff_level.0))
        }
    };
    let cliff = |a: (i32, i32), b: (i32, i32)| match (level(a.0, a.1), level(b.0, b.1)) {
        (Some(la), Some(lb)) => la != lb,
        _ => false,
    };
    let mut edges = Vec::new();
    for cy in y - 1..=y + 1 {
        for cx in x - 1..=x + 1 {
            let (fx, fy) = (cx as f32, cy as f32);
            if cx < x + 1 && cliff((cx, cy), (cx + 1, cy)) {
                edges.push(((fx + 1.0, fy), (fx + 1.0, fy + 1.0)));
            }
            if cy < y + 1 && cliff((cx, cy), (cx, cy + 1)) {
                edges.push(((fx, fy + 1.0), (fx + 1.0, fy + 1.0)));
            }
        }
    }
    edges
}

/// Distance from point `p` to segment `(a, b)`.
fn segment_distance(p: (f32, f32), (a, b): ((f32, f32), (f32, f32))) -> f32 {
    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
    let s = (((p.0 - a.0) * ex + (p.1 - a.1) * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
    let (qx, qy) = (a.0 + s * ex - p.0, a.1 + s * ey - p.1);
    (qx * qx + qy * qy).sqrt()
}

/// The largest drop under a body of each radius (by its bits) and where: in uncut cells, in cut
/// cells, and in the quadrants of uncut cells that the mesher draws with a convex 45° half cut.
#[derive(Debug, Default)]
struct BrinkDrops {
    uncut: BTreeMap<u32, (f32, (f32, f32))>,
    cut: BTreeMap<u32, (f32, (f32, f32))>,
    convex: BTreeMap<u32, (f32, (f32, f32))>,
}

/// Which quadrants of cell `(x, y)` (bottom-left, bottom-right, top-right, top-left) are a lone
/// high corner of their dual cell at some threshold: corners the mesher draws with the convex
/// 45° cut through the two edge midpoints, which removes solid below the plateau's floor (module
/// documentation of `mesher.rs`, W1). That cut is the same before and after this plan wherever no
/// full cut replaces it, and a body at the corner stands over its foot: the terrain plan's brink
/// limit was set for straight walls, so T-K reports these quadrants beside the limit instead of
/// holding them to it.
fn convex_quadrants(map: &GameMap, x: i32, y: i32) -> [bool; 4] {
    // Quadrant q of the cell lies in the dual cell centred on the cell's corner q, where the cell
    // is corner (q + 2) % 4.
    let corners = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)];
    let mut convex = [false; 4];
    for (q, &(di, dj)) in corners.iter().enumerate() {
        let (levels, _) = dual_corner_levels(map, di as u32, dj as u32);
        let k = (q + 2) % 4;
        convex[q] = levels[k] > levels[(k + 1) % 4].max(levels[(k + 3) % 4]);
    }
    convex
}

/// The quadrant (bottom-left, bottom-right, top-right, top-left) of cell `(x, y)` that point `p`
/// lies in; a point on a quadrant boundary is in every quadrant it touches.
fn quadrants_of(x: i32, y: i32, p: (f32, f32)) -> [bool; 4] {
    let (fx, fy) = (p.0 - x as f32, p.1 - y as f32);
    let (left, right, bottom, top) = (fx <= 0.5, fx >= 0.5, fy <= 0.5, fy >= 0.5);
    [left && bottom, right && bottom, right && top, left && top]
}

/// Whether the simulation admits a body of terrain radius `r` with its centre at `p` (world XZ):
/// `sim_map`'s wall contact finds no wall within `r` and the segment predicate lets it stand.
fn simulation_admits(map: &GameMap, pathfinder: &mut Pathfinder, p: (f32, f32), r: f32) -> bool {
    let at = SimVec2::new(SimScalar::from_num(p.0), SimScalar::from_num(p.1));
    let request = PathRequestWithVision {
        start: at,
        goal: at,
        radius: SimScalar::from_num(r),
        vision: None,
        block_unexplored_ramps: false,
        known_structures: None,
    };
    pathfinder.wall_contact(map, &request, at).is_none()
        && pathfinder.segment_is_passable(map, &request, at, at)
}

/// Which body centres a brink measure samples, and the bound it holds cut cells to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Admission {
    /// T-K: every centre that keeps its terrain radius clear of the block's cliff edges; cut
    /// cells bounded by their cliff height.
    CliffEdges,
    /// T-K′: every such centre the simulation admits too; cut cells bounded by the brink limit.
    Simulation,
}

/// T-K's measure on `map`: every walkable plateau cell near a cliff, every body centre on the
/// `1/16` lattice that `admission` admits for its terrain radius. The cliff edges of the cell's
/// block are the first filter of both: the simulation's walls include them, so a centre within
/// `r` of one is never admitted.
fn brink_drops(
    map: &GameMap,
    radii: &BTreeMap<u32, usize>,
    admission: Admission,
) -> (BrinkDrops, Vec<String>) {
    let mesh = MapMesh::build(map);
    let mut pathfinder = Pathfinder::new(map);
    let cuts: BTreeMap<(i32, i32), DiagonalCut> = diagonal_cuts(map)
        .into_iter()
        .map(|c| (c.cell, c))
        .collect();
    let mut drops = BrinkDrops::default();
    let mut failures = Vec::new();
    let step = 1.0 / BRINK_LATTICE as f32;
    for y in 0..map.height() as i32 {
        for x in 0..map.width() as i32 {
            let (ux, uy) = (x as u32, y as u32);
            let cell = map.cell(ux, uy).expect("the cell lies on the map");
            if cell.terrain_kind == sim_map::grid::TerrainKind::Abyss
                || map.ramp_profile(ux, uy).is_some()
            {
                continue;
            }
            let edges = cliff_edges_around(map, x, y);
            if edges.is_empty() {
                // No cliff edge within a cell: the drawn floor is the cell's own (a cut's band
                // reaches 0.523 from its cell's centre, inside the block of every cell it covers).
                continue;
            }
            let surface = cliff_level_height(i32::from(cell.cliff_level.0));
            let cut = cuts.get(&(x, y));
            let convex = convex_quadrants(map, x, y);
            for &radius_bits in radii.keys() {
                let r = f32::from_bits(radius_bits);
                for j in 0..=BRINK_LATTICE {
                    for i in 0..=BRINK_LATTICE {
                        let p = (x as f32 + i as f32 * step, y as f32 + j as f32 * step);
                        if edges.iter().any(|&e| segment_distance(p, e) < r)
                            || (admission == Admission::Simulation
                                && !simulation_admits(map, &mut pathfinder, p, r))
                        {
                            continue;
                        }
                        let Some(top) = mesh.top_at(ux, uy, p.0, p.1) else {
                            failures.push(format!("no terrain under {p:?} in cell ({x}, {y})"));
                            continue;
                        };
                        let drop = surface - top;
                        let in_convex = quadrants_of(x, y, p)
                            .iter()
                            .zip(convex)
                            .any(|(&inside, convex)| inside && convex);
                        let (bound, slot) = match cut {
                            Some(_) if admission == Admission::Simulation => {
                                (BRINK_LIMIT, &mut drops.cut)
                            }
                            Some(c) => (
                                CLIFF_LEVEL_HEIGHT as f32 * (c.la - c.lb) as f32,
                                &mut drops.cut,
                            ),
                            None if in_convex => (f32::INFINITY, &mut drops.convex),
                            None => (BRINK_LIMIT, &mut drops.uncut),
                        };
                        let entry = slot.entry(radius_bits).or_insert((0.0, p));
                        if drop > entry.0 {
                            *entry = (drop, p);
                        }
                        if drop > bound {
                            failures.push(format!(
                                "radius {r}: the terrain under {p:?} in {} cell ({x}, {y}) lies \
                                 {drop:.4} below the surface (bound {bound})",
                                if cut.is_some() { "cut" } else { "uncut" }
                            ));
                        }
                    }
                }
            }
        }
    }
    (drops, failures)
}

/// T-K (or T-K′, by `admission`) on `map`: prints the per-radius maxima and fails on a drop
/// beyond its bound.
fn assert_brink_drops(name: &str, map: &GameMap, admission: Admission) {
    let radii = ground_radii();
    let archetypes: usize = radii.values().sum();
    let (drops, failures) = brink_drops(map, &radii, admission);
    println!(
        "{name} ({admission:?}): {archetypes} ground archetypes, {} radii",
        radii.len()
    );
    for (&bits, &count) in &radii {
        let show = |slot: &BTreeMap<u32, (f32, (f32, f32))>| {
            slot.get(&bits).map_or("none".to_owned(), |(d, p)| {
                format!("{d:.4} at ({:.4}, {:.4})", p.0, p.1)
            })
        };
        println!(
            "  radius {:.4} ({count} archetypes): uncut cells {}, cut cells {}, convex half \
             cuts {}",
            f32::from_bits(bits),
            show(&drops.uncut),
            show(&drops.cut),
            show(&drops.convex)
        );
    }
    assert!(
        failures.is_empty(),
        "{name}: {} body centres over a drop beyond its bound, e.g.:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn brink_drop_under_a_body_is_bounded_showcase_canyon() {
    assert_brink_drops("showcase_canyon", &showcase_canyon(), Admission::CliffEdges);
}

#[test]
fn brink_drop_under_a_body_is_bounded_diagonal_fixture() {
    assert_brink_drops(
        "diagonal_fixture",
        &diagonal_fixture(),
        Admission::CliffEdges,
    );
}

#[test]
#[ignore = "diagonal-walk WP2 (B-251): red until the simulation clips each cut's outer triangle"]
fn brink_drop_where_the_simulation_admits_a_body_showcase_canyon() {
    assert_brink_drops("showcase_canyon", &showcase_canyon(), Admission::Simulation);
}

#[test]
#[ignore = "diagonal-walk WP2 (B-251): red until the simulation clips each cut's outer triangle"]
fn brink_drop_where_the_simulation_admits_a_body_diagonal_fixture() {
    assert_brink_drops(
        "diagonal_fixture",
        &diagonal_fixture(),
        Admission::Simulation,
    );
}
