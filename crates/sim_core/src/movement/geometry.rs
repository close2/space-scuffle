//! Exact conservative ground footprint geometry shared by placement and production.
//!
//! A ground body swept along a segment is checked against terrain by the open square envelope of
//! half-side `t` (its terrain radius) and against each structure by its disc against the
//! structure's footprint shape (plan `2026-11-18-contour-footprints` D-8): the disc is the body
//! model of contact and routes, so a point placement admits is one the contact rule holds. It is
//! used for production egress and endpoint-only placement; only producer egress may explicitly
//! exempt its own foundation, never terrain.

use sim_map::{CellFlags, FootprintOccupant, GameMap};
use sim_types::{EntityId, SimScalar, SimVec2};

pub(crate) fn ground_footprint_connected(
    map: &GameMap,
    start: SimVec2,
    end: SimVec2,
    radius: SimScalar,
    exempt: Option<EntityId>,
) -> bool {
    let r = i128::from(radius.raw());
    let sx = i128::from(start.x.raw());
    let sy = i128::from(start.y.raw());
    let ex = i128::from(end.x.raw());
    let ey = i128::from(end.y.raw());
    let bounds = (
        sx.min(ex) - r,
        sy.min(ey) - r,
        sx.max(ex) + r,
        sy.max(ey) + r,
    );
    if bounds.0 < 0
        || bounds.1 < 0
        || bounds.2 > (i128::from(map.width()) << 32)
        || bounds.3 > (i128::from(map.height()) << 32)
        || !map.ground_segment_is_connected(start, end, |_, _| true)
    {
        return false;
    }
    // PERFORMANCE RATIONALE:
    // Why: Center-cell DDA alone misses footprint overlap from fractional origins.
    // What: Stream only the local swept bounding rectangle, testing exact rational
    // segment/cell intersection without sampling, division or temporary buffers.
    // Trade-off: Against terrain cells a conservative square envelope covers the circular
    // collider, and its integer cell coverage is independent of step length or origin;
    // structures are tested by the exact disc against their shape after the loop.
    let touches = |x: u32, y: u32| {
        let x0 = i128::from(x) << 32;
        let y0 = i128::from(y) << 32;
        segment_overlaps_open_box(
            (sx, sy),
            (ex, ey),
            (
                x0 - r,
                y0 - r,
                x0 + (1_i128 << 32) + r,
                y0 + (1_i128 << 32) + r,
            ),
        )
    };
    for y in (bounds.1 >> 32)..=((bounds.3 - 1) >> 32) {
        for x in (bounds.0 >> 32)..=((bounds.2 - 1) >> 32) {
            // The footprint lies inside the map, checked above, so these are valid cells.
            let (Ok(cell_x), Ok(cell_y)) = (u32::try_from(x), u32::try_from(y)) else {
                return false;
            };
            let cell = (cell_x, cell_y);
            if !touches(cell.0, cell.1) {
                continue;
            }
            // The ground under a structure is walkable (D-5); structures are tested below.
            // So is the ground under a resource node; its outline is tested below (plan
            // `2026-11-20-resource-footprints` D-5 C2).
            let allowed = map.cell(cell.0, cell.1).is_ok_and(|c| {
                c.flags.intersects(
                    CellFlags::GROUND_PATHABLE
                        | CellFlags::OCCUPIED_BY_STRUCTURE
                        | CellFlags::OCCUPIED_BY_RESOURCE,
                )
            });
            if !allowed || !map.terrain_step_is_connected(cell, cell) {
                return false;
            }
            for neighbor in [
                (cell.0.checked_sub(1), Some(cell.1)),
                (Some(cell.0), cell.1.checked_sub(1)),
            ] {
                if let Some(other) = neighbor.0.zip(neighbor.1) {
                    if touches(other.0, other.1) && !map.terrain_step_is_connected(other, cell) {
                        return false;
                    }
                }
            }
        }
    }
    !map.footprint_occupants_touched(start, end, radius)
        .any(|(occupant, footprint)| {
            exempt.map(FootprintOccupant::Structure) != Some(occupant)
                && footprint.capsule_meets(start, end, radius)
        })
}

