//! The full cut of a 45° staircase's convex corner: which cells the straight diagonal cliff cuts,
//! and along which line (plan `docs/plans/2026-12-02-diagonal-cliff-walkability.md` D-1; the
//! predicate of plan `docs/plans/2026-12-01-straight-diagonal-cliffs.md` D-1, owner questions
//! Q0136 (c) and Q0139).
//!
//! # Responsibility
//! A 45° staircase cliff alternates convex corners `C` and concave corners `V`. The straight line
//! the drawing puts along it passes through the `V`s: the anti-diagonal of every high step cell
//! `X`, its **foot** `dx·x + dy·y = k`. This module owns the one rule that decides which cells are
//! cut that way ([`DiagonalCut::of_block`]), the classification of cells it reads
//! ([`marching_level`]), the map-level query ([`GameMap::diagonal_cut`]) and the exact side test
//! against the foot ([`DiagonalCut::beyond`]).
//!
//! # How it fits
//! The rule is terrain: a pure function of nine cells' levels, abyss and ramp flags. The client's
//! terrain mesher draws the chamfer from it (`client::terrain::mesher::diagonal` builds its `Cut`
//! from a [`DiagonalCut`]), so the drawing reads the simulation's rule and the two cannot drift
//! apart (charter §5: `client` depends on `sim_map`, never the reverse). The simulation's
//! walkability clip of the triangle beyond the foot (plan D-2 to D-6) reads the same rule.
//!
//! # Invariants
//! - **Pure:** a cut is a function of the 3x3 block of cells around `X` alone, evaluated on
//!   demand and never cached, so editing a cell can never leave a stale cut behind.
//! - **At most one cut per cell:** the directions are tried in one fixed order and the first that
//!   holds is the cell's cut.
//! - **No ramp in the block:** a block holding a ramp cell has no cut.
//! - **Exact:** levels are small integers and the side test is an integer comparison on raw
//!   Q32.32 coordinates in `i128`; nothing here uses a float (charter §3.1).

use sim_types::SimVec2;

use crate::grid::{GameMap, TerrainKind};

/// The four corner directions `(dx, dy)` a cut can take, each component `±1`, in the order the
/// predicate tries them.
const DIRECTIONS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The marching levels of a 3x3 block of cells, `block[b + 1][a + 1]` for the cell at offset
/// `(a, b)` from the block's centre: a plateau level, `-1` for the void, `None` for a ramp cell.
pub type BlockLevels = [[Option<i8>; 3]; 3];

/// The full cut of cell `cell` at its corner in direction `dir`, over the thresholds
/// `lb + 1 ..= t_hi` (plan `2026-12-01-straight-diagonal-cliffs` D-1).
///
/// Levels are marching levels: a plateau's cliff level, `-1` for the void.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiagonalCut {
    /// The cut cell `X`, in grid cells.
    pub cell: (u32, u32),
    /// The corner direction `(dx, dy)`, each `±1`: from `X`'s centre towards its corner `C` on
    /// the low side.
    pub dir: (i8, i8),
    /// `X`'s marching level.
    pub la: i8,
    /// The higher of the levels of `X`'s two neighbours towards `C`, `L₁ = X + (dx, 0)` and
    /// `L₂ = X + (0, dy)`: the foot stands at that level's height.
    pub lb: i8,
    /// The highest threshold at which the cut holds.
    pub t_hi: i8,
}

