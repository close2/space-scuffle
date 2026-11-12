//! Concealment and stance looks on a real GPU (plan `2026-11-09-effect-visualization` WP8).
//!
//! # Responsibility
//! Proves the WP8 looks all the way to pixels in the production client playing the shipped
//! skirmish. Two children each stage shots on open ground the player sees:
//! - **Poses** ([`stance_pose_child`]): empty ground; one of the player's units standing; the
//!   same unit burrowed, with and without the effect layers drawn; an enemy cloaked unit the
//!   player does not detect, with and without the effect layers; and one of the player's
//!   anchoring units half way through its mode switch, its bubble hidden, with only the
//!   `Critical` layers drawn and with none. The parent measures:
//!   - "burrow": the burrowed bubble changes at most [`BURROW_AREA_SHARE`] of the pixels the
//!     standing one changes inside its screen disc, and every pixel its looks change lies on
//!     the ground inside [`MOUND_RING_RADII`] of its radius (2 px tolerance);
//!   - "cloaked enemy, undetected": every pixel its shimmer changes takes a colour the ground
//!     already has within [`SHIMMER_REACH_PX`] (a bend of what lies behind, no tint), the
//!     patch's mean colour moves by at most [`SHIMMER_MEAN_LEVELS`], and without the effect
//!     layers it changes at most [`TOLERATED_PIXELS`] pixels (nothing of it leaks);
//!   - "siege mid-transition": the sweep's arc covers the switch's presented progress within
//!     [`ARC_TOLERANCE`] of the turn.
//! - **Sequences** ([`stance_sequence_child`]): one of the player's units burrows and comes up
//!   again through real mode commands, and an anchoring unit sieges and unsieges; the child
//!   samples the bubble's shell every frame and writes a picture every few frames, then stages
//!   a tableau of every look at once, twice: before and after a detector reaches the enemy.
//!   The parent checks the client-local settle of D-6: the dig goes on after the channel has
//!   ended until the body is fully sunk, never rises while digging, and the emerge never
//!   sinks.
//!
//! With `SPACE_SCUFFLE_STANCE_EVIDENCE_DIR` set, the parents copy every picture there, for the
//! review sheets.
//!
//! # How it fits
//! Each child re-executes this test binary through `support/gpu_run.rs` (charter §7, C21), as
//! `concealment_gpu_tests.rs` does. The shipped burrowing unit has no media yet, so the burrowing
//! unit is the presentable stand-in of `battle_harness::burrower`, given the shipped unit's
//! mode-switch ability in the session's catalog, with the effect tables rebuilt from that
//! catalog so it names its mode's looks. Every unit is chosen by catalog properties.
//!
//! # Invariants verified
//! - The staging published what it claims (concealment, presence, a mode switch under way).
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

use battle_harness::{assets, burrower, presentable, sim_position};

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::render::{Render, RenderApp};
use client::battle_pipeline::commands::ModeDirection;
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::bubble_shader::{BubbleDomeMaterial, BubbleInstance};
use client::effects::catalog::schema::Priority;
use client::effects::catalog::{EffectCatalog, UnitEffectTables};
use client::effects::lab::driver::track_render_idle;
use client::effects::lab::LabRenderIdle;
use client::effects::runtime::EffectRuntime;
use client::effects::stance::{channel_progress, StanceTrack};
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::scene_controller::LiveBattleBubble;
use client::{ClientMode, LabState, RtsCamera};
use serde::{Deserialize, Serialize};
use sim_core::SimWorld;
use sim_map::CellFlags;
use sim_types::{
    AbilityEffectSpec, Concealment, EntityId, MoverType, PlayerId, SimScalar, UnitOrder, UnitTypeId,
};
use std::sync::atomic::Ordering;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the directory a child writes to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_STANCE_CHILD_OUT";
/// Environment variable naming a directory the parents copy their pictures to.
const EVIDENCE_VAR: &str = "SPACE_SCUFFLE_STANCE_EVIDENCE_DIR";
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
/// Seconds of `Time<Virtual>`, at least, between a stopped clock and its capture: held looks
/// come in over at most half a second.
const SETTLE_SECONDS: f64 = 0.9;
/// Distances from the camera's target, in world units, at which the spots are sought.
const SPOT_DISTANCES: [f32; 9] = [4.0, 5.0, 6.0, 7.0, 8.0, 3.0, 9.0, 10.0, 11.0];
/// Clearance around a spot, in world units, from any unit, structure, node or other spot.
const SPOT_CLEARANCE: f32 = 3.0;

/// A pixel counts as changed when one of its channels moves by more than this, in levels.
const CHANGE_LEVEL: u8 = 12;
/// Changed pixels tolerated where nothing may be drawn (plan WP3's rule).
const TOLERATED_PIXELS: usize = 2;
/// Plan WP8: the burrowed bubble's above-ground area is at most this share of the standing one.
const BURROW_AREA_SHARE: f64 = 0.35;
/// The burrowed bubble still shows its owner a little of itself: at least this share.
const BURROW_AREA_FLOOR: f64 = 0.02;
/// The mound's ring: its looks lie on the ground within this many radii of the unit.
const MOUND_RING_RADII: f32 = 1.8;
/// Pixels of tolerance at a ring's edge (plan §10.2).
const EDGE_PX: f32 = 2.0;
/// The mound is drawn: at least this many pixels change.
const MOUND_MIN_PIXELS: usize = 40;
/// Reach of a shimmer's bend in pixels: its colours must already be on the ground this near.
const SHIMMER_REACH_PX: i64 = 12;
/// Levels a shimmer pixel may lie outside the colours of the ground around it (filtering).
const SHIMMER_SLACK: i16 = 6;
/// Share of bent pixels tolerated outside the ground's colours near them: the bend reads the
/// opaque scene before the smog and the transparent layers are drawn over it, so a few pixels
/// take a colour the final frame shows nowhere near. Measured on radv (2026-11-12) at two spots:
/// 88 of 2 097 and 84 of 2 170 bent pixels with a 6 or 8 px reach, 5 of 2 083 at 8 px; at
/// 12 px, 28 of 2 171 on radv and 54 of 2 149 (2.5 %) on lavapipe.
const SHIMMER_FOREIGN_SHARE: f64 = 0.05;
/// A shimmer moves a patch's mean colour by at most this many levels (no tint).
const SHIMMER_MEAN_LEVELS: f64 = 1.5;
/// A pixel counts as bent by the shimmer when a channel moves by more than this.
const SHIMMER_LEVEL: u8 = 4;
/// The shimmer is there to see: at least this many pixels bend.
const SHIMMER_MIN_PIXELS: usize = 8;
/// The mode switch is caught half way: at least this share done.
const SIEGE_CATCH: f32 = 0.45;
/// Plan WP8: the arc is within this share of the turn of the switch's progress.
const ARC_TOLERANCE: f32 = 0.05;
/// The arc's band on the ground, in radii of the unit (its ring is drawn at 1.85 radii, 0.2 of
/// that thick).
const ARC_BAND: (f32, f32) = (1.35, 2.1);
/// Angular bins of the arc measurement.
const ARC_BINS: usize = 360;

