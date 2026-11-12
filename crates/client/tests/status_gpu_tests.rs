//! Status and death looks on a real GPU (plan `2026-11-09-effect-visualization` WP9).
//!
//! # Responsibility
//! Proves the WP9 looks all the way to pixels in the production client playing the shipped
//! skirmish. Two children each stage scenes on open ground the player sees:
//! - **Poses** ([`status_pose_child`]): empty ground; one of the player's units standing; the
//!   same unit with its barrier raised and half spent, with and without the effect layers drawn;
//!   with the barrier almost spent; and with the barrier down. The parent measures the plan's
//!   "barrier" pose: the outer shell (the pixels in a ring just outside the bubble's outline)
//!   is drawn only while `barrier_ratio > 0`. It changes at least [`SHELL_RING_SHARE`] of that
//!   ring with half the barrier left and at least [`SHELL_LOW_PIXELS`] pixels with a twentieth
//!   left, and at most [`TOLERATED_PIXELS`] once the barrier is down or without its layers.
//! - **A battle** ([`status_battle_child`]): at once, one of the player's units stimmed and
//!   running, one slowed and walking, one charging, one with a barrier under an enemy's fire,
//!   and three whose blocks name a death effect, killed a second in. The child samples the
//!   units' looks every frame and writes a picture of each subject every frame (40 ms steps)
//!   and the whole window at a few moments. The parent checks that each status's look is held
//!   exactly while its status is published, that the stim's rim beats, that the charge lays its
//!   dust, that every absorbed hit is answered by at most one flash per beat, and that a unit
//!   whose block names a death draws its effect and no legacy glass shards.
//!
//! With `SPACE_SCUFFLE_STATUS_EVIDENCE_DIR` set, the parents copy every picture there, for the
//! review sheets.
//!
//! # How it fits
//! Each child re-executes this test binary through `support/gpu_run.rs` (charter §7, C21), as
//! `stance_gpu_tests.rs` does. Every unit is chosen by catalog properties (a capability, a
//! presentation block, media), never by name (C5); a unit given a barrier for the poses gets a
//! copy of the shipped barrier in the test world's catalog only.
//!
//! # Invariants verified
//! - The staging published what it claims (the barrier's state and fraction, each status).
//! - Each world change is published by a tick before the clock stops for its shot.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::needless_pass_by_value,
    clippy::too_many_arguments
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
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use battle_harness::{assets, presentable, sim_position};

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::render::RenderApp;
use bevy::time::TimeUpdateStrategy;
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::bubble_shader::{BubbleDomeMaterial, BubbleInstance};
use client::effects::catalog::{status_index, EffectCatalog, UnitEffectTables};
use client::effects::lab::driver::track_render_idle;
use client::effects::lab::LabRenderIdle;
use client::effects::runtime::EffectRuntime;
use client::effects::stance::{status_shown, StanceTrack};
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::scene_controller::LiveBattleBubble;
use client::vfx::ShatterShard;
use client::{ClientMode, LabState, RtsCamera};
use serde::{Deserialize, Serialize};
use sim_core::entity::{ActiveBuff, OVERCLOCK_BUFF_ID};
use sim_core::upgrades::ATTACK_DASH_ABILITY_SLUG;
use sim_core::SimWorld;
use sim_map::CellFlags;
use sim_types::{
    EntityId, MoverType, PlayerId, PlayerSlot, SimScalar, StatusKind, UnitOrder, UnitTypeId,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the directory a child writes to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_STATUS_CHILD_OUT";
/// Environment variable naming a directory the parents copy their pictures to.
const EVIDENCE_VAR: &str = "SPACE_SCUFFLE_STATUS_EVIDENCE_DIR";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// File a child writes its report to.
const REPORT_FILE: &str = "report.ron";

/// The viewer: the skirmish's first seat.
const VIEWER: PlayerId = PlayerId(0);
/// The other side.
const ENEMY: PlayerId = PlayerId(1);

/// Frames the camera's look height must hold still before the first shot.
const STILL_CAMERA_FRAMES: u32 = 3;
/// Rendered frames, at least, between a stopped clock and its capture.
const SETTLE_FRAMES: u32 = 3;
/// Seconds of `Time<Virtual>`, at least, between a stopped clock and its capture: a held look
/// comes in, or erodes out, within half a second.
const SETTLE_SECONDS: f64 = 0.9;
/// Distances from the camera's target, in world units, at which the spots are sought.
const SPOT_DISTANCES: [f32; 13] = [
    4.0, 5.0, 6.0, 7.0, 8.0, 3.0, 9.0, 10.0, 11.0, 3.5, 4.5, 5.5, 6.5,
];
/// Clearance around a spot, in world units, from any unit, structure, node or other spot.
const SPOT_CLEARANCE: f32 = 3.4;
/// Distance between two subjects of the battle, world units: their pictures do not overlap.
const SUBJECT_APART: f32 = 3.6;
/// Window pixels a subject's spot keeps from the window's sides and top.
const SCREEN_MARGIN: f32 = 110.0;
/// Share of the window's height above which a subject's spot lies (the HUD is below).
const SCREEN_BOTTOM_SHARE: f32 = 0.7;
/// Presentation time each frame of a child advances: one simulation tick.
const FRAME_STEP: Duration = Duration::from_millis(40);
/// Render frames in a row with no pipeline waiting before a picture is taken.
const IDLE_FRAMES: u32 = 8;

/// A pixel counts as changed when one of its channels moves by more than this, in levels.
const CHANGE_LEVEL: u8 = 8;
/// Changed pixels tolerated where nothing may be drawn (plan WP3's rule).
const TOLERATED_PIXELS: usize = 2;
/// The ring just outside a bubble's outline where only its outer shell draws, in radii of its
/// screen disc: clear of the bubble's own anti-aliased edge, inside the shell's 1.25 radii.
const SHELL_RING: (f32, f32) = (1.08, 1.3);
/// With half its barrier left, the outer shell changes at least this share of that ring.
const SHELL_RING_SHARE: f64 = 0.25;
/// With a twentieth of its barrier left, the outer shell still changes this many pixels of it.
const SHELL_LOW_PIXELS: usize = 40;
/// The barrier's fraction in the "up" and "low" shots.
const RATIO_UP: f32 = 0.5;
/// See [`RATIO_UP`].
const RATIO_LOW: f32 = 0.05;
/// Ticks a staged barrier stays raised: longer than any staging.
const RAISED_TICKS: u16 = 5_000;

/// The pose child's shots, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Shot {
    Empty,
    Standing,
    BarrierUp,
    BarrierUpBare,
    BarrierLow,
    BarrierDown,
}

const SHOTS: [Shot; 6] = [
    Shot::Empty,
    Shot::Standing,
    Shot::BarrierUp,
    Shot::BarrierUpBare,
    Shot::BarrierLow,
    Shot::BarrierDown,
];

