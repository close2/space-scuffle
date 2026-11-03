//! The `frame_budget` tool: frame time of the real client at a fixed number of units.
//!
//! # Responsibility
//! Owns the measurement of remediation Phase 6's exit criterion "frame budget measured at 200
//! and 400 units" (plan `docs/plans/2026-11-03-close-phase-6-7-8-exits.md`, decision D-4, work
//! package WP5). One run:
//! 1. reads the spec ([`FrameBudgetSpec`], `crates/client_diagnostics/data/frame_budget.ron`);
//! 2. builds the measurement's assets root under `--work-dir` ([`fixture`]);
//! 3. points `SPACE_SCUFFLE_ASSETS` at that root and builds the production client with
//!    `client::build_graphical_app(ClientMode::LiveBattle, ..)` on the chosen scenario, with
//!    `PresentMode::AutoNoVsync` so the frame interval reflects work, not the display;
//! 4. adds the recorder ([`recorder`]), which holds the camera on the spec's pose, waits for the
//!    battle to be ready, renders the warm-up frames, then measures every frame;
//! 5. writes the report ([`report`]) as RON to `--out` and returns it for the Markdown summary.
//!
//! # How it fits
//! The tool lives outside the shipped client (decision D3 of the remediation plan): the `client`
//! binary gains no flag, and the scenario is placed by the production scenario path from files
//! in a copied assets root. CI runs a short smoke run (`--warmup-frames` and `--measure-frames`)
//! through the GPU harness (`crates/client_diagnostics/tests/frame_budget_tests.rs`); the real
//! measurement runs on the RX 580 (WP6).
//!
//! The scenarios have eight players with 0, 25 or 50 units each, not two with 100 or 200 as the
//! plan proposed: the production placement fits at most 96 units around a 5×5 town hall
//! (`docs/questions/open/0018-frame-budget-army-layout.md`).
//!
//! # Invariants
//! - No unit, structure or loadout is named in Rust: the spec names the source loadout.
//! - The run fails with a typed error, never a partial report, when the battle fails, the match
//!   ends early, the scenario has the wrong number of units or too few units are in view.
//! - A report file is never overwritten.

pub mod fixture;
pub mod recorder;
pub mod report;

use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};
use clap::Parser;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The spec schema version this build reads.
pub const SPEC_FORMAT_VERSION: u32 = 1;

/// Variable through which the client finds its assets root.
const ASSETS_VARIABLE: &str = "SPACE_SCUFFLE_ASSETS";

/// Command line of the `frame_budget` binary.
#[derive(Parser, Debug, Clone)]
#[command(name = "frame_budget")]
#[command(about = "Measures the real client's frame time at a fixed number of units")]
pub struct FrameBudgetOptions {
    /// Measurement spec (RON).
    #[arg(long, default_value_os_t = FrameBudgetSpec::default_path())]
    pub spec: PathBuf,
    /// The real assets root the measurement's root is copied from. Defaults to the `assets/`
    /// directory of the source tree the tool was built from, wherever it is run.
    #[arg(long, default_value_os_t = default_source_assets())]
    pub source_assets: PathBuf,
    /// Directory that receives the measurement's assets root, `<work-dir>/root`. It must lie
    /// outside the repository's `assets/`.
    #[arg(long)]
    pub work_dir: PathBuf,
    /// Scenario name from the spec, such as `0`, `200` or `400`.
    #[arg(long)]
    pub scenario: String,
    /// Report file to create (RON); an existing file is refused.
    #[arg(long)]
    pub out: PathBuf,
    /// Smoke runs only: measure this many frames instead of the spec's tick window.
    #[arg(long)]
    pub measure_frames: Option<u32>,
    /// Smoke runs only: render this many warm-up frames instead of the spec's. A software
    /// rasteriser renders a 400-unit frame in seconds, so a smoke run on it cannot afford the
    /// spec's warm-up within the GPU harness's run limit.
    #[arg(long)]
    pub warmup_frames: Option<u32>,
}

/// The arena map the fixture generates.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArenaSpec {
    /// Map file, relative to the assets root.
    pub file: PathBuf,
    /// Map title.
    pub title: String,
    /// Map description.
    pub description: String,
    /// Width in cells.
    pub width: u32,
    /// Height in cells.
    pub height: u32,
    /// Terrain theme slug.
    pub theme: String,
    /// Start footprint `(width, height)` in cells; it must hold the source loadout's town hall.
    pub start_footprint: (u8, u8),
}

