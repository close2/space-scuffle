//! GPU theme texture array builders, CPU mip chain generator, scene lighting,
//! and void backdrop setup.
//!
//! # Architectural Responsibilities
//! - **Texture Arrays with CPU Mips:** Loads baked layer PNGs (Grass, `MetalPlate`, Dirt, Cliff)
//!   and compiles them into 4-layer 2D texture arrays with full CPU-generated mip chains.
//! - **Color-Space Correct Mip Filtering:** Albedo maps average in linear RGB and re-encode to sRGB;
//!   normal maps decode to vectors, average, renormalize, and re-encode; ORM maps average linearly.
//! - **Layer-Major Memory Layout:** Packs textures in layer-major order `[Layer 0 Mips, Layer 1 Mips, ...]`
//!   matching Bevy 0.15's default `TextureDataOrder::LayerMajor`.
//! - **Strict Error Handling:** Fails loudly with [`TerrainThemeError::MissingSource`] if any
//!   baked asset is absent. Zero dummy fallbacks or procedural stand-ins.
//! - **Sampler Quality:** Terrain texture arrays filter at [`TERRAIN_MAX_ANISOTROPY`]. Only the
//!   explicit, logged test override [`TERRAIN_ANISOTROPY_OVERRIDE_ENV`] changes that.

use std::path::{Path, PathBuf};

use asset_baker::terrain_theme::{TerrainTheme, TerrainThemeError, ThemeLayer};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::{CascadeShadowConfigBuilder, DirectionalLight};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    TextureViewDescriptor, TextureViewDimension,
};
use image::RgbaImage;

/// Anisotropic filtering clamp of the terrain texture arrays.
pub const TERRAIN_MAX_ANISOTROPY: u16 = 16;

/// Environment variable that replaces [`TERRAIN_MAX_ANISOTROPY`] for a test run.
///
/// Players never set this. It exists for lavapipe in Mesa releases before 25.1, whose anisotropic
/// sampler visits every texel of a footprint taken from coordinate differences, ignoring explicit
/// gradients and `maxAnisotropy`. `hex_sample` in `terrain.wgsl` is correct, but its coordinates
/// jump across hex edges, so one sample there reads about 10^9 texels and a frame does not finish.
/// See `docs/plans/2026-11-02-server-gate.md`.
///
/// The GPU test harness (`crates/client/tests/support/gpu_run.rs`) sets it only for a child on
/// `lavapipe` before Mesa 25.1, never for one on `radv`, which renders the production sampler
/// (Q0009, `docs/plans/2026-11-03-gpu-test-harness.md` decision D7).
pub const TERRAIN_ANISOTROPY_OVERRIDE_ENV: &str = "SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY";

/// GPU texture resources created from a terrain theme.
pub struct ThemeGpuTextures {
    /// 4-layer 2D array texture containing albedo maps in sRGB format.
    pub albedo: Image,
    /// 4-layer 2D array texture containing tangent-space normal maps in linear format.
    pub normal: Image,
    /// 4-layer 2D array texture containing packed ORM maps in linear format.
    pub orm: Image,
}

pub use asset_baker::{linear_to_srgb, srgb_to_linear};

/// Returns the anisotropy clamp for `override_value`, the override variable's text if it is set.
///
/// # Errors
/// Returns [`TerrainThemeError::InvalidAnisotropyOverride`] unless the text is an integer from 1
/// to 16, so a mistyped override cannot silently change what a test renders.
pub fn terrain_anisotropy_clamp(override_value: Option<&str>) -> Result<u16, TerrainThemeError> {
    let Some(text) = override_value else {
        return Ok(TERRAIN_MAX_ANISOTROPY);
    };
    text.parse()
        .ok()
        .filter(|clamp| (1..=TERRAIN_MAX_ANISOTROPY).contains(clamp))
        .ok_or_else(|| TerrainThemeError::InvalidAnisotropyOverride(text.to_owned()))
}

