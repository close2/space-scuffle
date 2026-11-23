//! The wavefront fallback of plan `2026-11-20-contact-solver` Amendment 12, step 2′: how a gap
//! walker whose straight way to its gap is blocked finds its way round the crowd.
//!
//! # Responsibility
//! [`SimWorld::wavefront_aim`] returns the step target of such a walker: the centre of the
//! lowest neighbouring cell of an integer distance field (a breadth-first wavefront; Barraquand &
//! Latombe 1991; Emerson, flow fields, *Game AI Pro* ch. 23) spread from the free points of the
//! target's ring over a local grid. Cells are blocked by terrain and by the discs of the bodies
//! that do not give way (enemies, the target, and friends that hold their ground), each inflated
//! by the walker's radius. A distance field has no local minimum other than its sources, and it
//! never runs through terrain, which answers both traces of P2 (evidence §13.3).
//!
//! # How it fits
//! The ring rule (`movement::ring`) asks for it only for a gap walker whose segment to its gap
//! point passes a body; the chase then steps toward the cell while measuring its progress against
//! the gap. One field is built per contested target and walker radius per tick, on first request,
//! and shared by every walker of that target and radius.
//!
//! # The grid (derived, not tuned)
//! - **Cell size `h = r / 2`:** half the walker's radius, so the narrowest opening a walker fits
//!   through (one diameter wide) spans at least two free cells once obstacles are inflated by `r`,
//!   and a disc is never thinner than two cells.
//! - **Extent:** a square of half-size `L = ρ + 2r + 2·r_max + r` round the target's tick-start
//!   position: the ring (`ρ`), one rank of the largest bodies outside it (`2·r_max`), the walker's
//!   own diameter to pass round that rank (`2r`), and its radius so its centre stays inside. A
//!   walker outside the square aims straight at its gap, as before, until it enters.
//! - The square holds at most [`MAX_SIDE`] cells a side; a larger one is not built and the walker
//!   aims straight.
//!
//! # Invariants
//! - **Deterministic and stateless:** integer distances; neighbours visited in a fixed order and
//!   ties broken by that order; built from the tick-start snapshot and the static map alone, with
//!   nothing kept between ticks.
//! - A field depends on its target and radius only, never on which walker asked first.

use super::concave::{blocking_interval, is_free, slot_point, ArcInterval};
use super::ring::RingChase;
use super::{MovementLayer, MovementMathError};
use crate::SimWorld;
use sim_types::{EntityId, PlayerId, SimAngle, SimScalar, SimVec2};

/// The largest grid side, in cells: the bound of one field's memory and work.
pub(crate) const MAX_SIDE: u32 = 512;

/// A cell no wave reached.
const UNREACHED: u32 = u32::MAX;
/// A cell blocked by terrain or a body.
const BLOCKED: u32 = u32::MAX - 1;

/// The neighbours a wave spreads to (4-connected, in this order).
const SPREAD: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
/// The neighbours a walker may step to (8-connected, in this order; ties go to the first).
const STEPS: [(i32, i32); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];

/// One target's distance field of one tick.
#[derive(Debug, Default)]
pub(crate) struct WaveField {
    /// The contested target.
    target: EntityId,
    /// The walkers' radius it was built for, world units.
    radius: SimScalar,
    /// The owner of its walkers, whose holding friends block.
    owner: PlayerId,
    /// The world position of cell (0, 0)'s lower corner.
    origin: SimVec2,
    /// The cell size, world units.
    cell: SimScalar,
    /// Cells a side.
    side: u32,
    /// Distances in cells from the nearest free ring point, row-major; [`UNREACHED`] or
    /// [`BLOCKED`] otherwise.
    distance: Vec<u32>,
}

