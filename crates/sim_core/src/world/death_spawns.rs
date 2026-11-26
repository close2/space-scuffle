//! Death spawns: the units a dying unit leaves after a delay (plan
//! `2026-12-01-death-spawns-and-hangar-commands` D-2, D-3; `StarCraft` II's `DeathResponse` that
//! runs a `CEffectCreatePersistent` whose `FinalEffect` is a `CEffectCreateUnit`, the Zerg
//! structures' Broodlings).
//!
//! # Responsibility
//! Owns the world's pending death spawns: queues one when a unit whose archetype has a
//! [`sim_types::DeathSpawnSpec`] is removed by the reap, counts each down once per tick, and
//! creates its units when the delay runs out: each prepared for the dead unit's owner, placed by
//! the shared placement search, faced as the dead unit was, given its timed life and committed,
//! with no order.
//!
//! # How it fits
//! [`crate::transport`]'s reap calls [`SimWorld::queue_death_spawn`] for each member of a death
//! group it commits, just before the member leaves the entity map, so a group whose preparation
//! fails (and is retried the next tick) queues nothing twice. The upkeep phase calls
//! [`SimWorld::step_death_spawns`] after the per-entity upkeep, before the effect engine, unit
//! tasks and acquisition, so the created units are ordinary idle units the rest of the tick
//! sees. Creation goes through the production spawn transaction (`prepare_spawn` /
//! `commit_spawn`) and the shared helpers of [`super::spawns`]; nothing here edits movement.
//!
//! # Invariants
//! - A spawn is queued only for a unit the reap removes (it died, or it was a passenger of a
//!   carrier that died) and never for one under construction whose spec is disabled while
//!   constructing. A cancelled foundation is removed by the reap like a dead one (5.0.14's
//!   `Cancel` death is not the `Remove` that `SDeathResponse` leaves out), so only that
//!   construction rule keeps it from leaving units.
//! - A unit dying on tick D with a delay of `n ≥ 1` leaves its units on tick D + n; a delay of 0
//!   leaves them on tick D + 1 (the reap is the tick's last phase).
//! - Entries are queued in reap order (death groups by root id, members by id) and created in
//!   queue order, so entity ids and the timed-life draws are deterministic.
//! - The pending list is gameplay state that outlives a tick, so it is hashed (the checksum's
//!   `death-spawns` section). The host's spec is read from the catalog at creation, never copied.
//! - A spawn with no valid place within its search range creates nothing and reports
//!   [`SimulationDiagnostic::DeathSpawnBlocked`]; a refused spawn transaction reports
//!   [`SimulationDiagnostic::ProductionInvariant`]. Neither is silent.

use crate::weapon_timing::SimulationDiagnostic;
use crate::world::SimWorld;
use sim_types::{EntityId, PlayerId, SimAngle, SimVec2, Tick, UnitTypeId};

/// Pending death spawns the world reserves room for when it is created (D-2). A spawn is queued
/// per dying unit with a death spawn and lives for its delay (9 ticks for every shipped row), so
/// this covers that many such deaths within one delay before the list first grows; it then keeps
/// its high-water mark.
pub(crate) const RESERVED_PENDING_DEATH_SPAWNS: usize = 16;

/// A spawn waiting for its delay after its host died (plan
/// `2026-12-01-death-spawns-and-hangar-commands` D-2). Hashed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingDeathSpawn {
    /// Ticks left until the units are created; created on the upkeep that brings it to 0.
    pub due_in: u16,
    /// The dead host's archetype, whose `death_spawn` is read at creation.
    pub host: UnitTypeId,
    /// The dead host's owner, who owns the created units.
    pub owner: PlayerId,
    /// Where the host stood when it died, world units.
    pub point: SimVec2,
    /// The dead host's heading, a binary angle given to the created units (`StarCraft` II sets no
    /// facing: D-11 A-3).
    pub heading: SimAngle,
}

impl SimWorld {
    /// The death spawns waiting for their delay, in creation order (D-2).
    #[must_use]
    pub fn pending_death_spawns(&self) -> &[PendingDeathSpawn] {
        &self.pending_death_spawns
    }