/// Downsamples an RGBA8 albedo image by 2x using linear color averaging.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
pub fn downsample_albedo_linear(src: &RgbaImage) -> RgbaImage {
    let (sw, sh) = src.dimensions();
    let dw = (sw / 2).max(1);
    let dh = (sh / 2).max(1);
    let mut dst = RgbaImage::new(dw, dh);

    for dy in 0..dh {
        for dx in 0..dw {
            let sx0 = (dx * 2).min(sw - 1);
            let sx1 = (dx * 2 + 1).min(sw - 1);
            let sy0 = (dy * 2).min(sh - 1);
            let sy1 = (dy * 2 + 1).min(sh - 1);

            let p00 = src.get_pixel(sx0, sy0);
            let p10 = src.get_pixel(sx1, sy0);
            let p01 = src.get_pixel(sx0, sy1);
            let p11 = src.get_pixel(sx1, sy1);

            let r_lin = (srgb_to_linear(f32::from(p00[0]) / 255.0)
                + srgb_to_linear(f32::from(p10[0]) / 255.0)
                + srgb_to_linear(f32::from(p01[0]) / 255.0)
                + srgb_to_linear(f32::from(p11[0]) / 255.0))
                * 0.25;

            let g_lin = (srgb_to_linear(f32::from(p00[1]) / 255.0)
                + srgb_to_linear(f32::from(p10[1]) / 255.0)
                + srgb_to_linear(f32::from(p01[1]) / 255.0)
                + srgb_to_linear(f32::from(p11[1]) / 255.0))
                * 0.25;

            let b_lin = (srgb_to_linear(f32::from(p00[2]) / 255.0)
                + srgb_to_linear(f32::from(p10[2]) / 255.0)
                + srgb_to_linear(f32::from(p01[2]) / 255.0)
                + srgb_to_linear(f32::from(p11[2]) / 255.0))
                * 0.25;

            let a =
                (u32::from(p00[3]) + u32::from(p10[3]) + u32::from(p01[3]) + u32::from(p11[3]) + 2)
                    / 4;

            let r_srgb = (linear_to_srgb(r_lin) * 255.0).round().clamp(0.0, 255.0) as u8;
            let g_srgb = (linear_to_srgb(g_lin) * 255.0).round().clamp(0.0, 255.0) as u8;
            let b_srgb = (linear_to_srgb(b_lin) * 255.0).round().clamp(0.0, 255.0) as u8;

            dst.put_pixel(dx, dy, image::Rgba([r_srgb, g_srgb, b_srgb, a as u8]));
        }
    }

    dst
}

