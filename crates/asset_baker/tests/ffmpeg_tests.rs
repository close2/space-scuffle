//! Integration tests for FFmpeg video extraction and strip baking.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown
)]

use asset_baker::ffmpeg_importer::FfmpegExtractConfig;
use asset_baker::{bake_concept_video, extract_and_pack_video, BakerError};
use std::collections::BTreeMap;
use std::path::PathBuf;

mod common;

#[test]
fn test_ffmpeg_not_found_error() {
    let temp_dir = tempfile::tempdir().expect("tempdir creation failed");
    let fake_ffmpeg = PathBuf::from("/nonexistent/bin/ffmpeg");
    let video_path = PathBuf::from("nonexistent.mp4");
    let png_path = temp_dir.path().join("out.png");
    let ron_path = temp_dir.path().join("out.ron");

    let config = FfmpegExtractConfig {
        ffmpeg_path: fake_ffmpeg.clone(),
        target_size: 256,
        target_fps: 24,
    };

    let result = extract_and_pack_video(
        &video_path,
        &png_path,
        &ron_path,
        "test",
        &config,
        BTreeMap::new(),
    );

    match result {
        Err(BakerError::FfmpegNotFound { path }) => {
            assert_eq!(path, fake_ffmpeg);
        }
        other => panic!("Expected FfmpegNotFound error, got {other:?}"),
    }
}

#[test]
fn test_video_source_not_found_error() {
    let temp_dir = tempfile::tempdir().expect("tempdir creation failed");
    let _ = common::fixture_clip();
    let ffmpeg_path = PathBuf::from("/usr/bin/ffmpeg");

    let fake_video = temp_dir.path().join("ghost_video.mp4");
    let png_path = temp_dir.path().join("out.png");
    let ron_path = temp_dir.path().join("out.ron");

    let config = FfmpegExtractConfig {
        ffmpeg_path,
        target_size: 256,
        target_fps: 24,
    };

    let result = extract_and_pack_video(
        &fake_video,
        &png_path,
        &ron_path,
        "test",
        &config,
        BTreeMap::new(),
    );

    match result {
        Err(BakerError::Io { path, .. }) => {
            assert_eq!(path, fake_video);
        }
        other => panic!("Expected Io error for missing source video, got {other:?}"),
    }
}

#[test]
fn test_concept_video_bake_smoke() {
    let concept_video = common::fixture_clip();

    let temp_dir = tempfile::tempdir().expect("tempdir creation failed");
    let meta =
        bake_concept_video(&concept_video, temp_dir.path()).expect("Concept video bake failed");

    assert_eq!(meta.name, "concept_video");
    assert_eq!(meta.frame_width, 256);
    assert_eq!(meta.frame_height, 256);
    assert_eq!(meta.fps, 24);
    assert!(meta.total_frames > 0, "Concept video produced 0 frames");

    let png_path = temp_dir.path().join("concept_video.png");
    let ron_path = temp_dir.path().join("concept_video.flipbook.ron");

    assert!(
        png_path.exists(),
        "concept_video.png was not written to disk"
    );
    assert!(
        ron_path.exists(),
        "concept_video.flipbook.ron was not written to disk"
    );
}

// --- Seam repair (plan `docs/plans/2026-11-04-veo-seamless-loops.md`, D-5) ------------------

mod seam {
    use super::common::loop_clip::{encode_clip, pattern_frame, steady_ramp, zoom_clip};
    use asset_baker::ffmpeg_importer::{decode_framed_clip, decode_repaired_clip, FrameSpec};
    use asset_baker::loop_metrics::{estimate_zoom, SeamSpec};
    use asset_baker::provenance::FrameSampling;
    use asset_baker::{BakeFraming, BakerError};
    use image::RgbaImage;
    use std::path::Path;

    const FFMPEG: &str = "/usr/bin/ffmpeg";

    /// A 128 px square bake at the synthetic clips' own rate.
    const CROP_128: FrameSpec = FrameSpec {
        size: 128,
        fps: 15,
        framing: BakeFraming::CenterCrop,
    };

    const REPAIR: SeamSpec = SeamSpec {
        scale_lock: true,
        crossfade_frames: 2,
    };

    fn assert_all_within(frames: &[RgbaImage], limit: f64) {
        for (k, frame) in frames.iter().enumerate() {
            let estimate = estimate_zoom(&frames[0], frame).unwrap();
            assert!(
                estimate.growth().abs() <= limit,
                "frame {k} differs from frame 0 by {estimate:?}"
            );
        }
    }

