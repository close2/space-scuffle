//! Research and the upgrade-adjusted stats every phase reads.
//!
//! # Responsibility
//! Applies completed research to a player's [`PlayerUpgradeState`] and answers the effective
//! speed, range, attack period, sight, maximum HP and ability-unlock queries derived from it.
//!
//! # How it fits
//! Phase 1 admits research through the production queue and completes it here; phases 3 to 7
//! read the `effective_*` queries. The `*_from_upgrades` helpers take the upgrade array alone so
//! callers holding a mutable borrow of `entities` can still use them.
//!
//! # Invariants
//! - `research_upgrade` stages every player and entity write and commits only after all of them
//!   are proven representable, so a failure leaves the world unchanged.
//! - Effective speed is never negative: a negative result is an error, not a clamp.
//! - Only owners with a [`sim_types::PlayerSlot`] have upgrades. A neutral or out-of-range
//!   owner reads [`NO_UPGRADES`] and never another player's state (9.4, F-07).

use crate::catalog::{UnitArchetype, WeaponSpec};
use crate::error::{SimCoreError, SimResult};
use crate::movement::MovementMathError;
use crate::weapon_timing::{resolve_attack_period, AttackRate};
use crate::SimWorld;
use sim_types::{PerPlayer, PlayerId, PlayerUpgradeState, SimScalar};

/// The upgrade state of every owner without a player slot (neutral and out-of-range IDs).
static NO_UPGRADES: PlayerUpgradeState = PlayerUpgradeState::new();

impl SimWorld {
    /// Returns the player's active upgrade state, or `None` for an owner without a player slot.
    #[must_use]
    pub fn player_upgrade_state(&self, player: PlayerId) -> Option<&PlayerUpgradeState> {
        self.player_upgrades.get(player)
    }

    /// Upgrade state `owner` fights and moves with: its own, or [`NO_UPGRADES`] without a slot.
    pub(crate) fn upgrades_of(
        upgrades: &PerPlayer<PlayerUpgradeState>,
        owner: PlayerId,
    ) -> &PlayerUpgradeState {
        upgrades.get(owner).unwrap_or(&NO_UPGRADES)
    }

    /// Returns the player's mutable upgrade state, or `None` for an owner without a player slot.
    pub fn player_upgrade_state_mut(
        &mut self,
        player: PlayerId,
    ) -> Option<&mut PlayerUpgradeState> {
        self.player_upgrades.get_mut(player)
    }

    /// Applies a researched upgrade to a player's upgrade state, updating any affected live entities.
    ///
    /// # Errors
    /// Returns typed errors for unknown or already completed research, or for
    /// unrepresentable cumulative/live stats.
    /// On failure all player state, resources and entity vitals remain unchanged.
    pub fn research_upgrade(&mut self, player_id: PlayerId, upgrade_slug: &str) -> SimResult<()> {
        let upgrade = self
            .upgrade_catalog
            .get_by_slug(upgrade_slug)
            .ok_or_else(|| SimCoreError::UnknownUpgrade(upgrade_slug.to_string()))?
            .clone();

        let player_slot = player_id
            .slot()
            .ok_or(MovementMathError::InvalidOwner(player_id))?;
        let mut staged_player = self.player_upgrades[player_slot].clone();
        if staged_player.completed_upgrade_slugs.contains(upgrade_slug) {
            return Err(SimCoreError::UpgradeAlreadyResearched(
                upgrade_slug.to_string(),
            ));
        }
        staged_player.apply_upgrade(upgrade_slug, &upgrade.effects)?;
        for archetype in self.catalog.archetypes() {
            Self::effective_speed_with_state(archetype, &staged_player)?;
        }

        // Stage live vitals as well as research state: a late overflow must not
        // leave earlier entities, completed slugs or other effects committed.
        let mut staged_vitals = Vec::new();
        for (&id, entity) in &mut self.entities {
            if entity.owner != player_id {
                continue;
            }
            let archetype = self.catalog.get(entity.unit_type_id)?;
            Self::effective_speed_with_state(archetype, &staged_player)?
                .checked_mul(entity.speed_multiplier()?)
                .ok_or(MovementMathError::Unrepresentable)?;
            let mut hp = entity.hp;
            let mut max_hp = entity.max_hp;
            for effect in &upgrade.effects {
                if let sim_types::UpgradeEffect::ModifyUnitStat {
                    target_slug,
                    max_hp_delta,
                    ..
                } = effect
                {
                    if archetype.slug == *target_slug && *max_hp_delta > SimScalar::ZERO {
                        let overflow = || SimCoreError::UpgradeEntityVitalsOverflow { entity: id };
                        hp = hp.checked_add(*max_hp_delta).ok_or_else(overflow)?;
                        max_hp = max_hp.checked_add(*max_hp_delta).ok_or_else(overflow)?;
                    }
                }
            }
            if hp != entity.hp || max_hp != entity.max_hp {
                staged_vitals.push((entity, hp, max_hp));
            }
        }

        self.player_upgrades[player_slot] = staged_player;
        for (entity, hp, max_hp) in staged_vitals {
            entity.hp = hp;
            entity.max_hp = max_hp;
        }
        Ok(())
    }

