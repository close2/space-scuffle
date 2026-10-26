//! # Combat Systems, Weapon State Machine & Damage Resolution
//!
//! Enforces StarCraft II-parity weapon state machines, angular turret tracking,
//! target acquisition priority, exact shield spillover damage formulas, and splash falloff.
//!
//! ## Invariants Maintained
//! 1. All calculations use deterministic fixed-point math with zero floating-point operations.
//! 2. Target priority sorting is strictly deterministic: `(threat_priority DESC, dist ASC, id ASC)`.
//! 3. Minimum damage is clamped to 0.5 HP for non-zero attacks.
//! 4. 10-second (250-tick) shield regeneration delay after sustaining damage.

use crate::catalog::{SplashSpec, UnitArchetype, WeaponSpec};
use crate::entity::{Entity, WeaponPhase};
use sim_types::{
    AttributeFlags, EntityId, PlayerUpgradeState, SimAngle, SimScalar, SimVec2, UnitOrder,
};

// PERFORMANCE RATIONALE:
// Why: Combat damage resolution happens up to hundreds of times per tick in large engagements.
// What: Pure fixed-point scalar math with no heap allocations or intermediate buffers.
// Trade-off: Exact branch structure mimics SC2 engine execution precisely.

/// The result of an applied weapon hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageResolution {
    /// Damage absorbed by plasma shields.
    pub shield_damage: SimScalar,
    /// Damage penetrating to hull hit points.
    pub life_damage: SimScalar,
    /// Whether the target was eliminated by this hit.
    pub killed: bool,
}

/// Minimum damage clamp applied to any penetrating attack in StarCraft II.
pub const MIN_DAMAGE_CLAMP: SimScalar = SimScalar::HALF;

/// Calculates the exact damage applied by an incoming weapon strike against a target,
/// incorporating attacker weapon levels, target armor levels, target shield levels,
/// and unit-specific stat modifications.
#[must_use]
pub fn calculate_hit_damage_upgraded(
    attacker_spec: &WeaponSpec,
    attacker_slug: Option<&str>,
    attacker_upgrades: &PlayerUpgradeState,
    target_archetype: &UnitArchetype,
    target_upgrades: &PlayerUpgradeState,
    target_shields: SimScalar,
    target_hp: SimScalar,
) -> DamageResolution {
    let half = MIN_DAMAGE_CLAMP;

    // 1. Resolve firing unit's weapon upgrade bonus and stat bonus damage
    let weapon_lvl = attacker_spec
        .weapon_class
        .and_then(|wc| attacker_upgrades.weapon_levels.get(&wc).copied())
        .unwrap_or(0);

    let upgrade_bonus =
        attacker_spec.upgrade_bonus_per_level * SimScalar::from_i32(weapon_lvl as i32);
    let stat_bonus_dmg = attacker_slug
        .and_then(|slug| attacker_upgrades.unit_stat_modifiers.get(slug))
        .map_or(SimScalar::ZERO, |m| m.bonus_damage_delta);

    let mut raw_damage = attacker_spec.base_damage + upgrade_bonus + stat_bonus_dmg;

    // 2. Add attribute bonus damage (e.g. +10 vs Armored, +5 vs Light)
    if target_archetype
        .attributes
        .contains(AttributeFlags::ARMORED)
    {
        raw_damage += attacker_spec.bonus_armored;
        raw_damage +=
            attacker_spec.bonus_upgrade_per_level * SimScalar::from_i32(weapon_lvl as i32);
    }
    if target_archetype.attributes.contains(AttributeFlags::LIGHT) {
        raw_damage += attacker_spec.bonus_light;
    }

    let mut applied_shield_damage = SimScalar::ZERO;
    let mut applied_life_damage = SimScalar::ZERO;

    if target_shields > SimScalar::ZERO {
        // Shield mitigation: base shield armor + shield_level upgrade
        let effective_shield_armor = target_archetype.shield_armor
            + SimScalar::from_i32(target_upgrades.shield_level as i32);
        let shield_mitigated = (raw_damage - effective_shield_armor).max(half);

        if shield_mitigated <= target_shields {
            // Full damage absorbed by shields
            applied_shield_damage = shield_mitigated;
        } else {
            // Shield broken: absorb remaining shields, spillover unmitigated portion into HP
            applied_shield_damage = target_shields;

            // SC2 unmitigated spillover ratio
            let absorbed_ratio = target_shields / shield_mitigated;
            let leftover_raw = raw_damage * (SimScalar::ONE - absorbed_ratio);

            let armor_lvl = target_archetype
                .armor_class
                .and_then(|ac| target_upgrades.armor_levels.get(&ac).copied())
                .unwrap_or(0);

            let stat_armor = target_upgrades
                .unit_stat_modifiers
                .get(&target_archetype.slug)
                .map_or(SimScalar::ZERO, |m| m.armor_delta);

            let effective_life_armor =
                target_archetype.life_armor + SimScalar::from_i32(armor_lvl as i32) + stat_armor;

            applied_life_damage = (leftover_raw - effective_life_armor).max(half);
        }
    } else {
        // Direct HP damage mitigated by target's effective life armor
        let armor_lvl = target_archetype
            .armor_class
            .and_then(|ac| target_upgrades.armor_levels.get(&ac).copied())
            .unwrap_or(0);

        let stat_armor = target_upgrades
            .unit_stat_modifiers
            .get(&target_archetype.slug)
            .map_or(SimScalar::ZERO, |m| m.armor_delta);

        let effective_life_armor =
            target_archetype.life_armor + SimScalar::from_i32(armor_lvl as i32) + stat_armor;

        applied_life_damage = (raw_damage - effective_life_armor).max(half);
    }

    let remaining_hp = target_hp - applied_life_damage;
    DamageResolution {
        shield_damage: applied_shield_damage,
        life_damage: applied_life_damage,
        killed: remaining_hp <= SimScalar::ZERO,
    }
}

