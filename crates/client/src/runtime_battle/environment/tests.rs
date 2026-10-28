//! Scheduled environment lifecycle tests, independent of GPU appearance checks.
//! Distinct values prevent coincidentally matching authored ambient settings from
//! concealing a resource leak; real visibility propagation verifies owned lights.

use super::*;
use bevy::render::view::VisibilityPlugin;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn baseline() -> EnvironmentValues {
    EnvironmentValues {
        ambient: AmbientLight {
            color: Color::srgb(0.2, 0.4, 0.6),
            brightness: 123.0,
        },
        clear: ClearColor(Color::srgb(0.13, 0.23, 0.33)),
    }
}

fn battle() -> EnvironmentValues {
    EnvironmentValues {
        ambient: AmbientLight {
            color: Color::srgb(0.8, 0.5, 0.2),
            brightness: 456.0,
        },
        clear: ClearColor(Color::srgb(0.02, 0.04, 0.06)),
    }
}

fn fixture() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        VisibilityPlugin,
        SceneEnvironmentPlugin,
    ))
    .init_resource::<Assets<Mesh>>()
    .add_event::<AppExit>()
    .insert_resource(LabState::default())
    .add_systems(Startup, |mut commands: Commands<'_, '_>| {
        let values = baseline();
        commands.insert_resource(values.ambient);
        commands.insert_resource(values.clear);
    });
    app
}

fn assert_environment(app: &App, expected: &EnvironmentValues) {
    assert_eq!(app.world().resource::<ClearColor>().0, expected.clear.0);
    let ambient = app.world().resource::<AmbientLight>();
    assert_eq!(ambient.color, expected.ambient.color);
    assert_eq!(ambient.brightness, expected.ambient.brightness);
}

fn committed(app: &mut App, values: EnvironmentValues) -> (Entity, Entity) {
    let root = app
        .world_mut()
        .spawn((
            BattleSceneRoot,
            CommittedBattleEnvironment(values),
            Transform::default(),
            Visibility::Inherited,
        ))
        .id();
    let light = app
        .world_mut()
        .spawn((DirectionalLight::default(), Transform::default()))
        .id();
    app.world_mut().entity_mut(root).add_child(light);
    (root, light)
}

#[test]
fn startup_baseline_restores_through_tabs_and_real_light_visibility() -> TestResult {
    let mut app = fixture();
    let arena = app
        .world_mut()
        .spawn((
            ArenaMapMarker,
            DirectionalLight::default(),
            Transform::default(),
        ))
        .id();
    app.update();
    assert_environment(&app, &baseline());
    let (_, light) = committed(&mut app, battle());
    for tab in [
        LabTab::LiveBattle,
        LabTab::VisualLab,
        LabTab::SwarmStressTest,
        LabTab::LiveBattle,
        LabTab::VisualLab,
    ] {
        app.world_mut().resource_mut::<LabState>().current_tab = tab;
        app.update();
        let is_battle = tab == LabTab::LiveBattle;
        assert_environment(&app, &if is_battle { battle() } else { baseline() });
        assert_eq!(
            app.world()
                .get::<InheritedVisibility>(light)
                .ok_or("battle light visibility missing")?
                .get(),
            is_battle
        );
        assert_eq!(
            app.world()
                .get::<InheritedVisibility>(arena)
                .ok_or("arena light visibility missing")?
                .get(),
            !is_battle
        );
    }
    Ok(())
}

#[test]
fn pending_root_preserves_arena_and_replacement_uses_committed_values_only() {
    let mut app = fixture();
    app.world_mut().resource_mut::<LabState>().current_tab = LabTab::LiveBattle;
    app.update();
    assert_environment(&app, &baseline());
    assert!(app.should_exit().is_none());
    let (old, _) = committed(&mut app, battle());
    app.update();
    assert_environment(&app, &battle());
    let mut replacement = battle();
    replacement.ambient.brightness = 789.0;
    replacement.clear = ClearColor(Color::srgb(0.3, 0.2, 0.1));
    app.world_mut().entity_mut(old).despawn_recursive();
    committed(&mut app, replacement.clone());
    app.update();
    assert_environment(&app, &replacement);
    app.world_mut().resource_mut::<LabState>().current_tab = LabTab::VisualLab;
    app.update();
    assert_environment(&app, &baseline());
}

#[test]
fn steady_frames_do_not_mark_global_resources_changed() {
    let mut app = fixture();
    app.update();
    let before_clear = app
        .world()
        .get_resource_ref::<ClearColor>()
        .map(|value| value.last_changed());
    let before_ambient = app
        .world()
        .get_resource_ref::<AmbientLight>()
        .map(|value| value.last_changed());
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(
        app.world()
            .get_resource_ref::<ClearColor>()
            .map(|value| value.last_changed()),
        before_clear
    );
    assert_eq!(
        app.world()
            .get_resource_ref::<AmbientLight>()
            .map(|value| value.last_changed()),
        before_ambient
    );
}

#[test]
fn duplicate_committed_roots_fail_without_replacing_globals() {
    let mut app = fixture();
    app.update();
    app.world_mut().resource_mut::<LabState>().current_tab = LabTab::LiveBattle;
    committed(&mut app, battle());
    committed(&mut app, battle());
    app.update();
    assert_environment(&app, &baseline());
    assert_eq!(app.should_exit(), Some(AppExit::error()));
    assert!(app.world().resource::<LabState>().battle_paused);
}

#[test]
fn lost_startup_baseline_is_an_explicit_lifecycle_failure() {
    let mut app = fixture();
    app.update();
    app.world_mut()
        .resource_mut::<SceneEnvironmentState>()
        .baseline = None;
    app.update();
    assert_environment(&app, &baseline());
    assert_eq!(app.should_exit(), Some(AppExit::error()));
    let lab = app.world().resource::<LabState>();
    assert!(lab.battle_paused);
    assert_eq!(
        lab.battle_failure.as_deref(),
        Some("Game scene environment was not captured after Startup")
    );
}

#[test]
fn owned_lighting_preserves_globals_and_standalone_lighting_installs_theme() -> TestResult {
    use asset_baker::TerrainThemeCatalog;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/catalog/terrain_themes.ron");
    let theme = TerrainThemeCatalog::load(&path)?
        .themes
        .into_iter()
        .next()
        .ok_or("no real terrain theme")?;
    for owned in [true, false] {
        let mut app = App::new();
        let initial = baseline();
        app.add_plugins(MinimalPlugins)
            .insert_resource(initial.ambient.clone())
            .insert_resource(initial.clear.clone());
        let actual = theme.clone();
        app.add_systems(Startup, move |mut commands: Commands<'_, '_>| {
            let root = owned.then(|| {
                commands
                    .spawn((Transform::default(), Visibility::Inherited))
                    .id()
            });
            crate::terrain::theme_gpu::setup_theme_lighting_owned(&mut commands, &actual, root);
        });
        app.update();
        if owned {
            assert_environment(&app, &initial);
        } else {
            let (ambient, clear) = crate::terrain::theme_gpu::theme_environment(&theme);
            assert_environment(&app, &EnvironmentValues { ambient, clear });
        }
        let mut lights = app
            .world_mut()
            .query_filtered::<Option<&Parent>, With<DirectionalLight>>();
        let parent = lights.get_single(app.world())?;
        assert_eq!(parent.is_some(), owned);
    }
    Ok(())
}
