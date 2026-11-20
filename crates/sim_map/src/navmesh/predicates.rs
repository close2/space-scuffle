//! Exact integer geometric predicates for the navigation mesh.
//!
//! # Responsibility
//! Owns every orientation, in-circle and distance comparison the mesh makes. Mesh vertices are
//! lattice points in quarter cells ([`LatticePoint`], plan `2026-11-18-contour-footprints` D-3);
//! query points (unit positions, segment ends) are fixed-point world positions reduced to
//! [`QPoint`], a Q16 grid of 1/65,536 cell.
//!
//! # How it fits
//! The constrained Delaunay construction (`cdt.rs`) uses [`orient2d`] and [`incircle`] on lattice
//! points. Point location, the segment predicate and the width test use the Q16 forms. Nothing
//! here uses a float, so every result is the same on every platform (charter §3.1).
//!
//! # Invariants and overflow margins
//! - Lattice coordinates are quarter cells in `0..=4 * NAV_MESH_MAX_DIMENSION` (2^14). Coordinate
//!   differences are below 2^15, so an [`orient2d`] product is below 2^30 and fits `i64` with a
//!   margin of 2^33. [`incircle`] multiplies a squared length (below 2^31) by a 2x2 determinant
//!   (below 2^31) and sums three such terms: below 2^64, evaluated in `i128` with a margin of
//!   2^63.
//! - Q16 coordinates are at most 2^12 * 2^16 = 2^28 in magnitude for points on the map, and the
//!   map test rejects anything else first. Differences are below 2^30, cross and dot products
//!   below 2^59, and a squared cross product below 2^118: every Q16 product is taken in `i128`.
//! - A [`SquaredWidth`] keeps `num / den` in cells squared. Widths of lattice geometry have
//!   `num < 2^62` (a squared cross product of quarter-cell vectors below 2^31) and
//!   `den < 2^35` (16 times a squared quarter-cell length below 2^31), so a comparison of two
//!   widths multiplies below 2^97 in `u128`.
//! - [`SquaredWidth::admits_diameter`] caps the radius at 2^12 world units (2^44 raw): no bounded
//!   width on a map of at most 2^12 cells a side admits a larger disc, and below the cap
//!   `4 r^2 den < 2^125` and `num 2^64 < 2^126`.

use sim_types::{SimScalar, SimVec2};

/// Largest map side, in cells, the navigation mesh supports. Every overflow margin above is
/// derived from it; `GameMap` files are limited to 256 cells by `MAX_MAP_DIMENSION`.
pub const NAV_MESH_MAX_DIMENSION: u32 = 1 << 12;

/// Bits of fraction in a [`QPoint`] coordinate.
pub const Q_BITS: u32 = 16;

/// Bits of fraction in a [`LatticePoint`] coordinate: the lattice is quarter cells.
pub const LATTICE_BITS: u32 = 2;

/// Lattice units per cell, `2^LATTICE_BITS`.
pub const LATTICE_PER_CELL: i32 = 1 << LATTICE_BITS;

/// Squared lattice units per squared cell: a squared length in lattice units divided by this is
/// in cells squared.
const LATTICE_SQ_PER_CELL_SQ: u64 = 1 << (2 * LATTICE_BITS);

/// A mesh vertex: a lattice point in quarter cells (plan `2026-11-18-contour-footprints` D-3).
/// `(4x, 4y)` is the corner shared by cells `(x - 1, y - 1)` to `(x, y)`; the points between are
/// the quarter points that footprint contours need.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct LatticePoint {
    /// Column of the lattice line, in quarter cells from the map's west edge.
    pub x: i32,
    /// Row of the lattice line, in quarter cells from the map's south edge.
    pub y: i32,
}

impl LatticePoint {
    /// Creates a lattice point from quarter-cell coordinates.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The lattice point at the corner `(x, y)` of the cell grid, in whole cells: the corner
    /// shared by cells `(x - 1, y - 1)` to `(x, y)`.
    #[must_use]
    pub const fn cell_corner(x: i32, y: i32) -> Self {
        Self {
            x: x * LATTICE_PER_CELL,
            y: y * LATTICE_PER_CELL,
        }
    }

