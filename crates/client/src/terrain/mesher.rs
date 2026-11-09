//! Pure CPU terrain mesher implementing dual-grid marching squares cliffs,
//! ramp quads with sim-matching heights, canonical wall profile rows with
//! deterministic jitter, and vertex ambient occlusion.
//!
//! # Architectural Responsibilities
//! - **Dual-Grid Marching Squares:** Evaluates cell centers as dual vertices and edge
//!   midpoints as contour crossings, producing exact cell-aligned cliff lines with 45° cuts.
//! - **Watertight Chunk Boundaries:** Boundary edges depend strictly on local cell corner
//!   levels, guaranteeing bit-identical vertex positions and normals on shared chunk seams.
//! - **Simulation Height Matching:** Ramp quads sample authoritative heights directly from
//!   `RampProfile::height_at`, ensuring the visual terrain surface equals the simulation altitude.
//! - **Zero Per-Frame Heap Allocations:** Steady-state mesh updates reuse [`MesherScratch`]
//!   and update `Mesh` vertex buffers in-place.
//!
//! # Coordinate Conventions
//! - Simulation X maps to Bevy X.
//! - Simulation Y maps to Bevy Z.
//! - Simulation altitude maps to Bevy Y.

use bevy::prelude::*;
use bevy::render::mesh::{MeshVertexAttribute, VertexAttributeValues};
use bevy::render::render_resource::VertexFormat;
use sim_map::grid::{GameMap, RampDir, RampProfile, TerrainKind, CLIFF_LEVEL_HEIGHT};
use sim_types::SimScalar;

/// Secondary UV channel attribute storing baked ambient occlusion and emissive mask:
/// `[ao, emissive_mask]`.
///
/// We use `ATTRIBUTE_UV_1` rather than vertex colors (`ATTRIBUTE_COLOR`) because Bevy's
/// `pbr_fragment.wgsl` unconditionally multiplies `base_color` by vertex color under `VERTEX_COLORS`.
pub const ATTRIBUTE_UV_1: MeshVertexAttribute =
    MeshVertexAttribute::new("Vertex_Uv1", 3, VertexFormat::Float32x2);

/// Number of dual cells per chunk dimension (16x16 dual cells per chunk).
pub const CHUNK_DUAL_CELLS: u32 = 16;

/// Horizontal inward bevel width at cliff rim tops in world units.
pub const RIM_WIDTH: f32 = 0.15;

/// Vertical drop from cliff plateau height to rim bottom in world units.
pub const RIM_DROP: f32 = 0.20;

/// Horizontal outward foot spread width at cliff bases in world units.
pub const FOOT_WIDTH: f32 = 0.22;

/// Vertical rise from lower plateau height to foot top in world units.
pub const FOOT_RISE: f32 = 0.28;

/// Vertical spacing between canonical wall profile rows in world units.
pub const WALL_ROW_SPACING: f32 = 0.75;

/// World altitude of abyss chasm wall bottoms.
pub const ABYSS_WALL_BOTTOM_Y: f32 = -6.0;

/// Horizontal displacement amplitude for canonical wall row pseudo-random jitter.
pub const WALL_JITTER_AMPLITUDE: f32 = 0.06;

/// Vertical height of ramp guard rails in world units.
pub const RAIL_HEIGHT: f32 = 0.30;

/// Horizontal width of ramp guard rails in world units.
pub const RAIL_WIDTH: f32 = 0.08;

/// Search radius in simulation cells for floor vertex ambient occlusion gathering.
pub const AO_RADIUS_CELLS: f32 = 1.5;

/// Maximum darkening strength applied by ambient occlusion.
pub const AO_STRENGTH: f32 = 0.45;

/// Ambient occlusion value applied at cliff wall feet.
pub const WALL_FOOT_AO: f32 = 0.55;

/// Maximum number of dirty terrain chunk meshes rebuilt per frame.
pub const MAX_CHUNK_REBUILDS_PER_FRAME: usize = 8;

/// Statistics gathered during chunk mesh generation for validation and test assertions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MeshStats {
    /// Number of floor vertices generated.
    pub floor_vertices: usize,
    /// Number of floor indices generated.
    pub floor_indices: usize,
    /// Number of wall segments generated.
    pub wall_segments: usize,
    /// Number of wall vertices generated.
    pub wall_vertices: usize,
    /// Number of wall indices generated.
    pub wall_indices: usize,
    /// Number of ramp surface vertices generated.
    pub ramp_vertices: usize,
    /// Number of guard rail vertices generated.
    pub rail_vertices: usize,
}

/// Reusable CPU buffer scratchpad for building terrain chunk meshes without allocations.
#[derive(Debug, Default, Clone, Resource)]
pub struct MesherScratch {
    /// World positions `[x, y, z]`.
    pub positions: Vec<[f32; 3]>,
    /// Vertex normals `[nx, ny, nz]`.
    pub normals: Vec<[f32; 3]>,
    /// Primary UV coordinates `[u, v]` (world xz).
    pub uv0: Vec<[f32; 2]>,
    /// Secondary UV channel `[ao, emissive_mask]`.
    pub uv1: Vec<[f32; 2]>,
    /// Triangle index buffer.
    pub indices: Vec<u32>,
}

impl MesherScratch {
    /// Clears all internal vertex and index buffers while retaining allocated capacity.
    pub fn clear(&mut self) {
        self.positions.clear();
        self.normals.clear();
        self.uv0.clear();
        self.uv1.clear();
        self.indices.clear();
    }
}

/// Surface classification of a cell for terrain mesh generation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SurfaceClass {
    /// Unpathable bottomless abyss or out-of-bounds void (numeric level -1).
    Void,
    /// Contiguous sloped ramp cell with its simulation profile.
    Ramp(RampProfile),
    /// Flat plateau cell at a discrete cliff level (0..=3).
    Plateau(i32),
}

impl SurfaceClass {}

/// Evaluates the surface classification of simulation cell `(cx, cy)`.
#[must_use]
pub fn surface_class(map: &GameMap, cx: i32, cy: i32) -> SurfaceClass {
    let Ok(ux) = u32::try_from(cx) else {
        return SurfaceClass::Void;
    };
    let Ok(uy) = u32::try_from(cy) else {
        return SurfaceClass::Void;
    };
    let Ok(cell) = map.cell(ux, uy) else {
        return SurfaceClass::Void;
    };
    if cell.terrain_kind == TerrainKind::Abyss {
        return SurfaceClass::Void;
    }
    if let Some(profile) = map.ramp_profile(ux, uy) {
        return SurfaceClass::Ramp(profile);
    }
    SurfaceClass::Plateau(i32::from(cell.cliff_level.0))
}

/// Marching levels of the four corner cells of dual cell `(di, dj)`, in corner order
/// bottom-left, bottom-right, top-right, top-left, and the corners that are ramp cells as a mask
/// in the marching-case bit order (bottom-left = 1, bottom-right = 2, top-right = 4,
/// top-left = 8).
///
/// A plateau corner has its cliff level and a void corner (abyss or off the map) has level -1.
/// A ramp corner spans a whole cliff level, so no level of its own is right; it takes its level
/// from its non-ramp neighbours in the dual cell (rule R4 of the module documentation): the
/// lowest of its non-ramp edge-adjacent corners, else its diagonal corner when that is not a
/// ramp, else -1 (a dual cell of four ramp corners emits nothing). At every threshold a ramp
/// corner so classifies like one of its edge-adjacent corners, and no marching contour crosses
/// the inside of a ramp quadrant.
#[must_use]
pub fn dual_corner_levels(map: &GameMap, di: u32, dj: u32) -> ([i32; 4], u8) {
    let cells = dual_corner_cells(di, dj);
    let mut base = [-1; 4];
    let mut ramp_mask = 0_u8;
    for (k, &(cx, cy)) in cells.iter().enumerate() {
        match surface_class(map, cx, cy) {
            SurfaceClass::Void => {}
            SurfaceClass::Plateau(level) => base[k] = level,
            SurfaceClass::Ramp(_) => ramp_mask |= 1 << k,
        }
    }
    let is_ramp = |k: usize| ramp_mask & (1 << k) != 0;
    let mut levels = base;
    for (k, level) in levels.iter_mut().enumerate() {
        if !is_ramp(k) {
            continue;
        }
        let lowest_adjacent = [(k + 1) % 4, (k + 3) % 4]
            .into_iter()
            .filter(|&a| !is_ramp(a))
            .map(|a| base[a])
            .min();
        let diagonal = (k + 2) % 4;
        *level = match lowest_adjacent {
            Some(adjacent) => adjacent,
            None if !is_ramp(diagonal) => base[diagonal],
            None => -1,
        };
    }
    (levels, ramp_mask)
}

