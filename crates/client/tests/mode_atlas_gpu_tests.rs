//! Mode atlases in a real battle on a real GPU (plan `2026-11-09-effect-visualization` WP13).
//!
//! # Responsibility
//! Proves that a unit switching into a catalog mode shows its mode atlas all the way to pixels.
//! The production client plays the shipped skirmish on a real window; its child spawns one unit of
//! the viewer for every catalog mode that ships a mode atlas, on open ground the viewer sees, and
//! takes three shots:
//! - `default`: the units in their default stance;
//! - `morphing`: half way through the first channel after the viewer ordered every unit into its
//!   mode through the production command queue (`submit_mode_switch`);
//! - `settled`: every unit settled in its mode.
//!
//! The child records, per shot and unit, the stance its bubble shows (`FlipbookPhase::stance`) and
//! whether its material binds the mode atlas's texture; the parent checks them and that each
//! unit's screen patch changed between the default and the settled shot. With
//! `SPACE_SCUFFLE_MODE_EVIDENCE_DIR` set, the parent also writes the three shots and an enlarged
//! crop of each unit's patch there, the review evidence of WP13.
//!
//! # How it fits
//! The pixel test re-executes this test binary on the virtual GPU through `support/gpu_run.rs`
//! (charter §7, check C21), as `concealment_gpu_tests.rs` does. The units are chosen from the
//! catalog and the shipped textures, never by name.
//!
//! # Invariants verified
//! - Default stance: the default atlas; during the channel into a mode: that mode's atlas, its
//!   enter morph; in the mode: the mode atlas.
//! - Each world change is published by a tick before the clock stops for its shot.

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

use battle_harness::{assets, sim_position};

use asset_baker::mode_media::ModeAtlasId;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use client::battle_pipeline::commands::ModeDirection;
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::bubble_shader::{BubbleDomeMaterial, BubbleInstance};
use client::flipbook_clock::FlipbookPhase;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::scene_controller::LiveBattleBubble;
use client::{ClientMode, FlipbookRegistry, LabState, RtsCamera};
use serde::{Deserialize, Serialize};
use sim_core::SimWorld;
use sim_map::CellFlags;
use sim_types::{EntityId, PlayerId, UnitOrder, UnitTypeId};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the directory the child writes to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_MODE_ATLAS_CHILD_OUT";
/// Environment variable naming a directory for the review evidence (optional).
const EVIDENCE_VAR: &str = "SPACE_SCUFFLE_MODE_EVIDENCE_DIR";
/// Name of the libtest entry the pixel test launches.
const CHILD_TEST: &str = "mode_atlas_pose_child";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// File the child writes its [`PoseReport`] to.
const REPORT_FILE: &str = "pose.ron";

/// The viewer, the skirmish's first seat, owns the staged units.
const VIEWER: PlayerId = PlayerId(0);

/// Rendered frames between a published change and its capture.
const SETTLE_FRAMES: u32 = 3;
/// Frames the camera's look height must hold still before the first shot.
const STILL_CAMERA_FRAMES: u32 = 3;
/// Distances from the camera's target, in world units, at which the units are placed.
const SPOT_DISTANCES: [f32; 5] = [4.0, 5.0, 6.0, 7.0, 3.0];
/// Clearance around a spot, in world units, from any unit, structure or resource node.
const SPOT_CLEARANCE: f32 = 3.0;
/// A unit's patch is the square around its projected sphere, this many radii from its centre.
const PATCH_RADII: f32 = 1.3;
/// Enlargement of the evidence crops.
const CROP_SCALE: u32 = 3;

/// A pixel counts as changed when one of its channels moves by more than this, in levels of 255.
const CHANGE_LEVEL: u8 = 12;
/// Share of a unit's patch its mode atlas must change between the default and the settled shot.
const CHANGED_SHARE: f64 = 0.15;

/// The three shots, in order.
const SHOTS: [&str; 3] = ["default", "morphing", "settled"];

/// One staged unit as one shot saw it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct UnitShot {
    /// The stance its bubble showed (`FlipbookPhase::stance`, debug-formatted).
    stance: String,
    /// Whether its material bound the mode atlas's texture.
    mode_texture: bool,
}

