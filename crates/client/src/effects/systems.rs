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
//!   ([`replaces_shatter`]). A tracer, beam or projectile also spawns its weapon's `travel`
//!   effect along the shot's line from the muzzle to where it lands ([`travel_request`], plan
//!   WP11), and the legacy combat card of an event whose unit names its weapon's looks is left
//!   out ([`replaces_legacy`]). An instant hit's or a blow's point is first moved from the
//!   struck unit's centre to its bubble's near surface (`effects::struck`), so the slug, the
//!   beam and the impact meet the glass instead of being drawn inside it.
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
//! stood, which is where the fog judged it. A shot's travel starts at its muzzle, but it is
//! admitted with its event where it lands and cut where the viewer stops seeing its path
//! ([`shot_clip`], plan WP11): a shot out of the fog is drawn from the fog's edge on. Its
//! source is not drawn, only the light of its round; as in `StarCraft II`, a cloaked attacker's
//! shots are seen (the legacy cards drew them from the source too).
//!
//! # Invariants
//! - Without effects named in data, nothing is spawned and nothing new is drawn.
//! - The steady state allocates nothing: events are read in place, the runtime's buffers are
//!   retained, and the light pool's entities exist from startup.

use super::binding::BoundValues;
use super::catalog::schema::{Binding, EventKind};
use super::catalog::{EffectCatalog, ShotReach, UnitEffectTables, EFFECTS_CATALOG_PATH};
use super::particles::LineClip;
use super::placement::WorldLine;
use super::render::EffectCatalogVersion;
use super::runtime::{CameraView, EffectRuntime, SpawnRequest, NO_CLIP};
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

/// [`spawn_request_struck`] for an event whose struck bubble is not known: an impact binds
/// [`super::struck::DEFAULT_STRUCK_RADIUS`] as `UnitRadius`.
#[must_use]
pub fn spawn_request(
    event: &CombatEventVisual,
    tables: &UnitEffectTables,
    catalog: &EffectCatalog,
) -> Option<SpawnRequest> {
    spawn_request_struck(event, None, tables, catalog)
}

