//! Concealment, stance and status looks held on units: burrowing, burrowed, cloaked, detected,
//! mode switches and anchored stances (plan `2026-11-09-effect-visualization` WP8, §5.5, D-5,
//! D-6), and stimmed, slowed, charging and barrier-shielded units (WP9).
//!
//! # Responsibility
//! Owns [`StanceTrack`], the client-local presentation state of one live bubble, and
//! [`present_stances`], which keeps every unit's looks held in the [`EffectRuntime`] and writes
//! the `Shell` layers of those looks into the unit's bubble material each frame:
//! - while a mode switch is channelled, its `enter`, `leave` or `transition` effect (the
//!   *morph*), whose progress follows the published channel ([`channel_progress`]) and, when
//!   the effect's own `span` is longer than the channel, finishes after the channel has ended
//!   (the client-local settle of D-6, [`settle_progress`]);
//! - while a unit stands in a mode, its `stance` effect (burrowed: `burrow_mound`, anchored:
//!   the unit's own);
//! - while a unit is concealed, its *cover*: the cloak of a unit its viewer owns, the shimmer of
//!   a cloaked enemy the viewer does not detect, or the mark of a concealed enemy it detects;
//! - while a status is published on a unit ([`status_shown`]), that status's look
//!   ([`UnitEffectTables::status`]), which may lay another effect at the unit's foot at a beat of
//!   its own (a charge's dust, [`emit_due`]) and spawn one on a published hit (a barrier's absorb
//!   flash, [`hit_due`]).
//!
//! # How it fits
//! The simulation publishes facts only (`concealment`, `stance` with the channel's remaining
//! and nominal ticks); the unit's presentation block, with the vocabulary's defaults, names the
//! effects ([`UnitEffectTables::mode`], [`UnitEffectTables::concealment`]); the battle fog
//! decides what the viewer may see ([`Presence`]). This module only keeps the looks in step
//! with those three and writes nothing back.
//!
//! # Fog (D-5)
//! A unit the viewer may not see ([`Presence::Hidden`]) holds no look at all, so nothing is
//! anchored on an undetected burrowed enemy. An undetected cloaked enemy
//! ([`Presence::Shimmer`]) holds only its shimmer, whose layers draw no colour (a distortion of
//! what lies behind), and its bubble is not drawn ([`crate::battle_hud::fog`]). Stances, morphs
//! and marks are held only on a unit drawn as it is ([`Presence::Shown`]), and so are status
//! looks and what they lay or spawn: a status shows nothing of a unit the viewer does not see.
//!
//! # Invariants
//! - The simulation keeps no presentation state: every value here is derived from the
//!   published records and `Time<Virtual>`, or is this module's own (D-6, charter §3.2).
//! - Every look held on a bubble is released when the bubble's [`StanceTrack`] is removed
//!   (its entity despawned), so no held effect outlives its unit.
//! - A frame on which no look changes writes no material and allocates nothing.

use super::binding::BoundValues;
use super::catalog::schema::Binding;
use super::catalog::{
    status_index, Block, EffectCatalog, EffectId, ShellLook, UnitEffectTables, STATUS_ORDER,
};
use super::pool::{EffectHandle, EffectInstance};
use super::runtime::{layer_clock, layer_strength, EffectRuntime, SpawnRequest};
use crate::battle_pipeline::fog::{presence_of, BattleFog, Presence};
use crate::bubble_shader::FlipbookRegistry;
use crate::bubble_shader::{
    sync_bubble_shell, BubbleDomeMaterial, BubbleInstance, ShellUniformGpu,
};
use crate::scene_controller::LiveBattleBubble;
use bevy::ecs::component::ComponentId;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::render::batching::NoAutomaticBatching;
use sim_types::{
    BubbleRenderInstance, Concealment, ModeTransitionView, ShapeKind, StatusFlags, StatusKind,
};

/// Seconds of one simulation tick.
const TICK_SECONDS: f32 = 1.0 / sim_types::TICKS_PER_SECOND as f32;

/// Progress of a published mode switch: `clamp((nominal - remaining) / nominal, 0, 1)` (plan
/// D-1). While the channel's random extra runs (`remaining` above `nominal`) it holds at 0, so
/// the extra reads as a short hold before the morph begins, as the game's `RandomDelay` does.
/// A switch with no nominal length is complete.
#[must_use]
pub fn channel_progress(transition: &ModeTransitionView) -> f32 {
    if transition.nominal_ticks == 0 {
        return 1.0;
    }
    let nominal = f32::from(transition.nominal_ticks);
    ((nominal - f32::from(transition.remaining_ticks)) / nominal).clamp(0.0, 1.0)
}

/// Seconds a morph takes on screen for a channel of `channel` seconds: the effect's own `span`
/// when it is longer (D-6), else the channel.
#[must_use]
pub fn morph_length(channel: f32, span: Option<f32>) -> f32 {
    span.map_or(channel, |span| span.max(channel))
        .max(TICK_SECONDS)
}

/// The morph's presented progress, 0 to 1, while the channel runs: the channel's progress
/// scaled so the morph plays at its own pace over `length` seconds; at the channel's end it has
/// reached `channel / length`.
#[must_use]
pub fn presented_progress(channel_progress: f32, channel: f32, length: f32) -> f32 {
    (channel_progress * channel / length.max(TICK_SECONDS)).clamp(0.0, 1.0)
}

/// The morph's progress `since` seconds after its channel ended at progress `from`: it goes
/// on at its own pace (1 over `length` seconds) until it is complete (D-6).
#[must_use]
pub fn settle_progress(from: f32, since: f32, length: f32) -> f32 {
    (from + since.max(0.0) / length.max(TICK_SECONDS)).clamp(0.0, 1.0)
}

