//! Diagonal cliff walkability: the simulation's walkable high ground of a 45° staircase step
//! stops at the drawn diagonal (plan `docs/plans/2026-12-02-diagonal-cliff-walkability.md` §6.1;
//! bug B-251; owner question Q0139).
//!
//! # Responsibility
//! These tests state, through `sim_map`'s public API, what the clip of each cut cell's outer
//! triangle `T(X)` means for every ground rule that reads terrain:
//! - **T-D2** (`predicate_matches_the_plan`, a guard): `GameMap::diagonal_cut` cuts exactly the
//!   cells the independent statement of the rule in `support/diagonal.rs` cuts.
//! - **T-W1** (`no_disc_stands_beyond_the_foot_*`): no disc of a catalog terrain radius stands
//!   closer than its radius to a foot, and every disc that keeps its radius from every wall
//!   stands free.
//! - **T-W2** (`no_route_lost_across_diagonals_*`, a guard): whether a route exists between
//!   sample points round every cut is the same as on `main` (recorded in
//!   `fixtures/diagonal_reachability.ron`).
//! - **T-W3** (`routes_and_contact_agree_on_the_clip`): every route T-W2 finds passes the
//!   segment predicate leg by leg, and the nearest reachable point to a goal in `T(X)` is no
//!   point of the cut cell closer than the radius to the foot.
//! - **T-M1** (`the_foot_is_a_wall_and_beyond_is_blocked`): the mesh walls every foot and only
//!   feet, blocks every triangle beyond one, and labels every other triangle as `main` does.
//! - **T-L** (`lane_cuts_ignore_diagonal_walls`, a guard): the terrain lane cuts are `main`'s
//!   (recorded in `fixtures/diagonal_lanes.ron`).
//! - **T-T1** (`segments_ending_beyond_the_foot_are_not_connected`): the grid traversal predicate
//!   refuses a segment that ends in `T(X)` and answers as on `main` between legal points
//!   (recorded in `fixtures/diagonal_segments.ron`).
//! - **T-P1** (`buildings_and_nodes_refuse_cut_cells`): building placement, resource placement
//!   and map validation refuse a footprint on a cut cell.
//!
//! Tests that need the clip are `#[ignore]`d with the work package that lands it (WP2): they were
//! written red first (WP0) and their red outputs are in the plan's evidence file, §2.
//!
//! # Records
//! The guards compare against files recorded on `main`'s behaviour. Setting
//! `SPACE_SCUFFLE_RECORD_DIAGONAL=1` writes them instead of comparing; it is never the default,
//! and a recorded file is reviewed like code.
//!
//! # Radii
//! Every radius is a distinct terrain radius of a ground body in the shipped unit catalog,
//! `assets/catalog/units.ron`, read at run time (charter §3.3: no archetype is named). `sim_map`
//! cannot depend on `sim_core`'s loader (charter §5), so the file is read through `ron` into the
//! fields the loader reads, with the loader's documented default: an omitted `terrain_radius`
//! equals `collision_radius` (`sim_core::catalog::UnitArchetype::terrain_radius`).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test maps are at most 256 cells a side; a broken law must fail the test loudly"
)]

#[path = "support/arena.rs"]
mod arena;
#[path = "support/diagonal.rs"]
mod diagonal;

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sim_map::{
    CliffLevel, DiagonalCut, EdgeKind, GameMap, LaneCuts, MapCell, NavMesh, PathRequestWithVision,
    Pathfinder, ResourceFootprint, ResourceFootprints, ResourceKind, ResourceNode, TriangleKind,
};
use sim_types::{FootprintShape, MoverType, ShapeKind, SimScalar, SimVec2};

use diagonal::{
    at_least_behind, cell_centre, closer_than_behind, diagonal_fixture, foot_side_raw,
    in_outer_triangle, independent_cut_of, independent_cuts, raw_point, segment_distance, to_f64,
    usable, walls_after_clip,
};

/// Environment variable that switches the guards from comparing to recording.
const RECORD_ENV: &str = "SPACE_SCUFFLE_RECORD_DIAGONAL";

/// Raw units per world unit (Q32.32).
const UNIT: i64 = 1 << 32;

/// Steps per cell of the lattice of disc centres T-W1 samples.
const DISC_LATTICE: i64 = 64;

