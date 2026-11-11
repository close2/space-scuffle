//! Tiered test gates: select the tests a change can break, run them, and record the result.
//!
//! # Responsibility
//! This crate is the tool behind `cargo gate` (blueprint `docs/plans/2026-11-04-tiered-test-gates.md`).
//! It reads the suite catalog `crates/gate/suites.ron`, the workspace's targets from
//! `cargo metadata`, and the changed paths from Git; it turns them into a [`Selection`] of test
//! targets for one [`Tier`], explains why each target was chosen, builds and runs exactly those
//! targets, and writes a [`Stamp`] keyed by the Git tree that passed.
//!
//! # How it fits
//! The library holds every decision as a pure function over plain data, so the selection rules,
//! the glob matcher, the reverse-dependency closure, the lane scheduler and the stamp format are
//! unit-tested without Git or Cargo. The binary (`src/main.rs`) only wires those functions to the
//! real repository. The crate depends on no other workspace crate (charter §5): a broken
//! simulation or client crate cannot break the tool that tests it.
//!
//! # Invariants
//! - Uncertain means more: a changed path no catalog entry covers selects the complete suite.
//! - Nothing is hidden from `cargo test --workspace`: the `full` tier selects every test target
//!   `cargo metadata` lists, and the catalog test (`tests/suite_catalog_tests.rs`, check C22)
//!   proves it.
//! - Every traversal and every report is ordered (`BTreeMap`, sorted vectors), so two runs on the
//!   same inputs print the same text.

mod catalog;
mod catalog_diff;
mod changes;
mod cli;
mod crash;
mod execute;
mod explain;
mod gate_lock;
mod glob;
mod lockfile;
mod manifest_diff;
mod memory;
mod merge;
mod runner;
mod schedule;
mod select;
mod sources;
mod stamp;
mod suspects;
mod test_log;
mod tier;
mod timings;
mod workspace;

pub use catalog::{
    CommandStep, PathRule, SuiteCatalog, TargetEntry, TargetRef, TestClass, CALLERS, CATALOG_PATH,
};
pub use catalog_diff::{CatalogDiff, RuleChange};
pub use changes::{capture, ChangeSpec, Git};
pub use cli::{
    parse_args, parse_args_with, Command, RunOptions, CEILING_MARGIN_MIB, DEFAULT_JOBS,
    DEFAULT_MEMORY_BUDGET_MIB, JOBS_VAR, USAGE,
};
pub use crash::{
    compiler_crashed, crash_log, incremental_dirs, run_with_crash_retry, StepRun, COMPILER_PANIC,
};
pub use execute::{
    build_invocations, failed_tests, gib, lanes_tsv, mark_flakes, overlapping, run_targets,
    stop_path, stop_request, stop_requested, time_limit, LaneOutcome, LaneRecord, LaneRun, Outcome,
    TargetResult,
};
pub use explain::render_selection;
pub use gate_lock::{
    current_holder, lock_path, stale_group_warning, waiting_message, GateLock, Holder, HELD_VAR,
    LOCK_PATH_VAR,
};
pub use glob::Glob;
pub use lockfile::{LockDiff, LockEntry};
pub use manifest_diff::{classify_manifest, parse_toml, ManifestChange, TomlError, TomlValue};
pub use memory::{
    descendants, group_members, kill_tree, own_process_group, read_process_table, MemorySource,
    ProcessInfo, ProcessTable, Sampler,
};
pub use merge::{changed_crates, choose_stamp, overlapping_crates, BranchCommit};
pub use runner::{check_target_dir, run_tier, stamp_dir, Context, RunReport, StepResult};
pub use schedule::{Job, Lane, MemoryLimits, Scheduler};
pub use select::{
    is_judged_by_content, select, select_change, Change, ClippyScope, CompleteCause, FileVersions,
    Inputs, PathMatch, Reason, Selected, Selection, LOCKFILE_PATH,
};
pub use sources::{
    blank, invokes_valgrind, mentioned_path_roots, mentions_path_root, uses_gpu_harness, Blanking,
    SourceIndex,
};
pub use stamp::{
    accepted_tiers, peak_estimates, Reuse, Stamp, StampTarget, StampTree, ESTIMATE_RUNS,
};
pub use suspects::{
    changes_since, last_green_full, render_suspects, suspect_report, suspects_for, MainCommit,
    HISTORY_LIMIT,
};
pub use test_log::{
    failing_targets, parse_test_log, render_log_timings, resolve_targets, LoggedBinary,
};
pub use tier::Tier;
pub use timings::{
    class_report, repeated_flakes, time_saved, ClassFinding, FlakeFinding, FLAKE_LIMIT,
    FLAKE_WINDOW_S,
};
pub use workspace::{DependencyKind, Package, Target, TargetId, TargetKind, Workspace};

use std::path::PathBuf;

/// Everything that can stop the gate from deciding or running a selection.
///
/// A gate that cannot read its inputs fails with one of these instead of selecting nothing, so an
/// unreadable catalog or repository can never pass as a change that needs no tests.
#[derive(Debug, thiserror::Error)]
pub enum GateError {
    /// A file or directory could not be read or written.
    #[error("cannot access {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying operating-system error.
        #[source]
        source: std::io::Error,
    },
    /// A process could not be started.
    #[error("cannot run `{program}`: {source}")]
    Spawn {
        /// The program that was invoked.
        program: String,
        /// The underlying operating-system error.
        #[source]
        source: std::io::Error,
    },
    /// A helper process (Git, `cargo metadata`) ran but failed.
    #[error("`{command}` failed with {status}: {stderr}")]
    CommandFailed {
        /// The command line, for the report.
        command: String,
        /// The exit status it returned.
        status: std::process::ExitStatus,
        /// Its standard error, which names the cause.
        stderr: String,
    },
    /// Cargo printed JSON this crate could not parse.
    #[error("invalid JSON from Cargo: {0}")]
    Json(#[from] serde_json::Error),
    /// `cargo metadata` output lacks a field the gate relies on.
    #[error("`cargo metadata` output lacks {0}")]
    MetadataShape(&'static str),
    /// The suite catalog is not valid RON for [`SuiteCatalog`].
    #[error("{path}: {source}")]
    CatalogSyntax {
        /// The catalog file.
        path: PathBuf,
        /// The parser's error, with line and column.
        #[source]
        source: ron::error::SpannedError,
    },
    /// The catalog parses but says something impossible (a bad glob, an unknown target).
    #[error("suite catalog: {0}")]
    Catalog(String),
    /// A glob pattern is malformed.
    #[error("glob `{pattern}`: {reason}")]
    Glob {
        /// The pattern as written.
        pattern: String,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// A stamp file is not valid RON for [`Stamp`].
    #[error("stamp {path}: {source}")]
    StampSyntax {
        /// The stamp file.
        path: PathBuf,
        /// The parser's error.
        #[source]
        source: ron::error::SpannedError,
    },
    /// A stamp could not be serialized.
    #[error("cannot serialize a stamp: {0}")]
    StampWrite(#[from] ron::Error),
    /// A stamp was requested for a working tree with uncommitted changes.
    #[error(
        "the working tree has uncommitted changes, so no tree hash names what was tested; \
         commit first ({0})"
    )]
    DirtyTree(String),
    /// A tier was run in an agent worktree without `CARGO_TARGET_DIR` (charter §7).
    #[error(
        "{0} is an agent worktree: set CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree-name> \
         first (charter §7)"
    )]
    TargetDirRequired(PathBuf),
    /// The command line could not be understood.
    #[error("{0}\n\n{USAGE}")]
    Usage(String),
}
