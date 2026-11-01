//! # Scenario Builder & Deterministic World Initialization
//!
//! Owns scenario construction, starting loadouts catalog parsing, and deterministic placement
//! of starting town halls and mobile unit formations onto a validated [`MapDocument`].
//!
//! ## System Role
//! Bridges declarative map documents ([`MapDocument`]) and start loadout definitions
//! ([`StartLoadoutCatalog`]) into an active lockstep simulation state ([`SimWorld`]).
//!
//! ## Invariants Maintained
//! - **Validation First:** Validates the map against all geometric and balance rules before
//!   instantiating the simulation world.
//! - **Exact Footprint Fit:** Validates that starting town halls fit strictly inside the reserved
//!   start location footprints without clipping or overflowing.
//! - **Deterministic Placement:** Mobile units are placed in deterministic radial concentric rings
//!   around the town hall center, adhering strictly to cardinal edge iteration order.
//! - **State Consistency:** World checksum is deterministically recalculated after all structures
//!   and units are placed.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sim_map::{
    validate_map, CellFlags, MapDocument, MapFormatError, MapValidationError, TerrainKind,
};
use sim_types::{PlayerId, ShapeKind, SimScalar, SimVec2};

use crate::catalog::UnitCatalog;
use crate::error::SimCoreError;
use crate::upgrades::UpgradeCatalog;
use crate::world::SimWorld;

/// Starting unit and structure composition for a player faction or loadout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartLoadout {
    /// Unique identifier slug (e.g. "`standard_automaton`").
    pub slug: String,
    /// User-facing display name.
    pub display_name: String,
    /// Slug of the faction town hall structure to spawn at the start location.
    pub town_hall: String,
    /// Mobile unit types and counts to spawn around the town hall.
    pub units: Vec<LoadoutUnit>,
}

/// Mobile unit count entry within a start loadout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadoutUnit {
    /// Archetype slug of the unit.
    pub slug: String,
    /// Number of units of this archetype to spawn.
    pub count: u8,
}

/// Catalog containing all authorable starting loadouts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartLoadoutCatalog {
    /// List of available start loadouts.
    pub loadouts: Vec<StartLoadout>,
}

impl StartLoadoutCatalog {
    /// Deserializes a loadouts catalog from a RON string.
    ///
    /// # Errors
    /// Returns [`ScenarioError::Ron`] if the string violates RON syntax or schema.
    pub fn from_ron_str(s: &str) -> Result<Self, ScenarioError> {
        Ok(ron::from_str(s)?)
    }

    /// Loads and deserializes a loadouts catalog from a filesystem path.
    ///
    /// # Errors
    /// Returns [`ScenarioError::Io`] if the file cannot be read, or [`ScenarioError::Ron`]
    /// if parsing fails.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ScenarioError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_ron_str(&content)
    }

    /// Looks up a start loadout by its unique slug.
    #[must_use]
    pub fn get(&self, slug: &str) -> Option<&StartLoadout> {
        self.loadouts.iter().find(|l| l.slug == slug)
    }
}

/// Player configuration for scenario instantiation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSetup {
    /// Zero-based player slot index (0..7).
    pub player_slot: u8,
    /// Slug of the start loadout to assign to this player.
    pub loadout_slug: String,
}

