//! The known answers of the eight families of plan `2026-11-27-gated-movement` (§5.2, `WP0a`).
//!
//! # Responsibility
//! [`KnownAnswer`] holds the facts of one generated scene that its answer is judged by (which
//! bodies form the wedge, how deep the queue is, where the lanes are), and [`judge`] states each
//! family's answer as a test on the scene's measured row: the base columns of
//! [`super::metrics::ScenarioRow`] and the answer's own columns of
//! [`super::metrics::known::KnownRow`]. The answers are the plan's, written down before any
//! `Gated` code existed, as the design's rule for adding to the suite requires (§3.2):
//!
//! | Family | Holds when | Hard (H-1, H-2, H-7) |
//! |---|---|---|
//! | `hold-line` | every walker completes within the scene's bound; no give-up | no holder displaced by others by more than 2⁻²⁰ in any tick |
//! | `enemy-press` | friend overlap within T7 (per scene: shares of near pair-ticks above D-α's bounds at most 0.1 %, maximum at most 15 %) | no enemy displaced except by its own step; enemy overlap at most 0.5 % |
//! | `deep-queue` | the front fills (`min(k, C)` attackers in range at the end); T7's friend overlap in every rank; M1 ≤ 1.0; M3 ≤ 0.5 | — |
//! | `wedge` | the walker never crosses between the two bodies; never overlaps either beyond D-α's 5 % (friends) or 0.5 % (enemies); M1 = 0; it completes | — |
//! | `hallway` | both files complete; M1 ≤ 1.0; no give-up; no pair changes the side it passes on | — |
//! | `stutter` | — | lag 1 on every move-back; displacement along it at least 0.9 · v · (ticks − 1); fire resumes within one weapon period of being in range |
//! | `surround-moving` | the fill reaches `min(k, C)` within 250 ticks of first contact; M1 ≤ 1.0 | — |
//! | `lanes` | each gap passes at least a quarter of the group; the group's final hull at most twice its packed area | — |
//!
//! # How it fits
//! The generator ([`super::corpus`]) attaches a [`KnownAnswer`] to every scene of a new family;
//! the scene's meter ([`super::metrics::known::KnownMeter`]) measures it; the runner writes the
//! verdict of [`judge`] as the last columns of the family's CSV; and the report
//! ([`super::report`]) states the share of scenes whose answer held. Figures are reported, not
//! judged, until `WP0b`'s first run: that is the last point at which a known answer may change.
//!
//! # Invariants
//! - [`judge`] is a pure function of its arguments.
//! - A failed part is named in [`Judgement::failed`]; [`Judgement::holds`] is true exactly when
//!   none failed, and [`Judgement::hard`] exactly when no hard part failed.

use super::corpus::Lattice;
use super::metrics::known::KnownRow;
use super::metrics::ScenarioRow;

/// D-α's moving bound: friend overlap share of the radius sum a moving pair may reach.
pub const FRIEND_MOVING_BOUND: f64 = 0.05;

/// D-α's standing bound: friend overlap share of the radius sum a standing pair may reach.
pub const FRIEND_STANDING_BOUND: f64 = 0.01;

/// T7's enemy bound: enemy overlap share of the radius sum.
pub const ENEMY_BOUND: f64 = 0.005;

/// T7's share of near pair-ticks that may exceed D-α's bounds (0.1 %).
pub const FRIEND_SHARE: f64 = 0.001;

/// T7's largest friend overlap share.
pub const FRIEND_MAX: f64 = 0.15;

/// T1's bound on M1, reversals per 1,000 moving unit-ticks.
pub const M1_BOUND: f64 = 1.0;

/// T2's bound on M3, standing waste per unit, world units.
pub const M3_BOUND: f64 = 0.5;

/// Ticks after first contact within which surround-moving's fill must reach `min(k, C)` (10 s).
pub const FILL_TICKS: u32 = 250;

/// The share of the stutter step's ideal displacement it must reach (R-11).
pub const STUTTER_SHARE: f64 = 0.9;

/// How far the lanes group's final hull may exceed its packed area.
pub const CLUMP_FACTOR: f64 = 2.0;

/// The stance of the enemy line of an `enemy-press` scene (its three sub-families).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stance {
    /// No order.
    Idle,
    /// Attacking a dummy behind the line, in range of it.
    Attacking,
    /// On Hold Position.
    Holding,
}

impl Stance {
    /// The sub-family's name in the scene's parameters.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Attacking => "attacking",
            Self::Holding => "holding",
        }
    }
}

/// Walkers of a `lanes` scene that crossed the wall through the first and through the second
/// gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LaneUse(pub [u32; 2]);

