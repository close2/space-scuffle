//! The single table of keys the game client reads while the world owns the keyboard.
//!
//! # Responsibility
//! Names every camera and battle key in one place, so a new binding is checked against all the
//! others. [`crate::camera`] and [`crate::battle_input`] read these constants and spell no key of
//! their own.
//!
//! # Invariants
//! - No key appears in two entries of [`ACTION_KEYS`]; a unit test enforces this. WASD pans the
//!   camera, which is why Hold and Stop do not use the `StarCraft` II `H` and `S` pair verbatim:
//!   `S` belongs to panning, so Stop uses [`STOP_KEY`].
//! - [`COMMAND_CARD_KEYS`] belong to card positions, never to a unit, so what a key does depends
//!   only on what the card shows there.
//! - Modifiers change what a key does but are not actions of their own.
//!
//! The map editor has its own tool shortcuts and is deliberately outside this table.

use bevy::prelude::*;

/// Pans the camera toward the top of the screen.
pub const PAN_FORWARD_KEYS: [KeyCode; 2] = [KeyCode::KeyW, KeyCode::ArrowUp];

/// Pans the camera toward the bottom of the screen.
pub const PAN_BACK_KEYS: [KeyCode; 2] = [KeyCode::KeyS, KeyCode::ArrowDown];

/// Pans the camera toward the left of the screen.
pub const PAN_LEFT_KEYS: [KeyCode; 2] = [KeyCode::KeyA, KeyCode::ArrowLeft];

/// Pans the camera toward the right of the screen.
pub const PAN_RIGHT_KEYS: [KeyCode; 2] = [KeyCode::KeyD, KeyCode::ArrowRight];

/// Recalls a camera bookmark; with [`CONTROL_MODIFIERS`] held, saves it.
pub const CAMERA_BOOKMARK_KEYS: [KeyCode; 4] = [KeyCode::F1, KeyCode::F2, KeyCode::F3, KeyCode::F4];

/// Orders the selected units to hold position.
pub const HOLD_POSITION_KEY: KeyCode = KeyCode::KeyH;

/// Orders the selected units to stop.
pub const STOP_KEY: KeyCode = KeyCode::KeyX;

/// Leaves a command mode or goes back one card page.
pub const CANCEL_KEY: KeyCode = KeyCode::Escape;

/// Number of command card positions.
pub const COMMAND_CARD_SLOTS: usize = 15;

/// Key of each command card position, in reading order. A position's key never depends on the unit.
pub const COMMAND_CARD_KEYS: [KeyCode; COMMAND_CARD_SLOTS] = [
    KeyCode::KeyQ,
    KeyCode::KeyE,
    KeyCode::KeyR,
    KeyCode::KeyT,
    KeyCode::KeyY,
    KeyCode::KeyF,
    KeyCode::KeyG,
    KeyCode::KeyJ,
    KeyCode::KeyK,
    KeyCode::KeyL,
    KeyCode::KeyZ,
    KeyCode::KeyC,
    KeyCode::KeyV,
    KeyCode::KeyB,
    KeyCode::KeyN,
];

/// The letter of each [`COMMAND_CARD_KEYS`] entry, as a button shows it.
pub const COMMAND_CARD_KEY_LABELS: [&str; COMMAND_CARD_SLOTS] = [
    "Q", "E", "R", "T", "Y", "F", "G", "J", "K", "L", "Z", "C", "V", "B", "N",
];

/// The letter of [`STOP_KEY`], as a button shows it.
pub const STOP_KEY_LABEL: &str = "X";

/// The letter of [`HOLD_POSITION_KEY`], as a button shows it.
pub const HOLD_POSITION_KEY_LABEL: &str = "H";

/// Number of control groups, one per entry of [`CONTROL_GROUP_KEYS`].
pub const CONTROL_GROUP_COUNT: usize = 10;

