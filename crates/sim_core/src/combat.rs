//! # Combat Systems, Weapon State Machine & Damage Resolution
//!
//! Enforces `StarCraft` II-parity weapon state machines, angular turret tracking,
//! target acquisition ranking, exact shield spillover damage formulas, and splash falloff.
//!
//! ## Invariants Maintained
//! 1. All calculations use deterministic fixed-point math with zero floating-point operations.
//! 2. Target ranking is strictly deterministic: `(threat DESC, attack target priority DESC,
//!    primary weapon DESC, tie-break ASC, id ASC)`.
//! 3. Minimum damage is clamped to 0.5 HP for non-zero attacks.
//! 4. Shield and life regeneration wait for the archetype's catalog delays
//!    (`shield_regen_delay_ticks`, `hp_regen_delay_ticks`) after sustaining damage; any damage,
//!    to shields or life, restarts both.

use crate::catalog::{AcquireLevel, SplashSpec, UnitArchetype, WeaponSpec};
use crate::entity::{Entity, WeaponPhase};
use sim_types::{
    AttackOrigin, EntityId, MoverType, PlayerUpgradeState, SimAngle, SimScalar, SimVec2, UnitOrder,
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

/// Extra reach a swing keeps at the moment of release, in world units.
///
/// An attacker stands still through its wind-up while its target keeps walking. Without this
/// allowance a chaser would cancel every swing at a fleeing unit, one step out of range.
/// `StarCraft` II gives its weapons the same half-cell `RangeSlop`.
pub const RELEASE_RANGE_SLOP: SimScalar = SimScalar::HALF;

/// Minimum damage clamp applied to any penetrating attack in `StarCraft` II.
pub const MIN_DAMAGE_CLAMP: SimScalar = SimScalar::HALF;

/// Raw damage of one strike before falloff and armor, in hit points.
///
/// Sums the base damage, weapon-level scaling, unit-specific stat bonus and attribute bonus.
/// The per-level attribute growth is granted once, and only when `spec` carries an authored
/// bonus against an attribute the target actually has.
fn raw_strike_damage(
    spec: &WeaponSpec,
    attacker_slug: Option<&str>,
    attacker_upgrades: &PlayerUpgradeState,
    target_archetype: &UnitArchetype,
) -> SimScalar {
    let weapon_lvl = spec
        .weapon_class
        .and_then(|wc| attacker_upgrades.weapon_levels.get(&wc).copied())
        .unwrap_or(0);
    let level = SimScalar::from_i32(i32::from(weapon_lvl));
    let stat_bonus_dmg = attacker_slug
        .and_then(|slug| attacker_upgrades.unit_stat_modifiers.get(slug))
        .map_or(SimScalar::ZERO, |m| m.bonus_damage_delta);

    // Every attribute bonus the target matches applies, each with its upgrade (B-14).
    let mut raw_damage = spec.base_damage + spec.upgrade_bonus_per_level * level + stat_bonus_dmg;
    for bonus in spec.attribute_bonuses.against(target_archetype.attributes) {
        raw_damage += bonus + spec.bonus_upgrade_per_level * level;
    }
    raw_damage
}

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
    mitigate_strike(
        raw_strike_damage(
            attacker_spec,
            attacker_slug,
            attacker_upgrades,
            target_archetype,
        ),
        target_archetype,
        target_upgrades,
        target_shields,
        target_hp,
    )
}