/// The spawn an admitted `event` asks for, if its source unit (or the defaults) names one. A
/// line's search point asks for the whole line seen only at that point; [`LineAssembly`]
/// merges the points of one line.
#[must_use]
pub fn spawn_request_struck(
    event: &CombatEventVisual,
    struck_radius: Option<f32>,
    tables: &UnitEffectTables,
    catalog: &EffectCatalog,
) -> Option<SpawnRequest> {
    let facing = super::struck::hit_facing(event).unwrap_or(0.0);
    let impact = |source: &EffectSource, at: [f32; 3], delay: f32| {
        tables.impact(*source).map(|effect| SpawnRequest {
            effect,
            anchor: world(at),
            bound: BoundValues::default()
                .with(Binding::FlightTime, delay)
                .with(Binding::Facing, facing)
                .with(
                    Binding::UnitRadius,
                    struck_radius.unwrap_or(super::struck::DEFAULT_STRUCK_RADIUS),
                ),
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
        // An instant shot's impact waits for its slug to arrive (review 1, E1-1).
        CombatEventVisual::HitscanTracer { to, source, .. }
        | CombatEventVisual::BeamVisual { to, source, .. } => {
            impact(source, *to, tables.impact_delay(*source))
        }
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

/// Distance in world units between the fog samples [`shot_clip`] takes along a shot.
const SHOT_FOG_STEP: f32 = 0.05;

/// Least distance in world units either side of a shot's path at which [`shot_clip`] asks the
/// fog; a shot's own reach (`Effect::reach`, its widest layers) is used when it is larger.
const SHOT_FOG_HALF_WIDTH: f32 = 0.35;

/// World units a shot's fog cut keeps beyond the last unseen sample: the clip's anti-aliased
/// edge reaches a little past its end (as `areas::CUT_MARGIN`).
const SHOT_FOG_MARGIN: f32 = 0.05;

/// The travel spawn of an admitted weapon `event`, if its source unit's block names a `travel`
/// effect for the weapon (plan WP11): the shot along a line from the muzzle to where it lands.
///
/// A tracer's or a beam's line starts where the shot leaves its source's bubble (the event
/// publishes the source's centre; the bubble's radius comes from the catalog, at most
/// 0.8 of the shot's length) and ends at the impact; a projectile's runs from where it was
/// launched (the simulation already publishes it on the source's bubble) to where it arrives,
/// and binds its flight time. A melee strike, a splash and a death have no travel.
#[must_use]
pub fn travel_request(
    event: &CombatEventVisual,
    tables: &UnitEffectTables,
    catalog: &EffectCatalog,
) -> Option<SpawnRequest> {
    let (from, to, flight, source) = match event {
        CombatEventVisual::HitscanTracer {
            from, to, source, ..
        }
        | CombatEventVisual::BeamVisual {
            from, to, source, ..
        } => {
            let (from, to) = (world(*from), world(*to));
            let length = from.distance(to);
            let muzzle = tables.muzzle(source.unit_type).min(0.8 * length);
            let from = from + (to - from).normalize_or_zero() * muzzle;
            (from, to, 0.0, source)
        }
        CombatEventVisual::ProjectileVisual {
            pos,
            vel,
            duration_secs,
            source,
            ..
        } => {
            let flight = duration_secs.unwrap_or(0.0).max(0.0);
            let from = world(*pos);
            (from, from + world(*vel) * flight, flight, source)
        }
        CombatEventVisual::MeleeSlashVisual { .. }
        | CombatEventVisual::SplashVisual { .. }
        | CombatEventVisual::BubbleShatter { .. } => return None,
    };
    let effect = tables.travel(*source)?;
    catalog.effect(effect)?;
    Some(SpawnRequest {
        effect,
        anchor: from,
        bound: BoundValues::default()
            .with(Binding::LineLength, from.distance(to))
            .with(Binding::FlightTime, flight),
        line: Some(WorldLine::whole(from, to)),
        delay: 0.0,
    })
}

/// The fog clip of a shot along `line` (Bevy world space) whose layers draw up to `reach`
/// world units from it (`Effect::reach`), for a viewer who sees map point `(x, y)` when
/// `sees(x, y)` (plan WP11, D-5): the shot is admitted where it lands, so it is drawn from its
/// landing point back to where it leaves the viewer's sight, and no further.
///
/// The fog is asked every [`SHOT_FOG_STEP`] from the landing point back towards the muzzle,
/// on the path and at its reach (at least [`SHOT_FOG_HALF_WIDTH`]) either side of it. Returns
/// [`NO_CLIP`] when every sample is seen, and otherwise a clip window along the shot's
/// horizontal direction that starts [`SHOT_FOG_MARGIN`] past the last seen sample before the
/// first unseen one (closer to the landing point). Fail-closed across the path too: unseen
/// ground anywhere within the reach beside the path cuts everything behind it, so a shot running
/// along a fog edge within its reach draws nothing over it (review 1, E1-2 (b)).
///
/// The clip is computed once, when the shot spawns: a projectile flies its flight under the
/// fog of that moment (at most a second or so; recorded in Amendment 25, E1-2 (c)).
#[must_use]
pub fn shot_clip(line: WorldLine, reach: ShotReach, sees: impl Fn(f32, f32) -> bool) -> LineClip {
    let along = Vec2::new(line.to.x - line.from.x, line.to.z - line.from.z);
    let length = along.length();
    if length <= SHOT_FOG_STEP {
        return NO_CLIP;
    }
    let axis = along / length;
    let normal = Vec2::new(-axis.y, axis.x);
    let start = Vec2::new(line.from.x, line.from.z);
    // Samples across the path every half cell out to the reach there, so no unseen cell beside
    // the path within the reach is stepped over: the flying layers' reach along the whole path,
    // and the muzzle layers' as well within their reach of the muzzle (review 2, E2-1).
    let seen_at = |distance: f32| {
        let half = if distance <= reach.muzzle {
            reach.muzzle.max(reach.path)
        } else {
            reach.path
        }
        .max(SHOT_FOG_HALF_WIDTH);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a reach of a few world units over half a cell"
        )]
        let across = (half / 0.5).ceil() as i32;
        let point = start + axis * distance;
        (-across..=across).all(|step| {
            #[expect(clippy::cast_precision_loss, reason = "a step below a few dozen")]
            let offset = (step as f32 / across.max(1) as f32) * half;
            let sample = point + normal * offset;
            sees(sample.x, sample.y)
        })
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a shot of at most a few dozen world units over a step of 0.05"
    )]
    let steps = (length / SHOT_FOG_STEP).ceil() as u32;
    for step in 0..=steps {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a step count below a few thousand is exact in f32"
        )]
        let distance = (length - step as f32 * SHOT_FOG_STEP).max(0.0);
        if !seen_at(distance) {
            let cut = (distance + SHOT_FOG_STEP + SHOT_FOG_MARGIN).min(length);
            let origin = axis.dot(start);
            return [axis.x, axis.y, origin + cut, origin + length + 1.0e3];
        }
    }
    NO_CLIP
}

