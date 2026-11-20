//! Strongly-typed map, vision, and pathfinding errors.
//!
//! This module defines the authoritative error types for the `sim_map` crate,
//! guaranteeing that all failure modes (out-of-bounds queries, blocked building
//! placement, disconnected paths, or corrupted map serializations) are
//! handled with explicit, descriptive error values rather than panics.

use sim_types::{EntityId, FootprintShape, ResourceNodeId, SimVec2};
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

    /// Target footprint covers a standing resource node's cells (plan
    /// `2026-11-20-resource-footprints` D-5 P1).
    #[error("target footprint covers resource node {0:?}")]
    OccupiedByResource(ResourceNodeId),

    /// A footprint cell is not walkable ground: the ground under a structure must be walkable,
    /// so the part of its cells its shape leaves open is open ground (plan
    /// `2026-11-18-contour-footprints` D-5).
    #[error("target cell is not walkable ground")]
    UnpathableTerrain,
}

/// Why a resource node's footprint could not be placed (plan `2026-11-20-resource-footprints`
/// D-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ResourceFootprintFailure {
    /// The footprint table has no row for the node's kind.
    #[error("no footprint for the node's kind")]
    NoFootprint,
    /// The kind's shape does not lie in its cells, or an outline is invalid.
    #[error("the kind's shape does not fit its cells")]
    InvalidShape,
    /// A footprint cell lies beyond the map.
    #[error("a footprint cell lies beyond the map")]
    OutOfBounds,
    /// A footprint cell is another node's.
    #[error("overlaps resource node {0:?}")]
    OverlapsNode(ResourceNodeId),
    /// A footprint cell holds a structure.
    #[error("overlaps structure {0:?}")]
    OverlapsStructure(EntityId),
    /// A footprint cell is not walkable, buildable, non-ramp ground.
    #[error("cell ({x}, {y}) is not open, buildable ground")]
    NotOpenGround {
        /// Cell column.
        x: u32,
        /// Cell row.
        y: u32,
    },
    /// A footprint cell is on another cliff level than the anchor.
    #[error("cell ({x}, {y}) is on another cliff level than the anchor")]
    CliffLevelMismatch {
        /// Cell column.
        x: u32,
        /// Cell row.
        y: u32,
    },
    /// The node already stands on the map.
    #[error("the node is already placed")]
    AlreadyPlaced,
    /// The node is not placed (removal of a node that does not stand).
    #[error("the node is not placed")]
    NotPlaced,
    /// A structure stands on the node, so it cannot be removed.
    #[error("a structure stands on the node")]
    UnderStructure,
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

    /// A pathing shape does not lie in its footprint cells or leaves a face of no length (plan
    /// `2026-11-18-contour-footprints` D-2), or an outline is not simple, has fewer than 3 or
    /// more than 16 vertices, or has an edge off the axes and diagonals (plan
    /// `2026-11-20-resource-footprints` D-2).
    #[error("footprint shape {shape:?} does not fit a {width}x{height} footprint")]
    InvalidFootprintShape {
        /// Footprint width in cells.
        width: u8,
        /// Footprint height in cells.
        height: u8,
        /// The refused shape, in quarter cells.
        shape: FootprintShape,
    },

    /// A resource node's footprint could not be placed or removed (plan
    /// `2026-11-20-resource-footprints` D-4).
    #[error("resource node {node:?}: {reason}")]
    ResourceFootprint {
        /// The node, by its index in the map's resources.
        node: ResourceNodeId,
        /// Why.
        reason: ResourceFootprintFailure,
    },

    /// A structure placed over a resource node must take exactly the node's standing cells
    /// (plan `2026-11-20-resource-footprints` D-6).
    #[error("structure {structure:?} is not exactly over resource node {node:?}")]
    NotOverResource {
        /// The structure.
        structure: EntityId,
        /// The node.
        node: ResourceNodeId,
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
