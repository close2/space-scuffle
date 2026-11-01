//! Capture-only foreground staging from actual propagated shell/ribbon bounds.
//! Missing geometry remains pending; no empty extent or hidden-scene substitution
//! can prepare a demonstration. Production scene objects remain untouched.

use super::{super::config::CaptureError, failure};
use bevy::{prelude::*, render::primitives::Aabb};

/// Fixed capture arrangement for the existing3-unit beams and0.8-radius shatter.
const DEMONSTRATION_GAP: f32 = 4.0;

/// Immutable event origin in simulation render X/Y/height and camera world-X/Z focus.
#[derive(Clone, Copy)]
pub(super) struct Stage {
    pub origin: [f32; 3],
    pub focus: Vec2,
}

/// Returns all eight actual transformed corners, rejecting malformed geometry.
pub(super) fn corners(bounds: &Aabb, global: &GlobalTransform) -> Result<[Vec3; 8], CaptureError> {
    let center: Vec3 = bounds.center.into();
    let half: Vec3 = bounds.half_extents.into();
    if !center.is_finite()
        || !half.is_finite()
        || half.cmplt(Vec3::ZERO).any()
        || !global.compute_matrix().is_finite()
    {
        return Err(failure("nonfinite or inverted ownership staging bounds"));
    }
    let points = std::array::from_fn(|index| {
        let sign = Vec3::new(
            if index & 1 == 0 { -1.0 } else { 1.0 },
            if index & 2 == 0 { -1.0 } else { 1.0 },
            if index & 4 == 0 { -1.0 } else { 1.0 },
        );
        global.transform_point(center + half * sign)
    });
    if points.iter().any(|point: &Vec3| !point.is_finite()) {
        return Err(failure("transformed ownership bounds overflowed"));
    }
    Ok(points)
}

/// Uses actual current Lab actors and visible wake geometry; absence is pending.
pub(super) fn prepare(world: &mut World) -> Result<Option<Stage>, CaptureError> {
    let mut query = world.query_filtered::<(
        Option<&crate::LabPreviewBubble>,
        Option<&crate::BubbleInstance>,
        Option<&Aabb>,
        Option<&GlobalTransform>,
        Option<&ViewVisibility>,
    ), Or<(With<crate::LabPreviewBubble>, With<crate::mist::MistRibbon>)>>(
    );
    let mut foremost: Option<f32> = None;
    let mut selected = None;
    let mut actors = 0;
    for (lab, bubble, bounds, global, visible) in query.iter(world) {
        if lab.is_none() {
            let Some(visible) = visible else {
                return Ok(None);
            };
            if !visible.get() {
                continue;
            }
        }
        let (Some(bounds), Some(global)) = (bounds, global) else {
            return Ok(None);
        };
        let points = corners(bounds, global)?;
        if bounds.half_extents.max_element() <= 0.0 {
            return Ok(None);
        }
        if lab.is_some() {
            let Some(bubble) = bubble else {
                return Err(failure("Lab staging actor lacks snapshot"));
            };
            if !Vec3::from_array(bubble.0.world_pos).is_finite()
                || !bubble.0.radius.is_finite()
                || bubble.0.radius <= 0.0
            {
                return Err(failure("nonfinite or nonpositive Lab staging snapshot"));
            }
            if global
                .translation()
                .xz()
                .distance(Vec2::new(bubble.0.world_pos[0], bubble.0.world_pos[1]))
                > 0.0001
            {
                return Ok(None);
            }
            actors += 1;
            if bubble.0.selected {
                selected = Some(&bubble.0);
            }
        }
        for point in points {
            foremost = Some(foremost.map_or(point.z, |value| value.max(point.z)));
        }
    }
    let (Some(actor), Some(front)) = (selected, foremost) else {
        return Ok(None);
    };
    if actors == 0 {
        return Ok(None);
    }
    let mut origin = actor.world_pos;
    origin[0] += actor.radius * 1.8;
    origin[1] = front + DEMONSTRATION_GAP;
    origin[2] += actor.radius * 1.5;
    let focus = Vec2::new(
        origin[0] + 1.0,
        f32::midpoint(origin[1], actor.world_pos[1]),
    );
    if !Vec3::from_array(origin).is_finite() || !focus.is_finite() {
        return Err(failure("foreground stage arithmetic overflowed"));
    }
    Ok(Some(Stage { origin, focus }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_or_overflowing_transformed_bounds_fail_before_extent_reduction() {
        let global = GlobalTransform::IDENTITY;
        for half in [
            Vec3::splat(-1.0),
            Vec3::splat(f32::NAN),
            Vec3::splat(f32::INFINITY),
        ] {
            let bounds = Aabb {
                center: Vec3::ZERO.into(),
                half_extents: half.into(),
            };
            assert!(corners(&bounds, &global).is_err());
        }
        let bounds = Aabb {
            center: Vec3::splat(f32::MAX).into(),
            half_extents: Vec3::splat(f32::MAX).into(),
        };
        assert!(corners(&bounds, &global).is_err());
        let bounds = Aabb {
            center: Vec3::ZERO.into(),
            half_extents: Vec3::splat(f32::MAX).into(),
        };
        assert!(corners(&bounds, &GlobalTransform::from_scale(Vec3::splat(2.0))).is_err());
    }
    #[test]
    fn missing_ribbon_visibility_or_bounds_remains_pending() -> Result<(), CaptureError> {
        let mut world = World::new();
        let entity = world.spawn(crate::mist::MistRibbon).id();
        assert!(prepare(&mut world)?.is_none());
        world.entity_mut(entity).insert(ViewVisibility::default());
        assert!(prepare(&mut world)?.is_none());
        Ok(())
    }
}
