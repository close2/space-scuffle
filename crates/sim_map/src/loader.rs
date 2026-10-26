//! Map serialization, RON deserialization, and Tiled TMJ importer.
//!
//! This module provides full round-trip serialization of `GameMap` using the
//! human-readable `.scmap.ron` format and imports standard Tiled `.tmj` JSON maps.

use crate::error::{MapError, Result};
use crate::grid::{
    CellFlags, CliffLevel, GameMap, MapCell, RampDir, ResourceKind, ResourceNode, TerrainKind,
    Watchtower,
};
use serde::{Deserialize, Serialize};
use sim_types::{EntityId, SimScalar, SimVec2};

/// High-level serializable representation of a complete map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScMapData {
    /// Human-readable map name.
    pub name: String,
    /// Grid width in 1x1 cells.
    pub width: u32,
    /// Grid height in 1x1 cells.
    pub height: u32,
    /// Flat list of map cells in row-major order.
    pub cells: Vec<MapCell>,
    /// Natural resource nodes on the map.
    pub resources: Vec<ResourceNode>,
    /// Neutral watchtowers.
    pub watchtowers: Vec<Watchtower>,
}

#[derive(Debug, Deserialize)]
struct TiledMapJson {
    width: u32,
    height: u32,
    #[serde(default)]
    tilewidth: u32,
    #[serde(default)]
    tileheight: u32,
    #[serde(default)]
    layers: Vec<TiledLayerJson>,
}

#[derive(Debug, Deserialize)]
struct TiledLayerJson {
    name: String,
    #[serde(rename = "type")]
    layer_type: String,
    #[serde(default)]
    data: Vec<u32>,
    #[serde(default)]
    objects: Vec<TiledObjectJson>,
}

#[derive(Debug, Deserialize)]
struct TiledObjectJson {
    #[serde(default)]
    id: u32,
    #[serde(default)]
    name: String,
    #[serde(rename = "type", default)]
    obj_type: String,
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
    #[serde(default)]
    properties: Vec<TiledPropertyJson>,
}

#[derive(Debug, Deserialize)]
struct TiledPropertyJson {
    name: String,
    value: serde_json::Value,
}

impl GameMap {
    /// Converts this map into serializable `ScMapData`.
    #[must_use]
    pub fn to_data(&self) -> ScMapData {
        ScMapData {
            name: "ExportedMap".into(),
            width: self.width(),
            height: self.height(),
            cells: self.cells().to_vec(),
            resources: self.resources().to_vec(),
            watchtowers: self.watchtowers().to_vec(),
        }
    }

    /// Constructs a `GameMap` from `ScMapData`.
    ///
    /// # Errors
    /// Returns `Err(MapError::InvalidDimensions)` if dimensions do not match cell vector length.
    pub fn from_data(data: ScMapData) -> Result<Self> {
        Self::from_cells(
            data.width,
            data.height,
            data.cells,
            data.resources,
            data.watchtowers,
        )
    }

    /// Serializes this map into a formatted RON string.
    ///
    /// # Errors
    /// Returns `Err(MapError::RonSerializationError)` if serialization fails.
    pub fn to_ron(&self) -> Result<String> {
        let data = self.to_data();
        ron::ser::to_string_pretty(&data, ron::ser::PrettyConfig::default())
            .map_err(|e| MapError::RonSerializationError(e.to_string()))
    }

    /// Deserializes a map from a RON string.
    ///
    /// # Errors
    /// Returns `Err(MapError::RonDeserializationError)` if deserialization fails.
    pub fn from_ron(ron_str: &str) -> Result<Self> {
        let data: ScMapData =
            ron::from_str(ron_str).map_err(|e| MapError::RonDeserializationError(e.to_string()))?;
        Self::from_data(data)
    }

