//! Which source files make up each test target, and what those files call.
//!
//! # Responsibility
//! Maps every test target to the files compiled into it ([`SourceIndex`]), so that a change to a
//! target's own file, or to a support file it includes with `#[path = "..."] mod x;`, can select
//! it. It also scans code, with comments and string literals blanked out, for the markers the
//! catalog test relies on: the GPU harness (`run_test_child`, `GpuCommand`,
//! `virtual_gpu_command`), a valgrind invocation, and paths into a given crate (`sim_fixtures::`).
//! From those paths it records, per workspace crate, the targets of other crates that call it
//! ([`SourceIndex::callers_of`]).
//!
//! # How it fits
//! The selection uses the index for the "own file" trigger of `gpu` and `valgrind` targets
//! (blueprint §3.3) and to resolve a `downstream` contract written `@callers` (plan
//! 2026-11-11-faster-gates §4.3); the catalog test (check C22) uses the scanners to prove every
//! GPU and valgrind file is classed and that the computed callers equal an independent scan.
//!
//! # Invariants
//! - A library owns every file under its crate's `src/` except `src/main.rs` and `src/bin/**`.
//! - A binary owns its root file, its `src/bin/<name>/` directory and the modules it declares.
//! - An integration test owns its root file and, transitively, the files its `mod` declarations
//!   load, following Rust's rules: `#[path]` is relative to the declaring file's directory, and
//!   a file loaded through `#[path]`, a crate root or a `mod.rs` owns its directory.
//! - Module declarations inside inline modules (`mod a { mod b; }`) are read as if top-level;
//!   no test target of this workspace nests one.
//! - A target is never a caller of its own crate.
//! - [`blank`] keeps every character position and every newline, so offsets found in blanked
//!   text index the original.

use crate::workspace::{Package, TargetId, TargetKind, Workspace};
use crate::GateError;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// What [`blank`] replaces with spaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blanking {
    /// Line and block comments only; string literals stay.
    Comments,
    /// Comments, string literals and character literals.
    CommentsAndLiterals,
}

/// Whether `c` can be part of a Rust identifier.
fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Index just past the string literal whose body starts at `start` (after the opening quote).
fn skip_string(chars: &[char], start: usize) -> usize {
    let mut i = start;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '"' => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// For a raw string starting at `i` (`r"`, `r#"`, `br"`), the number of `#` and the index of its
/// body; `None` if `i` does not start one.
fn raw_string_start(chars: &[char], i: usize) -> Option<(usize, usize)> {
    let mut j = i;
    if chars.get(j) == Some(&'b') {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    (chars.get(j) == Some(&'"')).then_some((hashes, j + 1))
}

/// Index just past the raw string whose body starts at `body`, closed by `"` and `hashes` `#`s.
fn skip_raw_string(chars: &[char], body: usize, hashes: usize) -> usize {
    let mut i = body;
    while i < chars.len() {
        if chars[i] == '"' && (1..=hashes).all(|k| chars.get(i + k) == Some(&'#')) {
            return i + 1 + hashes;
        }
        i += 1;
    }
    chars.len()
}

/// Index just past a character literal starting at `i`, or `None` for a lifetime.
fn char_literal_end(chars: &[char], i: usize) -> Option<usize> {
    if chars.get(i + 1) == Some(&'\\') {
        let mut j = i + 2;
        while j < chars.len() && chars[j] != '\'' {
            j += 1;
        }
        return Some((j + 1).min(chars.len()));
    }
    (chars.get(i + 2) == Some(&'\'')).then_some(i + 3)
}

/// Replaces comments (and, with [`Blanking::CommentsAndLiterals`], literals) by spaces.
///
/// Newlines are kept and every other character maps to one character, so positions in the
/// result are positions in `source`.
#[must_use]
pub fn blank(source: &str, mode: Blanking) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = chars.clone();
    let erase = |from: usize, to: usize, out: &mut Vec<char>| {
        for c in &mut out[from..to.min(chars.len())] {
            if *c != '\n' {
                *c = ' ';
            }
        }
    };
    let literals = mode == Blanking::CommentsAndLiterals;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let end = if c == '/' && next == Some('/') {
            let end = (i..chars.len())
                .find(|&j| chars[j] == '\n')
                .unwrap_or(chars.len());
            erase(i, end, &mut out);
            end
        } else if c == '/' && next == Some('*') {
            let end = block_comment_end(&chars, i);
            erase(i, end, &mut out);
            end
        } else if c == '"' {
            let end = skip_string(&chars, i + 1);
            if literals {
                erase(i, end, &mut out);
            }
            end
        } else if c == '\'' {
            match char_literal_end(&chars, i) {
                Some(end) => {
                    if literals {
                        erase(i, end, &mut out);
                    }
                    end
                }
                None => i + 1,
            }
        } else if is_ident(c) && (i == 0 || !is_ident(chars[i - 1])) {
            match raw_string_start(&chars, i) {
                Some((hashes, body)) => {
                    let end = skip_raw_string(&chars, body, hashes);
                    if literals {
                        erase(i, end, &mut out);
                    }
                    end
                }
                None => i + 1,
            }
        } else {
            i + 1
        };
        i = end.max(i + 1);
    }
    out.into_iter().collect()
}

