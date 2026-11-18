//! The "every effect" part of `frame_budget`'s effect stress (plan
//! `2026-11-09-effect-visualization` §10.3 and WP14, Amendment 31): next to the splash stream of
//! [`super::effect_stress`], a stream of shots, blows and deaths, and a held set of areas,
//! tethers, spell missiles and unit looks, so that one measured run draws every kind of look the
//! vocabulary has.
//!
//! # Responsibility
//! - [`mix_sources`] finds, from the catalogs only, every weapon whose presentation block names
//!   a travel or an impact (with the event its delivery publishes, [`Delivery`]), every death
//!   look, every ability look (area, tether or missile) and every `Status` and `Mode` effect of
//!   the vocabulary.
//! - [`stream_mix`] sends, per second of `Time<Virtual>`, the configured number of shots (each
//!   from one live unit to another, round-robin over the weapons, through the production event
//!   wiring) and deaths (a `BubbleShatter` at a live unit, round-robin over the death looks); it
//!   keeps the configured number of areas, tethers and missiles on live units through the
//!   production `keep_views` and `keep_missiles` (in a set of held looks of its own, so the
//!   battle's own areas are untouched), landing each missile's impact when it is replaced; and
//!   it holds the configured number of unit looks, round-robin over every `Status` and `Mode`
//!   effect, each on a live unit, moving with it.
//!
//! # How it fits
//! Installed by [`super::effect_stress::install`] when `--effect-mix` is given. The splash stream,
//! budget, level of detail and pools are the production ones, so the run measures what a battle
//! with every look on screen costs. A held unit look is held through the runtime's own `hold`
//! with the unit's radius bound, as `client::effects::stance` holds it; its `Shell` layers (which
//! write the unit's bubble material, not effect instances) are not applied here, so the measure
//! covers the effect passes, not the bubble's material.
//!
//! # Invariants
//! - No effect, unit, weapon or ability is named here: every source comes from the catalogs.
//! - Steady state allocates nothing: the sources, held handles and view lists are built once
//!   when the catalogs load and reused every frame.
//! - Every decision is a function of the frame's presented snapshot and a sequence number, so
//!   two runs of one battle stream the same events.

use super::effect_stress::{mix as hash_mix, weapon_of};
use bevy::prelude::*;
use client::battle_pipeline::session::BattleSession;
use client::effects::areas::{keep_missiles, keep_views, lay_spell_impact, AreaLooks, Keep};
use client::effects::binding::BoundValues;
use client::effects::catalog::schema::{Binding, EventKind};
use client::effects::catalog::{EffectCatalog, EffectId, UnitEffectTables};
use client::effects::pool::EffectHandle;
use client::effects::runtime::{EffectRuntime, SpawnRequest};
use client::vfx::CombatVisualEvent;
use client::{CatalogResource, LabState};
use serde::{Deserialize, Serialize};
use sim_types::snapshot::{BubbleRenderInstance, CombatEventVisual};
use sim_types::{
    AbilitySource, AnimState, AreaView, EffectSource, LinkView, ShapeKind, SpellMissileView,
    StrikeEnds, UnitTypeId, WeaponKey, WeaponSlot, WeaponVisualKind,
};

/// Areas the mix holds by default (the plan's §10.3 figure).
pub const DEFAULT_AREAS: u32 = 20;

/// Unit looks the mix holds by default (the plan's §10.3 figure of 50 statuses).
pub const DEFAULT_HELD_LOOKS: u32 = 50;

/// Tethers the mix holds by default.
pub const DEFAULT_LINKS: u32 = 4;

/// Spell missiles the mix keeps in flight by default.
pub const DEFAULT_MISSILES: u32 = 2;

/// Radius of a held area, world units (a Psionic Storm's 1.5).
const AREA_RADIUS: f32 = 1.5;

/// Ticks a held area lasts in all, and the remainder it keeps publishing.
const AREA_TICKS: (u16, u16) = (61, 60);

