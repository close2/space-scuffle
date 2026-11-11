//! Running a tier: fmt, clippy, the selected tests, doc tests, catalog commands, then a stamp.
//!
//! # Responsibility
//! Turns a parsed [`Command`] into a run (blueprint §5.3): loads the catalog, the workspace and
//! the targets' sources ([`Context`]), computes the change and the selection, prints the
//! selection, runs the steps the tier requires in order, prints the result table, and writes a
//! [`Stamp`] when a stamping tier passes on a clean tree. It also implements the `merge` tier's
//! use of a branch's stamp (blueprint §3.4), the flake rule (every failed target is rerun once at
//! once; one that then passes is flaky, not green, and a `full` run red only by flakes is
//! repeated once, recording them in its stamp: blueprint §3.5), the compiler-crash retry of every
//! Cargo step (`crash.rs`, Amendment 1), and the report's notes: the time
//! saved against the last `full` stamp (blueprint §8) and, for a red `full` run, the red rule's
//! suspect list (`suspects.rs`). Before building, `merge` and `full` reuse a green stamp of the
//! same clean tree when one exists (Amendment 3, "test a tree once").
//!
//! # How it fits
//! `src/main.rs` calls [`run_tier`]; everything that spawns a process lives in `execute.rs`,
//! every decision in the pure modules (`select.rs`, `merge.rs`, `schedule.rs`).
//!
//! # Invariants
//! - The cheap steps (fmt, clippy, the build) run first; the first one that fails ends the run,
//!   so no test time is spent on a change that does not compile or is not formatted.
//! - A tier refuses to run in an agent worktree without `CARGO_TARGET_DIR` (charter §7).
//! - Only a green `package`, `merge` or `full` run on a clean working tree writes a stamp; a
//!   dirty tree is reported, not stamped.
//! - A flaky target never makes a run green; only the one repeat of a `full` run can.
//! - A reused run builds and runs nothing, and its stamp names the stamp it stands on; it is
//!   only ever for the exact tree that stamp recorded, on a clean working tree.

use crate::catalog::SuiteCatalog;
use crate::changes::{cargo_program, ChangeSpec, Git};
use crate::cli::{Command, RunOptions};
use crate::cli::{CEILING_MARGIN_MIB, DEFAULT_MEMORY_BUDGET_MIB};
use crate::crash::{crash_log, incremental_dirs, run_with_crash_retry};
use crate::execute::{
    build_targets, gib, lanes_tsv, mark_flakes, overlapping, run_logged, run_targets, time_limit,
    LaneRun, Outcome, TargetResult,
};
use crate::explain::render_selection;
use crate::memory::{own_process_group, MemorySource};
use crate::merge::{changed_crates, choose_stamp, overlapping_crates, BranchCommit};
use crate::schedule::{Job, MemoryLimits, Scheduler};
use crate::select::{
    is_judged_by_content, select_change, Change, ClippyScope, Inputs, Reason, Selected, Selection,
};
use crate::sources::SourceIndex;
use crate::stamp::peak_estimates;
use crate::stamp::{Reuse, Stamp, StampTarget, StampTree};
use crate::suspects::suspect_report;
use crate::tier::Tier;
use crate::timings::time_saved;
use crate::workspace::{TargetId, Workspace};
use crate::GateError;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Everything the gate reads about the repository.
#[derive(Clone, Debug)]
pub struct Context {
    /// The repository.
    pub git: Git,
    /// The suite catalog.
    pub catalog: SuiteCatalog,
    /// The workspace's crates and targets.
    pub workspace: Workspace,
    /// The targets' source files.
    pub sources: SourceIndex,
}

impl Context {
    /// Loads the repository containing `dir`.
    ///
    /// # Errors
    /// Git, Cargo, catalog and source errors.
    pub fn load(dir: &Path) -> Result<Self, GateError> {
        let git = Git::discover(dir)?;
        let catalog = SuiteCatalog::load(git.root())?;
        let workspace = Workspace::load(git.root())?;
        let sources = SourceIndex::build(git.root(), &workspace)?;
        Ok(Self {
            git,
            catalog,
            workspace,
            sources,
        })
    }

    /// The inputs of [`select`].
    #[must_use]
    pub fn inputs(&self) -> Inputs<'_> {
        Inputs {
            catalog: &self.catalog,
            workspace: &self.workspace,
            sources: &self.sources,
        }
    }

    /// The change a spec names (default: everything since the merge base with `main`), with a
    /// description for the report. The files the selection judges by content come with both of
    /// their versions.
    ///
    /// # Errors
    /// Git errors.
    pub fn changes(
        &self,
        spec: Option<&str>,
        main_ref: &str,
    ) -> Result<(Change, String), GateError> {
        let spec = match spec {
            Some(text) => ChangeSpec::parse(text),
            None => ChangeSpec::Since(self.git.merge_base("HEAD", main_ref)?),
        };
        let what = match &spec {
            ChangeSpec::Since(base) => format!("changes since {}", short(base)),
            ChangeSpec::Between(from, to) => format!("changes {from}..{to}"),
        };
        Ok((self.change(&spec)?, what))
    }

    /// The change `spec` names, with both versions of every changed file judged by content.
    ///
    /// # Errors
    /// Git errors.
    pub fn change(&self, spec: &ChangeSpec) -> Result<Change, GateError> {
        let paths = self.git.changed_paths(spec)?;
        let mut contents = BTreeMap::new();
        for path in paths
            .iter()
            .filter(|path| is_judged_by_content(path, &self.workspace))
        {
            contents.insert(path.clone(), self.git.versions(spec, path)?);
        }
        Ok(Change { paths, contents })
    }
}

