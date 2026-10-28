//! Admission of authored directional-light inputs for the view-bound mist shader.
//!
//! This module does not choose lights. Bevy's prepared view buffer owns visibility,
//! layers, world-space direction, ordering, and capacity. Admission checks every rig,
//! including hidden rigs, before extraction and remains failed after an invalid edit.
//! The reference illuminance is immutable for one pool's lifetime, so retained GPU
//! packets and newly prepared view lights always share the same numerical contract.

use super::{MistError, MistPool};
use bevy::pbr::MAX_DIRECTIONAL_LIGHTS;
use bevy::prelude::*;

/// A directional-light input cannot be represented safely by the actual GPU formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MistLightError {
    /// Illuminance must be finite and nonnegative, in lux.
    #[error("mist light {0:?} has invalid illuminance in lux")]
    Illuminance(Entity),
    /// A linear RGB channel must be finite and nonnegative.
    #[error("mist light {entity:?} has invalid linear RGB channel {channel}")]
    Color {
        /// Main-world light entity, including hidden rigs.
        entity: Entity,
        /// Zero-based RGB channel.
        channel: usize,
    },
    /// Bevy's f32 RGB-times-lux packing overflowed before normalization.
    #[error("mist light {entity:?} RGB-times-lux overflow in channel {channel}")]
    Packing {
        /// Main-world light entity.
        entity: Entity,
        /// Zero-based RGB channel.
        channel: usize,
    },
    /// Normalized radiance exceeds the conservative finite accumulation envelope.
    #[error(
        "mist light {entity:?} normalized channel {channel} exceeds finite accumulation envelope"
    )]
    Normalized {
        /// Main-world light entity.
        entity: Entity,
        /// Zero-based RGB channel.
        channel: usize,
    },
}

/// Sticky main/render-world admission; `Pending` never authorizes a mist draw.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LightAdmission {
    #[default]
    Pending,
    Admitted,
    Failed(MistLightError),
}

impl LightAdmission {
    pub(super) const fn admitted(self) -> bool {
        matches!(self, Self::Admitted)
    }
}

/// Returns the legacy authored response reference, in lux, for absent RON fields.
pub(super) const fn default_reference_lux() -> f32 {
    10_000.0
}

/// Validates the exact f32 divisor and reciprocal used by possible shader lowering.
pub(super) fn validate_reference(reference: f32) -> Result<(), MistError> {
    if !reference.is_finite()
        || reference <= 0.0
        || !(1.0 / reference).is_finite()
        || 1.0 / reference <= 0.0
    {
        return Err(MistError::InvalidStyle(
            "reference illuminance and its f32 reciprocal must be finite and positive",
        ));
    }
    Ok(())
}

fn channel_limit() -> f32 {
    // The engine's compile-time limit is small on every supported backend. Summing
    // `MAX` four times in f32 avoids narrowing a target-dependent usize constant.
    let count = (0..MAX_DIRECTIONAL_LIGHTS).fold(0.0_f32, |sum, _| sum + 1.0);
    f32::MAX / (4.0 * count)
}

fn validate_light(
    entity: Entity,
    light: &DirectionalLight,
    reference: f32,
) -> Result<(), MistLightError> {
    let lux = light.illuminance;
    if !lux.is_finite() || lux < 0.0 {
        return Err(MistLightError::Illuminance(entity));
    }
    let color = light.color.to_linear();
    for (channel, value) in [color.red, color.green, color.blue].into_iter().enumerate() {
        if !value.is_finite() || value < 0.0 {
            return Err(MistLightError::Color { entity, channel });
        }
        // Match engine packing before shader division. An algebraically equivalent
        // f64 expression could hide an f32 intermediate overflow and is not admission.
        let packed = value * lux;
        if !packed.is_finite() {
            return Err(MistLightError::Packing { entity, channel });
        }
        let normalized = packed / reference;
        // Also admit reciprocal multiplication: shader backends may lower division.
        let lowered = packed * (1.0 / reference);
        if !normalized.is_finite()
            || !lowered.is_finite()
            || normalized >= channel_limit()
            || lowered >= channel_limit()
        {
            return Err(MistLightError::Normalized { entity, channel });
        }
    }
    Ok(())
}

