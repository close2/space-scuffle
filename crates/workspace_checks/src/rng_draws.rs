//! Check C25: every seeded draw in the simulation crates names a fixed-width type (bug B-140).
//!
//! # Responsibility
//! `rand` samples a `usize` or `isize` range, and `gen::<usize>()`, by the pointer width: one
//! 64-bit word on a 64-bit target, one 32-bit word on `wasm32`. A draw of that kind in the
//! simulation would break charter §3.1's bit-for-bit determinism across targets, and in a seeded
//! test corpus it draws a different corpus on each target, so a pinned digest cannot agree
//! (B-140: `sim_map`'s `validation_golden_tests` indexed its corpora with `usize` draws). The
//! type of an unannotated `gen_range(0..n)` is whatever inference picks, which a reader cannot
//! see at the call, so this check requires every `gen_range` and `gen` call to name its type
//! with a turbofish (`rng.gen_range::<u32, _>(0..n)`, `rng.gen::<i64>()`), and that type to be
//! neither `usize` nor `isize`. An index is drawn as a `u32` and converted after the draw.
//! Plan: `docs/plans/2026-11-18-contour-footprints.md`, Amendment 5.
//!
//! # How it fits
//! `tests/charter.rs` runs [`seeded_draws_name_a_fixed_width`] on the repository; the unit tests
//! below run it on planted trees. Clippy cannot express the rule: `disallowed-methods` bans a
//! method whatever its type parameter, and no lint sees the inferred type of a range.
//!
//! # Invariants
//! - Every Rust file of [`DRAW_CHECKED_CRATES`] is judged, test code, examples and benches
//!   included, since their seeded corpora pin digests; comments and literals are blanked first.
//! - A method name of [`DRAW_METHODS`] is judged when it is called after a `.`; a call without
//!   `::<` is reported, and so is one whose first type argument is `usize` or `isize`.
//! - Limit: the check reads text. It does not follow a draw through a generic helper whose
//!   caller picks the type, nor `rand::distributions` objects (`Uniform`, `Slice`); none is used
//!   in these crates, and a reviewer judges one that is added.

use crate::source::{line_of, word_tokens};
use crate::{code_text, rust_files, CheckError, Finding, Scope};
use std::path::Path;

/// Crates whose every Rust file is judged: the simulation crates and their test fixtures.
pub const DRAW_CHECKED_CRATES: [&str; 4] = ["sim_types", "sim_map", "sim_core", "sim_fixtures"];

/// `rand::Rng` methods whose sampled type is their first type parameter.
pub const DRAW_METHODS: [&str; 2] = ["gen_range", "gen"];

/// Types whose draws `rand` samples by the pointer width.
pub const PLATFORM_WIDTH_TYPES: [&str; 2] = ["usize", "isize"];

/// Check C25: every `gen_range` and `gen` call in [`DRAW_CHECKED_CRATES`] names a fixed-width
/// type with a turbofish.
///
/// # Errors
/// [`CheckError::Io`] if a directory or file cannot be read.
pub fn seeded_draws_name_a_fixed_width(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let crates_dir = root.join("crates");
    let mut findings = Vec::new();
    for path in rust_files(root, Scope::All)? {
        if !DRAW_CHECKED_CRATES
            .iter()
            .any(|krate| path.starts_with(crates_dir.join(krate)))
        {
            continue;
        }
        let chars: Vec<char> = code_text(&path, Scope::All)?.chars().collect();
        for (start, token) in word_tokens(&chars) {
            if !DRAW_METHODS.contains(&token.as_str()) || !previous_non_space_is(&chars, start, '.')
            {
                continue;
            }
            let after = skip_spaces(&chars, start + token.chars().count());
            let message = match type_argument(&chars, after) {
                Call::Untyped => format!(
                    "`.{token}(` names no type; inference picks it, and a `usize` draw differs \
                     between 64-bit targets and wasm32 (B-140): write `.{token}::<u32, ..>` or \
                     another fixed-width type"
                ),
                Call::Typed(ty) if PLATFORM_WIDTH_TYPES.contains(&ty.as_str()) => format!(
                    "`.{token}::<{ty}>` is sampled by the pointer width, so 64-bit targets and \
                     wasm32 draw differently (B-140): draw a `u32` and convert it after the draw"
                ),
                Call::Typed(_) | Call::Other => continue,
            };
            findings.push(Finding::new(root, &path, line_of(&chars, start), message));
        }
    }
    Ok(findings)
}

