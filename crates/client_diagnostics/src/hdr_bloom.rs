//! The HDR-and-bloom arm of the plan's D-11 experiment (`2026-11-09-effect-visualization`,
//! WP6c): turns every 3D camera of a diagnostic App into an HDR camera with a thresholded bloom.
//!
//! # Responsibility
//! Owns [`HdrBloomPlugin`] and the bloom it applies ([`experiment_bloom`]). `battle_capture
//! --effect-lab ... --hdr-bloom` and `frame_budget ... --hdr-bloom` add it, so the A/B contact
//! sheets, captures and frame budgets of D-11 run the production client, effect plugin and
//! shaders with only the camera changed.
//!
//! # How it fits
//! The shipped client is unchanged: the experiment lives in the diagnostics crate, which may
//! add plugins to the production App (decision D3 of the remediation plan). The effect
//! renderer already specialises its pipelines for an HDR view (`client::effects::render`).
//!
//! # Invariants
//! - Only values above [`BLOOM_THRESHOLD`] (linear, after exposure, before tone mapping) feed
//!   the bloom, so lit terrain and bubbles at or below display white do not glow.
//! - The camera's tone mapping is left as it is: with `hdr` on, Bevy applies it in a post pass
//!   to everything, effects included, instead of inside each PBR shader.

use bevy::core_pipeline::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};
use bevy::prelude::*;

/// Linear value below which nothing blooms (D-11: only emissive values above 1.0).
pub const BLOOM_THRESHOLD: f32 = 1.0;

/// Softness of the threshold's knee: values from about 0.9 to 1.1 blend in, so a core does
/// not switch its halo on in one frame as it cools through the threshold.
pub const BLOOM_THRESHOLD_SOFTNESS: f32 = 0.2;

/// Strength of the additive bloom.
pub const BLOOM_INTENSITY: f32 = 0.18;

/// The bloom of the experiment: Bevy's natural low-frequency shape with a threshold at
/// [`BLOOM_THRESHOLD`] and an additive composite, which Bevy recommends with a prefilter.
#[must_use]
pub fn experiment_bloom() -> Bloom {
    Bloom {
        intensity: BLOOM_INTENSITY,
        prefilter: BloomPrefilter {
            threshold: BLOOM_THRESHOLD,
            threshold_softness: BLOOM_THRESHOLD_SOFTNESS,
        },
        composite_mode: BloomCompositeMode::Additive,
        ..Bloom::NATURAL
    }
}

/// Makes every 3D camera an HDR camera with [`experiment_bloom`], on the frame it appears.
pub struct HdrBloomPlugin;

impl Plugin for HdrBloomPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, apply_hdr_bloom)
            .add_systems(PreUpdate, apply_hdr_bloom);
    }
}

/// The 3D cameras that have no bloom yet.
type UnbloomedCameras<'w, 's> =
    Query<'w, 's, (Entity, &'static mut Camera), (With<Camera3d>, Without<Bloom>)>;

/// Turns on `hdr` and adds the bloom on every 3D camera that has no bloom yet.
fn apply_hdr_bloom(mut commands: Commands<'_, '_>, mut cameras: UnbloomedCameras<'_, '_>) {
    for (entity, mut camera) in &mut cameras {
        camera.hdr = true;
        commands.entity(entity).insert(experiment_bloom());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_values_above_display_white_bloom() {
        let bloom = experiment_bloom();
        assert!((bloom.prefilter.threshold - 1.0).abs() < f32::EPSILON);
        assert_eq!(bloom.composite_mode, BloomCompositeMode::Additive);
        assert!(bloom.intensity > 0.0);
    }

    #[test]
    fn the_plugin_makes_a_camera_hdr_with_bloom() {
        let mut app = App::new();
        app.add_plugins(HdrBloomPlugin);
        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), Camera::default()))
            .id();
        app.update();
        let world = app.world();
        assert!(world.get::<Camera>(camera).is_some_and(|camera| camera.hdr));
        assert!(world.get::<Bloom>(camera).is_some());
    }
}
