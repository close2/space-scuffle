//! Exact integer geometric predicates for the navigation mesh.
//!
//! # Responsibility
//! Owns every orientation, in-circle and distance comparison the mesh makes. Mesh vertices are
//! lattice points in whole cells ([`LatticePoint`]); query points (unit positions, segment ends)
//! are fixed-point world positions reduced to [`QPoint`], a Q16 grid of 1/65,536 cell.
//!
//! # How it fits
//! The constrained Delaunay construction (`cdt.rs`) uses [`orient2d`] and [`incircle`] on lattice
//! points. Point location, the segment predicate and the width test use the Q16 forms. Nothing
//! here uses a float, so every result is the same on every platform (charter §3.1).
//!
//! # Invariants and overflow margins
//! - Lattice coordinates lie in `0..=NAV_MESH_MAX_DIMENSION` (2^12). Coordinate differences are
//!   below 2^13, so an [`orient2d`] product is below 2^26 and fits `i64` with a margin of 2^37.
//!   [`incircle`] multiplies a squared length (below 2^27) by a 2x2 determinant (below 2^27) and
//!   sums three such terms: below 2^56, evaluated in `i128` with a margin above 2^70.
//! - Q16 coordinates are at most 2^12 * 2^16 = 2^28 in magnitude for points on the map, and the
//!   map test rejects anything else first. Differences are below 2^30, cross and dot products
//!   below 2^59, and a squared cross product below 2^118: every Q16 product is taken in `i128`.
//! - A [`SquaredWidth`] keeps `num / den` in cells squared with `num < 2^54` and `den < 2^27`,
//!   so a comparison of two widths multiplies below 2^81 in `u128`.

use sim_types::{SimScalar, SimVec2};

/// Largest map side, in cells, the navigation mesh supports. Every overflow margin above is
/// derived from it; `GameMap` files are limited to 256 cells by `MAX_MAP_DIMENSION`.
pub const NAV_MESH_MAX_DIMENSION: u32 = 1 << 12;

/// Bits of fraction in a [`QPoint`] coordinate.
pub const Q_BITS: u32 = 16;

/// A mesh vertex: a lattice point in whole cells. `(x, y)` is the corner shared by cells
/// `(x - 1, y - 1)` to `(x, y)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LatticePoint {
    /// Column of the lattice line, in cells from the map's west edge.
    pub x: i32,
    /// Row of the lattice line, in cells from the map's south edge.
    pub y: i32,
}

impl LatticePoint {
    /// Creates a lattice point from whole-cell coordinates.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The same point as a fixed-point world position.
    #[must_use]
    pub fn to_sim(self) -> SimVec2 {
        SimVec2::from_i32(self.x, self.y)
    }

    /// The same point on the Q16 grid.
    #[must_use]
    pub fn to_q(self) -> QPoint {
        QPoint {
            x: i64::from(self.x) << Q_BITS,
            y: i64::from(self.y) << Q_BITS,
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
    /// `radius` is Q32.32; the comparison `4 r_raw^2 den <= num 2^64` stays below 2^120 for a
    /// radius below 2^20 world units, and any larger radius never passes a bounded width.
    #[must_use]
    pub fn admits_diameter(self, radius: SimScalar) -> bool {
        if self.is_unbounded() {
            return true;
        }
        let raw = u128::from(radius.raw().unsigned_abs());
        if raw >= 1 << 52 {
            return false;
        }
        4 * raw * raw * u128::from(self.den) <= (u128::from(self.num) << 64)
    }

    /// Numerator and denominator, for digests and diagnostics.
    #[must_use]
    pub fn parts(self) -> (u64, u64) {
        (self.num, self.den)
    }
}

/// Squared distance from the lattice point `c` to the segment `[a, b]`, in cells squared.
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
    let square = |x: i64, y: i64| (x * x + y * y).unsigned_abs();
    if length_sq == 0 || along <= 0 {
        return SquaredWidth::whole(square(wx, wy));
    }
    if along >= length_sq {
        return SquaredWidth::whole(square(
            i64::from(c.x) - i64::from(b.x),
            i64::from(c.y) - i64::from(b.y),
        ));
    }
    let cross = (dx * wy - dy * wx).unsigned_abs();
    SquaredWidth {
        num: cross * cross,
        den: length_sq.unsigned_abs(),
    }
}

/// Squared distance between two lattice points, in cells squared.
#[must_use]
pub fn lattice_distance_sq(a: LatticePoint, b: LatticePoint) -> SquaredWidth {
    let dx = i64::from(b.x) - i64::from(a.x);
    let dy = i64::from(b.y) - i64::from(a.y);
    SquaredWidth::whole((dx * dx + dy * dy).unsigned_abs())
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
        let m = i32::try_from(NAV_MESH_MAX_DIMENSION).unwrap_or(i32::MAX);
        let (a, b, c) = (p(0, 0), p(m, 0), p(m, m));
        assert_eq!(incircle(a, b, c, p(0, m)), 0);
        assert!(incircle(a, b, c, p(1, m - 1)) > 0);
        assert!(orient2d(a, b, c) > 0);
    }

    #[test]
    fn widths_compare_exactly() {
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
        let q = |x: i32, y: i32| p(x, y).to_q();
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
