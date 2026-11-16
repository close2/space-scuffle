//! `cargo gate <tier> --resume` through the public API, on a real Git repository.
//!
//! # Responsibility
//! Proves what `src/resume.rs` promises (plan `docs/plans/2026-11-11-faster-gates.md`,
//! Amendment 1; owner, Q0082 option 1) with real trees, catalogs and record files:
//! - a record written for a clean tree is found again for that tree and tier, and its passed
//!   targets and steps are reused, while failed, flaky, timed-out and unrun targets run again;
//! - another tree, another `rustc -V`, a changed `crates/gate/suites.ron` and a dirty working
//!   tree are refused;
//! - a flaky target is never reused, so it must pass in the resumed run, and only when every
//!   selected target has a pass on the tree does the merged result contain no target to run.
//!
//! # How it fits
//! The repository is a temporary one with a stand-in catalog file at the catalog's path, so the
//! key's tree and catalog hash are the real Git values; the toolchain is the real `rustc -V`.
//! The process-level part (a lane run's results feeding the record) lives in
//! `lane_run_tests.rs`, which owns the fake test executables.
//!
//! # Invariants
//! - Nothing here touches the user's `~/scratch`: every record lives in a temporary directory.

#![allow(clippy::expect_used, clippy::panic)]

use gate::{
    capture, plan_resume, toolchain, GateError, Git, Outcome, RecordedOutcome, ResultRecord,
    ResumeKey, Stamp, TargetId, TargetResult, TestClass, Tier, CATALOG_PATH, HELD_VAR,
    LOCK_PATH_VAR,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A temporary repository with one commit holding a stand-in catalog.
struct Repo {
    dir: tempfile::TempDir,
    git: Git,
}

impl Repo {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("invariant: a temporary directory can be made");
        let root = dir.path();
        capture("git", &["init", "--quiet"], root).expect("invariant: git is installed");
        let repo = Self {
            git: Git::discover(root).expect("invariant: the repository was just made"),
            dir,
        };
        repo.commit_catalog("SuiteCatalog(one)");
        repo
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn commit_catalog(&self, text: &str) {
        let path = self.root().join(CATALOG_PATH);
        std::fs::create_dir_all(
            path.parent()
                .expect("invariant: the catalog is in a directory"),
        )
        .expect("invariant: writable");
        std::fs::write(&path, text).expect("invariant: writable");
        self.commit("catalog");
    }

    fn commit(&self, message: &str) {
        let git = |args: &[&str]| capture("git", args, self.root()).expect("invariant: git works");
        git(&["add", "--all"]);
        git(&[
            "-c",
            "user.name=gate",
            "-c",
            "user.email=gate@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            message,
        ]);
    }

    fn key(&self) -> ResumeKey {
        ResumeKey::of_clean_head(&self.git, Tier::Package).expect("invariant: the tree is clean")
    }
}

fn result(name: &str, outcome: Outcome) -> TargetResult {
    TargetResult {
        target: TargetId::test("client", name),
        class: TestClass::Gpu,
        wall: Duration::from_secs(100),
        cpu: None,
        outcome,
        log: PathBuf::from(format!("{name}.log")),
        lane: None,
    }
}

fn names(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|name| format!("client/{name}")).collect()
}

/// The first run: fmt and clippy pass, `steady` passes, `wobbly` is flaky, `broken` fails, and
/// `unrun` never starts (the gate was killed).
fn first_run(key: &ResumeKey, stamps: &Path) -> Result<(), GateError> {
    let mut record = ResultRecord::new(key);
    record.record_step("fmt", "cargo fmt", true, false);
    record.record_step("clippy", "cargo clippy", true, false);
    record.record_step("build", "", true, false);
    record.record_target(&result("steady", Outcome::Passed));
    record.record_target(&result("wobbly", Outcome::Flaky { tests: vec![] }));
    record.record_target(&result("broken", Outcome::Failed { tests: vec![] }));
    record.write(stamps, 1)?;
    Ok(())
}

const SELECTED: [&str; 4] = ["steady", "wobbly", "broken", "unrun"];

