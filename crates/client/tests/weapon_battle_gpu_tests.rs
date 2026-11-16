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
//! - **The approach wave** fires an instant shot and a blow from above and below the target on
//!   screen, and an anti-air weapon at a flyer.
//! - **The last waves, `cross_plane`** (plan `2026-11-17-effect-snapshot-identity` WP4, B-125,
//!   and Amendment 3, B-128), fire the anti-air weapon at a flyer hovering over a packed group
//!   of three ground units, and then, in a wave of its own, the first anti-air weapon whose
//!   shots fly (a projectile), when the data offers one with looks on a presentable shooter.
//!   Each impact must show beside the **flyer's** glass by the rule below, and change beside the
//!   ground units no more than its blank run does, plus [`UNDER_ALLOWANCE`]: the struck bubble
//!   is found by the struck entity's id, never by where the hit was published.
//!
//! The units first stand unarmed for [`SETTLE_SECONDS`] and are pictured: the baseline. Then
//! they open fire. The parent checks that each staged family's looks were spawned while its
//! wave ran; that, against the baseline and outside every bubble's interior, they changed at
//! least [`SEEN_PIXELS`] round the path and the target; and that its impact, from its birth,
//! changed beside the struck bubble on the attacker's side at least twice what the same
//! subject changes there in a blank run of the wave (every impact drawn as nothing), plus
//! [`IMPACT_MARGIN`] (reviews 2 to 4, E2-2, E2-3, E3-1, E4-1: a look drawn inside the struck
//! bubble, or not at all, fails). It
//! prints per family what it saw, and with
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
use client::bubble_shader::BubbleInstance;
use client::effects::catalog::{EffectCatalog, UnitEffectTables};
use client::effects::lab::driver::track_render_idle;
use client::effects::lab::LabRenderIdle;
use client::effects::runtime::EffectRuntime;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::scene_controller::LiveBattleBubble;
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
/// Environment variable naming, comma-separated, effects the child draws as nothing (a copy of
/// the vocabulary with each replaced by an invisible card): a wave's blank run (review 4,
/// E4-1).
const PLANT_VAR: &str = "SPACE_SCUFFLE_WEAPON_BATTLE_PLANT";
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
/// World units beyond each end of a spot's widest pair at which the ground is probed: with the
/// probe's own half unit round it ([`walkable`]), the ground a unit's body of radius up to 1
/// covers there is on the map and walkable.
const BODY_REACH: f32 = 0.5;
/// A pixel counts as changed when one of its channels moves by at least this many levels.
const SEEN_LEVEL: u8 = 32;
/// Pixels round a subject's path and target, outside both bubbles, its looks must change at
/// some frame against the baseline (review 2, E2-3).
const SEEN_PIXELS: usize = 30;
/// Pixels beside the struck bubble on the attacker's side, outside every bubble's interior,
/// by which a subject's impact must exceed twice what its blank run leaves there (review 4,
/// E4-1: the units' animations, the attacker's rim turning as it strikes among them, change
/// those pixels on both runs; a blank melee impact left up to 33 there, on the blow from below).
const IMPACT_MARGIN: usize = 20;
/// Seconds the staged units stand, unarmed, before the baseline picture: they appear and
/// settle (review 2, E2-3).
const SETTLE_SECONDS: f64 = 0.4;
/// World units beyond the struck bubble's radius in which its impact is looked for.
const IMPACT_ZONE: f32 = 0.7;
/// World units either side of a shot's line left out of its impact's count: the shot's own
/// body ends there.
const PATH_HALF: f32 = 0.15;
/// Frames from an impact's birth in which it is looked for.
const IMPACT_FRAMES: usize = 4;
/// Share of a bubble's drawn disc that is masked: its interior, where its unit's flipbook
/// animates. The band of glass round it is measured, because a hit is drawn there (a hit on
/// the far side of a bubble lies over its upper rim on screen).
const MASK_SHARE: f32 = 0.8;
/// Frames of a family's sheet at 1:1 and, of those, at 2x.
const SHEET_FRAMES: (usize, usize) = (12, 6);
/// World units between the bubbles of the ground units packed under the cross-plane wave's
/// flyer: they nearly touch.
const UNDER_GAP: f32 = 0.05;
/// Pixels beside the ground units under the cross-plane wave's flyer by which its impact may
/// change more than its blank run changes there (plan `2026-11-17-effect-snapshot-identity`
/// WP4). The two runs stage the same battle frame for frame and differ only by the impact, so
/// what the impact draws there is the whole difference (on radv both counted 7 px). The
/// allowance covers a faint glow of the flyer's own impact spilling past its zone, and is far
/// below the [`IMPACT_MARGIN`] an impact must show by: an impact drawn on a ground unit's glass,
/// as the removed position match could draw it, changes there what it changes beside the flyer
/// (272 px on radv).
const UNDER_ALLOWANCE: usize = 8;

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

