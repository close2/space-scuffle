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

impl SurfaceClass {
    /// Returns the effective numeric cliff level used for dual-grid marching squares.
    ///
    /// Ramp cells participate in dual-grid contour analysis at their lower landing level.
    #[must_use]
    pub fn marching_level(self) -> i32 {
        match self {
            Self::Void => -1,
            Self::Ramp(p) => i32::from(p.low_level.0),
            Self::Plateau(lvl) => lvl,
        }
    }
}

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

/// Returns the effective numeric cliff level of cell `(cx, cy)` at its local corner `(corner_fx, corner_fy)`.
#[must_use]
pub fn corner_marching_level(
    map: &GameMap,
    cx: i32,
    cy: i32,
    corner_fx: f32,
    corner_fy: f32,
) -> i32 {
    let s = surface_class(map, cx, cy);
    match s {
        SurfaceClass::Void => -1,
        SurfaceClass::Plateau(lvl) => lvl,
        SurfaceClass::Ramp(prof) => {
            let h = prof
                .height_at(
                    SimScalar::from_num(corner_fx),
                    SimScalar::from_num(corner_fy),
                )
                .to_f32();
            (h / CLIFF_LEVEL_HEIGHT as f32).round() as i32
        }
    }
}

/// Returns the world altitude `H(level)` for a given discrete cliff level.
#[must_use]
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

/// Computes the outward normal direction of a cardinal ramp direction.
#[must_use]
pub fn ramp_step_2d(dir: RampDir) -> (i32, i32) {
    match dir {
        RampDir::North => (0, 1),
        RampDir::South => (0, -1),
        RampDir::East => (1, 0),
        RampDir::West => (-1, 0),
        RampDir::NorthEast => (1, 1),
        RampDir::NorthWest => (-1, 1),
        RampDir::SouthEast => (1, -1),
        RampDir::SouthWest => (-1, -1),
    }
}

#[inline]
fn to_cell_coords(c: (i32, i32)) -> Option<(u32, u32)> {
    match (u32::try_from(c.0), u32::try_from(c.1)) {
        (Ok(x), Ok(y)) => Some((x, y)),
        _ => None,
    }
}

