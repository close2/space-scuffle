//! Centred cells, erosion order, channel packing and mip chains (plan
//! `2026-11-09-effect-visualization.md` §6.4).
//!
//! # Responsibility
//! - [`centre_into_cells`]: resamples keyed pictures (one still, or every kept frame of a
//!   flipbook) into square cells of a fixed size with [`PADDING_PX`] of empty border, centred
//!   on the element's density-weighted centroid. A flipbook shares one centre and one scale over
//!   all its frames, so the element does not jitter from frame to frame.
//! - [`erosion_order`]: B channel, a blurred distance-to-edge times a noise sheet, normalised,
//!   so thin parts and the rim erode first when the shader raises its erosion threshold.
//! - [`pack_cell`]: one RGBA8 texel layout for every set: R shading, G density (alpha),
//!   B erosion order, A emissive mask.
//! - [`mip_chain`]: the mip levels of every cell, computed per cell so neighbouring cells never
//!   bleed into each other, down to [`MIN_MIP_CELL`] pixels.
//!
//! # How it fits
//! [`crate::effects::bake`] keys sources with [`crate::effects::keying`], then calls these in
//! order; [`crate::effects::texture_set`] writes the strips.
//!
//! # Invariants
//! - Cells are square, `cell_size` pixels, with a [`PADDING_PX`] border of zero density.
//! - Every step is deterministic: fixed sample patterns, integer mip averaging.

// Pixel coordinates and channel values are converted between integers and `f32`/`f64`
// throughout; every conversion is of a value bounded by the picture size or by 255.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use super::keying::{KeyedElement, Plane};
use super::noise::{perlin_noise, Fractal};
use super::EffectError;
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

/// Empty border around every element inside its cell, in pixels (plan §6.4).
pub const PADDING_PX: u32 = 2;

/// Smallest cell side a mip level keeps, in pixels.
pub const MIN_MIP_CELL: u32 = 16;

/// Most samples per axis taken in one target pixel's footprint when shrinking.
const MAX_SUPERSAMPLE: u32 = 8;

/// Density below which a pixel counts as outside the element (1/255): ignored by the extent.
const EXTENT_LEVEL: f32 = 1.0 / 255.0;

/// How the erosion order (B) is derived, recorded in provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErosionParams {
    /// The noise sheet the distance is multiplied by (Perlin, tileable).
    pub noise: Fractal,
    /// Radius of the box blur applied three times to the distance, in cell pixels.
    pub blur_radius_px: u32,
    /// Density at and above which a pixel is inside the element, in 1/255.
    pub mask_level: u8,
}

impl Default for ErosionParams {
    fn default() -> Self {
        Self {
            noise: Fractal {
                seed: 1,
                period: 4,
                octaves: 3,
                gain_permille: 500,
            },
            blur_radius_px: 2,
            mask_level: 8,
        }
    }
}

/// One centred cell before packing.
#[derive(Debug, Clone, PartialEq)]
pub struct CellPlanes {
    /// The keyed element resampled into the cell.
    pub keyed: KeyedElement,
}

