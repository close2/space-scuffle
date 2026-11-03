//! The source checks C6, C10 and C12 of the charter, which judge macro arguments and bindings.
//!
//! # Responsibility
//! - **C6:** production code embeds no shipped data: no `include_str!` or `include_bytes!`
//!   whose argument names a path under `assets/` (charter §3.3: data changes need no rebuild).
//! - **C10:** every `.expect("...")` message in production code starts with `invariant:`
//!   (charter §1.1: `expect` only for a stated invariant).
//! - **C12:** production code of the simulation crates never discards a value with `let _ =`
//!   (charter §1.1: no silent failures; a discarded `Result` is one).
//!
//! # How it fits
//! Like `checks.rs`, each check takes the repository root and returns every [`Finding`]. C6 and
//! C10 must read the *contents* of string literals, which [`code_text`] blanks. They therefore
//! locate the macro or call in the blanked production text (so comments, strings and
//! `#[cfg(test)]` items cannot match) and read the literal from the original source at the same
//! character positions, which blanking preserves.
//!
//! # Invariants
//! - Blanked and original text have the same characters at every index outside comments and
//!   literals, so a position found in one addresses the same token in the other.
//! - An `expect` whose argument is not a string literal cannot be judged and is reported.

use crate::source::{line_of, word_tokens};
use crate::{code_text, rust_files, CheckError, Finding, Scope, SIMULATION_CRATES};
use std::fs;
use std::path::{Path, PathBuf};

/// Macros that compile a file's contents into the binary (C6).
const EMBEDDING_MACROS: [&str; 2] = ["include_str", "include_bytes"];

/// The path fragment that marks shipped, run-time-loaded data (C6).
const SHIPPED_DATA_DIR: &str = "assets/";

/// The prefix every production `expect` message must start with (C10).
pub const INVARIANT_PREFIX: &str = "invariant:";

/// One production Rust file: its original characters and its blanked production text.
struct ProductionSource {
    path: PathBuf,
    original: Vec<char>,
    code: Vec<char>,
}

/// Every production Rust file under `root/crates`, optionally limited to the simulation crates.
fn production_sources(
    root: &Path,
    simulation_only: bool,
) -> Result<Vec<ProductionSource>, CheckError> {
    let crates_dir = root.join("crates");
    let mut sources = Vec::new();
    for path in rust_files(root, Scope::Production)? {
        if simulation_only
            && !SIMULATION_CRATES
                .iter()
                .any(|krate| path.starts_with(crates_dir.join(krate)))
        {
            continue;
        }
        let original = fs::read_to_string(&path)
            .map_err(|source| CheckError::Io {
                path: path.clone(),
                source,
            })?
            .chars()
            .collect();
        let code = code_text(&path, Scope::Production)?.chars().collect();
        sources.push(ProductionSource {
            path,
            original,
            code,
        });
    }
    Ok(sources)
}

/// Index of the first non-whitespace character at or after `from`, if any.
fn skip_whitespace(chars: &[char], from: usize) -> Option<usize> {
    (from..chars.len()).find(|&i| !chars[i].is_whitespace())
}

