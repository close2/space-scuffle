//! # Simulation Core Error Types
//!
//! Strongly-typed error hierarchy for the deterministic simulation engine.
//! Zero panics or silent failures; all fallible domain operations return explicit [`SimResult`].

use sim_types::EntityId;
use thiserror::Error;

/// Errors that can occur within the deterministic simulation core.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SimCoreError {
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
