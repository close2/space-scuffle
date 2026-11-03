//! Listing Rust sources and reducing them to the code a check should judge.
//!
//! # Responsibility
//! Owns the two questions every source check asks: *which files* (all Rust under `crates/`, or
//! only production code) and *which text* (code with comments and literal contents blanked, and,
//! for production, with `#[cfg(test)]` items removed).
//!
//! # How it fits
//! The checks in `checks.rs` scan the text returned by [`code_text`] token by token. Blanking
//! here, once, keeps a check from matching words in prose, doc comments or string messages.
//!
//! # Invariants
//! - Blanking replaces characters with spaces and keeps every newline, so the output has the
//!   same line count as the input and a match's line number is its line in the real file.
//! - Literal delimiters stay (`"`, `'`, `r#"`), only their contents are blanked, so blanked text
//!   still separates tokens where the source did.
//! - The lexer is not a Rust parser. It understands exactly what blanking needs: line and nested
//!   block comments, string, byte-string, C-string and raw-string literals, character literals,
//!   and the difference between a character literal and a lifetime.

use crate::CheckError;
use std::fs;
use std::path::{Path, PathBuf};

/// Which sources a check reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Code that ships: test directories, benches, test-named files, `#[cfg(test)]` items and the
    /// dev-only crates are left out.
    Production,
    /// Every Rust file under `crates/`.
    All,
}

/// Crates that exist only to test or check the others; their code is never production code.
const DEV_ONLY_CRATES: [&str; 2] = ["sim_fixtures", "workspace_checks"];

/// Rust files under `root/crates`, sorted by path.
///
/// With [`Scope::Production`] the list drops any file below a directory named `tests` or
/// `benches`, files named `tests.rs` or `*_tests.rs`, and the dev-only crates (`sim_fixtures`,
/// `workspace_checks`). Directories named `target` or starting with `.` are skipped, and symbolic
/// links are never followed.
///
/// # Errors
/// [`CheckError::Io`] if a directory under `root/crates` cannot be read.
pub fn rust_files(root: &Path, scope: Scope) -> Result<Vec<PathBuf>, CheckError> {
    let crates = root.join("crates");
    let mut files = Vec::new();
    walk_files(&crates, &mut files)?;
    files.retain(|path| path.extension().is_some_and(|ext| ext == "rs"));
    if scope == Scope::Production {
        files.retain(|path| is_production_path(&crates, path));
    }
    Ok(files)
}

/// Every regular file under `dir`, sorted by path, skipping symbolic links, `target` and hidden
/// directories. A missing `dir` yields no files.
///
/// # Errors
/// [`CheckError::Io`] if a directory exists but cannot be read.
pub(crate) fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), CheckError> {
    if !dir.is_dir() {
        return Ok(());
    }
    let io = |source| CheckError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut entries = fs::read_dir(dir)
        .map_err(io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io)?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let kind = entry.file_type().map_err(io)?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if kind.is_dir() {
            if name != "target" && !name.starts_with('.') {
                walk_files(&path, out)?;
            }
        } else if kind.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// Whether `path`, a file under `crates_dir`, is production code (see [`rust_files`]).
fn is_production_path(crates_dir: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(crates_dir) else {
        return false;
    };
    let parts: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    let Some((file_name, dirs)) = parts.split_last() else {
        return false;
    };
    let in_dev_crate = dirs
        .first()
        .is_some_and(|krate| DEV_ONLY_CRATES.contains(&krate.as_str()));
    let in_test_dir = dirs.iter().any(|dir| dir == "tests" || dir == "benches");
    let test_named = file_name == "tests.rs" || file_name.ends_with("_tests.rs");
    !(in_dev_crate || in_test_dir || test_named)
}

/// The code of the Rust file at `path`, ready for token scanning.
///
/// Comments and the contents of string and character literals are blanked (see
/// [`blank_comments_and_literals`]). With [`Scope::Production`], every top-level `#[cfg(test)]`
/// item is blanked too (see [`strip_test_items`]). Line numbers are preserved.
///
/// # Errors
/// [`CheckError::Io`] if the file cannot be read as UTF-8 text.
pub fn code_text(path: &Path, scope: Scope) -> Result<String, CheckError> {
    let source = fs::read_to_string(path).map_err(|source| CheckError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let code = blank_comments_and_literals(&source);
    Ok(match scope {
        Scope::Production => strip_test_items(&code),
        Scope::All => code,
    })
}

/// Whether `c` can continue an identifier or a numeric literal.
pub(crate) fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Blanks comments and literal contents in Rust source, keeping newlines and delimiters.
///
/// Line comments, doc comments and (nested) block comments become spaces. Inside string,
/// byte-string, C-string, raw-string and character literals only the delimiters remain.
/// Lifetimes such as `'a` and `'static` are kept as code.
#[must_use]
pub fn blank_comments_and_literals(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = chars.clone();
    let at = |i: usize| chars.get(i).copied();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && at(i + 1) == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out[i] = ' ';
                i += 1;
            }
        } else if c == '/' && at(i + 1) == Some('*') {
            i = blank_block_comment(&chars, &mut out, i);
        } else if c == '"' {
            i = blank_string(&chars, &mut out, i + 1);
        } else if c == '\'' {
            i = blank_char_or_skip_lifetime(&chars, &mut out, i);
        } else if is_ident_char(c) {
            let previous_is_ident = i > 0 && is_ident_char(chars[i - 1]);
            match raw_string_start(&chars, i).filter(|_| !previous_is_ident) {
                Some((hashes, body)) => i = blank_raw_string(&chars, &mut out, body, hashes),
                None => i += 1,
            }
        } else {
            i += 1;
        }
    }
    out.into_iter().collect()
}