/// The pose shots, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Shot {
    Empty,
    Upright,
    Burrowed,
    BurrowedBare,
    Shimmer,
    ShimmerBare,
    SiegeMid,
    SiegeMidBare,
}

const SHOTS: [Shot; 8] = [
    Shot::Empty,
    Shot::Upright,
    Shot::Burrowed,
    Shot::BurrowedBare,
    Shot::Shimmer,
    Shot::ShimmerBare,
    Shot::SiegeMid,
    Shot::SiegeMidBare,
];

impl Shot {
    fn file(self) -> String {
        format!("pose_{self:?}.png").to_lowercase()
    }
}

/// Where a unit stood in a shot and how the camera saw it.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct UnitView {
    /// Ground point under the unit, Bevy world space.
    ground: [f32; 3],
    /// Bubble radius, world units.
    radius: f32,
    /// The body's centre on screen, physical pixels.
    centre_px: [f32; 2],
    /// The body's radius on screen, pixels.
    radius_px: f32,
}

/// The camera of a shot.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct CameraView {
    /// Column-major `clip_from_world`.
    clip_from_world: [f32; 16],
    /// Physical viewport `[x, y, width, height]`.
    viewport: [f32; 4],
}

impl CameraView {
    fn matrix(&self) -> Mat4 {
        Mat4::from_cols_array(&self.clip_from_world)
    }

    /// Screen pixel of world `point`.
    fn project(&self, point: Vec3) -> Vec2 {
        let ndc = self.matrix().project_point3(point);
        let [left, top, width, height] = self.viewport;
        Vec2::new(
            left + f32::midpoint(ndc.x, 1.0) * width,
            top + f32::midpoint(1.0, -ndc.y) * height,
        )
    }

    /// The ground point (at height `ground_y`) seen through pixel `(column, row)`.
    fn unproject(&self, column: f32, row: f32, ground_y: f32) -> Option<Vec3> {
        let [left, top, width, height] = self.viewport;
        let ndc = Vec2::new(
            (column + 0.5 - left) / width * 2.0 - 1.0,
            1.0 - (row + 0.5 - top) / height * 2.0,
        );
        let inverse = self.matrix().inverse();
        let near = inverse.project_point3(ndc.extend(1.0));
        let far = inverse.project_point3(ndc.extend(0.1));
        let direction = far - near;
        if direction.y.abs() < 1e-6 {
            return None;
        }
        let along = (ground_y - near.y) / direction.y;
        Some(near + direction * along)
    }
}

/// What the pose child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PoseReport {
    camera: CameraView,
    burrower: UnitView,
    shimmer: UnitView,
    siege: UnitView,
    /// Concealment and presence published in each shot, by shot.
    published: Vec<(Shot, Concealment, String)>,
    /// The switch's presented progress in the `SiegeMid` shot.
    siege_progress: f32,
}

#[test]
fn concealment_and_stance_poses_read_as_the_plan_asks() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", "stance_pose_child"]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("stance poses");
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
            .find(|(shot, ..)| *shot == wanted)
            .map(|(_, concealment, presence)| (*concealment, presence.clone()))
    };
    assert_eq!(
        published(Shot::Burrowed),
        Some((Concealment::Burrowed, "Shown".to_owned())),
        "{report:?}"
    );
    assert_eq!(
        published(Shot::Shimmer),
        Some((Concealment::Cloaked, "Shimmer".to_owned())),
        "{report:?}"
    );
    let driver = gpu_run::selected_driver().driver;
    let empty = shot(Shot::Empty)?;

    // Burrow: the bubble's above-ground area, inside the standing bubble's screen disc.
    let unit = report.burrower;
    let standing = disc_changes(&empty, &shot(Shot::Upright)?, unit);
    let sunk = disc_changes(&empty, &shot(Shot::BurrowedBare)?, unit);
    eprintln!(
        "driver {driver}: standing bubble changes {standing} pixels of its disc, burrowed {sunk} \
         ({:.1} %)",
        100.0 * sunk as f64 / standing.max(1) as f64
    );
    assert!(standing > 200, "the standing unit is on screen: {standing}");
    assert!(
        (sunk as f64) <= BURROW_AREA_SHARE * standing as f64,
        "the burrowed bubble shows {sunk} of {standing} pixels"
    );
    assert!(
        (sunk as f64) >= BURROW_AREA_FLOOR * standing as f64,
        "its owner still sees the burrowed unit: {sunk} of {standing}"
    );
    // The mound: every pixel the looks change lies inside its ring on the ground.
    let (mound, outside, farthest) = mound_changes(
        &shot(Shot::BurrowedBare)?,
        &shot(Shot::Burrowed)?,
        &report.camera,
        unit,
    );
    eprintln!(
        "driver {driver}: the burrowed looks change {mound} pixels, {outside} beyond the ring; \
         farthest at {farthest:.2} radii"
    );
    assert!(mound >= MOUND_MIN_PIXELS, "the mound is drawn: {mound}");
    assert_eq!(outside, 0, "the mound stays inside its ring");

    // Undetected cloaked enemy: only a bend of what is behind it, and nothing without it.
    let enemy = report.shimmer;
    let bare = disc_patch_changes(&empty, &shot(Shot::ShimmerBare)?, enemy, 1.6);
    let shimmer = shimmer_check(&empty, &shot(Shot::Shimmer)?, enemy);
    eprintln!(
        "driver {driver}: undetected cloaked enemy without its looks: {bare} pixels; its \
         shimmer bends {} pixels, {} of them to a colour not on the ground near them, mean \
         colour moves {:.2} levels",
        shimmer.bent, shimmer.foreign, shimmer.mean_shift
    );
    assert!(
        bare <= TOLERATED_PIXELS,
        "the cloaked enemy leaks {bare} pixels"
    );
    assert!(
        shimmer.bent >= SHIMMER_MIN_PIXELS,
        "the shimmer is there to see"
    );
    assert!(
        (shimmer.foreign as f64) <= SHIMMER_FOREIGN_SHARE * shimmer.bent as f64,
        "the shimmer draws colours of its own: {} of {} bent pixels",
        shimmer.foreign,
        shimmer.bent
    );
    assert!(
        shimmer.mean_shift <= SHIMMER_MEAN_LEVELS,
        "the shimmer tints its patch by {:.2} levels",
        shimmer.mean_shift
    );

    // Siege mid-transition: the arc's share of the turn against the switch's progress.
    let drawn = arc_share(
        &shot(Shot::SiegeMidBare)?,
        &shot(Shot::SiegeMid)?,
        &report.camera,
        report.siege,
    );
    eprintln!(
        "driver {driver}: siege switch at {:.3}, its arc draws {drawn:.3} of the turn",
        report.siege_progress
    );
    assert!(report.siege_progress >= SIEGE_CATCH, "{report:?}");
    assert!(
        (drawn - report.siege_progress).abs() <= ARC_TOLERANCE,
        "the arc draws {drawn:.3} of the turn at progress {:.3}",
        report.siege_progress
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
        let report = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ron"));
        if name.starts_with(prefix) {
            std::fs::copy(&path, target.join(format!("{driver}_{name}")))?;
        } else if report {
            // Each child writes `report.ron`; the copy is named after the child's pictures.
            std::fs::copy(&path, target.join(format!("{driver}_{prefix}{name}")))?;
        }
    }
    Ok(())
}

