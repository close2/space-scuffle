//! Reads actual production effect timers/material identity for declared samples.
//! Capture never writes timers, shader seed, lifetimes, or ownership.

use super::{super::config::CaptureError, failure, EffectKind};
use crate::vfx::{
    BeamVisualComponent, CombatEffectOwner, CombatFxMaterial, MeleeSlashComponent,
    ProjectileVisual, ShatterShard,
};
use bevy::prelude::*;
use std::time::Duration;

/// Actual effect identity and time, copied without modifying production state.
#[derive(Clone, Copy)]
pub(super) struct Sample {
    pub entity: Entity,
    pub material: AssetId<CombatFxMaterial>,
    pub seed: u32,
    pub owner: sim_types::PlayerId,
    pub elapsed: Duration,
    pub duration: Duration,
}

/// Reads all active effects; a mismatched kind or missing lifetime is an error.
pub(super) fn samples(world: &mut World, kind: EffectKind) -> Result<Vec<Sample>, CaptureError> {
    let mut query = world.query::<(
        Entity,
        Option<&CombatEffectOwner>,
        &MeshMaterial3d<CombatFxMaterial>,
        Option<&BeamVisualComponent>,
        Option<&ProjectileVisual>,
        Option<&MeleeSlashComponent>,
        Option<&ShatterShard>,
    )>();
    let materials = world.resource::<Assets<CombatFxMaterial>>();
    query
        .iter(world)
        .map(|(entity, owner, handle, beam, projectile, melee, shard)| {
            let owner = owner.ok_or_else(|| failure("sample effect owner missing"))?;
            let timer = beam
                .map(|value| &value.lifetime)
                .or_else(|| projectile.map(|value| &value.lifetime))
                .or_else(|| melee.map(|value| &value.lifetime))
                .or_else(|| shard.map(|value| &value.lifetime))
                .ok_or_else(|| failure("sample effect has no production timer"))?;
            let material = materials
                .get(&handle.0)
                .ok_or_else(|| failure("sample effect material missing"))?;
            if !material.uniform.age.is_finite()
                || !material.uniform.seed.is_finite()
                || material.uniform.age.to_bits() != timer.fraction().to_bits()
            {
                return Err(failure("sample shader age is stale or nonfinite"));
            }
            if material.uniform.fx_kind != kind.shader_kind() {
                return Err(failure("sample contains wrong effect subtype"));
            }
            Ok(Sample {
                entity,
                material: handle.0.id(),
                seed: material.uniform.seed.to_bits(),
                owner: owner.0,
                elapsed: timer.elapsed(),
                duration: timer.duration(),
            })
        })
        .collect()
}
