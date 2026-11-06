//! Check C17: `DESIGN.md` links resolve, its status table cites real evidence, and the tick
//! rate it states is the simulation's.
//!
//! # Responsibility
//! `DESIGN.md` is the document a newcomer reads first. Before Phase 9 it linked into one
//! developer's home directory and stated a tick rate the code never had (finding Q-9). This
//! check keeps the rewritten document honest on the points a machine can judge: no `file://`
//! links, every relative link resolves, every row of the subsystem status table has a known
//! status and names evidence that exists, and every tick rate it states equals the constant the
//! caller passes in (`sim_types::TICKS_PER_SECOND`).
//!
//! # How it fits
//! `tests/charter.rs` passes the real constant, so this library stays independent of the
//! simulation crates; only the test target depends on `sim_types`, which charter §5 draws as a
//! dev edge. Whether the prose is accurate otherwise is a reviewer's judgement.
//!
//! # Invariants
//! - A tick rate is a number followed by `Hz` or `ticks/sec`; throughput figures are written
//!   `ticks/s` and are not tick rates. At least one tick rate must be stated.
//! - Evidence must exist for `Working` and `Partial` rows; `Not built` and `Removed` rows may cite
//!   none, but what they cite must exist.
//! - This file must pass C19, so the URL scheme it rejects is assembled with `concat!`.

use crate::markdown::{self, MarkdownLine};
use crate::{slash_path, CheckError, Finding};
use std::path::{Path, PathBuf};

/// The design document, relative to the repository root.
pub const DESIGN_DOC: &str = "DESIGN.md";

/// The statuses a subsystem may have in the status table.
pub const SUBSYSTEM_STATUSES: [&str; 4] = ["Working", "Partial", "Not built", "Removed"];

/// The heading text, compared in lower case, that marks the status table's section.
const STATUS_HEADING: &str = "subsystem status";

/// The URL scheme C17 rejects, assembled so this file does not contain it.
const FILE_SCHEME: &str = concat!("file", "://");

/// Units that make a preceding number a tick rate.
const RATE_UNITS: [&str; 2] = ["Hz", "ticks/sec"];

/// Check C17 on `DESIGN.md` under `root`. `tracked` lists the repository's tracked files
/// (relative to `root`), used to resolve bare evidence file names; `ticks_per_second` is the
/// simulation's tick rate.
///
/// # Errors
/// [`CheckError::Io`] if `DESIGN.md` cannot be read.
pub fn design_doc(
    root: &Path,
    tracked: &[PathBuf],
    ticks_per_second: u32,
) -> Result<Vec<Finding>, CheckError> {
    let path = root.join(DESIGN_DOC);
    let text = std::fs::read_to_string(&path).map_err(|source| CheckError::Io {
        path: path.clone(),
        source,
    })?;
    let tracked: Vec<String> = tracked.iter().map(|file| slash_path(file)).collect();
    let all = markdown::lines(&text);
    let mut problems = Vec::new();
    link_problems(root, &all, &mut problems);
    status_table_problems(root, &all, &tracked, &mut problems);
    tick_rate_problems(&all, ticks_per_second, &mut problems);
    Ok(problems
        .into_iter()
        .map(|(line, message)| Finding::new(root, &path, line, message))
        .collect())
}

