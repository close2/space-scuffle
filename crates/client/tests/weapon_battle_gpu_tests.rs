//! Weapon looks in a staged battle on a real GPU (plan `2026-11-09-effect-visualization` WP11,
//! review 1, V1-1).
//!
//! # Responsibility
//! Proves the WP11 weapon looks where a player sees them: in the production client playing the
//! shipped skirmish, on the shipped floor, at the battle camera's default zoom, fired by real
//! units at real bubbles. Each child stages one wave of shooters, each across open ground from
//! an unarmed enemy target, and pictures every subject every 40 ms frame for
//! [`WAVE_SECONDS`]:
//! - **The first waves** stage one presentable unit per *family*: a distinct pair of `travel`
//!   and `impact` looks named by a presentable unit's block for its ground weapon, as many a
//!   wave as the child finds spots for. Each fires its own weapon with its own looks.
//! - **The later waves** stage the *lab families*, looks named only by the blocks of units with no
//!   media (which the client cannot present): each is given, in the test world's catalog only,
//!   to a presentable shooter whose weapon publishes the same kind of event (visual kind and
//!   whether it flies, or failing a shooter of that visual kind, whether it flies), so the look
//!   is shown at 1:1 next to a real bubble.
//!
//! The parent checks that each staged family's looks were spawned while its wave ran and that
//! its picture changed, prints per family what it saw, and with
//! `SPACE_SCUFFLE_WEAPON_BATTLE_EVIDENCE_DIR` set writes a sheet per family: a 40 ms sequence
//! at 1:1 and the same frames at 2x.
//!
//! # How it fits
//! Each child re-executes this test binary through `support/gpu_run.rs` (charter §7, C21), as
//! `status_gpu_tests.rs` does. Every unit and look is chosen by catalog properties (media, a
//! weapon, a block's looks), never by name (C5).
//!
//! # Invariants verified
//! - The waves are planned from the catalog alone, so parent and child agree on them.
//! - Every unit staged stands at full health throughout, so nothing dies in the pictures.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
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
use client::effects::catalog::{EffectCatalog, UnitEffectTables};
use client::effects::lab::driver::track_render_idle;
use client::effects::lab::LabRenderIdle;
use client::effects::runtime::EffectRuntime;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::{ClientMode, LabState, RtsCamera};
use serde::{Deserialize, Serialize};
use sim_core::SimWorld;
use sim_map::CellFlags;
use sim_types::{
    EntityId, MoverType, PlayerId, SimScalar, UnitOrder, UnitTypeId, WeaponSlot, WeaponSpec,
    WeaponVisualKind,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the directory a child writes to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_WEAPON_BATTLE_CHILD_OUT";
/// Environment variable carrying the child's wave.
const WAVE_VAR: &str = "SPACE_SCUFFLE_WEAPON_BATTLE_WAVE";
/// Environment variable naming a directory the parent writes its sheets to.
const EVIDENCE_VAR: &str = "SPACE_SCUFFLE_WEAPON_BATTLE_EVIDENCE_DIR";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// File a child writes its report to.
const REPORT_FILE: &str = "report.ron";

/// The viewer: the skirmish's first seat.
const VIEWER: PlayerId = PlayerId(0);
/// The other side.
const ENEMY: PlayerId = PlayerId(1);

/// Frames the camera's look height must hold still before the staging.
const STILL_CAMERA_FRAMES: u32 = 3;
/// Frames the renderer is given to settle before the subjects are staged.
const FRAMING_FRAMES: u32 = 12;
/// Render frames in a row with no pipeline waiting before the staging.
const IDLE_FRAMES: u32 = 8;
/// Presentation time each frame of a child advances: one simulation tick.
const FRAME_STEP: Duration = Duration::from_millis(40);
/// Seconds each wave is pictured for.
const WAVE_SECONDS: f64 = 2.0;
/// Most shooters a wave stages.
const MAX_SUBJECTS: usize = 9;
/// Distances from the camera's target, in world units, at which the spots are sought.
const SPOT_DISTANCES: [f32; 14] = [
    3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 3.5, 4.5, 5.5, 6.5, 7.5, 2.5, 10.0,
];
/// Clearance around a spot, in world units, from any unit, structure or resource node (the
/// green crystal props stand on nodes; review 1, P1-3).
const SPOT_CLEARANCE: f32 = 3.0;
/// Distance between two subjects, world units.
const SUBJECT_APART: f32 = 3.2;
/// Window pixels a subject's spot keeps from the window's sides and top.
const SCREEN_MARGIN: f32 = 150.0;
/// Share of the window's height above which a subject's spot lies (the HUD is below).
const SCREEN_BOTTOM_SHARE: f32 = 0.7;
/// Size of a subject's picture, window pixels.
const CROP: [u32; 2] = [300, 180];
/// Longest gap between a shooter and its target, world units, so both fit one picture.
const MOST_GAP: f32 = 3.0;
/// A pixel counts as changed when one of its channels moves by at least this many levels.
const SEEN_LEVEL: u8 = 32;
/// Pixels of a subject's picture its shots must change at some frame against its first.
const SEEN_PIXELS: usize = 30;
/// Frames of a family's sheet at 1:1 and, of those, at 2x.
const SHEET_FRAMES: (usize, usize) = (12, 6);

/// A family of looks: a `travel` and an `impact` look a block names for a weapon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Family {
    travel: Option<String>,
    impact: Option<String>,
    /// The visual kind of the weapon the block names them for.
    kind: WeaponVisualKind,
    /// Whether that weapon's shots fly (a projectile speed).
    flies: bool,
}