    /// Validates whether a player possesses all prerequisite structures to construct a building.
    ///
    /// # Errors
    /// Returns [`SimCoreError::InvalidUnitType`] or [`SimCoreError::MissingPrerequisiteStructure`].
    pub fn can_construct_structure(
        &self,
        player_id: PlayerId,
        structure_slug: &str,
    ) -> SimResult<()> {
        crate::tech_tree::TechTreeValidator::can_construct_structure(
            self,
            player_id,
            structure_slug,
        )
    }

    /// Validates whether a player possesses prerequisites to research an upgrade.
    ///
    /// # Errors
    /// Returns [`SimCoreError::UnknownUpgrade`], [`SimCoreError::UpgradeAlreadyResearched`],
    /// or [`SimCoreError::MissingPrerequisiteStructure`].
    pub fn can_research_upgrade(&self, player_id: PlayerId, upgrade_slug: &str) -> SimResult<()> {
        crate::tech_tree::TechTreeValidator::can_research_upgrade(self, player_id, upgrade_slug)
    }

    pub(crate) fn effective_speed_from_upgrades(
        upgrades: &PerPlayer<PlayerUpgradeState>,
        archetype: &UnitArchetype,
        owner: PlayerId,
    ) -> Result<SimScalar, MovementMathError> {
        let state = upgrades
            .get(owner)
            .ok_or(MovementMathError::InvalidOwner(owner))?;
        Self::effective_speed_with_state(archetype, state)
    }

    fn effective_speed_with_state(
        archetype: &UnitArchetype,
        state: &PlayerUpgradeState,
    ) -> Result<SimScalar, MovementMathError> {
        if archetype.max_speed < SimScalar::ZERO {
            return Err(MovementMathError::NegativeSpeed);
        }
        let delta = state
            .unit_stat_modifiers
            .get(&archetype.slug)
            .map_or(SimScalar::ZERO, |m| m.move_speed_delta);
        let speed = archetype
            .max_speed
            .checked_add(delta)
            .ok_or(MovementMathError::Unrepresentable)?;
        if speed < SimScalar::ZERO {
            return Err(MovementMathError::NegativeSpeed);
        }
        Ok(speed)
    }

    pub(crate) fn effective_range_from_upgrades(
        upgrades: &PerPlayer<PlayerUpgradeState>,
        weapon: &WeaponSpec,
        unit_slug: &str,
        owner: PlayerId,
    ) -> SimScalar {
        let range_delta = Self::upgrades_of(upgrades, owner)
            .unit_stat_modifiers
            .get(unit_slug)
            .map_or(SimScalar::ZERO, |m| m.weapon_range_delta);
        weapon.max_range + range_delta
    }

    fn effective_period_from_upgrades(
        upgrades: &PerPlayer<PlayerUpgradeState>,
        weapon: &WeaponSpec,
        unit_slug: &str,
        owner: PlayerId,
    ) -> u16 {
        let cd_delta = Self::upgrades_of(upgrades, owner)
            .unit_stat_modifiers
            .get(unit_slug)
            .map_or(0, |m| m.attack_cooldown_delta);
        resolve_attack_period(weapon.period_ticks, cd_delta, AttackRate::ONE)
    }

