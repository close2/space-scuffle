//! Offline authored noise and erosion sheets (`asset_baker effect-noise`, plan
//! `2026-11-09-effect-visualization.md` §6.2 item 3).
//!
//! # Responsibility
//! Generates square sheets from recorded parameters ([`NoiseElementSpec`]): tileable value,
//! Perlin, Worley and curl noise (fractal sums of octaves), radial erosion gradients and ring
//! masks, one [`ChannelSpec`] per RGBA channel. A noise recipe ([`NoiseRecipe`]) names a set
//! of such sheets; [`crate::effects::texture_set`] writes them with the provenance
//! `Authored { tool: "asset_baker effect-noise", params }`.
//!
//! # How it fits
//! Noise is what the effect shaders need for erosion, distortion and flicker; it is authored
//! data, not a stand-in for media (charter §1.2 bans procedural *flipbooks* posing as media).
//! The same generator also supplies the erosion-order noise of every baked element
//! ([`crate::effects::packing`]).
//!
//! # Invariants
//! - **Deterministic on every host.** Only integer hashing, `+ - * /` and `sqrt` (all exactly
//!   rounded under IEEE 754) are used, never a host transcendental function, so the same
//!   parameters give the same bytes everywhere; the integrity test regenerates shipped sheets
//!   and compares pixels.
//! - **Tileable.** Lattice noise wraps at its `period` and Worley cells wrap at `cells`, so the
//!   left edge continues the right one and the top the bottom. Erosion gradients and ring
//!   masks are centred shapes and are not meant to tile.
//! - Parameters are integers (per-mille fractions), so a record round-trips exactly.

// Pixel coordinates and channel values are converted between integers and `f64` throughout;
// every conversion is of a value bounded by the sheet size (at most 4096) or by 255.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]

use super::EffectError;
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The `tool` an authored sheet's provenance names.
pub const EFFECT_NOISE_TOOL: &str = "asset_baker effect-noise";

/// Largest sheet side accepted, in pixels.
pub const MAX_SHEET_SIZE: u32 = 4096;

/// Most octaves of a fractal sum.
pub const MAX_OCTAVES: u8 = 8;

/// `1 / sqrt(2)`, the length of each component of a diagonal unit gradient.
const DIAGONAL: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// The eight unit gradients of the Perlin lattice.
const GRADIENTS: [(f64, f64); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (DIAGONAL, DIAGONAL),
    (-DIAGONAL, DIAGONAL),
    (DIAGONAL, -DIAGONAL),
    (-DIAGONAL, -DIAGONAL),
];

/// A fractal sum of lattice noise: `octaves` layers, the first with `period` lattice cells
/// across the sheet, each next one with twice the cells and `gain_permille / 1000` of the
/// amplitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fractal {
    /// Seed of the lattice hash.
    pub seed: u64,
    /// Lattice cells across the sheet in the first octave (at least 1).
    pub period: u32,
    /// Number of octaves, 1 to [`MAX_OCTAVES`].
    pub octaves: u8,
    /// Amplitude of each octave relative to the previous one, in thousandths.
    pub gain_permille: u32,
}

/// Which component of the curl of a Perlin potential a channel holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CurlComponent {
    /// `dpsi/dy`, encoded as `0.5 + 0.5 * v / max`.
    X,
    /// `-dpsi/dx`, encoded as `0.5 + 0.5 * v / max`.
    Y,
}

/// What one channel of a sheet holds. Every value is written as `round(255 * v)` with `v` in
/// `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelSpec {
    /// The same value everywhere.
    Constant(u8),
    /// Value noise: smoothly interpolated random lattice values.
    Value(Fractal),
    /// Perlin gradient noise, `0.5 + 0.5 * sqrt(2) * n`, clamped.
    Perlin(Fractal),
    /// Worley (cellular) noise: distance to the nearest feature point of `cells` x `cells`
    /// wrapping cells, scaled so one cell side is 1; `invert` writes `1 - d`.
    Worley {
        /// Seed of the feature points.
        seed: u64,
        /// Cells across the sheet (at least 1).
        cells: u32,
        /// Whether to write `1 - d`.
        invert: bool,
    },
    /// One component of the curl of a Perlin potential, a divergence-free flow; both
    /// components of one potential share their scale.
    Curl {
        /// The potential.
        potential: Fractal,
        /// Which component.
        component: CurlComponent,
    },
    /// A radial erosion gradient: 1 at the centre falling to 0 at the inscribed circle,
    /// perturbed by Perlin noise of `noise_permille / 1000` amplitude, so a threshold eats the
    /// shape from its rim inwards with a ragged edge.
    ErosionGradient {
        /// The perturbing noise.
        noise: Fractal,
        /// Amplitude of the perturbation, in thousandths.
        noise_permille: u32,
    },
    /// A ring mask: 1 between `inner_permille` and `outer_permille` of the inscribed radius,
    /// falling to 0 over `softness_permille` on each side (smoothstep).
    RingMask {
        /// Inner radius, in thousandths of the inscribed radius.
        inner_permille: u32,
        /// Outer radius, in thousandths of the inscribed radius.
        outer_permille: u32,
        /// Width of each soft edge, in thousandths of the inscribed radius.
        softness_permille: u32,
    },
}