/// Bilinear sample of `plane` at `(x, y)` in pixel units (pixel centres at `i + 0.5`); 0
/// outside.
fn bilinear(plane: &Plane, x: f64, y: f64) -> f32 {
    let (fx, fy) = (x - 0.5, y - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = ((fx - x0) as f32, (fy - y0) as f32);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let top = plane.get(x0, y0) * (1.0 - tx) + plane.get(x0 + 1, y0) * tx;
    let bottom = plane.get(x0, y0 + 1) * (1.0 - tx) + plane.get(x0 + 1, y0 + 1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// The density-weighted centroid of `pictures` together and the half side of the square
/// around it that holds every pixel with density above 1/255.
fn joint_frame(pictures: &[KeyedElement], element: &str) -> Result<(f64, f64, f64), EffectError> {
    let (mut sum, mut sx, mut sy) = (0.0_f64, 0.0_f64, 0.0_f64);
    for picture in pictures {
        let plane = &picture.density;
        for (index, d) in plane.data.iter().enumerate() {
            let x = f64::from(index as u32 % plane.width) + 0.5;
            let y = f64::from(index as u32 / plane.width) + 0.5;
            sum += f64::from(*d);
            sx += f64::from(*d) * x;
            sy += f64::from(*d) * y;
        }
    }
    if sum <= 0.0 {
        return Err(EffectError::EmptyElement {
            element: element.to_string(),
            reason: "the keyed element has no density".to_string(),
        });
    }
    let (cx, cy) = (sx / sum, sy / sum);
    let mut half = 1.0_f64;
    for picture in pictures {
        let plane = &picture.density;
        for (index, d) in plane.data.iter().enumerate() {
            if *d > EXTENT_LEVEL {
                let x = f64::from(index as u32 % plane.width);
                let y = f64::from(index as u32 / plane.width);
                // The pixel covers [x, x + 1): its far edge from the centroid.
                half = half
                    .max((x - cx).abs().max((x + 1.0 - cx).abs()))
                    .max((y - cy).abs().max((y + 1.0 - cy).abs()));
            }
        }
    }
    Ok((cx, cy, half))
}

/// Resamples every picture into a `cell_size` cell centred on the joint centroid, the joint
/// extent filling the cell inside [`PADDING_PX`].
///
/// # Errors
/// [`EffectError::EmptyElement`] when no picture has density, and
/// [`EffectError::InvalidRecipe`] for a cell too small for its padding.
pub fn centre_into_cells(
    pictures: &[KeyedElement],
    cell_size: u32,
    element: &str,
) -> Result<Vec<CellPlanes>, EffectError> {
    if cell_size <= 2 * PADDING_PX + 1 {
        return Err(EffectError::InvalidRecipe {
            path: element.into(),
            reason: format!("cell size {cell_size} leaves no room inside the padding"),
        });
    }
    let (cx, cy, half) = joint_frame(pictures, element)?;
    let inner = cell_size - 2 * PADDING_PX;
    let scale = 2.0 * half / f64::from(inner);
    let samples = (scale.ceil() as u32).clamp(1, MAX_SUPERSAMPLE);
    Ok(pictures
        .iter()
        .map(|picture| CellPlanes {
            keyed: resample_into_cell(picture, (cx - half, cy - half), scale, samples, cell_size),
        })
        .collect())
}

/// Resamples one keyed picture; `origin` is the source point at the inner region's top-left
/// corner and `scale` source pixels per cell pixel.
fn resample_into_cell(
    picture: &KeyedElement,
    origin: (f64, f64),
    scale: f64,
    samples: u32,
    cell_size: u32,
) -> KeyedElement {
    let mut density = Plane::zeros(cell_size, cell_size);
    let mut shading = Plane::zeros(cell_size, cell_size);
    let mut emissive = Plane::zeros(cell_size, cell_size);
    let inner = cell_size - 2 * PADDING_PX;
    let weight = 1.0 / (samples * samples) as f32;
    for ty in 0..inner {
        for tx in 0..inner {
            let (mut d, mut sd, mut e) = (0.0_f32, 0.0_f32, 0.0_f32);
            for sy in 0..samples {
                for sx in 0..samples {
                    let x = origin.0
                        + (f64::from(tx) + (f64::from(sx) + 0.5) / f64::from(samples)) * scale;
                    let y = origin.1
                        + (f64::from(ty) + (f64::from(sy) + 0.5) / f64::from(samples)) * scale;
                    let density_here = bilinear(&picture.density, x, y);
                    d += density_here;
                    sd += density_here * bilinear(&picture.shading, x, y);
                    e += bilinear(&picture.emissive, x, y);
                }
            }
            let index = ((ty + PADDING_PX) * cell_size + tx + PADDING_PX) as usize;
            density.data[index] = (d * weight).clamp(0.0, 1.0);
            shading.data[index] = if d > 0.0 {
                (sd / d).clamp(0.0, 1.0)
            } else {
                0.0
            };
            emissive.data[index] = (e * weight).clamp(0.0, 1.0);
        }
    }
    KeyedElement {
        shading,
        density,
        emissive,
    }
}

/// Chamfer (3-4) distance of every inside pixel to the nearest outside one; pixels beyond the
/// plane count as outside. In units of one pixel step.
fn chamfer_distance(inside: &[bool], size: u32) -> Vec<f32> {
    const STRAIGHT: f32 = 3.0;
    const DIAGONAL: f32 = 4.0;
    let n = i64::from(size);
    let mut dist: Vec<f32> = inside
        .iter()
        .map(|i| if *i { f32::MAX } else { 0.0 })
        .collect();
    let at = |dist: &[f32], x: i64, y: i64| {
        if x < 0 || y < 0 || x >= n || y >= n {
            0.0
        } else {
            dist[(y * n + x) as usize]
        }
    };
    for y in 0..n {
        for x in 0..n {
            let here = (y * n + x) as usize;
            let best = (at(&dist, x - 1, y) + STRAIGHT)
                .min(at(&dist, x, y - 1) + STRAIGHT)
                .min(at(&dist, x - 1, y - 1) + DIAGONAL)
                .min(at(&dist, x + 1, y - 1) + DIAGONAL);
            dist[here] = dist[here].min(best);
        }
    }
    for y in (0..n).rev() {
        for x in (0..n).rev() {
            let here = (y * n + x) as usize;
            let best = (at(&dist, x + 1, y) + STRAIGHT)
                .min(at(&dist, x, y + 1) + STRAIGHT)
                .min(at(&dist, x + 1, y + 1) + DIAGONAL)
                .min(at(&dist, x - 1, y + 1) + DIAGONAL);
            dist[here] = dist[here].min(best);
        }
    }
    dist.into_iter().map(|d| d / STRAIGHT).collect()
}

/// One horizontal then vertical box blur of `radius` (zero outside).
fn box_blur(values: &[f32], size: u32, radius: u32) -> Vec<f32> {
    let n = i64::from(size);
    let r = i64::from(radius);
    let norm = 1.0 / (2 * r + 1) as f32;
    let pass = |source: &[f32], horizontal: bool| -> Vec<f32> {
        (0..n * n)
            .map(|index| {
                let (x, y) = (index % n, index / n);
                (-r..=r)
                    .map(|k| {
                        let (sx, sy) = if horizontal { (x + k, y) } else { (x, y + k) };
                        if sx < 0 || sy < 0 || sx >= n || sy >= n {
                            0.0
                        } else {
                            source[(sy * n + sx) as usize]
                        }
                    })
                    .sum::<f32>()
                    * norm
            })
            .collect()
    };
    pass(&pass(values, true), false)
}

/// The erosion order of a cell (B): the chamfer distance to the edge of the region at or above
/// `mask_level`, box-blurred three times, normalised, times `0.5 + 0.5 * noise`, normalised
/// again; 0 outside the region. Low values erode first.
#[must_use]
pub fn erosion_order(density: &Plane, params: &ErosionParams) -> Plane {
    let size = density.width;
    let level = f32::from(params.mask_level) / 255.0;
    let inside: Vec<bool> = density.data.iter().map(|d| *d >= level).collect();
    let mut distance = chamfer_distance(&inside, size);
    for _ in 0..3 {
        distance = box_blur(&distance, size, params.blur_radius_px);
    }
    let side = f64::from(size);
    let mut order: Vec<f32> = distance
        .iter()
        .enumerate()
        .map(|(index, d)| {
            if !inside[index] {
                return 0.0;
            }
            let x = f64::from(index as u32 % size) + 0.5;
            let y = f64::from(index as u32 / size) + 0.5;
            let n = perlin_noise(&params.noise, x / side, y / side) as f32;
            d * (0.5 + 0.5 * n)
        })
        .collect();
    let peak = order.iter().copied().fold(0.0_f32, f32::max);
    if peak > 0.0 {
        for value in &mut order {
            *value /= peak;
        }
    }
    Plane {
        width: size,
        height: size,
        data: order,
    }
}

/// A value in `[0, 1]` as an 8-bit channel.
fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Packs one cell: R shading, G density, B erosion order, A emissive mask.
#[must_use]
pub fn pack_cell(cell: &CellPlanes, params: &ErosionParams) -> RgbaImage {
    let keyed = &cell.keyed;
    let erosion = erosion_order(&keyed.density, params);
    let size = keyed.density.width;
    let mut image = RgbaImage::new(size, size);
    for (index, pixel) in image.pixels_mut().enumerate() {
        *pixel = Rgba([
            to_byte(keyed.shading.data[index]),
            to_byte(keyed.density.data[index]),
            to_byte(erosion.data[index]),
            to_byte(keyed.emissive.data[index]),
        ]);
    }
    image
}

/// How a mip level averages the channels of four texels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MipFilter {
    /// Packed effect texels: R (shading) weighted by G (density), the others plain means.
    Packed,
    /// Data (noise) texels: every channel a plain mean.
    Data,
}

/// The next mip level of one cell: each texel the rounded mean of its 2x2 parents.
fn halve(cell: &RgbaImage, filter: MipFilter) -> RgbaImage {
    let side = (cell.width() / 2).max(1);
    let mut next = RgbaImage::new(side, side);
    for y in 0..side {
        for x in 0..side {
            let parents = [
                cell.get_pixel(2 * x, 2 * y),
                cell.get_pixel(2 * x + 1, 2 * y),
                cell.get_pixel(2 * x, 2 * y + 1),
                cell.get_pixel(2 * x + 1, 2 * y + 1),
            ];
            let mean = |k: usize| {
                let total: u32 = parents.iter().map(|p| u32::from(p[k])).sum();
                ((total + 2) / 4) as u8
            };
            let density: u32 = parents.iter().map(|p| u32::from(p[1])).sum();
            let shading = match filter {
                MipFilter::Packed if density > 0 => {
                    let weighted: u32 = parents
                        .iter()
                        .map(|p| u32::from(p[0]) * u32::from(p[1]))
                        .sum();
                    ((weighted + density / 2) / density) as u8
                }
                MipFilter::Packed | MipFilter::Data => mean(0),
            };
            next.put_pixel(x, y, Rgba([shading, mean(1), mean(2), mean(3)]));
        }
    }
    next
}

/// Every mip level of every cell: level 0 is `cells` itself, each next level halves the side,
/// down to [`MIN_MIP_CELL`] pixels.
#[must_use]
pub fn mip_chain(cells: &[RgbaImage], filter: MipFilter) -> Vec<Vec<RgbaImage>> {
    let mut levels = vec![cells.to_vec()];
    while let Some(last) = levels.last() {
        let side = last.first().map_or(0, RgbaImage::width);
        if side / 2 < MIN_MIP_CELL {
            break;
        }
        let next = last.iter().map(|cell| halve(cell, filter)).collect();
        levels.push(next);
    }
    levels
}

/// Stacks square cells into one vertical strip, the layout the client reads as a texture
/// array (one layer per cell).
#[must_use]
pub fn strip(cells: &[RgbaImage]) -> RgbaImage {
    let side = cells.first().map_or(0, RgbaImage::width);
    let mut image = RgbaImage::new(side, side * cells.len() as u32);
    for (layer, cell) in cells.iter().enumerate() {
        for (x, y, pixel) in cell.enumerate_pixels() {
            image.put_pixel(x, layer as u32 * side + y, *pixel);
        }
    }
    image
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn disc(size: u32, cx: f32, cy: f32, radius: f32) -> KeyedElement {
        let mut density = Plane::zeros(size, size);
        for (index, value) in density.data.iter_mut().enumerate() {
            let x = (index as u32 % size) as f32 + 0.5;
            let y = (index as u32 / size) as f32 + 0.5;
            if (x - cx).hypot(y - cy) <= radius {
                *value = 1.0;
            }
        }
        KeyedElement {
            shading: density.clone(),
            emissive: Plane::zeros(size, size),
            density,
        }
    }

    #[test]
    fn an_off_centre_element_is_centred_and_padded() {
        let cells = centre_into_cells(&[disc(100, 20.0, 70.0, 10.0)], 32, "e").expect("lit");
        let density = &cells[0].keyed.density;
        // The padding border is empty.
        for i in 0..32_i64 {
            for j in 0..2_i64 {
                assert_eq!(density.get(i, j), 0.0);
                assert_eq!(density.get(j, i), 0.0);
                assert_eq!(density.get(i, 31 - j), 0.0);
            }
        }
        // The centre is covered.
        assert!(density.get(16, 16) > 0.99);
    }

    #[test]
    fn erosion_order_is_lowest_at_the_rim_and_zero_outside() {
        let cells = centre_into_cells(&[disc(64, 32.0, 32.0, 28.0)], 64, "e").expect("lit");
        let order = erosion_order(&cells[0].keyed.density, &ErosionParams::default());
        assert_eq!(order.get(0, 0), 0.0);
        let centre = order.get(32, 32);
        let rim = order.get(4, 32);
        assert!(centre > rim, "{centre} {rim}");
        assert!(order.data.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn mips_halve_down_to_the_minimum_and_weight_shading_by_density() {
        let mut cell = RgbaImage::new(64, 64);
        cell.put_pixel(0, 0, Rgba([200, 255, 0, 0]));
        let levels = mip_chain(&[cell], MipFilter::Packed);
        let sides: Vec<u32> = levels.iter().map(|l| l[0].width()).collect();
        assert_eq!(sides, vec![64, 32, 16]);
        // Shading keeps the lit texel's value instead of averaging it with empty texels.
        assert_eq!(levels[1][0].get_pixel(0, 0)[0], 200);
        assert_eq!(levels[1][0].get_pixel(0, 0)[1], 64);
    }

    #[test]
    fn strips_stack_cells_in_order() {
        let a = RgbaImage::from_pixel(4, 4, Rgba([1, 0, 0, 0]));
        let b = RgbaImage::from_pixel(4, 4, Rgba([2, 0, 0, 0]));
        let s = strip(&[a, b]);
        assert_eq!(s.dimensions(), (4, 8));
        assert_eq!(s.get_pixel(0, 5)[0], 2);
    }
}
