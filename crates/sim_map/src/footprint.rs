//! Exact geometry of a placed structure's footprint shape (plan `2026-11-18-contour-footprints`
//! D-2, D-6, D-7).
//!
//! # Responsibility
//! A structure has two layers, as in `StarCraft` II 5.0.14: its footprint **cells** (the game's
//! `Place` layer), which placement, construction and knowledge read, and its **shape** (the
//! game's `Shape`), a box centred on the cells with 45° chamfered corners
//! ([`sim_types::FootprintShape`]), which pathing reads. This module owns the shape's geometry
//! once placed: its contour on the navigation mesh's quarter-cell lattice, and the exact
//! containment, segment and capsule predicates the mesh labels, the clearance raster and the
//! ground-movement rules use. [`StructureFootprint`] is what a caller places: the cells and the
//! shape together.
//!
//! # How it fits
//! `GameMap::place_structure` validates a [`StructureFootprint`] and records the shape on the
//! `PlacedStructure`. The navigation mesh cuts the contour (`navmesh/mod.rs`), the clearance field
//! rasterises it (`pathfinding.rs`), and `sim_core`'s centre rule and unit placement test segments
//! and discs against it.
//!
//! # Invariants
//! - The shape lies in its cells and is centred on them: its centre is
//!   `(4·left + 2·width, 4·bottom + 2·height)` in quarter cells, always a lattice point, so every
//!   contour vertex is a lattice point.
//! - Every predicate is exact integer arithmetic: containment on lattice coordinates scaled by an
//!   integer, segments and capsules on the Q16 grid of the mesh's own segment predicate (`i128`,
//!   `navmesh/predicates.rs`), so placement, routes and contact agree on the shape to that grid.

use sim_types::{FootprintShape, SimScalar, SimVec2};

