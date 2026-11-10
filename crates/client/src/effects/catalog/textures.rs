//! Reads the baked effect texture sets an effect vocabulary lists, into one CPU texture array.
//!
//! # Responsibility
//! Owns [`TextureAtlas`]: the elements of every listed set by name, and the texels of every
//! drawable (`PackedV1`) set concatenated into one layer-major array with its mip chain, ready
//! to become a single `texture_2d_array` on the GPU.
//!
//! # How it fits
//! The manifest type and its consistency check belong to `asset_baker`
//! (`asset_baker::effects::texture_set`, plan §6.4), which writes the sets; this module only
//! reads them, so the format has one definition. `NoiseV1` sets are resolved by name (their
//! elements are data for later blocks) but not uploaded by WP4's blocks.
//!
//! # Invariants
//! - Every drawable set shares one cell size and mip count (one array), else a typed error.
//! - Element names are unique across all listed sets.
//! - A strip whose size differs from its manifest is refused, never resampled.

use super::error::EffectCatalogError;
use asset_baker::effects::texture_set::{
    check_manifest, load_ron, ChannelLayout, EffectTextureSet, ElementBlend,
};
use std::collections::BTreeMap;
use std::path::Path;

/// One element of a listed set.
#[derive(Debug, Clone, PartialEq)]
pub struct AtlasElement {
    /// Layer of its first frame in the drawable array; meaningless when not drawable.
    pub first_layer: u32,
    /// Frames (1 for a still).
    pub frames: u32,
    /// Real play time of all frames in seconds, when the element is a timed flipbook.
    pub duration_secs: Option<f32>,
    /// Whether a block may draw it (`PackedV1` and not `Data`).
    pub drawable: bool,
}

/// The listed texture sets: elements by name and the drawable texels.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextureAtlas {
    /// Side of every level-0 cell in pixels; 0 when no drawable set is listed.
    pub cell_size: u32,
    /// Mip levels of every drawable cell.
    pub mip_count: u32,
    /// Layers of the drawable array.
    pub layers: u32,
    /// RGBA8 texels, layer-major: every mip of layer 0, then layer 1, and so on.
    pub texels: Vec<u8>,
    /// Every element of every listed set.
    pub elements: BTreeMap<String, AtlasElement>,
}

impl TextureAtlas {
    /// Reads every manifest in `manifests`, each relative to `root` unless absolute.
    ///
    /// # Errors
    /// [`EffectCatalogError::TextureManifest`], [`EffectCatalogError::TextureImage`],
    /// [`EffectCatalogError::StripSize`], [`EffectCatalogError::CellSizeMismatch`] or
    /// [`EffectCatalogError::DuplicateElement`].
    pub fn load(root: &Path, manifests: &[String]) -> Result<Self, EffectCatalogError> {
        let mut atlas = Self::default();
        let mut strips_by_set = Vec::new();
        for relative in manifests {
            let path = root.join(relative);
            let manifest: EffectTextureSet =
                load_ron(&path).map_err(|source| EffectCatalogError::TextureManifest {
                    path: path.clone(),
                    source: Box::new(source),
                })?;
            check_manifest(&manifest, &path).map_err(|source| {
                EffectCatalogError::TextureManifest {
                    path: path.clone(),
                    source: Box::new(source),
                }
            })?;
            let drawable_set = manifest.channels == ChannelLayout::PackedV1;
            if drawable_set {
                atlas.admit_shape(&manifest, &path)?;
            }
            for element in &manifest.elements {
                let entry = AtlasElement {
                    first_layer: atlas.layers + element.first_layer,
                    frames: element.frames,
                    duration_secs: element.duration_ms.map(millis_to_secs),
                    drawable: drawable_set && element.blend != ElementBlend::Data,
                };
                if atlas.elements.insert(element.name.clone(), entry).is_some() {
                    return Err(EffectCatalogError::DuplicateElement {
                        path,
                        element: element.name.clone(),
                    });
                }
            }
            if drawable_set {
                let directory = path.parent().unwrap_or(root);
                strips_by_set.push(read_strips(directory, &manifest)?);
                atlas.layers += manifest.layers;
            }
        }
        atlas.pack(&strips_by_set);
        Ok(atlas)
    }

    /// Records the first drawable set's cell size and mip count, or checks a later one's.
    fn admit_shape(
        &mut self,
        manifest: &EffectTextureSet,
        path: &Path,
    ) -> Result<(), EffectCatalogError> {
        let mips = u32::try_from(manifest.mip_files.len()).unwrap_or(u32::MAX);
        if self.cell_size == 0 {
            self.cell_size = manifest.cell_size;
            self.mip_count = mips;
            return Ok(());
        }
        if self.cell_size != manifest.cell_size || self.mip_count != mips {
            return Err(EffectCatalogError::CellSizeMismatch {
                path: path.to_path_buf(),
                found: manifest.cell_size,
                found_mips: manifest.mip_files.len(),
                expected: self.cell_size,
                expected_mips: self.mip_count as usize,
            });
        }
        Ok(())
    }

    /// Concatenates the sets' strips into the layer-major texel array.
    fn pack(&mut self, sets: &[Vec<image::RgbaImage>]) {
        for strips in sets {
            let Some(base) = strips.first() else {
                continue;
            };
            let layers = base.height() / base.width().max(1);
            for layer in 0..layers {
                for strip in strips {
                    let side = strip.width() as usize;
                    let start = layer as usize * side * side * 4;
                    let raw = strip.as_raw();
                    if let Some(cell) = raw.get(start..start + side * side * 4) {
                        self.texels.extend_from_slice(cell);
                    }
                }
            }
        }
    }
}

/// Milliseconds as seconds.
fn millis_to_secs(millis: u32) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a play time in seconds needs no more than f32 precision"
    )]
    let seconds = (f64::from(millis) / 1000.0) as f32;
    seconds
}

/// Decodes every mip strip of a drawable set and checks its size.
fn read_strips(
    directory: &Path,
    manifest: &EffectTextureSet,
) -> Result<Vec<image::RgbaImage>, EffectCatalogError> {
    let mut strips = Vec::with_capacity(manifest.mip_files.len());
    for (level, name) in manifest.mip_files.iter().enumerate() {
        let path = directory.join(name);
        let strip = image::open(&path)
            .map_err(|source| EffectCatalogError::TextureImage {
                path: path.clone(),
                source: Box::new(source),
            })?
            .to_rgba8();
        let side = (manifest.cell_size >> level).max(1);
        let expected = [side, side * manifest.layers];
        let found = [strip.width(), strip.height()];
        if found != expected {
            return Err(EffectCatalogError::StripSize {
                path,
                found,
                expected,
            });
        }
        strips.push(strip);
    }
    Ok(strips)
}
