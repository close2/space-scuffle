//! The effect stress mode of `frame_budget` (plan `2026-11-09-effect-visualization` §10.3,
//! WP6c): a steady stream of splash events over the measured battle, a capture of one stressed
//! frame and the share of the screen the effects cover on it.
//!
//! # Responsibility
//! - [`stress_sources`] finds every weapon of the unit catalog whose presentation block names a
//!   splash effect, with the zone radii and line the simulation would publish for it.
//! - [`spawn_stress_events`] sends `--effect-stress <rate>` splash events per second of
//!   `Time<Virtual>` while the battle runs, round-robin over those sources, each at a live unit
//!   of the presented snapshot chosen by a hash of its sequence number, on the plane the weapon
//!   strikes ([`strikes_plane`]: an air weapon's splash bursts among air units, a ground
//!   weapon's on the ground). The events go through
//!   the production wiring (`client::effects::systems::spawn_effects_from_events`), so budget,
//!   level of detail and pools act as in a battle.
//! - [`capture_stressed_frame`] takes a screenshot at the chosen tick and measures the effect
//!   coverage of that frame ([`effect_coverage`]).
//!
//! # How it fits
//! Added by [`super::run`] only when `--effect-stress` or `--capture` is given; a plain run is
//! unchanged. Statuses and areas are not streamed: the effect framework draws neither yet
//! (plan WP9, WP10).
//!
//! # Invariants
//! - No effect, unit or weapon is named here: the sources come from the catalogs.
//! - The stream allocates nothing per event; the coverage grid is allocated once, on the
//!   capture frame.
//! - The coverage is the union of the instances' projected footprints (cards as quads, decals
//!   as their ground rectangles): an upper bound of the pixels effects touch, since transparent
//!   texels inside a footprint count.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use client::battle_pipeline::session::BattleSession;
use client::effects::catalog::{EffectCatalog, UnitEffectTables};
use client::effects::instances::{
    DecalInstance, SpriteInstance, ORIENT_GROUND, ORIENT_STRETCHED, ORIENT_VERTICAL,
};
use client::effects::runtime::EffectRuntime;
use client::vfx::CombatVisualEvent;
use client::{CatalogResource, LabState, RtsCamera};
use serde::{Deserialize, Serialize};
use sim_core::catalog::UnitArchetype;
use sim_types::snapshot::CombatEventVisual;
use sim_types::{
    AnimState, EffectSource, LineView, PlayerId, ShapeKind, SplashShape, SplashSpec, WeaponKey,
    WeaponSlot, WeaponSpec,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

/// Side of one coverage cell in pixels.
pub const COVERAGE_CELL_PX: u32 = 4;

/// Height above its ground, world units, from which the stream counts a unit as airborne. Air
/// units fly several units up and ground units stand on their ground, so any value between
/// separates them; half a unit also tolerates a ground unit on a slope's edge.
pub const AIRBORNE_HEIGHT: f32 = 0.5;

/// One weapon whose splash the stream repeats.
#[derive(Debug, Clone, PartialEq)]
pub struct StressSource {
    /// Archetype and weapon, as the simulation tags its events.
    pub source: EffectSource,
    /// Name of the splash effect the unit's block chooses.
    pub effect: String,
    /// Radii of the full, middle and outermost zones, as the simulation publishes them.
    pub zones: [f32; 3],
    /// For a line splash: the first and the largest offset and the spacing, world units.
    pub line: Option<(f32, f32, f32)>,
}

/// The weapon of `archetype` in `key`'s slot, or the override of `key`'s mode.
fn weapon_of(archetype: &UnitArchetype, key: WeaponKey) -> Option<&WeaponSpec> {
    match (key.mode, key.slot) {
        (Some(mode), WeaponSlot::Air) => archetype
            .modes
            .get(usize::from(mode))?
            .air_weapon_override
            .as_ref()?
            .as_ref(),
        (Some(mode), _) => archetype
            .modes
            .get(usize::from(mode))?
            .ground_weapon_override
            .as_ref()?
            .as_ref(),
        (None, WeaponSlot::Primary) => archetype.weapon.as_ref(),
        (None, WeaponSlot::Ground) => archetype.ground_weapon.as_ref(),
        (None, WeaponSlot::Air) => archetype.air_weapon.as_ref(),
    }
}

/// The zone radii the simulation publishes for `splash`: the innermost, the middle one
/// (`zones[(n - 1) / 2]`) and the reach, as `sim_core`'s `splash_zone_radii` writes them.
#[must_use]
pub fn published_zones(splash: &SplashSpec) -> [f32; 3] {
    let radius = |index: usize| {
        splash
            .zones
            .get(index)
            .map_or(0.0, |zone| zone.radius.to_f32())
    };
    let last = splash.zones.len().saturating_sub(1);
    [radius(0), radius(last / 2), splash.reach().to_f32()]
}

/// The first and the largest offset and the spacing of a line splash, world units.
fn line_of(splash: &SplashSpec) -> Option<(f32, f32, f32)> {
    let SplashShape::Line(line) = &splash.shape else {
        return None;
    };
    let first = line.offsets.first()?.to_f32();
    let far = line.offsets.iter().copied().max()?.to_f32();
    let spacing = match line.offsets.as_slice() {
        [a, b, ..] => (*b - *a).abs().to_f32(),
        _ => 0.0,
    };
    Some((first, far, spacing))
}

/// Every weapon of `units` whose block names a splash effect in `tables`, in catalog order.
#[must_use]
pub fn stress_sources(
    units: &sim_core::catalog::UnitCatalog,
    tables: &UnitEffectTables,
    effects: &EffectCatalog,
) -> Vec<StressSource> {
    let mut sources = Vec::new();
    for archetype in units.archetypes() {
        let modes = u8::try_from(archetype.modes.len()).unwrap_or(u8::MAX);
        let keys = [WeaponSlot::Primary, WeaponSlot::Ground, WeaponSlot::Air]
            .into_iter()
            .flat_map(|slot| {
                std::iter::once(None)
                    .chain((0..modes).map(Some))
                    .map(move |mode| WeaponKey { slot, mode })
            });
        for key in keys {
            let source = EffectSource {
                unit_type: archetype.unit_type_id,
                weapon: key,
            };
            let (Some(effect), Some(splash)) = (
                tables.splash(source),
                weapon_of(archetype, key).and_then(|weapon| weapon.splash.as_ref()),
            ) else {
                continue;
            };
            let Some(effect) = effects.effect(effect) else {
                continue;
            };
            sources.push(StressSource {
                source,
                effect: effect.name.clone(),
                zones: published_zones(splash),
                line: line_of(splash),
            });
        }
    }
    sources
}

/// What `--effect-stress` and `--capture` ask for.
#[derive(Debug, Clone, PartialEq)]
pub struct StressConfig {
    /// Splash events per second of game time; 0 streams nothing.
    pub rate_per_second: u32,
    /// Screenshot of the stressed frame, if asked for.
    pub capture: Option<PathBuf>,
    /// Simulation tick of the captured frame.
    pub capture_tick: u32,
}

/// What the stress mode did, for the report.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StressSummary {
    /// Splash events per second asked for.
    pub rate_per_second: u32,
    /// The streamed sources: effect name and its zones (and line), one entry per weapon.
    pub sources: Vec<String>,
    /// Splash events sent.
    pub events_sent: u64,
    /// Effects the runtime spawned (stream and battle).
    pub effects_spawned: u64,
    /// Effects refused for a full instance pool.
    pub effects_refused: u64,
    /// Critical instances that found their pool full.
    pub critical_overflow: u64,
    /// Detail instances skipped for want of room.
    pub detail_skipped: u64,
    /// Instances skipped as too small on screen.
    pub lod_skipped: u64,
    /// Effects whose detail was retired early.
    pub retired: u64,
    /// The captured frame, if any.
    pub capture: Option<CaptureSummary>,
}

