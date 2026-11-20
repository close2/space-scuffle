//! Exact geometry of a placed footprint's shape (plans `2026-11-18-contour-footprints` D-2, D-6,
//! D-7 and `2026-11-20-resource-footprints` D-2).
//!
//! # Responsibility
//! A footprint has two layers, as in `StarCraft` II 5.0.14: its **cells** (the game's `Place`
//! layer), which placement, construction and knowledge read, and its **shape** (the game's
//! `Shape`, [`sim_types::FootprintShape`]): a box centred on the cells with 45° chamfered
//! corners, or an outline of up to 16 vertices. This module places every shape as one outline,
//! [`PlacedFootprint`], on the navigation mesh's quarter-cell lattice and owns the exact
//! containment, half-cell, segment and capsule predicates the mesh labels, the clearance raster
//! and the ground-movement rules use. [`StructureFootprint`] is what a caller places: the cells
//! and the shape together.
//!
//! # How it fits
//! `GameMap::place_structure` validates a [`StructureFootprint`] and records the shape on the
//! `PlacedStructure`, whose [`PlacedStructure::footprint`] places it here on demand. The
//! navigation mesh cuts the outline (`navmesh/mod.rs`), the clearance field rasterises it
//! (`pathfinding.rs`), and `sim_core`'s centre rule and unit placement test segments and discs
//! against it.
//!
//! # Invariants
//! - The shape lies in its cells and is centred on them: its centre is
//!   `(4·left + 2·width, 4·bottom + 2·height)` in quarter cells, always a lattice point, so every
//!   outline vertex is a lattice point.
//! - A placed outline is simple, has 3 to 16 vertices, runs counter-clockwise, and every edge
//!   runs along an axis or a diagonal (the mesh's unit-edge model, contour plan D-4). A `Box`
//!   shape's outline is its 4 or 8 contour vertices, counter-clockwise from the east end of its
//!   south face.
//! - Every predicate is exact integer arithmetic: containment by the crossing-number test on
//!   lattice coordinates scaled by an integer, segments and capsules on the Q16 grid of the mesh's
//!   own segment predicate (`i128`, `navmesh/predicates.rs`), so placement, routes and contact
//!   agree on the shape to that grid. For a `Box` each predicate returns what the box's closed
//!   form returns (property-tested against those forms).

use sim_types::{FootprintShape, SimScalar, SimVec2, QUARTER_OUTLINE_CAPACITY};

use crate::error::{MapError, Result};
use crate::grid::{GameMap, PlacedStructure, ResourceKind};
use crate::navmesh::predicates::{segments_touch, segments_within, LatticePoint, QPoint};
use crate::navmesh::segment::radius_q;

/// What a structure takes from the map when it is placed: its footprint cells and its pathing
/// shape (plan D-1, D-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructureFootprint {
    /// Footprint width in cells (the game's `Place` layer).
    pub width: u8,
    /// Footprint height in cells.
    pub height: u8,
    /// Pathing shape, centred on the cells, in quarter cells.
    pub shape: FootprintShape,
}

impl StructureFootprint {
    /// A `width` by `height` footprint whose shape is the full box of its cells.
    #[must_use]
    pub const fn square(width: u8, height: u8) -> Self {
        Self {
            width,
            height,
            shape: FootprintShape::full(width, height),
        }
    }

    /// Checks that the shape lies in its cells and keeps every face. A box: `0 < half_width ≤
    /// width/2`, `0 < half_height ≤ height/2` and `chamfer < min(half_width, half_height)`. An
    /// outline: 3 to 16 vertices inside the cells, every edge of positive length along an axis or
    /// a diagonal, and simple (no two edges meet except adjacent ones at their shared vertex).
    ///
    /// # Errors
    /// [`MapError::InvalidFootprintShape`] when it does not.
    pub fn validate(self) -> Result<()> {
        let fits = match self.shape {
            FootprintShape::Box {
                half_width_q,
                half_height_q,
                chamfer_q,
            } => {
                half_width_q > 0
                    && half_height_q > 0
                    && half_width_q <= 2 * u16::from(self.width)
                    && half_height_q <= 2 * u16::from(self.height)
                    && chamfer_q < half_width_q.min(half_height_q)
            }
            FootprintShape::Outline(outline) => {
                let (hw, hh) = (2 * i32::from(self.width), 2 * i32::from(self.height));
                let vertices: Vec<LatticePoint> = outline
                    .vertices()
                    .iter()
                    .map(|&(x, y)| LatticePoint::new(i32::from(x), i32::from(y)))
                    .collect();
                vertices.iter().all(|v| v.x.abs() <= hw && v.y.abs() <= hh)
                    && outline_is_simple(&vertices)
            }
        };
        if fits {
            Ok(())
        } else {
            Err(MapError::InvalidFootprintShape {
                width: self.width,
                height: self.height,
                shape: self.shape,
            })
        }
    }
}

/// Whether `vertices` form a simple polygon of 3 to 16 vertices whose edges each have positive
/// length and run along an axis or a diagonal.
fn outline_is_simple(vertices: &[LatticePoint]) -> bool {
    let count = vertices.len();
    if !(3..=QUARTER_OUTLINE_CAPACITY).contains(&count) {
        return false;
    }
    let edge = |i: usize| (vertices[i].to_q(), vertices[(i + 1) % count].to_q());
    let on_segment =
        |point: QPoint, (from, to): (QPoint, QPoint)| segments_touch(point, point, from, to);
    for i in 0..count {
        let (from, to) = (vertices[i], vertices[(i + 1) % count]);
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        if (dx, dy) == (0, 0) || !(dx == 0 || dy == 0 || dx.abs() == dy.abs()) {
            return false;
        }
        // The next edge shares only the vertex between them: neither folds back over the other.
        let next = edge((i + 1) % count);
        if on_segment(next.1, edge(i)) || on_segment(edge(i).0, next) {
            return false;
        }
        for j in i + 2..count {
            if i == 0 && j == count - 1 {
                continue;
            }
            let (first, second) = (edge(i), edge(j));
            if segments_touch(first.0, first.1, second.0, second.1) {
                return false;
            }
        }
    }
    true
}