impl WaveField {
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let side = i32::try_from(self.side).ok()?;
        if x < 0 || y < 0 || x >= side || y >= side {
            return None;
        }
        usize::try_from(y * side + x).ok()
    }

    /// The centre of cell `(x, y)`, world units.
    fn centre(&self, x: i32, y: i32) -> SimVec2 {
        let at = |base: SimScalar, k: i32| {
            base.saturating_add(
                self.cell
                    .saturating_mul(SimScalar::from_num(k))
                    .saturating_add(self.cell.saturating_mul(SimScalar::HALF)),
            )
        };
        SimVec2::new(at(self.origin.x, x), at(self.origin.y, y))
    }

    /// The cell holding `point`.
    fn cell_of(&self, point: SimVec2) -> Option<(i32, i32)> {
        let coordinate = |value: SimScalar, base: SimScalar| {
            let offset = i128::from(value.raw()) - i128::from(base.raw());
            let cells = offset.div_euclid(i128::from(self.cell.raw()).max(1));
            i32::try_from(cells).ok()
        };
        let (x, y) = (
            coordinate(point.x, self.origin.x)?,
            coordinate(point.y, self.origin.y)?,
        );
        self.index(x, y).map(|_| (x, y))
    }

    /// Marks every cell whose centre lies strictly within `reach` of `centre` as blocked.
    fn block_disc(&mut self, centre: SimVec2, reach: SimScalar) {
        let reach_raw = i128::from(reach.raw());
        let (Some(low), Some(high)) = (
            self.clamped_cell(
                centre.x.saturating_sub(reach),
                centre.y.saturating_sub(reach),
            ),
            self.clamped_cell(
                centre.x.saturating_add(reach),
                centre.y.saturating_add(reach),
            ),
        ) else {
            return;
        };
        for y in low.1..=high.1 {
            for x in low.0..=high.0 {
                let point = self.centre(x, y);
                let dx = i128::from(point.x.raw()) - i128::from(centre.x.raw());
                let dy = i128::from(point.y.raw()) - i128::from(centre.y.raw());
                if dx * dx + dy * dy < reach_raw * reach_raw {
                    if let Some(index) = self.index(x, y) {
                        self.distance[index] = BLOCKED;
                    }
                }
            }
        }
    }

    /// The cell of `(x, y)` clamped into the grid, or `None` when the grid is empty.
    fn clamped_cell(&self, x: SimScalar, y: SimScalar) -> Option<(i32, i32)> {
        let side = i32::try_from(self.side).ok()?.checked_sub(1)?;
        let coordinate = |value: SimScalar, base: SimScalar| {
            let offset = i128::from(value.raw()) - i128::from(base.raw());
            let cells = offset.div_euclid(i128::from(self.cell.raw()).max(1));
            i32::try_from(cells.clamp(0, i128::from(side))).unwrap_or(0)
        };
        Some((coordinate(x, self.origin.x), coordinate(y, self.origin.y)))
    }

    /// Spreads the wave from the cells already at distance 0 (4-connected, integer steps).
    // PERFORMANCE RATIONALE:
    // Why: The wave visits every free cell of the grid once per contested target and tick.
    // What: A retained index queue, consumed front to back by a cursor (no deque, no allocation in
    // steady state), and distances in one flat row-major vector.
    // Trade-off: The queue keeps the largest field's capacity between ticks.
    fn spread(&mut self, queue: &mut Vec<u32>) {
        queue.clear();
        for (index, &distance) in self.distance.iter().enumerate() {
            if distance == 0 {
                queue.push(u32::try_from(index).unwrap_or(u32::MAX));
            }
        }
        let side = i32::try_from(self.side).unwrap_or(0);
        let mut cursor = 0;
        while let Some(&index) = queue.get(cursor) {
            cursor += 1;
            let here = i32::try_from(index).unwrap_or(0);
            let (x, y) = (here % side, here / side);
            let next = self.distance[index as usize].saturating_add(1);
            for (dx, dy) in SPREAD {
                if let Some(neighbour) = self.index(x + dx, y + dy) {
                    if self.distance[neighbour] == UNREACHED {
                        self.distance[neighbour] = next;
                        queue.push(u32::try_from(neighbour).unwrap_or(u32::MAX));
                    }
                }
            }
        }
    }

    /// The centre of the reached neighbour of `point`'s cell with the lowest distance (ties to
    /// the first in [`STEPS`] order), or `None` when `point` lies outside the grid or no
    /// neighbour is reached.
    fn descend(&self, point: SimVec2) -> Option<SimVec2> {
        let (x, y) = self.cell_of(point)?;
        let mut best: Option<(u32, (i32, i32))> = None;
        for (dx, dy) in STEPS {
            let Some(index) = self.index(x + dx, y + dy) else {
                continue;
            };
            let distance = self.distance[index];
            if distance < BLOCKED && best.is_none_or(|(low, _)| distance < low) {
                best = Some((distance, (x + dx, y + dy)));
            }
        }
        best.map(|(_, (bx, by))| self.centre(bx, by))
    }
}

