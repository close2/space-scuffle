//! What the minimap draws over the terrain: resources, structures and units as squares (plan
//! `2026-12-01-minimap` D-5).
//!
//! # Responsibility
//! [`build_marks`] turns one presented snapshot, the viewer's fog and the map's billboards into
//! the ordered list of [`MinimapMark`]s the marks pass draws, in draw order:
//! 1. **resources** the viewer last saw present, in `ResourceUnitColor` (`core`
//!    `GameUIData:688`), then watchtowers;
//! 2. **remembered structures**: structures of other players the viewer was shown and does not
//!    see now, as it last saw them, sized and coloured as a live structure, times
//!    `remembered_alpha` (`FogVisibility Snapshot`; plan Amendment 1, D-17, B-252);
//! 3. **structures**, then
//! 4. **units**, so that a unit stays visible on top of a base;
//! 5. **attack snapshots**: B-183's still image of an attacker the viewer cannot see, one square
//!    sized and coloured from the image's record exactly as a live unit is, times
//!    `snapshot_alpha`, for exactly as long as the view lists it (D-6, closes B-204);
//! 6. **rally lines** of the selected own structures: from the structure through each slot's
//!    waypoints in `(slot, index)` order, in `rally_color` (D-12);
//! 7. **rings**: alert and player pings, which the shader animates from the presented tick (D-13).
//!
//! One rule decides which units are listed, the world's own: [`presence_of`] must say
//! [`Presence::Shown`] and the archetype must be selectable and order-targetable, the capability
//! test the world's picking applies. So no enemy in fog, no undetected burrowed or cloaked enemy,
//! no housed harvester and no hangar member is ever listed, and an attack-revealed enemy is. The
//! shader never hides a mark: what is listed is what the viewer may see (D-4).
//!
//! # Sizes and colours
//! A square's half-width is half the larger side of a structure's footprint, or a unit's radius,
//! in world units, at least `min_dot_px / 2` pixels (`MinUnitDotSize`, `core` `GameUIData:696`),
//! with a `border_px` border in `border_color` (`:691`, `:693`). Centres snap to pixel centres,
//! so a square and its border are symmetric. [`ColorMode::Player`] colours each owner from the
//! player palette; [`ColorMode::Team`] uses `StarCraft` II's team colours (`OverrideColors`,
//! `:561–590`): the viewer's units `self`, selected ones `selected`, every other player
//! `hostile` (no teams exist), neutral `neutral`. In both modes a unit of the viewer struck within
//! `flash_ticks` is `attacked_color` (`OverrideColors` `Attacked`, D-13).
//!
//! # Subjects
//! Beside each mark the builder records its [`MarkSubject`]: the resource node or the live unit
//! it shows. The minimap's pick ([`super::pick`]) reads them, so what a press picks is exactly
//! what is drawn there.
//!
//! # Invariants
//! - The list is a pure function of its inputs: two builds from the same inputs are equal.
//! - `subjects` has one entry per mark, index for index.
//! - The output vector is cleared and refilled, so a rebuild at or below its high-water mark
//!   allocates nothing.

use sim_core::catalog::UnitCatalog;
use sim_map::grid::ResourceNode;
use sim_types::{AttackSnapshotView, BubbleRenderInstance, EntityId, PlayerId, RallyView};

use super::alerts::AlertLog;
use super::layout::MinimapLayout;
use super::style::MinimapStyle;
use super::{ColorMode, MinimapMark};
use crate::battle_pipeline::fog::{presence_of, BattleFog, Presence};
use crate::player_presentation::PlayerPalette;
use crate::runtime_battle::MapObjectSnapshot;

/// The `kind` of a filled square with a border.
pub const MARK_SQUARE: u32 = 0;

/// The `kind` of a segment from `a` to `b`, `stroke` pixels wide (the shader draws the camera
/// frustum's edges with it, from the uniform).
pub const MARK_SEGMENT: u32 = 1;

/// The `kind` of a ring centred on `a` whose radius grows from `b[0]` to `b[1]` pixels and whose
/// opacity falls to zero over `extra` ticks from `start_tick`, `stroke` pixels wide.
pub const MARK_RING: u32 = 2;

