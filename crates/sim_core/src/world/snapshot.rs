//! Tick phase 10: the render snapshot.
//!
//! # Responsibility
//! Writes the world's visible state into a [`RenderSnapshot`]: player status, resource nodes,
//! one bubble per entity not carried in a transport, and the tick's visual events. A bubble's
//! attack cycle (`AttackSwing`) is derived from the weapon state machine through the same
//! weapon resolution combat uses, with no state of its own.
//!
//! # How it fits
//! The last phase of `step_into`, and the only place the simulation converts fixed-point values
//! to `f32` (charter §3.2).
//!
//! # Invariants
//! - Writing a snapshot changes no authoritative state and no checksum; it only drains
//!   `pending_events`.
//! - Bubbles are ordered by entity ID.

use crate::catalog::UnitArchetype;
use crate::entity::{Entity, WeaponPhase};
use crate::SimWorld;
use sim_map::SimMap;
use sim_types::{
    AttackSwing, BubbleRenderInstance, MoverType, PlayerSlot, PlayerStatusView, RenderSnapshot,
    ResourceNodeId, ResourceNodeView, SimScalar, Tick, WeaponVisualKind,
};

impl SimWorld {
    /// Phase 10: Build immutable render snapshot for client visualization.
    pub fn build_render_snapshot(&mut self) -> RenderSnapshot {
        let mut output = RenderSnapshot::default();
        self.write_render_snapshot(&mut output);
        output
    }

    /// Fraction of a foundation built, or of a building's front job done.
    fn published_progress(entity: &Entity) -> Option<f32> {
        let (remaining, total) = match (&entity.construction, entity.production_queue.front()) {
            (Some(record), _) => (record.remaining_ticks, record.total_ticks),
            (None, Some(job)) => (job.remaining_ticks, job.total_ticks),
            (None, None) => return None,
        };
        let done = total.saturating_sub(remaining);
        let fraction = u64::from(done) * (1 << 16) / u64::from(total.max(1));
        Some(SimScalar::from_raw(i64::try_from(fraction).unwrap_or(0) << 16).to_f32())
    }

    /// Writes the current immutable visual state into caller-owned reusable buffers.
    ///
    /// Entity records are ordered by entity ID. This consumes pending visual events
    /// exactly once, preserving both buffers' capacity; it is not a per-frame getter.
    /// No simulation phase executes and authoritative state/checksum are unchanged.
    pub fn write_render_snapshot(&mut self, output: &mut RenderSnapshot) {
        // PERFORMANCE RATIONALE:
        // Why: Paired client snapshots should reuse storage across ordinary ticks.
        // What: Clear and refill entity records, then move event elements without
        // replacing either Vec allocation. Reserve only for actual entity growth.
        // Trade-off: Callers must consume output events before reusing the buffer.
        output.bubbles.clear();
        output.bubbles.reserve(self.entities.len());
        output.combat_events.clear();
        output.tick = Tick(self.current_tick);
        output.checksum = self.last_checksum;
        output.match_result = self.match_state.result;
        self.write_player_views(output);
        self.write_resource_nodes(output);

        for entity in self.entities.values() {
            if entity.is_loaded_in_transport() {
                continue;
            }
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };
            let swing = self.published_swing(entity, arch);
            output
                .bubbles
                .push(bubble_instance(entity, arch, &self.map, swing));
        }

