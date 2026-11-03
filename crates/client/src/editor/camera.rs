//! Map editor camera controller subsystem.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.2 and `AGENTS.md`,
//! this module provides the dedicated RTS camera navigation system for the map editor.
//!
//! # Invariants Maintained
//! 1. **Component Reuse:** Operates directly on the existing [`RtsCamera`] component without modifying its type definition.
//! 2. **Input Hygiene:** Keyboard pan is strictly inhibited when Control is held (preventing conflict with shortcuts)
//!    or when egui requests keyboard input. Edge panning is disabled to permit UI interaction.
//! 3. **Pointer Decoupling:** Mouse wheel zooming is suspended while egui requests pointer input.
//! 4. **Bound Clamping:** Camera target coordinates are strictly clamped to the map boundary `[0, W] × [0, H]`.
//! 5. **Smooth Altitude Tracking:** Elevation `look_at_y` eases exponentially toward the continuous terrain surface altitude.

use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy_egui::EguiContexts;
use sim_types::{SimScalar, SimVec2};

use crate::camera::RtsCamera;
use crate::terrain::TerrainState;

/// Exponential easing speed in units per second for camera elevation tracking.
pub const CAMERA_HEIGHT_EASE_PER_SEC: f32 = 6.0;

/// Event requesting the editor camera to focus on a target simulation cell center.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorCameraFocus {
    /// Cell coordinate `(x, y)` to center in the viewport.
    pub cell: (u32, u32),
}

/// Sets the camera focus point squarely on the center of cell `(x, y)`.
#[expect(
    clippy::cast_precision_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
pub fn focus_cell(cam: &mut RtsCamera, x: u32, y: u32) {
    cam.target = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
}

/// Camera update system for the map editor.
///
/// Drives keyboard panning (WASD / Arrows), middle-mouse drag panning, scroll wheel zooming,
/// and map boundary clamping with smooth terrain height following.
#[expect(
    clippy::cast_precision_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn editor_camera_system(
    time: Res<'_, Time>,
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    mouse_buttons: Res<'_, ButtonInput<MouseButton>>,
    mut mouse_motion_events: EventReader<'_, '_, MouseMotion>,
    mut scroll_events: EventReader<'_, '_, MouseWheel>,
    mut contexts: EguiContexts<'_, '_>,
    terrain_state: Option<Res<'_, TerrainState>>,
    mut focus_events: EventReader<'_, '_, EditorCameraFocus>,
    mut query: Query<'_, '_, (&mut RtsCamera, &mut Transform)>,
) {
    let Ok((mut cam, mut transform)) = query.get_single_mut() else {
        return;
    };
    let dt = time.delta_secs();

    // Check egui input capture state gracefully
    let (wants_keyboard, wants_pointer) = match contexts.try_ctx_mut() {
        Some(ctx) => (ctx.wants_keyboard_input(), ctx.wants_pointer_input()),
        None => (false, false),
    };

    // 1. Process Focus Cell Events
    for ev in focus_events.read() {
        focus_cell(&mut cam, ev.cell.0, ev.cell.1);
    }

    let ctrl = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);

    // 2. Keyboard Panning (WASD / Arrows), strictly inhibited when Ctrl is held or egui owns keyboard
    if !ctrl && !wants_keyboard {
        let mut pan_dir = Vec2::ZERO;
        if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
            pan_dir.y += 1.0;
        }
        if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
            pan_dir.y -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
            pan_dir.x -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
            pan_dir.x += 1.0;
        }

        cam.pan(pan_dir, dt);
    }

    // 3. Middle-Mouse Drag Pan (Grab Scroll)
    if mouse_buttons.pressed(MouseButton::Middle) {
        for ev in mouse_motion_events.read() {
            let factor = cam.distance * 0.0018;
            cam.target.x -= ev.delta.x * factor;
            cam.target.y += ev.delta.y * factor;
        }
    } else {
        mouse_motion_events.clear();
    }

    // 4. Scroll Wheel Zoom, inhibited when pointer is interacting with egui
    if wants_pointer {
        scroll_events.clear();
    } else {
        for ev in scroll_events.read() {
            cam.zoom(ev.y);
        }
    }

    // 5. Clamp Target Coordinates to Map Bounds and Follow Smooth Terrain Altitude
    if let Some(ref terrain) = terrain_state {
        let max_w = terrain.map.width() as f32;
        let max_h = terrain.map.height() as f32;
        cam.target.x = cam.target.x.clamp(0.0, max_w);
        cam.target.y = cam.target.y.clamp(0.0, max_h);

        // Exponential easing toward surface altitude
        let sample_pos = SimVec2::new(
            SimScalar::from_num(cam.target.x),
            SimScalar::from_num(cam.target.y),
        );
        let target_alt = terrain.map.terrain_altitude_at(sample_pos);
        let ease_factor = 1.0 - (-CAMERA_HEIGHT_EASE_PER_SEC * dt).exp();
        cam.look_at_y += (target_alt - cam.look_at_y) * ease_factor;
    }

    // 6. Update Transform
    *transform = cam.compute_transform();
}
