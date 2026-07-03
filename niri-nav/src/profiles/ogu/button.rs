//! OGU-specific evdev → logical-button mapping.
//!
//! OGU gpio-keys device mapping (confirmed via DT + evtest):
//!
//! | Physical | DT label          | Evdev code           | Code # |
//! |----------|-------------------|----------------------|--------|
//! | A        | a-button          | BTN_EAST             | 305    |
//! | B        | b-button          | BTN_SOUTH            | 304    |
//! | X        | x-button          | BTN_NORTH            | 307    |
//! | Y        | y-button          | BTN_WEST             | 308    |
//! | D-Up     | dpad-up-button    | BTN_DPAD_UP          | 544    |
//! | D-Down   | dpad-down-button  | BTN_DPAD_DOWN        | 545    |
//! | D-Left   | dpad-left-button  | BTN_DPAD_LEFT        | 546    |
//! | D-Right  | dpad-right-button | BTN_DPAD_RIGHT       | 547    |
//! | L1       | top-left-button   | BTN_TL               | 310    |
//! | R1       | top-right-button  | BTN_TR               | 311    |
//! | L2       | top-left2-button  | BTN_TL2              | 312    |
//! | R2       | top-right2-button | BTN_TR2              | 313    |
//! | F1/Start | f1-button         | BTN_TRIGGER_HAPPY1   | 704    |
//! | F2/Sel   | f2-button         | BTN_TRIGGER_HAPPY2   | 705    |
//! | F3       | f3-button         | BTN_TRIGGER_HAPPY3   | 706    |
//! | F4       | f4-button         | BTN_TRIGGER_HAPPY4   | 707    |
//! | F5       | f5-button         | BTN_TRIGGER_HAPPY5   | 708    |
//! | F6       | f6-button         | BTN_TRIGGER_HAPPY6   | 709    |
//! | Vol+     | volume-up-button  | KEY_VOLUMEUP         | 115    |
//! | Vol-     | volume-down-button| KEY_VOLUMEDOWN       | 114    |
//!
//! NOTE: D-pad is EV_KEY (discrete buttons), NOT EV_ABS (analog hat).
//! NOTE: A/B are swapped vs Xbox convention (OGU A=BTN_EAST, B=BTN_SOUTH).

use crate::button::{Button, ButtonEvent, ButtonState};
use evdev::InputEvent;

pub fn map_event(ev: &InputEvent) -> Option<ButtonEvent> {
    use evdev::Key;

    if ev.event_type() != evdev::EventType::KEY {
        return None;
    }

    let state = match ev.value() {
        0 => ButtonState::Released,
        1 => ButtonState::Pressed,
        2 => return None, // kernel autorepeat; we do our own
        _ => return None,
    };

    let button = match Key(ev.code()) {
        // Face buttons (OGU silk screen — A/B swapped vs Xbox).
        Key::BTN_EAST => Button::A,
        Key::BTN_SOUTH => Button::B,
        Key::BTN_NORTH => Button::X,
        Key::BTN_WEST => Button::Y,

        // D-pad as discrete buttons.
        Key::BTN_DPAD_UP => Button::DpadUp,
        Key::BTN_DPAD_DOWN => Button::DpadDown,
        Key::BTN_DPAD_LEFT => Button::DpadLeft,
        Key::BTN_DPAD_RIGHT => Button::DpadRight,

        Key::BTN_TL => Button::L1,
        Key::BTN_TR => Button::R1,
        Key::BTN_TL2 => Button::L2,
        Key::BTN_TR2 => Button::R2,

        // OGU has no dedicated Start/Select — F1/F2 stand in.
        Key::BTN_TRIGGER_HAPPY1 => Button::Start,
        Key::BTN_TRIGGER_HAPPY2 => Button::Select,
        Key::BTN_TRIGGER_HAPPY3 => Button::F3,
        Key::BTN_TRIGGER_HAPPY4 => Button::F4,
        Key::BTN_TRIGGER_HAPPY5 => Button::F5,
        Key::BTN_TRIGGER_HAPPY6 => Button::F6,

        Key::KEY_VOLUMEUP => Button::VolumeUp,
        Key::KEY_VOLUMEDOWN => Button::VolumeDown,

        _ => return None,
    };

    Some(ButtonEvent { button, state })
}
