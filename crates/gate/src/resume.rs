//! Per-target results of a tier run on one tree, and `cargo gate <tier> --resume`.
//!
//! # Responsibility
//! Defines [`ResultRecord`], the record of what every step and test target of the latest run of
//! one tier did on one Git tree, its RON file beside the stamps
//! (`<stamp dir>/results/<tree hash>.<tier>.ron`), and [`plan_resume`], which decides what a
//! resumed run may take from it (plan `docs/plans/2026-11-11-faster-gates.md`, Amendment 1;
//! owner, Q0082 option 1).
//!
//! # How it fits
//! The runner keeps one record per run on a clean working tree and rewrites the file after every
//! step and every finished target, so a red, stopped or killed run leaves what it learned. With
//! `--resume`, the runner reads the record of the same tree and tier, asks [`plan_resume`] which
//! targets and steps passed under the same toolchain and catalog, and runs only the rest. The
//! stamp is written, as before, only when every selected target and step has passed on this
//! tree; it names what was reused ([`crate::Stamp::resumed_targets`]).
//!
//! # Invariants
//! - Only a pass is ever reused. A failed, flaky, timed-out, errored or skipped target, and one
//!   with no result, runs again; a flaky target must pass in the resumed run, so "a flake is not
//!   green" (blueprint §3.5) holds across resumes.
//! - A record is reused only for the identical [`ResumeKey`]: the same tier, the same tree hash
//!   of a clean working tree, the same `rustc -V`, and the same content of
//!   `crates/gate/suites.ron`. Anything else is refused with the reason, and the run tests the
//!   whole selection.
//! - Record files live in a subdirectory, so [`crate::Stamp::read_all`] never mistakes them for
//!   stamps.

use crate::catalog::{TestClass, CATALOG_PATH};
use crate::changes::{capture, Git};
use crate::execute::{Outcome, TargetResult};
use crate::stamp::StampTree;
use crate::tier::Tier;
use crate::GateError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The subdirectory of the stamp directory that holds the result records.
pub const RESULTS_DIR: &str = "results";

/// The environment variables that change what a test run proves, and so belong to the key:
/// `RUSTFLAGS` changes every compiled binary, and `SPACE_SCUFFLE_GPU_TEST_DRIVER` the driver a
/// GPU test renders on (`radv` or `lavapipe`, charter §7).
pub const KEY_ENVIRONMENT: [&str; 2] = ["RUSTFLAGS", "SPACE_SCUFFLE_GPU_TEST_DRIVER"];

