//! Whether a ring round an enemy has room for one more attacker, and where: the geometry behind
//! the D-R10 and D-CHURN diagnostics (plan `2026-11-24-solver-retarget-and-shoving` §5.1, ported
//! from the stopped G-6′ branch of plan `2026-11-20-contact-solver` Amendment 17).
//!
//! # Responsibility
//! [`free_arcs`] answers, from outside the simulation and in `f64`, the question the ring rule
//! asks in Q32.32: which points of the circle of radius `rho` round `centre` are clear of every
//! other body's disc grown by the asker's radius? Each body blocks the arc of points within
//! `r + r_o` of its centre, the exact half-width `acos((ρ² + d² − R²) / (2ρd))` of plan
//! `2026-11-20-contact-solver` Amendment 11, P1; the free arcs are what the union of those arcs
//! leaves. [`ring_has_room`] is whether any is left. [`is_free`] says whether one angle is free.
//!
//! # How it fits
//! [`crate::trace::contact_checks`] classifies, per tick, the units that stand on an automatic
//! chase whose target is out of reach while another enemy in their scan range has room on its
//! ring (D-R10), and the target returns whose old ring had room (D-CHURN). The matchup trace's
//! whole-matrix mode and the harness's mirror family both run it. This module holds only the
//! geometry, so it is tested on plain values.
//!
//! # Invariants
//! - Measurement only: nothing here feeds the simulation.
//! - A body whose disc does not reach the ring blocks nothing; one whose grown disc contains the
//!   whole ring blocks all of it.
//! - Angles are radians in `[0, 2π)`; a free arc is `(start, end)` with `start < end <= 2π`,
//!   and the arcs are sorted and disjoint.

use super::metrics::Point;
use std::f64::consts::TAU;

/// Angular slack, radians, below which a gap between blocked arcs is no room.
const EPSILON: f64 = 1e-9;

/// The arc `(start, end)` of the ring, radians in `[0, 2π)` with `end` possibly past `2π`, that a
/// body at distance `d` from the centre and bearing `bearing` blocks for a grown radius `grown`;
/// `None` when it blocks nothing, `Some((0, 2π))` when it blocks the whole ring.
fn blocked_arc(rho: f64, d: f64, bearing: f64, grown: f64) -> Option<(f64, f64)> {
    if d + rho <= grown {
        return Some((0.0, TAU));
    }
    if d - rho >= grown || rho - d >= grown {
        return None;
    }
    let cosine = ((rho * rho + d * d - grown * grown) / (2.0 * rho * d)).clamp(-1.0, 1.0);
    let half = cosine.acos();
    let start = (bearing - half).rem_euclid(TAU);
    Some((start, start + 2.0 * half))
}

/// The blocked arcs of the ring of radius `rho` round `centre` for an asker of radius `r`, each
/// split at `2π` and sorted by start.
fn blocked_arcs(centre: Point, rho: f64, r: f64, others: &[(Point, f64)]) -> Vec<(f64, f64)> {
    let mut arcs: Vec<(f64, f64)> = Vec::with_capacity(others.len() * 2);
    for &(position, radius) in others {
        let (dx, dy) = (position.0 - centre.0, position.1 - centre.1);
        let d = dx.hypot(dy);
        match blocked_arc(rho, d, dy.atan2(dx), r + radius) {
            None => {}
            Some((start, end)) if end > TAU => {
                arcs.push((start, TAU));
                arcs.push((0.0, end - TAU));
            }
            Some(arc) => arcs.push(arc),
        }
    }
    arcs.sort_by(|a, b| a.0.total_cmp(&b.0));
    arcs
}

/// The free arcs of the ring of radius `rho` round `centre` for an asker of radius `r`: the
/// angles no body of `others` (centre and radius) covers, sorted, each wider than `1e-9`
/// radians. An arc that crosses angle 0 is reported as two arcs, one ending at `2π`.
#[must_use]
pub fn free_arcs(centre: Point, rho: f64, r: f64, others: &[(Point, f64)]) -> Vec<(f64, f64)> {
    let mut free = Vec::new();
    let mut covered = 0.0_f64;
    for (start, end) in blocked_arcs(centre, rho, r, others) {
        if start > covered + EPSILON {
            free.push((covered, start));
        }
        covered = covered.max(end);
    }
    if covered < TAU - EPSILON {
        free.push((covered, TAU));
    }
    free
}

/// Whether the ring of radius `rho` round `centre` has an angle that no body of `others` covers
/// for an asker of radius `r`.
#[must_use]
pub fn ring_has_room(centre: Point, rho: f64, r: f64, others: &[(Point, f64)]) -> bool {
    !free_arcs(centre, rho, r, others).is_empty()
}

