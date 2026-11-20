//! The pathing shape of a footprint (plans `2026-11-18-contour-footprints` D-2 and
//! `2026-11-20-resource-footprints` D-2).
//!
//! # Responsibility
//! Owns [`FootprintShape`], the resolved form of `StarCraft` II 5.0.14's footprint `Shape`, in two
//! forms: a box centred on the footprint cells with its four corners cut at 45° by one chamfer
//! (every structure), and an outline of up to [`QUARTER_OUTLINE_CAPACITY`] vertices (the Vespene
//! geyser's 16-vertex Legacy of the Void shape). Its values are integers in quarter cells, so
//! every vertex of a placed shape lies on the navigation mesh's quarter-cell lattice and every rule
//! about it is exact.
//!
//! # How it fits
//! The catalog authors each structure's shape and each resource kind's shape in world units
//! (`footprint_shape` in `structures.ron`, `shape` in `placement_rules.ron`) and converts it here
//! at load. `ShapeKind::BuildingDome` carries a structure's, and `sim_map`'s `PlacedStructure`
//! keeps it for the structure's lifetime: the navigation mesh, the clearance field and the
//! ground-movement rules read it, while placement keeps reading the footprint's cells. This type
//! has no geometry methods beyond its extents; `sim_map` (`footprint.rs`) places every shape as
//! one outline on its lattice and owns the exact predicates.
//!
//! # Invariants
//! - The type itself only stores quarter-cell counts. Whether a shape fits its cells (a box:
//!   `0 < half_width ≤ w / 2`, `0 < half_height ≤ h / 2`, `chamfer < min(half extents)`; an
//!   outline: simple, 3 to 16 vertices, every edge along an axis or a diagonal, inside the cells)
//!   is checked where the cells are known: by the catalog loader and by `sim_map`'s placement.
//! - A [`QuarterOutline`] stores its vertices in a fixed array with a count; the slots past the
//!   count are always `(0, 0)`, so derived equality and hashing see only the used vertices.

use serde::{Deserialize, Serialize};

use crate::math::SimScalar;

/// Quarter cells per cell: the unit of a [`FootprintShape`]'s values.
pub const FOOTPRINT_QUARTERS_PER_CELL: u16 = 4;

/// The most vertices a [`QuarterOutline`] holds: the 5.0.14 geyser's 16.
pub const QUARTER_OUTLINE_CAPACITY: usize = 16;

/// A footprint's pathing shape, centred on its footprint cells, in quarter cells (1/4 world
/// unit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FootprintShape {
    /// A box of half-extents `half_width_q` by `half_height_q` with every corner cut at 45° by
    /// `chamfer_q` along each side. A chamfer of 0 is the full box. The 5.0.14 3×3 contour, for
    /// example, is `(6, 6, 2)`: half extents 1.5 and corners cut by 0.5.
    Box {
        /// Half the shape's east-west extent, in quarter cells.
        half_width_q: u16,
        /// Half the shape's north-south extent, in quarter cells.
        half_height_q: u16,
        /// How far each corner is cut along each of its two sides, in quarter cells.
        chamfer_q: u16,
    },
    /// A closed polygon given by its vertices in quarter cells from the centre of the cells, in
    /// order round the outline (either orientation).
    Outline(QuarterOutline),
}

/// Up to [`QUARTER_OUTLINE_CAPACITY`] vertices `(x, y)` in quarter cells, stored inline with a
/// count (`Copy`, no allocation). It serialises as its list of used vertices, and a list longer
/// than the capacity is refused when read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Vec<(i16, i16)>", into = "Vec<(i16, i16)>")]
pub struct QuarterOutline {
    /// The vertices; slots at or past `count` are `(0, 0)`.
    vertices: [(i16, i16); QUARTER_OUTLINE_CAPACITY],
    /// How many slots are used.
    count: u8,
}

impl QuarterOutline {
    /// The outline through `vertices`, in quarter cells from the shape's centre; `None` when
    /// there are more than [`QUARTER_OUTLINE_CAPACITY`]. Whether they form a valid shape is
    /// checked at placement (module invariants).
    #[must_use]
    pub fn new(vertices: &[(i16, i16)]) -> Option<Self> {
        if vertices.len() > QUARTER_OUTLINE_CAPACITY {
            return None;
        }
        let mut stored = [(0, 0); QUARTER_OUTLINE_CAPACITY];
        stored[..vertices.len()].copy_from_slice(vertices);
        Some(Self {
            vertices: stored,
            count: u8::try_from(vertices.len()).ok()?,
        })
    }

    /// The used vertices, in quarter cells from the shape's centre.
    #[must_use]
    pub fn vertices(&self) -> &[(i16, i16)] {
        &self.vertices[..usize::from(self.count)]
    }
}

impl TryFrom<Vec<(i16, i16)>> for QuarterOutline {
    type Error = String;

    fn try_from(vertices: Vec<(i16, i16)>) -> Result<Self, Self::Error> {
        Self::new(&vertices).ok_or_else(|| {
            format!(
                "an outline holds at most {QUARTER_OUTLINE_CAPACITY} vertices, not {}",
                vertices.len()
            )
        })
    }
}

impl From<QuarterOutline> for Vec<(i16, i16)> {
    fn from(outline: QuarterOutline) -> Self {
        outline.vertices().to_vec()
    }
}

impl FootprintShape {
    /// The full box of a `width` by `height` footprint in cells: the square shape every
    /// structure had before footprint contours, with no chamfer.
    #[must_use]
    pub const fn full(width: u8, height: u8) -> Self {
        Self::Box {
            half_width_q: width as u16 * (FOOTPRINT_QUARTERS_PER_CELL / 2),
            half_height_q: height as u16 * (FOOTPRINT_QUARTERS_PER_CELL / 2),
            chamfer_q: 0,
        }
    }

