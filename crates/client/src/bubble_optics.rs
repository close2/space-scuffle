//! Validated, unit-agnostic glass optics and their render-extraction synchronization.
//!
//! The snapshot owns gameplay signals; this module owns authored presentation parameters.
//! One loaded resource updates existing uniform lanes after snapshot synchronization without
//! creating materials, changing media handles, or allocating in the per-frame update.

use crate::{BubbleDomeMaterial, BubbleUniformGpu, ResolvedAssetsRoot};
use bevy::prelude::*;
use bevy::render::view::{RenderLayers, VisibilitySystems};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Failure to load a complete, finite optical style from the actual asset root.
#[derive(Debug, thiserror::Error)]
pub enum OpticsError {
    /// Authored style could not be read.
    #[error("cannot read bubble optics {path}: {source}")]
    Read {
        /// Exact attempted runtime path.
        path: PathBuf,
        /// Operating-system failure.
        source: std::io::Error,
    },
    /// RON document does not match the authored schema.
    #[error("invalid bubble optics syntax: {0}")]
    Parse(#[from] ron::error::SpannedError),
    /// A named setting violates its documented numeric contract.
    #[error("bubble optics setting outside its finite range: {0}")]
    Invalid(&'static str),
}

/// Generic optical settings, independent of creature identities and simulation balance.
#[derive(Debug, Clone, Resource, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BubbleOptics {
    /// Exposure of already-linearized video, in stops (-2..2).
    pub media_exposure_stops: f32,
    /// Dielectric reflectance at normal incidence (0..0.2).
    pub reflectance: f32,
    /// Power shaping the grazing-angle Fresnel transition (2..12).
    pub fresnel_power: f32,
    /// Linear reflected-light intensity (0..2).
    pub reflection_strength: f32,
    /// Glossy lobe roughness (0.05..1).
    pub glare_roughness: f32,
    /// Linear peak glossy highlight intensity (0..4).
    pub glare_strength: f32,
    /// Peak localized damage response (0..2).
    pub hit_strength: f32,
    /// Hull response angular half-width in radians (0.1..1.5).
    pub hit_spread_radians: f32,
    /// Linear selection rim intensity (0..2).
    pub selection_strength: f32,
    /// Healthy shell/media composite alpha (0.1..1).
    pub shell_alpha: f32,
    /// Linear facing indicator intensity (0..2).
    pub facing_strength: f32,
    /// Linear attack aperture intensity (0..2).
    pub attack_strength: f32,
}

impl Default for BubbleOptics {
    fn default() -> Self {
        Self {
            media_exposure_stops: 0.0,
            reflectance: 0.04,
            fresnel_power: 5.0,
            reflection_strength: 0.85,
            glare_roughness: 0.22,
            glare_strength: 1.5,
            hit_strength: 0.8,
            hit_spread_radians: 0.55,
            selection_strength: 0.7,
            shell_alpha: 0.94,
            facing_strength: 0.55,
            attack_strength: 0.7,
        }
    }
}

impl BubbleOptics {
    /// Loads and validates an actual RON file; no fallback is substituted on failure.
    ///
    /// # Errors
    /// Returns a typed read, syntax, or numeric validation failure.
    pub fn load(path: &Path) -> Result<Self, OpticsError> {
        let source = std::fs::read_to_string(path).map_err(|source| OpticsError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let style: Self = ron::from_str(&source)?;
        style.validate()?;
        Ok(style)
    }

    /// Validates finite numeric ranges before settings reach a GPU uniform.
    ///
    /// # Errors
    /// Returns the first invalid field's stable schema name.
    pub fn validate(&self) -> Result<(), OpticsError> {
        for (name, value, min, max) in [
            ("media_exposure_stops", self.media_exposure_stops, -2.0, 2.0),
            ("reflectance", self.reflectance, 0.0, 0.2),
            ("fresnel_power", self.fresnel_power, 2.0, 12.0),
            ("reflection_strength", self.reflection_strength, 0.0, 2.0),
            ("glare_roughness", self.glare_roughness, 0.05, 1.0),
            ("glare_strength", self.glare_strength, 0.0, 4.0),
            ("hit_strength", self.hit_strength, 0.0, 2.0),
            ("hit_spread_radians", self.hit_spread_radians, 0.1, 1.5),
            ("selection_strength", self.selection_strength, 0.0, 2.0),
            ("shell_alpha", self.shell_alpha, 0.1, 1.0),
            ("facing_strength", self.facing_strength, 0.0, 2.0),
            ("attack_strength", self.attack_strength, 0.0, 2.0),
        ] {
            if !value.is_finite() || value < min || value > max {
                return Err(OpticsError::Invalid(name));
            }
        }
        Ok(())
    }

    /// Packs the three authored parameter lanes; directions are supplied by the scene.
    #[must_use]
    pub fn uniform_lanes(&self) -> [Vec4; 3] {
        [
            Vec4::new(
                self.media_exposure_stops,
                self.reflectance,
                self.fresnel_power,
                self.reflection_strength,
            ),
            Vec4::new(
                self.glare_roughness,
                self.glare_strength,
                self.hit_strength,
                self.hit_spread_radians,
            ),
            Vec4::new(
                self.selection_strength,
                self.shell_alpha,
                self.facing_strength,
                self.attack_strength,
            ),
        ]
    }

    /// Applies authored settings and a world-space direction toward the primary light.
    pub fn apply(&self, uniform: &mut BubbleUniformGpu, light: Vec4) {
        let [media, glare, signals] = self.uniform_lanes();
        uniform.optics_media = media;
        uniform.optics_glare = glare;
        uniform.optics_signals = signals;
        uniform.optics_light = light;
    }
}

/// Installs strict runtime style loading and optical uniform updates after scene sync.
pub struct BubbleOpticsPlugin;

impl Plugin for BubbleOpticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_style).add_systems(
            PostUpdate,
            sync_optics
                .after(TransformSystem::TransformPropagate)
                .after(VisibilitySystems::CheckVisibility),
        );
    }
}

