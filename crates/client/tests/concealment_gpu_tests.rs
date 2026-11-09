//! An undetected burrowed enemy on a real GPU: the screen shows nothing of it (plan
//! `2026-11-09-effect-visualization` WP3, D-5, register entry B-106).
//!
//! # Responsibility
//! Proves the fog's detection rule all the way to pixels. The production client plays the shipped
//! skirmish on a real window; its child stages three shots of one patch of open ground the player
//! sees: empty, then with an enemy unit burrowed there and no detector of the player near it, then
//! with the same unit no longer concealed. The parent compares the patch, the screen rectangle
//! around the unit's projected sphere, across the shots:
//! - empty against burrowed: at most [`TOLERATED_PIXELS`] pixels change, so the burrowed enemy is
//!   not drawn, nor its ring, health bar or anything else hanging from it;
//! - empty against unconcealed: at least [`CONTROL_SHARE`] of the patch changes, so the patch is
//!   where the unit stands and the shots would have shown it.
//!
//! # How it fits
//! The pixel test re-executes this test binary on the virtual GPU through `support/gpu_run.rs`
//! (charter §7, check C21), as `skirmish_match_tests.rs` does. Staging writes the world directly:
//! the burrowed unit stands in a catalog mode marked `is_burrowed`, as the end of its channel
//! leaves it, but without any weapon, so no spine or flash can change the patch
//! (`battle_harness::burrower` explains the stand-in used while the shipped burrower has no
//! media). The
//! headless matrix of who sees what lives in `battle_fog_tests.rs`.
//!
//! # Invariants verified
//! - The staging published what it claims: the bubble exists, is concealed as `Burrowed`, and the
//!   fog answers `Hidden` for it in the burrowed shot and `Shown` in the control shot.
//! - Each world change is published by a tick before the clock stops for its shot, however short
//!   a frame is (plan `2026-11-03-gpu-test-harness`, decision D9).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
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

use std::path::{Path, PathBuf};

use battle_harness::{assets, burrower, sim_position};

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use client::battle_pipeline::fog::{BattleFog, Presence};
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::bubble_shader::BubbleInstance;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::scene_controller::LiveBattleBubble;
use client::{ClientMode, LabState, RtsCamera};
use serde::{Deserialize, Serialize};
use sim_core::SimWorld;
use sim_map::CellFlags;
use sim_types::{Concealment, EntityId, PlayerId, UnitOrder};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the directory the child writes to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_CONCEALMENT_CHILD_OUT";
/// Name of the libtest entry the pixel test launches.
const CHILD_TEST: &str = "concealment_pose_child";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// File the child writes its [`PoseReport`] to.
const REPORT_FILE: &str = "pose.ron";

/// The viewer: the skirmish's first seat.
const VIEWER: PlayerId = PlayerId(0);
/// The enemy whose unit burrows.
const ENEMY: PlayerId = PlayerId(1);

/// Rendered frames between a published change and its capture.
const SETTLE_FRAMES: u32 = 3;
/// Frames the camera's look height must hold still before the first shot.
const STILL_CAMERA_FRAMES: u32 = 3;
/// Distances from the camera's target, in world units, at which the patch is sought.
const PATCH_DISTANCES: [f32; 5] = [5.0, 6.0, 7.0, 8.0, 4.0];
/// Clearance around the patch, in world units, from any unit, structure or resource node.
const PATCH_CLEARANCE: f32 = 3.5;
/// The patch is the square around the projected sphere, this many radii from its centre.
const PATCH_RADII: f32 = 1.3;

/// A pixel counts as changed when one of its channels moves by more than this, in levels of 255.
/// On 2026-11-09 the empty and the burrowed shots, whose world differs only by the hidden unit and
/// the ticks between them, differed by at most 0 levels in the patch on both radv and lavapipe,
/// and the unconcealed unit changed 582 (radv) and 585 (lavapipe) of 930 patch pixels by up to
/// 175 levels; the parent prints these figures on every run.
const CHANGE_LEVEL: u8 = 12;
/// Changed pixels tolerated in the patch between the empty and the burrowed shots (plan WP3).
const TOLERATED_PIXELS: usize = 2;
/// Share of the patch the unconcealed unit must change, so the patch is proven to hold it.
const CONTROL_SHARE: f64 = 0.2;

/// The three shots, in order.
const SHOTS: [&str; 3] = ["empty", "burrowed", "unconcealed"];

