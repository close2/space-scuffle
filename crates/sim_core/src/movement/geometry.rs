//! Exact conservative ground footprint geometry shared by placement and production.
//!
//! Ordinary navigation retains its authored clearance classes. This stricter square
//! envelope is used for production egress and endpoint-only placement; only producer
//! egress may explicitly exempt its own foundation occupancy, never terrain.

use sim_map::{CellFlags, GameMap};
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
    // Trade-off: A conservative square map footprint covers the circular collider;
    // its integer cell coverage is independent of movement step length or origin.
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
            let allowed = map.cell(cell.0, cell.1).is_ok_and(|c| {
                c.flags.contains(CellFlags::GROUND_PATHABLE)
                    && !c.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE)
                    || exempt.is_some() && c.occupying_structure == exempt
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
    true
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
