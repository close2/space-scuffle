//! The source checks C3, C4, C7, C9, C13 and C14 of the charter.
//!
//! # Responsibility
//! One public function per check. Each takes the repository root (and, for C7, its allowlist),
//! reads what the check covers, and returns every violation as a [`Finding`].
//!
//! # How it fits
//! `tests/charter.rs` calls these on the real repository; the unit tests below call them on
//! temporary trees with planted violations, proving each check can fail. C2 lives in
//! `metadata.rs` because it reads Cargo's graph rather than sources.
//!
//! # Invariants
//! - A check never stops at the first violation: it returns all of them.
//! - Rust checks scan [`code_text`], so prose in comments and messages in strings cannot trigger
//!   or hide a violation. C3 is the exception: work markers live in comments, so it reads raw text.
//! - This file must pass C3 itself, so the markers are assembled with `concat!` and never written
//!   out whole.

use crate::source::{is_ident_char, line_of, walk_files, word_tokens};
use crate::{code_text, rust_files, slash_path, CheckError, Finding, Scope};
use std::fs;
use std::path::{Path, PathBuf};

/// The simulation crates, whose state must be deterministic on every platform (charter §3.1).
pub const SIMULATION_CRATES: [&str; 3] = ["sim_types", "sim_map", "sim_core"];

/// The library crates that must return typed errors (charter §1.1).
pub const LIBRARY_CRATES: [&str; 4] = ["sim_types", "sim_map", "sim_core", "asset_baker"];

/// Unfinished-work markers C3 rejects, assembled so this file does not contain them.
const WORK_MARKERS: [&str; 3] = [
    concat!("TO", "DO"),
    concat!("FIX", "ME"),
    concat!("X", "XX"),
];

/// Directories, relative to the root, whose every text file C3 reads.
const MARKER_SCOPES: [&str; 2] = ["crates", "assets/shaders"];

/// Message for a float in a simulation crate beyond the C7 allowlist.
pub const FLOAT_RATCHET_MESSAGE: &str = "authoritative state must be fixed-point (charter §3.1); \
     a presentation or authoring boundary must be added to the allowlist with a reason";

/// Interior-mutability type names that make a `static` global mutable state (C13).
const MUTABLE_CELLS: [&str; 5] = ["Mutex", "RwLock", "RefCell", "Cell", "UnsafeCell"];

/// One file of a simulation crate allowed to name `f32`/`f64` a fixed number of times (C7).
#[derive(Clone, Copy, Debug)]
pub struct FloatAllowance {
    /// File path relative to the repository root, with `/` separators.
    pub path: &'static str,
    /// Exact number of float tokens the file's production code holds today.
    pub count: usize,
    /// Why floats are legitimate there (a presentation or authoring boundary).
    pub reason: &'static str,
}

/// Check C3: no unfinished-work marker token in any text file under `crates/` or
/// `assets/shaders/`.
///
/// A marker counts only as a whole word. Files that are not UTF-8 (video fixtures, images) cannot
/// carry a comment and are skipped.
///
/// # Errors
/// [`CheckError::Io`] if a directory or file cannot be read.
pub fn no_work_markers(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let mut files = Vec::new();
    for scope in MARKER_SCOPES {
        walk_files(&root.join(scope), &mut files)?;
    }
    let mut findings = Vec::new();
    for path in files {
        let bytes = read_bytes(&path)?;
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            for (_, token) in word_tokens(&chars) {
                if WORK_MARKERS.contains(&token.as_str()) {
                    findings.push(Finding::new(
                        root,
                        &path,
                        index + 1,
                        format!(
                            "unfinished-work marker `{token}`; finish the work or record it as \
                             a plan item (charter §1.1)"
                        ),
                    ));
                }
            }
        }
    }
    Ok(findings)
}

/// Check C4: every Rust file under `crates/` begins with a `//!` module doc comment.
///
/// # Errors
/// [`CheckError::Io`] if a directory or file cannot be read.
pub fn module_docs(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let mut findings = Vec::new();
    for path in rust_files(root, Scope::All)? {
        let text = read_text(&path)?;
        if !text.starts_with("//!") {
            findings.push(Finding::new(
                root,
                &path,
                1,
                "file does not begin with a `//!` module doc comment stating responsibility, \
                 fit and invariants (charter §2.1)",
            ));
        }
    }
    Ok(findings)
}

