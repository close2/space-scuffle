//! Reproducible GPU captures driven through the normal production client systems.

use bevy::app::AppExit;
use clap::Parser;
use client_diagnostics::runtime_capture::{run_capture, CaptureOptions};

fn main() -> AppExit {
    match run_capture(CaptureOptions::parse()) {
        Ok(exit) => exit,
        Err(error) => {
            eprintln!("Battle capture failed: {error}");
            AppExit::error()
        }
    }
}