/// The stressed frame that was captured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureSummary {
    /// Screenshot file.
    pub path: PathBuf,
    /// Simulation tick of the frame.
    pub tick: u32,
    /// Live effects on that frame.
    pub live_effects: u32,
    /// Cards (sprites, rings, particles) drawn.
    pub cards: u32,
    /// Decals drawn.
    pub decals: u32,
    /// Coverage of the screen by effect footprints.
    pub coverage: Coverage,
}

/// How much of the screen effect footprints cover.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    /// Share of the screen inside at least one footprint, 0 to 1.
    pub covered_fraction: f64,
    /// Footprint area summed over every instance, as a multiple of the screen's area.
    pub overdraw: f64,
    /// Most footprints over one cell.
    pub max_layers: u32,
}

/// The stress mode's state, a resource of the main world.
#[derive(Resource, Debug)]
pub struct EffectStress {
    /// What was asked for.
    pub config: StressConfig,
    /// Sources found once the catalogs are loaded; `None` before.
    pub sources: Option<Vec<StressSource>>,
    /// Events owed to the stream, fractional.
    owed: f64,
    /// Events sent.
    pub sequence: u64,
    /// The captured frame, once taken.
    pub capture: Option<CaptureSummary>,
    /// Where the report section leaves the App.
    handle: StressHandle,
}

