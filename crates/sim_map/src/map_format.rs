//! Versioned, deterministic, human-readable map format (`*.scmap.ron`) and document model.
//!
//! # Responsibility
//! This module provides the authoritative serialization and deserialization layer for
//! Space Scuffle map documents (`MapFile` v1). It acts as the bridge between human-editable
//! or editor-authored map files on disk and the simulation runtime map (`GameMap`).
//!
//! # Invariants Maintained
//! - Format version 1 is strictly enforced.
//! - Cell grid layers are stored as row strings with North at row 0 (`y = height - 1 - row_idx`).
//! - Derived flags (`RAMP`, `UNPASSABLE_CLIFF`) are never persisted in the authorable flags layer.
//! - Runtime state (`OCCUPIED_BY_STRUCTURE`) is strictly forbidden in map files.
//! - Coordinate radii for watchtowers are stored in exact integer tenths of a world unit.
//! - Entity lists (`resources`, `watchtowers`, `start_locations`) are deterministically sorted by `MapEntityId`.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use sim_types::{EntityId, SimScalar, SimVec2};

use crate::error::MapError;
use crate::grid::{
    CellFlags, CliffLevel, GameMap, MapCell, RampDir, ResourceKind, ResourceNode, TerrainKind,
    Watchtower,
};

/// Current format version for `*.scmap.ron` files.
pub const CURRENT_MAP_FORMAT_VERSION: u32 = 1;

/// Minimum allowed map dimension (width or height) in cells.
pub const MIN_MAP_DIMENSION: u32 = 16;

/// Maximum allowed map dimension (width or height) in cells.
pub const MAX_MAP_DIMENSION: u32 = 256;

/// Base integer offset for watchtower entity IDs to keep them disjoint from simulation IDs.
pub const WATCHTOWER_ENTITY_ID_BASE: u32 = 1_000_000;

/// Map layer identifier for structured decoding error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapLayer {
    /// Cliff elevation layer ('0'..'3').
    Levels,
    /// Surface terrain material layer ('G', 'M', 'D', 'A').
    Terrain,
    /// Ramp ascending direction layer ('.', 'N', 'S', 'E', 'W').
    Ramps,
    /// Authorable cell flags layer (single hex digit '0'..'F').
    Flags,
}

