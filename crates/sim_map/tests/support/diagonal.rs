//! The diagonal fixture and an independent statement of the full-cut rule, for the `sim_map`
//! and `sim_core` tests of plan `docs/plans/2026-12-02-diagonal-cliff-walkability.md` (§6,
//! "Support"; bug B-251).
//!
//! # Responsibility
//! - [`diagonal_fixture`]: the client fixture of plan `2026-12-01-straight-diagonal-cliffs` §4.0,
//!   copied in the simulation's terms (its rows are the client's, character for character): runs
//!   with both end kinds, a one-step notch, ridges one and two cells wide, a three-cell band, a
//!   saddle channel, a void run, a void-to-ground run, a stacked staircase, a run beside a ramp,
//!   two runs meeting at a wall point and runs crossing a chunk seam.
//! - [`independent_cuts`]: the drawing plan's rule D-1 re-implemented from the plan text, reading
//!   the map's cells directly and never calling `sim_map::diagonal`, so a predicate that cuts the
//!   wrong cells fails against it instead of agreeing with itself. `sim_map` cannot depend on
//!   `client`, so the rule is copied, never imported.
//! - The outer triangle `T(X)` of a cut and the exact distances behind its foot that the
//!   walkability tests sample against.
//!
//! # How tests use it
//! Include it with `#[path = "support/diagonal.rs"]` (from `sim_core`'s tests with
//! `#[path = "../../sim_map/tests/support/diagonal.rs"]`). Each binary uses a subset.
//!
//! # Coordinates
//! Grid cells and world units coincide: a cell `(x, y)` spans `[x, x + 1] x [y, y + 1]`.

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test support: each binary uses a subset, and a broken fixture fails the test loudly"
)]

use sim_map::{CellFlags, CliffLevel, DiagonalCut, GameMap, MapCell, RampDir, TerrainKind};
use sim_types::{SimScalar, SimVec2};

/// Width of the diagonal fixture in cells: four 16-cell chunks.
pub const DIAGONAL_FIXTURE_WIDTH: u32 = 64;

/// Height of the diagonal fixture in cells.
pub const DIAGONAL_FIXTURE_HEIGHT: u32 = 48;

