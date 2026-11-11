//! The `frame_budget` tool: frame time of the real client at a fixed number of units.
//!
//! # Responsibility
//! Owns the measurement of remediation Phase 6's exit criterion "frame budget measured at 200
//! and 400 units" (plan `docs/plans/2026-11-03-close-phase-6-7-8-exits.md`, decision D-4, work
//! package WP5). One run:
//! 0. checks the declared `--display-path` against the environment ([`display`], `WP5b`), before
//!    anything is read or written;
//! 1. reads the spec ([`FrameBudgetSpec`], `crates/client_diagnostics/data/frame_budget.ron`);
//! 2. builds the measurement's assets root under `--work-dir` ([`fixture`]);
//! 3. points `SPACE_SCUFFLE_ASSETS` at that root and builds the production client with
//!    `client::build_graphical_app(ClientMode::LiveBattle, ..)` on the chosen scenario, with
//!    `PresentMode::AutoNoVsync` so the frame interval reflects work, not the display, at the
//!    `--resolution` asked for (default 1280×720), and with Bevy's `RenderDiagnosticsPlugin`
//!    when `--render-diagnostics` asks for per-pass GPU timings (attribution runs only);
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
//! - A run whose environment contradicts its declared display path writes nothing.

pub mod display;
pub mod effect_stress;
pub mod fixture;
pub mod recorder;
pub mod report;

use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};
use clap::Parser;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::str::FromStr;

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
    /// Where the frames are presented. Checked against the environment before anything else
    /// happens, and recorded in the report.
    #[arg(long, value_enum)]
    pub display_path: display::DisplayPath,
    /// Window size in physical pixels, `<width>x<height>`, each side from 320 to 7680.
    #[arg(long, default_value_t = Resolution::default())]
    pub resolution: Resolution,
    /// Attribution runs only: add Bevy's `RenderDiagnosticsPlugin` and report the mean GPU time
    /// of each pass it instruments. Timestamp queries and their readback add work, so such a
    /// run is never a figure of record.
    #[arg(long)]
    pub render_diagnostics: bool,
    /// Effect stress mode (plan `2026-11-09-effect-visualization` §10.3): this many splash
    /// events per second of game time over the battle, round-robin over every weapon whose
    /// presentation block names a splash effect ([`effect_stress`]).
    #[arg(long, value_name = "PER_SECOND")]
    pub effect_stress: Option<u32>,
    /// Screenshot (PNG) of the frame at the middle of the window (the first frame of a
    /// `--measure-frames` run), with the effect coverage of that frame in the report. The
    /// capture's readback adds work to one frame.
    #[arg(long, value_name = "PNG")]
    pub capture: Option<PathBuf>,
    /// Simulation tick of the `--capture` frame, instead of the middle of the window (the
    /// fight thins out as it goes on; an early tick shows the crowd).
    #[arg(long, value_name = "TICK")]
    pub capture_tick: Option<u32>,
    /// Screenshots of this many consecutive frames from the `--capture` frame on (default 1):
    /// frame `k > 0` is written beside it as `<stem>_<k>.png`, so a budget retirement or a
    /// pool at its cap can be followed frame by frame.
    #[arg(long, value_name = "FRAMES")]
    pub capture_frames: Option<u32>,
    /// Review captures only: advance game time by exactly this many milliseconds per frame,
    /// whatever the frame took, so consecutive captures are a fixed step apart (40 is one tick
    /// of the 25 Hz simulation). Frame times are then not real time, so such a run is never a
    /// figure of record.
    #[arg(long, value_name = "MS")]
    pub frame_step_ms: Option<u32>,
    /// Review captures only: the camera's distance in world units instead of the spec's (the
    /// battle camera's default is 22; the spec's wide view culls small effect layers by their
    /// size on screen), looking at the spec's target. It frames only part of the battle, so
    /// the check that every unit is in view is waived. Never a figure of record.
    #[arg(long, value_name = "UNITS")]
    pub camera_distance: Option<f32>,
    /// The plan's D-11 experiment: an HDR camera with a bloom only values above 1.0 feed
    /// ([`crate::hdr_bloom`]).
    #[arg(long)]
    pub hdr_bloom: bool,
}

/// Smallest window side `--resolution` accepts, in physical pixels.
pub const MIN_RESOLUTION_SIDE: u32 = 320;
/// Largest window side `--resolution` accepts, in physical pixels.
pub const MAX_RESOLUTION_SIDE: u32 = 7680;

/// A window size in physical pixels, written `<width>x<height>` (such as `1280x720`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
}

impl Default for Resolution {
    /// The client's default window, 1280×720.
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
        }
    }
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}x{}", self.width, self.height)
    }
}

/// Why a `--resolution` value was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolutionError {
    /// The value is not two integers joined by `x`.
    #[error("resolution {0:?} is not <width>x<height>, such as 1280x720")]
    Format(String),
    /// A side lies outside the accepted range.
    #[error(
        "resolution {0:?} has a side outside {MIN_RESOLUTION_SIDE} to {MAX_RESOLUTION_SIDE} pixels"
    )]
    OutOfRange(String),
}

