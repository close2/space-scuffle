//! How a decoded clip frame becomes a square game frame: the framing policy, the measured bar
//! colour, the edge feather and the composer (plan `docs/plans/2026-11-04-veo-letterbox-bake.md`,
//! decisions D-1 to D-3).
//!
//! # Responsibility
//! - [`BakeFraming`] names one of two treatments. `CenterCrop` cuts the centred square out of
//!   the clip, as every bake did before this module existed. `Letterbox` scales the whole clip
//!   frame to fit inside the square and fills the rest with opaque bars.
//! - [`FramingPolicy`] is the `framing` section of `assets/catalog/media_style.ron`: the
//!   treatment per render shape (`UnitSphere`, `BuildingDome`) and per-identity overrides. It is
//!   data, so which identities are letterboxed needs no code change (charter §3.3).
//! - [`measure_bar_rgb`] picks one bar colour per clip from the clip's own border, and
//!   [`frame_to_square`] composes every kept frame onto the square canvas with that colour and a
//!   linear feather.
//!
//! # How it fits
//! `ffmpeg_importer` asks FFmpeg for frames with [`BakeFraming::ffmpeg_filter`]: either already
//! square (crop) or scaled to fit (letterbox). After the frame budget has chosen the frames to
//! keep, it calls [`frame_to_square`]. `provenance` records the framing and the bar colour, and
//! `clip_review` builds the reviewer's "framed" contact sheet with the same functions, so the
//! review judges exactly the square that ships.
//!
//! # Invariants
//! - **No transparency.** The client's bubble shader samples only the RGB of a layer, so bars are
//!   opaque (alpha 255) and must look like the clip's backdrop.
//! - **Deterministic bytes.** Measuring and composing are integer arithmetic over the decoded
//!   frames in a fixed order, so the same decoded frames always give the same atlas bytes.
//! - **One colour per clip.** Every frame of a clip gets the same bar colour, so bars never
//!   flicker.
//! - **A square clip is untouched.** When the decoded frame already fills the square, no bar
//!   exists and the frames are returned unchanged, so both framings give the same bytes.

use crate::error::BakerError;
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use sim_types::enums::ShapeKind;
use std::collections::BTreeMap;
use std::path::Path;

/// Default side of a baked identity frame, in pixels (`bake-unit-videos --size`).
pub const DEFAULT_UNIT_FRAME_SIZE: u32 = 512;

/// Default playback rate of a baked identity atlas, in frames per second
/// (`bake-unit-videos --fps`).
pub const DEFAULT_UNIT_FPS: u32 = 15;

/// Largest accepted feather, in thousandths of the frame side. At 500 the blend would reach the
/// middle of a square frame; a wider value has no meaning.
pub const MAX_FEATHER_PERMILLE: u32 = 500;

/// The border band measured for the bar colour is `short_side / BAND_DIVISOR` pixels thick
/// (at least one), where `short_side` is the shorter side of the decoded frame: 4 rows of a
/// 512×288 frame.
pub const BAND_DIVISOR: u32 = 64;

/// How a clip frame is fitted into the square game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BakeFraming {
    /// Cut the centred square of side `min(width, height)` out of the frame and scale it. Parts
    /// of a wide subject beyond the square are lost.
    #[default]
    CenterCrop,
    /// Scale the whole frame to fit inside the square and fill the short axis with opaque bars
    /// of the clip's measured backdrop colour.
    Letterbox {
        /// Width of the linear blend from the clip's own pixels to the bar colour, in
        /// thousandths of the frame side (30 gives 15 px at 512 px). 0 leaves a hard edge.
        feather_permille: u32,
    },
}

impl BakeFraming {
    /// The FFmpeg filter that decodes a clip into frames for this framing at `frame_size`
    /// pixels and `fps` frames per second.
    ///
    /// `CenterCrop` returns square frames. `Letterbox` returns frames that fit inside the square
    /// with the clip's aspect ratio (a 1280×720 clip at 512 becomes 512×288); the square is
    /// completed by [`frame_to_square`].
    #[must_use]
    pub fn ffmpeg_filter(self, frame_size: u32, fps: u32) -> String {
        match self {
            Self::CenterCrop => {
                format!("crop=min(iw\\,ih):min(iw\\,ih),scale={frame_size}:{frame_size},fps={fps}")
            }
            Self::Letterbox { .. } => format!(
                "scale={frame_size}:{frame_size}:force_original_aspect_ratio=decrease,fps={fps}"
            ),
        }
    }

