//! The behaviour checks of plan `2026-11-24-solver-retarget-and-shoving` (§5.1, §6.1; on branch
//! `solver-followup`) and of the enemy-contact plan (on branch `enemy-contact-plan`), as
//! evidence `2026-11-25-solver-pair-cache` §2 summarises them, measured on a running world: enemy
//! shoving (D-SHOVE and its repair D-SHOVE-E), standing with room elsewhere (D-R10), target
//! returns (D-CHURN), and the counters the checks B1, B3, B6 and B7′ read.
//!
//! # Responsibility
//! [`ContactMeter`] watches a [`SimWorld`] after every tick and keeps, per side (player 0 and
//! player 1; other owners are not counted), a [`SideChecks`]:
//! - **D-SHOVE** ([`SideChecks::shove`]): world units a unit moved during a tick in which it
//!   proposed no motion, an enemy body of its class touched it at the tick's start, and it was
//!   not holding ([`shove_of`]). "Proposed no motion" is read from the tick-start state: its
//!   attack target lay in its band (the solver's "engaged"), or it had no order, `Idle` or
//!   `Stop`. "Touched" is a centre distance of at most the radius sum plus 1/64.
//! - **D-R10** ([`SideChecks::r10`]): unit-ticks a unit stood (no displacement, zero velocity) on
//!   an automatic chase (`AttackTarget`, `AttackOrigin::Acquired`) whose target lay beyond its
//!   band, while another enemy within its scan range had room on its ring, split into the four
//!   columns of [`RoomClass`] (leash, terrain, unruled, free).
//! - **D-CHURN** ([`SideChecks::churn`]): target returns A → B → A within
//!   [`CHURN_WINDOW_TICKS`] ([`ChurnTracker`]), `explained` when at the return tick A's ring had
//!   room for the unit or A outranked B (threat, attack target priority, primary weapon), and
//!   `unexplained` otherwise.
//! - **B1** ([`SideChecks::blocked`]): unit-ticks classified [`Activity::Blocked`].
//! - **D-SHOVE-E** ([`SideChecks::shove_e`],
//!   evidence `2026-11-25-solver-pair-cache` §2): for a unit that proposed no motion and was not holding, per tick, the largest outward
//!   radial part of its displacement over the enemies touching it at the tick's start
//!   ([`shove_e_of`]). A nudge along the unit's own attack ring is tangential to the enemy it
//!   touches and counts only to second order; a shove away from the enemy counts fully.
//! - **B7** ([`SideChecks::shots`], [`SideChecks::unit_ticks`]): releases (a weapon cooldown that
//!   rose, as [`super::swing::infer`] reads them; a unit that releases and dies in one tick is
//!   not counted) and living unit-ticks, so shots per unit-second can be formed.
//! - **B7′** ([`SideChecks::engaged_ticks`]): living unit-ticks at whose end the unit's attack
//!   target lay in its band (the solver's "engaged"), so shots per engaged unit-second can be
//!   formed: a smaller ring capacity lowers B7's rate without lowering this one.
//! - **B3** ([`ContactChecks::enemy_overlap`], [`ContactChecks::enemy_overlap_ticks`]): the
//!   largest overlap of two enemy bodies of one class at the end of any tick, as a fraction of
//!   their radius sum, and the pair-ticks above T7's enemy bound ([`ENEMY_OVERLAP_BOUND`]).
//! - **B6** ([`ContactChecks::friend_overlap`], [`ContactChecks::friend_overlap_ticks`]): the same
//!   for two friendly bodies, the pair-ticks counted above D-α's moving bound (the catalog's
//!   `moving_overlap_permille`), **from the first stepped tick on** (plan
//!   `2026-11-26-friend-crowd-overlap` §2.2, WP0): the bodies as a battle spawned them are not a
//!   tick's result, and the matrix's spawn jitter alone overlaps them by up to 37 %. The
//!   spawn-inclusive maximum is kept as [`CrowdChecks::friend_overlap_with_spawn`], reported only.
//! - **B6 over corrected pairs** ([`CrowdChecks::corrected_overlap`],
//!   [`CrowdChecks::corrected_overlap_ticks`], WP0): the same over the friend pairs the contact
//!   solver corrects, those not both holding (`Entity::cannot_be_pushed` on both is C-2's "none"
//!   row, which no solver iteration touches).
//! - **B8** ([`CrowdChecks::standing_able`], collision-classes plan §7.2 and §10.1 (a)): unit-ticks
//!   in which a living armed unit had, at the tick's start, an enemy body in its band (slop 0,
//!   neither hidden nor cloaked) and its weapon `Ready` at cooldown 0, and during the tick started
//!   no swing, did not turn and did not move ([`stands_while_able`]); and of those, the ticks in
//!   which a body of a class that does not touch its own (`CollisionClass::touches`) touched it
//!   at the tick's start
//!   ([`CrowdChecks::standing_able_cross`]).
//! - **Solver extension** ([`CrowdChecks::extended_ticks`], [`CrowdChecks::extended_iterations`],
//!   C-3′ of the same plan): ticks on which the contact solver ran more averaged iterations than
//!   the catalog's `contact.iterations`, and the iterations beyond it, summed.
//!
//! The columns of plan `2026-11-27-gated-movement` (`WP0a`) are kept apart, in [`MovementChecks`]
//! ([`ContactMeter::movement`]), so a table written before them still reads:
//! - **H-1** ([`MovementSide::held_moved_ticks`], [`MovementSide::held_moved_max`]): unit-ticks,
//!   and the largest displacement, of a unit that at the tick's start was holding or had its
//!   attack target in its band, proposed no step ([`moved_by_others`]), and was displaced by
//!   more than [`HELD_EPSILON`];
//! - **M1 and M1-hi** ([`MovementSide::moving_ticks`], [`MovementSide::reversals`],
//!   [`MovementSide::reversals_hi`]) with the harness's definition
//!   ([`crate::harness::metrics::reverses`]): a moving unit-tick is a step above `v / 8`; a
//!   reversal two consecutive steps of at least `v / 4` (`v / 2` for M1-hi) whose cosine is
//!   below −0.5;
//! - the cost probes of the world ([`sim_core::movement::MovementProbes`]), summed over the run;
//! - Amendment 3 of that plan (items 4 and 6): H-1 split by the first-contact exclusion
//!   ([`MovementSide::held_moved_first_contact_ticks`], [`MovementSide::held_moved_clean_ticks`],
//!   [`MovementSide::held_moved_clean_max`]: a displacement of a unit that, at the tick's start,
//!   was in a pair that already overlapped beyond the dead band when it first met, a spawn or an
//!   unload, [`FirstContactPairs`], is excluded and counted apart), and H-2 as repaired
//!   ([`MovementSide::shove_e_pair`]: D-SHOVE-E counted only on ticks on which enemy pairs moved
//!   the unit, [`SimWorld::enemy_pair_push`], outside the exclusion; the original D-SHOVE-E stays
//!   reported). The world's enemy-pair probe exists under the `Gated` pipeline only, so the
//!   repaired H-2 reads 0 under every other pipeline and is defined for `Gated`.
//!
//! A body, for the overlaps and for "touching", is a living unit that does not pass through
//! units (`Entity::passes_through_units`: hidden, mineral walking or collision suppressed) and
//! whose archetype is not immobile by default (structures stand on footprints, not discs).
//!
//! # How it fits
//! The whole-matrix mode ([`super::matrix`]) runs one meter per battle and writes its totals as
//! the last columns of `matrix.csv`; the harness's mirror family ([`crate::harness::mirror`])
//! writes D-SHOVE and D-R10 per scene. Ring room is the geometry of [`crate::harness::room`],
//! read at tick-start positions (the positions of the previous observation), with the bodies of
//! the asker's collision class ([`SimWorld::collision_class`]: ground, air, burrowed, or a class
//! the catalog authors; plan `2026-11-25-collision-classes` R-8). The leash is the unit's `leash_origin` (its
//! tick-start position when unset) and the catalog's acquire leash radius; terrain is a ring
//! point whose cell is not connected ground to the ring's centre (air bodies have none). Neither
//! the ring walk nor vision is modelled. Everything is `f64`, for measurement only.
//!
//! # Invariants
//! - The meter only reads the world; observing a battle does not change it.
//! - A unit's first observed tick counts nothing but its unit-tick: it has no tick-start state.
//! - The first observation (the starting state) adds to no friend overlap but
//!   [`CrowdChecks::friend_overlap_with_spawn`].
//! - Counts are per side and never decrease; the enemy overlap is a running maximum.

