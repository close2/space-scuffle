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

/// Arrival tolerance of a missile, in world units (2⁻²⁰): only fixed-point rounding of the
/// last step can leave a gap this small, so a missile within it has arrived.
const ARRIVAL_EPSILON: SimScalar = SimScalar::from_raw(1 << 12);

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

    /// (a) Advances each missile in id order; one that has arrived runs its impact and is
    /// removed, one whose unit target is no longer valid is dropped.
    fn step_effect_missiles(&mut self) {
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
            let gap = missile
                .position
                .distance(aim)
                .saturating_sub(radius)
                .max(SimScalar::ZERO);
            if gap <= ARRIVAL_EPSILON {
                self.effect_scratch.finished.push(missile.id);
                let source = match missile.target {
                    EffectTarget::Point(point) => point,
                    EffectTarget::Unit(_) => missile.position,
                };
                let context = missile.context.retarget(missile.target, source);
                self.run_effect_walk(impact, context);
                continue;
            }
            let step = speed.min(gap);
            let direction = (aim - missile.position).normalize_or_zero();
            let moved = missile.position + direction * step;
            self.effects.missiles[index].position = if step == gap && radius == SimScalar::ZERO {
                aim
            } else {
                moved
            };
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
mod tests {
    use super::pulse_offset;
    use sim_types::SimScalar;

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
}
