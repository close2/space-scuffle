//! The published site: the player page and the provenance record.
//!
//! # Responsibility
//! Plan `docs/plans/2026-11-05-ci-showcase-video.md`, decision D-9. [`render_page`] fills the
//! static template `crates/client_diagnostics/data/showcase_site.html` (a `<video>` player and the
//! provenance as text, no JavaScript); [`Provenance`] is `showcase.json`: what was rendered, where,
//! how long it took, and the size and SHA-256 of every file.
//!
//! # Invariants
//! - Every placeholder is replaced, and every inserted text is HTML-escaped; a template with a
//!   placeholder left over is an error, not a page.
//! - The template is part of this crate, not of `assets/` (check C6).

use super::encode::{POSTER_FILE, PROVENANCE_FILE, TEASER_FILE, VIDEO_FILE};
use super::recorder::SceneRecord;
use super::ShowcaseError;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::path::Path;

/// The player page template.
pub const TEMPLATE: &str = include_str!("../../data/showcase_site.html");

/// What the page says about the rendered video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageFacts {
    /// The pitch from `DESIGN.md` §1.
    pub pitch: String,
    /// Commit rendered.
    pub revision: String,
    /// Commit date.
    pub revision_date: String,
    /// Adapter of the first take.
    pub adapter: String,
    /// The terrain anisotropy override, if one was set.
    pub anisotropy: Option<String>,
    /// The CI run, if rendered by CI.
    pub run_url: Option<String>,
    /// Scenes of every take, in order.
    pub scenes: Vec<SceneRecord>,
}

/// Escapes `&`, `<`, `>`, `"` and `'` for HTML text and attribute values.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Fills `template` with `facts`.
///
/// # Errors
/// [`ShowcaseError::Encode`] if a `{{` placeholder remains.
pub fn render_page(template: &str, facts: &PageFacts) -> Result<String, ShowcaseError> {
    let scenes = facts
        .scenes
        .iter()
        .map(|scene| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                escape(&scene.name),
                scene.frames,
                scene.start_tick
            )
        })
        .collect::<Vec<_>>()
        .concat();
    let run = facts.run_url.as_ref().map_or_else(
        || "local render".to_owned(),
        |url| format!("<a href=\"{0}\">{0}</a>", escape(url)),
    );
    let page = template
        .replace("{{PITCH}}", &escape(&facts.pitch))
        .replace("{{VIDEO}}", VIDEO_FILE)
        .replace("{{TEASER}}", TEASER_FILE)
        .replace("{{POSTER}}", POSTER_FILE)
        .replace("{{JSON}}", PROVENANCE_FILE)
        .replace("{{REVISION}}", &escape(&facts.revision))
        .replace("{{REVISION_DATE}}", &escape(&facts.revision_date))
        .replace("{{ADAPTER}}", &escape(&facts.adapter))
        .replace(
            "{{ANISOTROPY}}",
            &escape(facts.anisotropy.as_deref().unwrap_or("none")),
        )
        .replace("{{RUN}}", &run)
        .replace("{{SCENES}}", &scenes);
    if let Some(at) = page.find("{{") {
        let end = page[at..].find("}}").map_or(page.len(), |end| at + end + 2);
        return Err(ShowcaseError::Encode {
            step: "page".to_owned(),
            reason: format!("placeholder {} is not filled", &page[at..end]),
        });
    }
    Ok(page)
}

/// Lowercase hexadecimal of `bytes`.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

/// Size and SHA-256 of one published file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    /// File name in the site directory.
    pub name: String,
    /// Bytes.
    pub bytes: u64,
    /// SHA-256, lowercase hexadecimal.
    pub sha256: String,
}

impl FileRecord {
    /// Reads `path` and records its size and digest.
    ///
    /// # Errors
    /// [`ShowcaseError::Io`].
    pub fn of(path: &Path) -> Result<Self, ShowcaseError> {
        let bytes = std::fs::read(path).map_err(|source| ShowcaseError::Io {
            path: path.to_owned(),
            source,
        })?;
        let digest = sha2::Sha256::digest(&bytes);
        Ok(Self {
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            bytes: bytes.len() as u64,
            sha256: hex(&digest),
        })
    }
}

/// One take in the provenance record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakeRecord {
    /// Take name.
    pub name: String,
    /// Adapter name.
    pub adapter: String,
    /// Driver details.
    pub driver_info: String,
    /// Mesa `major.minor`, if the driver is Mesa.
    pub mesa_release: Option<String>,
    /// The terrain anisotropy override, if set.
    pub anisotropy_override: Option<String>,
    /// Frames rendered.
    pub frames: u64,
    /// Seconds to readiness.
    pub readiness_seconds: f32,
    /// Seconds rendering frames.
    pub render_seconds: f32,
    /// Median wall time per frame, milliseconds.
    pub frame_wall_ms_p50: f32,
    /// 95th percentile wall time per frame, milliseconds.
    pub frame_wall_ms_p95: f32,
    /// Scenes as rendered.
    pub scenes: Vec<SceneRecord>,
}

/// `showcase.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// Commit rendered.
    pub revision: String,
    /// Commit date.
    pub revision_date: String,
    /// The CI run, if any.
    pub run_url: Option<String>,
    /// The `ffmpeg -version` first line.
    pub encoder: String,
    /// Teaser width and quality the ladder kept.
    pub teaser: (u32, u32),
    /// Every attempt of the teaser ladder.
    pub teaser_attempts: Vec<String>,
    /// The takes.
    pub takes: Vec<TakeRecord>,
    /// The published media files.
    pub files: Vec<FileRecord>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_covers_markup_and_quotes() {
        assert_eq!(escape("a<b>&\"c'"), "a&lt;b&gt;&amp;&quot;c&#39;");
    }

    #[test]
    fn a_left_placeholder_is_an_error() {
        let facts = PageFacts {
            pitch: String::new(),
            revision: String::new(),
            revision_date: String::new(),
            adapter: String::new(),
            anisotropy: None,
            run_url: None,
            scenes: vec![],
        };
        let result = render_page("<p>{{UNKNOWN}}</p>", &facts);
        assert!(
            matches!(&result, Err(ShowcaseError::Encode { reason, .. }) if reason.contains("{{UNKNOWN}}")),
            "{result:?}"
        );
    }
}