use super::activity::{classify, Activity, ActivityInput};
use super::{attack_target, in_band, in_band_at, to_f64};
use crate::harness::metrics::{reverses, Point};
use crate::harness::room::{classify_room, RingQuery, RoomClass};
use sim_core::movement::{CollisionClass, MovementLayer, MovementProbes};
use sim_core::{is_threat_to, Entity, SimWorld, UnitArchetype, WeaponPhase};
use sim_types::{AttackOrigin, EntityId, PlayerId, SimAngle, SimScalar, SimVec2, UnitOrder};
use std::collections::BTreeMap;

/// Ticks within which a return to the previous target counts as churn (1 s).
pub const CHURN_WINDOW_TICKS: u32 = 25;

/// Extra centre distance beyond the radius sum, world units, within which an enemy touches a
/// unit for D-SHOVE (the plan's 1/64, the simulation's standing drift).
pub const SHOVE_TOUCH_SLACK: f64 = 1.0 / 64.0;

/// T7's enemy bound (plan `2026-11-20-contact-solver` Amendment 11, P7): an enemy overlap above
/// this fraction of the radius sum at the end of a tick counts in
/// [`ContactChecks::enemy_overlap_ticks`].
pub const ENEMY_OVERLAP_BOUND: f64 = 0.005;

/// The displacement in one tick above which a unit counts as moved, world units (2⁻²⁰, H-1).
pub const HELD_EPSILON: f64 = 1.0 / 1_048_576.0;

/// One side's columns of plan `2026-11-27-gated-movement` over a run (module documentation).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MovementSide {
    /// H-1: unit-ticks of a holding or engaged unit displaced by others.
    pub held_moved_ticks: u32,
    /// H-1: the largest such displacement, world units.
    pub held_moved_max: f64,
    /// M1's denominator: unit-ticks whose step exceeded `v / 8`.
    pub moving_ticks: u32,
    /// M1: reversals.
    pub reversals: u32,
    /// M1-hi: reversals of steps of at least `v / 2`.
    pub reversals_hi: u32,
    /// H-1's first-contact exclusion: unit-ticks of [`Self::held_moved_ticks`] whose unit was, at
    /// the tick's start, in a pair of [`FirstContactPairs`].
    pub held_moved_first_contact_ticks: u32,
    /// H-1 as judged: [`Self::held_moved_ticks`] less the excluded ones.
    pub held_moved_clean_ticks: u32,
    /// H-1 as judged: the largest displacement of [`Self::held_moved_clean_ticks`], world units.
    pub held_moved_clean_max: f64,
    /// H-2 as repaired (Amendment 3, item 4): D-SHOVE-E counted only on unit-ticks on which
    /// enemy pairs moved the unit and its tick-start state was outside the exclusion, world units.
    pub shove_e_pair: f64,
    /// H-1 as Amendment 3a redefines it (holding units only), judged: unit-ticks a unit holding at
    /// the tick's start, outside the first-contact exclusion, was displaced by others.
    pub hold_moved_clean_ticks: u32,
    /// The largest displacement of [`Self::hold_moved_clean_ticks`], world units.
    pub hold_moved_clean_max: f64,
    /// H-1's exclusion for holding units: such unit-ticks in a first-contact pair.
    pub hold_moved_first_contact_ticks: u32,
    /// H-1b (Amendment 3a, reported): unit-ticks a unit attacking at the tick's start, not
    /// holding, was displaced by others.
    pub attack_moved_ticks: u32,
    /// H-1b: the largest such displacement, world units.
    pub attack_moved_max: f64,
    /// H-1b, judged: unit-ticks on which such a displacement took the unit out of its band
    /// ([`left_band`]).
    pub attack_left_band_ticks: u32,
}

