//! Image processing algorithms for terrain themes and PBR material derivation.
//!
//! # Architectural Charter
//! In accordance with `AGENTS.md` and
//! Section 4.2 of `docs/plans/21b-plan-map-editor.md`, `terrain_texture` provides authentic,
//! mathematically rigorous image processing algorithms to prepare AI-generated source art
//! for the dual-grid marching squares terrain renderer.
//!
//! ## Separable Two-Pass Tileability (`make_tileable`)
//! A naive single-pass cross blend is mathematically flawed: where the horizontal and vertical
//! arms of the central cross meet the outer perimeter (e.g. at $(0, size/2)$ or $(size-1, size/2)$),
//! neither image source is valid. The original image `img` does not tile across its exterior
//! borders, while the cyclically offset image `O` possesses an internal seam along its centerline.
//! Fading the mask near the borders inevitably preserves `O`'s internal seam across the boundary band.
//!
//! To eliminate all seams without compromise and prevent smeared "ghosting" artifacts
//! in structured textures (e.g. metal deck panels), this module implements a **separable two-pass**
//! Burt-Adelson multiband blend guided by **Efros-Freeman minimum-error boundary cuts**
//! (Efros & Freeman 2001, "Image Quilting"):
//!
//! 1. **Pass X (Horizontal Tileability)**:
//!    - An image $O_x$ is formed by shifting `img` by $size/2$ along the horizontal axis only.
//!      $O_x$ tiles horizontally across $x = 0 \leftrightarrow size - 1$, with an internal seam only
//!      at column $x = size/2$.
//!    - Within the left overlap band $x \in [size/2 - size/8, size/2 - size/32]$ and right overlap band
//!      $x \in [size/2 + size/32, size/2 + size/8]$, optimal seam paths $cut_{left}(y)$ and $cut_{right}(y)$
//!      are found by dynamic programming minimizing squared photometric difference
//!      $E(x,y) = \sum_{rgb} (a(x,y) - b(x,y))^2$.
//!    - The DP uses a **cyclic two-run approximation** to ensure periodic continuity vertically
//!      ($|cut(0) - cut(size - 1)| \le 1$) in $O(H \cdot K)$ time.
//!    - A hard spatial mask $m_x(x, y) = 1$ for $cut_{left}(y) \le x \le cut_{right}(y)$ (0 elsewhere)
//!      is fed into Burt-Adelson multiband blend, producing intermediate image $H$.
//! 2. **Pass Y (Vertical Tileability)**:
//!    - An image $O_y$ is formed by shifting $H$ by $size/2$ along the vertical axis only.
//!    - Symmetrical top and bottom minimum-error cuts $cut_{top}(x)$ and $cut_{bottom}(x)$ are computed
//!      along rows $y \in [size/2 \pm size/32, size/2 \pm size/8]$, cyclic in $x$.
//!    - Hard spatial mask $m_y(x, y) = 1$ for $cut_{top}(x) \le y \le cut_{bottom}(x)$ (0 elsewhere)
//!      feeds into multiband blend to produce the final seamlessly tileable image.
//!
//! Feeding a hard min-cut mask into the Burt-Adelson Laplacian pyramid prevents cross-fading of
//! high-frequency panel lines (eliminating ghosting) while smoothly merging low-frequency lighting.
//! All pyramid operations are shift-equivariant under periodic wrap-around boundary addressing,
//! guaranteeing zero residual seams in both dimensions.
//!
//! ## Resampling in Linear Space (`load_and_resize`)
//! Gamma-space downsampling darkens high-frequency specular highlights and misrepresents physical
//! radiant flux. Source images are decoded and converted to linear `f32` buffers *prior* to Lanczos3
//! spatial resizing.
//!
//! # Invariants
//! - All image processing routines operate in linear photometric space (`f32` buffers).
//! - Piecewise sRGB encoding/decoding adheres strictly to the IEC 61966-2-1 specification.
//! - All neighborhood sampling uses toroidal wrap-around addressing for seamless tiling.
//! - Zero placeholders, zero dummy fallbacks: missing or invalid source art fails loudly.

// Cast lints: pixel coordinate math, image buffer index conversions and Gaussian kernel strides
// operate on bounded dimensions (256 <= size <= 2048). Numeric casts between u32, usize,
// i32 and f32 are mathematically guaranteed never to overflow or wrap.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::many_single_char_names
)]

use crate::terrain_theme::{
    BakedLayerPaths, TerrainThemeCatalog, TerrainThemeError, ThemeLayer, TilingMode,
};
use image::{ImageBuffer, Luma, Rgba};
use std::fmt;
use std::path::{Path, PathBuf};

/// 32-bit floating-point linear RGBA image buffer.
pub type Rgba32FImage = ImageBuffer<Rgba<f32>, Vec<f32>>;

/// 32-bit floating-point single-channel luminance / height image buffer.
pub type GrayF32Image = ImageBuffer<Luma<f32>, Vec<f32>>;

/// 8-bit unsigned integer sRGB RGBA image buffer.
pub type Rgba8Image = ImageBuffer<Rgba<u8>, Vec<u8>>;

/// Number of pyramid levels used in Burt-Adelson Laplacian pyramid blending.
pub const PYRAMID_LEVELS: usize = 5;

/// Denominator for the seam blend transition half-width (size / 8).
pub const SEAM_HALF_WIDTH_DIVISOR: u32 = 8;

/// Denominator for the central seam plateau core width (size / 32).
///
/// Within $|dist| \le size / 32$ (e.g. $128 \pm 8$ for a $256 \times 256$ image), the spatial
/// blending mask equals 1.0, preserving interior detail exactly without attenuation while
/// smoothstepping down to 0.0 at $|dist| \ge size / 8$.
pub const SEAM_CORE_WIDTH_DIVISOR: u32 = 32;

/// Maximum permitted aspect ratio deviation allowed for source art (2%).
pub const MAX_ASPECT_RATIO_DEVIATION: f32 = 0.02;

/// Standard Rec.709 relative luminance coefficient for the red channel.
pub const LUMA_REC709_R: f32 = 0.2126;

/// Standard Rec.709 relative luminance coefficient for the green channel.
pub const LUMA_REC709_G: f32 = 0.7152;

/// Standard Rec.709 relative luminance coefficient for the blue channel.
pub const LUMA_REC709_B: f32 = 0.0722;

/// Ambient occlusion depression scaling multiplier.
pub const AO_SCALE: f32 = 2.5;

/// Minimum clamp floor for derived ambient occlusion.
pub const AO_MIN_CLAMP: f32 = 0.25;

/// Maximum clamp ceiling for derived ambient occlusion.
pub const AO_MAX_CLAMP: f32 = 1.0;