/// Applies shield armor, spillover, life armor and the minimum-damage clamp to `raw_damage`.
fn mitigate_strike(
    raw_damage: SimScalar,
    target_archetype: &UnitArchetype,
    target_upgrades: &PlayerUpgradeState,
    target_shields: SimScalar,
    target_hp: SimScalar,
) -> DamageResolution {
    let half = MIN_DAMAGE_CLAMP;

    let mut applied_shield_damage = SimScalar::ZERO;
    let mut applied_life_damage = SimScalar::ZERO;

    if target_shields > SimScalar::ZERO {
        // Shield mitigation: base shield armor + shield_level upgrade
        let effective_shield_armor = target_archetype.shield_armor
            + SimScalar::from_i32(i32::from(target_upgrades.shield_level));
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

            let effective_life_armor = target_archetype.life_armor
                + SimScalar::from_i32(i32::from(armor_lvl))
                + stat_armor;

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
            target_archetype.life_armor + SimScalar::from_i32(i32::from(armor_lvl)) + stat_armor;

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
/// Implements `StarCraft` II armor mitigation and shield spillover with default (unupgraded) player state:
/// 1. Computes raw damage including the weapon's bonus against each attribute the target has.
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

    // PERFORMANCE RATIONALE:
    // Why: Splash runs once per entity in the blast, and a scaled `WeaponSpec` clone allocates.
    // What: Scale the summed raw damage, which also covers the unit-specific stat bonus.
    // Trade-off: None for readability: one multiplication replaces one per damage component.
    Some(mitigate_strike(
        raw_strike_damage(
            attacker_spec,
            attacker_slug,
            attacker_upgrades,
            target_archetype,
        ) * damage_multiplier,
        target_archetype,
        target_upgrades,
        target_shields,
        target_hp,
    ))
}

/// Whether area damage centred on a `primary` entity reaches an `other` entity.
///
/// Ground and air are separate planes. A [`MoverType::CliffWalker`] stands in both, so it
/// shares a plane with everything.
#[must_use]
pub const fn shares_combat_plane(primary: MoverType, other: MoverType) -> bool {
    match (primary, other) {
        (MoverType::CliffWalker, _)
        | (_, MoverType::CliffWalker)
        | (MoverType::Air, MoverType::Air) => true,
        (MoverType::Air, _) | (_, MoverType::Air) => false,
        _ => true,
    }
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

/// Key used to deterministically sort target acquisition candidates; the smaller key is the
/// better target.
///
/// Order preference, as `StarCraft` II's automatic targeting (Liquipedia "Automatic
/// Targeting"):
/// 1. `threat` (a threat first; see [`is_threat_to`])
/// 2. `attack_target_priority` (higher first; `CUnit` `AttackTargetPriority`)
/// 3. `primary` (a target for the unit's primary weapon first; `CUnit` `WeaponArray` order)
/// 4. `tie_break` (smaller first): the archetype's tie-break measure (`CWeapon`
///    `AcquirePrioritization`, see [`crate::catalog::AcquireTieBreak`]). It decides only when the
///    unit chooses anew; a kept target is never compared on it.
/// 5. `entity_id` (lower first). Not a game rule: it only decides exact ties, deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetPriorityKey {
    /// Whether the candidate threatens the unit choosing.
    pub threat: bool,
    /// The candidate archetype's attack target priority.
    pub attack_target_priority: u8,
    /// Whether the unit's primary weapon can be used against the candidate.
    pub primary: bool,
    /// The tie-break measure, smaller first: the squared distance to the candidate
    /// (`Closest`), the turn of the firing facing towards it in binary angle units
    /// (`LeastTurning`), or the squared distance from the previous target's last position to
    /// it (`ClosestToPrevious`; the squared distance to the candidate while there is none).
    pub tie_break: SimScalar,
    /// Entity ID tie-breaker.
    pub entity_id: EntityId,
}

impl TargetPriorityKey {
    /// The criteria that let a candidate displace a target the unit keeps: threat, attack
    /// target priority, then the primary weapon (Liquipedia "Automatic Targeting": "a more
    /// important target (according to the first 3 criteria)"). A candidate replaces a kept
    /// target only if this is strictly greater.
    #[must_use]
    pub const fn rank(&self) -> (bool, u8, bool) {
        (self.threat, self.attack_target_priority, self.primary)
    }
}

impl Ord for TargetPriorityKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .rank()
            .cmp(&self.rank())
            .then_with(|| self.tie_break.cmp(&other.tie_break))
            .then_with(|| self.entity_id.0.cmp(&other.entity_id.0))
    }
}

impl PartialOrd for TargetPriorityKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Whether `candidate`, an entity of archetype `candidate_archetype`, threatens a unit of
/// archetype `chooser_archetype` (criterion 1 of `StarCraft` II's automatic targeting).
///
/// A candidate is a threat if its archetype is flagged always a threat (`AlwaysThreatens`), or
/// if it has a weapon for which the chooser is a legal target: a weapon against the chooser's
/// mover type whose target flags meet the chooser's attributes. A candidate whose acquire level
/// is [`AcquireLevel::Defensive`] (the workers') threatens only while it is attack-moving
/// (Liquipedia "Automatic Targeting", Threats and Notes). Range and the candidate's order
/// otherwise play no part.
#[must_use]
pub fn is_threat_to(
    candidate_archetype: &UnitArchetype,
    candidate: &Entity,
    chooser_archetype: &UnitArchetype,
) -> bool {
    if candidate_archetype.always_a_threat {
        return true;
    }
    if candidate_archetype.acquire_level == AcquireLevel::Defensive && !is_attack_moving(candidate)
    {
        return false;
    }
    candidate_archetype
        .weapon_against(
            chooser_archetype.mover_type,
            candidate.active_siege_weapon(),
        )
        .is_some_and(|weapon| weapon.target_flags.intersects(chooser_archetype.attributes))
}

