//! Splat and overlay field texture generators and partial region updaters.
//!
//! # Architectural Responsibilities
//! - **Splat Texture:** Binds cell surface types (Grass, `MetalPlate`, Dirt) into an RGBA8
//!   texture used by the terrain WGSL shader for continuous height-blended ground texturing.
//!   Abyss cells sample their nearest non-abyss neighbour to eliminate hard dark fringes at cliff rims.
//! - **Overlay Texture:** Encodes tactical cell flags (ground pathability, buildability),
//!   dynamic Line-of-Sight visibility states, and live map validation defect highlights
//!   into an unfilterable texture read via `textureLoad` after lighting.
//! - **Zero Per-Frame Heap Allocations:** Updates write directly into the pre-allocated
//!   raw byte buffers of Bevy [`Image`] assets.

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use sim_map::grid::{CellFlags, GameMap, TerrainKind};
use sim_map::validation::{CellRect, MapValidationReport};
use sim_map::vision::PlayerVisionMap;
use sim_types::VisibilityState;

/// Creates a new `WxH` [`Image`] initialized with splat weights from the simulation map.
#[must_use]
pub fn create_splat_image(map: &GameMap) -> Image {
    let width = map.width();
    let height = map.height();
    let mut data = vec![0u8; (width as usize) * (height as usize) * 4];

    write_splat_data(map, &mut data, None);

    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::ClampToEdge,
        address_mode_v: bevy::image::ImageAddressMode::ClampToEdge,
        mag_filter: bevy::image::ImageFilterMode::Linear,
        min_filter: bevy::image::ImageFilterMode::Linear,
        mipmap_filter: bevy::image::ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// Creates a new `WxH` [`Image`] initialized with tactical overlay channels from the simulation map.
#[must_use]
pub fn create_overlay_image(map: &GameMap) -> Image {
    let width = map.width();
    let height = map.height();
    let mut data = vec![0u8; (width as usize) * (height as usize) * 4];

    write_overlay_data(map, None, None, &mut data, None);

    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    )
}

/// Updates splat weights in the target [`Image`] across the full map or an optional bounding rect.
pub fn update_splat_image(map: &GameMap, image: &mut Image, rect: Option<CellRect>) {
    write_splat_data(map, &mut image.data, rect);
}

/// Updates tactical overlay channels in the target [`Image`].
pub fn update_overlay_image(
    map: &GameMap,
    validation_report: Option<&MapValidationReport>,
    los_map: Option<&PlayerVisionMap>,
    image: &mut Image,
    rect: Option<CellRect>,
) {
    write_overlay_data(map, validation_report, los_map, &mut image.data, rect);
}

/// Writes splat color values into a raw RGBA8 byte buffer.
fn write_splat_data(map: &GameMap, data: &mut [u8], rect: Option<CellRect>) {
    let width = map.width();
    let height = map.height();

    let (min_x, max_x, min_y, max_y) = if let Some(r) = rect {
        (
            r.min.0.min(width - 1),
            r.max_inclusive.0.min(width - 1),
            r.min.1.min(height - 1),
            r.max_inclusive.1.min(height - 1),
        )
    } else {
        (0, width - 1, 0, height - 1)
    };

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let Ok(cell) = map.cell(x, y) else {
                continue;
            };
            let kind = if cell.terrain_kind == TerrainKind::Abyss {
                find_nearest_non_abyss_kind(map, x, y)
            } else {
                cell.terrain_kind
            };

            let pixel_offset = ((y * width + x) * 4) as usize;
            if pixel_offset + 3 < data.len() {
                let (r, g, b) = match kind {
                    TerrainKind::Grass => (255, 0, 0),
                    TerrainKind::MetalPlate => (0, 255, 0),
                    TerrainKind::Dirt | TerrainKind::Abyss => (0, 0, 255),
                };
                data[pixel_offset] = r;
                data[pixel_offset + 1] = g;
                data[pixel_offset + 2] = b;
                data[pixel_offset + 3] = 0;
            }
        }
    }
}

/// Finds the nearest non-abyss terrain kind within radius 1 for abyss rim texture blending.
fn find_nearest_non_abyss_kind(map: &GameMap, x: u32, y: u32) -> TerrainKind {
    let w = i32::try_from(map.width()).unwrap_or(i32::MAX);
    let h = i32::try_from(map.height()).unwrap_or(i32::MAX);
    let ix = i32::try_from(x).unwrap_or(i32::MAX);
    let iy = i32::try_from(y).unwrap_or(i32::MAX);

    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = ix + dx;
            let ny = iy + dy;
            if nx >= 0 && nx < w && ny >= 0 && ny < h {
                if let (Ok(unx), Ok(uny)) = (u32::try_from(nx), u32::try_from(ny)) {
                    if let Ok(neighbor) = map.cell(unx, uny) {
                        if neighbor.terrain_kind != TerrainKind::Abyss {
                            return neighbor.terrain_kind;
                        }
                    }
                }
            }
        }
    }

    TerrainKind::Dirt
}

/// Writes tactical overlay channels into a raw RGBA8 byte buffer.
fn write_overlay_data(
    map: &GameMap,
    validation_report: Option<&MapValidationReport>,
    los_map: Option<&PlayerVisionMap>,
    data: &mut [u8],
    rect: Option<CellRect>,
) {
    let width = map.width();
    let height = map.height();

    let (min_x, max_x, min_y, max_y) = if let Some(r) = rect {
        (
            r.min.0.min(width - 1),
            r.max_inclusive.0.min(width - 1),
            r.min.1.min(height - 1),
            r.max_inclusive.1.min(height - 1),
        )
    } else {
        (0, width - 1, 0, height - 1)
    };

    // Pre-gather error bounding boxes to test cell containment
    let error_rects: Vec<CellRect> = if let Some(rep) = validation_report {
        rep.errors
            .iter()
            .map(sim_map::validation::MapValidationError::location)
            .collect()
    } else {
        Vec::new()
    };

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let Ok(cell) = map.cell(x, y) else {
                continue;
            };

            // Channel R: Pathability (255 if GROUND_PATHABLE and not Abyss)
            let pathable = cell.flags.contains(CellFlags::GROUND_PATHABLE)
                && cell.terrain_kind != TerrainKind::Abyss;
            let r = if pathable { 255 } else { 0 };

            // Channel G: Buildability (255 if BUILDABLE)
            let buildable = cell.flags.contains(CellFlags::BUILDABLE);
            let g = if buildable { 255 } else { 0 };

            // Channel B: LOS visibility
            let b = if let Some(los) = los_map {
                match los.visibility_at(x, y) {
                    VisibilityState::Visible => 255,
                    VisibilityState::Fogged => 96,
                    VisibilityState::Unexplored => 0,
                }
            } else {
                255
            };

            // Channel A: Validation error highlight
            let is_error = error_rects.iter().any(|rect| {
                x >= rect.min.0
                    && x <= rect.max_inclusive.0
                    && y >= rect.min.1
                    && y <= rect.max_inclusive.1
            });
            let a = if is_error { 255 } else { 0 };

            let pixel_offset = ((y * width + x) * 4) as usize;
            if pixel_offset + 3 < data.len() {
                data[pixel_offset] = r;
                data[pixel_offset + 1] = g;
                data[pixel_offset + 2] = b;
                data[pixel_offset + 3] = a;
            }
        }
    }
}
