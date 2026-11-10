//! `asset_baker effect-bake` and `asset_baker effect-noise`: recipes into texture sets (plan
//! `2026-11-09-effect-visualization.md` §6.4).
//!
//! # Responsibility
//! - [`EffectBakeRecipe`] is a set's bake recipe: its cell size, erosion parameters and every
//!   element with its look (emissive or lit) and source (a Veo clip, a generated still, or a
//!   black/white pair). [`bake_recipe`] keys, centres, packs and mips them into a
//!   [`BakedSet`] whose provenance cites the ledger line of every source.
//! - [`bake_noise`] turns a [`NoiseRecipe`] into a set of authored sheets.
//!
//! # How it fits
//! The CLI loads a recipe, calls one of these and writes the result with
//! [`crate::effects::texture_set::write_set`]. Sources are read from a sources directory
//! (`--sources-dir`), named by bare file names, and must each have a ledger line: a clip a
//! `downloaded` line, a still an `image_downloaded` line. A source without one is refused,
//! never shipped with an invented origin.
//!
//! # Invariants
//! - Keying follows the source and look: a pair is always difference-matted; a single still or
//!   clip is luminance-keyed, emissive or lit. A pair cannot be emissive.
//! - Every picture on black passes the black check before it is keyed.
//! - A clip is decoded once by the existing FFmpeg importer (centre crop at
//!   `min(2 x cell_size, 720)` pixels), and its frames are chosen over the effect span.

use super::flipbook::{select_span, SpanRecord};
use super::keying::{
    check_black, difference_matte, luminance_emissive, luminance_key, measure_levels,
    BlackCheckReport, KeyedElement,
};
use super::noise::{generate_sheet, NoiseRecipe, EFFECT_NOISE_TOOL};
use super::packing::{centre_into_cells, mip_chain, pack_cell, strip, ErosionParams, MipFilter};
use super::texture_set::{
    BakedSet, ChannelLayout, EffectElement, EffectSetProvenance, EffectTextureSet, ElementBlend,
    ElementOrigin, ElementProvenance, GeneratedSource, ImageRole, KeyingMethod, KeyingRecord,
    EFFECT_FORMAT_VERSION,
};
use super::{check_name, open_rgba, read_file, EffectError};
use crate::ffmpeg_importer::{decode_framed_clip, FrameSpec};
use crate::framing::BakeFraming;
use crate::ledger::{sha256_hex, LedgerEvent, NumberedRecord};
use crate::provenance::{ffmpeg_version, veo_origin_by_clip_sha256, ClipOrigin};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Cell sizes a baked set may use (plan §6.4).
pub const BAKED_CELL_SIZES: [u32; 2] = [256, 512];

/// Largest side a clip is decoded at, in pixels (Veo clips are 720 p).
pub const MAX_DECODE_SIZE: u32 = 720;

/// How an element is drawn, which chooses its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementLook {
    /// Emissive (fire, energy, glow): luminance key, drawn additively.
    Emissive,
    /// Lit and semi-transparent (smoke, dust, goo, stains): difference matte from a pair, else
    /// the luminance key fallback; drawn with premultiplied alpha.
    Lit,
}

/// Where an element's pictures come from. File names are bare names in the sources directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementSource {
    /// A Veo clip on black; the flipbook keeps `frames` frames of its effect span, decoded at
    /// `decode_fps`.
    Clip {
        /// The clip file.
        file: String,
        /// Frames the flipbook keeps.
        frames: u32,
        /// Decode rate, frames per second.
        decode_fps: u32,
    },
    /// One generated still on black.
    Still {
        /// The image file.
        file: String,
    },
    /// The same element generated on black and, by an edit request, on white.
    Pair {
        /// The image on black.
        black: String,
        /// The image on white.
        white: String,
    },
}

/// One element of a bake recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementRecipe {
    /// Element name.
    pub name: String,
    /// How it is drawn.
    pub look: ElementLook,
    /// Its source.
    pub source: ElementSource,
}

