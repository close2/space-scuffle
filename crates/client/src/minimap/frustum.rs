//! The camera's view on the minimap: its ground trapezoid (plan `2026-12-01-minimap` D-8).
//!
//! # Responsibility
//! [`ground_trapezoid`] casts the four corner rays of the RTS camera's **viewport** (the world
//! rectangle left by the HUD, not the window) onto the horizontal plane the camera looks at,
//! `y = look_at_y`, and returns the four hits on the map plane, in world units (simulation
//! `(x, y)`, Bevy `(x, z)`), in the order top-left, top-right, bottom-right, bottom-left of the
//! screen. The trapezoid's centre is therefore where a minimap click puts the camera. The marks
//! pass draws its four edges in `frustum_color` (`core` `GameUIData:687 FrustumColor`).
//!
//! # The cut
//! A corner ray that misses the plane in front of the camera, or meets it farther than
//! `max_distance` world units from the eye, is cut at `max_distance` along its ground projection,
//! so the shape stays finite whatever a future camera allows. The shipped camera (58° pitch, at
//! most 48 units away) never needs the cut (plan M-5).
//!
//! # Invariants
//! - Pure: no allocation, no state; four ray–plane hits per call.
//! - Every returned point is finite.

use bevy::prelude::*;

/// Corners of a viewport, in the trapezoid's order: top-left, top-right, bottom-right,
/// bottom-left, as fractions of the viewport's size.
const CORNERS: [Vec2; 4] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(1.0, 0.0),
    Vec2::new(1.0, 1.0),
    Vec2::new(0.0, 1.0),
];

/// The view's ground trapezoid of `camera` posed at `transform`, on the plane `y = plane_y`, in
/// world units. `None` when the camera has no computed viewport or a corner ray cannot be cast.
#[must_use]
pub fn ground_trapezoid(
    camera: &Camera,
    transform: &GlobalTransform,
    plane_y: f32,
    max_distance: f32,
) -> Option<[Vec2; 4]> {
    let size = camera.logical_viewport_size()?;
    // `viewport_to_world` rejects nothing inside the viewport; stay a hair inside its far edges.
    let inner = (size - Vec2::splat(0.5)).max(Vec2::ZERO);
    let mut rays = [Ray3d::new(Vec3::ZERO, Dir3::NEG_Y); 4];
    for (ray, corner) in rays.iter_mut().zip(CORNERS) {
        *ray = camera.viewport_to_world(transform, corner * inner).ok()?;
    }
    Some(trapezoid_from_rays(&rays, plane_y, max_distance))
}

/// The ground hits of `rays` on the plane `y = plane_y`, each cut at `max_distance` (see the
/// module's "The cut").
#[must_use]
pub fn trapezoid_from_rays(rays: &[Ray3d; 4], plane_y: f32, max_distance: f32) -> [Vec2; 4] {
    rays.map(|ray| ground_hit(ray, plane_y, max_distance))
}

/// Where `ray` meets the plane `y = plane_y`, as a map point, or its cut.
fn ground_hit(ray: Ray3d, plane_y: f32, max_distance: f32) -> Vec2 {
    let direction = *ray.direction;
    let origin = Vec2::new(ray.origin.x, ray.origin.z);
    let along = (plane_y - ray.origin.y) / direction.y;
    if along.is_finite() && along > 0.0 && along <= max_distance {
        let hit = ray.origin + direction * along;
        return Vec2::new(hit.x, hit.z);
    }
    let ground = Vec2::new(direction.x, direction.z).normalize_or_zero();
    origin + ground * max_distance
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed test is a panic"
)]
mod tests {
    use super::*;

    fn ray(origin: Vec3, toward: Vec3) -> Ray3d {
        Ray3d::new(origin, Dir3::new(toward - origin).expect("a direction"))
    }

    #[test]
    fn rays_meet_the_plane_in_order() {
        let eye = Vec3::new(10.0, 20.0, 30.0);
        let hits = [
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(20.0, 1.0, 0.0),
            Vec3::new(15.0, 1.0, 25.0),
            Vec3::new(5.0, 1.0, 25.0),
        ];
        let rays = hits.map(|hit| ray(eye, hit));
        let trapezoid = trapezoid_from_rays(&rays, 1.0, 200.0);
        for (got, want) in trapezoid.iter().zip(hits) {
            assert!(
                got.distance(Vec2::new(want.x, want.z)) < 1e-4,
                "{got} vs {want}"
            );
        }
    }

    #[test]
    fn a_ray_that_misses_the_plane_is_cut_along_its_ground_projection() {
        let eye = Vec3::new(0.0, 10.0, 0.0);
        let up = ray(eye, Vec3::new(3.0, 14.0, 4.0));
        let cut = ground_hit(up, 0.0, 50.0);
        assert!((cut - Vec2::new(30.0, 40.0)).length() < 1e-3, "{cut}");
        let far = ray(eye, Vec3::new(1000.0, 9.0, 0.0));
        assert!((ground_hit(far, 0.0, 50.0) - Vec2::new(50.0, 0.0)).length() < 1e-3);
    }

    #[test]
    fn a_ray_straight_up_stays_at_the_eye() {
        let eye = Vec3::new(4.0, 10.0, 5.0);
        let point = ground_hit(Ray3d::new(eye, Dir3::Y), 0.0, 50.0);
        assert_eq!(point, Vec2::new(4.0, 5.0));
    }
}
