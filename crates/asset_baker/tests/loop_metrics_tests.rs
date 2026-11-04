//! Behavioural tests of the loop measures and the seam repair primitives (plan
//! `docs/plans/2026-11-04-veo-seamless-loops.md`, decisions D-5 and D-6, WP2).
//!
//! Every frame here is rendered in memory by `common::loop_clip`, whose scene is computed
//! analytically, so the true magnification and offset of each frame are known exactly. The tests
//! that go through `FFmpeg` live in `ffmpeg_tests.rs` and `provenance_tests.rs`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

mod common;

use asset_baker::ffmpeg_importer::evenly_spaced_loop_indices;
use asset_baker::loop_metrics::{
    apply_scale_lock, crossfade_into_first, crossfade_weight, estimate_zoom, fit_line,
    plan_scale_lock, ssim, MAX_CROSSFADE_FRAMES,
};
use asset_baker::terrain_texture::linear_to_srgb;
use asset_baker::BakerError;
use common::loop_clip::{pattern_frame, steady_ramp};
use image::{Rgba, RgbaImage};

/// Frames of a 16:9 clip decoded at twice a 96 px frame, the shape the bake repairs.
const WIDE: (u32, u32) = (192, 108);

fn noise(width: u32, height: u32, seed: u64) -> RgbaImage {
    let mut state = seed;
    RgbaImage::from_fn(width, height, |_, _| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let value = (state >> 56) as u8;
        Rgba([value, value, value, 255])
    })
}

fn shifted_right(frame: &RgbaImage, pixels: u32) -> RgbaImage {
    RgbaImage::from_fn(frame.width(), frame.height(), |x, y| {
        *frame.get_pixel(x.saturating_sub(pixels), y)
    })
}

fn wide_frames(magnifications: &[f64]) -> Vec<RgbaImage> {
    magnifications
        .iter()
        .map(|m| pattern_frame(WIDE.0, WIDE.1, *m, [0.0, 0.0]))
        .collect()
}

// --- SSIM -------------------------------------------------------------------------------------

#[test]
fn ssim_of_identical_frames_is_one() {
    let frame = pattern_frame(96, 64, 1.0, [0.0, 0.0]);
    let value = ssim(&frame, &frame).unwrap();
    assert!((value - 1.0).abs() < 1e-12, "{value}");
}

#[test]
fn ssim_of_a_one_pixel_shift_is_high_but_below_one() {
    let frame = pattern_frame(96, 64, 1.0, [0.0, 0.0]);
    let value = ssim(&frame, &shifted_right(&frame, 1)).unwrap();
    assert!(value > 0.8 && value < 1.0, "{value}");
}

#[test]
fn ssim_of_unrelated_noise_is_low() {
    let value = ssim(&noise(96, 64, 1), &noise(96, 64, 2)).unwrap();
    assert!(value < 0.2, "{value}");
}

#[test]
fn ssim_refuses_frames_of_different_sizes() {
    assert!(matches!(
        ssim(&noise(96, 64, 1), &noise(64, 64, 1)),
        Err(BakerError::ValidationFailed { .. })
    ));
}

// --- Zoom estimator ---------------------------------------------------------------------------

#[test]
fn the_estimator_recovers_a_known_scale_and_offset() {
    let reference = pattern_frame(256, 256, 1.0, [0.0, 0.0]);
    let frame = pattern_frame(256, 256, 1.05, [6.0, -3.0]);
    let estimate = estimate_zoom(&reference, &frame).unwrap();
    assert!(
        (estimate.growth() - 0.05).abs() <= 0.005,
        "growth {estimate:?}"
    );
    assert!(
        (estimate.offset[0] - 6.0).abs() <= 1.0,
        "offset {estimate:?}"
    );
    assert!(
        (estimate.offset[1] + 3.0).abs() <= 1.0,
        "offset {estimate:?}"
    );
}

