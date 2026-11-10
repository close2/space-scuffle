//! `asset_baker effect-review`: texture contact sheets (plan
//! `2026-11-09-effect-visualization.md` §6.5, as for Veo clips, Q0026).
//!
//! # Responsibility
//! Renders one contact sheet per set, one row per element, so a reviewer can accept or reject
//! each element with a reason:
//! - packed elements (the middle frame of a flipbook): on black, on mid-grey, on two terrain
//!   swatches, gradient-mapped through two ramps ([`RAMP_WARM`], [`RAMP_COOL`]) on black, its
//!   erosion at 25, 50 and 75 % on mid-grey, and its alpha (G) histogram;
//! - noise sheets: R, G, B and A as grey, and R tiled 2 x 2 to show the seams;
//! - every flipbook, on a second sheet, as a frame strip on black (16 frames a row).
//!
//! A loop GIF is not written: the workspace builds the `image` crate with PNG and JPEG only,
//! and no GIF encoder is a dependency.
//!
//! # How it fits
//! The sheets are written to `docs/evidence/effects/review/` (by default) as
//! `<set>_texture_sheet.png`, `<set>_frames.png` and a legend `<set>_texture_sheet.txt`
//! naming rows and columns. The terrain swatches are the shipped terrain albedo textures, found
//! at run time.
//!
//! # Invariants
//! - The composite is the shader's rule: `out = bg * (1 - a * (1 - e)) + ramp(R) * a`, with
//!   `a` = G (eroded by B for the erosion columns) and `e` = A, so a lit element covers its
//!   background and an emissive one adds to it.
//! - Reading a set never changes it.

// Channel values and pixel coordinates are converted between `u8`, `u32` and `f32`; every
// value is bounded by 255 or by the sheet size.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use super::noise::smoothstep;
use super::texture_set::{load_ron, set_paths, ChannelLayout, EffectTextureSet};
use super::{open_rgba, png_bytes, write_atomic, EffectError};
use image::imageops::FilterType;
use image::{Rgba, RgbaImage};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Side of one tile of a sheet, in pixels.
pub const TILE: u32 = 128;

/// Side of one frame of a frame strip, in pixels.
pub const FRAME_TILE: u32 = 64;

/// Frames per row of a frame strip.
pub const FRAMES_PER_ROW: u32 = 16;

/// Gap between tiles, in pixels.
const GAP: u32 = 4;

/// Sheet background (between tiles).
const SHEET_BG: [u8; 3] = [24, 24, 28];

/// Mid-grey background.
const MID_GREY: [u8; 3] = [128, 128, 128];

/// Erosion thresholds shown, in percent.
pub const EROSION_STEPS: [u32; 3] = [25, 50, 75];

/// Width of the soft band of the eroded edge, as a share of the erosion range.
const EROSION_SOFTNESS: f32 = 0.05;

/// Bins of the alpha histogram.
const HISTOGRAM_BINS: u32 = 32;

/// A warm review ramp (fire-like): black, deep red, orange, yellow, near white.
pub const RAMP_WARM: [(f32, [u8; 3]); 5] = [
    (0.0, [0, 0, 0]),
    (0.35, [180, 30, 10]),
    (0.65, [255, 140, 20]),
    (0.85, [255, 220, 90]),
    (1.0, [255, 255, 240]),
];

/// A cool review ramp (acid-like): black, dark green, lime, pale yellow-green.
pub const RAMP_COOL: [(f32, [u8; 3]); 4] = [
    (0.0, [0, 0, 0]),
    (0.4, [20, 90, 20]),
    (0.7, [110, 220, 40]),
    (1.0, [235, 255, 180]),
];

/// What the review wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewOutput {
    /// The contact sheet.
    pub sheet: PathBuf,
    /// The frame strips, when the set has a flipbook.
    pub frames: Option<PathBuf>,
    /// The legend.
    pub legend: PathBuf,
}

/// The colour of `value` on `ramp`, interpolated linearly between stops.
fn ramp_colour(ramp: &[(f32, [u8; 3])], value: f32) -> [f32; 3] {
    let v = value.clamp(0.0, 1.0);
    for pair in ramp.windows(2) {
        let ((t0, c0), (t1, c1)) = (pair[0], pair[1]);
        if v <= t1 {
            let t = if t1 > t0 { (v - t0) / (t1 - t0) } else { 1.0 };
            return std::array::from_fn(|k| {
                (f32::from(c0[k]) + (f32::from(c1[k]) - f32::from(c0[k])) * t) / 255.0
            });
        }
    }
    ramp.last()
        .map_or([1.0; 3], |(_, c)| c.map(|k| f32::from(k) / 255.0))
}

