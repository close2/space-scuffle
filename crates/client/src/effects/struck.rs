//! Where a hit shows: on the struck unit's own bubble, towards its attacker (plan
//! `2026-11-09-effect-visualization` WP11, reviews 2 to 4: A2-1, A3-1, A3-2, A4-1, E3-5; plan
//! `2026-11-17-effect-snapshot-identity` D-3, Amendments 2, 3 and 7, B-125, B-128 and B-133).
//!
//! # Responsibility
//! The simulation publishes an instant hit (`HitscanTracer`, `BeamVisual`) and a melee strike
//! (`MeleeSlashVisual`) at the struck unit's map position, half a unit above the ground, and a
//! projectile (`ProjectileVisual`) arriving at the edge of its target's collision circle
//! towards the shooter, also half a unit above the ground. Each names the struck entity in the
//! event's [`StrikeEnds`]. Those points lie inside a ground unit's bubble, whose glass is drawn
//! over anything there, and far below a flyer's. This module moves them onto the struck
//! bubble's glass, where the hit shows and where the shot's slug or projectile ends:
//! - [`StruckBubbles::find`] finds the struck bubble **by the struck entity's id**, never by
//!   position: in the presented frame, else in the previous snapshot, else among the bubbles
//!   presented on the frame before this one, else nowhere. The third place serves a killing
//!   hit: its target is reaped on the hit's tick, and the hit is shown once the presentation
//!   reaches that tick, when usually neither snapshot it holds lists the target any more.
//! - [`at_struck_surface`] returns the event with its hit point on the struck bubble's surface.
//!   The horizontal position is the published point (the struck unit's centre on the hit's
//!   tick; for a projectile, its target's centre on the launch tick, recovered from the
//!   published arrival); the radius, drawn height and dome are the found bubble's. The point is moved
//!   towards the attacker, raised [`HIT_ELEVATION`] above the bubble's drawn centre (so a hit on
//!   the side of the bubble away from the camera is still in front of its glass: the camera
//!   looks down at about 58 degrees, and a point 45 degrees up the far side faces it), at the
//!   bubble's radius plus [`SURFACE_MARGIN`].
//! - [`launch_from_shooter`] starts a projectile's flight on its **shooter's** bubble, found by
//!   the shooter's id in the same places ([`StruckBubbles::find_shooter`]): the simulation
//!   publishes the start at the shooter's ground + 0.5, far below a flyer's bubble. The start
//!   goes onto the shooter's glass towards the target, the mirror of the landing rule.
//!
//! # How it fits
//! `effects::systems::spawn_effects_from_events` passes every admitted event through
//! [`at_struck_surface`], and a projectile then through [`launch_from_shooter`], before it asks
//! for the event's travel and impact, so a slug, a beam, its burn and a blow all meet the struck
//! bubble's glass, a flyer's included, whatever stands below or beside it, and a projectile
//! leaves its shooter's glass. Presentation only: the simulation's events and state are read, never
//! changed (D-1). The fog is not consulted here: the event was admitted by its published point
//! (`BattleFog::reveals`), and the bubble's outline shows no more than that point shows.
//!
//! # Invariants
//! - An instant hit or a blow without [`StrikeEnds`] (a synthetic event, a record older than
//!   the field), or whose struck bubble is found nowhere, is moved as if it struck a bubble of
//!   [`DEFAULT_STRUCK_RADIUS`] centred on its published point, the radius its impact binds, so a
//!   look placed `Behind` the hit still lands on that bubble's centre (review 3, E3-5). A
//!   projectile in that case keeps its published flight, as before B-128 was fixed; one whose
//!   shooter is found nowhere keeps its published start, as before B-133 was fixed. Both misses
//!   are counted, apart ([`StruckBubbles::found_in`], [`StruckBubbles::shooter_found_in`]).
//! - A projectile's start never moves more than [`MOST_PULL`] of the way to its arrival.
//! - A shot's hit point never moves past [`MOST_PULL`] of the way back to its muzzle.
//! - Every lookup is a binary search over an id-ordered slice: the snapshot writes its bubbles
//!   in entity order and the presentation keeps that order. The one retained buffer, the last
//!   frame's bubbles, is refilled in place, so steady state allocates nothing.
//! - The last frame's bubbles belong to one battle session: entity ids restart in a new
//!   `SimWorld`, so they are forgotten when the presented session changes or ends
//!   ([`StruckBubbles::follow`], Amendment 3, review condition 3).

use crate::battle_pipeline::commands::SessionGeneration;
use bevy::prelude::*;
use sim_types::snapshot::{BubbleRenderInstance, CombatEventVisual};
use sim_types::{EntityId, ShapeKind, StrikeEnds};

/// World units beyond the struck bubble's radius at which its hit shows, so it is drawn in
/// front of the glass rather than on it.
pub const SURFACE_MARGIN: f32 = 0.12;
/// Largest share of a shot's length its hit point is moved back towards its muzzle.
pub const MOST_PULL: f32 = 0.8;
/// The struck bubble's radius an impact binds (`UnitRadius`) when the bubble is not found.
pub const DEFAULT_STRUCK_RADIUS: f32 = 0.5;
/// Height above a dome's base at which a hit on its rim shows, so it is not sunk in the floor.
pub const DOME_RIM_LIFT: f32 = 0.1;
/// How far up the struck bubble, from its drawn centre, its hit shows: 45 degrees, radians.
pub const HIT_ELEVATION: f32 = std::f32::consts::FRAC_PI_4;
/// World units beyond the shooter's bubble's radius at which its projectile's flight starts:
/// none, so the flight leaves from the glass itself, where the drawn bubble's outline is (plan
/// `2026-11-17-effect-snapshot-identity` Amendment 7, B-133).
pub const LAUNCH_MARGIN: f32 = 0.0;

/// The bubble a hit struck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StruckBubble {
    /// Its glass radius, world units: the snapshot's drawn radius (a dome's is half its
    /// footprint, not the structure's range radius; plan `2026-11-18-structure-radius` D-6).
    pub radius: f32,
    /// Height of its drawn centre, world units (the event's height convention); a dome's base.
    pub height: f32,
    /// Whether it is a structure's dome: a half sphere standing on `height`, with nothing of
    /// it below (review 4, A4-1).
    pub dome: bool,
}

impl StruckBubble {
    /// The struck bubble a presented `bubble` is: its radius, its drawn centre's height (a unit
    /// sphere's centre is its radius above where it stands, a dome's is its base) and whether
    /// it is a dome.
    #[must_use]
    pub fn of(bubble: &BubbleRenderInstance) -> Self {
        let dome = matches!(bubble.shape, ShapeKind::BuildingDome { .. });
        Self {
            radius: bubble.radius,
            height: bubble.world_pos[2] + if dome { 0.0 } else { bubble.radius },
            dome,
        }
    }
}

/// The struck bubble of entity `id` among `bubbles`, which are in entity id order (a
/// snapshot's, or a presented frame's), by binary search.
#[must_use]
pub fn struck_by_id(bubbles: &[BubbleRenderInstance], id: EntityId) -> Option<StruckBubble> {
    bubbles
        .binary_search_by_key(&id, |bubble| bubble.entity_id)
        .ok()
        .and_then(|index| bubbles.get(index))
        .map(StruckBubble::of)
}

/// Where [`StruckBubbles::find`] found a struck bubble, in the order it looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StruckFrom {
    /// The presented frame.
    Presented,
    /// The previous snapshot of the presentation (`BattleFrameState::previous`).
    Previous,
    /// The bubbles presented on the frame before this one.
    LastFrame,
    /// Nowhere: the default bubble is used.
    Nowhere,
}