/// What a mark shows, for picking on the minimap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkSubject {
    /// The resource node of this index in map order.
    Node(u32),
    /// A live unit or structure.
    Bubble(EntityId),
    /// A still image of a unit or structure, never picked.
    Image,
    /// Anything else (a watchtower, a line, a ring), never picked.
    Other,
}

/// Everything a build reads.
#[derive(Clone, Copy)]
pub struct MarkSources<'a> {
    /// The mapping of the map onto the image.
    pub layout: MinimapLayout,
    /// Sizes and colours.
    pub style: &'a MinimapStyle,
    /// Player or team colours.
    pub mode: ColorMode,
    /// The player the battle is presented to.
    pub viewer: Option<PlayerId>,
    /// The viewer's knowledge; `None` shows the truth (no minimap exists then, but the rule is
    /// the world's).
    pub fog: Option<&'a BattleFog>,
    /// The presented snapshot's bubbles, sorted by entity.
    pub bubbles: &'a [BubbleRenderInstance],
    /// The battle's catalog, for the capability test.
    pub catalog: Option<&'a UnitCatalog>,
    /// The player palette, for [`ColorMode::Player`].
    pub palette: Option<&'a PlayerPalette>,
    /// The map's billboards: resources in map order, then watchtowers.
    pub objects: &'a [MapObjectSnapshot],
    /// The map's resource nodes, in map order, for their footprints.
    pub nodes: &'a [ResourceNode],
    /// The selected entities, sorted.
    pub selected: &'a [EntityId],
    /// The viewer's attack snapshots, sorted by entity.
    pub snapshots: &'a [AttackSnapshotView],
    /// The viewer's rally waypoints, sorted by entity, then slot and index.
    pub rallies: &'a [RallyView],
    /// Alerts, pings and struck units; `None` draws none.
    pub alerts: Option<&'a AlertLog>,
    /// The presented tick.
    pub tick: u32,
}

/// Clears `out` and fills it with the marks of `sources`, in draw order.
///
// PERFORMANCE RATIONALE:
// Why: The list is rebuilt on every presented tick (25 Hz at normal speed) for up to a few
// hundred bubbles, and the charter forbids steady-state allocation in presentation.
// What: The list is rebuilt rather than diffed: at most about 1,000 marks of 48 bytes, one pass
// over the bubbles per kind (two passes, O(N)), one O(1) catalog lookup per bubble and no pair
// loop. A diff would cost more bookkeeping than it saves, and the upload is one buffer write per
// rebuild either way. The output keeps its capacity across rebuilds.
// Trade-off: An unchanged bubble is rewritten on every rebuild.
pub fn build_marks(
    sources: &MarkSources<'_>,
    out: &mut Vec<MinimapMark>,
    subjects: &mut Vec<MarkSubject>,
) {
    out.clear();
    subjects.clear();
    push_map_objects(sources, out, subjects);
    push_remembered(sources, out, subjects);
    push_bubbles(sources, out, subjects, true);
    push_bubbles(sources, out, subjects, false);
    push_snapshots(sources, out, subjects);
    push_rallies(sources, out, subjects);
    push_rings(sources, out, subjects);
}

/// Pushes the resources the viewer last saw present, then the watchtowers.
fn push_map_objects(
    sources: &MarkSources<'_>,
    out: &mut Vec<MinimapMark>,
    subjects: &mut Vec<MarkSubject>,
) {
    let style = sources.style;
    let remembered = sources
        .fog
        .map(BattleFog::node_present)
        .filter(|present| !present.is_empty());
    for (index, object) in (0_u32..).zip(sources.objects) {
        let position = [object.position.x, object.position.z];
        if let Some(node) = sources.nodes.get(index as usize) {
            let present = remembered.map_or(object.visible, |present| {
                present.get(index as usize).copied().unwrap_or(false)
            });
            if present {
                let half = node.placed.map_or(object.size * 0.5, |placed| {
                    f32::from(placed.footprint.width.max(placed.footprint.height)) * 0.5
                });
                out.push(square(sources, position, half, style.resource.linear));
                subjects.push(MarkSubject::Node(index));
            }
        } else if object.visible {
            out.push(square(
                sources,
                position,
                style.watchtower_half,
                style.watchtower.linear,
            ));
            subjects.push(MarkSubject::Other);
        }
    }
}

