//! Exact CPU-side admission of one authored flipbook pair.
//!
//! A slug selects only its own PNG and RON. Metadata, decoded dimensions and action
//! ranges are validated together; no missing identity or action receives substitute
//! pixels. Digests describe the same immutable read buffers that were parsed/decoded,
//! and are content evidence rather than a claim of artistic approval.

use crate::{BakerError, FlipbookAtlasMeta, FlipbookLayout, Morph, SliceKey};
use sim_types::AnimState;
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

/// Optional limits from the actual rendering device, measured in pixels/layers.
#[derive(Debug, Clone, Copy)]
pub struct TextureLimits {
    /// Maximum width or height of a 2D image (also applies to the portrait strip).
    pub max_dimension_2d: u32,
    /// Maximum number of layers in the sampled texture array.
    pub max_array_layers: u32,
}

/// A precise failure to admit an explicitly named media pair.
#[derive(Debug, thiserror::Error)]
pub enum MediaLoadError {
    /// A slug cannot name a portable file within the selected texture directory.
    #[error("invalid media filename identity {slug:?}")]
    InvalidSlug {
        /// Rejected authored identity.
        slug: String,
    },
    /// A required file could not be read.
    #[error("cannot read media {path}: {source}")]
    Io {
        /// Exact selected path.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// Metadata could not be parsed or validated.
    #[error("invalid media metadata {path}: {source}")]
    Metadata {
        /// Exact selected metadata path.
        path: PathBuf,
        /// Parser/validation cause.
        #[source]
        source: BakerError,
    },
    /// The PNG decoder rejected the real selected bytes.
    #[error("invalid media image {path}: {source}")]
    Image {
        /// Exact selected image path.
        path: PathBuf,
        /// Decoder cause.
        #[source]
        source: image::ImageError,
    },
    /// The pair cannot represent the requested identity or supported texture format.
    #[error("invalid media representation at {path}: {reason}")]
    Representation {
        /// Exact offending metadata/image path.
        path: PathBuf,
        /// Violated representation invariant.
        reason: String,
    },
    /// A requested action was not authored; another action is never substituted.
    #[error("media {path} has no authored {action:?} action")]
    MissingAction {
        /// Exact metadata path.
        path: PathBuf,
        /// Requested gameplay animation.
        action: AnimState,
    },
    /// A mode atlas lacks one of its morphs; the default atlas is never substituted.
    #[error("mode media {path} has no authored {morph:?} morph")]
    MissingMorph {
        /// Exact metadata path.
        path: PathBuf,
        /// The missing morph.
        morph: Morph,
    },
}

/// Structurally admitted own-file media, before any GPU allocation.
#[derive(Debug)]
pub struct LoadedUnitMedia {
    /// Validated identity, dimensions and authored action ranges.
    pub metadata: FlipbookAtlasMeta,
    /// Decoded RGBA8 pixels in vertical-strip/layer order.
    pub rgba: Vec<u8>,
    /// Full strip height in pixels, checked against multiplication overflow.
    pub strip_height: u32,
    /// Lowercase BLAKE3 digest of the PNG bytes that were actually decoded.
    pub png_digest: String,
    /// Lowercase BLAKE3 digest of the RON bytes that were actually parsed.
    pub metadata_digest: String,
    /// Minimum/maximum observed alpha, without inventing transparency.
    pub alpha_range: [u8; 2],
}

/// Resolves one portable own-file identity without interpreting it as a path.
/// Accepts ASCII letters, digits, underscores and hyphens, excluding Windows
/// device basenames. This deliberately narrower policy is identical on all hosts.
///
/// # Errors
/// Returns [`MediaLoadError::InvalidSlug`] for traversal, separators or blank names.
pub fn media_paths(directory: &Path, slug: &str) -> Result<(PathBuf, PathBuf), MediaLoadError> {
    // Windows rejects reserved basenames even with extensions; enforce the same
    // identity contract on every host so a published bundle remains portable.
    let basename = slug.to_ascii_uppercase();
    let reserved = matches!(basename.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            basename.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
        });
    if slug.is_empty()
        || reserved
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(MediaLoadError::InvalidSlug {
            slug: slug.to_owned(),
        });
    }
    Ok((
        directory.join(format!("{slug}.png")),
        directory.join(format!("{slug}.flipbook.ron")),
    ))
}

