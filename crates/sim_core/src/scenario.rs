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
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sim_map::{
    validate_map, CellFlags, CliffLevel, MapDocument, MapFile, MapFormatError, MapStartLocation,
    MapValidationError, TerrainKind,
};
use sim_types::{PlayerId, PlayerSlot, ShapeKind, SimScalar, SimVec2, UnitTypeId};

use crate::catalog::{RuntimeCatalogs, UnitCatalog};
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
    /// Whether the map editor preselects this loadout for every player slot. Exactly one
    /// loadout in a catalog sets it ([`StartLoadoutCatalog::from_ron_str`] checks).
    #[serde(default)]
    pub editor_default: bool,
    /// Slug of the faction town hall structure to spawn at the start location.
    pub town_hall: String,
    /// Mobile unit types and counts to spawn around the town hall.
    pub units: Vec<LoadoutUnit>,
    /// Bank this loadout starts with. Omitted, the player keeps the world's default bank.
    #[serde(default)]
    pub starting_resources: Option<StartingResources>,
}

/// Resources a player starts a match with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartingResources {
    /// Mineral reserve.
    pub minerals: u32,
    /// Vespene gas reserve.
    pub gas: u32,
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
    /// Returns [`ScenarioError::Ron`] if the string violates RON syntax or schema, or
    /// [`ScenarioError::EditorDefaultLoadout`] unless exactly one loadout sets `editor_default`.
    pub fn from_ron_str(s: &str) -> Result<Self, ScenarioError> {
        let catalog: Self = ron::from_str(s)?;
        catalog.editor_default()?;
        Ok(catalog)
    }

    /// The loadout the map editor preselects for every player slot: the one that sets
    /// `editor_default`.
    ///
    /// # Errors
    /// Returns [`ScenarioError::EditorDefaultLoadout`] with the number of flagged loadouts unless
    /// that number is exactly one. A parsed catalog always has one; a catalog edited through its
    /// public field may not.
    pub fn editor_default(&self) -> Result<&StartLoadout, ScenarioError> {
        let mut flagged = self
            .loadouts
            .iter()
            .filter(|loadout| loadout.editor_default);
        match (flagged.next(), flagged.count()) {
            (Some(loadout), 0) => Ok(loadout),
            (first, rest) => Err(ScenarioError::EditorDefaultLoadout {
                count: usize::from(first.is_some()) + rest,
            }),
        }
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

    /// A start loadout catalog does not mark exactly one loadout `editor_default`.
    #[error("start loadouts must mark exactly one loadout editor_default, found {count}")]
    EditorDefaultLoadout {
        /// Number of loadouts that set `editor_default`.
        count: usize,
    },

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

    /// Scenario file declares a schema version this build does not read.
    #[error("unsupported scenario format version {0}")]
    UnsupportedVersion(u32),

    /// Rule catalogs in the assets directory could not be loaded.
    #[error(transparent)]
    Catalogs(#[from] crate::catalog::CatalogLoadError),
}

/// The scenario file schema version this build reads.
pub const SCENARIO_FORMAT_VERSION: u32 = 1;

/// The parts of a scenario file the simulation needs: map, seed and players.
///
/// Scenario files are shared with the client, which also reads a list of initial orders. That
/// list is the client's own and this type passes over it, so a playable start must carry
/// everything in its loadouts.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScenarioManifest {
    /// Scenario schema version.
    pub format_version: u32,
    /// Map path relative to the assets directory.
    pub map: PathBuf,
    /// Deterministic seed of the match.
    pub seed: u64,
    /// Player slots and their loadouts.
    pub players: Vec<PlayerSetup>,
}

impl ScenarioManifest {
    /// Parses a scenario from RON text.
    ///
    /// # Errors
    /// Returns [`ScenarioError::Ron`] on bad syntax and [`ScenarioError::UnsupportedVersion`]
    /// on a schema this build does not read.
    pub fn from_ron_str(text: &str) -> Result<Self, ScenarioError> {
        let manifest: Self = ron::from_str(text)?;
        if manifest.format_version != SCENARIO_FORMAT_VERSION {
            return Err(ScenarioError::UnsupportedVersion(manifest.format_version));
        }
        Ok(manifest)
    }
}

