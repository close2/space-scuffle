//! Plays and aggregates the movement evaluation harness of plan `2026-11-20-contact-solver` (§6).
//!
//! # Responsibility
//! The command-line entry point of the harness. Everything it does lives in
//! `sim_fixtures::harness`, where it is tested; this file only forwards the arguments and reports
//! the result.
//!
//! # How it fits
//! An example of `sim_core`, so it builds against the simulation exactly as its tests do, with the
//! test-support crate `sim_fixtures` as a dev-dependency. It names no unit: scenes use scratch
//! archetypes and the mirror chooses shipped ones by their properties.
//!
//! ```text
//! # One family's development range, original and relabelled, four threads:
//! cargo run --release -p sim_core --example movement_harness -- \
//!     --family gap --from 1 --count 1000 --jobs 4 --out ~/scratch/run/gap.csv
//! cargo run --release -p sim_core --example movement_harness -- \
//!     --family gap --from 1 --count 1000 --jobs 4 --relabel --out ~/scratch/run/gap-relabel.csv
//! # The tables of §5 and the verdicts of T1 to T9 of every run against the first:
//! cargo run --release -p sim_core --example movement_harness -- \
//!     --aggregate today=~/scratch/run-legacy new=~/scratch/run-solver
//! ```
//!
//! # Invariants
//! - Exits with status 2 on a usage error and 1 on any other failure, after printing it.

use sim_fixtures::harness::cli::{execute, parse};
use sim_fixtures::harness::{HarnessError, RunOptions};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match parse(&args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match execute(&command, &RunOptions::default()) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error @ HarnessError::Usage(_)) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
