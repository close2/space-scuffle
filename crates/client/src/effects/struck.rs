//! Where an instant or melee hit shows: on the struck unit's bubble, towards its attacker (plan
//! `2026-11-09-effect-visualization` WP11, reviews 2 and 3: A2-1, A3-1, A3-2, E3-3, E3-5).
//!
//! # Responsibility
//! The simulation publishes an instant hit (`HitscanTracer`, `BeamVisual`) and a melee strike
//! (`MeleeSlashVisual`) at the struck unit's map position, half a unit above the ground: inside
//! a ground unit's bubble, whose glass is drawn over anything there, and far below a flyer's.
//! This module moves those points onto the struck bubble's glass, where the hit shows and where
//! the shot's slug ends:
//! - [`StruckBubbles`] indexes the presented bubbles once a frame in which events arrive, in a
//!   uniform grid over the map kept in retained buffers: each bubble (a structure's dome
//!   included) is entered in every cell its radius plus [`MATCH_SLACK`] covers, so a query
//!   reads one cell;
//! - [`at_struck_surface`] returns the event with its hit point on the struck bubble's surface:
//!   towards the attacker, raised [`HIT_ELEVATION`] above the bubble's drawn centre (so a hit on
//!   the side of the bubble away from the camera is still in front of its glass: the camera
//!   looks down at about 58 degrees, and a point 45 degrees up the far side faces it), at the
//!   bubble's radius plus [`SURFACE_MARGIN`].
//!
//! # How it fits
//! `effects::systems::spawn_effects_from_events` passes every admitted event through
//! [`at_struck_surface`] before it asks for the event's travel and impact, so a slug, a beam,
//! its burn and a blow all meet the struck bubble's glass, a flyer's included. Presentation
//! only: the simulation's events and state are read, never changed (D-1).
//!
//! # Invariants
//! - An event whose struck bubble is not found (none presented within [`MATCH_SLACK`] of its
//!   radius, a synthetic event, an app without a battle) is moved as if it struck a bubble of
//!   [`DEFAULT_STRUCK_RADIUS`] centred on its published point, the radius its impact binds, so a
//!   look placed `Behind` the hit still lands on that bubble's centre (review 3, E3-5).
//! - A shot's hit point never moves past [`MOST_PULL`] of the way back to its muzzle.
//! - The index allocates nothing in steady state: its buffers are retained and refilled.

use bevy::prelude::*;
use sim_types::snapshot::{BubbleRenderInstance, CombatEventVisual};
use sim_types::{ShapeKind, WeaponSlot};

/// World units beyond the struck bubble's radius at which its hit shows, so it is drawn in
/// front of the glass rather than on it.
pub const SURFACE_MARGIN: f32 = 0.12;
/// World units a presented bubble's centre may lie from a published hit point and still be the
/// struck one, beyond its radius (it may have moved since the hit's tick, or be interpolated).
pub const MATCH_SLACK: f32 = 0.35;
/// Largest share of a shot's length its hit point is moved back towards its muzzle.
pub const MOST_PULL: f32 = 0.8;
/// The struck bubble's radius an impact binds (`UnitRadius`) when the bubble is not found.
pub const DEFAULT_STRUCK_RADIUS: f32 = 0.5;
/// Height above a dome's base at which a hit on its rim shows, so it is not sunk in the floor.
pub const DOME_RIM_LIFT: f32 = 0.1;
/// How far up the struck bubble, from its drawn centre, its hit shows: 45 degrees, radians.
pub const HIT_ELEVATION: f32 = std::f32::consts::FRAC_PI_4;
/// Height above its ground at which a unit's bubble counts as flying, world units (air units
/// fly 2.5 up; a ground unit's bubble stands on the ground).
pub const AIR_ABOVE_GROUND: f32 = 1.0;
/// Side of a cell of the index's grid, world units: about two unit radii.
pub const CELL: f32 = 1.0;
/// Most cells along either side of the grid; a larger map widens the cells instead.
pub const MOST_CELLS_PER_SIDE: usize = 512;

/// One indexed bubble.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Indexed {
    /// Map `x, y`.
    at: Vec2,
    /// Height of the drawn sphere's centre (a dome's: its base), world units.
    height: f32,
    /// Glass radius.
    radius: f32,
    /// Whether it flies.
    air: bool,
    /// Whether it is a structure's dome (a half sphere standing on its base).
    dome: bool,
}

