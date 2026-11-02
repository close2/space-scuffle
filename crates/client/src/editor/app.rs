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
//!    It runs the game client's ordered [`BattleSet`] stages rather than a parallel tick loop.
//! 4. **Unsaved Work Survives Closing:** Bevy's close-on-request behavior is disabled; the window
//!    only closes through [`editor_window_close_system`], in every phase.
//! 5. **Acyclic Module Organization:** Does not depend on the main Visual Lab ui (`lab_ui_system`)
//!    or arena environment generators (`spawn_lab_stage`).

use std::path::PathBuf;

use bevy::app::AppExit;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::render::{Render, RenderApp};
use bevy::window::WindowCloseRequested;

use crate::battle_pipeline::driver::{
    advance_battle_ticks, publish_combat_events, RetainBattleFailure,
};
use crate::battle_pipeline::session::BattleSession;
use crate::battle_pipeline::BattleSet;
use crate::battle_selection::BattleSelection;
use crate::bubble_optics::BubbleOpticsPlugin;
use crate::bubble_shader::{setup_bubble_assets_system, BubbleDomeMaterial};
use crate::camera::RtsCamera;
use crate::editor::camera::{editor_camera_system, EditorCameraFocus};
use crate::editor::commands::History;
use crate::editor::document::EditorDocument;
use crate::editor::document_load::{
    apply_document_load_system, EditorTerrainSpawner, LoadDocumentRequest,
};
use crate::editor::file_browser::FileBrowserState;
use crate::editor::los_preview::{
    update_los_preview_system, LosPreviewState, StartLoadoutCatalogResource,
};
use crate::editor::markers::{
    setup_marker_assets, sync_marker_entities_system, update_marker_billboards_system,
};
use crate::editor::playtest::{
    playtest_exit_system, playtest_hud_system, playtest_input_system, sync_playtest_status_system,
    teardown_playtest_system, EditorPhase, EditorRulesResource, PlaytestState,
};
use crate::editor::startup::EditorStartup;
use crate::editor::ui::{
    editor_ui_system, EditorToolState, NewMapModalState, PendingUnsavedAction,
    UnsavedChangesModalState,
};
use crate::editor::verification::{
    check_render_pipelines, keep_window_surface_in_use, update_playtest_verification_system,
    update_render_verification_system, PipelineSync, VerifyOutputDir, VerifyRenderState,
};
use crate::indicators::{
    setup_indicator_assets_system, update_indicators_system, SelectionRingMaterial,
};
use crate::lab_ui::{LabState, LabTab};
use crate::media::MediaFailurePolicy;
use crate::player_presentation::PresentationStatus;
use crate::scene_controller::{sync_live_battle_system, RenderEntityMap};
use crate::selection::SelectionState;
use crate::terrain::{TerrainPlugin, TerrainState};
use crate::vfx::{
    process_combat_events_system, setup_vfx_assets_system, update_beams_system,
    update_melee_slashes_system, update_projectiles_system, update_shatter_system,
    CombatFxMaterial, CombatVisualEvent,
};
use crate::{frame_limit_system, CatalogResource, FrameLimit};

/// Newtype resource wrapper around [`TerrainThemeCatalog`](asset_baker::TerrainThemeCatalog) for Bevy ECS insertion.
#[derive(Resource, Debug, Clone, Deref, DerefMut)]
pub struct ThemeCatalogResource(pub asset_baker::TerrainThemeCatalog);

/// Returns the window configuration for the editor.
///
/// `close_when_requested` is disabled because Bevy would otherwise destroy the window as soon as
/// the user clicks its X, before [`editor_window_close_system`] can protect unsaved work.
#[must_use]
pub fn editor_window_plugin() -> WindowPlugin {
    WindowPlugin {
        primary_window: Some(Window {
            title: "Space Scuffle — Map Editor".to_string(),
            resolution: (1600.0_f32, 900.0_f32).into(),
            ..default()
        }),
        close_when_requested: false,
        ..default()
    }
}

