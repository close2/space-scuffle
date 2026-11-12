//! The phase 2 sub-steps of the effect engine (plan `2026-11-10-spellcaster-mechanics` D-4,
//! D-16): (a) missiles, (b) persistents, (c) statuses, (d) displacements, in that order.
//!
//! Each sub-step steps only the instances it had when it started: a walk run inside a
//! sub-step appends new instances, which are first stepped on the next tick. Pulse k of a
//! status or persistent is due `round_half_up(k × period)` ticks after its start, computed
//! from the exact product each time, so no error accumulates.

use super::filter::unit_target;
use super::state::PersistentAnchor;
use crate::catalog::EffectNode;
use crate::SimWorld;
use sim_types::{EffectContext, EffectTarget, EntityId, SimScalar, SimVec2, StatusId, Tick};

/// What one step does to a missile (plan `2026-11-11-contact-and-missiles` D-6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MissileStep {
    /// The step reaches contact: the missile impacts on this step, at this point.
    Impact(SimVec2),
    /// The missile flies on, and now stands at this point.
    Fly(SimVec2),
}

/// One step of a missile at `position` flying `speed` world units a tick at `aim`, stopping
/// `radius` short of it (a unit target's collision radius, 0 for a point).
///
/// The game's Guidance missiles impact when they arrive (B-115), as our weapon projectiles do
/// (`advance_projectile`): if the gap to contact is at most `speed`, the missile impacts on
/// this step at the contact point, which is `aim` itself for a point and the point `radius`
/// from `aim` on the line from `position` for a unit (or `position`, if it is already within
/// `radius`). Otherwise it moves `speed` towards `aim`. No arrival tolerance is needed: the
/// test is `gap ≤ speed`, and the contact point is computed, not reached by stepping.
pub(crate) fn missile_step(
    position: SimVec2,
    aim: SimVec2,
    radius: SimScalar,
    speed: SimScalar,
) -> MissileStep {
    let distance = position.distance(aim);
    let gap = distance.saturating_sub(radius).max(SimScalar::ZERO);
    let direction = (aim - position).normalize_or_zero();
    if gap <= speed {
        let contact = if radius == SimScalar::ZERO {
            aim
        } else if distance <= radius {
            position
        } else {
            aim - direction * radius
        };
        return MissileStep::Impact(contact);
    }
    MissileStep::Fly(position + direction * speed)
}

/// Ticks after the start at which pulse `k` is due: `round_half_up(k × period)`, or `None` if
/// it is not representable.
#[must_use]
pub fn pulse_offset(period: SimScalar, k: u16) -> Option<u32> {
    let exact = period.checked_mul(SimScalar::from_i32(i32::from(k)))?;
    let rounded = exact.checked_add(SimScalar::HALF)?.raw() >> 32;
    u32::try_from(rounded).ok()
}

impl SimWorld {
    /// Runs the four sub-steps of the effect engine for this tick.
    pub(crate) fn step_effects(&mut self) {
        self.step_effect_missiles();
        self.step_persistents();
        self.step_statuses();
        self.step_displacements();
    }

    /// (a) Advances each missile in id order ([`missile_step`]); one whose step reaches contact
    /// runs its impact with the contact point as the source and is removed, one whose unit
    /// target is no longer valid is dropped.
    pub(crate) fn step_effect_missiles(&mut self) {
        let spells = self.catalog.spells_handle();
        let count = self.effects.missiles.len();
        self.effect_scratch.finished.clear();
        for index in 0..count {
            let missile = self.effects.missiles[index];
            let Some(EffectNode::LaunchMissile { speed, impact, .. }) =
                spells.node(missile.node).map(|node| node.kind)
            else {
                self.effect_scratch.finished.push(missile.id);
                continue;
            };
            let Some((aim, radius)) = self.missile_aim(&missile.context, missile.target) else {
                self.effect_scratch.finished.push(missile.id);
                continue;
            };
            match missile_step(missile.position, aim, radius, speed) {
                MissileStep::Impact(contact) => {
                    self.effect_scratch.finished.push(missile.id);
                    let context = missile.context.retarget(missile.target, contact);
                    self.run_effect_walk(impact, context);
                }
                MissileStep::Fly(moved) => self.effects.missiles[index].position = moved,
            }
        }
        let finished = &self.effect_scratch.finished;
        self.effects
            .missiles
            .retain(|missile| !finished.contains(&missile.id));
    }

