//! Verifies real scheduled input ownership, projection coordinate spaces and
//! catalog-derived UI metadata. Native OS delivery is verified separately.

use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContext, EguiUserTextures};
use client::input_policy::{InputOwnership, UiGeometry, ViewportCoordinates};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn camera_app() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            40,
        )))
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .add_event::<MouseMotion>()
        .add_event::<bevy::window::WindowEvent>()
        .add_event::<AppExit>()
        .init_resource::<client::camera_drag::CameraDragState>()
        .add_event::<MouseWheel>()
        .init_resource::<EguiUserTextures>()
        .init_resource::<UiGeometry>()
        .insert_resource(InputOwnership::interactive());
    let mut window = Window {
        focused: true,
        ..default()
    };
    window.set_cursor_position(Some(Vec2::new(800.0, 300.0)));
    let primary = app
        .world_mut()
        .spawn((window, PrimaryWindow, EguiContext::default()))
        .id();
    let rts = client::RtsCamera::hero_framing();
    let camera = app
        .world_mut()
        .spawn((Camera3d::default(), rts.compute_transform(), rts))
        .id();
    app.world_mut().resource_mut::<UiGeometry>().world = Some(egui::Rect::from_min_max(
        egui::pos2(360.0, 0.0),
        egui::pos2(1280.0, 480.0),
    ));
    app.add_systems(
        Update,
        (
            client::input_policy::sample_input_ownership,
            client::camera_control_system,
        )
            .chain(),
    );
    app.update();
    (app, primary, camera)
}

#[test]
fn keyboard_pan_follows_projected_forward_and_focus_blocks_it() -> TestResult {
    let (mut app, primary, camera) = camera_app();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyW);
    app.update();
    let moved = app
        .world()
        .get::<client::RtsCamera>(camera)
        .ok_or("camera missing")?
        .target;
    assert!(moved.y < 0.0);
    let transform = client::RtsCamera::hero_framing().compute_transform();
    let local_forward = transform
        .compute_matrix()
        .inverse()
        .transform_vector3(Vec3::new(moved.x, 0.0, moved.y));
    assert!(
        local_forward.y > 0.0,
        "pan target advances toward projected screen top"
    );
    app.world_mut()
        .get_mut::<Window>(primary)
        .ok_or("window missing")?
        .focused = false;
    app.update();
    assert_eq!(
        app.world()
            .get::<client::RtsCamera>(camera)
            .ok_or("camera missing")?
            .target,
        moved
    );
    Ok(())
}

#[test]
fn current_egui_text_focus_owns_keyboard() -> TestResult {
    let (mut app, primary, camera) = camera_app();
    let context = app
        .world_mut()
        .get_mut::<EguiContext>(primary)
        .ok_or("egui missing")?
        .get_mut()
        .clone();
    context.begin_pass(egui::RawInput::default());
    let mut value = String::from("1.0");
    egui::CentralPanel::default().show(&context, |ui| {
        ui.text_edit_singleline(&mut value).request_focus();
    });
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyW);
    app.update();
    assert!(!app.world().resource::<InputOwnership>().keyboard);
    assert_eq!(
        app.world()
            .get::<client::RtsCamera>(camera)
            .ok_or("camera missing")?
            .target,
        Vec2::ZERO
    );
    let _ = context.end_pass();
    Ok(())
}

#[test]
fn wheel_requires_the_primary_window_and_unobscured_pointer() -> TestResult {
    let (mut app, primary, camera) = camera_app();
    assert!(app.world().resource::<InputOwnership>().pointer);
    let other = app.world_mut().spawn_empty().id();
    let before = app
        .world()
        .get::<client::RtsCamera>(camera)
        .ok_or("camera missing")?
        .distance;
    app.world_mut().send_event(MouseWheel {
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: 1.0,
        window: other,
    });
    app.update();
    assert_eq!(
        app.world()
            .get::<client::RtsCamera>(camera)
            .ok_or("camera missing")?
            .distance,
        before
    );
    app.world_mut().send_event(MouseWheel {
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: 1.0,
        window: primary,
    });
    app.update();
    let zoomed = app
        .world()
        .get::<client::RtsCamera>(camera)
        .ok_or("camera missing")?
        .distance;
    assert!(zoomed < before);
    app.world_mut()
        .get_mut::<Window>(primary)
        .ok_or("window missing")?
        .set_cursor_position(Some(Vec2::new(100.0, 200.0)));
    app.world_mut().send_event(MouseWheel {
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: 1.0,
        window: primary,
    });
    app.update();
    assert!(!app.world().resource::<InputOwnership>().pointer);
    assert_eq!(
        app.world()
            .get::<client::RtsCamera>(camera)
            .ok_or("camera missing")?
            .distance,
        zoomed
    );
    Ok(())
}

