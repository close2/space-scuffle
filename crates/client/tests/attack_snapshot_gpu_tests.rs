//! The attack snapshot on a real GPU: drawn, dimmed, and absent from the sandbox (plan
//! `2026-11-24-attacker-snapshot` P-17, P-18).
//!
//! # Responsibility
//! Proves the snapshot all the way to pixels on the real bubble shader and the shipped media.
//! The production client plays the shipped skirmish on a real window. Its child stages an enemy
//! soldier on a cliff that strikes a local soldier below it, so the local player keeps an attack
//! snapshot, then, with the clock stopped, takes three shots of the same view:
//! - `snapshot`: the image drawn by the snapshot pool;
//! - `without`: the same frame with the image's entity hidden;
//! - `live`: the attacker revealed to the viewer by its own attack (B-55), so its live bubble is
//!   drawn where the image stood and the image is dropped.
//!
//! The parent compares the square of window pixels round the image's projected centre (its
//! footprint): at least [`FOOTPRINT_SHARE`] of it changes from `without` to `snapshot` (P-17,
//! drawn), and its mean chroma in `snapshot` is at most [`DIMMED_SHARE`] of that in `live`
//! (P-17, dimmed by the look `effects.ron` authors; the share bounds the test, not the look). A second child runs the same fight under the
//! developer sandbox and reports that no snapshot entity and no pool exist there (P-18).
//!
//! # How it fits
//! The pixel test re-executes this test binary on the virtual GPU through `support/gpu_run.rs`
//! (charter §7, check C21), as `rally_gpu_tests.rs` does, so it runs on radv locally and on
//! lavapipe in CI. Staging writes the world directly (`support/attack_snapshot_stage.rs`); the
//! headless claims (one image, never picked, hidden when dropped) are in `battle_fog_tests.rs`.
//!
//! # Invariants verified
//! - The three shots differ only by the image and the attacker's reveal: the clock is stopped
//!   for the first two, and the third is taken after one more published tick.
//! - Setting `SPACE_SCUFFLE_ATTACK_SNAPSHOT_KEEP` to a directory makes the parent copy the shots
//!   there, for review; it changes nothing the test asserts.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::struct_excessive_bools
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/attack_snapshot_stage.rs"]
mod attack_snapshot_stage;
#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

use std::path::{Path, PathBuf};

use attack_snapshot_stage::{stage, SnapshotStage, LOCAL_PLAYER};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use client::battle_hud::attack_snapshots::{AttackSnapshotBubble, AttackSnapshotPool};
use client::battle_hud::model::HudModel;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::{ClientMode, LabState, RtsCamera};
use serde::{Deserialize, Serialize};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the directory the child writes to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_ATTACK_SNAPSHOT_CHILD_OUT";
/// Environment variable naming a directory the parent copies the shots to, for review.
const KEEP_VAR: &str = "SPACE_SCUFFLE_ATTACK_SNAPSHOT_KEEP";
/// Name of the libtest entry the pixel test launches.
const CHILD_TEST: &str = "attack_snapshot_child";
/// Name of the libtest entry the sandbox test launches.
const SANDBOX_CHILD_TEST: &str = "sandbox_snapshot_child";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// File the child writes its [`SnapshotReport`] to.
const REPORT_FILE: &str = "snapshot.ron";

/// Camera distance, in world units, that frames the fight from close by.
const CAMERA_DISTANCE: f32 = 16.0;
/// Rendered frames between a change and its capture.
const SETTLE_FRAMES: u32 = 3;
/// Frames the camera's look height must hold still before a shot.
const STILL_CAMERA_FRAMES: u32 = 3;
/// Ticks the sandbox child runs after the snapshot is recorded, looking for a snapshot entity.
const SANDBOX_TICKS: u32 = 40;
/// Share of the image's radius the footprint square spans each way from its centre.
const FOOTPRINT_HALF: f32 = 0.6;

/// A pixel counts as changed when one of its channels moves by more than this, in levels of 255.
const CHANGE_LEVEL: u8 = 12;
/// Share of the footprint the image must change. The parent prints the figures on every run.
const FOOTPRINT_SHARE: f64 = 0.5;
/// Largest share of the live unit's mean chroma the snapshot may keep. On 2026-11-24 the
/// shipped look kept 0.20 of it on radv (6.82 against 34.87) and 0.21 on lavapipe; the same
/// still image without the look kept 0.80 (26.19 against 32.54 on lavapipe), so this bound is
/// the look's own dimming, not the stillness of the image.
const DIMMED_SHARE: f64 = 0.5;

/// The three shots, in order.
const SHOTS: [&str; 3] = ["snapshot", "without", "live"];