    /// Number of content rows (or columns) next to a bar that the feather blends, for a frame
    /// side of `frame_size` pixels. Always 0 for `CenterCrop`.
    #[must_use]
    pub fn feather_pixels(self, frame_size: u32) -> u32 {
        match self {
            Self::CenterCrop => 0,
            Self::Letterbox { feather_permille } => {
                let pixels = u64::from(frame_size) * u64::from(feather_permille) / 1000;
                u32::try_from(pixels).unwrap_or(u32::MAX)
            }
        }
    }
}

impl std::fmt::Display for BakeFraming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CenterCrop => f.write_str("centre crop"),
            Self::Letterbox { feather_permille } => {
                write!(f, "letterbox (feather {feather_permille} per mille)")
            }
        }
    }
}

/// The `framing` section of `media_style.ron`: which treatment each identity's clips get.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FramingPolicy {
    /// Treatment of identities rendered as a `UnitSphere` (mobile units).
    pub unit_sphere: BakeFraming,
    /// Treatment of identities rendered as a `BuildingDome` (structures).
    pub building_dome: BakeFraming,
    /// Exact catalog slug to treatment, for one identity whose clip needs the other treatment.
    #[serde(default)]
    pub overrides: BTreeMap<String, BakeFraming>,
}

impl FramingPolicy {
    /// The treatment of identity `slug`, whose render shape is `shape`: its override if the
    /// policy names it, else the treatment of its shape.
    #[must_use]
    pub fn framing_for(&self, slug: &str, shape: &ShapeKind) -> BakeFraming {
        if let Some(framing) = self.overrides.get(slug) {
            return *framing;
        }
        match shape {
            ShapeKind::UnitSphere => self.unit_sphere,
            ShapeKind::BuildingDome { .. } => self.building_dome,
        }
    }

    /// Checks the policy loaded from `path` against `catalog`.
    ///
    /// # Errors
    /// [`BakerError::UnknownFramingOverride`] for an override whose slug the catalog does not
    /// hold exactly, or [`BakerError::DataFile`] for a feather wider than
    /// [`MAX_FEATHER_PERMILLE`].
    pub fn check(&self, catalog: &sim_core::UnitCatalog, path: &Path) -> Result<(), BakerError> {
        for slug in self.overrides.keys() {
            if catalog.get_by_slug(slug).is_none_or(|a| a.slug != *slug) {
                return Err(BakerError::UnknownFramingOverride {
                    path: path.to_path_buf(),
                    slug: slug.clone(),
                });
            }
        }
        let every = [self.unit_sphere, self.building_dome]
            .into_iter()
            .chain(self.overrides.values().copied());
        for framing in every {
            if let BakeFraming::Letterbox { feather_permille } = framing {
                if feather_permille > MAX_FEATHER_PERMILLE {
                    return Err(BakerError::DataFile {
                        path: path.to_path_buf(),
                        reason: format!(
                            "feather_permille {feather_permille} is wider than {MAX_FEATHER_PERMILLE}"
                        ),
                    });
                }
            }
        }
        Ok(())
    }
}

/// Where the bars of a decoded frame go when it is completed to a square.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bars {
    /// The frame already fills the square.
    None,
    /// Above and below the content: the clip is wider than tall (letterbox).
    TopBottom,
    /// Left and right of the content: the clip is taller than wide (pillarbox).
    LeftRight,
}

impl Bars {
    /// The bars a `width`×`height` decoded frame needs inside a square of `frame_size`.
    #[must_use]
    pub fn for_content(width: u32, height: u32, frame_size: u32) -> Self {
        if height < frame_size && width >= height {
            Self::TopBottom
        } else if width < frame_size {
            Self::LeftRight
        } else {
            Self::None
        }
    }

    /// Distance in pixels from `(x, y)` of a `width`×`height` content frame to the nearest edge
    /// that touches a bar, or `None` when there is no bar.
    fn distance_to_bar(self, (x, y): (u32, u32), (width, height): (u32, u32)) -> Option<u32> {
        match self {
            Self::None => None,
            Self::TopBottom => Some(y.min(height - 1 - y)),
            Self::LeftRight => Some(x.min(width - 1 - x)),
        }
    }
}

/// The frames of one clip completed to squares, and the bar colour chosen for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramedFrames {
    /// Square frames of the requested side, in temporal order.
    pub frames: Vec<RgbaImage>,
    /// The measured bar colour as sRGB bytes; `None` when no bar was drawn.
    pub bar_rgb: Option<[u8; 3]>,
}

