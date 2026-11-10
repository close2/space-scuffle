//! Blocking metrics: how often attackers are stuck behind their own side, and how often units
//! flip direction from one tick to the next.
//!
//! # Responsibility
//! [`BlockingMeter`] watches a [`SimWorld`] after every tick and counts, per owner:
//! - **blocked attacker ticks** ([`BlockingMetrics::blocked_attacker_ticks`]): ticks a unit holds a
//!   living target, is outside its weapon's band against it, and has a friendly body that will not
//!   step aside within [`BLOCKER_REACH`] of its edge, on the side toward the target;
//! - **reversals** ([`BlockingMetrics::reversals`]): ticks whose achieved velocity points against
//!   the previous tick's (cosine below [`REVERSAL_COSINE`], both steps longer than
//!   [`REVERSAL_MIN_STEP`]) while the unit's order, target and goal are unchanged (a chased target
//!   that moved changes the goal) and no enemy touches the unit. This is invariant D-1.4 of plan
//!   `2026-11-09-friendly-blocking`, which the waiting rule makes hold;
//! - **stall give-ups** ([`BlockingMetrics::stall_giveups`]): the `NoProgress` and
//!   `PursuitAbandoned` route outcomes, orders given up because the unit stopped gaining ground.
//!
//! [`attackers_in_range`] counts, for one owner, the armed mobile non-harvesters and how many of
//! them have a living target inside their weapon's band: the showcase acceptance of that plan (§7).
//!
//! # How it fits
//! The instrument of plan `2026-11-09-friendly-blocking` (WP1). The whole-matrix mode
//! ([`super::matrix`]) runs a meter on every battle and writes its totals as extra `matrix.csv`
//! columns; the route trace ([`super::route`]) reports it per owner for a scenario such as the
//! showcase; and `crates/sim_core/tests/second_line_tests.rs` asserts on it. A friend "will not
//! step aside" unless it is idle (no order, or `Idle`), standing still, and pushable: the push table
//! of plan D-4, read from outside the simulation. Positions are converted to `f64` for
//! measurement only; nothing flows back into the simulation.
//!
//! # Invariants
//! - The meter only reads the world; observing a battle does not change it.
//! - A unit's first observed tick never counts a reversal: it has no previous step.
//! - Counts are per owner slot and never decrease.

use super::{attack_target, in_band, to_f64};
use sim_core::movement::RouteOutcomeKind;
use sim_core::{Entity, SimWorld, UnitArchetype};
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2, UnitOrder};
use std::collections::BTreeMap;

/// Gap between body edges, world units, within which a friend counts as blocking an attacker:
/// the plan's "within its radius plus 0.1".
pub const BLOCKER_REACH: f64 = 0.1;

/// Cosine between consecutive steps below which the second step counts as a reversal (D-1.4).
pub const REVERSAL_COSINE: f64 = -0.3;

/// Steps no longer than this, world units per tick, are standing still and never reverse.
pub const REVERSAL_MIN_STEP: f64 = 0.005;

/// Gap between body edges, world units, within which an enemy counts as touching a unit.
pub const ENEMY_TOUCH_GAP: f64 = 0.05;

/// One owner's blocking counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlockingMetrics {
    /// Unit-ticks spent out of range of a held target with a non-yielding friend in the way.
    pub blocked_attacker_ticks: u32,
    /// Unit-ticks whose step reversed the previous one under an unchanged order (D-1.4).
    pub reversals: u32,
    /// Orders given up for lack of progress (`NoProgress`, `PursuitAbandoned`).
    pub stall_giveups: u32,
}

impl BlockingMetrics {
    /// The sum of `self` and `other`, counter by counter, saturating.
    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self {
            blocked_attacker_ticks: self
                .blocked_attacker_ticks
                .saturating_add(other.blocked_attacker_ticks),
            reversals: self.reversals.saturating_add(other.reversals),
            stall_giveups: self.stall_giveups.saturating_add(other.stall_giveups),
        }
    }
}

/// What a unit was asked to do on one tick: if this changes, a change of direction is expected.
#[derive(Debug, Clone, PartialEq)]
struct Intent {
    order: Option<UnitOrder>,
    target: Option<EntityId>,
    goal: Option<SimVec2>,
}

