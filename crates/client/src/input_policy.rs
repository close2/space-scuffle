//! Explicit UI ownership and the unobscured primary-camera viewport.
//!
//! The production UI finishes its current pass before ownership is sampled. Camera
//! and selection share this decision; missing focus/context denies interaction.
//! Logical egui rectangles convert once to clamped physical camera coordinates.

use bevy::prelude::*;
use bevy::render::camera::Viewport;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts};

/// Explicit bridge between camera-local logical points and window/image coordinates.
///
/// Bevy0.15 projection methods omit the viewport origin in both directions.
#[derive(Debug, Clone, Copy)]
pub struct ViewportCoordinates {
    /// Window-relative viewport bounds in logical window pixels, not egui points.
    pub logical_rect: Rect,
    /// Physical window pixels per logical window pixel.
    pub scale: f32,
}

impl ViewportCoordinates {
    /// Reads the actual camera's computed target and viewport geometry.
    #[must_use]
    pub fn from_camera(camera: &Camera) -> Option<Self> {
        Some(Self {
            logical_rect: camera.logical_viewport_rect()?,
            scale: camera.target_scaling_factor()?,
        })
    }

    /// Adds the origin omitted by camera-local projection output.
    #[must_use]
    pub fn window_from_viewport(self, point: Vec2) -> Vec2 {
        self.logical_rect.min + point
    }

    /// Removes the origin for ray picking, rejecting points outside the viewport.
    #[must_use]
    pub fn viewport_from_window(self, point: Vec2) -> Option<Vec2> {
        (point.is_finite()
            && point.cmpge(self.logical_rect.min).all()
            && point.cmplt(self.logical_rect.max).all())
        .then_some(point - self.logical_rect.min)
    }

    /// Converts a window-logical point into full screenshot physical pixels.
    #[must_use]
    pub fn physical_from_window(self, point: Vec2) -> Vec2 {
        point * self.scale
    }
}

/// Projects a world point into logical window coordinates, including viewport origin.
#[must_use]
pub fn world_to_window(camera: &Camera, transform: &GlobalTransform, point: Vec3) -> Option<Vec2> {
    Some(
        ViewportCoordinates::from_camera(camera)?
            .window_from_viewport(camera.world_to_viewport(transform, point).ok()?),
    )
}

/// Named visible controls recorded for real native hit testing.
#[derive(Clone, Copy)]
#[repr(usize)]
pub enum UiTarget {
    /// Visual Lab tab.
    Lab,
    /// Loaded battle tab.
    Battle,
    /// Swarm preview tab.
    Swarm,
    /// Existing editable HP value slider.
    Health,
    /// Pause/resume battle control.
    Pause,
    /// Reset the same loaded battle.
    Reset,
}

/// Current UI geometry in egui logical points, shared by picking and evidence.
#[derive(Resource, Default)]
pub struct UiGeometry {
    /// Actual inspector bounds, excluding the separately reserved HUD.
    pub inspector: Option<egui::Rect>,
    /// Available world rectangle after all UI panels.
    pub world: Option<egui::Rect>,
    /// Current egui points-to-physical-pixels scale.
    pub pixels_per_point: f32,
    /// Actual visible response bounds for semantic controls.
    pub targets: [Option<egui::Rect>; 6],
}

impl UiGeometry {
    /// Records only a widget's visible hit rectangle, in egui logical points.
    pub fn record(&mut self, target: UiTarget, response: &egui::Response, clip: egui::Rect) {
        let rect = response.rect.intersect(clip);
        self.targets[target as usize] = rect.is_positive().then_some(rect);
    }
}

/// Shared interaction permission. Default is explicitly noninteractive.
#[derive(Resource, Default, Debug)]
pub struct InputOwnership {
    /// Whether this application actually owns an interactive primary window.
    pub interactive: bool,
    /// Current primary window identity; wheel events must match it.
    pub window: Option<Entity>,
    /// Current focus and egui keyboard ownership permit world shortcuts.
    pub keyboard: bool,
    /// Current focus, egui ownership and viewport bounds permit pointer input.
    pub pointer: bool,
}

impl InputOwnership {
    /// Creates production policy; permissions stay denied until sampled from UI.
    #[must_use]
    pub fn interactive() -> Self {
        Self {
            interactive: true,
            ..default()
        }
    }
}

/// Samples current-pass UI ownership after widgets have consumed their input.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn sample_input_ownership(
    mut ownership: ResMut<'_, InputOwnership>,
    windows: Query<'_, '_, (Entity, &Window), With<PrimaryWindow>>,
    mut contexts: EguiContexts<'_, '_>,
    geometry: Res<'_, UiGeometry>,
) {
    ownership.keyboard = false;
    ownership.pointer = false;
    ownership.window = None;
    if !ownership.interactive {
        return;
    }
    let (Ok((entity, window)), Some(context)) = (windows.get_single(), contexts.try_ctx_mut())
    else {
        return;
    };
    ownership.window = Some(entity);
    if !window.focused {
        return;
    }
    ownership.keyboard = !context.wants_keyboard_input();
    let inside = geometry
        .world
        .zip(window.cursor_position())
        .is_some_and(|(rect, cursor)| {
            let point = cursor * window.scale_factor() / context.pixels_per_point();
            rect.contains(egui::pos2(point.x, point.y))
        });
    ownership.pointer = inside && !context.wants_pointer_input();
}

/// Converts egui points to a nonempty physical viewport inside the window.
#[must_use]
pub fn physical_viewport(rect: egui::Rect, scale: f32, window: UVec2) -> Option<Viewport> {
    if !rect.is_finite() || !scale.is_finite() || scale <= 0.0 || window.min_element() == 0 {
        return None;
    }
    let min = (Vec2::new(rect.min.x, rect.min.y) * scale)
        .ceil()
        .max(Vec2::ZERO)
        .as_uvec2()
        .min(window);
    let max = (Vec2::new(rect.max.x, rect.max.y) * scale)
        .floor()
        .max(Vec2::ZERO)
        .as_uvec2()
        .min(window);
    if max.x <= min.x || max.y <= min.y {
        return None;
    }
    Some(Viewport {
        physical_position: min,
        physical_size: max - min,
        ..default()
    })
}

/// Keeps rendering/picking in the same unobscured physical viewport after resize.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn apply_world_viewport(
    geometry: Res<'_, UiGeometry>,
    windows: Query<'_, '_, &Window, With<PrimaryWindow>>,
    mut cameras: Query<'_, '_, &mut Camera, With<crate::RtsCamera>>,
) {
    let Ok(mut camera) = cameras.get_single_mut() else {
        return;
    };
    let viewport = geometry.world.and_then(|rect| {
        let window = windows.get_single().ok()?;
        physical_viewport(
            rect,
            geometry.pixels_per_point,
            UVec2::new(window.physical_width(), window.physical_height()),
        )
    });
    // A fully occluded or absent window has no world presentation area. Retain
    // the last valid rectangle for restoration, but never render through UI.
    camera.is_active = viewport.is_some();
    if let Some(viewport) = viewport {
        if camera.viewport.as_ref().is_none_or(|current| {
            current.physical_position != viewport.physical_position
                || current.physical_size != viewport.physical_size
        }) {
            camera.viewport = Some(viewport);
        }
    }
}