/// Checks whether the shared boundary between cell `a` and cell `b` is ramp-connected,
/// requiring wall and rim suppression along that edge.
#[must_use]
pub fn is_ramp_connected(map: &GameMap, ca: (i32, i32), cb: (i32, i32)) -> bool {
    let delta = (cb.0 - ca.0, cb.1 - ca.1);
    let cell_a_u = to_cell_coords(ca);
    let cell_b_u = to_cell_coords(cb);

    if let Some((ax, ay)) = cell_a_u {
        if let Some(prof) = map.ramp_profile(ax, ay) {
            let s = ramp_step_2d(prof.dir);
            if delta == s {
                if prof.index + 1 < prof.run_len {
                    return true;
                }
                // High landing connection
                if let Some((bx, by)) = cell_b_u {
                    if let Ok(cell_b) = map.cell(bx, by) {
                        if cell_b.terrain_kind != TerrainKind::Abyss
                            && cell_b.cliff_level.0 == prof.low_level.0 + 1
                        {
                            return true;
                        }
                    }
                }
            } else if delta == (-s.0, -s.1) {
                if prof.index > 0 {
                    return true;
                }
                // Low landing connection
                if let Some((bx, by)) = cell_b_u {
                    if let Ok(cell_b) = map.cell(bx, by) {
                        if cell_b.terrain_kind != TerrainKind::Abyss
                            && cell_b.cliff_level.0 == prof.low_level.0
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }

    // Symmetrically check from cb to ca
    let neg_delta = (-delta.0, -delta.1);
    if let Some((bx, by)) = cell_b_u {
        if let Some(prof) = map.ramp_profile(bx, by) {
            let s = ramp_step_2d(prof.dir);
            if neg_delta == s {
                if prof.index + 1 < prof.run_len {
                    return true;
                }
                if let Some((ax, ay)) = cell_a_u {
                    if let Ok(cell_a) = map.cell(ax, ay) {
                        if cell_a.terrain_kind != TerrainKind::Abyss
                            && cell_a.cliff_level.0 == prof.low_level.0 + 1
                        {
                            return true;
                        }
                    }
                }
            } else if neg_delta == (-s.0, -s.1) {
                if prof.index > 0 {
                    return true;
                }
                if let Some((ax, ay)) = cell_a_u {
                    if let Ok(cell_a) = map.cell(ax, ay) {
                        if cell_a.terrain_kind != TerrainKind::Abyss
                            && cell_a.cliff_level.0 == prof.low_level.0
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Evaluates ambient occlusion at a floor or ramp vertex.
#[must_use]
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

    let dual_x_start = chunk_x * CHUNK_DUAL_CELLS;
    let dual_x_end = ((chunk_x + 1) * CHUNK_DUAL_CELLS).min(width + 1);
    let dual_y_start = chunk_y * CHUNK_DUAL_CELLS;
    let dual_y_end = ((chunk_y + 1) * CHUNK_DUAL_CELLS).min(height + 1);

    // 1. Dual grid marching squares
    for dj in dual_y_start..dual_y_end {
        let dj_i = i32::try_from(dj).unwrap_or(0);
        for di in dual_x_start..dual_x_end {
            let di_i = i32::try_from(di).unwrap_or(0);
            let c_bl = (di_i - 1, dj_i - 1);
            let c_br = (di_i, dj_i - 1);
            let c_tr = (di_i, dj_i);
            let c_tl = (di_i - 1, dj_i);

            let l_bl = corner_marching_level(map, c_bl.0, c_bl.1, 1.0, 1.0);
            let l_br = corner_marching_level(map, c_br.0, c_br.1, 0.0, 1.0);
            let l_tr = corner_marching_level(map, c_tr.0, c_tr.1, 0.0, 0.0);
            let l_tl = corner_marching_level(map, c_tl.0, c_tl.1, 1.0, 0.0);

            let lo = l_bl.min(l_br).min(l_tr).min(l_tl);
            let hi = l_bl.max(l_br).max(l_tr).max(l_tl);

            let center_x = di as f32;
            let center_z = dj as f32;

            // Geometry coordinates of corners and midpoints
            let p_bl = Vec2::new(center_x - 0.5, center_z - 0.5);
            let p_br = Vec2::new(center_x + 0.5, center_z - 0.5);
            let p_tr = Vec2::new(center_x + 0.5, center_z + 0.5);
            let p_tl = Vec2::new(center_x - 0.5, center_z + 0.5);

            let p_b = Vec2::new(center_x, center_z - 0.5);
            let p_r = Vec2::new(center_x + 0.5, center_z);
            let p_t = Vec2::new(center_x, center_z + 0.5);
            let p_l = Vec2::new(center_x - 0.5, center_z);

            // Ramp suppression flags for the 4 edges
            let supp_b = is_ramp_connected(map, c_bl, c_br);
            let supp_r = is_ramp_connected(map, c_br, c_tr);
            let supp_t = is_ramp_connected(map, c_tl, c_tr);
            let supp_l = is_ramp_connected(map, c_bl, c_tl);

            let corner_coord = |c: DualCorner| match c {
                DualCorner::BottomLeft => p_bl,
                DualCorner::BottomRight => p_br,
                DualCorner::TopRight => p_tr,
                DualCorner::TopLeft => p_tl,
            };

            let midpoint_suppressed = |m: EdgeMidpoint| match m {
                EdgeMidpoint::Bottom => supp_b,
                EdgeMidpoint::Right => supp_r,
                EdgeMidpoint::Top => supp_t,
                EdgeMidpoint::Left => supp_l,
            };

            let midpoint_coord = |m: EdgeMidpoint| match m {
                EdgeMidpoint::Bottom => p_b,
                EdgeMidpoint::Right => p_r,
                EdgeMidpoint::Top => p_t,
                EdgeMidpoint::Left => p_l,
            };

            // Edge outward axis unit vector d = unit(B - A)
            let midpoint_d = |m: EdgeMidpoint, t: i32| match m {
                EdgeMidpoint::Bottom => {
                    if l_bl >= t {
                        Vec2::new(1.0, 0.0)
                    } else {
                        Vec2::new(-1.0, 0.0)
                    }
                }
                EdgeMidpoint::Right => {
                    if l_br >= t {
                        Vec2::new(0.0, 1.0)
                    } else {
                        Vec2::new(0.0, -1.0)
                    }
                }
                EdgeMidpoint::Top => {
                    if l_tl >= t {
                        Vec2::new(1.0, 0.0)
                    } else {
                        Vec2::new(-1.0, 0.0)
                    }
                }
                EdgeMidpoint::Left => {
                    if l_bl >= t {
                        Vec2::new(0.0, 1.0)
                    } else {
                        Vec2::new(0.0, -1.0)
                    }
                }
            };

            let midpoint_levels = |m: EdgeMidpoint, t: i32| match m {
                EdgeMidpoint::Bottom => {
                    if l_bl >= t {
                        (l_bl, l_br)
                    } else {
                        (l_br, l_bl)
                    }
                }
                EdgeMidpoint::Right => {
                    if l_br >= t {
                        (l_br, l_tr)
                    } else {
                        (l_tr, l_br)
                    }
                }
                EdgeMidpoint::Top => {
                    if l_tl >= t {
                        (l_tl, l_tr)
                    } else {
                        (l_tr, l_tl)
                    }
                }
                EdgeMidpoint::Left => {
                    if l_bl >= t {
                        (l_bl, l_tl)
                    } else {
                        (l_tl, l_bl)
                    }
                }
            };

            let case_for_t = |curr_t: i32| -> usize {
                let mut idx = 0;
                if l_bl >= curr_t {
                    idx |= 1;
                }
                if l_br >= curr_t {
                    idx |= 2;
                }
                if l_tr >= curr_t {
                    idx |= 4;
                }
                if l_tl >= curr_t {
                    idx |= 8;
                }
                idx
            };

            // Evaluate threshold runs for t in (lo + 1)..=hi
            if hi > lo {
                let mut t = lo + 1;
                while t <= hi {
                    let curr_case = case_for_t(t);
                    let ta = t;
                    let mut tb = t;
                    while tb < hi && case_for_t(tb + 1) == curr_case {
                        tb += 1;
                    }

                    if curr_case != 0 && curr_case != 15 {
                        let mcase = &MARCHING_CASES[curr_case];

                        // Emit region polygon as floor at H(tb)
                        let floor_y = cliff_level_height(tb);
                        let mut poly_verts = Vec::with_capacity(mcase.region.len());

                        for &vref in mcase.region {
                            let final_pos = match vref {
                                DualVertexRef::Corner(c) => corner_coord(c),
                                DualVertexRef::Midpoint(m) => {
                                    let (la, _) = midpoint_levels(m, tb);
                                    let rim_present = la == tb && !midpoint_suppressed(m);
                                    let base_coord = midpoint_coord(m);
                                    if rim_present {
                                        let d = midpoint_d(m, tb);
                                        base_coord - d * RIM_WIDTH
                                    } else {
                                        base_coord
                                    }
                                }
                            };

                            let ao = compute_floor_ao(map, final_pos.x, floor_y, final_pos.y);
                            poly_verts.push((final_pos, ao));
                        }

                        // Triangulate convex region polygon
                        if poly_verts.len() >= 3 {
                            let base_idx = scratch.positions.len() as u32;
                            for (pos2, ao) in &poly_verts {
                                scratch.positions.push([pos2.x, floor_y, pos2.y]);
                                scratch.normals.push([0.0, 1.0, 0.0]);
                                scratch.uv0.push([pos2.x, pos2.y]);
                                scratch.uv1.push([*ao, 0.0]);
                            }
                            stats.floor_vertices += poly_verts.len();

                            for k in 1..poly_verts.len() - 1 {
                                scratch.indices.push(base_idx);
                                scratch.indices.push(base_idx + (k + 1) as u32);
                                scratch.indices.push(base_idx + k as u32);
                                stats.floor_indices += 3;
                            }
                        }

                        // Emit contour wall segments
                        for &seg in mcase.segments {
                            // Check ramp suppression: if both endpoints are suppressed, omit wall
                            if midpoint_suppressed(seg.start) && midpoint_suppressed(seg.end) {
                                continue;
                            }

                            let p0 = midpoint_coord(seg.start);
                            let d0 = midpoint_d(seg.start, tb);
                            let (la0, lb0) = midpoint_levels(seg.start, tb);
                            let prof0 = generate_wall_profile(
                                p0,
                                d0,
                                la0,
                                lb0,
                                ta,
                                tb,
                                midpoint_suppressed(seg.start),
                            );

                            let p1 = midpoint_coord(seg.end);
                            let d1 = midpoint_d(seg.end, tb);
                            let (la1, lb1) = midpoint_levels(seg.end, tb);
                            let prof1 = generate_wall_profile(
                                p1,
                                d1,
                                la1,
                                lb1,
                                ta,
                                tb,
                                midpoint_suppressed(seg.end),
                            );

                            // Collect descending unique row heights
                            let mut heights: Vec<f32> =
                                Vec::with_capacity(prof0.len() + prof1.len());
                            for r in &prof0 {
                                if !heights.iter().any(|&h| (h - r.y).abs() < 1e-4) {
                                    heights.push(r.y);
                                }
                            }
                            for r in &prof1 {
                                if !heights.iter().any(|&h| (h - r.y).abs() < 1e-4) {
                                    heights.push(r.y);
                                }
                            }
                            heights.sort_by(|a, b| {
                                b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
                            });

                            if heights.len() >= 2 {
                                stats.wall_segments += 1;
                                let strip_base = scratch.positions.len() as u32;

                                for &y_row in &heights {
                                    let r0 = sample_profile_at_height(&prof0, y_row);
                                    let r1 = sample_profile_at_height(&prof1, y_row);

                                    scratch.positions.push([r0.xz.x, r0.y, r0.xz.y]);
                                    scratch
                                        .normals
                                        .push([r0.normal.x, r0.normal.y, r0.normal.z]);
                                    scratch.uv0.push([r0.xz.x, r0.xz.y]);
                                    scratch.uv1.push([r0.ao, 0.0]);

                                    scratch.positions.push([r1.xz.x, r1.y, r1.xz.y]);
                                    scratch
                                        .normals
                                        .push([r1.normal.x, r1.normal.y, r1.normal.z]);
                                    scratch.uv0.push([r1.xz.x, r1.xz.y]);
                                    scratch.uv1.push([r1.ao, 0.0]);

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
                        }
                    }

                    t = tb + 1;
                }
            }

            // PERFORMANCE RATIONALE:
            // For flat dual cells (hi == lo), emit the full cell quad. When a cliff is present (hi > lo),
            // the low region polygon (cases 1..14) matching the threshold run is emitted at H(lo) to avoid
            // hidden overdraw under the higher cliff plateau and guarantee watertight chunk boundaries.
            if lo >= 0 {
                let floor_y = cliff_level_height(lo);
                let low_case = if hi > lo {
                    (!case_for_t(lo + 1)) & 0x0F
                } else {
                    15
                };

                if low_case != 0 {
                    let mcase = &MARCHING_CASES[low_case];
                    let mut poly_verts = Vec::with_capacity(mcase.region.len());
                    for &vref in mcase.region {
                        let pos2 = match vref {
                            DualVertexRef::Corner(c) => corner_coord(c),
                            DualVertexRef::Midpoint(m) => midpoint_coord(m),
                        };
                        let ao = compute_floor_ao(map, pos2.x, floor_y, pos2.y);
                        poly_verts.push((pos2, ao));
                    }

                    if poly_verts.len() >= 3 {
                        let base_idx = scratch.positions.len() as u32;
                        for (pos2, ao) in &poly_verts {
                            scratch.positions.push([pos2.x, floor_y, pos2.y]);
                            scratch.normals.push([0.0, 1.0, 0.0]);
                            scratch.uv0.push([pos2.x, pos2.y]);
                            scratch.uv1.push([*ao, 0.0]);
                        }
                        stats.floor_vertices += poly_verts.len();

                        for k in 1..poly_verts.len() - 1 {
                            scratch.indices.push(base_idx);
                            scratch.indices.push(base_idx + (k + 1) as u32);
                            scratch.indices.push(base_idx + k as u32);
                            stats.floor_indices += 3;
                        }
                    }
                }
            }
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

                let ao_bl = compute_floor_ao(map, p_bl.x, h_bl, p_bl.z);
                let ao_br = compute_floor_ao(map, p_br.x, h_br, p_br.z);
                let ao_tr = compute_floor_ao(map, p_tr.x, h_tr, p_tr.z);
                let ao_tl = compute_floor_ao(map, p_tl.x, h_tl, p_tl.z);

                let base_idx = scratch.positions.len() as u32;
                scratch.positions.push([p_bl.x, p_bl.y, p_bl.z]);
                scratch.normals.push([norm.x, norm.y, norm.z]);
                scratch.uv0.push([p_bl.x, p_bl.z]);
                scratch.uv1.push([ao_bl, 0.0]);

                scratch.positions.push([p_br.x, p_br.y, p_br.z]);
                scratch.normals.push([norm.x, norm.y, norm.z]);
                scratch.uv0.push([p_br.x, p_br.z]);
                scratch.uv1.push([ao_br, 0.0]);

                scratch.positions.push([p_tr.x, p_tr.y, p_tr.z]);
                scratch.normals.push([norm.x, norm.y, norm.z]);
                scratch.uv0.push([p_tr.x, p_tr.z]);
                scratch.uv1.push([ao_tr, 0.0]);

                scratch.positions.push([p_tl.x, p_tl.y, p_tl.z]);
                scratch.normals.push([norm.x, norm.y, norm.z]);
                scratch.uv0.push([p_tl.x, p_tl.z]);
                scratch.uv1.push([ao_tl, 0.0]);

                scratch.indices.push(base_idx);
                scratch.indices.push(base_idx + 2);
                scratch.indices.push(base_idx + 1);

                scratch.indices.push(base_idx);
                scratch.indices.push(base_idx + 3);
                scratch.indices.push(base_idx + 2);

                stats.ramp_vertices += 4;

                // Ramp side edges: perpendicular to the run's direction
                // Side edges are parallel to dir
                let (side1, side2) = match profile.dir {
                    RampDir::North | RampDir::South => (
                        // West edge (p_bl -> p_tl) facing -X
                        (
                            p_bl,
                            p_tl,
                            (-1, 0),
                            Vec3::new(-1.0, 0.0, 0.0),
                            Vec3::new(1.0, 0.0, 0.0),
                        ),
                        // East edge (p_br -> p_tr) facing +X
                        (
                            p_br,
                            p_tr,
                            (1, 0),
                            Vec3::new(1.0, 0.0, 0.0),
                            Vec3::new(-1.0, 0.0, 0.0),
                        ),
                    ),
                    RampDir::East | RampDir::West => (
                        // South edge (p_bl -> p_br) facing -Z
                        (
                            p_bl,
                            p_br,
                            (0, -1),
                            Vec3::new(0.0, 0.0, -1.0),
                            Vec3::new(0.0, 0.0, 1.0),
                        ),
                        // North edge (p_tl -> p_tr) facing +Z
                        (
                            p_tl,
                            p_tr,
                            (0, 1),
                            Vec3::new(0.0, 0.0, 1.0),
                            Vec3::new(0.0, 0.0, -1.0),
                        ),
                    ),
                    _ => continue,
                };

                let Ok(cx_i) = i32::try_from(cx) else {
                    continue;
                };
                let Ok(cy_i) = i32::try_from(cy) else {
                    continue;
                };
                for (v_start, v_end, n_offset, perp_dir, inward_dir) in [side1, side2] {
                    let n_cell = (cx_i + n_offset.0, cy_i + n_offset.1);
                    let n_class = surface_class(map, n_cell.0, n_cell.1);

                    // Check if neighbour is a parallel ramp in the same run
                    let neighbour_same_run = match n_class {
                        SurfaceClass::Ramp(n_prof) => {
                            n_prof.dir == profile.dir
                                && n_prof.low_level == profile.low_level
                                && n_prof.index == profile.index
                        }
                        _ => false,
                    };

                    if !neighbour_same_run {
                        let neighbour_h = match n_class {
                            SurfaceClass::Void => ABYSS_WALL_BOTTOM_Y,
                            SurfaceClass::Plateau(lvl) => cliff_level_height(lvl),
                            SurfaceClass::Ramp(n_prof) => {
                                n_prof.height_at(SimScalar::HALF, SimScalar::HALF).to_f32()
                            }
                        };

                        // (a) Side wall down to neighbour if neighbour is lower
                        if v_start.y > neighbour_h || v_end.y > neighbour_h {
                            let wall_base = scratch.positions.len() as u32;
                            let bot_start =
                                Vec3::new(v_start.x, neighbour_h.min(v_start.y), v_start.z);
                            let bot_end = Vec3::new(v_end.x, neighbour_h.min(v_end.y), v_end.z);

                            scratch.positions.push([v_start.x, v_start.y, v_start.z]);
                            scratch.normals.push([perp_dir.x, 0.0, perp_dir.z]);
                            scratch.uv0.push([v_start.x, v_start.z]);
                            scratch.uv1.push([1.0, 0.0]);

                            scratch.positions.push([v_end.x, v_end.y, v_end.z]);
                            scratch.normals.push([perp_dir.x, 0.0, perp_dir.z]);
                            scratch.uv0.push([v_end.x, v_end.z]);
                            scratch.uv1.push([1.0, 0.0]);

                            scratch.positions.push([bot_end.x, bot_end.y, bot_end.z]);
                            scratch.normals.push([perp_dir.x, 0.0, perp_dir.z]);
                            scratch.uv0.push([bot_end.x, bot_end.z]);
                            scratch.uv1.push([WALL_FOOT_AO, 0.0]);

                            scratch
                                .positions
                                .push([bot_start.x, bot_start.y, bot_start.z]);
                            scratch.normals.push([perp_dir.x, 0.0, perp_dir.z]);
                            scratch.uv0.push([bot_start.x, bot_start.z]);
                            scratch.uv1.push([WALL_FOOT_AO, 0.0]);

                            // Orient wall triangles outward, emitting only non-degenerate triangles
                            stats.wall_indices += push_quad_oriented(
                                &scratch.positions,
                                &mut scratch.indices,
                                wall_base,
                                perp_dir,
                            );

                            stats.wall_vertices += 4;
                        }

                        // (b) Guard rail riding on the ramp edge
                        let rail_base = scratch.positions.len() as u32;
                        let in_offset = inward_dir * RAIL_WIDTH;
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
                        scratch.positions.push([t0.x, t0.y, t0.z]);
                        scratch.normals.push([0.0, 1.0, 0.0]);
                        scratch.uv0.push([t0.x, t0.z]);
                        scratch.uv1.push([1.0, 1.0]);

                        scratch.positions.push([t1.x, t1.y, t1.z]);
                        scratch.normals.push([0.0, 1.0, 0.0]);
                        scratch.uv0.push([t1.x, t1.z]);
                        scratch.uv1.push([1.0, 1.0]);

                        scratch.positions.push([t1_in.x, t1_in.y, t1_in.z]);
                        scratch.normals.push([0.0, 1.0, 0.0]);
                        scratch.uv0.push([t1_in.x, t1_in.z]);
                        scratch.uv1.push([1.0, 1.0]);

                        scratch.positions.push([t0_in.x, t0_in.y, t0_in.z]);
                        scratch.normals.push([0.0, 1.0, 0.0]);
                        scratch.uv0.push([t0_in.x, t0_in.z]);
                        scratch.uv1.push([1.0, 1.0]);

                        push_quad_oriented(
                            &scratch.positions,
                            &mut scratch.indices,
                            rail_base,
                            Vec3::Y,
                        );

                        // Outer side face: facing perp_dir
                        let out_base = scratch.positions.len() as u32;
                        scratch.positions.push([t0.x, t0.y, t0.z]);
                        scratch.normals.push([perp_dir.x, 0.0, perp_dir.z]);
                        scratch.uv0.push([t0.x, t0.z]);
                        scratch.uv1.push([1.0, 0.0]);

                        scratch.positions.push([p0.x, p0.y, p0.z]);
                        scratch.normals.push([perp_dir.x, 0.0, perp_dir.z]);
                        scratch.uv0.push([p0.x, p0.z]);
                        scratch.uv1.push([1.0, 0.0]);

                        scratch.positions.push([p1.x, p1.y, p1.z]);
                        scratch.normals.push([perp_dir.x, 0.0, perp_dir.z]);
                        scratch.uv0.push([p1.x, p1.z]);
                        scratch.uv1.push([1.0, 0.0]);

                        scratch.positions.push([t1.x, t1.y, t1.z]);
                        scratch.normals.push([perp_dir.x, 0.0, perp_dir.z]);
                        scratch.uv0.push([t1.x, t1.z]);
                        scratch.uv1.push([1.0, 0.0]);

                        push_quad_oriented(
                            &scratch.positions,
                            &mut scratch.indices,
                            out_base,
                            perp_dir,
                        );

                        stats.rail_vertices += 8;
                    }
                }
            }
        }
    }

    stats
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
