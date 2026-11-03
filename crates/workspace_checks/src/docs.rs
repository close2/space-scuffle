//! The documentation checks C18 (plan index) and C19 (notebook paths) of the charter.
//!
//! # Responsibility
//! C18 keeps `docs/plans/` navigable: every plan has exactly one row in the status index
//! `docs/plans/README.md`, with a status from a fixed set and a unique ID, and plans added since
//! the numbered series closed are named `YYYY-MM-DD-slug.md`. C19 keeps tracked text free of
//! paths that only resolved on the development notebook: no `file:///` links anywhere, and every
//! Markdown document that names a `/tmp/` path says that this evidence was not retained.
//!
//! # How it fits
//! Both come from Phase 9 step 9.6 (finding Q-10, decision D-10 of
//! `docs/plans/2026-11-03-phase-9-structure-and-docs.md`). `tests/charter.rs` runs them on the
//! repository; the unit tests below run them on temporary trees with planted violations. C19
//! judges only files Git tracks ([`tracked_files`]), so untracked local folders such as `tmp/`
//! are never read.
//!
//! # Invariants
//! - Both checks report every violation, never only the first.
//! - The numbered plan IDs are frozen in [`LEGACY_PLAN_IDS`]; the list may only shrink when a
//!   plan file is deleted, so a new numbered plan cannot appear without a reviewed edit here.
//! - This file must pass C19 itself, so the URL scheme it looks for is assembled with `concat!`
//!   and written out whole only inside code spans of its documentation.

use crate::{CheckError, Finding};
use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The statuses a plan may have in the index (C18).
pub const PLAN_STATUSES: [&str; 6] = [
    "Done",
    "Partly done",
    "Superseded",
    "Rejected",
    "Active",
    "Proposed",
];

/// IDs of the numbered plans, the naming scheme used until 2026-10-28 (C18).
///
/// A plan whose file name starts with one of these followed by `-` is a legacy plan with that ID.
/// Every other plan must be named `YYYY-MM-DD-slug.md`, and its ID is its file stem.
pub const LEGACY_PLAN_IDS: [&str; 56] = [
    "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "13", "14", "15", "16",
    "17", "18", "19", "20", "21a", "21b", "22", "23", "24", "25", "26", "27", "28", "29", "30",
    "31", "32", "33", "34", "35", "36", "37", "38", "39", "40", "41", "42", "43", "44", "45", "46",
    "47", "48", "49", "50", "51", "52", "53", "54", "55",
];

/// The line every Markdown document naming a `/tmp/` path carries under its title (C19).
pub const EVIDENCE_RETENTION_LINE: &str = "> Evidence under /tmp in this document was on the \
     development notebook and was not retained (Q-10, 2026-11-03).";

/// The URL scheme C19 rejects, assembled so this file does not contain it.
const FILE_SCHEME: &str = concat!("file", ":///");

/// The plan directory, relative to the repository root.
const PLANS_DIR: &str = "docs/plans";

/// The file name of the plan status index inside [`PLANS_DIR`].
const INDEX_FILE: &str = "README.md";

/// Header cells of the index table, in order.
const INDEX_COLUMNS: [&str; 5] = ["ID", "File", "Title", "Status", "Evidence"];