/// System handling the operating system's window close request in every phase.
///
/// A clean document exits immediately. Unsaved work opens the Save / Discard / Cancel prompt,
/// which renders only while editing, so a dirty playtest first returns to editing.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn editor_window_close_system(
    mut close_events: EventReader<'_, '_, WindowCloseRequested>,
    doc: Res<'_, EditorDocument>,
    phase: Res<'_, State<EditorPhase>>,
    mut next_phase: ResMut<'_, NextState<EditorPhase>>,
    mut modal: ResMut<'_, UnsavedChangesModalState>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if close_events.read().count() == 0 {
        return;
    }
    if !doc.dirty {
        exit.send(AppExit::Success);
        return;
    }
    modal.is_open = true;
    modal.pending_action = Some(PendingUnsavedAction::Quit);
    if *phase.get() == EditorPhase::Playtesting {
        next_phase.set(EditorPhase::Editing);
    }
}

/// Absolute path of the `assets/maps` directory, resolved once at startup.
#[derive(Resource, Debug, Clone, Deref)]
pub struct MapsDirectory(pub PathBuf);

/// Bevy plugin assembling the complete Space Scuffle map editor application.
pub struct MapEditorPlugin {
    /// Catalogs and initial document, loaded and validated before the app is built.
    pub startup: EditorStartup,
    /// Prepared output directory for automated render verification screenshots.
    pub verify_render: Option<VerifyOutputDir>,
    /// Optional frame count cap before automatic clean shutdown.
    pub max_frames: Option<u64>,
}