/// Index just past the `)` that closes the `(` at `open`, or the end of the text.
fn closing_paren(code: &[char], open: usize) -> usize {
    let mut depth = 0usize;
    for (i, &c) in code.iter().enumerate().skip(open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    code.len()
}

/// Check C6: no `include_str!`/`include_bytes!` in production code names a path containing
/// `assets/`.
///
/// Production code loads shipped data from disk at run time, so editing a catalog or map never
/// needs a rebuild. Test code and the dev-only `sim_fixtures` crate may embed the files.
///
/// # Errors
/// [`CheckError::Io`] if a directory or file cannot be read.
pub fn no_embedded_assets(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let mut findings = Vec::new();
    for source in production_sources(root, false)? {
        for (start, token) in word_tokens(&source.code) {
            if !EMBEDDING_MACROS.contains(&token.as_str()) {
                continue;
            }
            let Some(bang) = skip_whitespace(&source.code, start + token.len())
                .filter(|&i| source.code[i] == '!')
            else {
                continue;
            };
            let Some(open) =
                skip_whitespace(&source.code, bang + 1).filter(|&i| source.code[i] == '(')
            else {
                continue;
            };
            let close = closing_paren(&source.code, open);
            let argument: String = source.original[open..close].iter().collect();
            if argument.contains(SHIPPED_DATA_DIR) {
                findings.push(Finding::new(
                    root,
                    &source.path,
                    line_of(&source.code, start),
                    format!(
                        "`{token}!` embeds shipped data {argument}; production code must load \
                         it at run time so data changes need no rebuild (charter §3.3)"
                    ),
                ));
            }
        }
    }
    Ok(findings)
}

/// Check C10: every `.expect(...)` in production code has a string-literal message starting with
/// `invariant:`.
///
/// `expect` is permitted only where an invariant guarantees success, and the message must state
/// that invariant (charter §1.1). Attributes such as `#[expect(...)]` are not calls and are
/// ignored.
///
/// # Errors
/// [`CheckError::Io`] if a directory or file cannot be read.
pub fn expect_messages_state_invariants(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let mut findings = Vec::new();
    for source in production_sources(root, false)? {
        for (start, token) in word_tokens(&source.code) {
            if token != "expect" || !is_method_call(&source.code, start, token.len()) {
                continue;
            }
            let open = skip_whitespace(&source.code, start + token.len()).unwrap_or(start);
            let message = skip_whitespace(&source.code, open + 1)
                .and_then(|at| string_literal(&source.original, at));
            let complaint = match message {
                Some(text) if text.starts_with(INVARIANT_PREFIX) => continue,
                Some(text) => format!(
                    "`expect(\"{text}\")` must state the invariant it relies on, starting with \
                     `{INVARIANT_PREFIX}` (charter §1.1)"
                ),
                None => format!(
                    "`expect` needs a string-literal message starting with `{INVARIANT_PREFIX}` \
                     (charter §1.1)"
                ),
            };
            findings.push(Finding::new(
                root,
                &source.path,
                line_of(&source.code, start),
                complaint,
            ));
        }
    }
    Ok(findings)
}

/// Whether the identifier at `start` (of `len` characters) is a method call: preceded by `.` and
/// followed by `(`, ignoring whitespace.
fn is_method_call(code: &[char], start: usize, len: usize) -> bool {
    let dot = code[..start].iter().rev().find(|c| !c.is_whitespace()) == Some(&'.');
    let paren = skip_whitespace(code, start + len).is_some_and(|i| code[i] == '(');
    dot && paren
}

/// The contents of the plain or raw string literal starting at `at` in `original`, if one does.
fn string_literal(original: &[char], at: usize) -> Option<String> {
    let mut i = at;
    let raw = original.get(i) == Some(&'r');
    if raw {
        i += 1;
    }
    let mut hashes = 0;
    while raw && original.get(i) == Some(&'#') {
        hashes += 1;
        i += 1;
    }
    if original.get(i) != Some(&'"') {
        return None;
    }
    let body = i + 1;
    let mut j = body;
    while j < original.len() {
        match original[j] {
            '\\' if !raw => j += 2,
            '"' if original[j + 1..]
                .iter()
                .take(hashes)
                .filter(|&&c| c == '#')
                .count()
                == hashes =>
            {
                return Some(original[body..j].iter().collect());
            }
            _ => j += 1,
        }
    }
    None
}

/// Check C12: no `let _ =` (or `let _: T =`) in production code of the simulation crates.
///
/// A discarded value there is almost always a discarded `Result`, which hides a failure the
/// simulation must report (charter §1.1). Bind and handle it, or propagate it with `?`.
///
/// # Errors
/// [`CheckError::Io`] if a directory or file cannot be read.
pub fn no_discarded_results_in_simulation(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let mut findings = Vec::new();
    for source in production_sources(root, true)? {
        let tokens = word_tokens(&source.code);
        for pair in tokens.windows(2) {
            let [(start, keyword), (wild, pattern)] = pair else {
                continue;
            };
            if keyword != "let" || pattern != "_" {
                continue;
            }
            let follows = skip_whitespace(&source.code, wild + 1).map(|i| source.code[i]);
            if matches!(follows, Some('=' | ':')) {
                findings.push(Finding::new(
                    root,
                    &source.path,
                    line_of(&source.code, *start),
                    "`let _` discards a value, usually a `Result`; handle it or propagate it \
                     with `?` (charter §1.1)",
                ));
            }
        }
    }
    Ok(findings)
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

    fn lines(findings: &[Finding]) -> Vec<String> {
        let mut out: Vec<String> = findings
            .iter()
            .map(|f| format!("{}:{}", slash_path(&f.path), f.line))
            .collect();
        out.sort();
        out
    }

    #[test]
    fn c06_reports_embedded_assets_in_production_only() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[
            (
                "crates/sim_core/src/catalog.rs",
                "//! x\nconst U: &str = include_str!(\"../../../assets/catalog/units.ron\");\n\
                 // include_str!(\"../assets/in_a_comment.ron\")\n\
                 const S: &str = \"include_str!(\\\"assets/in_a_string\\\")\";\n\
                 const B: &[u8] = include_bytes! (\n    concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/assets/x.png\"),\n);\n\
                 const OK: &str = include_str!(\"table.ron\");\n\
                 #[cfg(test)]\nmod tests {\n    const T: &str = include_str!(\"../assets/t.ron\");\n}\n",
            ),
            (
                "crates/sim_core/tests/t.rs",
                "//! x\nconst T: &str = include_str!(\"../../assets/t.ron\");\n",
            ),
            (
                "crates/sim_fixtures/src/lib.rs",
                "//! x\nconst T: &str = include_str!(\"../../assets/t.ron\");\n",
            ),
        ])?;
        let findings = no_embedded_assets(repo.path())?;
        assert_eq!(
            lines(&findings),
            [
                "crates/sim_core/src/catalog.rs:2",
                "crates/sim_core/src/catalog.rs:5",
            ]
        );
        Ok(())
    }

    #[test]
    fn c10_reports_expect_messages_without_an_invariant() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[
            (
                "crates/client/src/bin/tool.rs",
                "//! x\n#[expect(clippy::too_many_lines, reason = \"x\")]\nfn main() {\n\
                 \x20   a.expect(\"invariant: checked above\");\n\
                 \x20   b.expect(\"failed to create directory\");\n\
                 \x20   c\n        .expect(\n            \"theme must exist\",\n        );\n\
                 \x20   d.expect(MESSAGE);\n\
                 \x20   e.expect(r#\"invariant: \"quoted\" raw\"#);\n\
                 \x20   f.expect(\"say \\\"invariant:\\\"\");\n\
                 \x20   expect(\"a free function, not a method\");\n\
                 }\n\
                 #[cfg(test)]\nmod tests {\n    fn t() { x.expect(\"test code may say anything\"); }\n}\n",
            ),
            (
                "crates/client/tests/t.rs",
                "//! x\nfn t() { x.expect(\"tests may say anything\"); }\n",
            ),
        ])?;
        let findings = expect_messages_state_invariants(repo.path())?;
        assert_eq!(
            lines(&findings),
            [
                "crates/client/src/bin/tool.rs:10",
                "crates/client/src/bin/tool.rs:12",
                "crates/client/src/bin/tool.rs:5",
                "crates/client/src/bin/tool.rs:7",
            ]
        );
        Ok(())
    }

    #[test]
    fn c12_reports_discarded_values_in_simulation_crates_only() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[
            (
                "crates/sim_core/src/world.rs",
                "//! x\nfn f() {\n    let _ = spawn();\n    let _: u8 = g();\n    let _unused = h();\n\
                 \x20   let (_, b) = pair();\n    let _ignored: u8 = 0;\n    // let _ = in_a_comment();\n}\n\
                 #[cfg(test)]\nmod tests {\n    fn t() { let _ = spawn(); }\n}\n",
            ),
            (
                "crates/client/src/lib.rs",
                "//! x\nfn f() { let _ = spawn(); }\n",
            ),
            (
                "crates/sim_map/tests/t.rs",
                "//! x\nfn f() { let _ = spawn(); }\n",
            ),
        ])?;
        let findings = no_discarded_results_in_simulation(repo.path())?;
        assert_eq!(
            lines(&findings),
            [
                "crates/sim_core/src/world.rs:3",
                "crates/sim_core/src/world.rs:4"
            ]
        );
        Ok(())
    }

    #[test]
    fn string_literals_are_read_with_escapes_and_raw_hashes() {
        let chars = |s: &str| s.chars().collect::<Vec<_>>();
        assert_eq!(
            string_literal(&chars("\"a\\\"b\" rest"), 0).as_deref(),
            Some("a\\\"b")
        );
        assert_eq!(
            string_literal(&chars("r#\"x\"y\"# rest"), 0).as_deref(),
            Some("x\"y")
        );
        assert_eq!(string_literal(&chars("MESSAGE"), 0), None);
    }
}
