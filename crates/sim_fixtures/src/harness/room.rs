//! Whether a ring round an enemy has room for one more attacker: the measurement behind the
//! harness's D-R10 diagnostic (plan `2026-11-20-contact-solver` Amendment 17, §17.3).
//!
//! # Responsibility
//! [`ring_has_room`] answers, from outside the simulation and in `f64`, the question the ring rule
//! asks in Q32.32: is any point of the circle of radius `rho` round `centre` clear of every other
//! body's disc grown by the asker's radius? Each body blocks the arc of points within `r + r_o`
//! of its centre, the exact half-width `acos((ρ² + d² − R²) / (2ρd))` of plan Amendment 11, P1;
//! the ring has room when the union of those arcs leaves any angle uncovered.
//!
//! # How it fits
//! [`super::mirror`] counts, per tick, the units that stand on an automatic chase whose target is
//! out of reach while another enemy in their scan range has room on its ring (D-R10). The
//! harness's maps are open, so the ring rule's terrain walk admits every angle; the leash is not
//! modelled (a scan range is far inside the 21 of an automatic chase's leash).
//!
//! # Invariants
//! - Measurement only: nothing here feeds the simulation.
//! - A body whose disc does not reach the ring blocks nothing; one whose grown disc contains the
//!   whole ring blocks all of it.

use super::metrics::Point;
use std::f64::consts::TAU;

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

/// Whether the ring of radius `rho` round `centre` has an angle that no body of `others` (centre
/// and radius) covers for an asker of radius `r`. Every arc is split at `2π`, the arcs are sorted
/// and swept; a gap wider than `1e-9` radians is room.
#[must_use]
pub fn ring_has_room(centre: Point, rho: f64, r: f64, others: &[(Point, f64)]) -> bool {
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
    let mut covered = 0.0_f64;
    for (start, end) in arcs {
        if start > covered + 1e-9 {
            return true;
        }
        covered = covered.max(end);
    }
    covered < TAU - 1e-9
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let arc = blocked_arc(1.0, 1.0, 0.0, 0.5).expect("blocks");
        assert!(arc.1 > TAU, "{arc:?}");
        assert!(blocked_arc(1.0, 3.0, 0.0, 0.5).is_none());
        assert_eq!(blocked_arc(1.0, 0.2, 1.0, 2.0), Some((0.0, TAU)));
    }
}