#[test]
fn viewport_origin_round_trips_window_picking_and_physical_capture_at_all_scales() -> TestResult {
    for scale in [1.0, 1.5, 2.0] {
        let coordinates = ViewportCoordinates {
            logical_rect: Rect::from_corners(Vec2::new(360.0, 24.0), Vec2::new(1280.0, 480.0)),
            scale,
        };
        let local = Vec2::new(460.0, 228.0);
        let window = coordinates.window_from_viewport(local);
        assert_eq!(window, Vec2::new(820.0, 252.0));
        assert_eq!(coordinates.viewport_from_window(window), Some(local));
        assert_eq!(
            coordinates.physical_from_window(window),
            Vec2::new(820.0 * scale, 252.0 * scale)
        );
        assert!(coordinates
            .viewport_from_window(Vec2::new(359.0, 200.0))
            .is_none());
        assert!(coordinates
            .viewport_from_window(Vec2::new(1280.0, 200.0))
            .is_none());
        let viewport = client::input_policy::physical_viewport(
            egui::Rect::from_min_max(egui::pos2(360.0, 24.0), egui::pos2(1280.0, 480.0)),
            scale,
            (Vec2::new(1280.0, 720.0) * scale).as_uvec2(),
        )
        .ok_or("viewport missing")?;
        assert_eq!(
            viewport.physical_position,
            (Vec2::new(360.0, 24.0) * scale).as_uvec2()
        );
        assert_eq!(
            viewport.physical_size,
            (Vec2::new(920.0, 456.0) * scale).as_uvec2()
        );
    }
    Ok(())
}

#[test]
#[allow(clippy::expect_used)]
fn wheel_units_and_grab_geometry_remain_consistent() {
    assert_eq!(
        client::camera::wheel_lines(40.0, MouseScrollUnit::Pixel),
        client::camera::wheel_lines(1.0, MouseScrollUnit::Line)
    );
    for eye in [Vec3::new(0.0, 8.0, 10.0), Vec3::new(0.0, 24.0, 30.0)] {
        let first = Vec3::new(-3.0, 1.0, -4.0);
        let second = Vec3::new(4.0, 1.0, 2.0);
        let previous = Ray3d::new(
            eye,
            Dir3::new(first - eye).expect("invariant: distinct eye and ground point"),
        );
        let current = Ray3d::new(
            eye,
            Dir3::new(second - eye).expect("invariant: distinct eye and ground point"),
        );
        let displacement = client::camera::focus_plane_drag(previous, current, 1.0)
            .expect("invariant: rays point toward the same plane");
        assert!((displacement - Vec2::new(-7.0, -6.0)).length() < 0.0001);
        let moved_eye = eye + Vec3::new(displacement.x, 0.0, displacement.y);
        let distance = (1.0 - moved_eye.y) / current.direction.y;
        assert!(
            (moved_eye + current.direction * distance - first).length() < 0.0001,
            "the current cursor ray remains on the original anchor after translating"
        );
    }
    let parallel = Ray3d::new(Vec3::Y, Dir3::X);
    let away = Ray3d::new(Vec3::Y, Dir3::Y);
    assert!(client::camera::focus_plane_drag(parallel, away, 0.0).is_none());
    assert!(client::camera::focus_plane_drag(away, away, 0.0).is_none());
}

#[test]
fn empty_world_viewport_deactivates_and_valid_resize_restores_camera() -> TestResult {
    let mut app = App::new();
    app.init_resource::<UiGeometry>()
        .add_systems(Update, client::input_policy::apply_world_viewport);
    // The UI publishes the real egui scale before viewport application.
    app.world_mut()
        .resource_mut::<UiGeometry>()
        .pixels_per_point = 1.0;
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    let camera = app
        .world_mut()
        .spawn((Camera::default(), client::RtsCamera::hero_framing()))
        .id();
    app.update();
    assert!(
        !app.world()
            .get::<Camera>(camera)
            .ok_or("camera missing")?
            .is_active
    );
    app.world_mut().resource_mut::<UiGeometry>().world = Some(egui::Rect::from_min_max(
        egui::pos2(360.0, 0.0),
        egui::pos2(1280.0, 480.0),
    ));
    app.update();
    assert!(
        app.world()
            .get::<Camera>(camera)
            .ok_or("camera missing")?
            .is_active
    );
    let valid = app
        .world()
        .get::<Camera>(camera)
        .ok_or("camera missing")?
        .viewport
        .clone();
    app.world_mut().resource_mut::<UiGeometry>().world = Some(egui::Rect::from_min_max(
        egui::pos2(360.0, 0.0),
        egui::pos2(360.0, 480.0),
    ));
    app.update();
    let hidden = app.world().get::<Camera>(camera).ok_or("camera missing")?;
    assert!(!hidden.is_active);
    assert_eq!(
        hidden.viewport.as_ref().map(|v| v.physical_size),
        valid.as_ref().map(|v| v.physical_size)
    );
    app.world_mut().resource_mut::<UiGeometry>().world = Some(egui::Rect::from_min_max(
        egui::pos2(360.0, 0.0),
        egui::pos2(960.0, 400.0),
    ));
    app.update();
    let restored = app.world().get::<Camera>(camera).ok_or("camera missing")?;
    assert!(restored.is_active);
    assert_eq!(
        restored.viewport.as_ref().map(|v| v.physical_size),
        Some(UVec2::new(600, 400))
    );
    Ok(())
}
