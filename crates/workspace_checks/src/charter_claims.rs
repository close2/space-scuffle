//! Checks C15 (test subjects), C16 (CI gate) and C20 (named paths) of the charter text itself.
//!
//! # Responsibility
//! These checks read `AGENTS.md` and the guides under `docs/guides/` that its rules link, and
//! compare what they say with the repository. C15 holds charter §4 to the table of test
//! subjects in `docs/guides/test-gates.md`: every named test file exists and holds tests, every
//! workspace crate has unit and integration tests, and the crates that promise property tests
//! have them. C16 holds §4.3 and that guide to CI and to the gate tool: each command of the
//! guide's **Full gate** list runs in `.github/workflows/ci.yml`, each `cargo gate <tier>` names
//! a tier `crates/gate` defines, and no other `cargo` command appears in §4 or in the guide
//! unchecked. C20 holds every repository path the charter or a guide names to existence.
//!
//! # How it fits
//! The rest of the checks judge code against rules; these three judge the charter's statements of
//! fact, so that a reader can trust that a file it names is there and a check it names runs.
//!
//! # Invariants
//! - Every check reads the documents' current text; nothing about the table, the commands or the
//!   paths is repeated here, except which crates promise property tests.
//! - Commands match modulo `--locked`, which pins the lock file and changes nothing else, and
//!   modulo what a CI step does with the output after the command (a pipe or a redirection).
//! - The tier names are read from the `Tier` enum's source as text, like every other source this
//!   crate reads, so `workspace_checks` keeps no dependency on the `gate` crate (charter §5).
//! - C20 resolves paths through Git's list of tracked files, so a file that exists only in one
//!   checkout cannot satisfy it.

use crate::markdown::{self, MarkdownLine};
use crate::source::walk_files;
use crate::{slash_path, CheckError, Finding};
use std::fs;
use std::path::{Path, PathBuf};

/// The charter file, relative to the repository root.
const CHARTER: &str = "AGENTS.md";

/// The guide holding the test-subject table (C15) and the full gate list (C16), relative to the
/// repository root.
pub const TEST_GATES_GUIDE: &str = "docs/guides/test-gates.md";

/// The directory of the charter's guides, whose named paths C20 checks, relative to the root.
pub const GUIDES_DIR: &str = "docs/guides/";

/// The CI workflow C16 reads, relative to the repository root.
pub const CI_WORKFLOW: &str = ".github/workflows/ci.yml";

/// The source of the gate tiers C16 reads, relative to the repository root.
pub const TIER_SOURCE: &str = "crates/gate/src/tier.rs";

/// The text that opens the list of commands CI must run, in [`TEST_GATES_GUIDE`] (C16).
const FULL_GATE_MARKER: &str = "- **Full gate**";

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
    document(root, CHARTER)
}

