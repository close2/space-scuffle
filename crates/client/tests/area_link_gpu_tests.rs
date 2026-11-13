//! Area and tether looks on a real GPU (plan `2026-11-09-effect-visualization` WP10).
//!
//! # Responsibility
//! Proves the WP10 looks all the way to pixels in the production client playing the shipped
//! skirmish. One child stages, on open ground the player sees:
//! - **Areas**: one of the player's units casts the shipped area spells whose casters' blocks
//!   name a look (a storm and a cloud) through the simulation's own cast order; with the
//!   battle paused while both areas are published, the child pictures them whole, with only
//!   their `Critical` layers and with no effect layer, and samples rays out of each area's
//!   centre on the ground. The parent checks the plan's "area edge at `AreaRadius` ±2 px": along
//!   every ray the outermost pixel the `Critical` layers change lies within
//!   [`EDGE_TOLERANCE_PX`] of the projected radius.
//! - **A tether**: the unit casts the shipped tether spell on an enemy; while the tether is
//!   published, the child pictures it whole, with its `Critical` layer only and with no effect
//!   layer. The parent checks "tether
//!   ends on both bubbles ±2 px": along the line between the two bodies, the pixels the tether
//!   changes run from each bubble's surface point to within [`EDGE_TOLERANCE_PX`].
//! - **Fog**: an enemy caster standing in the fog opens a storm across the edge of the viewer's
//!   sight and pulls a unit of its own, deeper in the fog, with its tether. The parent checks
//!   that the storm's `Critical` layers change no pixel of ground the viewer does not see and do
//!   change the ground it sees, and the child that no tether look is held (both ends hidden).
//! - **Sequences** (evidence only): the looks cast again and pictured every other frame at the
//!   default zoom, assembled by the parent into contact sheets.
//!
//! With `SPACE_SCUFFLE_AREA_EVIDENCE_DIR` set, the parent copies its pictures and sheets there.
//!
//! # How it fits
//! The child re-executes this test binary through `support/gpu_run.rs` (charter §7, C21), as
//! `status_gpu_tests.rs` does. The casters are chosen by catalog properties (presentation blocks
//! naming abilities, media), never by name (C5): the shipped casters have no media, so a
//! presentable unit gets copies of their abilities and blocks in the test world's catalog only.
//!
//! # Invariants verified
//! - The staging published what it claims (the areas, the tether, their owners).
//! - Each world change is published by a tick before the clock stops for its shot.

#![allow(
    clippy::single_match_else,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    clippy::type_complexity
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

use std::collections::BTreeMap;
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
use client::effects::areas::AreaLooks;
use client::effects::catalog::schema::Priority;
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
    AbilityTargetKind, EntityId, MoverType, PlayerId, SimScalar, UnitOrder, UnitTypeId,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the directory the child writes to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_AREA_CHILD_OUT";
/// Environment variable naming a directory the parent copies its pictures to.
const EVIDENCE_VAR: &str = "SPACE_SCUFFLE_AREA_EVIDENCE_DIR";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// File the child writes its report to.
const REPORT_FILE: &str = "report.ron";
/// The viewer: the skirmish's first seat.
const VIEWER: PlayerId = PlayerId(0);
/// The other side.
const ENEMY: PlayerId = PlayerId(1);
/// Frames the camera's look height must hold still before the staging starts.
const STILL_CAMERA_FRAMES: u32 = 3;
/// Rendered frames, at least, between a stopped clock and its capture.
const SETTLE_FRAMES: u32 = 3;
/// Seconds of `Time<Virtual>`, at least, between a stopped clock and its first capture: every
/// held layer is in.
const SETTLE_SECONDS: f64 = 0.6;
/// Presentation time each frame advances: one simulation tick.
const FRAME_STEP: Duration = Duration::from_millis(40);
/// Frames in a row with no pipeline waiting before a capture.
const IDLE_FRAMES: u32 = 8;
/// Ticks a staged cast may take to publish what it opens.
const CAST_TIMEOUT_TICKS: u32 = 200;
/// Distances from the camera's target, in world units, at which spots are sought.
const SPOT_DISTANCES: [f32; 15] = [
    4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 3.5, 4.5, 5.5, 6.5, 7.5, 10.0, 11.0, 12.0, 13.0,
];
/// Clearance around a spot from any unit, structure or node, world units.
const SPOT_CLEARANCE: f32 = 3.4;
/// Window pixels a spot keeps from the window's sides and top.
const SCREEN_MARGIN: f32 = 140.0;
/// Share of the window's height above which a spot lies (the HUD is below).
const SCREEN_BOTTOM_SHARE: f32 = 0.68;
/// Channel change, 0 to 255, at which a pixel counts as changed.
const CHANGE_LEVEL: u8 = 8;
/// The plan's tolerance on an area's edge and a tether's ends, window pixels.
const EDGE_TOLERANCE_PX: f32 = 2.0;
/// Rays sampled out of an area's centre.
const RAYS: usize = 32;
/// Samples along a ray, from [`RAY_FROM`] to [`RAY_TO`] radii.
const RAY_SAMPLES: usize = 91;
/// Nearest sample of a ray, in radii.
const RAY_FROM: f32 = 0.5;
/// Farthest sample of a ray, in radii.
const RAY_TO: f32 = 1.4;
/// World units a ground sample keeps from a cell boundary of the fog to count as seen or unseen.
const FOG_MARGIN: f32 = 0.25;
/// World units round a spot that must be walkable ground (beyond the largest area's radius and
/// the rays' reach over the smaller ones).
const GROUND_REACH: f32 = 2.6;
/// Seconds of the evidence sequences, from the casts.
const SEQUENCE_SECONDS: f64 = 6.2;
/// Seconds of an area's coming in pictured every frame, from its publication (V1-1).
const COME_IN_SECONDS: f64 = 0.4;
/// Seconds before an area's end from which it is pictured every frame (V1-1).
const BEFORE_END_SECONDS: f64 = 0.2;
/// Seconds after an area's or the tether's end it is still pictured every frame (V1-1).
const AFTER_END_SECONDS: f64 = 0.5;
/// Frame of the sequence at which the whole window is pictured with every unit in the areas
/// (V1-2), 2.0 s after the casts.
const CROWD_FRAME: u32 = 50;
/// Frames between two whole-window pictures of the battle sheet.
const BATTLE_EVERY: u32 = 20;
/// Units of each side staged in each area of the evidence sequences (V1-2).
const CROWD_PER_SIDE: usize = 4;
/// Pixels of a 2x crop of the crowd picture round each area.
const CROWD_CROP: [u32; 2] = [200, 150];
/// Least median luminance change, in levels, of the band just inside an area's projected
/// radius against the bare floor (WP10 review 1, E1-5 (a)).
const EDGE_CONTRAST: f32 = 32.0;
/// Window pixels outside an area's projected radius beyond which its whole look changes no
/// pixel along the near half's rays (E1-5 (b)).
const OUTSIDE_SPILL_PX: f32 = 4.0;
/// Radii out to which E1-5 (b) seeks spill: beyond lie the staged units at
/// [`SPOT_CLEARANCE`] and their shadows.
const SPILL_REACH: f32 = 1.2;
/// Band, in window pixels, just inside the projected radius for E1-5 (a).
const EDGE_BAND_PX: f32 = 3.0;
/// Unchanged ray samples (0.01 radii each) bridged inside an area's footprint.
const EDGE_GAP: usize = 4;
/// Window pixels of a sequence's crop round its subject.
const CROP: [u32; 2] = [360, 260];

/// The shots of the child, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Shot {
    Areas,
    AreasCritical,
    AreasBare,
    Tether,
    TetherCritical,
    TetherBare,
    Fog,
    FogCritical,
    FogBare,
}

impl Shot {
    fn file(self) -> String {
        format!("shot_{self:?}.png").to_lowercase()
    }
}