/// A bake recipe file, `<set>.bake.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectBakeRecipe {
    /// Set name.
    pub set: String,
    /// Cell side in pixels, one of [`BAKED_CELL_SIZES`].
    pub cell_size: u32,
    /// How the erosion order is derived; the default when absent.
    #[serde(default)]
    pub erosion: Option<ErosionParams>,
    /// The elements, in layer order.
    pub elements: Vec<ElementRecipe>,
}

impl EffectBakeRecipe {
    /// Loads and checks a recipe.
    ///
    /// # Errors
    /// [`EffectError::Io`], [`EffectError::Parse`], or what [`Self::check`] refuses.
    pub fn load(path: &Path) -> Result<Self, EffectError> {
        let recipe: Self = super::texture_set::load_ron(path)?;
        recipe.check(path)?;
        Ok(recipe)
    }

    /// Checks names, the cell size, duplicate elements and that no pair is emissive.
    ///
    /// # Errors
    /// [`EffectError::InvalidName`] or [`EffectError::InvalidRecipe`] naming `path`.
    pub fn check(&self, path: &Path) -> Result<(), EffectError> {
        check_name("set", &self.set)?;
        let invalid = |reason: String| EffectError::InvalidRecipe {
            path: path.to_path_buf(),
            reason,
        };
        if !BAKED_CELL_SIZES.contains(&self.cell_size) {
            return Err(invalid(format!(
                "cell_size {} is not one of {BAKED_CELL_SIZES:?}",
                self.cell_size
            )));
        }
        if self.elements.is_empty() {
            return Err(invalid("the recipe has no elements".to_string()));
        }
        let mut seen = BTreeSet::new();
        for element in &self.elements {
            check_name("element", &element.name)?;
            if !seen.insert(element.name.as_str()) {
                return Err(invalid(format!("element '{}' appears twice", element.name)));
            }
            if matches!(element.source, ElementSource::Pair { .. })
                && element.look == ElementLook::Emissive
            {
                return Err(invalid(format!(
                    "element '{}': a black/white pair is for lit elements; an emissive element is keyed by luminance from its picture on black",
                    element.name
                )));
            }
            if let ElementSource::Clip { frames: 0, .. }
            | ElementSource::Clip { decode_fps: 0, .. } = element.source
            {
                return Err(invalid(format!(
                    "element '{}': a clip needs frames and decode_fps above 0",
                    element.name
                )));
            }
        }
        Ok(())
    }
}

/// What a bake reads besides the recipe.
#[derive(Debug, Clone, Copy)]
pub struct BakeInputs<'a> {
    /// Directory holding every source file the recipe names.
    pub sources_dir: &'a Path,
    /// The media ledger, with line numbers.
    pub ledger: &'a [NumberedRecord],
    /// The FFmpeg executable.
    pub ffmpeg: &'a Path,
}

/// One element after keying, before packing.
struct KeyedSource {
    pictures: Vec<KeyedElement>,
    provenance: ElementProvenance,
    blend: ElementBlend,
    duration_ms: Option<u32>,
}