impl Shot {
    fn file(self) -> String {
        format!("pose_{self:?}.png").to_lowercase()
    }
}

/// Where a unit stood in a shot and how the camera saw it.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct UnitView {
    /// The body's centre on screen, physical pixels.
    centre_px: [f32; 2],
    /// The body's radius on screen, pixels.
    radius_px: f32,
}

/// What a shot published for the staged unit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Published {
    shot: Shot,
    barrier_up: bool,
    barrier_ratio: f32,
    /// Whether the unit's track holds its barrier look.
    held: bool,
}

/// What the pose child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PoseReport {
    unit: UnitView,
    published: Vec<Published>,
}

#[test]
fn the_outer_shell_is_drawn_only_while_the_barrier_has_absorption_left() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", "status_pose_child"]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("status poses");
    let report: PoseReport = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    keep_evidence(directory.path(), "pose_")?;
    let shot = |shot: Shot| -> TestResult<image::RgbImage> {
        Ok(image::open(directory.path().join(shot.file()))?.to_rgb8())
    };
    let published = |wanted: Shot| {
        report
            .published
            .iter()
            .find(|published| published.shot == wanted)
            .copied()
            .unwrap_or_else(|| panic!("{wanted:?} was recorded: {report:?}"))
    };
    let up = published(Shot::BarrierUp);
    assert!(
        up.barrier_up && (up.barrier_ratio - RATIO_UP).abs() < 1e-3 && up.held,
        "{up:?}"
    );
    let low = published(Shot::BarrierLow);
    assert!(
        low.barrier_up && (low.barrier_ratio - RATIO_LOW).abs() < 1e-3 && low.held,
        "{low:?}"
    );
    let down = published(Shot::BarrierDown);
    assert!(
        !down.barrier_up && down.barrier_ratio == 0.0 && !down.held,
        "{down:?}"
    );
    let standing = published(Shot::Standing);
    assert!(!standing.barrier_up && !standing.held, "{standing:?}");

    let driver = gpu_run::selected_driver().driver;
    let unit = report.unit;
    assert!(unit.radius_px > 8.0, "the unit is on screen: {unit:?}");
    let base = shot(Shot::Standing)?;
    let ring = ring_pixels(&base, unit);
    let changed =
        |other: Shot| -> TestResult<usize> { Ok(ring_changes(&base, &shot(other)?, unit)) };
    let (up_px, bare_px, low_px, down_px) = (
        changed(Shot::BarrierUp)?,
        changed(Shot::BarrierUpBare)?,
        changed(Shot::BarrierLow)?,
        changed(Shot::BarrierDown)?,
    );
    let empty_px = ring_changes(&shot(Shot::Empty)?, &base, unit);
    eprintln!(
        "driver {driver}: ring of {ring} pixels round the unit ({:.2}..{:.2} radii, {:.1} px \
         radius): barrier at {RATIO_UP} changes {up_px} ({:.1} %), without its layers \
         {bare_px}, at {RATIO_LOW} {low_px}, down {down_px}; the unit itself changes {empty_px} \
         of the ring against empty ground",
        SHELL_RING.0,
        SHELL_RING.1,
        unit.radius_px,
        100.0 * up_px as f64 / ring.max(1) as f64
    );
    assert!(
        up_px as f64 >= SHELL_RING_SHARE * ring as f64,
        "the outer shell is drawn while the barrier holds half: {up_px} of {ring}"
    );
    assert!(
        low_px >= SHELL_LOW_PIXELS,
        "the outer shell is still drawn with a twentieth left: {low_px}"
    );
    assert!(
        down_px <= TOLERATED_PIXELS,
        "no outer shell once the barrier is down: {down_px}"
    );
    assert!(
        bare_px <= TOLERATED_PIXELS,
        "the outer shell is the effect's own layer, not the bubble's: {bare_px}"
    );
    Ok(())
}

/// Copies every PNG of `directory` whose name starts with `prefix` to the evidence directory,
/// prefixed with the driver, when one is named.
fn keep_evidence(directory: &Path, prefix: &str) -> TestResult {
    let Some(target) = std::env::var_os(EVIDENCE_VAR) else {
        return Ok(());
    };
    let target = PathBuf::from(target);
    std::fs::create_dir_all(&target)?;
    let driver = gpu_run::selected_driver().driver;
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        let png = Path::new(&name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
        if name.starts_with(prefix) && png {
            std::fs::copy(&path, target.join(format!("{driver}_{name}")))?;
        }
    }
    Ok(())
}

fn change(a: [u8; 3], b: [u8; 3]) -> u8 {
    (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0)
}

/// Visits every pixel of `image` in the ring of [`SHELL_RING`] round `unit`.
fn for_ring(image: &image::RgbImage, unit: UnitView, mut visit: impl FnMut(u32, u32)) {
    let [cx, cy] = unit.centre_px;
    let (inner, outer) = (unit.radius_px * SHELL_RING.0, unit.radius_px * SHELL_RING.1);
    let low_x = (cx - outer).floor().max(0.0) as u32;
    let low_y = (cy - outer).floor().max(0.0) as u32;
    let high_x = ((cx + outer).ceil() as u32).min(image.width());
    let high_y = ((cy + outer).ceil() as u32).min(image.height());
    for y in low_y..high_y {
        for x in low_x..high_x {
            let distance = Vec2::new(x as f32 + 0.5 - cx, y as f32 + 0.5 - cy).length();
            if distance > inner && distance <= outer {
                visit(x, y);
            }
        }
    }
}

/// Pixels in the ring round `unit`.
fn ring_pixels(image: &image::RgbImage, unit: UnitView) -> usize {
    let mut count = 0;
    for_ring(image, unit, |_, _| count += 1);
    count
}

/// Pixels in the ring round `unit` that differ between `a` and `b`.
fn ring_changes(a: &image::RgbImage, b: &image::RgbImage, unit: UnitView) -> usize {
    let mut changed = 0;
    for_ring(a, unit, |x, y| {
        if change(a.get_pixel(x, y).0, b.get_pixel(x, y).0) > CHANGE_LEVEL {
            changed += 1;
        }
    });
    changed
}

// ---------------------------------------------------------------------------------------------
// Staging shared by both children
// ---------------------------------------------------------------------------------------------

/// The latest tick the session published, or 0 before the first.
fn published_tick(session: &BattleSession) -> u32 {
    session.published().map_or(0, |published| published.tick)
}

