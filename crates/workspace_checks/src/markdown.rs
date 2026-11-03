//! Reading the parts of a Markdown document that the charter checks judge.
//!
//! # Responsibility
//! Splits a document into numbered lines that know whether they sit inside a fenced code block,
//! finds a `##` section by its number, and extracts inline code spans, link targets and table
//! cells. It is not a Markdown parser: it understands exactly the constructs `AGENTS.md`,
//! `DESIGN.md` and the plan index use.
//!
//! # How it fits
//! The checks of `AGENTS.md` (C1, C15, C16, C20) and of `DESIGN.md` (C17) all read Markdown. They
//! share these helpers so that one rule decides, for every check, what counts as prose, what is
//! inside a code block and where a section ends.
//!
//! # Invariants
//! - Line numbers are one-based and refer to the real file, so findings point at real lines.
//! - A fence opens with a line starting with three backticks and closes with the next such line;
//!   the fence lines themselves count as inside the block.
//! - Inline code spans and links are read only outside fenced blocks.

/// One line of a Markdown document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MarkdownLine<'a> {
    /// One-based line number in the document.
    pub(crate) number: usize,
    /// The line's text, without its line break.
    pub(crate) text: &'a str,
    /// Whether the line is a fence line or lies inside a fenced code block.
    pub(crate) fenced: bool,
}

/// The lines of `text`, each marked as fenced or not.
pub(crate) fn lines(text: &str) -> Vec<MarkdownLine<'_>> {
    let mut open = false;
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            let fence = line.trim_start().starts_with("```");
            let fenced = open || fence;
            if fence {
                open = !open;
            }
            MarkdownLine {
                number: index + 1,
                text: line,
                fenced,
            }
        })
        .collect()
}

/// The lines of the level-2 section whose heading starts with `## {number}.`, heading included,
/// up to the next level-2 heading outside a code block. Empty if there is no such section.
pub(crate) fn section<'a>(all: &[MarkdownLine<'a>], number: &str) -> Vec<MarkdownLine<'a>> {
    let heading = format!("## {number}.");
    let Some(start) = all
        .iter()
        .position(|line| !line.fenced && line.text.starts_with(&heading))
    else {
        return Vec::new();
    };
    let end = all[start + 1..]
        .iter()
        .position(|line| !line.fenced && line.text.starts_with("## "))
        .map_or(all.len(), |offset| start + 1 + offset);
    all[start..end].to_vec()
}

/// The inline code spans of `line` (text between single backticks), in order.
///
/// An unmatched backtick ends the scan; a double-backtick span is read as two empty spans and
/// one plain run, which no charter document uses.
pub(crate) fn code_spans(line: &str) -> Vec<&str> {
    let mut spans = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else {
            break;
        };
        spans.push(&after[..close]);
        rest = &after[close + 1..];
    }
    spans
}

/// The targets of the inline links `[text](target)` on `line`, in order, outside code spans.
pub(crate) fn link_targets(line: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    let mut in_code = false;
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'`' => in_code = !in_code,
            b']' if !in_code && bytes.get(index + 1) == Some(&b'(') => {
                let start = index + 2;
                if let Some(length) = line[start..].find(')') {
                    targets.push(&line[start..start + length]);
                    index = start + length;
                }
            }
            _ => {}
        }
        index += 1;
    }
    targets
}

/// The cells of a table row `| a | b |`, trimmed, or `None` if `line` is not a table row.
pub(crate) fn table_cells(line: &str) -> Option<Vec<&str>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return None;
    }
    Some(
        trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect(),
    )
}

/// Whether every cell of a table row is a separator such as `---` or `:--:`.
pub(crate) fn is_separator_row(cells: &[&str]) -> bool {
    cells
        .iter()
        .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_mark_their_own_lines_and_their_contents() {
        let marked: Vec<bool> = lines("a\n```rust\nb\n```\nc\n")
            .iter()
            .map(|line| line.fenced)
            .collect();
        assert_eq!(marked, [false, true, true, true, false]);
    }

    #[test]
    fn a_section_ends_at_the_next_level_two_heading_outside_code() {
        let text = "# T\n## 1. One\nx\n```\n## not a heading\n```\n### sub\n## 2. Two\ny\n";
        let all = lines(text);
        let one: Vec<usize> = section(&all, "1").iter().map(|line| line.number).collect();
        assert_eq!(one, [2, 3, 4, 5, 6, 7]);
        let two: Vec<&str> = section(&all, "2").iter().map(|line| line.text).collect();
        assert_eq!(two, ["## 2. Two", "y"]);
        assert_eq!(section(&all, "3"), Vec::new());
    }

    #[test]
    fn code_spans_and_links_are_read_in_order() {
        let line = "see `a/b.rs` and [x](docs/y.md#z), not `[q](r)`, then `c` and [w](u)";
        assert_eq!(code_spans(line), ["a/b.rs", "[q](r)", "c"]);
        assert_eq!(link_targets(line), ["docs/y.md#z", "u"]);
        assert_eq!(code_spans("odd ` tick"), Vec::<&str>::new());
    }

    #[test]
    fn table_rows_split_into_trimmed_cells() {
        assert_eq!(table_cells("| a | `b` |"), Some(vec!["a", "`b`"]));
        assert_eq!(table_cells("not a row"), None);
        assert!(is_separator_row(&["---", ":--:"]));
        assert!(!is_separator_row(&["---", "x"]));
    }
}
