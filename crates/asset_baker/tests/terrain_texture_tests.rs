//! Unit and integration tests for terrain theme loading, validation, and texture processing.
//!
//! # Architectural Charter
//! In accordance with [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md) and
//! Section 4.3 of `docs/plans/21-plan-map-editor.md` (v3.1), these tests mathematically verify:
//! - Separable two-pass Burt-Adelson Laplacian pyramid multiband blending for tileability.
//! - Wrap seam quantification metric (`seam_score`).
//! - Height field derivation and dynamic range normalization.
//! - Tangent-space normal mapping with Sobel gradients in OpenGL (+Y up) format.
//! - Packed PBR Occlusion-Roughness-Metallic (ORM) channel derivation and range clamping.
//! - Exact IEC 61966-2-1 piecewise sRGB transfer function round-trip precision for 8-bit channels.
//! - Declarative theme catalog parsing, structural invariants, and typed validation error paths.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use asset_baker::{
    bake_theme, derive_height, derive_normal, derive_orm, find_horizontal_min_cut,
    find_vertical_min_cut, linear_to_srgb, load_and_resize, make_tileable, seam_score,
    srgb_to_linear, tiled_preview, GrayF32Image, MinCutBuffers, Rgba32FImage, Rgba8Image,
    TerrainTheme, TerrainThemeCatalog, TerrainThemeError, ThemeBackdrop, ThemeLayer, ThemeLayers,
    ThemeLighting, TilingMode, LAYER_INDEX_CLIFF, LAYER_INDEX_DIRT, LAYER_INDEX_GRASS,
    LAYER_INDEX_METAL_PLATE,
};
use image::{Luma, Rgba};
use std::path::PathBuf;

/// Constructs a deterministic 2D non-periodic value noise image on an ~8-pixel lattice.
fn deterministic_value_noise(width: u32, height: u32, cell_size: u32) -> Rgba32FImage {
    let mut img = Rgba32FImage::new(width, height);

    let hash_node = |gx: u32, gy: u32| -> f32 {
        let mut h = u64::from(gx).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ u64::from(gy).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 31;
        (h as f64 / u64::MAX as f64) as f32
    };

    for y in 0..height {
        let gy = y / cell_size;
        let fy = (y % cell_size) as f32 / cell_size as f32;
        let wy = fy * fy * (3.0 - 2.0 * fy);

        for x in 0..width {
            let gx = x / cell_size;
            let fx = (x % cell_size) as f32 / cell_size as f32;
            let wx = fx * fx * (3.0 - 2.0 * fx);

            let v00 = hash_node(gx, gy);
            let v10 = hash_node(gx + 1, gy);
            let v01 = hash_node(gx, gy + 1);
            let v11 = hash_node(gx + 1, gy + 1);

            let v0 = v00 * (1.0 - wx) + v10 * wx;
            let v1 = v01 * (1.0 - wx) + v11 * wx;
            let val = v0 * (1.0 - wy) + v1 * wy;

            img.put_pixel(x, y, Rgba([val, val, val, 1.0]));
        }
    }
    img
}

/// Constructs a deterministic 2D non-periodic seeded noise image without lattice-induced column synchrony.
fn deterministic_seeded_noise(width: u32, height: u32) -> Rgba32FImage {
    let mut img = Rgba32FImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let mut h = u64::from(x).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ u64::from(y).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            h ^= h >> 31;
            let val = (h as f64 / u64::MAX as f64) as f32;
            img.put_pixel(x, y, Rgba([val, val, val, 1.0]));
        }
    }
    img
}

/// Constructs a valid synthetic baseline `TerrainTheme` for validation testing.
fn sample_theme() -> TerrainTheme {
    TerrainTheme {
        slug: "test_theme".to_string(),
        display_name: "Test Theme".to_string(),
        texture_size: 1024,
        layers: ThemeLayers {
            grass: ThemeLayer {
                name: "moss".to_string(),
                source: "source_art/moss.png".to_string(),
                tiling: TilingMode::MinCutBlend,
                metallic: 0.0,
                roughness_range: (0.70, 0.95),
                normal_strength: 2.0,
                prompt: "moss prompt".to_string(),
            },
            metal_plate: ThemeLayer {
                name: "metal".to_string(),
                source: "source_art/metal.png".to_string(),
                tiling: TilingMode::MinCutBlend,
                metallic: 0.65,
                roughness_range: (0.30, 0.60),
                normal_strength: 3.0,
                prompt: "metal prompt".to_string(),
            },
            dirt: ThemeLayer {
                name: "dirt".to_string(),
                source: "source_art/dirt.png".to_string(),
                tiling: TilingMode::MinCutBlend,
                metallic: 0.0,
                roughness_range: (0.80, 1.00),
                normal_strength: 2.5,
                prompt: "dirt prompt".to_string(),
            },
            cliff: ThemeLayer {
                name: "cliff".to_string(),
                source: "source_art/cliff.png".to_string(),
                tiling: TilingMode::MinCutBlend,
                metallic: 0.05,
                roughness_range: (0.55, 0.90),
                normal_strength: 4.0,
                prompt: "cliff prompt".to_string(),
            },
        },
        backdrop: ThemeBackdrop {
            source: "source_art/void.png".to_string(),
            prompt: "void prompt".to_string(),
        },
        baked_dir: "textures/terrain/test_theme".to_string(),
        ground_tiles_per_unit: 0.25,
        cliff_tiles_per_unit: 0.20,
        lighting: ThemeLighting {
            sun_illuminance_lux: 14000.0,
            sun_elevation_deg: 48.0,
            sun_azimuth_deg: 225.0,
            ambient_color_srgb: (0.65, 0.78, 1.0),
            ambient_brightness: 350.0,
            void_color_srgb: (0.01, 0.012, 0.03),
            guardrail_emissive_srgb: (0.15, 0.85, 1.0),
            guardrail_emissive_intensity: 4.0,
        },
    }
}

