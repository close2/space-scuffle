//! Stages a ground unit tangent to a straight cliff wall of the shipped battle map, for the
//! cliff-contact GPU test (plan `2026-11-30-terrain-and-air-indicators` §5.2, G-1).
//!
//! # Responsibility
//! - [`Wall::find`]: a straight cliff edge `x = c` of the map between two ground plateaus one
//!   level apart, straight for [`FAR_ROWS`] rows beyond the contact row and [`NEAR_ROWS`] rows on
//!   the camera's side (+Y, towards the RTS camera's eye), eight cells in all, with flat low
//!   ground [`LOW_ACROSS`] cells across on those rows and no ramp, structure or resource within
//!   [`CLEARANCE_CELLS`] of them. These are the rows a camera at `MIN_CAMERA_DISTANCE` sees
//!   round the unit; the shipped battle map has no longer straight wall clear of ramps.
//! - [`contact_unit`] and [`brink_unit`]: the archetypes the stage writes, found by catalog
//!   property and media present, never by slug (charter C5).
//! - [`Wall::low_contact`] and [`Wall::high_brink`]: where a unit of a given radius stands
//!   tangent to the wall, on the low side or at the high side's brink.
//!
//! # How tests use it
//! Include it with `#[path = "support/cliff_contact_stage.rs"]`. The stage only reads the map
//! and the catalog; the test writes the unit's position itself, as the other stages do. The
//! simulation holds bodies at clearance 0 from a wall (evidence
//! `docs/evidence/2026-11-29-wall-grazing.md` case B), so a unit tangent to the wall is a state
//! the simulation itself produces.

#![allow(
    clippy::expect_used,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use std::path::Path;

use sim_core::SimWorld;
use sim_map::grid::{CellFlags, GameMap, TerrainKind};
use sim_types::{MoverType, ShapeKind, SimScalar, SimVec2, UnitTypeId};

/// Rows of the straight wall beyond the contact row (towards -Y, away from the RTS camera's eye):
/// at `MIN_CAMERA_DISTANCE` and the default pitch, the top of the canvas shows the ground about
/// five cells beyond the camera's target.
pub const FAR_ROWS: u32 = 5;

/// Rows of the straight wall on the camera's side of the contact row (towards +Y): the eye stands
/// about 1.6 cells beyond the target at `MIN_CAMERA_DISTANCE`, so nothing farther can be in front
/// of the unit.
pub const NEAR_ROWS: u32 = 2;

/// Cells around the staged cells that must hold no ramp, structure or resource.
pub const CLEARANCE_CELLS: u32 = 2;

/// Flat low ground across the wall, in cells.
pub const LOW_ACROSS: u32 = 4;

/// Cells of the high plateau beside the wall that must be flat.
const HIGH_ACROSS: u32 = 2;

/// A straight cliff edge along Y.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wall {
    /// The edge's X coordinate, world units: the boundary between cells `c - 1` and `c`.
    pub c: u32,
    /// The contact row: the unit stands at `y0 + 0.5`.
    pub y0: u32,
    /// `+1` when the low side is `x > c`, `-1` when it is `x < c`.
    pub low_side: i32,
    /// The low and high plateau levels.
    pub low_level: u8,
    pub high_level: u8,
}

/// The plateau level of cell `(x, y)` when it is plain ground: in bounds, not abyss, not a ramp,
/// with no structure or resource on it.
fn plain_level(map: &GameMap, x: i64, y: i64) -> Option<u8> {
    let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
    let cell = map.cell(x, y).ok()?;
    let plain = cell.terrain_kind != TerrainKind::Abyss
        && map.ramp_profile(x, y).is_none()
        && !cell
            .flags
            .intersects(CellFlags::OCCUPIED_BY_STRUCTURE | CellFlags::OCCUPIED_BY_RESOURCE);
    plain.then_some(cell.cliff_level.0)
}

impl Wall {
    /// The first wall of `map`, scanning edges by X then rows by Y.
    pub fn find(map: &GameMap) -> Self {
        for c in 1..map.width() {
            for y0 in FAR_ROWS..map.height() {
                for low_side in [1, -1] {
                    if let Some(wall) = Self::at(map, c, y0, low_side) {
                        return wall;
                    }
                }
            }
        }
        panic!("the shipped battle map has a straight cliff wall with flat ground beside it");
    }