/// What the child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PoseReport {
    /// `<slug>-<mode>` of each staged unit.
    atlases: Vec<String>,
    /// Each unit's patch in physical window pixels, `[left, top, right, bottom]` (exclusive).
    patches: Vec<[u32; 4]>,
    /// Per shot, per unit.
    shots: Vec<Vec<UnitShot>>,
}

#[test]
fn units_show_their_mode_atlas_while_they_morph_and_once_they_settle() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("mode atlas pose");

    let report: PoseReport = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    assert!(!report.atlases.is_empty(), "{report:?}");
    assert_eq!(report.shots.len(), SHOTS.len(), "{report:?}");
    let [default, morphing, settled] = [&report.shots[0], &report.shots[1], &report.shots[2]];
    for (index, atlas) in report.atlases.iter().enumerate() {
        assert_eq!(default[index].stance, "Default", "{atlas}: {report:?}");
        assert!(!default[index].mode_texture, "{atlas}: {report:?}");
        assert!(
            morphing[index].stance.starts_with("Enter")
                || morphing[index].stance.starts_with("Mode"),
            "{atlas}: {report:?}"
        );
        assert!(morphing[index].mode_texture, "{atlas}: {report:?}");
        assert!(
            settled[index].stance.starts_with("Mode"),
            "{atlas}: {report:?}"
        );
        assert!(settled[index].mode_texture, "{atlas}: {report:?}");
    }
    assert!(
        morphing.iter().any(|unit| unit.stance.starts_with("Enter")),
        "the morphing shot caught a morph: {report:?}"
    );

    let shot = |name: &str| -> TestResult<image::RgbImage> {
        Ok(image::open(directory.path().join(format!("{name}.png")))?.to_rgb8())
    };
    let images = [shot(SHOTS[0])?, shot(SHOTS[1])?, shot(SHOTS[2])?];
    for (atlas, patch) in report.atlases.iter().zip(&report.patches) {
        let pixels = patch_pixels(*patch, &images[0]);
        assert!(pixels > 100, "{atlas}: the patch is on screen: {report:?}");
        let changed = changed_pixels(&images[0], &images[2], *patch);
        eprintln!(
            "driver {}: {atlas} patch {patch:?} of {pixels} pixels; default vs settled: {changed} changed",
            gpu_run::selected_driver().driver,
        );
        assert!(
            changed as f64 >= CHANGED_SHARE * pixels as f64,
            "{atlas}: settling in the mode changed only {changed} of {pixels} pixels"
        );
    }
    if let Some(evidence) = std::env::var_os(EVIDENCE_VAR) {
        write_evidence(Path::new(&evidence), &report, &images)?;
    }
    Ok(())
}

/// Writes the shots and an enlarged crop of every unit's patch in every shot to `directory`.
fn write_evidence(
    directory: &Path,
    report: &PoseReport,
    images: &[image::RgbImage; 3],
) -> TestResult {
    std::fs::create_dir_all(directory)?;
    for (name, image) in SHOTS.iter().zip(images) {
        image.save(directory.join(format!("battle_{name}.png")))?;
    }
    for (atlas, patch) in report.atlases.iter().zip(&report.patches) {
        let [left, top, right, bottom] = clamp_patch(*patch, &images[0]);
        let (width, height) = (right - left, bottom - top);
        let mut strip = image::RgbImage::new(width * CROP_SCALE * 3, height * CROP_SCALE);
        for (column, image) in images.iter().enumerate() {
            let crop = image::imageops::crop_imm(image, left, top, width, height).to_image();
            let large = image::imageops::resize(
                &crop,
                width * CROP_SCALE,
                height * CROP_SCALE,
                image::imageops::FilterType::Nearest,
            );
            image::imageops::replace(
                &mut strip,
                &large,
                i64::from(column as u32 * width * CROP_SCALE),
                0,
            );
        }
        strip.save(directory.join(format!("battle_{atlas}.png")))?;
    }
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

/// Pixels of `patch` with a channel changed by more than [`CHANGE_LEVEL`] between `a` and `b`.
fn changed_pixels(a: &image::RgbImage, b: &image::RgbImage, patch: [u32; 4]) -> usize {
    let [left, top, right, bottom] = clamp_patch(patch, a);
    (top..bottom)
        .flat_map(|y| (left..right).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let (p, q) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
            (0..3).any(|c| p[c].abs_diff(q[c]) > CHANGE_LEVEL)
        })
        .count()
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
    /// until the shot's condition holds on a later published tick.
    Publishing { shot: usize, since: u32 },
    /// The condition holds and the clock stopped; `frames` have been rendered since.
    Settling { shot: usize, frames: u32 },
    /// Shot `shot` was requested and is being written.
    Capturing { shot: usize },
    /// Every shot is written.
    Done,
}