/// What the child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PoseReport {
    /// The patch in physical window pixels: `[left, top, right, bottom]`, right and bottom
    /// exclusive.
    patch: [u32; 4],
    /// The fog's answer for the unit in the burrowed and the unconcealed shot.
    presences: Vec<String>,
    /// The concealment each of those shots published for the unit.
    concealments: Vec<Concealment>,
}

#[test]
fn an_undetected_burrowed_enemy_changes_no_pixel() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("concealment pose");

    let report: PoseReport = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    assert_eq!(
        report.concealments,
        [Concealment::Burrowed, Concealment::None],
        "{report:?}"
    );
    assert_eq!(report.presences, ["Hidden", "Shown"], "{report:?}");
    let shot = |name: &str| -> TestResult<image::RgbImage> {
        Ok(image::open(directory.path().join(format!("{name}.png")))?.to_rgb8())
    };
    let [empty, burrowed, unconcealed] = [shot(SHOTS[0])?, shot(SHOTS[1])?, shot(SHOTS[2])?];
    let pixels = patch_pixels(report.patch, &empty);
    assert!(pixels > 100, "the patch is on screen: {report:?}");

    let hidden = compare(&empty, &burrowed, report.patch);
    let control = compare(&empty, &unconcealed, report.patch);
    eprintln!(
        "driver {}: patch {:?} of {pixels} pixels; empty vs burrowed: {} changed, largest \
         channel change {}; empty vs unconcealed: {} changed, largest {}",
        gpu_run::selected_driver().driver,
        report.patch,
        hidden.changed,
        hidden.largest,
        control.changed,
        control.largest
    );
    assert!(
        hidden.changed <= TOLERATED_PIXELS,
        "the undetected burrowed enemy changed {} pixels of its patch",
        hidden.changed
    );
    assert!(
        control.changed as f64 >= CONTROL_SHARE * pixels as f64,
        "the unconcealed unit changed only {} of {pixels} pixels: the patch is not where it stands",
        control.changed
    );
    Ok(())
}

/// Pixels of `patch` inside `image`.
fn patch_pixels(patch: [u32; 4], image: &image::RgbImage) -> usize {
    let [left, top, right, bottom] = clamp_patch(patch, image);
    ((right - left) * (bottom - top)) as usize
}

/// `patch` cut to the image's bounds.
fn clamp_patch(patch: [u32; 4], image: &image::RgbImage) -> [u32; 4] {
    let [left, top, right, bottom] = patch;
    [
        left.min(image.width()),
        top.min(image.height()),
        right.min(image.width()),
        bottom.min(image.height()),
    ]
}

/// How two captures differ over a patch.
struct Difference {
    /// Pixels with a channel changed by more than [`CHANGE_LEVEL`].
    changed: usize,
    /// The largest channel change seen, in levels of 255.
    largest: u8,
}

fn compare(a: &image::RgbImage, b: &image::RgbImage, patch: [u32; 4]) -> Difference {
    let [left, top, right, bottom] = clamp_patch(patch, a);
    let mut difference = Difference {
        changed: 0,
        largest: 0,
    };
    for y in top..bottom {
        for x in left..right {
            let (p, q) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
            let change = (0..3).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0);
            difference.largest = difference.largest.max(change);
            if change > CHANGE_LEVEL {
                difference.changed += 1;
            }
        }
    }
    difference
}

// ---------------------------------------------------------------------------------------------
// The child
// ---------------------------------------------------------------------------------------------

/// What the child is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Phase {
    /// The battle has not committed or published its second tick.
    #[default]
    Boot,
    /// The camera's look height has held at `height` for `still` frames.
    Booting { still: u32, height: f32 },
    /// The world was changed for shot `shot` while tick `since` was published; the clock runs
    /// until a later tick is.
    Publishing { shot: usize, since: u32 },
    /// The change is published and the clock stopped; `frames` have been rendered since.
    Settling { shot: usize, frames: u32 },
    /// Shot `shot` was requested and is being written.
    Capturing { shot: usize },
    /// Every shot is written.
    Done,
}

/// The child's staging.
#[derive(Resource, Default)]
struct Staging {
    directory: PathBuf,
    phase: Phase,
    /// Map position of the patch's centre, in world units.
    spot: Vec2,
    /// The burrowed enemy, once spawned.
    unit: Option<EntityId>,
    report: PoseReport,
}

/// The latest tick the session published, or 0 before the first.
fn published_tick(session: &BattleSession) -> u32 {
    session.published().map_or(0, |published| published.tick)
}

