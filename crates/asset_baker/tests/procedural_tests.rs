//! Integration tests for procedural atlas generation of units and building domes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::uninlined_format_args
)]

use asset_baker::manifest::FlipbookLayout;
use asset_baker::procedural_generator::{
    generate_procedural_atlas, generate_procedural_dome_atlas, generate_procedural_unit_atlas,
};
use asset_baker::{
    default_dome_archetypes, default_unit_archetypes, generate_all_procedural_atlases, BakerError,
};
use sim_core::UnitCatalog;
use sim_types::enums::AnimState;

/// The shipped unit catalog; the baker itself loads it from disk at run time.
fn catalog() -> UnitCatalog {
    sim_fixtures::shipped_catalogs().units
}

#[test]
fn test_all_mobile_units_generation() {
    let catalog = catalog();
    for unit in default_unit_archetypes(&catalog) {
        let output = generate_procedural_unit_atlas(&catalog, &unit, 192, 15)
            .unwrap_or_else(|e| panic!("Failed to generate unit {unit}: {e:?}"));

        assert_eq!(output.meta.name, unit);
        assert_eq!(output.meta.frame_width, 192);
        assert_eq!(output.meta.frame_height, 192);
        assert_eq!(output.meta.fps, 15);
        assert_eq!(output.meta.total_frames, 32); // 4 states * 8 frames
        assert_eq!(output.meta.layout, FlipbookLayout::VerticalStrip);

        // Verify image dimensions: 192 x (192 * 32)
        assert_eq!(output.image.width(), 192);
        assert_eq!(output.image.height(), 192 * 32);

        // Verify required animation states are mapped
        assert!(output.meta.actions.contains_key(&AnimState::Idle));
        assert!(output.meta.actions.contains_key(&AnimState::Walk));
        assert!(output.meta.actions.contains_key(&AnimState::Attack));
        assert!(output.meta.actions.contains_key(&AnimState::Die));

        // Verify pixel buffer contains non-zero pixels
        let has_visible_pixels = output
            .image
            .pixels()
            .any(|p| p[3] > 0 && (p[0] > 0 || p[1] > 0 || p[2] > 0));
        assert!(
            has_visible_pixels,
            "Generated image for {unit} was completely black"
        );
    }
}

#[test]
fn test_all_building_domes_generation() {
    let catalog = catalog();
    for dome in default_dome_archetypes(&catalog) {
        let output = generate_procedural_dome_atlas(&catalog, &dome, 256, 15)
            .unwrap_or_else(|e| panic!("Failed to generate dome {dome}: {e:?}"));

        assert_eq!(output.meta.name, dome);
        assert_eq!(output.meta.frame_width, 256);
        assert_eq!(output.meta.frame_height, 256);
        assert_eq!(output.meta.fps, 15);
        assert_eq!(output.meta.layout, FlipbookLayout::VerticalStrip);

        // Verify image dimensions
        assert_eq!(output.image.width(), 256);
        assert_eq!(output.image.height(), 256 * output.meta.total_frames);

        // Verify common dome states
        assert!(output.meta.actions.contains_key(&AnimState::Constructing));
        assert!(output.meta.actions.contains_key(&AnimState::Idle));
        assert!(output.meta.actions.contains_key(&AnimState::Die));

        // Verify pixel buffer contains non-zero pixels
        let has_visible_pixels = output
            .image
            .pixels()
            .any(|p| p[3] > 0 && (p[0] > 0 || p[1] > 0 || p[2] > 0));
        assert!(
            has_visible_pixels,
            "Generated image for {dome} was completely black"
        );
    }
}

#[test]
fn test_unknown_archetype_error() {
    let result = generate_procedural_atlas(&catalog(), "fictional_battlecruiser", None, None);
    match result {
        Err(BakerError::UnknownArchetype { name, valid }) => {
            assert_eq!(name, "fictional_battlecruiser");
            assert!(valid.iter().any(|v| v == "pulse_bot"));
        }
        other => panic!("Expected UnknownArchetype error, got {other:?}"),
    }
}

#[test]
fn test_generate_all_procedural_atlases_to_disk() {
    let temp_dir = tempfile::tempdir().expect("tempdir creation failed");
    let catalog = catalog();
    let metas =
        generate_all_procedural_atlases(&catalog, temp_dir.path()).expect("generate_all failed");

    let expected =
        default_unit_archetypes(&catalog).len() + default_dome_archetypes(&catalog).len();
    assert_eq!(metas.len(), expected);

    for meta in &metas {
        let png = temp_dir.path().join(format!("{}.png", meta.name));
        let ron = temp_dir.path().join(format!("{}.flipbook.ron", meta.name));
        assert!(png.exists(), "PNG file missing: {:?}", png);
        assert!(ron.exists(), "RON file missing: {:?}", ron);

        // Verify procedural dimensions and WebGPU height limit
        assert!(meta.frame_width == 192 || meta.frame_width == 256);
        assert!(meta.frame_height == 192 || meta.frame_height == 256);
        let height = meta.frame_height * meta.total_frames;
        assert!(height <= 16384);
    }
}