/// The facts of one scene of a new family that its known answer is judged by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownAnswer {
    /// `hold-line`.
    HoldLine,
    /// `enemy-press`, with the line's stance.
    EnemyPress {
        /// The sub-family.
        stance: Stance,
    },
    /// `deep-queue`: the attackers stand in `ranks` ranks, rank 0 at column `front` (sixty-
    /// fourths, nearest the pillar), each rank `pitch` behind the one before.
    DeepQueue {
        /// The front rank's x, sixty-fourths.
        front: Lattice,
        /// Distance between ranks, sixty-fourths.
        pitch: Lattice,
        /// Ranks.
        ranks: u32,
    },
    /// `wedge`: the two bodies' spawn points (sixty-fourths), and whether they are enemies
    /// (standing) rather than friends (holding).
    Wedge {
        /// The two bodies' spawn points.
        bodies: [(Lattice, Lattice); 2],
        /// Whether the bodies are enemies.
        enemies: bool,
    },
    /// `hallway`.
    Hallway,
    /// `stutter`: the attackers' weapon period, ticks.
    Stutter {
        /// Ticks between two releases of the attackers' weapon.
        period: u32,
    },
    /// `surround-moving`: the ring's capacity `C` and the attackers' band against the target
    /// (weapon range plus both radii, sixty-fourths).
    SurroundMoving {
        /// Discs that fit round the target at the band's edge.
        capacity: u32,
        /// The band's outer edge, centre to centre, sixty-fourths.
        band: Lattice,
    },
    /// `lanes`: the wall's column, the two gaps' rows (`low..high`), and the walkers' radius
    /// (sixty-fourths).
    Lanes {
        /// The wall's column.
        wall: u32,
        /// Each gap's first row and the row past its last.
        gaps: [(u32, u32); 2],
        /// The walkers' radius, sixty-fourths.
        radius: Lattice,
    },
}

/// The verdict of a scene's known answer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Judgement {
    /// Whether every part of the answer held.
    pub holds: bool,
    /// Whether every hard part (H-1, H-2, H-7) held.
    pub hard: bool,
    /// The parts that failed, by column name.
    pub failed: Vec<&'static str>,
}