/// Shared slot through which the stress mode's report section leaves the App, which drops
/// its resources when it exits.
#[derive(Debug, Clone, Default)]
pub struct StressHandle(Arc<Mutex<StressSummary>>);

impl StressHandle {
    /// The section as last published.
    #[must_use]
    pub fn snapshot(&self) -> StressSummary {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Runs `update` on the shared section.
    fn update(&self, update: impl FnOnce(&mut StressSummary)) {
        update(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

impl EffectStress {
    /// A stress mode that has sent nothing yet.
    #[must_use]
    pub fn new(config: StressConfig) -> Self {
        let handle = StressHandle::default();
        handle.update(|summary| summary.rate_per_second = config.rate_per_second);
        Self {
            config,
            sources: None,
            owed: 0.0,
            sequence: 0,
            capture: None,
            handle,
        }
    }

    /// Publishes the sources, once found (allocates once).
    fn publish_sources(&self, sources: &[StressSource]) {
        let lines = sources
            .iter()
            .map(|source| {
                format!(
                    "{} (unit {}, {:?}, zones {:?}, line {:?})",
                    source.effect,
                    source.source.unit_type.0,
                    source.source.weapon,
                    source.zones,
                    source.line
                )
            })
            .collect();
        self.handle.update(|summary| summary.sources = lines);
    }

    /// Publishes the counters (no allocation).
    fn publish_counters(&self, runtime: Option<&EffectRuntime>) {
        let stats = runtime.map(|runtime| runtime.stats).unwrap_or_default();
        self.handle.update(|summary| {
            summary.events_sent = self.sequence;
            summary.effects_spawned = stats.spawned;
            summary.effects_refused = stats.refused;
            summary.critical_overflow = stats.critical_overflow;
            summary.detail_skipped = stats.detail_skipped;
            summary.lod_skipped = stats.lod_skipped;
            summary.retired = stats.retired;
        });
    }
}

/// Adds the stress mode to `app` and returns the handle its report section leaves through.
pub fn install(app: &mut App, config: StressConfig) -> StressHandle {
    let stress = EffectStress::new(config);
    let handle = stress.handle.clone();
    app.insert_resource(stress)
        .add_systems(
            Update,
            spawn_stress_events
                .in_set(client::battle_pipeline::BattleSet::Present)
                .before(client::effects::systems::spawn_effects_from_events),
        )
        .add_systems(Last, capture_stressed_frame);
    handle
}

/// Whether a unit `height` world units above its ground lies on the plane `slot`'s weapon
/// strikes: an air weapon only airborne units, a ground weapon only units on the ground, a
/// primary weapon (which may strike both) any unit.
#[must_use]
pub fn strikes_plane(slot: WeaponSlot, height: f32) -> bool {
    let airborne = height > AIRBORNE_HEIGHT;
    match slot {
        WeaponSlot::Air => airborne,
        WeaponSlot::Ground => !airborne,
        WeaponSlot::Primary => true,
    }
}

/// A 32-bit mix of `value` (`SplitMix64`'s finaliser), for choosing units and angles.
#[must_use]
pub const fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// How many search points one splash of `source` has: one for a circle, every point of a line
/// as the simulation emits them (one event per point on the same tick).
#[must_use]
pub fn stress_points(source: &StressSource) -> u8 {
    match source.line {
        Some((first, far, spacing)) if spacing > f32::EPSILON => {
            let steps = ((far - first) / spacing).floor().clamp(0.0, 63.0);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a whole number in 0..=63"
            )]
            let steps = steps as u8;
            steps + 1
        }
        _ => 1,
    }
}

/// The splash event of search point `index` of `source` at `at` (`[x, y, altitude]`), its
/// line along `angle` radians.
#[must_use]
pub fn stress_event(
    source: &StressSource,
    at: [f32; 3],
    angle: f32,
    index: u8,
) -> CombatEventVisual {
    let (sin, cos) = angle.sin_cos();
    let along = |distance: f32| [at[0] + cos * distance, at[1] + sin * distance, at[2]];
    let (center, line) = match source.line {
        Some((first, far, spacing)) => (
            along(first + spacing * f32::from(index)),
            Some(LineView {
                from: at,
                to: along(far),
                spacing,
                index,
                count: stress_points(source),
            }),
        ),
        None => (at, None),
    };
    CombatEventVisual::SplashVisual {
        center,
        zones: source.zones,
        line,
        source: source.source,
        owner: PlayerId(0),
    }
}

/// The catalogs the sources are found in, once loaded.
#[derive(bevy::ecs::system::SystemParam)]
struct StressCatalogs<'w> {
    /// The unit catalog.
    units: Option<Res<'w, CatalogResource>>,
    /// The units' effect choices.
    tables: Option<Res<'w, UnitEffectTables>>,
    /// The effect vocabulary.
    effects: Option<Res<'w, EffectCatalog>>,
}

/// Sends the events the stream owes this frame while the battle runs.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn spawn_stress_events(
    mut stress: ResMut<'_, EffectStress>,
    time: Res<'_, Time<Virtual>>,
    lab: Res<'_, LabState>,
    session: Res<'_, BattleSession>,
    catalogs: StressCatalogs<'_>,
    mut events: EventWriter<'_, CombatVisualEvent>,
) {
    if stress.sources.is_none() {
        if let (Some(units), Some(tables), Some(effects)) =
            (catalogs.units, catalogs.tables, catalogs.effects)
        {
            let sources = stress_sources(&units.0, &tables, &effects);
            info!(
                "frame_budget: the effect stress streams {} splash sources",
                sources.len()
            );
            stress.publish_sources(&sources);
            stress.sources = Some(sources);
        }
    }
    if lab.battle_paused || session.world().is_none() || stress.config.rate_per_second == 0 {
        return;
    }
    let stress = &mut *stress;
    let Some(sources) = stress.sources.as_deref().filter(|list| !list.is_empty()) else {
        return;
    };
    let bubbles = &session.frames.presented().bubbles;
    let live = |index: usize, slot: WeaponSlot| {
        bubbles.get(index).filter(|bubble| {
            matches!(bubble.shape, ShapeKind::UnitSphere)
                && bubble.anim_state != AnimState::Die
                && strikes_plane(slot, bubble.world_pos[2] - bubble.ground_z)
        })
    };
    if bubbles.is_empty() {
        return;
    }
    stress.owed += time.delta_secs_f64() * f64::from(stress.config.rate_per_second);
    while stress.owed >= 1.0 {
        stress.owed -= 1.0;
        let sequence = stress.sequence;
        stress.sequence += 1;
        let hash = mix(sequence);
        let source = &sources[usize::try_from(sequence % sources.len() as u64).unwrap_or(0)];
        let start = usize::try_from(hash % bubbles.len() as u64).unwrap_or(0);
        // The first live unit on the weapon's plane at or after the hashed index; a frame
        // without one skips this source's event.
        let Some(bubble) = (0..bubbles.len())
            .map(|step| (start + step) % bubbles.len())
            .find_map(|index| live(index, source.source.weapon.slot))
        else {
            continue;
        };
        #[allow(
            clippy::cast_precision_loss,
            reason = "16 bits of the hash become an angle"
        )]
        let angle = ((hash >> 32) & 0xFFFF) as f32 / 65_536.0 * std::f32::consts::TAU;
        for index in 0..stress_points(source) {
            events.send(CombatVisualEvent(stress_event(
                source,
                bubble.world_pos,
                angle,
                index,
            )));
        }
    }
}