/// The bubble a hit struck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StruckBubble {
    /// Its glass radius, world units.
    pub radius: f32,
    /// Height of its drawn centre, world units (the event's height convention); a dome's base.
    pub height: f32,
    /// Whether it is a structure's dome: a half sphere standing on `height`, with nothing of
    /// it below (review 4, A4-1).
    pub dome: bool,
}

/// Which plane a weapon strikes, from its slot: an `Air` weapon a flyer, a `Ground` weapon a
/// ground unit; a `Primary` weapon either.
#[must_use]
pub const fn struck_plane(slot: WeaponSlot) -> Option<bool> {
    match slot {
        WeaponSlot::Air => Some(true),
        WeaponSlot::Ground => Some(false),
        WeaponSlot::Primary => None,
    }
}

/// The presented bubbles, in a uniform grid over the area they cover.
#[derive(Resource, Debug, Default)]
pub struct StruckBubbles {
    bubbles: Vec<Indexed>,
    /// For each cell, the start of its run in `entries`; one more than the cells.
    starts: Vec<u32>,
    /// Indices into `bubbles`, cell by cell.
    entries: Vec<u32>,
    /// Map point of the grid's lower corner.
    origin: Vec2,
    /// Side of a cell, world units.
    cell: f32,
    /// Cells along `x` and `y`.
    size: (usize, usize),
}

impl StruckBubbles {
    /// Refills the index from `bubbles`.
    pub fn rebuild(&mut self, bubbles: &[BubbleRenderInstance]) {
        self.bubbles.clear();
        self.bubbles.extend(bubbles.iter().map(|bubble| {
            let dome = matches!(bubble.shape, ShapeKind::BuildingDome { .. });
            Indexed {
                at: Vec2::new(bubble.world_pos[0], bubble.world_pos[1]),
                height: bubble.world_pos[2] + if dome { 0.0 } else { bubble.radius },
                radius: bubble.radius,
                air: !dome && bubble.world_pos[2] - bubble.ground_z > AIR_ABOVE_GROUND,
                dome,
            }
        }));
        self.fill();
    }

    /// Refills the index from `(map x, y, drawn centre height, radius, flies)` entries of unit
    /// bubbles (spheres).
    pub fn rebuild_from(&mut self, bubbles: impl IntoIterator<Item = (Vec2, f32, f32, bool)>) {
        self.bubbles.clear();
        self.bubbles.extend(
            bubbles
                .into_iter()
                .map(|(at, height, radius, air)| Indexed {
                    at,
                    height,
                    radius,
                    air,
                    dome: false,
                }),
        );
        self.fill();
    }

