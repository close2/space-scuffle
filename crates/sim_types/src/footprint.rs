//! The pathing shape of a structure's footprint (plan `2026-11-18-contour-footprints` D-2).
//!
//! # Responsibility
//! Owns [`FootprintShape`], the resolved form of `StarCraft` II 5.0.14's footprint `Shape`: a box
//! centred on the structure's footprint cells with its four corners cut at 45° by one chamfer.
//! Its values are integers in quarter cells, so every vertex of a placed shape lies on the
//! navigation mesh's quarter-cell lattice and every rule about it is exact.
//!
//! # How it fits
//! The catalog authors each structure's shape in world units (`footprint_shape` in
//! `structures.ron`) and converts it here at load. `ShapeKind::BuildingDome` carries it, and
//! `sim_map`'s `PlacedStructure` keeps it for the structure's lifetime: the navigation mesh, the
//! clearance field and the ground-movement rules read it, while placement keeps reading the
//! footprint's cells. This type has no geometry methods beyond its extents; the exact predicates
//! live in `sim_map` (`footprint.rs`), next to the lattice they use.
//!
//! # Invariants
//! - The type itself only stores three quarter-cell counts. Whether a shape fits its cells
//!   (`0 < half_width ≤ w / 2`, `0 < half_height ≤ h / 2`, `chamfer < min(half extents)`) is
//!   checked where the cells are known: by the catalog loader and by `sim_map`'s placement.

use serde::{Deserialize, Serialize};

use crate::math::SimScalar;

/// Quarter cells per cell: the unit of a [`FootprintShape`]'s values.
pub const FOOTPRINT_QUARTERS_PER_CELL: u16 = 4;

/// A structure's pathing shape: a box of half-extents `half_width_q` by `half_height_q`, centred
/// on its footprint cells, with every corner cut at 45° by `chamfer_q` along each side. All
/// three values are in quarter cells (1/4 world unit).
///
/// A chamfer of 0 is the full box. The 5.0.14 3×3 contour, for example, is `(6, 6, 2)`: half
/// extents 1.5 and corners cut by 0.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FootprintShape {
    /// Half the shape's east-west extent, in quarter cells.
    pub half_width_q: u16,
    /// Half the shape's north-south extent, in quarter cells.
    pub half_height_q: u16,
    /// How far each corner is cut along each of its two sides, in quarter cells.
    pub chamfer_q: u16,
}

impl FootprintShape {
    /// The full box of a `width` by `height` footprint in cells: the square shape every
    /// structure had before footprint contours, with no chamfer.
    #[must_use]
    pub const fn full(width: u8, height: u8) -> Self {
        Self {
            half_width_q: width as u16 * (FOOTPRINT_QUARTERS_PER_CELL / 2),
            half_height_q: height as u16 * (FOOTPRINT_QUARTERS_PER_CELL / 2),
            chamfer_q: 0,
        }
    }

    /// The half-extent of the shape's smaller side, in world units: the radius of the largest
    /// disc centred on the footprint that touches both of its narrower faces. The drawn dome of
    /// a structure has this radius (plan D-11).
    #[must_use]
    pub fn inscribed_half_extent(self) -> SimScalar {
        let quarters = self.half_width_q.min(self.half_height_q);
        SimScalar::from_raw(i64::from(quarters) << 30)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_shape_is_half_its_cells_without_a_chamfer() {
        let shape = FootprintShape::full(5, 3);
        assert_eq!(shape.half_width_q, 10);
        assert_eq!(shape.half_height_q, 6);
        assert_eq!(shape.chamfer_q, 0);
        assert_eq!(shape.inscribed_half_extent(), SimScalar::from_raw(3 << 31));
    }

    #[test]
    fn the_inscribed_half_extent_is_the_smaller_half_side() {
        let spore = FootprintShape {
            half_width_q: 2,
            half_height_q: 2,
            chamfer_q: 1,
        };
        assert_eq!(spore.inscribed_half_extent(), SimScalar::HALF);
        let oblong = FootprintShape {
            half_width_q: 6,
            half_height_q: 4,
            chamfer_q: 2,
        };
        assert_eq!(oblong.inscribed_half_extent(), SimScalar::ONE);
    }
}