/// Loads one exact PNG/RON pair from `directory` (the texture directory itself).
///
/// `limits` comes from the actual GPU when used for rendering; CPU audits pass
/// `None` and make no device-compatibility claim. Image decoding retains the image
/// crate's allocation limits. No source file is rewritten and no alias is tried.
///
/// # Errors
/// Returns an exact-path typed failure for I/O, metadata, decoding, dimensions,
/// arithmetic overflow or device limits. Missing actions are checked on request.
pub fn load_unit_media(
    directory: &Path,
    slug: &str,
    limits: Option<TextureLimits>,
) -> Result<LoadedUnitMedia, MediaLoadError> {
    let (png_path, metadata_path) = media_paths(directory, slug)?;
    let metadata_bytes = read(&metadata_path)?;
    let metadata: FlipbookAtlasMeta =
        ron::de::from_bytes(&metadata_bytes).map_err(|source| MediaLoadError::Metadata {
            path: metadata_path.clone(),
            source: BakerError::RonDeError(source),
        })?;
    metadata
        .validate()
        .map_err(|source| MediaLoadError::Metadata {
            path: metadata_path.clone(),
            source,
        })?;
    let invalid_meta = |reason: &str| MediaLoadError::Representation {
        path: metadata_path.clone(),
        reason: reason.into(),
    };
    if metadata.name != slug {
        return Err(invalid_meta(
            "metadata identity differs from requested slug",
        ));
    }
    if metadata.layout != FlipbookLayout::VerticalStrip {
        return Err(invalid_meta("runtime supports vertical strips only"));
    }
    if metadata.actions.is_empty() {
        return Err(invalid_meta("at least one explicit action is required"));
    }
    for action in metadata.actions.values() {
        if action.start_frame.checked_add(action.frame_count).is_none() {
            return Err(invalid_meta("action frame range overflows u32"));
        }
    }
    let strip_height = metadata
        .frame_height
        .checked_mul(metadata.total_frames)
        .ok_or_else(|| invalid_meta("strip height overflows u32"))?;
    let expected_bytes = u64::from(metadata.frame_width)
        .checked_mul(u64::from(strip_height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or_else(|| invalid_meta("decoded byte count exceeds addressable memory"))?;
    if let Some(limits) = limits {
        if metadata.frame_width > limits.max_dimension_2d
            || strip_height > limits.max_dimension_2d
            || metadata.total_frames > limits.max_array_layers
        {
            return Err(invalid_meta(
                "array or portrait dimensions exceed the actual device limits",
            ));
        }
    }
    let png_bytes = read(&png_path)?;
    let decode_error = |source| MediaLoadError::Image {
        path: png_path.clone(),
        source,
    };
    let dimensions =
        image::ImageReader::with_format(Cursor::new(&png_bytes), image::ImageFormat::Png)
            .into_dimensions()
            .map_err(decode_error)?;
    if dimensions != (metadata.frame_width, strip_height) {
        return Err(MediaLoadError::Representation {
            path: png_path,
            reason: format!(
                "PNG dimensions {dimensions:?} differ from authored {}x{strip_height}",
                metadata.frame_width
            ),
        });
    }
    let rgba = image::ImageReader::with_format(Cursor::new(&png_bytes), image::ImageFormat::Png)
        .decode()
        .map_err(decode_error)?
        .into_rgba8()
        .into_raw();
    if rgba.len() != expected_bytes {
        return Err(invalid_meta(
            "decoded RGBA byte count differs from authored dimensions",
        ));
    }
    let alpha_range = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .fold([u8::MAX, 0], |[low, high], pixel| {
            [low.min(pixel[3]), high.max(pixel[3])]
        });
    Ok(LoadedUnitMedia {
        metadata,
        rgba,
        strip_height,
        png_digest: blake3::hash(&png_bytes).to_hex().to_string(),
        metadata_digest: blake3::hash(&metadata_bytes).to_hex().to_string(),
        alpha_range,
    })
}

/// Rejects a missing explicit action without falling back to another slice.
///
/// # Errors
/// Returns [`MediaLoadError::MissingAction`] naming the requested metadata/action.
pub fn require_action(
    metadata: &FlipbookAtlasMeta,
    path: &Path,
    action: AnimState,
) -> Result<(), MediaLoadError> {
    if metadata.actions.contains_key(&action) {
        Ok(())
    } else {
        Err(MediaLoadError::MissingAction {
            path: path.to_owned(),
            action,
        })
    }
}

/// Rejects a mode atlas that lacks any of the `required` slices (plan
/// `2026-11-09-effect-visualization.md` WP13; [`crate::mode_media::required_mode_slices`]),
/// naming the first missing one in [`SliceKey`] order.
///
/// # Errors
/// [`MediaLoadError::MissingAction`] or [`MediaLoadError::MissingMorph`] naming the metadata.
pub fn require_slices(
    metadata: &FlipbookAtlasMeta,
    path: &Path,
    required: &[SliceKey],
) -> Result<(), MediaLoadError> {
    required.iter().try_for_each(|slot| match slot {
        SliceKey::Action(action) => require_action(metadata, path, *action),
        SliceKey::Morph(morph) if metadata.morphs.contains_key(morph) => Ok(()),
        SliceKey::Morph(morph) => Err(MediaLoadError::MissingMorph {
            path: path.to_owned(),
            morph: *morph,
        }),
    })
}

fn read(path: &Path) -> Result<Vec<u8>, MediaLoadError> {
    std::fs::read(path).map_err(|source| MediaLoadError::Io {
        path: path.to_owned(),
        source,
    })
}
