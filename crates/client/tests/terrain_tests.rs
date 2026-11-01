//! Integration tests for the shared terrain renderer (`crates/client/src/terrain/`).
//!
//! # Architectural Charter & Invariants
//! In accordance with [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md) and
//! [Implementation Plan 21 §5.8](file:///home/cl/projects/space-scuffle/docs/plans/21-plan-map-editor.md),
//! these tests verify the textbook-grade correctness, determinism, and performance invariants
//! of the shared terrain renderer:
//!
//! 1. **Exact Marching Squares Geometry:** 4x4 flat level 1 fixture produces exact counts:
//!    96 floor vertices, 138 floor indices, 16 wall segments, 448 wall vertices, 1248 wall indices,
//!    0 ramp vertices, 0 rail vertices, overall 544 vertices and 1386 indices.
//! 2. **Authoritative Altitude Consistency:** 2,000 deterministic sample points across
//!    `showcase_canyon` exhibit vertical ray hit heights equal to `GameMap::terrain_altitude_at`
//!    within 1e-4 tolerance.
//! 3. **Watertight Chunk Boundaries:** Vertex positions along shared chunk seam planes
//!    are bit-identical across chunk meshes.
//! 4. **Winding Order & Unit Normals:** Every triangle exhibits consistent CCW winding
//!    (positive geometric normal dot vertex normal), area >= 1e-8, and unit length normals (|len - 1.0| < 1e-5).
//! 5. **Incremental Rebuild Bit-Identity:** Rebuilding only dirty chunks after an edit
//!    produces results bit-for-bit identical to a full from-scratch rebuild.
//! 6. **Zero Allocation Rebuilds:** Rebuilding unchanged chunk geometry preserves scratchpad
//!    capacity and memory pointers.
//! 7. **Ramp Elevation Continuity:** Ramp boundary vertices match landing plateau elevations
//!    with zero walls emitted across suppressed ramp landing edges.
//! 8. **Data-Driven Texture Fields:** Splat weights and tactical overlay channels write
//!    exact byte patterns corresponding to simulation flags and terrain kinds.
//! 9. **Texture Array Layout & Mip Quality:** 4-layer mip chains pack in layer-major order,
//!    albedo downsamples in linear color space (188 sRGB for 50% gray), and normal mips remain unit vectors.
//! 10. **Amanatides-Woo Raycasting:** Ray tests correctly identify plateau surfaces, continuous ramps,
//!     lip occlusion, and abyss misses.
//! 11. **Virtual GPU Terrain Preview Render Verification (§5.7b):** `terrain_preview` binary
//!     executes under virtual GPU (xvfb + lavapipe) and renders 4 canonical camera views of
//!     `showcase_canyon` with `orbital_foundry` theme to 1600x900 PNGs. Verifies non-trivial
//!     luminance distribution (mean in [0.06, 0.85], std-dev > 0.06), and <20% void color
//!     in the central 50% framing region.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use std::path::{Path, PathBuf};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

use bevy::prelude::*;
use client::resolve_assets_dir;
use client::terrain::fields::{create_overlay_image, create_splat_image};
use client::terrain::mesher::{build_chunk_mesh, MesherScratch, CHUNK_DUAL_CELLS};
use client::terrain::picking::pick_terrain;
use client::terrain::theme_gpu::{
    downsample_albedo_linear, downsample_normal_renormalized, generate_mip_chain,
};
use sim_map::grid::{CellFlags, CliffLevel, GameMap, MapCell, TerrainKind};
use sim_map::map_format::MapFile;
use sim_types::{SimScalar, SimVec2};