/// H-1b's test (plan `2026-11-27-gated-movement` Amendment 3a): whether the unit `id`, which
/// others displaced from `start`, ended the tick out of its weapon band against its attack
/// target where it stands now, though from `start` it would have been in it.
#[must_use]
pub fn left_band(world: &SimWorld, id: EntityId, start: SimVec2) -> bool {
    let Some(entity) = world.entities.get(&id) else {
        return false;
    };
    let Ok(arch) = world.catalog.get(entity.unit_type_id) else {
        return false;
    };
    let Some(target) = attack_target(entity)
        .and_then(|t| world.entities.get(&t))
        .filter(|t| t.is_alive())
    else {
        return false;
    };
    in_band_at(world, (entity, arch), start, target)
        && !in_band(world, entity, arch, target, SimScalar::ZERO)
}

/// The pairs that already overlapped beyond the dead band when they first met: a pair of bodies
/// of which one was first observed while overlapping the other by more than
/// `dead_band_permille` of their radius sum (a spawn, an unload). A pair stays in the set while
/// its bodies stay in contact (overlapping) and alive. H-1 and H-2 exclude a displacement of a
/// unit in such a pair at the tick's start (plan `2026-11-27-gated-movement` §6.2, built by its
/// Amendment 3, item 6) and count it apart.
#[derive(Debug, Clone, Default)]
pub struct FirstContactPairs {
    /// Every body observed last time, with its position and radius.
    seen: BTreeMap<EntityId, (Point, f64)>,
    /// The pairs, lower id first.
    pairs: std::collections::BTreeSet<(EntityId, EntityId)>,
}

impl FirstContactPairs {
    /// An empty set, before any observation.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `id` is in a pair of the set, as of the latest observation.
    #[must_use]
    pub fn involves(&self, id: EntityId) -> bool {
        self.pairs.iter().any(|&(a, b)| a == id || b == id)
    }

    /// Observes `world` after a tick (or its starting state): a body seen for the first time
    /// joins a pair with every body it overlaps beyond the dead band; a pair that no longer
    /// overlaps, or lost a body, leaves the set.
    pub fn observe(&mut self, world: &SimWorld) {
        let band = f64::from(world.catalog.movement_rules().contact().dead_band_permille) / 1000.0;
        let mut now = BTreeMap::new();
        for (&id, entity) in &world.entities {
            let Ok(arch) = world.catalog.get(entity.unit_type_id) else {
                continue;
            };
            if entity.is_alive() && !entity.passes_through_units() && !arch.is_immobile_by_default()
            {
                now.insert(id, (point(entity.position), to_f64(arch.collision_radius)));
            }
        }
        let overlap = |(pa, ra): (Point, f64), (pb, rb): (Point, f64)| {
            let sum = ra + rb;
            (sum - (pa.0 - pb.0).hypot(pa.1 - pb.1)) / sum
        };
        self.pairs.retain(|(a, b)| match (now.get(a), now.get(b)) {
            (Some(&one), Some(&two)) => overlap(one, two) > 0.0,
            _ => false,
        });
        for (&id, &body) in now.iter().filter(|(id, _)| !self.seen.contains_key(id)) {
            for (&other, &them) in &now {
                if other != id && overlap(body, them) > band {
                    self.pairs.insert((id.min(other), id.max(other)));
                }
            }
        }
        self.seen = now;
    }
}

/// Both sides' columns of plan `2026-11-27-gated-movement`, and the cost probes, over a run.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MovementChecks {
    /// Per side, indexed by owner (player 0, player 1).
    pub sides: [MovementSide; 2],
    /// The world's cost probes ([`SimWorld::movement_probes`]) summed over the run's ticks.
    pub gate_rays: u64,
    /// See [`Self::gate_rays`].
    pub orbit_refusals: u64,
    /// See [`Self::gate_rays`].
    pub terrain_transfers: u64,
}

/// What a unit was doing at a tick's start, as the checks read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts about one unit's tick-start state, read by different checks"
)]
pub struct Standing {
    /// On Hold Position, unpushable or immobile.
    pub holding: bool,
    /// Its attack target lies in its band (the solver's "engaged").
    pub engaged: bool,
    /// It proposed no motion: engaged, or no order, `Idle` or `Stop`.
    pub proposed_none: bool,
    /// Holding or engaged: a unit no other body may move (H-1).
    pub held: bool,
}

/// The tick-start [`Standing`] of `entity`, of archetype `arch`, in `world`.
#[must_use]
pub fn standing_of(world: &SimWorld, entity: &Entity, arch: &UnitArchetype) -> Standing {
    let engaged = attack_target(entity)
        .and_then(|id| world.entities.get(&id))
        .filter(|enemy| enemy.is_alive())
        .is_some_and(|enemy| in_band(world, entity, arch, enemy, SimScalar::ZERO));
    let standing_order = matches!(
        entity.order_queue.front(),
        None | Some(UnitOrder::Idle | UnitOrder::Stop)
    );
    let holding = entity.is_on_hold() || entity.cannot_be_pushed();
    Standing {
        holding,
        engaged,
        proposed_none: engaged || standing_order,
        held: holding || engaged,
    }
}

/// The displacement `moved` (world units) of a unit that others caused: all of it when the unit
/// stood still at the tick's start (`still`) and proposed no step this tick (`proposed`, the
/// world's [`SimWorld::proposed_step`], absent or zero); otherwise none, since a unit that
/// stepped moved at least partly by its own step (H-1, H-2).
#[must_use]
pub fn moved_by_others(still: bool, proposed: Option<Point>, moved: Point) -> f64 {
    let stepped = proposed.is_some_and(|(x, y)| x != 0.0 || y != 0.0);
    if still && !stepped {
        moved.0.hypot(moved.1)
    } else {
        0.0
    }
}

