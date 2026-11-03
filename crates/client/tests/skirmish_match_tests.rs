//! The exit proof of Phase 5: a whole skirmish played to a result through input events alone.
//!
//! # Responsibility
//! Owns the two runs of the exit criterion, "a person can play a match to a result using only
//! mouse and keyboard" (blueprint WP9, decision D-13):
//! - `a_match_is_won_through_real_input_alone` plays the whole match headless, with the production
//!   client and the [`skirmish_pilot`] reading only the screen. It proves the **whole match**.
//! - `the_skirmish_hud_and_foundation_render_cleanly_on_a_real_window` runs the production client
//!   on a real window, the virtual GPU and Vulkan validation, and proves the **real renderer**: the
//!   pilot raises a foundation by input, and its dome fills from rim to apex on screen.
//!
//! Neither alone meets the criterion, and neither claims to.
//!
//! # How it fits
//! The windowed test re-executes this binary as its child, so no scripted-play code ships in the
//! client. A lavapipe frame of the shipped map takes seconds, so the child stages what would take
//! minutes of play: it boots paused and funds the bank, then sets the foundation's construction
//! share for each shot, through the world. That makes it a rendering test of the HUD, the ghost and
//! the fill, not a claim of play; the headless run stages nothing.
//!
//! # Invariants
//! - The headless run fails on any battle failure, simulation diagnostic, client refusal or
//!   revalidation rejection, and the pilot fails past its tick limit, printing its stage.
//! - The pilot's source names no session world accessor and no catalog identity.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;
#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;
#[path = "support/skirmish_pilot.rs"]
mod skirmish_pilot;

use std::path::{Path, PathBuf};
use std::time::Instant;

use battle_harness::{Harness, LOCAL_PLAYER};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::PrimaryWindow;
use client::battle_hud::feed::BattleFeed;
use client::battle_hud::result::{ResultScreen, ViewerResult};
use client::battle_hud::HudGeometry;
use client::battle_pipeline::driver::AdmissionOutcome;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::bubble_shader::BubbleInstance;
use client::input_policy::UiGeometry;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::scene_controller::LiveBattleBubble;
use client::{ClientMode, LabState, RtsCamera};
use serde::{Deserialize, Serialize};
use sim_types::{AnimState, MatchResult};
use skirmish_pilot::{Pilot, PilotGoal, SkirmishPilotPlugin, Stage};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Frames the headless match may take. The pilot's tick limit fails first on a slow match, at
/// four ticks a frame; this catches a pilot that stops issuing input altogether.
const MATCH_FRAME_LIMIT: u32 = 6_000;

// ---------------------------------------------------------------------------------------------
// The headless match
// ---------------------------------------------------------------------------------------------

/// Fails the run on anything a person would see as an error, with the pilot's stage.
fn assert_nothing_went_wrong(harness: &Harness) {
    let session = harness.app.world().resource::<BattleSession>();
    let pilot = harness.app.world().resource::<Pilot>();
    assert_eq!(
        session.diagnostics,
        [],
        "simulation diagnostics in {:?}",
        pilot.stage()
    );
    assert_eq!(
        session.rejected,
        [],
        "an order was rejected on revalidation in {:?}\n{}",
        pilot.stage(),
        pilot.log
    );
    assert_eq!(
        session.commands.latest_refusal(),
        None,
        "the client refused an order in {:?}\n{}",
        pilot.stage(),
        pilot.log
    );
    for notice in &session.notices {
        let accepted = match &notice.outcome {
            AdmissionOutcome::Production(outcome) => outcome.result.is_ok(),
            AdmissionOutcome::Movement(outcome) => outcome.result.is_ok(),
            AdmissionOutcome::Game(outcome) => outcome.result.is_ok(),
        };
        assert!(
            accepted,
            "the simulation refused an order in {:?}: {notice:?}\n{}",
            pilot.stage(),
            pilot.log
        );
    }
}