impl SimWorld {
    /// The step target of the gap walker `chase` (see the module documentation), or `None` when
    /// it lies outside its target's field or no field can be built.
    ///
    /// # Errors
    /// An unrepresentable distance or point.
    pub(crate) fn wavefront_aim(
        &mut self,
        chase: &RingChase,
        rho: SimScalar,
    ) -> Result<Option<SimVec2>, MovementMathError> {
        let owner = self
            .entities
            .get(&chase.entry.id)
            .map_or(PlayerId(0), |actor| actor.owner);
        let radius = chase.entry.policy.radius;
        let found = self.movement.wave_fields[..self.movement.wave_used]
            .iter()
            .position(|f| f.target == chase.target && f.radius == radius && f.owner == owner);
        let slot = if let Some(slot) = found {
            slot
        } else {
            let slot = self.movement.wave_used;
            if self.movement.wave_fields.len() <= slot {
                self.movement.wave_fields.push(WaveField::default());
            }
            let mut field = std::mem::take(&mut self.movement.wave_fields[slot]);
            let built = self.build_field(&mut field, chase, rho, owner);
            self.movement.wave_fields[slot] = field;
            if !built? {
                return Ok(None);
            }
            self.movement.wave_used += 1;
            slot
        };
        Ok(self.movement.wave_fields[slot].descend(chase.position))
    }

    /// Builds `field` for `chase`'s target and radius: blocks terrain and the bodies that hold
    /// their ground, seeds the free ring points and spreads the wave. `false` when the grid would
    /// exceed [`MAX_SIDE`].
    fn build_field(
        &mut self,
        field: &mut WaveField,
        chase: &RingChase,
        rho: SimScalar,
        owner: PlayerId,
    ) -> Result<bool, MovementMathError> {
        let unrepresentable = || MovementMathError::Unrepresentable;
        let policy = chase.entry.policy;
        let radius = policy.radius;
        let class = policy.body_class();
        let largest = self.movement.largest_radius[class];
        let centre = self.tick_start_position(chase.target);
        let cell = SimScalar::from_raw((radius.raw() / 2).max(1));
        // L = ρ + 2r + 2·r_max + r (see the module documentation).
        let half = rho
            .checked_add(
                radius
                    .checked_mul(SimScalar::from_num(3))
                    .ok_or_else(unrepresentable)?,
            )
            .and_then(|sum| sum.checked_add(largest.checked_mul(SimScalar::from_num(2))?))
            .ok_or_else(unrepresentable)?;
        let cells = (2 * i128::from(half.raw())).div_euclid(i128::from(cell.raw())) + 1;
        let Some(side) = u32::try_from(cells).ok().filter(|&side| side <= MAX_SIDE) else {
            return Ok(false);
        };
        field.target = chase.target;
        field.radius = radius;
        field.owner = owner;
        field.cell = cell;
        field.side = side;
        field.origin = SimVec2::new(centre.x.saturating_sub(half), centre.y.saturating_sub(half));
        field.distance.clear();
        field
            .distance
            .resize(side as usize * side as usize, UNREACHED);
        self.block_terrain(field, policy.layer, radius);
        self.block_holding_bodies(field, chase, owner, half);
        let mut intervals = std::mem::take(&mut self.movement.ring_intervals);
        let seeded = self.seed_ring(field, chase, rho, &mut intervals);
        self.movement.ring_intervals = intervals;
        seeded?;
        let mut queue = std::mem::take(&mut self.movement.wave_queue);
        field.spread(&mut queue);
        self.movement.wave_queue = queue;
        Ok(true)
    }