/// Bakes a recipe into a set: keys every element, centres it into cells, packs and mips them.
///
/// # Errors
/// [`EffectError::OriginMissing`] for a source without a ledger line,
/// [`EffectError::BlackCheckFailed`], [`EffectError::MatteRejected`],
/// [`EffectError::EmptyElement`], [`EffectError::TooManyLayers`], and I/O, image and FFmpeg
/// failures.
pub fn bake_recipe(
    recipe: &EffectBakeRecipe,
    inputs: BakeInputs<'_>,
) -> Result<BakedSet, EffectError> {
    recipe.check(Path::new(&recipe.set))?;
    let erosion = recipe.erosion.unwrap_or_default();
    let mut cells = Vec::new();
    let mut elements = Vec::new();
    let mut records = BTreeMap::new();
    let mut decoded_clip = false;
    for element in &recipe.elements {
        decoded_clip |= matches!(element.source, ElementSource::Clip { .. });
        let keyed = key_element(element, recipe.cell_size, inputs)?;
        let centred = centre_into_cells(&keyed.pictures, recipe.cell_size, &element.name)?;
        let half = recipe.cell_size / 2;
        elements.push(EffectElement {
            name: element.name.clone(),
            first_layer: layer_count(&cells),
            frames: layer_count(&centred),
            duration_ms: keyed.duration_ms,
            pivot_px: [half, half],
            blend: keyed.blend,
            channel_labels: None,
        });
        cells.extend(centred.iter().map(|cell| pack_cell(cell, &erosion)));
        records.insert(element.name.clone(), keyed.provenance);
    }
    check_layers(&recipe.set, layer_count(&cells), recipe.cell_size)?;
    let strips = mip_chain(&cells, MipFilter::Packed)
        .iter()
        .map(|level| strip(level))
        .collect();
    Ok(BakedSet {
        manifest: EffectTextureSet {
            format_version: EFFECT_FORMAT_VERSION,
            set: recipe.set.clone(),
            cell_size: recipe.cell_size,
            padding_px: super::packing::PADDING_PX,
            channels: ChannelLayout::PackedV1,
            layers: layer_count(&cells),
            mip_files: Vec::new(),
            elements,
        },
        provenance: EffectSetProvenance {
            format_version: EFFECT_FORMAT_VERSION,
            set: recipe.set.clone(),
            manifest_sha256: String::new(),
            mip_sha256: BTreeMap::new(),
            erosion: Some(erosion),
            ffmpeg_version: if decoded_clip {
                Some(ffmpeg_version(inputs.ffmpeg)?)
            } else {
                None
            },
            elements: records,
        },
        strips,
    })
}

/// A count of cells as a layer count (a strip never holds 2^32 cells).
fn layer_count<T>(cells: &[T]) -> u32 {
    u32::try_from(cells.len()).unwrap_or(u32::MAX)
}

/// Refuses a set whose strip would exceed the texture height limit.
fn check_layers(set: &str, layers: u32, cell_size: u32) -> Result<(), EffectError> {
    let strip = crate::prompts::MAX_ATLAS_STRIP_HEIGHT;
    let max = strip / cell_size.max(1);
    if layers > max {
        return Err(EffectError::TooManyLayers {
            set: set.to_string(),
            layers,
            cell_size,
            max,
            strip,
        });
    }
    Ok(())
}

/// Keys one element's source.
fn key_element(
    element: &ElementRecipe,
    cell_size: u32,
    inputs: BakeInputs<'_>,
) -> Result<KeyedSource, EffectError> {
    let blend = match element.look {
        ElementLook::Emissive => ElementBlend::Additive,
        ElementLook::Lit => ElementBlend::Premultiplied,
    };
    match &element.source {
        ElementSource::Clip {
            file,
            frames,
            decode_fps,
        } => key_clip(
            element,
            (file, *frames, *decode_fps),
            cell_size,
            inputs,
            blend,
        ),
        ElementSource::Still { file } => {
            let path = inputs.sources_dir.join(file);
            let picture = open_rgba(&path)?;
            let source = generated_source(inputs.ledger, element, file, &path, ImageRole::OnBlack)?;
            let black = check_black(&picture, &element.name, None)?;
            let levels = measure_levels(&[&picture], &element.name)?;
            let (keyed, method) = match element.look {
                ElementLook::Emissive => (
                    luminance_emissive(&picture, levels),
                    KeyingMethod::LuminanceEmissive,
                ),
                ElementLook::Lit => (luminance_key(&picture, levels), KeyingMethod::LuminanceKey),
            };
            Ok(KeyedSource {
                pictures: vec![keyed],
                provenance: ElementProvenance {
                    origin: ElementOrigin::GeneratedImage {
                        sources: vec![source],
                    },
                    keying: KeyingRecord {
                        method,
                        levels: Some(levels),
                        black_check: Some(black),
                        matte: None,
                    },
                    span: None,
                },
                blend,
                duration_ms: None,
            })
        }
        ElementSource::Pair { black, white } => key_pair(element, (black, white), inputs, blend),
    }
}