    /// The same point as a fixed-point world position (exact: a quarter cell is 2^30 raw).
    #[must_use]
    pub fn to_sim(self) -> SimVec2 {
        let raw = |v: i32| SimScalar::from_raw(i64::from(v) << (32 - LATTICE_BITS));
        SimVec2::new(raw(self.x), raw(self.y))
    }

    /// The same point on the Q16 grid (exact).
    #[must_use]
    pub fn to_q(self) -> QPoint {
        QPoint {
            x: i64::from(self.x) << (Q_BITS - LATTICE_BITS),
            y: i64::from(self.y) << (Q_BITS - LATTICE_BITS),
        }
    }
}

/// A point on the Q16 grid: world units times 2^16, rounded towards negative infinity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QPoint {
    /// East coordinate in 1/65,536 world units.
    pub x: i64,
    /// North coordinate in 1/65,536 world units.
    pub y: i64,
}

impl QPoint {
    /// Reduces a Q32.32 world position to the Q16 grid by an arithmetic shift (floor).
    #[must_use]
    pub fn from_sim(point: SimVec2) -> Self {
        Self {
            x: point.x.raw() >> (32 - Q_BITS),
            y: point.y.raw() >> (32 - Q_BITS),
        }
    }
}

/// Twice the signed area of the lattice triangle `(a, b, c)`: positive when `c` lies left of
/// the directed line `a -> b` (counter-clockwise), negative right of it, zero when collinear.
#[must_use]
pub fn orient2d(a: LatticePoint, b: LatticePoint, c: LatticePoint) -> i64 {
    let (abx, aby) = (
        i64::from(b.x) - i64::from(a.x),
        i64::from(b.y) - i64::from(a.y),
    );
    let (acx, acy) = (
        i64::from(c.x) - i64::from(a.x),
        i64::from(c.y) - i64::from(a.y),
    );
    abx * acy - aby * acx
}

/// In-circle test for the counter-clockwise lattice triangle `(a, b, c)`: positive when `d` lies
/// strictly inside its circumcircle, zero on it, negative outside.
#[must_use]
pub fn incircle(a: LatticePoint, b: LatticePoint, c: LatticePoint, d: LatticePoint) -> i128 {
    let row = |p: LatticePoint| {
        let dx = i128::from(p.x) - i128::from(d.x);
        let dy = i128::from(p.y) - i128::from(d.y);
        (dx, dy, dx * dx + dy * dy)
    };
    let (ax, ay, aa) = row(a);
    let (bx, by, bb) = row(b);
    let (cx, cy, cc) = row(c);
    aa * (bx * cy - by * cx) - bb * (ax * cy - ay * cx) + cc * (ax * by - ay * bx)
}

/// Orientation of three Q16 points, as [`orient2d`]: positive for a left turn.
#[must_use]
pub fn orient_q(a: QPoint, b: QPoint, c: QPoint) -> i128 {
    let (abx, aby) = (i128::from(b.x - a.x), i128::from(b.y - a.y));
    let (acx, acy) = (i128::from(c.x - a.x), i128::from(c.y - a.y));
    abx * acy - aby * acx
}

/// A squared distance `num / den` in squared Q16 units, compared exactly.
#[derive(Debug, Clone, Copy)]
pub struct QDistanceSq {
    num: i128,
    den: i128,
}

impl QDistanceSq {
    /// Whether this squared distance is strictly below `radius_q * radius_q`.
    #[must_use]
    pub fn is_below(self, radius_q: i64) -> bool {
        let r = i128::from(radius_q);
        self.num < r * r * self.den
    }

    /// Whether the two sets touch (the distance is zero).
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.num == 0
    }
}