impl Family {
    fn label(&self) -> String {
        format!(
            "{}+{}",
            self.travel.as_deref().unwrap_or("none"),
            self.impact.as_deref().unwrap_or("none")
        )
    }
}

/// The weapon of `arch` in `slot`.
fn slot_weapon(arch: &sim_core::UnitArchetype, slot: WeaponSlot) -> Option<&WeaponSpec> {
    match slot {
        WeaponSlot::Primary => arch.weapon.as_ref(),
        WeaponSlot::Ground => arch.ground_weapon.as_ref(),
        WeaponSlot::Air => arch.air_weapon.as_ref(),
    }
}

/// A shooter: a presentable mobile unit type and the slot of the ground weapon it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Shooter {
    unit_type: UnitTypeId,
    slot: WeaponSlot,
}

/// A planned subject: a shooter, the family it fires, and whether that family is its own.
type Planned = (Shooter, Family, bool);

/// The waves of at most `capacity` subjects, planned from `world`'s catalog alone: first each
/// presentable family on its own shooter, then each lab family lent to a native shooter whose
/// weapon publishes the same kind of event (the same visual kind and flight, or failing that
/// the same flight; a family with a travel look only to a shooter whose own family has one, so
/// a melee shooter never carries a shot), each shooter at most once a wave. Returns the waves and the lab families
/// no shooter could carry.
fn plan(world: &SimWorld, capacity: usize) -> (Vec<Vec<Planned>>, Vec<Family>) {
    let assets = assets();
    let mut native: Vec<(Shooter, Family)> = Vec::new();
    let mut lab: Vec<Family> = Vec::new();
    for arch in world.catalog.archetypes() {
        let shows = presentable(&assets, &arch.slug);
        for entry in &arch.effects.weapons {
            if entry.mode.is_some()
                || entry.weapon == WeaponSlot::Air
                || (entry.travel.is_none() && entry.impact.is_none())
            {
                continue;
            }
            let Some(spec) = slot_weapon(arch, entry.weapon) else {
                continue;
            };
            let family = Family {
                travel: entry.travel.clone(),
                impact: entry.impact.clone(),
                kind: spec.visual_kind,
                flies: spec.projectile_speed.is_some(),
            };
            let known = |list: &[Family]| list.iter().any(|other| other.label() == family.label());
            let natives: Vec<Family> = native.iter().map(|(_, family)| family.clone()).collect();
            if shows
                && arch.max_speed > SimScalar::ZERO
                && !known(&natives)
                && native
                    .iter()
                    .all(|(shooter, _)| shooter.unit_type != arch.unit_type_id)
            {
                native.push((
                    Shooter {
                        unit_type: arch.unit_type_id,
                        slot: entry.weapon,
                    },
                    family,
                ));
            } else if !shows && !known(&natives) && !known(&lab) {
                lab.push(family);
            }
        }
    }
    // A lab family shown natively (a later presentable unit names it) is not lent.
    let natives: Vec<String> = native.iter().map(|(_, family)| family.label()).collect();
    lab.retain(|family| !natives.contains(&family.label()));
    let capacity = capacity.max(1);
    let mut waves: Vec<Vec<Planned>> = native
        .chunks(capacity)
        .map(|chunk| {
            chunk
                .iter()
                .map(|(shooter, family)| (*shooter, family.clone(), true))
                .collect()
        })
        .collect();
    let first_lab = waves.len();
    let mut unstaged = Vec::new();
    for family in lab {
        let exact: Vec<Shooter> = native
            .iter()
            .filter(|(_, own)| own.kind == family.kind && own.flies == family.flies)
            .filter(|(_, own)| own.travel.is_some() == family.travel.is_some())
            .map(|(shooter, _)| *shooter)
            .collect();
        let matching = if exact.is_empty() {
            native
                .iter()
                .filter(|(_, own)| own.flies == family.flies)
                .filter(|(_, own)| own.travel.is_some() == family.travel.is_some())
                .map(|(shooter, _)| *shooter)
                .collect()
        } else {
            exact
        };
        if matching.is_empty() {
            unstaged.push(family);
            continue;
        }
        let free = |wave: &Vec<Planned>| {
            wave.len() < capacity
                && matching
                    .iter()
                    .any(|shooter| wave.iter().all(|(used, ..)| used != shooter))
        };
        let index = if let Some(index) = waves.iter().skip(first_lab).position(free) {
            index + first_lab
        } else {
            waves.push(Vec::new());
            waves.len() - 1
        };
        let wave = &mut waves[index];
        let shooter = *matching
            .iter()
            .find(|shooter| wave.iter().all(|(used, ..)| used != *shooter))
            .expect("invariant: the wave was chosen for a free matching shooter");
        wave.push((shooter, family, false));
    }
    (waves, unstaged)
}