/// The cells of [`diagonal_fixture`], row 0 (`y = 0`) first and column `x` at character `x`:
/// `0` to `2` a plateau of that level, `a` abyss, `s` a level-0 ramp cell ascending south.
///
/// The features, each in its own box on level-0 ground, at least one cell from the next
/// (numbered by the rows of plan §4.0):
/// - (1) `x 1..=8, y 1..=8`: a level-1 staircase `x + y <= 11`, both ends on straight walls; its
///   three other corners are rectangle corners (6).
/// - (14) `x 11..=21, y 1..=11`: a staircase `y <= x - 6`, whose cut cell `(15, 9)` has its centre on
///   the chunk seam `x = 15.5`.
/// - (2) `x 24..=37, y 1..=11`: a staircase that ends at the concave corner `(32, 6)` of a long wall.
/// - (3) `x 40..=50, y 1..=8`: a one-step notch at `x = 45` between two long walls.
/// - (12) `x 53..=62, y 1..=10`: a staircase whose top end `(56, 10)` lies beside the ramp `s` at
///   `(55..=56, 11)`, so the last cut before it falls back.
/// - (11) `x 4..=26, y 14..=30`: a stacked staircase 0/1/2 (`x + y <= 43` level 1, `<= 40` level 2),
///   both runs crossing the seam `x = 15.5` with a cut centre on it.
/// - (7) `x 29..=46, y 15..=21`: a one-cell diagonal ridge, a two-cell band and a three-cell band.
/// - (8) `x 51..=62, y 14..=30`: two plateaus with a one-cell diagonal low channel between them.
/// - (4) `x 29..=40, y 24..=31`: an isolated concave corner, ground (an L).
/// - (5) `x 1..=12, y 34..=46`: an isolated concave corner, void (an L around abyss).
/// - (9) `x 15..=28, y 34..=46`: a diagonal abyss canyon through level-1 ground.
/// - (10) `x 32..=43, y 35..=46`: a staircase whose low side is abyss from `y = 40` up, ground below.
/// - (13) `x 45..=62, y 35..=43`: a roof whose two staircases meet at the apex `(54, 44)`.
pub const DIAGONAL_FIXTURE_ROWS: [&str; 48] = [
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0111111110011111111111001111111111111100111111111110011111111110",
    "0111111110011111111111001111111111111100111111111110011111111110",
    "0111111110011111111111001111111111111100111111111110011111111110",
    "0111111100011111111111001111111111111100111111111110011111111110",
    "0111111000011111111111001111111111111100111111111110011111111100",
    "0111110000001111111111001111111100000000111111111110011111111000",
    "0111100000000111111111001111111000000000111111111110011111110000",
    "0111000000000011111111001111110000000000111110000000011111100000",
    "0000000000000001111111001111100000000000000000000000011111000000",
    "0000000000000000111111001111000000000000000000000000011110000000",
    "0000000000000000011111001110000000000000000000000000000ss0000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000222222222222222222222220000000000000000000000001111111111010",
    "0000222222222222222222222210010001100001110000000001111111110110",
    "0000222222222222222222222110001000110000111000000001111111101110",
    "0000222222222222222222221110000100011000011100000001111111011110",
    "0000222222222222222222211100000010001100001110000001111110111110",
    "0000222222222222222222111000000001000110000111000001111101111110",
    "0000222222222222222221110000000000100011000011100001111011111110",
    "0000222222222222222211100000000000010001100000000001110111111110",
    "0000222222222222222111000000000000000000000000000001101111111110",
    "0000222222222222221110000000000000000000000000000001011111111110",
    "0000222222222222211100000000011111111111100000000000111111111110",
    "0000222222222222111000000000011111111111100000000001111111111110",
    "0000222222222221110000000000011111111111100000000001111111111110",
    "0000222222222211100000000000011111111111100000000001111111111110",
    "0000222222222111000000000000011110000000000000000001111111111110",
    "0000222222221110000000000000011110000000000000000001111111111110",
    "0000222222211100000000000000011110000000000000000001111111111110",
    "0000000000000000000000000000011110000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "01111111111110011aaaa1111111100000000000000000000000000000000000",
    "011111111111100111aaaa111111100011111111100001111111111111111110",
    "0111111111111001111aaaa11111100011111111000000111111111111111100",
    "01111111111110011111aaaa1111100011111110000000011111111111111000",
    "01111aaaaaaaa00111111aaaa111100011111100000000001111111111110000",
    "01111aaaaaaaa001111111aaaa11100011111000000000000111111111100000",
    "01111aaaaaaaa0011111111aaaa110001111aaaaaaaa00000011111111000000",
    "01111aaaaaaaa00111111111aaaa1000111aaaaaaaaa00000001111110000000",
    "01111aaaaaaaa001111111111aaaa00011aaaaaaaaaa00000000111100000000",
    "01111aaaaaaaa0011111111111aaa0001aaaaaaaaaaa00000000011000000000",
    "01111aaaaaaaa00111111111111aa000aaaaaaaaaaaa00000000000000000000",
    "01111aaaaaaaa001111111111111a000aaaaaaaaaaaa00000000000000000000",
    "01111aaaaaaaa0011111111111111000aaaaaaaaaaaa00000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
];

