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
//!   to `LinkLength`;
//! - a spell missile holds its ability's `Missile` look while it is published
//!   ([`keep_missiles`]), and its impact is laid by the simulation's `SpellImpact` event, where
//!   and when the impact ran ([`lay_spell_impact`]; plan `2026-11-17-effect-snapshot-identity`
//!   D-6, B-124). A missile that stops being published without an impact (it fizzled) lays
//!   nothing.
//!
//! # Fog (D-5), judged per frame
//! - An area of the viewer's own (or with no fog kept) is drawn whole. An enemy's area is drawn
//!   only where the viewer sees it: [`area_sight`] tests the fog cells the disc overlaps; an
//!   area wholly in fog holds nothing, and one seen in part is cut by a half plane that lies
//!   beyond every corner of every unseen cell (fail-closed by construction; a clip window across
//!   the disc, [`EffectRuntime::set_fog_clip`]). The cut also holds for the effects the area
//!   lays, which follow its current clip every frame.
//! - An enemy area the viewer stops seeing is cut to nothing on that frame ([`CLIP_NOTHING`]),
//!   with what it laid, and released: it erodes out unseen.
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
use sim_types::snapshot::CombatEventVisual;
use sim_types::{
    AbilitySource, AreaView, BubbleRenderInstance, EntityId, LinkView, PlayerId, SpellMissileView,
};

/// Seconds of one simulation tick.
const TICK_SECONDS: f32 = 1.0 / sim_types::TICKS_PER_SECOND as f32;

/// Areas, and tethers, whose looks are held at once; more are not drawn (counted in
/// [`AreaLooks::refused`]). A battle of 400 units has a few dozen spells live at most.
pub const HELD_SLOTS: usize = 64;

/// Effects an area lays (its flares, its billows) whose clip the area keeps in step with its
/// own sight: more than live at once for the shipped looks (the storm lays every 0.16 s for
/// 0.8 s, 5 alive; the cloud every 0.2 s for 1.4 s, 7 alive).
pub const CHILD_SLOTS: usize = 16;

/// Samples across an area's diameter, for choosing among cuts the one that keeps the most.
const SIGHT_SAMPLES: usize = 17;

/// Axes tried for the cut of an area seen in part, evenly round the turn.
const CUT_DIRECTIONS: usize = 32;

/// World units the cut keeps beyond the unseen cells: the decals' clip feather reaches
/// `0.6 × clip_feather` past a clip's end (`decal.wgsl`, `CLIP_FEATHER_OUTSIDE`), 0.009 for the
/// default feather; this margin covers feathers up to 0.08.
const CUT_MARGIN: f32 = 0.05;

/// A clip window no point lies inside: a look whose area the viewer stopped seeing draws
/// nothing while it erodes out (WP10 review 1, E1-2).
pub const CLIP_NOTHING: LineClip = [1.0, 0.0, 1.0e6, 2.0e6];

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

/// The fog cells, 1-unit squares with integer corners, that the disc of `radius` round
/// `center` overlaps, as `(lower corner, seen)`; `sees` is asked at each cell's centre.
fn disc_cells(
    center: Vec2,
    radius: f32,
    sees: &impl Fn(f32, f32) -> bool,
) -> impl Iterator<Item = (Vec2, bool)> + '_ {
    let low = (center - Vec2::splat(radius)).floor();
    let high = (center + Vec2::splat(radius)).floor();
    #[expect(
        clippy::cast_possible_truncation,
        reason = "cell indices of a map are far below i32::MAX"
    )]
    let (columns, rows) = ((high.x - low.x) as i32 + 1, (high.y - low.y) as i32 + 1);
    (0..rows).flat_map(move |row| {
        (0..columns).filter_map(move |column| {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a few cells across an area are exact in f32"
            )]
            let corner = low + Vec2::new(column as f32, row as f32);
            let nearest = center.clamp(corner, corner + Vec2::ONE);
            (nearest.distance(center) < radius)
                .then(|| (corner, sees(corner.x + 0.5, corner.y + 0.5)))
        })
    })
}

/// How the viewer sees the area of radius `radius` (world units) centred at `center` (map
/// `x, y`), where `sees(x, y)` says whether a map point lies in a visible cell. The fog is
/// kept per 1-unit cell with integer corners (`BattleFog::sees`).
///
/// With every cell the disc overlaps seen it is [`AreaSight::Whole`], with none
/// [`AreaSight::Hidden`]. Otherwise the fog's edge across so small a disc is close to a line,
/// and the area is cut by a half plane that is fail-closed by construction (D-5; WP10 review 1,
/// E1-1): along each of [`CUT_DIRECTIONS`] axes the cut lies [`CUT_MARGIN`] beyond the
/// farthest corner of any unseen cell the disc overlaps, so no unseen ground is drawn; of those
/// cuts the one that keeps the most seen ground (counted on a 17 × 17 grid of the disc) is
/// taken, and an area none of whose seen ground any cut keeps is [`AreaSight::Hidden`].
#[must_use]
pub fn area_sight(center: Vec2, radius: f32, sees: impl Fn(f32, f32) -> bool) -> AreaSight {
    // The cells are taken over the disc grown by the cut's margin, so an unseen cell that only
    // the look's soft lip or the clip's feather could reach is cut too (WP10 review 2, P2-1).
    let reach = radius + CUT_MARGIN;
    let (mut seen_cells, mut unseen_cells) = (0_u32, 0_u32);
    for (_, seen) in disc_cells(center, reach, &sees) {
        if seen {
            seen_cells += 1;
        } else {
            unseen_cells += 1;
        }
    }
    if seen_cells == 0 {
        return AreaSight::Hidden;
    }
    if unseen_cells == 0 {
        return AreaSight::Whole;
    }
    let mut best: Option<(u32, Vec2, f32)> = None;
    for direction in 0..CUT_DIRECTIONS {
        #[expect(
            clippy::cast_precision_loss,
            reason = "direction indices below 64 are exact in f32"
        )]
        let axis =
            Vec2::from_angle(direction as f32 * std::f32::consts::TAU / CUT_DIRECTIONS as f32);
        let threshold = disc_cells(center, reach, &sees)
            .filter(|(_, seen)| !*seen)
            .flat_map(|(corner, _)| {
                [Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ONE].map(|offset| (corner + offset).dot(axis))
            })
            .fold(f32::NEG_INFINITY, f32::max)
            + CUT_MARGIN;
        let kept = kept_samples(center, radius, axis, threshold, &sees);
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