impl DiagonalCut {
    /// The cut of the cell at the centre of the 3x3 block `block`, if the rule holds at some
    /// threshold: `X` is convex at the corner `C` (both neighbours towards it lower), the three
    /// cells behind the opposite corner are high (backed), at least one of `C`'s two neighbouring
    /// corners is a concave corner whose low cell is ground, and no cell of the block is a ramp.
    ///
    /// `cell` is the block centre's grid cell, carried into the result.
    #[must_use]
    pub fn of_block(cell: (u32, u32), block: &BlockLevels) -> Option<Self> {
        let mut levels = [[0_i8; 3]; 3];
        for (row, source) in levels.iter_mut().zip(block) {
            for (slot, level) in row.iter_mut().zip(source) {
                *slot = (*level)?;
            }
        }
        let at = |a: i8, b: i8| {
            levels[usize::from((b + 1).unsigned_abs())][usize::from((a + 1).unsigned_abs())]
        };
        let la = at(0, 0);
        if la < 0 {
            return None;
        }
        DIRECTIONS.into_iter().find_map(|(dx, dy)| {
            let (l1, l2) = (at(dx, 0), at(0, dy));
            // Each neighbouring corner of C is a concave corner while its two high cells are
            // high; it counts only where its low cell is ground.
            let concave = [
                (l1 >= 0).then(|| at(dx, -dy)),
                (l2 >= 0).then(|| at(-dx, dy)),
            ];
            let reach = concave.into_iter().flatten().max()?;
            let backed = at(-dx, 0).min(at(0, -dy)).min(at(-dx, -dy));
            let t_hi = la.min(backed).min(reach);
            let lb = l1.max(l2);
            (lb < t_hi).then_some(Self {
                cell,
                dir: (dx, dy),
                la,
                lb,
                t_hi,
            })
        })
    }

    /// Whether the cut holds at threshold `t` (a marching level).
    #[must_use]
    pub fn active(&self, t: i8) -> bool {
        self.lb < t && t <= self.t_hi
    }

    /// The foot's integer constant `k`, in grid cells: the foot is the line `dx·x + dy·y = k`
    /// through `X`'s corners `P₁ = C − (0, dy)` and `P₂ = C − (dx, 0)`, where
    /// `C = X + ((dx > 0), (dy > 0))` is the corner on the low side.
    #[must_use]
    pub fn foot_constant(&self) -> i64 {
        let (dx, dy) = (i64::from(self.dir.0), i64::from(self.dir.1));
        let corner = (
            i64::from(self.cell.0) + i64::from(self.dir.0 > 0),
            i64::from(self.cell.1) + i64::from(self.dir.1 > 0),
        );
        dx * corner.0 + dy * corner.1 - 1
    }

    /// Whether world point `p` (world units, one per cell) lies in the open half-plane beyond the
    /// foot, towards `C`: `dx·x + dy·y > k`. A point on the foot is not beyond it.
    ///
    /// Exact: evaluated as `dx·x_raw + dy·y_raw > k·2³²` on the raw Q32.32 coordinates in `i128`
    /// (each raw value is below `2⁶³` in magnitude, so the sum is below `2⁶⁵`).
    #[must_use]
    pub fn beyond(&self, p: SimVec2) -> bool {
        let (dx, dy) = (i128::from(self.dir.0), i128::from(self.dir.1));
        let side = dx * i128::from(p.x.raw()) + dy * i128::from(p.y.raw());
        side > i128::from(self.foot_constant()) << 32
    }
}

/// The marching level of cell `(x, y)` for the cut rule: the cliff level of a plateau, `-1` for
/// the void (the abyss, or any cell off the map), `None` for a ramp cell. Grid cells; `x` and `y`
/// may lie off the map, which is the void.
///
/// Cliff levels are validated to `0..=3` (`CliffLevel::MAX`); a level above `i8::MAX`, which no
/// valid map holds, reads as `i8::MAX`.
// PERFORMANCE RATIONALE:
// Why: The mesher reads 16 cells through this function per dual cell on each chunk rebuild, and
// the drawing's full classification (`client::terrain::mesher::surface_class`) builds a ramp
// cell's profile, a walk along its run, that the rule never reads.
// What: The cell's own fields decide the class: abyss first, then the ramp flag (a cell has a ramp
// profile exactly when it has a ramp direction), then the cliff level.
// Trade-off: A second copy of `surface_class`'s three-way rule, kept beside the client's and
// checked against it by the client unit test `cut_level_classifies_as_surface_class`.
#[must_use]
pub fn marching_level(map: &GameMap, x: i64, y: i64) -> Option<i8> {
    let (Ok(ux), Ok(uy)) = (u32::try_from(x), u32::try_from(y)) else {
        return Some(-1);
    };
    match map.cell(ux, uy) {
        Err(_) => Some(-1),
        Ok(cell) if cell.terrain_kind == TerrainKind::Abyss => Some(-1),
        Ok(cell) if cell.ramp.is_some() => None,
        Ok(cell) => Some(i8::try_from(cell.cliff_level.0).unwrap_or(i8::MAX)),
    }
}