/// One area as pictured: its rays' projected samples.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AreaShot {
    /// The look's effect name, from the caster's block.
    look: String,
    /// Owner.
    owner: u8,
    /// Ground centre, world `x, y`, and radius.
    centre: [f32; 2],
    radius: f32,
    /// Per ray, the window pixel of each sample from [`RAY_FROM`] to [`RAY_TO`] radii.
    rays: Vec<Vec<[f32; 2]>>,
    /// Per ground sample of the disc: its window pixel and whether the viewer sees it (`None`
    /// within [`FOG_MARGIN`] of a fog boundary).
    ground: Vec<([f32; 2], Option<bool>)>,
    /// Window pixel of the centre.
    centre_px: [f32; 2],
}

/// The tether as pictured.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TetherShot {
    /// Window pixels of the two bodies' centres.
    from_centre: [f32; 2],
    to_centre: [f32; 2],
    /// Window pixels of the tether's two ends (each body's surface point facing the other).
    from_end: [f32; 2],
    to_end: [f32; 2],
}

/// What the child reports.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Report {
    areas: Vec<AreaShot>,
    tether: Option<TetherShot>,
    /// The enemy's storm across the fog edge.
    fog_area: Option<AreaShot>,
    /// Looks held while the enemy's tether (its caster in fog) was published: `(areas, links)`.
    fog_held: (usize, usize),
    /// The viewer's own tethers published at the fog shot (both ends shown, so held).
    fog_own_links: usize,
    /// Links the snapshot published at the fog shot.
    fog_links: usize,
    /// Sequence pictures written: `(look, files)`.
    sequences: Vec<(String, Vec<String>)>,
    /// The fog shot's unseen cells near the enemy area, as window-pixel quads (V1-3 overlay).
    fog_cells: Vec<[[f32; 2]; 4]>,
    /// Whether the fog edge chosen runs along a change of ground level (a cliff, V1-3).
    fog_cliff: bool,
    /// Window pixels of the areas' centres in the crowd picture (V1-2).
    crowd: Vec<[f32; 2]>,
}

/// How the effect layers are drawn for a shot.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
struct EffectView {
    /// No effect layer is drawn.
    bare: bool,
}

/// Clears the frame's effect instances for a bare shot.
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

/// What the child is doing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Phase {
    #[default]
    Boot,
    Booting {
        still: u32,
        height: f32,
    },
    /// Casts were ordered for stage `stage` at tick `since`; waiting for their publication.
    Casting {
        stage: usize,
        since: u32,
    },
    /// The clock is stopped since `at` for shot `shot`.
    Settling {
        shot: usize,
        frames: u32,
        at: f64,
    },
    Capturing,
    /// Evidence sequence: `frame` frames since the casts.
    Sequence {
        frame: u32,
        since: f64,
    },
    Draining,
    Done,
}

/// The stages: what is cast, and which shots are then taken.
const STAGES: [&[Shot]; 3] = [
    &[Shot::Areas, Shot::AreasCritical, Shot::AreasBare],
    &[Shot::Tether, Shot::TetherCritical, Shot::TetherBare],
    &[Shot::Fog, Shot::FogCritical, Shot::FogBare],
];

/// The child's staging.
#[derive(Resource, Default)]
struct Staging {
    directory: PathBuf,
    phase: Phase,
    /// The ability index, on the caster type, of each look: `(look, index, target kind)`.
    abilities: Vec<(String, u8, AbilityTargetKind)>,
    caster_type: Option<UnitTypeId>,
    caster: Option<EntityId>,
    victim: Option<EntityId>,
    spots: Vec<Vec2>,
    /// The fog stage: edge point, the enemy caster's place, the viewer's victim's place.
    fog: Option<(Vec2, Vec2, Vec2)>,
    enemy_caster: Option<EntityId>,
    outstanding: Arc<AtomicU32>,
    report: Report,
    /// The evidence child: the casts at once, pictured as sequences, no measured shots.
    sequence_only: bool,
    /// A second caster of the evidence child, for a storm overlapping the first (V1-2).
    second_caster: Option<EntityId>,
    /// What the evidence child pictures, by published area or tether.
    watches: Vec<Watch>,
}

/// An area or the tether the evidence child pictures (V1-1).
#[derive(Debug, Clone, Default)]
struct Watch {
    /// Name of its sheets: the look and a count, or `tether`.
    key: String,
    /// The area's id; `None` for the tether.
    id: Option<u32>,
    /// Where it stands, map `x, y`.
    at: Vec2,
    /// When it was first published, seconds of `Time<Virtual>`.
    first: f64,
    /// When it entered its last [`BEFORE_END_SECONDS`].
    ending: Option<f64>,
    /// When it was no longer published.
    gone: Option<f64>,
}

/// Whether the ground round `point` is open and walkable: every cell centre within 2 world
/// units and every point of the circle of [`GROUND_REACH`] round it, so an area of up to 2.25
/// world units centred there lies on the ground.
fn open(world: &SimWorld, point: Vec2) -> bool {
    let map = &world.map;
    let walkable = |probe: Vec2| {
        map.world_to_cell(sim_position(probe))
            .ok()
            .and_then(|(cx, cy)| map.cell(cx, cy).ok())
            .is_some_and(|cell| {
                cell.flags.contains(CellFlags::GROUND_PATHABLE)
                    && !cell
                        .flags
                        .intersects(CellFlags::OCCUPIED_BY_STRUCTURE | CellFlags::UNPASSABLE_CLIFF)
            })
    };
    let rim = (0..16).all(|step| {
        walkable(
            point + Vec2::from_angle(step as f32 * std::f32::consts::TAU / 16.0) * GROUND_REACH,
        )
    });
    open_cells(world, point) && rim
}

