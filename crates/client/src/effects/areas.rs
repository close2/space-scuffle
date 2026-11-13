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
use sim_types::{AbilitySource, AreaView, BubbleRenderInstance, EntityId, LinkView};

/// Seconds of one simulation tick.
const TICK_SECONDS: f32 = 1.0 / sim_types::TICKS_PER_SECOND as f32;

/// Areas, and tethers, whose looks are held at once; more are not drawn (counted in
/// [`AreaLooks::refused`]). A battle of 400 units has a few dozen spells live at most.
pub const HELD_SLOTS: usize = 64;

/// Effects an area lays (its flares, its billows) whose clip the area keeps in step with its
/// own sight: more than live at once for the shipped looks (a lay every 0.11 s living 0.8 s).
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
    let (mut seen_cells, mut unseen_cells) = (0_u32, 0_u32);
    for (_, seen) in disc_cells(center, radius, &sees) {
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
        let threshold = disc_cells(center, radius, &sees)
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

/// The looks held on the published areas and tethers.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct AreaLooks {
    /// Held areas, sorted by area id.
    areas: Vec<AreaSlot>,
    /// Held tethers, sorted by [`LinkKey`].
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
    if !shown(from) || !shown(to) {
        return;
    }
    let (from_end, to_end) = tether_ends(anchors(from).1, from.radius, anchors(to).1, to.radius);
    let line = WorldLine::whole(from_end, to_end);
    let bound = BoundValues::default().with(Binding::LinkLength, from_end.distance(to_end));
    let key = (link.from, link.to, link.source);
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

#[cfg(test)]
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
        disc_cells(center, radius, sees)
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
