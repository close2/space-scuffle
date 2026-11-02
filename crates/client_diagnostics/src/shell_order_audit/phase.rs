//! Passive observations immediately around Bevy's actual Transparent3d radix sort.
//!
//! Complete phase payloads retain unsupported items. Readiness inspects each real
//! queued pipeline and prepared shell material, never aggregate shader counts alone.

use super::{
    model::{
        AssociationKind, AuditConfig, AuditError, EpochEvidence, MaterialSource, PhaseEntry,
        ViewEvidence,
    },
    AuditBridge, Extracted,
};
use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::pbr::{PreparedMaterial, RenderMaterialInstances};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_phase::ViewSortedRenderPhases;
use bevy::render::render_resource::{
    CachedPipeline, CachedPipelineState, CachedRenderPipelineId, PipelineCache, PipelineCacheError,
};
use bevy::render::sync_world::MainEntity;
use bevy::render::view::{ExtractedView, RenderVisibleEntities};

/// Records the admitted actual phase before the engine sorter, optionally applying the declared fixture.
pub(super) fn queued(world: &mut World) {
    observe(world, false);
}
/// Completes the same extraction epoch after the actual engine sorter.
pub(super) fn sorted(world: &mut World) {
    observe(world, true);
}

/// A queued identifier can precede publication into Bevy's indexed pipeline array.
pub(super) fn published(
    cache: &PipelineCache,
    id: CachedRenderPipelineId,
) -> Option<&CachedPipeline> {
    cache.pipelines().nth(id.id())
}

fn observe(world: &mut World, sorted: bool) {
    let bridge = world.resource::<AuditBridge>().clone();
    if bridge.failure().is_err() || world.resource::<Extracted>().source.is_none() {
        return;
    }
    if let Err(error) = observe_inner(world, sorted, &bridge) {
        bridge.fail(error);
    }
}

fn observe_inner(world: &mut World, sorted: bool, bridge: &AuditBridge) -> Result<(), AuditError> {
    let config = bridge.lock()?.config.clone();
    let mut actual = gather(world, &config)?;
    if !sorted {
        if let Some(fixture) = &config.fixture {
            let original = actual.clone();
            if super::fixture::declare(world, &actual, fixture)? {
                actual = gather(world, &config)?;
                for (view, prior) in actual.views.iter_mut().zip(original.views) {
                    view.original_queue = Some(prior.queued);
                }
            } else {
                actual.ready = false;
                actual
                    .pending
                    .push("controlled two-shell draws pending".into());
            }
        }
        world.resource_mut::<Extracted>().queued = Some(actual);
        return Ok(());
    }
    let before = world
        .resource_mut::<Extracted>()
        .queued
        .take()
        .ok_or(AuditError::Lifecycle(
            "sorted phase without queued observation",
        ))?;
    if before.epoch != actual.epoch
        || before.views.len() != actual.views.len()
        || before.source != actual.source
    {
        return Err(AuditError::Lifecycle(
            "source/view changed inside phase sort",
        ));
    }
    for (view, prior) in actual.views.iter_mut().zip(before.views) {
        if view.render_entity != prior.render_entity {
            return Err(AuditError::Lifecycle("view identity changed inside sort"));
        }
        if !view.same_metadata(&prior) {
            return Err(AuditError::Lifecycle("view metadata changed inside sort"));
        }
        super::association::verify_permutation(&prior.queued, &view.queued)?;
        view.sorted = std::mem::take(&mut view.queued);
        view.queued = prior.queued;
        view.original_queue = prior.original_queue;
    }
    actual.ready &= before.ready;
    actual.pending.extend(before.pending);
    bridge.lock()?.collector.complete(actual)
}

