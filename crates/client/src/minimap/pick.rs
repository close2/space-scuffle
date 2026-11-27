//! What a press on the minimap aims at (plan `2026-12-01-minimap` D-7).
//!
//! # Responsibility
//! [`aim_at`] turns a map point pressed on the minimap into the same [`PointerAim`] a world press builds along its
//! ray, so the world and the minimap share one order path:
//! - **unit**: the topmost listed unit or structure square containing the pixel, in reverse draw
//!   order (units over structures, later over earlier), that the viewer may pick by the world's
//!   own rule ([`BattleView::can_pick`](crate::battle_input::BattleView)); never a square drawn
//!   for an image (an attack snapshot), because only live bubbles are listed as units;
//! - **node**: the topmost resource square containing it, found even under a unit, so that
//!   [`resolve_target`](crate::battle_input::context_order::resolve_target)'s mineral-line rule
//!   can prefer it over a friendly worker standing on it, as in the world;
//! - **ground**: always the pressed map point, at the terrain's height there.
//!
//! The press arrives in world units, already clamped to the camera's bound
//! ([`MinimapLayout::to_world_clamped`]); squares are tested at its minimap pixel.
//!
//! # Invariants
//! - Reads the retained marks and their subjects: what is picked is exactly what is drawn.
//! - O(marks) per press, no allocation; it never runs on a frame without a press.

use bevy::prelude::*;
use sim_map::grid::GameMap;
use sim_types::{BubbleRenderInstance, EntityId, PlayerId, ResourceNodeId, SimScalar, SimVec2};

use super::layout::MinimapLayout;
use super::marks::MarkSubject;
use super::MinimapMark;
use crate::battle_input::{PickedUnit, PointerAim};
use crate::runtime_battle::MapObjectSnapshot;

/// How far outside a square's edge, in pixels, a press still lands on it: half a pixel, so a press
/// on the border pixel counts.
const EDGE_SLACK_PX: f32 = 0.5;

/// What [`aim_at`] reads.
#[derive(Clone, Copy)]
pub struct AimSources<'a> {
    /// The mapping of the map onto the image.
    pub layout: MinimapLayout,
    /// The marks, in draw order.
    pub marks: &'a [MinimapMark],
    /// What each mark shows, index for index.
    pub subjects: &'a [MarkSubject],
    /// The presented bubbles, sorted by entity.
    pub bubbles: &'a [BubbleRenderInstance],
    /// The map's billboards: resources in map order, then watchtowers.
    pub objects: &'a [MapObjectSnapshot],
    /// The map, for the ground's height.
    pub map: &'a GameMap,
}

/// The aim of a press at map point `point` (simulation `(x, y)`, world units); `can_pick(id,
/// owner)` is the world's pick rule.
#[must_use]
pub fn aim_at(
    sources: &AimSources<'_>,
    point: Vec2,
    can_pick: impl Fn(EntityId, PlayerId) -> bool,
) -> PointerAim {
    let pixel = sources.layout.to_minimap(point);
    let height = sources.map.terrain_altitude_at(SimVec2::new(
        SimScalar::from_num(point.x),
        SimScalar::from_num(point.y),
    ));
    let ground = Vec3::new(point.x, height, point.y);
    let mut unit = None;
    let mut node = None;
    for (mark, subject) in sources.marks.iter().zip(sources.subjects).rev() {
        if !contains(mark, pixel) {
            continue;
        }
        match *subject {
            MarkSubject::Bubble(id) if unit.is_none() => {
                unit = picked(sources.bubbles, id).filter(|found| can_pick(found.id, found.owner));
            }
            MarkSubject::Node(index) if node.is_none() => {
                node = sources
                    .objects
                    .get(index as usize)
                    .map(|object| (ResourceNodeId(index), object.position));
            }
            _ => {}
        }
    }
    PointerAim {
        unit,
        node,
        ground: Some(ground),
    }
}

/// Whether square `mark` covers `pixel`.
fn contains(mark: &MinimapMark, pixel: Vec2) -> bool {
    mark.kind == super::marks::MARK_SQUARE
        && (pixel.x - mark.a[0]).abs() <= mark.b[0] + EDGE_SLACK_PX
        && (pixel.y - mark.a[1]).abs() <= mark.b[1] + EDGE_SLACK_PX
}