impl StruckFrom {
    /// Every place, in the order [`StruckBubbles::find`] looks.
    pub const ALL: [Self; 4] = [
        Self::Presented,
        Self::Previous,
        Self::LastFrame,
        Self::Nowhere,
    ];

    /// Index of this place in [`Self::ALL`].
    const fn index(self) -> usize {
        match self {
            Self::Presented => 0,
            Self::Previous => 1,
            Self::LastFrame => 2,
            Self::Nowhere => 3,
        }
    }
}

/// The bubbles a hit's struck entity, or a projectile's shooter, is looked up among beyond the
/// presented frame (those presented on the frame before this one, retained), and a count of
/// where each lookup found its bubble, for the steady-state tests and the evidence.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct StruckBubbles {
    /// The last presented frame's bubbles: entity id and struck bubble, in entity id order.
    last: Vec<(EntityId, StruckBubble)>,
    /// Struck-end lookups per [`StruckFrom`], in the order of [`StruckFrom::ALL`].
    found: [u64; 4],
    /// Shooter lookups (a projectile's launch, B-133) per [`StruckFrom`], in the same order.
    shooters: [u64; 4],
    /// The battle session `last` was kept for; `None` when none is presented.
    session: Option<SessionGeneration>,
}

impl StruckBubbles {
    /// Follows the presented battle session (`None`: none is presented): when it is not the
    /// one the last frame's bubbles were kept for, they are forgotten, because entity ids
    /// restart in a new session's `SimWorld` and an old id would name another unit. Call once
    /// a frame, before that frame's lookups. The buffer keeps its capacity.
    pub fn follow(&mut self, session: Option<SessionGeneration>) {
        if self.session != session {
            self.session = session;
            self.last.clear();
        }
    }

    /// The struck bubble of entity `id`: in `presented` (the presented frame), else in
    /// `previous` (the previous snapshot, when the presentation holds one), else among the
    /// bubbles last given to [`Self::remember`], else nowhere. Each slice is in entity id
    /// order. Counts where it was found.
    pub fn find(
        &mut self,
        presented: &[BubbleRenderInstance],
        previous: Option<&[BubbleRenderInstance]>,
        id: EntityId,
    ) -> Option<StruckBubble> {
        let (found, from) = self.locate(presented, previous, id);
        self.found[from.index()] += 1;
        found
    }

    /// The bubble of a projectile's shooter, entity `id`, looked up as [`Self::find`] looks up
    /// a struck bubble, in the same places and order (plan `2026-11-17-effect-snapshot-identity`
    /// Amendment 7, B-133). Counted apart from the struck-end lookups
    /// ([`Self::shooter_found_in`]).
    pub fn find_shooter(
        &mut self,
        presented: &[BubbleRenderInstance],
        previous: Option<&[BubbleRenderInstance]>,
        id: EntityId,
    ) -> Option<StruckBubble> {
        let (found, from) = self.locate(presented, previous, id);
        self.shooters[from.index()] += 1;
        found
    }

    /// The bubble of entity `id` and where it was found: in `presented`, else in `previous`,
    /// else among the last frame's bubbles, else nowhere. Counts nothing.
    fn locate(
        &self,
        presented: &[BubbleRenderInstance],
        previous: Option<&[BubbleRenderInstance]>,
        id: EntityId,
    ) -> (Option<StruckBubble>, StruckFrom) {
        if let Some(found) = struck_by_id(presented, id) {
            (Some(found), StruckFrom::Presented)
        } else if let Some(found) = previous.and_then(|previous| struck_by_id(previous, id)) {
            (Some(found), StruckFrom::Previous)
        } else if let Some((_, found)) = self
            .last
            .binary_search_by_key(&id, |(entity, _)| *entity)
            .ok()
            .and_then(|index| self.last.get(index))
        {
            (Some(*found), StruckFrom::LastFrame)
        } else {
            (None, StruckFrom::Nowhere)
        }
    }

    /// Keeps `presented` (this frame's presented bubbles, in entity id order) as the bubbles a
    /// later frame's hit may have struck; call once a frame, after this frame's lookups.
    pub fn remember(&mut self, presented: &[BubbleRenderInstance]) {
        self.last.clear();
        self.last.extend(
            presented
                .iter()
                .map(|bubble| (bubble.entity_id, StruckBubble::of(bubble))),
        );
    }

    /// How many lookups found their bubble in `from`.
    #[must_use]
    pub const fn found_in(&self, from: StruckFrom) -> u64 {
        self.found[from.index()]
    }

    /// How many shooter lookups ([`Self::find_shooter`]) found their bubble in `from`.
    #[must_use]
    pub const fn shooter_found_in(&self, from: StruckFrom) -> u64 {
        self.shooters[from.index()]
    }

    /// Address and capacity of the retained buffer, which an allocation would move or grow.
    #[must_use]
    pub fn buffer(&self) -> (usize, usize) {
        (self.last.as_ptr() as usize, self.last.capacity())
    }
}

/// The attacker and the struck entity of an instant hit, a blow or a projectile, when the
/// event names them.
#[must_use]
pub const fn strike_ends(event: &CombatEventVisual) -> Option<StrikeEnds> {
    match event {
        CombatEventVisual::HitscanTracer { ends, .. }
        | CombatEventVisual::BeamVisual { ends, .. }
        | CombatEventVisual::MeleeSlashVisual { ends, .. }
        | CombatEventVisual::ProjectileVisual { ends, .. } => *ends,
        CombatEventVisual::BubbleShatter { .. }
        | CombatEventVisual::SplashVisual { .. }
        | CombatEventVisual::SpellImpact { .. } => None,
    }
}

/// `point` (map `x, y`, height) moved onto the glass of a bubble of `struck` centred on it in
/// plan: towards `towards` (a unit map direction) and [`HIT_ELEVATION`] up from the bubble's
/// drawn centre, at its radius plus `margin` (world units; [`SURFACE_MARGIN`] for a hit,
/// [`LAUNCH_MARGIN`] for a launch), the horizontal move at most `most`.
fn onto_glass(point: &mut [f32; 3], struck: StruckBubble, towards: Vec2, most: f32, margin: f32) {
    let reach = struck.radius + margin;
    let across = (reach * HIT_ELEVATION.cos()).min(most);
    point[0] += towards.x * across;
    point[1] += towards.y * across;
    point[2] = struck.height + reach * HIT_ELEVATION.sin();
}

/// `point` (map `x, y`, height) moved onto the outline of a bubble of `struck` centred on it,
/// as the camera looking along `view` sees it ([`onto_outline`]), or onto its glass without a
/// camera ([`onto_glass`]): towards `towards` (a unit map direction), the horizontal move at
/// most `most`. A held beam's end follows its target this way every frame (plan
/// `2026-11-17-effect-snapshot-identity` D-4).
pub fn onto_struck_outline(
    point: &mut [f32; 3],
    struck: StruckBubble,
    towards: Vec2,
    most: f32,
    view: Option<Vec3>,
) {
    match view {
        Some(view) => onto_outline(point, struck, towards, most, view, SURFACE_MARGIN),
        None => onto_glass(point, struck, towards, most, SURFACE_MARGIN),
    }
}