/// Frames a spell missile flies before it lands and the next one is launched.
const MISSILE_FRAMES: u32 = 8;

/// World units a spell missile flies per frame.
const MISSILE_STEP: f32 = 0.1;

/// Radius of the arc a blow draws, world units (the simulation's melee events publish 0.5).
const BLOW_RADIUS: f32 = 0.5;

/// What the mix keeps on screen, from the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixConfig {
    /// Shots and blows per second of game time.
    pub shots_per_second: u32,
    /// Deaths per second of game time.
    pub deaths_per_second: u32,
    /// Areas held at once.
    pub areas: u32,
    /// Tethers held at once.
    pub links: u32,
    /// Spell missiles in flight at once.
    pub missiles: u32,
    /// Unit looks (statuses, stances, morphs, concealment) held at once.
    pub held_looks: u32,
}

impl MixConfig {
    /// The plan's mix at `rate` splashes per second: as many shots, a tenth as many deaths,
    /// and the default held counts.
    #[must_use]
    pub const fn at_rate(rate: u32) -> Self {
        Self {
            shots_per_second: rate,
            deaths_per_second: rate / 10,
            areas: DEFAULT_AREAS,
            links: DEFAULT_LINKS,
            missiles: DEFAULT_MISSILES,
            held_looks: DEFAULT_HELD_LOOKS,
        }
    }
}

/// The event a weapon's delivery publishes (`sim_core::world::presentation`): a projectile's
/// flight when it has a projectile speed, else an instant line, a beam or a blow by its visual
/// kind.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Delivery {
    /// `ProjectileVisual`, flying at this speed in world units per second.
    Projectile(f32),
    /// `HitscanTracer`.
    Tracer,
    /// `BeamVisual`, a lightning arc when the flag is set.
    Beam(bool),
    /// `MeleeSlashVisual`.
    Blow,
}

/// The delivery of `weapon`, or `None` for a weapon that publishes no delivery event.
#[must_use]
pub fn delivery(weapon: &sim_types::WeaponSpec) -> Option<Delivery> {
    delivery_of(
        weapon.projectile_speed.map(sim_types::SimScalar::to_f32),
        weapon.visual_kind,
    )
}

/// The delivery of a weapon with `projectile_speed` (world units per second) and `kind`.
#[must_use]
pub const fn delivery_of(
    projectile_speed: Option<f32>,
    kind: WeaponVisualKind,
) -> Option<Delivery> {
    if let Some(speed) = projectile_speed {
        return Some(Delivery::Projectile(speed));
    }
    match kind {
        WeaponVisualKind::Hitscan => Some(Delivery::Tracer),
        WeaponVisualKind::LaserBeam => Some(Delivery::Beam(false)),
        WeaponVisualKind::LightningArc => Some(Delivery::Beam(true)),
        WeaponVisualKind::MeleeSlash => Some(Delivery::Blow),
        WeaponVisualKind::Projectile | WeaponVisualKind::None => None,
    }
}

/// One weapon whose travel or impact the mix repeats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotSource {
    /// Archetype and weapon, as the simulation tags its events.
    pub source: EffectSource,
    /// The event it publishes.
    pub delivery: Delivery,
}

/// Everything the mix draws, found in the catalogs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MixSources {
    /// Weapons with a travel or an impact look.
    pub shots: Vec<ShotSource>,
    /// One unit type per death look.
    pub deaths: Vec<UnitTypeId>,
    /// Abilities whose look is an area.
    pub areas: Vec<AbilitySource>,
    /// Abilities whose look is a tether.
    pub links: Vec<AbilitySource>,
    /// Abilities whose look is a spell missile.
    pub missiles: Vec<AbilitySource>,
    /// Every `Status` and `Mode` effect of the vocabulary.
    pub held: Vec<EffectId>,
}