/// The diagonal fixture, built from [`DIAGONAL_FIXTURE_ROWS`].
pub fn diagonal_fixture() -> GameMap {
    let mut map = GameMap::new(DIAGONAL_FIXTURE_WIDTH, DIAGONAL_FIXTURE_HEIGHT, ground(0))
        .expect("the diagonal fixture's size is valid");
    for (y, row) in (0_u32..).zip(DIAGONAL_FIXTURE_ROWS) {
        assert_eq!(
            row.len(),
            DIAGONAL_FIXTURE_WIDTH as usize,
            "row {y} has the map's width"
        );
        for (x, symbol) in (0_u32..).zip(row.chars()) {
            let cell = match symbol {
                'a' => abyss(),
                's' => ramp_cell(0, RampDir::South),
                digit => ground(
                    digit
                        .to_digit(10)
                        .and_then(|d| u8::try_from(d).ok())
                        .expect("a plateau level digit"),
                ),
            };
            *map.cell_mut(x, y).expect("the row lies on the map") = cell;
        }
    }
    map
}

/// A buildable, pathable plateau cell at `level`.
pub fn ground(level: u8) -> MapCell {
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
        occupying_resource: None,
    }
}

/// A bottomless abyss cell.
pub fn abyss() -> MapCell {
    MapCell {
        terrain_kind: TerrainKind::Abyss,
        cliff_level: CliffLevel(0),
        flags: CellFlags::UNPASSABLE_CLIFF,
        ramp: None,
        occupying_structure: None,
        occupying_resource: None,
    }
}

/// A pathable ramp cell of a run whose low level is `low`, ascending towards `dir`.
pub fn ramp_cell(low: u8, dir: RampDir) -> MapCell {
    MapCell {
        terrain_kind: TerrainKind::Dirt,
        cliff_level: CliffLevel(low),
        flags: CellFlags::GROUND_PATHABLE.union(CellFlags::RAMP),
        ramp: Some(dir),
        occupying_structure: None,
        occupying_resource: None,
    }
}

// -----------------------------------------------------------------------------
// The rule, independently (drawing plan D-1)
// -----------------------------------------------------------------------------

/// What the rule reads of a cell: its plateau level, -1 for the abyss or off the map, or `None`
/// for a ramp cell. Read from the cell's fields directly.
fn analysis_level(map: &GameMap, x: i64, y: i64) -> Option<i32> {
    let (Ok(ux), Ok(uy)) = (u32::try_from(x), u32::try_from(y)) else {
        return Some(-1);
    };
    let Ok(cell) = map.cell(ux, uy) else {
        return Some(-1);
    };
    if cell.ramp.is_some() {
        None
    } else if cell.terrain_kind == TerrainKind::Abyss {
        Some(-1)
    } else {
        Some(i32::from(cell.cliff_level.0))
    }
}

/// The full cut of cell `(x, y)` by the drawing plan's D-1, stated from the plan's text: `X` is
/// high and both neighbours towards the corner are lower (convex); the three cells behind the
/// corner's opposite are high (backed); at least one of the corner's two neighbouring corners is
/// a concave corner with ground below; no cell of the 3x3 block is a ramp. The cut holds over the
/// thresholds `max(L1, L2) < t <= min(X, backing, highest concave reach)`; the first direction of
/// `(1, 1), (1, -1), (-1, 1), (-1, -1)` that holds is the cell's.
pub fn independent_cut_of(map: &GameMap, x: u32, y: u32) -> Option<DiagonalCut> {
    let level = |a: i64, b: i64| analysis_level(map, i64::from(x) + a, i64::from(y) + b);
    for b in -1..=1 {
        for a in -1..=1 {
            level(a, b)?;
        }
    }
    let at = |a: i64, b: i64| level(a, b).unwrap_or(-1);
    let la = at(0, 0);
    if la < 0 {
        return None;
    }
    for (dx, dy) in [(1_i64, 1_i64), (1, -1), (-1, 1), (-1, -1)] {
        let (l1, l2) = (at(dx, 0), at(0, dy));
        let mut reach: Option<i32> = None;
        if l1 >= 0 {
            reach = reach.max(Some(at(dx, -dy)));
        }
        if l2 >= 0 {
            reach = reach.max(Some(at(-dx, dy)));
        }
        let Some(reach) = reach else {
            continue;
        };
        let low_bound = l1.max(l2) + 1;
        let high_bound = la
            .min(at(-dx, 0))
            .min(at(0, -dy))
            .min(at(-dx, -dy))
            .min(reach);
        if low_bound <= high_bound {
            let small = |v: i64| i8::try_from(v).expect("a direction component");
            let level8 = |v: i32| i8::try_from(v).expect("a marching level");
            return Some(DiagonalCut {
                cell: (x, y),
                dir: (small(dx), small(dy)),
                la: level8(la),
                lb: level8(low_bound - 1),
                t_hi: level8(high_bound),
            });
        }
    }
    None
}

