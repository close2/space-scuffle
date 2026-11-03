//! Frame verification for one settled capture checkpoint.
//!
//! The capture driver calls [`verify_frame`] once the camera has settled on a checkpoint.
//! It checks, in a fixed order, the logical battle state, the camera pose, the scene root,
//! asset readiness, lights, the rendered roster, visible bubbles and their media, shader
//! readiness, and the optional ownership and transport proofs, then builds the record that
//! the screenshot readback completes.
//!
//! Invariants: a failed check returns a typed [`CaptureError`]; a check whose evidence is not
//! ready yet returns `Ok(None)` so the driver retries on a later frame, and never accepts the
//! frame. The checks only read production state, apart from the session's media validation
//! cache and ownership history.

use super::super::{
    artifacts::Record,
    config::{CaptureError, Goal},
    pipelines::PipelineEvidence,
};
use super::{camera, failure, Session};
use bevy::prelude::*;
use client::runtime_battle::{BattleSceneRoot, MapPresentation};
use client::{BubbleInstance, LabPreviewBubble, LabState, LabTab, LiveBattleBubble, RtsCamera};
use std::collections::BTreeSet;

/// Logical battle facts at the verified frame.
struct LogicalFrame {
    /// Authoritative simulation tick.
    tick: u32,
    /// Simulation checksum at `tick`.
    checksum: u64,
    /// Living actors that are not loaded into a transport.
    entities: usize,
}

/// Result of the visible-bubble pass, in physical window pixels.
struct VisibleBubbles {
    /// Bubbles of the checkpoint's scene inside the gameplay region.
    visible: usize,
    /// Every projected bubble center, with its battle entity ID if it has one.
    projected: Vec<(Option<u32>, Vec2)>,
    /// Center of the checkpoint subject, if visible in the gameplay region.
    subject_pixel: Option<[f32; 2]>,
    /// Center of the combat source, if visible in the gameplay region.
    source_pixel: Option<[f32; 2]>,
    /// Owners of the bubbles inside the gameplay region.
    owners: BTreeSet<u8>,
}

/// Everything measured about an accepted frame that its record stores.
struct FrameFacts {
    goal: Goal,
    logical: LogicalFrame,
    camera: RtsCamera,
    transform: Transform,
    lights: usize,
    effects: usize,
    roots: usize,
    bubbles: VisibleBubbles,
    required_mask: u8,
    counts: [usize; 3],
    transport: Option<super::super::transport::TransportProof>,
    ownership: Option<super::super::ownership::OwnershipProof>,
}

/// Verifies the settled frame of the current checkpoint and returns its record, `Ok(None)`
/// while evidence is still loading, or the first failed check.
pub(super) fn verify_frame(
    world: &mut World,
    session: &mut Session,
) -> Result<Option<Record>, CaptureError> {
    let goal = session.goal();
    let lab_goal = goal.is_preview();
    let logical = verify_logical_state(world, session, goal, lab_goal)?;
    let (camera_state, transform) = verify_camera(world, session, goal)?;
    let roots = verify_scene_root(world, lab_goal)?;
    if !production_assets_loaded(world) {
        return Ok(None);
    }
    let lights = verify_lights(world, session, goal)?;
    verify_roster(world, session, goal, lab_goal, logical.entities)?;
    let effects = verify_initial_effects(world, goal)?;
    let bubbles = verify_visible_bubbles(world, session, goal, lab_goal)?;
    verify_subjects(session, goal, &bubbles)?;
    let required_mask = required_shader_mask(goal, lab_goal);
    let evidence = world.resource::<PipelineEvidence>();
    if !evidence.ready(required_mask) {
        return Ok(None);
    }
    let counts = evidence.counts();
    if goal == Goal::Combat {
        verify_combat_effect_visible(world)?;
    }
    let ownership = if let Some(ownership) = &mut session.ownership {
        let Some(proof) = ownership.verify(
            world,
            goal.ownership_view(),
            session.script.gameplay_region,
            session.script.resolution,
        )?
        else {
            return Ok(None);
        };
        if goal == Goal::Combat {
            ownership.remember_combat();
        }
        Some(proof)
    } else {
        None
    };
    let transport = if let Some(transport) = &mut session.transport {
        Some(transport.verify(
            world,
            goal,
            &bubbles.projected,
            session.script.gameplay_region,
            session.script.resolution,
            (
                camera_state.target.to_array(),
                camera_state.distance,
                transform.translation.to_array(),
            ),
        )?)
    } else {
        None
    };
    let facts = FrameFacts {
        goal,
        logical,
        camera: camera_state,
        transform,
        lights,
        effects,
        roots,
        bubbles,
        required_mask,
        counts,
        transport,
        ownership,
    };
    record(world, session, facts).map(Some)
}