/// One unit the child stages: its type, its mode and the mode's index.
#[derive(Debug, Clone)]
struct Staged {
    unit_type: UnitTypeId,
    mode_name: String,
    mode_index: u8,
    atlas: String,
    id: Option<EntityId>,
}

/// The child's staging.
#[derive(Resource, Default)]
struct Staging {
    directory: PathBuf,
    phase: Phase,
    units: Vec<Staged>,
    report: PoseReport,
}

/// The latest tick the session published, or 0 before the first.
fn published_tick(session: &BattleSession) -> u32 {
    session.published().map_or(0, |published| published.tick)
}

/// One unit for every catalog mode whose atlas ships.
fn staged_units(world: &SimWorld) -> Vec<Staged> {
    let textures = assets().join("textures");
    world
        .catalog
        .archetypes()
        .flat_map(|arch| {
            arch.modes.iter().enumerate().filter_map(|(index, mode)| {
                let atlas = ModeAtlasId {
                    slug: &arch.slug,
                    stance: &mode.mode_name,
                }
                .atlas_name();
                textures
                    .join(format!("{atlas}.flipbook.ron"))
                    .is_file()
                    .then(|| Staged {
                        unit_type: arch.unit_type_id,
                        mode_name: mode.mode_name.clone(),
                        mode_index: u8::try_from(index).expect("few modes"),
                        atlas,
                        id: None,
                    })
            })
        })
        .collect()
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
            at.distance(point) > SPOT_CLEARANCE
        });
        let nodes = world.map.resources().iter().all(|node| {
            let at = Vec2::new(node.pos.x.to_f32(), node.pos.y.to_f32());
            at.distance(point) > SPOT_CLEARANCE
        });
        units && nodes
    };
    SPOT_DISTANCES
        .iter()
        .flat_map(|distance| {
            (0..16).map(move |step| {
                around + Vec2::from_angle(step as f32 * std::f32::consts::TAU / 16.0) * *distance
            })
        })
        .find(|point| open(world, *point) && clear(*point) && fog.sees(point.x, point.y))
        .expect("the start area has open ground in sight")
}

