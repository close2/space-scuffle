//! Loop continuity of baked clips: the measures (SSIM, camera zoom) and the seam repair that
//! makes a looping state restart without a visible jump (plan
//! `docs/plans/2026-11-04-veo-seamless-loops.md`, decisions D-5 and D-6).
//!
//! # Responsibility
//! - **Measures.** [`ssim`] is the luminance SSIM of two frames (Wang et al. 2004, 8×8
//!   windows). [`ZoomReference::estimate`] finds the camera zoom and shift that map one frame
//!   onto a reference frame ([`ZoomEstimate`]). [`fit_line`] fits a straight line by least
//!   squares. These are shared by the bake's repair and by the review's `loop-check`.
//! - **Repair.** [`SeamSpec`] is one state's treatment. [`plan_scale_lock`] measures every
//!   kept frame against the first, fits the zoom as a straight line in `ln(magnification)` and
//!   decides whether to lock it, leave it, or refuse the clip; [`apply_scale_lock`] enlarges
//!   every frame to the framing of the most zoomed-in one with sub-pixel bilinear sampling;
//!   [`downsample_half`] brings the twice-size frames to the frame size; and
//!   [`crossfade_into_first`] blends the last frames toward frame 0 in linear light.
//!
//! # How it fits
//! `ffmpeg_importer::decode_repaired_clip` runs the repair on the frames a state keeps, before
//! `framing::frame_to_square` completes them to squares, and `provenance` records what it did
//! (`SeamRecord`). Nothing here knows any catalog identity: which states are repaired is data
//! (`media_style.ron`, charter §3.3).
//!
//! # Coordinates and units
//! Positions are in pixels of the frames passed in, measured between pixel centres: pixel
//! `(x, y)` sits at `(x, y)`, and a frame's centre is `((width - 1) / 2, (height - 1) / 2)`.
//! A [`ZoomEstimate`] with magnification `m` and offset `d` says that the content the reference
//! shows at `p` appears in the measured frame at `c + m (p - c) + d`, `c` being the centre: `m`
//! above 1 is a push-in. Growth is `m - 1`, a fraction (0.06 is 6 %). Frame indices count kept
//! frames, not source frames.
//!
//! # Invariants
//! - **Real pixels only.** The repair resamples and blends decoded frames of the clip itself; it
//!   never draws content (charter §1.2). Enlarging toward the most zoomed-in frame only crops,
//!   so no pixel outside a source frame is needed except at most a fitted shift of a pixel or
//!   two at the border, which repeats the edge pixel.
//! - **A fit, not per-frame values.** The lock follows the fitted line, so estimator noise moves
//!   no frame on its own and the subject cannot wobble.
//! - **Refuse rather than hide.** A clip whose zoom does not follow a straight line within
//!   [`SCALE_LOCK_MAX_RESIDUAL`], or that zooms more than [`SCALE_LOCK_MAX_GROWTH`], is refused
//!   with [`BakerError::SeamRepairRefused`]: its motion is not a camera zoom.
//! - **Frame 0 is never blended.** The cross-fade changes only the last frames.
//! - **Deterministic.** Every loop runs in a fixed order over plain `f64` arithmetic, so the same
//!   frames give the same bytes on the same platform.

use crate::error::BakerError;
use crate::terrain_texture::{linear_to_srgb, srgb_to_linear};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

/// A fitted zoom smaller than this over the kept frames (0.5 %) is left alone.
pub const SCALE_LOCK_MIN_GROWTH: f64 = 0.005;

/// A fitted zoom larger than this over the kept frames (12 %) is refused: cropping it away
/// would cut too much of the frame.
pub const SCALE_LOCK_MAX_GROWTH: f64 = 0.12;

/// Largest accepted difference between a frame's measured `ln(magnification)` and the fitted
/// line (0.5 % of scale). A larger one means the subject moves or changes in a way a zoom does
/// not explain.
pub const SCALE_LOCK_MAX_RESIDUAL: f64 = 0.005;

/// Most frames a cross-fade may blend toward frame 0.
pub const MAX_CROSSFADE_FRAMES: u8 = 4;

/// Smallest scale `1 / magnification` the zoom search tries.
pub const ZOOM_SCALE_MIN: f64 = 0.85;

/// Largest scale `1 / magnification` the zoom search tries.
pub const ZOOM_SCALE_MAX: f64 = 1.15;

/// Step of the zoom search's scale grid, before refinement.
pub const ZOOM_SCALE_STEP: f64 = 0.005;

/// Largest shift the zoom search tries along each axis, as a fraction of that axis's side.
pub const ZOOM_MAX_OFFSET: f64 = 0.04;

/// Share of each side of the reference, centred, that the zoom is measured over.
pub const ZOOM_WINDOW: f64 = 0.8;

/// Side of the square SSIM window, in pixels.
pub const SSIM_WINDOW: usize = 8;

/// SSIM stabiliser `C1 = (0.01 · 255)²`.
const SSIM_C1: f64 = 6.5025;

/// SSIM stabiliser `C2 = (0.03 · 255)²`.
const SSIM_C2: f64 = 58.5225;

/// The zoom grid search runs on the first pyramid level whose longer side is at most this.
const COARSEST_SIDE: usize = 32;

/// No pyramid level is made with a shorter side below this.
const MIN_LEVEL_SIDE: usize = 16;

/// Refinement stops at the first pyramid level whose longer side is at most this; finer levels
/// add time but no accuracy that the line fit needs.
const FINEST_REFINED_SIDE: usize = 512;

/// Most Gauss-Newton steps per pyramid level.
const REFINE_STEPS: usize = 12;

/// A Gauss-Newton step smaller than this in magnification (0.001 %) and than
/// [`STEP_TOLERANCE_PIXELS`] in both offsets ends the refinement of a level.
const STEP_TOLERANCE_MAGNIFICATION: f64 = 1e-5;