/// `point` (map `x, y`, height) moved onto the outline of a bubble of `struck` centred on it in
/// plan, as the camera looking along `view` (Bevy world, unit) sees it: the point of the
/// bubble's silhouette, at its radius plus `margin` (world units), on the side of `towards` (a unit
/// map direction) on screen, so a shot's end, its hit and a blow are drawn at the edge of the
/// glass the attack comes from, never in front of the bubble's face or behind it (review 3,
/// A3-2). On a dome, which has nothing below its base, a point that would fall below the base
/// is put on the base rim instead (review 4, A4-1). The horizontal move is at most `most`.
/// Falls back to [`onto_glass`] when `towards` runs along the view.
fn onto_outline(
    point: &mut [f32; 3],
    struck: StruckBubble,
    towards: Vec2,
    most: f32,
    view: Vec3,
    margin: f32,
) {
    let across = Vec3::new(towards.x, 0.0, towards.y);
    let side = across - view * across.dot(view);
    if side.length_squared() < 1e-6 {
        onto_glass(point, struck, towards, most, margin);
        return;
    }
    let reach = struck.radius + margin;
    let mut offset = side.normalize() * reach;
    // A dome has nothing below its base: on the side towards the camera its visible outline is
    // its base rim, so the point stays there, a little above the floor (review 4, A4-1).
    if struck.dome && offset.y < DOME_RIM_LIFT {
        let flat = Vec2::new(offset.x, offset.z).normalize_or(towards) * reach;
        offset = Vec3::new(flat.x, DOME_RIM_LIFT, flat.y);
    }
    let flat = offset.x.hypot(offset.z);
    if flat > most {
        let shrink = most / flat;
        offset.x *= shrink;
        offset.z *= shrink;
    }
    point[0] += offset.x;
    point[1] += offset.z;
    point[2] = struck.height + offset.y;
}

/// `event` with its hit point on the struck bubble's glass: a tracer's or a beam's `to` moved
/// back along the shot, a melee strike's `pos` along its published `angle_rad` (the direction
/// from the struck unit towards its attacker), a projectile's arrival back along its flight
/// (see [`projectile_onto_struck`]), and raised onto the bubble (see [`onto_glass`]; onto the
/// bubble's outline as the camera looking along `view` sees it, when there is one, see
/// [`onto_outline`]). `struck_by_id(id)` gives the bubble of the struck entity `id` the event
/// names in its [`StrikeEnds`] (see [`StruckBubbles::find`]); it is not asked for an event that
/// names none. Every other event is returned as it is. The struck bubble's radius is returned
/// with the event, when it was found, for the impact to bind (`UnitRadius`); for an instant hit
/// or a blow an unfound bubble counts as one of [`DEFAULT_STRUCK_RADIUS`] centred on the
/// published point, and a projectile whose bubble is unfound keeps its published flight.
#[must_use]
pub fn at_struck_surface(
    event: &CombatEventVisual,
    mut struck_by_id: impl FnMut(EntityId) -> Option<StruckBubble>,
    view: Option<Vec3>,
) -> (CombatEventVisual, Option<f32>) {
    let mut moved = event.clone();
    let assumed = |point: &[f32; 3]| StruckBubble {
        radius: DEFAULT_STRUCK_RADIUS,
        height: point[2],
        dome: false,
    };
    let found = strike_ends(event).and_then(|ends| struck_by_id(ends.target));
    match &mut moved {
        CombatEventVisual::HitscanTracer { from, to, .. }
        | CombatEventVisual::BeamVisual { from, to, .. } => {
            let back = Vec2::new(from[0] - to[0], from[1] - to[1]);
            let length = back.length();
            if length > f32::EPSILON {
                let bubble = found.unwrap_or_else(|| assumed(to));
                onto_struck_outline(to, bubble, back / length, length * MOST_PULL, view);
            }
        }
        CombatEventVisual::MeleeSlashVisual { pos, angle_rad, .. } => {
            let bubble = found.unwrap_or_else(|| assumed(pos));
            let towards = Vec2::from_angle(*angle_rad);
            onto_struck_outline(pos, bubble, towards, f32::MAX, view);
        }
        CombatEventVisual::ProjectileVisual {
            pos,
            vel,
            duration_secs,
            ..
        } => {
            let Some(bubble) = found else {
                return (moved, None);
            };
            if !projectile_onto_struck(pos, vel, *duration_secs, bubble, view) {
                return (moved, None);
            }
        }
        CombatEventVisual::BubbleShatter { .. }
        | CombatEventVisual::SplashVisual { .. }
        | CombatEventVisual::SpellImpact { .. } => return (moved, None),
    }
    (moved, found.map(|bubble| bubble.radius))
}

/// Re-aims a projectile launched at `pos` with velocity `vel` (world units per second) for
/// `duration_secs` so that it arrives on the glass of `struck`, its target's bubble (plan
/// `2026-11-17-effect-snapshot-identity` Amendment 3, B-128). Returns whether it was re-aimed;
/// a projectile without a positive flight time is left as it is.
///
/// The simulation publishes the arrival at the edge of the target's drawn bubble towards the
/// shooter, `target - direction * drawn_radius`, at ground + 0.5 (`sim_core`'s
/// `launch_projectile`; a structure's dome is drawn at half its footprint, not at its range
/// radius, plan `2026-11-18-structure-radius` D-6). The bubble's radius in the snapshot is that
/// same drawn radius, so the target's centre on the launch tick is the arrival moved on by
/// `struck.radius` along the flight's horizontal direction. From that centre the arrival is put on the bubble's glass (its outline as the
/// camera looking along `view` sees it, when there is one) towards the shooter, at the bubble's
/// drawn height, as an instant hit's end is, at most [`MOST_PULL`] of the way back to the
/// launch point. The flight time stays; the velocity is recomputed to reach the new arrival, so
/// the travel and the impact (both derived from `pos + vel * duration_secs`) meet there.
fn projectile_onto_struck(
    pos: &[f32; 3],
    vel: &mut [f32; 3],
    duration_secs: Option<f32>,
    struck: StruckBubble,
    view: Option<Vec3>,
) -> bool {
    let flight = duration_secs.unwrap_or(0.0);
    if flight <= f32::EPSILON {
        return false;
    }
    let launch = Vec2::new(pos[0], pos[1]);
    let arrival = launch + Vec2::new(vel[0], vel[1]) * flight;
    let direction = (arrival - launch).normalize_or_zero();
    let centre = arrival + direction * struck.radius;
    let back = launch - centre;
    let length = back.length();
    let towards = back.normalize_or_zero();
    let mut point = [centre.x, centre.y, pos[2] + vel[2] * flight];
    onto_struck_outline(&mut point, struck, towards, length * MOST_PULL, view);
    for (axis, velocity) in vel.iter_mut().enumerate() {
        *velocity = (point[axis] - pos[axis]) / flight;
    }
    true
}

/// `event` with its hit point on its struck bubble ([`at_struck_surface`]) and, for a
/// projectile, its flight started on its shooter's bubble ([`launch_from_shooter`]), both
/// bubbles found by id through `struck` in `presented` (the presented frame) and `previous`
/// (the previous snapshot), each slice in entity id order. Without `struck` (an app without the
/// effect system's lookup) nothing is found, so a projectile keeps its published flight. Returns
/// the struck bubble's radius with the event, as [`at_struck_surface`] does.
#[must_use]
pub fn on_bubbles(
    event: &CombatEventVisual,
    struck: Option<&mut StruckBubbles>,
    presented: &[BubbleRenderInstance],
    previous: Option<&[BubbleRenderInstance]>,
    view: Option<Vec3>,
) -> (CombatEventVisual, Option<f32>) {
    let Some(struck) = struck else {
        return at_struck_surface(event, |_| None, view);
    };
    let (mut moved, radius) =
        at_struck_surface(event, |id| struck.find(presented, previous, id), view);
    launch_from_shooter(
        &mut moved,
        event,
        |id| struck.find_shooter(presented, previous, id),
        view,
    );
    (moved, radius)
}

