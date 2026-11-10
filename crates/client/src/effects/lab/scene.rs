//! The effect lab's stage: real terrain swatches, the theme's light and the battle camera.
//!
//! # Responsibility
//! Owns [`LabScene`]: one large ground plane drawn with the brightest or the darkest ground
//! layer of the first terrain theme (albedo, normal and occlusion-roughness-metallic textures
//! from the theme's baked directory, tiled at the theme's ground rate), lit by that theme's sun
//! and ambient light; the battle camera ([`crate::camera::RtsCamera`] at the battle pitch, with
//! the depth prepass the effect shaders read); and the reference overlay (a 1-unit grid and the
//! bound radii or line, drawn with gizmos only in the frames that ask for it).
//!
//! # How it fits
//! Built by [`super::EffectLabPlugin`] at startup from the assets root; the lab driver swaps
//! the plane's material and the camera's pose per take. Nothing here names a theme or a layer:
//! both come from `catalog/terrain_themes.ron` (charter C5, C6).
//!
//! # Invariants
//! - The ground lies at y = 0 and is flat, so a projected decal and a ground ring have a
//!   well-defined footprint.
//! - Every texture is loaded through the asset server from the runtime path the battle uses.

use super::program::{LabGround, LabSubject};
use crate::ResolvedAssetsRoot;
use asset_baker::terrain_theme::{TerrainTheme, TerrainThemeCatalog, ThemeLayer};
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use std::path::Path;

/// Path of the theme catalog below the assets root.
pub const THEME_CATALOG_PATH: &str = "catalog/terrain_themes.ron";

/// Side of the ground plane, world units.
const GROUND_SIDE: f32 = 96.0;

/// Half the number of grid cells drawn on each side of the focus.
const GRID_HALF_CELLS: u32 = 8;

/// The stage's handles.
#[derive(Resource, Debug, Clone)]
pub struct LabScene {
    /// The ground plane.
    pub ground: Entity,
    /// The camera.
    pub camera: Entity,
    /// Material of each [`LabGround`], in [`LabGround::ALL`] order.
    pub materials: [Handle<StandardMaterial>; 2],
    /// Every texture the stage loads, for the readiness check.
    pub images: Vec<Handle<Image>>,
}

/// A stage that cannot be built.
#[derive(Debug, thiserror::Error)]
pub enum LabSceneError {
    /// The theme catalog could not be read.
    #[error("terrain themes: {0}")]
    Themes(#[from] asset_baker::terrain_theme::TerrainThemeError),
    /// The catalog has no theme.
    #[error("terrain themes: the catalog defines no theme")]
    NoTheme,
    /// The ground mesh's tangents (for its normal map) could not be generated.
    #[error("ground mesh tangents: {0}")]
    Tangents(String),
    /// An albedo texture could not be read to measure its brightness.
    #[error("terrain albedo {path}: {source}")]
    Albedo {
        /// The texture.
        path: std::path::PathBuf,
        /// The decoder's failure.
        #[source]
        source: image::ImageError,
    },
}

/// The first theme of the catalog under `root`.
///
/// # Errors
/// [`LabSceneError::Themes`] or [`LabSceneError::NoTheme`].
pub fn lab_theme(root: &Path) -> Result<TerrainTheme, LabSceneError> {
    let catalog = TerrainThemeCatalog::load(&root.join(THEME_CATALOG_PATH))?;
    catalog
        .themes
        .into_iter()
        .next()
        .ok_or(LabSceneError::NoTheme)
}

/// The brightest and the darkest ground layer of `theme` (cliffs excluded), by the mean
/// Rec. 709 luminance of their albedo under `root`.
///
/// # Errors
/// [`LabSceneError::Albedo`] when an albedo cannot be decoded.
pub fn ground_layers(theme: &TerrainTheme, root: &Path) -> Result<[ThemeLayer; 2], LabSceneError> {
    let layers = [
        &theme.layers.grass,
        &theme.layers.metal_plate,
        &theme.layers.dirt,
    ];
    let mut measured = Vec::with_capacity(layers.len());
    for layer in layers {
        let path = root.join(theme.baked_paths(layer).albedo);
        let image = image::open(&path)
            .map_err(|source| LabSceneError::Albedo {
                path: path.clone(),
                source,
            })?
            .to_rgb8();
        let sum: f64 = image
            .pixels()
            .map(|pixel| {
                0.2126 * f64::from(pixel.0[0])
                    + 0.7152 * f64::from(pixel.0[1])
                    + 0.0722 * f64::from(pixel.0[2])
            })
            .sum();
        measured.push((sum, layer));
    }
    measured.sort_by(|a, b| b.0.total_cmp(&a.0));
    let bright = measured.first().map(|(_, layer)| (*layer).clone());
    let dark = measured.last().map(|(_, layer)| (*layer).clone());
    match (bright, dark) {
        (Some(bright), Some(dark)) => Ok([bright, dark]),
        _ => Err(LabSceneError::NoTheme),
    }
}

/// Loads `path` with a repeating, trilinear sampler (`srgb` for colour data).
fn load_tiled(server: &AssetServer, path: &Path, srgb: bool) -> Handle<Image> {
    server.load_with_settings(
        path.to_path_buf(),
        move |settings: &mut ImageLoaderSettings| {
            settings.is_srgb = srgb;
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
        },
    )
}

/// The ground material of `layer`, tiled at the theme's ground rate over the plane.
fn ground_material(
    server: &AssetServer,
    theme: &TerrainTheme,
    layer: &ThemeLayer,
    images: &mut Vec<Handle<Image>>,
) -> StandardMaterial {
    let paths = theme.baked_paths(layer);
    let albedo = load_tiled(server, &paths.albedo, true);
    let normal = load_tiled(server, &paths.normal, false);
    let orm = load_tiled(server, &paths.orm, false);
    images.extend([albedo.clone(), normal.clone(), orm.clone()]);
    StandardMaterial {
        base_color_texture: Some(albedo),
        normal_map_texture: Some(normal),
        metallic_roughness_texture: Some(orm.clone()),
        occlusion_texture: Some(orm),
        metallic: layer.metallic,
        perceptual_roughness: layer.roughness_range.1,
        uv_transform: Affine2::from_scale(Vec2::splat(GROUND_SIDE * theme.ground_tiles_per_unit)),
        ..default()
    }
}

/// Spawns the stage for `subject` and returns its handles.
///
/// # Errors
/// [`LabSceneError`] when the theme or its albedos cannot be read.
pub fn spawn_scene(
    commands: &mut Commands<'_, '_>,
    server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: &ResolvedAssetsRoot,
) -> Result<LabScene, LabSceneError> {
    let theme = lab_theme(root.as_path())?;
    let layers = ground_layers(&theme, root.as_path())?;
    let mut images = Vec::with_capacity(6);
    let handles = [
        materials.add(ground_material(server, &theme, &layers[0], &mut images)),
        materials.add(ground_material(server, &theme, &layers[1], &mut images)),
    ];
    let mesh = Plane3d::default()
        .mesh()
        .size(GROUND_SIDE, GROUND_SIDE)
        .build()
        .with_generated_tangents()
        .map_err(|error| LabSceneError::Tangents(error.to_string()))?;
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(handles[0].clone()),
            Transform::default(),
        ))
        .id();
    crate::terrain::theme_gpu::setup_theme_lighting(commands, &theme);
    let camera = commands
        .spawn((Camera3d::default(), DepthPrepass, Transform::default()))
        .id();
    Ok(LabScene {
        ground,
        camera,
        materials: handles,
        images,
    })
}