/// D-13: a whole match is won through input events, by a pilot that reads only the screen.
#[test]
fn a_match_is_won_through_real_input_alone() {
    let started = Instant::now();
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    skirmish_pilot::install(&mut harness.app, PilotGoal::MatchResult);
    let mut frames = 0;
    while harness.app.world().resource::<Pilot>().stage() != Stage::Done {
        harness.step(1);
        assert_nothing_went_wrong(&harness);
        frames += 1;
        assert!(
            frames < MATCH_FRAME_LIMIT,
            "the pilot stopped in {:?}\n{}",
            harness.app.world().resource::<Pilot>().stage(),
            harness.app.world().resource::<Pilot>().log
        );
    }
    // The verdict is read during presentation, and its window is drawn on the next frame.
    harness.step(2);
    let pilot = harness.app.world().resource::<Pilot>();
    eprintln!(
        "match won in {frames} frames, {} ticks, {:.1} s\n{}",
        harness.world().current_tick,
        started.elapsed().as_secs_f64(),
        pilot.log
    );

    assert_eq!(pilot.result(), Some(ViewerResult::Victory));
    assert!(matches!(
        harness.world().match_state.result,
        MatchResult::Victory { winner, .. } if winner == LOCAL_PLAYER
    ));
    let hud = harness.app.world().resource::<HudGeometry>();
    assert!(hud.result.is_some(), "the result window is on screen");
    let play_again = hud.play_again.expect("with its Play again button");
    let session = harness.app.world().resource::<BattleSession>();
    assert!(session.outcome_count > 0, "the simulation answered orders");

    // The match is over: a right click on the world issues nothing.
    let world_rect = harness
        .app
        .world()
        .resource::<UiGeometry>()
        .world
        .expect("a world on screen");
    let corner = Vec2::new(world_rect.left() + 40.0, world_rect.top() + 40.0);
    let lines = feed_lines(&harness);
    harness.click(MouseButton::Right, corner);
    assert_eq!(harness.pending_orders(), 0, "orders are shut");
    assert_eq!(harness.marker(), None);
    assert_eq!(feed_lines(&harness), lines, "and nothing new is reported");

    // egui measures a new window before it places it, so aim at the button as drawn now.
    let play_again = harness
        .app
        .world()
        .resource::<HudGeometry>()
        .play_again
        .unwrap_or(play_again);
    harness.click(
        MouseButton::Left,
        Vec2::new(play_again.center().x, play_again.center().y),
    );
    // A new match starts unpaused, so the frame that commits it runs its first tick.
    assert_eq!(harness.world().current_tick, 1);
    assert_eq!(harness.world().match_state.result, MatchResult::InProgress);
    let lab = harness.app.world().resource::<LabState>();
    assert!(!lab.battle_paused, "the new match runs");
    assert_eq!(lab.simulation_speed, LabState::default().simulation_speed);
    harness.step(2);
    assert!(harness.world().current_tick > 0);
    assert_eq!(
        harness.app.world().resource::<ResultScreen>().verdict(),
        None
    );
}

fn feed_lines(harness: &Harness) -> Vec<String> {
    harness
        .app
        .world()
        .resource::<BattleFeed>()
        .lines()
        .map(|line| line.text.clone())
        .collect()
}

