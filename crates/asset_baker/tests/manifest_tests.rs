//! Integration tests for FlipbookAtlasMeta serialization, deserialization, and validation invariants.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown
)]

use asset_baker::manifest::{ActionSlice, FlipbookAtlasMeta, FlipbookLayout};
use asset_baker::BakerError;
use sim_types::enums::AnimState;
use std::collections::BTreeMap;

#[test]
fn test_manifest_validation_success() {
    let mut actions = BTreeMap::new();
    actions.insert(
        AnimState::Idle,
        ActionSlice {
            start_frame: 0,
            frame_count: 8,
            looping: true,
            hold_last_frame: false,
            duration_ms: None,
        },
    );
    actions.insert(
        AnimState::Attack,
        ActionSlice {
            start_frame: 8,
            frame_count: 8,
            looping: false,
            hold_last_frame: true,
            duration_ms: None,
        },
    );

    let meta = FlipbookAtlasMeta {
        name: "test_marine".into(),
        frame_width: 192,
        frame_height: 192,
        fps: 15,
        total_frames: 16,
        layout: FlipbookLayout::VerticalStrip,
        actions,
    };

    assert!(meta.validate().is_ok());
}

#[test]
fn test_manifest_validation_zero_dimensions() {
    let mut actions = BTreeMap::new();
    actions.insert(
        AnimState::Idle,
        ActionSlice {
            start_frame: 0,
            frame_count: 4,
            looping: true,
            hold_last_frame: false,
            duration_ms: None,
        },
    );

    let meta_zero_width = FlipbookAtlasMeta {
        name: "zero_width".into(),
        frame_width: 0,
        frame_height: 192,
        fps: 15,
        total_frames: 4,
        layout: FlipbookLayout::VerticalStrip,
        actions: actions.clone(),
    };
    match meta_zero_width.validate() {
        Err(BakerError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("dimensions"));
        }
        other => panic!("Expected ValidationFailed, got {other:?}"),
    }

    let meta_zero_fps = FlipbookAtlasMeta {
        name: "zero_fps".into(),
        frame_width: 192,
        frame_height: 192,
        fps: 0,
        total_frames: 4,
        layout: FlipbookLayout::VerticalStrip,
        actions,
    };
    match meta_zero_fps.validate() {
        Err(BakerError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("FPS"));
        }
        other => panic!("Expected ValidationFailed, got {other:?}"),
    }
}

#[test]
fn test_manifest_validation_frame_range_overflow() {
    let mut actions = BTreeMap::new();
    actions.insert(
        AnimState::Idle,
        ActionSlice {
            start_frame: 10,
            frame_count: 8, // 10 + 8 = 18 > 16
            looping: true,
            hold_last_frame: false,
            duration_ms: None,
        },
    );

    let meta = FlipbookAtlasMeta {
        name: "overflow".into(),
        frame_width: 192,
        frame_height: 192,
        fps: 15,
        total_frames: 16,
        layout: FlipbookLayout::VerticalStrip,
        actions,
    };

    match meta.validate() {
        Err(BakerError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("exceeds total frames"));
        }
        other => panic!("Expected ValidationFailed, got {other:?}"),
    }
}

#[test]
fn test_manifest_validation_zero_frame_count() {
    let mut actions = BTreeMap::new();
    actions.insert(
        AnimState::Idle,
        ActionSlice {
            start_frame: 0,
            frame_count: 0,
            looping: true,
            hold_last_frame: false,
            duration_ms: None,
        },
    );

    let meta = FlipbookAtlasMeta {
        name: "zero_frames".into(),
        frame_width: 192,
        frame_height: 192,
        fps: 15,
        total_frames: 16,
        layout: FlipbookLayout::VerticalStrip,
        actions,
    };

    match meta.validate() {
        Err(BakerError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("frame count of 0"));
        }
        other => panic!("Expected ValidationFailed, got {other:?}"),
    }
}