/// The [`KEY_ENVIRONMENT`] variables as `lookup` reports them, one `NAME=value` per line, a
/// variable that is unset as `NAME` alone, so an unset and an empty variable differ.
#[must_use]
pub fn environment_of(lookup: impl Fn(&str) -> Option<String>) -> String {
    KEY_ENVIRONMENT
        .iter()
        .map(|name| {
            lookup(name).map_or_else(|| (*name).to_string(), |value| format!("{name}={value}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What must match for a recorded result to be reused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumeKey {
    /// The tier of the run.
    pub tier: Tier,
    /// The tree hash of the clean working tree (`git rev-parse HEAD^{tree}`).
    pub tree: String,
    /// The `HEAD` commit, for the report only: two commits of one tree share results.
    pub commit: String,
    /// `rustc -V` in the repository root, which follows `rust-toolchain.toml`.
    pub toolchain: String,
    /// The Git blob hash of `crates/gate/suites.ron` (`git hash-object`), whose thread caps,
    /// classes and time limits decide how a target runs.
    pub catalog: String,
    /// The [`KEY_ENVIRONMENT`] variables of the run ([`environment_of`]).
    pub environment: String,
}

impl ResumeKey {
    /// The key of the clean working tree of `git` for `tier`.
    ///
    /// # Errors
    /// [`GateError::DirtyTree`] when the working tree has a change, since its tree hash would not
    /// name what runs; process errors from Git or `rustc`.
    pub fn of_clean_head(git: &Git, tier: Tier) -> Result<Self, GateError> {
        let head = StampTree::of_clean_head(git)?;
        Ok(Self {
            tier,
            tree: head.tree,
            commit: head.commit,
            toolchain: toolchain(git.root())?,
            catalog: catalog_hash(git.root())?,
            environment: environment_of(|name| std::env::var(name).ok()),
        })
    }
}

/// The compiler's version line (`rustc 1.100.0 (… 2026-…)`), run in `root` so that
/// `rust-toolchain.toml` applies; `$RUSTC` replaces `rustc` when set, as it does for Cargo.
///
/// # Errors
/// [`GateError::Spawn`] or [`GateError::CommandFailed`] when the compiler cannot be asked.
pub fn toolchain(root: &Path) -> Result<String, GateError> {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    Ok(capture(&rustc, &["-V"], root)?.trim().to_string())
}

/// The Git blob hash of the suite catalog's content under `root`.
///
/// # Errors
/// Git errors, including a missing catalog.
pub fn catalog_hash(root: &Path) -> Result<String, GateError> {
    Ok(capture("git", &["hash-object", CATALOG_PATH], root)?
        .trim()
        .to_string())
}

/// How a target ended, as a record keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordedOutcome {
    /// Exit status zero in the lane run: the only outcome a resumed run reuses.
    Passed,
    /// A non-zero exit, confirmed by its rerun.
    Failed,
    /// Failed, then passed on its immediate rerun: not green (blueprint §3.5).
    Flaky,
    /// Killed at its time limit.
    TimedOut,
    /// Not built or not startable.
    Error,
    /// Not run, because a stop request ended the run.
    Skipped,
}

impl From<&Outcome> for RecordedOutcome {
    fn from(outcome: &Outcome) -> Self {
        match outcome {
            Outcome::Passed => Self::Passed,
            Outcome::Failed { .. } => Self::Failed,
            Outcome::Flaky { .. } => Self::Flaky,
            Outcome::TimedOut => Self::TimedOut,
            Outcome::Error(_) => Self::Error,
            Outcome::Skipped => Self::Skipped,
        }
    }
}

/// One target's latest result on the record's tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedTarget {
    /// How it ended.
    pub outcome: RecordedOutcome,
    /// Its class at the time of the run.
    pub class: TestClass,
    /// Wall time in milliseconds.
    pub wall_ms: u64,
    /// CPU time of it and its children in milliseconds, where the host reported it.
    pub cpu_ms: Option<u64>,
    /// The peak anonymous memory of its process tree in MiB, where the sampler saw it.
    pub peak_anon_mib: Option<u64>,
}

impl RecordedTarget {
    /// The record of `result`.
    #[must_use]
    pub fn of(result: &TargetResult) -> Self {
        let millis = |d: std::time::Duration| u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
        Self {
            outcome: RecordedOutcome::from(&result.outcome),
            class: result.class,
            wall_ms: millis(result.wall),
            cpu_ms: result.cpu.map(millis),
            peak_anon_mib: result.lane.as_ref().and_then(|lane| lane.peak_anon_mib),
        }
    }
}

/// One fixed step's latest result (fmt, clippy, build, doc tests, a catalog command).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedStep {
    /// The command line it ran (`cargo clippy --all-targets --locked -p a -- -D warnings`): a
    /// step is reused only for the same command, since the selection decides clippy's scope and
    /// the doc tests' crates.
    pub command: String,
    /// Whether it passed.
    pub passed: bool,
    /// Whether it passed or failed only after the compiler-crash retry (blueprint Amendment 1).
    pub retried: bool,
}

/// The latest result of every step and target one tier ran on one tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultRecord {
    /// The tier.
    pub tier: Tier,
    /// The tree hash the results belong to.
    pub tree: String,
    /// The commit last checked out for this tree.
    pub commit: String,
    /// `rustc -V` of the runs.
    pub toolchain: String,
    /// The Git blob hash of the suite catalog of the runs.
    pub catalog: String,
    /// The [`KEY_ENVIRONMENT`] variables of the runs. Records written before this field existed
    /// read as empty, which no run's key matches.
    #[serde(default)]
    pub environment: String,
    /// When the record was last written, in seconds since the Unix epoch.
    pub updated_unix_s: u64,
    /// Steps by name.
    pub steps: BTreeMap<String, RecordedStep>,
    /// Targets by catalog spelling (`client/gpu_validation_tests`).
    pub targets: BTreeMap<String, RecordedTarget>,
}