/// Downsamples an RGBA8 normal map by 2x with vector decoding, averaging, and renormalization.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
pub fn downsample_normal_renormalized(src: &RgbaImage) -> RgbaImage {
    let (sw, sh) = src.dimensions();
    let dw = (sw / 2).max(1);
    let dh = (sh / 2).max(1);
    let mut dst = RgbaImage::new(dw, dh);

    let decode = |p: &image::Rgba<u8>| -> Vec3 {
        Vec3::new(
            (f32::from(p[0]) / 255.0) * 2.0 - 1.0,
            (f32::from(p[1]) / 255.0) * 2.0 - 1.0,
            (f32::from(p[2]) / 255.0) * 2.0 - 1.0,
        )
    };

    for dy in 0..dh {
        for dx in 0..dw {
            let sx0 = (dx * 2).min(sw - 1);
            let sx1 = (dx * 2 + 1).min(sw - 1);
            let sy0 = (dy * 2).min(sh - 1);
            let sy1 = (dy * 2 + 1).min(sh - 1);

            let v00 = decode(src.get_pixel(sx0, sy0));
            let v10 = decode(src.get_pixel(sx1, sy0));
            let v01 = decode(src.get_pixel(sx0, sy1));
            let v11 = decode(src.get_pixel(sx1, sy1));

            let avg = (v00 + v10 + v01 + v11) * 0.25;
            let norm = if avg.length_squared() > 1e-6 {
                avg.normalize()
            } else {
                Vec3::Z
            };

            let r = ((norm.x * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            let g = ((norm.y * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            let b = ((norm.z * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;

            dst.put_pixel(dx, dy, image::Rgba([r, g, b, 255]));
        }
    }

    dst
}

/// Downsamples an RGBA8 ORM map by 2x using linear channel averaging.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
pub fn downsample_orm_linear(src: &RgbaImage) -> RgbaImage {
    let (sw, sh) = src.dimensions();
    let dw = (sw / 2).max(1);
    let dh = (sh / 2).max(1);
    let mut dst = RgbaImage::new(dw, dh);

    for dy in 0..dh {
        for dx in 0..dw {
            let sx0 = (dx * 2).min(sw - 1);
            let sx1 = (dx * 2 + 1).min(sw - 1);
            let sy0 = (dy * 2).min(sh - 1);
            let sy1 = (dy * 2 + 1).min(sh - 1);

            let p00 = src.get_pixel(sx0, sy0);
            let p10 = src.get_pixel(sx1, sy0);
            let p01 = src.get_pixel(sx0, sy1);
            let p11 = src.get_pixel(sx1, sy1);

            let r =
                (u32::from(p00[0]) + u32::from(p10[0]) + u32::from(p01[0]) + u32::from(p11[0]) + 2)
                    / 4;
            let g =
                (u32::from(p00[1]) + u32::from(p10[1]) + u32::from(p01[1]) + u32::from(p11[1]) + 2)
                    / 4;
            let b =
                (u32::from(p00[2]) + u32::from(p10[2]) + u32::from(p01[2]) + u32::from(p11[2]) + 2)
                    / 4;
            let a =
                (u32::from(p00[3]) + u32::from(p10[3]) + u32::from(p01[3]) + u32::from(p11[3]) + 2)
                    / 4;

            dst.put_pixel(dx, dy, image::Rgba([r as u8, g as u8, b as u8, a as u8]));
        }
    }

    dst
}

/// Builds the full mip chain for a layer texture.
#[must_use]
pub fn generate_mip_chain(
    base: RgbaImage,
    mip_levels: u32,
    downsample_fn: fn(&RgbaImage) -> RgbaImage,
) -> Vec<RgbaImage> {
    let mut mips = Vec::with_capacity(mip_levels as usize);
    if mip_levels == 0 {
        return mips;
    }
    let mut current = base;
    for _ in 1..mip_levels {
        let next = downsample_fn(&current);
        mips.push(current);
        current = next;
    }
    mips.push(current);
    mips
}

/// Loads a theme's baked PNG textures and creates three 4-layer 2D array [`Image`] assets.
///
/// Textures load layer by layer in [`TerrainTheme::ordered_layers`] order (`load_layer`); each
/// map kind then becomes one array (`build_texture_array`) whose layers carry full CPU mip chains
/// in layer-major order.
///
/// # Errors
/// Returns [`TerrainThemeError::MissingSource`] if any required texture file is missing,
/// [`TerrainThemeError::Image`] on decoding failure, or
/// [`TerrainThemeError::InvalidAnisotropyOverride`] if [`TERRAIN_ANISOTROPY_OVERRIDE_ENV`] is malformed.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "texture sizes stay far below the range of f32 and u32"
)]
pub fn load_theme_texture_arrays(
    theme: &TerrainTheme,
    assets_root: &Path,
) -> Result<ThemeGpuTextures, TerrainThemeError> {
    let anisotropy_override = std::env::var_os(TERRAIN_ANISOTROPY_OVERRIDE_ENV)
        .map(|value| value.to_string_lossy().into_owned());
    let anisotropy_clamp = terrain_anisotropy_clamp(anisotropy_override.as_deref())?;
    if anisotropy_clamp != TERRAIN_MAX_ANISOTROPY {
        warn!(
            "{TERRAIN_ANISOTROPY_OVERRIDE_ENV} lowers terrain anisotropy from \
             {TERRAIN_MAX_ANISOTROPY} to {anisotropy_clamp}; this is a test-only setting"
        );
    }

    let mut albedo_bases = Vec::with_capacity(4);
    let mut normal_bases = Vec::with_capacity(4);
    let mut orm_bases = Vec::with_capacity(4);
    for layer in theme.ordered_layers() {
        let [albedo, normal, orm] = load_layer(theme, layer, assets_root)?;
        albedo_bases.push(albedo);
        normal_bases.push(normal);
        orm_bases.push(orm);
    }

    let array = ArrayShape {
        size: theme.texture_size,
        mip_levels: (theme.texture_size as f32).log2() as u32 + 1,
        anisotropy_clamp,
    };
    Ok(ThemeGpuTextures {
        albedo: build_texture_array(
            albedo_bases,
            downsample_albedo_linear,
            TextureFormat::Rgba8UnormSrgb,
            &array,
        ),
        normal: build_texture_array(
            normal_bases,
            downsample_normal_renormalized,
            TextureFormat::Rgba8Unorm,
            &array,
        ),
        orm: build_texture_array(
            orm_bases,
            downsample_orm_linear,
            TextureFormat::Rgba8Unorm,
            &array,
        ),
    })
}

/// Loads one theme layer's baked `[albedo, normal, orm]` images.
///
/// All three files are checked for existence before any is decoded, so a missing file is reported
/// ahead of a corrupt sibling.
///
/// # Errors
/// [`TerrainThemeError::MissingSource`] for the first missing file in albedo, normal, orm order;
/// otherwise [`TerrainThemeError::Image`] for the first file that fails to decode.
fn load_layer(
    theme: &TerrainTheme,
    layer: &ThemeLayer,
    assets_root: &Path,
) -> Result<[RgbaImage; 3], TerrainThemeError> {
    let paths = theme.baked_paths(layer);
    let files = [
        assets_root.join(&paths.albedo),
        assets_root.join(&paths.normal),
        assets_root.join(&paths.orm),
    ];
    if let Some(missing) = files.iter().find(|path| !path.exists()) {
        return Err(TerrainThemeError::MissingSource(missing.clone()));
    }
    let [albedo, normal, orm] = &files;
    Ok([
        decode_rgba(albedo)?,
        decode_rgba(normal)?,
        decode_rgba(orm)?,
    ])
}

/// Decodes the image at `path` to RGBA8.
fn decode_rgba(path: &Path) -> Result<RgbaImage, TerrainThemeError> {
    Ok(image::open(path)
        .map_err(|e| TerrainThemeError::Image(format!("{}: {e}", path.display())))?
        .to_rgba8())
}

/// Shape and sampling shared by the three terrain texture arrays.
struct ArrayShape {
    /// Edge length of mip 0 in texels.
    size: u32,
    /// Mip levels per layer, down to 1x1.
    mip_levels: u32,
    /// Anisotropic filtering clamp of the sampler.
    anisotropy_clamp: u16,
}

/// Builds one 4-layer 2D array [`Image`] from its layer base images: a CPU mip chain per layer
/// made with `downsample`, packed layer-major (`[layer 0 mips, layer 1 mips, ...]`), with a
/// repeating trilinear sampler.
fn build_texture_array(
    bases: Vec<RgbaImage>,
    downsample: fn(&RgbaImage) -> RgbaImage,
    format: TextureFormat,
    shape: &ArrayShape,
) -> Image {
    let ArrayShape {
        size,
        mip_levels,
        anisotropy_clamp,
    } = *shape;

    // Calculate total buffer size across all 4 layers and their mips
    let mut per_layer_bytes = 0usize;
    for k in 0..mip_levels {
        let d = (size >> k).max(1) as usize;
        per_layer_bytes += d * d * 4;
    }
    let total_bytes = per_layer_bytes * 4;

    // Pack in layer-major order: Layer 0 (all mips), Layer 1 (all mips), ...
    let mut data = Vec::with_capacity(total_bytes);
    for base in bases {
        for mip in generate_mip_chain(base, mip_levels, downsample) {
            data.extend_from_slice(&mip);
        }
    }

    Image {
        data,
        texture_descriptor: TextureDescriptor {
            label: None,
            size: Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 4,
            },
            mip_level_count: mip_levels,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        texture_view_descriptor: Some(TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2Array),
            ..default()
        }),
        sampler: ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            anisotropy_clamp,
            ..default()
        }),
        asset_usage: RenderAssetUsages::default(),
    }
}

