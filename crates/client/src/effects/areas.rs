//! Looks held on the persistent areas and tethers the simulation publishes (plan
//! `2026-11-09-effect-visualization` WP10, §5.5, D-5, D-8).
//!
//! # Responsibility
//! Owns [`AreaLooks`], the client-local record of which effect is held on which published
//! [`AreaView`] and [`LinkView`], and [`present_areas_and_links`], which keeps those effects held
//! in the [`EffectRuntime`] every frame:
//! - an area (a storm, a cloud) holds the effect its caster's presentation block names for the
//!   ability that opened it ([`UnitEffectTables::ability`]), anchored at the area's centre and
//!   bound to `AreaRadius`, `AreaRemaining` and `AreaTotal`; an area effect that `emit`s lays
//!   that effect again at its beat while the area holds (a storm's churning flares);
//! - a tether holds its ability's `Link` effect along a line from the caster's bubble to the
//!   target's, each end on its bubble's surface, moved with both bubbles every frame and bound
//!   to `LinkLength`.
//!
//! # Fog (D-5), judged per frame
//! - An area of the viewer's own (or with no fog kept) is drawn whole. An enemy's area is drawn
//!   only where the viewer sees it: [`area_sight`] samples the disc against the viewer's
//!   visible cells; an area wholly in fog holds nothing, and one seen in part is cut where the
//!   fog begins (a clip window across the disc, [`EffectRuntime::set_fog_clip`]).
//! - A tether is drawn only while both its bubbles are drawn as they are
//!   ([`Presence::Shown`]); otherwise it would point at a unit the viewer may not see.
//!
//! # Invariants
//! - The simulation keeps no presentation state; every value here is derived from the
//!   presented snapshot and `Time<Virtual>` (charter §3.2).
//! - An area or tether that is no longer published, or no longer seen, has its look released
//!   (eroding out over the look's `exit`) on that frame.
//! - The record's buffers are sized at construction and retained: steady state allocates
//!   nothing.

use super::binding::BoundValues;
use super::catalog::schema::{Binding, EventKind};
use super::catalog::{EffectCatalog, EffectId, UnitEffectTables};
use super::particles::LineClip;
use super::placement::WorldLine;
use super::pool::EffectHandle;
use super::runtime::{EffectRuntime, SpawnRequest, NO_CLIP};
use super::stance::{anchors, emit_due};
use crate::battle_pipeline::fog::{presence_of, BattleFog, Presence};
use crate::battle_pipeline::session::BattleSession;
use bevy::prelude::*;
use sim_types::{AbilitySource, AreaView, BubbleRenderInstance, EntityId, LinkView};

/// Seconds of one simulation tick.
const TICK_SECONDS: f32 = 1.0 / sim_types::TICKS_PER_SECOND as f32;

/// Areas, and tethers, whose looks are held at once; more are not drawn (counted in
/// [`AreaLooks::refused`]). A battle of 400 units has a few dozen spells live at most.
pub const HELD_SLOTS: usize = 64;

/// Samples across an area's diameter for [`area_sight`].
const SIGHT_SAMPLES: usize = 17;

/// Axes tried for the cut of an area seen in part, evenly round the turn.
const CUT_DIRECTIONS: usize = 32;

/// How the viewer sees an area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AreaSight {
    /// Not at all: nothing of it is drawn.
    Hidden,
    /// Wholly: drawn whole.
    Whole,
    /// In part: drawn only on the side of this clip window (`dot(world.xz, axis) >= min`).
    Part(LineClip),
}