impl ResultRecord {
    /// An empty record for `key`.
    #[must_use]
    pub fn new(key: &ResumeKey) -> Self {
        Self {
            tier: key.tier,
            tree: key.tree.clone(),
            commit: key.commit.clone(),
            toolchain: key.toolchain.clone(),
            catalog: key.catalog.clone(),
            environment: key.environment.clone(),
            updated_unix_s: 0,
            steps: BTreeMap::new(),
            targets: BTreeMap::new(),
        }
    }

    /// Why this record cannot be reused for `key`, or `None` when it can.
    #[must_use]
    pub fn refusal(&self, key: &ResumeKey) -> Option<String> {
        if self.tier != key.tier {
            return Some(format!(
                "the recorded results are of the {} tier, not {}",
                self.tier, key.tier
            ));
        }
        if self.tree != key.tree {
            return Some(format!(
                "the recorded results are of tree {}, not {}",
                self.tree, key.tree
            ));
        }
        if self.toolchain != key.toolchain {
            return Some(format!(
                "the recorded results were built by `{}`, not `{}`",
                self.toolchain, key.toolchain
            ));
        }
        if self.catalog != key.catalog {
            return Some(format!(
                "{CATALOG_PATH} changed since the recorded results (blob {}, now {})",
                self.catalog, key.catalog
            ));
        }
        if self.environment != key.environment {
            return Some(format!(
                "the recorded results ran with another environment ({}), not ({})",
                self.environment.replace('\n', ", "),
                key.environment.replace('\n', ", ")
            ));
        }
        None
    }

    /// Records the latest result of step `name`, which ran `command`.
    pub fn record_step(&mut self, name: &str, command: &str, passed: bool, retried: bool) {
        let step = RecordedStep {
            command: command.to_string(),
            passed,
            retried,
        };
        self.steps.insert(name.to_string(), step);
    }

    /// Records `result` as its target's latest result.
    pub fn record_target(&mut self, result: &TargetResult) {
        self.targets
            .insert(result.target.to_string(), RecordedTarget::of(result));
    }

    /// The file of the record of `tree` and `tier` under the stamp directory `dir`.
    #[must_use]
    pub fn path(dir: &Path, tree: &str, tier: Tier) -> PathBuf {
        dir.join(RESULTS_DIR)
            .join(format!("{tree}.{}.ron", tier.name()))
    }

    /// The record of `tree` and `tier` under the stamp directory `dir`, if there is one.
    ///
    /// # Errors
    /// [`GateError::Io`] for an unreadable file; [`GateError::StampSyntax`] for a malformed one.
    pub fn read(dir: &Path, tree: &str, tier: Tier) -> Result<Option<Self>, GateError> {
        let path = Self::path(dir, tree, tier);
        match std::fs::read_to_string(&path) {
            Ok(text) => ron::from_str(&text)
                .map(Some)
                .map_err(|source| GateError::StampSyntax { path, source }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(GateError::Io { path, source }),
        }
    }

    /// Writes the record under the stamp directory `dir`, stamped with `now_unix_s`, through a
    /// temporary file and a rename, so a reader never sees half a record.
    ///
    /// # Errors
    /// [`GateError::Io`] if the directory or file cannot be written; [`GateError::StampWrite`]
    /// if serialization fails.
    pub fn write(&mut self, dir: &Path, now_unix_s: u64) -> Result<PathBuf, GateError> {
        self.updated_unix_s = now_unix_s;
        let path = Self::path(dir, &self.tree, self.tier);
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| GateError::Io { path, source }
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io(parent))?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())?;
        let partial = path.with_extension("ron.partial");
        std::fs::write(&partial, text).map_err(io(&partial))?;
        std::fs::rename(&partial, &path).map_err(io(&path))?;
        Ok(path)
    }
}

