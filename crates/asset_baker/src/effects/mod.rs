//! The effect-texture pipeline (plan `docs/plans/2026-11-09-effect-visualization.md` §6, WP5).
//!
//! # Responsibility
//! Turns sources into the greyscale, channel-packed texture sets the client's effect system
//! samples, each with a manifest and a provenance record:
//! - [`noise`]: offline authored, deterministic, tileable noise and erosion sheets
//!   (`asset_baker effect-noise`). These are authored assets (plan §6.2 item 3), not stand-ins
//!   for media, and their provenance is `Authored { tool, params }`.
//! - [`keying`]: the black check, the luminance key for emissive elements, the difference matte
//!   from a black/white pair and the luminance key fallback for lit semi-transparent elements.
//! - [`packing`]: erosion order, centroid-centred cells, the RGBA8 channel packing and the mip
//!   chain.
//! - [`flipbook`]: choosing a clip's effect span (onset to dissipation) and its frames.
//! - [`texture_set`]: the on-disk format (`<set>.texture.ron`, `<set>.provenance.ron`, one PNG
//!   per mip level) and its verification.
//! - [`bake`]: `asset_baker effect-bake`, a recipe of elements and their sources into a set.
//! - [`review`]: `asset_baker effect-review`, the texture contact sheets of §6.5.
//! - [`prompt`]: the prompt class "effect element" of `media_style.ron`.
//! - [`image_client`]: the Gemini image-model client on the Veo credential path.
//!
//! # How it fits
//! Generated stills come from [`image_client`] (every request in the media ledger, priced by
//! `media/veo_budget.ron`), Veo clips from the existing Veo pipeline, noise from [`noise`]. The
//! baker writes sets under `assets/effects/`; `bake_integrity_tests.rs` verifies that every
//! file there belongs to a set whose every element has a real origin.
//!
//! # Invariants
//! - Every shipped element has an origin that can be checked: a ledger line for a Veo clip or
//!   a generated image, or the recorded parameters that regenerate an authored noise sheet.
//! - Every computation is deterministic: the same sources and parameters give the same bytes.
//! - Library code returns [`EffectError`]; nothing here panics on bad input.

pub mod bake;
pub mod flipbook;
pub mod image_client;
pub mod keying;
pub mod noise;
pub mod packing;
pub mod prompt;
pub mod review;
pub mod texture_set;

use crate::error::BakerError;
use std::path::PathBuf;