/// Every weapon key of `archetype`: each slot, in the default stance and in each mode.
fn weapon_keys(archetype: &sim_core::catalog::UnitArchetype) -> impl Iterator<Item = WeaponKey> {
    let modes = u8::try_from(archetype.modes.len()).unwrap_or(u8::MAX);
    [WeaponSlot::Primary, WeaponSlot::Ground, WeaponSlot::Air]
        .into_iter()
        .flat_map(move |slot| {
            std::iter::once(None)
                .chain((0..modes).map(Some))
                .map(move |mode| WeaponKey { slot, mode })
        })
}

/// Every source of the mix in `units`, `tables` and `effects`, in catalog order.
#[must_use]
pub fn mix_sources(
    units: &sim_core::catalog::UnitCatalog,
    tables: &UnitEffectTables,
    effects: &EffectCatalog,
) -> MixSources {
    let mut sources = MixSources::default();
    let mut deaths = Vec::new();
    let event_of = |id: EffectId| effects.effect(id).map(|effect| effect.event);
    for archetype in units.archetypes() {
        for key in weapon_keys(archetype) {
            let source = EffectSource {
                unit_type: archetype.unit_type_id,
                weapon: key,
            };
            if tables.travel(source).is_none() && tables.impact(source).is_none() {
                continue;
            }
            if let Some(delivery) = weapon_of(archetype, key).and_then(delivery) {
                sources.shots.push(ShotSource { source, delivery });
            }
        }
        if let Some(death) = tables.death(archetype.unit_type_id) {
            if !deaths.contains(&death) {
                deaths.push(death);
                sources.deaths.push(archetype.unit_type_id);
            }
        }
        for index in 0..archetype.abilities.len() {
            let Ok(ability) = u8::try_from(index) else {
                break;
            };
            let source = AbilitySource {
                unit_type: archetype.unit_type_id,
                ability,
            };
            match tables.ability(source).and_then(event_of) {
                Some(EventKind::Area) => sources.areas.push(source),
                Some(EventKind::Link) => sources.links.push(source),
                Some(EventKind::Missile) => sources.missiles.push(source),
                _ => {}
            }
        }
    }
    sources.held = (0..effects.effects.len())
        .filter_map(|index| u16::try_from(index).ok().map(EffectId))
        .filter(|id| matches!(event_of(*id), Some(EventKind::Status | EventKind::Mode)))
        .collect();
    sources
}

/// What the mix did, for the report.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MixSummary {
    /// Shots and blows per second asked for.
    pub shots_per_second: u32,
    /// Deaths per second asked for.
    pub deaths_per_second: u32,
    /// Weapons streamed: unit type, weapon key and delivery.
    pub shot_sources: Vec<String>,
    /// Death looks streamed, as the unit types that name them.
    pub death_sources: Vec<String>,
    /// Abilities whose areas, tethers and missiles are held.
    pub ability_sources: Vec<String>,
    /// Unit looks held, by name.
    pub held_looks: Vec<String>,
    /// Shots and blows sent.
    pub shots_sent: u64,
    /// Deaths sent.
    pub deaths_sent: u64,
    /// Areas, tethers and missiles held on the last frame.
    pub views_held: u32,
    /// Unit looks held on the last frame.
    pub looks_held: u32,
}

/// One held unit look: the effect, the unit it is on and its handle.
#[derive(Debug, Clone, Copy)]
struct HeldLook {
    effect: EffectId,
    unit: Option<sim_types::EntityId>,
    handle: Option<EffectHandle>,
}

/// The mix's state, a resource of the main world.
#[derive(Resource, Debug)]
pub struct EffectMix {
    /// What was asked for.
    pub config: MixConfig,
    /// The sources, once the catalogs are loaded.
    pub sources: Option<MixSources>,
    /// Shots owed to the stream, fractional.
    shots_owed: f64,
    /// Deaths owed to the stream, fractional.
    deaths_owed: f64,
    /// Shots sent.
    shots_sent: u64,
    /// Deaths sent.
    deaths_sent: u64,
    /// Frames the held set has been kept.
    frames: u32,
    /// The mix's own held areas, tethers and missiles.
    looks: AreaLooks,
    /// The areas of this frame (retained).
    areas: Vec<AreaView>,
    /// The tethers of this frame (retained).
    links: Vec<LinkView>,
    /// The missiles of the previous and of this frame (retained).
    missiles: [Vec<SpellMissileView>; 2],
    /// The held unit looks.
    held: Vec<HeldLook>,
    /// The report section.
    pub summary: MixSummary,
}