#[test]
fn srgb_roundtrip_exact_for_u8() {
    for i in 0..=255u8 {
        let normalized = f32::from(i) / 255.0;
        let linear = srgb_to_linear(normalized);
        let srgb = linear_to_srgb(linear);
        let reconstructed = (srgb * 255.0).round() as u8;
        assert_eq!(
            i, reconstructed,
            "sRGB piecewise roundtrip failed for integer {i}"
        );
    }
}

#[test]
fn make_tileable_removes_seam() {
    let size = 256u32;
    let mut grad_img = Rgba32FImage::new(size, size);

    // Horizontal linear ramp from 0.0 to 1.0 creates a maximal step across x=0 and x=size-1
    for y in 0..size {
        for x in 0..size {
            let val = x as f32 / (size - 1) as f32;
            grad_img.put_pixel(x, y, Rgba([val, val, val, 1.0]));
        }
    }

    let before_score = seam_score(&grad_img);
    assert!(
        before_score > 20.0,
        "Expected high seam discontinuity score before tiling, found {before_score}"
    );

    let tiled = make_tileable(&grad_img);
    let after_score = seam_score(&tiled);

    println!("make_tileable_removes_seam: before = {before_score:.2}, after = {after_score:.2}");
    assert!(
        after_score < 1.5,
        "Expected seam score < 1.5 after separable two-pass Laplacian pyramid tiling, found {after_score}"
    );
}

#[test]
fn make_tileable_preserves_interior_detail() {
    let size = 256u32;
    let input = deterministic_value_noise(size, size, 8);
    let tiled = make_tileable(&input);
    let half = size / 2;

    // 1. Quarter points region: (64 ± 16, 64 ± 16) = [48..=80]
    // Here both mx and my are 0.0, so the two-pass algorithm reconstructs
    // input shifted by (size/2, size/2) = (128, 128).
    let mut sum_err_quarter = 0.0f32;
    let mut count_quarter = 0;
    for y in 48..=80 {
        for x in 48..=80 {
            let expected = input.get_pixel((x + half) % size, (y + half) % size)[0];
            let actual = tiled.get_pixel(x, y)[0];
            sum_err_quarter += (expected - actual).abs();
            count_quarter += 1;
        }
    }
    let mae_quarter = sum_err_quarter / count_quarter as f32;
    println!("make_tileable_preserves_interior_detail: quarter MAE = {mae_quarter:.6}");
    assert!(
        mae_quarter < 0.02,
        "Quarter region must match input shifted by (128, 128): MAE {mae_quarter} >= 0.02"
    );

    // 2. Centre region: (128 ± 8, 128 ± 8) = [120..=136]
    // Here both mx and my are 1.0, so the two-pass algorithm reconstructs input directly.
    let mut sum_err_centre = 0.0f32;
    let mut count_centre = 0;
    for y in 120..=136 {
        for x in 120..=136 {
            let expected = input.get_pixel(x, y)[0];
            let actual = tiled.get_pixel(x, y)[0];
            sum_err_centre += (expected - actual).abs();
            count_centre += 1;
        }
    }
    let mae_centre = sum_err_centre / count_centre as f32;
    println!("make_tileable_preserves_interior_detail: centre MAE = {mae_centre:.6}");
    assert!(
        mae_centre < 0.02,
        "Centre region must match input directly: MAE {mae_centre} >= 0.02"
    );
}

