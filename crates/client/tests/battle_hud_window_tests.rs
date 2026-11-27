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
//! A second child selects a hangar unit spawned for the viewer and reports where the hangar's
//! card entries and the members line were drawn (plan
//! `2026-12-01-death-spawns-and-hangar-commands` HC-15).
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
use client::battle_selection::BattleSelection;
use client::input_policy::UiGeometry;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::{ClientMode, RtsCamera};
use serde::{Deserialize, Serialize};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the file the child reports to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_HUD_CHILD_OUT";
/// Name of the libtest entry the test launches as its child.
const CHILD_TEST: &str = "windowed_hud_child";
/// Name of the libtest entry the hangar test launches as its child.
const HANGAR_CHILD_TEST: &str = "windowed_hangar_child";
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

/// Where the hangar child saw its surfaces drawn, in egui points: `(min x, min y, max x, max y)`.
#[derive(Debug, Serialize, Deserialize)]
struct HangarReport {
    /// The command panel.
    panel: (f32, f32, f32, f32),
    /// The Build, Cancel and Auto-build entries, card positions 1 to 3.
    slots: Vec<(f32, f32, f32, f32)>,
    /// The members line in the selection column.
    line: (f32, f32, f32, f32),
}

fn corners(rect: bevy_egui::egui::Rect) -> (f32, f32, f32, f32) {
    (rect.min.x, rect.min.y, rect.max.x, rect.max.y)
}

/// HC-15 (plan `2026-12-01-death-spawns-and-hangar-commands` §6.5): with a hangar unit of the
/// viewer selected, the three hangar entries and the members line are drawn inside the command
/// panel, and none overlaps another. Geometry, not pixels (this file's stated limit).
#[test]
fn the_hangar_entries_and_line_lie_inside_the_command_panel() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let report_path = directory.path().join("hangar.ron");
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", HANGAR_CHILD_TEST]);
    command.env(CHILD_OUT_VAR, &report_path);
    command.run("windowed hangar child");

    let report: HangarReport = ron::from_str(&std::fs::read_to_string(&report_path)?)?;
    let inside = |rect: (f32, f32, f32, f32)| {
        let panel = report.panel;
        rect.0 >= panel.0 && rect.1 >= panel.1 && rect.2 <= panel.2 && rect.3 <= panel.3
    };
    let overlap = |a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)| {
        a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
    };
    let mut drawn = report.slots.clone();
    drawn.push(report.line);
    assert_eq!(drawn.len(), 4, "{report:?}");
    for (index, rect) in drawn.iter().enumerate() {
        assert!(
            inside(*rect),
            "surface {index} leaves the panel: {report:?}"
        );
        for other in &drawn[index + 1..] {
            assert!(!overlap(*rect, *other), "{rect:?} overlaps {other:?}");
        }
    }
    Ok(())
}

/// The hangar unit the child spawned and selected, once it has.
#[derive(Resource, Default)]
struct HangarChild {
    unit: Option<sim_types::EntityId>,
    settled: u32,
}

/// Spawns the shipped hangar unit (found by property) for the viewer beside its first unit and
/// selects it, as soon as the session is ready; then, once its entries and line were drawn for a
/// few frames, writes the report and ends the child.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn hangar_child(
    mut child: ResMut<'_, HangarChild>,
    mut session: ResMut<'_, BattleSession>,
    mut selection: ResMut<'_, BattleSelection>,
    hud: Res<'_, HudGeometry>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if !session.is_ready() {
        return;
    }
    let Some(unit) = child.unit else {
        let Some(world) = session.world_mut() else {
            return;
        };
        let viewer = sim_types::PlayerId(0);
        let host = world
            .catalog
            .archetypes()
            .find(|a| a.capabilities.hangar.is_some())
            .expect("the shipped catalog has a hangar unit")
            .unit_type_id;
        let near = world
            .entities
            .values()
            .find(|e| e.owner == viewer)
            .expect("the viewer starts with units")
            .position;
        let at = near + sim_types::SimVec2::from_i32(4, 4);
        let unit = world
            .spawn_unit(host, viewer, at)
            .expect("an air unit spawns beside the base");
        selection.select_only(unit);
        child.unit = Some(unit);
        return;
    };
    // The selection binds to its session and owner on the first frames, which clears it, so the
    // unit is selected again until the selection keeps it.
    if selection.ids() != [unit] {
        selection.select_only(unit);
        child.settled = 0;
        return;
    }
    let slots: Vec<_> = hud.card[1..=3].iter().flatten().copied().collect();
    let (Some(panel), Some(line)) = (hud.bottom, hud.hangar_line) else {
        return;
    };
    if slots.len() < 3 {
        return;
    }
    child.settled += 1;
    if child.settled < SETTLE_FRAMES {
        return;
    }
    let report = HangarReport {
        panel: corners(panel),
        slots: slots.into_iter().map(corners).collect(),
        line: corners(line),
    };
    let path = std::env::var_os(CHILD_OUT_VAR).expect("the parent test names the file");
    let text = ron::to_string(&report).expect("a report has no unserializable field");
    std::fs::write(path, text).expect("the parent test's directory is writable");
    exit.send(AppExit::Success);
}

/// The child of [`the_hangar_entries_and_line_lie_inside_the_command_panel`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn windowed_hangar_child() {
    let mut app = client::build_graphical_app_off_main_thread(
        ClientMode::Skirmish,
        Some(CHILD_FRAME_LIMIT),
        SKIRMISH_SCENARIO.as_ref(),
    )
    .expect("the shipped assets must start the client");
    app.init_resource::<HangarChild>().add_systems(
        Update,
        hangar_child.in_set(client::battle_pipeline::BattleSet::Present),
    );
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}
