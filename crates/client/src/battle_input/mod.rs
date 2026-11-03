//! Turns mouse and keyboard input into battle selection and orders.
//!
//! # Responsibility
//! Owns the player's hands on a live battle: left click and box drag change
//! [`BattleSelection`], right click and the order keys go through
//! [`BattleCommandQueue`](crate::battle_pipeline::commands::BattleCommandQueue), and control group
//! keys store and recall selections. What a right click means is decided by [`context_order()`].
//! Keys come from [`crate::input_bindings`].
//!
//! # How it fits
//! These systems run in [`BattleSet::CollectIntent`], after UI ownership is sampled and before the
//! tick driver, so an order is executed on the frame it was issued. Selection is applied to the
//! bubbles later, by [`sync_live_battle_system`](crate::sync_live_battle_system).
//!
//! # Invariants
//! - Pointer and keyboard input act only when [`InputOwnership`] grants them to the world.
//! - Only units the player can see are picked: Bevy-visible, not behind terrain, and for a unit of
//!   another player, one the simulation says that player can target.
//! - Orders go only to the local player's units; a foreign unit can be looked at, never ordered.
//! - Picking costs O(N) per click and box selection O(N) per release. Nothing here scans bubbles
//!   or resource nodes on a frame without a click or release, and such frames allocate nothing.

pub mod context_order;
pub mod gesture;
pub mod order_marker;

use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts};
use sim_core::SimWorld;
use sim_types::enums::ShapeKind;
use sim_types::{EntityId, PlayerId, ResourceNodeId, UnitOrder};

use crate::battle_pipeline::commands::SubmissionError;
use crate::battle_pipeline::session::BattleSession;
use crate::battle_pipeline::BattleSet;
use crate::battle_presentation::BattlePresentation;
use crate::battle_selection::BattleSelection;
use crate::bubble_shader::BubbleInstance;
use crate::camera::RtsCamera;
use crate::input_bindings::{
    any_pressed, CONTROL_GROUP_KEYS, CONTROL_MODIFIERS, HOLD_POSITION_KEY, SHIFT_MODIFIERS,
    STOP_KEY,
};
use crate::input_policy::{world_to_window, InputOwnership, ViewportCoordinates};
use crate::lab_ui::{LabState, LabTab};
use crate::runtime_battle::MapPresentation;
use crate::scene_controller::LiveBattleBubble;
use crate::selection::ray_sphere_intersect;
use crate::terrain::{pick_terrain, TerrainState};

use context_order::{context_order, PointerTarget};
use gesture::{GestureOutcome, SelectionGesture};
use order_marker::OrderMarkers;

/// Extra pick radius in world units, so small units stay clickable from a zoomed-out camera.
const PICK_RADIUS_PADDING: f32 = 0.15;

/// Pick radius of a resource billboard in world units, about its authored half diagonal.
const NODE_PICK_RADIUS: f32 = 1.0;

/// How far in front of a bubble, in world units, terrain must be to count as hiding it.
const TERRAIN_OCCLUSION_MARGIN: f32 = 0.05;

/// Fill of the box drawn during a drag.
const BOX_FILL: egui::Color32 = egui::Color32::from_rgba_premultiplied(12, 40, 18, 48);

/// Outline of the box drawn during a drag.
const BOX_STROKE: egui::Color32 = egui::Color32::from_rgb(90, 240, 130);

/// Registers selection, order and marker systems on the game client.
pub struct BattleInputPlugin;

impl Plugin for BattleInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BattleSelection>()
            .init_resource::<SelectionGesture>()
            .add_systems(
                Startup,
                order_marker::setup_order_markers.after(crate::setup_indicator_assets_system),
            )
            .add_systems(
                Update,
                (
                    (
                        collect_pointer_intent,
                        collect_key_intent,
                        draw_selection_box,
                    )
                        .chain()
                        .after(crate::input_policy::sample_input_ownership)
                        .after(crate::lab_ui_system)
                        .in_set(BattleSet::CollectIntent),
                    order_marker::update_order_markers.in_set(BattleSet::Present),
                ),
            );
    }
}

type BubbleQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static GlobalTransform,
        &'static BubbleInstance,
        &'static InheritedVisibility,
    ),
    With<LiveBattleBubble>,
>;

/// Read access to everything a pointer gesture is resolved against.
#[derive(SystemParam)]
pub struct BattleView<'w, 's> {
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<RtsCamera>>,
    ownership: Res<'w, InputOwnership>,
    lab: Res<'w, LabState>,
    presentation: Res<'w, BattlePresentation>,
    terrain: Option<Res<'w, TerrainState>>,
    /// Map billboards, absent until a battle first commits.
    map_objects: Option<Res<'w, MapPresentation>>,
    bubbles: BubbleQuery<'w, 's>,
}

/// A unit under the pointer.
#[derive(Debug, Clone, Copy)]
struct PickedUnit {
    id: EntityId,
    owner: PlayerId,
    /// Point on the ground below the unit, in Bevy world space.
    ground: Vec3,
}

impl BattleView<'_, '_> {
    /// Whether a battle is on screen and its controls may act.
    fn is_open(&self, session: &BattleSession) -> bool {
        self.lab.current_tab == LabTab::LiveBattle
            && session.is_ready()
            && self.lab.battle_failure.is_none()
    }

    /// Returns the camera's view ray through `cursor`, a window position.
    fn ray(&self, cursor: Vec2) -> Option<Ray3d> {
        let (camera, transform) = self.cameras.get_single().ok()?;
        let viewport = ViewportCoordinates::from_camera(camera)?.viewport_from_window(cursor)?;
        camera.viewport_to_world(transform, viewport).ok()
    }

    /// Returns the cursor clamped to the world viewport, in window coordinates.
    ///
    /// A drag that crosses a panel keeps following the pointer along the viewport's edge.
    fn cursor_in_viewport(&self) -> Option<Vec2> {
        let cursor = self.windows.get_single().ok()?.cursor_position()?;
        let (camera, _) = self.cameras.get_single().ok()?;
        let rect = ViewportCoordinates::from_camera(camera)?.logical_rect;
        // `viewport_from_window` excludes the far edges, so stay a hair inside them.
        Some(cursor.clamp(rect.min, rect.max - Vec2::splat(0.5)))
    }

    /// Whether `id` is something `viewer` may see and pick.
    fn can_pick(world: &SimWorld, viewer: PlayerId, id: EntityId, owner: PlayerId) -> bool {
        if owner == viewer {
            world
                .entities
                .get(&id)
                .is_some_and(sim_core::Entity::is_alive)
        } else {
            world.is_entity_available_for_combat(viewer, id)
        }
    }

    /// Distance along `ray` to the terrain surface, if the ray reaches it.
    fn terrain_distance(&self, ray: Ray3d) -> Option<f32> {
        let hit = pick_terrain(&self.terrain.as_ref()?.map, ray.origin, *ray.direction)?;
        Some(hit.point.distance(ray.origin))
    }