#[test]
fn the_estimator_measures_a_pull_out_on_a_wide_frame() {
    let reference = pattern_frame(320, 180, 1.0, [0.0, 0.0]);
    let frame = pattern_frame(320, 180, 0.96, [-5.0, 2.0]);
    let estimate = estimate_zoom(&reference, &frame).unwrap();
    assert!(
        (estimate.growth() + 0.04).abs() <= 0.005,
        "growth {estimate:?}"
    );
    assert!(
        (estimate.offset[0] + 5.0).abs() <= 1.0,
        "offset {estimate:?}"
    );
    assert!(
        (estimate.offset[1] - 2.0).abs() <= 1.0,
        "offset {estimate:?}"
    );
}

#[test]
fn the_estimator_sees_no_zoom_between_identical_frames() {
    let frame = pattern_frame(160, 90, 1.0, [0.0, 0.0]);
    let estimate = estimate_zoom(&frame, &frame).unwrap();
    assert!(estimate.growth().abs() < 1e-4, "{estimate:?}");
    assert!(estimate.offset[0].abs() < 0.05 && estimate.offset[1].abs() < 0.05);
}

// --- Line fit ---------------------------------------------------------------------------------

#[test]
fn the_line_fit_of_an_exact_ramp_has_zero_residual() {
    let values: Vec<f64> = (0..12).map(|k| 0.25 - 0.0125 * f64::from(k)).collect();
    let fit = fit_line(&values).unwrap();
    assert!(fit.max_residual < 1e-12, "{fit:?}");
    assert!((fit.slope + 0.0125).abs() < 1e-12);
    assert!((fit.intercept - 0.25).abs() < 1e-12);
    assert!((fit.at(11.0) - values[11]).abs() < 1e-12);
}

#[test]
fn the_line_fit_of_a_step_has_a_residual_of_about_a_quarter_of_it() {
    let values: Vec<f64> = (0..16).map(|k| if k < 8 { 0.0 } else { 0.04 }).collect();
    let fit = fit_line(&values).unwrap();
    assert!(fit.max_residual > 0.008, "{fit:?}");
    assert!(fit_line(&[]).is_none());
    assert!(fit_line(&[0.5]).is_some_and(|fit| fit.max_residual == 0.0));
}

// --- Loop sampling ----------------------------------------------------------------------------

#[test]
fn loop_sampling_drops_the_final_frame_and_keeps_every_step_equal() {
    let indices = evenly_spaced_loop_indices(96, 16);
    assert_eq!(indices.len(), 16);
    assert_eq!(indices[0], 0);
    assert!(!indices.contains(&95), "{indices:?}");
    // The wrap step runs from the last kept frame to frame 95, which shows frame 0 again.
    let mut steps: Vec<usize> = indices.windows(2).map(|pair| pair[1] - pair[0]).collect();
    steps.push(95 - indices[15]);
    let shortest = *steps.iter().min().unwrap();
    let longest = *steps.iter().max().unwrap();
    assert!(longest - shortest <= 1, "uneven steps {steps:?}");
    assert_eq!(steps.iter().sum::<usize>(), 95);
}

#[test]
fn loop_sampling_of_a_short_clip_keeps_all_but_the_final_frame() {
    assert_eq!(
        evenly_spaced_loop_indices(10, 16),
        (0..9).collect::<Vec<_>>()
    );
    assert_eq!(
        evenly_spaced_loop_indices(10, 9),
        (0..9).collect::<Vec<_>>()
    );
    assert_eq!(evenly_spaced_loop_indices(1, 4), vec![0]);
    assert_eq!(evenly_spaced_loop_indices(0, 4), Vec::<usize>::new());
    assert_eq!(evenly_spaced_loop_indices(60, 0), Vec::<usize>::new());
}

// --- Cross-fade -------------------------------------------------------------------------------

#[test]
fn the_cross_fade_weights_of_two_frames_are_a_third_and_two_thirds() {
    assert!((crossfade_weight(0, 2) - 1.0 / 3.0).abs() < 1e-12);
    assert!((crossfade_weight(1, 2) - 2.0 / 3.0).abs() < 1e-12);
}

