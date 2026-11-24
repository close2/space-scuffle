//! Weapon looks in a staged battle on a real GPU (plan `2026-11-09-effect-visualization` WP11,
//! review 1, V1-1).
//!
//! # Responsibility
//! Proves the WP11 weapon looks where a player sees them: in the production client playing the
//! shipped skirmish, on the shipped floor, at the battle camera's default zoom, fired by real
//! units at real bubbles. Each child stages one wave of shooters, each across open ground from
//! an unarmed enemy target, and pictures every subject every 40 ms frame for
//! [`WAVE_FRAMES`] frames:
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
//!   is found by the struck entity's id, never by where the hit was published. The projectile's
//!   flight must start within [`LAUNCH_TOLERANCE_PX`] of the shooter's drawn bubble outline
//!   (Amendment 7, B-133: not below a flyer's bubble, where the simulation publishes it).
//!
//! The units first stand unarmed for [`SETTLE_FRAMES`] frames and are pictured: the baseline. Then
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
//! # Where the waves stand
//! Plan `2026-11-20-gpu-lane` D-3. The camera boots on the start base, beside the map's edge,
//! where the base's footprints and the ground off the map leave room for one subject (plan
//! §2.3). So before the spots are sought, each child pans the battle camera, without zooming,
//! to the nearest of a fixed set of candidate targets round the start at which
//! [`search_spots`] finds [`MAX_SUBJECTS`] spots ([`camera_target`]), and lets the camera and
//! the renderer settle again. The base's sight does not reach that far: at the candidates in
//! sight the probe of WP3 found at most 4 spots. So a spot may lie in the fog when it is
//! sought ([`Sight::OnceStaged`]): the wave's shooters, the viewer's own units, stand on it,
//! and at the baseline picture every subject's shooter and target must be in the viewer's
//! sight, or the child fails naming the subject, so nothing is judged under fog. A child that
//! finds fewer than [`MIN_SPOTS`] spots fails, counting the candidates each condition
//! rejected, so a change that crowds the waves fails instead of doubling the target's time.
//! Away from the base the ground has cliffs, so a spot must be [`level`]: a target below a
//! cliff's edge is hidden from the camera by the high ground. With several subjects a wave,
//! two may fire the same look, so each instance of a look is counted for the subject nearest
//! to it ([`owner`]), never for every subject that names the look.
//!
//! # How it fits
//! Each child re-executes this test binary through `support/gpu_run.rs` (charter §7, C21), as
//! `status_gpu_tests.rs` does. Every unit and look is chosen by catalog properties (media, a
//! weapon, a block's looks), never by name (C5).
//!
//! # Invariants verified
//! - The waves are planned from the catalog and the spots alone, so parent and child agree on
//!   them; the parent checks that every child planned as many waves as the first.
//! - Every unit staged stands at full health throughout, so nothing dies in the pictures.
//! - A wave's run and its blank show the same battle frame for frame: the warm-up before the
//!   staging waits on pipeline compilation, which is wall time, so the battle stands still at
//!   [`STAGE_TICK`] through it, and the settling and the wave last fixed counts of frames
//!   ([`SETTLE_FRAMES`], [`WAVE_FRAMES`]). A child staging at another tick fails.

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
/// Simulation tick at which a child pauses the battle until it stages its wave. The camera's
/// settling, the pan and the renderer's warm-up take as many frames as the pipelines take to
/// compile, which is wall time; with the battle paused through them, every child of a wave (its
/// run and its blank) stages on the same world at the same tick, and so fires at the same tick.
/// 42 ticks (1.68 s) is the battle time at which the children used to stage when nothing
/// delayed the warm-up, so the spots and the subjects' ground are those of the earlier runs.
const STAGE_TICK: u32 = 42;
/// Frames, of [`FRAME_STEP`] each, each wave is pictured for: 2 seconds.
const WAVE_FRAMES: u32 = 50;
/// Most shooters a wave stages.
const MAX_SUBJECTS: usize = 9;
/// Fewest spots a wave child may stage on (plan `2026-11-20-gpu-lane` D-3): a child that finds
/// fewer fails, naming the conditions that rejected the candidates.
const MIN_SPOTS: usize = 6;
/// Distances from the camera's target, in world units, at which the spots are sought.
const SPOT_DISTANCES: [f32; 14] = [
    3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 3.5, 4.5, 5.5, 6.5, 7.5, 2.5, 10.0,
];
/// Clearance around a spot, in world units, from any unit, structure or resource node (the
/// green crystal props stand on nodes; review 1, P1-3).
const SPOT_CLEARANCE: f32 = 3.0;
/// Clearance of a spot from every structure's and resource node's footprint cell, world units
/// (plan `2026-11-20-resource-footprints` Amendment 4): the subjects stand on open ground away
/// from the base's hall and its re-laid mineral line and geysers, where its workers walk.
const FOOTPRINT_CLEARANCE: f32 = 4.5;
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
/// Frames, of [`FRAME_STEP`] each, the staged units stand, unarmed, before the baseline
/// picture: they appear and settle (review 2, E2-3). 0.4 seconds.
const SETTLE_FRAMES: u32 = 10;
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
/// Window pixels within which a cross-plane projectile's flight starts from its shooter's drawn
/// bubble outline (plan `2026-11-17-effect-snapshot-identity` Amendment 7, B-133).
const LAUNCH_TOLERANCE_PX: f32 = 3.0;

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
    /// Where the first flight of the travel look started (its line's start, where the look is
    /// drawn at its birth), projected to window pixels (Amendment 7, B-133).
    #[serde(default)]
    launch_px: Option<[f32; 2]>,
}

