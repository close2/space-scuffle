//! The Bevy side of the effect system: loading, event wiring, the per-frame build and lights.
//!
//! # Responsibility
//! - [`load_effect_catalog`] reads the vocabulary at startup (from [`EffectCatalogSource`], or
//!   `catalog/effects.ron` under the assets root), resolves the units' presentation blocks and
//!   sizes the [`EffectRuntime`]; a failure is logged and exits the app (charter §1.2).
//! - [`spawn_effects_from_events`] turns each admitted [`CombatVisualEvent`] into an effect
//!   spawn through [`UnitEffectTables`]: a `SplashVisual` spawns its weapon's `splash` effect
//!   sized by its zones; a tracer, beam, projectile or melee event spawns its weapon's `impact`
//!   effect where it lands.
//! - [`build_effect_frame`] ages the effects to `Time<Virtual>` and rebuilds the frame for the
//!   active 3D camera; [`apply_effect_lights`] writes the frame's lights into the pooled
//!   point-light entities.
//!
//! # Fog (plan D-5)
//! Events arrive already admitted by `BattleFog::reveals`, judged where they land
//! (`CombatEventVisual::anchor`). Every effect here is anchored at that landing point (a
//! splash's centre, a line's first search point, a shot's impact or arrival); none is
//! anchored at the source, so an undetected attacker is never drawn. Launch and travel effects
//! (anchored at the source) and death effects are wired by later work packages.
//!
//! # Invariants
//! - Without effects named in data, nothing is spawned and nothing new is drawn.
//! - The steady state allocates nothing: events are read in place, the runtime's buffers are
//!   retained, and the light pool's entities exist from startup.

use super::binding::BoundValues;
use super::catalog::schema::{Binding, EventKind};
use super::catalog::{EffectCatalog, UnitEffectTables, EFFECTS_CATALOG_PATH};
use super::placement::WorldLine;
use super::render::EffectCatalogVersion;
use super::runtime::{CameraView, EffectRuntime, SpawnRequest};
use crate::vfx::{CombatLightEffect, CombatVisualEvent};
use crate::{CatalogResource, ResolvedAssetsRoot};
use bevy::prelude::*;
use sim_types::snapshot::CombatEventVisual;
use sim_types::EffectSource;
use std::path::PathBuf;

/// An explicit vocabulary file, replacing the shipped `catalog/effects.ron` (tests and tools).
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct EffectCatalogSource(pub PathBuf);

/// The pooled point lights of the `Light` block, spawned hidden at load.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectLightPool {
    /// The light entities.
    pub lights: Vec<Entity>,
}

/// Marks a pooled effect light.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectLight;

/// Reads the vocabulary and the units' blocks, sizes the runtime and spawns the light pool.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn load_effect_catalog(
    mut commands: Commands<'_, '_>,
    root: Res<'_, ResolvedAssetsRoot>,
    source: Option<Res<'_, EffectCatalogSource>>,
    units: Option<Res<'_, CatalogResource>>,
    version: Res<'_, EffectCatalogVersion>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let path = source.map_or_else(
        || root.as_path().join(EFFECTS_CATALOG_PATH),
        |source| source.0.clone(),
    );
    let loaded = EffectCatalog::load(&path, root.as_path()).and_then(|catalog| {
        let tables = units.as_ref().map_or_else(
            || Ok(UnitEffectTables::default()),
            |units| UnitEffectTables::build(&units.0, &catalog),
        )?;
        Ok((catalog, tables))
    });
    let (catalog, tables) = match loaded {
        Ok(loaded) => loaded,
        Err(error) => {
            error!("Effect catalog rejected: {error}");
            exit.send(AppExit::error());
            return;
        }
    };
    let lights = (0..catalog.budget.lights)
        .map(|_| {
            commands
                .spawn((
                    EffectLight,
                    CombatLightEffect,
                    PointLight {
                        intensity: 0.0,
                        shadows_enabled: false,
                        ..default()
                    },
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .id()
        })
        .collect();
    commands.insert_resource(EffectLightPool { lights });
    commands.insert_resource(EffectCatalogVersion(version.0.wrapping_add(1)));
    commands.insert_resource(EffectRuntime::new(&catalog));
    commands.insert_resource(tables);
    commands.insert_resource(catalog);
}

/// Snapshot `[x, y, altitude]` as a Bevy world position (Y up).
fn world(position: [f32; 3]) -> Vec3 {
    Vec3::new(position[0], position[2], position[1])
}

/// The spawn an admitted `event` asks for, if its source unit (or the defaults) names one.
#[must_use]
pub fn spawn_request(
    event: &CombatEventVisual,
    tables: &UnitEffectTables,
    catalog: &EffectCatalog,
) -> Option<SpawnRequest> {
    let impact = |source: &EffectSource, at: [f32; 3], delay: f32| {
        tables.impact(*source).map(|effect| SpawnRequest {
            effect,
            anchor: world(at),
            bound: BoundValues::default().with(Binding::FlightTime, delay),
            line: None,
            delay,
        })
    };
    match event {
        CombatEventVisual::SplashVisual {
            center,
            zones,
            line,
            source,
            ..
        } => {
            let effect = tables.splash(*source)?;
            let kind = catalog.effect(effect)?.event;
            let mut bound = BoundValues::default()
                .with(Binding::SplashFull, zones[0])
                .with(Binding::SplashMid, zones[1])
                .with(Binding::SplashReach, zones[2]);
            let anchor = world(*center);
            let world_line = match (kind, line) {
                (EventKind::Line, Some(line)) => {
                    if line.index != 0 {
                        return None;
                    }
                    let to = world(line.to);
                    bound.set(Binding::LineLength, anchor.distance(to));
                    bound.set(Binding::LineSpacing, line.spacing);
                    Some(WorldLine { from: anchor, to })
                }
                _ => None,
            };
            Some(SpawnRequest {
                effect,
                anchor,
                bound,
                line: world_line,
                delay: 0.0,
            })
        }
        CombatEventVisual::HitscanTracer { to, source, .. }
        | CombatEventVisual::BeamVisual { to, source, .. } => impact(source, *to, 0.0),
        CombatEventVisual::MeleeSlashVisual { pos, source, .. } => impact(source, *pos, 0.0),
        CombatEventVisual::ProjectileVisual {
            pos,
            vel,
            duration_secs,
            source,
            ..
        } => {
            let flight = duration_secs.unwrap_or(0.0).max(0.0);
            let arrival = [
                pos[0] + vel[0] * flight,
                pos[1] + vel[1] * flight,
                pos[2] + vel[2] * flight,
            ];
            impact(source, arrival, flight)
        }
        CombatEventVisual::BubbleShatter { .. } => None,
    }
}

/// Spawns the effect of every admitted combat event.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn spawn_effects_from_events(
    mut events: EventReader<'_, '_, CombatVisualEvent>,
    catalog: Option<Res<'_, EffectCatalog>>,
    tables: Option<Res<'_, UnitEffectTables>>,
    runtime: Option<ResMut<'_, EffectRuntime>>,
    time: Res<'_, Time<Virtual>>,
) {
    let (Some(catalog), Some(tables), Some(mut runtime)) = (catalog, tables, runtime) else {
        events.clear();
        return;
    };
    let now = time.elapsed_secs_f64();
    for event in events.read() {
        if let Some(request) = spawn_request(&event.0, &tables, &catalog) {
            runtime.spawn(&catalog, &request, now);
        }
    }
}