pub(super) fn admit_lights(
    pool: Option<Res<'_, MistPool>>,
    lights: Query<'_, '_, (Entity, Ref<'_, DirectionalLight>)>,
    mut admission: ResMut<'_, LightAdmission>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if matches!(*admission, LightAdmission::Failed(_)) {
        return;
    }
    let Some(pool) = pool else { return };
    for (entity, light) in &lights {
        if *admission == LightAdmission::Pending || light.is_changed() {
            if let Err(error) = validate_light(entity, &light, pool.style.reference_illuminance_lux)
            {
                *admission = LightAdmission::Failed(error);
                error!("{}", MistError::Light(error));
                exit.send(AppExit::error());
                return;
            }
        }
    }
    *admission = LightAdmission::Admitted;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light(rgb: f32, lux: f32) -> DirectionalLight {
        DirectionalLight {
            color: Color::linear_rgb(rgb, rgb, rgb),
            illuminance: lux,
            ..default()
        }
    }

    #[test]
    fn exact_f32_admission_rejects_intermediate_overflow_and_negative_data() {
        let entity = Entity::from_raw(7);
        assert!(validate_light(entity, &light(1.0, 14_000.0), 10_000.0).is_ok());
        assert!(matches!(
            validate_light(entity, &light(f32::MAX, 2.0), f32::MAX),
            Err(MistLightError::Packing { .. })
        ));
        assert!(matches!(
            validate_light(entity, &light(1.0, f32::MAX), 0.5),
            Err(MistLightError::Normalized { .. })
        ));
        assert!(matches!(
            validate_light(entity, &light(-0.1, 1.0), 10_000.0),
            Err(MistLightError::Color { .. })
        ));
        assert!(matches!(
            validate_light(entity, &light(1.0, f32::NAN), 10_000.0),
            Err(MistLightError::Illuminance(_))
        ));
        assert!(validate_light(entity, &light(0.0, 0.0), 10_000.0).is_ok());
    }

    #[test]
    fn conservative_boundary_uses_adjacent_f32_values() {
        let entity = Entity::from_raw(8);
        let limit = channel_limit();
        let below = f32::from_bits(limit.to_bits() - 1);
        assert!(validate_light(entity, &light(below, 1.0), 1.0).is_ok());
        assert!(matches!(
            validate_light(entity, &light(limit, 1.0), 1.0),
            Err(MistLightError::Normalized { .. })
        ));
        let above = f32::from_bits(limit.to_bits() + 1);
        assert!(validate_light(entity, &light(above, 1.0), 1.0).is_err());
    }

    #[test]
    fn reference_checks_reciprocal_and_legacy_style_default(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for reference in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::from_bits(1)] {
            assert!(validate_reference(reference).is_err());
        }
        assert!(validate_reference(f32::MAX).is_ok());
        let authored = include_str!("../../../../assets/catalog/mist.ron");
        let legacy = authored.replace("    reference_illuminance_lux: 10000.0,\n", "");
        let style: super::super::MistStyle = ron::from_str(&legacy)?;
        assert_eq!(style.reference_illuminance_lux, default_reference_lux());
        style.validate()?;
        Ok(())
    }

    #[test]
    fn hidden_new_and_changed_rigs_are_validated_and_failure_is_sticky(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<LightAdmission>()
            .add_event::<AppExit>()
            .add_systems(Update, admit_lights);
        let pool = app.world_mut().run_system_once(
            |mut commands: Commands<'_, '_>, mut meshes: ResMut<'_, Assets<Mesh>>| {
                let style = super::super::MistStyle {
                    max_emitters: 1,
                    ..default()
                };
                super::super::allocate_pool(style, &mut commands, &mut meshes)
            },
        )??;
        app.insert_resource(pool);
        let hidden = app
            .world_mut()
            .spawn((light(1.0, 14_000.0), Visibility::Hidden))
            .id();
        app.update();
        assert_eq!(
            *app.world().resource::<LightAdmission>(),
            LightAdmission::Admitted
        );
        app.world_mut()
            .entity_mut(hidden)
            .insert(light(1.0, f32::NAN));
        app.update();
        assert_eq!(
            *app.world().resource::<LightAdmission>(),
            LightAdmission::Failed(MistLightError::Illuminance(hidden))
        );
        assert!(!app.world().resource::<Events<AppExit>>().is_empty());
        app.world_mut()
            .entity_mut(hidden)
            .insert(light(1.0, 14_000.0));
        app.update();
        assert!(matches!(
            app.world().resource::<LightAdmission>(),
            LightAdmission::Failed(_)
        ));
        Ok(())
    }
}
