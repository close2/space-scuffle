//! Integration and quality gate tests for baked production terrain assets.
//!
//! # Architectural Charter
//! In accordance with [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md) and
//! Section 4.3 of `docs/plans/21-plan-map-editor.md`, this test suite validates that real,
//! production-grade terrain assets exist at the exact runtime paths expected by the client,
//! maintain valid texture dimensions, and satisfy strict wrap-around seam continuity metrics.
//!
//! # Invariants Maintained
//! - Every layer in the production theme (`orbital_foundry`) has baked albedo, normal,
//!   and ORM maps of dimension exactly `texture_size x texture_size`.
//! - Tiled preview textures have dimension exactly `2·texture_size x 2·texture_size`.
//! - Backdrop texture `void_nebula.png` exists at `texture_size x texture_size`.
//! - Raw source art files exist, decode cleanly, and are square.
//! - Seam discontinuity scores satisfy per-layer thresholds:
//!   - `MinCutBlend` layers: `seam_score < 1.5`.
//!   - `SourceSeamless` layers: `seam_score <= max_seam_score`.
//! - If any file is missing, the test fails loudly with the exact path rather than skipping.

#![allow(
    clippy::doc_markdown,
    clippy::panic,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::too_many_lines
)]

use asset_baker::{
    load_and_resize, seam_score, TerrainThemeCatalog, ThemeBackdrop, ThemeLayer, TilingMode,
};
use image::GenericImageView;
use std::path::{Path, PathBuf};

/// Resolves the absolute path to the workspace root `assets/` directory.
///
/// # Panics
/// Panics with a descriptive diagnostic message if the `assets/` directory cannot be located.
fn find_assets_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../assets"),
        manifest_dir.join("../assets"),
        PathBuf::from("assets"),
        PathBuf::from("../../assets"),
    ];

    for candidate in &candidates {
        if candidate.is_dir() && candidate.join("catalog/terrain_themes.ron").is_file() {
            return candidate
                .canonicalize()
                .expect("Failed to canonicalize valid assets directory path");
        }
    }

    panic!(
        "CRITICAL: assets directory not found. Searched candidates: {:?}",
        candidates
    );
}

/// Verifies presence, dimensions, and aspect ratio of backdrop source and baked textures.
fn verify_backdrop(
    assets_dir: &Path,
    backdrop: &ThemeBackdrop,
    baked_dir: &str,
    expected_size: u32,
) {
    let backdrop_src = assets_dir.join(&backdrop.source);
    assert!(
        backdrop_src.is_file(),
        "Backdrop source art missing at exact path: {}",
        backdrop_src.display()
    );
    let backdrop_src_dyn = image::open(&backdrop_src).unwrap_or_else(|e| {
        panic!(
            "Failed to decode backdrop source art at {}: {e}",
            backdrop_src.display()
        )
    });
    let (b_src_w, b_src_h) = backdrop_src_dyn.dimensions();
    assert_eq!(
        b_src_w,
        b_src_h,
        "Backdrop source art at {} must be square, found {b_src_w}x{b_src_h}",
        backdrop_src.display()
    );

    let baked_backdrop = assets_dir.join(baked_dir).join("void_nebula.png");
    assert!(
        baked_backdrop.is_file(),
        "Baked backdrop texture missing at exact path: {}",
        baked_backdrop.display()
    );
    let backdrop_dyn = image::open(&baked_backdrop).unwrap_or_else(|e| {
        panic!(
            "Failed to decode baked backdrop at {}: {e}",
            baked_backdrop.display()
        )
    });
    assert_eq!(
        backdrop_dyn.dimensions(),
        (expected_size, expected_size),
        "Baked backdrop at {} must be {expected_size}x{expected_size}, found {:?}",
        baked_backdrop.display(),
        backdrop_dyn.dimensions()
    );
}

