//! The title and status cards of the showcase video, generated from the repository's own documents.
//!
//! # Responsibility
//! Every card text except the hand-written captions is computed here, at run time, from two files
//! (plan `docs/plans/2026-11-05-ci-showcase-video.md`, decision D-7):
//! - `DESIGN.md` §1 gives the pitch (its first sentence) and the "Intent, not built" line;
//! - `DESIGN.md` §4 gives the subsystem table, counted per status for "Where it is";
//! - `docs/plans/README.md` gives the dated `Active` and `Proposed` plans for "Where it is heading".
//!
//! # How it fits
//! [`super::storyboard`] validates captions against [`SubsystemRow`]s, so a caption cannot keep
//! advertising a subsystem that was removed. The overlay paints [`TitleCard`] and [`StatusCard`].
//!
//! # Invariants
//! - Pure functions over text: no file is read here except by [`RepositoryCards::load`].
//! - A missing section, table or column, or a row with an unknown status, is a typed
//!   [`ShowcaseError::Cards`], never an empty card.
//! - The markdown reader is private and small on purpose: `workspace_checks` has its own, in a crate
//!   this one must not depend on (check C1).

use super::ShowcaseError;
use std::path::{Path, PathBuf};

/// Longest plan summary on the status card, in characters.
pub const PLAN_SUMMARY_MAX_CHARS: usize = 70;

/// Most plans listed under "Where it is heading".
pub const MAX_HEADING_PLANS: usize = 4;

/// The project name on the title card.
pub const PROJECT_NAME: &str = "Space Scuffle";

/// The label of the `DESIGN.md` §1 line that names what is intended but not built.
const INTENT_LABEL: &str = "**Intent, not built:**";

/// Status of one `DESIGN.md` §4 row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubsystemStatus {
    /// Built, tested and in the shipped game.
    Working,
    /// Built but limited, or not all shipped.
    Partial,
    /// Not built.
    NotBuilt,
    /// Built once, then deleted.
    Removed,
}

impl SubsystemStatus {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "Working" => Some(Self::Working),
            "Partial" => Some(Self::Partial),
            "Not built" => Some(Self::NotBuilt),
            "Removed" => Some(Self::Removed),
            _ => None,
        }
    }

    /// Whether a caption may name a subsystem with this status.
    #[must_use]
    pub const fn may_be_shown(self) -> bool {
        matches!(self, Self::Working | Self::Partial)
    }
}

/// One row of the `DESIGN.md` §4 subsystem table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubsystemRow {
    /// The subsystem's name, exactly as in the first column.
    pub name: String,
    /// Its status column.
    pub status: SubsystemStatus,
}

/// The opening card: project name, pitch and the revision footer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleCard {
    /// The project's name.
    pub name: String,
    /// The first sentence of `DESIGN.md` §1, markdown emphasis removed.
    pub pitch: String,
}

/// The closing card: "Where it is" and "Where it is heading".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusCard {
    /// Rows per status, in [`SubsystemStatus`] order; statuses without rows and `Removed` are left
    /// out, because the card describes what exists.
    pub counts: Vec<(SubsystemStatus, usize)>,
    /// Names of the `Partial` rows, in table order.
    pub partial: Vec<String>,
    /// Names of the `Not built` rows, in table order.
    pub not_built: Vec<String>,
    /// The first sentence after `**Intent, not built:**` in `DESIGN.md` §1.
    pub intent: String,
    /// Summaries of up to [`MAX_HEADING_PLANS`] dated `Active` or `Proposed` plans, newest first.
    pub plans: Vec<String>,
}