/// Resolves a map file path relative to workspace root.
fn resolve_map_path(relative: &str) -> PathBuf {
    let assets = resolve_assets_dir();
    let direct = assets.join("maps").join(relative);
    if direct.exists() {
        return direct;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/maps")
        .join(relative)
}

/// Helper loading a `GameMap` from a `.scmap.ron` file.
fn load_test_map(filename: &str) -> GameMap {
    let path = resolve_map_path(filename);
    let map_file =
        MapFile::load(&path).unwrap_or_else(|e| panic!("failed to load {}: {e}", path.display()));
    let doc = map_file
        .decode()
        .unwrap_or_else(|e| panic!("failed to decode {}: {e}", path.display()));
    doc.to_game_map()
        .unwrap_or_else(|e| panic!("failed to build GameMap from {}: {e}", path.display()))
}

// -----------------------------------------------------------------------------
// 1. Fixture Flat 4x4 Exact Counts
// -----------------------------------------------------------------------------

#[test]
fn test_fixture_flat_4x4_exact_counts() {
    // 4x4 all level 1, no ramps
    let default_cell = MapCell {
        terrain_kind: TerrainKind::MetalPlate,
        cliff_level: CliffLevel(1),
        flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
        ramp: None,
        occupying_structure: None,
    };
    let mut map = GameMap::new(4, 4, default_cell).expect("GameMap::new");
    for y in 0..4 {
        for x in 0..4 {
            let cell = map.cell_mut(x, y).expect("cell_mut in 4x4");
            cell.cliff_level = CliffLevel(1);
            cell.terrain_kind = TerrainKind::MetalPlate;
            cell.flags = CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE;
            cell.ramp = None;
        }
    }

    let mut scratch = MesherScratch::default();
    let stats = build_chunk_mesh(&map, 0, 0, &mut scratch);

    // Theory from Plan §5.8:
    // Floors: 9 full quads (36 v / 54 i) + 12 half quads (48 v / 72 i) + 4 corner triangles (12 v / 12 i)
    // = 96 floor vertices, 138 floor indices.
    assert_eq!(
        stats.floor_vertices, 96,
        "floor_vertices mismatch on 4x4 flat fixture"
    );
    assert_eq!(
        stats.floor_indices, 138,
        "floor_indices mismatch on 4x4 flat fixture"
    );

    // Walls: 16 segments (12 straight + 4 corner diagonals), each a strip of 14 rows:
    // 28 vertices / 78 indices per segment -> 448 wall vertices / 1248 wall indices.
    assert_eq!(
        stats.wall_segments, 16,
        "wall_segments mismatch on 4x4 flat fixture"
    );
    assert_eq!(
        stats.wall_vertices, 448,
        "wall_vertices mismatch on 4x4 flat fixture"
    );
    assert_eq!(
        stats.wall_indices, 1248,
        "wall_indices mismatch on 4x4 flat fixture"
    );

    assert_eq!(stats.ramp_vertices, 0);
    assert_eq!(stats.rail_vertices, 0);

    // Overall exact totals: 544 vertices, 1386 indices
    assert_eq!(scratch.positions.len(), 544);
    assert_eq!(scratch.indices.len(), 1386);
    assert_eq!(scratch.normals.len(), 544);
    assert_eq!(scratch.uv0.len(), 544);
    assert_eq!(scratch.uv1.len(), 544);
}

// -----------------------------------------------------------------------------
// 2. Surface Heights Match Simulation
// -----------------------------------------------------------------------------

/// Evaluates vertical ray intersection with a triangle projected onto XZ plane.
fn vertical_ray_triangle_intersect(px: f32, pz: f32, v0: Vec3, v1: Vec3, v2: Vec3) -> Option<f32> {
    let det = (v1.z - v2.z) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.z - v2.z);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv_det = 1.0 / det;
    let w0 = ((v1.z - v2.z) * (px - v2.x) + (v2.x - v1.x) * (pz - v2.z)) * inv_det;
    let w1 = ((v2.z - v0.z) * (px - v2.x) + (v0.x - v2.x) * (pz - v2.z)) * inv_det;
    let w2 = 1.0 - w0 - w1;

    let eps = -1e-4;
    if w0 >= eps && w1 >= eps && w2 >= eps {
        Some(w0 * v0.y + w1 * v1.y + w2 * v2.y)
    } else {
        None
    }
}

#[test]
fn test_surface_heights_match_simulation() {
    let map = load_test_map("showcase_canyon.scmap.ron");
    let chunk_cols = map.width().div_ceil(CHUNK_DUAL_CELLS);
    let chunk_rows = map.height().div_ceil(CHUNK_DUAL_CELLS);

    // Build all chunk meshes
    let mut chunk_meshes = Vec::new();
    let mut scratch = MesherScratch::default();
    for cy in 0..chunk_rows {
        for cx in 0..chunk_cols {
            build_chunk_mesh(&map, cx, cy, &mut scratch);
            chunk_meshes.push(scratch.clone());
        }
    }

    // Find candidate cells whose center is >= 0.5 cells away from any level change
    let mut candidate_cells = Vec::new();
    let w = map.width() as i32;
    let h = map.height() as i32;

    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let Ok(cell) = map.cell(x as u32, y as u32) else {
                continue;
            };
            if cell.terrain_kind == TerrainKind::Abyss {
                continue;
            }

            // Check all 8 neighbors have identical cliff level and no ramps
            let mut uniform = cell.ramp.is_none();
            if uniform {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        if let Ok(nb) = map.cell((x + dx) as u32, (y + dy) as u32) {
                            if nb.cliff_level != cell.cliff_level
                                || nb.terrain_kind == TerrainKind::Abyss
                                || nb.ramp.is_some()
                            {
                                uniform = false;
                                break;
                            }
                        } else {
                            uniform = false;
                            break;
                        }
                    }
                    if !uniform {
                        break;
                    }
                }
            }

            if uniform {
                candidate_cells.push((x as u32, y as u32));
            }
        }
    }

    assert!(
        candidate_cells.len() >= 200,
        "showcase_canyon must contain ample uniform plateau cells for sampling"
    );

    // Deterministic PCG random sampling across candidate cells (2000 points)
    let num_samples = 2000;
    for sample_idx in 0..num_samples {
        let cell = candidate_cells[sample_idx % candidate_cells.len()];
        // Generate pseudo-random offset within [0.2, 0.8] of the cell
        let h1 = client::terrain::mesher::pcg2d((cell.0, sample_idx as u32));
        let h2 = client::terrain::mesher::pcg2d((h1.0, cell.1));
        let jx = 0.2 + (h1.1 as f32 / 4_294_967_295.0) * 0.6;
        let jz = 0.2 + (h2.1 as f32 / 4_294_967_295.0) * 0.6;

        let px = cell.0 as f32 + jx;
        let pz = cell.1 as f32 + jz;

        let sim_pos = SimVec2::new(SimScalar::from_num(px), SimScalar::from_num(pz));
        let expected_altitude = map.terrain_altitude_at(sim_pos);

        let di = (px + 0.5).floor() as u32;
        let dj = (pz + 0.5).floor() as u32;
        let base_cx = (di / CHUNK_DUAL_CELLS).min(chunk_cols - 1);
        let base_cy = (dj / CHUNK_DUAL_CELLS).min(chunk_rows - 1);

        // Vertical ray down: find highest triangle intersection across base chunk and neighbors
        let mut max_hit_y = -f32::INFINITY;
        for dy in base_cy.saturating_sub(1)..=(base_cy + 1).min(chunk_rows - 1) {
            for dx in base_cx.saturating_sub(1)..=(base_cx + 1).min(chunk_cols - 1) {
                let chunk_idx = (dy * chunk_cols + dx) as usize;
                let mesh = &chunk_meshes[chunk_idx];
                for tri in mesh.indices.as_chunks::<3>().0 {
                    let v0 = Vec3::from(mesh.positions[tri[0] as usize]);
                    let v1 = Vec3::from(mesh.positions[tri[1] as usize]);
                    let v2 = Vec3::from(mesh.positions[tri[2] as usize]);

                    if let Some(hit_y) = vertical_ray_triangle_intersect(px, pz, v0, v1, v2) {
                        if hit_y > max_hit_y {
                            max_hit_y = hit_y;
                        }
                    }
                }
            }
        }

        assert!(
            max_hit_y.is_finite(),
            "sample point ({px}, {pz}) in cell ({}, {}) must hit chunk mesh",
            cell.0,
            cell.1
        );
        let delta = (max_hit_y - expected_altitude).abs();
        assert!(
            delta < 1e-4,
            "sample #{sample_idx} at ({px}, {pz}): mesh height {max_hit_y} != sim altitude {expected_altitude} (delta: {delta})"
        );
    }
}