/// The cover look of a unit concealed as `concealment` that the viewer sees as `presence`; `own`
/// says whether the viewer owns it (or no fog is kept). `None` when it has none.
#[must_use]
pub fn cover_effect(
    tables: &UnitEffectTables,
    presence: Presence,
    own: bool,
    concealment: Concealment,
) -> Option<EffectId> {
    let looks = tables.concealment();
    // A burrowed unit shown at all shows it by its stance (sunk in its mound), to its owner and
    // to an enemy that detects it alike, as the game draws a detected burrowed unit.
    match (presence, concealment) {
        (Presence::Hidden, _) | (Presence::Shown, Concealment::None | Concealment::Burrowed) => {
            None
        }
        (Presence::Shimmer, _) => looks.cloaked_enemy,
        (Presence::Shown, Concealment::Cloaked) if own => looks.cloaked_own,
        (Presence::Shown, Concealment::Cloaked) => looks.detected,
    }
}

/// The morph effect of `bubble`'s mode switch `transition`: entering a mode, that mode's
/// `enter` (or `transition`); leaving its current mode, that mode's `leave` (or `transition`).
#[must_use]
pub fn morph_effect(
    tables: &UnitEffectTables,
    bubble: &BubbleRenderInstance,
    transition: &ModeTransitionView,
) -> Option<EffectId> {
    if let Some(mode) = transition.entering {
        let looks = tables.mode(bubble.unit_type, mode)?;
        return looks.enter.or(looks.transition);
    }
    let looks = tables.mode(bubble.unit_type, bubble.stance.mode?)?;
    looks.leave.or(looks.transition)
}

/// The stance effect of `bubble` standing in its current mode: the mode's `stance`; for a
/// burrowed unit whose mode its viewer's catalog does not list, the burrowed default.
#[must_use]
pub fn stance_effect(tables: &UnitEffectTables, bubble: &BubbleRenderInstance) -> Option<EffectId> {
    let own = bubble
        .stance
        .mode
        .and_then(|mode| tables.mode(bubble.unit_type, mode))
        .and_then(|looks| looks.stance);
    own.or_else(|| {
        (bubble.concealment == Concealment::Burrowed)
            .then_some(tables.concealment().burrowed)
            .flatten()
    })
}

/// Whether `bubble` shows status `kind`: the simulation publishes it, and a barrier still has
/// absorption left (its shell is drawn only while `barrier_ratio > 0`, plan WP9).
#[must_use]
pub fn status_shown(bubble: &BubbleRenderInstance, kind: StatusKind) -> bool {
    bubble.status.has(kind) && (kind != StatusKind::Barrier || bubble.barrier_ratio > 0.0)
}

/// Whether the simulation published a hit on status `kind` this tick: only a barrier is hit
/// (`StatusFlags::barrier_hit`, damage it absorbed).
#[must_use]
pub const fn status_hit(flags: StatusFlags, kind: StatusKind) -> bool {
    matches!(kind, StatusKind::Barrier) && flags.barrier_hit
}

/// Whether a hit seen now (`hit`, with `was_hit` on the previous frame) spawns its effect: on
/// the frame a hit is first seen, and while hits go on at most once every `every` seconds
/// since the last one spawned at `last` (seconds of `Time<Virtual>`).
#[must_use]
pub fn hit_due(hit: bool, was_hit: bool, now: f64, last: Option<f64>, every: f32) -> bool {
    if !hit {
        return false;
    }
    !was_hit || last.is_none_or(|last| now - last >= f64::from(every))
}

/// Whether an effect laid every `every` seconds at a unit's own `phase` (a share of `every`, so
/// a group's dust does not puff in step) is due in the frame from `now - delta` to `now`.
#[must_use]
pub fn emit_due(now: f64, delta: f64, every: f32, phase: f32) -> bool {
    let every = f64::from(every.max(1e-3));
    let offset = f64::from(phase) * every;
    let beat = |time: f64| ((time + offset) / every).floor();
    beat(now) > beat(now - delta.max(0.0))
}

/// Speed in world units per second below which a published velocity counts as standing still.
const MOVING_SPEED: f32 = 1e-3;

/// A look held on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldLook {
    /// The effect.
    pub effect: EffectId,
    /// Its live instance.
    pub handle: EffectHandle,
}

/// A mode switch's morph held on a unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Morph {
    /// The look.
    pub look: HeldLook,
    /// The mode the switch enters (`None`: the default stance).
    pub entering: Option<u8>,
    /// Seconds of the switch's nominal channel.
    pub channel: f32,
    /// Seconds the morph takes on screen ([`morph_length`]).
    pub length: f32,
    /// Presented progress, 0 to 1.
    pub progress: f32,
    /// Once the channel has ended into the mode it entered: when, and at which progress.
    pub settling: Option<(f64, f32)>,
}