/// Marching level of the floor that the dual grid draws in corner `corner` (0..4, bottom-left,
/// bottom-right, top-right, top-left) of dual cell `(di, dj)` next to the dual cell's centre:
/// the median of the corner's level and the levels of its two edge-adjacent corners (rule R3).
///
/// Marching squares cut a corner's quadrant only when the corner is lone, unlike both
/// edge-adjacent corners; the part of the quadrant at the dual cell's centre then belongs to
/// the region of its neighbours. The median is the one level that covers both cases. The ramp
/// pass compares a ramp cell's edges with this level, so the dual grid and the ramp pass agree
/// on every height step by construction. Level -1 is the void.
#[must_use]
pub fn dual_floor_level_beside(map: &GameMap, di: u32, dj: u32, corner: usize) -> i32 {
    let (levels, _) = dual_corner_levels(map, di, dj);
    let mut three = [
        levels[corner % 4],
        levels[(corner + 1) % 4],
        levels[(corner + 3) % 4],
    ];
    three.sort_unstable();
    three[1]
}

/// Cells `(x, y)` at the corners of dual cell `(di, dj)`, in corner order bottom-left,
/// bottom-right, top-right, top-left. Dual cell `(di, dj)` is centred on the shared corner
/// `(di, dj)` of these four cells.
fn dual_corner_cells(di: u32, dj: u32) -> [(i32, i32); 4] {
    let di_i = i32::try_from(di).unwrap_or(0);
    let dj_i = i32::try_from(dj).unwrap_or(0);
    [
        (di_i - 1, dj_i - 1),
        (di_i, dj_i - 1),
        (di_i, dj_i),
        (di_i - 1, dj_i),
    ]
}

/// Returns the world altitude `H(level)` for a given discrete cliff level.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
pub fn cliff_level_height(level: i32) -> f32 {
    if level < 0 {
        ABYSS_WALL_BOTTOM_Y
    } else {
        level as f32 * CLIFF_LEVEL_HEIGHT as f32
    }
}

/// Integer hash function based on Jarzynski & Olano 2020 (`pcg2d`).
#[must_use]
pub fn pcg2d(v: (u32, u32)) -> (u32, u32) {
    let mut qx = v.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let mut qy = v.1.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    qx = qx.wrapping_add(qy.wrapping_mul(1_664_525));
    qy = qy.wrapping_add(qx.wrapping_mul(1_664_525));
    qx ^= qx >> 16;
    qy ^= qy >> 16;
    qx = qx.wrapping_add(qy.wrapping_mul(1_664_525));
    qy = qy.wrapping_add(qx.wrapping_mul(1_664_525));
    (qx ^ (qx >> 16), qy ^ (qy >> 16))
}

/// Computes a pseudo-random floating point value in `[0.0, 1.0]` for 3D integer coordinates.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
pub fn hash_i32_3(qx: i32, qz: i32, qy: i32) -> f32 {
    let (h1, _) = pcg2d((qx as u32, qz as u32));
    let (h2, _) = pcg2d((h1, qy as u32));
    h2 as f32 / 4_294_967_295.0
}

/// Edge midpoint crossings on a dual cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeMidpoint {
    /// Bottom edge midpoint between BL and BR: `(0.0, -0.5)`.
    Bottom,
    /// Right edge midpoint between BR and TR: `(+0.5, 0.0)`.
    Right,
    /// Top edge midpoint between TL and TR: `(0.0, +0.5)`.
    Top,
    /// Left edge midpoint between BL and TL: `(-0.5, 0.0)`.
    Left,
}

/// Dual cell corner identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DualCorner {
    /// Bottom-Left corner: `[-0.5, -0.5]`.
    BottomLeft,
    /// Bottom-Right corner: `[+0.5, -0.5]`.
    BottomRight,
    /// Top-Right corner: `[+0.5, +0.5]`.
    TopRight,
    /// Top-Left corner: `[-0.5, +0.5]`.
    TopLeft,
}

/// Vertex reference in a dual cell marching squares region polygon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DualVertexRef {
    /// Dual cell corner vertex (never receives rim inset).
    Corner(DualCorner),
    /// Dual cell edge midpoint crossing (contour vertex; receives rim inset if rim present).
    Midpoint(EdgeMidpoint),
}

/// Directed contour wall segment between two edge midpoints on a dual cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContourSegment {
    /// Starting edge midpoint crossing.
    pub start: EdgeMidpoint,
    /// Ending edge midpoint crossing.
    pub end: EdgeMidpoint,
}

/// Marching squares case definition containing convex region polygon vertices (CCW)
/// and directed contour wall segments.
#[derive(Debug, Clone, Copy)]
pub struct MarchingCase {
    /// Vertices of the region polygon in CCW order seen from +Y.
    pub region: &'static [DualVertexRef],
    /// Directed contour segments producing cliff wall quad strips.
    pub segments: &'static [ContourSegment],
}

// 16-entry lookup table for marching squares on cell centers with edge-midpoint crossings.
// Corner bit flags: BL = 1, BR = 2, TR = 4, TL = 8.
// Region polygons are strictly convex, oriented CCW seen from +Y.
// Saddles 5 and 10 resolve high-connected (both high corners joined).
const MARCHING_CASES: [MarchingCase; 16] = [
    // Case 0: (0000) None high
    MarchingCase {
        region: &[],
        segments: &[],
    },
    // Case 1: (0001) BL high
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::BottomLeft),
            DualVertexRef::Midpoint(EdgeMidpoint::Bottom),
            DualVertexRef::Midpoint(EdgeMidpoint::Left),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Bottom,
            end: EdgeMidpoint::Left,
        }],
    },
    // Case 2: (0010) BR high
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::BottomRight),
            DualVertexRef::Midpoint(EdgeMidpoint::Right),
            DualVertexRef::Midpoint(EdgeMidpoint::Bottom),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Right,
            end: EdgeMidpoint::Bottom,
        }],
    },
    // Case 3: (0011) BL and BR high (horizontal half quad)
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::BottomLeft),
            DualVertexRef::Corner(DualCorner::BottomRight),
            DualVertexRef::Midpoint(EdgeMidpoint::Right),
            DualVertexRef::Midpoint(EdgeMidpoint::Left),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Right,
            end: EdgeMidpoint::Left,
        }],
    },
    // Case 4: (0100) TR high
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::TopRight),
            DualVertexRef::Midpoint(EdgeMidpoint::Top),
            DualVertexRef::Midpoint(EdgeMidpoint::Right),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Top,
            end: EdgeMidpoint::Right,
        }],
    },
    // Case 5: (0101) BL and TR high (Saddle, high-connected convex hexagon)
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::BottomLeft),
            DualVertexRef::Midpoint(EdgeMidpoint::Bottom),
            DualVertexRef::Midpoint(EdgeMidpoint::Right),
            DualVertexRef::Corner(DualCorner::TopRight),
            DualVertexRef::Midpoint(EdgeMidpoint::Top),
            DualVertexRef::Midpoint(EdgeMidpoint::Left),
        ],
        segments: &[
            ContourSegment {
                start: EdgeMidpoint::Bottom,
                end: EdgeMidpoint::Right,
            },
            ContourSegment {
                start: EdgeMidpoint::Top,
                end: EdgeMidpoint::Left,
            },
        ],
    },
    // Case 6: (0110) BR and TR high (vertical half quad)
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::BottomRight),
            DualVertexRef::Corner(DualCorner::TopRight),
            DualVertexRef::Midpoint(EdgeMidpoint::Top),
            DualVertexRef::Midpoint(EdgeMidpoint::Bottom),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Top,
            end: EdgeMidpoint::Bottom,
        }],
    },
    // Case 7: (0111) BL, BR, TR high (convex pentagon)
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::BottomLeft),
            DualVertexRef::Corner(DualCorner::BottomRight),
            DualVertexRef::Corner(DualCorner::TopRight),
            DualVertexRef::Midpoint(EdgeMidpoint::Top),
            DualVertexRef::Midpoint(EdgeMidpoint::Left),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Top,
            end: EdgeMidpoint::Left,
        }],
    },
    // Case 8: (1000) TL high
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::TopLeft),
            DualVertexRef::Midpoint(EdgeMidpoint::Left),
            DualVertexRef::Midpoint(EdgeMidpoint::Top),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Left,
            end: EdgeMidpoint::Top,
        }],
    },
    // Case 9: (1001) BL and TL high (vertical half quad)
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::BottomLeft),
            DualVertexRef::Midpoint(EdgeMidpoint::Bottom),
            DualVertexRef::Midpoint(EdgeMidpoint::Top),
            DualVertexRef::Corner(DualCorner::TopLeft),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Bottom,
            end: EdgeMidpoint::Top,
        }],
    },
    // Case 10: (1010) BR and TL high (Saddle, high-connected convex hexagon)
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::BottomRight),
            DualVertexRef::Midpoint(EdgeMidpoint::Right),
            DualVertexRef::Midpoint(EdgeMidpoint::Top),
            DualVertexRef::Corner(DualCorner::TopLeft),
            DualVertexRef::Midpoint(EdgeMidpoint::Left),
            DualVertexRef::Midpoint(EdgeMidpoint::Bottom),
        ],
        segments: &[
            ContourSegment {
                start: EdgeMidpoint::Right,
                end: EdgeMidpoint::Top,
            },
            ContourSegment {
                start: EdgeMidpoint::Left,
                end: EdgeMidpoint::Bottom,
            },
        ],
    },
    // Case 11: (1011) BL, BR, TL high (convex pentagon)
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::BottomLeft),
            DualVertexRef::Corner(DualCorner::BottomRight),
            DualVertexRef::Midpoint(EdgeMidpoint::Right),
            DualVertexRef::Midpoint(EdgeMidpoint::Top),
            DualVertexRef::Corner(DualCorner::TopLeft),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Right,
            end: EdgeMidpoint::Top,
        }],
    },
    // Case 12: (1100) TR and TL high (horizontal half quad)
    MarchingCase {
        region: &[
            DualVertexRef::Midpoint(EdgeMidpoint::Left),
            DualVertexRef::Midpoint(EdgeMidpoint::Right),
            DualVertexRef::Corner(DualCorner::TopRight),
            DualVertexRef::Corner(DualCorner::TopLeft),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Left,
            end: EdgeMidpoint::Right,
        }],
    },
    // Case 13: (1101) BL, TR, TL high (convex pentagon)
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::BottomLeft),
            DualVertexRef::Midpoint(EdgeMidpoint::Bottom),
            DualVertexRef::Midpoint(EdgeMidpoint::Right),
            DualVertexRef::Corner(DualCorner::TopRight),
            DualVertexRef::Corner(DualCorner::TopLeft),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Bottom,
            end: EdgeMidpoint::Right,
        }],
    },
    // Case 14: (1110) BR, TR, TL high (convex pentagon)
    MarchingCase {
        region: &[
            DualVertexRef::Midpoint(EdgeMidpoint::Left),
            DualVertexRef::Midpoint(EdgeMidpoint::Bottom),
            DualVertexRef::Corner(DualCorner::BottomRight),
            DualVertexRef::Corner(DualCorner::TopRight),
            DualVertexRef::Corner(DualCorner::TopLeft),
        ],
        segments: &[ContourSegment {
            start: EdgeMidpoint::Left,
            end: EdgeMidpoint::Bottom,
        }],
    },
    // Case 15: (1111) All high (full quad)
    MarchingCase {
        region: &[
            DualVertexRef::Corner(DualCorner::BottomLeft),
            DualVertexRef::Corner(DualCorner::BottomRight),
            DualVertexRef::Corner(DualCorner::TopRight),
            DualVertexRef::Corner(DualCorner::TopLeft),
        ],
        segments: &[],
    },
];

