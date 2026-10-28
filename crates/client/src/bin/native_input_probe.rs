//! Runs passive native-input evidence against the actual production client.

use bevy::app::AppExit;
use clap::Parser;

fn main() -> AppExit {
    match client::native_input_probe::run(client::native_input_probe::ProbeOptions::parse()) {
        Ok(exit) => exit,
        Err(error) => {
            eprintln!("Native input probe failed: {error}");
            AppExit::error()
        }
    }
}