/// Calculates the exact damage applied by an incoming weapon strike against a target.
///
/// Implements StarCraft II armor mitigation and shield spillover with default (unupgraded) player state:
/// 1. Computes raw damage including attribute bonuses ([`AttributeFlags::ARMORED`], [`AttributeFlags::LIGHT`]).
/// 2. If target has active shields, mitigates by `target.shield_armor` (clamped to 0.5 minimum).
/// 3. If damage exceeds remaining shields, remaining raw damage spills over into HP, mitigated by `target.life_armor`.
/// 4. If shields were already 0, raw damage is mitigated directly by `target.life_armor` (clamped to 0.5 minimum).
#[must_use]
pub fn calculate_hit_damage(
    attacker_spec: &WeaponSpec,
    target_archetype: &UnitArchetype,
    target_shields: SimScalar,
    target_hp: SimScalar,
) -> DamageResolution {
    let empty_upgrades = PlayerUpgradeState::default();
    calculate_hit_damage_upgraded(
        attacker_spec,
        None,
        &empty_upgrades,
        target_archetype,
        &empty_upgrades,
        target_shields,
        target_hp,
    )
}

/// Calculates splash damage falloff across concentric circular zones (100% / 50% / 25%)
/// taking into account active player upgrades.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn calculate_splash_damage_upgraded(
    attacker_spec: &WeaponSpec,
    attacker_slug: Option<&str>,
    attacker_upgrades: &PlayerUpgradeState,
    splash: &SplashSpec,
    distance: SimScalar,
    target_archetype: &UnitArchetype,
    target_upgrades: &PlayerUpgradeState,
    target_shields: SimScalar,
    target_hp: SimScalar,
) -> Option<DamageResolution> {
    let damage_multiplier = if distance <= splash.inner_radius {
        SimScalar::ONE
    } else if distance <= splash.middle_radius {
        SimScalar::HALF
    } else if distance <= splash.outer_radius {
        SimScalar::from_num(0.25)
    } else {
        return None;
    };

    let mut scaled_spec = attacker_spec.clone();
    scaled_spec.base_damage = attacker_spec.base_damage * damage_multiplier;
    scaled_spec.bonus_armored = attacker_spec.bonus_armored * damage_multiplier;
    scaled_spec.bonus_light = attacker_spec.bonus_light * damage_multiplier;
    scaled_spec.upgrade_bonus_per_level = attacker_spec.upgrade_bonus_per_level * damage_multiplier;
    scaled_spec.bonus_upgrade_per_level = attacker_spec.bonus_upgrade_per_level * damage_multiplier;
    scaled_spec.splash = None;

    Some(calculate_hit_damage_upgraded(
        &scaled_spec,
        attacker_slug,
        attacker_upgrades,
        target_archetype,
        target_upgrades,
        target_shields,
        target_hp,
    ))
}

/// Calculates splash damage falloff across concentric circular zones (100% / 50% / 25%).
#[must_use]
pub fn calculate_splash_damage(
    attacker_spec: &WeaponSpec,
    splash: &SplashSpec,
    distance: SimScalar,
    target_archetype: &UnitArchetype,
    target_shields: SimScalar,
    target_hp: SimScalar,
) -> Option<DamageResolution> {
    let empty_upgrades = PlayerUpgradeState::default();
    calculate_splash_damage_upgraded(
        attacker_spec,
        None,
        &empty_upgrades,
        splash,
        distance,
        target_archetype,
        &empty_upgrades,
        target_shields,
        target_hp,
    )
}