/// Evaluates ambient occlusion at a floor or ramp vertex.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
pub fn compute_floor_ao(map: &GameMap, vx: f32, vy: f32, vz: f32) -> f32 {
    let min_cx = (vx - AO_RADIUS_CELLS).floor().max(0.0) as u32;
    let max_cx = (vx + AO_RADIUS_CELLS).ceil().min((map.width() - 1) as f32) as u32;
    let min_cy = (vz - AO_RADIUS_CELLS).floor().max(0.0) as u32;
    let max_cy = (vz + AO_RADIUS_CELLS).ceil().min((map.height() - 1) as f32) as u32;

    let rad_sq = AO_RADIUS_CELLS * AO_RADIUS_CELLS;
    let mut total = 0;
    let mut occluding = 0;

    for cy in min_cy..=max_cy {
        for cx in min_cx..=max_cx {
            let center_x = cx as f32 + 0.5;
            let center_z = cy as f32 + 0.5;
            let d_sq = (center_x - vx) * (center_x - vx) + (center_z - vz) * (center_z - vz);
            if d_sq <= rad_sq {
                let Ok(cx_i) = i32::try_from(cx) else {
                    continue;
                };
                let Ok(cy_i) = i32::try_from(cy) else {
                    continue;
                };
                total += 1;
                let cell_h = match surface_class(map, cx_i, cy_i) {
                    SurfaceClass::Void => ABYSS_WALL_BOTTOM_Y,
                    SurfaceClass::Plateau(lvl) => cliff_level_height(lvl),
                    SurfaceClass::Ramp(prof) => {
                        prof.height_at(SimScalar::HALF, SimScalar::HALF).to_f32()
                    }
                };
                if cell_h > vy + 0.5 {
                    occluding += 1;
                }
            }
        }
    }

    if total == 0 {
        1.0
    } else {
        1.0 - AO_STRENGTH * (occluding as f32 / total as f32)
    }
}

/// A single row entry in a wall vertex profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallProfileRow {
    /// World altitude Y.
    pub y: f32,
    /// World XZ position.
    pub xz: Vec2,
    /// Vertex surface normal.
    pub normal: Vec3,
    /// Ambient occlusion factor.
    pub ao: f32,
}

/// Generates the wall profile rows for an edge midpoint crossing.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
pub fn generate_wall_profile(
    p: Vec2,
    d: Vec2,
    level_a: i32,
    level_b: i32,
    ta: i32,
    tb: i32,
    suppressed: bool,
) -> Vec<WallProfileRow> {
    let mut rows = Vec::with_capacity(16);

    let y_top = cliff_level_height(tb);
    let y_bottom = cliff_level_height(ta - 1);

    let rim_present = level_a == tb && !suppressed;
    let foot_present = level_b == ta - 1 && ta > 0 && !suppressed;

    let d3 = Vec3::new(d.x, 0.0, d.y);

    let norm_rim = (d3 * RIM_DROP + Vec3::Y * RIM_WIDTH).normalize();
    let norm_foot = (d3 * FOOT_RISE + Vec3::Y * FOOT_WIDTH).normalize();

    // 1. Rim top
    if rim_present {
        rows.push(WallProfileRow {
            y: y_top,
            xz: p - d * RIM_WIDTH,
            normal: norm_rim,
            ao: 1.0,
        });
        // 2. Rim bottom
        rows.push(WallProfileRow {
            y: y_top - RIM_DROP,
            xz: p,
            normal: (norm_rim + d3).normalize(),
            ao: 1.0,
        });
    }

    // Top bound for canonical rows
    let top_bound = if rim_present { y_top - RIM_DROP } else { y_top };

    // Bottom bound for canonical rows
    let bot_bound = if foot_present {
        y_bottom + FOOT_RISE
    } else {
        y_bottom
    };

    // 3. Canonical rows
    let max_k = (top_bound / WALL_ROW_SPACING).floor() as i32;
    let min_k = (bot_bound / WALL_ROW_SPACING).ceil() as i32;

    for k in (min_k..=max_k).rev() {
        let y_k = k as f32 * WALL_ROW_SPACING;
        if y_k < top_bound - 1e-4 && y_k > bot_bound + 1e-4 {
            let qx = (p.x * 64.0).round() as i32;
            let qz = (p.y * 64.0).round() as i32;
            let qy = (y_k * 64.0).round() as i32;
            let h = hash_i32_3(qx, qz, qy);
            let disp = WALL_JITTER_AMPLITUDE * (2.0 * h - 1.0);
            let t = ((y_k - y_bottom) / (y_top - y_bottom)).clamp(0.0, 1.0);
            let ao = WALL_FOOT_AO + (1.0 - WALL_FOOT_AO) * t;

            rows.push(WallProfileRow {
                y: y_k,
                xz: p + d * disp,
                normal: d3,
                ao,
            });
        }
    }

    // 4. Foot top & bottom or wall bottom
    if foot_present {
        let t_ft = ((y_bottom + FOOT_RISE - y_bottom) / (y_top - y_bottom)).clamp(0.0, 1.0);
        let ao_ft = WALL_FOOT_AO + (1.0 - WALL_FOOT_AO) * t_ft;
        rows.push(WallProfileRow {
            y: y_bottom + FOOT_RISE,
            xz: p,
            normal: (norm_foot + d3).normalize(),
            ao: ao_ft,
        });
        rows.push(WallProfileRow {
            y: y_bottom,
            xz: p + d * FOOT_WIDTH,
            normal: norm_foot,
            ao: WALL_FOOT_AO,
        });
    } else {
        rows.push(WallProfileRow {
            y: y_bottom,
            xz: p,
            normal: d3,
            ao: WALL_FOOT_AO,
        });
    }

    rows
}