/// Check C7: floats in the simulation crates' production code appear only in allowlisted files,
/// exactly as often as the allowlist records.
///
/// A float token is `f32` or `f64`, or a numeric literal with that suffix (`1.0f32`). An
/// unlisted file is reported on every line that holds one. A listed file with more tokens is
/// reported at the first token over the limit; one with fewer must lower its entry, so the
/// ratchet only ever tightens. An entry naming a file that is not production code of a
/// simulation crate is reported as stale.
///
/// # Errors
/// [`CheckError::Io`] if a directory or file cannot be read.
pub fn float_ratchet(
    root: &Path,
    allowances: &[FloatAllowance],
) -> Result<Vec<Finding>, CheckError> {
    let mut findings = Vec::new();
    let files = simulation_production_files(root)?;
    let relative: Vec<String> = files
        .iter()
        .map(|path| slash_path(path.strip_prefix(root).unwrap_or(path)))
        .collect();
    for (path, rel) in files.iter().zip(&relative) {
        let chars: Vec<char> = code_text(path, Scope::Production)?.chars().collect();
        let floats: Vec<usize> = word_tokens(&chars)
            .into_iter()
            .filter(|(_, token)| is_float_token(token))
            .map(|(start, _)| line_of(&chars, start))
            .collect();
        match allowances.iter().find(|allowance| allowance.path == rel) {
            None => {
                let mut lines = floats;
                lines.dedup();
                for line in lines {
                    findings.push(Finding::new(root, path, line, FLOAT_RATCHET_MESSAGE));
                }
            }
            Some(allowance) if floats.len() > allowance.count => {
                findings.push(Finding::new(
                    root,
                    path,
                    floats[allowance.count],
                    format!(
                        "{} float tokens where the allowlist permits {}; {FLOAT_RATCHET_MESSAGE}",
                        floats.len(),
                        allowance.count
                    ),
                ));
            }
            Some(allowance) if floats.len() < allowance.count => {
                findings.push(Finding::new(
                    root,
                    path,
                    1,
                    format!(
                        "{} float tokens, fewer than the allowlist's {}: lower the entry in \
                         crates/workspace_checks/tests/charter.rs (the ratchet only tightens)",
                        floats.len(),
                        allowance.count
                    ),
                ));
            }
            Some(_) => {}
        }
    }
    for allowance in allowances {
        if !relative.iter().any(|rel| rel == allowance.path) {
            findings.push(Finding::new(
                root,
                Path::new(allowance.path),
                1,
                "C7 allowlist entry names a file that is not production code of a simulation \
                 crate; remove the entry",
            ));
        }
    }
    Ok(findings)
}

/// Production Rust files of the simulation crates, sorted by path.
fn simulation_production_files(root: &Path) -> Result<Vec<PathBuf>, CheckError> {
    let crates_dir = root.join("crates");
    Ok(rust_files(root, Scope::Production)?
        .into_iter()
        .filter(|path| {
            SIMULATION_CRATES
                .iter()
                .any(|krate| path.starts_with(crates_dir.join(krate)))
        })
        .collect())
}

/// Whether `token` is `f32`/`f64` or a decimal literal with that suffix.
fn is_float_token(token: &str) -> bool {
    let typed = token == "f32" || token == "f64";
    let suffixed = token.starts_with(|c: char| c.is_ascii_digit())
        && !token.starts_with("0x")
        && !token.starts_with("0b")
        && !token.starts_with("0o")
        && (token.ends_with("f32") || token.ends_with("f64"));
    typed || suffixed
}

/// The disallowed paths each simulation crate's `clippy.toml` must name, by configuration key.
const REQUIRED_CLIPPY_PATHS: [(&str, [&str; 2]); 2] = [
    (
        "disallowed-types",
        ["std::collections::HashMap", "std::collections::HashSet"],
    ),
    (
        "disallowed-methods",
        ["std::time::Instant::now", "std::time::SystemTime::now"],
    ),
];