/// Seen points of a 17 × 17 grid over the disc that lie on the drawn side of the cut
/// `dot(point, axis) >= threshold`.
fn kept_samples(
    center: Vec2,
    radius: f32,
    axis: Vec2,
    threshold: f32,
    sees: &impl Fn(f32, f32) -> bool,
) -> u32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample indices below 17 are exact in f32"
    )]
    let step = 2.0 * radius / (SIGHT_SAMPLES - 1) as f32;
    let mut kept = 0;
    for row in 0..SIGHT_SAMPLES {
        for column in 0..SIGHT_SAMPLES {
            #[expect(
                clippy::cast_precision_loss,
                reason = "sample indices below 17 are exact in f32"
            )]
            let offset = Vec2::new(column as f32, row as f32) * step - Vec2::splat(radius);
            let point = center + offset;
            if offset.length() <= radius && point.dot(axis) >= threshold && sees(point.x, point.y) {
                kept += 1;
            }
        }
    }
    kept
}

/// The ends of a tether between two bodies of radius `from_radius` and `to_radius` centred at
/// `from` and `to` (world space): on each body's surface, facing the other. Bodies that touch
/// or overlap give a tether of no length at their contact, halfway between the two surface
/// points, so the line shrinks to nothing as an Abduct pull arrives and never turns round or
/// runs across both bodies (plan Amendment 33, B-121). Coincident centres give `from` twice.
#[must_use]
pub fn tether_ends(from: Vec3, from_radius: f32, to: Vec3, to_radius: f32) -> (Vec3, Vec3) {
    let span = to - from;
    let length = span.length();
    let direction = span.normalize_or_zero();
    let (start, end) = (from + direction * from_radius, to - direction * to_radius);
    if length <= from_radius + to_radius {
        let contact = (start + end) * 0.5;
        return (contact, contact);
    }
    (start, end)
}

/// One held area.
#[derive(Debug, Clone, Copy, PartialEq)]
struct AreaSlot {
    /// The published area's id: the slots are kept sorted by it.
    id: u32,
    /// The effect held.
    effect: EffectId,
    /// Its live instance.
    handle: EffectHandle,
    /// The effects it laid lately, whose clip follows its own (stale handles are skipped).
    children: [Option<EffectHandle>; CHILD_SLOTS],
    /// The slot of `children` the next laid effect takes.
    next_child: usize,
    /// Whether it was kept this frame.
    kept: bool,
}

/// The key a tether's slot is sorted by: caster, target and ability.
type LinkKey = (EntityId, EntityId, AbilitySource);

/// One held tether.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LinkSlot {
    /// Caster, target and ability: the slots are kept sorted by it.
    key: LinkKey,
    /// The effect held.
    effect: EffectId,
    /// Its live instance.
    handle: EffectHandle,
    /// Whether it was kept this frame.
    kept: bool,
}

/// One held spell missile (plan WP11, F1).
#[derive(Debug, Clone, Copy, PartialEq)]
struct MissileSlot {
    /// The published missile's id: the slots are kept sorted by it.
    id: u32,
    /// Its live instance.
    handle: EffectHandle,
    /// Whether it was kept this frame.
    kept: bool,
}

/// The looks held on the published areas and tethers.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct AreaLooks {
    /// Held areas, sorted by area id.
    areas: Vec<AreaSlot>,
    /// Held tethers, sorted by [`LinkKey`].
    links: Vec<LinkSlot>,
    /// Held spell missiles, sorted by missile id.
    missiles: Vec<MissileSlot>,
    /// Areas and tethers not drawn because [`HELD_SLOTS`] were held.
    pub refused: u64,
}

