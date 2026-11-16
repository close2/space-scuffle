//! Where an instant or melee hit shows: on the struck unit's bubble, towards its attacker (plan
//! `2026-11-09-effect-visualization` WP11, review 2, A2-1).
//!
//! # Responsibility
//! The simulation publishes an instant hit (`HitscanTracer`, `BeamVisual`) and a melee strike
//! (`MeleeSlashVisual`) at the struck unit's centre, half a unit above the ground: the middle of
//! its bubble, whose glass is drawn over anything there. This module moves those points to the
//! bubble's near surface, where the hit shows and where the shot's slug ends:
//! - [`StruckBubbles`] indexes the presented bubbles by position once a frame in which events
//!   arrive, in a retained buffer;
//! - [`at_struck_surface`] returns the event with its hit point moved out of the struck bubble
//!   towards the attacker, by the bubble's radius and [`SURFACE_MARGIN`].
//!
//! # How it fits
//! `effects::systems::spawn_effects_from_events` passes every admitted event through
//! [`at_struck_surface`] before it asks for the event's travel and impact, so a slug, a beam,
//! its burn and a blow all meet the struck bubble's glass. Presentation only: the simulation's
//! events and state are read, never changed (D-1).
//!
//! # Invariants
//! - An event whose struck bubble is not found (none presented within [`MATCH_SLACK`] of its
//!   radius, a synthetic event, an app without a battle) is returned unchanged.
//! - A shot's hit point never moves past [`MOST_PULL`] of the way back to its muzzle.
//! - The index allocates nothing in steady state: its buffer is retained and refilled.

use bevy::prelude::*;
use sim_types::snapshot::{BubbleRenderInstance, CombatEventVisual};

/// World units beyond the struck bubble's radius at which its hit shows, so it is drawn in
/// front of the glass rather than on it.
pub const SURFACE_MARGIN: f32 = 0.06;
/// World units a presented bubble's centre may lie from a published hit point and still be the
/// struck one, beyond its radius (it may have moved since the hit's tick, or be interpolated).
pub const MATCH_SLACK: f32 = 0.35;
/// Largest share of a shot's length its hit point is moved back towards its muzzle.
pub const MOST_PULL: f32 = 0.8;
/// The struck bubble's radius an impact binds (`UnitRadius`) when the bubble is not found.
pub const DEFAULT_STRUCK_RADIUS: f32 = 0.5;

/// The presented bubbles, sorted by their map `x`, as `[x, y, radius]` (map coordinates).
#[derive(Resource, Debug, Default)]
pub struct StruckBubbles {
    sorted: Vec<[f32; 3]>,
    /// The largest radius indexed.
    widest: f32,
}

impl StruckBubbles {
    /// Refills the index from `bubbles` (map `x, y` in `world_pos`).
    pub fn rebuild(&mut self, bubbles: &[BubbleRenderInstance]) {
        self.rebuild_from(
            bubbles
                .iter()
                .map(|bubble| [bubble.world_pos[0], bubble.world_pos[1], bubble.radius]),
        );
    }

    /// Refills the index from `[x, y, radius]` entries (map coordinates).
    pub fn rebuild_from(&mut self, bubbles: impl IntoIterator<Item = [f32; 3]>) {
        self.sorted.clear();
        self.sorted.extend(bubbles);
        // PERFORMANCE RATIONALE:
        // Why: each instant or melee event asks for the bubble nearest its hit point, and a
        // battle publishes dozens of hits a tick among hundreds of bubbles.
        // What: the bubbles are sorted by `x` once per frame with events, and each query reads
        // only the slice within reach in `x`, found by binary search.
        // Trade-off: an O(N log N) sort per such frame instead of an O(N) scan per event.
        self.sorted.sort_unstable_by(|a, b| a[0].total_cmp(&b[0]));
        self.widest = self
            .sorted
            .iter()
            .map(|bubble| bubble[2])
            .fold(0.0_f32, f32::max);
    }

    /// The radius of the bubble struck at map point `(x, y)`: the presented bubble whose centre
    /// is nearest it, if that lies within its radius plus [`MATCH_SLACK`].
    #[must_use]
    pub fn radius_at(&self, x: f32, y: f32) -> Option<f32> {
        let reach = self.widest + MATCH_SLACK;
        let start = self.sorted.partition_point(|bubble| bubble[0] < x - reach);
        self.sorted[start..]
            .iter()
            .take_while(|bubble| bubble[0] <= x + reach)
            .map(|bubble| (bubble, (bubble[0] - x).hypot(bubble[1] - y)))
            .filter(|(bubble, distance)| *distance <= bubble[2] + MATCH_SLACK)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(bubble, _)| bubble[2])
    }
}