/// Initial and reset frames must show the loaded scenario unchanged, tab round trips must
/// keep the paused checksum, the presented pose must be the current tick, the production tab
/// must match the goal, and the rendered terrain must equal the simulation map.
fn verify_logical_state(
    world: &World,
    session: &Session,
    goal: Goal,
    lab_goal: bool,
) -> Result<LogicalFrame, CaptureError> {
    let pipeline = world.resource::<client::battle_pipeline::session::BattleSession>();
    let simulation = pipeline
        .world()
        .ok_or_else(|| failure("battle missing during capture"))?;
    let tick = simulation.current_tick;
    let checksum = simulation.compute_checksum();
    let entities = simulation
        .entities
        .values()
        .filter(|actor| actor.is_alive() && !actor.is_loaded_in_transport())
        .count();
    if matches!(goal, Goal::Overview | Goal::Reset)
        && (tick != 0
            || Some(checksum) != session.initial_checksum
            || simulation.entities != session.initial_entities)
    {
        return Err(failure(
            "initial/reset logical state differs from the loaded scenario",
        ));
    }
    if matches!(goal, Goal::Lab | Goal::Return) && Some(checksum) != session.saved_checksum {
        return Err(failure("tab transition changed paused battle state"));
    }
    if pipeline.frames.presented().tick.0 != tick {
        return Err(failure(
            "paused capture pose does not identify the current authoritative tick",
        ));
    }
    let expected_tab = if goal == Goal::Swarm {
        LabTab::SwarmStressTest
    } else if lab_goal {
        LabTab::VisualLab
    } else {
        LabTab::LiveBattle
    };
    if world.resource::<LabState>().current_tab != expected_tab {
        return Err(failure("production tab transition did not occur"));
    }
    let map = world.resource::<client::terrain::TerrainState>();
    if map.map.cells() != simulation.map.cells() {
        return Err(failure("render terrain differs from simulation map"));
    }
    Ok(LogicalFrame {
        tick,
        checksum,
        entities,
    })
}

/// The camera must sit at the goal's expected pose (hero framing for Lab, the saved battle
/// pose for Return, otherwise the checkpoint focus and distance in world units) within
/// 0.001, and its transform must have propagated. Returns the camera and its transform.
fn verify_camera(
    world: &mut World,
    session: &Session,
    goal: Goal,
) -> Result<(RtsCamera, Transform), CaptureError> {
    let (camera_state, transform) = camera(world)?;
    let expected = if goal == Goal::Lab {
        RtsCamera::hero_framing()
    } else if goal == Goal::Return {
        session
            .saved_camera
            .clone()
            .ok_or_else(|| failure("no saved battle camera"))?
    } else {
        let mut expected = camera_state.clone();
        expected.target = session
            .focus
            .ok_or_else(|| failure("capture focus missing"))?;
        expected.distance = session.script.checkpoints[session.index].distance;
        expected
    };
    if camera_state.target.distance(expected.target) > 0.001
        || (camera_state.distance - expected.distance).abs() > 0.001
    {
        return Err(failure(format!(
            "camera action had no expected effect: {:?} at {}, expected {:?} at {}",
            camera_state.target, camera_state.distance, expected.target, expected.distance
        )));
    }
    if transform
        .translation
        .distance(camera_state.compute_transform().translation)
        > 0.001
    {
        return Err(failure("camera transform did not propagate"));
    }
    Ok((camera_state, transform))
}