impl Default for AreaLooks {
    fn default() -> Self {
        Self {
            areas: Vec::with_capacity(HELD_SLOTS),
            links: Vec::with_capacity(HELD_SLOTS),
            missiles: Vec::with_capacity(HELD_SLOTS),
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

    /// How many spell missiles hold a look now, and the retained capacity of their record.
    #[must_use]
    pub fn missiles(&self) -> (usize, usize) {
        (self.missiles.len(), self.missiles.capacity())
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

/// Cuts `slot`'s look and every effect it laid that is still live to `clip`.
fn clip_area(slot: &AreaSlot, clip: LineClip, runtime: &mut EffectRuntime) {
    runtime.set_fog_clip(slot.handle, clip);
    for child in slot.children.iter().flatten() {
        runtime.set_fog_clip(*child, clip);
    }
}

// PERFORMANCE RATIONALE:
// Why: Every frame each published area must find its held slot; a scan of the slots per area
// would be quadratic in the live areas (WP10 review 1, E1-3; charter §3.3).
// What: The slots stay sorted by area id: a binary search per area, and a new slot is inserted
// in place within the slots' retained capacity.
// Trade-off: An insertion moves the later slots (at most HELD_SLOTS small records), once per
// new area, in exchange for O(log N) per area and frame and no allocation.
/// Keeps `area`'s look held in `looks`, seen as `sight`. An area the viewer stopped seeing is
/// cut to nothing at once, its laid effects with it, and left unkept, so it is released and
/// erodes out unseen (E1-2).
fn keep_area(looks: &mut AreaLooks, area: &AreaView, sight: AreaSight, keep: &mut Keep<'_>) {
    let Some(effect) = ability_effect(keep.catalog, keep.tables, area.source, EventKind::Area)
    else {
        return;
    };
    let found = looks.areas.binary_search_by_key(&area.id, |slot| slot.id);
    let clip = match sight {
        AreaSight::Hidden => {
            if let Ok(index) = found {
                clip_area(&looks.areas[index], CLIP_NOTHING, keep.runtime);
            }
            return;
        }
        AreaSight::Whole => NO_CLIP,
        AreaSight::Part(clip) => clip,
    };
    let anchor = world(area.center);
    let bound = area_bound(area);
    let index = match found {
        Ok(index) if looks.areas[index].effect == effect => {
            if !keep
                .runtime
                .update_held(looks.areas[index].handle, anchor, bound)
            {
                return;
            }
            index
        }
        Ok(_) => return,
        Err(index) => {
            if looks.areas.len() == looks.areas.capacity() {
                looks.refused += 1;
                return;
            }
            // An area seen on the tick it opened comes in; one first seen later (out of the
            // fog, or after a scene load) is shown settled.
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
            looks.areas.insert(
                index,
                AreaSlot {
                    id: area.id,
                    effect,
                    handle,
                    children: [None; CHILD_SLOTS],
                    next_child: 0,
                    kept: false,
                },
            );
            index
        }
    };
    let slot = &mut looks.areas[index];
    slot.kept = true;
    lay(slot, area, (anchor, bound), keep);
    clip_area(slot, clip, keep.runtime);
}

/// Lays `slot`'s `emit` effect when its beat is due, remembering it as a child of the area.
fn lay(
    slot: &mut AreaSlot,
    area: &AreaView,
    (anchor, bound): (Vec3, BoundValues),
    keep: &mut Keep<'_>,
) {
    let Some(emit) = keep
        .catalog
        .effect(slot.effect)
        .and_then(|effect| effect.emit)
    else {
        return;
    };
    // The area's own phase, so two storms side by side do not flare in step.
    #[expect(
        clippy::cast_precision_loss,
        reason = "the phase needs only the low bits of the id"
    )]
    let phase = (area.id % 97) as f32 / 97.0;
    if !emit_due(keep.now, keep.delta, emit.every, phase) {
        return;
    }
    let request = SpawnRequest {
        effect: emit.effect,
        anchor,
        bound,
        line: None,
        delay: 0.0,
    };
    if let Some(laid) = keep.runtime.spawn(keep.catalog, &request, keep.now) {
        slot.children[slot.next_child] = Some(laid);
        slot.next_child = (slot.next_child + 1) % CHILD_SLOTS;
    }
}

/// The presented bubble of `id`, by binary search (the snapshot's bubbles are in entity order).
fn bubble_of(bubbles: &[BubbleRenderInstance], id: EntityId) -> Option<&BubbleRenderInstance> {
    bubbles
        .binary_search_by_key(&id, |bubble| bubble.entity_id)
        .ok()
        .and_then(|index| bubbles.get(index))
}

/// Keeps `link`'s look held in `looks`, its ends on the presented bubbles, while `shown`
/// says both are drawn as they are (slots found by binary search, as for the areas).
fn keep_link(
    looks: &mut AreaLooks,
    link: &LinkView,
    bubbles: &[BubbleRenderInstance],
    shown: &impl Fn(&BubbleRenderInstance) -> bool,
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
    let key = (link.from, link.to, link.source);
    if !shown(from) || !shown(to) {
        // An end left the viewer's sight: the tether goes at once, with no exit at its last line
        // (WP10 review 2, P2-4).
        if let Ok(index) = looks.links.binary_search_by_key(&key, |slot| slot.key) {
            keep.runtime.retire(looks.links[index].handle);
        }
        return;
    }
    let (from_end, to_end) = tether_ends(anchors(from).1, from.radius, anchors(to).1, to.radius);
    let line = WorldLine::whole(from_end, to_end);
    let bound = BoundValues::default().with(Binding::LinkLength, from_end.distance(to_end));
    match looks.links.binary_search_by_key(&key, |slot| slot.key) {
        Ok(index) if looks.links[index].effect == effect => {
            let slot = &mut looks.links[index];
            slot.kept = keep.runtime.update_held_line(slot.handle, line, bound);
        }
        Ok(_) => {}
        Err(index) => {
            if looks.links.len() == looks.links.capacity() {
                looks.refused += 1;
                return;
            }
            let request = SpawnRequest {
                effect,
                anchor: from_end,
                bound,
                line: Some(line),
                delay: 0.0,
            };
            if let Some(handle) = keep.runtime.hold(keep.catalog, &request, keep.now, false) {
                looks.links.insert(
                    index,
                    LinkSlot {
                        key,
                        effect,
                        handle,
                        kept: true,
                    },
                );
            }
        }
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
    let missiles = session
        .as_deref()
        .map_or((&[][..], &[][..], 0.0), |session| {
            session.frames.presented_missiles()
        });
    keep_missiles(&mut looks, missiles, fog.as_deref(), &mut keep);
}

/// How the viewer under `fog` (`None`: no fog kept) sees `area`: its own whole, another's by
/// [`area_sight`] against the visible cells.
#[must_use]
pub fn sight_under(fog: Option<&BattleFog>, area: &AreaView) -> AreaSight {
    match fog {
        Some(fog) if fog.viewer() != Some(area.owner) => area_sight(
            Vec2::new(area.center[0], area.center[1]),
            area.radius,
            |x, y| fog.sees(x, y),
        ),
        _ => AreaSight::Whole,
    }
}

/// Brings `looks` up to the published `views` (areas and tethers) of a frame whose presented
/// bubbles are `bubbles` (in entity order) under `fog` (`None`: no fog kept), and releases the
/// looks of those no longer published or seen: the body of [`present_areas_and_links`].
pub fn keep_views(
    looks: &mut AreaLooks,
    views: (&[AreaView], &[LinkView]),
    bubbles: &[BubbleRenderInstance],
    fog: Option<&BattleFog>,
    keep: &mut Keep<'_>,
) {
    keep_views_seen(
        looks,
        views,
        bubbles,
        |area| sight_under(fog, area),
        |bubble| presence_of(fog, bubble) == Presence::Shown,
        keep,
    );
}

/// [`keep_views`] with the fog's two questions given: how each area is seen (`sight`) and
/// whether a bubble is drawn as it is (`shown`); tests ask them directly.
pub fn keep_views_seen(
    looks: &mut AreaLooks,
    (areas, links): (&[AreaView], &[LinkView]),
    bubbles: &[BubbleRenderInstance],
    sight: impl Fn(&AreaView) -> AreaSight,
    shown: impl Fn(&BubbleRenderInstance) -> bool,
    keep: &mut Keep<'_>,
) {
    for slot in &mut looks.areas {
        slot.kept = false;
    }
    for slot in &mut looks.links {
        slot.kept = false;
    }
    for area in areas {
        keep_area(looks, area, sight(area), keep);
    }
    for link in links {
        keep_link(looks, link, bubbles, &shown, keep);
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

/// Radius in world units round a flying missile that the viewer must see for its look to be
/// drawn: about the size of the shipped missile looks.
const MISSILE_SIGHT_RADIUS: f32 = 0.35;

/// The spell missiles of a presented instant: those of the tick the pose is interpolated
/// from, those of the tick it is interpolated towards (each in missile id order), and the
/// fraction between them (`BattleFrameState::presented_missiles`).
pub type MissileFrame<'a> = (&'a [SpellMissileView], &'a [SpellMissileView], f32);

/// How the viewer under `fog` (`None`: no fog kept) sees a disc of `radius` round `center`
/// (map `x, y`) of `owner`'s: whole when it is the viewer's own, else by [`area_sight`].
fn disc_sight_under(
    fog: Option<&BattleFog>,
    owner: PlayerId,
    center: Vec2,
    radius: f32,
) -> AreaSight {
    match fog {
        Some(fog) if fog.viewer() != Some(owner) => {
            area_sight(center, radius, |x, y| fog.sees(x, y))
        }
        _ => AreaSight::Whole,
    }
}

/// Keeps a look held on every published spell missile (plan WP11, F1), under `fog` (`None`: no
/// fog kept): the missile part of [`present_areas_and_links`].
pub fn keep_missiles(
    looks: &mut AreaLooks,
    missiles: MissileFrame<'_>,
    fog: Option<&BattleFog>,
    keep: &mut Keep<'_>,
) {
    keep_missiles_seen(
        looks,
        missiles,
        |owner, center, radius| disc_sight_under(fog, owner, center, radius),
        keep,
    );
}

// PERFORMANCE RATIONALE:
// Why: Every frame each published missile must find its held slot and its position on the
// next tick; scans would be quadratic in the live missiles (charter §3.3).
// What: The slots stay sorted by missile id and the snapshots publish missiles in id order, so
// each lookup is a binary search, and a new slot is inserted within the retained capacity.
// Trade-off: An insertion moves the later slots (at most HELD_SLOTS small records), once per
// new missile, in exchange for O(log N) per missile and frame and no allocation.
/// [`keep_missiles`] with the fog's question given: how the viewer sees a disc of a player's
/// round a map point (`sight(owner, center, radius)`); tests ask it directly.
///
/// A missile's look is held at its position, interpolated between the two ticks of the
/// presented instant, and drawn only while the viewer sees the ground round it. A missile no
/// longer published has its look released, and nothing more: whether and where it landed is
/// the simulation's `SpellImpact` event, laid by [`lay_spell_impact`] (plan
/// `2026-11-17-effect-snapshot-identity` D-6, B-124), so a missile that fizzled lays nothing.
pub fn keep_missiles_seen(
    looks: &mut AreaLooks,
    (from, to, fraction): MissileFrame<'_>,
    sight: impl Fn(PlayerId, Vec2, f32) -> AreaSight,
    keep: &mut Keep<'_>,
) {
    for slot in &mut looks.missiles {
        slot.kept = false;
    }
    for missile in from {
        let Some(effect) = ability_effect(
            keep.catalog,
            keep.tables,
            Some(missile.source),
            EventKind::Missile,
        ) else {
            continue;
        };
        let next = to
            .binary_search_by_key(&missile.id, |next| next.id)
            .ok()
            .and_then(|index| to.get(index))
            .map_or(missile.position, |next| next.position);
        let start = Vec3::from_array(missile.position);
        let position = start + (Vec3::from_array(next) - start) * fraction.clamp(0.0, 1.0);
        let anchor = world(position.to_array());
        let reach = keep
            .catalog
            .effect(effect)
            .map_or(0.0, |effect| effect.reach)
            .max(MISSILE_SIGHT_RADIUS);
        let clip = match sight(missile.owner, Vec2::new(position.x, position.y), reach) {
            AreaSight::Whole => NO_CLIP,
            AreaSight::Part(clip) => clip,
            AreaSight::Hidden => CLIP_NOTHING,
        };
        let handle = match looks
            .missiles
            .binary_search_by_key(&missile.id, |slot| slot.id)
        {
            Ok(index) => {
                let slot = &mut looks.missiles[index];
                slot.kept = keep
                    .runtime
                    .update_held(slot.handle, anchor, BoundValues::default());
                slot.handle
            }
            Err(index) => {
                if looks.missiles.len() == looks.missiles.capacity() {
                    looks.refused += 1;
                    continue;
                }
                let request = SpawnRequest {
                    effect,
                    anchor,
                    bound: BoundValues::default(),
                    line: None,
                    delay: 0.0,
                };
                let Some(handle) = keep.runtime.hold(keep.catalog, &request, keep.now, false)
                else {
                    continue;
                };
                looks.missiles.insert(
                    index,
                    MissileSlot {
                        id: missile.id,
                        handle,
                        kept: true,
                    },
                );
                handle
            }
        };
        keep.runtime.set_fog_clip(handle, clip);
        shed(missile.id, effect, anchor, clip, keep);
    }
    // One pass: the slots no longer published are released and go, so many missiles landing on
    // one frame cost one pass (review 1, E1-7).
    let now = keep.now;
    looks.missiles.retain(|slot| {
        if !slot.kept {
            keep.runtime.release(slot.handle, now);
        }
        slot.kept
    });
}

/// Lays the `emit` effect of missile `id`'s look `effect` at `anchor` when its beat is due (a
/// trail of motes it sheds), cut to the missile's own `clip`.
fn shed(id: u32, effect: EffectId, anchor: Vec3, clip: LineClip, keep: &mut Keep<'_>) {
    let Some(emit) = keep.catalog.effect(effect).and_then(|effect| effect.emit) else {
        return;
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "the phase needs only the low bits of the id"
    )]
    let phase = (id % 89) as f32 / 89.0;
    if !emit_due(keep.now, keep.delta, emit.every, phase) {
        return;
    }
    let request = SpawnRequest {
        effect: emit.effect,
        anchor,
        bound: BoundValues::default(),
        line: None,
        delay: 0.0,
    };
    if let Some(laid) = keep.runtime.spawn(keep.catalog, &request, keep.now) {
        keep.runtime.set_fog_clip(laid, clip);
    }
}

/// Lays the impact of a spell missile the simulation landed, from its `SpellImpact` `event`,
/// under `fog` (`None`: no fog kept); see [`lay_spell_impact_seen`]. Any other event lays
/// nothing.
pub fn lay_spell_impact(
    event: &CombatEventVisual,
    fog: Option<&BattleFog>,
    keep: &mut Keep<'_>,
) -> Option<EffectHandle> {
    lay_spell_impact_seen(
        event,
        |owner, center, radius| disc_sight_under(fog, owner, center, radius),
        keep,
    )
}

/// [`lay_spell_impact`] with the fog's question given: how the viewer sees a disc of a
/// player's round a map point (`sight(owner, center, radius)`); tests ask it directly.
///
/// The impact is the `impact` of the `Missile` look the caster's block names for the ability
/// (`source`), laid at the event's `pos`, where the simulation ran it (for Fungal Growth the
/// aimed point; plan `2026-11-17-effect-snapshot-identity` D-6, B-124), on the frame the
/// presentation reaches the impact's tick, bound to the extent the spell catalog gives the
/// ability ([`UnitEffectTables::missile_extent`]). The fog cuts it as an area of its whole reach
/// (D-5): in part where the viewer sees part of it, not at all where the viewer sees none of it.
/// Returns the laid impact.
pub fn lay_spell_impact_seen(
    event: &CombatEventVisual,
    sight: impl Fn(PlayerId, Vec2, f32) -> AreaSight,
    keep: &mut Keep<'_>,
) -> Option<EffectHandle> {
    let CombatEventVisual::SpellImpact {
        pos, owner, source, ..
    } = event
    else {
        return None;
    };
    let look = ability_effect(keep.catalog, keep.tables, Some(*source), EventKind::Missile)?;
    let impact = keep.catalog.effect(look)?.impact?;
    let extent = keep.tables.missile_extent(*source);
    let reach = keep
        .catalog
        .effect(impact)
        .map_or(0.0, |effect| effect.reach)
        .max(extent.radius)
        .max(MISSILE_SIGHT_RADIUS);
    let clip = match sight(*owner, Vec2::new(pos[0], pos[1]), reach) {
        AreaSight::Whole => NO_CLIP,
        AreaSight::Part(clip) => clip,
        AreaSight::Hidden => return None,
    };
    let request = SpawnRequest {
        effect: impact,
        anchor: world(*pos),
        bound: BoundValues::default()
            .with(Binding::AreaRadius, extent.radius)
            .with(Binding::AreaTotal, extent.seconds)
            .with(Binding::AreaRemaining, extent.seconds),
        line: None,
        delay: 0.0,
    };
    let handle = keep.runtime.spawn(keep.catalog, &request, keep.now)?;
    keep.runtime.set_fog_clip(handle, clip);
    Some(handle)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed test is a panic"
)]
mod tests {
    use super::*;

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

    /// A fog of 1-unit cells (as `BattleFog::sees` keeps it) whose cell `(column, row)` is seen
    /// when `seen_cell` says so.
    fn cells(seen_cell: impl Fn(i32, i32) -> bool) -> impl Fn(f32, f32) -> bool {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "test coordinates are small"
        )]
        move |x: f32, y: f32| seen_cell(x.floor() as i32, y.floor() as i32)
    }

    /// Whether any corner of an unseen cell the disc overlaps lies on the drawn side of `clip`.
    fn unseen_corner_drawn(
        center: Vec2,
        radius: f32,
        clip: LineClip,
        sees: &impl Fn(f32, f32) -> bool,
    ) -> bool {
        let axis = Vec2::new(clip[0], clip[1]);
        disc_cells(center, radius + CUT_MARGIN, sees)
            .filter(|(_, seen)| !*seen)
            .flat_map(|(corner, _)| [Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ONE].map(|o| corner + o))
            .any(|corner| corner.dot(axis) >= clip[2])
    }

    #[test]
    fn an_area_wholly_seen_or_unseen_is_whole_or_hidden() {
        let center = Vec2::new(10.0, 10.0);
        assert_eq!(area_sight(center, 1.5, |_, _| true), AreaSight::Whole);
        assert_eq!(area_sight(center, 1.5, |_, _| false), AreaSight::Hidden);
    }

    #[test]
    fn an_area_seen_in_part_is_cut_where_the_fog_begins() {
        let center = Vec2::new(10.0, 10.0);
        // The viewer sees the cells from column 11 on.
        let sees = cells(|column, _| column >= 11);
        let [ax, az, min, max] = part(area_sight(center, 2.0, &sees));
        assert!(
            ax > 0.99 && az.abs() < 0.05,
            "the axis points to the seen side"
        );
        assert!(
            (min - (11.0 + CUT_MARGIN)).abs() < 1e-4,
            "the cut lies on the fog's edge: {min}"
        );
        assert!(max > center.x + 2.0, "nothing is cut on the seen side");
        // A fog edge across the other axis turns the cut with it.
        let [ax, az, ..] = part(area_sight(center, 2.0, cells(|_, row| row < 9)));
        assert!(az < -0.99 && ax.abs() < 0.05);
    }

    /// E1-1 (WP10 review 1): the fog's edge between two samples of the grid. A cut half a sample
    /// step beyond the last unseen sample would draw unseen ground; the cut from the unseen
    /// cells' corners draws none, for an edge along either axis and for a diagonal staircase.
    #[test]
    fn the_cut_draws_no_corner_of_an_unseen_cell() {
        // Samples of a radius-2 disc centred at x = 10.1 lie at 8.1 + 0.25 k: 10.85 and 11.1
        // straddle the edge at 11.
        let center = Vec2::new(10.1, 10.3);
        let straight = cells(|column, _| column >= 11);
        let clip = part(area_sight(center, 2.0, &straight));
        assert!(
            clip[2] >= 11.0,
            "the cut lies beyond the last unseen cell: {clip:?}"
        );
        assert!(!unseen_corner_drawn(center, 2.0, clip, &straight));
        let staircase = cells(|column, row| column + row >= 21);
        let clip = part(area_sight(center, 2.0, &staircase));
        assert!(!unseen_corner_drawn(center, 2.0, clip, &staircase));
        let storm = cells(|column, row| column >= 10 && row >= 10);
        let clip = part(area_sight(Vec2::new(10.2, 10.4), 1.5, &storm));
        assert!(!unseen_corner_drawn(
            Vec2::new(10.2, 10.4),
            1.5,
            clip,
            &storm
        ));
    }

    /// P2-1 (WP10 review 2): an unseen cell that only the annulus between the radius and the
    /// cut's margin reaches is cut too.
    #[test]
    fn an_unseen_cell_just_past_the_radius_is_cut() {
        // The disc of radius 1.5 round (10.5, 10.5) reaches x = 12.0 exactly; the cell from
        // column 12 on is unseen.
        let center = Vec2::new(10.5, 10.5);
        let sees = cells(|column, _| column < 12);
        let clip = part(area_sight(center, 1.5, &sees));
        assert!(!unseen_corner_drawn(center, 1.5, clip, &sees));
        assert!(
            clip[0] < -0.99,
            "the cut faces away from the unseen cells: {clip:?}"
        );
    }

    #[test]
    fn clip_nothing_draws_nothing_anywhere_on_a_map() {
        let axis = Vec2::new(CLIP_NOTHING[0], CLIP_NOTHING[1]);
        assert!(Vec2::splat(10_000.0).dot(axis) < CLIP_NOTHING[2]);
    }

    #[test]
    fn a_tether_ends_on_both_bodies_surfaces() {
        let (from, to) = tether_ends(Vec3::ZERO, 0.5, Vec3::new(4.0, 0.0, 0.0), 0.75);
        assert!(from.distance(Vec3::new(0.5, 0.0, 0.0)) < 1e-6);
        assert!(to.distance(Vec3::new(3.25, 0.0, 0.0)) < 1e-6);
    }

    /// B-121: as the bodies close in, the tether shortens to nothing at their contact and
    /// never turns round: its direction never opposes the caster-to-victim direction.
    #[test]
    fn an_arriving_tether_shrinks_to_its_contact_and_never_turns_round() {
        let mut previous = f32::INFINITY;
        for step in 0..=60 {
            #[expect(clippy::cast_precision_loss, reason = "a small step count")]
            let gap = 3.0 - step as f32 * 0.05;
            let victim = Vec3::new(gap, 0.0, 0.0);
            let (from, to) = tether_ends(Vec3::ZERO, 0.5, victim, 0.75);
            let length = from.distance(to);
            assert!(length <= previous + 1e-6, "at {gap}: it only shortens");
            assert!(
                (to - from).dot(victim) >= 0.0,
                "at {gap}: it points from the caster to the victim"
            );
            previous = length;
        }
        let touching = tether_ends(Vec3::ZERO, 0.5, Vec3::new(1.25, 0.0, 0.0), 0.75);
        assert!(touching.0.distance(Vec3::new(0.5, 0.0, 0.0)) < 1e-6);
        assert_eq!(touching.0, touching.1, "touching bodies: no line");
        let (from, to) = tether_ends(Vec3::ZERO, 0.5, Vec3::new(0.8, 0.0, 0.0), 0.5);
        assert_eq!(from, to, "overlapping bodies: no line");
        assert!(from.distance(Vec3::new(0.4, 0.0, 0.0)) < 1e-6, "halfway");
        let (from, to) = tether_ends(Vec3::ONE, 0.5, Vec3::ONE, 0.5);
        assert_eq!((from, to), (Vec3::ONE, Vec3::ONE), "coincident centres");
    }

    #[test]
    fn the_record_is_retained_at_its_capacity() {
        let looks = AreaLooks::default();
        assert_eq!(looks.held(), (0, 0));
        assert!(looks.capacities().0 >= HELD_SLOTS && looks.capacities().1 >= HELD_SLOTS);
    }

    /// The shipped vocabulary and the shipped units' tables.
    fn shipped() -> (EffectCatalog, UnitEffectTables) {
        let root = crate::ResolvedAssetsRoot::resolve().expect("the assets root resolves");
        let catalog = EffectCatalog::load(
            &root
                .as_path()
                .join(super::super::catalog::EFFECTS_CATALOG_PATH),
            root.as_path(),
        )
        .expect("the shipped vocabulary loads");
        let units = sim_fixtures::shipped_catalogs().units;
        let tables = UnitEffectTables::build(&units, &catalog).expect("every block resolves");
        (catalog, tables)
    }

    /// The first shipped ability whose caster's block names a `Missile` look with an impact:
    /// the ability and that impact. Found by what the data names, never by name (C5).
    fn missile_ability(
        catalog: &EffectCatalog,
        tables: &UnitEffectTables,
    ) -> (AbilitySource, EffectId) {
        sim_fixtures::shipped_catalogs()
            .units
            .archetypes()
            .find_map(|archetype| {
                (0..archetype.abilities.len()).find_map(|index| {
                    let source = AbilitySource {
                        unit_type: archetype.unit_type_id,
                        ability: u8::try_from(index).ok()?,
                    };
                    let look = ability_effect(catalog, tables, Some(source), EventKind::Missile)?;
                    Some((source, catalog.effect(look)?.impact?))
                })
            })
            .expect("a shipped caster names a missile look with an impact")
    }

    /// A `SpellImpact` of `source` at map `(x, y)`, altitude `z`.
    fn impact_at(source: AbilitySource, missile: u32, [x, y, z]: [f32; 3]) -> CombatEventVisual {
        CombatEventVisual::SpellImpact {
            pos: [x, y, z],
            missile,
            owner: PlayerId(1),
            source,
        }
    }

    /// The anchors of the live instances of `effect`.
    fn anchors_of(runtime: &EffectRuntime, effect: EffectId) -> Vec<Vec3> {
        runtime
            .pool
            .iter()
            .filter(|(_, instance)| instance.effect == effect)
            .map(|(_, instance)| instance.anchor)
            .collect()
    }

    #[test]
    fn an_impact_is_laid_where_the_event_says() {
        let (catalog, tables) = shipped();
        let (source, impact) = missile_ability(&catalog, &tables);
        let extent = tables.missile_extent(source);
        let mut runtime = EffectRuntime::new(&catalog);
        let mut keep = Keep {
            catalog: &catalog,
            tables: &tables,
            runtime: &mut runtime,
            now: 1.0,
            delta: 0.04,
        };
        let laid = lay_spell_impact_seen(
            &impact_at(source, 3, [7.25, 4.5, 0.3]),
            |_, _, _| AreaSight::Whole,
            &mut keep,
        );
        assert!(laid.is_some(), "the impact is laid");
        // No other event lays an impact.
        let splash = CombatEventVisual::SplashVisual {
            center: [1.0, 1.0, 0.0],
            zones: [0.5, 0.8, 1.0],
            line: None,
            source: sim_types::EffectSource::default(),
            owner: PlayerId(1),
        };
        assert!(lay_spell_impact_seen(&splash, |_, _, _| AreaSight::Whole, &mut keep).is_none());
        assert_eq!(runtime.spawned_of(impact), 1);
        assert_eq!(
            anchors_of(&runtime, impact),
            vec![Vec3::new(7.25, 0.3, 4.5)]
        );
        let instance = runtime
            .pool
            .iter()
            .find(|(_, instance)| instance.effect == impact)
            .map(|(_, instance)| instance.bound)
            .expect("the impact is live");
        assert!((instance.get(Binding::AreaRadius) - extent.radius).abs() < f32::EPSILON);
        assert!((instance.get(Binding::AreaTotal) - extent.seconds).abs() < f32::EPSILON);
        // An impact the viewer sees none of is not laid.
        let mut keep = Keep {
            catalog: &catalog,
            tables: &tables,
            runtime: &mut runtime,
            now: 1.0,
            delta: 0.04,
        };
        let hidden = lay_spell_impact_seen(
            &impact_at(source, 4, [20.0, 20.0, 0.0]),
            |_, _, _| AreaSight::Hidden,
            &mut keep,
        );
        assert!(hidden.is_none());
        assert_eq!(runtime.spawned_of(impact), 1);
    }

    /// Review condition 2 (plan `2026-11-17-effect-snapshot-identity` Amendment 3): through a
    /// real [`BattleFog`], an impact the viewer sees none of lays nothing; one whose edge the
    /// viewer sees, and the viewer's own, are laid.
    #[test]
    fn a_real_fog_hiding_the_whole_impact_lays_nothing() {
        use crate::battle_pipeline::commands::SessionGeneration;
        use sim_types::{PlayerView, VisibilityState};
        const SIDE: u16 = 24;
        const SEEN_COLUMNS: u16 = 8;
        let (side, seen_columns) = (u32::from(SIDE), u32::from(SEEN_COLUMNS));
        let (catalog, tables) = shipped();
        let (source, impact) = missile_ability(&catalog, &tables);
        // The viewer, player 0, sees the columns left of x = 8 and nothing else.
        let view = PlayerView {
            viewer: PlayerId(0),
            map_width: side,
            map_height: side,
            visibility: (0..side * side)
                .map(|cell| {
                    if cell % side < seen_columns {
                        VisibilityState::Visible
                    } else {
                        VisibilityState::Fogged
                    }
                })
                .collect(),
            ..PlayerView::default()
        };
        let mut fog = BattleFog::default();
        fog.refill(SessionGeneration::FIRST, &view, std::iter::empty());
        assert_eq!(fog.viewer(), Some(PlayerId(0)));
        let mut runtime = EffectRuntime::new(&catalog);
        let mut keep = Keep {
            catalog: &catalog,
            tables: &tables,
            runtime: &mut runtime,
            now: 1.0,
            delta: 0.04,
        };
        // The enemy's impact far inside the unseen columns, its whole reach unseen.
        let reach = keep
            .catalog
            .effect(impact)
            .map_or(0.0, |effect| effect.reach)
            .max(tables.missile_extent(source).radius);
        let far = f32::from(SEEN_COLUMNS) + reach + 2.0;
        assert!(far + reach < f32::from(SIDE), "the reach fits on the map");
        let hidden = impact_at(source, 1, [far, 12.0, 0.0]);
        assert!(fog.reveals(&hidden), "the fog delivers every impact");
        assert!(lay_spell_impact(&hidden, Some(&fog), &mut keep).is_none());
        assert_eq!(keep.runtime.spawned_of(impact), 0, "nothing is laid");
        // Its edge seen: laid, cut where the fog begins.
        let edge = impact_at(source, 2, [f32::from(SEEN_COLUMNS) + 0.5, 12.0, 0.0]);
        assert!(lay_spell_impact(&edge, Some(&fog), &mut keep).is_some());
        // The viewer's own impact in the unseen columns: laid whole.
        let own = CombatEventVisual::SpellImpact {
            pos: [far, 12.0, 0.0],
            missile: 3,
            owner: PlayerId(0),
            source,
        };
        assert!(lay_spell_impact(&own, Some(&fog), &mut keep).is_some());
        assert_eq!(keep.runtime.spawned_of(impact), 2);
    }

    #[test]
    fn a_vanished_missile_without_an_impact_lays_nothing() {
        let (catalog, tables) = shipped();
        let (source, impact) = missile_ability(&catalog, &tables);
        let mut runtime = EffectRuntime::new(&catalog);
        let mut looks = AreaLooks::default();
        let flying = [SpellMissileView {
            id: 9,
            owner: PlayerId(1),
            position: [3.0, 3.0, 1.0],
            source,
        }];
        // Published for two frames, then gone with no `SpellImpact` (it fizzled).
        for (index, frame) in [&flying[..], &flying[..], &[][..]].into_iter().enumerate() {
            let mut keep = Keep {
                catalog: &catalog,
                tables: &tables,
                runtime: &mut runtime,
                #[expect(clippy::cast_precision_loss, reason = "a frame index")]
                now: index as f64 * 0.04,
                delta: 0.04,
            };
            keep_missiles_seen(
                &mut looks,
                (frame, frame, 0.0),
                |_, _, _| AreaSight::Whole,
                &mut keep,
            );
            assert_eq!(
                looks.missiles().0,
                usize::from(!frame.is_empty()),
                "frame {index}"
            );
        }
        assert_eq!(runtime.spawned_of(impact), 0, "no impact without the event");
    }

    /// Several ticks run in one frame at high battle speed: every tick's impact is still laid,
    /// each where its event says, because the impacts are events, which the timeline queues
    /// tick by tick, not a frame list of the two snapshots it presents (D-6).
    #[test]
    fn impacts_of_skipped_ticks_are_still_laid() {
        use crate::battle_pipeline::commands::SessionGeneration;
        use crate::battle_pipeline::timeline::BattleFrameState;
        let (catalog, tables) = shipped();
        let (source, impact) = missile_ability(&catalog, &tables);
        let mut frames = BattleFrameState::default();
        frames.initialize(SessionGeneration::FIRST).expect("fresh");
        frames.finish_initialization().expect("initialized");
        for (tick, at) in [(1, [4.0, 5.0, 0.0]), (2, [9.0, 2.0, 0.5])] {
            let snapshot = frames.begin_tick().expect("ready");
            snapshot.tick = sim_types::Tick(tick);
            snapshot.combat_events = vec![impact_at(source, tick, at)];
            frames.finish_tick().expect("published");
        }
        // One presentation of the latest tick: the snapshot before it, tick 1, is gone.
        frames.present(1.0).expect("presented");
        let mut runtime = EffectRuntime::new(&catalog);
        let mut keep = Keep {
            catalog: &catalog,
            tables: &tables,
            runtime: &mut runtime,
            now: 0.5,
            delta: 0.04,
        };
        let mut shown = 0;
        while let Some(event) = frames.pop_presented_event() {
            shown += 1;
            assert!(
                lay_spell_impact_seen(&event.event, |_, _, _| AreaSight::Whole, &mut keep)
                    .is_some()
            );
        }
        assert_eq!(shown, 2, "both ticks' impacts are shown");
        let mut laid = anchors_of(&runtime, impact);
        laid.sort_by(|a, b| a.x.total_cmp(&b.x));
        assert_eq!(
            laid,
            vec![Vec3::new(4.0, 0.0, 5.0), Vec3::new(9.0, 0.5, 2.0)]
        );
    }
}
