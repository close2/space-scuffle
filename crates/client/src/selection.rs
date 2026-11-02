//! Visual Lab actor picking and the analytical ray-sphere test shared by every picker.
//!
//! # Architectural Responsibilities
//! This module computes camera rays from cursor screen coordinates and tests them against the
//! showcase actors of the Visual Lab, updating [`SelectionState`]. Live battles select through
//! [`crate::battle_input`] and [`crate::battle_selection`] instead.
//!
//! The swarm is not pickable: its 200 bubbles share two materials, so none of them can be
//! highlighted on its own.
//!
//! # Invariants Maintained
//! - Ray-sphere tests are purely analytical with zero iterative or mesh-traversal overhead.
//! - Pointer input over `egui` UI windows takes precedence and skips viewport raycasting.
//! - Hidden actors are never picked. The closest visible actor along the camera ray ($t > 0$) is
//!   selected; a miss deselects, and the Lab then falls back to its current lineup slot.
//! - This module writes no material. Lab preview synchronization derives the highlight from
//!   [`SelectionState`] on the next frame.

use crate::bubble_shader::BubbleInstance;
use bevy::prelude::*;
use sim_types::ids::EntityId;

// PERFORMANCE RATIONALE:
// Why: StarCraft II click selection in a 200-unit battle must execute in sub-millisecond time.
// What: Analytical quadratic solution of |(O + t*D) - C|^2 = r^2 without bounding volume hierarchies or mesh collisions.
// Trade-off: Perfectly spherical collision volumes in exchange for O(N) fast vectorized ray tests.
/// Computes analytical intersection of a 3D ray with a sphere.
///
/// Returns the closest positive distance `t` along the ray such that
/// `origin + t * dir` lies on the sphere surface, or `None` if the ray misses or is strictly behind the origin.
///
/// # Mathematical Formulation
/// For ray `R(t) = O + t * D` and sphere `|P - C|^2 = r^2`:
/// Let `V = O - C`.
/// `b = 2 * (V · D)`
/// `c = |V|^2 - r^2`
/// `discriminant = b^2 - 4c`.
#[must_use]
pub fn ray_sphere_intersect(
    origin: Vec3,
    dir: Vec3,
    sphere_center: Vec3,
    sphere_radius: f32,
) -> Option<f32> {
    let v = origin - sphere_center;
    let b = 2.0 * v.dot(dir);
    let c = v.length_squared() - sphere_radius * sphere_radius;
    let discriminant = b * b - 4.0 * c;

    if discriminant < 0.0 {
        return None;
    }

    let sqrt_d = discriminant.sqrt();
    let t1 = (-b - sqrt_d) * 0.5;
    let t2 = f32::midpoint(-b, sqrt_d);

    if t1 > 0.0 {
        Some(t1)
    } else if t2 > 0.0 {
        Some(t2)
    } else {
        None
    }
}

/// Global resource tracking currently selected unit entity and metadata.
#[derive(Resource, Debug, Default, Clone)]
pub struct SelectionState {
    /// Authoritative simulation entity ID, if applicable.
    pub selected_entity_id: Option<EntityId>,
    /// Bevy ECS entity ID for the selected bubble.
    pub selected_entity: Option<Entity>,
    /// Selected index within the 4-hero lineup (0..4), if in Visual Lab mode.
    pub selected_hero_index: Option<usize>,
}

impl SelectionState {
    /// Selects an entity with optional simulation and hero index metadata.
    pub fn select(
        &mut self,
        entity: Entity,
        entity_id: Option<EntityId>,
        hero_index: Option<usize>,
    ) {
        self.selected_entity = Some(entity);
        self.selected_entity_id = entity_id;
        self.selected_hero_index = hero_index;
    }

    /// Clears the active selection.
    pub fn deselect(&mut self) {
        self.selected_entity = None;
        self.selected_entity_id = None;
        self.selected_hero_index = None;
    }

    /// Returns true if the specified Bevy entity is currently selected.
    #[must_use]
    pub fn is_selected(&self, entity: Entity) -> bool {
        self.selected_entity == Some(entity)
    }
}

