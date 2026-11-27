//! Tick phase 10: the render snapshot.
//!
//! # Responsibility
//! Writes the world's visible state into a [`RenderSnapshot`]: player status, resource nodes,
//! one bubble per entity not carried in a transport (a gatherer inside a structure is published
//! with `housed` set, so its client keeps one render entity through every gas trip; plan
//! `2026-11-24-hidden-harvesters` D-9), the catalog statuses each such bubble
//! bears, the effect engine's areas, links and spell missiles, and the tick's visual events,
//! including a `SpellImpact` per spell missile that landed during the tick. A bubble's attack
//! cycle (`AttackSwing`) is derived from the weapon state machine through the same weapon
//! resolution combat uses, with no state of its own; its concealment, stance, status and
//! barrier fraction come from `world::presentation`.
//!
//! It also builds the render record of an attack snapshot ([`snapshot_bubble`]) for the player
//! view, from the same archetype-and-position parts as a live unit's (plan
//! `2026-11-24-attacker-snapshot` D-4).
//!
//! # How it fits
//! The last phase of `step_into`, and the only place the simulation converts fixed-point values
//! to `f32` (charter §3.2); `SimWorld::write_player_view` calls [`snapshot_bubble`].
//!
//! # Invariants
//! - Writing a snapshot changes no authoritative state and no checksum; it only drains
//!   `pending_events`, the tick's barrier hits and the tick's spell-missile landings.
//! - Bubbles are ordered by entity ID, statuses by `(entity, status)`, areas by persistent id.
//! - A spell impact follows the tick's other events, in the order the impacts ran.

