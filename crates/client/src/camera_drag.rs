//! Owns one primary-window camera-grab gesture from accepted press to cancellation.
//!
//! Ordered native events preserve press-time positions and transient focus/viewport
//! changes. A retained global mouse button never grants gesture ownership. State and
//! diagnostics are fixed-size; geometry remains the camera's focus-plane projection.

use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::WindowEvent;

use crate::input_policy::ViewportCoordinates;

/// Reason the camera stopped owning a middle-button gesture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum DragCancellation {
    /// No gesture has been cancelled.
    #[default]
    None,
    /// Owning window lost focus.
    Focus,
    /// UI or pointer position owns interaction.
    Ownership,
    /// Viewport or scale changed or became unavailable.
    Geometry,
    /// Cursor left its window or world viewport.
    CursorDeparture,
    /// Native middle-button release arrived.
    Release,
    /// A focus-plane intersection was unavailable.
    InvalidRay,
    /// Owning window/camera was removed or replaced.
    MissingWindow,
}

/// Passive derived gesture evidence; never authoritative game state.
#[derive(Clone, Copy, Debug, Default)]
pub struct DragDiagnostics {
    /// True only after an accepted native press that has not been cancelled.
    pub active: bool,
    /// Window owning the active gesture, if any.
    pub window: Option<Entity>,
    /// Number of accepted native middle presses.
    pub accepted_presses: u64,
    /// Number of active gestures cancelled.
    pub cancellations: u64,
    /// Last reason for cancelling an active gesture.
    pub last_cancellation: DragCancellation,
}

/// Explicit failure rather than wrapping persistent gesture evidence counters.
#[derive(Debug, thiserror::Error)]
#[error("camera drag evidence counter overflow")]
pub struct DragCounterOverflow;

#[derive(Clone, Copy)]
pub(crate) struct DragContext {
    pub window: Entity,
    pub camera: Entity,
    pub focused: bool,
    pub pointer_owned: bool,
    pub viewport: Option<ViewportCoordinates>,
}

/// Persistent primary-camera drag state, initialized explicitly by its embedding.
#[derive(Resource, Default)]
pub struct CameraDragState {
    primary: Option<(Entity, Entity)>,
    focused: bool,
    cursor: Option<Vec2>,
    anchor: Option<Vec2>,
    geometry: Option<(Rect, u32)>,
    evidence: DragDiagnostics,
    failed: bool,
}

/// Required event readers and gesture state for the production camera system.
/// Independent readers inspect the same native batch without collecting events.
#[derive(bevy::ecs::system::SystemParam)]
pub struct CameraDragInput<'w, 's> {
    state: ResMut<'w, CameraDragState>,
    events: EventReader<'w, 's, WindowEvent>,
    geometry_events: EventReader<'w, 's, WindowEvent>,
    exit: EventWriter<'w, AppExit>,
}

impl CameraDragInput<'_, '_> {
    pub(crate) fn cancel_missing(&mut self) {
        let result = self
            .state
            .advance(None, false, self.events.read(), |_, _| None);
        self.geometry_events.clear();
        self.report(result);
    }

    fn report(&mut self, result: Result<Vec2, DragCounterOverflow>) -> Vec2 {
        match result {
            Ok(movement) => movement,
            Err(error) => {
                error!("Camera drag failed: {error}");
                self.exit.send(AppExit::error());
                Vec2::ZERO
            }
        }
    }

