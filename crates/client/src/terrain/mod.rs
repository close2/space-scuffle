//! Shared terrain rendering subsystem for Space Scuffle.
//!
//! # Architectural Charter
//! This module owns the high-performance, deterministic dual-grid terrain renderer,
//! texture array asset binding, GPU overlays, and picking routines used by both the
//! map editor and match playtest.
//!
//! # Invariants
//! - Visual cliff walls align strictly with simulation cell boundaries via dual-grid marching squares.
//! - Renders with zero per-frame heap allocations during steady-state terrain updates.
//! - Rebuilt chunk entities immediately update their [`Aabb`] to prevent frustum culling errors.
//! - Unwalkable abyss drops fade into the void backdrop according to theme settings.

pub mod fields;
pub mod material;
pub mod mesher;
pub mod picking;
pub mod theme_gpu;

pub use fields::{
    create_overlay_image, create_splat_image, update_overlay_image, update_splat_image,
};
pub use material::{
    create_default_terrain_base_material, TerrainExtension, TerrainMaterial, TerrainParams,
    HEX_CONTRAST, OVERLAY_FLAG_BRUSH, OVERLAY_FLAG_BUILDABILITY, OVERLAY_FLAG_GRID,
    OVERLAY_FLAG_LOS, OVERLAY_FLAG_PATHABILITY, OVERLAY_FLAG_VALIDATION,
};
pub use mesher::{
    apply_scratch_to_mesh, build_chunk_mesh, cliff_level_height, compute_floor_ao,
    generate_wall_profile, is_ramp_connected, pcg2d, surface_class, MeshStats, MesherScratch,
    SurfaceClass, ABYSS_WALL_BOTTOM_Y, AO_RADIUS_CELLS, AO_STRENGTH, ATTRIBUTE_UV_1,
    CHUNK_DUAL_CELLS, FOOT_RISE, FOOT_WIDTH, MAX_CHUNK_REBUILDS_PER_FRAME, RAIL_HEIGHT, RAIL_WIDTH,
    RIM_DROP, RIM_WIDTH, WALL_FOOT_AO, WALL_JITTER_AMPLITUDE, WALL_ROW_SPACING,
};
pub use picking::{pick_terrain, TerrainHit};
pub use theme_gpu::{
    load_theme_texture_arrays, setup_theme_backdrop, setup_theme_lighting, ThemeGpuTextures,
};

use asset_baker::{TerrainTheme, TerrainThemeError};
use bevy::pbr::{ExtendedMaterial, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::mesh::MeshAabb;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;
use sim_map::grid::{GameMap, MapCell};
use sim_map::validation::CellRect;
use std::path::Path;

/// ECS Resource managing authoritative map data and change notification revisions.
#[derive(Resource)]
pub struct TerrainState {
    /// Authoritative simulation game map.
    pub map: GameMap,
    /// Monotonically increasing revision counter incremented on every modification.
    pub revision: u64,
    /// Bounding rectangle of recently edited cells awaiting chunk rebuilds.
    pub dirty_cells: Option<CellRect>,
}

impl TerrainState {
    /// Creates a new terrain state wrapping the given map.
    #[must_use]
    pub fn new(map: GameMap) -> Self {
        Self {
            map,
            revision: 0,
            dirty_cells: None,
        }
    }

    /// Completely replaces the underlying map and triggers a full chunk rebuild.
    pub fn replace_map(&mut self, map: GameMap) {
        self.map = map;
        self.revision = self.revision.wrapping_add(1);
        self.dirty_cells = Some(CellRect {
            min: (0, 0),
            max_inclusive: (
                self.map.width().saturating_sub(1),
                self.map.height().saturating_sub(1),
            ),
        });
    }

    /// Applies a mutation closure to all cells within `rect` and marks them dirty.
    pub fn apply_cells(&mut self, rect: CellRect, mut f: impl FnMut(u32, u32, &mut MapCell)) {
        let w = self.map.width();
        let h = self.map.height();
        let min_x = rect.min.0.min(w - 1);
        let max_x = rect.max_inclusive.0.min(w - 1);
        let min_y = rect.min.1.min(h - 1);
        let max_y = rect.max_inclusive.1.min(h - 1);

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if let Ok(cell) = self.map.cell_mut(x, y) {
                    f(x, y, cell);
                }
            }
        }

        self.mark_dirty(rect);
    }

    /// Marks a cell rectangle dirty, unioning it with existing unbuilt edits.
    pub fn mark_dirty(&mut self, rect: CellRect) {
        self.revision = self.revision.wrapping_add(1);
        self.dirty_cells = Some(match self.dirty_cells {
            Some(prev) => CellRect {
                min: (prev.min.0.min(rect.min.0), prev.min.1.min(rect.min.1)),
                max_inclusive: (
                    prev.max_inclusive.0.max(rect.max_inclusive.0),
                    prev.max_inclusive.1.max(rect.max_inclusive.1),
                ),
            },
            None => rect,
        });
    }
}