/// Offset part of the refinement's stopping rule, in pixels of the level.
const STEP_TOLERANCE_PIXELS: f64 = 0.01;

/// One looping state's seam treatment, as `media_style.ron` states it in `loops` (D-1).
///
/// The fields and their names are those of the data file's `seam: Some((scale_lock: …,
/// crossfade_frames: …))`, so this type can read that entry directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeamSpec {
    /// Undo a steady camera zoom by enlarging every frame to the framing of the most
    /// zoomed-in one ([`plan_scale_lock`]).
    pub scale_lock: bool,
    /// How many of the last kept frames are blended toward frame 0 ([`crossfade_into_first`]);
    /// 0 for none, at most [`MAX_CROSSFADE_FRAMES`].
    pub crossfade_frames: u8,
}

// --- Luminance planes ----------------------------------------------------------------------

/// One channel of `f64` samples, row by row.
#[derive(Debug, Clone)]
struct Plane {
    width: usize,
    height: usize,
    values: Vec<f64>,
}

impl Plane {
    /// The Rec. 601 luma of `frame`, on the 0 to 255 scale of its sRGB bytes.
    fn luma(frame: &RgbaImage) -> Self {
        let values = frame
            .pixels()
            .map(|p| 0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2]))
            .collect();
        Self {
            width: frame.width() as usize,
            height: frame.height() as usize,
            values,
        }
    }

    /// The centre of the plane, in pixel-centre coordinates.
    fn centre(&self) -> [f64; 2] {
        [
            index_f64(self.width.saturating_sub(1)) / 2.0,
            index_f64(self.height.saturating_sub(1)) / 2.0,
        ]
    }

    /// The plane at half the size: each sample is the mean of a 2×2 block; an odd last row or
    /// column is dropped.
    fn half(&self) -> Self {
        let (width, height) = (self.width / 2, self.height / 2);
        let mut values = Vec::with_capacity(width * height);
        for y in 0..height {
            let top = 2 * y * self.width;
            let bottom = top + self.width;
            for x in 0..width {
                let sum = self.values[top + 2 * x]
                    + self.values[top + 2 * x + 1]
                    + self.values[bottom + 2 * x]
                    + self.values[bottom + 2 * x + 1];
                values.push(sum / 4.0);
            }
        }
        Self {
            width,
            height,
            values,
        }
    }

    /// The bilinear sample at `(x, y)`, or `None` outside the outermost pixel centres.
    fn sample(&self, x: f64, y: f64) -> Option<f64> {
        Footprint::at(self.width, self.height, x, y).map(|f| f.mix(|i| self.values[i]))
    }

    /// Each sample with its horizontal and vertical derivatives: central differences inside,
    /// one-sided at the edges.
    fn textured(&self) -> Textured {
        let (w, h) = (self.width, self.height);
        let at = |x: usize, y: usize| self.values[y * w + x];
        let derivative = |before: f64, after: f64, span: usize| (after - before) / index_f64(span);
        let mut texels = Vec::with_capacity(w * h);
        for y in 0..h {
            let (up, down) = (y.saturating_sub(1), (y + 1).min(h - 1));
            for x in 0..w {
                let (left, right) = (x.saturating_sub(1), (x + 1).min(w - 1));
                texels.push([
                    at(x, y),
                    derivative(at(left, y), at(right, y), (right - left).max(1)),
                    derivative(at(x, up), at(x, down), (down - up).max(1)),
                ]);
            }
        }
        Textured {
            width: w,
            height: h,
            texels,
        }
    }
}

/// A plane's samples with their derivatives, `[value, d/dx, d/dy]` per pixel, row by row.
struct Textured {
    width: usize,
    height: usize,
    texels: Vec<[f64; 3]>,
}

impl Textured {
    /// The bilinear sample of value and derivatives at `(x, y)`, or `None` outside.
    fn sample(&self, x: f64, y: f64) -> Option<[f64; 3]> {
        let footprint = Footprint::at(self.width, self.height, x, y)?;
        Some([0, 1, 2].map(|c| footprint.mix(|i| self.texels[i][c])))
    }
}

/// The four neighbouring samples of a sub-pixel position and its bilinear weights.
struct Footprint {
    /// Indices of the top-left, top-right, bottom-left and bottom-right samples.
    corners: [usize; 4],
    fx: f64,
    fy: f64,
}

impl Footprint {
    /// The footprint of `(x, y)` in a `width`×`height` grid, or `None` outside the outermost
    /// pixel centres.
    fn at(width: usize, height: usize, x: f64, y: f64) -> Option<Self> {
        let (max_x, max_y) = (index_f64(width - 1), index_f64(height - 1));
        if !(0.0..=max_x).contains(&x) || !(0.0..=max_y).contains(&y) {
            return None;
        }
        let (x0, y0) = (to_index(x), to_index(y));
        let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
        Some(Self {
            corners: [
                y0 * width + x0,
                y0 * width + x1,
                y1 * width + x0,
                y1 * width + x1,
            ],
            fx: x - index_f64(x0),
            fy: y - index_f64(y0),
        })
    }

    /// The bilinear mix of the four values `value` gives for the corner indices.
    fn mix(&self, value: impl Fn(usize) -> f64) -> f64 {
        let [a, b, c, d] = self.corners.map(value);
        let top = a + (b - a) * self.fx;
        let bottom = c + (d - c) * self.fx;
        top + (bottom - top) * self.fy
    }
}

/// `index` as `f64`, exact for every frame index or pixel coordinate (all far below 2^52).
#[allow(clippy::cast_precision_loss)]
fn index_f64(index: usize) -> f64 {
    index as f64
}