impl Plugin for MapEditorPlugin {
    #[expect(
        clippy::cast_precision_loss,
        reason = "map cell coordinates stay far below the range of f32 and i32"
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "a single ordered sequence that shares its local state"
    )]
    fn build(&self, app: &mut App) {
        let EditorStartup {
            assets_root,
            themes,
            loadouts,
            visuals,
            catalogs,
            palette,
            document,
        } = self.startup.clone();
        let map_w = document.doc.width;
        let map_h = document.doc.height;
        let automated = self.max_frames.is_some() || self.verify_render.is_some();
        let playtest_focus = document.doc.start_locations.first().map_or_else(
            || Vec2::new(map_w as f32, map_h as f32) * 0.5,
            |start| {
                Vec2::new(
                    start.cell.0 as f32 + f32::from(start.footprint.0) * 0.5,
                    start.cell.1 as f32 + f32::from(start.footprint.1) * 0.5,
                )
            },
        );

        // 1. Register Core Resources
        app.init_state::<EditorPhase>()
            .insert_resource(MapsDirectory(assets_root.as_path().join("maps")))
            .insert_resource(assets_root)
            .insert_resource(CatalogResource(catalogs.units.clone()))
            .insert_resource(EditorRulesResource(catalogs))
            .insert_resource(ThemeCatalogResource(themes))
            .insert_resource(StartLoadoutCatalogResource(loadouts))
            .insert_resource(visuals)
            .insert_resource(palette)
            .insert_resource(document)
            .insert_resource(MediaFailurePolicy::new(automated))
            .init_resource::<History>()
            .insert_resource(EditorToolState::new(map_w, map_h))
            .init_resource::<FileBrowserState>()
            .init_resource::<NewMapModalState>()
            .init_resource::<UnsavedChangesModalState>()
            .insert_resource(LosPreviewState::new(map_w, map_h))
            .init_resource::<PlaytestState>()
            .init_resource::<RenderEntityMap>()
            .init_resource::<SelectionState>()
            .init_resource::<BattleSelection>()
            .init_resource::<PresentationStatus>()
            .init_resource::<BattleSession>()
            .insert_resource(LabState {
                current_tab: LabTab::LiveBattle,
                ..default()
            })
            .insert_resource(FrameLimit {
                max_frames: self.max_frames,
                current_frame: 0,
                ..default()
            })
            .add_event::<CombatVisualEvent>()
            .add_event::<EditorCameraFocus>()
            .add_event::<LoadDocumentRequest>()
            .add_plugins(TerrainPlugin)
            .add_plugins(BubbleOpticsPlugin)
            .add_plugins(MaterialPlugin::<BubbleDomeMaterial>::default())
            .add_plugins(MaterialPlugin::<SelectionRingMaterial>::default())
            .add_plugins(MaterialPlugin::<CombatFxMaterial>::default());

        // A failed playtest stays on its HUD, because exiting here would skip the unsaved prompt.
        if !automated {
            app.init_resource::<RetainBattleFailure>();
        }

        // 2. Verification Mode Synchronization
        let pipeline_sync = PipelineSync::default();
        if let Some(ref verify_dir) = self.verify_render {
            app.insert_resource(pipeline_sync.clone());
            app.insert_resource(VerifyRenderState::new(verify_dir.clone(), playtest_focus));
        }

        // 3. Register Startup Systems
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

        // 4. Register Global & Editing Systems
        app.add_systems(
            Update,
            (
                frame_limit_system,
                editor_window_close_system,
                apply_document_load_system.after(editor_ui_system),
                editor_camera_system,
                update_marker_billboards_system,
                sync_marker_entities_system,
                editor_los_system.run_if(in_state(EditorPhase::Editing)),
            ),
        );

        if self.verify_render.is_some() {
            app.add_systems(
                Update,
                (
                    keep_window_surface_in_use,
                    update_render_verification_system.run_if(in_state(EditorPhase::Editing)),
                    update_playtest_verification_system.run_if(in_state(EditorPhase::Playtesting)),
                ),
            );
        } else {
            app.add_systems(
                Update,
                editor_ui_system.run_if(in_state(EditorPhase::Editing)),
            );
        }

        // 5. Register Playtesting Systems, in the game client's stage order
        app.add_systems(OnExit(EditorPhase::Playtesting), teardown_playtest_system);

        app.configure_sets(
            Update,
            (
                BattleSet::CollectIntent,
                BattleSet::AdvanceTicks,
                BattleSet::PreparePresentation,
                BattleSet::Present,
            )
                .chain()
                .run_if(in_state(EditorPhase::Playtesting)),
        );

        app.add_systems(
            Update,
            (
                (
                    playtest_exit_system,
                    playtest_input_system,
                    playtest_hud_system,
                )
                    .in_set(BattleSet::CollectIntent),
                advance_battle_ticks.in_set(BattleSet::AdvanceTicks),
                (sync_live_battle_system, apply_deferred)
                    .chain()
                    .in_set(BattleSet::PreparePresentation),
                (
                    sync_playtest_status_system,
                    publish_combat_events,
                    process_combat_events_system,
                    update_indicators_system,
                )
                    .chain()
                    .in_set(BattleSet::Present),
                (
                    update_projectiles_system,
                    update_beams_system,
                    update_melee_slashes_system,
                    update_shatter_system,
                )
                    .after(process_combat_events_system)
                    .in_set(BattleSet::Present),
            ),
        );

        // 6. Hook RenderApp for PipelineCache error checking in verify mode
        if self.verify_render.is_some() {
            if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
                render_app.insert_resource(pipeline_sync);
                render_app.add_systems(Render, check_render_pipelines);
            }
        }
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
fn editor_los_system(
    los_state: ResMut<'_, LosPreviewState>,
    doc: Res<'_, EditorDocument>,
    terrain: Res<'_, TerrainState>,
    terrain_params: ResMut<'_, crate::terrain::TerrainParamsState>,
    field_images: Option<Res<'_, crate::terrain::TerrainFieldImages>>,
    images: ResMut<'_, Assets<Image>>,
    catalog: Option<Res<'_, CatalogResource>>,
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
///
/// [`EditorStartup`] validated the theme, so a failure here means an asset vanished or is
/// corrupt; the application logs it and exits with an error instead of opening an empty scene.
#[expect(
    clippy::cast_precision_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn setup_editor_scene(
    mut spawner: EditorTerrainSpawner<'_, '_>,
    mut commands: Commands<'_, '_>,
    doc: Res<'_, EditorDocument>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if let Err(error) = spawner.spawn(&doc.doc, None) {
        error!("Cannot load terrain: {error}");
        exit.send(AppExit::error());
        return;
    }

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
