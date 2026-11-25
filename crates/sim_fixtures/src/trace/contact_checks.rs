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
//!   `moving_overlap_permille`).
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
//! the asker's class (ground, air or burrowed). The leash is the unit's `leash_origin` (its
//! tick-start position when unset) and the catalog's acquire leash radius; terrain is a ring
//! point whose cell is not connected ground to the ring's centre (air bodies have none). Neither
//! the ring walk nor vision is modelled. Everything is `f64`, for measurement only.
//!
//! # Invariants
//! - The meter only reads the world; observing a battle does not change it.
//! - A unit's first observed tick counts nothing but its unit-tick: it has no tick-start state.
//! - Counts are per side and never decrease; the enemy overlap is a running maximum.

use super::activity::{classify, Activity, ActivityInput};
use super::{attack_target, in_band, to_f64};
use crate::harness::metrics::Point;
use crate::harness::room::{classify_room, RingQuery, RoomClass};
use sim_core::movement::MovementLayer;
use sim_core::{is_threat_to, Entity, SimWorld, UnitArchetype};
use sim_types::{AttackOrigin, EntityId, PlayerId, SimScalar, SimVec2, UnitOrder};
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
    class: (MovementLayer, bool),
    /// A body for the overlaps and for touching (module documentation).
    body: bool,
    engaged: bool,
    proposed_none: bool,
    holding: bool,
    target: Option<EntityId>,
    cooldown: u16,
}

/// The meter of the module documentation.
#[derive(Debug, Clone, Default)]
pub struct ContactMeter {
    tick: u32,
    seen: BTreeMap<EntityId, Seen>,
    churn: ChurnTracker,
    checks: ContactChecks,
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
        self.checks
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
            now.insert(id, seen(world, entity, arch));
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
        let moving = f64::from(
            world
                .catalog
                .movement_rules()
                .contact()
                .moving_overlap_permille,
        ) / 1000.0;
        self.count_overlaps(&now, moving);
        self.seen = now;
        self.tick += 1;
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
        checks.shove_e += shove_e_of(before.proposed_none, before.holding, delta, away);
        checks.shots += u32::from(entity.weapon_cooldown_remaining > before.cooldown);
        checks.blocked += u32::from(is_blocked(world, entity, arch));
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
                    && body.class == before.class
                    && distance(body.at, before.at)
                        <= body.radius + before.radius + SHOVE_TOUCH_SLACK
            })
            .map(|(_, body)| (before.at.0 - body.at.0, before.at.1 - body.at.1))
            .collect()
    }

    /// Adds the end-of-tick overlaps of `bodies` to B3's and B6's counters; `moving` is D-α's
    /// moving bound as a fraction of the radius sum.
    fn count_overlaps(&mut self, bodies: &BTreeMap<EntityId, Seen>, moving: f64) {
        let list: Vec<&Seen> = bodies.values().filter(|b| b.body).collect();
        let checks = &mut self.checks;
        for (index, a) in list.iter().enumerate() {
            for b in &list[index + 1..] {
                if a.class != b.class {
                    continue;
                }
                let sum = a.radius + b.radius;
                let overlap = (sum - distance(a.at, b.at)) / sum;
                if a.owner == b.owner {
                    checks.friend_overlap = checks.friend_overlap.max(overlap);
                    checks.friend_overlap_ticks += u32::from(overlap > moving);
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
            walkable(world, chaser.class, body.at, p)
        })
    }
}

/// Every body of `class` but `excluded`: centre and radius.
fn ring_bodies(
    seen: &BTreeMap<EntityId, Seen>,
    class: (MovementLayer, bool),
    excluded: &[EntityId],
) -> Vec<(Point, f64)> {
    seen.iter()
        .filter(|(id, body)| body.class == class && !excluded.contains(id))
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
        walkable(world, chaser.class, body.at, p)
    })
    .is_some()
}

/// The tick-start state the meter keeps of `entity`.
fn seen(world: &SimWorld, entity: &Entity, arch: &UnitArchetype) -> Seen {
    let target = attack_target(entity);
    let engaged = target
        .and_then(|id| world.entities.get(&id))
        .filter(|enemy| enemy.is_alive())
        .is_some_and(|enemy| in_band(world, entity, arch, enemy, SimScalar::ZERO));
    let standing_order = matches!(
        entity.order_queue.front(),
        None | Some(UnitOrder::Idle | UnitOrder::Stop)
    );
    Seen {
        owner: entity.owner,
        position: entity.position,
        at: point(entity.position),
        radius: to_f64(arch.collision_radius),
        class: (
            MovementLayer::from_mover(arch.mover_type),
            entity.is_burrowed(arch),
        ),
        body: !entity.passes_through_units() && !arch.is_immobile_by_default(),
        engaged,
        proposed_none: engaged || standing_order,
        holding: entity.is_on_hold() || entity.cannot_be_pushed(),
        target,
        cooldown: entity.weapon_cooldown_remaining,
    }
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

/// Whether a chaser of `class` can stand at ring point `p` of a ring centred at `centre`: an air
/// body anywhere on the map; a ground body where the ground from the centre to `p` is
/// connected.
fn walkable(world: &SimWorld, class: (MovementLayer, bool), centre: Point, p: Point) -> bool {
    let (Some(from), Some(to)) = (vector(centre), vector(p)) else {
        return false;
    };
    if world.map.world_to_cell(to).is_err() {
        return false;
    }
    class.0 == MovementLayer::Air || world.map.ground_segment_is_connected(from, to, |_, _| true)
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