/// Whether the 5 by 5 cells around `point` are open, walkable ground.
fn open(world: &SimWorld, point: Vec2) -> bool {
    let map = &world.map;
    (-2..=2).all(|x: i32| {
        (-2..=2).all(|y: i32| {
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

/// `count` spots of open ground in the viewer's sight near `around`, clear of every unit, node
/// and other spot.
fn find_spots(
    world: &SimWorld,
    fog: &BattleFog,
    around: Vec2,
    count: usize,
    apart: f32,
    on_screen: impl Fn(Vec2) -> bool,
) -> Vec<Vec2> {
    let mut spots: Vec<Vec2> = Vec::new();
    let candidates: Vec<Vec2> = SPOT_DISTANCES
        .iter()
        .flat_map(|distance| {
            (0..24).map(move |step| {
                around + Vec2::from_angle(step as f32 * std::f32::consts::TAU / 24.0) * *distance
            })
        })
        .collect();
    for point in candidates {
        if spots.len() == count {
            break;
        }
        let units = world.entities.values().all(|unit| {
            let at = Vec2::new(unit.position.x.to_f32(), unit.position.y.to_f32());
            at.distance(point) > SPOT_CLEARANCE
        });
        let nodes = world.map.resources().iter().all(|node| {
            let at = Vec2::new(node.pos.x.to_f32(), node.pos.y.to_f32());
            at.distance(point) > SPOT_CLEARANCE
        });
        let apart = spots.iter().all(|spot| spot.distance(point) > apart);
        if open(world, point)
            && units
            && nodes
            && apart
            && fog.sees(point.x, point.y)
            && on_screen(point)
        {
            spots.push(point);
        }
    }
    assert_eq!(
        spots.len(),
        count,
        "the start area has open ground in sight"
    );
    spots
}

/// Holds every mobile unit of the viewer, so nothing walks through the shots.
fn hold_own_units(session: &mut BattleSession) {
    let world = session.world().expect("a ready session has a world");
    let own: Vec<EntityId> = world
        .entities
        .values()
        .filter(|unit| unit.owner == VIEWER && !unit.is_immobile)
        .map(|unit| unit.id)
        .collect();
    let (world, queue) = session.world_and_commands();
    queue
        .submit_selection(
            world.expect("a ready session has a world"),
            VIEWER,
            &own,
            UnitOrder::HoldPosition,
            false,
        )
        .expect("the client admits Hold Position for the viewer's own units");
}

/// Whether `id` is used by a unit on the field.
fn in_use(world: &SimWorld, id: UnitTypeId) -> bool {
    world.entities.values().any(|unit| unit.unit_type_id == id)
}

/// A presentable mobile ground unit type of `world` that `wanted` accepts and that is not one of
/// `taken`, preferring one nothing on the field uses.
fn presentable_ground(
    world: &SimWorld,
    taken: &[UnitTypeId],
    wanted: impl Fn(&sim_core::UnitArchetype) -> bool,
) -> Option<UnitTypeId> {
    let candidates: Vec<&sim_core::UnitArchetype> = world
        .catalog
        .archetypes()
        .filter(|arch| {
            arch.max_speed > SimScalar::ZERO
                && arch.mover_type == MoverType::Ground
                && arch.modes.is_empty()
                && !taken.contains(&arch.unit_type_id)
                && presentable(&assets(), &arch.slug)
                && wanted(arch)
        })
        .collect();
    candidates
        .iter()
        .find(|arch| !in_use(world, arch.unit_type_id))
        .or_else(|| candidates.first())
        .map(|arch| arch.unit_type_id)
}

/// A presentable mobile ground unit type of `world` given, in the test world's catalog only, a
/// copy of the shipped barrier (no presentable shipped unit has one; the barrier's own unit has
/// no media, so the client does not present it). Unarmed, so it fires on nothing.
fn barrier_unit(world: &mut SimWorld, taken: &[UnitTypeId]) -> UnitTypeId {
    let barrier = world
        .catalog
        .archetypes()
        .find_map(|arch| arch.capabilities.barrier)
        .expect("the shipped catalog has a barrier");
    let id = presentable_ground(world, taken, |_| true)
        .expect("the catalog ships a presentable ground unit");
    let archetype = world
        .catalog
        .get_mut(id)
        .expect("invariant: the type was just found in this catalog");
    archetype.capabilities.barrier = Some(barrier);
    archetype.weapon = None;
    archetype.ground_weapon = None;
    archetype.air_weapon = None;
    id
}

/// Spawns a unit of `unit_type` for `owner` at `place`, standing still where it is put.
fn spawn_still(
    world: &mut SimWorld,
    unit_type: UnitTypeId,
    owner: PlayerId,
    place: Vec2,
) -> EntityId {
    let id = world
        .spawn_unit(unit_type, owner, sim_position(place))
        .expect("open ground");
    let unit = world.entities.get_mut(&id).expect("just spawned");
    unit.unpushable = true;
    id
}

/// Raises `id`'s barrier with `ratio` of its absorption left, or lowers it (`None`).
fn set_barrier(world: &mut SimWorld, id: EntityId, ratio: Option<f32>) {
    let unit_type = world.entities.get(&id).expect("it stands").unit_type_id;
    let limit = world
        .catalog
        .get(unit_type)
        .ok()
        .and_then(|arch| arch.capabilities.barrier.as_ref())
        .map(|spec| spec.absorb_limit)
        .expect("the unit has a barrier");
    let unit = world.entities.get_mut(&id).expect("it stands");
    if let Some(ratio) = ratio {
        unit.barrier.raised_ticks = RAISED_TICKS;
        unit.barrier.absorb_left = limit * SimScalar::from_num(ratio);
    } else {
        unit.barrier.raised_ticks = 0;
        unit.barrier.absorb_left = SimScalar::ZERO;
    }
}

/// The view of `bubble`'s unit through `camera`.
fn unit_view(bubble: &BubbleInstance, camera: (&Camera, &GlobalTransform)) -> UnitView {
    let (camera, transform) = camera;
    let record = &bubble.0;
    let [x, y, altitude] = record.world_pos;
    let body = Vec3::new(x, altitude + record.radius, y);
    let project = |point: Vec3| {
        camera
            .world_to_viewport(transform, point)
            .expect("the body is in front of the camera")
    };
    let centre = project(body);
    let edge = project(body + *transform.right() * record.radius);
    // Viewport coordinates are logical pixels from the viewport's corner; the window is 1:1.
    let offset = camera
        .physical_viewport_rect()
        .map_or(Vec2::ZERO, |rect| rect.min.as_vec2());
    let centre = centre + offset;
    UnitView {
        centre_px: centre.to_array(),
        radius_px: centre.distance(edge + offset),
    }
}

/// How the effect layers are drawn for a shot.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
struct EffectView {
    /// No effect layer is drawn.
    bare: bool,
}

/// Applies [`EffectView`] after the frame's effects are built: a bare shot draws no card,
/// decal, distortion or light.
fn apply_effect_view(view: Res<'_, EffectView>, runtime: Option<ResMut<'_, EffectRuntime>>) {
    let Some(mut runtime) = runtime else {
        return;
    };
    if view.bare {
        runtime.frame.sorted.clear();
        runtime.frame.decals.clear();
        runtime.frame.decal_owners.clear();
        runtime.frame.distortions.clear();
        runtime.frame.lights.clear();
    }
}

/// Rebuilds the client's effect tables from the session's catalog, which the staging edited.
fn rebuild_tables(world: &mut World) {
    let catalog = world
        .resource::<BattleSession>()
        .world()
        .map(|sim| sim.catalog.clone());
    let (Some(units), Some(effects)) = (catalog, world.get_resource::<EffectCatalog>().cloned())
    else {
        panic!("the battle and the vocabulary are loaded before the staging");
    };
    let tables = UnitEffectTables::build(&units, &effects).expect("the shipped blocks resolve");
    world.insert_resource(tables);
}

/// Requests a screenshot to `path`; `done` runs once it is written.
fn capture(
    commands: &mut Commands<'_, '_>,
    path: PathBuf,
    done: impl Fn(&mut World) + Send + Sync + 'static,
) {
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<'_, ScreenshotCaptured>, mut commands: Commands<'_, '_>| {
            let image = trigger
                .event()
                .0
                .clone()
                .try_into_dynamic()
                .expect("a screenshot converts to an image")
                .to_rgb8();
            image.save(&path).expect("a screenshot must be written");
            commands.insert_resource(ShotWritten);
        },
    );
    commands.insert_resource(PendingDone(Box::new(done)));
}

/// Marks that the requested screenshot was written.
#[derive(Resource)]
struct ShotWritten;

/// What to do once the requested screenshot is written.
#[derive(Resource)]
struct PendingDone(Box<dyn Fn(&mut World) + Send + Sync>);

/// Runs the pending completion once the screenshot is written.
fn finish_capture(world: &mut World) {
    if world.remove_resource::<ShotWritten>().is_none() {
        return;
    }
    if let Some(PendingDone(done)) = world.remove_resource::<PendingDone>() {
        done(world);
    }
}

/// Requests a screenshot of this frame, writing each crop of `crops` (`(path, [left, top,
/// width, height])`, window pixels, clamped to the window) and, with `whole`, the whole window,
/// without waiting for it: the counter `outstanding` counts it until it is written.
fn capture_frame(
    commands: &mut Commands<'_, '_>,
    crops: Vec<(PathBuf, [u32; 4])>,
    whole: Option<PathBuf>,
    outstanding: &Arc<AtomicU32>,
) {
    outstanding.fetch_add(1, Ordering::SeqCst);
    let outstanding = Arc::clone(outstanding);
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<'_, ScreenshotCaptured>| {
            let image = trigger
                .event()
                .0
                .clone()
                .try_into_dynamic()
                .expect("a screenshot converts to an image")
                .to_rgb8();
            for (path, [left, top, width, height]) in &crops {
                let left = (*left).min(image.width().saturating_sub(1));
                let top = (*top).min(image.height().saturating_sub(1));
                let width = (*width).min(image.width() - left);
                let height = (*height).min(image.height() - top);
                image::imageops::crop_imm(&image, left, top, width, height)
                    .to_image()
                    .save(path)
                    .expect("a screenshot must be written");
            }
            if let Some(path) = &whole {
                image.save(path).expect("a screenshot must be written");
            }
            outstanding.fetch_sub(1, Ordering::SeqCst);
        },
    );
}

/// Boots the production client on the skirmish with the staging `system` and `staging`.
fn run_child<S: Resource, M>(staging: S, system: impl IntoSystemConfigs<M>) {
    let mut app = client::build_graphical_app_off_main_thread(
        ClientMode::Skirmish,
        Some(CHILD_FRAME_LIMIT),
        SKIRMISH_SCENARIO.as_ref(),
    )
    .expect("the shipped assets must start the client");
    let idle = LabRenderIdle::default();
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(idle.clone())
            .add_systems(bevy::render::Render, track_render_idle);
    }
    // Every frame advances presentation time by one 40 ms tick, whatever the frame took, so
    // the pictures are taken at a fixed step and every driver sees the same moments.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(FRAME_STEP))
        .insert_resource(idle)
        .insert_resource(staging)
        .init_resource::<EffectView>()
        .add_systems(Update, system.in_set(BattleSet::Present))
        .add_systems(
            PostUpdate,
            apply_effect_view.after(client::effects::systems::build_effect_frame),
        )
        .add_systems(Last, finish_capture);
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// Whether the renderer has had [`IDLE_FRAMES`] frames in a row with no pipeline waiting.
fn renderer_idle(idle: &LabRenderIdle) -> bool {
    idle.0.load(Ordering::Acquire) >= IDLE_FRAMES
}

/// The camera, once booted and still, as `(still frames, look height)`.
fn camera_still(still: u32, height: f32, rts: &RtsCamera) -> (u32, f32) {
    let still = if (rts.look_at_y - height).abs() < 1e-4 {
        still + 1
    } else {
        0
    };
    (still, rts.look_at_y)
}

/// The live bubbles with their looks.
type TrackQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static BubbleInstance,
        &'static MeshMaterial3d<BubbleDomeMaterial>,
        Option<&'static StanceTrack>,
    ),
    With<LiveBattleBubble>,
