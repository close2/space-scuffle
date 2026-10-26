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
use std::path::{Path, PathBuf};

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
    let ffmpeg_path = PathBuf::from("/usr/bin/ffmpeg");
    if !ffmpeg_path.exists() {
        return; // Skip if environment lacks ffmpeg
    }

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
    let ffmpeg_path = PathBuf::from("/usr/bin/ffmpeg");
    let concept_video = Path::new("tmp/gemini_generated_video_de659ff9.mp4");

    if !ffmpeg_path.exists() || !concept_video.exists() {
        eprintln!("Skipping test_concept_video_bake_smoke: ffmpeg or concept video missing.");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("tempdir creation failed");
    let meta =
        bake_concept_video(concept_video, temp_dir.path()).expect("Concept video bake failed");

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
