//! Assembling the published files from the takes: cut lists, ffmpeg arguments and the size ladder.
//!
//! # Responsibility
//! Plan `docs/plans/2026-11-05-ci-showcase-video.md`, decision D-9. From the storyboard and the take
//! manifests this module computes, as pure data, which frames each output holds ([`CutPlan`]) and
//! the exact `ffmpeg` arguments that write them; [`run_ffmpeg`] runs one encode; [`ladder`] tries
//! the teaser's width and quality steps in order and keeps the first under budget.
//!
//! # Outputs
//! - `showcase.mp4`: every take concatenated, H.264 High, `yuv420p`, BT.709, 25 fps, `+faststart`.
//! - `showcase.webp`: the teaser cut, every `frame_step`-th frame, animated WebP that loops.
//! - `showcase-poster.png`: the poster frame, lossless.
//!
//! # Invariants
//! - ffmpeg only cuts, scales and encodes the engine's frames; it draws nothing.
//! - Frame selection is by frame number on the lossless mezzanines, never by timestamp.
//! - Over budget is an error listing every size tried; nothing is published from it.

use super::recorder::TakeManifest;
use super::storyboard::Storyboard;
use super::ShowcaseError;
use std::path::{Path, PathBuf};
use std::process::Command;

/// File name of the full video.
pub const VIDEO_FILE: &str = "showcase.mp4";

/// File name of the animated teaser, which the README embeds.
pub const TEASER_FILE: &str = "showcase.webp";

/// File name of the poster still.
pub const POSTER_FILE: &str = "showcase-poster.png";

/// File name of the provenance record.
pub const PROVENANCE_FILE: &str = "showcase.json";

/// File name of the player page.
pub const PAGE_FILE: &str = "index.html";

/// One take as the encoders see it: its mezzanine and its manifest.
#[derive(Debug, Clone)]
pub struct TakeInput {
    /// The FFV1 mezzanine.
    pub mezzanine: PathBuf,
    /// Its manifest.
    pub manifest: TakeManifest,
}

/// A run of frames in the concatenation of all takes, `start..start + frames`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// First frame, counted over all takes in order.
    pub start: u64,
    /// Frames in the span.
    pub frames: u64,
}

/// Which frames every output holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutPlan {
    /// Frames of all takes together.
    pub total_frames: u64,
    /// The teaser's spans, in increasing order and disjoint.
    pub teaser: Vec<Span>,
    /// The poster frame.
    pub poster: u64,
}

impl CutPlan {
    /// Computes the cut from the storyboard and the takes' manifests.
    ///
    /// # Errors
    /// [`ShowcaseError::Encode`] when a scene is missing from the manifests, a cut lies outside its
    /// rendered scene, or the teaser's spans are out of order or overlap.
    pub fn new(storyboard: &Storyboard, takes: &[TakeInput]) -> Result<Self, ShowcaseError> {
        let locate = |scene: &str| -> Result<(u64, u64), ShowcaseError> {
            let mut offset = 0;
            for take in takes {
                if let Some(record) = take.manifest.scene(scene) {
                    return Ok((offset + record.first_frame, record.frames));
                }
                offset += take.manifest.frames.len() as u64;
            }
            Err(encode_error(
                "cut",
                format!("scene `{scene}` is in no take manifest"),
            ))
        };
        let mut teaser = Vec::with_capacity(storyboard.teaser.cuts.len());
        for cut in &storyboard.teaser.cuts {
            let (first, frames) = locate(&cut.scene)?;
            let length = u64::from(cut.frames);
            let skip = u64::from(cut.first);
            if skip + length > frames {
                return Err(encode_error(
                    "cut",
                    format!(
                        "the cut of `{}` needs {} frames, the scene has {frames}",
                        cut.scene,
                        skip + length
                    ),
                ));
            }
            let start = if cut.from_end {
                first + frames - skip - length
            } else {
                first + skip
            };
            if teaser
                .last()
                .is_some_and(|last: &Span| last.start + last.frames > start)
            {
                return Err(encode_error(
                    "cut",
                    format!(
                        "the cut of `{}` overlaps or precedes the previous cut",
                        cut.scene
                    ),
                ));
            }
            teaser.push(Span {
                start,
                frames: length,
            });
        }
        let (first, frames) = locate(&storyboard.poster.scene)?;
        if u64::from(storyboard.poster.frame) >= frames {
            return Err(encode_error(
                "poster",
                format!(
                    "scene `{}` has only {frames} frames",
                    storyboard.poster.scene
                ),
            ));
        }
        Ok(Self {
            total_frames: takes
                .iter()
                .map(|take| take.manifest.frames.len() as u64)
                .sum(),
            teaser,
            poster: first + u64::from(storyboard.poster.frame),
        })
    }

