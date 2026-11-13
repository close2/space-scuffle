//! # Deterministic Upgrade Effects & Player Upgrade State
//!
//! Upgrade effects and the deterministic per-player state they modify: weapon damage levels per
//! weapon class, armor levels per armor class, unit stat modifications, and research progress.
//! Weapon and armor classes are data (`assets/catalog/factions.ron`); the simulation stores only
//! their resolved [`WeaponClassId`] and [`ArmorClassId`].
//!
//! ## Invariants Maintained
//! 1. All stat deltas and bonus scaling values strictly use [`SimScalar`].
//! 2. Zero non-deterministic collections: upgrade state uses deterministic [`BTreeMap`] and [`BTreeSet`].
//! 3. Zero heap allocations during per-tick combat damage resolution.

use crate::ids::{ArmorClassId, WeaponClassId};
use crate::math::SimScalar;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Stat whose cumulative upgrade adjustment exceeded its representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpgradeStat {
    /// Maximum hull hit points.
    MaxHp,
    /// Movement speed per tick.
    MoveSpeed,
    /// Sight radius in world units.
    Sight,
    /// Weapon range in world units.
    WeaponRange,
    /// Armor points.
    Armor,
    /// Signed attack interval adjustment in ticks.
    AttackCooldown,
    /// Bonus damage per strike.
    BonusDamage,
}

/// An upgrade transaction could not represent one of its accumulated adjustments.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("upgrade adjustment overflow for '{target_slug}', stat {stat:?}")]
pub struct UpgradeApplicationError {
    /// Catalog slug receiving the overflowing adjustment.
    pub target_slug: String,
    /// Stat whose checked addition failed.
    pub stat: UpgradeStat,
}

/// Composable upgrade effect modifying weapon scaling, armor, unit stats, or abilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpgradeEffect {
    /// Advances scaling weapon level for an entire weapon class.
    WeaponDamageLevel {
        /// Weapon class receiving the level advancement, as resolved from its catalog slug.
        weapon_class: WeaponClassId,
        /// New level tier (1..=3).
        level: u8,
    },
    /// Advances life armor level for an entire armor class.
    ArmorLevel {
        /// Armor class receiving the level advancement, as resolved from its catalog slug.
        armor_class: ArmorClassId,
        /// New level tier (1..=3).
        level: u8,
    },
    /// Advances global plasma shield armor level (+1 per level).
    ShieldLevel {
        /// New level tier (1..=3).
        level: u8,
    },
    /// Modifies baseline statistics for a specific unit archetype slug.
    ModifyUnitStat {
        /// Target unit archetype slug (e.g. `unit_infantry`, `unit_walker`).
        target_slug: String,
        /// Maximum HP adjustment.
        #[serde(default)]
        max_hp_delta: SimScalar,
        /// Movement speed adjustment per tick.
        #[serde(default)]
        move_speed_delta: SimScalar,
        /// Sight radius adjustment.
        #[serde(default)]
        sight_delta: SimScalar,
        /// Weapon range adjustment.
        #[serde(default)]
        weapon_range_delta: SimScalar,
        /// Armor adjustment.
        #[serde(default)]
        armor_delta: SimScalar,
        /// Attack cooldown / period ticks adjustment (negative means faster attack rate).
        #[serde(default)]
        attack_cooldown_delta: i32,
        /// Bonus damage applied to each attack strike.
        #[serde(default)]
        bonus_damage_delta: SimScalar,
    },
    /// Unlocks an active or passive ability for a specific unit archetype slug.
    UnlockAbility {
        /// Target unit archetype slug.
        target_slug: String,
        /// Unlocked ability slug.
        ability_slug: String,
    },
    /// Runs an effect of `ability_effects.ron` on each unit the weapons of a specific unit
    /// archetype strike directly, once the owner has completed the upgrade (the game's
    /// `CValidatorPlayerRequirement` on a weapon's impact effect).
    ///
    /// Adds no per-player state: the completed upgrade slug is the unlock, read at each hit.
    WeaponHitEffect {
        /// Target unit archetype slug whose weapons gain the effect.
        target_slug: String,
        /// Effect key of `ability_effects.ron`, run with the struck unit as its target.
        effect: String,
    },
}

/// Cumulative stat modifications applied to a specific unit type for a player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UnitStatDeltas {
    /// Maximum HP adjustment.
    pub max_hp_delta: SimScalar,
    /// Movement speed adjustment per tick.
    pub move_speed_delta: SimScalar,
    /// Sight radius adjustment.
    pub sight_delta: SimScalar,
    /// Weapon range adjustment.
    pub weapon_range_delta: SimScalar,
    /// Armor adjustment.
    pub armor_delta: SimScalar,
    /// Attack cooldown adjustment.
    pub attack_cooldown_delta: i32,
    /// Bonus damage adjustment.
    pub bonus_damage_delta: SimScalar,
}

