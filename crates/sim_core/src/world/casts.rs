//! The cast pipeline of targeted effect abilities (plan `2026-11-10-spellcaster-mechanics` D-6,
//! D-7): edge-based range, approach, target vision and filter, origin placement, the single
//! caster of a group command, the after-cast lock, and the order-level admission the client
//! shares with the tick.
//!
//! # Responsibility
//! Answers, without mutation, the questions a cast of an [`AbilityEffectSpec::Effect`] ability
//! asks: may this unit take the order ([`SimWorld::would_accept_cast_order`]), is the target in
//! range from the caster's edge, which point does the approach walk to, where does the origin
//! fall, and which unit of a group casts. Also counts down `Entity::cast_lock_ticks`.
//!
//! # How it fits
//! `abilities.rs` admits and commits a cast; it calls the range, target and origin checks
//! here, so a cast now and an order for later apply one rule. Phase 1 (`commands.rs`) orders
//! the cast and picks the single caster; movement walks a caster whose target is out of range
//! ([`SimWorld::cast_approach_goal`]) and casts once it is in range. Other abilities (mode
//! toggles, Blink, hazard zones) keep their immediate path: none of this applies to them, save
//! the lock, whose length is 0 unless authored.
//!
//! # Invariants
//! - Nothing here writes, except [`SimWorld::count_down_cast_locks`].
//! - A unit's order is accepted only if a cast could follow once the unit stands in range and
//!   is free to act: admission adds only range, origin placement and the unit's freedom to act.
//! - The single-caster pick depends only on fixed-point distances, energy and `EntityId`s.

use crate::catalog::UnitArchetype;
use crate::effects::filter::{passes_filter, unit_target};
use crate::entity::Entity;
use crate::SimWorld;
use sim_types::{
    AbilityEffectSpec, AbilitySource, AbilitySpec, AbilityTargetKind, EffectContext, EffectTarget,
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, VisibilityState,
};