    /// A box's half width, half height and chamfer in quarter cells; `None` for an outline.
    #[must_use]
    pub const fn box_extents(self) -> Option<(u16, u16, u16)> {
        match self {
            Self::Box {
                half_width_q,
                half_height_q,
                chamfer_q,
            } => Some((half_width_q, half_height_q, chamfer_q)),
            Self::Outline(_) => None,
        }
    }

    /// The distance from the centre to the shape's boundary along the nearer axis, in world
    /// units: for a box, its smaller half side, the radius of the largest disc centred on the
    /// footprint that touches both of its narrower faces (the drawn dome of a structure has this
    /// radius, contour plan D-11). For an outline, the smallest distance at which one of the four
    /// axis rays from the centre meets an edge; edges along an axis or a diagonal (the placed
    /// invariant) cross an axis on the quarter lattice, so the value is exact. An outline that
    /// no axis ray meets has extent 0.
    #[must_use]
    pub fn inscribed_half_extent(self) -> SimScalar {
        let quarters = match self {
            Self::Box {
                half_width_q,
                half_height_q,
                ..
            } => i64::from(half_width_q.min(half_height_q)),
            Self::Outline(outline) => outline_axis_extent(&outline),
        };
        SimScalar::from_raw(quarters << 30)
    }
}

/// The nearest crossing of an axis ray from the origin with an edge of `outline`, in quarter
/// cells, truncated toward zero for an edge that is neither axis-aligned nor diagonal.
fn outline_axis_extent(outline: &QuarterOutline) -> i64 {
    let vertices = outline.vertices();
    let mut best: Option<i64> = None;
    for (i, &(x1, y1)) in vertices.iter().enumerate() {
        let (x2, y2) = vertices[(i + 1) % vertices.len()];
        let (x1, y1, x2, y2) = (i64::from(x1), i64::from(y1), i64::from(x2), i64::from(y2));
        // The x axis (y = 0) and the y axis (x = 0), each by swapping the coordinates.
        for (a1, b1, a2, b2) in [(x1, y1, x2, y2), (y1, x1, y2, x2)] {
            if (b1 <= 0 && b2 >= 0 || b1 >= 0 && b2 <= 0) && b1 != b2 {
                let a = a1 + (a2 - a1) * (-b1) / (b2 - b1);
                let distance = a.abs();
                best = Some(best.map_or(distance, |d| d.min(distance)));
            }
        }
    }
    best.unwrap_or(0)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a broken shape law must fail the test loudly"
)]
mod tests {
    use super::*;

    #[test]
    fn a_full_shape_is_half_its_cells_without_a_chamfer() {
        let shape = FootprintShape::full(5, 3);
        assert_eq!(
            shape,
            FootprintShape::Box {
                half_width_q: 10,
                half_height_q: 6,
                chamfer_q: 0
            }
        );
        assert_eq!(shape.inscribed_half_extent(), SimScalar::from_raw(3 << 31));
        assert_eq!(shape.box_extents(), Some((10, 6, 0)));
    }

    #[test]
    fn the_inscribed_half_extent_is_the_smaller_half_side() {
        let spore = FootprintShape::Box {
            half_width_q: 2,
            half_height_q: 2,
            chamfer_q: 1,
        };
        assert_eq!(spore.inscribed_half_extent(), SimScalar::HALF);
        let oblong = FootprintShape::Box {
            half_width_q: 6,
            half_height_q: 4,
            chamfer_q: 2,
        };
        assert_eq!(oblong.inscribed_half_extent(), SimScalar::ONE);
    }

    /// The 5.0.14 geyser outline (void Footprint:18-19) in quarter cells.
    fn geyser() -> QuarterOutline {
        QuarterOutline::new(&[
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
        .expect("invariant: sixteen vertices fit")
    }

    #[test]
    fn an_outline_keeps_its_vertices_and_refuses_too_many() {
        let outline = geyser();
        assert_eq!(outline.vertices().len(), 16);
        assert_eq!(outline.vertices()[1], (-3, -5));
        assert!(QuarterOutline::new(&[(0, 0); 17]).is_none());
        assert_eq!(FootprintShape::Outline(outline).box_extents(), None);
        let text = ron::to_string(&FootprintShape::Outline(outline))
            .expect("invariant: an outline serialises");
        let back: FootprintShape = ron::from_str(&text).expect("invariant: it reads back");
        assert_eq!(back, FootprintShape::Outline(outline));
        let long = format!("Outline([{}])", ["(0, 0)"; 17].join(", "));
        assert!(ron::from_str::<FootprintShape>(&long).is_err());
        let triangle =
            QuarterOutline::new(&[(0, 0), (4, 0), (0, 4)]).expect("invariant: three fit");
        assert_eq!(triangle.vertices(), &[(0, 0), (4, 0), (0, 4)]);
    }

    #[test]
    fn the_geyser_outline_meets_its_axes_at_its_recessed_sides() {
        // Each side is recessed to ±1 over its middle cell: the axis rays meet it at 1.0.
        assert_eq!(
            FootprintShape::Outline(geyser()).inscribed_half_extent(),
            SimScalar::ONE
        );
    }

    #[test]
    fn an_outline_box_has_the_extent_of_the_box() {
        let outline = QuarterOutline::new(&[(6, -4), (6, 4), (-6, 4), (-6, -4)])
            .expect("invariant: four fit");
        assert_eq!(
            FootprintShape::Outline(outline).inscribed_half_extent(),
            FootprintShape::Box {
                half_width_q: 6,
                half_height_q: 4,
                chamfer_q: 0
            }
            .inscribed_half_extent()
        );
    }
}
