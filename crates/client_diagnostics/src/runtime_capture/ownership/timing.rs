//! Reads actual production effect timers/material identity for declared samples.
//! Capture never writes timers, shader seed, lifetimes, or ownership.

use super::{super::config::CaptureError, failure, EffectKind};
use bevy::prelude::*;
use client::vfx::{
    BeamVisualComponent, CombatEffectOwner, CombatFxInstance, CombatFxMaterial,
    MeleeSlashComponent, ProjectileVisual, ShatterShard,
};
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
        &CombatFxInstance,
        Option<&BeamVisualComponent>,
        Option<&ProjectileVisual>,
        Option<&MeleeSlashComponent>,
        Option<&ShatterShard>,
    )>();
    query
        .iter(world)
        .map(
            |(entity, owner, handle, instance, beam, projectile, melee, shard)| {
                let owner = owner.ok_or_else(|| failure("sample effect owner missing"))?;
                let timer = beam
                    .map(|value| &value.lifetime)
                    .or_else(|| projectile.map(|value| &value.lifetime))
                    .or_else(|| melee.map(|value| &value.lifetime))
                    .or_else(|| shard.map(|value| &value.lifetime))
                    .ok_or_else(|| failure("sample effect has no production timer"))?;
                let shader = &instance.uniform;
                if !shader.age.is_finite()
                    || !shader.seed.is_finite()
                    || shader.age.to_bits() != timer.fraction().to_bits()
                {
                    return Err(failure("sample shader age is stale or nonfinite"));
                }
                if shader.fx_kind != kind.shader_kind() {
                    return Err(failure("sample contains wrong effect subtype"));
                }
                Ok(Sample {
                    entity,
                    material: handle.0.id(),
                    seed: shader.seed.to_bits(),
                    owner: owner.0,
                    elapsed: timer.elapsed(),
                    duration: timer.duration(),
                })
            },
        )
        .collect()
}