#[test]
fn make_tileable_has_no_internal_seam() {
    let size = 256u32;
    let input = deterministic_seeded_noise(size, size);
    let tiled = make_tileable(&input);

    let pixel_diff = |p1: &Rgba<f32>, p2: &Rgba<f32>| -> f32 {
        ((p1[0] - p2[0]).abs() + (p1[1] - p2[1]).abs() + (p1[2] - p2[2]).abs()) / 3.0
    };

    // Check input noise statistics first
    let mut in_col_steps = Vec::with_capacity(size as usize);
    for c in 0..size {
        let prev_x = if c == 0 { size - 1 } else { c - 1 };
        let mut sum_step = 0.0f32;
        for y in 0..size {
            sum_step += pixel_diff(input.get_pixel(prev_x, y), input.get_pixel(c, y));
        }
        in_col_steps.push(sum_step / size as f32);
    }
    let max_in_col = in_col_steps.iter().copied().fold(0.0f32, f32::max);
    let mean_in_col = in_col_steps.iter().sum::<f32>() / size as f32;
    let ratio_in_col = max_in_col / mean_in_col;
    println!(
        "INPUT noise col ratio: {ratio_in_col:.4} (max: {max_in_col:.6}, mean: {mean_in_col:.6})"
    );

    let mut col_steps = Vec::with_capacity(size as usize);
    for c in 0..size {
        let prev_x = if c == 0 { size - 1 } else { c - 1 };
        let mut sum_step = 0.0f32;
        for y in 0..size {
            sum_step += pixel_diff(tiled.get_pixel(prev_x, y), tiled.get_pixel(c, y));
        }
        col_steps.push(sum_step / size as f32);
    }
    let max_col = col_steps.iter().copied().fold(0.0f32, f32::max);
    let mean_col = col_steps.iter().sum::<f32>() / size as f32;
    let ratio_col = max_col / mean_col;

    let max_col_idx = col_steps
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).expect("cmp"))
        .expect("max")
        .0;
    println!(
        "make_tileable_has_no_internal_seam: col ratio = {ratio_col:.4} (max: {max_col:.6} at c={max_col_idx}, mean: {mean_col:.6})"
    );
    assert!(
        ratio_col < 1.5,
        "Column step ratio max/mean {ratio_col:.4} must be < 1.5 (detects residual column seam)"
    );

    // Rows: for every boundary r in 0..size (r=0 is wrap boundary size-1 -> 0)
    let mut row_steps = Vec::with_capacity(size as usize);
    for r in 0..size {
        let prev_y = if r == 0 { size - 1 } else { r - 1 };
        let mut sum_step = 0.0f32;
        for x in 0..size {
            sum_step += pixel_diff(tiled.get_pixel(x, prev_y), tiled.get_pixel(x, r));
        }
        row_steps.push(sum_step / size as f32);
    }
    let max_row = row_steps.iter().copied().fold(0.0f32, f32::max);
    let mean_row = row_steps.iter().sum::<f32>() / size as f32;
    let ratio_row = max_row / mean_row;
    println!(
        "make_tileable_has_no_internal_seam: row ratio = {ratio_row:.4} (max: {max_row:.6}, mean: {mean_row:.6})"
    );
    assert!(
        ratio_row < 1.5,
        "Row step ratio max/mean {ratio_row:.4} must be < 1.5 (detects residual row seam)"
    );
}

#[test]
fn normal_of_flat_height_is_up() {
    let size = 256u32;
    let flat = GrayF32Image::from_pixel(size, size, Luma([0.5f32]));
    let normal_map = derive_normal(&flat, 2.0);

    for y in 0..size {
        for x in 0..size {
            let pixel = normal_map.get_pixel(x, y);
            assert_eq!(
                *pixel,
                Rgba([128, 128, 255, 255]),
                "Flat surface must produce pure outward normal (128, 128, 255, 255) at ({x}, {y})"
            );
        }
    }
}

#[test]
fn normal_of_x_ramp_tilts_negative_x() {
    let size = 256u32;
    let mut ramp = GrayF32Image::new(size, size);

    for y in 0..size {
        for x in 0..size {
            ramp.put_pixel(x, y, Luma([x as f32 / size as f32]));
        }
    }

    let normal_map = derive_normal(&ramp, 2.0);

    // Sample an interior pixel well away from boundary wrap points
    let p = normal_map.get_pixel(128, 128);
    let nx = (f32::from(p[0]) / 255.0 - 0.5) * 2.0;
    let ny = (f32::from(p[1]) / 255.0 - 0.5) * 2.0;
    let nz = (f32::from(p[2]) / 255.0 - 0.5) * 2.0;

    println!("normal_of_x_ramp_tilts_negative_x: nx = {nx:.6}, ny = {ny:.6}, nz = {nz:.6}");
    assert!(
        nx < 0.0,
        "Height rising in +X direction must tilt normal in -X direction, found nx = {nx}"
    );
    assert!(
        ny.abs() < 0.01,
        "Height uniform along Y direction must have ny ≈ 0.0, found ny = {ny}"
    );
    assert!(
        nz > 0.0,
        "Normal must point upwards along +Z tangent axis, found nz = {nz}"
    );
}