/// Whether `angle` (radians, any value) lies in one of the sorted `arcs` of [`free_arcs`].
#[must_use]
pub fn is_free(angle: f64, arcs: &[(f64, f64)]) -> bool {
    let angle = angle.rem_euclid(TAU);
    arcs.iter()
        .any(|&(start, end)| angle >= start && angle <= end)
}

/// Sample angles of the free `arcs`, at most `step` radians apart and never at an arc's covered
/// end: each arc's midpoint, then points walking out from it to both ends. These are the slots
/// the diagnostics test against the leash and terrain, as the ring rule's walk samples its ring
/// every half radius.
#[must_use]
pub fn sample_angles(arcs: &[(f64, f64)], step: f64) -> Vec<f64> {
    let step = step.max(1e-6);
    let mut samples = Vec::new();
    for &(start, end) in arcs {
        let middle = f64::midpoint(start, end);
        samples.push(middle);
        let mut offset = step;
        while offset < 0.5 * (end - start) {
            samples.push(middle - offset);
            samples.push(middle + offset);
            offset += step;
        }
    }
    samples
}

/// Why a candidate's ring with room would not take a standing chaser (D-R10's four columns, plan
/// `2026-11-24-solver-retarget-and-shoving` §5.1), ordered from the least to the most telling:
/// a unit-tick with several candidates counts in the column of the greatest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoomClass {
    /// Every free slot lies beyond the chase's leash (game behaviour: the leash).
    Leash,
    /// Free slots within the leash exist, but none is on walkable ground.
    Terrain,
    /// The chaser's own bearing point on the candidate's ring is beyond the leash or not on
    /// walkable ground: the ring rule answers `Unruled` there, which G-6′ never takes.
    Unruled,
    /// A slot the ring rule would take: the chaser is in the candidate's band, its bearing point
    /// is free, or a free slot within the leash is on walkable ground. Only this column is a
    /// defect of R10's kind.
    Free,
}

impl RoomClass {
    /// Every class, in column order.
    pub const ALL: [Self; 4] = [Self::Leash, Self::Terrain, Self::Unruled, Self::Free];

    /// Position of the class in [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Leash => 0,
            Self::Terrain => 1,
            Self::Unruled => 2,
            Self::Free => 3,
        }
    }
}

/// One candidate ring as a standing chaser sees it, world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RingQuery {
    /// The chaser's centre and radius.
    pub asker: (Point, f64),
    /// The candidate's centre.
    pub centre: Point,
    /// The band's outer edge against the candidate: max range plus both radii, centre to centre.
    pub band: f64,
    /// An automatic chase's leash: its origin and radius.
    pub leash: Option<(Point, f64)>,
}

