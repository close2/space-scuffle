//! Application wiring, state transitions, and plugin assembly for the map editor.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.1 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this module provides the primary [`MapEditorPlugin`] assembling all sub-plugins, schedules,
//! resources, and systems required to run the standalone map editor and its render verification mode.
//!
//! # Invariants Maintained
//! 1. **Complete Subsystem Assembly:** Registers terrain rendering, bubble domes, tactical indicators,
//!    visual FX, camera navigation, holographic markers, egui UI, and deterministic playtest stepping.
//! 2. **State Cleanliness:** Live battle systems are registered strictly under `run_if(in_state(EditorPhase::Playtesting))`,
//!    preventing any interference with normal map authoring.
//! 3. **Non-Destructive Playtest Isolation:** Playtest initializes a separate simulation world and
//!    completely tears down all live bubbles and visual FX upon returning to `EditorPhase::Editing`.
//! 4. **Acyclic Module Organization:** Does not depend on the main Visual Lab ui (`lab_ui_system`)
//!    or arena environment generators (`spawn_arena_environment`).

#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use asset_baker::TerrainThemeCatalog;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::render::{Render, RenderApp};
use sim_core::scenario::{build_world, PlayerSetup, StartLoadoutCatalog};
use sim_map::map_format::{MapFile, MapMetadata};

use crate::terrain::TerrainState;

use crate::bubble_shader::{setup_bubble_assets_system, BubbleDomeMaterial};
use crate::camera::RtsCamera;
use crate::editor::camera::{editor_camera_system, EditorCameraFocus};
use crate::editor::commands::History;
use crate::editor::document::EditorDocument;
use crate::editor::file_browser::FileBrowserState;
use crate::editor::los_preview::{
    update_los_preview_system, LosPreviewState, StartLoadoutCatalogResource,
};
use crate::editor::markers::{
    setup_marker_assets, sync_marker_entities_system, update_marker_billboards_system,
    MapEntityVisualCatalog,
};
use crate::editor::playtest::{
    playtest_input_system, playtest_step_system, teardown_playtest_system, EditorPhase,
    PlaytestState, PLAYTEST_SEED,
};
use crate::editor::ui::{
    editor_ui_system, EditorToolState, NewMapModalState, UnsavedChangesModalState,
};
use crate::editor::verification::{
    check_render_pipelines, update_render_verification_system, PipelineSync, VerifyRenderState,
};
use crate::indicators::{
    setup_indicator_assets_system, update_indicators_system, SelectionRingMaterial,
};
use crate::lab_ui::{LabState, LabTab};
use crate::scene_controller::{sync_live_battle_system, RenderEntityMap};
use crate::selection::SelectionState;
use crate::terrain::{spawn_terrain, TerrainMaterial, TerrainPlugin};
use crate::vfx::{
    process_combat_events_system, setup_vfx_assets_system, update_beams_system,
    update_melee_slashes_system, update_mist_wake_system, update_projectiles_system,
    update_shatter_system, CombatFxMaterial, CombatVisualEvent, MistWakeMaterial,
};
use crate::{frame_limit_system, FrameLimit};

/// Newtype resource wrapper around [`TerrainThemeCatalog`] for Bevy ECS insertion.
#[derive(Resource, Debug, Clone, Deref, DerefMut)]
pub struct ThemeCatalogResource(pub TerrainThemeCatalog);

/// Bevy plugin assembling the complete Space Scuffle map editor application.
#[derive(Default)]
pub struct MapEditorPlugin {
    /// Optional file path of an initial map to open on startup.
    pub map_path: Option<PathBuf>,
    /// Optional dimensions `(width, height)` for initializing a blank map on startup.
    pub new_dimensions: Option<(u32, u32)>,
    /// Optional output directory for automated render verification screenshots.
    pub verify_render: Option<PathBuf>,
    /// Optional frame count cap before automatic clean shutdown.
    pub max_frames: Option<u64>,
}

