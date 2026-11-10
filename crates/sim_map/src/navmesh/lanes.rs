//! Terrain lanes: obstacle cuts and the homotopy words of routes (plan
//! `docs/plans/2026-11-10-group-routing.md`, D-2 and D-5; Bhattacharya, Kumar and Likhachev,
//! AAAI 2010).
//!
//! # Responsibility
//! Owns the lane description of a map's terrain. Every terrain obstacle (a connected component of
//! walls and unusable cells that does not touch the map border) gets one cut: a vertical segment
//! hanging from its lowest, then leftmost, lattice vertex `ζ` down to the first wall below. A
//! polyline's [`LaneWord`] is the freely reduced sequence of the cuts it crosses, `+(h + 1)` left
//! to right and `-(h + 1)` right to left for obstacle `h`. Two routes with the same endpoints are
//! in the same lane (the same homotopy class among the terrain obstacles) exactly when their
//! words are equal.
//!
//! # How it fits
//! [`crate::Pathfinder`] builds [`LaneCuts`] once from the terrain and keeps them across structure
//! rebuilds: structures are not obstacles for lanes, so the cuts are a pure function of the
//! terrain. `Pathfinder::lane_word` reads them; nothing in the simulation calls it yet (WP1 is
//! latent).
//!
//! # Invariants
//! - **Symbolic perturbation.** A cut at lattice column `x` lies on the line `x + ε`, so a point
//!   with `x_raw ≤ x · 2^32` is on its left; no cut overlaps a vertical wall or a vertical leg,
//!   and no cut passes through a lattice vertex. Cuts sharing a column are perturbed further in
//!   the order of their tops: a taller cut passes to the right of the obstacle a shorter one
//!   hangs from, so it lies further right. A cut's span is half-open, `(bottom, top]`.
//! - **Termination.** `ζ` is its obstacle's lowest vertex, so a cut ends on an obstacle whose
//!   own `ζ` is strictly lower, or on the border. The cuts form a forest rooted at the border.
//! - **Exactness.** Every crossing is decided in `i128` on raw Q32.32 coordinates; cut
//!   coordinates are lattice integers. No float, no hashed collection, `Vec`s in index order.

use sim_types::SimVec2;

use super::cdt::NONE;
use super::{EdgeGrid, EdgeKind, NavMesh};
use crate::grid::GameMap;

/// Fraction bits of a raw [`sim_types::SimScalar`].
const RAW_FRACTION_BITS: u32 = 32;

/// Raw coordinates are clamped to this magnitude before the exact crossing arithmetic, so that
/// every product fits `i128` (differences below 2^61, products below 2^122). It is 2^28 world
/// units, far beyond any map; a point that far away has no meaningful lane.
const RAW_COORDINATE_LIMIT: i64 = 1 << 60;

/// One obstacle's cut: the vertical segment `x = x + ε`, `bottom < y ≤ top`, in whole cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneCut {
    /// Index `h` of the obstacle; crossings of this cut are written `±(h + 1)`.
    pub obstacle: u32,
    /// Lattice column of the obstacle's representative vertex `ζ`, in cells from the west edge.
    pub x: i32,
    /// Row of the first wall below `ζ`, in cells from the south edge (exclusive end of the cut).
    pub bottom: i32,
    /// Row of `ζ`, the obstacle's lowest lattice vertex (inclusive end of the cut).
    pub top: i32,
    /// The obstacle the cut ends on, or `None` when it ends on the map border's component.
    pub below: Option<u32>,
}

/// A freely reduced lane word: the cut crossings of a polyline in order, `+(h + 1)` for a
/// left-to-right crossing of obstacle `h`'s cut and `-(h + 1)` for right to left, with every
/// adjacent pair `+a, -a` cancelled.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LaneWord {
    letters: Vec<i32>,
}