    // PERFORMANCE RATIONALE:
    // Why: A click must find the nearest unit in a battle of up to 400.
    // What: One analytic ray-sphere test per bubble, with the world lookup only after a hit.
    // Trade-off: O(N) per click rather than a spatial index, because this runs on a click, not
    // on a frame, and N stays in the hundreds.
    /// Returns the nearest unit under `ray` that `viewer` may see.
    fn pick_unit(&self, world: &SimWorld, viewer: PlayerId, ray: Ray3d) -> Option<PickedUnit> {
        let mut nearest: Option<(PickedUnit, f32)> = None;
        for (transform, bubble, visibility) in &self.bubbles {
            if !visibility.get() {
                continue;
            }
            let center = transform.translation();
            let radius = bubble.0.radius + PICK_RADIUS_PADDING;
            let Some(distance) = ray_sphere_intersect(ray.origin, *ray.direction, center, radius)
            else {
                continue;
            };
            if nearest.is_some_and(|(_, closest)| distance >= closest)
                || !Self::can_pick(world, viewer, bubble.0.entity_id, bubble.0.owner)
            {
                continue;
            }
            let unit = PickedUnit {
                id: bubble.0.entity_id,
                owner: bubble.0.owner,
                ground: Vec3::new(center.x, bubble.0.ground_z, center.z),
            };
            nearest = Some((unit, distance));
        }
        let (unit, distance) = nearest?;
        let hidden = self
            .terrain_distance(ray)
            .is_some_and(|ground| ground + TERRAIN_OCCLUSION_MARGIN < distance);
        (!hidden).then_some(unit)
    }

    // PERFORMANCE RATIONALE:
    // Why: A click must find the node under the pointer among the map's resource billboards.
    // What: One analytic ray-sphere test per node.
    // Trade-off: O(nodes) per click rather than a spatial index, because this runs on a click, and
    // a map holds tens of nodes.
    /// Returns the nearest shown resource node under `ray`, with its billboard's centre.
    fn pick_node(&self, world: &SimWorld, ray: Ray3d) -> Option<(ResourceNodeId, Vec3)> {
        let objects = &self.map_objects.as_ref()?.objects;
        // Records hold resources first, in map order, so a record's index is its node's ID.
        let resources = objects.get(..world.map.resources().len())?;
        let (index, center, distance) = (0_u32..)
            .zip(resources)
            .filter(|(_, object)| object.visible)
            .filter_map(|(index, object)| {
                let distance = ray_sphere_intersect(
                    ray.origin,
                    *ray.direction,
                    object.position,
                    NODE_PICK_RADIUS,
                )?;
                Some((index, object.position, distance))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))?;
        let hidden = self
            .terrain_distance(ray)
            .is_some_and(|ground| ground + TERRAIN_OCCLUSION_MARGIN < distance);
        (!hidden).then_some((ResourceNodeId(index), center))
    }

    /// Returns the terrain point under `ray`, in Bevy world space.
    fn ground_point(&self, ray: Ray3d) -> Option<Vec3> {
        let terrain = self.terrain.as_ref()?;
        let hit = pick_terrain(&terrain.map, ray.origin, *ray.direction)?;
        Some(hit.point)
    }

    /// Resolves what a right click along `ray` landed on, or `None` if it found nothing at all.
    ///
    /// Units come first, so a dome over a gas node wins. Neutral units count as ground.
    fn pointer_target(
        &self,
        world: &SimWorld,
        owner: PlayerId,
        ray: Ray3d,
    ) -> Option<PointerTarget> {
        let ground = self.ground_point(ray);
        let unit = self
            .pick_unit(world, owner, ray)
            .filter(|unit| unit.owner != PlayerId::NEUTRAL);
        if let Some(unit) = unit {
            return Some(if unit.owner == owner {
                PointerTarget::Friendly(unit.id, ground.unwrap_or(unit.ground))
            } else {
                PointerTarget::Enemy(unit.id, unit.ground)
            });
        }
        if let Some((node, center)) = self.pick_node(world, ray) {
            return Some(PointerTarget::Node(node, ground.unwrap_or(center)));
        }
        ground.map(PointerTarget::Ground)
    }