/// The client-local presentation state of one live bubble (D-6).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
#[component(on_remove = release_held_looks)]
pub struct StanceTrack {
    /// Whether the unit was shown as it is on the previous frame (a unit first shown in a
    /// stance is shown settled in it, its one-shot layers not replayed).
    pub seen: bool,
    /// The stance look.
    pub stance: Option<HeldLook>,
    /// The morph of a mode switch under way or settling.
    pub morph: Option<Morph>,
    /// The concealment look.
    pub cover: Option<HeldLook>,
    /// Whether the next stance look is held settled (a switch was cancelled).
    pub settle_next: bool,
    /// Looks released lately, still eroding out: their held `Shell` layers fade with them
    /// (a cloak's veil, a burrow's shadow), so a released look never pops.
    pub releasing: [Option<HeldLook>; RELEASING_SLOTS],
    /// The slot of `releasing` the next released look takes.
    pub next_releasing: usize,
    /// Whether the looks sink the unit as one showing its own mode clip. It follows the unit's
    /// mode or switch, and holds while a morph settles after the switch left its mode, so an
    /// emerge that ends its channel does not drop to the deep sink of a unit without a clip.
    pub clip: bool,
    /// The status looks, by [`status_index`].
    pub status: [Option<HeldLook>; STATUS_ORDER.len()],
    /// Whether a hit on a status was published on the previous frame, by [`status_index`].
    pub hit_seen: [bool; STATUS_ORDER.len()],
    /// When the last hit effect spawned, seconds of `Time<Virtual>`, by [`status_index`].
    pub last_hit: [Option<f64>; STATUS_ORDER.len()],
}

/// Looks a track remembers while they erode out: a unit can drop its stance, its morph, its
/// cover and a few statuses on one frame (a stim and a slow running out together).
pub const RELEASING_SLOTS: usize = 6;

/// Looks a track holds at once: stance, morph, cover and one per status kind.
const HELD_SLOTS: usize = 3 + STATUS_ORDER.len();

impl StanceTrack {
    /// The looks held now: stance, morph, cover and the statuses.
    fn looks(&self) -> [Option<HeldLook>; HELD_SLOTS] {
        let mut looks = [None; HELD_SLOTS];
        looks[0] = self.stance;
        looks[1] = self.morph.map(|morph| morph.look);
        looks[2] = self.cover;
        looks[3..].copy_from_slice(&self.status);
        looks
    }

    /// Remembers `look`, just released, while it erodes out (the oldest remembered one is
    /// forgotten; it has eroded out or will a moment later).
    fn remember_released(&mut self, look: HeldLook) {
        self.releasing[self.next_releasing] = Some(look);
        self.next_releasing = (self.next_releasing + 1) % RELEASING_SLOTS;
    }
}

/// Releases every look `entity`'s track holds when the track is removed.
fn release_held_looks(mut world: DeferredWorld<'_>, entity: Entity, _: ComponentId) {
    let Some(track) = world.get::<StanceTrack>(entity).copied() else {
        return;
    };
    let now = world
        .get_resource::<Time<Virtual>>()
        .map_or(0.0, Time::elapsed_secs_f64);
    if let Some(mut runtime) = world.get_resource_mut::<EffectRuntime>() {
        for look in track.looks().into_iter().flatten() {
            runtime.release(look.handle, now);
        }
    }
}

/// The effect system's state one bubble's looks are kept in step with.
pub struct LookContext<'a> {
    /// The vocabulary.
    pub catalog: &'a EffectCatalog,
    /// The units' choices.
    pub tables: &'a UnitEffectTables,
    /// Live effects.
    pub runtime: &'a mut EffectRuntime,
    /// Seconds of `Time<Virtual>`.
    pub now: f64,
    /// Seconds of `Time<Virtual>` since the previous frame.
    pub delta: f64,
}

impl LookContext<'_> {
    /// Spawns `effect` once at `anchor` with `bound` (what a held status lays or spawns).
    fn spawn(&mut self, effect: EffectId, anchor: Vec3, bound: BoundValues) {
        let request = SpawnRequest {
            effect,
            anchor,
            bound,
            line: None,
            delay: 0.0,
        };
        self.runtime.spawn(self.catalog, &request, self.now);
    }

    /// Releases `look`, if any.
    fn release(&mut self, look: Option<HeldLook>) {
        if let Some(look) = look {
            self.runtime.release(look.handle, self.now);
        }
    }

    /// Keeps `slot` holding `wanted` (releasing what it held otherwise), anchored at `anchor`
    /// with `bound`; a new look is held `settled` when asked.
    fn keep(
        &mut self,
        slot: &mut Option<HeldLook>,
        wanted: Option<EffectId>,
        anchor: Vec3,
        bound: BoundValues,
        settled: bool,
    ) {
        if slot.map(|look| look.effect) != wanted {
            self.release(slot.take());
            *slot = wanted.and_then(|effect| {
                let request = SpawnRequest {
                    effect,
                    anchor,
                    bound,
                    line: None,
                    delay: 0.0,
                };
                let handle = self
                    .runtime
                    .hold(self.catalog, &request, self.now, settled)?;
                Some(HeldLook { effect, handle })
            });
        }
        if let Some(look) = *slot {
            if !self.runtime.update_held(look.handle, anchor, bound) {
                *slot = None;
            }
        }
    }
}

/// Where a unit's ground looks stand and where its body is, in Bevy world space.
pub(crate) fn anchors(bubble: &BubbleRenderInstance) -> (Vec3, Vec3) {
    let [x, y, altitude] = bubble.world_pos;
    let ground = Vec3::new(x, bubble.ground_z, y);
    let lift = match bubble.shape {
        ShapeKind::UnitSphere => bubble.radius,
        ShapeKind::BuildingDome { .. } => 0.0,
    };
    (ground, Vec3::new(x, altitude + lift, y))
}

