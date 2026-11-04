//! Behavioural tests of `loop-check`, the measured loop-continuity review criterion (plan
//! `docs/plans/2026-11-04-veo-seamless-loops.md`, D-6, WP3).
//!
//! Every clip here is made at test time: `common::loop_clip` renders an analytic scene and the
//! real `FFmpeg` encodes it losslessly, so the check decodes, samples, repairs and measures it
//! through the bake's own path. Like a Veo Idle clip, the scene is still and one element moves:
//! a bright blob circles once per period, so a clip of `PERIOD + 1` frames whose last frame
//! equals its first is an exact loop. `FFmpeg`'s `testsrc2` (the plan's suggestion) prints a
//! running frame counter, so no cut of it is periodic.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

mod common;

use asset_baker::clip_review::{
    loop_check, LoopCheckRequest, LoopFailure, RepairOutcome, LOOP_MAX_GROWTH, LOOP_STEP_MARGIN,
    START_FIDELITY_MIN,
};
use asset_baker::ffmpeg_importer::FrameSpec;
use asset_baker::framing::BakeFraming;
use asset_baker::loop_metrics::SeamRepair;
use common::cli::{baker, identity_with_actions, shipped_catalog};
use common::loop_clip::{encode_clip, pattern_frame};
use image::RgbaImage;
use sim_types::enums::AnimState;
use std::path::{Path, PathBuf};

/// Frames of one loop period; the clip holds one more, equal to the first.
const PERIOD: usize = 16;

/// Size of the synthetic 16:9 source frames, in pixels.
const SOURCE: (u32, u32) = (512, 288);

/// Radius of the blob's orbit about the scene centre, in source pixels at magnification 1.
const ORBIT: f64 = 50.0;

/// Standard deviation of the blob's brightness profile, in the same units.
const BLOB_SIGMA: f64 = 10.0;

/// Brightness the blob adds at its centre, in sRGB byte units.
const BLOB_GAIN: f64 = 90.0;

/// The bake geometry the tests check: 256 px letterboxed squares at the clip's own rate.
const SPEC: FrameSpec = FrameSpec {
    size: 256,
    fps: common::loop_clip::CLIP_FPS,
    framing: BakeFraming::Letterbox {
        feather_permille: 30,
    },
};

/// The shipped repair of a looping state: scale lock, then a two-frame cross-fade.
const SHIPPED_SEAM: SeamRepair = SeamRepair {
    scale_lock: true,
    crossfade_frames: 2,
};

const FFMPEG: &str = "/usr/bin/ffmpeg";

/// The blob's scene position in frame `k`, on its orbit.
fn orbit(k: usize) -> [f64; 2] {
    let angle = std::f64::consts::TAU * k as f64 / PERIOD as f64;
    [ORBIT * angle.cos(), ORBIT * angle.sin()]
}

/// The still scene at `magnification`, its camera shifted by `offset` pixels, with the blob at
/// scene point `blob`. A pixel `p` shows scene point `(p - c - offset) / magnification`, as in
/// `common::loop_clip`.
fn scene_frame(magnification: f64, offset: [f64; 2], blob: [f64; 2]) -> RgbaImage {
    let mut frame = pattern_frame(SOURCE.0, SOURCE.1, magnification, offset);
    add_blob(
        &mut frame,
        (magnification, offset),
        blob,
        (BLOB_GAIN, BLOB_SIGMA),
    );
    frame
}

