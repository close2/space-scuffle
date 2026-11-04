//! Brick partition of the smog fluid grid and the per-brick rest flags of its sparse solver.
//!
//! # Responsibility
//! This module owns the index arithmetic of the 8 × 4 × 8 voxel bricks that tile the
//! `96 × 12 × 72` grid of [`super::fluid::SmogGrid3d`] (12 × 3 × 9 = 324 bricks), the 8-voxel row
//! segments the texture-packing pass walks (12 per grid row, 10,368 in all), and the fixed-size
//! flag sets the solver keeps per brick and per segment. It holds no voxel data: the slabs stay in
//! `fluid.rs`, which reads and writes the flags kept here.
//!
//! # How it fits
//! Finding C-18 (`docs/plans/2026-11-04-open-findings-c17-c18-p9-gaps.md`, D-2) makes the solver
//! skip a brick when its result is provably the value it already holds. Each pass of
//! `SmogGrid3d::step` asks the [`BrickState`] whether a brick (or a packing segment) may be
//! skipped, and records what it wrote. The per-voxel arithmetic of a brick that is not skipped is
//! the dense solver's, unchanged, so the output stays bit for bit that of the dense solver.
//!
//! # Invariants
//! - **A rest flag is a one-way promise.** `true` means every voxel of the brick (or segment)
//!   holds its group's rest value, bit for bit. `false` promises nothing: a flag may be `false`
//!   for a brick that is in fact at rest, never `true` for one that is not.
//! - **`occupied` errs the other way.** `false` means no voxel of the brick has
//!   `!(density < 1e-6)` or `!(glow < 1e-6)`; `true` promises nothing.
//! - **No allocation after construction.** Every set is a fixed array; [`BrickState`] is boxed
//!   once by the grid's constructor.
//! - Under `cfg(test)`, `force_dense` makes every skip query answer "compute", which turns the
//!   sparse solver into the dense one through the same per-voxel code, and makes packing walk the
//!   plain linear voxel order (the reference of the differential test in `fluid.rs`).

use super::fluid::{GRID_NX, GRID_NY, GRID_NZ};
use std::ops::{Range, RangeInclusive};

/// Voxels per brick along x.
pub(crate) const BRICK_NX: usize = 8;
/// Voxels per brick along y.
pub(crate) const BRICK_NY: usize = 4;
/// Voxels per brick along z.
pub(crate) const BRICK_NZ: usize = 8;

/// Bricks along x (12).
pub(crate) const BRICKS_X: usize = GRID_NX / BRICK_NX;
/// Bricks along y (3).
pub(crate) const BRICKS_Y: usize = GRID_NY / BRICK_NY;
/// Bricks along z (9).
pub(crate) const BRICKS_Z: usize = GRID_NZ / BRICK_NZ;
/// Bricks in the grid (324).
pub(crate) const BRICK_COUNT: usize = BRICKS_X * BRICKS_Y * BRICKS_Z;

/// Packing segments in the grid: one per brick-wide part of a grid row (10,368).
pub(crate) const SEGMENT_COUNT: usize = BRICKS_X * GRID_NY * GRID_NZ;

// The partition must tile the grid exactly: a partial brick would leave voxels no flag covers.
const _: () = assert!(GRID_NX.is_multiple_of(BRICK_NX));
const _: () = assert!(GRID_NY.is_multiple_of(BRICK_NY));
const _: () = assert!(GRID_NZ.is_multiple_of(BRICK_NZ));

/// Upper bound of a reach, in voxels: larger than every grid dimension, so a reach this large
/// covers the whole grid on its axis.
const REACH_LIMIT: usize = 1 << 16;

/// Flat index of the brick with brick coordinates `(bx, by, bz)`, x fastest (as voxels are).
#[inline]
#[must_use]
pub(crate) const fn brick_index(bx: usize, by: usize, bz: usize) -> usize {
    bx + by * BRICKS_X + bz * (BRICKS_X * BRICKS_Y)
}