/// On the first frame at or after the capture tick: a screenshot and the frame's coverage.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn capture_stressed_frame(
    mut commands: Commands<'_, '_>,
    mut stress: ResMut<'_, EffectStress>,
    session: Res<'_, BattleSession>,
    runtime: Option<Res<'_, EffectRuntime>>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform), With<RtsCamera>>,
) {
    stress.publish_counters(runtime.as_deref());
    let Some(path) = stress.config.capture.clone() else {
        return;
    };
    let Some(tick) = session.world().map(|world| world.current_tick) else {
        return;
    };
    if stress.capture.is_some() || tick < stress.config.capture_tick {
        return;
    }
    let Ok((camera, transform)) = cameras.get_single() else {
        return;
    };
    let viewport = camera
        .physical_viewport_size()
        .unwrap_or(UVec2::new(1280, 720));
    let (live_effects, cards, decals, coverage) =
        runtime
            .as_deref()
            .map_or((0, 0, 0, Coverage::default()), |runtime| {
                let project = |point: Vec3| camera.world_to_viewport(transform, point).ok();
                let view = ViewAxes::of(transform);
                (
                    runtime.pool.live_total(),
                    u32::try_from(runtime.frame.sorted.len()).unwrap_or(u32::MAX),
                    u32::try_from(runtime.frame.decals.len()).unwrap_or(u32::MAX),
                    effect_coverage(
                        &runtime.frame.sorted,
                        &runtime.frame.decals,
                        view,
                        viewport,
                        project,
                    ),
                )
            });
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path.clone()));
    info!("frame_budget: captured the stressed frame at tick {tick}: {coverage:?}");
    let capture = CaptureSummary {
        path,
        tick,
        live_effects,
        cards,
        decals,
        coverage,
    };
    let published = capture.clone();
    stress
        .handle
        .update(|summary| summary.capture = Some(published));
    stress.capture = Some(capture);
}

