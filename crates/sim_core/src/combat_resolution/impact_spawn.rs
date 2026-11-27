//! Impact spawns: the units a weapon's finishing missile creates (plan `2026-11-28-broodlings`
//! D-4, D-5; `StarCraft` II's `FinishEffect` `CEffectCreateUnit`, the Brood Lord's Broodlings).
//!
//! # Responsibility
//! Collects, while the tick's projectiles resolve, one pending spawn per finishing missile whose
//! weapon has a [`sim_types::ImpactSpawn`], and commits them all once the last projectile of the
//! tick has resolved: each created unit is placed, faced along the throw, given its timed life
//! and its first order.
//!
//! # How it fits
//! [`SimWorld::advance_projectiles`] calls [`SimWorld::collect_impact_spawn`] after every
//! `Impact` (whether the strike landed, was refused at impact, or the target had been lost and
//! the missile flew to its last point), and [`SimWorld::commit_impact_spawns`] after its loop.
//! A `Disjointed` missile (fuel-out or `Fizzle`) finishes nowhere and creates nothing. Units are
//! created through the production spawn transaction (`prepare_spawn` / `commit_spawn`), as the
//! hangar creates its members, and ordered through [`crate::Entity::issue_order`], never through a
//! `PlayerCommand`.
//!
//! # Invariants
//! - No unit created this tick is struck by another projectile or splash of the same tick: the
//!   spawns commit after every impact has resolved.
//! - Spawns commit in the order the impacts resolved (the projectile phase's reverse launch-list
//!   order), so entity ids and the timed-life draws are deterministic.
//! - The pending list is empty between ticks, so it is not hashed.
//! - A spawn with no valid place within its search range creates nothing and reports
//!   [`SimulationDiagnostic::ImpactSpawnBlocked`]; a refused spawn transaction reports
//!   [`SimulationDiagnostic::ProductionInvariant`]. Neither is silent.
//! - Placement reads terrain and structures only, never other units (the effect engine's
//!   placement rule, plan `2026-11-10` D-8; B-236): overlaps are separated by the next tick's
//!   movement. The search and the clock draw are the world's shared spawn helpers
//!   (`world::spawns`, plan `2026-12-01-death-spawns-and-hangar-commands` D-4).

use crate::entity::ActiveProjectile;
use crate::movement::MovementInvariant;
use crate::weapon_timing::SimulationDiagnostic;
use crate::world::SimWorld;
use sim_types::{
    AttackOrigin, EntityId, ImpactSpawn, PlayerId, SimAngle, SimVec2, SpawnOrder, Tick, UnitOrder,
};

/// One finished missile's spawn, waiting for the end of the projectile phase.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PendingSpawn {
    /// What the missile's weapon creates.
    spawn: ImpactSpawn,
    /// The missile's owner, who owns the created units.
    owner: PlayerId,
    /// Where the flight finished: the target's tracked position, or the last point.
    point: SimVec2,
    /// The missile's target, if it still had one at the finish.
    target: Option<EntityId>,
    /// Facing of the created units: from the launch point to the finish point.
    heading: SimAngle,
    /// Units to create: `per_strike` times the strikes the missile carried.
    count: u16,
}

impl SimWorld {
    /// Queues the spawn of the finished missile `proj`, if its weapon has one (D-4). Called after
    /// the missile's impact has resolved, whatever the strike did.
    pub(super) fn collect_impact_spawn(&mut self, proj: &ActiveProjectile) {
        let Some(spawn) = proj.weapon_spec.impact_spawn else {
            return;
        };
        let count = u16::from(spawn.per_strike) * u16::from(proj.strikes);
        if count == 0 {
            return;
        }
        let throw = proj.target_pos - proj.source_pos;
        self.combat.pending_spawns.push(PendingSpawn {
            spawn,
            owner: proj.owner,
            point: proj.target_pos,
            target: proj.target_id,
            heading: SimAngle::atan2(throw.y, throw.x),
            count,
        });
    }

    /// Commits every spawn queued this tick, in the order the impacts resolved (D-4).
    pub(super) fn commit_impact_spawns(&mut self) {
        if self.combat.pending_spawns.is_empty() {
            return;
        }
        let mut pending = std::mem::take(&mut self.combat.pending_spawns);
        for spawn in &pending {
            for _ in 0..spawn.count {
                self.commit_one_spawn(spawn);
            }
        }
        pending.clear();
        self.combat.pending_spawns = pending;
    }

    /// Creates one unit of `pending`: prepared for its owner, placed within the search range,
    /// faced along the throw, given its timed life, committed, then ordered.
    fn commit_one_spawn(&mut self, pending: &PendingSpawn) {
        let tick = Tick(self.current_tick);
        let mut prepared =
            match self.prepare_spawn(pending.spawn.unit, pending.owner, pending.point, 0) {
                Ok(prepared) => prepared,
                Err(source) => {
                    self.tick_diagnostics
                        .push(SimulationDiagnostic::ProductionInvariant { tick, source });
                    return;
                }
            };
        let Some(place) =
            self.spawn_place(&prepared.entity, pending.point, pending.spawn.search_range)
        else {
            self.tick_diagnostics
                .push(SimulationDiagnostic::ImpactSpawnBlocked {
                    tick,
                    unit: pending.spawn.unit,
                    point: pending.point,
                });
            return;
        };
        let unit = &mut prepared.entity;
        unit.position = place;
        unit.pos_at_tick_start = place;
        unit.heading = pending.heading;
        unit.turret_angle = pending.heading;
        prepared.entity.timed_life = self.draw_timed_life(pending.spawn.timed_life);
        let id = self.commit_spawn(prepared);
        self.give_spawn_order(id, pending);
    }