/// Brings `track` up to `bubble` as the viewer sees it (`presence`, `own`) and returns the
/// shell its looks give the bubble; `clip` says whether the unit shows its own clip for the
/// mode its stance or switch is about (WP13 media), which keeps its sink shallow.
pub fn present_looks(
    track: &mut StanceTrack,
    bubble: &BubbleRenderInstance,
    presence: Presence,
    own: bool,
    clip: bool,
    context: &mut LookContext<'_>,
) -> ShellUniformGpu {
    let (ground, body) = anchors(bubble);
    let unit = BoundValues::default()
        .with(Binding::UnitRadius, bubble.radius)
        .with(Binding::UnitHeight, bubble.radius * 2.0);
    let cover = cover_effect(context.tables, presence, own, bubble.concealment);
    if presence != Presence::Shown {
        context.release(track.stance.take());
        context.release(track.morph.take().map(|morph| morph.look));
        for slot in &mut track.status {
            context.release(slot.take());
        }
        track.hit_seen = [false; STATUS_ORDER.len()];
        context.keep(&mut track.cover, cover, body, unit, !track.seen);
        track.seen = false;
        return ShellUniformGpu::default();
    }
    let settled = !track.seen;
    let before = track.looks();
    keep_morph(track, bubble, ground, unit, context);
    let in_mode = bubble.stance.mode.is_some() || bubble.stance.transition.is_some();
    track.clip = kept_clip(track.clip, clip, !in_mode && track.morph.is_some());
    let clip = track.clip;
    if track.morph.is_some() {
        context.release(track.stance.take());
    } else {
        let stance_settled = settled || std::mem::take(&mut track.settle_next);
        let wanted = stance_effect(context.tables, bubble);
        let bound = unit.with(Binding::ChannelProgress, 1.0);
        context.keep(&mut track.stance, wanted, ground, bound, stance_settled);
    }
    context.keep(&mut track.cover, cover, body, unit, settled);
    keep_statuses(track, bubble, (ground, body), unit, settled, context);
    track.seen = true;
    let now_held = track.looks();
    for look in before.into_iter().flatten() {
        if !now_held.contains(&Some(look)) {
            track.remember_released(look);
        }
    }
    let mut shell = ShellSum::default();
    let morph_progress = track.morph.map_or(0.0, |morph| morph.progress);
    let released = track.releasing.iter().map(|look| (*look, 1.0));
    let statuses = track.status.iter().map(|look| (*look, 1.0));
    for (look, progress) in [
        (track.stance, 1.0),
        (track.morph.map(|morph| morph.look), morph_progress),
        (track.cover, 1.0),
    ]
    .into_iter()
    .chain(statuses)
    .chain(released)
    {
        let instance = look.and_then(|look| context.runtime.held(look.handle));
        if let Some(instance) = instance {
            shell.add(context.catalog, instance, context.now, progress, clip);
        }
    }
    shell.uniform(bubble.radius, bubble.ground_z)
}

/// Keeps `bubble`'s status looks held at its body (the `anchors` pair is ground and body), each
/// while [`status_shown`] holds, bound to the unit's size and its barrier's fraction; lays what
/// each lays at the unit's foot when [`emit_due`], and spawns its hit effect at the body when
/// [`hit_due`]. A look first seen on a unit already in its status is held `settled`.
fn keep_statuses(
    track: &mut StanceTrack,
    bubble: &BubbleRenderInstance,
    anchors: (Vec3, Vec3),
    unit: BoundValues,
    settled: bool,
    context: &mut LookContext<'_>,
) {
    let (ground, body) = anchors;
    let bound = unit.with(Binding::BarrierRatio, bubble.barrier_ratio);
    let moving = bubble.velocity[0].hypot(bubble.velocity[1]) >= MOVING_SPEED;
    let phase =
        super::placement::unit_float(super::placement::hash3(bubble.entity_id.0, 0x57A7, 0));
    for kind in STATUS_ORDER {
        let index = status_index(kind);
        let wanted = status_shown(bubble, kind)
            .then(|| context.tables.status(bubble.unit_type, kind))
            .flatten();
        context.keep(&mut track.status[index], wanted, body, bound, settled);
        let hit = status_hit(bubble.status, kind);
        let was_hit = std::mem::replace(&mut track.hit_seen[index], hit);
        let Some(effect) = track.status[index].and_then(|look| context.catalog.effect(look.effect))
        else {
            continue;
        };
        let (emit, on_hit) = (effect.emit, effect.hit);
        if let Some(emit) = emit {
            if (moving || !emit.moving) && emit_due(context.now, context.delta, emit.every, phase) {
                context.spawn(emit.effect, ground, bound);
            }
        }
        if let Some(on_hit) = on_hit {
            if hit_due(
                hit,
                was_hit,
                context.now,
                track.last_hit[index],
                on_hit.every,
            ) {
                context.spawn(on_hit.effect, body, bound);
                track.last_hit[index] = Some(context.now);
            }
        }
    }
}

/// Whether the looks sink the unit as one with its own mode clip: `previous` while
/// `settling_out` (a morph settles after the switch left its mode, D-6, so the unit has no mode
/// left to ask about), otherwise `wanted` (what its mode or switch shows now).
const fn kept_clip(previous: bool, wanted: bool, settling_out: bool) -> bool {
    if settling_out {
        previous
    } else {
        wanted
    }
}