/// Every full cut of `map` by [`independent_cut_of`], row by row.
pub fn independent_cuts(map: &GameMap) -> Vec<DiagonalCut> {
    let mut cuts = Vec::new();
    for y in 0..map.height() {
        for x in 0..map.width() {
            cuts.extend(independent_cut_of(map, x, y));
        }
    }
    cuts
}

// -----------------------------------------------------------------------------
// Geometry of a cut
// -----------------------------------------------------------------------------

/// A world point from raw Q32.32 coordinates.
pub fn raw_point(x: i64, y: i64) -> SimVec2 {
    SimVec2::new(SimScalar::from_raw(x), SimScalar::from_raw(y))
}

/// The foot's constant `k` (grid cells): the foot is `dx·x + dy·y = k`. Stated from the plan's
/// notation, `k = dx·Cx + dy·Cy − 1` with `C` the cut cell's corner on the low side.
pub fn foot_k(cut: &DiagonalCut) -> i64 {
    let (dx, dy) = (i64::from(cut.dir.0), i64::from(cut.dir.1));
    let cx = i64::from(cut.cell.0) + i64::from(cut.dir.0 > 0);
    let cy = i64::from(cut.cell.1) + i64::from(cut.dir.1 > 0);
    dx * cx + dy * cy - 1
}

/// `dx·x + dy·y − k` of point `p`, in raw units (2⁻³² world units): positive beyond the foot,
/// zero on it. Its magnitude over `√2` is the distance from the foot.
pub fn foot_side_raw(cut: &DiagonalCut, p: SimVec2) -> i128 {
    i128::from(cut.dir.0) * i128::from(p.x.raw()) + i128::from(cut.dir.1) * i128::from(p.y.raw())
        - (i128::from(foot_k(cut)) << 32)
}

/// Whether `p` lies at least `distance` behind the foot (towards the plateau), exactly:
/// `k − (dx·x + dy·y) >= √2·distance`, compared on squares in raw units.
pub fn at_least_behind(cut: &DiagonalCut, p: SimVec2, distance: SimScalar) -> bool {
    let behind = -foot_side_raw(cut, p);
    let d = i128::from(distance.raw());
    behind >= 0 && behind * behind >= 2 * d * d
}

/// Whether `p` lies strictly less than `distance` behind the foot, or beyond it.
pub fn closer_than_behind(cut: &DiagonalCut, p: SimVec2, distance: SimScalar) -> bool {
    !at_least_behind(cut, p, distance)
}

/// Whether `p` lies in the cut cell's closed square.
pub fn in_cell(cut: &DiagonalCut, p: SimVec2) -> bool {
    let (x0, y0) = (i64::from(cut.cell.0) << 32, i64::from(cut.cell.1) << 32);
    let unit = 1_i64 << 32;
    (x0..=x0 + unit).contains(&p.x.raw()) && (y0..=y0 + unit).contains(&p.y.raw())
}

/// Whether `p` lies in the open outer triangle `T(X)`: inside the cut cell and beyond the foot.
pub fn in_outer_triangle(cut: &DiagonalCut, p: SimVec2) -> bool {
    in_cell(cut, p) && foot_side_raw(cut, p) > 0
}