/// How a tile maps R to colour.
#[derive(Clone, Copy)]
enum Ramp<'a> {
    Grey,
    Map(&'a [(f32, [u8; 3])]),
}

/// Composites a packed cell over `background` by the shader's rule; `erosion` (0 to 1) removes
/// texels whose erosion order is below it.
fn composite(cell: &RgbaImage, background: &RgbaImage, ramp: Ramp<'_>, erosion: f32) -> RgbaImage {
    let mut out = RgbaImage::new(cell.width(), cell.height());
    for (x, y, texel) in cell.enumerate_pixels() {
        let bg = background.get_pixel(x, y);
        let shade = f32::from(texel[0]) / 255.0;
        let colour = match ramp {
            Ramp::Grey => [shade; 3],
            Ramp::Map(stops) => ramp_colour(stops, shade),
        };
        let order = f32::from(texel[2]) / 255.0;
        let kept = if erosion > 0.0 {
            smoothstep(
                f64::from(erosion),
                f64::from(erosion + EROSION_SOFTNESS),
                f64::from(order),
            ) as f32
        } else {
            1.0
        };
        let a = f32::from(texel[1]) / 255.0 * kept;
        let e = f32::from(texel[3]) / 255.0;
        let channel = |k: usize| {
            let b = f32::from(bg[k]) / 255.0;
            ((b * (1.0 - a * (1.0 - e)) + colour[k] * a).clamp(0.0, 1.0) * 255.0).round() as u8
        };
        out.put_pixel(x, y, Rgba([channel(0), channel(1), channel(2), 255]));
    }
    out
}

/// A flat background of `side` pixels.
fn flat(side: u32, rgb: [u8; 3]) -> RgbaImage {
    RgbaImage::from_pixel(side, side, Rgba([rgb[0], rgb[1], rgb[2], 255]))
}

/// The alpha (G) histogram of a cell over its covered texels, as a bar chart tile.
fn histogram(cell: &RgbaImage) -> RgbaImage {
    let mut bins = [0_u32; HISTOGRAM_BINS as usize];
    for texel in cell.pixels().filter(|t| t[1] > 0) {
        bins[(u32::from(texel[1]) * HISTOGRAM_BINS / 256) as usize] += 1;
    }
    let peak = bins.iter().copied().max().unwrap_or(0).max(1);
    let mut tile = flat(TILE, [40, 40, 44]);
    let bar = TILE / HISTOGRAM_BINS;
    for (index, count) in bins.iter().enumerate() {
        let height = (u64::from(*count) * u64::from(TILE - 8) / u64::from(peak)) as u32;
        for x in (index as u32 * bar)..((index as u32 + 1) * bar).saturating_sub(1) {
            for y in (TILE - 4 - height)..(TILE - 4) {
                tile.put_pixel(x, y, Rgba([230, 230, 230, 255]));
            }
        }
    }
    tile
}

/// The cell of `layer` in a level-0 strip.
fn cell_of(strip: &RgbaImage, cell_size: u32, layer: u32) -> RgbaImage {
    image::imageops::crop_imm(strip, 0, layer * cell_size, cell_size, cell_size).to_image()
}

/// Shrinks a tile to `side` pixels.
fn shrink(image: &RgbaImage, side: u32) -> RgbaImage {
    image::imageops::resize(image, side, side, FilterType::Triangle)
}

/// The row of tiles of one packed element.
fn packed_row(cell: &RgbaImage, swatches: &[RgbaImage; 2]) -> Vec<RgbaImage> {
    let side = cell.width();
    let black = flat(side, [0, 0, 0]);
    let grey = flat(side, MID_GREY);
    let mut tiles = vec![
        composite(cell, &black, Ramp::Grey, 0.0),
        composite(cell, &grey, Ramp::Grey, 0.0),
        composite(cell, &swatches[0], Ramp::Grey, 0.0),
        composite(cell, &swatches[1], Ramp::Grey, 0.0),
        composite(cell, &black, Ramp::Map(&RAMP_WARM), 0.0),
        composite(cell, &black, Ramp::Map(&RAMP_COOL), 0.0),
    ];
    tiles.extend(
        EROSION_STEPS
            .iter()
            .map(|step| composite(cell, &grey, Ramp::Grey, *step as f32 / 100.0)),
    );
    let mut row: Vec<RgbaImage> = tiles.iter().map(|t| shrink(t, TILE)).collect();
    row.push(histogram(cell));
    row
}