/// `moved`, a projectile already aimed at its struck bubble ([`at_struck_surface`]), with its
/// flight started on the glass of its shooter's bubble towards its target (plan
/// `2026-11-17-effect-snapshot-identity` Amendment 7, B-133, the mirror of B-128's landing).
/// `published` is the event as the simulation published it; `shooter_by_id(id)` gives the
/// bubble of the shooter `id` its [`StrikeEnds`] name (see [`StruckBubbles::find_shooter`]).
/// Returns whether the start was moved.
///
/// The simulation publishes the start at the edge of the shooter's drawn bubble towards the
/// target, `shooter + direction * drawn_radius`, at ground + 0.5 (`sim_core`'s
/// `launch_projectile`), so a flyer's projectile leaves from about 2.5 units below its bubble.
/// The bubble's radius in the snapshot is that same drawn radius, so the shooter's centre on the
/// launch tick is the published start moved back by `shooter.radius` along the published
/// flight's horizontal direction. From that centre the start is put on the bubble's outline as
/// the camera looking along `view` sees it (on its glass without a camera), towards the target,
/// at its drawn height and [`LAUNCH_MARGIN`] out, the horizontal move at most [`MOST_PULL`] of
/// the way to the arrival. The arrival and the flight time stay; the velocity is recomputed to
/// reach the arrival from the new start.
///
/// Anything other than a projectile naming its shooter, a projectile without a positive flight
/// time (neither asks for a lookup), and a projectile whose shooter is found nowhere (dead or
/// unseen on the launch frame; the lookup counts it) keep their start.
pub fn launch_from_shooter(
    moved: &mut CombatEventVisual,
    published: &CombatEventVisual,
    mut shooter_by_id: impl FnMut(EntityId) -> Option<StruckBubble>,
    view: Option<Vec3>,
) -> bool {
    let CombatEventVisual::ProjectileVisual {
        pos: published_pos,
        vel: published_vel,
        ends: Some(ends),
        ..
    } = published
    else {
        return false;
    };
    let CombatEventVisual::ProjectileVisual {
        pos,
        vel,
        duration_secs,
        ..
    } = moved
    else {
        return false;
    };
    let flight = duration_secs.unwrap_or(0.0);
    if flight <= f32::EPSILON {
        return false;
    }
    let Some(shooter) = shooter_by_id(ends.attacker) else {
        return false;
    };
    let direction = Vec2::new(published_vel[0], published_vel[1]).normalize_or_zero();
    let centre = Vec2::new(published_pos[0], published_pos[1]) - direction * shooter.radius;
    let arrival = [
        pos[0] + vel[0] * flight,
        pos[1] + vel[1] * flight,
        pos[2] + vel[2] * flight,
    ];
    let ahead = Vec2::new(arrival[0], arrival[1]) - centre;
    let mut start = [centre.x, centre.y, published_pos[2]];
    let most = ahead.length() * MOST_PULL;
    match view {
        Some(view) => onto_outline(&mut start, shooter, direction, most, view, LAUNCH_MARGIN),
        None => onto_glass(&mut start, shooter, direction, most, LAUNCH_MARGIN),
    }
    *pos = start;
    for (axis, velocity) in vel.iter_mut().enumerate() {
        *velocity = (arrival[axis] - start[axis]) / flight;
    }
    true
}

/// The direction an instant or melee hit came from, in radians in the ground plane (Bevy
/// `atan2(z, x)`, which is the map's `atan2(y, x)`): from the hit point towards the attacker.
/// `None` for an event that is no such hit.
#[must_use]
pub fn hit_facing(event: &CombatEventVisual) -> Option<f32> {
    match event {
        CombatEventVisual::HitscanTracer { from, to, .. }
        | CombatEventVisual::BeamVisual { from, to, .. } => {
            Some((from[1] - to[1]).atan2(from[0] - to[0]))
        }
        CombatEventVisual::MeleeSlashVisual { angle_rad, .. } => Some(*angle_rad),
        CombatEventVisual::ProjectileVisual { vel, .. } => Some((-vel[1]).atan2(-vel[0])),
        CombatEventVisual::BubbleShatter { .. }
        | CombatEventVisual::SplashVisual { .. }
        | CombatEventVisual::SpellImpact { .. } => None,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed test is a panic"
)]
mod tests {
    use super::*;
    use crate::battle_pipeline::timeline::BattleFrameState;
    use sim_types::{
        AnimState, Concealment, EffectSource, PlayerId, SilhouetteClass, StanceView, StatusFlags,
        Tick, UnitTypeId, WeaponKey, WeaponSlot, WeaponVisualKind,
    };

    /// The attacker of every test event.
    const ATTACKER: EntityId = EntityId(1);

    /// The hit point of an instant hit or a blow; NaN for anything else, which fails every
    /// comparison below.
    fn hit_point(event: &CombatEventVisual) -> [f32; 3] {
        match event {
            CombatEventVisual::HitscanTracer { to, .. } => *to,
            CombatEventVisual::MeleeSlashVisual { pos, .. } => *pos,
            _ => [f32::NAN; 3],
        }
    }

    fn source(slot: WeaponSlot) -> EffectSource {
        EffectSource {
            weapon: WeaponKey { slot, mode: None },
            ..EffectSource::default()
        }
    }

    /// A tracer from `from` to `to` that struck `target` (`None`: a synthetic event).
    fn tracer(
        from: [f32; 3],
        to: [f32; 3],
        slot: WeaponSlot,
        target: Option<EntityId>,
    ) -> CombatEventVisual {
        CombatEventVisual::HitscanTracer {
            from,
            to,
            owner: PlayerId(0),
            source: source(slot),
            ends: target.map(|target| StrikeEnds {
                attacker: ATTACKER,
                target,
            }),
        }
    }