/// The centre of cell `(x, y)`, world units.
pub fn cell_centre(x: u32, y: u32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

/// The cut cell's corner `C` on the low side, world units.
pub fn low_corner(cut: &DiagonalCut) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(cut.cell.0 + u32::from(cut.dir.0 > 0)),
        SimScalar::from_num(cut.cell.1 + u32::from(cut.dir.1 > 0)),
    )
}

// -------------------------------------------------------------------------------------------------
// Walls after the clip, for the reference distances
// -------------------------------------------------------------------------------------------------

/// A wall segment in world units.
pub type Segment = ((f64, f64), (f64, f64));

/// Whether cell `(x, y)` is on the map and its terrain is usable on its own.
pub fn usable(map: &GameMap, x: i64, y: i64) -> bool {
    let (Ok(ux), Ok(uy)) = (u32::try_from(x), u32::try_from(y)) else {
        return false;
    };
    ux < map.width() && uy < map.height() && map.terrain_step_is_connected((ux, uy), (ux, uy))
}

/// Whether the cell edge between `a` and its 4-neighbour `b` is a wall by `main`'s cell rule: one
/// side unusable or off the map, or the two not connected.
pub fn edge_is_wall(map: &GameMap, a: (i64, i64), b: (i64, i64)) -> bool {
    match (usable(map, a.0, a.1), usable(map, b.0, b.1)) {
        (true, true) => {
            !map.terrain_step_is_connected((a.0 as u32, a.1 as u32), (b.0 as u32, b.1 as u32))
        }
        (false, false) => false,
        _ => true,
    }
}

/// Every wall within `reach` cells of cell `(x, y)` after the clip: the cell edges of `main`'s
/// rule and the foot `P₂P₁` of every cut (from the independent rule).
pub fn walls_after_clip(map: &GameMap, x: u32, y: u32, reach: i64) -> Vec<Segment> {
    let mut walls = Vec::new();
    let (cx, cy) = (i64::from(x), i64::from(y));
    for j in cy - reach..=cy + reach {
        for i in cx - reach..=cx + reach {
            let (fi, fj) = (i as f64, j as f64);
            if edge_is_wall(map, (i, j), (i + 1, j)) {
                walls.push(((fi + 1.0, fj), (fi + 1.0, fj + 1.0)));
            }
            if edge_is_wall(map, (i, j), (i, j + 1)) {
                walls.push(((fi, fj + 1.0), (fi + 1.0, fj + 1.0)));
            }
            if let (Ok(ui), Ok(uj)) = (u32::try_from(i), u32::try_from(j)) {
                if let Some(cut) = independent_cut_of(map, ui, uj) {
                    let corner = (
                        f64::from(cut.cell.0) + f64::from(u8::from(cut.dir.0 > 0)),
                        f64::from(cut.cell.1) + f64::from(u8::from(cut.dir.1 > 0)),
                    );
                    let p2 = (corner.0 - f64::from(cut.dir.0), corner.1);
                    let p1 = (corner.0, corner.1 - f64::from(cut.dir.1));
                    walls.push((p2, p1));
                }
            }
        }
    }
    walls
}

/// Distance from `p` to segment `s`.
pub fn segment_distance(p: (f64, f64), (a, b): Segment) -> f64 {
    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
    let s = (((p.0 - a.0) * ex + (p.1 - a.1) * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
    let (qx, qy) = (a.0 + s * ex - p.0, a.1 + s * ey - p.1);
    (qx * qx + qy * qy).sqrt()
}

/// World coordinates of `p` in `f64` (exact for points on the `1/64` lattice).
pub fn to_f64(p: SimVec2) -> (f64, f64) {
    (
        p.x.raw() as f64 / 4_294_967_296.0,
        p.y.raw() as f64 / 4_294_967_296.0,
    )
}
