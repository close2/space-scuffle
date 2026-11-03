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
//! - Encoding never converts silently: `MapDocument::encode` and `MapDocument::from_game_map`
//!   share one per-cell check, so a cell either survives a save and load unchanged or the save
//!   fails with a typed error.
//! - Coordinate radii for watchtowers are stored in exact integer tenths of a world unit.
//! - Entity lists (`resources`, `watchtowers`, `start_locations`) are deterministically sorted by `MapEntityId`.
//! - Encoded bytes do not depend on the host: every line ends in `\n`, on every target.

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

/// Line terminator of every encoded `*.scmap.ron` document.
///
/// Invariant: a map is a data format, so its bytes must not depend on the host that wrote it.
/// `ron` defaults `PrettyConfig::new_line` to `\r\n` when built for Windows, which is why this
/// value is always set explicitly.
const MAP_RON_NEWLINE: &str = "\n";

/// The four flag bits an author may set; the flags layer stores exactly these as one hex digit.
const AUTHORABLE_FLAGS: CellFlags = CellFlags::BUILDABLE
    .union(CellFlags::GROUND_PATHABLE)
    .union(CellFlags::LOS_BLOCKER)
    .union(CellFlags::SPEED_ZONE);

/// Upper-case hexadecimal digits of the flags layer, indexed by the authorable bits (`0..=15`).
const FLAG_DIGITS: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F',
];

/// Checks one editable cell against everything format v1 cannot represent.
///
/// Shared by [`MapDocument::encode`] and [`MapDocument::from_game_map`], so the two cannot
/// drift. Checks, in order: cliff level within `0..=3`, a cardinal (or no) ramp, and no flag
/// bits outside [`AUTHORABLE_FLAGS`]. `(x, y)` are simulation cell coordinates, used only in
/// the error.
///
/// # Errors
/// [`MapFormatError::LevelOutOfRange`], [`MapFormatError::UnsupportedDiagonalRamp`] or
/// [`MapFormatError::NonAuthorableFlags`], for the first check that fails.
fn check_cell(x: u32, y: u32, spec: &MapCellSpec) -> Result<(), MapFormatError> {
    if spec.level > CliffLevel::MAX {
        return Err(MapFormatError::LevelOutOfRange {
            x,
            y,
            level: spec.level.0,
        });
    }
    if matches!(
        spec.ramp,
        Some(RampDir::NorthEast | RampDir::NorthWest | RampDir::SouthEast | RampDir::SouthWest)
    ) {
        return Err(MapFormatError::UnsupportedDiagonalRamp { x, y });
    }
    let extra = spec.flags.difference(AUTHORABLE_FLAGS);
    if !extra.is_empty() {
        return Err(MapFormatError::NonAuthorableFlags {
            x,
            y,
            bits: u32::from(extra.bits()),
        });
    }
    Ok(())
}