/// Radius in pixels for local 5x5 window standard deviation calculation.
pub const STDDEV_5X5_RADIUS: i32 = 2;

/// Sentinel seam score returned when interior gradient is zero but a boundary seam exists.
pub const SEAM_SCORE_DISCONTINUITY_SENTINEL: f32 = 100.0;

/// Converts an sRGB channel value in `[0.0, 1.0]` to linear light space
/// using the exact IEC 61966-2-1 piecewise transfer function.
#[must_use]
pub fn srgb_to_linear(s: f32) -> f32 {
    let s_clamped = s.clamp(0.0, 1.0);
    if s_clamped <= 0.04045 {
        s_clamped / 12.92
    } else {
        ((f64::from(s_clamped) + 0.055) / 1.055).powf(2.4) as f32
    }
}

/// Converts a linear light channel value in `[0.0, 1.0]` to sRGB space
/// using the exact IEC 61966-2-1 piecewise transfer function.
#[must_use]
pub fn linear_to_srgb(l: f32) -> f32 {
    let l_clamped = l.clamp(0.0, 1.0);
    if l_clamped <= 0.003_130_8 {
        l_clamped * 12.92
    } else {
        (1.055 * f64::from(l_clamped).powf(1.0 / 2.4) - 0.055) as f32
    }
}

/// Converts an `Rgba32FImage` in linear photometric space to an 8-bit sRGB `Rgba8Image`
/// using the exact IEC 61966-2-1 transfer function.
#[must_use]
pub fn linear_to_srgb_image(linear: &Rgba32FImage) -> Rgba8Image {
    let mut srgb_img = Rgba8Image::new(linear.width(), linear.height());
    for (x, y, pixel) in linear.enumerate_pixels() {
        let r = (linear_to_srgb(pixel[0]) * 255.0).round().clamp(0.0, 255.0) as u8;
        let g = (linear_to_srgb(pixel[1]) * 255.0).round().clamp(0.0, 255.0) as u8;
        let b = (linear_to_srgb(pixel[2]) * 255.0).round().clamp(0.0, 255.0) as u8;
        let a = (pixel[3] * 255.0).round().clamp(0.0, 255.0) as u8;
        srgb_img.put_pixel(x, y, Rgba([r, g, b, a]));
    }
    srgb_img
}

/// Evaluates standard GLSL smoothstep interpolation between `edge0` and `edge1`.
#[must_use]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let span = edge1 - edge0;
    if span.abs() < 1e-7 {
        return 0.0;
    }
    let t = ((x - edge0) / span).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Loads a source image from disk, validates square proportions, converts to linear
/// floating-point space first, and resizes to target resolution via Lanczos3 in linear space.
///
/// # Errors
/// Returns [`TerrainThemeError::MissingSource`] if the input path does not exist,
/// or [`TerrainThemeError::Invalid`] if the image aspect ratio diverges from 1.0 by > 2%.
pub fn load_and_resize(source: &Path, size: u32) -> Result<Rgba32FImage, TerrainThemeError> {
    if !source.exists() {
        return Err(TerrainThemeError::MissingSource(source.to_path_buf()));
    }

    let dynamic_img = image::open(source).map_err(|e| {
        TerrainThemeError::Image(format!(
            "Failed to decode source image at {}: {e}",
            source.display()
        ))
    })?;

    let (w, h) = (dynamic_img.width(), dynamic_img.height());
    let max_dim = w.max(h) as f32;
    let diff = (w as f32 - h as f32).abs() / max_dim;
    if diff > MAX_ASPECT_RATIO_DEVIATION {
        return Err(TerrainThemeError::Invalid {
            theme: "source_art".to_string(),
            reason: format!(
                "Source image at {} is not square: dimensions {w}x{h} deviate by {:.2}% (max allowed: 2%)",
                source.display(),
                diff * 100.0
            ),
        });
    }

    // Convert to linear Rgba32F FIRST, before resampling
    let rgba_u8 = dynamic_img.to_rgba8();
    let mut linear_source = Rgba32FImage::new(w, h);
    for (x, y, pixel) in rgba_u8.enumerate_pixels() {
        let r = srgb_to_linear(f32::from(pixel[0]) / 255.0);
        let g = srgb_to_linear(f32::from(pixel[1]) / 255.0);
        let b = srgb_to_linear(f32::from(pixel[2]) / 255.0);
        let a = f32::from(pixel[3]) / 255.0;
        linear_source.put_pixel(x, y, Rgba([r, g, b, a]));
    }

    // PERFORMANCE RATIONALE:
    // Why: Lanczos3 resampling to the size the image already has costs a full kernel pass and
    // still disperses and rounds every pixel.
    // What: When the source already matches `texture_size`, its linear pixels are used as they are.
    // Trade-off: One more branch, in exchange for skipping the pass and keeping a seamless
    // texture's photometric pixels bit for bit.
    let resized_linear = if w == size && h == size {
        linear_source
    } else {
        image::imageops::resize(
            &linear_source,
            size,
            size,
            image::imageops::FilterType::Lanczos3,
        )
    };

    Ok(resized_linear)
}

