//! Authoritative ground-surface connectivity and exact continuous grid traversal.
//!
//! Navigation and physical movement share these predicates so neither smoothing nor
//! steering can turn a ramp into permission to cross a cliff. Geometry is symmetric,
//! uses integer coordinates and fixed-point positions, and allocates no memory.
//! Occupancy, clearance and player knowledge remain policy supplied by the caller.

use crate::grid::{CellFlags, GameMap, RampDir, RampProfile, MAX_RAMP_RUN_CELLS};
use sim_types::SimVec2;

type Cell = (u32, u32);

impl GameMap {
    /// Tests terrain connectivity between the same cell or immediate neighbors.
    ///
    /// Coordinates are grid cells. Cardinal ramps join only matching ramp surfaces
    /// and their lower/upper landings. Both cardinal routes must connect at diagonal
    /// corners. Structure occupancy is ignored here and must be checked separately.
    /// Unsupported legacy diagonal ramps and malformed profiles are not traversable.
    #[must_use]
    pub fn terrain_step_is_connected(&self, from: Cell, to: Cell) -> bool {
        let dx = i64::from(to.0) - i64::from(from.0);
        let dy = i64::from(to.1) - i64::from(from.1);
        if dx.abs() > 1 || dy.abs() > 1 {
            return false;
        }
        if dx != 0 && dy != 0 {
            let x_corner = (to.0, from.1);
            let y_corner = (from.0, to.1);
            return self.cardinal_edge_is_connected(from, x_corner)
                && self.cardinal_edge_is_connected(x_corner, to)
                && self.cardinal_edge_is_connected(from, y_corner)
                && self.cardinal_edge_is_connected(y_corner, to);
        }
        self.cardinal_edge_is_connected(from, to)
    }

    /// Tests an entire ground-motion segment in fixed-point world units.
    ///
    /// The policy receives each entered `(previous, next)` cell pair, beginning with
    /// `(start, start)`. It may reject structures, footprints, unexplored cells or
    /// diagonal obstacle pinches. Terrain is always checked independently. A corner
    /// crossing is one diagonal step; cells merely touched at that point are checked
    /// for terrain continuity, while their occupancy follows the supplied policy.
    /// Out-of-bounds endpoints fail. Grid-boundary endpoints use floor/half-open cells.
    /// The query is allocation-free and examines every crossed cell, even for long moves.
    #[must_use]
    pub fn ground_segment_is_connected(
        &self,
        start: SimVec2,
        end: SimVec2,
        mut step_allowed: impl FnMut(Cell, Cell) -> bool,
    ) -> bool {
        let (Ok(mut current), Ok(goal)) = (self.world_to_cell(start), self.world_to_cell(end))
        else {
            return false;
        };
        if !self.terrain_step_is_connected(current, current) || !step_allowed(current, current) {
            return false;
        }

        // PERFORMANCE RATIONALE:
        // Why: Motion and smoothing must inspect narrow crossed cells without sampling gaps.
        // What: Compare exact boundary times as integer cross products, with no division.
        // Trade-off: i128 arithmetic per crossed cell replaces floating-point DDA and
        // temporary ray buffers. Nonnegative in-map raw coordinates are at most i64::MAX;
        // each product is below 2^126, so signed i128 intermediates cannot overflow.
        let sx = i128::from(start.x.0.to_bits());
        let sy = i128::from(start.y.0.to_bits());
        let dx = i128::from(end.x.0.to_bits()) - sx;
        let dy = i128::from(end.y.0.to_bits()) - sy;
        let unit = 1_i128 << 32;
        let mut remaining =
            u64::from(current.0.abs_diff(goal.0)) + u64::from(current.1.abs_diff(goal.1));

        while current != goal && remaining > 0 {
            let boundary_x = (i128::from(current.0) + i128::from(dx > 0)) * unit;
            let boundary_y = (i128::from(current.1) + i128::from(dy > 0)) * unit;
            let time_x = (boundary_x - sx).abs() * dy.abs();
            let time_y = (boundary_y - sy).abs() * dx.abs();
            let advance_x = current.0 != goal.0 && (current.1 == goal.1 || time_x <= time_y);
            let advance_y = current.1 != goal.1 && (current.0 == goal.0 || time_y <= time_x);
            let mut next = current;
            if advance_x {
                next.0 = if dx > 0 { current.0 + 1 } else { current.0 - 1 };
                remaining -= 1;
            }
            if advance_y {
                next.1 = if dy > 0 { current.1 + 1 } else { current.1 - 1 };
                remaining -= 1;
            }
            if !self.terrain_step_is_connected(current, next) || !step_allowed(current, next) {
                return false;
            }
            current = next;
        }
        current == goal
    }

