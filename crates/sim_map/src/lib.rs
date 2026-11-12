//! # `sim_map`
//!
//! Authoritative, deterministic map representation, terrain elevation,
//! Fog of War visibility, and navigation-mesh pathfinding for **Space Scuffle**.
//!
//! ## Architectural Principles
//! - **Dual-Grid Model:** A discrete 1x1 cell grid controls discrete spatial properties
//!   (cliffs, ramps, building footprints, line-of-sight occlusion), while continuous
//!   fixed-point `SimVec2` coordinates govern unit movement and steering.
//! - **Strict Determinism:** Pure fixed-point mathematics (`SimScalar`, `SimVec2`) are
//!   used for all authoritative simulation queries. Floating-point conversions (`f32`)
//!   are strictly isolated for visual interpolation.
//! - **Zero Allocations in Hot Paths:** Visibility checks and path queries reuse
//!   scratch buffers without allocating dynamic heap memory per tick.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(clippy::unreachable)]

pub mod error;
pub mod grid;
pub mod map_format;
pub mod navmesh;
pub mod pathfinding;
pub mod traversal;
pub mod validation;
pub mod vision;

pub use error::{MapError, PlacementFailureReason, Result};
pub use grid::{
    CellFlags, CliffLevel, GameMap, MapCell, PlacedStructure, RampDir, RampProfile, ResourceKind,
    ResourceNode, TerrainKind, Watchtower, CLIFF_LEVEL_HEIGHT, MAX_RAMP_RUN_CELLS,
};
pub use map_format::{
    MapCellSpec, MapDocument, MapEntityId, MapFile, MapFormatError, MapLayer, MapMetadata,
    MapResource, MapStartLocation, MapWatchtower, CURRENT_MAP_FORMAT_VERSION, MAX_MAP_DIMENSION,
    MIN_MAP_DIMENSION, WATCHTOWER_ENTITY_ID_BASE,
};
pub use navmesh::{
    funnel::Side,
    lanes::{LaneCut, LaneCuts, LaneWord},
    predicates::LatticePoint,
    search::MeshRoute,
    segment::WallContact,
    EdgeKind, NavMesh, NavMeshError, TriangleKind, TriangleLabel,
};
pub use pathfinding::{
    PathRequest, PathRequestWithVision, PathResult, Pathfinder, CORNER_SAFETY_MARGIN,
};
pub use sim_types::VisibilityState;
pub use validation::{
    validate_map, CellRect, MapValidationError, MapValidationReport, MapValidationWarning,
    MIN_RECOMMENDED_RAMP_WIDTH, MIN_START_LOCATIONS, MIN_START_SEPARATION_CELLS,
    RESOURCE_ASSOCIATION_RADIUS_CELLS, VALIDATION_PATH_RADIUS,
};
pub use vision::{watchtower_source, PlayerVisionMap, VisionSource, MAX_SIGHT_RADIUS_CELLS};

/// Canonical type alias for [`GameMap`] matching simulation core naming conventions.
pub type SimMap = GameMap;
