//! Explicit preview-only actions over the normal scene/event entrypoints. Old
//! effects expire through their real lifetime systems before a new subtype is
//! emitted; no fixture despawns effects or claims these events are sim combat.

use super::super::config::CaptureError;
use super::{failure, OwnershipState, View};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use client::player_presentation::PlayerPalette;
use client::vfx::{CombatFxMaterial, CombatVisualEvent};
use client::{LabState, LabTab, SwarmBubble};
use sim_types::CombatEventVisual;
use std::time::Duration;

/// Separate preview event subtype with its own rendered geometry checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runtime_capture) enum EffectKind {
    Hitscan,
    Projectile,
    Laser,
    Lightning,
    Melee,
    Shatter,
}

impl EffectKind {
    /// Returns the existing combat shader discriminant, independently of owner identity.
    pub(in crate::runtime_capture) fn shader_kind(self) -> u32 {
        match self {
            Self::Hitscan => 1,
            Self::Projectile => 4,
            Self::Laser => 0,
            Self::Lightning => 2,
            Self::Melee => 3,
            Self::Shatter => 5,
        }
    }

    fn event(self, origin: [f32; 3], owner: sim_types::PlayerId) -> CombatEventVisual {
        let mut end = origin;
        end[0] += 3.0;
        match self {
            Self::Hitscan => CombatEventVisual::HitscanTracer {
                from: origin,
                to: end,
                owner,
                source: sim_types::EffectSource::default(),
            },
            Self::Projectile => CombatEventVisual::ProjectileVisual {
                pos: origin,
                vel: [2.0, 0.0, 0.0],
                owner,
                duration_secs: Some(1.2),
                source: sim_types::EffectSource::default(),
            },
            Self::Laser | Self::Lightning => CombatEventVisual::BeamVisual {
                from: origin,
                to: end,
                owner,
                is_lightning: self == Self::Lightning,
                source: sim_types::EffectSource::default(),
            },
            Self::Melee => CombatEventVisual::MeleeSlashVisual {
                pos: origin,
                angle_rad: 0.75,
                radius: 0.8,
                owner,
                source: sim_types::EffectSource::default(),
            },
            Self::Shatter => CombatEventVisual::BubbleShatter {
                pos: origin,
                radius: 0.8,
                is_dome: false,
                owner,
                unit_type: sim_types::UnitTypeId::default(),
            },
        }
    }
}

