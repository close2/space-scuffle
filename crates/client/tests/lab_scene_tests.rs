//! Visual Lab and Swarm scene behaviour through the production synchronizers, media and catalog.
//!
//! # Invariants Verified
//! - The default Visual Lab shows the actors its controls act on (C-8).
//! - Combat VFX triggers place their effects inside the lab camera's view (C-9).
//! - Every swarm bubble stays on the stage floor across its bounce range, and the whole swarm
//!   stays in view (C-10).
//! - Smog never draws over a bubble, at any depth, on the real GPU (C-7).

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

use bevy::prelude::*;
use bevy::render::camera::PerspectiveProjection;
use client::lab_ui::LabVfxTrigger;
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use client::scene_controller::{SWARM_MAX_X, SWARM_MIN_X};
use sim_types::snapshot::CombatEventVisual;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// World `(min, max)` extent of the square stage floor on both horizontal axes.
const STAGE_FLOOR: (f32, f32) = (-16.0, 48.0);

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

/// C-15: a client that starts in battle must still fill the lineup when the Lab opens later.
#[test]
fn lab_opened_after_starting_in_battle_spawns_the_lineup() -> TestResult {
    let mut app = scene_app(client::LabState {
        current_tab: client::LabTab::LiveBattle,
        ..default()
    })?;
    app.add_systems(Update, client::sync_lab_preview_system);
    app.update();
    app.update();

    app.world_mut()
        .resource_mut::<client::LabState>()
        .current_tab = client::LabTab::VisualLab;
    app.update();
    app.update();

    let mut heroes = app
        .world_mut()
        .query_filtered::<(), With<client::LabPreviewBubble>>();
    assert!(
        heroes.iter(app.world()).count() > 0,
        "the lineup must spawn"
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

/// C-10: the swarm must stay where the camera and the stage floor can show it.
#[test]
fn swarm_stays_on_the_stage_floor_and_in_view() -> TestResult {
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

    // Bubbles only travel along X, so each one sweeps its row across the bounce range.
    for (position, radius) in &bubbles {
        for x in [SWARM_MIN_X, position.x, SWARM_MAX_X] {
            let swept = Vec2::new(x, position.z);
            assert!(
                swept.min_element() - radius >= STAGE_FLOOR.0
                    && swept.max_element() + radius <= STAGE_FLOOR.1,
                "row z={} leaves the stage floor at x={x}",
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

/// Median luminance of the rectangle `x_range` by `y_range` of a screenshot.
fn median_luminance(
    image: &image::GrayImage,
    x_range: std::ops::Range<u32>,
    y_range: std::ops::Range<u32>,
) -> u8 {
    let mut samples: Vec<u8> = y_range
        .flat_map(|y| x_range.clone().map(move |x| (x, y)))
        .map(|(x, y)| image.get_pixel(x, y).0[0])
        .collect();
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// C-7: the volume had one sort key, its box centre. Bubbles beyond that depth were drawn first and
/// all the smog on the ray landed on them, so far rows washed out and near rows did not.
///
/// The swarm is uniform art, so with correct compositing the far rows cannot be brighter than the
/// near rows. Measured on the real renderer, medians were 96 far and 77 near before the fix, and 77
/// far and 82 near after it. The bands are the rows' screen positions at the fixed 1280x720 window.
#[test]
fn smog_does_not_wash_out_bubbles_beyond_the_volume_centre() -> TestResult {
    const ROWS_X: std::ops::Range<u32> = 480..1170;
    const FAR_ROWS_Y: std::ops::Range<u32> = 205..390;
    const NEAR_ROWS_Y: std::ops::Range<u32> = 400..545;
    /// Headroom over the near rows, well below the 19 levels of the defect.
    const TOLERANCE: u8 = 8;

    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let screenshot = directory.path().join("swarm.png");
    let mut command =
        gpu_run::virtual_gpu_command(std::path::Path::new(env!("CARGO_BIN_EXE_client")));
    command.args(["--swarm", "--max-frames", "40", "--screenshot-out"]);
    command.arg(&screenshot);
    gpu_run::assert_passes_validation(&mut command, "client --swarm --max-frames 40");

    let image = image::open(&screenshot)?.to_luma8();
    let far = median_luminance(&image, ROWS_X, FAR_ROWS_Y);
    let near = median_luminance(&image, ROWS_X, NEAR_ROWS_Y);
    assert!(
        far <= near.saturating_add(TOLERANCE),
        "smog washes out far rows: median luminance {far} far, {near} near"
    );
    Ok(())
}

#[test]
fn game_camera_owns_the_depth_prepass_smog_clips_against() {
    let mut app = App::new();
    app.insert_resource(client::LabState::default())
        .add_systems(Startup, client::spawn_camera_system);
    app.update();
    let mut cameras = app.world_mut().query_filtered::<Entity, (
        With<Camera3d>,
        With<bevy::core_pipeline::prepass::DepthPrepass>,
    )>();
    assert_eq!(cameras.iter(app.world()).count(), 1);
}

/// C-15: the lineup is sorted once, so frames with an unchanged catalog neither sort nor allocate.
#[test]
fn showcase_lineup_rebuilds_only_when_the_catalog_changes() -> TestResult {
    let root = client::ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let catalog = &definition.catalogs.units;

    let mut lineup = client::scene_controller::ShowcaseLineup::default();
    lineup.refresh(catalog, false);
    assert!(lineup.is_empty(), "an unchanged catalog must not rebuild");

    lineup.refresh(catalog, true);
    assert!(!lineup.is_empty());
    let orders: Vec<_> = lineup
        .archetypes(catalog)
        .map(|archetype| archetype.showcase_order.unwrap_or(u8::MAX))
        .collect();
    assert!(orders.is_sorted(), "lineup must follow showcase_order");

    let storage = lineup.ids().as_ptr();
    lineup.refresh(catalog, false);
    lineup.refresh(catalog, true);
    assert_eq!(
        lineup.ids().as_ptr(),
        storage,
        "a rebuild reuses its storage"
    );
    Ok(())
}
