//! How a decoded clip frame becomes a square game frame: the framing policy, the measured bar
//! colour, the edge feather and the composer (plan `docs/plans/2026-11-04-veo-letterbox-bake.md`,
//! decisions D-1 to D-3).
//!
//! # Responsibility
//! - [`BakeFraming`] names one of three treatments. `CenterCrop` cuts the centred square out of
//!   the clip, as every bake did before this module existed. `Letterbox` scales the whole clip
//!   frame to fit inside the square and fills the rest with opaque bars. `ReframedLetterbox`
//!   (a mode's `reframe`, plan `docs/plans/2026-11-09-effect-visualization.md` Amendment 16)
//!   first crops a clip made from a [`reframe_picture`] back to the default scale, keeping the
//!   margin above and below the subject.
//! - [`reframe_picture`] shrinks a start picture about its centre into a margin of its own
//!   backdrop, so a clip has room for a pose that leaves the default frame.
//! - [`FramingPolicy`] is the `framing` section of `assets/catalog/media_style.ron`: the
//!   treatment per render shape (`UnitSphere`, `BuildingDome`) and per-identity overrides. It is
//!   data, so which identities are letterboxed needs no code change (charter §3.3).
//! - [`BakeFraming::ffmpeg_filter_supersampled`] decodes a looping state at twice the size for
//!   the seam repair's scale lock (plan `docs/plans/2026-11-04-veo-seamless-loops.md`, D-5).
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

/// Factor by which a state with a scale lock is decoded larger than its frame size, so the
/// lock's sub-pixel enlargement keeps detail ([`BakeFraming::ffmpeg_filter_supersampled`]).
pub const SUPERSAMPLE: u32 = 2;

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
    /// A [`Self::Letterbox`] of a clip generated from a **reframed** start picture
    /// ([`reframe_picture`]): the identity's picture scaled by `scale_permille` about its
    /// centre, with a margin of backdrop on every side. The bake first crops the clip to the
    /// centre `scale_permille` of its width, over its whole height, which brings the subject back
    /// to the scale and place it has in the identity's default atlas; the margins above and below
    /// stay in the frame, where a default atlas has its bars, so a part that rises above the
    /// default frame (a raised barrel, a muzzle blast) is kept instead of cut.
    ReframedLetterbox {
        /// As [`Self::Letterbox`]'s.
        feather_permille: u32,
        /// The reframe scale in thousandths, from [`MIN_REFRAME_PERMILLE`] to
        /// [`MAX_REFRAME_PERMILLE`].
        scale_permille: u32,
    },
}

/// Smallest accepted reframe scale, in thousandths: below half size the clip spends most of its
/// pixels on margin.
pub const MIN_REFRAME_PERMILLE: u32 = 500;

/// Largest accepted reframe scale, in thousandths; 1000 would be no reframe at all.
pub const MAX_REFRAME_PERMILLE: u32 = 999;