/// Keeps the morph of `bubble`'s mode switch held: starts it with the switch, follows the
/// channel, settles it after the channel (D-6) and releases it when it is complete or the
/// switch was cancelled.
fn keep_morph(
    track: &mut StanceTrack,
    bubble: &BubbleRenderInstance,
    ground: Vec3,
    unit: BoundValues,
    context: &mut LookContext<'_>,
) {
    if let Some(transition) = bubble.stance.transition {
        let wanted = morph_effect(context.tables, bubble, &transition);
        let same = track.morph.is_some_and(|morph| {
            Some(morph.look.effect) == wanted && morph.entering == transition.entering
        });
        if !same {
            context.release(track.morph.take().map(|morph| morph.look));
        }
        let channel = f32::from(transition.nominal_ticks) * TICK_SECONDS;
        let progress_of =
            |length: f32| presented_progress(channel_progress(&transition), channel, length);
        if track.morph.is_none() {
            if let Some(effect) = wanted {
                let length =
                    morph_length(channel, context.catalog.effect(effect).and_then(|e| e.span));
                context.release(track.stance.take());
                let bound = unit.with(Binding::ChannelProgress, progress_of(length));
                let mut slot = None;
                context.keep(&mut slot, Some(effect), ground, bound, false);
                track.morph = slot.map(|look| Morph {
                    look,
                    entering: transition.entering,
                    channel,
                    length,
                    progress: 0.0,
                    settling: None,
                });
            }
        }
        if let Some(morph) = track.morph.as_mut() {
            morph.progress = progress_of(morph.length);
            morph.settling = None;
        }
    } else if let Some(mut morph) = track.morph {
        if bubble.stance.mode != morph.entering {
            context.release(Some(morph.look));
            track.morph = None;
            track.settle_next = true;
            return;
        }
        let (since, from) = *morph.settling.get_or_insert((context.now, morph.progress));
        #[expect(
            clippy::cast_possible_truncation,
            reason = "seconds of a settle fit f32"
        )]
        let elapsed = (context.now - since) as f32;
        morph.progress = settle_progress(from, elapsed, morph.length);
        if morph.progress >= 1.0 {
            context.release(Some(morph.look));
            track.morph = None;
            return;
        }
        track.morph = Some(morph);
    }
    if let Some(morph) = track.morph {
        let bound = unit.with(Binding::ChannelProgress, morph.progress);
        if !context
            .runtime
            .update_held(morph.look.handle, ground, bound)
        {
            track.morph = None;
        }
    }
}

/// The `Shell` layers of the looks held on one unit, summed: the deepest sink, veil and soil,
/// the strongest tint and rim.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ShellSum {
    /// Share of the diameter sunk.
    pub sink: f32,
    /// Tint colour and share.
    pub tint: Vec4,
    /// Rim colour and strength.
    pub rim: Vec4,
    /// Share made see-through.
    pub veil: f32,
    /// Soil stain height as a share of the radius.
    pub soil: f32,
    /// Share of the specular highlight taken away.
    pub matte: f32,
    /// Share of the shadow taken away.
    pub shadow: f32,
}

impl ShellSum {
    /// Adds the `Shell` layers of `instance` at `now`; a progress-driven layer's life is
    /// `progress`, and `clip` says whether the unit shows its own clip for the look's mode
    /// (then a layer's `clip_sink` replaces its `sink`).
    ///
    /// A released look's held layers keep their veil, tint, rim, matte and shadow while they
    /// erode out over their exit, so a cloak or a shadow comes and goes without a pop; their
    /// sink and soil stop at once, because the next look (the morph after a stance, the stance
    /// after a morph) takes the body over on the same frame and each such pair is authored to
    /// meet. A released look's other layers add nothing.
    pub fn add(
        &mut self,
        catalog: &EffectCatalog,
        instance: &EffectInstance,
        now: f64,
        progress: f32,
        clip: bool,
    ) {
        let Some(effect) = catalog.effect(instance.effect) else {
            return;
        };
        let released = instance.released_at.is_some();
        for layer in &effect.layers {
            let Block::Shell(look) = &layer.block else {
                continue;
            };
            if released && !layer.held {
                continue;
            }
            let clock = layer_clock(layer, instance, now, 0.0);
            let life = match (look.progress, clock) {
                (true, _) => progress.clamp(0.0, 1.0),
                (false, Some(clock)) => clock.life,
                (false, None) => continue,
            };
            let mut strength =
                catalog.sample(layer.curve, life, 1.0) * layer_strength(layer, &instance.bound);
            let mut look = *look;
            if let Some(clock) = clock {
                let beat = look.beat(clock.age);
                look.tint.w *= beat;
                look.rim.w *= beat;
            }
            if clip {
                look.sink = look.clip_sink.unwrap_or(look.sink);
            }
            if released {
                let Some(clock) = clock else {
                    continue;
                };
                strength *= 1.0 - clock.exit;
                look.sink = 0.0;
                look.soil = 0.0;
            }
            self.add_look(&look, strength);
        }
    }

    /// Adds one shell `look` at intensity `strength`.
    pub fn add_look(&mut self, look: &ShellLook, strength: f32) {
        let strength = strength.clamp(0.0, 1.0);
        self.matte = self.matte.max(look.matte * strength);
        self.shadow = self.shadow.max(look.shadow * strength);
        self.sink = self.sink.max(look.sink * strength);
        self.veil = self.veil.max(look.veil * strength);
        self.soil = self.soil.max(look.soil * strength);
        if look.tint.w * strength > self.tint.w {
            self.tint = look.tint.truncate().extend(look.tint.w * strength);
        } else if look.soil * strength > 0.0 && self.tint.w == 0.0 {
            // A stain's colour is its look's tint even when the look tints no media.
            self.tint = look.tint.truncate().extend(0.0);
        }
        if look.rim.w * strength > self.rim.w {
            self.rim = look.rim.truncate().extend(look.rim.w * strength);
        }
    }