/// One side's checks over a run.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SideChecks {
    /// D-SHOVE: world units moved by contact while proposing no motion.
    pub shove: f64,
    /// D-SHOVE-E: world units moved outward from touching enemies while proposing no motion
    /// ([`shove_e_of`]).
    pub shove_e: f64,
    /// Living unit-ticks ending with the unit's attack target in its band (B7′).
    pub engaged_ticks: u32,
    /// D-R10 unit-ticks, indexed by [`RoomClass::index`]: leash, terrain, unruled, free.
    pub r10: [u32; 4],
    /// D-CHURN returns: explained, unexplained.
    pub churn: [u32; 2],
    /// Unit-ticks classified [`Activity::Blocked`].
    pub blocked: u32,
    /// Weapon releases of living units.
    pub shots: u32,
    /// Living unit-ticks.
    pub unit_ticks: u32,
}

/// The columns of plan `2026-11-26-friend-crowd-overlap` (WP0's measurement repair, B8's probe and
/// C-3′'s counters) over a run.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CrowdChecks {
    /// The largest overlap of two friendly bodies of one class over every observation, the
    /// starting state included, as a fraction of their radius sum (B6 before the repair).
    pub friend_overlap_with_spawn: f64,
    /// The largest overlap of two friendly bodies of one class, not both holding, at the end of
    /// any stepped tick, as a fraction of their radius sum.
    pub corrected_overlap: f64,
    /// Those pairs' pair-ticks above D-α's moving bound at the end of a stepped tick.
    pub corrected_overlap_ticks: u32,
    /// B8 per side, indexed by owner: unit-ticks standing while able to fight
    /// ([`stands_while_able`]).
    pub standing_able: [u32; 2],
    /// The unit-ticks of [`Self::standing_able`] in which a body of another class touched the
    /// unit at the tick's start.
    pub standing_able_cross: [u32; 2],
    /// Ticks on which the contact solver ran more averaged iterations than `contact.iterations`.
    pub extended_ticks: u32,
    /// The averaged iterations beyond `contact.iterations`, summed over the run's ticks.
    pub extended_iterations: u32,
}

/// Both sides' checks and the enemy overlap of a run.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ContactChecks {
    /// Per side, indexed by owner (player 0, player 1).
    pub sides: [SideChecks; 2],
    /// The largest overlap of two enemy bodies of one class at the end of any tick, as a
    /// fraction of their radius sum.
    pub enemy_overlap: f64,
    /// Enemy pair-ticks above [`ENEMY_OVERLAP_BOUND`] at the end of a tick.
    pub enemy_overlap_ticks: u32,
    /// The largest overlap of two friendly bodies of one class at the end of any tick, as a
    /// fraction of their radius sum.
    pub friend_overlap: f64,
    /// Friendly pair-ticks above D-α's moving bound at the end of a tick.
    pub friend_overlap_ticks: u32,
    /// The columns of plan `2026-11-26-friend-crowd-overlap`; `None` for a row read from a table
    /// written before them, whose [`Self::friend_overlap`] and [`Self::friend_overlap_ticks`]
    /// then include the starting state.
    pub crowd: Option<CrowdChecks>,
}

/// B8 for one unit and one tick: the unit was `able` at the tick's start (living, armed, an
/// enemy body in its band, its weapon `Ready` at cooldown 0), it stayed `still` (neither moved nor
/// turned), and its weapon is still `Ready` at cooldown 0 after the tick (it started no swing).
#[must_use]
pub const fn stands_while_able(able: bool, still: bool, ready_after: bool) -> bool {
    able && still && ready_after
}

/// D-SHOVE-E for one unit and one tick (evidence `2026-11-25-solver-pair-cache` §2):
/// `0` unless the unit proposed no motion and was not holding; otherwise the largest
/// `max(0, Δx · (s_u − s_e) / |s_u − s_e|)` over the enemies touching it at the tick's start,
/// `moved` being its displacement `Δx` (world units) and `away` the offsets `s_u − s_e` from each
/// touching enemy's tick-start centre to its own. An enemy whose centre coincided with the unit's
/// gives no direction and counts the whole displacement.
#[must_use]
pub fn shove_e_of(
    proposed_none: bool,
    holding: bool,
    moved: Point,
    away: impl IntoIterator<Item = Point>,
) -> f64 {
    if !proposed_none || holding {
        return 0.0;
    }
    away.into_iter()
        .map(|(x, y)| {
            let length = x.hypot(y);
            if length > 0.0 {
                ((moved.0 * x + moved.1 * y) / length).max(0.0)
            } else {
                moved.0.hypot(moved.1)
            }
        })
        .fold(0.0, f64::max)
}

/// D-SHOVE for one unit and one tick: `moved` (world units) counts when the unit proposed no
/// motion, was not holding, and an enemy touched it at the tick's start; otherwise nothing.
#[must_use]
pub fn shove_of(proposed_none: bool, holding: bool, enemy_touching: bool, moved: f64) -> f64 {
    if proposed_none && !holding && enemy_touching {
        moved
    } else {
        0.0
    }
}

/// One unit's last target switch, for D-CHURN: tick, from, to.
type Switch = (u32, EntityId, EntityId);

/// Per-unit memory of the last target switch, and the return test of D-CHURN.
#[derive(Debug, Clone, Default)]
pub struct ChurnTracker {
    last: BTreeMap<EntityId, Switch>,
}

impl ChurnTracker {
    /// Records `unit`'s switch from `from` to `to` at `tick`. Returns `None` for a switch that is
    /// no return, and for a return (the previous switch went from `to` to `from` at most
    /// [`CHURN_WINDOW_TICKS`] before) `Some(explained())`.
    pub fn switch(
        &mut self,
        unit: EntityId,
        tick: u32,
        (from, to): (EntityId, EntityId),
        explained: impl FnOnce() -> bool,
    ) -> Option<bool> {
        let previous = self.last.insert(unit, (tick, from, to));
        match previous {
            Some((at, old_from, old_to))
                if old_from == to
                    && old_to == from
                    && tick.saturating_sub(at) <= CHURN_WINDOW_TICKS =>
            {
                Some(explained())
            }
            _ => None,
        }
    }
}