/// What a run took from recorded results instead of running it: shown in its report and named
/// in its stamp.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resumed {
    /// Targets that passed on this tree earlier, by catalog spelling.
    pub targets: BTreeMap<String, RecordedTarget>,
    /// Steps that passed on this tree earlier, in run order.
    pub steps: Vec<String>,
}

/// What a resumed run takes from the recorded results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumePlan {
    /// The record this run continues: the recorded one when it was accepted, else a new one.
    pub record: ResultRecord,
    /// Selected targets that passed on this tree and are not run again.
    pub reused_targets: BTreeMap<String, RecordedTarget>,
    /// Steps that passed on this tree; each is skipped only when it would run the same command
    /// ([`ResumePlan::reuses_step`]).
    pub reused_steps: BTreeMap<String, RecordedStep>,
    /// Why nothing was reused, when a resume was asked for and refused.
    pub refused: Option<String>,
}

impl ResumePlan {
    /// A plan that reuses nothing and starts a new record for `key`.
    #[must_use]
    pub fn fresh(key: &ResumeKey) -> Self {
        Self {
            record: ResultRecord::new(key),
            reused_targets: BTreeMap::new(),
            reused_steps: BTreeMap::new(),
            refused: None,
        }
    }

    /// Whether the step `name`, about to run `command`, passed with the same command on this
    /// tree and is skipped.
    #[must_use]
    pub fn reuses_step(&self, name: &str, command: &str) -> bool {
        self.reused_steps
            .get(name)
            .is_some_and(|step| step.command == command)
    }

    /// Whether `target` passed on this tree and is skipped.
    #[must_use]
    pub fn reuses_target(&self, target: &str) -> bool {
        self.reused_targets.contains_key(target)
    }

    /// The lines that say what this run reuses, or why it reuses nothing.
    #[must_use]
    pub fn message(&self) -> String {
        if let Some(why) = &self.refused {
            return format!("resume: nothing reused, every selected target runs: {why}");
        }
        let mut out = format!(
            "resume: tree {}, {}: reusing {} passed target(s), and {} passed step(s) where its \
             command is unchanged; every other selected target runs",
            self.record.tree,
            self.record.toolchain,
            self.reused_targets.len(),
            self.reused_steps.len()
        );
        for name in self.reused_steps.keys() {
            let _ = write!(out, "\n  REUSED step {name}");
        }
        for name in self.reused_targets.keys() {
            let _ = write!(out, "\n  REUSED {name}");
        }
        out
    }
}