/// Check C9: each simulation crate has a `clippy.toml` whose `disallowed-types` names
/// `HashMap` and `HashSet` and whose `disallowed-methods` names `Instant::now` and
/// `SystemTime::now` (charter §3.1). Clippy itself then rejects their use.
///
/// Paths count whether written as plain strings or as `{ path = "...", reason = "..." }`.
///
/// # Errors
/// [`CheckError::Io`] if a `clippy.toml` exists but cannot be read.
pub fn simulation_clippy_config(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let mut findings = Vec::new();
    for krate in SIMULATION_CRATES {
        let path = root.join("crates").join(krate).join("clippy.toml");
        if !path.is_file() {
            findings.push(Finding::new(
                root,
                &path,
                1,
                "simulation crate has no clippy.toml banning hashed collections and clocks \
                 (charter §3.1)",
            ));
            continue;
        }
        let text = read_text(&path)?;
        for (key, required) in REQUIRED_CLIPPY_PATHS {
            let Some((line, values)) = toml_string_array(&text, key) else {
                findings.push(Finding::new(
                    root,
                    &path,
                    1,
                    format!("`{key}` is missing (charter §3.1)"),
                ));
                continue;
            };
            for wanted in required {
                if !values.iter().any(|value| value == wanted) {
                    findings.push(Finding::new(
                        root,
                        &path,
                        line,
                        format!("`{key}` does not name `{wanted}` (charter §3.1)"),
                    ));
                }
            }
        }
    }
    Ok(findings)
}

/// Finds the top-level TOML array `key = [ ... ]` in `text` and returns its line and the paths
/// it names: every string that is an array element or the value of a `path` field.
fn toml_string_array(text: &str, key: &str) -> Option<(usize, Vec<String>)> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|line| {
        line.strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })?;
    let body: String = lines[start..]
        .iter()
        .map(|line| strip_toml_comment(line))
        .collect::<Vec<_>>()
        .join("\n");
    let open = body.find('[')?;
    let mut values = Vec::new();
    let mut depth = 0usize;
    let mut previous = String::new();
    let mut chars = body[open..].chars();
    while let Some(c) = chars.next() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            '"' => {
                let value: String = chars.by_ref().take_while(|&q| q != '"').collect();
                let context: String = previous.chars().filter(|c| !c.is_whitespace()).collect();
                if context.ends_with('[') || context.ends_with(',') || context.ends_with("path=") {
                    values.push(value);
                }
                previous.clear();
                continue;
            }
            _ => {}
        }
        previous.push(c);
        if c == ',' || c == '{' || c == '[' {
            previous.clear();
            previous.push(c);
        }
    }
    Some((start + 1, values))
}