/// Failures of the effect-texture pipeline.
#[derive(Debug, thiserror::Error)]
pub enum EffectError {
    /// A file could not be read or written.
    #[error("effect I/O error at '{path}': {source}")]
    Io {
        /// The file or directory.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// An image could not be decoded or encoded.
    #[error("effect image error at '{path}': {source}")]
    Image {
        /// The image file.
        path: PathBuf,
        /// Underlying image error.
        #[source]
        source: image::ImageError,
    },
    /// A RON file could not be parsed.
    #[error("cannot parse '{path}': {source}")]
    Parse {
        /// The file.
        path: PathBuf,
        /// Parser error.
        #[source]
        source: ron::error::SpannedError,
    },
    /// A record could not be encoded as RON.
    #[error("cannot encode {what}: {source}")]
    Encode {
        /// What was being encoded.
        what: String,
        /// Encoder error.
        #[source]
        source: ron::Error,
    },
    /// A set or element name cannot name a portable file.
    #[error("invalid {what} name '{name}': use 1 to 64 characters of a-z, 0-9 and '_', starting with a letter")]
    InvalidName {
        /// `set` or `element`.
        what: &'static str,
        /// The refused name.
        name: String,
    },
    /// A recipe or command asks for something the pipeline cannot do.
    #[error("invalid effect recipe '{path}': {reason}")]
    InvalidRecipe {
        /// The recipe file (or the command, for a flag).
        path: PathBuf,
        /// What is wrong.
        reason: String,
    },
    /// A source picture is not on pure black (plan §6.4 black check).
    #[error("element '{element}' fails the black check{}: {reason}", frame_note(*.frame))]
    BlackCheckFailed {
        /// The element.
        element: String,
        /// The decoded clip frame, for a clip.
        frame: Option<usize>,
        /// Which rule it broke.
        reason: String,
    },
    /// The two pictures of a difference-matte pair disagree too much (plan §6.4).
    #[error("element '{element}': the black/white pair is rejected, the matte residual exceeds 4/255 on {over} of {total} pixels (limit 0.5 %; largest residual {max_residual}/255)")]
    MatteRejected {
        /// The element.
        element: String,
        /// Pixels whose residual exceeds 4/255.
        over: u64,
        /// Pixels compared.
        total: u64,
        /// Largest residual, in 1/255.
        max_residual: u8,
    },
    /// The two pictures of a pair have different sizes.
    #[error("element '{element}': the picture on black is {black:?} and the one on white {white:?}; a pair must match")]
    PairSizeMismatch {
        /// The element.
        element: String,
        /// Width and height of the picture on black.
        black: (u32, u32),
        /// Width and height of the picture on white.
        white: (u32, u32),
    },
    /// An element has nothing to pack: no lit pixel, or no frame of its clip shows it.
    #[error("element '{element}' is empty: {reason}")]
    EmptyElement {
        /// The element.
        element: String,
        /// Why.
        reason: String,
    },
    /// A source has no ledger line that records where it came from, so it cannot ship.
    #[error("element '{element}': source '{file}' (SHA-256 {sha256}) has no {wanted} line in the ledger; only media the pipeline generated (or authored noise) can enter an effect set")]
    OriginMissing {
        /// The element.
        element: String,
        /// The source file name.
        file: String,
        /// Its SHA-256.
        sha256: String,
        /// The ledger line it needs (`downloaded`, `image_downloaded`).
        wanted: &'static str,
    },
    /// The set has more layers than one texture strip can hold.
    #[error("set '{set}' needs {layers} layers of {cell_size} px, more than the {max} a {strip} px strip holds")]
    TooManyLayers {
        /// The set.
        set: String,
        /// Layers needed.
        layers: u32,
        /// Cell side in pixels.
        cell_size: u32,
        /// Layers that fit.
        max: u32,
        /// Strip height limit in pixels.
        strip: u32,
    },
    /// A manifest or provenance file is inconsistent.
    #[error("invalid effect texture set '{path}': {reason}")]
    InvalidSet {
        /// The manifest or provenance file.
        path: PathBuf,
        /// What is wrong.
        reason: String,
    },
    /// An effect prompt breaks the prompt rules of `media_style.ron`; nothing was sent.
    #[error("the effect prompt is not acceptable; nothing was sent: {}", issues_text(.issues))]
    PromptInvalid {
        /// Every issue found.
        issues: Vec<crate::prompts::PromptIssue>,
    },
    /// The image model answered without an image.
    #[error("the image model returned no image: {reason}")]
    ImageNotReturned {
        /// What the response said instead (finish or block reason), redacted.
        reason: String,
    },
    /// A transport, ffmpeg or other baker failure.
    #[error(transparent)]
    Baker(Box<BakerError>),
    /// The ledger could not be read or written.
    #[error(transparent)]
    Ledger(#[from] crate::ledger::LedgerError),
    /// The budget refused the request.
    #[error(transparent)]
    Budget(#[from] crate::budget::BudgetError),
    /// The FFmpeg version could not be recorded.
    #[error(transparent)]
    Provenance(#[from] crate::provenance::ProvenanceError),
}

impl From<BakerError> for EffectError {
    fn from(error: BakerError) -> Self {
        Self::Baker(Box::new(error))
    }
}

/// ` (decoded frame N)` for a clip frame, nothing for a still.
fn frame_note(frame: Option<usize>) -> String {
    frame.map_or_else(String::new, |frame| format!(" (decoded frame {frame})"))
}

/// The issues of a refused prompt, separated by semicolons.
fn issues_text(issues: &[crate::prompts::PromptIssue]) -> String {
    issues
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

/// Longest set or element name accepted.
pub const MAX_NAME_CHARS: usize = 64;

/// Accepts a set or element name that can name a portable file: 1 to [`MAX_NAME_CHARS`]
/// characters of `a-z`, `0-9` and `_`, starting with a letter.
///
/// # Errors
/// [`EffectError::InvalidName`] naming `what`.
pub fn check_name(what: &'static str, name: &str) -> Result<(), EffectError> {
    let valid = !name.is_empty()
        && name.len() <= MAX_NAME_CHARS
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(EffectError::InvalidName {
            what,
            name: name.to_string(),
        })
    }
}

/// Reads a whole file, naming it in the error.
///
/// # Errors
/// [`EffectError::Io`].
pub(crate) fn read_file(path: &std::path::Path) -> Result<Vec<u8>, EffectError> {
    std::fs::read(path).map_err(|source| EffectError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Writes `bytes` to `path` through a temporary file in the same directory and a rename, so a
/// reader never sees a partial file.
///
/// # Errors
/// [`EffectError::Io`].
pub(crate) fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<(), EffectError> {
    let io = |source| EffectError::Io {
        path: path.to_path_buf(),
        source,
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(parent).map_err(io)?;
    let temp = tempfile::NamedTempFile::new_in(parent).map_err(io)?;
    std::fs::write(temp.path(), bytes).map_err(io)?;
    temp.persist(path).map_err(|error| io(error.error))?;
    Ok(())
}

/// Encodes `image` as PNG bytes.
///
/// # Errors
/// [`EffectError::Image`] naming `path`, the file the bytes are meant for.
pub(crate) fn png_bytes(
    image: &image::RgbaImage,
    path: &std::path::Path,
) -> Result<Vec<u8>, EffectError> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|source| EffectError::Image {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(bytes.into_inner())
}

/// Decodes the image at `path` as RGBA8.
///
/// # Errors
/// [`EffectError::Image`].
pub(crate) fn open_rgba(path: &std::path::Path) -> Result<image::RgbaImage, EffectError> {
    image::open(path)
        .map(|image| image.to_rgba8())
        .map_err(|source| EffectError::Image {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_portable_lowercase_slugs() {
        for good in ["fx_blast", "a", "smoke_puff_2"] {
            assert!(check_name("set", good).is_ok(), "{good}");
        }
        let long = "a".repeat(MAX_NAME_CHARS + 1);
        for bad in [
            "",
            "Fx",
            "2fx",
            "fx-blast",
            "fx/blast",
            "..",
            "fx blast",
            long.as_str(),
        ] {
            assert!(
                matches!(check_name("set", bad), Err(EffectError::InvalidName { .. })),
                "{bad}"
            );
        }
    }
}