/// `file://` links anywhere, and relative link targets outside code that do not exist.
///
/// Shared with check C24, which judges `README.md` by the same rule.
pub(crate) fn link_problems(
    root: &Path,
    all: &[MarkdownLine<'_>],
    problems: &mut Vec<(usize, String)>,
) {
    for line in all {
        if line.text.contains(FILE_SCHEME) {
            problems.push((
                line.number,
                "absolute file link; link relative to the repository (charter §9, C17)".into(),
            ));
        }
        if line.fenced {
            continue;
        }
        for target in markdown::link_targets(line.text) {
            if is_external(target) {
                continue;
            }
            let file = target.split('#').next().unwrap_or(target);
            if !file.is_empty() && !root.join(file).exists() {
                problems.push((
                    line.number,
                    format!("link target `{target}` does not exist"),
                ));
            }
        }
    }
}

/// Whether a link target leaves the repository (a URL, a mail address or an in-page anchor).
pub(crate) fn is_external(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with('#')
        || target.starts_with(FILE_SCHEME)
}

/// Rows of the status table: a known status, and evidence that exists.
fn status_table_problems(
    root: &Path,
    all: &[MarkdownLine<'_>],
    tracked: &[String],
    problems: &mut Vec<(usize, String)>,
) {
    let Some(start) = all.iter().position(|line| {
        line.text.starts_with('#') && line.text.to_lowercase().contains(STATUS_HEADING)
    }) else {
        problems.push((
            1,
            format!("no `{STATUS_HEADING}` section with a status table"),
        ));
        return;
    };
    let mut header: Option<Vec<String>> = None;
    let mut rows = 0;
    for line in all[start + 1..].iter().filter(|line| !line.fenced) {
        if line.text.starts_with('#') {
            break;
        }
        let Some(cells) = markdown::table_cells(line.text) else {
            continue;
        };
        let Some(columns) = &header else {
            header = Some(cells.iter().map(ToString::to_string).collect());
            continue;
        };
        if markdown::is_separator_row(&cells) {
            continue;
        }
        rows += 1;
        let cell = |name: &str| {
            columns
                .iter()
                .position(|column| column == name)
                .and_then(|index| cells.get(index).copied())
                .unwrap_or("")
        };
        row_problems(
            root,
            line.number,
            cell("Status"),
            cell("Evidence"),
            tracked,
            problems,
        );
    }
    if rows == 0 {
        problems.push((
            all[start].number,
            "the status section has no table rows".into(),
        ));
    }
}

/// Problems with one status row.
fn row_problems(
    root: &Path,
    line: usize,
    status: &str,
    evidence: &str,
    tracked: &[String],
    problems: &mut Vec<(usize, String)>,
) {
    if !SUBSYSTEM_STATUSES.contains(&status) {
        problems.push((
            line,
            format!(
                "status `{status}` is not one of {}",
                SUBSYSTEM_STATUSES.join(", ")
            ),
        ));
    }
    let cited: Vec<&str> = markdown::code_spans(evidence)
        .into_iter()
        .chain(markdown::link_targets(evidence))
        .filter(|item| item.contains('/') || item.contains('.'))
        .collect();
    if cited.is_empty() && matches!(status, "Working" | "Partial") {
        problems.push((
            line,
            format!("a `{status}` subsystem must cite evidence that exists"),
        ));
    }
    for item in cited {
        let exists = if item.contains('/') {
            root.join(item.trim_end_matches('/')).exists()
        } else {
            let suffix = format!("/{item}");
            tracked
                .iter()
                .any(|file| file == item || file.ends_with(&suffix))
        };
        if !exists {
            problems.push((line, format!("evidence `{item}` does not exist")));
        }
    }
}

/// Every stated tick rate equals `ticks_per_second`, and at least one is stated.
fn tick_rate_problems(
    all: &[MarkdownLine<'_>],
    ticks_per_second: u32,
    problems: &mut Vec<(usize, String)>,
) {
    let mut stated = 0;
    for line in all {
        for (rate, unit) in tick_rates(line.text) {
            stated += 1;
            if rate != ticks_per_second.to_string() {
                problems.push((
                    line.number,
                    format!(
                        "states a tick rate of {rate} {unit}; the simulation runs at \
                         {ticks_per_second} (sim_types::TICKS_PER_SECOND)"
                    ),
                ));
            }
        }
    }
    if stated == 0 {
        problems.push((
            1,
            format!(
                "states no tick rate; say `{ticks_per_second} Hz` (sim_types::TICKS_PER_SECOND)"
            ),
        ));
    }
}

/// `(number, unit)` for each number on `line` followed, after optional spaces, by a rate unit.
fn tick_rates(line: &str) -> Vec<(String, &'static str)> {
    let chars: Vec<char> = line.chars().collect();
    let mut rates = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let starts_number = chars[index].is_ascii_digit()
            && (index == 0 || !(chars[index - 1].is_alphanumeric() || chars[index - 1] == '.'));
        if !starts_number {
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len() && (chars[index].is_ascii_digit() || chars[index] == '.') {
            index += 1;
        }
        let number: String = chars[start..index].iter().collect();
        let rest: String = chars[index..].iter().collect();
        let rest = rest.trim_start();
        for unit in RATE_UNITS {
            let follows = rest
                .strip_prefix(unit)
                .is_some_and(|after| !after.starts_with(|c: char| c.is_alphanumeric() || c == '/'));
            if follows {
                rates.push((number.trim_end_matches('.').to_string(), unit));
            }
        }
    }
    rates
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;

    fn repo(design: &str) -> Result<tempfile::TempDir, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        fs::create_dir_all(dir.path().join("docs"))?;
        fs::write(dir.path().join("docs/a.md"), "a")?;
        fs::create_dir_all(dir.path().join("crates/x/tests"))?;
        fs::write(dir.path().join("crates/x/tests/t_tests.rs"), "t")?;
        fs::write(dir.path().join(DESIGN_DOC), design)?;
        Ok(dir)
    }

    fn tracked() -> Vec<PathBuf> {
        vec![
            PathBuf::from("crates/x/tests/t_tests.rs"),
            PathBuf::from("docs/a.md"),
        ]
    }

    fn messages(findings: &[Finding]) -> Vec<String> {
        findings.iter().map(ToString::to_string).collect()
    }

    const GOOD: &str = "# D\n\nThe tick rate is 25 Hz; throughput 639 ticks/s.\n[a](docs/a.md#x) \
                        [web](https://e.org)\n\n## 4. Subsystem status\n\n\
                        | Subsystem | Status | What exists | Evidence | Known gaps |\n|---|---|---|---|---|\n\
                        | Math | Working | all | `t_tests.rs` | none |\n\
                        | Bots | Not built | nothing | none | all |\n\n## 5. Next\n";

    #[test]
    fn c17_accepts_a_document_that_matches_the_repository() -> Result<(), Box<dyn Error>> {
        let dir = repo(GOOD)?;
        let found = messages(&design_doc(dir.path(), &tracked(), 25)?);
        assert!(found.is_empty(), "{found:#?}");
        Ok(())
    }

    #[test]
    fn c17_reports_links_rates_statuses_and_evidence() -> Result<(), Box<dyn Error>> {
        let file_link = format!("[old]({FILE_SCHEME}/home/cl/x.md)");
        let design = format!(
            "# D\n\nRuns at 22.4 ticks/sec or 30 Hz, or 25Hz.\n{file_link} [gone](docs/b.md)\n\n\
             ## Subsystem status\n\n| Subsystem | Status | Evidence |\n|---|---|---|\n\
             | A | Working | none |\n| B | Done | `crates/x/tests/nope.rs` |\n"
        );
        let dir = repo(&design)?;
        let found = messages(&design_doc(dir.path(), &tracked(), 25)?);
        let expected = [
            "DESIGN.md:4: absolute file link",
            "DESIGN.md:4: link target `docs/b.md` does not exist",
            "DESIGN.md:10: a `Working` subsystem must cite evidence",
            "DESIGN.md:11: status `Done` is not one of",
            "DESIGN.md:11: evidence `crates/x/tests/nope.rs` does not exist",
            "DESIGN.md:3: states a tick rate of 22.4 ticks/sec",
            "DESIGN.md:3: states a tick rate of 30 Hz",
        ];
        assert_eq!(found.len(), expected.len(), "{found:#?}");
        for prefix in expected {
            assert!(
                found.iter().any(|line| line.starts_with(prefix)),
                "{prefix} in {found:#?}"
            );
        }
        Ok(())
    }

    #[test]
    fn c17_requires_a_stated_rate_and_a_status_table() -> Result<(), Box<dyn Error>> {
        let dir = repo("# D\n\nNo numbers here.\n")?;
        let found = messages(&design_doc(dir.path(), &tracked(), 25)?);
        assert_eq!(found.len(), 2, "{found:#?}");
        Ok(())
    }

    #[test]
    fn tick_rates_need_a_rate_unit_after_a_whole_number() {
        assert_eq!(
            tick_rates("25 Hz, 22.4 ticks/sec, 639 ticks/s, x86 Hz, 3 Hzz, 60Hz."),
            [
                ("25".to_string(), "Hz"),
                ("22.4".to_string(), "ticks/sec"),
                ("60".to_string(), "Hz")
            ]
        );
    }
}
