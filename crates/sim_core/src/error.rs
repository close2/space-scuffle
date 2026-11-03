//! # Simulation Core Error Types
//!
//! Strongly-typed error hierarchy for the deterministic simulation engine.
//! Zero panics or silent failures; all fallible domain operations return explicit [`SimResult`].

use sim_types::EntityId;
use thiserror::Error;

/// Errors that can occur within the deterministic simulation core.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SimCoreError {
    /// An entity order or buff has malformed authored/current numeric input.
    #[error(transparent)]
    EntityAdmission(#[from] crate::entity::EntityAdmissionError),
    /// Checked movement speed or geometry preparation failed.
    #[error(transparent)]
    MovementMath(#[from] crate::movement::MovementMathError),
    /// An atomic spawn or supply operation encountered invalid authoritative state.
    #[error(transparent)]
    ProductionInvariant(#[from] crate::production::ProductionInvariant),
    /// A researched effect could not be accumulated without losing numeric information.
    #[error(transparent)]
    UpgradeApplication(#[from] sim_types::UpgradeApplicationError),

    /// Applying an upgrade would overflow a live entity's hull hit points.
    #[error("upgrade would overflow live hit points for entity {entity:?}")]
    UpgradeEntityVitalsOverflow {
        /// Entity whose current or maximum hit points cannot represent the effect.
        entity: EntityId,
    },
    /// An entity with the given ID was not found in active simulation state.
    #[error("Entity with ID {0:?} does not exist")]
    EntityNotFound(EntityId),

    /// A command was invalid for the specified entity.
    #[error("Invalid command for entity {0:?}: {1}")]
    InvalidCommand(EntityId, String),

    /// Insufficient minerals or gas to execute the command.
    #[error("Insufficient resources: required {required} minerals, player has {available}")]
    InsufficientResources {
        /// Minerals required.
        required: u32,
        /// Minerals currently available.
        available: u32,
    },

    /// Command would exceed the player's maximum supply cap.
    #[error("Supply cap exceeded: current {current}, max {max}")]
    SupplyCapExceeded {
        /// Current supply consumed.
        current: u32,
        /// Maximum supply allowed.
        max: u32,
    },

    /// Building placement failed due to obstruction or unpathable terrain.
    #[error("Building placement invalid at ({x}, {y}): obstructed or unpathable")]
    InvalidBuildingPlacement {
        /// Cell X coordinate.
        x: u32,
        /// Cell Y coordinate.
        y: u32,
    },

    /// Archetype ID was not registered in the unit catalog.
    #[error("Catalog archetype {0} not found")]
    ArchetypeNotFound(u16),

    /// Underlying map error occurred during pathfinding, placement, or terrain queries.
    #[error("Map error: {0}")]
    MapError(String),

    /// Catalog failed to deserialize from a RON string or asset file.
    #[error("RON deserialization error: {0}")]
    RonDeserializationError(String),

    /// Catalog file could not be read from disk.
    #[error("I/O error: {0}")]
    IoError(String),

    /// Authored definitions reuse a numeric identifier within one registry.
    #[error("duplicate {kind} catalog ID {id}")]
    CatalogDuplicateId {
        /// Registry in which the identifier must be unique.
        kind: &'static str,
        /// Conflicting identifier from the authored data.
        id: u32,
    },

    /// Authored definitions reuse a slug under the registry's lookup semantics.
    #[error("duplicate {kind} catalog slug '{slug}'")]
    CatalogDuplicateSlug {
        /// Registry in which the slug must be unique.
        kind: &'static str,
        /// Conflicting authored slug.
        slug: String,
    },

    /// An authored value cannot be safely represented by the simulation schema.
    #[error("invalid catalog field '{field}' in '{definition}': {reason}")]
    CatalogInvalidValue {
        /// Definition slug or other author-facing identity.
        definition: String,
        /// Field path within the definition.
        field: String,
        /// Stable explanation of the violated input invariant.
        reason: &'static str,
    },

    /// An authored definition names a faction that `factions.ron` does not define.
    #[error("'{definition}' names unknown faction '{slug}'")]
    UnknownFaction {
        /// Slug of the unit, structure, upgrade or class that names the faction.
        definition: String,
        /// The faction slug that resolved to nothing.
        slug: String,
    },

    /// An authored definition names a weapon class that `factions.ron` does not define.
    #[error("'{definition}' names unknown weapon class '{slug}'")]
    UnknownWeaponClass {
        /// Slug of the unit, structure or upgrade that names the weapon class.
        definition: String,
        /// The weapon-class slug that resolved to nothing.
        slug: String,
    },

    /// An authored definition names an armor class that `factions.ron` does not define.
    #[error("'{definition}' names unknown armor class '{slug}'")]
    UnknownArmorClass {
        /// Slug of the unit or upgrade that names the armor class.
        definition: String,
        /// The armor-class slug that resolved to nothing.
        slug: String,
    },

    /// Missing prerequisite structure required for building construction or research.
    #[error("Missing prerequisite structure '{required}' to produce/research '{target}'")]
    MissingPrerequisiteStructure {
        /// Target structure or upgrade slug.
        target: String,
        /// Required prerequisite structure slug.
        required: String,
    },

    /// Specified upgrade slug was not found in the upgrade catalog.
    #[error("Unknown upgrade slug: {0}")]
    UnknownUpgrade(String),

    /// Upgrade has already been researched by this player.
    #[error("Upgrade '{0}' has already been researched")]
    UpgradeAlreadyResearched(String),

    /// Specified unit or structure type slug was not found.
    #[error("Invalid unit or structure type slug: {0}")]
    InvalidUnitType(String),
}

impl From<sim_map::MapError> for SimCoreError {
    fn from(err: sim_map::MapError) -> Self {
        Self::MapError(err.to_string())
    }
}

/// Specialized result type for simulation core operations.
pub type SimResult<T> = Result<T, SimCoreError>;
