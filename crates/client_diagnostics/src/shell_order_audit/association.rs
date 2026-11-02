//! Checked per-view render/main associations for the three audited material tuples.
//!
//! Temporary proxies are admitted only through actual visibility pairs, marker
//! components and registered draw IDs. Unknown custom commands remain limited.

use super::model::{
    AssociationKind, AuditError, DrawAdapter, MaterialSource, PhaseEntry, RenderAssociation,
};
use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::pbr::{DrawMesh, SetMaterialBindGroup, SetMeshBindGroup, SetMeshViewBindGroup};
use bevy::prelude::*;
use bevy::render::render_phase::{DrawFunctionId, DrawFunctions, SetItemPipeline};
use bevy::render::sync_world::{MainEntity, TemporaryRenderEntity};
use bevy::render::view::RenderVisibleEntities;
use std::collections::BTreeMap;

/// Exact public tuple corresponding to Bevy 0.15's private `DrawMaterial` alias.
/// Looking up this type inspects existing registration; it never adds a draw.
type MaterialDraw<M> = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshBindGroup<1>,
    SetMaterialBindGroup<M, 2>,
    DrawMesh,
);

/// Bounded, unambiguous full-generation proxy-to-main index for one actual view.
pub(super) struct VisiblePairs(BTreeMap<u64, u64>);

impl VisiblePairs {
    /// Indexes actual mesh pairs, rejecting duplicates rather than coalescing them.
    pub(super) fn new(visible: &RenderVisibleEntities, limit: usize) -> Result<Self, AuditError> {
        if visible.len::<With<Mesh3d>>() > limit {
            return Err(AuditError::Budget("visible mesh associations"));
        }
        let mut pairs = BTreeMap::new();
        for (render, main) in visible.iter::<With<Mesh3d>>() {
            if pairs
                .insert(render.to_bits(), main.id().to_bits())
                .is_some()
            {
                return Err(AuditError::Source(
                    "duplicate or ambiguous visible proxy".into(),
                ));
            }
        }
        Ok(Self(pairs))
    }

    /// Validates actual entity components against this view's exact visible pair.
    pub(super) fn inspect(
        &self,
        world: &World,
        render: Entity,
        main: MainEntity,
        draw: DrawFunctionId,
        material: &MaterialSource,
    ) -> Result<RenderAssociation, AuditError> {
        let entity = world
            .get_entity(render)
            .map_err(|_| AuditError::Source("phase render entity no longer exists".into()))?;
        if self.0.get(&render.to_bits()) != Some(&main.id().to_bits()) {
            return Err(AuditError::Source(
                "phase pair absent from actual view".into(),
            ));
        }
        let actual_main = entity.get::<MainEntity>().map(|main| main.id().to_bits());
        if actual_main.is_some_and(|actual| actual != main.id().to_bits()) {
            return Err(AuditError::Source(
                "render MainEntity contradicts visible pair".into(),
            ));
        }
        let temporary = entity.contains::<TemporaryRenderEntity>();
        let adapter = registered_adapter(world, material, draw);
        let kind = match adapter {
            Ok(Some(adapter)) if temporary => AssociationKind::Temporary(adapter),
            Ok(Some(adapter)) if actual_main.is_some() => AssociationKind::Synchronized(adapter),
            Ok(Some(_)) => AssociationKind::Unsupported("missing synchronized MainEntity"),
            Ok(None) => AssociationKind::Pending,
            Err(reason) => AssociationKind::Unsupported(reason),
        };
        Ok(RenderAssociation {
            visible_pair_confirmed: true,
            temporary_marker: temporary,
            render_main_entity: actual_main,
            kind,
        })
    }
}

/// Resolves the exact existing material tuple registration; absence is pending.
fn registered_adapter(
    world: &World,
    material: &MaterialSource,
    actual: DrawFunctionId,
) -> Result<Option<DrawAdapter>, &'static str> {
    let Some(functions) = world.get_resource::<DrawFunctions<Transparent3d>>() else {
        return Ok(None);
    };
    let functions = functions.read();
    let (adapter, expected) = match material {
        MaterialSource::Shell { .. } => (
            DrawAdapter::Shell,
            functions.get_id::<MaterialDraw<client::BubbleDomeMaterial>>(),
        ),
        MaterialSource::Standard { .. } => (
            DrawAdapter::Standard,
            functions.get_id::<MaterialDraw<StandardMaterial>>(),
        ),
        MaterialSource::Unsupported { .. } => return Err("unsupported material draw association"),
    };
    match expected {
        None => Ok(None),
        Some(expected) if expected == actual => Ok(Some(adapter)),
        Some(_) => Err("draw function differs from actual material tuple"),
    }
}

/// Validates the raw sort permutation, including multiplicity and proxy IDs.
/// Canonical serialization compares complete values, not a lossy hash; only the
/// vector ordinal is cleared. This allocation belongs solely to the bounded audit.
pub(super) fn verify_permutation(
    before: &[PhaseEntry],
    after: &[PhaseEntry],
) -> Result<(), AuditError> {
    fn keys(items: &[PhaseEntry]) -> Result<Vec<String>, AuditError> {
        let mut keys = Vec::with_capacity(items.len());
        for item in items {
            let mut payload = item.clone();
            payload.ordinal = 0;
            keys.push(
                ron::to_string(&payload).map_err(|error| AuditError::Source(error.to_string()))?,
            );
        }
        keys.sort();
        Ok(keys)
    }
    if before.len() != after.len() || keys(before)? != keys(after)? {
        return Err(AuditError::Lifecycle(
            "raw phase payload changed during sort",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