/// What a run on `key` resumes from `recorded`, for the targets in `selected`.
///
/// With no record, or a record [`ResultRecord::refusal`] refuses, nothing is reused and the run
/// starts a new record. Otherwise every selected target whose recorded outcome is
/// [`RecordedOutcome::Passed`] is reused, and every passed step is offered for reuse with its
/// command; the record is kept, so the resumed run's results join the earlier ones.
#[must_use]
pub fn plan_resume(
    recorded: Option<ResultRecord>,
    key: &ResumeKey,
    selected: &BTreeSet<String>,
) -> ResumePlan {
    let Some(record) = recorded else {
        return ResumePlan {
            refused: Some(format!(
                "no recorded results of the {} tier for tree {}",
                key.tier, key.tree
            )),
            ..ResumePlan::fresh(key)
        };
    };
    if let Some(why) = record.refusal(key) {
        return ResumePlan {
            refused: Some(why),
            ..ResumePlan::fresh(key)
        };
    }
    let reused_targets = record
        .targets
        .iter()
        .filter(|(name, target)| {
            selected.contains(*name) && target.outcome == RecordedOutcome::Passed
        })
        .map(|(name, target)| (name.clone(), target.clone()))
        .collect();
    let reused_steps = record
        .steps
        .iter()
        .filter(|(_, step)| step.passed)
        .map(|(name, step)| (name.clone(), step.clone()))
        .collect();
    let mut record = record;
    record.commit.clone_from(&key.commit);
    ResumePlan {
        record,
        reused_targets,
        reused_steps,
        refused: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::TargetId;
    use std::time::Duration;

    /// A change to a key that must make a record unusable.
    type Change = fn(&mut ResumeKey);

    fn key() -> ResumeKey {
        ResumeKey {
            tier: Tier::Package,
            tree: "tree1".into(),
            commit: "c1".into(),
            toolchain: "rustc 1.100.0".into(),
            catalog: "cat1".into(),
            environment: environment_of(|_| None),
        }
    }

    fn result(name: &str, outcome: Outcome) -> TargetResult {
        TargetResult {
            target: TargetId::test("client", name),
            class: TestClass::Gpu,
            wall: Duration::from_millis(1_500),
            cpu: Some(Duration::from_millis(900)),
            outcome,
            log: PathBuf::from(format!("{name}.log")),
            lane: None,
        }
    }

    fn record_with_every_outcome() -> ResultRecord {
        let mut record = ResultRecord::new(&key());
        record.record_step("fmt", "cargo fmt", true, false);
        record.record_step("clippy", "cargo clippy -p a", true, true);
        record.record_step("doc-tests", "cargo test --doc", false, false);
        for (name, outcome) in [
            ("passed", Outcome::Passed),
            ("failed", Outcome::Failed { tests: vec![] }),
            ("flaky", Outcome::Flaky { tests: vec![] }),
            ("timed_out", Outcome::TimedOut),
            ("error", Outcome::Error("not built".into())),
            ("skipped", Outcome::Skipped),
        ] {
            record.record_target(&result(name, outcome));
        }
        record
    }

    fn selected(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| format!("client/{name}")).collect()
    }

    #[test]
    fn only_passed_targets_and_steps_are_reused() {
        let all = [
            "passed",
            "failed",
            "flaky",
            "timed_out",
            "error",
            "skipped",
            "never_ran",
        ];
        let plan = plan_resume(Some(record_with_every_outcome()), &key(), &selected(&all));
        assert_eq!(plan.refused, None);
        assert_eq!(
            plan.reused_targets.keys().collect::<Vec<_>>(),
            ["client/passed"]
        );
        assert!(plan.reuses_target("client/passed"));
        for name in [
            "flaky",
            "failed",
            "timed_out",
            "error",
            "skipped",
            "never_ran",
        ] {
            assert!(
                !plan.reuses_target(&format!("client/{name}")),
                "{name} must run again"
            );
        }
        assert!(
            plan.reuses_step("fmt", "cargo fmt") && plan.reuses_step("clippy", "cargo clippy -p a")
        );
        assert!(
            !plan.reuses_step("clippy", "cargo clippy -p a -p b"),
            "a step with another command (a wider clippy scope) runs again"
        );
        assert!(
            !plan.reuses_step("doc-tests", "cargo test --doc"),
            "a failed step runs again"
        );
        assert!(!plan.reuses_step("build", ""), "a step never run runs");
        let message = plan.message();
        assert!(message.contains("REUSED client/passed"), "{message}");
        assert!(message.contains("REUSED step fmt"), "{message}");
        assert!(!message.contains("client/flaky"), "{message}");
    }

    #[test]
    fn a_passed_target_outside_the_selection_is_not_reused() {
        let plan = plan_resume(
            Some(record_with_every_outcome()),
            &key(),
            &selected(&["failed"]),
        );
        assert!(plan.reused_targets.is_empty());
        assert_eq!(
            plan.record.targets.len(),
            6,
            "the record keeps every result"
        );
    }

    #[test]
    fn another_tree_toolchain_catalog_or_tier_is_refused() {
        let recorded = record_with_every_outcome();
        let cases: [(&str, Change); 6] = [
            ("tree", |k| k.tree = "tree2".into()),
            ("RUSTFLAGS=-Cdebuginfo=0", |k| {
                k.environment = environment_of(|name| {
                    (name == "RUSTFLAGS").then(|| "-Cdebuginfo=0".to_string())
                });
            }),
            ("SPACE_SCUFFLE_GPU_TEST_DRIVER=radv", |k| {
                k.environment = environment_of(|name| {
                    (name == "SPACE_SCUFFLE_GPU_TEST_DRIVER").then(|| "radv".to_string())
                });
            }),
            ("rustc 1.101.0", |k| k.toolchain = "rustc 1.101.0".into()),
            ("suites.ron", |k| k.catalog = "cat2".into()),
            ("full", |k| k.tier = Tier::Full),
        ];
        for (needle, change) in cases {
            let mut other = key();
            change(&mut other);
            let plan = plan_resume(Some(recorded.clone()), &other, &selected(&["passed"]));
            let why = plan.refused.clone().unwrap_or_default();
            assert!(why.contains(needle), "{needle}: {why}");
            assert!(plan.reused_targets.is_empty() && plan.reused_steps.is_empty());
            assert_eq!(
                plan.record,
                ResultRecord::new(&other),
                "a new record starts"
            );
            assert!(plan.message().starts_with("resume: nothing reused"));
        }
        let none = plan_resume(None, &key(), &selected(&["passed"]));
        assert!(none
            .refused
            .is_some_and(|why| why.contains("no recorded results")));
    }

    #[test]
    fn a_record_round_trips_and_lives_outside_the_stamps() -> Result<(), GateError> {
        let dir = tempfile::tempdir().map_err(|source| GateError::Io {
            path: PathBuf::from("tempdir"),
            source,
        })?;
        let mut record = record_with_every_outcome();
        let path = record.write(dir.path(), 42)?;
        assert_eq!(
            path,
            dir.path().join("results").join("tree1.package.ron"),
            "records live in the results subdirectory"
        );
        assert_eq!(record.updated_unix_s, 42);
        assert_eq!(
            ResultRecord::read(dir.path(), "tree1", Tier::Package)?,
            Some(record)
        );
        assert_eq!(ResultRecord::read(dir.path(), "tree1", Tier::Full)?, None);
        assert_eq!(
            crate::Stamp::read_all(dir.path())?,
            Vec::new(),
            "a record is never read as a stamp"
        );
        std::fs::write(&path, "ResultRecord(").map_err(|source| GateError::Io {
            path: path.clone(),
            source,
        })?;
        assert!(matches!(
            ResultRecord::read(dir.path(), "tree1", Tier::Package),
            Err(GateError::StampSyntax { .. })
        ));
        Ok(())
    }

    #[test]
    fn a_later_result_replaces_an_earlier_one() {
        let mut record = ResultRecord::new(&key());
        record.record_target(&result("x", Outcome::Flaky { tests: vec![] }));
        record.record_target(&result("x", Outcome::Passed));
        record.record_step("fmt", "cargo fmt", false, false);
        record.record_step("fmt", "cargo fmt", true, false);
        let plan = plan_resume(Some(record), &key(), &selected(&["x"]));
        assert!(plan.reuses_target("client/x") && plan.reuses_step("fmt", "cargo fmt"));
        assert_eq!(plan.reused_targets["client/x"].wall_ms, 1_500);
        assert_eq!(plan.reused_targets["client/x"].cpu_ms, Some(900));
    }
}
