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
//! - `quick`, `package`, `full` and `merge` hold the gate lock (`src/gate_lock.rs`) from before
//!   their first step until they end; `explain`, `timings` and `suspects` never take it.

use gate::{
    class_report, current_holder, failing_targets, lock_path, parse_args, parse_test_log,
    render_log_timings, render_selection, repeated_flakes, run_tier, select_change, stamp_dir,
    stop_path, stop_request, suspect_report, Command, Context, GateError, GateLock, Holder, Stamp,
    TargetId, FLAKE_LIMIT, HELD_VAR,
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
    let (change, what) = context.changes(spec, main_ref)?;
    let selection = select_change(tier, context.inputs(), &change)?;
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
            "  {:<48} flaked {} times in 7 days ({} with other targets running; limit \
             {FLAKE_LIMIT}): a bug to fix, not to rerun",
            flake.target, flake.count, flake.overlapped
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

/// Takes the gate lock for `what` (a tier, or `merge <branch>`) run in `cwd`, then marks this
/// process's environment so that every process it starts, a nested `cargo gate` included, knows
/// the lock is held (`src/gate_lock.rs`, [`HELD_VAR`]).
fn take_gate_lock(what: &str, cwd: &Path) -> Result<GateLock, GateError> {
    let path = lock_path()?;
    let marker = std::env::var_os(HELD_VAR);
    let lock = GateLock::acquire(
        &path,
        marker.as_deref(),
        &Holder::this_process(what, cwd),
        |message| eprintln!("{message}"),
    )?;
    // Set while this process is still single-threaded: the gate starts its lanes later.
    std::env::set_var(HELD_VAR, lock.path());
    Ok(lock)
}

/// `cargo gate stop`: asks the gate holding the lock to stop, without waiting for the lock.
fn stop() -> Result<(), GateError> {
    let path = lock_path()?;
    match current_holder(&path)? {
        None => println!("cargo gate stop: no gate holds {}", path.display()),
        Some(holder) => {
            let request = stop_path(&path);
            std::fs::write(&request, stop_request(holder.pid)).map_err(|source| GateError::Io {
                path: request.clone(),
                source,
            })?;
            println!(
                "cargo gate stop: asked gate {} ({} in {}) to stop; it kills its running \
                 tests, skips the rest and exits red without a stamp",
                holder.pid,
                holder.tier,
                holder.worktree.display()
            );
        }
    }
    Ok(())
}

/// How a run ended.
enum Verdict {
    /// Green.
    Green,
    /// Red.
    Red,
    /// Ended by `cargo gate stop`.
    Stopped,
}

/// Runs a tier or `merge` under the gate lock and prints its report.
fn run_locked(what: &str, command: &Command, cwd: &Path) -> Result<Verdict, GateError> {
    let lock = take_gate_lock(what, cwd)?;
    let mut command = command.clone();
    if let Command::Run { options, .. } | Command::Merge { options, .. } = &mut command {
        options.stop_file = Some(stop_path(lock.path()));
    }
    let report = run_tier(&command, cwd)?;
    print!("{}", report.render());
    if report.stopped {
        // The request is answered; the next gate must not see it.
        std::fs::remove_file(stop_path(lock.path())).ok();
        return Ok(Verdict::Stopped);
    }
    Ok(if report.passed() {
        Verdict::Green
    } else {
        Verdict::Red
    })
}

/// Runs the command.
fn run(args: &[String]) -> Result<Verdict, GateError> {
    let green = |()| Verdict::Green;
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
        } => explain(&cwd, spec.as_deref(), *tier, &options.main_ref).map(green),
        Command::Timings { from_log, options } => {
            timings(&cwd, from_log.as_deref(), options).map(green)
        }
        Command::Suspects {
            targets,
            from_log,
            options,
        } => suspects(&cwd, targets, from_log.as_deref(), options).map(green),
        Command::Stop => stop().map(green),
        Command::Run { tier, .. } => run_locked(tier.name(), &command, &cwd),
        Command::Merge { branch, .. } => run_locked(&format!("merge {branch}"), &command, &cwd),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(Verdict::Green) => ExitCode::SUCCESS,
        Ok(Verdict::Red) => ExitCode::from(1),
        Ok(Verdict::Stopped) => ExitCode::from(2),
        Err(error) => {
            eprintln!("cargo gate: {error}");
            ExitCode::from(2)
        }
    }
}