/// Evaluates an interpolated wall profile row at a given height `target_y`.
fn sample_profile_at_height(profile: &[WallProfileRow], target_y: f32) -> WallProfileRow {
    if profile.is_empty() {
        return WallProfileRow {
            y: target_y,
            xz: Vec2::ZERO,
            normal: Vec3::Y,
            ao: 1.0,
        };
    }
    if target_y >= profile[0].y {
        return profile[0];
    }
    if target_y <= profile[profile.len() - 1].y {
        return profile[profile.len() - 1];
    }

    for i in 0..profile.len() - 1 {
        let r_top = profile[i];
        let r_bot = profile[i + 1];
        if target_y <= r_top.y && target_y >= r_bot.y {
            let span = r_top.y - r_bot.y;
            let t = if span.abs() < 1e-5 {
                0.0
            } else {
                (target_y - r_bot.y) / span
            };
            return WallProfileRow {
                y: target_y,
                xz: r_bot.xz.lerp(r_top.xz, t),
                normal: (r_bot.normal.lerp(r_top.normal, t)).normalize(),
                ao: r_bot.ao + (r_top.ao - r_bot.ao) * t,
            };
        }
    }

    profile[profile.len() - 1]
}

#[inline]
fn push_quad_oriented(
    positions: &[[f32; 3]],
    indices: &mut Vec<u32>,
    base: u32,
    expected_dir: Vec3,
) -> usize {
    let mut added = 0;
    let mut push_tri = |i0: u32, i1: u32, i2: u32| {
        let p0 = Vec3::from(positions[i0 as usize]);
        let p1 = Vec3::from(positions[i1 as usize]);
        let p2 = Vec3::from(positions[i2 as usize]);
        let cross = (p1 - p0).cross(p2 - p0);
        if cross.length_squared() > 1e-10 {
            if cross.dot(expected_dir) >= 0.0 {
                indices.push(i0);
                indices.push(i1);
                indices.push(i2);
            } else {
                indices.push(i0);
                indices.push(i2);
                indices.push(i1);
            }
            added += 3;
        }
    };

    push_tri(base, base + 1, base + 2);
    push_tri(base, base + 2, base + 3);
    added
}

/// Generates a chunk mesh for chunk `(chunk_x, chunk_y)` into the provided scratchpad.
///
/// The chunk covers dual cells `[chunk * CHUNK_DUAL_CELLS, (chunk + 1) * CHUNK_DUAL_CELLS)` on
/// each axis (clamped to the `(width + 1) x (height + 1)` dual grid) and the simulation cells of
/// the same index range (clamped to the map). Geometry is emitted in two ordered sections, and the
/// order is part of the output contract (index order, and so the mesh bytes, depend on it):
///
/// 1. `emit_dual_cell` for every dual cell, row by row: cliff plateaus, cliff walls and the
///    low floor of each dual cell.
/// 2. `emit_ramp_cell` for every ramp cell, row by row: the ramp surface, its side walls and
///    its guard rails.
///
/// Returns statistical metrics [`MeshStats`] for test verification.
pub fn build_chunk_mesh(
    map: &GameMap,
    chunk_x: u32,
    chunk_y: u32,
    scratch: &mut MesherScratch,
) -> MeshStats {
    scratch.clear();
    let mut stats = MeshStats::default();

    let width = map.width();
    let height = map.height();

    // 1. Dual grid marching squares
    let dual_x_start = chunk_x * CHUNK_DUAL_CELLS;
    let dual_x_end = ((chunk_x + 1) * CHUNK_DUAL_CELLS).min(width + 1);
    let dual_y_start = chunk_y * CHUNK_DUAL_CELLS;
    let dual_y_end = ((chunk_y + 1) * CHUNK_DUAL_CELLS).min(height + 1);
    for dj in dual_y_start..dual_y_end {
        for di in dual_x_start..dual_x_end {
            emit_dual_cell(map, di, dj, scratch, &mut stats);
        }
    }

    // 2. Ramp surface quads, side walls, and guard rails
    let cell_x_start = chunk_x * CHUNK_DUAL_CELLS;
    let cell_x_end = ((chunk_x + 1) * CHUNK_DUAL_CELLS).min(width);
    let cell_y_start = chunk_y * CHUNK_DUAL_CELLS;
    let cell_y_end = ((chunk_y + 1) * CHUNK_DUAL_CELLS).min(height);
    for cy in cell_y_start..cell_y_end {
        for cx in cell_x_start..cell_x_end {
            if let Some(profile) = map.ramp_profile(cx, cy) {
                emit_ramp_cell(map, cx, cy, profile, scratch, &mut stats);
            }
        }
    }

    stats
}

/// Marching levels, geometry and wall suppression of one dual cell.
///
/// Dual cell `(di, dj)` is centred on the shared corner of simulation cells
/// `(di - 1, dj - 1)` (bottom-left), `(di, dj - 1)` (bottom-right), `(di, dj)` (top-right) and
/// `(di - 1, dj)` (top-left). Positions are world XZ in world units.
#[derive(Debug, Clone, Copy)]
struct DualCell {
    /// Marching level of the bottom-left, bottom-right, top-right and top-left corner cells
    /// ([`dual_corner_levels`]).
    l_bl: i32,
    l_br: i32,
    l_tr: i32,
    l_tl: i32,
    /// Lowest and highest of the four corner levels.
    lo: i32,
    hi: i32,
    /// Corner positions.
    p_bl: Vec2,
    p_br: Vec2,
    p_tr: Vec2,
    p_tl: Vec2,
    /// Edge midpoint positions.
    p_b: Vec2,
    p_r: Vec2,
    p_t: Vec2,
    p_l: Vec2,
    /// Centre of the dual cell: the shared corner of its four cells.
    centre: Vec2,
    /// Corners that are ramp cells, in the marching-case bit order.
    ramp_mask: u8,
    /// Whether each edge, in bottom, right, top, left order, lies on a ramp cell's edge: one of
    /// its two corner cells is a ramp. A contour midpoint there gets no rim, foot or jitter, and
    /// a wall along it belongs to the ramp (rules R1, R2 and R6).
    suppressed: [bool; 4],
}

/// Samples the four corner cells of dual cell `(di, dj)` and lays out its geometry.
#[expect(
    clippy::cast_precision_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
fn dual_cell_corners(map: &GameMap, di: u32, dj: u32) -> DualCell {
    let ([l_bl, l_br, l_tr, l_tl], ramp_mask) = dual_corner_levels(map, di, dj);
    let is_ramp = |k: u8| ramp_mask & (1 << k) != 0;

    let center_x = di as f32;
    let center_z = dj as f32;

    DualCell {
        l_bl,
        l_br,
        l_tr,
        l_tl,
        lo: l_bl.min(l_br).min(l_tr).min(l_tl),
        hi: l_bl.max(l_br).max(l_tr).max(l_tl),
        p_bl: Vec2::new(center_x - 0.5, center_z - 0.5),
        p_br: Vec2::new(center_x + 0.5, center_z - 0.5),
        p_tr: Vec2::new(center_x + 0.5, center_z + 0.5),
        p_tl: Vec2::new(center_x - 0.5, center_z + 0.5),
        p_b: Vec2::new(center_x, center_z - 0.5),
        p_r: Vec2::new(center_x + 0.5, center_z),
        p_t: Vec2::new(center_x, center_z + 0.5),
        p_l: Vec2::new(center_x - 0.5, center_z),
        centre: Vec2::new(center_x, center_z),
        ramp_mask,
        suppressed: [
            is_ramp(0) || is_ramp(1),
            is_ramp(1) || is_ramp(2),
            is_ramp(2) || is_ramp(3),
            is_ramp(3) || is_ramp(0),
        ],
    }
}

impl DualCell {
    fn corner_coord(&self, c: DualCorner) -> Vec2 {
        match c {
            DualCorner::BottomLeft => self.p_bl,
            DualCorner::BottomRight => self.p_br,
            DualCorner::TopRight => self.p_tr,
            DualCorner::TopLeft => self.p_tl,
        }
    }

    fn midpoint_suppressed(&self, m: EdgeMidpoint) -> bool {
        match m {
            EdgeMidpoint::Bottom => self.suppressed[0],
            EdgeMidpoint::Right => self.suppressed[1],
            EdgeMidpoint::Top => self.suppressed[2],
            EdgeMidpoint::Left => self.suppressed[3],
        }
    }

    fn midpoint_coord(&self, m: EdgeMidpoint) -> Vec2 {
        match m {
            EdgeMidpoint::Bottom => self.p_b,
            EdgeMidpoint::Right => self.p_r,
            EdgeMidpoint::Top => self.p_t,
            EdgeMidpoint::Left => self.p_l,
        }
    }