/// Keys a black/white pair by the difference matte.
fn key_pair(
    element: &ElementRecipe,
    (black, white): (&String, &String),
    inputs: BakeInputs<'_>,
    blend: ElementBlend,
) -> Result<KeyedSource, EffectError> {
    let black_path = inputs.sources_dir.join(black);
    let white_path = inputs.sources_dir.join(white);
    let on_black = open_rgba(&black_path)?;
    let on_white = open_rgba(&white_path)?;
    let sources = vec![
        generated_source(
            inputs.ledger,
            element,
            black,
            &black_path,
            ImageRole::OnBlack,
        )?,
        generated_source(
            inputs.ledger,
            element,
            white,
            &white_path,
            ImageRole::OnWhite,
        )?,
    ];
    let black_check = check_black(&on_black, &element.name, None)?;
    let (keyed, matte) = difference_matte(&on_black, &on_white, &element.name)?;
    Ok(KeyedSource {
        pictures: vec![keyed],
        provenance: ElementProvenance {
            origin: ElementOrigin::GeneratedImage { sources },
            keying: KeyingRecord {
                method: KeyingMethod::DifferenceMatte,
                levels: None,
                black_check: Some(black_check),
                matte: Some(matte),
            },
            span: None,
        },
        blend,
        duration_ms: None,
    })
}

/// Decodes a clip, chooses its effect span and keys every kept frame.
fn key_clip(
    element: &ElementRecipe,
    (file, frames, decode_fps): (&String, u32, u32),
    cell_size: u32,
    inputs: BakeInputs<'_>,
    blend: ElementBlend,
) -> Result<KeyedSource, EffectError> {
    let path = inputs.sources_dir.join(file);
    let sha256 = sha256_hex(&read_file(&path)?);
    let origin = match veo_origin_by_clip_sha256(inputs.ledger, &sha256) {
        Some(ClipOrigin::Veo {
            model,
            operation,
            prompt_sha256,
            keyframe_sha256,
            ledger_line,
            ..
        }) => ElementOrigin::Veo {
            clip: file.clone(),
            clip_sha256: sha256,
            model,
            operation,
            prompt_sha256,
            keyframe_sha256,
            ledger_line,
        },
        _ => {
            return Err(EffectError::OriginMissing {
                element: element.name.clone(),
                file: file.clone(),
                sha256,
                wanted: "downloaded",
            })
        }
    };
    let spec = FrameSpec {
        size: (2 * cell_size).min(MAX_DECODE_SIZE),
        fps: decode_fps,
        framing: BakeFraming::CenterCrop,
    };
    let decoded = decode_framed_clip(inputs.ffmpeg, &path, spec, usize::MAX, &element.name)?;
    let span = select_span(&decoded.frames, frames, decode_fps, &element.name)?;
    let kept: Vec<&RgbaImage> = span
        .kept
        .iter()
        .filter_map(|index| decoded.frames.get(*index as usize))
        .collect();
    let (pictures, keying) = key_frames(element, &kept, &span)?;
    Ok(KeyedSource {
        pictures,
        provenance: ElementProvenance {
            origin,
            keying,
            span: Some(span.clone()),
        },
        blend,
        duration_ms: Some(span.duration_ms),
    })
}

/// Black-checks and keys the kept frames of a clip with one shared set of levels.
fn key_frames(
    element: &ElementRecipe,
    kept: &[&RgbaImage],
    span: &SpanRecord,
) -> Result<(Vec<KeyedElement>, KeyingRecord), EffectError> {
    let mut worst: Option<BlackCheckReport> = None;
    for (frame, index) in kept.iter().zip(&span.kept) {
        let report = check_black(frame, &element.name, Some(*index as usize))?;
        worst = Some(worst.map_or(report, |w| w.worst(report)));
    }
    let levels = measure_levels(kept, &element.name)?;
    let (pictures, method) = match element.look {
        ElementLook::Emissive => (
            kept.iter().map(|f| luminance_emissive(f, levels)).collect(),
            KeyingMethod::LuminanceEmissive,
        ),
        ElementLook::Lit => (
            kept.iter().map(|f| luminance_key(f, levels)).collect(),
            KeyingMethod::LuminanceKey,
        ),
    };
    Ok((
        pictures,
        KeyingRecord {
            method,
            levels: Some(levels),
            black_check: worst,
            matte: None,
        },
    ))
}