/// The parts of one answer as they are tested: name, whether hard, whether it held.
type Part = (&'static str, bool, bool);

/// M1 of `base`: reversals per 1,000 moving unit-ticks, 0 when nothing moved.
fn m1(base: &ScenarioRow) -> f64 {
    if base.moving_ticks == 0 {
        return 0.0;
    }
    1000.0 * f64::from(base.rev_amp) / f64::from(base.moving_ticks)
}

/// M3 of `base`: standing waste per unit.
fn m3(base: &ScenarioRow) -> f64 {
    if base.units == 0 {
        return 0.0;
    }
    base.standing_wasted / f64::from(base.units)
}

/// Whether `over` of `ticks` pair-ticks is within T7's share.
fn within_share(over: u32, ticks: u32) -> bool {
    ticks == 0 || f64::from(over) <= FRIEND_SHARE * f64::from(ticks)
}

/// T7's friend overlap on one scene's base columns.
fn friends_within_t7(base: &ScenarioRow) -> bool {
    within_share(base.fo_move_over, base.pair_ticks_move)
        && within_share(base.fo_rest_over, base.pair_ticks_rest)
        && base.fo_move_max.max(base.fo_rest_max) <= FRIEND_MAX
}

/// Whether every walker completed.
fn complete(base: &ScenarioRow) -> bool {
    base.done >= base.expected
}

/// The parts of `known`'s answer on `base` and `row`.
fn parts(known: KnownAnswer, base: &ScenarioRow, row: &KnownRow) -> Vec<Part> {
    match known {
        KnownAnswer::HoldLine => vec![
            ("held_moved", true, row.held_moved_ticks == 0),
            ("complete", false, complete(base)),
            ("giveups", false, base.giveups == 0),
        ],
        KnownAnswer::EnemyPress { .. } => vec![
            ("enemy_moved", true, row.enemy_moved_ticks == 0),
            ("enemy_overlap", true, row.enemy_pair_ov_max <= ENEMY_BOUND),
            ("friend_overlap", false, friends_within_t7(base)),
        ],
        KnownAnswer::DeepQueue { .. } => {
            let ranks_within = row
                .rank_fo_max
                .iter()
                .zip(row.rank_fo_over.iter().zip(&row.rank_pair_ticks))
                .all(|(&max, (&over, &ticks))| max <= FRIEND_MAX && within_share(over, ticks));
            vec![
                ("fill", false, complete(base)),
                ("rank_overlap", false, ranks_within),
                ("M1", false, m1(base) <= M1_BOUND),
                ("M3", false, m3(base) <= M3_BOUND),
            ]
        }
        KnownAnswer::Wedge { enemies, .. } => {
            let bound = if enemies {
                ENEMY_BOUND
            } else {
                FRIEND_MOVING_BOUND
            };
            vec![
                ("wedge_passed", false, row.wedge_passed == 0),
                ("wedge_overlap", false, row.wedge_ov_max <= bound),
                ("M1", false, base.rev_amp == 0),
                ("complete", false, complete(base)),
            ]
        }
        KnownAnswer::Hallway => vec![
            ("complete", false, complete(base)),
            ("M1", false, m1(base) <= M1_BOUND),
            ("giveups", false, base.giveups == 0),
            ("side_changes", false, row.side_changes == 0),
        ],
        KnownAnswer::Stutter { .. } => vec![
            (
                "stutter_lag",
                true,
                row.stutter_windows > 0 && row.stutter_lag_max == 1,
            ),
            ("stutter_displacement", true, row.stutter_short == 0),
            ("stutter_fire", true, row.stutter_fire_late == 0),
        ],
        KnownAnswer::SurroundMoving { .. } => vec![
            ("fill", false, row.fill_time.is_some()),
            ("M1", false, m1(base) <= M1_BOUND),
        ],
        KnownAnswer::Lanes { .. } => {
            let group: u32 = row.lanes.0.iter().sum::<u32>().max(base.expected);
            vec![
                (
                    "lanes",
                    false,
                    row.lanes.0.iter().all(|&count| 4 * count >= group),
                ),
                ("clump", false, row.hull_ratio <= CLUMP_FACTOR),
            ]
        }
    }
}

/// The verdict of `known`'s answer on the scene's base columns `base` and its answer columns
/// `row`.
#[must_use]
pub fn judge(known: KnownAnswer, base: &ScenarioRow, row: &KnownRow) -> Judgement {
    let parts = parts(known, base, row);
    let failed: Vec<&'static str> = parts
        .iter()
        .filter(|(_, _, held)| !held)
        .map(|(name, _, _)| *name)
        .collect();
    Judgement {
        holds: failed.is_empty(),
        hard: parts.iter().all(|(_, hard, held)| !hard || *held),
        failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row whose base columns meet every soft part: everyone complete, nothing moving.
    fn base() -> ScenarioRow {
        ScenarioRow {
            units: 4,
            expected: 4,
            done: 4,
            ..ScenarioRow::default()
        }
    }

    #[test]
    fn an_answer_with_nothing_measured_against_it_holds_where_nothing_is_required() {
        let row = KnownRow::default();
        for known in [
            KnownAnswer::HoldLine,
            KnownAnswer::EnemyPress {
                stance: Stance::Idle,
            },
            KnownAnswer::Hallway,
        ] {
            let verdict = judge(known, &base(), &row);
            assert!(verdict.holds && verdict.hard, "{known:?}: {verdict:?}");
        }
    }

    #[test]
    fn a_soft_failure_is_not_hard_and_a_hard_one_is() {
        let incomplete = ScenarioRow { done: 3, ..base() };
        let verdict = judge(KnownAnswer::HoldLine, &incomplete, &KnownRow::default());
        assert_eq!(verdict.failed, vec!["complete"]);
        assert!(!verdict.holds && verdict.hard);
        let stutter = judge(
            KnownAnswer::Stutter { period: 15 },
            &base(),
            &KnownRow::default(),
        );
        // No move-back measured: the lag part fails, and it is hard.
        assert!(!stutter.hard && stutter.failed == vec!["stutter_lag"]);
    }

    #[test]
    fn m1_and_m3_read_rates_not_counts() {
        let busy = ScenarioRow {
            rev_amp: 2,
            moving_ticks: 1000,
            standing_wasted: 1.0,
            ..base()
        };
        // M1 = 2 per 1,000 fails; M3 = 0.25 per unit holds.
        let row = KnownRow {
            rank_fo_max: vec![0.0],
            rank_fo_over: vec![0],
            rank_pair_ticks: vec![10],
            ..KnownRow::default()
        };
        let verdict = judge(
            KnownAnswer::DeepQueue {
                front: 0,
                pitch: 64,
                ranks: 1,
            },
            &busy,
            &row,
        );
        assert_eq!(verdict.failed, vec!["M1"]);
    }

    #[test]
    fn a_wedge_of_enemies_has_the_enemy_bound() {
        let row = KnownRow {
            wedge_ov_max: 0.01,
            ..KnownRow::default()
        };
        let one = ScenarioRow {
            expected: 1,
            done: 1,
            ..ScenarioRow::default()
        };
        let bodies = [(0, 0), (0, 64)];
        let friends = judge(
            KnownAnswer::Wedge {
                bodies,
                enemies: false,
            },
            &one,
            &row,
        );
        let enemies = judge(
            KnownAnswer::Wedge {
                bodies,
                enemies: true,
            },
            &one,
            &row,
        );
        assert!(friends.holds, "{friends:?}");
        assert_eq!(enemies.failed, vec!["wedge_overlap"]);
    }
}
