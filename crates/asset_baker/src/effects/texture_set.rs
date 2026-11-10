//! The on-disk format of an effect texture set and its verification (plan
//! `2026-11-09-effect-visualization.md` §6.4).
//!
//! # Format
//! A set `<set>` is four or more files in one directory (`assets/effects/` when shipped):
//! - `<set>.png`, `<set>.mip1.png`, ... : one vertical strip per mip level. Every layer is a
//!   square cell (`cell_size >> level` pixels); the client reads each strip as a texture
//!   array with one layer per cell. Level 0 is `cell_size` (256 or 512 for baked sets); each
//!   next level halves the side down to 16 pixels. Mips are computed per cell
//!   ([`crate::effects::packing::mip_chain`]), so cells never bleed into each other.
//! - `<set>.texture.ron`: the manifest, [`EffectTextureSet`]: cell size, padding, layer count,
//!   the mip files and every element with its first layer, frame count, play time, pivot,
//!   blend and channel meaning.
//! - `<set>.provenance.ron`: [`EffectSetProvenance`]: the SHA-256 of the manifest and of every
//!   strip, and per element its [`ElementOrigin`] (a Veo clip, generated images, or authored
//!   noise with its parameters), source hashes, keying method, levels, black check and matte
//!   residual.
//!
//! Texel layout of a packed set (one RGBA8 texel, the same for every set so one shader path
//! serves all): R shading, G density (alpha), B erosion order (low erodes first), A emissive
//! mask. A noise set stores four independent noise channels named in the element's
//! `channel_labels`.
//!
//! # Verification
//! [`verify_set`] lists everything that keeps a set from being shippable: missing or changed
//! files, elements without provenance, origins without the ledger line they cite, and authored
//! sheets whose recorded parameters no longer reproduce their pixels. `bake_integrity_tests.rs`
//! requires an empty list for every set under `assets/effects/` and no other files there.
//!
//! # Invariants
//! - Written RON ends every line in `\n` on every host.
//! - File names inside a manifest are bare names in the set's directory, never paths.

use super::flipbook::SpanRecord;
use super::keying::{BlackCheckReport, Levels, MatteReport};
use super::noise::{generate_sheet, NoiseElementSpec, EFFECT_NOISE_TOOL};
use super::packing::ErosionParams;
use super::{check_name, EffectError};
use crate::ledger::{sha256_hex, LedgerEvent, NumberedRecord};
use ron::ser::PrettyConfig;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Version of the manifest and provenance formats written by this module.
pub const EFFECT_FORMAT_VERSION: u32 = 1;

/// Suffix of a manifest file.
pub const MANIFEST_SUFFIX: &str = ".texture.ron";

/// Suffix of a provenance file.
pub const PROVENANCE_SUFFIX: &str = ".provenance.ron";

/// What the texels of a set mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelLayout {
    /// R shading, G density (alpha), B erosion order, A emissive mask.
    PackedV1,
    /// Four independent noise channels, named per element.
    NoiseV1,
}

/// How the client blends an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementBlend {
    /// Emissive: drawn additively (premultiplied with alpha 0), coloured by the ramp.
    Additive,
    /// Lit, semi-transparent: premultiplied alpha.
    Premultiplied,
    /// Data sampled by other elements' shaders, never drawn on its own.
    Data,
}

/// One element of a set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectElement {
    /// Element name.
    pub name: String,
    /// First layer (cell) of the element in every strip.
    pub first_layer: u32,
    /// Number of consecutive layers: flipbook frames, or 1 for a still.
    pub frames: u32,
    /// Real play time of all frames, in milliseconds; `None` for a still or a noise sheet.
    pub duration_ms: Option<u32>,
    /// Pivot in cell pixels from the top-left corner: the element's centroid for a baked
    /// element (the cell centre), the cell centre for a noise sheet.
    pub pivot_px: [u32; 2],
    /// How the client blends it.
    pub blend: ElementBlend,
    /// For a noise sheet, the content of R, G, B and A; `None` for a packed element.
    pub channel_labels: Option<[String; 4]>,
}

/// The manifest `<set>.texture.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectTextureSet {
    /// [`EFFECT_FORMAT_VERSION`] when written.
    pub format_version: u32,
    /// Set name.
    pub set: String,
    /// Side of every level-0 cell, in pixels.
    pub cell_size: u32,
    /// Empty border around each baked element inside its cell, in level-0 pixels (0 for
    /// noise sheets, which fill their cell and tile).
    pub padding_px: u32,
    /// What the texels mean.
    pub channels: ChannelLayout,
    /// Number of layers (cells) in every strip.
    pub layers: u32,
    /// The strip file of each mip level, level 0 first.
    pub mip_files: Vec<String>,
    /// The elements, in layer order.
    pub elements: Vec<EffectElement>,
}