    /// Edge outward axis unit vector `d = unit(B - A)` at threshold `t`, pointing from the high
    /// side `A` of the edge to its low side `B`.
    fn midpoint_d(&self, m: EdgeMidpoint, t: i32) -> Vec2 {
        match m {
            EdgeMidpoint::Bottom => {
                if self.l_bl >= t {
                    Vec2::new(1.0, 0.0)
                } else {
                    Vec2::new(-1.0, 0.0)
                }
            }
            EdgeMidpoint::Right => {
                if self.l_br >= t {
                    Vec2::new(0.0, 1.0)
                } else {
                    Vec2::new(0.0, -1.0)
                }
            }
            EdgeMidpoint::Top => {
                if self.l_tl >= t {
                    Vec2::new(1.0, 0.0)
                } else {
                    Vec2::new(-1.0, 0.0)
                }
            }
            EdgeMidpoint::Left => {
                if self.l_bl >= t {
                    Vec2::new(0.0, 1.0)
                } else {
                    Vec2::new(0.0, -1.0)
                }
            }
        }
    }

    /// Levels `(a, b)` of the two corners of an edge at threshold `t`: `a` on the high side.
    fn midpoint_levels(&self, m: EdgeMidpoint, t: i32) -> (i32, i32) {
        let (first, second) = match m {
            EdgeMidpoint::Bottom => (self.l_bl, self.l_br),
            EdgeMidpoint::Right => (self.l_br, self.l_tr),
            EdgeMidpoint::Top => (self.l_tl, self.l_tr),
            EdgeMidpoint::Left => (self.l_bl, self.l_tl),
        };
        if first >= t {
            (first, second)
        } else {
            (second, first)
        }
    }

    /// Marching squares case index at threshold `t` (bit set where the corner level is `>= t`).
    fn case_for_t(&self, t: i32) -> usize {
        let mut idx = 0;
        if self.l_bl >= t {
            idx |= 1;
        }
        if self.l_br >= t {
            idx |= 2;
        }
        if self.l_tr >= t {
            idx |= 4;
        }
        if self.l_tl >= t {
            idx |= 8;
        }
        idx
    }

    /// The wall profile of a contour midpoint for the threshold run `ta..=tb`. A midpoint on a
    /// ramp edge has neither rim nor foot, and no jitter, so that the wall ends exactly on the
    /// ramp edge (rule R6).
    fn midpoint_profile(&self, m: EdgeMidpoint, ta: i32, tb: i32) -> Vec<WallProfileRow> {
        let (la, lb) = self.midpoint_levels(m, tb);
        let p = self.midpoint_coord(m);
        let suppressed = self.midpoint_suppressed(m);
        let mut rows = generate_wall_profile(p, self.midpoint_d(m, tb), la, lb, ta, tb, suppressed);
        if suppressed {
            for row in &mut rows {
                row.xz = p;
            }
        }
        rows
    }

    /// The wall profile at the dual cell's centre for the half of a straight contour that ends
    /// at `kept`: the profile of `kept` without rim, foot or jitter (rule R6).
    fn centre_profile(&self, kept: EdgeMidpoint, ta: i32, tb: i32) -> Vec<WallProfileRow> {
        let (la, lb) = self.midpoint_levels(kept, tb);
        let mut rows =
            generate_wall_profile(self.centre, self.midpoint_d(kept, tb), la, lb, ta, tb, true);
        for row in &mut rows {
            row.xz = self.centre;
        }
        rows
    }
}

/// Section 1 of [`build_chunk_mesh`] for one dual cell: for every run of thresholds that share
/// a marching case, the raised plateau polygon at the run's top level and its cliff walls; then
/// the low floor polygon at the cell's lowest level.
///
/// A dual cell with a ramp corner classifies that corner from its neighbours
/// ([`dual_corner_levels`]), clips its floors to its non-ramp quadrants and leaves the ramp's
/// edges to the ramp pass (rules R1, R4 to R6); one with four ramp corners emits nothing. A dual
/// cell without a ramp corner is emitted exactly as before those rules existed.
fn emit_dual_cell(
    map: &GameMap,
    di: u32,
    dj: u32,
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) {
    let cell = dual_cell_corners(map, di, dj);
    if cell.ramp_mask == 0x0F {
        return;
    }
    let (lo, hi) = (cell.lo, cell.hi);

    // Evaluate threshold runs for t in (lo + 1)..=hi
    if hi > lo {
        let mut t = lo + 1;
        while t <= hi {
            let curr_case = cell.case_for_t(t);
            let ta = t;
            let mut tb = t;
            while tb < hi && cell.case_for_t(tb + 1) == curr_case {
                tb += 1;
            }

            if curr_case != 0 && curr_case != 15 {
                let mcase = &MARCHING_CASES[curr_case];

                // Emit region polygon as floor at H(tb), contour vertices inset by the rim
                let polygon = region_polygon(&cell, mcase.region, |vref| match vref {
                    DualVertexRef::Corner(c) => cell.corner_coord(c),
                    DualVertexRef::Midpoint(m) => {
                        let (la, _) = cell.midpoint_levels(m, tb);
                        let rim_present = la == tb && !cell.midpoint_suppressed(m);
                        let base_coord = cell.midpoint_coord(m);
                        if rim_present {
                            let d = cell.midpoint_d(m, tb);
                            base_coord - d * RIM_WIDTH
                        } else {
                            base_coord
                        }
                    }
                });
                emit_dual_floor(map, &cell, &polygon, cliff_level_height(tb), scratch, stats);

                emit_cliff_walls(&cell, mcase.segments, ta, tb, scratch, stats);
            }

            t = tb + 1;
        }
    }

    // PERFORMANCE RATIONALE:
    // Why: Emitting a full floor quad at every level of a cliff cell would draw low ground hidden
    // under the higher plateau, overdraw that grows with the number of cliff cells on screen.
    // What: A flat dual cell (hi == lo) emits its full quad; a cliff cell (hi > lo) emits only the
    // low-region polygon (marching cases 1..14) for the threshold run, at H(lo).
    // Trade-off: A case lookup per cliff cell instead of one quad, in exchange for no hidden
    // fragments and watertight chunk boundaries.
    if lo >= 0 {
        let low_case = if hi > lo {
            (!cell.case_for_t(lo + 1)) & 0x0F
        } else {
            15
        };

        if low_case != 0 {
            let region = MARCHING_CASES[low_case].region;
            let polygon = region_polygon(&cell, region, |vref| match vref {
                DualVertexRef::Corner(c) => cell.corner_coord(c),
                DualVertexRef::Midpoint(m) => cell.midpoint_coord(m),
            });
            emit_dual_floor(map, &cell, &polygon, cliff_level_height(lo), scratch, stats);
        }
    }
}

/// Most vertices a floor polygon of one dual cell can have: a marching region has at most six,
/// a ramp-aware cell adds the centre once, and each of at most two clipping half-planes adds one.
const FLOOR_POLYGON_CAPACITY: usize = 12;

/// A convex floor polygon in world XZ, counter-clockwise seen from +Y.
// PERFORMANCE RATIONALE:
// Why: Floors are built and clipped for every dual cell of every rebuilt chunk; a `Vec` per
// polygon would allocate in the terrain rebuild path.
// What: A fixed-capacity array on the stack, sized by the bound in `FLOOR_POLYGON_CAPACITY`.
// Trade-off: A hard vertex bound that the marching table and the clipping rules must respect,
// in exchange for no heap allocation per polygon.
#[derive(Debug, Clone, Copy)]
struct FloorPolygon {
    points: [Vec2; FLOOR_POLYGON_CAPACITY],
    len: usize,
}

impl FloorPolygon {
    const EMPTY: Self = Self {
        points: [Vec2::ZERO; FLOOR_POLYGON_CAPACITY],
        len: 0,
    };

    fn push(&mut self, p: Vec2) {
        debug_assert!(
            self.len < FLOOR_POLYGON_CAPACITY,
            "invariant: a dual-cell floor polygon has at most {FLOOR_POLYGON_CAPACITY} vertices"
        );
        if self.len < FLOOR_POLYGON_CAPACITY {
            self.points[self.len] = p;
            self.len += 1;
        }
    }

    fn points(&self) -> &[Vec2] {
        &self.points[..self.len]
    }