/// Combines two images `a` and `b` with spatial mask `m` via Burt-Adelson Laplacian pyramid blending.
///
/// Periodic wrap-around boundary addressing is used across all pyramid reduction and expansion
/// operations, preserving translation equivariance and periodic continuity across domain borders.
fn multiband_blend(a: &Rgba32FImage, b: &Rgba32FImage, m: &GrayF32Image) -> Rgba32FImage {
    let mut gauss_a = Vec::with_capacity(PYRAMID_LEVELS);
    let mut gauss_b = Vec::with_capacity(PYRAMID_LEVELS);
    let mut gauss_m = Vec::with_capacity(PYRAMID_LEVELS);

    gauss_a.push(a.clone());
    gauss_b.push(b.clone());
    gauss_m.push(m.clone());

    for k in 0..(PYRAMID_LEVELS - 1) {
        gauss_a.push(reduce_rgba(&gauss_a[k]));
        gauss_b.push(reduce_rgba(&gauss_b[k]));
        gauss_m.push(reduce_gray(&gauss_m[k]));
    }

    // Build Laplacian pyramids: L_k = G_k - expand(G_{k+1})
    let mut lap_a = Vec::with_capacity(PYRAMID_LEVELS);
    let mut lap_b = Vec::with_capacity(PYRAMID_LEVELS);

    for k in 0..(PYRAMID_LEVELS - 1) {
        let exp_a = expand_rgba(&gauss_a[k + 1]);
        let exp_b = expand_rgba(&gauss_b[k + 1]);
        lap_a.push(subtract_rgba(&gauss_a[k], &exp_a));
        lap_b.push(subtract_rgba(&gauss_b[k], &exp_b));
    }
    lap_a.push(gauss_a[PYRAMID_LEVELS - 1].clone());
    lap_b.push(gauss_b[PYRAMID_LEVELS - 1].clone());

    // Multiband blend: L_blend_k = M_k * L(a)_k + (1 - M_k) * L(b)_k
    let mut lap_blend = Vec::with_capacity(PYRAMID_LEVELS);
    for k in 0..PYRAMID_LEVELS {
        let w = lap_a[k].width();
        let h = lap_a[k].height();
        let mut blended_level = Rgba32FImage::new(w, h);

        for y in 0..h {
            for x in 0..w {
                let mask_val = gauss_m[k].get_pixel(x, y)[0];
                let pa = lap_a[k].get_pixel(x, y);
                let pb = lap_b[k].get_pixel(x, y);

                let r = mask_val * pa[0] + (1.0 - mask_val) * pb[0];
                let g = mask_val * pa[1] + (1.0 - mask_val) * pb[1];
                let b = mask_val * pa[2] + (1.0 - mask_val) * pb[2];
                let alpha = mask_val * pa[3] + (1.0 - mask_val) * pb[3];

                blended_level.put_pixel(x, y, Rgba([r, g, b, alpha]));
            }
        }
        lap_blend.push(blended_level);
    }

    // Collapse blended pyramid: R_{k} = L_blend_k + expand(R_{k+1})
    let mut collapsed = lap_blend[PYRAMID_LEVELS - 1].clone();
    for k in (0..(PYRAMID_LEVELS - 1)).rev() {
        let expanded = expand_rgba(&collapsed);
        collapsed = add_rgba(&lap_blend[k], &expanded);
    }

    for pixel in collapsed.pixels_mut() {
        pixel[0] = pixel[0].clamp(0.0, 1.0);
        pixel[1] = pixel[1].clamp(0.0, 1.0);
        pixel[2] = pixel[2].clamp(0.0, 1.0);
        pixel[3] = pixel[3].clamp(0.0, 1.0);
    }

    collapsed
}

/// Reusable scratch buffers for Efros-Freeman dynamic programming minimum-error cut computation.
///
/// Pre-allocated and reused across vertical and horizontal seam passes to avoid per-pass
/// heap reallocations.
#[derive(Default, Debug, Clone)]
pub struct MinCutBuffers {
    /// Cumulative transition cost grid: flattened matrix of size `sweep_len * band_width`.
    pub cost: Vec<f32>,
    /// Backtrack predecessor indices: flattened matrix of size `sweep_len * band_width`.
    pub parent: Vec<usize>,
}

impl MinCutBuffers {
    /// Creates an empty reusable buffer pool.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Prepares buffers to hold at least `sweep_len * band_width` elements.
    pub fn prepare(&mut self, sweep_len: usize, band_width: usize) {
        let total = sweep_len * band_width;
        if self.cost.len() < total {
            self.cost.resize(total, 0.0);
            self.parent.resize(total, 0);
        }
    }
}

/// Evaluates squared RGB Euclidean photometric difference in linear space:
/// $E(x, y) = \sum_{c \in \{r, g, b\}} (a(x, y)[c] - b(x, y)[c])^2$.
#[inline]
fn pixel_error_squared(a: &Rgba32FImage, b: &Rgba32FImage, x: u32, y: u32) -> f32 {
    let pa = a.get_pixel(x, y);
    let pb = b.get_pixel(x, y);
    let dr = pa[0] - pb[0];
    let dg = pa[1] - pb[1];
    let db = pa[2] - pb[2];
    dr * dr + dg * dg + db * db
}

/// Finds an optimal 8-connected cut through an error grid using a cyclic two-run
/// dynamic programming approximation (Efros & Freeman 2001, "Image Quilting").
///
/// # Cyclic Two-Run Approximation
/// To enforce that the cut wraps continuously across domain boundaries (such that
/// $|cut[0] - cut[sweep\_len - 1]| \le 1$), an exact solver would evaluate independent
/// DP runs for every candidate column in step 0, scaling as $O(K^2 \cdot sweep\_len)$.
///
/// Instead, we evaluate two runs:
/// 1. **Run 1 (Unconstrained)**: All start candidates in step 0 are initialized to their base cost.
///    The forward pass identifies the unconstrained global optimal endpoint $k^*_{end}$ at step `sweep_len - 1`.
/// 2. **Run 2 (Constrained Start)**: Step 0 is constrained strictly to $k^*_{end}$ (all other indices
///    are set to $\infty$). The forward pass is re-evaluated.
///
/// Selecting the final endpoint among the cyclic 8-connected neighbors of $k^*_{end}$
/// ($\{k^*_{end} - 1, k^*_{end}, k^*_{end} + 1\} \cap [0, band\_len - 1]$) mathematically guarantees
/// that $|cut[0] - cut[sweep\_len - 1]| \le 1$. Backtracking produces the optimal cut path.
///
/// # Deterministic Tie-Breaking
/// Candidate predecessors are evaluated in strictly increasing index order ($k - 1, k, k + 1$),
/// breaking ties deterministically toward the smaller index.
fn find_min_cut_grid<F>(
    sweep_len: usize,
    band_len: usize,
    mut cost_fn: F,
    buffers: &mut MinCutBuffers,
) -> Vec<usize>
where
    F: FnMut(usize, usize) -> f32,
{
    assert!(sweep_len >= 2, "sweep_len must be at least 2");
    assert!(band_len >= 1, "band_len must be at least 1");

    buffers.prepare(sweep_len, band_len);

    // --- Run 1: Unconstrained forward pass ---
    for k in 0..band_len {
        buffers.cost[k] = cost_fn(0, k);
    }

    for s in 1..sweep_len {
        let prev_offset = (s - 1) * band_len;
        let curr_offset = s * band_len;

        for k in 0..band_len {
            let mut best_prev_k = if k > 0 { k - 1 } else { k };
            let mut min_prev_cost = buffers.cost[prev_offset + best_prev_k];

            if k > 0 {
                let c_k = buffers.cost[prev_offset + k];
                if c_k < min_prev_cost {
                    min_prev_cost = c_k;
                    best_prev_k = k;
                }
            }
            if k + 1 < band_len {
                let c_next = buffers.cost[prev_offset + k + 1];
                if c_next < min_prev_cost {
                    min_prev_cost = c_next;
                    best_prev_k = k + 1;
                }
            }

            let e = cost_fn(s, k);
            buffers.cost[curr_offset + k] = e + min_prev_cost;
            buffers.parent[curr_offset + k] = best_prev_k;
        }
    }

    let last_offset = (sweep_len - 1) * band_len;
    let mut best_end_k = 0;
    let mut min_end_cost = buffers.cost[last_offset];
    for k in 1..band_len {
        let c = buffers.cost[last_offset + k];
        if c < min_end_cost {
            min_end_cost = c;
            best_end_k = k;
        }
    }

    // --- Run 2: Constrained start pass ---
    for k in 0..band_len {
        if k == best_end_k {
            buffers.cost[k] = cost_fn(0, k);
        } else {
            buffers.cost[k] = f32::INFINITY;
        }
    }

    for s in 1..sweep_len {
        let prev_offset = (s - 1) * band_len;
        let curr_offset = s * band_len;

        for k in 0..band_len {
            let mut best_prev_k = if k > 0 { k - 1 } else { k };
            let mut min_prev_cost = buffers.cost[prev_offset + best_prev_k];

            if k > 0 {
                let c_k = buffers.cost[prev_offset + k];
                if c_k < min_prev_cost {
                    min_prev_cost = c_k;
                    best_prev_k = k;
                }
            }
            if k + 1 < band_len {
                let c_next = buffers.cost[prev_offset + k + 1];
                if c_next < min_prev_cost {
                    min_prev_cost = c_next;
                    best_prev_k = k + 1;
                }
            }

            let e = cost_fn(s, k);
            buffers.cost[curr_offset + k] = e + min_prev_cost;
            buffers.parent[curr_offset + k] = best_prev_k;
        }
    }

    let start_candidate = best_end_k.saturating_sub(1);
    let end_candidate = (best_end_k + 1).min(band_len - 1);
    let mut final_k = start_candidate;
    let mut min_final_cost = buffers.cost[last_offset + final_k];
    for k in (start_candidate + 1)..=end_candidate {
        let c = buffers.cost[last_offset + k];
        if c < min_final_cost {
            min_final_cost = c;
            final_k = k;
        }
    }

    let mut cut = vec![0; sweep_len];
    let mut curr_k = final_k;
    cut[sweep_len - 1] = curr_k;
    for s in (1..sweep_len).rev() {
        curr_k = buffers.parent[s * band_len + curr_k];
        cut[s - 1] = curr_k;
    }

    cut
}