    /// The wall at edge `c`, contact row `y0` and low side `low_side`, if the map has one there.
    fn at(map: &GameMap, c: u32, y0: u32, low_side: i32) -> Option<Self> {
        let s = i64::from(low_side);
        // Cell column `k` cells into the low side (k >= 0) or the high side (k < 0).
        let column = |k: i64| {
            if s > 0 {
                i64::from(c) + k
            } else {
                i64::from(c) - 1 - k
            }
        };
        let first_row = i64::from(y0) - i64::from(FAR_ROWS);
        let last_row = i64::from(y0) + i64::from(NEAR_ROWS);
        let low_level = plain_level(map, column(0), i64::from(y0))?;
        let high_level = low_level.checked_add(1)?;
        for y in first_row..=last_row {
            for k in 0..i64::from(LOW_ACROSS) {
                (plain_level(map, column(k), y)? == low_level).then_some(())?;
            }
            for k in 1..=i64::from(HIGH_ACROSS) {
                (plain_level(map, column(-k), y)? == high_level).then_some(())?;
            }
        }
        // No ramp, structure or resource within the clearance of the staged cells.
        let margin = i64::from(CLEARANCE_CELLS);
        let (x_low, x_high) = (
            column(i64::from(LOW_ACROSS) - 1),
            column(-i64::from(HIGH_ACROSS)),
        );
        let (x0, x1) = (x_low.min(x_high) - margin, x_low.max(x_high) + margin);
        for y in first_row - margin..=last_row + margin {
            for x in x0..=x1 {
                // Beyond the map's border there is nothing to clear.
                let (Ok(ux), Ok(uy)) = (u32::try_from(x), u32::try_from(y)) else {
                    continue;
                };
                let Ok(cell) = map.cell(ux, uy) else {
                    continue;
                };
                if map.ramp_profile(ux, uy).is_some()
                    || cell.flags.intersects(
                        CellFlags::OCCUPIED_BY_STRUCTURE | CellFlags::OCCUPIED_BY_RESOURCE,
                    )
                {
                    return None;
                }
            }
        }
        Some(Self {
            c,
            y0,
            low_side,
            low_level,
            high_level,
        })
    }

    /// Where a unit of terrain radius `radius` stands tangent to the wall on the low side.
    pub fn low_contact(self, radius: SimScalar) -> SimVec2 {
        self.beside(radius, self.low_side)
    }

    /// Where a unit of terrain radius `radius` stands tangent to the wall at the high side's
    /// brink.
    pub fn high_brink(self, radius: SimScalar) -> SimVec2 {
        self.beside(radius, -self.low_side)
    }

    fn beside(self, radius: SimScalar, side: i32) -> SimVec2 {
        let edge = SimScalar::from_num(self.c);
        let x = if side > 0 {
            edge + radius
        } else {
            edge - radius
        };
        SimVec2::new(x, SimScalar::from_num(self.y0) + SimScalar::HALF)
    }
}

/// Whether the client ships the media of unit `slug`.
fn presentable(assets: &Path, slug: &str) -> bool {
    assets
        .join("textures")
        .join(format!("{slug}.flipbook.ron"))
        .is_file()
}

/// The mobile ground unit spheres with media in `world`'s catalog.
fn ground_units<'w>(
    world: &'w SimWorld,
    assets: &'w Path,
) -> impl Iterator<Item = &'w sim_core::catalog::UnitArchetype> + 'w {
    world.catalog.archetypes().filter(move |a| {
        a.mover_type == MoverType::Ground
            && a.selectable
            && a.max_speed > SimScalar::ZERO
            && matches!(a.shape, ShapeKind::UnitSphere)
            && presentable(assets, &a.slug)
    })
}

/// The ground unit with the smallest drawn radius among those whose drawn radius equals their
/// terrain radius, so the simulation holds its drawn sphere exactly tangent to a wall.
pub fn contact_unit(world: &SimWorld, assets: &Path) -> UnitTypeId {
    ground_units(world, assets)
        .filter(|a| a.drawn_radius() == a.terrain_radius)
        .min_by_key(|a| (a.drawn_radius(), a.unit_type_id))
        .expect("the catalog ships a ground unit drawn at its terrain radius, with media")
        .unit_type_id
}

/// The ground unit with the smallest terrain radius: the one that stands closest to a cliff's
/// brink on the high side (plan D-4).
pub fn brink_unit(world: &SimWorld, assets: &Path) -> UnitTypeId {
    ground_units(world, assets)
        .min_by_key(|a| (a.terrain_radius, a.unit_type_id))
        .expect("the catalog ships a ground unit with media")
        .unit_type_id
}