/// Blanks the block comment opening at `start`, honouring nesting; returns the index after it.
fn blank_block_comment(chars: &[char], out: &mut [char], start: usize) -> usize {
    let mut depth = 0usize;
    let mut i = start;
    while i < chars.len() {
        let pair = (chars[i], chars.get(i + 1).copied());
        if pair == ('/', Some('*')) {
            depth += 1;
            out[i] = ' ';
            out[i + 1] = ' ';
            i += 2;
        } else if pair == ('*', Some('/')) {
            depth -= 1;
            out[i] = ' ';
            out[i + 1] = ' ';
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            if chars[i] != '\n' {
                out[i] = ' ';
            }
            i += 1;
        }
    }
    i
}

/// Blanks a non-raw string body starting at `body`; returns the index after the closing quote.
fn blank_string(chars: &[char], out: &mut [char], body: usize) -> usize {
    let mut i = body;
    while i < chars.len() {
        match chars[i] {
            '"' => return i + 1,
            '\\' => {
                out[i] = ' ';
                if let Some(next) = chars.get(i + 1) {
                    if *next != '\n' {
                        out[i + 1] = ' ';
                    }
                }
                i += 2;
            }
            '\n' => i += 1,
            _ => {
                out[i] = ' ';
                i += 1;
            }
        }
    }
    i
}

