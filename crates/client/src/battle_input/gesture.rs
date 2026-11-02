//! The left-button gesture that tells a click from a box drag.
//!
//! # Invariants
//! - A gesture starts only from an accepted press, so a button held since before the press never
//!   arms one.
//! - All positions are logical window pixels, already clamped to the world viewport by the caller.
//! - Once a drag passes [`DRAG_THRESHOLD_PIXELS`] it stays a drag, even if it returns to its start.

use bevy::prelude::*;

/// Cursor travel in logical pixels at which a press becomes a box drag rather than a click.
pub const DRAG_THRESHOLD_PIXELS: f32 = 6.0;

/// What a finished left-button gesture meant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GestureOutcome {
    /// Released near its start, at this window position.
    Click(Vec2),
    /// Dragged over this rectangle, in window coordinates.
    Box(Rect),
}

/// Left-button gesture in progress, if any.
#[derive(Resource, Debug, Default)]
pub struct SelectionGesture {
    anchor: Option<Vec2>,
    cursor: Vec2,
    dragged: bool,
}

impl SelectionGesture {
    /// Starts a gesture at `cursor`, replacing any unfinished one.
    pub fn begin(&mut self, cursor: Vec2) {
        self.anchor = Some(cursor);
        self.cursor = cursor;
        self.dragged = false;
    }

    /// Records the cursor of an active gesture. Does nothing without one.
    pub fn track(&mut self, cursor: Vec2) {
        let Some(anchor) = self.anchor else {
            return;
        };
        self.cursor = cursor;
        self.dragged |= anchor.distance(cursor) >= DRAG_THRESHOLD_PIXELS;
    }

    /// Abandons an active gesture, as when the window loses focus.
    pub fn cancel(&mut self) {
        self.anchor = None;
        self.dragged = false;
    }

    /// Ends the gesture and reports what it meant, or `None` if none was active.
    pub fn finish(&mut self) -> Option<GestureOutcome> {
        let outcome = self
            .drag_rect()
            .map(GestureOutcome::Box)
            .or(self.anchor.map(|_| GestureOutcome::Click(self.cursor)));
        self.cancel();
        outcome
    }

    /// Rectangle of the drag in progress, in window coordinates.
    #[must_use]
    pub fn drag_rect(&self) -> Option<Rect> {
        self.anchor
            .filter(|_| self.dragged)
            .map(|anchor| Rect::from_corners(anchor, self.cursor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_travel_is_a_click_at_the_release_position() {
        let mut gesture = SelectionGesture::default();
        gesture.begin(Vec2::new(10.0, 10.0));
        gesture.track(Vec2::new(13.0, 10.0));
        assert_eq!(gesture.drag_rect(), None);
        assert_eq!(
            gesture.finish(),
            Some(GestureOutcome::Click(Vec2::new(13.0, 10.0)))
        );
        assert_eq!(gesture.finish(), None, "a gesture finishes once");
    }

    #[test]
    fn long_travel_is_a_box_in_any_direction() {
        let mut gesture = SelectionGesture::default();
        gesture.begin(Vec2::new(50.0, 40.0));
        gesture.track(Vec2::new(20.0, 90.0));
        let expected = Rect::new(20.0, 40.0, 50.0, 90.0);
        assert_eq!(gesture.drag_rect(), Some(expected));
        assert_eq!(gesture.finish(), Some(GestureOutcome::Box(expected)));
    }

    #[test]
    fn a_drag_that_returns_to_its_start_stays_a_drag() {
        let mut gesture = SelectionGesture::default();
        gesture.begin(Vec2::ZERO);
        gesture.track(Vec2::new(30.0, 0.0));
        gesture.track(Vec2::new(1.0, 1.0));
        assert!(matches!(gesture.finish(), Some(GestureOutcome::Box(_))));
    }

    #[test]
    fn cancel_and_tracking_without_a_press_produce_nothing() {
        let mut gesture = SelectionGesture::default();
        gesture.track(Vec2::new(30.0, 0.0));
        assert_eq!(gesture.finish(), None);
        gesture.begin(Vec2::ZERO);
        gesture.track(Vec2::new(30.0, 0.0));
        gesture.cancel();
        assert_eq!(gesture.drag_rect(), None);
        assert_eq!(gesture.finish(), None);
    }
}
