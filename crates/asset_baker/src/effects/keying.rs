//! Keying effect sources on black (plan `2026-11-09-effect-visualization.md` §6.4).
//!
//! # Responsibility
//! - [`check_black`]: the black check. The border band and the darkest 5 % of a source picture
//!   must be at most 3/255; a lit floor, a horizon or radiating stripes reach the border and
//!   fail it.
//! - [`luminance_emissive`]: the luminance key for emissive elements (fire, energy, glow):
//!   luminance after black-point and white-point levels is both intensity and coverage.
//! - [`difference_matte`]: Smith and Blinn's triangulation matting from the same element on
//!   black and on white: `alpha = 1 - (white - black)`, `colour = black / alpha`. The pair is
//!   rejected when the matte residual exceeds 4/255 on more than 0.5 % of the pixels.
//! - [`luminance_key`]: the fallback for lit semi-transparent elements without a pair:
//!   `density = smoothstep(levels(L))`, `shading = levels(L) / density`.
//!
//! # How it fits
//! [`crate::effects::bake`] keys every source picture (a still, a pair or each kept clip frame)
//! into a [`KeyedElement`], which [`crate::effects::packing`] centres, packs and erodes.
//!
//! # Invariants
//! - Arithmetic is on the stored 8-bit values divided by 255, the space the thresholds are
//!   stated in. Generated pictures are not physically lit composites, so the matte is computed
//!   on the values the model returned, not on linear light.
//! - Every plane of a [`KeyedElement`] has the source's width and height and values in `[0, 1]`.

// Pixel indices and 8-bit values are converted between integers and `f32` throughout; every
// conversion is of a value bounded by the picture size or by 255.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use super::EffectError;
use image::RgbaImage;
use serde::{Deserialize, Serialize};

/// Rec. 709 luma weights, as `terrain_texture` uses.
const LUMA: [f32; 3] = [
    crate::terrain_texture::LUMA_REC709_R,
    crate::terrain_texture::LUMA_REC709_G,
    crate::terrain_texture::LUMA_REC709_B,
];

/// Brightest value, in 1/255, that still counts as black (plan §6.4).
pub const BLACK_LEVEL: u8 = 3;

/// Share of the darkest pixels that must be black, in percent (plan §6.4).
pub const DARKEST_PERCENT: u32 = 5;

/// Most border-band pixels of one side brighter than [`BLACK_LEVEL`], in thousandths: room
/// for compression noise, not for a floor, a horizon or a ray.
pub const MAX_LIT_BORDER_PERMILLE: u32 = 10;

/// Border band thickness as a fraction of the shorter side (at least 2 pixels).
pub const BORDER_BAND_DIVISOR: u32 = 32;

/// Largest matte residual accepted at a pixel, in 1/255 (plan §6.4).
pub const MATTE_RESIDUAL_LIMIT: u8 = 4;

/// Most pixels whose residual exceeds [`MATTE_RESIDUAL_LIMIT`], in thousandths (0.5 %).
pub const MATTE_OVER_LIMIT_PERMILLE: u64 = 5;

/// Percentile of lit luminance taken as the white point, in thousandths.
pub const WHITE_POINT_PERMILLE: u32 = 995;

/// Lowest white point accepted, in 1/255, so a faint element is not stretched into noise.
pub const MIN_WHITE_POINT: u8 = 32;

/// A plane of `f32` values in `[0, 1]`, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height` values.
    pub data: Vec<f32>,
}

impl Plane {
    /// A plane of zeros.
    #[must_use]
    pub fn zeros(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0.0; (width as usize) * (height as usize)],
        }
    }

    /// The value at `(x, y)`, 0 outside the plane.
    #[must_use]
    pub fn get(&self, x: i64, y: i64) -> f32 {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            0.0
        } else {
            self.data[(y as usize) * (self.width as usize) + x as usize]
        }
    }
}

/// An element after keying: what [`crate::effects::packing`] packs into R, G and A.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyedElement {
    /// Greyscale shading (R).
    pub shading: Plane,
    /// Coverage / alpha (G).
    pub density: Plane,
    /// Emissive mask (A): 1 where the element is drawn additively.
    pub emissive: Plane,
}

/// The levels a luminance key used, in 1/255, recorded in provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Levels {
    /// Black point.
    pub black: u8,
    /// White point.
    pub white: u8,
}

/// What the black check measured on the worst picture, recorded in provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlackCheckReport {
    /// Thickness of the border band, in pixels.
    pub band_px: u32,
    /// Border-band pixels above [`BLACK_LEVEL`] per side (top, bottom, left, right), in
    /// thousandths.
    pub lit_border_permille: [u32; 4],
    /// Luminance at the [`DARKEST_PERCENT`] percentile, in 1/255.
    pub darkest_level: u8,
}