>;

// ---------------------------------------------------------------------------------------------
// The pose child
// ---------------------------------------------------------------------------------------------

/// What the pose child is doing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum PosePhase {
    #[default]
    Boot,
    Booting {
        still: u32,
        height: f32,
    },
    /// The world was changed for shot `shot` while tick `since` was published.
    Publishing {
        shot: usize,
        since: u32,
    },
    /// The clock is stopped since `at`; `frames` have been rendered since.
    Settling {
        shot: usize,
        frames: u32,
        at: f64,
    },
    Capturing,
    Done,
}

/// The pose child's staging.
#[derive(Resource, Default)]
struct PoseStaging {
    directory: PathBuf,
    phase: PosePhase,
    spot: Vec2,
    unit: Option<EntityId>,
    report: PoseReport,
}

/// Makes the change shot `index` shows.
fn change_pose_world(
    staging: &mut PoseStaging,
    session: &mut BattleSession,
    view: &mut EffectView,
    index: usize,
) {
    *view = EffectView::default();
    let world = session.world_mut().expect("a ready session has a world");
    match SHOTS[index] {
        Shot::Empty => {}
        Shot::Standing => {
            let unit_type = barrier_unit(world, &[]);
            let id = spawn_still(world, unit_type, VIEWER, staging.spot);
            world
                .entities
                .get_mut(&id)
                .expect("just spawned")
                .is_immobile = true;
            staging.unit = Some(id);
        }
        Shot::BarrierUp => set_barrier(world, staging.unit.expect("spawned"), Some(RATIO_UP)),
        Shot::BarrierUpBare => view.bare = true,
        Shot::BarrierLow => set_barrier(world, staging.unit.expect("spawned"), Some(RATIO_LOW)),
        Shot::BarrierDown => set_barrier(world, staging.unit.expect("spawned"), None),
    }
}