use super::presentation::{self, SnapshotPose};
use super::AIR_UNIT_ALTITUDE;
use crate::catalog::UnitArchetype;
use crate::entity::{Entity, WeaponPhase, OVERCLOCK_BUFF_ID};
use crate::SimWorld;
use sim_map::SimMap;
use sim_types::{
    AnimState, AreaView, AttackSwing, BubbleRenderInstance, CombatEventVisual, Concealment,
    EntityId, MoverType, PlayerSlot, PlayerStatusView, RenderSnapshot, ResourceNodeId,
    ResourceNodeView, SimScalar, SimVec2, StanceView, StatusFlags, StatusKind, StatusView, Tick,
    WeaponSpec, WeaponVisualKind,
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
        output.statuses.clear();
        output.combat_events.clear();
        output.tick = Tick(self.current_tick);
        output.checksum = self.last_checksum;
        output.match_result = self.match_state.result;
        self.write_player_views(output);
        self.write_resource_nodes(output);
        self.write_effect_views(output);

        self.presentation.settle_barrier_hits();
        for entity in self.entities.values() {
            if entity.is_loaded_in_transport() {
                continue;
            }
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };
            let swing = self.published_swing(entity, arch);
            let barrier_hit = self.presentation.barrier_hit(entity.id);
            let mut bubble = bubble_instance(entity, arch, &self.map, swing, barrier_hit);
            self.publish_statuses(entity.id, &mut bubble.status, &mut output.statuses);
            output.bubbles.push(bubble);
        }
        self.presentation.clear_barrier_hits();

        output.combat_events.append(&mut self.pending_events);
        for landed in self.presentation.drain_spell_impacts() {
            output.combat_events.push(CombatEventVisual::SpellImpact {
                pos: [
                    landed.contact.x.to_f32(),
                    landed.contact.y.to_f32(),
                    self.map.terrain_altitude_at(landed.contact),
                ],
                missile: landed.missile,
                owner: landed.owner,
                source: landed.source,
            });
        }
    }

    /// Replaces the effect engine's views (plan `2026-11-10-spellcaster-mechanics` D-19): an
    /// area per point-anchored persistent that searches, a spell missile per projectile
    /// missile, and a link per tether missile, tethered waiting persistent and tethered
    /// displacement whose caster lives. An area's id is its persistent's.
    fn write_effect_views(&self, output: &mut RenderSnapshot) {
        let spells = self.catalog.spells();
        output.areas.clear();
        output.links.clear();
        output.spell_missiles.clear();
        for persistent in self.effects.persistents() {
            let (Some(node), Some(facts)) =
                (spells.node(persistent.node), spells.facts(persistent.node))
            else {
                continue;
            };
            let crate::catalog::EffectNode::Persistent {
                period_ticks,
                count,
                ..
            } = node.kind
            else {
                continue;
            };
            match persistent.anchor {
                crate::effects::PersistentAnchor::Point(center) => {
                    let Some(radius) = facts.area_radius else {
                        continue;
                    };
                    let total = crate::effects::pulse_offset(period_ticks, count)
                        .map_or(0, |ticks| u16::try_from(ticks).unwrap_or(u16::MAX));
                    let elapsed = self.current_tick.wrapping_sub(persistent.created_tick.0);
                    let elapsed = u16::try_from(elapsed).unwrap_or(u16::MAX);
                    output.areas.push(AreaView {
                        id: persistent.id,
                        owner: persistent.context.owner,
                        center: [
                            center.x.to_f32(),
                            center.y.to_f32(),
                            self.map.terrain_altitude_at(center),
                        ],
                        radius: radius.to_f32(),
                        remaining_ticks: total.saturating_sub(elapsed),
                        total_ticks: total,
                        source: Some(persistent.context.ability),
                    });
                }
                crate::effects::PersistentAnchor::Unit(unit) => {
                    if facts.reaches_tether {
                        self.push_link(output, &persistent.context, unit);
                    }
                }
            }
        }
        for missile in self.effects.missiles() {
            let Some(crate::catalog::EffectNode::LaunchMissile { view, .. }) =
                spells.node(missile.node).map(|node| node.kind)
            else {
                continue;
            };
            match (view, missile.target) {
                (sim_types::MissileView::Projectile, _) => {
                    output.spell_missiles.push(sim_types::SpellMissileView {
                        id: missile.id,
                        owner: missile.context.owner,
                        position: [
                            missile.position.x.to_f32(),
                            missile.position.y.to_f32(),
                            self.map.terrain_altitude_at(missile.position),
                        ],
                        source: missile.context.ability,
                    });
                }
                (sim_types::MissileView::Tether, sim_types::EffectTarget::Unit(unit)) => {
                    self.push_link(output, &missile.context, unit);
                }
                (sim_types::MissileView::Tether, sim_types::EffectTarget::Point(_)) => {}
            }
        }
        for (id, entity) in &self.entities {
            let Some(flight) = entity.displacement else {
                continue;
            };
            if let Some(crate::catalog::EffectNode::Displace { tether: true, .. }) =
                spells.node(flight.node).map(|node| node.kind)
            {
                self.push_link(output, &flight.context, *id);
            }
        }
    }

    /// Adds a link from the context's caster to `to`, when the caster lives.
    fn push_link(
        &self,
        output: &mut RenderSnapshot,
        context: &sim_types::EffectContext,
        to: EntityId,
    ) {
        let Some(caster) = context
            .caster
            .filter(|id| self.entities.get(id).is_some_and(Entity::is_alive))
        else {
            return;
        };
        output.links.push(sim_types::LinkView {
            from: caster,
            to,
            owner: context.owner,
            source: context.ability,
        });
    }

    /// Raises the presentation flags of every status `entity` carries and appends each one's
    /// identity to `statuses` (plan `2026-11-17-effect-snapshot-identity` D-5).
    ///
    /// The effect engine keeps its statuses sorted by `(entity, status)` and unique, and the
    /// writer calls this in entity order, so `statuses` stays sorted with no sort of its own.
    fn publish_statuses(
        &self,
        entity: EntityId,
        flags: &mut StatusFlags,
        statuses: &mut Vec<StatusView>,
    ) {
        let spells = self.catalog.spells();
        for active in self.effects.statuses_of(entity) {
            let Some(status) = spells.status(active.status) else {
                continue;
            };
            statuses.push(StatusView {
                entity,
                status: active.status,
            });
            for kind in &status.spec.presentation {
                match kind {
                    StatusKind::Overclock => flags.overclock = true,
                    StatusKind::Dash => flags.dash = true,
                    StatusKind::Slowed => flags.slowed = true,
                    StatusKind::Barrier => flags.barrier_up = true,
                    StatusKind::DamageOverTime => flags.damage_over_time = true,
                    StatusKind::Stunned => flags.stunned = true,
                    StatusKind::RangeReduced => flags.range_reduced = true,
                    StatusKind::Revealed => flags.revealed = true,
                }
            }
        }
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

/// `value` as a share of `max`, unclamped; zero when `max` is not positive.
pub(crate) fn fraction(value: SimScalar, max: SimScalar) -> SimScalar {
    if max > SimScalar::ZERO {
        value / max
    } else {
        SimScalar::ZERO
    }
}

/// The parts of a render record that depend only on the archetype `arch`, the weapon a mode puts
/// in place of the archetype's (`mode_weapon`) and the position `position` on `map`: the record
/// of a live unit ([`bubble_instance`]) and of an attack snapshot ([`snapshot_bubble`]) share
/// them, so an image looks like its attacker did (plan `2026-11-24-attacker-snapshot` D-4).
struct Placed {
    /// `[x, y, altitude]` in world units: an air unit flies [`AIR_UNIT_ALTITUDE`] above the
    /// ground.
    world_pos: [f32; 3],
    /// Terrain height under the position, world units.
    ground_z: f32,
    /// The drawn radius, not the range edge (plan `2026-11-18-structure-radius` D-6).
    radius: f32,
    /// The visual of the mode's weapon, else of the archetype's ground or generic weapon.
    weapon_visual: WeaponVisualKind,
}

impl Placed {
    fn new(
        arch: &UnitArchetype,
        mode_weapon: Option<&WeaponSpec>,
        position: SimVec2,
        map: &SimMap,
    ) -> Self {
        let ground_z = map.terrain_altitude_at(position);
        let altitude_z = if matches!(arch.mover_type, MoverType::Air) {
            ground_z + AIR_UNIT_ALTITUDE
        } else {
            ground_z
        };
        Self {
            world_pos: [position.x.to_f32(), position.y.to_f32(), altitude_z],
            ground_z,
            radius: arch.drawn_radius().to_f32(),
            weapon_visual: mode_weapon
                .map(|w| w.visual_kind)
                .or_else(|| arch.ground_weapon.as_ref().map(|w| w.visual_kind))
                .or_else(|| arch.weapon.as_ref().map(|w| w.visual_kind))
                .unwrap_or(WeaponVisualKind::None),
        }
    }
}

/// A share in fixed point as the render record carries it, clamped to `0..=1`.
fn published_share(share: SimScalar) -> f32 {
    share.to_f32().clamp(0.0, 1.0)
}

/// On a foundation, its health as a share of the health its construction has built so far,
/// capped at 1 ([`crate::production::construction::Construction::built_hp`]); `None` on an
/// entity not under construction. The dome's damage look reads it, so that missing build
/// progress does not read as damage (B-210).
fn construction_hp_ratio(entity: &Entity) -> Option<f32> {
    entity
        .construction
        .as_ref()
        .map(|record| published_share(fraction(entity.hp, record.built_hp(entity.max_hp))))
}

/// The render record of one entity of archetype `arch` standing on `map`, in the attack cycle
/// `attack_swing` ([`SimWorld::published_swing`]); `barrier_hit` says whether its barrier
/// absorbed damage this tick.
fn bubble_instance(
    entity: &Entity,
    arch: &UnitArchetype,
    map: &SimMap,
    attack_swing: Option<AttackSwing>,
    barrier_hit: bool,
) -> BubbleRenderInstance {
    let placed = Placed::new(arch, entity.siege_weapon.as_deref(), entity.position, map);
    let is_attacking = matches!(
        entity.weapon_phase,
        WeaponPhase::Windup { .. } | WeaponPhase::Backswing { .. }
    );
    let shield_flash = (f32::from(entity.shield_hit_flash_ticks)) / 8.0;
    let hull_flash = (f32::from(entity.hull_hit_flash_ticks)) / 8.0;
    let speed_boosted = entity
        .active_buffs
        .iter()
        .any(|b| b.buff_id == OVERCLOCK_BUFF_ID);

    BubbleRenderInstance {
        entity_id: entity.id,
        shape: arch.shape,
        silhouette: arch.silhouette,
        unit_type: entity.unit_type_id,
        owner: entity.owner,
        world_pos: placed.world_pos,
        ground_z: placed.ground_z,
        radius: placed.radius,
        velocity: [entity.velocity.x.to_f32(), entity.velocity.y.to_f32()],
        heading_angle_rad: entity.heading.to_radians_f32(),
        aim_angle_rad: entity.turret_angle.to_radians_f32(),
        is_attacking,
        weapon_visual: placed.weapon_visual,
        attack_target_pos: None,
        anim_state: entity.current_anim,
        attack_swing,
        hp_ratio: published_share(fraction(entity.hp, entity.max_hp)),
        construction_hp_ratio: construction_hp_ratio(entity),
        shield_ratio: published_share(fraction(entity.shields, arch.max_shields)),
        shield_hit_flash: shield_flash,
        hull_hit_angle_rad: entity.last_hit_angle.to_radians_f32(),
        hull_hit_flash: hull_flash,
        build_or_queue_progress: SimWorld::published_progress(entity),
        selected: false,
        concealment: presentation::concealment(entity, arch),
        stance: presentation::stance(entity, arch),
        status: presentation::status(entity, barrier_hit),
        barrier_ratio: presentation::barrier_ratio(entity, arch),
        speed_boosted,
        housed: entity.is_housed(),
    }
}

/// A foundation's build progress and its construction health share, both in `0..=1` and both
/// exactly as the live render record publishes them; `None` on an entity not under construction
/// (plan `2026-12-01-minimap` Amendment 1, D-16). A producing structure's queue is never part of
/// it.
pub(crate) fn foundation_state(entity: &Entity) -> Option<(SimScalar, SimScalar)> {
    let record = entity.construction.as_ref()?;
    let done = record.total_ticks.saturating_sub(record.remaining_ticks);
    let progress = u64::from(done) * (1 << 16) / u64::from(record.total_ticks.max(1));
    let progress = SimScalar::from_raw(i64::try_from(progress).unwrap_or(0) << 16);
    let share = fraction(entity.hp, record.built_hp(entity.max_hp));
    Some((progress, share))
}

/// The render record of the image of `attacker` in `pose`, of archetype `arch`, on `map`: the
/// entity as it stood when the viewer last saw it, still (zero velocity, idle, no swing,
/// unselected, unconcealed, not housed, no flashes, no statuses) in the settled stance of its
/// mode (plan `2026-11-24-attacker-snapshot` D-4). `arch` is looked up by the pose's type, so a
/// dead entity's image is built as well. `construction`, a remembered foundation's progress and
/// health share ([`foundation_state`]), shows it `Constructing` at that progress (plan
/// `2026-12-01-minimap` D-16); an attack snapshot passes `None`.
pub(crate) fn snapshot_bubble(
    attacker: EntityId,
    pose: &SnapshotPose,
    arch: &UnitArchetype,
    map: &SimMap,
    construction: Option<(SimScalar, SimScalar)>,
) -> BubbleRenderInstance {
    let mode_weapon = pose
        .mode
        .and_then(|index| arch.modes.get(usize::from(index)))
        .and_then(|mode| mode.ground_weapon_override.as_ref())
        .and_then(Option::as_ref);
    let placed = Placed::new(arch, mode_weapon, pose.position, map);
    BubbleRenderInstance {
        entity_id: attacker,
        shape: arch.shape,
        silhouette: arch.silhouette,
        unit_type: pose.unit_type,
        owner: pose.owner,
        world_pos: placed.world_pos,
        ground_z: placed.ground_z,
        radius: placed.radius,
        velocity: [0.0, 0.0],
        heading_angle_rad: pose.heading.to_radians_f32(),
        aim_angle_rad: pose.turret.to_radians_f32(),
        is_attacking: false,
        weapon_visual: placed.weapon_visual,
        attack_target_pos: None,
        anim_state: if construction.is_some() {
            AnimState::Constructing
        } else {
            AnimState::Idle
        },
        attack_swing: None,
        hp_ratio: published_share(pose.hp_fraction),
        shield_ratio: published_share(pose.shield_fraction),
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: construction.map(|(progress, _)| progress.to_f32()),
        selected: false,
        concealment: Concealment::None,
        stance: StanceView {
            mode: pose.mode,
            transition: None,
        },
        status: StatusFlags::default(),
        construction_hp_ratio: construction.map(|(_, share)| published_share(share)),
        barrier_ratio: 0.0,
        speed_boosted: false,
        housed: false,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::weapon_timing::AttackRate;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{CombatEventVisual, RenderSnapshot, Tick, WeaponVisualKind};
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

    /// Scratch spell tables: a point missile presented as a projectile whose impact marks every
    /// unit within 2 with a long status. Appended to the shipped tables, as `sim_fixtures`
    /// does, under names of their own.
    fn install_scratch_marker(world: &mut SimWorld) -> sim_types::EffectId {
        use crate::catalog::SpellCatalog;
        let mut statuses =
            SpellCatalog::parse_statuses(include_str!("../../../../assets/catalog/statuses.ron"))
                .expect("invariant: the shipped statuses.ron parses");
        let mut effects = SpellCatalog::parse_effects(include_str!(
            "../../../../assets/catalog/ability_effects.ron"
        ))
        .expect("invariant: the shipped ability_effects.ron parses");
        statuses.extend(
            SpellCatalog::parse_statuses(
                r#"(statuses: [(name: "scratch_mark", duration_ticks: 500.0,
                    presentation: [DamageOverTime])])"#,
            )
            .expect("valid scratch statuses"),
        );
        effects.extend(
            SpellCatalog::parse_effects(
                r#"(effects: [
                    (name: "scratch_bolt", node: LaunchMissile((to: TargetPoint, speed: 0.5,
                        view: Projectile, impact: Use("scratch_bolt_search")))),
                    (name: "scratch_bolt_search", node: Search((radius: 2.0,
                        child: ApplyStatus((status: "scratch_mark"))))),
                ])"#,
            )
            .expect("valid scratch effects"),
        );
        let spells = SpellCatalog::from_defs(statuses, effects).expect("a valid merged catalog");
        world
            .catalog
            .set_spell_catalog(spells)
            .expect("the shipped abilities resolve in the merged tables");
        world
            .catalog
            .spells()
            .effect_id("scratch_bolt")
            .expect("installed")
    }

    /// D-8 of plan `2026-11-17-effect-snapshot-identity`: in a world holding statuses, a landing
    /// spell missile and instant hits, the checksum after the snapshot is written equals the one
    /// the tick computed before writing it, on every tick; and the frames did carry each fact.
    #[test]
    fn writing_a_snapshot_changes_no_checksum() {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat 32x32 map is valid");
        let mut world = crate::test_support::world(map, 11);
        let root = install_scratch_marker(&mut world);
        let shooter_type = world
            .catalog
            .archetypes()
            .find(|arch| {
                matches!(arch.shape, ShapeKind::UnitSphere)
                    && arch
                        .weapon_against(MoverType::Ground, None)
                        .is_some_and(|w| {
                            w.projectile_speed.is_none()
                                && w.visual_kind == WeaponVisualKind::Hitscan
                                && w.max_range >= SimScalar::from_i32(2)
                        })
            })
            .map(|arch| arch.unit_type_id)
            .expect("the shipped catalog has a unit with an instant ground weapon");
        let at = |x: i32, y: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
        for (owner, x) in [(0, 16), (1, 18)] {
            world
                .spawn_unit(shooter_type, PlayerId(owner), at(x, 16))
                .expect("the unit spawns");
        }
        let launch = at(16, 24);
        world.run_effect(
            root,
            sim_types::EffectContext {
                owner: PlayerId(0),
                caster: None,
                ability: sim_types::AbilitySource::default(),
                origin: launch,
                source_point: launch,
                target: sim_types::EffectTarget::Point(at(17, 16)),
            },
        );
        let mut bundle = sim_types::TickBundle {
            tick: Tick(0),
            commands: Vec::new(),
            state_checksum: None,
        };
        let mut output = RenderSnapshot::default();
        let (mut statuses, mut impacts, mut hits) = (0, 0, 0);
        for _ in 0..40 {
            bundle.tick = Tick(world.current_tick);
            world.step_into(&bundle, &mut output);
            assert_eq!(
                world.compute_checksum(),
                output.checksum,
                "tick {}: writing the snapshot moved the checksum",
                output.tick.0
            );
            statuses += output.statuses.len();
            for event in &output.combat_events {
                match event {
                    CombatEventVisual::SpellImpact { .. } => impacts += 1,
                    CombatEventVisual::HitscanTracer { ends, .. }
                    | CombatEventVisual::BeamVisual { ends, .. }
                    | CombatEventVisual::MeleeSlashVisual { ends, .. } => {
                        assert!(ends.is_some(), "{event:?}");
                        hits += 1;
                    }
                    _ => {}
                }
            }
            let before = world.compute_checksum();
            let mut again = RenderSnapshot::default();
            world.write_render_snapshot(&mut again);
            assert_eq!(
                world.compute_checksum(),
                before,
                "a second write moves nothing"
            );
            assert_eq!(
                again.statuses, output.statuses,
                "the statuses are state, not events"
            );
        }
        assert_eq!(impacts, 1, "the missile landed once");
        assert!(
            statuses > 0,
            "the marked units were published with their status"
        );
        assert!(hits > 0, "the units traded instant hits");
    }

    /// R-13 (plan `2026-11-18-structure-radius` D-6): a 3×3 dome whose radius is 1.8125
    /// publishes the drawn radius 1.5, half its footprint, while a unit publishes its collision
    /// radius.
    #[test]
    fn a_dome_is_drawn_at_its_footprint_not_its_radius() {
        let (mut world, unit, unit_arch) = armed_unit();
        let dome = world
            .catalog
            .archetypes()
            .find(|arch| {
                matches!(
                    arch.shape,
                    ShapeKind::BuildingDome {
                        footprint_w: 3,
                        footprint_h: 3,
                        ..
                    }
                )
            })
            .map(|arch| arch.unit_type_id)
            .expect("the shipped catalog has a 3x3 structure");
        world
            .catalog
            .get_mut(dome)
            .expect("the dome's archetype")
            .collision_radius = SimScalar::from_num(1.8125);
        let structure = world
            .spawn_structure(dome, PlayerId(1), 4, 4)
            .expect("open ground");
        let snapshot = world.build_render_snapshot();
        let radius_of = |id: EntityId| {
            snapshot
                .bubbles
                .iter()
                .find(|bubble| bubble.entity_id == id)
                .map(|bubble| bubble.radius)
                .expect("published")
        };
        assert!((radius_of(structure) - 1.5).abs() < f32::EPSILON);
        assert!(
            (radius_of(unit) - unit_arch.collision_radius.to_f32()).abs() < f32::EPSILON,
            "a unit is drawn at its collision radius"
        );
    }

    /// Plan `2026-11-24-attacker-snapshot` D-4: the image of an idle, unflashed, unselected
    /// entity is its live render record, field for field: the two records share every part
    /// that depends on the archetype, the mode and the position.
    #[test]
    fn a_snapshot_of_an_idle_unit_is_its_live_record() {
        let (world, id, arch) = armed_unit();
        let entity = &world.entities[&id];
        assert_eq!(entity.current_anim, AnimState::Idle, "premise");
        let live = bubble_instance(entity, &arch, &world.map, None, false);
        let pose = crate::combat_resolution::snapshot_pose(entity, &arch);
        assert_eq!(snapshot_bubble(id, &pose, &arch, &world.map, None), live);
    }

    /// D-4: in a settled mode whose weapon replaces the archetype's, the image keeps the mode's
    /// stance and weapon visual, as the live record does.
    #[test]
    fn a_snapshot_in_a_mode_keeps_the_modes_stance_and_weapon() {
        use sim_types::AbilityEffectSpec;
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat 32x32 map is valid");
        let mut world = crate::test_support::world(map, 9);
        let (arch, mode_name) = world
            .catalog
            .archetypes()
            .find_map(|arch| {
                arch.modes
                    .iter()
                    .find(|mode| {
                        !mode.is_cloaked && matches!(mode.ground_weapon_override, Some(Some(_)))
                    })
                    .map(|mode| (arch.clone(), mode.mode_name.clone()))
            })
            .expect("the shipped catalog has a mode that replaces its unit's ground weapon");
        let id = world
            .spawn_unit(arch.unit_type_id, PlayerId(0), SimVec2::from_i32(16, 16))
            .expect("the unit spawns");
        let effect = AbilityEffectSpec::SwitchMode { mode_name };
        let entity = world.entities.get_mut(&id).expect("the unit exists");
        entity.apply_mode_transition(&arch, &effect);
        let entity = &world.entities[&id];
        let live = bubble_instance(entity, &arch, &world.map, None, false);
        assert!(live.stance.mode.is_some(), "premise: in the mode");
        let pose = crate::combat_resolution::snapshot_pose(entity, &arch);
        assert_eq!(snapshot_bubble(id, &pose, &arch, &world.map, None), live);
    }

    /// P-9 (plan `2026-11-24-attacker-snapshot` §5): no tick decision reads the attack
    /// snapshots. Two worlds run the same battle, in which a ranged attacker on a plateau
    /// strikes a unit below it unseen; one of them forgets every snapshot after each tick.
    /// Their checksums agree on every tick, and the other world did keep a snapshot.
    #[test]
    fn forgetting_attack_snapshots_changes_no_checksum() {
        use sim_types::{AttackVisibility, TickBundle, UnitOrder};
        let build = || {
            let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
                .expect("invariant: a flat map");
            for y in 0..32 {
                for x in 16..32 {
                    map.cell_mut(x, y)
                        .expect("invariant: on the map")
                        .cliff_level = CliffLevel(1);
                }
            }
            let mut world = crate::test_support::world(map, 0x183);
            let kind = world
                .catalog
                .archetypes()
                .filter(|arch| {
                    matches!(arch.shape, ShapeKind::UnitSphere)
                        && arch.mover_type == MoverType::Ground
                })
                .find(|arch| {
                    arch.weapon_against(MoverType::Ground, None)
                        .is_some_and(|w| {
                            w.attack_visibility == AttackVisibility::Snapshot
                                && w.max_range >= SimScalar::from_i32(4)
                        })
                })
                .map(|arch| arch.unit_type_id)
                .expect("the shipped catalog has a ranged Snapshot attacker");
            let at = |x: f64| SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(16.5));
            let attacker = world
                .spawn_unit(kind, PlayerId(0), at(16.5))
                .expect("invariant: a free spot");
            let victim = world
                .spawn_unit(kind, PlayerId(1), at(13.5))
                .expect("invariant: a free spot");
            world
                .entities
                .get_mut(&attacker)
                .expect("spawned")
                .issue_order(UnitOrder::attack(victim), false)
                .expect("invariant: an attack order is admitted");
            world
        };
        let (mut keeping, mut forgetting) = (build(), build());
        let mut kept = 0;
        for tick in 0..200 {
            let bundle = TickBundle {
                tick: Tick(tick),
                commands: Vec::new(),
                state_checksum: None,
            };
            keeping.step(&bundle);
            forgetting.step(&bundle);
            forgetting.presentation.forget_attack_snapshots();
            kept += keeping
                .presentation
                .attack_snapshots_of(PlayerId(1))
                .count();
            assert_eq!(
                keeping.last_checksum, forgetting.last_checksum,
                "tick {tick}"
            );
        }
        assert!(kept > 0, "premise: the keeping world recorded snapshots");
    }

    /// R-8 (plan `2026-12-01-minimap` Amendment 1): no tick decision reads the remembered
    /// structures. Two worlds run the same scouting: a unit of player 0 beside a structure of
    /// player 1 walks away out of sight; one world forgets every remembered structure after each
    /// tick. Their checksums agree on every tick over 400, and the other world did remember.
    #[test]
    fn forgetting_remembered_structures_changes_no_checksum() {
        use sim_types::{TickBundle, UnitOrder};
        let build = || {
            let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))
                .expect("invariant: a flat map");
            let mut world = crate::test_support::world(map, 0x252);
            let dome = world
                .catalog
                .archetypes()
                .find(|arch| {
                    matches!(arch.shape, ShapeKind::BuildingDome { footprint_w, .. } if footprint_w <= 3)
                        && !arch.capabilities.is_gas_extractor
                })
                .map(|arch| arch.unit_type_id)
                .expect("the shipped catalog has a small structure");
            let walker = world
                .catalog
                .archetypes()
                .find(|arch| {
                    matches!(arch.shape, ShapeKind::UnitSphere)
                        && arch.mover_type == MoverType::Ground
                        && arch.max_speed > SimScalar::ZERO
                        && !arch.is_detector()
                })
                .map(|arch| arch.unit_type_id)
                .expect("the shipped catalog has a mobile ground unit");
            world
                .spawn_structure(dome, PlayerId(1), 30, 30)
                .expect("invariant: open ground");
            let scout = world
                .spawn_unit(walker, PlayerId(0), SimVec2::from_i32(31, 26))
                .expect("invariant: a free spot");
            world
                .entities
                .get_mut(&scout)
                .expect("spawned")
                .issue_order(
                    UnitOrder::Move {
                        target: SimVec2::from_i32(31, 2),
                    },
                    false,
                )
                .expect("invariant: a move is admitted");
            world
        };
        let (mut keeping, mut forgetting) = (build(), build());
        let mut kept = 0;
        for tick in 0..400 {
            let bundle = TickBundle {
                tick: Tick(tick),
                commands: Vec::new(),
                state_checksum: None,
            };
            keeping.step(&bundle);
            forgetting.step(&bundle);
            forgetting.presentation.forget_remembered_structures();
            kept += keeping
                .presentation
                .remembered_structures_of(PlayerId(0))
                .count();
            assert_eq!(
                keeping.last_checksum, forgetting.last_checksum,
                "tick {tick}"
            );
        }
        assert!(
            kept > 0,
            "premise: the keeping world remembered the structure"
        );
        let mut view = sim_types::PlayerView::default();
        keeping.write_player_view(PlayerId(0), &mut view);
        assert_eq!(
            view.remembered_structures.len(),
            1,
            "out of sight, remembered"
        );
    }

    /// The world of V-22: a plateau 32 × 32 (x >= 16 one level up, no ramp); a ground missile
    /// attacker of player 0 on the plateau's edge ordered to attack a unit of player 1 below it;
    /// a storm caster and a Fungal caster of player 0 on the edge, each with a unit of player 1
    /// below it; and the casts to start: each spell's root, its caster and its target point.
    fn launch_and_spell_world() -> (SimWorld, Vec<(sim_types::EffectId, EntityId, SimVec2)>) {
        use sim_types::{AbilityEffectSpec, AttackVisibility, UnitOrder};
        let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat map");
        for y in 0..32 {
            for x in 16..32 {
                map.cell_mut(x, y)
                    .expect("invariant: on the map")
                    .cliff_level = CliffLevel(1);
            }
        }
        let mut world = crate::test_support::world(map, 0x1a7);
        let missile = world
            .catalog
            .archetypes()
            .filter(|arch| {
                matches!(arch.shape, ShapeKind::UnitSphere) && arch.mover_type == MoverType::Ground
            })
            .find(|arch| {
                arch.weapon_against(MoverType::Ground, None)
                    .is_some_and(|w| {
                        w.projectile_speed.is_some()
                            && w.attack_visibility == AttackVisibility::Snapshot
                            && w.max_range >= SimScalar::from_i32(4)
                    })
            })
            .map(|arch| arch.unit_type_id)
            .expect("the shipped catalog has a ranged ground missile");
        let caster_of = |world: &SimWorld, key: &str| {
            world
                .catalog
                .archetypes()
                .find(|arch| {
                    arch.abilities.iter().any(|ability| {
                        matches!(&ability.effect, AbilityEffectSpec::Effect(k) if k == key)
                    })
                })
                .map(|arch| arch.unit_type_id)
                .expect("the shipped catalog has the spell's caster")
        };
        let at = |x: f64, y: f64| SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y));
        let attacker = world
            .spawn_unit(missile, PlayerId(0), at(16.5, 16.5))
            .expect("invariant: a free spot");
        let victim = world
            .spawn_unit(missile, PlayerId(1), at(13.5, 16.5))
            .expect("invariant: a free spot");
        world
            .entities
            .get_mut(&attacker)
            .expect("spawned")
            .issue_order(UnitOrder::attack(victim), false)
            .expect("invariant: an attack order is admitted");
        let mut casts = Vec::new();
        for (key, row) in [("psionic_storm", 6.5), ("fungal_growth", 26.5)] {
            let kind = caster_of(&world, key);
            let caster = world
                .spawn_unit(kind, PlayerId(0), at(16.5, row))
                .expect("invariant: a free spot");
            world
                .spawn_unit(missile, PlayerId(1), at(13.5, row))
                .expect("invariant: a free spot");
            let root = world
                .catalog
                .spells()
                .effect_id(key)
                .expect("the shipped spell");
            casts.push((root, caster, at(13.5, row)));
        }
        (world, casts)
    }

    /// V-22 (plan `2026-11-27-launch-and-spell-visibility` §5): P-9 with the new causes. Two
    /// worlds run a missile duel, a storm and a Fungal Growth cast from a plateau at units below
    /// it, unseen; one forgets every image after each tick. Their checksums agree on every tick,
    /// and the keeping world did keep images of the duel's launches and of both casters.
    #[test]
    fn forgetting_launch_and_spell_images_changes_no_checksum() {
        use sim_types::{AbilitySource, EffectContext, EffectTarget, TickBundle};
        let ((mut keeping, casts), (mut forgetting, _)) =
            (launch_and_spell_world(), launch_and_spell_world());
        let mut kept = std::collections::BTreeSet::new();
        for tick in 0..200 {
            if tick % 100 == 1 {
                for world in [&mut keeping, &mut forgetting] {
                    for &(root, caster, point) in &casts {
                        let position = world.entities[&caster].position;
                        world.run_effect(
                            root,
                            EffectContext {
                                owner: PlayerId(0),
                                caster: Some(caster),
                                ability: AbilitySource::default(),
                                origin: position,
                                source_point: position,
                                target: EffectTarget::Point(point),
                            },
                        );
                    }
                }
            }
            let bundle = TickBundle {
                tick: Tick(tick),
                commands: Vec::new(),
                state_checksum: None,
            };
            keeping.step(&bundle);
            forgetting.step(&bundle);
            forgetting.presentation.forget_attack_snapshots();
            kept.extend(
                keeping
                    .presentation
                    .attack_snapshots_of(PlayerId(1))
                    .map(|record| record.attacker),
            );
            assert_eq!(
                keeping.last_checksum, forgetting.last_checksum,
                "tick {tick}"
            );
        }
        assert!(
            casts.iter().all(|(_, caster, _)| kept.contains(caster)),
            "premise: the keeping world kept images of both casters, got {kept:?}"
        );
    }
}
