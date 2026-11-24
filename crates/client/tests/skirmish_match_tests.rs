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
//! It also owns the end-to-end check of mode commands and their looks (plan
//! `2026-11-09-effect-visualization` WP12): a burrow and a siege ordered by key publish their
//! stance in the `RenderSnapshot` and the owner's `PlayerView` over the channel, the effect
//! system holds each switch's morph and then its stance look at the right moments, and the
//! opponent's presentation obeys the fog rule (D-5): nothing of a unit under its fog, nothing of
//! an undetected burrowed unit, and the burrowed stance where its detector covers the unit. The
//! opponent's presentation is the production client shown to the opponent
//! (`BattlePresentation::controlled`, the developer view's participant choice); the orders under
//! test are still the local player's keys.
//!
//! # How it fits
//! The windowed test re-executes this binary as its child, so no scripted-play code ships in the
//! client. A debug build on the test GPU cannot play minutes of the match in the run limit, so the
//! child stages what would take minutes of play: it funds the bank, then sets the foundation's
//! construction share for each shot, through the world. That makes it a rendering test of the HUD,
//! the ghost and the fill, not a claim of play; the headless match stages nothing.
//!
//! Each staging step runs the clock until the session publishes a later tick, and only then stops
//! it ([`StagePhase`]). A frame is not a tick: a `lavapipe` frame of the shipped map takes seconds,
//! but a `radv` frame is far shorter than the 40 ms tick.
//! `a_staged_share_is_published_however_short_the_frame` runs the same step headless under 5 ms
//! frames.
//!
//! # Invariants
//! - The headless run fails on any battle failure, simulation diagnostic, client refusal or
//!   revalidation rejection, and the pilot fails past its tick limit, printing its stage.
//! - The pilot's source names no session world accessor and no catalog identity.
//! - The mode tests choose archetypes by capability (a burrowed or anchored mode, detection, an
//!   unarmed flyer) among those the client can present, never by name. Their setup may spawn
//!   units, kill the detector and choose the presented participant; every mode switch under test
//!   arrives through a key event.

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
use std::time::{Duration, Instant};

use battle_harness::{Harness, LOCAL_PLAYER};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::time::TimeUpdateStrategy;
use bevy::window::PrimaryWindow;
use client::battle_hud::card::{CardAction, CommandCard};
use client::battle_hud::feed::BattleFeed;
use client::battle_hud::model::HudModel;
use client::battle_hud::result::{ResultScreen, ViewerResult};
use client::battle_hud::HudGeometry;
use client::battle_pipeline::commands::{is_armed, mode_of_ability, ModeDirection};
use client::battle_pipeline::driver::AdmissionOutcome;
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::battle_presentation::BattlePresentation;
use client::bubble_shader::BubbleInstance;
use client::effects::catalog::{EffectCatalog, EffectId, ModeEffectIds, UnitEffectTables};
use client::effects::stance::{channel_progress, StanceTrack};
use client::input_policy::UiGeometry;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::scene_controller::{LiveBattleBubble, RenderEntityMap};
use client::{ClientMode, LabState, RtsCamera};
use serde::{Deserialize, Serialize};
use sim_types::{
    AnimState, Concealment, EntityId, MatchResult, ModeStateView, MoverType, PlayerId, SimScalar,
    StanceView, UnitTypeId,
};
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
    let catalog = sim_fixtures::shipped_catalogs().units;
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
/// `dome_fill_tests.rs` measured at least 56 between built and unbuilt rings on lavapipe. On
/// 2026-11-03 the gate measured crossed rings changing by 71.1 to 85.0 on radv and 70.7 to 84.2 on
/// lavapipe.
const CROSSED_FLOOR: f32 = 10.0;
/// How many times the change of a crossed ring must exceed that of a ring built in both shots. On
/// 2026-11-03 rings built in both shots changed by at most 8.7 on radv and 9.6 on lavapipe, so the
/// smallest ratio was 8.2 and 7.4.
///
/// Why the ratio does not depend on machine load (tiered gates plan, §2.6 and WP6): until battle
/// flipbook WP5 (2026-11-04) a dome's video advanced with wall time, so a loaded machine let more
/// of it play between the shots and a built ring changed by up to 24.0. The flipbook clock is now
/// presented simulation time, frozen while the child holds the battle paused, and a slow frame
/// can add at most Bevy's 250 ms virtual-time cap, about six ticks, between the shots. `lavapipe`,
/// whose frames take seconds, always runs at that cap, so it is the worst case any load can
/// reach. On 2026-11-05, at load averages of 17 to 27 on 16 cores, built rings changed by 0.3 to
/// 1.4 on radv and by 1.7 on lavapipe, against 60.1 or more for crossed rings: a ratio of 35.
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
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("windowed skirmish");
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