/// Records what shot `index` shows before it is captured.
fn record_pose(
    staging: &mut PoseStaging,
    index: usize,
    bubbles: &TrackQuery<'_, '_>,
    camera: (&Camera, &GlobalTransform),
) {
    let Some((bubble, _, track)) = bubbles
        .iter()
        .find(|(bubble, ..)| Some(bubble.0.entity_id) == staging.unit)
    else {
        return;
    };
    if SHOTS[index] == Shot::Standing {
        staging.report.unit = unit_view(bubble, camera);
    }
    staging.report.published.push(Published {
        shot: SHOTS[index],
        barrier_up: bubble.0.status.barrier_up,
        barrier_ratio: bubble.0.barrier_ratio,
        held: track.is_some_and(|track| track.status[status_index(StatusKind::Barrier)].is_some()),
    });
}

/// Advances the pose staging by one frame.
fn stage_poses(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, PoseStaging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    mut view: ResMut<'_, EffectView>,
    time: Res<'_, Time<Virtual>>,
    fog: Option<Res<'_, BattleFog>>,
    bubbles: TrackQuery<'_, '_>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform, &RtsCamera)>,
    idle: Res<'_, LabRenderIdle>,
) {
    let fog = fog.expect("the skirmish presents a player, so it keeps a fog");
    if !session.is_ready() {
        return;
    }
    let Ok((camera, camera_transform, rts)) = cameras.get_single() else {
        return;
    };
    let tick = published_tick(&session);
    let now = time.elapsed_secs_f64();
    staging.phase = match staging.phase {
        PosePhase::Boot if tick < 2 => PosePhase::Boot,
        PosePhase::Boot => PosePhase::Booting {
            still: 0,
            height: rts.look_at_y,
        },
        PosePhase::Booting { still, height } if still < STILL_CAMERA_FRAMES => {
            let (still, height) = camera_still(still, height, rts);
            PosePhase::Booting { still, height }
        }
        PosePhase::Booting { .. } => {
            let world = session.world().expect("a ready session has a world");
            staging.spot = find_spots(world, &fog, rts.target, 1, SPOT_CLEARANCE, |_| true)[0];
            hold_own_units(&mut session);
            change_pose_world(&mut staging, &mut session, &mut view, 0);
            commands.queue(rebuild_tables);
            lab.battle_paused = false;
            PosePhase::Publishing {
                shot: 0,
                since: tick,
            }
        }
        PosePhase::Publishing { shot, since } if tick <= since => {
            PosePhase::Publishing { shot, since }
        }
        PosePhase::Publishing { shot, .. } => {
            lab.battle_paused = true;
            PosePhase::Settling {
                shot,
                frames: 0,
                at: now,
            }
        }
        PosePhase::Settling { shot, frames, at }
            if frames < SETTLE_FRAMES || now - at < SETTLE_SECONDS || !renderer_idle(&idle) =>
        {
            PosePhase::Settling {
                shot,
                frames: frames + 1,
                at,
            }
        }
        PosePhase::Settling { shot, .. } => {
            record_pose(&mut staging, shot, &bubbles, (camera, camera_transform));
            let path = staging.directory.join(SHOTS[shot].file());
            capture(&mut commands, path, move |world: &mut World| {
                let next = shot + 1;
                world.resource_scope(|world, mut staging: Mut<'_, PoseStaging>| {
                    if next == SHOTS.len() {
                        let text = ron::to_string(&staging.report).expect("plain data");
                        std::fs::write(staging.directory.join(REPORT_FILE), text)
                            .expect("the parent's directory is writable");
                        staging.phase = PosePhase::Done;
                        world.send_event(AppExit::Success);
                        return;
                    }
                    world.resource_scope(|world, mut session: Mut<'_, BattleSession>| {
                        world.resource_scope(|world, mut view: Mut<'_, EffectView>| {
                            let since = published_tick(&session);
                            change_pose_world(&mut staging, &mut session, &mut view, next);
                            world.resource_mut::<LabState>().battle_paused = false;
                            staging.phase = PosePhase::Publishing { shot: next, since };
                        });
                    });
                });
                rebuild_tables(world);
            });
            PosePhase::Capturing
        }
        PosePhase::Capturing => PosePhase::Capturing,
        PosePhase::Done => PosePhase::Done,
    };
}

/// The child of [`the_outer_shell_is_drawn_only_while_the_barrier_has_absorption_left`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn status_pose_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    run_child(
        PoseStaging {
            directory: PathBuf::from(directory),
            ..PoseStaging::default()
        },
        stage_poses,
    );
}

// ---------------------------------------------------------------------------------------------
// The battle child
// ---------------------------------------------------------------------------------------------

/// The subjects of the battle, in the order of their spots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Subject {
    /// Stimmed and running.
    Stim,
    /// Slowed and walking.
    Daze,
    /// Charging.
    Charge,
    /// A barrier under an enemy's fire.
    Barrier,
    /// Three units whose blocks name a death effect, killed at [`DEATH_SECONDS`].
    Deaths,
}

const SUBJECTS: [Subject; 5] = [
    Subject::Stim,
    Subject::Daze,
    Subject::Charge,
    Subject::Barrier,
    Subject::Deaths,
];

impl Subject {
    fn name(self) -> String {
        format!("{self:?}").to_lowercase()
    }
}

/// Seconds the battle is pictured for.
const BATTLE_SECONDS: f64 = 3.6;
/// Seconds into the battle at which the death subjects are killed.
const DEATH_SECONDS: f64 = 1.0;
/// Seconds into the battle at which the whole window is pictured.
const WHOLE_AT: [f64; 4] = [0.4, 1.2, 2.0, 2.8];
/// Size of a subject's picture, window pixels.
const CROP: [u32; 2] = [260, 180];
/// Distance a moving subject is sent, world units.
const RUN: f32 = 4.0;
/// Distance from the charging unit to the enemy it is ordered to attack, centre to centre, world
/// units: inside the dash's window (a gap above its minimum, a path below its reach).
const CHARGE_DISTANCE: f32 = 3.8;
/// Frames the battle camera is given to settle on the subjects before they are staged.
const FRAMING_FRAMES: u32 = 12;