/// `n` with a comma between groups of three digits (`6,534`).
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The first 12 characters of a hash, for reports.
fn short(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

/// One fixed step's result (fmt, clippy, build, doc tests, a catalog command).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepResult {
    /// The step's name.
    pub name: String,
    /// Whether it passed.
    pub passed: bool,
    /// Its wall time.
    pub wall: Duration,
    /// Its log file.
    pub log: PathBuf,
    /// Whether its first attempt crashed the compiler, so it ran a second time on a cleared
    /// incremental cache (blueprint Amendment 1, D5); the first log is kept as
    /// `<name>.crash.log`.
    pub retried: bool,
}

/// The result of one tier run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunReport {
    /// The tier.
    pub tier: Tier,
    /// The fixed steps, in run order.
    pub steps: Vec<StepResult>,
    /// The test executables, in finishing order.
    pub targets: Vec<TargetResult>,
    /// The stamp written, or why none was.
    pub stamp: Result<PathBuf, String>,
    /// Lines printed after the stamp: the time saved, flake findings, the red rule's suspects.
    pub notes: Vec<String>,
    /// The test phase's wall time (the lane run, without reruns), when tests ran.
    pub test_wall: Option<Duration>,
    /// The largest live `anon` of the slice during the test phase, in MiB.
    pub slice_peak_anon_mib: Option<u64>,
    /// Whether `cargo gate stop` ended the run (red, no stamp, exit status 2).
    pub stopped: bool,
}

impl RunReport {
    /// Whether every step and every target passed.
    #[must_use]
    pub fn passed(&self) -> bool {
        !self.stopped
            && self.steps.iter().all(|step| step.passed)
            && self
                .targets
                .iter()
                .all(|target| target.outcome == Outcome::Passed)
    }

    /// The result table and the failures, for the terminal.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!("\ncargo gate {}: results\n", self.tier);
        for step in &self.steps {
            let verdict = if step.passed { "PASS" } else { "FAIL" };
            let retry = if step.retried {
                " (retried once after a compiler crash)"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "  {verdict:<5} {:>8.1} s            step {}{retry}",
                step.wall.as_secs_f64(),
                step.name
            );
        }
        let mut targets = self.targets.clone();
        targets.sort_by(|a, b| a.target.cmp(&b.target));
        for result in &targets {
            let verdict = result.outcome.verdict();
            let cpu = result.cpu.map_or_else(
                || "    -   ".to_string(),
                |c| format!("{:>8.1}", c.as_secs_f64()),
            );
            let _ = writeln!(
                out,
                "  {verdict:<5} {:>8.1} s {cpu} cpu  {:<8} {}",
                result.wall.as_secs_f64(),
                result.class.name(),
                result.target
            );
        }
        out.push_str(&self.render_failures());
        let verdict = if self.passed() { "GREEN" } else { "RED" };
        let stopped = if self.stopped {
            " (stopped by `cargo gate stop`)"
        } else {
            ""
        };
        let _ = writeln!(out, "\n{verdict}{stopped}: {}.", self.time_line());
        match &self.stamp {
            Ok(path) => {
                let _ = writeln!(out, "Stamp: {}", path.display());
            }
            Err(why) => {
                let _ = writeln!(out, "No stamp: {why}");
            }
        }
        for note in &self.notes {
            let _ = writeln!(out, "{note}");
        }
        out
    }

    /// The report's time summary: `steps 101 s, tests 2,410 s wall (6,534 s of executables,
    /// overlap 2.7×), slice anon peak 24.8 GiB` (plan §3.5).
    #[must_use]
    pub fn time_line(&self) -> String {
        let steps: Duration = self.steps.iter().map(|s| s.wall).sum();
        let executables: Duration = self.targets.iter().map(|t| t.wall).sum();
        let mut line = format!("steps {} s", thousands(steps.as_secs()));
        let Some(wall) = self.test_wall else {
            return line;
        };
        let overlap = if wall.is_zero() {
            "-".to_string()
        } else {
            format!("{:.1}", executables.as_secs_f64() / wall.as_secs_f64())
        };
        let _ = write!(
            line,
            ", tests {} s wall ({} s of executables, overlap {overlap}×)",
            thousands(wall.as_secs()),
            thousands(executables.as_secs())
        );
        if let Some(peak) = self.slice_peak_anon_mib {
            let _ = write!(line, ", slice anon peak {}", gib(peak));
        }
        line
    }

    /// The flaky targets whose failed run overlapped other targets on the lanes.
    #[must_use]
    pub fn overlapped_flakes(&self) -> Vec<String> {
        self.targets
            .iter()
            .filter(|t| matches!(t.outcome, Outcome::Flaky { .. }))
            .filter(|t| !overlapping(&self.targets, &t.target).is_empty())
            .map(|t| t.target.to_string())
            .collect()
    }

    /// The targets that failed and then passed on their immediate rerun, by name.
    #[must_use]
    pub fn flaky_targets(&self) -> Vec<String> {
        let mut flaky: Vec<String> = self
            .targets
            .iter()
            .filter(|result| matches!(result.outcome, Outcome::Flaky { .. }))
            .map(|result| result.target.to_string())
            .collect();
        flaky.sort();
        flaky
    }

    /// Whether the run is red only because of flaky targets: every step passed and every
    /// target passed or was flaky, with at least one flaky.
    #[must_use]
    pub fn red_only_by_flakes(&self) -> bool {
        self.steps.iter().all(|step| step.passed)
            && !self.flaky_targets().is_empty()
            && self
                .targets
                .iter()
                .all(|target| matches!(target.outcome, Outcome::Passed | Outcome::Flaky { .. }))
    }

    /// The targets that did not pass (failed, errored or flaky).
    #[must_use]
    pub fn failing_targets(&self) -> BTreeSet<TargetId> {
        self.targets
            .iter()
            .filter(|result| result.outcome != Outcome::Passed)
            .map(|result| result.target.clone())
            .collect()
    }

    /// The names of the steps that were retried after a compiler crash, in run order.
    #[must_use]
    pub fn retried_steps(&self) -> Vec<String> {
        self.steps
            .iter()
            .filter(|step| step.retried)
            .map(|step| step.name.clone())
            .collect()
    }

    /// The failing steps and targets with their logs.
    fn render_failures(&self) -> String {
        let mut out = String::new();
        for step in self.steps.iter().filter(|s| !s.passed) {
            let _ = writeln!(out, "\nFAILED step {}: {}", step.name, step.log.display());
            if step.retried {
                let _ = writeln!(
                    out,
                    "    the compiler crashed, and the retry failed too; the first log is {}",
                    crash_log(&step.log).display()
                );
            }
        }
        for result in &self.targets {
            match &result.outcome {
                Outcome::Passed | Outcome::Skipped => {}
                Outcome::Failed { tests } => {
                    let _ = writeln!(out, "\nFAILED {}: {}", result.target, result.log.display());
                    for test in tests {
                        let _ = writeln!(out, "    {test}");
                    }
                }
                Outcome::Error(why) => {
                    let _ = writeln!(out, "\nERROR {}: {why}", result.target);
                }
                Outcome::TimedOut => {
                    let _ = writeln!(
                        out,
                        "\nTIMED OUT {}: its process tree was killed at its time limit: {}",
                        result.target,
                        result.log.display()
                    );
                }
                Outcome::Flaky { tests } => {
                    let _ = writeln!(
                        out,
                        "\nFLAKY {}: failed, then passed on an immediate rerun; not green \
                         ({}; rerun log in rerun/)",
                        result.target,
                        result.log.display()
                    );
                    for test in tests {
                        let _ = writeln!(out, "    {test}");
                    }
                }
            }
        }
        out
    }
}

