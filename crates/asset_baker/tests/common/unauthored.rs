//! A copy of the shipped data in which one identity has no authored prompts.
//!
//! Every shipped identity has authored `veo_prompts` since the Veo requests of batch 2
//! (`docs/media/veo-video-requests-batch-2.md`). Tests of the display-name fallback, and of
//! the refusal to pay for it, therefore make the identity they need: they copy the shipped
//! `catalog/` and `media/` directories into a temporary bundle and empty that one identity's
//! `veo_prompts` map there. The shipped files are never written.
//!
//! Invariant relied on: no prompt holds a brace, because the style's `placeholder_markers`
//! forbid `{` and `}` in every text, so an identity's `veo_prompts` map ends at the first `}`
//! after it opens.

use std::path::Path;

/// Returns `text` (a unit or structure catalog file) with the `veo_prompts` map of the entry
/// `slug: "<slug>"` emptied, or `None` if the file holds no such entry.
///
/// Panics if the entry has no `veo_prompts` map before the next entry starts: the catalog
/// changed shape and this helper must be updated.
#[must_use]
pub fn without_prompts(text: &str, slug: &str) -> Option<String> {
    let entry = text.find(&format!("slug: \"{slug}\""))?;
    let next_entry = text[entry + 1..]
        .find("slug: \"")
        .map_or(text.len(), |offset| entry + 1 + offset);
    let key = "veo_prompts: {";
    let open = entry
        + text[entry..next_entry]
            .find(key)
            .unwrap_or_else(|| panic!("the entry of {slug} has no veo_prompts map"));
    let close = open
        + text[open..]
            .find('}')
            .unwrap_or_else(|| panic!("the veo_prompts map of {slug} is not closed"));
    Some(format!("{}veo_prompts: {{}}{}", &text[..open], &text[close + 1..]))
}

/// A temporary bundle (its root holds `catalog/` and `media/`) that is the shipped data with
/// every prompt of `slug` removed.
///
/// Panics if `slug` is in neither catalog file.
#[must_use]
pub fn bundle_without_prompts(slug: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let shipped = super::cli::shipped_assets();
    for sub in ["catalog", "media"] {
        std::fs::create_dir_all(dir.path().join(sub)).expect("bundle dir");
        for entry in std::fs::read_dir(shipped.join(sub)).expect("shipped dir") {
            let entry = entry.expect("entry");
            std::fs::copy(entry.path(), dir.path().join(sub).join(entry.file_name()))
                .expect("copy data file");
        }
    }
    let mut edited = false;
    for file in ["catalog/units.ron", "catalog/structures.ron"] {
        let path = dir.path().join(file);
        let text = std::fs::read_to_string(&path).expect("catalog file");
        if let Some(text) = without_prompts(&text, slug) {
            std::fs::write(&path, text).expect("write catalog file");
            edited = true;
        }
    }
    assert!(edited, "{slug} is in neither catalog file");
    dir
}

/// The unit catalog of `bundle`, loaded the way the baker loads it.
#[must_use]
pub fn catalog_of(bundle: &Path) -> sim_core::UnitCatalog {
    asset_baker::load_catalogs(bundle)
        .expect("edited catalogs load")
        .units
}