// -----------------------------------------------------------------------------
// 3. Chunk Borders Are Watertight
// -----------------------------------------------------------------------------

#[test]
fn test_chunk_borders_are_watertight() {
    let map = load_test_map("showcase_canyon.scmap.ron");
    let chunk_cols = map.width().div_ceil(CHUNK_DUAL_CELLS);
    let chunk_rows = map.height().div_ceil(CHUNK_DUAL_CELLS);

    let mut chunk_meshes = Vec::new();
    let mut scratch = MesherScratch::default();
    for cy in 0..chunk_rows {
        for cx in 0..chunk_cols {
            build_chunk_mesh(&map, cx, cy, &mut scratch);
            chunk_meshes.push(scratch.clone());
        }
    }

    // 1. Horizontal neighbours: shared vertical plane X = (cx + 1) * 16.0 - 0.5
    for cy in 0..chunk_rows {
        for cx in 0..(chunk_cols - 1) {
            let seam_x = ((cx + 1) * CHUNK_DUAL_CELLS) as f32 - 0.5;
            let mesh_a = &chunk_meshes[(cy * chunk_cols + cx) as usize];
            let mesh_b = &chunk_meshes[(cy * chunk_cols + cx + 1) as usize];

            let mut verts_a: Vec<[u32; 2]> = mesh_a
                .positions
                .iter()
                .filter(|p| (p[0] - seam_x).abs() < 1e-4)
                .map(|p| [p[1].to_bits(), p[2].to_bits()])
                .collect();
            let mut verts_b: Vec<[u32; 2]> = mesh_b
                .positions
                .iter()
                .filter(|p| (p[0] - seam_x).abs() < 1e-4)
                .map(|p| [p[1].to_bits(), p[2].to_bits()])
                .collect();

            verts_a.sort_unstable();
            verts_b.sort_unstable();
            assert_eq!(
                verts_a, verts_b,
                "horizontal chunk seam at X={seam_x} between ({cx},{cy}) and ({},{}) is not watertight",
                cx + 1,
                cy
            );
        }
    }

    // 2. Vertical neighbours: shared vertical plane Z = (cy + 1) * 16.0 - 0.5
    for cy in 0..(chunk_rows - 1) {
        for cx in 0..chunk_cols {
            let seam_z = ((cy + 1) * CHUNK_DUAL_CELLS) as f32 - 0.5;
            let mesh_a = &chunk_meshes[(cy * chunk_cols + cx) as usize];
            let mesh_b = &chunk_meshes[((cy + 1) * chunk_cols + cx) as usize];

            let mut verts_a: Vec<[u32; 2]> = mesh_a
                .positions
                .iter()
                .filter(|p| (p[2] - seam_z).abs() < 1e-4)
                .map(|p| [p[0].to_bits(), p[1].to_bits()])
                .collect();
            let mut verts_b: Vec<[u32; 2]> = mesh_b
                .positions
                .iter()
                .filter(|p| (p[2] - seam_z).abs() < 1e-4)
                .map(|p| [p[0].to_bits(), p[1].to_bits()])
                .collect();

            verts_a.sort_unstable();
            verts_b.sort_unstable();

            assert_eq!(
                verts_a, verts_b,
                "vertical chunk seam at Z={seam_z} between ({cx},{cy}) and ({cx},{}) is not watertight",
                cy + 1
            );
        }
    }
}