    /// The bubble uniform of this sum for a bubble of `radius` whose ground lies at Bevy
    /// height `ground`.
    #[must_use]
    // PERFORMANCE RATIONALE:
    // Why: The ground height under a unit changes while it walks over ramps, and a changed lane
    // modifies (re-prepares) its material; only the soil stain reads it.
    // What: The ground lane is written only while a soil stain is drawn, and is 0 otherwise.
    // Trade-off: One branch per unit and frame, in exchange for no material change on a walk.
    pub fn uniform(&self, radius: f32, ground: f32) -> ShellUniformGpu {
        let soil = self.soil * radius;
        ShellUniformGpu {
            tint: self.tint,
            rim: self.rim,
            shape: Vec4::new(
                self.sink * radius * 2.0,
                self.veil,
                soil,
                if soil > 0.0 { ground } else { 0.0 },
            ),
            driven: Vec4::new(1.0, self.matte, self.shadow, 0.0),
        }
    }
}

/// The bubbles whose looks [`present_stances`] keeps.
type StanceQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static BubbleInstance,
        Option<&'static mut StanceTrack>,
        &'static MeshMaterial3d<BubbleDomeMaterial>,
        Has<NoAutomaticBatching>,
    ),
    With<LiveBattleBubble>,
>;

/// Whether a bubble with `shell` must cast its shadow with its own material: its shadow lane or
/// its sink moves its shadow caster (`bubble_shadow.wgsl`).
///
/// Bevy 0.15 bins shadow draws by pipeline and mesh only (`ShadowBinKey`), and every bubble
/// shares one sphere, so a batch of bubbles casts with one representative's material and shell.
/// Only bubbles with a neutral shell are left batched, where any representative casts the
/// right shape; the others get [`NoAutomaticBatching`] while their shell moves the caster.
#[must_use]
pub fn casts_own_shadow(shell: &ShellUniformGpu) -> bool {
    shell.driven.z > 0.0 || shell.shape.x > 0.0
}

/// Whether `bubble`'s unit shows its own clip for the mode its switch enters or its stance
/// holds (WP13 media), so its looks sink it only a little.
fn shows_mode_clip(registry: Option<&FlipbookRegistry>, bubble: &BubbleRenderInstance) -> bool {
    let mode = match bubble.stance.transition {
        Some(transition) => transition.entering.or(bubble.stance.mode),
        None => bubble.stance.mode,
    };
    match (registry, mode) {
        (Some(registry), Some(mode)) => registry.get_mode(bubble.unit_type, mode).is_some(),
        _ => false,
    }
}