/// Index just past the (possibly nested) block comment starting at `start`.
fn block_comment_end(chars: &[char], start: usize) -> usize {
    let mut depth = 0usize;
    let mut i = start;
    while i < chars.len() {
        if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            depth += 1;
            i += 2;
        } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    chars.len()
}

/// Positions in `code` (already blanked) where identifier `word` starts a token.
fn token_starts(code: &[char], word: &str) -> Vec<usize> {
    let word: Vec<char> = word.chars().collect();
    (0..code.len().saturating_sub(word.len() - 1))
        .filter(|&i| code[i..].starts_with(&word) && (i == 0 || !is_ident(code[i - 1])))
        .collect()
}

/// Whether `source` calls into the client's GPU test harness: `run_test_child`, `GpuCommand`,
/// or a `virtual_gpu_command*` function, in code (not comments or strings).
#[must_use]
pub fn uses_gpu_harness(source: &str) -> bool {
    let code: Vec<char> = blank(source, Blanking::CommentsAndLiterals)
        .chars()
        .collect();
    ["run_test_child", "GpuCommand", "virtual_gpu_command"]
        .iter()
        .any(|marker| !token_starts(&code, marker).is_empty())
}

/// Whether `source` starts valgrind as a process: `Command::new("valgrind")` in code.
#[must_use]
pub fn invokes_valgrind(source: &str) -> bool {
    let original: Vec<char> = source.chars().collect();
    let code: Vec<char> = blank(source, Blanking::CommentsAndLiterals)
        .chars()
        .collect();
    token_starts(&code, "Command::new(").into_iter().any(|at| {
        let mut i = at + "Command::new(".len();
        while original.get(i).is_some_and(|c| c.is_whitespace()) {
            i += 1;
        }
        original[i.min(original.len())..].starts_with(&"\"valgrind\"".chars().collect::<Vec<_>>())
    })
}

/// Whether code in `source` names a path into crate `krate` (`krate::...`).
#[must_use]
pub fn mentions_path_root(source: &str, krate: &str) -> bool {
    let code: Vec<char> = blank(source, Blanking::CommentsAndLiterals)
        .chars()
        .collect();
    !token_starts(&code, &format!("{krate}::")).is_empty()
}

/// The crates of `names` that code in `source` names a path into (`name::...`).
///
/// Equivalent to [`mentions_path_root`] for each name, blanking `source` once.
#[must_use]
pub fn mentioned_path_roots<'a>(source: &str, names: &[&'a str]) -> BTreeSet<&'a str> {
    let code: Vec<char> = blank(source, Blanking::CommentsAndLiterals)
        .chars()
        .collect();
    names
        .iter()
        .copied()
        .filter(|name| !token_starts(&code, &format!("{name}::")).is_empty())
        .collect()
}

/// One `mod name;` declaration and its `#[path]` attribute, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ModuleDecl {
    /// The module's name.
    name: String,
    /// The value of a `#[path = "..."]` attribute on it.
    path: Option<String>,
}

/// Reads an identifier starting at `i`; returns it and the index after it.
fn read_ident(chars: &[char], i: usize) -> (String, usize) {
    let end = (i..chars.len())
        .find(|&j| !is_ident(chars[j]))
        .unwrap_or(chars.len());
    (chars[i..end].iter().collect(), end)
}