/// Whether the 3 by 3 cells around `point` are open, walkable ground.
fn open(world: &SimWorld, point: Vec2) -> bool {
    let map = &world.map;
    (-1..=1).all(|x: i32| {
        (-1..=1).all(|y: i32| {
            let probe = point + Vec2::new(x as f32, y as f32);
            map.world_to_cell(sim_position(probe))
                .ok()
                .and_then(|(cx, cy)| map.cell(cx, cy).ok())
                .is_some_and(|cell| {
                    cell.flags.contains(CellFlags::GROUND_PATHABLE)
                        && !cell.flags.intersects(
                            CellFlags::OCCUPIED_BY_STRUCTURE | CellFlags::UNPASSABLE_CLIFF,
                        )
                })
        })
    })
}

/// Open ground in the viewer's sight near `around`, clear of every unit and node.
fn find_spot(world: &SimWorld, fog: &BattleFog, around: Vec2) -> Vec2 {
    let clear = |point: Vec2| {
        let units = world.entities.values().all(|unit| {
            let at = Vec2::new(unit.position.x.to_f32(), unit.position.y.to_f32());
            at.distance(point) > PATCH_CLEARANCE
        });
        let nodes = world.map.resources().iter().all(|node| {
            let at = Vec2::new(node.pos.x.to_f32(), node.pos.y.to_f32());
            at.distance(point) > PATCH_CLEARANCE
        });
        units && nodes
    };
    PATCH_DISTANCES
        .iter()
        .flat_map(|distance| {
            (0..16).map(move |step| {
                around + Vec2::from_angle(step as f32 * std::f32::consts::TAU / 16.0) * *distance
            })
        })
        .find(|point| open(world, *point) && clear(*point) && fog.sees(point.x, point.y))
        .expect("the start area has open ground in sight")
}

/// Makes the change shot `shot` shows: none for the empty ground, the burrowed enemy, then the
/// same unit no longer concealed.
fn change_world(staging: &mut Staging, session: &mut BattleSession, shot: usize) {
    if shot == 0 {
        let world = session.world().expect("a ready session has a world");
        let own: Vec<EntityId> = world
            .entities
            .values()
            .filter(|unit| unit.owner == VIEWER && !unit.is_immobile)
            .map(|unit| unit.id)
            .collect();
        let (world, queue) = session.world_and_commands();
        // Workers walking through the patch would change it between the shots.
        queue
            .submit_selection(
                world.expect("a ready session has a world"),
                VIEWER,
                &own,
                UnitOrder::HoldPosition,
                false,
            )
            .expect("the client admits Hold Position for the viewer's own units");
        return;
    }
    let world = session.world_mut().expect("a ready session has a world");
    if shot == 1 {
        let (unit_type, mode) = burrower(world, &assets());
        let id = world
            .spawn_unit(unit_type, ENEMY, sim_position(staging.spot))
            .expect("open ground");
        let unit = world.entities.get_mut(&id).expect("just spawned");
        unit.is_cloaked = true;
        unit.active_mode = Some(mode);
        unit.is_immobile = true;
        unit.unpushable = true;
        staging.unit = Some(id);
    } else {
        let id = staging.unit.expect("the burrowed shot spawned the unit");
        world.entities.get_mut(&id).expect("it stands").is_cloaked = false;
    }
}

/// Records the fog's answer and the patch for the unit, once its bubble is on the field.
fn record_unit(
    staging: &mut Staging,
    fog: &BattleFog,
    bubbles: &Query<'_, '_, (&BubbleInstance, &GlobalTransform), With<LiveBattleBubble>>,
    camera: (&Camera, &GlobalTransform),
) {
    let id = staging.unit.expect("the burrowed shot spawned the unit");
    let (bubble, transform) = bubbles
        .iter()
        .find(|(bubble, _)| bubble.0.entity_id == id)
        .expect("the unit has a bubble");
    let presence: Presence = fog.presence(&bubble.0);
    staging.report.presences.push(format!("{presence:?}"));
    staging.report.concealments.push(bubble.0.concealment);
    if staging.report.presences.len() == 1 {
        staging.report.patch = patch(camera, transform.translation(), bubble.0.radius);
    }
}