#[test]
fn derive_height_normalized_and_smooth() {
    let size = 128u32;
    let mut albedo = Rgba32FImage::new(size, size);

    for y in 0..size {
        for x in 0..size {
            let r = if (x + y) % 16 < 8 { 0.2f32 } else { 0.8f32 };
            let g = if x < 64 { 0.1f32 } else { 0.9f32 };
            let b = if y < 64 { 0.3f32 } else { 0.7f32 };
            albedo.put_pixel(x, y, Rgba([r, g, b, 1.0]));
        }
    }

    let height = derive_height(&albedo);
    assert_eq!(height.width(), size);
    assert_eq!(height.height(), size);

    let mut min_val = f32::INFINITY;
    let mut max_val = f32::NEG_INFINITY;
    for p in height.pixels() {
        let v = p[0];
        if v < min_val {
            min_val = v;
        }
        if v > max_val {
            max_val = v;
        }
    }

    assert!(
        (0.0..=0.01).contains(&min_val),
        "Height field minimum must normalize to ~0.0, found {min_val}"
    );
    assert!(
        (0.99..=1.0).contains(&max_val),
        "Height field maximum must normalize to ~1.0, found {max_val}"
    );
}

#[test]
fn orm_ranges_respected() {
    let size = 128u32;
    let mut height = GrayF32Image::new(size, size);

    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 / 16.0;
            let fy = y as f32 / 16.0;
            let val = ((fx.sin() + fy.cos()) * 0.5 + 0.5).clamp(0.0, 1.0);
            height.put_pixel(x, y, Luma([val]));
        }
    }

    let layer = ThemeLayer {
        name: "test_layer".to_string(),
        source: "dummy.png".to_string(),
        tiling: TilingMode::MinCutBlend,
        metallic: 0.65,
        roughness_range: (0.30, 0.70),
        normal_strength: 3.0,
        prompt: "test".to_string(),
    };

    let orm = derive_orm(&height, &layer);
    let expected_metallic_u8 = (0.65f32 * 255.0).round() as u8;

    for y in 0..size {
        for x in 0..size {
            let p = orm.get_pixel(x, y);

            // Channel R: Ambient Occlusion in [0.25, 1.0]
            let ao = f32::from(p[0]) / 255.0;
            assert!(
                (0.24..=1.01).contains(&ao),
                "AO channel {ao} outside [0.25, 1.0] at ({x}, {y})"
            );

            // Channel G: Roughness in [0.30, 0.70]
            let roughness = f32::from(p[1]) / 255.0;
            assert!(
                (0.29..=0.71).contains(&roughness),
                "Roughness channel {roughness} outside [0.30, 0.70] at ({x}, {y})"
            );

            // Channel B: Metallic constant
            assert_eq!(
                p[2], expected_metallic_u8,
                "Metallic channel must equal constant {expected_metallic_u8}"
            );

            // Channel A: Opaque 255
            assert_eq!(p[3], 255, "Alpha channel must be 255");
        }
    }
}

#[test]
fn theme_catalog_real_file_validates() {
    let candidates = [
        PathBuf::from("assets/catalog/terrain_themes.ron"),
        PathBuf::from("../../assets/catalog/terrain_themes.ron"),
        PathBuf::from("../assets/catalog/terrain_themes.ron"),
    ];
    let path = candidates
        .into_iter()
        .find(|p| p.exists())
        .expect("assets/catalog/terrain_themes.ron must exist on disk");

    let catalog =
        TerrainThemeCatalog::load(&path).expect("Should load production terrain_themes.ron");

    assert!(
        !catalog.themes.is_empty(),
        "Catalog must contain at least one theme"
    );

    let foundry = catalog
        .get("orbital_foundry")
        .expect("orbital_foundry theme must be present in catalog");

    foundry
        .validate()
        .expect("orbital_foundry must pass semantic validation");
    assert_eq!(foundry.texture_size, 1024);
    assert_eq!(foundry.layers.grass.name, "lumen_moss");
    assert_eq!(foundry.layers.metal_plate.name, "deck_plating");
    assert_eq!(foundry.layers.dirt.name, "regolith");
    assert_eq!(foundry.layers.cliff.name, "basalt_wall");
}

#[test]
fn theme_validation_errors() {
    // 1. Non-power of two texture_size
    let mut theme = sample_theme();
    theme.texture_size = 1000;
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 2. Out of bounds texture_size (< 256)
    let mut theme = sample_theme();
    theme.texture_size = 128;
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 3. Out of bounds texture_size (> 2048)
    let mut theme = sample_theme();
    theme.texture_size = 4096;
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 4. Reversed roughness range
    let mut theme = sample_theme();
    theme.layers.grass.roughness_range = (0.80, 0.20);
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 5. Out of bounds roughness (> 1.0)
    let mut theme = sample_theme();
    theme.layers.dirt.roughness_range = (0.50, 1.20);
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 6. Out of bounds metallic (< 0.0)
    let mut theme = sample_theme();
    theme.layers.metal_plate.metallic = -0.1;
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 7. Non-positive normal strength
    let mut theme = sample_theme();
    theme.layers.cliff.normal_strength = 0.0;
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 8. Non-positive ground_tiles_per_unit
    let mut theme = sample_theme();
    theme.ground_tiles_per_unit = 0.0;
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 9. Non-positive cliff_tiles_per_unit
    let mut theme = sample_theme();
    theme.cliff_tiles_per_unit = -1.0;
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 10. Duplicate layer names
    let mut theme = sample_theme();
    theme.layers.grass.name = "duplicate_name".to_string();
    theme.layers.dirt.name = "duplicate_name".to_string();
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 11. Empty slug
    let mut theme = sample_theme();
    theme.slug = "   ".to_string();
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));
}