/// One staged subject of a wave.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SubjectReport {
    family: String,
    shooter: String,
    /// Whether the shooter fires its own looks or lent ones.
    own: bool,
    /// Spawns of the travel and the impact look while the wave ran.
    travel_spawned: u32,
    impact_spawned: u32,
    /// The subject's pictures, one per frame.
    frames: Vec<String>,
}

/// What a child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct WaveReport {
    subjects: Vec<SubjectReport>,
    /// Families no presentable shooter could carry (no shooter of their event kind).
    unstaged: Vec<String>,
    waves: usize,
}

/// What the child is doing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Phase {
    #[default]
    Boot,
    Booting {
        still: u32,
        height: f32,
    },
    Framing {
        frames: u32,
    },
    Running {
        at: f64,
        frame: u32,
    },
    Finishing,
}

/// The child's staging.
#[derive(Resource, Default)]
struct Staging {
    directory: PathBuf,
    wave: usize,
    phase: Phase,
    /// Each subject: its shooter, its target, its centre (world `x, y`), and its looks' ids
    /// with their spawn counts at the start.
    subjects: Vec<StagedSubject>,
    outstanding: Arc<AtomicU32>,
    report: WaveReport,
}

/// A staged subject.
struct StagedSubject {
    units: [EntityId; 2],
    centre: Vec2,
    looks: [Option<(client::effects::catalog::EffectId, u32)>; 2],
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

/// Up to `count` spots of open ground in the viewer's sight near `around`, clear of every unit,
/// node and other spot, on screen.
fn find_spots(
    world: &SimWorld,
    fog: &BattleFog,
    around: Vec2,
    count: usize,
    on_screen: impl Fn(Vec2) -> bool,
) -> Vec<Vec2> {
    let mut spots: Vec<Vec2> = Vec::new();
    for distance in SPOT_DISTANCES {
        for step in 0..24 {
            if spots.len() == count {
                return spots;
            }
            let point =
                around + Vec2::from_angle(step as f32 * std::f32::consts::TAU / 24.0) * distance;
            let clear_of = |at: Vec2| at.distance(point) > SPOT_CLEARANCE;
            // Clear of every unit's and structure's edge, not only its centre.
            let units = world.entities.values().all(|unit| {
                let radius = world
                    .catalog
                    .get(unit.unit_type_id)
                    .map_or(0.0, |arch| arch.collision_radius.to_f32());
                Vec2::new(unit.position.x.to_f32(), unit.position.y.to_f32()).distance(point)
                    > SPOT_CLEARANCE + radius
            });
            let nodes = world
                .map
                .resources()
                .iter()
                .all(|node| clear_of(Vec2::new(node.pos.x.to_f32(), node.pos.y.to_f32())));
            let apart = spots
                .iter()
                .all(|spot| spot.distance(point) > SUBJECT_APART);
            let ends = [-MOST_GAP * 0.5, MOST_GAP * 0.5]
                .iter()
                .all(|dx| on_screen(point + Vec2::new(*dx, 0.0)));
            // Open ground under both ends of the widest pair, so no picture runs off the map.
            let sides = [-MOST_GAP * 0.5, MOST_GAP * 0.5]
                .iter()
                .all(|dx| open(world, point + Vec2::new(*dx, 0.0)));
            if open(world, point)
                && sides
                && units
                && nodes
                && apart
                && fog.sees(point.x, point.y)
                && ends
            {
                spots.push(point);
            }
        }
    }
    spots
}

/// Whether a ground point lies in the window's open part, clear of its edges and of the HUD.
fn on_screen<'a>(
    world: &'a SimWorld,
    camera: (&'a Camera, &'a GlobalTransform),
) -> impl Fn(Vec2) -> bool + 'a {
    let (camera, transform) = camera;
    let window = camera
        .physical_viewport_rect()
        .map_or(Vec2::new(1280.0, 720.0), |rect| rect.max.as_vec2());
    move |point: Vec2| {
        let ground = Vec3::new(
            point.x,
            world.map.terrain_altitude_at(sim_position(point)),
            point.y,
        );
        camera.world_to_viewport(transform, ground).is_ok_and(|at| {
            at.x > SCREEN_MARGIN
                && at.x < window.x - SCREEN_MARGIN
                && at.y > SCREEN_MARGIN
                && at.y < window.y * SCREEN_BOTTOM_SHARE
        })
    }
}