/// If a raw string (`r"`, `r#"`, `br"`, `cr#"`, ...) starts at `i`, returns its hash count and
/// the index of its first body character.
fn raw_string_start(chars: &[char], i: usize) -> Option<(usize, usize)> {
    let mut j = i;
    if matches!(chars.get(j), Some('b' | 'c')) {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let hashes_start = j;
    while chars.get(j) == Some(&'#') {
        j += 1;
    }
    (chars.get(j) == Some(&'"')).then_some((j - hashes_start, j + 1))
}

/// Blanks a raw string body starting at `body` and closed by `"` plus `hashes` hash marks;
/// returns the index after the closing delimiter.
fn blank_raw_string(chars: &[char], out: &mut [char], body: usize, hashes: usize) -> usize {
    let mut i = body;
    while i < chars.len() {
        if chars[i] == '"' && (1..=hashes).all(|k| chars.get(i + k) == Some(&'#')) {
            return i + 1 + hashes;
        }
        if chars[i] != '\n' {
            out[i] = ' ';
        }
        i += 1;
    }
    i
}

/// At a `'` at `start`: blanks a character literal's contents and returns the index after it,
/// or returns the index after the quote for a lifetime or label.
fn blank_char_or_skip_lifetime(chars: &[char], out: &mut [char], start: usize) -> usize {
    match (chars.get(start + 1), chars.get(start + 2)) {
        (Some('\\'), _) => {
            let mut i = start + 1;
            while i < chars.len() && chars[i] != '\n' {
                if chars[i] == '\\' {
                    out[i] = ' ';
                    if let Some(slot) = out.get_mut(i + 1) {
                        *slot = ' ';
                    }
                    i += 2;
                } else if chars[i] == '\'' {
                    return i + 1;
                } else {
                    out[i] = ' ';
                    i += 1;
                }
            }
            i
        }
        (Some(&c), Some('\'')) if c != '\n' => {
            out[start + 1] = ' ';
            start + 3
        }
        _ => start + 1,
    }
}

/// Blanks every top-level `#[cfg(test)]` item in already-blanked code, keeping newlines.
///
/// A top-level item is one whose `#[cfg(test)]` attribute starts in column 0. The blanked span
/// runs from the attribute through any further attributes and the item itself, which ends at the
/// first `;` outside brackets, or at the `}` closing its first top-level `{` (plus a directly
/// following `;`). A file whose first column holds `#![cfg(test)]` is test-only and is blanked
/// whole.
#[must_use]
pub fn strip_test_items(code: &str) -> String {
    let chars: Vec<char> = code.chars().collect();
    let mut out = chars.clone();
    let mut line_start = 0;
    while line_start < chars.len() {
        if starts_with_at(&chars, line_start, "#![cfg(test)]") {
            return code
                .chars()
                .map(|c| if c == '\n' { '\n' } else { ' ' })
                .collect();
        }
        let next_line = if starts_with_at(&chars, line_start, "#[cfg(test)]") {
            let end = item_end(&chars, line_start);
            for slot in &mut out[line_start..end] {
                if *slot != '\n' {
                    *slot = ' ';
                }
            }
            end
        } else {
            line_start
        };
        line_start = chars[next_line..]
            .iter()
            .position(|&c| c == '\n')
            .map_or(chars.len(), |offset| next_line + offset + 1);
    }
    out.into_iter().collect()
}

/// Whether `chars[at..]` begins with `pattern`.
fn starts_with_at(chars: &[char], at: usize, pattern: &str) -> bool {
    let mut rest = chars.get(at..).unwrap_or_default().iter();
    pattern.chars().all(|p| rest.next() == Some(&p))
}

/// Index just past the item whose outer attributes start at `start` (see [`strip_test_items`]).
fn item_end(chars: &[char], start: usize) -> usize {
    let mut depth = 0usize;
    let mut i = start;
    while i < chars.len() {
        match chars[i] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let mut j = i + 1;
                    while j < chars.len() && chars[j].is_whitespace() && chars[j] != '\n' {
                        j += 1;
                    }
                    return if chars.get(j) == Some(&';') {
                        j + 1
                    } else {
                        i + 1
                    };
                }
            }
            ';' if depth == 0 => return i + 1,
            _ => {}
        }
        i += 1;
    }
    i
}

/// One-based line number of character index `index` in `text`.
pub(crate) fn line_of(text: &[char], index: usize) -> usize {
    1 + text[..index.min(text.len())]
        .iter()
        .filter(|&&c| c == '\n')
        .count()
}