/// Brick coordinates `[bx, by, bz]` of the flat brick index `b`.
#[inline]
#[must_use]
pub(crate) const fn brick_coords(b: usize) -> [usize; 3] {
    [
        b % BRICKS_X,
        (b / BRICKS_X) % BRICKS_Y,
        b / (BRICKS_X * BRICKS_Y),
    ]
}

/// Flat index of the brick that contains voxel `(ix, iy, iz)`.
#[inline]
#[must_use]
pub(crate) const fn brick_of_voxel(ix: usize, iy: usize, iz: usize) -> usize {
    brick_index(ix / BRICK_NX, iy / BRICK_NY, iz / BRICK_NZ)
}

/// Half-open voxel index ranges `[x, y, z]` covered by brick `b`.
#[inline]
#[must_use]
pub(crate) fn brick_voxels(b: usize) -> [Range<usize>; 3] {
    let [bx, by, bz] = brick_coords(b);
    [
        bx * BRICK_NX..(bx + 1) * BRICK_NX,
        by * BRICK_NY..(by + 1) * BRICK_NY,
        bz * BRICK_NZ..(bz + 1) * BRICK_NZ,
    ]
}

/// The voxels of brick `b` that lie at least `margin` voxels inside every face of the grid,
/// as half-open ranges `[x, y, z]` (possibly empty). `margin = 1` is the domain of the curl
/// loop (`1..N-1`), `margin = 2` that of the confinement force (`2..N-2`).
#[inline]
#[must_use]
pub(crate) fn brick_voxels_inset(b: usize, margin: usize) -> [Range<usize>; 3] {
    let [x, y, z] = brick_voxels(b);
    let inset = |r: Range<usize>, n: usize| r.start.max(margin)..r.end.min(n - margin);
    [inset(x, GRID_NX), inset(y, GRID_NY), inset(z, GRID_NZ)]
}

/// Flat indices of the face neighbours of brick `b` (at most six; fewer at the grid's faces,
/// where the neighbour would lie outside the grid). Unused entries are `None`.
#[must_use]
pub(crate) fn face_neighbours(b: usize) -> [Option<usize>; 6] {
    let [bx, by, bz] = brick_coords(b);
    let below = |c: usize| c.checked_sub(1);
    let above = |c: usize, n: usize| (c + 1 < n).then_some(c + 1);
    [
        below(bx).map(|x| brick_index(x, by, bz)),
        above(bx, BRICKS_X).map(|x| brick_index(x, by, bz)),
        below(by).map(|y| brick_index(bx, y, bz)),
        above(by, BRICKS_Y).map(|y| brick_index(bx, y, bz)),
        below(bz).map(|z| brick_index(bx, by, z)),
        above(bz, BRICKS_Z).map(|z| brick_index(bx, by, z)),
    ]
}

/// Inclusive brick coordinate ranges `[x, y, z]` of every brick that intersects the inclusive
/// voxel box `[lo, hi]`, clipped to the grid. The bounds are signed so a box may reach past the
/// grid's faces; an axis whose box lies wholly outside the grid, or whose `lo > hi`, yields an
/// empty range.
#[must_use]
pub(crate) fn bricks_in_voxel_box(lo: [isize; 3], hi: [isize; 3]) -> [RangeInclusive<usize>; 3] {
    let axis = |lo: isize, hi: isize, n: usize, brick: usize| -> RangeInclusive<usize> {
        let last = n.cast_signed() - 1;
        if lo > hi || hi < 0 || lo > last {
            // An empty inclusive range (start above end).
            return RangeInclusive::new(1, 0);
        }
        let lo = lo.clamp(0, last).cast_unsigned();
        let hi = hi.clamp(0, last).cast_unsigned();
        lo / brick..=hi / brick
    };
    [
        axis(lo[0], hi[0], GRID_NX, BRICK_NX),
        axis(lo[1], hi[1], GRID_NY, BRICK_NY),
        axis(lo[2], hi[2], GRID_NZ, BRICK_NZ),
    ]
}

