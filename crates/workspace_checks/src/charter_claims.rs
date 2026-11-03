//! Checks C15 (test subjects), C16 (CI gate) and C20 (named paths) of the charter text itself.
//!
//! # Responsibility
//! These checks read `AGENTS.md` and compare what it says with the repository. C15 holds charter
//! §4 to its table of test subjects: every named test file exists and holds tests, every
//! workspace crate has unit and integration tests, and the crates that promise property tests
//! have them. C16 holds §4.3 to CI: each gate command it lists runs in
//! `.github/workflows/ci.yml`. C20 holds every repository path the charter names to existence.
//!
//! # How it fits
//! The rest of the checks judge code against rules; these three judge the charter's statements of
//! fact, so that a reader can trust that a file it names is there and a check it names runs.
//!
//! # Invariants
//! - Every check reads the charter's current text; nothing about the table, the commands or the
//!   paths is repeated here, except which crates promise property tests.
//! - Commands match modulo `--locked`, which pins the lock file and changes nothing else.
//! - C20 resolves paths through Git's list of tracked files, so a file that exists only in one
//!   checkout cannot satisfy it.

use crate::markdown::{self, MarkdownLine};
use crate::source::walk_files;
use crate::{slash_path, CheckError, Finding};
use std::fs;
use std::path::{Path, PathBuf};

/// The charter file, relative to the repository root.
const CHARTER: &str = "AGENTS.md";

/// The CI workflow C16 reads, relative to the repository root.
pub const CI_WORKFLOW: &str = ".github/workflows/ci.yml";

/// Crates whose integration tests charter §4 promises include property tests (C15).
pub const PROPERTY_TESTED_CRATES: [&str; 2] = ["sim_types", "sim_map"];

/// Paths the charter names that are build output, not repository content (C20), with why.
pub const UNTRACKED_PATHS: [(&str, &str); 1] = [(
    "target/",
    "Cargo's build directory, created by the first build and ignored by Git",
)];

/// File extensions that make a bare backticked name a repository file name (C20).
const FILE_EXTENSIONS: [&str; 7] = [".rs", ".ron", ".md", ".toml", ".yml", ".wgsl", ".lock"];

/// Reads `AGENTS.md` under `root`.
fn charter_text(root: &Path) -> Result<(PathBuf, String), CheckError> {
    let path = root.join(CHARTER);
    let text = read(&path)?;
    Ok((path, text))
}