/// Identifier-like tokens (`[A-Za-z0-9_]+`, Unicode letters included) with their start indices.
pub(crate) fn word_tokens(text: &[char]) -> Vec<(usize, String)> {
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if is_ident_char(text[i]) {
            let start = i;
            while i < text.len() && is_ident_char(text[i]) {
                i += 1;
            }
            tokens.push((start, text[start..i].iter().collect()));
        } else {
            i += 1;
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn blanking_removes_comments_and_literal_contents_but_keeps_lines() {
        let source = "let a = \"f32 // no\"; // f32 comment\n/* outer /* f32 */ still */ f64\n";
        let blanked = blank_comments_and_literals(source);
        assert_eq!(blanked.lines().count(), source.lines().count());
        assert_eq!(blanked.len(), source.len());
        assert!(!blanked.contains("f32"));
        assert!(blanked.contains("f64"));
        assert!(blanked.contains("let a = \""));
    }

    #[test]
    fn raw_strings_and_char_literals_are_blanked_but_lifetimes_kept() {
        let source = "fn f<'a>(x: &'a str) -> &'static str { let q = '\"'; let e = '\\''; \
                      let r = r#\"he said \"f32\"\"#; let b = b'{'; x }";
        let blanked = blank_comments_and_literals(source);
        assert!(blanked.contains("'a>"));
        assert!(blanked.contains("&'static str"));
        assert!(!blanked.contains("f32"));
        assert!(!blanked.contains("he said"));
        assert_eq!(
            blanked.matches('{').count(),
            1,
            "the byte literal's brace is blanked"
        );
    }

    #[test]
    fn raw_identifiers_are_not_raw_strings() {
        let blanked = blank_comments_and_literals("let r#type = \"x\"; let br = 1;");
        assert!(blanked.contains("r#type"));
        assert!(blanked.contains("let br = 1;"));
    }

    #[test]
    fn test_items_are_removed_but_later_production_code_stays() {
        let code = "mod a;\n#[cfg(test)]\nmod tests;\n\nfn keep() -> f32 { 1.0 }\n\
                    #[cfg(test)]\n#[allow(dead_code)]\nmod inline {\n    fn t() -> f64 { 0.0 }\n}\n\
                    fn also_keep() {}\n";
        let stripped = strip_test_items(code);
        assert_eq!(stripped.lines().count(), code.lines().count());
        assert!(stripped.contains("mod a;"));
        assert!(!stripped.contains("mod tests"));
        assert!(stripped.contains("fn keep() -> f32"));
        assert!(!stripped.contains("f64"));
        assert!(stripped.contains("fn also_keep() {}"));
    }

    #[test]
    fn indented_cfg_test_is_not_top_level() {
        let code = "struct S {\n    #[cfg(test)]\n    count: u8,\n}\n";
        assert_eq!(strip_test_items(code), code);
    }

    #[test]
    fn inner_cfg_test_blanks_the_whole_file() {
        let stripped = strip_test_items("#![cfg(test)]\nfn f() -> f32 { 0.0 }\n");
        assert_eq!(stripped.trim(), "");
        assert_eq!(stripped.lines().count(), 2);
    }

    #[test]
    fn production_scope_drops_tests_benches_and_dev_crates() -> Result<(), Box<dyn Error>> {
        let tree = tempfile::tempdir()?;
        let files = [
            "crates/a/src/lib.rs",
            "crates/a/src/tests.rs",
            "crates/a/src/x/y_tests.rs",
            "crates/a/src/x/tests/mod.rs",
            "crates/a/tests/it.rs",
            "crates/a/benches/b.rs",
            "crates/a/build.rs",
            "crates/workspace_checks/src/lib.rs",
            "crates/sim_fixtures/src/lib.rs",
            "crates/a/src/notes.txt",
        ];
        for file in files {
            let path = tree.path().join(file);
            fs::create_dir_all(path.parent().ok_or("file has a parent")?)?;
            fs::write(&path, "//! doc\n")?;
        }
        let relative = |scope| -> Result<Vec<String>, CheckError> {
            Ok(rust_files(tree.path(), scope)?
                .iter()
                .map(|p| crate::slash_path(p.strip_prefix(tree.path()).unwrap_or(p)))
                .collect())
        };
        assert_eq!(
            relative(Scope::Production)?,
            ["crates/a/build.rs", "crates/a/src/lib.rs"]
        );
        assert_eq!(relative(Scope::All)?.len(), 9);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_not_followed() -> Result<(), Box<dyn Error>> {
        let tree = tempfile::tempdir()?;
        let outside = tree.path().join("outside");
        fs::create_dir_all(&outside)?;
        fs::write(outside.join("big.rs"), "fn f() {}\n")?;
        fs::create_dir_all(tree.path().join("crates/a/src"))?;
        std::os::unix::fs::symlink(&outside, tree.path().join("crates/a/assets"))?;
        assert_eq!(rust_files(tree.path(), Scope::All)?, Vec::<PathBuf>::new());
        Ok(())
    }

    #[test]
    fn line_of_counts_from_one() {
        let text: Vec<char> = "a\nb\nc".chars().collect();
        assert_eq!(line_of(&text, 0), 1);
        assert_eq!(line_of(&text, 2), 2);
        assert_eq!(line_of(&text, 4), 3);
    }
}