/// One frame's sample of the subjects.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BattleSample {
    /// Seconds since the battle was staged.
    at: f32,
    /// Per moving or shielded subject (`Stim`, `Daze`, `Charge`, `Barrier`): whether the status
    /// is published (for a barrier: up with absorption left) and whether its look is held.
    statuses: Vec<(Subject, bool, bool)>,
    /// The stimmed unit's rim strength in its bubble material.
    stim_rim: f32,
    /// Whether the barrier unit's bubble published a hit this frame.
    barrier_hit: bool,
    /// When the barrier unit's last absorb flash spawned.
    last_flash: Option<f64>,
    /// Live effect instances.
    live_effects: u32,
    /// Legacy glass-shard entities alive.
    shards: usize,
    /// Death subjects still standing.
    standing_deaths: usize,
}

/// What the battle child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct BattleReport {
    samples: Vec<BattleSample>,
    /// Effects spawned by the runtime when the deaths were staged and at the end.
    spawned: (u64, u64),
    /// Where each subject's units stood on screen on the first frame, window pixels.
    placed: Vec<(Subject, Vec<[f32; 2]>)>,
}

/// What the battle child is doing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum BattlePhase {
    #[default]
    Boot,
    Booting {
        still: u32,
        height: f32,
    },
    /// The renderer settles before the subjects are staged; `frames` have passed.
    Framing {
        frames: u32,
    },
    /// Running since `at`; `frame` pictures have been requested.
    Running {
        at: f64,
        frame: u32,
    },
    Finishing,
}

/// The battle child's staging.
#[derive(Resource, Default)]
struct BattleStaging {
    directory: PathBuf,
    phase: BattlePhase,
    /// The subjects' spots.
    spots: Vec<Vec2>,
    /// The unit of each subject (the first for `Deaths`).
    units: Vec<(Subject, Vec<EntityId>)>,
    killed: bool,
    /// Where the death subjects are pictured, fixed on the first frame.
    deaths_crop: Option<[u32; 4]>,
    outstanding: Arc<AtomicU32>,
    report: BattleReport,
}

/// A buff the simulation would give a unit, staged directly.
fn buff(buff_id: u8, ticks: u16, speed: f32, rate: f32) -> ActiveBuff {
    ActiveBuff {
        buff_id,
        remaining_ticks: ticks,
        speed_multiplier: SimScalar::from_num(speed),
        attack_rate_multiplier: SimScalar::from_num(rate),
    }
}

/// Stages every subject on its spot. Returns each subject's units.
fn stage_battle(session: &mut BattleSession, spots: &[Vec2]) -> Vec<(Subject, Vec<EntityId>)> {
    let world = session.world_mut().expect("a ready session has a world");
    let mut taken = Vec::new();
    let mut units = Vec::new();
    let mut moving = Vec::new();
    let mut attacks = Vec::new();
    for (subject, spot) in SUBJECTS.iter().zip(spots) {
        let ids = match subject {
            Subject::Stim => {
                let unit_type =
                    presentable_ground(world, &taken, |arch| arch.capabilities.overclock.is_some())
                        .expect("a presentable unit that stims");
                taken.push(unit_type);
                let id = world
                    .spawn_unit(unit_type, VIEWER, sim_position(*spot))
                    .expect("open ground");
                let speed = 1.5;
                world
                    .entities
                    .get_mut(&id)
                    .expect("spawned")
                    .active_buffs
                    .push(buff(OVERCLOCK_BUFF_ID, 268, speed, 1.5));
                moving.push((id, *spot));
                vec![id]
            }
            Subject::Daze => {
                let unit_type =
                    presentable_ground(world, &taken, |_| true).expect("a presentable ground unit");
                taken.push(unit_type);
                let id = world
                    .spawn_unit(unit_type, VIEWER, sim_position(*spot))
                    .expect("open ground");
                world
                    .entities
                    .get_mut(&id)
                    .expect("spawned")
                    .active_buffs
                    .push(buff(2, 200, 0.5, 1.0));
                moving.push((id, *spot));
                vec![id]
            }
            Subject::Charge => {
                // A real charge: the dash unlocked for the viewer, and an attack order on an
                // enemy inside the dash's window (more than its gap, nearer than its reach).
                let unit_type = presentable_ground(world, &taken, |arch| {
                    arch.capabilities.attack_dash.is_some()
                })
                .expect("a presentable unit that charges");
                taken.push(unit_type);
                let slug = world
                    .catalog
                    .get(unit_type)
                    .expect("invariant: the type was just found")
                    .slug
                    .clone();
                world.player_upgrades[PlayerSlot::ALL[0]]
                    .unlocked_abilities
                    .entry(slug)
                    .or_default()
                    .insert(ATTACK_DASH_ABILITY_SLUG.to_owned());
                let id = world
                    .spawn_unit(unit_type, VIEWER, sim_position(*spot))
                    .expect("open ground");
                let target_type =
                    presentable_ground(world, &taken, |_| true).expect("a presentable ground unit");
                taken.push(target_type);
                let target = spawn_still(
                    world,
                    target_type,
                    ENEMY,
                    *spot + Vec2::new(0.0, CHARGE_DISTANCE),
                );
                world
                    .entities
                    .get_mut(&target)
                    .expect("spawned")
                    .is_immobile = true;
                attacks.push((id, target));
                vec![id, target]
            }
            Subject::Barrier => {
                let unit_type = barrier_unit(world, &taken);
                taken.push(unit_type);
                let id = spawn_still(world, unit_type, VIEWER, *spot);
                world.entities.get_mut(&id).expect("spawned").is_immobile = true;
                // An armed enemy beside it, which fires on it: its barrier raises on the first
                // hit and absorbs until it is spent.
                // An attacker whose shots draw no splash of their own, so the flashes are the
                // barrier's alone.
                let attacker = presentable_ground(world, &taken, |arch| {
                    arch.weapon.is_some() && arch.effects.weapons.is_empty()
                })
                .expect("a presentable armed unit");
                let enemy = spawn_still(world, attacker, ENEMY, *spot + Vec2::new(0.0, 2.2));
                // Raised from the start, so the shell is pictured whole before the hits wear it
                // down; the attacker is ordered onto it, so the hits come.
                set_barrier(world, id, Some(1.0));
                world
                    .entities
                    .get_mut(&enemy)
                    .expect("spawned")
                    .order_queue
                    .push_back(UnitOrder::attack(id));
                vec![id, enemy]
            }
            Subject::Deaths => {
                let dying: Vec<UnitTypeId> = world
                    .catalog
                    .archetypes()
                    .filter(|arch| {
                        arch.effects.death.is_some()
                            && arch.mover_type == MoverType::Ground
                            && presentable(&assets(), &arch.slug)
                    })
                    .map(|arch| arch.unit_type_id)
                    .take(3)
                    .collect();
                assert!(!dying.is_empty(), "a presentable unit names its death");
                dying
                    .iter()
                    .enumerate()
                    .map(|(index, unit_type)| {
                        let place = *spot + Vec2::new(0.0, index as f32 * 1.6);
                        let id = spawn_still(world, *unit_type, VIEWER, place);
                        // Unarmed, so the dying units fire on none of the other subjects.
                        let archetype = world
                            .catalog
                            .get_mut(*unit_type)
                            .expect("invariant: a type of this catalog");
                        archetype.weapon = None;
                        archetype.ground_weapon = None;
                        archetype.air_weapon = None;
                        world.entities.get_mut(&id).expect("spawned").is_immobile = true;
                        id
                    })
                    .collect()
            }
        };
        units.push((*subject, ids));
    }
    session.publish();
    for (id, spot) in moving {
        let (world, queue) = session.world_and_commands();
        queue
            .submit_selection(
                world.expect("a ready session has a world"),
                VIEWER,
                &[id],
                UnitOrder::Move {
                    target: sim_position(spot + Vec2::new(RUN, 0.0)),
                },
                false,
            )
            .expect("the client admits a move for the viewer's own unit");
    }
    for (id, target) in attacks {
        let (world, queue) = session.world_and_commands();
        queue
            .submit_selection(
                world.expect("a ready session has a world"),
                VIEWER,
                &[id],
                UnitOrder::attack(target),
                false,
            )
            .expect("the client admits an attack on an enemy in sight");
    }
    units
}

