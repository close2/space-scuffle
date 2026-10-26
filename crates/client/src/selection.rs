//! Viewport click-to-select raycasting and analytical ray-sphere intersection.
//!
//! # Architectural Responsibilities
//! This module provides full 3D viewport entity selection for mobile unit bubbles and building domes.
//! It computes camera rays from cursor screen coordinates and executes analytical ray-sphere intersection
//! tests against all active entities to determine the closest hit, updating [`SelectionState`].
//!
//! # Invariants Maintained
//! - Ray-sphere tests are purely analytical with zero iterative or mesh-traversal overhead.
//! - Pointer input over `egui` UI windows takes precedence and skips viewport raycasting.
//! - The closest entity along the camera ray ($t > 0$) is selected; ties or misses deselect cleanly.

use crate::bubble_shader::{BubbleDomeMaterial, BubbleInstance, FLAG_SELECTED};
use bevy::prelude::*;
use bevy_egui::EguiContexts;
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
    let t2 = (-b + sqrt_d) * 0.5;

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

/// System processing left-click viewport raycasting to select unit bubbles and domes.
#[allow(clippy::type_complexity)]
pub fn selection_system(
    mouse_buttons: Res<'_, ButtonInput<MouseButton>>,
    windows: Query<'_, '_, &Window>,
    camera_query: Query<'_, '_, (&Camera, &GlobalTransform)>,
    mut contexts: EguiContexts<'_, '_>,
    mut selection_state: ResMut<'_, SelectionState>,
    mut bubble_query: Query<
        '_,
        '_,
        (
            Entity,
            &GlobalTransform,
            &mut BubbleInstance,
            &MeshMaterial3d<BubbleDomeMaterial>,
            Option<&HeroLineupIndex>,
        ),
    >,
    mut bubble_materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
) {
    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }

    // Skip picking if mouse pointer is over egui UI window or panel
    if contexts.ctx_mut().wants_pointer_input() {
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

    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    let ray_origin = ray.origin;
    let ray_dir = *ray.direction;

    let mut closest_hit: Option<(Entity, EntityId, Option<usize>, f32)> = None;

    for (entity, transform, instance, _, hero_idx) in bubble_query.iter() {
        let center = transform.translation();
        let radius = instance.0.radius;

        if let Some(t) = ray_sphere_intersect(ray_origin, ray_dir, center, radius) {
            if closest_hit
                .as_ref()
                .map_or(true, |(_, _, _, min_t)| t < *min_t)
            {
                closest_hit = Some((entity, instance.0.entity_id, hero_idx.map(|h| h.0), t));
            }
        }
    }

    if let Some((hit_entity, hit_id, hero_index, _)) = closest_hit {
        selection_state.select(hit_entity, Some(hit_id), hero_index);

        for (entity, _, mut instance, mat_handle, _) in &mut bubble_query {
            let is_selected = entity == hit_entity;
            instance.0.selected = is_selected;

            if let Some(mat) = bubble_materials.get_mut(&mat_handle.0) {
                mat.uniform.selected = u32::from(is_selected);
                if is_selected {
                    mat.uniform.feature_flags |= FLAG_SELECTED;
                } else {
                    mat.uniform.feature_flags &= !FLAG_SELECTED;
                }
            }
        }
    }
}

/// Canonical alias for the viewport selection handling system.
pub use selection_system as handle_viewport_selection_system;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_sphere_intersect_direct_hit() {
        let origin = Vec3::new(0.0, 10.0, 0.0);
        let dir = Vec3::new(0.0, -1.0, 0.0);
        let center = Vec3::new(0.0, 0.0, 0.0);
        let radius = 1.0;

        let hit = ray_sphere_intersect(origin, dir, center, radius);
        assert!(hit.is_some());
        let t = hit.expect("invariant: direct ray aligned with sphere center must intersect");
        assert!((t - 9.0).abs() < 1e-4);
    }

    #[test]
    fn test_ray_sphere_intersect_tangent() {
        let origin = Vec3::new(1.0, 10.0, 0.0);
        let dir = Vec3::new(0.0, -1.0, 0.0);
        let center = Vec3::new(0.0, 0.0, 0.0);
        let radius = 1.0;

        let hit = ray_sphere_intersect(origin, dir, center, radius);
        assert!(hit.is_some());
        let t = hit.expect("invariant: tangent ray grazing sphere surface must intersect");
        assert!((t - 10.0).abs() < 1e-4);
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