impl StatusCard {
    /// The counts as one line, for example `23 subsystems working, 3 partial, 2 not built`.
    #[must_use]
    pub fn counts_line(&self) -> String {
        let mut parts = Vec::with_capacity(self.counts.len());
        for (index, (status, count)) in self.counts.iter().enumerate() {
            let label = match status {
                SubsystemStatus::Working => "working",
                SubsystemStatus::Partial => "partial",
                SubsystemStatus::NotBuilt => "not built",
                SubsystemStatus::Removed => "removed",
            };
            if index == 0 {
                parts.push(format!("{count} subsystems {label}"));
            } else {
                parts.push(format!("{count} {label}"));
            }
        }
        parts.join(", ")
    }
}

/// Everything the cards and the storyboard validation read from the repository.
#[derive(Debug, Clone)]
pub struct RepositoryCards {
    /// The title card.
    pub title: TitleCard,
    /// The status card.
    pub status: StatusCard,
    /// The `DESIGN.md` §4 rows, for caption validation.
    pub subsystems: Vec<SubsystemRow>,
}

impl RepositoryCards {
    /// Reads `DESIGN.md` and `docs/plans/README.md` below `repository` and builds every card.
    ///
    /// # Errors
    /// [`ShowcaseError::Io`] if a file cannot be read, [`ShowcaseError::Cards`] if a section, table
    /// or row is missing or malformed.
    pub fn load(repository: &Path) -> Result<Self, ShowcaseError> {
        let design_path = repository.join("DESIGN.md");
        let plans_path = repository.join("docs/plans/README.md");
        let design = read(&design_path)?;
        let plans = read(&plans_path)?;
        Self::from_texts(&design_path, &design, &plans_path, &plans)
    }

    /// Builds every card from the two texts; the paths only name the sources in errors.
    ///
    /// # Errors
    /// [`ShowcaseError::Cards`] naming the file whose section, table or row is missing or malformed.
    pub fn from_texts(
        design_path: &Path,
        design: &str,
        plans_path: &Path,
        plans: &str,
    ) -> Result<Self, ShowcaseError> {
        let subsystems =
            subsystem_rows(design).map_err(|reason| cards_error(design_path, reason))?;
        let pitch = pitch(design).map_err(|reason| cards_error(design_path, reason))?;
        let intent = intent(design).map_err(|reason| cards_error(design_path, reason))?;
        let plans = heading_plans(plans).map_err(|reason| cards_error(plans_path, reason))?;
        Ok(Self {
            title: TitleCard {
                name: PROJECT_NAME.to_owned(),
                pitch,
            },
            status: status_card(&subsystems, intent, plans),
            subsystems,
        })
    }
}

fn read(path: &Path) -> Result<String, ShowcaseError> {
    std::fs::read_to_string(path).map_err(|source| ShowcaseError::Io {
        path: path.to_owned(),
        source,
    })
}

fn cards_error(path: &Path, reason: String) -> ShowcaseError {
    ShowcaseError::Cards {
        source_file: PathBuf::from(path),
        reason,
    }
}

/// Counts and lists the rows for the status card.
fn status_card(rows: &[SubsystemRow], intent: String, plans: Vec<String>) -> StatusCard {
    let mut counts = Vec::new();
    for status in [
        SubsystemStatus::Working,
        SubsystemStatus::Partial,
        SubsystemStatus::NotBuilt,
    ] {
        let count = rows.iter().filter(|row| row.status == status).count();
        if count > 0 {
            counts.push((status, count));
        }
    }
    let names = |status| {
        rows.iter()
            .filter(|row| row.status == status)
            .map(|row| row.name.clone())
            .collect()
    };
    StatusCard {
        counts,
        partial: names(SubsystemStatus::Partial),
        not_built: names(SubsystemStatus::NotBuilt),
        intent,
        plans,
    }
}

/// The body of the level-2 section whose heading starts with `## <number>.`, up to the next
/// level-2 heading.
fn section(text: &str, number: u32) -> Result<&str, String> {
    let prefix = format!("## {number}.");
    let mut start = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if let Some(begin) = start {
            if line.starts_with("## ") {
                return Ok(&text[begin..offset]);
            }
        } else if line.starts_with(&prefix) {
            start = Some(offset + line.len());
        }
        offset += line.len();
    }
    start
        .map(|begin| &text[begin..])
        .ok_or_else(|| format!("section {number} (`{prefix}`) is missing"))
}

