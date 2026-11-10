//! Choosing a clip's effect span and the frames a flipbook keeps (plan
//! `2026-11-09-effect-visualization.md` §6.4, "Flipbooks").
//!
//! # Responsibility
//! A Veo clip of an effect element starts black, shows the element and ends black (the
//! `effect_element.clip` prompt text asks for this). [`frame_activity`] measures how much
//! of each decoded frame is lit; [`select_span`] finds the effect span, from the onset (the
//! first frame whose activity reaches [`ACTIVITY_THRESHOLD_PERMILLE`] of the peak) to the
//! dissipation (the last such frame), and spreads the frames the flipbook keeps evenly over
//! it, both ends kept.
//!
//! # How it fits
//! [`crate::effects::bake`] decodes a clip with the existing FFmpeg importer
//! ([`crate::ffmpeg_importer::decode_framed_clip`]), calls [`select_span`], and keys the kept
//! frames; the [`SpanRecord`] goes into the element's provenance.
//!
//! # Invariants
//! - The kept frames are in temporal order and inside the span; a span shorter than the
//!   wanted count keeps every frame of it.
//! - The play time is the span's real length at the decode rate, so the flipbook plays at the
//!   speed the clip was filmed at however few frames it keeps.

use super::keying::BLACK_LEVEL;
use super::EffectError;
use crate::ffmpeg_importer::evenly_spaced_indices;
use image::RgbaImage;
use serde::{Deserialize, Serialize};

/// Share of the peak activity that starts and ends the effect span, in thousandths.
pub const ACTIVITY_THRESHOLD_PERMILLE: u64 = 20;

/// Which frames of a clip a flipbook element kept, recorded in provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanRecord {
    /// Frames the clip decoded to at `decode_fps`.
    pub decoded_frames: u32,
    /// Decode rate, frames per second.
    pub decode_fps: u32,
    /// First decoded frame of the effect span.
    pub onset: u32,
    /// Last decoded frame of the effect span.
    pub dissipation: u32,
    /// Decoded frame index of every kept frame, in order.
    pub kept: Vec<u32>,
    /// Real play time of the span, in milliseconds (at least 1).
    pub duration_ms: u32,
    /// The recipe's cut: frames before `from_ms` milliseconds were not used, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_ms: Option<u32>,
    /// The recipe's cut: only the clip's first `until_ms` milliseconds were used, if given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until_ms: Option<u32>,
    /// The recipe's framing, when it is not the centre crop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framing: Option<crate::framing::BakeFraming>,
}

impl SpanRecord {
    /// Moves a span chosen over a clip's frames from index `skipped` on back onto the whole
    /// clip's frame indices, of which `decoded_frames` were decoded.
    pub fn offset(&mut self, skipped: usize, decoded_frames: usize) {
        let skipped = to_u32(skipped);
        self.decoded_frames = to_u32(decoded_frames);
        self.onset = self.onset.saturating_add(skipped);
        self.dissipation = self.dissipation.saturating_add(skipped);
        for index in &mut self.kept {
            *index = index.saturating_add(skipped);
        }
    }
}

/// The activity of a frame: the sum over its pixels of how far each channel maximum rises
/// above [`BLACK_LEVEL`].
#[must_use]
pub fn frame_activity(frame: &RgbaImage) -> u64 {
    frame
        .pixels()
        .map(|p| u64::from(p[0].max(p[1]).max(p[2]).saturating_sub(BLACK_LEVEL)))
        .sum()
}

/// The effect span of `frames` and the `wanted` frames kept over it.
///
/// # Errors
/// [`EffectError::EmptyElement`] when no frame is lit, and [`EffectError::InvalidRecipe`]
/// (naming `element`) for a zero `wanted` or `decode_fps`.
pub fn select_span(
    frames: &[RgbaImage],
    wanted: u32,
    decode_fps: u32,
    element: &str,
) -> Result<SpanRecord, EffectError> {
    if wanted == 0 || decode_fps == 0 {
        return Err(EffectError::InvalidRecipe {
            path: element.into(),
            reason: "a flipbook needs at least one frame and a decode rate above 0".to_string(),
        });
    }
    let activity: Vec<u64> = frames.iter().map(frame_activity).collect();
    let peak = activity.iter().copied().max().unwrap_or(0);
    if peak == 0 {
        return Err(EffectError::EmptyElement {
            element: element.to_string(),
            reason: "no frame of the clip is brighter than black".to_string(),
        });
    }
    let active = |a: &u64| *a * 1000 >= peak * ACTIVITY_THRESHOLD_PERMILLE;
    let onset = activity.iter().position(active).unwrap_or(0);
    let dissipation = activity.iter().rposition(active).unwrap_or(onset);
    let span = dissipation - onset + 1;
    let kept = evenly_spaced_indices(span, wanted as usize)
        .into_iter()
        .map(|index| to_u32(onset + index))
        .collect();
    let duration_ms = (u64::from(to_u32(span)) * 1000)
        .div_ceil(u64::from(decode_fps))
        .max(1);
    Ok(SpanRecord {
        decoded_frames: to_u32(frames.len()),
        decode_fps,
        onset: to_u32(onset),
        dissipation: to_u32(dissipation),
        kept,
        duration_ms: u32::try_from(duration_ms).unwrap_or(u32::MAX),
        from_ms: None,
        until_ms: None,
        framing: None,
    })
}

/// A frame count or index as `u32`, saturating (a clip never decodes to 2^32 frames).
fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use image::Rgba;

    fn frame(level: u8) -> RgbaImage {
        let mut image = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255]));
        image.put_pixel(4, 4, Rgba([level, level, level, 255]));
        image
    }

    #[test]
    fn the_span_runs_from_onset_to_dissipation() {
        let levels = [0, 0, 2, 50, 200, 250, 120, 40, 3, 0, 0];
        let frames: Vec<RgbaImage> = levels.iter().map(|l| frame(*l)).collect();
        let span = select_span(&frames, 3, 10, "e").expect("lit");
        assert_eq!((span.onset, span.dissipation), (3, 7));
        assert_eq!(span.kept, vec![3, 5, 7]);
        assert_eq!(span.duration_ms, 500);
        assert_eq!(span.decoded_frames, 11);
    }

    #[test]
    fn an_offset_span_names_whole_clip_frames() {
        let levels = [0, 50, 200, 40, 0];
        let frames: Vec<RgbaImage> = levels.iter().map(|l| frame(*l)).collect();
        let mut span = select_span(&frames, 3, 10, "e").expect("lit");
        span.offset(4, 9);
        assert_eq!((span.onset, span.dissipation), (5, 7));
        assert_eq!(span.kept, vec![5, 6, 7]);
        assert_eq!(span.decoded_frames, 9);
    }

    #[test]
    fn a_short_span_keeps_every_frame_and_a_black_clip_is_empty() {
        let frames: Vec<RgbaImage> = [0, 100, 100, 0].iter().map(|l| frame(*l)).collect();
        assert_eq!(
            select_span(&frames, 8, 10, "e").expect("lit").kept,
            vec![1, 2]
        );
        let black: Vec<RgbaImage> = (0..4).map(|_| frame(0)).collect();
        assert!(matches!(
            select_span(&black, 4, 10, "e"),
            Err(EffectError::EmptyElement { .. })
        ));
    }
}