/// Largest channel change between two pixels, in levels.
fn change(a: [u8; 3], b: [u8; 3]) -> u8 {
    (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0)
}

/// Pixels inside `unit`'s screen disc that differ between `a` and `b`.
fn disc_changes(a: &image::RgbImage, b: &image::RgbImage, unit: UnitView) -> usize {
    disc_patch_changes(a, b, unit, 1.0)
}

/// Pixels within `scale` screen radii of `unit`'s body that differ between `a` and `b`.
fn disc_patch_changes(
    a: &image::RgbImage,
    b: &image::RgbImage,
    unit: UnitView,
    scale: f32,
) -> usize {
    let mut changed = 0;
    for_disc(a, unit, scale, |x, y| {
        if change(a.get_pixel(x, y).0, b.get_pixel(x, y).0) > CHANGE_LEVEL {
            changed += 1;
        }
    });
    changed
}

/// Calls `visit` for every pixel of `image` within `scale` screen radii of `unit`'s body.
fn for_disc(image: &image::RgbImage, unit: UnitView, scale: f32, mut visit: impl FnMut(u32, u32)) {
    let [cx, cy] = unit.centre_px;
    let reach = unit.radius_px * scale;
    let low_x = (cx - reach).floor().max(0.0) as u32;
    let low_y = (cy - reach).floor().max(0.0) as u32;
    let high_x = ((cx + reach).ceil() as u32).min(image.width());
    let high_y = ((cy + reach).ceil() as u32).min(image.height());
    for y in low_y..high_y {
        for x in low_x..high_x {
            if Vec2::new(x as f32 + 0.5 - cx, y as f32 + 0.5 - cy).length() <= reach {
                visit(x, y);
            }
        }
    }
}

/// The pixels `with` changes against `without` around `unit` (within 2.6 radii on screen: the ring and a margin beyond it, short of the units nearby),
/// how many of them lie on the ground beyond the mound's ring (2 px tolerance), and the
/// farthest one's distance in radii.
fn mound_changes(
    without: &image::RgbImage,
    with: &image::RgbImage,
    camera: &CameraView,
    unit: UnitView,
) -> (usize, usize, f32) {
    let ground = Vec3::from_array(unit.ground);
    let world_per_px = unit.radius / unit.radius_px.max(1.0);
    let ring = MOUND_RING_RADII * unit.radius + EDGE_PX * world_per_px;
    let (mut changed, mut outside, mut farthest) = (0, 0, 0.0_f32);
    for_disc(without, unit, 2.6, |x, y| {
        if change(without.get_pixel(x, y).0, with.get_pixel(x, y).0) <= CHANGE_LEVEL {
            return;
        }
        changed += 1;
        let Some(point) = camera.unproject(x as f32, y as f32, ground.y) else {
            outside += 1;
            return;
        };
        let distance = Vec2::new(point.x - ground.x, point.z - ground.z).length();
        farthest = farthest.max(distance / unit.radius);
        if distance > ring {
            outside += 1;
        }
    });
    (changed, outside, farthest)
}

/// What a shimmer did to its patch.
struct Shimmer {
    /// Pixels bent by more than [`SHIMMER_LEVEL`].
    bent: usize,
    /// Bent pixels whose colour the ground does not have within [`SHIMMER_REACH_PX`].
    foreign: usize,
    /// Largest per-channel move of the patch's mean colour, levels.
    mean_shift: f64,
}

