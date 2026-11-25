//! A selected structure's rally on a real GPU: the reticles and the chained lines are drawn while
//! it is selected and gone once it is not (plans `2026-11-23-production-queue` D-10, WP5, and
//! `2026-11-25-rallies` D-8, WP5).
//!
//! # Responsibility
//! Proves the rally display all the way to pixels. The production client plays the shipped
//! skirmish on a real window; its child gives the local base a rally route of two waypoints on
//! open ground in sight and stages three shots with the clock stopped: the base not selected,
//! selected, and not selected again. The parent compares four patches, the screen squares around
//! the two projected waypoints (the reticles), around a point three quarters of the way from the
//! base to the first waypoint (the first line), and around the midpoint between the waypoints
//! (the chained line):
//! - unselected against selected: at least [`RETICLE_SHARE`] of each reticle's patch and at
//!   least [`LINE_PIXELS`] pixels of each line's patch change, so every waypoint and the chain
//!   are drawn where the simulation's rally resolves;
//! - unselected against deselected: at most [`TOLERATED_PIXELS`] pixels of any patch change, so
//!   all are hidden again.
//!
//! # How it fits
//! The pixel test re-executes this test binary on the virtual GPU through `support/gpu_run.rs`
//! (charter §7, check C21), as `concealment_gpu_tests.rs` does, so it runs on radv locally and on
//! lavapipe in CI. Staging writes the world and the selection directly; the right click that sets
//! a rally is proved headless in `battle_context_order_tests.rs`, and the pool's transforms there
//! too.
//!
//! # Invariants verified
//! - The rally is published by a tick before the clock stops, and the shots differ only by the
//!   selection: the viewer's units hold position and no tick runs between the shots.
//! - The patches lie on the screen and away from the base, whose own selection ring changes with
//!   the selection.

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

use battle_harness::sim_position;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::battle_selection::BattleSelection;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::{ClientMode, LabState, RtsCamera};
use serde::{Deserialize, Serialize};
use sim_core::SimWorld;
use sim_map::CellFlags;
use sim_types::{EntityId, PlayerId, RallyRoute, RallyTarget, UnitOrder};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the directory the child writes to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_RALLY_CHILD_OUT";
/// Name of the libtest entry the pixel test launches.
const CHILD_TEST: &str = "rally_display_child";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// File the child writes its [`RallyReport`] to.
const REPORT_FILE: &str = "rally.ron";

/// The viewer: the skirmish's first seat.
const VIEWER: PlayerId = PlayerId(0);

/// Rendered frames between a change and its capture.
const SETTLE_FRAMES: u32 = 3;
/// Frames the camera's look height must hold still before staging.
const STILL_CAMERA_FRAMES: u32 = 3;
/// Distances from the base, in world units, at which the rally point is sought.
const RALLY_DISTANCES: [f32; 4] = [9.0, 10.0, 8.0, 11.0];
/// Distances from the base, in world units, at which the second waypoint is sought: the first's,
/// since the ground in sight around the start is no wider.
const SECOND_DISTANCES: [f32; 4] = RALLY_DISTANCES;
/// Least and most distance, in world units, between the two waypoints, so their reticles and
/// the chained line's patch stay apart, and the chain's midpoint off the first line.
const WAYPOINT_GAP: (f32, f32) = (4.0, 8.0);
/// Clearance around the rally point, in world units, from any unit and resource node.
const RALLY_CLEARANCE: f32 = 2.5;
/// Half the side of the reticle's patch, in world units: the reticle is 1.6 wide.
const RETICLE_HALF: f32 = 0.5;
/// Half the side of the line's patch, in world units: the line is 0.08 wide.
const LINE_HALF: f32 = 0.3;
/// Where along the line, from the base, its patch is taken.
const LINE_FRACTION: f32 = 0.75;

