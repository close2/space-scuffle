//! Explicit two-shell experiment over actual admitted Swarm media and materials.
//!
//! Only this opt-in fixture changes render poses/visibility and two queued items.
//! It preserves authoritative state, actual radii, appearance and unrelated draws.

use super::{
    model::{AuditError, FixtureOrder, FixtureSpec},
    AuditBridge,
};
use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::prelude::*;
use bevy::render::render_phase::ViewSortedRenderPhases;

/// Explicit controlled-fixture declaration; never present in ordinary captures.
#[derive(Resource, Clone)]
pub struct ControlledFixture(pub FixtureSpec);

/// Returns the declared visible identities only in the explicit controlled profile.
pub(crate) fn participants(world: &World) -> Option<[u32; 2]> {
    world
        .get_resource::<ControlledFixture>()
        .map(|value| value.0.participants)
}

/// Applies only declared centers and visibility after actual Swarm synchronization.
pub(super) fn place(world: &mut World) {
    if !world
        .resource::<client::LabState>()
        .current_tab
        .eq(&client::LabTab::SwarmStressTest)
    {
        return;
    }
    let spec = world.resource::<ControlledFixture>().0.clone();
    let bridge = world.resource::<AuditBridge>().clone();
    if bridge.failure().is_err() {
        return;
    }
    if let Err(error) = place_inner(world, &spec) {
        bridge.fail(error);
    }
}

fn place_inner(world: &mut World, spec: &FixtureSpec) -> Result<(), AuditError> {
    let mut query = world.query_filtered::<(
        Entity,
        &client::BubbleInstance,
        &MeshMaterial3d<client::BubbleDomeMaterial>,
    ), With<client::SwarmBubble>>();
    let actors: Vec<_> = query
        .iter(world)
        .map(|(entity, instance, material)| (entity, instance.0.entity_id.0, material.0.clone()))
        .collect();
    if actors.is_empty() {
        return Ok(());
    }
    for id in spec.participants {
        if actors.iter().filter(|(_, actual, _)| *actual == id).count() != 1 {
            return Err(AuditError::Source(
                "controlled participant absent or duplicated".into(),
            ));
        }
    }
    for (entity, logical, material) in actors {
        let index = spec.participants.iter().position(|id| *id == logical);
        let mut actor = world.entity_mut(entity);
        actor.insert(if index.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if let Some(index) = index {
            let position = Vec3::from_array(spec.positions[index]);
            actor
                .get_mut::<Transform>()
                .ok_or_else(|| AuditError::Source("fixture transform absent".into()))?
                .translation = position;
            let world_pos = [position.x, position.z, position.y];
            actor
                .get_mut::<client::BubbleInstance>()
                .ok_or_else(|| AuditError::Source("fixture instance absent".into()))?
                .0
                .world_pos = world_pos;
            world
                .resource_mut::<Assets<client::BubbleDomeMaterial>>()
                .get_mut(&material)
                .ok_or_else(|| AuditError::Source("fixture actual material absent".into()))?
                .uniform
                .world_pos = position;
        }
    }
    Ok(())
}

/// Swaps whole actual phase items only for the declared exact-depth experiment.
pub(super) fn declare(
    world: &mut World,
    evidence: &super::model::EpochEvidence,
    spec: &FixtureSpec,
) -> Result<bool, AuditError> {
    let mut changes = Vec::new();
    for view in &evidence.views {
        let positions: Vec<_> =
            spec.participants
                .iter()
                .map(|id| {
                    view.queued.iter().position(|entry| {
                        entry.logical.as_ref().is_some_and(|logical| {
                            logical.domain == "Swarm" && logical.entity == *id
                        })
                    })
                })
                .collect();
        let [Some(a), Some(b)] = positions[..] else {
            return Ok(false);
        };
        if view.queued[a].distance_bits != view.queued[b].distance_bits {
            return Err(AuditError::Source(
                "controlled centers do not have identical actual view-depth bits".into(),
            ));
        }
        if view.queued[a.min(b)..=a.max(b)]
            .iter()
            .any(|entry| entry.logical.is_none())
        {
            return Err(AuditError::Source(
                "controlled shells cross an unrelated draw".into(),
            ));
        }
        let swap = match spec.order {
            FixtureOrder::Forward => a > b,
            FixtureOrder::Reverse => b > a,
        };
        if swap {
            changes.push((Entity::from_bits(view.render_entity), a, b));
        }
    }
    if evidence.views.is_empty() {
        return Ok(false);
    }
    let mut phases = world.resource_mut::<ViewSortedRenderPhases<Transparent3d>>();
    for (view, a, b) in changes {
        let phase = phases
            .get_mut(&view)
            .ok_or(AuditError::Lifecycle("controlled view disappeared"))?;
        phase.items.swap(a, b);
    }
    Ok(true)
}

/// Checks the actual propagated world center against snapshot XY-altitude and shader XYZ.
pub(super) fn validate_centers(
    transform: Vec3,
    instance: [f32; 3],
    uniform: Vec3,
) -> Result<(), AuditError> {
    let snapshot_xyz = Vec3::new(instance[0], instance[2], instance[1]);
    if !transform.is_finite()
        || !snapshot_xyz.is_finite()
        || !uniform.is_finite()
        || transform.to_array().map(f32::to_bits) != snapshot_xyz.to_array().map(f32::to_bits)
        || transform.to_array().map(f32::to_bits) != uniform.to_array().map(f32::to_bits)
    {
        return Err(AuditError::Source(
            "controlled transform, snapshot and shader centers disagree".into(),
        ));
    }
    Ok(())
}