/// Compares `shimmer` with `empty` around `unit` (1.6 screen radii).
fn shimmer_check(empty: &image::RgbImage, shimmer: &image::RgbImage, unit: UnitView) -> Shimmer {
    let mut result = Shimmer {
        bent: 0,
        foreign: 0,
        mean_shift: 0.0,
    };
    let mut sums = [[0.0_f64; 3]; 2];
    let mut count = 0.0;
    for_disc(empty, unit, 1.6, |x, y| {
        let (before, after) = (empty.get_pixel(x, y).0, shimmer.get_pixel(x, y).0);
        for c in 0..3 {
            sums[0][c] += f64::from(before[c]);
            sums[1][c] += f64::from(after[c]);
        }
        count += 1.0;
        if change(before, after) <= SHIMMER_LEVEL {
            return;
        }
        result.bent += 1;
        let (mut low, mut high) = ([255_i16; 3], [0_i16; 3]);
        for dy in -SHIMMER_REACH_PX..=SHIMMER_REACH_PX {
            for dx in -SHIMMER_REACH_PX..=SHIMMER_REACH_PX {
                let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);
                if nx < 0
                    || ny < 0
                    || nx >= i64::from(empty.width())
                    || ny >= i64::from(empty.height())
                {
                    continue;
                }
                let around = empty.get_pixel(nx as u32, ny as u32).0;
                for c in 0..3 {
                    low[c] = low[c].min(i16::from(around[c]));
                    high[c] = high[c].max(i16::from(around[c]));
                }
            }
        }
        let inside = (0..3).all(|c| {
            let value = i16::from(after[c]);
            value >= low[c] - SHIMMER_SLACK && value <= high[c] + SHIMMER_SLACK
        });
        if !inside {
            result.foreign += 1;
        }
    });
    if count > 0.0 {
        result.mean_shift = (0..3)
            .map(|c| ((sums[1][c] - sums[0][c]) / count).abs())
            .fold(0.0, f64::max);
    }
    result
}

/// The share of the turn the arc drawn between `without` and `with` covers round `unit`: one
/// minus the largest run of angular bins on its band with no arc pixel.
fn arc_share(
    without: &image::RgbImage,
    with: &image::RgbImage,
    camera: &CameraView,
    unit: UnitView,
) -> f32 {
    let ground = Vec3::from_array(unit.ground);
    let mut bins = [false; ARC_BINS];
    let reach = UnitView {
        radius_px: unit.radius_px,
        ..unit
    };
    for_disc(without, reach, 2.8, |x, y| {
        if change(without.get_pixel(x, y).0, with.get_pixel(x, y).0) <= CHANGE_LEVEL {
            return;
        }
        let Some(point) = camera.unproject(x as f32, y as f32, ground.y) else {
            return;
        };
        let offset = Vec2::new(point.x - ground.x, point.z - ground.z);
        let radii = offset.length() / unit.radius;
        if radii < ARC_BAND.0 || radii > ARC_BAND.1 {
            return;
        }
        let turn = offset.y.atan2(offset.x) / std::f32::consts::TAU + 0.5;
        let bin = ((turn * ARC_BINS as f32) as usize).min(ARC_BINS - 1);
        bins[bin] = true;
    });
    if !bins.iter().any(|lit| *lit) {
        return 0.0;
    }
    // The longest circular run of unlit bins is the part of the turn not yet drawn.
    let mut longest = 0;
    let mut run = 0;
    for index in 0..ARC_BINS * 2 {
        if bins[index % ARC_BINS] {
            run = 0;
        } else {
            run += 1;
            longest = longest.max(run.min(ARC_BINS));
        }
    }
    1.0 - longest as f32 / ARC_BINS as f32
}

// ---------------------------------------------------------------------------------------------
// Staging shared by both children
// ---------------------------------------------------------------------------------------------

/// The latest tick the session published, or 0 before the first.
fn published_tick(session: &BattleSession) -> u32 {
    session.published().map_or(0, |published| published.tick)
}