/// Where a point lies relative to a closed outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Location {
    /// In the open interior.
    Inside,
    /// On an edge or a vertex.
    Boundary,
    /// Outside the closed outline.
    Outside,
}

/// A footprint's shape placed on the lattice: its cells and its outline (plan
/// `2026-11-20-resource-footprints` D-2). Built from a validated [`StructureFootprint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlacedFootprint {
    /// Bottom-left (minimum X, minimum Y) cell.
    pub top_left: (u32, u32),
    /// Width in cells.
    pub width: u8,
    /// Height in cells.
    pub height: u8,
    /// The outline's lattice points, counter-clockwise; slots at or past `count` are unused.
    outline: [LatticePoint; QUARTER_OUTLINE_CAPACITY],
    /// How many outline slots are used.
    count: u8,
}

impl PlacedFootprint {
    /// Places `footprint` with its bottom-left cell at `top_left`. The footprint must already be
    /// valid ([`StructureFootprint::validate`]); a placed structure's always is.
    #[must_use]
    pub fn new(top_left: (u32, u32), footprint: StructureFootprint) -> Self {
        let axis = |low: u32, size: u8| {
            i32::try_from(4 * i64::from(low) + 2 * i64::from(size)).unwrap_or(i32::MAX)
        };
        let centre = LatticePoint::new(
            axis(top_left.0, footprint.width),
            axis(top_left.1, footprint.height),
        );
        let at = |dx: i32, dy: i32| LatticePoint::new(centre.x + dx, centre.y + dy);
        let mut outline = [centre; QUARTER_OUTLINE_CAPACITY];
        let count = match footprint.shape {
            FootprintShape::Box {
                half_width_q,
                half_height_q,
                chamfer_q,
            } => {
                let (hw, hh, c) = (
                    i32::from(half_width_q),
                    i32::from(half_height_q),
                    i32::from(chamfer_q),
                );
                let vertices: &[LatticePoint] = if c == 0 {
                    &[at(hw, -hh), at(hw, hh), at(-hw, hh), at(-hw, -hh)]
                } else {
                    &[
                        at(hw - c, -hh),
                        at(hw, -hh + c),
                        at(hw, hh - c),
                        at(hw - c, hh),
                        at(-hw + c, hh),
                        at(-hw, hh - c),
                        at(-hw, -hh + c),
                        at(-hw + c, -hh),
                    ]
                };
                outline[..vertices.len()].copy_from_slice(vertices);
                vertices.len()
            }
            FootprintShape::Outline(quarters) => {
                let vertices = quarters.vertices();
                for (slot, &(x, y)) in outline.iter_mut().zip(vertices) {
                    *slot = at(i32::from(x), i32::from(y));
                }
                let used = &mut outline[..vertices.len()];
                if signed_area_twice(used) < 0 {
                    used.reverse();
                }
                vertices.len()
            }
        };
        Self {
            top_left,
            width: footprint.width,
            height: footprint.height,
            outline,
            count: u8::try_from(count).unwrap_or(0),
        }
    }

    /// The centre of the shape in lattice units (quarter cells): the centre of the cells.
    #[must_use]
    pub fn centre(&self) -> LatticePoint {
        let axis = |low: u32, size: u8| {
            i32::try_from(4 * i64::from(low) + 2 * i64::from(size)).unwrap_or(i32::MAX)
        };
        LatticePoint::new(
            axis(self.top_left.0, self.width),
            axis(self.top_left.1, self.height),
        )
    }

    /// The outline's vertices, counter-clockwise (module invariants).
    #[must_use]
    pub fn contour(&self) -> &[LatticePoint] {
        &self.outline[..usize::from(self.count)]
    }

    /// Whether the point `(x, y) / scale`, in lattice units, lies in the shape's open interior.
    /// Exact for any positive integer `scale`: the mesh labels pass a triangle's vertex sum with
    /// scale 3 (its centroid), and its interior cell lines a unit edge's doubled midpoint with
    /// scale 2.
    #[must_use]
    pub fn contains_open_scaled(&self, x: i64, y: i64, scale: i64) -> bool {
        let scale = i128::from(scale);
        self.locate((i128::from(x), i128::from(y)), |v| {
            (i128::from(v.x) * scale, i128::from(v.y) * scale)
        }) == Location::Inside
    }

    /// Whether the open half-cell whose south-west corner is the half-cell lattice point
    /// `(hx, hy)` (world `(hx/2, hy/2)`) meets the shape's open interior. The open square and the
    /// open outline meet exactly when an outline vertex lies strictly inside the square, an edge
    /// passes through the square's open interior, or (the boundary missing the open square
    /// entirely) the square's centre lies strictly inside the outline.
    #[must_use]
    pub fn meets_open_half_cell(&self, hx: i64, hy: i64) -> bool {
        // The half-cell in lattice units: [x0, x0 + 2] by [y0, y0 + 2].
        let (x0, y0) = (2 * hx, 2 * hy);
        let contour = self.contour();
        let n = contour.len();
        let edge_enters = (0..n).any(|i| {
            let (a, b) = (contour[i], contour[(i + 1) % n]);
            segment_meets_open_box(
                (i64::from(a.x), i64::from(a.y)),
                (i64::from(b.x), i64::from(b.y)),
                (x0, y0),
                (x0 + 2, y0 + 2),
            )
        });
        edge_enters || self.contains_open_scaled(x0 + 1, y0 + 1, 1)
    }