    /// The grid cell of map point `at`, if it lies on the grid.
    fn cell_of(&self, at: Vec2) -> Option<(usize, usize)> {
        let local = (at - self.origin) / self.cell;
        if local.x < 0.0 || local.y < 0.0 {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a non-negative cell coordinate, compared with the grid's size below"
        )]
        let (x, y) = (local.x as usize, local.y as usize);
        (x < self.size.0 && y < self.size.1).then_some((x, y))
    }

    /// The cells (inclusive ranges) a disc of `reach` round `at` covers, clamped to the grid.
    fn cells_of(&self, at: Vec2, reach: f32) -> ((usize, usize), (usize, usize)) {
        let clamp = |value: f32, size: usize| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a cell coordinate clamped to the grid, at most a few hundred"
            )]
            let cell = (value.max(0.0) as usize).min(size.saturating_sub(1));
            cell
        };
        let low = (at - Vec2::splat(reach) - self.origin) / self.cell;
        let high = (at + Vec2::splat(reach) - self.origin) / self.cell;
        (
            (clamp(low.x, self.size.0), clamp(high.x, self.size.0)),
            (clamp(low.y, self.size.1), clamp(high.y, self.size.1)),
        )
    }

    /// Lays out the grid over the indexed bubbles and fills it.
    fn fill(&mut self) {
        // PERFORMANCE RATIONALE:
        // Why: each instant or melee event asks for the bubble it struck, and a battle publishes
        // dozens of hits a tick among hundreds of bubbles, some of them wide structures; a scan
        // per event is O(events x bubbles).
        // What: a uniform grid, rebuilt once per frame with events by counting sort into
        // retained buffers; each bubble is entered in every cell its reach covers, so a query
        // reads one cell's short run.
        // Trade-off: a wide dome is entered in a few dozen cells, and the grid is rebuilt
        // whole each such frame, in exchange for a bounded query and no allocation.
        let (low, high) = self.bubbles.iter().fold(
            (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
            |(low, high), bubble| {
                let reach = Vec2::splat(bubble.radius + MATCH_SLACK);
                (low.min(bubble.at - reach), high.max(bubble.at + reach))
            },
        );
        self.starts.clear();
        self.entries.clear();
        if self.bubbles.is_empty() {
            self.size = (0, 0);
            return;
        }
        let span = (high - low).max(Vec2::splat(CELL));
        #[expect(
            clippy::cast_precision_loss,
            reason = "the cell cap is a few hundred, exact in f32"
        )]
        let most = MOST_CELLS_PER_SIDE as f32;
        self.cell = CELL.max(span.x / most).max(span.y / most);
        self.origin = low;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "at most MOST_CELLS_PER_SIDE cells per side"
        )]
        let cells =
            |extent: f32| ((extent / self.cell).ceil() as usize).clamp(1, MOST_CELLS_PER_SIDE);
        self.size = (cells(span.x), cells(span.y));
        let total = self.size.0 * self.size.1;
        self.starts.resize(total + 1, 0);
        for index in 0..self.bubbles.len() {
            let bubble = self.bubbles[index];
            let ((x0, x1), (y0, y1)) = self.cells_of(bubble.at, bubble.radius + MATCH_SLACK);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    self.starts[y * self.size.0 + x + 1] += 1;
                }
            }
        }
        for cell in 1..=total {
            self.starts[cell] += self.starts[cell - 1];
        }
        let filled = self.starts[total] as usize;
        self.entries.resize(filled, 0);
        // `starts[cell]` serves as the fill cursor of `cell - 1`'s run, then is restored.
        for index in 0..self.bubbles.len() {
            let bubble = self.bubbles[index];
            let ((x0, x1), (y0, y1)) = self.cells_of(bubble.at, bubble.radius + MATCH_SLACK);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let cell = y * self.size.0 + x;
                    let slot = self.starts[cell] as usize;
                    self.entries[slot] = u32::try_from(index).unwrap_or(u32::MAX);
                    self.starts[cell] += 1;
                }
            }
        }
        for cell in (1..=total).rev() {
            self.starts[cell] = self.starts[cell - 1];
        }
        self.starts[0] = 0;
    }

    /// The bubble struck at map point `(x, y)`, published at height `z`, by a weapon striking
    /// `plane` (`Some(true)` a flyer, `Some(false)` a ground unit, `None` either): among the
    /// presented bubbles whose centre lies within their radius plus [`MATCH_SLACK`] of the
    /// point in plan, one of the struck plane if any, the nearest in plan, ties broken by height.
    #[must_use]
    pub fn struck_at(&self, x: f32, y: f32, z: f32, plane: Option<bool>) -> Option<StruckBubble> {
        let at = Vec2::new(x, y);
        let (cx, cy) = self.cell_of(at)?;
        let cell = cy * self.size.0 + cx;
        let run = self.starts[cell] as usize..self.starts[cell + 1] as usize;
        self.entries[run]
            .iter()
            .filter_map(|index| self.bubbles.get(*index as usize))
            .map(|bubble| (bubble, bubble.at.distance(at)))
            .filter(|(bubble, distance)| *distance <= bubble.radius + MATCH_SLACK)
            .min_by(|a, b| {
                let key = |(bubble, distance): &(&Indexed, f32)| {
                    let wrong_plane = plane.is_some_and(|air| air != bubble.air);
                    (wrong_plane, distance + 0.05 * (bubble.height - z).abs())
                };
                let (first, second) = (key(a), key(b));
                first.0.cmp(&second.0).then(first.1.total_cmp(&second.1))
            })
            .map(|(bubble, _)| StruckBubble {
                radius: bubble.radius,
                height: bubble.height,
                dome: bubble.dome,
            })
    }
}

/// `point` (map `x, y`, height) moved onto the glass of a bubble of `struck` centred on it in
/// plan: towards `towards` (a unit map direction) and [`HIT_ELEVATION`] up from the bubble's
/// drawn centre, at its radius plus [`SURFACE_MARGIN`], the horizontal move at most `most`.
fn onto_glass(point: &mut [f32; 3], struck: StruckBubble, towards: Vec2, most: f32) {
    let reach = struck.radius + SURFACE_MARGIN;
    let across = (reach * HIT_ELEVATION.cos()).min(most);
    point[0] += towards.x * across;
    point[1] += towards.y * across;
    point[2] = struck.height + reach * HIT_ELEVATION.sin();
}