/// Index of the first non-whitespace character at or after `i`.
fn skip_space(chars: &[char], mut i: usize) -> usize {
    while chars.get(i).is_some_and(|c| c.is_whitespace()) {
        i += 1;
    }
    i
}

/// Index just past the bracket group opening at `open` (`[` or `(`), skipping strings.
fn group_end(chars: &[char], open: usize) -> usize {
    let mut depth = 0usize;
    let mut i = open;
    while i < chars.len() {
        match chars[i] {
            '[' | '(' => depth += 1,
            ']' | ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            '"' => {
                i = skip_string(chars, i + 1);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    chars.len()
}

/// The value of `path = "..."` in one attribute's text, if it is a path attribute.
fn path_attribute(attribute: &str) -> Option<String> {
    let inner = attribute.strip_prefix("#[")?.strip_suffix(']')?.trim();
    let value = inner
        .strip_prefix("path")?
        .trim_start()
        .strip_prefix('=')?
        .trim();
    Some(value.strip_prefix('"')?.strip_suffix('"')?.to_string())
}

/// Every file-module declaration (`mod x;`) in `source`, with its `#[path]`.
fn declared_modules(source: &str) -> Vec<ModuleDecl> {
    let chars: Vec<char> = blank(source, Blanking::Comments).chars().collect();
    let mut decls = Vec::new();
    let mut attributes: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '#' && chars.get(i + 1) == Some(&'[') {
            let end = group_end(&chars, i + 1);
            attributes.push(chars[i..end].iter().collect());
            i = end;
        } else if is_ident(c) {
            let (word, after) = read_ident(&chars, i);
            i = after;
            if word == "pub" {
                let next = skip_space(&chars, i);
                if chars.get(next) == Some(&'(') {
                    i = group_end(&chars, next);
                }
                continue;
            }
            if word == "mod" {
                let start = skip_space(&chars, i);
                let (name, after_name) = read_ident(&chars, start);
                let semicolon = skip_space(&chars, after_name);
                if !name.is_empty() && chars.get(semicolon) == Some(&';') {
                    let path = attributes.iter().find_map(|a| path_attribute(a));
                    decls.push(ModuleDecl { name, path });
                    i = semicolon + 1;
                }
            }
            attributes.clear();
        } else {
            if c == '"' {
                i = skip_string(&chars, i + 1);
            } else {
                i += 1;
            }
            attributes.clear();
        }
    }
    decls
}

/// `a/b/../c` as `a/c`; `None` if it climbs above the root.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// The directory part of a relative path (`a/b.rs` -> `a`).
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Joins two relative path parts.
fn join(dir: &str, rest: &str) -> String {
    if dir.is_empty() {
        rest.to_string()
    } else {
        format!("{dir}/{rest}")
    }
}

/// Every file reachable from `start` through module declarations, `start` included.
///
/// `start` is a crate root, so it owns its directory. Declared modules whose file does not exist
/// are skipped (they are inline or generated); `start` itself must exist.
fn module_closure(root: &Path, start: &str) -> Result<BTreeSet<String>, GateError> {
    let mut seen = BTreeSet::new();
    let mut queue = vec![(start.to_string(), true)];
    while let Some((file, owns_directory)) = queue.pop() {
        if !seen.insert(file.clone()) {
            continue;
        }
        let full = root.join(&file);
        let text = std::fs::read_to_string(&full).map_err(|source| GateError::Io {
            path: full.clone(),
            source,
        })?;
        let stem_dir = if owns_directory || file.ends_with("/mod.rs") {
            parent(&file).to_string()
        } else {
            file.trim_end_matches(".rs").to_string()
        };
        for decl in declared_modules(&text) {
            let candidates = match &decl.path {
                Some(path) => vec![(normalize(&join(parent(&file), path)), true)],
                None => vec![
                    (Some(join(&stem_dir, &format!("{}.rs", decl.name))), false),
                    (
                        Some(join(&stem_dir, &format!("{}/mod.rs", decl.name))),
                        true,
                    ),
                ],
            };
            if let Some((found, owns)) = candidates
                .into_iter()
                .filter_map(|(path, owns)| path.map(|p| (p, owns)))
                .find(|(path, _)| root.join(path).is_file())
            {
                queue.push((found, owns));
            }
        }
    }
    Ok(seen)
}

/// Every regular file under `dir` (relative to `root`), recursively, sorted; symbolic links are
/// not followed. A missing directory yields nothing.
fn files_under(root: &Path, dir: &str) -> Result<BTreeSet<String>, GateError> {
    let mut out = BTreeSet::new();
    let mut pending = vec![dir.to_string()];
    while let Some(current) = pending.pop() {
        let full = root.join(&current);
        let Ok(entries) = std::fs::read_dir(&full) else {
            continue;
        };
        for entry in entries {
            let entry = entry.map_err(|source| GateError::Io {
                path: full.clone(),
                source,
            })?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = join(&current, &name);
            let kind = entry.file_type().map_err(|source| GateError::Io {
                path: entry.path(),
                source,
            })?;
            if kind.is_dir() {
                pending.push(relative);
            } else if kind.is_file() {
                out.insert(relative);
            }
        }
    }
    Ok(out)
}

/// The files of every test target, the reverse map, and which targets call which crate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceIndex {
    /// Files compiled into each target, relative to the repository root.
    files: BTreeMap<TargetId, BTreeSet<String>>,
    /// Per workspace crate, the targets of other crates whose Rust files name `<crate>::` in
    /// code (the `@callers` of a downstream contract).
    callers: BTreeMap<String, BTreeSet<TargetId>>,
}

impl SourceIndex {
    /// An index from explicit file lists, for tests and synthetic workspaces; it records no
    /// callers.
    #[must_use]
    pub fn from_files(files: BTreeMap<TargetId, BTreeSet<String>>) -> Self {
        Self {
            files,
            callers: BTreeMap::new(),
        }
    }

    /// The same index with `targets` recorded as the callers of crate `krate`, for synthetic
    /// workspaces.
    #[must_use]
    pub fn with_callers(mut self, krate: &str, targets: BTreeSet<TargetId>) -> Self {
        self.callers.insert(krate.to_string(), targets);
        self
    }

    /// Reads the sources of every test target of `workspace` under `root`, and records for
    /// every workspace crate the targets of other crates whose Rust files name it in code.
    ///
    /// # Errors
    /// [`GateError::Io`] when a source file or directory cannot be read.
    pub fn build(root: &Path, workspace: &Workspace) -> Result<Self, GateError> {
        let mut files = BTreeMap::new();
        for package in workspace.packages.values() {
            for target in &package.targets {
                files.insert(target.id.clone(), target_files(root, package, target)?);
            }
        }
        let callers = callers_by_crate(root, workspace, &files)?;
        Ok(Self { files, callers })
    }

    /// The targets outside crate `krate` whose own Rust files (support files included) name
    /// `krate::` in code, sorted; empty for a crate nothing calls or an unknown name.
    #[must_use]
    pub fn callers_of(&self, krate: &str) -> BTreeSet<TargetId> {
        self.callers.get(krate).cloned().unwrap_or_default()
    }

    /// The files of `target` (empty for an unknown target).
    #[must_use]
    pub fn files_of(&self, target: &TargetId) -> BTreeSet<String> {
        self.files.get(target).cloned().unwrap_or_default()
    }

    /// Whether `path` is compiled into `target`.
    #[must_use]
    pub fn owns(&self, target: &TargetId, path: &str) -> bool {
        self.files
            .get(target)
            .is_some_and(|files| files.contains(path))
    }

    /// The targets `path` is compiled into, sorted.
    #[must_use]
    pub fn targets_of(&self, path: &str) -> Vec<TargetId> {
        self.files
            .iter()
            .filter(|(_, files)| files.contains(path))
            .map(|(target, _)| target.clone())
            .collect()
    }

    /// Every indexed file, sorted.
    #[must_use]
    pub fn all_files(&self) -> BTreeSet<String> {
        self.files.values().flatten().cloned().collect()
    }
}

/// Whether `path` names a Rust source file.
fn is_rust(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "rs")
}