/// How the viewer sees the area of radius `radius` (world units) centred at `center` (map
/// `x, y`), where `sees(x, y)` says whether a map point is in a visible cell.
///
/// The disc is sampled on a 17 × 17 grid. With every sample seen it is [`AreaSight::Whole`],
/// with none [`AreaSight::Hidden`]; otherwise the fog's edge across so small a disc is close to
/// a line, so the area is cut by a half plane. The cut fails closed (D-5): along each of
/// [`CUT_DIRECTIONS`] axes it lies half a sample step beyond the farthest unseen sample, so no
/// unseen sample is drawn, and of those cuts the one that keeps the most seen samples is taken;
/// an area none of whose seen samples any cut keeps is [`AreaSight::Hidden`].
#[must_use]
pub fn area_sight(center: Vec2, radius: f32, sees: impl Fn(f32, f32) -> bool) -> AreaSight {
    let mut samples = [(Vec2::ZERO, false); SIGHT_SAMPLES * SIGHT_SAMPLES];
    let mut count = 0;
    let mut seen_count = 0_u32;
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample indices below 17 are exact in f32"
    )]
    let step = 2.0 * radius / (SIGHT_SAMPLES - 1) as f32;
    for row in 0..SIGHT_SAMPLES {
        for column in 0..SIGHT_SAMPLES {
            #[expect(
                clippy::cast_precision_loss,
                reason = "sample indices below 17 are exact in f32"
            )]
            let offset = Vec2::new(column as f32, row as f32) * step - Vec2::splat(radius);
            if offset.length() > radius * 1.0001 {
                continue;
            }
            let point = center + offset;
            let seen = sees(point.x, point.y);
            samples[count] = (point, seen);
            count += 1;
            seen_count += u32::from(seen);
        }
    }
    let samples = &samples[..count];
    let total = u32::try_from(count).unwrap_or(u32::MAX);
    if seen_count == 0 {
        return AreaSight::Hidden;
    }
    if seen_count == total {
        return AreaSight::Whole;
    }
    // The cut that keeps the most seen samples: for each of CUT_DIRECTIONS axes, the half plane
    // half a step beyond the farthest unseen sample.
    let mut best: Option<(u32, Vec2, f32)> = None;
    for direction in 0..CUT_DIRECTIONS {
        #[expect(
            clippy::cast_precision_loss,
            reason = "direction indices below 64 are exact in f32"
        )]
        let axis =
            Vec2::from_angle(direction as f32 * std::f32::consts::TAU / CUT_DIRECTIONS as f32);
        let threshold = samples
            .iter()
            .filter(|(_, seen)| !*seen)
            .map(|(point, _)| point.dot(axis))
            .fold(f32::NEG_INFINITY, f32::max)
            + 0.5 * step;
        let kept = samples
            .iter()
            .filter(|(point, seen)| *seen && point.dot(axis) >= threshold)
            .count();
        let kept = u32::try_from(kept).unwrap_or(u32::MAX);
        if kept > best.map_or(0, |(most, _, _)| most) {
            best = Some((kept, axis, threshold));
        }
    }
    let Some((_, axis, threshold)) = best else {
        return AreaSight::Hidden;
    };
    let far = center.dot(axis) + 4.0 * radius + 1.0;
    AreaSight::Part([axis.x, axis.y, threshold, far])
}

/// The ends of a tether between two bodies of radius `from_radius` and `to_radius` centred at
/// `from` and `to` (world space): on each body's surface, facing the other. Bodies that touch
/// or overlap give the segment between their centres.
#[must_use]
pub fn tether_ends(from: Vec3, from_radius: f32, to: Vec3, to_radius: f32) -> (Vec3, Vec3) {
    let span = to - from;
    let length = span.length();
    if length <= from_radius + to_radius {
        return (from, to);
    }
    let direction = span / length;
    (from + direction * from_radius, to - direction * to_radius)
}

/// One held area.
#[derive(Debug, Clone, Copy, PartialEq)]
struct AreaSlot {
    /// The published area's id.
    id: u32,
    /// The effect held.
    effect: EffectId,
    /// Its live instance.
    handle: EffectHandle,
    /// Whether it was kept this frame.
    kept: bool,
}

/// One held tether.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LinkSlot {
    /// Its caster.
    from: EntityId,
    /// Its target.
    to: EntityId,
    /// The ability that made it.
    source: AbilitySource,
    /// The effect held.
    effect: EffectId,
    /// Its live instance.
    handle: EffectHandle,
    /// Whether it was kept this frame.
    kept: bool,
}

