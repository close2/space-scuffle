//! Text edits of the shipped `catalog/media_style.ron` for tests that load a changed copy.
//!
//! Tests that check how the loader treats an override replace one section's `overrides`
//! map with their own. The shipped data may hold overrides of its own (batch 1 added some
//! for one identity), so the edit replaces the whole map, whatever it holds, instead of
//! looking for an empty `overrides: {}`.
//!
//! Invariant relied on: the shipped style holds no brace inside a string, because its
//! `placeholder_markers` forbid `{` and `}` in every text. Counting braces therefore finds
//! the end of a map exactly.

/// Returns `text` with the `overrides` map of the section that starts at the first
/// occurrence of `section` (for example `"    framing: ("`) replaced by `replacement`, the
/// RON text of the new map (for example `"{}"` or `"{ \"slug\": CenterCrop }"`).
///
/// Panics with a message naming the section when it, or its `overrides` map, is missing
/// or unbalanced: the shipped file changed shape and the test must be updated.
pub fn with_section_overrides(text: &str, section: &str, replacement: &str) -> String {
    let start = text
        .find(section)
        .unwrap_or_else(|| panic!("the shipped style has no section {section:?}"));
    let key = "overrides: ";
    let at = start
        + text[start..]
            .find(&format!("{key}{{"))
            .unwrap_or_else(|| panic!("section {section:?} has no overrides map"));
    let open = at + key.len();
    let mut depth = 0_usize;
    for (offset, byte) in text.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let close = open + offset + 1;
                    return format!("{}{key}{replacement}{}", &text[..at], &text[close..]);
                }
            }
            _ => {}
        }
    }
    panic!("the overrides map of section {section:?} is not closed")
}