    /// Whether the closed segment `[a, b]` (world units, reduced to the Q16 grid) meets the
    /// closed shape: an end lies in it, or the segment touches its boundary.
    #[must_use]
    pub fn segment_meets(&self, a: SimVec2, b: SimVec2) -> bool {
        let (qa, qb) = (QPoint::from_sim(a), QPoint::from_sim(b));
        self.locate_q(qa) != Location::Outside || self.any_edge(|u, v| segments_touch(qa, qb, u, v))
    }

    /// Whether a disc of `radius` world units swept along `[a, b]` meets the shape's open
    /// interior: an end lies inside, or the segment comes closer to the boundary than the
    /// radius, rounded up to the Q16 grid as the mesh's segment predicate rounds it. Points on
    /// the Q16 grid and radii in multiples of 2⁻¹⁶ are decided exactly. `radius` is positive.
    #[must_use]
    pub fn capsule_meets(&self, a: SimVec2, b: SimVec2, radius: SimScalar) -> bool {
        let (qa, qb) = (QPoint::from_sim(a), QPoint::from_sim(b));
        let r = radius_q(radius);
        self.locate_q(qa) == Location::Inside
            || self.any_edge(|u, v| segments_within(qa, qb, u, v, r))
    }

    /// Where the Q16 point `p` lies relative to the closed shape.
    fn locate_q(&self, p: QPoint) -> Location {
        self.locate((i128::from(p.x), i128::from(p.y)), |v| {
            let q = v.to_q();
            (i128::from(q.x), i128::from(q.y))
        })
    }

    /// Where `p` lies relative to the outline whose vertices `scaled` maps into `p`'s units: on
    /// an edge, or else inside by the crossing number of a ray towards +x.
    fn locate(&self, p: (i128, i128), scaled: impl Fn(LatticePoint) -> (i128, i128)) -> Location {
        let contour = self.contour();
        let n = contour.len();
        let mut inside = false;
        for i in 0..n {
            let (a, b) = (scaled(contour[i]), scaled(contour[(i + 1) % n]));
            let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
            let within = p.0 >= a.0.min(b.0)
                && p.0 <= a.0.max(b.0)
                && p.1 >= a.1.min(b.1)
                && p.1 <= a.1.max(b.1);
            if cross == 0 && within {
                return Location::Boundary;
            }
            // The edge straddles the horizontal line through `p` and passes east of it: left of
            // an upward edge, right of a downward one.
            if (a.1 > p.1) != (b.1 > p.1) && (cross > 0) == (b.1 > a.1) {
                inside = !inside;
            }
        }
        if inside {
            Location::Inside
        } else {
            Location::Outside
        }
    }

    /// Whether `test` holds for some edge of the outline, each edge as a pair of Q16 points.
    fn any_edge(&self, test: impl Fn(QPoint, QPoint) -> bool) -> bool {
        let contour = self.contour();
        let n = contour.len();
        (0..n).any(|i| test(contour[i].to_q(), contour[(i + 1) % n].to_q()))
    }
}

/// Twice the signed area of a polygon: positive when it runs counter-clockwise.
fn signed_area_twice(vertices: &[LatticePoint]) -> i64 {
    let n = vertices.len();
    (0..n)
        .map(|i| {
            let (a, b) = (vertices[i], vertices[(i + 1) % n]);
            i64::from(a.x) * i64::from(b.y) - i64::from(b.x) * i64::from(a.y)
        })
        .sum()
}

/// Whether the closed segment `[a, b]` meets the open box `(low, high)`: the parameter ranges
/// in which each coordinate lies strictly inside its interval, intersected with `[0, 1]`, leave
/// an open interval (Liang–Barsky with strict bounds, compared as exact fractions).
fn segment_meets_open_box(a: (i64, i64), b: (i64, i64), low: (i64, i64), high: (i64, i64)) -> bool {
    // Fractions `num / den` with `den > 0`.
    let (mut lower, mut upper) = ((0_i64, 1_i64), (1_i64, 1_i64));
    for (start, delta, lo, hi) in [
        (a.0, b.0 - a.0, low.0, high.0),
        (a.1, b.1 - a.1, low.1, high.1),
    ] {
        if delta == 0 {
            if start <= lo || start >= hi {
                return false;
            }
            continue;
        }
        let (enter, leave) = if delta > 0 {
            ((lo - start, delta), (hi - start, delta))
        } else {
            ((start - hi, -delta), (start - lo, -delta))
        };
        if enter.0 * lower.1 > lower.0 * enter.1 {
            lower = enter;
        }
        if leave.0 * upper.1 < upper.0 * leave.1 {
            upper = leave;
        }
    }
    lower.0 * upper.1 < upper.0 * lower.1
}

impl PlacedStructure {
    /// The structure's shape placed on its cells.
    #[must_use]
    pub fn footprint(&self) -> PlacedFootprint {
        PlacedFootprint::new(
            self.top_left,
            StructureFootprint {
                width: self.width,
                height: self.height,
                shape: self.shape,
            },
        )
    }

    /// The centre of the shape in lattice units (quarter cells): the centre of the cells.
    #[must_use]
    pub fn shape_centre(&self) -> LatticePoint {
        self.footprint().centre()
    }

    /// [`PlacedFootprint::contains_open_scaled`] of the structure's shape.
    #[must_use]
    pub fn contains_open_scaled(&self, x: i64, y: i64, scale: i64) -> bool {
        self.footprint().contains_open_scaled(x, y, scale)
    }

    /// [`PlacedFootprint::meets_open_half_cell`] of the structure's shape.
    #[must_use]
    pub fn meets_open_half_cell(&self, hx: i64, hy: i64) -> bool {
        self.footprint().meets_open_half_cell(hx, hy)
    }

    /// [`PlacedFootprint::segment_meets`] of the structure's shape.
    #[must_use]
    pub fn segment_meets(&self, a: SimVec2, b: SimVec2) -> bool {
        self.footprint().segment_meets(a, b)
    }