/// The looks held on the published areas and tethers.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct AreaLooks {
    /// Held areas.
    areas: Vec<AreaSlot>,
    /// Held tethers.
    links: Vec<LinkSlot>,
    /// Areas and tethers not drawn because [`HELD_SLOTS`] were held.
    pub refused: u64,
}

impl Default for AreaLooks {
    fn default() -> Self {
        Self {
            areas: Vec::with_capacity(HELD_SLOTS),
            links: Vec::with_capacity(HELD_SLOTS),
            refused: 0,
        }
    }
}

impl AreaLooks {
    /// How many areas and tethers hold a look now.
    #[must_use]
    pub fn held(&self) -> (usize, usize) {
        (self.areas.len(), self.links.len())
    }

    /// Retained capacities, for the steady-state tests.
    #[must_use]
    pub fn capacities(&self) -> (usize, usize) {
        (self.areas.capacity(), self.links.capacity())
    }
}

/// Snapshot `[x, y, altitude]` as a Bevy world position (Y up).
fn world(position: [f32; 3]) -> Vec3 {
    Vec3::new(position[0], position[2], position[1])
}

/// The values an area binds.
fn area_bound(area: &AreaView) -> BoundValues {
    BoundValues::default()
        .with(Binding::AreaRadius, area.radius)
        .with(
            Binding::AreaRemaining,
            f32::from(area.remaining_ticks) * TICK_SECONDS,
        )
        .with(
            Binding::AreaTotal,
            f32::from(area.total_ticks) * TICK_SECONDS,
        )
}

/// The effect of `kind` the source's ability names, if any.
fn ability_effect(
    catalog: &EffectCatalog,
    tables: &UnitEffectTables,
    source: Option<AbilitySource>,
    kind: EventKind,
) -> Option<EffectId> {
    let effect = tables.ability(source?)?;
    (catalog.effect(effect)?.event == kind).then_some(effect)
}

/// The effect system's state the looks are kept in.
pub struct Keep<'a> {
    /// The vocabulary.
    pub catalog: &'a EffectCatalog,
    /// The units' choices.
    pub tables: &'a UnitEffectTables,
    /// Live effects.
    pub runtime: &'a mut EffectRuntime,
    /// Seconds of `Time<Virtual>`.
    pub now: f64,
    /// Seconds since the previous frame.
    pub delta: f64,
}

/// Keeps `area`'s look held in `looks`, seen as `sight`.
fn keep_area(looks: &mut AreaLooks, area: &AreaView, sight: AreaSight, keep: &mut Keep<'_>) {
    let Some(effect) = ability_effect(keep.catalog, keep.tables, area.source, EventKind::Area)
    else {
        return;
    };
    let clip = match sight {
        AreaSight::Hidden => return,
        AreaSight::Whole => NO_CLIP,
        AreaSight::Part(clip) => clip,
    };
    let anchor = world(area.center);
    let bound = area_bound(area);
    let index = looks
        .areas
        .iter()
        .position(|slot| slot.id == area.id && slot.effect == effect);
    let handle = if let Some(index) = index {
        let slot = &mut looks.areas[index];
        if !keep.runtime.update_held(slot.handle, anchor, bound) {
            return;
        }
        slot.kept = true;
        slot.handle
    } else {
        if looks.areas.len() == looks.areas.capacity() {
            looks.refused += 1;
            return;
        }
        // An area seen on the tick it opened comes in; one first seen later (out of the fog,
        // or after a scene load) is shown settled.
        let fresh = area.total_ticks.saturating_sub(area.remaining_ticks) <= 1;
        let request = SpawnRequest {
            effect,
            anchor,
            bound,
            line: None,
            delay: 0.0,
        };
        let Some(handle) = keep.runtime.hold(keep.catalog, &request, keep.now, !fresh) else {
            return;
        };
        looks.areas.push(AreaSlot {
            id: area.id,
            effect,
            handle,
            kept: true,
        });
        handle
    };
    keep.runtime.set_fog_clip(handle, clip);
    let Some(emit) = keep.catalog.effect(effect).and_then(|effect| effect.emit) else {
        return;
    };
    // The area's own phase, so two storms side by side do not flare in step.
    #[expect(
        clippy::cast_precision_loss,
        reason = "the phase needs only the low bits of the id"
    )]
    let phase = (area.id % 97) as f32 / 97.0;
    if emit_due(keep.now, keep.delta, emit.every, phase) {
        let request = SpawnRequest {
            effect: emit.effect,
            anchor,
            bound,
            line: None,
            delay: 0.0,
        };
        if let Some(laid) = keep.runtime.spawn(keep.catalog, &request, keep.now) {
            keep.runtime.set_fog_clip(laid, clip);
        }
    }
}