    /// The point a missile flies at and the radius it stops short by, or `None` when its unit
    /// target died, loaded, vanished or became untargetable for the caster's owner (D-8).
    fn missile_aim(
        &self,
        context: &EffectContext,
        target: EffectTarget,
    ) -> Option<(SimVec2, SimScalar)> {
        match target {
            EffectTarget::Point(point) => Some((point, SimScalar::ZERO)),
            EffectTarget::Unit(unit) => {
                let entity = unit_target(&self.entities, unit)?;
                if entity.owner != context.owner
                    && !Self::is_entity_targetable(
                        entity,
                        &self.map,
                        &self.vision_maps,
                        context.owner,
                    )
                {
                    return None;
                }
                let radius = self
                    .catalog
                    .get(entity.unit_type_id)
                    .map_or(SimScalar::ZERO, |arch| arch.collision_radius);
                Some((entity.position, radius))
            }
        }
    }

    /// (b) Runs each persistent's due pulses in id order; removes it after its last pulse or
    /// when its anchor unit is gone.
    fn step_persistents(&mut self) {
        let spells = self.catalog.spells_handle();
        let count = self.effects.persistents.len();
        self.effect_scratch.finished.clear();
        let now = self.current_tick;
        for index in 0..count {
            let persistent = self.effects.persistents[index];
            let Some(EffectNode::Persistent {
                periodic,
                period_ticks,
                count: pulses,
                ..
            }) = spells.node(persistent.node).map(|node| node.kind)
            else {
                self.effect_scratch.finished.push(persistent.id);
                continue;
            };
            let child_context = match persistent.anchor {
                PersistentAnchor::Point(point) => persistent
                    .context
                    .retarget(EffectTarget::Point(point), point),
                PersistentAnchor::Unit(unit) => {
                    let Some(entity) = unit_target(&self.entities, unit) else {
                        self.effect_scratch.finished.push(persistent.id);
                        continue;
                    };
                    persistent
                        .context
                        .retarget(EffectTarget::Unit(unit), entity.position)
                }
            };
            let elapsed = now.wrapping_sub(persistent.created_tick.0);
            let mut done = persistent.pulses_done;
            while done < pulses
                && pulse_offset(period_ticks, done + 1).is_some_and(|due| due <= elapsed)
            {
                done += 1;
                self.effects.persistents[index].pulses_done = done;
                if let Some(node) = periodic {
                    self.run_effect_walk(node, child_context);
                }
            }
            if done >= pulses {
                self.effect_scratch.finished.push(persistent.id);
            }
        }
        let finished = &self.effect_scratch.finished;
        self.effects
            .persistents
            .retain(|persistent| !finished.contains(&persistent.id));
    }

    /// (c) Runs each status's due pulses in `(EntityId, StatusId)` order, then removes it when
    /// it has expired or its bearer is gone. Statuses applied on this tick are skipped: no
    /// pulse can be due on the tick of application, since periods are at least one tick.
    fn step_statuses(&mut self) {
        let spells = self.catalog.spells_handle();
        let now = self.current_tick;
        let mut cursor: Option<(EntityId, StatusId)> = None;
        loop {
            let start = cursor.map_or(0, |key| {
                self.effects
                    .statuses
                    .partition_point(|s| (s.entity, s.status) <= key)
            });
            let Some(entry) = self.effects.statuses.get(start).copied() else {
                break;
            };
            let key = (entry.entity, entry.status);
            cursor = Some(key);
            if entry.applied_tick == Tick(now) {
                continue;
            }
            if !self.entities.contains_key(&entry.entity) {
                self.effects.statuses.remove(start);
                continue;
            }
            let Some(status) = spells.status(entry.status) else {
                continue;
            };
            let elapsed = now.wrapping_sub(entry.applied_tick.0);
            if let (Some(period), Some(effect)) = (status.spec.period_ticks, status.periodic_effect)
            {
                self.run_status_pulses(key, period, effect, elapsed);
            }
            if let Ok(index) = self.effects.find_status(key.0, key.1) {
                if SimScalar::from_num(elapsed) >= self.effects.statuses[index].expires_at {
                    self.effects.statuses.remove(index);
                    self.refresh_status_modifiers(key.0);
                }
            }
        }
    }

