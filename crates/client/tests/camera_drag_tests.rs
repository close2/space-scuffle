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

/// Map whose cells at `x >= 16` are one cliff level above the rest, and that level's altitude.
fn cliff_terrain() -> TestResult<(client::terrain::TerrainState, f32)> {
    let cliff = sim_map::CliffLevel(1);
    let mut map = sim_map::GameMap::new(
        32,
        32,
        sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
    )?;
    for y in 0..32 {
        for x in 16..32 {
            map.cell_mut(x, y)?.cliff_level = cliff;
        }
    }
    Ok((
        client::terrain::TerrainState::new(map),
        cliff.height_scalar().to_f32(),
    ))
}

/// C-14: the look-at height jumped to the new altitude on the first frame over a cliff.
#[test]
fn camera_eases_its_height_when_panning_over_a_cliff() -> TestResult {
    const FRAME: std::time::Duration = std::time::Duration::from_millis(16);

    let mut rig = Rig::new(1.0, 20.0);
    let (terrain, cliff_height) = cliff_terrain()?;
    assert!(cliff_height > 1.0);
    rig.app
        .insert_resource(terrain)
        .insert_resource(client::LabState {
            current_tab: client::LabTab::LiveBattle,
            ..default()
        })
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(FRAME));
    let set_target = |rig: &mut Rig, target: Vec2| -> TestResult {
        rig.app
            .world_mut()
            .get_mut::<client::RtsCamera>(rig.camera)
            .ok_or("camera")?
            .target = target;
        Ok(())
    };
    let look_at_y = |rig: &Rig| -> TestResult<f32> {
        Ok(rig
            .app
            .world()
            .get::<client::RtsCamera>(rig.camera)
            .ok_or("camera")?
            .look_at_y)
    };

    set_target(&mut rig, Vec2::new(8.0, 8.0))?;
    for _ in 0..120 {
        rig.app.update();
    }
    let low = look_at_y(&rig)?;
    assert!((low - 1.2).abs() < 0.01, "settled on low ground at {low}");

    set_target(&mut rig, Vec2::new(24.0, 8.0))?;
    rig.app.update();
    rig.app.update();
    let high = low + cliff_height;
    let early = look_at_y(&rig)?;
    assert!(
        early > low && early < low + (high - low) * 0.5,
        "{early} after two frames between {low} and {high}"
    );

    let mut previous = early;
    for _ in 0..120 {
        rig.app.update();
        let current = look_at_y(&rig)?;
        assert!(current >= previous, "height must not overshoot back");
        previous = current;
    }
    assert!(
        (previous - high).abs() < 0.01,
        "{previous} settles at {high}"
    );
    Ok(())
}