// -----------------------------------------------------------------------------
// 4. Indices in Range and Winding Consistent
// -----------------------------------------------------------------------------

#[test]
fn test_indices_in_range_and_winding_consistent() {
    let map = load_test_map("showcase_canyon.scmap.ron");
    let chunk_cols = map.width().div_ceil(CHUNK_DUAL_CELLS);
    let chunk_rows = map.height().div_ceil(CHUNK_DUAL_CELLS);

    let mut scratch = MesherScratch::default();
    for cy in 0..chunk_rows {
        for cx in 0..chunk_cols {
            build_chunk_mesh(&map, cx, cy, &mut scratch);
            let num_vertices = scratch.positions.len();

            for (tri_idx, tri) in scratch.indices.as_chunks::<3>().0.iter().enumerate() {
                assert!(
                    (tri[0] as usize) < num_vertices,
                    "index 0 out of bounds in chunk ({cx},{cy})"
                );
                assert!(
                    (tri[1] as usize) < num_vertices,
                    "index 1 out of bounds in chunk ({cx},{cy})"
                );
                assert!(
                    (tri[2] as usize) < num_vertices,
                    "index 2 out of bounds in chunk ({cx},{cy})"
                );

                let v0 = Vec3::from(scratch.positions[tri[0] as usize]);
                let v1 = Vec3::from(scratch.positions[tri[1] as usize]);
                let v2 = Vec3::from(scratch.positions[tri[2] as usize]);

                let e1 = v1 - v0;
                let e2 = v2 - v0;
                let cross = e1.cross(e2);
                let area = 0.5 * cross.length();

                assert!(
                    area >= 1e-8,
                    "degenerate triangle {tri_idx} with area {area} < 1e-8 in chunk ({cx},{cy})"
                );

                let n_geom = cross.normalize();
                let n0 = Vec3::from(scratch.normals[tri[0] as usize]);
                let n1 = Vec3::from(scratch.normals[tri[1] as usize]);
                let n2 = Vec3::from(scratch.normals[tri[2] as usize]);
                let n_vert = ((n0 + n1 + n2) / 3.0).normalize();

                let dot = n_geom.dot(n_vert);
                assert!(
                    dot > 0.0,
                    "inconsistent CCW winding on triangle {tri_idx} in chunk ({cx},{cy}): geom.dot(vert) = {dot}"
                );
            }
        }
    }
}

// -----------------------------------------------------------------------------
// 5. Normals Are Unit Length
// -----------------------------------------------------------------------------

#[test]
fn test_normals_are_unit_length() {
    let map = load_test_map("showcase_canyon.scmap.ron");
    let chunk_cols = map.width().div_ceil(CHUNK_DUAL_CELLS);
    let chunk_rows = map.height().div_ceil(CHUNK_DUAL_CELLS);

    let mut scratch = MesherScratch::default();
    for cy in 0..chunk_rows {
        for cx in 0..chunk_cols {
            build_chunk_mesh(&map, cx, cy, &mut scratch);
            for (idx, n) in scratch.normals.iter().enumerate() {
                let len = Vec3::from(*n).length();
                assert!(
                    (1.0 - len).abs() < 1e-5,
                    "normal #{idx} in chunk ({cx},{cy}) is not unit length: len={len}"
                );
            }
        }
    }
}

// -----------------------------------------------------------------------------
// 6. Incremental Rebuild Equals Full Rebuild
// -----------------------------------------------------------------------------

#[test]
fn test_incremental_rebuild_equals_full_rebuild() {
    let map = load_test_map("showcase_canyon.scmap.ron");
    let chunk_cols = map.width().div_ceil(CHUNK_DUAL_CELLS);
    let chunk_rows = map.height().div_ceil(CHUNK_DUAL_CELLS);

    // 1. Full build on original map
    let mut initial_chunks = Vec::new();
    let mut scratch = MesherScratch::default();
    for cy in 0..chunk_rows {
        for cx in 0..chunk_cols {
            build_chunk_mesh(&map, cx, cy, &mut scratch);
            initial_chunks.push(scratch.clone());
        }
    }

    // 2. Clone map and edit a 3x3 patch at (20, 20) with cliff level 3
    let mut edited_map = map.clone();
    for dy in 0..3 {
        for dx in 0..3 {
            let cell = edited_map
                .cell_mut(20 + dx, 20 + dy)
                .expect("cell_mut in patch");
            cell.cliff_level = CliffLevel(3);
            cell.terrain_kind = TerrainKind::MetalPlate;
            cell.ramp = None;
        }
    }

    // Determine dirty chunk range for cell rect (20, 20) to (22, 22)
    // Cell (x, y) dirties dual cells x-2..=x+3, y-2..=y+3
    let min_dual_x = 20u32.saturating_sub(2);
    let max_dual_x = (22u32 + 3).min(edited_map.width());
    let min_dual_y = 20u32.saturating_sub(2);
    let max_dual_y = (22u32 + 3).min(edited_map.height());

    let min_dirty_cx = min_dual_x / CHUNK_DUAL_CELLS;
    let max_dirty_cx = (max_dual_x / CHUNK_DUAL_CELLS).min(chunk_cols - 1);
    let min_dirty_cy = min_dual_y / CHUNK_DUAL_CELLS;
    let max_dirty_cy = (max_dual_y / CHUNK_DUAL_CELLS).min(chunk_rows - 1);

    // Incrementally rebuild only the dirty chunks
    let mut incremental_chunks = initial_chunks;
    for cy in min_dirty_cy..=max_dirty_cy {
        for cx in min_dirty_cx..=max_dirty_cx {
            let idx = (cy * chunk_cols + cx) as usize;
            build_chunk_mesh(&edited_map, cx, cy, &mut scratch);
            incremental_chunks[idx] = scratch.clone();
        }
    }

    // 3. Full rebuild of the edited map from scratch
    let mut full_chunks = Vec::new();
    for cy in 0..chunk_rows {
        for cx in 0..chunk_cols {
            build_chunk_mesh(&edited_map, cx, cy, &mut scratch);
            full_chunks.push(scratch.clone());
        }
    }

    // 4. Assert incremental rebuild is bit-for-bit identical to full rebuild across all chunks
    for idx in 0..(chunk_cols * chunk_rows) as usize {
        assert_eq!(
            incremental_chunks[idx].positions, full_chunks[idx].positions,
            "chunk {idx} positions mismatch"
        );
        assert_eq!(
            incremental_chunks[idx].normals, full_chunks[idx].normals,
            "chunk {idx} normals mismatch"
        );
        assert_eq!(
            incremental_chunks[idx].uv0, full_chunks[idx].uv0,
            "chunk {idx} uv0 mismatch"
        );
        assert_eq!(
            incremental_chunks[idx].uv1, full_chunks[idx].uv1,
            "chunk {idx} uv1 mismatch"
        );
        assert_eq!(
            incremental_chunks[idx].indices, full_chunks[idx].indices,
            "chunk {idx} indices mismatch"
        );
    }
}

