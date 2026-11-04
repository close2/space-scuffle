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
    class_report, parse_args, render_selection, run_tier, select, stamp_dir, Command, Context,
    GateError, Stamp,
};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

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

/// Prints the class report from the stamps.
fn timings(options: &gate::RunOptions) -> Result<(), GateError> {
    let dir = stamp_dir(options)?;
    let stamps = Stamp::read_all(&dir)?;
    let findings = class_report(&stamps);
    println!(
        "cargo gate timings: {} stamp(s) in {}",
        stamps.len(),
        dir.display()
    );
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
        Command::Timings { options } => timings(options).map(|()| true),
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