/// Whether, and why, the candidate ring of `query` has room for its chaser among `others` (every
/// other body of the chaser's class: centre and radius, the chaser and the candidate left out);
/// `walkable` says whether a ring point is on ground the chaser can stand on. `None` when the
/// ring has no room and the chaser is not in the band. The ring is the one the measure has read
/// since the stopped G-6′ branch: radius `band − r / 2`, just inside the band's edge.
#[must_use]
pub fn classify_room(
    query: RingQuery,
    others: &[(Point, f64)],
    walkable: impl Fn(Point) -> bool,
) -> Option<RoomClass> {
    let ((asker, r), centre) = (query.asker, query.centre);
    let (dx, dy) = (asker.0 - centre.0, asker.1 - centre.1);
    if dx.hypot(dy) <= query.band {
        return Some(RoomClass::Free);
    }
    let rho = query.band - r / 2.0;
    if rho <= 0.0 {
        return None;
    }
    let arcs = free_arcs(centre, rho, r, others);
    if arcs.is_empty() {
        return None;
    }
    let point_at = |angle: f64| (centre.0 + rho * angle.cos(), centre.1 + rho * angle.sin());
    let leashed = |p: Point| {
        query
            .leash
            .is_none_or(|(origin, radius)| (p.0 - origin.0).hypot(p.1 - origin.1) <= radius)
    };
    let bearing = dy.atan2(dx);
    let own = point_at(bearing);
    if !leashed(own) || !walkable(own) {
        return Some(RoomClass::Unruled);
    }
    if is_free(bearing, &arcs) {
        return Some(RoomClass::Free);
    }
    let mut class = RoomClass::Leash;
    for angle in sample_angles(&arcs, r / 2.0 / rho) {
        let slot = point_at(angle);
        if !leashed(slot) {
            continue;
        }
        if walkable(slot) {
            return Some(RoomClass::Free);
        }
        class = RoomClass::Terrain;
    }
    Some(class)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W0-T2 (plan `2026-11-24-solver-retarget-and-shoving`): one constructed case per column,
    /// and none for a full ring. The candidate stands at the origin with a band of 1.5; the
    /// chaser (radius 0.25) stands 4 to the west, so its bearing point is (−1.375, 0).
    #[test]
    fn r10_splits_leash_terrain_unruled_and_free() {
        let asker = ((-4.0, 0.0), 0.25);
        let query = RingQuery {
            asker,
            centre: (0.0, 0.0),
            band: 1.5,
            leash: None,
        };
        let everywhere = |_: Point| true;
        // The bearing point is blocked by a body on it; the rest of the ring is free.
        let west = [((-1.375, 0.0), 0.25)];
        assert_eq!(
            classify_room(query, &west, everywhere),
            Some(RoomClass::Free)
        );
        // Free bearing point: free.
        assert_eq!(classify_room(query, &[], everywhere), Some(RoomClass::Free));
        // A leash round the chaser that holds its bearing point but no free slot.
        let leashed = RingQuery {
            leash: Some(((-4.0, 0.0), 2.7)),
            ..query
        };
        assert_eq!(
            classify_room(leashed, &west, everywhere),
            Some(RoomClass::Leash)
        );
        // Every point east of x = −1.3 is in terrain: the bearing point stands, the slots do not.
        let west_only = |p: Point| p.0 < -1.3;
        assert_eq!(
            classify_room(query, &west, west_only),
            Some(RoomClass::Terrain)
        );
        // The bearing point itself is in terrain, or beyond the leash: unruled.
        assert_eq!(
            classify_room(query, &west, |p: Point| p.0 > -1.3),
            Some(RoomClass::Unruled)
        );
        let short = RingQuery {
            leash: Some(((-4.0, 0.0), 2.0)),
            ..query
        };
        assert_eq!(
            classify_room(short, &west, everywhere),
            Some(RoomClass::Unruled)
        );
        // A ring covered whole has no room; a chaser in the band is free whatever the ring.
        let cover = [((0.0, 0.0), 3.0)];
        assert_eq!(classify_room(query, &cover, everywhere), None);
        let near = RingQuery {
            asker: ((-1.4, 0.0), 0.25),
            ..query
        };
        assert_eq!(
            classify_room(near, &cover, everywhere),
            Some(RoomClass::Free)
        );
        assert!(RoomClass::Free > RoomClass::Unruled && RoomClass::Terrain > RoomClass::Leash);
    }

    /// An empty ring has room; one body blocks only its arc; a ring of touching bodies at its
    /// capacity leaves no room; a body whose grown disc holds the whole ring blocks it all.
    #[test]
    fn room_on_a_ring_follows_the_blocked_arcs() {
        let centre = (0.0, 0.0);
        assert!(ring_has_room(centre, 1.0, 0.25, &[]));
        assert!(ring_has_room(centre, 1.0, 0.25, &[((1.0, 0.0), 0.25)]));
        // Twelve bodies of radius 0.2 on the ring, 30° apart; for an asker of radius 0.1 each
        // blocks acos(1 − 0.3² / 2) ≈ 17.3° either side, more than 15°: the ring is covered.
        let full: Vec<_> = (0..12)
            .map(|k| {
                let a = f64::from(k) * TAU / 12.0;
                ((a.cos(), a.sin()), 0.2)
            })
            .collect();
        assert!(!ring_has_room(centre, 1.0, 0.1, &full));
        // Remove one: a 60° gap is covered only 17.3° from each side, so room opens.
        assert!(ring_has_room(centre, 1.0, 0.1, &full[1..]));
        assert!(!ring_has_room(centre, 1.0, 0.25, &[((0.0, 0.0), 1.5)]));
    }

    /// An arc that straddles angle 0 is split and still covers both sides of it.
    #[test]
    fn an_arc_across_angle_zero_covers_both_sides() {
        let arc = blocked_arc(1.0, 1.0, 0.0, 0.5);
        assert!(arc.is_some_and(|(_, end)| end > TAU), "{arc:?}");
        assert!(blocked_arc(1.0, 3.0, 0.0, 0.5).is_none());
        assert_eq!(blocked_arc(1.0, 0.2, 1.0, 2.0), Some((0.0, TAU)));
        let free = free_arcs((0.0, 0.0), 1.0, 0.25, &[((1.0, 0.0), 0.25)]);
        assert_eq!(free.len(), 1, "{free:?}");
        assert!(!is_free(0.0, &free) && !is_free(TAU - 0.01, &free));
        assert!(is_free(std::f64::consts::PI, &free));
    }

    /// The samples of a free arc start at its middle, stay inside it, and are at most `step`
    /// apart; an empty list gives none.
    #[test]
    fn samples_cover_each_free_arc_from_its_middle() {
        assert_eq!(sample_angles(&[], 0.1), Vec::<f64>::new());
        let samples = sample_angles(&[(1.0, 2.0)], 0.2);
        assert!((samples[0] - 1.5).abs() < 1e-12);
        assert!(samples.iter().all(|&a| a > 1.0 && a < 2.0), "{samples:?}");
        assert_eq!(samples.len(), 5, "{samples:?}");
    }
}
