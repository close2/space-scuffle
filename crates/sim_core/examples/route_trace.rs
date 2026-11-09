//! Plays a scenario battle and measures how directly its units follow their routes.
//!
//! # Responsibility
//! The command-line entry point of the route-quality harness of plan `2026-11-09-sc2-pathfinding`
//! (WP1, §3.1). Everything it does lives in `sim_fixtures::trace::route`, where it is tested; this
//! file only forwards the arguments and prints the summary.
//!
//! # How it fits
//! An example of `sim_core`, so it builds against the simulation exactly as the game does. The
//! scenario starts through `sim_core::scenario::load_scenario_battle`, the same initial orders the
//! client gives. It names no unit: the scenario and the measuring boxes are arguments.
//!
//! ```text
//! cargo run --release --locked -p sim_core --example route_trace -- \
//!     --scenario scenarios/showcase.ron --ticks 1500 --out ~/scratch/route/showcase
//! ```
//!
//! # Invariants
//! - Exits with status 2 on a usage error and 1 on any other failure, after printing it.

use sim_fixtures::trace::route::{
    run_route_trace, summary_markdown, RouteTraceArgs, RouteTraceError,
};
use std::process::ExitCode;

fn main() -> ExitCode {
    let outcome = RouteTraceArgs::parse(std::env::args().skip(1)).and_then(|args| {
        let output = run_route_trace(&args)?;
        print!("{}", summary_markdown(&output));
        println!("wrote {}", args.out.display());
        Ok(())
    });
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error @ RouteTraceError::Usage(_)) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("route_trace: {error}");
            ExitCode::FAILURE
        }
    }
}
