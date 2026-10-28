//! Gesture lifecycle regressions exercise ordered delivery independently of global
//! button state. Projection geometry is covered by the camera's ray tests.

#![allow(clippy::expect_used)]

use super::*;
use bevy::input::mouse::MouseButtonInput;
use bevy::window::{
    CursorLeft, CursorMoved, WindowFocused, WindowResized, WindowScaleFactorChanged,
};

fn context() -> DragContext {
    DragContext {
        window: Entity::from_raw(1),
        camera: Entity::from_raw(3),
        focused: true,
        pointer_owned: true,
        viewport: Some(ViewportCoordinates {
            logical_rect: Rect::new(100.0, 0.0, 500.0, 400.0),
            scale: 1.5,
        }),
    }
}

fn cursor(window: Entity, x: f32) -> WindowEvent {
    WindowEvent::CursorMoved(CursorMoved {
        window,
        position: Vec2::new(x, 100.0),
        delta: None,
    })
}

fn button(window: Entity, state: ButtonState) -> WindowEvent {
    WindowEvent::MouseButtonInput(MouseButtonInput {
        window,
        button: MouseButton::Middle,
        state,
    })
}

fn apply(state: &mut CameraDragState, context: DragContext, events: &[WindowEvent]) -> Vec2 {
    state
        .advance(
            Some(context),
            events.iter().any(|e| geometry_event(e, context.window)),
            events.iter(),
            |a, b| Some(a - b),
        )
        .expect("test counters have ample capacity")
}

fn armed() -> (CameraDragState, DragContext) {
    let context = context();
    let mut state = CameraDragState::default();
    apply(&mut state, context, &[]);
    apply(
        &mut state,
        context,
        &[
            cursor(context.window, 200.0),
            button(context.window, ButtonState::Pressed),
        ],
    );
    assert!(state.diagnostics().active);
    (state, context)
}

#[test]
fn fresh_press_uses_preceding_position_and_release_ends_movement() {
    let (mut state, context) = armed();
    assert_eq!(
        apply(&mut state, context, &[cursor(context.window, 230.0)]),
        Vec2::new(-30.0, 0.0)
    );
    assert_eq!(
        apply(
            &mut state,
            context,
            &[
                button(context.window, ButtonState::Released),
                cursor(context.window, 250.0)
            ]
        ),
        Vec2::ZERO
    );
    assert_eq!(
        state.diagnostics().last_cancellation,
        DragCancellation::Release
    );
}

#[test]
fn focus_loss_restore_and_motion_never_rearm_without_fresh_press() {
    let (mut state, context) = armed();
    let events = [
        WindowEvent::WindowFocused(WindowFocused {
            window: context.window,
            focused: false,
        }),
        WindowEvent::WindowFocused(WindowFocused {
            window: context.window,
            focused: true,
        }),
        cursor(context.window, 240.0),
    ];
    assert_eq!(apply(&mut state, context, &events), Vec2::ZERO);
    assert!(!state.diagnostics().active);
    assert_eq!(
        apply(&mut state, context, &[cursor(context.window, 260.0)]),
        Vec2::ZERO
    );
    assert_eq!(
        apply(
            &mut state,
            context,
            &[
                button(context.window, ButtonState::Pressed),
                cursor(context.window, 280.0)
            ]
        ),
        Vec2::new(-20.0, 0.0)
    );
    assert_eq!(state.diagnostics().accepted_presses, 2);
}

#[test]
fn ui_origin_press_cannot_arm_from_final_inside_position() {
    let context = context();
    let mut state = CameraDragState::default();
    apply(&mut state, context, &[]);
    assert_eq!(
        apply(
            &mut state,
            context,
            &[
                cursor(context.window, 50.0),
                button(context.window, ButtonState::Pressed),
                cursor(context.window, 240.0)
            ]
        ),
        Vec2::ZERO
    );
    assert!(!state.diagnostics().active);
    // No preceding delivered cursor is also insufficient, even if the final
    // Window.cursor_position would be inside by the time the system executes.
    state.cursor = None;
    apply(
        &mut state,
        context,
        &[
            button(context.window, ButtonState::Pressed),
            cursor(context.window, 280.0),
        ],
    );
    assert!(!state.diagnostics().active);
}

#[test]
fn outside_inside_crossing_cancels_even_with_final_owned_cursor() {
    let (mut state, context) = armed();
    assert_eq!(
        apply(
            &mut state,
            context,
            &[cursor(context.window, 99.0), cursor(context.window, 240.0)]
        ),
        Vec2::ZERO
    );
    assert_eq!(
        state.diagnostics().last_cancellation,
        DragCancellation::CursorDeparture
    );
    assert!(!state.diagnostics().active);
}

#[test]
fn geometry_event_discards_mixed_epoch_motion_even_when_final_rect_matches() {
    let (mut state, context) = armed();
    let events = [
        cursor(context.window, 230.0),
        WindowEvent::WindowResized(WindowResized {
            window: context.window,
            width: 800.0,
            height: 600.0,
        }),
        WindowEvent::WindowResized(WindowResized {
            window: context.window,
            width: 500.0,
            height: 400.0,
        }),
        cursor(context.window, 240.0),
        button(context.window, ButtonState::Pressed),
    ];
    assert_eq!(apply(&mut state, context, &events), Vec2::ZERO);
    assert!(!state.diagnostics().active);
    assert_eq!(
        state.diagnostics().last_cancellation,
        DragCancellation::Geometry
    );
    assert_eq!(
        apply(&mut state, context, &[cursor(context.window, 250.0)]),
        Vec2::ZERO
    );
}