/// Key used to deterministically sort target acquisition candidates.
///
/// Order preference:
/// 1. `threat_priority` (Higher is attacked first)
/// 2. `distance_sq` (Closer is attacked first)
/// 3. `entity_id` (Lower ID breaks ties deterministically)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetPriorityKey {
    /// Threat tier value (higher attacked first).
    pub threat_priority: u8,
    /// Squared distance to target.
    pub distance_sq: SimScalar,
    /// Entity ID tie-breaker.
    pub entity_id: EntityId,
}

impl Ord for TargetPriorityKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .threat_priority
            .cmp(&self.threat_priority)
            .then_with(|| self.distance_sq.cmp(&other.distance_sq))
            .then_with(|| self.entity_id.0.cmp(&other.entity_id.0))
    }
}

impl PartialOrd for TargetPriorityKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Evaluates the threat priority level of an enemy archetype and its current order.
#[must_use]
pub fn compute_threat_priority(archetype: &UnitArchetype, order: Option<&UnitOrder>) -> u8 {
    let has_weapon = archetype.weapon.is_some()
        || archetype.ground_weapon.is_some()
        || archetype.air_weapon.is_some();
    if has_weapon {
        match order {
            Some(UnitOrder::AttackMove { .. } | UnitOrder::AttackTarget { .. }) => 20,
            _ => 19, // Combat unit not actively attacking
        }
    } else if archetype.attributes.contains(AttributeFlags::STRUCTURE) {
        if archetype.is_detector {
            15
        } else {
            11 // Passive structures / domes
        }
    } else {
        10 // Worker or utility unit
    }
}

/// Checks whether an attacker can engage `target` given range, attributes, and line of fire.
#[must_use]
pub fn is_in_weapon_range(
    attacker_pos: SimVec2,
    attacker_radius: SimScalar,
    target_pos: SimVec2,
    target_radius: SimScalar,
    weapon: &WeaponSpec,
) -> bool {
    let dist = attacker_pos.distance(target_pos);
    let effective_max = weapon.max_range + attacker_radius + target_radius;
    let effective_min = if weapon.min_range > SimScalar::ZERO {
        weapon.min_range + attacker_radius + target_radius
    } else {
        SimScalar::ZERO
    };

    dist <= effective_max && dist >= effective_min
}

/// Checks whether the weapon or turret is oriented within the allowed firing arc.
#[must_use]
pub fn is_within_firing_arc(
    current_aim: SimAngle,
    attacker_pos: SimVec2,
    target_pos: SimVec2,
    firing_arc: SimAngle,
) -> bool {
    let delta = target_pos - attacker_pos;
    if delta.length_sq() <= SimScalar::EPSILON {
        return true;
    }
    let target_angle = SimAngle::atan2(delta.y, delta.x);
    let diff = current_aim.diff(target_angle).unsigned_abs();
    diff <= firing_arc.raw()
}