/// The row of tiles of one noise sheet: R, G, B, A as grey and R tiled 2 x 2.
fn noise_row(cell: &RgbaImage) -> Vec<RgbaImage> {
    let side = cell.width();
    let mut row: Vec<RgbaImage> = (0..4)
        .map(|k| {
            let mut grey = RgbaImage::new(side, side);
            for (x, y, texel) in cell.enumerate_pixels() {
                let v = texel[k];
                grey.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
            shrink(&grey, TILE)
        })
        .collect();
    let half = TILE / 2;
    let small = row[0].clone();
    let small = shrink(&small, half);
    let mut tiled = RgbaImage::new(TILE, TILE);
    for (ox, oy) in [(0, 0), (half, 0), (0, half), (half, half)] {
        image::imageops::replace(&mut tiled, &small, i64::from(ox), i64::from(oy));
    }
    row.push(tiled);
    row
}

/// Lays out rows of tiles of `tile` pixels on the sheet background.
fn lay_out(rows: &[Vec<RgbaImage>], tile: u32) -> RgbaImage {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0) as u32;
    let width = GAP + columns * (tile + GAP);
    let height = GAP + rows.len() as u32 * (tile + GAP);
    let mut sheet = RgbaImage::from_pixel(
        width.max(1),
        height.max(1),
        Rgba([SHEET_BG[0], SHEET_BG[1], SHEET_BG[2], 255]),
    );
    for (r, row) in rows.iter().enumerate() {
        for (c, image) in row.iter().enumerate() {
            let x = GAP + c as u32 * (tile + GAP);
            let y = GAP + r as u32 * (tile + GAP);
            image::imageops::replace(&mut sheet, image, i64::from(x), i64::from(y));
        }
    }
    sheet
}

/// Loads a terrain swatch and fits it to `side` pixels.
fn swatch(path: &Path, side: u32) -> Result<RgbaImage, EffectError> {
    Ok(image::imageops::resize(
        &open_rgba(path)?,
        side,
        side,
        FilterType::Triangle,
    ))
}

/// The first two terrain albedo textures under `<assets>/textures/terrain/`, sorted by path.
///
/// # Errors
/// [`EffectError::InvalidRecipe`] when fewer than two exist, and [`EffectError::Io`].
pub fn default_swatches(assets_root: &Path) -> Result<[PathBuf; 2], EffectError> {
    let root = assets_root.join("textures/terrain");
    let mut found = Vec::new();
    let io = |path: &Path, source| EffectError::Io {
        path: path.to_path_buf(),
        source,
    };
    for theme in std::fs::read_dir(&root).map_err(|e| io(&root, e))? {
        let theme = theme.map_err(|e| io(&root, e))?.path();
        if !theme.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(&theme).map_err(|e| io(&theme, e))? {
            let file = file.map_err(|e| io(&theme, e))?.path();
            if file
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("_albedo.png"))
            {
                found.push(file);
            }
        }
    }
    found.sort();
    match found.as_slice() {
        [first, second, ..] => Ok([first.clone(), second.clone()]),
        _ => Err(EffectError::InvalidRecipe {
            path: root,
            reason: "fewer than two terrain albedo textures; pass --swatch twice".to_string(),
        }),
    }
}

