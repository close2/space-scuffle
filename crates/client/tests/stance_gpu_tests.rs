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
//! `concealment_gpu_tests.rs` does. The burrowing unit is the shipped one, with its own
//! burrowed-mode clip (plan WP13), whenever it has media, else the presentable stand-in of
//! `battle_harness::burrower` given the shipped unit's mode-switch ability in the session's
//! catalog; the effect tables are rebuilt from the session's catalog so a stand-in names its
//! mode's looks. The cloaked units are `burrower`'s unarmed unit. Every unit is chosen by
//! catalog properties.
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
use bevy::time::TimeUpdateStrategy;
use client::battle_pipeline::commands::ModeDirection;
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::bubble_shader::FlipbookRegistry;
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
use sim_types::{BubbleRenderInstance, ShapeKind};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

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
const SPOT_DISTANCES: [f32; 13] = [
    4.0, 5.0, 6.0, 7.0, 8.0, 3.0, 9.0, 10.0, 11.0, 3.5, 4.5, 5.5, 6.5,
];
/// Clearance around a spot, in world units, from any unit, structure, node or other spot: wide
/// enough that a neighbour's looks (up to 1.8 radii) stay out of a spot's checked patch.
const SPOT_CLEARANCE: f32 = 3.4;

/// A pixel counts as changed when one of its channels moves by more than this, in levels.
const CHANGE_LEVEL: u8 = 12;
/// Changed pixels tolerated where nothing may be drawn (plan WP3's rule).
const TOLERATED_PIXELS: usize = 2;
/// Plan WP8: the burrowed bubble's above-ground area is at most this share of the standing one.
const BURROW_AREA_SHARE: f64 = 0.35;
/// Amendment 12 (B-1): a unit with its own burrowed clip shows at least this share of the
/// pixels its standing bubble changes inside its screen disc.
const CLIP_SHOWN_SHARE: f64 = 0.85;
/// Amendment 12 (B-2): a burrowed unit's shadow is gone: round its body it changes at most
/// this share of the pixels its standing shadow changes.
const SHADOW_LEFT_SHARE: f64 = 0.15;
/// Where a unit's shadow is sought round its body: from this many screen radii (clear of the
/// bubble, which a shallow sink moves down by a fifth of its radius on screen) ...
const SHADOW_FROM_RADII: f32 = 1.3;
/// ... to this many.
const SHADOW_REACH_RADII: f32 = 2.5;
/// The burrowed bubble still shows its owner a little of itself: at least this share.
const BURROW_AREA_FLOOR: f64 = 0.02;
/// The mound's ring: its looks lie on the ground within this many radii of the unit.
const MOUND_RING_RADII: f32 = 1.8;
/// Pixels of tolerance at a ring's edge (plan §10.2).
const EDGE_PX: f32 = 2.0;
/// The mound is drawn: at least this many pixels change.
const MOUND_MIN_PIXELS: usize = 40;
/// The ground ring round a burrowed body, in radii, whose share the looks change is checked
/// (WP8 review 2, B2-1).
const MOUND_GROUND_RING: (f32, f32) = (1.05, 1.6);
/// The looks change at least this share of that ring where it shows beside the body.
const MOUND_RING_SHARE: f64 = 0.5;
/// Reach of a shimmer's bend in pixels: its colours must already be on the ground this near
/// (the largest bend, lens plus noise, is about 3 px at the default zoom).
const SHIMMER_REACH_PX: i64 = 6;
/// World units behind the anchoring unit (away from the camera) at which a neutral unit stands
/// on its switch's furrow: the furrow's radius, 1.85 of the anchoring unit's radius of 0.875.
const BEHIND_DISTANCE: f32 = 1.62;
/// A body pixel of a unit standing on a darkening element keeps at least this share of its
/// light (linear luminance): what the bubble's slight translucency lets through from the
/// darkened ground behind it, about 0.92, and no more (the soil leaves about 0.6).
const BODY_KEPT: f32 = 0.85;
/// Share of that body's pixels allowed below [`BODY_KEPT`] (its rim, which blends with the
/// ground at the outline).
const BODY_DARKENED_SHARE: f64 = 0.05;
/// Outer falloff of the anchored look: the ground's factor goes from 0.95 to 0.75 over at
/// least this share of the unit's radius (review 3, A3-1: round 3's edge stepped within a pixel).
const FALLOFF_RADII: f32 = 0.15;

/// Euclidean RGB distance, in levels, a shimmer pixel may lie from the colours a filtered read
/// of the ground around it can give ([`blend_of_neighbours`]).
const SHIMMER_SLACK: f32 = 6.0;
/// Share of bent pixels tolerated outside the ground's colours near them (filtering at the
/// bend's edges). Round 1 read the scene copy at viewport-relative coordinates, i.e. from
/// elsewhere on the screen, and failed this at 133 of 2 074 bent pixels with a shadow-coloured
/// spot at the enemy's feet; with the copy read where the fragment is, 0 of 1 172 (pose) and
/// 0 of 1 279 (tableau) on radv (2026-11-12).
const SHIMMER_FOREIGN_SHARE: f64 = 0.005;
/// A shimmer moves a patch's mean colour by at most this many levels (no tint); measured 0.18
/// to 0.19 on radv after the fix, 4.10 before it.
const SHIMMER_MEAN_LEVELS: f64 = 0.75;
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
    /// A unit without a burrowed clip of its own, standing (the stand-in).
    PlainUpright,
    /// The same unit burrowed, with its looks (review 2, F2-1).
    PlainBurrowed,
    /// The same unit burrowed, without the effect layers: the deep sink.
    PlainBurrowedBare,
}