/// Builds the starting world of the scenario at `scenario_path` under `assets_root`.
///
/// Reads the scenario, its map, the start loadouts and all rule catalogs from real files, with
/// no embedded substitute. `scenario_path` is relative to `assets_root`, for example
/// `scenarios/skirmish.ron`.
///
/// # Errors
/// Returns a typed I/O, parse, catalog or placement failure.
pub fn load_scenario_world(
    assets_root: &Path,
    scenario_path: &Path,
) -> Result<SimWorld, ScenarioError> {
    let manifest =
        ScenarioManifest::from_ron_str(&std::fs::read_to_string(assets_root.join(scenario_path))?)?;
    let document = MapFile::load(&assets_root.join(&manifest.map))?.decode()?;
    let loadouts = StartLoadoutCatalog::load(assets_root.join("catalog/start_loadouts.ron"))?;
    let catalogs = RuntimeCatalogs::load_from_directory(assets_root.join("catalog"))?;
    build_world_with_catalogs(
        &document,
        &loadouts,
        &manifest.players,
        manifest.seed,
        catalogs.units,
        catalogs.upgrades,
    )
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
///    - A loadout's `starting_resources` replace the player's default bank.
/// 6. Records the match's players and recomputes the world checksum.
///
/// Resolves all loadout identities against `catalog`. No catalog files are read and no
/// embedded definitions are substituted; the catalogs are consumed by the world. Production
/// callers load them from disk ([`load_scenario_world`], `RuntimeCatalogs::load_from_directory`).
///
/// # Errors
/// Returns [`ScenarioError`] for invalid maps, absent loadout identities, invalid
/// footprints, missing player starts, or failed entity placement.
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
        let resolved = resolve_loadout(&world, loadouts, player, start_loc)?;
        apply_starting_bank(&mut world, start_loc, resolved.starting_resources)?;
        let hall = place_town_hall(&mut world, start_loc, &resolved)?;
        place_ring_units(
            &mut world,
            start_loc.player_slot,
            &hall,
            &resolved.units,
            &mut claimed_cells,
        )?;
    }

    // 6. Record the match's players, then deterministically recompute the checksum.
    world.update_match_state();
    world.last_checksum = world.compute_checksum();
    Ok(world)
}

/// One player's loadout resolved against the unit catalog and checked against its start.
struct ResolvedLoadout {
    /// Archetype of the town hall to spawn.
    town_hall: UnitTypeId,
    /// Town hall footprint `(width, height)` in cells.
    hall_size: (u32, u32),
    /// Bank replacing the player's default, if the loadout names one.
    starting_resources: Option<StartingResources>,
    /// Archetype of every mobile unit to spawn, in loadout order.
    units: Vec<UnitTypeId>,
}

/// The cells a placed town hall covers, inclusive, and the plateau its units must share.
struct PlacedHall {
    x_min: i32,
    x_max: i32,
    y_min: i32,
    y_max: i32,
    /// Cliff level of the hall's anchor cell.
    cliff_level: CliffLevel,
}

/// Step 5, first part: resolves `player`'s loadout and checks that its town hall is a dome
/// that fits the start footprint and that every unit slug names an archetype.
///
/// # Errors
/// [`ScenarioError::UnknownLoadout`], [`ScenarioError::UnknownSlug`],
/// [`ScenarioError::TownHallNotADome`] or [`ScenarioError::TownHallDoesNotFit`], checked in
/// that order: loadout, town hall slug, shape, fit, then each unit slug.
fn resolve_loadout(
    world: &SimWorld,
    loadouts: &StartLoadoutCatalog,
    player: &PlayerSetup,
    start_loc: &MapStartLocation,
) -> Result<ResolvedLoadout, ScenarioError> {
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

    Ok(ResolvedLoadout {
        town_hall: th_arch.unit_type_id,
        hall_size: (hall_w, hall_h),
        starting_resources: loadout.starting_resources,
        units: units_to_spawn,
    })
}