/// Whether the 5 by 5 cells around `point` are open, walkable ground (a ring of 1.85 radii round
/// a unit stays on the ground).
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
fn find_spots(world: &SimWorld, fog: &BattleFog, around: Vec2, count: usize) -> Vec<Vec2> {
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
        let apart = spots
            .iter()
            .all(|spot| spot.distance(point) > SPOT_CLEARANCE);
        if open(world, point) && units && nodes && apart && fog.sees(point.x, point.y) {
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

/// The presentable burrowing unit type of `world` that can switch into its burrowed mode by
/// command, and that mode's name: [`burrower`]'s unit, given the shipped burrowing unit's
/// mode-switch ability in the session's catalog when it is a stand-in.
fn burrowing_unit(world: &mut SimWorld) -> (UnitTypeId, String) {
    let (unit, mode) = burrower(world, &assets());
    let switch = world
        .catalog
        .archetypes()
        .flat_map(|arch| arch.abilities.iter())
        .find(|ability| {
            matches!(&ability.effect, AbilityEffectSpec::SwitchMode { mode_name } if *mode_name == mode)
        })
        .cloned()
        .expect("the shipped burrowing unit switches into its burrowed mode");
    let archetype = world
        .catalog
        .get_mut(unit)
        .expect("invariant: burrower returns a type of this catalog");
    if !archetype
        .abilities
        .iter()
        .any(|ability| ability.effect == switch.effect)
    {
        archetype.abilities.push(switch);
    }
    (unit, mode)
}

/// A presentable mobile ground unit type of `world` with a mode that anchors it and is not
/// burrowed, and that mode's name.
fn anchoring_unit(world: &SimWorld) -> (UnitTypeId, String) {
    world
        .catalog
        .archetypes()
        .find_map(|arch| {
            let mode = arch
                .modes
                .iter()
                .find(|mode| mode.is_immobile && !mode.burrowed)?;
            (arch.max_speed > SimScalar::ZERO
                && arch.mover_type == MoverType::Ground
                && presentable(&assets(), &arch.slug))
            .then(|| (arch.unit_type_id, mode.mode_name.clone()))
        })
        .expect("the shipped catalog has a presentable anchoring unit")
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

/// Puts `id` in its burrowed mode `mode`, as the end of the mode's channel leaves it.
fn burrow(world: &mut SimWorld, id: EntityId, mode: &str) {
    let unit = world.entities.get_mut(&id).expect("it stands");
    unit.is_cloaked = true;
    unit.active_mode = Some(mode.to_owned());
    unit.is_immobile = true;
}

/// Asks the viewer's unit `id` to switch into or out of `mode`.
fn switch_mode(session: &mut BattleSession, id: EntityId, mode: &str, direction: ModeDirection) {
    let (world, queue) = session.world_and_commands();
    queue
        .submit_mode_switch(
            world.expect("a ready session has a world"),
            VIEWER,
            &[id],
            mode,
            direction,
            false,
        )
        .expect("the unit can switch now");
}

/// The view of `bubble`'s unit through `camera`.
fn unit_view(bubble: &BubbleInstance, camera: (&Camera, &GlobalTransform)) -> UnitView {
    let (camera, transform) = camera;
    let record = &bubble.0;
    let [x, y, altitude] = record.world_pos;
    let ground = Vec3::new(x, record.ground_z, y);
    let body = Vec3::new(x, altitude + record.radius, y);
    let view = camera_view(camera, transform);
    let centre = view.project(body);
    let edge = view.project(body + *transform.right() * record.radius);
    UnitView {
        ground: ground.to_array(),
        radius: record.radius,
        centre_px: centre.to_array(),
        radius_px: centre.distance(edge),
    }
}

/// The camera as the parent measures with it.
fn camera_view(camera: &Camera, transform: &GlobalTransform) -> CameraView {
    let rectangle = camera
        .physical_viewport_rect()
        .expect("the camera renders through a viewport");
    let clip_from_world = camera.clip_from_view() * transform.compute_matrix().inverse();
    CameraView {
        clip_from_world: clip_from_world.to_cols_array(),
        viewport: [
            rectangle.min.x as f32,
            rectangle.min.y as f32,
            rectangle.width() as f32,
            rectangle.height() as f32,
        ],
    }
}

/// How the effect layers are drawn for a shot.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
struct EffectView {
    /// No effect layer is drawn.
    bare: bool,
    /// Only `Critical` layers are drawn.
    critical_only: bool,
    /// This unit's bubble is not drawn.
    hidden: Option<EntityId>,
}

/// Applies [`EffectView`] after the frame's effects are built: a bare shot draws no card,
/// decal, distortion or light.
fn apply_effect_view(view: Res<'_, EffectView>, runtime: Option<ResMut<'_, EffectRuntime>>) {
    let Some(mut runtime) = runtime else {
        return;
    };
    let only = view.critical_only.then_some(Priority::Critical);
    if runtime.only != only {
        runtime.only = only;
    }
    if view.bare {
        runtime.frame.sorted.clear();
        runtime.frame.decals.clear();
        runtime.frame.decal_owners.clear();
        runtime.frame.distortions.clear();
        runtime.frame.lights.clear();
    }
}

/// Hides the bubble [`EffectView::hidden`] names, after the fog has shown it.
fn hide_unit(
    view: Res<'_, EffectView>,
    mut bubbles: Query<'_, '_, (&BubbleInstance, &mut Visibility), With<LiveBattleBubble>>,
) {
    let Some(hidden) = view.hidden else {
        return;
    };
    for (bubble, mut visibility) in &mut bubbles {
        if bubble.0.entity_id == hidden {
            visibility.set_if_neq(Visibility::Hidden);
        }
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
            trigger
                .event()
                .0
                .clone()
                .try_into_dynamic()
                .expect("a screenshot converts to an image")
                .to_rgb8()
                .save(&path)
                .expect("a screenshot must be written");
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
            .add_systems(Render, track_render_idle);
    }
    app.insert_resource(idle)
        .insert_resource(staging)
        .init_resource::<EffectView>()
        .add_systems(Update, (system, hide_unit).in_set(BattleSet::Present))
        .add_systems(
            PostUpdate,
            apply_effect_view.after(client::effects::systems::build_effect_frame),
        )
        .add_systems(Last, finish_capture);
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// Render frames in a row with no pipeline waiting before a picture is taken: a bubble or an
/// effect whose pipeline is still compiling is not drawn yet.
const IDLE_FRAMES: u32 = 8;

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
    spots: Vec<Vec2>,
    burrower: Option<(EntityId, String)>,
    enemy: Option<EntityId>,
    siege: Option<EntityId>,
    report: PoseReport,
}

/// Makes the change shot `index` shows.
fn change_pose_world(
    staging: &mut PoseStaging,
    session: &mut BattleSession,
    view: &mut EffectView,
    index: usize,
) {
    let shot = SHOTS[index];
    *view = EffectView::default();
    match shot {
        Shot::Empty => hold_own_units(session),
        Shot::Upright => {
            let world = session.world_mut().expect("a ready session has a world");
            let (unit_type, mode) = burrowing_unit(world);
            let id = spawn_still(world, unit_type, VIEWER, staging.spots[0]);
            world
                .entities
                .get_mut(&id)
                .expect("just spawned")
                .is_immobile = true;
            staging.burrower = Some((id, mode));
        }
        Shot::Burrowed => {
            let (id, mode) = staging
                .burrower
                .clone()
                .expect("the upright shot spawned it");
            burrow(
                session.world_mut().expect("a ready session has a world"),
                id,
                &mode,
            );
        }
        Shot::Shimmer => {
            let world = session.world_mut().expect("a ready session has a world");
            let (unit_type, _) = burrower(world, &assets());
            let id = spawn_still(world, unit_type, ENEMY, staging.spots[1]);
            let unit = world.entities.get_mut(&id).expect("just spawned");
            unit.is_cloaked = true;
            unit.is_immobile = true;
            staging.enemy = Some(id);
        }
        Shot::SiegeMid => {
            let world = session.world_mut().expect("a ready session has a world");
            let (unit_type, mode) = anchoring_unit(world);
            let id = spawn_still(world, unit_type, VIEWER, staging.spots[2]);
            staging.siege = Some(id);
            session.publish();
            switch_mode(session, id, &mode, ModeDirection::Enter);
            view.critical_only = true;
            view.hidden = Some(id);
        }
        Shot::BurrowedBare | Shot::ShimmerBare => view.bare = true,
        Shot::SiegeMidBare => {
            view.bare = true;
            view.hidden = staging.siege;
        }
    }
}

/// Whether shot `index` may stop the clock: the siege switch must be half way.
fn pose_ready(
    staging: &PoseStaging,
    index: usize,
    bubbles: &Query<
        '_,
        '_,
        (&BubbleInstance, &GlobalTransform, Option<&StanceTrack>),
        With<LiveBattleBubble>,
    >,
) -> bool {
    if SHOTS[index] != Shot::SiegeMid {
        return true;
    }
    let Some(siege) = staging.siege else {
        return false;
    };
    bubbles.iter().any(|(bubble, ..)| {
        bubble.0.entity_id == siege
            && bubble
                .0
                .stance
                .transition
                .is_some_and(|transition| channel_progress(&transition) >= SIEGE_CATCH)
    })
}

/// Records what shot `index` shows before it is captured.
fn record_pose(
    staging: &mut PoseStaging,
    index: usize,
    fog: &BattleFog,
    bubbles: &Query<
        '_,
        '_,
        (&BubbleInstance, &GlobalTransform, Option<&StanceTrack>),
        With<LiveBattleBubble>,
    >,
    camera: (&Camera, &GlobalTransform),
) {
    let shot = SHOTS[index];
    staging.report.camera = camera_view(camera.0, camera.1);
    let find = |id: Option<EntityId>| {
        bubbles
            .iter()
            .find(|(bubble, ..)| Some(bubble.0.entity_id) == id)
    };
    let burrower = staging.burrower.as_ref().map(|(id, _)| *id);
    let (watched, slot) = match shot {
        Shot::Upright | Shot::Burrowed | Shot::BurrowedBare => (burrower, 0),
        Shot::Shimmer | Shot::ShimmerBare => (staging.enemy, 1),
        Shot::SiegeMid | Shot::SiegeMidBare => (staging.siege, 2),
        Shot::Empty => (None, 3),
    };
    let Some((bubble, _, track)) = find(watched) else {
        return;
    };
    let view = unit_view(bubble, camera);
    match slot {
        0 if shot == Shot::Upright => staging.report.burrower = view,
        1 => staging.report.shimmer = view,
        2 => {
            staging.report.siege = view;
            if shot == Shot::SiegeMid {
                staging.report.siege_progress = track
                    .and_then(|track| track.morph)
                    .map_or(0.0, |morph| morph.progress);
            }
        }
        _ => {}
    }
    staging.report.published.push((
        shot,
        bubble.0.concealment,
        format!("{:?}", fog.presence(&bubble.0)),
    ));
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
    bubbles: Query<
        '_,
        '_,
        (&BubbleInstance, &GlobalTransform, Option<&StanceTrack>),
        With<LiveBattleBubble>,
    >,
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
            staging.spots = find_spots(world, &fog, rts.target, 3);
            change_pose_world(&mut staging, &mut session, &mut view, 0);
            commands.queue(rebuild_tables);
            lab.battle_paused = false;
            PosePhase::Publishing {
                shot: 0,
                since: tick,
            }
        }
        PosePhase::Publishing { shot, since }
            if tick <= since || !pose_ready(&staging, shot, &bubbles) =>
        {
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
            record_pose(
                &mut staging,
                shot,
                &fog,
                &bubbles,
                (camera, camera_transform),
            );
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

/// The child of [`concealment_and_stance_poses_read_as_the_plan_asks`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn stance_pose_child() {
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
// The sequence child
// ---------------------------------------------------------------------------------------------

/// One frame's sample of a unit's look.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Sample {
    /// Seconds of `Time<Virtual>` since the switch was asked for.
    at: f32,
    /// The bubble's sink, world units.
    sink: f32,
    /// Whether the simulation still publishes the switch.
    channelling: bool,
    /// The published concealment.
    concealment: Concealment,
    /// The morph's presented progress, if a morph is held.
    progress: Option<f32>,
}

/// What the sequence child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SequenceReport {
    /// The burrowing unit's radius.
    radius: f32,
    /// Samples while it burrows.
    dig: Vec<Sample>,
    /// Samples while it comes up.
    emerge: Vec<Sample>,
    /// Samples while the anchoring unit sieges.
    siege: Vec<Sample>,
    /// Samples while it unsieges.
    unsiege: Vec<Sample>,
}

/// The sequences, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    Dig,
    Emerge,
    Siege,
    Unsiege,
}

const ACTS: [Act; 4] = [Act::Dig, Act::Emerge, Act::Siege, Act::Unsiege];

/// Seconds an act runs after the switch was asked for.
const ACT_SECONDS: [f64; 4] = [3.0, 1.5, 4.2, 4.0];
/// A picture every this many seconds of an act.
const PICTURE_STEP: f64 = 0.12;

/// What the sequence child is doing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum SequencePhase {
    #[default]
    Boot,
    Booting {
        still: u32,
        height: f32,
    },
    /// Act `act` runs since `at`; `pictures` were taken.
    Acting {
        act: usize,
        at: f64,
        pictures: u32,
    },
    /// The tableau's units are staged since `at`.
    Tableau {
        at: f64,
    },
    /// The tableau is shown without its effect layers since `at` (what the units show alone).
    Bare {
        at: f64,
    },
    /// A detector stands beside the tableau's enemy cloaked unit since `at`.
    Detected {
        at: f64,
    },
    /// A picture is being written; its completion moves the staging on.
    Capturing,
}