/// Computes an optimal vertical seam cut between images `a` and `b` within column range `[x_min, x_max]`.
///
/// Returns a vector of length `a.height()`, where element `y` contains the chosen cut column $x(y)$.
///
/// # Panics
/// Panics if `x_min > x_max`, if `x_max >= a.width()`, or if `a` and `b` have different dimensions.
#[must_use]
pub fn find_vertical_min_cut(
    a: &Rgba32FImage,
    b: &Rgba32FImage,
    x_min: u32,
    x_max: u32,
    buffers: &mut MinCutBuffers,
) -> Vec<u32> {
    assert!(x_min <= x_max, "x_min must be <= x_max");
    assert!(x_max < a.width(), "x_max must be within image bounds");
    assert_eq!(
        a.dimensions(),
        b.dimensions(),
        "Images must have identical dimensions"
    );

    let sweep_len = a.height() as usize;
    let band_len = (x_max - x_min + 1) as usize;

    let cut_indices = find_min_cut_grid(
        sweep_len,
        band_len,
        |y, k| pixel_error_squared(a, b, x_min + k as u32, y as u32),
        buffers,
    );

    cut_indices.into_iter().map(|k| x_min + k as u32).collect()
}

/// Computes an optimal horizontal seam cut between images `a` and `b` within row range `[y_min, y_max]`.
///
/// Returns a vector of length `a.width()`, where element `x` contains the chosen cut row $y(x)$.
///
/// # Panics
/// Panics if `y_min > y_max`, if `y_max >= a.height()`, or if `a` and `b` have different dimensions.
#[must_use]
pub fn find_horizontal_min_cut(
    a: &Rgba32FImage,
    b: &Rgba32FImage,
    y_min: u32,
    y_max: u32,
    buffers: &mut MinCutBuffers,
) -> Vec<u32> {
    assert!(y_min <= y_max, "y_min must be <= y_max");
    assert!(y_max < a.height(), "y_max must be within image bounds");
    assert_eq!(
        a.dimensions(),
        b.dimensions(),
        "Images must have identical dimensions"
    );

    let sweep_len = a.width() as usize;
    let band_len = (y_max - y_min + 1) as usize;

    let cut_indices = find_min_cut_grid(
        sweep_len,
        band_len,
        |x, k| pixel_error_squared(a, b, x as u32, y_min + k as u32),
        buffers,
    );

    cut_indices.into_iter().map(|k| y_min + k as u32).collect()
}

/// Applies separable two-pass Burt-Adelson Laplacian pyramid blending guided by
/// Efros-Freeman minimum-error boundary cuts to make a texture seamlessly tileable
/// without smeared ghosting or residual interior seams.
#[must_use]
pub fn make_tileable(img: &Rgba32FImage) -> Rgba32FImage {
    let width = img.width();
    let height = img.height();
    let half_w = width / 2;
    let half_h = height / 2;
    let seam_half_width = width / SEAM_HALF_WIDTH_DIVISOR;
    let seam_core_width = width / SEAM_CORE_WIDTH_DIVISOR;

    let mut buffers = MinCutBuffers::new();

    // --- Pass X (Horizontal tileability) ---
    // Ox is shifted by half_w in x only. Ox tiles horizontally; its seam is at column half_w.
    let mut ox = Rgba32FImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let src_x = (x + half_w) % width;
            ox.put_pixel(x, y, *img.get_pixel(src_x, y));
        }
    }

    // Dynamic programming minimum-error cuts for Pass X:
    let cut_left = find_vertical_min_cut(
        img,
        &ox,
        half_w - seam_half_width,
        half_w - seam_core_width,
        &mut buffers,
    );
    let cut_right = find_vertical_min_cut(
        img,
        &ox,
        half_w + seam_core_width,
        half_w + seam_half_width,
        &mut buffers,
    );

    // Hard mask: mx(x, y) = 1 for cut_left[y] <= x <= cut_right[y], else 0.
    let mut mx = GrayF32Image::new(width, height);
    for y in 0..height {
        let cl = cut_left[y as usize];
        let cr = cut_right[y as usize];
        for x in 0..width {
            let val = if x >= cl && x <= cr { 1.0 } else { 0.0 };
            mx.put_pixel(x, y, Luma([val]));
        }
    }

    // H tiles horizontally with no internal seam
    let h_img = multiband_blend(img, &ox, &mx);

    // --- Pass Y (Vertical tileability) ---
    // Oy is shifted by half_h in y only. Each row of Oy is a row of H, so Oy still tiles
    // horizontally and now also vertically across y=0 <-> y=height-1.
    let mut oy = Rgba32FImage::new(width, height);
    for y in 0..height {
        let src_y = (y + half_h) % height;
        for x in 0..width {
            oy.put_pixel(x, y, *h_img.get_pixel(x, src_y));
        }
    }

    // Dynamic programming minimum-error cuts for Pass Y:
    let cut_top = find_horizontal_min_cut(
        &h_img,
        &oy,
        half_h - seam_half_width,
        half_h - seam_core_width,
        &mut buffers,
    );
    let cut_bottom = find_horizontal_min_cut(
        &h_img,
        &oy,
        half_h + seam_core_width,
        half_h + seam_half_width,
        &mut buffers,
    );

    // Hard mask: my(x, y) = 1 for cut_top[x] <= y <= cut_bottom[x], else 0.
    let mut my = GrayF32Image::new(width, height);
    for x in 0..width {
        let ct = cut_top[x as usize];
        let cb = cut_bottom[x as usize];
        for y in 0..height {
            let val = if y >= ct && y <= cb { 1.0 } else { 0.0 };
            my.put_pixel(x, y, Luma([val]));
        }
    }

    // Final result tiles in both axes with no internal seam
    multiband_blend(&h_img, &oy, &my)
}