    /// Sutherland–Hodgman clip against the half-plane `normal · (p - origin) <= 0`. `normal` has
    /// components in `{-1, 0, 1}`. A new vertex on an axis-aligned line is snapped onto it, and
    /// each edge is cut from its own start, so two clips of one polygon against opposite
    /// half-planes produce bit-identical vertices on the shared line.
    fn clip(&self, normal: Vec2, origin: Vec2) -> Self {
        let side = |p: Vec2| normal.x * (p.x - origin.x) + normal.y * (p.y - origin.y);
        let mut out = Self::EMPTY;
        for i in 0..self.len {
            let prev = self.points[(i + self.len - 1) % self.len];
            let cur = self.points[i];
            let (s_prev, s_cur) = (side(prev), side(cur));
            if (s_prev < 0.0 && s_cur > 0.0) || (s_prev > 0.0 && s_cur < 0.0) {
                let mut q = prev + (cur - prev) * (s_prev / (s_prev - s_cur));
                if normal.x.abs() < 0.5 {
                    q.y = origin.y;
                }
                if normal.y.abs() < 0.5 {
                    q.x = origin.x;
                }
                out.push(q);
            }
            if s_cur <= 0.0 {
                out.push(cur);
            }
        }
        out
    }
}

/// Lays out a marching region polygon with `position` mapping each region vertex to world XZ.
/// In a dual cell with a ramp corner, the centre is inserted between two opposite midpoints, so
/// that a straight contour whose two halves differ (one on a ramp edge, rule R6) bends there.
fn region_polygon(
    cell: &DualCell,
    region: &[DualVertexRef],
    position: impl Fn(DualVertexRef) -> Vec2,
) -> FloorPolygon {
    let mut polygon = FloorPolygon::EMPTY;
    for (k, &vref) in region.iter().enumerate() {
        polygon.push(position(vref));
        let next = region[(k + 1) % region.len()];
        if cell.ramp_mask != 0 {
            if let (DualVertexRef::Midpoint(a), DualVertexRef::Midpoint(b)) = (vref, next) {
                if are_opposite(a, b) {
                    polygon.push(cell.centre);
                }
            }
        }
    }
    polygon
}

/// Whether two edge midpoints lie on opposite edges of a dual cell (a straight contour).
fn are_opposite(a: EdgeMidpoint, b: EdgeMidpoint) -> bool {
    matches!(
        (a, b),
        (EdgeMidpoint::Bottom, EdgeMidpoint::Top)
            | (EdgeMidpoint::Top, EdgeMidpoint::Bottom)
            | (EdgeMidpoint::Left, EdgeMidpoint::Right)
            | (EdgeMidpoint::Right, EdgeMidpoint::Left)
    )
}

/// Direction from a dual cell's centre to its corner `k` (bottom-left, bottom-right, top-right,
/// top-left), with components `±1`.
fn corner_direction(k: usize) -> Vec2 {
    match k % 4 {
        0 => Vec2::new(-1.0, -1.0),
        1 => Vec2::new(1.0, -1.0),
        2 => Vec2::new(1.0, 1.0),
        _ => Vec2::new(-1.0, 1.0),
    }
}

/// Emits a floor polygon of a dual cell at altitude `floor_y`: whole in a dual cell without a
/// ramp corner, else clipped to the union of the non-ramp quadrants (rule R5).
///
/// The union is cut into convex pieces whose outer edges are whole dual-cell edges wherever
/// both corners of the edge are non-ramp, so the pieces add no vertex on an edge that a
/// neighbouring dual cell meshes without one: one quadrant, a half of two adjacent quadrants,
/// two diagonal quadrants, or (three quadrants) two halves split along the diagonal through the
/// ramp corner.
fn emit_dual_floor(
    map: &GameMap,
    cell: &DualCell,
    polygon: &FloorPolygon,
    floor_y: f32,
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) {
    if cell.ramp_mask == 0 {
        emit_floor_points(map, polygon.points(), floor_y, scratch, stats);
        return;
    }
    let centre = cell.centre;
    let is_ramp = |k: usize| cell.ramp_mask & (1 << (k % 4)) != 0;
    // Half-plane normals keeping quadrant `k` (two of them) or the half of quadrants `k, k + 1`.
    let quadrant = |k: usize| {
        let o = corner_direction(k);
        [Vec2::new(-o.x, 0.0), Vec2::new(0.0, -o.y)]
    };
    let half = |k: usize| -(corner_direction(k) + corner_direction(k + 1)) * 0.5;
    let mut emit = |piece: FloorPolygon| {
        emit_floor_points(map, piece.points(), floor_y, scratch, stats);
    };
    match cell.ramp_mask.count_ones() {
        1 => {
            // Quadrants `first`, `opposite` and `last` follow the ramp corner counter-clockwise;
            // the split runs along the diagonal through the ramp corner and `opposite`.
            let ramp = (0..4).find(|&k| is_ramp(k)).unwrap_or(0);
            let (first, opposite) = (ramp + 1, ramp + 2);
            let diagonal = corner_direction(opposite);
            let normal = Vec2::new(diagonal.y, -diagonal.x);
            let toward_first = if normal.dot(corner_direction(first)) < 0.0 {
                normal
            } else {
                -normal
            };
            emit(polygon.clip(toward_first, centre).clip(half(first), centre));
            emit(
                polygon
                    .clip(-toward_first, centre)
                    .clip(half(opposite), centre),
            );
        }
        2 => {
            if let Some(k) = (0..4).find(|&k| !is_ramp(k) && !is_ramp(k + 1)) {
                emit(polygon.clip(half(k), centre));
            } else {
                for k in (0..4).filter(|&k| !is_ramp(k)) {
                    let [n1, n2] = quadrant(k);
                    emit(polygon.clip(n1, centre).clip(n2, centre));
                }
            }
        }
        _ => {
            for k in (0..4).filter(|&k| !is_ramp(k)) {
                let [n1, n2] = quadrant(k);
                emit(polygon.clip(n1, centre).clip(n2, centre));
            }
        }
    }
}

/// Emits a convex, CCW (seen from +Y) floor polygon at altitude `floor_y` as an upward-facing
/// triangle fan, with per-vertex ambient occlusion. Polygons with fewer than three vertices emit
/// nothing, and fan triangles of (near) zero area, which clipping can leave, are skipped.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a chunk holds far fewer than u32::MAX vertices"
)]
fn emit_floor_points(
    map: &GameMap,
    points: &[Vec2],
    floor_y: f32,
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) {
    if points.len() < 3 {
        return;
    }
    let base_idx = scratch.positions.len() as u32;
    for &pos2 in points {
        let ao = compute_floor_ao(map, pos2.x, floor_y, pos2.y);
        scratch.positions.push([pos2.x, floor_y, pos2.y]);
        scratch.normals.push([0.0, 1.0, 0.0]);
        scratch.uv0.push([pos2.x, pos2.y]);
        scratch.uv1.push([ao, 0.0]);
    }
    stats.floor_vertices += points.len();

    for k in 1..points.len() - 1 {
        let doubled_area = (points[k] - points[0]).perp_dot(points[k + 1] - points[0]);
        if doubled_area.abs() < MIN_FLOOR_TRIANGLE_DOUBLED_AREA {
            continue;
        }
        scratch.indices.push(base_idx);
        scratch.indices.push(base_idx + (k + 1) as u32);
        scratch.indices.push(base_idx + k as u32);
        stats.floor_indices += 3;
    }
}

/// Twice the smallest area of an emitted floor triangle, in square world units. Marching
/// regions never come near it; clipped pieces can leave slivers below it.
const MIN_FLOOR_TRIANGLE_DOUBLED_AREA: f32 = 1e-6;

/// Emits the cliff walls of one threshold run `ta..=tb` of a dual cell: one quad strip per contour
/// segment, from the wall profiles at the segment's two edge midpoints.
///
/// In a dual cell with a ramp corner, a straight segment (through the centre) is split there:
/// a half on a ramp edge is dropped, because the ramp cell owns that edge (rule R2), and the
/// other half runs from its midpoint to the centre (rule R6).
fn emit_cliff_walls(
    cell: &DualCell,
    segments: &[ContourSegment],
    ta: i32,
    tb: i32,
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) {
    for &seg in segments {
        if cell.ramp_mask == 0 || !are_opposite(seg.start, seg.end) {
            let prof0 = cell.midpoint_profile(seg.start, ta, tb);
            let prof1 = cell.midpoint_profile(seg.end, ta, tb);
            emit_wall_strip(&prof0, &prof1, scratch, stats);
            continue;
        }
        let on_ramp = |m| cell.midpoint_suppressed(m);
        match (on_ramp(seg.start), on_ramp(seg.end)) {
            (true, true) => {}
            (true, false) => {
                let prof0 = cell.centre_profile(seg.end, ta, tb);
                let prof1 = cell.midpoint_profile(seg.end, ta, tb);
                emit_wall_strip(&prof0, &prof1, scratch, stats);
            }
            (false, true) => {
                let prof0 = cell.midpoint_profile(seg.start, ta, tb);
                let prof1 = cell.centre_profile(seg.start, ta, tb);
                emit_wall_strip(&prof0, &prof1, scratch, stats);
            }
            // A straight contour borders every quadrant, so a ramp corner puts one of its
            // halves on a ramp edge; whole is still the right wall if it ever does not.
            (false, false) => {
                let prof0 = cell.midpoint_profile(seg.start, ta, tb);
                let prof1 = cell.midpoint_profile(seg.end, ta, tb);
                emit_wall_strip(&prof0, &prof1, scratch, stats);
            }
        }
    }
}