/// What the meter keeps of one unit between observations.
#[derive(Debug, Clone, Copy)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts about one unit's tick-start state, read by different checks"
)]
struct Seen {
    owner: PlayerId,
    position: SimVec2,
    at: Point,
    radius: f64,
    /// The unit's collision class ([`SimWorld::collision_class`]): two bodies touch, overlap and
    /// block rings for each other only when their classes touch (plan
    /// `2026-11-25-collision-classes` D-3, R-8).
    class: CollisionClass,
    /// The unit's movement layer, for the terrain of ring points.
    layer: MovementLayer,
    /// A body for the overlaps and for touching (module documentation).
    body: bool,
    engaged: bool,
    proposed_none: bool,
    holding: bool,
    /// Holding or engaged ([`Standing::held`]).
    held: bool,
    /// Top speed, world units per tick, for M1's floors.
    speed: f64,
    target: Option<EntityId>,
    cooldown: u16,
    heading: SimAngle,
    /// B8's premise: its weapon `Ready` at cooldown 0 and an enemy body in its band.
    able: bool,
}

/// The meter of the module documentation.
#[derive(Debug, Clone, Default)]
pub struct ContactMeter {
    tick: u32,
    seen: BTreeMap<EntityId, Seen>,
    churn: ChurnTracker,
    checks: ContactChecks,
    crowd: CrowdChecks,
    /// Each unit's step of the latest tick, for M1.
    steps: BTreeMap<EntityId, Point>,
    movement: MovementChecks,
    /// H-1's and H-2's first-contact exclusion, as of the latest observation.
    first: FirstContactPairs,
}

impl ContactMeter {
    /// A meter with nothing observed.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The checks so far.
    #[must_use]
    pub const fn total(&self) -> ContactChecks {
        let mut checks = self.checks;
        checks.crowd = Some(self.crowd);
        checks
    }

    /// The columns of plan `2026-11-27-gated-movement` so far (module documentation).
    #[must_use]
    pub const fn movement(&self) -> MovementChecks {
        self.movement
    }

    /// Records the state `world` has reached after one more tick (the first call records the
    /// starting state).
    pub fn observe(&mut self, world: &SimWorld) {
        let mut now = BTreeMap::new();
        for (&id, entity) in &world.entities {
            if !entity.is_alive() {
                continue;
            }
            let Ok(arch) = world.catalog.get(entity.unit_type_id) else {
                continue;
            };
            let Some(class) = world.collision_class(id) else {
                continue;
            };
            now.insert(id, seen(world, (entity, arch), class));
        }
        for (&id, current) in &now {
            let Some(side) = side_of(current.owner) else {
                continue;
            };
            self.checks.sides[side].unit_ticks += 1;
            self.checks.sides[side].engaged_ticks += u32::from(current.engaged);
            let (Some(entity), Some(&before)) = (world.entities.get(&id), self.seen.get(&id))
            else {
                continue;
            };
            self.count_unit(world, (id, entity), &before, side);
        }
        let contact = world.catalog.movement_rules().contact();
        let moving = f64::from(contact.moving_overlap_permille) / 1000.0;
        self.count_overlaps(&now, moving);
        let beyond =
            u32::from(world.contact_iterations()).saturating_sub(u32::from(contact.iterations));
        self.crowd.extended_ticks += u32::from(beyond > 0);
        self.crowd.extended_iterations = self.crowd.extended_iterations.saturating_add(beyond);
        if !self.seen.is_empty() {
            let probes: MovementProbes = world.movement_probes();
            self.movement.gate_rays += u64::from(probes.gate_rays);
            self.movement.orbit_refusals += u64::from(probes.orbit_refusals);
            self.movement.terrain_transfers += u64::from(probes.terrain_transfers);
        }
        self.count_movement(world, &now);
        self.first.observe(world);
        self.seen = now;
        self.tick += 1;
    }

    /// Adds every side's H-1 and M1 counts of the tick that ended in `now`.
    fn count_movement(&mut self, world: &SimWorld, now: &BTreeMap<EntityId, Seen>) {
        for (&id, current) in now {
            let Some(side) = side_of(current.owner) else {
                continue;
            };
            let Some(before) = self.seen.get(&id) else {
                continue;
            };
            let moved = (current.at.0 - before.at.0, current.at.1 - before.at.1);
            let counts = &mut self.movement.sides[side];
            let proposed = world.proposed_step(id).map(point);
            let displaced = moved_by_others(before.held, proposed, moved);
            if displaced > HELD_EPSILON {
                counts.held_moved_ticks += 1;
                counts.held_moved_max = counts.held_moved_max.max(displaced);
                if self.first.involves(id) {
                    counts.held_moved_first_contact_ticks += 1;
                } else {
                    counts.held_moved_clean_ticks += 1;
                    counts.held_moved_clean_max = counts.held_moved_clean_max.max(displaced);
                }
            }
            let held = moved_by_others(before.holding, proposed, moved);
            if held > HELD_EPSILON {
                if self.first.involves(id) {
                    counts.hold_moved_first_contact_ticks += 1;
                } else {
                    counts.hold_moved_clean_ticks += 1;
                    counts.hold_moved_clean_max = counts.hold_moved_clean_max.max(held);
                }
            }
            let attack = moved_by_others(before.engaged && !before.holding, proposed, moved);
            if attack > HELD_EPSILON {
                counts.attack_moved_ticks += 1;
                counts.attack_moved_max = counts.attack_moved_max.max(attack);
                counts.attack_left_band_ticks += u32::from(left_band(world, id, before.position));
            }
            let speed = current.speed;
            if moved.0.hypot(moved.1) > speed / 8.0 {
                counts.moving_ticks += 1;
            }
            if let Some(&last) = self.steps.get(&id) {
                if reverses(last, moved, speed / 4.0, -0.5) {
                    counts.reversals += 1;
                    if reverses(last, moved, speed / 2.0, -0.5) {
                        counts.reversals_hi += 1;
                    }
                }
            }
            self.steps.insert(id, moved);
        }
    }