/// The camera's right and up vectors in world space, for camera-facing cards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewAxes {
    /// Screen right.
    pub right: Vec3,
    /// Screen up.
    pub up: Vec3,
    /// View direction.
    pub forward: Vec3,
}

impl ViewAxes {
    /// The axes of a camera at `transform`.
    #[must_use]
    pub fn of(transform: &GlobalTransform) -> Self {
        Self {
            right: transform.right().as_vec3(),
            up: transform.up().as_vec3(),
            forward: transform.forward().as_vec3(),
        }
    }
}

/// The two half-edge vectors of `card`'s quad in world space.
fn card_edges(card: &SpriteInstance, view: ViewAxes) -> (Vec3, Vec3) {
    let half = card.half_size;
    let axis = Vec3::from_array(card.axis);
    match card.orient {
        ORIENT_GROUND => (Vec3::X * half, Vec3::Z * half),
        ORIENT_VERTICAL => (axis * half, Vec3::Y * half),
        ORIENT_STRETCHED => {
            let across = view.forward.cross(axis).normalize_or_zero();
            (axis * half * card.stretch, across * half)
        }
        _ => (view.right * half, view.up * half),
    }
}

/// The two half-edge vectors of `decal`'s ground rectangle.
fn decal_edges(decal: &DecalInstance) -> (Vec3, Vec3) {
    let (sin, cos) = decal.yaw.sin_cos();
    let x = Vec3::new(cos, 0.0, -sin) * decal.half_extents[0];
    let z = Vec3::new(sin, 0.0, cos) * decal.half_extents[2];
    (x, z)
}