/// The whole part of a coordinate, as an index; 0 for a negative one.
///
/// For the non-negative coordinates it is used on, truncation is the floor.
// PERFORMANCE RATIONALE:
// Why: `f64::floor` compiles to a library call on the baseline x86-64 target, and the zoom
// search takes the floor of two coordinates for each of millions of bilinear samples.
// What: The cast truncates toward zero, which equals the floor for the non-negative values
// passed here (the callers check or clamp the range first).
// Trade-off: The function is only correct for non-negative input, which its callers ensure.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_index(value: f64) -> usize {
    value.max(0.0) as usize
}

/// A channel value clamped to 0 to 255 and rounded to the nearest byte, halves up.
// PERFORMANCE RATIONALE:
// Why: `f64::round` compiles to a library call on the baseline x86-64 target, and the scale lock
// rounds 38 million channel values per twice-size Veo state.
// What: Adding one half to the clamped, non-negative value and truncating rounds it the same
// way (halves away from zero, which is up for these values).
// Trade-off: The equivalence holds only after the clamp, which is therefore done first.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_byte(value: f64) -> u8 {
    (value.clamp(0.0, 255.0) + 0.5) as u8
}

fn require_same_size(a: &RgbaImage, b: &RgbaImage, what: &str) -> Result<(), BakerError> {
    if a.dimensions() == b.dimensions() {
        return Ok(());
    }
    Err(BakerError::ValidationFailed {
        name: what.to_string(),
        reason: format!(
            "frames differ in size: {}x{} and {}x{}",
            a.width(),
            a.height(),
            b.width(),
            b.height()
        ),
    })
}

// --- SSIM -----------------------------------------------------------------------------------

/// Running sums of a plane, `(width + 1) × (height + 1)`, with a zero first row and column.
struct SummedArea {
    stride: usize,
    sums: Vec<f64>,
}

impl SummedArea {
    fn new(width: usize, height: usize, value: impl Fn(usize) -> f64) -> Self {
        let stride = width + 1;
        let mut sums = vec![0.0; stride * (height + 1)];
        for y in 0..height {
            let mut row = 0.0;
            for x in 0..width {
                row += value(y * width + x);
                sums[(y + 1) * stride + x + 1] = sums[y * stride + x + 1] + row;
            }
        }
        Self { stride, sums }
    }

    /// Sum of the `side`×`side` block whose top-left pixel is `(x, y)`.
    fn block(&self, x: usize, y: usize, side: usize) -> f64 {
        let s = self.stride;
        self.sums[(y + side) * s + x + side]
            - self.sums[y * s + x + side]
            - self.sums[(y + side) * s + x]
            + self.sums[y * s + x]
    }
}

/// The mean luminance SSIM of two frames of the same size, in −1 to 1 (1 for identical frames).
///
/// Every [`SSIM_WINDOW`]×[`SSIM_WINDOW`] window position (stride 1) contributes
/// `((2 μa μb + C1)(2 σab + C2)) / ((μa² + μb² + C1)(σa² + σb² + C2))` with population
/// variances, on Rec. 601 luma of the sRGB bytes (Wang et al. 2004).
///
/// # Errors
/// [`BakerError::ValidationFailed`] if the frames differ in size or are smaller than one window.
pub fn ssim(first: &RgbaImage, second: &RgbaImage) -> Result<f64, BakerError> {
    require_same_size(first, second, "SSIM")?;
    let (pa, pb) = (Plane::luma(first), Plane::luma(second));
    let (w, h) = (pa.width, pa.height);
    if w < SSIM_WINDOW || h < SSIM_WINDOW {
        return Err(BakerError::ValidationFailed {
            name: "SSIM".into(),
            reason: format!("a {w}x{h} frame is smaller than one {SSIM_WINDOW} px window"),
        });
    }
    // PERFORMANCE RATIONALE:
    // Why: Summing every 8×8 window directly costs 64 reads per window and statistic, about 190
    // million reads for one pair of 1024×576 frames.
    // What: Five summed-area tables (a, b, a², b², ab) give each window's sums in four reads.
    // Trade-off: Five full-frame f64 tables of memory per call and slightly less obvious code,
    // for a constant cost per window; the result is the same up to rounding.
    let sum_a = SummedArea::new(w, h, |i| pa.values[i]);
    let sum_b = SummedArea::new(w, h, |i| pb.values[i]);
    let sum_aa = SummedArea::new(w, h, |i| pa.values[i] * pa.values[i]);
    let sum_bb = SummedArea::new(w, h, |i| pb.values[i] * pb.values[i]);
    let sum_ab = SummedArea::new(w, h, |i| pa.values[i] * pb.values[i]);
    let area = index_f64(SSIM_WINDOW * SSIM_WINDOW);
    let mut total = 0.0;
    let mut windows = 0_usize;
    for y in 0..=h - SSIM_WINDOW {
        for x in 0..=w - SSIM_WINDOW {
            let mean_a = sum_a.block(x, y, SSIM_WINDOW) / area;
            let mean_b = sum_b.block(x, y, SSIM_WINDOW) / area;
            let var_a = sum_aa.block(x, y, SSIM_WINDOW) / area - mean_a * mean_a;
            let var_b = sum_bb.block(x, y, SSIM_WINDOW) / area - mean_b * mean_b;
            let cov = sum_ab.block(x, y, SSIM_WINDOW) / area - mean_a * mean_b;
            total += ((2.0 * mean_a * mean_b + SSIM_C1) * (2.0 * cov + SSIM_C2))
                / ((mean_a * mean_a + mean_b * mean_b + SSIM_C1) * (var_a + var_b + SSIM_C2));
            windows += 1;
        }
    }
    Ok(total / index_f64(windows))
}

// --- Zoom estimator -------------------------------------------------------------------------

/// The camera zoom and shift that map a reference frame onto another frame. See the module
/// docs for the coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomEstimate {
    /// How much larger the reference's content appears in the frame (above 1: push-in).
    pub magnification: f64,
    /// Where the reference's centre appears in the frame, relative to the frame's centre, in
    /// pixels `[x, y]` (x to the right, y down).
    pub offset: [f64; 2],
}