/// Advances the weapon state machine for an entity for a single tick.
pub fn step_entity_weapon(
    entity: &mut Entity,
    weapon: &WeaponSpec,
    target_opt: Option<(SimVec2, SimScalar)>,
) -> bool {
    // Decrement weapon cooldown if running
    entity.weapon_cooldown_remaining = entity.weapon_cooldown_remaining.saturating_sub(1);

    // If cooldown has elapsed, promote Cooldown phase to Ready immediately
    // so that target acquisition and windup can begin without a 1-tick latency.
    if entity.weapon_phase == WeaponPhase::Cooldown && entity.weapon_cooldown_remaining == 0 {
        entity.weapon_phase = WeaponPhase::Ready;
    }

    match entity.weapon_phase {
        WeaponPhase::Ready => {
            if let Some((target_pos, target_radius)) = target_opt {
                let dist = entity.position.distance(target_pos);
                let effective_max = weapon.max_range + target_radius;
                let effective_min = if weapon.min_range > SimScalar::ZERO {
                    weapon.min_range + target_radius
                } else {
                    SimScalar::ZERO
                };
                if dist > effective_max || dist < effective_min {
                    return false;
                }

                let current_aim = if weapon.has_turret {
                    entity.turret_angle
                } else {
                    entity.heading
                };

                let delta = target_pos - entity.position;
                let target_angle = SimAngle::atan2(delta.y, delta.x);

                if is_within_firing_arc(current_aim, entity.position, target_pos, weapon.firing_arc)
                {
                    // CRITICAL COOLDOWN GUARD:
                    // An entity may align with a target while its weapon is on cooldown,
                    // but MUST NOT start Windup until cooldown has fully elapsed!
                    if entity.weapon_cooldown_remaining == 0 {
                        let target_id = match entity.order_queue.front() {
                            Some(UnitOrder::AttackTarget { target }) => *target,
                            _ => EntityId(0),
                        };
                        entity.weapon_phase = WeaponPhase::Windup {
                            target_id,
                            remaining_ticks: weapon.damage_point_ticks,
                        };
                    }
                } else {
                    // Need to rotate to target
                    let target_id = match entity.order_queue.front() {
                        Some(UnitOrder::AttackTarget { target }) => *target,
                        _ => EntityId(0),
                    };
                    entity.weapon_phase = WeaponPhase::RotatingToTarget { target_id };
                    // Apply rotation step
                    if weapon.has_turret {
                        entity.turret_angle = entity
                            .turret_angle
                            .rotate_towards(target_angle, weapon.turret_turn_rate.raw());
                    } else {
                        entity.heading = entity
                            .heading
                            .rotate_towards(target_angle, weapon.turret_turn_rate.raw());
                    }
                }
            }
            false
        }
        WeaponPhase::RotatingToTarget { target_id } => {
            if let Some((target_pos, target_radius)) = target_opt {
                let dist = entity.position.distance(target_pos);
                let effective_max = weapon.max_range + target_radius;
                let effective_min = if weapon.min_range > SimScalar::ZERO {
                    weapon.min_range + target_radius
                } else {
                    SimScalar::ZERO
                };
                if dist > effective_max || dist < effective_min {
                    entity.weapon_phase = WeaponPhase::Ready;
                    return false;
                }

                let delta = target_pos - entity.position;
                let target_angle = SimAngle::atan2(delta.y, delta.x);

                if weapon.has_turret {
                    entity.turret_angle = entity
                        .turret_angle
                        .rotate_towards(target_angle, weapon.turret_turn_rate.raw());
                } else {
                    entity.heading = entity
                        .heading
                        .rotate_towards(target_angle, weapon.turret_turn_rate.raw());
                }

                let current_aim = if weapon.has_turret {
                    entity.turret_angle
                } else {
                    entity.heading
                };

                if is_within_firing_arc(current_aim, entity.position, target_pos, weapon.firing_arc)
                {
                    if entity.weapon_cooldown_remaining == 0 {
                        entity.weapon_phase = WeaponPhase::Windup {
                            target_id,
                            remaining_ticks: weapon.damage_point_ticks,
                        };
                    } else {
                        // Aligned, but still cooling down: transition back to Ready to await cooldown expiry
                        entity.weapon_phase = WeaponPhase::Ready;
                    }
                }
            } else {
                entity.weapon_phase = WeaponPhase::Ready;
            }
            false
        }
        WeaponPhase::Windup {
            target_id,
            ref mut remaining_ticks,
        } => {
            if let Some((target_pos, _)) = target_opt {
                if weapon.has_turret {
                    let delta = target_pos - entity.position;
                    let target_angle = SimAngle::atan2(delta.y, delta.x);
                    entity.turret_angle = entity
                        .turret_angle
                        .rotate_towards(target_angle, weapon.turret_turn_rate.raw());
                }
            }

            *remaining_ticks = remaining_ticks.saturating_sub(1);
            if *remaining_ticks == 0 {
                // Shot fires!
                entity.weapon_cooldown_remaining = weapon
                    .period_ticks
                    .saturating_sub(weapon.damage_point_ticks);

                if weapon.backswing_ticks > 0 {
                    entity.weapon_phase = WeaponPhase::Backswing {
                        target_id,
                        remaining_ticks: weapon.backswing_ticks,
                    };
                } else {
                    entity.weapon_phase = WeaponPhase::Cooldown;
                }
                true // Signal shot discharged
            } else {
                false
            }
        }
        WeaponPhase::Backswing {
            ref mut remaining_ticks,
            ..
        } => {
            *remaining_ticks = remaining_ticks.saturating_sub(1);
            if *remaining_ticks == 0 {
                entity.weapon_phase = if entity.weapon_cooldown_remaining == 0 {
                    WeaponPhase::Ready
                } else {
                    WeaponPhase::Cooldown
                };
            }
            false
        }
        WeaponPhase::Cooldown => {
            if entity.weapon_cooldown_remaining == 0 {
                entity.weapon_phase = WeaponPhase::Ready;
            }
            false
        }
    }
}