/// How far the staging of the bank and of one construction share has got.
///
/// Each step that changes the world runs the clock until the session has published a later tick,
/// and only then stops it, so the step reaches the screen however short a frame is (plan
/// `2026-11-03-gpu-test-harness`, decision D9). A frame is not a tick: on `radv` a frame of the
/// shipped map is far shorter than the 40 ms tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum StagePhase {
    /// The bank has not been funded yet.
    #[default]
    Fund,
    /// The bank was funded when tick `since` was published; the clock runs until a later one is.
    FundPublishing { since: u32 },
    /// The funded bank is published and the clock stopped; waiting for the pilot's foundation.
    AwaitFoundation,
    /// The share was set when tick `since` was published; the clock runs until a later one is.
    SharePublishing { since: u32 },
    /// The share is published and the clock stopped; `frames` have been rendered since.
    Settling { frames: u32 },
    /// [`SETTLE_FRAMES`] frames have been rendered: the shot is ready to capture.
    Ready,
}

/// Where the staging of the windowed child's shots stands.
#[derive(Resource, Debug, Default)]
struct Staging {
    phase: StagePhase,
    /// Index into [`SHARES`] of the shot being staged.
    shot: usize,
}

impl Staging {
    /// Moves on to the next share once the current shot has been taken.
    fn shot_taken(&mut self) {
        self.shot += 1;
        self.phase = StagePhase::AwaitFoundation;
    }
}

/// The latest tick the session published, or 0 before the first.
fn published_tick(session: &BattleSession) -> u32 {
    session.published().map_or(0, |published| published.tick)
}

/// Stages the child through the world: funds the bank at boot, then, once the pilot has raised a
/// foundation, sets its share for each shot. Shared by the windowed child and
/// `a_staged_share_is_published_however_short_the_frame`, so the headless test runs this code.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system with one declared parameter per dependency"
)]
fn stage_construction(
    mut staging: ResMut<'_, Staging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    pilot: Res<'_, Pilot>,
) {
    if !session.is_ready() {
        return;
    }
    let tick = published_tick(&session);
    staging.phase = match staging.phase {
        StagePhase::Fund => {
            let world = session.world_mut().expect("a ready session has a world");
            world
                .player_resources
                .get_mut(LOCAL_PLAYER)
                .expect("invariant: LOCAL_PLAYER has a player slot")
                .minerals += STAGED_MINERALS;
            lab.battle_paused = false;
            StagePhase::FundPublishing { since: tick }
        }
        StagePhase::SharePublishing { since } | StagePhase::FundPublishing { since }
            if tick <= since =>
        {
            staging.phase
        }
        StagePhase::FundPublishing { .. } => {
            lab.battle_paused = true;
            StagePhase::AwaitFoundation
        }
        StagePhase::AwaitFoundation => {
            let Some(&share) = SHARES.get(staging.shot) else {
                return;
            };
            if pilot.stage() != Stage::Done {
                return;
            }
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
            StagePhase::SharePublishing { since: tick }
        }
        StagePhase::SharePublishing { .. } => {
            lab.battle_paused = true;
            StagePhase::Settling { frames: 0 }
        }
        StagePhase::Settling { frames } if frames + 1 >= SETTLE_FRAMES => StagePhase::Ready,
        StagePhase::Settling { frames } => StagePhase::Settling { frames: frames + 1 },
        StagePhase::Ready => StagePhase::Ready,
    };
}

