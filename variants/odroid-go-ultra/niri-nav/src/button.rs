use evdev::InputEvent;

/// Logical buttons mapped from raw evdev events.
///
/// Named after their physical silk-screen labels on the OGU, NOT the
/// evdev code names (which follow Xbox convention and are swapped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    /// Physical "A" button (right-side, confirm) = BTN_EAST (305)
    A,
    /// Physical "B" button (bottom, cancel) = BTN_SOUTH (304)
    B,
    /// Physical "X" button (top) = BTN_NORTH (307)
    X,
    /// Physical "Y" button (left-side) = BTN_WEST (308)
    Y,
    /// Left shoulder L1 = BTN_TL (310)
    L1,
    /// Right shoulder R1 = BTN_TR (311)
    R1,
    /// Left trigger L2 = BTN_TL2 (312)
    L2,
    /// Right trigger R2 = BTN_TR2 (313)
    R2,
    /// F1 button = BTN_TRIGGER_HAPPY1 (704) - Home (toggles LAUNCHER/WINDOW_NAV)
    Start,
    /// F2 button = BTN_TRIGGER_HAPPY2 (705) - used as Select
    Select,
    /// F3 = BTN_TRIGGER_HAPPY3 (706)
    F3,
    /// F4 = BTN_TRIGGER_HAPPY4 (707)
    F4,
    /// F5 = BTN_TRIGGER_HAPPY5 (708)
    F5,
    /// F6 = BTN_TRIGGER_HAPPY6 (709)
    F6,
    VolumeUp,
    VolumeDown,
}

/// Whether a button was pressed or released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Released,
}

/// A parsed button event.
#[derive(Debug, Clone, Copy)]
pub struct ButtonEvent {
    pub button: Button,
    pub state: ButtonState,
}

/// Try to parse a raw evdev InputEvent into a ButtonEvent.
///
/// OGU gpio-keys device mapping (confirmed via DT + evtest):
///
/// | Physical | DT label          | Evdev code           | Code # |
/// |----------|-------------------|----------------------|--------|
/// | A        | a-button          | BTN_EAST             | 305    |
/// | B        | b-button          | BTN_SOUTH            | 304    |
/// | X        | x-button          | BTN_NORTH            | 307    |
/// | Y        | y-button          | BTN_WEST             | 308    |
/// | D-Up     | dpad-up-button    | BTN_DPAD_UP          | 544    |
/// | D-Down   | dpad-down-button  | BTN_DPAD_DOWN        | 545    |
/// | D-Left   | dpad-left-button  | BTN_DPAD_LEFT        | 546    |
/// | D-Right  | dpad-right-button | BTN_DPAD_RIGHT       | 547    |
/// | L1       | top-left-button   | BTN_TL               | 310    |
/// | R1       | top-right-button  | BTN_TR               | 311    |
/// | L2       | top-left2-button  | BTN_TL2              | 312    |
/// | R2       | top-right2-button | BTN_TR2              | 313    |
/// | F1/Start | f1-button         | BTN_TRIGGER_HAPPY1   | 704    |
/// | F2/Sel   | f2-button         | BTN_TRIGGER_HAPPY2   | 705    |
/// | F3       | f3-button         | BTN_TRIGGER_HAPPY3   | 706    |
/// | F4       | f4-button         | BTN_TRIGGER_HAPPY4   | 707    |
/// | F5       | f5-button         | BTN_TRIGGER_HAPPY5   | 708    |
/// | F6       | f6-button         | BTN_TRIGGER_HAPPY6   | 709    |
/// | Vol+     | volume-up-button  | KEY_VOLUMEUP         | 115    |
/// | Vol-     | volume-down-button| KEY_VOLUMEDOWN       | 114    |
///
/// NOTE: D-pad is EV_KEY (discrete buttons), NOT EV_ABS (analog hat).
/// NOTE: A/B are swapped vs Xbox convention (OGU A=BTN_EAST, B=BTN_SOUTH).
pub fn from_event(ev: &InputEvent) -> Option<ButtonEvent> {
    use evdev::Key;

    // Only handle EV_KEY events (the gpio-keys device only emits these)
    if ev.event_type() != evdev::EventType::KEY {
        return None;
    }

    let state = match ev.value() {
        0 => ButtonState::Released,
        1 => ButtonState::Pressed,
        2 => return None, // autorepeat from kernel, we do our own
        _ => return None,
    };

    let button = match Key(ev.code()) {
        // Face buttons (OGU silk screen -> evdev mapping is swapped)
        Key::BTN_EAST => Button::A,   // Physical A = evdev EAST
        Key::BTN_SOUTH => Button::B,  // Physical B = evdev SOUTH
        Key::BTN_NORTH => Button::X,  // Physical X = evdev NORTH
        Key::BTN_WEST => Button::Y,   // Physical Y = evdev WEST

        // D-pad (discrete buttons, not analog hat)
        Key::BTN_DPAD_UP => Button::DpadUp,
        Key::BTN_DPAD_DOWN => Button::DpadDown,
        Key::BTN_DPAD_LEFT => Button::DpadLeft,
        Key::BTN_DPAD_RIGHT => Button::DpadRight,

        // Shoulders / triggers
        Key::BTN_TL => Button::L1,
        Key::BTN_TR => Button::R1,
        Key::BTN_TL2 => Button::L2,
        Key::BTN_TR2 => Button::R2,

        // F-buttons (no dedicated Start/Select on OGU hardware)
        Key::BTN_TRIGGER_HAPPY1 => Button::Start,  // F1
        Key::BTN_TRIGGER_HAPPY2 => Button::Select,  // F2
        Key::BTN_TRIGGER_HAPPY3 => Button::F3,
        Key::BTN_TRIGGER_HAPPY4 => Button::F4,
        Key::BTN_TRIGGER_HAPPY5 => Button::F5,
        Key::BTN_TRIGGER_HAPPY6 => Button::F6,

        // Volume
        Key::KEY_VOLUMEUP => Button::VolumeUp,
        Key::KEY_VOLUMEDOWN => Button::VolumeDown,

        _ => return None,
    };

    Some(ButtonEvent { button, state })
}
