//! Plays the crowd scenes of plan `2026-11-29-wall-clearance` (Amendment 1) with the
//! route-clearance meter and writes their figures.
//!
//! # Responsibility
//! The command-line entry point of `sim_fixtures::trace::crowd`, where everything it does lives
//! and is tested; this file only forwards the arguments and prints the table.
//!
//! # How it fits
//! An example of `sim_core`, so it builds against the simulation exactly as its tests do. It names
//! no unit: the scenes use scratch archetypes.
//!
//! ```text
//! cargo run --release -p sim_core --example crowd_trace -- --out ~/scratch/crowd/S
//! cargo run --release -p sim_core --example crowd_trace -- --pipeline legacy --out ~/scratch/crowd/L
//! ```
//!
//! # Invariants
//! - Exits with status 2 on a usage error and 1 on any other failure, after printing it.

use sim_fixtures::harness::HarnessError;
use sim_fixtures::trace::crowd::{parse_crowd_args, run_crowd_trace};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (out, seeds, options) = match parse_crowd_args(&args) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match run_crowd_trace(&out, &seeds, &options) {
        Ok(table) => {
            print!("{table}");
            println!("wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Err(error @ HarnessError::Usage(_)) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("crowd_trace: {error}");
            ExitCode::FAILURE
        }
    }
}