/// What the windowed child has captured so far.
#[derive(Resource, Default)]
struct Capture {
    directory: PathBuf,
    /// Whether a screenshot has been requested and not yet written.
    pending: bool,
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

/// Captures each staged share once [`stage_construction`] reports it ready, then exits.
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "a Bevy system with one declared parameter per dependency"
)]
fn capture_shots(
    mut commands: Commands<'_, '_>,
    staging: Res<'_, Staging>,
    mut capture: ResMut<'_, Capture>,
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
    if staging.phase != StagePhase::Ready || capture.pending {
        return;
    }
    let (Some((centre, radius, presented)), Ok((camera, transform))) =
        (foundation(&bubbles), cameras.get_single())
    else {
        panic!("the foundation is on screen");
    };
    capture.report.presented.push(presented);
    if staging.shot == 0 {
        capture.report.rings = rings(camera, transform, centre, radius);
        let scale = ui.pixels_per_point;
        let bottom = hud.bottom.expect("the command panel is up");
        let world = ui.world.expect("the world is on screen");
        capture.report.panel_top = (bottom.top() * scale).ceil() as u32;
        capture.report.world_middle = (world.center().y * scale) as u32;
        capture.report.pilot_log.clone_from(&pilot.log);
    }
    capture.pending = true;
    let name = if staging.shot == 0 { "early" } else { "late" };
    let path = capture.directory.join(format!("{name}.png"));
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<'_, ScreenshotCaptured>,
              mut staging: ResMut<'_, Staging>,
              mut capture: ResMut<'_, Capture>,
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
            staging.shot_taken();
            capture.pending = false;
            if staging.shot == SHARES.len() {
                let text = ron::to_string(&capture.report).expect("plain data");
                std::fs::write(capture.directory.join(RINGS_FILE), text)
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

/// Frame time of the short-frame test, in milliseconds: an eighth of the 40 ms tick, so most
/// frames run no tick, as on `radv`.
const SHORT_FRAME_MS: u64 = 5;
/// Frames the short-frame test may take before it fails.
const SHORT_FRAME_LIMIT: u32 = 40_000;
/// Largest difference between a staged share and the one presented. The tick that publishes a
/// share also advances construction by one tick of its total.
const SHARE_TOLERANCE: f32 = 0.05;

/// The viewer's foundation's presented construction share, if it is on the field.
fn presented_share(harness: &mut Harness) -> Option<f32> {
    let world = harness.app.world_mut();
    world
        .query_filtered::<&BubbleInstance, With<LiveBattleBubble>>()
        .iter(world)
        .find(|bubble| {
            bubble.0.owner == LOCAL_PLAYER && bubble.0.anim_state == AnimState::Constructing
        })
        .map(|bubble| bubble.0.build_or_queue_progress.unwrap_or(0.0))
}

/// D9 (plan `2026-11-03-gpu-test-harness`): the windowed child's staging step, under frames an
/// eighth of a tick long, still publishes the funded bank and each staged share before it stops
/// the clock. This is the deterministic form of the race `radv`'s short frames exposed.
#[test]
fn a_staged_share_is_published_however_short_the_frame() {
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    // The production client starts its skirmish running, and the child stages from there.
    harness.resume();
    harness
        .app
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            SHORT_FRAME_MS,
        )));
    skirmish_pilot::install(&mut harness.app, PilotGoal::FoundationRaised);
    harness
        .app
        .init_resource::<Staging>()
        .add_systems(Update, stage_construction.in_set(BattleSet::Present));

    let mut previous = StagePhase::Fund;
    let mut presented = Vec::new();
    for _ in 0..SHORT_FRAME_LIMIT {
        harness.step(1);
        let phase = harness.app.world().resource::<Staging>().phase;
        if matches!(previous, StagePhase::FundPublishing { .. }) && phase != previous {
            let bank = harness
                .app
                .world()
                .resource::<HudModel>()
                .view
                .status
                .minerals;
            assert!(
                bank >= STAGED_MINERALS,
                "the clock stopped before a tick published the funded bank: the HUD shows {bank}"
            );
        }
        previous = phase;
        if phase != StagePhase::Ready {
            continue;
        }
        assert!(
            harness.app.world().resource::<LabState>().battle_paused,
            "the clock is stopped for the shot"
        );
        let shot = harness.app.world().resource::<Staging>().shot;
        let share = presented_share(&mut harness).expect("the foundation is on the field");
        assert!(
            (share - SHARES[shot]).abs() < SHARE_TOLERANCE,
            "shot {shot} staged {} but presented {share}; presented so far {presented:?}",
            SHARES[shot]
        );
        presented.push(share);
        harness
            .app
            .world_mut()
            .resource_mut::<Staging>()
            .shot_taken();
        if presented.len() == SHARES.len() {
            break;
        }
    }
    assert_eq!(
        presented.len(),
        SHARES.len(),
        "staging stopped in {previous:?}, the pilot in {:?}\n{}",
        harness.app.world().resource::<Pilot>().stage(),
        harness.app.world().resource::<Pilot>().log
    );
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
    .init_resource::<Staging>()
    .insert_resource(Capture {
        directory: Path::new(&directory).to_path_buf(),
        ..Capture::default()
    })
    .add_systems(
        Update,
        (stage_construction, capture_shots)
            .chain()
            .in_set(BattleSet::Present),
    );
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

// ---------------------------------------------------------------------------------------------
// Mode commands and their looks (plan 2026-11-09-effect-visualization WP12)
// ---------------------------------------------------------------------------------------------

/// The opponent of the viewer in the shipped skirmish.
const OPPONENT: PlayerId = PlayerId(1);
/// Frames a mode switch, its channel's random extra and the look's settle may take.
const SWITCH_FRAME_LIMIT: u32 = 400;
/// Frames run after a switch's looks settle, so the held stance is seen standing.
const STANDING_FRAMES: u32 = 10;

/// Which mode a [`mode_unit`] is chosen for, by the mode's catalog properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModeKind {
    /// A burrowed mode (`burrowed: true`, the Lurker analogue's).
    Burrow,
    /// An anchored mode that is not burrowed (`is_immobile`, the Siege Tank analogue's).
    Siege,
}

/// What one frame published and presented of one unit.
#[derive(Debug, Clone, Copy)]
struct StanceFrame {
    /// The simulation tick presented.
    tick: u32,
    /// The stance in the presented `RenderSnapshot`.
    stance: StanceView,
    /// The concealment in the presented `RenderSnapshot`.
    concealment: Concealment,
    /// The unit's entry in the viewer's `PlayerView::mode_states` (the viewer's own units only).
    state: Option<ModeStateView>,
    /// The tick of that `PlayerView`, written for the latest executed tick.
    view_tick: u32,
    /// The latest authoritative snapshot's tick and the unit's stance in it. The presented
    /// snapshot runs up to a tick behind (the timeline interpolates towards the latest), so the
    /// `PlayerView` is compared with this one.
    latest: (u32, StanceView),
    /// The morph the effect system holds on the unit, with its presented progress.
    morph: Option<(EffectId, f32)>,
    /// The stance look held on the unit.
    stance_look: Option<EffectId>,
    /// The concealment look held on the unit.
    cover: Option<EffectId>,
    /// Whether Bevy would draw the unit's bubble.
    drawn: bool,
}