/// Spawns theme directional and ambient lights into the scene.
pub fn setup_theme_lighting(commands: &mut Commands<'_, '_>, theme: &TerrainTheme) {
    setup_theme_lighting_owned(commands, theme, None);
}

/// Spawns theme lighting with an optional scene owner for runtime cleanup.
///
/// Owned lights do not change global ambient or clear-color resources: the runtime
/// commits those only when its complete scene succeeds. Standalone callers without
/// an owner retain the theme's immediate global environment installation.
pub fn setup_theme_lighting_owned(
    commands: &mut Commands<'_, '_>,
    theme: &TerrainTheme,
    owner: Option<Entity>,
) {
    let lighting = &theme.lighting;

    // Directional light rotation from elevation and azimuth angles
    let rotation = Quat::from_rotation_y(lighting.sun_azimuth_deg.to_radians())
        * Quat::from_rotation_x(-lighting.sun_elevation_deg.to_radians());

    let mut light = commands.spawn((
        DirectionalLight {
            illuminance: lighting.sun_illuminance_lux,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(rotation),
        CascadeShadowConfigBuilder {
            num_cascades: 3,
            first_cascade_far_bound: 15.0,
            maximum_distance: 140.0,
            ..default()
        }
        .build(),
    ));

    if let Some(parent) = owner {
        light.set_parent(parent);
    }

    if owner.is_none() {
        let (ambient, clear) = theme_environment(theme);
        commands.insert_resource(ambient);
        commands.insert_resource(clear);
    }
}

/// Builds the global ambient and clear-color values for an authored theme.
///
/// Colors enter as sRGB; ambient brightness retains Bevy's authored lighting units.
/// Owned runtime scenes commit these values through their environment lifecycle.
#[must_use]
pub fn theme_environment(theme: &TerrainTheme) -> (AmbientLight, ClearColor) {
    let lighting = &theme.lighting;
    (
        AmbientLight {
            color: Color::srgb(
                lighting.ambient_color_srgb.0,
                lighting.ambient_color_srgb.1,
                lighting.ambient_color_srgb.2,
            ),
            brightness: lighting.ambient_brightness,
        },
        ClearColor(Color::srgb(
            lighting.void_color_srgb.0,
            lighting.void_color_srgb.1,
            lighting.void_color_srgb.2,
        )),
    )
}

/// Spawns the deep-space void backdrop plane underneath the floating terrain map.
///
/// # Errors
/// Returns [`TerrainThemeError::MissingSource`] if the void nebula texture is absent.
pub fn setup_theme_backdrop(
    commands: &mut Commands<'_, '_>,
    theme: &TerrainTheme,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    assets_root: &Path,
    map_size: Vec2,
) -> Result<Entity, TerrainThemeError> {
    let backdrop_rel = PathBuf::from(&theme.baked_dir).join("void_nebula.png");
    let backdrop_path = assets_root.join(&backdrop_rel);

    if !backdrop_path.exists() {
        return Err(TerrainThemeError::MissingSource(backdrop_path));
    }

    let raw_img = image::open(&backdrop_path)
        .map_err(|e| TerrainThemeError::Image(format!("{}: {e}", backdrop_path.display())))?
        .to_rgba8();
    let (bw, bh) = raw_img.dimensions();

    let mut nebula_image = Image::new(
        Extent3d {
            width: bw,
            height: bh,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        raw_img.into_raw(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    nebula_image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });

    let image_handle = images.add(nebula_image);

    let backdrop_material = materials.add(StandardMaterial {
        base_color_texture: Some(image_handle),
        unlit: true,
        ..default()
    });

    let plane_size = (map_size.x.max(map_size.y) * 4.0).max(256.0);
    let plane_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(plane_size)));

    let entity = commands
        .spawn((
            Mesh3d(plane_mesh),
            MeshMaterial3d(backdrop_material),
            Transform::from_xyz(map_size.x * 0.5, -40.0, map_size.y * 0.5),
        ))
        .id();

    Ok(entity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_runs_at_the_full_clamp() {
        assert!(matches!(
            terrain_anisotropy_clamp(None),
            Ok(TERRAIN_MAX_ANISOTROPY)
        ));
    }

    #[test]
    fn override_accepts_every_clamp_in_range() {
        assert!(matches!(terrain_anisotropy_clamp(Some("1")), Ok(1)));
        assert!(matches!(terrain_anisotropy_clamp(Some("16")), Ok(16)));
    }

    #[test]
    fn malformed_override_is_refused_and_names_the_value() {
        for text in ["", "0", "17", "-1", "1.5", "off"] {
            let result = terrain_anisotropy_clamp(Some(text));
            assert!(
                matches!(
                    &result,
                    Err(TerrainThemeError::InvalidAnisotropyOverride(value)) if value == text
                ),
                "{text:?} must be refused by value, got {result:?}"
            );
        }
    }
}