/// Evaluates relative wrap-around seam quality.
///
/// Computes the ratio between the mean absolute step across opposite texture borders
/// (seam discontinuity) and the mean absolute step across adjacent interior pixels.
/// A score ≈ 1.0 confirms that boundary seams are statistically indistinguishable from
/// typical interior variations.
#[must_use]
pub fn seam_score(img: &Rgba32FImage) -> f32 {
    let w = img.width();
    let h = img.height();

    if w < 2 || h < 2 {
        return 1.0;
    }

    let pixel_diff = |p1: &Rgba<f32>, p2: &Rgba<f32>| -> f32 {
        ((p1[0] - p2[0]).abs() + (p1[1] - p2[1]).abs() + (p1[2] - p2[2]).abs()) / 3.0
    };

    // Seam column steps: |pixel(0, y) - pixel(w-1, y)|
    let mut sum_seam_col = 0.0f32;
    for y in 0..h {
        sum_seam_col += pixel_diff(img.get_pixel(0, y), img.get_pixel(w - 1, y));
    }
    let mean_seam_col = sum_seam_col / h as f32;

    // Seam row steps: |pixel(x, 0) - pixel(x, h-1)|
    let mut sum_seam_row = 0.0f32;
    for x in 0..w {
        sum_seam_row += pixel_diff(img.get_pixel(x, 0), img.get_pixel(x, h - 1));
    }
    let mean_seam_row = sum_seam_row / w as f32;

    // Interior adjacent column steps: |pixel(x, y) - pixel(x+1, y)|
    let mut sum_interior_col = 0.0f32;
    for y in 0..h {
        for x in 0..(w - 1) {
            sum_interior_col += pixel_diff(img.get_pixel(x, y), img.get_pixel(x + 1, y));
        }
    }
    let mean_interior_col = sum_interior_col / (h * (w - 1)) as f32;

    // Interior adjacent row steps: |pixel(x, y) - pixel(x, y+1)|
    let mut sum_interior_row = 0.0f32;
    for y in 0..(h - 1) {
        for x in 0..w {
            sum_interior_row += pixel_diff(img.get_pixel(x, y), img.get_pixel(x, y + 1));
        }
    }
    let mean_interior_row = sum_interior_row / ((h - 1) * w) as f32;

    let numerator = mean_seam_col + mean_seam_row;
    let denominator = mean_interior_col + mean_interior_row;

    if denominator < 1e-7 {
        if numerator < 1e-7 {
            1.0
        } else {
            SEAM_SCORE_DISCONTINUITY_SENTINEL
        }
    } else {
        numerator / denominator
    }
}

/// Derives a normalized height field from linear albedo using Rec.709 relative luminance
/// smoothed with a periodic Gaussian filter (σ = 1.0 px).
#[must_use]
pub fn derive_height(albedo_linear: &Rgba32FImage) -> GrayF32Image {
    let w = albedo_linear.width();
    let h = albedo_linear.height();

    let mut luma_img = GrayF32Image::new(w, h);
    for (x, y, pixel) in albedo_linear.enumerate_pixels() {
        let y_luma = LUMA_REC709_R * pixel[0] + LUMA_REC709_G * pixel[1] + LUMA_REC709_B * pixel[2];
        luma_img.put_pixel(x, y, Luma([y_luma]));
    }

    let blurred = gaussian_blur_gray(&luma_img, 1.0);

    let mut min_val = f32::INFINITY;
    let mut max_val = f32::NEG_INFINITY;
    for pixel in blurred.pixels() {
        let v = pixel[0];
        if v < min_val {
            min_val = v;
        }
        if v > max_val {
            max_val = v;
        }
    }

    let span = max_val - min_val;
    let mut normalized = GrayF32Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = blurred.get_pixel(x, y)[0];
            let norm = if span > 1e-6 {
                (v - min_val) / span
            } else {
                0.0
            };
            normalized.put_pixel(x, y, Luma([norm.clamp(0.0, 1.0)]));
        }
    }

    normalized
}