    pub(crate) fn update(
        &mut self,
        camera: Entity,
        controller: &mut crate::RtsCamera,
        lens: &Camera,
        window: Option<(Entity, &Window)>,
        ownership: &crate::input_policy::InputOwnership,
    ) {
        let coordinates = ViewportCoordinates::from_camera(lens);
        let context = window.map(|(entity, window)| DragContext {
            window: entity,
            camera,
            focused: window.focused,
            pointer_owned: ownership.pointer && ownership.window == Some(entity) && lens.is_active,
            viewport: coordinates,
        });
        // PERFORMANCE RATIONALE:
        // Why: A late geometry event must deny all presses in this same batch.
        // What: A second independent event cursor scans before ordered evaluation.
        // Trade-off: Two linear passes over delivered events avoid a scratch Vec.
        let geometry_changed = self.geometry_events.read().fold(false, |changed, event| {
            changed | context.is_some_and(|c| geometry_event(event, c.window))
        });
        let pose = GlobalTransform::from(controller.compute_transform());
        let result = self.state.advance(
            context,
            geometry_changed,
            self.events.read(),
            |previous, current| {
                let coordinates = coordinates?;
                crate::camera::focus_plane_drag(
                    lens.viewport_to_world(&pose, coordinates.viewport_from_window(previous)?)
                        .ok()?,
                    lens.viewport_to_world(&pose, coordinates.viewport_from_window(current)?)
                        .ok()?,
                    controller.look_at_y,
                )
            },
        );
        controller.target += self.report(result);
    }
}

impl CameraDragState {
    /// Returns fixed-size read-only evidence after the ordinary camera update.
    #[must_use]
    pub fn diagnostics(&self) -> DragDiagnostics {
        self.evidence
    }

    fn cancel(&mut self, reason: DragCancellation) -> Result<(), DragCounterOverflow> {
        if self.evidence.active {
            self.evidence.cancellations = self
                .evidence
                .cancellations
                .checked_add(1)
                .ok_or(DragCounterOverflow)?;
            self.evidence.last_cancellation = reason;
        }
        self.evidence.active = false;
        self.evidence.window = None;
        self.anchor = None;
        Ok(())
    }