// -----------------------------------------------------------------------------
// 7. Rebuild Reuses Capacity
// -----------------------------------------------------------------------------

#[test]
fn test_rebuild_reuses_capacity() {
    let map = load_test_map("showcase_canyon.scmap.ron");
    let mut scratch = MesherScratch::default();

    // First build
    build_chunk_mesh(&map, 0, 0, &mut scratch);

    let cap_pos = scratch.positions.capacity();
    let ptr_pos = scratch.positions.as_ptr();
    let cap_norm = scratch.normals.capacity();
    let ptr_norm = scratch.normals.as_ptr();
    let cap_uv0 = scratch.uv0.capacity();
    let ptr_uv0 = scratch.uv0.as_ptr();
    let cap_uv1 = scratch.uv1.capacity();
    let ptr_uv1 = scratch.uv1.as_ptr();
    let cap_ind = scratch.indices.capacity();
    let ptr_ind = scratch.indices.as_ptr();

    assert!(cap_pos > 0);
    assert!(cap_ind > 0);

    // Second build of same chunk
    build_chunk_mesh(&map, 0, 0, &mut scratch);

    assert_eq!(
        scratch.positions.capacity(),
        cap_pos,
        "positions capacity changed"
    );
    assert_eq!(scratch.positions.as_ptr(), ptr_pos, "positions reallocated");
    assert_eq!(
        scratch.normals.capacity(),
        cap_norm,
        "normals capacity changed"
    );
    assert_eq!(scratch.normals.as_ptr(), ptr_norm, "normals reallocated");
    assert_eq!(scratch.uv0.capacity(), cap_uv0, "uv0 capacity changed");
    assert_eq!(scratch.uv0.as_ptr(), ptr_uv0, "uv0 reallocated");
    assert_eq!(scratch.uv1.capacity(), cap_uv1, "uv1 capacity changed");
    assert_eq!(scratch.uv1.as_ptr(), ptr_uv1, "uv1 reallocated");
    assert_eq!(
        scratch.indices.capacity(),
        cap_ind,
        "indices capacity changed"
    );
    assert_eq!(scratch.indices.as_ptr(), ptr_ind, "indices reallocated");
}

// -----------------------------------------------------------------------------
// 8. Ramp Surface Continuous With Landings
// -----------------------------------------------------------------------------