impl FromStr for Resolution {
    type Err = ResolutionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let format = || ResolutionError::Format(text.to_owned());
        let (width, height) = text.split_once('x').ok_or_else(format)?;
        let side = |value: &str| value.parse::<u32>().map_err(|_| format());
        let (width, height) = (side(width)?, side(height)?);
        let range = MIN_RESOLUTION_SIDE..=MAX_RESOLUTION_SIDE;
        if !(range.contains(&width) && range.contains(&height)) {
            return Err(ResolutionError::OutOfRange(text.to_owned()));
        }
        Ok(Self { width, height })
    }
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
    /// The declared display path contradicts the environment.
    #[error(transparent)]
    DisplayPath(#[from] display::DisplayPathError),
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
    /// `--effect-stress` was asked for, but no unit's presentation block names a splash effect.
    #[error("--effect-stress: no weapon of the unit catalog names a splash effect")]
    NoStressSource,
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
    display::check_display_path(options.display_path, |name| {
        std::env::var_os(name).map(|value| value.to_string_lossy().into_owned())
    })?;
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
    configure_window(&mut app, options.resolution)?;
    if options.render_diagnostics {
        app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
    }
    let stressed = add_variant(&mut app, options, &spec);
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
            camera_distance: options
                .camera_distance
                .filter(|distance| distance.is_finite() && *distance > 0.0)
                .unwrap_or(spec.camera.distance),
            // A closer review camera frames only part of the battle, so it waives the check
            // that the whole army is in view.
            min_in_view_fraction: if options.camera_distance.is_some() {
                0.0
            } else {
                spec.min_in_view_fraction
            },
            capacity: usize::try_from(spec.max_recorded_frames).unwrap_or(usize::MAX),
            gpu_passes: options.render_diagnostics,
        },
    );
    let exit = client::render_compatibility::run_render_app(&mut app);
    let recorded = handle
        .take()
        .ok_or(FrameBudgetError::ExitedEarly(exit.clone()))??;
    if !exit.is_success() {
        return Err(FrameBudgetError::ExitedEarly(exit));
    }
    let effect_stress = stress_summary(options, stressed.as_ref())?;
    let environment = report::RunEnvironment {
        commit: source_commit(),
        adapter: recorded.adapter,
        driver: recorded.driver,
        cpus: std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
        load_before,
        load_after: recorded.load_after,
        resolution: recorded.resolution,
        present_mode: recorded.present_mode,
        display_path: options.display_path,
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
            gpu_passes: recorded.gpu_passes,
            variant: report::RunVariant {
                hdr_bloom: options.hdr_bloom,
                effect_stress,
            },
        },
        recorded.samples,
    )?;
    report.save(&options.out)?;
    Ok(report)
}

/// Adds the D-11 camera and the effect stress mode to `app` as `options` ask; returns the
/// stress mode's handle if it (or a capture) was installed.
fn add_variant(
    app: &mut App,
    options: &FrameBudgetOptions,
    spec: &FrameBudgetSpec,
) -> Option<effect_stress::StressHandle> {
    if options.hdr_bloom {
        app.add_plugins(crate::hdr_bloom::HdrBloomPlugin);
    }
    if let Some(step) = options.frame_step_ms {
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(u64::from(step)),
        ));
    }
    let stressed = options.effect_stress.is_some() || options.capture.is_some();
    stressed.then(|| {
        effect_stress::install(
            app,
            effect_stress::StressConfig {
                rate_per_second: options.effect_stress.unwrap_or(0),
                capture: options.capture.clone(),
                capture_frames: options.capture_frames.unwrap_or(1).max(1),
                capture_tick: options.capture_tick.unwrap_or_else(|| {
                    options.measure_frames.map_or(spec.measure_ticks / 2, |_| 0)
                }),
            },
        )
    })
}

/// The stress mode's report section, if it was installed.
///
/// # Errors
/// [`FrameBudgetError::NoStressSource`] when a stream was asked for and no weapon names a
/// splash effect.
fn stress_summary(
    options: &FrameBudgetOptions,
    handle: Option<&effect_stress::StressHandle>,
) -> Result<Option<effect_stress::StressSummary>, FrameBudgetError> {
    let Some(handle) = handle else {
        return Ok(None);
    };
    let summary = handle.snapshot();
    if options.effect_stress.is_some_and(|rate| rate > 0) && summary.sources.is_empty() {
        return Err(FrameBudgetError::NoStressSource);
    }
    Ok(Some(summary))
}

/// The `assets/` directory of the source tree this tool was built from.
#[must_use]
pub fn default_source_assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// Sets the primary window to `resolution` physical pixels at scale factor 1 and turns vsync off.
fn configure_window(app: &mut App, resolution: Resolution) -> Result<(), FrameBudgetError> {
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>();
    let mut window = query
        .get_single_mut(app.world_mut())
        .map_err(|_| FrameBudgetError::NoWindow)?;
    window.resolution.set_scale_factor_override(Some(1.0));
    window
        .resolution
        .set_physical_resolution(resolution.width, resolution.height);
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
    [
        display::MESA_VK_WSI_DEBUG,
        "VK_ICD_FILENAMES",
        "WGPU_BACKEND",
        display::DISPLAY,
        display::WAYLAND_DISPLAY,
    ]
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

    #[test]
    fn a_resolution_parses_two_sides_in_range() -> Result<(), ResolutionError> {
        assert_eq!(
            "1280x720".parse::<Resolution>()?,
            Resolution {
                width: 1280,
                height: 720
            }
        );
        assert_eq!(
            "640x360".parse::<Resolution>()?,
            Resolution {
                width: 640,
                height: 360
            }
        );
        assert_eq!(Resolution::default().to_string(), "1280x720");
        for refused in [
            "0x720",
            "1280",
            "1280x720x2",
            "8000x720",
            "axb",
            "",
            "319x720",
        ] {
            assert!(refused.parse::<Resolution>().is_err(), "{refused:?}");
        }
        Ok(())
    }
}