    /// Adds one unit's tick to its side's checks; `before` is its tick-start state.
    fn count_unit(
        &mut self,
        world: &SimWorld,
        (id, entity): (EntityId, &Entity),
        before: &Seen,
        side: usize,
    ) {
        let Ok(arch) = world.catalog.get(entity.unit_type_id) else {
            return;
        };
        let at = point(entity.position);
        let moved = (at.0 - before.at.0).hypot(at.1 - before.at.1);
        let away = self.touching_enemies(id, before);
        let delta = (at.0 - before.at.0, at.1 - before.at.1);
        let checks = &mut self.checks.sides[side];
        checks.shove += shove_of(
            before.proposed_none,
            before.holding,
            !away.is_empty(),
            moved,
        );
        let shove_e = shove_e_of(before.proposed_none, before.holding, delta, away);
        checks.shove_e += shove_e;
        let pushed = to_f64(world.enemy_pair_push(id)) > HELD_EPSILON;
        if pushed && !self.first.involves(id) {
            self.movement.sides[side].shove_e_pair += shove_e;
        }
        checks.shots += u32::from(entity.weapon_cooldown_remaining > before.cooldown);
        checks.blocked += u32::from(is_blocked(world, entity, arch));
        let still = entity.position == before.position && entity.heading == before.heading;
        let ready_after = is_ready(entity);
        if stands_while_able(before.able, still, ready_after) {
            self.crowd.standing_able[side] += 1;
            self.crowd.standing_able_cross[side] += u32::from(self.touched_across(id, before));
        }
        if let Some(class) = self.stands_with_room(world, (id, entity, arch), before) {
            self.checks.sides[side].r10[class.index()] += 1;
        }
        let target = attack_target(entity);
        if let (Some(from), Some(to)) = (before.target, target) {
            if from != to && world.entities.get(&from).is_some_and(Entity::is_alive) {
                let seen = &self.seen;
                let tick = self.tick;
                let explained = || returns_with_reason(world, (id, entity, arch), (from, to), seen);
                if let Some(reason) = self.churn.switch(id, tick, (from, to), explained) {
                    self.checks.sides[side].churn[usize::from(!reason)] += 1;
                }
            }
        }
    }

    /// The offsets from every enemy body of `before`'s class that touched it at the tick's start
    /// to it (`s_u − s_e`); empty when none did or it is no body.
    fn touching_enemies(&self, id: EntityId, before: &Seen) -> Vec<Point> {
        if !before.body {
            return Vec::new();
        }
        self.seen
            .iter()
            .filter(|(&other, body)| {
                other != id
                    && body.body
                    && body.owner != before.owner
                    && body.class.touches(before.class)
                    && distance(body.at, before.at)
                        <= body.radius + before.radius + SHOVE_TOUCH_SLACK
            })
            .map(|(_, body)| (before.at.0 - body.at.0, before.at.1 - body.at.1))
            .collect()
    }

    /// Whether a body of a class that does not touch `before`'s touched it at the tick's start (B8's
    /// cross-class column); `false` when it is no body.
    fn touched_across(&self, id: EntityId, before: &Seen) -> bool {
        before.body
            && self.seen.iter().any(|(&other, body)| {
                other != id
                    && body.body
                    && !body.class.touches(before.class)
                    && distance(body.at, before.at)
                        <= body.radius + before.radius + SHOVE_TOUCH_SLACK
            })
    }

    /// Adds the end-of-tick overlaps of `bodies` to B3's and B6's counters; `moving` is D-α's
    /// moving bound as a fraction of the radius sum. On the first observation (the starting
    /// state) only the spawn-inclusive friend maximum is kept.
    fn count_overlaps(&mut self, bodies: &BTreeMap<EntityId, Seen>, moving: f64) {
        let list: Vec<&Seen> = bodies.values().filter(|b| b.body).collect();
        let stepped = self.tick > 0;
        let (checks, crowd) = (&mut self.checks, &mut self.crowd);
        for (index, a) in list.iter().enumerate() {
            for b in &list[index + 1..] {
                if !a.class.touches(b.class) {
                    continue;
                }
                let sum = a.radius + b.radius;
                let overlap = (sum - distance(a.at, b.at)) / sum;
                if a.owner == b.owner {
                    crowd.friend_overlap_with_spawn = crowd.friend_overlap_with_spawn.max(overlap);
                    if !stepped {
                        continue;
                    }
                    checks.friend_overlap = checks.friend_overlap.max(overlap);
                    checks.friend_overlap_ticks += u32::from(overlap > moving);
                    if !(a.holding && b.holding) {
                        crowd.corrected_overlap = crowd.corrected_overlap.max(overlap);
                        crowd.corrected_overlap_ticks += u32::from(overlap > moving);
                    }
                } else {
                    checks.enemy_overlap = checks.enemy_overlap.max(overlap);
                    checks.enemy_overlap_ticks += u32::from(overlap > ENEMY_OVERLAP_BOUND);
                }
            }
        }
    }

    /// D-R10's class for a unit that stood this tick on an automatic chase whose target lay
    /// beyond its band: the greatest [`RoomClass`] over the other enemies in its scan range whose
    /// ring has room, read at the tick's start; `None` when the unit did not stand so, or no
    /// other enemy has room.
    fn stands_with_room(
        &self,
        world: &SimWorld,
        (id, entity, arch): (EntityId, &Entity, &UnitArchetype),
        before: &Seen,
    ) -> Option<RoomClass> {
        let Some(&UnitOrder::AttackTarget {
            target,
            origin: AttackOrigin::Acquired,
        }) = entity.order_queue.front()
        else {
            return None;
        };
        if entity.position != before.position || entity.velocity != SimVec2::ZERO {
            return None;
        }
        let target_body = self.seen.get(&target)?;
        let target_entity = world.entities.get(&target)?;
        if band_against(world, entity, target_entity)
            .is_none_or(|band| distance(before.at, target_body.at) <= band)
        {
            return None;
        }
        let leash = Some(leash_of(world, entity, before));
        self.seen
            .iter()
            .filter(|(&other, body)| other != target && other != id && body.owner != before.owner)
            .filter_map(|(&enemy, body)| {
                let band = band_against(world, entity, world.entities.get(&enemy)?)?;
                let reach = band - before.radius - body.radius;
                let scan = to_f64(arch.scan_range).max(reach);
                if distance(before.at, body.at) - before.radius - body.radius > scan {
                    return None;
                }
                self.room_on(world, (id, before), (enemy, body), band, leash)
            })
            .max()
    }