/// Authoritative upgrade state tracked per player inside simulation world.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PlayerUpgradeState {
    /// Researched weapon level (0..=3) per weapon classification.
    pub weapon_levels: BTreeMap<WeaponClassId, u8>,
    /// Researched armor level (0..=3) per armor classification.
    pub armor_levels: BTreeMap<ArmorClassId, u8>,
    /// Researched global shield armor level (0..=3).
    pub shield_level: u8,
    /// Set of completed upgrade slugs (e.g. `combat_shield`, `phase_blink`).
    pub completed_upgrade_slugs: BTreeSet<String>,
    /// Accumulated stat deltas keyed by unit slug.
    pub unit_stat_modifiers: BTreeMap<String, UnitStatDeltas>,
    /// Unlocked abilities keyed by unit slug.
    pub unlocked_abilities: BTreeMap<String, BTreeSet<String>>,
}

impl PlayerUpgradeState {
    /// Creates an empty upgrade state: no levels, no research, no modifiers.
    ///
    /// `const`, so a `static` empty state can stand in for owners without a player slot.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            weapon_levels: BTreeMap::new(),
            armor_levels: BTreeMap::new(),
            shield_level: 0,
            completed_upgrade_slugs: BTreeSet::new(),
            unit_stat_modifiers: BTreeMap::new(),
            unlocked_abilities: BTreeMap::new(),
        }
    }

    /// Atomically applies effects and completed research identity.
    ///
    /// # Errors
    /// On any cumulative stat overflow returns typed evidence without changing state.
    pub fn apply_upgrade(
        &mut self,
        upgrade_slug: &str,
        effects: &[UpgradeEffect],
    ) -> Result<(), UpgradeApplicationError> {
        // Research is infrequent; staging the complete player state keeps all
        // effects, unlocks and completion identity atomic on checked failure.
        let mut staged = self.clone();
        staged
            .completed_upgrade_slugs
            .insert(upgrade_slug.to_string());
        for effect in effects {
            match effect {
                UpgradeEffect::WeaponDamageLevel {
                    weapon_class,
                    level,
                } => {
                    staged.weapon_levels.insert(*weapon_class, *level);
                }
                UpgradeEffect::ArmorLevel { armor_class, level } => {
                    staged.armor_levels.insert(*armor_class, *level);
                }
                UpgradeEffect::ShieldLevel { level } => {
                    staged.shield_level = *level;
                }
                UpgradeEffect::ModifyUnitStat {
                    target_slug,
                    max_hp_delta,
                    move_speed_delta,
                    sight_delta,
                    weapon_range_delta,
                    armor_delta,
                    attack_cooldown_delta,
                    bonus_damage_delta,
                } => {
                    let entry = staged
                        .unit_stat_modifiers
                        .entry(target_slug.clone())
                        .or_default();
                    let overflow = |stat| UpgradeApplicationError {
                        target_slug: target_slug.clone(),
                        stat,
                    };
                    entry.max_hp_delta = entry
                        .max_hp_delta
                        .checked_add(*max_hp_delta)
                        .ok_or_else(|| overflow(UpgradeStat::MaxHp))?;
                    entry.move_speed_delta = entry
                        .move_speed_delta
                        .checked_add(*move_speed_delta)
                        .ok_or_else(|| overflow(UpgradeStat::MoveSpeed))?;
                    entry.sight_delta = entry
                        .sight_delta
                        .checked_add(*sight_delta)
                        .ok_or_else(|| overflow(UpgradeStat::Sight))?;
                    entry.weapon_range_delta = entry
                        .weapon_range_delta
                        .checked_add(*weapon_range_delta)
                        .ok_or_else(|| overflow(UpgradeStat::WeaponRange))?;
                    entry.armor_delta = entry
                        .armor_delta
                        .checked_add(*armor_delta)
                        .ok_or_else(|| overflow(UpgradeStat::Armor))?;
                    entry.attack_cooldown_delta = entry
                        .attack_cooldown_delta
                        .checked_add(*attack_cooldown_delta)
                        .ok_or_else(|| overflow(UpgradeStat::AttackCooldown))?;
                    entry.bonus_damage_delta = entry
                        .bonus_damage_delta
                        .checked_add(*bonus_damage_delta)
                        .ok_or_else(|| overflow(UpgradeStat::BonusDamage))?;
                }
                UpgradeEffect::UnlockAbility {
                    target_slug,
                    ability_slug,
                } => {
                    staged
                        .unlocked_abilities
                        .entry(target_slug.clone())
                        .or_default()
                        .insert(ability_slug.clone());
                }
                // The completed slug inserted above is the whole unlock; the hit reads it.
                UpgradeEffect::WeaponHitEffect { .. } => {}
            }
        }
        *self = staged;
        Ok(())
    }
}