/// Exactly one battle scene root exists, hidden exactly when a preview goal is active.
/// Returns the number of roots.
fn verify_scene_root(world: &mut World, lab_goal: bool) -> Result<usize, CaptureError> {
    let roots = world
        .query_filtered::<&Visibility, With<BattleSceneRoot>>()
        .iter(world)
        .copied()
        .collect::<Vec<_>>();
    if roots.len() != 1 || (roots[0] == Visibility::Hidden) != lab_goal {
        return Err(failure(
            "battle root visibility/count is wrong after lifecycle action",
        ));
    }
    Ok(roots.len())
}

/// Whether the map sprites, every terrain material texture and a painted egui frame are all
/// loaded. A false result is not a failure: the frame is retried later.
fn production_assets_loaded(world: &mut World) -> bool {
    let images = world.resource::<Assets<Image>>();
    let presentation = world.resource::<MapPresentation>();
    if !presentation
        .sprite_images
        .iter()
        .all(|image| images.contains(image))
    {
        return false;
    }
    let terrain_handle = world.resource::<client::terrain::TerrainMaterialHandle>();
    let materials = world.resource::<Assets<client::terrain::TerrainMaterial>>();
    let Some(terrain) = materials.get(&terrain_handle.0) else {
        return false;
    };
    if [
        &terrain.extension.albedo,
        &terrain.extension.normal,
        &terrain.extension.orm,
        &terrain.extension.splat,
        &terrain.extension.overlay,
    ]
    .iter()
    .any(|image| !images.contains(*image))
    {
        return false;
    }
    world
        .query::<&bevy_egui::EguiRenderOutput>()
        .iter(world)
        .any(|output| !output.paint_jobs.is_empty())
}

/// Counts the scene's directional, point and spot lights; a reset must keep the count of the
/// first recorded checkpoint.
fn verify_lights(world: &mut World, session: &Session, goal: Goal) -> Result<usize, CaptureError> {
    let light_count = world
        .query_filtered::<Entity, Or<(With<DirectionalLight>, With<PointLight>, With<SpotLight>)>>()
        .iter(world)
        .count();
    if goal == Goal::Reset
        && session
            .records
            .first()
            .is_some_and(|initial| initial.lights != light_count)
    {
        return Err(failure(
            "reset changed the number of production scene lights",
        ));
    }
    Ok(light_count)
}

/// The live battle bubbles are unique and match the simulation's `entities` (none while a
/// preview goal is active), and an initial or reset frame shows exactly the scenario's IDs.
fn verify_roster(
    world: &mut World,
    session: &Session,
    goal: Goal,
    lab_goal: bool,
    entities: usize,
) -> Result<(), CaptureError> {
    let live_ids = world
        .query::<&LiveBattleBubble>()
        .iter(world)
        .map(|live| live.entity_id)
        .collect::<Vec<_>>();
    let live_set = live_ids.iter().copied().collect::<BTreeSet<_>>();
    if live_ids.len() != live_set.len() || live_ids.len() != if lab_goal { 0 } else { entities } {
        return Err(failure(
            "tab/reset left duplicate, missing, or stale battle bubbles",
        ));
    }
    if matches!(goal, Goal::Overview | Goal::Reset)
        && live_set.iter().ne(session.initial_entities.keys())
    {
        return Err(failure(
            "reset render roster differs from initial scenario IDs",
        ));
    }
    Ok(())
}

/// Counts combat effect entities; an initial or reset frame must have none.
fn verify_initial_effects(world: &mut World, goal: Goal) -> Result<usize, CaptureError> {
    let effects = world
        .query_filtered::<Entity, With<MeshMaterial3d<client::vfx::CombatFxMaterial>>>()
        .iter(world)
        .count();
    if matches!(goal, Goal::Overview | Goal::Reset) && effects != 0 {
        return Err(failure("initial/reset scene contains stale combat effects"));
    }
    Ok(effects)
}

