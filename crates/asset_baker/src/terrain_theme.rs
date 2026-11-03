//! Terrain theme catalog definitions, validation, and file path resolutions.
//!
//! # Architectural Charter
//! In accordance with `AGENTS.md` and
//! Section 4.1 of `docs/plans/21-plan-map-editor.md`, `terrain_theme` provides the declarative
//! schema for map visual themes. It specifies the 4 primary surface texture layers
//! (grass, metal plate, dirt, cliff), the space void/backdrop, UV tiling densities, and
//! PBR scene lighting parameters.
//!
//! # Invariants
//! - Texture sizes must be powers of two in the range `[256, 2048]`.
//! - Layer roughness ranges must be strictly ordered with `0.0 <= min <= max <= 1.0`.
//! - Metallic parameters must reside within `[0.0, 1.0]`.
//! - Normal strengths and tiling frequencies must be strictly positive.
//! - All 4 texture layer names within a theme must be unique to avoid file collision.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

/// Minimum permitted texture dimension in pixels for baked terrain maps.
pub const MIN_THEME_TEXTURE_SIZE: u32 = 256;

/// Maximum permitted texture dimension in pixels for baked terrain maps.
pub const MAX_THEME_TEXTURE_SIZE: u32 = 2048;

/// Texture array layer index 0: grass / biological floor (e.g. `lumen_moss`).
pub const LAYER_INDEX_GRASS: usize = 0;

/// Texture array layer index 1: metal plate / constructed floor (e.g. `deck_plating`).
pub const LAYER_INDEX_METAL_PLATE: usize = 1;

/// Texture array layer index 2: dirt / regolith ground (e.g. `regolith`).
pub const LAYER_INDEX_DIRT: usize = 2;

/// Texture array layer index 3: vertical cliff wall (e.g. `basalt_wall`).
pub const LAYER_INDEX_CLIFF: usize = 3;

/// Errors encountered during theme catalog loading, validation, and path resolution.
#[derive(Debug, thiserror::Error)]
pub enum TerrainThemeError {
    /// Filesystem access failure.
    #[error("I/O error accessing {path:?}: {source}")]
    Io {
        /// Path that encountered the I/O error.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// RON deserialization failure.
    #[error("Failed to parse terrain theme catalog RON: {0}")]
    Ron(String),

    /// Requested theme slug not found in the catalog.
    #[error("Unknown terrain theme slug: '{0}'")]
    UnknownTheme(String),

    /// The terrain sampler override holds something other than a clamp from 1 to 16.
    #[error("Terrain anisotropy override must be an integer from 1 to 16, not '{0}'")]
    InvalidAnisotropyOverride(String),

    /// Theme specification failed structural or mathematical validation.
    #[error("Terrain theme '{theme}' is invalid: {reason}")]
    Invalid {
        /// Slug of the invalid theme.
        theme: String,
        /// Explanation of why validation failed.
        reason: String,
    },

    /// Expected input source art image was not found on disk.
    #[error("Missing source art image: {0:?}")]
    MissingSource(PathBuf),

    /// A layer configured with `SourceSeamless` exceeded its maximum permitted seam score.
    #[error("Source image for layer '{layer}' is not seamless: measured seam score {score:.2} exceeds max allowed {max:.2}")]
    SourceNotSeamless {
        /// Name of the failing layer.
        layer: String,
        /// Measured wrap seam score of the source image.
        score: f32,
        /// Maximum permitted seam score threshold.
        max: f32,
    },

    /// Image processing, decoding, or encoding failure.
    #[error("Image processing error: {0}")]
    Image(String),
}

/// Catalog containing all authorable terrain themes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainThemeCatalog {
    /// List of terrain themes registered in the catalog.
    pub themes: Vec<TerrainTheme>,
}

