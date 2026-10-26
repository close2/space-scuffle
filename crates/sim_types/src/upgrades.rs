//! # Deterministic Upgrade Matrix, Weapon/Armor Classes & Player Upgrade State
//!
//! Strongly-typed enums and deterministic state structures defining weapon damage scaling,
//! armor mitigation levels, unit stat modifications, and research progress per player.
//!
//! ## Invariants Maintained
//! 1. All stat deltas and bonus scaling values strictly use [`SimScalar`].
//! 2. Zero non-deterministic collections: upgrade state uses deterministic [`BTreeMap`] and [`BTreeSet`].
//! 3. Zero heap allocations during per-tick combat damage resolution.

use crate::math::SimScalar;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// High-level weapon classification determining which scaling upgrade applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum WeaponClass {
    /// Directorate infantry ballistic weapons (Marine, Marauder, Reaper, Ghost).
    DirectorateInfantry,
    /// Directorate ground vehicle cannons and railguns (Hellion, Tank, Thor).
    DirectorateVehicle,
    /// Directorate aerial ordnance and missiles (Viking, Banshee, Liberator).
    DirectorateShip,
    /// Cosmic Enclave ground psionic blades and particle beams (Zealot, Stalker, Immortal, Colossus).
    EnclaveGround,
    /// Cosmic Enclave aerial ray projectors and interceptors (Phoenix, Void Ray, Carrier).
    EnclaveAir,
    /// Sylvan Swarm melee claws, scythes, and cleaving horns (Zergling, Ultralisk).
    SwarmMelee,
    /// Sylvan Swarm projectile acid, spines, and bile (Roach, Ravager, Hydralisk, Lurker).
    SwarmMissile,
    /// Sylvan Swarm aerial flyers and parasites (Mutalisk, Corruptor, Brood Lord).
    SwarmFlyer,
}

/// High-level life armor classification determining which armor plating upgrade applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ArmorClass {
    /// Directorate infantry composite armor.
    DirectorateInfantry,
    /// Directorate vehicle and ship reinforced plating.
    DirectoratePlating,
    /// Cosmic Enclave ground exosuits and strider hulls.
    EnclaveGround,
    /// Cosmic Enclave starship chassis.
    EnclaveAir,
    /// Sylvan Swarm ground chitinous carapace.
    SwarmGroundCarapace,
    /// Sylvan Swarm aerial flyer carapace.
    SwarmFlyerCarapace,
}

/// Composable upgrade effect modifying weapon scaling, armor, unit stats, or abilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpgradeEffect {
    /// Advances scaling weapon level for an entire weapon class.
    WeaponDamageLevel {
        /// Weapon class receiving the level advancement.
        weapon_class: WeaponClass,
        /// New level tier (1..=3).
        level: u8,
    },
    /// Advances life armor level for an entire armor class.
    ArmorLevel {
        /// Armor class receiving the level advancement.
        armor_class: ArmorClass,
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
    pub weapon_levels: BTreeMap<WeaponClass, u8>,
    /// Researched armor level (0..=3) per armor classification.
    pub armor_levels: BTreeMap<ArmorClass, u8>,
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
    /// Creates an empty upgrade state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies an upgrade definition's effects to the player's active state.
    pub fn apply_upgrade(&mut self, upgrade_slug: &str, effects: &[UpgradeEffect]) {
        self.completed_upgrade_slugs
            .insert(upgrade_slug.to_string());
        for effect in effects {
            match effect {
                UpgradeEffect::WeaponDamageLevel {
                    weapon_class,
                    level,
                } => {
                    self.weapon_levels.insert(*weapon_class, *level);
                }
                UpgradeEffect::ArmorLevel { armor_class, level } => {
                    self.armor_levels.insert(*armor_class, *level);
                }
                UpgradeEffect::ShieldLevel { level } => {
                    self.shield_level = *level;
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
                    let entry = self
                        .unit_stat_modifiers
                        .entry(target_slug.clone())
                        .or_default();
                    entry.max_hp_delta += *max_hp_delta;
                    entry.move_speed_delta += *move_speed_delta;
                    entry.sight_delta += *sight_delta;
                    entry.weapon_range_delta += *weapon_range_delta;
                    entry.armor_delta += *armor_delta;
                    entry.attack_cooldown_delta += *attack_cooldown_delta;
                    entry.bonus_damage_delta += *bonus_damage_delta;
                }
                UpgradeEffect::UnlockAbility {
                    target_slug,
                    ability_slug,
                } => {
                    self.unlocked_abilities
                        .entry(target_slug.clone())
                        .or_default()
                        .insert(ability_slug.clone());
                }
            }
        }
    }
}