/// The window crop round a subject's units.
fn subject_crop(views: &[UnitView]) -> [u32; 4] {
    let [width, height] = CROP;
    let centre = views
        .iter()
        .fold(Vec2::ZERO, |sum, view| sum + Vec2::from(view.centre_px))
        / views.len().max(1) as f32;
    let left = (centre.x - width as f32 * 0.5).max(0.0) as u32;
    let top = (centre.y - height as f32 * 0.55).max(0.0) as u32;
    [left, top, width, height]
}

/// Samples this frame and requests its pictures.
fn battle_step(
    commands: &mut Commands<'_, '_>,
    staging: &mut BattleStaging,
    session: &mut BattleSession,
    runtime: &EffectRuntime,
    (bubbles, materials, shards): (&TrackQuery<'_, '_>, &Assets<BubbleDomeMaterial>, usize),
    camera: (&Camera, &GlobalTransform),
    since: f64,
    frame: u32,
) {
    let find = |id: EntityId| bubbles.iter().find(|(bubble, ..)| bubble.0.entity_id == id);
    let mut sample = BattleSample {
        at: since as f32,
        live_effects: runtime.pool.live_total(),
        shards,
        ..BattleSample::default()
    };
    let mut crops = Vec::new();
    for (subject, ids) in &staging.units {
        let views: Vec<UnitView> = ids
            .iter()
            .filter_map(|id| find(*id))
            .map(|(bubble, ..)| unit_view(bubble, camera))
            .collect();
        let path = staging
            .directory
            .join(format!("battle_{}_{frame:03}.png", subject.name()));
        // On the first frame its units are drawn (they appear a published tick after staging).
        let placed = staging.report.placed.iter().any(|(s, _)| s == subject);
        if !placed && !views.is_empty() {
            staging
                .report
                .placed
                .push((*subject, views.iter().map(|view| view.centre_px).collect()));
        }
        if *subject == Subject::Deaths {
            sample.standing_deaths = views.len();
            // The crop stays where the dying units stood, so their deaths are pictured whole.
            if staging.deaths_crop.is_none() && !views.is_empty() {
                staging.deaths_crop = Some(subject_crop(&views));
            }
            if let Some(crop) = staging.deaths_crop {
                crops.push((path, crop));
            }
        } else if !views.is_empty() {
            crops.push((path, subject_crop(&views)));
        }
        let kind = match subject {
            Subject::Stim => StatusKind::Overclock,
            Subject::Daze => StatusKind::Slowed,
            Subject::Charge => StatusKind::Dash,
            Subject::Barrier => StatusKind::Barrier,
            Subject::Deaths => continue,
        };
        let Some((bubble, material, track)) = ids.first().and_then(|id| find(*id)) else {
            continue;
        };
        let held = track.is_some_and(|track| track.status[status_index(kind)].is_some());
        sample
            .statuses
            .push((*subject, status_shown(&bubble.0, kind), held));
        match subject {
            Subject::Stim => {
                sample.stim_rim = materials
                    .get(&material.0)
                    .map_or(0.0, |material| material.shell.rim.w);
            }
            Subject::Barrier => {
                sample.barrier_hit = bubble.0.status.barrier_hit;
                sample.last_flash =
                    track.and_then(|track| track.last_hit[status_index(StatusKind::Barrier)]);
            }
            _ => {}
        }
    }
    let whole = WHOLE_AT
        .iter()
        .position(|at| (since - at).abs() < 0.02)
        .map(|index| staging.directory.join(format!("battle_whole_{index}.png")));
    capture_frame(commands, crops, whole, &staging.outstanding);
    if !staging.killed && since >= DEATH_SECONDS {
        let world = session.world_mut().expect("a ready session has a world");
        if let Some((_, ids)) = staging.units.iter().find(|(s, _)| *s == Subject::Deaths) {
            for id in ids {
                if let Some(unit) = world.entities.get_mut(id) {
                    unit.hp = SimScalar::ZERO;
                }
            }
        }
        staging.killed = true;
        staging.report.spawned.0 = runtime.stats.spawned;
    }
    staging.report.samples.push(sample);
}