/// The presented bubble of `id`, by binary search (the snapshot's bubbles are in entity order).
fn bubble_of(bubbles: &[BubbleRenderInstance], id: EntityId) -> Option<&BubbleRenderInstance> {
    bubbles
        .binary_search_by_key(&id, |bubble| bubble.entity_id)
        .ok()
        .and_then(|index| bubbles.get(index))
}

/// Keeps `link`'s look held in `looks`, its ends on the presented bubbles.
fn keep_link(
    looks: &mut AreaLooks,
    link: &LinkView,
    bubbles: &[BubbleRenderInstance],
    fog: Option<&BattleFog>,
    keep: &mut Keep<'_>,
) {
    let Some(effect) = ability_effect(
        keep.catalog,
        keep.tables,
        Some(link.source),
        EventKind::Link,
    ) else {
        return;
    };
    let (Some(from), Some(to)) = (bubble_of(bubbles, link.from), bubble_of(bubbles, link.to))
    else {
        return;
    };
    if presence_of(fog, from) != Presence::Shown || presence_of(fog, to) != Presence::Shown {
        return;
    }
    let (from_end, to_end) = tether_ends(anchors(from).1, from.radius, anchors(to).1, to.radius);
    let line = WorldLine::whole(from_end, to_end);
    let bound = BoundValues::default().with(Binding::LinkLength, from_end.distance(to_end));
    let index = looks.links.iter().position(|slot| {
        slot.from == link.from
            && slot.to == link.to
            && slot.source == link.source
            && slot.effect == effect
    });
    if let Some(index) = index {
        let slot = &mut looks.links[index];
        slot.kept = keep.runtime.update_held_line(slot.handle, line, bound);
        return;
    }
    if looks.links.len() == looks.links.capacity() {
        looks.refused += 1;
        return;
    }
    let request = SpawnRequest {
        effect,
        anchor: to_end,
        bound,
        line: Some(line),
        delay: 0.0,
    };
    if let Some(handle) = keep.runtime.hold(keep.catalog, &request, keep.now, false) {
        looks.links.push(LinkSlot {
            from: link.from,
            to: link.to,
            source: link.source,
            effect,
            handle,
            kept: true,
        });
    }
}

/// Keeps the looks of every published area and tether in step with the presented snapshot
/// and the viewer's fog (D-5), and releases those no longer published or seen.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn present_areas_and_links(
    session: Option<Res<'_, BattleSession>>,
    fog: Option<Res<'_, BattleFog>>,
    catalog: Option<Res<'_, EffectCatalog>>,
    tables: Option<Res<'_, UnitEffectTables>>,
    runtime: Option<ResMut<'_, EffectRuntime>>,
    looks: Option<ResMut<'_, AreaLooks>>,
    time: Res<'_, Time<Virtual>>,
) {
    let (Some(catalog), Some(tables), Some(mut runtime), Some(mut looks)) =
        (catalog, tables, runtime, looks)
    else {
        return;
    };
    let mut keep = Keep {
        catalog: &catalog,
        tables: &tables,
        runtime: &mut runtime,
        now: time.elapsed_secs_f64(),
        delta: time.delta_secs_f64(),
    };
    let (views, bubbles) = session
        .as_deref()
        .map_or(((&[][..], &[][..]), &[][..]), |session| {
            (
                session.frames.presented_effect_views(),
                session.frames.presented().bubbles.as_slice(),
            )
        });
    keep_views(&mut looks, views, bubbles, fog.as_deref(), &mut keep);
}