/// Whether the 5 by 5 cells around `point` are open, walkable ground (a unit's place).
fn open_cells(world: &SimWorld, point: Vec2) -> bool {
    let map = &world.map;
    (-2..=2).all(|x: i32| {
        (-2..=2).all(|y: i32| {
            map.world_to_cell(sim_position(point + Vec2::new(x as f32, y as f32)))
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

/// Whether `point` is clear of every unit and node by [`SPOT_CLEARANCE`].
fn clear(world: &SimWorld, point: Vec2) -> bool {
    let units = world.entities.values().all(|unit| {
        Vec2::new(unit.position.x.to_f32(), unit.position.y.to_f32()).distance(point)
            > SPOT_CLEARANCE
    });
    let nodes = world.map.resources().iter().all(|node| {
        Vec2::new(node.pos.x.to_f32(), node.pos.y.to_f32()).distance(point) > SPOT_CLEARANCE
    });
    units && nodes
}

/// A test of whether a ground point lies in the window's open part, seen through `camera`.
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

/// Points round `around`, nearest first.
fn candidates(around: Vec2) -> impl Iterator<Item = Vec2> {
    SPOT_DISTANCES.iter().flat_map(move |distance| {
        (0..24).map(move |step| {
            around + Vec2::from_angle(step as f32 * std::f32::consts::TAU / 24.0) * *distance
        })
    })
}

/// `count` spots of open ground in sight near `around`, clear and `apart` from each other.
fn find_spots(
    world: &SimWorld,
    fog: &BattleFog,
    around: Vec2,
    count: usize,
    apart: f32,
    visible: impl Fn(Vec2) -> bool,
) -> Vec<Vec2> {
    let mut spots: Vec<Vec2> = Vec::new();
    for point in candidates(around) {
        if spots.len() == count {
            break;
        }
        // The first two spots take the areas; the others are units' places.
        let ground = if spots.len() < 2 {
            open(world, point)
        } else {
            open_cells(world, point)
        };
        if ground
            && clear(world, point)
            && spots.iter().all(|spot| spot.distance(point) > apart)
            && fog.sees(point.x, point.y)
            && visible(point)
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

/// A point at the edge of the viewer's sight on screen, with a place for an enemy caster in the
/// fog behind it and a place for the caster's own victim deeper in the fog: `(edge, caster,
/// victim)`. Sought once the viewer's staged units stand, so their sight is in the fog.
fn find_fog_edge(
    world: &SimWorld,
    fog: &BattleFog,
    around: Vec2,
    cliff: bool,
    visible: impl Fn(Vec2) -> bool,
) -> Option<(Vec2, Vec2, Vec2)> {
    (0..48).find_map(|step| {
        let direction = Vec2::from_angle(step as f32 * std::f32::consts::TAU / 48.0);
        let mut previous = around;
        for distance in (4..60).map(|half| half as f32 * 0.5) {
            let point = around + direction * distance;
            if fog.sees(point.x, point.y) {
                previous = point;
                continue;
            }
            // `previous` is the last seen point before the fog along this direction.
            let edge = previous;
            let unseen_open =
                |place: &Vec2| open_cells(world, *place) && !fog.sees(place.x, place.y);
            let caster = (8..16)
                .map(|k| edge + direction * k as f32 * 0.5)
                .find(unseen_open)?;
            let victim = caster + direction * 2.5;
            let ok = visible(edge)
                && visible(edge + direction * 1.5)
                && open(world, edge)
                && unseen_open(&victim)
                && clear(world, edge)
                && (!cliff || {
                    let level = |at: Vec2| world.map.terrain_altitude_at(sim_position(at));
                    (level(edge) - level(edge + direction * 1.5)).abs() > 0.3
                });
            return ok.then_some((edge, caster, victim));
        }
        None
    })
}

/// The tick of the published snapshot.
fn published_tick(session: &BattleSession) -> u32 {
    session.published().map_or(0, |published| published.tick)
}

/// Gives a presentable ground unit type copies of every ability that a shipped unit's
/// presentation block names a look for, with those looks, and a deep energy pool; returns it
/// and `(look, index, target kind)` per copied ability.
fn caster_type(world: &mut SimWorld) -> (UnitTypeId, Vec<(String, u8, AbilityTargetKind)>) {
    let mut copied: Vec<(sim_types::AbilitySpec, String)> = Vec::new();
    for arch in world.catalog.archetypes() {
        for (name, look) in &arch.effects.abilities {
            if let Some(spec) = arch.abilities.iter().find(|spec| spec.name == *name) {
                copied.push((spec.clone(), look.clone()));
            }
        }
    }
    assert!(
        copied.len() >= 3,
        "the shipped blocks name area and tether looks: {copied:?}"
    );
    let id = world
        .catalog
        .archetypes()
        .filter(|arch| {
            arch.max_speed > SimScalar::ZERO
                && arch.mover_type == MoverType::Ground
                && arch.modes.is_empty()
                && arch.abilities.is_empty()
                && presentable(&assets(), &arch.slug)
        })
        .map(|arch| arch.unit_type_id)
        .next()
        .expect("the catalog ships a presentable ground unit");
    let archetype = world
        .catalog
        .get_mut(id)
        .expect("invariant: the type was just found in this catalog");
    archetype.max_energy = SimScalar::from_num(1000);
    archetype.weapon = None;
    archetype.ground_weapon = None;
    archetype.air_weapon = None;
    let mut abilities = Vec::new();
    for (spec, look) in copied {
        let index = u8::try_from(archetype.abilities.len()).expect("a few abilities");
        abilities.push((look.clone(), index, spec.target_kind));
        archetype.effects.abilities.insert(spec.name.clone(), look);
        archetype.abilities.push(spec);
    }
    (id, abilities)
}

/// Spawns a unit that stands where it is put, full of energy.
fn spawn_still(world: &mut SimWorld, kind: UnitTypeId, owner: PlayerId, place: Vec2) -> EntityId {
    let id = world
        .spawn_unit(kind, owner, sim_position(place))
        .expect("open ground");
    let unit = world.entities.get_mut(&id).expect("just spawned");
    unit.unpushable = true;
    unit.energy = SimScalar::from_num(1000);
    unit.issue_order(UnitOrder::HoldPosition, false)
        .expect("hold admitted");
    id
}

/// Orders `caster` to cast every look of `staging` of `kind` (the areas at `targets` in turn, or
/// the tether on `victim`), queued one after another.
fn order_casts(
    world: &mut SimWorld,
    staging: &Staging,
    caster: EntityId,
    kind: AbilityTargetKind,
    targets: &[Vec2],
    victim: Option<EntityId>,
) {
    let unit = world.entities.get_mut(&caster).expect("the caster stands");
    unit.energy = SimScalar::from_num(1000);
    let mut targets = targets.iter();
    let mut queue = false;
    for (_, index, target) in &staging.abilities {
        if *target != kind {
            continue;
        }
        let order = match kind {
            AbilityTargetKind::TargetEntity => UnitOrder::CastAbility {
                ability_index: *index,
                target_pos: None,
                target_entity: victim,
            },
            _ => UnitOrder::CastAbility {
                ability_index: *index,
                target_pos: Some(sim_position(*targets.next().expect("a target per area"))),
                target_entity: None,
            },
        };
        unit.issue_order(order, queue)
            .expect("the cast is admitted");
        queue = true;
    }
}

/// Orders the casts of stage `stage`.
fn stage_casts(staging: &mut Staging, world: &mut SimWorld) {
    let caster = staging.caster.expect("spawned");
    match staging_stage(staging) {
        0 => {
            let targets = [staging.spots[0], staging.spots[1]];
            order_casts(
                world,
                staging,
                caster,
                AbilityTargetKind::TargetPosition,
                &targets,
                None,
            );
        }
        1 => {
            let victim = staging.victim.expect("spawned");
            order_casts(
                world,
                staging,
                caster,
                AbilityTargetKind::TargetEntity,
                &[],
                Some(victim),
            );
        }
        _ => {
            let (edge, at, victim_at) = staging.fog.expect("found");
            let kind = staging.caster_type.expect("made");
            let enemy = spawn_still(world, kind, ENEMY, at);
            // The enemy pulls a unit of its own, deeper in the fog: a tether wholly unseen.
            let victim = spawn_still(world, kind, ENEMY, victim_at);
            staging.enemy_caster = Some(enemy);
            // Only the first area look: one storm across the edge.
            let (_, index, _) = staging
                .abilities
                .iter()
                .find(|(_, _, kind)| *kind == AbilityTargetKind::TargetPosition)
                .cloned()
                .expect("an area look");
            let (_, tether, _) = staging
                .abilities
                .iter()
                .find(|(_, _, kind)| *kind == AbilityTargetKind::TargetEntity)
                .cloned()
                .expect("a tether look");
            let unit = world.entities.get_mut(&enemy).expect("spawned");
            unit.issue_order(
                UnitOrder::CastAbility {
                    ability_index: index,
                    target_pos: Some(sim_position(edge)),
                    target_entity: None,
                },
                false,
            )
            .expect("the enemy's cast is admitted");
            unit.issue_order(
                UnitOrder::CastAbility {
                    ability_index: tether,
                    target_pos: None,
                    target_entity: Some(victim),
                },
                true,
            )
            .expect("the enemy's tether is admitted");
        }
    }
}

/// The stage the next cast belongs to: the number of stages whose shots were all taken.
fn staging_stage(staging: &Staging) -> usize {
    match staging.phase {
        Phase::Casting { stage, .. } => stage,
        _ => 0,
    }
}

/// Whether what stage `stage` casts is published.
fn stage_published(stage: usize, world: &BattleSession) -> bool {
    let snapshot = world.frames.current();
    match stage {
        0 => {
            snapshot
                .areas
                .iter()
                .filter(|area| area.owner == VIEWER)
                .count()
                >= 2
        }
        1 => snapshot.links.iter().any(|link| link.owner == VIEWER),
        _ => {
            snapshot.areas.iter().any(|area| area.owner == ENEMY)
                && snapshot.links.iter().any(|link| link.owner == ENEMY)
        }
    }
}

/// The shot `index` of all shots, counted over the stages.
fn shot_at(index: usize) -> (usize, Shot) {
    let mut left = index;
    for (stage, shots) in STAGES.iter().enumerate() {
        if left < shots.len() {
            return (stage, shots[left]);
        }
        left -= shots.len();
    }
    panic!("no shot {index}");
}

/// Shots in all.
fn shot_count() -> usize {
    STAGES.iter().map(|shots| shots.len()).sum()
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

/// Projects a Bevy world point to window pixels.
fn project3(camera: (&Camera, &GlobalTransform), point: Vec3) -> [f32; 2] {
    let (camera, transform) = camera;
    let offset = camera
        .physical_viewport_rect()
        .map_or(Vec2::ZERO, |rect| rect.min.as_vec2());
    camera
        .world_to_viewport(transform, point)
        .map_or([-1.0, -1.0], |at| (at + offset).to_array())
}

/// The picture of `area` through `camera`: its rays and its ground samples against `fog`.
fn area_shot(
    world: &SimWorld,
    camera: (&Camera, &GlobalTransform),
    fog: &BattleFog,
    area: &sim_types::AreaView,
    look: String,
) -> AreaShot {
    let centre = Vec2::new(area.center[0], area.center[1]);
    let radius = area.radius;
    let rays = (0..RAYS)
        .map(|ray| {
            let direction = Vec2::from_angle(ray as f32 * std::f32::consts::TAU / RAYS as f32);
            (0..RAY_SAMPLES)
                .map(|sample| {
                    let share =
                        RAY_FROM + (RAY_TO - RAY_FROM) * sample as f32 / (RAY_SAMPLES - 1) as f32;
                    project(world, camera, centre + direction * radius * share)
                })
                .collect()
        })
        .collect();
    let mut ground = Vec::new();
    let step = 0.1;
    let reach = (radius / step) as i32;
    for row in -reach..=reach {
        for column in -reach..=reach {
            let offset = Vec2::new(column as f32, row as f32) * step;
            if offset.length() > radius * 0.9 {
                continue;
            }
            let point = centre + offset;
            let seen = fog.sees(point.x, point.y);
            let firm = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
                .iter()
                .all(|(dx, dy)| {
                    fog.sees(point.x + dx * FOG_MARGIN, point.y + dy * FOG_MARGIN) == seen
                });
            ground.push((project(world, camera, point), firm.then_some(seen)));
        }
    }
    AreaShot {
        look,
        owner: area.owner.0,
        centre: centre.to_array(),
        radius,
        rays,
        ground,
        centre_px: project(world, camera, centre),
    }
}

/// The look names of the areas each caster type's block names, by ability index.
fn look_of(world: &SimWorld, source: Option<sim_types::AbilitySource>) -> String {
    source
        .and_then(|source| {
            let arch = world.catalog.get(source.unit_type).ok()?;
            let name = &arch.abilities.get(usize::from(source.ability))?.name;
            arch.effects.abilities.get(name).cloned()
        })
        .unwrap_or_default()
}

/// Records what shot `shot` shows before it is captured.
fn record(
    staging: &mut Staging,
    shot: Shot,
    session: &BattleSession,
    fog: &BattleFog,
    looks: &AreaLooks,
    camera: (&Camera, &GlobalTransform),
) {
    let world = session.world().expect("a ready session has a world");
    let snapshot = session.frames.current();
    match shot {
        Shot::AreasCritical => {
            staging.report.areas = snapshot
                .areas
                .iter()
                .filter(|area| area.owner == VIEWER)
                .map(|area| area_shot(world, camera, fog, area, look_of(world, area.source)))
                .collect();
        }
        Shot::Tether => {
            let link = snapshot
                .links
                .iter()
                .find(|link| link.owner == VIEWER)
                .expect("the tether is published");
            let body = |id: EntityId| {
                let bubble = snapshot
                    .bubbles
                    .iter()
                    .find(|bubble| bubble.entity_id == id)
                    .expect("both ends are presented");
                let [x, y, altitude] = bubble.world_pos;
                (Vec3::new(x, altitude + bubble.radius, y), bubble.radius)
            };
            let (from, from_radius) = body(link.from);
            let (to, to_radius) = body(link.to);
            let (from_end, to_end) =
                client::effects::areas::tether_ends(from, from_radius, to, to_radius);
            staging.report.tether = Some(TetherShot {
                from_centre: project3(camera, from),
                to_centre: project3(camera, to),
                from_end: project3(camera, from_end),
                to_end: project3(camera, to_end),
            });
        }
        Shot::TetherBare => {
            // The fog stage's places, with every staged unit of the viewer's in sight.
            let visible = on_screen(world, camera);
            let around = staging.spots[2];
            let cliff = find_fog_edge(world, fog, around, true, &visible);
            staging.report.fog_cliff = cliff.is_some();
            staging.fog = Some(
                cliff
                    .or_else(|| find_fog_edge(world, fog, around, false, &visible))
                    .expect("the viewer's sight ends on screen"),
            );
        }
        Shot::FogCritical => {
            staging.report.fog_area = snapshot
                .areas
                .iter()
                .find(|area| area.owner == ENEMY)
                .map(|area| area_shot(world, camera, fog, area, look_of(world, area.source)));
            // The unseen cells round it, for the evidence overlay (V1-3).
            if let Some(area) = snapshot.areas.iter().find(|area| area.owner == ENEMY) {
                let centre = Vec2::new(area.center[0], area.center[1]).floor();
                for row in -5..=5 {
                    for column in -5..=5 {
                        let corner = centre + Vec2::new(column as f32, row as f32);
                        if !fog.sees(corner.x + 0.5, corner.y + 0.5) {
                            staging.report.fog_cells.push(
                                [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]
                                    .map(|offset| project(world, camera, corner + offset)),
                            );
                        }
                    }
                }
            }
            staging.report.fog_held = looks.held();
            staging.report.fog_links = snapshot.links.len();
            staging.report.fog_own_links = snapshot
                .links
                .iter()
                .filter(|link| link.owner == VIEWER)
                .count();
        }
        _ => {}
    }
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

/// Requests a screenshot of this frame, writing its crops without waiting for it.
fn capture_crops(
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

/// Moves to the next shot once shot `shot` is written, or ends the shots.
fn after_shot(world: &mut World, shot: usize) {
    let next = shot + 1;
    world.resource_scope(|world, mut staging: Mut<'_, Staging>| {
        world.resource_mut::<EffectView>().bare = false;
        if let Some(mut runtime) = world.get_resource_mut::<EffectRuntime>() {
            runtime.only = None;
        }
        if next == shot_count() {
            staging.phase = Phase::Draining;
            return;
        }
        let (stage, _) = shot_at(next);
        let (previous_stage, _) = shot_at(shot);
        if stage == previous_stage {
            prepare_shot(world, next);
            let now = world.resource::<Time<Virtual>>().elapsed_secs_f64();
            staging.phase = Phase::Settling {
                shot: next,
                frames: 0,
                at: now,
            };
            return;
        }
        world.resource_scope(|world, mut session: Mut<'_, BattleSession>| {
            let since = published_tick(&session);
            staging.phase = Phase::Casting { stage, since };
            let sim = session.world_mut().expect("a ready session has a world");
            stage_casts(&mut staging, sim);
            world.resource_mut::<LabState>().battle_paused = false;
        });
    });
}

/// Orders the evidence sequence's casts: both areas, then the tether, from the viewer's caster.
fn begin_sequence(staging: &Staging, sim: &mut SimWorld) {
    let caster = staging.caster.expect("spawned");
    // The areas in reverse order (the longest last-listed first), each on its own spot, so the
    // longest area's end falls inside the sequence (V1-1).
    let areas: Vec<(usize, u8)> = staging
        .abilities
        .iter()
        .filter(|(_, _, kind)| *kind == AbilityTargetKind::TargetPosition)
        .map(|(_, index, _)| *index)
        .enumerate()
        .collect();
    let unit = sim.entities.get_mut(&caster).expect("the caster stands");
    unit.energy = SimScalar::from_num(1000);
    for (order, (spot, index)) in areas.iter().rev().enumerate() {
        unit.issue_order(
            UnitOrder::CastAbility {
                ability_index: *index,
                target_pos: Some(sim_position(staging.spots[*spot])),
                target_entity: None,
            },
            order > 0,
        )
        .expect("the cast is admitted");
    }
    let (_, tether, _) = staging
        .abilities
        .iter()
        .find(|(_, _, kind)| *kind == AbilityTargetKind::TargetEntity)
        .cloned()
        .expect("a tether look");
    sim.entities
        .get_mut(&caster)
        .expect("the caster stands")
        .issue_order(
            UnitOrder::CastAbility {
                ability_index: tether,
                target_pos: None,
                target_entity: staging.victim,
            },
            true,
        )
        .expect("the tether is admitted");
    // V1-2: a second storm overlapping the first.
    if let Some(second) = staging.second_caster {
        let (_, storm, _) = staging
            .abilities
            .iter()
            .find(|(_, _, kind)| *kind == AbilityTargetKind::TargetPosition)
            .cloned()
            .expect("an area look");
        let unit = sim
            .entities
            .get_mut(&second)
            .expect("the second caster stands");
        unit.energy = SimScalar::from_num(1000);
        unit.issue_order(
            UnitOrder::CastAbility {
                ability_index: storm,
                target_pos: Some(sim_position(staging.spots[0] + Vec2::new(1.3, 0.4))),
                target_entity: None,
            },
            false,
        )
        .expect("the second storm is admitted");
    }
}

/// Sets the effect view of shot `index`.
fn prepare_shot(world: &mut World, index: usize) {
    let (_, shot) = shot_at(index);
    let critical = matches!(
        shot,
        Shot::AreasCritical | Shot::TetherCritical | Shot::FogCritical
    );
    let bare = matches!(shot, Shot::AreasBare | Shot::TetherBare | Shot::FogBare);
    world.resource_mut::<EffectView>().bare = bare;
    if let Some(mut runtime) = world.get_resource_mut::<EffectRuntime>() {
        runtime.only = critical.then_some(Priority::Critical);
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

/// Sets the staging up once the camera is still: the caster type, the spots, the units.
fn set_up(
    staging: &mut Staging,
    session: &mut BattleSession,
    fog: &BattleFog,
    camera: (&Camera, &GlobalTransform),
    target: Vec2,
) {
    let sim = session.world_mut().expect("a ready session has a world");
    let (kind, abilities) = caster_type(sim);
    staging.caster_type = Some(kind);
    staging.abilities = abilities;
    let visible = on_screen(sim, camera);
    let spots = find_spots(sim, fog, target, 4, 5.0, &visible);
    drop(visible);
    staging.caster = Some(spawn_still(sim, kind, VIEWER, spots[2]));
    // The victim, of the same unarmed type, stands within the tether's range of the caster.
    let victim_at = spots[2] + (spots[3] - spots[2]).normalize_or_zero() * 3.6;
    staging.victim = Some(spawn_still(sim, kind, ENEMY, victim_at));
    if staging.sequence_only {
        // V1-2: units of both sides in each area, alternating round a ring inside it, and a
        // second caster for a storm overlapping the first.
        for spot in &spots[..2] {
            for index in 0..2 * CROWD_PER_SIDE {
                let angle = index as f32 * std::f32::consts::TAU / (2 * CROWD_PER_SIDE) as f32;
                let owner = if index % 2 == 0 { VIEWER } else { ENEMY };
                let place = *spot + Vec2::from_angle(angle) * 0.85;
                if let Ok(id) = sim.spawn_unit(kind, owner, sim_position(place)) {
                    let unit = sim.entities.get_mut(&id).expect("just spawned");
                    unit.unpushable = true;
                    unit.issue_order(UnitOrder::HoldPosition, false)
                        .expect("hold admitted");
                }
            }
        }
        let beside = spots[2] + (spots[3] - spots[2]).normalize_or_zero().perp() * 1.6;
        staging.second_caster = Some(spawn_still(sim, kind, VIEWER, beside));
    }
    staging.spots = spots;
}

/// Advances the staging by one frame.
fn stage(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, Staging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    time: Res<'_, Time<Virtual>>,
    fog: Option<Res<'_, BattleFog>>,
    looks: Res<'_, AreaLooks>,
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
    let tick = published_tick(&session);
    let now = time.elapsed_secs_f64();
    let before = std::mem::discriminant(&staging.phase);
    staging.phase = match staging.phase {
        Phase::Boot if tick < 2 => Phase::Boot,
        Phase::Boot => Phase::Booting {
            still: 0,
            height: rts.look_at_y,
        },
        Phase::Booting { still, height } if still < STILL_CAMERA_FRAMES => {
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
        Phase::Booting { .. } => {
            set_up(
                &mut staging,
                &mut session,
                &fog,
                (camera, transform),
                rts.target,
            );
            commands.queue(rebuild_tables);
            lab.battle_paused = false;
            let sim = session.world_mut().expect("a ready session has a world");
            if staging.sequence_only {
                begin_sequence(&staging, sim);
                Phase::Sequence {
                    frame: 0,
                    since: now,
                }
            } else {
                staging.phase = Phase::Casting {
                    stage: 0,
                    since: tick,
                };
                stage_casts(&mut staging, sim);
                staging.phase
            }
        }
        Phase::Casting { stage, since } if !stage_published(stage, &session) => {
            assert!(
                tick < since + CAST_TIMEOUT_TICKS,
                "stage {stage}'s casts published nothing in {CAST_TIMEOUT_TICKS} ticks"
            );
            Phase::Casting { stage, since }
        }
        Phase::Casting { stage, .. } => {
            lab.battle_paused = true;
            let first: usize = STAGES[..stage].iter().map(|shots| shots.len()).sum();
            Phase::Settling {
                shot: first,
                frames: 0,
                at: now,
            }
        }
        Phase::Settling { shot, frames, at }
            if frames < SETTLE_FRAMES
                || now - at < SETTLE_SECONDS
                || idle.0.load(Ordering::Acquire) < IDLE_FRAMES =>
        {
            Phase::Settling {
                shot,
                frames: frames + 1,
                at,
            }
        }
        Phase::Settling { shot, .. } => {
            let (_, which) = shot_at(shot);
            record(
                &mut staging,
                which,
                &session,
                &fog,
                &looks,
                (camera, transform),
            );
            let path = staging.directory.join(which.file());
            capture(&mut commands, path, move |world: &mut World| {
                after_shot(world, shot);
            });
            Phase::Capturing
        }
        Phase::Capturing => Phase::Capturing,
        Phase::Sequence { frame, since } if now - since < SEQUENCE_SECONDS => {
            let crops = sequence_frame(&mut staging, &session, (camera, transform), now, frame);
            if !crops.is_empty() {
                let outstanding = Arc::clone(&staging.outstanding);
                capture_crops(&mut commands, crops, &outstanding);
            }
            Phase::Sequence {
                frame: frame + 1,
                since,
            }
        }
        Phase::Sequence { .. } => Phase::Draining,
        Phase::Draining if staging.outstanding.load(Ordering::SeqCst) > 0 => Phase::Draining,
        Phase::Draining => {
            let text = ron::to_string(&staging.report).expect("plain data");
            std::fs::write(staging.directory.join(REPORT_FILE), text)
                .expect("the parent's directory is writable");
            commands.send_event(AppExit::Success);
            Phase::Done
        }
        Phase::Done => Phase::Done,
    };
    if std::mem::discriminant(&staging.phase) != before {
        eprintln!("area_link: {:?} at tick {tick}, {now:.2} s", staging.phase);
    }
}

/// Adds `file` to the sequence `key` of the report.
fn note(report: &mut Report, key: &str, file: String) {
    match report.sequences.iter_mut().find(|(look, _)| look == key) {
        Some((_, files)) => files.push(file),
        None => report.sequences.push((key.to_owned(), vec![file])),
    }
}

/// The crops the evidence child writes this frame (V1-1, V1-2): every frame of each of the
/// viewer's areas from its publication for [`COME_IN_SECONDS`], from [`BEFORE_END_SECONDS`]
/// before its end to [`AFTER_END_SECONDS`] after it, and of the tether while it is published
/// and [`AFTER_END_SECONDS`] after; the whole window every [`BATTLE_EVERY`] frames and at
/// [`CROWD_FRAME`].
fn sequence_frame(
    staging: &mut Staging,
    session: &BattleSession,
    camera: (&Camera, &GlobalTransform),
    now: f64,
    frame: u32,
) -> Vec<(PathBuf, [u32; 4])> {
    let sim = session.world().expect("a ready session has a world");
    let snapshot = session.frames.current();
    let mut wanted: Vec<(String, Vec2)> = Vec::new();
    for area in snapshot.areas.iter().filter(|area| area.owner == VIEWER) {
        let index = match staging
            .watches
            .iter()
            .position(|watch| watch.id == Some(area.id))
        {
            Some(index) => index,
            None => {
                let look = look_of(sim, area.source);
                let count = staging
                    .watches
                    .iter()
                    .filter(|watch| watch.key.starts_with(&look))
                    .count();
                staging.watches.push(Watch {
                    key: format!("{look}_{count}"),
                    id: Some(area.id),
                    at: Vec2::new(area.center[0], area.center[1]),
                    first: now,
                    ..Watch::default()
                });
                staging.watches.len() - 1
            }
        };
        let watch = &mut staging.watches[index];
        if f64::from(area.remaining_ticks) * 0.04 <= BEFORE_END_SECONDS {
            watch.ending.get_or_insert(now);
        }
        if now - watch.first <= COME_IN_SECONDS {
            wanted.push((format!("{}_in", watch.key), watch.at));
        }
        if watch.ending.is_some() {
            wanted.push((format!("{}_out", watch.key), watch.at));
        }
    }
    let tether = snapshot.links.iter().any(|link| link.owner == VIEWER);
    if tether && !staging.watches.iter().any(|watch| watch.key == "tether") {
        staging.watches.push(Watch {
            key: "tether".to_owned(),
            at: staging.spots[2],
            first: now,
            ..Watch::default()
        });
    }
    for watch in &mut staging.watches {
        let published = match watch.id {
            Some(id) => snapshot.areas.iter().any(|area| area.id == id),
            None => tether,
        };
        if published {
            if watch.id.is_none() {
                wanted.push(("tether".to_owned(), watch.at));
            }
            continue;
        }
        let gone = *watch.gone.get_or_insert(now);
        if now - gone <= AFTER_END_SECONDS {
            let key = if watch.id.is_some() {
                format!("{}_out", watch.key)
            } else {
                "tether".to_owned()
            };
            wanted.push((key, watch.at));
        }
    }
    let mut crops = Vec::new();
    for (key, at) in wanted {
        let [x, y] = project(sim, camera, at);
        let file = format!("seq_{key}_{frame:03}.png");
        let left = (x - CROP[0] as f32 / 2.0).max(0.0) as u32;
        let top = (y - CROP[1] as f32 / 2.0).max(0.0) as u32;
        crops.push((staging.directory.join(&file), [left, top, CROP[0], CROP[1]]));
        note(&mut staging.report, &key, file);
    }
    let whole = [0, 0, u32::MAX / 4, u32::MAX / 4];
    if frame.is_multiple_of(BATTLE_EVERY) {
        let file = format!("seq_battle_{frame:03}.png");
        crops.push((staging.directory.join(&file), whole));
        note(&mut staging.report, "battle", file);
    }
    if frame == CROWD_FRAME {
        let file = format!("seq_crowd_{frame:03}.png");
        crops.push((staging.directory.join(&file), whole));
        note(&mut staging.report, "crowd", file);
        staging.report.crowd = staging.spots[..2]
            .iter()
            .map(|spot| project(sim, camera, *spot))
            .collect();
    }
    crops
}

/// Boots the production client on the skirmish with the staging.
fn run_child(staging: Staging) {
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
    app.insert_resource(TimeUpdateStrategy::ManualDuration(FRAME_STEP))
        .insert_resource(idle)
        .insert_resource(staging)
        .init_resource::<EffectView>()
        .add_systems(Update, stage.in_set(BattleSet::Present))
        .add_systems(
            PostUpdate,
            apply_effect_view.after(client::effects::systems::build_effect_frame),
        )
        .add_systems(Last, finish_capture);
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// The child of [`areas_and_tethers_are_drawn_to_their_published_extent_and_only_where_seen`].
#[test]
#[ignore = "launched by the test below on a virtual display"]
fn area_link_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    run_child(Staging {
        directory: PathBuf::from(directory),
        ..Staging::default()
    });
}

/// The evidence child of the same test: the looks cast at once and pictured as sequences.
#[test]
#[ignore = "launched by the test below on a virtual display"]
fn area_link_sequence_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    run_child(Staging {
        directory: PathBuf::from(directory),
        sequence_only: true,
        ..Staging::default()
    });
}

/// Runs child `name` of this binary on the GPU harness with `directory` as its output; returns
/// its report.
fn run_gpu_child(name: &str, directory: &Path) -> TestResult<Report> {
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", name]);
    command.env(CHILD_OUT_VAR, directory);
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation(name);
    Ok(ron::from_str(&std::fs::read_to_string(
        directory.join(REPORT_FILE),
    )?)?)
}

/// The largest channel change between two pixels.
fn change(a: [u8; 3], b: [u8; 3]) -> u8 {
    (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0)
}

/// Whether window pixel `at` differs between `a` and `b` (off the window: no).
fn changed_at(a: &image::RgbImage, b: &image::RgbImage, at: [f32; 2]) -> bool {
    let (x, y) = (at[0].round(), at[1].round());
    if x < 0.0 || y < 0.0 || x >= a.width() as f32 || y >= a.height() as f32 {
        return false;
    }
    change(
        a.get_pixel(x as u32, y as u32).0,
        b.get_pixel(x as u32, y as u32).0,
    ) >= CHANGE_LEVEL
}

/// Pixel distance between two window points.
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    Vec2::from_array(a).distance(Vec2::from_array(b))
}

/// For each ray of `area`, the pixel distance (signed: outside positive) from the projected
/// radius to the outermost sample the `Critical` layers change.
fn edge_errors(area: &AreaShot, critical: &image::RgbImage, bare: &image::RgbImage) -> Vec<f32> {
    let at_radius =
        ((1.0 - RAY_FROM) / (RAY_TO - RAY_FROM) * (RAY_SAMPLES - 1) as f32).round() as usize;
    area.rays
        .iter()
        .map(|ray| {
            // The area's own footprint: from the innermost sample outwards, the changed run (gaps
            // of up to EDGE_GAP samples, the breakup's holes, bridged); what lies beyond a
            // longer gap belongs to something else.
            let mut outermost = 0;
            let mut gap = 0;
            for (index, at) in ray.iter().enumerate() {
                if changed_at(critical, bare, *at) {
                    outermost = index;
                    gap = 0;
                } else {
                    gap += 1;
                    if gap > EDGE_GAP && index > at_radius {
                        break;
                    }
                }
            }
            let error = distance(ray[outermost], ray[at_radius]);
            if outermost >= at_radius {
                error
            } else {
                -error
            }
        })
        .collect()
}

/// Copies the pictures and builds the contact sheets in `directory`, then into the evidence
/// directory when one is named.
fn keep_evidence(directory: &Path, report: &Report, driver: &str) -> TestResult {
    for (look, files) in &report.sequences {
        let pictures: Vec<image::RgbImage> = files
            .iter()
            .step_by(1)
            .filter_map(|file| {
                image::open(directory.join(file))
                    .ok()
                    .map(|image| image.to_rgb8())
            })
            .collect();
        let Some(first) = pictures.first() else {
            continue;
        };
        let (width, height) = (first.width(), first.height());
        let columns = if look == "battle" || look == "crowd" {
            2
        } else {
            6
        };
        let rows = pictures.len().div_ceil(columns) as u32;
        let mut sheet = image::RgbImage::new(width * columns as u32, height * rows);
        for (index, picture) in pictures.iter().enumerate() {
            let (column, row) = ((index % columns) as u32, (index / columns) as u32);
            image::imageops::replace(
                &mut sheet,
                picture,
                i64::from(column * width),
                i64::from(row * height),
            );
        }
        sheet.save(directory.join(format!("sheet_{look}_{driver}.png")))?;
    }
    // V1-2: the crowd at 2x round each area.
    if let Some(file) = report
        .sequences
        .iter()
        .find(|(look, _)| look == "crowd")
        .and_then(|(_, files)| files.first())
    {
        let picture = image::open(directory.join(file))?.to_rgb8();
        for (index, [x, y]) in report.crowd.iter().enumerate() {
            let left = (*x - CROWD_CROP[0] as f32 / 2.0).max(0.0) as u32;
            let top = (*y - CROWD_CROP[1] as f32 / 2.0).max(0.0) as u32;
            let crop = image::imageops::crop_imm(&picture, left, top, CROWD_CROP[0], CROWD_CROP[1])
                .to_image();
            image::imageops::resize(
                &crop,
                CROWD_CROP[0] * 2,
                CROWD_CROP[1] * 2,
                image::imageops::FilterType::Nearest,
            )
            .save(directory.join(format!("sheet_crowd2x_{index}_{driver}.png")))?;
        }
    }
    // V1-3: the fog shot with the viewer's unseen cells darkened.
    if !report.fog_cells.is_empty() {
        let mut picture = image::open(directory.join(Shot::Fog.file()))?.to_rgb8();
        for quad in &report.fog_cells {
            darken_quad(&mut picture, quad);
        }
        picture.save(directory.join(format!("sheet_fogcells_{driver}.png")))?;
    }
    let Some(target) = std::env::var_os(EVIDENCE_VAR) else {
        return Ok(());
    };
    let target = PathBuf::from(target);
    std::fs::create_dir_all(&target)?;
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        if name.starts_with("sheet_") || name.starts_with("shot_") {
            let kept = if name.starts_with("shot_") {
                name.replace("shot_", &format!("shot_{driver}_"))
            } else {
                name
            };
            std::fs::copy(&path, target.join(kept))?;
        }
    }
    Ok(())
}

/// Halves the brightness of the pixels of `picture` inside the convex quad `quad` (window
/// pixels, in order round it): the evidence overlay of an unseen cell (V1-3).
fn darken_quad(picture: &mut image::RgbImage, quad: &[[f32; 2]; 4]) {
    let xs = quad.iter().map(|corner| corner[0]);
    let ys = quad.iter().map(|corner| corner[1]);
    let (low_x, high_x) = (
        xs.clone().fold(f32::MAX, f32::min),
        xs.fold(f32::MIN, f32::max),
    );
    let (low_y, high_y) = (
        ys.clone().fold(f32::MAX, f32::min),
        ys.fold(f32::MIN, f32::max),
    );
    if low_x < 0.0 || low_y < 0.0 {
        return;
    }
    let side = |a: [f32; 2], b: [f32; 2], p: [f32; 2]| {
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    };
    for y in low_y as u32..=(high_y as u32).min(picture.height() - 1) {
        for x in low_x as u32..=(high_x as u32).min(picture.width() - 1) {
            let point = [x as f32 + 0.5, y as f32 + 0.5];
            let sides: Vec<f32> = (0..4)
                .map(|index| side(quad[index], quad[(index + 1) % 4], point))
                .collect();
            if sides.iter().all(|value| *value >= 0.0) || sides.iter().all(|value| *value <= 0.0) {
                let pixel = picture.get_pixel_mut(x, y);
                pixel.0 = pixel.0.map(|channel| channel / 2);
            }
        }
    }
}

/// Luminance (Rec. 709 weights on the display values) of a pixel, 0 to 255.
fn luminance(pixel: [u8; 3]) -> f32 {
    0.2126 * f32::from(pixel[0]) + 0.7152 * f32::from(pixel[1]) + 0.0722 * f32::from(pixel[2])
}

/// The pixel of `picture` at window point `at`, if inside.
fn pixel_at(picture: &image::RgbImage, at: [f32; 2]) -> Option<[u8; 3]> {
    let (x, y) = (at[0].round(), at[1].round());
    (x >= 0.0 && y >= 0.0 && x < picture.width() as f32 && y < picture.height() as f32)
        .then(|| picture.get_pixel(x as u32, y as u32).0)
}

/// E1-5 for `area`: the median luminance change of the whole look against the bare floor in the
/// band [`EDGE_BAND_PX`] just inside the projected radius, over every ray; and the pixels the
/// whole look changes more than [`OUTSIDE_SPILL_PX`] outside it along the near half's rays.
fn edge_visibility(
    area: &AreaShot,
    whole: &image::RgbImage,
    bare: &image::RgbImage,
) -> (f32, usize) {
    let at_radius =
        ((1.0 - RAY_FROM) / (RAY_TO - RAY_FROM) * (RAY_SAMPLES - 1) as f32).round() as usize;
    // Spill is sought out to SPILL_REACH radii: farther lie the staged units at
    // SPOT_CLEARANCE and their shadows.
    let spill_end = ((SPILL_REACH - RAY_FROM) / (RAY_TO - RAY_FROM) * (RAY_SAMPLES - 1) as f32)
        .round() as usize;
    let mut band = Vec::new();
    let mut spill = 0;
    for ray in &area.rays {
        let edge = ray[at_radius];
        for (index, at) in ray.iter().enumerate() {
            let from_edge = distance(*at, edge);
            if index <= at_radius && from_edge <= EDGE_BAND_PX {
                if let (Some(a), Some(b)) = (pixel_at(whole, *at), pixel_at(bare, *at)) {
                    band.push((luminance(a) - luminance(b)).abs());
                }
            }
            // The near half: rays running down the window, towards the camera.
            let near = ray[RAY_SAMPLES - 1][1] > ray[0][1];
            if near
                && index > at_radius
                && index <= spill_end
                && from_edge > OUTSIDE_SPILL_PX
                && changed_at(whole, bare, *at)
            {
                spill += 1;
            }
        }
    }
    band.sort_by(f32::total_cmp);
    let median = band.get(band.len() / 2).copied().unwrap_or(0.0);
    (median, spill)
}

#[test]
fn areas_and_tethers_are_drawn_to_their_published_extent_and_only_where_seen() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let driver = gpu_run::selected_driver().driver;
    let sequences = tempfile::tempdir()?;
    let sequence_report = run_gpu_child("area_link_sequence_child", sequences.path())?;
    keep_evidence(sequences.path(), &sequence_report, &driver.to_string())?;
    let directory = tempfile::tempdir()?;
    let report = run_gpu_child("area_link_child", directory.path())?;
    keep_evidence(directory.path(), &report, &driver.to_string())?;
    let shot = |shot: Shot| -> TestResult<image::RgbImage> {
        Ok(image::open(directory.path().join(shot.file()))?.to_rgb8())
    };

    // Areas: the edge on the radius.
    assert!(
        report.areas.len() >= 2,
        "both area looks were pictured: {report:?}"
    );
    let (critical, bare) = (shot(Shot::AreasCritical)?, shot(Shot::AreasBare)?);
    let whole = shot(Shot::Areas)?;
    let mut looks = BTreeMap::new();
    for area in &report.areas {
        let errors = edge_errors(area, &critical, &bare);
        let worst = errors
            .iter()
            .fold(0.0_f32, |worst, error| worst.max(error.abs()));
        let radius_px =
            distance(area.rays[0][0], area.rays[0][RAY_SAMPLES - 1]) / (RAY_TO - RAY_FROM);
        eprintln!(
            "driver {driver}: area '{}' of radius {} ({radius_px:.1} px): edge errors per ray \
             (px, outside positive) {errors:.1?}; worst {worst:.2}",
            area.look, area.radius
        );
        assert!(
            worst <= EDGE_TOLERANCE_PX,
            "'{}': the Critical edge lies on AreaRadius within {EDGE_TOLERANCE_PX} px: {errors:?}",
            area.look
        );
        looks.insert(area.look.clone(), worst);
        // E1-5: the edge can be seen, and nothing of the look spills past it on the ground.
        let (contrast, spill) = edge_visibility(area, &whole, &bare);
        // A1-4: the display colours at the centre, mid radius and edge, against the bare floor.
        let near = area
            .rays
            .iter()
            .max_by(|a, b| {
                (a[RAY_SAMPLES - 1][1] - a[0][1]).total_cmp(&(b[RAY_SAMPLES - 1][1] - b[0][1]))
            })
            .expect("rays");
        let edge_index =
            ((0.95 - RAY_FROM) / (RAY_TO - RAY_FROM) * (RAY_SAMPLES - 1) as f32).round() as usize;
        let colour = |at: [f32; 2]| (pixel_at(&whole, at), pixel_at(&bare, at));
        eprintln!(
            "driver {driver}: area '{}': edge band median luminance change {contrast:.1} levels \
             (least {EDGE_CONTRAST}); {spill} pixels changed beyond {OUTSIDE_SPILL_PX} px outside \
             on the near half; sRGB (look, bare floor) at the centre {:?}, mid radius {:?}, 0.95 \
             radius {:?}",
            area.look,
            colour(area.centre_px),
            colour(near[0]),
            colour(near[edge_index])
        );
        assert!(
            contrast >= EDGE_CONTRAST,
            "'{}': the edge can be seen: {contrast}",
            area.look
        );
        assert_eq!(spill, 0, "'{}': nothing spills past the edge", area.look);
    }
    assert!(looks.len() >= 2, "two different area looks: {looks:?}");

    // The tether: its ends on both bubbles.
    let tether = report.tether.clone().expect("the tether was pictured");
    let (with, without) = (shot(Shot::TetherCritical)?, shot(Shot::TetherBare)?);
    let line = Vec2::from_array(tether.to_centre) - Vec2::from_array(tether.from_centre);
    let steps = line.length().ceil() as usize;
    let along = |index: usize| {
        (Vec2::from_array(tether.from_centre) + line * index as f32 / steps as f32).to_array()
    };
    let changed: Vec<usize> = (0..=steps)
        .filter(|index| changed_at(&with, &without, along(*index)))
        .collect();
    let (Some(first), Some(last)) = (changed.first(), changed.last()) else {
        panic!("the tether changes pixels between the bodies: {tether:?}");
    };
    let (start_error, end_error) = (
        distance(along(*first), tether.from_end),
        distance(along(*last), tether.to_end),
    );
    eprintln!(
        "driver {driver}: tether from {:?} to {:?}: drawn from {:?} to {:?}; end errors \
         {start_error:.2} px and {end_error:.2} px; {} of {} pixels along it changed",
        tether.from_end,
        tether.to_end,
        along(*first),
        along(*last),
        changed.len(),
        steps + 1
    );
    // A1-4: the tongue's display colour on its centre line and at its edge, at the middle.
    let middle = along(steps / 2);
    let across = Vec2::from_array(line.to_array()).perp().normalize_or_zero();
    let tether_whole = shot(Shot::Tether)?;
    let mut edge_px = 0.0;
    while edge_px < 20.0
        && changed_at(
            &tether_whole,
            &without,
            (Vec2::from_array(middle) + across * (edge_px + 1.0)).to_array(),
        )
    {
        edge_px += 1.0;
    }
    eprintln!(
        "driver {driver}: tether sRGB at its middle: centre line {:?}, edge ({edge_px} px off the \
         line) {:?}, bare floor {:?}",
        pixel_at(&tether_whole, middle),
        pixel_at(
            &tether_whole,
            (Vec2::from_array(middle) + across * edge_px).to_array()
        ),
        pixel_at(&without, middle)
    );
    assert!(
        start_error <= EDGE_TOLERANCE_PX,
        "the tether starts on its caster's bubble"
    );
    assert!(
        end_error <= EDGE_TOLERANCE_PX,
        "the tether ends on its victim's bubble"
    );

    // Fog: the enemy's storm only where seen; its tether, whose caster is hidden, not at all.
    let fog_area = report
        .fog_area
        .clone()
        .expect("the enemy's area was published");
    let (fog_critical, fog_bare) = (shot(Shot::FogCritical)?, shot(Shot::FogBare)?);
    let (mut seen, mut seen_changed, mut unseen, mut unseen_changed) = (0, 0, 0, 0);
    for (at, sight) in &fog_area.ground {
        match sight {
            Some(true) => {
                seen += 1;
                seen_changed += usize::from(changed_at(&fog_critical, &fog_bare, *at));
            }
            Some(false) => {
                unseen += 1;
                unseen_changed += usize::from(changed_at(&fog_critical, &fog_bare, *at));
            }
            None => {}
        }
    }
    let full = shot(Shot::Fog)?;
    let unseen_full = fog_area
        .ground
        .iter()
        .filter(|(at, sight)| *sight == Some(false) && changed_at(&full, &fog_bare, *at))
        .count();
    let seen_full = fog_area
        .ground
        .iter()
        .filter(|(at, sight)| *sight == Some(true) && changed_at(&full, &fog_bare, *at))
        .count();
    eprintln!(
        "driver {driver}: enemy area across the fog edge: Critical layers change {seen_changed} of \
         {seen} seen ground samples and {unseen_changed} of {unseen} unseen ones; the whole look \
         changes {seen_full} seen and {unseen_full} unseen (cards standing in sight over ground \
         beyond it); looks held {:?} with {} tethers published, {} of them the viewer's",
        report.fog_held, report.fog_links, report.fog_own_links
    );
    eprintln!(
        "driver {driver}: fog edge along a change of ground level (a cliff): {}",
        report.fog_cliff
    );
    assert!(
        seen > 20 && unseen > 20,
        "the area lies across the fog edge: {seen} / {unseen}"
    );
    assert_eq!(
        unseen_changed, 0,
        "nothing of an enemy's area is drawn where the viewer does not see"
    );
    // The cut fails closed: a straight cut across the fog's staircase of cells gives up the seen
    // ground nearest the edge (Amendment 20), and the Critical floor is faint inside the rim, so
    // the seen side is judged on the whole look, a quarter at least.
    assert!(
        seen_full * 4 > seen,
        "the enemy's area is drawn where the viewer sees it"
    );
    assert!(
        report.fog_links > report.fog_own_links,
        "the enemy's tether was published"
    );
    assert_eq!(
        report.fog_held.1, report.fog_own_links,
        "only the viewer's own tethers are drawn, none whose ends the viewer does not see"
    );
    Ok(())
}

/// Unit check of the shot list.
#[test]
fn every_shot_belongs_to_one_stage() {
    assert_eq!(shot_count(), 9);
    assert_eq!(shot_at(0), (0, Shot::Areas));
    assert_eq!(shot_at(3), (1, Shot::Tether));
    assert_eq!(shot_at(8), (2, Shot::FogBare));
}