    /// Frames of the teaser after keeping every `step`-th frame of its cut.
    #[must_use]
    pub fn teaser_frames(&self, step: u32) -> u64 {
        let cut: u64 = self.teaser.iter().map(|span| span.frames).sum();
        cut.div_ceil(u64::from(step.max(1)))
    }
}

fn encode_error(step: &str, reason: String) -> ShowcaseError {
    ShowcaseError::Encode {
        step: step.to_owned(),
        reason,
    }
}

/// `-i` for every mezzanine, then a filter that concatenates them into `[all]`.
fn inputs_and_concat(takes: &[TakeInput]) -> (Vec<String>, String) {
    let mut arguments = vec!["-hide_banner".into(), "-loglevel".into(), "error".into()];
    for take in takes {
        arguments.push("-i".into());
        arguments.push(take.mezzanine.to_string_lossy().into_owned());
    }
    let labels = (0..takes.len())
        .map(|index| format!("[{index}:v]"))
        .collect::<Vec<_>>()
        .concat();
    let concat = format!("{labels}concat=n={}:v=1:a=0[all]", takes.len());
    (arguments, concat)
}

/// Arguments of the MP4 encode.
#[must_use]
pub fn video_arguments(storyboard: &Storyboard, takes: &[TakeInput], output: &Path) -> Vec<String> {
    let (mut arguments, concat) = inputs_and_concat(takes);
    let rate = storyboard.video.max_kbit_per_second;
    arguments.extend([
        "-filter_complex".into(),
        format!("{concat};[all]scale=out_color_matrix=bt709:out_range=tv,format=yuv420p[v]"),
        "-map".into(),
        "[v]".into(),
        "-c:v".into(),
        "libx264".into(),
        "-profile:v".into(),
        "high".into(),
        "-preset".into(),
        "slow".into(),
        "-crf".into(),
        storyboard.video.crf.to_string(),
        "-maxrate".into(),
        format!("{rate}k"),
        "-bufsize".into(),
        format!("{}k", rate * 2),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-colorspace".into(),
        "bt709".into(),
        "-color_primaries".into(),
        "bt709".into(),
        "-color_trc".into(),
        "bt709".into(),
        "-r".into(),
        storyboard.frame_rate.to_string(),
        "-movflags".into(),
        "+faststart".into(),
        "-an".into(),
        "-n".into(),
        output.to_string_lossy().into_owned(),
    ]);
    arguments
}