impl EffectMix {
    /// A mix that has sent nothing yet.
    #[must_use]
    pub fn new(config: MixConfig) -> Self {
        let capacity = |count: u32| usize::try_from(count).unwrap_or(0);
        Self {
            config,
            sources: None,
            shots_owed: 0.0,
            deaths_owed: 0.0,
            shots_sent: 0,
            deaths_sent: 0,
            frames: 0,
            looks: AreaLooks::default(),
            areas: Vec::with_capacity(capacity(config.areas)),
            links: Vec::with_capacity(capacity(config.links)),
            missiles: [
                Vec::with_capacity(capacity(config.missiles)),
                Vec::with_capacity(capacity(config.missiles)),
            ],
            held: Vec::with_capacity(capacity(config.held_looks)),
            summary: MixSummary {
                shots_per_second: config.shots_per_second,
                deaths_per_second: config.deaths_per_second,
                ..MixSummary::default()
            },
        }
    }

    /// Finds the sources once and writes their lines into the summary (allocates once).
    fn load(
        &mut self,
        units: &sim_core::catalog::UnitCatalog,
        tables: &UnitEffectTables,
        effects: &EffectCatalog,
    ) {
        let sources = mix_sources(units, tables, effects);
        self.summary.shot_sources = sources
            .shots
            .iter()
            .map(|shot| {
                format!(
                    "unit {}, {:?}, {:?}",
                    shot.source.unit_type.0, shot.source.weapon, shot.delivery
                )
            })
            .collect();
        self.summary.death_sources = sources
            .deaths
            .iter()
            .map(|unit| format!("unit {}", unit.0))
            .collect();
        self.summary.ability_sources = sources
            .areas
            .iter()
            .map(|source| ("area", source))
            .chain(sources.links.iter().map(|source| ("tether", source)))
            .chain(sources.missiles.iter().map(|source| ("missile", source)))
            .map(|(kind, source)| {
                format!(
                    "{kind}: unit {} ability {}",
                    source.unit_type.0, source.ability
                )
            })
            .collect();
        self.summary.held_looks = sources
            .held
            .iter()
            .filter_map(|id| effects.effect(*id).map(|effect| effect.name.clone()))
            .collect();
        let count = usize::try_from(self.config.held_looks).unwrap_or(0);
        self.held = (0..count)
            .filter_map(|index| sources.held.get(index % sources.held.len().max(1)))
            .map(|&effect| HeldLook {
                effect,
                unit: None,
                handle: None,
            })
            .collect();
        info!(
            "frame_budget: the effect mix streams {} shot, {} death sources, holds {} ability \
             looks and {} unit looks",
            sources.shots.len(),
            sources.deaths.len(),
            sources.areas.len() + sources.links.len() + sources.missiles.len(),
            sources.held.len()
        );
        self.sources = Some(sources);
    }
}

/// The live, standing unit bubbles of the presented snapshot.
fn live(bubble: &BubbleRenderInstance) -> bool {
    matches!(bubble.shape, ShapeKind::UnitSphere) && bubble.anim_state != AnimState::Die
}

/// The live unit at or after `start` in `bubbles`, wrapping, if any.
fn live_from(bubbles: &[BubbleRenderInstance], start: u64) -> Option<&BubbleRenderInstance> {
    let count = bubbles.len();
    if count == 0 {
        return None;
    }
    let start = usize::try_from(start % count as u64).unwrap_or(0);
    (0..count)
        .map(|step| &bubbles[(start + step) % count])
        .find(|bubble| live(bubble))
}