impl LaneWord {
    /// The empty word: a route that crosses no cut, or crosses back over every cut it crossed.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            letters: Vec::new(),
        }
    }

    /// Reduces `letters` freely into a word. A zero letter names no cut and is dropped.
    #[must_use]
    pub fn from_letters(letters: &[i32]) -> Self {
        let mut word = Self::empty();
        for &letter in letters {
            word.push(letter);
        }
        word
    }

    /// The letters of the reduced word, in crossing order.
    #[must_use]
    pub fn letters(&self) -> &[i32] {
        &self.letters
    }

    /// Number of letters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.letters.len()
    }

    /// Whether the word is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.letters.is_empty()
    }

    /// Appends one crossing, cancelling it against the last letter when they are inverse.
    pub fn push(&mut self, letter: i32) {
        if letter == 0 {
            return;
        }
        if self.letters.last() == Some(&-letter) {
            self.letters.pop();
        } else {
            self.letters.push(letter);
        }
    }

    /// Appends every letter of `other` in order, reducing as it goes.
    pub fn extend(&mut self, other: &Self) {
        for &letter in &other.letters {
            self.push(letter);
        }
    }
}

/// The cuts of a map's terrain obstacles (D-2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaneCuts {
    /// Sorted by `(x, top)`: the left-to-right order of the perturbed cut lines.
    cuts: Vec<LaneCut>,
}

/// Union-find over lattice points, with path halving; deterministic for a fixed union order.
struct Components {
    parent: Vec<u32>,
}

impl Components {
    fn new(points: usize) -> Self {
        Self {
            parent: (0..points)
                .map(|p| u32::try_from(p).unwrap_or(NONE))
                .collect(),
        }
    }

    fn find(&mut self, mut p: usize) -> usize {
        while self.parent[p] as usize != p {
            let grand = self.parent[self.parent[p] as usize];
            self.parent[p] = grand;
            p = grand as usize;
        }
        p
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            let (low, high) = (ra.min(rb), ra.max(rb));
            self.parent[high] = u32::try_from(low).unwrap_or(NONE);
        }
    }
}

