//! The result window on a real window, a real egui pass and the real renderer.
//!
//! # Responsibility
//! `battle_result_tests.rs` proves the screen's behaviour without a window. This proves that the
//! production skirmish reaches the same screen when `winit` drives it, and that its `Exit` button
//! ends the process through the real run loop.
//!
//! # How it fits
//! The test re-executes this binary on the virtual GPU. Its child is
//! [`client::build_graphical_app_off_main_thread`], the production assembly, because a child on a
//! libtest thread cannot open a window on the default backend. The child stages the end of the
//! match in the live world, then presses the button with the events a window backend sends.
//!
//! # Limits
//! This reads geometry, not pixels: screenshots show egui panels flat on the capture path. Xvfb
//! offers no pointer here, so the press is injected as Bevy events, past the operating system and
//! into everything after it. A frame of the shipped map takes seconds on the software renderer,
//! so the child does only what it must.
//!
//! # Evidence
//! With `SPACE_SCUFFLE_RESULT_GRAB` naming a file and `SPACE_SCUFFLE_SHOW_WINDOW=1` mapping the
//! window, the child also grabs the X display with `xwd` once the window has settled, before it
//! presses `Exit`. That grab is the pixel evidence of the result window in `docs/evidence/`,
//! because Bevy's own screenshots show egui flat. The gate does not set either variable.
//!
//! # Invariants
//! The child reports before it presses, and only the button can end it: nothing else in the child
//! exits, its frame limit is out of reach, and if the press fails the GPU run's deadline fails the
//! test.

#![allow(clippy::expect_used, clippy::panic)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui::Rect;
use client::battle_hud::result::ResultScreen;
use client::battle_hud::HudGeometry;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::input_policy::UiGeometry;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::ClientMode;
use serde::{Deserialize, Serialize};
use sim_types::{PlayerId, SimScalar};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the file the child reports to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_RESULT_CHILD_OUT";
/// Name of the libtest entry the test launches as its child.
const CHILD_TEST: &str = "windowed_result_child";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// Environment variable naming the XWD file the child grabs the display into, if any.
const GRAB_VAR: &str = "SPACE_SCUFFLE_RESULT_GRAB";
/// Frames presented after the window settles and before the grab, so the display holds it.
const GRAB_FRAMES: u32 = 3;
/// The other player of the shipped skirmish, whose base the child razes.
const ENEMY: PlayerId = PlayerId(1);

/// What the child saw before it pressed `Exit`, in egui points unless named otherwise.
#[derive(Debug, Serialize, Deserialize)]
struct ResultReport {
    verdict: String,
    /// Left, top, right and bottom edges of the window.
    window: (f32, f32, f32, f32),
    /// Same edges of the world rectangle.
    world: (f32, f32, f32, f32),
    /// Whether both buttons were drawn inside the window.
    buttons_inside: bool,
    /// Whether the rectangle's middle is the window's middle, to a point.
    centred: bool,
}

#[test]
fn the_exit_button_ends_the_real_client_successfully() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let report_path = directory.path().join("report.ron");
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, &report_path);
    command.allow_for_hex_tiling_driver_defect();
    command.run("windowed result child");

    let report: ResultReport = ron::from_str(&std::fs::read_to_string(&report_path)?)?;
    assert_eq!(report.verdict, "Victory");
    assert!(report.buttons_inside, "{report:?}");
    assert!(report.centred, "{report:?}");
    let (left, top, right, bottom) = report.window;
    assert!(left >= report.world.0 && right <= report.world.2);
    assert!(top >= report.world.1 && bottom <= report.world.3);
    Ok(())
}

/// How far the child has got.
#[derive(Resource, Default, PartialEq, Eq)]
enum Stage {
    /// Waiting for the first panels of a ready battle.
    #[default]
    Waiting,
    /// The end is staged, and the window has not settled.
    Staged,
    /// The cursor moved over the button, which egui hovers on the next frame.
    Hovering,
    /// The button has been pressed and released, and the app should be ending.
    Released,
}