/// The event `shot` publishes from the unit `from` at the unit `to`.
#[must_use]
pub fn shot_event(
    shot: &ShotSource,
    from: &BubbleRenderInstance,
    to: &BubbleRenderInstance,
) -> CombatEventVisual {
    let ends = Some(StrikeEnds {
        attacker: from.entity_id,
        target: to.entity_id,
    });
    let owner = from.owner;
    let source = shot.source;
    let (start, end) = (from.world_pos, to.world_pos);
    match shot.delivery {
        Delivery::Projectile(speed) => {
            let delta = Vec3::from(end) - Vec3::from(start);
            let distance = delta.length().max(f32::EPSILON);
            let speed = speed.max(f32::EPSILON);
            CombatEventVisual::ProjectileVisual {
                pos: start,
                vel: (delta / distance * speed).to_array(),
                owner,
                duration_secs: Some(distance / speed),
                source,
                ends,
            }
        }
        Delivery::Tracer => CombatEventVisual::HitscanTracer {
            from: start,
            to: end,
            owner,
            source,
            ends,
        },
        Delivery::Beam(is_lightning) => CombatEventVisual::BeamVisual {
            from: start,
            to: end,
            is_lightning,
            owner,
            source,
            ends,
        },
        Delivery::Blow => CombatEventVisual::MeleeSlashVisual {
            pos: end,
            angle_rad: (end[1] - start[1]).atan2(end[0] - start[0]),
            radius: BLOW_RADIUS,
            owner,
            source,
            ends,
        },
    }
}

/// The catalogs and runtime the mix uses, once loaded.
#[derive(bevy::ecs::system::SystemParam)]
pub struct MixParams<'w> {
    /// The unit catalog.
    units: Option<Res<'w, CatalogResource>>,
    /// The units' effect choices.
    tables: Option<Res<'w, UnitEffectTables>>,
    /// The effect vocabulary.
    effects: Option<Res<'w, EffectCatalog>>,
    /// Live effects.
    runtime: Option<ResMut<'w, EffectRuntime>>,
}

/// Adds the mix to `app` (the stress mode's `install` calls it).
pub fn install(app: &mut App, config: MixConfig) {
    app.insert_resource(EffectMix::new(config)).add_systems(
        Update,
        stream_mix
            .in_set(client::battle_pipeline::BattleSet::Present)
            .before(client::effects::systems::spawn_effects_from_events),
    );
}

/// Sends this frame's shots and deaths and keeps the held set on the presented units.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn stream_mix(
    mut mix: ResMut<'_, EffectMix>,
    time: Res<'_, Time<Virtual>>,
    lab: Res<'_, LabState>,
    session: Res<'_, BattleSession>,
    params: MixParams<'_>,
    mut events: EventWriter<'_, CombatVisualEvent>,
) {
    let (Some(units), Some(tables), Some(effects), Some(mut runtime)) =
        (params.units, params.tables, params.effects, params.runtime)
    else {
        return;
    };
    if mix.sources.is_none() {
        mix.load(&units.0, &tables, &effects);
    }
    if lab.battle_paused || session.world().is_none() {
        return;
    }
    let mix = &mut *mix;
    let bubbles = session.frames.presented().bubbles.as_slice();
    if bubbles.is_empty() {
        return;
    }
    let delta = time.delta_secs_f64();
    // Moved out for the frame (no allocation) so the helpers can borrow the rest of the mix.
    let Some(sources) = mix.sources.take() else {
        return;
    };
    send_shots(mix, &sources, bubbles, delta, &mut events);
    let mut keep = Keep {
        catalog: &effects,
        tables: &tables,
        runtime: &mut runtime,
        now: time.elapsed_secs_f64(),
        delta,
    };
    keep_held_views(mix, &sources, bubbles, &mut keep);
    keep_unit_looks(mix, bubbles, &mut keep);
    mix.sources = Some(sources);
}