/// Reads a file as UTF-8 text, mapping the error to [`CheckError::Io`].
fn read(path: &Path) -> Result<String, CheckError> {
    fs::read_to_string(path).map_err(|source| CheckError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Check C15: the test-subject table of charter §4 matches the crates' tests.
///
/// Each table row names a crate, a subject and its test files in code spans. Every named file
/// must exist as `crates/<crate>/tests/<file>` and contain `#[test]`; every directory under
/// `crates/` with a `Cargo.toml` must have a row, a `#[cfg(test)]` item in `src/` and a
/// `tests/*.rs` file; and each of [`PROPERTY_TESTED_CRATES`] must use `proptest!` in its
/// integration tests.
///
/// # Errors
/// [`CheckError::Io`] if the charter or a crate directory cannot be read.
pub fn test_subjects(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let (path, text) = charter_text(root)?;
    let all = markdown::lines(&text);
    let rows = subject_rows(&markdown::section(&all, "4"));
    let mut findings = Vec::new();
    if rows.is_empty() {
        findings.push(Finding::new(
            root,
            &path,
            1,
            "§4 has no table of test subjects (| Crate | Subject | Test files |)",
        ));
    }
    for (line, krate, files) in &rows {
        for file in files {
            let test = root.join("crates").join(krate).join("tests").join(file);
            let holds_tests = fs::read_to_string(&test).is_ok_and(|code| code.contains("#[test]"));
            if !holds_tests {
                findings.push(Finding::new(
                    root,
                    &path,
                    *line,
                    format!(
                        "§4 names `{file}` for `{krate}`, but crates/{krate}/tests/{file} does \
                         not exist or holds no #[test] (charter §4)"
                    ),
                ));
            }
        }
    }
    for krate in workspace_crate_dirs(root)? {
        findings.extend(crate_test_findings(root, &path, &krate, &rows)?);
    }
    Ok(findings)
}

/// `(line, crate, test files)` for each data row of the first table in `section` whose first
/// header cell is `Crate`.
fn subject_rows(section: &[MarkdownLine<'_>]) -> Vec<(usize, String, Vec<String>)> {
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in section.iter().filter(|line| !line.fenced) {
        let Some(cells) = markdown::table_cells(line.text) else {
            if in_table {
                break;
            }
            continue;
        };
        if cells.first() == Some(&"Crate") {
            in_table = true;
            continue;
        }
        if !in_table || markdown::is_separator_row(&cells) || cells.len() < 3 {
            continue;
        }
        let krate = cells[0].trim_matches('`').to_string();
        let files = markdown::code_spans(cells[cells.len() - 1])
            .into_iter()
            .map(str::to_string)
            .collect();
        rows.push((line.number, krate, files));
    }
    rows
}

/// Names of the directories under `crates/` that hold a `Cargo.toml`, sorted.
fn workspace_crate_dirs(root: &Path) -> Result<Vec<String>, CheckError> {
    let crates = root.join("crates");
    let io = |source| CheckError::Io {
        path: crates.clone(),
        source,
    };
    let mut names = Vec::new();
    for entry in fs::read_dir(&crates).map_err(io)? {
        let entry = entry.map_err(io)?;
        if entry.path().join("Cargo.toml").is_file() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// C15 findings for one crate: no table row, no unit tests, no integration tests, no property
/// tests where promised.
fn crate_test_findings(
    root: &Path,
    charter: &Path,
    krate: &str,
    rows: &[(usize, String, Vec<String>)],
) -> Result<Vec<Finding>, CheckError> {
    let dir = root.join("crates").join(krate);
    let mut sources = Vec::new();
    walk_files(&dir.join("src"), &mut sources)?;
    let mut tests = Vec::new();
    walk_files(&dir.join("tests"), &mut tests)?;
    tests.retain(|path| path.extension().is_some_and(|ext| ext == "rs"));
    let mut findings = Vec::new();
    let manifest = dir.join("Cargo.toml");
    let mut report = |path: &Path, message: String| {
        findings.push(Finding::new(root, path, 1, message));
    };
    if !rows.iter().any(|(_, name, _)| name == krate) {
        report(
            charter,
            format!("§4's test table has no row for crate `{krate}` (charter §4)"),
        );
    }
    if !any_contains(&sources, "#[cfg(test)]") {
        report(
            &manifest,
            format!("`{krate}` has no #[cfg(test)] unit tests in src/ (charter §4)"),
        );
    }
    if tests.is_empty() {
        report(
            &manifest,
            format!("`{krate}` has no tests/*.rs integration tests (charter §4)"),
        );
    }
    if PROPERTY_TESTED_CRATES.contains(&krate) && !any_contains(&tests, "proptest!") {
        report(
            &manifest,
            format!(
                "`{krate}` promises property tests but no tests/*.rs uses proptest! (charter §4)"
            ),
        );
    }
    Ok(findings)
}

/// Whether any of `files` is UTF-8 text containing `needle`.
fn any_contains(files: &[PathBuf], needle: &str) -> bool {
    files
        .iter()
        .any(|path| fs::read_to_string(path).is_ok_and(|text| text.contains(needle)))
}

/// Check C16: every gate command charter §4 lists (an inline code span starting with `cargo `)
/// is run verbatim, modulo `--locked`, by a step of [`CI_WORKFLOW`].
///
/// # Errors
/// [`CheckError::Io`] if the charter or the workflow cannot be read.
pub fn ci_runs_the_gate(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let (path, text) = charter_text(root)?;
    let workflow = read(&root.join(CI_WORKFLOW))?;
    let ci_commands: Vec<Vec<&str>> = workflow
        .lines()
        .map(|line| {
            let line = line.trim_start().trim_start_matches("- ").trim_start();
            command_words(line.strip_prefix("run:").unwrap_or(line))
        })
        .collect();
    let all = markdown::lines(&text);
    let mut findings = Vec::new();
    let mut gate = 0;
    for line in markdown::section(&all, "4")
        .iter()
        .filter(|line| !line.fenced)
    {
        for span in markdown::code_spans(line.text) {
            if !span.starts_with("cargo ") {
                continue;
            }
            gate += 1;
            let wanted = command_words(span);
            if !ci_commands.contains(&wanted) {
                findings.push(Finding::new(
                    root,
                    &path,
                    line.number,
                    format!("gate command `{span}` is not run by {CI_WORKFLOW} (charter §4.3)"),
                ));
            }
        }
    }
    if gate == 0 {
        findings.push(Finding::new(
            root,
            &path,
            1,
            "§4 lists no gate command (`cargo ...`)",
        ));
    }
    Ok(findings)
}

/// The whitespace-separated words of `command`, without `--locked`.
fn command_words(command: &str) -> Vec<&str> {
    command
        .split_whitespace()
        .filter(|word| *word != "--locked")
        .collect()
}

/// Check C20: every repository path named in an inline code span of `AGENTS.md` exists.
///
/// A span is a repository path if it has no whitespace, `=`, `<`, `>`, `*`, `{` or `$`, does not
/// start with `/` or `~`, and either contains `/` (resolved from the root; a trailing `/` names a
/// directory) or ends in a source or document extension (a bare file name, which some tracked
/// file must have). `tracked` is the repository's tracked files, relative to `root`.
/// [`UNTRACKED_PATHS`] are exempt.
///
/// # Errors
/// [`CheckError::Io`] if the charter cannot be read.
pub fn charter_paths_exist(root: &Path, tracked: &[PathBuf]) -> Result<Vec<Finding>, CheckError> {
    let (path, text) = charter_text(root)?;
    let tracked: Vec<String> = tracked.iter().map(|file| slash_path(file)).collect();
    let mut findings = Vec::new();
    for line in markdown::lines(&text).iter().filter(|line| !line.fenced) {
        for span in markdown::code_spans(line.text) {
            if !is_repository_path(span) || UNTRACKED_PATHS.iter().any(|(name, _)| *name == span) {
                continue;
            }
            if !names_tracked(span, &tracked) {
                findings.push(Finding::new(
                    root,
                    &path,
                    line.number,
                    format!("`{span}` names no file or directory in the repository (charter §9)"),
                ));
            }
        }
    }
    Ok(findings)
}

/// Whether a code span reads as a repository path (see [`charter_paths_exist`]).
fn is_repository_path(span: &str) -> bool {
    let plain = !span.is_empty()
        && !span.chars().any(|c| {
            c.is_whitespace() || matches!(c, '=' | '<' | '>' | '*' | '{' | '$' | '(' | ':')
        })
        && !span.starts_with('/')
        && !span.starts_with('~');
    plain && (span.contains('/') || FILE_EXTENSIONS.iter().any(|ext| span.ends_with(ext)))
}

/// Whether `span` names a tracked file, a directory holding one, or (bare) a tracked file name.
fn names_tracked(span: &str, tracked: &[String]) -> bool {
    if let Some(directory) = span.strip_suffix('/') {
        let prefix = format!("{directory}/");
        return tracked.iter().any(|file| file.starts_with(&prefix));
    }
    if span.contains('/') {
        let prefix = format!("{span}/");
        return tracked
            .iter()
            .any(|file| file == span || file.starts_with(&prefix));
    }
    let suffix = format!("/{span}");
    tracked
        .iter()
        .any(|file| file == span || file.ends_with(&suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    fn tree(files: &[(&str, &str)]) -> Result<tempfile::TempDir, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        for (path, text) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().ok_or("file has a parent")?)?;
            fs::write(full, text)?;
        }
        Ok(dir)
    }

    fn messages(findings: &[Finding]) -> Vec<String> {
        let mut out: Vec<String> = findings.iter().map(ToString::to_string).collect();
        out.sort();
        out
    }

    const CHARTER_4: &str =
        "# C\n\n## 4. Testing\n\n| Crate | Subject | Test files |\n|---|---|---|\n\
         | `sim_types` | math | `a_tests.rs`, `gone.rs` |\n| `tool` | t | `t.rs` |\n\n\
         3. Gate: `cargo test --workspace` and `cargo fmt --all -- --check`\n\n## 5. Next\n";

    #[test]
    fn c15_reports_missing_files_rows_unit_and_property_tests() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[
            ("AGENTS.md", CHARTER_4),
            ("crates/sim_types/Cargo.toml", ""),
            (
                "crates/sim_types/src/lib.rs",
                "//! x\n#[cfg(test)]\nmod tests {}\n",
            ),
            ("crates/sim_types/tests/a_tests.rs", "#[test]\nfn a() {}\n"),
            ("crates/tool/Cargo.toml", ""),
            ("crates/tool/src/lib.rs", "//! no unit tests\n"),
            ("crates/tool/tests/t.rs", "#[test]\nfn t() {}\n"),
            ("crates/other/Cargo.toml", ""),
            (
                "crates/other/src/lib.rs",
                "//! o\n#[cfg(test)]\nmod tests {}\n",
            ),
        ])?;
        let found = messages(&test_subjects(repo.path())?);
        assert_eq!(found.len(), 5, "{found:#?}");
        for expected in [
            "AGENTS.md:7: §4 names `gone.rs` for `sim_types`",
            "AGENTS.md:1: §4's test table has no row for crate `other`",
            "crates/other/Cargo.toml:1: `other` has no tests/*.rs",
            "crates/sim_types/Cargo.toml:1: `sim_types` promises property tests",
            "crates/tool/Cargo.toml:1: `tool` has no #[cfg(test)]",
        ] {
            assert!(
                found.iter().any(|line| line.starts_with(expected)),
                "{expected} in {found:#?}"
            );
        }
        Ok(())
    }

    #[test]
    fn c16_matches_commands_modulo_locked_and_reports_a_missing_one() -> Result<(), Box<dyn Error>>
    {
        let workflow = "jobs:\n  a:\n    steps:\n      - run: cargo test --workspace --locked\n\
                        \x20     - name: x\n        run: cargo fmt --all -- --check\n";
        let repo = tree(&[("AGENTS.md", CHARTER_4), (CI_WORKFLOW, workflow)])?;
        assert_eq!(ci_runs_the_gate(repo.path())?, Vec::new());
        let without_fmt = "      - run: cargo test --workspace --locked\n";
        let repo = tree(&[("AGENTS.md", CHARTER_4), (CI_WORKFLOW, without_fmt)])?;
        assert_eq!(
            messages(&ci_runs_the_gate(repo.path())?),
            [
                "AGENTS.md:10: gate command `cargo fmt --all -- --check` is not run by \
              .github/workflows/ci.yml (charter §4.3)"
            ]
        );
        Ok(())
    }

    #[test]
    fn c20_resolves_paths_directories_and_bare_names_through_git() -> Result<(), Box<dyn Error>> {
        let charter =
            "# C\nSee `docs/a.md`, `docs/`, `crates/x`, `b.rs`, `target/`, `gone/c.md`,\n\
                       `missing.rs`, `/run/lock/gpu.lock`, `~/scratch`, `X=a/b`, `crates/<c>/t`,\n\
                       `sim_core::world`, `radeon_icd.json`.\n```\n`in/fence.md`\n```\n";
        let repo = tree(&[("AGENTS.md", charter)])?;
        let tracked: Vec<PathBuf> = ["docs/a.md", "crates/x/src/b.rs", "AGENTS.md"]
            .iter()
            .map(PathBuf::from)
            .collect();
        assert_eq!(
            messages(&charter_paths_exist(repo.path(), &tracked)?),
            [
                "AGENTS.md:2: `gone/c.md` names no file or directory in the repository (charter §9)",
                "AGENTS.md:3: `missing.rs` names no file or directory in the repository (charter §9)",
            ]
        );
        Ok(())
    }
}