    /// Blocks every cell whose centre a walker of `radius` could not stand on: off the map, or
    /// within `radius` of a map cell its layer cannot walk.
    fn block_terrain(&self, field: &mut WaveField, layer: MovementLayer, radius: SimScalar) {
        let side = i32::try_from(field.side).unwrap_or(0);
        let (width, height) = (self.map.width(), self.map.height());
        let inside = |p: SimVec2| {
            let r = i128::from(radius.raw());
            let (x, y) = (i128::from(p.x.raw()), i128::from(p.y.raw()));
            x - r >= 0
                && y - r >= 0
                && x + r <= (i128::from(width) << 32)
                && y + r <= (i128::from(height) << 32)
        };
        for y in 0..side {
            for x in 0..side {
                if !inside(field.centre(x, y)) {
                    if let Some(index) = field.index(x, y) {
                        field.distance[index] = BLOCKED;
                    }
                }
            }
        }
        if layer != MovementLayer::Ground {
            return;
        }
        let low = field.origin;
        let span = field.cell.saturating_mul(SimScalar::from_num(side));
        let to_cell = |v: SimScalar| u32::try_from((v.raw() >> 32).max(0)).unwrap_or(0);
        let (x0, y0) = (
            to_cell(low.x.saturating_sub(radius)),
            to_cell(low.y.saturating_sub(radius)),
        );
        let (x1, y1) = (
            to_cell(low.x.saturating_add(span).saturating_add(radius)).min(width.saturating_sub(1)),
            to_cell(low.y.saturating_add(span).saturating_add(radius))
                .min(height.saturating_sub(1)),
        );
        for my in y0..=y1 {
            for mx in x0..=x1 {
                let middle = SimVec2::new(
                    SimScalar::from_num(mx) + SimScalar::HALF,
                    SimScalar::from_num(my) + SimScalar::HALF,
                );
                if !Self::is_position_pathable(&self.map, middle, middle) {
                    block_square(field, (mx, my), radius);
                }
            }
        }
    }

    /// Blocks the discs, inflated by the walker's radius, of the bodies that do not give way to
    /// it: the target, enemies, and friends that hold their ground (anchored, the contact
    /// definition).
    fn block_holding_bodies(
        &self,
        field: &mut WaveField,
        chase: &RingChase,
        owner: PlayerId,
        half: SimScalar,
    ) {
        let class = chase.entry.policy.body_class();
        let centre = self.tick_start_position(chase.target);
        let reach = half.saturating_add(self.movement.largest_radius[class]);
        let obstacles = &self.movement.obstacles[class];
        let radius = chase.entry.policy.radius;
        self.movement.grids[class].for_each_near(centre, reach, |index| {
            let body = &obstacles[index];
            let holds = body.id == chase.target || body.owner != owner || body.anchored;
            if holds && body.id != chase.entry.id {
                field.block_disc(body.pos, radius.saturating_add(body.radius));
            }
        });
    }

    /// Sets distance 0 on every unblocked cell holding a free point of the ring (step 1's exact
    /// intervals over every body but the target), sampled every cell's width of arc.
    fn seed_ring(
        &self,
        field: &mut WaveField,
        chase: &RingChase,
        rho: SimScalar,
        intervals: &mut Vec<ArcInterval>,
    ) -> Result<(), MovementMathError> {
        let policy = chase.entry.policy;
        let class = policy.body_class();
        let centre = self.tick_start_position(chase.target);
        intervals.clear();
        let obstacles = &self.movement.obstacles[class];
        let reach = rho
            .saturating_add(policy.radius)
            .saturating_add(self.movement.largest_radius[class]);
        let mut failed = None;
        self.movement.grids[class].for_each_near(centre, reach, |index| {
            let body = &obstacles[index];
            if body.id == chase.target || failed.is_some() {
                return;
            }
            match blocking_interval(centre, rho, policy.radius, body.pos, body.radius) {
                Ok(Some(interval)) => intervals.push(interval),
                Ok(None) => {}
                Err(error) => failed = Some(error),
            }
        });
        if let Some(error) = failed {
            return Err(error);
        }
        // One cell of arc, h / ρ radians, in binary units: 10,430 per radian, rounded down.
        let step = (i128::from(field.cell.raw()) * 10_430 / i128::from(rho.raw()).max(1)).max(1);
        let step = u32::try_from(step).unwrap_or(1);
        let mut angle = 0_u32;
        while angle < 65_536 {
            let at = SimAngle::from_raw(u16::try_from(angle).unwrap_or(0));
            if is_free(at, intervals) {
                let point = slot_point(centre, rho, at)?;
                if let Some((x, y)) = field.cell_of(point) {
                    if let Some(index) = field.index(x, y) {
                        if field.distance[index] == UNREACHED {
                            field.distance[index] = 0;
                        }
                    }
                }
            }
            angle += step;
        }
        Ok(())
    }
}

