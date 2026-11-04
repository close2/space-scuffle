//! Synthetic zoom clips for the seam repair tests (plan
//! `docs/plans/2026-11-04-veo-seamless-loops.md`, WP2).
//!
//! # What it makes
//! [`pattern_frame`] renders one frame of a fixed, smooth, textured scene, magnified about the
//! frame centre and shifted by a known amount. The scene is computed analytically for every
//! pixel, so a frame at magnification 1.06 is the exact picture a camera 6 % closer would see:
//! no resampling error is baked into the fixture. [`encode_clip`] writes such frames to a
//! lossless FFV1 clip with the real FFmpeg, so the bake decodes it through its normal path.
//!
//! # Coordinates
//! A frame pixel `p` shows the scene point `(p - c - offset) / magnification`, where `c` is the
//! frame centre in pixel-centre coordinates. So the point frame 0 shows at `p0` appears in a
//! frame of magnification `m` and offset `d` at `c + m (p0 - c) + d`, the convention of
//! `asset_baker::loop_metrics::ZoomEstimate`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use image::{Rgba, RgbaImage};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Playback rate of the synthetic clips, equal to the bake rate the tests use, so FFmpeg's
/// `fps` filter keeps every frame.
pub const CLIP_FPS: u32 = 15;

/// The FFmpeg executable the baker runs.
const FFMPEG: &str = "/usr/bin/ffmpeg";

/// The scene's colour at scene point `(u, v)`, in pixels at magnification 1 from the centre.
fn scene(u: f64, v: f64, side: f64) -> [u8; 3] {
    let envelope = (-(u * u + v * v) / (2.0 * (0.28 * side).powi(2))).exp();
    let texture_a = (0.19 * u + 0.05 * v).sin() * (0.08 * u - 0.17 * v + 1.3).cos();
    let texture_b = (0.11 * u + 0.23 * v + 0.4).sin();
    let backdrop = 0.25 * (0.06 * u - 0.04 * v).sin();
    let channel = |base: f64, a: f64, b: f64| -> u8 {
        let value = base + 30.0 * backdrop + envelope * (a * texture_a + b * texture_b);
        value.round().clamp(0.0, 255.0) as u8
    };
    [
        channel(60.0 + 120.0 * envelope, 70.0, 30.0),
        channel(70.0 + 80.0 * envelope, 40.0, 60.0),
        channel(110.0 + 20.0 * envelope, 25.0, -45.0),
    ]
}

/// One `width`×`height` frame of the scene at `magnification` with its centre shifted by
/// `offset` pixels.
#[must_use]
pub fn pattern_frame(width: u32, height: u32, magnification: f64, offset: [f64; 2]) -> RgbaImage {
    let cx = f64::from(width - 1) / 2.0;
    let cy = f64::from(height - 1) / 2.0;
    let side = f64::from(width.min(height));
    RgbaImage::from_fn(width, height, |x, y| {
        let scene_x = (f64::from(x) - cx - offset[0]) / magnification;
        let scene_y = (f64::from(y) - cy - offset[1]) / magnification;
        let [red, green, blue] = scene(scene_x, scene_y, side);
        Rgba([red, green, blue, 255])
    })
}

/// Magnifications of a steady push-in of `total_growth` over `frames` frames: frame `k` is
/// `(1 + total_growth)^(k / (frames - 1))`.
#[must_use]
pub fn steady_ramp(frames: usize, total_growth: f64) -> Vec<f64> {
    let last = (frames.max(2) - 1) as f64;
    (0..frames)
        .map(|k| (1.0 + total_growth).powf(k as f64 / last))
        .collect()
}

/// Writes `frames` as a lossless FFV1 clip `dir/name` at [`CLIP_FPS`] and returns its path.
///
/// # Panics
/// Panics if a frame cannot be written or FFmpeg fails.
#[must_use]
pub fn encode_clip(dir: &Path, name: &str, frames: &[RgbaImage]) -> PathBuf {
    let scratch = tempfile::tempdir().expect("frames directory");
    let frames_dir = scratch.path();
    for (index, frame) in frames.iter().enumerate() {
        frame
            .save(frames_dir.join(format!("f_{index:04}.png")))
            .expect("frame png");
    }
    let output = dir.join(name);
    assert!(
        Path::new(FFMPEG).is_file(),
        "{FFMPEG} is missing: video tests cannot run and must not be skipped. Install ffmpeg."
    );
    let status = Command::new(FFMPEG)
        .args(["-y", "-loglevel", "error", "-framerate"])
        .arg(CLIP_FPS.to_string())
        .arg("-i")
        .arg(frames_dir.join("f_%04d.png"))
        .args(["-c:v", "ffv1", "-pix_fmt", "bgr0"])
        .arg(&output)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg could not encode {name}");
    output
}

/// A clip of `side`×`side` frames, one per magnification, centred.
#[must_use]
pub fn zoom_clip(dir: &Path, name: &str, side: u32, magnifications: &[f64]) -> PathBuf {
    let frames: Vec<RgbaImage> = magnifications
        .iter()
        .map(|m| pattern_frame(side, side, *m, [0.0, 0.0]))
        .collect();
    encode_clip(dir, name, &frames)
}