/// The square of physical window pixels around a sphere at `centre` of `radius`, in Bevy world
/// space.
fn patch(camera: (&Camera, &GlobalTransform), centre: Vec3, radius: f32) -> [u32; 4] {
    let (camera, transform) = camera;
    let rectangle = camera
        .physical_viewport_rect()
        .expect("the camera renders through a viewport");
    let size = rectangle.size().as_vec2();
    let clip_from_world = camera.clip_from_view() * transform.compute_matrix().inverse();
    let to_window = |point: Vec3| {
        let ndc = clip_from_world.project_point3(point);
        rectangle.min.as_vec2() + Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * size
    };
    let middle = to_window(centre);
    let right = *transform.right();
    let reach = to_window(centre + right * radius * PATCH_RADII).distance(middle);
    let low = (middle - Vec2::splat(reach)).max(Vec2::ZERO);
    let high = middle + Vec2::splat(reach);
    [
        low.x as u32,
        low.y as u32,
        high.x.ceil() as u32,
        high.y.ceil() as u32,
    ]
}

/// Advances the staging by one frame: boot, then for each shot change the world, publish the
/// change, stop the clock, settle and capture.
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "a Bevy system with one declared parameter per dependency"
)]
fn stage_shots(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, Staging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    fog: Option<Res<'_, BattleFog>>,
    bubbles: Query<'_, '_, (&BubbleInstance, &GlobalTransform), With<LiveBattleBubble>>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform, &RtsCamera)>,
) {
    let Some(fog) = fog else {
        panic!("the skirmish presents a player, so it keeps a fog");
    };
    if !session.is_ready() {
        return;
    }
    let Ok((camera, camera_transform, rts)) = cameras.get_single() else {
        return;
    };
    let tick = published_tick(&session);
    staging.phase = match staging.phase {
        Phase::Boot if tick < 2 => Phase::Boot,
        Phase::Boot => Phase::Booting {
            still: 0,
            height: rts.look_at_y,
        },
        Phase::Booting { still, height } if still < STILL_CAMERA_FRAMES => Phase::Booting {
            still: if (rts.look_at_y - height).abs() < 1e-4 {
                still + 1
            } else {
                0
            },
            height: rts.look_at_y,
        },
        Phase::Booting { .. } => {
            let world = session.world().expect("a ready session has a world");
            staging.spot = find_spot(world, &fog, rts.target);
            change_world(&mut staging, &mut session, 0);
            lab.battle_paused = false;
            Phase::Publishing {
                shot: 0,
                since: tick,
            }
        }
        Phase::Publishing { shot, since } if tick <= since => Phase::Publishing { shot, since },
        Phase::Publishing { shot, .. } => {
            lab.battle_paused = true;
            Phase::Settling { shot, frames: 0 }
        }
        Phase::Settling { shot, frames } if frames < SETTLE_FRAMES => Phase::Settling {
            shot,
            frames: frames + 1,
        },
        Phase::Settling { shot, .. } => {
            if shot > 0 {
                record_unit(&mut staging, &fog, &bubbles, (camera, camera_transform));
            }
            capture(&mut commands, &staging.directory, shot);
            Phase::Capturing { shot }
        }
        Phase::Capturing { shot } => Phase::Capturing { shot },
        Phase::Done => Phase::Done,
    };
}

/// Requests shot `shot`; the observer writes it and moves the staging on.
fn capture(commands: &mut Commands<'_, '_>, directory: &Path, shot: usize) {
    let path = directory.join(format!("{}.png", SHOTS[shot]));
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<'_, ScreenshotCaptured>,
              mut staging: ResMut<'_, Staging>,
              mut session: ResMut<'_, BattleSession>,
              mut lab: ResMut<'_, LabState>,
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
            let next = shot + 1;
            if next == SHOTS.len() {
                let text = ron::to_string(&staging.report).expect("plain data");
                std::fs::write(staging.directory.join(REPORT_FILE), text)
                    .expect("the parent's directory is writable");
                staging.phase = Phase::Done;
                exit.send(AppExit::Success);
                return;
            }
            let since = published_tick(&session);
            change_world(&mut staging, &mut session, next);
            lab.battle_paused = false;
            staging.phase = Phase::Publishing { shot: next, since };
        },
    );
}

/// The child of [`an_undetected_burrowed_enemy_changes_no_pixel`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn concealment_pose_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    let mut app = client::build_graphical_app_off_main_thread(
        ClientMode::Skirmish,
        Some(CHILD_FRAME_LIMIT),
        SKIRMISH_SCENARIO.as_ref(),
    )
    .expect("the shipped assets must start the client");
    app.insert_resource(Staging {
        directory: Path::new(&directory).to_path_buf(),
        ..Staging::default()
    })
    .add_systems(Update, stage_shots.in_set(BattleSet::Present));
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}
