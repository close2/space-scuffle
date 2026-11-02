//! Exact on-disk unit media and material-binding evidence for fixed-bundle captures.
//!
//! Capture independently validates the selected slug's real PNG/metadata against
//! actual GPU image data, detecting post-admission registry/material mutation.
//! Successful immutable asset/metadata pairs are cached only for this capture run;
//! live content authoring during a run is outside the fixed-bundle contract.

use super::config::CaptureError;
use asset_baker::{FlipbookAtlasMeta, FlipbookLayout};
use bevy::prelude::*;
use bevy::render::render_resource::{TextureDimension, TextureFormat};
use sim_types::UnitTypeId;
use std::{collections::BTreeMap, path::Path};

pub(super) struct MediaBinding<'a> {
    pub unit_type: UnitTypeId,
    pub slug: &'a str,
    pub registered_texture: &'a Handle<Image>,
    pub material_texture: &'a Handle<Image>,
    pub metadata: &'a FlipbookAtlasMeta,
    pub image: &'a Image,
}

#[derive(Default)]
pub(super) struct ValidatedMedia {
    verified: BTreeMap<UnitTypeId, (AssetId<Image>, FlipbookAtlasMeta)>,
}

impl ValidatedMedia {
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Bevy passes system parameters by value"
    )]
    pub(super) fn validate(
        &mut self,
        root: &Path,
        binding: MediaBinding<'_>,
    ) -> Result<(), CaptureError> {
        let image_path = root.join("textures").join(format!("{}.png", binding.slug));
        if binding.registered_texture != binding.material_texture {
            return Err(CaptureError::MediaMismatch {
                path: image_path,
                reason: "visible material does not bind the validated archetype image",
            });
        }
        if self
            .verified
            .get(&binding.unit_type)
            .is_some_and(|(id, metadata)| {
                *id == binding.registered_texture.id() && metadata == binding.metadata
            })
        {
            return Ok(());
        }
        validate_source(root, binding.slug, binding.metadata, binding.image)?;
        self.verified.insert(
            binding.unit_type,
            (binding.registered_texture.id(), binding.metadata.clone()),
        );
        Ok(())
    }
}

fn validate_source(
    root: &Path,
    slug: &str,
    registered_meta: &FlipbookAtlasMeta,
    image: &Image,
) -> Result<(), CaptureError> {
    let meta_path = root.join("textures").join(format!("{slug}.flipbook.ron"));
    let image_path = root.join("textures").join(format!("{slug}.png"));
    let meta = FlipbookAtlasMeta::load_from_ron(&meta_path).map_err(|source| {
        CaptureError::MediaMetadata {
            path: meta_path.clone(),
            source,
        }
    })?;
    let mismatch = |reason| CaptureError::MediaMismatch {
        path: image_path.clone(),
        reason,
    };
    if meta.name != slug || meta.layout != FlipbookLayout::VerticalStrip || &meta != registered_meta
    {
        return Err(CaptureError::MediaMismatch {
            path: meta_path,
            reason: "authored identity/layout/metadata differs from the registered archetype",
        });
    }
    let height = meta
        .frame_height
        .checked_mul(meta.total_frames)
        .ok_or_else(|| mismatch("authored strip height overflows"))?;
    let decoded = image::open(&image_path)
        .map_err(|source| CaptureError::MediaImage {
            path: image_path.clone(),
            source,
        })?
        .to_rgba8();
    if decoded.dimensions() != (meta.frame_width, height) {
        return Err(mismatch(
            "PNG dimensions differ from the authored vertical strip",
        ));
    }
    let descriptor = &image.texture_descriptor;
    if descriptor.size.width != meta.frame_width
        || descriptor.size.height != meta.frame_height
        || descriptor.size.depth_or_array_layers != meta.total_frames
        || descriptor.dimension != TextureDimension::D2
        || descriptor.format != TextureFormat::Rgba8UnormSrgb
        || image.data != *decoded.as_raw()
    {
        return Err(mismatch(
            "registered GPU array dimensions/format/bytes differ from the actual PNG",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::{render_asset::RenderAssetUsages, render_resource::Extent3d};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn real_asset(
    ) -> Result<(std::path::PathBuf, FlipbookAtlasMeta, Image), Box<dyn std::error::Error>> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let catalog =
            sim_core::catalog::RuntimeCatalogs::load_from_directory(root.join("catalog"))?;
        let archetype = catalog
            .units
            .archetypes()
            .find(|a| a.showcase_hero)
            .ok_or("real showcase media required")?;
        let meta = FlipbookAtlasMeta::load_from_ron(
            &root
                .join("textures")
                .join(format!("{}.flipbook.ron", archetype.slug)),
        )?;
        let data = image::open(
            root.join("textures")
                .join(format!("{}.png", archetype.slug)),
        )?
        .to_rgba8()
        .into_raw();
        let image = Image::new(
            Extent3d {
                width: meta.frame_width,
                height: meta.frame_height,
                depth_or_array_layers: meta.total_frames,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        Ok((root, meta, image))
    }

    #[test]
    fn real_pair_matches_actual_array_and_changed_bytes_or_dimensions_fail() -> TestResult {
        let (root, meta, mut image) = real_asset()?;
        validate_source(&root, &meta.name, &meta, &image)?;
        image.data[0] ^= 1;
        assert!(matches!(
            validate_source(&root, &meta.name, &meta, &image),
            Err(CaptureError::MediaMismatch { .. })
        ));
        image.data[0] ^= 1;
        image.texture_descriptor.size.height += 1;
        assert!(matches!(
            validate_source(&root, &meta.name, &meta, &image),
            Err(CaptureError::MediaMismatch { .. })
        ));
        Ok(())
    }

    #[test]
    fn exact_paths_reject_missing_image_malformed_metadata_and_false_identity() -> TestResult {
        let (_, meta, image) = real_asset()?;
        let temp = tempfile::tempdir()?;
        std::fs::create_dir(temp.path().join("textures"))?;
        let path = temp
            .path()
            .join("textures")
            .join(format!("{}.flipbook.ron", meta.name));
        meta.save_to_ron(&path)?;
        assert!(matches!(
            validate_source(temp.path(), &meta.name, &meta, &image),
            Err(CaptureError::MediaImage { .. })
        ));
        std::fs::write(&path, "malformed metadata")?;
        assert!(matches!(
            validate_source(temp.path(), &meta.name, &meta, &image),
            Err(CaptureError::MediaMetadata { .. })
        ));
        let mut wrong = meta.clone();
        wrong.name.push_str("-wrong");
        wrong.save_to_ron(&path)?;
        assert!(matches!(
            validate_source(temp.path(), &meta.name, &meta, &image),
            Err(CaptureError::MediaMismatch { .. })
        ));
        Ok(())
    }

    #[test]
    fn actual_material_handle_is_checked_even_after_successful_cache_entry() -> TestResult {
        let (root, meta, image) = real_asset()?;
        let mut images = Assets::<Image>::default();
        let expected = images.add(image.clone());
        let wrong = images.add(image.clone());
        let mut cache = ValidatedMedia::default();
        for material in [&expected, &wrong] {
            let result = cache.validate(
                &root,
                MediaBinding {
                    unit_type: UnitTypeId(7),
                    slug: &meta.name,
                    registered_texture: &expected,
                    material_texture: material,
                    metadata: &meta,
                    image: &image,
                },
            );
            if material == &expected {
                result?;
            } else {
                assert!(matches!(result, Err(CaptureError::MediaMismatch { .. })));
            }
        }
        Ok(())
    }
}