    /// Queues the death spawn of `id`, about to be removed by the reap (D-3), if its archetype
    /// has one and it is not a foundation whose spec is disabled while constructing.
    pub(crate) fn queue_death_spawn(&mut self, id: EntityId) {
        let Some(host) = self.entities.get(&id) else {
            return;
        };
        let Some(spec) = self
            .catalog
            .get(host.unit_type_id)
            .ok()
            .and_then(|archetype| archetype.capabilities.death_spawn)
        else {
            return;
        };
        if spec.disabled_while_constructing && host.construction.is_some() {
            return;
        }
        self.pending_death_spawns.push(PendingDeathSpawn {
            due_in: spec.delay_ticks,
            host: host.unit_type_id,
            owner: host.owner,
            point: host.position,
            heading: host.heading,
        });
    }

    /// Counts every pending death spawn down by one tick and creates, in queue order, those that
    /// reach 0 (D-3), keeping the rest in order.
    // PERFORMANCE RATIONALE:
    // Why: The upkeep phase runs every tick and the list is empty almost always.
    // What: One emptiness check; otherwise the list is taken, drained in place (a stable
    //       retain by index that commits the due entries) and put back, keeping its capacity.
    // Trade-off: A hand-written retain instead of `Vec::retain`, whose closure could not borrow
    //            the world mutably to create the units.
    pub(crate) fn step_death_spawns(&mut self) {
        if self.pending_death_spawns.is_empty() {
            return;
        }
        let mut pending = std::mem::take(&mut self.pending_death_spawns);
        let mut kept = 0;
        for index in 0..pending.len() {
            let mut entry = pending[index];
            entry.due_in = entry.due_in.saturating_sub(1);
            if entry.due_in == 0 {
                self.create_death_spawn(entry);
            } else {
                pending[kept] = entry;
                kept += 1;
            }
        }
        pending.truncate(kept);
        self.pending_death_spawns = pending;
    }

    /// Creates the units of `entry` (D-3): `count` units of its host's spawn archetype, each
    /// placed by the shared search, faced as the host was and given its timed life, with no
    /// order. A host whose archetype no longer has a death spawn creates nothing.
    fn create_death_spawn(&mut self, entry: PendingDeathSpawn) {
        let Some(spec) = self
            .catalog
            .get(entry.host)
            .ok()
            .and_then(|archetype| archetype.capabilities.death_spawn)
        else {
            return;
        };
        let tick = Tick(self.current_tick);
        for _ in 0..spec.count {
            let mut prepared = match self.prepare_spawn(spec.unit, entry.owner, entry.point, 0) {
                Ok(prepared) => prepared,
                Err(source) => {
                    self.tick_diagnostics
                        .push(SimulationDiagnostic::ProductionInvariant { tick, source });
                    continue;
                }
            };
            let Some(place) = self.spawn_place(&prepared.entity, entry.point, spec.search_range)
            else {
                self.tick_diagnostics
                    .push(SimulationDiagnostic::DeathSpawnBlocked {
                        tick,
                        unit: spec.unit,
                        point: entry.point,
                    });
                continue;
            };
            let unit = &mut prepared.entity;
            unit.position = place;
            unit.pos_at_tick_start = place;
            unit.heading = entry.heading;
            unit.turret_angle = entry.heading;
            prepared.entity.timed_life = self.draw_timed_life(spec.timed_life);
            self.commit_spawn(prepared);
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    //! D-2, D-3: the countdown and drain order, and the checksum's reading of a pending entry
    //! (DS-12's first half).
    use super::PendingDeathSpawn;
    use crate::test_support::world;
    use crate::SimWorld;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{
        DeathSpawnSpec, MoverType, PlayerId, ShapeKind, SimAngle, SimScalar, SimVec2, UnitTypeId,
    };

    /// A flat world whose first structure type (by id) leaves `count` of the first mobile,
    /// supply-free-made ground unit after `delay` ticks, with no clock: its host and spawn types.
    fn hosted(count: u8, delay: u16) -> (SimWorld, UnitTypeId, UnitTypeId) {
        let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0))).expect("map");
        let mut world = world(map, 5);
        let mut ground: Vec<_> = world
            .catalog
            .archetypes()
            .filter(|a| a.mover_type == MoverType::Ground && a.max_speed > SimScalar::ZERO)
            .map(|a| a.unit_type_id)
            .collect();
        ground.sort();
        let mut domes: Vec<_> = world
            .catalog
            .archetypes()
            .filter(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
            .map(|a| a.unit_type_id)
            .collect();
        domes.sort();
        let (unit, host) = (ground[0], domes[0]);
        world.catalog.get_mut(unit).expect("unit").supply_cost = 0;
        world
            .catalog
            .get_mut(host)
            .expect("host")
            .capabilities
            .death_spawn = Some(DeathSpawnSpec {
            unit,
            count,
            delay_ticks: delay,
            search_range: SimScalar::ZERO,
            timed_life: None,
            disabled_while_constructing: true,
        });
        (world, host, unit)
    }

