//! HTPC button mapping for the 8BitDo Ultimate (2.4GHz dongle via xpad).
//!
//! xpad reports a standard Xbox layout:
//!
//! | Physical | Evdev code          | Code # |
//! |----------|---------------------|--------|
//! | A        | BTN_SOUTH           | 304    |  (bottom face button = confirm on Xbox/8BitDo)
//! | B        | BTN_EAST            | 305    |  (right face button = cancel)
//! | X        | BTN_WEST            | 307    |  (left face button)
//! | Y        | BTN_NORTH           | 308    |  (top face button)
//! | LB       | BTN_TL              | 310    |
//! | RB       | BTN_TR              | 311    |
//! | LT       | BTN_TL2 (or ABS_Z)  | 312    |  (xpad uses BTN_TL2 for digital press)
//! | RT       | BTN_TR2 (or ABS_RZ) | 313    |
//! | L3       | BTN_THUMBL          | 317    |
//! | R3       | BTN_THUMBR          | 318    |
//! | Start    | BTN_START           | 315    |
//! | Back     | BTN_SELECT          | 314    |
//! | Home     | BTN_MODE            | 316    |  (the "guide" button)
//! | D-pad    | ABS_HAT0X/Y         | -      |  (analog hat, NOT discrete keys)
//!
//! NOTE: D-pad is reported via ABS_HAT0X/ABS_HAT0Y (axis values -1/0/+1),
//! not via BTN_DPAD_* keys. We translate those events into Pressed events
//! at the edge and Released when returning to 0.

use crate::button::{Button, ButtonEvent, ButtonState};
use evdev::{AbsoluteAxisType, EventType, InputEvent, Key};
use std::sync::atomic::{AtomicI32, Ordering};

/// Last seen ABS_HAT0X/Y values, so we can synthesize Released events when
/// the hat returns to 0. Atomic so this stays a pure function from the
/// trait's perspective.
static HAT_X: AtomicI32 = AtomicI32::new(0);
static HAT_Y: AtomicI32 = AtomicI32::new(0);

pub fn map_event(ev: &InputEvent) -> Option<ButtonEvent> {
    match ev.event_type() {
        EventType::KEY => map_key(ev),
        EventType::ABSOLUTE => map_hat(ev),
        _ => None,
    }
}

fn map_key(ev: &InputEvent) -> Option<ButtonEvent> {
    let state = match ev.value() {
        0 => ButtonState::Released,
        1 => ButtonState::Pressed,
        _ => return None,
    };
    let button = match Key(ev.code()) {
        Key::BTN_SOUTH => Button::A,
        Key::BTN_EAST => Button::B,
        Key::BTN_WEST => Button::X,
        Key::BTN_NORTH => Button::Y,
        Key::BTN_TL => Button::L1,
        Key::BTN_TR => Button::R1,
        Key::BTN_TL2 => Button::L2,
        Key::BTN_TR2 => Button::R2,
        Key::BTN_THUMBL => Button::L3,
        Key::BTN_THUMBR => Button::R3,
        Key::BTN_START => Button::Start,
        Key::BTN_SELECT => Button::Select,
        Key::BTN_MODE => Button::Home,
        _ => return None,
    };
    Some(ButtonEvent { button, state })
}

fn map_hat(ev: &InputEvent) -> Option<ButtonEvent> {
    let code = AbsoluteAxisType(ev.code());
    let new = ev.value();
    match code {
        AbsoluteAxisType::ABS_HAT0X => {
            let prev = HAT_X.swap(new, Ordering::Relaxed);
            edge(prev, new, Button::DpadLeft, Button::DpadRight)
        }
        AbsoluteAxisType::ABS_HAT0Y => {
            let prev = HAT_Y.swap(new, Ordering::Relaxed);
            edge(prev, new, Button::DpadUp, Button::DpadDown)
        }
        _ => None,
    }
}

/// Synthesize a press on transition to ±1, a release on transition back to 0.
/// Movement from -1 to +1 (or back) without a stop releases the previous and
/// presses the new — but we report only the new press, since most evdev
/// streams produce two events (–1 → 0, 0 → +1).
fn edge(prev: i32, new: i32, neg: Button, pos: Button) -> Option<ButtonEvent> {
    if new == 0 {
        // Release of whichever direction was active.
        let button = if prev < 0 { neg } else { pos };
        Some(ButtonEvent {
            button,
            state: ButtonState::Released,
        })
    } else if new < 0 {
        Some(ButtonEvent {
            button: neg,
            state: ButtonState::Pressed,
        })
    } else {
        Some(ButtonEvent {
            button: pos,
            state: ButtonState::Pressed,
        })
    }
}