impl ChannelSpec {
    /// A short label of the channel's content, written into the manifest.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Constant(_) => "constant",
            Self::Value(_) => "value",
            Self::Perlin(_) => "perlin",
            Self::Worley { .. } => "worley",
            Self::Curl {
                component: CurlComponent::X,
                ..
            } => "curl_x",
            Self::Curl {
                component: CurlComponent::Y,
                ..
            } => "curl_y",
            Self::ErosionGradient { .. } => "erosion_gradient",
            Self::RingMask { .. } => "ring_mask",
        }
    }
}

/// One authored sheet: its element name and the content of its R, G, B and A channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoiseChannels {
    /// Red.
    pub r: ChannelSpec,
    /// Green.
    pub g: ChannelSpec,
    /// Blue.
    pub b: ChannelSpec,
    /// Alpha.
    pub a: ChannelSpec,
}

impl NoiseChannels {
    /// The four channels in RGBA order.
    #[must_use]
    pub fn all(&self) -> [ChannelSpec; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// One element of a noise recipe; also the `params` of its `Authored` provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoiseElementSpec {
    /// Element name (see [`super::check_name`]).
    pub name: String,
    /// The channels.
    pub channels: NoiseChannels,
}

/// A noise recipe file: one set of authored sheets, each `cell_size` pixels square.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoiseRecipe {
    /// Set name.
    pub set: String,
    /// Side of every sheet, in pixels (a power of two from 16 to [`MAX_SHEET_SIZE`]).
    pub cell_size: u32,
    /// The sheets, in layer order.
    pub elements: Vec<NoiseElementSpec>,
}

