//! # `sim_map`
//!
//! Authoritative, deterministic map representation, terrain elevation,
//! Fog of War visibility, and funnel pathfinding for **Space Scuffle**.
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
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::manual_let_else,
    clippy::cast_lossless
)]

pub mod error;
pub mod grid;
pub mod loader;
pub mod pathfinding;
pub mod vision;

pub use error::{MapError, PlacementFailureReason, Result};
pub use grid::{
    CellFlags, CliffLevel, GameMap, MapCell, PlacedStructure, RampDir, ResourceKind, ResourceNode,
    TerrainKind, Watchtower,
};
pub use loader::ScMapData;
pub use pathfinding::{
    PathRequest, PathRequestWithVision, PathResult, Pathfinder, PathfinderBuffers, PortalEdge,
    UnitClearance, CORNER_SAFETY_MARGIN, NO_HEAP_POS,
};
pub use sim_types::VisibilityState;
pub use vision::{PlayerVisionMap, VisionSource};

/// Canonical type alias for [`GameMap`] matching simulation core naming conventions.
pub type SimMap = GameMap;