/// Files Git tracks under `root`, sorted, as paths relative to `root`.
///
/// Runs `git -C <root> ls-files -z`, so the list matches what a clone contains and never
/// includes ignored or untracked local files.
///
/// # Errors
/// [`CheckError::GitSpawn`] if Git cannot be started, [`CheckError::GitFailed`] if it exits
/// unsuccessfully (for example outside a repository).
pub fn tracked_files(root: &Path) -> Result<Vec<PathBuf>, CheckError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output()
        .map_err(CheckError::GitSpawn)?;
    if !output.status.success() {
        return Err(CheckError::GitFailed {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let mut files: Vec<PathBuf> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| PathBuf::from(String::from_utf8_lossy(name).into_owned()))
        .collect();
    files.sort();
    Ok(files)
}

/// One data row of the plan index.
struct IndexRow {
    /// One-based line of the row in the index.
    line: usize,
    /// The ID cell.
    id: String,
    /// The plan file the File cell names (a link target or bare name).
    file: String,
    /// The Status cell.
    status: String,
    /// The Evidence cell.
    evidence: String,
}

/// Check C18: `docs/plans/README.md` indexes every plan in `docs/plans/` exactly once.
///
/// Every `*.md` file in `docs/plans/` other than the index is a plan. Each must be named either
/// `<legacy ID>-slug.md` with an ID from [`LEGACY_PLAN_IDS`] or `YYYY-MM-DD-slug.md`, no two may
/// share an ID, and each must have exactly one index row. Each row must have the five columns of
/// the table, name an existing plan, give that plan's ID (the legacy number, or the file stem for
/// a dated plan), a status from [`PLAN_STATUSES`] and non-empty evidence.
///
/// # Errors
/// [`CheckError::Io`] if the plan directory or the index cannot be read.
pub fn plan_index(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let dir = root.join(PLANS_DIR);
    let index_path = dir.join(INDEX_FILE);
    let plans = plan_files(&dir)?;
    let mut findings = plan_name_findings(root, &dir, &plans);
    let rows = if index_path.is_file() {
        parse_index(root, &index_path, &read_text(&index_path)?, &mut findings)
    } else {
        findings.push(Finding::new(
            root,
            &index_path,
            1,
            "the plan status index is missing (Q-10)",
        ));
        Vec::new()
    };
    let mut ids: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows {
        if let Some(first) = ids.insert(row.id.as_str(), row.line) {
            findings.push(Finding::new(
                root,
                &index_path,
                row.line,
                format!("ID `{}` is already used on line {first}", row.id),
            ));
        }
        findings.extend(row_findings(root, &index_path, row, &plans));
    }
    for plan in &plans {
        let count = rows.iter().filter(|row| &row.file == plan).count();
        if count == 0 {
            findings.push(Finding::new(
                root,
                &dir.join(plan),
                1,
                format!("plan is not in the index `{PLANS_DIR}/{INDEX_FILE}` (Q-10)"),
            ));
        } else if count > 1 {
            findings.push(Finding::new(
                root,
                &index_path,
                1,
                format!("`{plan}` has {count} index rows; exactly one is allowed"),
            ));
        }
    }
    Ok(findings)
}

/// Names of the plan files in `dir` (every `*.md` but the index), sorted.
fn plan_files(dir: &Path) -> Result<Vec<String>, CheckError> {
    let mut files = Vec::new();
    crate::source::walk_files(dir, &mut files)?;
    let mut names: Vec<String> = files
        .iter()
        .filter(|path| path.parent() == Some(dir) && is_markdown(path))
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| name != INDEX_FILE)
        .collect();
    names.sort();
    Ok(names)
}

/// Whether `path` has the Markdown extension `.md`, in any letter case.
fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// Findings about plan file names: an unknown naming scheme, or two files with one ID.
fn plan_name_findings(root: &Path, dir: &Path, plans: &[String]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut owners: BTreeMap<String, &str> = BTreeMap::new();
    for plan in plans {
        let Some(id) = plan_id(plan) else {
            findings.push(Finding::new(
                root,
                &dir.join(plan),
                1,
                "new plans are named `YYYY-MM-DD-slug.md`; numbered names are closed (Q-10)",
            ));
            continue;
        };
        if let Some(other) = owners.insert(id.clone(), plan) {
            findings.push(Finding::new(
                root,
                &dir.join(plan),
                1,
                format!("shares the plan ID `{id}` with `{other}`; plan IDs are unique (Q-10)"),
            ));
        }
    }
    findings
}