/// D-13: the pilot cannot read the authoritative world, and names no catalog identity.
#[test]
fn the_pilot_reads_the_screen_only() {
    let source = include_str!("support/skirmish_pilot.rs");
    for forbidden in [
        ".world()",
        "world_mut",
        "SimWorld",
        "LabState",
        "RtsCamera {",
    ] {
        assert!(
            !source.contains(forbidden),
            "skirmish_pilot.rs must not use {forbidden}"
        );
    }
    let catalog = sim_core::UnitCatalog::default_catalog();
    for archetype in catalog.archetypes() {
        assert!(
            !source.contains(&format!("\"{}\"", archetype.slug)),
            "skirmish_pilot.rs names {}",
            archetype.slug
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The windowed run
// ---------------------------------------------------------------------------------------------

/// Environment variable naming the directory the child writes to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_SKIRMISH_CHILD_OUT";
/// Name of the libtest entry the windowed test launches.
const CHILD_TEST: &str = "windowed_skirmish_child";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// Minerals the child adds to the bank, so the pilot can raise at once.
const STAGED_MINERALS: u32 = 1_000;
/// Rendered frames between staging a construction share and capturing it.
const SETTLE_FRAMES: u32 = 3;
/// Construction shares the child stages for its two shots.
const SHARES: [f32; 2] = [0.25, 0.75];
/// File the child writes the projected rings and the presented shares to.
const RINGS_FILE: &str = "rings.ron";
/// Shares of the dome's height sampled, from rim to near the apex.
const RING_SHARES: [f32; 9] = [0.05, 0.12, 0.2, 0.35, 0.45, 0.55, 0.65, 0.85, 0.92];
/// Smallest cosine between surface normal and view that is sampled, as in `dome_fill_tests.rs`.
const MIN_VIEW_COSINE: f32 = 0.4;
/// Step between sampled azimuths, in degrees.
const AZIMUTH_STEP_DEGREES: usize = 3;
/// A ring this near a presented share is neither built nor unbuilt in that shot.
const SCAN_LINE_MARGIN: f32 = 0.06;
/// Smallest mean difference, in levels of 255, between the shots of a ring the fill crossed.
/// `dome_fill_tests.rs` measured at least 56 between built and unbuilt rings on lavapipe.
const CROSSED_FLOOR: f32 = 10.0;
/// How many times the change of a crossed ring must exceed that of a ring built in both shots.
const CROSSED_RATIO: f32 = 3.0;

/// Screen pixels of one ring of the foundation's dome.
#[derive(Debug, Serialize, Deserialize)]
struct Ring {
    share: f32,
    pixels: Vec<(u32, u32)>,
}

/// What the child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct WindowedReport {
    /// The construction share each shot presented, from the bubble instance itself.
    presented: Vec<f32>,
    rings: Vec<Ring>,
    /// Physical pixel rows of the command panel's top edge and of the world's middle.
    panel_top: u32,
    world_middle: u32,
    /// The pilot's stages, with the tick each began on.
    pilot_log: String,
}

#[test]
fn the_skirmish_hud_and_foundation_render_cleanly_on_a_real_window() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let started = Instant::now();
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    gpu_run::allow_for_hex_tiling_driver_defect(&mut command);
    gpu_run::assert_passes_validation(&mut command, "windowed skirmish");
    eprintln!(
        "windowed skirmish child took {:.1} s",
        started.elapsed().as_secs_f64()
    );

    let report: WindowedReport =
        ron::from_str(&std::fs::read_to_string(directory.path().join(RINGS_FILE))?)?;
    eprintln!("{}", report.pilot_log);
    let early = image::open(directory.path().join("early.png"))?.to_rgb8();
    let late = image::open(directory.path().join("late.png"))?.to_rgb8();
    assert_fill_rises(&report, &early, &late);
    assert_panel_differs_from_world(&report, &early);
    Ok(())
}

/// WP3's ring method on the live foundation: rings the fill crossed between the shots change far
/// more than rings it had already built in both.
fn assert_fill_rises(report: &WindowedReport, early: &image::RgbImage, late: &image::RgbImage) {
    let [first, second] = report.presented[..] else {
        panic!("two shots: {report:?}");
    };
    assert!(first < second, "the shares rose: {first} then {second}");
    let mut crossed = Vec::new();
    let mut built = Vec::new();
    for ring in &report.rings {
        assert!(!ring.pixels.is_empty(), "ring {} is on screen", ring.share);
        let difference = mean_difference(early, late, &ring.pixels);
        if ring.share < first - SCAN_LINE_MARGIN {
            built.push((ring.share, difference));
        } else if ring.share > first + SCAN_LINE_MARGIN && ring.share < second - SCAN_LINE_MARGIN {
            crossed.push((ring.share, difference));
        }
    }
    eprintln!("crossed rings {crossed:?}, rings built in both {built:?}");
    assert!(!crossed.is_empty() && !built.is_empty(), "{report:?}");
    let stable = built.iter().map(|(_, d)| *d).fold(0.0, f32::max);
    for (share, difference) in crossed {
        assert!(
            difference > CROSSED_FLOOR && difference > CROSSED_RATIO * stable,
            "the ring at {share} changed by {difference}, a built one by up to {stable}"
        );
    }
}

/// The world and the command panel are different surfaces on screen. Bevy's capture shows egui
/// panels flat, so this proves only that the panel covers the bottom of the frame.
fn assert_panel_differs_from_world(report: &WindowedReport, shot: &image::RgbImage) {
    let row_mean = |y: u32| {
        let total: u64 = (0..shot.width())
            .flat_map(|x| shot.get_pixel(x, y).0)
            .map(u64::from)
            .sum();
        total as f32 / (shot.width() * 3) as f32
    };
    let panel = row_mean(report.panel_top + 8);
    let world = row_mean(report.world_middle);
    assert!(
        (panel - world).abs() > 2.0,
        "the panel row ({panel}) must differ from the world row ({world})"
    );
}

/// Mean absolute channel difference between two captures over `pixels`, in levels of 255.
fn mean_difference(a: &image::RgbImage, b: &image::RgbImage, pixels: &[(u32, u32)]) -> f32 {
    let total: u32 = pixels
        .iter()
        .flat_map(|&(x, y)| {
            let (left, right) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
            left.into_iter()
                .zip(right)
                .map(|(l, r)| u32::from(l.abs_diff(r)))
        })
        .sum();
    total as f32 / (pixels.len() * 3) as f32
}

/// Where the windowed child is.
#[derive(Resource, Default)]
struct Staging {
    directory: PathBuf,
    /// Whether the bank has been funded.
    funded: bool,
    /// Whether the clock has then been stopped.
    prepared: bool,
    /// Index into [`SHARES`] of the shot being staged.
    shot: usize,
    /// Frames rendered since the shot was staged.
    settled: u32,
    capture_pending: bool,
    report: WindowedReport,
}

/// Bevy world centre and radius of the viewer's foundation, and its presented share.
fn foundation(
    bubbles: &Query<'_, '_, (&BubbleInstance, &GlobalTransform), With<LiveBattleBubble>>,
) -> Option<(Vec3, f32, f32)> {
    bubbles
        .iter()
        .find(|(bubble, _)| {
            bubble.0.owner == LOCAL_PLAYER && bubble.0.anim_state == AnimState::Constructing
        })
        .map(|(bubble, transform)| {
            (
                transform.translation(),
                transform.scale().x,
                bubble.0.build_or_queue_progress.unwrap_or(0.0),
            )
        })
}

/// Stages the child: funds the bank at boot, then, once the pilot has raised a foundation, sets
/// its share for each shot and captures it.
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "a Bevy system with one declared parameter per dependency"
)]
fn stage_shots(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, Staging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    pilot: Res<'_, Pilot>,
    hud: Res<'_, HudGeometry>,
    ui: Res<'_, UiGeometry>,
    bubbles: Query<'_, '_, (&BubbleInstance, &GlobalTransform), With<LiveBattleBubble>>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform), With<RtsCamera>>,
    mut windows: Query<'_, '_, &mut Window, With<PrimaryWindow>>,
) {
    // Xvfb runs no window manager, so nothing would give the window focus, and an unfocused
    // window takes no input by design.
    if let Ok(mut window) = windows.get_single_mut() {
        if !window.focused {
            window.focused = true;
        }
    }
    if !session.is_ready() || staging.capture_pending {
        return;
    }
    if !staging.funded {
        // Funded on a running frame, so a tick publishes the bank to the HUD; stopped on the next.
        let world = session.world_mut().expect("a ready session has a world");
        world
            .player_resources
            .get_mut(LOCAL_PLAYER)
            .expect("invariant: LOCAL_PLAYER has a player slot")
            .minerals += STAGED_MINERALS;
        staging.funded = true;
        return;
    }
    if !staging.prepared {
        lab.battle_paused = true;
        staging.prepared = true;
        return;
    }
    if pilot.stage() != Stage::Done {
        return;
    }
    let Some(&share) = SHARES.get(staging.shot) else {
        return;
    };
    if staging.settled == 0 {
        // Set the share, then let one tick publish it before stopping the clock again.
        let world = session.world_mut().expect("a ready session has a world");
        let construction = world
            .entities
            .values_mut()
            .find_map(|unit| {
                (unit.owner == LOCAL_PLAYER)
                    .then_some(unit.construction.as_mut())
                    .flatten()
            })
            .expect("the pilot raised a foundation");
        construction.remaining_ticks =
            ((1.0 - share) * construction.total_ticks as f32).round() as u32;
        lab.battle_paused = false;
        staging.settled = 1;
        return;
    }
    lab.battle_paused = true;
    staging.settled += 1;
    if staging.settled < SETTLE_FRAMES {
        return;
    }
    let (Some((centre, radius, presented)), Ok((camera, transform))) =
        (foundation(&bubbles), cameras.get_single())
    else {
        panic!("the foundation is on screen");
    };
    staging.report.presented.push(presented);
    if staging.shot == 0 {
        staging.report.rings = rings(camera, transform, centre, radius);
        let scale = ui.pixels_per_point;
        let bottom = hud.bottom.expect("the command panel is up");
        let world = ui.world.expect("the world is on screen");
        staging.report.panel_top = (bottom.top() * scale).ceil() as u32;
        staging.report.world_middle = (world.center().y * scale) as u32;
        staging.report.pilot_log.clone_from(&pilot.log);
    }
    staging.capture_pending = true;
    let name = if staging.shot == 0 { "early" } else { "late" };
    let path = staging.directory.join(format!("{name}.png"));
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<'_, ScreenshotCaptured>,
              mut staging: ResMut<'_, Staging>,
              mut exit: EventWriter<'_, AppExit>| {
            trigger
                .event()
                .0
                .clone()
                .try_into_dynamic()
                .expect("a screenshot converts to an image")
                .to_rgb8()
                .save(&path)
                .expect("a screenshot must be written");
            staging.shot += 1;
            staging.settled = 0;
            staging.capture_pending = false;
            if staging.shot == SHARES.len() {
                let text = ron::to_string(&staging.report).expect("plain data");
                std::fs::write(staging.directory.join(RINGS_FILE), text)
                    .expect("the parent's directory is writable");
                exit.send(AppExit::Success);
            }
        },
    );
}