fn to_i32(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

impl LaneCuts {
    /// Builds the cuts of `map`'s terrain. Structures do not count: the walls and unusable
    /// cells read here do not depend on them, so the result is the same before and after any
    /// structure is placed or removed.
    #[must_use]
    pub fn build(map: &GameMap) -> Self {
        let edges = EdgeGrid::new(map);
        let (w, h) = (edges.width, edges.height);
        let point = |x: usize, y: usize| y * (w + 1) + x;
        let mut components = Components::new((w + 1) * (h + 1));
        let mut on_obstacle = vec![false; (w + 1) * (h + 1)];
        let is_wall = |kind: Option<EdgeKind>| kind == Some(EdgeKind::Wall);
        for y in 0..=h {
            for x in 0..=w {
                let mut link = |a: usize, b: usize| {
                    components.union(a, b);
                    on_obstacle[a] = true;
                    on_obstacle[b] = true;
                };
                if is_wall(edges.east(x, y)) {
                    link(point(x, y), point(x + 1, y));
                }
                if is_wall(edges.north(x, y)) {
                    link(point(x, y), point(x, y + 1));
                }
                // An unusable cell joins its four corners: its interior is part of the obstacle.
                if x < w && y < h && !super::classify_cell(map, x, y).usable {
                    link(point(x, y), point(x + 1, y));
                    link(point(x, y), point(x, y + 1));
                    link(point(x, y + 1), point(x + 1, y + 1));
                }
            }
        }
        let outside = components.find(point(0, 0));
        // Row-major order visits each component first at its lowest, then leftmost, point.
        let mut obstacle_of_root = vec![NONE; (w + 1) * (h + 1)];
        let mut representatives = Vec::new();
        for y in 0..=h {
            for x in 0..=w {
                let p = point(x, y);
                if !on_obstacle[p] {
                    continue;
                }
                let root = components.find(p);
                if root != outside && obstacle_of_root[root] == NONE {
                    obstacle_of_root[root] = u32::try_from(representatives.len()).unwrap_or(NONE);
                    representatives.push((x, y));
                }
            }
        }
        let mut cuts = Vec::with_capacity(representatives.len());
        for (index, &(x, top)) in representatives.iter().enumerate() {
            // The border row is all wall, so the scan always stops by `y = 0`.
            let bottom = (0..top)
                .rev()
                .find(|&y| is_wall(edges.east(x, y)))
                .unwrap_or(0);
            let below_root = components.find(point(x, bottom));
            cuts.push(LaneCut {
                obstacle: u32::try_from(index).unwrap_or(NONE),
                x: to_i32(x),
                bottom: to_i32(bottom),
                top: to_i32(top),
                below: (below_root != outside).then(|| obstacle_of_root[below_root]),
            });
        }
        cuts.sort_unstable_by_key(|cut| (cut.x, cut.top));
        Self { cuts }
    }

    /// Number of cuts, which is the number of terrain obstacles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cuts.len()
    }

    /// Whether the terrain has no obstacle: every route then has the empty word.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cuts.is_empty()
    }

    /// The cuts, sorted by `(x, top)`.
    #[must_use]
    pub fn cuts(&self) -> &[LaneCut] {
        &self.cuts
    }

    /// The cut of obstacle `h`, if there is one.
    #[must_use]
    pub fn cut_of(&self, obstacle: u32) -> Option<&LaneCut> {
        self.cuts.iter().find(|cut| cut.obstacle == obstacle)
    }

    /// Calls `emit` with the letter of every cut the straight leg `p -> q` crosses, in the
    /// order the leg crosses them.
    pub fn for_each_crossing(&self, p: SimVec2, q: SimVec2, mut emit: impl FnMut(i32)) {
        let (p, q) = (RawPoint::of(p), RawPoint::of(q));
        if p.x == q.x || self.cuts.is_empty() {
            return;
        }
        let (low, high) = (p.x.min(q.x), p.x.max(q.x));
        // A cut at column c lies between the leg's ends when low ≤ c·2^32 < high: the end with
        // the smaller x is on its left (≤) and the other on its right (>).
        let first = self.cuts.partition_point(|cut| cut.raw_x() < low);
        let last = self.cuts.partition_point(|cut| cut.raw_x() < high);
        let candidates = &self.cuts[first..last];
        let rightward = q.x > p.x;
        let mut visit = |cut: &LaneCut| {
            if crosses_span(p, q, cut) {
                let letter = to_i32(cut.obstacle as usize).saturating_add(1);
                emit(if rightward { letter } else { -letter });
            }
        };
        if rightward {
            candidates.iter().for_each(&mut visit);
        } else {
            candidates.iter().rev().for_each(&mut visit);
        }
    }

    /// The reduced word of the polyline through `points`.
    #[must_use]
    pub fn word(&self, points: &[SimVec2]) -> LaneWord {
        let mut word = LaneWord::empty();
        for pair in points.windows(2) {
            self.for_each_crossing(pair[0], pair[1], |letter| word.push(letter));
        }
        word
    }

    /// `K` of D-5: the largest number of cuts any single edge of `mesh` crosses. A sub-segment of
    /// an edge crosses at most this many, which bounds how far the restricted search's portal
    /// midpoint words may stray from a prefix of the target word.
    #[must_use]
    pub fn max_edge_crossings(&self, mesh: &NavMesh) -> u32 {
        if self.cuts.is_empty() {
            return 0;
        }
        let mut most = 0;
        for t in 0..mesh.triangle_count() {
            let t = u32::try_from(t).unwrap_or(NONE);
            let neighbours = mesh.triangle_neighbors(t);
            for (k, &n) in neighbours.iter().enumerate() {
                if n != NONE && n < t {
                    continue;
                }
                let (a, b) = (
                    mesh.corner(t, super::next(k)).to_sim(),
                    mesh.corner(t, super::prev(k)).to_sim(),
                );
                let mut count = 0;
                self.for_each_crossing(a, b, |_| count += 1);
                most = most.max(count);
            }
        }
        most
    }
}

impl LaneCut {
    fn raw_x(&self) -> i64 {
        i64::from(self.x) << RAW_FRACTION_BITS
    }
}

/// A point in raw Q32.32 units, clamped to [`RAW_COORDINATE_LIMIT`].
#[derive(Clone, Copy)]
struct RawPoint {
    x: i64,
    y: i64,
}