/// Projects a ground point (world `x, y`) to window pixels.
fn project(world: &SimWorld, camera: (&Camera, &GlobalTransform), point: Vec2) -> [f32; 2] {
    let (camera, transform) = camera;
    let ground = Vec3::new(
        point.x,
        world.map.terrain_altitude_at(sim_position(point)),
        point.y,
    );
    let offset = camera
        .physical_viewport_rect()
        .map_or(Vec2::ZERO, |rect| rect.min.as_vec2());
    camera
        .world_to_viewport(transform, ground)
        .map_or([-1.0, -1.0], |at| (at + offset).to_array())
}

/// A presentable mobile ground unit type that is no shooter of `wave`: the targets' type,
/// unarmed in the test world's catalog so the targets fire on nothing.
fn target_type(world: &mut SimWorld, wave: &[Planned]) -> UnitTypeId {
    let shooters: Vec<UnitTypeId> = wave.iter().map(|(shooter, ..)| shooter.unit_type).collect();
    let id = world
        .catalog
        .archetypes()
        .filter(|arch| {
            arch.max_speed > SimScalar::ZERO
                && arch.mover_type == MoverType::Ground
                && arch.modes.is_empty()
                && !shooters.contains(&arch.unit_type_id)
                && presentable(&assets(), &arch.slug)
        })
        .max_by_key(|arch| arch.collision_radius)
        .map(|arch| arch.unit_type_id)
        .expect("a presentable ground unit is no shooter");
    let archetype = world
        .catalog
        .get_mut(id)
        .expect("invariant: the type was just found in this catalog");
    archetype.weapon = None;
    archetype.ground_weapon = None;
    archetype.air_weapon = None;
    id
}