/// Fails when `root` is an agent worktree and `CARGO_TARGET_DIR` is unset (charter §7).
///
/// # Errors
/// [`GateError::TargetDirRequired`].
pub fn check_target_dir(root: &Path) -> Result<(), GateError> {
    if target_dir_required(root, std::env::var_os("CARGO_TARGET_DIR").as_deref()) {
        return Err(GateError::TargetDirRequired(root.to_path_buf()));
    }
    Ok(())
}

/// Whether a build in `root` with `CARGO_TARGET_DIR` = `target_dir` breaks charter §7: an agent
/// worktree must build into `~/scratch`, never into its own `target/`.
fn target_dir_required(root: &Path, target_dir: Option<&std::ffi::OsStr>) -> bool {
    let in_worktree = root.to_string_lossy().contains("/.claude/worktrees/");
    in_worktree && target_dir.is_none_or(std::ffi::OsStr::is_empty)
}

/// `~/scratch/<rest>`.
fn scratch(rest: &str) -> Result<PathBuf, GateError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        GateError::Usage("HOME is not set; pass --log-dir and --stamp-dir".to_string())
    })?;
    Ok(PathBuf::from(home).join("scratch").join(rest))
}

/// The stamp directory of `options`.
///
/// # Errors
/// [`GateError::Usage`] when the default is needed and `HOME` is unset.
pub fn stamp_dir(options: &RunOptions) -> Result<PathBuf, GateError> {
    options
        .stamp_dir
        .clone()
        .map_or_else(|| scratch("gate-stamps"), Ok)
}

/// Runs `command` (a `Run` or `Merge`) and returns its report.
///
/// # Errors
/// [`GateError::Usage`] for `explain` and `timings`, which `main.rs` handles; repository, Cargo
/// and I/O errors otherwise. Failing tests are a red report, not an error.
pub fn run_tier(command: &Command, cwd: &Path) -> Result<RunReport, GateError> {
    let context = Context::load(cwd)?;
    check_target_dir(context.git.root())?;
    if let Some(report) = reuse_green_stamp(&context, command)? {
        return Ok(report);
    }
    if let Some(pgid) = own_process_group() {
        println!(
            "process group {pgid}: stop with `cargo gate stop`, or `kill -TERM -- -{pgid}` for \
             the gate and every test it runs"
        );
    }
    match command {
        Command::Run { tier, options } => {
            let (change, what) = if *tier == Tier::Full {
                (Change::default(), "complete suite".to_string())
            } else {
                context.changes(options.base.as_deref(), &options.main_ref)?
            };
            let selection = select_change(*tier, context.inputs(), &change)?;
            println!("{}", render_selection(&selection, &what));
            let mut report = execute(&context, &selection, options, &[], &[])?;
            if *tier == Tier::Full && !report.stopped && report.red_only_by_flakes() {
                let flakes = report.flaky_targets();
                let overlapped = report.overlapped_flakes();
                println!(
                    "full: red only by flaky target(s) {}; repeating the full run once \
                     (blueprint §3.5)",
                    flakes.join(", ")
                );
                report = execute(&context, &selection, options, &flakes, &overlapped)?;
                report.notes.push(format!(
                    "The first attempt was red only by flakes ({}); this repeat records them in \
                     its stamp.",
                    flakes.join(", ")
                ));
            }
            annotate(&context, &mut report, options)?;
            Ok(report)
        }
        Command::Merge { branch, options } => {
            let selection = merge_selection(&context, branch, options)?;
            let mut report = execute(&context, &selection, options, &[], &[])?;
            annotate(&context, &mut report, options)?;
            Ok(report)
        }
        Command::Explain { .. }
        | Command::Timings { .. }
        | Command::Suspects { .. }
        | Command::Stop => Err(GateError::Usage(
            "explain, timings, suspects and stop run no tier".to_string(),
        )),
    }
}