    /// Runs the pulses of status `key` due by `elapsed` ticks and within its duration.
    fn run_status_pulses(
        &mut self,
        key: (EntityId, StatusId),
        period: SimScalar,
        effect: sim_types::EffectId,
        elapsed: u32,
    ) {
        loop {
            let Ok(index) = self.effects.find_status(key.0, key.1) else {
                return;
            };
            let entry = self.effects.statuses[index];
            let next = entry.pulses_done.saturating_add(1);
            let Some(due) = pulse_offset(period, next) else {
                return;
            };
            if due > elapsed || SimScalar::from_num(due) > entry.expires_at {
                return;
            }
            self.effects.statuses[index].pulses_done = next;
            if unit_target(&self.entities, key.0).is_some() {
                self.run_effect_walk(effect, entry.context);
            }
        }
    }

    /// (d) Moves each displaced unit along its flight in `EntityId` order; a unit whose flight
    /// is over lands, then runs its node's `on_land`.
    fn step_displacements(&mut self) {
        let spells = self.catalog.spells_handle();
        let now = self.current_tick;
        self.effect_scratch.landed.clear();
        for (id, entity) in &mut self.entities {
            let Some(flight) = entity.displacement else {
                continue;
            };
            let Some(EffectNode::Displace { flight_ticks, .. }) =
                spells.node(flight.node).map(|node| node.kind)
            else {
                entity.displacement = None;
                continue;
            };
            let elapsed = now
                .wrapping_sub(flight.start_tick.0)
                .min(u32::from(flight_ticks));
            let fraction =
                SimScalar::from_num(elapsed).saturating_div(SimScalar::from_num(flight_ticks));
            entity.position = flight.from + (flight.to - flight.from) * fraction;
            entity.velocity = SimVec2::ZERO;
            if elapsed >= u32::from(flight_ticks) {
                self.effect_scratch.landed.push(*id);
            }
        }
        for index in 0..self.effect_scratch.landed.len() {
            let id = self.effect_scratch.landed[index];
            self.land_displaced(id);
        }
    }