impl Intent {
    fn of(entity: &Entity) -> Self {
        Self {
            order: entity.order_queue.front().cloned(),
            target: entity.current_target,
            goal: entity.task.goal,
        }
    }
}

/// A unit's last observed step and intent.
#[derive(Debug, Clone)]
struct LastStep {
    velocity: (f64, f64),
    intent: Intent,
}

/// Counts the blocking metrics of a battle, one [`Self::observe`] per tick.
#[derive(Debug, Clone, Default)]
pub struct BlockingMeter {
    /// Each mobile unit's last observed step.
    last: BTreeMap<EntityId, LastStep>,
    /// Counters per owner slot.
    owners: BTreeMap<u8, BlockingMetrics>,
    /// Reversals per unit.
    unit_reversals: BTreeMap<EntityId, u32>,
}

impl BlockingMeter {
    /// A meter that has seen nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts the tick `world` has just stepped. Call once after every `SimWorld::step`.
    pub fn observe(&mut self, world: &SimWorld) {
        for outcome in world.route_outcomes() {
            if matches!(
                outcome.kind,
                RouteOutcomeKind::NoProgress | RouteOutcomeKind::PursuitAbandoned { .. }
            ) {
                if let Some(entity) = world.entities.get(&outcome.entity) {
                    self.owner(entity.owner).stall_giveups += 1;
                }
            }
        }
        let mut seen = BTreeMap::new();
        for (&id, entity) in &world.entities {
            if !entity.is_alive() || entity.is_immobile || entity.is_loaded_in_transport() {
                continue;
            }
            let Ok(arch) = world.catalog.get(entity.unit_type_id) else {
                continue;
            };
            let velocity = (to_f64(entity.velocity.x), to_f64(entity.velocity.y));
            let intent = Intent::of(entity);
            if let Some(last) = self.last.get(&id) {
                if last.intent == intent
                    && is_reversal(last.velocity, velocity)
                    && !excused(world, entity, arch)
                {
                    self.owner(entity.owner).reversals += 1;
                    *self.unit_reversals.entry(id).or_default() += 1;
                }
            }
            if is_blocked_attacker(world, entity, arch) {
                self.owner(entity.owner).blocked_attacker_ticks += 1;
            }
            seen.insert(id, LastStep { velocity, intent });
        }
        self.last = seen;
    }

    /// The counters of `owner`; zero for an owner never seen.
    #[must_use]
    pub fn of_owner(&self, owner: PlayerId) -> BlockingMetrics {
        self.owners.get(&owner.0).copied().unwrap_or_default()
    }

    /// The counters of every owner, summed.
    #[must_use]
    pub fn total(&self) -> BlockingMetrics {
        self.owners
            .values()
            .fold(BlockingMetrics::default(), |sum, owner| sum.plus(*owner))
    }

    /// Reversals counted for the unit `id`.
    #[must_use]
    pub fn reversals_of(&self, id: EntityId) -> u32 {
        self.unit_reversals.get(&id).copied().unwrap_or(0)
    }

    fn owner(&mut self, owner: PlayerId) -> &mut BlockingMetrics {
        self.owners.entry(owner.0).or_default()
    }
}

/// Whether the step `now` reverses the step `before` (both world units per tick).
#[must_use]
pub fn is_reversal(before: (f64, f64), now: (f64, f64)) -> bool {
    let (a, b) = (before.0.hypot(before.1), now.0.hypot(now.1));
    if a <= REVERSAL_MIN_STEP || b <= REVERSAL_MIN_STEP {
        return false;
    }
    (before.0 * now.0 + before.1 * now.1) / (a * b) < REVERSAL_COSINE
}

/// The gap between the edges of two bodies, world units; negative when they overlap.
fn edge_gap(world: &SimWorld, a: &Entity, a_arch: &UnitArchetype, b: &Entity) -> Option<f64> {
    let b_arch = world.catalog.get(b.unit_type_id).ok()?;
    let centres = to_f64(a.position.distance(b.position));
    Some(centres - to_f64(a_arch.collision_radius) - to_f64(b_arch.collision_radius))
}