    fn cardinal_edge_is_connected(&self, from: Cell, to: Cell) -> bool {
        let (Ok(a), Ok(b)) = (self.cell(from.0, from.1), self.cell(to.0, to.1)) else {
            return false;
        };
        // A placed structure removes GROUND_PATHABLE from otherwise usable terrain.
        // Physical and perceived occupancy are deliberately enforced by the caller.
        let terrain_usable = |flags: CellFlags| {
            flags.intersects(CellFlags::GROUND_PATHABLE | CellFlags::OCCUPIED_BY_STRUCTURE)
                && !flags.contains(CellFlags::UNPASSABLE_CLIFF)
        };
        if !terrain_usable(a.flags) || !terrain_usable(b.flags) {
            return false;
        }
        // PERFORMANCE RATIONALE:
        // Why: Portal widening repeatedly checks ordinary flat-cell edges.
        // What: Use the already-loaded cells before computing two ramp profiles.
        // Trade-off: One metadata branch avoids repeated cell lookups for flat
        // terrain while every edge involving a ramp keeps full profile validation.
        if a.ramp.is_none() && b.ramp.is_none() {
            return a.cliff_level == b.cliff_level;
        }
        let ap = self.ramp_profile(from.0, from.1);
        let bp = self.ramp_profile(to.0, to.1);
        if ap.is_some_and(|p| !self.profile_run_is_bounded(from, p))
            || bp.is_some_and(|p| !self.profile_run_is_bounded(to, p))
        {
            return false;
        }
        if from == to {
            return true;
        }
        let displacement = (
            i64::from(to.0) - i64::from(from.0),
            i64::from(to.1) - i64::from(from.1),
        );
        match (ap, bp) {
            (None, None) => a.cliff_level == b.cliff_level,
            (Some(ramp), None) => ramp_joins_landing(ramp, displacement, b.cliff_level.0),
            (None, Some(ramp)) => {
                ramp_joins_landing(ramp, (-displacement.0, -displacement.1), a.cliff_level.0)
            }
            (Some(a), Some(b)) => {
                let Some(axis) = cardinal_axis(a.dir) else {
                    return false;
                };
                a.dir == b.dir
                    && a.low_level == b.low_level
                    && a.run_len == b.run_len
                    && i64::from(b.index) - i64::from(a.index)
                        == displacement.0 * axis.0 + displacement.1 * axis.1
            }
        }
    }

    fn profile_run_is_bounded(&self, cell: Cell, profile: RampProfile) -> bool {
        if !valid_profile(profile) {
            return false;
        }
        let Some(axis) = cardinal_axis(profile.dir) else {
            return false;
        };
        // ramp_profile deliberately bounds its scan. Inspect one cell beyond each
        // reported end so a clipped profile cannot disguise an overlong run.
        for offset in [
            -i64::from(profile.index) - 1,
            i64::from(profile.run_len) - i64::from(profile.index),
        ] {
            let (Ok(x), Ok(y)) = (
                u32::try_from(i64::from(cell.0) + axis.0 * offset),
                u32::try_from(i64::from(cell.1) + axis.1 * offset),
            ) else {
                continue;
            };
            if self.cell(x, y).is_ok_and(|neighbor| {
                neighbor.ramp == Some(profile.dir) && neighbor.cliff_level == profile.low_level
            }) {
                return false;
            }
        }
        true
    }
}