/// `line` without a trailing `#` comment; a `#` inside a double-quoted string is kept.
fn strip_toml_comment(line: &str) -> &str {
    let mut in_string = false;
    for (index, c) in line.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

/// Check C13: no `static` in production code has a type containing `Mutex`, `RwLock`,
/// `RefCell`, `Cell`, `UnsafeCell` or an `Atomic*` type (charter §1.1: no global mutable state).
///
/// Write-once cells (`OnceLock`, `OnceCell`, `LazyLock`) are allowed. `static` items inside
/// `thread_local!` are checked like any other.
///
/// # Errors
/// [`CheckError::Io`] if a directory or file cannot be read.
pub fn no_mutable_statics(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let mut findings = Vec::new();
    for path in rust_files(root, Scope::Production)? {
        let chars: Vec<char> = code_text(&path, Scope::Production)?.chars().collect();
        for (start, token) in word_tokens(&chars) {
            if token != "static" || (start > 0 && chars[start - 1] == '\'') {
                continue;
            }
            let declared = declared_type(&chars, start + token.len());
            let declared_chars: Vec<char> = declared.chars().collect();
            let mutable = word_tokens(&declared_chars).into_iter().find(|(_, name)| {
                MUTABLE_CELLS.contains(&name.as_str()) || name.starts_with("Atomic")
            });
            if let Some((_, name)) = mutable {
                findings.push(Finding::new(
                    root,
                    &path,
                    line_of(&chars, start),
                    format!(
                        "`static` of type `{}` holding `{name}` is global mutable state \
                         (charter §1.1); pass the state explicitly",
                        declared.split_whitespace().collect::<Vec<_>>().join(" ")
                    ),
                ));
            }
        }
    }
    Ok(findings)
}

/// The text between the `:` after a `static` name and the `=` or `;` that ends its type.
fn declared_type(chars: &[char], after_static: usize) -> String {
    let Some(colon) = chars[after_static..]
        .iter()
        .position(|&c| c == ':' || c == '=' || c == ';')
        .map(|offset| after_static + offset)
        .filter(|&index| chars[index] == ':')
    else {
        return String::new();
    };
    chars[colon + 1..]
        .iter()
        .take_while(|&&c| c != '=' && c != ';')
        .collect()
}

/// Check C14: no `pub fn` in the production code of a library crate returns
/// `Result<_, String>` or `Result<_, Box<dyn ...>>` (charter §1.1: typed `thiserror` errors).
///
/// # Errors
/// [`CheckError::Io`] if a directory or file cannot be read.
pub fn typed_library_errors(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let crates_dir = root.join("crates");
    let mut findings = Vec::new();
    for path in rust_files(root, Scope::Production)? {
        if !LIBRARY_CRATES
            .iter()
            .any(|krate| path.starts_with(crates_dir.join(krate)))
        {
            continue;
        }
        let chars: Vec<char> = code_text(&path, Scope::Production)?.chars().collect();
        let tokens = word_tokens(&chars);
        for (position, (start, token)) in tokens.iter().enumerate() {
            if token != "pub" || !is_plain_pub(&chars, *start) {
                continue;
            }
            let Some(fn_index) = tokens[position + 1..]
                .iter()
                .take(4)
                .take_while(|(_, word)| {
                    matches!(
                        word.as_str(),
                        "const" | "async" | "unsafe" | "extern" | "fn"
                    )
                })
                .find(|(_, word)| word == "fn")
                .map(|(index, _)| *index)
            else {
                continue;
            };
            let returned = return_type(&chars, fn_index);
            if let Some(error) = untyped_error(&returned) {
                findings.push(Finding::new(
                    root,
                    &path,
                    line_of(&chars, *start),
                    format!(
                        "public function returns `{returned}`; its error `{error}` must be a \
                         typed `thiserror` enum (charter §1.1)"
                    ),
                ));
            }
        }
    }
    Ok(findings)
}

/// Whether the `pub` at `start` is plain `pub` (not `pub(crate)` or `pub(super)`).
fn is_plain_pub(chars: &[char], start: usize) -> bool {
    let after = start + 3;
    let previous_ok = start == 0 || !is_ident_char(chars[start - 1]);
    let next = chars[after..].iter().find(|c| !c.is_whitespace());
    previous_ok && next != Some(&'(')
}

/// The return type of the function whose `fn` keyword starts at `fn_index`, with whitespace
/// removed; empty when it returns `()`.
fn return_type(chars: &[char], fn_index: usize) -> String {
    let mut depth = 0isize;
    let mut generics = 0isize;
    let mut i = fn_index;
    let mut seen_params = false;
    while i < chars.len() {
        let arrow = chars[i] == '-' && chars.get(i + 1) == Some(&'>');
        if arrow {
            if depth == 0 && generics == 0 && seen_params {
                return collect_return_type(chars, i + 2);
            }
            i += 2;
            continue;
        }
        match chars[i] {
            '<' => generics += 1,
            '>' => generics -= 1,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && generics == 0 {
                    seen_params = true;
                }
            }
            '{' | ';' if depth == 0 && generics == 0 => return String::new(),
            _ => {}
        }
        i += 1;
    }
    String::new()
}

/// Collects a return type starting at `from`, up to `{`, `;` or a `where` clause, outside
/// brackets; whitespace is removed.
fn collect_return_type(chars: &[char], from: usize) -> String {
    let mut depth = 0isize;
    let mut out = String::new();
    let mut i = from;
    while i < chars.len() {
        let c = chars[i];
        let arrow = c == '-' && chars.get(i + 1) == Some(&'>');
        if arrow {
            out.push_str("->");
            i += 2;
            continue;
        }
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            '{' | ';' if depth <= 0 => break,
            'w' if depth <= 0
                && chars[i..].iter().take(5).collect::<String>() == "where"
                && i > 0
                && !is_ident_char(chars[i - 1])
                && !chars.get(i + 5).copied().is_some_and(is_ident_char) =>
            {
                break;
            }
            _ => {}
        }
        if !c.is_whitespace() {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// If `returned` is a two-argument `Result` whose error is `String` or `Box<dyn ...>`, returns
/// that error type.
fn untyped_error(returned: &str) -> Option<String> {
    let open = returned.find("Result<")?;
    let prefix = &returned[..open];
    if !(prefix.is_empty() || prefix.ends_with("::")) {
        return None;
    }
    let inner = &returned[open + "Result<".len()..];
    let mut depth = 0isize;
    let mut args = vec![String::new()];
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '-' if chars.peek() == Some(&'>') => {
                chars.next();
                if let Some(last) = args.last_mut() {
                    last.push_str("->");
                }
                continue;
            }
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' if depth == 0 => break,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                args.push(String::new());
                continue;
            }
            _ => {}
        }
        if let Some(last) = args.last_mut() {
            last.push(c);
        }
    }
    let error = args.get(1)?.as_str();
    let string = matches!(
        error,
        "String" | "std::string::String" | "alloc::string::String"
    );
    let boxed = error
        .trim_start_matches("std::boxed::")
        .trim_start_matches("alloc::boxed::")
        .starts_with("Box<dyn");
    (string || boxed).then(|| error.to_string())
}