impl StanceFrame {
    /// Whether the effect system holds any look on the unit.
    const fn holds_a_look(&self) -> bool {
        self.morph.is_some() || self.stance_look.is_some() || self.cover.is_some()
    }
}

/// The skirmish, paused, with the camera on the local player's first worker, and that worker's
/// position (open ground in view).
fn skirmish_at_home() -> (Harness, Vec2) {
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let world = harness.world();
    let worker = world
        .entities
        .values()
        .find(|unit| {
            unit.owner == LOCAL_PLAYER
                && world
                    .catalog
                    .get(unit.unit_type_id)
                    .is_ok_and(|archetype| archetype.capabilities.can_construct)
        })
        .map(|unit| unit.id)
        .expect("the skirmish starts with workers");
    let home = harness.simulated_position(worker);
    harness.look_at(home);
    (harness, home)
}

/// Whether the client can present `unit_type`: the HUD catalog records why it cannot.
fn presentable_type(harness: &Harness, unit_type: UnitTypeId) -> bool {
    harness
        .app
        .world()
        .resource::<HudModel>()
        .catalog
        .archetype(unit_type)
        .is_some_and(|commands| commands.unpresentable.is_none())
}

/// A presentable mobile archetype whose mode ability switches into a mode of `kind`, and that
/// mode's index in its `modes`. Chosen by capability, never by name (charter §3.3).
fn mode_unit(harness: &Harness, kind: ModeKind) -> (UnitTypeId, u8) {
    harness
        .world()
        .catalog
        .archetypes()
        .filter(|archetype| {
            archetype.max_speed > SimScalar::ZERO
                && presentable_type(harness, archetype.unit_type_id)
        })
        .find_map(|archetype| {
            let mode = archetype
                .abilities
                .iter()
                .find_map(|ability| mode_of_ability(archetype, ability))?;
            let index = archetype
                .modes
                .iter()
                .position(|candidate| candidate.mode_name == mode)?;
            let spec = &archetype.modes[index];
            let fits = match kind {
                ModeKind::Burrow => spec.burrowed,
                ModeKind::Siege => !spec.burrowed && spec.is_immobile,
            };
            fits.then(|| {
                let index = u8::try_from(index).expect("invariant: a catalog has few modes");
                (archetype.unit_type_id, index)
            })
        })
        .unwrap_or_else(|| panic!("the catalog ships a presentable {kind:?} unit"))
}

/// A presentable, unarmed, mobile air archetype that does not detect: an enemy that sees
/// without detecting, and that the local units cannot shoot down while it looks.
fn spotter_type(harness: &Harness) -> UnitTypeId {
    harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.max_speed > SimScalar::ZERO
                && archetype.mover_type == MoverType::Air
                && !archetype.is_detector()
                && !is_armed(archetype)
                && presentable_type(harness, archetype.unit_type_id)
        })
        .expect("the catalog ships a presentable unarmed air unit that does not detect")
        .unit_type_id
}

/// A presentable mobile detector.
fn detector_type(harness: &Harness) -> UnitTypeId {
    harness
        .world()
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.max_speed > SimScalar::ZERO
                && archetype.is_detector()
                && presentable_type(harness, archetype.unit_type_id)
        })
        .expect("the catalog ships a presentable mobile detector")
        .unit_type_id
}

/// Spawns a unit of `unit_type` for `owner` on open ground `distance` from `near`, and runs the
/// ticks that present it.
fn spawn_near(
    harness: &mut Harness,
    near: Vec2,
    distance: f32,
    unit_type: UnitTypeId,
    owner: PlayerId,
) -> EntityId {
    let at = harness.open_ground(near, distance);
    let unit = harness
        .world_mut()
        .spawn_unit(unit_type, owner, battle_harness::sim_position(at))
        .expect("open ground");
    run_mode_ticks(harness, 2);
    unit
}

/// Fails on any simulation diagnostic.
fn assert_no_diagnostics(harness: &Harness) {
    let session = harness.app.world().resource::<BattleSession>();
    assert_eq!(session.diagnostics, [], "simulation diagnostics");
}

/// Runs `ticks` simulation ticks, then stops the clock.
fn run_mode_ticks(harness: &mut Harness, ticks: u32) {
    let goal = harness.world().current_tick + ticks;
    harness.resume();
    let mut frames = 0;
    while harness.world().current_tick < goal {
        harness.step(1);
        frames += 1;
        assert!(frames < 10_000, "the battle stopped ticking");
    }
    harness.pause();
    assert_no_diagnostics(harness);
}