/// The generated source a still's bytes came from: the last `image_downloaded` line of these
/// bytes and the start of its request.
///
/// # Errors
/// [`EffectError::OriginMissing`] when the ledger has no such lines, and [`EffectError::Io`].
pub fn generated_source(
    ledger: &[NumberedRecord],
    element: &ElementRecipe,
    file: &str,
    path: &Path,
    role: ImageRole,
) -> Result<GeneratedSource, EffectError> {
    let sha256 = sha256_hex(&read_file(path)?);
    let missing = || EffectError::OriginMissing {
        element: element.name.clone(),
        file: file.to_string(),
        sha256: sha256.clone(),
        wanted: "image_downloaded",
    };
    let (ledger_line, request_id) = ledger
        .iter()
        .rev()
        .find_map(|numbered| match &numbered.record.event {
            LedgerEvent::ImageDownloaded {
                sha256: s,
                request_id,
                ..
            } if *s == sha256 => Some((numbered.line, request_id.clone())),
            _ => None,
        })
        .ok_or_else(missing)?;
    let (model, prompt_sha256) = ledger
        .iter()
        .find_map(|numbered| match &numbered.record.event {
            LedgerEvent::ImageRequestStarted {
                request_id: r,
                model,
                prompt_sha256,
                ..
            } if *r == request_id => Some((model.clone(), prompt_sha256.clone())),
            _ => None,
        })
        .ok_or_else(missing)?;
    Ok(GeneratedSource {
        role,
        file: file.to_string(),
        sha256,
        model,
        prompt_sha256,
        request_id,
        ledger_line,
    })
}

/// Bakes a noise recipe into a set of authored sheets: one cell per element, no padding, data
/// mips.
///
/// # Errors
/// What [`NoiseRecipe::check`] refuses, and [`EffectError::TooManyLayers`].
pub fn bake_noise(recipe: &NoiseRecipe) -> Result<BakedSet, EffectError> {
    recipe.check(&PathBuf::from(&recipe.set))?;
    let half = recipe.cell_size / 2;
    let mut cells = Vec::new();
    let mut elements = Vec::new();
    let mut records = BTreeMap::new();
    for spec in &recipe.elements {
        elements.push(EffectElement {
            name: spec.name.clone(),
            first_layer: layer_count(&cells),
            frames: 1,
            duration_ms: None,
            pivot_px: [half, half],
            blend: ElementBlend::Data,
            channel_labels: Some(spec.channels.all().map(|c| c.label().to_string())),
        });
        cells.push(generate_sheet(&spec.channels, recipe.cell_size));
        records.insert(
            spec.name.clone(),
            ElementProvenance {
                origin: ElementOrigin::Authored {
                    tool: EFFECT_NOISE_TOOL.to_string(),
                    params: spec.clone(),
                },
                keying: KeyingRecord {
                    method: KeyingMethod::Authored,
                    levels: None,
                    black_check: None,
                    matte: None,
                },
                span: None,
            },
        );
    }
    check_layers(&recipe.set, layer_count(&cells), recipe.cell_size)?;
    let strips = mip_chain(&cells, MipFilter::Data)
        .iter()
        .map(|level| strip(level))
        .collect();
    Ok(BakedSet {
        manifest: EffectTextureSet {
            format_version: EFFECT_FORMAT_VERSION,
            set: recipe.set.clone(),
            cell_size: recipe.cell_size,
            padding_px: 0,
            channels: ChannelLayout::NoiseV1,
            layers: layer_count(&cells),
            mip_files: Vec::new(),
            elements,
        },
        provenance: EffectSetProvenance {
            format_version: EFFECT_FORMAT_VERSION,
            set: recipe.set.clone(),
            manifest_sha256: String::new(),
            mip_sha256: BTreeMap::new(),
            erosion: None,
            ffmpeg_version: None,
            elements: records,
        },
        strips,
    })
}
