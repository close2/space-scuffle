//! The Bevy side of the effect system: loading, event wiring, the per-frame build and lights.
//!
//! # Responsibility
//! - [`load_effect_catalog`] reads the vocabulary at startup (from [`EffectCatalogSource`], or
//!   `catalog/effects.ron` under the assets root), resolves the units' presentation blocks and
//!   sizes the [`EffectRuntime`]; a failure is logged and exits the app (charter §1.2).
//! - [`spawn_effects_from_events`] turns each admitted [`CombatVisualEvent`] into an effect
//!   spawn through [`UnitEffectTables`]: a `SplashVisual` spawns its weapon's `splash` effect
//!   sized by its zones; a tracer, beam, projectile or melee event spawns its weapon's `impact`
//!   effect where it lands; a `BubbleShatter` spawns the dead unit's `death` effect, sized by
//!   its radius, where it stood (plan WP9), and the legacy glass shatter is then left out
//!   ([`replaces_shatter`]).
//! - [`spawn_mode_trails`] lays the `moving` effect of a unit that moves in a mode naming one
//!   (a burrowed unit's ground-disturbance trail, plan Amendment 2, R5), where the viewer sees
//!   the ground, at the effect's `repeat` period.
//! - [`build_effect_frame`] ages the effects to `Time<Virtual>` and rebuilds the frame for the
//!   active 3D camera; [`apply_effect_lights`] writes the frame's lights into the pooled
//!   point-light entities.
//!
//! # Fog (plan D-5)
//! Events arrive already admitted by `BattleFog::reveals`, judged where they land
//! (`CombatEventVisual::anchor`). Every effect here is anchored at that landing point (a
//! splash's centre, a line's first search point, a shot's impact or arrival); none is
//! anchored at the source, so an undetected attacker is never drawn. A line is admitted point
//! by point: it spawns once from any admitted search point and draws only at the points the
//! viewer sees ([`LineAssembly`], plan Amendment 2, R6), so an undetected burrowed attacker's
//! line shows wherever it is seen, from its first point on. A death is anchored where the unit
//! stood, which is where the fog judged it. Launch and travel effects (anchored at the source)
//! are wired by later work packages.
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

/// The pooled point lights of the `Light` block, spawned hidden at load when any effect has a
/// light layer (none otherwise).
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
    // The pool exists only when some effect has a light layer, so a vocabulary without lights
    // (the shipped one today) adds no light entity to any scene.
    let uses_lights = catalog.effects.iter().any(|effect| {
        effect
            .layers
            .iter()
            .any(|layer| matches!(layer.block, super::catalog::Block::Light { .. }))
    });
    let lights = (0..if uses_lights {
        catalog.budget.lights
    } else {
        0
    })
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

/// Merges the admitted search points of one line into one spawn (plan Amendment 2, R6).
///
/// The simulation emits every search point of a line on the same tick, one after another, and
/// the fog admits each by where it lands, so the points a viewer may see arrive consecutively.
/// Each becomes a request for the whole line that sees only its own point; this merges a run of
/// requests for the same line into one whose `seen` mask holds every admitted point. A line
/// whose first point is fogged still shows where the viewer sees it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LineAssembly {
    pending: Option<SpawnRequest>,
}

/// How far, in world units, two requests' far ends may lie and still be one line. Every
/// search point of a line publishes the same far end; the anchors are not compared, because
/// each is recomputed from its point by the line's spacing, which an unevenly spaced line (the
/// flame line's last search lies a double spacing beyond the one before it) does not hold.
const SAME_LINE_TOLERANCE: f32 = 1e-4;

impl LineAssembly {
    /// Takes `request` and returns what is ready to spawn: a request without a line at once,
    /// a line's request once a request of another line arrives, and nothing while the same
    /// line goes on.
    pub fn push(&mut self, request: SpawnRequest) -> Option<SpawnRequest> {
        let Some(line) = request.line else {
            return Some(request);
        };
        if let Some(pending) = self.pending.as_mut() {
            if let Some(pending_line) = pending.line.as_mut() {
                if pending.effect == request.effect
                    && pending_line.to.distance(line.to) <= SAME_LINE_TOLERANCE
                {
                    pending_line.seen |= line.seen;
                    return None;
                }
            }
        }
        self.pending.replace(request)
    }

    /// The line still being merged, if any; call once the frame's events are read.
    pub fn finish(&mut self) -> Option<SpawnRequest> {
        self.pending.take()
    }
}