/// `point` (map `x, y`, height) moved onto the outline of a bubble of `struck` centred on it in
/// plan, as the camera looking along `view` (Bevy world, unit) sees it: the point of the
/// bubble's silhouette, at its radius plus [`SURFACE_MARGIN`], on the side of `towards` (a unit
/// map direction) on screen, so a shot's end, its hit and a blow are drawn at the edge of the
/// glass the attack comes from, never in front of the bubble's face or behind it (review 3,
/// A3-2). On a dome, which has nothing below its base, a point that would fall below the base
/// is put on the base rim instead (review 4, A4-1). The horizontal move is at most `most`.
/// Falls back to [`onto_glass`] when `towards` runs along the view.
fn onto_outline(point: &mut [f32; 3], struck: StruckBubble, towards: Vec2, most: f32, view: Vec3) {
    let across = Vec3::new(towards.x, 0.0, towards.y);
    let side = across - view * across.dot(view);
    if side.length_squared() < 1e-6 {
        onto_glass(point, struck, towards, most);
        return;
    }
    let reach = struck.radius + SURFACE_MARGIN;
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
/// from the struck unit towards its attacker), and raised onto the bubble (see
/// [`onto_glass`]; onto the bubble's outline as the camera looking along `view` sees it, when
/// there is one, see [`onto_outline`]). `struck_at(x, y, z, plane)` gives the bubble struck at map point `(x, y)`,
/// published at height `z`, by a weapon striking `plane` (see [`StruckBubbles::struck_at`]).
/// Every other event is returned as it is. The struck bubble's radius is returned with the
/// event, when it was found, for the impact to bind (`UnitRadius`); an unfound bubble counts as
/// one of [`DEFAULT_STRUCK_RADIUS`] centred on the published point.
#[must_use]
pub fn at_struck_surface(
    event: &CombatEventVisual,
    struck_at: impl Fn(f32, f32, f32, Option<bool>) -> Option<StruckBubble>,
    view: Option<Vec3>,
) -> (CombatEventVisual, Option<f32>) {
    let mut moved = event.clone();
    let assumed = |point: &[f32; 3]| StruckBubble {
        radius: DEFAULT_STRUCK_RADIUS,
        height: point[2],
        dome: false,
    };
    let radius = match &mut moved {
        CombatEventVisual::HitscanTracer {
            from, to, source, ..
        }
        | CombatEventVisual::BeamVisual {
            from, to, source, ..
        } => {
            let found = struck_at(to[0], to[1], to[2], struck_plane(source.weapon.slot));
            let back = Vec2::new(from[0] - to[0], from[1] - to[1]);
            let length = back.length();
            if length > f32::EPSILON {
                let bubble = found.unwrap_or_else(|| assumed(to));
                match view {
                    Some(view) => onto_outline(to, bubble, back / length, length * MOST_PULL, view),
                    None => onto_glass(to, bubble, back / length, length * MOST_PULL),
                }
            }
            found.map(|bubble| bubble.radius)
        }
        CombatEventVisual::MeleeSlashVisual {
            pos,
            angle_rad,
            source,
            ..
        } => {
            let found = struck_at(pos[0], pos[1], pos[2], struck_plane(source.weapon.slot));
            let bubble = found.unwrap_or_else(|| assumed(pos));
            let towards = Vec2::from_angle(*angle_rad);
            match view {
                Some(view) => onto_outline(pos, bubble, towards, f32::MAX, view),
                None => onto_glass(pos, bubble, towards, f32::MAX),
            }
            found.map(|bubble| bubble.radius)
        }
        CombatEventVisual::ProjectileVisual { .. }
        | CombatEventVisual::BubbleShatter { .. }
        | CombatEventVisual::SplashVisual { .. }
        | CombatEventVisual::SpellImpact { .. } => None,
    };
    (moved, radius)
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
mod tests {
    use super::*;
    use sim_types::{EffectSource, PlayerId, WeaponKey};

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

    fn tracer(from: [f32; 3], to: [f32; 3], slot: WeaponSlot) -> CombatEventVisual {
        CombatEventVisual::HitscanTracer {
            from,
            to,
            owner: PlayerId(0),
            source: source(slot),
            ends: None,
        }
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn a_tracer_ends_on_the_struck_bubbles_glass_towards_its_muzzle_and_up() {
        let bubble = StruckBubble {
            radius: 0.5,
            height: 0.5,
            dome: false,
        };
        let shot = tracer([0.0, 0.0, 0.5], [4.0, 0.0, 0.5], WeaponSlot::Primary);
        let (moved, radius) = at_struck_surface(&shot, |_, _, _, _| Some(bubble), None);
        assert_eq!(radius, Some(0.5));
        let to = hit_point(&moved);
        let reach = 0.5 + SURFACE_MARGIN;
        assert!(near(to[0], 4.0 - reach * HIT_ELEVATION.cos()), "{to:?}");
        assert!(near(to[1], 0.0) && near(to[2], 0.5 + reach * HIT_ELEVATION.sin()));
        assert_eq!(moved, tracer([0.0, 0.0, 0.5], to, WeaponSlot::Primary));
    }

    #[test]
    fn a_hit_on_a_flyer_rises_to_its_glass() {
        let flyer = StruckBubble {
            radius: 0.6,
            height: 3.1,
            dome: false,
        };
        let shot = tracer([0.0, 0.0, 0.5], [4.0, 0.0, 0.5], WeaponSlot::Air);
        let to = hit_point(&at_struck_surface(&shot, |_, _, _, _| Some(flyer), None).0);
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
        // A camera looking down at 58 degrees towards +z (the map's +y).
        let pitch = 58.0_f32.to_radians();
        let view = Vec3::new(0.0, -pitch.sin(), pitch.cos());
        // Shot from the camera's side (map -y): its end on the lower outline, towards the
        // camera and down; from the far side: on the upper outline, away and up.
        let near = tracer([4.0, -4.0, 0.5], [4.0, 0.0, 0.5], WeaponSlot::Primary);
        let to = hit_point(&at_struck_surface(&near, |_, _, _, _| Some(bubble), Some(view)).0);
        let reach = 0.5 + SURFACE_MARGIN;
        assert!(
            near_eq(to[1], -reach * pitch.sin()) && near_eq(to[2], 0.5 - reach * pitch.cos()),
            "{to:?}"
        );
        let far = tracer([4.0, 4.0, 0.5], [4.0, 0.0, 0.5], WeaponSlot::Primary);
        let to = hit_point(&at_struck_surface(&far, |_, _, _, _| Some(bubble), Some(view)).0);
        assert!(
            near_eq(to[1], reach * pitch.sin()) && near_eq(to[2], 0.5 + reach * pitch.cos()),
            "{to:?}"
        );
        // Across the view: the outline's side, level with the centre.
        let across = tracer([0.0, 0.0, 0.5], [4.0, 0.0, 0.5], WeaponSlot::Primary);
        let to = hit_point(&at_struck_surface(&across, |_, _, _, _| Some(bubble), Some(view)).0);
        assert!(near_eq(to[0], 4.0 - reach) && near_eq(to[2], 0.5), "{to:?}");
    }

    #[test]
    fn a_dome_is_struck_on_its_visible_outline_never_below_its_base() {
        // A dome of radius 2 standing on the floor at 0 (review 4, A4-1).
        let dome = StruckBubble {
            radius: 2.0,
            height: 0.0,
            dome: true,
        };
        let pitch = 58.0_f32.to_radians();
        let view = Vec3::new(0.0, -pitch.sin(), pitch.cos());
        // From below on screen (the camera's side), across, and from above.
        for (from, name) in [
            ([10.0, -10.0, 0.5], "from below"),
            ([0.0, 0.0, 0.5], "across"),
            ([10.0, 10.0, 0.5], "from above"),
        ] {
            let shot = tracer(from, [10.0, 0.0, 0.5], WeaponSlot::Primary);
            let to = hit_point(&at_struck_surface(&shot, |_, _, _, _| Some(dome), Some(view)).0);
            let flat = (to[0] - 10.0).hypot(to[1]);
            assert!(to[2] >= 0.0, "{name}: at or above the base: {to:?}");
            // On the outline: as far out as the dome's radius in plan, or up its far side.
            assert!(
                flat >= 2.0 - 1e-4 || to[2] > 2.0 * 0.5,
                "{name}: outside the dome: {to:?}"
            );
        }
        // From below: on the base rim, towards the camera, lifted off the floor.
        let shot = tracer([10.0, -10.0, 0.5], [10.0, 0.0, 0.5], WeaponSlot::Primary);
        let to = hit_point(&at_struck_surface(&shot, |_, _, _, _| Some(dome), Some(view)).0);
        assert!(
            near_eq(to[2], DOME_RIM_LIFT) && near_eq(to[1], -(2.0 + SURFACE_MARGIN)),
            "{to:?}"
        );
    }

    fn near_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn a_short_shot_is_never_pulled_past_its_muzzle() {
        let bubble = StruckBubble {
            radius: 2.0,
            height: 0.5,
            dome: false,
        };
        let shot = tracer([0.0, 0.0, 0.5], [0.5, 0.0, 0.5], WeaponSlot::Primary);
        let to = hit_point(&at_struck_surface(&shot, |_, _, _, _| Some(bubble), None).0);
        assert!(near(to[0], 0.5 * (1.0 - MOST_PULL)), "{to:?}");
    }

    #[test]
    fn a_blow_moves_towards_its_attacker_and_an_unknown_bubble_counts_as_the_default() {
        let blow = CombatEventVisual::MeleeSlashVisual {
            pos: [2.0, 2.0, 0.5],
            angle_rad: std::f32::consts::FRAC_PI_2,
            radius: 0.5,
            owner: PlayerId(1),
            source: EffectSource::default(),
            ends: None,
        };
        let bubble = StruckBubble {
            radius: 0.4,
            height: 0.4,
            dome: false,
        };
        let pos = hit_point(&at_struck_surface(&blow, |_, _, _, _| Some(bubble), None).0);
        let across = (0.4 + SURFACE_MARGIN) * HIT_ELEVATION.cos();
        assert!(near(pos[0], 2.0) && near(pos[1], 2.0 + across), "{pos:?}");
        let (moved, radius) = at_struck_surface(&blow, |_, _, _, _| None, None);
        assert_eq!(radius, None);
        let pos = hit_point(&moved);
        let across = (DEFAULT_STRUCK_RADIUS + SURFACE_MARGIN) * HIT_ELEVATION.cos();
        assert!(
            near(pos[1], 2.0 + across),
            "the default bubble's surface: {pos:?}"
        );
        assert_eq!(hit_facing(&blow), Some(std::f32::consts::FRAC_PI_2));
    }

    #[test]
    fn the_grid_finds_the_nearest_bubble_within_its_radius_and_slack() {
        let mut index = StruckBubbles::default();
        index.rebuild_from([
            (Vec2::new(5.0, 0.0), 0.4, 0.4, false),
            (Vec2::new(0.0, 0.0), 0.6, 0.6, false),
            (Vec2::new(0.9, 0.0), 0.3, 0.3, false),
            // A wide dome far off, and a flyer over the first unit.
            (Vec2::new(30.0, 30.0), 0.0, 4.0, false),
            (Vec2::new(5.1, 0.1), 3.1, 0.6, true),
        ]);
        let radius = |x, y, plane| {
            index
                .struck_at(x, y, 0.5, plane)
                .map(|bubble| bubble.radius)
        };
        assert_eq!(radius(0.1, 0.0, None), Some(0.6));
        assert_eq!(radius(0.85, 0.05, None), Some(0.3));
        assert_eq!(radius(3.0, 3.0, None), None, "nothing within reach");
        assert_eq!(
            radius(28.0, 31.0, None),
            Some(4.0),
            "the dome from any cell it covers"
        );
        assert_eq!(
            radius(5.0, 0.0, Some(false)),
            Some(0.4),
            "a ground weapon, the ground unit"
        );
        assert_eq!(
            radius(5.0, 0.0, Some(true)),
            Some(0.6),
            "an anti-air weapon, the flyer"
        );
        assert_eq!(radius(-5.0, -5.0, None), None, "off the grid");
        let facing = hit_facing(&tracer(
            [0.0, 0.0, 0.5],
            [0.0, 3.0, 0.5],
            WeaponSlot::Primary,
        ))
        .unwrap_or(f32::NAN);
        assert!(
            near(facing, -std::f32::consts::FRAC_PI_2),
            "towards the muzzle"
        );
    }
}