    /// [`PlacedFootprint::capsule_meets`] of the structure's shape.
    #[must_use]
    pub fn capsule_meets(&self, a: SimVec2, b: SimVec2, radius: SimScalar) -> bool {
        self.footprint().capsule_meets(a, b, radius)
    }
}

/// One resource kind's footprint, resolved from `placement_rules.ron` (plan
/// `2026-11-20-resource-footprints` D-1): which cells a node of the kind takes relative to its
/// anchor cell, the pathing shape centred on those cells, and whether a node that empties leaves
/// the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceFootprint {
    /// The node's own cells as `(dx, dy, width, height)`: the rectangle's minimum corner in grid
    /// cells from the anchor cell, and its size in cells (the game's `Place` `x` cells).
    pub cells: (i8, i8, u8, u8),
    /// The pathing shape, centred on those cells, in quarter cells (the game's `Shape`).
    pub shape: FootprintShape,
    /// Whether a node of this kind is removed from the map when it empties (the game's
    /// `RemoveWhenEmpty`).
    pub remove_when_empty: bool,
}

impl ResourceFootprint {
    /// The cells and shape as a [`StructureFootprint`], the form placement validates.
    #[must_use]
    pub const fn footprint(&self) -> StructureFootprint {
        StructureFootprint {
            width: self.cells.2,
            height: self.cells.3,
            shape: self.shape,
        }
    }
}

/// Every resource kind's footprint (plan `2026-11-20-resource-footprints` D-1). The catalog
/// fills it from `placement_rules.ron`; `sim_map` places nodes by it. At most one row per kind.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResourceFootprints {
    rows: Vec<(ResourceKind, ResourceFootprint)>,
}

impl ResourceFootprints {
    /// No footprint for any kind.
    pub const NONE: Self = Self { rows: Vec::new() };

    /// A table of `rows`; a later row of a kind already present is ignored.
    #[must_use]
    pub fn new(rows: Vec<(ResourceKind, ResourceFootprint)>) -> Self {
        let mut table = Self::NONE;
        for (kind, footprint) in rows {
            if table.get(kind).is_none() {
                table.rows.push((kind, footprint));
            }
        }
        table
    }

    /// The footprint of nodes of `kind`, if the table has one.
    #[must_use]
    pub fn get(&self, kind: ResourceKind) -> Option<&ResourceFootprint> {
        self.rows
            .iter()
            .find(|(row_kind, _)| *row_kind == kind)
            .map(|(_, footprint)| footprint)
    }

    /// Every row, in the order given.
    pub fn iter(&self) -> impl Iterator<Item = &(ResourceKind, ResourceFootprint)> + '_ {
        self.rows.iter()
    }
}

impl GameMap {
    /// The distinct structures occupying any cell of the closed bounding box of a disc of
    /// `radius` world units swept from `start` to `end`, each once, in the row-major order of the
    /// first box cell it occupies. Every structure whose shape the capsule could meet is among
    /// them, since a shape lies in its cells. Allocation-free.
    pub fn structure_occupants_touched(
        &self,
        start: SimVec2,
        end: SimVec2,
        radius: SimScalar,
    ) -> impl Iterator<Item = &PlacedStructure> + '_ {
        let r = radius.raw().max(0);
        let cell_range = |a: i64, b: i64, size: u32| {
            let low = a.min(b).saturating_sub(r) >> 32;
            let high = a.max(b).saturating_add(r) >> 32;
            let clamp = |v: i64| u32::try_from(v.clamp(0, i64::from(size) - 1)).unwrap_or(0);
            let empty = high < 0 || low >= i64::from(size);
            (clamp(low), clamp(high), empty)
        };
        let (x0, x1, empty_x) = cell_range(start.x.raw(), end.x.raw(), self.width());
        let (y0, y1, empty_y) = cell_range(start.y.raw(), end.y.raw(), self.height());
        let rows = if empty_x || empty_y {
            y0..y0
        } else {
            y0..y1 + 1
        };
        rows.flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
            .filter_map(move |(x, y)| {
                let id = self.cell(x, y).ok()?.occupying_structure?;
                let placed = self.structures().get(&id)?;
                // A structure's cells form a rectangle, so its first cell in the box is the
                // box cell at its own minimum corner clamped into the box.
                let first = (placed.top_left.0.max(x0), placed.top_left.1.max(y0));
                (first == (x, y)).then_some(placed)
            })
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::many_single_char_names,
    reason = "a broken geometry law must fail the test loudly; the oracles keep the plan's names"
)]
mod tests {
    use super::*;
    use crate::grid::{CliffLevel, GameMap, MapCell};
    use sim_types::EntityId;

    fn shape(hw: u16, hh: u16, c: u16) -> FootprintShape {
        FootprintShape::Box {
            half_width_q: hw,
            half_height_q: hh,
            chamfer_q: c,
        }
    }

    fn placed(left: u32, bottom: u32, width: u8, height: u8, s: FootprintShape) -> PlacedStructure {
        let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a positive side is a valid map size");
        let id = EntityId(7);
        map.place_structure(
            id,
            (left, bottom),
            StructureFootprint {
                width,
                height,
                shape: s,
            },
        )
        .expect("invariant: open ground and a shape inside its cells");
        map.structures()
            .get(&id)
            .cloned()
            .expect("invariant: placed above")
    }

    /// A point given in sixteenths of a cell, on the Q16 grid.
    fn sixteenths(x: i64, y: i64) -> SimVec2 {
        SimVec2::new(SimScalar::from_raw(x << 28), SimScalar::from_raw(y << 28))
    }

    #[test]
    fn a_full_shape_is_its_cells() {
        let p = placed(3, 4, 3, 2, FootprintShape::full(3, 2));
        let footprint = p.footprint();
        assert_eq!(
            footprint.contour(),
            [
                LatticePoint::cell_corner(6, 4),
                LatticePoint::cell_corner(6, 6),
                LatticePoint::cell_corner(3, 6),
                LatticePoint::cell_corner(3, 4),
            ]
        );
        // Every half-cell of the cells meets the shape, and none outside them.
        for hy in 6..14 {
            for hx in 4..14 {
                let inside = (6..12).contains(&hx) && (8..12).contains(&hy);
                assert_eq!(p.meets_open_half_cell(hx, hy), inside, "({hx}, {hy})");
            }
        }
    }

