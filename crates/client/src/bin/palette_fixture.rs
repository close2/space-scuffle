//! Prepares a separately serialized sparse-owner production capture input.
//! Native verification still requires the actual `battle_capture` renderer afterwards.

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Options {
    /// Fresh directory for the real remapped map and scenario files.
    #[arg(long)]
    out: PathBuf,
}

fn main() -> std::process::ExitCode {
    let options = Options::parse();
    let result = client::ResolvedAssetsRoot::resolve()
        .map_err(|error| error.to_string())
        .and_then(|root| {
            client::player_presentation::fixture::prepare(root.as_path(), &options.out)
                .map_err(|error| error.to_string())
        });
    match result {
        Ok(fixture) => {
            println!("{fixture:#?}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Sparse ownership fixture failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