/// Removes markdown emphasis and code markers and collapses whitespace.
fn plain(text: &str) -> String {
    let stripped: String = text
        .chars()
        .filter(|character| !matches!(character, '*' | '`'))
        .collect();
    let words: Vec<&str> = stripped
        .split_whitespace()
        .map(|word| word.trim_matches('_'))
        .collect();
    words.join(" ")
}

/// The first sentence of `text`: up to and including the first `.` followed by whitespace or the
/// end.
fn first_sentence(text: &str) -> Option<String> {
    let characters: Vec<char> = text.chars().collect();
    for (index, character) in characters.iter().enumerate() {
        if *character == '.'
            && characters
                .get(index + 1)
                .is_none_or(|next| next.is_whitespace())
        {
            return Some(characters[..=index].iter().collect());
        }
    }
    None
}

/// The pitch: the first sentence of the first paragraph of `DESIGN.md` §1.
fn pitch(design: &str) -> Result<String, String> {
    let body = section(design, 1)?;
    let paragraph: Vec<&str> = body
        .lines()
        .skip_while(|line| line.trim().is_empty())
        .take_while(|line| !line.trim().is_empty())
        .collect();
    let text = plain(&paragraph.join(" "));
    first_sentence(&text)
        .filter(|sentence| sentence.len() > 1)
        .ok_or_else(|| "section 1 has no opening sentence".to_owned())
}

/// The first sentence after the `**Intent, not built:**` label of `DESIGN.md` §1.
fn intent(design: &str) -> Result<String, String> {
    let body = section(design, 1)?;
    let start = body
        .find(INTENT_LABEL)
        .ok_or_else(|| format!("section 1 has no line starting `{INTENT_LABEL}`"))?;
    let rest = &body[start + INTENT_LABEL.len()..];
    let paragraph: Vec<&str> = rest
        .lines()
        .take_while(|line| !line.trim().is_empty())
        .collect();
    let text = plain(&paragraph.join(" "));
    first_sentence(&text).ok_or_else(|| "the intent line has no sentence".to_owned())
}

/// Splits one markdown table row into trimmed cells. A `|` inside a code span or escaped as `\|`
/// belongs to the cell.
fn cells(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    let inner = line.strip_prefix('|')?.strip_suffix('|')?;
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut in_code = false;
    let mut characters = inner.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' if characters.peek() == Some(&'|') => {
                current.push('|');
                characters.next();
            }
            '`' => {
                in_code = !in_code;
                current.push(character);
            }
            '|' if !in_code => cells.push(std::mem::take(&mut current).trim().to_owned()),
            _ => current.push(character),
        }
    }
    cells.push(current.trim().to_owned());
    Some(cells)
}

/// The rows of the first table in `body` whose header row starts with `header`, as cells. The
/// separator row is skipped; the table ends at the first line that is not a row.
fn table(body: &str, header: &[&str]) -> Result<Vec<Vec<String>>, String> {
    let mut lines = body.lines();
    let found = lines.by_ref().any(|line| {
        cells(line).is_some_and(|row| {
            row.len() >= header.len() && row.iter().zip(header).all(|(cell, name)| cell == name)
        })
    });
    if !found {
        return Err(format!("no table with the columns {}", header.join(", ")));
    }
    let separator = lines.next().and_then(cells);
    if !separator.is_some_and(|row| {
        row.iter()
            .all(|cell| cell.chars().all(|c| matches!(c, '-' | ':')))
    }) {
        return Err(format!(
            "the table {} has no separator row",
            header.join(", ")
        ));
    }
    let mut rows = Vec::new();
    for line in lines {
        let Some(row) = cells(line) else {
            break;
        };
        if row.len() < header.len() {
            return Err(format!("malformed table row `{}`", line.trim()));
        }
        rows.push(row);
    }
    Ok(rows)
}