/// Sends the shots and deaths owed this frame.
fn send_shots(
    mix: &mut EffectMix,
    sources: &MixSources,
    bubbles: &[BubbleRenderInstance],
    delta: f64,
    events: &mut EventWriter<'_, CombatVisualEvent>,
) {
    mix.shots_owed += delta * f64::from(mix.config.shots_per_second);
    while mix.shots_owed >= 1.0 && !sources.shots.is_empty() {
        mix.shots_owed -= 1.0;
        let sequence = mix.shots_sent;
        mix.shots_sent += 1;
        let shot =
            &sources.shots[usize::try_from(sequence % sources.shots.len() as u64).unwrap_or(0)];
        let hash = hash_mix(sequence ^ 0x5348_4f54);
        let (Some(from), Some(to)) = (live_from(bubbles, hash), live_from(bubbles, hash >> 32))
        else {
            continue;
        };
        if from.entity_id == to.entity_id {
            continue;
        }
        events.send(CombatVisualEvent(shot_event(shot, from, to)));
    }
    mix.deaths_owed += delta * f64::from(mix.config.deaths_per_second);
    while mix.deaths_owed >= 1.0 && !sources.deaths.is_empty() {
        mix.deaths_owed -= 1.0;
        let sequence = mix.deaths_sent;
        mix.deaths_sent += 1;
        let unit_type =
            sources.deaths[usize::try_from(sequence % sources.deaths.len() as u64).unwrap_or(0)];
        let Some(at) = live_from(bubbles, hash_mix(sequence ^ 0x4445_4154)) else {
            continue;
        };
        events.send(CombatVisualEvent(CombatEventVisual::BubbleShatter {
            pos: at.world_pos,
            radius: at.radius,
            is_dome: false,
            owner: at.owner,
            unit_type,
        }));
    }
    mix.summary.shots_sent = mix.shots_sent;
    mix.summary.deaths_sent = mix.deaths_sent;
}

/// Keeps the areas, tethers and missiles on the presented units through the production code.
fn keep_held_views(
    mix: &mut EffectMix,
    sources: &MixSources,
    bubbles: &[BubbleRenderInstance],
    keep: &mut Keep<'_>,
) {
    mix.frames = mix.frames.wrapping_add(1);
    mix.areas.clear();
    if !sources.areas.is_empty() {
        for index in 0..mix.config.areas {
            let Some(at) = live_from(bubbles, hash_mix(u64::from(index) ^ 0x4152_4541)) else {
                break;
            };
            mix.areas.push(AreaView {
                id: index + 1,
                owner: at.owner,
                center: [at.world_pos[0], at.world_pos[1], at.ground_z],
                radius: AREA_RADIUS,
                remaining_ticks: AREA_TICKS.1,
                total_ticks: AREA_TICKS.0,
                source: sources
                    .areas
                    .get(usize::try_from(index).unwrap_or(0) % sources.areas.len())
                    .copied(),
            });
        }
    }
    mix.links.clear();
    if !sources.links.is_empty() {
        for index in 0..mix.config.links {
            let hash = hash_mix(u64::from(index) ^ 0x4c49_4e4b);
            let (Some(from), Some(to)) = (live_from(bubbles, hash), live_from(bubbles, hash >> 32))
            else {
                break;
            };
            if from.entity_id == to.entity_id {
                continue;
            }
            mix.links.push(LinkView {
                from: from.entity_id,
                to: to.entity_id,
                owner: from.owner,
                source: sources.links[usize::try_from(index).unwrap_or(0) % sources.links.len()],
            });
        }
    }
    keep_views(
        &mut mix.looks,
        (&mix.areas, &mix.links),
        bubbles,
        None,
        keep,
    );
    keep_flying_missiles(mix, sources, bubbles, keep);
    mix.summary.views_held =
        u32::try_from(mix.areas.len() + mix.links.len() + mix.missiles[1].len())
            .unwrap_or(u32::MAX);
}

