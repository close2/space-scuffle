//! Checks C8 (function length) and C11 (performance rationale format) of the charter.
//!
//! # Responsibility
//! C8 keeps the 100-line limit on production functions real: no production code suppresses
//! `clippy::too_many_lines`, the workspace enables the lint, and every `clippy.toml` the workspace
//! uses states the same threshold. C11 keeps every performance rationale block in the
//! structured form charter §2 prescribes, with `Why:`, `What:` and `Trade-off:` lines.
//!
//! # How it fits
//! Clippy itself measures the functions; C8 only makes sure nothing switches that measurement off
//! or loosens it in one crate. Whether a performance divergence is justified is a reviewer's
//! judgement; C11 checks only that the justification is written down in full.
//!
//! # Invariants
//! - C8 reads production code through [`code_text`], so the lint's name in a comment or string is
//!   not a suppression; test code is exempt (Phase 9 decision D-1).
//! - Clippy reads only the nearest `clippy.toml`, so C8 requires the root file and checks every
//!   `crates/*/clippy.toml`.
//! - This file must pass C11 itself, so the block marker is assembled with `concat!`.

use crate::source::{walk_files, word_tokens};
use crate::{code_text, rust_files, CheckError, Finding, Scope};
use std::fs;
use std::path::{Path, PathBuf};

/// The lint C8 protects.
const LENGTH_LINT: &str = "too_many_lines";

/// The `clippy.toml` setting and value every configuration must state (C8).
pub const LENGTH_THRESHOLD_SETTING: &str = "too-many-lines-threshold = 100";

/// The marker that opens a performance rationale block, assembled so this file has none.
pub const RATIONALE_MARKER: &str = concat!("PERFORMANCE", " RATIONALE");

/// The labels a rationale block must give, each at the start of one of its comment lines (C11).
pub const RATIONALE_LABELS: [&str; 3] = ["Why:", "What:", "Trade-off:"];

/// How many comment lines after the marker may hold the labels (C11).
pub const RATIONALE_WINDOW: usize = 8;

/// Directories, relative to the root, whose text files C11 reads.
const RATIONALE_SCOPES: [&str; 2] = ["crates", "assets/shaders"];

/// Check C8: no production code names `clippy::too_many_lines`, the root `Cargo.toml` sets the
/// lint to `warn` or `deny`, and the root and every crate `clippy.toml` state
/// [`LENGTH_THRESHOLD_SETTING`].
///
/// # Errors
/// [`CheckError::Io`] if a source, manifest or configuration file cannot be read.
pub fn function_length_limit(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let mut findings = Vec::new();
    for path in rust_files(root, Scope::Production)? {
        let chars: Vec<char> = code_text(&path, Scope::Production)?.chars().collect();
        let mut line = 1;
        let mut counted = 0;
        for (start, token) in word_tokens(&chars) {
            line += chars[counted..start].iter().filter(|&&c| c == '\n').count();
            counted = start;
            if token == LENGTH_LINT {
                findings.push(Finding::new(
                    root,
                    &path,
                    line,
                    "production code suppresses `clippy::too_many_lines`; split the function \
                     instead (charter §2.4)",
                ));
            }
        }
    }
    let manifest = root.join("Cargo.toml");
    let enabled = read(&manifest)?.lines().any(|line| {
        let line: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        line == "too_many_lines=\"warn\"" || line == "too_many_lines=\"deny\""
    });
    if !enabled {
        findings.push(Finding::new(
            root,
            &manifest,
            1,
            "`[workspace.lints.clippy]` does not set `too_many_lines` to warn or deny (charter §2.4)",
        ));
    }
    for config in clippy_configs(root)? {
        if !config.is_file() {
            findings.push(Finding::new(
                root,
                &config,
                1,
                format!("missing; it must state `{LENGTH_THRESHOLD_SETTING}` (charter §2.4)"),
            ));
        } else if !read(&config)?
            .lines()
            .any(|line| line.trim() == LENGTH_THRESHOLD_SETTING)
        {
            findings.push(Finding::new(
                root,
                &config,
                1,
                format!(
                    "does not state `{LENGTH_THRESHOLD_SETTING}`; every clippy.toml must use \
                     the same threshold (charter §2.4)"
                ),
            ));
        }
    }
    Ok(findings)
}

/// The root `clippy.toml` (required) and every `crates/*/clippy.toml` that exists.
fn clippy_configs(root: &Path) -> Result<Vec<PathBuf>, CheckError> {
    let mut configs = vec![root.join("clippy.toml")];
    let crates = root.join("crates");
    if crates.is_dir() {
        let io = |source| CheckError::Io {
            path: crates.clone(),
            source,
        };
        let mut entries: Vec<PathBuf> = fs::read_dir(&crates)
            .map_err(io)?
            .map(|entry| entry.map(|entry| entry.path().join("clippy.toml")))
            .collect::<Result<_, _>>()
            .map_err(io)?;
        entries.sort();
        configs.extend(entries.into_iter().filter(|path| path.is_file()));
    }
    Ok(configs)
}

/// Check C11: every line naming [`RATIONALE_MARKER`] in a text file under `crates/` or
/// `assets/shaders/` is a `//` comment holding only the marker and a colon, and the next
/// [`RATIONALE_WINDOW`] lines of the same comment start with each of [`RATIONALE_LABELS`].
///
/// # Errors
/// [`CheckError::Io`] if a directory cannot be read.
pub fn performance_rationale_format(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let mut files = Vec::new();
    for scope in RATIONALE_SCOPES {
        walk_files(&root.join(scope), &mut files)?;
    }
    let mut findings = Vec::new();
    for path in files {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            if line.contains(RATIONALE_MARKER) {
                if let Some(message) = rationale_problem(&lines, index) {
                    findings.push(Finding::new(root, &path, index + 1, message));
                }
            }
        }
    }
    Ok(findings)
}