/// A pixel counts as changed when one of its channels moves by more than this, in levels of 255.
/// On 2026-11-23 on radv the unselected and deselected shots differed by 0 pixels in both
/// patches; the selected shot changed 103 of the reticle patch's 900 pixels by up to 138 levels
/// and 52 of the line patch's pixels by up to 85. The parent prints these figures on every run.
const CHANGE_LEVEL: u8 = 12;
/// Share of the reticle's patch the selected shot must change. The reticle is the selection
/// ring's material, an annulus with a clear middle, so it covers about a tenth of its patch.
const RETICLE_SHARE: f64 = 0.05;
/// Pixels of the line's patch the selected shot must change.
const LINE_PIXELS: usize = 4;
/// Changed pixels tolerated in either patch between the two unselected shots.
const TOLERATED_PIXELS: usize = 2;

/// The three shots, in order.
const SHOTS: [&str; 3] = ["unselected", "selected", "deselected"];

/// What the child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct RallyReport {
    /// The reticle's patch in physical window pixels: `[left, top, right, bottom]`, right and
    /// bottom exclusive.
    reticle: [u32; 4],
    /// The line's patch, likewise.
    line: [u32; 4],
    /// The second waypoint's reticle patch, likewise.
    second: [u32; 4],
    /// The chained line's patch, between the waypoints, likewise.
    chain: [u32; 4],
    /// Whether the view published the base's rally at the staged point.
    published: bool,
}

