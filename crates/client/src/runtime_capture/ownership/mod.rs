//! Opt-in ownership capture evidence over actual production entities/materials.
//! Identity is read from snapshots and admitted effect components, never inferred
//! from hue. Retained effects keep their first-observed origin across tab changes.
//! Capture-only collections are outside production synchronization/performance scope.

mod control;
mod freeze;
mod geometry;
mod staging;
mod timing;

use super::config::CaptureError;
use crate::battle_pipeline::session::BattleSession;
use crate::player_presentation::PlayerPalette;
use crate::vfx::{
    BeamVisualComponent, CombatEffectOwner, CombatFxMaterial, MeleeSlashComponent,
    ProjectileVisual, ShatterShard,
};
use bevy::prelude::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub(super) use control::EffectKind;
pub(super) use freeze::{Fingerprint, ReadbackEvidence};
pub(super) use geometry::OwnershipProof;

/// Wraps capture evidence failures without changing production state.
pub(super) fn failure(message: impl Into<String>) -> CaptureError {
    CaptureError::Verification(message.into())
}

/// Exact ownership checkpoint context, including explicit preview-only events.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum View {
    Battle,
    Lab,
    Return,
    Reset,
    NeutralTower,
    Swarm,
    Effect(EffectKind),
}

/// First-observed effect identity and lifetime history; generation is absent for previews.
#[derive(Debug, Clone, Serialize)]
pub(super) struct EffectProof {
    pub entity: u64,
    pub owner: u8,
    pub material: String,
    pub source_generation: Option<u64>,
    pub first_observed_tick: u32,
    pub linear_rgb: [f32; 3],
    pub remaining_seconds: f32,
    pub ended: Option<&'static str>,
}

/// Capture-session history retained across ordinary scene transitions and resets.
#[derive(Default)]
pub(super) struct OwnershipState {
    effects: BTreeMap<u64, EffectProof>,
    last_generation: Option<u64>,
    retained_at_combat: BTreeSet<u64>,
    pending_effect: Option<EffectKind>,
    effect_emitted: bool,
    target_elapsed: std::time::Duration,
    stage: Option<staging::Stage>,
    lightning_identity: Option<timing::Sample>,
}

impl OwnershipState {
    /// Records actual admitted effects, rejecting unexplained disappearance or identity changes.
    pub(super) fn observe(&mut self, world: &mut World, view: View) -> Result<(), CaptureError> {
        let pipeline = world.resource::<BattleSession>();
        let generation = pipeline.commands.generation().map(|value| value.value());
        let tick = pipeline.frames.presented().tick.0;
        let preview = world.resource::<crate::LabState>().current_tab != crate::LabTab::LiveBattle;
        let reset = self.last_generation.is_some() && generation != self.last_generation;
        let dt = world.resource::<Time>().delta_secs();
        let mut query = world.query::<(
            Entity,
            Option<&CombatEffectOwner>,
            &MeshMaterial3d<CombatFxMaterial>,
            Option<&BeamVisualComponent>,
            Option<&ProjectileVisual>,
            Option<&MeleeSlashComponent>,
            Option<&ShatterShard>,
        )>();
        let mut present = BTreeSet::new();
        for (entity, owner, handle, beam, projectile, melee, shard) in query.iter(world) {
            let owner =
                owner.ok_or_else(|| failure("actual effect has no captured owner component"))?;
            let id = entity.to_bits();
            present.insert(id);
            let material = world
                .resource::<Assets<CombatFxMaterial>>()
                .get(&handle.0)
                .ok_or_else(|| failure("ownership effect material missing"))?;
            let remaining = beam
                .map(|v| v.lifetime.remaining_secs())
                .or_else(|| projectile.map(|v| v.lifetime.remaining_secs()))
                .or_else(|| melee.map(|v| v.lifetime.remaining_secs()))
                .or_else(|| shard.map(|v| v.lifetime.remaining_secs()))
                .ok_or_else(|| failure("ownership effect has no production lifetime"))?;
            if let Some(proof) = self.effects.get_mut(&id) {
                if proof.ended.is_some()
                    || proof.owner != owner.0 .0
                    || proof.material != format!("{:?}", handle.0.id())
                    || proof.linear_rgb.map(f32::to_bits)
                        != material
                            .uniform
                            .primary_color
                            .truncate()
                            .to_array()
                            .map(f32::to_bits)
                {
                    return Err(failure("retained effect identity/material/owner changed"));
                }
                proof.remaining_seconds = remaining;
            } else {
                let palette = world.resource::<PlayerPalette>();
                let style = if preview {
                    palette.preview_style(owner.0)
                } else {
                    palette.battle_style(owner.0, &world.resource::<BattleSession>().metadata)
                }
                .map_err(|error| failure(error.to_string()))?;
                if style.linear.truncate() != material.uniform.primary_color.truncate() {
                    return Err(failure("new effect hue differs from its admitted owner"));
                }
                self.effects.insert(
                    id,
                    EffectProof {
                        entity: id,
                        owner: owner.0 .0,
                        material: format!("{:?}", handle.0.id()),
                        source_generation: if preview { None } else { generation },
                        first_observed_tick: tick,
                        linear_rgb: style.linear.truncate().to_array(),
                        remaining_seconds: remaining,
                        ended: None,
                    },
                );
            }
        }
        for (id, proof) in &mut self.effects {
            if proof.ended.is_none() && !present.contains(id) {
                proof.ended = Some(if reset || view == View::Reset {
                    "generation reset"
                } else if proof.remaining_seconds <= dt + 0.000_01 {
                    "production lifetime expired"
                } else {
                    return Err(failure(format!(
                        "effect {id} disappeared before expiry/reset"
                    )));
                });
            }
        }
        if matches!(view, View::Lab | View::Return) {
            for id in &self.retained_at_combat {
                if self
                    .effects
                    .get(id)
                    .is_some_and(|entry| entry.ended.is_some())
                {
                    return Err(failure(
                        "zero-duration Lab/Return lost a retained combat effect",
                    ));
                }
            }
        }
        self.last_generation = generation;
        Ok(())
    }

    /// Marks live combat effects whose identity must survive the zero-duration tab round trip.
    pub(super) fn remember_combat(&mut self) {
        self.retained_at_combat = self
            .effects
            .iter()
            .filter(|(_, entry)| entry.ended.is_none())
            .map(|(id, _)| *id)
            .collect();
    }
}

#[cfg(test)]
pub(super) mod tests;