/// What is wrong with the rationale block whose marker is on `lines[index]`, if anything.
fn rationale_problem(lines: &[&str], index: usize) -> Option<String> {
    let header = format!("{RATIONALE_MARKER}:");
    if comment_text(lines[index]) != Some(header.as_str()) {
        return Some(format!(
            "a rationale block opens with a `// {header}` line of its own (charter §2.3)"
        ));
    }
    let body: Vec<&str> = lines[index + 1..]
        .iter()
        .take(RATIONALE_WINDOW)
        .map_while(|line| comment_text(line))
        .collect();
    let missing: Vec<&str> = RATIONALE_LABELS
        .into_iter()
        .filter(|label| !body.iter().any(|text| text.starts_with(label)))
        .collect();
    (!missing.is_empty()).then(|| {
        format!(
            "{RATIONALE_MARKER} block lacks {} within {RATIONALE_WINDOW} comment lines \
             (charter §2.3)",
            missing.join(", ")
        )
    })
}

/// The text of a `//` comment line (any of `//`, `///`, `//!`), trimmed, or `None` for code.
fn comment_text(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("//")?;
    let rest = rest
        .strip_prefix('/')
        .or_else(|| rest.strip_prefix('!'))
        .unwrap_or(rest);
    Some(rest.trim())
}

/// Reads a file as UTF-8 text, mapping the error to [`CheckError::Io`].
fn read(path: &Path) -> Result<String, CheckError> {
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

    fn tree(files: &[(&str, &str)]) -> Result<tempfile::TempDir, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        for (path, text) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().ok_or("file has a parent")?)?;
            fs::write(full, text)?;
        }
        Ok(dir)
    }

    fn locations(findings: &[Finding]) -> Vec<String> {
        let mut out: Vec<String> = findings
            .iter()
            .map(|f| format!("{}:{}", slash_path(&f.path), f.line))
            .collect();
        out.sort();
        out
    }

    const MANIFEST: &str = "[workspace.lints.clippy]\ntoo_many_lines = \"warn\"\n";
    const CONFIG: &str = "# limit\ntoo-many-lines-threshold = 100\n";

    #[test]
    fn c08_reports_production_suppressions_but_not_tests_or_prose() -> Result<(), Box<dyn Error>> {
        let attribute = format!("#[expect(clippy::{LENGTH_LINT}, reason = \"long\")]");
        let repo = tree(&[
            ("Cargo.toml", MANIFEST),
            ("clippy.toml", CONFIG),
            (
                "crates/a/src/lib.rs",
                &format!("//! a\n// {LENGTH_LINT} in prose\n{attribute}\nfn f() {{}}\n"),
            ),
            (
                "crates/a/tests/t.rs",
                &format!("//! t\n{attribute}\nfn g() {{}}\n"),
            ),
            (
                "crates/a/src/m.rs",
                &format!(
                    "//! m\n#[cfg(test)]\nmod tests {{\n    {attribute}\n    fn h() {{}}\n}}\n"
                ),
            ),
        ])?;
        assert_eq!(
            locations(&function_length_limit(repo.path())?),
            ["crates/a/src/lib.rs:3"]
        );
        Ok(())
    }

    #[test]
    fn c08_requires_the_lint_and_one_threshold_everywhere() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[
            (
                "Cargo.toml",
                "[workspace.lints.clippy]\ntoo_many_lines = \"allow\"\n",
            ),
            ("crates/a/clippy.toml", "too-many-lines-threshold = 60\n"),
            ("crates/b/clippy.toml", CONFIG),
            ("crates/b/src/lib.rs", "//! b\n"),
        ])?;
        assert_eq!(
            locations(&function_length_limit(repo.path())?),
            ["Cargo.toml:1", "clippy.toml:1", "crates/a/clippy.toml:1"]
        );
        Ok(())
    }

    #[test]
    fn c11_accepts_a_full_block_and_reports_partial_or_inline_ones() -> Result<(), Box<dyn Error>> {
        let full = format!(
            "//! f\n// {RATIONALE_MARKER}:\n// Why: slow.\n// What: a table.\n// detail\n\
             // Trade-off: memory.\nfn f() {{}}\n"
        );
        let partial = format!(
            "//! p\n    // {RATIONALE_MARKER}:\n    // Why: slow.\n    // What: a table.\n\
             fn p() {{}}\n// Trade-off: too late, code came first.\n"
        );
        let inline = format!(
            "//! i\n// {RATIONALE_MARKER} & MORE:\n// Why: a\n// What: b\n// Trade-off: c\n"
        );
        let shader = format!("// {RATIONALE_MARKER}:\n// Why: a\n");
        let repo = tree(&[
            ("crates/a/src/full.rs", &full),
            ("crates/a/src/partial.rs", &partial),
            ("crates/a/src/inline.rs", &inline),
            ("assets/shaders/s.wgsl", &shader),
        ])?;
        let findings = performance_rationale_format(repo.path())?;
        assert_eq!(
            locations(&findings),
            [
                "assets/shaders/s.wgsl:1",
                "crates/a/src/inline.rs:2",
                "crates/a/src/partial.rs:2"
            ]
        );
        assert!(findings
            .iter()
            .any(|f| f.message.contains("lacks Trade-off:")));
        Ok(())
    }
}
