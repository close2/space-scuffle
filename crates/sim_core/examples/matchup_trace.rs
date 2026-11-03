//! Traces a "who wins" matrix battle tick by tick on many seeds.
//!
//! # Responsibility
//! The command-line entry point of the battle trace tool of the battle 33 investigation
//! (`docs/plans/2026-11-04-zealot-marine-investigation.md`, work package 3). Everything it does
//! lives in `sim_fixtures::trace::cli`, where it is tested; this file only forwards the arguments
//! and reports the result.
//!
//! # How it fits
//! An example of `sim_core`, so it builds against the simulation exactly as the matrix test does,
//! and uses the test-support crate `sim_fixtures` as a dev-dependency. It names no unit: the
//! battle, catalog and fixture are arguments.
//!
//! ```text
//! cargo run --release -p sim_core --example matchup_trace -- \
//!     --battle 33_24_marines_vs_12_zealots --seeds 64 --out ~/scratch/zm_wp3/baseline
//! ```
//!
//! # Invariants
//! - Exits with status 2 on a usage error and 1 on any other failure, after printing it.

use sim_fixtures::trace::cli::{run, RunError, TraceArgs};
use std::process::ExitCode;

fn main() -> ExitCode {
    let outcome = TraceArgs::parse(std::env::args().skip(1)).and_then(|args| run(&args));
    match outcome {
        Ok(output) => {
            for result in &output.results {
                let summary = &result.summary;
                println!(
                    "seed {}: end tick {}, survivors blue {} red {}",
                    result.seed, summary.end_tick, summary.survivors[0], summary.survivors[1]
                );
            }
            println!("wrote {}", output.directory.display());
            ExitCode::SUCCESS
        }
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
