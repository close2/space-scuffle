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
        },
    );
    actions.insert(
        AnimState::Attack,
        ActionSlice {
            start_frame: 8,
            frame_count: 8,
            looping: false,
            hold_last_frame: true,
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
        },
    );
    actions.insert(
        AnimState::Walk,
        ActionSlice {
            start_frame: 8,
            frame_count: 8,
            looping: true,
            hold_last_frame: false,
        },
    );
    actions.insert(
        AnimState::Attack,
        ActionSlice {
            start_frame: 16,
            frame_count: 8,
            looping: false,
            hold_last_frame: false,
        },
    );
    actions.insert(
        AnimState::Die,
        ActionSlice {
            start_frame: 24,
            frame_count: 8,
            looping: false,
            hold_last_frame: true,
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