/// Exact squared distance from `p` to the closed segment `[u, v]`, all on the Q16 grid.
#[must_use]
pub fn point_segment_distance_sq(p: QPoint, u: QPoint, v: QPoint) -> QDistanceSq {
    let (dx, dy) = (i128::from(v.x - u.x), i128::from(v.y - u.y));
    let (wx, wy) = (i128::from(p.x - u.x), i128::from(p.y - u.y));
    let length_sq = dx * dx + dy * dy;
    let along = wx * dx + wy * dy;
    if length_sq == 0 || along <= 0 {
        return QDistanceSq {
            num: wx * wx + wy * wy,
            den: 1,
        };
    }
    if along >= length_sq {
        let (ex, ey) = (i128::from(p.x - v.x), i128::from(p.y - v.y));
        return QDistanceSq {
            num: ex * ex + ey * ey,
            den: 1,
        };
    }
    let cross = dx * wy - dy * wx;
    QDistanceSq {
        num: cross * cross,
        den: length_sq,
    }
}

/// Whether the closed segments `[a, b]` and `[c, d]` share a point.
#[must_use]
pub fn segments_touch(a: QPoint, b: QPoint, c: QPoint, d: QPoint) -> bool {
    let d1 = orient_q(c, d, a).signum();
    let d2 = orient_q(c, d, b).signum();
    let d3 = orient_q(a, b, c).signum();
    let d4 = orient_q(a, b, d).signum();
    if d1 * d2 < 0 && d3 * d4 < 0 {
        return true;
    }
    let within = |p: QPoint, q: QPoint, r: QPoint| {
        r.x >= p.x.min(q.x) && r.x <= p.x.max(q.x) && r.y >= p.y.min(q.y) && r.y <= p.y.max(q.y)
    };
    (d1 == 0 && within(c, d, a))
        || (d2 == 0 && within(c, d, b))
        || (d3 == 0 && within(a, b, c))
        || (d4 == 0 && within(a, b, d))
}

/// Whether the closed Q16 segments `[a, b]` and `[c, d]` touch or come closer than `radius_q`.
///
/// The minimum distance between two disjoint segments is attained at an endpoint of one of them,
/// so four point-to-segment tests decide it. Each test compares `num < r^2 den` with `num` below
/// 2^118 and `r^2 den` below 2^103 for a radius below 2^22 Q16 units (64 cells).
#[must_use]
pub fn segments_within(a: QPoint, b: QPoint, c: QPoint, d: QPoint, radius_q: i64) -> bool {
    segments_touch(a, b, c, d)
        || point_segment_distance_sq(a, c, d).is_below(radius_q)
        || point_segment_distance_sq(b, c, d).is_below(radius_q)
        || point_segment_distance_sq(c, a, b).is_below(radius_q)
        || point_segment_distance_sq(d, a, b).is_below(radius_q)
}

/// A squared clearance `num / den` in cells squared between lattice geometry, or unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SquaredWidth {
    num: u64,
    den: u64,
}

impl SquaredWidth {
    /// No obstacle bounds the width.
    pub const UNBOUNDED: Self = Self {
        num: u64::MAX,
        den: 1,
    };

    /// A whole number of squared cells.
    #[must_use]
    pub const fn whole(num: u64) -> Self {
        Self { num, den: 1 }
    }

    /// Whether this is [`Self::UNBOUNDED`].
    #[must_use]
    pub fn is_unbounded(self) -> bool {
        self == Self::UNBOUNDED
    }

    /// Whether `self < other`, exactly.
    #[must_use]
    pub fn less_than(self, other: Self) -> bool {
        if other.is_unbounded() {
            return !self.is_unbounded();
        }
        if self.is_unbounded() {
            return false;
        }
        u128::from(self.num) * u128::from(other.den) < u128::from(other.num) * u128::from(self.den)
    }

    /// The smaller of two widths.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        if other.less_than(self) {
            other
        } else {
            self
        }
    }

    /// Whether a disc of `radius` world units passes: `(2 * radius)^2 <= self`.
    ///
    /// `radius` is Q32.32; the comparison `4 r_raw^2 den <= num 2^64` stays below 2^126 for a
    /// radius below 2^12 world units (module doc), and any larger radius never passes a bounded
    /// width: the widest bounded width on the largest map is its diagonal, below 2^12.5 cells.
    #[must_use]
    pub fn admits_diameter(self, radius: SimScalar) -> bool {
        if self.is_unbounded() {
            return true;
        }
        let raw = u128::from(radius.raw().unsigned_abs());
        if raw >= 1 << 44 {
            return false;
        }
        4 * raw * raw * u128::from(self.den) <= (u128::from(self.num) << 64)
    }

    /// Numerator and denominator, for digests and diagnostics.
    #[must_use]
    pub fn parts(self) -> (u64, u64) {
        (self.num, self.den)
    }

    /// Numerator and denominator in lowest terms (zero is `0 / 1`; [`Self::UNBOUNDED`] keeps its
    /// parts): the same value always yields the same pair, whatever unit it was computed in.
    #[must_use]
    pub fn reduced_parts(self) -> (u64, u64) {
        if self.is_unbounded() {
            return (self.num, self.den);
        }
        let divisor = gcd(self.num, self.den);
        if divisor == 0 {
            return (0, 1);
        }
        (self.num / divisor, self.den / divisor)
    }
}