impl TerrainThemeCatalog {
    /// Loads and parses the terrain theme catalog from a RON file on disk.
    ///
    /// # Errors
    /// Returns [`TerrainThemeError::Io`] if the file cannot be read, or
    /// [`TerrainThemeError::Ron`] if deserialization fails.
    pub fn load(path: &Path) -> Result<Self, TerrainThemeError> {
        let content = std::fs::read_to_string(path).map_err(|source| TerrainThemeError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_ron_str(&content)
    }

    /// Deserializes a terrain theme catalog from a RON formatted string.
    ///
    /// # Errors
    /// Returns [`TerrainThemeError::Ron`] if the string does not conform to the schema.
    pub fn from_ron_str(s: &str) -> Result<Self, TerrainThemeError> {
        ron::from_str::<Self>(s).map_err(|err| TerrainThemeError::Ron(err.to_string()))
    }

    /// Finds a theme by its unique slug identifier.
    #[must_use]
    pub fn get(&self, slug: &str) -> Option<&TerrainTheme> {
        self.themes.iter().find(|t| t.slug == slug)
    }
}

/// Complete visual specification of a terrain theme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainTheme {
    /// Unique machine-readable identifier (e.g. `"orbital_foundry"`).
    pub slug: String,
    /// Human-readable display title (e.g. `"Orbital Foundry"`).
    pub display_name: String,
    /// Square resolution in pixels of all baked textures (e.g. 1024).
    pub texture_size: u32,
    /// Texture layers for grass, metal plate, dirt, and cliffs.
    pub layers: ThemeLayers,
    /// Space backdrop / abyss void skybox configuration.
    pub backdrop: ThemeBackdrop,
    /// Relative output directory under `assets/` where baked PNGs are written.
    pub baked_dir: String,
    /// UV tiling frequency for horizontal ground meshes (tiles per world unit).
    pub ground_tiles_per_unit: f32,
    /// UV tiling frequency for vertical cliff wall meshes (tiles per world unit).
    pub cliff_tiles_per_unit: f32,
    /// Directional and ambient lighting settings associated with this theme.
    pub lighting: ThemeLighting,
}

impl TerrainTheme {
    /// Returns the four theme layers in canonical texture array index order:
    /// grass (0), metal plate (1), dirt (2), and cliff (3).
    #[must_use]
    pub fn ordered_layers(&self) -> [&ThemeLayer; 4] {
        [
            &self.layers.grass,
            &self.layers.metal_plate,
            &self.layers.dirt,
            &self.layers.cliff,
        ]
    }

    /// Resolves the standard baked output filenames for a given layer.
    ///
    /// Produces relative paths rooted at [`self.baked_dir`]:
    /// - `<baked_dir>/<name>_albedo.png`
    /// - `<baked_dir>/<name>_normal.png`
    /// - `<baked_dir>/<name>_orm.png`
    /// - `<baked_dir>/<name>_tiled_preview.png`
    #[must_use]
    pub fn baked_paths(&self, layer: &ThemeLayer) -> BakedLayerPaths {
        let dir = PathBuf::from(&self.baked_dir);
        BakedLayerPaths {
            albedo: dir.join(format!("{}_albedo.png", layer.name)),
            normal: dir.join(format!("{}_normal.png", layer.name)),
            orm: dir.join(format!("{}_orm.png", layer.name)),
            tiled_preview: dir.join(format!("{}_tiled_preview.png", layer.name)),
        }
    }