/// Applies the canonical `*.scmap.ron` layout to `base`, replacing whatever `base` chose.
///
/// `base` is a parameter so that tests can start from the Windows default on any host.
fn canonical_pretty_config(base: PrettyConfig) -> PrettyConfig {
    base.depth_limit(3)
        .indentor("    ".to_string())
        .struct_names(false)
        .new_line(MAP_RON_NEWLINE.to_string())
}

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

    /// A [`MapDocument`]'s cell vector does not hold exactly `width * height` cells, so it
    /// cannot be encoded row by row.
    #[error("document of {width}x{height} cells holds {found} cell specifications")]
    CellCountMismatch {
        /// Grid width in cells.
        width: u32,
        /// Grid height in cells.
        height: u32,
        /// Number of entries actually present in `MapDocument::cells`.
        found: usize,
    },

    /// A [`MapCellSpec`] carries flag bits outside the four authorable ones (`BUILDABLE`,
    /// `GROUND_PATHABLE`, `LOS_BLOCKER`, `SPEED_ZONE`). Derived bits (`RAMP`,
    /// `UNPASSABLE_CLIFF`) and runtime state (`OCCUPIED_BY_STRUCTURE`) are never authored.
    #[error("non-authorable flag bits {bits:#06x} in cell ({x}, {y})")]
    NonAuthorableFlags {
        /// X coordinate of cell.
        x: u32,
        /// Y coordinate of cell.
        y: u32,
        /// The offending bits, as the raw `CellFlags` value with the authorable bits removed.
        bits: u32,
    },

    /// Diagonal ramp encountered when converting from `GameMap` or encoding a [`MapDocument`]
    /// (format v1 supports cardinal ramps only).
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

    /// Resource or watchtower cell lies outside the map grid.
    #[error("entity {id:?} at cell ({x}, {y}) lies outside the map")]
    EntityOutOfBounds {
        /// Map entity identifier.
        id: MapEntityId,
        /// X coordinate of the entity cell.
        x: u32,
        /// Y coordinate of the entity cell.
        y: u32,
    },

    /// Watchtower identifier cannot be mapped into the simulation entity ID range.
    #[error("watchtower identifier {id:?} is too large for the simulation entity range")]
    EntityIdOutOfRange {
        /// Map entity identifier.
        id: MapEntityId,
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
    /// Visual and acoustic terrain theme slug (e.g. "`orbital_foundry`").
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
        ron::ser::to_string_pretty(self, canonical_pretty_config(PrettyConfig::new()))
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
        self.check_dimensions()?;
        self.check_row_shapes()?;
        self.check_layer_chars()?;
        let cells = self.assemble_cells()?;

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

    /// The four cell layers in the documented error order, each with its row strings.
    fn layers(&self) -> [(MapLayer, &Vec<String>); 4] {
        [
            (MapLayer::Levels, &self.levels),
            (MapLayer::Terrain, &self.terrain),
            (MapLayer::Ramps, &self.ramps),
            (MapLayer::Flags, &self.flags),
        ]
    }

    /// Decode stage 0: both dimensions lie in `MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION`.
    fn check_dimensions(&self) -> Result<(), MapFormatError> {
        if !(MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&self.width)
            || !(MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&self.height)
        {
            return Err(MapFormatError::DimensionsOutOfRange {
                width: self.width,
                height: self.height,
            });
        }
        Ok(())
    }

    /// Decode stage 1: every layer has `height` rows of `width` characters, in layer order.
    fn check_row_shapes(&self) -> Result<(), MapFormatError> {
        for (layer, rows) in self.layers() {
            let row_count = saturating_u32(rows.len());
            if row_count != self.height {
                return Err(MapFormatError::RowCount {
                    layer,
                    expected: self.height,
                    found: row_count,
                });
            }
            for (row_idx, row_str) in rows.iter().enumerate() {
                let columns = saturating_u32(row_str.chars().count());
                if columns != self.width {
                    return Err(MapFormatError::RowLength {
                        layer,
                        row: saturating_u32(row_idx),
                        expected: self.width,
                        found: columns,
                    });
                }
            }
        }
        Ok(())
    }

    /// Decode stage 2: every character is valid for its layer, checked layer by layer, top to
    /// bottom (row 0 to height-1), left to right (col 0 to width-1).
    ///
    /// Requires stage 1, which proved every layer has `height` rows.
    fn check_layer_chars(&self) -> Result<(), MapFormatError> {
        for (layer, rows) in self.layers() {
            let is_valid: fn(char) -> bool = match layer {
                MapLayer::Levels => |ch| matches!(ch, '0'..='3'),
                MapLayer::Terrain => |ch| terrain_from_char(ch).is_some(),
                MapLayer::Ramps => |ch| ramp_from_char(ch).is_some(),
                MapLayer::Flags => |ch| ch.is_ascii_hexdigit(),
            };
            for row_idx in 0..self.height {
                let y = self.height - 1 - row_idx;
                for (col_idx, ch) in rows[row_idx as usize].chars().enumerate() {
                    if !is_valid(ch) {
                        return Err(MapFormatError::InvalidCellChar {
                            layer,
                            x: saturating_u32(col_idx),
                            y,
                            ch,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Decode stage 3: assembles the `MapCellSpec` array in row-major order,
    /// `index = y * width + x`.
    ///
    /// Requires stages 1 and 2. The per-character conversions still return typed errors rather
    /// than trusting them.
    fn assemble_cells(&self) -> Result<Vec<MapCellSpec>, MapFormatError> {
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
                let invalid = |layer, ch| MapFormatError::InvalidCellChar { layer, x, y, ch };
                let level_char = level_chars[x as usize];
                let lvl = level_char
                    .to_digit(4)
                    .and_then(|digit| u8::try_from(digit).ok())
                    .ok_or_else(|| invalid(MapLayer::Levels, level_char))?;
                let terrain_char = terrain_chars[x as usize];
                let trn = terrain_from_char(terrain_char)
                    .ok_or_else(|| invalid(MapLayer::Terrain, terrain_char))?;
                let ramp_char = ramp_chars[x as usize];
                let rmp =
                    ramp_from_char(ramp_char).ok_or_else(|| invalid(MapLayer::Ramps, ramp_char))?;
                let flag_char = flag_chars[x as usize];
                let flag_val = flag_char
                    .to_digit(16)
                    .and_then(|digit| u16::try_from(digit).ok())
                    .ok_or_else(|| invalid(MapLayer::Flags, flag_char))?;
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
        Ok(cells)
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
    ///
    /// Everything is checked before anything is written, so the result either succeeds or
    /// names the first thing format v1 cannot represent. A document that
    /// [`MapFile::decode`] or [`MapDocument::from_game_map`] produced always encodes.
    ///
    /// # Errors
    /// - [`MapFormatError::DimensionsOutOfRange`] if width or height is outside
    ///   `MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION`.
    /// - [`MapFormatError::CellCountMismatch`] if `cells.len() != width * height`.
    /// - Per cell, in row-major order from `y = 0`, the first error of the checks
    ///   [`MapDocument::from_game_map`] also applies: [`MapFormatError::LevelOutOfRange`],
    ///   [`MapFormatError::UnsupportedDiagonalRamp`], [`MapFormatError::NonAuthorableFlags`].
    ///   `from_game_map`'s ramp- and abyss-flag consistency checks concern the derived `RAMP`
    ///   and `UNPASSABLE_CLIFF` bits, which a [`MapCellSpec`] cannot carry: they are
    ///   non-authorable, so they are rejected as [`MapFormatError::NonAuthorableFlags`].
    pub fn encode(&self) -> Result<MapFile, MapFormatError> {
        self.check_encodable()?;
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
                // Index and digit arithmetic are in range: `check_encodable` verified the cell
                // count and that every level lies in `0..=3`.
                let spec = &self.cells[(y * self.width + x) as usize];
                lvl_row.push(char::from(b'0' + spec.level.0));
                let trn_char = match spec.terrain {
                    TerrainKind::Grass => 'G',
                    TerrainKind::MetalPlate => 'M',
                    TerrainKind::Dirt => 'D',
                    TerrainKind::Abyss => 'A',
                };
                trn_row.push(trn_char);

                let rmp_char = match spec.ramp {
                    None => '.',
                    Some(RampDir::North) => 'N',
                    Some(RampDir::South) => 'S',
                    Some(RampDir::East) => 'E',
                    Some(RampDir::West) => 'W',
                    // `check_encodable` already rejected diagonals; this arm keeps the match
                    // total without a panic path and returns the same error.
                    Some(
                        RampDir::NorthEast
                        | RampDir::NorthWest
                        | RampDir::SouthEast
                        | RampDir::SouthWest,
                    ) => return Err(MapFormatError::UnsupportedDiagonalRamp { x, y }),
                };
                rmp_row.push(rmp_char);

                // `check_encodable` rejected every bit outside the low four, so the digit is
                // the whole flags value; the mask only keeps the index total.
                flg_row.push(FLAG_DIGITS[usize::from(spec.flags.bits() & 0x000F)]);
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

        Ok(MapFile {
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
        })
    }

    /// Encode stage 0: dimensions in range, exactly `width * height` cells, and every cell
    /// representable ([`check_cell`]), in row-major order from `y = 0`.
    fn check_encodable(&self) -> Result<(), MapFormatError> {
        let (width, height) = (self.width, self.height);
        if !(MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&width)
            || !(MIN_MAP_DIMENSION..=MAX_MAP_DIMENSION).contains(&height)
        {
            return Err(MapFormatError::DimensionsOutOfRange { width, height });
        }
        if self.cells.len() != (width as usize) * (height as usize) {
            return Err(MapFormatError::CellCountMismatch {
                width,
                height,
                found: self.cells.len(),
            });
        }
        // Rows of `width` cells, so `zip` with `0..height` and `0..width` visits every cell once.
        for (y, row) in (0..height).zip(self.cells.chunks_exact(width as usize)) {
            for (x, spec) in (0..width).zip(row) {
                check_cell(x, y, spec)?;
            }
        }
        Ok(())
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

    /// Returns the cell at a signed offset from `cell`, or `None` if that leaves the grid.
    #[must_use]
    pub fn offset_cell(&self, cell: (u32, u32), offset: (i32, i32)) -> Option<(u32, u32)> {
        let x = cell.0.checked_add_signed(offset.0)?;
        let y = cell.1.checked_add_signed(offset.1)?;
        (x < self.width && y < self.height).then_some((x, y))
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
        let mut flags = spec.flags & AUTHORABLE_FLAGS;
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
            self.require_cell_in_bounds(res.id, res.cell)?;
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
            self.require_cell_in_bounds(wt.id, wt.cell)?;
            let id = WATCHTOWER_ENTITY_ID_BASE
                .checked_add(wt.id.0)
                .ok_or(MapFormatError::EntityIdOutOfRange { id: wt.id })?;
            let center = SimVec2::new(
                SimScalar::from_num(wt.cell.0) + SimScalar::HALF,
                SimScalar::from_num(wt.cell.1) + SimScalar::HALF,
            );
            watchtowers.push(Watchtower {
                id: EntityId(id),
                pos: center,
                capture_radius: tenths_to_radius(wt.capture_radius_tenths),
                reveal_radius: tenths_to_radius(wt.reveal_radius_tenths),
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

                // Derived and runtime bits are checked against the `MapCell` below, so only the
                // authorable bits enter the spec, and `check_cell`'s flag check cannot fail here.
                let spec = MapCellSpec {
                    level: cell.cliff_level,
                    terrain: cell.terrain_kind,
                    ramp: cell.ramp,
                    flags: cell.flags & AUTHORABLE_FLAGS,
                };
                check_cell(x, y, &spec)?;

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

                cells.push(spec);
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
            let (Some(capture_radius_tenths), Some(reveal_radius_tenths)) = (
                radius_to_tenths(wt.capture_radius),
                radius_to_tenths(wt.reveal_radius),
            ) else {
                return Err(MapFormatError::NonTenthRadius { x: cx, y: cy });
            };

            watchtowers.push(MapWatchtower {
                id: MapEntityId(next_id),
                cell: (cx, cy),
                capture_radius_tenths,
                reveal_radius_tenths,
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
        MapEntityId(max_id.saturating_add(1))
    }

    fn require_cell_in_bounds(
        &self,
        id: MapEntityId,
        cell: (u32, u32),
    ) -> Result<(), MapFormatError> {
        if cell.0 < self.width && cell.1 < self.height {
            Ok(())
        } else {
            Err(MapFormatError::EntityOutOfBounds {
                id,
                x: cell.0,
                y: cell.1,
            })
        }
    }
}

/// Decodes one terrain layer character.
fn terrain_from_char(ch: char) -> Option<TerrainKind> {
    match ch {
        'G' => Some(TerrainKind::Grass),
        'M' => Some(TerrainKind::MetalPlate),
        'D' => Some(TerrainKind::Dirt),
        'A' => Some(TerrainKind::Abyss),
        _ => None,
    }
}

/// Decodes one ramp layer character. The outer `None` means the character is invalid.
#[expect(
    clippy::option_option,
    reason = "'.' is a valid cell with no ramp, which differs from an invalid character"
)]
fn ramp_from_char(ch: char) -> Option<Option<RampDir>> {
    match ch {
        '.' => Some(None),
        'N' => Some(Some(RampDir::North)),
        'S' => Some(Some(RampDir::South)),
        'E' => Some(Some(RampDir::East)),
        'W' => Some(Some(RampDir::West)),
        _ => None,
    }
}

/// Converts a row or column count for error reporting, which needs no more than saturation.
fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Converts an authored radius in tenths of a world unit to world units.
fn tenths_to_radius(tenths: u16) -> SimScalar {
    SimScalar::from_num(tenths) / SimScalar::from_num(10)
}

/// Recovers the exact tenths value of a radius, or `None` if it is not one.
///
/// Most tenths have no exact Q32.32 representation, so `tenths / 10` is truncated and multiplying
/// back by ten falls just short of a whole number. The test is therefore not whether the scaled
/// value is whole: round to the nearest tenth, then require [`tenths_to_radius`] to reproduce the
/// input bit for bit.
fn radius_to_tenths(radius: SimScalar) -> Option<u16> {
    let nearest = (radius * SimScalar::from_num(10) + SimScalar::HALF).saturating_to_u32();
    let tenths = u16::try_from(nearest).ok()?;
    (tenths_to_radius(tenths) == radius).then_some(tenths)
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_pretty_config, check_cell, MapCellSpec, MapFile, MapFormatError, MAP_RON_NEWLINE,
    };
    use crate::grid::{CellFlags, CliffLevel, RampDir, TerrainKind};
    use ron::ser::PrettyConfig;

    const SHOWCASE: &str = include_str!("../../../assets/maps/showcase_canyon.scmap.ron");

    /// The config `PrettyConfig::new()` returns on Windows, reproduced on any host.
    ///
    /// On Linux the `ron` default is already `\n`, so only this base can show the override working.
    fn windows_default() -> PrettyConfig {
        PrettyConfig::new().new_line("\r\n".to_string())
    }

    #[test]
    fn canonical_config_replaces_a_windows_default_newline() {
        assert_eq!(MAP_RON_NEWLINE, "\n");
        assert_eq!(
            canonical_pretty_config(windows_default()).new_line,
            MAP_RON_NEWLINE
        );
    }

    #[test]
    fn encoding_from_a_windows_default_reproduces_the_committed_bytes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let file = MapFile::from_ron_str(SHOWCASE)?;
        let encoded =
            ron::ser::to_string_pretty(&file, canonical_pretty_config(windows_default()))?;
        assert!(!encoded.contains('\r'));
        assert_eq!(encoded, SHOWCASE);
        Ok(())
    }

    #[test]
    fn to_ron_string_writes_no_carriage_return() -> Result<(), MapFormatError> {
        let encoded = MapFile::from_ron_str(SHOWCASE)?.to_ron_string()?;
        assert!(encoded.contains('\n'));
        assert!(!encoded.contains('\r'));
        Ok(())
    }

    /// A cell that breaks all three rules reports them in `from_game_map`'s order, one at a
    /// time as each is fixed, and an authorable cell passes.
    #[test]
    fn check_cell_reports_level_then_diagonal_then_flags() {
        let mut spec = MapCellSpec {
            level: CliffLevel(7),
            terrain: TerrainKind::Grass,
            ramp: Some(RampDir::SouthWest),
            flags: CellFlags::BUILDABLE | CellFlags::RAMP | CellFlags::UNPASSABLE_CLIFF,
        };
        assert!(matches!(
            check_cell(2, 9, &spec),
            Err(MapFormatError::LevelOutOfRange {
                x: 2,
                y: 9,
                level: 7
            })
        ));
        spec.level = CliffLevel::MAX;
        assert!(matches!(
            check_cell(2, 9, &spec),
            Err(MapFormatError::UnsupportedDiagonalRamp { x: 2, y: 9 })
        ));
        spec.ramp = Some(RampDir::South);
        assert!(matches!(
            check_cell(2, 9, &spec),
            Err(MapFormatError::NonAuthorableFlags {
                x: 2,
                y: 9,
                bits: 0x0060
            })
        ));
        spec.flags = CellFlags::BUILDABLE
            | CellFlags::GROUND_PATHABLE
            | CellFlags::LOS_BLOCKER
            | CellFlags::SPEED_ZONE;
        assert!(check_cell(2, 9, &spec).is_ok());
    }
}