/// Blocks every cell of `field` whose centre lies strictly within `radius` of the unit square of
/// map cell `(mx, my)`.
fn block_square(field: &mut WaveField, (mx, my): (u32, u32), radius: SimScalar) {
    let low = SimVec2::new(SimScalar::from_num(mx), SimScalar::from_num(my));
    let high = SimVec2::new(low.x + SimScalar::ONE, low.y + SimScalar::ONE);
    let (Some(a), Some(b)) = (
        field.clamped_cell(low.x.saturating_sub(radius), low.y.saturating_sub(radius)),
        field.clamped_cell(high.x.saturating_add(radius), high.y.saturating_add(radius)),
    ) else {
        return;
    };
    let r = i128::from(radius.raw());
    for y in a.1..=b.1 {
        for x in a.0..=b.0 {
            let p = field.centre(x, y);
            let nearest = |v: SimScalar, lo: SimScalar, hi: SimScalar| {
                i128::from(v.raw()) - i128::from(v.clamp(lo, hi).raw())
            };
            let (dx, dy) = (nearest(p.x, low.x, high.x), nearest(p.y, low.y, high.y));
            if dx * dx + dy * dy < r * r {
                if let Some(index) = field.index(x, y) {
                    field.distance[index] = BLOCKED;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    /// A 10 × 10 field of unit cells at the origin, every cell unreached.
    fn open_field() -> WaveField {
        WaveField {
            cell: SimScalar::ONE,
            side: 10,
            distance: vec![UNREACHED; 100],
            ..WaveField::default()
        }
    }

    /// The wave spreads in integer steps round a wall, never through it, and a walker descends
    /// toward the source round the wall's end, not into it (no local minimum).
    #[test]
    fn the_wave_goes_round_a_wall_and_the_walker_follows_it() {
        let mut field = open_field();
        // A wall on column 5 from row 0 to row 7; the source at (8, 2).
        for y in 0..8 {
            if let Some(index) = field.index(5, y) {
                field.distance[index] = BLOCKED;
            }
        }
        if let Some(index) = field.index(8, 2) {
            field.distance[index] = 0;
        }
        field.spread(&mut Vec::new());
        let at_cell = |field: &WaveField, x, y| field.index(x, y).map(|i| field.distance[i]);
        // Round the wall's end at row 8: from (2, 2) the way is 3 + 6 + 3 + 6 = 18 cells.
        assert_eq!(at_cell(&field, 2, 2), Some(18));
        assert_eq!(at_cell(&field, 5, 3), Some(BLOCKED));
        // The walker at (2.5, 2.5) steps up toward the wall's end, not right into the wall.
        let aim = field.descend(at(2.5, 2.5));
        let up = aim.is_some_and(|p| p.y > at(0.0, 2.5).y);
        assert!(up, "{aim:?}");
        assert!(field.descend(at(20.0, 2.0)).is_none());
    }

    /// A disc blocks the cells whose centres lie strictly inside it, and the grid's cell lookup
    /// is the floor of the offset over the cell size.
    #[test]
    fn a_disc_blocks_the_cells_inside_it() {
        let mut field = open_field();
        field.block_disc(at(5.0, 5.0), SimScalar::ONE);
        let blocked: Vec<(i32, i32)> = (0..10)
            .flat_map(|y| (0..10).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                field
                    .index(x, y)
                    .is_some_and(|i| field.distance[i] == BLOCKED)
            })
            .collect();
        // Centres (4.5, 4.5), (5.5, 4.5), (4.5, 5.5), (5.5, 5.5) lie 0.707 away.
        assert_eq!(blocked, vec![(4, 4), (5, 4), (4, 5), (5, 5)]);
        assert_eq!(field.cell_of(at(3.99, 0.0)), Some((3, 0)));
    }
}