/// Pushes the listed structures (`structures` true) or units.
fn push_bubbles(
    sources: &MarkSources<'_>,
    out: &mut Vec<MinimapMark>,
    subjects: &mut Vec<MarkSubject>,
    structures: bool,
) {
    for bubble in sources.bubbles {
        let footprint = bubble.shape.footprint();
        if footprint.is_some() != structures || !listed(sources, bubble) {
            continue;
        }
        let Some(mut color) = color_of(sources, bubble) else {
            continue;
        };
        let flashing = Some(bubble.owner) == sources.viewer
            && sources.alerts.is_some_and(|alerts| {
                alerts.flashes(bubble.entity_id, sources.tick, sources.style.flash_ticks)
            });
        if flashing {
            color = sources.style.attacked.linear;
        }
        out.push(bubble_square(sources, bubble, color));
        subjects.push(MarkSubject::Bubble(bubble.entity_id));
    }
}

/// The square of `bubble` in `color`: half the larger side of a structure's footprint, or a
/// unit's radius.
fn bubble_square(
    sources: &MarkSources<'_>,
    bubble: &BubbleRenderInstance,
    color: [f32; 4],
) -> MinimapMark {
    let half = bubble
        .shape
        .footprint()
        .map_or(bubble.radius, |(width, height)| {
            f32::from(width.max(height)) * 0.5
        });
    let [x, y, _] = bubble.world_pos;
    square(sources, [x, y], half, color)
}

/// Pushes one square per remembered structure the view lists, as a live structure of its owner
/// is drawn, at `remembered_alpha`. An image, never picked.
fn push_remembered(
    sources: &MarkSources<'_>,
    out: &mut Vec<MinimapMark>,
    subjects: &mut Vec<MarkSubject>,
) {
    let Some(fog) = sources.fog else {
        return;
    };
    for image in fog.remembered_structures() {
        let Some(mut color) = color_of(sources, &image.bubble) else {
            continue;
        };
        color[3] = sources.style.remembered_alpha;
        out.push(bubble_square(sources, &image.bubble, color));
        subjects.push(MarkSubject::Image);
    }
}

/// Pushes one square per attack snapshot the view lists, as its unit would be drawn, at
/// `snapshot_alpha`. A snapshot is an image, never picked.
fn push_snapshots(
    sources: &MarkSources<'_>,
    out: &mut Vec<MinimapMark>,
    subjects: &mut Vec<MarkSubject>,
) {
    for snapshot in sources.snapshots {
        let Some(mut color) = color_of(sources, &snapshot.bubble) else {
            continue;
        };
        color[3] = sources.style.snapshot_alpha;
        out.push(bubble_square(sources, &snapshot.bubble, color));
        subjects.push(MarkSubject::Image);
    }
}

// PERFORMANCE RATIONALE:
// Why: Rally lines are rebuilt with the marks on every presented tick.
// What: The selection and the rallies are both sorted by entity, so one merge walk pairs them,
// with a binary search into the sorted bubbles for each structure's position.
// Trade-off: O(selected + rallies + selected structures × log bubbles) per rebuild instead of a
// map, which would allocate.
/// Pushes the rally lines of the selected structures: from the structure to each slot's first
/// waypoint, then waypoint to waypoint.
fn push_rallies(
    sources: &MarkSources<'_>,
    out: &mut Vec<MinimapMark>,
    subjects: &mut Vec<MarkSubject>,
) {
    let mut rallies = sources.rallies;
    for id in sources.selected {
        let start = rallies.partition_point(|rally| rally.entity < *id);
        rallies = &rallies[start..];
        let end = rallies.partition_point(|rally| rally.entity == *id);
        let (own, rest) = rallies.split_at(end);
        rallies = rest;
        let Some(origin) = bubble_at(sources.bubbles, *id)
            .filter(|bubble| Some(bubble.owner) == sources.viewer)
            .map(|bubble| [bubble.world_pos[0], bubble.world_pos[1]])
        else {
            continue;
        };
        let mut from = origin;
        let mut slot = None;
        for rally in own {
            if slot != Some(rally.slot) {
                from = origin;
                slot = Some(rally.slot);
            }
            out.push(segment(sources, from, rally.point));
            subjects.push(MarkSubject::Other);
            from = rally.point;
        }
    }
}

