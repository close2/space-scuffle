//! Actual-renderer startup regression for queued but unpublished pipeline IDs.
//!
//! This tests cache lifecycle safety, not shader correctness or visual acceptance.
//! Native captures separately compile the actual scene pipelines and inspect images.

use bevy::prelude::*;
use bevy::render::{
    render_resource::{PipelineCache, RenderPipelineDescriptor, VertexState},
    RenderApp,
};

#[test]
fn actual_renderer_new_queue_id_is_pending_without_indexing_unpublished_storage(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = crate::ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let mut app = App::new();
    app.add_plugins(
        crate::presentation::GamePresentationPlugins
            .set(AssetPlugin {
                file_path: root.bevy_file_path()?.into(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: None,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>(),
    );
    app.finish();
    let shader = app
        .world()
        .resource::<AssetServer>()
        .load("shaders/bubble_dome.wgsl");
    let result: Result<(), Box<dyn std::error::Error>> = (|| {
        let render = app
            .get_sub_app_mut(RenderApp)
            .ok_or("real render app absent")?;
        let cache = render.world().resource::<PipelineCache>();
        let id = cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("queued-id lifecycle only; no draw submitted".into()),
            layout: vec![],
            push_constant_ranges: vec![],
            vertex: VertexState {
                shader,
                entry_point: "vertex".into(),
                shader_defs: vec![],
                buffers: vec![],
            },
            primitive: default(),
            depth_stencil: None,
            multisample: default(),
            fragment: None,
            zero_initialize_workgroup_memory: true,
        });
        if super::phase::published(cache, id).is_some() {
            return Err("new queued pipeline was unexpectedly published".into());
        }
        Ok(())
    })();
    // No frame processes this deliberately unsubmitted descriptor. Normal renderer
    // shutdown still joins any real startup work; no device or app is leaked.
    app.cleanup();
    let shutdown = crate::render_compatibility::finish_rendering(&mut app);
    result?;
    shutdown?;
    Ok(())
}

/// Real client startup and scene extraction; every exit joins renderer workers.
#[test]
fn actual_loaded_scene_validates_temporary_and_synchronized_proxy_lifecycles(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = crate::ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let definition = crate::runtime_battle::BattleDefinition::load(
        root.as_path(),
        crate::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?;
    let mut app = App::new();
    app.insert_resource(crate::CatalogResource(definition.catalogs.units.clone()))
        .insert_resource(definition.palette.clone())
        .insert_resource(definition)
        .insert_resource(root.clone())
        .add_plugins(
            crate::presentation::GamePresentationPlugins
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
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>(),
        )
        .add_plugins(bevy_egui::EguiPlugin)
        .add_plugins(crate::ClientAppPlugin {
            mode: crate::ClientMode::VisualLab,
            max_frames: None,
            screenshot_out: None,
        })
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::ZERO,
        ));
    let config = super::AuditConfig {
        max_views: 4,
        max_items: 4096,
        max_epochs: 16,
        max_bytes: 64 * 1024 * 1024,
        max_source_bytes: 64 * 1024 * 1024,
        fixture: Some(super::FixtureSpec {
            participants: [1000, 1100],
            positions: [[15.75, 1.5, 10.0], [16.25, 1.5, 10.0]],
            order: super::FixtureOrder::Forward,
        }),
    };
    super::install(&mut app, config)?;
    app.finish();
    app.cleanup();
    let result = verify_loaded_proxy_frames(&mut app);
    let shutdown = crate::render_compatibility::finish_rendering(&mut app);
    result?;
    shutdown?;
    Ok(())
}