/// Seconds the detected tableau runs before its picture: detection is published, the mark
/// comes in.
const DETECT_SECONDS: f64 = 1.5;
/// Seconds the bare tableau is shown before its picture.
const BARE_SECONDS: f64 = 0.4;

/// The sequence child's staging.
#[derive(Resource, Default)]
struct SequenceStaging {
    directory: PathBuf,
    phase: SequencePhase,
    spots: Vec<Vec2>,
    burrower: Option<(EntityId, String)>,
    siege: Option<(EntityId, String)>,
    report: SequenceReport,
    /// A screenshot is being written.
    busy: bool,
}

/// Starts act `act`: spawns its unit on the first act of each and asks for the switch.
fn start_act(staging: &mut SequenceStaging, session: &mut BattleSession, act: Act) {
    match act {
        Act::Dig => {
            let world = session.world_mut().expect("a ready session has a world");
            let (unit_type, mode) = burrowing_unit(world);
            let id = spawn_still(world, unit_type, VIEWER, staging.spots[0]);
            staging.burrower = Some((id, mode.clone()));
            session.publish();
            switch_mode(session, id, &mode, ModeDirection::Enter);
        }
        Act::Emerge => {
            let (id, mode) = staging.burrower.clone().expect("the dig spawned it");
            switch_mode(session, id, &mode, ModeDirection::Leave);
        }
        Act::Siege => {
            let world = session.world_mut().expect("a ready session has a world");
            let (unit_type, mode) = anchoring_unit(world);
            let id = spawn_still(world, unit_type, VIEWER, staging.spots[1]);
            staging.siege = Some((id, mode.clone()));
            session.publish();
            switch_mode(session, id, &mode, ModeDirection::Enter);
        }
        Act::Unsiege => {
            let (id, mode) = staging.siege.clone().expect("the siege spawned it");
            switch_mode(session, id, &mode, ModeDirection::Leave);
        }
    }
}