/// Step 5, a loadout's `starting_resources` replace the start location's player's default bank.
///
/// # Errors
/// [`ScenarioError::InvalidMap`] with [`MapValidationError::PlayerSlotOutOfRange`] if the start
/// location's slot has no player storage. Step 1's `validate_map` already rejects such a map,
/// so this only restates that check where the slot is used.
fn apply_starting_bank(
    world: &mut SimWorld,
    start_loc: &MapStartLocation,
    starting: Option<StartingResources>,
) -> Result<(), ScenarioError> {
    let Some(slot) = PlayerSlot::new(start_loc.player_slot) else {
        return Err(ScenarioError::InvalidMap(vec![
            MapValidationError::PlayerSlotOutOfRange {
                id: start_loc.id,
                slot: start_loc.player_slot,
            },
        ]));
    };
    if let Some(start) = starting {
        let resources = &mut world.player_resources[slot];
        resources.minerals = start.minerals;
        resources.gas = start.gas;
    }
    Ok(())
}

/// Step 5, the town hall: centres it in the start footprint, `top_left + (fp - hall) / 2`,
/// and spawns it for the start's player.
///
/// # Errors
/// [`ScenarioError::Spawn`] when the structure cannot be placed or its anchor cell read.
fn place_town_hall(
    world: &mut SimWorld,
    start_loc: &MapStartLocation,
    resolved: &ResolvedLoadout,
) -> Result<PlacedHall, ScenarioError> {
    let (hall_w, hall_h) = resolved.hall_size;
    let fp_w = u32::from(start_loc.footprint.0);
    let fp_h = u32::from(start_loc.footprint.1);

    // Center the town hall in the start footprint.
    let (tl_x, tl_y) = start_loc.cell;
    let hall_cell_x = tl_x + (fp_w - hall_w) / 2;
    let hall_cell_y = tl_y + (fp_h - hall_h) / 2;

    world.spawn_structure(
        resolved.town_hall,
        PlayerId(start_loc.player_slot),
        hall_cell_x,
        hall_cell_y,
    )?;

    let anchor_cliff_level = world
        .map
        .cell(hall_cell_x, hall_cell_y)
        .map_err(|e| ScenarioError::Spawn(SimCoreError::MapError(e.to_string())))?
        .cliff_level;

    let cell_coordinate = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
    Ok(PlacedHall {
        x_min: cell_coordinate(hall_cell_x),
        x_max: cell_coordinate(hall_cell_x + hall_w - 1),
        y_min: cell_coordinate(hall_cell_y),
        y_max: cell_coordinate(hall_cell_y + hall_h - 1),
        cliff_level: anchor_cliff_level,
    })
}