/// For `merge` and `full`: when the clean `HEAD` tree already has a green stamp the tier accepts
/// ([`Stamp::reusable`]), prints which one, writes this tier's stamp naming it, and returns a
/// green report that ran nothing. `None` means the tier runs as before.
///
/// # Errors
/// Repository and stamp-directory errors.
fn reuse_green_stamp(context: &Context, command: &Command) -> Result<Option<RunReport>, GateError> {
    let (tier, options) = match command {
        Command::Run { tier, options } => (*tier, options),
        Command::Merge { options, .. } => (Tier::Merge, options),
        Command::Explain { .. }
        | Command::Timings { .. }
        | Command::Suspects { .. }
        | Command::Stop => return Ok(None),
    };
    if options.no_run || options.no_reuse {
        return Ok(None);
    }
    let dir = stamp_dir(options)?;
    let Some(reuse) = Stamp::reusable(&context.git, &dir, tier)? else {
        return Ok(None);
    };
    let message = reuse.message(tier);
    println!("{message}");
    Ok(Some(RunReport {
        tier,
        steps: Vec::new(),
        targets: Vec::new(),
        stamp: write_reuse_stamp(context, &reuse, tier, options, &dir),
        notes: vec![message],
        test_wall: None,
        slice_peak_anon_mib: None,
        stopped: false,
    }))
}

/// Writes `tier`'s stamp for a run that reused `reuse`: no targets or commands, and the reused
/// stamp's file name in [`Stamp::reused`]. When the reused stamp is this tier's own, it already
/// is this tier's stamp of the tree and is kept as it is.
fn write_reuse_stamp(
    context: &Context,
    reuse: &Reuse,
    tier: Tier,
    options: &RunOptions,
    dir: &Path,
) -> Result<PathBuf, String> {
    if reuse.stamp.tier == tier {
        return Ok(reuse.path.clone());
    }
    let reused = reuse
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    let stamp = Stamp {
        tier,
        tree: reuse.head.tree.clone(),
        commit: reuse.head.commit.clone(),
        base: context.git.merge_base("HEAD", &options.main_ref).ok(),
        host: host_name(),
        finished_unix_s: unix_now(),
        targets: Vec::new(),
        commands: Vec::new(),
        flakes: Vec::new(),
        retries: Vec::new(),
        reused,
        slice_peak_anon_mib: None,
        jobs: None,
        test_wall_ms: None,
        overlapped_flakes: Vec::new(),
    };
    stamp.write(dir).map_err(|error| error.to_string())
}

/// This machine's name, for stamps.
fn host_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|text| text.trim().to_string())
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Seconds since the Unix epoch; zero on a clock before 1970.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Adds the report's notes: flake findings, the time saved against the last `full` stamp (for
/// selective tiers), and for a red `full` run the red rule's suspect list (blueprint §3.5, §8).
///
/// # Errors
/// Git, catalog and selection errors while computing the suspects.
fn annotate(
    context: &Context,
    report: &mut RunReport,
    options: &RunOptions,
) -> Result<(), GateError> {
    let flaky = report.flaky_targets();
    if !flaky.is_empty() {
        report.notes.push(format!(
            "Flaky: {}. A flake is not green: record a finding; two flakes of one target in a \
             week are a bug to fix, not to rerun (blueprint §3.5).",
            flaky.join(", ")
        ));
    }
    let stamps = stamp_dir(options).and_then(|dir| Stamp::read_all(&dir));
    let stamps = match stamps {
        Ok(stamps) => stamps,
        Err(error) => {
            report.notes.push(format!(
                "Time saved and suspects: stamps unreadable: {error}"
            ));
            Vec::new()
        }
    };
    let run_ms = report
        .targets
        .iter()
        .map(|target| u64::try_from(target.wall.as_millis()).unwrap_or(u64::MAX))
        .sum();
    if let Some(line) = time_saved(report.tier, &stamps, run_ms) {
        report.notes.push(line);
    }
    let failing = report.failing_targets();
    if report.tier == Tier::Full && !failing.is_empty() {
        report.notes.push(suspect_report(
            context,
            &failing,
            options.base.as_deref(),
            &stamps,
        )?);
    }
    Ok(())
}

/// The selection of the `merge` tier for `branch`, merged as the current `HEAD`.
fn merge_selection(
    context: &Context,
    branch: &str,
    options: &RunOptions,
) -> Result<Selection, GateError> {
    let git = &context.git;
    let parent = git.commit("HEAD^1")?;
    let commits: Vec<BranchCommit> = git
        .commits_between(&parent, branch)?
        .into_iter()
        .map(|commit| {
            let tree = git.tree(&commit)?;
            Ok(BranchCommit { commit, tree })
        })
        .collect::<Result<_, GateError>>()?;
    let dir = stamp_dir(options)?;
    let found = choose_stamp(&commits, |tree| Stamp::read(&dir, tree, Tier::Package))?;
    let valid = match found {
        Some((index, stamp)) if index > 0 => {
            let later = git.changed_paths(&ChangeSpec::Between(
                commits[index].commit.clone(),
                commits[0].commit.clone(),
            ))?;
            let docs_only = later
                .iter()
                .all(|path| context.catalog.docs.iter().any(|g| g.matches(path)));
            docs_only.then_some(stamp)
        }
        other => other.map(|(_, stamp)| stamp),
    };
    let Some(stamp) = valid else {
        println!("merge: {branch} has no valid package stamp; running the package tier on the merge result");
        let change = context.change(&ChangeSpec::Between(parent.clone(), "HEAD".into()))?;
        let mut selection = select_change(Tier::Package, context.inputs(), &change)?;
        selection.tier = Tier::Merge;
        println!(
            "{}",
            render_selection(&selection, &format!("changes {}..HEAD", short(&parent)))
        );
        return Ok(selection);
    };
    let main_changed = match &stamp.base {
        Some(base) => changed_crates(
            &git.changed_paths(&ChangeSpec::Between(base.clone(), parent.clone()))?,
            &context.workspace,
            &context.catalog,
        ),
        None => context.workspace.packages.keys().cloned().collect(),
    };
    let stamped: BTreeSet<String> = stamp
        .targets
        .iter()
        .filter_map(|t| t.target.split('/').next().map(str::to_string))
        .collect();
    let overlap = overlapping_crates(&stamped, &main_changed);
    println!(
        "merge: package stamp of tree {} found; main moved in {} crate(s) the branch tested",
        short(&stamp.tree),
        overlap.len()
    );
    restamp_selection(context, &stamp, &overlap)
}

