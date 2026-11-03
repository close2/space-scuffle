//! Measures the real client's frame time at 0, 200 or 400 units and writes a report.
//!
//! Usage: `frame_budget --display-path <kwin-virtual|xvfb-software-present|desktop>
//! --work-dir <dir> --scenario <0|200|400> --out <report.ron> [--resolution <W>x<H>]
//! [--render-diagnostics] [--spec <spec.ron>] [--source-assets <dir>] [--measure-frames <n>]
//! [--warmup-frames <n>]`. The display path is checked against the environment before anything
//! runs. `--resolution` and `--render-diagnostics` are for attribution runs, the last two
//! options for smoke runs only. The report is RON; a Markdown summary goes to stdout.
//! Any failure exits non-zero with its typed reason on stderr. See
//! `client_diagnostics::frame_budget` for what a run does.

use clap::Parser;
use client_diagnostics::frame_budget::{run, FrameBudgetOptions};
use std::process::ExitCode;

fn main() -> ExitCode {
    let options = FrameBudgetOptions::parse();
    match run(&options) {
        Ok(report) => {
            println!("{}", report.markdown());
            println!("Report: {}", options.out.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("frame_budget failed: {error}");
            ExitCode::FAILURE
        }
    }
}
