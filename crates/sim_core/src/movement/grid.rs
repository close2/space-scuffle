//! Flat, tick-start spatial index over a slice of bodies.
//!
//! Locomotion and collision each ask "who is near this unit?" once per unit per tick. This module
//! answers that from a snapshot, so the answer does not depend on which units already moved.
//!
//! # Invariants
//! - Entries are slice indices, so callers read neighbours from their own flat records.
//! - Within a bucket, indices keep slice order, and buckets are visited in a fixed order, so a
//!   query's visiting order depends only on the indexed positions.

use crate::steering::{bucket_coordinate, SPATIAL_CELL_SIZE};
use sim_types::{SimScalar, SimVec2};

/// Compressed bucket lists over a uniform grid of `SPATIAL_CELL_SIZE` world units.
#[derive(Debug, Default)]
pub(crate) struct BodyGrid {
    width: usize,
    height: usize,
    /// `starts[b]..starts[b + 1]` delimits bucket `b` in `items`.
    starts: Vec<u32>,
    items: Vec<u32>,
}

impl BodyGrid {
    // PERFORMANCE RATIONALE:
    // Why: A vector per bucket scatters a tick's bodies across the heap and needs a walk over
    // every bucket to reset, and queries then chase one pointer per bucket.
    // What: A counting sort packs all bodies into one array, rebuilt from scratch each tick.
    // Trade-off: `starts` is reset per rebuild, O(map area / 4) of cheap u32 writes, which is
    // small next to one hash of the map and needs no allocation after the first tick.
    /// Rebuilds the index over `positions` for a map of `map_width` by `map_height` cells.
    pub(crate) fn rebuild(
        &mut self,
        map_width: u32,
        map_height: u32,
        positions: impl Iterator<Item = SimVec2> + Clone,
    ) {
        self.width = (map_width as usize).div_ceil(SPATIAL_CELL_SIZE).max(1);
        self.height = (map_height as usize).div_ceil(SPATIAL_CELL_SIZE).max(1);
        self.starts.clear();
        self.starts.resize(self.width * self.height + 1, 0);
        let mut count = 0_u32;
        for position in positions.clone() {
            let bucket = self.bucket_of(position);
            self.starts[bucket + 1] += 1;
            count += 1;
        }
        for bucket in 1..self.starts.len() {
            self.starts[bucket] += self.starts[bucket - 1];
        }
        self.items.clear();
        self.items.resize(count as usize, 0);
        // Filling each bucket from its end, over the bodies in reverse, would need them twice;
        // walking forward with `starts` as cursors, then shifting it back, keeps slice order.
        for (index, position) in (0_u32..).zip(positions) {
            let bucket = self.bucket_of(position);
            let slot = self.starts[bucket];
            self.items[slot as usize] = index;
            self.starts[bucket] = slot + 1;
        }
        for bucket in (1..self.starts.len()).rev() {
            self.starts[bucket] = self.starts[bucket - 1];
        }
        self.starts[0] = 0;
    }

    fn bucket_of(&self, position: SimVec2) -> usize {
        let x = bucket_coordinate(position.x.raw(), SPATIAL_CELL_SIZE, self.width);
        let y = bucket_coordinate(position.y.raw(), SPATIAL_CELL_SIZE, self.height);
        y * self.width + x
    }

    /// Calls `visit` with the slice index of every body in buckets within `reach` of `position`.
    ///
    /// Candidates are a superset: callers apply their own exact distance test.
    pub(crate) fn for_each_near(
        &self,
        position: SimVec2,
        reach: SimScalar,
        mut visit: impl FnMut(usize),
    ) {
        let span = |value: SimScalar, cells: usize| {
            let raw = reach.raw();
            (
                bucket_coordinate(value.raw().saturating_sub(raw), SPATIAL_CELL_SIZE, cells),
                bucket_coordinate(value.raw().saturating_add(raw), SPATIAL_CELL_SIZE, cells),
            )
        };
        let (first_x, last_x) = span(position.x, self.width);
        let (first_y, last_y) = span(position.y, self.height);
        for y in first_y..=last_y {
            let row = y * self.width;
            let start = self.starts[row + first_x] as usize;
            let end = self.starts[row + last_x + 1] as usize;
            for &index in &self.items[start..end] {
                visit(index as usize);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32) -> SimVec2 {
        SimVec2::from_i32(x, y)
    }

    fn near(grid: &BodyGrid, position: SimVec2, reach: i32) -> Vec<usize> {
        let mut found = Vec::new();
        grid.for_each_near(position, SimScalar::from_i32(reach), |i| found.push(i));
        found
    }

    #[test]
    fn query_returns_every_body_in_reach_and_skips_far_ones() {
        let bodies = [at(1, 1), at(3, 1), at(30, 30), at(1, 4)];
        let mut grid = BodyGrid::default();
        grid.rebuild(32, 32, bodies.iter().copied());
        let mut found = near(&grid, at(2, 2), 2);
        found.sort_unstable();
        assert_eq!(found, [0, 1, 3]);
        assert_eq!(near(&grid, at(30, 31), 1), [2]);
    }

    #[test]
    fn bodies_outside_the_map_clamp_into_edge_buckets() {
        let bodies = [at(-5, -5), at(40, 40)];
        let mut grid = BodyGrid::default();
        grid.rebuild(32, 32, bodies.iter().copied());
        assert_eq!(near(&grid, at(0, 0), 1), [0]);
        assert_eq!(near(&grid, at(31, 31), 1), [1]);
    }

    #[test]
    fn rebuild_replaces_the_previous_contents() {
        let mut grid = BodyGrid::default();
        grid.rebuild(16, 16, [at(4, 4), at(5, 4)].into_iter());
        grid.rebuild(16, 16, [at(12, 12)].into_iter());
        assert_eq!(near(&grid, at(4, 4), 1), [0_usize; 0]);
        assert_eq!(near(&grid, at(12, 12), 1), [0]);
    }
}
