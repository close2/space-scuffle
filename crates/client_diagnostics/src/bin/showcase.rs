//! Renders the CI showcase video: a storyboarded tour of a real battle (plan
//! `docs/plans/2026-11-05-ci-showcase-video.md`).
//!
//! Usage:
//! - `showcase render --driver <radv|lavapipe> --out <dir> [--revision <sha>] [--revision-date
//!   <YYYY-MM-DD>] [--run-url <url>] [--storyboard <path>]`: every take, then the site. On `radv`
//!   each take runs under `with-gpu-lock`, one lock per take (charter §7).
//! - `showcase take --take <name> --out <dir> --revision <sha> --revision-date <date>
//!   [--storyboard <path>] [--repository <dir>]`: one take on whatever GPU the environment selects.
//! - `showcase assemble --takes <dir> --revision <sha> --revision-date <date> --out <dir>
//!   [--run-url <url>] [--storyboard <path>]`: the site from rendered takes, no GPU.
//!
//! Any failure exits non-zero with its typed reason on stderr. See `client_diagnostics::showcase`.

use clap::{Parser, Subcommand};
use client_diagnostics::showcase::render::{
    assemble, render, AssembleOptions, Driver, RenderOptions,
};
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
    /// Renders every take on a driver, then assembles the site into `<out>/site`.
    Render {
        /// Where the takes render.
        #[arg(long, value_enum)]
        driver: Driver,
        /// New output directory.
        #[arg(long)]
        out: PathBuf,
        /// Storyboard, relative to the assets root unless absolute.
        #[arg(long, default_value = DEFAULT_STORYBOARD)]
        storyboard: PathBuf,
        /// Commit rendered; `git rev-parse HEAD` when absent.
        #[arg(long)]
        revision: Option<String>,
        /// Commit date; the commit's own date when absent.
        #[arg(long)]
        revision_date: Option<String>,
        /// The CI run that renders.
        #[arg(long)]
        run_url: Option<String>,
    },
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
    /// Cuts and encodes the site from rendered takes.
    Assemble {
        /// Storyboard, relative to the assets root unless absolute.
        #[arg(long, default_value = DEFAULT_STORYBOARD)]
        storyboard: PathBuf,
        /// Directory of the takes' mezzanines and manifests.
        #[arg(long)]
        takes: PathBuf,
        /// Commit rendered.
        #[arg(long)]
        revision: String,
        /// Commit date.
        #[arg(long)]
        revision_date: String,
        /// The CI run that rendered.
        #[arg(long)]
        run_url: Option<String>,
        /// New site directory.
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Render {
            driver,
            out,
            storyboard,
            revision,
            revision_date,
            run_url,
        } => render(&RenderOptions {
            driver,
            out,
            storyboard,
            revision,
            revision_date,
            run_url,
        })
        .map(|site| println!("Showcase site: {}", site.display())),
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
        Command::Assemble {
            storyboard,
            takes,
            revision,
            revision_date,
            run_url,
            out,
        } => assemble(&AssembleOptions {
            storyboard,
            takes,
            revision,
            revision_date,
            run_url,
            out,
        })
        .map(|site| println!("Showcase site: {}", site.display())),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("showcase failed: {error}");
            ExitCode::FAILURE
        }
    }
}