/// The `merge` selection for a stamped branch: the charter checks, plus the stamp's `fast` and
/// `slow` targets in crates where `main` moved.
fn restamp_selection(
    context: &Context,
    stamp: &Stamp,
    overlap: &BTreeSet<String>,
) -> Result<Selection, GateError> {
    let mut targets = BTreeMap::new();
    for reference in &context.catalog.always {
        for id in context.catalog.resolve(reference, &context.workspace)? {
            let class = context.catalog.class_of(&id);
            targets.insert(
                id,
                Selected {
                    class,
                    reasons: BTreeSet::from([Reason::Always]),
                },
            );
        }
    }
    let known: BTreeSet<TargetId> = context
        .workspace
        .test_targets()
        .map(|t| t.id.clone())
        .collect();
    for entry in &stamp.targets {
        let Some(id) = known.iter().find(|id| id.to_string() == entry.target) else {
            continue;
        };
        let class = context.catalog.class_of(id);
        if class.runs_on_crate_change() && overlap.contains(&id.package) {
            let reasons = BTreeSet::from([Reason::Downstream(format!("main ({})", id.package))]);
            targets.insert(id.clone(), Selected { class, reasons });
        }
    }
    let selection = Selection {
        tier: Tier::Merge,
        paths: Vec::new(),
        complete: None,
        docs_only: false,
        changed_crates: overlap.clone(),
        targets,
        commands: BTreeMap::new(),
        clippy: ClippyScope::Workspace,
    };
    println!("{}", render_selection(&selection, "a stamped branch"));
    Ok(selection)
}

/// Runs one fixed step in the repository root, retrying it once after a compiler crash, and
/// records it.
fn step(
    steps: &mut Vec<StepResult>,
    name: &str,
    program: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    context: &Context,
    log_dir: &Path,
) -> Result<bool, GateError> {
    println!("step {name}: {program} {}", args.join(" "));
    let log = log_dir.join(format!("{name}.log"));
    let caches = incremental_dirs(&context.workspace.target_directory, args);
    let start = Instant::now();
    let run = run_with_crash_retry(
        &log,
        &caches,
        || run_logged(program, args, env, context.git.root(), &log),
        |passed| *passed,
    )?;
    steps.push(StepResult {
        name: name.to_string(),
        passed: run.value,
        wall: start.elapsed(),
        log,
        retried: run.retried,
    });
    Ok(run.value)
}

/// Converts string literals to owned arguments.
fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

/// The clippy arguments for a scope, or `None` to skip clippy.
fn clippy_args(scope: &ClippyScope) -> Option<Vec<String>> {
    let mut args = strings(&["clippy", "--all-targets", "--locked"]);
    match scope {
        ClippyScope::Skip => return None,
        ClippyScope::Workspace => args.push("--workspace".to_string()),
        ClippyScope::Packages(crates) => {
            for krate in crates {
                args.extend(["-p".to_string(), krate.clone()]);
            }
        }
    }
    args.extend(strings(&["--", "-D", "warnings"]));
    Some(args)
}

/// Runs the selection's steps and tests, then stamps a green run; `flakes` are the flaky
/// targets of an earlier attempt of the same run, recorded in the stamp.
fn execute(
    context: &Context,
    selection: &Selection,
    options: &RunOptions,
    flakes: &[String],
    overlapped: &[String],
) -> Result<RunReport, GateError> {
    let tree = context.git.tree("HEAD")?;
    let log_dir = match &options.log_dir {
        Some(dir) => dir.clone(),
        None => scratch("logs/gate")?
            .join(&tree)
            .join(selection.tier.name()),
    };
    std::fs::create_dir_all(&log_dir).map_err(|source| GateError::Io {
        path: log_dir.clone(),
        source,
    })?;
    println!("logs: {}", log_dir.display());
    let mut report = RunReport {
        tier: selection.tier,
        steps: Vec::new(),
        targets: Vec::new(),
        stamp: Err("the run is not green".to_string()),
        notes: Vec::new(),
        test_wall: None,
        slice_peak_anon_mib: None,
        stopped: false,
    };
    let cargo = cargo_program();
    let none = BTreeMap::new();
    let mut ok = step(
        &mut report.steps,
        "fmt",
        &cargo,
        &strings(&["fmt", "--all", "--", "--check"]),
        &none,
        context,
        &log_dir,
    )?;
    if ok {
        if let Some(args) = clippy_args(&selection.clippy) {
            ok = step(
                &mut report.steps,
                "clippy",
                &cargo,
                &args,
                &none,
                context,
                &log_dir,
            )?;
        }
    }
    if ok {
        run_tests(context, selection, options, &log_dir, &mut report)?;
    }
    if report.passed() {
        report.stamp = stamp_run(context, selection, options, &report, (flakes, overlapped));
    } else if report.stopped {
        report.stamp = Err("stopped by `cargo gate stop`".to_string());
    }
    Ok(report)
}