/// Validates the real media of every visible bubble of the goal's scene (Swarm, Visual Lab
/// or live battle) and projects its center into physical window pixels, counting those inside
/// the normalized gameplay region.
#[expect(
    clippy::cast_precision_loss,
    reason = "capture coordinates are bounded by the window size and frame budget"
)]
fn verify_visible_bubbles(
    world: &mut World,
    session: &mut Session,
    goal: Goal,
    lab_goal: bool,
) -> Result<VisibleBubbles, CaptureError> {
    let mut bubbles = VisibleBubbles {
        visible: 0,
        projected: Vec::new(),
        subject_pixel: None,
        source_pixel: None,
        owners: BTreeSet::new(),
    };
    let mut bubble_query = world.query::<(
        &BubbleInstance,
        &MeshMaterial3d<client::BubbleDomeMaterial>,
        &GlobalTransform,
        &ViewVisibility,
        Option<&LiveBattleBubble>,
        Option<&LabPreviewBubble>,
        Option<&client::SwarmBubble>,
    )>();
    let mut camera_query = world.query_filtered::<(&Camera, &GlobalTransform), With<Camera3d>>();
    let (view, view_transform) = camera_query
        .get_single(world)
        .map_err(|e| failure(e.to_string()))?;
    for (bubble, material_handle, global, visibility, battle, lab_bubble, swarm) in
        bubble_query.iter(world)
    {
        let expected_instance = if goal == Goal::Swarm {
            swarm.is_some()
        } else if lab_goal {
            lab_bubble.is_some()
        } else {
            battle.is_some()
        };
        if !visibility.get() || !expected_instance {
            continue;
        }
        validate_bubble_media(world, session, bubble, material_handle)?;
        let Some(window_point) =
            client::input_policy::world_to_window(view, view_transform, global.translation())
        else {
            continue;
        };
        let coordinates = client::input_policy::ViewportCoordinates::from_camera(view)
            .ok_or_else(|| failure("capture camera viewport coordinates unavailable"))?;
        let pixel = coordinates.physical_from_window(window_point);
        bubbles
            .projected
            .push((battle.map(|b| b.entity_id.0), pixel));
        let [x, y, w, h] = session.script.gameplay_region;
        let [width, height] = session.script.resolution;
        if pixel.x >= x * width as f32
            && pixel.x <= (x + w) * width as f32
            && pixel.y >= y * height as f32
            && pixel.y <= (y + h) * height as f32
        {
            bubbles.visible += 1;
            bubbles.owners.insert(bubble.0.owner.0);
            if battle.is_some_and(|b| Some(b.entity_id) == session.subject) {
                bubbles.subject_pixel = Some(pixel.to_array());
            }
            if battle.is_some_and(|b| Some(b.entity_id) == session.combat_source) {
                bubbles.source_pixel = Some(pixel.to_array());
            }
        }
    }
    Ok(bubbles)
}