/// Lends each lab family of `wave` to its shooter's weapon entry, in the test world's catalog.
fn lend(world: &mut SimWorld, wave: &[Planned]) {
    for (shooter, family, _) in wave.iter().filter(|(.., own)| !own) {
        let archetype = world
            .catalog
            .get_mut(shooter.unit_type)
            .expect("invariant: the planned shooter is of this catalog");
        if let Some(entry) = archetype
            .effects
            .weapons
            .iter_mut()
            .find(|entry| entry.weapon == shooter.slot && entry.mode.is_none())
        {
            entry.travel.clone_from(&family.travel);
            entry.impact.clone_from(&family.impact);
        }
    }
}

/// Spawns a unit of `unit_type` for `owner` at `place`, unpushable.
fn spawn_still(
    world: &mut SimWorld,
    unit_type: UnitTypeId,
    owner: PlayerId,
    place: Vec2,
) -> EntityId {
    let id = world
        .spawn_unit(unit_type, owner, sim_position(place))
        .expect("open ground");
    world
        .entities
        .get_mut(&id)
        .expect("just spawned")
        .unpushable = true;
    id
}

/// Stages the child's wave on `spots`: each shooter on the left of its spot, its target on the
/// right, within its weapon's range, and orders the attacks.
fn stage(
    staging: &mut Staging,
    session: &mut BattleSession,
    catalog: &EffectCatalog,
    runtime: &EffectRuntime,
    spots: &[Vec2],
) {
    let world = session.world_mut().expect("a ready session has a world");
    let (waves, unstaged) = plan(world, spots.len());
    staging.report.waves = waves.len();
    staging.report.unstaged = unstaged.iter().map(Family::label).collect();
    let Some(wave) = waves.get(staging.wave) else {
        return;
    };
    let target = target_type(world, wave);
    lend(world, wave);
    let target_radius = world
        .catalog
        .get(target)
        .map_or(0.5, |arch| arch.collision_radius.to_f32());
    let mut attacks = Vec::new();
    for ((shooter, family, own), spot) in wave.iter().zip(spots) {
        let arch = world
            .catalog
            .get(shooter.unit_type)
            .expect("invariant: the planned shooter is of this catalog");
        let range = slot_weapon(arch, shooter.slot).map_or(1.0, |spec| spec.max_range.to_f32());
        let radii = arch.collision_radius.to_f32() + target_radius;
        let gap = (radii + range * 0.7).min(MOST_GAP).max(radii + 0.05);
        let slug = arch.slug.clone();
        let from = spawn_still(
            world,
            shooter.unit_type,
            VIEWER,
            *spot - Vec2::new(gap * 0.5, 0.0),
        );
        let to = spawn_still(world, target, ENEMY, *spot + Vec2::new(gap * 0.5, 0.0));
        world.entities.get_mut(&to).expect("spawned").is_immobile = true;
        attacks.push((from, to));
        let look = |name: &Option<String>| {
            name.as_deref()
                .and_then(|name| catalog.effect_id(name))
                .map(|id| (id, runtime.spawned_of(id)))
        };
        staging.subjects.push(StagedSubject {
            units: [from, to],
            centre: *spot,
            looks: [look(&family.travel), look(&family.impact)],
        });
        staging.report.subjects.push(SubjectReport {
            family: family.label(),
            shooter: slug,
            own: *own,
            ..SubjectReport::default()
        });
    }
    session.publish();
    for (from, to) in attacks {
        let (world, queue) = session.world_and_commands();
        queue
            .submit_selection(
                world.expect("a ready session has a world"),
                VIEWER,
                &[from],
                UnitOrder::attack(to),
                false,
            )
            .expect("the client admits an attack by the viewer's own unit");
    }
}

