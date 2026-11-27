//! What the minimap draws over the terrain: resources, structures and units as squares (plan
//! `2026-12-01-minimap` D-5).
//!
//! # Responsibility
//! [`build_marks`] turns one presented snapshot, the viewer's fog and the map's billboards into
//! the ordered list of [`MinimapMark`]s the marks pass draws, in draw order:
//! 1. **resources** the viewer last saw present, in `ResourceUnitColor` (`core`
//!    `GameUIData:688`), then watchtowers;
//! 2. **structures**, then
//! 3. **units**, so that a unit stays visible on top of a base.
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
//! `hostile` (no teams exist), neutral `neutral`.
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
use sim_types::{BubbleRenderInstance, EntityId, PlayerId};

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
    push_bubbles(sources, out, subjects, true);
    push_bubbles(sources, out, subjects, false);
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
        let Some(color) = color_of(sources, bubble) else {
            continue;
        };
        let half = footprint.map_or(bubble.radius, |(width, height)| {
            f32::from(width.max(height)) * 0.5
        });
        let [x, y, _] = bubble.world_pos;
        out.push(square(sources, [x, y], half, color));
        subjects.push(MarkSubject::Bubble(bubble.entity_id));
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
        pad: 0.0,
    }
}
