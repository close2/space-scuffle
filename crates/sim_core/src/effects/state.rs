//! The hashed state of the effect engine (plan `2026-11-10-spellcaster-mechanics` D-13, §5.3).
//!
//! # Responsibility
//! Declares the live instances effect trees leave behind: statuses on units, persistents,
//! missiles, and the per-entity displacement, plus the world-level table that holds them.
//!
//! # How it fits
//! `SimWorld::effects` owns an [`EffectWorld`]; `Entity::displacement` holds a
//! [`Displacement`]. The checksum encodes every field here (`checksum/encoding.rs`). Specs and
//! the effect arena are catalog data and are referred to by [`EffectId`] and [`StatusId`].
//!
//! # Invariants
//! - `statuses` is sorted by `(entity, status)` and holds at most one entry per pair (D-5).
//! - `persistents` and `missiles` are sorted by id; ids come from the `next_*` counters and
//!   are never reused within a world.
//! - Every value is fixed-point or integer; no float enters (C7).

use sim_types::{
    EffectContext, EffectId, EffectTarget, EntityId, SimScalar, SimVec2, StatusId, Tick,
};

/// One status on one unit (D-3 to D-5).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ActiveStatus {
    /// The bearer.
    pub entity: EntityId,
    /// Which status.
    pub status: StatusId,
    /// The tick of the first application; pulses count from it.
    pub applied_tick: Tick,
    /// Ticks after `applied_tick` at which the status ends: refreshed to the elapsed ticks plus
    /// the status's duration on each re-application.
    pub expires_at: SimScalar,
    /// Periodic pulses already run.
    pub pulses_done: u16,
    /// The context of the first application, with the bearer as target (D-5, D-15).
    pub context: EffectContext,
}

/// Where a persistent sits.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PersistentAnchor {
    /// A fixed point in world units.
    Point(SimVec2),
    /// A unit, followed while it lives; the persistent ends when it dies, loads or is gone.
    Unit(EntityId),
}

/// A live `Persistent` node instance (D-16).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ActivePersistent {
    /// Identity, from `EffectWorld::next_persistent_id`.
    pub id: u32,
    /// The `Persistent` node it runs.
    pub node: EffectId,
    /// The context its pulses run with (before the anchor is applied).
    pub context: EffectContext,
    /// Where it sits.
    pub anchor: PersistentAnchor,
    /// The tick it was created; pulse k is due `round_half_up(k × period)` ticks after.
    pub created_tick: Tick,
    /// Pulses already run.
    pub pulses_done: u16,
}

/// A live `LaunchMissile` node instance (D-16).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ActiveMissile {
    /// Identity, from `EffectWorld::next_missile_id`.
    pub id: u32,
    /// The `LaunchMissile` node it runs.
    pub node: EffectId,
    /// The context its impact runs with.
    pub context: EffectContext,
    /// Current position in world units.
    pub position: SimVec2,
    /// The point it flies to, or the unit it homes on.
    pub target: EffectTarget,
}

/// A unit flown by a `Displace` node (D-8): its position is a pure function of the ticks since
/// `start_tick`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Displacement {
    /// The `Displace` node, whose `on_land` runs on landing.
    pub node: EffectId,
    /// The context `on_land` runs with.
    pub context: EffectContext,
    /// Where the flight started.
    pub from: SimVec2,
    /// Where it lands.
    pub to: SimVec2,
    /// The tick the flight started; the unit first moves on the next tick.
    pub start_tick: Tick,
}

/// The engine's hashed world-level state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectWorld {
    /// Every status, sorted by `(entity, status)`.
    pub(crate) statuses: Vec<ActiveStatus>,
    /// Live persistents in id order.
    pub(crate) persistents: Vec<ActivePersistent>,
    /// Live missiles in id order.
    pub(crate) missiles: Vec<ActiveMissile>,
    /// The id of the next persistent.
    pub(crate) next_persistent_id: u32,
    /// The id of the next missile.
    pub(crate) next_missile_id: u32,
}

/// Instances reserved when a world is created: two storms, a missile and a few delays need far
/// fewer, so a match grows these only in extreme cases (D-18).
const RESERVED_INSTANCES: usize = 64;
/// Statuses reserved when a world is created: a storm on a clump of 40 plus a few other spells.
const RESERVED_STATUSES: usize = 128;

impl Default for EffectWorld {
    fn default() -> Self {
        Self {
            statuses: Vec::with_capacity(RESERVED_STATUSES),
            persistents: Vec::with_capacity(RESERVED_INSTANCES),
            missiles: Vec::with_capacity(RESERVED_INSTANCES),
            next_persistent_id: 1,
            next_missile_id: 1,
        }
    }
}

impl EffectWorld {
    /// Every status, sorted by `(entity, status)`.
    #[must_use]
    pub fn statuses(&self) -> &[ActiveStatus] {
        &self.statuses
    }