impl NoiseRecipe {
    /// Loads and checks a recipe.
    ///
    /// # Errors
    /// [`EffectError::Io`], [`EffectError::Parse`], or [`EffectError::InvalidRecipe`] /
    /// [`EffectError::InvalidName`] for a recipe [`Self::check`] refuses.
    pub fn load(path: &Path) -> Result<Self, EffectError> {
        let text = std::fs::read_to_string(path).map_err(|source| EffectError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let recipe: Self = ron::from_str(&text).map_err(|source| EffectError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        recipe.check(path)?;
        Ok(recipe)
    }

    /// Checks names, size and every channel's parameters.
    ///
    /// # Errors
    /// [`EffectError::InvalidName`] or [`EffectError::InvalidRecipe`] naming `path`.
    pub fn check(&self, path: &Path) -> Result<(), EffectError> {
        super::check_name("set", &self.set)?;
        let invalid = |reason: String| EffectError::InvalidRecipe {
            path: path.to_path_buf(),
            reason,
        };
        if !self.cell_size.is_power_of_two() || !(16..=MAX_SHEET_SIZE).contains(&self.cell_size) {
            return Err(invalid(format!(
                "cell_size {} is not a power of two from 16 to {MAX_SHEET_SIZE}",
                self.cell_size
            )));
        }
        if self.elements.is_empty() {
            return Err(invalid("the recipe has no elements".to_string()));
        }
        let mut seen = std::collections::BTreeSet::new();
        for element in &self.elements {
            super::check_name("element", &element.name)?;
            if !seen.insert(element.name.as_str()) {
                return Err(invalid(format!("element '{}' appears twice", element.name)));
            }
            for channel in element.channels.all() {
                check_channel(&channel)
                    .map_err(|reason| invalid(format!("element '{}': {reason}", element.name)))?;
            }
        }
        Ok(())
    }
}

/// Checks one channel's parameters, returning the reason it is refused.
fn check_channel(channel: &ChannelSpec) -> Result<(), String> {
    let fractal = |f: &Fractal| {
        if f.period == 0 || f.period > MAX_SHEET_SIZE {
            Err(format!(
                "period {} is not in 1..={MAX_SHEET_SIZE}",
                f.period
            ))
        } else if f.octaves == 0 || f.octaves > MAX_OCTAVES {
            Err(format!("octaves {} is not in 1..={MAX_OCTAVES}", f.octaves))
        } else if f.gain_permille == 0 || f.gain_permille > 1000 {
            Err(format!(
                "gain_permille {} is not in 1..=1000",
                f.gain_permille
            ))
        } else {
            Ok(())
        }
    };
    match channel {
        ChannelSpec::Constant(_) => Ok(()),
        ChannelSpec::Value(f) | ChannelSpec::Perlin(f) => fractal(f),
        ChannelSpec::Curl { potential, .. } => fractal(potential),
        ChannelSpec::ErosionGradient {
            noise,
            noise_permille,
        } => {
            fractal(noise)?;
            if *noise_permille > 1000 {
                return Err(format!("noise_permille {noise_permille} is above 1000"));
            }
            Ok(())
        }
        ChannelSpec::Worley { cells, .. } => {
            if *cells == 0 || *cells > MAX_SHEET_SIZE {
                Err(format!("cells {cells} is not in 1..={MAX_SHEET_SIZE}"))
            } else {
                Ok(())
            }
        }
        ChannelSpec::RingMask {
            inner_permille,
            outer_permille,
            softness_permille,
        } => {
            if inner_permille >= outer_permille || *outer_permille > 1000 {
                Err(format!(
                    "ring radii {inner_permille}..{outer_permille} must rise and end at most at 1000"
                ))
            } else if *softness_permille > 500 {
                Err(format!(
                    "softness_permille {softness_permille} is above 500"
                ))
            } else {
                Ok(())
            }
        }
    }
}

/// `splitmix64` finaliser: a bijective 64-bit mix with full avalanche.
#[must_use]
pub fn mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A hash of a lattice point under `seed`.
fn lattice_hash(seed: u64, x: u32, y: u32) -> u64 {
    mix64(seed ^ mix64((u64::from(x) << 32) | u64::from(y)))
}

/// A hash as a value in `[0, 1)`, from its top 53 bits.
fn unit(hash: u64) -> f64 {
    (hash >> 11) as f64 / (1_u64 << 53) as f64
}

/// The quintic fade `6t^5 - 15t^4 + 10t^3`.
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Linear interpolation.
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Hermite smoothstep of `x` between `edge0` and `edge1`.
#[must_use]
pub fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    if edge1 <= edge0 {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The lattice cell and the position inside it of `u` in `[0, 1)` on a lattice of `period`
/// cells that wraps.
fn cell_of(u: f64, period: u32) -> (u32, u32, f64) {
    let scaled = u * f64::from(period);
    let floor = scaled.floor();
    let cell = (floor as i64).rem_euclid(i64::from(period)) as u32;
    let next = (cell + 1) % period;
    (cell, next, scaled - floor)
}

/// Value noise in `[0, 1]` at `(u, v)` in sheet units, one octave.
fn value_octave(seed: u64, period: u32, u: f64, v: f64) -> f64 {
    let (x0, x1, fx) = cell_of(u, period);
    let (y0, y1, fy) = cell_of(v, period);
    let corner = |x, y| unit(lattice_hash(seed, x, y));
    let (sx, sy) = (fade(fx), fade(fy));
    lerp(
        lerp(corner(x0, y0), corner(x1, y0), sx),
        lerp(corner(x0, y1), corner(x1, y1), sx),
        sy,
    )
}

/// Perlin noise in about `[-0.71, 0.71]` at `(u, v)` in sheet units, one octave.
fn perlin_octave(seed: u64, period: u32, u: f64, v: f64) -> f64 {
    let (x0, x1, fx) = cell_of(u, period);
    let (y0, y1, fy) = cell_of(v, period);
    let dot = |x, y, dx: f64, dy: f64| {
        let (gx, gy) = GRADIENTS[(lattice_hash(seed, x, y) & 7) as usize];
        gx * dx + gy * dy
    };
    let (sx, sy) = (fade(fx), fade(fy));
    lerp(
        lerp(dot(x0, y0, fx, fy), dot(x1, y0, fx - 1.0, fy), sx),
        lerp(
            dot(x0, y1, fx, fy - 1.0),
            dot(x1, y1, fx - 1.0, fy - 1.0),
            sx,
        ),
        sy,
    )
}

/// A fractal sum of `octave`, normalised by the total amplitude.
fn fractal_sum(f: &Fractal, u: f64, v: f64, octave: fn(u64, u32, f64, f64) -> f64) -> f64 {
    let gain = f64::from(f.gain_permille) / 1000.0;
    let (mut total, mut amplitude, mut norm, mut period) = (0.0, 1.0, 0.0, f.period);
    for index in 0..f.octaves {
        total += amplitude * octave(f.seed.wrapping_add(u64::from(index)), period, u, v);
        norm += amplitude;
        amplitude *= gain;
        period = period.saturating_mul(2);
    }
    total / norm
}

/// Fractal value noise in `[0, 1]`.
#[must_use]
pub fn value_noise(f: &Fractal, u: f64, v: f64) -> f64 {
    fractal_sum(f, u, v, value_octave)
}

/// Fractal Perlin noise mapped to `[0, 1]`.
#[must_use]
pub fn perlin_noise(f: &Fractal, u: f64, v: f64) -> f64 {
    (0.5 + 0.5 * std::f64::consts::SQRT_2 * fractal_sum(f, u, v, perlin_octave)).clamp(0.0, 1.0)
}

/// Worley F1 distance at `(u, v)`, one cell side = 1, clamped to `[0, 1]`.
#[must_use]
pub fn worley(seed: u64, cells: u32, u: f64, v: f64) -> f64 {
    let (cx, _, fx) = cell_of(u, cells);
    let (cy, _, fy) = cell_of(v, cells);
    let n = i64::from(cells);
    let mut best = f64::INFINITY;
    for dy in -1_i64..=1 {
        for dx in -1_i64..=1 {
            let x = (i64::from(cx) + dx).rem_euclid(n) as u32;
            let y = (i64::from(cy) + dy).rem_euclid(n) as u32;
            let hash = lattice_hash(seed, x, y);
            let px = dx as f64 + unit(hash);
            let py = dy as f64 + unit(mix64(hash));
            let (ex, ey) = (px - fx, py - fy);
            best = best.min((ex * ex + ey * ey).sqrt());
        }
    }
    best.clamp(0.0, 1.0)
}

/// Generates one sheet of `size` pixels square. Pixel `(x, y)` samples the sheet at its
/// centre, `((x + 0.5) / size, (y + 0.5) / size)`.
#[must_use]
pub fn generate_sheet(channels: &NoiseChannels, size: u32) -> RgbaImage {
    let planes: Vec<Vec<f64>> = channels
        .all()
        .iter()
        .map(|channel| channel_plane(channel, size))
        .collect();
    let mut image = RgbaImage::new(size, size);
    for (index, pixel) in image.pixels_mut().enumerate() {
        let value = |plane: &Vec<f64>| (plane[index].clamp(0.0, 1.0) * 255.0).round() as u8;
        *pixel = Rgba([
            value(&planes[0]),
            value(&planes[1]),
            value(&planes[2]),
            value(&planes[3]),
        ]);
    }
    image
}

/// The values of one channel, row by row.
fn channel_plane(channel: &ChannelSpec, size: u32) -> Vec<f64> {
    let side = f64::from(size);
    let coords = move |index: usize| {
        let x = (index as u32) % size;
        let y = (index as u32) / size;
        ((f64::from(x) + 0.5) / side, (f64::from(y) + 0.5) / side)
    };
    let count = (size as usize) * (size as usize);
    match *channel {
        ChannelSpec::Constant(value) => vec![f64::from(value) / 255.0; count],
        ChannelSpec::Value(f) => (0..count)
            .map(|i| {
                let (u, v) = coords(i);
                value_noise(&f, u, v)
            })
            .collect(),
        ChannelSpec::Perlin(f) => (0..count)
            .map(|i| {
                let (u, v) = coords(i);
                perlin_noise(&f, u, v)
            })
            .collect(),
        ChannelSpec::Worley {
            seed,
            cells,
            invert,
        } => (0..count)
            .map(|i| {
                let (u, v) = coords(i);
                let d = worley(seed, cells, u, v);
                if invert {
                    1.0 - d
                } else {
                    d
                }
            })
            .collect(),
        ChannelSpec::Curl {
            potential,
            component,
        } => curl_plane(&potential, component, size),
        ChannelSpec::ErosionGradient {
            noise,
            noise_permille,
        } => (0..count)
            .map(|i| {
                let (u, v) = coords(i);
                let radius = radial(u, v);
                let jitter = (perlin_noise(&noise, u, v) - 0.5) * f64::from(noise_permille) / 500.0;
                (1.0 - radius + jitter).clamp(0.0, 1.0)
            })
            .collect(),
        ChannelSpec::RingMask {
            inner_permille,
            outer_permille,
            softness_permille,
        } => {
            let (inner, outer, soft) = (
                f64::from(inner_permille) / 1000.0,
                f64::from(outer_permille) / 1000.0,
                f64::from(softness_permille) / 1000.0,
            );
            (0..count)
                .map(|i| {
                    let (u, v) = coords(i);
                    let r = radial(u, v);
                    smoothstep(inner - soft, inner, r) * (1.0 - smoothstep(outer, outer + soft, r))
                })
                .collect()
        }
    }
}

/// Distance of `(u, v)` from the sheet centre, 1 at the inscribed circle.
fn radial(u: f64, v: f64) -> f64 {
    let (dx, dy) = (u - 0.5, v - 0.5);
    (dx * dx + dy * dy).sqrt() * 2.0
}

/// One component of the curl of the Perlin potential, by central differences of one pixel,
/// scaled by the largest magnitude of either component over the sheet.
fn curl_plane(potential: &Fractal, component: CurlComponent, size: u32) -> Vec<f64> {
    let side = f64::from(size);
    let h = 1.0 / side;
    let psi = |u: f64, v: f64| perlin_noise(potential, u.rem_euclid(1.0), v.rem_euclid(1.0));
    let count = (size as usize) * (size as usize);
    let mut flow = Vec::with_capacity(count);
    let mut largest = 0.0_f64;
    for index in 0..count {
        let x = (index as u32) % size;
        let y = (index as u32) / size;
        let (u, v) = ((f64::from(x) + 0.5) / side, (f64::from(y) + 0.5) / side);
        let ddy = (psi(u, v + h) - psi(u, v - h)) / (2.0 * h);
        let ddx = (psi(u + h, v) - psi(u - h, v)) / (2.0 * h);
        largest = largest.max(ddy.abs()).max(ddx.abs());
        flow.push((ddy, -ddx));
    }
    let scale = if largest > 0.0 { 0.5 / largest } else { 0.0 };
    flow.into_iter()
        .map(|(cx, cy)| {
            let value = match component {
                CurlComponent::X => cx,
                CurlComponent::Y => cy,
            };
            0.5 + value * scale
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRACTAL: Fractal = Fractal {
        seed: 7,
        period: 4,
        octaves: 3,
        gain_permille: 500,
    };

    #[test]
    fn lattice_noise_wraps_at_its_period() {
        for v in [0.1, 0.37, 0.9] {
            for noise in [value_noise, perlin_noise] {
                let left = noise(&FRACTAL, 0.0, v);
                let right = noise(&FRACTAL, 1.0 - 1e-12, v);
                assert!((left - right).abs() < 1e-6, "{left} {right}");
            }
        }
    }

    #[test]
    fn worley_is_zero_at_a_feature_point_and_wraps() {
        let a = worley(3, 5, 0.0, 0.42);
        let b = worley(3, 5, 1.0 - 1e-12, 0.42);
        assert!((a - b).abs() < 1e-6);
        assert!((0.0..=1.0).contains(&a));
    }

    #[test]
    fn ring_mask_is_one_inside_the_band_and_zero_outside() {
        let ring = ChannelSpec::RingMask {
            inner_permille: 400,
            outer_permille: 600,
            softness_permille: 50,
        };
        let plane = channel_plane(&ring, 64);
        // Centre pixel (r = 0) and a corner (r > 1) are outside the band.
        assert_eq!(plane[32 * 64 + 32], 0.0);
        assert_eq!(plane[0], 0.0);
        // r = 0.5 lies in the band: pixel x = 48 is (48.5 - 32) / 32 = 0.52 from the centre.
        assert!(plane[32 * 64 + 48] > 0.99, "{}", plane[32 * 64 + 48]);
    }

    #[test]
    fn invalid_channel_parameters_are_refused() {
        let bad = Fractal {
            octaves: 0,
            ..FRACTAL
        };
        assert!(check_channel(&ChannelSpec::Perlin(bad)).is_err());
        assert!(check_channel(&ChannelSpec::Worley {
            seed: 0,
            cells: 0,
            invert: false
        })
        .is_err());
        assert!(check_channel(&ChannelSpec::RingMask {
            inner_permille: 600,
            outer_permille: 400,
            softness_permille: 10
        })
        .is_err());
        assert!(check_channel(&ChannelSpec::Perlin(FRACTAL)).is_ok());
    }

    #[test]
    fn curl_components_share_a_scale_and_stay_in_range() {
        let x = curl_plane(&FRACTAL, CurlComponent::X, 32);
        let y = curl_plane(&FRACTAL, CurlComponent::Y, 32);
        let peak = x
            .iter()
            .chain(&y)
            .map(|v| (v - 0.5).abs())
            .fold(0.0, f64::max);
        assert!((peak - 0.5).abs() < 1e-9, "{peak}");
    }
}