/// Emits a wall quad strip between two wall profiles, with one vertex pair per distinct row height
/// of either profile (heights within `1e-4` merge), top to bottom. Degenerate triangles are
/// dropped. A strip needs at least two distinct heights.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a chunk holds far fewer than u32::MAX vertices"
)]
fn emit_wall_strip(
    prof0: &[WallProfileRow],
    prof1: &[WallProfileRow],
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) {
    // Collect descending unique row heights
    let mut heights: Vec<f32> = Vec::with_capacity(prof0.len() + prof1.len());
    for r in prof0.iter().chain(prof1) {
        if !heights.iter().any(|&h| (h - r.y).abs() < 1e-4) {
            heights.push(r.y);
        }
    }
    heights.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    if heights.len() < 2 {
        return;
    }
    stats.wall_segments += 1;
    let strip_base = scratch.positions.len() as u32;

    for &y_row in &heights {
        for row in [
            sample_profile_at_height(prof0, y_row),
            sample_profile_at_height(prof1, y_row),
        ] {
            scratch.positions.push([row.xz.x, row.y, row.xz.y]);
            scratch
                .normals
                .push([row.normal.x, row.normal.y, row.normal.z]);
            scratch.uv0.push([row.xz.x, row.xz.y]);
            scratch.uv1.push([row.ao, 0.0]);
        }
        stats.wall_vertices += 2;
    }

    for k in 0..heights.len() - 1 {
        let v0 = strip_base + (k * 2) as u32;
        let v1 = strip_base + (k * 2 + 1) as u32;
        let v2 = strip_base + ((k + 1) * 2 + 1) as u32;
        let v3 = strip_base + ((k + 1) * 2) as u32;

        let p0 = Vec3::from(scratch.positions[v0 as usize]);
        let p1 = Vec3::from(scratch.positions[v1 as usize]);
        let p2 = Vec3::from(scratch.positions[v2 as usize]);
        let p3 = Vec3::from(scratch.positions[v3 as usize]);

        let cross1 = (p1 - p0).cross(p2 - p0);
        if cross1.length_squared() > 1e-10 {
            scratch.indices.push(v0);
            scratch.indices.push(v1);
            scratch.indices.push(v2);
            stats.wall_indices += 3;
        }

        let cross2 = (p2 - p0).cross(p3 - p0);
        if cross2.length_squared() > 1e-10 {
            scratch.indices.push(v0);
            scratch.indices.push(v2);
            scratch.indices.push(v3);
            stats.wall_indices += 3;
        }
    }
}

/// A straight stretch of a ramp cell's edge on the ramp surface: a side edge carrying a guard
/// rail, or a half-edge carrying a wall.
#[derive(Debug, Clone, Copy)]
struct RampSide {
    /// Edge start on the ramp surface (world space).
    start: Vec3,
    /// Edge end on the ramp surface (world space).
    end: Vec3,
    /// Unit vector pointing out of the ramp across this edge.
    outward: Vec3,
    /// Unit vector pointing into the ramp across this edge.
    inward: Vec3,
}

/// Height difference in world units below which a ramp edge and the surface beside it count as
/// level, and no wall closes them.
const RAMP_STEP_EPSILON: f32 = 1e-4;

/// One half of an edge of a ramp cell (rule R2), in cell-local half-cell units: it runs from the
/// cell corner `corner` to the edge midpoint `mid`, and borders the cell at `neighbour_offset`.
/// Each half lies in the one dual cell centred on `corner`.
#[derive(Debug, Clone, Copy)]
struct RampHalfEdge {
    corner: (u8, u8),
    mid: (u8, u8),
    neighbour_offset: (i32, i32),
}

/// The eight half-edges of a ramp cell: south, east, north and west edge, two halves each.
const RAMP_HALF_EDGES: [RampHalfEdge; 8] = [
    RampHalfEdge {
        corner: (0, 0),
        mid: (1, 0),
        neighbour_offset: (0, -1),
    },
    RampHalfEdge {
        corner: (2, 0),
        mid: (1, 0),
        neighbour_offset: (0, -1),
    },
    RampHalfEdge {
        corner: (2, 0),
        mid: (2, 1),
        neighbour_offset: (1, 0),
    },
    RampHalfEdge {
        corner: (2, 2),
        mid: (2, 1),
        neighbour_offset: (1, 0),
    },
    RampHalfEdge {
        corner: (2, 2),
        mid: (1, 2),
        neighbour_offset: (0, 1),
    },
    RampHalfEdge {
        corner: (0, 2),
        mid: (1, 2),
        neighbour_offset: (0, 1),
    },
    RampHalfEdge {
        corner: (0, 2),
        mid: (0, 1),
        neighbour_offset: (-1, 0),
    },
    RampHalfEdge {
        corner: (0, 0),
        mid: (0, 1),
        neighbour_offset: (-1, 0),
    },
];

/// Section 2 of [`build_chunk_mesh`] for one ramp cell `(cx, cy)`: the sloped surface quad, the
/// walls on its half-edges (rule R2), then, on a cardinal ramp, a guard rail on each side edge
/// not shared with the same ramp run.
fn emit_ramp_cell(
    map: &GameMap,
    cx: u32,
    cy: u32,
    profile: RampProfile,
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) {
    let [p_bl, p_br, p_tr, p_tl] = emit_ramp_surface(map, cx, cy, profile, scratch, stats);
    let (Ok(cx_i), Ok(cy_i)) = (i32::try_from(cx), i32::try_from(cy)) else {
        return;
    };
    for half in RAMP_HALF_EDGES {
        emit_ramp_half_edge_wall(map, (cx_i, cy_i), profile, half, scratch, stats);
    }

    // Ramp side edges are parallel to the run's direction
    let (side1, side2) = match profile.dir {
        RampDir::North | RampDir::South => (
            // West edge (p_bl -> p_tl) facing -X
            (p_bl, p_tl, (-1, 0), Vec3::new(-1.0, 0.0, 0.0)),
            // East edge (p_br -> p_tr) facing +X
            (p_br, p_tr, (1, 0), Vec3::new(1.0, 0.0, 0.0)),
        ),
        RampDir::East | RampDir::West => (
            // South edge (p_bl -> p_br) facing -Z
            (p_bl, p_br, (0, -1), Vec3::new(0.0, 0.0, -1.0)),
            // North edge (p_tl -> p_tr) facing +Z
            (p_tl, p_tr, (0, 1), Vec3::new(0.0, 0.0, 1.0)),
        ),
        _ => return,
    };
    for (start, end, offset, outward) in [side1, side2] {
        let neighbour_same_run = match surface_class(map, cx_i + offset.0, cy_i + offset.1) {
            SurfaceClass::Ramp(n_prof) => {
                n_prof.dir == profile.dir
                    && n_prof.low_level == profile.low_level
                    && n_prof.index == profile.index
            }
            _ => false,
        };
        if !neighbour_same_run {
            let side = RampSide {
                start,
                end,
                outward,
                inward: -outward,
            };
            emit_guard_rail(&side, scratch, stats);
        }
    }
}

