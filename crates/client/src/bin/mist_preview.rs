//! Actual GPU capture entrypoint for production movement-wake visual verification.
//!
//! Uses the real asset loader, terrain renderer, and mist plugin, and fails on shader,
//! screenshot, readiness, or scene errors instead of silently substituting assets.

use bevy::app::AppExit;
use clap::Parser;
use client::mist::capture::{run_capture, CaptureOptions};

fn main() -> AppExit {
    match run_capture(CaptureOptions::parse()) {
        Ok(exit) => exit,
        Err(error) => {
            eprintln!("Mist capture failed: {error}");
            AppExit::error()
        }
    }
}