    pub(crate) fn advance<'a>(
        &mut self,
        context: Option<DragContext>,
        geometry_changed: bool,
        mut events: impl Iterator<Item = &'a WindowEvent>,
        mut displacement: impl FnMut(Vec2, Vec2) -> Option<Vec2>,
    ) -> Result<Vec2, DragCounterOverflow> {
        let result = self.advance_inner(context, geometry_changed, &mut events, &mut displacement);
        // Even an absent window or failed counter consumes this frame's events.
        // Events from a previous owner must never arm a replacement camera.
        events.for_each(|_| {});
        if result.is_err() {
            self.failed = true;
            self.evidence.active = false;
            self.evidence.window = None;
            self.anchor = None;
        }
        result
    }

    fn prepare(&mut self, context: DragContext) -> Result<bool, DragCounterOverflow> {
        if self.primary != Some((context.window, context.camera)) {
            self.cancel(DragCancellation::MissingWindow)?;
            self.primary = Some((context.window, context.camera));
            self.focused = context.focused;
            self.cursor = None;
        }
        let geometry = context
            .viewport
            .map(|v| (v.logical_rect, v.scale.to_bits()));
        let transition = self.geometry != geometry;
        if transition || geometry.is_none() {
            self.cancel(DragCancellation::Geometry)?;
            self.cursor = None;
        }
        self.geometry = geometry;
        if !context.focused {
            self.cancel(DragCancellation::Focus)?;
            self.focused = false;
            self.cursor = None;
        }
        if !context.pointer_owned {
            self.cancel(DragCancellation::Ownership)?;
        }
        Ok(transition)
    }

    fn advance_inner<'a>(
        &mut self,
        context: Option<DragContext>,
        geometry_changed: bool,
        events: &mut impl Iterator<Item = &'a WindowEvent>,
        displacement: &mut impl FnMut(Vec2, Vec2) -> Option<Vec2>,
    ) -> Result<Vec2, DragCounterOverflow> {
        if self.failed {
            return Ok(Vec2::ZERO);
        }
        let Some(context) = context else {
            self.cancel(DragCancellation::MissingWindow)?;
            self.primary = None;
            self.cursor = None;
            self.geometry = None;
            return Ok(Vec2::ZERO);
        };
        let mut geometry_transition = self.prepare(context)? || geometry_changed;
        let mut movement = Vec2::ZERO;
        for event in events {
            if geometry_event(event, context.window) {
                geometry_transition = true;
                self.cursor = None;
            }
            movement += self.event(context, event, geometry_transition, displacement)?;
            if !movement.is_finite() {
                self.cancel(DragCancellation::InvalidRay)?;
                movement = Vec2::ZERO;
            }
            if geometry_transition {
                self.cancel(DragCancellation::Geometry)?;
                // Earlier samples may use a pre-change projection. Discard this
                // frame's total rather than applying mixed coordinate epochs.
                movement = Vec2::ZERO;
            }
        }
        Ok(movement)
    }

    fn event(
        &mut self,
        context: DragContext,
        event: &WindowEvent,
        geometry_transition: bool,
        displacement: &mut impl FnMut(Vec2, Vec2) -> Option<Vec2>,
    ) -> Result<Vec2, DragCounterOverflow> {
        match event {
            WindowEvent::WindowFocused(e) if e.window == context.window => {
                self.focused = e.focused;
                if !e.focused {
                    self.cancel(DragCancellation::Focus)?;
                    self.cursor = None;
                }
            }
            WindowEvent::CursorMoved(e) if e.window == context.window => {
                return self.move_cursor(context, e.position, geometry_transition, displacement);
            }
            WindowEvent::MouseButtonInput(e)
                if e.window == context.window && e.button == MouseButton::Middle =>
            {
                if e.state == ButtonState::Released {
                    self.cancel(DragCancellation::Release)?;
                } else if self.focused
                    && context.focused
                    && context.pointer_owned
                    && !geometry_transition
                {
                    self.press(context)?;
                }
            }
            WindowEvent::CursorLeft(e) if e.window == context.window => {
                self.cancel(DragCancellation::CursorDeparture)?;
                self.cursor = None;
            }
            WindowEvent::WindowDestroyed(e) if e.window == context.window => {
                self.cancel(DragCancellation::MissingWindow)?;
                self.cursor = None;
                self.focused = false;
            }
            _ => {}
        }
        Ok(Vec2::ZERO)
    }

    fn press(&mut self, context: DragContext) -> Result<(), DragCounterOverflow> {
        if let Some(cursor) = self.cursor.filter(|p| inside(context, *p)) {
            // A raw fresh press can recover even when the engine's global held
            // bit stayed set after a release outside this window.
            self.evidence.accepted_presses = self
                .evidence
                .accepted_presses
                .checked_add(1)
                .ok_or(DragCounterOverflow)?;
            self.evidence.active = true;
            self.evidence.window = Some(context.window);
            self.anchor = Some(cursor);
        }
        Ok(())
    }

    fn move_cursor(
        &mut self,
        context: DragContext,
        position: Vec2,
        geometry_transition: bool,
        displacement: &mut impl FnMut(Vec2, Vec2) -> Option<Vec2>,
    ) -> Result<Vec2, DragCounterOverflow> {
        self.cursor = Some(position);
        if !inside(context, position) {
            self.cancel(DragCancellation::CursorDeparture)?;
        } else if self.evidence.active && !geometry_transition {
            if let Some(previous) = self.anchor {
                if let Some(delta) = displacement(previous, position).filter(|v| v.is_finite()) {
                    self.anchor = Some(position);
                    return Ok(delta);
                }
                self.cancel(DragCancellation::InvalidRay)?;
            }
        }
        Ok(Vec2::ZERO)
    }
}

fn inside(context: DragContext, position: Vec2) -> bool {
    context
        .viewport
        .and_then(|v| v.viewport_from_window(position))
        .is_some()
}

pub(crate) fn geometry_event(event: &WindowEvent, window: Entity) -> bool {
    match event {
        WindowEvent::WindowResized(e) => e.window == window,
        WindowEvent::WindowScaleFactorChanged(e) => e.window == window,
        WindowEvent::WindowBackendScaleFactorChanged(e) => e.window == window,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