/// What a child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct WaveReport {
    subjects: Vec<SubjectReport>,
    /// Families no presentable shooter could carry (no shooter of their event kind).
    unstaged: Vec<String>,
    waves: usize,
    /// Where the camera was panned to before the spots were sought (world `x, y`; plan
    /// `2026-11-20-gpu-lane` D-3), and the spots found there.
    #[serde(default)]
    camera: [f32; 2],
    #[serde(default)]
    spots: usize,
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
    /// The units are staged, unarmed, and have stood for `frames` frames.
    Settling {
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
    /// Whether the camera has been panned to the waves' ground (plan `2026-11-20-gpu-lane`
    /// D-3); the boot and framing phases run again after the pan.
    panned: bool,
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
    /// The instances of its travel and its impact look seen so far, as `(pool slot, birth's
    /// bits)`: an instance is the subject's when the subject is the nearest of those firing the
    /// same look ([`owner`]), so subjects of one wave that share a look are told apart.
    seen: [Vec<(usize, u64)>; 2],
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

/// Whether no structure's and no resource node's footprint cell lies within
/// [`FOOTPRINT_CLEARANCE`] of `point` (plan `2026-11-20-resource-footprints` Amendment 4): since nodes have their
/// 5.0.14 footprints (a mineral field 2×1, a geyser 3×3) and the geysers stand in the
/// base corners, a node's centre no longer says how close its body is, and a spot keeps the
/// clearance from the footprint itself, on open ground like the spots away from the base.
fn footprints_clear(world: &SimWorld, point: Vec2) -> bool {
    let map = &world.map;
    let reach = (FOOTPRINT_CLEARANCE * 2.0).ceil() as i32;
    (-reach..=reach).all(|x: i32| {
        (-reach..=reach).all(|y: i32| {
            let offset = Vec2::new(x as f32, y as f32) * 0.5;
            let probe = point + offset;
            offset.length() > FOOTPRINT_CLEARANCE
                || map
                    .world_to_cell(sim_position(probe))
                    .ok()
                    .and_then(|(cx, cy)| map.cell(cx, cy).ok())
                    .is_none_or(|cell| {
                        !cell.flags.intersects(
                            CellFlags::OCCUPIED_BY_STRUCTURE | CellFlags::OCCUPIED_BY_RESOURCE,
                        )
                    })
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

/// What a candidate spot must be, in the order [`SpotSearch::rejected`] counts them: open
/// ground under the 5 by 5 probe; walkable at both ends of the widest pair; level, at one height
/// under the probe and both ends ([`level`]); clear of every unit; clear of every node and
/// footprint; apart from the spots already found; in the viewer's sight; on screen.
const CONDITIONS: [&str; 8] = [
    "open", "sides", "level", "units", "nodes", "apart", "fog", "screen",
];
/// Most the terrain's height may differ, world units, between a spot and any point of its probe
/// or the ends of its widest pair: a subject never straddles a cliff's edge, where the high
/// ground hides a target below it from the camera (WP3's first trial on the panned ground).
const LEVEL_TOLERANCE: f32 = 0.05;

/// When a spot must be in the viewer's sight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sight {
    /// Already, when the spot is sought.
    Now,
    /// Once the wave's shooters, the viewer's own units, stand on it (plan `2026-11-20-gpu-lane`
    /// D-3 item 2): the fog is still counted, but rejects nothing, and the baseline picture
    /// asserts that every staged unit is seen.
    OnceStaged,
}

/// The spots [`search_spots`] found near a point and, per condition of [`CONDITIONS`], how many
/// of the candidates it tried failed it (a candidate failing several counts under each).
#[derive(Debug, Clone, Default, PartialEq)]
struct SpotSearch {
    /// The spots found, world `x, y`, in the order they were found.
    spots: Vec<Vec2>,
    /// Candidates tried before the search stopped.
    tried: usize,
    /// Rejections per condition of [`CONDITIONS`].
    rejected: [usize; CONDITIONS.len()],
}

impl SpotSearch {
    /// The rejection counts, one `name: count` per condition.
    fn rejections(&self) -> String {
        CONDITIONS
            .iter()
            .zip(self.rejected)
            .map(|(name, count)| format!("{name}: {count}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The spots, if there are at least [`MIN_SPOTS`] (plan `2026-11-20-gpu-lane` D-3, the
    /// guard); otherwise a refusal that counts the candidates each condition rejected, so a
    /// map or clearance change that crowds the waves fails here instead of doubling the
    /// target's time.
    fn at_least_min(&self, around: Vec2) -> Result<&[Vec2], String> {
        if self.spots.len() >= MIN_SPOTS {
            Ok(&self.spots)
        } else {
            Err(format!(
                "only {} spots near {around:?} (at least {MIN_SPOTS} needed); of {} candidates \
                 rejected per condition: {}",
                self.spots.len(),
                self.tried,
                self.rejections()
            ))
        }
    }
}

/// Whether the terrain under the 5 by 5 probe round `point` and under both ends of the widest
/// pair, each a body's reach further out, stands within [`LEVEL_TOLERANCE`] of its height at
/// `point`.
fn level(world: &SimWorld, point: Vec2) -> bool {
    let map = &world.map;
    let height = |at: Vec2| map.terrain_altitude_at(sim_position(at));
    let centre = height(point);
    let reach = MOST_GAP * 0.5 + BODY_REACH;
    let probe =
        (-2..=2).flat_map(|x: i32| (-2..=2).map(move |y: i32| Vec2::new(x as f32, y as f32)));
    let ends = [-reach, reach]
        .into_iter()
        .flat_map(|offset| [Vec2::new(offset, 0.0), Vec2::new(0.0, offset)]);
    probe
        .chain(ends)
        .all(|offset| (height(point + offset) - centre).abs() <= LEVEL_TOLERANCE)
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
    search_spots(world, fog, around, count, Sight::Now, on_screen).spots
}

/// [`find_spots`], counting per condition the candidates that failed it; with
/// [`Sight::OnceStaged`] a spot outside the viewer's sight is admitted (and counted).
fn search_spots(
    world: &SimWorld,
    fog: &BattleFog,
    around: Vec2,
    count: usize,
    sight: Sight,
    on_screen: impl Fn(Vec2) -> bool,
) -> SpotSearch {
    let mut search = SpotSearch::default();
    for distance in SPOT_DISTANCES {
        for step in 0..24 {
            if search.spots.len() == count {
                return search;
            }
            let point =
                around + Vec2::from_angle(step as f32 * std::f32::consts::TAU / 24.0) * distance;
            let met = spot_conditions(world, fog, point, &search.spots, &on_screen);
            search.tried += 1;
            for (rejected, met) in search.rejected.iter_mut().zip(met) {
                *rejected += usize::from(!met);
            }
            let required = |(index, met): (usize, &bool)| {
                *met || (sight == Sight::OnceStaged && CONDITIONS[index] == "fog")
            };
            if met.iter().enumerate().all(required) {
                search.spots.push(point);
            }
        }
    }
    search
}

/// Whether `point` meets each condition of [`CONDITIONS`], given the spots already found.
fn spot_conditions(
    world: &SimWorld,
    fog: &BattleFog,
    point: Vec2,
    spots: &[Vec2],
    on_screen: &impl Fn(Vec2) -> bool,
) -> [bool; CONDITIONS.len()] {
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
        .all(|node| clear_of(Vec2::new(node.pos.x.to_f32(), node.pos.y.to_f32())))
        && footprints_clear(world, point);
    let apart = spots
        .iter()
        .all(|spot| spot.distance(point) > SUBJECT_APART);
    let ends = [-MOST_GAP * 0.5, MOST_GAP * 0.5]
        .iter()
        .all(|dx| on_screen(point + Vec2::new(*dx, 0.0)));
    // Open ground at both ends of the widest pair, across and up and down the screen (the
    // approach wave stands pairs along the map's `y`), so no picture runs off the map. Each end
    // is probed a body's reach further out, so no staged unit's disc runs off the map's edge (a
    // shooter of radius 0.6 at x = 0.54 broke the simulation's movement domain).
    let reach = MOST_GAP * 0.5 + BODY_REACH;
    let sides = [-reach, reach].iter().all(|offset| {
        walkable(world, point + Vec2::new(*offset, 0.0))
            && walkable(world, point + Vec2::new(0.0, *offset))
    });
    [
        open(world, point),
        sides,
        level(world, point),
        units,
        nodes,
        apart,
        fog.sees(point.x, point.y),
        ends,
    ]
}

/// World units between the rings of candidate camera targets round the start (plan
/// `2026-11-20-gpu-lane` D-3).
const PAN_STEP: f32 = 2.0;
/// Rings of candidate camera targets round the start.
const PAN_RINGS: usize = 12;
/// Candidate camera targets on each ring, evenly round it.
const PAN_DIRECTIONS: usize = 16;

/// The candidate camera targets (world `x, y`), nearest the start first: `start` itself, then
/// [`PAN_RINGS`] rings [`PAN_STEP`] apart, each of [`PAN_DIRECTIONS`] points from the map's `+x`
/// round, every point kept on the map as the battle camera keeps its own target.
fn camera_targets(world: &SimWorld, start: Vec2) -> Vec<Vec2> {
    let high = Vec2::new(
        world.map.width() as f32 - 0.5,
        world.map.height() as f32 - 0.5,
    );
    let mut targets = vec![start];
    for ring in 1..=PAN_RINGS {
        for step in 0..PAN_DIRECTIONS {
            let angle = step as f32 * std::f32::consts::TAU / PAN_DIRECTIONS as f32;
            let point = (start + Vec2::from_angle(angle) * PAN_STEP * ring as f32)
                .clamp(Vec2::splat(0.5), high);
            if !targets.contains(&point) {
                targets.push(point);
            }
        }
    }
    targets
}

/// The camera target the waves are staged round (plan `2026-11-20-gpu-lane` D-3): the first of
/// [`camera_targets`] round `start` at which [`search_spots`] finds [`MAX_SUBJECTS`] spots, or
/// else the one where it finds the most (the nearest of those). `on_screen` is the window's
/// open part as the camera sees it with its target at `start`; a candidate sees it moved by the
/// pan, since panning moves the camera without turning or zooming it. Depends on the map, the
/// catalog, the units and the fog alone, so the parent's test and every child agree.
fn camera_target(
    world: &SimWorld,
    fog: &BattleFog,
    start: Vec2,
    on_screen: impl Fn(Vec2) -> bool,
) -> (Vec2, SpotSearch) {
    let mut best: Option<(Vec2, SpotSearch)> = None;
    for target in camera_targets(world, start) {
        let shift = target - start;
        let search = search_spots(
            world,
            fog,
            target,
            MAX_SUBJECTS,
            Sight::OnceStaged,
            |point| on_screen(point - shift),
        );
        if search.spots.len() == MAX_SUBJECTS {
            return (target, search);
        }
        if best
            .as_ref()
            .is_none_or(|(_, found)| search.spots.len() > found.spots.len())
        {
            best = Some((target, search));
        }
    }
    best.expect("invariant: the start itself is a candidate")
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
            seen: [Vec::new(), Vec::new()],
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

/// The subject an instance of look `look` (0 travel, 1 impact) belongs to: of the subjects
/// whose looks name its effect, the one nearest on the map to where the instance is, measured
/// from an impact's anchor to the target, and from a travel's line start (or anchor) to the
/// shooter (or target). Subjects stand at least [`SUBJECT_APART`] apart, so the nearest is the
/// one that fired it.
fn owner(
    subjects: &[StagedSubject],
    look: usize,
    instance: &client::effects::pool::EffectInstance,
) -> Option<usize> {
    let plane = |at: Vec3| Vec2::new(at.x, at.z);
    let distance = |subject: &StagedSubject| match (look, instance.line) {
        (0, Some(line)) => plane(line.from).distance(subject.shooter.0),
        _ => plane(instance.anchor).distance(subject.target.0),
    };
    subjects
        .iter()
        .enumerate()
        .filter(|(_, subject)| subject.looks[look].is_some_and(|(id, _)| id == instance.effect))
        .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))
        .map(|(index, _)| index)
}

/// Records each subject's spawns since fire was opened at `opened` (seconds of
/// `Time<Virtual>`): the distinct instances of its looks it owns ([`owner`]), and whether one
/// of its impacts is born and alive at `now`.
fn count(staging: &mut Staging, runtime: &EffectRuntime, opened: f64, now: f64) {
    let mut born = vec![false; staging.subjects.len()];
    for (slot, instance) in runtime.pool().iter() {
        if instance.birth < opened {
            continue;
        }
        for look in 0..2 {
            let Some(index) = owner(&staging.subjects, look, instance) else {
                continue;
            };
            let key = (slot, instance.birth.to_bits());
            let seen = &mut staging.subjects[index].seen[look];
            if !seen.contains(&key) {
                seen.push(key);
            }
            // Review 3, E3-1: an instant shot's impact waits for its slug, so the impact is
            // looked for from its birth, not from its spawn request.
            born[index] |= look == 1 && instance.birth <= now;
        }
    }
    for ((subject, report), born) in staging
        .subjects
        .iter()
        .zip(&mut staging.report.subjects)
        .zip(born)
    {
        report.travel_spawned = u32::try_from(subject.seen[0].len()).unwrap_or(u32::MAX);
        report.impact_spawned = u32::try_from(subject.seen[1].len()).unwrap_or(u32::MAX);
        report.impacts.push(report.impact_spawned);
        report.impact_born.push(born);
    }
}

/// Records, for each subject whose travel look flew since `opened` (seconds of
/// `Time<Virtual>`) and has no launch recorded yet, where its earliest flight started,
/// projected through `camera` (Amendment 7, B-133).
fn record_launch(
    staging: &mut Staging,
    runtime: &EffectRuntime,
    camera: (&Camera, &GlobalTransform),
    opened: f64,
) {
    let subjects = &staging.subjects;
    for (index, (subject, report)) in subjects
        .iter()
        .zip(&mut staging.report.subjects)
        .enumerate()
    {
        let Some((travel, _)) = subject.looks[0] else {
            continue;
        };
        if report.launch_px.is_some() {
            continue;
        }
        let first = runtime
            .pool()
            .iter()
            .filter(|(_, instance)| instance.effect == travel && instance.birth >= opened)
            .filter(|(_, instance)| owner(subjects, 0, instance) == Some(index))
            .filter_map(|(_, instance)| instance.line.map(|line| (instance.birth, line.from)))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        report.launch_px = first.map(|(_, from)| project3(camera, from));
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
    (mut cameras, bubbles): (
        Query<'_, '_, (&Camera, &GlobalTransform, &mut RtsCamera)>,
        Query<'_, '_, &BubbleInstance, With<LiveBattleBubble>>,
    ),
    idle: Res<'_, LabRenderIdle>,
) {
    let fog = fog.expect("the skirmish presents a player, so it keeps a fog");
    if !session.is_ready() {
        return;
    }
    let Ok((camera, transform, mut rts)) = cameras.get_single_mut() else {
        return;
    };
    let tick = session.published().map_or(0, |published| published.tick);
    let now = time.elapsed_secs_f64();
    let idle_long = idle.0.load(Ordering::Acquire) >= IDLE_FRAMES;
    staging.phase = match staging.phase {
        Phase::Boot if tick < STAGE_TICK => Phase::Boot,
        Phase::Boot => {
            // The battle stands still at STAGE_TICK until the wave is staged, however many
            // frames the camera and the renderer take to settle.
            lab.battle_paused = true;
            Phase::Booting {
                still: 0,
                height: rts.look_at_y,
            }
        }
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
        Phase::Framing { .. } if !staging.panned => {
            // D-3 item 1: pan, without zooming, to where the waves have room, then let the
            // camera and the renderer settle again.
            let world = session.world().expect("a ready session has a world");
            let (target, _) = camera_target(
                world,
                &fog,
                rts.target,
                on_screen(world, (camera, transform)),
            );
            rts.target = target;
            staging.panned = true;
            staging.report.camera = target.to_array();
            Phase::Booting {
                still: 0,
                height: rts.look_at_y,
            }
        }
        Phase::Framing { .. } => {
            let (Some(runtime), Some(catalog)) = (effects.0.as_deref(), effects.1.as_deref())
            else {
                panic!("the effect system is loaded before the staging");
            };
            assert_eq!(
                tick, STAGE_TICK,
                "wave {}: the battle stands still at the staging tick through the warm-up",
                staging.wave
            );
            let world = session.world().expect("a ready session has a world");
            let search = search_spots(
                world,
                &fog,
                rts.target,
                MAX_SUBJECTS,
                Sight::OnceStaged,
                on_screen(world, (camera, transform)),
            );
            // D-3 item 3: the guard against crowded waves.
            let spots = match search.at_least_min(rts.target) {
                Ok(spots) => spots.to_vec(),
                Err(refusal) => panic!("wave {}: {refusal}", staging.wave),
            };
            staging.report.spots = spots.len();
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
            Phase::Settling { frames: 0 }
        }
        Phase::Settling { frames } if frames < SETTLE_FRAMES => {
            heal(&staging, &mut session);
            Phase::Settling { frames: frames + 1 }
        }
        Phase::Settling { .. } => {
            assert_staged_units_seen(&staging, &session, &fog);
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
        Phase::Running { at, frame } if frame < WAVE_FRAMES => {
            heal(&staging, &mut session);
            picture(
                &mut commands,
                &mut staging,
                &session,
                (camera, transform),
                frame,
            );
            if let Some(runtime) = effects.0.as_deref() {
                count(&mut staging, runtime, at, now);
                record_launch(&mut staging, runtime, (camera, transform), at);
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

/// D-3 item 2: every subject's shooter and target stand in the viewer's sight when the
/// baseline is pictured, so nothing is pictured under fog; fails naming the subject otherwise.
fn assert_staged_units_seen(staging: &Staging, session: &BattleSession, fog: &BattleFog) {
    let world = session.world().expect("a ready session has a world");
    for (subject, report) in staging.subjects.iter().zip(&staging.report.subjects) {
        for (role, id) in ["shooter", "target"].iter().zip(subject.units) {
            let Some(unit) = world.entities.get(&id) else {
                panic!(
                    "wave {}: {} ({:?}): its {role} is gone before the baseline",
                    staging.wave, report.family, report.approach
                );
            };
            let (x, y) = (unit.position.x.to_f32(), unit.position.y.to_f32());
            assert!(
                fog.sees(x, y),
                "wave {}: {} ({:?}): its {role} at ({x:.2}, {y:.2}) is not in the viewer's sight \
                 at the baseline",
                staging.wave,
                report.family,
                report.approach
            );
        }
    }
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
        // The pictures and the spawn counts of a frame describe the same pool: the child reads
        // it after this frame's effects are spawned and its held beams kept. Unordered, the
        // systems are ambiguous and the multi-threaded executor runs them either way round,
        // by thread timing: a child that read the pool first marked each impact's birth a frame
        // late and measured four other frames (a blank melee impact then counted 21 and 49 px
        // beside the struck bubble instead of 9 and 36, and its wave failed).
        .add_systems(
            Update,
            stage_wave_child
                .in_set(BattleSet::Present)
                .after(client::effects::systems::spawn_effects_from_events)
                .after(client::effects::beams::keep_beams),
        );
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

/// B-133 (plan `2026-11-17-effect-snapshot-identity` Amendment 7): whether `subject`'s first
/// flight started within [`LAUNCH_TOLERANCE_PX`] of its shooter's drawn bubble outline, not
/// below a flyer's bubble; prints the measure and returns the failure, if any.
fn judge_launch(subject: &SubjectReport, wave: usize, driver: &str) -> Option<String> {
    let [cx, cy, radius] = subject.shooter_px;
    let Some([x, y]) = subject.launch_px else {
        return Some(format!(
            "{} (CrossPlane): no flight of its travel look was seen",
            subject.family
        ));
    };
    let from_centre = (x - cx).hypot(y - cy);
    let off = (from_centre - radius).abs();
    eprintln!(
        "driver {driver}: wave {wave} {} (CrossPlane): its flight starts {from_centre:.2} px \
         from the shooter's drawn centre, whose outline is {radius:.2} px out: {off:.2} px off \
         the outline (tolerance {LAUNCH_TOLERANCE_PX})",
        subject.family
    );
    (off > LAUNCH_TOLERANCE_PX).then(|| {
        format!(
            "{} (CrossPlane): its flight starts {off:.2} px off the shooter's drawn bubble \
             outline (tolerance {LAUNCH_TOLERANCE_PX})",
            subject.family
        )
    })
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
    let mut planned = None;
    loop {
        let (report, directory) = run_wave(wave, None)?;
        assert!(
            !report.subjects.is_empty(),
            "wave {wave} staged no subject (no open spot was found)"
        );
        eprintln!(
            "driver {driver}: wave {wave} staged {} subjects on {} spots round the camera's \
             target {:?}",
            report.subjects.len(),
            report.spots,
            report.camera
        );
        if wave == 0 {
            eprintln!(
                "driver {driver}: {} waves; lab families no presentable shooter carries: {:?}",
                report.waves, report.unstaged
            );
        }
        // Every child plans the waves from the same catalog and the same spots.
        let waves = *planned.get_or_insert(report.waves);
        assert_eq!(
            report.waves, waves,
            "wave {wave} planned {} waves, wave 0 {waves}",
            report.waves
        );
        // The wave's blank: the same staging with every impact of the wave drawn as nothing.
        let mut impacts: Vec<&str> = report.subjects.iter().filter_map(impact_of).collect();
        impacts.sort_unstable();
        impacts.dedup();
        let (blank, blank_directory) = run_wave(wave, Some(&impacts.join(",")))?;
        for (index, subject) in report.subjects.iter().enumerate() {
            let (judged, seen) = judge(directory.path(), subject, wave, &driver)?;
            let impact_seen = seen.impact;
            failures.extend(judged);
            if subject.approach == Approach::CrossPlane && subject.flies {
                failures.extend(judge_launch(subject, wave, &driver));
            }
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

// --- Held beam (plan `2026-11-17-effect-snapshot-identity` WP7, D-4, B-126) --------------------
//
// A presentable shooter whose block names a sustained travel look (a held beam) fires at an
// unarmed target walking across the screen in front of it; the target is killed after
// [`BEAM_STREAM_FRAMES`]. Every 40 ms frame the child records where the held beam's line ends
// and where the target's drawn bubble's outline point towards the shooter is, both projected
// to the window, and how many beams of the look are held and live; at [`BEAM_SAMPLE_FRAME`],
// and once the beam's exit and one frame are over after the target's death, it stops the clock
// and pictures the window with and without effect layers. The parent checks that one beam is
// held through the stream, that its end lies within [`BEAM_END_TOLERANCE_PX`] of the target's
// outline point on every frame, that the paired pictures show the beam reaching that point
// while the target lives, and that after its death no beam instance is live and no pixel of
// its path changes.

/// Environment variable naming the held-beam child's output directory.
const BEAM_CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_HELD_BEAM_CHILD_OUT";
/// Frames the beam streams at the walking target, from the first frame it is held, before the
/// target is killed.
const BEAM_STREAM_FRAMES: u32 = 40;
/// Frames the child waits for the first held beam (the shooter walks into range and winds up)
/// before it kills the target anyway, so the parent fails on a beam that never came.
const BEAM_MAX_WAIT: u32 = 150;
/// Frame of the stream, from the first frame the beam is held, at which the paired pictures
/// are taken.
const BEAM_SAMPLE_FRAME: u32 = 20;
/// Frames of the stream, from the first held beam, pictured round the beam's end at 1:1: about
/// two strikes of the Void-Ray analogue's 0.36 s period (review round 1, R2-1).
const BEAM_SEQUENCE: (u32, u32) = (4, 20);
/// Window pixels of the sequence's crop round the beam's end.
const BEAM_SEQUENCE_CROP: [u32; 2] = [96, 96];
/// Frames recorded after the kill.
const BEAM_AFTER_FRAMES: u32 = 12;
/// The plan's tolerance on the beam's end, window pixels.
const BEAM_END_TOLERANCE_PX: f32 = 3.0;
/// World units the target walks across the screen, and the shooter stands in front of its path.
const BEAM_WALK: f32 = 3.0;
/// Channel change, 0 to 255, at which a pixel of the paired pictures counts as changed.
const BEAM_CHANGE_LEVEL: u8 = 8;
/// Window pixels either side of the beam's path sampled in the paired pictures.
const BEAM_PATH_HALF_PX: f32 = 3.0;
/// Shares of the path from the muzzle sampled after the death: clear of both bubbles and of
/// the death's look round the target.
const BEAM_PATH_SPAN: (f32, f32) = (0.2, 0.6);

/// One recorded frame of the held-beam child.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct BeamFrame {
    /// Frame since the attack was ordered.
    frame: u32,
    /// Whether the presented frame shows the target.
    target_shown: bool,
    /// Beams [`client::effects::beams::HeldBeams`] holds, and live instances of the look.
    held: usize,
    live: usize,
    /// The held beam's line, muzzle and end, projected (window pixels), when one is held.
    muzzle_px: Option<[f32; 2]>,
    end_px: Option<[f32; 2]>,
    /// The target's drawn bubble's outline point towards the shooter, projected.
    outline_px: Option<[f32; 2]>,
    /// Seconds since the youngest live burn (the per-strike impact) was born, if one is live
    /// (review round 1, R2-1: the strike's flare on the target's rim).
    #[serde(default)]
    burn_age: Option<f32>,
    /// That burn's centre (its anchor), projected (review round 2, R3-1: it follows the beam's
    /// end).
    #[serde(default)]
    burn_px: Option<[f32; 2]>,
}

/// What the held-beam child reports.
#[derive(Debug, Default, Serialize, Deserialize)]
struct BeamReport {
    look: String,
    shooter: String,
    frames: Vec<BeamFrame>,
    /// The paired pictures at the sample frame and after the death: `(drawn, bare)` files, and
    /// the beam's projected muzzle and end (after the death, as last held).
    sample: Option<(String, String, [f32; 2], [f32; 2])>,
    after: Option<(String, String, [f32; 2], [f32; 2])>,
    /// The look's exit, seconds.
    exit: f32,
    /// Frame the target was killed on.
    kill_frame: u32,
    /// The 1:1 sequence round the beam's end through two strikes (R2-1): frame and file.
    #[serde(default)]
    sequence: Vec<(u32, String)>,
}

/// What the held-beam child is doing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum BeamPhase {
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
        frame: u32,
    },
    /// The clock is stopped for a paired picture: `bare` once the drawn one is taken; `after`
    /// for the picture after the death.
    Picturing {
        frame: u32,
        bare: bool,
        settle: u32,
        after: bool,
    },
    Finishing,
}

/// The held-beam child's staging.
#[derive(Resource, Default)]
struct BeamStaging {
    directory: PathBuf,
    phase: BeamPhase,
    shooter: Option<EntityId>,
    target: Option<EntityId>,
    look: Option<client::effects::catalog::EffectId>,
    /// The last held beam's muzzle and end, Bevy world space (projected when pictured, through
    /// the camera of that moment).
    last_line: Option<(Vec3, Vec3)>,
    /// The frame the target was first missing from the presented frame.
    gone: Option<u32>,
    /// The first frame a beam was held.
    first_held: Option<u32>,
    /// The per-strike impact look (the burn), whose flare R2-1 asks for.
    burn: Option<client::effects::catalog::EffectId>,
    /// The frame to record in `PostUpdate`, once transforms are propagated (what is drawn).
    record_now: Option<u32>,
    outstanding: Arc<AtomicU32>,
    report: BeamReport,
}

/// Whether the effect layers are drawn: cleared for the bare picture of a pair.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
struct BeamView {
    bare: bool,
}

/// Clears the frame's effect instances for a bare picture.
fn apply_beam_view(view: Res<'_, BeamView>, runtime: Option<ResMut<'_, EffectRuntime>>) {
    let Some(mut runtime) = runtime else {
        return;
    };
    if view.bare {
        runtime.frame.sorted.clear();
        runtime.frame.draw_decals = false;
        runtime.frame.distortions.clear();
        runtime.frame.lights.clear();
    }
}

/// The held beam's shooter (found by data, C5): the first presentable unit whose block names a
/// sustained travel look for one of its weapons; failing one (the beam units have no media),
/// a presentable mobile ground unit with a primary weapon, lent in the test world's catalog
/// only the looks of the shipped beam weapon with the shortest period (its travel and impact), its weapon made an instant
/// beam (`LaserBeam`, no projectile speed, a range of at least 6, the beam unit's cadence), as
/// the waves lend lab families. Returns the unit, its
/// weapon's slot and the look's name.
fn beam_shooter(
    world: &mut SimWorld,
    catalog: &EffectCatalog,
) -> Option<(UnitTypeId, WeaponSlot, String)> {
    let sustained = |name: &String| {
        catalog
            .effect_id(name)
            .and_then(|id| catalog.effect(id))
            .is_some_and(|effect| effect.sustain.is_some())
    };
    let own = world.catalog.archetypes().find_map(|arch| {
        if arch.max_speed <= SimScalar::ZERO || !presentable(&assets(), &arch.slug) {
            return None;
        }
        arch.effects.weapons.iter().find_map(|entry| {
            let travel = entry.travel.as_ref().filter(|travel| sustained(travel))?;
            (entry.mode.is_none() && slot_weapon(arch, entry.weapon).is_some())
                .then(|| (arch.unit_type_id, entry.weapon, travel.clone()))
        })
    });
    if own.is_some() {
        return own;
    }
    // The shipped beam weapon with the shortest period (the Void-Ray analogue's), whose look
    // and cadence are lent, so the 1:1 sequence spans several strikes.
    let (travel, impact, timing) = world
        .catalog
        .archetypes()
        .flat_map(|arch| {
            arch.effects.weapons.iter().filter_map(|entry| {
                let travel = entry.travel.as_ref().filter(|travel| sustained(travel))?;
                let timing = slot_weapon(arch, entry.weapon).cloned()?;
                Some((travel.clone(), entry.impact.clone(), Some(timing)))
            })
        })
        .min_by_key(|(_, _, timing)| {
            timing
                .as_ref()
                .map_or(u16::MAX, |weapon| weapon.period_ticks)
        })?;
    let id = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.max_speed > SimScalar::ZERO
                && arch.mover_type == MoverType::Ground
                && arch.modes.is_empty()
                && arch.weapon.is_some()
                && (arch.ground_weapon.is_none() || arch.ground_weapon == arch.weapon)
                && presentable(&assets(), &arch.slug)
        })?
        .unit_type_id;
    let archetype = world
        .catalog
        .get_mut(id)
        .expect("invariant: the type was just found in this catalog");
    for weapon in [archetype.weapon.as_mut(), archetype.ground_weapon.as_mut()]
        .into_iter()
        .flatten()
    {
        weapon.visual_kind = WeaponVisualKind::LaserBeam;
        weapon.projectile_speed = None;
        // The beam unit's own cadence, so the strikes (and their burns) come as the beam's do.
        if let Some(timing) = &timing {
            weapon.period_ticks = timing.period_ticks;
            weapon.damage_point_ticks = timing.damage_point_ticks;
            weapon.backswing_ticks = timing.backswing_ticks;
            weapon.random_delay_min = timing.random_delay_min;
            weapon.random_delay_max = timing.random_delay_max;
        }
        // A beam's reach (the Void-Ray analogue's is 6), so the walking target stays in range
        // and the beam is not broken off by the shooter walking after it.
        weapon.max_range = weapon.max_range.max(SimScalar::from_num(6));
    }
    archetype.effects.weapons = vec![sim_core::catalog::WeaponEffects {
        weapon: WeaponSlot::Primary,
        mode: None,
        launch: None,
        travel: Some(travel.clone()),
        impact,
        splash: None,
    }];
    Some((id, WeaponSlot::Primary, travel))
}

/// Stages the held beam: the shooter in front of the target's walk across the screen, the
/// target walking, the attack ordered.
fn stage_beam(
    staging: &mut BeamStaging,
    session: &mut BattleSession,
    catalog: &EffectCatalog,
    spot: Vec2,
    up: Vec2,
) {
    let world = session.world_mut().expect("a ready session has a world");
    let (shooter_type, slot, look) =
        beam_shooter(world, catalog).expect("a presentable unit names a held beam");
    let air = slot == WeaponSlot::Air;
    let range = world
        .catalog
        .get(shooter_type)
        .ok()
        .and_then(|arch| slot_weapon(arch, slot))
        .map_or(3.0_f32, |weapon| weapon.max_range.to_f32());
    let target_type = target_type(world, &[], air);
    let reach = (range * 0.6).clamp(1.5, 2.5);
    let shooter = spawn_still(world, shooter_type, VIEWER, spot - up * reach);
    let across = Vec2::new(up.y, -up.x);
    let target = world
        .spawn_unit(
            target_type,
            ENEMY,
            sim_position(spot - across * BEAM_WALK * 0.5),
        )
        .expect("open ground");
    world
        .entities
        .get_mut(&target)
        .expect("just spawned")
        .issue_order(
            UnitOrder::Move {
                target: sim_position(spot + across * BEAM_WALK * 0.5),
            },
            false,
        )
        .expect("the walk is admitted");
    staging.report.shooter = world
        .catalog
        .get(shooter_type)
        .map_or_else(|_| String::new(), |arch| arch.slug.clone());
    staging.report.look.clone_from(&look);
    staging.report.exit = catalog
        .effect_id(&look)
        .and_then(|id| catalog.effect(id))
        .map_or(0.0, |effect| effect.held_exit);
    staging.look = catalog.effect_id(&look);
    staging.burn = world
        .catalog
        .get(shooter_type)
        .ok()
        .and_then(|arch| {
            arch.effects
                .weapons
                .iter()
                .find(|entry| entry.weapon == slot && entry.mode.is_none())
        })
        .and_then(|entry| entry.impact.as_ref())
        .and_then(|name| catalog.effect_id(name));
    staging.shooter = Some(shooter);
    staging.target = Some(target);
    let (world, queue) = session.world_and_commands();
    queue
        .submit_selection(
            world.expect("a ready session has a world"),
            VIEWER,
            &[shooter],
            UnitOrder::attack(target),
            false,
        )
        .expect("the client admits an attack by the viewer's own unit");
}

/// The outline point of the drawn bubble at `centre` (Bevy world) of `radius` towards
/// `towards` (a Bevy world point), as the camera sees it, at the struck surface's margin:
/// its silhouette's point on the side of `towards`.
fn outline_point(
    camera: (&Camera, &GlobalTransform),
    centre: Vec3,
    radius: f32,
    towards: Vec3,
) -> [f32; 2] {
    let view = camera.1.forward().as_vec3();
    let across = Vec3::new(towards.x - centre.x, 0.0, towards.z - centre.z).normalize_or_zero();
    let side = (across - view * across.dot(view)).normalize_or_zero();
    project3(
        camera,
        centre + side * (radius + client::effects::struck::SURFACE_MARGIN),
    )
}

/// Records one frame of the held-beam child.
fn record_beam(
    staging: &mut BeamStaging,
    frame: u32,
    camera: (&Camera, &GlobalTransform),
    drawn: &[(EntityId, Vec3, f32)],
    beams: &client::effects::beams::HeldBeams,
    (runtime, now): (&EffectRuntime, f64),
) {
    let (Some(shooter), Some(target), Some(look)) = (staging.shooter, staging.target, staging.look)
    else {
        return;
    };
    let find = |id: EntityId| drawn.iter().find(|(entity, ..)| *entity == id);
    let slot = beams.slots().iter().find(|slot| slot.attacker == shooter);
    let line = slot
        .and_then(|slot| runtime.held(slot.handle))
        .and_then(|instance| instance.line);
    let (muzzle_px, end_px) = line.map_or((None, None), |line| {
        (
            Some(project3(camera, line.from)),
            Some(project3(camera, line.to)),
        )
    });
    if let Some(line) = line {
        staging.last_line = Some((line.from, line.to));
    }
    let outline_px = find(target)
        .zip(find(shooter))
        .map(|(target, shooter)| outline_point(camera, target.1, target.2, shooter.1));
    let youngest = staging.burn.and_then(|burn| {
        runtime
            .pool()
            .iter()
            .filter(|(_, instance)| instance.effect == burn && instance.birth <= now)
            .map(|(_, instance)| ((now - instance.birth) as f32, instance.anchor))
            .reduce(|a, b| if b.0 < a.0 { b } else { a })
    });
    let target_shown = find(target).is_some();
    if !target_shown && staging.gone.is_none() {
        staging.gone = Some(frame);
    }
    staging.report.frames.push(BeamFrame {
        frame,
        target_shown,
        held: beams.slots().len(),
        live: runtime
            .pool()
            .iter()
            .filter(|(_, instance)| instance.effect == look)
            .count(),
        muzzle_px,
        end_px,
        outline_px,
        burn_age: youngest.map(|(age, _)| age),
        burn_px: youngest.map(|(_, anchor)| project3(camera, anchor)),
    });
}

/// Records the frame the staging asked for, once transforms are propagated, so the beam's
/// line and the target's bubble are compared as this frame draws them.
fn record_beam_frame(
    mut staging: ResMut<'_, BeamStaging>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform), With<RtsCamera>>,
    bubbles: Query<'_, '_, (&BubbleInstance, &GlobalTransform), With<LiveBattleBubble>>,
    beams: Option<Res<'_, client::effects::beams::HeldBeams>>,
    runtime: Option<Res<'_, EffectRuntime>>,
    time: Res<'_, Time<Virtual>>,
) {
    let Some(frame) = staging.record_now.take() else {
        return;
    };
    let (Ok(camera), Some(beams), Some(runtime)) = (cameras.get_single(), beams, runtime) else {
        return;
    };
    let drawn: Vec<(EntityId, Vec3, f32)> = bubbles
        .iter()
        .map(|(bubble, global)| (bubble.0.entity_id, global.translation(), bubble.0.radius))
        .collect();
    record_beam(
        &mut staging,
        frame,
        camera,
        &drawn,
        &beams,
        (&runtime, time.elapsed_secs_f64()),
    );
    if staging.first_held.is_none()
        && staging
            .report
            .frames
            .last()
            .is_some_and(|recorded| recorded.held > 0)
    {
        staging.first_held = Some(frame);
    }
}

/// Advances the held-beam child by one frame.
fn stage_beam_child(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, BeamStaging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    mut time: ResMut<'_, Time<Virtual>>,
    mut view: ResMut<'_, BeamView>,
    fog: Option<Res<'_, BattleFog>>,
    catalog: Option<Res<'_, EffectCatalog>>,
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
    let idle_long = idle.0.load(Ordering::Acquire) >= IDLE_FRAMES;
    staging.phase = match staging.phase {
        BeamPhase::Boot if tick < 2 => BeamPhase::Boot,
        BeamPhase::Boot => BeamPhase::Booting {
            still: 0,
            height: rts.look_at_y,
        },
        BeamPhase::Booting { still, height } if still < STILL_CAMERA_FRAMES || !idle_long => {
            let still = if (rts.look_at_y - height).abs() < 1e-4 {
                still + 1
            } else {
                0
            };
            BeamPhase::Booting {
                still,
                height: rts.look_at_y,
            }
        }
        BeamPhase::Booting { .. } => BeamPhase::Framing { frames: 0 },
        BeamPhase::Framing { frames } if frames < FRAMING_FRAMES || !idle_long => {
            BeamPhase::Framing { frames: frames + 1 }
        }
        BeamPhase::Framing { .. } => {
            let catalog = catalog
                .as_deref()
                .expect("the effect system is loaded before the staging");
            let world = session.world().expect("a ready session has a world");
            let spot = *find_spots(
                world,
                &fog,
                rts.target,
                1,
                on_screen(world, (camera, transform)),
            )
            .first()
            .expect("the start area has an open spot on screen");
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
            stage_beam(&mut staging, &mut session, catalog, spot, up);
            commands.queue(rebuild_tables);
            lab.battle_paused = false;
            BeamPhase::Running { frame: 0 }
        }
        BeamPhase::Running { frame } => {
            view.bare = false;
            staging.record_now = Some(frame);
            // R2-1: the beam's end at 1:1 through two strikes, before the sample pair.
            let in_sequence = staging.first_held.is_some_and(|first| {
                (first + BEAM_SEQUENCE.0..first + BEAM_SEQUENCE.1).contains(&frame)
            });
            if let Some([x, y]) = staging
                .report
                .frames
                .last()
                .and_then(|recorded| recorded.end_px)
                .filter(|_| in_sequence)
            {
                let file = format!("beam_seq_{frame:03}.png");
                let left = (x - BEAM_SEQUENCE_CROP[0] as f32 / 2.0).max(0.0) as u32;
                let top = (y - BEAM_SEQUENCE_CROP[1] as f32 / 2.0).max(0.0) as u32;
                capture_frame(
                    &mut commands,
                    vec![(
                        staging.directory.join(&file),
                        [left, top, BEAM_SEQUENCE_CROP[0], BEAM_SEQUENCE_CROP[1]],
                    )],
                    &staging.outstanding,
                );
                staging.report.sequence.push((frame, file));
            }
            let kill_at = staging
                .first_held
                .map_or(BEAM_MAX_WAIT, |first| first + BEAM_STREAM_FRAMES);
            let sample_at = staging.first_held.map(|first| first + BEAM_SAMPLE_FRAME);
            let world = session.world_mut().expect("a ready session has a world");
            if frame < kill_at {
                // The target stands at full health while the beam streams.
                if let Some(unit) = staging.target.and_then(|id| world.entities.get_mut(&id)) {
                    unit.hp = SimScalar::from_num(10_000);
                }
            } else if frame == kill_at {
                staging.report.kill_frame = frame;
                if let Some(unit) = staging.target.and_then(|id| world.entities.get_mut(&id)) {
                    unit.hp = SimScalar::ZERO;
                }
            }
            let exit_frames = (staging.report.exit / 0.04).ceil() as u32;
            let after_due = staging
                .gone
                .is_some_and(|gone| frame == gone + exit_frames + 1)
                && staging.report.after.is_none();
            if Some(frame) == sample_at && staging.report.sample.is_none() || after_due {
                time.pause();
                BeamPhase::Picturing {
                    frame,
                    bare: false,
                    settle: 0,
                    after: after_due,
                }
            } else if frame >= kill_at + BEAM_AFTER_FRAMES && staging.report.after.is_some() {
                lab.battle_paused = true;
                BeamPhase::Finishing
            } else {
                BeamPhase::Running { frame: frame + 1 }
            }
        }
        BeamPhase::Picturing {
            frame,
            bare,
            settle,
            after,
        } if settle < 3 || !idle_long => {
            // The view of the picture to come, set while its frames settle.
            view.bare = bare;
            BeamPhase::Picturing {
                frame,
                bare,
                settle: settle + 1,
                after,
            }
        }
        BeamPhase::Picturing {
            frame, bare, after, ..
        } => {
            let name = if after { "after" } else { "sample" };
            let file = format!("beam_{name}_{}.png", if bare { "bare" } else { "drawn" });
            capture_frame(
                &mut commands,
                vec![(
                    staging.directory.join(&file),
                    [0, 0, u32::MAX / 4, u32::MAX / 4],
                )],
                &staging.outstanding,
            );
            if bare {
                let (muzzle, end) = staging
                    .last_line
                    .map_or(([0.0; 2], [0.0; 2]), |(from, to)| {
                        (
                            project3((camera, transform), from),
                            project3((camera, transform), to),
                        )
                    });
                let drawn_file = format!("beam_{name}_drawn.png");
                let pair = Some((drawn_file, file, muzzle, end));
                if after {
                    staging.report.after = pair;
                } else {
                    staging.report.sample = pair;
                }
                // The screenshot is of this frame's render, so the view changes only from the
                // next frame on.
                time.unpause();
                BeamPhase::Running { frame: frame + 1 }
            } else {
                BeamPhase::Picturing {
                    frame,
                    bare: true,
                    settle: 0,
                    after,
                }
            }
        }
        BeamPhase::Finishing if staging.outstanding.load(Ordering::SeqCst) > 0 => {
            BeamPhase::Finishing
        }
        BeamPhase::Finishing => {
            let text = ron::to_string(&staging.report).expect("plain data");
            std::fs::write(staging.directory.join(REPORT_FILE), text)
                .expect("the parent's directory is writable");
            commands.send_event(AppExit::Success);
            BeamPhase::Finishing
        }
    };
}

/// The held-beam child: boots the production client on the skirmish and stages the beam.
#[test]
#[ignore = "launched by the test below on a virtual display"]
fn held_beam_child() {
    let directory = std::env::var_os(BEAM_CHILD_OUT_VAR).expect("the parent names a directory");
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
        .insert_resource(BeamStaging {
            directory: PathBuf::from(directory),
            ..BeamStaging::default()
        })
        .init_resource::<BeamView>()
        .add_systems(
            Update,
            stage_beam_child
                .in_set(BattleSet::Present)
                .after(client::effects::beams::keep_beams),
        )
        .add_systems(
            PostUpdate,
            (
                apply_beam_view.after(client::effects::systems::build_effect_frame),
                record_beam_frame.after(TransformSystem::TransformPropagate),
            ),
        );
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// Changed pixels of `drawn` against `bare` within `half` window pixels of the segment from
/// `from` to `to`, between shares `span` of its length.
fn changed_along(
    drawn: &image::RgbImage,
    bare: &image::RgbImage,
    (from, to): ([f32; 2], [f32; 2]),
    span: (f32, f32),
    half: f32,
) -> usize {
    let (from, to) = (Vec2::from_array(from), Vec2::from_array(to));
    let axis = to - from;
    let length = axis.length().max(1e-3);
    let (low, high) = (from.min(to) - half - 1.0, from.max(to) + half + 1.0);
    let mut count = 0;
    for y in low.y.max(0.0) as u32..(high.y as u32).min(drawn.height()) {
        for x in low.x.max(0.0) as u32..(high.x as u32).min(drawn.width()) {
            let point = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - from;
            let along = point.dot(axis) / (length * length);
            let off = (point - axis * along).length();
            if along >= span.0 && along <= span.1 && off <= half {
                let moved = drawn
                    .get_pixel(x, y)
                    .0
                    .iter()
                    .zip(bare.get_pixel(x, y).0.iter())
                    .any(|(a, b)| a.abs_diff(*b) >= BEAM_CHANGE_LEVEL);
                count += usize::from(moved);
            }
        }
    }
    count
}

/// Plan `2026-11-17-effect-snapshot-identity` WP7 (B-126): a held beam follows a walking target
/// with its end on the target's outline on every frame, one beam for the whole stream, and
/// nothing of it remains its exit and one frame after the target's death.
#[test]
fn a_held_beam_follows_its_target_and_ends_with_it() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let driver = gpu_run::selected_driver().driver.to_string();
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", "held_beam_child"]);
    command.env(BEAM_CHILD_OUT_VAR, directory.path());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("held beam");
    let report: BeamReport = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    if let Some(target) = std::env::var_os(EVIDENCE_VAR).map(PathBuf::from) {
        std::fs::create_dir_all(&target)?;
        for (_, file) in &report.sequence {
            std::fs::copy(
                directory.path().join(file),
                target.join(format!("held_beam_{driver}_{file}")),
            )?;
        }
        for pair in [&report.sample, &report.after].into_iter().flatten() {
            for file in [&pair.0, &pair.1] {
                std::fs::copy(
                    directory.path().join(file),
                    target.join(format!("held_beam_{driver}_{file}")),
                )?;
            }
        }
    }
    // The stream: from the first frame a beam is held until the kill, one beam, its end on the
    // target's outline point.
    let stream: Vec<&BeamFrame> = report
        .frames
        .iter()
        .filter(|frame| frame.frame < report.kill_frame && frame.held > 0)
        .collect();
    let first = stream.first().map_or(u32::MAX, |frame| frame.frame);
    let errors: Vec<f32> = stream
        .iter()
        .filter_map(|frame| {
            let end = Vec2::from_array(frame.end_px?);
            Some(end.distance(Vec2::from_array(frame.outline_px?)))
        })
        .collect();
    let worst = errors.iter().copied().fold(0.0_f32, f32::max);
    let travel = stream
        .first()
        .zip(stream.last())
        .and_then(|(a, b)| Some(Vec2::from_array(a.end_px?).distance(Vec2::from_array(b.end_px?))))
        .unwrap_or(0.0);
    eprintln!(
        "driver {driver}: {} ({}) holds its beam from frame {first} on {} frames to the kill at \
         {}; at most {} held and {} live; its end lies at most {worst:.2} px from the target's \
         outline point over {} frames (mean {:.2} px) and travelled {travel:.1} px",
        report.shooter,
        report.look,
        stream.len(),
        report.kill_frame,
        stream.iter().map(|frame| frame.held).max().unwrap_or(0),
        stream.iter().map(|frame| frame.live).max().unwrap_or(0),
        errors.len(),
        errors.iter().sum::<f32>() / errors.len().max(1) as f32,
    );
    let timeline: String = report
        .frames
        .iter()
        .map(|frame| match (frame.target_shown, frame.held) {
            (false, _) => 'x',
            (true, 0) => '.',
            (true, _) => '#',
        })
        .collect();
    eprintln!("driver {driver}: frames (# beam held, . none, x target gone): {timeline}");
    let burns: Vec<String> = report
        .sequence
        .iter()
        .filter_map(|(frame, _)| {
            report
                .frames
                .iter()
                .find(|recorded| recorded.frame == *frame)
        })
        .map(|recorded| {
            format!(
                "{}:{}",
                recorded.frame,
                recorded
                    .burn_age
                    .map_or_else(|| "-".to_owned(), |age| format!("{age:.2}"))
            )
        })
        .collect();
    eprintln!(
        "driver {driver}: 1:1 sequence round the beam's end, frame:age of the youngest burn (s): \
         {}",
        burns.join(" ")
    );
    assert!(
        stream.len() >= 30,
        "the beam is held on most of the stream ({} frames)",
        stream.len()
    );
    assert!(
        stream
            .iter()
            .all(|frame| frame.held == 1 && frame.live == 1),
        "one beam, one live instance, for the whole stream"
    );
    assert_eq!(errors.len(), stream.len(), "every frame measured its end");
    assert!(
        worst <= BEAM_END_TOLERANCE_PX,
        "the beam's end lies within {BEAM_END_TOLERANCE_PX} px of the target's outline point"
    );
    assert!(
        travel > 20.0,
        "the target walked across the screen ({travel:.1} px)"
    );
    // Review round 2, R3-1: the youngest burn's centre stays on the beam's end.
    let burn_errors: Vec<f32> = stream
        .iter()
        .filter_map(|frame| {
            Some(Vec2::from_array(frame.burn_px?).distance(Vec2::from_array(frame.end_px?)))
        })
        .collect();
    let burn_worst = burn_errors.iter().copied().fold(0.0_f32, f32::max);
    eprintln!(
        "driver {driver}: the youngest burn's centre lies at most {burn_worst:.2} px from the \
         beam's end over {} frames with a burn",
        burn_errors.len()
    );
    assert!(
        burn_errors.len() >= 20,
        "burns are live on most of the stream"
    );
    assert!(
        burn_worst <= BEAM_END_TOLERANCE_PX,
        "the burn follows the beam's end within {BEAM_END_TOLERANCE_PX} px"
    );
    // The sample pair: the beam is drawn along its path and at its end.
    let (drawn, bare, muzzle, end) = report.sample.as_ref().ok_or("the sample was pictured")?;
    let drawn_image = image::open(directory.path().join(drawn))?.to_rgb8();
    let bare_image = image::open(directory.path().join(bare))?.to_rgb8();
    let along = changed_along(
        &drawn_image,
        &bare_image,
        (*muzzle, *end),
        (0.0, 1.0),
        BEAM_PATH_HALF_PX,
    );
    let at_end = changed_along(
        &drawn_image,
        &bare_image,
        (*muzzle, *end),
        (0.97, 1.0),
        BEAM_END_TOLERANCE_PX,
    );
    eprintln!(
        "driver {driver}: sample, {BEAM_SAMPLE_FRAME} frames into the stream: {along} px of the \
         path and {at_end} \
         px within {BEAM_END_TOLERANCE_PX} px of its end changed"
    );
    assert!(along > 20, "the beam is drawn along its path");
    assert!(
        at_end > 0,
        "the beam's end is drawn at the target's outline"
    );
    // After the death: nothing of the beam is live or drawn its exit and a frame later.
    let gone = report
        .frames
        .iter()
        .find(|frame| frame.frame >= report.kill_frame && !frame.target_shown)
        .map(|frame| frame.frame)
        .ok_or("the target died")?;
    let exit_frames = (report.exit / 0.04).ceil() as u32;
    let lingering: Vec<u32> = report
        .frames
        .iter()
        .filter(|frame| frame.frame > gone + exit_frames && frame.live > 0)
        .map(|frame| frame.frame)
        .collect();
    let (drawn, bare, ..) = report.after.as_ref().ok_or("the after pair was pictured")?;
    // The beam's path where it was last held, as that frame drew it (the camera stands still).
    let (muzzle, end) = report
        .frames
        .iter()
        .rev()
        .find_map(|frame| frame.muzzle_px.zip(frame.end_px))
        .ok_or("a beam was held")?;
    let (muzzle, end) = (&muzzle, &end);
    let drawn_image = image::open(directory.path().join(drawn))?.to_rgb8();
    let bare_image = image::open(directory.path().join(bare))?.to_rgb8();
    let remains = changed_along(
        &drawn_image,
        &bare_image,
        (*muzzle, *end),
        BEAM_PATH_SPAN,
        BEAM_PATH_HALF_PX,
    );
    let window_changed = drawn_image
        .pixels()
        .zip(bare_image.pixels())
        .filter(|(a, b)| {
            a.0.iter()
                .zip(b.0.iter())
                .any(|(a, b)| a.abs_diff(*b) >= BEAM_CHANGE_LEVEL)
        })
        .count();
    eprintln!(
        "driver {driver}: after the death, the window's effect layers change {window_changed} px, any death look \
         included; the beam's last muzzle {muzzle:?} and end {end:?}"
    );
    eprintln!(
        "driver {driver}: the target left the presented frame on frame {gone}; exit {:.2} s \
         ({exit_frames} frames); frames with the beam live after that and a frame: \
         {lingering:?}; {remains} px of its former path changed",
        report.exit
    );
    assert!(lingering.is_empty(), "the beam outlives its exit");
    assert_eq!(remains, 0, "no pixel of the beam remains after its exit");
    Ok(())
}

// --- The waves' ground (plan `2026-11-20-gpu-lane` D-3), without a GPU -------------------------

/// The shipped skirmish in the production client without a window, after its first ticks (the
/// fog is first computed by a tick), with the camera still at its boot target.
fn skirmish_without_window() -> battle_harness::Harness {
    let mut harness =
        battle_harness::Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    harness.resume();
    let mut frames = 0;
    while harness.world().current_tick < 2 {
        harness.step(1);
        frames += 1;
        assert!(frames < 10_000, "the battle stopped ticking");
    }
    harness.pause();
    // The look height eases over several frames.
    harness.step(40);
    harness
}

/// Calls `view` with the battle, its fog, the camera and the camera's target.
fn with_view<R>(
    harness: &mut battle_harness::Harness,
    view: impl FnOnce(&SimWorld, &BattleFog, (&Camera, &GlobalTransform), Vec2) -> R,
) -> R {
    let app = harness.app.world_mut();
    let mut cameras = app.query::<(&Camera, &GlobalTransform, &RtsCamera)>();
    let (camera, transform, rts) = cameras.single(app);
    let world = app
        .resource::<BattleSession>()
        .world()
        .expect("committed battle");
    let fog = app.resource::<BattleFog>();
    view(world, fog, (camera, transform), rts.target)
}

/// D-3 item 1: on the shipped skirmish the search pans to a target with room for at least
/// [`MIN_SPOTS`] subjects, and finds the same target every time.
#[test]
fn the_camera_pans_to_ground_with_room_for_the_waves() {
    let mut harness = skirmish_without_window();
    let (first, second, at_start) = with_view(&mut harness, |world, fog, camera, start| {
        let screen = || on_screen(world, camera);
        (
            camera_target(world, fog, start, screen()),
            camera_target(world, fog, start, screen()),
            search_spots(world, fog, start, MAX_SUBJECTS, Sight::OnceStaged, screen()),
        )
    });
    eprintln!(
        "at the boot target {} spots ({}); panned to {:?}: {} spots ({})",
        at_start.spots.len(),
        at_start.rejections(),
        first.0,
        first.1.spots.len(),
        first.1.rejections()
    );
    assert!(
        first.1.spots.len() >= MIN_SPOTS,
        "the camera's target has room for at least {MIN_SPOTS} subjects: {}",
        first.1.rejections()
    );
    assert_eq!(first, second, "the search is deterministic");
}

/// D-3 item 4: at full capacity the subjects plan into at most 8 waves, and they are the
/// subjects planned one a wave: the same families, as their own or lent, from the same
/// approaches (a lent family may ride another matching shooter, since a wave stages each
/// shooter once).
#[test]
fn the_waves_at_capacity_stage_every_subject_of_one_a_wave() {
    let harness = skirmish_without_window();
    let world = harness.world();
    let subjects = |capacity: usize| {
        let (waves, unstaged) = plan(world, capacity);
        let mut subjects: Vec<String> = waves
            .iter()
            .flatten()
            .map(|(_, family, own, approach)| format!("{} {own} {approach:?}", family.label()))
            .collect();
        subjects.sort();
        (waves.len(), subjects, unstaged)
    };
    let (one_waves, one, one_unstaged) = subjects(1);
    let (full_waves, full, full_unstaged) = subjects(MAX_SUBJECTS);
    eprintln!("{one_waves} waves of one subject, {full_waves} at capacity {MAX_SUBJECTS}");
    assert!(full_waves <= 8, "{full_waves} waves at capacity");
    assert!(full_waves < one_waves);
    assert_eq!(full, one, "the same subjects at either capacity");
    assert_eq!(full_unstaged, one_unstaged);
}

/// D-3 item 3: a world crowded so that fewer than [`MIN_SPOTS`] spots exist makes the staging
/// refuse, and the refusal counts the candidates each condition rejected.
#[test]
fn a_crowded_world_makes_the_staging_refuse_with_counts_per_condition() {
    let mut harness = skirmish_without_window();
    let (target, found) = with_view(&mut harness, |world, fog, camera, start| {
        camera_target(world, fog, start, on_screen(world, camera))
    });
    assert!(found.at_least_min(target).is_ok());
    // Units every three world units over the target's surroundings leave no spot clear.
    let crowd = harness
        .world()
        .catalog
        .archetypes()
        .find(|arch| arch.max_speed > SimScalar::ZERO && arch.mover_type == MoverType::Ground)
        .map(|arch| arch.unit_type_id)
        .expect("the catalog has a ground unit");
    let world = harness.world_mut();
    for x in -6..=6 {
        for y in -6..=6 {
            let place = target + Vec2::new(x as f32, y as f32) * 3.0;
            if let Ok(id) = world.spawn_unit(crowd, ENEMY, sim_position(place)) {
                world
                    .entities
                    .get_mut(&id)
                    .expect("just spawned")
                    .is_immobile = true;
            }
        }
    }
    let refusal = with_view(&mut harness, |world, fog, camera, start| {
        let shift = target - start;
        let screen = on_screen(world, camera);
        search_spots(
            world,
            fog,
            target,
            MAX_SUBJECTS,
            Sight::OnceStaged,
            |point| screen(point - shift),
        )
        .at_least_min(target)
        .map(<[Vec2]>::len)
    })
    .expect_err("a crowded world refuses the staging");
    eprintln!("{refusal}");
    assert!(refusal.contains(&format!("at least {MIN_SPOTS} needed")));
    for name in CONDITIONS {
        assert!(refusal.contains(&format!("{name}: ")), "{refusal}");
    }
    assert!(
        !refusal.contains("units: 0,"),
        "the crowd rejected candidates: {refusal}"
    );
}