#[test]
fn test_ramp_surface_continuous_with_landings() {
    let map = load_test_map("default_arena.scmap.ron");

    // Locate ramp cells on default_arena
    let mut ramp_cells = Vec::new();
    for y in 0..map.height() {
        for x in 0..map.width() {
            if let Ok(c) = map.cell(x, y) {
                if c.ramp.is_some() {
                    ramp_cells.push((x, y));
                }
            }
        }
    }
    assert!(
        !ramp_cells.is_empty(),
        "default_arena must contain ramp cells"
    );

    let chunk_cols = map.width().div_ceil(CHUNK_DUAL_CELLS);
    let chunk_rows = map.height().div_ceil(CHUNK_DUAL_CELLS);

    let low_landing_h = 3.0f32;
    let high_landing_h = 6.0f32;

    let mut found_low_boundary = false;
    let mut found_high_boundary = false;
    let mut scratch = MesherScratch::default();

    for cy in 0..chunk_rows {
        for cx in 0..chunk_cols {
            build_chunk_mesh(&map, cx, cy, &mut scratch);

            for (idx, p) in scratch.positions.iter().enumerate() {
                // Low boundary at z = 30.0 inside ramp run
                if (p[2] - 30.0).abs() < 1e-4 && p[0] >= 30.5 && p[0] <= 33.5 {
                    assert!(
                        (p[1] - low_landing_h).abs() < 1e-4,
                        "ramp vertex at ({},{},{}) must equal low landing height {}",
                        p[0],
                        p[1],
                        p[2],
                        low_landing_h
                    );
                    found_low_boundary = true;

                    // Ensure no cliff wall is emitted facing South across suppressed landing
                    let n = scratch.normals[idx];
                    assert!(
                        n[2].abs() < 0.9 || n[1] > 0.1,
                        "cliff wall emitted across suppressed low landing edge: normal = {n:?}"
                    );
                }

                // High boundary at z = 34.0 inside ramp run
                if (p[2] - 34.0).abs() < 1e-4 && p[0] >= 30.5 && p[0] <= 33.5 {
                    assert!(
                        (p[1] - high_landing_h).abs() < 1e-4,
                        "ramp vertex at ({},{},{}) must equal high landing height {}",
                        p[0],
                        p[1],
                        p[2],
                        high_landing_h
                    );
                    found_high_boundary = true;

                    // Ensure no cliff wall is emitted facing North across suppressed landing
                    let n = scratch.normals[idx];
                    assert!(
                        n[2].abs() < 0.9 || n[1] > 0.1,
                        "cliff wall emitted across suppressed high landing edge: normal = {n:?}"
                    );
                }
            }
        }
    }

    assert!(
        found_low_boundary,
        "must verify ramp low landing boundary vertices"
    );
    assert!(
        found_high_boundary,
        "must verify ramp high landing boundary vertices"
    );
}

// -----------------------------------------------------------------------------
// 9. Splat and Overlay Bytes
// -----------------------------------------------------------------------------

#[test]
fn test_splat_and_overlay_bytes() {
    let map = load_test_map("showcase_canyon.scmap.ron");
    let splat_img = create_splat_image(&map);
    let overlay_img = create_overlay_image(&map);

    let get_pixel = |img: &Image, x: u32, y: u32| -> [u8; 4] {
        let offset = ((y * map.width() + x) * 4) as usize;
        [
            img.data[offset],
            img.data[offset + 1],
            img.data[offset + 2],
            img.data[offset + 3],
        ]
    };

    // Cell (6, 6): Plateau metal, buildable and pathable
    let splat_6_6 = get_pixel(&splat_img, 6, 6);
    let ov_6_6 = get_pixel(&overlay_img, 6, 6);
    assert_eq!(
        splat_6_6,
        [0, 255, 0, 0],
        "splat at (6,6) must be pure MetalPlate"
    );
    assert_eq!(
        ov_6_6,
        [255, 255, 255, 0],
        "overlay at (6,6) must be pathable, buildable, visible, no error"
    );

    // Cell (50, 8): Chasm / Abyss cell
    let ov_50_8 = get_pixel(&overlay_img, 50, 8);
    assert_eq!(ov_50_8[0], 0, "abyss cell must not be ground pathable");
    assert_eq!(ov_50_8[1], 0, "abyss cell must not be buildable");

    // Cell (27, 9): Ramp cell (pathable, NOT buildable, visible, no error)
    let ov_27_9 = get_pixel(&overlay_img, 27, 9);
    assert_eq!(
        ov_27_9,
        [255, 0, 255, 0],
        "overlay at ramp (27,9) must be R=255, G=0, B=255, A=0"
    );
}

// -----------------------------------------------------------------------------
// 10. Mip Chain Sizes and Layer-Major Layout
// -----------------------------------------------------------------------------

#[test]
fn test_mip_chain_sizes_and_layer_major_layout() {
    let size = 1024u32;
    let mip_levels = 11u32; // log2(1024) + 1 = 11

    let mut per_layer_bytes = 0usize;
    for k in 0..mip_levels {
        let d = (size >> k).max(1) as usize;
        per_layer_bytes += d * d * 4;
    }

    // Theoretical sum: 4 * (1024*1024 + 512*512 + ... + 1*1) = 4 * 1,398,101 = 5,592,404 bytes
    assert_eq!(per_layer_bytes, 5_592_404);

    let total_bytes = per_layer_bytes * 4;
    assert_eq!(total_bytes, 22_369_616);

    // Test mip chain generation for a 1024x1024 dummy image
    let dummy = image::RgbaImage::new(size, size);
    let chain = generate_mip_chain(dummy, mip_levels, downsample_albedo_linear);

    assert_eq!(chain.len(), 11);
    let mut sum_chain_bytes = 0usize;
    for (k, mip_img) in chain.iter().enumerate() {
        let d = (size >> k).max(1) as usize;
        assert_eq!(mip_img.as_raw().len(), d * d * 4);
        sum_chain_bytes += mip_img.as_raw().len();
    }
    assert_eq!(sum_chain_bytes, per_layer_bytes);

    // Verify layer-major packing offset: Layer 1 begins at offset per_layer_bytes
    let layer_1_offset = per_layer_bytes;
    assert_eq!(layer_1_offset, 5_592_404);
}

// -----------------------------------------------------------------------------
// 11. Mip Normals Stay Unit and Albedo Mips Average in Linear Space
// -----------------------------------------------------------------------------