/// Which picture of a generated element a source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageRole {
    /// The element on black (the only picture of a luminance-keyed still).
    OnBlack,
    /// The element on white, the second picture of a difference-matte pair.
    OnWhite,
}

/// One generated image an element was keyed from, as the ledger records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedSource {
    /// Which picture this is.
    pub role: ImageRole,
    /// File name of the source image.
    pub file: String,
    /// SHA-256 of the source image, lowercase hex.
    pub sha256: String,
    /// Model identifier of the request.
    pub model: String,
    /// SHA-256 of the request's prompt, lowercase hex.
    pub prompt_sha256: String,
    /// The request identifier in the ledger.
    pub request_id: String,
    /// One-based line of the ledger's `image_downloaded` record of this image.
    pub ledger_line: usize,
}

/// Where an element came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementOrigin {
    /// A Veo clip this pipeline generated, cited by its `downloaded` ledger line.
    Veo {
        /// File name of the clip.
        clip: String,
        /// SHA-256 of the clip, lowercase hex.
        clip_sha256: String,
        /// Model identifier of the request.
        model: String,
        /// Operation that produced the clip.
        operation: String,
        /// SHA-256 of the request's prompt, lowercase hex.
        prompt_sha256: String,
        /// SHA-256 of the keyframe sent, if any.
        keyframe_sha256: Option<String>,
        /// One-based line of the ledger's `downloaded` record of this clip.
        ledger_line: usize,
    },
    /// One generated image, or a black/white pair, from the image model.
    GeneratedImage {
        /// The pictures, the one on black first.
        sources: Vec<GeneratedSource>,
    },
    /// An authored sheet regenerated from its recorded parameters.
    Authored {
        /// The tool, [`EFFECT_NOISE_TOOL`].
        tool: String,
        /// The parameters.
        params: NoiseElementSpec,
    },
}

/// How an element was keyed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyingMethod {
    /// Luminance key, emissive (additive).
    LuminanceEmissive,
    /// Difference matte from a black/white pair.
    DifferenceMatte,
    /// Luminance key fallback for a lit element without a pair.
    LuminanceKey,
    /// Not keyed: an authored sheet.
    Authored,
}

/// The keying record of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyingRecord {
    /// The method.
    pub method: KeyingMethod,
    /// Levels of a luminance key.
    pub levels: Option<Levels>,
    /// The worst black check over the element's pictures on black.
    pub black_check: Option<BlackCheckReport>,
    /// The matte residual of a pair.
    pub matte: Option<MatteReport>,
}

/// The provenance of one element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementProvenance {
    /// Where it came from.
    pub origin: ElementOrigin,
    /// How it was keyed.
    pub keying: KeyingRecord,
    /// For a flipbook, the frames kept from its clip.
    pub span: Option<SpanRecord>,
}

/// The provenance record `<set>.provenance.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectSetProvenance {
    /// [`EFFECT_FORMAT_VERSION`] when written.
    pub format_version: u32,
    /// Set name.
    pub set: String,
    /// SHA-256 of the manifest, lowercase hex.
    pub manifest_sha256: String,
    /// SHA-256 of every strip file, by file name.
    pub mip_sha256: BTreeMap<String, String>,
    /// The erosion parameters of a packed set; `None` for a noise set.
    pub erosion: Option<ErosionParams>,
    /// First line of `ffmpeg -version`, when a clip was decoded.
    pub ffmpeg_version: Option<String>,
    /// Every element, by name.
    pub elements: BTreeMap<String, ElementProvenance>,
}

/// The manifest and provenance paths of `set` in `dir`.
///
/// # Errors
/// [`EffectError::InvalidName`] for a set name that cannot name a portable file.
pub fn set_paths(dir: &Path, set: &str) -> Result<(PathBuf, PathBuf), EffectError> {
    check_name("set", set)?;
    Ok((
        dir.join(format!("{set}{MANIFEST_SUFFIX}")),
        dir.join(format!("{set}{PROVENANCE_SUFFIX}")),
    ))
}

/// The strip file name of mip `level` of `set`: `<set>.png`, then `<set>.mip<level>.png`.
#[must_use]
pub fn mip_file_name(set: &str, level: usize) -> String {
    if level == 0 {
        format!("{set}.png")
    } else {
        format!("{set}.mip{level}.png")
    }
}