    /// Live persistents in id order.
    #[must_use]
    pub fn persistents(&self) -> &[ActivePersistent] {
        &self.persistents
    }

    /// Live missiles in id order.
    #[must_use]
    pub fn missiles(&self) -> &[ActiveMissile] {
        &self.missiles
    }

    /// The statuses `entity` carries, in status order.
    #[must_use]
    pub fn statuses_of(&self, entity: EntityId) -> &[ActiveStatus] {
        let start = self.statuses.partition_point(|s| s.entity < entity);
        let end = self.statuses.partition_point(|s| s.entity <= entity);
        &self.statuses[start..end]
    }

    /// Whether `entity` carries `status`.
    #[must_use]
    pub fn has_status(&self, entity: EntityId, status: StatusId) -> bool {
        self.find_status(entity, status).is_ok()
    }

    /// The position of `(entity, status)` in the sorted table, or where it would be inserted.
    pub(crate) fn find_status(&self, entity: EntityId, status: StatusId) -> Result<usize, usize> {
        self.statuses
            .binary_search_by(|s| (s.entity, s.status).cmp(&(entity, status)))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::test_support::world;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{AbilitySource, PlayerId, UnitTypeId};

    fn context() -> EffectContext {
        EffectContext {
            owner: PlayerId(0),
            caster: Some(EntityId(1)),
            ability: AbilitySource::default(),
            origin: SimVec2::from_i32(1, 1),
            source_point: SimVec2::from_i32(2, 2),
            target: EffectTarget::Point(SimVec2::from_i32(3, 3)),
        }
    }

    type Edit = fn(&mut EffectWorld);

    /// Every field of the effect state, and of the context it stores, moves the world checksum
    /// (plan D-13): each edit below changes exactly one field of a populated state.
    #[test]
    fn every_effect_state_field_changes_the_world_checksum() {
        let map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a small flat map");
        let mut world = world(map, 7);
        let empty = world.compute_checksum();
        world.effects.statuses.push(ActiveStatus {
            entity: EntityId(1),
            status: StatusId(0),
            applied_tick: Tick(1),
            expires_at: SimScalar::ONE,
            pulses_done: 0,
            context: context(),
        });
        world.effects.persistents.push(ActivePersistent {
            id: 1,
            node: EffectId(0),
            context: context(),
            anchor: PersistentAnchor::Point(SimVec2::ZERO),
            created_tick: Tick(1),
            pulses_done: 0,
        });
        world.effects.missiles.push(ActiveMissile {
            id: 1,
            node: EffectId(0),
            context: context(),
            position: SimVec2::ZERO,
            target: EffectTarget::Unit(EntityId(3)),
        });
        let populated = world.compute_checksum();
        assert_ne!(empty, populated);
        let edits: [(&str, Edit); 22] = [
            ("next persistent id", |e| e.next_persistent_id += 1),
            ("next missile id", |e| e.next_missile_id += 1),
            ("status entity", |e| e.statuses[0].entity = EntityId(2)),
            ("status id", |e| e.statuses[0].status = StatusId(1)),
            ("applied tick", |e| e.statuses[0].applied_tick = Tick(2)),
            ("expires at", |e| e.statuses[0].expires_at = SimScalar::HALF),
            ("status pulses", |e| e.statuses[0].pulses_done = 1),
            ("context owner", |e| {
                e.statuses[0].context.owner = PlayerId(1);
            }),
            ("context caster", |e| e.statuses[0].context.caster = None),
            ("context ability", |e| {
                e.statuses[0].context.ability.unit_type = UnitTypeId(4);
            }),
            ("context origin", |e| {
                e.statuses[0].context.origin = SimVec2::ZERO;
            }),
            ("context source", |e| {
                e.statuses[0].context.source_point = SimVec2::ZERO;
            }),
            ("context target kind", |e| {
                e.statuses[0].context.target = EffectTarget::Unit(EntityId(3));
            }),
            ("persistent id", |e| e.persistents[0].id = 2),
            ("persistent node", |e| e.persistents[0].node = EffectId(1)),
            ("persistent anchor", |e| {
                e.persistents[0].anchor = PersistentAnchor::Unit(EntityId(0));
            }),
            ("persistent created", |e| {
                e.persistents[0].created_tick = Tick(3);
            }),
            ("persistent pulses", |e| e.persistents[0].pulses_done = 2),
            ("missile id", |e| e.missiles[0].id = 2),
            ("missile node", |e| e.missiles[0].node = EffectId(2)),
            ("missile position", |e| {
                e.missiles[0].position = SimVec2::from_i32(1, 0);
            }),
            ("missile target", |e| {
                e.missiles[0].target = EffectTarget::Point(SimVec2::ZERO);
            }),
        ];
        for (name, edit) in edits {
            let saved = world.effects.clone();
            edit(&mut world.effects);
            assert_ne!(world.compute_checksum(), populated, "{name}");
            world.effects = saved;
            assert_eq!(world.compute_checksum(), populated, "{name} restored");
        }
    }
}