/// Derives a tangent-space normal map (+Y up, OpenGL format) from a height field using
/// periodic Sobel gradients scaled by `normal_strength`.
#[must_use]
pub fn derive_normal(height: &GrayF32Image, strength: f32) -> Rgba8Image {
    let w = height.width();
    let h = height.height();
    let mut normal_img = Rgba8Image::new(w, h);

    for y in 0..h {
        let ym1 = if y == 0 { h - 1 } else { y - 1 };
        let yp1 = if y + 1 == h { 0 } else { y + 1 };

        for x in 0..w {
            let xm1 = if x == 0 { w - 1 } else { x - 1 };
            let xp1 = if x + 1 == w { 0 } else { x + 1 };

            let z00 = height.get_pixel(xm1, ym1)[0];
            let z10 = height.get_pixel(x, ym1)[0];
            let z20 = height.get_pixel(xp1, ym1)[0];

            let z01 = height.get_pixel(xm1, y)[0];
            let z21 = height.get_pixel(xp1, y)[0];

            let z02 = height.get_pixel(xm1, yp1)[0];
            let z12 = height.get_pixel(x, yp1)[0];
            let z22 = height.get_pixel(xp1, yp1)[0];

            // Sobel horizontal derivative: gx = (dRight - dLeft) / 8
            let gx = ((z20 + 2.0 * z21 + z22) - (z00 + 2.0 * z01 + z02)) / 8.0;

            // Sobel vertical derivative in OpenGL +Y up space:
            // Since row ym1 is top (higher V coordinate), dUp - dDown = (top - bottom) / 8
            let gy = ((z00 + 2.0 * z10 + z20) - (z02 + 2.0 * z12 + z22)) / 8.0;

            let nx = -strength * gx;
            let ny = -strength * gy;
            let nz = 1.0f32;

            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            let (nx_unit, ny_unit, nz_unit) = if len > 1e-7 {
                (nx / len, ny / len, nz / len)
            } else {
                (0.0, 0.0, 1.0)
            };

            let r = ((nx_unit * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            let g = ((ny_unit * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            let b = ((nz_unit * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            let a = 255u8;

            normal_img.put_pixel(x, y, Rgba([r, g, b, a]));
        }
    }

    normal_img
}

/// Derives a packed Occlusion-Roughness-Metallic (ORM) texture for PBR lighting.
///
/// Channel packing:
/// - **R (Ambient Occlusion)**: `clamp(1.0 - 2.5 * max(0, blur(h, σ=6) - h), 0.25, 1.0)`
/// - **G (Roughness)**: `lerp(r_min, r_max, 1.0 - normalize(stddev_5x5(h)))`
/// - **B (Metallic)**: Uniform metallic constant from layer configuration.
/// - **A**: Fully opaque (255).
#[must_use]
pub fn derive_orm(height: &GrayF32Image, layer: &ThemeLayer) -> Rgba8Image {
    let w = height.width();
    let h = height.height();
    let mut orm_img = Rgba8Image::new(w, h);

    // 1. Wide Gaussian blur (σ = 6.0 px) for broad crevice occlusion
    let blurred_wide = gaussian_blur_gray(height, 6.0);

    // 2. Compute local 5x5 window standard deviation with wrap-around addressing
    let mut stddev_map = vec![0.0f32; (w * h) as usize];
    let mut max_stddev = 0.0f32;

    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0f32;
            let mut sum_sq = 0.0f32;

            for dy in -STDDEV_5X5_RADIUS..=STDDEV_5X5_RADIUS {
                let ny = ((y as i32 + dy).rem_euclid(h as i32)) as u32;
                for dx in -STDDEV_5X5_RADIUS..=STDDEV_5X5_RADIUS {
                    let nx = ((x as i32 + dx).rem_euclid(w as i32)) as u32;
                    let val = height.get_pixel(nx, ny)[0];
                    sum += val;
                    sum_sq += val * val;
                }
            }

            let count = 25.0f32;
            let mean = sum / count;
            let variance = (sum_sq / count - mean * mean).max(0.0);
            let s = variance.sqrt();
            stddev_map[(y * w + x) as usize] = s;
            if s > max_stddev {
                max_stddev = s;
            }
        }
    }

    let (r_min, r_max) = layer.roughness_range;
    let metallic_u8 = (layer.metallic.clamp(0.0, 1.0) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8;

    for y in 0..h {
        for x in 0..w {
            let local_h = height.get_pixel(x, y)[0];
            let broad_h = blurred_wide.get_pixel(x, y)[0];

            // R: Ambient Occlusion
            let depression = (broad_h - local_h).max(0.0);
            let ao = (1.0 - AO_SCALE * depression).clamp(AO_MIN_CLAMP, AO_MAX_CLAMP);
            let r_ao = (ao * 255.0).round().clamp(0.0, 255.0) as u8;

            // G: Roughness
            let s = stddev_map[(y * w + x) as usize];
            let norm_stddev = if max_stddev > 1e-6 {
                (s / max_stddev).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let roughness_factor = 1.0 - norm_stddev;
            let roughness = r_min + roughness_factor * (r_max - r_min);
            let g_roughness = (roughness * 255.0).round().clamp(0.0, 255.0) as u8;

            orm_img.put_pixel(x, y, Rgba([r_ao, g_roughness, metallic_u8, 255]));
        }
    }

    orm_img
}

/// Generates a 2x2 repetition preview image for human visual inspection of tiling seams.
#[must_use]
pub fn tiled_preview(albedo: &Rgba8Image) -> Rgba8Image {
    let w = albedo.width();
    let h = albedo.height();
    let mut preview = Rgba8Image::new(w * 2, h * 2);

    for y in 0..h {
        for x in 0..w {
            let p = *albedo.get_pixel(x, y);
            preview.put_pixel(x, y, p);
            preview.put_pixel(x + w, y, p);
            preview.put_pixel(x, y + h, p);
            preview.put_pixel(x + w, y + h, p);
        }
    }

    preview
}

/// Result report generated upon baking a complete terrain visual theme.
#[derive(Debug, Clone)]
pub struct BakeReport {
    /// Slug identifier of the baked theme.
    pub theme_slug: String,
    /// Detailed reports for each of the four surface layers.
    pub layers: Vec<LayerBakeReport>,
    /// Destination path of the backdrop texture.
    pub backdrop_path: PathBuf,
}

impl fmt::Display for BakeReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Bake Report for Terrain Theme: '{}'", self.theme_slug)?;
        writeln!(f, "{:-<92}", "")?;
        for layer in &self.layers {
            writeln!(
                f,
                "Layer: {:<14} | Mode: {:<26} | Seam Score: {:5.2} -> {:5.2} | Output: {}",
                layer.layer_name,
                layer.tiling_mode,
                layer.seam_score_before,
                layer.seam_score_after,
                layer.paths.albedo.display()
            )?;
        }
        writeln!(f, "Backdrop: {}", self.backdrop_path.display())?;
        writeln!(f, "{:-<92}", "")?;
        Ok(())
    }
}

/// Per-layer baking statistics including wrap seam metrics before and after processing.
#[derive(Debug, Clone)]
pub struct LayerBakeReport {
    /// Unique name of this layer.
    pub layer_name: String,
    /// Tiling mode used for this layer.
    pub tiling_mode: TilingMode,
    /// Output paths of the generated albedo, normal, ORM, and preview files.
    pub paths: BakedLayerPaths,
    /// Wrap seam score of the raw source art image.
    pub seam_score_before: f32,
    /// Wrap seam score after tiling processing.
    pub seam_score_after: f32,
}

/// Helper function to process and bake all four texture products for a single layer.
fn bake_single_layer(
    layer: &ThemeLayer,
    texture_size: u32,
    assets_dir: &Path,
    baked_paths: &BakedLayerPaths,
) -> Result<LayerBakeReport, TerrainThemeError> {
    let src_path = assets_dir.join(&layer.source);
    if !src_path.exists() {
        return Err(TerrainThemeError::MissingSource(src_path));
    }

    let raw_linear = load_and_resize(&src_path, texture_size)?;
    let seam_score_before = seam_score(&raw_linear);

    let (tiled_linear, seam_score_after) = match layer.tiling {
        TilingMode::MinCutBlend => {
            let tiled = make_tileable(&raw_linear);
            let score_after = seam_score(&tiled);
            (tiled, score_after)
        }
        TilingMode::SourceSeamless { max_seam_score } => {
            if seam_score_before > max_seam_score {
                return Err(TerrainThemeError::SourceNotSeamless {
                    layer: layer.name.clone(),
                    score: seam_score_before,
                    max: max_seam_score,
                });
            }
            (raw_linear, seam_score_before)
        }
    };

    let albedo_u8 = linear_to_srgb_image(&tiled_linear);
    let abs_albedo = assets_dir.join(&baked_paths.albedo);
    albedo_u8.save(&abs_albedo).map_err(|e| {
        TerrainThemeError::Image(format!("Failed to save {}: {e}", abs_albedo.display()))
    })?;

    let height_field = derive_height(&tiled_linear);
    let normal_map = derive_normal(&height_field, layer.normal_strength);
    let abs_normal = assets_dir.join(&baked_paths.normal);
    normal_map.save(&abs_normal).map_err(|e| {
        TerrainThemeError::Image(format!("Failed to save {}: {e}", abs_normal.display()))
    })?;

    let orm_map = derive_orm(&height_field, layer);
    let abs_orm = assets_dir.join(&baked_paths.orm);
    orm_map.save(&abs_orm).map_err(|e| {
        TerrainThemeError::Image(format!("Failed to save {}: {e}", abs_orm.display()))
    })?;

    let preview_map = tiled_preview(&albedo_u8);
    let abs_preview = assets_dir.join(&baked_paths.tiled_preview);
    preview_map.save(&abs_preview).map_err(|e| {
        TerrainThemeError::Image(format!("Failed to save {}: {e}", abs_preview.display()))
    })?;

    Ok(LayerBakeReport {
        layer_name: layer.name.clone(),
        tiling_mode: layer.tiling,
        paths: baked_paths.clone(),
        seam_score_before,
        seam_score_after,
    })
}

/// Helper function to process and save the backdrop void texture.
fn bake_backdrop(
    source_rel: &str,
    texture_size: u32,
    assets_dir: &Path,
    dst_abs: &Path,
) -> Result<(), TerrainThemeError> {
    let backdrop_src = assets_dir.join(source_rel);
    if !backdrop_src.exists() {
        return Err(TerrainThemeError::MissingSource(backdrop_src));
    }

    let raw_backdrop = load_and_resize(&backdrop_src, texture_size)?;
    let tiled_backdrop = make_tileable(&raw_backdrop);
    let backdrop_u8 = linear_to_srgb_image(&tiled_backdrop);

    backdrop_u8.save(dst_abs).map_err(|e| {
        TerrainThemeError::Image(format!("Failed to save {}: {e}", dst_abs.display()))
    })?;

    Ok(())
}

/// Bakes all four texture layers and the void backdrop for a specified theme.
///
/// # Errors
/// Returns [`TerrainThemeError`] if catalog parsing fails, any source image is missing,
/// dimensions deviate from square, or filesystem writing encounters an error.
pub fn bake_theme(
    catalog_path: &Path,
    theme_slug: &str,
    assets_dir: &Path,
) -> Result<BakeReport, TerrainThemeError> {
    let catalog = TerrainThemeCatalog::load(catalog_path)?;
    let theme = catalog
        .get(theme_slug)
        .ok_or_else(|| TerrainThemeError::UnknownTheme(theme_slug.to_string()))?;

    theme.validate()?;

    let baked_out_dir = assets_dir.join(&theme.baked_dir);
    std::fs::create_dir_all(&baked_out_dir).map_err(|source| TerrainThemeError::Io {
        path: baked_out_dir.clone(),
        source,
    })?;

    let mut layer_reports = Vec::new();
    for layer in theme.ordered_layers() {
        let paths = theme.baked_paths(layer);
        let report = bake_single_layer(layer, theme.texture_size, assets_dir, &paths)?;
        layer_reports.push(report);
    }

    let backdrop_dst = baked_out_dir.join("void_nebula.png");
    bake_backdrop(
        &theme.backdrop.source,
        theme.texture_size,
        assets_dir,
        &backdrop_dst,
    )?;

    Ok(BakeReport {
        theme_slug: theme_slug.to_string(),
        layers: layer_reports,
        backdrop_path: PathBuf::from(&theme.baked_dir).join("void_nebula.png"),
    })
}

// ---------------------------------------------------------------------------
// Internal Pyramid Operations & Separable Convolutions
// ---------------------------------------------------------------------------

// PERFORMANCE RATIONALE:
// Why: Full 2D 5x5 convolution requires 25 taps per pixel.
// What: We decompose the Burt-Adelson Gaussian filter [1, 4, 6, 4, 1] / 16 into separable
// 1D horizontal and vertical passes with wrap-around addressing.
// Trade-off: Two 5-tap passes (10 ops) replaces one 25-tap pass, cutting compute by 60%
// while preserving bit-exact floating-point mathematical symmetry.

fn reduce_rgba(src: &Rgba32FImage) -> Rgba32FImage {
    let w = src.width();
    let h = src.height();
    let dst_w = (w / 2).max(1);
    let dst_h = (h / 2).max(1);

    let weights = [0.0625f32, 0.25, 0.375, 0.25, 0.0625];

    // Horizontal 1D pass
    let mut h_buf = Rgba32FImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let mut accum = [0.0f32; 4];
            for (idx, &wt) in weights.iter().enumerate() {
                let offset = idx as i32 - 2;
                let nx = ((x as i32 + offset).rem_euclid(w as i32)) as u32;
                let p = src.get_pixel(nx, y);
                accum[0] += wt * p[0];
                accum[1] += wt * p[1];
                accum[2] += wt * p[2];
                accum[3] += wt * p[3];
            }
            h_buf.put_pixel(x, y, Rgba(accum));
        }
    }

    // Vertical 1D pass with 2x subsampling
    let mut dst = Rgba32FImage::new(dst_w, dst_h);
    for dy in 0..dst_h {
        let sy = (dy * 2).min(h - 1);
        for dx in 0..dst_w {
            let sx = (dx * 2).min(w - 1);
            let mut accum = [0.0f32; 4];
            for (idx, &wt) in weights.iter().enumerate() {
                let offset = idx as i32 - 2;
                let ny = ((sy as i32 + offset).rem_euclid(h as i32)) as u32;
                let p = h_buf.get_pixel(sx, ny);
                accum[0] += wt * p[0];
                accum[1] += wt * p[1];
                accum[2] += wt * p[2];
                accum[3] += wt * p[3];
            }
            dst.put_pixel(dx, dy, Rgba(accum));
        }
    }

    dst
}

fn expand_rgba(src: &Rgba32FImage) -> Rgba32FImage {
    let w = src.width();
    let h = src.height();
    let dst_w = w * 2;
    let dst_h = h * 2;

    // Step 1: Expand horizontally to (dst_w, h)
    let mut h_expanded = Rgba32FImage::new(dst_w, h);
    for y in 0..h {
        for x in 0..w {
            let xm1 = ((x as i32 - 1).rem_euclid(w as i32)) as u32;
            let xp1 = ((x as i32 + 1).rem_euclid(w as i32)) as u32;

            let s_m1 = src.get_pixel(xm1, y);
            let s_0 = src.get_pixel(x, y);
            let s_p1 = src.get_pixel(xp1, y);

            let even = [
                0.125 * s_m1[0] + 0.75 * s_0[0] + 0.125 * s_p1[0],
                0.125 * s_m1[1] + 0.75 * s_0[1] + 0.125 * s_p1[1],
                0.125 * s_m1[2] + 0.75 * s_0[2] + 0.125 * s_p1[2],
                0.125 * s_m1[3] + 0.75 * s_0[3] + 0.125 * s_p1[3],
            ];
            h_expanded.put_pixel(2 * x, y, Rgba(even));

            let odd = [
                0.5 * s_0[0] + 0.5 * s_p1[0],
                0.5 * s_0[1] + 0.5 * s_p1[1],
                0.5 * s_0[2] + 0.5 * s_p1[2],
                0.5 * s_0[3] + 0.5 * s_p1[3],
            ];
            h_expanded.put_pixel(2 * x + 1, y, Rgba(odd));
        }
    }

    // Step 2: Expand vertically to (dst_w, dst_h)
    let mut dst = Rgba32FImage::new(dst_w, dst_h);
    for x in 0..dst_w {
        for y in 0..h {
            let ym1 = ((y as i32 - 1).rem_euclid(h as i32)) as u32;
            let yp1 = ((y as i32 + 1).rem_euclid(h as i32)) as u32;

            let s_m1 = h_expanded.get_pixel(x, ym1);
            let s_0 = h_expanded.get_pixel(x, y);
            let s_p1 = h_expanded.get_pixel(x, yp1);

            let even = [
                0.125 * s_m1[0] + 0.75 * s_0[0] + 0.125 * s_p1[0],
                0.125 * s_m1[1] + 0.75 * s_0[1] + 0.125 * s_p1[1],
                0.125 * s_m1[2] + 0.75 * s_0[2] + 0.125 * s_p1[2],
                0.125 * s_m1[3] + 0.75 * s_0[3] + 0.125 * s_p1[3],
            ];
            dst.put_pixel(x, 2 * y, Rgba(even));

            let odd = [
                0.5 * s_0[0] + 0.5 * s_p1[0],
                0.5 * s_0[1] + 0.5 * s_p1[1],
                0.5 * s_0[2] + 0.5 * s_p1[2],
                0.5 * s_0[3] + 0.5 * s_p1[3],
            ];
            dst.put_pixel(x, 2 * y + 1, Rgba(odd));
        }
    }

    dst
}

fn reduce_gray(src: &GrayF32Image) -> GrayF32Image {
    let w = src.width();
    let h = src.height();
    let dst_w = (w / 2).max(1);
    let dst_h = (h / 2).max(1);

    let weights = [0.0625f32, 0.25, 0.375, 0.25, 0.0625];

    let mut h_buf = GrayF32Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let mut accum = 0.0f32;
            for (idx, &wt) in weights.iter().enumerate() {
                let offset = idx as i32 - 2;
                let nx = ((x as i32 + offset).rem_euclid(w as i32)) as u32;
                accum += wt * src.get_pixel(nx, y)[0];
            }
            h_buf.put_pixel(x, y, Luma([accum]));
        }
    }

    let mut dst = GrayF32Image::new(dst_w, dst_h);
    for dy in 0..dst_h {
        let sy = (dy * 2).min(h - 1);
        for dx in 0..dst_w {
            let sx = (dx * 2).min(w - 1);
            let mut accum = 0.0f32;
            for (idx, &wt) in weights.iter().enumerate() {
                let offset = idx as i32 - 2;
                let ny = ((sy as i32 + offset).rem_euclid(h as i32)) as u32;
                accum += wt * h_buf.get_pixel(sx, ny)[0];
            }
            dst.put_pixel(dx, dy, Luma([accum]));
        }
    }

    dst
}

