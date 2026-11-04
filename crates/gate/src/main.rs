//! `cargo gate`: the command-line entry point of the tiered test gates.
//!
//! # Responsibility
//! Parses the arguments, runs the command against the repository containing the current
//! directory, prints the result, and turns it into an exit status: 0 for green (or a report that
//! ran nothing), 1 for a red run, 2 for an error that prevented a decision.
//!
//! # How it fits
//! `.cargo/config.toml` aliases `cargo gate` to `cargo run --quiet --locked --package gate --`.
//! All logic lives in the library; this file only connects it to the process.
//!
//! # Invariants
//! - An error is printed and exits 2; it is never reported as a green run.

use gate::{
    class_report, failing_targets, parse_args, parse_test_log, render_log_timings,
    render_selection, repeated_flakes, run_tier, select, stamp_dir, suspect_report, Command,
    Context, GateError, Stamp, TargetId, FLAKE_LIMIT,
};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Prints the selection of `explain` and runs nothing.
fn explain(
    cwd: &Path,
    spec: Option<&str>,
    tier: gate::Tier,
    main_ref: &str,
) -> Result<(), GateError> {
    let context = Context::load(cwd)?;
    let (changed, what) = context.changes(spec, main_ref)?;
    let selection = select(tier, context.inputs(), &changed)?;
    print!("{}", render_selection(&selection, &what));
    Ok(())
}

/// Reads a `cargo test` log.
fn read_log(path: &Path) -> Result<String, GateError> {
    std::fs::read_to_string(path).map_err(|source| GateError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Prints the class and flake report from the stamps, or the Markdown report of one log.
fn timings(
    cwd: &Path,
    from_log: Option<&Path>,
    options: &gate::RunOptions,
) -> Result<(), GateError> {
    if let Some(log) = from_log {
        let context = Context::load(cwd)?;
        let binaries = parse_test_log(&read_log(log)?);
        print!(
            "{}",
            render_log_timings(&binaries, &context.workspace, &context.catalog)
        );
        return Ok(());
    }
    let dir = stamp_dir(options)?;
    let stamps = Stamp::read_all(&dir)?;
    let findings = class_report(&stamps);
    println!(
        "cargo gate timings: {} stamp(s) in {}",
        stamps.len(),
        dir.display()
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    for flake in repeated_flakes(&stamps, now) {
        println!(
            "  {:<48} flaked {} times in 7 days (limit {FLAKE_LIMIT}): a bug to fix, not to rerun",
            flake.target, flake.count
        );
    }
    if findings.is_empty() {
        println!("Every class agrees with its recorded times.");
    }
    for finding in findings {
        println!(
            "  {:<48} {:<5} median {:>7.1} s over {} run(s)",
            finding.target,
            finding.class.name(),
            Duration::from_millis(finding.median_ms).as_secs_f64(),
            finding.runs
        );
    }
    Ok(())
}

/// Prints the red rule's suspect list for the named targets and those a log reports failing.
fn suspects(
    cwd: &Path,
    targets: &[String],
    from_log: Option<&Path>,
    options: &gate::RunOptions,
) -> Result<(), GateError> {
    let context = Context::load(cwd)?;
    let known: Vec<TargetId> = context
        .workspace
        .test_targets()
        .map(|target| target.id.clone())
        .collect();
    let mut failing = BTreeSet::new();
    for name in targets {
        let id = known
            .iter()
            .find(|id| id.to_string() == *name)
            .ok_or_else(|| GateError::Usage(format!("`{name}` is not a test target")))?;
        failing.insert(id.clone());
    }
    if let Some(log) = from_log {
        failing.extend(failing_targets(&read_log(log)?, &context.workspace));
    }
    if failing.is_empty() {
        println!("cargo gate suspects: no failing target named or found in the log.");
        return Ok(());
    }
    let stamps = Stamp::read_all(&stamp_dir(options)?)?;
    print!(
        "{}",
        suspect_report(&context, &failing, options.base.as_deref(), &stamps)?
    );
    Ok(())
}

/// Runs the command; `Ok(true)` is green.
fn run(args: &[String]) -> Result<bool, GateError> {
    let command = parse_args(args)?;
    let cwd = std::env::current_dir().map_err(|source| GateError::Io {
        path: ".".into(),
        source,
    })?;
    match &command {
        Command::Explain {
            spec,
            tier,
            options,
        } => explain(&cwd, spec.as_deref(), *tier, &options.main_ref).map(|()| true),
        Command::Timings { from_log, options } => {
            timings(&cwd, from_log.as_deref(), options).map(|()| true)
        }
        Command::Suspects {
            targets,
            from_log,
            options,
        } => suspects(&cwd, targets, from_log.as_deref(), options).map(|()| true),
        Command::Run { .. } | Command::Merge { .. } => {
            let report = run_tier(&command, &cwd)?;
            print!("{}", report.render());
            Ok(report.passed())
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("cargo gate: {error}");
            ExitCode::from(2)
        }
    }
}