impl Plugin for MapEditorPlugin {
    fn build(&self, app: &mut App) {
        let assets_dir = crate::resolve_assets_dir();

        // 1. Load Catalogs
        let themes_path = assets_dir.join("catalog/terrain_themes.ron");
        let theme_catalog = TerrainThemeCatalog::load(&themes_path).unwrap_or_else(|e| {
            panic!(
                "failed to load theme catalog at {}: {e}",
                themes_path.display()
            )
        });

        let loadouts_path = assets_dir.join("catalog/start_loadouts.ron");
        let start_loadouts = StartLoadoutCatalog::load(&loadouts_path).unwrap_or_else(|e| {
            panic!(
                "failed to load start loadouts at {}: {e}",
                loadouts_path.display()
            )
        });

        let visuals_path = assets_dir.join("catalog/map_entity_visuals.ron");
        let entity_visuals = MapEntityVisualCatalog::load(&visuals_path).unwrap_or_else(|e| {
            panic!(
                "failed to load entity visuals at {}: {e}",
                visuals_path.display()
            )
        });

        // 2. Initialize Initial Document
        let initial_doc = if let Some(ref path) = self.map_path {
            let file = MapFile::load(path)
                .unwrap_or_else(|e| panic!("failed to load map at {}: {e}", path.display()));
            let doc = file
                .decode()
                .unwrap_or_else(|e| panic!("failed to decode map at {}: {e}", path.display()));
            EditorDocument::new(doc, Some(path.clone()))
        } else if let Some((w, h)) = self.new_dimensions {
            let meta = MapMetadata {
                theme: "orbital_foundry".to_string(),
                ..MapMetadata::default()
            };
            EditorDocument::new_blank(w, h, meta).expect("valid new blank map dimensions")
        } else {
            let default_path = assets_dir.join("maps/showcase_canyon.scmap.ron");
            if default_path.exists() {
                let file = MapFile::load(&default_path)
                    .unwrap_or_else(|e| panic!("failed to load default map: {e}"));
                let doc = file
                    .decode()
                    .unwrap_or_else(|e| panic!("failed to decode default map: {e}"));
                EditorDocument::new(doc, Some(default_path))
            } else {
                EditorDocument::new_blank(64, 64, MapMetadata::default())
                    .expect("valid fallback blank map")
            }
        };

        let map_w = initial_doc.doc.width;
        let map_h = initial_doc.doc.height;

        // 3. Register Core Resources
        app.init_state::<EditorPhase>()
            .init_resource::<crate::CatalogResource>()
            .insert_resource(ThemeCatalogResource(theme_catalog))
            .insert_resource(StartLoadoutCatalogResource(start_loadouts))
            .insert_resource(entity_visuals)
            .insert_resource(initial_doc)
            .init_resource::<History>()
            .insert_resource(EditorToolState::new(map_w, map_h))
            .init_resource::<FileBrowserState>()
            .init_resource::<NewMapModalState>()
            .init_resource::<UnsavedChangesModalState>()
            .insert_resource(LosPreviewState::new(map_w, map_h))
            .init_resource::<PlaytestState>()
            .init_resource::<RenderEntityMap>()
            .init_resource::<SelectionState>()
            .insert_resource(FrameLimit {
                max_frames: self.max_frames,
                current_frame: 0,
                ..default()
            })
            .add_event::<CombatVisualEvent>()
            .add_event::<EditorCameraFocus>()
            .add_plugins(TerrainPlugin)
            .add_plugins(MaterialPlugin::<BubbleDomeMaterial>::default())
            .add_plugins(MaterialPlugin::<SelectionRingMaterial>::default())
            .add_plugins(MaterialPlugin::<MistWakeMaterial>::default())
            .add_plugins(MaterialPlugin::<CombatFxMaterial>::default());

        // 4. Verification Mode Synchronization
        let pipeline_sync = PipelineSync::default();
        if let Some(ref verify_dir) = self.verify_render {
            app.insert_resource(pipeline_sync.clone());
            app.insert_resource(VerifyRenderState::new(verify_dir.clone()));
        }

        // 5. Register Startup Systems
        app.add_systems(
            Startup,
            (
                setup_bubble_assets_system,
                setup_indicator_assets_system,
                setup_vfx_assets_system,
                setup_marker_assets,
                setup_editor_scene,
            ),
        );

        // 6. Register Global & Editing Systems
        app.add_systems(
            Update,
            (
                frame_limit_system,
                editor_camera_system,
                update_marker_billboards_system,
                sync_marker_entities_system,
                editor_los_system.run_if(in_state(EditorPhase::Editing)),
            ),
        );

        if self.verify_render.is_some() {
            app.add_systems(
                Update,
                update_render_verification_system.run_if(in_state(EditorPhase::Editing)),
            );
        } else {
            app.add_systems(
                Update,
                editor_ui_system.run_if(in_state(EditorPhase::Editing)),
            );
        }

        // 7. Register Playtesting Systems
        app.add_systems(OnEnter(EditorPhase::Playtesting), setup_playtest_lab_state);
        app.add_systems(OnExit(EditorPhase::Playtesting), teardown_playtest_system);

        app.add_systems(
            Update,
            (
                playtest_step_system.before(sync_live_battle_system),
                playtest_input_system,
                sync_live_battle_system,
                process_combat_events_system,
                update_indicators_system,
                update_mist_wake_system,
                update_projectiles_system,
                update_beams_system,
                update_melee_slashes_system,
                update_shatter_system,
            )
                .run_if(in_state(EditorPhase::Playtesting)),
        );

        // 8. Hook RenderApp for PipelineCache error checking in verify mode
        if self.verify_render.is_some() {
            if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
                render_app.insert_resource(pipeline_sync);
                render_app.add_systems(Render, check_render_pipelines);
            }
        }
    }
}