/// Emits the wall on one half-edge of ramp cell `cell` (rule R2): where the ramp's edge lies
/// above the surface beside it, a side wall facing out of the ramp, down to that surface.
///
/// The surface beside a non-ramp neighbour is the dual grid's floor in the neighbour's quadrant
/// of the half-edge's dual cell ([`dual_floor_level_beside`], rule R3), so this wall and the
/// dual grid's floor meet exactly. A ramp neighbour or a higher neighbour gets no wall here.
#[expect(
    clippy::cast_precision_loss,
    reason = "map cell coordinates stay far below the range of f32"
)]
fn emit_ramp_half_edge_wall(
    map: &GameMap,
    cell: (i32, i32),
    profile: RampProfile,
    half: RampHalfEdge,
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) {
    let neighbour = (
        cell.0 + half.neighbour_offset.0,
        cell.1 + half.neighbour_offset.1,
    );
    if matches!(
        surface_class(map, neighbour.0, neighbour.1),
        SurfaceClass::Ramp(_)
    ) {
        return;
    }
    // The dual cell centred on the half-edge's cell corner, and the neighbour's corner in it.
    let di = cell.0 + i32::from(half.corner.0 / 2);
    let dj = cell.1 + i32::from(half.corner.1 / 2);
    let (Ok(di_u), Ok(dj_u)) = (u32::try_from(di), u32::try_from(dj)) else {
        return;
    };
    let corner = match (neighbour.0 - (di - 1), neighbour.1 - (dj - 1)) {
        (0, 0) => 0,
        (1, 0) => 1,
        (1, 1) => 2,
        _ => 3,
    };
    let beside = cliff_level_height(dual_floor_level_beside(map, di_u, dj_u, corner));

    let edge_point = |(hx, hy): (u8, u8)| {
        let local = |h: u8| match h {
            0 => SimScalar::ZERO,
            1 => SimScalar::HALF,
            _ => SimScalar::ONE,
        };
        Vec3::new(
            cell.0 as f32 + f32::from(hx) * 0.5,
            profile.height_at(local(hx), local(hy)).to_f32(),
            cell.1 as f32 + f32::from(hy) * 0.5,
        )
    };
    let start = edge_point(half.corner);
    let end = edge_point(half.mid);
    let outward = Vec3::new(
        half.neighbour_offset.0 as f32,
        0.0,
        half.neighbour_offset.1 as f32,
    );
    if start.y.max(end.y) > beside + RAMP_STEP_EPSILON {
        let side = RampSide {
            start,
            end,
            outward,
            inward: -outward,
        };
        emit_ramp_side_wall(&side, beside, scratch, stats);
    }
}

/// Emits the sloped surface quad of ramp cell `(cx, cy)` at the simulation's authoritative corner
/// altitudes, and returns its corners `[bottom-left, bottom-right, top-right, top-left]` in world
/// space.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "map cell coordinates and chunk vertex counts stay far below the range of f32 and u32"
)]
fn emit_ramp_surface(
    map: &GameMap,
    cx: u32,
    cy: u32,
    profile: RampProfile,
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) -> [Vec3; 4] {
    // Authoritative simulation surface altitudes at the 4 cell corners
    let h_bl = profile.height_at(SimScalar::ZERO, SimScalar::ZERO).to_f32();
    let h_br = profile.height_at(SimScalar::ONE, SimScalar::ZERO).to_f32();
    let h_tr = profile.height_at(SimScalar::ONE, SimScalar::ONE).to_f32();
    let h_tl = profile.height_at(SimScalar::ZERO, SimScalar::ONE).to_f32();

    let p_bl = Vec3::new(cx as f32, h_bl, cy as f32);
    let p_br = Vec3::new(cx as f32 + 1.0, h_br, cy as f32);
    let p_tr = Vec3::new(cx as f32 + 1.0, h_tr, cy as f32 + 1.0);
    let p_tl = Vec3::new(cx as f32, h_tl, cy as f32 + 1.0);

    let norm = ((p_tl - p_bl).cross(p_br - p_bl)).normalize();

    let corners = [p_bl, p_br, p_tr, p_tl];
    let base_idx = scratch.positions.len() as u32;
    for p in corners {
        let ao = compute_floor_ao(map, p.x, p.y, p.z);
        scratch.positions.push([p.x, p.y, p.z]);
        scratch.normals.push([norm.x, norm.y, norm.z]);
        scratch.uv0.push([p.x, p.z]);
        scratch.uv1.push([ao, 0.0]);
    }

    scratch.indices.push(base_idx);
    scratch.indices.push(base_idx + 2);
    scratch.indices.push(base_idx + 1);

    scratch.indices.push(base_idx);
    scratch.indices.push(base_idx + 3);
    scratch.indices.push(base_idx + 2);

    stats.ramp_vertices += 4;
    corners
}

/// Pushes one vertex with a horizontal normal `outward` (its Y component is dropped).
fn push_side_vertex(scratch: &mut MesherScratch, p: Vec3, outward: Vec3, ao: f32, emissive: f32) {
    scratch.positions.push([p.x, p.y, p.z]);
    scratch.normals.push([outward.x, 0.0, outward.z]);
    scratch.uv0.push([p.x, p.z]);
    scratch.uv1.push([ao, emissive]);
}

/// Emits the vertical wall below a ramp side edge, down to the neighbour's altitude
/// `neighbour_h` (never above the edge itself), facing out of the ramp.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a chunk holds far fewer than u32::MAX vertices"
)]
fn emit_ramp_side_wall(
    side: &RampSide,
    neighbour_h: f32,
    scratch: &mut MesherScratch,
    stats: &mut MeshStats,
) {
    let (v_start, v_end, perp_dir) = (side.start, side.end, side.outward);
    let wall_base = scratch.positions.len() as u32;
    let bot_start = Vec3::new(v_start.x, neighbour_h.min(v_start.y), v_start.z);
    let bot_end = Vec3::new(v_end.x, neighbour_h.min(v_end.y), v_end.z);

    push_side_vertex(scratch, v_start, perp_dir, 1.0, 0.0);
    push_side_vertex(scratch, v_end, perp_dir, 1.0, 0.0);
    push_side_vertex(scratch, bot_end, perp_dir, WALL_FOOT_AO, 0.0);
    push_side_vertex(scratch, bot_start, perp_dir, WALL_FOOT_AO, 0.0);

    // Orient wall triangles outward, emitting only non-degenerate triangles
    stats.wall_indices += push_quad_oriented(
        &scratch.positions,
        &mut scratch.indices,
        wall_base,
        perp_dir,
    );

    stats.wall_vertices += 4;
}

/// Emits a guard rail riding on a ramp side edge: an emissive top face (`uv1.y = 1`) inset by
/// [`RAIL_WIDTH`] toward the ramp at [`RAIL_HEIGHT`] above the edge, and an outer side face.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a chunk holds far fewer than u32::MAX vertices"
)]
fn emit_guard_rail(side: &RampSide, scratch: &mut MesherScratch, stats: &mut MeshStats) {
    let (v_start, v_end, perp_dir) = (side.start, side.end, side.outward);
    let rail_base = scratch.positions.len() as u32;
    let in_offset = side.inward * RAIL_WIDTH;
    let up_offset = Vec3::Y * RAIL_HEIGHT;

    let p0 = v_start;
    let p1 = v_end;
    let p0_in = v_start + in_offset;
    let p1_in = v_end + in_offset;

    let t0 = p0 + up_offset;
    let t1 = p1 + up_offset;
    let t0_in = p0_in + up_offset;
    let t1_in = p1_in + up_offset;

    // Top face: flagged emissive with uv1.y = 1.0
    for p in [t0, t1, t1_in, t0_in] {
        scratch.positions.push([p.x, p.y, p.z]);
        scratch.normals.push([0.0, 1.0, 0.0]);
        scratch.uv0.push([p.x, p.z]);
        scratch.uv1.push([1.0, 1.0]);
    }
    push_quad_oriented(&scratch.positions, &mut scratch.indices, rail_base, Vec3::Y);

    // Outer side face: facing perp_dir
    let out_base = scratch.positions.len() as u32;
    for p in [t0, p0, p1, t1] {
        push_side_vertex(scratch, p, perp_dir, 1.0, 0.0);
    }
    push_quad_oriented(&scratch.positions, &mut scratch.indices, out_base, perp_dir);

    stats.rail_vertices += 8;
}

/// Updates a Bevy [`Mesh`] in place using data buffered inside [`MesherScratch`].
///
/// # Invariants
/// - Does not allocate new heap memory once the mesh attribute buffers have grown to capacity.
// PERFORMANCE RATIONALE:
// Why: Allocating new vertex vectors on every terrain edit induces memory fragmentation and GC pressure.
// What: We mutate existing `Float32x3` and `Float32x2` vertex attribute buffers in place via `attribute_mut`.
// Trade-off: None. Reusing existing vector capacities is zero-allocation steady-state.
pub fn apply_scratch_to_mesh(scratch: &MesherScratch, mesh: &mut Mesh) {
    if let Some(VertexAttributeValues::Float32x3(v)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        v.clear();
        v.extend_from_slice(&scratch.positions);
    } else {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, scratch.positions.clone());
    }

    if let Some(VertexAttributeValues::Float32x3(v)) = mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL) {
        v.clear();
        v.extend_from_slice(&scratch.normals);
    } else {
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, scratch.normals.clone());
    }

    if let Some(VertexAttributeValues::Float32x2(v)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        v.clear();
        v.extend_from_slice(&scratch.uv0);
    } else {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, scratch.uv0.clone());
    }

    if let Some(VertexAttributeValues::Float32x2(v)) = mesh.attribute_mut(ATTRIBUTE_UV_1) {
        v.clear();
        v.extend_from_slice(&scratch.uv1);
    } else {
        mesh.insert_attribute(ATTRIBUTE_UV_1, scratch.uv1.clone());
    }

    mesh.insert_indices(bevy::render::mesh::Indices::U32(scratch.indices.clone()));
}
