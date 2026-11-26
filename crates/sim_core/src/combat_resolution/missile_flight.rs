//! # Missile Flight: Speed Phases by Launch Range
//!
//! Owns how fast a weapon missile flies: one constant speed, or the weapon's movers
//! (`WeaponSpec::missile_movers`, `StarCraft` II's `CEffectLaunchMissile` `Movers`), range bands
//! of speed phases chosen once at the release (plan `2026-11-27-weapon-delivery` D-2). A phase
//! ends after a path flown since the release or once a path is left to the impact (`PhaseEnd`,
//! `StarCraft` II's `Outro` read by its sign; plan `2026-11-28-missile-movers-b52` D-2).
//!
//! ## How it fits
//! [`super`] launches and advances projectiles; it asks this module which mover a release
//! chooses ([`choose_mover`]), what speed the missile starts with ([`launch_speed`]), how far a
//! mover missile flies on one tick ([`MoverTick`]) and when its next phase starts
//! ([`enter_due_phases`], after each step and once at the release, so a missile released inside
//! a target-relative end starts in the next phase), and how many ticks the renderer event lasts
//! ([`flight_ticks`]). A
//! constant missile never reaches this module's integration: [`super`] keeps its old step, so
//! every weapon without movers flies byte for byte as before.
//!
//! ## Invariants
//! 1. Integers only: additions, a halving and a `min` of fixed-point values; no division by a
//!    variable, no square root, no float, so every target computes the same flight.
//! 2. A tick's step is the mean of the speeds at its two ends, so a phase of constant
//!    acceleration `a` from `v₀` has flown exactly `v₀k + ak²/2` after `k` ticks until its cap.
//! 3. A phase index never passes the mover's last phase, and the speed never exceeds the
//!    current phase's `max_speed` after a step.
//! 4. A `Flown` end is positive (catalog validation), so it is never due at the release: every
//!    mover without a `Remaining` end starts and flies exactly as before that end existed.

use sim_types::{MissileMover, MissilePhase, PhaseEnd, SimScalar};

/// How a missile's speed develops over its flight, for [`flight_ticks`].
#[derive(Debug, Clone, Copy)]
pub(super) enum FlightProfile<'a> {
    /// One speed for the whole flight, world units per tick.
    Constant(SimScalar),
    /// The phases of the mover chosen at the release.
    Mover(&'a MissileMover),
}

/// The index of the first of `movers` whose bound admits the release's `edge_distance` (world
/// units, centre distance less both collision radii), `≤` inclusive (`IfRangeLTE`), or `None`
/// when the list is empty: a constant missile. A list that admits no distance also gives `None`;
/// catalog validation refuses such a list (its last mover admits any distance).
pub(super) fn choose_mover(movers: &[MissileMover], edge_distance: SimScalar) -> Option<u8> {
    movers
        .iter()
        .position(|mover| mover.up_to_range.is_none_or(|bound| edge_distance <= bound))
        .and_then(|index| u8::try_from(index).ok())
}

/// The speed a missile flying `mover` starts with: its first phase's `speed`, zero if it sets
/// none.
pub(super) fn launch_speed(mover: &MissileMover) -> SimScalar {
    mover
        .phases
        .first()
        .and_then(|phase| phase.speed)
        .unwrap_or(SimScalar::ZERO)
}

/// One tick of a mover missile's flight in `phase`, from the speed it has at the tick's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MoverTick {
    /// Path flown this tick, world units: the mean of the speeds at the tick's two ends.
    pub(super) step: SimScalar,
    /// Speed at the tick's end, world units per tick: raised by the phase's acceleration, at
    /// most its top speed (a speed carried in above it drops to it).
    pub(super) next: SimScalar,
}

impl MoverTick {
    /// The tick of `phase` that starts at `speed`.
    pub(super) fn of(phase: &MissilePhase, speed: SimScalar) -> Self {
        let next = (speed + phase.acceleration).min(phase.max_speed);
        Self {
            step: (speed + next) / 2,
            next,
        }
    }
}

/// Whether `end` is due for a missile that has flown `travelled` since its release and has
/// `remaining` left to its impact, world units: a `Flown` end once the path flown reaches it, a
/// `Remaining` end once the path left is at most it (plan `2026-11-28-missile-movers-b52` D-2).
const fn is_due(end: PhaseEnd, travelled: SimScalar, remaining: SimScalar) -> bool {
    match end {
        PhaseEnd::Flown(after) => travelled.raw() >= after.raw(),
        PhaseEnd::Remaining(within) => remaining.raw() <= within.raw(),
    }
}

