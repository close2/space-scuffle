//! Runs the production camera and engine projection systems with ordered window
//! events. These are scheduled integration tests, not claims of native OS delivery.

use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseWheel};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::render::camera::{camera_system, ManualTextureViews, Viewport};
use bevy::window::{
    PrimaryWindow, WindowCreated, WindowEvent, WindowFocused, WindowResized,
    WindowScaleFactorChanged,
};
use client::camera_drag::CameraDragState;
use client::input_policy::{InputOwnership, ViewportCoordinates};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Rig {
    app: App,
    window: Entity,
    camera: Entity,
}

impl Rig {
    fn new(scale: f32, distance: f32) -> Self {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .init_resource::<ManualTextureViews>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<CameraDragState>()
            .add_event::<AssetEvent<Image>>()
            .add_event::<WindowCreated>()
            .add_event::<WindowResized>()
            .add_event::<WindowScaleFactorChanged>()
            .add_event::<WindowEvent>()
            .add_event::<MouseMotion>()
            .add_event::<MouseWheel>()
            .add_event::<AppExit>();
        let mut window = Window {
            focused: true,
            ..default()
        };
        window.resolution.set_scale_factor_override(Some(scale));
        window.resolution.set(1280.0, 720.0);
        let window = app.world_mut().spawn((window, PrimaryWindow)).id();
        let mut rts = client::RtsCamera::hero_framing();
        rts.distance = distance;
        let camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                Camera {
                    viewport: Some(Viewport {
                        physical_position: (Vec2::new(360.0, 24.0) * scale).as_uvec2(),
                        physical_size: (Vec2::new(920.0, 456.0) * scale).as_uvec2(),
                        ..default()
                    }),
                    ..default()
                },
                rts.compute_transform(),
                rts,
            ))
            .id();
        app.insert_resource(InputOwnership {
            interactive: true,
            window: Some(window),
            keyboard: true,
            pointer: true,
        })
        .add_systems(PreUpdate, camera_system::<Projection>)
        .add_systems(Update, client::camera_control_system);
        app.update();
        app.update();
        Self {
            app,
            window,
            camera,
        }
    }

    fn cursor(&mut self, point: Vec2) {
        self.app
            .world_mut()
            .send_event(WindowEvent::CursorMoved(CursorMoved {
                window: self.window,
                position: point,
                delta: None,
            }));
    }
    fn press(&mut self) {
        self.app
            .world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Middle);
        self.app
            .world_mut()
            .send_event(WindowEvent::MouseButtonInput(MouseButtonInput {
                window: self.window,
                button: MouseButton::Middle,
                state: ButtonState::Pressed,
            }));
    }
    fn target(&self) -> TestResult<Vec2> {
        Ok(self
            .app
            .world()
            .get::<client::RtsCamera>(self.camera)
            .ok_or("camera missing")?
            .target)
    }
    fn focus(&mut self, focused: bool) -> TestResult {
        self.app
            .world_mut()
            .get_mut::<Window>(self.window)
            .ok_or("window missing")?
            .focused = focused;
        self.app
            .world_mut()
            .send_event(WindowEvent::WindowFocused(WindowFocused {
                window: self.window,
                focused,
            }));
        Ok(())
    }
}

#[test]
fn scheduled_grab_preserves_off_center_anchor_at_two_distances_and_scales() -> TestResult {
    for scale in [1.0, 1.5] {
        for distance in [9.5, 18.4] {
            let mut rig = Rig::new(scale, distance);
            let previous = Vec2::new(1010.0, 150.0);
            let current = Vec2::new(1070.0, 190.0);
            let lens = rig
                .app
                .world()
                .get::<Camera>(rig.camera)
                .ok_or("lens missing")?;
            let coordinates =
                ViewportCoordinates::from_camera(lens).ok_or("engine projection unavailable")?;
            let rts = rig
                .app
                .world()
                .get::<client::RtsCamera>(rig.camera)
                .ok_or("camera missing")?;
            let ray = lens
                .viewport_to_world(
                    &GlobalTransform::from(rts.compute_transform()),
                    coordinates
                        .viewport_from_window(previous)
                        .ok_or("outside viewport")?,
                )
                .map_err(|_| "engine ray conversion failed")?;
            let anchor =
                ray.origin + ray.direction * ((rts.look_at_y - ray.origin.y) / ray.direction.y);
            rig.cursor(previous);
            rig.press();
            rig.app.update();
            assert_eq!(rig.target()?, Vec2::ZERO, "press only anchors");
            rig.cursor(current);
            rig.app.update();
            assert_ne!(rig.target()?, Vec2::ZERO);
            let lens = rig
                .app
                .world()
                .get::<Camera>(rig.camera)
                .ok_or("lens missing")?;
            let pose = GlobalTransform::from(
                *rig.app
                    .world()
                    .get::<Transform>(rig.camera)
                    .ok_or("transform missing")?,
            );
            let projected = client::input_policy::world_to_window(lens, &pose, anchor)
                .ok_or("anchor projection failed")?;
            assert!((projected-current).length() < 0.005, "actual projected anchor {projected} differs from pointer {current}, scale={scale}, distance={distance}");
        }
    }
    Ok(())
}

#[test]
fn scheduled_focus_cancellation_ignores_retained_button_until_raw_fresh_press() -> TestResult {
    let mut rig = Rig::new(1.5, 9.5);
    rig.cursor(Vec2::new(800.0, 200.0));
    rig.press();
    rig.app.update();
    rig.cursor(Vec2::new(820.0, 220.0));
    rig.app.update();
    let before = rig.target()?;
    rig.focus(false)?;
    rig.focus(true)?;
    rig.cursor(Vec2::new(850.0, 240.0));
    rig.app.update();
    assert_eq!(rig.target()?, before);
    assert!(rig
        .app
        .world()
        .resource::<ButtonInput<MouseButton>>()
        .pressed(MouseButton::Middle));
    assert!(
        !rig.app
            .world()
            .resource::<CameraDragState>()
            .diagnostics()
            .active
    );
    rig.cursor(Vec2::new(870.0, 250.0));
    rig.app.update();
    assert_eq!(
        rig.target()?,
        before,
        "delivered motion cannot revive cancelled ownership"
    );
    rig.press();
    rig.app.update();
    assert_eq!(rig.target()?, before, "new press anchors without a jump");
    rig.cursor(Vec2::new(890.0, 260.0));
    rig.app.update();
    assert_ne!(rig.target()?, before);
    Ok(())
}

#[test]
fn scheduled_outside_crossing_and_camera_replacement_cancel() -> TestResult {
    let mut rig = Rig::new(1.0, 9.5);
    rig.cursor(Vec2::new(800.0, 200.0));
    rig.press();
    rig.app.update();
    rig.cursor(Vec2::new(359.0, 200.0));
    rig.cursor(Vec2::new(850.0, 200.0));
    rig.app.update();
    assert_eq!(rig.target()?, Vec2::ZERO);
    rig.press();
    rig.app.update();
    let lens = rig
        .app
        .world()
        .get::<Camera>(rig.camera)
        .ok_or("lens missing")?
        .clone();
    rig.app.world_mut().despawn(rig.camera);
    let rts = client::RtsCamera::hero_framing();
    rig.camera = rig
        .app
        .world_mut()
        .spawn((Camera3d::default(), lens, rts.compute_transform(), rts))
        .id();
    rig.cursor(Vec2::new(900.0, 200.0));
    rig.app.update();
    assert_eq!(rig.target()?, Vec2::ZERO);
    assert!(
        !rig.app
            .world()
            .resource::<CameraDragState>()
            .diagnostics()
            .active
    );
    Ok(())
}