    /// Returns the local player's units inside `area`, a rectangle in window coordinates.
    ///
    /// Mobile units win over structures, so sweeping an army past a base selects only the army.
    fn units_in_box(&self, world: &SimWorld, owner: PlayerId, area: Rect) -> Vec<EntityId> {
        let Ok((camera, camera_transform)) = self.cameras.get_single() else {
            return Vec::new();
        };
        let right = *camera_transform.right();
        let mut units = Vec::new();
        let mut structures = Vec::new();
        for (transform, bubble, visibility) in &self.bubbles {
            if !visibility.get() || bubble.0.owner != owner {
                continue;
            }
            let center = transform.translation();
            let Some(on_screen) = world_to_window(camera, camera_transform, center) else {
                continue;
            };
            let reach = world_to_window(camera, camera_transform, center + right * bubble.0.radius)
                .map_or(0.0, |edge| edge.distance(on_screen));
            if !area.inflate(reach).contains(on_screen)
                || !Self::can_pick(world, owner, bubble.0.entity_id, owner)
            {
                continue;
            }
            if matches!(bubble.0.shape, ShapeKind::BuildingDome { .. }) {
                structures.push(bubble.0.entity_id);
            } else {
                units.push(bubble.0.entity_id);
            }
        }
        if units.is_empty() {
            structures
        } else {
            units
        }
    }
}

/// Reads the mouse: left button selects, right button orders.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn collect_pointer_intent(
    mouse: Res<'_, ButtonInput<MouseButton>>,
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    view: BattleView<'_, '_>,
    mut gesture: ResMut<'_, SelectionGesture>,
    mut selection: ResMut<'_, BattleSelection>,
    mut session: ResMut<'_, BattleSession>,
    mut markers: OrderMarkers<'_, '_>,
) {
    let owner = view.presentation.controlled;
    selection.bind_owner(owner);
    let focused = view.windows.get_single().is_ok_and(|window| window.focused);
    let (Some(owner), true, true) = (owner, focused, view.is_open(&session)) else {
        gesture.cancel();
        return;
    };
    let shift = any_pressed(&keyboard, &SHIFT_MODIFIERS);
    let cursor = view.cursor_in_viewport();

    if mouse.just_pressed(MouseButton::Left) && view.ownership.pointer {
        if let Some(cursor) = cursor {
            gesture.begin(cursor);
        }
    }
    if let Some(cursor) = cursor {
        gesture.track(cursor);
    }
    // A press and release can arrive in one frame, so the release is checked after the press.
    if !mouse.pressed(MouseButton::Left) {
        if let (Some(outcome), Some(world)) = (gesture.finish(), session.world()) {
            match outcome {
                GestureOutcome::Click(position) => {
                    apply_click(&view, world, owner, position, shift, &mut selection);
                }
                GestureOutcome::Box(area) => {
                    if !shift {
                        selection.clear();
                    }
                    selection.extend(view.units_in_box(world, owner, area));
                }
            }
        }
    }

    if mouse.just_pressed(MouseButton::Right)
        && view.ownership.pointer
        && !selection.ids().is_empty()
    {
        if let Some(ray) = cursor.and_then(|cursor| view.ray(cursor)) {
            issue_pointer_order(
                &view,
                ray,
                owner,
                shift,
                &selection,
                &mut session,
                &mut markers,
            );
        }
    }
}

/// Applies a click at `position`. A click on nothing clears the selection unless Shift is held.
fn apply_click(
    view: &BattleView<'_, '_>,
    world: &SimWorld,
    owner: PlayerId,
    position: Vec2,
    additive: bool,
    selection: &mut BattleSelection,
) {
    let picked = view
        .ray(position)
        .and_then(|ray| view.pick_unit(world, owner, ray));
    match picked {
        Some(unit) if unit.owner == owner && additive => selection.toggle(unit.id),
        // Shift only adds to an army, and a foreign unit can never join one.
        _ if additive => {}
        Some(unit) if unit.owner == owner => selection.select_only(unit.id),
        Some(unit) => selection.inspect(unit.id),
        None => selection.clear(),
    }
}