/// The per-channel median of the outermost border band of every frame along the edges that
/// receive `bars`, or `None` for [`Bars::None`] or no frames.
///
/// The band is `max(1, short_side / BAND_DIVISOR)` pixels thick, `short_side` being the
/// shorter side of the frames. The median is the lower median (rank `(n - 1) / 2` of the `n`
/// sorted values) of each channel independently. A subject that touches the edge over less than
/// half of the band, such as rising vapour, therefore does not move it.
///
/// # Errors
/// [`BakerError::ValidationFailed`] if the frames do not all have the same size.
pub fn measure_bar_rgb(frames: &[RgbaImage], bars: Bars) -> Result<Option<[u8; 3]>, BakerError> {
    let Some(first) = frames.first() else {
        return Ok(None);
    };
    let (width, height) = first.dimensions();
    if bars == Bars::None {
        return Ok(None);
    }
    require_uniform(frames, width, height)?;
    let band = (width.min(height) / BAND_DIVISOR).max(1);
    // PERFORMANCE RATIONALE:
    // Why: A 512×288 clip of 32 frames has about 130,000 border pixels per channel; sorting
    // copies of them would allocate and cost O(n log n).
    // What: Three fixed 256-bin histograms on the stack give the exact median in one pass.
    // Trade-off: None in clarity worth noting; the result is identical to sorting.
    let mut histograms = [[0_u64; 256]; 3];
    for frame in frames {
        for (x, y, pixel) in frame.enumerate_pixels() {
            let in_band = match bars {
                Bars::TopBottom => y < band || y >= height.saturating_sub(band),
                Bars::LeftRight => x < band || x >= width.saturating_sub(band),
                Bars::None => false,
            };
            if in_band {
                for (channel, histogram) in histograms.iter_mut().enumerate() {
                    histogram[usize::from(pixel[channel])] += 1;
                }
            }
        }
    }
    Ok(Some(histograms.map(|histogram| lower_median(&histogram))))
}

/// The lower median of the values counted in `histogram`.
fn lower_median(histogram: &[u64; 256]) -> u8 {
    let total: u64 = histogram.iter().sum();
    let rank = total.saturating_sub(1) / 2;
    let mut seen = 0_u64;
    for (value, count) in (0_u8..=255).zip(histogram) {
        seen += count;
        if seen > rank {
            return value;
        }
    }
    u8::MAX
}

fn require_uniform(frames: &[RgbaImage], width: u32, height: u32) -> Result<(), BakerError> {
    match frames.iter().find(|f| f.dimensions() != (width, height)) {
        Some(other) => Err(BakerError::ValidationFailed {
            name: "decoded clip".into(),
            reason: format!(
                "decoded frames differ in size: {width}x{height} and {}x{}",
                other.width(),
                other.height()
            ),
        }),
        None => Ok(()),
    }
}

/// Completes the decoded frames of one clip to `frame_size`×`frame_size` squares.
///
/// `CenterCrop` frames are already square and are returned unchanged. `Letterbox` frames that
/// do not fill the square are centred on a canvas of the colour [`measure_bar_rgb`] chooses, and
/// the content's outermost [`BakeFraming::feather_pixels`] rows (or columns) next to each bar
/// are blended linearly toward that colour. A frame that already fills the square is returned
/// unchanged under either framing.
///
/// # Errors
/// [`BakerError::ValidationFailed`] if the frames differ in size, are larger than the square,
/// or are not square under `CenterCrop`.
pub fn frame_to_square(
    frames: Vec<RgbaImage>,
    frame_size: u32,
    framing: BakeFraming,
) -> Result<FramedFrames, BakerError> {
    let Some((width, height)) = frames.first().map(RgbaImage::dimensions) else {
        return Ok(FramedFrames {
            frames,
            bar_rgb: None,
        });
    };
    require_uniform(&frames, width, height)?;
    let unchanged = (width, height) == (frame_size, frame_size);
    if unchanged || framing == BakeFraming::CenterCrop {
        if !unchanged {
            return Err(BakerError::ValidationFailed {
                name: "decoded clip".into(),
                reason: format!(
                    "a centre-cropped frame is {width}x{height}, not {frame_size}x{frame_size}"
                ),
            });
        }
        return Ok(FramedFrames {
            frames,
            bar_rgb: None,
        });
    }
    if width > frame_size || height > frame_size {
        return Err(BakerError::ValidationFailed {
            name: "decoded clip".into(),
            reason: format!("a {width}x{height} frame does not fit a {frame_size} px square"),
        });
    }
    let bars = Bars::for_content(width, height, frame_size);
    let Some(bar_rgb) = measure_bar_rgb(&frames, bars)? else {
        return Ok(FramedFrames {
            frames,
            bar_rgb: None,
        });
    };
    let short = match bars {
        Bars::LeftRight => width,
        Bars::TopBottom | Bars::None => height,
    };
    let feather = framing.feather_pixels(frame_size).min(short / 2);
    let layout = Composition {
        frame_size,
        bar_rgb,
        bars,
        feather,
    };
    Ok(FramedFrames {
        frames: frames.iter().map(|frame| layout.compose(frame)).collect(),
        bar_rgb: Some(bar_rgb),
    })
}