/// Holds every staged unit at full health, so nothing dies in the pictures.
fn heal(staging: &Staging, session: &mut BattleSession) {
    let world = session.world_mut().expect("a ready session has a world");
    for subject in &staging.subjects {
        for id in subject.units {
            let Some(unit) = world.entities.get(&id) else {
                continue;
            };
            let full = world
                .catalog
                .get(unit.unit_type_id)
                .map_or(unit.hp, |arch| arch.max_hp);
            if let Some(unit) = world.entities.get_mut(&id) {
                unit.hp = full;
            }
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

/// Requests a screenshot of this frame, writing each crop of `crops` (`(path, [left, top,
/// width, height])`, window pixels, clamped to the window), without waiting for it: the counter
/// `outstanding` counts it until it is written.
fn capture_frame(
    commands: &mut Commands<'_, '_>,
    crops: Vec<(PathBuf, [u32; 4])>,
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
            outstanding.fetch_sub(1, Ordering::SeqCst);
        },
    );
}

/// Pictures every subject this frame.
fn picture(
    commands: &mut Commands<'_, '_>,
    staging: &mut Staging,
    session: &BattleSession,
    camera: (&Camera, &GlobalTransform),
    frame: u32,
) {
    let world = session.world().expect("a ready session has a world");
    let mut crops = Vec::new();
    for (index, subject) in staging.subjects.iter().enumerate() {
        let [x, y] = project(world, camera, subject.centre);
        let file = format!("wave{}_{index}_{frame:03}.png", staging.wave);
        let left = (x - CROP[0] as f32 / 2.0).max(0.0) as u32;
        let top = (y - CROP[1] as f32 * 0.6).max(0.0) as u32;
        crops.push((staging.directory.join(&file), [left, top, CROP[0], CROP[1]]));
        staging.report.subjects[index].frames.push(file);
    }
    if !crops.is_empty() {
        capture_frame(commands, crops, &staging.outstanding);
    }
}

/// Records each subject's spawns since the staging.
fn count(staging: &mut Staging, runtime: &EffectRuntime) {
    for (subject, report) in staging.subjects.iter().zip(&mut staging.report.subjects) {
        let spawned =
            |look: Option<(_, u32)>| look.map_or(0, |(id, at)| runtime.spawned_of(id) - at);
        report.travel_spawned = spawned(subject.looks[0]);
        report.impact_spawned = spawned(subject.looks[1]);
    }
}

/// Advances the child by one frame.
fn stage_wave_child(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, Staging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    time: Res<'_, Time<Virtual>>,
    fog: Option<Res<'_, BattleFog>>,
    effects: (
        Option<Res<'_, EffectRuntime>>,
        Option<Res<'_, EffectCatalog>>,
    ),
    cameras: Query<'_, '_, (&Camera, &GlobalTransform, &RtsCamera)>,
    idle: Res<'_, LabRenderIdle>,
) {
    let fog = fog.expect("the skirmish presents a player, so it keeps a fog");
    if !session.is_ready() {
        return;
    }
    let Ok((camera, transform, rts)) = cameras.get_single() else {
        return;
    };
    let tick = session.published().map_or(0, |published| published.tick);
    let now = time.elapsed_secs_f64();
    let idle_long = idle.0.load(Ordering::Acquire) >= IDLE_FRAMES;
    staging.phase = match staging.phase {
        Phase::Boot if tick < 2 => Phase::Boot,
        Phase::Boot => Phase::Booting {
            still: 0,
            height: rts.look_at_y,
        },
        Phase::Booting { still, height } if still < STILL_CAMERA_FRAMES || !idle_long => {
            let still = if (rts.look_at_y - height).abs() < 1e-4 {
                still + 1
            } else {
                0
            };
            Phase::Booting {
                still,
                height: rts.look_at_y,
            }
        }
        Phase::Booting { .. } => Phase::Framing { frames: 0 },
        Phase::Framing { frames } if frames < FRAMING_FRAMES || !idle_long => {
            Phase::Framing { frames: frames + 1 }
        }
        Phase::Framing { .. } => {
            let (Some(runtime), Some(catalog)) = (effects.0.as_deref(), effects.1.as_deref())
            else {
                panic!("the effect system is loaded before the staging");
            };
            let world = session.world().expect("a ready session has a world");
            let spots = find_spots(
                world,
                &fog,
                rts.target,
                MAX_SUBJECTS,
                on_screen(world, (camera, transform)),
            );
            hold_own_units(&mut session);
            stage(&mut staging, &mut session, catalog, runtime, &spots);
            commands.queue(rebuild_tables);
            lab.battle_paused = false;
            Phase::Running { at: now, frame: 0 }
        }
        Phase::Running { at, frame } if now - at <= WAVE_SECONDS => {
            heal(&staging, &mut session);
            picture(
                &mut commands,
                &mut staging,
                &session,
                (camera, transform),
                frame,
            );
            if let Some(runtime) = effects.0.as_deref() {
                count(&mut staging, runtime);
            }
            Phase::Running {
                at,
                frame: frame + 1,
            }
        }
        Phase::Running { .. } => {
            lab.battle_paused = true;
            Phase::Finishing
        }
        Phase::Finishing if staging.outstanding.load(Ordering::SeqCst) > 0 => Phase::Finishing,
        Phase::Finishing => {
            let text = ron::to_string(&staging.report).expect("plain data");
            std::fs::write(staging.directory.join(REPORT_FILE), text)
                .expect("the parent's directory is writable");
            commands.send_event(AppExit::Success);
            Phase::Finishing
        }
    };
}

/// Holds every mobile unit of the viewer, so nothing walks through the pictures.
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

/// The child: boots the production client on the skirmish and stages its wave.
#[test]
#[ignore = "launched by the test below on a virtual display"]
fn weapon_wave_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    let wave: usize = std::env::var(WAVE_VAR)
        .expect("the parent names the wave")
        .parse()
        .expect("the wave is a number");
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
    // Every frame advances presentation time by one 40 ms tick, whatever the frame took.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(FRAME_STEP))
        .insert_resource(idle)
        .insert_resource(Staging {
            directory: PathBuf::from(directory),
            wave,
            ..Staging::default()
        })
        .add_systems(Update, stage_wave_child.in_set(BattleSet::Present));
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// Runs the child of `wave`; returns its report and directory.
fn run_wave(wave: usize) -> TestResult<(WaveReport, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", "weapon_wave_child"]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.env(WAVE_VAR, wave.to_string());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("weapon wave");
    let report: WaveReport = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    Ok((report, directory))
}

/// The most pixels of any frame of `frames` that changed by [`SEEN_LEVEL`] against the first.
fn most_changed(directory: &Path, frames: &[String]) -> TestResult<usize> {
    let Some(first) = frames.first() else {
        return Ok(0);
    };
    let first = image::open(directory.join(first))?.to_rgb8();
    let mut most = 0;
    for file in frames.iter().skip(1) {
        let frame = image::open(directory.join(file))?.to_rgb8();
        let changed = frame
            .pixels()
            .zip(first.pixels())
            .filter(|(a, b)| {
                a.0.iter()
                    .zip(b.0.iter())
                    .any(|(a, b)| a.abs_diff(*b) >= SEEN_LEVEL)
            })
            .count();
        most = most.max(changed);
    }
    Ok(most)
}

/// Writes `subject`'s sheet: [`SHEET_FRAMES`] frames from its first changed frame at 1:1 over
/// the first of them at 2x.
fn write_sheet(directory: &Path, subject: &SubjectReport, target: &Path, name: &str) -> TestResult {
    let pictures: Vec<image::RgbImage> = subject
        .frames
        .iter()
        .filter_map(|file| image::open(directory.join(file)).ok())
        .map(|image| image.to_rgb8())
        .collect();
    let Some(first) = pictures.first() else {
        return Ok(());
    };
    let start = pictures
        .iter()
        .position(|picture| {
            picture.pixels().zip(first.pixels()).any(|(a, b)| {
                a.0.iter()
                    .zip(b.0.iter())
                    .any(|(a, b)| a.abs_diff(*b) >= SEEN_LEVEL)
            })
        })
        .unwrap_or(0)
        .saturating_sub(1);
    let (width, height) = (first.width(), first.height());
    let run: Vec<&image::RgbImage> = pictures.iter().skip(start).take(SHEET_FRAMES.0).collect();
    let columns = 6;
    let rows_1x = run.len().div_ceil(columns) as u32;
    let doubled = SHEET_FRAMES.1.min(run.len());
    let columns_2x = 3;
    let rows_2x = doubled.div_ceil(columns_2x) as u32;
    let mut sheet = image::RgbImage::new(
        width * columns as u32,
        height * rows_1x + height * 2 * rows_2x,
    );
    for (index, picture) in run.iter().enumerate() {
        let (column, row) = ((index % columns) as u32, (index / columns) as u32);
        image::imageops::replace(
            &mut sheet,
            *picture,
            i64::from(column * width),
            i64::from(row * height),
        );
    }
    for (index, picture) in run.iter().take(doubled).enumerate() {
        let big = image::imageops::resize(
            *picture,
            width * 2,
            height * 2,
            image::imageops::FilterType::Nearest,
        );
        let (column, row) = ((index % columns_2x) as u32, (index / columns_2x) as u32);
        image::imageops::replace(
            &mut sheet,
            &big,
            i64::from(column * width * 2),
            i64::from(height * rows_1x + row * height * 2),
        );
    }
    sheet.save(target.join(name))?;
    Ok(())
}

/// Review 1, V1-1: every family of weapon looks fires in a staged battle at the default zoom,
/// the presentable ones on their own units and the lab ones lent to a real bubble.
#[test]
fn weapon_looks_fire_in_a_staged_battle() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let driver = gpu_run::selected_driver().driver;
    let evidence = std::env::var_os(EVIDENCE_VAR).map(PathBuf::from);
    if let Some(target) = &evidence {
        std::fs::create_dir_all(target)?;
    }
    let mut wave = 0;
    let mut failures = Vec::new();
    loop {
        let (report, directory) = run_wave(wave)?;
        if wave == 0 {
            eprintln!(
                "driver {driver}: {} waves; lab families no presentable shooter carries: {:?}",
                report.waves, report.unstaged
            );
        }
        for (index, subject) in report.subjects.iter().enumerate() {
            let changed = most_changed(directory.path(), &subject.frames)?;
            eprintln!(
                "driver {driver}: wave {wave} {} on {} ({}): travel spawned {}, impact spawned \
                 {}, at most {changed} px changed by {SEEN_LEVEL} levels over {} frames",
                subject.family,
                subject.shooter,
                if subject.own { "its own" } else { "lent" },
                subject.travel_spawned,
                subject.impact_spawned,
                subject.frames.len()
            );
            let (travel, impact) = subject
                .family
                .split_once('+')
                .expect("invariant: a family's label joins its two looks with '+'");
            if travel != "none" && subject.travel_spawned == 0 {
                failures.push(format!("{}: its travel never spawned", subject.family));
            }
            if impact != "none" && subject.impact_spawned == 0 {
                failures.push(format!("{}: its impact never spawned", subject.family));
            }
            if changed < SEEN_PIXELS {
                failures.push(format!("{}: {changed} px changed", subject.family));
            }
            if let Some(target) = &evidence {
                write_sheet(
                    directory.path(),
                    subject,
                    target,
                    &format!(
                        "wp11r2_battle_{driver}_w{wave}_{index}_{}.png",
                        subject.family
                    ),
                )?;
            }
        }
        wave += 1;
        if wave >= report.waves {
            break;
        }
    }
    assert!(
        failures.is_empty(),
        "families that did not show:\n{}",
        failures.join("\n")
    );
    Ok(())
}