fn load_style(
    mut commands: Commands<'_, '_>,
    root: Res<'_, ResolvedAssetsRoot>,
    mut exit: EventWriter<'_, AppExit>,
) {
    match BubbleOptics::load(&root.as_path().join("catalog/bubble_optics.ron")) {
        Ok(style) => commands.insert_resource(style),
        Err(error) => {
            error!("{error}");
            exit.send(AppExit::error());
        }
    }
}

// Explicit visibility/layer contracts mirror the renderer light extraction inputs.
#[allow(clippy::type_complexity)]
fn sync_optics(
    style: Option<Res<'_, BubbleOptics>>,
    lights: Query<
        '_,
        '_,
        (
            &DirectionalLight,
            &GlobalTransform,
            &ViewVisibility,
            Option<&RenderLayers>,
        ),
    >,
    handles: Query<'_, '_, (&MeshMaterial3d<BubbleDomeMaterial>, Option<&RenderLayers>)>,
    mut materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
) {
    let Some(style) = style else {
        return;
    };
    let lanes = style.uniform_lanes();
    let default_layers = RenderLayers::default();
    for (handle, layers) in &handles {
        let layers = layers.unwrap_or(&default_layers);
        let strongest = lights
            .iter()
            .filter(|(_, _, visible, light_layers)| {
                visible.get() && layers.intersects(light_layers.unwrap_or(&default_layers))
            })
            .max_by(|a, b| a.0.illuminance.total_cmp(&b.0.illuminance));
        let light = strongest.map_or(Vec4::new(0.0, 1.0, 0.0, 0.0), |(light, transform, _, _)| {
            let direction = *transform.back();
            direction.extend((light.illuminance / 20_000.0).clamp(0.0, 1.0))
        });
        let Some(material) = materials.get(&handle.0) else {
            continue;
        };
        let uniform = &material.uniform;
        let changed = uniform.optics_media != lanes[0]
            || uniform.optics_glare != lanes[1]
            || uniform.optics_signals != lanes[2]
            || uniform.optics_light != light;
        // Assets::iter_mut marks every visited material modified. Check through an immutable
        // handle lookup first so unchanged and shared materials do not trigger extraction.
        if changed {
            if let Some(material) = materials.get_mut(&handle.0) {
                style.apply(&mut material.uniform, light);
            }
        }
    }
}