/// Strongly-typed error representing failure to parse, validate, decode, or encode map files.
#[derive(Debug, thiserror::Error)]
pub enum MapFormatError {
    /// File system or I/O error occurred during file access.
    #[error("I/O error at {path:?}: {source}")]
    Io {
        /// Target file path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// RON syntax error during parsing.
    #[error("RON syntax error: {0}")]
    RonSyntax(String),

    /// RON serialization failure.
    #[error("RON serialization error: {0}")]
    RonSerialize(String),

    /// File format version is unsupported by this binary.
    #[error("unsupported map format version {found}; supported version is {supported}")]
    UnsupportedVersion {
        /// Format version discovered in file.
        found: u32,
        /// Highest supported format version.
        supported: u32,
    },

    /// Grid width or height is outside supported bounds `[MIN_MAP_DIMENSION, MAX_MAP_DIMENSION]`.
    #[error("map dimensions ({width}x{height}) out of supported range [{MIN_MAP_DIMENSION}..={MAX_MAP_DIMENSION}]")]
    DimensionsOutOfRange {
        /// Grid width in cells.
        width: u32,
        /// Grid height in cells.
        height: u32,
    },

    /// Layer string array length does not match map height.
    #[error("layer {layer:?} has {found} rows, expected {expected}")]
    RowCount {
        /// Layer being parsed.
        layer: MapLayer,
        /// Expected row count (map height).
        expected: u32,
        /// Actual row count discovered.
        found: u32,
    },

    /// Layer row string character count does not match map width.
    #[error("layer {layer:?} row {row} has {found} columns, expected {expected}")]
    RowLength {
        /// Layer being parsed.
        layer: MapLayer,
        /// Row index within layer (0 = North).
        row: u32,
        /// Expected column count (map width).
        expected: u32,
        /// Actual column count discovered.
        found: u32,
    },

    /// Invalid character in layer string.
    #[error("invalid character '{ch}' in layer {layer:?} at cell ({x}, {y})")]
    InvalidCellChar {
        /// Layer containing the invalid character.
        layer: MapLayer,
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell (simulation space).
        y: u32,
        /// Character encountered.
        ch: char,
    },

    /// Cliff plateau level is outside valid range `0..=3`.
    #[error("cliff level {level} out of range 0..=3 at cell ({x}, {y})")]
    LevelOutOfRange {
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell.
        y: u32,
        /// Raw level value encountered.
        level: u8,
    },

    /// Diagonal ramp encountered when converting from `GameMap` (format v1 supports cardinal ramps only).
    #[error("unsupported diagonal ramp at cell ({x}, {y})")]
    UnsupportedDiagonalRamp {
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell.
        y: u32,
    },

    /// Cell contains runtime state (`OCCUPIED_BY_STRUCTURE`) during export.
    #[error("runtime structure occupancy state present in cell ({x}, {y})")]
    RuntimeStateInCell {
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell.
        y: u32,
    },

    /// Ramp field and cell flags disagreement in `GameMap`.
    #[error("inconsistent ramp flag at cell ({x}, {y})")]
    InconsistentRampFlag {
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell.
        y: u32,
    },

    /// Abyss terrain and `UNPASSABLE_CLIFF` flag disagreement in `GameMap`.
    #[error("inconsistent abyss flag at cell ({x}, {y})")]
    InconsistentAbyssFlag {
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell.
        y: u32,
    },

    /// Watchtower radius is not an exact integer multiple of 0.1 world units.
    #[error("watchtower radius at cell ({x}, {y}) is not an exact tenth of a world unit")]
    NonTenthRadius {
        /// X coordinate of watchtower cell.
        x: u32,
        /// Y coordinate of watchtower cell.
        y: u32,
    },

    /// Low-level map error from `GameMap` operations.
    #[error("map error: {0}")]
    GameMap(#[from] MapError),
}

/// Metadata header describing the map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MapMetadata {
    /// Human-readable map title.
    pub title: String,
    /// Author or organization name.
    pub author: String,
    /// Detailed description of the map layout and mechanics.
    pub description: String,
    /// Visual and acoustic terrain theme slug (e.g. "orbital_foundry").
    pub theme: String,
}

/// Unique entity identifier for entities placed in a map document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MapEntityId(pub u32);

/// Natural resource node placed on a map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapResource {
    /// Unique identifier across map entities.
    pub id: MapEntityId,
    /// Mineral crystal or gas geyser.
    pub kind: ResourceKind,
    /// Discrete grid cell coordinate `(x, y)`.
    pub cell: (u32, u32),
    /// Harvestable quantity of the resource.
    pub amount: u32,
}

/// Neutral watchtower placed on a map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapWatchtower {
    /// Unique identifier across map entities.
    pub id: MapEntityId,
    /// Discrete grid cell coordinate `(x, y)`.
    pub cell: (u32, u32),
    /// Capture proximity radius in tenths of a world unit (e.g. 30 = 3.0 world units).
    pub capture_radius_tenths: u16,
    /// Vision reveal radius in tenths of a world unit (e.g. 120 = 12.0 world units).
    pub reveal_radius_tenths: u16,
}

/// Player starting base foundation location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapStartLocation {
    /// Unique identifier across map entities.
    pub id: MapEntityId,
    /// Player slot index (0..7).
    pub player_slot: u8,
    /// Top-left (minimum X, minimum Y) cell coordinate.
    pub cell: (u32, u32),
    /// Reserved building footprint size `(width, height)` in cells.
    pub footprint: (u8, u8),
}

/// On-disk serialized representation of a Space Scuffle map (`MapFile` v1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapFile {
    /// Schema format version (must be 1).
    pub format_version: u32,
    /// Human-readable map metadata.
    pub metadata: MapMetadata,
    /// Map width in cells.
    pub width: u32,
    /// Map height in cells.
    pub height: u32,
    /// Cliff elevation layer strings (row 0 = north, '0'..'3').
    pub levels: Vec<String>,
    /// Surface terrain type layer strings ('G', 'M', 'D', 'A').
    pub terrain: Vec<String>,
    /// Ramp ascending direction strings ('.', 'N', 'S', 'E', 'W').
    pub ramps: Vec<String>,
    /// Authorable flag hex digit strings ('0'..'F').
    pub flags: Vec<String>,
    /// Natural harvestable resource nodes.
    pub resources: Vec<MapResource>,
    /// Neutral vision watchtowers.
    pub watchtowers: Vec<MapWatchtower>,
    /// Player spawn and base locations.
    pub start_locations: Vec<MapStartLocation>,
}