/// The side, in pixels, that a reframe at `scale_permille` keeps of a side of `side` pixels:
/// `side · scale_permille / 1000`, rounded down to an even number, so a centred crop has whole
/// pixels on both sides. [`reframe_picture`] and [`BakeFraming::ffmpeg_filter`] both use it.
#[must_use]
pub fn reframed_side(side: u32, scale_permille: u32) -> u32 {
    let scaled = u64::from(side) * u64::from(scale_permille) / 2000 * 2;
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

impl BakeFraming {
    /// The reframed form of this framing at `scale_permille` ([`Self::ReframedLetterbox`]),
    /// keeping its feather; `None` for [`Self::CenterCrop`], whose square crop would cut the
    /// margin a reframe makes, and for a framing that is already reframed.
    #[must_use]
    pub fn reframed(self, scale_permille: u32) -> Option<Self> {
        match self {
            Self::Letterbox { feather_permille } => Some(Self::ReframedLetterbox {
                feather_permille,
                scale_permille,
            }),
            Self::CenterCrop | Self::ReframedLetterbox { .. } => None,
        }
    }

    /// The FFmpeg crop that undoes a reframe, followed by a comma, or an empty string.
    fn reframe_crop(self) -> String {
        match self {
            Self::ReframedLetterbox { scale_permille, .. } => {
                format!("crop=trunc(iw*{scale_permille}/2000)*2:ih,")
            }
            Self::CenterCrop | Self::Letterbox { .. } => String::new(),
        }
    }

    /// The FFmpeg filter that decodes a clip into frames for this framing at `frame_size`
    /// pixels and `fps` frames per second.
    ///
    /// `CenterCrop` returns square frames. `Letterbox` returns frames that fit inside the square
    /// with the clip's aspect ratio (a 1280×720 clip at 512 becomes 512×288); the square is
    /// completed by [`frame_to_square`]. `ReframedLetterbox` crops the clip's centre
    /// ([`reframed_side`] of its width, all of its height) before it scales like `Letterbox`
    /// (a 1280×720 clip at 700 per mille is cropped to 896×720 and becomes 512×411).
    #[must_use]
    pub fn ffmpeg_filter(self, frame_size: u32, fps: u32) -> String {
        match self {
            Self::CenterCrop => {
                format!("crop=min(iw\\,ih):min(iw\\,ih),scale={frame_size}:{frame_size},fps={fps}")
            }
            Self::Letterbox { .. } | Self::ReframedLetterbox { .. } => format!(
                "{}scale={frame_size}:{frame_size}:force_original_aspect_ratio=decrease,fps={fps}",
                self.reframe_crop()
            ),
        }
    }

    /// The FFmpeg filter that decodes a clip at [`SUPERSAMPLE`] times `frame_size` for the seam
    /// repair's scale lock (plan `docs/plans/2026-11-04-veo-seamless-loops.md`, D-5), at `fps`
    /// frames per second.
    ///
    /// The frames have the shape [`BakeFraming::ffmpeg_filter`] gives, at twice the size: a
    /// `2·frame_size` square for `CenterCrop`, or for `Letterbox` the whole frame fitted inside
    /// it with both sides even, so a 2:1 box filter (`loop_metrics::downsample_half`) brings
    /// them to the frame size exactly.
    #[must_use]
    pub fn ffmpeg_filter_supersampled(self, frame_size: u32, fps: u32) -> String {
        let side = frame_size.saturating_mul(SUPERSAMPLE);
        match self {
            Self::CenterCrop => {
                format!("crop=min(iw\\,ih):min(iw\\,ih),scale={side}:{side},fps={fps}")
            }
            Self::Letterbox { .. } | Self::ReframedLetterbox { .. } => format!(
                "{}scale={side}:{side}:force_original_aspect_ratio=decrease:force_divisible_by={SUPERSAMPLE},fps={fps}",
                self.reframe_crop()
            ),
        }
    }

    /// Number of content rows (or columns) next to a bar that the feather blends, for a frame
    /// side of `frame_size` pixels. Always 0 for `CenterCrop`.
    #[must_use]
    pub fn feather_pixels(self, frame_size: u32) -> u32 {
        match self {
            Self::CenterCrop => 0,
            Self::Letterbox { feather_permille }
            | Self::ReframedLetterbox {
                feather_permille, ..
            } => {
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
            Self::ReframedLetterbox {
                feather_permille,
                scale_permille,
            } => write!(
                f,
                "letterbox of a {scale_permille} per mille reframe (feather {feather_permille} per mille)"
            ),
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
            if let BakeFraming::ReframedLetterbox { .. } = framing {
                return Err(BakerError::DataFile {
                    path: path.to_path_buf(),
                    reason: format!(
                        "framing: {framing} belongs to a mode's `reframe`, not to an identity"
                    ),
                });
            }
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

    /// `pixel` blended toward the bar colour with [`blend_toward`].
    fn blend(&self, pixel: Rgba<u8>, weight: u32) -> Rgba<u8> {
        blend_toward(pixel, self.bar_rgb, self.feather, weight)
    }
}

/// `pixel` moved `weight / (feather + 1)` of the way toward `bar_rgb`, rounded to the nearest
/// integer, opaque. `weight` runs from `feather` at the outermost row down to 1.
fn blend_toward(pixel: Rgba<u8>, bar_rgb: [u8; 3], feather: u32, weight: u32) -> Rgba<u8> {
    let denominator = feather + 1;
    let mix = |own: u8, bar: u8| -> u8 {
        let value =
            (u32::from(own) * (denominator - weight) + u32::from(bar) * weight + denominator / 2)
                / denominator;
        u8::try_from(value).unwrap_or(u8::MAX)
    };
    Rgba([
        mix(pixel[0], bar_rgb[0]),
        mix(pixel[1], bar_rgb[1]),
        mix(pixel[2], bar_rgb[2]),
        255,
    ])
}

/// A start picture reframed for a mode's clips ([`reframe_picture`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReframedPicture {
    /// The reframed picture, the size of its source.
    pub image: RgbaImage,
    /// The colour of its margin as sRGB bytes: the bar colour a letterbox bake of the source
    /// would measure ([`measure_bar_rgb`] over its top and bottom bands), so the margin that a
    /// [`BakeFraming::ReframedLetterbox`] bake keeps above and below the subject looks like the
    /// default atlas's bars.
    pub margin_rgb: [u8; 3],
}

/// Side, in pixels of the shrunk picture, of the cells over which [`reframe_picture`] averages
/// the backdrop it extends into the margin: wide enough to blur away detail (a smoke plume's
/// curls), narrow enough to follow the backdrop's gradient (a lit floor).
pub const REFRAME_BACKDROP_CELL: u32 = 40;

/// Reframes `source` for clips that need room around the subject: the picture scaled to
/// `scale_permille` about its centre ([`reframed_side`] of each side, Lanczos filter) on a
/// canvas of `source`'s size. The margin continues the picture's own backdrop: next to the
/// shrunk picture it takes the colour of the picture's nearest edge, averaged over cells of
/// [`REFRAME_BACKDROP_CELL`] pixels, and it fades linearly to the measured border colour
/// ([`ReframedPicture::margin_rgb`]) at the canvas's edge. The shrunk picture's outermost rows and
/// columns, `feather_permille` of the source's height, blend linearly into that averaged
/// backdrop, so the margin has no edge, line or panel for a video model to keep or animate.
///
/// The inverse is the crop of [`BakeFraming::ReframedLetterbox`] at the same scale: a clip that
/// starts on the result and is baked with it starts on the source's default framing, with the
/// margin above and below kept where the default bake draws its bars.
///
/// Deterministic: the same source and parameters give the same bytes.
///
/// # Errors
/// [`BakerError::ValidationFailed`] for a scale outside [`MIN_REFRAME_PERMILLE`] to
/// [`MAX_REFRAME_PERMILLE`], a feather wider than [`MAX_FEATHER_PERMILLE`], or a source too
/// small to keep a pixel.
pub fn reframe_picture(
    source: &RgbaImage,
    scale_permille: u32,
    feather_permille: u32,
) -> Result<ReframedPicture, BakerError> {
    let invalid = |reason: String| BakerError::ValidationFailed {
        name: "reframed picture".into(),
        reason,
    };
    if !(MIN_REFRAME_PERMILLE..=MAX_REFRAME_PERMILLE).contains(&scale_permille) {
        return Err(invalid(format!(
            "scale {scale_permille} per mille is outside {MIN_REFRAME_PERMILLE} to {MAX_REFRAME_PERMILLE}"
        )));
    }
    if feather_permille > MAX_FEATHER_PERMILLE {
        return Err(invalid(format!(
            "feather {feather_permille} per mille is wider than {MAX_FEATHER_PERMILLE}"
        )));
    }
    let (width, height) = source.dimensions();
    let inner_size = (
        reframed_side(width, scale_permille),
        reframed_side(height, scale_permille),
    );
    if inner_size.0 == 0 || inner_size.1 == 0 || inner_size == (width, height) {
        return Err(invalid(format!(
            "a {width}x{height} source keeps no pixel, or no margin, at {scale_permille} per mille"
        )));
    }
    let Some(margin_rgb) = measure_bar_rgb(std::slice::from_ref(source), Bars::TopBottom)? else {
        return Err(invalid("the source has no border to measure".into()));
    };
    let filter = image::imageops::FilterType::Lanczos3;
    let inner = image::imageops::resize(source, inner_size.0, inner_size.1, filter);
    let backdrop = averaged_backdrop(&inner);
    let feather = u32::try_from(u64::from(height) * u64::from(feather_permille) / 1000)
        .unwrap_or(u32::MAX)
        .min(inner_size.0 / 2)
        .min(inner_size.1 / 2);
    let layout = ReframeLayout {
        canvas: (width, height),
        inner: inner_size,
        margin_rgb,
    };
    let mut image = RgbaImage::new(width, height);
    for (x, y, out) in image.enumerate_pixels_mut() {
        *out = match layout.inner_point(x, y) {
            Some((ix, iy)) => {
                let edge = ix
                    .min(iy)
                    .min(inner_size.0 - 1 - ix)
                    .min(inner_size.1 - 1 - iy);
                let own = *inner.get_pixel(ix, iy);
                if edge < feather {
                    let [r, g, b, _] = backdrop.get_pixel(ix, iy).0;
                    blend_toward(own, [r, g, b], feather, feather - edge)
                } else {
                    Rgba([own[0], own[1], own[2], 255])
                }
            }
            None => layout.margin_pixel(x, y, &backdrop),
        };
    }
    Ok(ReframedPicture { image, margin_rgb })
}

/// `inner` averaged over cells of [`REFRAME_BACKDROP_CELL`] pixels and smoothly enlarged back to
/// its own size (two triangle-filter resizes): the low-frequency backdrop.
fn averaged_backdrop(inner: &RgbaImage) -> RgbaImage {
    let (width, height) = inner.dimensions();
    let filter = image::imageops::FilterType::Triangle;
    let cells = image::imageops::resize(
        inner,
        (width / REFRAME_BACKDROP_CELL).max(1),
        (height / REFRAME_BACKDROP_CELL).max(1),
        filter,
    );
    image::imageops::resize(&cells, width, height, filter)
}

/// Where the shrunk picture lies on the reframed canvas.
struct ReframeLayout {
    /// Canvas size, in pixels.
    canvas: (u32, u32),
    /// Size of the shrunk picture, centred on the canvas, in pixels.
    inner: (u32, u32),
    /// The colour the margin fades to at the canvas's edge.
    margin_rgb: [u8; 3],
}

impl ReframeLayout {
    /// Left and top margins, in pixels.
    fn offset(&self) -> (u32, u32) {
        (
            (self.canvas.0 - self.inner.0) / 2,
            (self.canvas.1 - self.inner.1) / 2,
        )
    }

    /// The point of the shrunk picture under canvas pixel `(x, y)`, or `None` in the margin.
    fn inner_point(&self, x: u32, y: u32) -> Option<(u32, u32)> {
        let (left, top) = self.offset();
        let ix = x.checked_sub(left).filter(|ix| *ix < self.inner.0)?;
        let iy = y.checked_sub(top).filter(|iy| *iy < self.inner.1)?;
        Some((ix, iy))
    }

    /// The margin's colour at canvas pixel `(x, y)`: the averaged backdrop at the nearest point
    /// of the shrunk picture, moved toward `margin_rgb` by the share of the margin crossed.
    fn margin_pixel(&self, x: u32, y: u32, backdrop: &RgbaImage) -> Rgba<u8> {
        let (left, top) = self.offset();
        // Distance past the shrunk picture along one axis, in thousandths of that axis's margin.
        let share = |at: u32, start: u32, len: u32, margin: u32| -> u32 {
            let past = if at < start {
                start - at
            } else {
                at.saturating_sub(start + len - 1)
            };
            (past * 1000 / margin.max(1)).min(1000)
        };
        let weight = share(x, left, self.inner.0, left).max(share(y, top, self.inner.1, top));
        let nearest = (
            x.clamp(left, left + self.inner.0 - 1) - left,
            y.clamp(top, top + self.inner.1 - 1) - top,
        );
        let edge = backdrop.get_pixel(nearest.0, nearest.1);
        let mix = |own: u8, far: u8| -> u8 {
            let value = (u32::from(own) * (1000 - weight) + u32::from(far) * weight + 500) / 1000;
            u8::try_from(value).unwrap_or(u8::MAX)
        };
        Rgba([
            mix(edge[0], self.margin_rgb[0]),
            mix(edge[1], self.margin_rgb[1]),
            mix(edge[2], self.margin_rgb[2]),
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
    fn the_supersampled_filter_decodes_at_twice_the_size() {
        assert_eq!(
            BakeFraming::CenterCrop.ffmpeg_filter_supersampled(512, 15),
            "crop=min(iw\\,ih):min(iw\\,ih),scale=1024:1024,fps=15"
        );
        assert_eq!(
            BakeFraming::Letterbox {
                feather_permille: 30
            }
            .ffmpeg_filter_supersampled(512, 15),
            "scale=1024:1024:force_original_aspect_ratio=decrease:force_divisible_by=2,fps=15"
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

    const REFRAMED: BakeFraming = BakeFraming::ReframedLetterbox {
        feather_permille: 30,
        scale_permille: 700,
    };

    #[test]
    fn a_reframed_letterbox_crops_the_centre_width_before_it_scales() {
        assert_eq!(
            REFRAMED.ffmpeg_filter(512, 15),
            "crop=trunc(iw*700/2000)*2:ih,scale=512:512:force_original_aspect_ratio=decrease,fps=15"
        );
        assert_eq!(
            REFRAMED.ffmpeg_filter_supersampled(512, 15),
            "crop=trunc(iw*700/2000)*2:ih,scale=1024:1024:force_original_aspect_ratio=decrease:force_divisible_by=2,fps=15"
        );
        assert_eq!(REFRAMED.feather_pixels(512), 15);
        assert_eq!(
            REFRAMED.to_string(),
            "letterbox of a 700 per mille reframe (feather 30 per mille)"
        );
    }

    #[test]
    fn the_reframed_side_is_even_and_rounded_down() {
        assert_eq!(reframed_side(1280, 700), 896);
        assert_eq!(reframed_side(720, 700), 504);
        assert_eq!(reframed_side(101, 500), 50);
        assert_eq!(reframed_side(1280, 999), 1278);
    }

    #[test]
    fn only_a_letterbox_can_be_reframed() {
        let letterbox = BakeFraming::Letterbox {
            feather_permille: 30,
        };
        assert_eq!(letterbox.reframed(700), Some(REFRAMED));
        assert_eq!(BakeFraming::CenterCrop.reframed(700), None);
        assert_eq!(REFRAMED.reframed(700), None);
    }

    /// A 400×200 backdrop with a 40×20 red box in the middle.
    fn boxed() -> RgbaImage {
        let mut image = wide(400, 200);
        for y in 90..110 {
            for x in 180..220 {
                image.put_pixel(x, y, Rgba([220, 30, 30, 255]));
            }
        }
        image
    }

    #[test]
    fn a_reframed_picture_shrinks_about_its_centre_into_its_own_backdrop() {
        let source = boxed();
        let reframed = reframe_picture(&source, 600, 50).unwrap();
        assert_eq!(reframed.image.dimensions(), (400, 200));
        assert_eq!(reframed.margin_rgb, [44, 48, 74]);
        // 600 per mille keeps 240×120 at (80, 40); the box becomes 24×12 at (188, 94).
        assert_eq!(reframed.image.get_pixel(200, 100).0, [220, 30, 30, 255]);
        let near_backdrop = |x: u32, y: u32| {
            let pixel = reframed.image.get_pixel(x, y).0;
            pixel[..3]
                .iter()
                .zip(BACKDROP)
                .all(|(own, want)| own.abs_diff(want) <= 6)
        };
        // Clear of the box's filter ringing, the shrunk backdrop is the backdrop.
        for (x, y) in [(183, 100), (217, 100), (200, 88), (200, 112)] {
            assert!(near_backdrop(x, y), "({x}, {y})");
        }
        // A plain backdrop extends into a margin of nearly the same colour, with no visible
        // edge; the averaged backdrop holds a trace of the box, which fades out toward the
        // canvas's edge, where the margin is exactly the measured colour.
        for (x, y) in [(79, 100), (320, 100), (200, 39), (200, 160)] {
            assert!(near_backdrop(x, y), "({x}, {y})");
        }
        for (x, y) in [(0, 0), (399, 199), (0, 100), (200, 0)] {
            assert_eq!(reframed.image.get_pixel(x, y).0, BACKDROP, "({x}, {y})");
        }
    }

    #[test]
    fn the_margin_continues_the_edge_and_fades_to_the_border_colour() {
        // A backdrop lit from below: the bottom rows are brighter than the top ones.
        let mut source = wide(200, 100);
        for y in 80..100 {
            for x in 0..200 {
                source.put_pixel(x, y, Rgba([90, 94, 120, 255]));
            }
        }
        let reframed = reframe_picture(&source, 600, 0).unwrap();
        let below = |y: u32| reframed.image.get_pixel(100, y)[0];
        // Just below the shrunk picture the margin is near the lit floor; at the canvas's
        // bottom edge it is the measured border colour, the lower median of both bands.
        assert!(below(80) > below(99), "{} then {}", below(80), below(99));
        assert_eq!(
            reframed.image.get_pixel(100, 99).0[..3],
            reframed.margin_rgb
        );
        assert_eq!(reframed.margin_rgb, [44, 48, 74]);
    }

    #[test]
    fn a_reframe_is_deterministic() {
        let source = boxed();
        assert_eq!(
            reframe_picture(&source, 700, 60).unwrap(),
            reframe_picture(&source, 700, 60).unwrap()
        );
    }

    #[test]
    fn a_reframe_out_of_range_is_refused() {
        let source = boxed();
        for (scale, feather) in [(499, 0), (1000, 0), (700, MAX_FEATHER_PERMILLE + 1)] {
            assert!(
                matches!(
                    reframe_picture(&source, scale, feather),
                    Err(BakerError::ValidationFailed { .. })
                ),
                "{scale} {feather}"
            );
        }
        assert!(reframe_picture(&wide(2, 2), 999, 0).is_err(), "no margin");
    }
}