/// Reach of a semi-Lagrangian back-trace along one axis, in voxels (D-2, rule 5):
/// `ceil(speed * dt * inv_cell_size) + 2`, where `speed` bounds `|velocity|` along the axis in
/// world units per second, `dt` is in seconds and `inv_cell_size` in voxels per world unit.
/// One voxel of the margin is the trilinear upper corner, one absorbs the rounding of
/// `world_to_grid`. A non-finite or huge product returns a reach that covers the whole grid.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "the product is checked finite, non-negative and below REACH_LIMIT before the cast"
)]
#[must_use]
pub(crate) fn back_trace_reach(speed: f32, dt: f32, inv_cell_size: f32) -> usize {
    let voxels = speed * dt * inv_cell_size;
    // The negated comparison sends a NaN to the full reach.
    if !(voxels >= 0.0 && voxels < REACH_LIMIT as f32) {
        return REACH_LIMIT;
    }
    voxels.ceil() as usize + 2
}

/// One boolean per brick: a rest promise or an occupancy mark, by the owner's convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrickFlags([bool; BRICK_COUNT]);

impl BrickFlags {
    /// A set with every brick's flag equal to `value`.
    #[must_use]
    pub(crate) const fn filled(value: bool) -> Self {
        Self([value; BRICK_COUNT])
    }

    /// The flag of brick `b`.
    #[inline]
    #[must_use]
    pub(crate) fn get(&self, b: usize) -> bool {
        self.0[b]
    }

    /// Sets the flag of brick `b`.
    #[inline]
    pub(crate) fn set(&mut self, b: usize, value: bool) {
        self.0[b] = value;
    }

    /// Whether any brick in the inclusive brick coordinate ranges has its flag set.
    #[must_use]
    pub(crate) fn any_in(&self, ranges: &[RangeInclusive<usize>; 3]) -> bool {
        ranges[2].clone().any(|bz| {
            ranges[1]
                .clone()
                .any(|by| ranges[0].clone().any(|bx| self.0[brick_index(bx, by, bz)]))
        })
    }

    /// Sets the flag of every brick in the inclusive brick coordinate ranges.
    pub(crate) fn set_in(&mut self, ranges: &[RangeInclusive<usize>; 3], value: bool) {
        for bz in ranges[2].clone() {
            for by in ranges[1].clone() {
                for bx in ranges[0].clone() {
                    self.0[brick_index(bx, by, bz)] = value;
                }
            }
        }
    }
}

/// Flat index of the packing segment `sx` (`0..12`) of grid row `(iy, iz)`, in the linear order
/// of the voxels it covers (`ix = 8 sx .. 8 sx + 8`).
#[inline]
#[must_use]
pub(crate) const fn segment_index(sx: usize, iy: usize, iz: usize) -> usize {
    sx + iy * BRICKS_X + iz * (BRICKS_X * GRID_NY)
}

/// One boolean per packing segment: whether its bytes hold the rest bytes (D-2, rule 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SegmentFlags([bool; SEGMENT_COUNT]);

impl SegmentFlags {
    /// A set with every segment's flag equal to `value`.
    #[must_use]
    pub(crate) const fn filled(value: bool) -> Self {
        Self([value; SEGMENT_COUNT])
    }

    /// The flag of segment `s`.
    #[inline]
    #[must_use]
    pub(crate) fn get(&self, s: usize) -> bool {
        self.0[s]
    }

    /// Sets the flag of segment `s`.
    #[inline]
    pub(crate) fn set(&mut self, s: usize, value: bool) {
        self.0[s] = value;
    }
}

/// Whether a density or glow value may count as smog for the dense code's `< 1e-6` tests:
/// `!(value < 1e-6)`, so a NaN counts (the conservative direction for occupancy).
#[inline]
#[must_use]
pub(crate) fn may_hold_smog(value: f32) -> bool {
    value.is_nan() || value >= 1e-6
}

/// Largest of `acc` and `|value|`, where a NaN `value` yields `+inf`: the bound must hold for any
/// comparison the dense code makes, and every comparison with a NaN takes the "not small" side.
#[inline]
#[must_use]
pub(crate) fn max_magnitude(acc: f32, value: f32) -> f32 {
    let magnitude = value.abs();
    if magnitude > acc {
        magnitude
    } else if magnitude.is_nan() {
        f32::INFINITY
    } else {
        acc
    }
}