/// Stages the tableau: the burrowing unit burrowed again, a cloaked unit of the viewer's, an
/// enemy cloaked unit the viewer does not detect (a detector comes beside it for the second
/// picture, [`detect_in_tableau`]), and the anchoring unit sieged again.
fn stage_tableau(staging: &SequenceStaging, session: &mut BattleSession) {
    let world = session.world_mut().expect("a ready session has a world");
    if let Some((id, mode)) = &staging.burrower {
        burrow(world, *id, mode);
    }
    let (cloaker, _) = burrower(world, &assets());
    let own = spawn_still(world, cloaker, VIEWER, staging.spots[2]);
    world
        .entities
        .get_mut(&own)
        .expect("just spawned")
        .is_cloaked = true;
    let hidden = spawn_still(world, cloaker, ENEMY, staging.spots[3]);
    world
        .entities
        .get_mut(&hidden)
        .expect("just spawned")
        .is_cloaked = true;
    for unit in world.entities.values_mut() {
        if unit.owner == ENEMY && unit.is_cloaked {
            unit.is_immobile = true;
        }
    }
    if let Some((id, mode)) = &staging.siege {
        let (id, mode) = (*id, mode.clone());
        switch_mode(session, id, &mode, ModeDirection::Enter);
    }
}

/// Puts a detector of the viewer's beside the tableau's enemy cloaked unit, so the viewer
/// detects it.
fn detect_in_tableau(world: &mut World) {
    let spot = world.resource::<SequenceStaging>().spots[3];
    let mut session = world.resource_mut::<BattleSession>();
    let sim = session.world_mut().expect("a ready session has a world");
    let detector = sim
        .catalog
        .archetypes()
        .find(|arch| {
            arch.is_detector
                && arch.max_speed > SimScalar::ZERO
                && presentable(&assets(), &arch.slug)
        })
        .expect("the shipped catalog has a presentable mobile detector")
        .unit_type_id;
    spawn_still(sim, detector, VIEWER, spot + Vec2::new(1.6, 0.0));
    world.resource_mut::<LabState>().battle_paused = false;
    let now = world.resource::<Time<Virtual>>().elapsed_secs_f64();
    world.resource_mut::<SequenceStaging>().phase = SequencePhase::Detected { at: now };
}

/// The live bubbles with their materials and tracks.
type ShellQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static BubbleInstance,
        &'static MeshMaterial3d<BubbleDomeMaterial>,
        Option<&'static StanceTrack>,
    ),
    With<LiveBattleBubble>,
>;

/// The sample of `id`'s look now.
fn sample(
    id: EntityId,
    at: f32,
    bubbles: &ShellQuery<'_, '_>,
    materials: &Assets<BubbleDomeMaterial>,
) -> Option<Sample> {
    let (bubble, material, track) = bubbles
        .iter()
        .find(|(bubble, ..)| bubble.0.entity_id == id)?;
    Some(Sample {
        at,
        sink: materials
            .get(&material.0)
            .map_or(0.0, |material| material.shell.shape.x),
        channelling: bubble.0.stance.transition.is_some(),
        concealment: bubble.0.concealment,
        progress: track
            .and_then(|track| track.morph)
            .map(|morph| morph.progress),
    })
}