impl BlackCheckReport {
    /// The report of whichever of `self` and `other` is closer to failing.
    #[must_use]
    pub fn worst(self, other: Self) -> Self {
        let key = |r: &Self| (r.lit_border_permille.iter().copied().max(), r.darkest_level);
        if key(&other) > key(&self) {
            other
        } else {
            self
        }
    }
}

/// What the difference matte measured, recorded in provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatteReport {
    /// Pixels whose residual exceeds [`MATTE_RESIDUAL_LIMIT`].
    pub over_limit: u64,
    /// Pixels compared.
    pub pixels: u64,
    /// Largest residual, in 1/255.
    pub max_residual: u8,
}

/// The 8-bit luminance of an RGB pixel, in `[0, 1]`.
fn luma(pixel: image::Rgba<u8>) -> f32 {
    (LUMA[0] * f32::from(pixel[0]) + LUMA[1] * f32::from(pixel[1]) + LUMA[2] * f32::from(pixel[2]))
        / 255.0
}

/// Luminance of every pixel, in 1/255, rounded.
fn luma_levels(image: &RgbaImage) -> Vec<u8> {
    image
        .pixels()
        .map(|p| (luma(*p) * 255.0).round() as u8)
        .collect()
}

/// The value at `permille` of the sorted values (lower rank).
fn percentile(values: &mut [u8], permille: u32) -> u8 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let rank = (values.len() - 1) * permille as usize / 1000;
    values[rank]
}

/// The black check of plan §6.4 on one picture.
///
/// # Errors
/// [`EffectError::BlackCheckFailed`] naming `element` and `frame`: a border side with more than
/// [`MAX_LIT_BORDER_PERMILLE`] of its band above [`BLACK_LEVEL`], or a darkest-5 % level above
/// it.
pub fn check_black(
    image: &RgbaImage,
    element: &str,
    frame: Option<usize>,
) -> Result<BlackCheckReport, EffectError> {
    let (width, height) = image.dimensions();
    let band = (width.min(height) / BORDER_BAND_DIVISOR)
        .max(2)
        .min(width.min(height));
    let levels = luma_levels(image);
    let at = |x: u32, y: u32| levels[(y as usize) * (width as usize) + x as usize];
    let lit_share = |pixels: &mut dyn Iterator<Item = u8>| {
        let (mut lit, mut total) = (0_u64, 0_u64);
        for level in pixels {
            total += 1;
            lit += u64::from(level > BLACK_LEVEL);
        }
        (lit * 1000).div_ceil(total.max(1)) as u32
    };
    let sides = [
        lit_share(
            &mut (0..band)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .map(|(x, y)| at(x, y)),
        ),
        lit_share(
            &mut (height - band..height)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .map(|(x, y)| at(x, y)),
        ),
        lit_share(
            &mut (0..height)
                .flat_map(|y| (0..band).map(move |x| (x, y)))
                .map(|(x, y)| at(x, y)),
        ),
        lit_share(
            &mut (0..height)
                .flat_map(|y| (width - band..width).map(move |x| (x, y)))
                .map(|(x, y)| at(x, y)),
        ),
    ];
    let mut sorted = levels.clone();
    let darkest_level = percentile(&mut sorted, DARKEST_PERCENT * 10);
    let report = BlackCheckReport {
        band_px: band,
        lit_border_permille: sides,
        darkest_level,
    };
    let fail = |reason: String| EffectError::BlackCheckFailed {
        element: element.to_string(),
        frame,
        reason,
    };
    for (name, permille) in ["top", "bottom", "left", "right"].iter().zip(sides) {
        if permille > MAX_LIT_BORDER_PERMILLE {
            return Err(fail(format!(
                "{permille}/1000 of the {name} border band ({band} px) is above {BLACK_LEVEL}/255 (limit {MAX_LIT_BORDER_PERMILLE}/1000): a lit floor, horizon or ray reaches the edge"
            )));
        }
    }
    if darkest_level > BLACK_LEVEL {
        return Err(fail(format!(
            "the darkest {DARKEST_PERCENT} % reach {darkest_level}/255, above {BLACK_LEVEL}/255: the background is not black"
        )));
    }
    Ok(report)
}

/// The levels of a luminance key on `pictures`: black point [`BLACK_LEVEL`], white point the
/// [`WHITE_POINT_PERMILLE`] percentile of the pixels above it (at least [`MIN_WHITE_POINT`]).
///
/// # Errors
/// [`EffectError::EmptyElement`] when no pixel is above the black point.
pub fn measure_levels(pictures: &[&RgbaImage], element: &str) -> Result<Levels, EffectError> {
    let mut lit: Vec<u8> = pictures
        .iter()
        .flat_map(|picture| luma_levels(picture))
        .filter(|level| *level > BLACK_LEVEL)
        .collect();
    if lit.is_empty() {
        return Err(EffectError::EmptyElement {
            element: element.to_string(),
            reason: format!("no pixel is brighter than {BLACK_LEVEL}/255"),
        });
    }
    let white = percentile(&mut lit, WHITE_POINT_PERMILLE).max(MIN_WHITE_POINT);
    Ok(Levels {
        black: BLACK_LEVEL,
        white,
    })
}