    #[test]
    fn contour_vertices_are_on_the_quarter_lattice_for_odd_and_even_sizes() {
        let cases = [
            // 1×1 Sensor Tower: centred on a cell centre, uncut.
            (
                1,
                1,
                shape(2, 2, 0),
                vec![(12, 8), (12, 12), (8, 12), (8, 8)],
            ),
            // 2×2 contour: centred on a lattice corner, cut by 0.5.
            (
                2,
                2,
                shape(4, 4, 2),
                vec![
                    (14, 8),
                    (16, 10),
                    (16, 14),
                    (14, 16),
                    (10, 16),
                    (8, 14),
                    (8, 10),
                    (10, 8),
                ],
            ),
            // Spore Crawler: a 1×1 box cut by 0.25, centred in its 2×2 cells.
            (
                2,
                2,
                shape(2, 2, 1),
                vec![
                    (13, 10),
                    (14, 11),
                    (14, 13),
                    (13, 14),
                    (11, 14),
                    (10, 13),
                    (10, 11),
                    (11, 10),
                ],
            ),
            // 3×3 contour: centred on a cell centre.
            (
                3,
                3,
                shape(6, 6, 2),
                vec![
                    (18, 8),
                    (20, 10),
                    (20, 18),
                    (18, 20),
                    (10, 20),
                    (8, 18),
                    (8, 10),
                    (10, 8),
                ],
            ),
            // 5×5 contour, cut by 1.5.
            (
                5,
                5,
                shape(10, 10, 6),
                vec![
                    (22, 8),
                    (28, 14),
                    (28, 22),
                    (22, 28),
                    (14, 28),
                    (8, 22),
                    (8, 14),
                    (14, 8),
                ],
            ),
            // A scratch 3×2 with (1.5, 1, 0.5).
            (
                3,
                2,
                shape(6, 4, 2),
                vec![
                    (18, 8),
                    (20, 10),
                    (20, 14),
                    (18, 16),
                    (10, 16),
                    (8, 14),
                    (8, 10),
                    (10, 8),
                ],
            ),
        ];
        for (width, height, s, expected) in cases {
            let p = placed(2, 2, width, height, s);
            let footprint = p.footprint();
            let got: Vec<(i32, i32)> = footprint.contour().iter().map(|v| (v.x, v.y)).collect();
            assert_eq!(got, expected, "{width}x{height} {s:?}");
        }
    }

    #[test]
    fn contains_open_is_exact_on_a_chamfer() {
        // A 2×2 cut by 0.5 at cells (2, 2): its south-west chamfer runs from lattice (10, 8) to
        // (8, 10), on the line x + y = 18.
        let p = placed(2, 2, 2, 2, shape(4, 4, 2));
        // The chamfer's midpoint (9, 9) is on the boundary, not in the open interior.
        assert!(!p.contains_open_scaled(9, 9, 1));
        // A third of a lattice unit inside it, and outside it.
        assert!(p.contains_open_scaled(28, 27, 3));
        assert!(!p.contains_open_scaled(27, 27, 3));
        // The cell corner the chamfer cuts away is outside; the centre is inside.
        assert!(!p.contains_open_scaled(8, 8, 1));
        assert!(p.contains_open_scaled(12, 12, 1));
        // Faces are boundary too.
        assert!(!p.contains_open_scaled(16, 12, 1));
        assert!(p.contains_open_scaled(47, 36, 3));
    }

    #[test]
    fn segment_meets_the_closed_shape() {
        // The 2×2 contour at cells (2, 2): world chamfer x + y = 4.5 from (2.5, 2) to (2, 2.5).
        let p = placed(2, 2, 2, 2, shape(4, 4, 2));
        // Along the cut corner, outside the shape: x + y = 4.25 < 4.5.
        assert!(!p.segment_meets(sixteenths(32, 36), sixteenths(36, 32)));
        // Touching the chamfer at one point: x + y = 4.5 at its end.
        assert!(p.segment_meets(sixteenths(32, 32), sixteenths(36, 36)));
        // Through the shape.
        assert!(p.segment_meets(sixteenths(16, 48), sixteenths(80, 48)));
        // Inside it entirely.
        assert!(p.segment_meets(sixteenths(46, 46), sixteenths(50, 50)));
        // Ending on the east face, and stopping one sixteenth short of it.
        assert!(p.segment_meets(sixteenths(80, 48), sixteenths(64, 48)));
        assert!(!p.segment_meets(sixteenths(80, 48), sixteenths(65, 48)));
    }

    #[test]
    fn capsule_meets_is_exact_at_a_chamfer() {
        // A 5×5 contour at cells (2, 2) to (6, 6), centred on (4.5, 4.5): its south face is
        // y = 2 from x = 3.5 to x = 5.5, its south-west chamfer runs from (3.5, 2) to (2, 3.5),
        // on x + y = 5.5.
        let p = placed(2, 2, 5, 5, shape(10, 10, 6));
        let t = SimScalar::from_raw(5 << 28); // 0.3125
        let sixteenth = SimScalar::from_raw(1 << 28);
        // A point on the Q16 grid exactly t from the chamfer's south end (3.5, 2), in that
        // vertex's region (direction (-3, -4) / 5 lies between the face and the chamfer normals).
        let near_vertex = sixteenths(56 - 3, 32 - 4);
        assert!(!p.capsule_meets(near_vertex, near_vertex, t));
        assert!(p.capsule_meets(near_vertex, near_vertex, t + SimScalar::from_raw(1 << 16)));
        // A point on the chamfer's normal through its midpoint (2.75, 2.75), a quarter cell out
        // on each axis: (2.5, 2.5) is (5.5 - 5) / √2 = √2 / 4 ≈ 0.353553 from the chamfer.
        let off_face = sixteenths(40, 40);
        let below = SimScalar::from_raw(23_170 << 16); // 0.353546..., below √2 / 4
        let above = SimScalar::from_raw(23_171 << 16); // 0.353561..., above it
        assert!(!p.capsule_meets(off_face, off_face, below));
        assert!(p.capsule_meets(off_face, off_face, above));
        // A sweep one cell west from that point comes no nearer than its start, so a disc of t
        // does not meet the shape; a disc a sixteenth larger does.
        let start = near_vertex;
        let end = sixteenths(56 - 3 - 16, 32 - 4);
        assert!(!p.capsule_meets(start, end, t));
        assert!(p.capsule_meets(start, end, t + sixteenth));
        // A centre inside the shape always meets.
        let inside = sixteenths(72, 72);
        assert!(p.capsule_meets(inside, inside, t));
    }