fn raze_enemy(session: &mut BattleSession) {
    let world = session.world_mut().expect("a ready battle has a world");
    for unit in world.entities.values_mut() {
        if unit.owner == ENEMY && unit.is_immobile {
            unit.hp = SimScalar::ZERO;
        }
    }
}

fn write_report(screen: &ResultScreen, hud: &HudGeometry, ui: &UiGeometry) {
    let (Some(window), Some(world), Some(again), Some(exit)) =
        (hud.result, ui.world, hud.play_again, hud.exit)
    else {
        panic!("the report needs a drawn window");
    };
    let edges = |rect: Rect| (rect.left(), rect.top(), rect.right(), rect.bottom());
    let report = ResultReport {
        verdict: screen
            .verdict()
            .map_or_else(String::new, |verdict| verdict.title().to_owned()),
        window: edges(window),
        world: edges(world),
        buttons_inside: window.contains_rect(again) && window.contains_rect(exit),
        centred: (window.center() - world.center()).length() < 1.0,
    };
    let path = std::env::var_os(CHILD_OUT_VAR).expect("the parent test names the file");
    let text = ron::to_string(&report).expect("a report has no unserializable field");
    std::fs::write(path, text).expect("the parent test's directory is writable");
}

/// Stages the end, reports the window, and presses `Exit`, one step per frame.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
fn drive_to_exit(
    mut stage: ResMut<'_, Stage>,
    mut previous: Local<'_, Option<Rect>>,
    mut grab_wait: Local<'_, u32>,
    mut session: ResMut<'_, BattleSession>,
    screen: Res<'_, ResultScreen>,
    hud: Res<'_, HudGeometry>,
    ui: Res<'_, UiGeometry>,
    mut windows: Query<'_, '_, (Entity, &mut Window), With<PrimaryWindow>>,
    mut cursor: EventWriter<'_, CursorMoved>,
    mut buttons: EventWriter<'_, MouseButtonInput>,
) {
    let Ok((entity, mut window)) = windows.get_single_mut() else {
        return;
    };
    let mut send = |state| {
        buttons.send(MouseButtonInput {
            button: MouseButton::Left,
            state,
            window: entity,
        });
    };
    match *stage {
        Stage::Waiting => {
            if session.is_ready() && hud.top.is_some() {
                raze_enemy(&mut session);
                *stage = Stage::Staged;
            }
        }
        Stage::Staged => {
            // egui measures a new window before it places it, so its first rectangles move.
            let Some(exit) = hud.exit.filter(|exit| *previous == Some(*exit)) else {
                *previous = hud.exit;
                return;
            };
            if let Some(path) = std::env::var_os(GRAB_VAR) {
                if *grab_wait < GRAB_FRAMES {
                    *grab_wait += 1;
                    return;
                }
                let status = std::process::Command::new("xwd")
                    .args(["-root", "-silent", "-out"])
                    .arg(&path)
                    .status()
                    .expect("xwd runs on the virtual display");
                assert!(status.success(), "xwd failed: {status:?}");
            }
            write_report(&screen, &hud, &ui);
            let position = Vec2::new(exit.center().x, exit.center().y);
            window.set_cursor_position(Some(position));
            cursor.send(CursorMoved {
                window: entity,
                position,
                delta: None,
            });
            *stage = Stage::Hovering;
        }
        Stage::Hovering => {
            // egui refuses a click held for over 0.8 s, which a lavapipe frame can exceed, so both
            // halves of the click go in one frame on every driver.
            send(ButtonState::Pressed);
            send(ButtonState::Released);
            *stage = Stage::Released;
        }
        Stage::Released => {}
    }
}

/// The child of [`the_exit_button_ends_the_real_client_successfully`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn windowed_result_child() {
    let mut app = client::build_graphical_app_off_main_thread(
        ClientMode::Skirmish,
        Some(CHILD_FRAME_LIMIT),
        SKIRMISH_SCENARIO.as_ref(),
    )
    .expect("the shipped assets must start the client");
    app.init_resource::<Stage>()
        .add_systems(Update, drive_to_exit.in_set(BattleSet::Present));
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}