/// Step 5, the units: spawns `units` for player `slot` on rings `d = 1, 2, 3` around `hall`,
/// each ring in [`ring_perimeter`] order, skipping cells that are off the map, on another
/// plateau, not ground-pathable, abyss, ramp, occupied by a structure, or in `claimed_cells`.
///
/// # Errors
/// [`ScenarioError::NotEnoughSpawnCells`] when the rings hold too few cells, and
/// [`ScenarioError::Spawn`] when a unit cannot be spawned.
fn place_ring_units(
    world: &mut SimWorld,
    slot: u8,
    hall: &PlacedHall,
    units: &[UnitTypeId],
    claimed_cells: &mut BTreeSet<(u32, u32)>,
) -> Result<(), ScenarioError> {
    let map_w = i32::try_from(world.map.width()).unwrap_or(i32::MAX);
    let map_h = i32::try_from(world.map.height()).unwrap_or(i32::MAX);
    let mut spawned_count = 0;
    let total_needed = units.len();

    for d in 1..=3 {
        if spawned_count >= total_needed {
            break;
        }

        let x0 = hall.x_min - d;
        let x1 = hall.x_max + d;
        let y0 = hall.y_min - d;
        let y1 = hall.y_max + d;

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
            if cell.cliff_level != hall.cliff_level {
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
            let unit_type = units[spawned_count];
            world.spawn_unit(unit_type, PlayerId(slot), pos)?;
            claimed_cells.insert((ux, uy));
            spawned_count += 1;
        }
    }

    if spawned_count < total_needed {
        return Err(ScenarioError::NotEnoughSpawnCells { slot });
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{
        build_world_with_catalogs, ring_perimeter, LoadoutUnit, PlayerSetup, StartLoadout,
    };
    use super::{ScenarioError, StartLoadoutCatalog};
    use sim_map::{CellFlags, CliffLevel, MapCellSpec, MapDocument, MapMetadata, MapStartLocation};
    use sim_map::{MapEntityId, TerrainKind};
    use sim_types::{ShapeKind, SimScalar, SimVec2, UnitTypeId};

    /// A 32x32 open grass map with a 5x5 start at `(6, 6)` for slot 0 and an unused one.
    fn open_document() -> MapDocument {
        let cell = MapCellSpec {
            level: CliffLevel(0),
            ramp: None,
            terrain: TerrainKind::Grass,
            flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
        };
        let start = |id: u32, player_slot: u8, at: u32| MapStartLocation {
            id: MapEntityId(id),
            player_slot,
            cell: (at, at),
            footprint: (5, 5),
        };
        MapDocument {
            metadata: MapMetadata {
                title: "Placement".to_string(),
                author: "scenario tests".to_string(),
                description: "Open ground with one used start".to_string(),
                theme: "orbital_foundry".to_string(),
            },
            width: 32,
            height: 32,
            cells: vec![cell; 32 * 32],
            resources: Vec::new(),
            watchtowers: Vec::new(),
            start_locations: vec![start(1, 0, 6), start(2, 1, 24)],
        }
    }

    /// Pins where units land: the hall's rings nearest first, each in perimeter order, every
    /// unit in loadout order, at the centre of its cell. Every name comes from the catalog.
    #[test]
    fn units_fill_the_nearest_ring_first_in_perimeter_and_loadout_order(
    ) -> Result<(), ScenarioError> {
        let missing = |what: &str| ScenarioError::UnknownSlug(what.to_string());
        let catalog = crate::test_support::shipped_catalogs().units;
        let hall = catalog
            .archetypes()
            .find(|a| {
                matches!(a.shape, ShapeKind::BuildingDome { footprint_w, footprint_h }
                    if footprint_w <= 5 && footprint_h <= 5)
            })
            .ok_or_else(|| missing("a dome of at most 5x5 cells"))?;
        let mut mobile = catalog
            .archetypes()
            .filter(|a| matches!(a.shape, ShapeKind::UnitSphere));
        let (first, second) = mobile
            .next()
            .zip(mobile.next())
            .ok_or_else(|| missing("two mobile units"))?;
        let loadouts = StartLoadoutCatalog {
            loadouts: vec![StartLoadout {
                slug: "placement".to_string(),
                display_name: "Placement".to_string(),
                editor_default: true,
                town_hall: hall.slug.clone(),
                units: vec![
                    LoadoutUnit {
                        slug: first.slug.clone(),
                        count: 1,
                    },
                    LoadoutUnit {
                        slug: second.slug.clone(),
                        count: 24,
                    },
                ],
                starting_resources: None,
            }],
        };
        let players = [PlayerSetup {
            player_slot: 0,
            loadout_slug: "placement".to_string(),
        }];
        let catalogs = crate::test_support::shipped_catalogs();
        let world = build_world_with_catalogs(
            &open_document(),
            &loadouts,
            &players,
            7,
            catalogs.units,
            catalogs.upgrades,
        )?;

        let placed = world
            .map
            .structures()
            .values()
            .next()
            .ok_or_else(|| missing("the placed town hall"))?;
        let left = i32::try_from(placed.top_left.0).unwrap_or(i32::MAX);
        let bottom = i32::try_from(placed.top_left.1).unwrap_or(i32::MAX);
        let right = left + i32::from(placed.width) - 1;
        let top = bottom + i32::from(placed.height) - 1;
        let centre = |c: i32| SimScalar::from_i32(c) + SimScalar::HALF;
        let expected: Vec<(UnitTypeId, SimVec2)> = (1..=3)
            .flat_map(|d| ring_perimeter(left - d, right + d, bottom - d, top + d))
            .take(25)
            .enumerate()
            .map(|(index, (x, y))| {
                let unit = if index == 0 { first } else { second };
                (unit.unit_type_id, SimVec2::new(centre(x), centre(y)))
            })
            .collect();
        let spawned: Vec<(UnitTypeId, SimVec2)> = world
            .entities
            .values()
            .filter(|e| !world.map.structures().contains_key(&e.id))
            .map(|e| (e.unit_type_id, e.position))
            .collect();
        assert_eq!(spawned, expected);
        // The first ring holds fewer than 25 cells, so the order of rings decides the result.
        let first_ring = 2 * (right - left + 3) + 2 * (top - bottom + 1);
        assert!(first_ring < 25, "first ring of {first_ring} cells");
        Ok(())
    }
}