impl ZoomEstimate {
    /// The zoom as growth `magnification - 1`: 0.05 for a 5 % push-in, negative for a pull-out.
    #[must_use]
    pub fn growth(&self) -> f64 {
        self.magnification - 1.0
    }

    /// The scale `1 / magnification` that maps the frame back onto the reference (plan D-6's
    /// `s`, searched in [`ZOOM_SCALE_MIN`] to [`ZOOM_SCALE_MAX`]).
    #[must_use]
    pub fn scale(&self) -> f64 {
        1.0 / self.magnification
    }
}

/// The warp under test: magnification and offset in pixels of one pyramid level.
#[derive(Debug, Clone, Copy)]
struct Warp {
    magnification: f64,
    offset: [f64; 2],
}

/// A reference frame prepared for repeated zoom estimates: its luma pyramid, finest first.
#[derive(Debug, Clone)]
pub struct ZoomReference {
    pyramid: Vec<Plane>,
}

impl ZoomReference {
    /// Prepares `reference` for [`ZoomReference::estimate`].
    ///
    /// # Errors
    /// [`BakerError::ValidationFailed`] if a side is shorter than 16 pixels.
    pub fn new(reference: &RgbaImage) -> Result<Self, BakerError> {
        let (w, h) = reference.dimensions();
        if (w.min(h) as usize) < MIN_LEVEL_SIDE {
            return Err(BakerError::ValidationFailed {
                name: "zoom estimate".into(),
                reason: format!("a {w}x{h} frame is too small to measure a zoom"),
            });
        }
        Ok(Self {
            pyramid: pyramid(Plane::luma(reference)),
        })
    }

    /// The zoom and shift that map the reference onto `frame` (same size), measured over the
    /// centred [`ZOOM_WINDOW`] share of the reference.
    ///
    /// A grid search on the coarsest pyramid level tries every scale from [`ZOOM_SCALE_MIN`] to
    /// [`ZOOM_SCALE_MAX`] in steps of [`ZOOM_SCALE_STEP`] and every whole-pixel shift up to
    /// [`ZOOM_MAX_OFFSET`] of each side, by mean squared luma difference. Gauss-Newton steps then
    /// refine the best one, level by level, to a fraction of a pixel and of a grid step.
    ///
    /// # Errors
    /// [`BakerError::ValidationFailed`] if `frame` is not the reference's size.
    pub fn estimate(&self, frame: &RgbaImage) -> Result<ZoomEstimate, BakerError> {
        let finest = &self.pyramid[0];
        let size = (frame.width() as usize, frame.height() as usize);
        if size != (finest.width, finest.height) {
            return Err(BakerError::ValidationFailed {
                name: "zoom estimate".into(),
                reason: format!(
                    "frames differ in size: {}x{} and {}x{}",
                    finest.width, finest.height, size.0, size.1
                ),
            });
        }
        let frames = pyramid(Plane::luma(frame));
        let coarsest = self.pyramid.len() - 1;
        let refined_to = self
            .pyramid
            .iter()
            .position(|plane| plane.width.max(plane.height) <= FINEST_REFINED_SIDE)
            .unwrap_or(coarsest);
        let mut warp = grid_search(&self.pyramid[coarsest], &frames[coarsest]);
        for level in (refined_to..=coarsest).rev() {
            if level < coarsest {
                warp.offset = [warp.offset[0] * 2.0, warp.offset[1] * 2.0];
            }
            warp = refine(&self.pyramid[level], &frames[level], warp);
        }
        let to_full = index_f64(1 << refined_to);
        Ok(ZoomEstimate {
            magnification: warp.magnification,
            offset: [warp.offset[0] * to_full, warp.offset[1] * to_full],
        })
    }
}

/// The zoom and shift that map `reference` onto `frame`. See [`ZoomReference::estimate`].
///
/// # Errors
/// As [`ZoomReference::new`] and [`ZoomReference::estimate`].
pub fn estimate_zoom(reference: &RgbaImage, frame: &RgbaImage) -> Result<ZoomEstimate, BakerError> {
    ZoomReference::new(reference)?.estimate(frame)
}

/// `plane` and its halvings, finest first, down to the first level whose longer side is at most
/// [`COARSEST_SIDE`] or whose next halving would be shorter than [`MIN_LEVEL_SIDE`].
fn pyramid(plane: Plane) -> Vec<Plane> {
    let mut levels = vec![plane];
    loop {
        let last = &levels[levels.len() - 1];
        if last.width.max(last.height) <= COARSEST_SIDE
            || last.width.min(last.height) / 2 < MIN_LEVEL_SIDE
        {
            return levels;
        }
        let half = last.half();
        levels.push(half);
    }
}

/// The reference pixels the zoom is measured over: the centred [`ZOOM_WINDOW`] share.
fn window(
    plane: &Plane,
) -> (
    std::ops::RangeInclusive<usize>,
    std::ops::RangeInclusive<usize>,
) {
    let [cx, cy] = plane.centre();
    let half_w = ZOOM_WINDOW / 2.0 * index_f64(plane.width);
    let half_h = ZOOM_WINDOW / 2.0 * index_f64(plane.height);
    (
        to_index((cx - half_w).ceil())..=to_index((cx + half_w).floor()),
        to_index((cy - half_h).ceil())..=to_index((cy + half_h).floor()),
    )
}

/// Mean squared luma difference between `reference` over its window and `frame` under `warp`,
/// or infinity when under half of the window lands inside `frame`.
fn warp_cost(reference: &Plane, frame: &Plane, warp: Warp) -> f64 {
    let [cx, cy] = reference.centre();
    let (xs, ys) = window(reference);
    let (mut total, mut count, mut tried) = (0.0, 0_usize, 0_usize);
    for y in ys {
        let sy = cy + warp.magnification * (index_f64(y) - cy) + warp.offset[1];
        for x in xs.clone() {
            tried += 1;
            let sx = cx + warp.magnification * (index_f64(x) - cx) + warp.offset[0];
            if let Some(value) = frame.sample(sx, sy) {
                let difference = value - reference.values[y * reference.width + x];
                total += difference * difference;
                count += 1;
            }
        }
    }
    if count * 2 < tried {
        f64::INFINITY
    } else {
        total / index_f64(count)
    }
}