/// Reads a file as bytes, mapping the error to [`CheckError::Io`].
fn read_bytes(path: &Path) -> Result<Vec<u8>, CheckError> {
    fs::read(path).map_err(|source| CheckError::Io {
        path: path.to_path_buf(),
        source,
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
    fn c03_reports_every_marker_with_its_line() -> Result<(), Box<dyn Error>> {
        let marker = WORK_MARKERS[0];
        let other = WORK_MARKERS[1];
        let repo = tree(&[
            (
                "crates/a/src/lib.rs",
                &format!("//! ok\n// {marker}: later\nfn f() {{}}\n"),
            ),
            ("assets/shaders/x.wgsl", &format!("// fine\n\n// {other}\n")),
            (
                "crates/a/src/clean.rs",
                &format!("//! {marker}S and {marker}_LIST are words\n"),
            ),
            ("assets/other/y.wgsl", &format!("// {marker}\n")),
        ])?;
        let findings = no_work_markers(repo.path())?;
        assert_eq!(
            lines(&findings),
            ["assets/shaders/x.wgsl:3", "crates/a/src/lib.rs:2"]
        );
        Ok(())
    }

    #[test]
    fn c03_skips_binary_files() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[])?;
        let path = repo.path().join("crates/a/fixture.mp4");
        fs::create_dir_all(path.parent().ok_or("parent")?)?;
        let mut bytes = vec![0xff, 0xfe, 0x00];
        bytes.extend_from_slice(WORK_MARKERS[0].as_bytes());
        fs::write(path, bytes)?;
        assert_eq!(no_work_markers(repo.path())?, Vec::<Finding>::new());
        Ok(())
    }

    #[test]
    fn c04_reports_files_without_module_docs() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[
            ("crates/a/src/lib.rs", "//! documented\n"),
            ("crates/a/src/bare.rs", "fn f() {}\n"),
            ("crates/a/tests/late.rs", "\n//! too late\n"),
        ])?;
        let findings = module_docs(repo.path())?;
        assert_eq!(
            lines(&findings),
            ["crates/a/src/bare.rs:1", "crates/a/tests/late.rs:1"]
        );
        Ok(())
    }

    #[test]
    fn c07_reports_unlisted_exceeded_lowered_and_stale_entries() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[
            (
                "crates/sim_core/src/new.rs",
                "//! x\nfn f(a: f32) {}\n// f64 in a comment\n",
            ),
            (
                "crates/sim_core/src/listed.rs",
                "//! x\nfn f(a: f32) {}\nfn g() -> f64 { 1.0f64 }\n",
            ),
            ("crates/sim_map/src/lower.rs", "//! x\nfn f(a: f32) {}\n"),
            (
                "crates/sim_types/src/ok.rs",
                "//! x\nfn f(a: f32) {}\n#[cfg(test)]\nmod tests { fn t(_: f64) {} }\n",
            ),
            ("crates/sim_core/tests/t.rs", "//! x\nfn f(a: f32) {}\n"),
            ("crates/client/src/lib.rs", "//! x\nfn f(a: f32) {}\n"),
            (
                "crates/sim_core/src/hex.rs",
                "//! x\nconst X: u32 = 0x1f32;\n",
            ),
        ])?;
        let allow = |path, count| FloatAllowance {
            path,
            count,
            reason: "test",
        };
        let allowances = [
            allow("crates/sim_core/src/listed.rs", 2),
            allow("crates/sim_map/src/lower.rs", 2),
            allow("crates/sim_types/src/ok.rs", 1),
            allow("crates/sim_core/src/gone.rs", 1),
        ];
        let findings = float_ratchet(repo.path(), &allowances)?;
        assert_eq!(
            lines(&findings),
            [
                "crates/sim_core/src/gone.rs:1",
                "crates/sim_core/src/listed.rs:3",
                "crates/sim_core/src/new.rs:2",
                "crates/sim_map/src/lower.rs:1",
            ]
        );
        let unlisted = findings
            .iter()
            .find(|f| f.path.ends_with("new.rs"))
            .ok_or("unlisted file reported")?;
        assert!(unlisted.message.contains("must be fixed-point"));
        Ok(())
    }

    #[test]
    fn c09_reports_missing_files_keys_and_paths() -> Result<(), Box<dyn Error>> {
        let complete = "too-many-lines-threshold = 100\n\
            disallowed-types = [\n  { path = \"std::collections::HashMap\", reason = \"r\" },\n\
              \"std::collections::HashSet\",\n]\n\
            disallowed-methods = [\n  { path = \"std::time::Instant::now\", reason = \"r\" },\n\
              { path = \"std::time::SystemTime::now\", reason = \"r\" },\n]\n";
        let partial = "disallowed-types = [\n  { path = \"std::collections::HashMap\", \
            reason = \"not std::collections::HashSet\" }, # \"std::collections::HashSet\"\n]\n";
        let repo = tree(&[
            ("crates/sim_types/clippy.toml", complete),
            ("crates/sim_map/clippy.toml", partial),
        ])?;
        let findings = simulation_clippy_config(repo.path())?;
        let messages: Vec<String> = findings.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "crates/sim_map/clippy.toml:1: `disallowed-types` does not name \
                 `std::collections::HashSet` (charter §3.1)",
                "crates/sim_map/clippy.toml:1: `disallowed-methods` is missing (charter §3.1)",
                "crates/sim_core/clippy.toml:1: simulation crate has no clippy.toml banning \
                 hashed collections and clocks (charter §3.1)",
            ]
        );
        Ok(())
    }

    #[test]
    fn c13_reports_mutable_statics_but_not_write_once_or_test_code() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[
            (
                "crates/a/src/lib.rs",
                "//! x\nstatic OK: OnceLock<u8> = OnceLock::new();\n\
                 static COUNT: AtomicU32 = AtomicU32::new(0);\nfn f(_: &'static str) {}\n\
                 thread_local! {\n    static SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::new());\n}\n\
                 static TABLE: [u32; 2] = [0, 1];\n\
                 // static C: Mutex<u8> in a comment\n\
                 #[cfg(test)]\nmod tests { static L: Mutex<()> = Mutex::new(()); }\n",
            ),
            ("crates/a/tests/t.rs", "//! x\nstatic L: Mutex<()> = Mutex::new(());\n"),
        ])?;
        let findings = no_mutable_statics(repo.path())?;
        assert_eq!(
            lines(&findings),
            ["crates/a/src/lib.rs:3", "crates/a/src/lib.rs:6"]
        );
        Ok(())
    }

    #[test]
    fn c14_reports_untyped_errors_in_public_library_functions() -> Result<(), Box<dyn Error>> {
        let repo = tree(&[
            (
                "crates/sim_map/src/lib.rs",
                "//! x\npub fn a() -> Result<u8, String> { Ok(0) }\n\
                 pub(crate) fn b() -> Result<u8, String> { Ok(0) }\n\
                 pub fn c() -> Result<Vec<u8>, MapError> { Ok(vec![]) }\n\
                 pub const fn d(\n    x: u8,\n) -> std::result::Result<u8, Box<dyn std::error::Error>>\nwhere\n{\n    Ok(x)\n}\n\
                 pub fn e() -> Result<String, MapError> { Err(MapError) }\n\
                 pub fn f() -> io::Result<u8> { Ok(0) }\n\
                 pub fn g() -> Result<Box<dyn Fn() -> u8>, String> { Err(String::new()) }\n\
                 pub fn h<F: Fn() -> u8>(f: F) -> u8 { f() }\n",
            ),
            ("crates/client/src/lib.rs", "//! x\npub fn a() -> Result<u8, String> { Ok(0) }\n"),
            ("crates/sim_core/tests/t.rs", "//! x\npub fn a() -> Result<u8, String> { Ok(0) }\n"),
        ])?;
        let findings = typed_library_errors(repo.path())?;
        assert_eq!(
            lines(&findings),
            [
                "crates/sim_map/src/lib.rs:14",
                "crates/sim_map/src/lib.rs:2",
                "crates/sim_map/src/lib.rs:5",
            ]
        );
        Ok(())
    }

    #[test]
    fn float_tokens_include_suffixed_literals_but_not_hex() {
        assert!(is_float_token("f32"));
        assert!(is_float_token("1f64"));
        assert!(is_float_token("0_5f32"));
        assert!(!is_float_token("0x1f32"));
        assert!(!is_float_token("f32x4"));
    }
}