/// Builds and runs the selected targets, then doc tests and catalog commands.
fn run_tests(
    context: &Context,
    selection: &Selection,
    options: &RunOptions,
    log_dir: &Path,
    report: &mut RunReport,
) -> Result<(), GateError> {
    let root = context.git.root();
    let whole = selection.complete.is_some();
    let ids: BTreeSet<TargetId> = selection.targets.keys().cloned().collect();
    let start = Instant::now();
    println!("step build: {} test target(s)", ids.len());
    let build_log = log_dir.join("build.log");
    let caches = incremental_dirs(&context.workspace.target_directory, &[]);
    let built = run_with_crash_retry(
        &build_log,
        &caches,
        || build_targets(root, &context.workspace, &ids, whole, &build_log),
        Option::is_some,
    )?;
    report.steps.push(StepResult {
        name: "build".to_string(),
        passed: built.value.is_some(),
        wall: start.elapsed(),
        log: build_log,
        retried: built.retried,
    });
    let Some(executables) = built.value else {
        return Ok(());
    };
    if options.no_run {
        report.stamp = Err("--no-run builds only".to_string());
        return Ok(());
    }
    let (expected, estimates) = recorded(options);
    let jobs: Vec<Job> = selection
        .targets
        .iter()
        .map(|(id, selected)| {
            let expected = expected.get(&id.to_string()).copied().unwrap_or_default();
            Job {
                target: id.clone(),
                class: selected.class,
                expected,
                test_threads: context.catalog.test_threads_of(id),
                estimate_mib: estimates.get(&id.to_string()).copied(),
                exclusive: context.catalog.exclusive_of(id),
                timeout: Some(time_limit(expected, context.catalog.timeout_of(id))),
            }
        })
        .collect();
    let memory = MemorySource::discover();
    let limits = memory_limits(options, &memory);
    let scheduler = Scheduler::new(options.jobs, jobs, limits);
    let run = LaneRun {
        root,
        workspace: &context.workspace,
        executables: &executables,
        log_dir,
        memory,
        stop_file: options.stop_file.clone(),
    };
    let lanes = run_targets(&run, scheduler);
    write_lanes_tsv(log_dir, &lanes.results)?;
    report.targets = lanes.results;
    report.test_wall = Some(lanes.wall);
    report.slice_peak_anon_mib = lanes.slice_peak_mib;
    report.stopped = lanes.stopped;
    if report.stopped {
        return Ok(());
    }
    rerun_failures(context, &run, report)?;
    note_overlapping_flakes(report, options.jobs);
    run_extras(context, selection, log_dir, report)
}

/// The admission limits of this run: the budget and ceiling options, else the defaults.
fn memory_limits(options: &RunOptions, memory: &MemorySource) -> MemoryLimits {
    let ceiling = options.memory_ceiling_mib.or_else(|| {
        memory
            .cgroup
            .as_ref()
            .and(memory.limit_bytes)
            .map(|limit| crate::memory::mib(limit).saturating_sub(CEILING_MARGIN_MIB))
    });
    let budget = options
        .memory_budget_mib
        .unwrap_or(DEFAULT_MEMORY_BUDGET_MIB);
    MemoryLimits::new(budget, ceiling)
}

/// Writes `lanes.tsv` into the log directory.
fn write_lanes_tsv(log_dir: &Path, results: &[TargetResult]) -> Result<(), GateError> {
    let path = log_dir.join("lanes.tsv");
    std::fs::write(&path, lanes_tsv(results)).map_err(|source| GateError::Io { path, source })
}

/// Adds a note naming, for each flaky target of a run on several lanes, the targets that
/// overlapped its failed run (plan §3.6).
fn note_overlapping_flakes(report: &mut RunReport, jobs: usize) {
    if jobs <= 1 {
        return;
    }
    let notes: Vec<String> = report
        .targets
        .iter()
        .filter(|t| matches!(t.outcome, Outcome::Flaky { .. }))
        .map(|t| {
            let others = overlapping(&report.targets, &t.target);
            if others.is_empty() {
                format!("FLAKY {} ran alone", t.target)
            } else {
                format!("FLAKY {} overlapped {}", t.target, others.join(", "))
            }
        })
        .collect();
    report.notes.extend(notes);
}

/// Reruns every failed target once, at once and alone, and marks those that pass as flaky
/// (blueprint §3.5). Rerun logs go to `<log dir>/rerun/`, beside the first run's.
///
/// # Errors
/// [`GateError::Io`] if the rerun log directory cannot be created.
fn rerun_failures(
    context: &Context,
    first: &LaneRun<'_>,
    report: &mut RunReport,
) -> Result<(), GateError> {
    let failed: Vec<Job> = report
        .targets
        .iter()
        .filter(|result| matches!(result.outcome, Outcome::Failed { .. } | Outcome::TimedOut))
        .map(|result| {
            let record = result.lane.as_ref();
            Job {
                target: result.target.clone(),
                class: result.class,
                expected: result.wall,
                test_threads: context.catalog.test_threads_of(&result.target),
                estimate_mib: record.and_then(|r| r.peak_anon_mib.or(r.estimate_mib)),
                exclusive: true,
                timeout: Some(time_limit(
                    result.wall,
                    context.catalog.timeout_of(&result.target),
                )),
            }
        })
        .collect();
    if failed.is_empty() {
        return Ok(());
    }
    let rerun_dir = first.log_dir.join("rerun");
    std::fs::create_dir_all(&rerun_dir).map_err(|source| GateError::Io {
        path: rerun_dir.clone(),
        source,
    })?;
    println!(
        "rerunning {} failed target(s) once, alone, to tell a flake from a failure",
        failed.len()
    );
    let rerun = LaneRun {
        log_dir: &rerun_dir,
        memory: first.memory.clone(),
        stop_file: first.stop_file.clone(),
        ..*first
    };
    let reruns = run_targets(&rerun, Scheduler::new(1, failed, MemoryLimits::UNLIMITED));
    report.stopped |= reruns.stopped;
    mark_flakes(&mut report.targets, &reruns.results);
    Ok(())
}