impl OwnershipState {
    /// Selects a normal production tab and optional world-X/Z camera focus.
    pub(in crate::runtime_capture) fn apply(
        &mut self,
        world: &mut World,
        view: View,
        elapsed: Duration,
    ) -> Result<Option<Vec2>, CaptureError> {
        match view {
            View::NeutralTower => {
                world.resource_mut::<LabState>().current_tab = LabTab::LiveBattle;
                let tower = world
                    .resource::<client::runtime_battle::MapPresentation>()
                    .objects
                    .iter()
                    .find(|record| record.ownership_tint && record.owner.is_none())
                    .ok_or_else(|| failure("real map has no neutral tower"))?;
                Ok(Some(tower.position.xz()))
            }
            View::Swarm => {
                let mut lab = world.resource_mut::<LabState>();
                lab.current_tab = LabTab::SwarmStressTest;
                lab.swarm_active = true;
                Ok(None)
            }
            View::Effect(kind) => {
                world.resource_mut::<LabState>().current_tab = LabTab::VisualLab;
                if elapsed > Duration::from_millis(40) {
                    self.continue_lightning(world, kind, elapsed)?;
                    return Ok(self.stage.map(|stage| stage.focus));
                }
                if elapsed != Duration::from_millis(40) {
                    return Err(failure("initial effect sample must be40ms"));
                }
                self.pending_effect = Some(kind);
                self.effect_emitted = false;
                self.target_elapsed = elapsed;
                self.stage = None;
                self.lightning_identity = None;
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Ages real preview lifetimes and returns a world-X/Z focus when actors exist.
    pub(in crate::runtime_capture) fn settle(
        &mut self,
        world: &mut World,
        view: View,
    ) -> Result<Option<Vec2>, CaptureError> {
        if view == View::Swarm {
            if let Some(ids) = crate::shell_order_audit::fixture::participants(world) {
                let mut centers = [None, None];
                let mut query = world.query_filtered::<(&client::BubbleInstance, &GlobalTransform), With<SwarmBubble>>();
                for (instance, transform) in query.iter(world) {
                    if let Some(index) = ids.iter().position(|id| *id == instance.0.entity_id.0) {
                        if centers[index]
                            .replace(transform.translation().xz())
                            .is_some()
                        {
                            return Err(failure("controlled focus identity duplicated"));
                        }
                    }
                }
                return match centers {
                    [Some(a), Some(b)] if a.is_finite() && b.is_finite() => Ok(Some((a + b) * 0.5)),
                    [Some(_), Some(_)] => Err(failure("controlled propagated focus is nonfinite")),
                    _ => Ok(None),
                };
            }
            let mut query = world.query_filtered::<&Transform, With<SwarmBubble>>();
            let mut low = Vec2::splat(f32::INFINITY);
            let mut high = Vec2::splat(f32::NEG_INFINITY);
            for transform in query.iter(world) {
                low = low.min(transform.translation.xz());
                high = high.max(transform.translation.xz());
            }
            return Ok(low.is_finite().then_some((low + high) * 0.5));
        }
        let View::Effect(kind) = view else {
            return Ok(None);
        };
        if !self.effect_emitted
            && world
                .query::<&MeshMaterial3d<CombatFxMaterial>>()
                .iter(world)
                .next()
                .is_some()
        {
            *world.resource_mut::<TimeUpdateStrategy>() =
                TimeUpdateStrategy::ManualDuration(Duration::from_secs(2));
            return Ok(None);
        }
        if !self.effect_emitted {
            // Time was updated before this control system. Changing the strategy
            // affects the next frame only; a residual expiry delta would destroy
            // a newly spawned short-lived effect during this frame's Present set.
            *world.resource_mut::<TimeUpdateStrategy>() =
                TimeUpdateStrategy::ManualDuration(Duration::ZERO);
            if !world.resource::<Time>().delta().is_zero() {
                return Ok(None);
            }
        }
        if self.stage.is_none() {
            self.stage = super::staging::prepare(world)?;
        }
        let Some(stage) = self.stage else {
            return Ok(None);
        };
        if !self.effect_emitted {
            let owner = world.resource::<PlayerPalette>().effect();
            world
                .resource_mut::<Events<CombatVisualEvent>>()
                .send(CombatVisualEvent(kind.event(stage.origin, owner)));
            self.effect_emitted = true;
        }
        self.advance_sample(world, kind)?;
        Ok(Some(stage.focus))
    }

    /// Checks post-production exact age before any frozen settling frame is counted.
    pub(in crate::runtime_capture) fn sample_ready(
        &self,
        world: &mut World,
        kind: EffectKind,
    ) -> Result<bool, CaptureError> {
        if !self.effect_emitted || self.pending_effect != Some(kind) {
            return Ok(false);
        }
        let samples = super::timing::samples(world, kind)?;
        if samples.is_empty() {
            return Ok(false);
        }
        for sample in &samples {
            if sample.elapsed > self.target_elapsed || sample.duration <= self.target_elapsed {
                return Err(failure(
                    "post-production effect sample exceeded its declared time",
                ));
            }
        }
        Ok(world.resource::<Time>().delta().is_zero()
            && samples
                .iter()
                .all(|sample| sample.elapsed == self.target_elapsed))
    }

    fn continue_lightning(
        &mut self,
        world: &mut World,
        kind: EffectKind,
        elapsed: Duration,
    ) -> Result<(), CaptureError> {
        let previous = match elapsed {
            value if value == Duration::from_millis(80) => Duration::from_millis(40),
            value if value == Duration::from_millis(120) => Duration::from_millis(80),
            _ => return Err(failure("unsupported lightning sample target")),
        };
        if kind != EffectKind::Lightning
            || self.pending_effect != Some(kind)
            || !self.effect_emitted
            || self.target_elapsed != previous
            || self.stage.is_none()
        {
            return Err(failure(
                "lightning continuation lacks its original prepared sample",
            ));
        }
        let samples = super::timing::samples(world, kind)?;
        if samples.len() != 1 || samples[0].elapsed != previous {
            return Err(failure(
                "lightning continuation changed elapsed time or membership",
            ));
        }
        self.check_lightning_identity(samples[0])?;
        self.target_elapsed = elapsed;
        Ok(())
    }

    fn check_lightning_identity(
        &mut self,
        sample: super::timing::Sample,
    ) -> Result<(), CaptureError> {
        if let Some(original) = self.lightning_identity {
            if original.entity != sample.entity
                || original.material != sample.material
                || original.seed != sample.seed
                || original.owner != sample.owner
                || original.duration != sample.duration
            {
                return Err(failure(
                    "retained lightning identity/material/seed/lifetime changed",
                ));
            }
        } else {
            self.lightning_identity = Some(sample);
        }
        Ok(())
    }

    fn advance_sample(&mut self, world: &mut World, kind: EffectKind) -> Result<(), CaptureError> {
        let samples = super::timing::samples(world, kind)?;
        let Some(first) = samples.first() else {
            return Ok(());
        };
        for sample in &samples {
            if sample.elapsed > self.target_elapsed
                || sample.duration <= self.target_elapsed
                || sample.elapsed != first.elapsed
            {
                return Err(failure(
                    "effect sample overrun, inconsistent age or insufficient lifetime",
                ));
            }
            if kind == EffectKind::Lightning {
                self.check_lightning_identity(*sample)?;
            }
        }
        if world.resource::<Time>().delta().is_zero() && first.elapsed < self.target_elapsed {
            *world.resource_mut::<TimeUpdateStrategy>() = TimeUpdateStrategy::ManualDuration(
                self.target_elapsed
                    .checked_sub(first.elapsed)
                    .ok_or_else(|| failure("effect sample time subtraction underflow"))?,
            );
        }
        Ok(())
    }
}
