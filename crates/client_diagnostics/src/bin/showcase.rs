//! Renders the CI showcase video: a storyboarded tour of a real battle (plan
//! `docs/plans/2026-11-05-ci-showcase-video.md`).
//!
//! Usage: `showcase take --take <name> --out <dir> --revision <sha> --revision-date <date>
//! [--storyboard <path>] [--repository <dir>]`: one take on whatever GPU the environment selects;
//! wrap it in `with-gpu-lock` on the shared physical GPU (charter §7).
//!
//! Any failure exits non-zero with its typed reason on stderr. See `client_diagnostics::showcase`.

use clap::{Parser, Subcommand};
use client_diagnostics::showcase::take::{run_take, TakeOptions};
use client_diagnostics::showcase::DEFAULT_STORYBOARD;
use std::path::PathBuf;
use std::process::ExitCode;

/// The showcase tool.
#[derive(Parser, Debug)]
#[command(about = "Renders the Space Scuffle showcase video with the production client")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Renders one take into `<out>/<take>.mkv` and `<out>/<take>.manifest.ron`.
    Take {
        /// Storyboard, relative to the assets root unless absolute.
        #[arg(long, default_value = DEFAULT_STORYBOARD)]
        storyboard: PathBuf,
        /// Take name.
        #[arg(long)]
        take: String,
        /// Output directory.
        #[arg(long)]
        out: PathBuf,
        /// Commit rendered, shown in the footer.
        #[arg(long)]
        revision: String,
        /// Commit date, `YYYY-MM-DD`, shown in the footer.
        #[arg(long)]
        revision_date: String,
        /// Repository root with `DESIGN.md`; defaults to the parent of the assets root.
        #[arg(long)]
        repository: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Take {
            storyboard,
            take,
            out,
            revision,
            revision_date,
            repository,
        } => run_take(&TakeOptions {
            storyboard,
            take,
            out,
            revision,
            revision_date,
            repository,
        })
        .map(|manifest| println!("Take manifest: {}", manifest.display())),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("showcase failed: {error}");
            ExitCode::FAILURE
        }
    }
}
