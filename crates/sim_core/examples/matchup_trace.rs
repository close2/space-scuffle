//! Traces a "who wins" matrix battle tick by tick on many seeds, or plays the whole matrix.
//!
//! # Responsibility
//! The command-line entry point of the battle trace tool of the battle 33 investigation
//! (`docs/plans/2026-11-04-zealot-marine-investigation.md`, work package 3), extended with the
//! whole-matrix mode of the combat alignment plan (`docs/plans/2026-11-04-combat-sc2-alignment.md`,
//! WP0 and D-5). Everything it does lives in `sim_fixtures::trace::cli` and
//! `sim_fixtures::trace::matrix`, where it is tested; this file only forwards the arguments and
//! reports the result.
//!
//! # How it fits
//! An example of `sim_core`, so it builds against the simulation exactly as the matrix test does,
//! and uses the test-support crate `sim_fixtures` as a dev-dependency. It names no unit: the
//! battle, catalog and fixture are arguments.
//!
//! ```text
//! # One battle, traced tick by tick into <out>/<battle>/:
//! cargo run --release -p sim_core --example matchup_trace -- \
//!     --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out ~/scratch/b33
//! # Every battle on the seeds of record plus 16, outcomes only, into <out>/matrix.{csv,md},
//! # compared with an earlier run:
//! cargo run --release -p sim_core --example matchup_trace -- \
//!     --all-battles --seeds 16 --out ~/scratch/step2/matrix \
//!     --compare ~/scratch/step1/matrix/matrix.csv
//! ```
//!
//! # Invariants
//! - Exits with status 2 on a usage error and 1 on any other failure, after printing it.

use sim_fixtures::trace::cli::{run, RunError, TraceArgs};
use sim_fixtures::trace::matrix::run_matrix;
use std::process::ExitCode;

/// Plays what `args` selects and prints a short report.
fn execute(args: &TraceArgs) -> Result<(), RunError> {
    if args.all_battles {
        let output = run_matrix(args)?;
        let failures = |of_record: bool| {
            output
                .rows
                .iter()
                .filter(|row| !row.expectation_met && (row.of_record || !of_record))
                .count()
        };
        println!(
            "{} runs: expectation failures {} on the seeds of record, {} over all seeds",
            output.rows.len(),
            failures(true),
            failures(false)
        );
        if let Some(comparison) = &output.comparison {
            if comparison.moved.is_empty() {
                println!("bit-identical to {}", comparison.file.display());
            } else {
                println!(
                    "moved against {}: {}",
                    comparison.file.display(),
                    comparison.moved.join(", ")
                );
            }
        }
        println!("wrote {}", output.directory.display());
    } else {
        let output = run(args)?;
        for result in &output.results {
            let summary = &result.summary;
            println!(
                "seed {}: end tick {}, survivors blue {} red {}",
                result.seed, summary.end_tick, summary.survivors[0], summary.survivors[1]
            );
        }
        println!("wrote {}", output.directory.display());
    }
    Ok(())
}

fn main() -> ExitCode {
    let outcome = TraceArgs::parse(std::env::args().skip(1)).and_then(|args| execute(&args));
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error @ RunError::Usage(_)) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("matchup_trace: {error}");
            ExitCode::FAILURE
        }
    }
}