/// Reads the document `name` (relative to `root`).
fn document(root: &Path, name: &str) -> Result<(PathBuf, String), CheckError> {
    let path = root.join(name);
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

/// Check C15: the test-subject table of [`TEST_GATES_GUIDE`] (charter §4) matches the crates'
/// tests.
///
/// Each table row names a crate, a subject and its test files in code spans. Every named file
/// must exist as `crates/<crate>/tests/<file>` and contain `#[test]`; every directory under
/// `crates/` with a `Cargo.toml` must have a row, a `#[cfg(test)]` item in `src/` and a
/// `tests/*.rs` file; and each of [`PROPERTY_TESTED_CRATES`] must use `proptest!` in its
/// integration tests.
///
/// # Errors
/// [`CheckError::Io`] if the guide or a crate directory cannot be read.
pub fn test_subjects(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let (path, text) = document(root, TEST_GATES_GUIDE)?;
    let rows = subject_rows(&markdown::lines(&text));
    let mut findings = Vec::new();
    if rows.is_empty() {
        findings.push(Finding::new(
            root,
            &path,
            1,
            "no table of test subjects (| Crate | Subject | Test files |) (charter §4)",
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
                        "the table names `{file}` for `{krate}`, but crates/{krate}/tests/{file} \
                         does not exist or holds no #[test] (charter §4)"
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

/// `(line, crate, test files)` for each data row of the first table in `section` (a section or a
/// whole document) whose first header cell is `Crate`.
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
/// tests where promised. `table` is the document holding the test-subject table.
fn crate_test_findings(
    root: &Path,
    table: &Path,
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
            table,
            format!("the test-subject table has no row for crate `{krate}` (charter §4)"),
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

/// Check C16: the `cargo` commands of charter §4 and of [`TEST_GATES_GUIDE`] are each accounted
/// for.
///
/// - Every code span starting with `cargo ` in the list under the line that starts with
///   `- **Full gate**` is run, modulo `--locked`, by a step of [`CI_WORKFLOW`]; a CI step may
///   pipe or redirect the command's output. The guide must have that list.
/// - Every other span of the form `cargo gate <word> …` names a tier of the `Tier` enum in
///   [`TIER_SOURCE`]; a bare `cargo gate` names the tool and is accepted.
/// - Any other `cargo` span in charter §4 or in the guide is a finding, so no gate command appears
///   without a check.
///
/// # Errors
/// [`CheckError::Io`] if the charter, the guide, the workflow or the tier source cannot be read.
pub fn ci_runs_the_gate(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let workflow = read(&root.join(CI_WORKFLOW))?;
    let gate = GateSources {
        ci_commands: workflow
            .lines()
            .map(|line| {
                let line = line.trim_start().trim_start_matches("- ").trim_start();
                shell_command_words(line.strip_prefix("run:").unwrap_or(line))
            })
            .collect(),
        tiers: tier_names(&read(&root.join(TIER_SOURCE))?),
    };
    let (charter, charter_source) = charter_text(root)?;
    let all = markdown::lines(&charter_source);
    let mut findings = Vec::new();
    gate.check(root, &charter, &markdown::section(&all, "4"), &mut findings);
    let (guide, guide_text) = document(root, TEST_GATES_GUIDE)?;
    let full_commands = gate.check(root, &guide, &markdown::lines(&guide_text), &mut findings);
    if full_commands == 0 {
        findings.push(Finding::new(
            root,
            &guide,
            1,
            format!("no `{FULL_GATE_MARKER}` list of `cargo` commands (charter §4.3)"),
        ));
    }
    Ok(findings)
}

/// What C16 compares a document's `cargo` commands with.
struct GateSources<'a> {
    /// The words of each CI workflow line's command.
    ci_commands: Vec<Vec<&'a str>>,
    /// The tier names of the gate tool, lower case.
    tiers: Vec<String>,
}

impl GateSources<'_> {
    /// Adds a finding to `findings` for each unaccounted `cargo` span of `document`'s `lines`, and
    /// returns how many commands its **Full gate** list holds.
    fn check(
        &self,
        root: &Path,
        document: &Path,
        lines: &[MarkdownLine<'_>],
        findings: &mut Vec<Finding>,
    ) -> usize {
        let lines: Vec<&MarkdownLine<'_>> = lines.iter().filter(|line| !line.fenced).collect();
        let full_list = full_gate_lines(&lines);
        let mut report = |line: usize, message: String| {
            findings.push(Finding::new(root, document, line, message));
        };
        let mut full_commands = 0;
        for line in &lines {
            for span in markdown::code_spans(line.text) {
                if !span.starts_with("cargo ") {
                    continue;
                }
                let wanted = command_words(span);
                if full_list.contains(&line.number) {
                    full_commands += 1;
                    if !self.ci_commands.contains(&wanted) {
                        report(
                            line.number,
                            format!(
                                "full gate command `{span}` is not run by {CI_WORKFLOW} \
                                 (charter §4.3)"
                            ),
                        );
                    }
                } else if wanted == ["cargo", "gate"] {
                    // The tool itself, named in prose: no command to check.
                } else if let ["cargo", "gate", tier, ..] = wanted.as_slice() {
                    if !self.tiers.iter().any(|name| name == tier) {
                        report(
                            line.number,
                            format!(
                                "`{span}` names no tier of {TIER_SOURCE} (tiers: {}) (charter §4.3)",
                                self.tiers.join(", ")
                            ),
                        );
                    }
                } else {
                    report(
                        line.number,
                        format!(
                            "`{span}` is neither in the Full gate list nor a `cargo gate <tier>` \
                             (charter §4.3)"
                        ),
                    );
                }
            }
        }
        full_commands
    }
}

/// The line numbers of the items nested under the `- **Full gate**` line of `lines`: every
/// following non-blank line indented deeper than it, up to the first that is not.
fn full_gate_lines(lines: &[&MarkdownLine<'_>]) -> Vec<usize> {
    let indent = |text: &str| text.len() - text.trim_start().len();
    let Some(start) = lines
        .iter()
        .position(|line| line.text.trim_start().starts_with(FULL_GATE_MARKER))
    else {
        return Vec::new();
    };
    let depth = indent(lines[start].text);
    lines[start + 1..]
        .iter()
        .filter(|line| !line.text.trim().is_empty())
        .take_while(|line| indent(line.text) > depth)
        .map(|line| line.number)
        .collect()
}

/// The lower-case names of the variants of `pub enum Tier` in `source`: one identifier per
/// line between the enum's braces, doc comments skipped.
fn tier_names(source: &str) -> Vec<String> {
    source
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("pub enum Tier"))
        .skip(1)
        .take_while(|line| line.trim() != "}")
        .filter_map(|line| line.trim().strip_suffix(','))
        .filter(|name| !name.is_empty() && name.chars().all(char::is_alphanumeric))
        .map(str::to_lowercase)
        .collect()
}

/// The words of a CI step's command up to a pipe, a list operator or a redirection, without
/// `--locked`: `cargo test --locked 2>&1 | tee log` is `cargo test`.
fn shell_command_words(command: &str) -> Vec<&str> {
    command
        .split_whitespace()
        .take_while(|word| !matches!(*word, "|" | "||" | "&&" | ";") && !word.contains('>'))
        .filter(|word| *word != "--locked")
        .collect()
}

/// The whitespace-separated words of `command`, without `--locked`.
fn command_words(command: &str) -> Vec<&str> {
    command
        .split_whitespace()
        .filter(|word| *word != "--locked")
        .collect()
}

/// Check C20: every repository path named in an inline code span of `AGENTS.md`, or of a tracked
/// Markdown guide under [`GUIDES_DIR`], exists.
///
/// A span is a repository path if it has no whitespace, `=`, `<`, `>`, `*`, `{` or `$`, does not
/// start with `/` or `~`, and either contains `/` (resolved from the root; a trailing `/` names a
/// directory) or ends in a source or document extension (a bare file name, which some tracked
/// file must have). `tracked` is the repository's tracked files, relative to `root`.
/// [`UNTRACKED_PATHS`] are exempt.
///
/// # Errors
/// [`CheckError::Io`] if the charter or a guide cannot be read.
pub fn charter_paths_exist(root: &Path, tracked: &[PathBuf]) -> Result<Vec<Finding>, CheckError> {
    let tracked: Vec<String> = tracked.iter().map(|file| slash_path(file)).collect();
    let mut documents = vec![CHARTER.to_string()];
    documents.extend(
        tracked
            .iter()
            .filter(|file| {
                file.starts_with(GUIDES_DIR)
                    && Path::new(file).extension().is_some_and(|ext| ext == "md")
            })
            .cloned(),
    );
    let mut findings = Vec::new();
    for name in &documents {
        let (path, text) = document(root, name)?;
        for line in markdown::lines(&text).iter().filter(|line| !line.fenced) {
            for span in markdown::code_spans(line.text) {
                if !is_repository_path(span)
                    || UNTRACKED_PATHS.iter().any(|(name, _)| *name == span)
                {
                    continue;
                }
                if !names_tracked(span, &tracked) {
                    findings.push(Finding::new(
                        root,
                        &path,
                        line.number,
                        format!(
                            "`{span}` names no file or directory in the repository (charter §9)"
                        ),
                    ));
                }
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

    const CHARTER_4: &str = "# C\n\n## 4. Testing\n\n\
         3. Gates by `cargo gate`: `cargo gate package`, then `cargo gate merge <branch>`.\n\n\
         ## 5. Next\n";

    const GUIDE: &str = "# G\n\n| Crate | Subject | Test files |\n|---|---|---|\n\
         | `sim_types` | math | `a_tests.rs`, `gone.rs` |\n| `tool` | t | `t.rs` |\n\n\
         - **Full gate** (CI):\n\
         \x20 - `cargo test --workspace`\n\
         \x20 - `cargo fmt --all -- --check`\n\
         - Docs: `cargo gate package`.\n";

    const TIERS: &str = "//! t\npub enum Tier {\n    /// Cheap.\n    Quick,\n    Package,\n    Merge,\n    Full,\n}\n";

    #[test]
    fn c15_reports_missing_files_rows_unit_and_property_tests() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[
            (TEST_GATES_GUIDE, GUIDE),
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
            "docs/guides/test-gates.md:5: the table names `gone.rs` for `sim_types`",
            "docs/guides/test-gates.md:1: the test-subject table has no row for crate `other`",
            "crates/other/Cargo.toml:1: `other` has no tests/*.rs",
            "crates/sim_types/Cargo.toml:1: `sim_types` promises property tests",
            "crates/tool/Cargo.toml:1: `tool` has no #[cfg(test)]",
        ] {
            assert!(
                found.iter().any(|line| line.starts_with(expected)),
                "{expected} in {found:#?}"
            );
        }
        let repo = tree(&[(TEST_GATES_GUIDE, "# G\n\nNo table.\n")])?;
        fs::create_dir_all(repo.path().join("crates"))?;
        assert_eq!(
            messages(&test_subjects(repo.path())?),
            ["docs/guides/test-gates.md:1: no table of test subjects \
                 (| Crate | Subject | Test files |) (charter §4)"]
        );
        Ok(())
    }

    #[test]
    fn c16_matches_commands_modulo_locked_and_reports_a_missing_one() -> Result<(), Box<dyn Error>>
    {
        let workflow = "jobs:\n  a:\n    steps:\n      - run: cargo test --workspace --locked \
                        2>&1 | tee \"$RUNNER_TEMP/t.log\"\n\
                        \x20     - name: x\n        run: cargo fmt --all -- --check\n";
        let repo = tree(&[
            ("AGENTS.md", CHARTER_4),
            (TEST_GATES_GUIDE, GUIDE),
            (CI_WORKFLOW, workflow),
            (TIER_SOURCE, TIERS),
        ])?;
        assert_eq!(ci_runs_the_gate(repo.path())?, Vec::new());
        let without_fmt = "      - run: cargo test --workspace --locked\n";
        let repo = tree(&[
            ("AGENTS.md", CHARTER_4),
            (TEST_GATES_GUIDE, GUIDE),
            (CI_WORKFLOW, without_fmt),
            (TIER_SOURCE, TIERS),
        ])?;
        assert_eq!(
            messages(&ci_runs_the_gate(repo.path())?),
            [
                "docs/guides/test-gates.md:10: full gate command `cargo fmt --all -- --check` is \
                 not run by .github/workflows/ci.yml (charter §4.3)"
            ]
        );
        Ok(())
    }

    #[test]
    fn c16_reports_unknown_tiers_stray_commands_and_a_missing_full_list(
    ) -> Result<(), Box<dyn Error>> {
        let workflow =
            "      - run: cargo test --workspace\n      - run: cargo fmt --all -- --check\n";
        let charter = CHARTER_4.replace(
            "`cargo gate package`, then",
            "`cargo gate everything`, `cargo build`, then",
        );
        let guide = GUIDE.replace("`cargo gate package`.", "`cargo gate nightly`.");
        let repo = tree(&[
            ("AGENTS.md", charter.as_str()),
            (TEST_GATES_GUIDE, guide.as_str()),
            (CI_WORKFLOW, workflow),
            (TIER_SOURCE, TIERS),
        ])?;
        assert_eq!(
            messages(&ci_runs_the_gate(repo.path())?),
            [
                "AGENTS.md:5: `cargo build` is neither in the Full gate list nor a \
                 `cargo gate <tier>` (charter §4.3)",
                "AGENTS.md:5: `cargo gate everything` names no tier of crates/gate/src/tier.rs \
                 (tiers: quick, package, merge, full) (charter §4.3)",
                "docs/guides/test-gates.md:11: `cargo gate nightly` names no tier of \
                 crates/gate/src/tier.rs (tiers: quick, package, merge, full) (charter §4.3)",
            ]
        );
        let no_list = GUIDE.replace("- **Full gate** (CI):", "- CI:");
        let repo = tree(&[
            ("AGENTS.md", CHARTER_4),
            (TEST_GATES_GUIDE, no_list.as_str()),
            (CI_WORKFLOW, workflow),
            (TIER_SOURCE, TIERS),
        ])?;
        let found = messages(&ci_runs_the_gate(repo.path())?);
        assert!(
            found
                .iter()
                .any(|line| line.contains("test-gates.md:1: no `- **Full gate**` list")),
            "{found:#?}"
        );
        assert!(
            found
                .iter()
                .any(|line| line.contains("`cargo test --workspace` is neither")),
            "without the list its commands are stray: {found:#?}"
        );
        Ok(())
    }

    #[test]
    fn tier_names_are_the_enum_variants_in_lower_case() {
        assert_eq!(tier_names(TIERS), ["quick", "package", "merge", "full"]);
        assert_eq!(tier_names("pub struct Nothing;"), Vec::<String>::new());
        assert_eq!(
            shell_command_words("cargo test --locked 2>&1 | tee x"),
            ["cargo", "test"]
        );
    }

    #[test]
    fn c20_resolves_paths_directories_and_bare_names_through_git() -> Result<(), Box<dyn Error>> {
        let charter =
            "# C\nSee `docs/a.md`, `docs/`, `crates/x`, `b.rs`, `target/`, `gone/c.md`,\n\
                       `missing.rs`, `/run/lock/gpu.lock`, `~/scratch`, `X=a/b`, `crates/<c>/t`,\n\
                       `sim_core::world`, `radeon_icd.json`.\n```\n`in/fence.md`\n```\n";
        let guide = "# G\nSee `docs/a.md` and `crates/x/gone.rs`.\n";
        let repo = tree(&[
            ("AGENTS.md", charter),
            ("docs/guides/g.md", guide),
            ("docs/guides/notes.txt", "`not/checked.rs`\n"),
        ])?;
        let tracked: Vec<PathBuf> = [
            "docs/a.md",
            "crates/x/src/b.rs",
            "AGENTS.md",
            "docs/guides/g.md",
            "docs/guides/notes.txt",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(
            messages(&charter_paths_exist(repo.path(), &tracked)?),
            [
                "AGENTS.md:2: `gone/c.md` names no file or directory in the repository (charter §9)",
                "AGENTS.md:3: `missing.rs` names no file or directory in the repository (charter §9)",
                "docs/guides/g.md:2: `crates/x/gone.rs` names no file or directory in the \
                 repository (charter §9)",
            ]
        );
        Ok(())
    }
}