const SHOTS: [Shot; 11] = [
    Shot::Empty,
    Shot::Upright,
    Shot::Burrowed,
    Shot::BurrowedBare,
    Shot::Shimmer,
    Shot::ShimmerBare,
    Shot::SiegeMid,
    Shot::SiegeMidBare,
    Shot::PlainUpright,
    Shot::PlainBurrowed,
    Shot::PlainBurrowedBare,
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
    /// The burrowing unit burrowed, its disc where its shallow sink draws it.
    burrowed: UnitView,
    /// The unit without a burrowed clip of its own.
    plain: UnitView,
    /// A unit of the viewer's with a neutral shell, away from the staged units: its shadow
    /// must be the same while a burrowed unit's shell moves its own (review 2, S-2).
    neutral: UnitView,
    /// The other bubbles near it, whose animated pictures its shadow ring must leave out.
    neutral_neighbours: Vec<UnitView>,
    /// The neutral unit on the back of the siege switch's furrow (review 3, E3-1).
    behind: UnitView,
    /// Whether the burrowing unit shows its own burrowed clip (WP13 media).
    burrower_has_clip: bool,
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

    // Burrow, a unit with its own burrowed clip (Amendment 12, B-1): the clip's disc mostly
    // shows, and its shadow is gone. A unit without one: sunk to a low dome.
    let unit = report.burrower;
    assert!(
        report.burrower_has_clip,
        "the shipped burrowing unit has its burrowed clip"
    );
    let standing = disc_changes(&empty, &shot(Shot::Upright)?, unit);
    let burrowed_bare = shot(Shot::BurrowedBare)?;
    let shown = disc_changes(&empty, &burrowed_bare, report.burrowed);
    let shadow_standing = ring_changes(&empty, &shot(Shot::Upright)?, unit);
    let shadow_burrowed = ring_changes(&empty, &burrowed_bare, unit);
    eprintln!(
        "driver {driver}: clip unit standing changes {standing} pixels of its disc, burrowed \
         {shown} ({:.1} %); round it (its shadow) {shadow_standing} standing, {shadow_burrowed} \
         burrowed",
        100.0 * shown as f64 / standing.max(1) as f64
    );
    assert!(standing > 200, "the standing unit is on screen: {standing}");
    assert!(
        (shown as f64) >= CLIP_SHOWN_SHARE * standing as f64,
        "the burrowed clip shows {shown} of {standing} pixels"
    );
    assert!(
        shadow_standing > 100,
        "the standing unit casts a shadow round it: {shadow_standing}"
    );
    assert!(
        (shadow_burrowed as f64) <= SHADOW_LEFT_SHARE * shadow_standing as f64,
        "the burrowed unit's shadow is gone: {shadow_burrowed} of {shadow_standing}"
    );
    // A unit standing on the back of the siege switch's furrow (review 3, E3-1): the furrow
    // darkens the ground round its foot, and its body keeps its light (darkening elements are
    // drawn before every transparent item, so they never multiply a bubble).
    let behind = report.behind;
    assert!(
        behind.radius_px > 0.0,
        "the unit behind the furrow is in sight"
    );
    let (siege_mid, siege_mid_bare) = (shot(Shot::SiegeMid)?, shot(Shot::SiegeMidBare)?);
    let body = body_kept(&siege_mid_bare, &siege_mid, behind);
    let ground_round = foot_darkened(&siege_mid_bare, &siege_mid, behind);
    eprintln!(
        "driver {driver}: a unit on the back of the siege furrow: {} of {} body pixels keep less \
         than {BODY_KEPT} of their light (lowest {:.3}); the furrow darkens {ground_round} pixels \
         round its foot",
        body.darkened, body.pixels, body.lowest
    );
    assert!(ground_round > 0, "the furrow runs under the unit behind it");
    assert!(
        (body.darkened as f64) <= BODY_DARKENED_SHARE * body.pixels as f64,
        "the furrow darkens the body of the unit standing on it: {} of {}",
        body.darkened,
        body.pixels
    );

    // Mixed bubbles (review 2, S-2): while the burrowed unit's shell moves its own shadow
    // caster (it alone is unbatched), a neutral unit of the viewer's, batched with the others,
    // casts the same shadow as when no shell moved any caster.
    let neutral = report.neutral;
    assert!(
        neutral.radius_px > 0.0,
        "a neutral unit of the viewer's is in sight"
    );
    let neutral_shadow = ring_changes_outside(
        &empty,
        &shot(Shot::Burrowed)?,
        neutral,
        &report.neutral_neighbours,
    );
    eprintln!(
        "driver {driver}: a neutral unit beside the burrowed one: {neutral_shadow} pixels of its \
         shadow ring changed against the empty shot"
    );
    assert!(
        neutral_shadow <= TOLERATED_PIXELS,
        "the neutral unit's shadow changed: {neutral_shadow} pixels"
    );
    let plain = report.plain;
    let plain_standing = disc_changes(&empty, &shot(Shot::PlainUpright)?, plain);
    let sunk = disc_changes(&empty, &shot(Shot::PlainBurrowedBare)?, plain);
    eprintln!(
        "driver {driver}: unit without a clip standing changes {plain_standing} pixels of its \
         disc, burrowed {sunk} ({:.1} %)",
        100.0 * sunk as f64 / plain_standing.max(1) as f64
    );
    assert!(plain_standing > 200, "the plain unit is on screen");
    assert!(
        (sunk as f64) <= BURROW_AREA_SHARE * plain_standing as f64,
        "the burrowed bubble shows {sunk} of {plain_standing} pixels"
    );
    assert!(
        (sunk as f64) >= BURROW_AREA_FLOOR * plain_standing as f64,
        "its owner still sees the burrowed unit: {sunk} of {plain_standing}"
    );
    // The mound: every pixel the looks change lies inside its ring on the ground, and they
    // change most of the ground round the body that shows beside it (review 2, B2-1: the
    // ground, not only the body, reads as dug in).
    let mound = mound_changes(
        &shot(Shot::BurrowedBare)?,
        &shot(Shot::Burrowed)?,
        &report.camera,
        unit,
    );
    eprintln!(
        "driver {driver}: the burrowed looks change {} pixels, {} beyond the ring; farthest at \
         {:.2} radii; they change {:.1} % of the ground between {} and {} radii beside the body",
        mound.changed,
        mound.outside,
        mound.farthest,
        100.0 * mound.ring_share,
        MOUND_GROUND_RING.0,
        MOUND_GROUND_RING.1
    );
    assert!(
        mound.changed >= MOUND_MIN_PIXELS,
        "the mound is drawn: {}",
        mound.changed
    );
    assert_eq!(mound.outside, 0, "the mound stays inside its ring");
    assert!(
        mound.ring_share >= MOUND_RING_SHARE,
        "the mound changes {:.1} % of the ground round the body",
        100.0 * mound.ring_share
    );

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

/// Pixels round `unit`'s body (between [`SHADOW_FROM_RADII`] and [`SHADOW_REACH_RADII`] of
/// its screen radius) that differ between `a` and `b`: where its shadow falls.
fn ring_changes(a: &image::RgbImage, b: &image::RgbImage, unit: UnitView) -> usize {
    let [cx, cy] = unit.centre_px;
    let mut changed = 0;
    for_disc(a, unit, SHADOW_REACH_RADII, |x, y| {
        let distance = Vec2::new(x as f32 + 0.5 - cx, y as f32 + 0.5 - cy).length();
        if distance > unit.radius_px * SHADOW_FROM_RADII
            && change(a.get_pixel(x, y).0, b.get_pixel(x, y).0) > CHANGE_LEVEL
        {
            changed += 1;
        }
    });
    changed
}

/// Linear luminance of an sRGB pixel (Rec. 709 weights).
fn linear_luminance(pixel: [u8; 3]) -> f32 {
    let channel = |value: u8| {
        let c = f32::from(value) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(pixel[0]) + 0.7152 * channel(pixel[1]) + 0.0722 * channel(pixel[2])
}

/// How much light the body of `unit` keeps in `after` against `before`.
struct BodyKept {
    /// Body pixels (within 0.9 screen radii, short of the rim).
    pixels: usize,
    /// Of them, those keeping less than [`BODY_KEPT`] of their light.
    darkened: usize,
    /// The lowest share kept.
    lowest: f32,
}

/// [`BodyKept`] of `unit` between `before` and `after`.
fn body_kept(before: &image::RgbImage, after: &image::RgbImage, unit: UnitView) -> BodyKept {
    let mut kept = BodyKept {
        pixels: 0,
        darkened: 0,
        lowest: 1.0,
    };
    for_disc(before, unit, 0.9, |x, y| {
        let was = linear_luminance(before.get_pixel(x, y).0);
        if was <= 1e-3 {
            return;
        }
        let share = linear_luminance(after.get_pixel(x, y).0) / was;
        kept.pixels += 1;
        kept.lowest = kept.lowest.min(share);
        kept.darkened += usize::from(share < BODY_KEPT);
    });
    kept
}

/// Pixels between 1.1 and 2 screen radii of `unit` that `after` darkens to below
/// [`BODY_KEPT`] of their light in `before`: the ground round it.
fn foot_darkened(before: &image::RgbImage, after: &image::RgbImage, unit: UnitView) -> usize {
    let [cx, cy] = unit.centre_px;
    let mut darkened = 0;
    for_disc(before, unit, 2.0, |x, y| {
        let distance = Vec2::new(x as f32 + 0.5 - cx, y as f32 + 0.5 - cy).length();
        let was = linear_luminance(before.get_pixel(x, y).0);
        if distance > 1.1 * unit.radius_px
            && was > 1e-3
            && linear_luminance(after.get_pixel(x, y).0) / was < BODY_KEPT
        {
            darkened += 1;
        }
    });
    darkened
}

/// The outer falloff of a darkening look round `unit`, in pixels: along every horizontal-ish
/// ray from its centre (within 30 degrees of left and right, where the screen is not
/// foreshortened), the distance from the outermost pixel keeping at most 0.75 of its light to
/// the outermost keeping at most 0.95; the median over the rays that reach 0.75, and their
/// count.
fn outer_falloff(
    before: &image::RgbImage,
    after: &image::RgbImage,
    unit: UnitView,
) -> (f32, usize) {
    let [cx, cy] = unit.centre_px;
    let reach = 3.0 * unit.radius_px;
    let mut widths = Vec::new();
    for step in -6_i32..=6 {
        for side in [1.0_f32, -1.0] {
            let angle = (step as f32 * 5.0).to_radians();
            let direction = Vec2::new(side * angle.cos(), angle.sin());
            let (mut at_75, mut at_95) = (None, None);
            let mut distance = unit.radius_px;
            while distance < reach {
                let point = Vec2::new(cx, cy) + direction * distance;
                let (x, y) = (point.x.round(), point.y.round());
                if x < 0.0 || y < 0.0 || x >= before.width() as f32 || y >= before.height() as f32 {
                    break;
                }
                let was = linear_luminance(before.get_pixel(x as u32, y as u32).0);
                if was > 1e-3 {
                    let kept = linear_luminance(after.get_pixel(x as u32, y as u32).0) / was;
                    if kept <= 0.75 {
                        at_75 = Some(distance);
                    }
                    if kept <= 0.95 {
                        at_95 = Some(distance);
                    }
                }
                distance += 0.5;
            }
            if let (Some(inner), Some(outer)) = (at_75, at_95) {
                widths.push(outer - inner);
            }
        }
    }
    widths.sort_by(f32::total_cmp);
    let median = widths.get(widths.len() / 2).copied().unwrap_or(0.0);
    (median, widths.len())
}

/// Pixels within `scale` screen radii of `unit`'s body that `after` makes lighter and darker
/// than `before` by more than [`LIGHTEN_LEVELS`] of luminance (Rec. 709 weights), and the
/// patch's size.
fn lightening(
    before: &image::RgbImage,
    after: &image::RgbImage,
    unit: UnitView,
    scale: f32,
) -> (usize, usize, usize) {
    let luminance = |pixel: [u8; 3]| {
        0.2126 * f32::from(pixel[0]) + 0.7152 * f32::from(pixel[1]) + 0.0722 * f32::from(pixel[2])
    };
    let (mut lighter, mut darker, mut patch) = (0, 0, 0);
    for_disc(before, unit, scale, |x, y| {
        patch += 1;
        let step = luminance(after.get_pixel(x, y).0) - luminance(before.get_pixel(x, y).0);
        if step > LIGHTEN_LEVELS {
            lighter += 1;
        } else if step < -LIGHTEN_LEVELS {
            darker += 1;
        }
    });
    (lighter, darker, patch)
}

/// [`ring_changes`] leaving out the pixels of `others`' bodies on screen (with a pixel's
/// margin), whose pictures animate.
fn ring_changes_outside(
    a: &image::RgbImage,
    b: &image::RgbImage,
    unit: UnitView,
    others: &[UnitView],
) -> usize {
    let [cx, cy] = unit.centre_px;
    let mut changed = 0;
    for_disc(a, unit, SHADOW_REACH_RADII, |x, y| {
        let at = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
        let on_other = others
            .iter()
            .any(|other| at.distance(Vec2::from_array(other.centre_px)) <= other.radius_px + 1.0);
        if !on_other
            && at.distance(Vec2::new(cx, cy)) > unit.radius_px * SHADOW_FROM_RADII
            && change(a.get_pixel(x, y).0, b.get_pixel(x, y).0) > CHANGE_LEVEL
        {
            changed += 1;
        }
    });
    changed
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

/// What a burrowed unit's looks change on the ground round it.
struct Mound {
    /// Pixels changed within 2.6 screen radii (the ring and a margin beyond it, short of the
    /// units nearby).
    changed: usize,
    /// Changed pixels on the ground beyond the mound's ring (2 px tolerance).
    outside: usize,
    /// The farthest changed pixel's distance on the ground, radii.
    farthest: f32,
    /// Share of the ground ring between [`MOUND_GROUND_RING`] radii, where it shows beside the
    /// body (not behind it on screen), that the looks change.
    ring_share: f64,
}

/// What `with` changes against `without` round `unit`: see [`Mound`].
fn mound_changes(
    without: &image::RgbImage,
    with: &image::RgbImage,
    camera: &CameraView,
    unit: UnitView,
) -> Mound {
    let ground = Vec3::from_array(unit.ground);
    let world_per_px = unit.radius / unit.radius_px.max(1.0);
    let ring = MOUND_RING_RADII * unit.radius + EDGE_PX * world_per_px;
    let [cx, cy] = unit.centre_px;
    let mut mound = Mound {
        changed: 0,
        outside: 0,
        farthest: 0.0,
        ring_share: 0.0,
    };
    let (mut ring_pixels, mut ring_changed) = (0_usize, 0_usize);
    for_disc(without, unit, 2.6, |x, y| {
        let changed = change(without.get_pixel(x, y).0, with.get_pixel(x, y).0) > CHANGE_LEVEL;
        let point = camera.unproject(x as f32, y as f32, ground.y);
        let distance = point
            .map(|point| Vec2::new(point.x - ground.x, point.z - ground.z).length() / unit.radius);
        let beside_body =
            Vec2::new(x as f32 + 0.5 - cx, y as f32 + 0.5 - cy).length() > unit.radius_px;
        if let Some(distance) = distance {
            if beside_body && (MOUND_GROUND_RING.0..=MOUND_GROUND_RING.1).contains(&distance) {
                ring_pixels += 1;
                ring_changed += usize::from(changed);
            }
        }
        if !changed {
            return;
        }
        mound.changed += 1;
        let Some(distance) = distance else {
            mound.outside += 1;
            return;
        };
        mound.farthest = mound.farthest.max(distance);
        if distance * unit.radius > ring {
            mound.outside += 1;
        }
    });
    mound.ring_share = ring_changed as f64 / ring_pixels.max(1) as f64;
    mound
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
        if !blend_of_neighbours(empty, x, y, after) {
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

/// Whether `colour` is, as a whole colour, near what a bent read of `reference` near `(x, y)`
/// can give. A filtered read blends the four pixels of one 2 by 2 block, so the colour is
/// accepted when it lies within [`SHIMMER_SLACK`] (Euclidean RGB distance) of the per-channel
/// box those four pixels span, for some block whose top-left pixel lies within
/// [`SHIMMER_REACH_PX`] of `(x, y)` (-6 to 5 either way; the block reaches one pixel further).
/// The box, not the four colours' convex hull, is tested: within one block a colour assembled
/// from channels of its different pixels still passes, which a 2 by 2 block's blend nearly
/// spans anyway. Across blocks it does not: unlike a box over the whole neighbourhood, this
/// refuses a colour assembled from channels of pixels in different blocks, such as a dark
/// neutral spot on multi-coloured ground (review 1's note on the per-channel check; review 2's
/// note on this comment).
fn blend_of_neighbours(reference: &image::RgbImage, x: u32, y: u32, colour: [u8; 3]) -> bool {
    let (width, height) = (i64::from(reference.width()), i64::from(reference.height()));
    for dy in -SHIMMER_REACH_PX..SHIMMER_REACH_PX {
        for dx in -SHIMMER_REACH_PX..SHIMMER_REACH_PX {
            let (left, top) = (i64::from(x) + dx, i64::from(y) + dy);
            if left < 0 || top < 0 || left + 1 >= width || top + 1 >= height {
                continue;
            }
            let (mut low, mut high) = ([255.0_f32; 3], [0.0_f32; 3]);
            for (ox, oy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let texel = reference.get_pixel((left + ox) as u32, (top + oy) as u32).0;
                for c in 0..3 {
                    low[c] = low[c].min(f32::from(texel[c]));
                    high[c] = high[c].max(f32::from(texel[c]));
                }
            }
            let distance = (0..3)
                .map(|c| {
                    let value = f32::from(colour[c]);
                    let outside = (low[c] - value).max(value - high[c]).max(0.0);
                    outside * outside
                })
                .sum::<f32>()
                .sqrt();
            if distance <= SHIMMER_SLACK {
                return true;
            }
        }
    }
    false
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

/// World units from `point` to the ground's edge (the first cell that is not walkable ground,
/// or the map's end) along the axes, in half-unit steps up to 8.
fn edge_room(world: &SimWorld, point: Vec2) -> f32 {
    let map = &world.map;
    let ground = |probe: Vec2| {
        map.world_to_cell(sim_position(probe))
            .ok()
            .and_then(|(cx, cy)| map.cell(cx, cy).ok())
            .is_some_and(|cell| cell.flags.contains(CellFlags::GROUND_PATHABLE))
    };
    [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y]
        .iter()
        .map(|direction| {
            (1..=16)
                .map(|step| step as f32 * 0.5)
                .take_while(|reach| ground(point + *direction * *reach))
                .last()
                .unwrap_or(0.0)
        })
        .fold(f32::INFINITY, f32::min)
}

/// Moves the spot farthest from the ground's edge to `index`: the shimmer staged there then
/// bends no ground edge, whose step from lit ground to the dark beyond would move its patch's
/// mean colour, which the shimmer check bounds.
fn roomiest_at(world: &SimWorld, spots: &mut [Vec2], index: usize) {
    let roomiest = (0..spots.len())
        .max_by(|a, b| edge_room(world, spots[*a]).total_cmp(&edge_room(world, spots[*b])))
        .expect("invariant: find_spots returned at least one spot");
    spots.swap(index, roomiest);
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
/// command, and that mode's name: the shipped burrowing unit itself once it has media (its own
/// burrowed-mode clip then plays in its dome, plan WP13), else [`burrower`]'s stand-in given the
/// shipped unit's mode-switch ability in the session's catalog. The shipped unit is armed in
/// its mode; nothing it could target is near the spots it is put on.
fn burrowing_unit(world: &mut SimWorld) -> (UnitTypeId, String) {
    let shipped = world.catalog.archetypes().find_map(|arch| {
        let mode = arch.modes.iter().find(|mode| mode.burrowed)?;
        presentable(&assets(), &arch.slug).then(|| (arch.unit_type_id, mode.mode_name.clone()))
    });
    if let Some(shipped) = shipped {
        return shipped;
    }
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

/// A presentable, unarmed unit type of `world` with a burrowed mode but no clip for it (no
/// mode media), and that mode's name: a mobile ground archetype nothing on the field uses,
/// given in the session's catalog a copy of the shipped burrowed mode without its weapon and no
/// weapons of its own (as `battle_harness::burrower` makes its stand-in). Only the test
/// world's catalog changes.
fn plain_burrower(world: &mut SimWorld) -> (UnitTypeId, String) {
    let mode = world
        .catalog
        .archetypes()
        .find_map(|arch| arch.modes.iter().find(|mode| mode.burrowed).cloned())
        .expect("the shipped catalog has a burrowing unit");
    let in_use = |world: &SimWorld, id: UnitTypeId| {
        world.entities.values().any(|unit| unit.unit_type_id == id)
    };
    let stand_in = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.max_speed > SimScalar::ZERO
                && arch.mover_type == MoverType::Ground
                && arch.modes.is_empty()
                && !in_use(world, arch.unit_type_id)
                && presentable(&assets(), &arch.slug)
        })
        .expect("the catalog ships a presentable ground unit nothing on the field uses")
        .unit_type_id;
    let archetype = world
        .catalog
        .get_mut(stand_in)
        .expect("invariant: the type was just found in this catalog");
    archetype.weapon = None;
    archetype.ground_weapon = None;
    archetype.air_weapon = None;
    let mut burrowed = mode.clone();
    burrowed.ground_weapon_override = None;
    archetype.modes.push(burrowed);
    (stand_in, mode.mode_name)
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
    capture_part(commands, path, None, done);
}

/// Requests a screenshot to `path`, cut to `crop` (`[left, top, width, height]` in window
/// pixels, clamped to the window) when given; `done` runs once it is written.
fn capture_part(
    commands: &mut Commands<'_, '_>,
    path: PathBuf,
    crop: Option<[u32; 4]>,
    done: impl Fn(&mut World) + Send + Sync + 'static,
) {
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<'_, ScreenshotCaptured>, mut commands: Commands<'_, '_>| {
            let mut image = trigger
                .event()
                .0
                .clone()
                .try_into_dynamic()
                .expect("a screenshot converts to an image")
                .to_rgb8();
            if let Some([left, top, width, height]) = crop {
                let left = left.min(image.width().saturating_sub(1));
                let top = top.min(image.height().saturating_sub(1));
                let width = width.min(image.width() - left);
                let height = height.min(image.height() - top);
                image = image::imageops::crop_imm(&image, left, top, width, height).to_image();
            }
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
    // Every frame advances presentation time by one 40 ms tick, whatever the frame took, so
    // the sequences are taken at a fixed step and every driver sees the same moments (review
    // 1, F-2).
    app.insert_resource(TimeUpdateStrategy::ManualDuration(FRAME_STEP))
        .insert_resource(idle)
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

/// Presentation time each frame of a child advances: one simulation tick.
const FRAME_STEP: Duration = Duration::from_millis(40);

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
    plain: Option<(EntityId, String)>,
    /// A neutral unit of the viewer's standing on the back of the siege switch's furrow.
    behind: Option<EntityId>,
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
            // A unit of the viewer's on the furrow's back half (away from the camera, which the
            // arc covers first), its body over the darkened band (review 3, E3-1).
            let worker = world
                .entities
                .values()
                .find(|unit| unit.owner == VIEWER && !unit.is_immobile && unit.id != id)
                .map(|unit| unit.unit_type_id)
                .expect("the viewer has a mobile unit");
            let behind = staging.spots[2] - Vec2::new(0.0, BEHIND_DISTANCE);
            staging.behind = Some(spawn_still(world, worker, VIEWER, behind));
            session.publish();
            switch_mode(session, id, &mode, ModeDirection::Enter);
            view.critical_only = true;
            view.hidden = Some(id);
        }
        Shot::PlainUpright => {
            let world = session.world_mut().expect("a ready session has a world");
            let (unit_type, mode) = plain_burrower(world);
            let id = spawn_still(world, unit_type, VIEWER, staging.spots[3]);
            world
                .entities
                .get_mut(&id)
                .expect("just spawned")
                .is_immobile = true;
            staging.plain = Some((id, mode));
        }
        Shot::PlainBurrowed => {
            let (id, mode) = staging
                .plain
                .clone()
                .expect("the plain upright shot spawned it");
            burrow(
                session.world_mut().expect("a ready session has a world"),
                id,
                &mode,
            );
        }
        Shot::BurrowedBare | Shot::ShimmerBare | Shot::PlainBurrowedBare => view.bare = true,
        Shot::SiegeMidBare => {
            view.bare = true;
            view.hidden = staging.siege;
        }
    }
}

/// Whether shot `index` may stop the clock: the siege switch must be half way.
fn pose_ready(staging: &PoseStaging, index: usize, bubbles: &ShellQuery<'_, '_>) -> bool {
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
    bubbles: &ShellQuery<'_, '_>,
    materials: &Assets<BubbleDomeMaterial>,
    registry: &FlipbookRegistry,
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
    let plain = staging.plain.as_ref().map(|(id, _)| *id);
    let (watched, slot) = match shot {
        Shot::Upright | Shot::Burrowed | Shot::BurrowedBare => (burrower, 0),
        Shot::Shimmer | Shot::ShimmerBare => (staging.enemy, 1),
        Shot::SiegeMid | Shot::SiegeMidBare => (staging.siege, 2),
        Shot::PlainUpright | Shot::PlainBurrowed | Shot::PlainBurrowedBare => (plain, 4),
        Shot::Empty => (None, 3),
    };
    if shot == Shot::Empty {
        // The viewer's mobile unit nearest the burrowing unit's spot (at least the spots'
        // clearance from it), whose shell stays neutral in every shot.
        let spot = staging.spots[0];
        let neutral = bubbles
            .iter()
            .filter(|(bubble, ..)| {
                bubble.0.owner == VIEWER
                    && bubble.0.concealment == Concealment::None
                    && bubble.0.stance.mode.is_none()
                    && !matches!(bubble.0.shape, ShapeKind::BuildingDome { .. })
            })
            .min_by(|(a, ..), (b, ..)| {
                let distance = |bubble: &BubbleInstance| {
                    Vec2::new(bubble.0.world_pos[0], bubble.0.world_pos[1]).distance(spot)
                };
                distance(a).total_cmp(&distance(b))
            });
        if let Some((bubble, ..)) = neutral {
            let view = unit_view(bubble, camera);
            staging.report.neutral = view;
            staging.report.neutral_neighbours = bubbles
                .iter()
                .filter(|(other, ..)| other.0.entity_id != bubble.0.entity_id)
                .map(|(other, ..)| unit_view(other, camera))
                .filter(|other| {
                    let [x, y] = other.centre_px;
                    let [cx, cy] = view.centre_px;
                    Vec2::new(x - cx, y - cy).length()
                        < view.radius_px * SHADOW_REACH_RADII + other.radius_px
                })
                .collect();
        }
    }
    let Some((bubble, material, track)) = find(watched) else {
        return;
    };
    let view = unit_view(bubble, camera);
    match slot {
        0 if shot == Shot::Upright => staging.report.burrower = view,
        0 if shot == Shot::BurrowedBare => {
            // The burrowed body's disc lies lower by its sink: measure it where it is drawn.
            let sink = materials
                .get(&material.0)
                .map_or(0.0, |material| material.shell.shape.x);
            let lowered = BubbleInstance(BubbleRenderInstance {
                world_pos: [
                    bubble.0.world_pos[0],
                    bubble.0.world_pos[1],
                    bubble.0.world_pos[2] - sink,
                ],
                ..bubble.0.clone()
            });
            staging.report.burrowed = unit_view(&lowered, camera);
            staging.report.burrower_has_clip = bubble
                .0
                .stance
                .mode
                .is_some_and(|mode| registry.get_mode(bubble.0.unit_type, mode).is_some());
        }
        4 if shot == Shot::PlainUpright => staging.report.plain = view,
        1 => staging.report.shimmer = view,
        2 => {
            staging.report.siege = view;
            if let Some((behind, ..)) = find(staging.behind) {
                staging.report.behind = unit_view(behind, camera);
            }
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
    bubbles: ShellQuery<'_, '_>,
    (materials, registry): (
        Res<'_, Assets<BubbleDomeMaterial>>,
        Res<'_, FlipbookRegistry>,
    ),
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
            staging.spots = find_spots(world, &fog, rts.target, 4);
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
                &materials,
                &registry,
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
    /// Samples while it burrows, one per 40 ms step.
    dig: Vec<Sample>,
    /// Samples while it comes up.
    emerge: Vec<Sample>,
    /// Samples while the anchoring unit sieges.
    siege: Vec<Sample>,
    /// Samples while it unsieges.
    unsiege: Vec<Sample>,
    /// The tableau's enemy cloaked unit, undetected, as the camera saw it.
    enemy: UnitView,
    /// The tableau's enemy burrowed unit, undetected, where it stands (review 1, F-1).
    burrowed_enemy: UnitView,
    /// The tableau's anchored unit of the viewer's.
    anchored: UnitView,
    /// The tableau's cloaked unit of the viewer's.
    own_cloaked: UnitView,
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

/// Seconds of presentation time an act runs after the switch was asked for: the dig's 1.8 s
/// and its settle, the emerge's channel and its lingering earth, and each siege channel (72 or
/// 63 ticks and up to 9 random ones) with the anchoring after it.
const ACT_SECONDS: [f64; 4] = [2.4, 1.4, 3.6, 3.2];
/// Width and height of a sequence picture, cut round the acting unit, in window pixels.
const SEQUENCE_CROP: [u32; 2] = [320, 240];

/// What the sequence child is doing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum SequencePhase {
    #[default]
    Boot,
    Booting {
        still: u32,
        height: f32,
    },
    /// Act `act` runs since `at`; `pictures` were taken, one per 40 ms step.
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
    /// The detected tableau is shown without its effect layers since `at` (review 1, C-1).
    DetectedBare {
        at: f64,
    },
    /// A picture is being written; its completion moves the staging on.
    Capturing,
}

/// Seconds the detected tableau runs before its picture: detection is published, the mark
/// comes in.
const DETECT_SECONDS: f64 = 1.5;
/// Seconds the tableau runs before its picture: the anchoring unit's switch (up to 81 ticks)
/// and its anchor's short dust have ended, so every look stands settled.
const TABLEAU_SECONDS: f64 = 5.0;
/// Levels of luminance a darkening look may raise a pixel by (filtering at its edges).
const LIGHTEN_LEVELS: f32 = 6.0;
/// Share of the anchored unit's patch its looks may lighten by more than [`LIGHTEN_LEVELS`].
const LIGHTEN_SHARE: f64 = 0.005;
/// Seconds a bare tableau is shown before its picture.
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
    /// The window rectangle the act's pictures are cut to.
    crop: Option<[u32; 4]>,
    /// Pictures requested and not yet written.
    outstanding: Arc<AtomicU32>,
    /// The acts this child plays: indices into [`ACTS`], from the first to before the end.
    acts: (usize, usize),
    /// Whether the tableau follows the acts.
    tableau: bool,
}

/// Environment variable naming the part of the sequences a child plays: `burrow` (dig and
/// emerge) or `siege` (siege, unsiege and the tableau). Each part is its own child, so each
/// stays inside the GPU harness's run limit with a picture every 40 ms step.
const PART_VAR: &str = "SPACE_SCUFFLE_STANCE_PART";
/// The parts, with their acts and whether the tableau follows.
const PARTS: [(&str, (usize, usize), bool); 2] =
    [("burrow", (0, 2), false), ("siege", (2, 4), true)];

/// The report file of part `part`.
fn part_report(part: &str) -> String {
    format!("report_{part}.ron")
}

/// Writes `report` as part `part`'s report into `directory`.
fn write_part_report(directory: &Path, part: &str, report: &SequenceReport) {
    let text = ron::to_string(report).expect("plain data");
    std::fs::write(directory.join(part_report(part)), text)
        .expect("the parent's directory is writable");
}

/// The name of the part a staging plays.
fn part_name(staging: &SequenceStaging) -> &'static str {
    PARTS
        .iter()
        .find(|(_, acts, _)| *acts == staging.acts)
        .map_or("burrow", |(name, ..)| *name)
}

/// Starts act `act`: spawns its unit on the first act of each and asks for the switch.
fn start_act(staging: &mut SequenceStaging, session: &mut BattleSession, act: Act) {
    staging.crop = None;
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
/// enemy cloaked unit the viewer does not detect (a detector comes beside it for the later
/// pictures, [`detect_in_tableau`]), an enemy burrowed unit it does not detect, and the
/// anchoring unit sieged again.
fn stage_tableau(world: &mut World) {
    world.resource_scope(|world, staging: Mut<'_, SequenceStaging>| {
        let mut session = world.resource_mut::<BattleSession>();
        let sim = session.world_mut().expect("a ready session has a world");
        let (digger, dig_mode) = if let Some((id, mode)) = &staging.burrower {
            (*id, mode.clone())
        } else {
            let (unit_type, mode) = burrowing_unit(sim);
            (spawn_still(sim, unit_type, VIEWER, staging.spots[0]), mode)
        };
        burrow(sim, digger, &dig_mode);
        let (stand_in, mode) = burrower(sim, &assets());
        let own = spawn_still(sim, stand_in, VIEWER, staging.spots[2]);
        sim.entities.get_mut(&own).expect("just spawned").is_cloaked = true;
        let hidden = spawn_still(sim, stand_in, ENEMY, staging.spots[3]);
        sim.entities
            .get_mut(&hidden)
            .expect("just spawned")
            .is_cloaked = true;
        let dug_in = spawn_still(sim, stand_in, ENEMY, staging.spots[4]);
        burrow(sim, dug_in, &mode);
        for unit in sim.entities.values_mut() {
            if unit.owner == ENEMY && unit.is_cloaked {
                unit.is_immobile = true;
            }
        }
        if let Some((id, mode)) = &staging.siege {
            let (id, mode) = (*id, mode.clone());
            switch_mode(&mut session, id, &mode, ModeDirection::Enter);
        }
    });
    world.resource_mut::<LabState>().battle_paused = false;
    let now = world.resource::<Time<Virtual>>().elapsed_secs_f64();
    world.resource_mut::<SequenceStaging>().phase = SequencePhase::Tableau { at: now };
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

/// Moves the staging to `phase` with the effect layers drawn (`bare` false) or not, at the
/// present time.
fn show_tableau(world: &mut World, bare: bool, phase: fn(f64) -> SequencePhase) {
    world.resource_mut::<EffectView>().bare = bare;
    let now = world.resource::<Time<Virtual>>().elapsed_secs_f64();
    world.resource_mut::<SequenceStaging>().phase = phase(now);
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

/// The window rectangle of [`SEQUENCE_CROP`] centred on `view`'s body.
fn crop_round(view: UnitView) -> [u32; 4] {
    let [width, height] = SEQUENCE_CROP;
    let [x, y] = view.centre_px;
    let left = (x - width as f32 * 0.5).max(0.0) as u32;
    let top = (y - height as f32 * 0.55).max(0.0) as u32;
    [left, top, width, height]
}

/// Requests a screenshot of this frame to `path`, cut to `crop`, without waiting for it: the
/// counter `outstanding` counts it until it is written. Every frame advances presentation time
/// by [`FRAME_STEP`], so a picture requested each frame lies exactly one step after the last,
/// however long the readback takes.
fn capture_frame(
    commands: &mut Commands<'_, '_>,
    path: PathBuf,
    [left, top, width, height]: [u32; 4],
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
            let left = left.min(image.width().saturating_sub(1));
            let top = top.min(image.height().saturating_sub(1));
            let width = width.min(image.width() - left);
            let height = height.min(image.height() - top);
            image::imageops::crop_imm(&image, left, top, width, height)
                .to_image()
                .save(&path)
                .expect("a screenshot must be written");
            outstanding.fetch_sub(1, Ordering::SeqCst);
        },
    );
}

/// One frame of act `act` that began at `at`: samples the acting unit and requests picture
/// `pictures` of this frame (one per 40 ms step, [`capture_frame`]). Once the act's time is up
/// and its pictures are written, starts the next act, ends the part or stages the tableau.
/// Returns the next phase.
fn act_step(
    commands: &mut Commands<'_, '_>,
    staging: &mut SequenceStaging,
    session: &mut BattleSession,
    now: f64,
    (bubbles, materials): (&ShellQuery<'_, '_>, &Assets<BubbleDomeMaterial>),
    camera: (&Camera, &GlobalTransform),
    (act, at, pictures): (usize, f64, u32),
) -> SequencePhase {
    if now - at >= ACT_SECONDS[act] {
        if staging.outstanding.load(Ordering::SeqCst) > 0 {
            return SequencePhase::Acting { act, at, pictures };
        }
        return end_act(commands, staging, session, now, act);
    }
    let since = (now - at) as f32;
    let unit = match ACTS[act] {
        Act::Dig | Act::Emerge => staging.burrower.as_ref().map(|(id, _)| *id),
        Act::Siege | Act::Unsiege => staging.siege.as_ref().map(|(id, _)| *id),
    };
    let found = unit.and_then(|id| bubbles.iter().find(|(bubble, ..)| bubble.0.entity_id == id));
    if let Some((bubble, ..)) = found {
        if staging.report.radius == 0.0 && matches!(ACTS[act], Act::Dig) {
            staging.report.radius = bubble.0.radius;
        }
        if staging.crop.is_none() {
            staging.crop = Some(crop_round(unit_view(bubble, camera)));
        }
    }
    if let Some(sampled) = unit.and_then(|id| sample(id, since, bubbles, materials)) {
        match ACTS[act] {
            Act::Dig => staging.report.dig.push(sampled),
            Act::Emerge => staging.report.emerge.push(sampled),
            Act::Siege => staging.report.siege.push(sampled),
            Act::Unsiege => staging.report.unsiege.push(sampled),
        }
    }
    let Some(crop) = staging.crop else {
        return SequencePhase::Acting { act, at, pictures };
    };
    // Named by its step since the switch was asked for, so a label t = step × 40 ms is exact
    // even when the unit's bubble appeared a frame or two after the request.
    let step = (f64::from(since) / FRAME_STEP.as_secs_f64()).round() as u32;
    let name = format!("seq_{:?}_{step:03}.png", ACTS[act]).to_lowercase();
    capture_frame(
        commands,
        staging.directory.join(name),
        crop,
        &staging.outstanding,
    );
    SequencePhase::Acting {
        act,
        at,
        pictures: pictures + 1,
    }
}

/// Ends act `act` at `now`: starts the part's next act, or ends the part, or (in the part
/// with the tableau) pictures the scene and then stages the tableau.
fn end_act(
    commands: &mut Commands<'_, '_>,
    staging: &mut SequenceStaging,
    session: &mut BattleSession,
    now: f64,
    act: usize,
) -> SequencePhase {
    if act + 1 < staging.acts.1 {
        start_act(staging, session, ACTS[act + 1]);
        commands.queue(rebuild_tables);
        return SequencePhase::Acting {
            act: act + 1,
            at: now,
            pictures: 0,
        };
    }
    if !staging.tableau {
        write_part_report(&staging.directory, part_name(staging), &staging.report);
        commands.queue(|world: &mut World| {
            world.send_event(AppExit::Success);
        });
        return SequencePhase::Capturing;
    }
    // The scene before the tableau, for the undetected burrowed enemy's check.
    capture(
        commands,
        staging.directory.join("seq_pre_tableau.png"),
        stage_tableau,
    );
    SequencePhase::Capturing
}

/// Records the tableau's undetected enemies as the camera sees them.
fn record_tableau(
    report: &mut SequenceReport,
    anchored: Option<EntityId>,
    fog: &BattleFog,
    bubbles: &ShellQuery<'_, '_>,
    camera: (&Camera, &GlobalTransform),
) {
    for (bubble, ..) in bubbles.iter() {
        if Some(bubble.0.entity_id) == anchored {
            report.anchored = unit_view(bubble, camera);
        }
        if bubble.0.owner == VIEWER && bubble.0.concealment == Concealment::Cloaked {
            report.own_cloaked = unit_view(bubble, camera);
        }
        if bubble.0.owner != ENEMY {
            continue;
        }
        let presence = fog.presence(&bubble.0);
        match (bubble.0.concealment, presence) {
            (Concealment::Cloaked, client::battle_pipeline::fog::Presence::Shimmer) => {
                report.enemy = unit_view(bubble, camera);
            }
            (Concealment::Burrowed, client::battle_pipeline::fog::Presence::Hidden) => {
                report.burrowed_enemy = unit_view(bubble, camera);
            }
            _ => {}
        }
    }
}

/// Advances the sequence staging by one frame.
#[expect(
    clippy::too_many_lines,
    reason = "one state machine; each arm is a step of the staging"
)]
fn stage_sequences(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, SequenceStaging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    time: Res<'_, Time<Virtual>>,
    fog: Option<Res<'_, BattleFog>>,
    bubbles: ShellQuery<'_, '_>,
    materials: Res<'_, Assets<BubbleDomeMaterial>>,
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
            staging.spots = find_spots(world, &fog, rts.target, 5);
            roomiest_at(world, &mut staging.spots, 3);
            hold_own_units(&mut session);
            let first = staging.acts.0;
            start_act(&mut staging, &mut session, ACTS[first]);
            commands.queue(rebuild_tables);
            lab.battle_paused = false;
            SequencePhase::Acting {
                act: first,
                at: now,
                pictures: 0,
            }
        }
        SequencePhase::Acting { act, at, pictures } => act_step(
            &mut commands,
            &mut staging,
            &mut session,
            now,
            (&bubbles, &materials),
            (camera, camera_transform),
            (act, at, pictures),
        ),
        SequencePhase::Tableau { at } if now - at < TABLEAU_SECONDS => {
            SequencePhase::Tableau { at }
        }
        SequencePhase::Tableau { .. } => {
            lab.battle_paused = true;
            let anchored = staging.siege.as_ref().map(|(id, _)| *id);
            record_tableau(
                &mut staging.report,
                anchored,
                &fog,
                &bubbles,
                (camera, camera_transform),
            );
            write_part_report(&staging.directory, part_name(&staging), &staging.report);
            capture(
                &mut commands,
                staging.directory.join("seq_tableau.png"),
                |world: &mut World| {
                    show_tableau(world, true, |at| SequencePhase::Bare { at });
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
                    show_tableau(world, true, |at| SequencePhase::DetectedBare { at });
                },
            );
            SequencePhase::Capturing
        }
        SequencePhase::DetectedBare { at } if now - at < BARE_SECONDS => {
            SequencePhase::DetectedBare { at }
        }
        SequencePhase::DetectedBare { .. } => {
            capture(
                &mut commands,
                staging.directory.join("seq_tableau_detected_bare.png"),
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
    let mut parts = Vec::new();
    for (part, ..) in PARTS {
        let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
        command.args([
            "--ignored",
            "--exact",
            "--nocapture",
            "stance_sequence_child",
        ]);
        command.env(CHILD_OUT_VAR, directory.path());
        command.env(PART_VAR, part);
        command.allow_for_hex_tiling_driver_defect();
        command.assert_passes_validation(&format!("stance sequences, {part}"));
        let report: SequenceReport = ron::from_str(&std::fs::read_to_string(
            directory.path().join(part_report(part)),
        )?)?;
        parts.push(report);
    }
    keep_evidence(directory.path(), "seq_")?;
    let [burrowing, sieging] =
        <[SequenceReport; 2]>::try_from(parts).map_err(|_| "one report per part")?;
    let report = SequenceReport {
        radius: burrowing.radius,
        dig: burrowing.dig,
        emerge: burrowing.emerge,
        ..sieging
    };
    // The battle tableau's undetected cloaked enemy: against the same frame without the effect
    // layers, its shimmer only bends what is there, with no colour, shadow or contact darkening
    // of its own (WP8 round 1 found a shadow-coloured spot at its feet: the distortion read the
    // scene copy at viewport-relative coordinates, i.e. from elsewhere on the screen).
    let enemy = report.enemy;
    assert!(
        enemy.radius_px > 0.0,
        "the tableau has an undetected cloaked enemy"
    );
    let picture = |name: &str| -> TestResult<image::RgbImage> {
        Ok(image::open(directory.path().join(name))?.to_rgb8())
    };
    let tableau = shimmer_check(
        &picture("seq_tableau_bare.png")?,
        &picture("seq_tableau.png")?,
        enemy,
    );
    eprintln!(
        "driver {}: battle tableau, undetected cloaked enemy: its shimmer bends {} pixels, {} of \
         them to a colour not near them without it, mean colour moves {:.2} levels",
        gpu_run::selected_driver().driver,
        tableau.bent,
        tableau.foreign,
        tableau.mean_shift
    );
    assert!(
        (tableau.foreign as f64) <= SHIMMER_FOREIGN_SHARE * tableau.bent.max(1) as f64,
        "the tableau's shimmer draws colours of its own: {} of {}",
        tableau.foreign,
        tableau.bent
    );
    assert!(
        tableau.mean_shift <= SHIMMER_MEAN_LEVELS,
        "the tableau's shimmer moves its patch's mean colour by {:.2} levels",
        tableau.mean_shift
    );
    let driver = gpu_run::selected_driver().driver;

    // The tableau's undetected burrowed enemy (review 1, F-1): where it stands, the tableau
    // equals the scene before it was staged (at most the WP3 tolerance of changed pixels).
    let dug_in = report.burrowed_enemy;
    assert!(
        dug_in.radius_px > 0.0,
        "the tableau has an undetected burrowed enemy"
    );
    let hidden_changes = disc_patch_changes(
        &picture("seq_pre_tableau.png")?,
        &picture("seq_tableau.png")?,
        dug_in,
        1.3,
    );
    eprintln!(
        "driver {driver}: battle tableau, undetected burrowed enemy: {hidden_changes} pixels \
         changed where it stands"
    );
    assert!(
        hidden_changes <= TOLERATED_PIXELS,
        "the undetected burrowed enemy changes {hidden_changes} pixels"
    );

    // The anchored unit's furrow, pressed soil and cracks darken the floor, its shadow
    // included, and lighten nothing (review 2, B2-2, B2-3): against the same frame without the
    // effect layers, within 2.2 radii.
    let anchored = report.anchored;
    assert!(anchored.radius_px > 0.0, "the tableau has an anchored unit");
    let (lightened, darkened, patch) = lightening(
        &picture("seq_tableau_bare.png")?,
        &picture("seq_tableau.png")?,
        anchored,
        2.2,
    );
    eprintln!(
        "driver {driver}: battle tableau, anchored unit: its looks darken {darkened} and lighten \
         {lightened} of {patch} pixels within 2.2 radii (by more than {LIGHTEN_LEVELS} levels)"
    );
    assert!(
        (lightened as f64) <= LIGHTEN_SHARE * patch as f64,
        "the anchored unit's looks lighten {lightened} of {patch} pixels"
    );
    assert!(darkened > 0, "the anchored unit's looks are drawn");
    let (falloff, rays) = outer_falloff(
        &picture("seq_tableau_bare.png")?,
        &picture("seq_tableau.png")?,
        anchored,
    );
    eprintln!(
        "driver {driver}: battle tableau, anchored unit: its looks' outer edge fades from 0.95 to \
         0.75 of the ground's light over {falloff:.1} pixels ({:.2} radii), median of {rays} \
         rays",
        falloff / anchored.radius_px
    );
    assert!(
        rays > 0,
        "the anchored look darkens the ground beside the hull"
    );
    assert!(
        falloff >= FALLOFF_RADII * anchored.radius_px,
        "the anchored look's edge is hard: {falloff:.1} pixels"
    );

    // The body's depth once dug in: whatever the unit's sink (shallow with its own clip, deep
    // without), the dig ends there.
    let full = report
        .dig
        .last()
        .map_or(0.0, |sample| sample.sink)
        .max(1e-3);

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
    let part = std::env::var(PART_VAR).expect("the parent names a part");
    let (_, acts, tableau) = PARTS
        .into_iter()
        .find(|(name, ..)| *name == part)
        .expect("a known part");
    run_child(
        SequenceStaging {
            directory: PathBuf::from(directory),
            acts,
            tableau,
            ..SequenceStaging::default()
        },
        stage_sequences,
    );
}