impl SimWorld {
    /// Whether `entity_id` could take an order to cast the ability at `ability_index` of its
    /// archetype at `target_pos` (world units) or on `target_entity`: alive and not carried,
    /// the ability unlocked and off cooldown, the energy at hand, and the target valid, visible
    /// to the caster's owner and admitted by the ability's target filter (plan D-7.2).
    ///
    /// Range is not asked, because the caster walks into range; nor is the caster's freedom to
    /// act now (a channel, a stun or the after-cast lock), because the order waits for it. The
    /// client asks this before it submits a cast, and phase 1 asks it again before it gives
    /// the order. For an ability that is not an effect tree this is [`Self::would_admit_cast`].
    #[must_use]
    pub fn would_accept_cast_order(
        &self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> bool {
        let Some((entity, arch, ability)) = self.cast_parts(entity_id, ability_index) else {
            return false;
        };
        let AbilityEffectSpec::Effect(key) = &ability.effect else {
            return self.would_admit_cast(entity_id, ability_index, target_pos, target_entity);
        };
        entity.is_alive()
            && !entity.is_loaded_in_transport()
            && self.is_ability_unlocked(arch, entity.owner, &ability.name)
            && entity
                .ability_cooldowns
                .get(usize::from(ability_index))
                .is_none_or(|&cooldown| cooldown == 0)
            && ability.energy_cost >= SimScalar::ZERO
            && entity.energy >= ability.energy_cost
            && self.catalog.spells().effect_id(key).is_some()
            && self.cast_target_admitted(
                entity,
                ability,
                ability_index,
                (target_pos, target_entity),
            )
    }

    /// The caster, its archetype and the ability at `ability_index`, if all exist.
    fn cast_parts(
        &self,
        entity_id: EntityId,
        ability_index: u8,
    ) -> Option<(&Entity, &UnitArchetype, &AbilitySpec)> {
        let entity = self.entities.get(&entity_id)?;
        let arch = self.catalog.get(entity.unit_type_id).ok()?;
        let ability = arch.abilities.get(usize::from(ability_index))?;
        Some((entity, arch, ability))
    }

    /// Whether the ability at `ability_index` of `entity_id`'s archetype is an effect tree, the
    /// abilities this pipeline governs.
    pub(super) fn is_effect_ability(&self, entity_id: EntityId, ability_index: u8) -> bool {
        self.cast_parts(entity_id, ability_index)
            .is_some_and(|(_, _, ability)| matches!(ability.effect, AbilityEffectSpec::Effect(_)))
    }

    /// Whether the cast's target suits the ability (plan D-7.7): a point for a point ability;
    /// for a unit ability a unit that lives in the world, passes the ability's `target_filter`
    /// for the caster, and is the caster's owner's own or seen by that owner (a visible cell,
    /// and detected if concealed).
    pub(super) fn cast_target_admitted(
        &self,
        caster: &Entity,
        ability: &AbilitySpec,
        ability_index: u8,
        (target_pos, target_entity): (Option<SimVec2>, Option<EntityId>),
    ) -> bool {
        match ability.target_kind {
            AbilityTargetKind::TargetPosition => target_pos.is_some(),
            AbilityTargetKind::TargetEntity => {
                let Some(target) = target_entity.and_then(|id| unit_target(&self.entities, id))
                else {
                    return false;
                };
                let filtered = ability.target_filter.as_ref().is_none_or(|filter| {
                    self.catalog
                        .spells()
                        .resolve_filter(filter)
                        .is_some_and(|resolved| {
                            let context = cast_context(caster, ability_index, target.id);
                            passes_filter(resolved, &context, target, &self.catalog, &self.effects)
                        })
                });
                filtered && (target.owner == caster.owner || self.is_seen_by(caster.owner, target))
            }
            AbilityTargetKind::InstantSelf | AbilityTargetKind::ToggleMode => false,
        }
    }

    /// Whether `viewer` sees `target` well enough to aim a spell at it: its cell is visible to
    /// `viewer` (where `viewer` has a vision map) and, if concealed, detected.
    fn is_seen_by(&self, viewer: PlayerId, target: &Entity) -> bool {
        let visible = self.vision_maps.get(viewer).is_none_or(|vision| {
            self.map
                .world_to_cell(target.position)
                .is_ok_and(|(x, y)| vision.visibility_at(x, y) == VisibilityState::Visible)
        });
        visible && Self::is_entity_targetable(target, &self.map, &self.vision_maps, viewer)
    }

    /// Whether `caster` stands in range of the cast's target (plan D-7.1, `StarCraft` II
    /// `RangeUseCasterRadius`): a point within `cast_range` of the caster's edge, a unit's edge
    /// within `cast_range` of the caster's edge. `None` when the target is missing or gone.
    pub(super) fn cast_in_range(
        &self,
        caster: &Entity,
        ability: &AbilitySpec,
        (target_pos, target_entity): (Option<SimVec2>, Option<EntityId>),
    ) -> Option<bool> {
        let (point, target_radius) = self.cast_target_body(target_pos, target_entity)?;
        let reach = ability
            .cast_range
            .saturating_add(self.radius_of(caster))
            .saturating_add(target_radius);
        Some(crate::movement::math::within_distance(
            caster.position,
            point,
            i128::from(reach.raw()),
        ))
    }

    /// The target's centre and radius in world units: a unit's if one is given, else the point.
    fn cast_target_body(
        &self,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> Option<(SimVec2, SimScalar)> {
        match target_entity {
            Some(id) => {
                let target = unit_target(&self.entities, id)?;
                Some((target.position, self.radius_of(target)))
            }
            None => target_pos.map(|point| (point, SimScalar::ZERO)),
        }
    }

    /// The collision radius of `entity`'s archetype, 0 if it has none.
    fn radius_of(&self, entity: &Entity) -> SimScalar {
        self.catalog
            .get(entity.unit_type_id)
            .map_or(SimScalar::ZERO, |arch| arch.collision_radius)
    }

    /// The point a caster whose front order is this cast walks to, or `None` when it should try
    /// the cast now: the ability is not an effect tree, the target is gone (the cast is then
    /// refused and the order dropped), or the caster stands in range (plan D-7.1). A unit target
    /// is followed: the goal is its position this tick.
    pub(crate) fn cast_approach_goal(
        &self,
        caster: &Entity,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> Option<SimVec2> {
        let (_, _, ability) = self.cast_parts(caster.id, ability_index)?;
        if !matches!(ability.effect, AbilityEffectSpec::Effect(_)) {
            return None;
        }
        let targets = (target_pos, target_entity);
        if self.cast_in_range(caster, ability, targets)? {
            return None;
        }
        self.cast_target_body(target_pos, target_entity)
            .map(|(point, _)| point)
    }

    /// The origin of a cast's effect tree (plan D-6, D-8): without `origin` placement the
    /// caster's position; with it, the first point valid for the placed unit's movement layer
    /// (the target unit, or the caster for a point cast) among the start, `forward_offset` ahead
    /// of the caster along its heading, and the rings around it within `search_range`. `None`
    /// when no point is valid: the cast is refused.
    pub(super) fn place_cast_origin(
        &self,
        caster: &Entity,
        ability: &AbilitySpec,
        target_entity: Option<EntityId>,
    ) -> Option<SimVec2> {
        let Some(placement) = ability.origin else {
            return Some(caster.position);
        };
        let ahead = SimVec2::new(caster.heading.cos(), caster.heading.sin());
        let start = caster.position + ahead * placement.forward_offset;
        let placed = target_entity.unwrap_or(caster.id);
        self.first_valid_placement(placed, start, placement.search_range)
    }

    /// The one unit of `cmd` that casts a non-queued single-caster effect ability (plan D-7.6,
    /// `StarCraft` II `BestUnit`): among the commanded units of the player whose ability at
    /// `ability_index` is such an ability and that would accept the order, the one whose edge is
    /// nearest the target's, then the one with more energy, then the lowest `EntityId`.
    pub(super) fn pick_single_caster(
        &self,
        cmd: &PlayerCommand,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> Option<EntityId> {
        let (point, target_radius) = self.cast_target_body(target_pos, target_entity)?;
        cmd.entities
            .iter()
            .filter_map(|&id| {
                let (entity, _, ability) = self.cast_parts(id, ability_index)?;
                let single = entity.owner == cmd.player_id
                    && ability.single_caster
                    && matches!(ability.effect, AbilityEffectSpec::Effect(_))
                    && self.would_accept_cast_order(id, ability_index, target_pos, target_entity);
                single.then(|| {
                    let edge = entity
                        .position
                        .distance(point)
                        .saturating_sub(self.radius_of(entity))
                        .saturating_sub(target_radius);
                    (edge, std::cmp::Reverse(entity.energy), id)
                })
            })
            .min()
            .map(|(_, _, id)| id)
    }

    /// Counts down every unit's after-cast lock by one tick (plan D-7.4). Runs after the weapon
    /// phase, so a lock of `n` set on tick `t` holds the unit on ticks `t` to `t + n − 1`,
    /// whether the cast ran in phase 1 or in movement.
    pub(crate) fn count_down_cast_locks(&mut self) {
        for entity in self.entities.values_mut() {
            entity.cast_lock_ticks = entity.cast_lock_ticks.saturating_sub(1);
        }
    }
}

/// The context a target filter is checked with at admission: the caster's, aimed at `target`.
fn cast_context(caster: &Entity, ability_index: u8, target: EntityId) -> EffectContext {
    EffectContext {
        owner: caster.owner,
        caster: Some(caster.id),
        ability: AbilitySource {
            unit_type: caster.unit_type_id,
            ability: ability_index,
        },
        origin: caster.position,
        source_point: caster.position,
        target: EffectTarget::Unit(target),
    }
}