/// A visible bubble must show the real flipbook registered for its archetype, through its own
/// material, and that media must pass the session's on-disk validation. Fallbacks are rejected.
fn validate_bubble_media(
    world: &World,
    session: &mut Session,
    bubble: &BubbleInstance,
    material_handle: &MeshMaterial3d<client::BubbleDomeMaterial>,
) -> Result<(), CaptureError> {
    let registry = world.resource::<client::FlipbookRegistry>();
    let Some(texture) = registry.textures_by_id.get(&bubble.0.unit_type) else {
        return Err(failure(format!(
            "visible archetype {:?} has no real flipbook; fallback rejected",
            bubble.0.unit_type
        )));
    };
    if !world.resource::<Assets<Image>>().contains(texture)
        || !registry.manifests_by_id.contains_key(&bubble.0.unit_type)
    {
        return Err(failure("visible flipbook image/metadata missing"));
    }
    let archetype = world
        .resource::<client::CatalogResource>()
        .0
        .get(bubble.0.unit_type)
        .map_err(|error| failure(error.to_string()))?;
    let metadata = registry
        .manifests_by_id
        .get(&bubble.0.unit_type)
        .ok_or_else(|| failure("visible media metadata disappeared"))?;
    let image = world
        .resource::<Assets<Image>>()
        .get(texture)
        .ok_or_else(|| failure("visible media image disappeared"))?;
    let material = world
        .resource::<Assets<client::BubbleDomeMaterial>>()
        .get(&material_handle.0)
        .ok_or_else(|| failure("visible bubble material is missing"))?;
    session.media.validate(
        &session.root,
        super::super::media::MediaBinding {
            unit_type: bubble.0.unit_type,
            slug: &archetype.slug,
            registered_texture: texture,
            material_texture: material
                .video_texture
                .as_ref()
                .ok_or_else(|| failure("visible bubble material has no image binding"))?,
            metadata,
            image,
        },
    )
}

/// Something must be visible (except at the neutral watchtower), the tracked subject must be
/// visible, and a combat frame must show both sides and the combat source.
fn verify_subjects(
    session: &Session,
    goal: Goal,
    bubbles: &VisibleBubbles,
) -> Result<(), CaptureError> {
    let visible = bubbles.visible;
    let projected = &bubbles.projected;
    if (visible == 0 && goal != Goal::NeutralWatchtower)
        || session.subject.is_some() && bubbles.subject_pixel.is_none()
    {
        return Err(failure(format!(
            "intended subjects are not visible in gameplay image region; visible={visible}, subject={:?}, projected={projected:?}", session.subject
        )));
    }
    if goal == Goal::Combat && (bubbles.owners.len() < 2 || bubbles.source_pixel.is_none()) {
        return Err(failure("combat view does not contain opposing sides"));
    }
    Ok(())
}

/// The production shader pipelines the goal draws, as a mask over the shader order of
/// [`PipelineEvidence`]: bit 0 terrain, bit 1 bubble domes, bit 2 combat effects.
fn required_shader_mask(goal: Goal, lab_goal: bool) -> u8 {
    if matches!(
        goal.ownership_view(),
        super::super::ownership::View::Effect(_)
    ) {
        2 | 4
    } else if lab_goal {
        2
    } else if goal == Goal::Combat {
        1 | 2 | 4
    } else {
        1 | 2
    }
}

/// After damage, at least one produced combat effect must be visible.
fn verify_combat_effect_visible(world: &mut World) -> Result<(), CaptureError> {
    let effect_visible = world
        .query_filtered::<&ViewVisibility, With<MeshMaterial3d<client::vfx::CombatFxMaterial>>>()
        .iter(world)
        .any(|visibility| visibility.get());
    if !effect_visible {
        return Err(failure(
            "damage occurred but produced combat effect is not visible",
        ));
    }
    Ok(())
}