/// Advances the sequence staging by one frame.
fn stage_sequences(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, SequenceStaging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    time: Res<'_, Time<Virtual>>,
    fog: Option<Res<'_, BattleFog>>,
    bubbles: ShellQuery<'_, '_>,
    materials: Res<'_, Assets<BubbleDomeMaterial>>,
    cameras: Query<'_, '_, &RtsCamera>,
    idle: Res<'_, LabRenderIdle>,
) {
    let fog = fog.expect("the skirmish presents a player, so it keeps a fog");
    if !session.is_ready() {
        return;
    }
    let Ok(rts) = cameras.get_single() else {
        return;
    };
    let tick = published_tick(&session);
    let now = time.elapsed_secs_f64();
    staging.phase = match staging.phase {
        SequencePhase::Boot if tick < 2 => SequencePhase::Boot,
        SequencePhase::Boot => SequencePhase::Booting {
            still: 0,
            height: rts.look_at_y,
        },
        SequencePhase::Booting { still, height }
            if still < STILL_CAMERA_FRAMES || !renderer_idle(&idle) =>
        {
            let (still, height) = camera_still(still, height, rts);
            SequencePhase::Booting { still, height }
        }
        SequencePhase::Booting { .. } => {
            let world = session.world().expect("a ready session has a world");
            staging.spots = find_spots(world, &fog, rts.target, 4);
            hold_own_units(&mut session);
            start_act(&mut staging, &mut session, ACTS[0]);
            commands.queue(rebuild_tables);
            lab.battle_paused = false;
            SequencePhase::Acting {
                act: 0,
                at: now,
                pictures: 0,
            }
        }
        SequencePhase::Acting { act, at, pictures } => {
            let since = (now - at) as f32;
            let unit = match ACTS[act] {
                Act::Dig | Act::Emerge => staging.burrower.as_ref().map(|(id, _)| *id),
                Act::Siege | Act::Unsiege => staging.siege.as_ref().map(|(id, _)| *id),
            };
            if let Some(found) = unit.and_then(|id| sample(id, since, &bubbles, &materials)) {
                if staging.report.radius == 0.0 {
                    if let Some((bubble, ..)) = bubbles
                        .iter()
                        .find(|(bubble, ..)| Some(bubble.0.entity_id) == unit)
                    {
                        staging.report.radius = bubble.0.radius;
                    }
                }
                let list = match ACTS[act] {
                    Act::Dig => &mut staging.report.dig,
                    Act::Emerge => &mut staging.report.emerge,
                    Act::Siege => &mut staging.report.siege,
                    Act::Unsiege => &mut staging.report.unsiege,
                };
                list.push(found);
            }
            let mut pictures = pictures;
            if !staging.busy && f64::from(since) >= f64::from(pictures) * PICTURE_STEP {
                let name = format!("seq_{:?}_{pictures:03}.png", ACTS[act]).to_lowercase();
                staging.busy = true;
                capture(
                    &mut commands,
                    staging.directory.join(name),
                    |world: &mut World| {
                        world.resource_mut::<SequenceStaging>().busy = false;
                    },
                );
                pictures += 1;
            }
            if now - at < ACT_SECONDS[act] {
                SequencePhase::Acting { act, at, pictures }
            } else if act + 1 < ACTS.len() {
                start_act(&mut staging, &mut session, ACTS[act + 1]);
                commands.queue(rebuild_tables);
                SequencePhase::Acting {
                    act: act + 1,
                    at: now,
                    pictures: 0,
                }
            } else {
                stage_tableau(&staging, &mut session);
                SequencePhase::Tableau { at: now }
            }
        }
        SequencePhase::Tableau { at } if now - at < ACT_SECONDS[2] || staging.busy => {
            SequencePhase::Tableau { at }
        }
        SequencePhase::Tableau { .. } => {
            lab.battle_paused = true;
            let text = ron::to_string(&staging.report).expect("plain data");
            std::fs::write(staging.directory.join(REPORT_FILE), text)
                .expect("the parent's directory is writable");
            capture(
                &mut commands,
                staging.directory.join("seq_tableau.png"),
                |world: &mut World| {
                    world.resource_mut::<EffectView>().bare = true;
                    let now = world.resource::<Time<Virtual>>().elapsed_secs_f64();
                    world.resource_mut::<SequenceStaging>().phase = SequencePhase::Bare { at: now };
                },
            );
            SequencePhase::Capturing
        }
        SequencePhase::Bare { at } if now - at < BARE_SECONDS => SequencePhase::Bare { at },
        SequencePhase::Bare { .. } => {
            capture(
                &mut commands,
                staging.directory.join("seq_tableau_bare.png"),
                |world: &mut World| {
                    world.resource_mut::<EffectView>().bare = false;
                    detect_in_tableau(world);
                },
            );
            SequencePhase::Capturing
        }
        SequencePhase::Detected { at } if now - at < DETECT_SECONDS => {
            SequencePhase::Detected { at }
        }
        SequencePhase::Detected { .. } => {
            lab.battle_paused = true;
            capture(
                &mut commands,
                staging.directory.join("seq_tableau_detected.png"),
                |world: &mut World| {
                    world.send_event(AppExit::Success);
                },
            );
            SequencePhase::Capturing
        }
        SequencePhase::Capturing => SequencePhase::Capturing,
    };
}

#[test]
fn a_burrow_settles_after_its_channel_and_stances_follow_their_switches() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args([
        "--ignored",
        "--exact",
        "--nocapture",
        "stance_sequence_child",
    ]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("stance sequences");
    let report: SequenceReport = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    keep_evidence(directory.path(), "seq_")?;
    let full = 0.9 * 2.0 * report.radius;
    let driver = gpu_run::selected_driver().driver;

    // The dig: the body never rises, the channel ends before it is fully sunk (the stats change
    // at 33 ticks, the dig runs to 45, D-6) and it is fully sunk after that.
    let dig = &report.dig;
    assert!(
        dig.iter().any(|sample| sample.channelling),
        "the burrow was published: {dig:?}"
    );
    for pair in dig.windows(2) {
        assert!(
            pair[1].sink + 1e-4 >= pair[0].sink,
            "the digging body rose: {pair:?}"
        );
    }
    let ended = dig
        .iter()
        .position(|sample| !sample.channelling && sample.concealment == Concealment::Burrowed)
        .expect("the channel ends inside the act");
    let at_end = dig[ended];
    eprintln!(
        "driver {driver}: the burrow's channel ended at {:.2} s with the body {:.0} % sunk",
        at_end.at,
        100.0 * at_end.sink / full
    );
    assert!(
        at_end.sink < full - 1e-3,
        "the dig settles after the channel: {at_end:?}"
    );
    let settled = dig
        .iter()
        .find(|sample| sample.sink >= full - 1e-3)
        .expect("the body ends fully sunk");
    assert!(settled.at > at_end.at, "{settled:?} after {at_end:?}");
    assert!(
        (settled.at - at_end.at) < 1.0,
        "the settle takes the dig's remaining 12 ticks, not more: {settled:?} after {at_end:?}"
    );

    // The emerge: the body never sinks again and stands fully up at the end.
    let emerge = &report.emerge;
    for pair in emerge.windows(2) {
        assert!(
            pair[1].sink <= pair[0].sink + 1e-4,
            "the emerging body sank: {pair:?}"
        );
    }
    assert!(
        emerge.last().is_some_and(|sample| sample.sink == 0.0),
        "{emerge:?}"
    );

    // The siege and unsiege: a morph is held while the channel runs, its progress never goes
    // back, and none is held once the unit stands in its stance.
    for (name, samples) in [("siege", &report.siege), ("unsiege", &report.unsiege)] {
        let progress: Vec<f32> = samples
            .iter()
            .filter_map(|sample| sample.progress)
            .collect();
        assert!(
            !progress.is_empty(),
            "{name}: a morph was held: {samples:?}"
        );
        for pair in progress.windows(2) {
            assert!(
                pair[1] + 1e-5 >= pair[0],
                "{name}: the morph went back: {pair:?}"
            );
        }
        assert!(
            samples
                .last()
                .is_some_and(|sample| sample.progress.is_none() && !sample.channelling),
            "{name}: the switch completes inside the act"
        );
    }
    Ok(())
}

/// The child of [`a_burrow_settles_after_its_channel_and_stances_follow_their_switches`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn stance_sequence_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    run_child(
        SequenceStaging {
            directory: PathBuf::from(directory),
            ..SequenceStaging::default()
        },
        stage_sequences,
    );
}