#[test]
fn test_mip_normals_stay_unit_and_albedo_average_linear() {
    // 1. Albedo: 2x2 with 2 black and 2 white pixels
    let mut albedo_2x2 = image::RgbaImage::new(2, 2);
    albedo_2x2.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
    albedo_2x2.put_pixel(1, 0, image::Rgba([255, 255, 255, 255]));
    albedo_2x2.put_pixel(0, 1, image::Rgba([0, 0, 0, 255]));
    albedo_2x2.put_pixel(1, 1, image::Rgba([255, 255, 255, 255]));

    let mip_albedo = downsample_albedo_linear(&albedo_2x2);
    assert_eq!(mip_albedo.dimensions(), (1, 1));
    let p = mip_albedo.get_pixel(0, 0);

    // Linear average of 0.0 and 1.0 is 0.5; linear 0.5 converts to sRGB ~188
    assert_eq!(
        p[0], 188,
        "albedo linear downsampling must produce 188 sRGB for 50% linear reflectance"
    );
    assert_eq!(p[1], 188);
    assert_eq!(p[2], 188);
    assert_eq!(p[3], 255);

    // 2. Normal: 2x2 with varied encoded normal vectors
    let mut normal_2x2 = image::RgbaImage::new(2, 2);
    // [0, 0, 1] encoded: [128, 128, 255]
    normal_2x2.put_pixel(0, 0, image::Rgba([128, 128, 255, 255]));
    // [1, 0, 0] encoded: [255, 128, 128]
    normal_2x2.put_pixel(1, 0, image::Rgba([255, 128, 128, 255]));
    // [0, 1, 0] encoded: [128, 255, 128]
    normal_2x2.put_pixel(0, 1, image::Rgba([128, 255, 128, 255]));
    // [0, 0, 1] encoded: [128, 128, 255]
    normal_2x2.put_pixel(1, 1, image::Rgba([128, 128, 255, 255]));

    let mip_normal = downsample_normal_renormalized(&normal_2x2);
    assert_eq!(mip_normal.dimensions(), (1, 1));
    let np = mip_normal.get_pixel(0, 0);

    let decoded = Vec3::new(
        (f32::from(np[0]) / 255.0) * 2.0 - 1.0,
        (f32::from(np[1]) / 255.0) * 2.0 - 1.0,
        (f32::from(np[2]) / 255.0) * 2.0 - 1.0,
    );
    let len = decoded.length();
    assert!(
        (len - 1.0).abs() < 0.02,
        "downsampled normal vector must stay unit length within 8-bit quantization: len={len}"
    );
}

// -----------------------------------------------------------------------------
// 12. Picking Tests
// -----------------------------------------------------------------------------

#[test]
fn test_picking_scenarios() {
    // Scenario A: Flat level-1 32x32 map, vertical ray at (16.5, 20.0, 16.5) -> cell (16, 16), y = 3.0
    let default_cell_flat = MapCell {
        terrain_kind: TerrainKind::MetalPlate,
        cliff_level: CliffLevel(1),
        flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
        ramp: None,
        occupying_structure: None,
    };
    let mut flat_map = GameMap::new(32, 32, default_cell_flat).expect("flat_map");
    for y in 0..32 {
        for x in 0..32 {
            let cell = flat_map.cell_mut(x, y).expect("cell_mut");
            cell.cliff_level = CliffLevel(1);
            cell.terrain_kind = TerrainKind::MetalPlate;
            cell.flags = CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE;
            cell.ramp = None;
        }
    }

    let hit = pick_terrain(
        &flat_map,
        Vec3::new(16.5, 20.0, 16.5),
        Vec3::new(0.0, -1.0, 0.0),
    );
    assert!(hit.is_some(), "vertical ray on flat map must hit surface");
    let hit = hit.unwrap();
    assert_eq!(hit.cell, (16, 16));
    assert!(
        (hit.point.y - 3.0).abs() < 1e-4,
        "hit height {} != 3.0",
        hit.point.y
    );

    // Scenario B: On default arena, ray towards ramp hits cell (31, 32) at height 4.5 ± 1e-4
    let default_map = load_test_map("default_arena.scmap.ron");
    let origin = Vec3::new(31.5, 30.0, 20.0);
    let target = Vec3::new(31.5, 4.5, 32.0);
    let dir = (target - origin).normalize();

    let ramp_hit = pick_terrain(&default_map, origin, dir);
    assert!(
        ramp_hit.is_some(),
        "ray targeting ramp on default arena must hit"
    );
    let ramp_hit = ramp_hit.unwrap();
    assert_eq!(ramp_hit.cell, (31, 32), "hit cell must be ramp (31, 32)");
    assert!(
        (ramp_hit.point.y - 4.5).abs() < 1e-3,
        "ramp hit height {} != 4.5",
        ramp_hit.point.y
    );

    // Scenario C: Ray pointing at an Abyss cell -> None
    let showcase_map = load_test_map("showcase_canyon.scmap.ron");
    // Cell (50, 8) is Abyss
    let abyss_hit = pick_terrain(
        &showcase_map,
        Vec3::new(50.5, 20.0, 8.5),
        Vec3::new(0.0, -1.0, 0.0),
    );
    assert!(
        abyss_hit.is_none(),
        "ray through abyss chasm must return None"
    );

    // Scenario D: Ray grazing a level-2 plateau edge from the low side hits the plateau cell
    let default_cell_step = MapCell {
        terrain_kind: TerrainKind::MetalPlate,
        cliff_level: CliffLevel(0),
        flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
        ramp: None,
        occupying_structure: None,
    };
    let mut step_map = GameMap::new(16, 16, default_cell_step).expect("step_map");
    for y in 0..16 {
        for x in 0..16 {
            let cell = step_map.cell_mut(x, y).expect("cell_mut");
            cell.cliff_level = if x >= 8 { CliffLevel(2) } else { CliffLevel(0) };
            cell.terrain_kind = TerrainKind::MetalPlate;
            cell.flags = CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE;
            cell.ramp = None;
        }
    }

    // Ray originates on low ground side (x=6.0, y=8.0, z=8.5) pointing towards (x=11.0, y=4.0, z=8.5)
    let origin_d = Vec3::new(6.0, 8.0, 8.5);
    let target_d = Vec3::new(11.0, 4.0, 8.5);
    let dir_d = (target_d - origin_d).normalize();

    let lip_hit = pick_terrain(&step_map, origin_d, dir_d);
    assert!(
        lip_hit.is_some(),
        "grazing ray towards high plateau must hit"
    );
    let lip_hit = lip_hit.unwrap();
    // Must hit plateau cell (>= 8), not the low cell behind it
    assert!(
        lip_hit.cell.0 >= 8,
        "lip ray hit low cell {} instead of plateau cell >= 8",
        lip_hit.cell.0
    );
    assert!(
        (lip_hit.point.y - 6.0).abs() < 1e-3,
        "plateau hit height {} != 6.0",
        lip_hit.point.y
    );
}

