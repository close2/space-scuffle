//! Records delivered native button/window events separately from engine held state.
//! This passive evidence distinguishes lost delivery from gesture cancellation;
//! counters are checked and write failures use the probe's normal exit path.

use std::io::Write;
use std::sync::atomic::Ordering;

use bevy::input::mouse::MouseButtonInput;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowEvent};

use super::Evidence;
use client::camera_drag::{CameraDragState, DragDiagnostics};

pub(super) const HEADER: &str = ",middle_down,middle_up,foreign_middle,last_middle_window,last_middle_pressed,window_event_seq,middle_seq,focus_lost_seq,focus_gained_seq,cursor_seq,geometry_seq,drag_active,drag_window,drag_presses,drag_cancels,drag_cancel_reason,delivered_cursor_x,delivered_cursor_y,cursor_leaves";

#[derive(Default)]
pub(super) struct ButtonDelivery {
    down: u64,
    up: u64,
    foreign: u64,
    last_window: u64,
    last_pressed: bool,
    sequence: u64,
    middle_sequence: u64,
    focus_lost: u64,
    focus_gained: u64,
    cursor: u64,
    geometry: u64,
    drag: DragDiagnostics,
    delivered_cursor: Option<Vec2>,
    cursor_leaves: u64,
}

#[derive(Debug, thiserror::Error)]
#[error("native input telemetry counter overflow")]
struct CounterOverflow;

fn increment(counter: &mut u64) -> Result<(), CounterOverflow> {
    *counter = counter.checked_add(1).ok_or(CounterOverflow)?;
    Ok(())
}

impl ButtonDelivery {
    fn button(
        &mut self,
        event: &MouseButtonInput,
        primary: Option<Entity>,
    ) -> Result<(), CounterOverflow> {
        if event.button != MouseButton::Middle {
            return Ok(());
        }
        self.last_window = event.window.to_bits();
        self.last_pressed = event.state.is_pressed();
        if Some(event.window) != primary {
            increment(&mut self.foreign)
        } else if self.last_pressed {
            increment(&mut self.down)
        } else {
            increment(&mut self.up)
        }
    }

    fn event(
        &mut self,
        event: &WindowEvent,
        primary: Option<Entity>,
    ) -> Result<(), CounterOverflow> {
        increment(&mut self.sequence)?;
        match event {
            WindowEvent::MouseButtonInput(e)
                if Some(e.window) == primary && e.button == MouseButton::Middle =>
            {
                self.middle_sequence = self.sequence;
            }
            WindowEvent::WindowFocused(e) if Some(e.window) == primary => {
                if e.focused {
                    self.focus_gained = self.sequence;
                } else {
                    self.focus_lost = self.sequence;
                }
            }
            WindowEvent::CursorMoved(e) if Some(e.window) == primary => {
                self.cursor = self.sequence;
                self.delivered_cursor = Some(e.position);
            }
            WindowEvent::CursorLeft(e) if Some(e.window) == primary => {
                increment(&mut self.cursor_leaves)?;
            }
            WindowEvent::WindowResized(e) if Some(e.window) == primary => {
                self.geometry = self.sequence;
            }
            WindowEvent::WindowScaleFactorChanged(e) if Some(e.window) == primary => {
                self.geometry = self.sequence;
            }
            WindowEvent::WindowBackendScaleFactorChanged(e) if Some(e.window) == primary => {
                self.geometry = self.sequence;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn write(&self, writer: &mut impl Write) -> std::io::Result<()> {
        write!(
            writer,
            ",{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.down,
            self.up,
            self.foreign,
            self.last_window,
            u8::from(self.last_pressed),
            self.sequence,
            self.middle_sequence,
            self.focus_lost,
            self.focus_gained,
            self.cursor,
            self.geometry,
            u8::from(self.drag.active),
            self.drag.window.map_or(0, Entity::to_bits),
            self.drag.accepted_presses,
            self.drag.cancellations,
            self.drag.last_cancellation as u8
        )?;
        let cursor = self.delivered_cursor.unwrap_or(Vec2::splat(-1.0));
        write!(writer, ",{},{},{}", cursor.x, cursor.y, self.cursor_leaves)
    }
}

pub(super) fn record(
    mut evidence: ResMut<'_, Evidence>,
    mut buttons: EventReader<'_, '_, MouseButtonInput>,
    mut events: EventReader<'_, '_, WindowEvent>,
    windows: Query<'_, '_, Entity, With<PrimaryWindow>>,
    drag: Res<'_, CameraDragState>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let primary = windows.get_single().ok();
    let result = buttons
        .read()
        .try_for_each(|event| evidence.button_delivery.button(event, primary))
        .and_then(|()| {
            events
                .read()
                .try_for_each(|event| evidence.button_delivery.event(event, primary))
        });
    evidence.button_delivery.drag = drag.diagnostics();
    if let Err(error) = result {
        if !evidence.failed.swap(true, Ordering::AcqRel) {
            error!("Native input evidence failed: {error}");
            exit.send(AppExit::error());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::ButtonState;
    use bevy::window::WindowFocused;

    #[test]
    fn raw_delivery_and_order_counters_are_distinct_from_held_state() -> Result<(), CounterOverflow>
    {
        let primary = Entity::from_raw(1);
        let mut evidence = ButtonDelivery::default();
        let press = MouseButtonInput {
            window: primary,
            button: MouseButton::Middle,
            state: ButtonState::Pressed,
        };
        evidence.button(&press, Some(primary))?;
        evidence.event(&WindowEvent::MouseButtonInput(press), Some(primary))?;
        evidence.event(
            &WindowEvent::WindowFocused(WindowFocused {
                window: primary,
                focused: false,
            }),
            Some(primary),
        )?;
        evidence.event(
            &WindowEvent::WindowFocused(WindowFocused {
                window: primary,
                focused: true,
            }),
            Some(primary),
        )?;
        assert_eq!((evidence.down, evidence.up), (1, 0));
        assert_eq!(
            (
                evidence.middle_sequence,
                evidence.focus_lost,
                evidence.focus_gained
            ),
            (1, 2, 3)
        );
        assert!(
            !evidence.drag.active,
            "raw telemetry does not grant gesture ownership"
        );
        evidence.button(
            &MouseButtonInput {
                window: Entity::from_raw(2),
                button: MouseButton::Middle,
                state: ButtonState::Released,
            },
            Some(primary),
        )?;
        assert_eq!((evidence.foreign, evidence.up), (1, 0));
        Ok(())
    }

    #[test]
    fn native_evidence_overflow_is_not_wrapped() {
        let mut evidence = ButtonDelivery {
            sequence: u64::MAX,
            ..Default::default()
        };
        assert!(evidence
            .event(
                &WindowEvent::WindowFocused(WindowFocused {
                    window: Entity::from_raw(1),
                    focused: false
                }),
                Some(Entity::from_raw(1))
            )
            .is_err());
        assert_eq!(evidence.sequence, u64::MAX);
    }
}