#[test]
fn the_key_names_the_clean_tree_the_toolchain_and_the_catalog() -> Result<(), GateError> {
    let repo = Repo::new();
    let key = repo.key();
    assert_eq!(key.tree, repo.git.tree("HEAD")?);
    assert_eq!(key.commit, repo.git.commit("HEAD")?);
    assert!(key.toolchain.starts_with("rustc "), "{}", key.toolchain);
    assert_eq!(key.toolchain, toolchain(repo.root())?);
    let blob = capture(
        "git",
        &["rev-parse", &format!("HEAD:{CATALOG_PATH}")],
        repo.root(),
    )?;
    assert_eq!(key.catalog, blob.trim(), "the catalog's Git blob hash");
    Ok(())
}

#[test]
fn a_resumed_run_reuses_passes_of_the_same_tree_and_reruns_the_rest() -> Result<(), GateError> {
    let repo = Repo::new();
    let stamps = tempfile::tempdir().map_err(|source| GateError::Io {
        path: PathBuf::from("tempdir"),
        source,
    })?;
    let key = repo.key();
    first_run(&key, stamps.path())?;
    // A second commit with the same files has the same tree, so it resumes too.
    repo.commit("empty");
    let again = repo.key();
    assert_eq!(again.tree, key.tree);
    let recorded = ResultRecord::read(stamps.path(), &again.tree, Tier::Package)?;
    let plan = plan_resume(recorded, &again, &names(&SELECTED));
    assert_eq!(plan.refused, None);
    assert_eq!(
        plan.reused_targets.keys().cloned().collect::<BTreeSet<_>>(),
        names(&["steady"])
    );
    for name in ["wobbly", "broken", "unrun"] {
        assert!(
            !plan.reuses_target(&format!("client/{name}")),
            "{name} must run again"
        );
    }
    assert!(plan.reuses_step("fmt", "cargo fmt") && plan.reuses_step("clippy", "cargo clippy"));
    assert!(plan.message().contains("REUSED client/steady"));
    assert_eq!(plan.record.commit, again.commit, "the record follows HEAD");
    Ok(())
}

#[test]
fn a_flaky_target_must_pass_in_the_resumed_run_before_nothing_is_left_to_run(
) -> Result<(), GateError> {
    let repo = Repo::new();
    let stamps = tempfile::tempdir().map_err(|source| GateError::Io {
        path: PathBuf::from("tempdir"),
        source,
    })?;
    let key = repo.key();
    first_run(&key, stamps.path())?;
    let selected = names(&SELECTED);
    let to_run = |plan: &gate::ResumePlan| -> BTreeSet<String> {
        selected
            .iter()
            .filter(|name| !plan.reuses_target(name))
            .cloned()
            .collect()
    };

    // The resumed run: the flaky target flakes again, the others pass.
    let recorded = ResultRecord::read(stamps.path(), &key.tree, Tier::Package)?;
    let mut plan = plan_resume(recorded, &key, &selected);
    plan.record
        .record_target(&result("wobbly", Outcome::Flaky { tests: vec![] }));
    plan.record
        .record_target(&result("broken", Outcome::Passed));
    plan.record.record_target(&result("unrun", Outcome::Passed));
    plan.record.write(stamps.path(), 2)?;
    let recorded = ResultRecord::read(stamps.path(), &key.tree, Tier::Package)?;
    let plan = plan_resume(recorded, &key, &selected);
    assert_eq!(
        to_run(&plan),
        names(&["wobbly"]),
        "a flake is not green: it runs again"
    );

    // The next resume: the flaky target passes, so every selected target has a pass.
    let mut plan = plan;
    plan.record
        .record_target(&result("wobbly", Outcome::Passed));
    plan.record.write(stamps.path(), 3)?;
    let recorded = ResultRecord::read(stamps.path(), &key.tree, Tier::Package)?;
    let plan = plan_resume(recorded, &key, &selected);
    assert!(to_run(&plan).is_empty(), "every target passed on this tree");
    assert!(plan
        .record
        .targets
        .values()
        .all(|target| target.outcome == RecordedOutcome::Passed));
    Ok(())
}