#[test]
fn theme_validation_max_seam_score_range() {
    // 1. Non-positive max_seam_score (<= 0.0)
    let mut theme = sample_theme();
    theme.layers.metal_plate.tiling = TilingMode::SourceSeamless {
        max_seam_score: 0.0,
    };
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    let mut theme = sample_theme();
    theme.layers.metal_plate.tiling = TilingMode::SourceSeamless {
        max_seam_score: -0.5,
    };
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 2. Out of bounds max_seam_score (> 3.0)
    let mut theme = sample_theme();
    theme.layers.metal_plate.tiling = TilingMode::SourceSeamless {
        max_seam_score: 3.05,
    };
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 3. Non-finite max_seam_score
    let mut theme = sample_theme();
    theme.layers.metal_plate.tiling = TilingMode::SourceSeamless {
        max_seam_score: f32::NAN,
    };
    assert!(matches!(
        theme.validate(),
        Err(TerrainThemeError::Invalid { .. })
    ));

    // 4. Valid max_seam_score in (0.0, 3.0]
    let mut theme = sample_theme();
    theme.layers.metal_plate.tiling = TilingMode::SourceSeamless {
        max_seam_score: 2.0,
    };
    assert!(theme.validate().is_ok());

    let mut theme = sample_theme();
    theme.layers.metal_plate.tiling = TilingMode::SourceSeamless {
        max_seam_score: 3.0,
    };
    assert!(theme.validate().is_ok());
}

#[test]
fn ordered_layers_and_layer_index_constants() {
    let theme = sample_theme();
    let ordered = theme.ordered_layers();

    assert_eq!(ordered[LAYER_INDEX_GRASS].name, theme.layers.grass.name);
    assert_eq!(
        ordered[LAYER_INDEX_METAL_PLATE].name,
        theme.layers.metal_plate.name
    );
    assert_eq!(ordered[LAYER_INDEX_DIRT].name, theme.layers.dirt.name);
    assert_eq!(ordered[LAYER_INDEX_CLIFF].name, theme.layers.cliff.name);
}

#[test]
fn tiled_preview_creates_quadrants() {
    let size = 64u32;
    let mut albedo = Rgba8Image::new(size, size);
    for y in 0..size {
        for x in 0..size {
            albedo.put_pixel(x, y, Rgba([x as u8, y as u8, 42, 255]));
        }
    }

    let preview = tiled_preview(&albedo);
    assert_eq!(preview.width(), size * 2);
    assert_eq!(preview.height(), size * 2);

    for y in 0..size {
        for x in 0..size {
            let orig = albedo.get_pixel(x, y);
            assert_eq!(preview.get_pixel(x, y), orig);
            assert_eq!(preview.get_pixel(x + size, y), orig);
            assert_eq!(preview.get_pixel(x, y + size), orig);
            assert_eq!(preview.get_pixel(x + size, y + size), orig);
        }
    }
}

#[test]
fn bake_theme_fails_loudly_on_missing_source() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let catalog_path = temp_dir.path().join("terrain_themes.ron");

    let theme = sample_theme();
    let catalog = TerrainThemeCatalog {
        themes: vec![theme],
    };
    let ron_str = ron::to_string(&catalog).expect("serialize catalog");
    std::fs::write(&catalog_path, ron_str).expect("write catalog");

    let res = bake_theme(&catalog_path, "test_theme", temp_dir.path());
    match res {
        Err(TerrainThemeError::MissingSource(path)) => {
            assert!(
                path.to_string_lossy().contains("source_art"),
                "Expected missing source error pointing to source_art, got {path:?}"
            );
        }
        other => panic!("Expected MissingSource error, got {other:?}"),
    }
}

#[test]
fn load_and_resize_rejects_non_square_source() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let non_square_path = temp_dir.path().join("rectangular.png");

    // Create an image with 10% aspect ratio divergence (100x110)
    let non_square = Rgba8Image::new(100, 110);
    non_square
        .save(&non_square_path)
        .expect("save non-square image");

    let res = load_and_resize(&non_square_path, 256);
    match res {
        Err(TerrainThemeError::Invalid { reason, .. }) => {
            assert!(
                reason.contains("not square"),
                "Expected aspect ratio error message, got '{reason}'"
            );
        }
        other => panic!("Expected Invalid error for non-square image, got {other:?}"),
    }
}