/// Brightens `frame`, seen through `camera` (magnification, offset), by a Gaussian blob at
/// scene point `centre` with `(gain, sigma)` in sRGB byte units and scene pixels.
fn add_blob(frame: &mut RgbaImage, camera: (f64, [f64; 2]), centre: [f64; 2], blob: (f64, f64)) {
    let (magnification, offset) = camera;
    let (gain, sigma) = blob;
    let cx = f64::from(frame.width() - 1) / 2.0;
    let cy = f64::from(frame.height() - 1) / 2.0;
    for (x, y, pixel) in frame.enumerate_pixels_mut() {
        let u = (f64::from(x) - cx - offset[0]) / magnification - centre[0];
        let v = (f64::from(y) - cy - offset[1]) / magnification - centre[1];
        let lift = gain * (-(u * u + v * v) / (2.0 * sigma * sigma)).exp();
        for channel in &mut pixel.0[..3] {
            *channel = (f64::from(*channel) + lift).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// `PERIOD + 1` frames of the still scene with the circling blob, frame `k` at magnification
/// `zoom(k)`.
fn loop_frames(zoom: impl Fn(usize) -> f64) -> Vec<RgbaImage> {
    (0..=PERIOD)
        .map(|k| scene_frame(zoom(k), [0.0, 0.0], orbit(k)))
        .collect()
}

/// [`loop_frames`] at magnification 1 with a hard cut two frames before the end: from frame
/// `PERIOD - 2` on, the camera jumps sideways.
fn cut_frames() -> Vec<RgbaImage> {
    let mut frames = loop_frames(|_| 1.0);
    for (k, frame) in frames.iter_mut().enumerate().skip(PERIOD - 2) {
        *frame = scene_frame(1.0, [60.0, -30.0], orbit(k));
    }
    frames
}

/// [`loop_frames`] at magnification 1 in which a new part grows steadily on the subject, as
/// the antennas of the rejected clip `a1eb317d` do. There is no camera zoom, but the zoom
/// estimator reads the growing part as about 1.5 % of push-in, which the scale lock then
/// "repairs" instead of refusing (the case WP2 reported): every step changes the picture a
/// little, the restart a lot.
fn morph_frames() -> Vec<RgbaImage> {
    let mut frames = loop_frames(|_| 1.0);
    for (k, frame) in frames.iter_mut().enumerate() {
        let grown = k as f64 / PERIOD as f64;
        add_blob(
            frame,
            (1.0, [0.0, 0.0]),
            [-110.0, -50.0],
            (150.0 * grown, 24.0),
        );
    }
    frames
}

fn request<'a>(
    clip: &'a Path,
    seam: Option<SeamRepair>,
    pin: Option<&'a Path>,
) -> LoopCheckRequest<'a> {
    LoopCheckRequest {
        clip,
        spec: SPEC,
        budget: PERIOD,
        seam,
        pin,
        label: "synthetic loop",
    }
}

fn check(
    clip: &Path,
    seam: Option<SeamRepair>,
    pin: Option<&Path>,
) -> asset_baker::clip_review::LoopCheck {
    let result = loop_check(Path::new(FFMPEG), &request(clip, seam, pin))
        .expect("loop-check measures the clip");
    // Shown by `cargo test -- --nocapture`, and with every failure.
    eprintln!("{}\n{}", clip.display(), result.render());
    result
}

fn synthetic_loop(dir: &Path) -> PathBuf {
    encode_clip(dir, "loop.mkv", &loop_frames(|_| 1.0))
}

fn zoom_ramp_loop(dir: &Path) -> PathBuf {
    encode_clip(
        dir,
        "zoom_ramp.mkv",
        &loop_frames(|k| 1.05_f64.powf(k as f64 / PERIOD as f64)),
    )
}

#[test]
fn the_thresholds_are_the_plans() {
    assert!((LOOP_STEP_MARGIN - 0.03).abs() < 1e-12);
    assert!((LOOP_MAX_GROWTH - 0.015).abs() < 1e-12);
    assert!((START_FIDELITY_MIN - 0.90).abs() < 1e-12);
}

#[test]
fn a_synthetic_loop_passes_raw_and_repaired() {
    let dir = tempfile::tempdir().expect("temp dir");
    let clip = synthetic_loop(dir.path());
    let result = check(&clip, Some(SHIPPED_SEAM), None);
    assert_eq!(result.raw.frames, PERIOD);
    assert!(
        result.raw.failures(None).is_empty(),
        "raw: {:?}",
        result.raw
    );
    // Every step of an exact loop, the wrap included, is the same move of the camera.
    assert!(
        (result.raw.wrap - result.raw.step).abs() < 0.02,
        "{:?}",
        result.raw
    );
    assert!(result.raw.growth.abs() < 0.005, "{:?}", result.raw);
    assert!(result.passes(), "{:?}", result.failures());
}

#[test]
fn a_zoom_ramp_fails_raw_and_passes_repaired() {
    let dir = tempfile::tempdir().expect("temp dir");
    let clip = zoom_ramp_loop(dir.path());
    let result = check(&clip, Some(SHIPPED_SEAM), None);
    let raw = result.raw.failures(None);
    assert!(
        raw.iter().any(|f| matches!(f, LoopFailure::Growth { .. })),
        "raw growth {:+.4} must fail: {raw:?}",
        result.raw.growth
    );
    assert!(result.raw.growth > 0.04, "{:?}", result.raw);
    let RepairOutcome::Measured { measures, seam } = &result.repaired else {
        panic!(
            "a steady 5 % zoom is repaired, not refused: {:?}",
            result.repaired
        );
    };
    let seam = seam.expect("a repaired state records its seam");
    assert!(
        (40..=55).contains(&seam.scale_permille),
        "scale {} per mille",
        seam.scale_permille
    );
    assert!(measures.growth.abs() <= LOOP_MAX_GROWTH, "{measures:?}");
    assert!(result.passes(), "{:?}", result.failures());
}

#[test]
fn a_hard_cut_two_frames_before_the_end_fails_on_worst_even_after_the_cross_fade() {
    let dir = tempfile::tempdir().expect("temp dir");
    let frames = cut_frames();
    let clip = encode_clip(dir.path(), "cut.mkv", &frames);
    let result = check(&clip, Some(SHIPPED_SEAM), None);
    let failures = result.failures();
    assert!(
        failures
            .iter()
            .any(|f| matches!(f, LoopFailure::Worst { .. })),
        "{failures:?}"
    );
    let RepairOutcome::Measured { measures, .. } = &result.repaired else {
        panic!(
            "a cut without zoom is not refused by the scale lock: {:?}",
            result.repaired
        );
    };
    assert!(
        measures.worst < measures.step - LOOP_STEP_MARGIN,
        "{measures:?}"
    );
    assert_ne!(
        measures.worst_at,
        PERIOD - 1,
        "the cross-fade moves the jump off the wrap, and `worst` still sees it"
    );
}

#[test]
fn a_morph_the_scale_lock_does_not_refuse_fails_on_worst_after_the_repair() {
    let dir = tempfile::tempdir().expect("temp dir");
    let clip = encode_clip(dir.path(), "morph.mkv", &morph_frames());
    let result = check(&clip, Some(SHIPPED_SEAM), None);
    let RepairOutcome::Measured { measures, .. } = &result.repaired else {
        panic!(
            "the bake repairs this morph rather than refusing it: {:?}",
            result.repaired
        );
    };
    assert!(measures.growth.abs() <= LOOP_MAX_GROWTH, "{measures:?}");
    assert!(
        result
            .failures()
            .iter()
            .any(|f| matches!(f, LoopFailure::Worst { .. })),
        "{}",
        result.render()
    );
}

#[test]
fn start_fidelity_compares_frame_zero_with_the_pin_framed_the_same_way() {
    let dir = tempfile::tempdir().expect("temp dir");
    let clip = synthetic_loop(dir.path());
    let pin = dir.path().join("pin.png");
    scene_frame(1.0, [0.0, 0.0], orbit(0))
        .save(&pin)
        .expect("pin");
    let other = dir.path().join("other.png");
    pattern_frame(SOURCE.0, SOURCE.1, 1.12, [40.0, -25.0])
        .save(&other)
        .expect("other pin");

    let pinned = check(&clip, Some(SHIPPED_SEAM), Some(&pin));
    let fidelity = pinned
        .start_fidelity
        .expect("a pinned clip reports fidelity");
    assert!(fidelity > 0.98, "fidelity {fidelity}");
    assert!(pinned.passes(), "{:?}", pinned.failures());

    let wrong = check(&clip, Some(SHIPPED_SEAM), Some(&other));
    assert!(wrong.start_fidelity.expect("fidelity") < START_FIDELITY_MIN);
    assert!(wrong
        .failures()
        .iter()
        .any(|f| matches!(f, LoopFailure::StartFidelity { .. })));

    assert!(check(&clip, Some(SHIPPED_SEAM), None)
        .start_fidelity
        .is_none());
}

#[test]
fn a_state_without_a_seam_is_judged_on_the_frames_its_bake_keeps() {
    let dir = tempfile::tempdir().expect("temp dir");
    let clip = zoom_ramp_loop(dir.path());
    let result = check(&clip, None, None);
    let RepairOutcome::Measured { measures, seam } = &result.repaired else {
        panic!("nothing to refuse without a seam");
    };
    assert!(seam.is_none());
    // The atlas keeps such a state's final frame too (no loop sampling), so the restart shows
    // the whole zoom at once.
    assert!(measures.growth > 0.04, "{measures:?}");
    assert!(!result.passes());
}

#[test]
fn a_zoom_the_scale_lock_refuses_fails_with_the_refusal() {
    let dir = tempfile::tempdir().expect("temp dir");
    // A push-in of 5 % that jumps by 4 % halfway: no straight line explains it.
    let frames = loop_frames(|k| {
        if k < PERIOD / 2 {
            1.0 + 0.004 * k as f64
        } else {
            1.07 + 0.002 * k as f64
        }
    });
    let clip = encode_clip(dir.path(), "jump.mkv", &frames);
    let result = check(&clip, Some(SHIPPED_SEAM), None);
    assert!(
        matches!(result.repaired, RepairOutcome::Refused { .. }),
        "{:?}",
        result.repaired
    );
    assert!(result
        .failures()
        .iter()
        .any(|f| matches!(f, LoopFailure::RepairRefused { .. })));
}

#[test]
fn the_cli_prints_raw_and_repaired_numbers_and_fails_a_failing_loop() {
    let dir = tempfile::tempdir().expect("temp dir");
    let catalog = shipped_catalog();
    let unit = identity_with_actions(
        &catalog,
        &[AnimState::Idle, AnimState::Constructing],
        false,
        |_| true,
    );
    let sheets = dir.path().join("sheets");
    let evidence = dir.path().join("evidence");
    let run = |clip: &Path| {
        baker()
            .args([
                "loop-check",
                "--unit",
                &unit,
                "--state",
                "idle",
                "--size",
                "256",
                "--no-pin",
            ])
            .arg("--clip")
            .arg(clip)
            .arg("--sheets-dir")
            .arg(&sheets)
            .arg("--evidence-dir")
            .arg(&evidence)
            .output()
            .expect("baker runs")
    };

    let ramp = run(&zoom_ramp_loop(dir.path()));
    let out = String::from_utf8_lossy(&ramp.stdout);
    assert!(
        ramp.status.success(),
        "{out}\n{}",
        String::from_utf8_lossy(&ramp.stderr)
    );
    for needle in [
        "raw:",
        "repaired:",
        "wrap",
        "step",
        "worst",
        "growth",
        "verdict: PASS",
    ] {
        assert!(out.contains(needle), "missing {needle:?} in:\n{out}");
    }
    assert!(out.contains("FAIL"), "the raw line fails:\n{out}");
    for name in ["repaired", "seam"] {
        let file = format!("{unit}_idle_{name}.png");
        assert!(sheets.join(&file).is_file(), "{file}");
        assert!(evidence.join(&file).is_file(), "{file}");
    }

    let frames = cut_frames();
    let cut = run(&encode_clip(dir.path(), "cut.mkv", &frames));
    let out = String::from_utf8_lossy(&cut.stdout);
    assert!(!cut.status.success(), "{out}");
    assert!(out.contains("verdict: FAIL"), "{out}");
}

#[test]
fn the_cli_refuses_a_state_that_does_not_loop() {
    let dir = tempfile::tempdir().expect("temp dir");
    let catalog = shipped_catalog();
    let unit = identity_with_actions(
        &catalog,
        &[AnimState::Idle, AnimState::Constructing],
        false,
        |_| true,
    );
    let output = baker()
        .args([
            "loop-check",
            "--unit",
            &unit,
            "--state",
            "constructing",
            "--no-pin",
        ])
        .arg("--clip")
        .arg(synthetic_loop(dir.path()))
        .output()
        .expect("baker runs");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not loop"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