/// Flies the missiles a step each frame; every [`MISSILE_FRAMES`] frames each lands its impact
/// and the next one is launched over another unit.
fn keep_flying_missiles(
    mix: &mut EffectMix,
    sources: &MixSources,
    bubbles: &[BubbleRenderInstance],
    keep: &mut Keep<'_>,
) {
    if sources.missiles.is_empty() {
        return;
    }
    let generation = mix.frames / MISSILE_FRAMES;
    let flown = MISSILE_STEP * f32::from(u8::try_from(mix.frames % MISSILE_FRAMES).unwrap_or(0));
    let [previous, current] = &mut mix.missiles;
    std::mem::swap(previous, current);
    current.clear();
    for index in 0..mix.config.missiles {
        let id = generation * mix.config.missiles + index;
        let Some(at) = live_from(bubbles, hash_mix(u64::from(id) ^ 0x4d49_5353)) else {
            break;
        };
        current.push(SpellMissileView {
            id,
            owner: at.owner,
            position: [at.world_pos[0] + flown, at.world_pos[1], at.ground_z],
            source: sources.missiles[usize::try_from(index).unwrap_or(0) % sources.missiles.len()],
        });
    }
    if mix.frames.is_multiple_of(MISSILE_FRAMES) {
        for missile in previous.iter() {
            lay_spell_impact(
                &CombatEventVisual::SpellImpact {
                    pos: missile.position,
                    missile: missile.id,
                    owner: missile.owner,
                    source: missile.source,
                },
                None,
                keep,
            );
        }
    }
    keep_missiles(&mut mix.looks, (current, current, 0.0), None, keep);
}