/// The best warp of the coarse grid described at [`ZoomReference::estimate`]; ties keep the
/// first tried, so the result is deterministic.
fn grid_search(reference: &Plane, frame: &Plane) -> Warp {
    let reach = |side: usize| to_index((ZOOM_MAX_OFFSET * index_f64(side)).ceil());
    let (reach_x, reach_y) = (reach(reference.width), reach(reference.height));
    let steps = to_index(((ZOOM_SCALE_MAX - ZOOM_SCALE_MIN) / ZOOM_SCALE_STEP).round());
    let mut best = Warp {
        magnification: 1.0,
        offset: [0.0, 0.0],
    };
    let mut best_cost = warp_cost(reference, frame, best);
    for step in 0..=steps {
        let scale = ZOOM_SCALE_MIN + index_f64(step) * ZOOM_SCALE_STEP;
        for dy in 0..=2 * reach_y {
            for dx in 0..=2 * reach_x {
                let warp = Warp {
                    magnification: 1.0 / scale,
                    offset: [
                        index_f64(dx) - index_f64(reach_x),
                        index_f64(dy) - index_f64(reach_y),
                    ],
                };
                let cost = warp_cost(reference, frame, warp);
                if cost < best_cost {
                    best = warp;
                    best_cost = cost;
                }
            }
        }
    }
    best
}

/// The Gauss-Newton normal equations of one warp, with its cost.
struct NormalEquations {
    /// `Jᵀ J` over the window.
    normal: [[f64; 3]; 3],
    /// `-Jᵀ r` over the window.
    rhs: [f64; 3],
    /// Mean squared residual, infinity when under half of the window lands inside the frame.
    cost: f64,
}

/// Accumulates the normal equations of `warp` for the parameters (magnification, offset x,
/// offset y) over the reference's window.
fn normal_equations(reference: &Plane, frame: &Textured, warp: Warp) -> NormalEquations {
    let [cx, cy] = reference.centre();
    let (xs, ys) = window(reference);
    let mut system = NormalEquations {
        normal: [[0.0; 3]; 3],
        rhs: [0.0; 3],
        cost: 0.0,
    };
    let (mut count, mut tried) = (0_usize, 0_usize);
    for y in ys {
        let ry = index_f64(y) - cy;
        let sy = cy + warp.magnification * ry + warp.offset[1];
        for x in xs.clone() {
            tried += 1;
            let rx = index_f64(x) - cx;
            let sx = cx + warp.magnification * rx + warp.offset[0];
            let Some([value, dx, dy]) = frame.sample(sx, sy) else {
                continue;
            };
            count += 1;
            let residual = value - reference.values[y * reference.width + x];
            system.cost += residual * residual;
            let jacobian = [dx * rx + dy * ry, dx, dy];
            for row in 0..3 {
                system.rhs[row] -= jacobian[row] * residual;
                for col in 0..3 {
                    system.normal[row][col] += jacobian[row] * jacobian[col];
                }
            }
        }
    }
    system.cost = if count * 2 < tried {
        f64::INFINITY
    } else {
        system.cost / index_f64(count)
    };
    system
}

/// Gauss-Newton refinement of `warp` on one pyramid level: the best warp tried is returned, so
/// a step that raises the cost is undone; the magnification stays within the searched range.
fn refine(reference: &Plane, frame: &Plane, warp: Warp) -> Warp {
    let frame = frame.textured();
    let (mut best, mut best_cost) = (warp, f64::INFINITY);
    let mut current = warp;
    // PERFORMANCE RATIONALE:
    // Why: Each pass over the window samples three bilinear values per pixel, about 300,000 per
    // pass at the finest level; a separate cost pass per step would add a third of that again.
    // What: One pass yields the cost of the current warp and its normal equations; a step that
    // did not lower the cost is detected on the next pass and the previous warp is kept.
    // Trade-off: The loop keeps two warps instead of one; the result is the same warp a
    // test-then-accept loop would keep.
    for _ in 0..REFINE_STEPS {
        let system = normal_equations(reference, &frame, current);
        if system.cost >= best_cost {
            break;
        }
        best = current;
        best_cost = system.cost;
        let Some(delta) = solve3(system.normal, system.rhs) else {
            break;
        };
        if delta[0].abs() < STEP_TOLERANCE_MAGNIFICATION
            && delta[1].abs() < STEP_TOLERANCE_PIXELS
            && delta[2].abs() < STEP_TOLERANCE_PIXELS
        {
            break;
        }
        current = Warp {
            magnification: (current.magnification + delta[0])
                .clamp(1.0 / ZOOM_SCALE_MAX, 1.0 / ZOOM_SCALE_MIN),
            offset: [current.offset[0] + delta[1], current.offset[1] + delta[2]],
        };
    }
    best
}