#[test]
fn the_cross_fade_blends_the_last_frames_toward_frame_zero_in_linear_light() {
    let black = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255]));
    let white = RgbaImage::from_pixel(8, 8, Rgba([255, 255, 255, 255]));
    let mut frames = vec![black.clone()];
    frames.extend(std::iter::repeat_n(white.clone(), 7));
    crossfade_into_first(&mut frames, 2).unwrap();
    assert_eq!(frames[0], black, "frame 0 is never changed");
    assert!(
        frames[1..6].iter().all(|frame| *frame == white),
        "frames before the fade are untouched"
    );
    let expected = |linear: f32| (linear_to_srgb(linear) * 255.0).round() as u8;
    assert_eq!(frames[6].get_pixel(3, 3)[0], expected(2.0 / 3.0));
    assert_eq!(frames[7].get_pixel(3, 3)[0], expected(1.0 / 3.0));
    assert_eq!(frames[7].get_pixel(3, 3)[3], 255);
}

#[test]
fn a_cross_fade_longer_than_allowed_is_refused() {
    let frame = RgbaImage::from_pixel(4, 4, Rgba([9, 9, 9, 255]));
    let mut many = vec![frame.clone(); 40];
    let too_long = usize::from(MAX_CROSSFADE_FRAMES) + 1;
    assert!(matches!(
        crossfade_into_first(&mut many, too_long),
        Err(BakerError::ValidationFailed { .. })
    ));
    let mut few = vec![frame; 3];
    assert!(
        crossfade_into_first(&mut few, 3).is_err(),
        "a fade over every frame would blend frame 0"
    );
    assert!(crossfade_into_first(&mut few, 2).is_ok());
    let mut none: Vec<RgbaImage> = Vec::new();
    assert!(crossfade_into_first(&mut none, 0).is_ok());
}

// --- Scale lock -------------------------------------------------------------------------------

#[test]
fn a_steady_push_in_is_fitted_and_locked_to_the_last_frame() {
    let frames = wide_frames(&steady_ramp(12, 0.06));
    let plan = plan_scale_lock(&frames, "steady").unwrap();
    assert!(plan.apply, "{plan:?}");
    assert!((plan.growth - 0.06).abs() < 0.004, "{plan:?}");
    assert!(plan.residual < 0.002, "{plan:?}");
    let locked = apply_scale_lock(&frames, &plan);
    assert_eq!(locked.len(), frames.len());
    let last = locked.last().unwrap();
    for (k, frame) in locked.iter().enumerate() {
        assert_eq!(frame.dimensions(), WIDE);
        let estimate = estimate_zoom(last, frame).unwrap();
        assert!(
            estimate.growth().abs() <= 0.015,
            "frame {k} still differs from the last by {estimate:?}"
        );
    }
}

#[test]
fn a_ramp_with_a_sudden_jump_is_refused() {
    let mut magnifications = steady_ramp(12, 0.05);
    for m in magnifications.iter_mut().skip(6) {
        *m *= 1.04;
    }
    let refused = plan_scale_lock(&wide_frames(&magnifications), "jump");
    assert!(
        matches!(refused, Err(BakerError::SeamRepairRefused { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_push_in_beyond_twelve_percent_is_refused() {
    let refused = plan_scale_lock(&wide_frames(&steady_ramp(12, 0.14)), "deep");
    assert!(
        matches!(refused, Err(BakerError::SeamRepairRefused { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_drift_under_half_a_percent_is_left_alone() {
    let frames = wide_frames(&steady_ramp(12, 0.003));
    let plan = plan_scale_lock(&frames, "drift").unwrap();
    assert!(!plan.apply, "{plan:?}");
    assert_eq!(plan.enlargement_permille(), 0);
    assert_eq!(apply_scale_lock(&frames, &plan), frames);
}