    /// The [`RoomClass`] of `enemy`'s ring for the chaser `asker` at the tick's start.
    fn room_on(
        &self,
        world: &SimWorld,
        (asker, chaser): (EntityId, &Seen),
        (enemy, body): (EntityId, &Seen),
        band: f64,
        leash: Option<(Point, f64)>,
    ) -> Option<RoomClass> {
        let others = ring_bodies(&self.seen, chaser.class, &[asker, enemy]);
        let query = RingQuery {
            asker: (chaser.at, chaser.radius),
            centre: body.at,
            band,
            leash,
        };
        classify_room(query, &others, |p| {
            walkable(world, chaser.layer, body.at, p)
        })
    }
}

/// Every body whose class touches `class`, but `excluded`: centre and radius.
fn ring_bodies(
    seen: &BTreeMap<EntityId, Seen>,
    class: CollisionClass,
    excluded: &[EntityId],
) -> Vec<(Point, f64)> {
    seen.iter()
        .filter(|(id, body)| body.class.touches(class) && !excluded.contains(id))
        .map(|(_, body)| (body.at, body.radius))
        .collect()
}

/// Whether a return of `unit` from `from` to `to` is explained (D-CHURN): `to`'s ring had room
/// for it at the tick's start (`seen`), or `to` outranks `from`.
fn returns_with_reason(
    world: &SimWorld,
    (unit, entity, arch): (EntityId, &Entity, &UnitArchetype),
    (from, to): (EntityId, EntityId),
    seen: &BTreeMap<EntityId, Seen>,
) -> bool {
    let rank = |id: EntityId| {
        let candidate = world.entities.get(&id)?;
        let candidate_arch = world.catalog.get(candidate.unit_type_id).ok()?;
        Some((
            is_threat_to(candidate_arch, candidate, arch),
            candidate_arch.attack_target_priority,
            arch.primary_weapon_reaches(candidate_arch.mover_type),
        ))
    };
    if rank(to) > rank(from) {
        return true;
    }
    let (Some(chaser), Some(body), Some(target)) =
        (seen.get(&unit), seen.get(&to), world.entities.get(&to))
    else {
        return false;
    };
    let Some(band) = band_against(world, entity, target) else {
        return false;
    };
    let others = ring_bodies(seen, chaser.class, &[unit, to]);
    let query = RingQuery {
        asker: (chaser.at, chaser.radius),
        centre: body.at,
        band,
        leash: None,
    };
    classify_room(query, &others, |p| {
        walkable(world, chaser.layer, body.at, p)
    })
    .is_some()
}

/// The tick-start state the meter keeps of `entity` (archetype `arch`, collision class `class`).
fn seen(
    world: &SimWorld,
    (entity, arch): (&Entity, &UnitArchetype),
    class: CollisionClass,
) -> Seen {
    let target = attack_target(entity);
    let standing = standing_of(world, entity, arch);
    let able = is_ready(entity)
        && side_of(entity.owner).is_some()
        && world.entities.values().any(|enemy| {
            enemy.is_alive()
                && enemy.owner != entity.owner
                && side_of(enemy.owner).is_some()
                && !enemy.is_hidden()
                && !enemy.is_cloaked
                && in_band(world, entity, arch, enemy, SimScalar::ZERO)
        });
    Seen {
        owner: entity.owner,
        position: entity.position,
        at: point(entity.position),
        radius: to_f64(arch.collision_radius),
        class,
        layer: MovementLayer::from_mover(arch.mover_type),
        body: !entity.passes_through_units() && !arch.is_immobile_by_default(),
        engaged: standing.engaged,
        proposed_none: standing.proposed_none,
        holding: standing.holding,
        held: standing.held,
        speed: to_f64(arch.max_speed),
        target,
        cooldown: entity.weapon_cooldown_remaining,
        heading: entity.heading,
        able,
    }
}

/// Whether `entity`'s weapon is `Ready` at cooldown 0 (B8).
fn is_ready(entity: &Entity) -> bool {
    matches!(entity.weapon_phase, WeaponPhase::Ready) && entity.weapon_cooldown_remaining == 0
}

/// Whether `entity` is [`Activity::Blocked`] after the tick, as the battle trace classifies it.
fn is_blocked(world: &SimWorld, entity: &Entity, arch: &UnitArchetype) -> bool {
    let Some(target) = attack_target(entity)
        .and_then(|id| world.entities.get(&id))
        .filter(|enemy| enemy.is_alive() && enemy.owner != entity.owner)
    else {
        return false;
    };
    if in_band(world, entity, arch, target, SimScalar::ZERO) {
        return false;
    }
    let enemy_in_band = world.entities.values().any(|other| {
        other.owner != entity.owner
            && other.is_alive()
            && in_band(world, entity, arch, other, SimScalar::ZERO)
    });
    let offset = target.position - entity.position;
    classify(&ActivityInput {
        enemy_in_band,
        to_target: Some((to_f64(offset.x), to_f64(offset.y))),
        step: (to_f64(entity.velocity.x), to_f64(entity.velocity.y)),
        max_speed: to_f64(arch.max_speed),
    }) == Activity::Blocked
}

/// The leash of `entity`'s automatic chase: its origin (the tick-start position when the memory
/// holds none) and the catalog's acquire leash radius.
fn leash_of(world: &SimWorld, entity: &Entity, before: &Seen) -> (Point, f64) {
    let origin = entity.movement.leash_origin.map_or(before.at, point);
    let radius = to_f64(world.catalog.movement_rules().acquire_leash_radius());
    (origin, radius)
}