/// The ID of the plan file `name`, or `None` if it follows neither naming scheme.
#[must_use]
pub fn plan_id(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".md")?;
    if is_dated_stem(stem) {
        return Some(stem.to_string());
    }
    let (prefix, slug) = stem.split_once('-')?;
    (LEGACY_PLAN_IDS.contains(&prefix) && is_slug(slug)).then(|| prefix.to_string())
}

/// Whether `stem` is `YYYY-MM-DD-slug` with a lowercase, hyphen-separated slug.
fn is_dated_stem(stem: &str) -> bool {
    let bytes = stem.as_bytes();
    let digit = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
    let shape = (0..4).all(digit)
        && bytes.get(4) == Some(&b'-')
        && (5..7).all(digit)
        && bytes.get(7) == Some(&b'-')
        && (8..10).all(digit)
        && bytes.get(10) == Some(&b'-');
    shape && is_slug(&stem[11..])
}

/// Whether `slug` is non-empty lowercase ASCII words joined by single hyphens.
fn is_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// Parses the index table, reporting malformed rows. Header and separator rows are skipped.
fn parse_index(root: &Path, path: &Path, text: &str, findings: &mut Vec<Finding>) -> Vec<IndexRow> {
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells == INDEX_COLUMNS || cells.iter().all(|cell| is_separator_cell(cell)) {
            continue;
        }
        if cells.len() != INDEX_COLUMNS.len() {
            findings.push(Finding::new(
                root,
                path,
                index + 1,
                format!(
                    "row has {} cells; the index columns are {}",
                    cells.len(),
                    INDEX_COLUMNS.join(" | ")
                ),
            ));
            continue;
        }
        rows.push(IndexRow {
            line: index + 1,
            id: cells[0].trim_matches('`').to_string(),
            file: link_target(cells[1]),
            status: cells[3].to_string(),
            evidence: cells[4].to_string(),
        });
    }
    rows
}

/// Whether `cell` is a table separator cell such as `---` or `:--:`.
fn is_separator_cell(cell: &str) -> bool {
    !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':')
}

/// The target of a Markdown link in `cell`, or the cell itself without backticks.
fn link_target(cell: &str) -> String {
    match cell.split_once("](") {
        Some((_, rest)) => rest.split(')').next().unwrap_or(rest).to_string(),
        None => cell.trim_matches('`').to_string(),
    }
}

/// Findings about one index row: unknown file, wrong ID, unknown status, empty evidence.
fn row_findings(root: &Path, index: &Path, row: &IndexRow, plans: &[String]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut report = |message: String| {
        findings.push(Finding::new(root, index, row.line, message));
    };
    if plans.contains(&row.file) {
        if let Some(expected) = plan_id(&row.file) {
            if row.id != expected {
                report(format!(
                    "ID `{}` does not match `{}`, whose ID is `{expected}`",
                    row.id, row.file
                ));
            }
        }
    } else {
        report(format!(
            "`{}` is not a plan file in `{PLANS_DIR}/`; link the file by its name",
            row.file
        ));
    }
    if !PLAN_STATUSES.contains(&row.status.as_str()) {
        report(format!(
            "status `{}` is not one of: {}",
            row.status,
            PLAN_STATUSES.join(", ")
        ));
    }
    if row.evidence.is_empty() {
        report(String::from(
            "evidence is empty; name a commit, the superseding plan or a remediation decision",
        ));
    }
    findings
}