/// Metadata and handle entry for a single spatial chunk.
#[derive(Debug, Clone)]
pub struct ChunkEntry {
    /// Entity owning this chunk's mesh and transform components.
    pub entity: Entity,
    /// Handle to the chunk's [`Mesh`] asset.
    pub mesh: Handle<Mesh>,
    /// Whether this chunk requires geometry rebuilding.
    pub dirty: bool,
    /// Chunk grid column index.
    pub chunk_x: u32,
    /// Chunk grid row index.
    pub chunk_y: u32,
}

/// ECS Resource managing all partitioned terrain chunks.
#[derive(Resource, Default)]
pub struct TerrainChunks {
    /// Spatial grid chunks in row-major order.
    pub chunks: Vec<ChunkEntry>,
    /// Number of chunk columns.
    pub cols: u32,
    /// Number of chunk rows.
    pub rows: u32,
}

/// ECS Resource holding the handle to the active [`TerrainMaterial`].
#[derive(Resource, Clone, Deref)]
pub struct TerrainMaterialHandle(pub Handle<TerrainMaterial>);

/// ECS Resource holding handles to dynamic splat and tactical overlay textures.
#[derive(Resource, Clone)]
pub struct TerrainFieldImages {
    /// Handle to the RGBA8 splat weight texture.
    pub splat: Handle<Image>,
    /// Handle to the unfilterable RGBA8 tactical overlay texture.
    pub overlay: Handle<Image>,
}

/// ECS Resource storing the CPU copy of terrain shader parameters.
#[derive(Resource, Clone, Deref, DerefMut)]
pub struct TerrainParamsState(pub TerrainParams);

/// Terrain rendering plugin.
pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .init_resource::<MesherScratch>()
            .init_resource::<TerrainChunks>()
            .add_systems(
                Update,
                update_terrain_system.run_if(resource_exists::<TerrainState>),
            );
    }
}