/// One player of every scenario.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSpec {
    /// Player slot, 0 to 7.
    pub slot: u8,
    /// Top-left cell of the player's start footprint.
    pub start_cell: (u32, u32),
    /// Slot whose start this player's units attack-move to.
    pub target_slot: u8,
}

/// One measured scenario.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioSpec {
    /// Name used on the command line and in file names: ASCII letters, digits, `_` and `-`.
    pub name: String,
    /// Mobile units each player starts with.
    pub units_per_player: u32,
}

/// The fixed camera pose, in world units (one unit is one grid cell).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraSpec {
    /// Ground point `(x, z)` the camera looks at.
    pub target: (f32, f32),
    /// Distance from the target along the view direction.
    pub distance: f32,
}

/// The measurement spec, `crates/client_diagnostics/data/frame_budget.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameBudgetSpec {
    /// Schema version, [`SPEC_FORMAT_VERSION`].
    pub format_version: u32,
    /// Slug of the shipped start loadout whose unit mix is scaled.
    pub source_loadout: String,
    /// Seed of every scenario.
    pub seed: u64,
    /// The arena map.
    pub map: ArenaSpec,
    /// Players of every scenario.
    pub players: Vec<PlayerSpec>,
    /// Measured scenarios.
    pub scenarios: Vec<ScenarioSpec>,
    /// Fixed camera pose.
    pub camera: CameraSpec,
    /// Frames rendered, battle paused, between readiness and measurement.
    pub warmup_frames: u32,
    /// Simulation ticks of the measured window.
    pub measure_ticks: u32,
    /// Ticks of the combat window.
    pub combat_window_ticks: u32,
    /// Smallest share of live units in view on every measured frame, in `(0, 1]`.
    pub min_in_view_fraction: f64,
    /// Rows preallocated for the measured window.
    pub max_recorded_frames: u32,
}