/// Recalls control group `index`; with [`CONTROL_MODIFIERS`] held, assigns it.
///
/// The order matches the top row of a keyboard, so group 9 is the `0` key.
pub const CONTROL_GROUP_KEYS: [KeyCode; CONTROL_GROUP_COUNT] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
    KeyCode::Digit0,
];

/// Turns a recall into a save or an assignment, and suspends keyboard panning while held.
pub const CONTROL_MODIFIERS: [KeyCode; 2] = [KeyCode::ControlLeft, KeyCode::ControlRight];

/// Adds to the selection on a click or drag, and queues an order on a right click.
pub const SHIFT_MODIFIERS: [KeyCode; 2] = [KeyCode::ShiftLeft, KeyCode::ShiftRight];

/// Every action key, grouped by the action that owns it.
pub const ACTION_KEYS: [&[KeyCode]; 9] = [
    &PAN_FORWARD_KEYS,
    &PAN_BACK_KEYS,
    &PAN_LEFT_KEYS,
    &PAN_RIGHT_KEYS,
    &CAMERA_BOOKMARK_KEYS,
    &[HOLD_POSITION_KEY, STOP_KEY],
    &[CANCEL_KEY],
    &COMMAND_CARD_KEYS,
    &CONTROL_GROUP_KEYS,
];

/// One-line summary of the battle controls, shown in the inspector.
pub const BATTLE_CONTROLS_HELP: &str = "Left click selects, drag boxes, Shift adds. Right click \
moves, attacks, gathers, builds or boards by its target, Shift queues. H holds, X stops. Escape \
cancels a command mode. Ctrl+0-9 assigns a group, 0-9 recalls it.";

/// Returns whether any key of `keys` is held.
#[must_use]
pub fn any_pressed(keyboard: &ButtonInput<KeyCode>, keys: &[KeyCode]) -> bool {
    keys.iter().any(|key| keyboard.pressed(*key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_key_is_bound_to_two_actions() {
        let keys: Vec<KeyCode> = ACTION_KEYS
            .iter()
            .flat_map(|group| group.iter())
            .copied()
            .collect();
        for (index, key) in keys.iter().enumerate() {
            assert!(
                !keys[index + 1..].contains(key),
                "{key:?} is bound to two actions"
            );
        }
    }

    #[test]
    fn command_card_keys_avoid_panning_hold_and_stop() {
        let reserved: Vec<KeyCode> = [
            PAN_FORWARD_KEYS.as_slice(),
            &PAN_BACK_KEYS,
            &PAN_LEFT_KEYS,
            &PAN_RIGHT_KEYS,
            &[HOLD_POSITION_KEY, STOP_KEY, CANCEL_KEY],
        ]
        .concat();
        for key in &COMMAND_CARD_KEYS {
            assert!(
                !reserved.contains(key),
                "{key:?} is a card key and reserved"
            );
        }
    }

    #[test]
    fn every_key_label_names_its_key() {
        let keys = COMMAND_CARD_KEYS
            .iter()
            .zip(COMMAND_CARD_KEY_LABELS)
            .chain([
                (&STOP_KEY, STOP_KEY_LABEL),
                (&HOLD_POSITION_KEY, HOLD_POSITION_KEY_LABEL),
            ]);
        for (key, label) in keys {
            assert_eq!(format!("{key:?}"), format!("Key{label}"));
        }
    }

    #[test]
    fn modifiers_are_not_action_keys() {
        for modifier in CONTROL_MODIFIERS.iter().chain(&SHIFT_MODIFIERS) {
            assert!(
                !ACTION_KEYS.iter().any(|group| group.contains(modifier)),
                "{modifier:?} must stay a modifier"
            );
        }
    }

    #[test]
    fn any_pressed_reads_every_listed_key() {
        let mut keyboard = ButtonInput::<KeyCode>::default();
        assert!(!any_pressed(&keyboard, &SHIFT_MODIFIERS));
        keyboard.press(KeyCode::ShiftRight);
        assert!(any_pressed(&keyboard, &SHIFT_MODIFIERS));
        assert!(!any_pressed(&keyboard, &CONTROL_MODIFIERS));
    }
}