/// Projects rings of a dome's upper half to physical window pixels through the live camera.
fn rings(camera: &Camera, transform: &GlobalTransform, centre: Vec3, radius: f32) -> Vec<Ring> {
    let rectangle = camera
        .physical_viewport_rect()
        .expect("the camera renders through a viewport");
    let size = rectangle.size().as_vec2();
    let clip_from_world = camera.clip_from_view() * transform.compute_matrix().inverse();
    let eye = transform.translation();
    RING_SHARES
        .iter()
        .map(|&share| {
            let ring = (1.0 - share * share).sqrt();
            let pixels = (0..360)
                .step_by(AZIMUTH_STEP_DEGREES)
                .filter_map(|degrees| {
                    let (sin, cos) = (degrees as f32).to_radians().sin_cos();
                    let normal = Vec3::new(cos * ring, share, sin * ring);
                    let point = centre + normal * radius;
                    let facing = normal.dot((eye - point).normalize()) > MIN_VIEW_COSINE;
                    facing.then(|| {
                        let ndc = clip_from_world.project_point3(point);
                        let pixel = Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * size;
                        let window = rectangle.min + pixel.as_uvec2();
                        (window.x, window.y)
                    })
                })
                .collect();
            Ring { share, pixels }
        })
        .collect()
}

/// The child of [`the_skirmish_hud_and_foundation_render_cleanly_on_a_real_window`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn windowed_skirmish_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    let mut app = client::build_graphical_app_off_main_thread(
        ClientMode::Skirmish,
        Some(CHILD_FRAME_LIMIT),
        SKIRMISH_SCENARIO.as_ref(),
    )
    .expect("the shipped assets must start the client");
    app.add_plugins(SkirmishPilotPlugin {
        goal: PilotGoal::FoundationRaised,
    })
    .insert_resource(Staging {
        directory: Path::new(&directory).to_path_buf(),
        ..Staging::default()
    })
    .add_systems(Update, stage_shots.in_set(BattleSet::Present));
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}
