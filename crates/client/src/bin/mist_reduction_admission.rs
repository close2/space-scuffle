//! Runs the complete declared CPU wake-reduction corpus without a renderer or editor.
//!
//! Non-success remains an error exit with retained per-window evidence. This binary
//! has no sampled/default-short mode and cannot approve volumetric field quality.

use clap::Parser;
use client::mist::reduction_admission::AdmissionCorpus;
use std::path::PathBuf;

#[derive(Parser)]
struct Arguments {
    /// Exact existing runtime asset root.
    #[arg(long)]
    assets: PathBuf,
    /// Relative authored full-matrix verification specification.
    #[arg(long)]
    spec: PathBuf,
    /// New evidence directory; existing output is rejected.
    #[arg(long)]
    out: PathBuf,
}

fn main() -> std::process::ExitCode {
    let arguments = Arguments::parse();
    match AdmissionCorpus::load(&arguments.assets, &arguments.spec)
        .and_then(|corpus| corpus.run(&arguments.out))
    {
        Ok(summary) => {
            println!("CPU control admission: {} windows, {} actual display frames; field/art remain unaccepted", summary.attempted_windows, summary.display_frames);
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("CPU control admission failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