fn subtract_rgba(a: &Rgba32FImage, b: &Rgba32FImage) -> Rgba32FImage {
    let w = a.width();
    let h = a.height();
    let mut dst = Rgba32FImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let pa = a.get_pixel(x, y);
            let pb = b.get_pixel(x, y);
            dst.put_pixel(
                x,
                y,
                Rgba([pa[0] - pb[0], pa[1] - pb[1], pa[2] - pb[2], pa[3] - pb[3]]),
            );
        }
    }
    dst
}

fn add_rgba(a: &Rgba32FImage, b: &Rgba32FImage) -> Rgba32FImage {
    let w = a.width();
    let h = a.height();
    let mut dst = Rgba32FImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let pa = a.get_pixel(x, y);
            let pb = b.get_pixel(x, y);
            dst.put_pixel(
                x,
                y,
                Rgba([pa[0] + pb[0], pa[1] + pb[1], pa[2] + pb[2], pa[3] + pb[3]]),
            );
        }
    }
    dst
}

/// Periodic 2D Gaussian blur with wrap-around boundary addressing.
fn gaussian_blur_gray(img: &GrayF32Image, sigma: f32) -> GrayF32Image {
    let w = img.width();
    let h = img.height();

    let radius = (3.0 * sigma).ceil() as i32;
    let two_sigma_sq = 2.0 * sigma * sigma;

    let mut weights = Vec::with_capacity((2 * radius + 1) as usize);
    let mut sum = 0.0f32;
    for i in -radius..=radius {
        let wt = (-((i * i) as f32) / two_sigma_sq).exp();
        weights.push(wt);
        sum += wt;
    }
    for wt in &mut weights {
        *wt /= sum;
    }

    // Horizontal pass
    let mut h_pass = GrayF32Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let mut accum = 0.0f32;
            for (idx, &wt) in weights.iter().enumerate() {
                let offset = idx as i32 - radius;
                let nx = ((x as i32 + offset).rem_euclid(w as i32)) as u32;
                accum += wt * img.get_pixel(nx, y)[0];
            }
            h_pass.put_pixel(x, y, Luma([accum]));
        }
    }

    // Vertical pass
    let mut v_pass = GrayF32Image::new(w, h);
    for x in 0..w {
        for y in 0..h {
            let mut accum = 0.0f32;
            for (idx, &wt) in weights.iter().enumerate() {
                let offset = idx as i32 - radius;
                let ny = ((y as i32 + offset).rem_euclid(h as i32)) as u32;
                accum += wt * h_pass.get_pixel(x, ny)[0];
            }
            v_pass.put_pixel(x, y, Luma([accum]));
        }
    }

    v_pass
}