/// `value` as pretty RON with `\n` line ends and a final newline.
///
/// # Errors
/// [`EffectError::Encode`] naming `what`.
pub fn to_ron_text<T: Serialize>(value: &T, what: &str) -> Result<String, EffectError> {
    ron::ser::to_string_pretty(value, PrettyConfig::default().new_line("\n".to_string()))
        .map(|text| text + "\n")
        .map_err(|source| EffectError::Encode {
            what: what.to_string(),
            source,
        })
}

/// Reads a RON file.
///
/// # Errors
/// [`EffectError::Io`] or [`EffectError::Parse`].
pub fn load_ron<T: DeserializeOwned>(path: &Path) -> Result<T, EffectError> {
    let text = std::fs::read_to_string(path).map_err(|source| EffectError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    ron::from_str(&text).map_err(|source| EffectError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

/// A set ready to be written: its strips per mip level, manifest and provenance (whose
/// hashes [`write_set`] fills in).
#[derive(Debug, Clone)]
pub struct BakedSet {
    /// The manifest; `mip_files` is filled in by [`write_set`].
    pub manifest: EffectTextureSet,
    /// The provenance; the hashes are filled in by [`write_set`].
    pub provenance: EffectSetProvenance,
    /// One strip per mip level, level 0 first.
    pub strips: Vec<image::RgbaImage>,
}

/// Writes a set into `dir`: every strip, then the manifest, then the provenance, each through
/// a temporary file and a rename, so a provenance file exists only beside complete strips.
/// Returns the manifest and provenance as written.
///
/// # Errors
/// [`EffectError::InvalidName`], [`EffectError::Io`], [`EffectError::Image`] or
/// [`EffectError::Encode`].
pub fn write_set(
    dir: &Path,
    mut baked: BakedSet,
) -> Result<(EffectTextureSet, EffectSetProvenance), EffectError> {
    let (manifest_path, provenance_path) = set_paths(dir, &baked.manifest.set)?;
    let set = baked.manifest.set.clone();
    baked.manifest.mip_files.clear();
    baked.provenance.mip_sha256.clear();
    for (level, strip) in baked.strips.iter().enumerate() {
        let name = mip_file_name(&set, level);
        let path = dir.join(&name);
        let bytes = super::png_bytes(strip, &path)?;
        super::write_atomic(&path, &bytes)?;
        baked
            .provenance
            .mip_sha256
            .insert(name.clone(), sha256_hex(&bytes));
        baked.manifest.mip_files.push(name);
    }
    let manifest_text = to_ron_text(&baked.manifest, "effect texture manifest")?;
    super::write_atomic(&manifest_path, manifest_text.as_bytes())?;
    baked.provenance.manifest_sha256 = sha256_hex(manifest_text.as_bytes());
    let provenance_text = to_ron_text(&baked.provenance, "effect provenance")?;
    super::write_atomic(&provenance_path, provenance_text.as_bytes())?;
    Ok((baked.manifest, baked.provenance))
}

/// Checks a manifest's own consistency: names, layer ranges, mip files.
///
/// # Errors
/// [`EffectError::InvalidSet`] or [`EffectError::InvalidName`].
pub fn check_manifest(manifest: &EffectTextureSet, path: &Path) -> Result<(), EffectError> {
    let invalid = |reason: String| EffectError::InvalidSet {
        path: path.to_path_buf(),
        reason,
    };
    check_name("set", &manifest.set)?;
    if manifest.format_version != EFFECT_FORMAT_VERSION {
        return Err(invalid(format!(
            "format_version {} is not {EFFECT_FORMAT_VERSION}",
            manifest.format_version
        )));
    }
    if manifest.mip_files.is_empty() || manifest.cell_size == 0 {
        return Err(invalid("no mip level or a zero cell size".to_string()));
    }
    for (level, name) in manifest.mip_files.iter().enumerate() {
        if *name != mip_file_name(&manifest.set, level) {
            return Err(invalid(format!(
                "mip {level} is '{name}', not the set's own name"
            )));
        }
    }
    let mut next = 0_u32;
    for element in &manifest.elements {
        check_name("element", &element.name)?;
        if element.first_layer != next || element.frames == 0 {
            return Err(invalid(format!(
                "element '{}' starts at layer {} with {} frames; elements must tile the layers in order",
                element.name, element.first_layer, element.frames
            )));
        }
        next = next.saturating_add(element.frames);
    }
    if next != manifest.layers {
        return Err(invalid(format!(
            "the elements cover {next} layers, the manifest says {}",
            manifest.layers
        )));
    }
    Ok(())
}

/// One thing that keeps a set from being shippable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetProblem {
    /// The manifest or provenance cannot be read or is inconsistent.
    Unreadable(String),
    /// A recorded hash does not match the file.
    HashMismatch {
        /// The file.
        file: String,
    },
    /// A strip has the wrong size for its level.
    StripSize {
        /// The file.
        file: String,
    },
    /// An element of the manifest has no provenance, or the other way round.
    ElementMismatch {
        /// The element.
        element: String,
    },
    /// An element's origin cites no matching ledger line.
    OriginUnverified {
        /// The element.
        element: String,
        /// Why.
        reason: String,
    },
    /// An authored sheet does not reproduce from its parameters.
    AuthoredMismatch {
        /// The element.
        element: String,
    },
}

/// Everything that keeps the set `set` in `dir` from being shippable (see the module docs);
/// empty for a verified set.
#[must_use]
pub fn verify_set(dir: &Path, set: &str, ledger: &[NumberedRecord]) -> Vec<SetProblem> {
    let (manifest_path, provenance_path) = match set_paths(dir, set) {
        Ok(paths) => paths,
        Err(error) => return vec![SetProblem::Unreadable(error.to_string())],
    };
    let loaded = load_ron::<EffectTextureSet>(&manifest_path).and_then(|manifest| {
        check_manifest(&manifest, &manifest_path)?;
        let provenance = load_ron::<EffectSetProvenance>(&provenance_path)?;
        Ok((manifest, provenance))
    });
    let (manifest, provenance) = match loaded {
        Ok(pair) => pair,
        Err(error) => return vec![SetProblem::Unreadable(error.to_string())],
    };
    let mut problems = verify_files(dir, &manifest, &provenance, &manifest_path);
    let names: Vec<&String> = manifest.elements.iter().map(|e| &e.name).collect();
    for name in provenance.elements.keys().filter(|n| !names.contains(n)) {
        problems.push(SetProblem::ElementMismatch {
            element: name.clone(),
        });
    }
    for element in &manifest.elements {
        let Some(record) = provenance.elements.get(&element.name) else {
            problems.push(SetProblem::ElementMismatch {
                element: element.name.clone(),
            });
            continue;
        };
        if let Some(problem) = verify_origin(dir, &manifest, element, &record.origin, ledger) {
            problems.push(problem);
        }
    }
    problems
}

/// The hash and size checks of the manifest and every strip.
fn verify_files(
    dir: &Path,
    manifest: &EffectTextureSet,
    provenance: &EffectSetProvenance,
    manifest_path: &Path,
) -> Vec<SetProblem> {
    let mut problems = Vec::new();
    let hash_of = |path: &Path| std::fs::read(path).ok().map(|bytes| sha256_hex(&bytes));
    if hash_of(manifest_path).as_deref() != Some(provenance.manifest_sha256.as_str()) {
        problems.push(SetProblem::HashMismatch {
            file: format!("{}{MANIFEST_SUFFIX}", manifest.set),
        });
    }
    if provenance.set != manifest.set || provenance.mip_sha256.len() != manifest.mip_files.len() {
        problems.push(SetProblem::Unreadable(
            "the provenance names another set or other strips".to_string(),
        ));
    }
    for (level, file) in manifest.mip_files.iter().enumerate() {
        let path = dir.join(file);
        if hash_of(&path).as_ref() != provenance.mip_sha256.get(file) {
            problems.push(SetProblem::HashMismatch { file: file.clone() });
            continue;
        }
        let side = manifest.cell_size >> level;
        let size_ok = image::image_dimensions(&path)
            .is_ok_and(|dims| dims == (side, side.saturating_mul(manifest.layers)));
        if !size_ok {
            problems.push(SetProblem::StripSize { file: file.clone() });
        }
    }
    problems
}

/// The check of one element's origin against the ledger, or its regeneration.
fn verify_origin(
    dir: &Path,
    manifest: &EffectTextureSet,
    element: &EffectElement,
    origin: &ElementOrigin,
    ledger: &[NumberedRecord],
) -> Option<SetProblem> {
    let unverified = |reason: String| {
        Some(SetProblem::OriginUnverified {
            element: element.name.clone(),
            reason,
        })
    };
    match origin {
        ElementOrigin::Veo {
            clip_sha256,
            operation,
            ledger_line,
            ..
        } => match line(ledger, *ledger_line) {
            Some(
                LedgerEvent::Downloaded {
                    sha256,
                    operation: op,
                    ..
                }
                | LedgerEvent::EffectClipDownloaded {
                    sha256,
                    operation: op,
                    ..
                },
            ) if sha256 == clip_sha256 && op == operation => None,
            _ => unverified(format!(
                "ledger line {ledger_line} is not the download of clip {clip_sha256}"
            )),
        },
        ElementOrigin::GeneratedImage { sources } => {
            if sources.is_empty() {
                return unverified("no generated source is recorded".to_string());
            }
            sources
                .iter()
                .find_map(|source| verify_generated(source, ledger).err())
                .and_then(unverified)
        }
        ElementOrigin::Authored { tool, params } => {
            if tool != EFFECT_NOISE_TOOL {
                return unverified(format!("tool '{tool}' is not '{EFFECT_NOISE_TOOL}'"));
            }
            verify_authored(dir, manifest, element, params)
        }
    }
}

/// The ledger event on one-based `line`.
fn line(ledger: &[NumberedRecord], line: usize) -> Option<&LedgerEvent> {
    ledger
        .iter()
        .find(|numbered| numbered.line == line)
        .map(|numbered| &numbered.record.event)
}

/// Checks that a generated source's ledger line downloaded its bytes for its request, and
/// that the request's start recorded its model and prompt.
fn verify_generated(source: &GeneratedSource, ledger: &[NumberedRecord]) -> Result<(), String> {
    match line(ledger, source.ledger_line) {
        Some(LedgerEvent::ImageDownloaded {
            sha256, request_id, ..
        }) if *sha256 == source.sha256 && *request_id == source.request_id => {}
        _ => {
            return Err(format!(
                "ledger line {} is not the image_downloaded line of {} for request {}",
                source.ledger_line, source.sha256, source.request_id
            ))
        }
    }
    let started = ledger.iter().any(|numbered| {
        matches!(&numbered.record.event, LedgerEvent::ImageRequestStarted {
            request_id, model, prompt_sha256, ..
        } if *request_id == source.request_id && *model == source.model
            && *prompt_sha256 == source.prompt_sha256)
    });
    if started {
        Ok(())
    } else {
        Err(format!(
            "no image_request_started line of request {} with model {} and the recorded prompt",
            source.request_id, source.model
        ))
    }
}

/// Regenerates an authored sheet and compares it with its layer of the level-0 strip.
fn verify_authored(
    dir: &Path,
    manifest: &EffectTextureSet,
    element: &EffectElement,
    params: &NoiseElementSpec,
) -> Option<SetProblem> {
    let mismatch = Some(SetProblem::AuthoredMismatch {
        element: element.name.clone(),
    });
    if params.name != element.name || element.frames != 1 {
        return mismatch;
    }
    let Some(first) = manifest.mip_files.first() else {
        return mismatch;
    };
    let Ok(strip) = super::open_rgba(&dir.join(first)) else {
        return mismatch;
    };
    let side = manifest.cell_size;
    let expected = generate_sheet(&params.channels, side);
    let top = element.first_layer.saturating_mul(side);
    let same = strip.height() >= top.saturating_add(side)
        && expected
            .enumerate_pixels()
            .all(|(x, y, pixel)| strip.get_pixel(x, top + y) == pixel);
    if same {
        None
    } else {
        mismatch
    }
}

/// Every set in `dir`: the stems of its `*.texture.ron` files, sorted.
///
/// # Errors
/// [`EffectError::Io`] if `dir` exists but cannot be listed; a missing `dir` has no sets.
pub fn list_sets(dir: &Path) -> Result<Vec<String>, EffectError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(EffectError::Io {
                path: dir.to_path_buf(),
                source,
            })
        }
    };
    let mut sets: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(MANIFEST_SUFFIX))
                .map(ToString::to_string)
        })
        .collect();
    sets.sort();
    Ok(sets)
}

/// The files a set owns in its directory: manifest, provenance and every strip it lists.
///
/// # Errors
/// As [`load_ron`] for the manifest, and [`EffectError::InvalidName`].
pub fn owned_files(dir: &Path, set: &str) -> Result<Vec<String>, EffectError> {
    let (manifest_path, _) = set_paths(dir, set)?;
    let manifest: EffectTextureSet = load_ron(&manifest_path)?;
    let mut files = vec![
        format!("{set}{MANIFEST_SUFFIX}"),
        format!("{set}{PROVENANCE_SUFFIX}"),
    ];
    files.extend(manifest.mip_files);
    Ok(files)
}
