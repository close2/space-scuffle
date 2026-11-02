//! Space Scuffle client application entry point.
//!
//! # Architectural Charter
//! Provides the command-line entrypoint for the Bevy 3D renderer and Bubble Visual Lab.
//! Supports flags for interactive visual inspection, the playable skirmish, the live
//! deterministic battle sandbox, swarm stress testing, and headless CI verification.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::PathBuf;

use clap::Parser;
use client::runtime_battle::{DEFAULT_BATTLE_SCENARIO, SKIRMISH_SCENARIO};
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

    /// Boot into the developer sandbox: a live 2-army `sim_core` battle with the full inspector.
    #[arg(long, default_value_t = false)]
    pub battle: bool,

    /// Play the skirmish as one side. Takes precedence over `--battle`.
    #[arg(long, default_value_t = false)]
    pub skirmish: bool,

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
    /// Defaults to the skirmish under `--skirmish` and to the default battle otherwise.
    #[arg(long)]
    pub scenario: Option<PathBuf>,

    /// Optional output file path for capturing a full-resolution PNG screenshot on `max_frames`.
    #[arg(long)]
    pub screenshot_out: Option<PathBuf>,
}

impl CliArgs {
    /// The launch mode these flags select. `--headless-check` outranks every other flag.
    fn mode(&self) -> ClientMode {
        if self.headless_check {
            ClientMode::HeadlessCheck
        } else if self.skirmish {
            ClientMode::Skirmish
        } else if self.battle {
            ClientMode::LiveBattle
        } else if self.swarm {
            ClientMode::SwarmStressTest
        } else {
            ClientMode::VisualLab
        }
    }

    /// The scenario to load, relative to the assets root. An explicit `--scenario` always wins.
    fn scenario_path(&self) -> PathBuf {
        self.scenario.clone().unwrap_or_else(|| {
            PathBuf::from(if self.skirmish {
                SKIRMISH_SCENARIO
            } else {
                DEFAULT_BATTLE_SCENARIO
            })
        })
    }
}

fn main() -> bevy::app::AppExit {
    let args = CliArgs::parse();
    let mode = args.mode();
    let scenario = args.scenario_path();
    run_app_with_options(mode, args.max_frames, &scenario, args.screenshot_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), clap::Error>;

    fn parse(flags: &[&str]) -> Result<CliArgs, clap::Error> {
        CliArgs::try_parse_from(std::iter::once(&"client").chain(flags))
    }

    #[test]
    fn skirmish_selects_its_mode_and_scenario() -> TestResult {
        let args = parse(&["--skirmish"])?;
        assert_eq!(args.mode(), ClientMode::Skirmish);
        assert_eq!(args.scenario_path(), PathBuf::from(SKIRMISH_SCENARIO));
        Ok(())
    }

    #[test]
    fn skirmish_takes_precedence_over_battle() -> TestResult {
        let args = parse(&["--battle", "--skirmish"])?;
        assert_eq!(args.mode(), ClientMode::Skirmish);
        assert_eq!(args.scenario_path(), PathBuf::from(SKIRMISH_SCENARIO));
        Ok(())
    }

    #[test]
    fn battle_keeps_the_default_scenario() -> TestResult {
        let args = parse(&["--battle"])?;
        assert_eq!(args.mode(), ClientMode::LiveBattle);
        assert_eq!(args.scenario_path(), PathBuf::from(DEFAULT_BATTLE_SCENARIO));
        Ok(())
    }

    #[test]
    fn an_explicit_scenario_wins_in_either_mode() -> TestResult {
        let chosen = "scenarios/chosen.ron";
        for flags in [
            ["--skirmish", "--scenario", chosen],
            ["--battle", "--scenario", chosen],
        ] {
            assert_eq!(parse(&flags)?.scenario_path(), PathBuf::from(chosen));
        }
        Ok(())
    }

    #[test]
    fn headless_check_outranks_skirmish() -> TestResult {
        let args = parse(&["--skirmish", "--headless-check"])?;
        assert_eq!(args.mode(), ClientMode::HeadlessCheck);
        Ok(())
    }
}
