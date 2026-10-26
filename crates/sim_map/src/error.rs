//! Strongly-typed map, vision, and pathfinding errors.
//!
//! This module defines the authoritative error types for the `sim_map` crate,
//! guaranteeing that all failure modes (out-of-bounds queries, blocked building
//! placement, disconnected paths, or corrupted map serializations) are
//! handled with explicit, descriptive error values rather than panics.

use sim_types::{EntityId, SimVec2};
use thiserror::Error;

/// Specific cause for why building placement failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PlacementFailureReason {
    /// Target cell is marked unbuildable by terrain flags.
    #[error("target cell is marked unbuildable")]
    UnbuildableTerrain,

    /// Building footprint spans cells on different cliff elevation levels.
    #[error("building footprint straddles multiple cliff levels")]
    CliffLevelMismatch,

    /// Building overlaps a ramp cell.
    #[error("building overlaps a ramp tile")]
    OverlapsRamp,

    /// Target footprint is already occupied by another structure.
    #[error("target footprint is already occupied by entity {0:?}")]
    OccupiedByStructure(EntityId),

    /// Footprint extends beyond map boundaries.
    #[error("footprint extends beyond map boundaries")]
    OutOfBounds,
}

/// Comprehensive error type for all `sim_map` operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MapError {
    /// Grid coordinate is outside valid map boundaries.
    #[error("coordinate ({x}, {y}) out of map bounds ({width}x{height})")]
    OutOfBounds {
        /// X coordinate queried.
        x: i32,
        /// Y coordinate queried.
        y: i32,
        /// Width of map.
        width: u32,
        /// Height of map.
        height: u32,
    },

    /// Building footprint dimensions must be greater than zero.
    #[error("invalid building footprint dimensions ({width}x{height}); must be > 0")]
    InvalidFootprintDimensions {
        /// Width in cells.
        width: u8,
        /// Height in cells.
        height: u8,
    },

    /// Building placement was blocked at the given cell.
    #[error("building placement failed at cell ({cell_x}, {cell_y}): {reason}")]
    PlacementBlocked {
        /// Top-left cell X.
        cell_x: u32,
        /// Top-left cell Y.
        cell_y: u32,
        /// Specific reason for placement rejection.
        reason: PlacementFailureReason,
    },

    /// Structure with specified `EntityId` was not found in map registry.
    #[error("structure with EntityId {0:?} was not found on the map")]
    StructureNotFound(EntityId),

    /// Pathfinding start position is out of bounds or impassable.
    #[error("pathfinding start position {0:?} is out of bounds or impassable")]
    InvalidStart(SimVec2),

    /// Pathfinding goal position is out of bounds or impassable.
    #[error("pathfinding goal position {0:?} is out of bounds or impassable")]
    InvalidGoal(SimVec2),

    /// No navigable path could be found between start and goal.
    #[error("no valid path found from {start:?} to {goal:?}")]
    NoPathFound {
        /// Requested start position.
        start: SimVec2,
        /// Requested goal position.
        goal: SimVec2,
    },

    /// Deserialization of RON map format failed.
    #[error("failed to deserialize map RON data: {0}")]
    RonDeserializationError(String),

    /// Serialization of RON map format failed.
    #[error("failed to serialize map RON data: {0}")]
    RonSerializationError(String),

    /// Parsing of Tiled TMJ map format failed.
    #[error("failed to parse Tiled TMJ map: {0}")]
    TiledParseError(String),

    /// Map dimensions must be greater than zero.
    #[error("invalid map dimensions: width={width}, height={height}")]
    InvalidDimensions {
        /// Map width.
        width: u32,
        /// Map height.
        height: u32,
    },
}

/// Convenience result alias for `sim_map` operations.
pub type Result<T> = std::result::Result<T, MapError>;