    /// Validates all semantic invariants of the theme.
    ///
    /// # Errors
    /// Returns [`TerrainThemeError::Invalid`] if:
    /// - `texture_size` is not a power of two or is outside `[256, 2048]`.
    /// - `ground_tiles_per_unit` or `cliff_tiles_per_unit` are not strictly positive.
    /// - Layer names are duplicated.
    /// - Any layer has out-of-range metallic `[0.0, 1.0]`, unordered roughness, or non-positive normal strength.
    /// - Required string fields (slug, display_name, baked_dir, source, prompt) are empty.
    pub fn validate(&self) -> Result<(), TerrainThemeError> {
        let invalid = |reason: String| TerrainThemeError::Invalid {
            theme: self.slug.clone(),
            reason,
        };

        if self.slug.trim().is_empty() {
            return Err(invalid("Theme slug must not be empty".to_string()));
        }
        if self.display_name.trim().is_empty() {
            return Err(invalid("Theme display_name must not be empty".to_string()));
        }
        if self.baked_dir.trim().is_empty() {
            return Err(invalid("Theme baked_dir must not be empty".to_string()));
        }

        // Texture size must be power of two in [256, 2048]
        if !self.texture_size.is_power_of_two()
            || self.texture_size < MIN_THEME_TEXTURE_SIZE
            || self.texture_size > MAX_THEME_TEXTURE_SIZE
        {
            return Err(invalid(format!(
                "texture_size must be a power of two between {MIN_THEME_TEXTURE_SIZE} and {MAX_THEME_TEXTURE_SIZE}, found {}",
                self.texture_size
            )));
        }

        // Tiling frequencies must be strictly positive and finite
        if !self.ground_tiles_per_unit.is_finite() || self.ground_tiles_per_unit <= 0.0 {
            return Err(invalid(format!(
                "ground_tiles_per_unit must be strictly positive and finite, found {}",
                self.ground_tiles_per_unit
            )));
        }
        if !self.cliff_tiles_per_unit.is_finite() || self.cliff_tiles_per_unit <= 0.0 {
            return Err(invalid(format!(
                "cliff_tiles_per_unit must be strictly positive and finite, found {}",
                self.cliff_tiles_per_unit
            )));
        }

        // Layer names must be unique
        let mut seen_names = BTreeSet::new();
        for layer in self.ordered_layers() {
            if layer.name.trim().is_empty() {
                return Err(invalid("Layer name must not be empty".to_string()));
            }
            if !seen_names.insert(&layer.name) {
                return Err(invalid(format!(
                    "Duplicate layer name '{}' found in theme",
                    layer.name
                )));
            }
            validate_layer(&self.slug, layer)?;
        }

        // Backdrop source & prompt
        if self.backdrop.source.trim().is_empty() {
            return Err(invalid("Backdrop source must not be empty".to_string()));
        }
        if self.backdrop.prompt.trim().is_empty() {
            return Err(invalid("Backdrop prompt must not be empty".to_string()));
        }

        Ok(())
    }
}

/// Helper function to validate semantic invariants for an individual texture layer.
fn validate_layer(theme: &str, layer: &ThemeLayer) -> Result<(), TerrainThemeError> {
    let invalid = |reason: String| TerrainThemeError::Invalid {
        theme: theme.to_string(),
        reason,
    };

    if layer.source.trim().is_empty() {
        return Err(invalid(format!(
            "Layer '{}' has an empty source path",
            layer.name
        )));
    }
    if layer.prompt.trim().is_empty() {
        return Err(invalid(format!(
            "Layer '{}' has an empty prompt string",
            layer.name
        )));
    }

    // Metallic must be in [0.0, 1.0]
    if !layer.metallic.is_finite() || layer.metallic < 0.0 || layer.metallic > 1.0 {
        return Err(invalid(format!(
            "Layer '{}' metallic must be in [0.0, 1.0], found {}",
            layer.name, layer.metallic
        )));
    }

    // Roughness range: 0.0 <= min <= max <= 1.0
    let (r_min, r_max) = layer.roughness_range;
    if !r_min.is_finite() || !r_max.is_finite() || r_min < 0.0 || r_max > 1.0 || r_min > r_max {
        return Err(invalid(format!(
            "Layer '{}' roughness_range must be ordered within [0.0, 1.0], found ({r_min}, {r_max})",
            layer.name
        )));
    }

    // Normal strength must be positive and finite
    if !layer.normal_strength.is_finite() || layer.normal_strength <= 0.0 {
        return Err(invalid(format!(
            "Layer '{}' normal_strength must be strictly positive and finite, found {}",
            layer.name, layer.normal_strength
        )));
    }

    // Tiling mode validation
    match layer.tiling {
        TilingMode::MinCutBlend => {}
        TilingMode::SourceSeamless { max_seam_score } => {
            if !max_seam_score.is_finite() || max_seam_score <= 0.0 || max_seam_score > 3.0 {
                return Err(invalid(format!(
                    "Layer '{}' max_seam_score must be in (0.0, 3.0], found {max_seam_score}",
                    layer.name
                )));
            }
        }
    }

    Ok(())
}