/// Greatest common divisor by Euclid's algorithm; `gcd(0, 0)` is 0.
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Squared distance from the lattice point `c` to the segment `[a, b]`, in cells squared (the
/// squared lattice distance over [`LATTICE_SQ_PER_CELL_SQ`]).
#[must_use]
pub fn lattice_point_segment_width(
    c: LatticePoint,
    a: LatticePoint,
    b: LatticePoint,
) -> SquaredWidth {
    let (dx, dy) = (
        i64::from(b.x) - i64::from(a.x),
        i64::from(b.y) - i64::from(a.y),
    );
    let (wx, wy) = (
        i64::from(c.x) - i64::from(a.x),
        i64::from(c.y) - i64::from(a.y),
    );
    let length_sq = dx * dx + dy * dy;
    let along = wx * dx + wy * dy;
    let square = |x: i64, y: i64| SquaredWidth {
        num: (x * x + y * y).unsigned_abs(),
        den: LATTICE_SQ_PER_CELL_SQ,
    };
    if length_sq == 0 || along <= 0 {
        return square(wx, wy);
    }
    if along >= length_sq {
        return square(
            i64::from(c.x) - i64::from(b.x),
            i64::from(c.y) - i64::from(b.y),
        );
    }
    let cross = (dx * wy - dy * wx).unsigned_abs();
    SquaredWidth {
        num: cross * cross,
        den: length_sq.unsigned_abs() * LATTICE_SQ_PER_CELL_SQ,
    }
}

/// Squared distance between two lattice points, in cells squared.
#[must_use]
pub fn lattice_distance_sq(a: LatticePoint, b: LatticePoint) -> SquaredWidth {
    let dx = i64::from(b.x) - i64::from(a.x);
    let dy = i64::from(b.y) - i64::from(a.y);
    SquaredWidth {
        num: (dx * dx + dy * dy).unsigned_abs(),
        den: LATTICE_SQ_PER_CELL_SQ,
    }
}

/// Dot product `(a - o) . (b - o)` of lattice vectors.
#[must_use]
pub fn lattice_dot(o: LatticePoint, a: LatticePoint, b: LatticePoint) -> i64 {
    (i64::from(a.x) - i64::from(o.x)) * (i64::from(b.x) - i64::from(o.x))
        + (i64::from(a.y) - i64::from(o.y)) * (i64::from(b.y) - i64::from(o.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32, y: i32) -> LatticePoint {
        LatticePoint::new(x, y)
    }

    #[test]
    fn orientation_signs() {
        assert!(orient2d(p(0, 0), p(1, 0), p(0, 1)) > 0);
        assert!(orient2d(p(0, 0), p(0, 1), p(1, 0)) < 0);
        assert_eq!(orient2d(p(0, 0), p(2, 2), p(5, 5)), 0);
    }

    #[test]
    fn incircle_signs_and_cocircular_square() {
        let (a, b, c) = (p(0, 0), p(2, 0), p(2, 2));
        assert_eq!(incircle(a, b, c, p(0, 2)), 0);
        assert!(incircle(a, b, c, p(1, 1)) > 0);
        assert!(incircle(a, b, c, p(5, 5)) < 0);
    }

    #[test]
    fn extremes_do_not_overflow() {
        // The largest lattice coordinate: the map limit in quarter cells, 2^14.
        let m = i32::try_from(NAV_MESH_MAX_DIMENSION).unwrap_or(i32::MAX) * LATTICE_PER_CELL;
        assert_eq!(m, 1 << 14);
        let (a, b, c) = (p(0, 0), p(m, 0), p(m, m));
        assert_eq!(incircle(a, b, c, p(0, m)), 0);
        assert!(incircle(a, b, c, p(1, m - 1)) > 0);
        assert!(orient2d(a, b, c) > 0);
        assert!(orient2d(p(0, 0), p(m, m - 1), p(m - 1, m - 2)) != 0);
        // The widest width on the largest map compares and admits without overflow.
        let diagonal = lattice_distance_sq(p(0, 0), p(m, m));
        let across = lattice_point_segment_width(p(0, 0), p(m, 1), p(1, m));
        assert!(across.less_than(diagonal));
        let cap = SimScalar::from_i32(4096);
        assert!(!diagonal.admits_diameter(cap));
        assert!(diagonal.admits_diameter(SimScalar::from_i32(2896)));
        assert!(!diagonal.admits_diameter(SimScalar::from_i32(2897)));
    }

    #[test]
    fn lattice_points_are_quarter_cells() {
        assert_eq!(
            LatticePoint::cell_corner(3, 5).to_sim(),
            SimVec2::from_i32(3, 5)
        );
        assert_eq!(
            LatticePoint::new(13, 7).to_sim(),
            SimVec2::new(SimScalar::from_raw(13 << 30), SimScalar::from_raw(7 << 30))
        );
        assert_eq!(
            LatticePoint::new(13, 7).to_q(),
            QPoint::from_sim(LatticePoint::new(13, 7).to_sim())
        );
    }

    #[test]
    fn squared_widths_are_in_cells_squared() {
        // Two lattice points 4 apart are one cell apart: a disc of radius 0.5 passes between
        // them, and one of 0.5 + 2^-32 does not.
        let width = lattice_distance_sq(p(0, 0), p(4, 0));
        assert!(width.admits_diameter(SimScalar::HALF));
        assert!(!width.admits_diameter(SimScalar::HALF + SimScalar::EPSILON));
        let same = |a: SquaredWidth, b: SquaredWidth| !a.less_than(b) && !b.less_than(a);
        assert!(same(width, SquaredWidth::whole(1)), "{width:?}");
        assert_eq!(width.reduced_parts(), (1, 1));
        let perpendicular = lattice_point_segment_width(p(0, 0), p(-4, 4), p(4, 4));
        assert!(
            same(perpendicular, SquaredWidth::whole(1)),
            "{perpendicular:?}"
        );
        assert_eq!(perpendicular.reduced_parts(), (1, 1));
    }

    #[test]
    fn widths_compare_exactly() {
        let p = LatticePoint::cell_corner;
        let w = lattice_point_segment_width(p(0, 0), p(-1, 1), p(1, 1));
        let same = |a: SquaredWidth, b: SquaredWidth| !a.less_than(b) && !b.less_than(a);
        assert!(same(w, SquaredWidth::whole(1)), "{w:?}");
        assert!(w.admits_diameter(SimScalar::HALF));
        assert!(!w.admits_diameter(SimScalar::HALF + SimScalar::EPSILON));
        let diagonal = lattice_point_segment_width(p(0, 0), p(0, 2), p(2, 0));
        assert!(same(diagonal, SquaredWidth::whole(2)), "{diagonal:?}");
        assert!(SquaredWidth::whole(1).less_than(diagonal));
    }

    #[test]
    fn q_distances() {
        let q = |x: i32, y: i32| LatticePoint::cell_corner(x, y).to_q();
        let d = point_segment_distance_sq(q(0, 1), q(-1, 0), q(1, 0));
        assert!(d.is_below(1 << 16 | 1));
        assert!(!d.is_below(1 << 16));
        assert!(segments_within(q(0, 0), q(2, 2), q(0, 2), q(2, 0), 0));
        assert!(!segments_within(
            q(0, 0),
            q(2, 0),
            q(0, 1),
            q(2, 1),
            1 << 16
        ));
        assert!(segments_within(
            q(0, 0),
            q(2, 0),
            q(0, 1),
            q(2, 1),
            (1 << 16) + 1
        ));
    }
}