/// Holds each configured unit look on a live unit, moving it with the unit; a look whose unit
/// is gone is released and held again on another.
fn keep_unit_looks(mix: &mut EffectMix, bubbles: &[BubbleRenderInstance], keep: &mut Keep<'_>) {
    let mut held = 0;
    for (index, look) in mix.held.iter_mut().enumerate() {
        let unit = look.unit.and_then(|id| {
            bubbles
                .iter()
                .find(|bubble| bubble.entity_id == id && live(bubble))
        });
        if let (Some(bubble), Some(handle)) = (unit, look.handle) {
            if keep
                .runtime
                .move_anchor(handle, Vec3::from(bubble.world_pos))
            {
                held += 1;
                continue;
            }
        }
        if let Some(handle) = look.handle.take() {
            keep.runtime.release(handle, keep.now);
        }
        let seed = u64::try_from(index).unwrap_or(u64::MAX) ^ 0x4c4f_4f4b;
        let Some(bubble) = live_from(bubbles, hash_mix(seed)) else {
            look.unit = None;
            continue;
        };
        let mut bound = BoundValues::default();
        bound.set(Binding::UnitRadius, bubble.radius);
        look.unit = Some(bubble.entity_id);
        look.handle = keep.runtime.hold(
            keep.catalog,
            &SpawnRequest {
                effect: look.effect,
                anchor: Vec3::from(bubble.world_pos),
                bound,
                line: None,
                delay: 0.0,
            },
            keep.now,
            false,
        );
        held += u32::from(look.handle.is_some());
    }
    mix.summary.looks_held = held;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bubble(id: u32, x: f32, dying: bool) -> BubbleRenderInstance {
        BubbleRenderInstance {
            entity_id: sim_types::EntityId(id),
            shape: ShapeKind::UnitSphere,
            silhouette: sim_types::SilhouetteClass::HeavyHex,
            unit_type: UnitTypeId::default(),
            owner: sim_types::PlayerId(1),
            world_pos: [x, 0.0, 0.5],
            ground_z: 0.0,
            radius: 0.5,
            velocity: [0.0; 2],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: false,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: if dying {
                AnimState::Die
            } else {
                AnimState::Idle
            },
            attack_swing: None,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: false,
            concealment: sim_types::Concealment::None,
            speed_boosted: false,
            stance: sim_types::StanceView::default(),
            status: sim_types::StatusFlags::default(),
            barrier_ratio: 0.0,
        }
    }

    #[test]
    fn live_from_skips_dying_units_and_wraps() {
        let bubbles = [
            bubble(1, 0.0, false),
            bubble(2, 1.0, true),
            bubble(3, 2.0, false),
        ];
        assert_eq!(live_from(&bubbles, 1).map(|b| b.entity_id.0), Some(3));
        assert_eq!(live_from(&bubbles, 5).map(|b| b.entity_id.0), Some(3));
        assert_eq!(live_from(&bubbles, 3).map(|b| b.entity_id.0), Some(1));
        assert!(live_from(&[], 0).is_none());
        assert!(live_from(&[bubble(4, 0.0, true)], 0).is_none());
    }

    #[test]
    fn a_projectile_arrives_at_its_target_at_its_speed() -> Result<(), String> {
        let (from, to) = (bubble(1, 0.0, false), bubble(2, 4.0, false));
        let shot = ShotSource {
            source: EffectSource::default(),
            delivery: Delivery::Projectile(8.0),
        };
        let CombatEventVisual::ProjectileVisual {
            pos,
            vel,
            duration_secs: Some(duration),
            ends: Some(ends),
            ..
        } = shot_event(&shot, &from, &to)
        else {
            return Err("a projectile event".into());
        };
        let arrival = Vec3::from(pos) + Vec3::from(vel) * duration;
        assert!(arrival.distance(Vec3::from(to.world_pos)) < 1e-4);
        assert!((duration - 0.5).abs() < 1e-6);
        assert_eq!((ends.attacker.0, ends.target.0), (1, 2));
        Ok(())
    }

    #[test]
    fn each_delivery_publishes_its_own_event() -> Result<(), String> {
        let (from, to) = (bubble(1, 0.0, false), bubble(2, 3.0, false));
        let event = |delivery| {
            shot_event(
                &ShotSource {
                    source: EffectSource::default(),
                    delivery,
                },
                &from,
                &to,
            )
        };
        assert!(matches!(
            event(Delivery::Tracer),
            CombatEventVisual::HitscanTracer { .. }
        ));
        assert!(matches!(
            event(Delivery::Beam(true)),
            CombatEventVisual::BeamVisual {
                is_lightning: true,
                ..
            }
        ));
        let CombatEventVisual::MeleeSlashVisual { pos, angle_rad, .. } = event(Delivery::Blow)
        else {
            return Err("a blow".into());
        };
        assert_eq!(pos, to.world_pos);
        assert!(angle_rad.abs() < 1e-6, "the blow faces along +x");
        Ok(())
    }

    #[test]
    fn delivery_follows_projectile_speed_then_visual_kind() {
        assert_eq!(
            delivery_of(None, WeaponVisualKind::Hitscan),
            Some(Delivery::Tracer)
        );
        assert_eq!(
            delivery_of(None, WeaponVisualKind::LaserBeam),
            Some(Delivery::Beam(false))
        );
        assert_eq!(
            delivery_of(None, WeaponVisualKind::MeleeSlash),
            Some(Delivery::Blow)
        );
        assert_eq!(delivery_of(None, WeaponVisualKind::Projectile), None);
        assert_eq!(delivery_of(None, WeaponVisualKind::None), None);
        assert_eq!(
            delivery_of(Some(10.0), WeaponVisualKind::Projectile),
            Some(Delivery::Projectile(10.0))
        );
    }

    #[test]
    fn the_plans_mix_scales_with_the_splash_rate() {
        let config = MixConfig::at_rate(200);
        assert_eq!(
            (
                config.shots_per_second,
                config.deaths_per_second,
                config.areas,
                config.held_looks
            ),
            (200, 20, DEFAULT_AREAS, DEFAULT_HELD_LOOKS)
        );
    }
}