/// Tiling strategy used to guarantee boundary continuity for a terrain texture layer.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum TilingMode {
    /// Burt-Adelson Laplacian pyramid blending guided by Efros-Freeman minimum-error boundary cuts.
    #[default]
    MinCutBlend,
    /// Direct pass-through of source art that is already seamless.
    ///
    /// Skips boundary blending and verifies that the measured seam score does not exceed `max_seam_score`.
    SourceSeamless {
        /// Maximum permissible wrap seam discontinuity score in `(0.0, 3.0]`.
        max_seam_score: f32,
    },
}

impl fmt::Display for TilingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MinCutBlend => f.pad("MinCutBlend"),
            Self::SourceSeamless { max_seam_score } => {
                f.pad(&format!("SourceSeamless(max={max_seam_score:.2})"))
            }
        }
    }
}

/// The set of four terrain texture layers required by the dual-grid marching squares renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeLayers {
    /// Layer for grass / biological ground (mirrors `sim_types::enums::TerrainKind::Grass`).
    pub grass: ThemeLayer,
    /// Layer for metal plate / constructed deck (mirrors `sim_types::enums::TerrainKind::MetalPlate`).
    pub metal_plate: ThemeLayer,
    /// Layer for dirt / regolith ground (mirrors `sim_types::enums::TerrainKind::Dirt`).
    pub dirt: ThemeLayer,
    /// Layer for vertical cliff walls.
    pub cliff: ThemeLayer,
}

/// Material properties and source art provenance for a single terrain surface layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeLayer {
    /// Human- and machine-readable unique layer name (e.g. `"lumen_moss"`).
    pub name: String,
    /// Relative path under `assets/` to the raw source art PNG image.
    pub source: String,
    /// Tiling strategy (defaults to [`TilingMode::MinCutBlend`]).
    #[serde(default)]
    pub tiling: TilingMode,
    /// Uniform metallic reflectance coefficient in `[0.0, 1.0]`.
    pub metallic: f32,
    /// Target roughness interval `(min, max)` within `[0.0, 1.0]`.
    pub roughness_range: (f32, f32),
    /// Sobel gradient multiplier for derived normal map steepness.
    pub normal_strength: f32,
    /// Text prompt used for generative source art synthesis.
    pub prompt: String,
}

/// Backdrop void texture specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeBackdrop {
    /// Relative path under `assets/` to the source art void backdrop PNG.
    pub source: String,
    /// Text prompt used for generative backdrop synthesis.
    pub prompt: String,
}

/// Lighting configuration bundled with the terrain theme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeLighting {
    /// Direct directional sun illuminance in lux.
    pub sun_illuminance_lux: f32,
    /// Sun elevation angle above the horizon in degrees.
    pub sun_elevation_deg: f32,
    /// Sun horizontal azimuth angle in degrees.
    pub sun_azimuth_deg: f32,
    /// Ambient light color in linear/sRGB space.
    pub ambient_color_srgb: (f32, f32, f32),
    /// Ambient light brightness intensity.
    pub ambient_brightness: f32,
    /// Fog / void clear color in sRGB space.
    pub void_color_srgb: (f32, f32, f32),
    /// Emissive color of ramp guard rails in sRGB space.
    pub guardrail_emissive_srgb: (f32, f32, f32),
    /// Emissive intensity multiplier for ramp guard rails.
    pub guardrail_emissive_intensity: f32,
}

/// Relative output filesystem paths for all baked texture artifacts of a layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakedLayerPaths {
    /// Albedo texture path (sRGB PNG).
    pub albedo: PathBuf,
    /// Tangent-space normal map path (+Y up, OpenGL format).
    pub normal: PathBuf,
    /// Packed Occlusion-Roughness-Metallic texture path.
    pub orm: PathBuf,
    /// 2x2 repetition preview image path for seam inspection.
    pub tiled_preview: PathBuf,
}
