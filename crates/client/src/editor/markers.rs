//! Holographic marker rendering and document reconciliation for map entities.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.4 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this module renders map entities (resources, watchtowers, and start locations) as additive
//! holographic projections with cylindrical yaw-only camera billboarding and flat decal quads.
//!
//! # Invariants Maintained
//! 1. **Zero Per-Frame Allocations:** Billboard transforms are updated in place each frame
//!    without heap allocations.
//! 2. **Additive Unlit Projection:** Entity textures are rendered using `StandardMaterial` with
//!    additive blending (`AlphaMode::Add`) and unlit shading on pure black backgrounds.
//! 3. **Entity Id Reconciliation:** Markers are mapped 1-to-1 with document entities via
//!    [`MapEntityId`] in a [`BTreeMap<MapEntityId, Entity>`], synchronized only when entities change.
//! 4. **Surface Alignment:** Billboard quads have their base precisely at the terrain surface;
//!    start location decal quads sit at `surface + 0.02` to prevent Z-fighting.

use std::collections::BTreeMap;
use std::f32::consts::FRAC_PI_2;
use std::path::Path;

use bevy::prelude::*;
use serde::Deserialize;
use sim_map::grid::ResourceKind;
use sim_map::map_format::{MapDocument, MapEntityId};
use sim_types::{SimScalar, SimVec2};

use crate::camera::RtsCamera;
use crate::editor::document::EditorDocument;
use crate::terrain::TerrainState;

fn default_marker_size() -> f32 {
    1.5
}

/// Visual entry for a marker billboard sprite.
#[derive(Debug, Clone, Deserialize)]
pub struct MarkerSpriteEntry {
    /// Relative asset path to the sprite texture.
    pub sprite: String,
    /// Vertical and horizontal size in world units.
    #[serde(default = "default_marker_size")]
    pub size: f32,
    /// Prompt used to generate the asset.
    pub prompt: String,
}

/// Visual entry for a start location decal emblem.
#[derive(Debug, Clone, Deserialize)]
pub struct StartLocationVisualEntry {
    /// Relative asset path to the emblem texture.
    pub sprite: String,
    /// Prompt used to generate the asset.
    pub prompt: String,
}

/// Declarative catalog describing entity visuals from `assets/catalog/map_entity_visuals.ron`.
#[derive(Debug, Clone, Deserialize, Resource)]
pub struct MapEntityVisualCatalog {
    /// Mineral cluster visual specification.
    pub mineral: MarkerSpriteEntry,
    /// Plasma geyser vent visual specification.
    pub gas: MarkerSpriteEntry,
    /// Sensor watchtower visual specification.
    pub watchtower: MarkerSpriteEntry,
    /// Start location landing pad visual specification.
    pub start_location: StartLocationVisualEntry,
    /// Canonical sRGB player colors for start location decal tinting.
    pub player_colors_srgb: Vec<(f32, f32, f32)>,
}

impl MapEntityVisualCatalog {
    /// Loads the catalog from a filesystem path.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let content = std::fs::read_to_string(path)?;
        let catalog = ron::from_str(&content)?;
        Ok(catalog)
    }
}

/// ECS component tagging a spawned entity marker.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapMarkerComponent {
    /// Authoritative document entity ID.
    pub id: MapEntityId,
    /// Whether this marker is a cylindrical camera billboard.
    pub is_billboard: bool,
}

/// ECS Resource storing handles to shared quad meshes and loaded marker materials.
#[derive(Resource)]
pub struct MarkerAssets {
    /// Quad mesh for mineral billboard.
    pub mineral_mesh: Handle<Mesh>,
    /// Material for mineral billboard.
    pub mineral_material: Handle<StandardMaterial>,
    /// Quad mesh for gas billboard.
    pub gas_mesh: Handle<Mesh>,
    /// Material for gas billboard.
    pub gas_material: Handle<StandardMaterial>,
    /// Quad mesh for watchtower billboard.
    pub watchtower_mesh: Handle<Mesh>,
    /// Material for watchtower billboard.
    pub watchtower_material: Handle<StandardMaterial>,
    /// Texture handle for start location emblem.
    pub start_texture: Handle<Image>,
    /// Materials for start location decals, indexed by player slot.
    pub start_materials: Vec<Handle<StandardMaterial>>,
}