/// What follows a draw method's name.
enum Call {
    /// `(` directly: the type is left to inference.
    Untyped,
    /// `::<T` with `T` the first type argument, trimmed.
    Typed(String),
    /// Anything else, such as a field or a function definition's name.
    Other,
}

/// Classifies the text at `index`, just after a method name and its spaces.
fn type_argument(chars: &[char], index: usize) -> Call {
    let rest = &chars[index.min(chars.len())..];
    if rest.first() == Some(&'(') {
        return Call::Untyped;
    }
    if !rest.starts_with(&[':', ':']) {
        return Call::Other;
    }
    let after_colons = skip_spaces(chars, index + 2);
    if chars.get(after_colons) != Some(&'<') {
        return Call::Other;
    }
    let ty: String = chars[after_colons + 1..]
        .iter()
        .take_while(|&&c| c != ',' && c != '>')
        .collect();
    Call::Typed(ty.trim().to_owned())
}

/// The first index at or after `index` that is not whitespace (or the text's length).
fn skip_spaces(chars: &[char], index: usize) -> usize {
    let mut i = index.min(chars.len());
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

/// Whether the last character before `index` that is not whitespace is `expected`.
fn previous_non_space_is(chars: &[char], index: usize, expected: char) -> bool {
    chars[..index.min(chars.len())]
        .iter()
        .rev()
        .find(|c| !c.is_whitespace())
        .is_some_and(|&c| c == expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slash_path;
    use std::error::Error;
    use std::fs;

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
        findings
            .iter()
            .map(|f| format!("{}:{}", slash_path(&f.path), f.line))
            .collect()
    }

    #[test]
    fn untyped_and_platform_width_draws_are_reported() -> Result<(), Box<dyn Error>> {
        let dir = tree(&[
            (
                "crates/sim_map/tests/corpus_tests.rs",
                "//! x\nfn f(rng: &mut R, v: &[u8]) {\n    let a = v[rng.gen_range(0..v.len())];\n\
                 \x20   let b = rng\n        .gen_range :: < usize, _ >(0..4);\n\
                 \x20   let c: i64 = rng.gen();\n    let d = rng.gen::<isize>();\n}\n",
            ),
            (
                "crates/sim_core/src/combat.rs",
                "//! x\nfn g(rng: &mut R) -> i64 { rng.gen_range(0..=9_i64) }\n",
            ),
            (
                "crates/sim_fixtures/src/lib.rs",
                "//! x\nfn h(rng: &mut R) -> u8 { rng.gen() }\n",
            ),
        ])?;
        let findings = seeded_draws_name_a_fixed_width(dir.path())?;
        assert_eq!(
            lines(&findings),
            [
                "crates/sim_core/src/combat.rs:2",
                "crates/sim_fixtures/src/lib.rs:2",
                "crates/sim_map/tests/corpus_tests.rs:3",
                "crates/sim_map/tests/corpus_tests.rs:5",
                "crates/sim_map/tests/corpus_tests.rs:6",
                "crates/sim_map/tests/corpus_tests.rs:7",
            ],
            "{findings:?}"
        );
        assert!(findings[0].message.contains("names no type"));
        assert!(findings[3].message.contains("pointer width"));
        Ok(())
    }

    #[test]
    fn typed_draws_comments_definitions_and_other_crates_pass() -> Result<(), Box<dyn Error>> {
        let dir = tree(&[
            (
                "crates/sim_map/tests/corpus_tests.rs",
                "//! rng.gen_range(0..n)\n// rng.gen()\nfn gen_range(n: u32) -> u32 { n }\n\
                 fn f(rng: &mut R, gen: u32) {\n    let a = rng.gen_range::<u32, _>(0..gen);\n\
                 \x20   let b = rng.gen::<i64>();\n    let c = \"rng.gen_range(0..9)\";\n\
                 \x20   let d = cfg.gen;\n    let e = rng.gen_range ::<f64, _>(0.0..1.0);\n}\n",
            ),
            (
                "crates/client/src/effects.rs",
                "//! x\nfn f(rng: &mut R) -> usize { rng.gen_range(0..9) }\n",
            ),
        ])?;
        let findings = seeded_draws_name_a_fixed_width(dir.path())?;
        assert!(findings.is_empty(), "{findings:?}");
        Ok(())
    }
}