/// What a child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SnapshotReport {
    /// The footprint in physical window pixels: `[left, top, right, bottom]`, right and bottom
    /// exclusive.
    footprint: [u32; 4],
    /// Whether the view listed the attacker's snapshot when the first shot was staged.
    listed: bool,
    /// Whether the view no longer listed it, and the attacker was revealed, for the last shot.
    revealed: bool,
    /// Snapshot entities that existed in the sandbox child (P-18).
    sandbox_entities: usize,
    /// Whether the sandbox child had a snapshot pool (P-18).
    sandbox_pool: bool,
    /// Whether the sandbox child's simulation had recorded the snapshot it did not draw.
    sandbox_recorded: bool,
}

/// Runs `child` on the virtual GPU and returns its report and directory.
fn run_child(child: &str, what: &str) -> TestResult<(SnapshotReport, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", child]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation(what);
    let report = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    Ok((report, directory))
}

/// P-17: the snapshot is drawn where its footprint projects, and dimmer than the same unit live.
#[test]
fn an_attack_snapshot_is_drawn_dimmed_on_the_real_shader() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let (report, directory) = run_child(CHILD_TEST, "attack snapshot")?;
    assert!(report.listed && report.revealed, "{report:?}");
    let shot = |name: &str| -> TestResult<image::RgbImage> {
        Ok(image::open(directory.path().join(format!("{name}.png")))?.to_rgb8())
    };
    if let Some(keep) = std::env::var_os(KEEP_VAR) {
        std::fs::create_dir_all(&keep)?;
        for name in SHOTS {
            let file = format!("{name}.png");
            std::fs::copy(directory.path().join(&file), Path::new(&keep).join(&file))?;
        }
    }
    let [snapshot, without, live] = [shot(SHOTS[0])?, shot(SHOTS[1])?, shot(SHOTS[2])?];
    let pixels = patch_pixels(report.footprint, &snapshot);
    assert!(pixels > 50, "the footprint is on screen: {report:?}");
    let changed = changed_pixels(&without, &snapshot, report.footprint);
    let (dimmed, lit) = (
        mean_chroma(&snapshot, report.footprint),
        mean_chroma(&live, report.footprint),
    );
    eprintln!(
        "driver {}: footprint {:?} of {pixels} pixels; the image changed {changed}; mean chroma \
         {dimmed:.2} as a snapshot, {lit:.2} live",
        gpu_run::selected_driver().driver,
        report.footprint,
    );
    assert!(
        changed as f64 >= FOOTPRINT_SHARE * pixels as f64,
        "the image changed only {changed} of {pixels} pixels of its footprint"
    );
    assert!(
        dimmed <= DIMMED_SHARE * lit,
        "the snapshot's mean chroma {dimmed:.2} is not at most {DIMMED_SHARE} of the live \
         unit's {lit:.2}"
    );
    Ok(())
}

/// P-18: under the developer sandbox the same fight records a snapshot in the simulation, and
/// nothing draws it: no snapshot entity, no pool.
#[test]
fn the_sandbox_draws_no_attack_snapshot() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let (report, _directory) = run_child(SANDBOX_CHILD_TEST, "sandbox attack snapshot")?;
    assert!(report.sandbox_recorded, "premise: {report:?}");
    assert_eq!(report.sandbox_entities, 0, "{report:?}");
    assert!(!report.sandbox_pool, "{report:?}");
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

/// Pixels of `patch` with a channel changed by more than [`CHANGE_LEVEL`] from `a` to `b`.
fn changed_pixels(a: &image::RgbImage, b: &image::RgbImage, patch: [u32; 4]) -> usize {
    let [left, top, right, bottom] = clamp_patch(patch, a);
    let mut changed = 0;
    for y in top..bottom {
        for x in left..right {
            let (p, q) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
            if (0..3).any(|c| p[c].abs_diff(q[c]) > CHANGE_LEVEL) {
                changed += 1;
            }
        }
    }
    changed
}

/// Mean over `patch` of each pixel's chroma, its largest channel minus its smallest, in levels.
fn mean_chroma(image: &image::RgbImage, patch: [u32; 4]) -> f64 {
    let [left, top, right, bottom] = clamp_patch(patch, image);
    let mut sum = 0_u64;
    for y in top..bottom {
        for x in left..right {
            let [r, g, b] = image.get_pixel(x, y).0;
            sum += u64::from(r.max(g).max(b) - r.min(g).min(b));
        }
    }
    sum as f64 / patch_pixels(patch, image).max(1) as f64
}

// ---------------------------------------------------------------------------------------------
// The children
// ---------------------------------------------------------------------------------------------

