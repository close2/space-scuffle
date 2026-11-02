//! Custom Bevy `ExtendedMaterial` and GPU uniform binding for the shared terrain renderer.
//!
//! # Architectural Responsibilities
//! - Binds terrain uniform properties ([`TerrainParams`]) into `@group(2) @binding(100)`.
//! - Binds 4-layer 2D texture arrays for albedo (101/102), normal (103/104), and ORM (105/106).
//! - Binds bilinear splat texture (107/108) and unfilterable overlay texture (109).
//! - Declares the fragment shader extension pointing to `shaders/terrain.wgsl`.
use asset_baker::TerrainTheme;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};

/// Default sharpening exponent applied to hex-tiling blending weights.
pub const HEX_CONTRAST: f32 = 6.0;

/// Overlay bitflag: Anti-aliased cell grid lines on floor surfaces.
pub const OVERLAY_FLAG_GRID: u32 = 1 << 0;
/// Overlay bitflag: Unpathable ground cells highlighted with red tint.
pub const OVERLAY_FLAG_PATHABILITY: u32 = 1 << 1;
/// Overlay bitflag: Unbuildable cells highlighted with amber tint.
pub const OVERLAY_FLAG_BUILDABILITY: u32 = 1 << 2;
/// Overlay bitflag: Line-of-Sight visibility fog and shadow modulation.
pub const OVERLAY_FLAG_LOS: u32 = 1 << 3;
/// Overlay bitflag: Active brush tool footprint and boundary highlight.
pub const OVERLAY_FLAG_BRUSH: u32 = 1 << 4;
/// Overlay bitflag: Map validation error location pulsing red fill.
pub const OVERLAY_FLAG_VALIDATION: u32 = 1 << 5;

mod gpu_layout {
    //! Holds one `ShaderType` struct, so the lint expectation below covers nothing else.
    #![expect(
        dead_code,
        reason = "the ShaderType derive emits layout checks nothing calls"
    )]

    use bevy::prelude::*;
    use bevy::render::render_resource::ShaderType;

    /// Uniform buffer struct bound at `@group(2) @binding(100)`.
    ///
    /// Memory layout strictly satisfies std140 16-byte alignment rules (64 bytes total).
    #[derive(Clone, Copy, Debug, ShaderType, Reflect)]
    pub struct TerrainParams {
        /// Map width and height in cell units.
        pub map_size: Vec2,
        /// Brush center in cell coordinates (x, y).
        pub brush_center: Vec2,
        /// Brush radius in cell units.
        pub brush_radius: f32,
        /// Brush shape: 0 = circle, 1 = square.
        pub brush_shape: u32,
        /// Bitwise combination of `OVERLAY_FLAG_*`.
        pub overlay_flags: u32,
        /// Texture tiling frequency for floor surface layers (repeats per world unit).
        pub ground_tiles_per_unit: f32,
        /// Texture tiling frequency for cliff triplanar projection.
        pub cliff_tiles_per_unit: f32,
        /// Contrast sharpening exponent for hex-tiling weight blending.
        pub hex_contrast: f32,
        /// Layers authored as seamless structured surfaces, sampled without random rotations.
        pub regular_tiling_mask: u32,
        /// Padding ensuring 16-byte alignment before the 16-byte vector.
        pub pad: u32,
        /// Linear emissive color multiplied by intensity for ramp guard rails.
        pub guardrail_emissive: Vec4,
    }
}
pub use gpu_layout::TerrainParams;

impl TerrainParams {
    /// Constructs [`TerrainParams`] directly from the declarative [`TerrainTheme`] and map grid dimensions.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "map cell coordinates stay far below the range of f32 and i32"
    )]
    pub fn from_theme(theme: &TerrainTheme, map_w: u32, map_h: u32) -> Self {
        let (gr_r, gr_g, gr_b) = theme.lighting.guardrail_emissive_srgb;
        let intensity = theme.lighting.guardrail_emissive_intensity;
        Self {
            map_size: Vec2::new(map_w as f32, map_h as f32),
            brush_center: Vec2::ZERO,
            brush_radius: 2.0,
            brush_shape: 0,
            overlay_flags: OVERLAY_FLAG_GRID,
            ground_tiles_per_unit: theme.ground_tiles_per_unit,
            cliff_tiles_per_unit: theme.cliff_tiles_per_unit,
            hex_contrast: HEX_CONTRAST,
            regular_tiling_mask: theme.ordered_layers().iter().enumerate().fold(
                0,
                |mask, (index, layer)| {
                    if matches!(layer.tiling, asset_baker::TilingMode::SourceSeamless { .. }) {
                        mask | (1 << index)
                    } else {
                        mask
                    }
                },
            ),
            pad: 0,
            guardrail_emissive: Vec4::new(
                gr_r * intensity,
                gr_g * intensity,
                gr_b * intensity,
                1.0,
            ),
        }
    }
}

/// Material extension providing terrain uniform and texture array bindings.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TerrainExtension {
    /// Uniform parameters buffer.
    #[uniform(100)]
    pub params: TerrainParams,
    /// 4-layer 2D array of albedo textures.
    #[texture(101, dimension = "2d_array")]
    #[sampler(102)]
    pub albedo: Handle<Image>,
    /// 4-layer 2D array of normal maps.
    #[texture(103, dimension = "2d_array")]
    #[sampler(104)]
    pub normal: Handle<Image>,
    /// 4-layer 2D array of ORM maps.
    #[texture(105, dimension = "2d_array")]
    #[sampler(106)]
    pub orm: Handle<Image>,
    /// Continuous 2D splat weight texture.
    #[texture(107)]
    #[sampler(108)]
    pub splat: Handle<Image>,
    /// Discrete 2D tactical overlay texture read via `textureLoad`.
    #[texture(109, sample_type = "float", filterable = false)]
    pub overlay: Handle<Image>,
}

impl MaterialExtension for TerrainExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }
}

/// Complete terrain material extending Bevy's [`StandardMaterial`].
pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExtension>;

/// Constructs a default base [`StandardMaterial`] configured for the terrain extension.
#[must_use]
pub fn create_default_terrain_base_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.5,
        ..default()
    }
}