/// System updating dirty terrain chunks, synchronizing uniform parameters,
/// and uploading partial dirty rects to splat and overlay textures.
// PERFORMANCE RATIONALE:
// Why: Rebuilding all chunks on large maps stalls the main thread.
// What: We limit chunk rebuilds to at most `MAX_CHUNK_REBUILDS_PER_FRAME` (8) per frame,
// reusing `MesherScratch` to eliminate dynamic heap allocations.
// Splat and overlay textures are updated at most once per frame across the accumulated dirty rect.
pub fn update_terrain_system(
    mut commands: Commands<'_, '_>,
    mut terrain_state: ResMut<'_, TerrainState>,
    mut terrain_chunks: ResMut<'_, TerrainChunks>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut scratch: ResMut<'_, MesherScratch>,
    mut materials: ResMut<'_, Assets<TerrainMaterial>>,
    mut images: ResMut<'_, Assets<Image>>,
    mat_handle: Option<Res<'_, TerrainMaterialHandle>>,
    params_state: Option<Res<'_, TerrainParamsState>>,
    field_images: Option<Res<'_, TerrainFieldImages>>,
) {
    // 1. Check for newly marked dirty cell regions
    if terrain_state.dirty_cells.is_some() {
        if let Some(rect) = terrain_state.bypass_change_detection().dirty_cells.take() {
            let w = terrain_state.map.width();
            let h = terrain_state.map.height();

            // Write dirty rect into splat and overlay textures at most once per frame
            if let Some(ref fields) = field_images {
                if let Some(splat_img) = images.get_mut(&fields.splat) {
                    update_splat_image(&terrain_state.map, splat_img, Some(rect));
                }
                if let Some(overlay_img) = images.get_mut(&fields.overlay) {
                    update_overlay_image(&terrain_state.map, None, None, overlay_img, Some(rect));
                }
            }

            // Editing cell (x, y) dirties dual cells x-2..=x+3, y-2..=y+3
            let min_dual_x = rect.min.0.saturating_sub(2);
            let max_dual_x = (rect.max_inclusive.0 + 3).min(w);
            let min_dual_y = rect.min.1.saturating_sub(2);
            let max_dual_y = (rect.max_inclusive.1 + 3).min(h);

            let min_chunk_x = min_dual_x / CHUNK_DUAL_CELLS;
            let max_chunk_x = max_dual_x / CHUNK_DUAL_CELLS;
            let min_chunk_y = min_dual_y / CHUNK_DUAL_CELLS;
            let max_chunk_y = max_dual_y / CHUNK_DUAL_CELLS;

            for chunk in &mut terrain_chunks.chunks {
                if chunk.chunk_x >= min_chunk_x
                    && chunk.chunk_x <= max_chunk_x
                    && chunk.chunk_y >= min_chunk_y
                    && chunk.chunk_y <= max_chunk_y
                {
                    chunk.dirty = true;
                }
            }
        }
    }

    // 2. Rebuild up to MAX_CHUNK_REBUILDS_PER_FRAME dirty chunks
    let mut rebuild_count = 0;
    for chunk in &mut terrain_chunks.chunks {
        if chunk.dirty {
            if let Some(mesh) = meshes.get_mut(&chunk.mesh) {
                build_chunk_mesh(
                    &terrain_state.map,
                    chunk.chunk_x,
                    chunk.chunk_y,
                    &mut scratch,
                );
                apply_scratch_to_mesh(&scratch, mesh);

                // Bevy 0.15 does not automatically recompute AABBs after mesh buffer mutation
                if let Some(aabb) = mesh.compute_aabb() {
                    commands.entity(chunk.entity).insert(aabb);
                }

                chunk.dirty = false;
                rebuild_count += 1;
                if rebuild_count >= MAX_CHUNK_REBUILDS_PER_FRAME {
                    break;
                }
            }
        }
    }

    // 3. Sync uniform params to GPU material if changed
    if let Some(ref params) = params_state {
        if params.is_changed() {
            if let Some(handle) = mat_handle {
                if let Some(mat) = materials.get_mut(&handle.0) {
                    mat.extension.params = ***params;
                }
            }
        }
    }
}

/// Spawns the complete terrain hierarchy: material, texture arrays, splat/overlay fields,
/// chunks, directional/ambient lighting, and void backdrop plane.
///
/// # Errors
/// Returns [`TerrainThemeError`] if required baked texture files cannot be loaded from `assets_dir`.
pub fn spawn_terrain(
    commands: &mut Commands<'_, '_>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TerrainMaterial>,
    std_materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    theme: &TerrainTheme,
    map: GameMap,
    assets_dir: &Path,
) -> Result<(), TerrainThemeError> {
    spawn_terrain_owned(
        commands,
        meshes,
        materials,
        std_materials,
        images,
        theme,
        map,
        assets_dir,
        None,
        None,
    )
}