#[expect(
    clippy::too_many_lines,
    reason = "a single ordered sequence that shares its local state"
)]
fn gather(world: &World, config: &AuditConfig) -> Result<EpochEvidence, AuditError> {
    let extracted = world.resource::<Extracted>();
    let source = extracted
        .source
        .clone()
        .ok_or(AuditError::Lifecycle("source table missing"))?;
    let phases = world
        .get_resource::<ViewSortedRenderPhases<Transparent3d>>()
        .ok_or(AuditError::Lifecycle("actual transparent phases absent"))?;
    if phases.len() > config.max_views {
        return Err(AuditError::Budget("actual views"));
    }
    let cache = world.resource::<PipelineCache>();
    let mut pending = Vec::new();
    let mut views = Vec::new();
    for (entity, phase) in phases.iter() {
        if phase.items.is_empty() {
            continue;
        }
        if phase.items.len() > config.max_items {
            return Err(AuditError::Budget("actual phase items"));
        }
        let view = world
            .get::<ExtractedView>(*entity)
            .ok_or_else(|| AuditError::Source("phase missing extracted view".into()))?;
        let main = world
            .get::<MainEntity>(*entity)
            .ok_or_else(|| AuditError::Source("view main/render identity missing".into()))?;
        if !view.world_from_view.compute_matrix().is_finite()
            || !view.clip_from_view.is_finite()
            || view
                .clip_from_world
                .is_some_and(|matrix| !matrix.is_finite())
        {
            return Err(AuditError::Source("nonfinite view matrices".into()));
        }
        let visible = world.get::<RenderVisibleEntities>(*entity).ok_or_else(|| {
            AuditError::Source("populated phase lacks actual visible pairs".into())
        })?;
        let pairs = super::association::VisiblePairs::new(visible, config.max_items)?;
        let mut items = Vec::with_capacity(phase.items.len());
        let mut logical = std::collections::BTreeSet::new();
        for (ordinal, item) in phase.items.iter().enumerate() {
            if !item.distance.is_finite() {
                return Err(AuditError::Source("nonfinite phase depth".into()));
            }
            let main_entity = item.entity.1.id().to_bits();
            let association = source
                .items
                .binary_search_by_key(&main_entity, |source| source.main_entity)
                .ok()
                .map(|index| &source.items[index]);
            let material = association.map_or(
                MaterialSource::Unsupported {
                    reason: "phase item has no supported visible mesh source",
                },
                |source| source.material.clone(),
            );
            if let Some(identity) = association.and_then(|source| source.logical.as_ref()) {
                if !logical.insert(identity.clone()) {
                    return Err(AuditError::Source(
                        "duplicate shell identity across view".into(),
                    ));
                }
            }
            let published = published(cache, item.pipeline);
            match published.map(|entry| &entry.state) {
                Some(CachedPipelineState::Ok(_)) => {}
                None
                | Some(
                    CachedPipelineState::Queued
                    | CachedPipelineState::Creating(_)
                    | CachedPipelineState::Err(
                        PipelineCacheError::ShaderNotLoaded(_)
                        | PipelineCacheError::ShaderImportNotYetAvailable,
                    ),
                ) => pending.push(format!("pipeline {:?}", item.pipeline)),
                Some(CachedPipelineState::Err(error)) => {
                    return Err(AuditError::Source(format!(
                        "terminal pipeline error: {error:?}"
                    )))
                }
            }
            if matches!(material, MaterialSource::Shell { .. }) {
                let instances =
                    world.resource::<RenderMaterialInstances<client::BubbleDomeMaterial>>();
                let prepared =
                    world.resource::<RenderAssets<PreparedMaterial<client::BubbleDomeMaterial>>>();
                if instances
                    .get(&item.entity.1)
                    .is_none_or(|id| prepared.get(*id).is_none())
                {
                    pending.push(format!("prepared shell {main_entity}"));
                }
                if association
                    .and_then(|source| source.logical.as_ref())
                    .is_none()
                {
                    return Err(AuditError::Source(
                        "queued shell lacks logical identity".into(),
                    ));
                }
            }
            if matches!(material, MaterialSource::Standard { .. })
                && !prepared_material::<StandardMaterial>(world, item.entity.1)
            {
                pending.push(format!("prepared standard {main_entity}"));
            }
            let render_association = pairs.inspect(
                world,
                item.entity.0,
                item.entity.1,
                item.draw_function,
                &material,
            )?;
            if matches!(render_association.kind, AssociationKind::Pending) {
                pending.push(format!("actual draw registration {main_entity}"));
            }
            items.push(PhaseEntry {
                ordinal,
                render_entity: item.entity.0.to_bits(),
                association: render_association,
                main_entity,
                logical: association.and_then(|source| source.logical.clone()),
                distance_bits: item.distance.to_bits(),
                pipeline: format!("{:?}", item.pipeline),
                pipeline_description: format!("{:?}", published.map(|entry| &entry.descriptor)),
                draw_function: format!("{:?}", item.draw_function),
                material,
                mesh: association.map(|source| source.mesh.clone()),
                batch_range: [item.batch_range.start, item.batch_range.end],
                extra_index: format!("{:?}", item.extra_index),
            });
        }
        let mut lighting = vec![
            format!(
                "uniform_offset={:?}",
                world
                    .get::<bevy::pbr::ViewLightsUniformOffset>(*entity)
                    .map(|offset| offset.offset)
            ),
            format!(
                "uniform_buffer={:?}",
                world
                    .get_resource::<bevy::pbr::LightMeta>()
                    .and_then(|meta| meta.view_gpu_lights.buffer())
                    .map(bevy::render::render_resource::Buffer::id)
            ),
        ];
        lighting.push(format!(
            "view_layers={:?}; selected_local_lights={:?}",
            world.get::<bevy::render::view::RenderLayers>(*entity),
            world
                .get::<bevy::pbr::ViewLightEntities>(*entity)
                .map(|lights| &lights.lights)
        ));
        for light_entity in world.iter_entities() {
            if let Some(light) = light_entity.get::<bevy::pbr::ExtractedDirectionalLight>() {
                lighting.push(format!("directional {:?}: color={:?}; lux={}; matrix={:?}; layers={:?}; shadows={}; depth_bias={}; normal_bias={}; cascades={:?}", light_entity.id(), light.color.to_f32_array().map(f32::to_bits), light.illuminance.to_bits(), light.transform.compute_matrix().to_cols_array().map(f32::to_bits), light.render_layers, light.shadows_enabled, light.shadow_depth_bias.to_bits(), light.shadow_normal_bias.to_bits(), light.cascade_shadow_config));
            }
        }
        lighting.sort();
        if world
            .get::<bevy::pbr::ViewLightsUniformOffset>(*entity)
            .is_none()
        {
            pending.push("actual prepared view-light offset unavailable".into());
        }
        views.push(ViewEvidence {
            render_entity: entity.to_bits(),
            main_entity: main.id().to_bits(),
            world_from_view: view
                .world_from_view
                .compute_matrix()
                .to_cols_array()
                .map(f32::to_bits),
            clip_from_view: view.clip_from_view.to_cols_array().map(f32::to_bits),
            clip_from_world: view
                .clip_from_world
                .map(|matrix| matrix.to_cols_array().map(f32::to_bits)),
            viewport: view.viewport.to_array(),
            hdr: view.hdr,
            color_grading: format!("{:?}", view.color_grading),
            lighting,
            queued: items,
            original_queue: None,
            sorted: vec![],
        });
    }
    views.sort_by_key(|view| view.render_entity);
    if views.is_empty() {
        pending.push("no relevant populated view".into());
    }
    Ok(EpochEvidence {
        epoch: extracted.epoch,
        source,
        views,
        ready: pending.is_empty(),
        pending,
    })
}

fn prepared_material<M: Material>(world: &World, entity: MainEntity) -> bool {
    let instances = world.resource::<RenderMaterialInstances<M>>();
    let prepared = world.resource::<RenderAssets<PreparedMaterial<M>>>();
    instances
        .get(&entity)
        .is_some_and(|id| prepared.get(*id).is_some())
}