    fn entry(host: UnitTypeId, due_in: u16, x: i32) -> PendingDeathSpawn {
        PendingDeathSpawn {
            due_in,
            host,
            owner: PlayerId(1),
            point: SimVec2::from_i32(x, 10),
            heading: SimAngle::ZERO,
        }
    }

    fn count_of(world: &SimWorld, unit: UnitTypeId) -> usize {
        world
            .entities
            .values()
            .filter(|e| e.unit_type_id == unit)
            .count()
    }

    /// Entries count down once per step; the due ones are created in queue order (ids rise with
    /// the queue position) and removed, the rest keep their order; an empty list stays empty.
    #[test]
    fn entries_count_down_and_the_due_ones_are_created_in_queue_order() {
        let (mut world, host, unit) = hosted(2, 3);
        world.step_death_spawns();
        assert_eq!(world.pending_death_spawns(), []);
        world.pending_death_spawns.extend([
            entry(host, 2, 10),
            entry(host, 1, 20),
            entry(host, 3, 30),
        ]);
        world.step_death_spawns();
        assert_eq!(count_of(&world, unit), 2, "the second entry was due");
        assert_eq!(
            world
                .pending_death_spawns()
                .iter()
                .map(|e| (e.due_in, e.point.x))
                .collect::<Vec<_>>(),
            [(1, SimScalar::from_i32(10)), (2, SimScalar::from_i32(30))]
        );
        world.step_death_spawns();
        world.step_death_spawns();
        assert_eq!(world.pending_death_spawns(), []);
        let mut created: Vec<_> = world
            .entities
            .values()
            .filter(|e| e.unit_type_id == unit)
            .map(|e| (e.id, e.position.x))
            .collect();
        created.sort();
        let xs: Vec<_> = created.iter().map(|(_, x)| *x).collect();
        assert_eq!(xs, [20, 20, 10, 10, 30, 30].map(SimScalar::from_i32));
    }

    /// A due entry whose host has lost its death spawn creates nothing; a delay of 0 is created
    /// on the next step.
    #[test]
    fn a_host_without_a_spawn_creates_nothing_and_a_zero_delay_is_due_next_step() {
        let (mut world, host, unit) = hosted(3, 0);
        world.pending_death_spawns.push(entry(host, 0, 10));
        world.step_death_spawns();
        assert_eq!(count_of(&world, unit), 3);
        world.pending_death_spawns.push(entry(host, 1, 10));
        world
            .catalog
            .get_mut(host)
            .expect("host")
            .capabilities
            .death_spawn = None;
        world.step_death_spawns();
        assert_eq!(count_of(&world, unit), 3);
        assert_eq!(world.pending_death_spawns(), []);
    }

    /// DS-12: two worlds differing only in one field of one pending entry have different
    /// checksums, and an entry at all differs from none.
    #[test]
    fn every_field_of_a_pending_entry_is_hashed() {
        let (mut world, host, unit) = hosted(1, 9);
        let none = world.compute_checksum();
        let base = entry(host, 5, 10);
        world.pending_death_spawns.push(base);
        let reference = world.compute_checksum();
        assert_ne!(none, reference);
        let variants = [
            PendingDeathSpawn { due_in: 6, ..base },
            PendingDeathSpawn { host: unit, ..base },
            PendingDeathSpawn {
                owner: PlayerId(0),
                ..base
            },
            PendingDeathSpawn {
                point: SimVec2::from_i32(11, 10),
                ..base
            },
            PendingDeathSpawn {
                heading: SimAngle::DEG_180,
                ..base
            },
        ];
        let mut sums = vec![reference];
        for variant in variants {
            world.pending_death_spawns[0] = variant;
            sums.push(world.compute_checksum());
        }
        for (i, a) in sums.iter().enumerate() {
            for b in &sums[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