/// Whether the effect system draws the delivery of weapon `event` in place of its legacy
/// combat card (plan WP11): a tracer, beam, projectile or melee event whose source unit names
/// its weapon's looks in data ([`UnitEffectTables::names_weapon`]), or a death whose unit has
/// a death effect ([`replaces_shatter`]). Without the effect tables nothing is replaced.
#[must_use]
pub fn replaces_legacy(tables: Option<&UnitEffectTables>, event: &CombatEventVisual) -> bool {
    match event {
        CombatEventVisual::BubbleShatter { unit_type, .. } => replaces_shatter(tables, *unit_type),
        CombatEventVisual::HitscanTracer { source, .. }
        | CombatEventVisual::BeamVisual { source, .. }
        | CombatEventVisual::ProjectileVisual { source, .. }
        | CombatEventVisual::MeleeSlashVisual { source, .. } => {
            tables.is_some_and(|tables| tables.names_weapon(*source))
        }
        CombatEventVisual::SplashVisual { .. } => false,
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
    fog: Option<Res<'_, crate::battle_pipeline::fog::BattleFog>>,
    time: Res<'_, Time<Virtual>>,
    (session, mut struck): (
        Option<Res<'_, crate::battle_pipeline::session::BattleSession>>,
        Option<ResMut<'_, super::struck::StruckBubbles>>,
    ),
) {
    let (Some(catalog), Some(tables), Some(mut runtime)) = (catalog, tables, runtime) else {
        events.clear();
        return;
    };
    let now = time.elapsed_secs_f64();
    let mut lines = LineAssembly::default();
    if !events.is_empty() {
        if let (Some(struck), Some(session)) = (struck.as_deref_mut(), session.as_deref()) {
            struck.rebuild(&session.frames.presented().bubbles);
        }
    }
    for event in events.read() {
        // An instant hit or a blow shows on the struck bubble's near surface, where the shot's
        // slug ends (review 2, A2-1).
        let (event, struck_radius) = match (struck.as_deref(), session.is_some()) {
            (Some(struck), true) => {
                super::struck::at_struck_surface(&event.0, |x, y| struck.radius_at(x, y))
            }
            _ => (event.0.clone(), None),
        };
        // A shot's travel is its own effect, never merged with another (plan WP11), cut where
        // the viewer stops seeing it.
        if let Some(shot) = travel_request(&event, &tables, &catalog) {
            let owner = event.owner();
            let clip = match fog.as_deref() {
                Some(fog) if fog.viewer() != Some(owner) => shot_clip(
                    shot.line
                        .unwrap_or(WorldLine::whole(shot.anchor, shot.anchor)),
                    catalog
                        .effect(shot.effect)
                        .map_or(ShotReach::default(), |effect| effect.shot_reach),
                    |x, y| fog.sees(x, y),
                ),
                _ => NO_CLIP,
            };
            if let Some(handle) = runtime.spawn(&catalog, &shot, now) {
                runtime.set_fog_clip(handle, clip);
            }
        }
        if let Some(ready) = spawn_request_struck(&event, struck_radius, &tables, &catalog)
            .and_then(|request| lines.push(request))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A shot along +x (map x) at map y = 5.5, from x = 1.5 to x = 9.5.
    fn shot() -> WorldLine {
        WorldLine::whole(Vec3::new(1.5, 0.5, 5.5), Vec3::new(9.5, 0.5, 5.5))
    }

    #[test]
    fn a_shot_seen_whole_is_not_cut() {
        assert_eq!(
            shot_clip(shot(), ShotReach::uniform(0.0), |_, _| true),
            NO_CLIP
        );
    }

    #[test]
    fn a_shot_out_of_the_fog_is_drawn_from_the_fog_edge_to_where_it_lands() {
        // The viewer sees map x >= 5 only: the muzzle is in the fog, the landing point seen.
        let clip = shot_clip(shot(), ShotReach::uniform(0.0), |x, _| x >= 5.0);
        let [x, z, min, max] = clip;
        assert!(
            (x - 1.0).abs() < 1e-6 && z.abs() < 1e-6,
            "cut along the shot: {clip:?}"
        );
        assert!(
            min >= 5.0,
            "nothing drawn over the unseen ground: cut at {min}"
        );
        assert!(min <= 5.0 + 0.15, "the seen part is kept: cut at {min}");
        assert!(max >= 9.5, "drawn to where it lands: {max}");
    }

    #[test]
    fn the_fog_beside_a_shot_cuts_it_too() {
        // Unseen cells just beside the path (map y < 5.3) left of x = 4: the shot's glow
        // would reach over them, so the cut lies beyond them.
        let clip = shot_clip(shot(), ShotReach::uniform(0.0), |x, y| y >= 5.3 || x >= 4.0);
        assert!(clip[2] >= 4.0, "cut at {}", clip[2]);
    }

    #[test]
    fn a_shot_along_a_fog_edge_within_its_reach_draws_nothing() {
        // Unseen ground 0.8 units beside the whole path (map y < 4.7): a shot reaching 1 unit
        // across draws nothing, one reaching 0.4 draws whole.
        let clip = shot_clip(shot(), ShotReach::uniform(1.0), |_, y| y >= 4.7);
        assert!(clip[2] >= 9.5, "cut past the landing point: {}", clip[2]);
        assert_eq!(
            shot_clip(shot(), ShotReach::uniform(0.4), |_, y| y >= 4.7),
            NO_CLIP
        );
    }

    #[test]
    fn a_shot_seen_only_where_it_lands_draws_at_most_its_end() {
        let clip = shot_clip(shot(), ShotReach::uniform(0.0), |x, _| x >= 9.45);
        assert!(clip[2] >= 9.45, "cut at {}", clip[2]);
    }

    #[test]
    fn a_vertical_shot_is_not_cut() {
        let line = WorldLine::whole(Vec3::new(2.0, 3.0, 2.0), Vec3::new(2.0, 0.5, 2.0));
        assert_eq!(
            shot_clip(line, ShotReach::uniform(0.0), |_, _| false),
            NO_CLIP
        );
    }
}