    /// Ends the flight of `id` at its destination, re-validated for its movement layer (else the
    /// nearest valid point of the origin ring, else where it started), then runs `on_land`.
    fn land_displaced(&mut self, id: EntityId) {
        let Some(flight) = self.entities.get(&id).and_then(|e| e.displacement) else {
            return;
        };
        let landing = self
            .first_valid_placement(id, flight.to, crate::effects::LANDING_RING_RANGE)
            .unwrap_or(flight.from);
        let spells = self.catalog.spells_handle();
        let on_land = match spells.node(flight.node).map(|node| node.kind) {
            Some(EffectNode::Displace { on_land, .. }) => on_land,
            _ => None,
        };
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.position = landing;
            entity.displacement = None;
            entity.current_funnel_waypoints.clear();
        }
        if let Some(node) = on_land {
            let context = flight.context.retarget(EffectTarget::Unit(id), landing);
            self.run_effect_walk(node, context);
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{missile_step, pulse_offset, MissileStep};
    use crate::catalog::EffectNode;
    use crate::effects::state::ActiveMissile;
    use crate::test_support::world;
    use crate::SimWorld;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{
        AbilitySource, EffectAnchor, EffectContext, EffectId, EffectTarget, EntityId, MoverType,
        PlayerId, SimScalar, SimVec2,
    };

    fn period(text: &str) -> SimScalar {
        ron::from_str(text).unwrap_or(SimScalar::ZERO)
    }

    /// The pulse ticks of plan §3: Storm's status (10.196) and Fungal Growth (4.464).
    #[test]
    fn pulse_ticks_round_the_exact_product() {
        let storm: Vec<u32> = (1..=7)
            .filter_map(|k| pulse_offset(period("10.196429"), k))
            .collect();
        assert_eq!(storm, vec![10, 20, 31, 41, 51, 61, 71]);
        let fungal: Vec<u32> = (1..=16)
            .filter_map(|k| pulse_offset(period("4.464286"), k))
            .collect();
        assert_eq!(
            fungal,
            vec![4, 9, 13, 18, 22, 27, 31, 36, 40, 45, 49, 54, 58, 63, 67, 71]
        );
        let storm_area: Vec<u32> = (1..=6)
            .filter_map(|k| pulse_offset(period("10.2"), k))
            .collect();
        assert_eq!(storm_area, vec![10, 20, 31, 41, 51, 61]);
    }

    /// A flat world with the shipped catalogs.
    fn flat_world() -> SimWorld {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a small flat map");
        world(map, 11)
    }

    /// The first shipped `LaunchMissile` node flying to `anchor`, with its speed.
    fn launch_node(world: &SimWorld, anchor: EffectAnchor) -> (EffectId, SimScalar) {
        let spells = world.catalog.spells();
        (0..spells.node_count())
            .find_map(|index| {
                let id = EffectId(u16::try_from(index).ok()?);
                match spells.node(id)?.kind {
                    EffectNode::LaunchMissile { to, speed, .. } if to == anchor => {
                        Some((id, speed))
                    }
                    _ => None,
                }
            })
            .expect("the shipped catalog launches a missile at this anchor")
    }

    /// Spawns the first mobile ground archetype of the shipped catalog for player 0 at `at`;
    /// returns it and its collision radius.
    fn ground_unit(world: &mut SimWorld, at: SimVec2) -> (EntityId, SimScalar) {
        let (kind, radius) = world
            .catalog
            .archetypes()
            .find(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
            .map(|arch| (arch.unit_type_id, arch.collision_radius))
            .expect("the shipped catalog has a mobile ground unit");
        let id = world
            .spawn_unit(kind, PlayerId(0), at)
            .expect("invariant: a free spot on an empty map");
        (id, radius)
    }

    /// A missile of player 0 running `node`, at `position`, flying at `target`.
    fn missile(node: EffectId, position: SimVec2, target: EffectTarget) -> ActiveMissile {
        ActiveMissile {
            id: 0,
            node,
            context: EffectContext {
                owner: PlayerId(0),
                caster: None,
                ability: AbilitySource::default(),
                origin: position,
                source_point: position,
                target,
            },
            position,
            target,
        }
    }

    /// T-M1 (plan `2026-11-11-contact-and-missiles` D-6, B-115): a missile whose step reaches
    /// contact impacts on that step. A homing missile 0.5 from its unit's edge and a missile
    /// 0.5 from its point are both gone after one step at their shipped speeds (over 0.5).
    #[test]
    fn a_missile_impacts_on_the_step_that_reaches_contact() {
        let mut world = flat_world();
        let centre = SimVec2::from_i32(16, 16);
        let (target, radius) = ground_unit(&mut world, centre);
        let (node, speed) = launch_node(&world, EffectAnchor::TargetUnit);
        assert!(speed > SimScalar::HALF);
        let start = SimVec2::new(centre.x - radius - SimScalar::HALF, centre.y);
        world
            .effects
            .missiles
            .push(missile(node, start, EffectTarget::Unit(target)));
        world.step_effect_missiles();
        assert!(
            world.effects.missiles.is_empty(),
            "the homing missile impacts on its first step"
        );

        let (node, speed) = launch_node(&world, EffectAnchor::TargetPoint);
        assert!(speed > SimScalar::HALF);
        let point = SimVec2::from_i32(8, 8);
        let start = SimVec2::new(point.x, point.y - SimScalar::HALF);
        world
            .effects
            .missiles
            .push(missile(node, start, EffectTarget::Point(point)));
        world.step_effect_missiles();
        assert!(
            world.effects.missiles.is_empty(),
            "the point missile impacts on its first step"
        );
    }

    /// T-M1, the contact point: a unit-target missile 0.5 from contact at 1.05 impacts at the
    /// target's edge on the line it came from; a point-target missile impacts on its point; a
    /// missile further than its speed from contact flies exactly its speed.
    #[test]
    fn the_step_impacts_at_the_contact_point_or_flies_its_speed() {
        let speed = SimScalar::from_num(1.05);
        let radius = SimScalar::from_num(0.375);
        let aim = SimVec2::from_i32(10, 10);
        let start = SimVec2::new(aim.x - radius - SimScalar::HALF, aim.y);
        assert_eq!(
            missile_step(start, aim, radius, speed),
            MissileStep::Impact(SimVec2::new(aim.x - radius, aim.y))
        );
        let start = SimVec2::new(aim.x, aim.y - SimScalar::HALF);
        assert_eq!(
            missile_step(start, aim, SimScalar::ZERO, speed),
            MissileStep::Impact(aim)
        );
        let inside = SimVec2::new(aim.x + SimScalar::from_num(0.25), aim.y);
        assert_eq!(
            missile_step(inside, aim, radius, speed),
            MissileStep::Impact(inside),
            "already within the radius: impacts where it is"
        );
        let far = SimVec2::new(aim.x - SimScalar::from_i32(3), aim.y);
        assert_eq!(
            missile_step(far, aim, radius, speed),
            MissileStep::Fly(SimVec2::new(far.x + speed, aim.y))
        );
        let edge = SimVec2::new(aim.x - radius - speed, aim.y);
        assert_eq!(
            missile_step(edge, aim, radius, speed),
            MissileStep::Impact(SimVec2::new(aim.x - radius, aim.y)),
            "a gap of exactly the speed is reached on this step"
        );
    }
}
