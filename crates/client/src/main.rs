//! Space Scuffle client application entry point.
//!
//! # Architectural Charter
//! Provides the command-line entrypoint for the Bevy 3D renderer and Bubble Visual Lab.
//! Supports flags for interactive visual inspection, live deterministic battle sandbox,
//! swarm stress testing, and headless CI verification.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use clap::Parser;
use client::{run_app_with_options, ClientMode};

/// Command-line arguments configuring the client launch mode.
#[derive(Parser, Debug)]
#[command(name = "client")]
#[command(about = "Space Scuffle Bevy 3D Client & Bubble Visual Lab")]
#[command(version)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent user-facing toggle"
)]
pub struct CliArgs {
    /// Boot into the Interactive Bubble & Dome Visual Lab inspector (default).
    #[arg(long, default_value_t = false)]
    pub lab: bool,

    /// Boot into the live 2-army deterministic `sim_core` combat sandbox.
    #[arg(long, default_value_t = false)]
    pub battle: bool,

    /// Boot into the 200-bubble swarm readability stress test.
    #[arg(long, default_value_t = false)]
    pub swarm: bool,

    /// Run 60 frames in headless CI mode and exit cleanly with code 0.
    #[arg(long, default_value_t = false)]
    pub headless_check: bool,

    /// Exit cleanly after executing the specified number of rendered frames (for GPU CI and automated verification).
    #[arg(long)]
    pub max_frames: Option<u64>,

    /// Declarative battle scenario, relative to the selected assets root.
    #[arg(long, default_value = client::runtime_battle::DEFAULT_BATTLE_SCENARIO)]
    pub scenario: std::path::PathBuf,

    /// Optional output file path for capturing a full-resolution PNG screenshot on `max_frames`.
    #[arg(long)]
    pub screenshot_out: Option<std::path::PathBuf>,
}

fn main() -> bevy::app::AppExit {
    let args = CliArgs::parse();

    let mode = if args.headless_check {
        ClientMode::HeadlessCheck
    } else if args.battle {
        ClientMode::LiveBattle
    } else if args.swarm {
        ClientMode::SwarmStressTest
    } else {
        ClientMode::VisualLab
    };

    run_app_with_options(mode, args.max_frames, &args.scenario, args.screenshot_out)
}