/// The fields of one catalog unit that decide its terrain body.
#[derive(Deserialize)]
struct CatalogUnit {
    shape: ShapeKind,
    mover_type: MoverType,
    collision_radius: f32,
    terrain_radius: Option<f32>,
}

/// The shipped unit catalog's units.
#[derive(Deserialize)]
struct CatalogUnits {
    units: Vec<CatalogUnit>,
}

/// Every distinct terrain radius of a ground body (a mobile sphere that is not air) in the
/// shipped catalog, smallest first, with how many archetypes have it.
fn catalog_ground_radii() -> BTreeMap<i64, usize> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog/units.ron");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the shipped catalog {} must read: {e}", path.display()));
    let catalog: CatalogUnits = ron::from_str(&text)
        .unwrap_or_else(|e| panic!("the shipped catalog {} must parse: {e}", path.display()));
    let mut radii = BTreeMap::new();
    for unit in catalog.units {
        if unit.mover_type != MoverType::Air && matches!(unit.shape, ShapeKind::UnitSphere) {
            let radius = unit.terrain_radius.unwrap_or(unit.collision_radius);
            *radii.entry(SimScalar::from_num(radius).raw()).or_insert(0) += 1;
        }
    }
    assert!(!radii.is_empty(), "the catalog has ground bodies");
    radii
}

/// The distinct ground terrain radii of the catalog, smallest first.
fn radii() -> Vec<SimScalar> {
    catalog_ground_radii()
        .keys()
        .map(|&raw| SimScalar::from_raw(raw))
        .collect()
}

/// The showcase canyon, loaded from `assets/maps/showcase_canyon.scmap.ron`.
fn showcase_canyon() -> GameMap {
    arena::load_game_map(arena::SHOWCASE_CANYON_FILE)
}

/// The maps these tests read, by name.
fn maps() -> [(&'static str, GameMap); 3] {
    [
        ("diagonal_fixture", diagonal_fixture()),
        ("showcase_canyon", showcase_canyon()),
        ("default_arena", arena::default_arena()),
    ]
}

/// A request for a disc of `radius` standing at `p`, seeing every structure.
fn request(p: SimVec2, goal: SimVec2, radius: SimScalar) -> PathRequestWithVision<'static> {
    PathRequestWithVision {
        start: p,
        goal,
        radius,
        vision: None,
        block_unexplored_ramps: false,
        known_structures: None,
    }
}

/// Path of a record file under `tests/fixtures/`.
fn record_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Compares `value` with the record `name`, or writes it there in record mode.
fn compare_or_record<T>(name: &str, value: &T)
where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let path = record_path(name);
    if std::env::var_os(RECORD_ENV).is_some() {
        let text = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())
            .expect("a record serializes");
        std::fs::create_dir_all(path.parent().expect("invariant: a record has a directory"))
            .expect("the fixtures directory is writable");
        std::fs::write(&path, text + "\n").expect("the record is writable");
        println!("recorded {}", path.display());
        return;
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the record {} must read: {e}", path.display()));
    let recorded: T = ron::from_str(&text)
        .unwrap_or_else(|e| panic!("the record {} must parse: {e}", path.display()));
    assert_eq!(&recorded, value, "{name} differs from main's record");
}

// -------------------------------------------------------------------------------------------------
// T-D2
// -------------------------------------------------------------------------------------------------

