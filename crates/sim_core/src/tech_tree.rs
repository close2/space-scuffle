//! # Tech Tree Prerequisite Validation
//!
//! Validates whether a player possesses required prerequisite structures
//! to construct buildings, train units, or research upgrades.
//!
//! ## Invariants Maintained
//! 1. 100% deterministic queries over active completed structures in [`SimWorld`].
//! 2. Zero hardcoded slugs in validation engine; all requirements query data catalogs.

use crate::error::{SimCoreError, SimResult};
use crate::world::SimWorld;
use sim_types::PlayerId;
use std::collections::BTreeSet;

/// Authoritative validator for building placement, unit production, and upgrade research.
pub struct TechTreeValidator;

impl TechTreeValidator {
    /// Validates whether a player possesses all prerequisite structures to construct a building.
    ///
    /// # Errors
    /// Returns [`SimCoreError::InvalidUnitType`] if structure slug does not exist,
    /// or [`SimCoreError::MissingPrerequisiteStructure`] if a requirement is unfulfilled.
    pub fn can_construct_structure(
        world: &SimWorld,
        player_id: PlayerId,
        structure_slug: &str,
    ) -> SimResult<()> {
        let structure_arch = world
            .catalog
            .get_by_slug(structure_slug)
            .ok_or_else(|| SimCoreError::InvalidUnitType(structure_slug.to_string()))?;

        let existing_completed_structures: BTreeSet<&str> = world
            .entities
            .values()
            .filter(|e| e.owner == player_id && e.is_completed)
            .filter_map(|e| {
                world
                    .catalog
                    .get(e.unit_type_id)
                    .ok()
                    .map(|a| a.slug.as_str())
            })
            .collect();

        for req in &structure_arch.requires_structures {
            if !existing_completed_structures.contains(req.as_str()) {
                return Err(SimCoreError::MissingPrerequisiteStructure {
                    target: structure_slug.to_string(),
                    required: req.clone(),
                });
            }
        }
        Ok(())
    }

    /// Validates whether a player possesses prerequisites to research an upgrade.
    ///
    /// # Errors
    /// Returns [`SimCoreError::UnknownUpgrade`] if upgrade slug is unknown,
    /// [`SimCoreError::UpgradeAlreadyResearched`] if already complete,
    /// or [`SimCoreError::MissingPrerequisiteStructure`] if requirements are missing.
    pub fn can_research_upgrade(
        world: &SimWorld,
        player_id: PlayerId,
        upgrade_slug: &str,
    ) -> SimResult<()> {
        let upgrade = world
            .upgrade_catalog
            .get_by_slug(upgrade_slug)
            .ok_or_else(|| SimCoreError::UnknownUpgrade(upgrade_slug.to_string()))?;

        let player_state = world.player_upgrade_state(player_id);
        if player_state.completed_upgrade_slugs.contains(upgrade_slug) {
            return Err(SimCoreError::UpgradeAlreadyResearched(
                upgrade_slug.to_string(),
            ));
        }

        let existing_completed_structures: BTreeSet<&str> = world
            .entities
            .values()
            .filter(|e| e.owner == player_id && e.is_completed)
            .filter_map(|e| {
                world
                    .catalog
                    .get(e.unit_type_id)
                    .ok()
                    .map(|a| a.slug.as_str())
            })
            .collect();

        for req in &upgrade.requires_structures {
            if !existing_completed_structures.contains(req.as_str()) {
                return Err(SimCoreError::MissingPrerequisiteStructure {
                    target: upgrade_slug.to_string(),
                    required: req.clone(),
                });
            }
        }
        Ok(())
    }
}