/// Spawns terrain with optional explicit ownership for runtime reset and tab visibility.
///
/// All chunks, the directional light and the backdrop become children of `owner`.
/// Existing editor callers use [`spawn_terrain`] without a runtime owner.
///
/// # Errors
/// Returns [`TerrainThemeError`] if a required real texture cannot be loaded.
pub fn spawn_terrain_owned(
    commands: &mut Commands<'_, '_>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TerrainMaterial>,
    std_materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    theme: &TerrainTheme,
    map: GameMap,
    assets_dir: &Path,
    owner: Option<Entity>,
    overlay_flags: Option<u32>,
) -> Result<(), TerrainThemeError> {
    let width = map.width();
    let height = map.height();

    // 1. Load theme texture arrays with full CPU mip chains
    let gpu_textures = load_theme_texture_arrays(theme, assets_dir)?;
    let albedo_handle = images.add(gpu_textures.albedo);
    let normal_handle = images.add(gpu_textures.normal);
    let orm_handle = images.add(gpu_textures.orm);

    // 2. Generate splat and overlay field images
    let splat_image = create_splat_image(&map);
    let overlay_image = create_overlay_image(&map);
    let splat_handle = images.add(splat_image);
    let overlay_handle = images.add(overlay_image);

    // 3. Construct TerrainParams uniform
    let mut params = TerrainParams::from_theme(theme, width, height);
    if let Some(flags) = overlay_flags {
        params.overlay_flags = flags;
    }

    // 4. Construct extended material
    let base_material = create_default_terrain_base_material();
    let extension = TerrainExtension {
        params,
        albedo: albedo_handle,
        normal: normal_handle,
        orm: orm_handle,
        splat: splat_handle.clone(),
        overlay: overlay_handle.clone(),
    };
    let mat_handle = materials.add(ExtendedMaterial {
        base: base_material,
        extension,
    });

    // 5. Build and spawn all terrain spatial chunks
    let cols = width.div_ceil(CHUNK_DUAL_CELLS);
    let rows = height.div_ceil(CHUNK_DUAL_CELLS);

    let mut scratch = MesherScratch::default();
    let mut chunk_entries = Vec::with_capacity((cols * rows) as usize);

    for chunk_y in 0..rows {
        for chunk_x in 0..cols {
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
            build_chunk_mesh(&map, chunk_x, chunk_y, &mut scratch);
            apply_scratch_to_mesh(&scratch, &mut mesh);

            let aabb = mesh.compute_aabb();
            let mesh_handle = meshes.add(mesh);

            let mut ent = commands.spawn((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(mat_handle.clone()),
                Transform::IDENTITY,
                Visibility::default(),
            ));
            if let Some(aabb) = aabb {
                ent.insert(aabb);
            }
            if let Some(parent) = owner {
                ent.set_parent(parent);
            }

            chunk_entries.push(ChunkEntry {
                entity: ent.id(),
                mesh: mesh_handle,
                dirty: false,
                chunk_x,
                chunk_y,
            });
        }
    }

    // 7. Setup lighting and void backdrop from theme
    let backdrop = setup_theme_backdrop(
        commands,
        theme,
        meshes,
        std_materials,
        images,
        assets_dir,
        Vec2::new(width as f32, height as f32),
    )?;

    if let Some(parent) = owner {
        commands.entity(backdrop).set_parent(parent);
    }
    theme_gpu::setup_theme_lighting_owned(commands, theme, owner);

    // 6. Insert resources
    commands.insert_resource(TerrainState::new(map));
    commands.insert_resource(TerrainChunks {
        chunks: chunk_entries,
        cols,
        rows,
    });
    commands.insert_resource(TerrainMaterialHandle(mat_handle));
    commands.insert_resource(TerrainFieldImages {
        splat: splat_handle,
        overlay: overlay_handle,
    });
    commands.insert_resource(TerrainParamsState(params));

    Ok(())
}
