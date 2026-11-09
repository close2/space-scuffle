//! Visual Lab and Swarm scene behaviour through the production synchronizers, media and catalog.
//!
//! # Invariants Verified
//! - The default Visual Lab shows the actors its controls act on (C-8).
//! - Combat VFX triggers place their effects inside the lab camera's view (C-9).
//! - Every swarm bubble stays on the stage floor across its bounce range, and the whole swarm
//!   stays in view (C-10).
//! - Smog never draws over a bubble, at any depth, on the real GPU (C-7).
//! - The Construction Progress slider drives the fill of the selected dome or sphere alone (5.8).
//! - The selected actor plays the selected state's slice at its own period on the shared flipbook
//!   clock, from its first frame, without its material changing; the unit portrait shows the same
//!   frame from that slice only (plan `2026-11-04-battle-flipbook-animation`, D-9).

#![allow(
    clippy::cast_precision_loss,
    reason = "atlas layer counts are far below 2^24, exact in f32"
)]

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
use sim_types::enums::{AnimState, ShapeKind};
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
        .init_resource::<client::battle_selection::BattleSelection>()
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
        | CombatEventVisual::BubbleShatter { pos, .. }
        | CombatEventVisual::SplashVisual { center: pos, .. } => vec![world(*pos)],
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

/// 5.8: the shader takes its height from `shape_kind`, so the slider must keep working on both shapes.
#[test]
fn construction_slider_reaches_the_selected_dome_or_sphere() -> TestResult {
    const PROGRESS: f32 = 0.3;
    let probe = scene_app(client::LabState::default())?;
    let catalog = &probe.world().resource::<client::CatalogResource>().0;
    let mut lineup = client::scene_controller::ShowcaseLineup::default();
    lineup.refresh(catalog, true);
    // The Lab truthfully refuses an actor whose media has no Constructing slice (plan
    // 2026-11-04-phase-8-5-veo-media.md, D-6: no fallback ranges), so only actors whose shipped
    // manifest holds one are staged; at least one dome and one sphere must remain.
    let textures = client::resolve_assets_dir().join("textures");
    let shapes: Vec<(usize, bool)> = lineup
        .archetypes(catalog)
        .enumerate()
        .filter(|(_, archetype)| {
            asset_baker::FlipbookAtlasMeta::load_from_ron(
                &textures.join(format!("{}.flipbook.ron", archetype.slug)),
            )
            .is_ok_and(|meta| meta.actions.contains_key(&AnimState::Constructing))
        })
        .map(|(index, archetype)| {
            (
                index,
                matches!(archetype.shape, ShapeKind::BuildingDome { .. }),
            )
        })
        .collect();
    assert!(
        shapes.iter().any(|(_, dome)| *dome) && shapes.iter().any(|(_, dome)| !*dome),
        "a dome and a sphere with Constructing media are needed: {shapes:?}"
    );

    for (selected, is_dome) in shapes {
        let mut app = scene_app(client::LabState {
            anim_state: AnimState::Constructing,
            build_progress: PROGRESS,
            selected_hero_index: selected,
            ..default()
        })?;
        app.add_systems(Update, client::sync_lab_preview_system);
        app.update();
        app.update();

        let mut heroes = app.world_mut().query::<(
            &client::HeroLineupIndex,
            &MeshMaterial3d<client::BubbleDomeMaterial>,
        )>();
        let materials = app.world().resource::<Assets<client::BubbleDomeMaterial>>();
        let mut seen = 0;
        for (index, material) in heroes.iter(app.world()) {
            let uniform = materials.get(&material.0).ok_or("material lost")?.uniform;
            seen += 1;
            if index.0 == selected {
                assert_eq!(uniform.shape_kind, u32::from(is_dome));
                assert_eq!(uniform.anim_state, AnimState::Constructing as u32);
                assert_eq!(uniform.build_progress, PROGRESS);
            } else {
                assert_ne!(uniform.anim_state, AnimState::Constructing as u32);
            }
        }
        assert!(seen > 1, "the lineup must spawn for slot {selected}");
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
    /// Headroom over the near rows, well below the 19 levels of the defect. On 2026-11-03 the gate
    /// measured far 83 against near 86 on radv, and 75 against 80 on lavapipe.
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
    command.assert_passes_validation("client --swarm --max-frames 40");

    let image = image::open(&screenshot)?.to_luma8();
    let far = median_luminance(&image, ROWS_X, FAR_ROWS_Y);
    let near = median_luminance(&image, ROWS_X, NEAR_ROWS_Y);
    eprintln!("median luminance {far} far, {near} near");
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

// --- One flipbook clock for the Lab (plan 2026-11-04-battle-flipbook-animation, WP6, D-9) -------

/// Lab frames last the 40 ms of one simulation tick, so the Lab clock advances 1024 units a frame.
const LAB_FRAME: std::time::Duration = std::time::Duration::from_millis(40);

/// Clock units of one [`LAB_FRAME`].
const LAB_FRAME_UNITS: u32 = 1024;

/// The Lab preview app on a manual clock, with actor `selected` in `anim_state`.
fn timed_lab_app(selected: usize, anim_state: AnimState) -> TestResult<App> {
    let mut app = scene_app(client::LabState {
        selected_hero_index: selected,
        anim_state,
        ..default()
    })?;
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(LAB_FRAME))
        .add_systems(Update, client::sync_lab_preview_system);
    Ok(app)
}

/// The first lineup slot whose shipped atlas has a looping `action` slice of more than one frame,
/// and that atlas.
fn lineup_slot_with(action: AnimState) -> TestResult<(usize, asset_baker::FlipbookAtlasMeta)> {
    let root = client::ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let catalog = &definition.catalogs.units;
    let mut lineup = client::scene_controller::ShowcaseLineup::default();
    lineup.refresh(catalog, true);
    let textures = client::resolve_assets_dir().join("textures");
    let found = lineup
        .archetypes(catalog)
        .take(4)
        .enumerate()
        .find_map(|(index, archetype)| {
            asset_baker::FlipbookAtlasMeta::load_from_ron(
                &textures.join(format!("{}.flipbook.ron", archetype.slug)),
            )
            .ok()
            .filter(|meta| {
                meta.actions
                    .get(&action)
                    .is_some_and(|slice| slice.looping && slice.frame_count > 1)
            })
            .map(|meta| (index, meta))
        });
    found.ok_or_else(|| format!("no lineup actor has a looping {action:?} slice").into())
}

/// The selected preview actor's material uniform, and the Lab clock of the current frame.
fn selected_preview(app: &mut App, selected: usize) -> TestResult<(client::BubbleUniformGpu, u32)> {
    let now = client::flipbook_clock::elapsed_clock(app.world().resource::<Time>().elapsed());
    let mut heroes = app.world_mut().query::<(
        &client::HeroLineupIndex,
        &MeshMaterial3d<client::BubbleDomeMaterial>,
    )>();
    let handle = heroes
        .iter(app.world())
        .find(|(index, _)| index.0 == selected)
        .map(|(_, material)| material.0.clone())
        .ok_or("the selected actor must spawn")?;
    let uniform = app
        .world()
        .resource::<Assets<client::BubbleDomeMaterial>>()
        .get(&handle)
        .ok_or("material lost")?
        .uniform;
    Ok((uniform, now))
}

/// D-9 and D-6: the selected actor plays the selected state's slice at the clip's own speed on the
/// shared clock, with its material untouched while the state lasts, and a state change restarts
/// the slice at its first frame (D-5).
#[test]
fn lab_preview_plays_the_selected_slice_on_the_shared_clock() -> TestResult {
    let (selected, meta) = lineup_slot_with(AnimState::Walk)?;
    let slice = meta.actions[&AnimState::Walk];
    let mut app = timed_lab_app(selected, AnimState::Idle)?;
    for _ in 0..5 {
        app.update();
    }
    app.world_mut()
        .resource_mut::<client::LabState>()
        .anim_state = AnimState::Walk;
    app.update();

    let (first, now) = selected_preview(&mut app, selected)?;
    let lanes = first.flipbook_lanes();
    assert_eq!(
        client::flipbook_clock::flipbook_layer(now, &lanes),
        slice.start_frame,
        "the Walk slice starts at its first frame when the state is selected"
    );
    assert_eq!(
        lanes.period,
        client::flipbook_clock::slice_period(&meta, &slice),
        "the slice plays at its own period (D-6), not 0.67 s for every slice"
    );
    assert_eq!(lanes.count, slice.frame_count);

    // Two passes over the slice, one frame at a time.
    let frames = 2 * lanes.period.div_ceil(LAB_FRAME_UNITS) + 2;
    let mut shown = Vec::new();
    for _ in 0..frames {
        app.update();
        let (uniform, now) = selected_preview(&mut app, selected)?;
        assert_eq!(uniform, first, "time passing modified the preview material");
        shown.push(client::flipbook_clock::flipbook_layer(now, &lanes));
    }
    let expected: std::collections::BTreeSet<u32> =
        (slice.start_frame..slice.start_frame + slice.frame_count).collect();
    let seen: std::collections::BTreeSet<u32> = shown.iter().copied().collect();
    assert_eq!(
        seen, expected,
        "every layer of the slice, and only those: {shown:?}"
    );
    let steps_back = shown.windows(2).filter(|pair| pair[1] < pair[0]).count();
    assert!(
        steps_back <= 2,
        "the slice plays in order and wraps once per pass: {shown:?}"
    );
    Ok(())
}

/// D-9: the inspector portrait shows the frame the selected bubble shows, from the selected
/// state's slice only, as rows of the portrait strip.
#[test]
fn lab_portrait_shows_the_selected_bubbles_frame() -> TestResult {
    let (selected, meta) = lineup_slot_with(AnimState::Idle)?;
    let slice = meta.actions[&AnimState::Idle];
    let mut app = timed_lab_app(selected, AnimState::Idle)?;
    app.update();
    let mut seen = std::collections::BTreeSet::new();
    let frames = 2 * client::flipbook_clock::slice_period(&meta, &slice).div_ceil(LAB_FRAME_UNITS);
    for _ in 0..frames {
        app.update();
        let (uniform, now) = selected_preview(&mut app, selected)?;
        let lanes = uniform.flipbook_lanes();
        let portrait = client::lab_ui::portrait_frame(now, &lanes, &meta);
        assert_eq!(
            portrait.layer,
            client::flipbook_clock::flipbook_layer(now, &lanes),
            "the portrait and the bubble show one frame"
        );
        assert!(
            (slice.start_frame..slice.start_frame + slice.frame_count).contains(&portrait.layer),
            "layer {} is outside the Idle slice",
            portrait.layer
        );
        let total = meta.total_frames as f32;
        assert_eq!(
            portrait.rows,
            [
                portrait.layer as f32 / total,
                (portrait.layer + 1) as f32 / total
            ]
        );
        seen.insert(portrait.layer);
    }
    assert_eq!(
        seen.len(),
        slice.frame_count as usize,
        "the portrait plays the whole slice: {seen:?}"
    );
    Ok(())
}