#[test]
fn another_tree_catalog_or_toolchain_is_refused_and_a_dirty_tree_has_no_key(
) -> Result<(), GateError> {
    let repo = Repo::new();
    let stamps = tempfile::tempdir().map_err(|source| GateError::Io {
        path: PathBuf::from("tempdir"),
        source,
    })?;
    let key = repo.key();
    first_run(&key, stamps.path())?;
    let recorded = ResultRecord::read(stamps.path(), &key.tree, Tier::Package)?
        .expect("invariant: the first run wrote it");

    // Another toolchain on the same tree.
    let mut newer = key.clone();
    newer.toolchain = "rustc 9.99.0 (0000000 2099-01-01)".into();
    let plan = plan_resume(Some(recorded.clone()), &newer, &names(&SELECTED));
    assert!(plan
        .refused
        .as_deref()
        .is_some_and(|why| why.contains("rustc 9.99.0")));
    assert!(plan.reused_targets.is_empty() && plan.reused_steps.is_empty());

    // A changed catalog is a new tree; its record does not exist, and the old one is refused
    // for its catalog even when read across trees.
    repo.commit_catalog("SuiteCatalog(two)");
    let changed = repo.key();
    assert_ne!(changed.catalog, key.catalog);
    assert_eq!(
        ResultRecord::read(stamps.path(), &changed.tree, Tier::Package)?,
        None
    );
    let plan = plan_resume(Some(recorded.clone()), &changed, &names(&SELECTED));
    let why = plan.refused.unwrap_or_default();
    assert!(why.contains("tree"), "{why}");
    let mut same_tree = changed.clone();
    same_tree.tree.clone_from(&key.tree);
    let plan = plan_resume(Some(recorded), &same_tree, &names(&SELECTED));
    let why = plan.refused.unwrap_or_default();
    assert!(why.contains(CATALOG_PATH), "{why}");

    // A dirty working tree has no key, so nothing is recorded or resumed.
    std::fs::write(repo.root().join("new.txt"), "x").map_err(|source| GateError::Io {
        path: repo.root().join("new.txt"),
        source,
    })?;
    assert!(matches!(
        ResumeKey::of_clean_head(&repo.git, Tier::Package),
        Err(GateError::DirtyTree(_))
    ));
    Ok(())
}

/// The test file of the end-to-end crate. It counts its runs in `$WOBBLE_DIR/<crate>.runs`;
/// with `FAIL_FIRST` replaced by `true`, its first run fails and every later one passes.
const COUNTING_TEST: &str = r#"//! Counts its runs; fails on the first when `FAIL_FIRST` is set.

#[test]
fn counts() {
    let dir = std::path::PathBuf::from(std::env::var("WOBBLE_DIR").expect("set by the test"));
    let file = dir.join(format!("{}.runs", env!("CARGO_CRATE_NAME")));
    let runs = std::fs::read_to_string(&file).map_or(0, |text| text.len()) + 1;
    std::fs::write(&file, "x".repeat(runs)).expect("writable");
    assert!(!(FAIL_FIRST && runs == 1), "the first run fails");
}
"#;

/// A Git repository with one crate whose test `steady` passes and whose test `wobbly` fails on
/// its first run only, the catalog the gate needs, and two commits, so `--base HEAD~1` selects
/// everything.
fn wobbly_repository(root: &Path) {
    let write = |path: &str, text: &str| {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().expect("invariant: a file has a parent"))
            .expect("invariant: writable");
        std::fs::write(full, text).expect("invariant: writable");
    };
    let run = |program: &str, args: &[&str]| {
        let status = std::process::Command::new(program)
            .args(args)
            .current_dir(root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("invariant: git and cargo exist");
        assert!(status.success(), "{program} {args:?} failed");
    };
    let commit = |message: &str| {
        run("git", &["add", "-A"]);
        run(
            "git",
            &[
                "-c",
                "user.name=gate",
                "-c",
                "user.email=gate@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                message,
            ],
        );
    };
    std::fs::create_dir_all(root).expect("invariant: writable");
    run("git", &["init", "--quiet"]);
    commit("empty");
    write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/wobble\"]\nresolver = \"2\"\n",
    );
    write(
        "crates/wobble/Cargo.toml",
        "[package]\nname = \"wobble\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write("crates/wobble/src/lib.rs", "//! Nothing to test.\n");
    write(
        "crates/wobble/tests/steady.rs",
        &COUNTING_TEST.replace("FAIL_FIRST", "false"),
    );
    write(
        "crates/wobble/tests/wobbly.rs",
        &COUNTING_TEST.replace("FAIL_FIRST", "true"),
    );
    write(
        "crates/gate/suites.ron",
        "SuiteCatalog(global: [], docs: [])\n",
    );
    write(".gitignore", "/target\n");
    run("cargo", &["generate-lockfile", "--offline"]);
    commit("wobble");
}

