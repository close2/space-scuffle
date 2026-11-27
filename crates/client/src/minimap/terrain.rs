//! The minimap's per-cell terrain texture (plan `2026-12-01-minimap` D-3).
//!
//! # Responsibility
//! Encodes the map, one RGBA8 texel per cell in row-major order (row `y` of the texture is map
//! row `y`, so the texture's top row is the map's `y = 0`, the minimap's top):
//! - **R**: the ground's height at the cell's centre over the height of the highest cliff level,
//!   so a flat cell of level `k` holds `k / 3` and a ramp cell lies between its two landings;
//! - **G**: 255 when ground units may walk the cell (`GROUND_PATHABLE`) and it is not the abyss;
//! - **B**: 255 on the high side of a cliff edge: a flat cell with a flat 4-neighbour on a lower
//!   cliff level. Ramps join levels without a cliff, so a ramp and its landings mark no edge;
//! - **A**: 255.
//!
//! The shader reads these texels with `textureLoad`, one per cell, so edges stay sharp at any
//! minimap size. The ground's colour comes from the terrain's own splat texture and albedo
//! arrays, not from here.
//!
//! # Invariants
//! - The buffer is cleared and refilled in place: a rebuild at the same map size allocates
//!   nothing.
//! - The encoding reads only presentation-side map queries; it decides nothing of the simulation.

use sim_map::grid::{CellFlags, CliffLevel, GameMap, TerrainKind};
use sim_types::{SimScalar, SimVec2};

/// Bytes per texel of the cell texture (`Rgba8Unorm`).
pub const CELL_TEXEL_BYTES: usize = 4;

/// The 4-neighbourhood, as `(dx, dy)` cell offsets.
const NEIGHBOURS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Fills `out` with the cell texture of `map`: `width × height` texels of
/// [`CELL_TEXEL_BYTES`] each, row-major.
///
// PERFORMANCE RATIONALE:
// Why: The encoding runs once per battle (and once per terrain edit), not per frame, but it is
// part of the minimap's retained state, which the charter keeps allocation-free in steady state.
// What: The caller's buffer is cleared and refilled, so the same map size reuses its storage.
// Trade-off: The caller keeps one byte vector alive per minimap (64 KiB at the largest map).
pub fn write_cell_texels(map: &GameMap, out: &mut Vec<u8>) {
    out.clear();
    let top = CliffLevel::MAX.height_f32();
    for y in 0..map.height() {
        for x in 0..map.width() {
            out.extend_from_slice(&cell_texel(map, x, y, top));
        }
    }
}

/// The texel of cell `(x, y)`; `top` is the height of the highest cliff level, in world units.
fn cell_texel(map: &GameMap, x: u32, y: u32, top: f32) -> [u8; CELL_TEXEL_BYTES] {
    let Ok(cell) = map.cell(x, y) else {
        return [0, 0, 0, u8::MAX];
    };
    let centre = SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    );
    let height = (map.terrain_altitude_at(centre) / top).clamp(0.0, 1.0);
    let walkable =
        cell.flags.contains(CellFlags::GROUND_PATHABLE) && cell.terrain_kind != TerrainKind::Abyss;
    [
        unit_byte(height),
        if walkable { u8::MAX } else { 0 },
        if is_high_edge(map, x, y) { u8::MAX } else { 0 },
        u8::MAX,
    ]
}

/// Whether flat cell `(x, y)` has a flat 4-neighbour on a lower cliff level.
#[must_use]
pub fn is_high_edge(map: &GameMap, x: u32, y: u32) -> bool {
    let Ok(cell) = map.cell(x, y) else {
        return false;
    };
    if cell.ramp.is_some() {
        return false;
    }
    NEIGHBOURS.iter().any(|&offset| {
        map.offset_cell((x, y), offset)
            .and_then(|(nx, ny)| map.cell(nx, ny).ok())
            .is_some_and(|other| other.ramp.is_none() && other.cliff_level < cell.cliff_level)
    })
}

/// `value` in `[0, 1]` as the nearest byte.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to [0, 1] first, so the product is in [0, 255]"
)]
fn unit_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * f32::from(u8::MAX)).round() as u8
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed test is a panic"
)]
mod tests {
    use super::*;
    use sim_map::grid::{MapCell, RampDir};

    /// A 4 by 1 strip: level 0, a ramp east from level 0, level 1, level 0.
    fn strip() -> GameMap {
        let mut map = GameMap::new(4, 1, MapCell::default_ground(CliffLevel(0))).expect("a map");
        map.cell_mut(1, 0).expect("cell").ramp = Some(RampDir::East);
        map.cell_mut(2, 0).expect("cell").cliff_level = CliffLevel(1);
        map
    }

    #[test]
    fn heights_order_with_levels_and_ramps_lie_between() {
        let mut out = Vec::new();
        write_cell_texels(&strip(), &mut out);
        let red: Vec<u8> = out.chunks(CELL_TEXEL_BYTES).map(|texel| texel[0]).collect();
        assert_eq!(red[0], 0);
        assert_eq!(red[2], 85, "level 1 of 3");
        assert!(red[0] < red[1] && red[1] < red[2], "{red:?}");
    }

    #[test]
    fn only_the_high_side_of_a_cliff_is_an_edge() {
        let map = strip();
        assert!(
            is_high_edge(&map, 2, 0),
            "level 1 above level 0 to its east"
        );
        assert!(!is_high_edge(&map, 3, 0), "the low side");
        assert!(!is_high_edge(&map, 1, 0), "a ramp is no cliff");
        assert!(!is_high_edge(&map, 0, 0));
    }

    #[test]
    fn a_rebuild_reuses_the_buffer() {
        let map = strip();
        let mut out = Vec::new();
        write_cell_texels(&map, &mut out);
        let (address, capacity) = (out.as_ptr(), out.capacity());
        write_cell_texels(&map, &mut out);
        assert_eq!((out.as_ptr(), out.capacity()), (address, capacity));
        assert_eq!(out.len(), 4 * CELL_TEXEL_BYTES);
    }
}