#[test]
fn synthetic_bake_theme_end_to_end() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let assets_dir = temp_dir.path().to_path_buf();

    // 1. Generate 5 synthetic source images: 4 layers + 1 backdrop
    let source_dir = assets_dir.join("source_art").join("test_theme");
    std::fs::create_dir_all(&source_dir).expect("create source_dir");

    let filenames = ["moss.png", "metal.png", "dirt.png", "cliff.png", "void.png"];
    for &fname in &filenames {
        let p = source_dir.join(fname);
        let mut img = Rgba8Image::new(256, 256);
        for y in 0..256 {
            for x in 0..256 {
                let r = ((x * 3) % 256) as u8;
                let g = ((y * 5) % 256) as u8;
                let b = (((x + y) * 2) % 256) as u8;
                img.put_pixel(x, y, Rgba([r, g, b, 255]));
            }
        }
        img.save(&p).expect("save synthetic source image");
    }

    let mut theme = sample_theme();
    theme.texture_size = 256;
    theme.layers.grass.source = "source_art/test_theme/moss.png".to_string();
    theme.layers.metal_plate.source = "source_art/test_theme/metal.png".to_string();
    theme.layers.dirt.source = "source_art/test_theme/dirt.png".to_string();
    theme.layers.cliff.source = "source_art/test_theme/cliff.png".to_string();
    theme.backdrop.source = "source_art/test_theme/void.png".to_string();

    let catalog_path = assets_dir.join("terrain_themes.ron");
    let catalog = TerrainThemeCatalog {
        themes: vec![theme],
    };
    std::fs::write(&catalog_path, ron::to_string(&catalog).expect("ron")).expect("write catalog");

    let report =
        bake_theme(&catalog_path, "test_theme", &assets_dir).expect("bake_theme must succeed");

    assert_eq!(report.theme_slug, "test_theme");
    assert_eq!(report.layers.len(), 4);

    let baked_out_dir = assets_dir.join("textures/terrain/test_theme");
    for layer_rep in &report.layers {
        let albedo = assets_dir.join(&layer_rep.paths.albedo);
        let normal = assets_dir.join(&layer_rep.paths.normal);
        let orm = assets_dir.join(&layer_rep.paths.orm);
        let preview = assets_dir.join(&layer_rep.paths.tiled_preview);

        assert!(albedo.exists(), "Albedo map missing: {albedo:?}");
        assert!(normal.exists(), "Normal map missing: {normal:?}");
        assert!(orm.exists(), "ORM map missing: {orm:?}");
        assert!(preview.exists(), "Tiled preview missing: {preview:?}");

        // Validate image dimensions
        let loaded_albedo = image::open(&albedo).expect("open albedo");
        assert_eq!(loaded_albedo.width(), 256);
        assert_eq!(loaded_albedo.height(), 256);

        let loaded_preview = image::open(&preview).expect("open preview");
        assert_eq!(loaded_preview.width(), 512);
        assert_eq!(loaded_preview.height(), 512);

        // Seam score should be well controlled after tiling
        assert!(
            layer_rep.seam_score_after <= layer_rep.seam_score_before
                || layer_rep.seam_score_after < 2.0,
            "Seam score should be well controlled after tiling"
        );
    }

    let backdrop_file = baked_out_dir.join("void_nebula.png");
    assert!(backdrop_file.exists(), "Backdrop void_nebula.png missing");
}

#[test]
fn min_cut_follows_zero_cost_corridor() {
    let size = 256u32;
    let mut a = Rgba32FImage::new(size, size);
    let mut b = Rgba32FImage::new(size, size);

    // Corridor strictly within [96, 120]: c = 108, amplitude = 8 -> x in [100, 116]
    let c = 108i32;
    let mut corridor = Vec::with_capacity(size as usize);

    for y in 0..size {
        let phase = 2.0 * std::f32::consts::PI * (y as f32) / (size as f32);
        let offset = (8.0 * phase.sin()).round() as i32;
        let corr_x = (c + offset) as u32;
        corridor.push(corr_x);

        for x in 0..size {
            if x == corr_x {
                // Identical along the corridor -> zero photometric error
                a.put_pixel(x, y, Rgba([0.5, 0.5, 0.5, 1.0]));
                b.put_pixel(x, y, Rgba([0.5, 0.5, 0.5, 1.0]));
            } else {
                // High contrast elsewhere -> error > 0
                a.put_pixel(x, y, Rgba([0.1, 0.1, 0.1, 1.0]));
                b.put_pixel(x, y, Rgba([0.9, 0.9, 0.9, 1.0]));
            }
        }
    }

    let mut buffers = MinCutBuffers::new();
    let cut = find_vertical_min_cut(&a, &b, 96, 120, &mut buffers);

    assert_eq!(cut.len(), size as usize);
    for y in 0..size {
        let expected = corridor[y as usize];
        let actual = cut[y as usize];
        assert_eq!(
            actual, expected,
            "Row {y} cut must match zero-cost corridor exactly: expected {expected}, got {actual}"
        );
    }
}