/// Decoded specification of a single cell within an editable `MapDocument`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapCellSpec {
    /// Cliff plateau elevation.
    pub level: CliffLevel,
    /// Surface terrain material.
    pub terrain: TerrainKind,
    /// Cardinal ascending ramp direction if this cell is a ramp.
    pub ramp: Option<RampDir>,
    /// Only the 4 authorable bitflags (`BUILDABLE`, `GROUND_PATHABLE`, `LOS_BLOCKER`, `SPEED_ZONE`).
    pub flags: CellFlags,
}

/// In-memory editable map document used by the map editor and scenario builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapDocument {
    /// Map metadata header.
    pub metadata: MapMetadata,
    /// Map width in cells.
    pub width: u32,
    /// Map height in cells.
    pub height: u32,
    /// Row-major cell specifications (`index = y * width + x`).
    pub cells: Vec<MapCellSpec>,
    /// Natural resource nodes sorted by id.
    pub resources: Vec<MapResource>,
    /// Neutral observation watchtowers sorted by id.
    pub watchtowers: Vec<MapWatchtower>,
    /// Player starting locations sorted by id.
    pub start_locations: Vec<MapStartLocation>,
}

impl MapFile {
    /// Parses a map file from a RON string, checking format syntax and version.
    ///
    /// # Errors
    /// Returns [`MapFormatError::RonSyntax`] if parsing fails, or
    /// [`MapFormatError::UnsupportedVersion`] if `format_version != CURRENT_MAP_FORMAT_VERSION`.
    pub fn from_ron_str(s: &str) -> Result<Self, MapFormatError> {
        let file: Self = ron::from_str(s).map_err(|e| MapFormatError::RonSyntax(e.to_string()))?;
        if file.format_version != CURRENT_MAP_FORMAT_VERSION {
            return Err(MapFormatError::UnsupportedVersion {
                found: file.format_version,
                supported: CURRENT_MAP_FORMAT_VERSION,
            });
        }
        Ok(file)
    }

    /// Serializes this map file to a formatted, human-readable RON string.
    ///
    /// # Errors
    /// Returns [`MapFormatError::RonSerialize`] on serialization failure.
    pub fn to_ron_string(&self) -> Result<String, MapFormatError> {
        let pretty = PrettyConfig::new()
            .depth_limit(3)
            .indentor("    ".to_string())
            .struct_names(false);
        ron::ser::to_string_pretty(self, pretty)
            .map_err(|e| MapFormatError::RonSerialize(e.to_string()))
    }