/// Doc tests of the selected libraries, then the selected catalog commands.
fn run_extras(
    context: &Context,
    selection: &Selection,
    log_dir: &Path,
    report: &mut RunReport,
) -> Result<(), GateError> {
    let cargo = cargo_program();
    let none = BTreeMap::new();
    let mut doc = strings(&["test", "--locked", "--doc"]);
    if selection.complete.is_some() {
        doc.push("--workspace".to_string());
    } else {
        let libraries: BTreeSet<&str> = selection
            .targets
            .keys()
            .filter(|id| id.kind == crate::workspace::TargetKind::Lib)
            .map(|id| id.package.as_str())
            .collect();
        if libraries.is_empty() {
            doc.clear();
        }
        for krate in libraries {
            doc.extend(["-p".to_string(), krate.to_string()]);
        }
    }
    if !doc.is_empty() {
        step(
            &mut report.steps,
            "doc-tests",
            &cargo,
            &doc,
            &none,
            context,
            log_dir,
        )?;
    }
    for command in &context.catalog.commands {
        if selection.commands.contains_key(&command.name) {
            step(
                &mut report.steps,
                &command.name,
                &command.program,
                &command.args,
                &command.env,
                context,
                log_dir,
            )?;
        }
    }
    Ok(())
}

/// Each target's wall time in its most recent stamp, for longest-first ordering and time
/// limits, and its memory estimate from this host's recent stamps (plan §3.3).
fn recorded(options: &RunOptions) -> (BTreeMap<String, Duration>, BTreeMap<String, u64>) {
    let mut stamps = stamp_dir(options)
        .and_then(|dir| Stamp::read_all(&dir))
        .unwrap_or_default();
    stamps.sort_by_key(|stamp| stamp.finished_unix_s);
    let mut timings = BTreeMap::new();
    for stamp in &stamps {
        for target in &stamp.targets {
            timings.insert(target.target.clone(), Duration::from_millis(target.wall_ms));
        }
    }
    (timings, peak_estimates(&stamps, &host_name()))
}