/// The presented bubble of `id`, by binary search.
fn bubble_at(bubbles: &[BubbleRenderInstance], id: EntityId) -> Option<&BubbleRenderInstance> {
    bubbles
        .binary_search_by_key(&id, |bubble| bubble.entity_id)
        .ok()
        .and_then(|index| bubbles.get(index))
}

/// Pushes one ring per live ping.
fn push_rings(
    sources: &MarkSources<'_>,
    out: &mut Vec<MinimapMark>,
    subjects: &mut Vec<MarkSubject>,
) {
    let Some(alerts) = sources.alerts else {
        return;
    };
    let style = &sources.style.alerts;
    for ping in alerts.pings() {
        let centre = sources.layout.to_minimap(ping.at);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a ping lasts a few hundred ticks"
        )]
        let ticks = ping.ticks as f32;
        out.push(MinimapMark {
            a: centre.to_array(),
            b: [style.ring_radius_px.0, style.ring_radius_px.1],
            color: ping.color,
            kind: MARK_RING,
            start_tick: ping.tick,
            stroke: style.ring_px,
            extra: ticks,
        });
        subjects.push(MarkSubject::Other);
    }
}

/// A rally line from map point `from` to `to`, in `rally_color`.
fn segment(sources: &MarkSources<'_>, from: [f32; 2], to: [f32; 2]) -> MinimapMark {
    let layout = sources.layout;
    MinimapMark {
        a: layout
            .to_minimap(bevy::math::Vec2::from_array(from))
            .to_array(),
        b: layout
            .to_minimap(bevy::math::Vec2::from_array(to))
            .to_array(),
        color: sources.style.rally.linear,
        kind: MARK_SEGMENT,
        start_tick: 0,
        stroke: sources.style.rally_px,
        extra: 0.0,
    }
}

/// Whether `bubble` is drawn on the minimap: shown by the world's own rule, and a unit a player
/// can pick.
fn listed(sources: &MarkSources<'_>, bubble: &BubbleRenderInstance) -> bool {
    presence_of(sources.fog, bubble) == Presence::Shown
        && sources.catalog.is_some_and(|catalog| {
            catalog
                .get(bubble.unit_type)
                .is_ok_and(|archetype| archetype.selectable && archetype.order_targetable)
        })
}

/// The linear colour of `bubble` in the current mode, or `None` when its owner has no style.
fn color_of(sources: &MarkSources<'_>, bubble: &BubbleRenderInstance) -> Option<[f32; 4]> {
    match sources.mode {
        ColorMode::Player => sources
            .palette?
            .resolve(bubble.owner)
            .ok()
            .map(|style| style.linear.to_array()),
        ColorMode::Team => {
            let team = &sources.style.team;
            let color = if bubble.owner == PlayerId::NEUTRAL {
                team.neutral
            } else if Some(bubble.owner) != sources.viewer {
                team.hostile
            } else if sources.selected.binary_search(&bubble.entity_id).is_ok() {
                team.selected
            } else {
                team.own
            };
            Some(color.linear)
        }
    }
}

/// A bordered square centred on map point `world` (world units), `half` world units each way,
/// at least half the minimum dot size.
fn square(sources: &MarkSources<'_>, world: [f32; 2], half: f32, color: [f32; 4]) -> MinimapMark {
    let style = sources.style;
    let pixel = sources
        .layout
        .to_minimap(bevy::math::Vec2::from_array(world));
    let half_px = sources.layout.pixels(half).max(style.min_dot_px * 0.5);
    MinimapMark {
        a: [pixel.x.floor() + 0.5, pixel.y.floor() + 0.5],
        b: [half_px, half_px],
        color,
        kind: MARK_SQUARE,
        start_tick: 0,
        stroke: style.border_px,
        extra: 0.0,
    }
}