/// The `DESIGN.md` §4 subsystem table.
fn subsystem_rows(design: &str) -> Result<Vec<SubsystemRow>, String> {
    let body = section(design, 4)?;
    let rows = table(body, &["Subsystem", "Status"])?;
    if rows.is_empty() {
        return Err("the subsystem table has no rows".to_owned());
    }
    rows.into_iter()
        .map(|row| {
            let status = SubsystemStatus::parse(&row[1])
                .ok_or_else(|| format!("subsystem `{}` has unknown status `{}`", row[0], row[1]))?;
            Ok(SubsystemRow {
                name: row[0].clone(),
                status,
            })
        })
        .collect()
}

/// Whether a plan ID is dated (`YYYY-MM-DD-slug`); numbered plans are closed history.
fn is_dated(id: &str) -> bool {
    let bytes = id.as_bytes();
    bytes.len() > 11
        && bytes[..10].iter().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                *byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
        && bytes[10] == b'-'
}

/// Cuts a plan title at the first ` (` or `:`, then at [`PLAN_SUMMARY_MAX_CHARS`] characters on a
/// word boundary.
#[must_use]
pub fn plan_summary(title: &str) -> String {
    let title = plain(title);
    let cut = [title.find(" ("), title.find(':')]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(title.len());
    let summary = title[..cut].trim();
    if summary.chars().count() <= PLAN_SUMMARY_MAX_CHARS {
        return summary.to_owned();
    }
    let mut result = String::new();
    for word in summary.split(' ') {
        let length =
            result.chars().count() + usize::from(!result.is_empty()) + word.chars().count();
        if length > PLAN_SUMMARY_MAX_CHARS {
            break;
        }
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(word);
    }
    if result.is_empty() {
        summary.chars().take(PLAN_SUMMARY_MAX_CHARS).collect()
    } else {
        result
    }
}

/// The dated `Active` and `Proposed` plans of the plan index, newest first by ID.
fn heading_plans(index: &str) -> Result<Vec<String>, String> {
    let rows = table(index, &["ID", "File", "Title", "Status"])?;
    let mut plans: Vec<(String, String)> = rows
        .into_iter()
        .filter(|row| is_dated(&row[0]) && matches!(row[3].as_str(), "Active" | "Proposed"))
        .map(|row| (row[0].clone(), plan_summary(&row[2])))
        .collect();
    plans.sort_by(|left, right| right.0.cmp(&left.0));
    Ok(plans
        .into_iter()
        .take(MAX_HEADING_PLANS)
        .map(|(_, summary)| summary)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_keep_pipes_in_code_spans_and_escapes() {
        assert_eq!(
            cells("| a | `b | c` | d \\| e |"),
            Some(vec!["a".into(), "`b | c`".into(), "d | e".into()])
        );
        assert_eq!(cells("not a row"), None);
    }

    #[test]
    fn first_sentence_ignores_inner_periods() {
        assert_eq!(
            first_sentence("Version 0.15 works. Then more."),
            Some("Version 0.15 works.".into())
        );
        assert_eq!(first_sentence("no end"), None);
    }

    #[test]
    fn dated_ids_are_recognised() {
        assert!(is_dated("2026-11-05-ci-showcase-video"));
        assert!(!is_dated("55"));
        assert!(!is_dated("2026-11-05"));
        assert!(!is_dated("2026_11_05-x"));
    }

    #[test]
    fn summaries_cut_at_parenthesis_colon_and_word_boundary() {
        assert_eq!(plan_summary("Short title (with detail)"), "Short title");
        assert_eq!(
            plan_summary("CI showcase video: a `tool`"),
            "CI showcase video"
        );
        let long = "word ".repeat(30);
        let summary = plan_summary(&long);
        assert!(summary.chars().count() <= PLAN_SUMMARY_MAX_CHARS);
        assert!(summary.ends_with("word"));
    }
}
