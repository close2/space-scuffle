//! The mapping between the map plane and the minimap image (plan `2026-12-01-minimap` D-2).
//!
//! # Responsibility
//! Owns [`MinimapLayout`], the one place the minimap's coordinate spaces meet:
//! - **world units** on the map plane: simulation `(x, y)`, which is Bevy's `(x, z)`, one unit per
//!   grid cell, from `(0, 0)` at the map's corner to `(width, height)`;
//! - **minimap pixels**: physical pixels of the minimap image, from its top-left corner, `x` to the
//!   right and `y` down.
//!
//! Screen-up is simulation −y in the world view (the camera looks along −z) and pixel −y on the
//! minimap, so neither axis flips. A map that is not square is letterboxed: it is scaled to fit
//! the square's longer side and centred, and the bands left over are the background, as
//! `StarCraft` II's black minimap background is (`core` `GameUIData:694`).
//!
//! # How it fits
//! The CPU side (the marks builder, picking, the egui panel) and the GPU side (the uniform of
//! `assets/shaders/minimap.wgsl`) both use the `scale` and `offset` computed here, so a point maps
//! to the same pixel on both.
//!
//! # Invariants
//! - `scale` is positive and finite, and the map rectangle lies inside the square.
//! - [`MinimapLayout::to_world`] inverts [`MinimapLayout::to_minimap`] up to rounding.
//! - [`MinimapLayout::to_world_clamped`] answers a point no farther than half a cell inside every
//!   map edge, the bound the RTS camera clamps its target to.

use bevy::prelude::*;

/// How far inside each map edge a clamped point stays, in world units: the camera's own bound.
pub const EDGE_INSET: f32 = 0.5;

/// The minimap's mapping for one map and one image size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapLayout {
    /// Map width in cells (world units).
    pub map_width: f32,
    /// Map height in cells (world units).
    pub map_height: f32,
    /// Side of the square image, in physical pixels.
    pub side: f32,
    /// Pixels per world unit.
    pub scale: f32,
    /// Pixel position of the map's `(0, 0)` corner, from the image's top-left.
    pub offset: Vec2,
}

impl MinimapLayout {
    /// The layout of a `map_width` by `map_height` map, in cells, on a square image `side`
    /// physical pixels wide. `None` when either size is zero or the side is not positive and
    /// finite: there is then nothing to map.
    #[must_use]
    pub fn new(map_width: u32, map_height: u32, side: f32) -> Option<Self> {
        if map_width == 0 || map_height == 0 || !side.is_finite() || side <= 0.0 {
            return None;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "map sides are at most 256 cells (`MAX_MAP_DIMENSION`)"
        )]
        let (width, height) = (map_width as f32, map_height as f32);
        let scale = side / width.max(height);
        Some(Self {
            map_width: width,
            map_height: height,
            side,
            scale,
            offset: Vec2::new((side - width * scale) * 0.5, (side - height * scale) * 0.5),
        })
    }

    /// The pixel, from the image's top-left, of map point `world` (simulation `(x, y)`, world
    /// units).
    #[must_use]
    pub fn to_minimap(&self, world: Vec2) -> Vec2 {
        self.offset + world * self.scale
    }

    /// The map point, in world units, under pixel `pixel`. Points in the letterbox map off the
    /// map.
    #[must_use]
    pub fn to_world(&self, pixel: Vec2) -> Vec2 {
        (pixel - self.offset) / self.scale
    }

    /// As [`Self::to_world`], clamped to `[0.5, width − 0.5] × [0.5, height − 0.5]`, so a press in
    /// the letterbox aims at the nearest map edge exactly where the camera would stop.
    #[must_use]
    pub fn to_world_clamped(&self, pixel: Vec2) -> Vec2 {
        let world = self.to_world(pixel);
        Vec2::new(
            world.x.clamp(EDGE_INSET, self.map_width - EDGE_INSET),
            world.y.clamp(EDGE_INSET, self.map_height - EDGE_INSET),
        )
    }

    /// Whether pixel `pixel` lies on the map rather than in the letterbox or outside the image.
    #[must_use]
    pub fn contains(&self, pixel: Vec2) -> bool {
        let world = self.to_world(pixel);
        (0.0..self.map_width).contains(&world.x) && (0.0..self.map_height).contains(&world.y)
    }

    /// Converts a length in world units to pixels.
    #[must_use]
    pub fn pixels(&self, world_units: f32) -> f32 {
        world_units * self.scale
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed test is a panic"
)]
mod tests {
    use super::*;

    #[test]
    fn a_square_map_fills_the_image() {
        let layout = MinimapLayout::new(64, 64, 128.0).expect("a layout");
        assert_eq!(layout.scale, 2.0);
        assert_eq!(layout.offset, Vec2::ZERO);
        assert_eq!(layout.to_minimap(Vec2::new(64.0, 64.0)), Vec2::splat(128.0));
    }

    #[test]
    fn a_wide_map_is_letterboxed_top_and_bottom() {
        let layout = MinimapLayout::new(96, 64, 192.0).expect("a layout");
        assert_eq!(layout.scale, 2.0);
        assert_eq!(layout.offset, Vec2::new(0.0, 32.0));
        assert!(!layout.contains(Vec2::new(10.0, 10.0)));
        assert!(layout.contains(Vec2::new(10.0, 40.0)));
        assert_eq!(
            layout.to_world_clamped(Vec2::new(10.0, 1.0)),
            Vec2::new(5.0, 0.5)
        );
    }

    #[test]
    fn degenerate_inputs_have_no_layout() {
        assert!(MinimapLayout::new(0, 64, 100.0).is_none());
        assert!(MinimapLayout::new(64, 64, 0.0).is_none());
        assert!(MinimapLayout::new(64, 64, f32::NAN).is_none());
    }

    #[test]
    fn the_far_corner_is_outside() {
        let layout = MinimapLayout::new(64, 64, 128.0).expect("a layout");
        assert!(layout.contains(Vec2::ZERO));
        assert!(!layout.contains(Vec2::splat(128.0)));
    }
}