#[test]
fn a_selected_structures_rally_is_drawn_and_hidden_when_deselected() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("rally display");

    let report: RallyReport = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    assert!(report.published, "{report:?}");
    let shot = |name: &str| -> TestResult<image::RgbImage> {
        Ok(image::open(directory.path().join(format!("{name}.png")))?.to_rgb8())
    };
    let [unselected, selected, deselected] = [shot(SHOTS[0])?, shot(SHOTS[1])?, shot(SHOTS[2])?];
    let reticle_pixels = patch_pixels(report.reticle, &unselected);
    assert!(
        reticle_pixels > 50,
        "the reticle's patch is on screen: {report:?}"
    );
    assert!(
        patch_pixels(report.line, &unselected) > 9,
        "the line's patch is on screen: {report:?}"
    );

    for patch in [report.second, report.chain] {
        assert!(
            patch_pixels(patch, &unselected) > 9,
            "the second waypoint's patches are on screen: {report:?}"
        );
    }
    let second_pixels = patch_pixels(report.second, &unselected);
    let second_drawn = compare(&unselected, &selected, report.second);
    let chain_drawn = compare(&unselected, &selected, report.chain);
    let second_hidden = compare(&unselected, &deselected, report.second);
    let chain_hidden = compare(&unselected, &deselected, report.chain);
    eprintln!(
        "second waypoint patch {:?} of {second_pixels} pixels, chain patch {:?}; selected: \
         second {} changed (largest {}), chain {} changed (largest {}); deselected: second {}, \
         chain {}",
        report.second,
        report.chain,
        second_drawn.changed,
        second_drawn.largest,
        chain_drawn.changed,
        chain_drawn.largest,
        second_hidden.changed,
        chain_hidden.changed
    );
    assert!(
        second_drawn.changed as f64 >= RETICLE_SHARE * second_pixels as f64,
        "the second reticle changed only {} of {second_pixels} pixels",
        second_drawn.changed
    );
    assert!(
        chain_drawn.changed >= LINE_PIXELS,
        "the chained line changed only {} pixels",
        chain_drawn.changed
    );
    assert!(
        second_hidden.changed <= TOLERATED_PIXELS && chain_hidden.changed <= TOLERATED_PIXELS,
        "deselected, the second waypoint still changed {} and {} pixels",
        second_hidden.changed,
        chain_hidden.changed
    );
    let reticle_drawn = compare(&unselected, &selected, report.reticle);
    let line_drawn = compare(&unselected, &selected, report.line);
    let reticle_hidden = compare(&unselected, &deselected, report.reticle);
    let line_hidden = compare(&unselected, &deselected, report.line);
    eprintln!(
        "driver {}: reticle patch {:?} of {reticle_pixels} pixels, line patch {:?}; selected: \
         reticle {} changed (largest {}), line {} changed (largest {}); deselected: reticle {}, \
         line {}",
        gpu_run::selected_driver().driver,
        report.reticle,
        report.line,
        reticle_drawn.changed,
        reticle_drawn.largest,
        line_drawn.changed,
        line_drawn.largest,
        reticle_hidden.changed,
        line_hidden.changed
    );
    assert!(
        reticle_drawn.changed as f64 >= RETICLE_SHARE * reticle_pixels as f64,
        "the reticle changed only {} of {reticle_pixels} pixels at the rally point",
        reticle_drawn.changed
    );
    assert!(
        line_drawn.changed >= LINE_PIXELS,
        "the line changed only {} pixels",
        line_drawn.changed
    );
    assert!(
        reticle_hidden.changed <= TOLERATED_PIXELS && line_hidden.changed <= TOLERATED_PIXELS,
        "deselected, the rally still changed {} and {} pixels",
        reticle_hidden.changed,
        line_hidden.changed
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
    /// The rally was set while tick `since` was published; the clock runs until a later tick is.
    Publishing { since: u32 },
    /// The clock is stopped and shot `shot`'s selection applied; `frames` rendered since.
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
    /// The viewer's base.
    base: Option<EntityId>,
    /// The base's centre, the first waypoint and the second, map positions in world units.
    from: Vec2,
    to: Vec2,
    then: Vec2,
    report: RallyReport,
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

/// Open ground in the viewer's sight at one of `distances` from `base`, clear of units and nodes,
/// that `also` accepts.
fn find_rally(
    world: &SimWorld,
    fog: &BattleFog,
    base: Vec2,
    distances: [f32; 4],
    also: impl Fn(Vec2) -> bool,
) -> Vec2 {
    let clear = |point: Vec2| {
        let units = world.entities.values().all(|unit| {
            let at = Vec2::new(unit.position.x.to_f32(), unit.position.y.to_f32());
            at.distance(point) > RALLY_CLEARANCE || at == base
        });
        let nodes = world.map.resources().iter().all(|node| {
            let at = Vec2::new(node.pos.x.to_f32(), node.pos.y.to_f32());
            at.distance(point) > RALLY_CLEARANCE
        });
        units && nodes
    };
    distances
        .iter()
        .flat_map(|distance| {
            (0..16).map(move |step| {
                base + Vec2::from_angle(step as f32 * std::f32::consts::TAU / 16.0) * *distance
            })
        })
        .find(|point| {
            open(world, *point) && clear(*point) && fog.sees(point.x, point.y) && also(*point)
        })
        .expect("the start area has open ground in sight")
}

/// Holds the viewer's units and gives its base a rally at open ground.
fn stage_rally(staging: &mut Staging, session: &mut BattleSession, fog: &BattleFog) {
    let world = session.world().expect("a ready session has a world");
    let base = world
        .entities
        .values()
        .find(|entity| {
            entity.owner == VIEWER
                && world
                    .catalog
                    .get(entity.unit_type_id)
                    .is_ok_and(|arch| !arch.trains_units.is_empty())
        })
        .map(|entity| entity.id)
        .expect("the viewer has a production structure");
    let from = Vec2::new(
        world.entities[&base].position.x.to_f32(),
        world.entities[&base].position.y.to_f32(),
    );
    let to = find_rally(world, fog, from, RALLY_DISTANCES, |_| true);
    let then = find_rally(world, fog, from, SECOND_DISTANCES, |point| {
        (WAYPOINT_GAP.0..=WAYPOINT_GAP.1).contains(&point.distance(to))
    });
    let own: Vec<EntityId> = world
        .entities
        .values()
        .filter(|unit| unit.owner == VIEWER && !unit.is_immobile)
        .map(|unit| unit.id)
        .collect();
    let (world, queue) = session.world_and_commands();
    // Workers walking across a patch would change it between the shots.
    queue
        .submit_selection(
            world.expect("a ready session has a world"),
            VIEWER,
            &own,
            UnitOrder::HoldPosition,
            false,
        )
        .expect("the client admits Hold Position for the viewer's own units");
    let world = session.world_mut().expect("a ready session has a world");
    world
        .entities
        .get_mut(&base)
        .expect("the base stands")
        .rallies = vec![RallyRoute {
        slot: 0,
        waypoints: vec![
            RallyTarget::Point(sim_position(to)),
            RallyTarget::Point(sim_position(then)),
        ],
    }];
    staging.base = Some(base);
    staging.from = from;
    staging.to = to;
    staging.then = then;
}

/// The square of physical window pixels around the ground point `at` (map position), `half`
/// world units each way along the camera's right.
fn patch(camera: (&Camera, &GlobalTransform), world: &SimWorld, at: Vec2, half: f32) -> [u32; 4] {
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
    let ground = Vec3::new(at.x, world.map.terrain_altitude_at(sim_position(at)), at.y);
    let middle = to_window(ground);
    let reach = to_window(ground + *transform.right() * half).distance(middle);
    let low = (middle - Vec2::splat(reach)).max(Vec2::ZERO);
    let high = middle + Vec2::splat(reach);
    [
        low.x as u32,
        low.y as u32,
        high.x.ceil() as u32,
        high.y.ceil() as u32,
    ]
}

/// Records the patches and whether the view published the rally.
fn record(staging: &mut Staging, session: &BattleSession, camera: (&Camera, &GlobalTransform)) {
    let world = session.world().expect("a ready session has a world");
    let base = staging.base.expect("staged");
    let point = world.rally_point(base, 0);
    staging.report.published = point.is_some_and(|point| {
        Vec2::new(point.x.to_f32(), point.y.to_f32()).distance(staging.to) < 1e-3
    });
    staging.report.reticle = patch(camera, world, staging.to, RETICLE_HALF);
    let along = staging.from + (staging.to - staging.from) * LINE_FRACTION;
    staging.report.line = patch(camera, world, along, LINE_HALF);
    staging.report.second = patch(camera, world, staging.then, RETICLE_HALF);
    let middle = (staging.to + staging.then) * 0.5;
    staging.report.chain = patch(camera, world, middle, LINE_HALF);
}

/// Applies shot `shot`'s selection: the base selected only in the second.
fn select_for(selection: &mut BattleSelection, base: EntityId, shot: usize) {
    if shot == 1 {
        selection.select_only(base);
    } else {
        selection.clear();
    }
}

/// Advances the staging by one frame: boot, set the rally, publish it, stop the clock, then for
/// each shot apply its selection, settle and capture.
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
    mut selection: ResMut<'_, BattleSelection>,
    fog: Option<Res<'_, BattleFog>>,
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
            stage_rally(&mut staging, &mut session, &fog);
            selection.clear();
            lab.battle_paused = false;
            Phase::Publishing { since: tick }
        }
        Phase::Publishing { since } if tick <= since + 1 => Phase::Publishing { since },
        Phase::Publishing { .. } => {
            lab.battle_paused = true;
            record(&mut staging, &session, (camera, camera_transform));
            Phase::Settling { shot: 0, frames: 0 }
        }
        Phase::Settling { shot, frames } if frames < SETTLE_FRAMES => Phase::Settling {
            shot,
            frames: frames + 1,
        },
        Phase::Settling { shot, .. } => {
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
              mut selection: ResMut<'_, BattleSelection>,
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
            let base = staging.base.expect("staged");
            select_for(&mut selection, base, next);
            staging.phase = Phase::Settling {
                shot: next,
                frames: 0,
            };
        },
    );
}

/// The child of [`a_selected_structures_rally_is_drawn_and_hidden_when_deselected`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn rally_display_child() {
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