/// The spawn an admitted `event` asks for, if its source unit (or the defaults) names one. A
/// line's search point asks for the whole line seen only at that point; [`LineAssembly`]
/// merges the points of one line.
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
            let mut anchor = world(*center);
            let world_line = match (kind, line) {
                (EventKind::Line, Some(line)) => {
                    // Any search point of the line stands for the whole line: the effect is
                    // anchored at the first point, found back along the line's axis, and sees
                    // only this point until [`LineAssembly`] merges the line's other admitted
                    // points (plan Amendment 2, R6).
                    let to = world(line.to);
                    let axis = (to - world(line.from)).normalize_or_zero();
                    anchor -= axis * line.spacing * f32::from(line.index);
                    bound.set(Binding::LineLength, anchor.distance(to));
                    bound.set(Binding::LineSpacing, line.spacing);
                    Some(WorldLine {
                        from: anchor,
                        to,
                        seen: 1 << line.index.min(63),
                    })
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
        CombatEventVisual::BubbleShatter {
            pos,
            radius,
            unit_type,
            ..
        } => tables.death(*unit_type).map(|effect| SpawnRequest {
            effect,
            anchor: world(*pos),
            bound: BoundValues::default()
                .with(Binding::UnitRadius, *radius)
                .with(Binding::UnitHeight, *radius * 2.0),
            line: None,
            delay: 0.0,
        }),
    }
}

/// Whether the death of a unit of `unit_type` draws its own `death` effect instead of the
/// legacy glass shatter (plan WP9: the default stays the shatter; a unit's presentation block,
/// or the vocabulary's `defaults.death`, may name an effect that replaces it). Without the
/// effect tables (an app without the effect system) the shatter is drawn.
#[must_use]
pub fn replaces_shatter(
    tables: Option<&UnitEffectTables>,
    unit_type: sim_types::UnitTypeId,
) -> bool {
    tables.is_some_and(|tables| tables.death(unit_type).is_some())
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
    let mut lines = LineAssembly::default();
    for event in events.read() {
        if let Some(ready) =
            spawn_request(&event.0, &tables, &catalog).and_then(|request| lines.push(request))
        {
            runtime.spawn(&catalog, &ready, now);
        }
    }
    if let Some(ready) = lines.finish() {
        runtime.spawn(&catalog, &ready, now);
    }
}

/// Speed in world units per second below which a published velocity counts as standing still.
const MOVING_SPEED: f32 = 1e-3;

/// The trail `bubble` lays in the frame that ran from `now - delta` to `now` (seconds of
/// `Time<Virtual>`), if it lays one (plan Amendment 2, R5).
///
/// A unit lays its mode's `moving` effect while its published velocity is not zero, once per
/// the effect's `repeat` period, at a phase of its own (so a group's trails do not pulse
/// together), where it stands on the ground. Under fog, the trail of another player's unit is
/// laid only where the viewer sees the ground; whether the viewer detects the unit does not
/// matter, because the unit is not drawn, only the ground it disturbs.
#[must_use]
pub fn trail_request(
    bubble: &sim_types::BubbleRenderInstance,
    fog: Option<&crate::battle_pipeline::fog::BattleFog>,
    tables: &UnitEffectTables,
    catalog: &EffectCatalog,
    now: f64,
    delta: f64,
) -> Option<SpawnRequest> {
    let mode = bubble.stance.mode?;
    let effect = tables.mode(bubble.unit_type, mode)?.moving?;
    let period = f64::from(catalog.effect(effect)?.repeat?);
    let [vx, vy] = bubble.velocity;
    if vx.hypot(vy) < MOVING_SPEED {
        return None;
    }
    let [x, y, _] = bubble.world_pos;
    if fog.is_some_and(|fog| !fog.shows(bubble.owner, x, y)) {
        return None;
    }
    let phase = f64::from(super::placement::unit_float(super::placement::hash3(
        bubble.entity_id.0,
        0x7A11,
        0,
    ))) * period;
    let beat = |time: f64| ((time + phase) / period).floor();
    if beat(now) <= beat(now - delta.max(0.0)) {
        return None;
    }
    Some(SpawnRequest {
        effect,
        anchor: Vec3::new(x, bubble.ground_z, y),
        bound: BoundValues::default().with(Binding::UnitRadius, bubble.radius),
        line: None,
        delay: 0.0,
    })
}

/// Lays the trails of the units that move in a mode whose block names a `moving` effect.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn spawn_mode_trails(
    bubbles: Query<
        '_,
        '_,
        &crate::bubble_shader::BubbleInstance,
        With<crate::scene_controller::LiveBattleBubble>,
    >,
    fog: Option<Res<'_, crate::battle_pipeline::fog::BattleFog>>,
    catalog: Option<Res<'_, EffectCatalog>>,
    tables: Option<Res<'_, UnitEffectTables>>,
    runtime: Option<ResMut<'_, EffectRuntime>>,
    time: Res<'_, Time<Virtual>>,
) {
    let (Some(catalog), Some(tables), Some(mut runtime)) = (catalog, tables, runtime) else {
        return;
    };
    if !tables.has_trails() {
        return;
    }
    let now = time.elapsed_secs_f64();
    let delta = time.delta_secs_f64();
    for bubble in &bubbles {
        if let Some(request) =
            trail_request(&bubble.0, fog.as_deref(), &tables, &catalog, now, delta)
        {
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
