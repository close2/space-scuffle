//! StarCraft II style RTS perspective camera controller.
//!
//! # Architectural Responsibilities
//! This module owns RTS camera locomotion, 58° perspective pitch projection, smooth WASD/arrow
//! keyboard panning, middle-mouse grab-scroll, scroll-wheel exponential zoom, and hotkey
//! camera bookmarks (Ctrl+F1..F4 save, F1..F4 recall).
//!
//! # Invariants
//! - Pitch angle is fixed at 58.0 degrees (`~1.012` radians) matching classical RTS overview framing.
//! - Zoom distance is clamped to `[3.0, 48.0]` world units.
//! - Camera look target sits squarely on the ground plane (y = 0 or terrain surface).

use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use std::collections::HashMap;

/// Minimum zoom distance in world units for close inspection.
pub const MIN_CAMERA_DISTANCE: f32 = 3.0;

/// Maximum zoom distance in world units for full battlefield overview.
pub const MAX_CAMERA_DISTANCE: f32 = 48.0;

/// Default StarCraft II perspective pitch in radians (58.0 degrees).
pub const DEFAULT_CAMERA_PITCH_RAD: f32 = 58.0 * (std::f32::consts::PI / 180.0);

/// Component configuring an RTS perspective camera.
#[derive(Component, Debug, Clone)]
pub struct RtsCamera {
    /// Ground focus point on the horizontal plane (x = world x, y = world z).
    pub target: Vec2,
    /// Distance from focus point along the view direction.
    pub distance: f32,
    /// Pitch angle in radians relative to horizontal.
    pub pitch: f32,
    /// Keyboard panning speed in world units per second.
    pub pan_speed: f32,
    /// Look target elevation above ground plane (defaults to 1.2 for hero framing).
    pub look_at_y: f32,
    /// Saved camera bookmarks mapping hotkey to (target, distance).
    pub bookmarks: HashMap<KeyCode, (Vec2, f32)>,
}

impl Default for RtsCamera {
    fn default() -> Self {
        Self {
            target: Vec2::new(16.0, 32.0),
            distance: 22.0,
            pitch: DEFAULT_CAMERA_PITCH_RAD,
            pan_speed: 25.0,
            look_at_y: 1.2,
            bookmarks: HashMap::new(),
        }
    }
}

impl RtsCamera {
    /// Creates an RTS camera preset framed closely on central hero entities (distance 9.5, looking at (0.0, 1.2, 0.0)).
    #[must_use]
    pub fn hero_framing() -> Self {
        Self {
            target: Vec2::new(0.0, 0.0),
            distance: 9.5,
            pitch: DEFAULT_CAMERA_PITCH_RAD,
            pan_speed: 25.0,
            look_at_y: 1.2,
            bookmarks: HashMap::new(),
        }
    }

    /// Applies directional pan velocity clamped by delta time.
    pub fn pan(&mut self, direction: Vec2, dt: f32) {
        if direction.length_squared() > 0.0 {
            self.target += direction.normalize() * self.pan_speed * dt;
        }
    }

    /// Adjusts zoom distance with smooth clamping to `[MIN_CAMERA_DISTANCE, MAX_CAMERA_DISTANCE]`.
    pub fn zoom(&mut self, scroll_delta: f32) {
        let zoom_step = -scroll_delta * (self.distance * 0.12);
        self.distance = (self.distance + zoom_step).clamp(MIN_CAMERA_DISTANCE, MAX_CAMERA_DISTANCE);
    }

    /// Stores current camera bookmark for the specified key code.
    pub fn save_bookmark(&mut self, key: KeyCode) {
        self.bookmarks.insert(key, (self.target, self.distance));
    }

    /// Recalls a previously stored camera bookmark, returning true if found.
    pub fn recall_bookmark(&mut self, key: &KeyCode) -> bool {
        if let Some(&(target, distance)) = self.bookmarks.get(key) {
            self.target = target;
            self.distance = distance;
            true
        } else {
            false
        }
    }

    // PERFORMANCE RATIONALE:
    // Why: Computing the camera transform involves simple trigonometric rotation and offset math.
    // What: Pure analytical quaternion computation without matrix inversion or allocation.
    // Trade-off: Cheap O(1) transform update executed once per render frame.
    /// Computes the 3D world transform for this camera's current state.
    #[must_use]
    pub fn compute_transform(&self) -> Transform {
        let rotation = Quat::from_rotation_x(-self.pitch);
        let offset = rotation * Vec3::new(0.0, 0.0, self.distance);
        let ground_center = Vec3::new(self.target.x, 0.0, self.target.y);
        let eye_pos = ground_center + offset;
        let look_at_pos = Vec3::new(self.target.x, self.look_at_y, self.target.y);
        Transform::from_translation(eye_pos).looking_at(look_at_pos, Vec3::Y)
    }
}

/// System controlling RTS camera movement via keyboard, mouse grab, scroll wheel, and bookmarks.
pub fn camera_control_system(
    time: Res<'_, Time>,
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    mouse_buttons: Res<'_, ButtonInput<MouseButton>>,
    mut mouse_motion_events: EventReader<'_, '_, MouseMotion>,
    mut scroll_events: EventReader<'_, '_, MouseWheel>,
    mut query: Query<'_, '_, (&mut RtsCamera, &mut Transform)>,
) {
    let Ok((mut cam, mut transform)) = query.get_single_mut() else {
        return;
    };
    let dt = time.delta_secs();

    // 1. Keyboard Panning (WASD / Arrow Keys)
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

    // 2. Middle-Mouse Drag Pan
    if mouse_buttons.pressed(MouseButton::Middle) {
        for ev in mouse_motion_events.read() {
            let factor = cam.distance * 0.0018;
            cam.target.x -= ev.delta.x * factor;
            cam.target.y += ev.delta.y * factor;
        }
    } else {
        mouse_motion_events.clear();
    }

    // 3. Scroll Wheel Zoom
    for ev in scroll_events.read() {
        cam.zoom(ev.y);
    }

    // 4. Camera Bookmarks (Ctrl+F1..F4 save, F1..F4 recall)
    let ctrl = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    for key in [KeyCode::F1, KeyCode::F2, KeyCode::F3, KeyCode::F4] {
        if keyboard.just_pressed(key) {
            if ctrl {
                cam.save_bookmark(key);
            } else {
                cam.recall_bookmark(&key);
            }
        }
    }

    // Update Transform
    *transform = cam.compute_transform();
}

/// Canonical alias for the camera control system.
pub use camera_control_system as update_camera_system;