fn verify_loaded_proxy_frames(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    use super::model::AssociationKind;
    use bevy::render::{
        camera::RenderTarget,
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        sync_world::{RenderEntity, SyncToRenderWorld},
    };
    {
        let mut lab = app.world_mut().resource_mut::<crate::LabState>();
        lab.current_tab = crate::LabTab::SwarmStressTest;
        lab.swarm_active = true;
    }
    app.update();
    let mut image = Image::new_fill(
        Extent3d {
            width: 1280,
            height: 720,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    for (mut camera, mut pose) in app
        .world_mut()
        .query::<(&mut Camera, &mut crate::RtsCamera)>()
        .iter_mut(app.world_mut())
    {
        camera.target = RenderTarget::Image(target.clone());
        pose.target = Vec2::new(16.0, 10.0);
        pose.distance = 10.0;
    }
    let synchronized = app
        .world_mut()
        .query::<(Entity, &crate::BubbleInstance)>()
        .iter(app.world())
        .find(|(_, instance)| instance.0.entity_id.0 == 1100)
        .map(|(entity, _)| entity)
        .ok_or("actual Swarm actor absent")?;
    app.world_mut()
        .entity_mut(synchronized)
        .insert(SyncToRenderWorld);
    let bridge = app.world().resource::<super::AuditBridge>().clone();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut first: Option<super::model::EpochEvidence> = None;
    while std::time::Instant::now() < deadline {
        app.update();
        bridge.failure()?;
        if app.should_exit().is_some() {
            return Err("actual client emitted AppExit".into());
        }
        let frame = bridge.lock()?.collector.latest_for_test().cloned();
        let Some(frame) = frame.filter(|frame| frame.ready) else {
            continue;
        };
        let entries: Vec<_> = frame
            .views
            .iter()
            .flat_map(|view| &view.sorted)
            .filter(|item| item.logical.is_some())
            .collect();
        if entries.len() != 2 {
            return Err("actual fixture did not queue both shells".into());
        }
        let temporary = entries
            .iter()
            .find(|item| item.logical.as_ref().is_some_and(|id| id.entity == 1000))
            .ok_or("temporary source missing")?;
        let persistent = entries
            .iter()
            .find(|item| item.logical.as_ref().is_some_and(|id| id.entity == 1100))
            .ok_or("synchronized source missing")?;
        if !matches!(temporary.association.kind, AssociationKind::Temporary(_))
            || !matches!(
                persistent.association.kind,
                AssociationKind::Synchronized(_)
            )
        {
            return Err(format!(
                "actual association mismatch: {:?} / {:?}",
                temporary.association, persistent.association
            )
            .into());
        }
        let mapping = app
            .world()
            .get::<RenderEntity>(synchronized)
            .ok_or("actual persistent main mapping absent")?;
        if mapping.id().to_bits() != persistent.render_entity {
            return Err("persistent mapping differs".into());
        }
        let render = app.get_sub_app(RenderApp).ok_or("render app")?;
        if render
            .world()
            .get_entity(Entity::from_bits(temporary.render_entity))
            .is_ok()
        {
            return Err("temporary proxy survived actual PostCleanup".into());
        }
        if render
            .world()
            .get_entity(Entity::from_bits(persistent.render_entity))
            .is_err()
        {
            return Err("persistent proxy unexpectedly removed".into());
        }
        if let Some(previous) = &first {
            if !previous.same_sources_and_order(&frame) {
                return Err(
                    "complete actual source/view/sorted payload changed between ready frames"
                        .into(),
                );
            }
            let old = previous
                .views
                .iter()
                .flat_map(|view| &view.sorted)
                .find(|item| item.main_entity == temporary.main_entity)
                .ok_or("previous source")?;
            if old.render_entity == temporary.render_entity {
                return Err("temporary full-generation identity did not advance".into());
            }
            if !old.same_draw(temporary) {
                return Err("actual temporary semantic payload changed".into());
            }
            let old_persistent = previous
                .views
                .iter()
                .flat_map(|view| &view.sorted)
                .find(|item| item.main_entity == persistent.main_entity)
                .ok_or("previous persistent")?;
            if !old_persistent.same_draw(persistent) {
                return Err("actual synchronized payload changed".into());
            }
            return Ok(());
        }
        first = Some(frame);
    }
    Err("actual loaded draw readiness exceeded 60-second deadline".into())
}