/// Check C19: no `file:///` link in any of `files`, and every Markdown file among them that
/// names a `/tmp/` path carries [`EVIDENCE_RETENTION_LINE`].
///
/// `files` are paths relative to `root`, normally [`tracked_files`]. Symbolic links, files that
/// no longer exist in the working tree and files that are not UTF-8 are skipped: none can hold a
/// link a reader would follow. A `file:///` inside an inline code span (between backticks on
/// one line) names the scheme and is not a link, so it is allowed. A `/tmp/` path counts only
/// when it is absolute (not the tail of a longer path such as `crates/x/tmp/`) and outside a
/// fenced code block, where it is a command's scratch directory rather than evidence. A
/// Markdown file whose every such line links into a `docs/evidence/` file needs no retention
/// line.
///
/// # Errors
/// [`CheckError::Io`] if a file exists but cannot be read.
pub fn notebook_references(root: &Path, files: &[PathBuf]) -> Result<Vec<Finding>, CheckError> {
    let mut findings = Vec::new();
    for relative in files {
        let path = root.join(relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == ErrorKind::NotFound => continue,
            Err(source) => return Err(CheckError::Io { path, source }),
        };
        if !metadata.is_file() {
            continue;
        }
        let bytes = fs::read(&path).map_err(|source| CheckError::Io {
            path: path.clone(),
            source,
        })?;
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        findings.extend(file_link_findings(root, &path, &text));
        if is_markdown(relative) {
            findings.extend(tmp_evidence_finding(root, &path, &text));
        }
    }
    Ok(findings)
}

/// Every `file:///` outside an inline code span in `text`.
fn file_link_findings(root: &Path, path: &Path, text: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for (at, _) in line.match_indices(FILE_SCHEME) {
            let ticks = line[..at].matches('`').count();
            if ticks % 2 == 0 {
                findings.push(Finding::new(
                    root,
                    path,
                    index + 1,
                    "absolute file URL; use a relative link in Markdown or a backticked \
                     repository path in Rust docs (Q-10)",
                ));
            }
        }
    }
    findings
}

/// A finding at the first unmarked `/tmp/` line of a Markdown `text` that lacks the retention
/// line, or `None`.
fn tmp_evidence_finding(root: &Path, path: &Path, text: &str) -> Option<Finding> {
    if text
        .lines()
        .any(|line| line.trim() == EVIDENCE_RETENTION_LINE)
    {
        return None;
    }
    let mut fenced = false;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if !fenced && names_absolute_tmp(line) && !links_into_evidence(line) {
            return Some(Finding::new(
                root,
                path,
                index + 1,
                format!(
                    "names a /tmp/ path, which did not survive the notebook; add under the \
                     title: {EVIDENCE_RETENTION_LINE}"
                ),
            ));
        }
    }
    None
}

/// Whether `line` contains `/tmp/` not preceded by a path character.
fn names_absolute_tmp(line: &str) -> bool {
    line.match_indices("/tmp/").any(|(at, _)| {
        line[..at].chars().next_back().is_none_or(|before| {
            !(before.is_ascii_alphanumeric() || matches!(before, '_' | '.' | '-' | '~'))
        })
    })
}

/// Whether `line` holds a Markdown link whose target lies under an `evidence/` directory.
fn links_into_evidence(line: &str) -> bool {
    line.split("](").skip(1).any(|rest| {
        rest.split(')')
            .next()
            .is_some_and(|target| target.contains("evidence/"))
    })
}