/// Builds the checkpoint record from the verified frame; the screenshot readback fills in
/// its pixel metrics later.
fn record(world: &World, session: &Session, facts: FrameFacts) -> Result<Record, CaptureError> {
    let pipeline = world.resource::<client::battle_pipeline::session::BattleSession>();
    let checkpoint = &session.script.checkpoints[session.index];
    Ok(Record {
        shell_audit: None,
        shell_fixture: if facts.goal == Goal::Swarm {
            session
                .script
                .shell_audit
                .as_ref()
                .and_then(|audit| audit.fixture.clone())
        } else {
            None
        },
        name: checkpoint.name.clone(),
        goal: facts.goal,
        tick: facts.logical.tick,
        presented_tick: pipeline.frames.presented().tick.0,
        session_generation: pipeline
            .commands
            .generation()
            .ok_or_else(|| failure("capture has no committed session"))?
            .value(),
        command_outcomes: pipeline.outcome_count,
        combat_tick: session.combat_tick,
        combat_generation: session.combat_generation,
        ramp_generation: session.ramp_generation,
        checksum: facts.logical.checksum,
        virtual_seconds: world.resource::<Time>().elapsed_secs_f64(),
        camera_target: facts.camera.target.to_array(),
        camera_distance: facts.camera.distance,
        camera_translation: facts.transform.translation.to_array(),
        subject: session.subject.map(|id| id.0),
        subject_pixel: facts.bubbles.subject_pixel,
        ramp_entry_level: session.entry_level,
        ramp_exit_level: session.exit_level,
        ramp_trace: session.ramp_trace.clone(),
        combat_source: session.combat_source.map(|id| id.0),
        entities: facts.logical.entities,
        lights: facts.lights,
        combat_effects: facts.effects,
        source_pixel: facts.bubbles.source_pixel,
        visible_units: facts.bubbles.visible,
        scene_roots: facts.roots,
        terrain_chunks: world
            .resource::<client::terrain::TerrainChunks>()
            .chunks
            .len(),
        required_shader_mask: facts.required_mask,
        ready_pipeline_counts: facts.counts,
        image: session.out.join(format!("{}.png", checkpoint.name)),
        pixels: None,
        transport: facts.transport,
        ownership: facts.ownership,
    })
}

#[cfg(test)]
mod tests {
    use super::super::super::config::CaptureScript;
    use super::*;

    /// Characterises the stage order of `verify_frame` at the overview checkpoint: the
    /// logical state is checked first, then the camera pose, then the scene root. Each stage
    /// is made to fail in turn on the real default battle.
    #[test]
    fn verify_frame_checks_logical_state_then_camera_then_scene_root(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let script_path = root.join("captures/default_battle.ron");
        let script = CaptureScript::load(&script_path)?;
        let definition = client::runtime_battle::BattleDefinition::load(
            &root,
            client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
        )?;
        let simulation = definition.instantiate()?;
        let output = tempfile::tempdir()?;
        let mut session = Session::new(
            script,
            definition.source,
            script_path,
            root,
            output.path().to_owned(),
            client::presentation::AudioPolicy::DisabledNoPlaybackSubsystem,
        );
        assert_eq!(session.goal(), Goal::Overview);
        let mut world = World::new();
        world.insert_resource(LabState {
            current_tab: LabTab::LiveBattle,
            ..LabState::default()
        });
        world.insert_resource(client::terrain::TerrainState::new(simulation.map.clone()));
        let initial_entities = simulation.entities.clone();
        let initial_checksum = simulation.compute_checksum();
        world.insert_resource(
            client::battle_pipeline::session::BattleSession::default().with_world(simulation),
        );

        let error = verify_frame(&mut world, &mut session)
            .err()
            .ok_or("an unrecorded scenario baseline must fail")?;
        assert!(
            error.to_string().contains("initial/reset logical state"),
            "{error}"
        );

        session.initial_entities = initial_entities;
        session.initial_checksum = Some(initial_checksum);
        let focus = Vec2::new(5.0, 6.0);
        session.focus = Some(focus);
        let camera = world
            .spawn((
                Camera3d::default(),
                RtsCamera::default(),
                Transform::default(),
            ))
            .id();
        let error = verify_frame(&mut world, &mut session)
            .err()
            .ok_or("a camera away from the focus must fail")?;
        assert!(
            error
                .to_string()
                .contains("camera action had no expected effect"),
            "{error}"
        );

        let pose = RtsCamera {
            target: focus,
            distance: session.script.checkpoints[session.index].distance,
            ..RtsCamera::default()
        };
        let transform = pose.compute_transform();
        world.entity_mut(camera).insert((pose, transform));
        let error = verify_frame(&mut world, &mut session)
            .err()
            .ok_or("a frame without a battle scene root must fail")?;
        assert!(
            error.to_string().contains("battle root visibility/count"),
            "{error}"
        );
        Ok(())
    }
}
