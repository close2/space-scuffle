//! Visual Lab and Swarm scene behaviour through the production synchronizers, media and catalog.
//!
//! # Invariants Verified
//! - The default Visual Lab shows the actors its controls act on (C-8).
//! - Combat VFX triggers place their effects inside the lab camera's view (C-9).
//! - No swarm bubble overlaps the arena plateau or ramp, at spawn or across its bounce range, and
//!   the whole swarm stays in view (C-10).

use bevy::prelude::*;
use bevy::render::camera::PerspectiveProjection;
use client::arena_render::{HIGH_GROUND_FOOTPRINT, RAMP_FOOTPRINT};
use client::lab_ui::LabVfxTrigger;
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use client::scene_controller::{SWARM_MAX_X, SWARM_MIN_X};
use sim_types::snapshot::CombatEventVisual;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Pixels the inspector panel leaves for the world at the default 1280x720 window.
const WORLD_VIEWPORT_ASPECT: f32 = 920.0 / 720.0;

fn scene_app(lab: client::LabState) -> TestResult<App> {
    let root = client::ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_event::<AppExit>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<client::BubbleDomeMaterial>>()
        .init_resource::<Assets<client::SelectionRingMaterial>>()
        .init_resource::<client::selection::SelectionState>()
        .init_resource::<client::player_presentation::PresentationStatus>()
        .insert_resource(definition.palette.clone())
        .insert_resource(root)
        .insert_resource(client::CatalogResource(definition.catalogs.units.clone()))
        .insert_resource(lab)
        .add_systems(
            Startup,
            (
                client::setup_bubble_assets_system,
                client::setup_indicator_assets_system,
            ),
        );
    Ok(app)
}

/// Normalized device coordinates of `point` as seen by `camera` with the default lens.
fn to_ndc(camera: &client::RtsCamera, point: Vec3) -> Vec3 {
    let lens = PerspectiveProjection::default();
    let projection = Mat4::perspective_rh(lens.fov, WORLD_VIEWPORT_ASPECT, lens.near, 1000.0);
    let view = camera.compute_transform().compute_matrix().inverse();
    projection.project_point3(view.transform_point3(point))
}

fn in_view(camera: &client::RtsCamera, point: Vec3) -> bool {
    let ndc = to_ndc(camera, point);
    ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0
}

/// Bevy world position of a snapshot `[x, y, altitude]` coordinate.
fn world(position: [f32; 3]) -> Vec3 {
    Vec3::new(position[0], position[2], position[1])
}

/// World positions an event starts from and aims at.
fn event_points(event: &CombatEventVisual) -> Vec<Vec3> {
    match event {
        CombatEventVisual::HitscanTracer { from, to, .. }
        | CombatEventVisual::BeamVisual { from, to, .. } => vec![world(*from), world(*to)],
        CombatEventVisual::ProjectileVisual {
            pos,
            vel,
            duration_secs,
            ..
        } => {
            let start = world(*pos);
            let flight = duration_secs.unwrap_or(0.0);
            vec![start, start + world(*vel) * flight]
        }
        CombatEventVisual::MeleeSlashVisual { pos, .. }
        | CombatEventVisual::BubbleShatter { pos, .. } => vec![world(*pos)],
    }
}

/// C-8: every default-state control acts on the showcase actors, so they must be on screen.
#[test]
fn default_visual_lab_shows_the_actors_its_controls_act_on() -> TestResult {
    let mut app = scene_app(client::LabState::default())?;
    app.add_systems(Update, client::sync_lab_preview_system);
    app.update();
    app.update();

    let mut heroes = app
        .world_mut()
        .query_filtered::<&Visibility, With<client::LabPreviewBubble>>();
    let visibilities: Vec<Visibility> = heroes.iter(app.world()).copied().collect();
    assert!(!visibilities.is_empty(), "the lineup must spawn");
    assert!(
        visibilities.iter().all(|v| *v != Visibility::Hidden),
        "the default lab hides its subject: {visibilities:?}"
    );
    Ok(())
}

/// C-9: effects at `[16, 16]` landed about 22 world units from the lab camera's focus.
#[test]
fn every_lab_vfx_trigger_stays_inside_the_lab_camera_view() {
    let camera = client::RtsCamera::hero_framing();
    for trigger in LabVfxTrigger::ALL {
        let event = trigger.event(0.0, sim_types::PlayerId(1));
        for point in event_points(&event) {
            assert!(
                in_view(&camera, point),
                "{trigger:?} reaches {point}, NDC {}",
                to_ndc(&camera, point)
            );
        }
    }
}

/// Whether a circle at `center` with `radius` overlaps a horizontal footprint.
fn overlaps(center: Vec2, radius: f32, (min, max): (Vec2, Vec2)) -> bool {
    center.distance(center.clamp(min, max)) < radius
}

/// C-10: 50 of 200 bubbles spawned inside the plateau, hidden by its geometry.
#[test]
fn swarm_never_overlaps_the_arena_plateau_or_ramp() -> TestResult {
    let mut app = scene_app(client::LabState {
        current_tab: client::LabTab::SwarmStressTest,
        swarm_active: true,
        ..default()
    })?;
    app.world_mut().spawn(client::RtsCamera::default());
    app.add_systems(Update, client::sync_swarm_stress_system);
    app.update();
    app.update();

    let mut swarm = app
        .world_mut()
        .query_filtered::<(&Transform, &client::BubbleInstance), With<client::SwarmBubble>>();
    let bubbles: Vec<(Vec3, f32)> = swarm
        .iter(app.world())
        .map(|(transform, instance)| (transform.translation, instance.0.radius))
        .collect();
    assert_eq!(bubbles.len(), 200);

    let inside_at_spawn = bubbles
        .iter()
        .filter(|(position, radius)| {
            overlaps(position.xz(), *radius, HIGH_GROUND_FOOTPRINT)
                || overlaps(position.xz(), *radius, RAMP_FOOTPRINT)
        })
        .count();
    assert_eq!(inside_at_spawn, 0, "bubbles spawned inside arena geometry");

    // Bubbles only travel along X, so each one sweeps its row across the bounce range.
    for (position, radius) in &bubbles {
        for x in [SWARM_MIN_X, position.x, SWARM_MAX_X] {
            let swept = Vec2::new(x, position.z);
            assert!(
                !overlaps(swept, *radius, HIGH_GROUND_FOOTPRINT)
                    && !overlaps(swept, *radius, RAMP_FOOTPRINT),
                "row z={} reaches arena geometry at x={x}",
                position.z
            );
        }
    }

    let camera = {
        let mut cameras = app.world_mut().query::<&client::RtsCamera>();
        cameras.single(app.world()).clone()
    };
    for (position, _) in &bubbles {
        for x in [SWARM_MIN_X, SWARM_MAX_X] {
            let swept = Vec3::new(x, position.y, position.z);
            assert!(
                in_view(&camera, swept),
                "swarm reaches {swept} outside the view, NDC {}",
                to_ndc(&camera, swept)
            );
        }
    }
    Ok(())
}