/// Taps the key of the command card's mode entry in `direction` for the selection.
fn tap_mode_key(harness: &mut Harness, direction: ModeDirection) {
    harness.step(1);
    let card = harness.app.world().resource::<CommandCard>();
    let key = card
        .slots
        .iter()
        .flatten()
        .find(|slot| {
            matches!(slot.action, CardAction::SwitchMode { direction: d, .. } if d == direction)
        })
        .map_or_else(
            || panic!("no {direction:?} entry: {:?}", card.slots),
            |slot| slot.key,
        );
    harness.tap_key(key);
    assert_eq!(
        harness.pending_orders(),
        1,
        "the key queued the switch: {}",
        harness.status()
    );
}

/// Shows the battle to `viewer` from now on, as the developer view's participant choice does.
fn present_to(harness: &mut Harness, viewer: PlayerId) {
    harness
        .app
        .world_mut()
        .resource_mut::<BattlePresentation>()
        .controlled = Some(viewer);
    harness.step(2);
    assert_eq!(
        harness.app.world().resource::<BattleFog>().viewer(),
        Some(viewer),
        "the fog follows the viewer"
    );
}

/// What the current frame published and presented of `unit`.
fn stance_frame(harness: &Harness, unit: EntityId) -> StanceFrame {
    let world = harness.app.world();
    let session = world.resource::<BattleSession>();
    let snapshot = session.frames.presented();
    let bubble = snapshot
        .bubbles
        .iter()
        .find(|bubble| bubble.entity_id == unit)
        .expect("the snapshot publishes the unit");
    let entity = *world
        .resource::<RenderEntityMap>()
        .0
        .get(&unit)
        .expect("the unit has a bubble");
    let track = world
        .get::<StanceTrack>(entity)
        .copied()
        .unwrap_or_default();
    StanceFrame {
        tick: snapshot.tick.0,
        stance: bubble.stance,
        concealment: bubble.concealment,
        state: world
            .resource::<HudModel>()
            .view
            .mode_states
            .iter()
            .find(|state| state.entity == unit)
            .copied(),
        view_tick: world.resource::<HudModel>().view.tick.0,
        latest: {
            let latest = session.frames.current();
            let stance = latest
                .bubbles
                .iter()
                .find(|bubble| bubble.entity_id == unit)
                .map_or_else(StanceView::default, |bubble| bubble.stance);
            (latest.tick.0, stance)
        },
        morph: track.morph.map(|morph| (morph.look.effect, morph.progress)),
        stance_look: track.stance.map(|look| look.effect),
        cover: track.cover.map(|look| look.effect),
        drawn: world
            .get::<InheritedVisibility>(entity)
            .is_some_and(|visibility| visibility.get()),
    }
}

/// Runs the clock frame by frame from a switch just ordered until `unit` stands in its new
/// stance with its morph released (or, when `hidden`, until the switch is over), and returns
/// every frame's record.
fn watch_switch(harness: &mut Harness, unit: EntityId, hidden: bool) -> Vec<StanceFrame> {
    let mut frames = Vec::new();
    let mut standing = 0;
    let mut began = false;
    harness.resume();
    for _ in 0..SWITCH_FRAME_LIMIT {
        harness.step(1);
        let frame = stance_frame(harness, unit);
        began |= frame.stance.transition.is_some();
        frames.push(frame);
        let over = began && frame.stance.transition.is_none();
        if over && (hidden || frame.morph.is_none()) {
            standing += 1;
        }
        if standing > STANDING_FRAMES {
            harness.pause();
            assert_no_diagnostics(harness);
            return frames;
        }
    }
    panic!("the switch never settled: {frames:#?}");
}

/// The name of effect `id` in the vocabulary.
fn effect_name(harness: &Harness, id: EffectId) -> String {
    harness
        .app
        .world()
        .resource::<EffectCatalog>()
        .effect(id)
        .map_or_else(|| format!("{id:?}"), |effect| effect.name.clone())
}

/// Seconds the vocabulary gives effect `id` on screen, if it states its own span.
fn effect_span(harness: &Harness, id: EffectId) -> Option<f32> {
    harness
        .app
        .world()
        .resource::<EffectCatalog>()
        .effect(id)
        .and_then(|effect| effect.span)
}

/// The looks `unit_type`'s presentation block, with the vocabulary's defaults, gives its mode
/// `mode`.
fn mode_looks(harness: &Harness, unit_type: UnitTypeId, mode: u8) -> ModeEffectIds {
    harness
        .app
        .world()
        .resource::<UnitEffectTables>()
        .mode(unit_type, mode)
        .expect("every mode of every archetype has looks")
}

/// What one switch is expected to publish and present.
struct ExpectedSwitch {
    /// The mode entered (`None`: back to the default stance).
    entering: Option<u8>,
    /// The mode the unit stands in before the switch.
    from: Option<u8>,
    /// The morph held while the switch runs.
    morph: EffectId,
    /// The stance look held once it stands in the new stance.
    stance: Option<EffectId>,
    /// The concealment published once the switch is over.
    concealment: Concealment,
    /// Whether the viewer owns the unit, so its `PlayerView` lists the unit's mode state.
    own: bool,
}