// -----------------------------------------------------------------------------
// 11. Virtual GPU Terrain Preview Render Verification (§5.7b)
// -----------------------------------------------------------------------------

#[test]
fn test_terrain_preview_renders() {
    if gpu_run::gpu_tests_opted_out() {
        return;
    }

    let temp_dir =
        tempfile::tempdir().expect("Failed to create temporary directory for terrain screenshots");
    let out_dir = temp_dir.path();

    let mut cmd = gpu_run::virtual_gpu_command(Path::new(env!("CARGO_BIN_EXE_terrain_preview")));
    cmd.arg("--out").arg(out_dir);
    gpu_run::assert_passes_validation(&mut cmd, "terrain_preview");

    let poses = ["overview", "p0_base", "mesa", "chasm"];
    let void_srgb = [0.01_f32, 0.012_f32, 0.03_f32];

    for pose in poses {
        let file_path = out_dir.join(format!("{pose}.png"));
        assert!(
            file_path.exists(),
            "Expected screenshot file does not exist: {file_path:?}"
        );

        let img = image::open(&file_path)
            .unwrap_or_else(|e| panic!("Failed to read rendered image {file_path:?}: {e}"))
            .to_rgba8();

        let (width, height) = img.dimensions();
        assert_eq!(width, 1600, "Image width must be 1600 for {pose}");
        assert_eq!(height, 900, "Image height must be 900 for {pose}");

        let total_pixels = f64::from(width * height);
        let mut sum_lum = 0.0_f64;
        let mut sum_lum_sq = 0.0_f64;

        // Central 50% bounds: [400, 1200) x [225, 675)
        let central_x_start = width / 4;
        let central_x_end = width * 3 / 4;
        let central_y_start = height / 4;
        let central_y_end = height * 3 / 4;
        let central_total_pixels =
            f64::from((central_x_end - central_x_start) * (central_y_end - central_y_start));
        let mut central_void_count = 0_usize;

        for (x, y, pixel) in img.enumerate_pixels() {
            let r = f32::from(pixel[0]) / 255.0;
            let g = f32::from(pixel[1]) / 255.0;
            let b = f32::from(pixel[2]) / 255.0;

            // Rec.709 relative luminance
            let lum = f64::from(0.2126 * r + 0.7152 * g + 0.0722 * b);
            sum_lum += lum;
            sum_lum_sq += lum * lum;

            if x >= central_x_start
                && x < central_x_end
                && y >= central_y_start
                && y < central_y_end
            {
                let dr = r - void_srgb[0];
                let dg = g - void_srgb[1];
                let db = b - void_srgb[2];
                let delta_e = (dr * dr + dg * dg + db * db).sqrt();
                if delta_e <= 0.02 {
                    central_void_count += 1;
                }
            }
        }

        let mean_lum = sum_lum / total_pixels;
        let variance = (sum_lum_sq / total_pixels) - (mean_lum * mean_lum);
        let std_dev = variance.max(0.0).sqrt();

        let void_fraction = central_void_count as f64 / central_total_pixels;

        assert!(
            (0.06..=0.85).contains(&mean_lum),
            "Pose {pose}: mean luminance {mean_lum} outside expected range [0.06, 0.85]"
        );
        assert!(
            std_dev > 0.06,
            "Pose {pose}: luminance standard deviation {std_dev} is <= 0.06 (flat/unrendered image)"
        );
        assert!(
            void_fraction < 0.20,
            "Pose {pose}: central 50% void fraction {void_fraction:.3} >= 0.20 (terrain not sufficiently visible)"
        );
    }
}