/// Runs `gate package --base HEAD~1` with `extra` in `work`, with a private gate lock, stamp,
/// log and target directory under `dir`; returns whether it was green and what it printed.
fn package_gate(dir: &Path, work: &Path, extra: &[&str]) -> (bool, String) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_gate"))
        .args(["package", "--base", "HEAD~1", "--jobs", "1"])
        .args(["--memory-ceiling-gib", "1000", "--stamp-dir"])
        .arg(dir.join("stamps"))
        .arg("--log-dir")
        .arg(dir.join("logs"))
        .args(extra)
        .current_dir(work)
        .env(LOCK_PATH_VAR, dir.join("gate.lock"))
        .env_remove(HELD_VAR)
        .env_remove("SPACE_SCUFFLE_GATE_JOBS")
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env("WOBBLE_DIR", dir)
        .output()
        .expect("invariant: the gate binary was built");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    (output.status.success(), text)
}

/// How often the end-to-end test `name` ran.
fn runs(dir: &Path, name: &str) -> usize {
    std::fs::read_to_string(dir.join(format!("{name}.runs"))).map_or(0, |text| text.len())
}

/// The real binary: a run red only by a flake writes no stamp but leaves its record;
/// `--resume` runs only the flaky target, reuses fmt, clippy and the passed targets, and stamps
/// the tree once the flaky target has passed, naming what it reused.
#[test]
fn the_gate_resumes_a_red_run_and_stamps_only_when_every_target_passed() {
    let dir = tempfile::tempdir().expect("invariant: a temporary directory can be made");
    let work = dir.path().join("work");
    wobbly_repository(&work);
    let stamps = dir.path().join("stamps");
    let tree = Git::discover(&work)
        .and_then(|git| git.tree("HEAD"))
        .expect("invariant: the repository was just made");

    let (green, text) = package_gate(dir.path(), &work, &[]);
    assert!(!green, "a flake is not green:\n{text}");
    assert!(text.contains("FLAKY wobble/wobbly"), "{text}");
    assert_eq!(
        (runs(dir.path(), "steady"), runs(dir.path(), "wobbly")),
        (1, 2)
    );
    assert_eq!(
        Stamp::read(&stamps, &tree, Tier::Package).expect("invariant: readable"),
        None,
        "a red run writes no stamp"
    );
    let record = ResultRecord::read(&stamps, &tree, Tier::Package)
        .expect("invariant: readable")
        .expect("the red run left its record");
    assert_eq!(
        record.targets["wobble/wobbly"].outcome,
        RecordedOutcome::Flaky
    );
    assert_eq!(
        record.targets["wobble/steady"].outcome,
        RecordedOutcome::Passed
    );

    let (green, text) = package_gate(dir.path(), &work, &["--resume"]);
    assert!(green, "the flaky target passed in the resumed run:\n{text}");
    assert!(text.contains("REUSED wobble/steady"), "{text}");
    assert!(
        text.contains("step fmt: passed on this tree before with the same command"),
        "{text}"
    );
    assert_eq!(
        (runs(dir.path(), "steady"), runs(dir.path(), "wobbly")),
        (1, 3),
        "only the flaky target ran again"
    );
    let stamp = Stamp::read(&stamps, &tree, Tier::Package)
        .expect("invariant: readable")
        .expect("a green resumed run stamps the tree");
    assert!(stamp.is_green());
    assert!(stamp.resumed_targets.contains(&"wobble/steady".to_string()));
    assert!(stamp.resumed_steps.contains(&"fmt".to_string()));
    let names: BTreeSet<&str> = stamp.targets.iter().map(|t| t.target.as_str()).collect();
    assert!(
        names.contains("wobble/steady") && names.contains("wobble/wobbly"),
        "the stamp names every selected target: {names:?}"
    );
}