/// Luminance after `levels`, in `[0, 1]`.
fn leveled(pixel: image::Rgba<u8>, levels: Levels) -> f32 {
    let black = f32::from(levels.black) / 255.0;
    let white = f32::from(levels.white) / 255.0;
    ((luma(pixel) - black) / (white - black)).clamp(0.0, 1.0)
}

/// The luminance key for an emissive element: shading and density are the leveled luminance,
/// and the emissive mask is 1 wherever the element covers anything.
#[must_use]
pub fn luminance_emissive(image: &RgbaImage, levels: Levels) -> KeyedElement {
    let (width, height) = image.dimensions();
    let values: Vec<f32> = image.pixels().map(|p| leveled(*p, levels)).collect();
    let emissive = values
        .iter()
        .map(|v| if *v > 0.0 { 1.0 } else { 0.0 })
        .collect();
    KeyedElement {
        shading: Plane {
            width,
            height,
            data: values.clone(),
        },
        density: Plane {
            width,
            height,
            data: values,
        },
        emissive: Plane {
            width,
            height,
            data: emissive,
        },
    }
}

/// Leveled luminance over which the silhouette key's density rises from 0 to 1: the
/// anti-aliased rim of an opaque element against pure black.
pub const SILHOUETTE_BAND: f32 = 0.04;

/// The silhouette key for an opaque, hard-edged lit element on pure black (chitin, rock):
/// `density = smoothstep(0, SILHOUETTE_BAND, l)` and `shading = l`, with `l` the leveled
/// luminance, so dark parts of the element stay opaque and keep their dark shading; nothing
/// is emissive. The black check guarantees the background is at most the black point.
#[must_use]
pub fn silhouette_key(image: &RgbaImage, levels: Levels) -> KeyedElement {
    let (width, height) = image.dimensions();
    let mut shading = Plane::zeros(width, height);
    let mut density = Plane::zeros(width, height);
    for (index, pixel) in image.pixels().enumerate() {
        let l = leveled(*pixel, levels);
        let t = (l / SILHOUETTE_BAND).clamp(0.0, 1.0);
        density.data[index] = t * t * (3.0 - 2.0 * t);
        shading.data[index] = l;
    }
    KeyedElement {
        shading,
        density,
        emissive: Plane::zeros(width, height),
    }
}

/// The luminance key fallback for a lit semi-transparent element without a pair:
/// `density = smoothstep(0, 1, l)` and `shading = min(1, l / density)` with `l` the leveled
/// luminance; nothing is emissive.
#[must_use]
pub fn luminance_key(image: &RgbaImage, levels: Levels) -> KeyedElement {
    let (width, height) = image.dimensions();
    let mut shading = Plane::zeros(width, height);
    let mut density = Plane::zeros(width, height);
    for (index, pixel) in image.pixels().enumerate() {
        let l = leveled(*pixel, levels);
        let d = l * l * (3.0 - 2.0 * l);
        density.data[index] = d;
        shading.data[index] = if d > 0.0 { (l / d).min(1.0) } else { 0.0 };
    }
    KeyedElement {
        shading,
        density,
        emissive: Plane::zeros(width, height),
    }
}