/// Verifies source art and all baked textures (albedo, normal, ORM, preview) for a single layer.
fn verify_layer(assets_dir: &Path, layer: &ThemeLayer, baked_dir: &str, expected_size: u32) {
    // 1. Source art validation
    let src_path = assets_dir.join(&layer.source);
    assert!(
        src_path.is_file(),
        "Source art for layer '{}' missing at exact path: {}",
        layer.name,
        src_path.display()
    );

    let src_dyn = image::open(&src_path).unwrap_or_else(|e| {
        panic!(
            "Failed to decode source art for layer '{}' at {}: {e}",
            layer.name,
            src_path.display()
        )
    });
    let (src_w, src_h) = src_dyn.dimensions();
    assert_eq!(
        src_w,
        src_h,
        "Source art for layer '{}' at {} must be square, found {src_w}x{src_h}",
        layer.name,
        src_path.display()
    );

    let layer_dir = assets_dir.join(baked_dir);
    let albedo_path = layer_dir.join(format!("{}_albedo.png", layer.name));
    let normal_path = layer_dir.join(format!("{}_normal.png", layer.name));
    let orm_path = layer_dir.join(format!("{}_orm.png", layer.name));
    let preview_path = layer_dir.join(format!("{}_tiled_preview.png", layer.name));

    // 2. Albedo map and seam score
    assert!(
        albedo_path.is_file(),
        "Baked albedo texture for layer '{}' missing at exact path: {}",
        layer.name,
        albedo_path.display()
    );
    let albedo_dyn = image::open(&albedo_path).unwrap_or_else(|e| {
        panic!(
            "Failed to decode baked albedo for layer '{}' at {}: {e}",
            layer.name,
            albedo_path.display()
        )
    });
    assert_eq!(
        albedo_dyn.dimensions(),
        (expected_size, expected_size),
        "Baked albedo for layer '{}' must be {expected_size}x{expected_size}, found {:?}",
        layer.name,
        albedo_dyn.dimensions()
    );

    let albedo_linear = load_and_resize(&albedo_path, expected_size).unwrap_or_else(|e| {
        panic!(
            "Failed to load baked albedo into linear space for layer '{}' at {}: {e}",
            layer.name,
            albedo_path.display()
        )
    });
    let score = seam_score(&albedo_linear);
    println!(
        "Layer '{}' [Mode: {}] baked albedo seam score: {score:.3}",
        layer.name, layer.tiling
    );

    match layer.tiling {
        TilingMode::MinCutBlend => {
            assert!(
                score < 1.5,
                "Albedo seam score for MinCutBlend layer '{}' must be < 1.5, found {score:.3} at {}",
                layer.name,
                albedo_path.display()
            );
        }
        TilingMode::SourceSeamless { max_seam_score } => {
            assert!(
                score <= max_seam_score,
                "Albedo seam score for SourceSeamless layer '{}' must be <= {max_seam_score:.3}, found {score:.3} at {}",
                layer.name,
                albedo_path.display()
            );
        }
    }

    // 3. Normal map
    assert!(
        normal_path.is_file(),
        "Baked normal texture for layer '{}' missing at exact path: {}",
        layer.name,
        normal_path.display()
    );
    let normal_dyn = image::open(&normal_path).unwrap_or_else(|e| {
        panic!(
            "Failed to decode baked normal for layer '{}' at {}: {e}",
            layer.name,
            normal_path.display()
        )
    });
    assert_eq!(
        normal_dyn.dimensions(),
        (expected_size, expected_size),
        "Baked normal for layer '{}' must be {expected_size}x{expected_size}, found {:?}",
        layer.name,
        normal_dyn.dimensions()
    );

    // 4. ORM map
    assert!(
        orm_path.is_file(),
        "Baked ORM texture for layer '{}' missing at exact path: {}",
        layer.name,
        orm_path.display()
    );
    let orm_dyn = image::open(&orm_path).unwrap_or_else(|e| {
        panic!(
            "Failed to decode baked ORM for layer '{}' at {}: {e}",
            layer.name,
            orm_path.display()
        )
    });
    assert_eq!(
        orm_dyn.dimensions(),
        (expected_size, expected_size),
        "Baked ORM for layer '{}' must be {expected_size}x{expected_size}, found {:?}",
        layer.name,
        orm_dyn.dimensions()
    );

    // 5. Tiled preview
    let expected_preview_size = expected_size * 2;
    assert!(
        preview_path.is_file(),
        "Baked tiled preview for layer '{}' missing at exact path: {}",
        layer.name,
        preview_path.display()
    );
    let preview_dyn = image::open(&preview_path).unwrap_or_else(|e| {
        panic!(
            "Failed to decode baked tiled preview for layer '{}' at {}: {e}",
            layer.name,
            preview_path.display()
        )
    });
    assert_eq!(
        preview_dyn.dimensions(),
        (expected_preview_size, expected_preview_size),
        "Baked tiled preview for layer '{}' must be {expected_preview_size}x{expected_preview_size}, found {:?}",
        layer.name,
        preview_dyn.dimensions()
    );
}

#[test]
fn baked_real_assets_exist_and_are_tileable() {
    let assets_dir = find_assets_dir();
    let catalog_path = assets_dir.join("catalog/terrain_themes.ron");

    assert!(
        catalog_path.is_file(),
        "Catalog file missing at exact path: {}",
        catalog_path.display()
    );

    let catalog = TerrainThemeCatalog::load(&catalog_path).expect(
        "Production terrain_themes.ron must parse successfully according to declarative schema",
    );

    let theme = catalog
        .get("orbital_foundry")
        .expect("Theme 'orbital_foundry' must exist in production catalog");

    theme
        .validate()
        .expect("Theme 'orbital_foundry' must satisfy all semantic and numeric invariants");

    let expected_size = theme.texture_size;

    // 1. Verify backdrop
    verify_backdrop(
        &assets_dir,
        &theme.backdrop,
        &theme.baked_dir,
        expected_size,
    );

    // 2. Verify all four layers
    for layer in theme.ordered_layers() {
        verify_layer(&assets_dir, layer, &theme.baked_dir, expected_size);
    }
}