/// The band's outer edge (centre to centre, world units) at which `entity` reaches `enemy`:
/// weapon range plus both radii; `None` when it has no weapon against it.
fn band_against(world: &SimWorld, entity: &Entity, enemy: &Entity) -> Option<f64> {
    let arch = world.catalog.get(entity.unit_type_id).ok()?;
    let enemy_arch = world.catalog.get(enemy.unit_type_id).ok()?;
    let weapon = arch.weapon_against(enemy_arch.mover_type, entity.active_siege_weapon())?;
    Some(to_f64(
        weapon.max_range + arch.collision_radius + enemy_arch.collision_radius,
    ))
}

/// Whether a chaser of `layer` can stand at ring point `p` of a ring centred at `centre`: an air
/// body anywhere on the map; a ground body where the ground from the centre to `p` is
/// connected.
fn walkable(world: &SimWorld, layer: MovementLayer, centre: Point, p: Point) -> bool {
    let (Some(from), Some(to)) = (vector(centre), vector(p)) else {
        return false;
    };
    if world.map.world_to_cell(to).is_err() {
        return false;
    }
    layer == MovementLayer::Air || world.map.ground_segment_is_connected(from, to, |_, _| true)
}

/// The side index of `owner`: player 0 and player 1; `None` for any other.
fn side_of(owner: PlayerId) -> Option<usize> {
    (owner.0 < 2).then_some(usize::from(owner.0))
}

/// A fixed-point position as an `f64` point.
fn point(position: SimVec2) -> Point {
    (to_f64(position.x), to_f64(position.y))
}

/// An `f64` point as a fixed-point vector, `None` when it is not representable.
fn vector(p: Point) -> Option<SimVec2> {
    let scalar =
        |value: f64| (value.is_finite() && value.abs() < 1.0e6).then(|| SimScalar::from_num(value));
    Some(SimVec2::new(scalar(p.0)?, scalar(p.1)?))
}

/// The distance between two points.
fn distance(a: Point, b: Point) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W0-T1 (plan `2026-11-24-solver-retarget-and-shoving`): a standing engaged unit displaced
    /// by 0.1 with an enemy in contact counts 0.1; a walking unit, a holder, and a unit without
    /// enemy contact count 0.
    #[test]
    fn shove_counts_only_contact_displacement_of_units_that_propose_none() {
        assert!((shove_of(true, false, true, 0.1) - 0.1).abs() < 1e-12);
        assert!(shove_of(false, false, true, 0.1).abs() < 1e-12);
        assert!(shove_of(true, true, true, 0.1).abs() < 1e-12);
        assert!(shove_of(true, false, false, 0.1).abs() < 1e-12);
    }

    /// W0-T1′ (evidence `2026-11-25-solver-pair-cache` §2): a standing unit displaced
    /// 0.1 straight away from a touching enemy counts 0.1; a unit nudged 0.1 along its ring
    /// round the touching enemy counts at most `0.1 · 0.1 / (r + r_e)` (second order); a walking
    /// unit, a holder and a unit pushed towards the enemy count 0; the largest enemy counts.
    #[test]
    fn shove_e_counts_only_the_outward_part_from_touching_enemies() {
        let away = [(1.0, 0.0)];
        assert!((shove_e_of(true, false, (0.1, 0.0), away) - 0.1).abs() < 1e-12);
        // Along the ring of radius R = r + r_e round the enemy, an arc of 0.1.
        let ring: f64 = 0.375 + 0.75;
        let angle = 0.1 / ring;
        let nudge = (ring * angle.cos() - ring, ring * angle.sin());
        let counted = shove_e_of(true, false, nudge, [(ring, 0.0)]);
        assert!(counted >= 0.0 && counted <= 0.1 * 0.1 / ring, "{counted}");
        assert!(shove_e_of(false, false, (0.1, 0.0), away).abs() < 1e-12);
        assert!(shove_e_of(true, true, (0.1, 0.0), away).abs() < 1e-12);
        assert!(shove_e_of(true, false, (-0.1, 0.0), away).abs() < 1e-12);
        assert!(shove_e_of(true, false, (0.1, 0.0), []).abs() < 1e-12);
        let two = [(1.0, 0.0), (0.0, 1.0)];
        assert!((shove_e_of(true, false, (0.1, 0.05), two) - 0.1).abs() < 1e-12);
        assert!((shove_e_of(true, false, (0.03, 0.04), [(0.0, 0.0)]) - 0.05).abs() < 1e-12);
    }

    /// B8 (plan `2026-11-26-friend-crowd-overlap` WP0): a unit able at the tick's start that
    /// stays still and ready counts; one that moved, turned or swung does not; nor does one that
    /// was not able.
    #[test]
    fn standing_while_able_needs_all_three() {
        assert!(stands_while_able(true, true, true));
        assert!(!stands_while_able(true, false, true));
        assert!(!stands_while_able(true, true, false));
        assert!(!stands_while_able(false, true, true));
    }

    /// W0-T3: a return to the previous target within the window is reported with its reason; a
    /// return after the window, a switch to a third target, and a first switch are not returns.
    #[test]
    fn churn_separates_explained_returns() {
        let (unit, a, b, c) = (EntityId(1), EntityId(10), EntityId(11), EntityId(12));
        let mut churn = ChurnTracker::default();
        assert_eq!(churn.switch(unit, 100, (a, b), || true), None);
        assert_eq!(churn.switch(unit, 110, (b, a), || true), Some(true));
        assert_eq!(churn.switch(unit, 120, (a, b), || false), Some(false));
        assert_eq!(churn.switch(unit, 130, (b, c), || true), None);
        assert_eq!(churn.switch(unit, 200, (c, b), || true), None);
        let late = 200 + CHURN_WINDOW_TICKS + 1;
        assert_eq!(churn.switch(unit, late, (b, c), || true), None);
        // Another unit's switches are its own.
        assert_eq!(churn.switch(EntityId(2), 300, (b, c), || true), None);
    }
}