/// Reads a file as UTF-8 text, mapping the error to [`CheckError::Io`].
fn read_text(path: &Path) -> Result<String, CheckError> {
    fs::read_to_string(path).map_err(|source| CheckError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slash_path;
    use std::error::Error;

    /// A temporary repository with the given files.
    fn tree(files: &[(&str, &str)]) -> Result<tempfile::TempDir, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        for (path, text) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().ok_or("file has a parent")?)?;
            fs::write(full, text)?;
        }
        Ok(dir)
    }

    /// `path:line` of every finding, sorted.
    fn lines(findings: &[Finding]) -> Vec<String> {
        let mut out: Vec<String> = findings
            .iter()
            .map(|f| format!("{}:{}", slash_path(&f.path), f.line))
            .collect();
        out.sort();
        out
    }

    const HEADER: &str = "| ID | File | Title | Status | Evidence |\n|---|---|---|---|---|\n";

    #[test]
    fn plan_ids_follow_the_two_naming_schemes() {
        assert_eq!(plan_id("05-plan-x.md").as_deref(), Some("05"));
        assert_eq!(plan_id("21b-plan-map-editor.md").as_deref(), Some("21b"));
        assert_eq!(
            plan_id("2026-11-03-phase-9.md").as_deref(),
            Some("2026-11-03-phase-9")
        );
        assert_eq!(plan_id("56-plan-new.md"), None, "numbered names are closed");
        assert_eq!(plan_id("21c-plan-new.md"), None);
        assert_eq!(plan_id("2026-11-3-x.md"), None);
        assert_eq!(plan_id("2026-11-03-Upper.md"), None);
        assert_eq!(plan_id("2026-11-03-.md"), None);
        assert_eq!(plan_id("05-plan-x.txt"), None);
    }

    #[test]
    fn c18_passes_a_complete_index() -> Result<(), Box<dyn Error>> {
        let index = format!(
            "# Plans\n\n{HEADER}| 05 | [05-a.md](05-a.md) | A | Done | `abc1234` |\n\
             | 2026-11-03-b | [2026-11-03-b.md](2026-11-03-b.md) | B | Active | remediation 9.6 |\n"
        );
        let repo = tree(&[
            ("docs/plans/README.md", &index),
            ("docs/plans/05-a.md", "# A\n"),
            ("docs/plans/2026-11-03-b.md", "# B\n"),
        ])?;
        assert_eq!(plan_index(repo.path())?, Vec::new());
        Ok(())
    }

    #[test]
    fn c18_reports_every_index_and_naming_fault() -> Result<(), Box<dyn Error>> {
        let index = format!(
            "{HEADER}| 05 | [05-a.md](05-a.md) | A | Done | x |\n\
             | 05 | [05-a.md](05-a.md) | A again | Done | x |\n\
             | 06 | [06-b.md](06-b.md) | B | Finished | x |\n\
             | 07 | [07-gone.md](07-gone.md) | Gone | Done | x |\n\
             | 2026-11-03-c | [2026-11-03-c.md](2026-11-03-c.md) | C | Proposed |  |\n\
             | 08 | too | few |\n"
        );
        let repo = tree(&[
            ("docs/plans/README.md", &index),
            ("docs/plans/05-a.md", "# A\n"),
            ("docs/plans/06-b.md", "# B\n"),
            ("docs/plans/2026-11-03-c.md", "# C\n"),
            ("docs/plans/09-unindexed.md", "# D\n"),
            ("docs/plans/09-twin.md", "# E\n"),
            ("docs/plans/60-new-numbered.md", "# F\n"),
        ])?;
        let findings = plan_index(repo.path())?;
        assert_eq!(
            lines(&findings),
            vec![
                "docs/plans/09-twin.md:1",         // not indexed
                "docs/plans/09-unindexed.md:1",    // shares ID 09 with 09-twin
                "docs/plans/09-unindexed.md:1",    // not indexed
                "docs/plans/60-new-numbered.md:1", // numbered names are closed
                "docs/plans/60-new-numbered.md:1", // not indexed
                "docs/plans/README.md:1",          // 05-a.md has two rows
                "docs/plans/README.md:4",          // duplicate ID 05
                "docs/plans/README.md:5",          // unknown status
                "docs/plans/README.md:6",          // names a missing file
                "docs/plans/README.md:7",          // empty evidence
                "docs/plans/README.md:8",          // wrong cell count
            ]
        );
        Ok(())
    }

    #[test]
    fn c18_reports_a_missing_index_and_every_unindexed_plan() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[("docs/plans/01-a.md", "# A\n")])?;
        assert_eq!(
            lines(&plan_index(repo.path())?),
            vec!["docs/plans/01-a.md:1", "docs/plans/README.md:1"]
        );
        Ok(())
    }

    #[test]
    fn c18_reports_an_id_that_does_not_match_its_file() -> Result<(), Box<dyn Error>> {
        let index =
            format!("{HEADER}| 2026-11-03 | [2026-11-03-c.md](2026-11-03-c.md) | C | Done | x |\n");
        let repo = tree(&[
            ("docs/plans/README.md", &index),
            ("docs/plans/2026-11-03-c.md", "# C\n"),
        ])?;
        let findings = plan_index(repo.path())?;
        assert_eq!(lines(&findings), vec!["docs/plans/README.md:3"]);
        assert!(findings[0].message.contains("`2026-11-03-c`"));
        Ok(())
    }

    #[test]
    fn c19_reports_file_links_outside_code_spans() -> Result<(), Box<dyn Error>> {
        let link = format!("[a]({FILE_SCHEME}home/x/a.md)");
        let markdown = format!("# T\n{link}\nThe `{FILE_SCHEME}` scheme is fine in code.\n");
        let rust = format!("//! {link}\nconst U: &str = \"{FILE_SCHEME}x\";\n");
        let repo = tree(&[("docs/a.md", &markdown), ("crates/a/src/lib.rs", &rust)])?;
        let files = [
            PathBuf::from("docs/a.md"),
            PathBuf::from("crates/a/src/lib.rs"),
        ];
        assert_eq!(
            lines(&notebook_references(repo.path(), &files)?),
            vec![
                "crates/a/src/lib.rs:1",
                "crates/a/src/lib.rs:2",
                "docs/a.md:2"
            ]
        );
        Ok(())
    }

    #[test]
    fn c19_requires_the_retention_line_for_tmp_evidence() -> Result<(), Box<dyn Error>> {
        let marked = format!("# M\n\n{EVIDENCE_RETENTION_LINE}\n\nSee /tmp/run01/a.png.\n");
        let files_and_text = [
            (
                "docs/unmarked.md",
                "# U\n\nfirst line\nCaptured at `/tmp/run01/a.png`.\n",
            ),
            ("docs/marked.md", marked.as_str()),
            (
                "docs/relative.md",
                "# R\n\nThe folder crates/asset_baker/tmp/x is local.\n",
            ),
            ("docs/fenced.md", "# F\n\n```sh\nprobe /tmp/out\n```\n"),
            (
                "docs/kept.md",
                "# K\n\nCopied /tmp/run01/a.png to [a](../docs/evidence/a.png).\n",
            ),
            ("docs/notes.txt", "Captured at /tmp/run01/a.png.\n"),
        ];
        let repo = tree(&files_and_text)?;
        let files: Vec<PathBuf> = files_and_text
            .iter()
            .map(|(path, _)| PathBuf::from(path))
            .chain([PathBuf::from("docs/deleted.md")])
            .collect();
        assert_eq!(
            lines(&notebook_references(repo.path(), &files)?),
            vec!["docs/unmarked.md:4"]
        );
        Ok(())
    }

    #[test]
    fn absolute_tmp_paths_are_told_from_relative_ones() {
        assert!(names_absolute_tmp("/tmp/a"));
        assert!(names_absolute_tmp("see `/tmp/a`"));
        assert!(names_absolute_tmp("(/tmp/a)"));
        assert!(!names_absolute_tmp("crates/x/tmp/a"));
        assert!(!names_absolute_tmp("~/tmp/a and .tmp/ and a-tmp/"));
        assert!(!names_absolute_tmp("/tmp without a slash"));
    }

    #[test]
    fn tracked_files_lists_this_repository() -> Result<(), Box<dyn Error>> {
        let files = tracked_files(&crate::repo_root())?;
        assert!(files.contains(&PathBuf::from("AGENTS.md")));
        assert!(files.contains(&PathBuf::from("crates/workspace_checks/src/lib.rs")));
        assert!(files.windows(2).all(|pair| pair[0] < pair[1]));
        Ok(())
    }
}