    #[test]
    fn occupants_touched_are_distinct_and_in_cell_order() {
        let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a positive side is a valid map size");
        for (id, cell, size) in [
            (1, (4, 4), 3),
            (2, (8, 4), 2),
            (3, (4, 9), 2),
            (4, (20, 20), 2),
        ] {
            map.place_structure(EntityId(id), cell, StructureFootprint::square(size, size))
                .expect("invariant: open ground");
        }
        let ids = |a: SimVec2, b: SimVec2, r: SimScalar| -> Vec<u32> {
            map.structure_occupants_touched(a, b, r)
                .map(|p| p.id.0)
                .collect()
        };
        let quarter = SimScalar::from_raw(1 << 30);
        // A sweep from (5.5, 3) to (9.5, 10) at 0.25 touches structures 1, 2 and 3 once each.
        assert_eq!(
            ids(sixteenths(88, 48), sixteenths(152, 160), quarter),
            [1, 2, 3]
        );
        // Reversed, the order is still the cells' order.
        assert_eq!(
            ids(sixteenths(152, 160), sixteenths(88, 48), quarter),
            [1, 2, 3]
        );
        // A disc beside structure 4 (cells 20 and 21) whose box ends just short of it, and one
        // whose closed box reaches its first cell.
        let beside = sixteenths(304, 304);
        assert_eq!(
            ids(beside, beside, SimScalar::ONE - SimScalar::EPSILON),
            [0_u32; 0]
        );
        assert_eq!(ids(beside, beside, SimScalar::ONE), [4]);
        // Off the map, nothing.
        assert_eq!(
            ids(sixteenths(-64, -64), sixteenths(-32, -32), quarter),
            [0_u32; 0]
        );
    }

    #[test]
    fn a_shape_outside_its_cells_is_refused() {
        let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a positive side is a valid map size");
        for (width, height, s) in [
            (2, 2, shape(5, 4, 2)),
            (2, 2, shape(4, 4, 4)),
            (2, 2, shape(0, 4, 0)),
            (3, 2, shape(6, 5, 1)),
        ] {
            let footprint = StructureFootprint {
                width,
                height,
                shape: s,
            };
            assert_eq!(
                map.place_structure(EntityId(3), (4, 4), footprint),
                Err(MapError::InvalidFootprintShape {
                    width,
                    height,
                    shape: s
                })
            );
        }
        assert!(map.structures().is_empty());
        assert!(map
            .place_structure(EntityId(3), (4, 4), StructureFootprint::square(2, 2))
            .is_ok());
    }

    // ---- Plan `2026-11-20-resource-footprints` WP1: the general outline against the box. ----

    /// The box's contour by its closed form, counter-clockwise from the east end of its south
    /// face: the test oracle of the general outline (today's construction before WP1).
    fn box_contour(p: &PlacedFootprint, hw: i32, hh: i32, c: i32) -> Vec<LatticePoint> {
        let centre = p.centre();
        let at = |dx: i32, dy: i32| LatticePoint::new(centre.x + dx, centre.y + dy);
        if c == 0 {
            return vec![at(hw, -hh), at(hw, hh), at(-hw, hh), at(-hw, -hh)];
        }
        vec![
            at(hw - c, -hh),
            at(hw, -hh + c),
            at(hw, hh - c),
            at(hw - c, hh),
            at(-hw + c, hh),
            at(-hw, hh - c),
            at(-hw, -hh + c),
            at(-hw + c, -hh),
        ]
    }

    /// The box's closed-form open containment of `(x, y) / scale` (lattice units).
    fn box_contains_open_scaled(
        p: &PlacedFootprint,
        b: (i64, i64, i64),
        x: i64,
        y: i64,
        s: i64,
    ) -> bool {
        let centre = p.centre();
        let (hw, hh, c) = b;
        let dx = (x - s * i64::from(centre.x)).abs();
        let dy = (y - s * i64::from(centre.y)).abs();
        dx < s * hw && dy < s * hh && dx + dy < s * (hw + hh - c)
    }

    /// The box's closed-form half-cell test: four separating directions.
    fn box_meets_open_half_cell(p: &PlacedFootprint, b: (i64, i64, i64), hx: i64, hy: i64) -> bool {
        let centre = p.centre();
        let (cx, cy) = (i64::from(centre.x), i64::from(centre.y));
        let (hw, hh, c) = b;
        let reach = hw + hh - c;
        let (x0, y0) = (2 * hx, 2 * hy);
        let overlaps = |la: i64, ha: i64, lb: i64, hb: i64| la < hb && lb < ha;
        overlaps(x0, x0 + 2, cx - hw, cx + hw)
            && overlaps(y0, y0 + 2, cy - hh, cy + hh)
            && overlaps(x0 + y0, x0 + y0 + 4, cx + cy - reach, cx + cy + reach)
            && overlaps(x0 - y0 - 2, x0 - y0 + 2, cx - cy - reach, cx - cy + reach)
    }