#[test]
fn min_cut_is_cyclic() {
    let size = 256u32;
    let a = deterministic_seeded_noise(size, size);
    let b = deterministic_value_noise(size, size, 8);
    let mut buffers = MinCutBuffers::new();

    // 1. Vertical cut
    let cut_v = find_vertical_min_cut(&a, &b, 96, 120, &mut buffers);
    assert_eq!(cut_v.len(), size as usize);
    let diff_v = (cut_v[0] as i32 - cut_v[size as usize - 1] as i32).abs();
    println!(
        "min_cut_is_cyclic vertical diff: {diff_v} (cut[0]={}, cut[N-1]={})",
        cut_v[0],
        cut_v[size as usize - 1]
    );
    assert!(
        diff_v <= 1,
        "Vertical cut endpoints must be cyclic neighbors (diff <= 1), found {diff_v}"
    );

    // 2. Horizontal cut
    let cut_h = find_horizontal_min_cut(&a, &b, 96, 120, &mut buffers);
    assert_eq!(cut_h.len(), size as usize);
    let diff_h = (cut_h[0] as i32 - cut_h[size as usize - 1] as i32).abs();
    println!(
        "min_cut_is_cyclic horizontal diff: {diff_h} (cut[0]={}, cut[N-1]={})",
        cut_h[0],
        cut_h[size as usize - 1]
    );
    assert!(
        diff_h <= 1,
        "Horizontal cut endpoints must be cyclic neighbors (diff <= 1), found {diff_h}"
    );
}

#[test]
fn make_tileable_structured_no_ghosting() {
    let size = 256u32;
    let mut panel_img = Rgba32FImage::from_pixel(size, size, Rgba([0.05, 0.05, 0.05, 1.0]));

    // Generate synthetic "panel" image: random axis-aligned rectangles with 2-px dark gaps, seeded.
    let mut rects = vec![(0u32, 0u32, size, size)];
    let mut rng_state = 0x1234_5678_9ABC_DEF0u64;
    let mut next_rand = || -> u64 {
        rng_state = rng_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        rng_state ^ (rng_state >> 32)
    };

    let mut i = 0;
    while i < rects.len() {
        let (rx, ry, rw, rh) = rects[i];
        if rw >= 64 && (rh < 64 || (next_rand() % 2 == 0)) {
            let split = 24 + ((next_rand() as u32) % (rw - 48 + 1));
            rects[i] = (rx, ry, split, rh);
            rects.push((rx + split, ry, rw - split, rh));
        } else if rh >= 64 {
            let split = 24 + ((next_rand() as u32) % (rh - 48 + 1));
            rects[i] = (rx, ry, rw, split);
            rects.push((rx, ry + split, rw, rh - split));
        } else {
            i += 1;
        }
    }

    for (idx, &(rx, ry, rw, rh)) in rects.iter().enumerate() {
        let lum = 0.35 + (((idx * 137 + 42) % 100) as f32 / 100.0) * 0.55;
        for py in (ry + 1)..(ry + rh.saturating_sub(1)) {
            for px in (rx + 1)..(rx + rw.saturating_sub(1)) {
                panel_img.put_pixel(px, py, Rgba([lum, lum, lum, 1.0]));
            }
        }
    }

    let tiled = make_tileable(&panel_img);
    let half = size / 2;

    // Transition bands: [96, 120] and [136, 160]
    let in_band = |v: u32| (96..=120).contains(&v) || (136..=160).contains(&v);

    let mut total_band_pixels = 0usize;
    let mut ghost_pixels = 0usize;

    for y in 0..size {
        for x in 0..size {
            if in_band(x) || in_band(y) {
                total_band_pixels += 1;

                let out_val = tiled.get_pixel(x, y)[0];

                let s_img = panel_img.get_pixel(x, y)[0];
                let s_shift_x = panel_img.get_pixel((x + half) % size, y)[0];
                let s_shift_y = panel_img.get_pixel(x, (y + half) % size)[0];
                let s_shift_xy = panel_img.get_pixel((x + half) % size, (y + half) % size)[0];

                let d0 = (out_val - s_img).abs();
                let d1 = (out_val - s_shift_x).abs();
                let d2 = (out_val - s_shift_y).abs();
                let d3 = (out_val - s_shift_xy).abs();

                let min_diff = d0.min(d1).min(d2).min(d3);
                if min_diff > 0.1 {
                    ghost_pixels += 1;
                }
            }
        }
    }

    let ghost_fraction = ghost_pixels as f32 / total_band_pixels as f32;
    println!(
        "make_tileable_structured_no_ghosting: ghost fraction = {ghost_fraction:.4} ({ghost_pixels}/{total_band_pixels} pixels)"
    );
    assert!(
        ghost_fraction < 0.02,
        "Ghost pixel fraction {ghost_fraction:.4} must be < 2% (0.02)"
    );
}