    /// Loads and parses a map file from the specified path.
    ///
    /// # Errors
    /// Returns [`MapFormatError::Io`] if reading fails, or format errors if content is invalid.
    pub fn load(path: &Path) -> Result<Self, MapFormatError> {
        let content = std::fs::read_to_string(path).map_err(|e| MapFormatError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        Self::from_ron_str(&content)
    }

    /// Atomically saves this map file to disk by writing to a temporary file,
    /// flushing with `sync_all`, and renaming over the destination path.
    ///
    /// # Errors
    /// Returns [`MapFormatError::Io`] on write or rename failure.
    pub fn save_atomic(&self, path: &Path) -> Result<(), MapFormatError> {
        let s = self.to_ron_string()?;
        let mut tmp_path_str = path.as_os_str().to_os_string();
        tmp_path_str.push(".tmp");
        let tmp_path = PathBuf::from(tmp_path_str);

        {
            let mut file = File::create(&tmp_path).map_err(|e| MapFormatError::Io {
                path: tmp_path.clone(),
                source: e,
            })?;
            file.write_all(s.as_bytes())
                .map_err(|e| MapFormatError::Io {
                    path: tmp_path.clone(),
                    source: e,
                })?;
            file.sync_all().map_err(|e| MapFormatError::Io {
                path: tmp_path.clone(),
                source: e,
            })?;
        }

        std::fs::rename(&tmp_path, path).map_err(|e| MapFormatError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        Ok(())
    }

    /// Decodes the serialized map file into an editable [`MapDocument`].
    ///
    /// Checks dimensions, row counts, row lengths, and character validity.
    /// Decoding reports the first error in deterministic order:
    /// layers (`Levels`, `Terrain`, `Ramps`, `Flags`), rows top to bottom, characters left to right.
    ///
    /// # Errors
    /// Returns a typed [`MapFormatError`] describing the first invalid property detected.
    pub fn decode(&self) -> Result<MapDocument, MapFormatError> {
        if !(MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&self.width)
            || !(MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&self.height)
        {
            return Err(MapFormatError::DimensionsOutOfRange {
                width: self.width,
                height: self.height,
            });
        }

        let layers = [
            (MapLayer::Levels, &self.levels),
            (MapLayer::Terrain, &self.terrain),
            (MapLayer::Ramps, &self.ramps),
            (MapLayer::Flags, &self.flags),
        ];

        // 1. Validate row counts and row lengths in layer order
        for &(layer, rows) in &layers {
            if rows.len() as u32 != self.height {
                return Err(MapFormatError::RowCount {
                    layer,
                    expected: self.height,
                    found: rows.len() as u32,
                });
            }
            for (row_idx, row_str) in rows.iter().enumerate() {
                if row_str.chars().count() as u32 != self.width {
                    return Err(MapFormatError::RowLength {
                        layer,
                        row: row_idx as u32,
                        expected: self.width,
                        found: row_str.chars().count() as u32,
                    });
                }
            }
        }

        // 2. Validate characters layer by layer, top to bottom (row 0 to height-1), left to right (col 0 to width-1)
        for row_idx in 0..self.height {
            let y = self.height - 1 - row_idx;
            for (col_idx, ch) in self.levels[row_idx as usize].chars().enumerate() {
                if !matches!(ch, '0'..='3') {
                    return Err(MapFormatError::InvalidCellChar {
                        layer: MapLayer::Levels,
                        x: col_idx as u32,
                        y,
                        ch,
                    });
                }
            }
        }

        for row_idx in 0..self.height {
            let y = self.height - 1 - row_idx;
            for (col_idx, ch) in self.terrain[row_idx as usize].chars().enumerate() {
                if !matches!(ch, 'G' | 'M' | 'D' | 'A') {
                    return Err(MapFormatError::InvalidCellChar {
                        layer: MapLayer::Terrain,
                        x: col_idx as u32,
                        y,
                        ch,
                    });
                }
            }
        }

        for row_idx in 0..self.height {
            let y = self.height - 1 - row_idx;
            for (col_idx, ch) in self.ramps[row_idx as usize].chars().enumerate() {
                if !matches!(ch, '.' | 'N' | 'S' | 'E' | 'W') {
                    return Err(MapFormatError::InvalidCellChar {
                        layer: MapLayer::Ramps,
                        x: col_idx as u32,
                        y,
                        ch,
                    });
                }
            }
        }

        for row_idx in 0..self.height {
            let y = self.height - 1 - row_idx;
            for (col_idx, ch) in self.flags[row_idx as usize].chars().enumerate() {
                if ch.to_digit(16).is_none() {
                    return Err(MapFormatError::InvalidCellChar {
                        layer: MapLayer::Flags,
                        x: col_idx as u32,
                        y,
                        ch,
                    });
                }
            }
        }

        // 3. Assemble MapCellSpec array in row-major order: index = y * width + x
        let total_cells = (self.width as usize) * (self.height as usize);
        let mut cells = Vec::with_capacity(total_cells);
        cells.resize(
            total_cells,
            MapCellSpec {
                level: CliffLevel::MIN,
                terrain: TerrainKind::Dirt,
                ramp: None,
                flags: CellFlags::empty(),
            },
        );

        for row_idx in 0..self.height {
            let y = self.height - 1 - row_idx;
            let level_chars: Vec<char> = self.levels[row_idx as usize].chars().collect();
            let terrain_chars: Vec<char> = self.terrain[row_idx as usize].chars().collect();
            let ramp_chars: Vec<char> = self.ramps[row_idx as usize].chars().collect();
            let flag_chars: Vec<char> = self.flags[row_idx as usize].chars().collect();

            for x in 0..self.width {
                let lvl = level_chars[x as usize] as u8 - b'0';
                let trn = match terrain_chars[x as usize] {
                    'G' => TerrainKind::Grass,
                    'M' => TerrainKind::MetalPlate,
                    'D' => TerrainKind::Dirt,
                    'A' => TerrainKind::Abyss,
                    _ => unreachable!(),
                };
                let rmp = match ramp_chars[x as usize] {
                    '.' => None,
                    'N' => Some(RampDir::North),
                    'S' => Some(RampDir::South),
                    'E' => Some(RampDir::East),
                    'W' => Some(RampDir::West),
                    _ => unreachable!(),
                };
                let flag_val = flag_chars[x as usize].to_digit(16).unwrap_or(0) as u16;
                let flg = CellFlags::from_bits_truncate(flag_val);

                let cell_idx = (y * self.width + x) as usize;
                cells[cell_idx] = MapCellSpec {
                    level: CliffLevel(lvl),
                    terrain: trn,
                    ramp: rmp,
                    flags: flg,
                };
            }
        }

        let mut resources = self.resources.clone();
        resources.sort_by_key(|r| r.id);

        let mut watchtowers = self.watchtowers.clone();
        watchtowers.sort_by_key(|w| w.id);

        let mut start_locations = self.start_locations.clone();
        start_locations.sort_by_key(|s| s.id);

        Ok(MapDocument {
            metadata: self.metadata.clone(),
            width: self.width,
            height: self.height,
            cells,
            resources,
            watchtowers,
            start_locations,
        })
    }
}

impl MapDocument {
    /// Constructs a blank map document filled with level 1 dirt, buildable & ground pathable flags.
    ///
    /// # Errors
    /// Returns [`MapFormatError::DimensionsOutOfRange`] if width or height is outside `16..=256`.
    pub fn new_blank(
        width: u32,
        height: u32,
        metadata: MapMetadata,
    ) -> Result<Self, MapFormatError> {
        if !(MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&width)
            || !(MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&height)
        {
            return Err(MapFormatError::DimensionsOutOfRange { width, height });
        }

        let total = (width as usize) * (height as usize);
        let default_flags = CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE;
        let cells = vec![
            MapCellSpec {
                level: CliffLevel(1),
                terrain: TerrainKind::Dirt,
                ramp: None,
                flags: default_flags,
            };
            total
        ];

        Ok(Self {
            metadata,
            width,
            height,
            cells,
            resources: Vec::new(),
            watchtowers: Vec::new(),
            start_locations: Vec::new(),
        })
    }

    /// Encodes this editable document into a compact [`MapFile`] ready for serialization.
    #[must_use]
    pub fn encode(&self) -> MapFile {
        let mut levels = Vec::with_capacity(self.height as usize);
        let mut terrain = Vec::with_capacity(self.height as usize);
        let mut ramps = Vec::with_capacity(self.height as usize);
        let mut flags = Vec::with_capacity(self.height as usize);

        for row_idx in 0..self.height {
            let y = self.height - 1 - row_idx;
            let mut lvl_row = String::with_capacity(self.width as usize);
            let mut trn_row = String::with_capacity(self.width as usize);
            let mut rmp_row = String::with_capacity(self.width as usize);
            let mut flg_row = String::with_capacity(self.width as usize);

            for x in 0..self.width {
                let spec = &self.cells[(y * self.width + x) as usize];
                lvl_row.push((b'0' + spec.level.0) as char);
                let trn_char = match spec.terrain {
                    TerrainKind::Grass => 'G',
                    TerrainKind::MetalPlate => 'M',
                    TerrainKind::Dirt => 'D',
                    TerrainKind::Abyss => 'A',
                };
                trn_row.push(trn_char);

                let rmp_char = match spec.ramp {
                    Some(RampDir::North) => 'N',
                    Some(RampDir::South) => 'S',
                    Some(RampDir::East) => 'E',
                    Some(RampDir::West) => 'W',
                    None | Some(_) => '.',
                };
                rmp_row.push(rmp_char);

                let authorable_bits = (spec.flags.bits() & 0x000F) as u32;
                let flg_char = char::from_digit(authorable_bits, 16)
                    .unwrap_or('0')
                    .to_ascii_uppercase();
                flg_row.push(flg_char);
            }

            levels.push(lvl_row);
            terrain.push(trn_row);
            ramps.push(rmp_row);
            flags.push(flg_row);
        }

        let mut resources = self.resources.clone();
        resources.sort_by_key(|r| r.id);

        let mut watchtowers = self.watchtowers.clone();
        watchtowers.sort_by_key(|w| w.id);

        let mut start_locations = self.start_locations.clone();
        start_locations.sort_by_key(|s| s.id);

        MapFile {
            format_version: CURRENT_MAP_FORMAT_VERSION,
            metadata: self.metadata.clone(),
            width: self.width,
            height: self.height,
            levels,
            terrain,
            ramps,
            flags,
            resources,
            watchtowers,
            start_locations,
        }
    }

    /// Returns a reference to the cell specification at `(x, y)`, or `None` if out of bounds.
    #[must_use]
    pub fn cell(&self, x: u32, y: u32) -> Option<&MapCellSpec> {
        if x < self.width && y < self.height {
            Some(&self.cells[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    /// Returns a mutable reference to the cell specification at `(x, y)`, or `None` if out of bounds.
    #[must_use]
    pub fn cell_mut(&mut self, x: u32, y: u32) -> Option<&mut MapCellSpec> {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            Some(&mut self.cells[idx])
        } else {
            None
        }
    }

    /// Converts an editable [`MapCellSpec`] to an authoritative [`MapCell`],
    /// deriving `RAMP` and `UNPASSABLE_CLIFF` flags and ensuring runtime state is clear.
    #[must_use]
    pub fn to_map_cell(spec: &MapCellSpec) -> MapCell {
        let mut flags = spec.flags
            & (CellFlags::BUILDABLE
                | CellFlags::GROUND_PATHABLE
                | CellFlags::LOS_BLOCKER
                | CellFlags::SPEED_ZONE);
        if spec.ramp.is_some() {
            flags.insert(CellFlags::RAMP);
        }
        if spec.terrain == TerrainKind::Abyss {
            flags.insert(CellFlags::UNPASSABLE_CLIFF);
        }

        MapCell {
            cliff_level: spec.level,
            ramp: spec.ramp,
            terrain_kind: spec.terrain,
            flags,
            occupying_structure: None,
        }
    }

    /// Converts this document to an authoritative simulation [`GameMap`].
    ///
    /// # Errors
    /// Returns [`MapFormatError::GameMap`] if `GameMap::from_cells` fails.
    pub fn to_game_map(&self) -> Result<GameMap, MapFormatError> {
        let total = (self.width as usize) * (self.height as usize);
        let mut cells = Vec::with_capacity(total);
        for spec in &self.cells {
            cells.push(Self::to_map_cell(spec));
        }

        let mut resources = Vec::with_capacity(self.resources.len());
        for res in &self.resources {
            let center = SimVec2::new(
                SimScalar::from_num(res.cell.0) + SimScalar::HALF,
                SimScalar::from_num(res.cell.1) + SimScalar::HALF,
            );
            resources.push(ResourceNode {
                pos: center,
                kind: res.kind,
                remaining: res.amount,
                cell: res.cell,
            });
        }

        let mut watchtowers = Vec::with_capacity(self.watchtowers.len());
        for wt in &self.watchtowers {
            let center = SimVec2::new(
                SimScalar::from_num(wt.cell.0) + SimScalar::HALF,
                SimScalar::from_num(wt.cell.1) + SimScalar::HALF,
            );
            let cap_rad = SimScalar::from_num(wt.capture_radius_tenths) / SimScalar::from_num(10);
            let rev_rad = SimScalar::from_num(wt.reveal_radius_tenths) / SimScalar::from_num(10);
            watchtowers.push(Watchtower {
                id: EntityId(WATCHTOWER_ENTITY_ID_BASE + wt.id.0),
                pos: center,
                capture_radius: cap_rad,
                reveal_radius: rev_rad,
                controlling_player: None,
            });
        }

        GameMap::from_cells(self.width, self.height, cells, resources, watchtowers)
            .map_err(MapFormatError::GameMap)
    }

    /// Converts an existing [`GameMap`] into an editable [`MapDocument`].
    ///
    /// Rejects diagonal ramps, runtime structure occupancy state, flag inconsistencies,
    /// out-of-range cliff levels, and non-tenth watchtower radii.
    ///
    /// # Errors
    /// Returns [`MapFormatError`] if the game map violates document invariants.
    pub fn from_game_map(map: &GameMap, metadata: MapMetadata) -> Result<Self, MapFormatError> {
        let width = map.width();
        let height = map.height();

        if !(MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&width)
            || !(MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&height)
        {
            return Err(MapFormatError::DimensionsOutOfRange { width, height });
        }

        let total = (width as usize) * (height as usize);
        let mut cells = Vec::with_capacity(total);

        for y in 0..height {
            for x in 0..width {
                let cell = map.cell(x, y).map_err(MapFormatError::GameMap)?;

                if cell.cliff_level.0 > 3 {
                    return Err(MapFormatError::LevelOutOfRange {
                        x,
                        y,
                        level: cell.cliff_level.0,
                    });
                }

                if let Some(ramp_dir) = cell.ramp {
                    if matches!(
                        ramp_dir,
                        RampDir::NorthEast
                            | RampDir::NorthWest
                            | RampDir::SouthEast
                            | RampDir::SouthWest
                    ) {
                        return Err(MapFormatError::UnsupportedDiagonalRamp { x, y });
                    }
                }

                if cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE)
                    || cell.occupying_structure.is_some()
                {
                    return Err(MapFormatError::RuntimeStateInCell { x, y });
                }

                if cell.ramp.is_some() != cell.flags.contains(CellFlags::RAMP) {
                    return Err(MapFormatError::InconsistentRampFlag { x, y });
                }

                let is_abyss = cell.terrain_kind == TerrainKind::Abyss;
                let has_unpassable = cell.flags.contains(CellFlags::UNPASSABLE_CLIFF);
                if is_abyss != has_unpassable {
                    return Err(MapFormatError::InconsistentAbyssFlag { x, y });
                }

                let authorable_flags = cell.flags
                    & (CellFlags::BUILDABLE
                        | CellFlags::GROUND_PATHABLE
                        | CellFlags::LOS_BLOCKER
                        | CellFlags::SPEED_ZONE);

                cells.push(MapCellSpec {
                    level: cell.cliff_level,
                    terrain: cell.terrain_kind,
                    ramp: cell.ramp,
                    flags: authorable_flags,
                });
            }
        }

        let mut next_id = 1u32;
        let mut resources = Vec::with_capacity(map.resources().len());
        for r in map.resources() {
            resources.push(MapResource {
                id: MapEntityId(next_id),
                kind: r.kind,
                cell: r.cell,
                amount: r.remaining,
            });
            next_id += 1;
        }

        let mut watchtowers = Vec::with_capacity(map.watchtowers().len());
        for wt in map.watchtowers() {
            let (cx, cy) = map.world_to_cell(wt.pos).map_err(MapFormatError::GameMap)?;
            let cap_scaled = wt.capture_radius * SimScalar::from_num(10);
            let rev_scaled = wt.reveal_radius * SimScalar::from_num(10);

            if cap_scaled.0.frac() != 0
                || cap_scaled < SimScalar::ZERO
                || cap_scaled > SimScalar::from_num(u16::MAX)
                || rev_scaled.0.frac() != 0
                || rev_scaled < SimScalar::ZERO
                || rev_scaled > SimScalar::from_num(u16::MAX)
            {
                return Err(MapFormatError::NonTenthRadius { x: cx, y: cy });
            }

            watchtowers.push(MapWatchtower {
                id: MapEntityId(next_id),
                cell: (cx, cy),
                capture_radius_tenths: cap_scaled.0.to_num::<u16>(),
                reveal_radius_tenths: rev_scaled.0.to_num::<u16>(),
            });
            next_id += 1;
        }

        Ok(Self {
            metadata,
            width,
            height,
            cells,
            resources,
            watchtowers,
            start_locations: Vec::new(),
        })
    }

    /// Allocates the next available entity ID for a new resource, watchtower, or start location.
    #[must_use]
    pub fn next_entity_id(&self) -> MapEntityId {
        let max_res = self.resources.iter().map(|r| r.id.0).max().unwrap_or(0);
        let max_wt = self.watchtowers.iter().map(|w| w.id.0).max().unwrap_or(0);
        let max_start = self
            .start_locations
            .iter()
            .map(|s| s.id.0)
            .max()
            .unwrap_or(0);
        let max_id = max_res.max(max_wt).max(max_start);
        MapEntityId(max_id + 1)
    }
}