    /// Imports a map from a Tiled JSON (`.tmj`) string.
    ///
    /// # Errors
    /// Returns `Err(MapError::TiledParseError)` if parsing fails or layers are invalid.
    pub fn from_tiled_tmj(json_str: &str) -> Result<Self> {
        let parsed: TiledMapJson =
            serde_json::from_str(json_str).map_err(|e| MapError::TiledParseError(e.to_string()))?;

        if parsed.width == 0 || parsed.height == 0 {
            return Err(MapError::InvalidDimensions {
                width: parsed.width,
                height: parsed.height,
            });
        }

        let total_cells = (parsed.width as usize) * (parsed.height as usize);
        let mut cliffs = vec![CliffLevel::MIN; total_cells];
        let mut terrains = vec![TerrainKind::Grass; total_cells];
        let mut ramps: Vec<Option<RampDir>> = vec![None; total_cells];
        let mut flags = vec![CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE; total_cells];
        let mut has_flags_layer = false;

        let mut resources = Vec::new();
        let mut watchtowers = Vec::new();

        let tile_w = if parsed.tilewidth > 0 {
            parsed.tilewidth as f32
        } else {
            32.0
        };
        let tile_h = if parsed.tileheight > 0 {
            parsed.tileheight as f32
        } else {
            32.0
        };

        for layer in parsed.layers {
            match layer.layer_type.as_str() {
                "tilelayer" => {
                    let layer_data = layer.data;
                    if layer_data.len() != total_cells {
                        continue;
                    }
                    match layer.name.as_str() {
                        "Cliffs" => {
                            for (i, &val) in layer_data.iter().enumerate() {
                                cliffs[i] = CliffLevel((val.min(3)) as u8);
                            }
                        }
                        "Terrain" => {
                            for (i, &val) in layer_data.iter().enumerate() {
                                terrains[i] = match val {
                                    1 => TerrainKind::MetalPlate,
                                    2 => TerrainKind::Dirt,
                                    3 => TerrainKind::Abyss,
                                    _ => TerrainKind::Grass,
                                };
                            }
                        }
                        "Ramps" => {
                            for (i, &val) in layer_data.iter().enumerate() {
                                ramps[i] = match val {
                                    1 => Some(RampDir::North),
                                    2 => Some(RampDir::South),
                                    3 => Some(RampDir::East),
                                    4 => Some(RampDir::West),
                                    5 => Some(RampDir::NorthEast),
                                    6 => Some(RampDir::NorthWest),
                                    7 => Some(RampDir::SouthEast),
                                    8 => Some(RampDir::SouthWest),
                                    _ => None,
                                };
                            }
                        }
                        "Flags" => {
                            has_flags_layer = true;
                            for (i, &val) in layer_data.iter().enumerate() {
                                flags[i] = CellFlags::from_bits_truncate(val as u16);
                            }
                        }
                        _ => {}
                    }
                }
                "objectgroup" => {
                    for obj in layer.objects {
                        let kind_str = if obj.obj_type.is_empty() {
                            obj.name.as_str()
                        } else {
                            obj.obj_type.as_str()
                        };

                        let cell_x = (obj.x / tile_w).max(0.0) as u32;
                        let cell_y = (obj.y / tile_h).max(0.0) as u32;
                        let pos = SimVec2::new(
                            SimScalar::from_f32_lossy(obj.x / tile_w),
                            SimScalar::from_f32_lossy(obj.y / tile_h),
                        );

                        if kind_str == "Mineral" || kind_str == "Gas" {
                            let mut remaining = if kind_str == "Mineral" { 1500 } else { 2500 };
                            for prop in &obj.properties {
                                if prop.name == "remaining" {
                                    if let Some(num) = prop.value.as_u64() {
                                        remaining = num as u32;
                                    }
                                }
                            }
                            resources.push(ResourceNode {
                                pos,
                                kind: if kind_str == "Mineral" {
                                    ResourceKind::Mineral
                                } else {
                                    ResourceKind::Gas
                                },
                                remaining,
                                cell: (cell_x, cell_y),
                            });
                        } else if kind_str == "Watchtower" {
                            let mut capture_r = SimScalar::from_num(3);
                            let mut reveal_r = SimScalar::from_num(12);
                            for prop in &obj.properties {
                                if prop.name == "capture_radius" {
                                    if let Some(val) = prop.value.as_f64() {
                                        capture_r = SimScalar::from_f32_lossy(val as f32);
                                    }
                                } else if prop.name == "reveal_radius" {
                                    if let Some(val) = prop.value.as_f64() {
                                        reveal_r = SimScalar::from_f32_lossy(val as f32);
                                    }
                                }
                            }
                            watchtowers.push(Watchtower {
                                id: EntityId(obj.id),
                                pos,
                                capture_radius: capture_r,
                                reveal_radius: reveal_r,
                                controlling_player: None,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        let mut cells = Vec::with_capacity(total_cells);
        for i in 0..total_cells {
            let mut cell_flags = if has_flags_layer {
                flags[i]
            } else if ramps[i].is_some() {
                CellFlags::GROUND_PATHABLE | CellFlags::RAMP
            } else {
                CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE
            };

            if ramps[i].is_some() {
                cell_flags.insert(CellFlags::RAMP);
            }

            cells.push(MapCell {
                cliff_level: cliffs[i],
                ramp: ramps[i],
                terrain_kind: terrains[i],
                flags: cell_flags,
                occupying_structure: None,
            });
        }

        Self::from_cells(parsed.width, parsed.height, cells, resources, watchtowers)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_ron_round_trip() {
        let arena = GameMap::default_arena();
        let ron_str = arena.to_ron().expect("RON serialization should succeed");
        assert_ne!(ron_str, "");

        let loaded = GameMap::from_ron(&ron_str).expect("RON deserialization should succeed");
        assert_eq!(arena.width(), loaded.width());
        assert_eq!(arena.height(), loaded.height());
        assert_eq!(arena.cells().len(), loaded.cells().len());
        assert_eq!(arena.resources().len(), loaded.resources().len());
        assert_eq!(arena.watchtowers().len(), loaded.watchtowers().len());
    }

    #[test]
    fn test_invalid_ron_error() {
        let err = GameMap::from_ron("invalid ron {{{{");
        assert!(matches!(err, Err(MapError::RonDeserializationError(_))));
    }

    #[test]
    fn test_tmj_importer() {
        let tmj = r#"{
            "width": 2,
            "height": 2,
            "tilewidth": 32,
            "tileheight": 32,
            "layers": [
                {
                    "name": "Cliffs",
                    "type": "tilelayer",
                    "data": [0, 1, 2, 3]
                },
                {
                    "name": "Terrain",
                    "type": "tilelayer",
                    "data": [0, 1, 2, 3]
                },
                {
                    "name": "Ramps",
                    "type": "tilelayer",
                    "data": [0, 1, 0, 0]
                },
                {
                    "name": "Resources",
                    "type": "objectgroup",
                    "objects": [
                        {
                            "id": 1,
                            "name": "Mineral",
                            "type": "Mineral",
                            "x": 32.0,
                            "y": 0.0,
                            "properties": [
                                { "name": "remaining", "value": 1500 }
                            ]
                        }
                    ]
                },
                {
                    "name": "Watchtowers",
                    "type": "objectgroup",
                    "objects": [
                        {
                            "id": 2,
                            "name": "Tower",
                            "type": "Watchtower",
                            "x": 0.0,
                            "y": 32.0,
                            "properties": [
                                { "name": "capture_radius", "value": 3.0 },
                                { "name": "reveal_radius", "value": 12.0 }
                            ]
                        }
                    ]
                }
            ]
        }"#;

        let map = GameMap::from_tiled_tmj(tmj).expect("TMJ import should succeed");
        assert_eq!(map.width(), 2);
        assert_eq!(map.height(), 2);
        assert_eq!(map.cell(0, 0).unwrap().cliff_level, CliffLevel(0));
        assert_eq!(map.cell(1, 0).unwrap().cliff_level, CliffLevel(1));
        assert_eq!(map.cell(1, 0).unwrap().ramp, Some(RampDir::North));
        assert_eq!(map.resources().len(), 1);
        assert_eq!(map.watchtowers().len(), 1);
    }

    #[test]
    fn test_invalid_tmj_error() {
        let err = GameMap::from_tiled_tmj("not valid json");
        assert!(matches!(err, Err(MapError::TiledParseError(_))));
    }
}