#[test]
fn source_seamless_layer_is_passed_through() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let assets_dir = temp_dir.path().to_path_buf();
    let source_dir = assets_dir.join("source_art").join("test_theme");
    std::fs::create_dir_all(&source_dir).expect("create source_dir");

    let size = 256u32;

    // Create synthetic periodic image for metal_plate: continuous boundary values
    let mut periodic_img = Rgba8Image::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let fx = (x as f32 / size as f32) * 2.0 * std::f32::consts::PI;
            let fy = (y as f32 / size as f32) * 2.0 * std::f32::consts::PI;
            let r = ((fx.sin() * 0.5 + 0.5) * 255.0).round() as u8;
            let g = ((fy.cos() * 0.5 + 0.5) * 255.0).round() as u8;
            let b = (((fx + fy).sin() * 0.5 + 0.5) * 255.0).round() as u8;
            periodic_img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    periodic_img
        .save(source_dir.join("metal.png"))
        .expect("save metal.png");

    // Dummy images for other layers and backdrop
    for &fname in &["moss.png", "dirt.png", "cliff.png", "void.png"] {
        let dummy = Rgba8Image::from_pixel(size, size, Rgba([128, 128, 128, 255]));
        dummy.save(source_dir.join(fname)).expect("save dummy");
    }

    let mut theme = sample_theme();
    theme.texture_size = size;
    theme.layers.grass.source = "source_art/test_theme/moss.png".to_string();
    theme.layers.metal_plate.source = "source_art/test_theme/metal.png".to_string();
    theme.layers.metal_plate.tiling = TilingMode::SourceSeamless {
        max_seam_score: 2.0,
    };
    theme.layers.dirt.source = "source_art/test_theme/dirt.png".to_string();
    theme.layers.cliff.source = "source_art/test_theme/cliff.png".to_string();
    theme.backdrop.source = "source_art/test_theme/void.png".to_string();

    let catalog = TerrainThemeCatalog {
        themes: vec![theme],
    };
    let catalog_path = temp_dir.path().join("terrain_themes.ron");
    let ron_str = ron::to_string(&catalog).expect("serialize catalog");
    std::fs::write(&catalog_path, ron_str).expect("write catalog");

    let report = bake_theme(&catalog_path, "test_theme", &assets_dir).expect("bake theme");

    let metal_report = &report.layers[LAYER_INDEX_METAL_PLATE];
    assert_eq!(
        metal_report.tiling_mode,
        TilingMode::SourceSeamless {
            max_seam_score: 2.0
        }
    );
    assert_eq!(
        metal_report.seam_score_before,
        metal_report.seam_score_after
    );

    // Read the baked albedo PNG back from disk and verify bit-for-bit exact identity
    let baked_albedo = image::open(assets_dir.join(&metal_report.paths.albedo))
        .expect("open baked albedo")
        .to_rgba8();

    assert_eq!(baked_albedo.dimensions(), (size, size));
    for y in 0..size {
        for x in 0..size {
            let expected = periodic_img.get_pixel(x, y);
            let actual = baked_albedo.get_pixel(x, y);
            assert_eq!(
                actual, expected,
                "Bit-exact mismatch at ({x}, {y}): baked {actual:?}, expected {expected:?}"
            );
        }
    }
}

#[test]
fn source_seamless_rejects_seamed_source() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let assets_dir = temp_dir.path().to_path_buf();
    let source_dir = assets_dir.join("source_art").join("test_theme");
    std::fs::create_dir_all(&source_dir).expect("create source_dir");

    let size = 256u32;

    // Discontinuous horizontal gradient (maximal wrap boundary discontinuity: seam_score > 20)
    let mut gradient_img = Rgba8Image::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let val = ((x as f32 / size as f32) * 255.0).round() as u8;
            gradient_img.put_pixel(x, y, Rgba([val, val, val, 255]));
        }
    }
    gradient_img
        .save(source_dir.join("metal.png"))
        .expect("save metal.png");

    for &fname in &["moss.png", "dirt.png", "cliff.png", "void.png"] {
        let dummy = Rgba8Image::from_pixel(size, size, Rgba([128, 128, 128, 255]));
        dummy.save(source_dir.join(fname)).expect("save dummy");
    }

    let mut theme = sample_theme();
    theme.texture_size = size;
    theme.layers.grass.source = "source_art/test_theme/moss.png".to_string();
    theme.layers.metal_plate.source = "source_art/test_theme/metal.png".to_string();
    theme.layers.metal_plate.tiling = TilingMode::SourceSeamless {
        max_seam_score: 2.0,
    };
    theme.layers.dirt.source = "source_art/test_theme/dirt.png".to_string();
    theme.layers.cliff.source = "source_art/test_theme/cliff.png".to_string();
    theme.backdrop.source = "source_art/test_theme/void.png".to_string();

    let catalog = TerrainThemeCatalog {
        themes: vec![theme],
    };
    let catalog_path = temp_dir.path().join("terrain_themes.ron");
    let ron_str = ron::to_string(&catalog).expect("serialize catalog");
    std::fs::write(&catalog_path, ron_str).expect("write catalog");

    let res = bake_theme(&catalog_path, "test_theme", &assets_dir);
    match res {
        Err(TerrainThemeError::SourceNotSeamless { layer, score, max }) => {
            assert_eq!(layer, "metal");
            assert!(
                score > 2.0,
                "Expected measured seam score to exceed 2.0, got {score}"
            );
            assert!(
                (max - 2.0).abs() < 1e-4,
                "Expected max threshold 2.0, got {max}"
            );
        }
        other => panic!("Expected SourceNotSeamless error, got {other:?}"),
    }
}