        output.combat_events.append(&mut self.pending_events);
    }

    /// The attack cycle `entity` (of archetype `arch`) is in, derived from its weapon state.
    ///
    /// `Some` from the tick a swing's wind-up starts until the weapon is ready again: in
    /// `Windup` (`elapsed = damage point − remaining`), in `Backswing`, and in `Cooldown` while
    /// the cooldown runs (`elapsed = period − cooldown left`, clamped below the period and not
    /// below the damage point, where a random firing delay holds it). `None`
    /// in `Ready` and `RotatingToTarget`, and when the weapon or the attack rate cannot be
    /// resolved. The release charges `period − damage point` ticks of cooldown, so the two
    /// formulas meet at the release tick (`combat::release_swing`).
    ///
    /// The weapon comes from [`SimWorld::resolved_weapon`], as in combat. Its target plane is
    /// the swing's bound target in `Windup` and `Backswing`; `Cooldown` holds no target, so it
    /// is `current_target`'s plane. A target already gone counts as ground, as in combat; if
    /// the archetype has no weapon against the plane so found, the other plane's weapon is used,
    /// so a swing whose target died stays published until its cooldown ends.
    fn published_swing(&self, entity: &Entity, arch: &UnitArchetype) -> Option<AttackSwing> {
        let target = match entity.weapon_phase {
            WeaponPhase::Ready | WeaponPhase::RotatingToTarget { .. } => return None,
            WeaponPhase::Cooldown if entity.weapon_cooldown_remaining == 0 => return None,
            WeaponPhase::Windup { target_id, .. } | WeaponPhase::Backswing { target_id, .. } => {
                Some(target_id)
            }
            WeaponPhase::Cooldown => entity.current_target,
        };
        let target_mover = target
            .and_then(|id| self.entities.get(&id))
            .and_then(|target| self.catalog.get(target.unit_type_id).ok())
            .map_or(MoverType::Ground, |target| target.mover_type);
        let other_plane = if target_mover == MoverType::Air {
            MoverType::Ground
        } else {
            MoverType::Air
        };
        let rate = entity.attack_rate_multiplier().ok()?;
        let resolve =
            |mover| Self::resolved_weapon(&self.player_upgrades, entity, arch, mover, rate);
        let weapon = resolve(target_mover).or_else(|| resolve(other_plane))?;
        let damage_point_ticks = weapon.spec.damage_point_ticks;
        let period_ticks = weapon.period_ticks;
        let elapsed = match entity.weapon_phase {
            WeaponPhase::Windup {
                remaining_ticks, ..
            } => damage_point_ticks.saturating_sub(remaining_ticks),
            // A random firing delay (B-3) can charge more than `period − damage point`; the
            // swing then holds at its release rather than stepping back into the wind-up.
            _ => period_ticks
                .saturating_sub(entity.weapon_cooldown_remaining)
                .max(damage_point_ticks),
        };
        Some(AttackSwing {
            elapsed_ticks: elapsed.min(period_ticks.saturating_sub(1)),
            damage_point_ticks,
            period_ticks,
        })
    }

    /// Writes every player slot's resources, supply and defeat flag.
    fn write_player_views(&self, output: &mut RenderSnapshot) {
        for (slot, view) in PlayerSlot::ALL.into_iter().zip(output.players.iter_mut()) {
            let resources = &self.player_resources[slot];
            *view = PlayerStatusView {
                minerals: resources.minerals,
                gas: resources.gas,
                supply_used: resources.supply_used,
                supply_max: resources.supply_max,
                defeated: self.match_state.defeated & (1 << slot.index()) != 0,
            };
        }
    }

    /// Replaces the resource node list with the map's nodes, in map order.
    fn write_resource_nodes(&self, output: &mut RenderSnapshot) {
        output.resource_nodes.clear();
        output
            .resource_nodes
            .extend(
                self.map
                    .resources()
                    .iter()
                    .zip(0_u32..)
                    .map(|(node, index)| ResourceNodeView {
                        id: ResourceNodeId(index),
                        kind: node.kind,
                        position: [node.pos.x.to_f32(), node.pos.y.to_f32()],
                        remaining: node.remaining,
                    }),
            );
    }
}