fn valid_profile(profile: RampProfile) -> bool {
    cardinal_axis(profile.dir).is_some()
        && profile.run_len > 0
        && profile.run_len <= MAX_RAMP_RUN_CELLS
        && profile.index < profile.run_len
        && profile.low_level.0.checked_add(1).is_some()
}

fn cardinal_axis(direction: RampDir) -> Option<(i64, i64)> {
    match direction {
        RampDir::North => Some((0, 1)),
        RampDir::South => Some((0, -1)),
        RampDir::East => Some((1, 0)),
        RampDir::West => Some((-1, 0)),
        RampDir::NorthEast | RampDir::NorthWest | RampDir::SouthEast | RampDir::SouthWest => None,
    }
}

fn ramp_joins_landing(profile: RampProfile, displacement: (i64, i64), level: u8) -> bool {
    let Some(axis) = cardinal_axis(profile.dir) else {
        return false;
    };
    if displacement == axis {
        profile.index + 1 == profile.run_len && profile.low_level.0.checked_add(1) == Some(level)
    } else if displacement == (-axis.0, -axis.1) {
        profile.index == 0 && profile.low_level.0 == level
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CliffLevel, MapCell};
    use sim_types::SimScalar;

    #[test]
    fn profile_boundaries_and_landing_orientation() {
        let profile = RampProfile {
            dir: RampDir::North,
            low_level: CliffLevel(1),
            index: 0,
            run_len: 1,
        };
        assert!(valid_profile(profile));
        assert!(ramp_joins_landing(profile, (0, -1), 1));
        assert!(ramp_joins_landing(profile, (0, 1), 2));
        assert!(!ramp_joins_landing(profile, (1, 0), 1));
        for invalid in [
            RampProfile {
                run_len: 0,
                ..profile
            },
            RampProfile {
                run_len: MAX_RAMP_RUN_CELLS + 1,
                ..profile
            },
            RampProfile {
                index: 1,
                ..profile
            },
            RampProfile {
                low_level: CliffLevel(u8::MAX),
                ..profile
            },
            RampProfile {
                dir: RampDir::NorthEast,
                ..profile
            },
        ] {
            assert!(!valid_profile(invalid));
        }
    }

    #[test]
    fn exact_corner_and_boundary_reverse_traversal() -> crate::Result<()> {
        let map = GameMap::new(8, 8, MapCell::default_ground(CliffLevel(1)))?;
        let start = SimVec2::new(SimScalar::from_num(1), SimScalar::from_num(1));
        let end = SimVec2::new(SimScalar::from_num(4), SimScalar::from_num(4));
        let mut forward = Vec::new();
        assert!(map.ground_segment_is_connected(start, end, |a, b| {
            forward.push((a, b));
            true
        }));
        assert_eq!(
            forward,
            [
                ((1, 1), (1, 1)),
                ((1, 1), (2, 2)),
                ((2, 2), (3, 3)),
                ((3, 3), (4, 4))
            ]
        );
        let mut backward = Vec::new();
        assert!(map.ground_segment_is_connected(end, start, |a, b| {
            backward.push((a, b));
            true
        }));
        assert_eq!(
            backward,
            [
                ((4, 4), (4, 4)),
                ((4, 4), (3, 3)),
                ((3, 3), (2, 2)),
                ((2, 2), (1, 1))
            ]
        );
        let invalid = SimVec2::new(SimScalar::from_raw(i64::MAX), SimScalar::ZERO);
        assert!(!map.ground_segment_is_connected(start, invalid, |_, _| true));
        Ok(())
    }
}