    /// The box's closed-form containment of a Q16 point.
    fn box_contains_q(p: &PlacedFootprint, b: (i64, i64, i64), q: QPoint, open: bool) -> bool {
        let centre = p.centre().to_q();
        let shift = crate::navmesh::predicates::Q_BITS - crate::navmesh::predicates::LATTICE_BITS;
        let (hw, hh, c) = b;
        let (hw, hh, reach) = (hw << shift, hh << shift, (hw + hh - c) << shift);
        let (dx, dy) = ((q.x - centre.x).abs(), (q.y - centre.y).abs());
        if open {
            dx < hw && dy < hh && dx + dy < reach
        } else {
            dx <= hw && dy <= hh && dx + dy <= reach
        }
    }

    /// Every valid box of footprints 1×1 to 5×5: `(width, height, hw, hh, c)`.
    fn every_box() -> Vec<(u8, u8, u16, u16, u16)> {
        let mut boxes = Vec::new();
        for width in 1..=5_u8 {
            for height in 1..=5_u8 {
                for hw in 1..=2 * u16::from(width) {
                    for hh in 1..=2 * u16::from(height) {
                        for c in 0..hw.min(hh) {
                            boxes.push((width, height, hw, hh, c));
                        }
                    }
                }
            }
        }
        boxes
    }

    #[test]
    fn a_box_outline_is_its_contour() {
        let boxes = every_box();
        assert!(boxes.len() > 1_000);
        for (width, height, hw, hh, c) in boxes {
            for top_left in [(0, 0), (3, 7), (10, 4)] {
                let footprint = StructureFootprint {
                    width,
                    height,
                    shape: shape(hw, hh, c),
                };
                footprint
                    .validate()
                    .expect("invariant: every_box yields valid boxes");
                let p = PlacedFootprint::new(top_left, footprint);
                let expected = box_contour(&p, i32::from(hw), i32::from(hh), i32::from(c));
                assert_eq!(
                    p.contour(),
                    expected.as_slice(),
                    "{footprint:?} at {top_left:?}"
                );
            }
        }
    }

    /// A Q16 coordinate near the lattice: a quarter-cell multiple plus a small offset, so that
    /// boundaries, vertices and chamfers are hit exactly as well as just missed.
    fn near_lattice(quarters: i64, offset: i64) -> i64 {
        (quarters << 14) + offset
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

        #[test]
        fn general_predicates_agree_with_the_box_closed_forms(
            pick in 0_usize..100_000,
            left in 0_u32..6,
            bottom in 0_u32..6,
            points in proptest::collection::vec(
                (-4_i64..28, -4_i64..28, -2_i64..=2, -2_i64..=2, 1_i64..=3), 48),
            radius_q in 1_i64..(1 << 17),
        ) {
            let boxes = every_box();
            let (width, height, hw, hh, c) = boxes[pick % boxes.len()];
            let footprint = StructureFootprint { width, height, shape: shape(hw, hh, c) };
            let p = PlacedFootprint::new((left, bottom), footprint);
            let b = (i64::from(hw), i64::from(hh), i64::from(c));
            let origin = (4 * i64::from(left), 4 * i64::from(bottom));
            let radius = SimScalar::from_raw(radius_q << 16);
            let r = radius_q;
            let contour: Vec<QPoint> = box_contour(&p, i32::from(hw), i32::from(hh), i32::from(c))
                .iter()
                .map(|v| v.to_q())
                .collect();
            let n = contour.len();
            for window in points.windows(2) {
                let (ax, ay, oa, ob, scale) = window[0];
                let (bx, by, oc, od, _) = window[1];
                // Lattice containment at scale 1 to 3.
                let (x, y) = (scale * (origin.0 + ax) + oa, scale * (origin.1 + ay) + ob);
                proptest::prop_assert_eq!(
                    p.contains_open_scaled(x, y, scale),
                    box_contains_open_scaled(&p, b, x, y, scale),
                    "contains ({}, {}) / {}", x, y, scale
                );
                // Half-cells.
                let (hx, hy) = ((origin.0 + ax).div_euclid(2), (origin.1 + ay).div_euclid(2));
                proptest::prop_assert_eq!(
                    p.meets_open_half_cell(hx, hy),
                    box_meets_open_half_cell(&p, b, hx, hy),
                    "half-cell ({}, {})", hx, hy
                );
                // Segments and capsules on the Q16 grid.
                let qa = QPoint { x: near_lattice(origin.0 + ax, oa), y: near_lattice(origin.1 + ay, ob) };
                let qb = QPoint { x: near_lattice(origin.0 + bx, oc), y: near_lattice(origin.1 + by, od) };
                let to_sim = |q: QPoint| SimVec2::new(SimScalar::from_raw(q.x << 16), SimScalar::from_raw(q.y << 16));
                let (a, bb) = (to_sim(qa), to_sim(qb));
                let any_edge = |test: &dyn Fn(QPoint, QPoint) -> bool| {
                    (0..n).any(|i| test(contour[i], contour[(i + 1) % n]))
                };
                let segment_oracle = box_contains_q(&p, b, qa, false)
                    || any_edge(&|u, v| segments_touch(qa, qb, u, v));
                proptest::prop_assert_eq!(p.segment_meets(a, bb), segment_oracle, "segment {:?} {:?}", qa, qb);
                let capsule_oracle = box_contains_q(&p, b, qa, true)
                    || any_edge(&|u, v| segments_within(qa, qb, u, v, r));
                proptest::prop_assert_eq!(p.capsule_meets(a, bb, radius), capsule_oracle, "capsule {:?} {:?}", qa, qb);
                // The open and closed point tests on their own.
                proptest::prop_assert_eq!(p.locate_q(qa) == Location::Inside, box_contains_q(&p, b, qa, true));
                proptest::prop_assert_eq!(p.locate_q(qa) != Location::Outside, box_contains_q(&p, b, qa, false));
            }
        }
    }