/// The material handle of `ground`.
#[must_use]
pub fn material_of(scene: &LabScene, ground: LabGround) -> Handle<StandardMaterial> {
    match ground {
        LabGround::Bright => scene.materials[0].clone(),
        LabGround::Dark => scene.materials[1].clone(),
    }
}

/// Draws the reference overlay for `subject`: a 1-unit grid around its focus and the bound
/// radii as circles (full damage and reach) or, for a line, the line and the full-damage
/// circle at every search point.
pub fn draw_overlay(gizmos: &mut Gizmos<'_, '_>, subject: &LabSubject, radii: [f32; 2]) {
    let focus = subject.focus();
    let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let lift = Vec3::Y * 0.01;
    gizmos.grid(
        Isometry3d::new(Vec3::new(focus.x, 0.0, focus.y) + lift, flat),
        UVec2::splat(GRID_HALF_CELLS * 2),
        Vec2::ONE,
        Color::srgba(1.0, 1.0, 1.0, 0.35),
    );
    let [full, reach] = radii;
    let anchor = Vec3::new(subject.anchor.x, 0.0, subject.anchor.z) + lift;
    let Some(to) = subject.line_to else {
        if full > 0.0 {
            gizmos.circle(
                Isometry3d::new(anchor, flat),
                full,
                Color::srgb(1.0, 0.25, 0.2),
            );
        }
        if reach > 0.0 {
            gizmos.circle(
                Isometry3d::new(anchor, flat),
                reach,
                Color::srgb(1.0, 0.9, 0.2),
            );
        }
        return;
    };
    let to = Vec3::new(to.x, 0.0, to.z) + lift;
    gizmos.line(anchor, to, Color::srgb(1.0, 0.9, 0.2));
    let length = anchor.distance(to);
    let spacing = subject
        .bound
        .get(super::super::catalog::schema::Binding::LineSpacing);
    if spacing <= f32::EPSILON || full <= 0.0 {
        return;
    }
    let direction = (to - anchor).normalize_or_zero();
    let mut distance = 0.0;
    while distance <= length + 1e-3 {
        gizmos.circle(
            Isometry3d::new(anchor + direction * distance, flat),
            full,
            Color::srgb(1.0, 0.25, 0.2),
        );
        distance += spacing;
    }
}

/// Opacity of the black fog veil: the ground behind it keeps a quarter of its colour, as the
/// battle terrain's fog factor (`terrain.wgsl`, `mix(0.25, 1.0, visible)`) leaves it.
const FOG_VEIL_OPACITY: f32 = 0.75;
/// Height of the fog veil over the ground: above the ground's depth, far below any effect.
const FOG_VEIL_HEIGHT: f32 = 0.002;

/// Spawns the fog veil over the ground below world x `below_x`: an unlit black plane whose
/// blend leaves the ground a quarter of its colour with a hard edge, the battle terrain's fog
/// over cells the viewer does not see. It is blended and writes no depth prepass, so decals
/// still project onto the ground beneath it.
pub fn spawn_fog_veil(
    commands: &mut Commands<'_, '_>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    below_x: f32,
) {
    let left = -GROUND_SIDE * 0.5;
    let width = (below_x - left).max(0.0);
    if width <= 0.0 {
        return;
    }
    let material = StandardMaterial {
        base_color: Color::BLACK.with_alpha(FOG_VEIL_OPACITY),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    };
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(width, GROUND_SIDE))),
        MeshMaterial3d(materials.add(material)),
        Transform::from_xyz(left + width * 0.5, FOG_VEIL_HEIGHT, 0.0),
    ));
}