/// For every workspace crate, the targets of other crates whose Rust files name it in code.
///
/// Each file is read and blanked once, however many targets compile it.
fn callers_by_crate(
    root: &Path,
    workspace: &Workspace,
    files: &BTreeMap<TargetId, BTreeSet<String>>,
) -> Result<BTreeMap<String, BTreeSet<TargetId>>, GateError> {
    let names: Vec<&str> = workspace.packages.keys().map(String::as_str).collect();
    let mut mentioned: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut callers: BTreeMap<String, BTreeSet<TargetId>> = BTreeMap::new();
    for (target, target_files) in files {
        for file in target_files.iter().filter(|file| is_rust(file)) {
            if !mentioned.contains_key(file.as_str()) {
                let full = root.join(file);
                let text = std::fs::read_to_string(&full)
                    .map_err(|source| GateError::Io { path: full, source })?;
                mentioned.insert(file.as_str(), mentioned_path_roots(&text, &names));
            }
            for krate in mentioned.get(file.as_str()).into_iter().flatten() {
                if *krate != target.package {
                    callers
                        .entry((*krate).to_string())
                        .or_default()
                        .insert(target.clone());
                }
            }
        }
    }
    Ok(callers)
}

/// The files of one target, by the rules in the module documentation.
fn target_files(
    root: &Path,
    package: &Package,
    target: &crate::workspace::Target,
) -> Result<BTreeSet<String>, GateError> {
    let src = join(&package.dir, "src");
    let bin_dir = join(&src, "bin");
    match &target.id.kind {
        TargetKind::Lib => Ok(files_under(root, &src)?
            .into_iter()
            .filter(|file| {
                *file != join(&src, "main.rs") && !file.starts_with(&format!("{bin_dir}/"))
            })
            .collect()),
        TargetKind::Bin(name) => {
            let mut files = module_closure(root, &target.src_path)?;
            if target.src_path.starts_with(&format!("{bin_dir}/")) {
                files.extend(files_under(root, &join(&bin_dir, name))?);
            }
            Ok(files)
        }
        TargetKind::Test(_) => {
            let dir = parent(&target.src_path);
            if target.src_path.ends_with("/main.rs") && dir != join(&package.dir, "tests") {
                files_under(root, dir)
            } else {
                module_closure(root, &target.src_path)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blanking_keeps_positions_and_newlines() {
        let source =
            "let a = \"x // y\"; // note\n/* b\n */ let c = 'q'; let d: &'static str = r#\"z\"#;";
        let code = blank(source, Blanking::CommentsAndLiterals);
        assert_eq!(code.chars().count(), source.chars().count());
        assert_eq!(code.lines().count(), source.lines().count());
        assert!(!code.contains("note") && !code.contains('x') && !code.contains('q'));
        assert!(code.contains("'static"), "a lifetime is not a literal");
        assert!(!code.contains('z'));
        let comments_only = blank(source, Blanking::Comments);
        assert!(comments_only.contains("\"x // y\""), "a string hides `//`");
        assert!(!comments_only.contains("note"));
    }

    #[test]
    fn gpu_markers_count_in_code_only() {
        assert!(uses_gpu_harness(
            "fn t() { gpu_run::run_test_child(\"a\", \"b\"); }"
        ));
        assert!(uses_gpu_harness(
            "let c = gpu_run::virtual_gpu_command_on_screen(x);"
        ));
        assert!(uses_gpu_harness("use x::GpuCommand;"));
        assert!(!uses_gpu_harness(
            "// run_test_child\nlet s = \"GpuCommand\";"
        ));
        assert!(!uses_gpu_harness("fn my_run_test_child() {}"));
    }

    #[test]
    fn valgrind_counts_only_as_a_started_program() {
        assert!(invokes_valgrind(
            "let o = Command::new( \"valgrind\").arg(x);"
        ));
        assert!(!invokes_valgrind(
            "// Command::new(\"valgrind\")\nlet s = \"valgrind\";"
        ));
        assert!(!invokes_valgrind("let o = Command::new(\"ffmpeg\");"));
    }

    #[test]
    fn crate_paths_are_found_in_code_not_text() {
        assert!(mentions_path_root(
            "let w = sim_fixtures::world(m, 1);",
            "sim_fixtures"
        ));
        assert!(!mentions_path_root(
            "//! uses sim_fixtures::world",
            "sim_fixtures"
        ));
        assert!(!mentions_path_root("my_sim_fixtures::x()", "sim_fixtures"));
    }

    #[test]
    fn module_declarations_carry_their_path_attribute() {
        let source = "#[allow(\n    dead_code,\n    reason = \"shared ]\"\n)]\n#[path = \"support/gpu_run.rs\"]\nmod gpu_run;\npub(crate) mod plain;\nmod inline { }\nfn f() {}\n#[path = \"x.rs\"]\nfn not_a_module() {}\nmod after;";
        let decls = declared_modules(source);
        assert_eq!(
            decls,
            vec![
                ModuleDecl {
                    name: "gpu_run".to_string(),
                    path: Some("support/gpu_run.rs".to_string())
                },
                ModuleDecl {
                    name: "plain".to_string(),
                    path: None
                },
                ModuleDecl {
                    name: "after".to_string(),
                    path: None
                },
            ]
        );
    }

    #[test]
    fn module_closure_follows_rust_file_rules() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(|source| GateError::Io {
            path: "tempdir".into(),
            source,
        })?;
        let root = dir.path();
        let write = |path: &str, text: &str| -> Result<(), GateError> {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).map_err(|source| GateError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            std::fs::write(&full, text).map_err(|source| GateError::Io { path: full, source })
        };
        write(
            "c/tests/a_tests.rs",
            "#[path = \"support/h.rs\"]\nmod h;\nmod common;",
        )?;
        write("c/tests/support/h.rs", "mod inner;")?;
        write("c/tests/support/inner.rs", "")?;
        write("c/tests/common/mod.rs", "mod deep;")?;
        write("c/tests/common/deep.rs", "")?;
        write("c/tests/unused.rs", "")?;
        let files = module_closure(root, "c/tests/a_tests.rs")?;
        let expected: BTreeSet<String> = [
            "c/tests/a_tests.rs",
            "c/tests/common/deep.rs",
            "c/tests/common/mod.rs",
            "c/tests/support/h.rs",
            "c/tests/support/inner.rs",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(files, expected);
        Ok(())
    }

    #[test]
    fn callers_are_found_through_support_files_in_code_only_outside_the_key_crate(
    ) -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(|source| GateError::Io {
            path: "tempdir".into(),
            source,
        })?;
        let root = dir.path();
        let write = |path: &str, text: &str| -> Result<(), GateError> {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).map_err(|source| GateError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            std::fs::write(&full, text).map_err(|source| GateError::Io { path: full, source })
        };
        write(
            "crates/app/tests/via_support.rs",
            "#[path = \"support/fx.rs\"]\nmod fx;",
        )?;
        write(
            "crates/app/tests/support/fx.rs",
            "pub fn w() { fixtures::world(); }",
        )?;
        write(
            "crates/app/tests/in_text.rs",
            "// fixtures::world\nconst S: &str = \"fixtures::world\";",
        )?;
        write("crates/fixtures/tests/own.rs", "fn f() { fixtures::x(); }")?;
        let mut workspace = Workspace::default();
        for (name, tests) in [
            ("app", vec!["via_support", "in_text"]),
            ("fixtures", vec!["own"]),
        ] {
            workspace.packages.insert(
                name.to_string(),
                Package {
                    name: name.to_string(),
                    version: "0.1.0".to_string(),
                    dir: format!("crates/{name}"),
                    dependencies: BTreeMap::new(),
                    targets: tests
                        .into_iter()
                        .map(|test| crate::workspace::Target {
                            id: TargetId::test(name, test),
                            src_path: format!("crates/{name}/tests/{test}.rs"),
                        })
                        .collect(),
                },
            );
        }
        let index = SourceIndex::build(root, &workspace)?;
        assert_eq!(
            index.callers_of("fixtures"),
            BTreeSet::from([TargetId::test("app", "via_support")])
        );
        assert!(index.callers_of("app").is_empty());
        assert!(index.callers_of("unknown").is_empty());
        Ok(())
    }

    #[test]
    fn paths_normalize_and_refuse_to_climb_out() {
        assert_eq!(normalize("a/b/../c.rs"), Some("a/c.rs".to_string()));
        assert_eq!(normalize("../x"), None);
    }
}