/// Writes the stamp of a green run, or says why there is none.
fn stamp_run(
    context: &Context,
    selection: &Selection,
    options: &RunOptions,
    report: &RunReport,
    (flakes, overlapped): (&[String], &[String]),
) -> Result<PathBuf, String> {
    if selection.tier == Tier::Quick {
        return Err("the quick tier writes no stamp".to_string());
    }
    if options.no_run {
        return Err("--no-run builds only".to_string());
    }
    let tree = StampTree::of_clean_head(&context.git).map_err(|error| error.to_string())?;
    let base = context.git.merge_base("HEAD", &options.main_ref).ok();
    let host = host_name();
    let finished_unix_s = unix_now();
    let millis = |d: Duration| u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
    let stamp = Stamp {
        tier: selection.tier,
        tree: tree.tree,
        commit: tree.commit,
        base,
        host,
        finished_unix_s,
        targets: report
            .targets
            .iter()
            .map(|result| StampTarget {
                target: result.target.to_string(),
                class: result.class,
                wall_ms: millis(result.wall),
                cpu_ms: result.cpu.map(millis),
                passed: result.outcome == Outcome::Passed,
                peak_anon_mib: result.lane.as_ref().and_then(|lane| lane.peak_anon_mib),
            })
            .collect(),
        commands: selection.commands.keys().cloned().collect(),
        flakes: flakes.to_vec(),
        retries: report.retried_steps(),
        reused: None,
        slice_peak_anon_mib: report.slice_peak_anon_mib,
        jobs: Some(options.jobs),
        test_wall_ms: report.test_wall.map(millis),
        overlapped_flakes: overlapped.to_vec(),
    };
    let dir = stamp_dir(options).map_err(|error| error.to_string())?;
    stamp.write(&dir).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TestClass;

    #[test]
    fn clippy_scopes_become_cargo_arguments() {
        assert_eq!(clippy_args(&ClippyScope::Skip), None);
        let workspace = clippy_args(&ClippyScope::Workspace).unwrap_or_default();
        assert_eq!(
            workspace.join(" "),
            "clippy --all-targets --locked --workspace -- -D warnings"
        );
        let crates = ClippyScope::Packages(BTreeSet::from(["a".to_string(), "b".to_string()]));
        let some = clippy_args(&crates).unwrap_or_default();
        assert_eq!(
            some.join(" "),
            "clippy --all-targets --locked -p a -p b -- -D warnings"
        );
    }

    #[test]
    fn a_worktree_without_a_target_dir_is_refused() {
        let worktree = Path::new("/home/u/repo/.claude/worktrees/agent-x");
        let scratch = std::ffi::OsStr::new("/home/u/scratch/cargo-target/agent-x");
        assert!(target_dir_required(worktree, None));
        assert!(target_dir_required(
            worktree,
            Some(std::ffi::OsStr::new(""))
        ));
        assert!(!target_dir_required(worktree, Some(scratch)));
        assert!(!target_dir_required(Path::new("/home/u/repo"), None));
    }

    #[test]
    fn a_retried_step_is_shown_in_the_table_and_named_for_the_stamp() {
        let step = |name: &str, passed: bool, retried: bool| StepResult {
            name: name.into(),
            passed,
            wall: Duration::from_secs(1),
            log: PathBuf::from(format!("/l/{name}.log")),
            retried,
        };
        let mut report = RunReport {
            tier: Tier::Package,
            steps: vec![step("fmt", true, false), step("clippy", true, true)],
            targets: Vec::new(),
            stamp: Err("the run is not green".into()),
            notes: Vec::new(),
            test_wall: None,
            slice_peak_anon_mib: None,
            stopped: false,
        };
        assert!(report.passed(), "a crash cured by the retry is green");
        assert_eq!(report.retried_steps(), ["clippy"]);
        assert!(report
            .render()
            .contains("step clippy (retried once after a compiler crash)"));
        report.steps.push(step("build", false, true));
        assert!(!report.passed(), "a second crash stays red");
        let text = report.render();
        assert!(text.contains("FAILED step build: /l/build.log"), "{text}");
        assert!(
            text.contains("the first log is /l/build.crash.log"),
            "{text}"
        );
    }

    fn lane_result(name: &str, start_s: u64, wall_s: u64, outcome: Outcome) -> TargetResult {
        TargetResult {
            target: TargetId::test("c", name),
            class: TestClass::Slow,
            wall: Duration::from_secs(wall_s),
            cpu: None,
            outcome,
            log: PathBuf::from(format!("{name}.log")),
            lane: Some(crate::execute::LaneRecord {
                lane: crate::schedule::Lane::General(1),
                start: Duration::from_secs(start_s),
                peak_anon_mib: Some(1024),
                estimate_mib: Some(1000),
            }),
        }
    }

    fn lane_report(targets: Vec<TargetResult>) -> RunReport {
        RunReport {
            tier: Tier::Full,
            steps: vec![StepResult {
                name: "build".into(),
                passed: true,
                wall: Duration::from_secs(101),
                log: PathBuf::from("build.log"),
                retried: false,
            }],
            targets,
            stamp: Err("the run is not green".into()),
            notes: Vec::new(),
            test_wall: Some(Duration::from_secs(2_410)),
            slice_peak_anon_mib: Some(25_395),
            stopped: false,
        }
    }

    #[test]
    fn the_final_line_shows_test_phase_wall_the_executables_sum_and_the_overlap() {
        let report = lane_report(vec![
            lane_result("a", 0, 4_000, Outcome::Passed),
            lane_result("b", 0, 2_534, Outcome::Passed),
        ]);
        assert_eq!(
            report.time_line(),
            "steps 101 s, tests 2,410 s wall (6,534 s of executables, overlap 2.7×), \
             slice anon peak 24.7 GiB"
        );
        assert!(report
            .render()
            .contains("GREEN: steps 101 s, tests 2,410 s wall"));
        let mut unrun = report.clone();
        unrun.test_wall = None;
        assert_eq!(unrun.time_line(), "steps 101 s");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(thousands(999), "999");
    }

    #[test]
    fn a_stopped_run_writes_no_stamp_and_is_red() {
        let mut report = lane_report(vec![
            lane_result("a", 0, 10, Outcome::Skipped),
            lane_result("b", 0, 10, Outcome::Passed),
        ]);
        report.stopped = true;
        report.stamp = Err("stopped by `cargo gate stop`".into());
        assert!(!report.passed());
        let text = report.render();
        assert!(
            text.contains("RED (stopped by `cargo gate stop`)"),
            "{text}"
        );
        assert!(text.contains("No stamp: stopped"), "{text}");
        assert!(text.contains("SKIP"), "{text}");
    }

    #[test]
    fn a_flake_under_lanes_names_the_targets_that_overlapped_it() {
        let mut report = lane_report(vec![
            lane_result("flaky", 10, 20, Outcome::Flaky { tests: vec![] }),
            lane_result("during", 25, 50, Outcome::Passed),
            lane_result("after", 31, 5, Outcome::Passed),
            lane_result("timeout", 0, 5, Outcome::TimedOut),
        ]);
        note_overlapping_flakes(&mut report, 3);
        assert_eq!(report.notes, ["FLAKY c/flaky overlapped c/during"]);
        assert_eq!(report.overlapped_flakes(), ["c/flaky"]);
        assert!(report.render().contains("TIMED OUT c/timeout"));
        let mut alone = report.clone();
        alone.notes.clear();
        note_overlapping_flakes(&mut alone, 1);
        assert!(alone.notes.is_empty(), "one lane names no overlap");
    }

    #[test]
    fn the_report_is_red_when_any_step_or_target_fails() {
        let mut report = RunReport {
            tier: Tier::Package,
            steps: vec![StepResult {
                name: "fmt".into(),
                passed: true,
                wall: Duration::from_secs(1),
                log: PathBuf::from("fmt.log"),
                retried: false,
            }],
            targets: vec![TargetResult {
                target: TargetId::lib("core"),
                class: TestClass::Fast,
                wall: Duration::from_secs(2),
                cpu: None,
                outcome: Outcome::Failed {
                    tests: vec!["a::b".into()],
                },
                log: PathBuf::from("core__lib.log"),
                lane: None,
            }],
            stamp: Err("the run is not green".into()),
            notes: vec!["Time saved: 1.0 min".into()],
            test_wall: None,
            slice_peak_anon_mib: None,
            stopped: false,
        };
        assert!(!report.passed());
        let text = report.render();
        assert!(text.contains("FAILED core/lib: core__lib.log"));
        assert!(text.contains("    a::b"));
        assert!(text.contains("RED"));
        assert!(text.ends_with("Time saved: 1.0 min\n"), "{text}");
        assert!(!report.red_only_by_flakes());
        assert_eq!(
            report.failing_targets(),
            BTreeSet::from([TargetId::lib("core")])
        );
        report.targets[0].outcome = Outcome::Flaky {
            tests: vec!["a::b".into()],
        };
        assert!(!report.passed(), "a flake is not green");
        assert!(report.red_only_by_flakes());
        assert_eq!(report.flaky_targets(), ["core/lib"]);
        assert!(report
            .render()
            .contains("FLAKY core/lib: failed, then passed"));
        report.targets[0].outcome = Outcome::Passed;
        assert!(report.passed());
        assert!(!report.red_only_by_flakes());
    }
}