    /// The 5.0.14 geyser outline (void Footprint:18-19, plan §2.2 rule 3) in quarter cells.
    fn geyser_shape() -> FootprintShape {
        FootprintShape::Outline(
            sim_types::QuarterOutline::new(&[
                (-2, -4),
                (-3, -5),
                (-5, -3),
                (-4, -2),
                (-4, 2),
                (-5, 3),
                (-3, 5),
                (-2, 4),
                (2, 4),
                (3, 5),
                (5, 3),
                (4, 2),
                (4, -2),
                (5, -3),
                (3, -5),
                (2, -4),
            ])
            .expect("invariant: sixteen vertices fit"),
        )
    }

    #[test]
    fn the_geyser_outline_is_contained_exactly() {
        // A 3×3 at cells (2, 2) to (4, 4): centre (3.5, 3.5), lattice (14, 14).
        let footprint = StructureFootprint {
            width: 3,
            height: 3,
            shape: geyser_shape(),
        };
        footprint
            .validate()
            .expect("invariant: the geyser fits its 3x3");
        let p = PlacedFootprint::new((2, 2), footprint);
        assert_eq!(p.contour().len(), 16);
        assert!(
            signed_area_twice(p.contour()) > 0,
            "stored counter-clockwise"
        );
        // Lattice points times 2 (scale 2), relative to the centre (28, 28).
        let open = |dx: i64, dy: i64| p.contains_open_scaled(28 + dx, 28 + dy, 2);
        let closed = |dx: i64, dy: i64| {
            let q = |v: i64| SimScalar::from_raw(v << 29);
            let point = SimVec2::new(q(28 + dx), q(28 + dy));
            p.segment_meets(point, point)
        };
        // The centre is inside.
        assert!(open(0, 0));
        // Each recessed side lies at ±1 (8 doubled quarters) over its middle cell: the middle
        // cell's centre is on it, a sixteenth further in is inside, and the band between the
        // recess and the hull's line at ±1.25 is outside.
        for (sx, sy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
            assert!(
                !open(8 * sx, 8 * sy) && closed(8 * sx, 8 * sy),
                "recess ({sx}, {sy})"
            );
            assert!(open(7 * sx, 7 * sy), "inside the recess ({sx}, {sy})");
            assert!(!closed(9 * sx, 9 * sy), "outside the recess ({sx}, {sy})");
        }
        // The lobe tips (±0.75, ±1.25) and (±1.25, ±0.75) are on the boundary.
        for (dx, dy) in [
            (6, 10),
            (10, 6),
            (-6, 10),
            (-10, 6),
            (6, -10),
            (10, -6),
            (-6, -10),
            (-10, -6),
        ] {
            assert!(!open(dx, dy) && closed(dx, dy), "lobe tip ({dx}, {dy})");
        }
        // A lobe's diagonal passes through (±1, ±1); just inside it is in the shape.
        assert!(!open(8, 8) && closed(8, 8));
        assert!(open(7, 7));
        // The corners of the 3×3 cells are outside.
        assert!(!closed(12, 12) && !closed(-12, 12));
    }

    #[test]
    fn an_outline_must_be_simple_axis_or_diagonal_and_on_the_lattice() {
        let outline = |vertices: &[(i16, i16)]| {
            FootprintShape::Outline(
                sim_types::QuarterOutline::new(vertices).expect("invariant: at most sixteen"),
            )
        };
        let refused = [
            // Fewer than three vertices.
            outline(&[(0, 0), (4, 0)]),
            // An edge of slope 2.
            outline(&[(-2, -2), (2, -2), (0, 2)]),
            // A bow tie: two edges cross.
            outline(&[(-2, -2), (2, 2), (2, -2), (-2, 2)]),
            // A vertex outside the 2×2 cells (half extent 4 quarters).
            outline(&[(-4, -4), (5, -4), (5, 4), (-4, 4)]),
            // A zero-length edge (a repeated vertex).
            outline(&[(-2, -2), (2, -2), (2, -2), (2, 2), (-2, 2)]),
            // An edge folding back over its predecessor.
            outline(&[(-2, -2), (2, -2), (0, -2), (0, 2)]),
            // All vertices on one line.
            outline(&[(-2, 0), (0, 0), (2, 0)]),
            // Two non-adjacent edges touching at a vertex (a pinched figure eight).
            outline(&[
                (0, 0),
                (2, -2),
                (4, 0),
                (2, 2),
                (0, 0),
                (-2, 2),
                (-4, 0),
                (-2, -2),
            ]),
        ];
        let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a positive side is a valid map size");
        for s in refused {
            let footprint = StructureFootprint {
                width: 2,
                height: 2,
                shape: s,
            };
            assert_eq!(
                map.place_structure(EntityId(3), (4, 4), footprint),
                Err(MapError::InvalidFootprintShape {
                    width: 2,
                    height: 2,
                    shape: s
                }),
                "{s:?}"
            );
        }
        assert!(map.structures().is_empty());
        // Every vertex is in quarter cells from a lattice centre, so a placed outline is always
        // on the lattice; a clockwise square and the geyser are accepted.
        let clockwise = outline(&[(-4, -4), (-4, 4), (4, 4), (4, -4)]);
        let footprint = StructureFootprint {
            width: 2,
            height: 2,
            shape: clockwise,
        };
        assert!(map.place_structure(EntityId(3), (4, 4), footprint).is_ok());
        let geyser = StructureFootprint {
            width: 3,
            height: 3,
            shape: geyser_shape(),
        };
        assert!(map.place_structure(EntityId(4), (8, 8), geyser).is_ok());
        let placed = map
            .structures()
            .get(&EntityId(3))
            .expect("invariant: placed");
        assert!(signed_area_twice(placed.footprint().contour()) > 0);
    }
}