/// The render record of one entity of archetype `arch` standing on `map`, in the attack cycle
/// `attack_swing` ([`SimWorld::published_swing`]).
fn bubble_instance(
    entity: &Entity,
    arch: &UnitArchetype,
    map: &SimMap,
    attack_swing: Option<AttackSwing>,
) -> BubbleRenderInstance {
    let ground_z = map.terrain_altitude_at(entity.position);
    let altitude_z = if matches!(arch.mover_type, MoverType::Air) {
        ground_z + 2.5
    } else {
        ground_z
    };

    let is_attacking = matches!(
        entity.weapon_phase,
        WeaponPhase::Windup { .. } | WeaponPhase::Backswing { .. }
    );

    let hp_ratio = if entity.max_hp > SimScalar::ZERO {
        (entity.hp / entity.max_hp).to_f32().clamp(0.0, 1.0)
    } else {
        0.0
    };

    let shield_ratio = if arch.max_shields > SimScalar::ZERO {
        (entity.shields / arch.max_shields).to_f32().clamp(0.0, 1.0)
    } else {
        0.0
    };

    let shield_flash = (f32::from(entity.shield_hit_flash_ticks)) / 8.0;
    let hull_flash = (f32::from(entity.hull_hit_flash_ticks)) / 8.0;

    let speed_boosted = entity.active_buffs.iter().any(|b| b.buff_id == 1);

    BubbleRenderInstance {
        entity_id: entity.id,
        shape: arch.shape,
        silhouette: arch.silhouette,
        unit_type: entity.unit_type_id,
        owner: entity.owner,
        world_pos: [
            entity.position.x.to_f32(),
            entity.position.y.to_f32(),
            altitude_z,
        ],
        ground_z,
        radius: arch.collision_radius.to_f32(),
        velocity: [entity.velocity.x.to_f32(), entity.velocity.y.to_f32()],
        heading_angle_rad: entity.heading.to_radians_f32(),
        aim_angle_rad: entity.turret_angle.to_radians_f32(),
        is_attacking,
        weapon_visual: entity
            .siege_weapon
            .as_ref()
            .map(|w| w.visual_kind)
            .or_else(|| arch.ground_weapon.as_ref().map(|w| w.visual_kind))
            .or_else(|| arch.weapon.as_ref().map(|w| w.visual_kind))
            .unwrap_or(WeaponVisualKind::None),
        attack_target_pos: None,
        anim_state: entity.current_anim,
        attack_swing,
        hp_ratio,
        shield_ratio,
        shield_hit_flash: shield_flash,
        hull_hit_angle_rad: entity.last_hit_angle.to_radians_f32(),
        hull_hit_flash: hull_flash,
        build_or_queue_progress: SimWorld::published_progress(entity),
        selected: false,
        cloaked: entity.is_cloaked,
        speed_boosted,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::weapon_timing::AttackRate;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{EntityId, PlayerId, ShapeKind, SimVec2, WeaponSpec};

    /// A world holding one mobile unit armed against ground, its ID and its archetype.
    fn armed_unit() -> (SimWorld, EntityId, UnitArchetype) {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat 32x32 map is valid");
        let mut world = crate::test_support::world(map, 7);
        let arch = world
            .catalog
            .archetypes()
            .find(|arch| {
                matches!(arch.shape, ShapeKind::UnitSphere)
                    && arch
                        .weapon_against(MoverType::Ground, None)
                        .is_some_and(|w| w.damage_point_ticks >= 2 && w.period_ticks >= 4)
            })
            .cloned()
            .expect("the shipped catalog has a mobile unit armed against ground");
        let at = SimVec2::new(SimScalar::from_i32(16), SimScalar::from_i32(16));
        let id = world
            .spawn_unit(arch.unit_type_id, PlayerId(0), at)
            .expect("the unit spawns");
        (world, id, arch)
    }

    /// The swing `world` publishes for `id` in `phase` with `cooldown` ticks left.
    fn swing_in(
        world: &mut SimWorld,
        id: EntityId,
        arch: &UnitArchetype,
        phase: WeaponPhase,
        cooldown: u16,
    ) -> Option<AttackSwing> {
        let entity = world.entities.get_mut(&id).expect("the unit exists");
        entity.weapon_phase = phase;
        entity.weapon_cooldown_remaining = cooldown;
        let entity = world.entities.get(&id).expect("the unit exists");
        world.published_swing(entity, arch)
    }

    #[test]
    fn no_swing_is_published_outside_an_attack_cycle() {
        let (mut world, id, arch) = armed_unit();
        let gone = EntityId(9_999);
        for (phase, cooldown) in [
            (WeaponPhase::Ready, 0),
            (WeaponPhase::Ready, 5),
            (WeaponPhase::RotatingToTarget { target_id: gone }, 0),
            (WeaponPhase::Cooldown, 0),
        ] {
            assert_eq!(
                swing_in(&mut world, id, &arch, phase.clone(), cooldown),
                None,
                "{phase:?}"
            );
        }
    }

    #[test]
    fn elapsed_counts_down_the_windup_and_the_cooldown_and_stays_below_the_period() {
        let (mut world, id, arch) = armed_unit();
        let weapon = arch
            .weapon_against(MoverType::Ground, None)
            .expect("invariant: armed_unit() requires a ground weapon");
        let (dp, period) = (weapon.damage_point_ticks, weapon.period_ticks);
        let gone = EntityId(9_999);
        let swing = |elapsed_ticks| {
            Some(AttackSwing {
                elapsed_ticks,
                damage_point_ticks: dp,
                period_ticks: period,
            })
        };
        let windup = |remaining_ticks| WeaponPhase::Windup {
            target_id: gone,
            remaining_ticks,
        };
        let backswing = WeaponPhase::Backswing {
            target_id: gone,
            remaining_ticks: 1,
        };
        assert_eq!(swing_in(&mut world, id, &arch, windup(dp), 0), swing(0));
        assert_eq!(swing_in(&mut world, id, &arch, windup(1), 0), swing(dp - 1));
        assert_eq!(
            swing_in(&mut world, id, &arch, backswing.clone(), period - dp),
            swing(dp),
            "the release charges period - damage point"
        );
        assert_eq!(
            swing_in(&mut world, id, &arch, WeaponPhase::Cooldown, 1),
            swing(period - 1)
        );
        assert_eq!(
            swing_in(&mut world, id, &arch, backswing, 0),
            swing(period - 1),
            "a backswing outlasting the cooldown stays below the period"
        );
    }

    #[test]
    fn a_random_delay_holds_the_swing_at_its_release() {
        // B-3: a release that draws a positive delay charges more than `period - damage point`;
        // the published swing stays at the release instead of stepping back into the wind-up.
        let (mut world, id, arch) = armed_unit();
        let weapon = arch
            .weapon_against(MoverType::Ground, None)
            .expect("invariant: armed_unit() requires a ground weapon");
        let (dp, period) = (weapon.damage_point_ticks, weapon.period_ticks);
        let swing = |elapsed_ticks| {
            Some(AttackSwing {
                elapsed_ticks,
                damage_point_ticks: dp,
                period_ticks: period,
            })
        };
        let backswing = WeaponPhase::Backswing {
            target_id: EntityId(9_999),
            remaining_ticks: 1,
        };
        for extra in 1..=2 {
            assert_eq!(
                swing_in(
                    &mut world,
                    id,
                    &arch,
                    backswing.clone(),
                    period - dp + extra
                ),
                swing(dp),
                "{extra} ticks of delay"
            );
        }
    }

    #[test]
    fn a_swing_whose_target_is_gone_falls_back_to_the_weapon_the_archetype_has() {
        let (mut world, id, arch) = armed_unit();
        let ground = arch
            .weapon_against(MoverType::Ground, None)
            .cloned()
            .expect("invariant: armed_unit() requires a ground weapon");
        // The same body with only an anti-air weapon of its own timings.
        let air = WeaponSpec {
            period_ticks: ground.period_ticks + 7,
            damage_point_ticks: ground.damage_point_ticks + 1,
            ..ground
        };
        let air_only = UnitArchetype {
            weapon: None,
            ground_weapon: None,
            air_weapon: Some(air.clone()),
            ..arch
        };
        let swing = swing_in(
            &mut world,
            id,
            &air_only,
            WeaponPhase::Backswing {
                target_id: EntityId(9_999),
                remaining_ticks: 2,
            },
            air.period_ticks - air.damage_point_ticks,
        );
        assert_eq!(
            swing,
            Some(AttackSwing {
                elapsed_ticks: air.damage_point_ticks,
                damage_point_ticks: air.damage_point_ticks,
                period_ticks: air.period_ticks,
            })
        );
    }

    #[test]
    fn the_resolved_period_follows_the_attack_rate() {
        let (world, id, arch) = armed_unit();
        let entity = world.entities.get(&id).expect("the unit exists");
        let rate = |value| AttackRate::new(SimScalar::from_i32(value)).expect("a positive rate");
        let resolve = |rate| {
            SimWorld::resolved_weapon(
                &world.player_upgrades,
                entity,
                &arch,
                MoverType::Ground,
                rate,
            )
            .expect("the unit is armed against ground")
        };
        let plain = resolve(AttackRate::ONE);
        let doubled = resolve(rate(2));
        assert_eq!(plain.period_ticks, plain.spec.period_ticks);
        assert_eq!(doubled.period_ticks, plain.spec.period_ticks / 2);
        assert_eq!(doubled.to_spec().period_ticks, doubled.period_ticks);
        assert_eq!(doubled.to_spec().max_range, plain.max_range);
    }
}