fn editor_los_system(
    los_state: ResMut<'_, LosPreviewState>,
    doc: Res<'_, EditorDocument>,
    terrain: Res<'_, TerrainState>,
    terrain_params: ResMut<'_, crate::terrain::TerrainParamsState>,
    field_images: Option<Res<'_, crate::terrain::TerrainFieldImages>>,
    images: ResMut<'_, Assets<Image>>,
    catalog: Option<Res<'_, crate::CatalogResource>>,
    loadouts: Option<Res<'_, StartLoadoutCatalogResource>>,
    tool_state: Res<'_, EditorToolState>,
) {
    let slug = &tool_state.player_loadouts[0];
    update_los_preview_system(
        los_state,
        doc,
        terrain,
        terrain_params,
        field_images,
        images,
        catalog,
        loadouts,
        tool_state.hovered_cell,
        slug,
    );
}

/// Spawns the initial 3D RTS camera and instantiates the shared terrain renderer hierarchy.
fn setup_editor_scene(
    mut commands: Commands<'_, '_>,
    doc: Res<'_, EditorDocument>,
    theme_catalog: Res<'_, ThemeCatalogResource>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut terrain_materials: ResMut<'_, Assets<TerrainMaterial>>,
    mut std_materials: ResMut<'_, Assets<StandardMaterial>>,
    mut images: ResMut<'_, Assets<Image>>,
) {
    let assets_dir = crate::resolve_assets_dir();

    // 1. Look up theme from catalog
    let theme_slug = &doc.doc.metadata.theme;
    let theme = theme_catalog
        .get(theme_slug)
        .or_else(|| theme_catalog.themes.first())
        .expect("at least one terrain theme must exist in catalog");

    let game_map = doc
        .doc
        .to_game_map()
        .unwrap_or_else(|e| panic!("failed to convert document to GameMap: {e}"));

    // 2. Spawn Shared Terrain Hierarchy
    spawn_terrain(
        &mut commands,
        &mut meshes,
        &mut terrain_materials,
        &mut std_materials,
        &mut images,
        theme,
        game_map,
        &assets_dir,
    )
    .expect("failed to spawn shared terrain hierarchy");

    // 3. Spawn Camera
    let center_x = (doc.doc.width as f32) * 0.5;
    let center_z = (doc.doc.height as f32) * 0.5;
    let cam = RtsCamera {
        target: Vec2::new(center_x, center_z),
        distance: 40.0,
        ..default()
    };
    let transform = cam.compute_transform();

    commands.spawn((Camera3d::default(), transform, cam));
}

/// Sets up the paused [`LabState`] when entering playtesting mode.
fn setup_playtest_lab_state(
    mut commands: Commands<'_, '_>,
    doc: Res<'_, EditorDocument>,
    loadouts: Res<'_, StartLoadoutCatalogResource>,
    tool_state: Res<'_, EditorToolState>,
) {
    let players: Vec<PlayerSetup> = (0..doc.doc.start_locations.len() as u8)
        .map(|slot| PlayerSetup {
            player_slot: slot,
            loadout_slug: tool_state.player_loadouts[slot as usize].clone(),
        })
        .collect();

    let world = build_world(&doc.doc, &loadouts.0, &players, PLAYTEST_SEED)
        .expect("validated map must successfully construct SimWorld");

    commands.insert_resource(LabState {
        current_tab: LabTab::LiveBattle,
        sim_world: Some(world),
        battle_paused: true,
        ..default()
    });
}