/// Asserts the frames of the channel of a switch `expected`: the published stance and mode
/// state follow it, the published progress runs forward from near 0 to near 1, and the morph is
/// held on every frame with its presented progress never running back. Returns the last
/// presented progress.
fn assert_channel(harness: &Harness, channel: &[StanceFrame], expected: &ExpectedSwitch) -> f32 {
    let mut last_progress = 0.0_f32;
    let mut last_channel = 0.0_f32;
    for frame in channel {
        let tick = frame.tick;
        let transition = frame.stance.transition.expect("inside the channel");
        assert_eq!(transition.entering, expected.entering, "tick {tick}");
        assert_eq!(frame.stance.mode, expected.from, "tick {tick}");
        let published = channel_progress(&transition);
        assert!(
            published >= last_channel,
            "the published channel runs forward: {published} after {last_channel}"
        );
        last_channel = published;
        assert!(frame.drawn, "tick {tick}: the switching unit is drawn");
        let (morph, progress) = frame
            .morph
            .unwrap_or_else(|| panic!("tick {tick}: no morph held: {frame:?}"));
        assert_eq!(
            morph,
            expected.morph,
            "tick {tick}: {} held",
            effect_name(harness, morph)
        );
        assert!(
            progress >= last_progress,
            "the morph never runs back: {progress} after {last_progress}"
        );
        last_progress = progress;
        assert_eq!(frame.stance_look, None, "no stance look while it switches");
    }
    let first = channel[0].stance.transition.expect("in the channel");
    assert!(
        channel_progress(&first) < 0.1,
        "the channel is first published near its start"
    );
    assert!(
        last_channel > 0.85,
        "the channel is published to near its end: {last_channel}"
    );
    last_progress
}

/// Asserts that the owner's `PlayerView::mode_states` follows the switch into `entering` in
/// `frames`: on every frame whose view and latest snapshot describe the same tick, the unit is
/// listed as channelling exactly while the snapshot publishes the switch, and its settled mode is
/// the mode it enters while it channels and the mode it stands in otherwise. Both halves are
/// seen.
fn assert_mode_states(frames: &[StanceFrame], entering: Option<u8>) {
    let (mut channelling, mut standing) = (0, 0);
    for frame in frames {
        let (tick, stance) = frame.latest;
        if frame.view_tick != tick {
            continue;
        }
        let state = frame.state.expect("the viewer's mode unit is listed");
        assert_eq!(
            state.channeling,
            stance.transition.is_some(),
            "tick {tick}: {state:?} against {stance:?}"
        );
        let settled = stance
            .transition
            .map_or(stance.mode, |switch| switch.entering);
        assert_eq!(state.settled_mode, settled, "tick {tick}");
        if state.channeling {
            assert_eq!(state.settled_mode, entering, "tick {tick}");
            channelling += 1;
        } else {
            standing += 1;
        }
    }
    assert!(
        channelling > 0 && standing > 0,
        "the view is seen in and after the channel: {channelling} and {standing} frames"
    );
}

/// Asserts that `frames`, recorded by [`watch_switch`] while the viewer sees the unit, publish
/// and present the switch `expected`: the channel as [`assert_channel`] checks it; then the new
/// stance and concealment are published, the morph finishes on the client after the channel
/// when its span is longer (D-6), and it hands over to the stance look.
fn assert_switch(harness: &Harness, frames: &[StanceFrame], expected: &ExpectedSwitch) {
    let first = frames
        .iter()
        .position(|frame| frame.stance.transition.is_some())
        .expect("the switch is published");
    let end = first
        + frames[first..]
            .iter()
            .position(|frame| frame.stance.transition.is_none())
            .expect("the switch ends");
    let channel = &frames[first..end];
    let mut last_progress = assert_channel(harness, channel, expected);
    if expected.own {
        assert_mode_states(frames, expected.entering);
    }

    let after = &frames[end..];
    for frame in after {
        let tick = frame.tick;
        assert_eq!(frame.stance.mode, expected.entering, "tick {tick}");
        assert_eq!(frame.concealment, expected.concealment, "tick {tick}");
        if let Some((morph, progress)) = frame.morph {
            assert_eq!(morph, expected.morph, "the same morph settles");
            assert!(progress >= last_progress, "the settle never runs back");
            last_progress = progress;
            assert_eq!(frame.stance_look, None, "no stance look while it settles");
        }
    }
    let nominal = channel[0]
        .stance
        .transition
        .expect("in the channel")
        .nominal_ticks;
    let channel_seconds = f32::from(nominal) / f32::from(sim_types::TICKS_PER_SECOND);
    if effect_span(harness, expected.morph).is_some_and(|span| span > channel_seconds + 0.1) {
        assert!(
            after[0].morph.is_some(),
            "a morph longer than its channel settles on after it (D-6)"
        );
    }
    let last = after.last().expect("frames after the switch");
    assert_eq!(last.morph, None, "the morph is released once complete");
    assert_eq!(
        last.stance_look,
        expected.stance,
        "the stance look: {:?}",
        last.stance_look.map(|look| effect_name(harness, look))
    );
    assert!(last.drawn, "the unit is drawn in its new stance");
}