/// ECS Resource mapping authoritative [`MapEntityId`] to spawned Bevy [`Entity`].
#[derive(Resource, Debug, Default)]
pub struct MarkerEntities(pub BTreeMap<MapEntityId, Entity>);

/// Initializes marker assets from the loaded visual catalog.
pub fn setup_marker_assets(
    mut commands: Commands<'_, '_>,
    asset_server: Res<'_, AssetServer>,
    catalog: Res<'_, MapEntityVisualCatalog>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
) {
    let mineral_tex = asset_server.load(&catalog.mineral.sprite);
    let mineral_mesh = meshes.add(Rectangle::new(catalog.mineral.size, catalog.mineral.size));
    let mineral_mat = materials.add(StandardMaterial {
        base_color_texture: Some(mineral_tex),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });

    let gas_tex = asset_server.load(&catalog.gas.sprite);
    let gas_mesh = meshes.add(Rectangle::new(catalog.gas.size, catalog.gas.size));
    let gas_mat = materials.add(StandardMaterial {
        base_color_texture: Some(gas_tex),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });

    let watchtower_tex = asset_server.load(&catalog.watchtower.sprite);
    let watchtower_mesh = meshes.add(Rectangle::new(
        catalog.watchtower.size,
        catalog.watchtower.size,
    ));
    let watchtower_mat = materials.add(StandardMaterial {
        base_color_texture: Some(watchtower_tex),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });

    let start_tex = asset_server.load(&catalog.start_location.sprite);
    let mut start_mats = Vec::with_capacity(catalog.player_colors_srgb.len());
    for &(r, g, b) in &catalog.player_colors_srgb {
        start_mats.push(materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            base_color_texture: Some(start_tex.clone()),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            ..default()
        }));
    }

    commands.insert_resource(MarkerAssets {
        mineral_mesh,
        mineral_material: mineral_mat,
        gas_mesh,
        gas_material: gas_mat,
        watchtower_mesh,
        watchtower_material: watchtower_mat,
        start_texture: start_tex,
        start_materials: start_mats,
    });
    commands.init_resource::<MarkerEntities>();
}

/// Updates cylindrical yaw-only rotations for all billboard markers to face the active camera.
// PERFORMANCE RATIONALE:
// Why: Billboard orientation must track camera movement every frame.
// What: Modifies Transform::rotation in place directly inside the ECS query with zero allocations.
// Trade-off: O(M) simple trigonometric yaw updates where M is the small count of map markers.
pub fn update_marker_billboards_system(
    cam_query: Query<'_, '_, &GlobalTransform, With<RtsCamera>>,
    mut billboard_query: Query<'_, '_, (&GlobalTransform, &mut Transform, &MapMarkerComponent)>,
) {
    let Ok(cam_gtf) = cam_query.get_single() else {
        return;
    };
    let cam_pos = cam_gtf.translation();

    for (marker_gtf, mut transform, marker) in &mut billboard_query {
        if !marker.is_billboard {
            continue;
        }

        let ent_pos = marker_gtf.translation();
        let dx = cam_pos.x - ent_pos.x;
        let dz = cam_pos.z - ent_pos.z;

        if dx * dx + dz * dz > 1e-4 {
            let yaw = dx.atan2(dz);
            transform.rotation = Quat::from_rotation_y(yaw);
        }
    }
}

