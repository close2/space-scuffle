//! Exact CPU-source fingerprint for the visual state submitted to screenshot readback.
//! Bevy extracts default Time and writes its wrapped seconds into Globals.time;
//! these are source values, not a readback of the private GPU globals buffer.

use super::{super::config::CaptureError, failure};
use bevy::{
    ecs::system::{SystemParam, SystemState},
    prelude::*,
};
use client::vfx::{
    BeamVisualComponent, CombatEffectOwner, CombatFxMaterial, MeleeSlashComponent,
    ProjectileVisual, ShatterShard,
};
use serde::{Deserialize, Serialize};

/// Actual timer and material identity for one relevant effect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::runtime_capture) struct EffectFingerprint {
    pub entity: u64,
    pub material: String,
    pub owner: u8,
    pub seed_bits: u32,
    pub shader_age_bits: u32,
    pub shader_kind: u32,
    pub color_bits: [u32; 4],
    pub aspect_bits: u32,
    pub elapsed_ns: u64,
    pub duration_ns: u64,
}

/// Exact effect state and default-Time inputs consumed by Bevy's Globals extraction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::runtime_capture) struct Fingerprint {
    pub presentation_elapsed_ns: u64,
    pub globals_wrapped_seconds_bits: u32,
    pub delta_ns: u64,
    pub effects: Vec<EffectFingerprint>,
}

impl Fingerprint {
    /// Reads the real resources/components without advancing or replacing any clock.
    pub(in crate::runtime_capture) fn capture(world: &mut World) -> Result<Self, CaptureError> {
        SystemState::<ReadbackEvidence<'_, '_>>::new(world)
            .get(world)
            .capture()
    }

    /// Rejects any visual time, identity, material seed, or timer change during readback.
    pub(in crate::runtime_capture) fn verify(&self, actual: &Self) -> Result<(), CaptureError> {
        if self != actual || self.delta_ns != 0 {
            return Err(failure(
                "visual effect or shader time changed during asynchronous readback",
            ));
        }
        Ok(())
    }
}

type ActualEffect = (
    Entity,
    Option<&'static CombatEffectOwner>,
    &'static MeshMaterial3d<CombatFxMaterial>,
    Option<&'static BeamVisualComponent>,
    Option<&'static ProjectileVisual>,
    Option<&'static MeleeSlashComponent>,
    Option<&'static ShatterShard>,
);

/// Read-only screenshot callback access to the same actual source state as Last verification.
#[derive(SystemParam)]
pub(in crate::runtime_capture) struct ReadbackEvidence<'w, 's> {
    time: Res<'w, Time>,
    materials: Res<'w, Assets<CombatFxMaterial>>,
    effects: Query<'w, 's, ActualEffect>,
}

impl ReadbackEvidence<'_, '_> {
    /// Copies actual relevant effects, sorted by entity identity for stable equality.
    pub(in crate::runtime_capture) fn capture(&self) -> Result<Fingerprint, CaptureError> {
        let mut effects = Vec::new();
        for (entity, owner, handle, beam, projectile, melee, shard) in &self.effects {
            let owner = owner.ok_or_else(|| failure("readback effect owner missing"))?;
            let material = self
                .materials
                .get(&handle.0)
                .ok_or_else(|| failure("readback effect material missing"))?;
            let timer = beam
                .map(|value| &value.lifetime)
                .or_else(|| projectile.map(|value| &value.lifetime))
                .or_else(|| melee.map(|value| &value.lifetime))
                .or_else(|| shard.map(|value| &value.lifetime))
                .ok_or_else(|| failure("readback effect timer missing"))?;
            if !material.uniform.age.is_finite()
                || !material.uniform.seed.is_finite()
                || !material.uniform.aspect.is_finite()
                || !material.uniform.primary_color.is_finite()
                || material.uniform.age.to_bits() != timer.fraction().to_bits()
            {
                return Err(failure(
                    "readback material is nonfinite or its age differs from actual timer",
                ));
            }
            effects.push(EffectFingerprint {
                entity: entity.to_bits(),
                material: format!("{:?}", handle.0.id()),
                owner: owner.0 .0,
                seed_bits: material.uniform.seed.to_bits(),
                shader_age_bits: material.uniform.age.to_bits(),
                shader_kind: material.uniform.fx_kind,
                color_bits: material.uniform.primary_color.to_array().map(f32::to_bits),
                aspect_bits: material.uniform.aspect.to_bits(),
                elapsed_ns: nanoseconds(timer.elapsed())?,
                duration_ns: nanoseconds(timer.duration())?,
            });
        }
        effects.sort_by_key(|sample| sample.entity);
        Ok(Fingerprint {
            presentation_elapsed_ns: nanoseconds(self.time.elapsed())?,
            globals_wrapped_seconds_bits: self.time.elapsed_secs_wrapped().to_bits(),
            delta_ns: nanoseconds(self.time.delta())?,
            effects,
        })
    }
}

/// Converts exact nanoseconds to the RON-supported representation without truncation.
fn nanoseconds(duration: std::time::Duration) -> Result<u64, CaptureError> {
    u64::try_from(duration.as_nanos())
        .map_err(|_| failure("visual timestamp exceeds u64 nanosecond representation"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nanosecond_encoding_preserves_large_values_and_rejects_overflow() -> Result<(), CaptureError>
    {
        assert_eq!(
            nanoseconds(std::time::Duration::from_secs(5))?,
            5_000_000_000
        );
        assert!(nanoseconds(std::time::Duration::MAX).is_err());
        Ok(())
    }
}