/// Coverage of the screen (`viewport` pixels) by the footprints of `cards` and `decals`,
/// rasterised on cells of [`COVERAGE_CELL_PX`]; `project` maps world points to pixels.
#[must_use]
pub fn effect_coverage(
    cards: &[SpriteInstance],
    decals: &[DecalInstance],
    view: ViewAxes,
    viewport: UVec2,
    project: impl Fn(Vec3) -> Option<Vec2>,
) -> Coverage {
    let columns = viewport.x.div_ceil(COVERAGE_CELL_PX).max(1);
    let rows = viewport.y.div_ceil(COVERAGE_CELL_PX).max(1);
    let mut layers = vec![0_u32; (columns * rows) as usize];
    let quads = cards
        .iter()
        .filter(|card| card.intensity > 0.0)
        .map(|card| (Vec3::from_array(card.center), card_edges(card, view)))
        .chain(
            decals
                .iter()
                .filter(|decal| decal.intensity > 0.0)
                .map(|decal| (Vec3::from_array(decal.center), decal_edges(decal))),
        );
    for (center, (a, b)) in quads {
        let corners = [
            center - a - b,
            center + a - b,
            center + a + b,
            center - a + b,
        ];
        let Some(screen) = corners
            .iter()
            .map(|corner| project(*corner))
            .collect::<Option<Vec<Vec2>>>()
        else {
            continue;
        };
        fill_quad(&mut layers, columns, rows, &screen);
    }
    let total = f64::from(columns * rows);
    let covered = layers.iter().filter(|count| **count > 0).count();
    #[allow(clippy::cast_precision_loss, reason = "cell counts are far below 2^52")]
    Coverage {
        covered_fraction: covered as f64 / total,
        overdraw: layers.iter().map(|count| f64::from(*count)).sum::<f64>() / total,
        max_layers: layers.iter().copied().max().unwrap_or(0),
    }
}