/// Marker component for indexed hero lineup entities in Visual Lab mode.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeroLineupIndex(pub usize);

type LabActorQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GlobalTransform,
        &'static BubbleInstance,
        &'static InheritedVisibility,
        &'static HeroLineupIndex,
    ),
>;

/// System processing left-click viewport raycasting to select Visual Lab actors.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn selection_system(
    mouse_buttons: Res<'_, ButtonInput<MouseButton>>,
    windows: Query<'_, '_, &Window, With<bevy::window::PrimaryWindow>>,
    camera_query: Query<'_, '_, (&Camera, &GlobalTransform), With<crate::RtsCamera>>,
    ownership: Res<'_, crate::input_policy::InputOwnership>,
    lab: Res<'_, crate::LabState>,
    mut selection_state: ResMut<'_, SelectionState>,
    actors: LabActorQuery<'_, '_>,
) {
    if lab.current_tab != crate::LabTab::VisualLab
        || !ownership.pointer
        || !mouse_buttons.just_pressed(MouseButton::Left)
    {
        return;
    }

    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    let Some(viewport_cursor) = crate::input_policy::ViewportCoordinates::from_camera(camera)
        .and_then(|coordinates| coordinates.viewport_from_window(cursor_pos))
    else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, viewport_cursor) else {
        return;
    };

    let mut closest_hit: Option<(Entity, EntityId, usize, f32)> = None;

    for (entity, transform, instance, visibility, hero) in &actors {
        if !visibility.get() {
            continue;
        }
        let center = transform.translation();
        if let Some(t) = ray_sphere_intersect(ray.origin, *ray.direction, center, instance.0.radius)
        {
            if closest_hit
                .as_ref()
                .is_none_or(|(_, _, _, min_t)| t < *min_t)
            {
                closest_hit = Some((entity, instance.0.entity_id, hero.0, t));
            }
        }
    }

    if let Some((hit_entity, hit_id, hero_index, _)) = closest_hit {
        selection_state.select(hit_entity, Some(hit_id), Some(hero_index));
    } else {
        selection_state.deselect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_sphere_intersect_direct_hit() {
        let origin = Vec3::new(0.0, 10.0, 0.0);
        let dir = Vec3::new(0.0, -1.0, 0.0);
        let center = Vec3::new(0.0, 0.0, 0.0);
        let radius = 1.0;

        let hit = ray_sphere_intersect(origin, dir, center, radius);
        assert!(hit.is_some_and(|t| (t - 9.0).abs() < 1e-4));
    }

    #[test]
    fn test_ray_sphere_intersect_tangent() {
        let origin = Vec3::new(1.0, 10.0, 0.0);
        let dir = Vec3::new(0.0, -1.0, 0.0);
        let center = Vec3::new(0.0, 0.0, 0.0);
        let radius = 1.0;

        let hit = ray_sphere_intersect(origin, dir, center, radius);
        assert!(hit.is_some_and(|t| (t - 10.0).abs() < 1e-4));
    }

    #[test]
    fn test_ray_sphere_intersect_miss() {
        let origin = Vec3::new(1.5, 10.0, 0.0);
        let dir = Vec3::new(0.0, -1.0, 0.0);
        let center = Vec3::new(0.0, 0.0, 0.0);
        let radius = 1.0;

        let hit = ray_sphere_intersect(origin, dir, center, radius);
        assert!(hit.is_none());
    }

    #[test]
    fn test_selection_state_lifecycle() {
        let mut state = SelectionState::default();
        assert!(state.selected_entity.is_none());
        assert!(state.selected_hero_index.is_none());

        let entity = Entity::from_raw(42);
        state.select(entity, Some(EntityId(101)), Some(2));
        assert!(state.is_selected(entity));
        assert_eq!(state.selected_entity_id, Some(EntityId(101)));
        assert_eq!(state.selected_hero_index, Some(2));

        state.deselect();
        assert!(!state.is_selected(entity));
        assert!(state.selected_entity.is_none());
    }
}