/// The difference matte of a pair: per pixel, each channel's `alpha_k = 1 - (W_k - B_k)`, the
/// alpha their mean, the residual the largest departure of a channel from it (or of the mean
/// from `[0, 1]`); `colour = B / alpha`, shading its luminance. Nothing is emissive.
///
/// # Errors
/// [`EffectError::PairSizeMismatch`] for pictures of different sizes, and
/// [`EffectError::MatteRejected`] when more than [`MATTE_OVER_LIMIT_PERMILLE`] of the pixels
/// have a residual above [`MATTE_RESIDUAL_LIMIT`].
pub fn difference_matte(
    black: &RgbaImage,
    white: &RgbaImage,
    element: &str,
) -> Result<(KeyedElement, MatteReport), EffectError> {
    if black.dimensions() != white.dimensions() {
        return Err(EffectError::PairSizeMismatch {
            element: element.to_string(),
            black: black.dimensions(),
            white: white.dimensions(),
        });
    }
    let (width, height) = black.dimensions();
    let mut shading = Plane::zeros(width, height);
    let mut density = Plane::zeros(width, height);
    let (mut over, mut max_residual) = (0_u64, 0_u8);
    for (index, (b, w)) in black.pixels().zip(white.pixels()).enumerate() {
        let channel_alpha: [f32; 3] =
            std::array::from_fn(|k| 1.0 - (f32::from(w[k]) - f32::from(b[k])) / 255.0);
        let mean = channel_alpha.iter().sum::<f32>() / 3.0;
        let alpha = mean.clamp(0.0, 1.0);
        let spread = channel_alpha
            .iter()
            .map(|a| (a - mean).abs())
            .fold((mean - alpha).abs(), f32::max);
        let residual = (spread * 255.0).round().min(255.0) as u8;
        max_residual = max_residual.max(residual);
        over += u64::from(residual > MATTE_RESIDUAL_LIMIT);
        density.data[index] = alpha;
        shading.data[index] = if alpha > 0.5 / 255.0 {
            (luma(*b) / alpha).min(1.0)
        } else {
            0.0
        };
    }
    let pixels = u64::from(width) * u64::from(height);
    let report = MatteReport {
        over_limit: over,
        pixels,
        max_residual,
    };
    if over * 1000 > pixels * MATTE_OVER_LIMIT_PERMILLE {
        return Err(EffectError::MatteRejected {
            element: element.to_string(),
            over,
            total: pixels,
            max_residual,
        });
    }
    Ok((
        KeyedElement {
            shading,
            density,
            emissive: Plane::zeros(width, height),
        },
        report,
    ))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use image::Rgba;

    fn flat(width: u32, height: u32, level: u8) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba([level, level, level, 255]))
    }

    #[test]
    fn a_black_picture_with_a_centred_element_passes() {
        let mut image = flat(64, 64, 0);
        for y in 24..40 {
            for x in 24..40 {
                image.put_pixel(x, y, Rgba([200, 200, 200, 255]));
            }
        }
        let report = check_black(&image, "e", None).expect("passes");
        assert_eq!(report.lit_border_permille, [0; 4]);
        assert_eq!(report.band_px, 2);
    }

    #[test]
    fn a_grey_background_fails_the_darkest_rule() {
        assert!(matches!(
            check_black(&flat(64, 64, 6), "e", Some(3)),
            Err(EffectError::BlackCheckFailed { frame: Some(3), .. })
        ));
    }

    #[test]
    fn levels_stretch_the_brightest_lit_pixels_to_one() {
        let mut image = flat(10, 10, 0);
        image.put_pixel(5, 5, Rgba([128, 128, 128, 255]));
        let levels = measure_levels(&[&image], "e").expect("lit");
        assert_eq!(levels.white, 128);
        let keyed = luminance_emissive(&image, levels);
        assert!((keyed.density.data[55] - 1.0).abs() < 1e-6);
        assert_eq!(keyed.emissive.data[55], 1.0);
        assert_eq!(keyed.emissive.data[0], 0.0);
        assert!(matches!(
            measure_levels(&[&flat(4, 4, 0)], "e"),
            Err(EffectError::EmptyElement { .. })
        ));
    }

    #[test]
    fn the_luminance_key_fallback_keeps_shading_at_most_one() {
        let mut image = flat(4, 4, 0);
        image.put_pixel(1, 1, Rgba([60, 60, 60, 255]));
        let keyed = luminance_key(
            &image,
            Levels {
                black: 3,
                white: 255,
            },
        );
        assert!(keyed.density.data[5] > 0.0 && keyed.density.data[5] < 0.2);
        assert!(keyed.shading.data[5] <= 1.0);
        assert_eq!(keyed.emissive.data[5], 0.0);
    }

    #[test]
    fn the_silhouette_key_keeps_dark_parts_opaque_and_dark() {
        let mut image = flat(4, 4, 0);
        image.put_pixel(1, 1, Rgba([16, 16, 16, 255]));
        image.put_pixel(2, 2, Rgba([230, 230, 230, 255]));
        image.put_pixel(3, 3, Rgba([5, 5, 5, 255]));
        let levels = Levels {
            black: 3,
            white: 230,
        };
        let keyed = silhouette_key(&image, levels);
        assert!(
            (keyed.density.data[5] - 1.0).abs() < 1e-6,
            "a dark body pixel is opaque"
        );
        assert!(keyed.shading.data[5] < 0.1, "and keeps its dark shading");
        assert!((keyed.density.data[10] - 1.0).abs() < 1e-6);
        assert!((keyed.shading.data[10] - 1.0).abs() < 1e-6);
        assert!(
            keyed.density.data[15] > 0.0 && keyed.density.data[15] < 1.0,
            "an anti-aliased rim pixel is partly covered"
        );
        assert_eq!(keyed.density.data[0], 0.0, "black stays empty");
        assert!(keyed.emissive.data.iter().all(|value| *value == 0.0));
    }
}