/// Brings `looks` up to the published `views` (areas and tethers) of a frame whose presented
/// bubbles are `bubbles` (in entity order) under `fog` (`None`: no fog kept), and releases the
/// looks of those no longer published or seen: the body of [`present_areas_and_links`].
pub fn keep_views(
    looks: &mut AreaLooks,
    (areas, links): (&[AreaView], &[LinkView]),
    bubbles: &[BubbleRenderInstance],
    fog: Option<&BattleFog>,
    keep: &mut Keep<'_>,
) {
    for slot in &mut looks.areas {
        slot.kept = false;
    }
    for slot in &mut looks.links {
        slot.kept = false;
    }
    for area in areas {
        let sight = match fog {
            Some(fog) if fog.viewer() != Some(area.owner) => area_sight(
                Vec2::new(area.center[0], area.center[1]),
                area.radius,
                |x, y| fog.sees(x, y),
            ),
            _ => AreaSight::Whole,
        };
        keep_area(looks, area, sight, keep);
    }
    for link in links {
        keep_link(looks, link, bubbles, fog, keep);
    }
    let now = keep.now;
    looks.areas.retain(|slot| {
        if !slot.kept {
            keep.runtime.release(slot.handle, now);
        }
        slot.kept
    });
    looks.links.retain(|slot| {
        if !slot.kept {
            keep.runtime.release(slot.handle, now);
        }
        slot.kept
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_area_wholly_seen_or_unseen_is_whole_or_hidden() {
        let center = Vec2::new(10.0, 10.0);
        assert_eq!(area_sight(center, 1.5, |_, _| true), AreaSight::Whole);
        assert_eq!(area_sight(center, 1.5, |_, _| false), AreaSight::Hidden);
    }

    /// The clip of an area seen in part (asserted), or no clip.
    fn part(sight: AreaSight) -> LineClip {
        assert!(
            matches!(sight, AreaSight::Part(_)),
            "a half-seen area is seen in part: {sight:?}"
        );
        match sight {
            AreaSight::Part(clip) => clip,
            AreaSight::Whole | AreaSight::Hidden => NO_CLIP,
        }
    }

    #[test]
    fn an_area_seen_in_part_is_cut_where_the_fog_begins() {
        let center = Vec2::new(10.0, 10.0);
        // The viewer sees the half plane x >= 10.4.
        let [ax, az, min, max] = part(area_sight(center, 2.0, |x, _| x >= 10.4));
        assert!(
            ax > 0.99 && az.abs() < 0.05,
            "the axis points to the seen side"
        );
        assert!(
            (min - 10.4).abs() <= 0.26,
            "the cut lies at the fog's edge: {min}"
        );
        assert!(max > center.x + 2.0, "nothing is cut on the seen side");
        // A fog edge across the other axis turns the cut with it.
        let [ax, az, ..] = part(area_sight(center, 2.0, |_, y| y < 9.5));
        assert!(az < -0.99 && ax.abs() < 0.05);
    }

    #[test]
    fn a_tether_ends_on_both_bodies_surfaces() {
        let (from, to) = tether_ends(Vec3::ZERO, 0.5, Vec3::new(4.0, 0.0, 0.0), 0.75);
        assert!(from.distance(Vec3::new(0.5, 0.0, 0.0)) < 1e-6);
        assert!(to.distance(Vec3::new(3.25, 0.0, 0.0)) < 1e-6);
        let (from, to) = tether_ends(Vec3::ZERO, 0.5, Vec3::new(0.8, 0.0, 0.0), 0.5);
        assert_eq!((from, to), (Vec3::ZERO, Vec3::new(0.8, 0.0, 0.0)));
    }

    #[test]
    fn the_record_is_retained_at_its_capacity() {
        let looks = AreaLooks::default();
        assert_eq!(looks.held(), (0, 0));
        assert!(looks.capacities().0 >= HELD_SLOTS && looks.capacities().1 >= HELD_SLOTS);
    }
}