/// Whether a living enemy touches `entity`, or the target it attacks moved this tick: either
/// makes a change of direction expected, so D-1.4 does not count it ("no enemy touches it", "goal
/// unchanged": a chase's goal is its target's centre).
fn excused(world: &SimWorld, entity: &Entity, arch: &UnitArchetype) -> bool {
    let target_moved = attack_target(entity)
        .and_then(|id| world.entities.get(&id))
        .is_some_and(|target| target.velocity != SimVec2::ZERO);
    target_moved
        || world.entities.values().any(|other| {
            other.is_alive()
                && other.owner != entity.owner
                && !other.is_loaded_in_transport()
                && edge_gap(world, entity, arch, other).is_some_and(|gap| gap <= ENEMY_TOUCH_GAP)
        })
}

/// Whether `friend` steps aside for a unit walking into it: idle, standing still and pushable.
fn yields(friend: &Entity) -> bool {
    matches!(friend.order_queue.front(), None | Some(UnitOrder::Idle))
        && friend.velocity == SimVec2::ZERO
        && !friend.cannot_be_pushed()
}

/// Whether `entity` holds a living target outside its weapon's band, with a friendly body that
/// will not yield within [`BLOCKER_REACH`] of its edge on the side toward the target.
fn is_blocked_attacker(world: &SimWorld, entity: &Entity, arch: &UnitArchetype) -> bool {
    let Some(target) = attack_target(entity)
        .and_then(|id| world.entities.get(&id))
        .filter(|target| target.is_alive())
    else {
        return false;
    };
    if in_band(world, entity, arch, target, SimScalar::ZERO) {
        return false;
    }
    let toward = target.position - entity.position;
    world.entities.values().any(|friend| {
        friend.id != entity.id
            && friend.owner == entity.owner
            && friend.is_alive()
            && !friend.is_loaded_in_transport()
            && !yields(friend)
            && {
                let offset = friend.position - entity.position;
                to_f64(offset.x) * to_f64(toward.x) + to_f64(offset.y) * to_f64(toward.y) > 0.0
            }
            && edge_gap(world, entity, arch, friend).is_some_and(|gap| gap <= BLOCKER_REACH)
    })
}

/// Armed mobile units of `owner` that are not harvesters, and how many of them have a living
/// target inside their weapon's band: `(in_range, attackers)`.
#[must_use]
pub fn attackers_in_range(world: &SimWorld, owner: PlayerId) -> (u32, u32) {
    let mut counts = (0, 0);
    for entity in world.entities.values() {
        if entity.owner != owner || !entity.is_alive() || entity.is_immobile {
            continue;
        }
        let Ok(arch) = world.catalog.get(entity.unit_type_id) else {
            continue;
        };
        let armed =
            arch.weapon.is_some() || arch.ground_weapon.is_some() || arch.air_weapon.is_some();
        if !armed || arch.capabilities.harvester.is_some() {
            continue;
        }
        counts.1 += 1;
        let in_range = attack_target(entity)
            .and_then(|id| world.entities.get(&id))
            .filter(|target| target.is_alive())
            .is_some_and(|target| in_band(world, entity, arch, target, SimScalar::ZERO));
        counts.0 += u32::from(in_range);
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_steps_reverse_and_small_or_sideways_ones_do_not() {
        assert!(is_reversal((0.02, 0.0), (-0.02, 0.001)));
        assert!(is_reversal((0.0192, -0.0215), (-0.0192, 0.0215)));
        // A quarter turn is a turn, not a reversal.
        assert!(!is_reversal((0.02, 0.0), (0.0, 0.02)));
        // Standing still on either tick is never a reversal.
        assert!(!is_reversal((0.004, 0.0), (-0.02, 0.0)));
        assert!(!is_reversal((0.02, 0.0), (0.0, 0.0)));
        // The threshold sits at a cosine of -0.3, about 107 degrees.
        assert!(!is_reversal((1.0, 0.0), (-0.29, 0.957)));
        assert!(is_reversal((1.0, 0.0), (-0.31, 0.951)));
    }

    #[test]
    fn metrics_add_counter_by_counter_and_saturate() {
        let a = BlockingMetrics {
            blocked_attacker_ticks: 1,
            reversals: 2,
            stall_giveups: 3,
        };
        let b = BlockingMetrics {
            blocked_attacker_ticks: u32::MAX,
            reversals: 5,
            stall_giveups: 0,
        };
        assert_eq!(
            a.plus(b),
            BlockingMetrics {
                blocked_attacker_ticks: u32::MAX,
                reversals: 7,
                stall_giveups: 3,
            }
        );
    }
}