/// The active 3D camera as a [`CameraView`].
fn camera_view(
    cameras: &Query<'_, '_, (&Camera, &GlobalTransform, &Projection), With<Camera3d>>,
) -> Option<CameraView> {
    let (camera, transform, projection) = cameras
        .iter()
        .filter(|(camera, _, _)| camera.is_active)
        .max_by_key(|(camera, _, _)| camera.order)?;
    let Projection::Perspective(perspective) = projection else {
        return None;
    };
    let viewport = camera
        .physical_viewport_size()
        .map_or(Vec2::new(1280.0, 720.0), |size| size.as_vec2());
    Some(CameraView {
        position: transform.translation(),
        forward: transform.forward().as_vec3(),
        fov_y: perspective.fov,
        viewport_px: viewport.y.max(1.0),
        aspect: viewport.x / viewport.y.max(1.0),
    })
}

/// Ages the effects and rebuilds this frame's instances.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn build_effect_frame(
    catalog: Option<Res<'_, EffectCatalog>>,
    runtime: Option<ResMut<'_, EffectRuntime>>,
    time: Res<'_, Time<Virtual>>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform, &Projection), With<Camera3d>>,
) {
    let (Some(catalog), Some(mut runtime)) = (catalog, runtime) else {
        return;
    };
    if runtime.pool.live_total() == 0
        && runtime.particles.is_empty()
        && runtime.frame.sorted.is_empty()
        && runtime.frame.decals.is_empty()
        && runtime.frame.lights.is_empty()
    {
        return;
    }
    let camera = camera_view(&cameras);
    runtime.build_frame(&catalog, time.elapsed_secs_f64(), camera.as_ref());
}

/// Writes the frame's lights into the pooled light entities and hides the rest.
pub fn apply_effect_lights(
    runtime: Option<Res<'_, EffectRuntime>>,
    pool: Option<Res<'_, EffectLightPool>>,
    mut lights: Query<
        '_,
        '_,
        (
            &mut PointLight,
            &mut Transform,
            &mut GlobalTransform,
            &mut Visibility,
        ),
        With<EffectLight>,
    >,
) {
    let (Some(runtime), Some(pool)) = (runtime, pool) else {
        return;
    };
    if !runtime.is_changed() {
        return;
    }
    let frame_lights = &runtime.frame.lights;
    for (index, entity) in pool.lights.iter().enumerate() {
        let Ok((mut light, mut transform, mut global, mut visibility)) = lights.get_mut(*entity)
        else {
            continue;
        };
        if let Some(wanted) = frame_lights.get(index) {
            light.color = Color::linear_rgb(wanted.color[0], wanted.color[1], wanted.color[2]);
            light.intensity = wanted.intensity;
            light.range = wanted.range.max(0.01);
            transform.translation = Vec3::from_array(wanted.position);
            *global = GlobalTransform::from(*transform);
            visibility.set_if_neq(Visibility::Visible);
        } else {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}