/// The morph, the stance look and the morph back of `unit_type`'s mode `mode` of `kind`, and the
/// concealment it publishes standing in it.
fn switch_looks(
    harness: &Harness,
    kind: ModeKind,
    unit_type: UnitTypeId,
    mode: u8,
) -> (EffectId, Option<EffectId>, EffectId, Concealment) {
    let looks = mode_looks(harness, unit_type, mode);
    let concealment = match kind {
        ModeKind::Burrow => {
            // The burrowed defaults (plan WP8, Amendment 11) reach the unit's mode.
            let defaults = harness
                .app
                .world()
                .resource::<UnitEffectTables>()
                .concealment();
            assert_eq!(looks.enter, defaults.burrow_enter);
            assert_eq!(looks.leave, defaults.burrow_leave);
            assert_eq!(looks.stance, defaults.burrowed);
            Concealment::Burrowed
        }
        ModeKind::Siege => Concealment::None,
    };
    let enter = looks
        .enter
        .or(looks.transition)
        .expect("entering the mode has a look");
    let leave = looks
        .leave
        .or(looks.transition)
        .expect("leaving the mode has a look");
    let stance = looks.stance.expect("standing in the mode has a look");
    eprintln!(
        "{kind:?}: enter {}, stance {}, leave {}",
        effect_name(harness, enter),
        effect_name(harness, stance),
        effect_name(harness, leave)
    );
    (enter, Some(stance), leave, concealment)
}

/// Q0070 and WP12: a unit burrowed and unburrowed, and a tank sieged and unsieged, by their keys
/// alone, publish their stance and mode state over the channel, and the effect system holds the
/// burrow's dig, mound and emerge and the siege's seal sweep and anchor at the right moments.
#[test]
fn burrow_and_siege_by_key_publish_their_stance_and_hold_their_looks() {
    let (mut harness, home) = skirmish_at_home();
    for (kind, distance) in [(ModeKind::Burrow, 4.0), (ModeKind::Siege, 7.0)] {
        let (unit_type, mode) = mode_unit(&harness, kind);
        let (enter, stance, leave, concealment) = switch_looks(&harness, kind, unit_type, mode);
        let unit = spawn_near(&mut harness, home, distance, unit_type, LOCAL_PLAYER);
        harness.select(unit);

        tap_mode_key(&mut harness, ModeDirection::Enter);
        let frames = watch_switch(&mut harness, unit, false);
        assert_switch(
            &harness,
            &frames,
            &ExpectedSwitch {
                entering: Some(mode),
                from: None,
                morph: enter,
                stance,
                concealment,
                own: true,
            },
        );
        assert_eq!(
            frames.last().expect("frames").cover,
            None,
            "{kind:?}: its owner sees it by its stance, with no cover look (Amendment 11)"
        );

        tap_mode_key(&mut harness, ModeDirection::Leave);
        let frames = watch_switch(&mut harness, unit, false);
        assert_switch(
            &harness,
            &frames,
            &ExpectedSwitch {
                entering: None,
                from: Some(mode),
                morph: leave,
                stance: None,
                concealment: Concealment::None,
                own: true,
            },
        );
        assert!(
            !frames.last().expect("frames").holds_a_look(),
            "{kind:?}: back in its default stance, nothing is held"
        );
    }
}

/// D-5 end to end: while the local player burrows and sieges by key where the opponent has no
/// sight, the opponent's presentation draws neither unit and holds none of their looks on any
/// frame; the local player's own presentation of the same units holds them (the control).
#[test]
fn the_opponent_sees_nothing_of_switches_under_its_fog() {
    let (mut harness, home) = skirmish_at_home();
    let (burrower_type, _) = mode_unit(&harness, ModeKind::Burrow);
    let (tank_type, _) = mode_unit(&harness, ModeKind::Siege);
    let burrower = spawn_near(&mut harness, home, 4.0, burrower_type, LOCAL_PLAYER);
    let tank = spawn_near(&mut harness, home, 7.0, tank_type, LOCAL_PLAYER);

    // Both switches are ordered by key, then the battle is shown to the opponent.
    harness.select(burrower);
    tap_mode_key(&mut harness, ModeDirection::Enter);
    run_mode_ticks(&mut harness, 1);
    harness.select(tank);
    tap_mode_key(&mut harness, ModeDirection::Enter);
    present_to(&mut harness, OPPONENT);
    for unit in [burrower, tank] {
        let frame = stance_frame(&harness, unit);
        assert!(!frame.drawn, "the opponent has no sight here: {frame:?}");
    }

    let frames = watch_switch(&mut harness, tank, true);
    let burrowed = stance_frame(&harness, burrower);
    assert_eq!(burrowed.concealment, Concealment::Burrowed, "{burrowed:?}");
    for frame in &frames {
        let tick = frame.tick;
        assert!(!frame.drawn, "tick {tick}: the tank under fog is drawn");
        assert!(
            !frame.holds_a_look(),
            "tick {tick}: a look is held on a unit under fog: {frame:?}"
        );
        assert_eq!(
            frame.state, None,
            "the opponent's view lists no foreign mode"
        );
    }
    assert!(
        frames.iter().any(|frame| frame.stance.transition.is_some()),
        "the switch ran while the opponent watched"
    );
    assert!(!burrowed.drawn && !burrowed.holds_a_look(), "{burrowed:?}");

    // The control: shown to its owner again, both stand in their modes with their looks.
    present_to(&mut harness, LOCAL_PLAYER);
    for unit in [burrower, tank] {
        let frame = stance_frame(&harness, unit);
        assert!(frame.drawn && frame.stance_look.is_some(), "{frame:?}");
    }
}