// PERFORMANCE RATIONALE:
// Why: an empty or mostly empty smog field still cost about 5.7 ms per frame (C-18), because
// every pass of the solver visited every one of the 82,944 voxels, whatever the field held.
// What: rest flags per 8 x 4 x 8 brick (and per 8-voxel row segment for the packed bytes) let
// each pass skip a brick whose result is provably the value it already holds, bit for bit; the
// rules per pass are D-2 of the plan, documented where each pass applies them in `fluid.rs`.
// Trade-off: flag bookkeeping on every write path, and a promise that must hold for correctness
// (a wrong `true` would silently freeze part of the field); the differential test in `fluid.rs`
// compares the sparse and dense solvers on every slab bit for bit, and the flag invariant test
// checks every promise after every step.
/// Every flag the sparse solver keeps beside the slabs of [`super::fluid::SmogGrid3d`].
///
/// Sets named `*_scratch` describe the scratch slab of their group; the advection passes swap
/// them with the field's sets when they swap the slabs.
#[derive(Debug, Clone)]
pub(crate) struct BrickState {
    /// Velocity `u_x`, `u_y`, `u_z` all `+0.0` in the brick.
    pub(crate) velocity_rest: BrickFlags,
    /// As `velocity_rest`, for the velocity scratch slabs.
    pub(crate) velocity_rest_scratch: BrickFlags,
    /// Density and glow `+0.0` and tint the tail tint in the brick.
    pub(crate) scalar_rest: BrickFlags,
    /// As `scalar_rest`, for the scalar scratch slabs.
    pub(crate) scalar_rest_scratch: BrickFlags,
    /// Some voxel may have density or glow not below `1e-6` (conservative direction: `true`).
    pub(crate) occupied: BrickFlags,
    /// As `occupied`, for the scalar scratch slabs.
    pub(crate) occupied_scratch: BrickFlags,
    /// `vorticity_x`, `vorticity_y`, `vorticity_z` and `vorticity_mag` all `+0.0` in the brick.
    pub(crate) vorticity_rest: BrickFlags,
    /// Per axis, the largest `|u|` the last velocity advection wrote into the brick, in world
    /// units per second (`+inf` if it wrote a NaN).
    pub(crate) max_abs_u: [[f32; 3]; BRICK_COUNT],
    /// The largest `vorticity_mag` the last curl pass wrote into the brick, per second.
    pub(crate) max_vort_mag: [f32; BRICK_COUNT],
    /// The segment's density bytes hold alpha `0`.
    pub(crate) density_bytes_rest: SegmentFlags,
    /// The segment's velocity bytes hold `(128, 128, 128, 0)` in every voxel.
    pub(crate) velocity_bytes_rest: SegmentFlags,
    /// Test-only switch: every skip query answers "compute", every reach is infinite.
    #[cfg(test)]
    pub(crate) force_dense: bool,
}

impl BrickState {
    /// A state that promises nothing: every rest flag `false`, every brick occupied. The owner
    /// rescans its slabs to set the real flags.
    #[must_use]
    pub(crate) fn unknown() -> Box<Self> {
        Box::new(Self {
            velocity_rest: BrickFlags::filled(false),
            velocity_rest_scratch: BrickFlags::filled(false),
            scalar_rest: BrickFlags::filled(false),
            scalar_rest_scratch: BrickFlags::filled(false),
            occupied: BrickFlags::filled(true),
            occupied_scratch: BrickFlags::filled(true),
            vorticity_rest: BrickFlags::filled(false),
            max_abs_u: [[f32::INFINITY; 3]; BRICK_COUNT],
            max_vort_mag: [f32::INFINITY; BRICK_COUNT],
            density_bytes_rest: SegmentFlags::filled(false),
            velocity_bytes_rest: SegmentFlags::filled(false),
            #[cfg(test)]
            force_dense: false,
        })
    }