/// Adds one layer to every cell whose centre lies inside the convex quad `screen` (pixels).
#[allow(
    clippy::cast_precision_loss,
    reason = "cell indices and the grid size are far below 2^23"
)]
fn fill_quad(layers: &mut [u32], columns: u32, rows: u32, screen: &[Vec2]) {
    let cell = COVERAGE_CELL_PX as f32;
    let (min, max) = screen.iter().fold(
        (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
        |(min, max), point| (min.min(*point), max.max(*point)),
    );
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to the grid before conversion"
    )]
    let span = |low: f32, high: f32, limit: u32| {
        let first = (low / cell).floor().clamp(0.0, limit as f32) as u32;
        let last = (high / cell).ceil().clamp(0.0, limit as f32) as u32;
        first..last
    };
    let signed_area = |p: Vec2, q: Vec2, r: Vec2| (q - p).perp_dot(r - p);
    for row in span(min.y, max.y, rows) {
        for column in span(min.x, max.x, columns) {
            let point = Vec2::new((column as f32 + 0.5) * cell, (row as f32 + 0.5) * cell);
            let sides: [f32; 4] =
                std::array::from_fn(|i| signed_area(screen[i], screen[(i + 1) % 4], point));
            let inside =
                sides.iter().all(|side| *side >= 0.0) || sides.iter().all(|side| *side <= 0.0);
            if inside {
                layers[(row * columns + column) as usize] += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(
        clippy::unnecessary_wraps,
        reason = "the projection's signature is fallible"
    )]
    fn identity_project(point: Vec3) -> Option<Vec2> {
        Some(Vec2::new(point.x, point.z))
    }

    #[test]
    fn coverage_counts_union_and_overdraw() {
        let ground = |x: f32, half: f32| SpriteInstance {
            center: [x, 0.0, 48.0],
            half_size: half,
            intensity: 1.0,
            orient: ORIENT_GROUND,
            ..SpriteInstance::default()
        };
        let view = ViewAxes {
            right: Vec3::X,
            up: Vec3::Z,
            forward: Vec3::NEG_Y,
        };
        // Two 40x40 px squares overlapping by half on a 200x100 screen, edges between cell centres.
        let cards = [ground(40.0, 20.0), ground(60.0, 20.0)];
        let coverage = effect_coverage(&cards, &[], view, UVec2::new(200, 100), identity_project);
        let screen = 200.0 * 100.0;
        assert!((coverage.covered_fraction - 60.0 * 40.0 / screen).abs() < 1e-9);
        assert!((coverage.overdraw - 2.0 * 40.0 * 40.0 / screen).abs() < 1e-9);
        assert_eq!(coverage.max_layers, 2);
        let faded = [SpriteInstance {
            intensity: 0.0,
            ..cards[0]
        }];
        let none = effect_coverage(&faded, &[], view, UVec2::new(200, 100), identity_project);
        assert_eq!(none, Coverage::default());
    }

    #[test]
    fn a_decal_covers_its_rotated_ground_rectangle() {
        let decal = DecalInstance {
            center: [48.0, 0.0, 48.0],
            half_extents: [20.0, 1.0, 12.0],
            yaw: std::f32::consts::FRAC_PI_2,
            intensity: 1.0,
            ..DecalInstance::default()
        };
        let view = ViewAxes {
            right: Vec3::X,
            up: Vec3::Z,
            forward: Vec3::NEG_Y,
        };
        let coverage = effect_coverage(&[], &[decal], view, UVec2::new(100, 100), identity_project);
        assert!((coverage.covered_fraction - 40.0 * 24.0 / 10_000.0).abs() < 1e-9);
    }

    #[test]
    fn a_stress_line_starts_at_its_first_offset() -> Result<(), String> {
        let source = StressSource {
            source: EffectSource::default(),
            effect: "line".into(),
            zones: [0.5, 0.5, 0.5],
            line: Some((1.0, 9.0, 1.0)),
        };
        assert_eq!(stress_points(&source), 9, "points 1 to 9, one apart");
        let event = stress_event(&source, [10.0, 20.0, 0.0], 0.0, 0);
        let CombatEventVisual::SplashVisual { center, line, .. } = event else {
            return Err(format!("a stress event is a splash: {event:?}"));
        };
        assert_eq!(center, [11.0, 20.0, 0.0]);
        assert_eq!(
            line.map(|line| (line.index, line.from, line.to)),
            Some((0, [10.0, 20.0, 0.0], [19.0, 20.0, 0.0]))
        );
        Ok(())
    }

    #[test]
    fn an_air_weapon_strikes_only_airborne_units() {
        assert!(strikes_plane(WeaponSlot::Air, 3.0));
        assert!(!strikes_plane(WeaponSlot::Air, 0.0));
        assert!(strikes_plane(WeaponSlot::Ground, 0.1));
        assert!(!strikes_plane(WeaponSlot::Ground, 3.0));
        assert!(strikes_plane(WeaponSlot::Primary, 0.0));
        assert!(strikes_plane(WeaponSlot::Primary, 3.0));
    }

    #[test]
    fn the_hash_spreads_its_inputs() {
        assert_ne!(mix(0), mix(1));
        assert_ne!(mix(1) % 400, mix(2) % 400);
    }
}
