//! Concealment and stance looks held on units: burrowing, burrowed, cloaked, detected, mode
//! switches and anchored stances (plan `2026-11-09-effect-visualization` WP8, §5.5, D-5, D-6).
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
//!   a cloaked enemy the viewer does not detect, or the mark of a concealed enemy it detects.
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
//! and marks are held only on a unit drawn as it is ([`Presence::Shown`]).
//!
//! # Invariants
//! - The simulation keeps no presentation state: every value here is derived from the
//!   published records and `Time<Virtual>`, or is this module's own (D-6, charter §3.2).
//! - Every look held on a bubble is released when the bubble's [`StanceTrack`] is removed
//!   (its entity despawned), so no held effect outlives its unit.
//! - A frame on which no look changes writes no material and allocates nothing.

use super::binding::BoundValues;
use super::catalog::schema::Binding;
use super::catalog::{Block, EffectCatalog, EffectId, ShellLook, UnitEffectTables};
use super::pool::{EffectHandle, EffectInstance};
use super::runtime::{layer_clock, EffectRuntime, SpawnRequest};
use crate::battle_pipeline::fog::{presence_of, BattleFog, Presence};
use crate::bubble_shader::{
    sync_bubble_shell, BubbleDomeMaterial, BubbleInstance, ShellUniformGpu,
};
use crate::scene_controller::LiveBattleBubble;
use bevy::ecs::component::ComponentId;
use bevy::ecs::world::DeferredWorld;
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use sim_types::{BubbleRenderInstance, Concealment, ModeTransitionView, ShapeKind};

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
        for look in [
            track.stance,
            track.morph.map(|morph| morph.look),
            track.cover,
        ]
        .into_iter()
        .flatten()
        {
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
}

impl LookContext<'_> {
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
fn anchors(bubble: &BubbleRenderInstance) -> (Vec3, Vec3) {
    let [x, y, altitude] = bubble.world_pos;
    let ground = Vec3::new(x, bubble.ground_z, y);
    let lift = match bubble.shape {
        ShapeKind::UnitSphere => bubble.radius,
        ShapeKind::BuildingDome { .. } => 0.0,
    };
    (ground, Vec3::new(x, altitude + lift, y))
}

/// Brings `track` up to `bubble` as the viewer sees it (`presence`, `own`) and returns the
/// shell its looks give the bubble.
pub fn present_looks(
    track: &mut StanceTrack,
    bubble: &BubbleRenderInstance,
    presence: Presence,
    own: bool,
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
        context.keep(&mut track.cover, cover, body, unit, !track.seen);
        track.seen = false;
        return ShellUniformGpu::default();
    }
    let settled = !track.seen;
    keep_morph(track, bubble, ground, unit, context);
    if track.morph.is_some() {
        context.release(track.stance.take());
    } else {
        let stance_settled = settled || std::mem::take(&mut track.settle_next);
        let wanted = stance_effect(context.tables, bubble);
        let bound = unit.with(Binding::ChannelProgress, 1.0);
        context.keep(&mut track.stance, wanted, ground, bound, stance_settled);
    }
    context.keep(&mut track.cover, cover, body, unit, settled);
    track.seen = true;
    let mut shell = ShellSum::default();
    for (look, progress) in [
        (track.stance, 1.0),
        (
            track.morph.map(|morph| morph.look),
            track.morph.map_or(0.0, |morph| morph.progress),
        ),
        (track.cover, 1.0),
    ] {
        let instance = look.and_then(|look| context.runtime.held(look.handle));
        if let Some(instance) = instance {
            shell.add(context.catalog, instance, context.now, progress);
        }
    }
    shell.uniform(bubble.radius, bubble.ground_z)
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
}

impl ShellSum {
    /// Adds the `Shell` layers of held `instance` at `now`; a progress-driven layer's life is
    /// `progress`. A released look adds nothing: the next look takes over on the same frame,
    /// and each pair of looks that follow one another is authored to meet (a morph ends where
    /// its stance begins).
    pub fn add(
        &mut self,
        catalog: &EffectCatalog,
        instance: &EffectInstance,
        now: f64,
        progress: f32,
    ) {
        if instance.released_at.is_some() {
            return;
        }
        let Some(effect) = catalog.effect(instance.effect) else {
            return;
        };
        for layer in &effect.layers {
            let Block::Shell(look) = &layer.block else {
                continue;
            };
            let life = if look.progress {
                progress.clamp(0.0, 1.0)
            } else {
                match layer_clock(layer, instance, now, 0.0) {
                    Some(clock) => clock.life,
                    None => continue,
                }
            };
            self.add_look(look, catalog.sample(layer.curve, life, 1.0));
        }
    }

    /// Adds one shell `look` at intensity `strength`.
    pub fn add_look(&mut self, look: &ShellLook, strength: f32) {
        let strength = strength.clamp(0.0, 1.0);
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
            driven: Vec4::new(1.0, 0.0, 0.0, 0.0),
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
        Has<NotShadowCaster>,
    ),
    With<LiveBattleBubble>,
>;

/// Keeps every live bubble's looks held and writes their shells into the bubble materials.
///
/// A bubble seen for the first time gets its [`StanceTrack`]. A bubble sunk or veiled by its
/// looks casts no shadow (its shadow would be the whole sphere's), and casts one again when
/// neither holds.
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
    time: Res<'_, Time<Virtual>>,
    mut materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    mut bubbles: StanceQuery<'_, '_>,
) {
    let (Some(catalog), Some(tables), Some(mut runtime)) = (catalog, tables, runtime) else {
        return;
    };
    let fog = fog.as_deref();
    let mut context = LookContext {
        catalog: &catalog,
        tables: &tables,
        runtime: &mut runtime,
        now: time.elapsed_secs_f64(),
    };
    for (entity, bubble, track, material, shadowless) in &mut bubbles {
        let presence = presence_of(fog, &bubble.0);
        let own = fog.is_none_or(|fog| fog.viewer() == Some(bubble.0.owner));
        let shell = if let Some(mut track) = track {
            present_looks(&mut track, &bubble.0, presence, own, &mut context)
        } else {
            let mut track = StanceTrack::default();
            let shell = present_looks(&mut track, &bubble.0, presence, own, &mut context);
            commands.entity(entity).insert(track);
            shell
        };
        sync_bubble_shell(&mut materials, &material.0, &shell);
        let hide_shadow = shell.shape.x > 0.0 || shell.shape.y > 0.0;
        if hide_shadow && !shadowless {
            commands.entity(entity).insert(NotShadowCaster);
        } else if !hide_shadow && shadowless {
            commands.entity(entity).remove::<NotShadowCaster>();
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
        };
        let cloak = ShellLook {
            sink: 0.0,
            tint: Vec4::new(0.2, 0.3, 0.4, 0.1),
            rim: Vec4::new(0.5, 0.7, 1.0, 0.6),
            veil: 0.7,
            soil: 0.0,
            progress: false,
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
        assert_eq!(ShellSum::default().uniform(0.5, 0.0).shape.x, 0.0);
    }
}