/// Where a subject's shooter stands from its target (review 3, A3-1, A3-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum Approach {
    /// On the target's left on screen.
    #[default]
    Across,
    /// Above the target on screen: on the side away from the camera.
    Above,
    /// Below the target on screen: on the side towards the camera.
    Below,
    /// On the target's left, the target a flyer.
    AtFlyer,
    /// On the target's left, the target a flyer hovering over a packed group of ground units
    /// (plan `2026-11-17-effect-snapshot-identity` WP4, B-125: the cross-plane case).
    CrossPlane,
}

/// A planned subject: a shooter, the family it fires, whether that family is its own, and
/// where the shooter stands.
type Planned = (Shooter, Family, bool, Approach);

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
                .map(|(shooter, family)| (*shooter, family.clone(), true, Approach::Across))
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
        wave.push((shooter, family, false, Approach::Across));
    }
    waves.extend(
        approach_subjects(world, &native)
            .chunks(capacity)
            .map(<[Planned]>::to_vec),
    );
    waves.extend(cross_plane_waves(world));
    (waves, unstaged)
}

/// The first presentable mobile unit whose block names looks for its anti-air weapon, and
/// that family.
fn anti_air_shooter(world: &SimWorld) -> Option<(Shooter, Family)> {
    anti_air_shooter_where(world, |_| true)
}

/// [`anti_air_shooter`] among the units whose anti-air weapon `admits`.
fn anti_air_shooter_where(
    world: &SimWorld,
    admits: impl Fn(&WeaponSpec) -> bool,
) -> Option<(Shooter, Family)> {
    let assets = assets();
    world.catalog.archetypes().find_map(|arch| {
        let entry = arch.effects.weapons.iter().find(|entry| {
            entry.weapon == WeaponSlot::Air
                && entry.mode.is_none()
                && (entry.travel.is_some() || entry.impact.is_some())
        })?;
        let spec = arch.air_weapon.as_ref().filter(|spec| admits(spec))?;
        (presentable(&assets, &arch.slug) && arch.max_speed > SimScalar::ZERO).then(|| {
            (
                Shooter {
                    unit_type: arch.unit_type_id,
                    slot: WeaponSlot::Air,
                },
                Family {
                    travel: entry.travel.clone(),
                    impact: entry.impact.clone(),
                    kind: spec.visual_kind,
                    flies: spec.projectile_speed.is_some(),
                },
            )
        })
    })
}

/// The last waves, `cross_plane` (plan `2026-11-17-effect-snapshot-identity` WP4, B-125): the
/// anti-air shooter fires at a flyer hovering over a packed group of ground units. When the
/// first anti-air shooter whose shots fly is another unit, it fires in a wave of its own
/// (Amendment 3, B-128: a projectile lands on the flyer's glass, not below it); a wave of its
/// own, because each subject packs three ground units round its spot.
fn cross_plane_waves(world: &SimWorld) -> Vec<Vec<Planned>> {
    let instant = anti_air_shooter(world);
    let flying = anti_air_shooter_where(world, |spec| spec.projectile_speed.is_some())
        .filter(|(shooter, _)| instant.as_ref().is_none_or(|(first, _)| first != shooter));
    [instant, flying]
        .into_iter()
        .flatten()
        .map(|(shooter, family)| vec![(shooter, family, true, Approach::CrossPlane)])
        .collect()
}