    /// Gives the created unit `id` its first order (D-4): attack the missile's target while it
    /// lives and the unit may attack it, else attack-move to the finish point.
    fn give_spawn_order(&mut self, id: EntityId, pending: &PendingSpawn) {
        let order = match pending.spawn.order {
            SpawnOrder::AttackTargetOrPoint => pending
                .target
                .filter(|&target| self.spawn_may_attack(id, target))
                .map_or(
                    UnitOrder::AttackMove {
                        target: pending.point,
                    },
                    |target| UnitOrder::AttackTarget {
                        target,
                        origin: AttackOrigin::Explicit,
                    },
                ),
        };
        let Some(unit) = self.entities.get_mut(&id) else {
            return;
        };
        if let Err(source) = unit.issue_order(order, false) {
            self.tick_diagnostics
                .push(SimulationDiagnostic::MovementInvariant {
                    tick: Tick(self.current_tick),
                    source: MovementInvariant::Admission { entity: id, source },
                });
        }
    }

    /// Whether the created unit `unit` may attack `target` now: the target lives, is neither
    /// carried nor housed, passes the unit's owner's weapon filters (seen, detected if concealed)
    /// and the unit's weapon admits its plane and attributes; the rule a player's attack order
    /// and a hangar member's attack are admitted by.
    fn spawn_may_attack(&self, unit: EntityId, target: EntityId) -> bool {
        let (Some(attacker), Some(victim)) = (self.entities.get(&unit), self.entities.get(&target))
        else {
            return false;
        };
        if !victim.is_alive() || victim.is_loaded_in_transport() || victim.is_housed() {
            return false;
        }
        let (Ok(arch), Ok(victim_arch)) = (
            self.catalog.get(attacker.unit_type_id),
            self.catalog.get(victim.unit_type_id),
        ) else {
            return false;
        };
        self.is_weapon_target_available(attacker.owner, target)
            && attacker.can_attack(arch, victim, victim_arch)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    //! Plan `2026-11-28-broodlings` D-7: the checksum's reading of a magazine, a clock and an
    //! impact spawn (the placement search's test moved with it to `world::spawns`).
    use crate::test_support::world;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{MoverType, PlayerId, SimScalar, SimVec2, UnitTypeId};

    /// The first shipped ground unit and a structure type, by id.
    fn ground_and_structure(world: &crate::SimWorld) -> (UnitTypeId, UnitTypeId) {
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
            .filter(|a| matches!(a.shape, sim_types::ShapeKind::BuildingDome { .. }))
            .map(|a| a.unit_type_id)
            .collect();
        domes.sort();
        (ground[0], domes[0])
    }

    /// B-10: two worlds differing only in a magazine count, a clock, or an impact spawn value
    /// carried by a projectile have different checksums.
    #[test]
    fn a_magazine_a_clock_and_an_impact_spawn_are_hashed() {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0))).expect("map");
        let mut base = world(map, 3);
        let (unit, _) = ground_and_structure(&base);
        let id = base
            .spawn_unit(unit, PlayerId(0), SimVec2::from_i32(8, 8))
            .expect("a unit");
        let reference = base.compute_checksum();
        let mut sums = vec![reference];
        let edits: [fn(&mut crate::entity::Entity); 3] = [
            |e| {
                e.strike_magazine = Some(crate::entity::StrikeMagazineState {
                    armed: 1,
                    arming_left: 0,
                    build_left: 0,
                });
            },
            |e| e.timed_life = Some(89),
            |e| e.timed_life = Some(90),
        ];
        for edit in edits {
            let unit = base.entities.get_mut(&id).expect("unit");
            let saved = (unit.strike_magazine, unit.timed_life);
            edit(unit);
            sums.push(base.compute_checksum());
            let unit = base.entities.get_mut(&id).expect("unit");
            (unit.strike_magazine, unit.timed_life) = saved;
        }
        assert_eq!(base.compute_checksum(), reference, "premise: restored");
        for (i, a) in sums.iter().enumerate() {
            for b in &sums[i + 1..] {
                assert_ne!(a, b);
            }
        }
        let weapon = base
            .catalog
            .archetypes()
            .find_map(|a| a.weapon.clone().filter(|w| w.projectile_speed.is_some()))
            .expect("a missile weapon");
        let projectile = |spawn: Option<sim_types::ImpactSpawn>| crate::entity::ActiveProjectile {
            id: 0,
            owner: PlayerId(0),
            attacker_id: None,
            source_pos: SimVec2::from_i32(1, 1),
            current_pos: SimVec2::from_i32(1, 1),
            target_id: None,
            target_pos: SimVec2::from_i32(2, 2),
            target_mover: MoverType::Ground,
            speed: SimScalar::ONE,
            weapon_spec: sim_types::WeaponSpec {
                impact_spawn: spawn,
                ..weapon.clone()
            },
            source_slug: None,
            remaining_lifetime_ticks: 10,
            strikes: 2,
            target_teleport_count: 0,
            mover: None,
            phase: 0,
            travelled: SimScalar::ZERO,
        };
        let spawn = sim_types::ImpactSpawn {
            unit,
            per_strike: 1,
            search_range: SimScalar::from_i32(5),
            timed_life: None,
            order: sim_types::SpawnOrder::AttackTargetOrPoint,
        };
        let mut with = Vec::new();
        for variant in [
            None,
            Some(spawn),
            Some(sim_types::ImpactSpawn {
                per_strike: 2,
                ..spawn
            }),
        ] {
            base.projectiles.push(projectile(variant));
            with.push(base.compute_checksum());
            base.projectiles.clear();
        }
        assert_ne!(with[0], with[1]);
        assert_ne!(with[1], with[2]);
    }
}