/// `event` with its hit point on the struck bubble's near surface, towards the attacker: a
/// tracer's or a beam's `to` moved back along the shot, a melee strike's `pos` moved along its
/// published `angle_rad` (the direction from the struck unit towards its attacker), each by
/// the bubble's radius plus [`SURFACE_MARGIN`]. `radius_at(x, y)` gives the radius of the
/// bubble struck at map point `(x, y)`. Every other event, and an event whose bubble is not
/// found, is returned as it is. The struck bubble's radius is returned with the event, when it
/// was found, for the impact to bind (`UnitRadius`).
#[must_use]
pub fn at_struck_surface(
    event: &CombatEventVisual,
    radius_at: impl Fn(f32, f32) -> Option<f32>,
) -> (CombatEventVisual, Option<f32>) {
    let mut moved = event.clone();
    let radius = match &mut moved {
        CombatEventVisual::HitscanTracer { from, to, .. }
        | CombatEventVisual::BeamVisual { from, to, .. } => {
            let radius = radius_at(to[0], to[1]);
            let back = Vec2::new(from[0] - to[0], from[1] - to[1]);
            let length = back.length();
            if let Some(radius) = radius.filter(|_| length > f32::EPSILON) {
                let pull = (radius + SURFACE_MARGIN).min(length * MOST_PULL);
                let step = back / length * pull;
                to[0] += step.x;
                to[1] += step.y;
            }
            radius
        }
        CombatEventVisual::MeleeSlashVisual { pos, angle_rad, .. } => {
            let radius = radius_at(pos[0], pos[1]);
            if let Some(radius) = radius {
                let pull = radius + SURFACE_MARGIN;
                pos[0] += angle_rad.cos() * pull;
                pos[1] += angle_rad.sin() * pull;
            }
            radius
        }
        CombatEventVisual::ProjectileVisual { .. }
        | CombatEventVisual::BubbleShatter { .. }
        | CombatEventVisual::SplashVisual { .. } => None,
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
        CombatEventVisual::BubbleShatter { .. } | CombatEventVisual::SplashVisual { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_types::{EffectSource, PlayerId};

    /// The hit point of an instant hit or a blow; NaN for anything else, which fails every
    /// comparison below.
    fn hit_point(event: &CombatEventVisual) -> [f32; 3] {
        match event {
            CombatEventVisual::HitscanTracer { to, .. } => *to,
            CombatEventVisual::MeleeSlashVisual { pos, .. } => *pos,
            _ => [f32::NAN; 3],
        }
    }

    fn tracer(from: [f32; 3], to: [f32; 3]) -> CombatEventVisual {
        CombatEventVisual::HitscanTracer {
            from,
            to,
            owner: PlayerId(0),
            source: EffectSource::default(),
        }
    }

    #[test]
    fn a_tracer_ends_on_the_struck_bubbles_near_surface() {
        let (moved, radius) =
            at_struck_surface(&tracer([0.0, 0.0, 0.5], [4.0, 0.0, 0.5]), |_, _| Some(0.5));
        assert_eq!(radius, Some(0.5));
        let to = hit_point(&moved);
        assert!(
            (to[0] - (4.0 - 0.5 - SURFACE_MARGIN)).abs() < 1e-5,
            "{to:?}"
        );
        assert!(to[1].abs() < 1e-6 && (to[2] - 0.5).abs() < 1e-6);
        assert_eq!(
            moved,
            tracer([0.0, 0.0, 0.5], to),
            "only the hit point is moved, not the muzzle"
        );
    }

    #[test]
    fn a_short_shot_is_never_pulled_past_its_muzzle() {
        let (moved, _) =
            at_struck_surface(&tracer([0.0, 0.0, 0.5], [0.5, 0.0, 0.5]), |_, _| Some(2.0));
        let to = hit_point(&moved);
        assert!((to[0] - 0.5 * (1.0 - MOST_PULL)).abs() < 1e-5, "{to:?}");
    }

    #[test]
    fn a_blow_moves_towards_its_attacker_and_an_unknown_bubble_moves_nothing() {
        let blow = CombatEventVisual::MeleeSlashVisual {
            pos: [2.0, 2.0, 0.5],
            angle_rad: std::f32::consts::FRAC_PI_2,
            radius: 0.5,
            owner: PlayerId(1),
            source: EffectSource::default(),
        };
        let pos = hit_point(&at_struck_surface(&blow, |_, _| Some(0.4)).0);
        assert!((pos[0] - 2.0).abs() < 1e-5 && (pos[1] - (2.4 + SURFACE_MARGIN)).abs() < 1e-5);
        assert_eq!(at_struck_surface(&blow, |_, _| None), (blow.clone(), None));
        assert_eq!(hit_facing(&blow), Some(std::f32::consts::FRAC_PI_2));
    }

    #[test]
    fn the_index_finds_the_nearest_bubble_within_its_radius_and_slack() {
        let mut index = StruckBubbles::default();
        index.rebuild_from([[5.0, 0.0, 0.4], [0.0, 0.0, 0.6], [0.9, 0.0, 0.3]]);
        assert_eq!(index.radius_at(0.1, 0.0), Some(0.6));
        assert_eq!(index.radius_at(0.85, 0.05), Some(0.3));
        assert_eq!(index.radius_at(5.0, 0.6), Some(0.4));
        assert_eq!(index.radius_at(3.0, 3.0), None, "nothing within reach");
        let facing = hit_facing(&tracer([0.0, 0.0, 0.5], [0.0, 3.0, 0.5])).unwrap_or(f32::NAN);
        assert!(
            (facing + std::f32::consts::FRAC_PI_2).abs() < 1e-6,
            "towards the muzzle"
        );
    }
}