/// Moves `phase` on past every phase of `mover` whose end is due ([`is_due`]) for a missile that
/// has flown `travelled` and has `remaining` left to its impact, world units, never past the
/// last; a phase entered with its own `speed` restarts `speed` at it, one without carries
/// `speed` in.
pub(super) fn enter_due_phases(
    mover: &MissileMover,
    phase: &mut u8,
    travelled: SimScalar,
    remaining: SimScalar,
    speed: &mut SimScalar,
) {
    while let Some(current) = mover.phases.get(usize::from(*phase)) {
        let ended = current
            .ends
            .is_some_and(|end| is_due(end, travelled, remaining));
        let Some(entered) = mover.phases.get(usize::from(*phase) + 1).filter(|_| ended) else {
            return;
        };
        *phase += 1;
        if let Some(start) = entered.speed {
            *speed = start;
        }
    }
}

/// The phase of `mover` with index `phase`, or its last when the index has passed it (which
/// [`enter_due_phases`] never does); `None` for a mover without phases, which catalog validation
/// refuses.
pub(super) fn current_phase(mover: &MissileMover, phase: u8) -> Option<&MissilePhase> {
    mover
        .phases
        .get(usize::from(phase))
        .or_else(|| mover.phases.last())
}

/// Ticks a missile of `profile` takes to cover `path` world units, for the renderer event's
/// flight time (presentation only). A constant missile takes `ceil(path / speed)`, `main`'s
/// value exactly; a mover missile steps the same integration as its flight towards a target
/// that stands still (the path left is `path` less the path flown), its phase ends checked once
/// before the first tick and after each step, the tick that covers the rest of the path
/// included, at most `max_ticks` (its lifetime).
pub(super) fn flight_ticks(profile: FlightProfile<'_>, path: SimScalar, max_ticks: u16) -> u32 {
    match profile {
        FlightProfile::Constant(speed) => (path / speed).saturating_ceil().saturating_to_u32(),
        FlightProfile::Mover(mover) => {
            let mut speed = launch_speed(mover);
            let mut phase = 0;
            let mut travelled = SimScalar::ZERO;
            enter_due_phases(mover, &mut phase, travelled, path, &mut speed);
            for tick in 1..=u32::from(max_ticks) {
                let Some(current) = current_phase(mover, phase) else {
                    return tick;
                };
                let flown = MoverTick::of(current, speed);
                if path - travelled <= flown.step {
                    return tick;
                }
                travelled += flown.step;
                speed = flown.next;
                enter_due_phases(mover, &mut phase, travelled, path - travelled, &mut speed);
            }
            u32::from(max_ticks)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{choose_mover, enter_due_phases, flight_ticks, FlightProfile, MoverTick};
    use sim_types::{MissileMover, MissilePhase, PhaseEnd, SimScalar};

    fn scalar(value: f64) -> SimScalar {
        SimScalar::from_num(value)
    }

    /// A phase whose end, if any, is a path flown.
    fn phase(speed: Option<f64>, acceleration: f64, max: f64, ends: Option<f64>) -> MissilePhase {
        MissilePhase {
            speed: speed.map(scalar),
            acceleration: scalar(acceleration),
            max_speed: scalar(max),
            ends: ends.map(|after| PhaseEnd::Flown(scalar(after))),
        }
    }

    /// A phase ending once `within` is left to the impact.
    fn phase_within(speed: Option<f64>, acceleration: f64, max: f64, within: f64) -> MissilePhase {
        MissilePhase {
            ends: Some(PhaseEnd::Remaining(scalar(within))),
            ..phase(speed, acceleration, max, None)
        }
    }

    /// One phase for [`oracle`]: start speed, acceleration, top speed and end, the end's flag
    /// `true` for a path flown and `false` for a path left.
    type OraclePhase = (Option<f64>, f64, f64, Option<(bool, f64)>);

    /// The oracle of plan `2026-11-28-missile-movers-b52` D-2 in `f64` (exact for the dyadic
    /// values used here): the tick on which `phases` cover `path` towards a target standing
    /// still, every end checked once at the release and after each step.
    fn oracle(phases: &[OraclePhase], path: f64) -> u32 {
        let mut index = 0;
        let mut speed = phases[0].0.unwrap_or(0.0);
        let mut travelled = 0.0;
        let enter = |index: &mut usize, speed: &mut f64, travelled: f64| {
            while *index + 1 < phases.len()
                && phases[*index].3.is_some_and(|(flown, end)| {
                    if flown {
                        travelled >= end
                    } else {
                        path - travelled <= end
                    }
                })
            {
                *index += 1;
                if let Some(start) = phases[*index].0 {
                    *speed = start;
                }
            }
        };
        enter(&mut index, &mut speed, travelled);
        for tick in 1..=10_000 {
            let (_, acceleration, max, _) = phases[index];
            let next = (speed + acceleration).min(max);
            let step = f64::midpoint(speed, next);
            if path - travelled <= step {
                return tick;
            }
            travelled += step;
            speed = next;
            enter(&mut index, &mut speed, travelled);
        }
        u32::MAX
    }

    /// W-1: a constant profile's flight is `ceil(path / speed)` exactly, as `main` computes it,
    /// over a table of paths and speeds (every shipped missile speed among them).
    #[test]
    fn a_constant_flight_is_the_ceiling_of_path_over_speed() {
        let catalog = crate::test_support::shipped_catalogs().units;
        let mut speeds: Vec<SimScalar> = catalog
            .archetypes()
            .flat_map(|arch| [&arch.weapon, &arch.ground_weapon, &arch.air_weapon])
            .flatten()
            .filter_map(|weapon| weapon.projectile_speed)
            .collect();
        assert!(!speeds.is_empty(), "the shipped catalog has missiles");
        speeds.extend([
            scalar(0.05),
            scalar(1.0),
            scalar(3.3),
            SimScalar::from_raw(7),
        ]);
        for &speed in &speeds {
            for path in [0.05, 0.5, 1.0, 1.05, 2.24, 5.999, 6.0, 7.375, 12.0, 40.0] {
                let path = scalar(path);
                let expected = (path / speed).saturating_ceil().saturating_to_u32();
                assert_eq!(
                    flight_ticks(FlightProfile::Constant(speed), path, 125),
                    expected,
                    "{path:?} at {speed:?}"
                );
            }
        }
    }

    /// W-2: one phase of constant acceleration covers `v₀k + ak²/2` after `k` ticks, so its
    /// flight is the first `k` that reaches the path; capped, the speed never exceeds the cap.
    #[test]
    fn one_phase_integrates_exactly() {
        let mover = MissileMover {
            up_to_range: None,
            phases: vec![phase(Some(0.25), 0.125, 64.0, None)],
        };
        for path in [0.25, 0.3, 1.0, 3.5, 9.75, 20.0] {
            let expected = (1_u32..=10_000)
                .find(|&k| {
                    let k = f64::from(k);
                    0.25 * k + 0.125 * k * k / 2.0 >= path
                })
                .unwrap_or(0);
            assert_eq!(
                flight_ticks(FlightProfile::Mover(&mover), scalar(path), 125),
                expected,
                "path {path}"
            );
        }
        let capped = phase(None, 0.5, 0.75, None);
        let mut speed = SimScalar::ZERO;
        let mut steps = Vec::new();
        for _ in 0..4 {
            let tick = MoverTick::of(&capped, speed);
            assert!(tick.next <= capped.max_speed);
            steps.push(tick.step);
            speed = tick.next;
        }
        assert_eq!(
            steps,
            vec![scalar(0.25), scalar(0.625), scalar(0.75), scalar(0.75)]
        );
        let carried_in = MoverTick::of(&capped, scalar(2.0));
        assert_eq!(carried_in.next, scalar(0.75), "a speed above the cap drops");
    }

    /// W-3: a phase ends on the first path that reaches its end; the next restarts at its own
    /// speed or carries the speed in; the index never passes the last phase.
    #[test]
    fn a_phase_switches_once_its_end_is_reached() {
        let mover = MissileMover {
            up_to_range: None,
            phases: vec![
                phase(Some(0.25), 0.0, 0.25, Some(1.0)),
                phase(Some(2.0), 0.0, 4.0, Some(1.5)),
                phase(None, 1.0, 4.0, None),
            ],
        };
        let mut index = 0;
        let mut speed = scalar(0.25);
        // A `Flown` end ignores the path left, however short.
        let near = SimScalar::ZERO;
        enter_due_phases(&mover, &mut index, scalar(0.75), near, &mut speed);
        assert_eq!((index, speed), (0, scalar(0.25)), "not yet");
        enter_due_phases(&mover, &mut index, scalar(1.0), near, &mut speed);
        assert_eq!((index, speed), (1, scalar(2.0)), "restarts at its speed");
        enter_due_phases(&mover, &mut index, scalar(1.6), near, &mut speed);
        assert_eq!((index, speed), (2, scalar(2.0)), "carries the speed in");
        enter_due_phases(&mover, &mut index, scalar(100.0), near, &mut speed);
        assert_eq!(index, 2, "never past the last phase");
        let mut index = 0;
        let mut speed = scalar(0.25);
        enter_due_phases(&mover, &mut index, scalar(2.0), near, &mut speed);
        assert_eq!(index, 2, "a long tick passes every phase it ended");
    }

    /// M-2 (plan `2026-11-28-missile-movers-b52` D-2): a `Remaining` end is due once the path
    /// left is at most its distance, inclusive, whatever the path flown; consecutive ends pass
    /// together on one check; a positive `Flown` end is never due at the release (M-4).
    #[test]
    fn a_target_relative_end_is_due_once_the_path_left_is_within_it() {
        let mover = MissileMover {
            up_to_range: None,
            phases: vec![
                phase_within(Some(0.5), 0.0, 0.5, 4.0),
                phase_within(None, 0.25, 1.0, 1.0),
                phase(None, 0.5, 2.0, None),
            ],
        };
        let epsilon = SimScalar::from_raw(1);
        let mut index = 0;
        let mut speed = scalar(0.5);
        enter_due_phases(
            &mover,
            &mut index,
            scalar(100.0),
            scalar(4.0) + epsilon,
            &mut speed,
        );
        assert_eq!(
            (index, speed),
            (0, scalar(0.5)),
            "not yet, however far flown"
        );
        enter_due_phases(&mover, &mut index, SimScalar::ZERO, scalar(4.0), &mut speed);
        assert_eq!(
            (index, speed),
            (1, scalar(0.5)),
            "due at 4, the speed carried in"
        );
        enter_due_phases(&mover, &mut index, SimScalar::ZERO, scalar(1.5), &mut speed);
        assert_eq!(index, 1, "the next end is nearer");
        let mut index = 0;
        enter_due_phases(&mover, &mut index, SimScalar::ZERO, scalar(0.5), &mut speed);
        assert_eq!(index, 2, "a release inside both ends passes both");
        let flown = MissileMover {
            up_to_range: None,
            phases: vec![
                phase(Some(0.5), 0.0, 0.5, Some(0.25)),
                phase(None, 0.5, 2.0, None),
            ],
        };
        let mut index = 0;
        enter_due_phases(
            &flown,
            &mut index,
            SimScalar::ZERO,
            SimScalar::ZERO,
            &mut speed,
        );
        assert_eq!(
            index, 0,
            "a positive `Flown` end is never due at the release"
        );
    }

    /// M-2, M-4, M-7: the flight of a mover with a `Remaining` end over a table of paths around
    /// its distance equals the oracle's, a path inside the end starting in the next phase.
    #[test]
    fn a_target_relative_flight_matches_the_oracle() {
        let phases = [
            (Some(0.5), 0.0, 0.5, Some((false, 4.0))),
            (None, 0.75, 2.0, None),
        ];
        let mover = MissileMover {
            up_to_range: None,
            phases: vec![
                phase_within(Some(0.5), 0.0, 0.5, 4.0),
                phase(None, 0.75, 2.0, None),
            ],
        };
        for path in [
            0.25, 0.5, 1.0, 3.5, 3.75, 4.0, 4.25, 4.5, 4.75, 5.0, 6.0, 8.5, 12.25,
        ] {
            assert_eq!(
                flight_ticks(FlightProfile::Mover(&mover), scalar(path), 125),
                oracle(&phases, path),
                "path {path}"
            );
        }
        // Inside the end from the release: the kill phase's integration from the start speed.
        assert_eq!(
            flight_ticks(FlightProfile::Mover(&mover), scalar(3.0), 125),
            3
        );
        // Outside it: two ticks at 0.5 bring 5 down to 4; then 0.875, 1.625 and the last 1.5.
        assert_eq!(
            flight_ticks(FlightProfile::Mover(&mover), scalar(5.0), 125),
            5
        );
    }

    /// W-4: the first mover whose bound admits the distance, inclusive; none for no movers.
    #[test]
    fn the_first_admitting_band_is_chosen() {
        let band = |bound: Option<f64>| MissileMover {
            up_to_range: bound.map(scalar),
            phases: vec![phase(Some(1.0), 0.0, 1.0, None)],
        };
        let movers = vec![band(Some(1.0)), band(Some(3.0)), band(None)];
        let epsilon = SimScalar::from_raw(1);
        for (distance, expected) in [
            (scalar(-0.5), Some(0)),
            (scalar(1.0), Some(0)),
            (scalar(1.0) + epsilon, Some(1)),
            (scalar(3.0), Some(1)),
            (scalar(3.0) + epsilon, Some(2)),
            (scalar(500.0), Some(2)),
        ] {
            assert_eq!(choose_mover(&movers, distance), expected, "{distance:?}");
        }
        assert_eq!(choose_mover(&[], scalar(1.0)), None);
        assert_eq!(choose_mover(&movers[..2], scalar(4.0)), None);
    }

    /// The flight of a mover never exceeds its lifetime.
    #[test]
    fn a_mover_flight_is_bounded_by_the_lifetime() {
        let mover = MissileMover {
            up_to_range: None,
            phases: vec![phase(Some(0.05), 0.0, 0.05, None)],
        };
        assert_eq!(
            flight_ticks(FlightProfile::Mover(&mover), scalar(40.0), 10),
            10
        );
    }
}