/// Synchronizes spawned marker entities with the authoritative document.
pub fn reconcile_marker_entities(
    commands: &mut Commands<'_, '_>,
    doc: &MapDocument,
    terrain: &TerrainState,
    catalog: &MapEntityVisualCatalog,
    assets: &MarkerAssets,
    marker_map: &mut MarkerEntities,
    meshes: &mut Assets<Mesh>,
) {
    let mut current_ids = BTreeMap::new();

    // 1. Gather all document entities
    for res in &doc.resources {
        let (mesh, mat, size) = match res.kind {
            ResourceKind::Mineral => (
                assets.mineral_mesh.clone(),
                assets.mineral_material.clone(),
                catalog.mineral.size,
            ),
            ResourceKind::Gas => (
                assets.gas_mesh.clone(),
                assets.gas_material.clone(),
                catalog.gas.size,
            ),
        };
        let center_x = res.cell.0 as f32 + 0.5;
        let center_z = res.cell.1 as f32 + 0.5;
        let ground_y = terrain.map.terrain_altitude_at(SimVec2::new(
            SimScalar::from_num(center_x),
            SimScalar::from_num(center_z),
        ));
        let translation = Vec3::new(center_x, ground_y + size * 0.5, center_z);

        current_ids.insert(res.id, (mesh, mat, translation, true, Quat::IDENTITY));
    }

    for tower in &doc.watchtowers {
        let size = catalog.watchtower.size;
        let center_x = tower.cell.0 as f32 + 0.5;
        let center_z = tower.cell.1 as f32 + 0.5;
        let ground_y = terrain.map.terrain_altitude_at(SimVec2::new(
            SimScalar::from_num(center_x),
            SimScalar::from_num(center_z),
        ));
        let translation = Vec3::new(center_x, ground_y + size * 0.5, center_z);

        current_ids.insert(
            tower.id,
            (
                assets.watchtower_mesh.clone(),
                assets.watchtower_material.clone(),
                translation,
                true,
                Quat::IDENTITY,
            ),
        );
    }

    for start in &doc.start_locations {
        let (fw, fh) = (start.footprint.0 as f32, start.footprint.1 as f32);
        let center_x = start.cell.0 as f32 + fw * 0.5;
        let center_z = start.cell.1 as f32 + fh * 0.5;
        let ground_y = terrain.map.terrain_altitude_at(SimVec2::new(
            SimScalar::from_num(center_x),
            SimScalar::from_num(center_z),
        ));
        let translation = Vec3::new(center_x, ground_y + 0.02, center_z);

        let mesh = meshes.add(Rectangle::new(fw, fh));
        let color_idx = (start.player_slot as usize) % assets.start_materials.len();
        let mat = assets.start_materials[color_idx].clone();
        let rotation = Quat::from_rotation_x(-FRAC_PI_2);

        current_ids.insert(start.id, (mesh, mat, translation, false, rotation));
    }

    // 2. Despawn removed entities
    let removed_ids: Vec<MapEntityId> = marker_map
        .0
        .keys()
        .copied()
        .filter(|id| !current_ids.contains_key(id))
        .collect();

    for id in removed_ids {
        if let Some(entity) = marker_map.0.remove(&id) {
            commands.entity(entity).despawn_recursive();
        }
    }

    // 3. Spawn or update remaining entities
    for (id, (mesh, mat, translation, is_billboard, rotation)) in current_ids {
        if let Some(&entity) = marker_map.0.get(&id) {
            commands.entity(entity).insert((
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_translation(translation).with_rotation(rotation),
            ));
        } else {
            let entity = commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::from_translation(translation).with_rotation(rotation),
                    Visibility::default(),
                    MapMarkerComponent { id, is_billboard },
                ))
                .id();
            marker_map.0.insert(id, entity);
        }
    }
}

/// System synchronizing marker entities whenever the document or terrain revision advances.
pub fn sync_marker_entities_system(
    mut commands: Commands<'_, '_>,
    doc: Res<'_, EditorDocument>,
    terrain: Res<'_, TerrainState>,
    catalog: Option<Res<'_, MapEntityVisualCatalog>>,
    assets: Option<Res<'_, MarkerAssets>>,
    mut marker_map: ResMut<'_, MarkerEntities>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
) {
    let (Some(cat), Some(ast)) = (catalog, assets) else {
        return;
    };

    if doc.is_changed() || terrain.is_changed() {
        reconcile_marker_entities(
            &mut commands,
            &doc.doc,
            &terrain,
            &cat,
            &ast,
            &mut marker_map,
            &mut meshes,
        );
    }
}