    /// A presented unit bubble of entity `id` at map `at` on ground at 0, standing `altitude`
    /// up (a flyer's 2.5).
    fn bubble(id: u32, at: [f32; 2], altitude: f32, radius: f32) -> BubbleRenderInstance {
        BubbleRenderInstance {
            entity_id: EntityId(id),
            shape: ShapeKind::UnitSphere,
            silhouette: SilhouetteClass::HeavyHex,
            unit_type: UnitTypeId::default(),
            owner: PlayerId(1),
            world_pos: [at[0], at[1], altitude],
            ground_z: 0.0,
            radius,
            velocity: [0.0; 2],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: false,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: AnimState::Idle,
            attack_swing: None,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: false,
            concealment: Concealment::None,
            speed_boosted: false,
            stance: StanceView::default(),
            status: StatusFlags::default(),
            barrier_ratio: 0.0,
        }
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// The hit point of `event` moved by the lookup of `struck` over `presented` and
    /// `previous`, without a camera.
    fn moved_by(
        event: &CombatEventVisual,
        struck: &mut StruckBubbles,
        presented: &[BubbleRenderInstance],
        previous: Option<&[BubbleRenderInstance]>,
    ) -> ([f32; 3], Option<f32>) {
        let (moved, radius) =
            at_struck_surface(event, |id| struck.find(presented, previous, id), None);
        (hit_point(&moved), radius)
    }

    #[test]
    fn a_tracer_ends_on_the_struck_bubbles_glass_towards_its_muzzle_and_up() {
        let bubble = StruckBubble {
            radius: 0.5,
            height: 0.5,
            dome: false,
        };
        let shot = tracer(
            [0.0, 0.0, 0.5],
            [4.0, 0.0, 0.5],
            WeaponSlot::Primary,
            Some(EntityId(2)),
        );
        let (moved, radius) = at_struck_surface(&shot, |_| Some(bubble), None);
        assert_eq!(radius, Some(0.5));
        let to = hit_point(&moved);
        let reach = 0.5 + SURFACE_MARGIN;
        assert!(near(to[0], 4.0 - reach * HIT_ELEVATION.cos()), "{to:?}");
        assert!(near(to[1], 0.0) && near(to[2], 0.5 + reach * HIT_ELEVATION.sin()));
        assert_eq!(
            moved,
            tracer([0.0, 0.0, 0.5], to, WeaponSlot::Primary, Some(EntityId(2)))
        );
    }

    #[test]
    fn a_hit_takes_its_targets_bubble_by_id_not_its_neighbours() {
        // The target (id 4, radius 0.4) stood at (5, 0) on the hit's tick; it has since moved
        // 0.3 on, and a bigger neighbour (id 3, radius 0.9) now stands nearer the published
        // point. The hit takes the target's own radius and height.
        let presented = [
            bubble(3, [5.05, 0.0], 0.0, 0.9),
            bubble(4, [5.3, 0.0], 0.0, 0.4),
        ];
        let shot = tracer(
            [0.0, 0.0, 0.5],
            [5.0, 0.0, 0.5],
            WeaponSlot::Ground,
            Some(EntityId(4)),
        );
        let mut struck = StruckBubbles::default();
        let (to, radius) = moved_by(&shot, &mut struck, &presented, None);
        assert_eq!(radius, Some(0.4), "the target's own radius");
        let reach = 0.4 + SURFACE_MARGIN;
        assert!(near(to[0], 5.0 - reach * HIT_ELEVATION.cos()), "{to:?}");
        assert!(near(to[2], 0.4 + reach * HIT_ELEVATION.sin()), "{to:?}");
        assert_eq!(struck.found_in(StruckFrom::Presented), 1);
    }

    /// B-125: a `Primary` weapon's shot at a flyer hovering over a packed ground group. The
    /// published point is the flyer's centre at ground + 0.5, inside the ground unit under it;
    /// the hit is drawn on the flyer's glass, 2.5 units above the ground units' glass.
    #[test]
    fn a_primary_shot_at_a_flyer_over_a_crowd_is_drawn_on_the_flyer() {
        let presented = [
            bubble(10, [8.0, 8.0], 0.0, 0.5),
            bubble(11, [8.9, 8.0], 0.0, 0.5),
            bubble(12, [7.1, 8.0], 0.0, 0.5),
            bubble(13, [8.0, 8.0], 2.5, 0.5),
        ];
        let shot = tracer(
            [2.0, 8.0, 0.5],
            [8.0, 8.0, 0.5],
            WeaponSlot::Primary,
            Some(EntityId(13)),
        );
        let mut struck = StruckBubbles::default();
        let (to, radius) = moved_by(&shot, &mut struck, &presented, None);
        assert_eq!(radius, Some(0.5));
        let glass = (0.5 + SURFACE_MARGIN) * HIT_ELEVATION.sin();
        assert!(
            near(to[2], 2.5 + 0.5 + glass),
            "on the flyer's glass: {to:?}"
        );
        // The ground unit under it would have put the hit 2.5 units lower.
        let under = moved_by(
            &tracer(
                [2.0, 8.0, 0.5],
                [8.0, 8.0, 0.5],
                WeaponSlot::Primary,
                Some(EntityId(10)),
            ),
            &mut struck,
            &presented,
            None,
        )
        .0;
        assert!(near(to[2] - under[2], 2.5), "{to:?} over {under:?}");
    }

    /// A killing hit: its target was reaped on the hit's tick. When the presentation shows that
    /// tick exactly, the previous snapshot still lists the target.
    #[test]
    fn a_killing_hit_finds_its_target_in_the_previous_frame() {
        let previous = [
            bubble(20, [3.0, 3.0], 0.0, 0.6),
            bubble(21, [6.0, 3.0], 0.0, 0.3),
        ];
        let presented = [bubble(21, [6.0, 3.0], 0.0, 0.3)];
        let blow = CombatEventVisual::MeleeSlashVisual {
            pos: [3.0, 3.0, 0.5],
            angle_rad: 0.0,
            radius: 0.5,
            owner: PlayerId(0),
            source: EffectSource::default(),
            ends: Some(StrikeEnds {
                attacker: EntityId(21),
                target: EntityId(20),
            }),
        };
        let mut struck = StruckBubbles::default();
        let (pos, radius) = moved_by(&blow, &mut struck, &presented, Some(&previous));
        assert_eq!(radius, Some(0.6));
        let reach = 0.6 + SURFACE_MARGIN;
        assert!(near(pos[0], 3.0 + reach * HIT_ELEVATION.cos()), "{pos:?}");
        assert_eq!(struck.found_in(StruckFrom::Previous), 1);
    }

    /// Amendment 2: the common case of a killing hit. Its event is shown once the presentation
    /// reaches the hit's tick, which it does while interpolating from that tick towards the
    /// next: both snapshots it holds then lack the target. The bubbles presented on the frame
    /// before, interpolated towards the hit's tick, still hold it.
    #[test]
    fn a_killing_hit_shown_while_interpolating_finds_its_target_in_the_last_frame() {
        let mut frames = BattleFrameState::default();
        let target = bubble(30, [4.0, 4.0], 2.5, 0.7);
        let shooter = bubble(31, [1.0, 4.0], 0.0, 0.4);
        let initial = frames.initialize(SessionGeneration::FIRST).expect("fresh");
        initial.bubbles = vec![target.clone(), shooter.clone()];
        frames.finish_initialization().expect("initialized");
        let hit = tracer(
            [1.0, 4.0, 0.5],
            [4.0, 4.0, 0.5],
            WeaponSlot::Air,
            Some(EntityId(30)),
        );
        // Tick 1: the hit kills the target, which is reaped on that tick.
        let tick = frames.begin_tick().expect("ready");
        tick.tick = Tick(1);
        tick.bubbles = vec![shooter.clone()];
        tick.combat_events = vec![hit];
        frames.finish_tick().expect("published");
        let mut struck = StruckBubbles::default();
        // The frame that ran tick 1 shows tick 0 interpolated towards it, the target present.
        frames.present(0.5).expect("presented");
        assert!(frames.pop_presented_event().is_none(), "not yet shown");
        struck.remember(&frames.presented().bubbles);
        // The next frame runs tick 2 and shows tick 1 interpolated towards it: the hit pops.
        let tick = frames.begin_tick().expect("ready");
        tick.tick = Tick(2);
        tick.bubbles = vec![shooter];
        frames.finish_tick().expect("published");
        frames.present(0.25).expect("presented");
        let event = frames
            .pop_presented_event()
            .expect("the hit is shown")
            .event;
        let presented = &frames.presented().bubbles;
        let previous = frames
            .previous()
            .map(|snapshot| snapshot.bubbles.as_slice());
        assert!(struck_by_id(presented, EntityId(30)).is_none());
        assert!(previous.is_some_and(|previous| struck_by_id(previous, EntityId(30)).is_none()));
        let (to, radius) = moved_by(&event, &mut struck, presented, previous);
        assert_eq!(radius, Some(0.7), "the flyer's own bubble");
        assert!(to[2] > 2.5 + 0.7, "on the flyer's glass: {to:?}");
        assert_eq!(struck.found_in(StruckFrom::LastFrame), 1);
    }

    #[test]
    fn an_event_without_ends_counts_as_the_default_bubble() {
        let presented = [bubble(40, [2.0, 2.0], 2.5, 0.9)];
        let mut struck = StruckBubbles::default();
        let blow = CombatEventVisual::MeleeSlashVisual {
            pos: [2.0, 2.0, 0.5],
            angle_rad: std::f32::consts::FRAC_PI_2,
            radius: 0.5,
            owner: PlayerId(1),
            source: EffectSource::default(),
            ends: None,
        };
        let (pos, radius) = moved_by(&blow, &mut struck, &presented, None);
        assert_eq!(radius, None);
        let reach = DEFAULT_STRUCK_RADIUS + SURFACE_MARGIN;
        assert!(
            near(pos[0], 2.0) && near(pos[1], 2.0 + reach * HIT_ELEVATION.cos()),
            "the default bubble's surface: {pos:?}"
        );
        assert!(near(pos[2], 0.5 + reach * HIT_ELEVATION.sin()), "{pos:?}");
        assert_eq!(
            StruckFrom::ALL.map(|from| struck.found_in(from)),
            [0; 4],
            "no lookup is asked for"
        );
        // A struck entity found nowhere is counted and counts as the default too.
        let shot = tracer(
            [0.0, 2.0, 0.5],
            [2.0, 2.0, 0.5],
            WeaponSlot::Primary,
            Some(EntityId(99)),
        );
        let (_, radius) = moved_by(&shot, &mut struck, &presented, None);
        assert_eq!(radius, None);
        assert_eq!(struck.found_in(StruckFrom::Nowhere), 1);
        assert_eq!(hit_facing(&blow), Some(std::f32::consts::FRAC_PI_2));
    }

    #[test]
    fn a_hit_on_a_flyer_rises_to_its_glass() {
        let flyer = StruckBubble {
            radius: 0.6,
            height: 3.1,
            dome: false,
        };
        let shot = tracer(
            [0.0, 0.0, 0.5],
            [4.0, 0.0, 0.5],
            WeaponSlot::Air,
            Some(EntityId(2)),
        );
        let to = hit_point(&at_struck_surface(&shot, |_| Some(flyer), None).0);
        assert!(
            near(to[2], 3.1 + (0.6 + SURFACE_MARGIN) * HIT_ELEVATION.sin()),
            "{to:?}"
        );
    }

    #[test]
    fn a_shot_ends_on_the_outline_the_camera_sees() {
        let bubble = StruckBubble {
            radius: 0.5,
            height: 0.5,
            dome: false,
        };
        let target = Some(EntityId(2));
        // A camera looking down at 58 degrees towards +z (the map's +y).
        let pitch = 58.0_f32.to_radians();
        let view = Vec3::new(0.0, -pitch.sin(), pitch.cos());
        // Shot from the camera's side (map -y): its end on the lower outline, towards the
        // camera and down; from the far side: on the upper outline, away and up.
        let below = tracer(
            [4.0, -4.0, 0.5],
            [4.0, 0.0, 0.5],
            WeaponSlot::Primary,
            target,
        );
        let to = hit_point(&at_struck_surface(&below, |_| Some(bubble), Some(view)).0);
        let reach = 0.5 + SURFACE_MARGIN;
        assert!(
            near(to[1], -reach * pitch.sin()) && near(to[2], 0.5 - reach * pitch.cos()),
            "{to:?}"
        );
        let far = tracer(
            [4.0, 4.0, 0.5],
            [4.0, 0.0, 0.5],
            WeaponSlot::Primary,
            target,
        );
        let to = hit_point(&at_struck_surface(&far, |_| Some(bubble), Some(view)).0);
        assert!(
            near(to[1], reach * pitch.sin()) && near(to[2], 0.5 + reach * pitch.cos()),
            "{to:?}"
        );
        // Across the view: the outline's side, level with the centre.
        let across = tracer(
            [0.0, 0.0, 0.5],
            [4.0, 0.0, 0.5],
            WeaponSlot::Primary,
            target,
        );
        let to = hit_point(&at_struck_surface(&across, |_| Some(bubble), Some(view)).0);
        assert!(near(to[0], 4.0 - reach) && near(to[2], 0.5), "{to:?}");
    }

    #[test]
    fn a_dome_is_struck_on_its_visible_outline_never_below_its_base() {
        // A dome of radius 2 standing on the floor at 0 (review 4, A4-1).
        let dome = StruckBubble {
            radius: 2.0,
            height: 0.0,
            dome: true,
        };
        let target = Some(EntityId(2));
        let pitch = 58.0_f32.to_radians();
        let view = Vec3::new(0.0, -pitch.sin(), pitch.cos());
        // From below on screen (the camera's side), across, and from above.
        for (from, name) in [
            ([10.0, -10.0, 0.5], "from below"),
            ([0.0, 0.0, 0.5], "across"),
            ([10.0, 10.0, 0.5], "from above"),
        ] {
            let shot = tracer(from, [10.0, 0.0, 0.5], WeaponSlot::Primary, target);
            let to = hit_point(&at_struck_surface(&shot, |_| Some(dome), Some(view)).0);
            let flat = (to[0] - 10.0).hypot(to[1]);
            assert!(to[2] >= 0.0, "{name}: at or above the base: {to:?}");
            // On the outline: as far out as the dome's radius in plan, or up its far side.
            assert!(
                flat >= 2.0 - 1e-4 || to[2] > 2.0 * 0.5,
                "{name}: outside the dome: {to:?}"
            );
        }
        // From below: on the base rim, towards the camera, lifted off the floor.
        let shot = tracer(
            [10.0, -10.0, 0.5],
            [10.0, 0.0, 0.5],
            WeaponSlot::Primary,
            target,
        );
        let to = hit_point(&at_struck_surface(&shot, |_| Some(dome), Some(view)).0);
        assert!(
            near(to[2], DOME_RIM_LIFT) && near(to[1], -(2.0 + SURFACE_MARGIN)),
            "{to:?}"
        );
        // A presented dome's struck bubble stands on its base.
        let mut presented = bubble(50, [10.0, 0.0], 0.3, 2.0);
        presented.shape = ShapeKind::BuildingDome {
            footprint_w: 3,
            footprint_h: 3,
        };
        assert_eq!(
            StruckBubble::of(&presented),
            StruckBubble {
                radius: 2.0,
                height: 0.3,
                dome: true,
            }
        );
    }

    #[test]
    fn a_short_shot_is_never_pulled_past_its_muzzle() {
        let bubble = StruckBubble {
            radius: 2.0,
            height: 0.5,
            dome: false,
        };
        let shot = tracer(
            [0.0, 0.0, 0.5],
            [0.5, 0.0, 0.5],
            WeaponSlot::Primary,
            Some(EntityId(2)),
        );
        let to = hit_point(&at_struck_surface(&shot, |_| Some(bubble), None).0);
        assert!(near(to[0], 0.5 * (1.0 - MOST_PULL)), "{to:?}");
    }

    /// A projectile launched at `from` that the simulation published arriving at `to` after
    /// one second, fired at `target` (`None`: a synthetic event).
    fn projectile(from: [f32; 3], to: [f32; 3], target: Option<EntityId>) -> CombatEventVisual {
        CombatEventVisual::ProjectileVisual {
            pos: from,
            vel: [to[0] - from[0], to[1] - from[1], to[2] - from[2]],
            owner: PlayerId(0),
            duration_secs: Some(1.0),
            source: source(WeaponSlot::Ground),
            ends: target.map(|target| StrikeEnds {
                attacker: ATTACKER,
                target,
            }),
        }
    }

    /// Where a projectile arrives: its launch point plus its velocity over its flight.
    fn arrival(event: &CombatEventVisual) -> [f32; 3] {
        match event {
            CombatEventVisual::ProjectileVisual {
                pos,
                vel,
                duration_secs: Some(flight),
                ..
            } => [
                pos[0] + vel[0] * flight,
                pos[1] + vel[1] * flight,
                pos[2] + vel[2] * flight,
            ],
            _ => [f32::NAN; 3],
        }
    }

    /// Distance from `point` to the drawn centre of a unit bubble at map `at`, standing
    /// `altitude` up with `radius`.
    fn from_centre(point: [f32; 3], at: [f32; 2], altitude: f32, radius: f32) -> f32 {
        Vec3::new(
            point[0] - at[0],
            point[1] - at[1],
            point[2] - (altitude + radius),
        )
        .length()
    }

    /// B-128 (Amendment 3), a ground target: the simulation publishes the arrival at the edge
    /// of the target's collision circle at ground + 0.5, inside its bubble. The projectile is
    /// re-aimed onto the bubble's glass towards the shooter, never at its centre.
    #[test]
    fn a_projectile_at_a_ground_unit_lands_on_its_glass() {
        let presented = [bubble(2, [5.0, 0.0], 0.0, 0.5)];
        let shot = projectile([1.0, 0.0, 0.5], [4.5, 0.0, 0.5], Some(EntityId(2)));
        let mut struck = StruckBubbles::default();
        let (moved, radius) =
            at_struck_surface(&shot, |id| struck.find(&presented, None, id), None);
        assert_eq!(radius, Some(0.5), "the target's own radius");
        assert_eq!(struck.found_in(StruckFrom::Presented), 1);
        let to = arrival(&moved);
        let reach = 0.5 + SURFACE_MARGIN;
        assert!(near(to[0], 5.0 - reach * HIT_ELEVATION.cos()), "{to:?}");
        assert!(near(to[1], 0.0), "{to:?}");
        assert!(near(to[2], 0.5 + reach * HIT_ELEVATION.sin()), "{to:?}");
        assert!(
            near(from_centre(to, [5.0, 0.0], 0.0, 0.5), reach),
            "on the glass, outside the interior: {to:?}"
        );
        assert!(
            from_centre(arrival(&shot), [5.0, 0.0], 0.0, 0.5) <= 0.5 + 1e-4,
            "the published arrival lies within the bubble's radius, short of its glass"
        );
        // The launch, the owner, the source and the flight time are kept.
        let CombatEventVisual::ProjectileVisual {
            pos, duration_secs, ..
        } = &moved
        else {
            panic!("still a projectile: {moved:?}");
        };
        assert_eq!((*pos, *duration_secs), ([1.0, 0.0, 0.5], Some(1.0)));
        assert_eq!(moved.owner(), shot.owner());
        assert_eq!(moved.source(), shot.source());
    }

    /// B-128 (Amendment 3), a flyer over a ground unit: the projectile lands on the flyer's
    /// glass, 2.5 units above the ground unit's, not at its published point below it.
    #[test]
    fn a_projectile_at_a_flyer_lands_on_its_glass_not_below_it() {
        let presented = [
            bubble(3, [6.0, 2.0], 0.0, 0.6),
            bubble(4, [6.0, 2.0], 2.5, 0.6),
        ];
        // Fired from (2, 2) along +x: the arrival is published 0.6 short of the flyer's centre.
        let shot = projectile([2.0, 2.0, 0.5], [5.4, 2.0, 0.5], Some(EntityId(4)));
        let mut struck = StruckBubbles::default();
        let view = {
            let pitch = 58.0_f32.to_radians();
            Vec3::new(0.0, -pitch.sin(), pitch.cos())
        };
        let (moved, radius) =
            at_struck_surface(&shot, |id| struck.find(&presented, None, id), Some(view));
        assert_eq!(radius, Some(0.6));
        let to = arrival(&moved);
        let reach = 0.6 + SURFACE_MARGIN;
        // Across the view: on the outline's side towards the shooter, level with the flyer's
        // drawn centre.
        assert!(
            near(to[0], 6.0 - reach) && near(to[1], 2.0) && near(to[2], 2.5 + 0.6),
            "{to:?}"
        );
        assert!(near(from_centre(to, [6.0, 2.0], 2.5, 0.6), reach), "{to:?}");
        // The same shot at the ground unit under it lands 2.5 units lower.
        let under = arrival(
            &at_struck_surface(
                &projectile([2.0, 2.0, 0.5], [5.4, 2.0, 0.5], Some(EntityId(3))),
                |id| struck.find(&presented, None, id),
                Some(view),
            )
            .0,
        );
        assert!(near(to[2] - under[2], 2.5), "{to:?} over {under:?}");
    }

    /// A projectile whose target is found nowhere, or which names none, keeps its published
    /// flight (Amendment 3); the miss is counted under `Nowhere`.
    #[test]
    fn a_projectile_whose_target_is_unknown_keeps_its_flight() {
        let presented = [bubble(5, [3.0, 3.0], 0.0, 0.5)];
        let mut struck = StruckBubbles::default();
        let lost = projectile([0.0, 3.0, 0.5], [2.5, 3.0, 0.5], Some(EntityId(77)));
        let (moved, radius) =
            at_struck_surface(&lost, |id| struck.find(&presented, None, id), None);
        assert_eq!((moved, radius), (lost, None));
        assert_eq!(struck.found_in(StruckFrom::Nowhere), 1);
        let synthetic = projectile([0.0, 3.0, 0.5], [2.5, 3.0, 0.5], None);
        let (moved, radius) =
            at_struck_surface(&synthetic, |id| struck.find(&presented, None, id), None);
        assert_eq!((moved, radius), (synthetic, None));
        assert_eq!(
            StruckFrom::ALL.map(|from| struck.found_in(from)),
            [0, 0, 0, 1],
            "an event without ends asks for no lookup"
        );
    }

    /// Where a projectile's flight starts: its launch point.
    fn start(event: &CombatEventVisual) -> [f32; 3] {
        match event {
            CombatEventVisual::ProjectileVisual { pos, .. } => *pos,
            _ => [f32::NAN; 3],
        }
    }

    /// `shot` aimed at its struck bubble and launched from its shooter's, both looked up in
    /// `presented` through `struck`, the camera looking along `view`.
    fn launched(
        shot: &CombatEventVisual,
        struck: &mut StruckBubbles,
        presented: &[BubbleRenderInstance],
        view: Option<Vec3>,
    ) -> (CombatEventVisual, bool) {
        let (mut moved, _) = at_struck_surface(shot, |id| struck.find(presented, None, id), view);
        let started = launch_from_shooter(
            &mut moved,
            shot,
            |id| struck.find_shooter(presented, None, id),
            view,
        );
        (moved, started)
    }

    /// B-133 (Amendment 7): an anti-air flyer's projectile. The simulation publishes its start
    /// at the edge of the shooter's bubble at ground + 0.5, 2.5 units below the bubble. The
    /// flight starts on the shooter's own glass towards the target, and still lands on the
    /// target's glass.
    #[test]
    fn a_flyer_shooters_flight_starts_on_its_glass_towards_the_target() {
        // The shooter (ATTACKER, a flyer of radius 0.6) at (2, 2); its target, a flyer of
        // radius 0.5, at (6, 2). Published: from 0.6 past the shooter's centre to 0.5 short of
        // the target's.
        let presented = [
            bubble(1, [2.0, 2.0], 2.5, 0.6),
            bubble(2, [6.0, 2.0], 2.5, 0.5),
        ];
        let shot = projectile([2.6, 2.0, 0.5], [5.5, 2.0, 0.5], Some(EntityId(2)));
        let pitch = 58.0_f32.to_radians();
        let view = Vec3::new(0.0, -pitch.sin(), pitch.cos());
        let mut struck = StruckBubbles::default();
        let (moved, started) = launched(&shot, &mut struck, &presented, Some(view));
        assert!(started);
        assert_eq!(struck.shooter_found_in(StruckFrom::Presented), 1);
        assert_eq!(struck.found_in(StruckFrom::Presented), 1, "counted apart");
        let from = start(&moved);
        // Across the view: on the outline's side towards the target, level with the shooter's
        // drawn centre, on its glass.
        assert!(
            near(from[0], 2.0 + 0.6 + LAUNCH_MARGIN) && near(from[1], 2.0),
            "{from:?}"
        );
        assert!(
            near(from[2], 2.5 + 0.6),
            "on the flyer, not below it: {from:?}"
        );
        assert!(
            near(from_centre(from, [2.0, 2.0], 2.5, 0.6), 0.6 + LAUNCH_MARGIN),
            "{from:?}"
        );
        // The arrival found by the struck end is kept.
        let (aimed, _) =
            at_struck_surface(&shot, |id| struck.find(&presented, None, id), Some(view));
        let (to, kept) = (arrival(&moved), arrival(&aimed));
        assert!(
            (0..3).all(|axis| near(to[axis], kept[axis])),
            "{to:?} against {kept:?}"
        );
        // The event wiring's one call does both ends; without the lookup, neither moves.
        assert_eq!(
            on_bubbles(&shot, Some(&mut struck), &presented, None, Some(view)).0,
            moved
        );
        assert_eq!(
            on_bubbles(&shot, None, &presented, None, Some(view)),
            (shot.clone(), None)
        );
        // Without a camera: on the glass, towards the target and up.
        let from = start(&launched(&shot, &mut struck, &presented, None).0);
        let reach = 0.6 + LAUNCH_MARGIN;
        assert!(
            near(from[0], 2.0 + reach * HIT_ELEVATION.cos())
                && near(from[2], 2.5 + 0.6 + reach * HIT_ELEVATION.sin()),
            "{from:?}"
        );
    }

    /// B-133 (Amendment 7): a ground shooter's start only moves onto its own glass, found by
    /// its id, never onto a flyer hovering over it.
    #[test]
    fn a_ground_shooters_start_moves_only_to_its_own_glass() {
        let presented = [
            bubble(1, [2.0, 2.0], 0.0, 0.5),
            bubble(2, [6.0, 2.0], 0.0, 0.5),
            bubble(3, [2.0, 2.0], 2.5, 0.9),
        ];
        let shot = projectile([2.5, 2.0, 0.5], [5.5, 2.0, 0.5], Some(EntityId(2)));
        let mut struck = StruckBubbles::default();
        let (moved, started) = launched(&shot, &mut struck, &presented, None);
        assert!(started);
        let from = start(&moved);
        let reach = 0.5 + LAUNCH_MARGIN;
        assert!(
            near(from_centre(from, [2.0, 2.0], 0.0, 0.5), reach),
            "on its own glass: {from:?}"
        );
        assert!(
            near(from[0], 2.0 + reach * HIT_ELEVATION.cos())
                && near(from[1], 2.0)
                && near(from[2], 0.5 + reach * HIT_ELEVATION.sin()),
            "towards the target and up its own glass: {from:?}"
        );
        assert!(
            from[2] < 2.5,
            "never on the flyer hovering over it: {from:?}"
        );
        // The flight time is kept.
        let CombatEventVisual::ProjectileVisual { duration_secs, .. } = &moved else {
            panic!("still a projectile: {moved:?}");
        };
        assert_eq!(*duration_secs, Some(1.0));
    }

    /// B-133 (Amendment 7): a shooter found nowhere (dead or unseen on the launch frame) keeps
    /// today's start, and the miss is counted; an event that names no shooter asks for no
    /// lookup.
    #[test]
    fn a_projectile_whose_shooter_is_unknown_keeps_its_start_and_is_counted() {
        let presented = [bubble(2, [6.0, 2.0], 0.0, 0.5)];
        let shot = projectile([2.5, 2.0, 0.5], [5.5, 2.0, 0.5], Some(EntityId(2)));
        let mut struck = StruckBubbles::default();
        let (aimed, _) = at_struck_surface(&shot, |id| struck.find(&presented, None, id), None);
        let (moved, started) = launched(&shot, &mut struck, &presented, None);
        assert!(!started);
        assert_eq!(moved, aimed, "the landing is aimed, the start kept");
        assert_eq!(start(&moved), [2.5, 2.0, 0.5]);
        assert_eq!(
            StruckFrom::ALL.map(|from| struck.shooter_found_in(from)),
            [0, 0, 0, 1]
        );
        let synthetic = projectile([2.5, 2.0, 0.5], [5.5, 2.0, 0.5], None);
        let (moved, started) = launched(&synthetic, &mut struck, &presented, None);
        assert!(!started);
        assert_eq!(moved, synthetic);
        assert_eq!(
            StruckFrom::ALL.map(|from| struck.shooter_found_in(from)),
            [0, 0, 0, 1],
            "an event without ends asks for no lookup"
        );
        // Not a projectile: nothing is asked or moved.
        let mut hit = tracer(
            [0.0, 0.0, 0.5],
            [4.0, 0.0, 0.5],
            WeaponSlot::Primary,
            Some(EntityId(2)),
        );
        let published = hit.clone();
        assert!(!launch_from_shooter(&mut hit, &published, |_| None, None));
        assert_eq!(hit, published);
    }

    /// Review condition 3 (Amendment 3): the last frame's bubbles are forgotten when the
    /// presented session changes or ends, so an id of an old session never finds a bubble.
    #[test]
    fn the_last_frame_is_forgotten_when_the_session_changes() {
        let presented = [bubble(6, [1.0, 1.0], 2.5, 0.7)];
        let first = Some(SessionGeneration::FIRST);
        let mut struck = StruckBubbles::default();
        struck.follow(first);
        struck.remember(&presented);
        let kept = struck.buffer();
        struck.follow(first);
        assert!(
            struck.find(&[], None, EntityId(6)).is_some(),
            "same session"
        );
        let next = SessionGeneration::FIRST.next().ok();
        struck.follow(next);
        assert_eq!(struck.find(&[], None, EntityId(6)), None, "a new session");
        struck.follow(next);
        struck.remember(&presented);
        struck.follow(None);
        assert_eq!(struck.find(&[], None, EntityId(6)), None, "no session");
        assert_eq!(struck.buffer().1, kept.1, "the buffer keeps its capacity");
        assert_eq!(struck.found_in(StruckFrom::LastFrame), 1);
        assert_eq!(struck.found_in(StruckFrom::Nowhere), 2);
    }

    #[test]
    fn the_last_frame_is_kept_in_place() {
        let presented = [
            bubble(1, [0.0, 0.0], 0.0, 0.5),
            bubble(2, [1.0, 0.0], 0.0, 0.5),
        ];
        let mut struck = StruckBubbles::default();
        struck.remember(&presented);
        let kept = struck.buffer();
        for _ in 0..8 {
            struck.remember(&presented);
            struck.remember(&presented[..1]);
        }
        assert_eq!(struck.buffer(), kept, "refilled in place");
        let facing = hit_facing(&tracer(
            [0.0, 0.0, 0.5],
            [0.0, 3.0, 0.5],
            WeaponSlot::Primary,
            None,
        ))
        .unwrap_or(f32::NAN);
        assert!(
            near(facing, -std::f32::consts::FRAC_PI_2),
            "towards the muzzle"
        );
    }
}
