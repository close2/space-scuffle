//! Runs `frame_budget` arms interleaved on the shared host and judges them (plan
//! `2026-11-19-retained-effect-instances` D-14).
//!
//! Usage: `frame_budget_series --display-path kwin-virtual --out <series.ron> [--arms B,S,P]
//! [--pairs 6] [--cpus 32-47] [--frame-budget <binary>] [--extra=<option> ...]`. Each child takes
//! the GPU lock itself, one run at a time; never run this inside `with-gpu-lock`. The series
//! report (RON) is rewritten after every run, and a Markdown summary goes to stdout. See
//! `client_diagnostics::frame_budget::series` for the order and the verdict.

use clap::Parser;
use client_diagnostics::frame_budget::series::{markdown, run_series, SeriesOptions};
use std::process::ExitCode;

fn main() -> ExitCode {
    let options = SeriesOptions::parse();
    match run_series(&options) {
        Ok(report) => {
            println!("{}", markdown(&report));
            println!("Series: {}", options.out.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("frame_budget_series failed: {error}");
            ExitCode::FAILURE
        }
    }
}