/// Strongly-typed error variants encountered during scenario creation and world building.
#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    /// File system or I/O error occurred during file access.
    #[error("I/O error reading scenario files: {0}")]
    Io(#[from] std::io::Error),

    /// RON deserialization failed.
    #[error("failed to deserialize scenario RON: {0}")]
    Ron(#[from] ron::error::SpannedError),

    /// Map document conversion to `GameMap` failed.
    #[error("failed to convert map document to game map: {0}")]
    MapConversion(#[from] MapFormatError),

    /// Map failed deterministic validation.
    #[error("map failed validation: {0:?}")]
    InvalidMap(Vec<MapValidationError>),

    /// Player specified a slot with no matching start location on the map.
    #[error("missing start location for player slot {slot}")]
    MissingStartLocation {
        /// Player slot index.
        slot: u8,
    },

    /// Start loadout slug was not found in the catalog.
    #[error("unknown loadout slug: {0}")]
    UnknownLoadout(String),

    /// Unit or structure slug was not found in the unit catalog.
    #[error("unknown unit or structure slug: {0}")]
    UnknownSlug(String),

    /// Town hall archetype does not have a dome shape.
    #[error("town hall archetype {0} is not a dome structure")]
    TownHallNotADome(String),

    /// Town hall footprint is larger than the start location footprint.
    #[error("town hall footprint does not fit in start location footprint for slot {slot}")]
    TownHallDoesNotFit {
        /// Player slot index.
        slot: u8,
    },

    /// Not enough valid pathable cells within distance 3 to spawn all starting units.
    #[error("not enough pathable spawn cells around town hall for slot {slot}")]
    NotEnoughSpawnCells {
        /// Player slot index.
        slot: u8,
    },

    /// Simulation engine failed to spawn an entity.
    #[error("failed to spawn entity: {0}")]
    Spawn(#[from] SimCoreError),
}

/// Builds an authoritative simulation [`SimWorld`] from a map document, loadouts, and player setup.
///
/// ## Placement Algorithm
/// 1. Validates the map document with [`validate_map`]; rejects any errors.
/// 2. Converts the map document into a simulation [`GameMap`](sim_map::GameMap).
/// 3. Instantiates the initial [`SimWorld`].
/// 4. Ensures all specified players have a designated [`sim_map::MapStartLocation`].
/// 5. For each start location in ascending slot order with a matching [`PlayerSetup`]:
///    - Resolves the requested loadout and unit archetypes.
///    - Checks that the town hall archetype is a dome shape that fits within the start footprint.
///    - Centers the town hall inside the footprint `(top_left + (fp - hall) / 2)` and spawns it.
///    - Spawns mobile units radially in concentric rings `d = 1, 2, 3` around the town hall footprint.
///    - Within each ring, cells are visited in perimeter order:
///      South edge (X ascending) -> East edge (Y ascending) -> North edge (X descending) -> West edge (Y descending).
///    - Units are only placed on in-bounds, ground-pathable, non-abyss, non-ramp, unoccupied cells.
/// 6. Recomputes the world checksum for authoritative simulation start.
///
/// # Errors
/// Returns [`ScenarioError`] on invalid maps, missing start locations, unknown slugs,
/// dome fitting failures, or entity spawning failures.
pub fn build_world(
    doc: &MapDocument,
    loadouts: &StartLoadoutCatalog,
    players: &[PlayerSetup],
    seed: u64,
) -> Result<SimWorld, ScenarioError> {
    build_world_with_catalogs(
        doc,
        loadouts,
        players,
        seed,
        UnitCatalog::new_starter_roster(),
        UpgradeCatalog::default_catalog(),
    )
}

/// Builds a validated scenario using supplied unit and upgrade rule catalogs.
///
/// Uses the same deterministic cell placement and seed semantics as [`build_world`],
/// resolving all loadout identities against `catalog`. No catalog files are read and
/// no embedded definitions are substituted. The catalogs are consumed by the world.
///
/// # Errors
/// Returns [`ScenarioError`] for invalid maps, absent loadout identities, invalid
/// footprints, missing player starts, or failed entity placement.
#[expect(
    clippy::too_many_lines,
    reason = "sequential scenario construction; splitting is plan step 9.2"
)]
pub fn build_world_with_catalogs(
    doc: &MapDocument,
    loadouts: &StartLoadoutCatalog,
    players: &[PlayerSetup],
    seed: u64,
    catalog: UnitCatalog,
    upgrade_catalog: UpgradeCatalog,
) -> Result<SimWorld, ScenarioError> {
    // 1. Validate the map document.
    let report = validate_map(doc);
    if !report.errors.is_empty() {
        return Err(ScenarioError::InvalidMap(report.errors));
    }

    // 2. Convert map document to GameMap.
    let game_map = doc.to_game_map()?;

    // 3. Create simulation world.
    let mut world = SimWorld::with_catalogs(game_map, seed, catalog, upgrade_catalog);

    // 4. Verify all players have an assigned start location.
    for player in players {
        if !doc
            .start_locations
            .iter()
            .any(|loc| loc.player_slot == player.player_slot)
        {
            return Err(ScenarioError::MissingStartLocation {
                slot: player.player_slot,
            });
        }
    }

    // Sort start locations by slot order.
    let mut sorted_starts = doc.start_locations.clone();
    sorted_starts.sort_by_key(|loc| loc.player_slot);

    let map_w = i32::try_from(world.map.width()).unwrap_or(i32::MAX);
    let map_h = i32::try_from(world.map.height()).unwrap_or(i32::MAX);
    let mut claimed_cells = BTreeSet::new();

    // 5. Spawn structures and units for each player in start location slot order.
    for start_loc in &sorted_starts {
        let Some(player) = players
            .iter()
            .find(|p| p.player_slot == start_loc.player_slot)
        else {
            // Start locations without an assigned player are left empty.
            continue;
        };

        let loadout = loadouts
            .get(&player.loadout_slug)
            .ok_or_else(|| ScenarioError::UnknownLoadout(player.loadout_slug.clone()))?;

        let th_arch = world
            .catalog
            .get_by_slug(&loadout.town_hall)
            .ok_or_else(|| ScenarioError::UnknownSlug(loadout.town_hall.clone()))?;

        let (hall_w, hall_h) = match th_arch.shape {
            ShapeKind::BuildingDome {
                footprint_w,
                footprint_h,
            } => (u32::from(footprint_w), u32::from(footprint_h)),
            ShapeKind::UnitSphere => {
                return Err(ScenarioError::TownHallNotADome(loadout.town_hall.clone()));
            }
        };

        let fp_w = u32::from(start_loc.footprint.0);
        let fp_h = u32::from(start_loc.footprint.1);
        if hall_w > fp_w || hall_h > fp_h {
            return Err(ScenarioError::TownHallDoesNotFit {
                slot: start_loc.player_slot,
            });
        }

        // Verify all unit slugs exist in the catalog.
        let mut units_to_spawn = Vec::new();
        for unit_entry in &loadout.units {
            let unit_arch = world
                .catalog
                .get_by_slug(&unit_entry.slug)
                .ok_or_else(|| ScenarioError::UnknownSlug(unit_entry.slug.clone()))?;
            for _ in 0..unit_entry.count {
                units_to_spawn.push(unit_arch.unit_type_id);
            }
        }

        // Center the town hall in the start footprint.
        let (tl_x, tl_y) = start_loc.cell;
        let hall_cell_x = tl_x + (fp_w - hall_w) / 2;
        let hall_cell_y = tl_y + (fp_h - hall_h) / 2;

        world.spawn_structure(
            th_arch.unit_type_id,
            PlayerId(start_loc.player_slot),
            hall_cell_x,
            hall_cell_y,
        )?;

        let anchor_cliff_level = world
            .map
            .cell(hall_cell_x, hall_cell_y)
            .map_err(|e| ScenarioError::Spawn(SimCoreError::MapError(e.to_string())))?
            .cliff_level;

        // Place mobile units on concentric rings around town hall footprint.
        let cell_coordinate = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
        let x_min = cell_coordinate(hall_cell_x);
        let x_max = cell_coordinate(hall_cell_x + hall_w - 1);
        let y_min = cell_coordinate(hall_cell_y);
        let y_max = cell_coordinate(hall_cell_y + hall_h - 1);

        let mut spawned_count = 0;
        let total_needed = units_to_spawn.len();

        for d in 1..=3 {
            if spawned_count >= total_needed {
                break;
            }

            let x0 = x_min - d;
            let x1 = x_max + d;
            let y0 = y_min - d;
            let y1 = y_max + d;

            for (cx, cy) in ring_perimeter(x0, x1, y0, y1) {
                if spawned_count >= total_needed {
                    break;
                }
                if cx < 0 || cy < 0 || cx >= map_w || cy >= map_h {
                    continue;
                }
                let (Ok(ux), Ok(uy)) = (u32::try_from(cx), u32::try_from(cy)) else {
                    continue;
                };

                let Ok(cell) = world.map.cell(ux, uy) else {
                    continue;
                };

                // Units must spawn on the same cliff plateau as the town hall anchor cell.
                if cell.cliff_level != anchor_cliff_level {
                    continue;
                }
                if !cell.flags.contains(CellFlags::GROUND_PATHABLE) {
                    continue;
                }
                if cell.terrain_kind == TerrainKind::Abyss {
                    continue;
                }
                if cell.ramp.is_some() || cell.flags.contains(CellFlags::RAMP) {
                    continue;
                }
                if cell.occupying_structure.is_some() {
                    continue;
                }
                if claimed_cells.contains(&(ux, uy)) {
                    continue;
                }

                let pos = SimVec2::new(
                    SimScalar::from_num(ux) + SimScalar::HALF,
                    SimScalar::from_num(uy) + SimScalar::HALF,
                );
                let unit_type = units_to_spawn[spawned_count];
                world.spawn_unit(unit_type, PlayerId(start_loc.player_slot), pos)?;
                claimed_cells.insert((ux, uy));
                spawned_count += 1;
            }
        }

        if spawned_count < total_needed {
            return Err(ScenarioError::NotEnoughSpawnCells {
                slot: start_loc.player_slot,
            });
        }
    }

    // 6. Deterministically recompute simulation checksum.
    world.last_checksum = world.compute_checksum();
    Ok(world)
}

/// Returns an iterator yielding discrete integer coordinates along the perimeter of the rectangle
/// `[x0, x1] × [y0, y1]` in canonical counter-clockwise cardinal order without heap allocation.
///
/// ## Perimeter Traversal Order
/// 1. **South Edge:** `y = y0`, `x` ascending from `x0` to `x1`.
/// 2. **East Edge:** `x = x1`, `y` ascending from `y0 + 1` to `y1`.
/// 3. **North Edge:** `y = y1`, `x` descending from `x1 - 1` down to `x0`.
/// 4. **West Edge:** `x = x0`, `y` descending from `y1 - 1` down to `y0 + 1`.
pub fn ring_perimeter(x0: i32, x1: i32, y0: i32, y1: i32) -> impl Iterator<Item = (i32, i32)> {
    let south = (x0..=x1).map(move |x| (x, y0));
    let east = ((y0 + 1)..=y1).map(move |y| (x1, y));
    let north = (x0..x1).rev().map(move |x| (x, y1));
    let west = ((y0 + 1)..y1).rev().map(move |y| (x0, y));

    south.chain(east).chain(north).chain(west)
}