/// What a child is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Phase {
    /// The battle has not committed or published its second tick.
    #[default]
    Boot,
    /// The fight is staged and the clock runs until the view lists the snapshot.
    Striking,
    /// The clock is stopped; the camera's look height has held for `still` frames.
    Framing { still: u32, height: f32 },
    /// Shot `shot` is staged; `frames` rendered since.
    Settling { shot: usize, frames: u32 },
    /// Shot `shot` was requested and is being written.
    Capturing { shot: usize },
    /// The attacker was revealed while tick `since` was published; the clock runs until a later
    /// tick is.
    Revealing { since: u32 },
    /// The sandbox child counts frames after the simulation recorded the snapshot.
    Watching { until: u32 },
    /// Everything is written.
    Done,
}

/// The child's staging.
#[derive(Resource, Default)]
struct Staging {
    directory: PathBuf,
    phase: Phase,
    staged: Option<SnapshotStage>,
    report: SnapshotReport,
}

/// The latest tick the session published, or 0 before the first.
fn published_tick(session: &BattleSession) -> u32 {
    session.published().map_or(0, |published| published.tick)
}

/// The square of physical window pixels round the world point `centre`, `half` world units
/// each way along the camera's right.
fn patch(camera: (&Camera, &GlobalTransform), centre: Vec3, half: f32) -> [u32; 4] {
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
    let reach = to_window(centre + *transform.right() * half).distance(middle);
    let low = (middle - Vec2::splat(reach)).max(Vec2::ZERO);
    let high = middle + Vec2::splat(reach);
    [
        low.x as u32,
        low.y as u32,
        high.x.ceil() as u32,
        high.y.ceil() as u32,
    ]
}

/// The single pooled snapshot entity, its translation and scale, if drawn.
fn pooled_image(
    pool: Option<&AttackSnapshotPool>,
    images: &Query<'_, '_, (&GlobalTransform, &Visibility), With<AttackSnapshotBubble>>,
) -> Option<(Entity, Vec3, f32)> {
    let entity = pool?.entities().next()?;
    let (transform, visibility) = images.get(entity).ok()?;
    (*visibility != Visibility::Hidden).then(|| {
        let (scale, _, translation) = transform.to_scale_rotation_translation();
        (entity, translation, scale.x)
    })
}

/// Whether the local view lists `attacker`'s snapshot.
fn listed(model: &HudModel, attacker: sim_types::EntityId) -> bool {
    model
        .view
        .attack_snapshots
        .iter()
        .any(|entry| entry.entity == attacker)
}

/// Advances the pixel child by one frame: boot, stage the fight, run until the snapshot is
/// drawn, frame it, then take the three shots.
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
    model: Res<'_, HudModel>,
    pool: Option<Res<'_, AttackSnapshotPool>>,
    images: Query<'_, '_, (&GlobalTransform, &Visibility), With<AttackSnapshotBubble>>,
    mut cameras: Query<'_, '_, (&Camera, &GlobalTransform, &mut RtsCamera)>,
) {
    if !session.is_ready() {
        return;
    }
    let Ok((camera, camera_transform, mut rts)) = cameras.get_single_mut() else {
        return;
    };
    let tick = published_tick(&session);
    staging.phase = match staging.phase {
        Phase::Boot if tick < 2 => Phase::Boot,
        Phase::Boot => {
            let world = session.world_mut().expect("a ready session has a world");
            let staged = stage(world);
            rts.target = staged.high;
            rts.distance = CAMERA_DISTANCE;
            staging.staged = Some(staged);
            lab.battle_paused = false;
            Phase::Striking
        }
        Phase::Striking => {
            let attacker = staging.staged.expect("staged").attacker;
            if listed(&model, attacker) && pooled_image(pool.as_deref(), &images).is_some() {
                lab.battle_paused = true;
                Phase::Framing {
                    still: 0,
                    height: rts.look_at_y,
                }
            } else {
                Phase::Striking
            }
        }
        Phase::Framing { still, height } if still < STILL_CAMERA_FRAMES => Phase::Framing {
            still: if (rts.look_at_y - height).abs() < 1e-4 {
                still + 1
            } else {
                0
            },
            height: rts.look_at_y,
        },
        Phase::Framing { .. } => {
            let attacker = staging.staged.expect("staged").attacker;
            staging.report.listed = listed(&model, attacker);
            let (_, centre, radius) =
                pooled_image(pool.as_deref(), &images).expect("the image is drawn");
            staging.report.footprint =
                patch((camera, camera_transform), centre, radius * FOOTPRINT_HALF);
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
        Phase::Revealing { since } if tick <= since + 1 => Phase::Revealing { since },
        Phase::Revealing { .. } => {
            lab.battle_paused = true;
            let attacker = staging.staged.expect("staged").attacker;
            staging.report.revealed =
                !listed(&model, attacker) && model.view.attack_revealed.contains(&attacker);
            Phase::Settling { shot: 2, frames: 0 }
        }
        Phase::Watching { .. } | Phase::Done => staging.phase,
    };
}

/// Requests shot `shot`; the observer writes it and stages the next.
fn capture(commands: &mut Commands<'_, '_>, directory: &Path, shot: usize) {
    let path = directory.join(format!("{}.png", SHOTS[shot]));
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<'_, ScreenshotCaptured>,
              mut staging: ResMut<'_, Staging>,
              mut session: ResMut<'_, BattleSession>,
              mut lab: ResMut<'_, LabState>,
              pool: Option<Res<'_, AttackSnapshotPool>>,
              mut visibilities: Query<'_, '_, &mut Visibility, With<AttackSnapshotBubble>>,
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
            staging.phase = match shot {
                0 => {
                    // The same frame without the image.
                    for entity in pool.iter().flat_map(|pool| pool.entities()) {
                        if let Ok(mut visibility) = visibilities.get_mut(entity) {
                            *visibility = Visibility::Hidden;
                        }
                    }
                    Phase::Settling { shot: 1, frames: 0 }
                }
                1 => {
                    // The attacker's own attack reveals it: its live bubble where the image was.
                    let attacker = staging.staged.expect("staged").attacker;
                    let slot = LOCAL_PLAYER.slot().expect("the viewer is seated");
                    let since = published_tick(&session);
                    session
                        .world_mut()
                        .expect("a ready session has a world")
                        .entities
                        .get_mut(&attacker)
                        .expect("the attacker lives")
                        .attack_reveal_ticks[slot] = 1_000;
                    lab.battle_paused = false;
                    Phase::Revealing { since }
                }
                _ => {
                    write_report(&staging);
                    exit.send(AppExit::Success);
                    Phase::Done
                }
            };
        },
    );
}

