//! Tick phase 10: the render snapshot.
//!
//! # Responsibility
//! Writes the world's visible state into a [`RenderSnapshot`]: player status, resource nodes,
//! one bubble per entity not carried in a transport, and the tick's visual events.
//!
//! # How it fits
//! The last phase of `step_into`, and the only place the simulation converts fixed-point values
//! to `f32` (charter §3.2).
//!
//! # Invariants
//! - Writing a snapshot changes no authoritative state and no checksum; it only drains
//!   `pending_events`.
//! - Bubbles are ordered by entity ID.

use crate::entity::{Entity, WeaponPhase};
use crate::SimWorld;
use sim_types::{
    BubbleRenderInstance, PlayerStatusView, RenderSnapshot, ResourceNodeId, ResourceNodeView,
    SimScalar, Tick, WeaponVisualKind,
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
        for (slot, view) in output.players.iter_mut().enumerate() {
            let resources = &self.player_resources[slot];
            *view = PlayerStatusView {
                minerals: resources.minerals,
                gas: resources.gas,
                supply_used: resources.supply_used,
                supply_max: resources.supply_max,
                defeated: self.match_state.defeated & (1 << slot) != 0,
            };
        }
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

        for entity in self.entities.values() {
            if entity.is_loaded_in_transport() {
                continue;
            }
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };

            let ground_z = self.map.terrain_altitude_at(entity.position);
            let altitude_z = if matches!(arch.mover_type, sim_types::MoverType::Air) {
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

            output.bubbles.push(BubbleRenderInstance {
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
                anim_progress: entity.anim_progress.to_f32(),
                hp_ratio,
                shield_ratio,
                shield_hit_flash: shield_flash,
                hull_hit_angle_rad: entity.last_hit_angle.to_radians_f32(),
                hull_hit_flash: hull_flash,
                build_or_queue_progress: Self::published_progress(entity),
                selected: false,
                cloaked: entity.is_cloaked,
                speed_boosted,
            });
        }

        output.combat_events.append(&mut self.pending_events);
    }
}