/// Everything needed to compose one content frame onto its square.
struct Composition {
    frame_size: u32,
    bar_rgb: [u8; 3],
    bars: Bars,
    feather: u32,
}

impl Composition {
    /// The content centred on a bar-coloured square, feathered next to the bars.
    fn compose(&self, content: &RgbaImage) -> RgbaImage {
        let [r, g, b] = self.bar_rgb;
        let mut square =
            RgbaImage::from_pixel(self.frame_size, self.frame_size, Rgba([r, g, b, 255]));
        let (width, height) = content.dimensions();
        let left = (self.frame_size - width) / 2;
        let top = (self.frame_size - height) / 2;
        for (x, y, pixel) in content.enumerate_pixels() {
            let distance = self.bars.distance_to_bar((x, y), (width, height));
            let blended = match distance {
                Some(d) if d < self.feather => self.blend(*pixel, self.feather - d),
                _ => Rgba([pixel[0], pixel[1], pixel[2], 255]),
            };
            square.put_pixel(left + x, top + y, blended);
        }
        square
    }

    /// `pixel` moved `weight / (feather + 1)` of the way toward the bar colour, rounded to the
    /// nearest integer. `weight` runs from `feather` at the outermost row down to 1.
    fn blend(&self, pixel: Rgba<u8>, weight: u32) -> Rgba<u8> {
        let denominator = self.feather + 1;
        let mix = |own: u8, bar: u8| -> u8 {
            let value = (u32::from(own) * (denominator - weight)
                + u32::from(bar) * weight
                + denominator / 2)
                / denominator;
            u8::try_from(value).unwrap_or(u8::MAX)
        };
        Rgba([
            mix(pixel[0], self.bar_rgb[0]),
            mix(pixel[1], self.bar_rgb[1]),
            mix(pixel[2], self.bar_rgb[2]),
            255,
        ])
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const BACKDROP: [u8; 4] = [44, 48, 74, 255];

    fn wide(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba(BACKDROP))
    }

    #[test]
    fn the_crop_filter_is_byte_for_byte_the_old_literal() {
        assert_eq!(
            BakeFraming::CenterCrop.ffmpeg_filter(512, 15),
            "crop=min(iw\\,ih):min(iw\\,ih),scale=512:512,fps=15"
        );
        assert_eq!(
            BakeFraming::Letterbox {
                feather_permille: 30
            }
            .ffmpeg_filter(512, 15),
            "scale=512:512:force_original_aspect_ratio=decrease,fps=15"
        );
    }

    #[test]
    fn the_feather_is_a_share_of_the_frame_side() {
        let framing = BakeFraming::Letterbox {
            feather_permille: 30,
        };
        assert_eq!(framing.feather_pixels(512), 15);
        assert_eq!(BakeFraming::CenterCrop.feather_pixels(512), 0);
    }

    #[test]
    fn bars_follow_the_short_axis() {
        assert_eq!(Bars::for_content(512, 288, 512), Bars::TopBottom);
        assert_eq!(Bars::for_content(288, 512, 512), Bars::LeftRight);
        assert_eq!(Bars::for_content(512, 512, 512), Bars::None);
    }

    #[test]
    fn a_bright_plume_on_a_fifth_of_the_top_edge_does_not_move_the_median() {
        let mut frame = wide(64, 36);
        for x in 0..13 {
            for y in 0..3 {
                frame.put_pixel(x + 20, y, Rgba([250, 250, 250, 255]));
            }
        }
        let measured = measure_bar_rgb(&[frame], Bars::TopBottom).unwrap();
        assert_eq!(measured, Some([44, 48, 74]));
    }

    #[test]
    fn the_lower_median_of_an_even_count_is_the_smaller_middle_value() {
        let mut histogram = [0_u64; 256];
        histogram[10] = 2;
        histogram[20] = 2;
        assert_eq!(lower_median(&histogram), 10);
        histogram[20] = 3;
        assert_eq!(lower_median(&histogram), 20);
    }

    #[test]
    fn frames_of_different_sizes_are_refused() {
        let frames = vec![wide(64, 36), wide(64, 34)];
        let framing = BakeFraming::Letterbox {
            feather_permille: 0,
        };
        assert!(matches!(
            frame_to_square(frames, 64, framing),
            Err(BakerError::ValidationFailed { .. })
        ));
    }

    #[test]
    fn a_centre_crop_frame_must_already_be_square() {
        assert!(frame_to_square(vec![wide(64, 36)], 64, BakeFraming::CenterCrop).is_err());
        let square = vec![wide(64, 64)];
        let framed = frame_to_square(square.clone(), 64, BakeFraming::CenterCrop).unwrap();
        assert_eq!(framed.frames, square);
        assert_eq!(framed.bar_rgb, None);
    }
}