/// Exact open-interior slab intersection; mere tangency is not footprint overlap.
/// The caller first bounds both full-radius endpoints inside the map. With
/// `M=i64::MAX` and `U=2^32`, raw endpoints lie in `[r,M]`, so `|delta|<=M-r`.
/// Cell edges (including one west/south neighbor) give `|numerator|<=M+r+2U`.
/// Thus every product is `<=(M+r+2U)(M-r)<=M²+2UM<2^127`. A stationary
/// segment uses no delta products. All expansion/index arithmetic stays i128.
pub(crate) fn segment_overlaps_open_box(
    start: (i128, i128),
    end: (i128, i128),
    bounds: (i128, i128, i128, i128),
) -> bool {
    let mut lower = (0_i128, 1_i128);
    let mut upper = (1_i128, 1_i128);
    for (origin, delta, min, max) in [
        (start.0, end.0 - start.0, bounds.0, bounds.2),
        (start.1, end.1 - start.1, bounds.1, bounds.3),
    ] {
        if delta == 0 {
            if origin <= min || origin >= max {
                return false;
            }
            continue;
        }
        let (enter, leave, denominator) = if delta > 0 {
            (min - origin, max - origin, delta)
        } else {
            (origin - max, origin - min, -delta)
        };
        if enter * lower.1 > lower.0 * denominator {
            lower = (enter, denominator);
        }
        if leave * upper.1 < upper.0 * denominator {
            upper = (leave, denominator);
        }
        if lower.0 * upper.1 >= upper.0 * lower.1 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    //! T-G1 (plan `2026-12-02-diagonal-cliff-walkability` §6.2, B-251): a swept ground body
    //! keeps its square envelope out of every cut cell's outer triangle.
    use super::*;
    use sim_map::{CliffLevel, MapCell};

    /// Side of the test maps, in cells.
    const SIDE: u32 = 24;

    /// The staircase's constant: high cells (level 1) are `i + j <= STAIR`, low ones (level 0)
    /// `i + j >= STAIR + 1`, in the frame of direction `(1, 1)`. Every step cell `i + j = STAIR`
    /// away from the map's edges is cut, with its foot on `x + y = STAIR + 1`.
    const STAIR: u32 = 23;

    /// A map of the staircase mirrored into direction `dir`: cell `(i, j)` of the `(1, 1)` frame
    /// is cell `(mirror(i), mirror(j))`, mirroring an axis whose component is negative.
    fn staircase(dir: (i8, i8)) -> Result<GameMap, sim_map::MapError> {
        let mut map = GameMap::new(SIDE, SIDE, MapCell::default_ground(CliffLevel(0)))?;
        for j in 0..SIDE {
            for i in 0..SIDE {
                if i + j <= STAIR {
                    let (x, y) = (mirror(i, dir.0), mirror(j, dir.1));
                    map.cell_mut(x, y)?.cliff_level = CliffLevel(1);
                }
            }
        }
        Ok(map)
    }

    /// Cell index `v` of the `(1, 1)` frame on an axis of sign `s`.
    fn mirror(v: u32, s: i8) -> u32 {
        if s > 0 {
            v
        } else {
            SIDE - 1 - v
        }
    }

    /// Raw coordinate `v` of the `(1, 1)` frame on an axis of sign `s`.
    fn mirror_raw(v: i64, s: i8) -> i64 {
        if s > 0 {
            v
        } else {
            (i64::from(SIDE) << 32) - v
        }
    }

    /// The reference: in the `(1, 1)` frame the walls a body on the plateau may not reach are the
    /// low cells and the step cells' outer triangles, whose union away from the staircase's ends
    /// is the open half-plane `x + y > STAIR + 1`. The open square of half-side `r` round a centre
    /// meets it exactly when `cx + cy + 2r > STAIR + 1`, and `cx + cy` is linear along a segment,
    /// so the swept body is admitted exactly when both endpoints keep `cx + cy + 2r <= STAIR + 1`
    /// (tangency is not overlap).
    fn admitted(a: (i64, i64), b: (i64, i64), r: i64) -> bool {
        let limit = i64::from(STAIR + 1) << 32;
        a.0 + a.1 + 2 * r <= limit && b.0 + b.1 + 2 * r <= limit
    }

    /// A deterministic sequence for the sampled endpoints (`SplitMix64`).
    struct Sequence(u64);

    impl Sequence {
        fn below(&mut self, n: u64) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            (z ^ (z >> 31)) % n
        }
    }

    #[test]
    #[ignore = "diagonal-walk WP2 (B-251): red until the footprint test clips the outer triangle"]
    fn swept_bodies_refuse_the_outer_triangle() -> Result<(), sim_map::MapError> {
        // Terrain radii in sixteenths of a cell: every distinct value of the shipped catalog's
        // ground bodies (0.3125 to 1.0) and the smallest and largest admissible quarters.
        let radii = [4_i64, 5, 6, 8, 9, 12, 14, 16];
        let sixteenth = 1_i64 << 28;
        let mut failures = Vec::new();
        let mut checked = 0_u32;
        for dir in [(1_i8, 1_i8), (1, -1), (-1, 1), (-1, -1)] {
            let map = staircase(dir)?;
            let mut sequence = Sequence(0x7E57_D1A6);
            for r16 in radii {
                let r = r16 * sixteenth;
                // Endpoints in the frame's box [9, 14]², on the sixty-fourth lattice: the middle of
                // the staircase, more than two cells from its ends and the map's edges.
                let mut cases: Vec<((i64, i64), (i64, i64))> = Vec::new();
                let point = |sequence: &mut Sequence| {
                    let at = |s: &mut Sequence| (9_i64 << 32) + to_raw64(s.below(5 * 64));
                    (at(sequence), at(sequence))
                };
                for _ in 0..400 {
                    cases.push((point(&mut sequence), point(&mut sequence)));
                }
                // Tangency: both endpoints on the line x + y + 2r = STAIR + 1, and one raw unit
                // beyond it.
                let on = |x: i64| (x, (i64::from(STAIR + 1) << 32) - 2 * r - x);
                cases.push((on(11 << 32), on((12 << 32) + (1 << 31))));
                cases.push((on(11 << 32), (on(12 << 32).0, on(12 << 32).1 + 1)));
                for (a, b) in cases {
                    // Only segments whose endpoints stand on the plateau in the frame.
                    if a.0 + a.1 > i64::from(STAIR + 1) << 32
                        || b.0 + b.1 > i64::from(STAIR + 1) << 32
                    {
                        continue;
                    }
                    let world = |p: (i64, i64)| {
                        SimVec2::new(
                            SimScalar::from_raw(mirror_raw(p.0, dir.0)),
                            SimScalar::from_raw(mirror_raw(p.1, dir.1)),
                        )
                    };
                    let got = ground_footprint_connected(
                        &map,
                        world(a),
                        world(b),
                        SimScalar::from_raw(r),
                        None,
                    );
                    let want = admitted(a, b, r);
                    checked += 1;
                    if got != want {
                        failures.push(format!(
                            "dir {dir:?} r {r16}/16: {a:?} -> {b:?} admitted {got}, expected {want}"
                        ));
                    }
                }
            }
        }
        assert!(checked > 1_000, "premise: enough segments on the plateau");
        let wrongly_refused = failures
            .iter()
            .filter(|f| f.contains("admitted false"))
            .count();
        assert!(
            failures.is_empty(),
            "{} of {checked} segments disagree ({wrongly_refused} wrongly refused), e.g.:\n{}",
            failures.len(),
            failures
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
        Ok(())
    }

    /// `v` sixty-fourths of a cell in raw units.
    fn to_raw64(v: u64) -> i64 {
        i64::try_from(v).map_or(i64::MAX, |v| v << 26)
    }
}