/// Why a frame-budget run failed.
#[derive(Debug, thiserror::Error)]
pub enum FrameBudgetError {
    /// The spec could not be read.
    #[error("frame-budget spec {path}: {source}")]
    SpecIo {
        /// Spec file.
        path: PathBuf,
        /// The failure.
        #[source]
        source: std::io::Error,
    },
    /// The spec could not be parsed.
    #[error("frame-budget spec {path}: {source}")]
    SpecParse {
        /// Spec file.
        path: PathBuf,
        /// The parse failure.
        #[source]
        source: ron::error::SpannedError,
    },
    /// The spec breaks a rule of its schema.
    #[error("invalid frame-budget spec: {0}")]
    InvalidSpec(String),
    /// The requested scenario is not in the spec.
    #[error("scenario {0} is not in the spec")]
    UnknownScenario(String),
    /// The report file already exists.
    #[error("report {0} already exists; reports are never overwritten")]
    OutputExists(PathBuf),
    /// The real assets root could not be found.
    #[error(transparent)]
    Assets(#[from] client::AssetsDirError),
    /// The measurement's assets root could not be built.
    #[error(transparent)]
    Fixture(#[from] fixture::FixtureError),
    /// The client could not start.
    #[error(transparent)]
    Startup(#[from] client::ClientStartupError),
    /// The client has no primary window.
    #[error("the client has no primary window")]
    NoWindow,
    /// The measurement failed while the client ran.
    #[error(transparent)]
    Recording(#[from] recorder::RecordingError),
    /// The client exited before the measured window ended.
    #[error("the client exited ({0:?}) before the measured window ended")]
    ExitedEarly(AppExit),
    /// The report could not be built or written.
    #[error(transparent)]
    Report(#[from] report::ReportError),
}

impl FrameBudgetSpec {
    /// The spec shipped with this crate, resolved from the crate's source directory.
    #[must_use]
    pub fn default_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("data/frame_budget.ron")
    }

    /// Reads and validates the spec at `path`.
    ///
    /// # Errors
    /// [`FrameBudgetError::SpecIo`], [`FrameBudgetError::SpecParse`] or
    /// [`FrameBudgetError::InvalidSpec`].
    pub fn load(path: &Path) -> Result<Self, FrameBudgetError> {
        let text = std::fs::read_to_string(path).map_err(|source| FrameBudgetError::SpecIo {
            path: path.to_owned(),
            source,
        })?;
        let spec: Self = ron::from_str(&text).map_err(|source| FrameBudgetError::SpecParse {
            path: path.to_owned(),
            source,
        })?;
        spec.validate()?;
        Ok(spec)
    }

    /// Checks the rules the schema cannot express.
    ///
    /// # Errors
    /// [`FrameBudgetError::InvalidSpec`] naming the first broken rule.
    pub fn validate(&self) -> Result<(), FrameBudgetError> {
        let invalid = |reason: String| Err(FrameBudgetError::InvalidSpec(reason));
        if self.format_version != SPEC_FORMAT_VERSION {
            return invalid(format!("format_version {}", self.format_version));
        }
        let slots: std::collections::BTreeSet<u8> =
            self.players.iter().map(|player| player.slot).collect();
        if self.players.is_empty() || slots.len() != self.players.len() {
            return invalid("players must be non-empty with distinct slots".into());
        }
        for player in &self.players {
            if player.target_slot == player.slot || !slots.contains(&player.target_slot) {
                return invalid(format!(
                    "player {} has no other player as target",
                    player.slot
                ));
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for scenario in &self.scenarios {
            let safe = !scenario.name.is_empty()
                && scenario
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            if !safe || !names.insert(scenario.name.as_str()) {
                return invalid(format!(
                    "scenario name {:?} is unsafe or repeated",
                    scenario.name
                ));
            }
            if scenario
                .units_per_player
                .checked_mul(self.player_count())
                .is_none()
            {
                return invalid(format!("scenario {} has too many units", scenario.name));
            }
        }
        let (x, z) = self.camera.target;
        if !(x.is_finite() && z.is_finite() && self.camera.distance.is_finite())
            || self.camera.distance <= 0.0
        {
            return invalid("camera pose must be finite with a positive distance".into());
        }
        if !(self.min_in_view_fraction > 0.0 && self.min_in_view_fraction <= 1.0) {
            return invalid("min_in_view_fraction must be in (0, 1]".into());
        }
        if self.measure_ticks == 0 || self.max_recorded_frames == 0 {
            return invalid("measure_ticks and max_recorded_frames must be positive".into());
        }
        Ok(())
    }

    /// The scenario called `name`.
    ///
    /// # Errors
    /// [`FrameBudgetError::UnknownScenario`].
    pub fn scenario(&self, name: &str) -> Result<&ScenarioSpec, FrameBudgetError> {
        self.scenarios
            .iter()
            .find(|scenario| scenario.name == name)
            .ok_or_else(|| FrameBudgetError::UnknownScenario(name.to_owned()))
    }

    /// Number of players, which [`Self::validate`] bounds by the eight player slots.
    #[must_use]
    pub fn player_count(&self) -> u32 {
        u32::try_from(self.players.len()).unwrap_or(u32::MAX)
    }
}

/// Runs one measurement and writes its report to `options.out`.
///
/// # Errors
/// Every [`FrameBudgetError`]: a bad spec or scenario, an existing report, a fixture or startup
/// failure, a failed measurement, or an early exit.
pub fn run(options: &FrameBudgetOptions) -> Result<report::FrameBudgetReport, FrameBudgetError> {
    let load_before = read_load_average();
    let spec = FrameBudgetSpec::load(&options.spec)?;
    let scenario = spec.scenario(&options.scenario)?.clone();
    if options.out.exists() {
        return Err(FrameBudgetError::OutputExists(options.out.clone()));
    }
    let source = client::ResolvedAssetsRoot::from_directory(&options.source_assets)?;
    let root = fixture::build_fixture_root(&spec, source.as_path(), &options.work_dir)?;
    // The client resolves its root only from this variable or by discovery
    // (`client::ResolvedAssetsRoot::resolve`). The process is still single-threaded here.
    std::env::set_var(ASSETS_VARIABLE, &root);
    let mut app = client::build_graphical_app(
        client::ClientMode::LiveBattle,
        None,
        &fixture::scenario_path(&scenario.name),
    )?;
    configure_window(&mut app)?;
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_paused = true;
    app.insert_resource(client::media::MediaFailurePolicy::new(true));
    let expected_units = scenario.units_per_player * spec.player_count();
    let warmup_frames = options.warmup_frames.unwrap_or(spec.warmup_frames);
    let window = match options.measure_frames {
        Some(frames) => recorder::MeasurementWindow::Frames(frames),
        None => recorder::MeasurementWindow::Ticks(spec.measure_ticks),
    };
    let handle = recorder::install(
        &mut app,
        recorder::RecorderConfig {
            expected_units,
            warmup_frames,
            window,
            camera_target: Vec2::new(spec.camera.target.0, spec.camera.target.1),
            camera_distance: spec.camera.distance,
            min_in_view_fraction: spec.min_in_view_fraction,
            capacity: usize::try_from(spec.max_recorded_frames).unwrap_or(usize::MAX),
        },
    );
    let exit = client::render_compatibility::run_render_app(&mut app);
    let recorded = handle
        .take()
        .ok_or(FrameBudgetError::ExitedEarly(exit.clone()))??;
    if !exit.is_success() {
        return Err(FrameBudgetError::ExitedEarly(exit));
    }
    let environment = report::RunEnvironment {
        commit: source_commit(),
        adapter: recorded.adapter,
        driver: recorded.driver,
        cpus: std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
        load_before,
        load_after: recorded.load_after,
        resolution: recorded.resolution,
        present_mode: recorded.present_mode,
        graphics_environment: graphics_environment(),
    };
    let report = report::FrameBudgetReport::new(
        report::ReportInputs {
            scenario: scenario.name,
            expected_units,
            warmup_frames,
            window: window.to_string(),
            combat_window_ticks: spec.combat_window_ticks,
            environment,
        },
        recorded.samples,
    )?;
    report.save(&options.out)?;
    Ok(report)
}

/// The `assets/` directory of the source tree this tool was built from.
#[must_use]
pub fn default_source_assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// Sets the primary window to the client's default size at scale factor 1 and turns vsync off.
fn configure_window(app: &mut App) -> Result<(), FrameBudgetError> {
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>();
    let mut window = query
        .get_single_mut(app.world_mut())
        .map_err(|_| FrameBudgetError::NoWindow)?;
    window.resolution.set_scale_factor_override(Some(1.0));
    window.present_mode = PresentMode::AutoNoVsync;
    window.title = "Space Scuffle — frame budget".into();
    Ok(())
}

/// `/proc/loadavg`, or why it could not be read.
#[must_use]
pub fn read_load_average() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .unwrap_or_else(|error| format!("unavailable: {error}"))
        .trim()
        .to_owned()
}

/// The commit of the tool's source tree, or why it is unavailable.
fn source_commit() -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output();
    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        Ok(output) => format!(
            "unavailable: git exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(error) => format!("unavailable: {error}"),
    }
}

/// The graphics variables that change what a run measures, as the process saw them.
fn graphics_environment() -> Vec<(String, String)> {
    ["MESA_VK_WSI_DEBUG", "VK_ICD_FILENAMES", "WGPU_BACKEND"]
        .into_iter()
        .map(|name| {
            let value = std::env::var(name).unwrap_or_else(|_| "unset".into());
            (name.to_owned(), value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_spec_is_valid_and_names_its_three_scenarios() -> Result<(), FrameBudgetError> {
        let spec = FrameBudgetSpec::load(&FrameBudgetSpec::default_path())?;
        let totals: Vec<(String, u32)> = spec
            .scenarios
            .iter()
            .map(|scenario| {
                (
                    scenario.name.clone(),
                    scenario.units_per_player * spec.player_count(),
                )
            })
            .collect();
        assert_eq!(
            totals,
            vec![("0".into(), 0), ("200".into(), 200), ("400".into(), 400)]
        );
        assert!(matches!(
            spec.scenario("300"),
            Err(FrameBudgetError::UnknownScenario(_))
        ));
        Ok(())
    }

    #[test]
    fn invalid_specs_are_refused() -> Result<(), FrameBudgetError> {
        let spec = FrameBudgetSpec::load(&FrameBudgetSpec::default_path())?;
        let mut broken = spec.clone();
        broken.players[0].target_slot = broken.players[0].slot;
        assert!(broken.validate().is_err());
        let mut broken = spec.clone();
        broken.players[1].slot = broken.players[0].slot;
        assert!(broken.validate().is_err());
        let mut broken = spec.clone();
        broken.scenarios[0].name = "../escape".into();
        assert!(broken.validate().is_err());
        let mut broken = spec.clone();
        broken.scenarios[1].name = broken.scenarios[0].name.clone();
        assert!(broken.validate().is_err());
        let mut broken = spec.clone();
        broken.camera.distance = f32::NAN;
        assert!(broken.validate().is_err());
        let mut broken = spec.clone();
        broken.min_in_view_fraction = 0.0;
        assert!(broken.validate().is_err());
        let mut broken = spec;
        broken.measure_ticks = 0;
        assert!(broken.validate().is_err());
        Ok(())
    }
}