/// The approach subjects (review 3, A3-1, A3-2): the first native instant shot and the first
/// native blow, each from above and from below the target on screen, and a shooter with an
/// anti-air weapon firing it at a flyer. They are staged in as many waves as the spots found
/// need, none left out.
fn approach_subjects(world: &SimWorld, native: &[(Shooter, Family)]) -> Vec<Planned> {
    let mut wave = Vec::new();
    for kind in [WeaponVisualKind::Hitscan, WeaponVisualKind::MeleeSlash] {
        if let Some((shooter, family)) = native.iter().find(|(_, family)| family.kind == kind) {
            for approach in [Approach::Above, Approach::Below] {
                wave.push((*shooter, family.clone(), true, approach));
            }
        }
    }
    if let Some((shooter, family)) = anti_air_shooter(world) {
        wave.push((shooter, family, true, Approach::AtFlyer));
    }
    wave
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
    /// Spawns of the impact look so far, per frame.
    impacts: Vec<u32>,
    /// The subject's pictures, one per frame.
    frames: Vec<String>,
    /// The picture with the units standing and no shot fired yet.
    base: String,
    /// Window pixel of the pictures' top left corner.
    crop: [u32; 2],
    /// The shooter's and the target's bubbles on screen: centre and radius, window pixels.
    shooter_px: [f32; 3],
    target_px: [f32; 3],
    /// Window pixels per world unit at the target.
    scale: f32,
    /// Where the shooter stood.
    approach: Approach,
    /// The visual kind of the weapon whose looks it fires, and whether its shots fly.
    kind: Option<WeaponVisualKind>,
    flies: bool,
    /// Every presented bubble's drawn disc at the baseline, `[x, y, radius]`, window pixels
    /// (review 3, E3-1: masked, the subject's own and its neighbours').
    masks: Vec<[f32; 3]>,
    /// Per frame, whether one of the impact look's instances is born and alive.
    impact_born: Vec<bool>,
    /// The drawn discs of the ground units under a cross-plane subject's flyer, `[x, y,
    /// radius]`, window pixels (plan `2026-11-17-effect-snapshot-identity` WP4).
    #[serde(default)]
    under_px: Vec<[f32; 3]>,
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
    /// The units are staged, unarmed, since `at`.
    Settling {
        at: f64,
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
    /// The shooters' weapons, held back while the baseline is pictured: `(type, weapon,
    /// ground weapon, air weapon)`.
    held_weapons: Vec<HeldWeapons>,
    outstanding: Arc<AtomicU32>,
    report: WaveReport,
}

/// A shooter type's weapons, held back while the baseline is pictured.
type HeldWeapons = (
    UnitTypeId,
    Option<WeaponSpec>,
    Option<WeaponSpec>,
    Option<WeaponSpec>,
);

/// A staged subject.
struct StagedSubject {
    units: [EntityId; 2],
    centre: Vec2,
    /// The shooter's and the target's places (map `x, y`) and radii.
    shooter: (Vec2, f32),
    target: (Vec2, f32),
    looks: [Option<(client::effects::catalog::EffectId, u32)>; 2],
    /// The ground units packed under a cross-plane subject's flyer.
    under: Vec<EntityId>,
}

/// Whether the ground under `point` and half a unit round it is walkable.
fn walkable(world: &SimWorld, point: Vec2) -> bool {
    let map = &world.map;
    [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .all(|(x, y): &(i32, i32)| {
            let probe = point + Vec2::new(*x as f32 * 0.5, *y as f32 * 0.5);
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
            // Open ground at both ends of the widest pair, across and up and down the screen
            // (the approach wave stands pairs along the map's `y`).
            // Each end is probed a body's reach further out, so no staged unit's disc runs off
            // the map's edge (a shooter of radius 0.6 at x = 0.54 broke the simulation's
            // movement domain).
            let reach = MOST_GAP * 0.5 + BODY_REACH;
            let sides = [-reach, reach].iter().all(|offset| {
                walkable(world, point + Vec2::new(*offset, 0.0))
                    && walkable(world, point + Vec2::new(0.0, *offset))
            });
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

/// A presentable mobile unit type, a flyer when `air` and a ground unit otherwise, that is no
/// shooter of `wave`: the targets' type, unarmed in the test world's catalog so the targets
/// fire on nothing.
fn target_type(world: &mut SimWorld, wave: &[Planned], air: bool) -> UnitTypeId {
    let shooters: Vec<UnitTypeId> = wave.iter().map(|(shooter, ..)| shooter.unit_type).collect();
    let mover = if air {
        MoverType::Air
    } else {
        MoverType::Ground
    };
    let id = world
        .catalog
        .archetypes()
        .filter(|arch| {
            arch.max_speed > SimScalar::ZERO
                && arch.mover_type == mover
                && arch.modes.is_empty()
                && !shooters.contains(&arch.unit_type_id)
                && presentable(&assets(), &arch.slug)
        })
        .max_by_key(|arch| arch.collision_radius)
        .map(|arch| arch.unit_type_id)
        .expect("a presentable unit of that plane is no shooter");
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
    for (shooter, family, ..) in wave.iter().filter(|(_, _, own, _)| !own) {
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
    up: Vec2,
) {
    let world = session.world_mut().expect("a ready session has a world");
    let (waves, unstaged) = plan(world, spots.len());
    staging.report.waves = waves.len();
    staging.report.unstaged = unstaged.iter().map(Family::label).collect();
    let Some(wave) = waves.get(staging.wave) else {
        return;
    };
    let ground_target = target_type(world, wave, false);
    let flyer_target = wave
        .iter()
        .any(|(.., approach)| matches!(approach, Approach::AtFlyer | Approach::CrossPlane))
        .then(|| target_type(world, wave, true));
    let ground_radius = world
        .catalog
        .get(ground_target)
        .map_or(0.5, |arch| arch.collision_radius.to_f32());
    lend(world, wave);
    for ((shooter, family, own, approach), spot) in wave.iter().zip(spots) {
        let target = match approach {
            Approach::AtFlyer | Approach::CrossPlane => flyer_target.unwrap_or(ground_target),
            _ => ground_target,
        };
        let target_radius = world
            .catalog
            .get(target)
            .map_or(0.5, |arch| arch.collision_radius.to_f32());
        let arch = world
            .catalog
            .get(shooter.unit_type)
            .expect("invariant: the planned shooter is of this catalog");
        let range = slot_weapon(arch, shooter.slot).map_or(1.0, |spec| spec.max_range.to_f32());
        let radii = arch.collision_radius.to_f32() + target_radius;
        let gap = (radii + range * 0.7).min(MOST_GAP).max(radii + 0.05);
        let slug = arch.slug.clone();
        let shooter_radius = arch.collision_radius.to_f32();
        // From the shooter to the target, on the map.
        let towards = match approach {
            Approach::Across | Approach::AtFlyer | Approach::CrossPlane => Vec2::X,
            Approach::Above => -up,
            Approach::Below => up,
        };
        let shooter_at = *spot - towards * gap * 0.5;
        let target_at = *spot + towards * gap * 0.5;
        let from = spawn_still(world, shooter.unit_type, VIEWER, shooter_at);
        let to = spawn_still(world, target, ENEMY, target_at);
        world.entities.get_mut(&to).expect("spawned").is_immobile = true;
        eprintln!(
            "weapon wave {}: {slug} ({from:?}) at {shooter_at:?} fires {} ({approach:?}) at \
             {to:?} at {target_at:?}",
            staging.wave,
            family.label()
        );
        // The cross-plane case (B-125): ground units packed under the flyer, one straight
        // under it and one either side up and down the screen, touching it.
        let under: Vec<EntityId> = if *approach == Approach::CrossPlane {
            let apart = 2.0 * ground_radius + UNDER_GAP;
            [0.0, apart, -apart]
                .iter()
                .map(|offset| {
                    let unit = spawn_still(world, ground_target, ENEMY, target_at + up * *offset);
                    world.entities.get_mut(&unit).expect("spawned").is_immobile = true;
                    unit
                })
                .collect()
        } else {
            Vec::new()
        };
        let look = |name: &Option<String>| {
            name.as_deref()
                .and_then(|name| catalog.effect_id(name))
                .map(|id| (id, runtime.spawned_of(id)))
        };
        staging.subjects.push(StagedSubject {
            units: [from, to],
            centre: *spot,
            shooter: (shooter_at, shooter_radius),
            target: (target_at, target_radius),
            looks: [look(&family.travel), look(&family.impact)],
            under,
        });
        staging.report.subjects.push(SubjectReport {
            family: family.label(),
            shooter: slug,
            own: *own,
            approach: *approach,
            kind: Some(family.kind),
            flies: family.flies,
            ..SubjectReport::default()
        });
    }
    // The shooters stand unarmed until the baseline is pictured (review 2, E2-3).
    let shooters: Vec<UnitTypeId> = wave.iter().map(|(shooter, ..)| shooter.unit_type).collect();
    for unit_type in shooters {
        if staging.held_weapons.iter().any(|held| held.0 == unit_type) {
            continue;
        }
        let archetype = world
            .catalog
            .get_mut(unit_type)
            .expect("invariant: the planned shooter is of this catalog");
        staging.held_weapons.push((
            unit_type,
            archetype.weapon.take(),
            archetype.ground_weapon.take(),
            archetype.air_weapon.take(),
        ));
    }
    session.publish();
}

/// Gives the shooters their weapons back and orders every attack.
fn open_fire(staging: &mut Staging, session: &mut BattleSession) {
    let world = session.world_mut().expect("a ready session has a world");
    for (unit_type, weapon, ground, air) in staging.held_weapons.drain(..) {
        let archetype = world
            .catalog
            .get_mut(unit_type)
            .expect("invariant: a held shooter is of this catalog");
        archetype.weapon = weapon;
        archetype.ground_weapon = ground;
        archetype.air_weapon = air;
    }
    for subject in &staging.subjects {
        let [from, to] = subject.units;
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

/// Pictures every subject's baseline and records where its bubbles stand on screen.
fn picture_base(
    commands: &mut Commands<'_, '_>,
    staging: &mut Staging,
    session: &BattleSession,
    camera: (&Camera, &GlobalTransform),
    bubbles: &[(EntityId, Vec3, f32)],
) {
    let world = session.world().expect("a ready session has a world");
    // A bubble's disc on screen: its drawn centre (review 3, E3-1) and radius.
    let disc = |(centre, radius): (Vec3, f32)| {
        let [cx, cy] = project3(camera, centre);
        let [ex, ey] = project3(camera, centre + Vec3::X * radius);
        [cx, cy, (ex - cx).hypot(ey - cy)]
    };
    let drawn = |id: EntityId| {
        bubbles
            .iter()
            .find(|(bubble, ..)| *bubble == id)
            .map(|(_, centre, radius)| disc((*centre, *radius)))
    };
    let masks: Vec<[f32; 3]> = bubbles
        .iter()
        .map(|(_, centre, radius)| disc((*centre, *radius)))
        .collect();
    let mut crops = Vec::new();
    for (index, subject) in staging.subjects.iter().enumerate() {
        let [x, y] = project(world, camera, subject.centre);
        let left = (x - CROP[0] as f32 / 2.0).max(0.0) as u32;
        let top = (y - CROP[1] as f32 * 0.6).max(0.0) as u32;
        let file = format!("wave{}_{index}_base.png", staging.wave);
        crops.push((staging.directory.join(&file), [left, top, CROP[0], CROP[1]]));
        let on_ground = |(at, radius): (Vec2, f32)| {
            let [cx, cy] = project(world, camera, at);
            let [ex, ey] = project(world, camera, at + Vec2::new(radius, 0.0));
            [cx, cy, (ex - cx).hypot(ey - cy)]
        };
        let report = &mut staging.report.subjects[index];
        report.base = file;
        report.crop = [left, top];
        report.shooter_px = drawn(subject.units[0]).unwrap_or_else(|| on_ground(subject.shooter));
        report.target_px = drawn(subject.units[1]).unwrap_or_else(|| on_ground(subject.target));
        report.scale = report.target_px[2] / subject.target.1.max(1e-3);
        report.masks.clone_from(&masks);
        report.under_px = subject.under.iter().filter_map(|id| drawn(*id)).collect();
    }
    capture_frame(commands, crops, &staging.outstanding);
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
fn count(staging: &mut Staging, runtime: &EffectRuntime, now: f64) {
    for (subject, report) in staging.subjects.iter().zip(&mut staging.report.subjects) {
        let spawned =
            |look: Option<(_, u32)>| look.map_or(0, |(id, at)| runtime.spawned_of(id) - at);
        report.travel_spawned = spawned(subject.looks[0]);
        report.impact_spawned = spawned(subject.looks[1]);
        report.impacts.push(report.impact_spawned);
        // Review 3, E3-1: an instant shot's impact waits for its slug, so the impact is looked
        // for from its birth, not from its spawn request.
        let born = subject.looks[1].is_some_and(|(id, _)| {
            runtime
                .pool
                .iter()
                .any(|(_, instance)| instance.effect == id && instance.birth <= now)
        });
        report.impact_born.push(born);
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
    (cameras, bubbles): (
        Query<'_, '_, (&Camera, &GlobalTransform, &RtsCamera)>,
        Query<'_, '_, &BubbleInstance, With<LiveBattleBubble>>,
    ),
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
            // The map direction that runs up the screen (away from the camera).
            let up = {
                let [_, y0] = project(world, (camera, transform), rts.target);
                let [_, y1] = project(world, (camera, transform), rts.target + Vec2::Y);
                if y1 < y0 {
                    Vec2::Y
                } else {
                    -Vec2::Y
                }
            };
            hold_own_units(&mut session);
            stage(&mut staging, &mut session, catalog, runtime, &spots, up);
            commands.queue(rebuild_tables);
            lab.battle_paused = false;
            Phase::Settling { at: now }
        }
        Phase::Settling { at } if now - at < SETTLE_SECONDS => {
            heal(&staging, &mut session);
            Phase::Settling { at }
        }
        Phase::Settling { .. } => {
            let drawn: Vec<(EntityId, Vec3, f32)> = bubbles
                .iter()
                .map(|bubble| {
                    let instance = &bubble.0;
                    let dome = matches!(instance.shape, sim_types::ShapeKind::BuildingDome { .. });
                    let [x, y, altitude] = instance.world_pos;
                    let lift = if dome { 0.0 } else { instance.radius };
                    (
                        instance.entity_id,
                        Vec3::new(x, altitude + lift, y),
                        instance.radius,
                    )
                })
                .collect();
            picture_base(
                &mut commands,
                &mut staging,
                &session,
                (camera, transform),
                &drawn,
            );
            open_fire(&mut staging, &mut session);
            if let Some(runtime) = effects.0.as_deref() {
                for subject in &mut staging.subjects {
                    for look in subject.looks.iter_mut().flatten() {
                        look.1 = runtime.spawned_of(look.0);
                    }
                }
            }
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
                count(&mut staging, runtime, now);
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
    if let Ok(planted) = std::env::var(PLANT_VAR) {
        let path = PathBuf::from(&directory).join("planted_effects.ron");
        planted_vocabulary(&planted, &path);
        app.insert_resource(client::effects::systems::EffectCatalogSource(path));
    }
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

/// Writes to `path` a copy of the shipped vocabulary in which each effect `planted` names
/// (comma-separated, each a `Point` effect) draws one card too small to be drawn.
fn planted_vocabulary(planted: &str, path: &Path) {
    let root = client::ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let shipped = root
        .as_path()
        .join(client::effects::catalog::EFFECTS_CATALOG_PATH);
    let mut text = std::fs::read_to_string(&shipped).expect("the shipped vocabulary reads");
    for name in planted.split(',') {
        let head = format!("        \"{name}\": (\n            class:");
        let start = text.find(&head).expect("a planted effect is defined");
        let end = start
            + text[start..]
                .find("\n        ),\n")
                .expect("the definition ends")
            + "\n        ),\n".len();
        let blank = format!(
            "        \"{name}\": (\n            class: Minor,\n            event: Point,\n            \
             layers: [\n                (block: Sprite(texture: \"droplet\", ramp: \"tracer_body\", \
             shape: Radial), size: (scale: 0.001), duration: 0.2),\n            ],\n        ),\n"
        );
        text.replace_range(start..end, &blank);
    }
    std::fs::write(path, text).expect("the child's directory is writable");
}

/// Runs the child of `wave`, with the effects `planted` names drawn as nothing; returns its
/// report and directory.
fn run_wave(wave: usize, planted: Option<&str>) -> TestResult<(WaveReport, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", "weapon_wave_child"]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.env(WAVE_VAR, wave.to_string());
    if let Some(planted) = planted {
        command.env(PLANT_VAR, planted);
    }
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("weapon wave");
    let report: WaveReport = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    Ok((report, directory))
}

/// Whether pixel `(x, y)` changed by [`SEEN_LEVEL`] between `a` and `b`.
fn changed(a: &image::RgbImage, b: &image::RgbImage, x: u32, y: u32) -> bool {
    a.get_pixel(x, y)
        .0
        .iter()
        .zip(b.get_pixel(x, y).0.iter())
        .any(|(a, b)| a.abs_diff(*b) >= SEEN_LEVEL)
}

/// What a subject's looks changed against its baseline, outside every bubble's interior.
#[derive(Debug, Clone, Copy, Default)]
struct Seen {
    /// The most pixels any frame changed round its path and its target (review 2, E2-2, E2-3).
    around: usize,
    /// The most its impact changed just outside the struck bubble on the attacker's side, in
    /// the [`IMPACT_FRAMES`] frames after each impact's birth.
    impact: usize,
    /// The most changed, in those frames, just outside the ground units under a cross-plane
    /// subject's flyer on the attacker's side, away from the flyer's own zone (plan
    /// `2026-11-17-effect-snapshot-identity` WP4); 0 for any other subject.
    under: usize,
}

/// [`Seen`] of `subject` from its pictures in `directory`.
fn seen_against_base(directory: &Path, subject: &SubjectReport) -> TestResult<Seen> {
    let base = image::open(directory.join(&subject.base))?.to_rgb8();
    let crop = Vec2::new(subject.crop[0] as f32, subject.crop[1] as f32);
    let shooter = Vec2::new(subject.shooter_px[0], subject.shooter_px[1]) - crop;
    let target = Vec2::new(subject.target_px[0], subject.target_px[1]) - crop;
    let target_r = subject.target_px[2] * MASK_SHARE;
    let zone = subject.target_px[2] + IMPACT_ZONE * subject.scale;
    let towards = (shooter - target).normalize_or_zero();
    // Every presented bubble's drawn disc is masked: the subject's two and its neighbours'
    // (review 3, E3-1).
    let masks: Vec<(Vec2, f32)> = subject
        .masks
        .iter()
        .map(|[x, y, radius]| (Vec2::new(*x, *y) - crop, radius * MASK_SHARE))
        .filter(|(centre, radius)| {
            centre.x + radius >= 0.0
                && centre.y + radius >= 0.0
                && centre.x - radius <= CROP[0] as f32
                && centre.y - radius <= CROP[1] as f32
        })
        .collect();
    // The impact is looked for from its birth (review 3, E3-1): in the frames where one of its
    // instances is born and alive, up to IMPACT_FRAMES after each birth.
    let impact_frames: Vec<usize> = subject
        .impact_born
        .iter()
        .enumerate()
        .filter(|(index, born)| **born && (*index == 0 || !subject.impact_born[index - 1]))
        .flat_map(|(index, _)| index..index + IMPACT_FRAMES)
        .filter(|index| subject.impact_born.get(*index).copied().unwrap_or(false))
        .collect();
    // PERFORMANCE RATIONALE:
    // Why: the masks hold every presented bubble, and the crop is measured on every frame.
    // What: each pixel is classed once (masked, round path or target, beside the struck
    // bubble), and the frames are read through that class.
    // Trade-off: one class buffer per subject.
    let (width, height) = (base.width(), base.height());
    let direction = (target - shooter).normalize_or_zero();
    let travels = !subject.family.starts_with("none+");
    // The ground units under a cross-plane subject's flyer: their centre and the zone beside
    // them an impact on their glass would change.
    let under: Vec<(Vec2, f32)> = subject
        .under_px
        .iter()
        .map(|[x, y, radius]| {
            (
                Vec2::new(*x, *y) - crop,
                radius + IMPACT_ZONE * subject.scale,
            )
        })
        .collect();
    let beside_under = |at: Vec2| {
        at.distance(target) > zone
            && under.iter().any(|(centre, reach)| {
                at.distance(*centre) <= *reach
                    && (at - *centre).normalize_or_zero().dot(towards) > -0.2
            })
    };
    // Per pixel: round the path or the target, beside the struck bubble, beside a ground unit
    // under a cross-plane flyer.
    let class: Vec<(u32, u32, bool, bool, bool)> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            let at = Vec2::new(x as f32, y as f32);
            if at.distance(target) <= target_r
                || masks
                    .iter()
                    .any(|(centre, radius)| at.distance(*centre) <= *radius)
            {
                return None;
            }
            let along = (at - shooter).dot(direction);
            let off = (at - shooter).perp_dot(direction).abs();
            let on_path = along >= 0.0 && along <= shooter.distance(target) && off <= zone;
            let near_target = at.distance(target) <= zone;
            let under = beside_under(at);
            (on_path || near_target || under).then_some((
                x,
                y,
                on_path || near_target,
                // Beside the struck bubble on the attacker's side, off the shot's own line (a
                // beam's or a slug's end lies on it).
                near_target
                    && (at - target).normalize_or_zero().dot(towards) > -0.2
                    && (!travels || off > PATH_HALF * subject.scale),
                under,
            ))
        })
        .collect();
    let mut seen = Seen::default();
    for (index, file) in subject.frames.iter().enumerate() {
        let frame = image::open(directory.join(file))?.to_rgb8();
        if frame.width() < width || frame.height() < height {
            continue;
        }
        let (mut around, mut impact, mut under) = (0, 0, 0);
        for (x, y, near, beside, below) in &class {
            if changed(&frame, &base, *x, *y) {
                around += usize::from(*near);
                impact += usize::from(*beside);
                under += usize::from(*below);
            }
        }
        seen.around = seen.around.max(around);
        if impact_frames.contains(&index) {
            seen.impact = seen.impact.max(impact);
            seen.under = seen.under.max(under);
        }
    }
    Ok(seen)
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
    // From two frames before the first impact's birth (the shots are under way by then).
    let start = subject
        .impact_born
        .iter()
        .position(|born| *born)
        .unwrap_or(0)
        .saturating_sub(2);
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

/// Measures one subject, prints what it saw, and returns its failures and its impact pixels.
fn judge(
    directory: &Path,
    subject: &SubjectReport,
    wave: usize,
    driver: &str,
) -> TestResult<(Vec<String>, Seen)> {
    let seen = seen_against_base(directory, subject)?;
    let (changed, impact_seen) = (seen.around, seen.impact);
    eprintln!(
        "driver {driver}: wave {wave} {} on {} ({}, {:?}): travel spawned {}, impact spawned {}; \
         against the units-only baseline, outside every bubble's interior, at most {changed} px round path \
         and target and, from the impact's birth, {impact_seen} px beside the struck bubble \
         changed by {SEEN_LEVEL} levels over {} frames",
        subject.family,
        subject.shooter,
        if subject.own { "its own" } else { "lent" },
        subject.approach,
        subject.travel_spawned,
        subject.impact_spawned,
        subject.frames.len()
    );
    let (travel, impact) = subject
        .family
        .split_once('+')
        .expect("invariant: a family's label joins its two looks with '+'");
    let mut failures = Vec::new();
    if travel != "none" && subject.travel_spawned == 0 {
        failures.push(format!("{}: its travel never spawned", subject.family));
    }
    if impact != "none" && subject.impact_spawned == 0 {
        failures.push(format!("{}: its impact never spawned", subject.family));
    }
    if changed < SEEN_PIXELS {
        failures.push(format!("{}: {changed} px changed", subject.family));
    }
    Ok((failures, seen))
}

/// The impact look of `subject`'s family, if it has one.
fn impact_of(subject: &SubjectReport) -> Option<&str> {
    subject
        .family
        .split_once('+')
        .map(|(_, impact)| impact)
        .filter(|impact| *impact != "none")
}

/// Review 1, V1-1; reviews 2 to 4 (E2-3, E3-1, A3-1, A3-2, E4-1): every family of weapon
/// looks fires in a staged battle at the default zoom, the presentable ones on their own units
/// and the lab ones lent to a real bubble, an instant shot and a blow from above and below the
/// target on screen, and an anti-air weapon at a flyer. Each wave is run twice, the second time
/// with every impact of the wave drawn as nothing (its blank): each subject's impact must change
/// beside the struck bubble at least twice what its own blank changes there, plus
/// [`IMPACT_MARGIN`], so what the units' animations change on both runs cancels.
#[test]
fn weapon_looks_fire_in_a_staged_battle() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let driver = gpu_run::selected_driver().driver.to_string();
    let evidence = std::env::var_os(EVIDENCE_VAR).map(PathBuf::from);
    if let Some(target) = &evidence {
        std::fs::create_dir_all(target)?;
    }
    let mut wave = 0;
    let mut failures = Vec::new();
    loop {
        let (report, directory) = run_wave(wave, None)?;
        assert!(
            !report.subjects.is_empty(),
            "wave {wave} staged no subject (no open spot was found)"
        );
        if wave == 0 {
            eprintln!(
                "driver {driver}: {} waves; lab families no presentable shooter carries: {:?}",
                report.waves, report.unstaged
            );
        }
        // The wave's blank: the same staging with every impact of the wave drawn as nothing.
        let mut impacts: Vec<&str> = report.subjects.iter().filter_map(impact_of).collect();
        impacts.sort_unstable();
        impacts.dedup();
        let (blank, blank_directory) = run_wave(wave, Some(&impacts.join(",")))?;
        for (index, subject) in report.subjects.iter().enumerate() {
            let (judged, seen) = judge(directory.path(), subject, wave, &driver)?;
            let impact_seen = seen.impact;
            failures.extend(judged);
            if impact_of(subject).is_some() {
                let blank_subject = blank
                    .subjects
                    .get(index)
                    .filter(|other| other.family == subject.family)
                    .ok_or("the blank run stages the same subjects")?;
                let blank = seen_against_base(blank_directory.path(), blank_subject)?;
                let blank_seen = blank.impact;
                let needed = 2 * blank_seen + IMPACT_MARGIN;
                // B-125 (plan `2026-11-17-effect-snapshot-identity` WP4): the flyer's impact is
                // drawn on the flyer, not on a ground unit under it.
                if subject.approach == Approach::CrossPlane {
                    eprintln!(
                        "driver {driver}: wave {wave} {} (CrossPlane): beside the {} ground \
                         units under the flyer, the impact changed {} px, its blank {} px \
                         (allowed {})",
                        subject.family,
                        subject.under_px.len(),
                        seen.under,
                        blank.under,
                        blank.under + UNDER_ALLOWANCE
                    );
                    if subject.under_px.is_empty() {
                        failures.push(format!(
                            "{} (CrossPlane): no ground unit was drawn under the flyer",
                            subject.family
                        ));
                    }
                    if seen.under > blank.under + UNDER_ALLOWANCE {
                        failures.push(format!(
                            "{} (CrossPlane): its impact changed {} px beside the ground units \
                             under the flyer, its blank {}",
                            subject.family, seen.under, blank.under
                        ));
                    }
                }
                eprintln!(
                    "driver {driver}: wave {wave} {} ({:?}): impact {impact_seen} px beside the \
                     struck bubble, its blank {blank_seen} px (needs {needed})",
                    subject.family, subject.approach
                );
                if impact_seen < needed {
                    failures.push(format!(
                        "{} ({:?}): its impact changed {impact_seen} px beside the struck \
                         bubble, its blank {blank_seen}",
                        subject.family, subject.approach
                    ));
                }
            }
            if let Some(target) = &evidence {
                write_sheet(
                    directory.path(),
                    subject,
                    target,
                    &format!(
                        "wp11r5_battle_{driver}_w{wave}_{index}_{:?}_{}.png",
                        subject.approach, subject.family
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