#[test]
fn test_manifest_ron_roundtrip() {
    let temp_dir = tempfile::tempdir().expect("tempdir creation failed");
    let ron_path = temp_dir.path().join("test_atlas.flipbook.ron");

    let mut actions = BTreeMap::new();
    actions.insert(
        AnimState::Idle,
        ActionSlice {
            start_frame: 0,
            frame_count: 8,
            looping: true,
            hold_last_frame: false,
            duration_ms: None,
        },
    );
    actions.insert(
        AnimState::Walk,
        ActionSlice {
            start_frame: 8,
            frame_count: 8,
            looping: true,
            hold_last_frame: false,
            duration_ms: None,
        },
    );
    actions.insert(
        AnimState::Attack,
        ActionSlice {
            start_frame: 16,
            frame_count: 8,
            looping: false,
            hold_last_frame: false,
            duration_ms: None,
        },
    );
    actions.insert(
        AnimState::Die,
        ActionSlice {
            start_frame: 24,
            frame_count: 8,
            looping: false,
            hold_last_frame: true,
            duration_ms: None,
        },
    );

    let meta = FlipbookAtlasMeta {
        name: "marine_roundtrip".into(),
        frame_width: 192,
        frame_height: 192,
        fps: 15,
        total_frames: 32,
        layout: FlipbookLayout::VerticalStrip,
        actions,
    };

    meta.save_to_ron(&ron_path).expect("save_to_ron failed");
    let loaded = FlipbookAtlasMeta::load_from_ron(&ron_path).expect("load_from_ron failed");

    assert_eq!(meta, loaded);
}

// --- Per-slice clip duration (plan `2026-11-04-battle-flipbook-animation.md`, WP3, D-6) -------

/// Every committed `assets/textures/*.flipbook.ron`; all of them predate `duration_ms`.
fn committed_manifests() -> Vec<std::path::PathBuf> {
    let textures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/textures");
    let mut paths: Vec<_> = std::fs::read_dir(&textures)
        .expect("assets/textures is readable")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".flipbook.ron"))
        })
        .collect();
    paths.sort();
    paths
}

#[test]
fn committed_manifests_without_a_duration_reserialise_byte_for_byte() {
    let paths = committed_manifests();
    assert!(!paths.is_empty(), "no committed manifest was found");
    for path in paths {
        let text = std::fs::read_to_string(&path).expect("manifest text");
        let meta = FlipbookAtlasMeta::load_from_ron(&path).expect("committed manifest loads");
        assert!(
            meta.actions
                .values()
                .all(|slice| slice.duration_ms.is_none()),
            "{} has no duration field to read",
            path.display()
        );
        let out = tempfile::tempdir().expect("tempdir");
        let written = out.path().join("rewritten.flipbook.ron");
        meta.save_to_ron(&written).expect("re-serialise");
        let rewritten = std::fs::read_to_string(&written).expect("rewritten text");
        assert_eq!(
            rewritten,
            text,
            "{} must keep its bytes, and so its provenance hash",
            path.display()
        );
    }
}

fn timed_meta(duration_ms: Option<u32>) -> FlipbookAtlasMeta {
    let slice = |start_frame, looping, duration_ms| ActionSlice {
        start_frame,
        frame_count: 10,
        looping,
        hold_last_frame: false,
        duration_ms,
    };
    FlipbookAtlasMeta {
        name: "timed".into(),
        frame_width: 64,
        frame_height: 64,
        fps: 15,
        total_frames: 20,
        layout: FlipbookLayout::VerticalStrip,
        actions: BTreeMap::from([
            (AnimState::Idle, slice(0, true, duration_ms)),
            (AnimState::Attack, slice(10, false, None)),
        ]),
    }
}

#[test]
fn a_manifest_with_a_duration_round_trips() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("timed.flipbook.ron");
    let meta = timed_meta(Some(4010));
    meta.save_to_ron(&path).expect("save");
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(text.contains("duration_ms: Some(4010)"), "{text}");
    assert_eq!(
        text.matches("duration_ms").count(),
        1,
        "an absent duration is not written: {text}"
    );
    let loaded = FlipbookAtlasMeta::load_from_ron(&path).expect("load");
    assert_eq!(loaded, meta);
    assert_eq!(loaded.actions[&AnimState::Idle].duration_ms, Some(4010));
    assert_eq!(loaded.actions[&AnimState::Attack].duration_ms, None);
}

#[test]
fn a_zero_duration_is_refused() {
    match timed_meta(Some(0)).validate() {
        Err(BakerError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("duration"), "{reason}");
        }
        other => panic!("expected ValidationFailed, got {other:?}"),
    }
}