impl RawPoint {
    fn of(p: SimVec2) -> Self {
        let clamp = |v: i64| v.clamp(-RAW_COORDINATE_LIMIT, RAW_COORDINATE_LIMIT);
        Self {
            x: clamp(p.x.raw()),
            y: clamp(p.y.raw()),
        }
    }
}

/// Sign of `y(c + ε) − level` along the line through `p` and `q`, where `c` is the cut's column
/// in raw units and `level` a raw height; `p.x ≠ q.x`.
fn height_sign(p: RawPoint, q: RawPoint, column: i64, level: i64) -> i32 {
    let (dx, dy) = (i128::from(q.x - p.x), i128::from(q.y - p.y));
    // y(column) − level = numerator / dx.
    let numerator = i128::from(p.y - level) * dx + i128::from(column - p.x) * dy;
    let sign = if numerator == 0 {
        // Exactly at `level` on the column: the ε step moves along the slope, dy / dx.
        dy.signum() * dx.signum()
    } else {
        numerator.signum() * dx.signum()
    };
    // `sign` is -1, 0 or 1 by construction.
    i32::try_from(sign).unwrap_or(0)
}

/// Whether the leg `p -> q`, whose ends lie on opposite sides of `cut`, crosses it within its
/// half-open span `(bottom, top]`.
fn crosses_span(p: RawPoint, q: RawPoint, cut: &LaneCut) -> bool {
    let column = cut.raw_x();
    let level = |row: i32| i64::from(row) << RAW_FRACTION_BITS;
    height_sign(p, q, column, level(cut.bottom)) > 0
        && height_sign(p, q, column, level(cut.top)) <= 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{CellFlags, CliffLevel, MapCell};
    use sim_types::SimScalar;

    /// A point in quarter cells.
    fn point(x4: i32, y4: i32) -> SimVec2 {
        SimVec2::new(
            SimScalar::from_raw(i64::from(x4) << 30),
            SimScalar::from_raw(i64::from(y4) << 30),
        )
    }

    fn one_cut() -> LaneCuts {
        LaneCuts {
            cuts: vec![LaneCut {
                obstacle: 0,
                x: 5,
                bottom: 0,
                top: 4,
                below: None,
            }],
        }
    }

    const NONE_CROSSED: Vec<i32> = Vec::new();

    fn crossings(cuts: &LaneCuts, p: SimVec2, q: SimVec2) -> Vec<i32> {
        let mut out = Vec::new();
        cuts.for_each_crossing(p, q, |letter| out.push(letter));
        out
    }

    #[test]
    fn words_reduce_freely() {
        assert_eq!(LaneWord::from_letters(&[1, -1]), LaneWord::empty());
        assert_eq!(LaneWord::from_letters(&[1, 2, -2, 3]).letters(), &[1, 3]);
        assert_eq!(LaneWord::from_letters(&[1, 1, -1]).letters(), &[1]);
        assert_eq!(LaneWord::from_letters(&[0, 2, 0]).letters(), &[2]);
        let mut word = LaneWord::from_letters(&[2, 1]);
        word.extend(&LaneWord::from_letters(&[-1, -2, 3]));
        assert_eq!(word.letters(), &[3]);
    }

    #[test]
    fn a_leg_crosses_left_to_right_positive_and_back_negative() {
        let cuts = one_cut();
        assert_eq!(crossings(&cuts, point(16, 8), point(24, 8)), vec![1]);
        assert_eq!(crossings(&cuts, point(24, 8), point(16, 8)), vec![-1]);
        assert_eq!(crossings(&cuts, point(16, 24), point(24, 24)), NONE_CROSSED);
        assert_eq!(crossings(&cuts, point(4, 8), point(12, 8)), NONE_CROSSED);
    }

    #[test]
    fn a_point_on_the_cut_column_is_on_its_left() {
        let cuts = one_cut();
        // Ending exactly on x = 5 does not cross; continuing from there to the right does.
        assert_eq!(crossings(&cuts, point(16, 8), point(20, 8)), NONE_CROSSED);
        assert_eq!(crossings(&cuts, point(20, 8), point(24, 8)), vec![1]);
        // A vertical leg along the column never crosses.
        assert_eq!(crossings(&cuts, point(20, 2), point(20, 14)), NONE_CROSSED);
    }

    #[test]
    fn the_span_is_half_open_and_decided_by_the_slope_at_its_ends() {
        let cuts = one_cut();
        // Through the top vertex ζ = (5, 4): a horizontal leg is on the cut (top inclusive).
        assert_eq!(crossings(&cuts, point(16, 16), point(24, 16)), vec![1]);
        // Rising through ζ: just right of the column it is above the top, so no crossing.
        assert_eq!(crossings(&cuts, point(16, 12), point(24, 20)), NONE_CROSSED);
        // Falling through ζ: just right of the column it is below the top, so it crosses.
        assert_eq!(crossings(&cuts, point(16, 20), point(24, 12)), vec![1]);
        // Through the bottom end (5, 0): rising crosses, falling and level do not.
        assert_eq!(crossings(&cuts, point(16, -4), point(24, 4)), vec![1]);
        assert_eq!(crossings(&cuts, point(16, 4), point(24, -4)), NONE_CROSSED);
        assert_eq!(crossings(&cuts, point(16, 0), point(24, 0)), NONE_CROSSED);
    }

    #[test]
    fn crossings_come_in_the_order_the_leg_meets_the_cuts() {
        let cuts = LaneCuts {
            cuts: vec![
                LaneCut {
                    obstacle: 1,
                    x: 3,
                    bottom: 0,
                    top: 5,
                    below: None,
                },
                // Two cuts in one column: the shorter (obstacle 2) lies left of the taller.
                LaneCut {
                    obstacle: 2,
                    x: 7,
                    bottom: 0,
                    top: 3,
                    below: None,
                },
                LaneCut {
                    obstacle: 0,
                    x: 7,
                    bottom: 0,
                    top: 6,
                    below: None,
                },
            ],
        };
        assert_eq!(crossings(&cuts, point(4, 4), point(36, 4)), vec![2, 3, 1]);
        assert_eq!(
            crossings(&cuts, point(36, 4), point(4, 4)),
            vec![-1, -3, -2]
        );
        assert_eq!(
            cuts.word(&[point(4, 4), point(36, 4), point(4, 4)]),
            LaneWord::empty()
        );
    }

    #[test]
    fn splitting_a_leg_keeps_its_crossings() {
        let cuts = one_cut();
        let (p, q) = (point(13, 4), point(30, 15));
        let whole = cuts.word(&[p, q]);
        for eighths in 1..8 {
            let split = p + (q - p) * SimScalar::from_raw(eighths << 29);
            assert_eq!(cuts.word(&[p, split, q]), whole, "split at {eighths}/8");
        }
    }

    #[test]
    fn far_points_do_not_overflow() {
        let cuts = one_cut();
        let (far_west, far_east) = (
            SimVec2::new(SimScalar::MIN, SimScalar::from_i32(2)),
            SimVec2::new(SimScalar::MAX, SimScalar::from_i32(2)),
        );
        assert_eq!(crossings(&cuts, far_west, far_east), vec![1]);
    }

    #[test]
    fn an_island_gets_one_cut_to_the_border_and_a_border_bay_gets_none(
    ) -> Result<(), crate::MapError> {
        let mut map = GameMap::new(12, 12, MapCell::default_ground(CliffLevel(1)))?;
        // Island: cells (4..6, 5..7). Bay touching the west border: cells (0..2, 9).
        for (x, y) in [(4, 5), (5, 5), (4, 6), (5, 6), (0, 9), (1, 9)] {
            map.cell_mut(x, y)?.flags = CellFlags::empty();
        }
        let cuts = LaneCuts::build(&map);
        assert_eq!(
            cuts.cuts(),
            &[LaneCut {
                obstacle: 0,
                x: 4,
                bottom: 0,
                top: 5,
                below: None,
            }]
        );
        Ok(())
    }
}
