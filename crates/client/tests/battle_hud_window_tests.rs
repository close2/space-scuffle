//! The player's HUD on a real window, a real egui pass and the real renderer.
//!
//! # Responsibility
//! `battle_hud_tests.rs` proves the HUD's behaviour without a window. This proves what only a
//! window can show: the production skirmish opens under the right title, with no inspector, and
//! the camera renders exactly the room the panels leave, in physical pixels of the real surface.
//!
//! # How it fits
//! The test re-executes this binary on the virtual GPU. Its child is
//! [`client::build_graphical_app_off_main_thread`], the production assembly, because a child on a
//! libtest thread cannot open a window on the default backend.
//!
//! # Limits
//! This reads geometry, not pixels. Screenshots show egui panels flat on the capture path, so a
//! pixel assertion on a panel would prove nothing in either direction.
//!
//! # Invariants
//! The child reports only from the live application, once its session is ready and a panel has
//! been drawn, and the parent fails if no report arrives.

#![allow(clippy::expect_used, clippy::panic, clippy::cast_precision_loss)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use client::battle_hud::panels::{COMMAND_PANEL_HEIGHT, RESOURCE_BAR_HEIGHT};
use client::battle_hud::HudGeometry;
use client::battle_pipeline::session::BattleSession;
use client::input_policy::UiGeometry;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::{ClientMode, RtsCamera};
use serde::{Deserialize, Serialize};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the file the child reports to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_HUD_CHILD_OUT";
/// Name of the libtest entry the test launches as its child.
const CHILD_TEST: &str = "windowed_hud_child";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// Frames with panels before reporting, so egui and the camera agree on the surface size. Few,
/// because a frame of the shipped map takes seconds on lavapipe. This counts frames, not time, so
/// it holds however short a frame is.
const SETTLE_FRAMES: u32 = 2;

/// What the child saw, in the units each field names.
#[derive(Debug, Serialize, Deserialize)]
struct WindowReport {
    title: String,
    /// Physical pixels of the surface.
    surface: (u32, u32),
    /// Physical pixels per egui point.
    scale: f32,
    /// Bottom edge of the resource bar, in egui points.
    top_bottom: f32,
    /// Top edge of the command panel, in egui points.
    bottom_top: f32,
    inspector_shown: bool,
    /// Physical pixels.
    viewport_position: (u32, u32),
    /// Physical pixels.
    viewport_size: (u32, u32),
}

#[test]
fn the_skirmish_window_gives_the_camera_only_what_the_panels_leave() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let report_path = directory.path().join("report.ron");
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, &report_path);
    command.run("windowed HUD child");

    let report: WindowReport = ron::from_str(&std::fs::read_to_string(&report_path)?)?;
    assert_eq!(report.title, "Space Scuffle");
    assert!(!report.inspector_shown);
    assert_eq!(
        report.viewport_position.0, 0,
        "no inspector reserves the left"
    );
    assert_eq!(report.viewport_size.0, report.surface.0);

    let top_edge = (report.top_bottom * report.scale).ceil();
    let bottom_edge = (report.bottom_top * report.scale).floor();
    assert_eq!(report.viewport_position.1 as f32, top_edge);
    assert_eq!(
        (report.viewport_position.1 + report.viewport_size.1) as f32,
        bottom_edge
    );
    let reserved = (RESOURCE_BAR_HEIGHT + COMMAND_PANEL_HEIGHT) * report.scale;
    assert!(
        report.viewport_size.1 as f32 <= report.surface.1 as f32 - reserved,
        "{report:?}"
    );
    Ok(())
}

#[derive(Resource, Default)]
struct Settled(u32);

/// Writes the report and ends the child, once the battle has run for a moment.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
fn report_window(
    mut settled: ResMut<'_, Settled>,
    session: Res<'_, BattleSession>,
    hud: Res<'_, HudGeometry>,
    ui: Res<'_, UiGeometry>,
    windows: Query<'_, '_, &Window, With<PrimaryWindow>>,
    cameras: Query<'_, '_, &Camera, With<RtsCamera>>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let (Ok(window), Ok(camera)) = (windows.get_single(), cameras.get_single()) else {
        return;
    };
    let (Some(top), Some(bottom), Some(viewport)) = (hud.top, hud.bottom, &camera.viewport) else {
        return;
    };
    if !session.is_ready() || window.physical_width() == 0 {
        return;
    }
    settled.0 += 1;
    if settled.0 < SETTLE_FRAMES {
        return;
    }
    let report = WindowReport {
        title: window.title.clone(),
        surface: (window.physical_width(), window.physical_height()),
        scale: ui.pixels_per_point,
        top_bottom: top.bottom(),
        bottom_top: bottom.top(),
        inspector_shown: ui.inspector.is_some(),
        viewport_position: (viewport.physical_position.x, viewport.physical_position.y),
        viewport_size: (viewport.physical_size.x, viewport.physical_size.y),
    };
    let path = std::env::var_os(CHILD_OUT_VAR).expect("the parent test names the file");
    let text = ron::to_string(&report).expect("a report has no unserializable field");
    std::fs::write(path, text).expect("the parent test's directory is writable");
    exit.send(AppExit::Success);
}

/// The child of [`the_skirmish_window_gives_the_camera_only_what_the_panels_leave`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn windowed_hud_child() {
    let mut app = client::build_graphical_app_off_main_thread(
        ClientMode::Skirmish,
        Some(CHILD_FRAME_LIMIT),
        SKIRMISH_SCENARIO.as_ref(),
    )
    .expect("the shipped assets must start the client");
    app.init_resource::<Settled>().add_systems(
        Update,
        report_window.in_set(client::battle_pipeline::BattleSet::Present),
    );
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}