    /// Computes nonnegative movement speed in world units per25Hz tick, including upgrades.
    ///
    /// # Errors
    /// Rejects an invalid owner, negative authored/effective speed or numeric overflow.
    pub fn effective_max_speed(
        &self,
        archetype: &UnitArchetype,
        owner: PlayerId,
    ) -> Result<SimScalar, MovementMathError> {
        Self::effective_speed_from_upgrades(&self.player_upgrades, archetype, owner)
    }

    /// Computes the effective weapon range taking into account active upgrades.
    #[must_use]
    pub fn effective_weapon_range(
        &self,
        weapon: &WeaponSpec,
        unit_slug: &str,
        owner: PlayerId,
    ) -> SimScalar {
        Self::effective_range_from_upgrades(&self.player_upgrades, weapon, unit_slug, owner)
    }

    /// The range, in world units, of `entity`'s `weapon` (of archetype `unit_slug`): its owner's
    /// upgrades, then its statuses (plan `2026-11-10-spellcaster-mechanics` §6.4).
    #[must_use]
    pub fn entity_weapon_range(
        &self,
        entity: &crate::Entity,
        weapon: &WeaponSpec,
        unit_slug: &str,
    ) -> SimScalar {
        entity.status_modifiers().weapon_range(
            self.effective_weapon_range(weapon, unit_slug, entity.owner),
            weapon.melee,
        )
    }

    /// Computes the upgraded interval in 25 Hz ticks, clamped to `1..=u16::MAX`.
    /// Temporary rate buffs are excluded from this upgrade-only query.
    #[must_use]
    pub fn effective_attack_period(
        &self,
        weapon: &WeaponSpec,
        unit_slug: &str,
        owner: PlayerId,
    ) -> u16 {
        Self::effective_period_from_upgrades(&self.player_upgrades, weapon, unit_slug, owner)
    }

    /// Computes the sight radius in world units, including upgrades.
    #[must_use]
    pub fn effective_sight_radius(&self, archetype: &UnitArchetype, owner: PlayerId) -> SimScalar {
        Self::sight_radius_from_upgrades(&self.player_upgrades, archetype, owner)
    }

    pub(super) fn sight_radius_from_upgrades(
        upgrades: &PerPlayer<PlayerUpgradeState>,
        archetype: &UnitArchetype,
        owner: PlayerId,
    ) -> SimScalar {
        let delta = Self::upgrades_of(upgrades, owner)
            .unit_stat_modifiers
            .get(&archetype.slug)
            .map_or(SimScalar::ZERO, |m| m.sight_delta);
        (archetype.sight_radius + delta).max(SimScalar::ZERO)
    }

    /// Whether `owner` may use `ability_slug` of `archetype`.
    ///
    /// Abilities no upgrade unlocks are always available. Otherwise the owner must have
    /// completed an upgrade that unlocks this one.
    #[must_use]
    pub fn is_ability_unlocked(
        &self,
        archetype: &UnitArchetype,
        owner: PlayerId,
        ability_slug: &str,
    ) -> bool {
        !self
            .upgrade_catalog
            .unlocks_ability(&archetype.slug, ability_slug)
            || Self::upgrades_of(&self.player_upgrades, owner)
                .unlocked_abilities
                .get(&archetype.slug)
                .is_some_and(|unlocked| unlocked.contains(ability_slug))
    }

    /// Computes the effective maximum HP taking into account active upgrades.
    #[must_use]
    pub fn effective_max_hp(&self, archetype: &UnitArchetype, owner: PlayerId) -> SimScalar {
        let hp_delta = Self::upgrades_of(&self.player_upgrades, owner)
            .unit_stat_modifiers
            .get(&archetype.slug)
            .map_or(SimScalar::ZERO, |m| m.max_hp_delta);
        archetype.max_hp + hp_delta
    }
}