/// T-D2 (guard): `GameMap::diagonal_cut` equals the independent rule cell by cell on the fixture
/// and both shipped maps; `default_arena` has no cut.
#[test]
fn predicate_matches_the_plan() {
    let mut failures = Vec::new();
    for (name, map) in maps() {
        let mut count = 0_usize;
        for y in 0..map.height() {
            for x in 0..map.width() {
                let (got, want) = (map.diagonal_cut(x, y), independent_cut_of(&map, x, y));
                count += usize::from(want.is_some());
                if got != want {
                    failures.push(format!("{name} ({x}, {y}): {got:?}, expected {want:?}"));
                }
            }
        }
        println!("{name}: {count} cut cells");
        let expect_cuts = name != "default_arena";
        if (count > 0) != expect_cuts {
            failures.push(format!(
                "{name}: {count} cuts, expected some: {expect_cuts}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// -------------------------------------------------------------------------------------------------
// T-W1
// -------------------------------------------------------------------------------------------------

/// T-W1 on `map`: for every cut and radius, every lattice centre of the cut cell's interior
/// closer than `r` to the foot (on the high side or beyond it) is in contact or not standable,
/// and every centre at least `r + 1/64` from the foot and from every other wall is free.
fn assert_no_disc_beyond_the_foot(name: &str, map: &GameMap) {
    let mut pathfinder = Pathfinder::new(map);
    let cuts = independent_cuts(map);
    assert!(!cuts.is_empty(), "premise: {name} has cuts");
    let margin = SimScalar::from_raw(UNIT / DISC_LATTICE);
    let mut failures = Vec::new();
    let (mut refused, mut admitted) = (0_usize, 0_usize);
    for cut in &cuts {
        let walls = walls_after_clip(map, cut.cell.0, cut.cell.1, 2);
        for r in radii() {
            let clear = r + margin;
            for j in 1..DISC_LATTICE {
                for i in 1..DISC_LATTICE {
                    let p = raw_point(
                        (i64::from(cut.cell.0) << 32) + i * (UNIT / DISC_LATTICE),
                        (i64::from(cut.cell.1) << 32) + j * (UNIT / DISC_LATTICE),
                    );
                    let req = request(p, p, r);
                    let contact = pathfinder.wall_contact(map, &req, p).is_some();
                    let standable = pathfinder.segment_is_passable(map, &req, p, p);
                    if closer_than_behind(cut, p, r) {
                        refused += 1;
                        if !contact && standable {
                            failures.push(format!(
                                "{name} cut {:?} r {}: {:?} stands free, {} raw past the line r \
                                 behind the foot",
                                cut.cell,
                                r.to_f32(),
                                to_f64(p),
                                foot_side_raw(cut, p)
                            ));
                        }
                    } else if at_least_behind(cut, p, clear)
                        && walls
                            .iter()
                            .all(|&w| segment_distance(to_f64(p), w) >= f64::from(clear.to_f32()))
                    {
                        admitted += 1;
                        if contact || !standable {
                            failures.push(format!(
                                "{name} cut {:?} r {}: {:?} keeps its radius from every wall but \
                                 contact {contact}, standable {standable}",
                                cut.cell,
                                r.to_f32(),
                                to_f64(p)
                            ));
                        }
                    }
                }
            }
        }
    }
    println!(
        "{name}: {} cuts, {} radii, {refused} centres that must be refused, {admitted} that must \
         stand free, {} failures",
        cuts.len(),
        radii().len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{name}: {} failures, e.g.:\n{}",
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
#[ignore = "diagonal-walk WP2 (B-251): red until the mesh walls each cut's foot"]
fn no_disc_stands_beyond_the_foot_diagonal_fixture() {
    assert_no_disc_beyond_the_foot("diagonal_fixture", &diagonal_fixture());
}

#[test]
#[ignore = "diagonal-walk WP2 (B-251): red until the mesh walls each cut's foot"]
fn no_disc_stands_beyond_the_foot_showcase_canyon() {
    assert_no_disc_beyond_the_foot("showcase_canyon", &showcase_canyon());
}

// -------------------------------------------------------------------------------------------------
// T-W2 and T-W3
// -------------------------------------------------------------------------------------------------

/// The sample point of cell `(x, y)` for a disc of radius `r`: the cell's centre, or, for a cut
/// cell, the nearest point on the `1/64` lattice along the diagonal from its centre that lies at
/// least `r + 1/64` behind the foot (the nearest legal point of a cut cell).
fn sample_point(map: &GameMap, x: u32, y: u32, r: SimScalar) -> SimVec2 {
    let centre = cell_centre(x, y);
    let Some(cut) = independent_cut_of(map, x, y) else {
        return centre;
    };
    // The smallest m with m·√2/64 >= r + 1/64, exactly: 2·m² >= (64·r + 1)², in raw units.
    let need = i128::from(r.raw()) * i128::from(DISC_LATTICE) + i128::from(UNIT);
    let step = UNIT / DISC_LATTICE;
    let mut m = 0_i64;
    while 2 * i128::from(m * UNIT) * i128::from(m * UNIT) < need * need {
        m += 1;
    }
    raw_point(
        centre.x.raw() - i64::from(cut.dir.0) * m * step,
        centre.y.raw() - i64::from(cut.dir.1) * m * step,
    )
}

/// The cells T-W2 samples on `map`: every walkable cell within one cell of a cut, row by row.
fn sample_cells(map: &GameMap) -> Vec<(u32, u32)> {
    let mut cells = std::collections::BTreeSet::new();
    for cut in independent_cuts(map) {
        let (cx, cy) = (i64::from(cut.cell.0), i64::from(cut.cell.1));
        for j in cy - 1..=cy + 1 {
            for i in cx - 1..=cx + 1 {
                if usable(map, i, j) {
                    cells.insert((j as u32, i as u32));
                }
            }
        }
    }
    cells.into_iter().map(|(y, x)| (x, y)).collect()
}

/// The reachability classes of one map and radius: sample `k` is in class `classes[k]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReachabilityRecord {
    map: String,
    radius_raw: i64,
    cells: Vec<(u32, u32)>,
    classes: Vec<u32>,
}

/// T-W2's classes on `map` for radius `r`, and every funnel route found on the way. Routes from a
/// sample to an earlier class's representative decide its class: for one radius, reachability
/// between points where the disc fits is an equivalence, so the classes determine whether a route
/// exists between every pair of samples.
fn reachability(
    name: &str,
    map: &GameMap,
    pathfinder: &mut Pathfinder,
    r: SimScalar,
) -> (ReachabilityRecord, Vec<Vec<SimVec2>>) {
    let cells = sample_cells(map);
    let mut representatives: Vec<SimVec2> = Vec::new();
    let mut classes = Vec::with_capacity(cells.len());
    let mut routes = Vec::new();
    for &(x, y) in &cells {
        let p = sample_point(map, x, y, r);
        let mut class = None;
        for (k, &rep) in representatives.iter().enumerate() {
            let req = request(p, rep, r);
            let fallbacks = pathfinder.mesh_funnel_fallbacks();
            if let Ok(route) = pathfinder.find_path_with_vision(map, &req) {
                // A portal-midpoint fallback (B-141: kept by design for wedged starts and exact
                // diagonal pinches) makes no claim about its legs; only funnel routes are kept.
                if pathfinder.mesh_funnel_fallbacks() == fallbacks {
                    routes.push(route.waypoints);
                }
                class = Some(k);
                break;
            }
        }
        let class = class.unwrap_or_else(|| {
            representatives.push(p);
            representatives.len() - 1
        });
        classes.push(u32::try_from(class).expect("invariant: few classes"));
    }
    (
        ReachabilityRecord {
            map: name.to_owned(),
            radius_raw: r.raw(),
            cells,
            classes,
        },
        routes,
    )
}

/// T-W2 on `map`: the reachability classes of every radius equal the record.
fn assert_no_route_lost(name: &str, map: &GameMap) {
    let mut pathfinder = Pathfinder::new(map);
    let records: Vec<ReachabilityRecord> = radii()
        .into_iter()
        .map(|r| reachability(name, map, &mut pathfinder, r).0)
        .collect();
    for record in &records {
        println!(
            "{name} r {}: {} samples in {} classes",
            SimScalar::from_raw(record.radius_raw).to_f32(),
            record.cells.len(),
            record.classes.iter().max().map_or(0, |m| m + 1)
        );
    }
    compare_or_record(&format!("diagonal_reachability_{name}.ron"), &records);
}

#[test]
fn no_route_lost_across_diagonals_diagonal_fixture() {
    assert_no_route_lost("diagonal_fixture", &diagonal_fixture());
}

#[test]
fn no_route_lost_across_diagonals_showcase_canyon() {
    assert_no_route_lost("showcase_canyon", &showcase_canyon());
}

/// T-W3: every funnel route T-W2 finds passes `segment_is_passable` leg by leg (but for legs from a
/// point where the disc does not fit, which a unit may always leave), and the nearest
/// reachable point to a goal inside `T(X)`, from the cell behind the cut on its level, is no
/// point of the cut cell closer than the radius to the foot.
#[test]
#[ignore = "diagonal-walk WP2 (B-251): red until the mesh walls each cut's foot"]
fn routes_and_contact_agree_on_the_clip() {
    let mut failures = Vec::new();
    for (name, map) in [
        ("diagonal_fixture", diagonal_fixture()),
        ("showcase_canyon", showcase_canyon()),
    ] {
        let mut pathfinder = Pathfinder::new(&map);
        let mut legs = 0_usize;
        for r in radii() {
            let (_, routes) = reachability(name, &map, &mut pathfinder, r);
            for route in routes {
                // A unit standing where its body does not fit may always leave (pathfinding plan
                // D-3), so a leg from such a point (the start, or a corner on the way out of a
                // pinch the start lies in) is exempt.
                for leg in route.windows(2) {
                    let fits = {
                        let req = request(leg[0], leg[0], r);
                        pathfinder.wall_contact(&map, &req, leg[0]).is_none()
                            && pathfinder.segment_is_passable(&map, &req, leg[0], leg[0])
                    };
                    if !fits {
                        continue;
                    }
                    legs += 1;
                    let req = request(leg[0], leg[1], r);
                    if !pathfinder.segment_is_passable(&map, &req, leg[0], leg[1]) {
                        failures.push(format!(
                            "{name} r {}: leg {:?} -> {:?} is not passable",
                            r.to_f32(),
                            to_f64(leg[0]),
                            to_f64(leg[1])
                        ));
                    }
                }
            }
        }
        let mut goals = 0_usize;
        for cut in independent_cuts(&map) {
            let Some(start) = behind_cell(&map, &cut) else {
                continue;
            };
            // A goal deep in the outer triangle: an eighth of a cell from C along the diagonal.
            let corner = diagonal::low_corner(&cut);
            let eighth = UNIT / 8;
            let goal = raw_point(
                corner.x.raw() - i64::from(cut.dir.0) * eighth,
                corner.y.raw() - i64::from(cut.dir.1) * eighth,
            );
            assert!(
                in_outer_triangle(&cut, goal),
                "premise: the goal lies in T(X)"
            );
            for r in radii() {
                goals += 1;
                let req = request(cell_centre(start.0, start.1), goal, r);
                match pathfinder.nearest_reachable_point(&map, &req) {
                    Ok(Some(p)) if diagonal::in_cell(&cut, p) && closer_than_behind(&cut, p, r) => {
                        failures.push(format!(
                            "{name} cut {:?} r {}: the nearest reachable point {:?} is closer than \
                             r behind the foot",
                            cut.cell,
                            r.to_f32(),
                            to_f64(p)
                        ));
                    }
                    Ok(_) => {}
                    Err(e) => failures.push(format!(
                        "{name} cut {:?} r {}: nearest reachable point failed: {e}",
                        cut.cell,
                        r.to_f32()
                    )),
                }
            }
        }
        println!("{name}: {legs} legs, {goals} goals in outer triangles");
    }
    assert!(
        failures.is_empty(),
        "{} failures, e.g.:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// A cell behind cut `cut` on its level: the first of `X − (dx, 0)`, `X − (0, dy)` and
/// `X − (dx, dy)` whose level is `X`'s and which is usable.
fn behind_cell(map: &GameMap, cut: &DiagonalCut) -> Option<(u32, u32)> {
    let (x, y) = (i64::from(cut.cell.0), i64::from(cut.cell.1));
    let (dx, dy) = (i64::from(cut.dir.0), i64::from(cut.dir.1));
    [(x - dx, y), (x, y - dy), (x - dx, y - dy)]
        .into_iter()
        .filter(|&(i, j)| usable(map, i, j))
        .map(|(i, j)| (i as u32, j as u32))
        .find(|&(i, j)| {
            map.cell(i, j)
                .is_ok_and(|c| i64::from(c.cliff_level.0) == i64::from(cut.la))
        })
}

// -------------------------------------------------------------------------------------------------
// T-M1 and T-L
// -------------------------------------------------------------------------------------------------

/// The foot of `cut` in lattice units (quarter cells): its two ends `P₂`, `P₁`.
fn foot_in_lattice(cut: &DiagonalCut) -> [(i32, i32); 2] {
    let corner = (
        4 * (cut.cell.0 as i32 + i32::from(cut.dir.0 > 0)),
        4 * (cut.cell.1 as i32 + i32::from(cut.dir.1 > 0)),
    );
    [
        (corner.0 - 4 * i32::from(cut.dir.0), corner.1),
        (corner.0, corner.1 - 4 * i32::from(cut.dir.1)),
    ]
}

/// T-M1 on the fixture: diagonal wall runs are exactly the feet, triangles beyond a foot are
/// blocked, and every other triangle carries the label of `main`'s rule for its centroid cell
/// (pathfinding plan D-2: open where the cell's terrain is usable, its level and ramp).
#[test]
#[ignore = "diagonal-walk WP2 (B-251): red until the mesh walls each cut's foot"]
fn the_foot_is_a_wall_and_beyond_is_blocked() {
    let map = diagonal_fixture();
    let mesh = NavMesh::build(&map).expect("the fixture builds a mesh");
    let cuts = independent_cuts(&map);
    let mut failures = Vec::new();
    // Every foot, as the set of its four unit lattice edges.
    let mut feet = std::collections::BTreeSet::new();
    for cut in &cuts {
        let [a, b] = foot_in_lattice(cut);
        let step = ((b.0 - a.0).signum(), (b.1 - a.1).signum());
        for k in 0..4 {
            let p = (a.0 + k * step.0, a.1 + k * step.1);
            let q = (p.0 + step.0, p.1 + step.1);
            feet.insert((p.min(q), p.max(q)));
        }
    }
    let mut covered = std::collections::BTreeSet::new();
    for run in mesh.constraint_runs() {
        let (from, to) = ((run.from.x, run.from.y), (run.to.x, run.to.y));
        let diagonal = from.0 != to.0 && from.1 != to.1;
        if !diagonal || run.kind != EdgeKind::Wall {
            continue;
        }
        let step = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
        let mut p = from;
        while p != to {
            let q = (p.0 + step.0, p.1 + step.1);
            let edge = (p.min(q), p.max(q));
            if feet.contains(&edge) {
                covered.insert(edge);
            } else {
                failures.push(format!("diagonal wall edge {edge:?} lies on no foot"));
            }
            p = q;
        }
    }
    for edge in feet.difference(&covered) {
        failures.push(format!("foot edge {edge:?} is not a wall"));
    }
    let by_cell: BTreeMap<(u32, u32), DiagonalCut> = cuts.iter().map(|c| (c.cell, *c)).collect();
    for t in 0..mesh.triangle_count() as u32 {
        let [a, b, c] = mesh.triangle_vertices(t).map(|v| mesh.vertex(v));
        // Three times the centroid, in raw units: lattice units are 2³⁰ raw.
        let (sx, sy) = (
            i64::from(a.x + b.x + c.x) << 30,
            i64::from(a.y + b.y + c.y) << 30,
        );
        let centroid = raw_point(sx / 3, sy / 3);
        let (cx, cy) = (
            (i64::from(a.x + b.x + c.x).div_euclid(12)) as u32,
            (i64::from(a.y + b.y + c.y).div_euclid(12)) as u32,
        );
        let label = mesh.label(t);
        let beyond = by_cell.get(&(cx, cy)).is_some_and(|cut| {
            // Exact on three times the centroid: 3·(dx·x + dy·y) > 3·k.
            i128::from(cut.dir.0) * i128::from(sx) + i128::from(cut.dir.1) * i128::from(sy)
                > 3 * (i128::from(diagonal::foot_k(cut)) << 32)
        });
        let cell = map.cell(cx, cy).expect("the centroid lies on the map");
        let kind = if beyond {
            TriangleKind::Blocked
        } else if usable(&map, i64::from(cx), i64::from(cy)) {
            TriangleKind::Open
        } else {
            TriangleKind::Blocked
        };
        if label.kind != kind || label.cliff_level != cell.cliff_level.0 || label.ramp != cell.ramp
        {
            failures.push(format!(
                "triangle {t} (centroid {:?}, cell ({cx}, {cy})): label {label:?}, expected {kind:?} \
                 at level {}",
                to_f64(centroid),
                cell.cliff_level.0
            ));
        }
    }
    println!(
        "diagonal_fixture: {} cuts, {} foot edges, {} triangles",
        cuts.len(),
        feet.len(),
        mesh.triangle_count()
    );
    assert!(
        failures.is_empty(),
        "{} failures, e.g.:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The lane cuts of one map, as recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LaneRecord {
    map: String,
    /// `(obstacle, x, bottom, top, below)` of every cut, in order.
    cuts: Vec<(u32, i32, i32, i32, Option<u32>)>,
    /// `max_edge_crossings` of the map's mesh; recorded only for a map with no cut, whose mesh
    /// the clip does not change.
    max_edge_crossings: Option<u32>,
}

/// T-L (guard): `LaneCuts::build` on the fixture and the canyon equals `main`'s record, and on
/// `default_arena` (no cut) so does `max_edge_crossings`.
#[test]
fn lane_cuts_ignore_diagonal_walls() {
    let records: Vec<LaneRecord> = maps()
        .into_iter()
        .map(|(name, map)| {
            let cuts = LaneCuts::build(&map);
            let max_edge_crossings = (name == "default_arena").then(|| {
                let mesh = NavMesh::build(&map).expect("the arena builds a mesh");
                cuts.max_edge_crossings(&mesh)
            });
            LaneRecord {
                map: name.to_owned(),
                cuts: cuts
                    .cuts()
                    .iter()
                    .map(|c| (c.obstacle, c.x, c.bottom, c.top, c.below))
                    .collect(),
                max_edge_crossings,
            }
        })
        .collect();
    compare_or_record("diagonal_lanes.ron", &records);
}

// -------------------------------------------------------------------------------------------------
// T-T1
// -------------------------------------------------------------------------------------------------

/// The answers of `ground_segment_is_connected` on one map between legal points round its cuts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SegmentRecord {
    map: String,
    pairs: u64,
    connected: u64,
    /// FNV-1a over the answers in order, one byte each.
    digest: u64,
}

/// The legal points round `cut` on the quarter lattice of its 3x3 block: points in no outer
/// triangle (a point on a foot is legal).
fn legal_points(map: &GameMap, cut: &DiagonalCut) -> Vec<SimVec2> {
    let quarter = UNIT / 4;
    let (x0, y0) = (i64::from(cut.cell.0) - 1, i64::from(cut.cell.1) - 1);
    let mut points = Vec::new();
    for j in 0..=12 {
        for i in 0..=12 {
            let (px, py) = ((x0 << 32) + i * quarter, (y0 << 32) + j * quarter);
            let p = raw_point(px, py);
            let off_map = px < 0
                || py < 0
                || px >= i64::from(map.width()) << 32
                || py >= i64::from(map.height()) << 32;
            if off_map {
                continue;
            }
            let in_some_triangle = [-1_i64, 0]
                .into_iter()
                .flat_map(|b| [-1_i64, 0].map(move |a| (a, b)))
                .filter_map(|(a, b)| {
                    let cx = u32::try_from((px >> 32) + a).ok()?;
                    let cy = u32::try_from((py >> 32) + b).ok()?;
                    independent_cut_of(map, cx, cy)
                })
                .any(|other| in_outer_triangle(&other, p));
            if !in_some_triangle {
                points.push(p);
            }
        }
    }
    points
}

/// T-T1: a segment that ends in `T(X)` is not connected; between legal points the predicate
/// answers as on `main` (L-1), by the record.
#[test]
#[ignore = "diagonal-walk WP2 (B-251): red until the traversal predicate tests its endpoints"]
fn segments_ending_beyond_the_foot_are_not_connected() {
    let mut failures = Vec::new();
    for (name, map) in [
        ("diagonal_fixture", diagonal_fixture()),
        ("showcase_canyon", showcase_canyon()),
    ] {
        for cut in independent_cuts(&map) {
            let Some(start) = behind_cell(&map, &cut) else {
                continue;
            };
            let from = cell_centre(start.0, start.1);
            let corner = diagonal::low_corner(&cut);
            let eighth = UNIT / 8;
            let inside = raw_point(
                corner.x.raw() - i64::from(cut.dir.0) * eighth,
                corner.y.raw() - i64::from(cut.dir.1) * eighth,
            );
            for (a, b) in [(from, inside), (inside, from), (inside, inside)] {
                if map.ground_segment_is_connected(a, b, |_, _| true) {
                    failures.push(format!(
                        "{name} cut {:?}: {:?} -> {:?} ends in the outer triangle but is connected",
                        cut.cell,
                        to_f64(a),
                        to_f64(b)
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures, e.g.:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// T-T1, the guard half: between legal points round every cut, `ground_segment_is_connected`
/// answers as on `main`, by the record (L-1: such a segment meets an outer triangle only by
/// crossing a cliff edge, which the cell rule refuses already).
#[test]
fn segments_between_legal_points_answer_as_on_main() {
    let mut records = Vec::new();
    for (name, map) in [
        ("diagonal_fixture", diagonal_fixture()),
        ("showcase_canyon", showcase_canyon()),
    ] {
        let (mut pairs, mut connected) = (0_u64, 0_u64);
        let mut digest = 0xcbf2_9ce4_8422_2325_u64;
        for cut in independent_cuts(&map) {
            let points = legal_points(&map, &cut);
            for &a in &points {
                for &b in &points {
                    let answer = map.ground_segment_is_connected(a, b, |_, _| true);
                    pairs += 1;
                    connected += u64::from(answer);
                    digest = (digest ^ u64::from(answer)).wrapping_mul(0x0100_0000_01b3);
                }
            }
        }
        println!("{name}: {pairs} segments, {connected} connected");
        records.push(SegmentRecord {
            map: name.to_owned(),
            pairs,
            connected,
            digest,
        });
    }
    compare_or_record("diagonal_segments.ron", &records);
}

// -------------------------------------------------------------------------------------------------
// T-P1
// -------------------------------------------------------------------------------------------------

/// T-P1: a building footprint covering a cut cell, a resource node on one, and a watchtower moved
/// onto one in a shipped map's document are all refused.
#[test]
#[ignore = "diagonal-walk WP2 (B-251): red until placement refuses cut cells"]
fn buildings_and_nodes_refuse_cut_cells() {
    let mut failures = Vec::new();
    let map = diagonal_fixture();
    let cuts = independent_cuts(&map);
    for cut in &cuts {
        let (x, y) = cut.cell;
        if map.can_place_building(x, y, 1, 1).is_ok() {
            failures.push(format!("a 1x1 building on cut cell ({x}, {y}) is admitted"));
        }
    }
    // A mineral node anchored on each cut cell, with a one-cell footprint.
    let table = ResourceFootprints::new(vec![(
        ResourceKind::Mineral,
        ResourceFootprint {
            cells: (0, 0, 1, 1),
            shape: FootprintShape::full(1, 1),
            remove_when_empty: true,
        },
    )]);
    for cut in &cuts {
        let (x, y) = cut.cell;
        let cells = (0..map.height())
            .flat_map(|j| (0..map.width()).map(move |i| (i, j)))
            .map(|(i, j)| map.cell(i, j).expect("the cell lies on the map").clone())
            .collect::<Vec<MapCell>>();
        let half = |v: u32| SimScalar::from_raw((i64::from(v) << 32) + (1 << 31));
        let node = ResourceNode {
            pos: SimVec2::new(half(x), half(y)),
            kind: ResourceKind::Mineral,
            remaining: 1_500,
            cell: (x, y),
            placed: None,
        };
        let mut with_node =
            GameMap::from_cells(map.width(), map.height(), cells, vec![node], Vec::new())
                .expect("the fixture's cells rebuild a map");
        if with_node.place_resource_footprints(&table).is_ok() {
            failures.push(format!("a mineral node on cut cell ({x}, {y}) is admitted"));
        }
    }
    // A shipped map's watchtower moved onto a cut cell gains a validation error.
    let mut doc = arena::load_document(arena::SHOWCASE_CANYON_FILE);
    let canyon = doc.to_game_map().expect("the canyon converts");
    let base = sim_map::validate_map(&doc).errors.len();
    let cut = independent_cuts(&canyon)
        .into_iter()
        .next()
        .expect("premise: the canyon has cuts");
    if let Some(tower) = doc.watchtowers.first_mut() {
        tower.cell = cut.cell;
        let errors = sim_map::validate_map(&doc).errors.len();
        if errors <= base {
            failures.push(format!(
                "a watchtower on cut cell {:?} adds no validation error ({errors} errors)",
                cut.cell
            ));
        }
    } else {
        failures.push("premise: the canyon has a watchtower".to_owned());
    }
    println!("{} cuts tried", cuts.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The fixture's premise: its rows are the client's and it builds a navigation mesh.
#[test]
fn the_fixture_builds_a_mesh() {
    let map = diagonal_fixture();
    assert_eq!(
        map.cell(5, 1).expect("on the map").cliff_level,
        CliffLevel(1)
    );
    assert!(NavMesh::build(&map).is_ok(), "the fixture builds a mesh");
}