/// D-5 end to end, the opponent's side: an enemy spotter sees the burrow begin (the dig is held
/// while the digging unit is still in plain sight), loses the unit with all its looks when its
/// concealment flips, and a detector brings it back settled in its mound, with no cover look,
/// until the detector dies. A tank sieged by key in the spotter's sight shows the opponent its
/// seal sweep and anchor as it shows its owner.
#[test]
fn the_opponent_sees_a_burrowed_unit_only_where_it_detects_it() {
    let (mut harness, home) = skirmish_at_home();
    let (burrower_type, burrowed_mode) = mode_unit(&harness, ModeKind::Burrow);
    let (tank_type, siege_mode) = mode_unit(&harness, ModeKind::Siege);
    let burrower = spawn_near(&mut harness, home, 4.0, burrower_type, LOCAL_PLAYER);
    let tank = spawn_near(&mut harness, home, 7.0, tank_type, LOCAL_PLAYER);
    let at = harness.simulated_position(burrower);
    let spotter_type = spotter_type(&harness);
    let spotter = spawn_near(&mut harness, at, 3.0, spotter_type, OPPONENT);

    // The burrow, ordered by key and watched by the opponent.
    harness.select(burrower);
    tap_mode_key(&mut harness, ModeDirection::Enter);
    present_to(&mut harness, OPPONENT);
    let frames = watch_switch(&mut harness, burrower, true);
    let flip = frames
        .iter()
        .position(|frame| frame.concealment == Concealment::Burrowed)
        .expect("the burrow conceals the unit");
    let looks = mode_looks(&harness, burrower_type, burrowed_mode);
    let dig = looks.enter.expect("the burrow has a dig");
    let digging = &frames[..flip];
    assert!(
        digging
            .iter()
            .any(|frame| frame.drawn && frame.morph.map(|(look, _)| look) == Some(dig)),
        "the spotter sees the dig while the unit is in plain sight: {digging:#?}"
    );
    for frame in &frames[flip..] {
        let tick = frame.tick;
        assert!(!frame.drawn, "tick {tick}: undetected, yet drawn");
        assert!(
            !frame.holds_a_look(),
            "tick {tick}: a look is held on an undetected burrowed enemy: {frame:?}"
        );
    }
    assert!(
        harness.world().entities.contains_key(&spotter),
        "the spotter still stands"
    );

    // A detector of the opponent's brings it back, settled in its mound.
    let detector_type = detector_type(&harness);
    let detector = spawn_near(&mut harness, at, 3.0, detector_type, OPPONENT);
    harness.step(2);
    let seen = stance_frame(&harness, burrower);
    assert!(seen.drawn, "detected, it is drawn: {seen:?}");
    assert_eq!(seen.stance_look, looks.stance, "and shown in its mound");
    assert_eq!(seen.morph, None, "settled: the dig is not replayed");
    assert_eq!(
        seen.cover, None,
        "a detected burrowed unit has no cover look"
    );
    assert!(
        harness
            .app
            .world()
            .resource::<HudModel>()
            .view
            .revealed
            .contains(&burrower),
        "the opponent's view lists it as revealed"
    );

    harness
        .world_mut()
        .entities
        .get_mut(&detector)
        .expect("the detector stands")
        .hp = SimScalar::ZERO;
    run_mode_ticks(&mut harness, 3);
    let lost = stance_frame(&harness, burrower);
    assert!(
        !lost.drawn && !lost.holds_a_look(),
        "undetected again: {lost:?}"
    );

    // The siege, ordered by key, in the spotter's sight.
    present_to(&mut harness, LOCAL_PLAYER);
    harness.select(tank);
    tap_mode_key(&mut harness, ModeDirection::Enter);
    present_to(&mut harness, OPPONENT);
    let frames = watch_switch(&mut harness, tank, false);
    let (enter, stance, _, concealment) =
        switch_looks(&harness, ModeKind::Siege, tank_type, siege_mode);
    assert_switch(
        &harness,
        &frames,
        &ExpectedSwitch {
            entering: Some(siege_mode),
            from: None,
            morph: enter,
            stance,
            concealment,
            own: false,
        },
    );
}