/// Writes the contact sheets of `set` in `effects_dir` to `output_dir`.
///
/// # Errors
/// [`EffectError::Io`], [`EffectError::Image`], [`EffectError::Parse`] and
/// [`EffectError::InvalidSet`].
pub fn review_set(
    effects_dir: &Path,
    set: &str,
    swatch_paths: &[PathBuf; 2],
    output_dir: &Path,
) -> Result<ReviewOutput, EffectError> {
    let (manifest_path, _) = set_paths(effects_dir, set)?;
    let manifest: EffectTextureSet = load_ron(&manifest_path)?;
    super::texture_set::check_manifest(&manifest, &manifest_path)?;
    let strip_path = effects_dir.join(&manifest.mip_files[0]);
    let strip = open_rgba(&strip_path)?;
    let side = manifest.cell_size;
    let swatches = [
        swatch(&swatch_paths[0], side)?,
        swatch(&swatch_paths[1], side)?,
    ];
    let mut rows = Vec::new();
    let mut frame_rows = Vec::new();
    let mut legend = format!("Contact sheet of effect texture set '{set}' (plan 2026-11-09-effect-visualization §6.5).\n");
    let columns = match manifest.channels {
        ChannelLayout::PackedV1 => "on black | on mid-grey | on swatch 1 | on swatch 2 | warm ramp | cool ramp | erosion 25 % | erosion 50 % | erosion 75 % | alpha histogram",
        ChannelLayout::NoiseV1 => "R | G | B | A | R tiled 2x2",
    };
    let _ = writeln!(
        legend,
        "Columns: {columns}\nSwatches: {} ; {}\nRows:",
        swatch_paths[0].display(),
        swatch_paths[1].display()
    );
    for element in &manifest.elements {
        let middle = element.first_layer + element.frames / 2;
        let cell = cell_of(&strip, side, middle);
        rows.push(match manifest.channels {
            ChannelLayout::PackedV1 => packed_row(&cell, &swatches),
            ChannelLayout::NoiseV1 => noise_row(&cell),
        });
        let _ = writeln!(
            legend,
            "  {} ({} frame(s), middle frame shown)",
            element.name, element.frames
        );
        if element.frames > 1 {
            let black = flat(side, [0, 0, 0]);
            let frames: Vec<RgbaImage> = (element.first_layer
                ..element.first_layer + element.frames)
                .map(|layer| {
                    shrink(
                        &composite(&cell_of(&strip, side, layer), &black, Ramp::Grey, 0.0),
                        FRAME_TILE,
                    )
                })
                .collect();
            frame_rows.extend(
                frames
                    .chunks(FRAMES_PER_ROW as usize)
                    .map(<[RgbaImage]>::to_vec),
            );
        }
    }
    let sheet_path = output_dir.join(format!("{set}_texture_sheet.png"));
    let sheet = lay_out(&rows, TILE);
    write_atomic(&sheet_path, &png_bytes(&sheet, &sheet_path)?)?;
    let frames = if frame_rows.is_empty() {
        None
    } else {
        let path = output_dir.join(format!("{set}_frames.png"));
        let strip_sheet = lay_out(&frame_rows, FRAME_TILE);
        write_atomic(&path, &png_bytes(&strip_sheet, &path)?)?;
        Some(path)
    };
    let legend_path = output_dir.join(format!("{set}_texture_sheet.txt"));
    write_atomic(&legend_path, legend.as_bytes())?;
    Ok(ReviewOutput {
        sheet: sheet_path,
        frames,
        legend: legend_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramps_interpolate_between_stops() {
        assert_eq!(ramp_colour(&RAMP_COOL, 0.0), [0.0; 3]);
        let top = ramp_colour(&RAMP_WARM, 1.0);
        assert!((top[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn lit_texels_cover_and_emissive_texels_add() {
        let grey = flat(1, MID_GREY);
        let lit = RgbaImage::from_pixel(1, 1, Rgba([255, 255, 255, 0]));
        assert_eq!(
            composite(&lit, &grey, Ramp::Grey, 0.0).get_pixel(0, 0)[0],
            255
        );
        let dark_lit = RgbaImage::from_pixel(1, 1, Rgba([0, 255, 255, 0]));
        assert_eq!(
            composite(&dark_lit, &grey, Ramp::Grey, 0.0).get_pixel(0, 0)[0],
            0
        );
        let dark_emissive = RgbaImage::from_pixel(1, 1, Rgba([0, 255, 255, 255]));
        assert_eq!(
            composite(&dark_emissive, &grey, Ramp::Grey, 0.0).get_pixel(0, 0)[0],
            128
        );
        // Erosion above the texel's order removes it.
        let low_order = RgbaImage::from_pixel(1, 1, Rgba([255, 255, 10, 0]));
        assert_eq!(
            composite(&low_order, &grey, Ramp::Grey, 0.5).get_pixel(0, 0)[0],
            128
        );
    }
}