impl GameMap {
    /// The full cut of cell `(x, y)` (grid cells), if it has one (plan
    /// `2026-12-02-diagonal-cliff-walkability` D-1): [`DiagonalCut::of_block`] on the 3x3 block
    /// around it, read through [`marching_level`]. `None` for a cell off the map.
    // PERFORMANCE RATIONALE:
    // Why: The walkability rules ask this of cells on flat ground almost always, where no cut
    // can hold, and the full rule reads nine cells.
    // What: A cut needs `X` to be a plateau with both neighbours towards its corner lower and no
    // ramp among them, so the cell and its four 4-neighbours are read first (five reads) and the
    // 3x3 block is read only when some pair of perpendicular neighbours is lower.
    // Trade-off: The pre-filter is a second statement of part of the rule; it only rejects blocks
    // `of_block` rejects too, which the unit test `the_early_out_never_drops_a_cut` checks on
    // every block of the fixture shapes.
    #[must_use]
    pub fn diagonal_cut(&self, x: u32, y: u32) -> Option<DiagonalCut> {
        if x >= self.width() || y >= self.height() {
            return None;
        }
        let (cx, cy) = (i64::from(x), i64::from(y));
        let level = |a: i64, b: i64| marching_level(self, cx + a, cy + b);
        let la = level(0, 0).filter(|&l| l >= 0)?;
        let east = level(1, 0)?;
        let west = level(-1, 0)?;
        let north = level(0, 1)?;
        let south = level(0, -1)?;
        let low_x = east < la || west < la;
        let low_y = north < la || south < la;
        if !(low_x && low_y) {
            return None;
        }
        let mut block: BlockLevels = [[None; 3]; 3];
        for (b, row) in (-1..=1).zip(block.iter_mut()) {
            for (a, slot) in (-1..=1).zip(row.iter_mut()) {
                *slot = level(a, b);
            }
        }
        DiagonalCut::of_block((x, y), &block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{CellFlags, CliffLevel, MapCell, RampDir};
    use sim_types::SimScalar;

    /// A 3x3 map whose cells have the marching levels `levels` (rows bottom to top, `-1` the
    /// abyss), with a ramp at `ramp` if given.
    #[expect(clippy::expect_used, reason = "a failed fixture is a failed test")]
    fn block_map(levels: [[i8; 3]; 3], ramp: Option<(u32, u32)>) -> GameMap {
        let mut map = GameMap::new(3, 3, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: 3x3 is a valid map size");
        for (j, row) in (0_u32..).zip(levels) {
            for (i, level) in (0_u32..).zip(row) {
                let cell = map
                    .cell_mut(i, j)
                    .expect("invariant: the cell lies on the 3x3 map");
                match u8::try_from(level) {
                    Ok(level) => cell.cliff_level = CliffLevel(level),
                    Err(_) => cell.terrain_kind = TerrainKind::Abyss,
                }
            }
        }
        if let Some((x, y)) = ramp {
            let cell = map
                .cell_mut(x, y)
                .expect("invariant: the ramp lies on the 3x3 map");
            cell.cliff_level = CliffLevel(0);
            cell.ramp = Some(RampDir::North);
            cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
        }
        map
    }

    /// A staircase step for direction `(1, 1)`, rows `j = 0..3` bottom to top: `X = (1, 1)` at 1,
    /// its low neighbours `L1 = (2, 1)` and `L2 = (1, 2)` and the diagonal `K = (2, 2)` at 0,
    /// everything else at 1, so both neighbouring corners are ground concave corners.
    const RUN_CELL: [[i8; 3]; 3] = [[1, 1, 1], [1, 1, 0], [1, 0, 0]];

    /// Whether cell `(1, 1)` of `map` is cut in direction `dir` at threshold `t`.
    fn cut_at(map: &GameMap, dir: (i8, i8), t: i8) -> bool {
        map.diagonal_cut(1, 1)
            .is_some_and(|cut| cut.dir == dir && cut.active(t))
    }

    /// One case of T-D1: why, the block, the direction, and the thresholds at which it is cut.
    type BlockCase = (&'static str, [[i8; 3]; 3], (i8, i8), &'static [i8]);

    /// T-D1 (plan `2026-12-02-diagonal-cliff-walkability` §6.1, the drawing plan's T-S2 table
    /// moved with the predicate): the full cut follows its 3x3 block.
    #[test]
    fn the_cut_follows_its_block() {
        let mut run_end = RUN_CELL;
        run_end[2][0] = 0;
        let mut rectangle = RUN_CELL;
        rectangle[2][0] = 0;
        rectangle[0][2] = 0;
        let void_only = [[1, 1, 1], [1, 1, -1], [1, -1, -1]];
        let mut open_behind = RUN_CELL;
        open_behind[0][0] = 0;
        let mut saddle = RUN_CELL;
        saddle[2][2] = 1;
        let stacked = [[2, 2, 2], [2, 2, 0], [2, 1, 0]];
        let low_backing = [[2, 2, 2], [2, 3, 0], [3, 0, 0]];
        let cases: [BlockCase; 10] = [
            (
                "a run cell: two ground concave corners",
                RUN_CELL,
                (1, 1),
                &[1],
            ),
            (
                "a run's end at a straight wall: one concave corner",
                run_end,
                (1, 1),
                &[1],
            ),
            (
                "a rectangle corner: no concave corner beside it",
                rectangle,
                (1, 1),
                &[],
            ),
            (
                "a void-only run: no concave corner has ground below",
                void_only,
                (1, 1),
                &[],
            ),
            (
                "the corner opposite C is not interior (a ridge)",
                open_behind,
                (1, 1),
                &[],
            ),
            ("a saddle: K is high", saddle, (1, 1), &[1]),
            ("the run's direction only", RUN_CELL, (-1, -1), &[]),
            ("the run's direction only", RUN_CELL, (1, -1), &[]),
            (
                "stacked: the cut run is max(L1, L2) < t <= level(X)",
                stacked,
                (1, 1),
                &[2],
            ),
            ("the backing caps the cut run", low_backing, (1, 1), &[1, 2]),
        ];
        let mut failures = Vec::new();
        for (why, levels, dir, holds) in cases {
            let map = block_map(levels, None);
            for t in 0..=4 {
                let expected = holds.contains(&t);
                if cut_at(&map, dir, t) != expected {
                    failures.push(format!("{why}: dir {dir:?} t {t}: expected {expected}"));
                }
            }
        }
        for y in 0..3 {
            for x in 0..3 {
                if block_map(RUN_CELL, Some((x, y)))
                    .diagonal_cut(1, 1)
                    .is_some()
                {
                    failures.push(format!(
                        "a ramp at ({x}, {y}) of the block: expected no cut"
                    ));
                }
            }
        }
        // The control: the same block without a ramp is cut, so the ramp cases test the ramp.
        if !cut_at(&block_map(RUN_CELL, None), (1, 1), 1) {
            failures.push("the run cell without a ramp: expected a cut".to_owned());
        }
        // The cut run's levels.
        let expected = DiagonalCut {
            cell: (1, 1),
            dir: (1, 1),
            la: 2,
            lb: 1,
            t_hi: 2,
        };
        if block_map(stacked, None).diagonal_cut(1, 1) != Some(expected) {
            failures.push(format!("stacked: expected {expected:?}"));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// T-D1, the side test: a point on the foot is not beyond it, one raw unit towards `C` is, one
    /// raw unit back is not, in all four directions.
    #[test]
    fn beyond_is_the_open_half_plane_past_the_foot() {
        for dir in DIRECTIONS {
            let cut = DiagonalCut {
                cell: (5, 7),
                dir,
                la: 1,
                lb: 0,
                t_hi: 1,
            };
            // The cell's centre lies on its foot.
            let centre = SimVec2::new(
                SimScalar::from_num(5) + SimScalar::HALF,
                SimScalar::from_num(7) + SimScalar::HALF,
            );
            let nudge = |steps: i64| {
                SimVec2::new(
                    SimScalar::from_raw(centre.x.raw() + steps * i64::from(dir.0)),
                    centre.y,
                )
            };
            assert!(!cut.beyond(centre), "{dir:?}: the foot itself");
            assert!(cut.beyond(nudge(1)), "{dir:?}: one raw unit beyond");
            assert!(!cut.beyond(nudge(-1)), "{dir:?}: one raw unit behind");
            // The corner C is beyond, the opposite corner O behind.
            let (cx, cy) = (5 + i32::from(dir.0 > 0), 7 + i32::from(dir.1 > 0));
            let corner = SimVec2::new(SimScalar::from_num(cx), SimScalar::from_num(cy));
            let opposite = SimVec2::new(
                SimScalar::from_num(cx - i32::from(dir.0)),
                SimScalar::from_num(cy - i32::from(dir.1)),
            );
            assert!(cut.beyond(corner), "{dir:?}: the corner C");
            assert!(!cut.beyond(opposite), "{dir:?}: the opposite corner O");
        }
    }

    /// The early-out of `GameMap::diagonal_cut` drops no cut: on every block of the T-D1 shapes,
    /// in all four rotations and with each cell in turn lowered to each level, it agrees with
    /// `of_block` read directly.
    #[test]
    fn the_early_out_never_drops_a_cut() {
        let shapes: [[[i8; 3]; 3]; 3] = [
            RUN_CELL,
            [[2, 2, 2], [2, 2, 0], [2, 1, 0]],
            [[1, 1, 1], [1, 1, -1], [1, -1, -1]],
        ];
        let mut checked = 0_u32;
        for shape in shapes {
            let mut levels = shape;
            for _ in 0..4 {
                levels = rotate(levels);
                for j in 0..3 {
                    for i in 0..3 {
                        for lowered in -1..=3 {
                            let mut variant = levels;
                            variant[j][i] = lowered;
                            let map = block_map(variant, None);
                            let block = variant.map(|row| row.map(Some));
                            assert_eq!(
                                map.diagonal_cut(1, 1),
                                DiagonalCut::of_block((1, 1), &block),
                                "block {variant:?}"
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert!(checked > 0);
    }

    /// `levels` turned a quarter turn counter-clockwise.
    fn rotate(levels: [[i8; 3]; 3]) -> [[i8; 3]; 3] {
        let mut out = [[0; 3]; 3];
        for (j, row) in out.iter_mut().enumerate() {
            for (i, slot) in row.iter_mut().enumerate() {
                *slot = levels[i][2 - j];
            }
        }
        out
    }

    /// `marching_level` reads off-map cells and the abyss as the void and ramps as `None`.
    #[test]
    fn marching_level_classifies_cells() {
        let map = block_map([[0, 1, -1], [2, 3, 0], [0, 0, 0]], Some((2, 2)));
        assert_eq!(marching_level(&map, -1, 0), Some(-1));
        assert_eq!(marching_level(&map, 3, 0), Some(-1));
        assert_eq!(marching_level(&map, 2, 0), Some(-1));
        assert_eq!(marching_level(&map, 1, 1), Some(3));
        assert_eq!(marching_level(&map, 2, 2), None);
        assert_eq!(map.diagonal_cut(3, 3), None);
    }
}