#[test]
fn scale_change_clears_stale_cursor_until_new_delivery() {
    let (mut state, context) = armed();
    apply(
        &mut state,
        context,
        &[WindowEvent::WindowScaleFactorChanged(
            WindowScaleFactorChanged {
                window: context.window,
                scale_factor: 2.0,
            },
        )],
    );
    apply(
        &mut state,
        context,
        &[button(context.window, ButtonState::Pressed)],
    );
    assert!(!state.diagnostics().active);
    apply(
        &mut state,
        context,
        &[
            cursor(context.window, 250.0),
            button(context.window, ButtonState::Pressed),
        ],
    );
    assert!(state.diagnostics().active);
}

#[test]
fn ownership_loss_and_window_departure_both_require_new_press() {
    let (mut state, mut context) = armed();
    context.pointer_owned = false;
    apply(&mut state, context, &[]);
    context.pointer_owned = true;
    assert_eq!(
        apply(&mut state, context, &[cursor(context.window, 220.0)]),
        Vec2::ZERO
    );
    apply(
        &mut state,
        context,
        &[button(context.window, ButtonState::Pressed)],
    );
    apply(
        &mut state,
        context,
        &[WindowEvent::CursorLeft(CursorLeft {
            window: context.window,
        })],
    );
    assert_eq!(
        apply(&mut state, context, &[cursor(context.window, 240.0)]),
        Vec2::ZERO
    );
    assert_eq!(state.diagnostics().cancellations, 2);
}

#[test]
fn foreign_window_events_cannot_move_or_cancel_owned_drag() {
    let (mut state, context) = armed();
    let foreign = Entity::from_raw(2);
    assert_eq!(
        apply(
            &mut state,
            context,
            &[
                cursor(foreign, 50.0),
                button(foreign, ButtonState::Released),
                WindowEvent::WindowFocused(WindowFocused {
                    window: foreign,
                    focused: false
                })
            ]
        ),
        Vec2::ZERO
    );
    assert!(state.diagnostics().active);
    assert_eq!(
        apply(&mut state, context, &[cursor(context.window, 220.0)]),
        Vec2::new(-20.0, 0.0)
    );
}

#[test]
fn absent_owner_drains_events_and_invalid_ray_cancels() {
    let (mut state, context) = armed();
    let events = [button(context.window, ButtonState::Pressed)];
    let mut consumed = 0;
    state
        .advance(
            None,
            false,
            events.iter().inspect(|_| consumed += 1),
            |_, _| None,
        )
        .expect("small counters");
    assert_eq!(consumed, 1);
    assert!(!state.diagnostics().active);
    let (mut state, context) = armed();
    assert_eq!(
        state
            .advance(
                Some(context),
                false,
                [cursor(context.window, 220.0)].iter(),
                |_, _| None
            )
            .expect("small counters"),
        Vec2::ZERO
    );
    assert_eq!(
        state.diagnostics().last_cancellation,
        DragCancellation::InvalidRay
    );
}

#[test]
fn counter_overflow_is_explicit_and_failed_state_never_moves() {
    let (mut state, context) = armed();
    state.evidence.cancellations = u64::MAX;
    assert!(state
        .advance(
            Some(context),
            false,
            [button(context.window, ButtonState::Released)].iter(),
            |a, b| Some(a - b)
        )
        .is_err());
    assert_eq!(
        apply(
            &mut state,
            context,
            &[
                button(context.window, ButtonState::Pressed),
                cursor(context.window, 240.0)
            ]
        ),
        Vec2::ZERO
    );
    assert!(!state.diagnostics().active);
}

#[test]
fn same_window_camera_replacement_requires_a_new_cursor_and_press() {
    let (mut state, mut context) = armed();
    context.camera = Entity::from_raw(4);
    assert_eq!(
        apply(&mut state, context, &[cursor(context.window, 240.0)]),
        Vec2::ZERO
    );
    assert!(!state.diagnostics().active);
    assert_eq!(
        state.diagnostics().last_cancellation,
        DragCancellation::MissingWindow
    );
    apply(
        &mut state,
        context,
        &[button(context.window, ButtonState::Pressed)],
    );
    assert!(state.diagnostics().active);
}

#[test]
fn late_geometry_event_never_admits_an_earlier_press() {
    let context = context();
    let mut state = CameraDragState::default();
    apply(&mut state, context, &[]);
    apply(
        &mut state,
        context,
        &[
            cursor(context.window, 200.0),
            button(context.window, ButtonState::Pressed),
            WindowEvent::WindowResized(WindowResized {
                window: context.window,
                width: 500.0,
                height: 400.0,
            }),
        ],
    );
    assert_eq!(state.diagnostics().accepted_presses, 0);
    assert!(!state.diagnostics().active);
}