/// Sends the selection the order that a right click along `ray` means.
fn issue_pointer_order(
    view: &BattleView<'_, '_>,
    ray: Ray3d,
    owner: PlayerId,
    queued: bool,
    selection: &BattleSelection,
    session: &mut BattleSession,
    markers: &mut OrderMarkers<'_, '_>,
) {
    let (world, queue) = session.world_and_commands();
    let Some(world) = world else {
        queue.reject(&SubmissionError::NotReady);
        return;
    };
    let Some(target) = view.pointer_target(world, owner, ray) else {
        queue.reject(&SubmissionError::NoDestination);
        return;
    };
    let (order, kind, position) = context_order(world, owner, selection.ids(), target);
    match queue.submit_selection(world, owner, selection.ids(), order, queued) {
        Ok(_) => markers.show(kind, position),
        Err(error) => queue.reject(&error),
    }
}

/// Reads the keyboard: control groups, Hold and Stop.
///
/// Keys are read as ordered events, not from [`ButtonInput`]. At a low frame rate a whole Ctrl+1
/// chord can arrive within one frame, and by then `ButtonInput` shows Ctrl as already released,
/// which would turn the assignment into a recall.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn collect_key_intent(
    mut events: EventReader<'_, '_, KeyboardInput>,
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    mut control_held: Local<'_, bool>,
    view: BattleView<'_, '_>,
    mut selection: ResMut<'_, BattleSelection>,
    mut session: ResMut<'_, BattleSession>,
) {
    let owned = view.ownership.keyboard && view.is_open(&session);
    for event in events.read() {
        let pressed = event.state.is_pressed();
        if CONTROL_MODIFIERS.contains(&event.key_code) {
            *control_held = pressed;
        } else if owned && pressed && !event.repeat {
            let group = CONTROL_GROUP_KEYS
                .iter()
                .position(|key| *key == event.key_code);
            if let Some(index) = group {
                if *control_held {
                    selection.assign_group(index);
                } else {
                    selection.recall_group(index);
                }
            } else if event.key_code == HOLD_POSITION_KEY {
                order_selection(&view, &selection, &mut session, UnitOrder::HoldPosition);
            } else if event.key_code == STOP_KEY {
                order_selection(&view, &selection, &mut session, UnitOrder::Stop);
            }
        }
    }
    // The held state is authoritative again once this frame's events are consumed, which also
    // recovers from a release that went to another window.
    *control_held = any_pressed(&keyboard, &CONTROL_MODIFIERS);
}

/// Sends an order that needs no destination to the selection, if there is one.
fn order_selection(
    view: &BattleView<'_, '_>,
    selection: &BattleSelection,
    session: &mut BattleSession,
    order: UnitOrder,
) {
    let Some(owner) = view.presentation.controlled else {
        return;
    };
    if selection.ids().is_empty() {
        return;
    }
    let (world, queue) = session.world_and_commands();
    let result = world
        .ok_or(SubmissionError::NotReady)
        .and_then(|world| queue.submit_selection(world, owner, selection.ids(), order, false));
    if let Err(error) = result {
        queue.reject(&error);
    }
}

/// Draws the box of a drag in progress over the world.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn draw_selection_box(
    gesture: Res<'_, SelectionGesture>,
    windows: Query<'_, '_, &Window, With<PrimaryWindow>>,
    mut contexts: EguiContexts<'_, '_>,
) {
    let (Some(area), Ok(window), Some(context)) = (
        gesture.drag_rect(),
        windows.get_single(),
        contexts.try_ctx_mut(),
    ) else {
        return;
    };
    let points_per_pixel = window.scale_factor() / context.pixels_per_point();
    let corner = |point: Vec2| egui::pos2(point.x, point.y) * points_per_pixel;
    let rect = egui::Rect::from_min_max(corner(area.min), corner(area.max));
    context
        .layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("battle_selection_box"),
        ))
        .rect(
            rect,
            0.0,
            BOX_FILL,
            egui::Stroke::new(1.5_f32, BOX_STROKE),
            egui::StrokeKind::Middle,
        );
}