/// Whether `entity` carries out an attack-move: its front order is one, or it is an automatic
/// attack that acquisition put in front of one.
fn is_attack_moving(entity: &Entity) -> bool {
    let mut orders = entity.order_queue.iter();
    match orders.next() {
        Some(UnitOrder::AttackMove { .. }) => true,
        Some(UnitOrder::AttackTarget {
            origin: AttackOrigin::Acquired,
            ..
        }) => matches!(orders.next(), Some(UnitOrder::AttackMove { .. })),
        _ => false,
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

/// The enemy a weapon cycle is working against, resolved by the combat phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponTarget {
    /// Target entity handle.
    pub id: EntityId,
    /// Target centre in world coordinates.
    pub position: SimVec2,
    /// Attacker and target collision radii combined, in world units.
    pub combined_radius: SimScalar,
}

/// Rotates the weapon mount towards `angle`: a turret by at most its own rate, a hull-mounted
/// weapon by turning the hull within what is left of this tick's hull budget
/// ([`Entity::turn_hull_towards`]).
fn slew_towards(entity: &mut Entity, weapon: &WeaponSpec, angle: SimAngle) {
    if weapon.has_turret {
        let step = weapon.turret_turn_rate.raw();
        entity.turret_angle = entity.turret_angle.rotate_towards(angle, step);
    } else {
        entity.turn_hull_towards(angle);
    }
}

/// Current aim of the weapon mount: the turret's angle for a turreted weapon, else the hull's.
pub(crate) fn weapon_aim(entity: &Entity, weapon: &WeaponSpec) -> SimAngle {
    if weapon.has_turret {
        entity.turret_angle
    } else {
        entity.heading
    }
}

/// Whether `distance` lies in the engagement band, with `slop` extra reach at the far end.
fn in_engagement_band(
    distance: SimScalar,
    weapon: &WeaponSpec,
    combined_radius: SimScalar,
    slop: SimScalar,
) -> bool {
    let closest = if weapon.min_range > SimScalar::ZERO {
        weapon.min_range + combined_radius
    } else {
        SimScalar::ZERO
    };
    distance <= weapon.max_range + combined_radius + slop && distance >= closest
}

/// Releases the swing against `target`. Returns true when the shot discharges.
///
/// The release is the one place a shot leaves the weapon: a target that left the engagement band
/// plus [`RELEASE_RANGE_SLOP`] cancels the swing (phase `Ready`, no cooldown charged); otherwise
/// the cooldown is charged with `period − damage point`, so that the cycle from the start of
/// the swing to the start of the next one is `period_ticks`, and the phase becomes `Backswing`
/// (or `Cooldown` for a weapon without one).
fn release_swing(entity: &mut Entity, weapon: &WeaponSpec, target: WeaponTarget) -> bool {
    let distance = entity.position.distance(target.position);
    if !in_engagement_band(distance, weapon, target.combined_radius, RELEASE_RANGE_SLOP) {
        entity.weapon_phase = WeaponPhase::Ready;
        return false;
    }
    entity.weapon_cooldown_remaining = weapon
        .period_ticks
        .saturating_sub(weapon.damage_point_ticks);
    entity.weapon_phase = if weapon.backswing_ticks > 0 {
        WeaponPhase::Backswing {
            target_id: target.id,
            remaining_ticks: weapon.backswing_ticks,
        }
    } else {
        WeaponPhase::Cooldown
    };
    true
}

/// Starts a swing against `target`. Returns true when the shot discharges in this call.
///
/// A damage point of 0 means the damage lands on the tick the attack starts, as in `StarCraft`
/// II, so such a swing releases at once and never shows a `Windup` (plan
/// `2026-11-04-combat-sc2-alignment` D-3, Q0036). Any other swing winds up for
/// `damage_point_ticks` ticks and releases in [`step_windup`]. Either way the release falls
/// `damage_point_ticks` after the start, which is what keeps the cycle at `period_ticks`.
fn start_swing(entity: &mut Entity, weapon: &WeaponSpec, target: WeaponTarget) -> bool {
    if weapon.damage_point_ticks == 0 {
        return release_swing(entity, weapon, target);
    }
    entity.weapon_phase = WeaponPhase::Windup {
        target_id: target.id,
        remaining_ticks: weapon.damage_point_ticks,
    };
    false
}

/// Steps a ready weapon: starts the swing if aimed, or begins rotating and starts the swing on
/// this same tick if the turn aligns it ([`slew_and_try_swing`]). Returns true when the shot
/// discharges in this call (a swing with damage point 0, [`start_swing`]).
fn step_ready_weapon(entity: &mut Entity, weapon: &WeaponSpec, target: WeaponTarget) -> bool {
    let distance = entity.position.distance(target.position);
    if !in_engagement_band(distance, weapon, target.combined_radius, SimScalar::ZERO) {
        return false;
    }
    let delta = target.position - entity.position;
    let target_angle = SimAngle::atan2(delta.y, delta.x);
    let aim = weapon_aim(entity, weapon);
    if is_within_firing_arc(aim, entity.position, target.position, weapon.firing_arc) {
        // An unrestricted arc removes the aiming gate, not the aiming: the turret
        // or hull keeps slewing, so a firing weapon never stays pointed away.
        if weapon.firing_arc >= SimAngle::DEG_180 {
            slew_towards(entity, weapon, target_angle);
        }
        // CRITICAL COOLDOWN GUARD:
        // An entity may align with a target while its weapon is on cooldown,
        // but MUST NOT start a swing until cooldown has fully elapsed!
        entity.weapon_cooldown_remaining == 0 && start_swing(entity, weapon, target)
    } else {
        entity.weapon_phase = WeaponPhase::RotatingToTarget {
            target_id: target.id,
        };
        slew_and_try_swing(entity, weapon, target, target_angle)
    }
}

/// Turns the weapon mount towards `target_angle`, then starts the swing at once if that turn
/// brought `target` inside the firing arc and the cooldown has run out.
///
/// This is the one place a rotating weapon checks its alignment, shared by a weapon that starts
/// rotating this tick ([`step_ready_weapon`]) and one that was already rotating
/// ([`step_rotating_weapon`]). `StarCraft` II turns and attacks in one motion: the swing starts
/// on the game loop the target enters the arc (`CWeapon` `Arc`/`ArcSlop`), with no extra loop
/// after the turn (remediation plan §4.2 M-22; plan `2026-11-05-sc2-deviation-fixes` WP1, B-1).
/// An aligned weapon still cooling down returns to `Ready` to wait. Returns true when the shot
/// discharges in this call (a swing with damage point 0, [`start_swing`]).
fn slew_and_try_swing(
    entity: &mut Entity,
    weapon: &WeaponSpec,
    target: WeaponTarget,
    target_angle: SimAngle,
) -> bool {
    slew_towards(entity, weapon, target_angle);
    let aim = weapon_aim(entity, weapon);
    if is_within_firing_arc(aim, entity.position, target.position, weapon.firing_arc) {
        if entity.weapon_cooldown_remaining == 0 {
            return start_swing(entity, weapon, target);
        }
        entity.weapon_phase = WeaponPhase::Ready;
    }
    false
}

/// Steps a rotating weapon until it is aligned, or returns it to ready if the target left.
/// Returns true when the shot discharges in this call (a swing with damage point 0 that starts
/// on the call it is aligned, [`start_swing`]).
fn step_rotating_weapon(entity: &mut Entity, weapon: &WeaponSpec, target: WeaponTarget) -> bool {
    let distance = entity.position.distance(target.position);
    if !in_engagement_band(distance, weapon, target.combined_radius, SimScalar::ZERO) {
        entity.weapon_phase = WeaponPhase::Ready;
        return false;
    }
    let delta = target.position - entity.position;
    slew_and_try_swing(entity, weapon, target, SimAngle::atan2(delta.y, delta.x))
}

/// Steps a swing bound to `bound`. Returns true when the shot discharges.
///
/// `target_opt` must be `bound` or absent. A swing without its target, or whose target left
/// the engagement band, is cancelled: no shot, and no cooldown is charged. A wind-up releases
/// on the step that brings `remaining_ticks` to 0 ([`release_swing`]). The rule never creates
/// a `Windup` with 0 ticks left (a damage point of 0 releases in [`start_swing`]); one set
/// from outside still releases on its next step.
fn step_windup(
    entity: &mut Entity,
    weapon: &WeaponSpec,
    target_opt: Option<WeaponTarget>,
    bound: EntityId,
    remaining_ticks: u16,
) -> bool {
    let Some(target) = target_opt.filter(|target| target.id == bound) else {
        entity.weapon_phase = WeaponPhase::Ready;
        return false;
    };
    if weapon.has_turret {
        let delta = target.position - entity.position;
        slew_towards(entity, weapon, SimAngle::atan2(delta.y, delta.x));
    }

    let remaining_ticks = remaining_ticks.saturating_sub(1);
    if remaining_ticks > 0 {
        entity.weapon_phase = WeaponPhase::Windup {
            target_id: bound,
            remaining_ticks,
        };
        return false;
    }
    release_swing(entity, weapon, target)
}

/// Advances the weapon state machine for an entity for a single tick.
///
/// `target_opt` is the available enemy this weapon should work against; during a swing the
/// caller passes the swing's bound target. Returns true when a shot discharges this tick.
/// A carried actor returns false without changing any field, including cooldown.
///
/// The attack cycle is `weapon.period_ticks` for every damage point: a swing that starts on
/// tick T releases on T + `damage_point_ticks` (on T itself for a damage point of 0) and charges
/// `period − damage point` ticks of cooldown, so the next swing can start on T + `period_ticks`.
pub fn step_entity_weapon(
    entity: &mut Entity,
    weapon: &WeaponSpec,
    target_opt: Option<WeaponTarget>,
) -> bool {
    // Valid cargo has no weapon participation; even cooldown/phase remain frozen.
    if entity.is_loaded_in_transport() {
        return false;
    }
    // Decrement weapon cooldown if running
    entity.weapon_cooldown_remaining = entity.weapon_cooldown_remaining.saturating_sub(1);

    // If cooldown has elapsed, promote Cooldown phase to Ready immediately
    // so that target acquisition and windup can begin without a 1-tick latency.
    if entity.weapon_phase == WeaponPhase::Cooldown && entity.weapon_cooldown_remaining == 0 {
        entity.weapon_phase = WeaponPhase::Ready;
    }

    match entity.weapon_phase {
        WeaponPhase::Ready => {
            target_opt.is_some_and(|target| step_ready_weapon(entity, weapon, target))
        }
        WeaponPhase::RotatingToTarget { .. } => {
            if let Some(target) = target_opt {
                step_rotating_weapon(entity, weapon, target)
            } else {
                entity.weapon_phase = WeaponPhase::Ready;
                false
            }
        }
        WeaponPhase::Windup {
            target_id,
            remaining_ticks,
        } => step_windup(entity, weapon, target_opt, target_id, remaining_ticks),
        WeaponPhase::Backswing {
            target_id,
            remaining_ticks,
        } => step_backswing(entity, weapon, target_opt, target_id, remaining_ticks),
        WeaponPhase::Cooldown => false,
    }
}

/// Steps a backswing: a lock on orders that never delays the next attack.
///
/// The backswing loses one tick. While the cooldown runs, it is only the follow-through of the
/// last shot: it ends in `Cooldown`, or carries on. Once the cooldown has run out, the weapon
/// steps exactly as a `Ready` weapon would this tick. If that starts a swing or turns the
/// weapon towards `target_opt`, the backswing has ended; if nothing starts (no target, or the
/// target is out of the engagement band), the backswing runs on to its end as before, so the
/// unit stays anchored and presented as attacking for its full follow-through.
///
/// This matches `StarCraft` II, where the cooldown runs from the start of the attack in
/// parallel with the backswing, so the next attack is never delayed by it (plan
/// `2026-11-04-combat-sc2-alignment` D-2, Q0038). `bound` is the target of the swing whose
/// follow-through this is; `remaining_ticks` is the backswing left before this tick. Returns
/// true when the shot discharges in this call: a swing with damage point 0 that starts out of
/// the backswing releases at once (D-3).
fn step_backswing(
    entity: &mut Entity,
    weapon: &WeaponSpec,
    target_opt: Option<WeaponTarget>,
    bound: EntityId,
    remaining_ticks: u16,
) -> bool {
    let remaining_ticks = remaining_ticks.saturating_sub(1);
    if entity.weapon_cooldown_remaining == 0 {
        if let Some(target) = target_opt {
            entity.weapon_phase = WeaponPhase::Ready;
            let released = step_ready_weapon(entity, weapon, target);
            if entity.weapon_phase != WeaponPhase::Ready {
                return released;
            }
        }
    }
    entity.weapon_phase = match (remaining_ticks, entity.weapon_cooldown_remaining) {
        (0, 0) => WeaponPhase::Ready,
        (0, _) => WeaponPhase::Cooldown,
        (remaining_ticks, _) => WeaponPhase::Backswing {
            target_id: bound,
            remaining_ticks,
        },
    };
    false
}