/// Keeps every live bubble's looks held and writes their shells into the bubble materials.
///
/// A bubble seen for the first time gets its [`StanceTrack`]. Its shadow fades with its
/// looks through the shell's shadow lane (the shadow caster shrinks into the body,
/// `bubble_shadow.wgsl`), never toggled, so it never pops (WP8 review 1, B-2).
#[expect(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "Bevy passes system parameters by value, one per declared dependency"
)]
pub fn present_stances(
    mut commands: Commands<'_, '_>,
    fog: Option<Res<'_, BattleFog>>,
    catalog: Option<Res<'_, EffectCatalog>>,
    tables: Option<Res<'_, UnitEffectTables>>,
    runtime: Option<ResMut<'_, EffectRuntime>>,
    registry: Option<Res<'_, FlipbookRegistry>>,
    time: Res<'_, Time<Virtual>>,
    materials: Option<ResMut<'_, Assets<BubbleDomeMaterial>>>,
    mut bubbles: StanceQuery<'_, '_>,
) {
    // Apps without bubbles (the effect lab) have no bubble materials and nothing to present.
    let (Some(catalog), Some(tables), Some(mut runtime), Some(mut materials)) =
        (catalog, tables, runtime, materials)
    else {
        return;
    };
    let fog = fog.as_deref();
    let registry = registry.as_deref();
    let mut context = LookContext {
        catalog: &catalog,
        tables: &tables,
        runtime: &mut runtime,
        now: time.elapsed_secs_f64(),
        delta: time.delta_secs_f64(),
    };
    // PERFORMANCE RATIONALE:
    // Why: An unbatched bubble costs one shadow draw per cascade; marking all 400 units of a
    // battle would add about a thousand draws (WP8 review 2, S-2).
    // What: Only bubbles whose shell moves their shadow caster are unbatched, and only while it
    // does; the component is inserted or removed on the frame that changes.
    // Trade-off: A structural change on a unit's concealment or stance switch, in exchange for
    // no extra draw for ordinary units.
    for (entity, bubble, track, material, unbatched) in &mut bubbles {
        let presence = presence_of(fog, &bubble.0);
        let own = fog.is_none_or(|fog| fog.viewer() == Some(bubble.0.owner));
        let clip = shows_mode_clip(registry, &bubble.0);
        let shell = if let Some(mut track) = track {
            present_looks(&mut track, &bubble.0, presence, own, clip, &mut context)
        } else {
            let mut track = StanceTrack::default();
            let shell = present_looks(&mut track, &bubble.0, presence, own, clip, &mut context);
            commands.entity(entity).insert(track);
            shell
        };
        sync_bubble_shell(&mut materials, &material.0, &shell);
        match (casts_own_shadow(&shell), unbatched) {
            (true, false) => {
                commands.entity(entity).insert(NoAutomaticBatching);
            }
            (false, true) => {
                commands.entity(entity).remove::<NoAutomaticBatching>();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transition(remaining: u16, nominal: u16, entering: Option<u8>) -> ModeTransitionView {
        ModeTransitionView {
            entering,
            remaining_ticks: remaining,
            nominal_ticks: nominal,
        }
    }

    fn bubble(status: StatusFlags, barrier_ratio: f32) -> BubbleRenderInstance {
        use sim_types::{
            AnimState, EntityId, PlayerId, SilhouetteClass, StanceView, UnitTypeId,
            WeaponVisualKind,
        };
        BubbleRenderInstance {
            entity_id: EntityId(1),
            shape: ShapeKind::UnitSphere,
            silhouette: SilhouetteClass::HeavyHex,
            unit_type: UnitTypeId::default(),
            owner: PlayerId(0),
            world_pos: [0.0, 0.0, 0.0],
            ground_z: 0.0,
            radius: 0.45,
            velocity: [0.0; 2],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: false,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: AnimState::Idle,
            attack_swing: None,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: false,
            concealment: Concealment::None,
            speed_boosted: false,
            stance: StanceView::default(),
            status,
            barrier_ratio,
        }
    }

    #[test]
    fn a_status_is_shown_while_published_and_a_barrier_only_while_it_absorbs() {
        let none = bubble(StatusFlags::default(), 0.0);
        for kind in STATUS_ORDER {
            assert!(!status_shown(&none, kind), "{kind:?}");
        }
        let stimmed = bubble(
            StatusFlags {
                overclock: true,
                slowed: true,
                dash: true,
                ..StatusFlags::default()
            },
            0.0,
        );
        assert!(status_shown(&stimmed, StatusKind::Overclock));
        assert!(status_shown(&stimmed, StatusKind::Slowed));
        assert!(status_shown(&stimmed, StatusKind::Dash));
        assert!(!status_shown(&stimmed, StatusKind::Barrier));
        let barrier = StatusFlags {
            barrier_up: true,
            ..StatusFlags::default()
        };
        assert!(status_shown(&bubble(barrier, 0.4), StatusKind::Barrier));
        assert!(
            !status_shown(&bubble(barrier, 0.0), StatusKind::Barrier),
            "the outer shell only while barrier_ratio > 0"
        );
        assert!(
            !status_shown(&bubble(StatusFlags::default(), 0.4), StatusKind::Barrier),
            "nor once the barrier is down"
        );
    }

    #[test]
    fn only_a_barrier_is_hit_and_hits_flash_at_their_beat() {
        let hit = StatusFlags {
            barrier_up: true,
            barrier_hit: true,
            ..StatusFlags::default()
        };
        assert!(status_hit(hit, StatusKind::Barrier));
        for kind in STATUS_ORDER {
            if kind != StatusKind::Barrier {
                assert!(!status_hit(hit, kind), "{kind:?}");
            }
        }
        assert!(!status_hit(StatusFlags::default(), StatusKind::Barrier));
        // The first frame of a hit flashes; while hits go on, at most once per beat.
        assert!(hit_due(true, false, 10.0, Some(9.99), 0.2));
        assert!(!hit_due(true, true, 10.0, Some(9.9), 0.2));
        assert!(hit_due(true, true, 10.0, Some(9.75), 0.2));
        assert!(hit_due(true, true, 10.0, None, 0.2));
        assert!(!hit_due(false, true, 10.0, None, 0.2), "no hit, no flash");
    }

    #[test]
    fn an_emitted_effect_is_laid_once_per_beat_at_the_units_own_phase() {
        let step = 0.04;
        let frames = |phase: f32, last: u32| {
            (1..=last)
                .filter(|frame| emit_due(f64::from(*frame) * step, step, 0.5, phase))
                .collect::<Vec<_>>()
        };
        // 10 seconds at a 0.5 s beat: 20 spawns whatever the phase.
        assert_eq!(frames(0.0, 250).len(), 20);
        assert_eq!(frames(0.37, 250).len(), 20);
        // Two phases do not lay on the same frames.
        assert_ne!(frames(0.0, 50), frames(0.5, 50));
        // A frame longer than the beat lays one, not several.
        assert!(emit_due(1.0, 2.0, 0.1, 0.0));
        assert!(!emit_due(1.0, 0.0, 0.1, 0.0), "no time, no spawn");
    }

    #[test]
    fn a_pulsing_shell_beats_its_tint_and_rim_only() {
        let look = ShellLook {
            sink: 0.0,
            tint: Vec4::new(0.9, 0.3, 0.2, 0.2),
            rim: Vec4::new(1.0, 0.3, 0.2, 0.6),
            veil: 0.0,
            soil: 0.0,
            progress: false,
            clip_sink: None,
            matte: 0.3,
            shadow: 0.0,
            pulse: Some(crate::effects::catalog::schema::PulseDef {
                period: 0.6,
                depth: 0.6,
            }),
        };
        assert!((look.beat(0.0) - 1.0).abs() < 1e-6);
        assert!((look.beat(0.3) - 0.4).abs() < 1e-5);
        assert!((look.beat(0.6) - 1.0).abs() < 1e-5);
        assert!((look.beat(0.15) - 0.7).abs() < 1e-5);
        let steady = ShellLook {
            pulse: None,
            ..look
        };
        assert_eq!(steady.beat(0.3), 1.0);
    }

    #[test]
    fn only_a_shell_that_moves_the_shadow_caster_casts_unbatched() {
        // A neutral shell, and one that only tints or veils: batched with the other bubbles.
        assert!(!casts_own_shadow(&ShellUniformGpu::default()));
        let veiled = ShellSum {
            veil: 0.45,
            tint: Vec4::new(0.3, 0.4, 0.6, 0.15),
            ..ShellSum::default()
        };
        assert!(!casts_own_shadow(&veiled.uniform(0.5, 0.0)));
        // A shadow lane or a sink moves the caster.
        let shaded = ShellSum {
            shadow: 0.7,
            ..ShellSum::default()
        };
        assert!(casts_own_shadow(&shaded.uniform(0.5, 0.0)));
        let sunk = ShellSum {
            sink: 0.12,
            ..ShellSum::default()
        };
        assert!(casts_own_shadow(&sunk.uniform(0.5, 0.0)));
    }

    #[test]
    fn an_emerge_settling_after_its_channel_keeps_the_clip_sink() {
        // Burrowed with a clip, then switching out: the switch's mode decides.
        assert!(kept_clip(false, true, false));
        // The channel has ended, the unit has no mode, the morph still settles: kept.
        assert!(kept_clip(true, false, true));
        // Settled out of every mode: what the unit shows now.
        assert!(!kept_clip(true, false, false));
        // A unit without a clip never gains one while it settles.
        assert!(!kept_clip(false, true, true));
    }

    #[test]
    fn the_random_extra_holds_the_morph_at_its_start() {
        // A 33-tick channel with 4 extra ticks drawn: 37 remaining at its first tick.
        for remaining in [37, 35, 34, 33] {
            assert_eq!(
                channel_progress(&transition(remaining, 33, Some(0))),
                0.0,
                "{remaining}"
            );
        }
        assert!(channel_progress(&transition(32, 33, Some(0))) > 0.0);
    }

    #[test]
    fn progress_runs_from_zero_to_one_and_clamps() {
        assert_eq!(channel_progress(&transition(72, 72, Some(0))), 0.0);
        assert!((channel_progress(&transition(36, 72, Some(0))) - 0.5).abs() < 1e-6);
        assert_eq!(channel_progress(&transition(0, 72, Some(0))), 1.0);
        assert_eq!(channel_progress(&transition(500, 72, None)), 0.0);
        assert_eq!(
            channel_progress(&transition(3, 0, None)),
            1.0,
            "no nominal length"
        );
    }

    #[test]
    fn leaving_runs_the_same_way_as_entering() {
        // Leaving publishes `entering: None` and the leaving channel's nominal ticks; the
        // progress is the share of that channel gone either way.
        let entering = channel_progress(&transition(5, 11, Some(0)));
        let leaving = channel_progress(&transition(5, 11, None));
        assert_eq!(entering, leaving);
        assert!((leaving - 6.0 / 11.0).abs() < 1e-6);
    }

    #[test]
    fn a_morph_longer_than_its_channel_finishes_after_it_at_its_own_pace() {
        // Q0069: the burrowed stats at 33 ticks, the dig to 45 (1.8 s).
        let channel = 33.0 * TICK_SECONDS;
        let length = morph_length(channel, Some(1.8));
        assert!((length - 1.8).abs() < 1e-6);
        let at_end = presented_progress(1.0, channel, length);
        assert!((at_end - 33.0 / 45.0).abs() < 1e-5, "{at_end}");
        // The remaining 12 ticks of the dig after the channel.
        let settled = settle_progress(at_end, 12.0 * TICK_SECONDS, length);
        assert!((settled - 1.0).abs() < 1e-5, "{settled}");
        assert!(settle_progress(at_end, 6.0 * TICK_SECONDS, length) < 1.0);
        assert_eq!(settle_progress(at_end, 10.0, length), 1.0, "clamped");
    }

    #[test]
    fn a_morph_shorter_than_its_channel_follows_the_channel() {
        let channel = 72.0 * TICK_SECONDS;
        assert_eq!(morph_length(channel, Some(0.5)), channel);
        assert_eq!(morph_length(channel, None), channel);
        assert!((presented_progress(0.5, channel, channel) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_shell_sum_takes_the_deepest_sink_and_strongest_tint() {
        let mut sum = ShellSum::default();
        let deep = ShellLook {
            sink: 0.9,
            tint: Vec4::new(0.1, 0.08, 0.05, 0.3),
            rim: Vec4::ZERO,
            veil: 0.0,
            soil: 0.4,
            progress: true,
            clip_sink: Some(0.1),
            matte: 1.0,
            shadow: 1.0,
            pulse: None,
        };
        let cloak = ShellLook {
            sink: 0.0,
            tint: Vec4::new(0.2, 0.3, 0.4, 0.1),
            rim: Vec4::new(0.5, 0.7, 1.0, 0.6),
            veil: 0.7,
            soil: 0.0,
            progress: false,
            clip_sink: None,
            matte: 0.0,
            shadow: 0.6,
            pulse: None,
        };
        sum.add_look(&deep, 0.5);
        sum.add_look(&cloak, 1.0);
        assert!((sum.sink - 0.45).abs() < 1e-6);
        assert!((sum.soil - 0.2).abs() < 1e-6);
        assert_eq!(sum.veil, 0.7);
        assert!(
            (sum.tint.w - 0.15).abs() < 1e-6,
            "the burrow tint at half strength wins"
        );
        assert_eq!(sum.rim, cloak.rim);
        let uniform = sum.uniform(0.5, 2.0);
        assert!((uniform.shape.x - 0.45).abs() < 1e-6, "sink in world units");
        assert!((uniform.shape.z - 0.1).abs() < 1e-6, "soil in world units");
        assert_eq!(uniform.shape.w, 2.0);
        assert_eq!(uniform.driven.x, 1.0);
        assert!(
            (uniform.driven.y - 0.5).abs() < 1e-6,
            "matte at half strength"
        );
        assert!(
            (uniform.driven.z - 0.6).abs() < 1e-6,
            "the cloak's shadow wins"
        );
        assert_eq!(ShellSum::default().uniform(0.5, 0.0).shape.x, 0.0);
        let mut no_soil = ShellSum::default();
        no_soil.add_look(&cloak, 1.0);
        assert_eq!(
            no_soil.uniform(0.5, 2.0).shape.w,
            0.0,
            "no ground lane without a stain"
        );
    }
}