/// Solves the 3×3 system `a x = b` by Gaussian elimination with partial pivoting, or `None`
/// when it is singular.
fn solve3(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> Option<[f64; 3]> {
    let scale = a.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
    if scale == 0.0 {
        return None;
    }
    for col in 0..3 {
        let pivot = (col..3).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() <= scale * 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let pivot_row = a[col];
        for row in col + 1..3 {
            let factor = a[row][col] / pivot_row[col];
            for (value, above) in a[row].iter_mut().zip(pivot_row).skip(col) {
                *value -= factor * above;
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = [0.0; 3];
    for row in (0..3).rev() {
        let known: f64 = (row + 1..3).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - known) / a[row][row];
    }
    Some(x)
}

// --- Line fit -------------------------------------------------------------------------------

/// A least-squares straight line through `(k, values[k])`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineFit {
    /// Value of the line at index 0.
    pub intercept: f64,
    /// Change of the line per index.
    pub slope: f64,
    /// Largest absolute difference between a value and the line.
    pub max_residual: f64,
}

impl LineFit {
    /// The line's value at index `x`.
    #[must_use]
    pub fn at(&self, x: f64) -> f64 {
        self.intercept + self.slope * x
    }
}

/// Fits a straight line to `values` against their indices `0, 1, …` by least squares, or
/// `None` for no values. One value gives a flat line through it.
#[must_use]
pub fn fit_line(values: &[f64]) -> Option<LineFit> {
    let n = index_f64(values.len());
    let mean_y = values.iter().sum::<f64>() / n;
    if values.len() < 2 {
        return values.first().map(|&value| LineFit {
            intercept: value,
            slope: 0.0,
            max_residual: 0.0,
        });
    }
    let mean_x = (n - 1.0) / 2.0;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (k, value) in values.iter().enumerate() {
        let dx = index_f64(k) - mean_x;
        sxy += dx * (value - mean_y);
        sxx += dx * dx;
    }
    let slope = sxy / sxx;
    let mut fit = LineFit {
        intercept: mean_y - slope * mean_x,
        slope,
        max_residual: 0.0,
    };
    fit.max_residual = values
        .iter()
        .enumerate()
        .map(|(k, value)| (value - fit.at(index_f64(k))).abs())
        .fold(0.0, f64::max);
    Some(fit)
}

// --- Scale lock -----------------------------------------------------------------------------

/// What [`plan_scale_lock`] measured and decided for one clip's kept frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleLockPlan {
    /// Number of kept frames measured.
    pub frame_count: usize,
    /// The line fitted to each frame's `ln(magnification)` against frame 0.
    pub log_magnification: LineFit,
    /// The line fitted to each frame's horizontal offset, in pixels of the measured frames.
    pub offset_x: LineFit,
    /// The line fitted to each frame's vertical offset, in pixels of the measured frames.
    pub offset_y: LineFit,
    /// Fitted change of magnification from the first kept frame to the last, as a fraction
    /// (0.06 for a 6 % push-in, negative for a pull-out).
    pub growth: f64,
    /// Largest difference between a measured `ln(magnification)` and the line: about the
    /// relative scale error, so 0.005 is 0.5 %.
    pub residual: f64,
    /// Whether [`apply_scale_lock`] changes the frames: false when the zoom is under
    /// [`SCALE_LOCK_MIN_GROWTH`].
    pub apply: bool,
}

impl ScaleLockPlan {
    /// The kept frame every frame is enlarged to: the most zoomed in on the fitted line.
    fn target(&self) -> usize {
        if self.log_magnification.slope >= 0.0 {
            self.frame_count.saturating_sub(1)
        } else {
            0
        }
    }

    /// The enlargement applied to the most enlarged frame (the first one, for a push-in) as a
    /// fraction, or 0 when nothing is applied.
    #[must_use]
    pub fn enlargement(&self) -> f64 {
        if !self.apply {
            return 0.0;
        }
        let span = index_f64(self.frame_count.saturating_sub(1));
        (self.log_magnification.slope.abs() * span).exp() - 1.0
    }

    /// [`ScaleLockPlan::enlargement`] in per mille, rounded.
    #[must_use]
    pub fn enlargement_permille(&self) -> u32 {
        rounded_permille(self.enlargement())
    }
}

/// `fraction` in per mille, rounded, saturating at `u32::MAX`; 0 for a negative fraction.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn rounded_permille(fraction: f64) -> u32 {
    (fraction * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// Measures every frame's zoom against frame 0, fits it as a straight line in
/// `ln(magnification)` (and the offsets as straight lines) and decides the lock (D-5):
/// under [`SCALE_LOCK_MIN_GROWTH`] nothing is done; otherwise a residual over
/// [`SCALE_LOCK_MAX_RESIDUAL`] or a zoom over [`SCALE_LOCK_MAX_GROWTH`] refuses the clip.
///
/// `frames` are the kept frames in order, all of one size; `label` names the clip in errors.
///
/// # Errors
/// [`BakerError::SeamRepairRefused`] as above, or [`BakerError::ValidationFailed`] for frames of
/// different or too small sizes.
pub fn plan_scale_lock(frames: &[RgbaImage], label: &str) -> Result<ScaleLockPlan, BakerError> {
    let mut logs = Vec::with_capacity(frames.len());
    let mut xs = Vec::with_capacity(frames.len());
    let mut ys = Vec::with_capacity(frames.len());
    if let Some(first) = frames.first() {
        let reference = ZoomReference::new(first)?;
        logs.push(0.0);
        xs.push(0.0);
        ys.push(0.0);
        for frame in &frames[1..] {
            let estimate = reference.estimate(frame)?;
            logs.push(estimate.magnification.ln());
            xs.push(estimate.offset[0]);
            ys.push(estimate.offset[1]);
        }
    }
    let flat = LineFit {
        intercept: 0.0,
        slope: 0.0,
        max_residual: 0.0,
    };
    let log_magnification = fit_line(&logs).unwrap_or(flat);
    let span = index_f64(frames.len().saturating_sub(1));
    let mut plan = ScaleLockPlan {
        frame_count: frames.len(),
        log_magnification,
        offset_x: fit_line(&xs).unwrap_or(flat),
        offset_y: fit_line(&ys).unwrap_or(flat),
        growth: (log_magnification.slope * span).exp() - 1.0,
        residual: log_magnification.max_residual,
        apply: false,
    };
    let depth = (log_magnification.slope.abs() * span).exp() - 1.0;
    if depth < SCALE_LOCK_MIN_GROWTH {
        return Ok(plan);
    }
    let refuse = |reason: String| BakerError::SeamRepairRefused {
        clip: label.to_string(),
        growth_percent: plan.growth * 100.0,
        residual_percent: plan.residual * 100.0,
        reason,
    };
    if plan.residual > SCALE_LOCK_MAX_RESIDUAL {
        return Err(refuse(format!(
            "the scale of some frames departs from a steady zoom by more than {} %",
            SCALE_LOCK_MAX_RESIDUAL * 100.0
        )));
    }
    if depth > SCALE_LOCK_MAX_GROWTH {
        return Err(refuse(format!(
            "the zoom is deeper than {} %",
            SCALE_LOCK_MAX_GROWTH * 100.0
        )));
    }
    plan.apply = true;
    Ok(plan)
}

/// Every frame enlarged about the fitted centre to the framing of the plan's most zoomed-in
/// frame, at the same size, by bilinear sampling at sub-pixel positions; the frames unchanged
/// when the plan applies nothing.
///
/// Output pixel `q` of kept frame `k` samples frame `k` at
/// `c + (M_k / M_t)(q - c - D_t) + D_k`, where `M` and `D` are the fitted magnification and
/// offset and `t` the target frame. A sample beyond the frame repeats its edge pixel.
#[must_use]
pub fn apply_scale_lock(frames: &[RgbaImage], plan: &ScaleLockPlan) -> Vec<RgbaImage> {
    if !plan.apply {
        return frames.to_vec();
    }
    let target = index_f64(plan.target());
    let target_offset = [plan.offset_x.at(target), plan.offset_y.at(target)];
    frames
        .iter()
        .enumerate()
        .map(|(k, frame)| {
            let k = index_f64(k);
            let ratio = (plan.log_magnification.at(k) - plan.log_magnification.at(target)).exp();
            let offset = [plan.offset_x.at(k), plan.offset_y.at(k)];
            resample(frame, ratio, target_offset, offset)
        })
        .collect()
}

/// `frame` sampled at `c + ratio (q - c - before) + after` for every pixel `q`.
fn resample(frame: &RgbaImage, ratio: f64, before: [f64; 2], after: [f64; 2]) -> RgbaImage {
    let (w, h) = (frame.width() as usize, frame.height() as usize);
    let cx = index_f64(w.saturating_sub(1)) / 2.0;
    let cy = index_f64(h.saturating_sub(1)) / 2.0;
    let (max_x, max_y) = (index_f64(w - 1), index_f64(h - 1));
    let source = frame.as_raw();
    let mut resampled = RgbaImage::new(frame.width(), frame.height());
    let out: &mut [u8] = &mut resampled;
    // PERFORMANCE RATIONALE:
    // Why: A twice-size Veo state is 16 frames of 1024×576 pixels: 9.4 million bilinear samples
    // of four channels per bake; `get_pixel`/`put_pixel` bounds-check and copy every access.
    // What: The source columns and weights of every output column are computed once per frame
    // (the map is the same for every row), each row's source rows once, and the loop reads and
    // writes the raw RGBA bytes through row slices.
    // Trade-off: A per-frame column table and index arithmetic in place of pixel accessors, for
    // a repair well under a second.
    let columns: Vec<(usize, usize, f64)> = (0..w)
        .map(|x| {
            let sx = (cx + ratio * (index_f64(x) - cx - before[0]) + after[0]).clamp(0.0, max_x);
            let x0 = to_index(sx);
            (4 * x0, 4 * (x0 + 1).min(w - 1), sx - index_f64(x0))
        })
        .collect();
    let row_bytes = 4 * w;
    for (y, out_row) in out.chunks_exact_mut(row_bytes).enumerate() {
        let sy = (cy + ratio * (index_f64(y) - cy - before[1]) + after[1]).clamp(0.0, max_y);
        let y0 = to_index(sy);
        let fy = sy - index_f64(y0);
        let top = &source[y0 * row_bytes..(y0 + 1) * row_bytes];
        let y1 = (y0 + 1).min(h - 1);
        let bottom = &source[y1 * row_bytes..(y1 + 1) * row_bytes];
        for (pixel, &(left, right, fx)) in out_row.as_chunks_mut::<4>().0.iter_mut().zip(&columns) {
            for (c, channel) in pixel.iter_mut().enumerate() {
                let upper = lerp(top[left + c], top[right + c], fx);
                let lower = lerp(bottom[left + c], bottom[right + c], fx);
                *channel = to_byte(upper + (lower - upper) * fy);
            }
        }
    }
    resampled
}

/// `a + (b - a) · t` for two bytes.
fn lerp(a: u8, b: u8, t: f64) -> f64 {
    let a = f64::from(a);
    a + (f64::from(b) - a) * t
}

/// `frame` at half its size: each pixel is the rounded mean of a 2×2 block (a box filter); an
/// odd last row or column is dropped.
#[must_use]
pub fn downsample_half(frame: &RgbaImage) -> RgbaImage {
    let (w, h) = (frame.width() / 2, frame.height() / 2);
    let mut half = RgbaImage::new(w, h);
    let source = frame.as_raw();
    let source_row = 4 * frame.width() as usize;
    let out: &mut [u8] = &mut half;
    // PERFORMANCE RATIONALE:
    // Why: Every repaired frame passes through here: 16 frames of 1024×576 per Veo state, where
    // `get_pixel` would bounds-check and copy each of 9.4 million reads.
    // What: The loop walks the raw RGBA bytes of each pair of source rows by slice.
    // Trade-off: Index arithmetic in place of pixel accessors; the result is identical.
    for (y, out_row) in out.chunks_exact_mut(4 * w as usize).enumerate() {
        let top = &source[2 * y * source_row..(2 * y + 1) * source_row];
        let bottom = &source[(2 * y + 1) * source_row..(2 * y + 2) * source_row];
        for (x, pixel) in out_row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            for (c, channel) in pixel.iter_mut().enumerate() {
                let sum = u16::from(top[8 * x + c])
                    + u16::from(top[8 * x + 4 + c])
                    + u16::from(bottom[8 * x + c])
                    + u16::from(bottom[8 * x + 4 + c]);
                *channel = u8::try_from((sum + 2) / 4).unwrap_or(u8::MAX);
            }
        }
    }
    half
}

// --- Cross-fade -----------------------------------------------------------------------------

/// The weight toward frame 0 of the `j`-th of the last `n` frames: `(j + 1) / (n + 1)`.
#[must_use]
pub fn crossfade_weight(j: usize, n: usize) -> f64 {
    index_f64(j + 1) / index_f64(n + 1)
}

/// Blends the last `n` frames toward frame 0 in linear light: the `j`-th of them becomes
/// `(1 - w) · itself + w · frame 0` with `w` = [`crossfade_weight`]`(j, n)`, per colour channel,
/// converted from and back to sRGB bytes. Frame 0, the other frames and alpha are unchanged.
///
/// # Errors
/// [`BakerError::ValidationFailed`] if `n` exceeds [`MAX_CROSSFADE_FRAMES`] or a quarter of the
/// frames, or the frames differ in size.
pub fn crossfade_into_first(frames: &mut [RgbaImage], n: usize) -> Result<(), BakerError> {
    if n == 0 {
        return Ok(());
    }
    if n > usize::from(MAX_CROSSFADE_FRAMES) || 4 * n > frames.len() {
        return Err(BakerError::ValidationFailed {
            name: "cross-fade".into(),
            reason: format!(
                "{n} cross-fade frames of {} kept: at most {MAX_CROSSFADE_FRAMES}, and at most a quarter of the frames",
                frames.len()
            ),
        });
    }
    let Some((first, rest)) = frames.split_first_mut() else {
        return Ok(());
    };
    // PERFORMANCE RATIONALE:
    // Why: The sRGB decoding curve takes a power function per channel and pixel.
    // What: A 256-entry table holds the linear value of every sRGB byte.
    // Trade-off: 1 KiB of table per call; the values are those `srgb_to_linear` returns.
    let mut linear = [0.0_f32; 256];
    for (byte, value) in (0_u8..=255).zip(linear.iter_mut()) {
        *value = srgb_to_linear(f32::from(byte) / 255.0);
    }
    let start = rest.len() - n;
    for (j, frame) in rest[start..].iter_mut().enumerate() {
        require_same_size(first, frame, "cross-fade")?;
        let weight = crossfade_weight(j, n);
        for (pixel, zero) in frame.pixels_mut().zip(first.pixels()) {
            for c in 0..3 {
                let own = f64::from(linear[usize::from(pixel[c])]);
                let target = f64::from(linear[usize::from(zero[c])]);
                let mixed = (1.0 - weight) * own + weight * target;
                pixel[c] = to_byte(f64::from(linear_to_srgb(narrow(mixed))) * 255.0);
            }
        }
    }
    Ok(())
}

/// A linear-light value in 0 to 1 as `f32`, the precision of the sRGB curve functions.
#[allow(clippy::cast_possible_truncation)]
fn narrow(value: f64) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plane_halves_by_averaging_blocks() {
        let plane = Plane {
            width: 3,
            height: 2,
            values: vec![0.0, 4.0, 9.0, 8.0, 12.0, 9.0],
        };
        let half = plane.half();
        assert_eq!((half.width, half.height), (1, 1));
        assert!((half.values[0] - 6.0).abs() < 1e-12);
    }

    #[test]
    fn bilinear_samples_interpolate_and_refuse_outside() {
        let plane = Plane {
            width: 2,
            height: 2,
            values: vec![0.0, 10.0, 20.0, 30.0],
        };
        assert_eq!(plane.sample(0.5, 0.5), Some(15.0));
        assert_eq!(plane.sample(1.0, 1.0), Some(30.0));
        assert_eq!(plane.sample(1.01, 0.0), None);
        assert_eq!(plane.sample(-0.01, 0.0), None);
    }

    #[test]
    fn the_three_by_three_solver_inverts_and_refuses_singular_systems() {
        let a = [[2.0, 1.0, 0.0], [1.0, 3.0, 1.0], [0.0, 1.0, 4.0]];
        let x = solve3(a, [3.0, 5.0, 5.0]).unwrap_or([f64::NAN; 3]);
        for (value, expected) in x.iter().zip([1.0, 1.0, 1.0]) {
            assert!((value - expected).abs() < 1e-12, "{x:?}");
        }
        assert!(solve3(
            [[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [0.0, 0.0, 1.0]],
            [1.0; 3]
        )
        .is_none());
        assert!(solve3([[0.0; 3]; 3], [0.0; 3]).is_none());
    }

    #[test]
    fn a_box_downsample_rounds_the_block_mean() {
        let mut frame = RgbaImage::new(2, 2);
        for (i, pixel) in frame.pixels_mut().enumerate() {
            let v = u8::try_from(i).unwrap_or(0);
            *pixel = image::Rgba([v, 10 * v, 255, 255]);
        }
        let half = downsample_half(&frame);
        assert_eq!(half.get_pixel(0, 0).0, [2, 15, 255, 255]);
    }

    #[test]
    fn resampling_with_ratio_one_and_no_shift_is_the_identity() {
        let frame = RgbaImage::from_fn(9, 5, |x, y| {
            image::Rgba([u8::try_from(x * 20 + y).unwrap_or(0), 7, 9, 255])
        });
        assert_eq!(resample(&frame, 1.0, [0.0, 0.0], [0.0, 0.0]), frame);
    }

    #[test]
    fn per_mille_rounds_and_saturates() {
        assert_eq!(rounded_permille(0.0564), 56);
        assert_eq!(rounded_permille(-0.2), 0);
    }
}