    #[test]
    fn a_six_percent_push_in_is_locked_and_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let clip = zoom_clip(dir.path(), "ramp.mkv", 320, &steady_ramp(31, 0.06));
        let repaired =
            decode_repaired_clip(Path::new(FFMPEG), &clip, CROP_128, 16, "ramp", Some(REPAIR))
                .unwrap();
        let record = repaired.seam.expect("a repaired state has a seam record");
        assert_eq!(record.sampling, FrameSampling::Loop);
        assert_eq!(record.crossfade_frames, 2);
        // Loop sampling keeps source frames 0, 1.875, ... 28 of 30, so the kept span grows by
        // 1.06^(28/30) - 1, about 56 per mille.
        assert!(
            record.scale_permille.abs_diff(56) <= 4,
            "about 56 per mille, got {record:?}"
        );
        let frames = &repaired.framed.frames;
        assert_eq!(frames.len(), 16);
        assert!(frames.iter().all(|f| f.dimensions() == (128, 128)));
        assert_all_within(frames, 0.015);
    }

    #[test]
    fn the_unrepaired_ramp_grows_beyond_the_review_limit() {
        let dir = tempfile::tempdir().unwrap();
        let clip = zoom_clip(dir.path(), "ramp.mkv", 320, &steady_ramp(31, 0.06));
        let raw = SeamSpec {
            scale_lock: false,
            crossfade_frames: 0,
        };
        let sampled =
            decode_repaired_clip(Path::new(FFMPEG), &clip, CROP_128, 16, "raw", Some(raw)).unwrap();
        let record = sampled.seam.unwrap();
        assert_eq!(record.scale_permille, 0);
        let last = sampled.framed.frames.last().unwrap();
        let growth = estimate_zoom(&sampled.framed.frames[0], last)
            .unwrap()
            .growth();
        assert!(growth > 0.045, "the raw loop still pushes in: {growth}");
    }

    #[test]
    fn a_ramp_with_a_sudden_jump_is_refused_by_the_bake() {
        let dir = tempfile::tempdir().unwrap();
        let mut magnifications = steady_ramp(31, 0.05);
        for m in magnifications.iter_mut().skip(15) {
            *m *= 1.04;
        }
        let clip = zoom_clip(dir.path(), "jump.mkv", 320, &magnifications);
        let refused =
            decode_repaired_clip(Path::new(FFMPEG), &clip, CROP_128, 16, "jump", Some(REPAIR));
        match refused {
            Err(BakerError::SeamRepairRefused { clip, .. }) => assert_eq!(clip, "jump"),
            other => panic!("expected SeamRepairRefused, got {other:?}"),
        }
    }

    #[test]
    fn a_drift_of_a_third_of_a_percent_is_left_unscaled() {
        let dir = tempfile::tempdir().unwrap();
        let clip = zoom_clip(dir.path(), "drift.mkv", 320, &steady_ramp(31, 0.003));
        let repaired = decode_repaired_clip(
            Path::new(FFMPEG),
            &clip,
            CROP_128,
            16,
            "drift",
            Some(REPAIR),
        )
        .unwrap();
        assert_eq!(repaired.seam.unwrap().scale_permille, 0);
    }

    #[test]
    fn a_letterboxed_push_in_is_locked_inside_its_bars() {
        let dir = tempfile::tempdir().unwrap();
        let frames: Vec<RgbaImage> = steady_ramp(31, 0.08)
            .iter()
            .map(|m| pattern_frame(320, 180, *m, [0.0, 0.0]))
            .collect();
        let clip = encode_clip(dir.path(), "wide.mkv", &frames);
        let spec = FrameSpec {
            framing: BakeFraming::Letterbox {
                feather_permille: 0,
            },
            ..CROP_128
        };
        let repaired =
            decode_repaired_clip(Path::new(FFMPEG), &clip, spec, 12, "wide", Some(REPAIR)).unwrap();
        assert!(repaired.framed.bar_rgb.is_some());
        let record = repaired.seam.unwrap();
        // Twelve loop samples of 31 frames end at frame 27: 1.08^(27/30) - 1 is 72 per mille.
        assert!(record.scale_permille.abs_diff(72) <= 5, "{record:?}");
        // The content band of each square is 72 rows high, centred.
        let content: Vec<RgbaImage> = repaired
            .framed
            .frames
            .iter()
            .map(|f| image::imageops::crop_imm(f, 0, 28, 128, 72).to_image())
            .collect();
        assert_all_within(&content, 0.015);
    }

    #[test]
    fn a_clip_without_a_seam_decodes_exactly_as_before() {
        let clip = super::common::fixture_clip();
        let before = decode_framed_clip(Path::new(FFMPEG), &clip, CROP_128, 16, "before").unwrap();
        let after =
            decode_repaired_clip(Path::new(FFMPEG), &clip, CROP_128, 16, "after", None).unwrap();
        assert_eq!(after.framed, before);
        assert!(after.seam.is_none());
    }
}