/// Writes the report for the parent.
fn write_report(staging: &Staging) {
    let text = ron::to_string(&staging.report).expect("plain data");
    std::fs::write(staging.directory.join(REPORT_FILE), text)
        .expect("the parent's directory is writable");
}

/// Advances the sandbox child by one frame: boot, stage the fight, run until the simulation
/// records the snapshot for the local player, run [`SANDBOX_TICKS`] more, then report.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system with one declared parameter per dependency"
)]
fn watch_sandbox(
    mut staging: ResMut<'_, Staging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    pool: Option<Res<'_, AttackSnapshotPool>>,
    images: Query<'_, '_, (), With<AttackSnapshotBubble>>,
    mut view: Local<'_, sim_types::PlayerView>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if !session.is_ready() {
        return;
    }
    let tick = published_tick(&session);
    staging.phase = match staging.phase {
        Phase::Boot if tick < 2 => Phase::Boot,
        Phase::Boot => {
            let world = session.world_mut().expect("a ready session has a world");
            staging.staged = Some(stage(world));
            lab.battle_paused = false;
            Phase::Striking
        }
        Phase::Striking => {
            let world = session.world().expect("a ready session has a world");
            world.write_player_view(LOCAL_PLAYER, &mut view);
            if view.attack_snapshots.is_empty() {
                Phase::Striking
            } else {
                staging.report.sandbox_recorded = true;
                Phase::Watching {
                    until: tick + SANDBOX_TICKS,
                }
            }
        }
        Phase::Watching { until } if tick < until => {
            staging.report.sandbox_entities =
                staging.report.sandbox_entities.max(images.iter().count());
            Phase::Watching { until }
        }
        Phase::Watching { .. } => {
            staging.report.sandbox_pool = pool.is_some();
            write_report(&staging);
            exit.send(AppExit::Success);
            Phase::Done
        }
        phase => phase,
    };
}

/// Builds the child's client in `mode`, with `staging_system` in `Present`.
fn run_staged_child<M>(mode: ClientMode, staging_system: impl IntoSystemConfigs<M>) {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    let mut app = client::build_graphical_app_off_main_thread(
        mode,
        Some(CHILD_FRAME_LIMIT),
        SKIRMISH_SCENARIO.as_ref(),
    )
    .expect("the shipped assets must start the client");
    app.insert_resource(Staging {
        directory: Path::new(&directory).to_path_buf(),
        ..Staging::default()
    })
    .add_systems(Update, staging_system);
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// The child of [`an_attack_snapshot_is_drawn_dimmed_on_the_real_shader`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn attack_snapshot_child() {
    run_staged_child(ClientMode::Skirmish, stage_shots.in_set(BattleSet::Present));
}

/// The child of [`the_sandbox_draws_no_attack_snapshot`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn sandbox_snapshot_child() {
    run_staged_child(
        ClientMode::LiveBattle,
        watch_sandbox.in_set(BattleSet::Present),
    );
}