    /// Whether the sparse rules are switched off (only ever `true` in the differential test).
    #[inline]
    #[must_use]
    #[cfg_attr(
        not(test),
        expect(
            clippy::unused_self,
            reason = "the switch exists only in test builds; release builds always answer false"
        )
    )]
    pub(crate) fn dense(&self) -> bool {
        #[cfg(test)]
        {
            self.force_dense
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    /// Velocity advection may skip brick `b`: its source velocity is at rest (rule 4).
    #[inline]
    #[must_use]
    pub(crate) fn velocity_source_at_rest(&self, b: usize) -> bool {
        !self.dense() && self.velocity_rest.get(b)
    }

    /// The curl of brick `b` is the rest value: the brick and its face neighbours have velocity
    /// at rest (rule 2).
    #[must_use]
    pub(crate) fn curl_at_rest(&self, b: usize) -> bool {
        !self.dense()
            && self.velocity_rest.get(b)
            && face_neighbours(b)
                .iter()
                .flatten()
                .all(|&n| self.velocity_rest.get(n))
    }

    /// The confinement force loop skips every voxel of brick `b` (rule 3).
    #[inline]
    #[must_use]
    pub(crate) fn confinement_idle(&self, b: usize) -> bool {
        !self.dense() && self.max_vort_mag[b] < 1e-4
    }

    /// Density advection of brick `b` yields the rest value: no source brick within the
    /// back-trace reach (`reach`, voxels per axis) is occupied (rule 5).
    #[must_use]
    pub(crate) fn scalar_sources_empty(&self, b: usize, reach: [usize; 3]) -> bool {
        if self.dense() {
            return false;
        }
        let [x, y, z] = brick_voxels(b);
        let grow = |r: Range<usize>, by: usize| {
            let by = by.min(REACH_LIMIT).cast_signed();
            (r.start.cast_signed() - by, r.end.cast_signed() - 1 + by)
        };
        let (x0, x1) = grow(x, reach[0]);
        let (y0, y1) = grow(y, reach[1]);
        let (z0, z1) = grow(z, reach[2]);
        !self
            .occupied
            .any_in(&bricks_in_voxel_box([x0, y0, z0], [x1, y1, z1]))
    }

    /// Packing may skip segment `s` of brick `b`: every input is at rest and the bytes already
    /// hold what the dense code would write (rule 6).
    #[inline]
    #[must_use]
    pub(crate) fn segment_at_rest(&self, b: usize, s: usize) -> bool {
        !self.dense()
            && self.scalar_rest.get(b)
            && self.velocity_rest.get(b)
            && self.vorticity_rest.get(b)
            && self.density_bytes_rest.get(s)
            && self.velocity_bytes_rest.get(s)
    }

    /// Exchanges the velocity flags with their scratch set, as the slabs are swapped.
    pub(crate) fn swap_velocity(&mut self) {
        std::mem::swap(&mut self.velocity_rest, &mut self.velocity_rest_scratch);
    }

    /// Exchanges the scalar flags (rest and occupancy) with their scratch sets.
    pub(crate) fn swap_scalars(&mut self) {
        std::mem::swap(&mut self.scalar_rest, &mut self.scalar_rest_scratch);
        std::mem::swap(&mut self.occupied, &mut self.occupied_scratch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voxel_index(ix: usize, iy: usize, iz: usize) -> usize {
        ix + iy * GRID_NX + iz * GRID_NX * GRID_NY
    }

    #[test]
    fn partition_has_the_planned_shape() {
        assert_eq!((BRICKS_X, BRICKS_Y, BRICKS_Z), (12, 3, 9));
        assert_eq!(BRICK_COUNT, 324);
        assert_eq!(SEGMENT_COUNT, 10_368);
    }

    /// Every voxel maps to exactly one brick, that brick's ranges contain it, and the bricks'
    /// ranges together cover each voxel once.
    #[test]
    fn voxel_to_brick_round_trip_covers_every_voxel_once() {
        let mut seen = vec![0u8; GRID_NX * GRID_NY * GRID_NZ];
        for b in 0..BRICK_COUNT {
            let [bx, by, bz] = brick_coords(b);
            assert_eq!(brick_index(bx, by, bz), b);
            let [xs, ys, zs] = brick_voxels(b);
            for iz in zs.clone() {
                for iy in ys.clone() {
                    for ix in xs.clone() {
                        assert_eq!(brick_of_voxel(ix, iy, iz), b);
                        seen[voxel_index(ix, iy, iz)] += 1;
                    }
                }
            }
        }
        assert!(seen.iter().all(|&n| n == 1));
        for iz in 0..GRID_NZ {
            for iy in 0..GRID_NY {
                for ix in 0..GRID_NX {
                    let [xs, ys, zs] = brick_voxels(brick_of_voxel(ix, iy, iz));
                    assert!(xs.contains(&ix) && ys.contains(&iy) && zs.contains(&iz));
                }
            }
        }
    }

    #[test]
    fn segments_follow_linear_voxel_order() {
        let mut expected = 0;
        for iz in 0..GRID_NZ {
            for iy in 0..GRID_NY {
                for sx in 0..BRICKS_X {
                    assert_eq!(segment_index(sx, iy, iz), expected);
                    expected += 1;
                }
            }
        }
    }

    #[test]
    fn inset_ranges_match_the_curl_and_force_domains() {
        let corner = brick_index(0, 0, 0);
        assert_eq!(brick_voxels_inset(corner, 1), [1..8, 1..4, 1..8]);
        assert_eq!(brick_voxels_inset(corner, 2), [2..8, 2..4, 2..8]);
        let far = brick_index(BRICKS_X - 1, BRICKS_Y - 1, BRICKS_Z - 1);
        assert_eq!(brick_voxels_inset(far, 1), [88..95, 8..11, 64..71]);
        assert_eq!(brick_voxels_inset(far, 2), [88..94, 8..10, 64..70]);
        let inner = brick_index(5, 1, 4);
        assert_eq!(brick_voxels_inset(inner, 2), brick_voxels(inner));
    }

    /// Face neighbours are exactly the bricks one step away along one axis, clipped at the
    /// grid's faces: a corner brick has three, an inner brick six.
    #[test]
    fn face_neighbours_are_clipped_at_grid_edges() {
        for b in 0..BRICK_COUNT {
            let [bx, by, bz] = brick_coords(b);
            let found: Vec<usize> = face_neighbours(b).iter().flatten().copied().collect();
            let mut expected = Vec::new();
            for n in 0..BRICK_COUNT {
                let [nx, ny, nz] = brick_coords(n);
                let d = nx.abs_diff(bx) + ny.abs_diff(by) + nz.abs_diff(bz);
                if d == 1 {
                    expected.push(n);
                }
            }
            let mut sorted = found.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, expected, "brick {b}");
        }
        assert_eq!(
            face_neighbours(brick_index(0, 0, 0))
                .iter()
                .flatten()
                .count(),
            3
        );
        assert_eq!(
            face_neighbours(brick_index(4, 1, 4))
                .iter()
                .flatten()
                .count(),
            6
        );
        assert_eq!(
            face_neighbours(brick_index(11, 2, 8))
                .iter()
                .flatten()
                .count(),
            3
        );
    }

    /// The reach query returns every brick that intersects a voxel box, and no other, with the
    /// box clipped to the grid; the brute-force reference tests each brick's voxel ranges.
    #[test]
    fn box_query_returns_exactly_the_intersecting_bricks() {
        let boxes: [([isize; 3], [isize; 3]); 8] = [
            ([0, 0, 0], [0, 0, 0]),
            ([7, 3, 7], [8, 4, 8]),
            ([-5, -5, -5], [3, 2, 1]),
            ([90, 10, 60], [200, 30, 100]),
            ([-1000, -1000, -1000], [1000, 1000, 1000]),
            ([10, 5, 10], [9, 6, 12]),
            ([96, 0, 0], [100, 5, 5]),
            ([-9, 0, 0], [-1, 5, 5]),
        ];
        for (lo, hi) in boxes {
            let ranges = bricks_in_voxel_box(lo, hi);
            for b in 0..BRICK_COUNT {
                let [bx, by, bz] = brick_coords(b);
                let in_query =
                    ranges[0].contains(&bx) && ranges[1].contains(&by) && ranges[2].contains(&bz);
                let vox = brick_voxels(b);
                let intersects = (0..3).all(|a| {
                    let (s, e) = (vox[a].start.cast_signed(), vox[a].end.cast_signed() - 1);
                    lo[a] <= hi[a] && s <= hi[a] && e >= lo[a]
                });
                assert_eq!(in_query, intersects, "box {lo:?}..={hi:?}, brick {b}");
            }
        }
    }

    #[test]
    fn reach_adds_the_corner_and_rounding_margin() {
        assert_eq!(back_trace_reach(0.0, 1.0 / 60.0, 3.4), 2);
        // 2.5 voxels round up to 3, plus the margin of 2.
        assert_eq!(back_trace_reach(2.5, 1.0, 1.0), 5);
        assert_eq!(back_trace_reach(3.0, 1.0, 1.0), 5);
        assert_eq!(back_trace_reach(f32::INFINITY, 0.05, 3.4), REACH_LIMIT);
        assert_eq!(back_trace_reach(f32::NAN, 0.05, 3.4), REACH_LIMIT);
        assert_eq!(back_trace_reach(1.0e30, 0.05, 3.4), REACH_LIMIT);
    }

    #[test]
    fn smog_test_is_the_negated_dense_threshold() {
        for v in [
            0.0,
            -0.0,
            1e-7,
            9.99e-7,
            1e-6,
            2.0,
            -5.0,
            f32::NAN,
            f32::INFINITY,
        ] {
            assert_eq!(
                may_hold_smog(v),
                !matches!(v.partial_cmp(&1e-6), Some(std::cmp::Ordering::Less)),
                "{v}"
            );
        }
    }

    #[test]
    fn max_magnitude_sends_nan_to_infinity() {
        assert_eq!(max_magnitude(0.0, -2.0), 2.0);
        assert_eq!(max_magnitude(3.0, 2.0), 3.0);
        assert_eq!(max_magnitude(3.0, f32::NAN), f32::INFINITY);
        assert_eq!(max_magnitude(0.0, -0.0).to_bits(), 0);
    }

    #[test]
    fn flag_sets_query_and_fill_boxes() {
        let mut flags = BrickFlags::filled(false);
        let all = bricks_in_voxel_box([0, 0, 0], [95, 11, 71]);
        assert!(!flags.any_in(&all));
        let one = bricks_in_voxel_box([17, 5, 30], [17, 5, 30]);
        flags.set_in(&one, true);
        assert!(flags.get(brick_of_voxel(17, 5, 30)));
        assert!(flags.any_in(&all));
        assert!(!flags.any_in(&bricks_in_voxel_box([0, 0, 0], [15, 11, 71])));
        let empty = bricks_in_voxel_box([5, 0, 0], [4, 11, 71]);
        assert!(!flags.any_in(&empty));
    }

    #[test]
    fn dense_switch_disables_every_skip() {
        let mut state = BrickState::unknown();
        for b in 0..BRICK_COUNT {
            state.velocity_rest.set(b, true);
            state.scalar_rest.set(b, true);
            state.vorticity_rest.set(b, true);
            state.occupied.set(b, false);
            state.max_vort_mag[b] = 0.0;
        }
        state.density_bytes_rest = SegmentFlags::filled(true);
        state.velocity_bytes_rest = SegmentFlags::filled(true);
        assert!(state.velocity_source_at_rest(0));
        assert!(state.curl_at_rest(0));
        assert!(state.confinement_idle(0));
        assert!(state.scalar_sources_empty(0, [2, 2, 2]));
        assert!(state.segment_at_rest(0, 0));
        state.force_dense = true;
        assert!(!state.velocity_source_at_rest(0));
        assert!(!state.curl_at_rest(0));
        assert!(!state.confinement_idle(0));
        assert!(!state.scalar_sources_empty(0, [2, 2, 2]));
        assert!(!state.segment_at_rest(0, 0));
    }
}
