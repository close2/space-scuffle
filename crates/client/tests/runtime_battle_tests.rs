//! Verifies real versioned battle startup, deterministic reset and map-relative
//! combat orders through the production loader, with explicit failure diagnostics.

use client::runtime_battle::{
    BattleDefinition, BattleInitialOrder, BattleLoadError, DEFAULT_BATTLE_SCENARIO,
};
use sim_map::MapFile;
use sim_types::{PlayerId, Tick, TickBundle, UnitOrder};
use std::path::PathBuf;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

#[test]
fn loaded_battle_has_real_terrain_combatants_and_repeatable_reset() -> TestResult {
    let definition = BattleDefinition::load(&assets(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut first = definition.instantiate()?;
    let mut second = definition.instantiate()?;
    assert_eq!(first.map.cells(), second.map.cells());
    assert!(first.map.cells().iter().any(|cell| cell.ramp.is_some()));
    assert_eq!(first.compute_checksum(), second.compute_checksum());
    for player in [PlayerId(0), PlayerId(1)] {
        let combatants = first
            .entities
            .values()
            .filter(|entity| {
                entity.owner == player
                    && matches!(
                        entity.order_queue.front(),
                        Some(UnitOrder::AttackMove { .. })
                    )
            })
            .count();
        assert!(
            combatants >= 2,
            "default game must field actual combat units"
        );
        let targets = definition.start_approaches(&first)?;
        assert!(targets.contains_key(&player));
    }
    for tick in 0..16 {
        let bundle = TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        };
        first.step(&bundle);
        second.step(&bundle);
        assert_eq!(first.last_checksum(), second.last_checksum());
    }
    let reset = definition.instantiate()?;
    assert_eq!(reset.current_tick, 0);
    assert_ne!(reset.compute_checksum(), first.compute_checksum());
    Ok(())
}

#[test]
fn default_loaded_battle_reaches_actual_combat() -> TestResult {
    let definition = BattleDefinition::load(&assets(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut world = definition.instantiate()?;
    let mut combat_seen = false;
    for tick in 0..2400 {
        let snapshot = world.step(&TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
        if !snapshot.combat_events.is_empty() {
            combat_seen = true;
            break;
        }
    }
    assert!(
        combat_seen,
        "loaded game armies must actually navigate into combat"
    );
    Ok(())
}

#[test]
fn both_real_versioned_maps_support_the_same_declarative_battle() -> TestResult {
    let mut definition = BattleDefinition::load(&assets(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    for map in ["default_arena.scmap.ron", "showcase_canyon.scmap.ron"] {
        definition.document = MapFile::load(&assets().join("maps").join(map))?.decode()?;
        let world = definition.instantiate()?;
        assert_eq!(world.map.width(), definition.document.width);
        assert_eq!(world.map.height(), definition.document.height);
        assert!(world
            .entities
            .values()
            .any(|entity| entity.owner == PlayerId(1)));
    }
    Ok(())
}

#[test]
fn invalid_orders_and_worker_only_battles_fail_explicitly() -> TestResult {
    let mut definition = BattleDefinition::load(&assets(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    definition.scenario.initial_orders = vec![BattleInitialOrder::AttackMoveToStart {
        player_slot: 0,
        target_player_slot: 7,
    }];
    assert!(matches!(
        definition.instantiate(),
        Err(BattleLoadError::Start(7))
    ));
    definition.scenario.initial_orders = vec![BattleInitialOrder::AttackMoveToStart {
        player_slot: 0,
        target_player_slot: 1,
    }];
    definition.scenario.players[0].loadout_slug = "standard_automaton".into();
    assert!(matches!(
        definition.instantiate(),
        Err(BattleLoadError::NoCombatants(0))
    ));
    Ok(())
}

#[test]
fn missing_selected_scenario_does_not_fall_back() {
    let absent = assets().join("scenarios/absent-battle.ron");
    assert!(
        matches!(BattleDefinition::load(&assets(), &absent), Err(BattleLoadError::Io { path, .. }) if path == absent)
    );
}

#[test]
fn malformed_versions_and_duplicate_players_are_rejected() -> TestResult {
    let dir = tempfile::tempdir()?;
    let scenario = dir.path().join("scenario.ron");
    std::fs::write(&scenario, "(")?;
    assert!(matches!(
        BattleDefinition::load(&assets(), &scenario),
        Err(BattleLoadError::Ron { .. })
    ));
    std::fs::write(&scenario, "(format_version:9,map:\"maps/default_arena.scmap.ron\",seed:1,players:[],initial_orders:[])")?;
    assert!(matches!(
        BattleDefinition::load(&assets(), &scenario),
        Err(BattleLoadError::Version(9))
    ));
    std::fs::write(&scenario, "(format_version:1,map:\"maps/default_arena.scmap.ron\",seed:1,players:[(player_slot:0,loadout_slug:\"a\"),(player_slot:0,loadout_slug:\"b\")],initial_orders:[])")?;
    assert!(matches!(
        BattleDefinition::load(&assets(), &scenario),
        Err(BattleLoadError::PlayerSlot(0))
    ));
    Ok(())
}

fn runtime_scene_app() -> Result<bevy::prelude::App, Box<dyn std::error::Error>> {
    use bevy::prelude::*;
    use client::runtime_battle::{manage_battle_scene, update_map_presentation};
    use client::terrain::TerrainMaterial;
    let root = client::ResolvedAssetsRoot::from_directory(&assets())?;
    let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: root.bevy_file_path()?.into(),
            ..default()
        },
        ImagePlugin::default(),
        TransformPlugin,
        client::runtime_battle::SceneEnvironmentPlugin,
    ))
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .init_asset::<TerrainMaterial>()
    .insert_resource(definition.palette.clone())
    .init_resource::<client::player_presentation::PresentationStatus>()
    .add_event::<AppExit>()
    .insert_resource(root)
    .insert_resource(definition)
    .insert_resource(client::LabState {
        current_tab: client::LabTab::LiveBattle,
        battle_paused: true,
        ..default()
    })
    .init_resource::<client::battle_pipeline::session::BattleSession>()
    .init_resource::<client::RenderEntityMap>()
    .init_resource::<client::SelectionState>()
    .add_systems(
        Update,
        (
            client::runtime_battle::sync_scene_camera,
            manage_battle_scene,
        )
            .chain(),
    )
    .add_systems(PostUpdate, update_map_presentation);
    app.world_mut().spawn((
        Camera3d::default(),
        Transform::default(),
        client::RtsCamera::default(),
    ));
    Ok(app)
}

fn camera_pose(app: &mut bevy::prelude::App) -> client::RtsCamera {
    app.world_mut()
        .query::<&client::RtsCamera>()
        .single(app.world())
        .clone()
}

#[test]
fn runtime_scene_reset_and_tab_visibility_preserve_the_loaded_map() -> TestResult {
    use bevy::prelude::*;
    use client::runtime_battle::BattleSceneRoot;
    use client::terrain::{TerrainChunks, TerrainMaterial, TerrainMaterialHandle, TerrainState};
    let mut app = runtime_scene_app()?;
    app.update();
    let scene_count = app
        .world_mut()
        .query_filtered::<Entity, With<BattleSceneRoot>>()
        .iter(app.world())
        .count();
    assert_eq!(scene_count, 1);
    let original_chunks = app.world().resource::<TerrainChunks>().chunks.len();
    let lab = app.world().resource::<client::LabState>();
    let world = lab
        .sim_world
        .as_ref()
        .ok_or("battle scene did not create a simulation")?;
    assert_eq!(
        world.map.cells(),
        app.world().resource::<TerrainState>().map.cells()
    );
    let initial_checksum = world.compute_checksum();
    let material_handle = &app.world().resource::<TerrainMaterialHandle>().0;
    let material = app
        .world()
        .resource::<Assets<TerrainMaterial>>()
        .get(material_handle)
        .ok_or("terrain material missing")?;
    assert_eq!(material.extension.params.overlay_flags, 0);
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_reset_requested = true;
    app.update();
    let scene_count = app
        .world_mut()
        .query_filtered::<Entity, With<BattleSceneRoot>>()
        .iter(app.world())
        .count();
    assert_eq!(
        scene_count, 1,
        "reset must replace rather than accumulate scenes"
    );
    assert_eq!(
        app.world().resource::<TerrainChunks>().chunks.len(),
        original_chunks
    );
    assert_eq!(
        app.world()
            .resource::<client::LabState>()
            .sim_world
            .as_ref()
            .ok_or("reset lost world")?
            .compute_checksum(),
        initial_checksum
    );
    let battle_camera = camera_pose(&mut app);
    app.world_mut()
        .resource_mut::<client::LabState>()
        .current_tab = client::LabTab::VisualLab;
    app.update();
    let visibility = app
        .world_mut()
        .query_filtered::<&Visibility, With<BattleSceneRoot>>()
        .single(app.world());
    assert_eq!(*visibility, Visibility::Hidden);
    let lab_camera = camera_pose(&mut app);
    assert_eq!(lab_camera.target, client::RtsCamera::hero_framing().target);
    assert_eq!(
        lab_camera.distance,
        client::RtsCamera::hero_framing().distance
    );
    app.world_mut()
        .resource_mut::<client::LabState>()
        .current_tab = client::LabTab::LiveBattle;
    app.update();
    let visibility = app
        .world_mut()
        .query_filtered::<&Visibility, With<BattleSceneRoot>>()
        .single(app.world());
    assert_eq!(*visibility, Visibility::Visible);
    let restored = camera_pose(&mut app);
    assert_eq!(restored.target, battle_camera.target);
    assert_eq!(restored.distance, battle_camera.distance);
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<DirectionalLight>>()
            .iter(app.world())
            .count(),
        1
    );
    Ok(())
}

#[test]
fn structured_terrain_sampling_preserves_uniform_layout() -> TestResult {
    use bevy::render::render_resource::ShaderType;
    use client::terrain::TerrainParams;
    let definition = BattleDefinition::load(&assets(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let params = TerrainParams::from_theme(&definition.theme, 64, 64);
    assert_eq!(TerrainParams::min_size().get(), 64);
    assert_eq!(size_of::<TerrainParams>(), 64);
    assert_eq!(
        params.regular_tiling_mask,
        1 << 1,
        "authored deck plating retains coherent seams"
    );
    let mut theme = definition.theme.clone();
    theme.layers.metal_plate.tiling = asset_baker::TilingMode::MinCutBlend;
    assert_eq!(
        TerrainParams::from_theme(&theme, 64, 64).regular_tiling_mask,
        0
    );
    Ok(())
}

#[test]
fn production_plugin_schedule_initializes_terrain_and_restores_tab_camera() -> TestResult {
    use bevy::prelude::*;
    use bevy::winit::WinitPlugin;
    let root = client::ResolvedAssetsRoot::from_directory(&assets())?;
    let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut app = App::new();
    app.insert_resource(client::CatalogResource(definition.catalogs.units.clone()))
        .insert_resource(definition.palette.clone())
        .insert_resource(definition)
        .insert_resource(root.clone())
        .add_plugins(
            client::presentation::GamePresentationPlugins
                .set(AssetPlugin {
                    file_path: root.bevy_file_path()?.into(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        visible: false,
                        ..default()
                    }),
                    ..default()
                })
                .disable::<WinitPlugin>(),
        )
        .add_plugins(bevy_egui::EguiPlugin)
        .add_plugins(client::ClientAppPlugin {
            mode: client::ClientMode::LiveBattle,
            max_frames: None,
        });
    app.finish();
    app.cleanup();
    assert!(!app.is_plugin_added::<bevy::audio::AudioPlugin>());
    assert_eq!(
        *app.world().resource::<client::presentation::AudioPolicy>(),
        client::presentation::AudioPolicy::DisabledNoPlaybackSubsystem
    );
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_paused = true;
    let mut exit_cursor = app.world().resource::<Events<AppExit>>().get_cursor();
    app.update();
    assert!(
        exit_cursor
            .read(app.world().resource::<Events<AppExit>>())
            .next()
            .is_none(),
        "the production startup must not emit an application failure"
    );
    app.update();
    assert!(app
        .world()
        .contains_resource::<client::terrain::TerrainState>());
    assert!(app
        .world()
        .resource::<client::LabState>()
        .sim_world
        .is_some());
    assert_active_scene_lights(&mut app, true);
    let battle_camera = camera_pose(&mut app);
    app.world_mut()
        .resource_mut::<client::LabState>()
        .current_tab = client::LabTab::VisualLab;
    app.update();
    assert_active_scene_lights(&mut app, false);
    let lab_camera = camera_pose(&mut app);
    assert_eq!(lab_camera.target, client::RtsCamera::hero_framing().target);
    app.world_mut()
        .resource_mut::<client::LabState>()
        .current_tab = client::LabTab::LiveBattle;
    app.update();
    assert_active_scene_lights(&mut app, true);
    let restored = camera_pose(&mut app);
    assert_eq!(restored.target, battle_camera.target);
    assert_eq!(restored.distance, battle_camera.distance);
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<bevy::window::PrimaryWindow>>()
        .get_single(app.world())?;
    app.world_mut().despawn(window);
    app.world_mut()
        .send_event(bevy::input::mouse::MouseButtonInput {
            button: MouseButton::Left,
            state: bevy::input::ButtonState::Pressed,
            window,
        });
    app.update();
    assert_eq!(app.should_exit(), Some(AppExit::Success));
    client::render_compatibility::finish_rendering(&mut app)?;
    Ok(())
}

#[test]
fn committed_environment_survives_failed_candidate_and_resets_in_same_tab() -> TestResult {
    use bevy::prelude::*;
    use client::runtime_battle::BattleSceneRoot;
    let mut app = runtime_scene_app()?;
    let arena_clear = Color::srgb(0.11, 0.22, 0.33);
    app.insert_resource(ClearColor(arena_clear));
    app.insert_resource(AmbientLight {
        color: Color::srgb(0.4, 0.3, 0.2),
        brightness: 123.0,
    });
    app.update();
    let old_root = app
        .world_mut()
        .query_filtered::<Entity, With<BattleSceneRoot>>()
        .single(app.world());
    let committed_clear = app.world().resource::<ClearColor>().0;
    let committed_ambient = app.world().resource::<AmbientLight>().clone();
    let baked_dir = {
        let mut candidate = app.world_mut().resource_mut::<BattleDefinition>();
        candidate.theme.lighting.void_color_srgb = (0.6, 0.4, 0.2);
        candidate.theme.lighting.ambient_brightness = 789.0;
        std::mem::replace(
            &mut candidate.theme.baked_dir,
            "textures/terrain/missing-environment-test-bundle".into(),
        )
    };
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_reset_requested = true;
    app.update();
    assert!(app
        .world()
        .resource::<client::LabState>()
        .battle_failure
        .is_some());
    assert_eq!(app.world().resource::<ClearColor>().0, committed_clear);
    assert_eq!(
        app.world().resource::<AmbientLight>().brightness,
        committed_ambient.brightness
    );
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<BattleSceneRoot>>()
            .single(app.world()),
        old_root
    );
    app.world_mut()
        .resource_mut::<BattleDefinition>()
        .theme
        .baked_dir = baked_dir;
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_reset_requested = true;
    app.update();
    assert!(app
        .world()
        .resource::<client::LabState>()
        .battle_failure
        .is_none());
    assert_ne!(
        app.world_mut()
            .query_filtered::<Entity, With<BattleSceneRoot>>()
            .single(app.world()),
        old_root
    );
    assert_eq!(
        app.world().resource::<ClearColor>().0,
        Color::srgb(0.6, 0.4, 0.2)
    );
    assert_eq!(app.world().resource::<AmbientLight>().brightness, 789.0);
    app.world_mut()
        .resource_mut::<client::LabState>()
        .current_tab = client::LabTab::VisualLab;
    app.update();
    assert_eq!(app.world().resource::<ClearColor>().0, arena_clear);
    assert_eq!(app.world().resource::<AmbientLight>().brightness, 123.0);
    Ok(())
}

fn assert_active_scene_lights(app: &mut bevy::prelude::App, battle: bool) {
    use bevy::prelude::*;
    let mut lights = app.world_mut().query::<(
        &DirectionalLight,
        &InheritedVisibility,
        &ViewVisibility,
        Option<&client::arena_render::ArenaMapMarker>,
    )>();
    let mut visible = 0;
    for (_, inherited, extracted, arena) in lights.iter(app.world()) {
        let expected = arena.is_none() == battle;
        assert_eq!(
            inherited.get(),
            expected,
            "scene hierarchy must propagate before extraction"
        );
        assert_eq!(
            extracted.get(),
            expected,
            "actual renderer light visibility must match active scene"
        );
        visible += usize::from(extracted.get());
    }
    assert_eq!(visible, 1, "exactly the active scene light rig is visible");
}

#[test]
fn failed_candidate_retains_pipeline_generation_commands_and_published_metadata() -> TestResult {
    use bevy::prelude::*;
    use client::battle_pipeline::session::BattleSession;
    let mut app = runtime_scene_app()?;
    app.update();
    app.world_mut()
        .resource_scope(|world, mut session: Mut<'_, BattleSession>| -> TestResult {
            let simulation = world
                .resource::<client::LabState>()
                .sim_world
                .as_ref()
                .ok_or("world absent")?;
            let actor = simulation
                .entities
                .values()
                .find(|actor| !actor.is_immobile)
                .ok_or("mobile actor absent")?;
            session.commands.submit(
                simulation,
                sim_types::PlayerCommand {
                    player_id: actor.owner,
                    entities: vec![actor.id],
                    order: UnitOrder::HoldPosition,
                    queue: false,
                },
            )?;
            session.frames.present(1.0)?;
            Ok(())
        })?;
    let session = app.world().resource::<BattleSession>();
    let generation = session.commands.generation();
    let snapshot = session.frames.presented().clone();
    let title = session.metadata.map_title.clone();
    let checksum = app
        .world()
        .resource::<client::LabState>()
        .sim_world
        .as_ref()
        .ok_or("world absent")?
        .compute_checksum();
    let original_dir = {
        let mut candidate = app.world_mut().resource_mut::<BattleDefinition>();
        candidate.document.metadata.title = "Candidate never committed".into();
        std::mem::replace(
            &mut candidate.theme.baked_dir,
            "textures/terrain/missing-pipeline-test-bundle".into(),
        )
    };
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_reset_requested = true;
    app.update();
    let session = app.world().resource::<BattleSession>();
    assert_eq!(session.commands.generation(), generation);
    assert_eq!(session.commands.pending_len(), 1);
    assert_eq!(session.frames.presented(), &snapshot);
    assert_eq!(session.metadata.map_title, title);
    assert_eq!(
        app.world()
            .resource::<client::LabState>()
            .sim_world
            .as_ref()
            .ok_or("world absent")?
            .compute_checksum(),
        checksum
    );
    app.world_mut()
        .resource_mut::<BattleDefinition>()
        .theme
        .baked_dir = original_dir;
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_reset_requested = true;
    app.update();
    let session = app.world().resource::<BattleSession>();
    assert_ne!(session.commands.generation(), generation);
    assert_eq!(session.commands.pending_len(), 0);
    assert_eq!(session.metadata.map_title, "Candidate never committed");
    Ok(())
}

fn changed_palette(
) -> Result<client::player_presentation::PlayerPalette, Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    std::fs::create_dir(temporary.path().join("catalog"))?;
    let mut source = std::fs::read_to_string(assets().join("catalog/map_entity_visuals.ron"))?;
    let palette_start = source.find("player_colors_srgb:").ok_or("palette schema")?;
    let start = palette_start + source[palette_start..].find('(').ok_or("color start")?;
    let end = start + source[start..].find(')').ok_or("color end")? + 1;
    source.replace_range(start..end, "(0.1, 0.2, 0.3)");
    std::fs::write(
        temporary.path().join("catalog/map_entity_visuals.ron"),
        source,
    )?;
    let definition = BattleDefinition::load(&assets(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let count = definition
        .catalogs
        .units
        .archetypes()
        .filter(|arch| arch.showcase_hero)
        .count();
    Ok(client::player_presentation::PlayerPalette::load(
        temporary.path(),
        count,
    )?)
}

#[test]
fn palette_and_labels_commit_together_and_failed_candidate_keeps_old_colors() -> TestResult {
    use client::battle_pipeline::session::BattleSession;
    use client::player_presentation::PlayerPalette;
    let mut app = runtime_scene_app()?;
    app.update();
    let original = app
        .world()
        .resource::<PlayerPalette>()
        .resolve(PlayerId(0))?
        .ui;
    let replacement = changed_palette()?;
    let changed = replacement.resolve(PlayerId(0))?.ui;
    assert_ne!(original, changed);
    let old_path = {
        let mut candidate = app.world_mut().resource_mut::<BattleDefinition>();
        candidate.palette = replacement;
        std::mem::replace(
            &mut candidate.theme.baked_dir,
            "textures/terrain/missing-palette-candidate".into(),
        )
    };
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_reset_requested = true;
    app.update();
    assert_eq!(
        app.world()
            .resource::<PlayerPalette>()
            .resolve(PlayerId(0))?
            .ui,
        original
    );
    assert_eq!(
        app.world().resource::<BattleSession>().metadata.players[0].color,
        original
    );
    app.world_mut()
        .resource_mut::<BattleDefinition>()
        .theme
        .baked_dir = old_path;
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_reset_requested = true;
    app.update();
    assert_eq!(
        app.world()
            .resource::<PlayerPalette>()
            .resolve(PlayerId(0))?
            .ui,
        changed
    );
    assert_eq!(
        app.world().resource::<BattleSession>().metadata.players[0].color,
        changed
    );
    Ok(())
}

#[test]
fn real_watchtower_material_uses_neutral_and_committed_player_palette() -> TestResult {
    use bevy::prelude::*;
    use client::player_presentation::PlayerPalette;
    use client::runtime_battle::{MapObject, MapPresentation};
    let mut app = runtime_scene_app()?;
    app.update();
    for owner in [None, Some(PlayerId(0)), Some(PlayerId(1))] {
        let position = {
            let mut lab = app.world_mut().resource_mut::<client::LabState>();
            let simulation = lab.sim_world.as_mut().ok_or("world")?;
            let tower = simulation
                .map
                .watchtowers_mut()
                .first_mut()
                .ok_or("real watchtower")?;
            tower.controlling_player = owner;
            app.world()
                .resource::<MapPresentation>()
                .objects
                .iter()
                .find(|record| record.ownership_tint)
                .ok_or("tower record")?
                .position
        };
        app.world_mut().resource_mut::<MapPresentation>().tick = None;
        app.update();
        let expected = app
            .world()
            .resource::<PlayerPalette>()
            .resolve(owner.unwrap_or(PlayerId::NEUTRAL))?
            .color();
        let mut query = app
            .world_mut()
            .query_filtered::<(&Transform, &MeshMaterial3d<StandardMaterial>), With<MapObject>>();
        let (_, handle) = query
            .iter(app.world())
            .find(|(transform, _)| transform.translation == position)
            .ok_or("actual tower mesh")?;
        let material = app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .get(&handle.0)
            .ok_or("actual tower material")?;
        assert_eq!(material.base_color.to_linear(), expected.to_linear());
    }
    Ok(())
}
