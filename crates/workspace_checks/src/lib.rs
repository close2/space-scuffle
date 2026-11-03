//! Executable checks of the engineering charter (`AGENTS.md`).
//!
//! # Responsibility
//! This crate turns statements the charter makes about the code into checks that fail the
//! build. It owns the shared machinery (finding the repository, listing Rust sources, blanking
//! comments and literals, reading `cargo metadata`) and one function per check. The checks
//! themselves run as `crates/workspace_checks/tests/charter.rs`, one `#[test]` per check ID.
//!
//! # How it fits
//! The crate is a development tool (`publish = false`) and depends on no other workspace crate,
//! so it reads the simulation and client crates as text and the dependency graph through Cargo.
//! `cargo test --workspace`, the gate every agent runs and CI's `check-and-test` job runs, executes
//! its tests; no extra CI step is needed. Check IDs (`C2`, `C3`, ...) are the ones used in the
//! Phase 9 blueprint (`docs/plans/2026-11-03-phase-9-structure-and-docs.md`) and in the charter.
//!
//! # Invariants
//! - Every check collects **all** violations and reports each as `path:line: message`, with
//!   `path` relative to the repository root and written with `/` separators on every host.
//! - Every check is a pure function of the files under the root it is given, so each one is
//!   tested against planted violations in a temporary tree (positive controls) as well as run on
//!   the real repository.
//! - Traversal is deterministic (sorted by path) and never follows symbolic links; the client's
//!   `assets` link would otherwise pull the whole media tree into every scan.

mod checks;
mod metadata;
mod source;

pub use checks::{float_ratchet, FLOAT_RATCHET_MESSAGE};
pub use checks::{
    module_docs, no_mutable_statics, no_work_markers, simulation_clippy_config,
    typed_library_errors, FloatAllowance, LIBRARY_CRATES, SIMULATION_CRATES,
};
pub use metadata::{cargo_metadata, engine_free_simulation, FORBIDDEN_SIMULATION_DEPENDENCIES};
pub use source::{blank_comments_and_literals, code_text, rust_files, strip_test_items, Scope};

use std::fmt;
use std::path::{Path, PathBuf};

/// Errors that stop a check from reading what it has to judge.
///
/// A check that cannot read its input fails with one of these instead of reporting no findings,
/// so an unreadable tree can never pass as a clean one.
#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    /// A file or directory could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The path that failed, as given to the filesystem.
        path: PathBuf,
        /// The underlying operating-system error.
        #[source]
        source: std::io::Error,
    },
    /// The `cargo metadata` process could not be started.
    #[error("cannot run `{program} metadata`: {source}")]
    MetadataSpawn {
        /// The Cargo executable that was invoked.
        program: String,
        /// The underlying operating-system error.
        #[source]
        source: std::io::Error,
    },
    /// `cargo metadata` ran but exited unsuccessfully.
    #[error("`cargo metadata` failed with {status}: {stderr}")]
    MetadataFailed {
        /// The exit status Cargo reported.
        status: std::process::ExitStatus,
        /// Cargo's standard error, which names the cause.
        stderr: String,
    },
    /// `cargo metadata` printed something that is not JSON.
    #[error("`cargo metadata` printed invalid JSON: {0}")]
    MetadataJson(#[from] serde_json::Error),
    /// The metadata JSON lacks a field this crate relies on.
    #[error("`cargo metadata` output lacks {0}")]
    MetadataShape(&'static str),
}

/// One violation of a charter rule, located in the repository.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    /// File the violation is in, relative to the repository root.
    pub path: PathBuf,
    /// One-based line number of the violation; `1` when the whole file is at fault.
    pub line: usize,
    /// What is wrong and which charter rule it breaks.
    pub message: String,
}

impl Finding {
    /// Builds a finding for `path` (absolute or relative) under `root`.
    ///
    /// `path` is stored relative to `root` when it lies inside it, so reports read the same on
    /// every machine and every checkout.
    #[must_use]
    pub fn new(root: &Path, path: &Path, line: usize, message: impl Into<String>) -> Self {
        let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        Self {
            path: relative,
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for Finding {
    /// Formats as `path:line: message`, with `/` separators on every host.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}",
            slash_path(&self.path),
            self.line,
            self.message
        )
    }
}

/// Renders `path` with `/` separators, whatever the host uses.
#[must_use]
pub fn slash_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Renders every finding on its own line, in path and line order, for a failing test.
#[must_use]
pub fn render(findings: &[Finding]) -> String {
    let mut sorted = findings.to_vec();
    sorted.sort();
    sorted
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Repository root: two directories above this crate's manifest (`crates/workspace_checks`).
///
/// Derived from `CARGO_MANIFEST_DIR` at compile time, so the checks always judge the checkout
/// they were built from, wherever the test binary runs.
#[must_use]
pub fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .map_or_else(|| manifest.to_path_buf(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_display_is_path_line_message_with_slashes() {
        let root = Path::new("/repo");
        let finding = Finding::new(root, &root.join("crates").join("a.rs"), 7, "bad");
        assert_eq!(finding.to_string(), "crates/a.rs:7: bad");
    }

    #[test]
    fn render_sorts_by_path_then_line() {
        let root = Path::new("/r");
        let findings = vec![
            Finding::new(root, Path::new("/r/b.rs"), 1, "x"),
            Finding::new(root, Path::new("/r/a.rs"), 9, "y"),
            Finding::new(root, Path::new("/r/a.rs"), 2, "z"),
        ];
        assert_eq!(render(&findings), "a.rs:2: z\na.rs:9: y\nb.rs:1: x");
    }

    #[test]
    fn repo_root_holds_the_workspace_manifest_and_charter() {
        let root = repo_root();
        assert!(root.join("Cargo.toml").is_file());
        assert!(root.join("AGENTS.md").is_file());
    }
}