/// Spawns the staged units and holds every other unit of the viewer in place.
fn stage_units(staging: &mut Staging, session: &mut BattleSession, fog: &BattleFog, around: Vec2) {
    let world = session.world_mut().expect("a ready session has a world");
    staging.units = staged_units(world);
    for unit in &mut staging.units {
        let spot = find_spot(world, fog, around);
        unit.id = Some(
            world
                .spawn_unit(unit.unit_type, VIEWER, sim_position(spot))
                .expect("open ground"),
        );
    }
    staging.report.atlases = staging
        .units
        .iter()
        .map(|unit| unit.atlas.clone())
        .collect();
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

/// Orders every staged unit into its mode through the production command queue.
fn order_modes(staging: &Staging, session: &mut BattleSession) {
    for unit in &staging.units {
        let id = unit.id.expect("staged");
        let (world, queue) = session.world_and_commands();
        queue
            .submit_mode_switch(
                world.expect("a ready session has a world"),
                VIEWER,
                &[id],
                &unit.mode_name,
                ModeDirection::Enter,
                false,
            )
            .expect("the client admits the mode switch for a fresh unit of the viewer");
    }
}

/// Whether shot `shot`'s condition holds in the published world.
fn ready(staging: &Staging, bubbles: &[(EntityId, sim_types::StanceView)], shot: usize) -> bool {
    let stance = |id: EntityId| {
        bubbles
            .iter()
            .find(|(bubble, _)| *bubble == id)
            .map(|(_, stance)| *stance)
    };
    let staged = staging.units.iter().filter_map(|unit| stance(unit.id?));
    match shot {
        0 => staging
            .units
            .iter()
            .all(|unit| unit.id.and_then(stance).is_some()),
        1 => staged.clone().any(|stance| {
            stance.transition.is_some_and(|transition| {
                transition.remaining_ticks <= transition.nominal_ticks / 2
            })
        }),
        _ => {
            let all: Vec<_> = staged.collect();
            all.len() == staging.units.len()
                && all
                    .iter()
                    .all(|stance| stance.mode.is_some() && stance.transition.is_none())
        }
    }
}

/// Records each staged unit's shown stance and texture, and its patch on the first shot.
fn record_shot(
    staging: &mut Staging,
    bubbles: &Query<
        '_,
        '_,
        (
            &BubbleInstance,
            &GlobalTransform,
            &FlipbookPhase,
            &MeshMaterial3d<BubbleDomeMaterial>,
        ),
        With<LiveBattleBubble>,
    >,
    materials: &Assets<BubbleDomeMaterial>,
    registry: &FlipbookRegistry,
    camera: (&Camera, &GlobalTransform),
) {
    let mut shot = Vec::new();
    let mut patches = Vec::new();
    for unit in &staging.units {
        let id = unit.id.expect("staged");
        let (bubble, transform, phase, material) = bubbles
            .iter()
            .find(|(bubble, ..)| bubble.0.entity_id == id)
            .expect("the unit has a bubble");
        let texture = materials
            .get(&material.0)
            .and_then(|material| material.video_texture.clone());
        let mode_texture = registry
            .get_mode(unit.unit_type, unit.mode_index)
            .is_some_and(|atlas| texture.as_ref() == Some(&atlas.texture));
        shot.push(UnitShot {
            stance: format!("{:?}", phase.stance),
            mode_texture,
        });
        patches.push(patch(camera, transform.translation(), bubble.0.radius));
    }
    if staging.report.shots.is_empty() {
        staging.report.patches = patches;
    }
    staging.report.shots.push(shot);
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

/// Advances the staging by one frame: boot, then for each shot change the world, run the clock
/// until the shot's condition is published, stop it, settle and capture.
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
    registry: Res<'_, FlipbookRegistry>,
    materials: Res<'_, Assets<BubbleDomeMaterial>>,
    bubbles: Query<
        '_,
        '_,
        (
            &BubbleInstance,
            &GlobalTransform,
            &FlipbookPhase,
            &MeshMaterial3d<BubbleDomeMaterial>,
        ),
        With<LiveBattleBubble>,
    >,
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
    let stances: Vec<(EntityId, sim_types::StanceView)> = bubbles
        .iter()
        .map(|(bubble, ..)| (bubble.0.entity_id, bubble.0.stance))
        .collect();
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
            stage_units(&mut staging, &mut session, &fog, rts.target);
            lab.battle_paused = false;
            Phase::Publishing {
                shot: 0,
                since: tick,
            }
        }
        Phase::Publishing { shot, since } if tick <= since || !ready(&staging, &stances, shot) => {
            Phase::Publishing { shot, since }
        }
        Phase::Publishing { shot, .. } => {
            lab.battle_paused = true;
            Phase::Settling { shot, frames: 0 }
        }
        Phase::Settling { shot, frames } if frames < SETTLE_FRAMES => Phase::Settling {
            shot,
            frames: frames + 1,
        },
        Phase::Settling { shot, .. } => {
            record_shot(
                &mut staging,
                &bubbles,
                &materials,
                &registry,
                (camera, camera_transform),
            );
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
            if next == 1 {
                order_modes(&staging, &mut session);
            }
            lab.battle_paused = false;
            staging.phase = Phase::Publishing { shot: next, since };
        },
    );
}

/// The child of [`units_show_their_mode_atlas_while_they_morph_and_once_they_settle`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn mode_atlas_pose_child() {
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