/// The `select` expression keeping the frames of `spans`.
fn select_expression(spans: &[Span]) -> String {
    spans
        .iter()
        .map(|span| {
            format!(
                "between(n\\,{}\\,{})",
                span.start,
                span.start + span.frames - 1
            )
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Arguments of one teaser encode at `width` and `quality`.
#[must_use]
pub fn teaser_arguments(
    storyboard: &Storyboard,
    takes: &[TakeInput],
    plan: &CutPlan,
    step: (u32, u32),
    output: &Path,
) -> Vec<String> {
    let (width, quality) = step;
    let (mut arguments, concat) = inputs_and_concat(takes);
    let frame_step = storyboard.teaser.frame_step.max(1);
    let teaser_rate = f64::from(storyboard.frame_rate) / f64::from(frame_step);
    arguments.extend([
        "-filter_complex".into(),
        format!(
            "{concat};[all]select='{}',setpts=N/({}*TB),framestep={frame_step},setpts=N/({teaser_rate}*TB),scale={width}:-2:flags=lanczos[v]",
            select_expression(&plan.teaser),
            storyboard.frame_rate,
        ),
        "-map".into(),
        "[v]".into(),
        "-c:v".into(),
        "libwebp_anim".into(),
        "-lossless".into(),
        "0".into(),
        "-quality".into(),
        quality.to_string(),
        "-compression_level".into(),
        "6".into(),
        "-loop".into(),
        "0".into(),
        "-an".into(),
        "-f".into(),
        "webp".into(),
        "-n".into(),
        output.to_string_lossy().into_owned(),
    ]);
    arguments
}

/// Arguments of the poster still.
#[must_use]
pub fn poster_arguments(takes: &[TakeInput], plan: &CutPlan, output: &Path) -> Vec<String> {
    let (mut arguments, concat) = inputs_and_concat(takes);
    arguments.extend([
        "-filter_complex".into(),
        format!("{concat};[all]select='eq(n\\,{})'[v]", plan.poster),
        "-map".into(),
        "[v]".into(),
        "-frames:v".into(),
        "1".into(),
        "-n".into(),
        output.to_string_lossy().into_owned(),
    ]);
    arguments
}

/// Runs `ffmpeg` with `arguments`.
///
/// # Errors
/// [`ShowcaseError::Encode`] naming `step` with ffmpeg's exit status and error output.
pub fn run_ffmpeg(step: &str, arguments: &[String]) -> Result<(), ShowcaseError> {
    let output = Command::new("ffmpeg")
        .args(arguments)
        .output()
        .map_err(|error| encode_error(step, format!("cannot run ffmpeg: {error}")))?;
    if !output.status.success() {
        return Err(encode_error(
            step,
            format!(
                "ffmpeg {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    Ok(())
}

/// One rung of the size ladder and the size it gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt {
    /// Width, pixels.
    pub width: u32,
    /// WebP quality.
    pub quality: u32,
    /// Bytes written.
    pub bytes: u64,
}

impl std::fmt::Display for Attempt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} px at quality {}: {} bytes",
            self.width, self.quality, self.bytes
        )
    }
}

/// Tries every width, and at each width every quality, in order; returns the first attempt within
/// `budget` bytes and every attempt made. `encode` writes one rung and returns its size.
///
/// # Errors
/// The first error of `encode`, or [`ShowcaseError::OverBudget`] listing every attempt in order.
pub fn ladder(
    output: &str,
    widths: &[u32],
    qualities: &[u32],
    budget: u64,
    mut encode: impl FnMut(u32, u32) -> Result<u64, ShowcaseError>,
) -> Result<(Attempt, Vec<Attempt>), ShowcaseError> {
    let mut attempts = Vec::new();
    for &width in widths {
        for &quality in qualities {
            let bytes = encode(width, quality)?;
            let attempt = Attempt {
                width,
                quality,
                bytes,
            };
            attempts.push(attempt);
            if bytes <= budget {
                return Ok((attempt, attempts));
            }
        }
    }
    Err(ShowcaseError::OverBudget {
        output: output.to_owned(),
        budget,
        attempts: attempts.iter().map(ToString::to_string).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_expression_lists_inclusive_ranges() {
        let spans = [
            Span {
                start: 0,
                frames: 3,
            },
            Span {
                start: 10,
                frames: 2,
            },
        ];
        assert_eq!(
            select_expression(&spans),
            "between(n\\,0\\,2)+between(n\\,10\\,11)"
        );
    }

    #[test]
    fn teaser_frames_round_up_per_step() {
        let plan = CutPlan {
            total_frames: 100,
            teaser: vec![
                Span {
                    start: 0,
                    frames: 75,
                },
                Span {
                    start: 80,
                    frames: 4,
                },
            ],
            poster: 3,
        };
        assert_eq!(plan.teaser_frames(2), 40);
        assert_eq!(plan.teaser_frames(1), 79);
    }
}