/// Advances the battle staging by one frame.
fn stage_battle_child(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, BattleStaging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    time: Res<'_, Time<Virtual>>,
    fog: Option<Res<'_, BattleFog>>,
    bubbles: TrackQuery<'_, '_>,
    materials: Res<'_, Assets<BubbleDomeMaterial>>,
    shards: Query<'_, '_, (), With<ShatterShard>>,
    runtime: Option<Res<'_, EffectRuntime>>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform, &RtsCamera)>,
    idle: Res<'_, LabRenderIdle>,
) {
    let fog = fog.expect("the skirmish presents a player, so it keeps a fog");
    if !session.is_ready() {
        return;
    }
    let Ok((camera, camera_transform, rts)) = cameras.get_single() else {
        return;
    };
    let tick = published_tick(&session);
    let now = time.elapsed_secs_f64();
    staging.phase = match staging.phase {
        BattlePhase::Boot if tick < 2 => BattlePhase::Boot,
        BattlePhase::Boot => BattlePhase::Booting {
            still: 0,
            height: rts.look_at_y,
        },
        BattlePhase::Booting { still, height }
            if still < STILL_CAMERA_FRAMES || !renderer_idle(&idle) =>
        {
            let (still, height) = camera_still(still, height, rts);
            BattlePhase::Booting { still, height }
        }
        BattlePhase::Booting { .. } => BattlePhase::Framing { frames: 0 },
        BattlePhase::Framing { frames } if frames < FRAMING_FRAMES || !renderer_idle(&idle) => {
            BattlePhase::Framing { frames: frames + 1 }
        }
        BattlePhase::Framing { .. } => {
            let world = session.world().expect("a ready session has a world");
            // Spots whose subjects are pictured whole: inside the window's open part, clear of
            // its edges and of the HUD along its bottom.
            let window = camera
                .physical_viewport_rect()
                .map_or(Vec2::new(1280.0, 720.0), |rect| rect.max.as_vec2());
            let on_screen = |point: Vec2| {
                let ground = Vec3::new(
                    point.x,
                    world.map.terrain_altitude_at(sim_position(point)),
                    point.y,
                );
                camera
                    .world_to_viewport(camera_transform, ground)
                    .is_ok_and(|at| {
                        at.x > SCREEN_MARGIN
                            && at.x < window.x - SCREEN_MARGIN
                            && at.y > SCREEN_MARGIN
                            && at.y < window.y * SCREEN_BOTTOM_SHARE
                    })
            };
            staging.spots = find_spots(
                world,
                &fog,
                rts.target,
                SUBJECTS.len(),
                SUBJECT_APART,
                on_screen,
            );
            hold_own_units(&mut session);
            let spots = staging.spots.clone();
            staging.units = stage_battle(&mut session, &spots);
            commands.queue(rebuild_tables);
            lab.battle_paused = false;
            BattlePhase::Running { at: now, frame: 0 }
        }
        BattlePhase::Running { at, frame } if now - at <= BATTLE_SECONDS => {
            let runtime = runtime.expect("the effect system is loaded");
            battle_step(
                &mut commands,
                &mut staging,
                &mut session,
                &runtime,
                (&bubbles, &materials, shards.iter().count()),
                (camera, camera_transform),
                now - at,
                frame,
            );
            BattlePhase::Running {
                at,
                frame: frame + 1,
            }
        }
        BattlePhase::Running { .. } => {
            lab.battle_paused = true;
            staging.report.spawned.1 = runtime.map_or(0, |runtime| runtime.stats.spawned);
            BattlePhase::Finishing
        }
        BattlePhase::Finishing if staging.outstanding.load(Ordering::SeqCst) > 0 => {
            BattlePhase::Finishing
        }
        BattlePhase::Finishing => {
            let text = ron::to_string(&staging.report).expect("plain data");
            std::fs::write(staging.directory.join(REPORT_FILE), text)
                .expect("the parent's directory is writable");
            commands.send_event(AppExit::Success);
            BattlePhase::Finishing
        }
    };
}

/// The child of [`status_looks_follow_their_statuses_and_deaths_replace_the_shatter`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn status_battle_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    run_child(
        BattleStaging {
            directory: PathBuf::from(directory),
            ..BattleStaging::default()
        },
        stage_battle_child,
    );
}

#[test]
fn status_looks_follow_their_statuses_and_deaths_replace_the_shatter() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", "status_battle_child"]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("status battle");
    let report: BattleReport = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    keep_evidence(directory.path(), "battle_")?;
    let driver = gpu_run::selected_driver().driver;
    let samples = &report.samples;
    for (subject, placed) in &report.placed {
        let at: Vec<String> = placed
            .iter()
            .map(|[x, y]| format!("({x:.0}, {y:.0})"))
            .collect();
        eprintln!("driver {driver}: {subject:?} stands at {}", at.join(", "));
    }
    assert!(
        samples.len() > 60,
        "the battle ran: {} frames",
        samples.len()
    );

    // Each status's look is held exactly while its status is shown.
    for subject in [
        Subject::Stim,
        Subject::Daze,
        Subject::Charge,
        Subject::Barrier,
    ] {
        let frames: Vec<(bool, bool)> = samples
            .iter()
            .filter_map(|sample| {
                sample
                    .statuses
                    .iter()
                    .find(|(s, ..)| *s == subject)
                    .map(|(_, shown, held)| (*shown, *held))
            })
            .collect();
        let shown = frames.iter().filter(|(shown, _)| *shown).count();
        // The sample is taken in the frame the snapshot arrives, before the looks are kept in
        // step with it, so on the first frame after a status changes the look may lag by one.
        let mismatched = frames
            .iter()
            .enumerate()
            .filter(|(index, (shown, held))| {
                let changed = index
                    .checked_sub(1)
                    .is_none_or(|before| frames[before].0 != *shown);
                shown != held && !changed
            })
            .count();
        eprintln!(
            "driver {driver}: {subject:?}: status shown on {shown} of {} frames, look held \
             differently on {mismatched}",
            frames.len()
        );
        assert!(shown > 0, "{subject:?}'s status was staged");
        assert_eq!(mismatched, 0, "{subject:?}'s look follows its status");
    }
    // The stim's rim beats while it runs.
    let rims: Vec<f32> = samples
        .iter()
        .filter(|sample| sample.at > 0.5 && sample.at < 2.5)
        .map(|sample| sample.stim_rim)
        .collect();
    let (low, high) = rims.iter().fold((f32::MAX, 0.0_f32), |(low, high), rim| {
        (low.min(*rim), high.max(*rim))
    });
    eprintln!("driver {driver}: the stimmed unit's rim runs from {low:.3} to {high:.3}");
    assert!(high > 0.4 && high - low > 0.2, "the stim's rim beats");
    // Hits on the barrier are answered by a flash, at most one per beat.
    let hits = samples.iter().filter(|sample| sample.barrier_hit).count();
    let mut flashes: Vec<f64> = samples
        .iter()
        .filter_map(|sample| sample.last_flash)
        .collect();
    flashes.dedup();
    eprintln!(
        "driver {driver}: the barrier absorbed hits on {hits} frames and flashed {} times",
        flashes.len()
    );
    assert!(
        hits > 0 && !flashes.is_empty(),
        "the barrier was hit and flashed"
    );
    assert!(
        flashes
            .windows(2)
            .all(|pair| pair[1] - pair[0] >= 0.16 - 1e-6),
        "flashes keep their beat: {flashes:?}"
    );
    // The deaths: the units died, their effects were spawned, and no legacy shard was drawn.
    let after = samples
        .iter()
        .filter(|sample| f64::from(sample.at) > DEATH_SECONDS + 0.2);
    let mut died = false;
    for sample in after {
        died |= sample.standing_deaths == 0;
        assert_eq!(sample.shards, 0, "no glass shards at {:.2} s", sample.at);
    }
    assert!(died, "the death subjects died");
    eprintln!(
        "driver {driver}: effects spawned {} before the deaths, {} at the end",
        report.spawned.0, report.spawned.1
    );
    assert!(report.spawned.1 > report.spawned.0, "effects were spawned");
    Ok(())
}