use crate::error::{MapError, Result};
use crate::grid::{GameMap, PlacedStructure};
use crate::navmesh::predicates::{
    segments_touch, segments_within, LatticePoint, QPoint, LATTICE_BITS, Q_BITS,
};
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

    /// Checks that the shape lies in its cells and keeps every face: `0 < half_width ≤ width/2`,
    /// `0 < half_height ≤ height/2` and `chamfer < min(half_width, half_height)`.
    ///
    /// # Errors
    /// [`MapError::InvalidFootprintShape`] when it does not.
    pub fn validate(self) -> Result<()> {
        let FootprintShape {
            half_width_q,
            half_height_q,
            chamfer_q,
        } = self.shape;
        let fits = half_width_q > 0
            && half_height_q > 0
            && half_width_q <= 2 * u16::from(self.width)
            && half_height_q <= 2 * u16::from(self.height)
            && chamfer_q < half_width_q.min(half_height_q);
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

/// Q16 units per lattice unit.
const Q_PER_LATTICE: u32 = Q_BITS - LATTICE_BITS;

impl PlacedStructure {
    /// The centre of the shape in lattice units (quarter cells): the centre of the cells.
    #[must_use]
    pub fn shape_centre(&self) -> LatticePoint {
        let axis = |low: u32, size: u8| {
            i32::try_from(4 * i64::from(low) + 2 * i64::from(size)).unwrap_or(i32::MAX)
        };
        LatticePoint::new(
            axis(self.top_left.0, self.width),
            axis(self.top_left.1, self.height),
        )
    }

    /// The shape's vertices, counter-clockwise from the east end of its south face, and how many
    /// of the eight slots are used: 8 for a chamfered shape, 4 for a full box.
    #[must_use]
    pub fn contour(&self) -> ([LatticePoint; 8], usize) {
        let centre = self.shape_centre();
        let (hw, hh, c) = (
            i32::from(self.shape.half_width_q),
            i32::from(self.shape.half_height_q),
            i32::from(self.shape.chamfer_q),
        );
        let at = |dx: i32, dy: i32| LatticePoint::new(centre.x + dx, centre.y + dy);
        if c == 0 {
            let corners = [at(hw, -hh), at(hw, hh), at(-hw, hh), at(-hw, -hh)];
            return (
                [
                    corners[0], corners[1], corners[2], corners[3], corners[0], corners[1],
                    corners[2], corners[3],
                ],
                4,
            );
        }
        (
            [
                at(hw - c, -hh),
                at(hw, -hh + c),
                at(hw, hh - c),
                at(hw - c, hh),
                at(-hw + c, hh),
                at(-hw, hh - c),
                at(-hw, -hh + c),
                at(-hw + c, -hh),
            ],
            8,
        )
    }

    /// Whether the point `(x, y) / scale`, in lattice units, lies in the shape's open interior.
    /// Exact for any positive integer `scale`: the mesh labels pass a triangle's vertex sum with
    /// scale 3 (its centroid).
    #[must_use]
    pub fn contains_open_scaled(&self, x: i64, y: i64, scale: i64) -> bool {
        let centre = self.shape_centre();
        let dx = (x - scale * i64::from(centre.x)).abs();
        let dy = (y - scale * i64::from(centre.y)).abs();
        let (hw, hh, c) = self.extents();
        dx < scale * hw && dy < scale * hh && dx + dy < scale * (hw + hh - c)
    }

    /// Whether the open half-cell whose south-west corner is the half-cell lattice point
    /// `(hx, hy)` (world `(hx/2, hy/2)`) meets the shape's open interior. Two convex polygons'
    /// interiors meet iff no edge direction of either separates them, so four exact interval
    /// tests decide it: east, north and the two diagonals.
    #[must_use]
    pub fn meets_open_half_cell(&self, hx: i64, hy: i64) -> bool {
        let centre = self.shape_centre();
        let (cx, cy) = (i64::from(centre.x), i64::from(centre.y));
        let (hw, hh, c) = self.extents();
        let reach = hw + hh - c;
        // The half-cell in lattice units: [x0, x0 + 2] by [y0, y0 + 2].
        let (x0, y0) = (2 * hx, 2 * hy);
        let overlaps =
            |low_a: i64, high_a: i64, low_b: i64, high_b: i64| low_a < high_b && low_b < high_a;
        overlaps(x0, x0 + 2, cx - hw, cx + hw)
            && overlaps(y0, y0 + 2, cy - hh, cy + hh)
            && overlaps(x0 + y0, x0 + y0 + 4, cx + cy - reach, cx + cy + reach)
            && overlaps(x0 - y0 - 2, x0 - y0 + 2, cx - cy - reach, cx - cy + reach)
    }

    /// Whether the closed segment `[a, b]` (world units, reduced to the Q16 grid) meets the
    /// closed shape: an end lies in it, or the segment touches its boundary.
    #[must_use]
    pub fn segment_meets(&self, a: SimVec2, b: SimVec2) -> bool {
        let (qa, qb) = (QPoint::from_sim(a), QPoint::from_sim(b));
        self.contains_q(qa, false) || self.any_edge(|u, v| segments_touch(qa, qb, u, v))
    }

    /// Whether a disc of `radius` world units swept along `[a, b]` meets the shape's open
    /// interior: an end lies inside, or the segment comes closer to the boundary than the
    /// radius, rounded up to the Q16 grid as the mesh's segment predicate rounds it. Points on
    /// the Q16 grid and radii in multiples of 2⁻¹⁶ are decided exactly. `radius` is positive.
    #[must_use]
    pub fn capsule_meets(&self, a: SimVec2, b: SimVec2, radius: SimScalar) -> bool {
        let (qa, qb) = (QPoint::from_sim(a), QPoint::from_sim(b));
        let r = radius_q(radius);
        self.contains_q(qa, true) || self.any_edge(|u, v| segments_within(qa, qb, u, v, r))
    }

    /// The shape's half-width, half-height and chamfer in lattice units.
    fn extents(&self) -> (i64, i64, i64) {
        (
            i64::from(self.shape.half_width_q),
            i64::from(self.shape.half_height_q),
            i64::from(self.shape.chamfer_q),
        )
    }

    /// Whether the Q16 point `p` lies in the shape: strictly inside when `open`, else inside or
    /// on the boundary.
    fn contains_q(&self, p: QPoint, open: bool) -> bool {
        let centre = self.shape_centre().to_q();
        let (hw, hh, c) = self.extents();
        let (hw, hh, reach) = (
            hw << Q_PER_LATTICE,
            hh << Q_PER_LATTICE,
            (hw + hh - c) << Q_PER_LATTICE,
        );
        let (dx, dy) = ((p.x - centre.x).abs(), (p.y - centre.y).abs());
        if open {
            dx < hw && dy < hh && dx + dy < reach
        } else {
            dx <= hw && dy <= hh && dx + dy <= reach
        }
    }

    /// Whether `test` holds for some edge of the contour, each edge as a pair of Q16 points.
    fn any_edge(&self, test: impl Fn(QPoint, QPoint) -> bool) -> bool {
        let (vertices, count) = self.contour();
        (0..count).any(|i| test(vertices[i].to_q(), vertices[(i + 1) % count].to_q()))
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
    reason = "a broken geometry law must fail the test loudly"
)]
mod tests {
    use super::*;
    use crate::grid::{CliffLevel, GameMap, MapCell};
    use sim_types::EntityId;

    fn shape(hw: u16, hh: u16, c: u16) -> FootprintShape {
        FootprintShape {
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
        let (vertices, count) = p.contour();
        assert_eq!(count, 4);
        assert_eq!(
            vertices[..4],
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
            let (vertices, count) = p.contour();
            let got: Vec<(i32, i32)> = vertices[..count].iter().map(|v| (v.x, v.y)).collect();
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
}