/// The live bubble of `id` as a picked unit, with the ground below it.
fn picked(bubbles: &[BubbleRenderInstance], id: EntityId) -> Option<PickedUnit> {
    let index = bubbles
        .binary_search_by_key(&id, |bubble| bubble.entity_id)
        .ok()?;
    let bubble = bubbles.get(index)?;
    Some(PickedUnit {
        id,
        owner: bubble.owner,
        ground: Vec3::new(bubble.world_pos[0], bubble.ground_z, bubble.world_pos[1]),
    })
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed test is a panic"
)]
mod tests {
    use super::*;
    use sim_map::grid::{CliffLevel, MapCell};

    fn square(centre: [f32; 2], half: f32) -> MinimapMark {
        MinimapMark {
            a: centre,
            b: [half, half],
            color: [1.0; 4],
            kind: super::super::marks::MARK_SQUARE,
            start_tick: 0,
            stroke: 1.0,
            pad: 0.0,
        }
    }

    fn bubble(id: u32, owner: u8, at: [f32; 2]) -> BubbleRenderInstance {
        use sim_types::enums::{AnimState, ShapeKind, SilhouetteClass, WeaponVisualKind};
        BubbleRenderInstance {
            entity_id: EntityId(id),
            shape: ShapeKind::UnitSphere,
            silhouette: SilhouetteClass::LightProngs,
            unit_type: sim_types::UnitTypeId(1),
            owner: PlayerId(owner),
            world_pos: [at[0], at[1], 0.5],
            ground_z: 0.0,
            radius: 0.5,
            velocity: [0.0, 0.0],
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
            concealment: sim_types::Concealment::None,
            speed_boosted: false,
            stance: sim_types::StanceView::default(),
            status: sim_types::StatusFlags::default(),
            construction_hp_ratio: None,
            barrier_ratio: 0.0,
            housed: false,
        }
    }

    #[test]
    fn the_topmost_pickable_square_wins_and_the_ground_is_always_there() {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0))).expect("a map");
        let layout = MinimapLayout::new(32, 32, 64.0).expect("a layout");
        let bubbles = [bubble(1, 0, [8.0, 8.0]), bubble(2, 1, [8.0, 8.0])];
        let marks = [square([16.5, 16.5], 6.0), square([16.5, 16.5], 2.0)];
        let subjects = [
            MarkSubject::Bubble(EntityId(1)),
            MarkSubject::Bubble(EntityId(2)),
        ];
        let sources = AimSources {
            layout,
            marks: &marks,
            subjects: &subjects,
            bubbles: &bubbles,
            objects: &[],
            map: &map,
        };
        let aim = aim_at(&sources, Vec2::new(8.25, 8.25), |_, _| true);
        assert_eq!(
            aim.unit.map(|unit| unit.id),
            Some(EntityId(2)),
            "drawn last"
        );
        let aim = aim_at(&sources, Vec2::new(8.25, 8.25), |id, _| id != EntityId(2));
        assert_eq!(
            aim.unit.map(|unit| unit.id),
            Some(EntityId(1)),
            "falls through"
        );
        let aim = aim_at(&sources, Vec2::new(20.0, 20.0), |_, _| true);
        assert!(aim.unit.is_none());
        let ground = aim.ground.expect("the ground");
        assert!((ground.x - 20.0).abs() < 1e-4 && (ground.z - 20.0).abs() < 1e-4);
    }

    #[test]
    fn an_image_square_is_never_a_unit() {
        let map = GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0))).expect("a map");
        let layout = MinimapLayout::new(8, 8, 8.0).expect("a layout");
        let marks = [square([4.5, 4.5], 2.0)];
        let subjects = [MarkSubject::Image];
        let sources = AimSources {
            layout,
            marks: &marks,
            subjects: &subjects,
            bubbles: &[],
            objects: &[],
            map: &map,
        };
        assert!(aim_at(&sources, Vec2::new(4.5, 4.5), |_, _| true)
            .unit
            .is_none());
    }
}
